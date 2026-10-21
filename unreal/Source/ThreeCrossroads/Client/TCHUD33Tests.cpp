#include "Client/TCHUD31Labels.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Client/TCStats.h"
#include "Client/TCLocale.h"
#include "Client/TCMarkers.h"
#include "Client/TCSupportPowers.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Client/TCHUD33Tests.inl"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud33AssociationTest,"ThreeCrossroads.HUD33.GroundAssociation",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud33AssociationTest::RunTest(const FString&)
{
 auto Check=[&](const char* Name,bool Ok){TestTrue(UTF8_TO_TCHAR(Name),Ok);};
 TCHud33Test::Run(Check);
 if (!TestTrue(TEXT("original Barlow metrics load"),FTCType::IsLoaded())) return false;
 // Minimal actual delivered fields from the retained crowded recording, no synthetic unit positions.
 TSharedPtr<FJsonObject> Input;
 const FString Snapshot=TEXT(R"({"nodes":[[104,24,2.5,0]],"strikes":[["strafe",68,78,0,4,0],["dive",86,81,0,4,1]]})");
 if (!TestTrue(TEXT("recorded public HUD fields parse"),FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Snapshot),Input))) return false;
 FTCMarkers Markers; Markers.ApplySnapshot(*Input);
 FTCSupportPowers Powers; FJsonObject Start; Powers.ApplyStart(Start); Powers.ApplySnapshot(*Input);
 FTCMatchState State;
 const auto Strikes=Powers.Strikes(State);
 if (!TestEqual(TEXT("one delivered income source"),Markers.ResourceNodes().Num(),1) ||
     !TestEqual(TEXT("both delivered support warnings"),Strikes.Num(),2)) return false;
 TArray<FString> Keys={FTCMarkers::NodeTag(Markers.ResourceNodes()[0])};
 for (const auto& Strike:Strikes)
 {
  const auto* Def=FTCSupportPowers::Find(Strike.Kind);
  if (!TestNotNull(TEXT("original support definition"),Def)) return false;
  TestEqual(TEXT("authoritative complete countdown"),Strike.T,4.0);
  Keys.Add(FString::Printf(TEXT("%s in %.1f s"),*Def->Name,Strike.T));
 }
 TArray<double> English,Spanish;
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 {
  FTCLocale::FScopedLanguage Scope(Language);
  TArray<double>& Widths=Language==ETCLanguage::English?English:Spanish;
  // Same source face/size and complete English-keyed strings as the actual caption painters.
  Widths.Add((FTCType::Face(true)->Width(TC_T(Keys[0]),30,0,false)+32)*160.0/256.0);
  for (int32 I=1;I<Keys.Num();I++)
   Widths.Add(FTCType::Width(TC_T(Keys[I]),14*1.3f)+10);
 }
 TCHud33Test::Run(Check,English,Spanish);
 return !HasAnyErrors();
}
#endif

#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCRows30Harness.h"
#include "Engine/TextureRenderTarget2D.h"
#include "CanvasTypes.h"
struct FTCHud33Access
{
 static void Prepare(ATCHUD& H, ATCPlayerController& PC,UCanvas* Canvas) { H.PlayerOwner=&PC; H.Canvas=Canvas; }
 static int32 PanelCount(const ATCHUD& H) { return H.Panels.Num(); }
};
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud33NoticeTest,"ThreeCrossroads.HUD33.NoticeExclusion",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud33NoticeTest::RunTest(const FString&)
{
 FTCLocale::FScopedLanguage Language(ETCLanguage::Spanish);
 TCRows30::FRows30Scene Scene; if (!Scene.Ready(*this)) return false; Scene.Playing();
 auto* Hud=Scene.World->SpawnActor<ATCHUD>();
 auto* Target=NewObject<UTextureRenderTarget2D>(); Target->InitCustomFormat(1280,720,PF_B8G8R8A8,false); Target->UpdateResourceImmediate(true);
 FCanvas Draw(Target->GameThread_GetRenderTargetResource(),nullptr,Scene.World,ERHIFeatureLevel::SM5);
 auto* Canvas=NewObject<UCanvas>(); Canvas->Init(1280,720,nullptr,&Draw);
 FTCHud33Access::Prepare(*Hud,*Scene.PC,Canvas);
 for (int32 Scale:{100,150})
 {
  Scene.PC->UiScalePercent=Scale; Scene.PC->GraphicsNotice.Empty(); Hud->DrawHUD();
  const int32 Without=FTCHud33Access::PanelCount(*Hud);
  Scene.PC->GraphicsNotice=FTCGraphics::DropNotice(); Scene.PC->GraphicsNoticeAt=Scene.World->GetRealTimeSeconds(); Hud->DrawHUD();
  TestTrue(TEXT("visible automatic notice reserves bounds before deferred income captions"),FTCHud33Access::PanelCount(*Hud)>Without);
  if (Scale==100) TestTrue(TEXT("both wrapped Spanish notice lines remain visible and protected"),FTCHud33Access::PanelCount(*Hud)>=Without+2);
 }
 Draw.Flush_GameThread(); return !HasAnyErrors();
}
#endif
