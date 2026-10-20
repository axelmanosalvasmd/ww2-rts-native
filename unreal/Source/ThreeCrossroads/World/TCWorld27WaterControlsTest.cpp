// One-factor presentation probes. They keep water geometry, mask and level alive
// while isolating visibility, foam and water-only shadow reception for GPU lanes.
#if WITH_DEV_AUTOMATION_TESTS
#include "World/TCWater.h"
#include "Client/TCLighting.h"
#include "Client/TCGraphics.h"
#include "Net/TCMatchState.h"
#include "Components/DirectionalLightComponent.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "HAL/IConsoleManager.h"
#include "Materials/Material.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/AutomationTest.h"
#include "ProceduralMeshComponent.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld27WaterControlsTest, "ThreeCrossroads.World.WaterPresentationControls", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld27WaterControlsTest::RunTest(const FString& Parameters)
{
	struct FSetting
	{
		IConsoleVariable* Var;
		int32 Previous;
		explicit FSetting(const TCHAR* Name) : Var(IConsoleManager::Get().FindConsoleVariable(Name)), Previous(Var ? Var->GetInt() : 0) {}
		~FSetting() { if (Var) Var->Set(Previous, ECVF_SetByCode); }
		void Set(int32 Value) { if (Var) Var->Set(Value, ECVF_SetByCode); }
	};
	FSetting Visible(TEXT("tc.World27.WaterVisible")), Foam(TEXT("tc.World27.WaterFoam")), Shadow(TEXT("tc.World27.WaterShadow"));
	if (!TestNotNull(TEXT("water visibility control"), Visible.Var)
		|| !TestNotNull(TEXT("water foam control"), Foam.Var)
		|| !TestNotNull(TEXT("water-only shadow control"), Shadow.Var)) return false;
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("World27WaterControls"));
	if (!TestNotNull(TEXT("scratch world"), World)) return false;
	GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World);
	World->InitializeActorsForPlay(FURL());
	ATCLighting* Light = World->SpawnActor<ATCLighting>();
	if (!TestNotNull(TEXT("source sun control"), Light)) { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); return false; }
	Light->Sun->ShadowAmount = .63f;
	FTCMatchState State;
	State.bHasMatch = true; State.Map.W = State.Map.H = 8;
	State.Map.Chars.Init('W',64); State.Map.Levels.Init(0,64); State.Map.Known.Init(true,64);
	State.FogVisible.Init(true,64); State.FogExplored.Init(true,64);
	for (ETCGraphicsLevel Level : {ETCGraphicsLevel::High, ETCGraphicsLevel::Low})
	{
		FTCGraphicsLevelScope Graphics(Level);
		Visible.Set(1); Foam.Set(1); Shadow.Set(1);
		FTCWater Water; Water.Attach(World); Water.ResetFrom(State); Water.SetPresentationTime(37);
		Water.Flush(State,37,false);
		UProceduralMeshComponent* Mesh = Water.MeshComponent();
		UMaterialInstanceDynamic* Material = Water.MaterialInstance();
		if (!TestNotNull(TEXT("water carrier"),Mesh) || !TestNotNull(TEXT("water material"),Material)) { Water.Detach(); continue; }
		const int32 Builds = Water.GeometryBuilds(), Triangles = Water.TrianglesDrawn();
		const float Height = Water.WaterAt(7,7), Time = Water.TimeSent();
		TestTrue(TEXT("default presentation draws water"),Mesh->IsVisible());
		Visible.Set(0); Water.Flush(State,37,false);
		TestFalse(TEXT("off control removes the actual water carrier"),Mesh->IsVisible());
		TestTrue(TEXT("off control retains the water query"),Water.HasWater());
		TestEqual(TEXT("off control retains water height"),Water.WaterAt(7,7),Height);
		Visible.Set(1); Water.Flush(State,37,false);
		TestTrue(TEXT("water returns without a rebuild"),Mesh->IsVisible());
		float FoamDefault = -1;
		TestTrue(TEXT("base material graph consumes the foam parameter"),Material->GetMaterial()->GetScalarParameterValue(FMaterialParameterInfo(TEXT("FoamOn")),FoamDefault));
		TestEqual(TEXT("base material foam defaults to original behavior"),FoamDefault,1.f);
		float FoamOn = -1;
		TestTrue(TEXT("generated water shader exposes foam control"),Material->GetScalarParameterValue(FMaterialParameterInfo(TEXT("FoamOn")),FoamOn));
		TestEqual(TEXT("default foam matches the original shader"),FoamOn,1.f);
		Foam.Set(0); Water.Flush(State,37,false);
		Material->GetScalarParameterValue(FMaterialParameterInfo(TEXT("FoamOn")),FoamOn);
		TestEqual(TEXT("foam-only control reaches the actual MID"),FoamOn,0.f);
		FLinearColor Sun;
		Material->GetVectorParameterValue(FMaterialParameterInfo(TEXT("SunDirection")),Sun);
		TestNearlyEqual(TEXT("foam control retains water shadow reception"),Sun.A,.63f,1e-6f);
		Foam.Set(1); Shadow.Set(0); Water.Flush(State,37,false);
		Material->GetVectorParameterValue(FMaterialParameterInfo(TEXT("SunDirection")),Sun);
		TestEqual(TEXT("water-only shadow control leaves the sun alive"),Sun.A,0.f);
		TestEqual(TEXT("water shadow control does not change the scene sun"),Light->Sun->ShadowAmount,.63f);
		Shadow.Set(1); Water.Flush(State,37,false);
		Material->GetVectorParameterValue(FMaterialParameterInfo(TEXT("SunDirection")),Sun);
		TestNearlyEqual(TEXT("water shadow reception returns"),Sun.A,.63f,1e-6f);
		TestEqual(TEXT("one-factor probes do not rebuild water geometry"),Water.GeometryBuilds(),Builds);
		TestEqual(TEXT("one-factor probes retain triangles"),Water.TrianglesDrawn(),Triangles);
		TestEqual(TEXT("one-factor probes retain the held clock"),Water.TimeSent(),Time);
		Water.Detach();
	}
	GEngine->DestroyWorldContext(World); World->DestroyWorld(false);
	return true;
}
#endif
