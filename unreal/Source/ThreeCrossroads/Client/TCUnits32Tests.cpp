// Authored recipient fixtures. These are consumer regressions, not live combat evidence.
#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCEffects.h"
#include "Client/TCGraphics.h"
#include "Client/TCUnitMotion.h"
#include "Client/TCUnitTable.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Dom/JsonObject.h"
#include "Engine/Engine.h"
#include "Engine/StaticMesh.h"
#include "Engine/World.h"
#include "Materials/MaterialInterface.h"
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCUnits32SupportFall,
 "ThreeCrossroads.Units32.SupportShootdownContinuity",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCUnits32SupportFall::RunTest(const FString&)
{
 FTCUnitTable Table; FString Error;
 if (!TestTrue(TEXT("unit definitions load"), Table.Load(Error))) return false;
 FTCModelLibrary Models;
 if (!TestTrue(TEXT("original model library loads"), Models.Load(Error))) return false;
 const auto* Entry = Models.Resolve({TEXT("attacker"), 1, FString()});
 const auto* Spec = FTCUnitMotion::AircraftSpec(TEXT("attacker"), 1);
 if (!TestNotNull(TEXT("original attacker"), Entry) || !TestNotNull(TEXT("original shadow spec"), Spec)) return false;
 UMaterialInterface* Shadow = Cast<UMaterialInterface>(Spec->Shadow.TryLoad());
 if (!TestNotNull(TEXT("original aircraft shadow material"), Shadow)) return false;

 for (ETCGraphicsLevel Tier : {ETCGraphicsLevel::High, ETCGraphicsLevel::Low})
 {
  const FTCGraphicsLevelScope Quality(Tier);
  UWorld* World = UWorld::CreateWorld(EWorldType::Game, false);
  if (!TestNotNull(TEXT("transient world"), World)) return false;
  GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World);
  World->InitializeActorsForPlay(FURL());
  FTCMatchState State;
  auto* Effects = World->SpawnActor<ATCEffects>();
  if (!TestNotNull(TEXT("effects actor"), Effects))
  { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); return false; }
  Effects->Bind(&State, &Table, nullptr, nullptr, nullptr);
  auto Feed = [&](const TCHAR* Line)
  {
   TSharedPtr<FJsonObject> Raw;
   if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Line), Raw)) return false;
   if (Raw->GetStringField(TEXT("t")) == TEXT("start"))
   {
    if (!State.ApplyStart(*Raw)) return false;
    Effects->OnStart(State);
   }
   else
   {
    State.ApplySnapshot(*Raw); // The return value reports drift, not successful decoding.
    Effects->OnSnapshot(State, *Raw);
   }
   return true;
  };
  // TimeOnly advances the world clock without automatically ticking actors. Use the production actor update.
  auto Step = [&]() { World->Tick(LEVELTICK_TimeOnly, .05f); Effects->Tick(.05f); };
  auto Submitted = [&]()
  {
   int32 Bodies = 0, Shadows = 0;
   TArray<UInstancedStaticMeshComponent*> Components; Effects->GetComponents(Components);
   for (auto* C : Components)
   {
    if (C->GetMaterial(0) == Shadow) Shadows += C->GetInstanceCount();
    if (C->GetStaticMesh() == Entry->Parts[0].Mesh.TryLoad()) Bodies += C->GetInstanceCount();
    if (Effects->Fx.AircraftNow().Num() == 1 && C->GetInstanceCount() == 1)
    {
     const FVector P = Effects->Fx.AircraftNow()[0].P;
     FTransform Transform; C->GetInstanceTransform(0, Transform, true);
     if (C->GetStaticMesh() == Entry->Parts[0].Mesh.TryLoad())
      TestTrue(TEXT("submitted original body follows this aircraft position"), Transform.GetLocation().Equals(P * 100., .01));
     if (C->GetMaterial(0) == Shadow)
      TestTrue(TEXT("submitted shadow follows this aircraft on ground"), Transform.GetLocation().Equals(FVector(P.X * 100., P.Y * 100., 30.), .01));
    }
   }
   return FIntPoint(Bodies, Shadows);
  };
  bool Ready = TestTrue(TEXT("delivered start"), Feed(TEXT(R"({"t":"start","matchId":32,"you":0,"names":["Me","Foe"],"teams":[0,1],"factions":[0,1],"map":{"name":"air32","w":8,"h":8,"rows":["........","........","........","........","........","........","........","........"],"points":[]}})")));
  Ready &= TestTrue(TEXT("delivered support pass"), Feed(TEXT(R"({"t":"s","tick":1,"units":[],"shots":[{"k":"strafe","x":20,"z":20,"dir":0,"fo":1}]})")));
  Step();
  Ready &= TestEqual(TEXT("one support aircraft before impact"), Effects->Fx.AircraftNow().Num(), 1);
  if (Ready)
  {
   const int32 Serial = Effects->Fx.AircraftNow()[0].Serial;
   TestTrue(TEXT("original body and its shadow submitted before impact"), Submitted() == FIntPoint(1, 1));
   TestTrue(TEXT("delivered shoot-down"), Feed(TEXT(R"({"t":"s","tick":2,"units":[],"shots":[{"k":"shotdown","kind":"strafe","x":20,"z":20}]})")));
   bool Falling = false, Crashed = false;
   double LastHeight = 0;
   // Hit is 1.3 to 1.45 seconds; fall ends on terrain contact or after three seconds.
   // Check every update, including the exact handoff update, rather than only the final count.
   for (int32 Frame = 0; Frame < 100; ++Frame)
   {
    Step();
    const auto& Aircraft = Effects->Fx.AircraftNow();
    const bool Impact = Effects->Fx.DecalsOn(0) > 0;
    if (Impact)
    {
     Crashed = true;
     TestEqual(TEXT("crash removes aircraft draw"), Aircraft.Num(), 0);
     TestTrue(TEXT("crash removes original body and shadow instances"), Submitted() == FIntPoint(0, 0));
     break;
    }
    if (!TestEqual(FString::Printf(TEXT("continuous aircraft through hit, frame %d"), Frame), Aircraft.Num(), 1)) continue;
    TestTrue(TEXT("original body and shadow submitted on every airborne frame"), Submitted() == FIntPoint(1, 1));
    TestEqual(TEXT("handoff retains aircraft instance identity"), Aircraft[0].Serial, Serial);
    TestEqual(TEXT("fall retains recipient owner"), Aircraft[0].Owner, 1);
    TestEqual(TEXT("fall retains original support role"), Aircraft[0].Type, FString(TEXT("attacker")));
    if (!FMath::IsNearlyZero(Aircraft[0].Roll))
    {
     if (Falling) TestTrue(TEXT("fall continues downward"), Aircraft[0].P.Z < LastHeight);
     Falling = true;
    }
    LastHeight = Aircraft[0].P.Z;
   }
   TestTrue(TEXT("delivered hit advances into falling motion"), Falling);
   TestTrue(TEXT("fall reaches crash"), Crashed);
   // Crash fire emits for 24 seconds; its last smoke can live another 13 seconds.
   for (int32 Frame = 0; Frame < 800; ++Frame) Step();
   TestEqual(TEXT("aircraft cannot reappear after crash"), Effects->Fx.AircraftNow().Num(), 0);
   TestTrue(TEXT("body and shadow stay removed"), Submitted() == FIntPoint(0, 0));
   TestEqual(TEXT("associated transient crash and smoke effects expire"), Effects->Fx.Particles().Num(), 0);
   // An unharmed support pass already departs correctly. Retain this passing branch as a guard.
   TestTrue(TEXT("delivered unharmed pass"), Feed(TEXT(R"({"t":"s","tick":3,"units":[],"shots":[{"k":"strafe","x":20,"z":20,"dir":0,"fo":1}]})")));
   Step();
   TestEqual(TEXT("unharmed pass enters"), Effects->Fx.AircraftNow().Num(), 1);
   TestTrue(TEXT("unharmed original body and shadow enter"), Submitted() == FIntPoint(1, 1));
   for (int32 Frame = 0; Frame < 80; ++Frame) Step();
   TestEqual(TEXT("unharmed pass departs"), Effects->Fx.AircraftNow().Num(), 0);
   TestTrue(TEXT("unharmed pass removes original body and shadow"), Submitted() == FIntPoint(0, 0));
  }
  Effects->Bind(nullptr, nullptr, nullptr, nullptr, nullptr);
  GEngine->DestroyWorldContext(World); World->DestroyWorld(false);
 }
 return true;
}
#endif
