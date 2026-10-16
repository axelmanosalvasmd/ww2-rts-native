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
#include "Client/TCGraphics.h"
#include "Engine/Texture2D.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Misc/App.h"
#include "HAL/PlatformTime.h"

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

void ATCUnitRenderer::Draw(const FTCMatchState& State, const FTCUnitTable& Table, const ATCTerrain* Terrain, const TSet<int32>& Selected, float DeltaSeconds, const TArray<int32>* SnapshotOrder)
{
	// A cooked headless client cannot initialize the ring meshes' render resources.
	if (!FApp::CanEverRender()) return;
	EnsureModels();
	PrepareTrenchSeats(State, Table, SnapshotOrder);
	UpdateRingLooks();
	TMap<UInstancedStaticMeshComponent*, TArray<FTransform>> Frames;
	TMap<UInstancedStaticMeshComponent*, TArray<uint64>> FrameKeys; // FTCInstanceHistory keys, by placeholder batch
	for (const auto& Pair : Batches) Frames.Add(Pair.Value, {});
	History.ResetCounts();
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
	DrawnList.Reset();
	DrawnMenList.Reset();
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
		if (UsesModel(U.Type) && Library.IsLoaded() && DrawModel(State, U, Def, S, Terrain, Selected.Contains(U.Id), DeltaSeconds, Eye))
		{
			Stats.ModelUnits++;
			if (Stats.ModelUnits <= 60) DrawnList += FString::Printf(TEXT("%s%d %s %d %.1f %.1f %s"), DrawnList.IsEmpty() ? TEXT("") : TEXT(", "), U.Id, *U.Type, U.Owner, S.X, S.Z, S.bFar ? TEXT("far") : TEXT("near"));
			continue;
		}
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
				FrameKeys.FindOrAdd(Batch(U.Owner, Man)).Add(FTCInstanceHistory::ManKey(U.Id, I));
			}
		}
		else if (Def && Def->bAir)
		{
			FrameKeys.FindOrAdd(Batch(U.Owner, Air)).Add(FTCInstanceHistory::PartKey(U.Id, 0));
			// +90 degrees about +Y turns the cone's apex from +Z to +X, the heading every other unit faces along
			Frames.FindOrAdd(Batch(U.Owner, Air)).Add(FTransform(FQuat(FVector::UpVector, S.Rot) * FQuat(FVector::RightVector, PI / 2), At + FVector(0, 0, LiftMetres(Def) * 100.f), FVector(Radius, Radius, Radius * 1.6f)));
		}
		else if (Def && Def->bStructure)
		{
			FrameKeys.FindOrAdd(Batch(U.Owner, Structure)).Add(FTCInstanceHistory::PartKey(U.Id, 0));
			Frames.FindOrAdd(Batch(U.Owner, Structure)).Add(FTransform(Facing, At + FVector(0, 0, 200.f * FMath::Max(0.15f, U.Built)), FVector(Radius * 2.f, Radius * 2.f, 4.f * FMath::Max(0.15f, U.Built))));
		}
		else
		{
			FrameKeys.FindOrAdd(Batch(U.Owner, Vehicle)).Add(FTCInstanceHistory::PartKey(U.Id, 0));
			Frames.FindOrAdd(Batch(U.Owner, Vehicle)).Add(FTransform(Facing, At + FVector(0, 0, 110.f), FVector(Radius * 2.f, Radius * 1.3f, 2.2f)));
		}
		if (Selected.Contains(U.Id))
		{
			FrameKeys.FindOrAdd(Batch(-1, Ring)).Add(FTCInstanceHistory::RingKey(U.Id, true));
			Frames.FindOrAdd(Batch(-1, Ring)).Add(FTransform(FQuat::Identity, At + FVector(0, 0, 5.f), FVector(Radius * 2.f + 1.f, Radius * 2.f + 1.f, 0.05f)));
		}
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
		const TArray<uint64>* Keys = FrameKeys.Find(Ism);
		History.Write(Ism, Pair.Value, Keys ? *Keys : TArray<uint64>());
		if (Pair.Value.Num()) Ism->MarkRenderStateDirty();
	}
	FlushModelFrame();
	if (Crowd)
	{
		// the soldiers and the bodies (unit-models.js createBodies: age, fade, sink, near or far each by its own distance)
		Crowd->EndFrame(DeltaSeconds, Eye, [Terrain](double X, double Y) { return Terrain ? Terrain->HeightAt(X / 100.0, Y / 100.0) * 100.0 : 0.0; }, CurrentLodMetres());
		const FTCSoldierCrowdStats& C = Crowd->Stats();
		Stats.Corpses = C.Corpses;
		Stats.Figures = C.NearFigures + C.FarFigures;
		Stats.Components += C.Components;
		LastInstances += C.NearMen + C.FarMen + C.Corpses;
	}
	Stats.Matched = History.Matched + (Crowd ? Crowd->History.Matched : 0);
	Stats.Moving = History.Moving + (Crowd ? Crowd->History.Moving : 0);
	ReportFrame();
	// every 5 s of real time while tracked hulls are drawn: how many turn their wheels and how far they have rolled
	const double Real = FPlatformTime::Seconds();
	if (Stats.WheelHulls && Real - WheelLogAt >= 5.0)
	{
		WheelLogAt = Real;
		UE_LOG(LogTemp, Log, TEXT("wheels and tracks: %d hulls drawn turning their wheels, the furthest rolled %.2f m"), Stats.WheelHulls, Stats.WheelTravelMax);
	}
}

float ATCUnitRenderer::CurrentLodMetres()
{
	return FTCGraphics::Get().Budget().LodMetres;
}

namespace TCWheelTracksImpl
{
	DEFINE_LOG_CATEGORY_STATIC(LogTCWheels, Log, All);
	// -TCWheelTravel=<metres>,<radians>: added to every hull's travel and turn (a verification switch: the comparison
	// pictures show the wheels and tracks of a still hull at a known travel, as the reference's wheelTravel attribute set
	// by hand). -TCNoWheelTracks draws the catalog hulls (no motion).
	struct FSwitches { bool bOff = false; double Travel = 0, Turn = 0; };
	const FSwitches& Switches()
	{
		static const FSwitches S = []
		{
			FSwitches Out;
			Out.bOff = FParse::Param(FCommandLine::Get(), TEXT("TCNoWheelTracks"));
			FString Arg;
			if (FParse::Value(FCommandLine::Get(), TEXT("-TCWheelTravel="), Arg, false)) // the comma is part of the value
			{
				FString A, B;
				if (!Arg.Split(TEXT(","), &A, &B)) A = Arg;
				Out.Travel = FCString::Atod(*A);
				Out.Turn = B.IsEmpty() ? 0.0 : FCString::Atod(*B);
			}
			return Out;
		}();
		return S;
	}
}

bool ATCUnitRenderer::ApplyWheelTracks(UInstancedStaticMeshComponent* Ism, const FTCModelPart& Part, bool bFaded)
{
	using namespace TCWheelTracksImpl;
	if (!Ism || bFaded || Switches().bOff) return false;
	if (!bWheelTracksTried)
	{
		bWheelTracksTried = true;
		if (WheelTracks.Load(WheelTracksWhy))
		{
			const FString Extra = Switches().Travel || Switches().Turn ? FString::Printf(TEXT("; -TCWheelTravel adds %.3f m and %.3f rad"), Switches().Travel, Switches().Turn) : FString();
			UE_LOG(LogTCWheels, Log, TEXT("wheels and tracks: %d hulls, %d loops in the path table%s"), WheelTracks.NumHulls(), WheelTracks.Paths().Num(), *Extra);
		}
		else
		{
			UE_LOG(LogTCWheels, Warning, TEXT("wheels and tracks: off (%s)"), *WheelTracksWhy);
		}
	}
	const FTCWheelTracks::FHull* Hull = WheelTracks.IsLoaded() ? WheelTracks.Find(Part.Mesh) : nullptr;
	if (!Hull) return false;
	UStaticMesh* Mesh = Cast<UStaticMesh>(Hull->Mesh.TryLoad());
	if (!Mesh || Mesh->GetStaticMaterials().Num() != Hull->Materials.Num())
	{
		UE_LOG(LogTCWheels, Warning, TEXT("wheels and tracks: %s does not load as the copy of %s; the catalog hull is drawn"), *Hull->Mesh.ToString(), *Part.Mesh.ToString());
		return false;
	}
	UTexture2D* Paths = WheelTracks.Texture();
	if (!Paths) return false;
	ModelObjects.AddUnique(Paths);
	TArray<UMaterialInterface*> Mats;
	for (const FSoftObjectPath& P : Hull->Materials)
	{
		const FString Key = P.ToString();
		UMaterialInstanceDynamic* Mid = nullptr;
		if (const TObjectPtr<UMaterialInstanceDynamic>* Found = WheelMids.Find(Key)) Mid = *Found;
		else if (UMaterialInterface* Base = Cast<UMaterialInterface>(P.TryLoad()))
		{
			Mid = UMaterialInstanceDynamic::Create(Base, this);
			Mid->SetTextureParameterValue(FTCWheelTracks::TextureParameter, Paths);
			WheelMids.Add(Key, Mid);
			ModelObjects.Add(Mid);
		}
		if (!Mid) return false;
		Mats.Add(Mid);
	}
	Ism->SetStaticMesh(Mesh);
	for (int32 I = 0; I < Mats.Num(); I++) Ism->SetMaterial(I, Mats[I]);
	Ism->SetNumCustomDataFloats(FTCWheelTracks::Floats);
	return true;
}

void ATCUnitRenderer::AppendPartData(FFrameBatch& B, const UInstancedStaticMeshComponent* Ism, const TArray<float>& Custom, const FTCPartMotion& Motion)
{
	const int32 Floats = Ism ? Ism->NumCustomDataFloats : Custom.Num();
	B.Floats = Floats;
	B.Data.Append(Custom);
	if (Floats == FTCWheelTracks::Floats && Custom.Num() == FTCWheelTracks::TravelFloat)
	{
		// wheel-motion.js moveWheels: the instance's wheelTravel (metres rolled, radians turned since first drawn)
		const TCWheelTracksImpl::FSwitches& Sw = TCWheelTracksImpl::Switches();
		B.Data.Add(static_cast<float>(Motion.WheelTravel + Sw.Travel));
		B.Data.Add(static_cast<float>(Motion.WheelTurn + Sw.Turn));
		Stats.WheelHulls++;
		Stats.WheelTravelMax = FMath::Max(Stats.WheelTravelMax, static_cast<float>(FMath::Abs(Motion.WheelTravel + Sw.Travel)));
	}
	else for (int32 K = Custom.Num(); K < Floats; K++) B.Data.Add(0.f);
}

#if WITH_EDITOR && WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Materials/Material.h"
#include "Materials/MaterialExpressionCustom.h"
#include "Materials/MaterialExpressionCollectionParameter.h"
#include "Materials/MaterialParameterCollection.h"
#include "StaticMeshCompiler.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Engine/TextureRenderTarget2D.h"
#include "Engine/Engine.h"
#include "ProceduralMeshComponent.h"
#include "Misc/ScopeExit.h"
#include "RenderingThread.h"
#include "RHI.h"
#include "Client/TCLighting.h"

// Generated soldier materials must retain the model's lighting and all named custom outputs. A disconnected
// output or collection GUID can compile successfully while silently dropping the reference hemisphere light.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCInfantryMaterialLookTest, "ThreeCrossroads.Infantry.MaterialLook",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCInfantryMaterialLookTest::RunTest(const FString&)
{
	// infantry-authored.js tints the atlas by faction/sourceGrey in linear light, with one sRGB texture decode.
	const FLinearColor SourceGrey(FColor(0x5d, 0x66, 0x68));
	const TCHAR* Tags[] = { TEXT("US"), TEXT("DE"), TEXT("SU"), TEXT("UK") };
	const FColor Tunics[] = { FColor(0x5f, 0x60, 0x46), FColor(0x5d, 0x66, 0x68), FColor(0x7f, 0x7a, 0x5c), FColor(0x6e, 0x5f, 0x42) };
	for (int32 I = 0; I < 4; I++)
	{
		auto* Mat = LoadObject<UMaterialInterface>(nullptr, *FString::Printf(TEXT("/Game/Models/Motion/MI_InfantryCrowdLook_%s"), Tags[I]));
		if (!TestNotNull(Tags[I], Mat)) continue;
		FLinearColor Actual;
		const FLinearColor Tunic(Tunics[I]);
		const FLinearColor Expected(Tunic.R / SourceGrey.R, Tunic.G / SourceGrey.G, Tunic.B / SourceGrey.B, 1.f);
		TestTrue(TEXT("linear faction tint is bound"), Mat->GetVectorParameterValue(FMaterialParameterInfo(TEXT("Tunic")), Actual));
		TestTrue(TEXT("faction tint uses linear colour ratios"), Actual.Equals(Expected, 2e-6f));
		for (const TCHAR* Name : { TEXT("InfantryColor"), TEXT("InfantryNormal") })
		{
			UTexture* Texture = nullptr;
			TestTrue(TEXT("the drawn crowd atlas is bound"), Mat->GetTextureParameterValue(FMaterialParameterInfo(Name), Texture));
			if (TestNotNull(Name, Texture))
			{
				TestEqual(TEXT("only the drawn colour atlas is sRGB"), Texture->SRGB != 0, FString(Name) == TEXT("InfantryColor"));
				if (FString(Name) == TEXT("InfantryNormal"))
					TestEqual(TEXT("authored normal RGB survives texture compression"), Texture->CompressionSettings, TC_VectorDisplacementmap);
			}
		}
	}
	auto Node = [](UMaterial* M, const TCHAR* Name) -> UMaterialExpressionCustom*
	{
		if (M) for (UMaterialExpression* E : M->GetExpressions())
			if (auto* C = Cast<UMaterialExpressionCustom>(E); C && C->Description == Name) return C;
		return nullptr;
	};
	UMaterial* Master = LoadObject<UMaterial>(nullptr, TEXT("/Game/Models/Materials/M_TCModel"));
	if (!TestNotNull(TEXT("reference model material"), Master)) return false;
	UMaterialExpressionCustom* MasterModel = Node(Master, TEXT("TCModel"));
	UMaterialExpressionCustom* MasterLook = Node(Master, TEXT("TCLookIndirect"));
	if (!TestNotNull(TEXT("model shader"), MasterModel) || !TestNotNull(TEXT("indirect shader"), MasterLook)) return false;
	for (const TCHAR* Name : {TEXT("M_InfantryCrowdLook"), TEXT("M_InfantryCrowdKitLook"),
		TEXT("M_InfantryCrowdLookFaded"), TEXT("M_InfantryCrowdKitLookFaded")})
	{
		UMaterial* M = LoadObject<UMaterial>(nullptr, *FString::Printf(TEXT("/Game/Models/Motion/%s"), Name));
		if (!TestNotNull(Name, M)) continue;
		auto* Model = Node(M, TEXT("TCModel"));
		auto* Look = Node(M, TEXT("TCLookIndirect"));
		if (!TestNotNull(TEXT("soldier model shader"), Model) || !TestNotNull(TEXT("soldier indirect shader"), Look)) continue;
		TestEqual(TEXT("model code comes from the master"), Model->Code, MasterModel->Code);
		TestEqual(TEXT("indirect code comes from the master"), Look->Code, MasterLook->Code);
		TestFalse(TEXT("normal is in world space"), M->bTangentSpaceNormal != 0);
		for (const FCustomInput& I : Look->Inputs)
		{
			const FString Label = FString::Printf(TEXT("%s.%s"), Name, *I.InputName.ToString());
			if (I.InputName == TEXT("Color") || I.InputName == TEXT("Roughness") || I.InputName == TEXT("Metallic") ||
				I.InputName == TEXT("Specular") || I.InputName == TEXT("NormalWS") || I.InputName == TEXT("Emissive"))
			{
				int32 Index = 0;
				if (I.InputName != TEXT("Color"))
				{
					Index = INDEX_NONE;
					for (int32 K = 0; K < Model->AdditionalOutputs.Num(); K++)
						if (Model->AdditionalOutputs[K].OutputName == I.InputName) Index = K + 1;
				}
				TestTrue(*Label, Index != INDEX_NONE && I.Input.Expression == Model && I.Input.OutputIndex == Index);
			}
		}
		for (UMaterialExpression* E : M->GetExpressions())
		{
			if (auto* C = Cast<UMaterialExpressionCollectionParameter>(E))
				TestTrue(*FString::Printf(TEXT("%s collection %s is bound"), Name, *C->ParameterName.ToString()),
					C->Collection && C->ParameterId.IsValid() && C->ParameterId == C->Collection->GetParameterId(C->ParameterName));
		}
		for (auto P : {MP_BaseColor, MP_Roughness, MP_Metallic, MP_Specular, MP_Normal, MP_EmissiveColor})
		{
			const FExpressionInput* I = M->GetExpressionInputForProperty(P);
			const int32 Index = P == MP_BaseColor || P == MP_EmissiveColor ? 0 :
				P == MP_Roughness ? 1 : P == MP_Metallic ? 2 : P == MP_Specular ? 3 : 4;
			TestTrue(*FString::Printf(TEXT("%s property %d"), Name, int32(P)),
				I && I->Expression == (P == MP_EmissiveColor ? Look : Model) && I->OutputIndex == Index);
		}
	}
	return true;
}

// Exercise the drawn model shader, not a CPU copy of its formula. These flat charts have the reference's tangent
// frame after the Three Y/Z swap: T = +Y, B = -X, N = +Z. Changing authored blue must change the rendered normal.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCInfantryNormalAtlasTest, "ThreeCrossroads.Infantry.NormalAtlas",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCInfantryNormalAtlasTest::RunTest(const FString&)
{
	if (GUsingNullRHI) { AddWarning(TEXT("NormalAtlas requires a rendering RHI; no rendered assertion ran")); return true; }
	auto* Master = LoadObject<UMaterial>(nullptr, TEXT("/Game/Models/Materials/M_TCModel"));
	if (!TestNotNull(TEXT("drawn model material"), Master)) return false;
	Master->EnsureIsComplete();
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, MakeUniqueObjectName(GetTransientPackage(), UWorld::StaticClass(), TEXT("TCNormalAtlasTest")));
	GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World);
	ON_SCOPE_EXIT { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); };
	World->InitializeActorsForPlay(FURL());
	AActor* Actor = World->SpawnActor<AActor>();
	auto* Mesh = NewObject<UProceduralMeshComponent>(Actor);
	Actor->SetRootComponent(Mesh); Actor->AddInstanceComponent(Mesh); Mesh->RegisterComponent();
	const TArray<FVector> Positions = { {-100,-100,0}, {-100,100,0}, {100,100,0}, {100,-100,0} };
	const TArray<int32> Indices = { 0,1,2,0,2,3 };
	const TArray<FVector> Normals = { FVector::UpVector, FVector::UpVector, FVector::UpVector, FVector::UpVector };
	const TArray<FVector2D> UV = { {0,1},{1,1},{1,0},{0,0} };
	const TArray<FVector2D> Mat = { {-2,1},{-2,1},{-2,1},{-2,1} };
	const TArray<FVector2D> Scale = { {1,0},{1,0},{1,0},{1,0} };
	const TArray<FLinearColor> Colors = { FLinearColor::White,FLinearColor::White,FLinearColor::White,FLinearColor::White };
	Mesh->CreateMeshSection_LinearColor(0, Positions, Indices, Normals, UV, Mat, {}, Scale, Colors, {}, false, false);
	auto* Mid = UMaterialInstanceDynamic::Create(Master, Actor);
	Mid->SetScalarParameterValue(TEXT("ModelTextures"), 0.f);
	Mid->SetScalarParameterValue(TEXT("InfantryReady"), 1.f);
	Mid->SetScalarParameterValue(TEXT("InfantryDetail"), 1.f);
	Mesh->SetMaterial(0, Mid);
	auto* Target = NewObject<UTextureRenderTarget2D>(Actor);
	Target->InitCustomFormat(32, 32, PF_FloatRGBA, true);
	Target->UpdateResourceImmediate(true);
	auto* Capture = NewObject<USceneCaptureComponent2D>(Actor);
	Capture->TextureTarget = Target; Capture->CaptureSource = SCS_Normal;
	Capture->bCaptureEveryFrame = Capture->bCaptureOnMovement = false;
	Capture->FOVAngle = 35.f;
	Actor->AddInstanceComponent(Capture); Capture->RegisterComponent();
	Capture->SetWorldLocationAndRotation(FVector(0,0,400), FRotator(-90,0,0));
	FVector First;
	for (int32 Blue : { 166,230 })
	{
		auto* Normal = UTexture2D::CreateTransient(1, 1, PF_B8G8R8A8);
		Normal->SRGB = false; Normal->NeverStream = true;
		auto& Mip = Normal->GetPlatformData()->Mips[0];
		FColor* Pixel = static_cast<FColor*>(Mip.BulkData.Lock(LOCK_READ_WRITE));
		*Pixel = FColor(191,64,Blue); Mip.BulkData.Unlock(); Normal->UpdateResource();
		Mid->SetTextureParameterValue(TEXT("InfantryNormal"), Normal);
		World->SendAllEndOfFrameUpdates(); FlushRenderingCommands();
		Capture->CaptureScene(); FlushRenderingCommands();
		TArray<FLinearColor> Pixels;
		if (!TestTrue(TEXT("normal capture readback"), Target->GameThread_GetRenderTargetResource()->ReadLinearColorPixels(Pixels, FReadSurfaceDataFlags(RCM_MinMax)))) return false;
		if (!TestEqual(TEXT("normal capture size"), Pixels.Num(), 32 * 32)) return false;
		const FLinearColor& P = Pixels[16 * 32 + 16];
		const FVector Actual(P.R, P.G, P.B);
		// infantry-material.js reads RGB * 2 - 1, then scales XY by 0.65, then normalizes the frame transform.
		const FVector Map((191.f / 255.f * 2.f - 1.f) * 0.65f, (64.f / 255.f * 2.f - 1.f) * 0.65f, Blue / 255.f * 2.f - 1.f);
		const FVector Expected = FVector(-Map.Y, Map.X, Map.Z).GetSafeNormal();
		AddInfo(FString::Printf(TEXT("atlas blue %d: rendered %s, reference %s"), Blue, *Actual.ToString(), *Expected.ToString()));
		TestTrue(TEXT("RGB decode and reflected cotangent frame match the reference"), Actual.Equals(Expected, 0.025));
		if (Blue == 166) First = Actual;
		else TestTrue(TEXT("authored blue survives without RG reconstruction"), FMath::Abs(Actual.Z - First.Z) > 0.25);
	}
	// The same drawn shader must receive live hemisphere parameters. A white Lambert chart with irradiance PI/4
	// has radiance 1/4. This also separates a normal-map error from a disconnected ambient-light input.
	Mid->SetScalarParameterValue(TEXT("InfantryReady"), 0.f);
	Mid->SetScalarParameterValue(TEXT("BaseSpecular"), 0.f);
	Mid->SetVectorParameterValue(TEXT("RimColor"), FLinearColor::Black);
	Capture->CaptureSource = SCS_SceneColorHDR;
	TCLook::LockPostProcess(Capture->PostProcessSettings, nullptr, 0.85f);
	for (float Intensity : { 1.f,0.5f })
	{
		FTCMood Mood = TCLook::DefaultMood();
		Mood.Top = Mood.Bottom = FLinearColor(PI / 4.f, PI / 4.f, PI / 4.f);
		Mood.HemiI = Intensity;
		TCLook::SetCollection(World, Mood, 0.f, 0.f, true);
		World->SendAllEndOfFrameUpdates(); FlushRenderingCommands();
		Capture->CaptureScene(); FlushRenderingCommands();
		TArray<FLinearColor> Pixels;
		if (!TestTrue(TEXT("hemisphere capture readback"), Target->GameThread_GetRenderTargetResource()->ReadLinearColorPixels(Pixels, FReadSurfaceDataFlags(RCM_MinMax)))) return false;
		if (!TestEqual(TEXT("hemisphere capture size"), Pixels.Num(), 32 * 32)) return false;
		const FLinearColor& Actual = Pixels[16 * 32 + 16];
		const float Expected = Intensity / 4.f;
		AddInfo(FString::Printf(TEXT("model hemisphere %.1f: rendered %s, reference %.6f"), Intensity, *Actual.ToString(), Expected));
		TestTrue(TEXT("live hemisphere reaches the drawn material in linear light"), Actual.Equals(FLinearColor(Expected, Expected, Expected, Actual.A), 0.015f));
	}
	// Check the production instanced-static-mesh path too. A procedural chart alone cannot detect an ISM-only
	// disconnect. Equal top/bottom irradiance removes normal and sun-direction differences from this assertion.
	Mesh->SetVisibility(false);
	auto* Hq = LoadObject<UStaticMesh>(nullptr, TEXT("/Game/Models/Meshes/hq/SM_hq_0_body.SM_hq_0_body"));
	if (!TestNotNull(TEXT("reference HQ mesh"), Hq)) return false;
	TArray<UStaticMesh*> PendingMeshes = { Hq };
	FStaticMeshCompilingManager::Get().FinishCompilation(PendingMeshes);
	auto* Ism = NewObject<UInstancedStaticMeshComponent>(Actor);
	Ism->SetStaticMesh(Hq); Ism->SetMaterial(0, Mid); Ism->SetNumCustomDataFloats(TCModelData::Count);
	Ism->SetMobility(EComponentMobility::Movable);
	Actor->AddInstanceComponent(Ism); Ism->RegisterComponent(); Ism->AddInstance(FTransform::Identity);
	for (int32 I = 0; I < 3; ++I) Ism->SetCustomDataValue(0, I, 1.f, false);
	Ism->MarkRenderStateDirty();
	Capture->FOVAngle = 68.621216f;
	Capture->SetWorldLocationAndRotation(FVector(0,2326.732,3253.662), FRotator(-54.430988,-90,0));
	AddInfo(FString::Printf(TEXT("instanced HQ bounds %s"), *Ism->Bounds.ToString()));
	Capture->CaptureSource = SCS_BaseColor;
	World->SendAllEndOfFrameUpdates(); FlushRenderingCommands(); Capture->CaptureScene(); FlushRenderingCommands();
	TArray<FLinearColor> Albedo;
	if (!TestTrue(TEXT("instanced HQ albedo readback"), Target->GameThread_GetRenderTargetResource()->ReadLinearColorPixels(Albedo, FReadSurfaceDataFlags(RCM_MinMax)))) return false;
	float AlbedoSum = 0.f; for (const FLinearColor& P : Albedo) AlbedoSum += P.GetLuminance();
	AddInfo(FString::Printf(TEXT("instanced HQ summed albedo %.6f"), AlbedoSum));
	if (!TestTrue(TEXT("reference HQ is visible in the capture"), AlbedoSum > 0.1f)) return false;
	Capture->CaptureSource = SCS_SceneColorHDR;
	float Full = 0.f;
	for (float Intensity : { 1.f,0.5f })
	{
		FTCMood Mood = TCLook::DefaultMood();
		Mood.Top = Mood.Bottom = FLinearColor(PI / 4.f, PI / 4.f, PI / 4.f); Mood.HemiI = Intensity;
		TCLook::SetCollection(World, Mood, 0.f, 0.f, true);
		World->SendAllEndOfFrameUpdates(); FlushRenderingCommands(); Capture->CaptureScene(); FlushRenderingCommands();
		TArray<FLinearColor> Pixels;
		if (!TestTrue(TEXT("instanced HQ readback"), Target->GameThread_GetRenderTargetResource()->ReadLinearColorPixels(Pixels, FReadSurfaceDataFlags(RCM_MinMax)))) return false;
		if (!TestEqual(TEXT("instanced HQ capture size"), Pixels.Num(), 32 * 32)) return false;
		float Sum = 0.f;
		for (int32 Y = 8; Y < 24; ++Y) for (int32 X = 8; X < 24; ++X) Sum += Pixels[Y * 32 + X].GetLuminance();
		AddInfo(FString::Printf(TEXT("instanced HQ hemisphere %.1f: summed linear luminance %.6f"), Intensity, Sum));
		if (Intensity == 1.f) { Full = Sum; TestTrue(TEXT("instanced HQ receives hemisphere light"), Full > 0.01f); }
		else TestTrue(TEXT("instanced HQ follows live hemisphere intensity"), FMath::Abs(Sum - Full * 0.5f) < Full * 0.025f);
	}
	return true;
}
#endif
