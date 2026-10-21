// Source-owned round28 regressions. Isolated models do not start a socket or a match server.
#include "Client/TCHUD.h"
#include "Client/TCGameMode.h"
#include "Client/TCAlerts.h"
#include "Client/TCLocale.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Engine/Canvas.h"
#include "Engine/Engine.h"
#include "Engine/GameInstance.h"
#include "Engine/World.h"
#include "GameFramework/WorldSettings.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

struct FTCHud28Access
{
 static void Observe(ATCHUD& Hud, ATCPlayerController* PC, const FBox2D& Box)
 {
  Hud.PlayerOwner=PC; Hud.DrawnViewport=FVector2D(1280,720);
  Hud.Buttons.Reset();
  Hud.Buttons.Add({Box,FTCUiCommand(),TEXT("Grenadiers"),false,TEXT("Training: Grenadiers")});
 }
 static void Cover(ATCHUD& Hud, const FBox2D& Box)
 { Hud.Buttons.Add({Box,FTCUiCommand(),TEXT("Different inert overlay"),false,TEXT("Other target")}); }
 static void Enable(ATCHUD& Hud) { Hud.Buttons[0].bEnabled=true; }
};

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud28TrainingTest,"ThreeCrossroads.HUD28.TrainingBetweenFrames",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud28TrainingTest::RunTest(const FString&)
{
 UWorld* World=UWorld::CreateWorld(EWorldType::Game,false);
 if (!TestNotNull(TEXT("isolated HUD world"),World)) return false;
 if (!TestNotNull(TEXT("engine for isolated world context"),GEngine)) { World->DestroyWorld(false); return false; }
 UGameInstance* Instance=NewObject<UGameInstance>(World);
 World->SetGameInstance(Instance);
 FWorldContext& Context=GEngine->CreateNewWorldContext(EWorldType::Game);
 Context.SetCurrentWorld(World); Context.OwningGameInstance=Instance;
 auto Cleanup=[&]() { World->DestroyWorld(false); GEngine->DestroyWorldContext(World); };
 World->GetWorldSettings()->DefaultGameMode=ATCGameMode::StaticClass();
 if (!TestTrue(TEXT("real authority mode without BeginPlay"),World->SetGameMode(FURL()))) { Cleanup(); return false; }
 ATCGameMode* Mode=World->GetAuthGameMode<ATCGameMode>();
 ATCPlayerController* PC=World->SpawnActor<ATCPlayerController>();
 ATCHUD* Hud=World->SpawnActor<ATCHUD>();
 if (!TestNotNull(TEXT("mode"),Mode) || !TestNotNull(TEXT("controller"),PC) || !TestNotNull(TEXT("HUD"),Hud)) { Cleanup(); return false; }
 auto Parse=[](const TCHAR* Text) { TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text),O); return O; };
 FTCMatchState& State=Mode->State;
 State.bHasMatch=true; State.You=1; State.LobbySeat=1; State.bLobbySinceOpen=true; State.Factions={0,1};
 FTCUnit Hq; Hq.Id=2; Hq.Type=TEXT("hq"); Hq.Owner=1; Hq.Hp=100; Hq.Built=1; State.Units.Add(2,Hq); PC->Selected={2};
 Mode->Recruit.SetUnitTableReference(FTCRecruitment::ReferencePin);
 Mode->Recruit.ApplyStart(*Parse(TEXT(R"({"factions":[0,1],"map":{}})")));
 Mode->Recruit.ApplySnapshot(*Parse(TEXT(R"({"mp":126,"fuel":70,"mode":{"kind":"classic"},"units":[[2]],"queues":[[2,0.3,-1,-1,"rifle","rifle"]],"productionJobs":[[2,[{"id":1,"type":"rifle","status":"active","mp":100,"fuel":0},{"id":5,"type":"rifle","status":"waiting","mp":100,"fuel":0}]]]})")));
 const FTCCommandCard Fixture=Mode->Recruit.Card(State,PC->Selected);
 if (!TestTrue(TEXT("real recruitment fixture has active rifle and waiting job5"),Fixture.bTraining && Fixture.Slots.Num()==5 && Fixture.Slots[1].Job==5)) { Cleanup(); return false; }
 const FBox2D Slot(FVector2D(489,593.2),FVector2D(523.8,628)); FVector2D At;
 FTCHud28Access::Observe(*Hud,PC,Slot);
 Hud->SetCanvas(nullptr,nullptr); // GameViewportClient clears it after every HUD PostRender.
 TestTrue(TEXT("retained drawn disabled slot is available to input between render callbacks"),Hud->ObservedTrainingSlot(Slot,2,TEXT("rifle"),At));
 TestEqual(TEXT("target is the actual observed slot centre"),At,Slot.GetCenter());
 UCanvas* DuringRender=NewObject<UCanvas>(); DuringRender->ClipX=1280; DuringRender->ClipY=720;
 Hud->SetCanvas(DuringRender,nullptr);
 TestTrue(TEXT("during-render control uses the same drawn slot"),Hud->ObservedTrainingSlot(Slot,2,TEXT("rifle"),At));
 TestFalse(TEXT("wrong producer is refused"),Hud->ObservedTrainingSlot(Slot,3,TEXT("rifle"),At));
 TestFalse(TEXT("wrong active type is refused"),Hud->ObservedTrainingSlot(Slot,2,TEXT("mg"),At));
 TestFalse(TEXT("changed rectangle is refused"),Hud->ObservedTrainingSlot(FBox2D(Slot.Min+FVector2D(1,0),Slot.Max+FVector2D(1,0)),2,TEXT("rifle"),At));
 FTCHud28Access::Enable(*Hud);
 TestFalse(TEXT("an enabled rectangle cannot qualify as the disabled training slot"),Hud->ObservedTrainingSlot(Slot,2,TEXT("rifle"),At));
 FTCHud28Access::Observe(*Hud,PC,Slot); FTCHud28Access::Cover(*Hud,Slot);
 TestFalse(TEXT("a different Action=None overlay does not prove the training target is uncovered"),Hud->ObservedTrainingSlot(Slot,2,TEXT("rifle"),At));
 const FTCBuyResult Waiting=Mode->Recruit.CancelProduction(State,2,5);
 TestTrue(TEXT("waiting job5 still sends its paid cancellation envelope"),Waiting.Messages.Num()==1 && Waiting.Messages[0]->GetNumberField(TEXT("id"))==2 && Waiting.Messages[0]->GetNumberField(TEXT("job"))==5);
 TestTrue(TEXT("active job has no cancellation envelope and no local refund"),Mode->Recruit.CancelProduction(State,2,1).Messages.IsEmpty() && Mode->Recruit.Mp()==126);
 Cleanup();
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud28HistoryClipTest,"ThreeCrossroads.HUD28.HistoryBottomClip",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud28HistoryClipTest::RunTest(const FString&)
{
 for (float Scale : {1.f,1.5f})
 {
  FTCAlerts Alerts; for (int32 I=0;I<12;I++) Alerts.Push(TEXT("ready"),TEXT("Rifle Squad ready"),true,FVector2D(I,0),0);
  Alerts.bHistoryOpen=true;
  FTCAlertViewInput In; In.Now=2; In.ViewW=1280/Scale; In.ViewH=720/Scale; In.Right=In.ViewW-8; In.MinimapTop=In.ViewH-192; In.UiScale=Scale;
  const FTCPaint Paint=Alerts.Layout(In,FTCPaint::ApproxMeasure());
  const FTCPaintItem* History=Paint.Find(TEXT("alert:history"));
  if (!TestNotNull(TEXT("expanded original history"),History)) continue;
  const FTCPaintItem* Last=nullptr;
  for (const FTCPaintItem& Item : Paint.Items) if (Item.Role==TEXT("alert:entry")) Last=&Item;
  if (!TestNotNull(TEXT("visible history entries"),Last)) continue;
  TestTrue(TEXT("CSS overflow retains the partially visible final entry"),Last->Clip.bIsValid && Last->Pos.Y<Last->Clip.Max.Y && Last->Pos.Y+Last->Size.Y>Last->Clip.Max.Y);
  if (Last->Clip.bIsValid)
  {
   TestTrue(TEXT("last entry reaches the physical content bottom"),FMath::IsNearlyEqual(Last->Clip.Max.Y,History->Pos.Y+History->Size.Y-4-FMath::FloorToFloat(Scale)/Scale,.03));
   for (const FTCPaintHit& Hit : Paint.Hits) if (Hit.Tag.StartsWith(TEXT("alert:entry:")))
    TestTrue(TEXT("history entry hit is cropped with its visible pixels"),Hit.Rect.Max.Y<=Last->Clip.Max.Y+.01);
  }
 }
 return !HasAnyErrors();
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud28BuildSymbolsTest,"ThreeCrossroads.HUD28.BuildSymbols",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud28BuildSymbolsTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)}) for (float Scale : {1.f,1.5f})
 {
  FTCLocale::FScopedLanguage Lang(Language);
  FControl Build; Build.Label=TEXT("HQ"); Build.Icon=TEXT("hq"); Build.Command=FTCUiCommand(ETCUiAction::Build); Build.Command.Unit=TEXT("hq");
  FControl Cache=Build; Cache.Icon=Cache.Command.Unit=TEXT("supplycache");
  const FBottomLayout L=LeftPanel(Pixels.Y/Scale,TEXT(""),{},{},{},{Build,Cache},FTCPaint::ApproxMeasure(),Scale);
  for (const FHit& H : L.Hits) if (H.Control.Command.Action==ETCUiAction::Build)
  {
   const bool Symbol=L.Icons.ContainsByPredicate([&](const FIcon& I) { return I.Key==H.Control.Command.Unit && I.Box.Min.Equals(H.Box.Min+FVector2D(3,3),.01) && I.Box.GetSize().Equals(FVector2D(15,15),.01); });
   TestEqual(TEXT("original orderBtn sym paints the 15px secondary building symbol, excluding the separate Supply Cache button"),Symbol,H.Control.Command.Unit!=TEXT("supplycache"));
   TestTrue(TEXT("the original 23px primary tool icon remains"),L.Icons.ContainsByPredicate([&](const FIcon& I) { return I.Key==H.Control.Icon && I.Box.GetSize().Equals(FVector2D(23,23),.01) && H.Box.IsInsideOrOn(I.Box.GetCenter()); }));
  }
 }
 return !HasAnyErrors();
}
#endif
