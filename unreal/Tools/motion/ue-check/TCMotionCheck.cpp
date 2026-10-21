// Verification only (see TCMotionCheck.h).
#include "Check/TCMotionCheck.h"
#include "Client/TCUnitMotion.h"
#include "Client/TCInfantryRig.h"
#include "Client/TCModelLibrary.h"
#include "AssetCompilingManager.h"
#include "Components/DirectionalLightComponent.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Components/SkyLightComponent.h"
#include "Components/StaticMeshComponent.h"
#include "Dom/JsonObject.h"
#include "Engine/Engine.h"
#include "Engine/StaticMesh.h"
#include "Engine/TextureCube.h"
#include "Engine/TextureRenderTarget2D.h"
#include "Engine/World.h"
#include "ImageUtils.h"
#include "Materials/Material.h"
#include "MaterialShared.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "RenderingThread.h"
#include "Serialization/JsonSerializer.h"
#include "ShaderCompiler.h"
#include "StaticMeshResources.h"
#include "TextureResource.h"

int32 UTCMotionCheckCommandlet::Main(const FString& Params)
{
	FString OutPath, PngPath;
	FParse::Value(*Params, TEXT("Out="), OutPath);
	FParse::Value(*Params, TEXT("Png="), PngPath);
	TSharedRef<FJsonObject> Report = MakeShared<FJsonObject>();
	TArray<TSharedPtr<FJsonValue>> Failures;
	auto Fail = [&Failures](const FString& What) { UE_LOG(LogTemp, Error, TEXT("MOTION FAIL %s"), *What); Failures.Add(MakeShared<FJsonValueString>(What)); };
	Report->SetBoolField(TEXT("canRender"), FApp::CanEverRender());

	// ------------------------------------------------------------------ the materials compile on this platform
	const TCHAR* Paths[] = { TEXT("/Game/Models/Motion/M_InfantryCrowdFaded.M_InfantryCrowdFaded"), TEXT("/Game/Models/Motion/M_InfantryCrowdKitFaded.M_InfantryCrowdKitFaded"),
		TEXT("/Game/Models/Motion/M_InfantryFar.M_InfantryFar"), TEXT("/Game/Models/Motion/M_InfantryFarFaded.M_InfantryFarFaded") };
	TArray<UMaterial*> Materials;
	for (const TCHAR* Path : Paths) if (UMaterial* M = LoadObject<UMaterial>(nullptr, Path)) Materials.Add(M); else Fail(FString::Printf(TEXT("missing %s"), Path));
	for (UMaterial* M : Materials) M->ForceRecompileForRendering();
	if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();
	TArray<TSharedPtr<FJsonValue>> MaterialReports;
	for (UMaterial* M : Materials)
	{
		const FMaterialResource* Resource = M->GetMaterialResource(GMaxRHIShaderPlatform);
		TSharedRef<FJsonObject> R = MakeShared<FJsonObject>();
		R->SetStringField(TEXT("material"), M->GetName());
		TArray<TSharedPtr<FJsonValue>> Errors;
		if (Resource) for (const FString& E : Resource->GetCompileErrors()) Errors.Add(MakeShared<FJsonValueString>(E));
		R->SetArrayField(TEXT("compileErrors"), Errors);
		R->SetBoolField(TEXT("hasShaderMap"), Resource && Resource->GetGameThreadShaderMap() != nullptr);
		R->SetStringField(TEXT("blend"), StaticEnum<EBlendMode>()->GetNameStringByValue(M->BlendMode));
		if (!Resource || !Errors.IsEmpty() || !Resource->GetGameThreadShaderMap()) Fail(FString::Printf(TEXT("%s does not compile"), *M->GetName()));
		MaterialReports.Add(MakeShared<FJsonValueObject>(R));
	}
	Report->SetArrayField(TEXT("materials"), MaterialReports);

	// ------------------------------------------------------------------ the far figures' keyframes against their data
	{
		const FString BinPath = FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Models/Motion/FarFigures.bin"));
		Report->SetStringField(TEXT("farFigures"), BinPath);
	}

	// ------------------------------------------------------------------ the scene
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("MotionCheck"));
	FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
	Context.SetCurrentWorld(World);
	AActor* Holder = World->SpawnActor<AActor>();
	USceneComponent* Root = NewObject<USceneComponent>(Holder);
	Holder->SetRootComponent(Root);
	Root->RegisterComponent();
	UTCSoldierCrowd* Crowd = NewObject<UTCSoldierCrowd>(Holder);
	Crowd->SetupAttachment(Root);
	Crowd->RegisterComponent();
	FString LoadError;
	if (!Crowd->Load(LoadError)) Fail(FString::Printf(TEXT("crowd load: %s"), *LoadError));
	Report->SetBoolField(TEXT("near"), Crowd->HasNear());
	Report->SetBoolField(TEXT("far"), Crowd->HasFar());
	auto Ground = [](double, double) { return 0.0; };

	// Row A (y = -300): near men of a US rifle squad in the postures and gaits of animate(); row B (y = 0): the same
	// motion states on the far figures (drawn close, so they can be seen); row C (y = 300): camouflaged near and far men
	// and bodies. Each man faces -X (toward the camera, which looks along +X).
	struct FState { const TCHAR* Name; FTCInfantryMotion M; };
	TArray<FState> States;
	auto Add = [&States](const TCHAR* Name, TFunctionRef<void(FTCInfantryMotion&)> Set) { FState S{ Name, FTCInfantryMotion() }; Set(S.M); States.Add(S); };
	Add(TEXT("stand-aim"), [](FTCInfantryMotion&) {});
	Add(TEXT("kneel"), [](FTCInfantryMotion& M) { M.PostureWeights = FVector4(0, 1, 0, 0); });
	Add(TEXT("prone"), [](FTCInfantryMotion& M) { M.PostureWeights = FVector4(0, 0, 1, 0); });
	Add(TEXT("run"), [](FTCInfantryMotion& M) { M.PostureWeights = FVector4(0, 0, 0, 1); M.Moving = 1; M.Phase = 0.3f; });
	Add(TEXT("walk-fire"), [](FTCInfantryMotion& M) { M.Moving = 1; M.Aim = 1; M.Phase = 0.6f; });
	Add(TEXT("crawl"), [](FTCInfantryMotion& M) { M.PostureWeights = FVector4(0, 0, 1, 0); M.Moving = 1; M.Phase = 0.25f; });
	Add(TEXT("dig"), [](FTCInfantryMotion& M) { M.Work = 1; M.WorkPhase = 0.4f; });
	Add(TEXT("half-down"), [](FTCInfantryMotion& M) { M.PostureWeights = FVector4(0.5, 0, 0.5, 0); });
	const FQuat Facing = FQuat::Identity; // facing +X, toward the cameras
	const float Scale = UTCInfantryRigLibrary::FigureScale(TEXT("rifle"));
	TArray<TSharedPtr<FJsonValue>> Placed;
	Crowd->BeginFrame();
	for (int32 Row = 0; Row < 2; Row++)
		for (int32 I = 0; I < States.Num(); I++)
		{
			const FTCInfantryBlend Blend = UTCInfantryRigLibrary::BlendFor(TEXT("rifle"), States[I].M);
			FTCSoldierDraw D;
			D.FigureType = TEXT("rifle"); D.Faction = 0; D.Slot = 2; D.Blend = Blend; D.bFar = Row == 1;
			D.Owner = FLinearColor(0.0437f, 0.1714f, 0.6724f); // seat 0 (main.js COLORS[0]), linear
			// the man node's offset keeps the muzzle point fixed between postures (animate() man.position)
			D.Man = FTransform(Facing, FVector(-250.0 * Row + Blend.ManOffset.X * Scale, -400.0 + I * 100.0, Blend.ManOffset.Y * Scale), FVector(Scale));
			if (!Crowd->AddMan(D)) Fail(FString::Printf(TEXT("%s %s not drawn"), Row ? TEXT("far") : TEXT("near"), States[I].Name));
			Placed.Add(MakeShared<FJsonValueString>(FString::Printf(TEXT("%s %s"), Row ? TEXT("far") : TEXT("near"), States[I].Name)));
		}
	// camouflaged sniper pair (near and far) and a German MG crew in the far figure, kneeling
	for (int32 Far = 0; Far < 2; Far++)
		for (int32 Slot = 0; Slot < 2; Slot++)
		{
			FTCSoldierDraw D;
			D.FigureType = TEXT("sniper"); D.Faction = 1; D.Slot = Slot; D.Fade = 0.45f; D.bFar = Far == 1;
			FTCInfantryMotion M; M.PostureWeights = FVector4(0, 0, 1, 0);
			D.Blend = UTCInfantryRigLibrary::BlendFor(TEXT("sniper"), M);
			D.Owner = FLinearColor(0.6038f, 0.0423f, 0.0273f);
			D.Man = FTransform(Facing, FVector(-500.0 - Far * 250.0, 450.0 + Slot * 150.0, D.Blend.ManOffset.Y * 1.35), FVector(1.35));
			if (!Crowd->AddMan(D)) Fail(TEXT("camouflaged sniper not drawn"));
		}
	// beside the far stand-aim man: the catalog's own far figure of the same man (import-models.py mesh and material,
	// rest pose), for the colors under the same light
	FTCModelLibrary Library;
	FString LibError;
	const bool bLibrary = Library.Load(LibError);
	if (bLibrary)
		if (const FTCModelEntry* Rifle = Library.Resolve({ TEXT("rifle"), 0, FString() }))
		{
			const int32 Part = Rifle->FindPart(TEXT("man_2_far"));
			UInstancedStaticMeshComponent* Ism = NewObject<UInstancedStaticMeshComponent>(Holder);
			if (Part != INDEX_NONE && FTCModelLibrary::SetupComponent(Ism, Rifle->Parts[Part], false))
			{
				Ism->RegisterComponentWithWorld(World);
				const FTCInfantryBlend Stand = UTCInfantryRigLibrary::BlendFor(TEXT("rifle"), FTCInfantryMotion());
				const int32 Index = Ism->AddInstance(FTransform(Facing, FVector(-250.0 + Stand.ManOffset.X * Scale, -500.0, 0), Rifle->Parts[Part].Rest.GetScale3D()), true);
				Ism->SetCustomData(Index, Library.CustomData(0, 0, 1.f, 0.f), true);
				Report->SetStringField(TEXT("catalogFarFigure"), Rifle->Parts[Part].Mesh.ToString());
				// the catalog mesh and the crowd's far mesh are the same standing build: their boxes must agree
				const FBox Catalog = Ism->GetStaticMesh()->GetBoundingBox();
				Report->SetStringField(TEXT("catalogFarBox"), Catalog.ToString());
				for (UActorComponent* C : Holder->GetComponents())
					if (UInstancedStaticMeshComponent* Far = Cast<UInstancedStaticMeshComponent>(C))
						if (Far->GetStaticMesh() && Far->GetStaticMesh()->GetName().StartsWith(TEXT("SM_Far_rifle_0_rifleB")))
						{
							const FBox Mine = Far->GetStaticMesh()->GetBoundingBox();
							Report->SetStringField(TEXT("crowdFarBox"), Mine.ToString());
							const double Gap = FMath::Max((Mine.Min - Catalog.Min).GetAbsMax(), (Mine.Max - Catalog.Max).GetAbsMax());
							Report->SetNumberField(TEXT("farBoxGapCm"), Gap);
							Report->SetStringField(TEXT("crowdFarMesh"), Far->GetStaticMesh()->GetName());
							if (Gap > 0.05) Fail(FString::Printf(TEXT("far figure box differs from the catalog's by %.3f cm"), Gap));
							break;
						}
			}
			else Fail(TEXT("no catalog far figure man_2_far for rifle|0"));
		}
	// three bodies (unit-models.js corpses: the fallen build resting on its lowest point)
	Crowd->AddCorpse(TEXT("rifle"), 0, 1, FLinearColor::Blue, FVector(-150, 450, 0), 0.3, 1.1f);
	Crowd->AddCorpse(TEXT("conscript"), 2, 3, FLinearColor::Blue, FVector(-150, 600, 0), 2.0, 1.1f);
	Crowd->AddCorpse(TEXT("mg"), 1, 1, FLinearColor::Red, FVector(-150, 750, 0), 4.0, 1.35f);
	Crowd->EndFrame(0.016f, FVector(600, 0, 160), Ground);
	Report->SetArrayField(TEXT("placed"), Placed);
	const FTCSoldierCrowdStats& S = Crowd->Stats();
	TSharedRef<FJsonObject> Stats = MakeShared<FJsonObject>();
	Stats->SetNumberField(TEXT("nearMen"), S.NearMen); Stats->SetNumberField(TEXT("farMen"), S.FarMen); Stats->SetNumberField(TEXT("fadedMen"), S.FadedMen);
	Stats->SetNumberField(TEXT("corpses"), S.Corpses); Stats->SetNumberField(TEXT("nearCorpses"), S.NearCorpses); Stats->SetNumberField(TEXT("farCorpses"), S.FarCorpses);
	Stats->SetNumberField(TEXT("nearFigures"), S.NearFigures); Stats->SetNumberField(TEXT("farFigures"), S.FarFigures); Stats->SetNumberField(TEXT("components"), S.Components);
	Stats->SetNumberField(TEXT("refused"), S.Refused);
	Report->SetObjectField(TEXT("crowd"), Stats);

	// ------------------------------------------------------------------ vehicles from the catalog with their motion
	TArray<TSharedPtr<FJsonValue>> VehicleReports;
	if (bLibrary)
	{
		FTCUnitMotion Motion;
		struct FVeh { const TCHAR* Type; int32 Fac; FVector At; double Rot; };
		const FVeh Vehicles[] = { { TEXT("medium"), 0, FVector(-1400, -600, 0), 0.4 }, { TEXT("kaiju"), 0, FVector(-4200, 900, 0), 0.6 }, { TEXT("fighter"), 0, FVector(-1600, 700, 400), -0.5 } };
		for (const FVeh& V : Vehicles)
		{
			const FTCModelEntry* Entry = Library.Resolve({ V.Type, V.Fac, FString() });
			if (!Entry) { Fail(FString::Printf(TEXT("no catalog model for %s"), V.Type)); continue; }
			// drive it: accelerate and turn (the hull pitches and rolls, the kaiju strides, the plane banks)
			const int32 Id = 100 + static_cast<int32>(&V - Vehicles);
			FVector At = V.At;
			double Rot = V.Rot;
			const FTCPartMotion* M = nullptr;
			for (int32 I = 0; I < 25; I++)
			{
				Rot += 0.02;
				At += FVector(FMath::Cos(Rot), FMath::Sin(Rot), 0) * (I * 25.0) * 0.033;
				M = &Motion.Update(Id, V.Type, At, Rot, true, 0.033f, I * 0.033);
			}
			const FTransform Unit(FQuat(FVector::UpVector, Rot), At);
			TMap<FName, FQuat> Turns;
			Turns.Add(TEXT("turret"), FQuat(FVector::UpVector, 0.5));
			int32 Parts = 0;
			for (int32 P = 0; P < Entry->Parts.Num(); P++)
			{
				const FTCModelPart& Part = Entry->Parts[P];
				if (Part.bHidden) continue;
				UStaticMeshComponent* C = NewObject<UStaticMeshComponent>(Holder);
				UStaticMesh* Mesh = Cast<UStaticMesh>(Part.Mesh.TryLoad());
				if (!Mesh) continue;
				C->SetStaticMesh(Mesh);
				for (int32 Slot = 0; Slot < Part.Materials.Num(); Slot++) if (UMaterialInterface* Mi = Cast<UMaterialInterface>(Part.Materials[Slot].TryLoad())) C->SetMaterial(Slot, Mi);
				C->SetWorldTransform(FTCUnitMotion::PartTransform(*Entry, P, Unit, Turns, M ? *M : FTCPartMotion()));
				C->RegisterComponentWithWorld(World);
				Parts++;
			}
			TSharedRef<FJsonObject> R = MakeShared<FJsonObject>();
			R->SetStringField(TEXT("type"), V.Type);
			R->SetNumberField(TEXT("parts"), Parts);
			R->SetStringField(TEXT("body"), M ? M->Body.GetRotation().Rotator().ToString() : FString());
			TArray<TSharedPtr<FJsonValue>> Moves;
			if (M) for (const auto& Pm : M->Parts) Moves.Add(MakeShared<FJsonValueString>(Pm.Key.ToString() + TEXT(" ") + Pm.Value.ToString()));
			R->SetArrayField(TEXT("parts moved"), Moves);
			VehicleReports.Add(MakeShared<FJsonValueObject>(R));
		}
	}
	else Fail(FString::Printf(TEXT("catalog: %s"), *LibError));
	Report->SetArrayField(TEXT("vehicles"), VehicleReports);

	// ------------------------------------------------------------------ light, ground, captures
	UDirectionalLightComponent* Sun = NewObject<UDirectionalLightComponent>(Holder);
	Sun->SetWorldRotation(FRotator(-40.f, 160.f, 0.f)); Sun->SetIntensity(8.f); Sun->RegisterComponentWithWorld(World);
	UDirectionalLightComponent* Fill = NewObject<UDirectionalLightComponent>(Holder);
	Fill->SetWorldRotation(FRotator(-20.f, -40.f, 0.f)); Fill->SetIntensity(4.f); Fill->SetCastShadows(false); Fill->RegisterComponentWithWorld(World);
	USkyLightComponent* Sky = NewObject<USkyLightComponent>(Holder);
	Sky->SourceType = SLS_SpecifiedCubemap;
	Sky->Cubemap = LoadObject<UTextureCube>(nullptr, TEXT("/Engine/MapTemplates/Sky/DaylightAmbientCubemap.DaylightAmbientCubemap"));
	Sky->SetIntensity(1.f);
	Sky->RegisterComponentWithWorld(World);
	Sky->RecaptureSky();
	UStaticMeshComponent* Floor = NewObject<UStaticMeshComponent>(Holder);
	Floor->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Plane.Plane")));
	Floor->SetWorldTransform(FTransform(FQuat::Identity, FVector(-1500, 0, 0), FVector(80, 80, 1)));
	Floor->RegisterComponentWithWorld(World);
	FAssetCompilingManager::Get().FinishAllCompilation();
	if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();

	struct FView { const TCHAR* Suffix; FVector At; FRotator Look; float Fov; };
	const FView Views[] = {
		{ TEXT("soldiers"), FVector(700, 150, 320), FRotator(-16.f, 180.f, 0.f), 80.f },
		{ TEXT("far-close"), FVector(150, -50, 220), FRotator(-22.f, 180.f, 0.f), 60.f },
		{ TEXT("far-compare"), FVector(450, -450, 200), FRotator(-8.f, 180.f, 0.f), 16.f },
		{ TEXT("vehicles"), FVector(1200, 300, 1500), FRotator(-22.f, 180.f, 0.f), 80.f },
	};
	TArray<TSharedPtr<FJsonValue>> Shots;
	for (const FView& View : Views)
	{
		UTextureRenderTarget2D* Target = NewObject<UTextureRenderTarget2D>(Holder);
		Target->RenderTargetFormat = RTF_RGBA8_SRGB;
		Target->ClearColor = FLinearColor(0.42f, 0.47f, 0.52f);
		Target->InitAutoFormat(1600, 900);
		Target->UpdateResourceImmediate(true);
		USceneCaptureComponent2D* Capture = NewObject<USceneCaptureComponent2D>(Holder);
		Capture->TextureTarget = Target;
		Capture->CaptureSource = SCS_FinalColorLDR;
		Capture->bCaptureEveryFrame = false;
		Capture->bCaptureOnMovement = false;
		Capture->FOVAngle = View.Fov;
		Capture->PostProcessSettings.bOverride_AutoExposureMethod = true;
		Capture->PostProcessSettings.AutoExposureMethod = AEM_Manual;
		Capture->PostProcessSettings.bOverride_AutoExposureBias = true;
		Capture->PostProcessSettings.AutoExposureBias = 1.5f;
		Capture->PostProcessSettings.bOverride_AutoExposureApplyPhysicalCameraExposure = true;
		Capture->PostProcessSettings.AutoExposureApplyPhysicalCameraExposure = false;
		Capture->PostProcessBlendWeight = 1.f;
		Capture->SetWorldLocationAndRotation(View.At, View.Look);
		Capture->RegisterComponentWithWorld(World);
		for (int32 Frame = 0; Frame < 6; Frame++)
		{
			World->SendAllEndOfFrameUpdates();
			Capture->CaptureScene();
			FlushRenderingCommands();
		}
		TArray<FColor> Pixels;
		FTextureRenderTargetResource* Resource = Target->GameThread_GetRenderTargetResource();
		const FString Path = FPaths::GetBaseFilename(PngPath, false) + TEXT("-") + View.Suffix + TEXT(".png");
		if (!Resource || !Resource->ReadPixels(Pixels) || Pixels.IsEmpty()) { Fail(FString::Printf(TEXT("could not read the %s capture"), View.Suffix)); continue; }
		for (FColor& C : Pixels) C.A = 255;
		TArray64<uint8> Png;
		FImageUtils::PNGCompressImageArray(1600, 900, TArrayView64<const FColor>(Pixels.GetData(), Pixels.Num()), Png);
		FFileHelper::SaveArrayToFile(Png, *Path);
		Shots.Add(MakeShared<FJsonValueString>(Path));
	}
	Report->SetArrayField(TEXT("captures"), Shots);
	Report->SetArrayField(TEXT("failures"), Failures);
	Report->SetBoolField(TEXT("pass"), Failures.IsEmpty());
	GEngine->DestroyWorldContext(World);
	World->DestroyWorld(false);
	FString Json;
	FJsonSerializer::Serialize(Report, TJsonWriterFactory<>::Create(&Json));
	FFileHelper::SaveStringToFile(Json, *OutPath);
	UE_LOG(LogTemp, Display, TEXT("MOTION %s"), Failures.IsEmpty() ? TEXT("PASS") : TEXT("FAIL"));
	return Failures.IsEmpty() ? 0 : 1;
}
