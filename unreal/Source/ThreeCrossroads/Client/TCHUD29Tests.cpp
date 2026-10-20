// Focused HUD regressions. The integration lead executes them in Unreal.
#include "Client/TCHUD.h"
#include "Client/TCObjectives.h"
#include "Client/TCLocale.h"
#include "Client/TCLogistics.h"
#include "Client/TCUnitTable.h"
#include "Client/TCAlerts.h"
#include "Client/TCGameMode.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Engine/Engine.h"
#include "Engine/GameInstance.h"
#include "Engine/World.h"
#include "GameFramework/WorldSettings.h"

struct FTCHud29Access
{
 static void Observe(ATCHUD& Hud,ATCPlayerController* PC,const FBox2D& Box)
 {
  Hud.PlayerOwner=PC; Hud.Buttons.Reset();
  FTCUiCommand Command(ETCUiAction::MatchControl); Command.Unit=TEXT("hudpanel:plane:1");
  Hud.Buttons.Add({Box,Command,TEXT("Fighter"),true,TEXT("Fighter: Ready")});
 }
 static void Cover(ATCHUD& Hud,const FBox2D& Box)
 { Hud.Buttons.Add({Box,FTCUiCommand(),TEXT("covered"),false,TEXT("covered")}); }
};

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud29DeliveredAirTest,"ThreeCrossroads.HUD29.DeliveredPlanePresses",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud29DeliveredAirTest::RunTest(const FString&)
{
 UWorld* World=UWorld::CreateWorld(EWorldType::Game,false);
 if (!TestNotNull(TEXT("isolated input world"),World)) return false;
 if (!TestNotNull(TEXT("engine"),GEngine)) { World->DestroyWorld(false); return false; }
 UGameInstance* Instance=NewObject<UGameInstance>(World); World->SetGameInstance(Instance);
 FWorldContext& Context=GEngine->CreateNewWorldContext(EWorldType::Game); Context.SetCurrentWorld(World); Context.OwningGameInstance=Instance;
 auto Cleanup=[&]() { World->DestroyWorld(false); GEngine->DestroyWorldContext(World); };
 World->GetWorldSettings()->DefaultGameMode=ATCGameMode::StaticClass();
 if (!TestTrue(TEXT("authority without socket startup"),World->SetGameMode(FURL()))) { Cleanup(); return false; }
 ATCGameMode* Mode=World->GetAuthGameMode<ATCGameMode>();
 ATCPlayerController* PC=World->SpawnActor<ATCPlayerController>(); ATCHUD* Hud=World->SpawnActor<ATCHUD>();
 if (!TestNotNull(TEXT("controller"),PC) || !TestNotNull(TEXT("HUD"),Hud)) { Cleanup(); return false; }
 FTCMatchState& State=Mode->State; State.bHasMatch=true; State.You=State.LobbySeat=0; State.bLobbySinceOpen=true;
 for (int32 Id=1;Id<=2;Id++) { FTCUnit U; U.Id=Id; U.Type=TEXT("fighter"); U.Owner=0; State.Units.Add(Id,U); FTCAirRow Row; Row.Id=Id; State.Air.Add(Row); }
 const FBox2D Box(FVector2D(1000,120),FVector2D(1200,150)); FTCHud29Access::Observe(*Hud,PC,Box);
 Hud->SetCanvas(nullptr,nullptr);
 TestTrue(TEXT("first delivered press is consumed between draws"),Hud->PlanePressAt(Box.GetCenter(),false,false,0));
 TestEqual(TEXT("first press selects only that aircraft"),PC->Selected.Num(),1);
 TestTrue(TEXT("second press at 0.32 s is consumed with no intervening draw or controller poll"),Hud->PlanePressAt(Box.GetCenter(),false,false,.32));
 TestEqual(TEXT("both own aircraft are selected by the real HUD command dispatcher"),PC->Selected.Num(),2);
 Hud->PlanePressAt(Box.GetCenter(),false,false,1); Hud->PlanePressAt(Box.GetCenter(),false,false,1.551);
 TestEqual(TEXT("the rejected historical 0.551 s interval stays a single selection"),PC->Selected.Num(),1);
 Hud->PlanePressAt(Box.GetCenter(),false,false,2);
 TestFalse(TEXT("a world press is left to the controller"),Hud->PlanePressAt(FVector2D(20,20),false,false,2.1));
 Hud->PlanePressAt(Box.GetCenter(),false,false,2.2);
 TestEqual(TEXT("an intervening non-plane press breaks the double click"),PC->Selected.Num(),1);
 PC->bControlsSheetOpen=true;
 TestFalse(TEXT("modal Controls still owns input"),Hud->PlanePressAt(Box.GetCenter(),false,false,2)); PC->bControlsSheetOpen=false;
 FTCHud29Access::Cover(*Hud,Box);
 TestFalse(TEXT("a topmost inert overlay blocks the plane beneath"),Hud->PlanePressAt(Box.GetCenter(),false,false,3));
 Cleanup();
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud29StackTest,"ThreeCrossroads.HUD29.PanelStack",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud29StackTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)}) for (float Scale : {1.f,1.5f})
 {
  FTCLocale::FScopedLanguage Lang(Language);
  FProductionInfo Info; Info.Name=TEXT("Platoon HQ"); Info.Status=TEXT("Training Rifle Squad, 12 s left"); Info.Tier=TEXT("Tier 1");
  FControl Tech; Tech.Label=TEXT("Upgrade to Tier 2: 300 MP, 60s"); Info.Research.Add(Tech);
  FControl Rifle; Rifle.Label=TEXT("Rifle Squad"); Rifle.Icon=TEXT("rifle"); Rifle.Value=TEXT("100 MP"); Rifle.Command=FTCUiCommand(ETCUiAction::Recruit);
  const FBottomLayout L=ProductionPanel(Pixels/Scale,Pixels.X/Scale-200,Info,{TEXT("Infantry")},{{Rifle}},
   {TEXT("Logistics"),TEXT("Platoon HQ #2"),TEXT("Ammunition stock: 120.0"),TEXT("Provisions stock: 120.0 s")},FTCPaint::ApproxMeasure(),Scale);
  TestTrue(TEXT("Logistics remains available"),L.Logistics.bIsValid);
  for (const FBox2D& Panel : L.Paint.Panels) if (!Panel.Equals(L.Logistics,.01))
   TestFalse(TEXT("Logistics never overlaps production or research"),Panel.Intersect(L.Logistics));
  for (const FTextFit& Text : L.TextBounds) if (!L.Logistics.IsInsideOrOn(Text.Box.GetCenter()))
   TestFalse(TEXT("foreground HQ/research text cannot bleed into Logistics"),Text.Box.Intersect(L.Logistics));
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud29NotesTest,"ThreeCrossroads.HUD29.OrderDiagnostics",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud29NotesTest::RunTest(const FString&)
{
 TestFalse(TEXT("successful socket envelopes are hidden in normal play"),FTCHudFit::ShowOrderNote(true,false));
 TestTrue(TEXT("explicit diagnostics retain successful envelopes"),FTCHudFit::ShowOrderNote(true,true));
 TestTrue(TEXT("normal play still explains a refused action"),FTCHudFit::ShowOrderNote(false,false));
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud29SelectionTest,"ThreeCrossroads.HUD29.SelectionTraining",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud29SelectionTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 FTCUnitTable Table; FString Error; if (!TestTrue(TEXT("original unit table"),Table.Load(Error))) return false;
 auto Parse=[](const TCHAR* Text) { TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text),O); return O; };
 FTCMatchState State; State.bHasMatch=true; State.You=State.LobbySeat=0; State.bLobbySinceOpen=true; State.Factions={0,1};
 FTCRecruitment Recruit; Recruit.SetUnitTableReference(FTCRecruitment::ReferencePin); Recruit.ApplyStart(*Parse(TEXT(R"({"factions":[0,1],"map":{}})")));
 Recruit.ApplySnapshot(*Parse(TEXT(R"({"mp":200,"mode":{"kind":"classic"},"units":[[1],[2]],"queues":[[1,0.3,-1,-1,"rifle","rifle"],[2,0.5,-1,-1,"engineer"]]})")));
 FTCLogistics Logistics;
 for (int32 Id=1;Id<=4;Id++) { FTCUnit U; U.Id=Id; U.Owner=0; U.Type=Id<=2?TEXT("hq"):Id==3?TEXT("tank"):TEXT("rifle"); U.Hp=100; State.Units.Add(Id,U); }
 const TArray<FSelection> Single=SelectionRows(State,Table,Recruit,Logistics,{1});
 if (!TestEqual(TEXT("single HQ row"),Single.Num(),1)) return false;
 TestEqual(TEXT("single producer shows its actual queue and progress"),Single[0].Training,FString::Printf(TEXT("Training %s 30%% (2/5)"),*FTCAlerts::UnitName(State,&Table,TEXT("rifle"),0)));
 const TArray<FSelection> Many=SelectionRows(State,Table,Recruit,Logistics,{1,2,3,4});
 TestEqual(TEXT("grouped rows"),Many.Num(),3);
 TestEqual(TEXT("source rank precedes creation id"),Many[0].Type,FString(TEXT("rifle")));
 const FSelection* Hq=Many.FindByPredicate([](const FSelection& R) { return R.Type==TEXT("hq"); });
 if (TestNotNull(TEXT("grouped producers"),Hq))
 {
  TestEqual(TEXT("all training producers are counted"),Hq->Training,FString(TEXT("2 buildings training")));
  TestEqual(TEXT("health is summed by type"),Hq->Hp,200);
 }
 const FBottomLayout Drawn=LeftPanel(720,TEXT("1 unit selected"),Single,{},{},{},FTCPaint::ApproxMeasure());
 TestTrue(TEXT("training summary reaches the real selection painter"),Drawn.Paint.TextsOf(TEXT("bottom:selection-training")).Num()>0);
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud29EconomyTest,"ThreeCrossroads.HUD29.EconomyCap",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud29EconomyTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 for (bool AtCap : {false,true})
 {
  FControl Mun; Mun.Label=TEXT("Mun"); Mun.Value=TEXT("25"); Mun.Icon=TEXT("resource-mun");
  FControl Fuel; Fuel.Label=TEXT("Fuel"); Fuel.Value=TEXT("40"); Fuel.Icon=TEXT("resource-fuel");
  const FBottomLayout L=EconomyPanel(1280,TEXT("200 MP"),TEXT("+4/s (upkeep −1)"),TEXT("12.75/12 units"),{},FTCPaint::ApproxMeasure(),1,{Mun,Fuel},{},0,0,AtCap);
  const FTCPaintItem* Population=L.Paint.Find(TEXT("bottom:population"));
  if (TestNotNull(TEXT("population remains visible with fractional army use"),Population))
  {
   TestTrue(TEXT("cap is a warning only when actually reached"),Population->Colour.Equals(AtCap?TCCss::RedHi:TCCss::Dim));
   TestEqual(TEXT("fractional population survives"),Population->Text,FString(TEXT("12.75/12 units")));
  }
  TestEqual(TEXT("both construction currencies are shown"),L.Paint.TextsOf(TEXT("bottom:resource")).Num(),2);
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud29ThreatTest,"ThreeCrossroads.HUD29.MinimapThreatPrivacy",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud29ThreatTest::RunTest(const FString&)
{
 FTCUnitTable Table; FString Error; if (!TestTrue(TEXT("original unit table"),Table.Load(Error))) return false;
 FTCMatchState State; State.bHasMatch=true; State.You=State.LobbySeat=0; State.bLobbySinceOpen=true; State.Teams={0,1};
 State.Map.W=State.Map.H=4; State.Map.bWorld=true; State.Map.Known.Init(true,16); State.FogVisible.Init(0,16); State.FogExplored.Init(1,16);
 for (int32 Id=1;Id<=3;Id++) { FTCUnit U; U.Id=Id; U.Owner=Id==3?0:1; U.Type=TEXT("rifle"); U.X=Id==2?5:3; U.Z=3; State.Units.Add(Id,U); }
 State.FogVisible[5]=1;
 const auto Threats=FTCHudFit::MinimapThreats(State,Table);
 TestTrue(TEXT("only the currently seen armed enemy gets a range, excluding explored enemy and own unit"),Threats.Num()==1 && Threats[0].At==FVector2D(3,3) && Threats[0].Radius>0);
 State.Map.Known[5]=false;
 TestTrue(TEXT("unknown World cells cannot expose a stale range"),FTCHudFit::MinimapThreats(State,Table).IsEmpty());
 State.Map.Known[5]=true; State.FogVisible[5]=0;
 TestTrue(TEXT("visibility loss removes the range immediately"),FTCHudFit::MinimapThreats(State,Table).IsEmpty());
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud29AirTest,"ThreeCrossroads.HUD29.AirInputAndScroll",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud29AirTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 FTCMatchState State; State.bHasMatch=true; State.You=State.LobbySeat=0; State.bLobbySinceOpen=true;
 for (int32 Id=1;Id<=3;Id++) { FTCUnit U; U.Id=Id; U.Owner=Id==3?1:0; U.Type=TEXT("fighter"); State.Units.Add(Id,U); FTCAirRow Row; Row.Id=Id; State.Air.Add(Row); }
 for (double Interval : {.2,.319,.32,.321,.551})
 {
  FPlaneInteraction Clicks;
  TSet<int32> Selection=Clicks.Press(State,{},1,false,0);
  Selection=Clicks.Press(State,Selection,1,false,Interval);
  TestEqual(TEXT("two consumed presses keep the original 0.32 s threshold"),Selection.Num(),Interval<=.32?2:1);
 }
 for (float Scale : {1.f,1.5f})
 {
  TArray<FControl> Air;
  for (int32 Id=1;Id<=32;Id++) { FControl C; C.Label=TEXT("Fighter"); C.Value=TEXT("Ready"); C.Command=FTCUiCommand(ETCUiAction::MatchControl); C.Command.Unit=FString::Printf(TEXT("hudpanel:plane:%d"),Id); Air.Add(C); }
  const float Bottom=520/Scale;
  const FBottomLayout First=EconomyPanel(1280/Scale,TEXT("120 MP"),TEXT("+4/s"),TEXT("1.5/20 units"),{},FTCPaint::ApproxMeasure(),Scale,{},Air,Bottom);
  TestTrue(TEXT("long air lists scroll without covering the minimap"),First.AirMax>0 && First.Economy.Max.Y<=Bottom+.01);
  const FBottomLayout Last=EconomyPanel(1280/Scale,TEXT("120 MP"),TEXT("+4/s"),TEXT("1.5/20 units"),{},FTCPaint::ApproxMeasure(),Scale,{},Air,Bottom,First.AirMax);
  TestTrue(TEXT("the last own aircraft remains selectable"),Last.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Unit==TEXT("hudpanel:plane:32"); }));
 }
 return !HasAnyErrors();
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud29MinimapClipTest,"ThreeCrossroads.HUD29.MinimapOverlayClip",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud29MinimapClipTest::RunTest(const FString&)
{
 const FBox2D Clip(FVector2D(10,20),FVector2D(110,120));
 const TArray<FVector2D> Crossing={{0,0},{70,0},{70,70},{0,70}};
 const auto Visible=FTCMinimap::ClipPolygon(Crossing,Clip);
 TestEqual(TEXT("an edge-crossing overlay retains its visible rectangle"),Visible.Num(),4);
 for (const FVector2D& P : Visible) TestTrue(TEXT("every submitted overlay vertex stays in the minimap"),Clip.IsInsideOrOn(P));
 const auto Hidden=FTCMinimap::ClipPolygon({FVector2D(-10,-10),FVector2D(-5,-10),FVector2D(-5,-5)},Clip);
 TestTrue(TEXT("an overlay wholly outside the minimap submits no pixels"),Hidden.IsEmpty());
 const TArray<FVector2D> Surrounding={{0,0},{140,0},{140,140},{0,140}};
 const auto Whole=FTCMinimap::ClipPolygon(Surrounding,Clip);
 TestEqual(TEXT("a surrounding overlay clips to the minimap, without a spurious exterior strip"),Whole.Num(),4);
 return !HasAnyErrors();
}
#endif
