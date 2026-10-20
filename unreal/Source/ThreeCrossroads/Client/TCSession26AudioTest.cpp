// Uses the real async selected-bank adapter. No audio device, components or audible-output assertion.
#include "Client/TCAudio.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Engine/World.h"
#include "HAL/PlatformTime.h"

class FTCSession26AudioBankCommand : public IAutomationLatentCommand
{
public:
	explicit FTCSession26AudioBankCommand(FAutomationTestBase* InTest) : Test(InTest)
	{
		World = UWorld::CreateWorld(EWorldType::Game, false);
		if (!World) return;
		Audio = NewObject<UTCAudio>(World); Audio->AddToRoot();
		Audio->MatchStarted(1, 1);
		StaleToken = Audio->GetMixer().MatchToken();
		Audio->MatchStarted(2, 0); // restart while original effect requests are still pending
		BeganAt = FPlatformTime::Seconds();
	}
	virtual ~FTCSession26AudioBankCommand() override
	{
		if (Audio)
		{
			Audio->ReturnedToLobby();
			if (Audio->SfxHandle.IsValid()) Audio->SfxHandle->CancelHandle();
			if (Audio->VoiceHandle.IsValid()) Audio->VoiceHandle->CancelHandle();
			Audio->RemoveFromRoot();
		}
		if (World) World->DestroyWorld(false);
	}
	virtual bool Update() override
	{
		if (!Audio) { Test->AddError(TEXT("Could not create isolated selected-bank subsystem")); return true; }
		const auto Bank = Audio->BankState();
		// Timeout is an async failure watchdog, never a load-time or performance assertion.
		if (!Bank.bCallbackCompleted)
		{
			if (FPlatformTime::Seconds() - BeganAt < 60) return false;
			Test->AddError(TEXT("Selected-bank completion callback never arrived")); return true;
		}
		Test->TestTrue(TEXT("actual current callback has all original effects and selected lines"), Bank.FullyLoaded());
		Test->TestEqual(TEXT("35 original effects reached the mixer"), Bank.Effects, 35);
		Test->TestEqual(TEXT("24 original selected takes reached the mixer"), Bank.VoiceLines, 24);
		Test->TestEqual(TEXT("source faction fallback and seat choose this folder"), Bank.Folder, Phase == 0 ? FString(TEXT("ru")) : FString(TEXT("us")));
		Test->TestEqual(TEXT("source sorted speaker selection"), Bank.Speaker, Phase == 0 ? FString(TEXT("brian")) : FString(TEXT("harry")));
		const int32 CurrentToken = Audio->GetMixer().MatchToken();
		Audio->OnVoicesLoaded(StaleToken);
		Test->TestTrue(TEXT("late former-match callback leaves current completion unchanged"), Audio->CompletedLoadToken == CurrentToken && Audio->BankState().FullyLoaded());
		FTCAudioMixer& Mixer = Audio->GetMixer();
		Mixer.SetRunning(true); // logical cue plan only; Tick/Sync and engine components are not called
		Mixer.Advance(10.0); // fixed logical time clears the source voice gap retained across matches
		Mixer.SetRandom([]() { return 0.25; });
		Mixer.Voice(TEXT("move"));
		const FString Prefix = TEXT("voice:") + Bank.Speaker + TEXT("_");
		bool bSelectedLine = false;
		for (const FTCAudioSound& Sound : Mixer.Sounds()) if (!Sound.bDone && Sound.Key.StartsWith(Prefix)) bSelectedLine = true;
		Test->TestTrue(TEXT("move cue uses an actually loaded line from this selected bank"), bSelectedLine);
		Test->TestEqual(TEXT("cue planning creates no engine audio components"), Audio->StartedCount(), 0);
		Mixer.SetRunning(false);
		Audio->ReturnedToLobby();
		Audio->OnVoicesLoaded(CurrentToken);
		Test->TestFalse(TEXT("late completion after lobby cannot mark the bank current"), Audio->BankState().bCallbackCompleted);
		if (Phase++ == 0)
		{
			Audio->MatchStarted(3, 1); // UK uses the original US bank and second sorted speaker
			BeganAt = FPlatformTime::Seconds(); return false;
		}
		return true;
	}
private:
	FAutomationTestBase* Test = nullptr;
	UWorld* World = nullptr;
	UTCAudio* Audio = nullptr;
	int32 Phase = 0, StaleToken = 0;
	double BeganAt = 0;
};

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession26AudioBankTest, "ThreeCrossroads.Session.SelectedAudioBank", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession26AudioBankTest::RunTest(const FString&)
{
	AddCommand(new FTCSession26AudioBankCommand(this));
	return true;
}
#endif
