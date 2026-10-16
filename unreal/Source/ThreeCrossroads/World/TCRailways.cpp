// World Conquest railways (client/railways.js). The port's rules are in TCRailCore (TCRailways.h); this file draws them.
#include "World/TCRailways.h"
#include "World/TCTerrain.h"
#include "Net/TCMatchState.h"
#include "Client/TCLighting.h"
#include "World/TCFogFrontier.h"
#include "ThreeCrossroads.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/MaterialInterface.h"
#include "Engine/StaticMesh.h"
#include "Engine/Texture2D.h"
#include "Engine/World.h"
#include "GameFramework/Actor.h"
#include "MeshDescription.h"
#include "StaticMeshAttributes.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"

namespace TCRailwaysImpl
{
	// the world masters' Lambert (unreal/Tools/world/make-world-materials.py): MeshLambertMaterial's colour times the
	// instance's custom data tint, dimmed by the fog texel of its cell, as the reference's 'rail-known-fog-v1' material
	// (railways.js lit(): MeshLambertMaterial with surfaces.js fogShader)
	constexpr const TCHAR* LambertPath = TEXT("/Game/World/Materials/M_TCWorldLambert.M_TCWorldLambert");
	constexpr int32 CustomFloats = 6; // TCWorldObjects.cpp WorldFloats: tint (3), bark grey, roof ridge and slope
	constexpr double Cm = 100.0;

	const TArray<TSharedPtr<FJsonValue>>* ArrayOf(const TSharedPtr<FJsonValue>& V)
	{
		const TArray<TSharedPtr<FJsonValue>>* A = nullptr;
		return V.IsValid() && V->TryGetArray(A) ? A : nullptr;
	}
	bool Number(const TSharedPtr<FJsonValue>& V, double& Out) { return V.IsValid() && V->Type == EJson::Number && (Out = V->AsNumber(), FMath::IsFinite(Out)); }
}

// ---------------------------------------------------------------------------------------------------------------------
// The cell fog texture

bool FTCCellFogMap::Update(const FTCMatchState& State, const ATCTerrain* Terrain)
{
	const int32 MW = State.Map.W, MH = State.Map.H;
	if (MW <= 0 || MH <= 0) return false;
	bool bNew = false;
	if (!Tex.IsValid() || W != MW || H != MH)
	{
		Release();
		UTexture2D* T = UTexture2D::CreateTransient(MW, MH, PF_G8);
		if (!T) return false;
		T->AddToRoot(); // held by dynamic materials too, but written before they take it
		T->SRGB = false;
		T->Filter = TF_Bilinear; // surfaces.js uses fog.js's LinearFilter, including on roofs and tracks
		T->AddressX = TA_Clamp;
		T->AddressY = TA_Clamp;
		T->NeverStream = true;
		T->UpdateResource();
		Tex = T; W = MW; H = MH;
		Bytes.Init(0, W * H);
		bNew = true;
	}
	// Raw alpha follows the same faded state as the ground overlay.
	const bool bReveal = Terrain && Terrain->IsEndRevealed();
	bool bChanged = bNew;
	for (int32 C = 0; C < Bytes.Num(); C++)
	{
		const uint8 B = TCFogLook::AlphaByte(State, bReveal, C);
		if (Bytes[C] != B) { Bytes[C] = B; bChanged = true; }
	}
	if (!bChanged) return bNew;
	uint8* Copy = new uint8[Bytes.Num()];
	FMemory::Memcpy(Copy, Bytes.GetData(), Bytes.Num());
	FUpdateTextureRegion2D* Region = new FUpdateTextureRegion2D(0, 0, 0, 0, W, H);
	Tex->UpdateTextureRegions(0, 1, Region, W, 1, Copy, [](uint8* Data, const FUpdateTextureRegion2D* Regions) { delete[] Data; delete Regions; });
	return bNew;
}

void FTCCellFogMap::Release()
{
	if (UTexture2D* T = Tex.Get()) T->RemoveFromRoot();
	Tex.Reset(); Bytes.Reset(); W = H = 0;
}

// ---------------------------------------------------------------------------------------------------------------------

bool FTCRailways::Attach(UWorld* World)
{
	if (Actor.IsValid()) return true;
	if (!World) return false;
	FActorSpawnParameters Params;
	Params.ObjectFlags |= RF_Transient;
	AActor* A = World->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity, Params);
	if (!A) return false;
	USceneComponent* Root = NewObject<USceneComponent>(A, TEXT("Railways"));
	Root->SetMobility(EComponentMobility::Movable);
	A->SetRootComponent(Root);
	A->AddInstanceComponent(Root);
	Root->RegisterComponent();
	Actor = A;
	return true;
}

void FTCRailways::Detach()
{
	if (AActor* A = Actor.Get()) A->Destroy();
	Actor.Reset();
	Batches.Reset(); Materials.Reset(); BoxMesh.Reset(); CylinderMesh.Reset();
	Fog.Release();
	bOn = false;
}

FTransform FTCRailways::ToUnreal(const TCRailCore::FMat4& M)
{
	// three (x, y up, z) -> Unreal (x, z, y): a mesh built with the same swap keeps its local axes, so the matrix is
	// P M P^-1. Its columns are the images of three's local axes; Unreal's local X, Y, Z are three's x, z, y.
	const double* E = M.E;
	const FVector X(E[0], E[2], E[1]), Y(E[8], E[10], E[9]), Z(E[4], E[6], E[5]), O(E[12] * TCRailwaysImpl::Cm, E[14] * TCRailwaysImpl::Cm, E[13] * TCRailwaysImpl::Cm);
	return FTransform(FMatrix(X, Y, Z, O));
}

UStaticMesh* FTCRailways::BuildMesh(UObject* Outer, bool bCylinder, UMaterialInterface* Material)
{
	std::vector<double> Pos, Nrm;
	std::vector<int> Idx;
	if (bCylinder) TCRailCore::Cylinder(Pos, Nrm, Idx); else TCRailCore::Box(Pos, Nrm, Idx);
	FMeshDescription Desc;
	FStaticMeshAttributes Attributes(Desc);
	Attributes.Register();
	TVertexAttributesRef<FVector3f> Positions = Attributes.GetVertexPositions();
	TVertexInstanceAttributesRef<FVector3f> Normals = Attributes.GetVertexInstanceNormals();
	TVertexInstanceAttributesRef<FVector4f> Colors = Attributes.GetVertexInstanceColors();
	TVertexInstanceAttributesRef<FVector2f> UVs = Attributes.GetVertexInstanceUVs();
	// the world Lambert multiplies by the vertex colour and by UV3.x (the importer's colour scale): white and 1
	UVs.SetNumChannels(4);
	const FPolygonGroupID Group = Desc.CreatePolygonGroup();
	Attributes.GetPolygonGroupMaterialSlotNames()[Group] = TEXT("Rail");
	TArray<FVertexInstanceID> Inst;
	for (size_t I = 0; I < Pos.size() / 3; I++)
	{
		const FVertexID V = Desc.CreateVertex();
		Positions[V] = FVector3f((float)(Pos[I * 3] * 100.0), (float)(Pos[I * 3 + 2] * 100.0), (float)(Pos[I * 3 + 1] * 100.0));
		const FVertexInstanceID VI = Desc.CreateVertexInstance(V);
		Normals[VI] = FVector3f((float)Nrm[I * 3], (float)Nrm[I * 3 + 2], (float)Nrm[I * 3 + 1]);
		Colors[VI] = FVector4f(1, 1, 1, 1);
		for (int32 K = 0; K < 3; K++) UVs.Set(VI, K, FVector2f(0.f, 0.f));
		UVs.Set(VI, 3, FVector2f(1.f, 0.f));
		Inst.Add(VI);
	}
	// three.js counter-clockwise faces stay front faces through the axis swap with the same index order
	for (size_t I = 0; I + 2 < Idx.size(); I += 3) Desc.CreateTriangle(Group, { Inst[Idx[I]], Inst[Idx[I + 1]], Inst[Idx[I + 2]] });
	UStaticMesh* Mesh = NewObject<UStaticMesh>(Outer, bCylinder ? TEXT("SM_TCRailTower") : TEXT("SM_TCRailBox"));
	Mesh->GetStaticMaterials().Add(FStaticMaterial(Material, TEXT("Rail")));
	UStaticMesh::FBuildMeshDescriptionsParams Params;
	Params.bBuildSimpleCollision = false;
	Params.bFastBuild = true;
	Params.bMarkPackageDirty = false;
	if (!Mesh->BuildFromMeshDescriptions({ &Desc }, Params)) return nullptr;
	// run-time meshes carry no texture-streaming UV data (slice 11 lead: the cooked game's UVChannelData ensure)
	for (FStaticMaterial& M : Mesh->GetStaticMaterials()) M.UVChannelData = FMeshUVChannelInfo(1.f);
	return Mesh;
}

UInstancedStaticMeshComponent* FTCRailways::Batch(int32 Mat, bool bCylinder)
{
	const int32 Key = Mat * 2 + (bCylinder ? 1 : 0);
	if (TWeakObjectPtr<UInstancedStaticMeshComponent>* Found = Batches.Find(Key)) if (Found->IsValid()) return Found->Get();
	AActor* A = Actor.Get();
	if (!A) return nullptr;
	if (Materials.Num() != TCRailCore::MatCount)
	{
		Materials.Reset();
		UMaterialInterface* Master = LoadObject<UMaterialInterface>(nullptr, TCRailwaysImpl::LambertPath);
		if (!Master) { UE_LOG(LogTC, Error, TEXT("railways: %s is missing (unreal/Tools/world/make-world-materials.py); no railway is drawn"), TCRailwaysImpl::LambertPath); return nullptr; }
		for (int32 M = 0; M < TCRailCore::MatCount; M++)
		{
			UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(Master, A, FName(*FString::Printf(TEXT("MID_TCRail_%hs"), TCRailCore::MatName[M])));
			Mid->SetVectorParameterValue(TEXT("MaterialColor"), TCLook::HexLinear(TCRailCore::MatHex[M]));
			Mid->SetScalarParameterValue(TEXT("MapOn"), 0.f);
			Mid->SetScalarParameterValue(TEXT("UseGrey"), 0.f);
			Mid->SetScalarParameterValue(TEXT("RoofUV"), 0.f);
			if (UTexture2D* T = Fog.Texture())
			{
				Mid->SetTextureParameterValue(TEXT("FogMap"), T);
				Mid->SetVectorParameterValue(TEXT("FogSize"), Fog.Size());
				Mid->SetScalarParameterValue(TEXT("FogOn"), 1.f);
			}
			Materials.Emplace(Mid);
		}
	}
	TStrongObjectPtr<UStaticMesh>& Slot = bCylinder ? CylinderMesh : BoxMesh;
	if (!Slot) Slot.Reset(BuildMesh(A, bCylinder, Materials[Mat].Get()));
	if (!Slot) { UE_LOG(LogTC, Error, TEXT("railways: the %s mesh could not be built"), bCylinder ? TEXT("water tower") : TEXT("box")); return nullptr; }
	UInstancedStaticMeshComponent* C = NewObject<UInstancedStaticMeshComponent>(A, FName(*FString::Printf(TEXT("Rail_%hs%s"), TCRailCore::MatName[Mat], bCylinder ? TEXT("_tower") : TEXT(""))));
	C->SetStaticMesh(Slot.Get());
	C->SetMaterial(0, Materials[Mat].Get());
	C->SetNumCustomDataFloats(TCRailwaysImpl::CustomFloats);
	C->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	C->SetCanEverAffectNavigation(false);
	// the track's InstancedMeshes receive shadows and cast none; the station's parts cast and receive
	const bool bTrack = Mat == TCRailCore::Ballast || Mat == TCRailCore::Sleeper || Mat == TCRailCore::Rail;
	C->SetCastShadow(!bTrack);
	C->SetupAttachment(A->GetRootComponent());
	A->AddInstanceComponent(C);
	C->RegisterComponent();
	Batches.Add(Key, C);
	return C;
}

void FTCRailways::Reset(const FTCMatchState& State)
{
	ClearDrawn();
	Model.Reset(State.Map.W);
	Centres.Reset();
	MapCells = State.Map.W * State.Map.H;
	bOn = State.Map.bWorld && MapCells > 0;
	LoggedRebuilds = LoggedStations = 0;
	UE_LOG(LogTC, Log, TEXT("railways: %s (reference %hs)"), bOn ? TEXT("World map, explored track and stations are drawn") : TEXT("not a World map, none"), TCRailCore::ReferencePin);
}

void FTCRailways::ClearDrawn()
{
	for (TPair<int32, TWeakObjectPtr<UInstancedStaticMeshComponent>>& B : Batches) if (UInstancedStaticMeshComponent* C = B.Value.Get()) C->ClearInstances();
}

void FTCRailways::ApplyCells(const FJsonObject& Message)
{
	using namespace TCRailwaysImpl;
	if (!bOn) return;
	const TArray<TSharedPtr<FJsonValue>>* Rows = nullptr;
	if (!Message.TryGetArrayField(TEXT("cells"), Rows) || !Rows) return;
	for (const TSharedPtr<FJsonValue>& Row : *Rows)
	{
		const TArray<TSharedPtr<FJsonValue>>* R = ArrayOf(Row);
		double C;
		if (!R || R->Num() < 5 || !Number((*R)[0], C) || C < 0 || C >= (double)MapCells) continue;
		const TSharedPtr<FJsonObject>* Data = nullptr;
		if (!(*R)[4].IsValid() || !(*R)[4]->TryGetObject(Data) || !Data || !Data->IsValid()) continue;
		double Rail = 0, Station = 0;
		const bool bRail = Number((*Data)->TryGetField(TEXT("rail")), Rail) && Rail != 0; // data?.rail (truthy)
		const TSharedPtr<FJsonValue> St = (*Data)->TryGetField(TEXT("station"));
		const bool bStation = St.IsValid() && St->Type != EJson::None;                    // data?.station !== undefined
		if (bStation) Number(St, Station);
		if (bRail || bStation) Model.Cell_((int)C, bRail ? (int)Rail : 0, bStation, (int)Station);
	}
}

void FTCRailways::ApplySnapshot(const FJsonObject& Snapshot)
{
	using namespace TCRailwaysImpl;
	if (!bOn) return;
	// center: id => lastSnap?.world?.regions?.find(r => r.id === id)
	Centres.Reset();
	const TSharedPtr<FJsonObject>* World = nullptr;
	if (!Snapshot.TryGetObjectField(TEXT("world"), World) || !World || !World->IsValid()) return;
	const TArray<TSharedPtr<FJsonValue>>* Regions = nullptr;
	if (!(*World)->TryGetArrayField(TEXT("regions"), Regions) || !Regions) return;
	for (const TSharedPtr<FJsonValue>& V : *Regions)
	{
		const TSharedPtr<FJsonObject>* O = nullptr;
		if (!V.IsValid() || !V->TryGetObject(O) || !O) continue;
		double Id, X, Z;
		if (Number((*O)->TryGetField(TEXT("id")), Id) && Number((*O)->TryGetField(TEXT("x")), X) && Number((*O)->TryGetField(TEXT("z")), Z) && !Centres.Contains((int32)Id))
			Centres.Add((int32)Id, FVector2D(X, Z));
	}
}

void FTCRailways::Frame(float Dt, const FTCMatchState& State, const ATCTerrain* Terrain)
{
	if (!bOn || !Terrain) return;
	if (Fog.Update(State, Terrain))
		for (const TStrongObjectPtr<UMaterialInstanceDynamic>& M : Materials)
			if (UMaterialInstanceDynamic* Mid = M.Get()) { Mid->SetTextureParameterValue(TEXT("FogMap"), Fog.Texture()); Mid->SetVectorParameterValue(TEXT("FogSize"), Fog.Size()); Mid->SetScalarParameterValue(TEXT("FogOn"), 1.f); }
	const size_t Before = Model.BuiltStations.size();
	bool bRebuilt = false;
	auto Height = [Terrain](double X, double Z) { return (double)Terrain->HeightAt((float)X, (float)Z); };
	auto Centre = [this](int Region, double& X, double& Z) { const FVector2D* C = Centres.Find(Region); if (!C) return false; X = C->X; Z = C->Y; return true; };
	if (!Model.Frame(Dt, Height, Centre, &bRebuilt)) return;
	if (bRebuilt) DrawTrack();
	for (size_t I = Before; I < Model.BuiltStations.size(); I++) DrawStation(Model.BuiltStations[I]);
	if (Model.Rebuilds != LoggedRebuilds && (Model.Rebuilds <= 3 || Model.Rebuilds % 20 == 0))
		UE_LOG(LogTC, Log, TEXT("railways: %s"), *Describe());
	LoggedRebuilds = Model.Rebuilds;
	if ((int32)Model.BuiltStations.size() != LoggedStations)
	{
		LoggedStations = (int32)Model.BuiltStations.size();
		const TCRailCore::FStation& S = Model.BuiltStations.back();
		UE_LOG(LogTC, Log, TEXT("railways: station of region %d built at (%.1f, %.1f), yaw %.2f; %s"), S.Region, S.X, S.Z, S.Yaw, *Describe());
	}
}

void FTCRailways::DrawTrack()
{
	auto Fill = [this](int32 Mat, const std::vector<TCRailCore::FMat4>& Ms)
	{
		UInstancedStaticMeshComponent* C = Batch(Mat, false);
		if (!C) return;
		C->ClearInstances();
		TArray<FTransform> Ts;
		Ts.Reserve(Ms.size());
		for (const TCRailCore::FMat4& M : Ms) Ts.Add(ToUnreal(M));
		C->AddInstances(Ts, false, false, false);
		const float Ones[TCRailwaysImpl::CustomFloats] = { 1, 1, 1, 1, 1, 1 };
		for (int32 I = 0; I < Ts.Num(); I++) C->SetCustomData(I, TArrayView<const float>(Ones, TCRailwaysImpl::CustomFloats), false);
		C->MarkRenderStateDirty();
	};
	Fill(TCRailCore::Ballast, Model.BallastM);
	Fill(TCRailCore::Sleeper, Model.TiesM);
	Fill(TCRailCore::Rail, Model.RailsM);
}

void FTCRailways::DrawStation(const TCRailCore::FStation& St)
{
	const float Ones[TCRailwaysImpl::CustomFloats] = { 1, 1, 1, 1, 1, 1 };
	for (const TCRailCore::FPart& P : St.Parts)
		if (UInstancedStaticMeshComponent* C = Batch(P.Mat, P.bCylinder))
		{
			const int32 I = C->AddInstance(ToUnreal(P.M), true);
			C->SetCustomData(I, TArrayView<const float>(Ones, TCRailwaysImpl::CustomFloats), true);
		}
}

bool FTCRailways::FirstStation(FVector2D& Out) const
{
	if (Model.BuiltStations.empty()) return false;
	Out = FVector2D(Model.BuiltStations[0].X, Model.BuiltStations[0].Z);
	return true;
}

bool FTCRailways::TrackCentre(FVector2D& Out) const
{
	if (Model.TrackOrder.empty()) return false;
	const int C = Model.TrackOrder[Model.TrackOrder.size() / 2];
	Out = FVector2D(TCRailCore::FModel::CellX(C, Model.W), TCRailCore::FModel::CellZ(C, Model.W));
	return true;
}

FString FTCRailways::Describe() const
{
	return FString::Printf(TEXT("track %d cells, %d halves, %d sleepers, %d rails; %d of %d stations built%s; %d rebuilds"), (int32)Model.TrackOrder.size(),
		(int32)Model.BallastM.size(), (int32)Model.TiesM.size(), (int32)Model.RailsM.size(), (int32)Model.BuiltStations.size(), (int32)Model.StationOrder.size(),
		Model.Waiting ? TEXT(" (waiting for a town)") : TEXT(""), Model.Rebuilds);
}

int32 FTCRailways::InstancesDrawn() const
{
	int32 N = 0;
	for (const TPair<int32, TWeakObjectPtr<UInstancedStaticMeshComponent>>& B : Batches) if (const UInstancedStaticMeshComponent* C = B.Value.Get()) N += C->GetInstanceCount();
	return N;
}
