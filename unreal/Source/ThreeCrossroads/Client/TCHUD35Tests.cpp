// HUD startup policy. The integration lead runs this in the engine suite.
#include "Client/TCHUD.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCRows30Harness.h"
#include "Client/TCLocale.h"
#include "Client/TCOrders.h"
#include "Misc/CommandLine.h"
#include "Misc/ScopeExit.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud35StartupTest,"ThreeCrossroads.HUD35.GameStartScreenMessages",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud35StartupTest::RunTest(const FString&)
{
 const FString SavedCommand=FCommandLine::Get();
 const bool SavedMessages=GAreScreenMessagesEnabled;
 ON_SCOPE_EXIT { FCommandLine::Set(*SavedCommand); GAreScreenMessagesEnabled=SavedMessages; };
 TCRows30::FRows30Scene Scene; if (!Scene.Ready(*this)) return false;
 auto Start=[&](bool Automation,const TCHAR* Command)
 {
  TGuardValue<bool> AutomationGuard(GIsAutomationTesting,Automation);
  FCommandLine::Set(Command); GAreScreenMessagesEnabled=true;
  auto* Hud=Scene.World->SpawnActor<ATCHUD>();
  Hud->DispatchBeginPlay();
  return GAreScreenMessagesEnabled;
 };
 TestFalse(TEXT("ordinary game startup suppresses engine overlays"),Start(false,TEXT("")));
 TestTrue(TEXT("automation retains engine overlays"),Start(true,TEXT("")));
 TestTrue(TEXT("native diagnostics retains engine overlays"),Start(false,TEXT("-TCNativeDiagnostics")));
 TestTrue(TEXT("explicit screen-message diagnostic retains engine overlays"),Start(false,TEXT("-TCScreenMessages")));
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud35RefusalTest,"ThreeCrossroads.HUD35.RefusalHint",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud35RefusalTest::RunTest(const FString&)
{
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 {
  FTCLocale::FScopedLanguage Scope(Language);
  const FString Sentence=TCOrders::DenySentence(TEXT("cooldown"),TEXT("fireat"));
  const FString Expected=Language==ETCLanguage::English?TEXT("That order is still on cooldown"):TEXT("Esa orden todavía se está recargando");
  TestEqual(TEXT("hint uses exact keyed refusal without command id"),FTCHudFit::HintText(false,TEXT(""),false,Sentence,0),Expected);
  TestEqual(TEXT("refusal retains original two-second lifetime"),FTCHudFit::HintText(false,TEXT(""),false,Sentence,1.999),Expected);
  TestTrue(TEXT("expired refusal disappears"),FTCHudFit::HintText(false,TEXT(""),false,Sentence,2).IsEmpty());
 }
 return !HasAnyErrors();
}
#endif
