// Real renderer regressions. Integration runs these with a rendering RHI.
#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "Client/TCEffects.h"
#include "Net/TCMatchState.h"
#include "World/TCTerrain.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "Engine/StaticMesh.h"
#include "Misc/AutomationTest.h"
#include "Misc/ScopeExit.h"

namespace TCUnits31Tests
{
 constexpr auto Flags = EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter;
 struct FWorld
 {
  UWorld* W = UWorld::CreateWorld(EWorldType::Game, false);
  FWorld() { if (W) GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(W); }
  ~FWorld() { if (W) { GEngine->DestroyWorldContext(W); W->DestroyWorld(false); } }
 };
 FTCUnit Unit(int32 Id, const FString& Type, int32 Owner, const FTCUnitTable& Table)
 {
  FTCUnit U; U.Id=Id; U.Type=Type; U.Owner=Owner; U.X=20; U.Z=30; U.Built=1;
  if (const auto* D=Table.Find(Type)) U.Hp=D->MaxHp();
  return U;
 }
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCUnits31Catalog, "ThreeCrossroads.Units31.All47ModelsFourFactions", TCUnits31Tests::Flags)
bool FTCUnits31Catalog::RunTest(const FString&)
{
 using namespace TCUnits31Tests;
 FWorld World; if (!TestNotNull(TEXT("world"),World.W)) return false;
 FTCUnitTable Table; FString Error;
 if (!TestTrue(TEXT("unit definitions"),Table.Load(Error))) return false;
 TestEqual(TEXT("full unit catalog"),Table.Num(),47);
 auto* V=World.W->SpawnActor<ATCUnitRenderer>();
 V->SetEnabledBatches((1u<<ATCUnitRenderer::BatchCount)-1);
 V->SetViewPoint(FVector(2000,1000,2500));
 for (int32 F=0; F<4; F++) for (const FString& Type : Table.Types())
 {
  V->Reset(); FTCMatchState S; S.bHasMatch=true; S.Factions={F};
  S.Units.Add(1,Unit(1,Type,0,Table)); V->Draw(S,Table,nullptr,{},0);
  ETCModelMatch Match=ETCModelMatch::None;
  const auto* Entry=V->Models().Resolve({Type,F,FString()},&Match);
  const FString Label=FString::Printf(TEXT("%s faction %d"),*Type,F);
  TestTrue(Label+TEXT(" resolves to original or source stand-in"),Entry && (Match==ETCModelMatch::Exact || Match==ETCModelMatch::Borrowed));
  TestEqual(Label+TEXT(" model units"),V->ModelStats().ModelUnits,1);
  TestEqual(Label+TEXT(" missing models"),V->ModelStats().MissingModel,0);
  TestEqual(Label+TEXT(" placeholders"),V->ModelStats().PlaceholderUnits,0);
  TestTrue(Label+TEXT(" submitted geometry"),V->ModelStats().PartInstances+V->ModelStats().CrowdMen+V->ModelStats().FarMen>0);
 }
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCUnits31Snapshots, "ThreeCrossroads.Units31.DeliveredCadenceWaterline", TCUnits31Tests::Flags)
bool FTCUnits31Snapshots::RunTest(const FString&)
{
 using namespace TCUnits31Tests;
 FWorld World; if (!TestNotNull(TEXT("world"),World.W)) return false;
 FTCUnitTable Table; FString Error;
 if (!TestTrue(TEXT("unit definitions"),Table.Load(Error))) return false;
 auto* V=World.W->SpawnActor<ATCUnitRenderer>();
 FTCMatchState S; S.bHasMatch=true; S.Factions={0}; S.Snapshots=1;
 S.Units.Add(1,Unit(1,TEXT("tank"),0,Table));
 V->ApplySnapshot(S,Table,1); V->Draw(S,Table,nullptr,{},0);
 S.Snapshots++; S.Units[1].X=24; V->ApplySnapshot(S,Table,1.2);
 // Repeated receipt consumption must not distort the delivered gap.
 V->ApplySnapshot(S,Table,1.21); V->Draw(S,Table,nullptr,{},.05f);
 FVector2D At; TestTrue(TEXT("tank drawn"),V->DrawnAt(1,At));
 TestTrue(TEXT("120 ms smoothed cadence reaches source x"),FMath::IsNearlyEqual(At.X,21.363037479198225,1e-5));
 V->Reset(); S.Snapshots++; S.Units[1].X=20;
 V->ApplySnapshot(S,Table,10); V->Draw(S,Table,nullptr,{},0);
 S.Snapshots++; S.Units[1].X=24; V->ApplySnapshot(S,Table,10.1); V->Draw(S,Table,nullptr,{},.05f);
 V->DrawnAt(1,At);
 TestTrue(TEXT("new match resets cadence to 100 ms"),FMath::IsNearlyEqual(At.X,21.573877361149466,1e-5));
 // Real relief: a river's water surface sits above its bed.
 S.Map.W=S.Map.H=12; S.Map.Chars.Init('W',144); S.Map.Levels.Init(0,144); S.Map.Known.Init(true,144);
 S.FogVisible.Init(true,144); S.FogExplored.Init(true,144);
 auto* Terrain=World.W->SpawnActor<ATCTerrain>(); Terrain->ResetFrom(S); Terrain->Flush(S);
 V->Reset(); S.Units.Reset(); auto Boat=Unit(2,TEXT("gunboat"),0,Table); Boat.X=Boat.Z=12; S.Units.Add(2,Boat);
 V->Draw(S,Table,Terrain,{},0);
 const double Water=Terrain->WaterAt(12,12), Bed=Terrain->HeightAt(12,12);
 TestTrue(TEXT("fixture has water above the bed"),FMath::IsFinite(Water) && Water>Bed);
 const auto* M=V->MotionOf(2); const auto* E=V->Models().Resolve({TEXT("gunboat"),0,FString()});
 if (!TestNotNull(TEXT("boat motion"),M) || !TestNotNull(TEXT("boat model"),E)) return false;
 const int32 Hull=E->FindPart(TEXT("hull")); if (!TestTrue(TEXT("hull exists"),Hull!=INDEX_NONE)) return false;
 const FTransform Expected=FTCUnitMotion::PartTransform(*E,Hull,FTransform(FVector(1200,1200,Water*100)),{},*M);
 bool Found=false; TArray<UInstancedStaticMeshComponent*> Cs; V->GetComponents(Cs);
 for (auto* C:Cs) if (C->GetStaticMesh()==E->Parts[Hull].Mesh.TryLoad()) for (int32 I=0;I<C->GetInstanceCount();I++)
 { FTransform T; C->GetInstanceTransform(I,T,true); Found|=T.Equals(Expected,.02); }
 TestTrue(TEXT("actual submitted hull rides water, not riverbed"),Found);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCUnits31Aircraft, "ThreeCrossroads.Units31.AircraftScaleAndLifecycle", TCUnits31Tests::Flags)
bool FTCUnits31Aircraft::RunTest(const FString&)
{
 using namespace TCUnits31Tests;
 FWorld World; if (!TestNotNull(TEXT("world"),World.W)) return false;
 FTCUnitTable Table; FString Error;
 if (!TestTrue(TEXT("unit definitions"),Table.Load(Error))) return false;
 auto* V=World.W->SpawnActor<ATCUnitRenderer>();
 for (int32 F=0;F<4;F++) for (const FString Type : {TEXT("fighter"),TEXT("attacker"),TEXT("bomber")})
 {
  V->Reset(); FTCMatchState S; S.bHasMatch=true; S.Factions={F};
  S.Units.Add(1,Unit(1,Type,0,Table));
  for (int32 UnitFlags : {TCClientFlags::PlaneAtBase,0,0,0,TCClientFlags::PlaneAtBase})
  {
   S.Units[1].Flags=UnitFlags; S.Units[1].Rot+=.3f; V->Draw(S,Table,nullptr,{},.05f);
   const bool Visible=UnitFlags==0;
   TestEqual(TEXT("park, launch, orbit, return, rearm body"),V->ModelStats().ModelUnits,Visible?1:0);
   TestEqual(TEXT("park, launch, orbit, return, rearm shadow"),V->ModelStats().AircraftShadows,Visible?1:0);
   if (!Visible) continue;
   const auto* E=V->Models().Resolve({Type,F,FString()});
   const auto* M=V->MotionOf(1); if (!TestNotNull(TEXT("plane motion"),M) || !TestNotNull(TEXT("plane model"),E)) return false;
   TestTrue(TEXT("original propellers animate"),!M->Parts.IsEmpty());
   bool Found=false; TArray<UInstancedStaticMeshComponent*> Cs; V->GetComponents(Cs);
   for (auto* C:Cs) for (int32 I=0;I<C->GetInstanceCount();I++)
   {
    FTransform T; C->GetInstanceTransform(I,T,true);
    if (C->GetStaticMesh()==E->Parts[0].Mesh.TryLoad())
     Found|=T.GetScale3D().Equals(E->Parts[0].Rest.GetScale3D()*TCUnits31::AirframeScale,1e-4) && T.GetLocation().Z>1900;
   }
   TestTrue(TEXT("submitted original airframe reduces ground occlusion and stays at flight altitude"),Found);
  }
 }
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCUnits31Terrain, "ThreeCrossroads.Units31.TerrainContact", TCUnits31Tests::Flags)
bool FTCUnits31Terrain::RunTest(const FString&)
{
 TCUnits31::FGroundPose Pose;
 TestTrue(TEXT("delivered compound slope poses the visual hull"),Pose.Update(10,10,0,2,true,.05,[](double X,double Z){return .5*X+.25*Z;}));
 TestTrue(TEXT("source slope pitch"),FMath::IsNearlyEqual(Pose.Pitch,.4636476090008061,1e-12));
 TestTrue(TEXT("source compound roll"),FMath::IsNearlyEqual(Pose.Roll,-.21998797739545944,1e-12));
 Pose.Update(10,10,0,2,true,.05,[](double,double){return 0.;});
 TestTrue(TEXT("tracked settling uses source response"),FMath::IsNearlyEqual(Pose.Pitch,.25445520286681583,1e-12));
 TestFalse(TEXT("unknown sample suspends pose"),Pose.Update(10,10,0,2,true,.05,[](double,double){return NAN;}));
 TestTrue(TEXT("unknown terrain retains no sample debt"),Pose.Suspended);
 Pose.Update(10,10,0,2,true,.05,[](double,double){return 0.;});
 TestTrue(TEXT("known terrain resumes immediately without a stale slope"),Pose.Pitch==0 && Pose.Roll==0 && !Pose.Suspended);
 return true;
}
#endif
