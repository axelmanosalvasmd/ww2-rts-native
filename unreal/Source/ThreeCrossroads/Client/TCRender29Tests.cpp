// Receipt and visibility regressions. Run ThreeCrossroads.Render29 on the integration lead's editor.
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/ScopeExit.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "Client/TCCoverPreview.h"
#include "Client/TCMarkers.h"
#include "Client/TCEffects.h"
#include "Client/TCSupportOrders.h"
#include "Net/TCMatchState.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender29CoverOrderTest, "ThreeCrossroads.Render29.CoverRecipientOrder",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRender29CoverOrderTest::RunTest(const FString&)
{
 FTCMatchState State; State.bHasMatch = true; State.You = 0; State.Teams = {0, 1};
 // More than the 48-hull cap, in an order that differs from ascending unit ids.
 for (int32 I = 0; I < 50; I++)
 {
  FTCUnit U; U.Id = 100 - I; U.Type = TEXT("tank"); U.Owner = 1; U.Hp = 100; U.X = 10; U.Z = 10;
  State.Units.Add(U.Id, U); State.UnitOrder.Add(U.Id);
 }
 FTCCoverKnown Known; FTCCoverSources Sources;
 FTCCoverPreview::Gather(State, {}, Sources, Known);
 if (!TestEqual(TEXT("only recipient-held rows are gathered"), Known.Units.Num(), 50)) return false;
 for (int32 I = 0; I < Known.Units.Num(); I++)
  TestEqual(TEXT("hull cap and equal-distance enemy ties retain recipient insertion order"), Known.Units[I].Id, State.UnitOrder[I]);
 State.Units.Remove(100); State.UnitOrder.Remove(100);
 FTCCoverPreview::Gather(State, {}, Sources, Known);
 TestEqual(TEXT("a fog-removed enemy cannot remain in cover scans"), Known.Units[0].Id, 99);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender29SnapshotTest, "ThreeCrossroads.Render29.SquadReceipts",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCRender29SnapshotTest::RunTest(const FString&)
{
 UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCRender29Squads"));
 if (!TestNotNull(TEXT("test world"), World)) return false;
 GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World);
 ON_SCOPE_EXIT { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); };
 ATCUnitRenderer* View = World->SpawnActor<ATCUnitRenderer>();
 FTCUnitTable Table; FString Error;
 if (!TestTrue(TEXT("original unit table"), Table.Load(Error))) return false;
 const FTCUnitDef* Def = Table.Find(TEXT("rifle"));
 if (!TestNotNull(TEXT("rifle definition"), Def) || !TestNotNull(TEXT("renderer"), View)) return false;
 FTCMatchState State; State.bHasMatch = true; State.Factions = {0}; State.Snapshots = 1;
 FTCUnit U; U.Id = 71; U.Type = TEXT("rifle"); U.Owner = 0; U.X = U.Z = 20; U.Hp = Def->MaxHp();
 State.Units.Add(U.Id, U); State.UnitOrder.Add(U.Id);
 View->SetViewPoint(FVector(2000, 2000, 3500));
 View->Draw(State, Table, nullptr, {}, 0.f, &State.UnitOrder);
 const UTCSoldierCrowd* Crowd = View->SoldierCrowd();
 if (!TestNotNull(TEXT("crowd"), Crowd)) return false;
 const int32 Initial = View->ModelStats().CrowdMen + View->ModelStats().FarMen;
 if (!TestTrue(TEXT("the original squad has standing figures"), Initial > 2)) return false;
 auto LoseOne = [&]
 {
  State.Units[71].Hp = FMath::FloorToInt(double(Initial - Crowd->NumCorpses() - 1) * Def->MaxHp() / Initial);
  State.Snapshots++; State.Tick += 2; View->ApplySnapshot(State, Table);
 };
 // These receipts deliberately have no Draw between them. The selected victim must fall only once.
 const FVector Victim = Crowd->Squads[71].Men[0].Man.GetLocation();
 FTCShot Shot; Shot.bHasTarget = true; Shot.T = 71; Shot.M = 0; State.Shots.Add(Shot);
 LoseOne(); if (!TestEqual(TEXT("first targeted casualty is consumed on receipt"), Crowd->NumCorpses(), 1)) return false;
 TestEqual(TEXT("the operative-hit figure falls instead of the last figure"), Crowd->Corpses[0].Slot, 0);
 TestTrue(TEXT("the operative-hit figure falls where it stood"), FMath::IsNearlyEqual(Crowd->Corpses[0].X, Victim.X / 100., 1e-7) && FMath::IsNearlyEqual(Crowd->Corpses[0].Z, Victim.Y / 100., 1e-7));
 State.Shots.Reset();
 State.Units[71].Flags |= TCFlags::Riding; State.Units[71].X += 10;
 View->Draw(State, Table, nullptr, {}, .1f, &State.UnitOrder);
 TestEqual(TEXT("a carried squad submits no near bodies"), View->ModelStats().CrowdMen, 0);
 TestEqual(TEXT("a carried squad submits no far bodies"), View->ModelStats().FarMen, 0);
 const FVector HiddenVictim = Crowd->Squads[71].Men[Initial - 2].Man.GetLocation();
 LoseOne(); if (!TestEqual(TEXT("a carried squad's casualty does not wait for visibility"), Crowd->NumCorpses(), 2)) return false;
 TestTrue(TEXT("hidden casualty follows the carried root"), FMath::IsNearlyEqual(Crowd->Corpses[1].X, HiddenVictim.X / 100., 1e-7) && FMath::IsNearlyEqual(Crowd->Corpses[1].Z, HiddenVictim.Y / 100., 1e-7));
 View->ApplySnapshot(State, Table);
 TestEqual(TEXT("same receipt is idempotent"), Crowd->NumCorpses(), 2);
 State.Units[71].Flags = 0; State.Snapshots++; State.Units[71].Hp = Def->MaxHp();
 View->ApplySnapshot(State, Table);
 TestEqual(TEXT("reinforcement does not replay casualties"), Crowd->NumCorpses(), 2);
 View->Draw(State, Table, nullptr, {}, .016f, &State.UnitOrder);
 State.Units.Reset(); State.UnitOrder.Reset(); State.Snapshots++;
 View->ApplySnapshot(State, Table);
 TestEqual(TEXT("fog removal produces no bodies"), Crowd->NumCorpses(), 2);
 FVector2D RemovedAt;
 TestFalse(TEXT("fog removal also forgets the drawn location"), View->DrawnAt(71, RemovedAt));
 View->Reset(); TestEqual(TEXT("new match clears corpses"), Crowd->NumCorpses(), 0);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender29CarrierAirTest, "ThreeCrossroads.Render29.CarrierAircraftLifecycle",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCRender29CarrierAirTest::RunTest(const FString&)
{
 UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCRender29Air"));
 if (!TestNotNull(TEXT("test world"), World)) return false;
 GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World);
 ON_SCOPE_EXIT { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); };
 ATCUnitRenderer* View = World->SpawnActor<ATCUnitRenderer>();
 FTCUnitTable Table; FString Error;
 if (!TestTrue(TEXT("original table"), Table.Load(Error)) || !TestNotNull(TEXT("renderer"), View)) return false;
 FTCMatchState S; S.bHasMatch = true; S.Factions = {0};
 FTCUnit U; U.Id = 1; U.Type = TEXT("tank"); U.Owner = 0; U.X = U.Z = 20; U.Hp = 100;
 U.Flags = TCCargo::Flag | (2 << TCCargo::Shift); S.Units.Add(1, U);
 View->Draw(S, Table, nullptr, {}, 0);
 if (!TestEqual(TEXT("two embarked squads draw six deck riders"), View->ModelStats().Riders, 6)) return false;
 if (!TestNotNull(TEXT("rider crowd"), View->SoldierCrowd())) return false;
 S.Units[1].Flags = 0; View->Draw(S, Table, nullptr, {}, .016f);
 TestEqual(TEXT("unload clears every rider"), View->ModelStats().Riders, 0);
 TestEqual(TEXT("unload makes no rider corpses"), View->SoldierCrowd()->NumCorpses(), 0);
 S.Units[1].Type = TEXT("halftrack"); S.Units[1].Flags = U.Flags;
 View->Draw(S, Table, nullptr, {}, .016f);
 TestEqual(TEXT("halftrack passengers stay inside"), View->ModelStats().Riders, 0);
 S.Units[1].Type = TEXT("fighter"); S.Units[1].Flags = 0;
 View->Draw(S, Table, nullptr, {}, .016f);
 TestEqual(TEXT("airborne aircraft draws one ground silhouette"), View->ModelStats().AircraftShadows, 1);
 S.Units[1].Flags = TCClientFlags::PlaneAtBase; S.Units[1].Rot = 1.f;
 View->Draw(S, Table, nullptr, {}, .016f);
 TestEqual(TEXT("parked aircraft draws no body"), View->ModelStats().ModelUnits, 0);
 TestEqual(TEXT("parked aircraft draws no shadow"), View->ModelStats().AircraftShadows, 0);
 TestTrue(TEXT("hidden aircraft heading still advances"), View->MotionOf(1) && View->MotionOf(1)->Heading > 0);
 S.Units.Reset(); View->Draw(S, Table, nullptr, {}, .016f);
 TestNull(TEXT("removed aircraft leaves no motion state"), View->MotionOf(1));
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender29DebrisTest, "ThreeCrossroads.Render29.DebrisReconnect",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRender29DebrisTest::RunTest(const FString&)
{
 FTCMatchState S; S.Tick = 200;
 FTCFalling F; F.Id = TEXT("wall1"); F.Kind = TEXT("section"); F.X = 10; F.Y = 5; F.Z = 20; S.Falling.Add(F);
 FTCShot Hit; Hit.K = TEXT("collapse"); Hit.Id = TEXT("impact1"); Hit.At = 10; S.Shots.Add(Hit);
 FTCDebrisView View; int32 Contacts = 0, Removed = 0; FTCDebrisRow Pose;
 View.On.Contact = [&](const FTCShot&, const FTCShotExtra&) { Contacts++; };
 View.On.Pose = [&](const FString&, const FTCDebrisRow& R) { Pose = R; };
 View.On.Remove = [&](const FString&) { Removed++; };
 View.Snapshot(S, {}, {}, 0); TestEqual(TEXT("fresh contact once"), Contacts, 1);
 S.Tick = 202; S.Falling[0].X = 14; S.Falling[0].Y = 3; S.Falling[0].Tilt = .6;
 View.Snapshot(S, {}, {}, .1); View.Frame(.15);
 TestTrue(TEXT("halfway delivered pose"), FMath::IsNearlyEqual(Pose.X, 12.) && FMath::IsNearlyEqual(Pose.Y, 4.) && FMath::IsNearlyEqual(Pose.Tilt, .3));
 TestEqual(TEXT("duplicate event does not replay"), Contacts, 1);
 View.Reset(); S.Tick = 220; View.Snapshot(S, {}, {}, 1);
 TestEqual(TEXT("reconnect restores the delivered section directly"), Pose.X, 14.);
 TestEqual(TEXT("historical collapse does not replay after reconnect"), Contacts, 1);
 S.Falling.Reset(); S.Tick = 222; View.Snapshot(S, {}, {}, 1.1);
 TestEqual(TEXT("expired section is removed"), View.Count(), 0);
 TestEqual(TEXT("old section removed on reset and final expiry"), Removed, 2);
 return true;
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender29AirFallTest, "ThreeCrossroads.Render29.AircraftFallRule",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRender29AirFallTest::RunTest(const FString&)
{
 TCRender29Aircraft::FFall F; F.X = 10; F.Y = 20; F.Z = 20; F.Heading = .4; F.Speed = 15; F.Roll = .2; F.Pitch = -.1;
 TCRender29Aircraft::Advance(F, .1);
 // aircraft.js fall, fixed rollRate 2 and yaw 1.3, one 0.1 second frame.
 TestTrue(TEXT("source gravity"), FMath::IsNearlyEqual(F.Vy, -1.4, 1e-12));
 TestTrue(TEXT("source height"), FMath::IsNearlyEqual(F.Y, 19.86, 1e-12));
 TestTrue(TEXT("source turning heading"), FMath::IsNearlyEqual(F.Heading, .53, 1e-12));
 TestTrue(TEXT("source roll acceleration"), FMath::IsNearlyEqual(F.Roll, .4, 1e-12) && FMath::IsNearlyEqual(F.RollRate, 2.5, 1e-12));
 TestTrue(TEXT("aircraft drag uses 0.45"), FMath::IsNearlyEqual(F.Speed, 15 * FMath::Exp(-.045), 1e-12));
 F.Y = 100; F.Age = 3;
 TestFalse(TEXT("timeout keeps the original strict boundary"), TCRender29Aircraft::Landed(F, 0));
 F.Age += .001;
 TestTrue(TEXT("fall ends after three seconds"), TCRender29Aircraft::Landed(F, 0));
 F.Age = 0; F.Y = .6;
 TestTrue(TEXT("ground contact includes 0.6 metres"), TCRender29Aircraft::Landed(F, 0));
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender29EffectsAircraftTest, "ThreeCrossroads.Render29.EffectAircraftModels",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCRender29EffectsAircraftTest::RunTest(const FString&)
{
 UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCRender29EffectAir"));
 if (!TestNotNull(TEXT("effect aircraft test world"), World)) return false;
 GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World);
 ON_SCOPE_EXIT { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); };
 ATCEffects* View = World->SpawnActor<ATCEffects>();
 FTCUnitTable Table; FString Error;
 if (!TestTrue(TEXT("original unit table"), Table.Load(Error)) || !TestNotNull(TEXT("effects actor"), View)) return false;
 FTCMatchState S; S.bHasMatch = true; S.You = 0; S.Factions = {0, 1, 2};
 View->Bind(&S, &Table, nullptr, nullptr, nullptr);
 FTCFxContext Context; Context.Table = &Table;
 FTCShot Recon; Recon.K = TEXT("recon"); Recon.bHasPos = true; Recon.X = Recon.Z = 20; Recon.bHasShooterOwner = true; Recon.Fo = 1;
 S.Shots.Add(Recon); View->Fx.Snapshot(S, nullptr, Context, 0); View->Fx.Frame(Context, .1, .1f); View->Present(.1);
 if (!TestEqual(TEXT("one support flight"), View->Fx.AircraftNow().Num(), 1)) return false;
 TestEqual(TEXT("recon reuses original fighter"), View->Fx.AircraftNow()[0].Type, FString(TEXT("fighter")));
 TestTrue(TEXT("original parts and soft silhouette are actually submitted"), View->AircraftInstancesDrawn() > 1);
 View->Reset(); S.Shots.Reset();
 FTCUnit U; U.Id = 9; U.Type = TEXT("bomber"); U.Owner = 2; U.Hp = 100; S.Units.Add(9, U);
 View->Fx.Start(S); S.Units.Reset();
 FTCShot Down; Down.K = TEXT("planedown"); Down.bHasPos = true; Down.X = Down.Z = 20; Down.bHasTarget = true; Down.T = 9;
 S.Shots.Add(Down); View->Fx.Snapshot(S, nullptr, Context, 0); View->Fx.Frame(Context, .1, .1f); View->Present(.1);
 if (!TestEqual(TEXT("one shot-down aircraft"), View->Fx.AircraftNow().Num(), 1)) return false;
 const auto& Plane = View->Fx.AircraftNow()[0];
 TestEqual(TEXT("a just-removed bomber keeps its recipient-held model"), Plane.Type, FString(TEXT("bomber")));
 TestEqual(TEXT("a just-removed bomber keeps its owner"), Plane.Owner, 2);
 TestTrue(TEXT("falling original model and silhouette are submitted"), View->AircraftInstancesDrawn() > 1);
 TArray<UInstancedStaticMeshComponent*> Components; View->GetComponents(Components);
 for (const auto* C : Components) if (C->GetInstanceCount())
  TestFalse(TEXT("effect aircraft do not cast a duplicate hull shadow"), C->CastShadow);
 View->OnStart(S);
 TestEqual(TEXT("new match has no old aircraft draw records"), View->Fx.AircraftNow().Num(), 0);
 for (const auto* C : Components) TestEqual(TEXT("new match removes every aircraft part and silhouette"), C->GetInstanceCount(), 0);
 View->Reset();
 return true;
}
#endif
