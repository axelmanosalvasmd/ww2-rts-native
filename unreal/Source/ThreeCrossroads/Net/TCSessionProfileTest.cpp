// The owner R1 profile is checked outside the wire. Until a supported launch is verified, refuse match admission.
#include "Net/TCConnection.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCMatchControls.h"
#include "Client/TCMatchOptions.h"
#include "Client/TCPersistence.h"
#include "Net/TCMatchState.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/CommandLine.h"
#include "Misc/ScopeExit.h"
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

	// Round32: these use the actual transport boundaries, not a copied approval helper.
	// This block also compiles against unchanged round31 production for the ordered RED run.
	{
		const FString SavedLine32 = FCommandLine::Get();
		const FString SavedApproval32 = FPlatformMisc::GetEnvironmentVariable(TEXT("TC_OWNER_R1_APPROVED_URL"));
		const FString SavedQualification32 = FPlatformMisc::GetEnvironmentVariable(TEXT("TC_R1_QUALIFICATION"));
		ON_SCOPE_EXIT { FCommandLine::Set(*SavedLine32); FPlatformMisc::SetEnvironmentVar(TEXT("TC_OWNER_R1_APPROVED_URL"), *SavedApproval32); FPlatformMisc::SetEnvironmentVar(TEXT("TC_R1_QUALIFICATION"), *SavedQualification32); };
		FPlatformMisc::SetEnvironmentVar(TEXT("TC_R1_QUALIFICATION"), TEXT(""));
		struct FEndpointCase { const TCHAR* Name; const TCHAR* Cli; const TCHAR* Active; const TCHAR* Approved; bool bAdmit; };
		const FEndpointCase Cases[] = {
			{ TEXT("exact active"), TEXT("ws://127.0.0.1:31417/ws"), TEXT("ws://127.0.0.1:31417/ws"), TEXT("ws://127.0.0.1:31417/ws"), true },
			{ TEXT("invite overrides stale approved CLI"), TEXT("ws://127.0.0.1:31417/ws"), TEXT("ws://127.0.0.1:31418/ws"), TEXT("ws://127.0.0.1:31417/ws"), false },
			{ TEXT("absent approval"), TEXT("ws://127.0.0.1:31417/ws"), TEXT("ws://127.0.0.1:31417/ws"), TEXT(""), false },
			{ TEXT("path case mismatch"), TEXT("ws://127.0.0.1:31417/WS"), TEXT("ws://127.0.0.1:31417/WS"), TEXT("ws://127.0.0.1:31417/ws"), false },
			{ TEXT("directory replacement approved active"), TEXT("ws://127.0.0.1:31418/ws"), TEXT("ws://127.0.0.1:31417/ws"), TEXT("ws://127.0.0.1:31417/ws"), true },
		};
		for (const FEndpointCase& Row : Cases)
		{
			FCommandLine::Set(*FString::Printf(TEXT("-TCServer=%s"), Row.Cli));
			FPlatformMisc::SetEnvironmentVar(TEXT("TC_OWNER_R1_APPROVED_URL"), Row.Approved);
			TSharedPtr<FJsonObject> UiLobby;
			FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(RustLobby), UiLobby);
			FTCLobbySettings UiSettings; UiSettings.SetConnectionUrl(Row.Active); UiSettings.ApplyLobby(*UiLobby);
			FTCMatchControls UiControls; UiControls.SetConnectionUrl(Row.Active);
			TestEqual(FString::Printf(TEXT("R32 %s Start UI context"), Row.Name), UiSettings.StartAllowed(), Row.bAdmit);
			UiLobby->SetStringField(TEXT("state"), TEXT("play")); UiControls.ApplyLobby(*UiLobby);
			TestEqual(FString::Printf(TEXT("R32 %s Restart UI context"), Row.Name), UiControls.MenuControl(ETCMatchControl::Restart, -1, 1).bEnabled, Row.bAdmit);
			UiSettings.Reset(); UiControls.Reset();
			UiLobby->SetStringField(TEXT("state"), TEXT("lobby")); UiSettings.ApplyLobby(*UiLobby);
			UiLobby->SetStringField(TEXT("state"), TEXT("play")); UiControls.ApplyLobby(*UiLobby);
			TestFalse(TEXT("R32 reset Start UI cannot inherit approval"), UiSettings.StartAllowed());
			TestFalse(TEXT("R32 reset Restart UI cannot inherit approval"), UiControls.MenuControl(ETCMatchControl::Restart, -1, 1).bEnabled);
			// Recreate each connection as directory/room changes do. Reconnect retains the same settings,
			// while Retire invalidates old socket generations. Neither may inherit another connection's approval.
			for (const FString& Payload : { Start, Snapshot })
			{
				FTCConnection::FSettings Settings32; Settings32.Url = Row.Active;
				const auto C = MakeShared<FTCConnection>(Settings32); C->bRunning = true;
				C->ReceiveText(RustLobby, C->Generation);
				for (const TCHAR* Type : { TEXT("start"), TEXT("restart"), TEXT("loadSave") })
				{
					const auto Command = MakeShared<FJsonObject>(); Command->SetStringField(TEXT("t"), Type);
					const ETCSendResult Expected = Row.bAdmit && FString(Type) != TEXT("loadSave") ? ETCSendResult::NotOpen : ETCSendResult::ProfileRefused;
					TestTrue(FString::Printf(TEXT("R32 %s command %s gate"), Row.Name, Type), C->TrySend(Command) == Expected);
				}
				TArray<FString> Publications;
				C->OnText = [&](const FString&) { Publications.Add(TEXT("raw")); };
				C->OnMessage = [&](const FJsonObject&, int32) { Publications.Add(TEXT("parsed")); };
				const uint64 OldGeneration = C->Generation; C->Retire(false);
				C->ReceiveText(Payload, OldGeneration);
				TestEqual(TEXT("R32 retired generation remains unpublished"), Publications.Num(), 0);
				C->ReceiveText(Payload, C->Generation);
				TestTrue(FString::Printf(TEXT("R32 %s incoming %s both decoder gates"), Row.Name, *Payload), Row.bAdmit ? Publications == TArray<FString>{TEXT("raw"), TEXT("parsed")} : Publications.IsEmpty());
				TestEqual(FString::Printf(TEXT("R32 %s running after admission"), Row.Name), C->IsRunning(), Row.bAdmit);
				// The first match can precede the new connection's lobby on reconnect.
				const auto Fresh = MakeShared<FTCConnection>(Settings32); Fresh->bRunning = true;
				int32 HeldRaw = 0, HeldParsed = 0;
				Fresh->OnText = [&](const FString& Text) { if (Text == Payload) ++HeldRaw; };
				Fresh->OnMessage = [&](const FJsonObject& M, int32) { if (M.GetStringField(TEXT("t")) == (Payload == Start ? TEXT("start") : TEXT("s"))) ++HeldParsed; };
				Fresh->ReceiveText(Payload, Fresh->Generation);
				TestTrue(TEXT("R32 unidentified host holds both decoders"), HeldRaw == 0 && HeldParsed == 0);
				Fresh->ReceiveText(RustLobby, Fresh->Generation);
				TestTrue(FString::Printf(TEXT("R32 %s buffered reconnect gates"), Row.Name), HeldRaw == (Row.bAdmit ? 1 : 0) && HeldParsed == HeldRaw);
			}
		}
	}


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

	// Approval is a local attestation for this exact endpoint. All ordinary refusal cases above remain required.
	const FString SavedLine=FCommandLine::Get();
	const FString SavedApproved=FPlatformMisc::GetEnvironmentVariable(TEXT("TC_OWNER_R1_APPROVED_URL"));
	ON_SCOPE_EXIT { FCommandLine::Set(*SavedLine); FPlatformMisc::SetEnvironmentVar(TEXT("TC_OWNER_R1_APPROVED_URL"),*SavedApproved); };
	FCommandLine::Set(TEXT("-TCServer=ws://127.0.0.1:31417/ws"));
	FPlatformMisc::SetEnvironmentVar(TEXT("TC_OWNER_R1_APPROVED_URL"),TEXT("ws://127.0.0.1:31418/ws"));
	TestFalse(TEXT("a different endpoint is unapproved"),FTCConnection::OwnerR1AdmissionEnabled(TEXT("ws://127.0.0.1:31417/ws")));
	FPlatformMisc::SetEnvironmentVar(TEXT("TC_OWNER_R1_APPROVED_URL"),TEXT("ws://127.0.0.1:31417/ws"));
	TestTrue(TEXT("the exactly approved endpoint admits R1"),FTCConnection::OwnerR1AdmissionEnabled(TEXT("ws://127.0.0.1:31417/ws")));
	TestFalse(TEXT("approved Start passes profile gate"),FTCConnection::RustProfileCommandBlocked(ETCHostKind::Rust,TEXT("start"),TEXT("ws://127.0.0.1:31417/ws")));
	TestFalse(TEXT("approved Restart passes profile gate"),FTCConnection::RustProfileCommandBlocked(ETCHostKind::Rust,TEXT("restart"),TEXT("ws://127.0.0.1:31417/ws")));
	TestTrue(TEXT("approved Load remains refused"),FTCConnection::RustProfileCommandBlocked(ETCHostKind::Rust,TEXT("loadSave"),TEXT("ws://127.0.0.1:31417/ws")));
	TSharedPtr<FJsonObject> ApprovedLobby;
	FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(RustLobby),ApprovedLobby);
	FTCLobbySettings ApprovedSettings; ApprovedSettings.SetConnectionUrl(TEXT("ws://127.0.0.1:31417/ws")); ApprovedSettings.ApplyLobby(*ApprovedLobby);
	TestTrue(TEXT("approved valid roster enables Start"),ApprovedSettings.StartAllowed());
	ApprovedLobby->SetStringField(TEXT("state"),TEXT("play")); Controls.SetConnectionUrl(TEXT("ws://127.0.0.1:31417/ws")); Controls.ApplyLobby(*ApprovedLobby);
	TestTrue(TEXT("approved Restart is enabled"),Controls.MenuControl(ETCMatchControl::Restart,-1,1).bEnabled);
	FTCPersistence ApprovedSaves; ApprovedSaves.ApplyLobby(*ApprovedLobby);
	FTCMatchState ApprovedState; ApprovedState.RoomState=TEXT("lobby"); ApprovedState.bAmHost=true;
	TestFalse(TEXT("approved persistence never builds Load"),ApprovedSaves.PlanLoad(ApprovedState,TEXT("any-save")).Message.IsValid());
	for(const FString& BadLobby : {RustLobby.Replace(TEXT("\"proto\":0"),TEXT("\"proto\":1")),RustLobby.Replace(TEXT("\"proto\":0"),TEXT("\"proto\":\"invalid\""))})
	{
		FTCConnection::FSettings ProtocolSettings; ProtocolSettings.Url=TEXT("ws://127.0.0.1:31417/ws");
		const auto C=MakeShared<FTCConnection>(ProtocolSettings); C->bRunning=true;
		int32 Publications=0; C->OnText=[&](const FString& Text){if(Text==Start || Text==Snapshot) ++Publications;};
		C->ReceiveText(BadLobby,C->Generation); C->ReceiveText(Start,C->Generation);
		TestTrue(TEXT("approval cannot bypass malformed or future protocol"),!C->IsRunning() && Publications==0);
	}
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
