// Focused recipient-view regressions for the integration lead. Run ThreeCrossroads.Look30 with a rendering RHI.
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/ScopeExit.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "Engine/StaticMesh.h"
#include "Client/TCMarkers.h"
#include "Client/TCLook30Cargo.h"
#include "World/TCWorldObjects.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Client/TCCoverPreview.h"
#include "Client/TCOrders.h"
#include "Client/TCSupportOrders.h"
#include "Client/TCLogistics.h"
#include "Client/TCModelLibrary.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "Client/TCEffects.h"
#include "Net/TCMatchState.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCLook30Test
{
 TSharedPtr<FJsonObject> Json(const FString& Text)
 {
  TSharedPtr<FJsonObject> Out;
  FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Out);
  return Out;
 }
 constexpr auto Flags = EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30Markings, "ThreeCrossroads.Look30.UnitMarkings", TCLook30Test::Flags)
bool FTCLook30Markings::RunTest(const FString&)
{
 struct FCase { double Hp; int Supp; bool Selected, Hovered, Garrisoned, Bar; int Vet; };
 const FCase Cases[] = {{1,0,false,false,false,false,0}, {.5,0,false,false,false,true,1},
  {1,50,false,false,false,true,2}, {1,90,false,false,false,true,3}, {1,0,true,false,false,true,2},
  {1,0,false,true,false,true,1}, {1,0,false,false,true,true,3}, {1,0,false,false,false,false,3}};
 for (const auto& C : Cases)
 {
  TestEqual(TEXT("source health-bar predicate"), TCMarkersCore::HealthBarShown(C.Hp,C.Supp,C.Selected,C.Hovered,C.Garrisoned), C.Bar);
  TestEqual(TEXT("veterancy stays independent of the bar"), TCMarkersCore::StarsShown(C.Vet), C.Vet);
  TestEqual(TEXT("suppression ribbon length"), TCMarkersCore::SuppressionFill(C.Supp).W, 2.3*C.Supp/100);
  if (C.Supp) TestEqual(TEXT("pinned versus suppressed colour"),TCMarkersCore::SuppressionHex(C.Supp), C.Supp>=90?0xff3b2au:0xffd23au);
 }
 TestFalse(TEXT("source near-full threshold hides the bar"),TCMarkersCore::HealthBarShown(2.2995/2.3,0,false,false,false));
 TestTrue(TEXT("below source threshold shows the bar"),TCMarkersCore::HealthBarShown(2.2985/2.3,0,false,false,false));
 TestTrue(TEXT("two mixed units show ranges"), TCMarkersCore::RangeRingsShown(2,2,[](int,int){return false;}));
 TestFalse(TEXT("three mixed units hide ranges"), TCMarkersCore::RangeRingsShown(3,3,[](int,int){return false;}));
 TestTrue(TEXT("four identical units show ranges"), TCMarkersCore::RangeRingsShown(4,4,[](int,int){return true;}));
 TestFalse(TEXT("five identical units hide ranges"), TCMarkersCore::RangeRingsShown(5,5,[](int,int){return true;}));
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30Cover, "ThreeCrossroads.Look30.CoverClassificationFlash", TCLook30Test::Flags)
bool FTCLook30Cover::RunTest(const FString&)
{
 TArray<uint8> Chars, Damage, Seen; Chars.Init('.',81); Damage.Init(0,81); Seen.Init(0,81);
 FTCCoverKnown K; K.Grid={9,9,Chars,Damage}; K.Explored=Seen; K.bInfantrySelected=K.bGarrisonSelected=true;
 Chars[40]='T'; Chars[41]='B'; Chars[39]='#'; Seen[40]=Seen[41]=Seen[39]=1;
 TestEqual(TEXT("trench heavy"), FTCCoverPreview::CellCover(K.Grid,4,4),ETCCoverKind::Heavy);
 TestEqual(TEXT("house edge heavy for garrison infantry"),FTCCoverPreview::CellCover(K.Grid,5,4),ETCCoverKind::Heavy);
 TestEqual(TEXT("house excluded for non-garrison infantry"),FTCCoverPreview::CellCover(K.Grid,5,4,false),ETCCoverKind::None);
 TestEqual(TEXT("sandbags light"),FTCCoverPreview::CellCover(K.Grid,3,4),ETCCoverKind::Light);
 TestEqual(TEXT("bare ground open"),FTCCoverPreview::CellCover(K.Grid,4,3),ETCCoverKind::Open);
 // 2.9 m is the first solid sample. Stage 2 must remove its directional protection.
 TArray<uint8> Line={'.','.','#','.','.'}, Stages={0,0,0,0,0}; FTCCoverGrid G={5,1,Line,Stages};
 TestTrue(TEXT("whole solid at 2.9m covers"),FTCCoverPreview::SolidToward(G,1.2,1,1,0));
 Stages[2]=8; TestTrue(TEXT("damaged solid still covers"),FTCCoverPreview::SolidToward(G,1.2,1,1,0));
 Stages[2]=16; TestFalse(TEXT("nearly destroyed solid at 2.9m does not cover"),FTCCoverPreview::SolidToward(G,1.2,1,1,0));
 FTCCoverPreview P; P.Start(K);
 FTCCoverPointer Pointer; Pointer.bInside=Pointer.bOverView=Pointer.bGroundHit=Pointer.bAimAllows=true; Pointer.Ground={9,9};
 P.Frame(0,K,Pointer);
 TestEqual(TEXT("only known terrain produces a ground shield"),P.CursorMarks().Ground.Num(),1);
 TestEqual(TEXT("known house and sandbags have through-model shields"),P.CursorMarks().Through.Num(),2);
 const auto Ground=P.CursorMarks().Ground, Through=P.CursorMarks().Through;
 P.Flash(9,9,K); P.Frame(.1,K,Pointer);
 TestTrue(TEXT("destination uses identical classifications"),P.FlashMarks().Ground==Ground && P.FlashMarks().Through==Through);
 TestTrue(TEXT("flash is visible"),P.View().bFlashGround);
 P.Start(K); P.Frame(0,K,Pointer);
 TestTrue(TEXT("new match clears destination marks"),P.FlashMarks().Ground.IsEmpty() && P.FlashMarks().Through.IsEmpty());
 TestFalse(TEXT("new match ends destination flash"),P.View().bFlashGround);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30Rally, "ThreeCrossroads.Look30.RallyVisibility", TCLook30Test::Flags)
bool FTCLook30Rally::RunTest(const FString&)
{
 TMap<int32,FTCMarkUnit> Units;
 for (int32 Id : {1,2}) { auto& U=Units.Add(Id); U.Id=Id; U.At={double(Id)*10,10}; U.bRally=true; U.Rally={double(Id)*20,30}; }
 const FVector2D Army(90,90);
 const auto ArmyOnly=TCOrders::Markers({},Units,&Army);
 TestTrue(TEXT("army rally exists without a selection"),ArmyOnly.Num()>0);
 TestTrue(TEXT("army flag is at delivered army rally"),ArmyOnly.ContainsByPredicate([&](const FTCMarkPrim& P){return P.Op==FTCMarkPrim::EOp::Ring && P.At==Army;}));
 const auto Selected=TCOrders::Markers({1},Units,&Army);
 TestTrue(TEXT("selected building flag is present"),Selected.ContainsByPredicate([](const FTCMarkPrim& P){return P.Op==FTCMarkPrim::EOp::Ring && P.At==FVector2D(20,30);}));
 TestFalse(TEXT("unselected building flag is absent"),Selected.ContainsByPredicate([](const FTCMarkPrim& P){return P.Op==FTCMarkPrim::EOp::Ring && P.At==FVector2D(40,30);}));
 const auto WithoutOther=TCOrders::Markers({1},TMap<int32,FTCMarkUnit>{{1,Units[1]}},&Army);
 TestEqual(TEXT("unselected building contributes no primitives"),Selected.Num(),WithoutOther.Num());
 TestTrue(TEXT("selected building adds its rally"),Selected.Num()>ArmyOnly.Num());
 Units[1].Rally={50,60}; const auto Replacement=TCOrders::Markers({1},Units,nullptr);
 TestFalse(TEXT("replacement has no old rally ring"),Replacement.ContainsByPredicate([](const FTCMarkPrim& P){return P.Op==FTCMarkPrim::EOp::Ring && P.At==FVector2D(20,30);}));
 Units[1].bRally=false;
 TestEqual(TEXT("reset clears both army and selected-building rally"),TCOrders::Markers({1},Units,nullptr).Num(),0);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30Logistics, "ThreeCrossroads.Look30.LogisticsGrouping", TCLook30Test::Flags)
bool FTCLook30Logistics::RunTest(const FString&)
{
 FTCMatchState S; S.bHasMatch=true; S.You=S.LobbySeat=0; S.bLobbySinceOpen=true;
 FTCLogistics L; L.ApplyStart();
 auto Send=[&](int32 Tick,const TCHAR* Events)
 {
  const auto J=TCLook30Test::Json(FString::Printf(TEXT("{\"tick\":%d,\"logistics\":{\"enabled\":true,\"events\":%s}}"),Tick,Events));
  L.ApplySnapshot(*J,S); return L.TakeAlerts();
 };
 const auto First=Send(0,TEXT("[{\"id\":1,\"type\":\"low\",\"x\":1,\"z\":1,\"count\":2},{\"id\":2,\"type\":\"lowReserve\",\"x\":29,\"z\":29,\"count\":3},{\"id\":3,\"type\":\"blocked\",\"x\":1,\"z\":1},{\"id\":4,\"type\":\"low\",\"x\":30,\"z\":1}]"));
 if (!TestEqual(TEXT("same text and 30m area merge; other text and area separate"),First.Num(),3)) return false;
 TestEqual(TEXT("summed source count and text"),First[0].Text,FString(TEXT("Low supplies: 5")));
 TestEqual(TEXT("duplicate ids stay suppressed"),Send(398,TEXT("[{\"id\":1,\"type\":\"low\",\"x\":1,\"z\":1}]")).Num(),0);
 TestEqual(TEXT("new event inside original 20 second throttle"),Send(399,TEXT("[{\"id\":5,\"type\":\"low\",\"x\":1,\"z\":1}]")).Num(),0);
 TestEqual(TEXT("new id at original 20 second boundary emits"),Send(400,TEXT("[{\"id\":6,\"type\":\"low\",\"x\":1,\"z\":1}]")).Num(),1);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30Ghost, "ThreeCrossroads.Look30.GhostLifecycle", TCLook30Test::Flags)
bool FTCLook30Ghost::RunTest(const FString&)
{
 UWorld* W=UWorld::CreateWorld(EWorldType::Game,false,TEXT("TCLook30Ghost"));
 if (!TestNotNull(TEXT("world"),W)) return false;
 GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(W);
 FTCWorldObjectsView View;
 ON_SCOPE_EXIT { View.Detach(); GEngine->DestroyWorldContext(W); W->DestroyWorld(false); };
 if (!TestTrue(TEXT("world model renderer attaches"),View.Attach(W))) return false;
 FTCWorldObjects Model;
 FTCMatchState S; S.bHasMatch=true;
 const FString Row=TEXT("[7,\"barracks\",1,20,30,0,100,0,0,0,0,0,0,0,0]");
 auto Send=[&](const FString& Units,const FString& Ghosts)
 {
  const auto J=TCLook30Test::Json(TEXT("{\"all\":true,\"units\":")+Units+TEXT(",\"ghosts\":")+Ghosts+TEXT("}"));
  S.ApplySnapshot(*J); View.DrawModels(Model,S,nullptr);
 };
 Send(TEXT("[")+Row+TEXT("]"),TEXT("[]"));
 TestTrue(TEXT("live building has a single identity"),S.Units.Contains(7) && S.Ghosts.IsEmpty() && View.GhostsDrawn==0);
 Send(TEXT("[]"),TEXT("[[7,\"barracks\",1,20,30,1]]"));
 if (!TestEqual(TEXT("lost sight leaves one delivered remembered identity"),S.Ghosts.Num(),1)) return false;
 TestTrue(TEXT("last known position is retained"),S.Ghosts[0].X==20 && S.Ghosts[0].Z==30 && !S.Units.Contains(7));
 TestEqual(TEXT("one remembered model is submitted"),View.GhostsDrawn,1);
 int32 Parts=0;
 const auto* Remembered=View.Library.Resolve({TEXT("barracks"),1,FString()});
 if (!TestNotNull(TEXT("remembered source building"),Remembered)) return false;
 const FTransform LastSeen(FVector(2000,3000,0));
 for (const auto& Pair : View.ModelComps) if (const auto* Component=Pair.Value.Get())
  for (int32 I=0; I<Component->GetInstanceCount(); I++)
  {
   const int32 FadeAt=I*Component->NumCustomDataFloats+TCModelData::Fade;
   if (!TestTrue(TEXT("drawn ghost has custom fade"),Component->PerInstanceSMCustomData.IsValidIndex(FadeAt))) return false;
   TestEqual(TEXT("actual remembered-model instance uses source fade"),Component->PerInstanceSMCustomData[FadeAt],.35f);
   FTransform Drawn; Component->GetInstanceTransform(I,Drawn,true);
   bool AtLastSeen=false;
   for (int32 P=0; P<Remembered->Parts.Num(); P++)
    if (Component->GetStaticMesh()==Remembered->Parts[P].Mesh.TryLoad())
     AtLastSeen|=Drawn.Equals(FTCModelLibrary::PartTransform(*Remembered,P,LastSeen,{}),.02);
   TestTrue(TEXT("actual ghost parts stay at delivered last-seen coordinates"),AtLastSeen);
   Parts++;
  }
 TestTrue(TEXT("remembered building has drawn original parts"),Parts>0);
 const FString Reappeared=TEXT("[7,\"barracks\",1,40,50,0,100,0,0,0,0,0,0,0,0]");
 Send(TEXT("[")+Reappeared+TEXT("]"),TEXT("[[7,\"barracks\",1,20,30,1]]"));
 TestTrue(TEXT("reappearance replaces the ghost even if both are delivered"),S.Units.Contains(7) && S.Ghosts.IsEmpty() && View.GhostsDrawn==0);
 TestTrue(TEXT("live replacement uses newly delivered coordinates"),S.Units[7].X==40 && S.Units[7].Z==50);
 Send(TEXT("[]"),TEXT("[]")); TestTrue(TEXT("removal clears both views"),S.Units.IsEmpty() && S.Ghosts.IsEmpty());
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30Debris, "ThreeCrossroads.Look30.ReconnectPose", TCLook30Test::Flags)
bool FTCLook30Debris::RunTest(const FString&)
{
 FTCMatchState S; S.Tick=200;
 FTCFalling F; F.Id=TEXT("mid-collapse"); F.Kind=TEXT("section"); F.X=10; F.Y=4; F.Z=20; F.Tilt=.4; S.Falling.Add(F);
 FTCShot Old; Old.K=TEXT("collapse"); Old.Id=TEXT("past-contact"); Old.At=9; S.Shots.Add(Old);
 FTCDebrisView Recipient; FTCDebrisRow Pose; int32 Contacts=0,Removed=0;
 Recipient.On.Remove=[&](const FString&){Removed++;};
 Recipient.On.Pose=[&](const FString&,const FTCDebrisRow& R){Pose=R;};
 Recipient.On.Contact=[&](const FTCShot&,const FTCShotExtra&){Contacts++;};
 Recipient.Snapshot(S,{},{},10);
 TestTrue(TEXT("fresh recipient starts immediately at current delivered pose"),Pose.X==10 && Pose.Y==4 && Pose.Tilt==.4);
 TestEqual(TEXT("fresh recipient does not replay historical contact"),Contacts,0);
 S.Tick=202; S.Falling[0].X=14; S.Falling[0].Y=2; S.Falling[0].Tilt=.8;
 FTCShot Fresh=Old; Fresh.Id=TEXT("fresh-contact"); Fresh.At=10.1; S.Shots.Add(Fresh);
 Recipient.Snapshot(S,{},{},10.1); Recipient.Frame(10.15);
 TestEqual(TEXT("fresh delivered contact emits once"),Contacts,1);
 TestTrue(TEXT("one interpolation to next delivered pose"),FMath::IsNearlyEqual(Pose.X,12.,1e-6) && FMath::IsNearlyEqual(Pose.Y,3.,1e-6) && FMath::IsNearlyEqual(Pose.Tilt,.6,1e-6));
 Recipient.Frame(20); TestTrue(TEXT("no extrapolation after latest delivered pose"),Pose.X==14 && Pose.Y==2);
 Recipient.Snapshot(S,{},{},10.2);
 TestEqual(TEXT("neither old nor duplicate contact replays"),Contacts,1);
 Recipient.Reset(); S.Tick=220; Recipient.Snapshot(S,{},{},11);
 TestTrue(TEXT("reset restores current delivered pose immediately"),Pose.X==14 && Pose.Y==2);
 TestEqual(TEXT("reset does not replay either historical contact"),Contacts,1);
 S.Falling.Reset(); S.Tick=222; Recipient.Snapshot(S,{},{},11.1);
 TestEqual(TEXT("expired section is removed"),Recipient.Count(),0);
 TestEqual(TEXT("reset and expiry each remove the section once"),Removed,2);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30CarrierAir, "ThreeCrossroads.Look30.CarrierAircraftLifecycle", TCLook30Test::Flags)
bool FTCLook30CarrierAir::RunTest(const FString&)
{
 UWorld* W=UWorld::CreateWorld(EWorldType::Game,false,TEXT("TCLook30Air"));
 if (!TestNotNull(TEXT("world"),W)) return false;
 GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(W);
 ON_SCOPE_EXIT { GEngine->DestroyWorldContext(W); W->DestroyWorld(false); };
 auto* View=W->SpawnActor<ATCUnitRenderer>(); FTCUnitTable Table; FString Error;
 if (!TestNotNull(TEXT("renderer"),View) || !TestTrue(TEXT("source unit definitions"),Table.Load(Error))) return false;
 FTCMatchState S; S.bHasMatch=true; S.Factions={0};
 FTCUnit Carrier; Carrier.Id=1; Carrier.Type=TEXT("tank"); Carrier.Hp=100; Carrier.X=Carrier.Z=20; Carrier.Owner=0;
 Carrier.Flags=TCCargo::Flag|(2<<TCCargo::Shift); S.Units.Add(1,Carrier);
 FTCUnit Rider; Rider.Id=2; Rider.Type=TEXT("rifle"); Rider.Owner=0; Rider.Hp=100; Rider.X=Rider.Z=20; Rider.Flags=TCFlags::Riding;
 S.Units.Add(2,Rider);
 TestEqual(TEXT("delivered cargo count is squads, not individual men"),TCCargo::Count(Carrier.Flags),2);
 TestEqual(TEXT("exact tank cargo tag"),TCLook30::CargoLabel(TCCargo::Count(Carrier.Flags),Table.Find(TEXT("tank"))->Carries),FString(TEXT("Carrying 2 of 2 squads")));
 TestEqual(TEXT("exact interior passenger tag"),TCLook30::CargoLabel(1,Table.Find(TEXT("halftrack"))->Carries),FString(TEXT("Carrying a squad")));
 TestFalse(TEXT("delivered RIDING hides the independent squad body and markers"),ATCUnitRenderer::IsDrawn(Rider));
 View->Draw(S,Table,nullptr,{},0);
 TestEqual(TEXT("tank carries two squads as six deck men"),View->ModelStats().Riders,6);
 S.Units[1].Flags=0; View->Draw(S,Table,nullptr,{},.016f);
 TestEqual(TEXT("unload clears all deck riders"),View->ModelStats().Riders,0);
 TestEqual(TEXT("unload creates no rider corpses"),View->SoldierCrowd()->NumCorpses(),0);
 S.Units[1].Flags=Carrier.Flags;
 S.Units[1].Type=TEXT("halftrack"); View->Draw(S,Table,nullptr,{},.016f);
 TestEqual(TEXT("interior passengers do not become deck men"),View->ModelStats().Riders,0);
 TestEqual(TEXT("interior rider submits no independent squad"),View->ModelStats().CrowdMen,0);
 TestEqual(TEXT("interior rider submits no far squad"),View->ModelStats().FarMen,0);
 S.Units.Remove(2); S.Units[1].Type=TEXT("fighter");
 FTCCombatEffects Fx;
 // Parked, launch, orbit, return and rearm use the delivered visibility flag. Smoke also obeys that flag.
 struct FStep { const TCHAR* Name; int32 Flags; double Hp; bool Visible; };
 const FStep Steps[]={{TEXT("parked"),TCClientFlags::PlaneAtBase,.2,false},{TEXT("launch"),0,1,true},
  {TEXT("orbit"),0,.5,true},{TEXT("return"),0,.2,true},{TEXT("rearm"),TCClientFlags::PlaneAtBase,.2,false}};
 for (const auto& Step : Steps)
 {
  S.Units[1].Flags=Step.Flags; S.Units[1].Rot=1; View->Draw(S,Table,nullptr,{},.016f);
  TestEqual(Step.Name,View->ModelStats().AircraftShadows,Step.Visible?1:0);
  TestEqual(TEXT("body visibility follows delivered air state"),View->ModelStats().ModelUnits,Step.Visible?1:0);
  Fx.Reset(); Fx.PlaneDamageFrame(1,Step.Hp,ATCUnitRenderer::IsDrawn(S.Units[1]),.1f,FVector(20,20,20),0,-2,2);
  TestEqual(TEXT("smoke appears only for visible damaged aircraft"),Fx.Particles().Num()>0,Step.Visible && Step.Hp<.6);
 }
 TestTrue(TEXT("hidden plane heading still advances"),View->MotionOf(1) && View->MotionOf(1)->Heading>0);
 S.Units[1].Flags=0; S.Tick=20; Fx.Start(S); S.Units.Reset();
 FTCShot Down; Down.K=TEXT("planedown"); Down.bHasTarget=true; Down.T=1; Down.bHasPos=true; Down.X=Down.Z=20;
 S.Shots.Add(Down); FTCFxContext C; C.Table=&Table; Fx.Snapshot(S,nullptr,C,1); Fx.Frame(C,1.1,.1f);
 TestEqual(TEXT("delivered shoot-down starts one falling aircraft"),Fx.AircraftNow().Num(),1);
 View->Draw(S,Table,nullptr,{},.016f);
 TestEqual(TEXT("shot down removes the live aircraft shadow"),View->ModelStats().AircraftShadows,0);
 TestNull(TEXT("removed aircraft forgets motion state"),View->MotionOf(1));
 Fx.Reset(); S.Shots.Reset(); Fx.Start(S); Fx.Snapshot(S,nullptr,C,2); Fx.Frame(C,2.1,.1f);
 TestEqual(TEXT("fog removal alone never invents a shoot-down"),Fx.AircraftNow().Num(),0);
 return true;
}
#endif
