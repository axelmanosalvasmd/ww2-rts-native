#include "Client/TCHUD31Labels.h"
#include "Client/TCMinimap.h"
#include "Net/TCMatchState.h"
#include "Client/TCUnitTable.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud32CrowdedTest,"ThreeCrossroads.HUD32.CrowdedInformation",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud32CrowdedTest::RunTest(const FString&)
{
 for (float Scale : {1.f,1.5f})
 {
  auto Box=[&](float X,float Y,float W,float H) { return FBox2D(FVector2D(X,Y)*Scale,FVector2D(X+W,Y+H)*Scale); };
  int32 Income=0,Support=0;
  const FBox2D Viewport=Box(0,0,1280,720), Icon=Box(676,246,42,42), Score=Box(330,8,620,124);
  TArray<FBox2D> Occupied={Icon,Score};
  auto Paint=[&](FBox2D Bounds,int32& Count)
  {
   const FVector2D Offset=TCHud31::WorldLabelOffset();
   const FBox2D Placed(Bounds.Min+Offset,Bounds.Max+Offset);
   TestTrue(TEXT("protected caption clears icons, panels and other captions"),TCHud31::WorldLabelVisible(Placed,Occupied));
   TestTrue(TEXT("protected caption remains in viewport"),Placed.Min.X>=6 && Placed.Min.Y>=6 && Placed.Max.X<=Viewport.Max.X-6 && Placed.Max.Y<=Viewport.Max.Y-6);
   Occupied.Add(Placed); Count++;
  };
  TCHud31::BeginWorldLabels(nullptr,Viewport);
  TCHud31::ProtectWorldInformation(nullptr,Icon);
  TCHud31::SubmitWorldLabel(nullptr,Box(664,254,96,30),[&]() { Paint(Box(664,254,96,30),Income); });
  TCHud31::SubmitWorldLabel(nullptr,Box(670,264,190,30),[&]() { Paint(Box(670,264,190,30),Support); });
  TCHud31::FinishWorldLabels({Score});
  TestEqual(TEXT("crowded income survives"),Income,1);
  TestEqual(TEXT("crowded support countdown survives"),Support,1);
  TestTrue(TEXT("placement offset cannot leak outside a callback"),TCHud31::WorldLabelOffset().IsNearlyZero());
  TCHud31::BeginWorldLabels(nullptr,Viewport);
  TCHud31::SubmitWorldLabel(nullptr,Box(676,246,42,42),[&]() { Income++; });
  TCHud31::FinishWorldLabels({});
  TestEqual(TEXT("previous frame icons and caption occupancy are cleared"),Income,2);
 }
 // The physical viewport stays 1280x720 at UI150. The tall score panel covers the original support anchor.
 {
  const FBox2D Viewport(FVector2D(0,0),FVector2D(1280,720));
  TArray<FBox2D> Panels={FBox2D(FVector2D(496,12),FVector2D(784,393)),
   FBox2D(FVector2D(842,12),FVector2D(1268,368)),FBox2D(FVector2D(790,370),FVector2D(1268,411)),
   FBox2D(FVector2D(986,426),FVector2D(1268,708))};
  TArray<FBox2D> Occupied=Panels;
  int32 Visible=0;
  TCHud31::BeginWorldLabels(nullptr,Viewport);
  for (const FBox2D Bounds : {FBox2D(FVector2D(775,330),FVector2D(855,360)),
       FBox2D(FVector2D(540,386),FVector2D(735,425)),FBox2D(FVector2D(765,440),FVector2D(960,479))})
   TCHud31::SubmitWorldLabel(nullptr,Bounds,[&,Bounds]()
   {
    const FVector2D Offset=TCHud31::WorldLabelOffset();
    const FBox2D Placed(Bounds.Min+Offset,Bounds.Max+Offset);
    TestTrue(TEXT("UI150 captions clear tall score, sidebar and each other"),TCHud31::WorldLabelVisible(Placed,Occupied));
    TestTrue(TEXT("UI150 captions stay inside physical viewport"),Placed.Min.X>=6 && Placed.Min.Y>=6 && Placed.Max.X<=1274 && Placed.Max.Y<=714);
    Occupied.Add(Placed); ++Visible;
   });
  TCHud31::FinishWorldLabels(Panels);
  TestEqual(TEXT("UI150 income and both support countdowns survive tall panels"),Visible,3);
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud32WorldMinimapTest,"ThreeCrossroads.HUD32.WorldMinimapReadability",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud32WorldMinimapTest::RunTest(const FString&)
{
 TSharedPtr<FJsonObject> Start;
 const FString Json=TEXT(R"({"t":"start","matchId":32,"you":0,"teams":[0,1],"map":{"name":"World","w":2,"h":2,"world":true,"rows":["HH","HH"],"heights":["44","44"]},"fog":{"v":"AB","e":"AB"},"cells":[[0,"W",1,0,null]]})");
 FTCMatchState State;
 if (!TestTrue(TEXT("World fixture parses"),FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Json),Start) && State.ApplyStart(*Start))) return false;
 FTCUnitTable Table;
 FTCMinimap Map;
 Map.Refresh(State,Table);
 for (float Scale : {1.f,1.5f})
 {
  Map.UiScale=Scale;
  TestTrue(TEXT("World minimap arranges"),Map.Arrange(1280,720,State,-90));
  TestEqual(TEXT("World information square is enlarged"),Map.Rect().Max.X-Map.Rect().Min.X,double(228*Scale));
  for (bool Infantry : {false,true})
  {
   TestEqual(TEXT("held World unit symbol survives large map scale"),TCMinimapCore::UnitDotRadius(true,Infantry,.02,Scale),double(2*Scale));
   TestEqual(TEXT("authored symbol scale stays exact"),TCMinimapCore::UnitDotRadius(false,Infantry,.02,Scale),(Infantry?2.6:3.5)*.02);
  }
  FColor Pixel;
  TestTrue(TEXT("unknown pixel exists"),Map.PixelOf(1,Pixel));
  TestEqual(TEXT("enlargement never reveals unknown terrain"),Pixel,FColor(10,10,10,255));
 }
 return !HasAnyErrors();
}
#endif
