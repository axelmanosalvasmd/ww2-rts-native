// The original cover/cloud meshes draw before main.js/apron.js fog meshes.
// Check the actual carrier pass, component order and MID, then actual HDR pixels.
// This does not accept a public-map screenshot or Low output-space composition.
#if WITH_DEV_AUTOMATION_TESTS
#include "World/TCAtmosphere.h"
#include "World/TCFogFrontier.h"
#include "World/TCTerrain.h"
#include "Net/TCMatchState.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Engine/Engine.h"
#include "Engine/Texture2D.h"
#include "Engine/TextureRenderTarget2D.h"
#include "Engine/World.h"
#include "GameFramework/WorldSettings.h"
#include "Materials/Material.h"
#include "MaterialShared.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/AutomationTest.h"
#include "ProceduralMeshComponent.h"
#include "RHI.h"
#include "RenderingThread.h"
#include "TextureResource.h"
#if WITH_EDITOR
#include "AssetCompilingManager.h"
#include "ShaderCompiler.h"
#endif

namespace TCWorld27GroundTests
{
	struct FWorld
	{
		UWorld* World = nullptr;
		FWorld()
		{
			World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("World27Ground"));
			if (!World) return;
			GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World);
			World->InitializeActorsForPlay(FURL());
		}
		~FWorld()
		{
			if (World) { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); }
		}
	};
	FTCMatchState SnowState()
	{
		FTCMatchState S;
		S.bHasMatch = true; S.Map.W = S.Map.H = 8;
		S.Map.Name = TEXT("Ardennes Crossing"); S.Weather.Now = TEXT("snow");
		S.Map.Chars.Init('.', 64); S.Map.Levels.Init(0, 64); S.Map.Known.Init(true, 64);
		S.FogVisible.Init(false, 64); S.FogExplored.Init(true, 64);
		return S;
	}
	struct FActiveFog
	{
		const FTCFogFrontier* Previous = TCFogLook::Active();
		FTCFogFrontier Fog;
		FActiveFog(UWorld* World, const FTCMatchState& State)
		{
			Fog.Attach(World); Fog.Start(State); TCFogLook::SetActive(&Fog); Fog.Frame(0, false, State);
		}
		~FActiveFog() { TCFogLook::SetActive(Previous); Fog.Detach(); }
	};
	void FinishShaders()
	{
#if WITH_EDITOR
		FAssetCompilingManager::Get().FinishAllCompilation();
		if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();
#endif
		FlushRenderingCommands();
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld27GroundLayerTest, "ThreeCrossroads.World.GroundWeatherLayers", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld27GroundLayerTest::RunTest(const FString& Parameters)
{
	using namespace TCWorld27GroundTests;
	FWorld Scratch;
	if (!TestNotNull(TEXT("scratch world"), Scratch.World)) return false;
	FTCMatchState State = SnowState();
	FActiveFog Shared(Scratch.World, State);
	ATCTerrain* Terrain = Scratch.World->SpawnActor<ATCTerrain>();
	if (!TestNotNull(TEXT("ground"), Terrain)) return false;
	Terrain->ResetFrom(State);
	while (Terrain->ChunksQueued()) Terrain->Flush(State);
	// The same extra actor path used by the outer apron. Offset bounds must not
	// change the required order of fog versus snow.
	AActor* Outer = Scratch.World->SpawnActor<AActor>();
	if (!TestNotNull(TEXT("outer ground actor"), Outer)) return false;
	UProceduralMeshComponent* Extra = NewObject<UProceduralMeshComponent>(Outer);
	Outer->SetRootComponent(Extra); Outer->AddInstanceComponent(Extra); Extra->RegisterComponent();
	const TArray<FVector> P = { FVector(-800,0,0), FVector(0,0,0), FVector(-800,1600,0), FVector(0,1600,0) };
	const TArray<FVector> N = { FVector::UpVector, FVector::UpVector, FVector::UpVector, FVector::UpVector };
	Extra->CreateMeshSection_LinearColor(0, P, {0,2,3,0,3,1}, N, {}, {}, {}, false);
	FTCFogOverlay Overlay; Overlay.Attach(Scratch.World); Overlay.SetSection(0, P, {0,2,3,0,3,1}, N); Overlay.Flush(State, true);
	if (!TestNotNull(TEXT("outer fog carrier"), Overlay.MeshComponent()) || !TestTrue(TEXT("outer fog material loads"), Overlay.Ready()))
	{ Overlay.Detach(); return false; }
	TestTrue(TEXT("shared fog uses the before-DOF pass"), Overlay.MeshComponent()->GetMaterial(0)->GetMaterial()->TranslucencyPass == MTP_BeforeDOF);
	for (ETCGraphicsLevel Level : {ETCGraphicsLevel::High, ETCGraphicsLevel::Low})
	{
		FTCGraphicsLevelScope Graphics(Level);
		FTCAtmosphere View;
		if (!TestTrue(TEXT("atmosphere attaches"), View.Attach(Scratch.World))) continue;
		View.AddGroundActor(Outer); View.StartMatch(State, TEXT("ardennes-crossing"), Terrain);
		View.SetHazeDistances(10000, 20000);
		View.Update(0.f, State, Terrain, nullptr, FVector2D(8,8));
		if (TestTrue(TEXT("original atmosphere materials load"), View.MaterialsLoaded()))
		{
			TestTrue(TEXT("weather ground and fog share the before-DOF pass"), View.GroundMat->GetMaterial()->TranslucencyPass == MTP_BeforeDOF);
			float FogOn = -1;
			TestTrue(TEXT("ground fog parameter exists"), View.GroundMat->GetScalarParameterValue(FMaterialParameterInfo(TEXT("FogOn")), FogOn));
			TestEqual(TEXT("common overlay shades snow once, without the legacy ground multiplier"), FogOn, 0.f);
			float Cover = -1;
			View.GroundMat->GetScalarParameterValue(FMaterialParameterInfo(TEXT("Cover")), Cover);
			TestEqual(TEXT("snow retains the original full cover goal"), Cover, 1.f);
		}
		TestTrue(TEXT("playable ground and outer ground are both mirrored"), View.Overlays.Num() >= 2);
		for (const auto& Pair : View.Overlays)
			if (const UProceduralMeshComponent* Ground = Pair.Value.Get())
				TestTrue(TEXT("snow/cloud carrier sorts strictly before the common fog regardless of bounds"), Ground->TranslucencySortPriority < Overlay.MeshComponent()->TranslucencySortPriority);
		// Keep the explicit no-common-overlay fallback. A later reattachment must
		// not depend on whether the fog texture's pixels happened to change.
		TCFogLook::SetActive(nullptr); View.Update(0.f, State, Terrain, nullptr, FVector2D(8,8));
		float Fallback = -1;
		if (View.GroundMat) View.GroundMat->GetScalarParameterValue(FMaterialParameterInfo(TEXT("FogOn")), Fallback);
		TestEqual(TEXT("legacy fog remains when there is no shared overlay"), Fallback, 1.f);
		TCFogLook::SetActive(&Shared.Fog); View.Update(0.f, State, Terrain, nullptr, FVector2D(8,8));
		if (View.GroundMat) View.GroundMat->GetScalarParameterValue(FMaterialParameterInfo(TEXT("FogOn")), Fallback);
		TestEqual(TEXT("shared overlay reattachment removes the duplicate multiplier"), Fallback, 0.f);
		View.Detach();
	}
	Overlay.Detach();
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld27GroundPixelsTest, "ThreeCrossroads.World.GroundWeatherPixels", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld27GroundPixelsTest::RunTest(const FString& Parameters)
{
	using namespace TCWorld27GroundTests;
	if (GUsingNullRHI) { AddError(TEXT("GroundWeatherPixels requires actual RHI pixels")); return false; }
	FTCGraphicsLevelScope High(ETCGraphicsLevel::High);
	FWorld Scratch;
	if (!TestNotNull(TEXT("scratch world"), Scratch.World)) return false;
	FTCMatchState State = SnowState();
	FActiveFog Shared(Scratch.World, State);
	ATCTerrain* Terrain = Scratch.World->SpawnActor<ATCTerrain>();
	if (!TestNotNull(TEXT("ground"), Terrain)) return false;
	Terrain->ResetFrom(State); while (Terrain->ChunksQueued()) Terrain->Flush(State);
	// A known unlit control isolates composition from sun, caster and atlas.
	UMaterialInterface* Plain = LoadObject<UMaterialInterface>(nullptr, FTCFogFrontier::UnknownMaterialPath);
	if (!TestNotNull(TEXT("unlit opaque control material"), Plain)) return false;
	UMaterialInstanceDynamic* Base = UMaterialInstanceDynamic::Create(Plain, Terrain);
	Base->SetVectorParameterValue(TEXT("Color"), FLinearColor(.18,.18,.18,1));
	TArray<UProceduralMeshComponent*> Ground;
	Terrain->GetComponents<UProceduralMeshComponent>(Ground);
	for (UProceduralMeshComponent* C : Ground) for (int S=0; S<C->GetNumSections(); S++) C->SetMaterial(S, Base);
	FTCAtmosphere View;
	TestTrue(TEXT("atmosphere attaches"), View.Attach(Scratch.World));
	View.StartMatch(State, TEXT("ardennes-crossing"), Terrain); View.SetHazeDistances(10000, 20000);
	View.Update(0.f, State, Terrain, nullptr, FVector2D(8,8));
	if (!TestNotNull(TEXT("snow material"), View.GroundMat.Get())) { View.Detach(); return false; }
	View.GroundMat->SetScalarParameterValue(TEXT("CloudOn"), 0.f);
	// Exact RGBA8 source-noise control: small=128/255, dry=1, normal.y=1,
	// road=0. Use the stored texel rather than rounding it to one half.
	TStrongObjectPtr<UTexture2D> Constant(UTexture2D::CreateTransient(1,1,PF_B8G8R8A8));
	FTexture2DMipMap& Mip = Constant->GetPlatformData()->Mips[0];
	void* Bytes = Mip.BulkData.Lock(LOCK_READ_WRITE);
	const FColor Half(128,128,128,128); FMemory::Memcpy(Bytes, &Half, 4); Mip.BulkData.Unlock();
	Constant->SRGB=false; Constant->NeverStream=true; Constant->UpdateResource();
	View.GroundMat->SetTextureParameterValue(TEXT("Noise"), Constant.Get());
	FTCFogOverlay Overlay; Overlay.Attach(Scratch.World);
	const TArray<FVector> P = { FVector(0,0,0), FVector(1600,0,0), FVector(0,1600,0), FVector(1600,1600,0) };
	Overlay.SetSection(0, P, {0,2,3,0,3,1}, {FVector::UpVector,FVector::UpVector,FVector::UpVector,FVector::UpVector}); Overlay.Flush(State);
	if (!TestNotNull(TEXT("fog pixel carrier"), Overlay.MeshComponent()) || !TestTrue(TEXT("fog pixel material loads"), Overlay.Ready()))
	{ Overlay.Detach(); View.Detach(); return false; }
	// Terrain's own overlay remains active too. Hide it by capturing only the
	// opaque control, actual weather mirrors and the explicitly held fog mesh.
	UTextureRenderTarget2D* Target = NewObject<UTextureRenderTarget2D>(Scratch.World->GetWorldSettings());
	Target->RenderTargetFormat=RTF_RGBA16f; Target->InitAutoFormat(64,64); Target->UpdateResourceImmediate(true);
	USceneCaptureComponent2D* Capture = NewObject<USceneCaptureComponent2D>(Scratch.World->GetWorldSettings());
	Capture->TextureTarget=Target; Capture->CaptureSource=SCS_SceneColorHDRNoAlpha;
	Capture->ProjectionType=ECameraProjectionMode::Orthographic; Capture->OrthoWidth=1600;
	Capture->bCaptureEveryFrame=false; Capture->bCaptureOnMovement=false;
	Capture->ShowFlags.SetEyeAdaptation(false); Capture->ShowFlags.SetAntiAliasing(false);
	Capture->PrimitiveRenderMode=ESceneCapturePrimitiveRenderMode::PRM_UseShowOnlyList;
	for (UProceduralMeshComponent* C : Ground) Capture->ShowOnlyComponent(C);
	for (const auto& Pair : View.Overlays) if (UProceduralMeshComponent* C = Pair.Value.Get()) Capture->ShowOnlyComponent(C);
	Capture->ShowOnlyComponent(Overlay.MeshComponent());
	Capture->SetWorldLocationAndRotation(FVector(800,800,2000), FRotator(-90,0,0));
	Capture->RegisterComponentWithWorld(Scratch.World);
#if WITH_EDITOR
	// A regenerated material can have no complete real-RHI shader map and no
	// queued job yet. Request this carrier before waiting, rather than reading
	// the fallback's invisible pixels as a weather composition result.
	const FMaterialResource* Resource = View.GroundMat->GetMaterial()->GetMaterialResource(GMaxRHIShaderPlatform);
	if (!Resource || !Resource->IsGameThreadShaderMapComplete()) View.GroundMat->GetMaterial()->ForceRecompileForRendering();
#endif
	FinishShaders();
#if WITH_EDITOR
	Resource = View.GroundMat->GetMaterial()->GetMaterialResource(GMaxRHIShaderPlatform);
	if (!TestTrue(TEXT("actual snow carrier shader is complete before readback"), Resource && Resource->IsGameThreadShaderMapComplete()))
	{ Capture->DestroyComponent(); Overlay.Detach(); View.Detach(); return false; }
#endif
	auto Pixel = [&]()
	{
		for (int I=0; I<3; I++) { Scratch.World->SendAllEndOfFrameUpdates(); Capture->CaptureScene(); FlushRenderingCommands(); }
		TArray<FLinearColor> Pixels;
		if (!Target->GameThread_GetRenderTargetResource()->ReadLinearColorPixels(Pixels) || Pixels.Num()!=4096)
		{ AddError(TEXT("actual weather readback failed")); return FLinearColor::Black; }
		return Pixels[32*64+32];
	};
	Overlay.MeshComponent()->SetVisibility(false);
	View.GroundMat->SetScalarParameterValue(TEXT("CoverOn"), 0.f); const FLinearColor Opaque=Pixel();
	View.GroundMat->SetScalarParameterValue(TEXT("CoverOn"), 1.f); const FLinearColor Snow=Pixel();
	Overlay.MeshComponent()->SetVisibility(true); const FLinearColor Fogged=Pixel();
	const float Alpha=.82f*(.72f+.28f*(128.f/255));
	const FLinearColor ExpectedSnow=FMath::Lerp(Opaque,TCLook::HexLinear(0xeef1f4),Alpha);
	const double FogAlpha=TCFogFrontierCore::Mist(120.0/255,8.125,8.125,Shared.Fog.PresentationTime());
	AddInfo(FString::Printf(TEXT("World27 actual HDR opaque %.6f, snow %.6f, fogged %.6f, source alpha %.9f, fog %.9f"),Opaque.R,Snow.R,Fogged.R,Alpha,FogAlpha));
	TestTrue(TEXT("opaque control is drawn"), Opaque.R>.15f && Opaque.R<.21f);
	TestNearlyEqual(TEXT("original snow alpha before fog"),Snow.R,ExpectedSnow.R,.015f);
	TestNearlyEqual(TEXT("fog shades the actual snow once after cover"),Fogged.R,(float)TCFogFrontierCore::Composite(Snow.R,FogAlpha),.015f);
	Capture->DestroyComponent(); Overlay.Detach(); View.Detach();
	return true;
}
#endif
