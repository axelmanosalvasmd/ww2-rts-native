// The owner R1 profile is checked outside the wire. Until a supported launch is verified, refuse match admission.
#include "Net/TCConnection.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCMatchControls.h"
#include "Client/TCMatchOptions.h"
#include "Client/TCPersistence.h"
#include "Net/TCMatchState.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSessionRustProfileAdmissionTest, "ThreeCrossroads.Session.RustProfileAdmission", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSessionRustProfileAdmissionTest::RunTest(const FString&)
{
	const FString RustLobby = TEXT(R"({"t":"lobby","proto":0,"rules":"fixture-rules","code":"profile","state":"lobby","mode":"conquest","mapName":"default","maps":["default"],"spawns":3,"army":"standard","weather":"map","sandbox":false,"listed":false,"role":"commander","amHost":true,"you":0,"host":0,"spectator":false,"spectators":[],"operatives":[],"players":[{"name":"Alder","team":0,"faction":0,"ai":false,"connected":true},{"name":"Fern","team":1,"faction":1,"ai":false,"connected":true}]})");
	const FString ReferenceLobby = TEXT(R"({"t":"lobby","state":"lobby","mode":"conquest","players":[]})");
	const FString Start = TEXT(R"({"t":"start","matchId":1})");
	const FString Snapshot = TEXT(R"({"t":"s","tick":1})");
	const FString Refusal = FTCConnection::RequiredRustProfileRefusal();

	// Exercise the socket's actual receive method without opening a socket. The callbacks are the two decoder
	// publication points. Neither may observe a refused remote start, reconnect start or unsolicited snapshot.
	for (const FString& Payload : { Start, Snapshot })
	{
		const auto C = MakeShared<FTCConnection>(FTCConnection::FSettings());
		C->bRunning = true;
		TArray<FString> Calls;
		C->OnText = [&](const FString&) { Calls.Add(TEXT("raw")); };
		C->OnMessage = [&](const FJsonObject&, int32) { Calls.Add(TEXT("parsed")); };
		C->ReceiveText(RustLobby, C->Generation);
		TestTrue(TEXT("Rust lobby remains inspectable"), C->IsRunning() && C->IsRustHost() && Calls.Num() == 2);
		FTCLobbySettings L; FTCMatchOptions O;
		TSharedPtr<FJsonObject> Lobby;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(RustLobby), Lobby);
		L.ApplyLobby(*Lobby); O.ApplyLobby(*Lobby);
		TestFalse(TEXT("wire protocol/rules are insufficient for Start"), L.StartAllowed());
		TestEqual(TEXT("otherwise valid roster receives the profile refusal"), L.StartLine(), Refusal);
		TestEqual(TEXT("read-only logistics states requirement and uncertainty"), O.LogisticsLine(), FString(TEXT("R1 requires logistics off; host unverified.")));
		// An automatic reconnect keeps the known host kind until its new lobby arrives. A running room's start
		// takes this same guard, without waiting for the player to press the locally disabled Start button.
		++C->Generation;
		C->ReceiveText(RustLobby.Replace(TEXT("\"state\":\"lobby\""), TEXT("\"state\":\"play\"")), C->Generation);
		Calls.Reset();
		C->Banner.Retry(10, 4, false);
		C->RetryAt = 14;
		C->ReceiveText(Payload, C->Generation);
		TestEqual(TEXT("refused match reaches neither decoder"), Calls.Num(), 0);
		TestFalse(TEXT("refused match stops the connection"), C->IsRunning());
		TestTrue(TEXT("refusal has no retry or misleading retry banner"), C->RetryAt < 0 && !C->Banner.bRetry);
		TestEqual(TEXT("persistent connection status explains refusal"), C->Status(), Refusal);
		C->ReceiveText(Snapshot, C->Generation);
		TestEqual(TEXT("later traffic stays unpublished"), Calls.Num(), 0);
		for (const TCHAR* Type : { TEXT("start"), TEXT("restart"), TEXT("loadSave") })
		{
			const auto Command = MakeShared<FJsonObject>(); Command->SetStringField(TEXT("t"), Type);
			int32 Bytes = 99;
			TestTrue(TEXT("direct transport cannot bypass profile refusal"), C->TrySend(Command, &Bytes) == ETCSendResult::ProfileRefused);
			TestEqual(TEXT("refused command serializes no bytes"), Bytes, 0);
		}
	}

	for (const FString& Payload : { Start, Snapshot })
	{
		const auto C = MakeShared<FTCConnection>(FTCConnection::FSettings()); C->bRunning = true;
		int32 Publications = 0;
		C->OnText = [&](const FString&) { ++Publications; };
		C->OnMessage = [&](const FJsonObject&, int32) { ++Publications; };
		C->ReceiveText(Payload, C->Generation);
		TestTrue(TEXT("match before a host lobby waits without publication"), C->IsRunning() && Publications == 0);
		C->ReceiveText(RustLobby, C->Generation);
		TestTrue(TEXT("a later Rust lobby is inspectable but its held match is refused"), !C->IsRunning() && Publications == 2);
		TestEqual(TEXT("held Rust match uses the same profile refusal"), C->Status(), Refusal);
	}
	{
		const auto C = MakeShared<FTCConnection>(FTCConnection::FSettings()); C->bRunning = true;
		TArray<FString> Received;
		C->OnText = [&](const FString& Text) { Received.Add(Text); };
		C->ReceiveText(Start, C->Generation); C->ReceiveText(Snapshot, C->Generation);
		TestEqual(TEXT("fresh reference reconnect stays unpublished until its lobby"), Received.Num(), 0);
		C->ReceiveText(ReferenceLobby, C->Generation);
		TestTrue(TEXT("asynchronous reference lobby preserves start/snapshot/lobby byte order"), Received == TArray<FString>{Start, Snapshot, ReferenceLobby});
		TestTrue(TEXT("fresh reference reconnect remains admitted"), C->IsRunning() && C->HostKind() == ETCHostKind::Reference);
	}
	{
		const auto C = MakeShared<FTCConnection>(FTCConnection::FSettings()); C->bRunning = true;
		int32 Publications = 0; C->OnText = [&](const FString&) { ++Publications; };
		for (int32 I = 0; I < 65; ++I) C->ReceiveText(Snapshot, C->Generation);
		TestTrue(TEXT("an unidentified host cannot grow an unbounded match queue"), !C->IsRunning() && Publications == 0 && C->PendingHostMessages.IsEmpty());
	}

	// The reference's original bytes and raw-before-parsed ordering survive the new refusal path.
	const auto Reference = MakeShared<FTCConnection>(FTCConnection::FSettings()); Reference->bRunning = true;
	TArray<FString> Calls;
	Reference->OnText = [&](const FString& Text) { Calls.Add(TEXT("raw:") + Text); };
	Reference->OnMessage = [&](const FJsonObject& Message, int32 Bytes)
	{
		Calls.Add(TEXT("parsed:") + Message.GetStringField(TEXT("t")));
		TestTrue(TEXT("reference byte length still supplied"), Bytes > 0);
	};
	Reference->ReceiveText(ReferenceLobby, Reference->Generation);
	Reference->ReceiveText(Start, Reference->Generation);
	Reference->ReceiveText(Snapshot, Reference->Generation);
	TestTrue(TEXT("reference connection and match remain admitted"), Reference->IsRunning() && Reference->HostKind() == ETCHostKind::Reference);
	const TArray<FString> Expected = { TEXT("raw:") + ReferenceLobby, TEXT("parsed:lobby"), TEXT("raw:") + Start, TEXT("parsed:start"), TEXT("raw:") + Snapshot, TEXT("parsed:s") };
	TestTrue(TEXT("reference callbacks retain exact bytes and order"), Calls == Expected);
	Reference->ReceiveText(Start, Reference->Generation + 1);
	TestEqual(TEXT("retired socket callback is ignored"), Calls.Num(), Expected.Num());
	Reference->ReceiveText(TEXT("invalid json"), Reference->Generation);
	TestEqual(TEXT("reference malformed raw text still reaches its existing decoder"), Calls.Last(), FString(TEXT("raw:invalid json")));
	for (const TCHAR* Type : { TEXT("start"), TEXT("restart"), TEXT("loadSave") })
	{
		const auto Command = MakeShared<FJsonObject>(); Command->SetStringField(TEXT("t"), Type);
		TestTrue(TEXT("reference send retains ordinary socket check"), Reference->TrySend(Command) == ETCSendResult::NotOpen);
	}

	// MenuControl uses the same explanation instead of offering a Restart that cannot send.
	TSharedPtr<FJsonObject> Lobby;
	FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(RustLobby), Lobby);
	Lobby->SetStringField(TEXT("state"), TEXT("play"));
	FTCMatchControls Controls; Controls.ApplyLobby(*Lobby);
	const FTCControlButton Restart = Controls.MenuControl(ETCMatchControl::Restart, -1, 1);
	TestTrue(TEXT("Rust Restart is visibly disabled with the shared explanation"), Restart.bShown && !Restart.bEnabled && Restart.Why == Refusal);
	const FTCControlRequest Press = Controls.PressMenu(ETCMatchControl::Restart, -1, 1);
	TestTrue(TEXT("disabled Restart neither arms nor builds a request"), !Press.bArmed && !Press.Message.IsValid() && Press.Note == Refusal);
	Lobby->RemoveField(TEXT("proto")); Controls.ApplyLobby(*Lobby);
	TestTrue(TEXT("reference Restart remains enabled"), Controls.MenuControl(ETCMatchControl::Restart, -1, 1).bEnabled);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSessionRustProfileLoadTest, "ThreeCrossroads.Session.RustProfileLoad", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSessionRustProfileLoadTest::RunTest(const FString&)
{
	TSharedPtr<FJsonObject> Lobby;
	if (!TestTrue(TEXT("save-list fixture parses"), FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TEXT(R"({"t":"lobby","proto":0,"role":"commander","saves":[{"id":"profile-1234567890123-manual","kind":"manual","savedAt":1,"mode":"conquest","minutes":0,"names":["Alder","Fern"]}]})")), Lobby))) return false;
	FTCMatchState State; State.RoomState = TEXT("lobby"); State.RoomCode = TEXT("profile"); State.bAmHost = true;
	FTCPersistence P; P.ApplyLobby(*Lobby);
	const FTCSaveListView Rust = P.SaveList(State);
	TestTrue(TEXT("Rust saved rows remain inspectable"), Rust.bShown && Rust.Rows.Num() == 1);
	TestFalse(TEXT("Rust cannot offer load before profile verification"), Rust.bLoadButtons);
	const auto Refused = P.PlanLoad(State, TEXT("profile-1234567890123-manual"));
	TestFalse(TEXT("direct Load cannot build a start request"), Refused.Message.IsValid());
	TestEqual(TEXT("Load shares the required-profile explanation"), Refused.Note, FString(FTCConnection::RequiredRustProfileRefusal()));
	Lobby->RemoveField(TEXT("proto")); P.ApplyLobby(*Lobby);
	TestTrue(TEXT("reference Load button remains available"), P.SaveList(State).bLoadButtons);
	const auto Sent = P.PlanLoad(State, TEXT("profile-1234567890123-manual"));
	if (TestTrue(TEXT("reference Load still builds a request"), Sent.Message.IsValid()))
	{
		TestEqual(TEXT("reference load command unchanged"), Sent.Message->GetStringField(TEXT("t")), FString(TEXT("loadSave")));
		TestEqual(TEXT("reference load id unchanged"), Sent.Message->GetStringField(TEXT("id")), FString(TEXT("profile-1234567890123-manual")));
	}
	return true;
}
#endif
