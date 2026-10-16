// R1: an offline commander remains human while the host waits for the same seat to reconnect.
#include "Client/TCMatchControls.h"
#include "Client/TCLobbySettings.h"
#include "Net/TCConnection.h"
#include "Client/TCPersistence.h"
#include "Client/TCKeys.h"
#include "Misc/Paths.h"
#include "HAL/FileManager.h"
#include "Misc/FileHelper.h"
#include "Misc/ConfigCacheIni.h"
#include "GameFramework/GameUserSettings.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSessionHelloTest, "ThreeCrossroads.Session.Hello", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSessionHelloTest::RunTest(const FString&)
{
	FTCConnection::FSettings S;
	S.Name = TEXT("Alder"); S.Token = TEXT("fixture-only"); S.bSpectate = true;
	S.Listing = MakeShared<FJsonObject>(); S.Listing->SetBoolField(TEXT("public"), false); S.Listing->SetBoolField(TEXT("tutorial"), true); S.Listing->SetStringField(TEXT("title"), TEXT("Tutorial"));
	const auto H = FTCConnection::Hello(S, true);
	TestEqual(TEXT("hello name"), H->GetStringField(TEXT("name")), S.Name);
	TestEqual(TEXT("hello token"), H->GetStringField(TEXT("token")), S.Token);
	TestEqual(TEXT("hello protocol"), H->GetIntegerField(TEXT("proto")), FTCConnection::ProtocolVersion);
	TestEqual(TEXT("hello client"), H->GetStringField(TEXT("client")), FString(FTCConnection::ClientId));
	TestTrue(TEXT("listing and watching survive reconnect"), H->GetObjectField(TEXT("listing"))->GetBoolField(TEXT("tutorial")) && H->GetBoolField(TEXT("spectate")));
	TestFalse(TEXT("Take a seat overrides the initial spectator switch on reconnect"), FTCConnection::Hello(S, false)->HasField(TEXT("spectate")));
	S.Name = TEXT("Fern");
	TestEqual(TEXT("rename is used in next hello"), FTCConnection::Hello(S, false)->GetStringField(TEXT("name")), S.Name);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSessionCommandGateTest, "ThreeCrossroads.Session.CommandGate", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSessionCommandGateTest::RunTest(const FString&)
{
	TSharedPtr<FJsonObject> L;
	FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TEXT(R"({"t":"lobby","state":"play","you":0,"spectator":true,"players":[{"name":"Alder","connected":true,"ai":false}]})")), L);
	FTCMatchControls C; C.ApplyLobby(*L);
	for (const TCHAR* Type : { TEXT("move"),TEXT("amove"),TEXT("stop"),TEXT("orders"),TEXT("stance"),TEXT("buy"),TEXT("spawn"),TEXT("nextwave") }) TestFalse(Type,C.Sendable(Type));
	for (const TCHAR* Type : { TEXT("ping"),TEXT("resync"),TEXT("name"),TEXT("pause"),TEXT("resume"),TEXT("restart"),TEXT("end"),TEXT("save"),TEXT("leave"),TEXT("handAi") }) TestTrue(Type,C.Sendable(Type));
	L->SetBoolField(TEXT("spectator"),false); C.ApplyLobby(*L);
	TestTrue(TEXT("seated commander can order"),C.Sendable(TEXT("move")));
	C.SetOut(true); TestFalse(TEXT("eliminated commander cannot order"),C.Sendable(TEXT("move")));
	L->SetStringField(TEXT("state"),TEXT("lobby")); C.ApplyLobby(*L);
	TestTrue(TEXT("lobby settings remain sendable"),C.Sendable(TEXT("map")));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSessionPreferencesTest, "ThreeCrossroads.Session.PreferencesRestart", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSessionPreferencesTest::RunTest(const FString&)
{
	// A game validates this object before BeginPlay. Editor automation has not necessarily done so.
	if (UGameUserSettings* Settings = UGameUserSettings::GetGameUserSettings()) Settings->ValidateSettings();
	FTCSettingsStore& Game = FTCSettingsStore::Game();
	const auto SavedName = Game.Get(TEXT("ww2-name"));
	GConfig->RemoveKey(TEXT("/Script/Engine.GameUserSettings"), TEXT("Version"), GGameUserSettingsIni);
	TestTrue(TEXT("game preference save includes Unreal's version header"), Game.Set(TEXT("ww2-name"), TEXT("Preference fixture")));
	int32 Version = 0;
	FConfigFile OnDisk;
	OnDisk.Read(FConfigCacheIni::GetDestIniFilename(TEXT("GameUserSettings"), nullptr, *FPaths::GeneratedConfigDir()));
	TestTrue(TEXT("next engine launch will keep the whole preference branch"), OnDisk.GetInt(TEXT("/Script/Engine.GameUserSettings"), TEXT("Version"), Version) && Version > 0);
	if (SavedName.IsSet()) Game.Set(TEXT("ww2-name"), SavedName.GetValue()); else Game.Remove(TEXT("ww2-name"));
	const FString File = FPaths::Combine(FPaths::ProjectSavedDir(), TEXT("Automation/SessionPreferences.ini"));
	IFileManager::Get().MakeDirectory(*FPaths::GetPath(File), true);
	const TMap<FString,FString> Values = {
		{TEXT("ww2-name"),TEXT("Fern Álvarez")}, {TEXT("ww2-edge"),TEXT("False")}, {TEXT("ww2-pan"),TEXT("2")},
		{TEXT("ww2-capture"),TEXT("always")}, {TEXT("ww2-volume"),TEXT("0.35")}, {TEXT("ww2-muted"),TEXT("1")},
		{TEXT("ww2-ui-scale"),TEXT("150")}, {TEXT("ww2-stats"),TEXT("{\"on\":true,\"position\":\"bottom-left\"}")},
		{TEXT("ww2-cover"),TEXT("False")}, {TEXT("ww2-controls-seen"),TEXT("1")}, {TEXT("ww2rts-lang"),TEXT("es")},
		{TEXT("ww2-autocast"),TEXT("{\"classic:rifle\":false,\"rifle\":true}")}, {TEXT("ww2-gfx"),TEXT("high")},
		{TEXT("ww2-gfx-auto"),TEXT("1")}, {TEXT("ww2-gore"),TEXT("off")}
	};
	{
		FTCSettingsStore First(File);
		for (const auto& Entry : FTCSettingsStore::Entries())
		{
			const FString* V = Values.Find(Entry.Reference);
			if (TestNotNull(*FString::Printf(TEXT("saved preference fixture %s"), Entry.Reference), V)) TestTrue(Entry.Reference, First.Set(Entry.Reference, *V));
		}
		TestTrue(TEXT("flush preferences"), First.Flush());
	}
	FTCSettingsStore Restart(File);
	for (const auto& Pair : Values) TestEqual(*Pair.Key, Restart.Get(Pair.Key).Get(FString()), Pair.Value);
	TestFalse(TEXT("edge consumed after restart"), FTCSettingsStore::ReadEdge(Restart.Get(TEXT("ww2-edge"))));
	TestEqual(TEXT("pan consumed after restart"), FTCSettingsStore::ReadPan(Restart.Get(TEXT("ww2-pan"))), 2);
	TestEqual(TEXT("capture consumed after restart"), FTCMouseCapture::ValidMode(Restart.Get(TEXT("ww2-capture")).Get(FString())), FString(TEXT("always")));
	TestEqual(TEXT("UI consumed after restart"), FTCSettingsStore::ReadUIScale(Restart.Get(TEXT("ww2-ui-scale"))), 150);
	TestEqual(TEXT("volume consumed after restart"), FTCSettingsStore::ReadVolume(Restart.Get(TEXT("ww2-volume")), Restart.Get(TEXT("ww2-muted"))), 0.35);
	TestEqual(TEXT("language consumed after restart"), FTCSettingsStore::ReadLanguage(Restart.Get(TEXT("ww2rts-lang")), TEXT("en")), FString(TEXT("es")));
	IFileManager::Get().Delete(*File);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRustLifecycleOfflineCommanderTest, "ThreeCrossroads.Net.RustLifecycle.OfflineCommander", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCRustLifecycleOfflineCommanderTest::RunTest(const FString&)
{
	TSharedPtr<FJsonObject> Lobby;
	if (!TestTrue(TEXT("offline R1 lobby decoded"), FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TEXT(R"({"t":"lobby","proto":0,"code":"R19","state":"play","role":"commander","amHost":true,"you":0,"host":0,"spectator":false,"spectators":[],"operatives":[],"listed":false,"mode":"conquest","mapName":"default","maps":["default"],"spawns":3,"army":"standard","weather":"map","sandbox":false,"players":[{"name":"Alder","team":0,"ai":false,"connected":true},{"name":"Fern","team":1,"ai":false,"connected":false}]})")), Lobby))) return false;
	FTCMatchControls Controls;
	TestTrue(TEXT("offline lobby applied"), Controls.ApplyLobby(*Lobby));
	Controls.SetPaused(true);
	Controls.ToggleMenu();
	const FTCControlButton Hand = Controls.MenuControl(ETCMatchControl::HandAi, 1, 0);
	TestTrue(TEXT("offline seat remains visible in the menu"), Hand.bShown);
	TestFalse(TEXT("R1 never offers an AI takeover"), Hand.bEnabled);
	TestEqual(TEXT("existing restriction explains it"), Hand.Why, FString(TEXT("Rust host: no AI commanders.")));
	const FTCControlRequest Refused = Controls.PressMenu(ETCMatchControl::HandAi, 1, 0);
	TestFalse(TEXT("direct button invocation cannot send takeover"), Refused.Message.IsValid());
	TestEqual(TEXT("direct invocation explains refusal"), Refused.Note, Hand.Why);
	TestTrue(TEXT("refusal keeps the menu open"), Controls.MenuOpen());
	FTCLobbySettings Settings;
	Settings.ApplyLobby(*Lobby);
	const auto Command = MakeShared<FJsonObject>();
	Command->SetStringField(TEXT("t"), TEXT("handAi")); Command->SetNumberField(TEXT("slot"), 1);
	TestEqual(TEXT("common send guard agrees with the disabled button"), Settings.RustCommandRefusal(*Command), Hand.Why);
	Lobby->GetArrayField(TEXT("players"))[1]->AsObject()->SetBoolField(TEXT("connected"), true);
	Controls.ApplyLobby(*Lobby);
	Controls.SetPaused(false);
	TestFalse(TEXT("same-seat return removes the offline takeover button"), Controls.MenuControl(ETCMatchControl::HandAi, 1, 1).bShown);
	TestFalse(TEXT("seat stays human"), Controls.Seats()[1].bAi);
	// Applying a reference lobby on the same model must restore the reference's takeover path.
	Lobby->RemoveField(TEXT("proto"));
	Lobby->GetArrayField(TEXT("players"))[1]->AsObject()->SetBoolField(TEXT("connected"), false);
	Controls.ApplyLobby(*Lobby); Settings.ApplyLobby(*Lobby);
	const FTCControlButton Reference = Controls.MenuControl(ETCMatchControl::HandAi, 1, 2);
	TestTrue(TEXT("reference offline takeover stays enabled"), Reference.bShown && Reference.bEnabled);
	const FTCControlRequest Sent = Controls.PressMenu(ETCMatchControl::HandAi, 1, 2);
	if (TestTrue(TEXT("reference takeover still sends"), Sent.Message.IsValid()))
	{
		TestEqual(TEXT("reference command"), Sent.Message->GetStringField(TEXT("t")), FString(TEXT("handAi")));
		TestEqual(TEXT("reference seat"), Sent.Message->GetIntegerField(TEXT("slot")), 1);
	}
	TestEqual(TEXT("reference send guard unchanged"), Settings.RustCommandRefusal(*Command), FString());
	return true;
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRustTerminalNoticeTest, "ThreeCrossroads.Net.RustLifecycle.TerminalNotice", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCRustTerminalNoticeTest::RunTest(const FString&)
{
	const FString Notice = FTCMatchControls::RestartRefusal(true, true, TEXT("restart"), TEXT("scenario"));
	TestTrue(TEXT("in-play restart refusal explains terminal fault restriction"), Notice.Contains(TEXT("server fault")) && Notice.Contains(TEXT("cannot restart")));
	TestTrue(TEXT("reference keeps its sentence"), FTCMatchControls::RestartRefusal(false, true, TEXT("restart"), TEXT("scenario")).IsEmpty());
	TestTrue(TEXT("initial start keeps its sentence"), FTCMatchControls::RestartRefusal(true, false, TEXT("start"), TEXT("scenario")).IsEmpty());
	TestTrue(TEXT("other reasons untouched"), FTCMatchControls::RestartRefusal(true, true, TEXT("restart"), TEXT("blocked")).IsEmpty());
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSessionNoticeTest, "ThreeCrossroads.Session.ConnectionNotice", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSessionNoticeTest::RunTest(const FString&)
{
	FTCConnectionNotice N; N.Retry(100, 4, false);
	TestEqual(TEXT("retry countdown"), N.Left(101.1), 3);
	TestEqual(TEXT("new socket attempt retains banner"), N.Text(104), FString(TEXT("Connection lost. Retrying in 1 s")));
	FJsonObject M; M.SetStringField(TEXT("t"), TEXT("lobby")); N.Receive(M);
	TestTrue(TEXT("first server message hides it"), N.Text(104).IsEmpty());
	M.SetStringField(TEXT("t"), TEXT("full")); M.SetStringField(TEXT("reason"), TEXT("started")); N.Receive(M); N.Retry(110, 10, true);
	TestEqual(TEXT("full ten-second wait"), N.Text(110), FString(TEXT("Trying this room again in 10 s")));
	TestEqual(TEXT("full started reason"), N.RoomLine(), FString(TEXT("A match is running in this room. You will join when it ends.")));
	M.SetStringField(TEXT("t"), TEXT("replaced")); N.Receive(M);
	TestTrue(TEXT("replaced offers reclaim without automatic retry"), N.bReplaced && N.Left(111) < 0 && N.Text(111) == TEXT("Open in another tab"));
	N = FTCConnectionNotice(); TestFalse(TEXT("Use it here resets notice"), N.bReplaced);
	M.SetStringField(TEXT("t"), TEXT("left")); N.Receive(M);
	TestTrue(TEXT("left explains AI takeover"), N.bLeft && N.Left(112) < 0 && N.Text(112).Contains(TEXT("an AI took over")));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSessionPingTest, "ThreeCrossroads.Session.PingDiagnostics", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSessionPingTest::RunTest(const FString&)
{
	FTCPerf P; P.Pinged(100); P.Pinged(2000);
	FJsonObject Pong; Pong.SetNumberField(TEXT("c"), 2000); Pong.SetArrayField(TEXT("srv"), { MakeShared<FJsonValueNumber>(2.5), MakeShared<FJsonValueNumber>(2) });
	P.Pong(Pong, 2040); P.Frame(100, 100, 0, 0, 0, 8, 3, 0); P.Frame(3201, 3201, 0, 0, 0, 8, 3, 0);
	const auto D = FTCMatchControls::PingDiagnostics(P.Stats());
	TestEqual(TEXT("twelve diagnostics in reference order"), D.Num(), 12);
	TestEqual(TEXT("ping RTT"), D[6]->AsNumber(), 40.0);
	TestEqual(TEXT("one stalled and one answered ping, 50 percent loss"), D[8]->AsNumber(), 50.0);
	TestTrue(TEXT("JS heap unavailable natively"), D[9]->IsNull());
	TestEqual(TEXT("server p95 read"), P.Stats().SrvTick.Get(0), 2.5);
	TestEqual(TEXT("entities"), D[10]->AsNumber(), 8.0);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSessionFlashTest, "ThreeCrossroads.Session.MatchStarted", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSessionFlashTest::RunTest(const FString&)
{
	FTCMatchStartFlash F; F.Start(true, false, 10);
	TestTrue(TEXT("inactive new match flashes"), F.Frame(false, 10));
	TestFalse(TEXT("normal title after a second"), F.Frame(false, 11));
	TestTrue(TEXT("match title after two seconds"), F.Frame(false, 12));
	TestFalse(TEXT("focus stops flash"), F.Frame(true, 12));
	TestFalse(TEXT("stays stopped"), F.Frame(false, 14));
	F.Start(true, true, 20); TestFalse(TEXT("resume never flashes"), F.Frame(false, 20));
	F.Start(false, false, 30); TestFalse(TEXT("visible match never flashes"), F.Frame(false, 30));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSessionViewTest, "ThreeCrossroads.Session.ViewMemory", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSessionViewTest::RunTest(const FString&)
{
	FTCMatchView V; V.MatchId = 7; V.X = 12; V.Z = 34; V.Yaw = 0.2; V.Dist = 70; V.Selected = { 4, 5 }; V.Groups.Add(3, { 5, 4 });
	const FString Dir = FPaths::Combine(FPaths::ProjectSavedDir(), TEXT("Automation/SessionViewMemory"));
	FTCMatchViewMemory First; First.Open(Dir, TEXT("ws://localhost:123/ws"), TEXT("room1"), TEXT("fixture-seat"));
	TestTrue(TEXT("saved"), First.Write(V));
	FTCMatchViewMemory AfterRestart; AfterRestart.Open(Dir, TEXT("ws://localhost:123/ws"), TEXT("room1"), TEXT("fixture-seat"));
	FTCMatchView R;
	TestTrue(TEXT("fresh store reads persisted view"), AfterRestart.Read(R));
	TestTrue(TEXT("camera, selection and group order survive"), R.MatchId == 7 && R.X == 12 && R.Yaw == 0.2 && R.Selected == V.Selected && R.Groups.FindRef(3) == V.Groups.FindRef(3));
	FTCMatchViewMemory Other; Other.Open(Dir, TEXT("ws://localhost:123/ws"), TEXT("room1"), TEXT("another-seat"));
	TestFalse(TEXT("different token cannot restore"), Other.Read(R));
	auto O = V.Json(); O->GetObjectField(TEXT("camera"))->SetNumberField(TEXT("x"), NAN);
	TestFalse(TEXT("corrupt camera ignored"), FTCMatchView::Parse(*O, R));
	AfterRestart.Clear(); TestFalse(TEXT("clear persists"), First.Read(R));
	IFileManager::Get().DeleteDirectory(*Dir);
	return true;
}
#endif
