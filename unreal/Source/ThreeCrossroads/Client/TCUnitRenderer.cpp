#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "World/TCTerrain.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Engine/StaticMesh.h"
#include "Engine/World.h"
#include "GameFramework/PlayerController.h"
#include "Camera/PlayerCameraManager.h"
#include "UObject/ConstructorHelpers.h"

// The engine's basic shapes are 100 cm across (cylinder and cone 100 cm tall, centred; the cone's apex is at +Z).

bool ATCUnitRenderer::IsDrawn(const FTCUnit& U)
{
	return !(U.Flags & (TCFlags::Garrisoned | TCFlags::Riding | TCClientFlags::PlaneAtBase));
}

float ATCUnitRenderer::LiftMetres(const FTCUnitDef* Def)
{
	return Def && Def->bAir ? AirAltitude : 0.f;
}

ATCUnitRenderer::ATCUnitRenderer()
{
	PrimaryActorTick.bCanEverTick = false;
	RootComponent = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cyl(TEXT("/Engine/BasicShapes/Cylinder.Cylinder"));
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cube(TEXT("/Engine/BasicShapes/Cube.Cube"));
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cone(TEXT("/Engine/BasicShapes/Cone.Cone"));
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Mat(TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	CylinderMesh = Cyl.Object; CubeMesh = Cube.Object; ConeMesh = Cone.Object; BaseMaterial = Mat.Object;
}

FLinearColor ATCUnitRenderer::SeatColor(int32 Owner)
{
	// client/main.js COLORS: blue, red, chalk, orange, violet, cyan; anything else 0xdddddd
	static const FColor Colors[] = { FColor(0x3b, 0x73, 0xd6), FColor(0xcc, 0x3a, 0x2e), FColor(0xec, 0xe6, 0xd6), FColor(0xe2, 0x83, 0x2b), FColor(0x9b, 0x5c, 0xd4), FColor(0x35, 0xb6, 0xc0) };
	return FLinearColor(Owner >= 0 && Owner < 6 ? Colors[Owner] : FColor(0xdd, 0xdd, 0xdd));
}

UInstancedStaticMeshComponent* ATCUnitRenderer::Batch(int32 Owner, EKind Kind)
{
	const int32 Key = (FMath::Clamp(Owner, -1, 62) + 2) * KindCount + Kind;
	if (TObjectPtr<UInstancedStaticMeshComponent>* Found = Batches.Find(Key)) return *Found;
	UInstancedStaticMeshComponent* Ism = NewObject<UInstancedStaticMeshComponent>(this);
	Ism->SetStaticMesh(Kind == Man || Kind == Ring ? CylinderMesh : Kind == Air ? ConeMesh : CubeMesh);
	Ism->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	Ism->SetCastShadow(Kind != Ring);
	Ism->SetMobility(EComponentMobility::Movable);
	Ism->SetupAttachment(RootComponent);
	Ism->RegisterComponent();
	if (BaseMaterial)
	{
		UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(BaseMaterial, this);
		Mid->SetVectorParameterValue(TEXT("Color"), Kind == Ring ? FLinearColor(0.55f, 1.f, 0.45f) : SeatColor(Owner));
		Ism->SetMaterial(0, Mid);
	}
	Batches.Add(Key, Ism);
	Owned.Add(Ism);
	return Ism;
}

bool ATCUnitRenderer::DrawnAt(int32 Id, FVector2D& Out) const
{
	const FShown* S = Shown.Find(Id);
	if (!S) return false;
	Out = FVector2D(S->X, S->Z);
	return true;
}

void ATCUnitRenderer::Draw(const FTCMatchState& State, const FTCUnitTable& Table, const ATCTerrain* Terrain, const TSet<int32>& Selected, float DeltaSeconds)
{
	EnsureModels();
	UpdateRingLooks();
	TMap<UInstancedStaticMeshComponent*, TArray<FTransform>> Frames;
	for (const auto& Pair : Batches) Frames.Add(Pair.Value, {});
	Frame.Reset();
	Stats = FTCUnitModelStats();
	const float Ease = 1.f - FMath::Exp(-DeltaSeconds * 10.f);
	// where the camera is, for the soldiers' level of detail (unit-models.js animate eye)
	FVector Eye = FVector(0, 0, 1e9);
	if (ViewOverride.IsSet()) Eye = ViewOverride.GetValue();
	else if (const UWorld* W = GetWorld())
		if (const APlayerController* PC = W->GetFirstPlayerController())
			if (PC->PlayerCameraManager) Eye = PC->PlayerCameraManager->GetCameraLocation();
	if (Crowd) Crowd->BeginFrame();
	TSet<int32> Present;
	for (const auto& Pair : State.Units)
	{
		const FTCUnit& U = Pair.Value;
		Present.Add(U.Id);
		FShown& S = Shown.FindOrAdd(U.Id, FShown{ U.X, U.Z, U.Rot, U.Aim });
		if (FMath::Square(S.X - U.X) + FMath::Square(S.Z - U.Z) > 30.f * 30.f) { const bool bFar = S.bFar; S = FShown{ U.X, U.Z, U.Rot, U.Aim }; S.bFar = bFar; } // a jump: no glide across the map
		S.X += (U.X - S.X) * Ease; S.Z += (U.Z - S.Z) * Ease;
		S.Rot += FMath::FindDeltaAngleRadians(S.Rot, U.Rot) * Ease;
		// main.js: v.aim = lerpAngle(v.aim, v.taim, k * 0.6)
		if (!S.bAimSet) { S.Aim = U.Aim; S.bAimSet = true; }
		S.Aim += FMath::FindDeltaAngleRadians(S.Aim, U.Aim) * Ease * 0.6f;
		if (!IsDrawn(U)) continue; // inside a building or a carrier, or a plane parked at its base (client/main.js)
		const FTCUnitDef* Def = Table.Find(U.Type);
		if (UsesModel(U.Type) && Library.IsLoaded() && DrawModel(State, U, Def, S, Terrain, Selected.Contains(U.Id), DeltaSeconds, Eye)) { Stats.ModelUnits++; continue; }
		if (UsesModel(U.Type)) Stats.MissingModel++;
		Stats.PlaceholderUnits++;
		const float Radius = Def ? Def->Radius : 1.5f;
		const float Ground = Terrain ? Terrain->HeightAt(S.X, S.Z) : 0.f;
		const FQuat Facing(FVector::UpVector, S.Rot);
		const FVector At = ATCTerrain::ToWorld(S.X, S.Z, Ground);
		if (Def && Def->bInfantry)
		{
			// the men still standing, in ranks of three across the facing (presentation only)
			const int32 Alive = U.Flags & TCFlags::Operative ? 1 : Def->HpPer > 0 ? FMath::Clamp(FMath::DivideAndRoundUp(U.Hp, Def->HpPer), 1, Def->Models) : Def->Models;
			for (int32 I = 0; I < Alive; I++)
			{
				const float Across = ((I % 3) - 1) * 1.3f, Behind = (I / 3) * 1.3f;
				const FVector Local(-Behind * 100.f, Across * 100.f, 85.f);
				Frames.FindOrAdd(Batch(U.Owner, Man)).Add(FTransform(Facing, At + Facing.RotateVector(Local), FVector(0.45f, 0.45f, 1.7f)));
			}
		}
		else if (Def && Def->bAir)
			// +90 degrees about +Y turns the cone's apex from +Z to +X, the heading every other unit faces along
			Frames.FindOrAdd(Batch(U.Owner, Air)).Add(FTransform(FQuat(FVector::UpVector, S.Rot) * FQuat(FVector::RightVector, PI / 2), At + FVector(0, 0, LiftMetres(Def) * 100.f), FVector(Radius, Radius, Radius * 1.6f)));
		else if (Def && Def->bStructure)
			Frames.FindOrAdd(Batch(U.Owner, Structure)).Add(FTransform(Facing, At + FVector(0, 0, 200.f * FMath::Max(0.15f, U.Built)), FVector(Radius * 2.f, Radius * 2.f, 4.f * FMath::Max(0.15f, U.Built))));
		else
			Frames.FindOrAdd(Batch(U.Owner, Vehicle)).Add(FTransform(Facing, At + FVector(0, 0, 110.f), FVector(Radius * 2.f, Radius * 1.3f, 2.2f)));
		if (Selected.Contains(U.Id))
			Frames.FindOrAdd(Batch(-1, Ring)).Add(FTransform(FQuat::Identity, At + FVector(0, 0, 5.f), FVector(Radius * 2.f + 1.f, Radius * 2.f + 1.f, 0.05f)));
	}
	for (auto It = Shown.CreateIterator(); It; ++It) if (!Present.Contains(It.Key())) It.RemoveCurrent();
	for (auto It = Squads.CreateIterator(); It; ++It)
	{
		if (Present.Contains(It.Key())) continue;
		// main.js removeUnit: a squad killed outright drops its men where they stood; one that left sight just goes
		if (Crowd) Crowd->SquadGone(It.Key(), KilledIds.Contains(It.Key()));
		It.RemoveCurrent();
	}
	KilledIds.Reset(); // noted the frame they left (FTCCombatEffects::TakeKilledGone), so used now or never
	UnitMotion.Retain(Present);
	LastInstances = 0;
	for (auto& Pair : Frames)
	{
		UInstancedStaticMeshComponent* Ism = Pair.Key;
		LastInstances += Pair.Value.Num();
		if (Ism->GetInstanceCount() == Pair.Value.Num()) { if (Pair.Value.Num()) Ism->BatchUpdateInstancesTransforms(0, Pair.Value, true, true, true); }
		else { Ism->ClearInstances(); if (Pair.Value.Num()) Ism->AddInstances(Pair.Value, false, true); }
	}
	FlushModelFrame();
	if (Crowd)
	{
		// the soldiers and the bodies (unit-models.js createBodies: age, fade, sink, near or far each by its own distance)
		Crowd->EndFrame(DeltaSeconds, Eye, [Terrain](double X, double Y) { return Terrain ? Terrain->HeightAt(X / 100.0, Y / 100.0) * 100.0 : 0.0; }, LodMetres);
		const FTCSoldierCrowdStats& C = Crowd->Stats();
		Stats.Corpses = C.Corpses;
		Stats.Figures = C.NearFigures + C.FarFigures;
		Stats.Components += C.Components;
		LastInstances += C.NearMen + C.FarMen + C.Corpses;
	}
	ReportFrame();
}
