// Round 27 regressions against pinned hud.js, index.html and alerts.css.
// The lead runs the unchanged tests against baseline production, then the minimal fixes.
#include "Client/TCHUD.h"
#include "Client/TCAlerts.h"
#include "Client/TCFormationControls.h"
#include "Client/TCLocale.h"
#include "Client/TCStats.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

namespace TCHud27Test
{
 using namespace FTCHudFit;
 FTCMeasure Measure() { return FTCType::IsLoaded()?FTCType::Measure():FTCPaint::ApproxMeasure(); }
 FControl Control(ETCUiAction Action,const FString& Unit=FString())
 {
  FControl C; C.Command=FTCUiCommand(Action); C.Command.Unit=Unit;
  C.Label=C.Hover=TEXT("Original control"); C.Icon=TEXT("stop"); return C;
 }
 TArray<FControl> Root()
 {
  TArray<FControl> Out={Control(ETCUiAction::Retreat),Control(ETCUiAction::AttackMove),Control(ETCUiAction::Stop),
   Control(ETCUiAction::Stance,TEXT("holdFire")),Control(ETCUiAction::Stance,TEXT("holdPos")),Control(ETCUiAction::Stance,TEXT("autoRetreat")),Control(ETCUiAction::Cover)};
  for (ETCUiAction A : {ETCUiAction::Formation,ETCUiAction::BuildMenu,ETCUiAction::TrenchMenu})
  { FControl C=Control(A,A==ETCUiAction::Formation?TEXT("menu"):TEXT("")); C.bSubmenu=true; Out.Add(C); }
  return Out;
 }
 const FHit* Find(const FBottomLayout& L,const FControl& C)
 { return L.Hits.FindByPredicate([&](const FHit& H) { return H.Control.Command.Action==C.Command.Action && H.Control.Command.Unit==C.Command.Unit; }); }
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud27RootTest,"ThreeCrossroads.HUD27.RootOrders",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud27RootTest::RunTest(const FString&)
{
 using namespace TCHud27Test;
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)}) for (float Scale : {1.f,1.5f})
 for (ETCUiAction MenuKind : {ETCUiAction::Formation,ETCUiAction::Build,ETCUiAction::Entrench})
 {
  FTCLocale::FScopedLanguage Lang(Language); const FVector2D View=Pixels/Scale;
  TArray<FControl> Orders=Root(),Menu;
  Orders[3].bEnabled=false; Orders[3].Hover=TEXT("You are spectating");
  for (int32 I=0;I<8;I++) Menu.Add(Control(MenuKind,FString::FromInt(I)));
  const FBottomLayout L=LeftPanel(View.Y,TEXT("1 unit selected"),{{TEXT("rifle"),TEXT("Rifle Squad"),1,100,100,{}}},{},Orders,Menu,Measure(),Scale,0,1,true,false,0,80,0,{},LeftWidth(View,Scale));
  for (const FControl& C : Orders)
  {
   const FHit* Hit=Find(L,C);
   if (!TestNotNull(TEXT("opening a submenu retains the actual root hit"),Hit)) continue;
   TestEqual(TEXT("source root grid retains its 50 CSS pixel row"),Hit->Box.GetSize().Y,50.0);
   TestTrue(TEXT("root target is visible in this bounded eight-card fixture"),Hit->Box.Min.Y>=0 && Hit->Box.Max.Y<=View.Y);
   TestTrue(TEXT("root hit uses half-open edges"),Contains(Hit->Box,Hit->Box.GetCenter()) && !Contains(Hit->Box,FVector2D(Hit->Box.Max.X,Hit->Box.GetCenter().Y)));
   TestEqual(TEXT("layout retains disabled state separately from target existence"),Hit->Control.bEnabled,C.bEnabled);
   TestEqual(TEXT("disabled control retains the source explanation"),ControlTip(Hit->Control),C.Hover);
  }
  TestEqual(TEXT("source grids have no native Next/Previous pagination"),L.MenuMax,0);
  TestFalse(TEXT("root grid never acquires menu pager targets"),L.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Unit.StartsWith(TEXT("hudpanel:scroll:menu:")); }));
  TestEqual(TEXT("submenu has its original heading"),L.Paint.TextsOf(TEXT("bottom:submenu-title")).Num(),1);
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud27TabsTest,"ThreeCrossroads.HUD27.BuildTabs",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud27TabsTest::RunTest(const FString&)
{
 using namespace TCHud27Test;
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)}) for (float Scale : {1.f,1.5f})
 {
  FTCLocale::FScopedLanguage Lang(Language); const FVector2D View=Pixels/Scale;
  TArray<FControl> Tabs;
  for (const TCHAR* Label : {TEXT("Base"),TEXT("Defenses"),TEXT("Field works"),TEXT("Engineering")})
  { FControl C=Control(ETCUiAction::MatchControl,FString::Printf(TEXT("hudpanel:buildtab:%d"),Tabs.Num())); C.Label=C.Hover=Label; Tabs.Add(C); }
  const FBottomLayout L=LeftPanel(View.Y,TEXT(""),{},{},Root(),{Control(ETCUiAction::Build,TEXT("hq"))},Measure(),Scale,0,1,true,false,0,80,0,Tabs,LeftWidth(View,Scale));
  for (const FControl& C : Tabs)
  {
   const FHit* H=Find(L,C); if (!TestNotNull(TEXT("actual source build tab hit"),H)) continue;
   TestEqual(TEXT("tab keeps the original complete label"),H->Control.Label,C.Label);
   TestEqual(TEXT("tab hover retains its complete label independently"),ControlTip(H->Control),C.Hover);
   TestTrue(TEXT("full label is painted, without an invented ellipsis"),L.Paint.Items.ContainsByPredicate([&](const FTCPaintItem& I) { return I.Kind==ETCPaintKind::Text && I.Text==TC_T(C.Label) && I.FontPx==12 && H->Box.IsInsideOrOn(I.Pos); }));
   TestTrue(TEXT("source flex tab minimum includes label and 2px horizontal padding"),H->Box.GetSize().X+0.02>=Measure()(TC_T(C.Label),12,false,0)+4);
  }
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud27TruckTest,"ThreeCrossroads.HUD27.TruckGrid",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud27TruckTest::RunTest(const FString&)
{
 using namespace TCHud27Test;
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)}) for (float Scale : {1.f,1.5f})
 {
  FTCLocale::FScopedLanguage Lang(Language); const FVector2D View=Pixels/Scale;
  FControl Stop=Control(ETCUiAction::Stop),Resume=Control(ETCUiAction::Resume); Resume.Value=TEXT("Resume deliveries");
  const FBottomLayout L=LeftPanel(View.Y,TEXT("1 unit selected"),{{TEXT("truck"),TEXT("Supply Truck"),1,120,120,{}}},{},{Stop,Resume},{},Measure(),Scale,0,1,true,false,0,80,0,{},LeftWidth(View,Scale));
  const FHit* S=Find(L,Stop),*R=Find(L,Resume);
  if (!TestNotNull(TEXT("truck Stop hit"),S) || !TestNotNull(TEXT("truck Resume hit"),R)) continue;
  TestEqual(TEXT("source truck grid has two order targets"),L.Hits.FilterByPredicate([](const FHit& H) { return H.Control.Command.Action!=ETCUiAction::MatchControl; }).Num(),2);
  TestTrue(TEXT("source auto minimum widens Resume relative to the empty grid tracks"),R->Box.GetSize().X>S->Box.GetSize().X);
  TestTrue(TEXT("Resume fits its actual unwrapped translated caption"),R->Box.GetSize().X>=Measure()(TC_T(Resume.Value),13,false,0)+2);
  const FTCPaintItem* Value=L.Paint.Find(TEXT("bottom:value"));
  TestTrue(TEXT("painted Resume caption stays within its own hit, clear of Stop"),Value && Value->Pos.X-Measure()(Value->Text,13,false,0)/2>=R->Box.Min.X);
  TestEqual(TEXT("Stop keeps its source tooltip"),ControlTip(S->Control),FString(TEXT("Stop selected units")));
  TestEqual(TEXT("Resume keeps its source tooltip"),ControlTip(R->Control),Resume.Value);
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud27HistoryTest,"ThreeCrossroads.HUD27.HistoryOverlap",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud27HistoryTest::RunTest(const FString&)
{
 using namespace TCHud27Test;
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)}) for (float Scale : {1.f,1.5f})
 {
  FTCLocale::FScopedLanguage Lang(Language); const FVector2D View=Pixels/Scale;
  FTCAlerts A;
  for (int32 I=0;I<8;I++) A.Push(TEXT("unitLost"),TEXT("Flamethrower Squad lost"),true,FVector2D(I*40,20),0);
  FTCAlertViewInput In; In.Now=2.5; In.ViewW=View.X; In.ViewH=View.Y; In.Right=View.X-8; In.MinimapTop=View.Y-192; In.UiScale=Scale;
  const FTCPaint Closed=A.Layout(In,Measure()); A.bHistoryOpen=true; const FTCPaint Open=A.Layout(In,Measure());
  TestTrue(TEXT("history expands despite the crowded loss stack"),Open.TextsOf(TEXT("alert:entry-text")).Num()>0);
  TestEqual(TEXT("both source history navigation buttons remain"),Open.TextsOf(TEXT("alert:nav-label")).Num(),2);
  TestTrue(TEXT("actual expanded entry hit exists independently of its text"),Open.Hits.ContainsByPredicate([](const FTCPaintHit& H) { return H.Tag.StartsWith(TEXT("alert:entry:")); }));
  const FTCPaintItem* C=Closed.Find(TEXT("alert:history")),*O=Open.Find(TEXT("alert:history"));
  if (!TestNotNull(TEXT("closed history box"),C) || !TestNotNull(TEXT("open history box"),O)) continue;
  TestTrue(TEXT("history grows upward, retaining the minimap bottom anchor"),O->Pos.Y<C->Pos.Y && FMath::IsNearlyEqual(O->Pos.Y+O->Size.Y,C->Pos.Y+C->Size.Y,.02));
  const float Border=FMath::FloorToFloat(Scale)/Scale;
  const float Line=FMath::FloorToFloat(14.f*1.35f*Scale*64)/(Scale*64);
  const float ExpectedH=(Border+8+Line)+(8+Line+10+2*Border)+FMath::Min(250.f,float(Pixels.Y)*.3f)+Border;
  TestTrue(TEXT("30vh measures the physical viewport and includes the list padding"),FMath::IsNearlyEqual(O->Size.Y,double(ExpectedH),.05));
  TestEqual(TEXT("history retains all eight identities"),A.History().Entries().Num(),8);
 }
 return !HasAnyErrors();
}
#endif
