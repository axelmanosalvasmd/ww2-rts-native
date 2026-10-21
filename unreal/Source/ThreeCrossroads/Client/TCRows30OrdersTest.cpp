#include "Client/TCRows30Harness.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCArmyCommands.h"
using namespace TCRows30;

TC_ROWS30_TEST(FTCRows30GroupTest,"CLI44.GroupMembership");
bool FTCRows30GroupTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing(); S.Unit(1,TEXT("rifle")); S.Unit(2,TEXT("rifle"),0,120,80); S.Unit(3,TEXT("truck")); S.Unit(4,TEXT("hq"));
 auto& G=S.PC->Groups; auto& Selected=S.PC->Selected;
 auto Apply=[&](int32 N,bool Ctrl,bool Shift,bool Alt,double Now=0.) { return G.Apply(N,FTCControlGroups::OpForChord(Ctrl,Shift,Alt),S.M->State,S.M->UnitTable,Selected,Now); };
 Selected={1,3,4}; Apply(1,true,false,false); TestTrue(TEXT("Ctrl set excludes truck and retains building"),G.Members(1).Num()==2 && G.Members(1).Contains(1) && G.Members(1).Contains(4));
 Selected={2,3}; Apply(1,false,true,false); Apply(1,true,true,false); TestTrue(TEXT("Shift and Ctrl+Shift append uniquely without trucks"),G.Members(1).Num()==3 && G.Members(1).Contains(2));
 Selected={1,3,4}; Apply(2,false,false,true); TestTrue(TEXT("Alt transfers only army out of other groups"),G.Members(2)==TArray<int32>{1} && !G.Members(1).Contains(1) && G.Members(1).Contains(4));
 Selected.Reset(); auto First=Apply(2,false,false,false,0); auto Boundary=Apply(2,false,false,false,.3);
 TestTrue(TEXT("recall replaces selection and exact 300ms centres"),Selected.Num()==1 && Selected.Contains(1) && !First.bCentre && Boundary.bCentre);
 TestFalse(TEXT("strictly past 300ms does not centre"),Apply(2,false,false,false,.6000005).bCentre);
 TestFalse(TEXT("backwards clock does not centre"),Apply(2,false,false,false,.1).bCentre);
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30TruckBoxTest,"CLI45.TruckBoxSelection");
bool FTCRows30TruckBoxTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing();
 S.Unit(1,TEXT("rifle")); S.Unit(2,TEXT("truck")); S.Unit(3,TEXT("truck"),1); S.Unit(4,TEXT("train"));
 // All four models occupy the same visible point. A full projected box exercises BoxSelect, not eligibility alone.
 FTCRows30Access::Projection(*S.PC);
 S.Cam->Place(FVector2D(100,60),5,-90,54.43099f,60);
 TestFalse(TEXT("ordinary box mode starts off"),S.M->Logistics.SelectionModeActive());
 S.PC->BoxSelect(FVector2D(-100000,-100000),FVector2D(100000,100000),false);
 TestTrue(TEXT("ordinary drag takes infantry alone"),S.PC->Selected.Num()==1 && S.PC->Selected.Contains(1));
 TestTrue(TEXT("actual toolbar switch toggles on"),S.PC->RunUi(ETCUiAction::TruckSelect));
 S.PC->BoxSelect(FVector2D(-100000,-100000),FVector2D(100000,100000),false);
 TestTrue(TEXT("truck drag takes only own mobile truck"),S.PC->Selected.Num()==1 && S.PC->Selected.Contains(2));
 S.PC->RunUi(ETCUiAction::TruckSelect);
 S.PC->BoxSelect(FVector2D(-100000,-100000),FVector2D(100000,100000),false);
 TestTrue(TEXT("switch off restores normal drag"),S.PC->Selected.Num()==1 && S.PC->Selected.Contains(1));
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30DispatchTest,"CLI47.RightClickPriority");
bool FTCRows30DispatchTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing();
 S.Unit(1,TEXT("rifle")); S.Unit(2,TEXT("mortar")); S.Unit(3,TEXT("truck")); S.Unit(4,TEXT("fighter")); S.Unit(5,TEXT("engineer"));
 S.Unit(10,TEXT("halftrack")); S.Unit(11,TEXT("barracks")); S.Unit(12,TEXT("rifle"),1);
 S.M->State.Units[11].Hp=1;
 auto Start=Json(TEXT(R"({"you":0,"factions":[0,1],"teams":[0,1],"map":{"w":128,"h":64,"rows":[]}})"));
 S.M->Support.ApplyStart(*Start); S.M->Field.ApplyStart(*Start); S.M->Logistics.ApplyStart(); S.M->Logistics.ApplySnapshot(*Json(TEXT(R"({"logistics":{"enabled":true}})")),S.M->State);
 S.M->Support.ApplySnapshot(*Json(TEXT(R"({"units":[[11,"barracks",0,100,60,0,0,1,0,0,0,0,0,0,0.5]]})")));

 FTCCursor C; C.bHasGround=true; C.Ground=FVector2D(100,60); C.Friend=10; C.Building=11; C.Enemy=12; C.bHouse=true; C.House=FVector2D(110,70);
 S.M->Field.ApplySnapshot(*Json(TEXT(R"({"units":[[1,"rifle",0,100,60,0,0,100,0,0,0,0,0],[5,"engineer",0,100,60,0,0,100,0,0,0,0,0]],"works":[[30,0,100,60,0,"trench"]]})")));
 struct FCase { TSet<int32> Ids; bool Friend,Building,Enemy,House; const TCHAR* Type; };
 const FCase Cases[]={{{3},true,true,true,true,TEXT("supply")},{{4},true,true,true,true,TEXT("escort")},{{1},true,true,true,true,TEXT("board")},{{5},false,true,true,true,TEXT("assist")},{{1},false,false,true,true,TEXT("entrench")},{{2},false,false,true,true,TEXT("attack")},{{1,2,3},false,false,false,true,TEXT("orders")}};
 for(const auto& Row:Cases)
 { if(Row.Ids.Num()==3) S.M->Field.ApplySnapshot(*Json(TEXT(R"({"works":[]})")));
  S.Socket->Sent.Reset(); S.PC->Selected=Row.Ids; FTCCursor At=C; if(!Row.Friend) At.Friend=0; if(!Row.Building) At.Building=0; if(!Row.Enemy) At.Enemy=0; At.bHouse=Row.House;
  S.PC->IssueOrder(At,false,true); auto O=S.Last(*this,Row.Type); if(!O) continue;
  TestTrue(TEXT("one queue flag on outer command"),O->GetBoolField(TEXT("queue")));
  if(FString(Row.Type)!=TEXT("orders"))
  {
   const auto& Ids=O->GetArrayField(TEXT("ids")); TestEqual(TEXT("only eligible selected unit is dispatched"),Ids.Num(),1);
   if(Ids.Num()==1) TestEqual(TEXT("exact selected id"),int32(Ids[0]->AsNumber()),Row.Ids.Array()[0]);
   if(O->HasField(TEXT("target"))) TestEqual(TEXT("priority resolves exact target"),int32(O->GetNumberField(TEXT("target"))),FString(Row.Type)==TEXT("attack")?12:10);
   if(FString(Row.Type)==TEXT("assist")) TestEqual(TEXT("assist names exact site"),O->GetNumberField(TEXT("id")),11.);
  }
  if(FString(Row.Type)==TEXT("entrench")) TestEqual(TEXT("delivered planned works beat enemy and house"),O->GetNumberField(TEXT("join")),30.);
  if(FString(Row.Type)==TEXT("orders"))
  { const auto& Parts=O->GetArrayField(TEXT("commands")); TestEqual(TEXT("house partitions infantry, gun and truck"),Parts.Num(),3);
   const TCHAR* Types[]={TEXT("garrison"),TEXT("fireat"),TEXT("move")};
   for(int32 I=0;I<Parts.Num() && I<3;I++) { auto P=Parts[I]->AsObject(); TestEqual(TEXT("reference house command priority"),P->GetStringField(TEXT("t")),FString(Types[I])); TestFalse(TEXT("parts have no queue field"),P->HasField(TEXT("queue")));
    if(I<2) { const auto& IDs=P->GetArrayField(TEXT("ids")); TestTrue(TEXT("house part names the expected infantry or gun"),IDs.Num()==1 && int32(IDs[0]->AsNumber())==I+1); }
    else { const auto& Moves=P->GetArrayField(TEXT("orders")); TestTrue(TEXT("house move names the remaining truck"),Moves.Num()==1 && int32(Moves[0]->AsArray()[0]->AsNumber())==3); } }
  }
 }
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30AttackMoveTest,"CLI49.AttackMoveAndStop");
bool FTCRows30AttackMoveTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing(); S.Unit(1,TEXT("rifle")); S.PC->Selected={1};
 TestTrue(TEXT("G invokes production binding"),FTCRows30Access::Binding(*S.PC,TEXT("KeyG"))); TestTrue(TEXT("G arms aim"),S.PC->Aim==ETCAim::AttackMove);
 S.PC->CancelAim(); TestTrue(TEXT("right-click/Esc cancellation entry clears aim"),S.PC->Aim==ETCAim::None && S.Socket->Sent.IsEmpty());
 FTCRows30Access::Binding(*S.PC,TEXT("KeyG")); S.PC->AimClick(true,FVector2D(120,70),true);
 auto Armed=S.Last(*this,TEXT("amove")); if(Armed) TestTrue(TEXT("Shift armed click queues"),Armed->GetBoolField(TEXT("queue")));
 TestTrue(TEXT("click spends G aim"),S.PC->Aim==ETCAim::None);
 FTCCursor C; C.bHasGround=true; C.Ground=FVector2D(120,70);
 for(bool Queue:{false,true}) { S.Socket->Sent.Reset(); S.PC->IssueOrder(C,true,Queue); auto O=S.Last(*this,TEXT("amove")); if(O) TestEqual(TEXT("Ctrl right-click keeps Shift queue"),O->GetBoolField(TEXT("queue")),Queue); }
 S.Socket->Sent.Reset(); S.PC->IssueOrder(C,false,false); S.Last(*this,TEXT("move"));
 S.Socket->Sent.Reset(); FTCRows30Access::Binding(*S.PC,TEXT("KeyX")); auto Stop=S.Last(*this,TEXT("stop"));
 if(Stop) TestEqual(TEXT("Stop has only source fields"),TCOrders::ToJson(Stop),FString(TEXT("{\"t\":\"stop\",\"ids\":[1]}")));
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30ArmyTest,"CLI50.StanceRetreatRally");
bool FTCRows30ArmyTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing(); S.Unit(1,TEXT("rifle")); S.Unit(2,TEXT("rifle")); S.Unit(3,TEXT("hq")); S.Unit(4,TEXT("rifle"),1);
 S.PC->Selected={1,2,3,4};
 for(const auto& Pair:TArray<TPair<FString,int32>>{{TEXT("holdFire"),2048},{TEXT("holdPos"),4096},{TEXT("autoRetreat"),8192}})
 {
  FTCUiCommand Stance(ETCUiAction::Stance); Stance.Unit=Pair.Key;
  S.M->State.Units[1].Flags=Pair.Value; S.M->State.Units[2].Flags=0; S.Socket->Sent.Reset(); S.PC->RunCommand(Stance);
  auto On=S.Last(*this,TEXT("stance")); if(On) { TestTrue(TEXT("mixed aggregate switches on"),On->GetBoolField(TEXT("on"))); TestEqual(TEXT("only own mobile ids"),On->GetArrayField(TEXT("ids")).Num(),2); }
  S.M->State.Units[2].Flags=Pair.Value; S.Socket->Sent.Reset(); S.PC->RunCommand(Stance); auto Off=S.Last(*this,TEXT("stance")); if(Off) TestFalse(TEXT("all set switches off"),Off->GetBoolField(TEXT("on")));
 }
 S.PC->Selected={1}; S.Socket->Sent.Reset(); auto* A=S.M->Audio(); if(!TestNotNull(TEXT("audio subsystem"),A)) return false;
 const int32 Before=FTCRows30Access::VoiceAsked(*A,TEXT("retreat")); FTCRows30Access::Binding(*S.PC,TEXT("KeyR"));
 S.Last(*this,TEXT("retreat")); TestEqual(TEXT("R requests exactly one retreat bark"),FTCRows30Access::VoiceAsked(*A,TEXT("retreat")),Before+1);
 for(bool Classic:{false,true})
 { S.M->State.Mode=Classic?TEXT("classic"):TEXT("conquest"); S.M->State.ModeRow.Kind=S.M->State.Mode; S.PC->Selected=Classic?TSet<int32>{3}:TSet<int32>{1};
  S.Socket->Sent.Reset(); FTCRows30Access::Binding(*S.PC,TEXT("KeyH"),true); TestTrue(TEXT("Shift+H arms rally"),S.PC->Aim==ETCAim::Rally);
  S.PC->AimClick(true,FVector2D(120,70),false); auto Rally=S.Last(*this,TEXT("rally")); if(Rally) { TestEqual(TEXT("only building rally includes ids"),Rally->HasField(TEXT("ids")),Classic); TestEqual(TEXT("rally x"),Rally->GetNumberField(TEXT("x")),120.); TestEqual(TEXT("rally z"),Rally->GetNumberField(TEXT("z")),70.); }
 }
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30CoverTest,"CLI51.TakeCover");
bool FTCRows30CoverTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing(); S.Unit(1,TEXT("rifle")); S.Unit(2,TEXT("tank"));
 S.PC->Selected={1};
 for(bool Queue:{false,true}) { S.Socket->Sent.Reset(); S.PC->RunCommand(FTCUiCommand(ETCUiAction::Cover),Queue); auto O=S.Last(*this,TEXT("cover"));
  if(O) TestEqual(TEXT("cover exact selected ids and queue"),TCOrders::ToJson(O),Queue?FString(TEXT("{\"t\":\"cover\",\"ids\":[1],\"queue\":true}")):FString(TEXT("{\"t\":\"cover\",\"ids\":[1],\"queue\":false}"))); }
 S.Socket->Sent.Reset(); FTCRows30Access::Binding(*S.PC,TEXT("KeyC"),true); auto Key=S.Last(*this,TEXT("cover")); if(Key) TestFalse(TEXT("Shift+C key does not queue"),Key->GetBoolField(TEXT("queue")));
 for(TSet<int32> Ids:{TSet<int32>{},TSet<int32>{2}}) { S.Socket->Sent.Reset(); S.PC->Selected=Ids; TestFalse(TEXT("no eligible infantry produces no order"),S.PC->RunUi(ETCUiAction::Cover)); TestTrue(TEXT("no transport submission"),S.Socket->Sent.IsEmpty()); }
 return !HasAnyErrors();
}
#endif
