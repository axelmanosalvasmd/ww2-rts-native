// ATCUnitRenderer's model path: the reference game's own unit models, as the pinned browser client draws them.
//   client/main.js       makeUnit (owner ring, selection ring), the frame loop (root at the ground under the eased
//                        position, rotation.y = -rot, turret.rotation.y = -(aim - rot), v.body.scale.y while built,
//                        planes AIR_ALT up), the snapshot's alive count, camouflage opacity 0.45 (flags & 256)
//   client/unit-models.js buildModel (which model, UK stand-ins), animate() for a squad (Client/TCInfantryRig.h
//                        FTCInfantrySquadAnimation: LOD 110 m with 4 m hysteresis, posture goal and blend, aim blend,
//                        dig work, moveSquad, the men's pose node and muzzle offset), createBodies (UTCSoldierCrowd)
//   client/model-motion.js, aircraft.js, unit-models.js stride: the vehicles' presentation motion (Client/TCUnitMotion.h)
// Assets: Content/Models (unreal/Tools/models/import-models.py; ModelCatalog.json lists models, parts and rest
// places), Content/Models/Infantry (unreal/Tools/models/infantry; Source/kits.json lists each figure's kit) and
// Content/Models/Motion (unreal/Tools/motion: the far figures and the faded and far crowd materials).
// Axes as everywhere in this client: reference (x, y, z) metres -> Unreal (x, z, y) * 100; three's rotation.y = a is
// an Unreal yaw of -a.
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "World/TCTerrain.h"
#include "Client/TCLighting.h"
#include "EngineUtils.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/MaterialInterface.h"
#include "Engine/StaticMesh.h"
#include "Engine/Texture2D.h"
#include "MeshDescription.h"
#include "StaticMeshAttributes.h"
#include "Misc/App.h"
#include "Misc/CommandLine.h"
#include "HAL/PlatformTime.h"
#include "Misc/Parse.h"

DEFINE_LOG_CATEGORY_STATIC(LogTCUnitModels, Log, All);

namespace TCUnitModelsImpl
{
	// The adoption batches (ATCUnitRenderer::EModelBatch order): the pilot first, then the rest of the catalog.
	struct FBatchTypes { const TCHAR* Name; const TCHAR* Types; };
	const FBatchTypes Batches[ATCUnitRenderer::BatchCount] = {
		{ TEXT("pilot"), TEXT("medium,at,rifle,barracks") },
		{ TEXT("vehicles"), TEXT("tank,rocket,flaktrack,armoredcar,halftrack,lorry,train,tankdestroyer,tiger,churchill,truck,kaiju") },
		{ TEXT("infantry"), TEXT("mg,flak,mortar,sniper,medic,howitzer,flamer,ranger,conscript,commando,engineer") },
		{ TEXT("airnaval"), TEXT("fighter,attacker,bomber,lcvp,gunboat,destroyer") },
		{ TEXT("structures"), TEXT("bunker,hq,depot,motorpool,airfield,shipyard,flakpos,worldbase,armory,pillbox,tower,wall,gate,supplycache") },
	};
	// The batches a client draws with models unless -TCModels= says otherwise.
	constexpr uint32 DefaultBatches = (1u << ATCUnitRenderer::BatchPilot) | (1u << ATCUnitRenderer::BatchVehicles) | (1u << ATCUnitRenderer::BatchInfantry) | (1u << ATCUnitRenderer::BatchAirNaval) | (1u << ATCUnitRenderer::BatchStructures);

	// shared/sim.js UNITS[type].w.moveFire !== undefined at the pin (unit-models.js v.squad.moveFire)
	bool MovesFiring(const FString& Type)
	{
		static const TCHAR* List[] = { TEXT("rifle"), TEXT("tank"), TEXT("fighter"), TEXT("attacker"), TEXT("flaktrack"), TEXT("armoredcar"), TEXT("medium"), TEXT("halftrack"), TEXT("lcvp"), TEXT("gunboat"),
			TEXT("tankdestroyer"), TEXT("flamer"), TEXT("bomber"), TEXT("ranger"), TEXT("tiger"), TEXT("conscript"), TEXT("churchill"), TEXT("commando"), TEXT("kaiju"), TEXT("engineer"), TEXT("truck") };
		for (const TCHAR* T : List) if (Type == T) return true;
		return false;
	}
	// shared/sim.js UNITS[type].camo at the pin: their models fade to 0.45 while flags & 256 (main.js)
	bool Camouflages(const FString& Type) { return Type == TEXT("sniper") || Type == TEXT("commando"); }
	// unit-models.js: guns that traverse (guns.js { pivot }) are the whole of v.turret
	bool GunTraverses(const FString& Type) { return Type == TEXT("at") || Type == TEXT("flak"); }
	// main.js v.traverse: how far a turret may turn either way of the hull (0: all round). The kaiju's head
	// (unit-models.js, 1 rad) and the casemate guns of the StuG and the SU-85 (armor-medium.js limit 0.21); by the
	// model drawn, so a UK tank destroyer (the American M10 in British paint) turns all round as in the reference.
	double TraverseLimit(const FTCModelEntry& Entry)
	{
		if (Entry.Type == TEXT("kaiju")) return 1.0;
		if (Entry.Type == TEXT("tankdestroyer") && (Entry.Faction == 1 || Entry.Faction == 2)) return 0.21;
		return 0.0;
	}

	// "man_12_near" -> 12, near; false for anything else
	bool ParseMan(const FString& Name, int32& OutIndex, bool& bOutNear)
	{
		if (!Name.StartsWith(TEXT("man_"))) return false;
		const bool bNear = Name.EndsWith(TEXT("_near")), bFar = Name.EndsWith(TEXT("_far"));
		if (!bNear && !bFar) return false;
		const FString Mid = Name.Mid(4, Name.Len() - 4 - (bNear ? 5 : 4));
		if (Mid.IsEmpty() || !Mid.IsNumeric()) return false;
		OutIndex = FCString::Atoi(*Mid);
		bOutNear = bNear;
		return true;
	}

	// The two levels of detail the reference builds for soldiers and crew guns: "_near" parts while close, "_far" ones
	// beyond LOD; 0 for a part drawn at every distance.
	int32 LodOf(const FName& Name)
	{
		const FString S = Name.ToString();
		return S.EndsWith(TEXT("_near")) ? 1 : S.EndsWith(TEXT("_far")) ? 2 : 0;
	}

	// the hidden shadow twin of a unit part drawn on the units' lighting channel (ATCUnitRenderer::PartComponent);
	// the renderer's header is not this file's, so the pairs live here
	TMap<TWeakObjectPtr<UInstancedStaticMeshComponent>, TWeakObjectPtr<UInstancedStaticMeshComponent>>& ShadowTwins()
	{
		static TMap<TWeakObjectPtr<UInstancedStaticMeshComponent>, TWeakObjectPtr<UInstancedStaticMeshComponent>> Pairs;
		return Pairs;
	}

	// unit-models.js vehicleDamageAnchor's v.damageAnchorLocal, once per model: the bounds of every mesh under the hull
	// body except the turret, the mounts and what hangs on them (and the owner and selection rings, which are not parts
	// here), then (min x + 15 % of the length, top + 0.18 m, mid width). Three's x (forward) is Unreal X, y (up) Z.
	FVector DamageAnchorLocal(FTCUnitMotion& Motion, const FTCModelEntry& Entry)
	{
		if (const FVector* Found = Motion.LocalDamageAnchors.Find(Entry.Key)) return *Found;
		static const FTCPartMotion Still;
		const TMap<FName, FQuat> NoTurns;
		TArray<bool> Skip;
		Skip.SetNum(Entry.Parts.Num());
		FBox Box(ForceInit);
		for (int32 I = 0; I < Entry.Parts.Num(); I++)
		{
			const FTCModelPart& Part = Entry.Parts[I];
			const FString Name = Part.Name.ToString();
			const int32 Parent = Part.Parent.IsNone() ? INDEX_NONE : Entry.FindPart(Part.Parent);
			int32 Man; bool bNear;
			Skip[I] = Name == TEXT("turret") || Name.StartsWith(TEXT("mount_")) || (GunTraverses(Entry.Type) && Name.StartsWith(TEXT("gun_"))) || ParseMan(Name, Man, bNear)
				|| LodOf(Part.Name) == 2 || (Parent != INDEX_NONE && Skip[Parent]);
			if (Skip[I]) continue;
			const UStaticMesh* Mesh = Cast<UStaticMesh>(Part.Mesh.TryLoad());
			if (!Mesh) continue;
			const FBox Local = Mesh->GetBoundingBox();
			if (!Local.IsValid) continue;
			Box += Local.TransformBy(FTCUnitMotion::PartTransform(Entry, I, FTransform::Identity, NoTurns, Still));
		}
		// three: bounds.isEmpty() ? (-1, 1.8, 0)
		const FVector Out = Box.IsValid
			? FVector(Box.Min.X + (Box.Max.X - Box.Min.X) * 0.15, (Box.Min.Y + Box.Max.Y) / 2, Box.Max.Z + 18.0)
			: FVector(-100.0, 0.0, 180.0);
		Motion.LocalDamageAnchors.Add(Entry.Key, Out);
		return Out;
	}
}

const TCHAR* ATCUnitRenderer::BatchName(int32 Batch)
{
	return Batch >= 0 && Batch < BatchCount ? TCUnitModelsImpl::Batches[Batch].Name : TEXT("-");
}

int32 ATCUnitRenderer::BatchOf(const FString& Type)
{
	for (int32 B = 0; B < BatchCount; B++)
	{
		TArray<FString> Types;
		FString(TCUnitModelsImpl::Batches[B].Types).ParseIntoArray(Types, TEXT(","));
		if (Types.Contains(Type)) return B;
	}
	return INDEX_NONE;
}

void ATCUnitRenderer::EnsureModels()
{
	if (bModelsTried) return;
	bModelsTried = true;
	if (!bBatchesSet) EnabledBatches = TCUnitModelsImpl::DefaultBatches;
	FString Arg;
	if (!bBatchesSet && FParse::Value(FCommandLine::Get(), TEXT("-TCModels="), Arg))
	{
		uint32 Mask = 0;
		TArray<FString> Names;
		Arg.ParseIntoArray(Names, TEXT(","));
		for (const FString& N : Names)
		{
			if (N == TEXT("all")) Mask = (1u << BatchCount) - 1;
			else if (N == TEXT("none")) Mask = 0;
			else for (int32 B = 0; B < BatchCount; B++) if (N == BatchName(B)) Mask |= 1u << B;
		}
		EnabledBatches = Mask;
	}
	if (!Library.Load(LibraryError))
	{
		UE_LOG(LogTCUnitModels, Warning, TEXT("unit models: off, every unit is a placeholder (%s)"), *LibraryError);
		return;
	}
	// the soldiers: near figures from the kits (Content/Models/Infantry), far figures (Content/Models/Motion); men it
	// cannot draw fall back to the catalog's rest figures
	Crowd = NewObject<UTCSoldierCrowd>(this);
	Crowd->SetupAttachment(RootComponent);
	Crowd->RegisterComponent();
	FString CrowdError;
	if (!Crowd->Load(CrowdError)) UE_LOG(LogTCUnitModels, Warning, TEXT("unit models: soldier crowd incomplete (%s): the men it cannot draw take the catalog's rest figures"), *CrowdError);
	TArray<FString> On;
	for (int32 B = 0; B < BatchCount; B++) if (IsBatchEnabled(B)) On.Add(BatchName(B));
	UE_LOG(LogTCUnitModels, Log, TEXT("unit models: catalog %d models (reference %s), soldiers near %s, far %s; batches drawn with models: %s"),
		Library.Num(), *Library.ReferenceCommit(), Crowd->HasNear() ? TEXT("on") : TEXT("off"), Crowd->HasFar() ? TEXT("on") : TEXT("off"), On.IsEmpty() ? TEXT("none") : *FString::Join(On, TEXT(",")));
}

void ATCUnitRenderer::Reset()
{
	Shown.Reset();
	Squads.Reset();
	UnitMotion.Reset();
	KilledIds.Reset();
	if (Crowd)
	{
		// the bodies go with the match; one empty frame clears every crowd instance now
		Crowd->ClearCorpses();
		Crowd->BeginFrame();
		Crowd->EndFrame(0.f, FVector::ZeroVector, [](double, double) { return 0.0; }, LodMetres);
	}
}

FString ATCUnitRenderer::ModelStatusLine() const
{
	if (!Library.IsLoaded()) return FString::Printf(TEXT("models off (%s)"), *LibraryError);
	TArray<FString> On;
	for (int32 B = 0; B < BatchCount; B++) if (IsBatchEnabled(B)) On.Add(BatchName(B));
	return FString::Printf(TEXT("models [%s]: units %d model, %d placeholder (%d without a model); instances %d parts, %d near men, %d far men (%d faded), %d bodies, %d rings; %d in motion; %d components, %d figures"),
		On.IsEmpty() ? TEXT("none") : *FString::Join(On, TEXT(",")), Stats.ModelUnits, Stats.PlaceholderUnits, Stats.MissingModel,
		Stats.PartInstances, Stats.CrowdMen, Stats.FarMen, Stats.FadedMen, Stats.Corpses, Stats.Rings, Stats.MovingHulls, Stats.Components, Stats.Figures);
}

UInstancedStaticMeshComponent* ATCUnitRenderer::PartComponent(const FTCModelPart& Part, bool bFaded)
{
	FString Key = Part.Mesh.ToString();
	for (const FSoftObjectPath& M : bFaded ? Part.FadedMaterials : Part.Materials) Key += TEXT("|") + M.ToString();
	if (const TObjectPtr<UInstancedStaticMeshComponent>* Found = PartBatches.Find(Key)) return *Found;
	UInstancedStaticMeshComponent* Ism = NewObject<UInstancedStaticMeshComponent>(this);
	if (!FTCModelLibrary::SetupComponent(Ism, Part, bFaded))
	{
		UE_LOG(LogTCUnitModels, Warning, TEXT("unit models: mesh %s does not load"), *Part.Mesh.ToString());
		PartBatches.Add(Key, nullptr);
		return nullptr;
	}
	Ism->SetupAttachment(RootComponent);
	// the reference's unit models cast the sun's shadow but never receive it (TCLook::UnitChannel): with the units' own
	// sun in the world the part is drawn on channel 1, and a hidden twin on channel 0 casts its shadow
	if (TCLook::HasUnitSun(GetWorld()))
	{
		const bool bCasts = Ism->CastShadow;
		Ism->SetLightingChannels(false, true, false);
		Ism->SetCastShadow(false);
		if (bCasts)
		{
			UInstancedStaticMeshComponent* Twin = NewObject<UInstancedStaticMeshComponent>(this);
			Twin->SetStaticMesh(Ism->GetStaticMesh());
			for (int32 M = 0; M < Ism->GetNumMaterials(); M++) Twin->SetMaterial(M, Ism->GetMaterial(M));
			Twin->SetNumCustomDataFloats(Ism->NumCustomDataFloats);
			Twin->SetCollisionEnabled(ECollisionEnabled::NoCollision);
			Twin->SetMobility(EComponentMobility::Movable);
			Twin->SetCastShadow(true);
			Twin->bCastHiddenShadow = true;
			Twin->SetHiddenInGame(true);
			Twin->SetVisibility(false);
			Twin->SetupAttachment(Ism);
			Twin->RegisterComponent();
			ModelObjects.Add(Twin);
			TCUnitModelsImpl::ShadowTwins().Add(Ism, Twin);
		}
	}
	Ism->RegisterComponent();
	ModelObjects.Add(Ism);
	PartBatches.Add(Key, Ism);
	return Ism;
}

namespace TCUnitModelsImpl
{
	// The overlay ring look (Content/Models/Materials/Look/M_TCLookRing, unreal/Tools/look/make-look-assets.py)
	constexpr const TCHAR* RingPath = TEXT("/Game/Models/Materials/Look/M_TCLookRing.M_TCLookRing");
	// client/overlay.js FRAG_END: the dark edge outside the core
	const FLinearColor RingRim(0.012f, 0.016f, 0.014f, 1.f);
	constexpr float RingHalfCm = 10.f;   // markers.js ownerRing / selectionRing: w 0.2 m
	constexpr float RingMinPx = 1.1f;    // overlay.js Builder.line / ring: min 1.1 px of half-width
	constexpr float RingCore = 0.74f;    // overlay.js: core, where the dark edge starts

	// client/overlay.js Builder.ring: n = max(40, min(360, ceil(2 pi r / 1.1))) steps round the circle, each a pair of
	// vertices on the line pushed out either way along the radius (aEdge), side +1 and -1 (aLine.y), core 0.74; with
	// fill, a hub at the centre (side -3) fanned to the inner edge. Here the edges are built at RingHalfCm from the line
	// (the material moves them further out when the pixel minimum is wider), UV0 = (side, core), UV1 = the edge
	// direction, UV2 = (RingHalfCm, RingMinPx).
	UStaticMesh* BuildRingMesh(UObject* Outer, float RadiusCm, bool bFill, UMaterialInterface* Material)
	{
		FMeshDescription Desc;
		FStaticMeshAttributes Attributes(Desc);
		Attributes.Register();
		TVertexAttributesRef<FVector3f> Positions = Attributes.GetVertexPositions();
		TVertexInstanceAttributesRef<FVector3f> Normals = Attributes.GetVertexInstanceNormals();
		TVertexInstanceAttributesRef<FVector2f> UVs = Attributes.GetVertexInstanceUVs();
		UVs.SetNumChannels(3);
		const FPolygonGroupID Group = Desc.CreatePolygonGroup();
		Attributes.GetPolygonGroupMaterialSlotNames()[Group] = TEXT("Ring");
		const double R = RadiusCm / 100.0;
		const int32 N = FMath::Max(40, FMath::Min(360, (int32)FMath::CeilToDouble(2.0 * PI * R / 1.1)));
		auto Vert = [&](const FVector3f& P, float Side, const FVector2f& Dir, float Half)
		{
			const FVertexID V = Desc.CreateVertex();
			Positions[V] = P;
			const FVertexInstanceID I = Desc.CreateVertexInstance(V);
			Normals[I] = FVector3f(0, 0, 1);
			UVs.Set(I, 0, FVector2f(Side, RingCore));
			UVs.Set(I, 1, Dir);
			UVs.Set(I, 2, FVector2f(Half, Half > 0.f ? RingMinPx : 0.f));
			return I;
		};
		const FVertexInstanceID Hub = bFill ? Vert(FVector3f::ZeroVector, -3.f, FVector2f::ZeroVector, 0.f) : FVertexInstanceID();
		FVertexInstanceID PrevOut, PrevIn;
		for (int32 I = 0; I <= N; I++)
		{
			// the circle from north, clockwise seen from above (sx = sin a, sz = -cos a in three's x z)
			const double A = 2.0 * PI * I / N;
			const FVector2f D((float)FMath::Sin(A), (float)-FMath::Cos(A)); // three (x, z) is Unreal (X, Y)
			const FVertexInstanceID Out = Vert(FVector3f(D.X, D.Y, 0.f) * (RadiusCm + RingHalfCm), 1.f, D, RingHalfCm);
			const FVertexInstanceID In = Vert(FVector3f(D.X, D.Y, 0.f) * (RadiusCm - RingHalfCm), -1.f, -D, RingHalfCm);
			if (I)
			{
				Desc.CreateTriangle(Group, { PrevIn, PrevOut, In });
				Desc.CreateTriangle(Group, { PrevOut, Out, In });
				if (bFill) Desc.CreateTriangle(Group, { Hub, PrevIn, In });
			}
			PrevOut = Out; PrevIn = In;
		}
		UStaticMesh* Mesh = NewObject<UStaticMesh>(Outer);
		Mesh->GetStaticMaterials().Add(FStaticMaterial(Material, TEXT("Ring")));
		UStaticMesh::FBuildMeshDescriptionsParams Params;
		Params.bBuildSimpleCollision = false;
		Params.bFastBuild = true;
		Mesh->BuildFromMeshDescriptions({ &Desc }, Params);
		// a mesh built at run time has no texture-streaming UV data; the ring's material samples no texture, so a default
		// density (a cooked game otherwise trips the engine's 'UVChannelData.bInitialized' ensure)
		for (FStaticMaterial& M : Mesh->GetStaticMaterials()) M.UVChannelData = FMeshUVChannelInfo(1.f);
		return Mesh;
	}
}

UStaticMesh* ATCUnitRenderer::RingMesh(float Radius)
{
	// markers.js ownerRing: the overlay ribbon, 0.2 m wide centred on the radius (selectionRing's filled one: AddRing)
	const int32 Key = FMath::RoundToInt(Radius) * 2;
	if (const TObjectPtr<UStaticMesh>* Found = RingMeshes.Find(Key)) return *Found;
	UStaticMesh* Mesh = TCUnitModelsImpl::BuildRingMesh(this, Radius, false, BaseMaterial);
	ModelObjects.Add(Mesh);
	RingMeshes.Add(Key, Mesh);
	return Mesh;
}

FLinearColor ATCUnitRenderer::RingColorFor(const FLinearColor& Display) const
{
	// what the post's ACES fit maps to Display (linear, before the sRGB transfer): the colour three writes for a ring
	// that is not tone mapped (graphics Low draws straight to the canvas). Only the fallback ring (M_TCLookLine, when
	// M_TCLookRing is missing) uses it now; see AddRing. Per channel, multiplicative steps; the fit is monotone.
	if (RingExposure <= 0.f) return Display;
	FLinearColor X(FMath::Max(Display.R, 1e-4f), FMath::Max(Display.G, 1e-4f), FMath::Max(Display.B, 1e-4f), 1.f);
	for (int32 I = 0; I < 48; I++)
	{
		const FLinearColor Y = TCLook::ACESFilmic(X, RingExposure);
		X.R = FMath::Clamp(X.R * Display.R / FMath::Max(Y.R, 1e-5f), 1e-5f, 64.f);
		X.G = FMath::Clamp(X.G * Display.G / FMath::Max(Y.G, 1e-5f), 1e-5f, 64.f);
		X.B = FMath::Clamp(X.B * Display.B / FMath::Max(Y.B, 1e-5f), 1e-5f, 64.f);
	}
	return X;
}

void ATCUnitRenderer::UpdateRingLooks()
{
	if (!LightingSeen.IsValid() && GetWorld())
	{
		TActorIterator<ATCLighting> It(GetWorld());
		if (It) LightingSeen = *It;
	}
	const float Exposure = LightingSeen.IsValid() ? LightingSeen->Shown().Exposure : 0.f;
	if (Exposure == RingExposure) return;
	RingExposure = Exposure;
	// only the fallback rings are listed here (their colour follows the exposure); M_TCLookRing rings are tone mapped
	for (auto& P : RingLooks) if (P.Value.Mid) P.Value.Mid->SetVectorParameterValue(TEXT("Color"), RingColorFor(P.Value.Display));
}

void ATCUnitRenderer::AddRing(float Radius, const FLinearColor& Color, float Opacity, int32 ColorKey, const FVector& At)
{
	using namespace TCUnitModelsImpl;
	const int32 Key = FMath::RoundToInt(Radius) * 1000 + ColorKey;
	TObjectPtr<UInstancedStaticMeshComponent>* Found = RingBatches.Find(Key);
	UInstancedStaticMeshComponent* Ism = Found ? Found->Get() : nullptr;
	if (!Found)
	{
		// markers.js: the selection ring (white, colour key 100) is a thin light ring over a faint wash (fill 0.08)
		const bool bSelection = ColorKey == 100;
		UMaterialInterface* Overlay = LoadObject<UMaterialInterface>(nullptr, RingPath);
		Ism = NewObject<UInstancedStaticMeshComponent>(this);
		if (Overlay)
		{
			const int32 MeshKey = FMath::RoundToInt(Radius) * 2 + (bSelection ? 1 : 0);
			TObjectPtr<UStaticMesh>* Mesh = RingMeshes.Find(MeshKey);
			if (!Mesh)
			{
				UStaticMesh* Made = BuildRingMesh(this, Radius, bSelection, Overlay);
				ModelObjects.Add(Made);
				Mesh = &RingMeshes.Add(MeshKey, Made);
			}
			Ism->SetStaticMesh(*Mesh);
		}
		else Ism->SetStaticMesh(RingMesh(Radius));
		Ism->SetCollisionEnabled(ECollisionEnabled::NoCollision);
		Ism->SetCastShadow(false);
		Ism->SetMobility(EComponentMobility::Movable);
		// markers.js overlay rings: see-through (opacity 0.9 owner, 0.95 selection), no depth write, drawn after the
		// effects' particles (renderOrder 2 and 3 against fx.js 1.5), so a smoke screen never hides them; the sprite
		// batch sorts at 10 (Client/TCEffects.cpp)
		Ism->SetTranslucentSortPriority(20);
		// the material widens the ribbon to its pixel minimum in the vertex shader: room for that in the bounds
		Ism->SetBoundsScale(1.5f);
		Ism->SetupAttachment(RootComponent);
		Ism->RegisterComponent();
		if (Overlay)
		{
			// client/overlay.js makeOverlay. The game's graphics High (the default) draws through light.js's composer, so
			// the overlay is tone mapped with the rest of the frame (OutputPass) and toneMapped: false has no effect
			// there: the ring's linear colour goes in as it is and M_TCLookPost tone maps it.
			UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(Overlay, this);
			Mid->SetVectorParameterValue(TEXT("Color"), Color);
			Mid->SetVectorParameterValue(TEXT("Rim"), RingRim);
			Mid->SetScalarParameterValue(TEXT("Opacity"), Opacity);
			Mid->SetScalarParameterValue(TEXT("Fill"), bSelection ? 0.08f : 0.f);
			Ism->SetMaterial(0, Mid);
			ModelObjects.Add(Mid);
		}
		else if (UMaterialInterface* Line = LoadObject<UMaterialInterface>(nullptr, TCLook::LinePath))
		{
			// without M_TCLookRing: the plain ribbon of slice 11 (unlit, see-through, the ACES pre-image of its colour)
			UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(Line, this);
			Mid->SetVectorParameterValue(TEXT("Color"), RingColorFor(Color));
			Mid->SetScalarParameterValue(TEXT("Opacity"), Opacity);
			Ism->SetMaterial(0, Mid);
			RingLooks.Add(Key, { Mid, Color });
			ModelObjects.Add(Mid);
		}
		else if (BaseMaterial)
		{
			// without the look assets: the opaque engine material, as before slice 11
			UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(BaseMaterial, this);
			Mid->SetVectorParameterValue(TEXT("Color"), Color);
			Ism->SetMaterial(0, Mid);
			ModelObjects.Add(Mid);
		}
		ModelObjects.Add(Ism);
		RingBatches.Add(Key, Ism);
	}
	if (!Ism) return;
	Frame.FindOrAdd(Ism).Transforms.Add(FTransform(At));
	Stats.Rings++;
}

bool ATCUnitRenderer::DrawModel(const FTCMatchState& State, const FTCUnit& U, const FTCUnitDef* Def, FShown& S, const ATCTerrain* Terrain, bool bSelected, float Dt, const FVector& Eye)
{
	using namespace TCUnitModelsImpl;
	// main.js facOf: the start's faction for the seat, else slot % 3
	const int32 Faction = State.Factions.IsValidIndex(U.Owner) ? State.Factions[U.Owner] : U.Owner < 0 ? 0 : U.Owner % 3;
	ETCModelMatch Match = ETCModelMatch::None;
	const FTCModelEntry* Entry = Library.Resolve({ U.Type, Faction, FString() }, &Match);
	if (!Entry) return false;
	const float Ground = Terrain ? Terrain->HeightAt(S.X, S.Z) : 0.f;
	const FVector At = ATCTerrain::ToWorld(S.X, S.Z, Ground);
	const FQuat Facing(FVector::UpVector, S.Rot);
	FTransform Root(Facing, At + FVector(0, 0, LiftMetres(Def) * 100.f));
	// main.js: a structure's body rises from 0.15 of its height as it is built
	if (Def && Def->bStructure) Root.SetScale3D(FVector(1, 1, 0.15f + 0.85f * FMath::Clamp(U.Built, 0.f, 1.f)));
	// unit-models.js animate for a squad or crew first: it decides the level of detail of the men and of their gun
	FSquadMen Men;
	const bool bMen = AnimateSquad(*Entry, U, Def, Faction, S, FTransform(Facing, At), Terrain, Dt, Eye, Men);
	if (!bMen && Def && Def->bInfantry)
	{
		// a squad type without men in its model: the plain LOD swap, 4 m of hysteresis
		const double Metres2 = FVector::DistSquared(Eye, At) / 10000.0;
		S.bFar = S.bFar ? Metres2 > FMath::Square(LodMetres - 4.0) : Metres2 > FMath::Square(LodMetres + 4.0);
	}
	const bool bFaded = Camouflages(U.Type) && (U.Flags & 256);
	const TArray<float> Custom = Library.CustomData(U.Owner, Faction, bFaded ? Library.CamouflageFade : 1.f, 0.f);
	// main.js: turret.rotation.y = -(aim - rot), every mount of a ship the same; an Unreal yaw of aim - rot
	TMap<FName, FQuat> Turns;
	double Turn = FMath::FindDeltaAngleRadians(S.Rot, S.Aim);
	if (const double Limit = TraverseLimit(*Entry)) Turn = FMath::Clamp(Turn, -Limit, Limit);
	const FQuat Traverse(FVector::UpVector, Turn);
	TArray<bool> Visible;
	Visible.SetNum(Entry->Parts.Num());
	for (int32 I = 0; I < Entry->Parts.Num(); I++)
	{
		const FTCModelPart& Part = Entry->Parts[I];
		const FString Name = Part.Name.ToString();
		if (Name == TEXT("turret") || Name.StartsWith(TEXT("mount_")) || (GunTraverses(U.Type) && Name.StartsWith(TEXT("gun_")))) Turns.Add(Part.Name, Traverse);
		int32 Man; bool bNear;
		if (ParseMan(Name, Man, bNear)) { Visible[I] = false; continue; } // DrawSquadMen
		const int32 Lod = LodOf(Part.Name);
		bool bShow = Lod == 1 ? !S.bFar : Lod == 2 ? S.bFar : !Part.bHidden;
		const int32 Parent = Part.Parent.IsNone() ? INDEX_NONE : Entry->FindPart(Part.Parent);
		if (Parent != INDEX_NONE && !Visible[Parent]) bShow = false;
		Visible[I] = bShow;
	}
	// model-motion.js, aircraft.js, unit-models.js stride: the hull settling, the swell, the bank, the propellers and the
	// kaiju's legs (squads and their guns have none: animate() moves the men)
	static const FTCPartMotion Still;
	const bool bMoves = !(Def && Def->bInfantry) && !(Def && Def->bStructure);
	const FTCPartMotion& Motion = bMoves ? UnitMotion.Update(U.Id, U.Type, Root.GetLocation(), S.Rot, true, Dt, GetWorld() ? GetWorld()->GetTimeSeconds() : 0.0) : Still;
	if (bMoves && (FTCUnitMotion::HasHullMotion(U.Type) || FTCUnitMotion::IsPlane(U.Type))) Stats.MovingHulls++;
	// resolve every component first, so a unit with a missing mesh falls back whole instead of half drawn
	TArray<TPair<UInstancedStaticMeshComponent*, FTransform>, TInlineAllocator<16>> Parts;
	for (int32 I = 0; I < Entry->Parts.Num(); I++)
	{
		if (!Visible[I]) continue;
		UInstancedStaticMeshComponent* Ism = PartComponent(Entry->Parts[I], bFaded);
		if (!Ism) return false;
		Parts.Add({ Ism, FTCUnitMotion::PartTransform(*Entry, I, Root, Turns, Motion) });
	}
	// unit-models.js vehicleDamageAnchor for a ground vehicle (main.js runs vehicleDamageFrame for those): the engine
	// deck point the damage smoke rises from, carried by the hull motion (Client/TCEffects.h VehicleDamageFrame)
	if (bMoves && Def && !Def->bAir && !FTCUnitMotion::IsSea(U.Type))
		UnitMotion.SetDamageAnchor(U.Id, (Motion.Body * Root).TransformPosition(DamageAnchorLocal(UnitMotion, *Entry)));
	for (const auto& P : Parts)
	{
		FFrameBatch& B = Frame.FindOrAdd(P.Key);
		B.Floats = TCModelData::Count;
		B.Transforms.Add(P.Value);
		B.Data.Append(Custom);
		Stats.PartInstances++;
	}
	if (bMen) DrawSquadMen(*Entry, U, Faction, S, FTransform(Facing, At), Men, Custom);
	// main.js makeUnit: the owner ring (white while it retreats), and the selection ring; planes show neither ring
	const float Radius = Def ? Def->Radius : 1.5f;
	if (!(Def && Def->bAir))
	{
		const bool bRetreat = (U.Flags & 1) != 0;
		AddRing((Radius + 0.4f) * 100.f, bRetreat ? FLinearColor(FColor(0xff, 0xff, 0xff)) : SeatColor(U.Owner), 0.9f, bRetreat ? 101 : FMath::Clamp(U.Owner, -1, 62) + 2, At + FVector(0, 0, 10.f));
	}
	if (bSelected) AddRing((Radius + 1.f) * 100.f, FLinearColor(FColor(0xf4, 0xf2, 0xea)), 0.95f, 100, Root.GetLocation() + FVector(0, 0, 14.f));
	return true;
}

bool ATCUnitRenderer::AnimateSquad(const FTCModelEntry& Entry, const FTCUnit& U, const FTCUnitDef* Def, int32 Faction, FShown& S, const FTransform& Root, const ATCTerrain* Terrain, float Dt, const FVector& Eye, FSquadMen& Out)
{
	using namespace TCUnitModelsImpl;
	// man_K_near and man_K_far, by K
	for (const FTCModelPart& P : Entry.Parts) { int32 Man; bool bNear; if (ParseMan(P.Name.ToString(), Man, bNear)) Out.Men = FMath::Max(Out.Men, Man + 1); }
	if (!Out.Men) return false;
	Out.Near.Init(INDEX_NONE, Out.Men); Out.Far.Init(INDEX_NONE, Out.Men);
	for (int32 I = 0; I < Entry.Parts.Num(); I++)
	{
		int32 Man; bool bNear;
		if (ParseMan(Entry.Parts[I].Name.ToString(), Man, bNear)) (bNear ? Out.Near : Out.Far)[Man] = I;
	}
	// main.js: alive = ceil(hp * men / (models * hpPer)), one for an operative
	const int32 Total = Def ? Def->Models * Def->HpPer : 0;
	Out.Alive = U.Flags & TCFlags::Operative ? 1 : Total > 0 ? FMath::Clamp((int32)FMath::CeilToDouble((double)U.Hp * Out.Men / Total), 0, Out.Men) : Out.Men;
	Out.FigureType = FTCModelLibrary::FigureType(U.Type);

	FSquad& Sq = Squads.FindOrAdd(U.Id);
	if (!Sq.Anim.IsSetUp(U.Type, Faction, Out.Men))
	{
		// unit-models.js man.position: SLOTS * SLOT_SCALE, the catalog's man_K_near rest places; man.scale likewise
		TArray<FVector2D> Slots;
		float Scale = 0.f;
		for (int32 K = 0; K < Out.Men; K++)
		{
			const FTransform& Rest = Out.Near[K] != INDEX_NONE ? Entry.Parts[Out.Near[K]].Rest : FTransform::Identity;
			Slots.Add(FVector2D(Rest.GetLocation().X, Rest.GetLocation().Y));
			if (Out.Near[K] != INDEX_NONE && Scale == 0.f) Scale = static_cast<float>(Rest.GetScale3D().X);
		}
		Sq.Anim.Setup(U.Type, Faction, Slots, Scale > 0.f ? Scale : 1.1f);
	}
	Sq.Anim.SetAlive(Out.Alive);
	FTCInfantrySquadFrame F;
	F.Root = Root.GetLocation();
	F.Yaw = -S.Rot; // three's root.rotation.y
	F.Eye = Eye;
	F.LodMetres = LodMetres;
	F.Id = U.Id; F.Flags = U.Flags; F.Cover = U.Cover; F.Suppression = U.Supp;
	F.bHasTarget = U.Target != 0; F.bMoveFire = MovesFiring(U.Type); F.Aim = S.Aim;
	F.bGarrisoned = (U.Flags & TCFlags::Garrisoned) != 0;
	if (Terrain) F.GroundCm = [Terrain](double X, double Y) { return Terrain->HeightAt(X / 100.0, Y / 100.0) * 100.0; };
	Sq.Anim.Animate(F, Dt);
	S.bFar = Sq.Anim.IsFar();
	return true;
}

void ATCUnitRenderer::DrawSquadMen(const FTCModelEntry& Entry, const FTCUnit& U, int32 Faction, const FShown& S, const FTransform& Root, const FSquadMen& Men, const TArray<float>& Custom)
{
	using namespace TCUnitModelsImpl;
	const FSquad* Sq = Squads.Find(U.Id);
	if (!Sq) return;
	const FTCInfantrySquadAnimation& Anim = Sq->Anim;
	const bool bFaded = Camouflages(U.Type) && (U.Flags & 256);
	const TArray<FTCInfantryManPose>& Posed = Anim.Men();
	for (int32 K = 0; K < Men.Men && K < Posed.Num(); K++)
	{
		if (!Posed[K].bVisible) continue;
		FTCSoldierDraw D;
		D.FigureType = Men.FigureType; D.Faction = Faction; D.Slot = K;
		D.Man = Anim.ManTransform(K, Root);
		D.Blend = Posed[K].Blend;
		D.Owner = Library.SeatColor(U.Owner);
		D.Fade = bFaded ? Library.CamouflageFade : 1.f; // main.js camouflage opacity 0.45 (flags & 256)
		D.bFar = S.bFar;
		D.UnitId = U.Id; D.ManIndex = K;
		if (Crowd && Crowd->AddMan(D))
		{
			(S.bFar ? Stats.FarMen : Stats.CrowdMen)++;
			if (bFaded) Stats.FadedMen++;
			continue;
		}
		// the crowd has no figure for him: the catalog's figure in its rest pose, where animate() put him
		const int32 PartIndex = S.bFar ? (Men.Far.IsValidIndex(K) ? Men.Far[K] : INDEX_NONE) : Men.Near[K];
		if (PartIndex == INDEX_NONE) continue;
		if (UInstancedStaticMeshComponent* Ism = PartComponent(Entry.Parts[PartIndex], bFaded))
		{
			FFrameBatch& B = Frame.FindOrAdd(Ism);
			B.Floats = TCModelData::Count;
			B.Transforms.Add(D.Man);
			B.Data.Append(Custom);
			if (S.bFar) Stats.FarMen++; else Stats.PartInstances++;
		}
	}
	// main.js: the men past the new alive count fall where they stood
	if (Crowd) Crowd->SquadAlive(U.Id, Men.Alive);
}

void ATCUnitRenderer::FlushModelFrame()
{
	TArray<UInstancedStaticMeshComponent*, TInlineAllocator<256>> All;
	for (const auto& P : PartBatches) if (P.Value) All.Add(P.Value);
	for (const auto& P : RingBatches) if (P.Value) All.Add(P.Value);
	for (UInstancedStaticMeshComponent* Ism : All)
	{
		const FFrameBatch* B = Frame.Find(Ism);
		const int32 N = B ? B->Transforms.Num() : 0;
		LastInstances += N;
		if (Ism->GetInstanceCount() == N)
		{
			if (!N) continue;
			Ism->BatchUpdateInstancesTransforms(0, B->Transforms, true, false, true);
		}
		else
		{
			Ism->ClearInstances();
			if (N) Ism->AddInstances(B->Transforms, false, true, false);
		}
		if (N && B->Floats > 0 && B->Data.Num() == N * B->Floats) Ism->SetCustomData(0, N - 1, B->Data, false);
		Ism->MarkRenderStateDirty();
	}
	// the shadow twins follow their parts
	for (auto It = TCUnitModelsImpl::ShadowTwins().CreateIterator(); It; ++It)
	{
		UInstancedStaticMeshComponent* Ism = It.Key().Get();
		UInstancedStaticMeshComponent* Twin = It.Value().Get();
		if (!Ism || !Twin) { It.RemoveCurrent(); continue; }
		if (Ism->GetOwner() != this) continue;
		const FFrameBatch* B = Frame.Find(Ism);
		const int32 N = B ? B->Transforms.Num() : 0;
		if (Twin->GetInstanceCount() == N)
		{
			if (!N) continue;
			Twin->BatchUpdateInstancesTransforms(0, B->Transforms, true, false, true);
		}
		else
		{
			Twin->ClearInstances();
			if (N) Twin->AddInstances(B->Transforms, false, true, false);
		}
		if (N && B->Floats > 0 && B->Data.Num() == N * B->Floats && Twin->NumCustomDataFloats == B->Floats) Twin->SetCustomData(0, N - 1, B->Data, false);
		Twin->MarkRenderStateDirty();
	}
	Stats.Components = All.Num();
}

void ATCUnitRenderer::ReportFrame()
{
	const double Dt = FApp::GetDeltaTime();
	FrameSum += Dt; FrameMax = FMath::Max(FrameMax, Dt); FrameCount++;
	const double Now = FPlatformTime::Seconds();
	if (ReportAt == 0) ReportAt = Now;
	if (Now - ReportAt < 5.0) return;
	UE_LOG(LogTCUnitModels, Log, TEXT("unit models: %s | frame %.2f ms mean, %.2f ms worst over %d frames"), *ModelStatusLine(), FrameSum / FMath::Max(1, FrameCount) * 1000.0, FrameMax * 1000.0, FrameCount);
	ReportAt = Now; FrameSum = FrameMax = 0; FrameCount = 0;
}
