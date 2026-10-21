// Authored recipient integration coverage. No commands, simulation, live match or rendered pixels are implied.
#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCUnits33Fixture.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitMotion.h"
#include "Client/TCUnitTable.h"
#include "Client/TCEffects.h"
#include "Client/TCGraphics.h"
#include "Net/TCMatchState.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Dom/JsonObject.h"
#include "Engine/Engine.h"
#include "Engine/StaticMesh.h"
#include "Engine/World.h"
#include "Materials/MaterialInterface.h"
#include "Misc/App.h"
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCUnits33Tests
{
 constexpr auto Flags = EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter;
 struct FRecipient
 {
  UWorld* World = nullptr;
  FTCMatchState State;
  FTCUnitTable Table;
  ATCUnitRenderer* Renderer = nullptr;
  ATCEffects* Effects = nullptr;
  FString Error;
  bool Init()
  {
   if (!FApp::CanEverRender() || !Table.Load(Error)) return false;
   World = UWorld::CreateWorld(EWorldType::Game, false);
   if (!World) return false;
   GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World);
   World->InitializeActorsForPlay(FURL());
   Renderer = World->SpawnActor<ATCUnitRenderer>();
   Effects = World->SpawnActor<ATCEffects>();
   if (!Renderer || !Effects) return false;
   Renderer->SetEnabledBatches((1u << ATCUnitRenderer::BatchCount) - 1);
   Effects->Bind(&State, &Table, nullptr, Renderer, nullptr);
   return Feed(TCUnits33Fixture::Start);
  }
  ~FRecipient()
  {
   if (Effects) Effects->Bind(nullptr, nullptr, nullptr, nullptr, nullptr);
   if (World) { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); }
  }
  bool Feed(const FString& Text)
  {
   TSharedPtr<FJsonObject> Raw;
   if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Raw)) return false;
   if (Raw->GetStringField(TEXT("t")) == TEXT("start"))
   {
    if (!State.ApplyStart(*Raw)) return false;
    Effects->OnStart(State);
   }
   else
   {
    if (State.ApplySnapshot(*Raw)) return false; // true is a held-set drift, not successful decoding
    Renderer->ApplySnapshot(State, Table, State.Tick * .1);
    Effects->OnSnapshot(State, *Raw);
   }
   return true;
  }
  void Step(float Dt = .05f)
  {
   World->Tick(LEVELTICK_TimeOnly, Dt);
   Renderer->Draw(State, Table, nullptr, {}, Dt);
   Effects->Tick(Dt);
  }
  int32 Particles(ETCFxPreset Preset) const
  {
   int32 Count = 0;
   for (const auto& P : Effects->Fx.Particles()) if (P.Preset == Preset) ++Count;
   return Count;
  }
 };
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCUnits33Lifecycle,
 "ThreeCrossroads.Units33.CommandableRecipientLifecycle", TCUnits33Tests::Flags)
bool FTCUnits33Lifecycle::RunTest(const FString&)
{
 using namespace TCUnits33Tests;
 for (ETCGraphicsLevel Tier : {ETCGraphicsLevel::High, ETCGraphicsLevel::Low})
 {
  const FTCGraphicsLevelScope Quality(Tier);
  FRecipient R;
  if (!TestTrue(TEXT("render-capable recipient world and original definitions"), R.Init())) return false;
  AddInfo(FString::Printf(TEXT("authored own US fighter, tier %s"), LexToString(FTCGraphics::Get().Level())));
  FQuat OutBody = FQuat::Identity, OutProp = FQuat::Identity;
  FVector2D LastPosition;
  for (const auto& Stage : TCUnits33Fixture::Stages)
  {
   if (!TestTrue(Stage.Name, R.Feed(Stage.Raw))) return false;
   R.Step();
   if (!TestEqual(TEXT("owner retains parked and airborne plane"), R.State.Units.Num(), 1) ||
       !TestEqual(TEXT("owner air row decoded"), R.State.Air.Num(), 1)) return false;
   TestEqual(TEXT("air row refers to delivered fighter"), R.State.Air[0].Id, 33);
   TestEqual(TEXT("delivered state retained without inferring a transition"), R.State.Air[0].State, Stage.State);
   if (Stage.State == 4) TestEqual(TEXT("delivered rearm timer"), R.State.Air[0].Timer, 30);
   if (Stage.State == 0) TestEqual(TEXT("ready fuel retained"), R.State.Air[0].Fuel, 50);
   TestEqual(TEXT("original body visibility"), R.Renderer->ModelStats().ModelUnits, Stage.Visible ? 1 : 0);
   TestEqual(TEXT("outline visibility"), R.Renderer->ModelStats().AircraftShadows, Stage.Visible ? 1 : 0);
   TestEqual(TEXT("no placeholder"), R.Renderer->ModelStats().PlaceholderUnits, 0);
   const auto* Entry = R.Renderer->Models().Resolve({TEXT("fighter"), 0, FString()});
   const auto* Spec = FTCUnitMotion::AircraftSpec(TEXT("fighter"), 0);
   if (!TestNotNull(TEXT("original US fighter"), Entry) || !TestNotNull(TEXT("original outline"), Spec)) return false;
   auto* Mesh = Cast<UStaticMesh>(Entry->Parts[0].Mesh.TryLoad());
   auto* Shadow = Cast<UMaterialInterface>(Spec->Shadow.TryLoad());
   if (!TestNotNull(TEXT("body asset"), Mesh) || !TestNotNull(TEXT("shadow asset"), Shadow)) return false;
   int32 Bodies = 0, Shadows = 0;
   TArray<UInstancedStaticMeshComponent*> Components;
   R.Renderer->GetComponents(Components);
   FVector2D At; R.Renderer->DrawnAt(33, At);
   const auto* Motion = R.Renderer->MotionOf(33);
   for (auto* C : Components)
   {
    if (C->GetStaticMesh() == Mesh) Bodies += C->GetInstanceCount();
    if (C->GetMaterial(0) == Shadow) Shadows += C->GetInstanceCount();
    if (!Stage.Visible) continue;
    for (int32 I = 0; I < C->GetInstanceCount(); ++I)
    {
     FTransform T; C->GetInstanceTransform(I, T, true);
     if (C->GetMaterial(0) == Shadow)
     {
      TestTrue(TEXT("actual outline stays under eased aircraft at ground+0.3m"), T.GetLocation().Equals(FVector(At.X * 100, At.Y * 100, 30), .02));
      if (Motion) TestTrue(TEXT("outline yaw follows eased flight heading"), T.GetRotation().Equals(FQuat(FVector::UpVector, Motion->Heading), 1e-4));
     }
     if (C->GetStaticMesh() == Mesh && Motion)
     {
      const FTransform Root(FQuat(FVector::UpVector, Motion->Heading), FVector(At.X * 100, At.Y * 100, 2000), FVector(TCUnits31::AirframeScale));
      const FTransform Expected = FTCUnitMotion::PartTransform(*Entry, 0, Root, {}, *Motion);
      TestTrue(TEXT("actual original body at 20m flight root with bank and authored part transform"), T.Equals(Expected, .02));
     }
    }
   }
   TestEqual(TEXT("actual body instance count clears at base/rearm and returns on launch"), Bodies, Stage.Visible ? 1 : 0);
   TestEqual(TEXT("actual outline instance count clears at base/rearm and returns on launch"), Shadows, Stage.Visible ? 1 : 0);
   if (!Stage.Visible) continue;
   if (!TestNotNull(TEXT("production flight motion"), Motion)) return false;
   TestTrue(TEXT("original propeller motion exists"), !Motion->Parts.IsEmpty());
   if (FString(Stage.Name) == TEXT("out"))
   {
    OutBody = Motion->Body.GetRotation();
    if (!Motion->Parts.IsEmpty()) OutProp = Motion->Parts.CreateConstIterator().Value().GetRotation();
   }
   if (Stage.State == 2)
   {
    TestFalse(TEXT("received orbit heading changes original body motion"), Motion->Body.GetRotation().Equals(OutBody, 1e-4));
    TestTrue(TEXT("orbit banks beyond the original small idle wobble"), Motion->Body.GetRotation().AngularDistance(FQuat::Identity) > .08);
    if (!Motion->Parts.IsEmpty()) TestFalse(TEXT("propeller continues spinning during orbit"), Motion->Parts.CreateConstIterator().Value().GetRotation().Equals(OutProp, 1e-4));
    TestTrue(TEXT("received orbit positions move submitted aircraft"), FVector2D::Distance(At, LastPosition) > .1);
   }
   LastPosition = At;
  }
 }
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCUnits33Damage,
 "ThreeCrossroads.Units33.CommandableRecipientDamageVisibility", TCUnits33Tests::Flags)
bool FTCUnits33Damage::RunTest(const FString&)
{
 using namespace TCUnits33Tests;
 for (ETCGraphicsLevel Tier : {ETCGraphicsLevel::High, ETCGraphicsLevel::Low})
 {
  const FTCGraphicsLevelScope Quality(Tier);
  FRecipient R;
  if (!TestTrue(TEXT("render-capable recipient world"), R.Init())) return false;
  AddInfo(FString::Printf(TEXT("authored damage thresholds, tier %s"), LexToString(FTCGraphics::Get().Level())));
  auto Damage = [&](int32 Hp, bool Hidden)
  {
   return R.Feed(FString::Printf(TEXT(R"({"t":"s","tick":%d,"units":[[33,"fighter",0,30,30,0,0,%d,0,0,0,0,%d,0,1]],"air":[[33,%d,48,60,%d]]})"), R.State.Tick + 1, Hp, Hidden ? 512 : 0, Hidden ? 4 : 2, Hidden ? 30 : 0));
  };
  // Use fresh recipient updates and the production actor's damage consumer, not PlaneDamage alone.
  TestTrue(TEXT("exact60percent delivered"), Damage(120, false)); R.Step();
  TestEqual(TEXT("60percent emits no damage smoke"), R.Effects->Fx.Particles().Num(), 0);
  TestTrue(TEXT("below60percent delivered"), Damage(119, false)); R.Step();
  TestEqual(TEXT("light smoke emitted by production actor"), R.Particles(ETCFxPreset::AirSmoke), 1);
  TestEqual(TEXT("light damage has no flame"), R.Particles(ETCFxPreset::Burn), 0);
  // Enough updates for either tier's smoke interval, while retaining the same original aircraft.
  TestTrue(TEXT("exact30percent delivered"), Damage(60, false));
  for (int32 I = 0; I < 6; ++I) R.Step();
  TestEqual(TEXT("30percent has no dark smoke"), R.Particles(ETCFxPreset::AirSmokeDark), 0);
  TestEqual(TEXT("30percent has no flame"), R.Particles(ETCFxPreset::Burn), 0);
  TestTrue(TEXT("below30percent delivered"), Damage(59, false));
  for (int32 I = 0; I < 6; ++I) R.Step();
  TestTrue(TEXT("heavy damage emits dark smoke"), R.Particles(ETCFxPreset::AirSmokeDark) > 0);
  TestTrue(TEXT("heavy damage emits fire"), R.Particles(ETCFxPreset::Burn) > 0);
  for (const auto& P : R.Effects->Fx.Particles())
  {
   if (P.Preset == ETCFxPreset::AirSmoke || P.Preset == ETCFxPreset::AirSmokeDark)
    TestTrue(TEXT("damage smoke originates near aircraft altitude"), P.P.Z > 19 && P.P.Z < 24);
  }
  TestTrue(TEXT("damaged rearming plane delivered"), Damage(59, true));
  // Already airborne smoke may linger, as in the original. Hidden planes must stop renewing it.
  for (int32 I = 0; I < 60; ++I) R.Step();
  TestEqual(TEXT("hidden damaged plane submits no body"), R.Renderer->ModelStats().ModelUnits, 0);
  TestEqual(TEXT("hidden damaged plane submits no outline"), R.Renderer->ModelStats().AircraftShadows, 0);
  TestEqual(TEXT("old smoke/fire drains without hidden renewal"), R.Effects->Fx.Particles().Num(), 0);
  TestTrue(TEXT("repaired relaunch delivered"), Damage(200, false)); R.Step();
  TestEqual(TEXT("relaunch restores body"), R.Renderer->ModelStats().ModelUnits, 1);
  TestEqual(TEXT("relaunch restores ground outline"), R.Renderer->ModelStats().AircraftShadows, 1);
  TestEqual(TEXT("repaired relaunch emits no stale damage"), R.Effects->Fx.Particles().Num(), 0);
 }
 return true;
}
#endif
