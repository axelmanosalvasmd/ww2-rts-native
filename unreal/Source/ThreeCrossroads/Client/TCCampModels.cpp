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
#include "Misc/AutomationTest.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Client/TCCampModels.Data.inl"

namespace
{
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
	template <size_t N> UStaticMesh* ReferenceMesh(UObject* Outer, const double (&V)[N][8], UMaterialInterface* Mat)
	{
		FMeshDescription D; FStaticMeshAttributes Attr(D); Attr.Register();
		auto Pos = Attr.GetVertexPositions(); auto Norm = Attr.GetVertexInstanceNormals();
		auto Tangent = Attr.GetVertexInstanceTangents(); auto Sign = Attr.GetVertexInstanceBinormalSigns();
		auto Uv = Attr.GetVertexInstanceUVs(); Uv.SetNumChannels(4);
		auto Col = Attr.GetVertexInstanceColors();
		const FPolygonGroupID Group = D.CreatePolygonGroup();
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
		auto* Mesh = NewObject<UStaticMesh>(Outer); Mesh->GetStaticMaterials().Add(FStaticMaterial(Mat));
		UStaticMesh::FBuildMeshDescriptionsParams P; P.bBuildSimpleCollision = false; P.bFastBuild = true;
		if (!Mesh->BuildFromMeshDescriptions({ &D }, P)) return nullptr;
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
			Flag->SetCastShadow(false); // main.js buildHQ's flag does not cast a shadow
			if (TCLook::HasUnitSun(M.GetWorld())) Flag->SetLightingChannels(false, true, false);
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
	for (const FTCResourceNode& N : M.Markers.ResourceNodes())
	{
		const double H = M.Terrain ? M.Terrain->HeightAt(N.X, N.Z) : 0;
		Add(Nodes, FTransform(FVector(N.X * 100, N.Z * 100, (H + 0.3) * 100)));
	}
	UE_LOG(LogTC, Log, TEXT("camp models: %d sandbags, %d resource boxes, tent %d; reference e4e579a"), BagCount, M.Markers.ResourceNodes().Num(), bTent);
}

#if WITH_DEV_AUTOMATION_TESTS
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
