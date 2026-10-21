// Aircraft consumer regressions. Live-match captures are a separate integration check.
#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCModelLibrary.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "Client/TCEffects.h"
#include "Client/TCGraphics.h"
#include "Client/TCAircraft34Capture.h"
#include "Client/TCCameraPawn.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Engine/Engine.h"
#include "Engine/StaticMesh.h"
#include "Engine/World.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/AutomationTest.h"

namespace TCAircraft34
{
 constexpr auto Flags = EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter;
 struct FWorld
 {
  UWorld* W = UWorld::CreateWorld(EWorldType::Game, false);
  FWorld() { if (W) GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(W); }
  ~FWorld() { if (W) { GEngine->DestroyWorldContext(W); W->DestroyWorld(false); } }
 };
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAircraft34Framing, "ThreeCrossroads.Aircraft34.CaptureFraming", TCAircraft34::Flags)
bool FTCAircraft34Framing::RunTest(const FString&)
{
 // The delivered recon's first three heights must leave its full envelope below
 // the scoreboard and above the lower HUD, with its ground anchor also visible.
 const FTCPinhole Camera(FVector::ZeroVector,-90.f,54.43099f,TCAircraft34::CaptureDistance(),1280.0/720.0);
 for (double Height : {37.77,35.27,31.45})
 for (double X : {-5.0,5.0}) for (double Y : {-5.0,5.0}) for (double Z : {-1.0,1.0})
 {
  FVector2D P;
  TestTrue(TEXT("aircraft envelope projects in front of camera"),Camera.Project(FVector(X,Y,Height+Z),1280,720,P));
  TestTrue(TEXT("aircraft envelope clears scoreboard and lower panels"),P.X>260 && P.X<1020 && P.Y>145 && P.Y<395);
 }
 FVector2D Ground;
 TestTrue(TEXT("ground shadow anchor remains visible"),Camera.Project(FVector::ZeroVector,1280,720,Ground));
 TestTrue(TEXT("ground anchor clears lower panels"),Ground.Y>145 && Ground.Y<395);
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAircraft34Skin, "ThreeCrossroads.Aircraft34.ReadableSkin", TCAircraft34::Flags)
bool FTCAircraft34Skin::RunTest(const FString&)
{
 FTCModelLibrary Models; FString Error;
 if (!TestTrue(TEXT("original catalog loads"), Models.Load(Error))) return false;
 for (auto Tier : {ETCGraphicsLevel::High, ETCGraphicsLevel::Low})
 {
  const FTCGraphicsLevelScope Quality(Tier);
  for (int32 Faction = 0; Faction < 4; ++Faction)
  for (const FString Type : {TEXT("fighter"), TEXT("attacker"), TEXT("bomber"), TEXT("transport")})
  {
   const auto* Entry = Models.Resolve({Type, Faction, FString()});
   if (!TestNotNull(TEXT("original aircraft"), Entry)) return false;
   int32 Skins = 0;
   for (const auto& Part : Entry->Parts)
   {
    auto* C = NewObject<UInstancedStaticMeshComponent>();
    if (!TestTrue(TEXT("original aircraft component"), FTCModelLibrary::SetupComponent(C, Part, false))) return false;
    TestEqual(TEXT("original mesh retained"), C->GetStaticMesh().Get(), Cast<UStaticMesh>(Part.Mesh.TryLoad()));
    for (int32 Slot = 0; Slot < Part.Materials.Num(); ++Slot)
    {
     const FString Path = Part.Materials[Slot].ToString();
     auto* M = C->GetMaterial(Slot);
     auto Scalar = [&](const TCHAR* Name) { float V=-1; TestTrue(TEXT("skin scalar exists"), M->GetScalarParameterValue(FMaterialParameterInfo(Name),V)); return V; };
     if (Path.Contains(TEXT("MI_TCModel_METAL_MAT")) || Path.Contains(TEXT("MI_TCModel_BODY_MAT")))
     {
      ++Skins;
      TestNotNull(TEXT("aircraft skin uses local material instance"), Cast<UMaterialInstanceDynamic>(M));
      // ModelTextures must be off or the surface table overrides BaseMetalness.
      TestEqual(TEXT("paint uses original vertex colors without surface-table metal"), Scalar(TEXT("ModelTextures")), 0.f);
      TestEqual(TEXT("skin receives diffuse light without environment reflections"), Scalar(TEXT("BaseMetalness")), 0.f);
      TestEqual(TEXT("matte skin roughness"), Scalar(TEXT("BaseRoughness")), .78f);
      FLinearColor Rim; TestTrue(TEXT("rim parameter exists"), M->GetVectorParameterValue(FMaterialParameterInfo(TEXT("RimColor")), Rim));
      TestTrue(TEXT("modest rim fill"), Rim.Equals(FLinearColor(.12f,.14f,.16f), .0001f));
      TestEqual(TEXT("skin remains lit"), Scalar(TEXT("Unlit")), 0.f);
     }
     else TestEqual(TEXT("propeller discs and blades retain original materials"), M, Cast<UMaterialInterface>(Part.Materials[Slot].TryLoad()));
    }
   }
   TestTrue(TEXT("aircraft has a treated body"), Skins > 0);
  }
 }
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAircraft34Flak, "ThreeCrossroads.Aircraft34.FlakTiming", TCAircraft34::Flags)
bool FTCAircraft34Flak::RunTest(const FString&)
{
 for (auto Tier : {ETCGraphicsLevel::High, ETCGraphicsLevel::Low})
 for (bool HasPass : {false, true})
 {
  const FTCGraphicsLevelScope Quality(Tier);
  FTCCombatEffects Fx; FTCMatchState S; FTCFxContext C;
  C.HeightAt = [](float, float) { return 4.f; };
  S.bHasMatch = true; S.You = 0; S.Tick = 1; Fx.Start(S);
  FTCShot Flak; Flak.K=TEXT("flak"); Flak.X=Flak.Z=20; Flak.bHasPos=true;
  if (HasPass)
  {
   FTCShot Pass=Flak; Pass.K=TEXT("recon"); S.Shots={Pass};
   Fx.Snapshot(S,nullptr,C,0);
  }
  S.Tick++; S.Shots={Flak}; Fx.Snapshot(S,nullptr,C,0);
  auto Puffs = [&]() { int32 N=0; for (const auto& P:Fx.Particles()) N+=P.Preset==ETCFxPreset::Flak; return N; };
  TestEqual(TEXT("flak starts with no premature puffs"),Puffs(),0);
  // The four support bursts are scheduled at .7 + i*.14 + random*.06.
  // Fallback bursts are at i*.12. At 1.2 seconds every burst has fired and none expired.
  double Clock=0;
  for (int32 I=0; I<24; ++I) { Clock+=.05; Fx.Frame(C,Clock,.05f); if (HasPass && I<12) TestEqual(TEXT("support flak waits until .7 seconds"),Puffs(),0); }
  TestEqual(TEXT("four complete flak bursts at the original tier budget"),Puffs(),Tier==ETCGraphicsLevel::Low?8:16);
  for (const auto& P:Fx.Particles()) if (P.Preset==ETCFxPreset::Flak)
   TestTrue(TEXT("flak puffs are airborne above terrain"),P.P.Z>10);
  if (HasPass) TestEqual(TEXT("flak alone does not shoot down the pass"),Fx.AircraftNow().Num(),1);
 }
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAircraft34Airfield, "ThreeCrossroads.Aircraft34.ClassicAirfield", TCAircraft34::Flags)
bool FTCAircraft34Airfield::RunTest(const FString&)
{
 TCAircraft34::FWorld World;
 if (!TestNotNull(TEXT("world"),World.W)) return false;
 FTCUnitTable Table; FString Error;
 if (!TestTrue(TEXT("unit table loads"),Table.Load(Error))) return false;
 auto* Renderer=World.W->SpawnActor<ATCUnitRenderer>();
 if (!TestNotNull(TEXT("renderer"),Renderer)) return false;
 Renderer->SetEnabledBatches((1u<<ATCUnitRenderer::BatchCount)-1);
 for (auto Tier : {ETCGraphicsLevel::High, ETCGraphicsLevel::Low})
 for (int32 Faction=0; Faction<4; ++Faction)
 {
  const FTCGraphicsLevelScope Quality(Tier);
  Renderer->Reset(); FTCMatchState S; S.bHasMatch=true; S.Factions={Faction};
  FTCUnit U; U.Id=1; U.Owner=0; U.Type=TEXT("airfield"); U.X=U.Z=20; U.Built=1; U.Hp=Table.Find(U.Type)->MaxHp();
  S.Units.Add(U.Id,U); Renderer->Draw(S,Table,nullptr,{},.05f);
  const auto* E=Renderer->Models().Resolve({U.Type,Faction,FString()});
  if (!TestNotNull(TEXT("original Classic airfield model"),E)) return false;
  TestEqual(TEXT("airfield uses original model"),Renderer->ModelStats().ModelUnits,1);
  TestEqual(TEXT("airfield has no aircraft ground silhouette"),Renderer->ModelStats().AircraftShadows,0);
  bool Body=false; TArray<UInstancedStaticMeshComponent*> Components; Renderer->GetComponents(Components);
  for (auto* C:Components) if (C->GetStaticMesh()==E->Parts[0].Mesh.TryLoad() && C->GetInstanceCount()==1)
  {
   Body=true;
   TestEqual(TEXT("runway, hangar, tower and windsock retain original combined geometry/material"),C->GetMaterial(0),Cast<UMaterialInterface>(E->Parts[0].Materials[0].TryLoad()));
  }
  TestTrue(TEXT("airfield body submitted"),Body);
 }
 return true;
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAircraft34Down, "ThreeCrossroads.Aircraft34.CommandableCrashLifecycle", TCAircraft34::Flags)
bool FTCAircraft34Down::RunTest(const FString&)
{
 FTCUnitTable Table; FString Error;
 if (!TestTrue(TEXT("unit table"),Table.Load(Error))) return false;
 for (auto Tier : {ETCGraphicsLevel::High, ETCGraphicsLevel::Low})
 for (const FString Type : {TEXT("fighter"),TEXT("attacker"),TEXT("bomber")})
 {
  const FTCGraphicsLevelScope Quality(Tier);
  FTCCombatEffects Fx; FTCFxContext C; C.Table=&Table;
  C.HeightAt=[](float,float) { return 5.f; };
  FTCMatchState S; S.bHasMatch=true; S.You=0; S.Tick=1;
  FTCUnit U; U.Id=7; U.Owner=1; U.Type=Type; U.X=U.Z=20; U.Rot=.7; U.Hp=100;
  S.Units.Add(7,U); Fx.Start(S); Fx.Snapshot(S,nullptr,C,0);
  S.Tick++; S.Units.Reset();
  FTCShot Down; Down.K=TEXT("planedown"); Down.T=7; Down.bHasTarget=true; Down.X=Down.Z=20; Down.bHasPos=true;
  S.Shots={Down}; Fx.Snapshot(S,nullptr,C,0);
  double Clock=0, LastHeight=25; bool Crashed=false;
  for (int32 I=0; I<65; ++I)
  {
   Clock+=.05; Fx.Frame(C,Clock,.05f);
   if (Fx.DecalsOn(0)>0) { Crashed=true; TestEqual(TEXT("impact removes commandable falling copy"),Fx.AircraftNow().Num(),0); break; }
   if (!TestEqual(TEXT("continuous commandable fall before impact"),Fx.AircraftNow().Num(),1)) break;
   const auto& A=Fx.AircraftNow()[0];
   TestEqual(TEXT("fall retains last held role"),A.Type,Type);
   TestEqual(TEXT("fall retains last held owner"),A.Owner,1);
   TestTrue(TEXT("fall descends toward raised terrain"),A.P.Z<LastHeight); LastHeight=A.P.Z;
  }
  TestTrue(TEXT("all commandable roles crash within original three-second bound"),Crashed);
  const auto Sounds=Fx.TakeSounds();
  TestTrue(TEXT("crash plays original destruction sound"),Sounds.Contains(TEXT("vehicle_destroyed")));
  Fx.Reset(); S.Shots.Reset(); Fx.Start(S); Fx.Snapshot(S,nullptr,C,4); Fx.Frame(C,4.05,.05f);
  TestEqual(TEXT("reset cannot resurrect a falling aircraft"),Fx.AircraftNow().Num(),0);
 }
 return true;
}
#endif
