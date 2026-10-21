// Focused round-26 presentation regressions. The integration lead runs these in the actual engine.
#include "Client/TCHUD.h"
#include "Client/TCHUD26Feedback.h"
#include "Client/TCStats.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud26LetterTest, "ThreeCrossroads.HUD.LetterFeedback", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud26LetterTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 TCHud26::FLetterHit Hit;
 Hit.Press(1000);
 TestTrue(TEXT("letter pulse starts even when buying will be refused"),Hit.Active(1000));
 TestTrue(TEXT("pulse remains at 139ms"),Hit.Active(1139));
 TestFalse(TEXT("the original timeout removes it at 140ms"),Hit.Active(1140));
 Hit.Press(2000); Hit.Press(2100);
 TestFalse(TEXT("the first callback removes a repeated hit at its own deadline"),Hit.Active(2140));
 Hit.Press(2150);
 TestTrue(TEXT("a subsequent press can light the same card again"),Hit.Active(2150));
 TestFalse(TEXT("an older pending callback still removes the new hit"),Hit.Active(2240));

 FControl Rifle; Rifle.Label=TEXT("Rifle Squad"); Rifle.Icon=TEXT("rifle"); Rifle.Key=TEXT("Q"); Rifle.Value=TEXT("100 MP");
 Rifle.Command=FTCUiCommand(ETCUiAction::Recruit); Rifle.Command.Unit=TEXT("rifle"); Rifle.bLetterHit=true;
 Rifle.bEnabled=false;
 for (float Scale : {1.f,1.5f})
 {
  FBottomLayout L=RecruitPanel(1280/Scale,720/Scale,1080/Scale,{}, {},{{Rifle}}, {},FTCPaint::ApproxMeasure(),Scale);
  const FTCPaintItem* Card=L.Paint.Find(TEXT("bottom:card"));
  TestTrue(TEXT("a refused letter card still paints the source cell-lo pulse"),Card && Card->Colour.Equals(TCCss::Hex(0x15181a)));
  const FHit* Target=L.Hits.FindByPredicate([](const FHit& H) { return H.Control.Command.Action==ETCUiAction::Recruit; });
  if (!TestNotNull(TEXT("actual drawn card hit"),Target)) continue;
  ApplyPointer(L,Target->Box.GetCenter(),true);
  TestTrue(TEXT("hover/held cannot override the source hit selector"),L.Paint.Find(TEXT("bottom:card"))->Colour.Equals(TCCss::Hex(0x15181a)));
  const FTCPaintItem* Border=L.Paint.Items.FindByPredicate([&](const FTCPaintItem& I) { return I.Kind==ETCPaintKind::Frame && I.Pos.Equals(Target->Box.Min) && I.Size.Equals(Target->Box.GetSize()); });
  TestTrue(TEXT("pulse has the original opaque brass border"),Border && Border->Colour.Equals(TCCss::Brass));
 }
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud26OrdersTest, "ThreeCrossroads.HUD.SelectionOrders", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud26OrdersTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 TArray<FControl> Candidates;
 for (ETCUiAction Action : {ETCUiAction::Stop,ETCUiAction::AttackMove,ETCUiAction::Resume,ETCUiAction::Rail})
 { FControl C; C.Label=TEXT("candidate"); C.Command=FTCUiCommand(Action); Candidates.Add(C); }
 for (float Scale : {1.f,1.5f})
 {
  for (const FString& Type : {FString(TEXT("hq")),FString(TEXT("barracks")),FString(TEXT("truck"))})
  {
   FSelection S; S.Type=Type; S.Name=Type; S.Hp=S.MaxHp=100;
   const FBottomLayout L=LeftPanel(1080/Scale,TEXT("1 unit selected"),{S},{},Candidates,{},FTCPaint::ApproxMeasure(),Scale,0,1,true);
   if (Type==TEXT("truck"))
   {
    TestEqual(TEXT("original truck Orders has exactly Stop and Resume"),L.Hits.FilterByPredicate([](const FHit& H) { return H.Control.Command.Action!=ETCUiAction::MatchControl; }).Num(),2);
    TestTrue(TEXT("source Stop and Resume targets remain"),L.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Action==ETCUiAction::Stop; }) && L.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Action==ETCUiAction::Resume; }));
   }
   else TestFalse(TEXT("an all-building selection has no Orders panel"),L.Orders.bIsValid);
   TestFalse(TEXT("neither building nor truck selection has group-transfer controls"),L.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Unit.StartsWith(TEXT("hudpanel:destination")) || H.Control.Command.Unit==TEXT("hudpanel:transfer"); }));
  }
  FSelection Troop; Troop.Type=TEXT("rifle"); Troop.Name=TEXT("Rifle Squad");
  FSelection Hq; Hq.Type=TEXT("hq"); Hq.Name=TEXT("Headquarters");
  const FBottomLayout Mixed=LeftPanel(1080/Scale,TEXT("2 units selected"),{Troop,Hq},{},Candidates,{},FTCPaint::ApproxMeasure(),Scale,0,1,true);
  TestTrue(TEXT("mixed selection retains general Orders and group transfer"),Mixed.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Action==ETCUiAction::AttackMove; }) && Mixed.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Unit==TEXT("hudpanel:transfer"); }));
 }
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud26HitTest, "ThreeCrossroads.HUD.HitEdges", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud26HitTest::RunTest(const FString&)
{
 const FBox2D A(FVector2D(10,10),FVector2D(30,30)), B(FVector2D(30,10),FVector2D(50,30));
 TestTrue(TEXT("DOM hit includes its left/top border"),FTCHudFit::Contains(A,FVector2D(10,10)));
 TestFalse(TEXT("DOM hit excludes its right border"),FTCHudFit::Contains(A,FVector2D(30,20)));
 TestTrue(TEXT("the adjacent target owns the shared boundary"),FTCHudFit::Contains(B,FVector2D(30,20)));
 TestFalse(TEXT("DOM hit excludes its bottom border"),FTCHudFit::Contains(A,FVector2D(20,30)));
 FTCHudFit::FTooltipInteraction Tip;
 Tip.Frame(FVector2D(20,20),true,A);
 TestFalse(TEXT("a pressed tooltip stays dismissed"),Tip.Allows(FVector2D(20,20),A));
 Tip.Frame(FVector2D(30,20),false,A);
 TestTrue(TEXT("leaving at the exact boundary permits the adjacent tooltip"),Tip.Allows(FVector2D(30,20),B));
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud26TruckTest, "ThreeCrossroads.HUD.TruckCardWidth", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud26TruckTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 for (float Scale : {1.f,1.5f}) for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)})
 {
  const FVector2D View=Pixels/Scale;
  const FBottomLayout L=RecruitPanel(View.X,View.Y,View.X-200,{}, {},{}, {},FTCPaint::ApproxMeasure(),Scale,
   {TEXT("Logistics"),TEXT("Supply Truck #407"),TEXT("Loading cargo"),TEXT("Ammunition cargo: 100.0"),TEXT("Provisions cargo: 100 s"),TEXT("Fuel cargo: 100 s"),TEXT("Right-click an allied unit or Supply Cache to deliver carried cargo")});
  TestTrue(TEXT("source fit-content truck card leaves space on both sides"),L.Recruit.GetSize().X<View.X-200-LeftWidth(View,Scale)-16);
  const float Hair=FMath::FloorToFloat(Scale)/Scale;
  TestTrue(TEXT("embedded card retains the parent's snapped 7px padding and border"),FMath::IsNearlyEqual(L.Logistics.Min.X-L.Recruit.Min.X,double(7+Hair),.01));
  TestTrue(TEXT("original Logistics min-width is 240px"),L.Logistics.GetSize().X>=240-.01);
 }
 return true;
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud26ControlsTest, "ThreeCrossroads.HUD.ControlsDialog", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud26ControlsTest::RunTest(const FString&)
{
 for (FVector2D View : {FVector2D(1280,720),FVector2D(1920,1080)})
 {
  const FTCHudFit::FControlsLayout L=FTCHudFit::ControlsDialog(View,FTCPaint::ApproxMeasure());
  TestEqual(TEXT("original dialog width is 720 physical pixels, outside HUD zoom"),L.Bounds.GetSize().X,720.0);
  TestTrue(TEXT("original 82vh/760px height cap"),L.Bounds.GetSize().Y<=FMath::Min(View.Y*.82,760.0));
  const FTCPaintItem* Backdrop=L.Paint.Find(TEXT("controls:backdrop"));
  TestTrue(TEXT("original modal has its .72 dim backdrop"),Backdrop && FMath::IsNearlyEqual(Backdrop->Colour.A,.72f));
  TestEqual(TEXT("all four original sections are present in the scroll model"),L.Paint.TextsOf(TEXT("controls:section:title")).Num(),4);
  TestTrue(TEXT("long original help remains scrollable"),L.ScrollMax>0);
  const FTCHudFit::FControlsLayout End=FTCHudFit::ControlsDialog(View,FTCPaint::ApproxMeasure(),L.ScrollMax);
  TestTrue(TEXT("sticky original Close target is unchanged when scrolled"),End.Close.Equals(L.Close,.01));
  TestTrue(TEXT("all keyboard descriptions remain reachable"),End.Paint.TextsOf(TEXT("controls:body")).Num()==L.Paint.TextsOf(TEXT("controls:body")).Num());
 }
 return true;
}
#endif
