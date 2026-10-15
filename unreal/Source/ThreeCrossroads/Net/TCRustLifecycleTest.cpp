// R1: an offline commander remains human while the host waits for the same seat to reconnect.
#include "Client/TCMatchControls.h"
#include "Client/TCLobbySettings.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

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
#endif
