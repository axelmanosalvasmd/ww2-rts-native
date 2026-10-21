#include "Client/TCHUD31Labels.h"
#include "Client/TCMarkers.h"
#include "Client/TCRoomBrowser.h"
#include "Dom/JsonObject.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud31LabelsTest,"ThreeCrossroads.HUD31.WorldLabelExclusion",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud31LabelsTest::RunTest(const FString&)
{
 for (float Scale : {1.f,1.5f})
 {
  auto Box=[&](float X,float Y,float W,float H) { return FBox2D(FVector2D(X,Y)*Scale,FVector2D(X+W,Y+H)*Scale); };
  int32 Painted=0;
  TCHud31::BeginWorldLabels(nullptr);
  // Labels arrive before the HUD measures this frame's score and tooltip.
  TCHud31::SubmitWorldLabel(nullptr,Box(570,45,140,30),[&]() { Painted+=1; });
  TCHud31::SubmitWorldLabel(nullptr,Box(570,92,140,30),[&]() { Painted+=10; });
  TCHud31::SubmitWorldLabel(nullptr,Box(400,220,140,30),[&]() { Painted+=100; });
  TestEqual(TEXT("world text is deferred until current panel bounds exist"),Painted,0);
  TCHud31::FinishWorldLabels({Box(500,8,280,70),Box(560,88,160,45)});
  TestEqual(TEXT("HQ under Scores tied and tooltip is culled, clear income remains"),Painted,100);
  TestFalse(TEXT("world label cannot sit flush on a panel edge"),TCHud31::WorldLabelVisible(Box(500,78,140,30),{Box(500,8,280,70)}));
  TestTrue(TEXT("world label with clear gap survives"),TCHud31::WorldLabelVisible(Box(500,90,140,30),{Box(500,8,280,70)}));
  TCHud31::BeginWorldLabels(nullptr);
  TCHud31::SubmitWorldLabel(nullptr,Box(570,45,140,30),[&]() { Painted+=1; });
  TCHud31::FinishWorldLabels({});
  TestEqual(TEXT("same anchor is visible immediately after panel disappears"),Painted,101);
  TCHud31::FinishWorldLabels({});
  TestEqual(TEXT("no prior-frame label is replayed"),Painted,101);
  int32 Nearby=0;
  TCHud31::BeginWorldLabels(nullptr);
  TCHud31::SubmitWorldLabel(nullptr,Box(400,220,140,30),[&]() { Nearby+=1; });
  TCHud31::SubmitWorldLabel(nullptr,Box(410,235,180,30),[&]() { Nearby+=10; });
  TCHud31::SubmitWorldLabel(nullptr,Box(700,220,140,30),[&]() { Nearby+=100; });
  TCHud31::FinishWorldLabels({});
  TestEqual(TEXT("nearby world captions never paint over one another"),Nearby,101);
 }
 const auto Income=TCMarkersCore::TagStyle(), Hq=TCMarkersCore::HqStyle();
 TestTrue(TEXT("world income and secondary tags have at least 18px text at 720p"),Income.Px*Income.Sx/Income.W>=18);
 TestTrue(TEXT("HQ text also has at least 18px"),Hq.Px*Hq.Sx/Hq.W>=18);
 return !HasAnyErrors();
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud31DirectoryTest,"ThreeCrossroads.RoomBrowser.AdvancingChoices",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud31DirectoryTest::RunTest(const FString&)
{
 FTCRoomBrowser B;
 const FString First=TEXT(R"({"rooms":[{"code":"alpha","title":"First","mode":"conquest","map":"default","state":"lobby","humans":1,"ai":0,"occupied":1,"capacity":4,"joinable":true},{"code":"bravo","title":"Second","mode":"conquest","map":"default","state":"lobby","humans":3,"ai":0,"occupied":3,"capacity":4,"joinable":true}]})");
 TestFalse(TEXT("controlled first transport failure"),B.Request(TEXT("invalid-hud31"),0));
 TestTrue(TEXT("first delivered directory"),B.ApplyRooms(First));
 TestEqual(TEXT("Quick play uses the most humans"),B.QuickPlay(true,TEXT("fresh31"),TEXT("Host")).Room,FString(TEXT("bravo")));
 TestFalse(TEXT("automatic refresh waits the full five seconds"),B.RefreshDue(4.999));
 TestTrue(TEXT("automatic refresh due at five seconds"),B.RefreshDue(5));
 TestFalse(TEXT("controlled refresh fails"),B.Request(TEXT("invalid-hud31"),5));
 TestFalse(TEXT("failed refresh cannot quick-join stale directory"),B.QuickPlay(false,TEXT("fresh31"),TEXT("Host")).bGo);
 TestFalse(TEXT("retry waits another full five seconds"),B.RefreshDue(9.999));
 TestTrue(TEXT("retry due at ten seconds"),B.RefreshDue(10));
 FString Changed=First.Replace(TEXT("\"humans\":1"),TEXT("\"humans\":3")).Replace(TEXT("\"occupied\":1"),TEXT("\"occupied\":3"));
 TestTrue(TEXT("retry installs a changed directory"),B.ApplyRooms(Changed));
 TestEqual(TEXT("equal humans now choose the lower room code"),B.QuickPlay(true,TEXT("fresh31"),TEXT("Host")).Room,FString(TEXT("alpha")));
 TestTrue(TEXT("empty successful directory"),B.ApplyRooms(TEXT("{\"rooms\":[]}")));
 const auto Quick=B.QuickPlay(true,TEXT("fresh31"),TEXT("Host"));
 TestTrue(TEXT("empty directory creates public room"),Quick.bGo && Quick.bListingGiven && Quick.Listing.ServerListed());
 const auto Tutorial=B.Tutorial(TEXT("lesson31"),TEXT("Host"));
 const auto Listing=Tutorial.Listing.ToJson();
 TestTrue(TEXT("Tutorial enters new unlisted room with listing.tutorial"),Tutorial.bGo && Tutorial.bListingGiven && Tutorial.Room==TEXT("lesson31") && !Listing->GetBoolField(TEXT("public")) && Listing->GetBoolField(TEXT("tutorial")) && Listing->GetStringField(TEXT("title"))==TEXT("Tutorial") && !Tutorial.Listing.ServerListed());
 TestEqual(TEXT("HTTP timeout stays eight seconds"),FTCRoomBrowser::TimeoutSeconds,8.0);
 return !HasAnyErrors();
}
#endif
