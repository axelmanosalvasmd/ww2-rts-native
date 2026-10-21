// Decoded HUD/action fixtures. Only the integration lead executes engine automation.
#include "Client/TCHUD.h"
#include "Client/TCHUD30Core.h"
#include "Client/TCGameMode.h"
#include "Client/TCUnitTable.h"
#include "Client/TCLogistics.h"
#include "Client/TCAlerts.h"
#include "Client/TCLocale.h"
#include "Client/TCWorldRecovery.h"
#include "Client/TCFieldworks.h"
#include "Client/TCConstruction.h"
#include "Client/TCSupportOrders.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Engine/Engine.h"
#include "Engine/GameInstance.h"
#include "Engine/World.h"
#include "GameFramework/WorldSettings.h"
#include "Client/TCHUD30Reference.inl"

namespace Hud30
{
 TSharedPtr<FJsonObject> Parse(const FString& Text)
 { TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text),O); return O; }
 TSharedPtr<FJsonObject> Start(int32 Size=64)
 {
  auto O=Parse(TEXT(R"({"you":0,"teams":[0,1],"factions":[0,1],"names":["Alder","Fern"],"map":{}})"));
  auto Map=O->GetObjectField(TEXT("map")); Map->SetNumberField(TEXT("w"),Size); Map->SetNumberField(TEXT("h"),Size);
  TArray<TSharedPtr<FJsonValue>> Rows; for (int32 I=0;I<Size;I++) Rows.Add(MakeShared<FJsonValueString>(FString::ChrN(Size,'.')));
  Map->SetArrayField(TEXT("rows"),Rows); return O;
 }
 void Confirm(FTCMatchState& S) { S.You=S.LobbySeat=0; S.bLobbySinceOpen=true; }
 TArray<int32> Ids(const TSharedPtr<FJsonObject>& M)
 { TArray<int32> A; if (M) for (const auto& V : M->GetArrayField(TEXT("ids"))) A.Add((int32)V->AsNumber()); A.Sort(); return A; }
 TArray<FString> Strings(const TArray<TSharedPtr<FJsonValue>>& Values)
 { TArray<FString> A; for (const auto& V : Values) A.Add(V->AsString()); return A; }
 struct FWorld
 {
  UWorld* World=nullptr; ATCGameMode* Mode=nullptr; ATCPlayerController* PC=nullptr; ATCHUD* Hud=nullptr;
  FWorld()
  {
   if (!GEngine) return;
   World=UWorld::CreateWorld(EWorldType::Game,false); if (!World) return;
   auto* Instance=NewObject<UGameInstance>(World); World->SetGameInstance(Instance);
   auto& Context=GEngine->CreateNewWorldContext(EWorldType::Game); Context.SetCurrentWorld(World); Context.OwningGameInstance=Instance;
   World->GetWorldSettings()->DefaultGameMode=ATCGameMode::StaticClass();
   if (!World->SetGameMode(FURL())) return;
   Mode=World->GetAuthGameMode<ATCGameMode>(); PC=World->SpawnActor<ATCPlayerController>(); Hud=World->SpawnActor<ATCHUD>();
  }
  ~FWorld() { if (World) { World->DestroyWorld(false); GEngine->DestroyWorldContext(World); } }
  bool Ready() const { return Mode && PC && Hud; }
 };
}
struct FTCHud30Access
{
 static void Set(ATCHUD& H,ATCPlayerController* PC,const FTCHudFit::FBottomLayout& L)
 {
  H.PlayerOwner=PC; H.Buttons.Reset(); H.bShowHUD=true;
  for (const auto& Hit : L.Hits) H.Buttons.Add({Hit.Box,Hit.Control.Command,Hit.Control.Label,Hit.Control.bEnabled,FTCHudFit::ControlTip(Hit.Control)});
 }
 static void Cover(ATCHUD& H,const FBox2D& Box) { H.Buttons.Add({Box,FTCUiCommand(),FString(),false,FString()}); }
 static void Hide(ATCHUD& H) { H.bShowHUD=false; }
 static FTCMinimap& Map(ATCHUD& H) { return H.Minimap; }
 static void DrawOverlays(ATCHUD& H) { H.DrawMinimapOverlays(100); }
};

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30ScoreTest,"ThreeCrossroads.HUD30.ScoreTeams",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30ScoreTest::RunTest(const FString&)
{
 FTCScoreStrip S; S.Kind=TEXT("classic"); S.Lead.bShown=true; S.Lead.Mode=TEXT("Sudden death in"); S.Lead.Clock=TEXT("25:00");
 for (const TCHAR* Name : {TEXT("Alder"),TEXT("Fern")}) { FTCScoreTeam T; FTCScoreSeat P; P.Name=Name; P.Held=TEXT("0 held"); T.Seats.Add(P); T.Num=TEXT("3000 / 3000"); T.bBar=true; S.Teams.Add(T); }
 for (float Pixels : {1280.f,1920.f}) for (float Scale : {1.f,1.5f})
 {
  const auto P=FTCHudFit::ScoreCard(S,Pixels/Scale,FTCPaint::ApproxMeasure(),false,Scale);
  const auto* Card=P.Find(TEXT("scores:panel")); if (!TestNotNull(TEXT("opaque scoreboard"),Card)) return false;
  TestEqual(TEXT("world flag and label cannot bleed through"),Card->Colour.A,1.f);
  TArray<FVector2D> Seats; for (const auto& I : P.Items) if (I.Role==TEXT("scores:seat")) Seats.Add(I.Pos);
  if (!TestEqual(TEXT("both team names drawn"),Seats.Num(),2)) return false;
  TestTrue(TEXT("teams share a row or a clean column, never a diagonal"),FMath::Abs(Seats[0].Y-Seats[1].Y)<.01 || FMath::Abs(Seats[0].X-Seats[1].X)<.01);
  for (auto At : Seats) TestTrue(TEXT("team remains inside card"),P.Panels[0].IsInsideOrOn(At));
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30SelectionTest,"ThreeCrossroads.HUD30.DecodedSelection",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30SelectionTest::RunTest(const FString&)
{
 Hud30::FWorld W; if (!TestTrue(TEXT("input fixture"),W.Ready())) return false;
 FString Error; if (!TestTrue(TEXT("reference unit table"),W.Mode->UnitTable.Load(Error))) return false;
 auto& S=W.Mode->State; TestTrue(TEXT("decoded start"),S.ApplyStart(*Hud30::Start())); Hud30::Confirm(S);
 auto Snap=Hud30::Parse(TEXT(R"({"tick":1,"units":[[1,"rifle",0,12,12,0,0,121,55,0,2,0,256,2,1],[2,"rifle",0,14,12,0,0,79,95,0,3,0,1,0,1],[3,"tank",0,16,12,0,0,300,0,0,0,0,0,0,1]],"mp":1000})"));
 S.ApplySnapshot(*Snap); FTCRecruitment R; FTCLogistics L;
 const auto Rows=FTCHudFit::SelectionRows(S,W.Mode->UnitTable,R,L,{1,2,3});
 if (!TestEqual(TEXT("two decoded types"),Rows.Num(),2)) return false;
 const auto* Rifle=Rows.FindByPredicate([](const auto& Row){return Row.Type==TEXT("rifle");}); if (!TestNotNull(TEXT("rifle row"),Rifle)) return false;
 TestEqual(TEXT("health summed from decoded units"),Rifle->Hp,200);
 TestEqual(TEXT("maximum summed from original definition"),Rifle->MaxHp,2*W.Mode->UnitTable.Find(TEXT("rifle"))->MaxHp());
 TestEqual(TEXT("faction label"),Rifle->Name,FString(TEXT("Rifle Squad")));
 for (const TCHAR* Tag : {TEXT("Hidden"),TEXT("Retreating"),TEXT("Suppressed"),TEXT("Pinned"),TEXT("In trench"),TEXT("By cover")}) TestTrue(Tag,Rifle->Tags.Contains(Tag));
 const auto Layout=FTCHudFit::LeftPanel(720,TEXT("3 units selected"),Rows,{},{},{},FTCPaint::ApproxMeasure());
 FTCHud30Access::Set(*W.Hud,W.PC,Layout);
 const auto* Hit=Layout.Hits.FindByPredicate([](const auto& H){return H.Control.Command.Unit==TEXT("hudpanel:type:rifle");}); if (!TestNotNull(TEXT("actual type hit"),Hit)) return false;
 FTCUiCommand C; TestTrue(TEXT("hit resolves"),W.Hud->CommandAt(Hit->Box.GetCenter(),C));
 W.PC->Selected={1,2,3}; W.Hud->RunPanelCommand(C,false); TestTrue(TEXT("click keeps type IDs"),W.PC->Selected.Num()==2 && W.PC->Selected.Contains(1) && W.PC->Selected.Contains(2));
 W.PC->Selected={1,2,3}; W.Hud->RunPanelCommand(C,true); TestTrue(TEXT("Shift click removes type IDs"),W.PC->Selected.Num()==1 && W.PC->Selected.Contains(3));
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30FeedbackTest,"ThreeCrossroads.HUD30.FeedbackAndHints",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30FeedbackTest::RunTest(const FString&)
{
 Hud30::FWorld W; if (!TestTrue(TEXT("feedback handler world"),W.Ready())) return false;
 W.PC->Aim=ETCAim::AttackMove; W.PC->AimHint=TEXT("Click where to attack-move");
 W.PC->ShowFeedback(TEXT("Not enough manpower"),2);
 TestFalse(TEXT("actual local feedback is recorded as refusal"),W.PC->bLastOrderSent);
 auto Visible=[&](double Age) { return FTCHudFit::HintText(W.PC->Aim!=ETCAim::None,W.PC->AimHint,W.PC->bLastOrderSent,W.PC->LastOrderNote,Age); };
 TestEqual(TEXT("actual handler sentence replaces hint"),Visible(0),FString(TEXT("Not enough manpower")));
 TestEqual(TEXT("armed hint restored at deadline"),Visible(2),W.PC->AimHint);
 W.PC->CancelAim(); TestTrue(TEXT("cancelled aim stays cleared after expiry"),Visible(2).IsEmpty());
 const auto Ref=Hud30::Parse(Hud30Reference);
 for (const auto& V : Ref->GetArrayField(TEXT("denials"))) { auto O=V->AsObject(); TestEqual(TEXT("full pinned deny map"),TCOrders::DenySentence(O->GetStringField(TEXT("code")),TEXT("buy")),O->GetStringField(TEXT("text"))); }
 TestEqual(TEXT("order queue distinction"),TCOrders::DenySentence(TEXT("queueFull"),TEXT("move")),FString(TEXT("This unit already has 8 queued orders")));
 TestEqual(TEXT("unknown deny hides details"),TCOrders::DenySentence(TEXT("unknown-private-target"),TEXT("move")),FString(TEXT("That order cannot happen")));
 TestTrue(TEXT("armed hint initially visible"),TCHud30::HintLayer(true,true,0)==TCHud30::EHint::Armed);
 TestTrue(TEXT("refusal replaces armed hint"),TCHud30::HintLayer(true,false,0)==TCHud30::EHint::Refusal);
 TestTrue(TEXT("refusal lasts until original two-second deadline"),TCHud30::HintLayer(true,false,1.999)==TCHud30::EHint::Refusal);
 TestTrue(TEXT("prior armed hint returns at two seconds"),TCHud30::HintLayer(true,false,2)==TCHud30::EHint::Armed);
 TestTrue(TEXT("cancelling clears armed hint"),TCHud30::HintLayer(false,false,2)==TCHud30::EHint::Empty);
 TestTrue(TEXT("success note does not restore cancelled aim"),TCHud30::HintLayer(false,true,0)==TCHud30::EHint::Empty);
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30LogisticsTest,"ThreeCrossroads.HUD30.LogisticsLabels",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30LogisticsTest::RunTest(const FString&)
{
 FTCMatchState S; S.ApplyStart(*Hud30::Start()); Hud30::Confirm(S);
 FTCUnit U; U.Id=1; U.Owner=0; U.Type=TEXT("truck"); U.X=U.Z=12; S.Units.Add(1,U);
 FTCLogistics L; L.ApplyStart(); const auto Ref=Hud30::Parse(Hud30Reference);
 for (const TCHAR* Family : {TEXT("reserves"),TEXT("trucks"),TEXT("stores")}) for (const auto& V : Ref->GetArrayField(Family))
 {
  auto Case=V->AsObject(); auto Input=MakeShared<FJsonObject>(*Case->GetObjectField(TEXT("input"))); Input->SetNumberField(TEXT("id"),1); Input->SetNumberField(TEXT("owner"),0); Input->SetNumberField(TEXT("x"),12); Input->SetNumberField(TEXT("z"),12);
  auto Data=MakeShared<FJsonObject>(); Data->SetBoolField(TEXT("enabled"),true);
  const FString Key=FString(Family)==TEXT("reserves")?TEXT("units"):Family;
  Data->SetArrayField(Key,{MakeShared<FJsonValueObject>(Input)}); auto Snap=MakeShared<FJsonObject>(); Snap->SetObjectField(TEXT("logistics"),Data); L.ApplySnapshot(*Snap,S);
  TArray<FString> Actual;
  if (Key==TEXT("units")) { U.Type=TEXT("rifle"); Actual=FTCLogistics::ReserveLabels(L.FindReserve(1)); }
  else if (Key==TEXT("trucks")) { U.Type=TEXT("truck"); Actual=FTCLogistics::TruckLabels(L.FindTruck(1)); }
  else { U.Type=TEXT("supplycache"); Actual=FTCLogistics::StoreLabels(L.StoreForUnit(U)); }
  S.Units[1]=U;
  const auto Expected=Hud30::Strings(Case->GetArrayField(TEXT("expected"))); TestEqual(TEXT("exact executed JS labels"),Actual,Expected);
  const auto Card=L.Card(S,{1}); if (!TestTrue(TEXT("decoded labels reach actual card"),Card.bVisible && Card.Entries.Num()==1)) return false;
  TestEqual(TEXT("card preserves numeric/job semantics"),Card.Entries[0].Lines,Expected);
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30RecoveryTest,"ThreeCrossroads.HUD30.WorldRecovery",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30RecoveryTest::RunTest(const FString&)
{
 FTCMatchState S; auto Start=Hud30::Start(); S.ApplyStart(*Start); Hud30::Confirm(S);
 FTCWorldRecovery R; R.ApplyStart(*Start);
 for (int32 Mp : {99,100})
 {
  auto Snap=Hud30::Parse(FString::Printf(TEXT(R"({"tick":1,"mode":{"kind":"world"},"mp":%d,"world":{"recovery":{"available":true,"hq":100,"engineer":60}}})"),Mp));
  R.ApplySnapshot(*Snap,S); const auto Card=R.Card(S,{});
  TestTrue(TEXT("missing HQ shows recovery"),Card.bShown && Card.bMissingHq); TestEqual(TEXT("actual card name"),Card.Name,FString(TEXT("Restore HQ"))); TestEqual(TEXT("actual cost"),Card.Cost,FString(TEXT("100 MP")));
  TestEqual(TEXT("resource eligibility"),Card.bDisabled,Mp<100); const auto Result=R.Recover(S,{}); TestEqual(TEXT("only affordable recovery sends"),Result.bSend,Mp>=100);
  if (Result.bSend) TestEqual(TEXT("exact recovery envelope"),TCOrders::ToJson(Result.Message),FString(TEXT(R"({"t":"recover"})")));
 }
 FTCUnit E; E.Id=7; E.Owner=0; E.Type=TEXT("engineer"); S.Units.Add(7,E);
 TestTrue(TEXT("selected Engineer gets Build instead of Restore"),R.Card(S,{7}).Decision==TCWorldRecoveryCore::ECard::Engineers);
 FTCUnit H=E; H.Id=8; H.Type=TEXT("hq"); S.Units.Add(8,H);
 TestEqual(TEXT("HQ present restores Engineer"),R.Card(S,{}).Name,FString(TEXT("Restore Engineer")));
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30TransportTest,"ThreeCrossroads.HUD30.TransportAndScuttle",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30TransportTest::RunTest(const FString&)
{
 FTCUnitTable Table; FString Error; if (!TestTrue(TEXT("reference table"),Table.Load(Error))) return false;
 FTCMatchState S; S.ApplyStart(*Hud30::Start()); Hud30::Confirm(S); S.Mode=TEXT("world");
 const auto Snap=Hud30::Parse(TEXT(R"({"units":[[1,"rifle",0,12,12,0,0,200,0,0,0,0,0,0,1],[2,"halftrack",0,14,12,0,0,200,0,0,0,0,524288,0,1],[3,"hq",0,20,20,0,0,3000,0,0,0,0,0,0,1],[4,"rifle",1,40,40,0,0,200,0,0,0,0,0,0,1],[5,"fighter",0,25,25,0,0,200,0,0,0,0,0,0,1],[6,"gunboat",0,28,28,0,0,200,0,0,0,0,0,0,1]]})")); S.ApplySnapshot(*Snap);
 FTCSupportOrders P; P.ApplyStart(*Hud30::Start()); P.ApplySnapshot(*Snap);
 auto Mount=P.PlanMountUp(S,Table,{1,2}); TestEqual(TEXT("Mount up preserves selected IDs for server pairing"),Hud30::Ids(Mount.Message),TArray<int32>({1,2}));
 if (TestTrue(TEXT("board envelope"),Mount.Message.IsValid())) { TestEqual(TEXT("board type"),Mount.Message->GetStringField(TEXT("t")),FString(TEXT("board"))); TestFalse(TEXT("Mount up has no target or queue"),Mount.Message->HasField(TEXT("target")) || Mount.Message->HasField(TEXT("queue"))); }
 FTCCursor Cursor; Cursor.Friend=2; Cursor.bHasGround=true; Cursor.Ground={14,12};
 auto Board=P.PlanRightClick(S,Table,{1},Cursor,true);
 TestEqual(TEXT("own carrier right-click exact envelope"),TCOrders::ToJson(Board.Message),FString(TEXT(R"({"t":"board","ids":[1],"target":2,"queue":true})")));
 auto Unload=P.PlanUnload(S,Table,{1,2,3,999}); TestEqual(TEXT("unload only carriers"),TCOrders::ToJson(Unload.Message),FString(TEXT(R"({"t":"unload","ids":[2]})")));
 const auto Rail=TCOrders::RailUnits(S,Table,{1,2,3,4,5,6,999}); TestEqual(TEXT("only own held ground movers can ride rail"),Rail,TArray<int32>({1,2}));
 TestEqual(TEXT("World destination envelope"),TCOrders::ToJson(TCOrders::RailCommand(Rail,88,96)),FString(TEXT(R"({"t":"rail","ids":[1,2],"x":88,"z":96})")));
 auto Destroy=TCOrders::PlanDestroy({1,3,4,999}); TestEqual(TEXT("scuttle preserves foreign/stale IDs for server validation"),Hud30::Ids(Destroy.Message),TArray<int32>({1,3,4,999}));
 if (Destroy.Message) { TestEqual(TEXT("scuttle request"),Destroy.Message->GetStringField(TEXT("t")),FString(TEXT("destroy"))); TestEqual(TEXT("no local refund fields"),Destroy.Message->Values.Num(),2); }
 TestFalse(TEXT("no selection sends nothing"),TCOrders::PlanDestroy({}).Message.IsValid()); TestEqual(TEXT("planning never removes local units"),S.Units.Num(),6);
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30FieldTest,"ThreeCrossroads.HUD30.FieldworkClicks",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30FieldTest::RunTest(const FString&)
{
 FTCMatchState S; auto Start=Hud30::Start(); S.ApplyStart(*Start); Hud30::Confirm(S);
 auto Snap=Hud30::Parse(TEXT(R"({"mp":1000,"units":[[1,"rifle",0,18,18,0,0,200,0,0,0,0,0,0,1],[2,"rifle",0,32,18,0,0,200,0,0,0,0,0,0,1]],"works":[[7,false,22,22,0,"trench"]]})")); S.ApplySnapshot(*Snap);
 FTCFieldworks F; F.SetUnitTableReference(FTCFieldworks::ReferencePin); F.ApplyStart(*Start); F.ApplySnapshot(*Snap);
 FTCFieldAim Aim; TestTrue(TEXT("arm actual trench-line action"),F.StartEntrench(S,TEXT("line"),TEXT("trench"),{1,2},Aim).bArm);
 auto First=F.Click(S,Aim,true,{22,22},false,{1,2}); TestTrue(TEXT("first click pins without sending"),First.bPinnedCenter && !First.Message);
 auto Preview=F.Preview(S,Aim,true,{38,22},{1,2}); TestTrue(TEXT("line has accepted segments and price summary"),Preview.Pieces.Num()>0 && Preview.Hint.Contains(TEXT("MP")) && Preview.Hint.Contains(TEXT("segment")));
 int32 Price=0; for (const auto& Piece : Preview.Pieces) { Price+=Piece.Cost; TestTrue(TEXT("rejected placements are filtered from preview"),Piece.Place.bOk && Piece.Place.Cells.Num()>0); }
 TestTrue(TEXT("price is the sum of accepted segments"),Preview.Hint.Contains(FString::Printf(TEXT("%d MP"),Price)));
 auto Second=F.Click(S,Aim,true,{38,22},true,{1,2}); TestTrue(TEXT("second click sends queued line and spends pattern aim"),Second.Message.IsValid() && !Aim.IsArmed());
 if (Second.Message) { TestEqual(TEXT("line message"),Second.Message->GetStringField(TEXT("t")),FString(TEXT("entrench"))); TestEqual(TEXT("line takes selected diggers"),Hud30::Ids(Second.Message),TArray<int32>({1,2})); TestTrue(TEXT("Shift queue preserved"),Second.Message->GetBoolField(TEXT("queue"))); TestEqual(TEXT("line end"),Second.Message->GetNumberField(TEXT("x2")),38.); }
 TestTrue(TEXT("arm individual trench"),F.StartDig(S,TEXT("trench"),{1,2},Aim).bArm); F.Click(S,Aim,true,{20,22},false,{1,2});
 auto Dig=F.Click(S,Aim,true,{26,22},true,{1,2}); TestTrue(TEXT("Shift dig continues at a new centre"),Dig.Message.IsValid() && Aim.IsArmed() && !Aim.bHasCenter); TestEqual(TEXT("nearest digger receives single dig"),Hud30::Ids(Dig.Message),TArray<int32>({1}));
 auto Join=F.PlanJoin(S,{1,2},true,{22,22},false); TestTrue(TEXT("right click joins delivered project"),Join.Message.IsValid());
 if (Join.Message) { TestEqual(TEXT("project ID"),Join.Message->GetNumberField(TEXT("join")),7.); TestEqual(TEXT("selected join IDs"),Hud30::Ids(Join.Message),TArray<int32>({1,2})); }
 // Entirely unseen far cells must never enter the accepted segment list.
 bool Throws=false; const auto Far=F.EntrenchPieces(S,TEXT("line"),TEXT("trench"),{110,110},{122,110},{1,2},FTCPositionOf(),Throws);
 TestTrue(TEXT("unseen rejected segments omitted"),Far.IsEmpty() && !Throws);
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30BuildTest,"ThreeCrossroads.HUD30.ConstructionClicks",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30BuildTest::RunTest(const FString&)
{
 FTCUnitTable Table; FString Error; if (!TestTrue(TEXT("reference table"),Table.Load(Error))) return false;
 FTCMatchState S; auto Start=Hud30::Start(); S.ApplyStart(*Start); Hud30::Confirm(S); S.Mode=TEXT("classic");
 auto Snap=Hud30::Parse(TEXT(R"({"mode":{"kind":"classic"},"mp":10000,"mun":1000,"fuel":1000,"units":[[1,"engineer",0,18,18,0,0,100,0,0,0,0,0,0,1],[2,"barracks",0,28,28,0,0,100,0,0,0,0,0,0,0.4],[3,"hq",0,12,12,0,0,3000,0,0,0,0,0,0,1]],"nodes":[[22,22],[34,22]]})")); S.ApplySnapshot(*Snap);
 FTCConstruction B; B.SetUnitTableReference(FTCConstruction::ReferencePin); B.ApplyStart(*Start); B.ApplySnapshot(*Snap);
 const auto Card=B.BuildCard(S,{1}); TestTrue(TEXT("selected Engineer gets actual Build card"),Card.bShown && Card.Buttons.ContainsByPredicate([](const auto& C){return C.Kind==TEXT("depot");}));
 TestTrue(TEXT("depot arms from card action"),B.Arm(S,TEXT("depot"),{1}).bArm);
 const auto SnapNode=B.Placement(S,TEXT("depot"),{23,23}); TestTrue(TEXT("depot snaps onto resource node"),SnapNode.bOk && SnapNode.At.Equals({22,22},.001));
 const auto Unseen=B.Click(S,TEXT("depot"),true,{110,110},true,{1}); TestFalse(TEXT("unseen footprint refuses before send"),Unseen.Message.IsValid());
 for (FVector2D At : {FVector2D(23,23),FVector2D(35,23)}) { const auto Click=B.Click(S,TEXT("depot"),true,At,true,{1}); TestTrue(TEXT("Shift place sends and keeps build aim"),Click.Message.IsValid() && Click.bKeepAim); if (Click.Message) TestTrue(TEXT("build queues behind current order"),Click.Message->GetBoolField(TEXT("queue"))); }
 FTCSupportOrders P; P.ApplyStart(*Hud30::Start()); P.ApplySnapshot(*Snap); FTCCursor Cursor; Cursor.bHasGround=true; Cursor.Building=2; Cursor.Ground={28,28};
 const auto Assist=P.PlanRightClick(S,Table,{1},Cursor,false); TestEqual(TEXT("assist actual damaged site"),TCOrders::ToJson(Assist.Message),FString(TEXT(R"({"t":"assist","ids":[1],"id":2,"queue":false})")));
 const auto Cancel=P.PlanCancelSite(S,Table,{2},true); TestEqual(TEXT("site Cancel actual plan"),TCOrders::ToJson(Cancel.Message),FString(TEXT(R"({"t":"cancel","id":2})"))); TestTrue(TEXT("Classic Cancel clears selection only after send"),Cancel.bClearSelection);
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30TooltipTest,"ThreeCrossroads.HUD30.TooltipInventory",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30TooltipTest::RunTest(const FString&)
{
 Hud30::FWorld W; if (!TestTrue(TEXT("tooltip world"),W.Ready())) return false;
 W.Mode->State.ApplyStart(*Hud30::Start()); Hud30::Confirm(W.Mode->State);
 const auto Layout=FTCHudFit::IconBar(true,false,FTCPaint::ApproxMeasure(),1,true,true,true);
 FTCHud30Access::Set(*W.Hud,W.PC,Layout);
 for (const auto& Hit : Layout.Hits)
 {
  const FString Expected=FTCHudFit::ControlTip(Hit.Control); TestFalse(TEXT("every actual toolbar control has a tooltip"),Expected.IsEmpty());
  TestEqual(TEXT("actual topmost tooltip preserves full control caption"),W.Hud->TooltipAt(Hit.Box.GetCenter()),Expected);
 }
 TestTrue(TEXT("walk actual retained toolbar inventory"),W.Hud->TooltipInventoryErrors().IsEmpty());
 const auto& First=Layout.Hits[0]; FTCHud30Access::Cover(*W.Hud,First.Box);
 TestTrue(TEXT("covered hit cannot show underlying caption"),W.Hud->TooltipAt(First.Box.GetCenter()).IsEmpty());
 FTCHud30Access::Set(*W.Hud,W.PC,Layout); W.PC->bControlsSheetOpen=true;
 TestTrue(TEXT("modal sheet hides tooltips"),W.Hud->TooltipAt(First.Box.GetCenter()).IsEmpty()); W.PC->bControlsSheetOpen=false;
 FTCHud30Access::Hide(*W.Hud); TestTrue(TEXT("hidden HUD has no tooltip"),W.Hud->TooltipAt(First.Box.GetCenter()).IsEmpty());
 FTCHudFit::FControl C; C.Label=TEXT("Recon Flight"); C.Hover=TEXT("Not enough manpower"); C.bEnabled=false;
 FTCHudFit::FBottomLayout Unavailable; Unavailable.Hits.Add({First.Box,C}); FTCHud30Access::Set(*W.Hud,W.PC,Unavailable);
 { FTCLocale::FScopedLanguage Lang(ETCLanguage::Spanish); TestEqual(TEXT("availability reason translates in current state"),TC_T(W.Hud->TooltipAt(First.Box.GetCenter())),TC_T(FString(TEXT("Not enough manpower")))); TestTrue(TEXT("Spanish actually changes the keyed reason"),TC_T(W.Hud->TooltipAt(First.Box.GetCenter()))!=C.Hover); }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30MinimapTest,"ThreeCrossroads.HUD30.CompositeMinimap",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30MinimapTest::RunTest(const FString&)
{
 Hud30::FWorld W; if (!TestTrue(TEXT("overlay world"),W.Ready())) return false;
 FString Error; if (!TestTrue(TEXT("reference table"),W.Mode->UnitTable.Load(Error))) return false;
 auto Start=Hud30::Start();
 Start->GetObjectField(TEXT("map"))->SetArrayField(TEXT("points"),{MakeShared<FJsonValueObject>(Hud30::Parse(TEXT(R"({"x":10,"y":10})"))),MakeShared<FJsonValueObject>(Hud30::Parse(TEXT(R"({"x":20,"y":10,"needs":0})")))});
 auto& S=W.Mode->State; S.ApplyStart(*Start); Hud30::Confirm(S);
 auto Snap=Hud30::Parse(TEXT(R"({"tick":1,"mode":{"kind":"world"},"mp":200,"units":[[1,"truck",0,20,20,0,0,100,0,0,0,0,0,0,1]],"points":[[0,1,0,0,0,0],[0,1,0,0,0,1]],"strikes":[["strafe",50,50,0,4,1]],"world":{"regions":[{"id":0,"name":"Known station","team":0,"x":20,"z":20,"bounds":[10,10,30,30]}],"stations":[{"region":0,"x":20,"z":20}]},"logistics":{"enabled":true,"trucks":[{"id":1,"route":[{"x":30,"z":20}],"cargo":{}}],"stores":[{"id":"known","owner":0,"x":24,"z":20},{"id":"unknown","owner":0,"x":110,"z":110}]}})"));
 S.ApplySnapshot(*Snap); S.Map.bWorld=true; S.bFogLifted=false; S.Map.Known.Init(true,64*64); S.FogVisible.Init(0,64*64); S.FogExplored.Init(1,64*64);
 for (int32 Z=48;Z<64;Z++) for (int32 X=48;X<64;X++) { const int32 I=Z*64+X; S.Map.Known[I]=false; S.FogExplored[I]=0; }
 W.Mode->WorldRec.ApplyStart(*Start); W.Mode->WorldRec.ApplySnapshot(*Snap,S); W.Mode->Powers.ApplyStart(*Start); W.Mode->Powers.ApplySnapshot(*Snap);
 W.Mode->Logistics.ApplyStart(); W.Mode->Logistics.ApplySnapshot(*Snap,S); W.Mode->Logistics.ToggleOverlay();
 FTCAlertPush Ping; TestTrue(TEXT("delivered team ping"),W.Mode->AlertFeed.ReceivePing(*Hud30::Parse(TEXT(R"({"t":"ping","from":0,"x":26,"z":22})")),S,99.75,Ping));
 W.PC->Selected={1}; FTCHud30Access::Set(*W.Hud,W.PC,{}); auto& Map=FTCHud30Access::Map(*W.Hud); Map.Refresh(S,W.Mode->UnitTable); TestTrue(TEXT("actual minimap arranged"),Map.Arrange(1280,720,S,0));
 TMap<FString,int32> Counts; Map.ObserveOverlay=[&](const FString& Role,const TArray<FVector2D>& Polygon,FLinearColor)
 {
  Counts.FindOrAdd(Role)++; for (auto P : Polygon) TestTrue(TEXT("actual composite primitive clipped to minimap"),Map.Rect().IsInsideOrOn(P));
 };
 FTCHud30Access::DrawOverlays(*W.Hud);
 for (const TCHAR* Role : {TEXT("station"),TEXT("strike"),TEXT("store"),TEXT("route"),TEXT("ping")}) TestTrue(Role,Counts.FindRef(Role)>0);
 // Delivered points can coexist with World overlays. Force the locked state via the public snapshot rows.
 TestTrue(TEXT("needs-link primitive is part of composite"),Counts.FindRef(TEXT("needs-link"))>0);
 TestEqual(TEXT("only known store submits a square"),Counts.FindRef(TEXT("store")),1);
 S.FogExplored[10*64+12]=0; // route 20,20 -> 30,20 crosses this cell
 Counts.Reset(); FTCHud30Access::DrawOverlays(*W.Hud); TestEqual(TEXT("route with unknown intermediate segment submits nothing"),Counts.FindRef(TEXT("route")),0);
 const int32 Unknown=55*64+55; TestTrue(TEXT("rail cannot reveal unknown World terrain"),FTCMinimap::CellColour(S,Unknown,false,true)==FColor(10,10,10,255));
 TestTrue(TEXT("known rail is represented in terrain image"),FTCMinimap::CellColour(S,10*64+10,false,true)!=FTCMinimap::CellColour(S,10*64+10,false,false));
 Map.ObserveOverlay=nullptr;
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud30PortraitTest,"ThreeCrossroads.HUD30.PortraitFamilies",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud30PortraitTest::RunTest(const FString&)
{
 const auto Gallery=FTCHudFit::PortraitGallery(FTCPaint::ApproxMeasure());
 TestEqual(TEXT("five model families across three factions"),Gallery.Icons.Num(),15);
 for (const auto& I : Gallery.Icons) { TestTrue(TEXT("uses the actual live model portrait path"),I.bPortrait); TestTrue(TEXT("explicit faction survives independent of current match seats"),I.Faction>=0 && I.Faction<3); TestTrue(TEXT("portrait legible at card size"),I.Box.GetSize().X>=100 && I.Box.GetSize().Y>=75); }
 return !HasAnyErrors();
}
#endif
