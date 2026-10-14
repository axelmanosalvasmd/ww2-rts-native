#include "Client/TCModelCompare.h"
#include "Client/TCGraphics.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "Client/TCTutorial.h"
#include "Net/TCMatchState.h"
#include "Components/DirectionalLightComponent.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Components/SkyLightComponent.h"
#include "Components/SkyAtmosphereComponent.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/Engine.h"
#include "Engine/StaticMesh.h"
#include "Engine/TextureCube.h"
#include "Engine/TextureRenderTarget2D.h"
#include "Engine/World.h"
#include "GameFramework/WorldSettings.h"
#include "ImageUtils.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/FileHelper.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "TextureResource.h"
#include "RenderingThread.h"
#if WITH_EDITOR
#include "AssetCompilingManager.h"
#include "ShaderCompiler.h"
#endif

DEFINE_LOG_CATEGORY_STATIC(LogTCModelCompare, Log, All);

bool TCTour::ParseLine(const FString& Line, FTCTourPose& P)
{
	FString L = Line;
	int32 Hash;
	if (L.FindChar('#', Hash)) L.LeftInline(Hash);
	TArray<FString> F;
	L.ParseIntoArrayWS(F);
	if (F.Num() < 6) return false;
	P = FTCTourPose();
	P.Name = F[0];
	P.bWhenPaused = F[1].StartsWith(TEXT("p"));
	P.At = FCString::Atod(P.bWhenPaused ? *F[1].Mid(1) : *F[1]);
	P.bFrontier = F[2] == TEXT("frontier");
	P.X = FCString::Atod(*F[2]); P.Z = FCString::Atod(*F[3]);
	P.bWideShare = F[4].StartsWith(TEXT("w"));
	P.Dist = FCString::Atod(P.bWideShare ? *F[4].Mid(1) : *F[4]);
	P.Yaw = FCString::Atod(*F[5]);
	if (F[2].StartsWith(TEXT("unit:")))
	{
		FString Type = F[2].Mid(5), Seat;
		if (Type.Split(TEXT("@"), &Type, &Seat) && Seat.IsNumeric()) P.UnitSeat = FCString::Atoi(*Seat);
		P.UnitType = Type;
		P.X = P.Z = 0;
	}
	if (F[2].StartsWith(TEXT("step:"))) { P.Step = FMath::Max(0, FCString::Atoi(*F[2].Mid(5))); P.X = P.Z = 0; }
	P.bHud = F.Num() > 6 && F[6] == TEXT("hud");
	return true;
}

bool TCTour::StepFocus(int32 PoseStep, FVector2D& Out)
{
	const TArray<FTCTutorialStep>& Steps = TCTutorialData::Steps();
	if (PoseStep < 1 || PoseStep > Steps.Num()) return false;
	const FIntPoint C = Steps[PoseStep - 1].At;
	Out = FVector2D(C.X * TCTutorialData::Cell + 1.0, C.Y * TCTutorialData::Cell + 1.0); // tour-reference.mjs: at * 2 + 1
	return true;
}

int32 TCTour::UnitSeatFor(const FTCTourPose& P, bool bSpectator, int32 You)
{
	if (P.UnitSeat >= 0) return P.UnitSeat;
	return bSpectator || You < 0 ? 0 : You;
}

int32 TCTour::PickUnit(const FTCTourPose& P, const FTCMatchState& State, int32 Seat, FVector2D& OutAt)
{
	int32 Best = -1;
	if (P.UnitType == TEXT("vet"))
	{
		// round 17 (lead): unit:vet, the living unit with the most veterancy stars of any seat, the lowest id among equals
		// (tour-reference.mjs picks the same from the page's v.vet); -1 while nobody has a star
		int32 BestStars = 0;
		for (const auto& U : State.Units)
			if (U.Value.Hp > 0 && U.Value.Stars > 0 && (U.Value.Stars > BestStars || (U.Value.Stars == BestStars && U.Key < Best)))
			{
				Best = U.Key; BestStars = U.Value.Stars; OutAt = FVector2D(U.Value.X, U.Value.Z);
			}
		return Best;
	}
	for (const auto& U : State.Units)
		if (U.Value.Type == P.UnitType && U.Value.Owner == Seat && U.Value.Hp > 0 && (Best < 0 || U.Key < Best)) { Best = U.Key; OutAt = FVector2D(U.Value.X, U.Value.Z); }
	return Best;
}

namespace TCModelCompareImpl
{
	// reference (x, y, z) metres -> Unreal cm
	FVector FromReference(const TArray<TSharedPtr<FJsonValue>>& A) { return A.Num() == 3 ? FVector(A[0]->AsNumber(), A[2]->AsNumber(), A[1]->AsNumber()) * 100.0 : FVector::ZeroVector; }
	FLinearColor Hex(const TSharedPtr<FJsonObject>& O, const TCHAR* Field, uint32 Default)
	{
		double V = Default;
		O->TryGetNumberField(Field, V);
		const uint32 C = (uint32)V;
		return FLinearColor(FColor((C >> 16) & 255, (C >> 8) & 255, C & 255));
	}
	double Num(const TSharedPtr<FJsonObject>& O, const TCHAR* Field, double Default) { double V = Default; O->TryGetNumberField(Field, V); return V; }

	// A world to draw in, outside any game: the renderer is spawned in it like the game mode spawns it.
	struct FScratchWorld
	{
		UWorld* World = nullptr;
		explicit FScratchWorld(const TCHAR* Name)
		{
			World = UWorld::CreateWorld(EWorldType::Game, false, Name);
			FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
			Context.SetCurrentWorld(World);
			World->InitializeActorsForPlay(FURL());
		}
		~FScratchWorld()
		{
			GEngine->DestroyWorldContext(World);
			World->DestroyWorld(false);
		}
	};

	FTCUnit MakeUnit(int32 Id, const FString& Type, int32 Owner, float X, float Z, const FTCUnitTable& Table)
	{
		FTCUnit U;
		U.Id = Id; U.Type = Type; U.Owner = Owner; U.X = X; U.Z = Z;
		const FTCUnitDef* Def = Table.Find(Type);
		U.Hp = Def ? FMath::Max(1, Def->MaxHp()) : 100;
		U.Built = 1.f;
		return U;
	}

	void FinishCompiling()
	{
#if WITH_EDITOR
		FAssetCompilingManager::Get().FinishAllCompilation();
		if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();
#endif
	}
}

UTCModelCompareCommandlet::UTCModelCompareCommandlet()
{
	// the defaults (client, editor and server): a world made without GIsClient gets no render scene to capture
	LogToConsole = true;
}

int32 UTCModelCompareCommandlet::Main(const FString& Params)
{
	using namespace TCModelCompareImpl;
	FString CamerasPath, OutDir;
	FParse::Value(*Params, TEXT("Cameras="), CamerasPath);
	FParse::Value(*Params, TEXT("Out="), OutDir);
	FString Text;
	TSharedPtr<FJsonObject> Doc;
	if (CamerasPath.IsEmpty() || OutDir.IsEmpty() || !FFileHelper::LoadFileToString(Text, *CamerasPath) || !FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Doc) || !Doc.IsValid())
	{
		UE_LOG(LogTCModelCompare, Error, TEXT("usage: -run=TCModelCompare -Cameras=<cameras.json> -Out=<dir> (could not read '%s')"), *CamerasPath);
		return 1;
	}
	IFileManager::Get().MakeDirectory(*OutDir, true);
	FTCUnitTable Table;
	FString Error;
	if (!Table.Load(Error)) { UE_LOG(LogTCModelCompare, Error, TEXT("unit table: %s"), *Error); return 1; }
	const int32 W = (int32)Num(Doc, TEXT("width"), 1280), H = (int32)Num(Doc, TEXT("height"), 720);
	const TSharedPtr<FJsonObject>* LightPtr;
	TSharedPtr<FJsonObject> Light = Doc->TryGetObjectField(TEXT("light"), LightPtr) ? *LightPtr : MakeShared<FJsonObject>();

	FScratchWorld Scratch(TEXT("TCModelCompare"));
	UWorld* World = Scratch.World;
	UObject* Outer = World->GetWorldSettings();
	ATCUnitRenderer* Renderer = World->SpawnActor<ATCUnitRenderer>(ATCUnitRenderer::StaticClass(), FTransform::Identity);
	FString Batches;
	if (!FParse::Value(*Params, TEXT("TCModels="), Batches)) Renderer->SetEnabledBatches((1u << ATCUnitRenderer::BatchCount) - 1);

	// -GameLight: the game's own light instead of the viewer's (ATCGameMode::SpawnScene: the engine-default sun at
	// (-50, 35, 0) lighting a sky atmosphere, a real-time sky light, auto exposure). Shots get a "-gamelight" suffix.
	const bool bGameLight = FParse::Param(*Params, TEXT("GameLight"));
	// the reference viewer's scene (client/viewer.js): the sun and sky fill of the default mood, grass, a 2 m grid
	const TArray<TSharedPtr<FJsonValue>>* SunDir;
	FVector Sun(0.5, -0.5, 0.7);
	if (Light->TryGetArrayField(TEXT("sunDir"), SunDir)) Sun = FromReference(*SunDir).GetSafeNormal();
	UDirectionalLightComponent* SunLight = NewObject<UDirectionalLightComponent>(Outer);
	USkyLightComponent* Sky = NewObject<USkyLightComponent>(Outer);
	if (bGameLight)
	{
		SunLight->SetWorldRotation(FRotator(-50.f, 35.f, 0.f));
		SunLight->SetAtmosphereSunLight(true);
		SunLight->RegisterComponentWithWorld(World);
		USkyAtmosphereComponent* Atmosphere = NewObject<USkyAtmosphereComponent>(Outer);
		Atmosphere->RegisterComponentWithWorld(World);
		Sky->SetRealTimeCapture(true);
	}
	else
	{
		SunLight->SetWorldRotation((-Sun).Rotation());
		SunLight->SetLightColor(Hex(Light, TEXT("sun"), 0xffffff));
		SunLight->SetIntensity(Num(Light, TEXT("sunLux"), 6.0));
		SunLight->RegisterComponentWithWorld(World);
		Sky->SourceType = SLS_SpecifiedCubemap;
		Sky->Cubemap = LoadObject<UTextureCube>(nullptr, TEXT("/Engine/MapTemplates/Sky/DaylightAmbientCubemap.DaylightAmbientCubemap"));
		Sky->SetLightColor(Hex(Light, TEXT("hemiTop"), 0xffffff));
		Sky->SetIntensity(Num(Light, TEXT("skyIntensity"), 1.0));
	}
	Sky->RegisterComponentWithWorld(World);
	Sky->RecaptureSky();
	USkyLightComponent::UpdateSkyCaptureContents(World); // nothing ticks in a commandlet: process the capture now
	// no haze: the reference views keep it off the model (viewer.js fog.near = 1e4 m); its sky color is not reproduced
	// either (nothing is drawn behind the ground's horizon here), see README.md
	UMaterialInterface* Basic = LoadObject<UMaterialInterface>(nullptr, TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	auto Flat = [&](const FLinearColor& C) { UMaterialInstanceDynamic* M = UMaterialInstanceDynamic::Create(Basic, Outer); M->SetVectorParameterValue(TEXT("Color"), C); return M; };
	UStaticMeshComponent* Ground = NewObject<UStaticMeshComponent>(Outer);
	Ground->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Plane.Plane")));
	Ground->SetWorldTransform(FTransform(FQuat::Identity, FVector::ZeroVector, FVector(600, 600, 1)));
	Ground->SetMaterial(0, Flat(Hex(Light, TEXT("ground"), 0x6c6c45)));
	Ground->RegisterComponentWithWorld(World);
	// GridHelper(48, 24): lines every 2 m over 48 m, 2 cm over the ground, at 0.45 opacity (blended into one flat color)
	UInstancedStaticMeshComponent* Grid = NewObject<UInstancedStaticMeshComponent>(Outer);
	Grid->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Cube.Cube")));
	Grid->SetCastShadow(false);
	const FLinearColor GroundC = Hex(Light, TEXT("ground"), 0x6c6c45), LineC = Hex(Light, TEXT("grid"), 0x56623a);
	Grid->SetMaterial(0, Flat(FMath::Lerp(GroundC, LineC, 0.45f)));
	for (int32 I = 0; I <= 24; I++)
	{
		const double O = (I - 12) * 200.0;
		Grid->AddInstance(FTransform(FQuat::Identity, FVector(O, 0, 2), FVector(0.03, 48.0, 0.01)));
		Grid->AddInstance(FTransform(FQuat::Identity, FVector(0, O, 2), FVector(48.0, 0.03, 0.01)));
	}
	Grid->RegisterComponentWithWorld(World);

	TArray<TSharedPtr<FJsonValue>> Reports;
	int32 Failures = 0;
	for (const TSharedPtr<FJsonValue>& AV : Doc->GetArrayField(TEXT("assets")))
	{
		const TSharedPtr<FJsonObject> A = AV->AsObject();
		const FString Name = A->GetStringField(TEXT("name")), Type = A->GetStringField(TEXT("type"));
		const int32 Fac = (int32)Num(A, TEXT("fac"), 0), Owner = (int32)Num(A, TEXT("owner"), 0);
		FTCMatchState State;
		State.bHasMatch = true;
		State.Factions.Init(Fac, Owner + 1);
		State.Units.Add(1, MakeUnit(1, Type, Owner, 0.f, 0.f, Table));
		Renderer->Reset();
		Renderer->SetViewPoint(FVector(0, 3000, 3000)); // viewer.js nearEye: 30 m up and back, inside LOD
		Renderer->Draw(State, Table, nullptr, TSet<int32>(), 0.f);
		FinishCompiling();
		Renderer->Draw(State, Table, nullptr, TSet<int32>(), 0.f);
		const FString Status = Renderer->ModelStatusLine();
		UE_LOG(LogTCModelCompare, Display, TEXT("COMPARE %s (%s fac %d owner %d): %s"), *Name, *Type, Fac, Owner, *Status);
		TSharedRef<FJsonObject> R = MakeShared<FJsonObject>();
		R->SetStringField(TEXT("name"), Name);
		R->SetStringField(TEXT("renderer"), Status);
		R->SetNumberField(TEXT("modelUnits"), Renderer->ModelStats().ModelUnits);
		if (Renderer->ModelStats().ModelUnits != 1) { Failures++; UE_LOG(LogTCModelCompare, Error, TEXT("COMPARE %s: not drawn with its model"), *Name); }
		TArray<TSharedPtr<FJsonValue>> Shots;
		for (const TSharedPtr<FJsonValue>& VV : A->GetArrayField(TEXT("views")))
		{
			const TSharedPtr<FJsonObject> V = VV->AsObject();
			const FString Label = V->GetStringField(TEXT("label"));
			const FVector Pos = FromReference(V->GetArrayField(TEXT("pos"))), Target = FromReference(V->GetArrayField(TEXT("target")));
			const double VFov = Num(V, TEXT("fov"), 30.0);
			const double HFov = FMath::RadiansToDegrees(2.0 * FMath::Atan(FMath::Tan(FMath::DegreesToRadians(VFov) / 2.0) * W / H));
			UTextureRenderTarget2D* Target2D = NewObject<UTextureRenderTarget2D>(Outer);
			Target2D->RenderTargetFormat = RTF_RGBA8_SRGB;
			Target2D->ClearColor = Hex(Light, TEXT("haze"), 0xb8c4cc);
			Target2D->InitAutoFormat(W, H);
			Target2D->UpdateResourceImmediate(true);
			USceneCaptureComponent2D* Capture = NewObject<USceneCaptureComponent2D>(Outer);
			Capture->TextureTarget = Target2D;
			Capture->CaptureSource = SCS_FinalColorLDR;
			Capture->bCaptureEveryFrame = false;
			Capture->bCaptureOnMovement = false;
			Capture->FOVAngle = HFov;
			Capture->PostProcessSettings.bOverride_AutoExposureMethod = !bGameLight;
			Capture->PostProcessSettings.AutoExposureMethod = AEM_Manual;
			Capture->PostProcessSettings.bOverride_AutoExposureBias = !bGameLight;
			Capture->PostProcessSettings.AutoExposureBias = Num(Light, TEXT("exposureBias"), 1.0);
			Capture->PostProcessSettings.bOverride_AutoExposureApplyPhysicalCameraExposure = !bGameLight;
			Capture->PostProcessSettings.AutoExposureApplyPhysicalCameraExposure = false;
			// no Lumen in a one-off capture (it has no history to build its scene from): the classic reflection environment, so
			// metal reflects the sky light
			Capture->PostProcessSettings.bOverride_ReflectionMethod = true;
			Capture->PostProcessSettings.ReflectionMethod = EReflectionMethod::ScreenSpace;
			Capture->PostProcessSettings.bOverride_DynamicGlobalIlluminationMethod = true;
			Capture->PostProcessSettings.DynamicGlobalIlluminationMethod = EDynamicGlobalIlluminationMethod::None;
			Capture->PostProcessBlendWeight = 1.f;
			Capture->SetWorldLocationAndRotation(Pos, (Target - Pos).Rotation());
			Capture->RegisterComponentWithWorld(World);
			for (int32 F = 0; F < (bGameLight ? 16 : 4); F++) { World->SendAllEndOfFrameUpdates(); Capture->CaptureScene(); FlushRenderingCommands(); }
			TArray<FColor> Pixels;
			FTextureRenderTargetResource* Resource = Target2D->GameThread_GetRenderTargetResource();
			const FString Png = FPaths::Combine(OutDir, FString::Printf(TEXT("%s-%s-unreal%s.png"), *Name, *Label, bGameLight ? TEXT("-gamelight") : TEXT("")));
			if (Resource && Resource->ReadPixels(Pixels) && Pixels.Num() == W * H)
			{
				for (FColor& C : Pixels) C.A = 255;
				TArray64<uint8> Bytes;
				FImageUtils::PNGCompressImageArray(W, H, TArrayView64<const FColor>(Pixels.GetData(), Pixels.Num()), Bytes);
				FFileHelper::SaveArrayToFile(Bytes, *Png);
				Shots.Add(MakeShared<FJsonValueString>(Png));
			}
			else { Failures++; UE_LOG(LogTCModelCompare, Error, TEXT("COMPARE %s %s: capture unreadable"), *Name, *Label); }
			Capture->DestroyComponent();
		}
		R->SetArrayField(TEXT("shots"), Shots);
		Reports.Add(MakeShared<FJsonValueObject>(R));
	}
	TSharedRef<FJsonObject> Report = MakeShared<FJsonObject>();
	Report->SetArrayField(TEXT("assets"), Reports);
	Report->SetNumberField(TEXT("failures"), Failures);
	FString Json;
	FJsonSerializer::Serialize(Report, TJsonWriterFactory<>::Create(&Json));
	FFileHelper::SaveStringToFile(Json, *FPaths::Combine(OutDir, TEXT("unreal-report.json")));
	UE_LOG(LogTCModelCompare, Display, TEXT("COMPARE %s: %d failures"), Failures ? TEXT("FAIL") : TEXT("PASS"), Failures);
	return Failures ? 1 : 0;
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

// The renderer's model path against the imported content: which units take it, the instances it makes for each, the
// level-of-detail swap, the turret traverse, owner tint and the build-up scale. Runs under -nullrhi (meshes load and
// instance; nothing is rendered). Skips with a note when Content/Models has not been imported.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCUnitModelsRendererTest, "ThreeCrossroads.Models.Renderer", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCUnitModelsRendererTest::RunTest(const FString& Parameters)
{
	const FTCGraphicsLevelScope HighLevel(ETCGraphicsLevel::High); // round 15 (gfxlow): the expectations are High's (shake, LOD 110 m), not the saved level's
	using namespace TCModelCompareImpl;
	// adoption: every unit type of the pinned UNITS table belongs to exactly one batch, the pilot first
	static const TCHAR* Types[] = { TEXT("rifle"), TEXT("mg"), TEXT("at"), TEXT("tank"), TEXT("rocket"), TEXT("flak"), TEXT("fighter"), TEXT("attacker"), TEXT("flaktrack"), TEXT("mortar"), TEXT("sniper"),
		TEXT("armoredcar"), TEXT("medium"), TEXT("halftrack"), TEXT("lorry"), TEXT("train"), TEXT("lcvp"), TEXT("gunboat"), TEXT("destroyer"), TEXT("medic"), TEXT("tankdestroyer"), TEXT("howitzer"),
		TEXT("flamer"), TEXT("bomber"), TEXT("ranger"), TEXT("tiger"), TEXT("conscript"), TEXT("churchill"), TEXT("commando"), TEXT("bunker"), TEXT("kaiju"), TEXT("engineer"), TEXT("hq"), TEXT("depot"),
		TEXT("barracks"), TEXT("motorpool"), TEXT("airfield"), TEXT("shipyard"), TEXT("flakpos"), TEXT("worldbase"), TEXT("truck"), TEXT("armory"), TEXT("pillbox"), TEXT("tower"), TEXT("wall"), TEXT("gate"), TEXT("supplycache") };
	for (const TCHAR* T : Types) TestTrue(FString::Printf(TEXT("%s has a batch"), T), ATCUnitRenderer::BatchOf(T) != INDEX_NONE);
	TestEqual(TEXT("Sherman in the pilot"), ATCUnitRenderer::BatchOf(TEXT("medium")), (int32)ATCUnitRenderer::BatchPilot);
	TestEqual(TEXT("AT gun in the pilot"), ATCUnitRenderer::BatchOf(TEXT("at")), (int32)ATCUnitRenderer::BatchPilot);
	TestEqual(TEXT("rifles in the pilot"), ATCUnitRenderer::BatchOf(TEXT("rifle")), (int32)ATCUnitRenderer::BatchPilot);
	TestEqual(TEXT("barracks in the pilot"), ATCUnitRenderer::BatchOf(TEXT("barracks")), (int32)ATCUnitRenderer::BatchPilot);
	TestEqual(TEXT("unknown type"), ATCUnitRenderer::BatchOf(TEXT("zeppelin")), (int32)INDEX_NONE);

	FTCUnitTable Table;
	FString Error;
	if (!Table.Load(Error)) { AddError(TEXT("unit table: ") + Error); return false; }
	FScratchWorld Scratch(TEXT("TCUnitModelsTest"));
	ATCUnitRenderer* R = Scratch.World->SpawnActor<ATCUnitRenderer>(ATCUnitRenderer::StaticClass(), FTransform::Identity);
	if (!TestNotNull(TEXT("renderer"), R)) return false;
	R->SetEnabledBatches(1u << ATCUnitRenderer::BatchPilot);
	FTCMatchState State;
	State.bHasMatch = true;
	State.Factions = { 0, 1 };
	FTCUnit Sherman = MakeUnit(1, TEXT("medium"), 0, 0.f, 0.f, Table);
	Sherman.Aim = PI / 2; // turret turned a quarter left of the hull (reference sign)
	State.Units.Add(1, Sherman);
	State.Units.Add(2, MakeUnit(2, TEXT("at"), 0, 10.f, 0.f, Table));
	State.Units.Add(3, MakeUnit(3, TEXT("rifle"), 0, 20.f, 0.f, Table));
	FTCUnit Barracks = MakeUnit(4, TEXT("barracks"), 0, 40.f, 0.f, Table);
	Barracks.Built = 0.5f;
	State.Units.Add(4, Barracks);
	State.Units.Add(5, MakeUnit(5, TEXT("tank"), 1, 60.f, 0.f, Table)); // vehicles batch: off here, a placeholder
	FTCUnit Inside = MakeUnit(6, TEXT("rifle"), 0, 30.f, 0.f, Table);
	Inside.Flags = TCFlags::Garrisoned;
	State.Units.Add(6, Inside);
	R->SetViewPoint(FVector(1000, 3000, 3000));
	R->Draw(State, Table, nullptr, TSet<int32>(), 0.f);
	if (!R->ModelsLoaded())
	{
		AddWarning(TEXT("no imported models (") + R->ModelLoadError() + TEXT("): the model path is untested here; every unit fell back to a placeholder"));
		TestEqual(TEXT("all placeholders"), R->ModelStats().PlaceholderUnits, 5);
		return true;
	}
	const FTCUnitModelStats& S = R->ModelStats();
	AddInfo(R->ModelStatusLine());
	TestEqual(TEXT("pilot units drawn with models"), S.ModelUnits, 4);
	TestEqual(TEXT("the tank (not adopted) is a placeholder"), S.PlaceholderUnits, 1);
	TestEqual(TEXT("no adopted unit without a model"), S.MissingModel, 0);
	TestEqual(TEXT("owner rings under the four"), S.Rings, 4);
	const FTCModelEntry* Rifles = R->Models().Find(TEXT("rifle|0"));
	const FTCModelEntry* Gun = R->Models().Find(TEXT("at|0"));
	int32 Men = 0;
	for (const FTCModelEntry* E : { Rifles, Gun }) if (E) for (const FTCModelPart& P : E->Parts) if (P.Name.ToString().StartsWith(TEXT("man_")) && P.Name.ToString().EndsWith(TEXT("_near"))) Men++;
	TestTrue(TEXT("rifle and AT models in the catalog"), Rifles && Gun && Men > 0);
	TestEqual(TEXT("every man near the camera is a skinned figure"), S.CrowdMen, Men);
	TestEqual(TEXT("no far men near the camera"), S.FarMen, 0);
	TestTrue(TEXT("figures baked"), S.Figures > 0);
	TestFalse(TEXT("squad near"), R->IsFar(3));

	// instances: the Sherman's turret turned by aim - rot, the hull painted with the owner's seat color
	FTransform Turret, Hull, Body;
	bool bTurret = false, bHull = false, bBody = false;
	TArray<float> HullData;
	for (UActorComponent* C : R->GetComponents())
	{
		UInstancedStaticMeshComponent* Ism = Cast<UInstancedStaticMeshComponent>(C);
		if (!Ism || !Ism->GetStaticMesh() || !Ism->GetInstanceCount()) continue;
		const FString Mesh = Ism->GetStaticMesh()->GetName();
		if (Mesh == TEXT("SM_medium_0_turret")) bTurret = Ism->GetInstanceTransform(0, Turret, true);
		// the Sherman's hull is its copy with the reference's wheel and track data when unreal/Tools/motion/make-wheel-tracks.py
		// made one (two more floats: wheel-motion.js wheelTravel), else the catalog's
		if (Mesh == TEXT("SM_medium_0_hull") || Mesh == TEXT("SM_medium_0_hull_Tracks")) { bHull = Ism->GetInstanceTransform(0, Hull, true); HullData = TArray<float>(Ism->PerInstanceSMCustomData.GetData(), Ism->NumCustomDataFloats); }
		if (Mesh == TEXT("SM_barracks_0_body")) bBody = Ism->GetInstanceTransform(0, Body, true);
	}
	TestTrue(TEXT("Sherman hull and turret instanced"), bTurret && bHull);
	TestTrue(TEXT("turret faces +Y (a quarter turn)"), Turret.GetRotation().RotateVector(FVector::ForwardVector).Equals(FVector(0, 1, 0), 1e-3));
	TestTrue(TEXT("hull faces +X"), Hull.GetRotation().RotateVector(FVector::ForwardVector).Equals(FVector(1, 0, 0), 1e-3));
	TestTrue(TEXT("turret on its ring above the hull"), Turret.GetLocation().Z > 100.0 && FMath::Abs(Turret.GetLocation().X - 15.0) < 1.0);
	const FLinearColor Seat0 = R->Models().SeatColor(0);
	TestTrue(TEXT("hull custom data: the catalog's floats, plus travel and turn on the wheel copy"), HullData.Num() == TCModelData::Count || (R->WheelTracksLoaded() && HullData.Num() == FTCWheelTracks::Floats));
	if (HullData.Num() == FTCWheelTracks::Floats) TestTrue(TEXT("a hull that has not moved has rolled nothing"), HullData[FTCWheelTracks::TravelFloat] == 0.f && HullData[FTCWheelTracks::TurnFloat] == 0.f);
	TestTrue(TEXT("hull custom data carries the owner's color"), HullData.Num() >= TCModelData::Count && FMath::IsNearlyEqual(HullData[TCModelData::OwnerR], Seat0.R, 1e-5f) && FMath::IsNearlyEqual(HullData[TCModelData::OwnerB], Seat0.B, 1e-5f));
	TestTrue(TEXT("hull custom data carries the US vehicle paint"), HullData.Num() >= TCModelData::Count && FMath::IsNearlyEqual(HullData[TCModelData::VehicleR], R->Models().VehiclePaint(0).R, 1e-5f));
	TestTrue(TEXT("barracks half built: 0.575 of its height"), bBody && FMath::IsNearlyEqual(Body.GetScale3D().Z, 0.575, 1e-4));

	// far: the squad and the gun crew swap to the far figure, the gun to its far mesh
	R->SetViewPoint(FVector(0, 0, 50000));
	R->Draw(State, Table, nullptr, TSet<int32>(), 0.f);
	TestTrue(TEXT("squad far"), R->IsFar(3));
	TestEqual(TEXT("far: no skinned men"), R->ModelStats().CrowdMen, 0);
	TestEqual(TEXT("far: every man is a far figure"), R->ModelStats().FarMen, Men);
	// selection adds a ring; hysteresis: 4 m inside LOD still far, 4 m further in near again
	TSet<int32> Selected;
	Selected.Add(1);
	R->Draw(State, Table, nullptr, Selected, 0.f);
	TestEqual(TEXT("selection ring"), R->ModelStats().Rings, 5);
	R->SetViewPoint(FVector(2000, 0, (ATCUnitRenderer::LodMetres - 2.f) * 100.f));
	R->Draw(State, Table, nullptr, TSet<int32>(), 0.f);
	TestTrue(TEXT("2 m inside LOD: still far (hysteresis)"), R->IsFar(3));
	R->SetViewPoint(FVector(2000, 0, (ATCUnitRenderer::LodMetres - 6.f) * 100.f));
	R->Draw(State, Table, nullptr, TSet<int32>(), 0.f);
	TestFalse(TEXT("6 m inside LOD: near again"), R->IsFar(3));
	// losses: a squad at a third of its health shows a third of its men
	const FTCUnitDef* RifleDef = Table.Find(TEXT("rifle"));
	if (RifleDef && Rifles)
	{
		State.Units[3].Hp = RifleDef->MaxHp() / 3;
		R->Draw(State, Table, nullptr, TSet<int32>(), 0.f);
		int32 RifleMen = 0;
		for (const FTCModelPart& P : Rifles->Parts) if (P.Name.ToString().EndsWith(TEXT("_near")) && P.Name.ToString().StartsWith(TEXT("man_"))) RifleMen++;
		TestEqual(TEXT("a third of the riflemen"), R->ModelStats().CrowdMen, Men - RifleMen + (int32)FMath::CeilToDouble((double)(RifleDef->MaxHp() / 3) * RifleMen / RifleDef->MaxHp()));
	}
	// every batch on: the tank takes its model too
	R->SetEnabledBatches((1u << ATCUnitRenderer::BatchCount) - 1);
	R->Draw(State, Table, nullptr, TSet<int32>(), 0.f);
	TestEqual(TEXT("all batches: no placeholders"), R->ModelStats().PlaceholderUnits, 0);
	R->Destroy();
	return true;
}
// Round 17 (backlog 1b a): a spectator's renderer. Round 16's Conquest spectator pose showed none of the AI's moving
// squads: they were drawn (the status line counted them) but written without previous transforms, so the temporal
// anti-aliasing smeared them into the ground. A spectator's state (no seat, every seat's units, fog lifted) draws every
// unit with its model and owner ring, and a unit that moved between two frames carries last frame's transform on every
// instance (parts, men, ring), while one that stood still carries none.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCUnitModelsSpectatorTest, "ThreeCrossroads.Models.SpectatorMotion", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCUnitModelsSpectatorTest::RunTest(const FString& Parameters)
{
	const FTCGraphicsLevelScope HighLevel(ETCGraphicsLevel::High);
	using namespace TCModelCompareImpl;
	FTCUnitTable Table;
	FString Error;
	if (!Table.Load(Error)) { AddError(TEXT("unit table: ") + Error); return false; }
	FScratchWorld Scratch(TEXT("TCUnitModelsSpectatorTest"));
	ATCUnitRenderer* R = Scratch.World->SpawnActor<ATCUnitRenderer>(ATCUnitRenderer::StaticClass(), FTransform::Identity);
	if (!TestNotNull(TEXT("renderer"), R)) return false;
	FTCMatchState State;
	State.bHasMatch = true;
	State.bSpectator = true;
	State.You = -1;
	State.bFogLifted = true;
	State.Factions = { 0, 1 };
	// seat 0's idle Sherman and squad, seat 1's (the AI's) squad and tank on the move across the centre
	State.Units.Add(1, MakeUnit(1, TEXT("medium"), 0, 0.f, 0.f, Table));
	State.Units.Add(2, MakeUnit(2, TEXT("rifle"), 0, 10.f, 0.f, Table));
	State.Units.Add(3, MakeUnit(3, TEXT("rifle"), 1, 40.f, 40.f, Table));
	State.Units.Add(4, MakeUnit(4, TEXT("tank"), 1, 60.f, 40.f, Table));
	R->SetViewPoint(FVector(2500, 2000, 6000)); // 60 m up: the squads near (LOD 110 m)
	R->Draw(State, Table, nullptr, TSet<int32>(), 1.f);
	if (!R->ModelsLoaded())
	{
		AddWarning(TEXT("no imported models (") + R->ModelLoadError() + TEXT("): untested here"));
		return true;
	}
	TestEqual(TEXT("a spectator draws every seat's units with models"), R->ModelStats().ModelUnits, 4);
	TestEqual(TEXT("no placeholders"), R->ModelStats().PlaceholderUnits, 0);
	TestEqual(TEXT("an owner ring under each"), R->ModelStats().Rings, 4);
	TestTrue(TEXT("men drawn"), R->ModelStats().CrowdMen > 0);
	TestEqual(TEXT("first frame: nothing moved yet"), R->ModelStats().Moving, 0);
	// where every instance was in the first frame, by component
	auto Snapshot = [R]()
	{
		TMap<UInstancedStaticMeshComponent*, TArray<FTransform>> Out;
		for (UActorComponent* C : R->GetComponents())
			if (UInstancedStaticMeshComponent* Ism = Cast<UInstancedStaticMeshComponent>(C))
				if (Ism->IsVisible() && Ism->GetInstanceCount())
					for (int32 I = 0; I < Ism->GetInstanceCount(); I++) { FTransform T; Ism->GetInstanceTransform(I, T, true); Out.FindOrAdd(Ism).Add(T); }
		return Out;
	};
	const TMap<UInstancedStaticMeshComponent*, TArray<FTransform>> Before = Snapshot();
	// the AI's two units move 3 m east (the snapshot's next rows); seat 0's stand still
	State.Units[3].X += 3.f;
	State.Units[4].X += 3.f;
	R->Draw(State, Table, nullptr, TSet<int32>(), 1.f);
	AddInfo(R->ModelStatusLine());
	TestTrue(TEXT("the moving units' instances carry motion"), R->ModelStats().Moving > 0);
	TestTrue(TEXT("every instance matched last frame's"), R->ModelStats().Matched >= R->ModelStats().Moving);
	// each drawn instance: its previous transform is one of last frame's places of that component (matched by its key,
	// never a teleport to its own place when it moved); the tank's rigid parts came from exactly 3 m west
	int32 Moved = 0, Still = 0, Without = 0, Unseen = 0, Rigid3m = 0;
	for (const auto& P : Snapshot())
	{
		const TArray<FTransform>* Old = Before.Find(P.Key);
		for (int32 I = 0; I < P.Value.Num(); I++)
		{
			FTransform Prev;
			if (!P.Key->GetInstancePrevTransform(I, Prev, true)) { Without++; continue; }
			const FVector D = P.Value[I].GetLocation() - Prev.GetLocation();
			if (D.Size2D() < 1.0) { Still++; continue; }
			Moved++;
			bool bSeen = false;
			if (Old) for (const FTransform& O : *Old) if (O.GetLocation().Equals(Prev.GetLocation(), 1.0)) bSeen = true;
			if (!bSeen) Unseen++;
			if (FMath::Abs(D.X - 300.0) < 5.0 && FMath::Abs(D.Y) < 5.0) Rigid3m++;
		}
	}
	AddInfo(FString::Printf(TEXT("instances moved %d (%d exactly 3 m east), still %d, without a previous transform %d, from no place of last frame %d"), Moved, Rigid3m, Still, Without, Unseen));
	TestEqual(TEXT("every drawn instance has a previous transform"), Without, 0);
	TestTrue(TEXT("the moving squad, tank and rings moved"), Moved > 0);
	TestTrue(TEXT("the tank's parts and the rings moved exactly 3 m east"), Rigid3m >= 2);
	TestTrue(TEXT("seat 0's idle units stood still"), Still > 0);
	TestEqual(TEXT("each moved instance came from a place it had last frame"), Unseen, 0);
	// a third frame without movement: the tank's parts carry no motion vector any more
	R->Draw(State, Table, nullptr, TSet<int32>(), 1.f);
	int32 Lingering = 0;
	for (const auto& P : Snapshot())
		for (int32 I = 0; I < P.Value.Num(); I++) { FTransform Prev; if (P.Key->GetInstancePrevTransform(I, Prev, true) && FMath::Abs((P.Value[I].GetLocation() - Prev.GetLocation()).X - 300.0) < 5.0) Lingering++; }
	TestEqual(TEXT("standing again: nothing still carries the 3 m step"), Lingering, 0);
	R->Reset();
	R->Destroy();
	return true;
}
// Round 17 (backlog 1b d): the tour lines both halves of a side by side read, and the Tutorial's step poses: a step:<n>
// pose waits for the snapshot's step and centres where shared/tutorial.js STEPS[n - 1].at points (round 16's native
// spectator tour put step:1 at the origin; the browser at (27, 45)).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTourPoseTest, "ThreeCrossroads.Compare.TourPoses", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCTourPoseTest::RunTest(const FString& Parameters)
{
	FTCTourPose P;
	TestFalse(TEXT("a comment"), TCTour::ParseLine(TEXT("# name seconds x z distance yaw"), P));
	TestFalse(TEXT("too short"), TCTour::ParseLine(TEXT("a 1 2 3 4"), P));
	TestTrue(TEXT("plain"), TCTour::ParseLine(TEXT("conquest-centre-60 60 80 80 90 -90 hud # the centre"), P));
	TestTrue(TEXT("plain fields"), P.Name == TEXT("conquest-centre-60") && P.At == 60 && P.X == 80 && P.Z == 80 && P.Dist == 90 && P.Yaw == -90 && P.bHud && !P.bWideShare && P.Step == 0 && P.UnitType.IsEmpty());
	TestTrue(TEXT("paused pose"), TCTour::ParseLine(TEXT("frozen p95 80 80 60 -90"), P) && P.bWhenPaused && P.At == 95);
	TestTrue(TEXT("plain pose is not paused"), TCTour::ParseLine(TEXT("moving 95 80 80 60 -90"), P) && !P.bWhenPaused);
	TestTrue(TEXT("wide share"), TCTour::ParseLine(TEXT("w 10 1 2 w0.5 0"), P) && P.bWideShare && P.Dist == 0.5 && !P.bHud);
	TestTrue(TEXT("frontier"), TCTour::ParseLine(TEXT("f 10 frontier 0 60 -90"), P) && P.bFrontier);
	TestTrue(TEXT("unit of the viewer's seat"), TCTour::ParseLine(TEXT("u 10 unit:medium 0 40 -90"), P) && P.UnitType == TEXT("medium") && P.UnitSeat == -1);
	TestEqual(TEXT("a spectator follows seat 0"), TCTour::UnitSeatFor(P, true, -1), 0);
	TestEqual(TEXT("a player its own"), TCTour::UnitSeatFor(P, false, 2), 2);
	TestTrue(TEXT("unit of seat 1"), TCTour::ParseLine(TEXT("u 10 unit:rifle@1 0 40 -90"), P) && P.UnitType == TEXT("rifle") && P.UnitSeat == 1);
	TestEqual(TEXT("seat 1 for anyone"), TCTour::UnitSeatFor(P, true, -1), 1);
	TestTrue(TEXT("step"), TCTour::ParseLine(TEXT("tutorial-step1 20 step:1 0 60 -90 hud"), P) && P.Step == 1 && P.X == 0 && P.bHud);
	FVector2D At;
	TestTrue(TEXT("step 1 has a cell"), TCTour::StepFocus(1, At));
	TestTrue(TEXT("step 1: STEPS[0].at [13, 22] is (27, 45) m, where the browser tour framed it"), At.Equals(FVector2D(27, 45)));
	TestTrue(TEXT("step 9: [62, 22] is (125, 45) m"), TCTour::StepFocus(9, At) && At.Equals(FVector2D(125, 45)));
	TestFalse(TEXT("no step 10"), TCTour::StepFocus(10, At));
	TestFalse(TEXT("no step 0"), TCTour::StepFocus(0, At));
	TestFalse(TEXT("before the tutorial (step -1)"), TCTour::StepReached(1, -1));
	TestTrue(TEXT("step 1 at mode.step 0"), TCTour::StepReached(1, 0));
	TestFalse(TEXT("step 3 at mode.step 1"), TCTour::StepReached(3, 1));
	TestTrue(TEXT("step 3 past"), TCTour::StepReached(3, 4));
	FTCMatchState State;
	FTCUnit A; A.Id = 9; A.Type = TEXT("rifle"); A.Owner = 1; A.Hp = 50; A.X = 5; A.Z = 6;
	FTCUnit B = A; B.Id = 4; B.X = 7; B.Z = 8;
	FTCUnit C = A; C.Id = 2; C.Owner = 0;
	FTCUnit D = A; D.Id = 1; D.Hp = 0;
	State.Units.Add(9, A); State.Units.Add(4, B); State.Units.Add(2, C); State.Units.Add(1, D);
	TCTour::ParseLine(TEXT("u 10 unit:rifle@1 0 40 -90"), P);
	TestEqual(TEXT("the lowest living id of seat 1"), TCTour::PickUnit(P, State, 1, At), 4);
	TestTrue(TEXT("its place"), At.Equals(FVector2D(7, 8)));
	TestEqual(TEXT("seat 2 has none"), TCTour::PickUnit(P, State, 2, At), -1);
	TCTour::ParseLine(TEXT("v 10 unit:vet 0 40 -90"), P);
	TestEqual(TEXT("unit:vet: nobody has a star yet"), TCTour::PickUnit(P, State, 0, At), -1);
	State.Units[9].Stars = 1; State.Units[2].Stars = 2; State.Units[4].Stars = 2; State.Units[1].Stars = 3;
	TestEqual(TEXT("unit:vet: the most stars of any seat, the lowest id among equals, not a dead one"), TCTour::PickUnit(P, State, 1, At), 2);
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
