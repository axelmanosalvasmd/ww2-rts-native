// Reference connection.js retires terminal/control packets even before the asynchronous first lobby arrives.
#include "Net/TCConnection.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession26TransportOrderingTest, "ThreeCrossroads.Session.PreLobbyControlOrdering", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession26TransportOrderingTest::RunTest(const FString&)
{
	const FString Start = TEXT(R"({"t":"start","matchId":26})");
	const FString Snapshot = TEXT(R"({"t":"s","tick":26})");
	const FString Lobby = TEXT(R"({"t":"lobby","state":"play","players":[]})");
	const FString Version = TEXT(R"({"t":"deny","cmd":"join","reason":"version"})");
	for (const FString& Control : { FString(TEXT(R"({"t":"replaced"})")), FString(TEXT(R"({"t":"left"})")), FString(TEXT(R"({"t":"full","reason":"seats"})")), Version })
	{
		const auto C = MakeShared<FTCConnection>(FTCConnection::FSettings());
		C->bRunning = true; C->LastNow = 100;
		const uint64 OldGeneration = C->Generation;
		TArray<FString> Calls;
		C->OnText = [&](const FString& Text) { Calls.Add(TEXT("raw:") + Text); };
		C->OnMessage = [&](const FJsonObject& Message, int32 Bytes)
		{
			TestEqual(TEXT("control callback supplies real UTF-8 size"), Bytes, FTCConnection::Utf8Length(Control));
			Calls.Add(TEXT("parsed:") + Message.GetStringField(TEXT("t")));
		};
		C->ReceiveText(Start, OldGeneration); C->ReceiveText(Snapshot, OldGeneration);
		TestTrue(TEXT("unknown-host match stays behind the admission gate"), Calls.IsEmpty() && C->PendingHostMessages.Num() == 2);
		C->ReceiveText(Control, OldGeneration);
		TestEqual(TEXT("control has raw and parsed callbacks without a lobby"), Calls.Num(), 2);
		if (Calls.Num() == 2) TestEqual(TEXT("control retains original bytes"), Calls[0], TEXT("raw:") + Control);
		TestTrue(TEXT("terminal/control retirement discards buffered match"), C->PendingHostMessages.IsEmpty() && C->Generation != OldGeneration);
		if (Control.Contains(TEXT("\"full\"")))
		{
			TestTrue(TEXT("full room still retries after ten seconds"), C->IsRunning() && C->bWaitingForRoom && C->RetryAt == 110 && C->Banner.bFull);
			TestEqual(TEXT("full-room countdown survives opening the next socket"), C->RetryLeft(101), 9);
		}
		else TestTrue(TEXT("terminal seat/version packets stop without retry"), !C->IsRunning() && C->RetryAt < 0);
		C->ReceiveText(Lobby, OldGeneration);
		C->ReceiveText(Start, OldGeneration);
		TestEqual(TEXT("retired generation never publishes a late lobby or match"), Calls.Num(), 2);
	}
	// A routine denial stays ordered behind the pending match. It is not a transport-terminal version denial.
	const auto C = MakeShared<FTCConnection>(FTCConnection::FSettings()); C->bRunning = true;
	const FString Deny = TEXT(R"({"t":"deny","cmd":"move","reason":"blocked"})");
	TArray<FString> Received; C->OnText = [&](const FString& Text) { Received.Add(Text); };
	C->ReceiveText(Start, C->Generation); C->ReceiveText(Deny, C->Generation);
	TestTrue(TEXT("nonterminal packets retain match order until host classification"), Received.IsEmpty());
	C->ReceiveText(Lobby, C->Generation);
	TestTrue(TEXT("reference control preserves start/deny/lobby bytes"), Received == TArray<FString>{ Start, Deny, Lobby });
	return true;
}
#endif
