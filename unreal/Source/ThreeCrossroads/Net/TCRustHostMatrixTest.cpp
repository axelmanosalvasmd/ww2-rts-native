// Owner decisions 090 R1 and 097. Real lobby models, no live socket or wall clock.
#include "Net/TCConnection.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCMatchOptions.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Policies/CondensedJsonPrintPolicy.h"

namespace TCRustHostMatrixTest
{
	TSharedRef<FJsonObject> Lobby()
	{
		TSharedPtr<FJsonObject> O;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TEXT(R"({"t":"lobby","proto":0,"code":"R18","state":"lobby","role":"commander","amHost":true,"you":0,"host":0,"spectator":false,"spectators":[],"operatives":[],"listed":false,"defenderTeam":0,"mode":"conquest","mapName":"default","maps":["default","el-alamein","custom"],"spawns":3,"army":"standard","weather":"map","sandbox":false,"players":[{"name":"a","team":0,"faction":0,"ai":false,"connected":true},{"name":"b","team":1,"faction":1,"ai":false,"connected":true}]})")), O);
		return O.ToSharedRef();
	}
	FString Wire(const TSharedPtr<FJsonObject>& O)
	{
		if (!O) return FString();
		FString S;
		FJsonSerializer::Serialize(O.ToSharedRef(), TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&S));
		return S;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRustHostDetectionTest, "ThreeCrossroads.Net.RustHostDetection", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCRustHostDetectionTest::RunTest(const FString&)
{
	using namespace TCRustHostMatrixTest;
	const auto O = Lobby();
	int32 Proto = 99; FString Rules;
	O->SetStringField(TEXT("rules"), TEXT("hash"));
	TestTrue(TEXT("proto zero identifies Rust"), FTCConnection::HostKindOf(*O, &Proto, &Rules) == ETCHostKind::Rust);
	TestEqual(TEXT("v0"), Proto, 0); TestEqual(TEXT("rules exposed"), Rules, FString(TEXT("hash")));
	for (const TSharedPtr<FJsonValue>& V : TArray<TSharedPtr<FJsonValue>>{ MakeShared<FJsonValueNull>(), MakeShared<FJsonValueString>(TEXT("0")), MakeShared<FJsonValueNumber>(0.5), MakeShared<FJsonValueNumber>(1), MakeShared<FJsonValueBoolean>(false) })
	{
		O->SetField(TEXT("proto"), V);
		TestTrue(TEXT("present malformed or future proto never becomes reference"), FTCConnection::HostKindOf(*O, &Proto) == ETCHostKind::Rust);
		FTCLobbySettings L; L.ApplyLobby(*O);
		TestFalse(TEXT("unsupported proto cannot start"), L.StartAllowed());
		TestEqual(TEXT("unsupported version reason"), L.StartLine(), FString(TEXT("Rust host: unsupported lobby protocol version.")));
	}
	O->RemoveField(TEXT("proto"));
	TestTrue(TEXT("absence identifies reference"), FTCConnection::HostKindOf(*O, &Proto) == ETCHostKind::Reference);
	TestEqual(TEXT("reference version absent"), Proto, -1);
	O->SetStringField(TEXT("t"), TEXT("s"));
	TestTrue(TEXT("snapshot is no host announcement"), FTCConnection::HostKindOf(*O) == ETCHostKind::Unknown);
	O->SetStringField(TEXT("t"), TEXT("deny")); O->SetStringField(TEXT("cmd"), TEXT("join")); O->SetStringField(TEXT("reason"), TEXT("version"));
	TestTrue(TEXT("version denial terminal"), FTCConnection::IsVersionDeny(*O));
	O->SetStringField(TEXT("t"), TEXT("s")); TestFalse(TEXT("non-deny not terminal"), FTCConnection::IsVersionDeny(*O));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRustHostMatrixTest, "ThreeCrossroads.Net.RustHostMatrix", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCRustHostMatrixTest::RunTest(const FString&)
{
	using namespace TCRustHostMatrixTest;
	FTCLobbySettings L; FTCMatchOptions Options;
	const auto Base = Lobby();
	const FString ProfileRefusal = FTCConnection::RequiredRustProfileRefusal();
	TestTrue(TEXT("valid lobby accepted"), L.ApplyLobby(*Base));
	TestFalse(TEXT("valid roster alone cannot qualify R1"), L.StartAllowed());
	TestEqual(TEXT("required profile remains unverified"), L.StartLine(), ProfileRefusal);
	Options.ApplyLobby(*Base);
	TestEqual(TEXT("required logistics is not an observed host option"), Options.LogisticsLine(), FString(TEXT("R1 requires logistics off; host unverified.")));
	TestFalse(TEXT("Sandbox on refused"), Options.PlanSandbox(true).Message.IsValid());
	TestFalse(TEXT("Sandbox off fixed control"), Options.Control(ETCMatchOption::Sandbox).bEditable);
	for (const FString& Weather : FTCMatchOptions::WeatherChoices())
	{
		const auto O = Lobby(); O->SetStringField(TEXT("weather"), Weather); L.ApplyLobby(*O);
		TestEqual(TEXT("existing weather choice cannot qualify the private profile"), L.RustStartRefusal(), ProfileRefusal);
		Options.ApplyLobby(*O); TestTrue(TEXT("weather remains editable"), Options.Control(ETCMatchOption::Weather).bEditable);
	}
	// Full committed public map catalogue at d2911267. A lobby-advertised custom map is still refused.
	const TArray<FString> Maps = {
TEXT("ardennes-crossing"), TEXT("bastogne"), TEXT("bocage"), TEXT("crater-field"), TEXT("crossroads-village"), TEXT("default"), TEXT("el-alamein"), TEXT("helms-deep"), TEXT("hill-112"), TEXT("hill-112-xl"), TEXT("hot-gates"), TEXT("hurtgen-forest"), TEXT("island-towns"), TEXT("kasserine-pass"), TEXT("king-of-the-hill"), TEXT("monte-cassino"), TEXT("monte-cassino-xl"), TEXT("no-mans-land"), TEXT("pegasus-bridge"), TEXT("pegasus-bridge-xl"), TEXT("prokhorovka"), TEXT("river-towns"), TEXT("seawall"), TEXT("seawall-xl"), TEXT("seelow-heights"), TEXT("six-fronts"), TEXT("stalingrad-factory"), TEXT("test-arena"), TEXT("the-great-bridge"), TEXT("the-polder"), TEXT("three-islands"), TEXT("tutorial"), TEXT("twin-valleys") };
	for (const FString& Map : Maps)
	{
		const auto O = Lobby(); O->SetStringField(TEXT("mapName"), Map);
		O->SetArrayField(TEXT("maps"), { MakeShared<FJsonValueString>(Map) });
		L.ApplyLobby(*O); TestFalse(*Map, L.StartAllowed());
		TestEqual(TEXT("public map passes roster checks but profile is unverified"), L.RustStartRefusal(), ProfileRefusal);
	}
	auto Refused = [&](const TCHAR* Expected, TFunction<void(FJsonObject&)> Change)
	{
		const auto O = Lobby(); Change(*O); L.ApplyLobby(*O);
		TestFalse(Expected, L.StartAllowed()); TestEqual(Expected, L.StartLine(), FString(Expected));
		// Removing only proto must restore the reference's exact old predicate and reason.
		O->RemoveField(TEXT("proto")); L.ApplyLobby(*O);
		TestFalse(TEXT("reference identified"), L.IsRustHost());
		const int32 N = O->GetArrayField(TEXT("players")).Num();
		const bool ExpectedReference = N > 0 && N <= O->GetIntegerField(TEXT("spawns"));
		TestEqual(TEXT("reference predicate unchanged"), L.StartAllowed(), ExpectedReference);
		Options.ApplyLobby(*O); TestEqual(TEXT("reference has no logistics line"), Options.LogisticsLine(), FString());
	};
	Refused(TEXT("Rust host: trusted private rooms only."), [](FJsonObject& O) { O.SetBoolField(TEXT("listed"), true); });
	Refused(TEXT("Rust host: no late join or start during a match. Reconnect with the same seat token."), [](FJsonObject& O) { O.SetStringField(TEXT("state"), TEXT("play")); });
	Refused(TEXT("Rust host: no operatives."), [](FJsonObject& O) { O.SetStringField(TEXT("role"), TEXT("operative")); });
	Refused(TEXT("Rust host: no operatives."), [](FJsonObject& O) { O.SetArrayField(TEXT("operatives"), { MakeShared<FJsonValueString>(TEXT("op")) }); });
	Refused(TEXT("Rust host: no operatives."), [](FJsonObject& O) { O.GetArrayField(TEXT("players"))[1]->AsObject()->SetStringField(TEXT("role"), TEXT("operative")); });
	Refused(TEXT("Rust host: no spectators."), [](FJsonObject& O) { O.SetBoolField(TEXT("spectator"), true); });
	Refused(TEXT("Rust host: no spectators."), [](FJsonObject& O) { O.SetArrayField(TEXT("spectators"), { MakeShared<FJsonValueString>(TEXT("watcher")) }); });
	Refused(TEXT("Rust host: no spectators."), [](FJsonObject& O) { O.RemoveField(TEXT("spectators")); });
	Refused(TEXT("Rust host: no operatives."), [](FJsonObject& O) { O.RemoveField(TEXT("operatives")); });
	for (const TCHAR* Mode : { TEXT("world"), TEXT("assault"), TEXT("classic"), TEXT("horde"), TEXT("tutorial"), TEXT("annihilation"), TEXT("invalid") })
		Refused(TEXT("Rust host: Conquest/Default only."), [Mode](FJsonObject& O) { O.SetStringField(TEXT("mode"), Mode); });
	for (const TCHAR* Map : { TEXT("custom"), TEXT("absent") })
		Refused(TEXT("Rust host: choose a committed public map with at least 2 spawns."), [Map](FJsonObject& O) { O.SetStringField(TEXT("mapName"), Map); });
	Refused(TEXT("Rust host: choose a committed public map with at least 2 spawns."), [](FJsonObject& O) { O.SetNumberField(TEXT("spawns"), 1); });
	Refused(TEXT("Rust host: choose a committed public map with at least 2 spawns."), [](FJsonObject& O) { O.SetArrayField(TEXT("maps"), {}); });
	for (const TCHAR* Army : { TEXT("large"), TEXT("massive"), TEXT("endless") })
		Refused(TEXT("Rust host: Standard army only."), [Army](FJsonObject& O) { O.SetStringField(TEXT("army"), Army); });
	Refused(TEXT("Rust host: Sandbox must be off."), [](FJsonObject& O) { O.SetBoolField(TEXT("sandbox"), true); });
	Refused(TEXT("Rust host: choose an existing weather option."), [](FJsonObject& O) { O.SetStringField(TEXT("weather"), TEXT("invalid")); });
	Refused(TEXT("Rust host: no AI commanders."), [](FJsonObject& O) { O.GetArrayField(TEXT("players"))[1]->AsObject()->SetBoolField(TEXT("ai"), true); });
	Refused(TEXT("Rust host: exactly 2 human commanders required."), [](FJsonObject& O) { auto P = O.GetArrayField(TEXT("players")); P.RemoveAt(1); O.SetArrayField(TEXT("players"), P); });
	Refused(TEXT("Rust host: exactly 2 human commanders required."), [](FJsonObject& O) { auto P = O.GetArrayField(TEXT("players")); const auto Extra = P[1]; P.Add(Extra); O.SetArrayField(TEXT("players"), P); });
	for (int32 Team : { 0, 2, 5, -1 })
		Refused(TEXT("Rust host: one commander on Team 1 and one on Team 2."), [Team](FJsonObject& O) { O.GetArrayField(TEXT("players"))[1]->AsObject()->SetNumberField(TEXT("team"), Team); });
	Refused(TEXT("Waiting for the host to start..."), [](FJsonObject& O) { O.SetBoolField(TEXT("amHost"), false); O.SetNumberField(TEXT("you"), 1); });
	Refused(TEXT("Waiting for the host to start..."), [](FJsonObject& O) { O.SetNumberField(TEXT("you"), 1); });
	Refused(TEXT("Rust host: exactly 2 human commanders required."), [](FJsonObject& O) { O.SetArrayField(TEXT("players"), {}); });
	const auto Swapped = Lobby(); Swapped->GetArrayField(TEXT("players"))[0]->AsObject()->SetNumberField(TEXT("team"), 1); Swapped->GetArrayField(TEXT("players"))[1]->AsObject()->SetNumberField(TEXT("team"), 0);
	L.ApplyLobby(*Swapped); TestEqual(TEXT("either seat order passes roster checks, profile still unverified"), L.RustStartRefusal(), ProfileRefusal);
	const auto Offline = Lobby(); Offline->GetArrayField(TEXT("players"))[1]->AsObject()->SetBoolField(TEXT("connected"), false);
	L.ApplyLobby(*Offline); TestEqual(TEXT("097 offline seat adds no readiness refusal"), L.RustStartRefusal(), ProfileRefusal);
	L.ApplyLobby(*Base);
	for (const TPair<FString, FString>& Pair : TArray<TPair<FString, FString>>{
		{ TEXT("addAi"), TEXT("Rust host: no AI commanders.") }, { TEXT("handAi"), TEXT("Rust host: no AI commanders.") }, { TEXT("spectate"), TEXT("Rust host: no spectators.") }, { TEXT("fps"), TEXT("Rust host: no operatives.") } })
	{
		const auto C = MakeShared<FJsonObject>(); C->SetStringField(TEXT("t"), Pair.Key);
		TestEqual(TEXT("raw command refusal matches UI"), L.RustCommandRefusal(*C), Pair.Value);
	}
	const auto Start = MakeShared<FJsonObject>(); Start->SetStringField(TEXT("t"), TEXT("start"));
	TestEqual(TEXT("unverified start command refused"), L.RustCommandRefusal(*Start), ProfileRefusal);
	Start->SetStringField(TEXT("t"), TEXT("restart"));
	TestEqual(TEXT("unverified healthy restart refused"), L.RustCommandRefusal(*Start), ProfileRefusal);
	Start->SetStringField(TEXT("t"), TEXT("loadSave"));
	TestEqual(TEXT("load cannot bypass the profile gate"), L.RustCommandRefusal(*Start), ProfileRefusal);
	const auto Play = Lobby(); Play->SetStringField(TEXT("state"), TEXT("play")); L.ApplyLobby(*Play);
	Start->SetStringField(TEXT("t"), TEXT("sit")); TestEqual(TEXT("late seat refused"), L.RustCommandRefusal(*Start), L.StartLine());
	L.ApplyLobby(*Base);
	TestFalse(TEXT("no other mode request"), L.Plan(ETCLobbySetting::Mode, -1, TEXT("world")).Message.IsValid());
	TestFalse(TEXT("no custom map request"), L.Plan(ETCLobbySetting::Map, -1, TEXT("custom")).Message.IsValid());
	TestFalse(TEXT("no larger army request"), L.Plan(ETCLobbySetting::Army, -1, TEXT("large")).Message.IsValid());
	TestFalse(TEXT("no Team 3 request"), L.Plan(ETCLobbySetting::Team, 1, 2).Message.IsValid());
	const auto Sb = Lobby(); Sb->SetBoolField(TEXT("sandbox"), true); Options.ApplyLobby(*Sb);
	TestTrue(TEXT("can correct Sandbox to off"), Options.PlanSandbox(false).Message.IsValid());
	TestFalse(TEXT("cannot enable Sandbox even if lobby currently on"), Options.PlanSandbox(true).Message.IsValid());
	L.Reset(); Options.Reset(); TestFalse(TEXT("reset forgets host"), L.IsRustHost()); TestEqual(TEXT("reset removes fixed line"), Options.LogisticsLine(), FString());
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCReferenceMatrixUnchangedTest, "ThreeCrossroads.Net.ReferenceMatrixUnchanged", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCReferenceMatrixUnchangedTest::RunTest(const FString&)
{
	using namespace TCRustHostMatrixTest;
	const auto O = Lobby(); O->RemoveField(TEXT("proto"));
	FTCLobbySettings L; FTCMatchOptions P;
	L.ApplyLobby(*O); P.ApplyLobby(*O);
	TestTrue(TEXT("reference accepted"), L.StartAllowed()); TestEqual(TEXT("reference start line unchanged"), L.StartLine(), FString());
	TestEqual(TEXT("world request bytes unchanged"), Wire(L.Plan(ETCLobbySetting::Mode, -1, TEXT("world")).Message), FString(TEXT(R"({"t":"mode","v":"world"})")));
	TestEqual(TEXT("custom map request bytes unchanged"), Wire(L.Plan(ETCLobbySetting::Map, -1, TEXT("custom")).Message), FString(TEXT(R"({"t":"map","name":"custom"})")));
	TestEqual(TEXT("army request bytes unchanged"), Wire(L.Plan(ETCLobbySetting::Army, -1, TEXT("large")).Message), FString(TEXT(R"({"t":"army","v":"large"})")));
	TestEqual(TEXT("Team 3 request bytes unchanged"), Wire(L.Plan(ETCLobbySetting::Team, 1, 2).Message), FString(TEXT(R"({"t":"team","slot":1,"v":2})")));
	TestEqual(TEXT("Sandbox request bytes unchanged"), Wire(P.PlanSandbox(true).Message), FString(TEXT(R"({"t":"sandbox","v":true})")));
	O->SetBoolField(TEXT("sandbox"), true); O->SetBoolField(TEXT("listed"), true); O->SetStringField(TEXT("army"), TEXT("endless"));
	O->GetArrayField(TEXT("players"))[1]->AsObject()->SetBoolField(TEXT("ai"), true);
	L.ApplyLobby(*O); TestTrue(TEXT("reference keeps AI, public, sandbox, Endless combination"), L.StartAllowed());
	for (const TCHAR* Type : { TEXT("addAi"), TEXT("handAi"), TEXT("spectate"), TEXT("sit"), TEXT("fps"), TEXT("start"), TEXT("restart") })
	{
		const auto C = MakeShared<FJsonObject>(); C->SetStringField(TEXT("t"), Type);
		TestEqual(TEXT("reference commands untouched"), L.RustCommandRefusal(*C), FString());
	}
	O->SetStringField(TEXT("mode"), TEXT("assault")); O->SetNumberField(TEXT("defenderTeam"), 0); L.ApplyLobby(*O);
	TestTrue(TEXT("reference Assault unaffected"), L.StartAllowed());
	O->SetStringField(TEXT("mode"), TEXT("world")); O->SetNumberField(TEXT("spawns"), 0); L.ApplyLobby(*O);
	TestTrue(TEXT("reference World unaffected"), L.StartAllowed());
	return true;
}
#endif
