#include "Client/TCCampModels.h"
#include "Client/TCGameMode.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCLighting.h"
#include "Client/TCMarkers.h"
#include "World/TCTerrain.h"
#include "World/TCFogFrontier.h"
#include "ThreeCrossroads.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Components/SceneComponent.h"
#include "Engine/StaticMesh.h"
#include "Engine/Texture2D.h"
#include "Engine/World.h"
#include "Materials/MaterialInterface.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/Material.h"
#include "ProceduralMeshComponent.h"
#include "StaticMeshAttributes.h"
#include "StaticMeshResources.h"
#include "Misc/AutomationTest.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Client/TCCampModels.Data.inl"

namespace
{
	const FName CampSurfaceSlot(TEXT("CampSurface"));
	bool HasCampSurfaceSlot(const UStaticMesh* Mesh)
	{
		const FStaticMeshRenderData* Data = Mesh ? Mesh->GetRenderData() : nullptr;
		return Data && Data->LODResources.Num() == 1 && Data->LODResources[0].Sections.Num() == 1 &&
			Data->LODResources[0].Sections[0].MaterialIndex == 0 && Mesh->GetStaticMaterials().Num() == 1 &&
			Mesh->GetStaticMaterials()[0].MaterialSlotName == CampSurfaceSlot;
	}
	UInstancedStaticMeshComponent* InstanceComponent(AActor* A, UStaticMesh* Mesh, const TArray<UMaterialInterface*>& Materials)
	{
		auto* C = NewObject<UInstancedStaticMeshComponent>(A);
		C->SetMobility(EComponentMobility::Movable);
		C->SetupAttachment(A->GetRootComponent());
		C->SetStaticMesh(Mesh);
		C->SetCollisionEnabled(ECollisionEnabled::NoCollision);
		C->NumCustomDataFloats = 6;
		for (int32 I = 0; I < Materials.Num(); I++)
		{
#if WITH_EDITOR
			// Complete generated materials once before these dynamic meshes draw in uncooked -game.
			if (Materials[I]) Materials[I]->GetMaterial()->EnsureIsComplete();
#endif
			C->SetMaterial(I, Materials[I]);
		}
		A->AddInstanceComponent(C); C->RegisterComponent();
		// The component override alone cannot repair an invalid render-section material index in a cooked build.
		static const bool bAudit = FParse::Param(FCommandLine::Get(), TEXT("TCCampMaterialAudit"));
		if (bAudit && Mesh && Mesh->GetRenderData())
			for (const FStaticMeshLODResources& Lod : Mesh->GetRenderData()->LODResources)
				for (const FStaticMeshSection& S : Lod.Sections)
					UE_LOG(LogTC, Log, TEXT("camp material: section %d, resolved %s, default %d, instanced %d"),
						S.MaterialIndex, *GetPathNameSafe(C->GetMaterial(S.MaterialIndex)),
						C->GetMaterial(S.MaterialIndex) == UMaterial::GetDefaultMaterial(MD_Surface),
						C->GetMaterial(S.MaterialIndex) && C->GetMaterial(S.MaterialIndex)->CheckMaterialUsage_Concurrent(MATUSAGE_InstancedStaticMeshes));
		return C;
	}
	UMaterialInterface* Surface(const TCHAR* Name)
	{
		const FString Path = FString::Printf(TEXT("/Game/World/Materials/Instances/MI_TCWorld_Surface_%s.MI_TCWorld_Surface_%s"), Name, Name);
		auto* Mat = LoadObject<UMaterialInterface>(nullptr, *Path);
		if (!Mat) UE_LOG(LogTC, Error, TEXT("camp models: missing %s; regenerate world materials including HQ camp slots"), *Path);
		return Mat;
	}
	void Add(UInstancedStaticMeshComponent* C, const FTransform& T, const FLinearColor& Tint = FLinearColor::White)
	{
		const int32 I = C->AddInstance(T, true);
		const float D[6] = { Tint.R, Tint.G, Tint.B, 1, 1, 1 };
		C->SetCustomData(I, MakeArrayView(D), true);
	}
	UInstancedStaticMeshComponent* CastWithoutReceiving(AActor* A, UInstancedStaticMeshComponent* Visible, bool bUnitSun)
	{
		// main.js mesh() sets castShadow=true on the HQ cloth and node box. receiveShadow keeps Three's false
		// default. The visible pieces use the unshadowed unit sun, with hidden scenery-channel casters.
		Visible->SetCastShadow(!bUnitSun);
		if (!bUnitSun) return nullptr;
		Visible->SetLightingChannels(false, true, false);
		auto* Caster = InstanceComponent(A, Visible->GetStaticMesh(), { Visible->GetMaterial(0) });
		Caster->SetCastShadow(true); Caster->bCastHiddenShadow = true;
		Caster->SetHiddenInGame(true); Caster->SetVisibility(false);
		return Caster;
	}
	template <size_t N> UStaticMesh* ReferenceMesh(UObject* Outer, const double (&V)[N][8], UMaterialInterface* Mat)
	{
		FMeshDescription D; FStaticMeshAttributes Attr(D); Attr.Register();
		auto Pos = Attr.GetVertexPositions(); auto Norm = Attr.GetVertexInstanceNormals();
		auto Tangent = Attr.GetVertexInstanceTangents(); auto Sign = Attr.GetVertexInstanceBinormalSigns();
		auto Uv = Attr.GetVertexInstanceUVs(); Uv.SetNumChannels(4);
		auto Col = Attr.GetVertexInstanceColors();
		const FPolygonGroupID Group = D.CreatePolygonGroup();
		// BuildFromMeshDescription resolves this name against FStaticMaterial::MaterialSlotName at runtime.
		// An unnamed group with a non-null material resolves to INDEX_NONE, then the default grey surface.
		Attr.GetPolygonGroupMaterialSlotNames()[Group] = CampSurfaceSlot;
		for (size_t I = 0; I < N; I += 3)
		{
			TArray<FVertexInstanceID> Tri;
			// Three's counter-clockwise front faces become Unreal's clockwise faces through the y/z swap.
			for (int K : { 0, 1, 2 })
			{
				const double* P = V[I + K];
				const FVertexID Id = D.CreateVertex(); Pos[Id] = FVector3f(P[0], P[1], P[2]);
				const FVertexInstanceID Vi = D.CreateVertexInstance(Id);
				Norm[Vi] = FVector3f(P[3], P[4], P[5]); Col[Vi] = FVector4f(1, 1, 1, 1);
				const FVector3f Side = FMath::Abs(Norm[Vi].Z) < 0.9f ? FVector3f::UpVector : FVector3f::ForwardVector;
				Tangent[Vi] = FVector3f::CrossProduct(Side, Norm[Vi]).GetSafeNormal(); Sign[Vi] = 1.f;
				Uv.Set(Vi, 0, FVector2f(P[6], P[7])); Uv.Set(Vi, 3, FVector2f(1, 0));
				Tri.Add(Vi);
			}
			D.CreateTriangle(Group, Tri);
		}
		auto* Mesh = NewObject<UStaticMesh>(Outer); Mesh->GetStaticMaterials().Add(FStaticMaterial(Mat, CampSurfaceSlot));
		UStaticMesh::FBuildMeshDescriptionsParams P; P.bBuildSimpleCollision = false; P.bFastBuild = true;
		if (!Mesh->BuildFromMeshDescriptions({ &D }, P)) return nullptr;
		if (!HasCampSurfaceSlot(Mesh))
		{
			UE_LOG(LogTC, Error, TEXT("camp models: runtime mesh did not bind CampSurface to render section 0 (%s)"), *GetPathNameSafe(Mat));
			return nullptr;
		}
		if (FParse::Param(FCommandLine::Get(), TEXT("TCCampMaterialAudit")))
			UE_LOG(LogTC, Log, TEXT("camp runtime surface: %s; section 0; slot CampSurface; UV channels %d"),
				*GetPathNameSafe(Mat), Mesh->GetRenderData()->LODResources[0].GetNumTexCoords());
		for (FStaticMaterial& M : Mesh->GetStaticMaterials()) M.UVChannelData = FMeshUVChannelInfo(1.f);
		return Mesh;
	}
}

FTCMarkerScene::~FTCMarkerScene()
{
	if (AActor* A = Actor.Get()) A->Destroy();
	if (UTexture2D* Fog = FogTexture.Get()) Fog->RemoveFromRoot();
}

bool FTCMarkerScene::TentForMode(const FString& Mode, bool bWorldMap)
{
	// main.js classicMode() and shared/sim.js isSkirmishBaseMode().
	return !bWorldMap && !Mode.IsEmpty() && Mode != TEXT("classic") && Mode != TEXT("world") &&
		Mode != TEXT("conquest") && Mode != TEXT("assault") && Mode != TEXT("annihilation") && Mode != TEXT("horde");
}

FString FTCMarkerScene::CampMode(const FString& StartMode, const FString& LobbyMode)
{
	// A same-token join can receive start before its first lobby. Rebuild when that lobby supplies the mode.
	return LobbyMode.IsEmpty() ? StartMode : LobbyMode;
}

bool FTCMarkerScene::BagInside(double X, double Z, double W, double H)
{
	return X >= 0.75 && Z >= 0.75 && X <= W - 0.75 && Z <= H - 0.75;
}

bool FTCMarkerScene::CampSlotDrawn(int32 Slot, const TArray<TOptional<FVector2D>>& Spawns)
{
	if (!Spawns.IsValidIndex(Slot) || !Spawns[Slot].IsSet()) return false;
	// Tutorial shares an HQ position between seats. Three's LessEqualDepth leaves the last camp's paint
	// visible on coincident opaque faces. Keep that same camp instead of relying on Unreal's depth tie order.
	for (int32 Later = Slot + 1; Later < Spawns.Num(); Later++)
		if (Spawns[Later].IsSet() && Spawns[Later].GetValue() == Spawns[Slot].GetValue()) return false;
	return true;
}

void FTCMarkerScene::Begin(const ATCGameMode& M)
{
	if (!Actor.IsValid())
	{
		FActorSpawnParameters P; P.ObjectFlags |= RF_Transient;
		AActor* A = M.GetWorld()->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity, P);
		if (!A) return;
		Actor = A;
		auto* Root = NewObject<USceneComponent>(A); A->SetRootComponent(Root); A->AddInstanceComponent(Root); Root->RegisterComponent();
		for (int32 I = 0; I < 3; I++)
		{
			auto* C = NewObject<UProceduralMeshComponent>(A);
			C->SetupAttachment(Root); C->SetCollisionEnabled(ECollisionEnabled::NoCollision); C->SetCastShadow(false);
			C->SetTranslucentSortPriority(I == 2 ? 0 : I ? 4 : 2);
			A->AddInstanceComponent(C); C->RegisterComponent();
			if (I == 2) Borders = C; else if (I) Through = C; else Ground = C;
		}
	}
	if (Frame != GFrameCounter)
	{
		Frame = GFrameCounter; Section = 0;
		Ground->ClearAllMeshSections(); Through->ClearAllMeshSections(); Borders->ClearAllMeshSections();
		DepthBatch.Reset(); ThroughBatch.Reset(); BorderBatch.Reset();
		UpdateFog(M);
		SyncModels(M);
	}
}

void FTCMarkerScene::Triangle(const FVector& A, const FVector& B, const FVector& C, const FLinearColor& Colour, bool bDepthTest, bool bBeforeFog)
{
	if (Colour.A <= 0) return;
	FBatch& Batch = bBeforeFog ? BorderBatch : bDepthTest ? DepthBatch : ThroughBatch;
	const int32 Start = Batch.Vertices.Num();
	Batch.Vertices.Append({ A, B, C }); Batch.Indices.Append({ Start, Start + 1, Start + 2 });
	Batch.Colours.Append({ Colour, Colour, Colour });
	Batch.UV.Append({ FVector2D(-3,0), FVector2D(-3,0), FVector2D(-3,0) });
}

void FTCMarkerScene::RibbonTriangle(const FVector& A, const FVector& B, const FVector& C, const FLinearColor& Colour, bool bDepthTest, const FVector2D& U, const FVector2D& V, const FVector2D& W)
{
	if (Colour.A <= 0) return;
	Triangle(A,B,C,Colour,bDepthTest);
	FBatch& Batch = bDepthTest ? DepthBatch : ThroughBatch;
	const int32 K=Batch.UV.Num()-3;
	Batch.UV[K]=U; Batch.UV[K+1]=V; Batch.UV[K+2]=W;
}

void FTCMarkerScene::Flush()
{
	if (!Actor.IsValid()) return;
	for (int32 I = 0; I < 3; I++)
	{
		FBatch& B = I == 2 ? BorderBatch : I ? ThroughBatch : DepthBatch;
		if (B.Vertices.IsEmpty()) continue;
		auto* C = I == 2 ? Borders.Get() : I ? Through.Get() : Ground.Get();
		// A/B verification only: the same ground triangles with scene depth disabled. No game state changes.
		static const bool bGroundThrough = FParse::Param(FCommandLine::Get(), TEXT("TCMarkersGroundThrough"));
		const TCHAR* Path = (I == 1 || bGroundThrough) ? TEXT("/Game/Markers/M_TCMarkerThrough.M_TCMarkerThrough") : TEXT("/Game/Markers/M_TCMarkerGround.M_TCMarkerGround");
		UMaterialInterface* Mat = LoadObject<UMaterialInterface>(nullptr, Path);
		if (!Mat) { UE_LOG(LogTC, Error, TEXT("markers: missing %s; run the marker material generator"), Path); B.Reset(); continue; }
		C->CreateMeshSection_LinearColor(Section, B.Vertices, B.Indices, {}, B.UV, B.Colours, {}, false, false);
		C->SetMaterial(Section, Mat); Section++; B.Reset();
	}
}

// Camp materials share the recipient's fog look with all the other map pieces. Geometry remains where
// buildHQ/nodeMark put it; a remembered node's box dims when it leaves sight.
UMaterialInterface* FTCMarkerScene::WithFog(UMaterialInterface* Base)
{
	if (!Base) return nullptr;
	if (auto* Existing = FogMaterials.Find(Base)) if (Existing->IsValid()) return Existing->Get();
	auto* Mid = UMaterialInstanceDynamic::Create(Base, Actor.Get());
	Mid->SetTextureParameterValue(TEXT("FogMap"), FogTexture.Get());
	Mid->SetVectorParameterValue(TEXT("FogSize"), FLinearColor(FogW * 2.f, FogH * 2.f, 0, 1));
	Mid->SetScalarParameterValue(TEXT("FogOn"), 1.f);
	FogMaterials.Add(Base, Mid);
	return Mid;
}

void FTCMarkerScene::UpdateFog(const ATCGameMode& M)
{
	const int32 W = M.State.Map.W, H = M.State.Map.H;
	if (W <= 0 || H <= 0) return;
	bool bChanged = false;
	if (!FogTexture.IsValid() || W != FogW || H != FogH)
	{
		if (auto* Old = FogTexture.Get()) Old->RemoveFromRoot();
		auto* T = UTexture2D::CreateTransient(W, H, PF_G8);
		if (!T) return;
		T->AddToRoot(); T->SRGB = false; T->Filter = TF_Bilinear; // fog.js LinearFilter, like the other world surfaces
		T->AddressX = TA_Clamp; T->AddressY = TA_Clamp; T->NeverStream = true; T->UpdateResource();
		FogTexture = T; FogW = W; FogH = H; FogBytes.Init(0, W * H);
		bChanged = true; // a clear (all-zero alpha) texture still needs its first upload
		for (auto& Pair : FogMaterials) if (auto* Mid = Pair.Value.Get())
		{
			Mid->SetTextureParameterValue(TEXT("FogMap"), T);
			Mid->SetVectorParameterValue(TEXT("FogSize"), FLinearColor(W * 2.f, H * 2.f, 0, 1));
		}
	}
	const bool bReveal = M.Terrain && M.Terrain->IsEndRevealed();
	for (int32 C = 0; C < FogBytes.Num(); C++)
	{
		// World materials consume fog.js alpha, not the legacy brightness multiplier. Clear is zero.
		const uint8 Value = TCFogLook::AlphaByte(M.State, bReveal, C);
		bChanged |= FogBytes[C] != Value; FogBytes[C] = Value;
	}
	if (!bChanged) return;
	auto* Copy = new uint8[FogBytes.Num()];
	FMemory::Memcpy(Copy, FogBytes.GetData(), FogBytes.Num());
	auto* Region = new FUpdateTextureRegion2D(0, 0, 0, 0, W, H);
	FogTexture->UpdateTextureRegions(0, 1, Region, W, 1, Copy,
		[](uint8* Data, const FUpdateTextureRegion2D* Regions) { delete[] Data; delete Regions; });
}

void FTCMarkerScene::SyncModels(const ATCGameMode& M)
{
	const auto& S = M.State;
	const FString Mode = CampMode(S.StartMode, S.Mode);
	FString Key = FString::Printf(TEXT("%d:%d:%s:%d:%d"), S.Map.W, S.Map.H, *Mode, S.Map.bWorld, M.Markers.ResourceNodes().Num());
	for (int32 I = 0; I < S.Spawns.Num(); I++) if (S.Spawns[I].IsSet()) Key += FString::Printf(TEXT(":%d,%s,%d"), I, *S.Spawns[I]->ToString(), S.Factions.IsValidIndex(I) ? S.Factions[I] : 0);
	if (Key == ModelKey) return;
	for (auto C : Models) if (C.IsValid()) C->DestroyComponent(); Models.Reset();
	ModelKey = Key;
	FString Error;
	if (!Library.IsLoaded() && !Library.Load(Error)) { UE_LOG(LogTC, Error, TEXT("camp models: %s"), *Error); return; }
	AActor* A = Actor.Get();
	const bool bTent = TentForMode(Mode, S.Map.bWorld);
	UMaterialInterface* BagMat = WithFog(Surface(TEXT("burlap"))), *WoodMat = WithFog(Surface(TEXT("wood")));
	UStaticMesh* BagMesh = ReferenceMesh(A, BagVertices, BagMat);
	auto* Bags = InstanceComponent(A, BagMesh, { BagMat }); Models.Add(Bags);
	UMaterialInterface* FlagBase = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Effects/Materials/M_TCHQCloth.M_TCHQCloth"));
	UStaticMesh* FlagMesh = ReferenceMesh(A, FlagVertices, FlagBase);
	int32 BagCount = 0;
	for (int32 Slot = 0; Slot < S.Spawns.Num(); Slot++)
	{
		if (!S.Spawns[Slot].IsSet()) continue;
		const FVector2D Sp = S.Spawns[Slot].GetValue();
		const double H = M.Terrain ? M.Terrain->HeightAt(Sp.X, Sp.Y) : 0;
		const FVector Origin(Sp.X * 100, Sp.Y * 100, H * 100);
		for (const auto& B : CampBags)
		{
			if (!BagInside(Sp.X + B[0] / 100, Sp.Y + B[1] / 100, S.Map.W * 2.0, S.Map.H * 2.0)) continue;
			Add(Bags, FTransform(FQuat(B[3], B[4], B[5], B[6]), Origin + FVector(B[0], B[1], B[2]), FVector(B[7], B[8], B[9])), FLinearColor(B[10], B[11], B[12])); BagCount++;
		}
		if (!CampSlotDrawn(Slot, S.Spawns)) continue;
		if (FlagBase)
		{
			auto* FlagMat = UMaterialInstanceDynamic::Create(FlagBase, A);
			FlagMat->SetVectorParameterValue(TEXT("Color"), ATCUnitRenderer::SeatColor(Slot));
			auto* Flag = InstanceComponent(A, FlagMesh, { FlagMat }); Models.Add(Flag);
			auto* Caster = CastWithoutReceiving(A, Flag, TCLook::HasUnitSun(M.GetWorld()));
			if (Caster) { Models.Add(Caster); Add(Caster, FTransform(Origin)); }
			Add(Flag, FTransform(Origin));
		}
		else UE_LOG(LogTC, Error, TEXT("camp models: missing M_TCHQCloth; run Tools/bridge-replay/make-lookfx-assets.py"));
		const int32 Faction = S.Factions.IsValidIndex(Slot) ? S.Factions[Slot] : 0;
		const FString Type = bTent ? TEXT("hq-camp") : TEXT("hq-camp-flagpole");
		const FTCModelEntry* Entry = Library.Find(FString::Printf(TEXT("%s|%d"), *Type, Faction));
		if (!Entry) continue;
		for (const FTCModelPart& Part : Entry->Parts)
		{
			TArray<UMaterialInterface*> Mats;
			for (const FSoftObjectPath& Source : Part.Materials)
			{
				const FString Name = TEXT("MI_TCWorld_") + Source.GetAssetName().Mid(5);
				const FString Path = FString::Printf(TEXT("/Game/World/Materials/Instances/%s.%s"), *Name, *Name);
				auto* Mat = LoadObject<UMaterialInterface>(nullptr, *Path);
				if (!Mat) UE_LOG(LogTC, Error, TEXT("camp models: missing %s; regenerate world materials including HQ camp slots"), *Path);
				Mats.Add(WithFog(Mat));
			}
			auto* C = InstanceComponent(A, Cast<UStaticMesh>(Part.Mesh.TryLoad()), Mats); Models.Add(C);
			Add(C, Part.Rest * FTransform(Origin));
		}
	}
	auto* Nodes = InstanceComponent(A, ReferenceMesh(A, NodeVertices, WoodMat), { WoodMat }); Models.Add(Nodes);
	auto* NodeCaster = CastWithoutReceiving(A, Nodes, TCLook::HasUnitSun(M.GetWorld()));
	if (NodeCaster) Models.Add(NodeCaster);
	for (const FTCResourceNode& N : M.Markers.ResourceNodes())
	{
		const double H = M.Terrain ? M.Terrain->HeightAt(N.X, N.Z) : 0;
		const FTransform At(FVector(N.X * 100, N.Z * 100, (H + 0.3) * 100));
		Add(Nodes, At);
		if (NodeCaster) Add(NodeCaster, At);
	}
	UE_LOG(LogTC, Log, TEXT("camp models: %d sandbags, %d resource boxes, tent %d; reference e4e579a"), BagCount, M.Markers.ResourceNodes().Num(), bTent);
}

#if WITH_DEV_AUTOMATION_TESTS
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCCampMaterialSectionsTest, "ThreeCrossroads.Markers.CampMaterialSections", (EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext) | EAutomationTestFlags::EngineFilter)
bool FTCCampMaterialSectionsTest::RunTest(const FString&)
{
	// Inspect the actual render sections, which survive cooking. Editor mesh descriptions and null materials
	// missed the unnamed polygon-group versus auto-named material-slot mismatch in the old camp test.
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCCampMaterials"));
	if (!TestNotNull(TEXT("camp material fixture world"), World)) return false;
	AActor* Actor = World->SpawnActor<AActor>();
	if (TestNotNull(TEXT("camp material fixture actor"), Actor))
	{
		auto* Root = NewObject<USceneComponent>(Actor);
		Actor->SetRootComponent(Root); Actor->AddInstanceComponent(Root); Root->RegisterComponent();
		auto Check = [&](const TCHAR* Name, const auto& Vertices, UMaterialInterface* Base)
		{
			if (!TestNotNull(Name, Base)) return;
			// Real play wraps the burlap/wood surfaces in dynamic fog instances.
			auto* Material = UMaterialInstanceDynamic::Create(Base, Actor);
			UStaticMesh* Mesh = ReferenceMesh(Actor, Vertices, Material);
			if (!TestNotNull(TEXT("reference runtime mesh builds with a real material"), Mesh)) return;
			TestTrue(TEXT("render section resolves the named material slot to zero"), HasCampSurfaceSlot(Mesh));
			if (!HasCampSurfaceSlot(Mesh)) return;
			const FStaticMeshLODResources& Lod = Mesh->GetRenderData()->LODResources[0];
			// The fast runtime builder pads its vertex buffer to eight channels. Check the consumed data.
			TestTrue(TEXT("source UV0 and colour scale UV3 are available"), Lod.GetNumTexCoords() >= 4);
			bool bSourceUVs = Lod.GetNumVertices() > 0, bColourScale = true;
			for (uint32 I = 0; I < Lod.GetNumVertices(); I++)
			{
				const FVector3f Position = Lod.VertexBuffers.PositionVertexBuffer.VertexPosition(I);
				const FVector2f UV = Lod.VertexBuffers.StaticMeshVertexBuffer.GetVertexUV(I, 0);
				bool bMatchesSource = false;
				for (const auto& Source : Vertices)
					if (Position.Equals(FVector3f(Source[0], Source[1], Source[2]), 0.001f) &&
						UV.Equals(FVector2f(Source[6], Source[7]), 0.001f)) { bMatchesSource = true; break; }
				bSourceUVs &= bMatchesSource;
				bColourScale &= Lod.VertexBuffers.StaticMeshVertexBuffer.GetVertexUV(I, 3).Equals(FVector2f(1, 0), 0.001f);
			}
			TestTrue(TEXT("every rendered UV0 matches an original vertex"), bSourceUVs);
			TestTrue(TEXT("every rendered UV3 retains the source colour scale"), bColourScale);
			auto* Component = InstanceComponent(Actor, Mesh, { Material });
			Add(Component, FTransform::Identity);
			UMaterialInterface* Resolved = Component->GetMaterial(Lod.Sections[0].MaterialIndex);
			TestTrue(TEXT("actual section consumes the requested dynamic material"), Resolved == Material);
			TestTrue(TEXT("resolved camp material supports instanced static meshes"), Resolved && Resolved->CheckMaterialUsage_Concurrent(MATUSAGE_InstancedStaticMeshes));
			TestTrue(TEXT("grey default material is never substituted"), Resolved != UMaterial::GetDefaultMaterial(MD_Surface));
			TestEqual(TEXT("reference instance tint custom data survives"), Component->PerInstanceSMCustomData.Num(), 6);
		};
		Check(TEXT("source burlap surface"), BagVertices, Surface(TEXT("burlap")));
		Check(TEXT("source wood surface"), NodeVertices, Surface(TEXT("wood")));
		Check(TEXT("source HQ cloth"), FlagVertices, LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Effects/Materials/M_TCHQCloth.M_TCHQCloth")));
	}
	World->DestroyWorld(false);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCCampMeshShadowsTest, "ThreeCrossroads.Markers.CampMeshShadows", (EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext) | EAutomationTestFlags::EngineFilter)
bool FTCCampMeshShadowsTest::RunTest(const FString&)
{
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCCampMeshShadows"));
	if (!TestNotNull(TEXT("camp shadow fixture world"), World)) return false;
	AActor* Actor = World->SpawnActor<AActor>();
	if (TestNotNull(TEXT("camp shadow fixture actor"), Actor))
	{
		auto* Root = NewObject<USceneComponent>(Actor);
		Actor->SetRootComponent(Root); Actor->AddInstanceComponent(Root); Root->RegisterComponent();
		auto Check = [&](const auto& Vertices, UMaterialInterface* Material)
		{
			if (!TestNotNull(TEXT("original mesh material"), Material)) return;
			UStaticMesh* Mesh = ReferenceMesh(Actor, Vertices, Material);
			if (!TestNotNull(TEXT("reference shadow mesh"), Mesh)) return;
			auto* Visible = InstanceComponent(Actor, Mesh, { Material });
			auto* Caster = CastWithoutReceiving(Actor, Visible, true);
			TestFalse(TEXT("visible source mesh does not receive scenery-channel shadows"), Visible->LightingChannels.bChannel0);
			TestTrue(TEXT("visible source mesh uses the unshadowed unit sun"), Visible->LightingChannels.bChannel1);
			TestFalse(TEXT("visible mesh delegates ground casting to its twin"), Visible->CastShadow);
			if (TestNotNull(TEXT("mesh() casts its shadow through a hidden twin"), Caster))
			{
				const FTransform At(FVector(1100, 4300, 65));
				Add(Visible, At); Add(Caster, At);
				TestTrue(TEXT("twin casts onto scenery"), Caster->CastShadow && Caster->bCastHiddenShadow && Caster->LightingChannels.bChannel0);
				TestTrue(TEXT("twin never draws a second mesh"), Caster->bHiddenInGame && !Caster->IsVisible());
				TestTrue(TEXT("twin uses the original material and geometry"), Caster->GetMaterial(0) == Material && Caster->GetStaticMesh() == Mesh);
				FTransform ShownAt, ShadowAt;
				Visible->GetInstanceTransform(0, ShownAt, true); Caster->GetInstanceTransform(0, ShadowAt, true);
				TestTrue(TEXT("ground caster has exactly the displayed source pose"), ShownAt.Equals(ShadowAt));
			}
			auto* SingleSun = InstanceComponent(Actor, Mesh, { Material });
			TestNull(TEXT("single-sun fixtures do not need a duplicate caster"), CastWithoutReceiving(Actor, SingleSun, false));
			TestTrue(TEXT("single-sun mesh still casts the source shadow"), SingleSun->CastShadow);
		};
		Check(FlagVertices, LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Effects/Materials/M_TCHQCloth.M_TCHQCloth")));
		Check(NodeVertices, Surface(TEXT("wood")));
	}
	World->DestroyWorld(false);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMarkerDepthTest, "ThreeCrossroads.Markers.DepthPolicy", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMarkerDepthTest::RunTest(const FString&)
{
	UMaterial* GroundMat = LoadObject<UMaterial>(nullptr, TEXT("/Game/Markers/M_TCMarkerGround.M_TCMarkerGround"));
	UMaterial* ThroughMat = LoadObject<UMaterial>(nullptr, TEXT("/Game/Markers/M_TCMarkerThrough.M_TCMarkerThrough"));
	UMaterial* UnitRing = LoadObject<UMaterial>(nullptr, TEXT("/Game/Models/Materials/Look/M_TCLookRing.M_TCLookRing"));
	if (!TestNotNull(TEXT("generated ground material"), GroundMat) || !TestNotNull(TEXT("generated through material"), ThroughMat) || !TestNotNull(TEXT("generated unit ring material"), UnitRing)) return false;
	TestFalse(TEXT("ground is hidden by opaque scene depth"), GroundMat->bDisableDepthTest);
	// markers.js ownerRing/selectionRing use makeOverlay's depthTest=true default. Only range rings opt out.
	TestFalse(TEXT("owner and selection rings hide behind trees and houses"), UnitRing->bDisableDepthTest);
	TestTrue(TEXT("range and faint cover layer remain visible through models"), ThroughMat->bDisableDepthTest);
	TestTrue(TEXT("ground blends in scene linear light"), GroundMat->BlendMode == BLEND_Translucent);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCCampPlacementTest, "ThreeCrossroads.Markers.CampPlacement", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCCampPlacementTest::RunTest(const FString&)
{
	FTCModelLibrary CampLibrary;
	FString LoadError;
	if (TestTrue(TEXT("source camp catalog loads"), CampLibrary.Load(LoadError)))
		for (int32 Faction = 0; Faction < 4; Faction++) for (bool bTent : { false, true })
		{
			const FString Key = FString::Printf(TEXT("%s|%d"), bTent ? TEXT("hq-camp") : TEXT("hq-camp-flagpole"), Faction);
			const FTCModelEntry* Camp = CampLibrary.Find(Key);
			if (TestNotNull(*Key, Camp))
			{
				TestEqual(TEXT("merged original camp has one part"), Camp->Parts.Num(), 1);
				if (Camp->Parts.Num()) TestEqual(TEXT("tent has canvas, timber, mast and rope surfaces; bare pole has timber and rope"), Camp->Parts[0].Materials.Num(), bTent ? 3 : 2);
			}
		}
	// main.js buildHQ and GEO.plane: no time-dependent vertex displacement at the pinned revision.
	FBox FlagBounds(ForceInit);
	for (const auto& V : FlagVertices)
	{
		FlagBounds += FVector(V[0], V[1], V[2]);
		TestTrue(TEXT("flag faces reference +Z (Unreal +Y)"), FVector(V[3], V[4], V[5]).Equals(FVector(0, 1, 0), 1e-9));
	}
	TestTrue(TEXT("HQ cloth dimensions from main.js"), FlagBounds.GetSize().Equals(FVector(450, 0, 280), 1e-4));
	TestTrue(TEXT("HQ cloth is attached 5 cm from the mast at 13 m"), FlagBounds.GetCenter().Equals(FVector(230, 0, 1300), 1e-4));
	UMaterialInterface* FlagMaterial = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Effects/Materials/M_TCHQCloth.M_TCHQCloth"));
	if (TestNotNull(TEXT("reference cloth surface loads"), FlagMaterial)) TestTrue(TEXT("reference DoubleSide cloth"), FlagMaterial->IsTwoSided());
	UTexture2D* Cloth = LoadObject<UTexture2D>(nullptr, TEXT("/Game/Effects/Textures/T_HQCloth.T_HQCloth"));
	if (TestNotNull(TEXT("exported flagMat canvas loads"), Cloth))
	{
		TestTrue(TEXT("cloth colour texture is decoded from sRGB exactly once"), Cloth->SRGB);
#if WITH_EDITORONLY_DATA
		TestEqual(TEXT("reference cloth width"), int32(Cloth->Source.GetSizeX()), 128);
		TestEqual(TEXT("reference cloth height"), int32(Cloth->Source.GetSizeY()), 64);
#endif
	}
	TestEqual(TEXT("reference sandbag instances"), int32(UE_ARRAY_COUNT(CampBags)), 472);
	TestFalse(TEXT("bag centre outside 0.75 m pad is omitted"), FTCMarkerScene::BagInside(0.74, 5, 20, 20));
	TestTrue(TEXT("bag centre at pad is kept"), FTCMarkerScene::BagInside(0.75, 19.25, 20, 20));
	TestFalse(TEXT("bag centre outside far pad is omitted"), FTCMarkerScene::BagInside(5, 19.26, 20, 20));
	for (const FString Mode : { TEXT("conquest"), TEXT("classic"), TEXT("world"), TEXT("assault"), TEXT("annihilation"), TEXT("horde") }) TestFalse(*Mode, FTCMarkerScene::TentForMode(Mode, false));
	TestTrue(TEXT("scenario uses tent"), FTCMarkerScene::TentForMode(TEXT("scenario"), false));
	TestTrue(TEXT("tutorial uses tent"), FTCMarkerScene::TentForMode(TEXT("tutorial"), false));
	const FString BeforeLobby = FTCMarkerScene::CampMode(TEXT(""), TEXT(""));
	const FString AfterLobby = FTCMarkerScene::CampMode(TEXT(""), TEXT("tutorial"));
	TestFalse(TEXT("start before lobby has no tent yet"), FTCMarkerScene::TentForMode(BeforeLobby, false));
	TestTrue(TEXT("late Tutorial lobby enables the full camp"), FTCMarkerScene::TentForMode(AfterLobby, false));
	TestNotEqual(TEXT("late lobby changes the model cache key"), BeforeLobby, AfterLobby);
	TestEqual(TEXT("known start mode is retained until lobby arrives"), FTCMarkerScene::CampMode(TEXT("scenario"), TEXT("")), FString(TEXT("scenario")));
	TestFalse(TEXT("World map suppresses tent"), FTCMarkerScene::TentForMode(TEXT("scenario"), true));
	const TArray<TOptional<FVector2D>> SharedSpawns = { FVector2D(11, 43), FVector2D(11, 43), {}, FVector2D(137, 77) };
	TestFalse(TEXT("coincident camp uses the last reference seat's paint"), FTCMarkerScene::CampSlotDrawn(0, SharedSpawns));
	TestTrue(TEXT("last coincident camp remains visible"), FTCMarkerScene::CampSlotDrawn(1, SharedSpawns));
	TestFalse(TEXT("empty spawn has no camp"), FTCMarkerScene::CampSlotDrawn(2, SharedSpawns));
	TestTrue(TEXT("separate camp remains visible"), FTCMarkerScene::CampSlotDrawn(3, SharedSpawns));
	for (const TCHAR* Name : { TEXT("burlap"), TEXT("wood"), TEXT("canvas_Painted"), TEXT("wood_Painted"), TEXT("darkwood_Painted") })
		TestNotNull(*FString::Printf(TEXT("reference camp surface %s"), Name), Surface(Name));
	UWorld* FogWorld = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCCampFog"));
	if (TestNotNull(TEXT("camp fog fixture world"), FogWorld))
	{
		ATCGameMode* Match = FogWorld->SpawnActor<ATCGameMode>();
		if (TestNotNull(TEXT("camp fog fixture match"), Match))
		{
			Match->State.Map.W = 3; Match->State.Map.H = 1;
			Match->State.FogVisible = { 1, 0, 0 }; Match->State.FogExplored = { 1, 1, 0 };
			FTCMarkerScene Camp;
			Camp.UpdateFog(*Match);
			TestTrue(TEXT("camp surfaces bind raw fog alpha: clear 0, remembered 120, unseen 185"), Camp.FogBytes == TArray<uint8>({ 0, 120, 185 }));
			if (TestNotNull(TEXT("camp alpha texture"), Camp.FogTexture.Get()))
				TestTrue(TEXT("camp fog has source bilinear filtering"), Camp.FogTexture->Filter == TF_Bilinear);
			Match->State.bFogLifted = true;
			Camp.UpdateFog(*Match);
			TestTrue(TEXT("spectator and lifted camps stay clear"), Camp.FogBytes == TArray<uint8>({ 0, 0, 0 }));
		}
		FogWorld->DestroyWorld(false);
	}
#if WITH_EDITOR
	// Check the built mesh, including its consumed index order. An extra reversal exposes the unlit inside faces.
	auto CheckFaces = [&](UStaticMesh* Mesh)
	{
		if (!TestNotNull(TEXT("reference mesh builds"), Mesh)) return;
		const FMeshDescription* D = Mesh->GetMeshDescription(0);
		if (!TestNotNull(TEXT("built reference mesh description"), D)) return;
		const FStaticMeshConstAttributes A(*D);
		const auto P = A.GetVertexPositions(); const auto N = A.GetVertexInstanceNormals();
		bool bFrontFaces = true;
		for (FTriangleID Id : D->Triangles().GetElementIDs())
		{
			const auto V = D->GetTriangleVertexInstances(Id);
			const FVector3f P0 = P[D->GetVertexInstanceVertex(V[0])], P1 = P[D->GetVertexInstanceVertex(V[1])], P2 = P[D->GetVertexInstanceVertex(V[2])];
			bFrontFaces &= FVector3f::DotProduct(FVector3f::CrossProduct(P1 - P0, P2 - P0), N[V[0]] + N[V[1]] + N[V[2]]) < 0;
		}
		TestTrue(TEXT("outward reference normals have Unreal front-face winding"), bFrontFaces);
	};
	CheckFaces(ReferenceMesh(GetTransientPackage(), BagVertices, nullptr));
	CheckFaces(ReferenceMesh(GetTransientPackage(), NodeVertices, nullptr));
	CheckFaces(ReferenceMesh(GetTransientPackage(), FlagVertices, nullptr));
#endif
	return true;
}
#endif
