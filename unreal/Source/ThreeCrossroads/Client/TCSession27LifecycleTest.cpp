// Original page navigation drops room authority and saves its final view. Same-match receiveStart uses live state.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Net/TCConnection.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Engine/World.h"
#include "HAL/FileManager.h"
#include "Misc/CommandLine.h"
#include "Misc/FileHelper.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCSession27Lifecycle
{
	enum class ENavigation { Room, Directory };

	// Declared as an automation-only friend by the lead's test-access.patch. No live sockets are started.
	struct FAccess
	{
		static bool Enter(ATCGameMode& Match, const FTCRoomEntry& Entry)
		{
			Match.BaseSettings.Url = TEXT("invalid-session27-url");
			return Match.ControlsEnter(Entry, true);
		}
		static void Directory(ATCGameMode& Match)
		{
			// A stopped, socketless connection represents the page being left. Invalid URL blocks HTTP setup too.
			Match.Connection = MakeShared<FTCConnection>(FTCConnection::FSettings());
			Match.ServerUrl = TEXT("invalid-session27-url");
		}
		static void Start(ATCGameMode& Match, const FJsonObject& Message) { Match.ControlsMessage(TEXT("start"), Message); }
	};

	FString Scratch()
	{
		FString Directory;
		FParse::Value(FCommandLine::Get(), TEXT("TCSession27Scratch="), Directory);
		return Directory.IsEmpty() ? FPaths::Combine(FPaths::AutomationTransientDir(), TEXT("Session27")) : Directory;
	}

	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> Object;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Object);
		return Object;
	}

	TSharedPtr<FJsonObject> Lobby(const TCHAR* State = TEXT("lobby"))
	{
		auto Object = Parse(TEXT(R"({"t":"lobby","code":"room27a","state":"lobby","you":0,"amHost":true,"host":0,"spectator":false,"role":"commander","mode":"conquest","mapName":"default","maps":["default","hill-112"],"army":"standard","worldSize":"huge","weather":"map","sandbox":false,"spawns":3,"players":[{"name":"Alder","team":0,"faction":0,"connected":true,"ai":false},{"name":"Fern","team":1,"faction":1,"connected":true,"ai":false}],"saves":[{"id":"room27a-1767225600000-manual","kind":"manual","savedAt":1767225600000,"mode":"conquest","minutes":2,"names":["Alder","Fern"]}]})"));
		Object->SetStringField(TEXT("state"), State);
		return Object;
	}

	TSharedPtr<FJsonObject> Start(int32 MatchId = 27)
	{
		auto Object = Parse(TEXT(R"({"t":"start","matchId":27,"you":0,"map":{"name":"default","w":2,"h":2,"rows":["..",".."]},"names":["Alder","Fern"],"teams":[0,1]})"));
		Object->SetNumberField(TEXT("matchId"), MatchId);
		return Object;
	}

	struct FScene
	{
		UWorld* World = UWorld::CreateWorld(EWorldType::Game, false);
		ATCGameMode* Match = World ? World->SpawnActor<ATCGameMode>() : nullptr;
		ATCPlayerController* Controller = World ? World->SpawnActor<ATCPlayerController>() : nullptr;
		FScene()
		{
			// This isolated world has not initialized actors for play. Register without BeginPlay or network startup.
			if (World && Controller) World->AddController(Controller);
		}
		~FScene() { if (World) World->DestroyWorld(false); }
		bool Ready(FAutomationTestBase& Test) const
		{
			return Test.TestNotNull(TEXT("isolated lifecycle world"), World)
				&& Test.TestNotNull(TEXT("production GameMode"), Match)
				&& Test.TestNotNull(TEXT("production controller"), Controller)
				&& Test.TestTrue(TEXT("view hooks find the actual controller"), World->GetFirstPlayerController() == Controller);
		}
		void Playing()
		{
			Match->State.ApplyLobby(*Lobby(TEXT("play")));
			Match->State.ApplyStart(*Start());
			Match->State.Snapshots = 1;
			for (int32 Id : {4, 5})
			{
				FTCUnit Unit; Unit.Id = Id; Unit.Type = TEXT("rifle"); Unit.Owner = 0; Unit.Hp = 1;
				Match->State.Units.Add(Id, Unit);
			}
			Controller->FocusOn(FVector2D(123, 234)); Controller->ZoomTo(70);
			Controller->Selected = {4, 5};
			TMap<int32, TArray<int32>> Groups; Groups.Add(3, {5, 4});
			Controller->Groups.Restore(Groups, Match->State);
			Match->Controls.bViewSeatActive = true;
		}
	};

	void LeaveRoomWithoutTransport(FAutomationTestBase& Test, ATCGameMode& Match, ENavigation Navigation)
	{
		if (Navigation == ENavigation::Directory)
		{
			FAccess::Directory(Match);
			FTCUiCommand Command(ETCUiAction::MatchControl); Command.Unit = TEXT("Browse");
			Test.TestTrue(TEXT("Browse executes its actual directory navigation path"), Match.RunControl(Command, false));
			Test.TestTrue(TEXT("directory is shown with no HTTP transport setup"), Match.bRoomBrowserOpen && FTCRoomBrowser::RoomsUrl(Match.ServerUrl).IsEmpty());
			return;
		}
		// Stop at URL validation after the real navigation teardown. No socket, token or server is accessed.
		FTCRoomEntry Entry; Entry.bGo = true; Entry.Room = TEXT("room27b"); Entry.Name = TEXT("Alder");
		Entry.Note = TEXT("session27 navigation regression");
		Test.AddExpectedError(TEXT("not connecting: -TCServer must be a ws:// or wss:// URL"), EAutomationExpectedErrorFlags::Contains, 1);
		Test.TestFalse(TEXT("intentional invalid URL is setup refusal, not a transport outcome"), FAccess::Enter(Match, Entry));
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession27RoomAuthorityTest, "ThreeCrossroads.Session.CrossRoomAuthority", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession27RoomAuthorityTest::RunTest(const FString&)
{
	using namespace TCSession27Lifecycle;
	for (const ENavigation Navigation : {ENavigation::Room, ENavigation::Directory})
	{
		AddInfo(Navigation == ENavigation::Room ? TEXT("direct room navigation") : TEXT("directory navigation"));
		TCSession27Lifecycle::FScene Scene; if (!Scene.Ready(*this)) return false;
		ATCGameMode& M = *Scene.Match;
		const auto Old = Lobby();
		M.State.ApplyLobby(*Old); M.Controls.ApplyLobby(*Old); M.Lobby.ApplyLobby(*Old); M.Options.ApplyLobby(*Old); M.Persistence.ApplyLobby(*Old);
		const auto Army = M.Lobby.Plan(ETCLobbySetting::Army, -1, TEXT("large"));
		const auto Weather = M.Options.PlanWeather(TEXT("rain"));
		const auto Load = M.Persistence.PlanLoad(M.State, TEXT("room27a-1767225600000-manual"));
		if (!TestTrue(TEXT("old room grants actual host setting and save plans"), Army.Message && Weather.Message && Load.Message)) return false;
		M.Lobby.NoteSent(Army, 10); M.Options.NoteSent(Weather, 10); M.Persistence.NoteSent(Load, true, 10);
		TestTrue(TEXT("repro starts with pending mutations and a listed save"), M.Lobby.PendingCount() == 1 && M.Options.PendingCount() == 1 && M.Persistence.Rows().Num() == 1);
		LeaveRoomWithoutTransport(*this, M, Navigation);
		TestFalse(TEXT("new room has no previous lobby authority"), M.Lobby.HasLobby());
		TestFalse(TEXT("new room has no previous weather/sandbox authority"), M.Options.HasLobby());
		TestTrue(TEXT("old mutation confirmations cannot cross rooms"), M.Lobby.PendingCount() == 0 && M.Options.PendingCount() == 0);
		TestFalse(TEXT("waiting for a new lobby cannot plan old host army changes"), M.Lobby.Plan(ETCLobbySetting::Army, -1, TEXT("massive")).Message.IsValid());
		TestFalse(TEXT("waiting for a new lobby cannot plan old host weather changes"), M.Options.PlanWeather(TEXT("snow")).Message.IsValid());
		TestTrue(TEXT("old saves and request receipts cannot appear in the new room"), M.Persistence.Rows().IsEmpty() && M.Persistence.Status().Outcome == ETCPersistenceOutcome::None);
		// A new reference guest receives only its actual authority, after the intentional setup refusal above.
		const auto Guest = Lobby(); Guest->SetStringField(TEXT("code"), TEXT("room27b")); Guest->SetBoolField(TEXT("amHost"), false); Guest->SetNumberField(TEXT("you"), 1);
		M.State.ApplyLobby(*Guest); M.Lobby.ApplyLobby(*Guest); M.Options.ApplyLobby(*Guest);
		TestFalse(TEXT("new guest never inherits former host authority"), M.Lobby.Plan(ETCLobbySetting::Army, -1, TEXT("large")).Message.IsValid());
		TestTrue(TEXT("new guest retains original own-faction authority"), M.Lobby.Plan(ETCLobbySetting::Faction, 1, 2).Message.IsValid());
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession27FinalViewTest, "ThreeCrossroads.Session.CrossRoomFinalView", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession27FinalViewTest::RunTest(const FString&)
{
	using namespace TCSession27Lifecycle;
	for (const ENavigation Navigation : {ENavigation::Room, ENavigation::Directory})
	{
		AddInfo(Navigation == ENavigation::Room ? TEXT("direct room navigation") : TEXT("directory navigation"));
		TCSession27Lifecycle::FScene Scene; if (!Scene.Ready(*this)) return false;
		const FString Directory = FPaths::Combine(Scratch(), Navigation == ENavigation::Room ? TEXT("final-view-room") : TEXT("final-view-directory"));
		ATCGameMode& M = *Scene.Match; Scene.Playing();
		M.Controls.ViewMemory.Open(Directory, TEXT("ws://source.invalid/ws"), TEXT("room27a"), TEXT("test-only-room27a"));
		// The preceding periodic write is deliberately stale. Navigation must capture changes since it, as pagehide does.
		FTCMatchView Previous; Previous.MatchId = 27; Previous.X = 1; Previous.Z = 2; Previous.Dist = 45;
		if (!TestTrue(TEXT("writable control stores the preceding periodic view"), M.Controls.ViewMemory.Write(Previous))) return false;
		FTCMatchViewMemory Reader; Reader.Open(Directory, TEXT("ws://source.invalid/ws"), TEXT("room27a"), TEXT("test-only-room27a"));
		LeaveRoomWithoutTransport(*this, M, Navigation);
		FTCMatchView Saved;
		if (TestTrue(TEXT("old room final view remains readable after navigating"), Reader.Read(Saved)))
		{
			TestTrue(TEXT("navigation saves the final live camera, not the last periodic write"), Saved.X == 123 && Saved.Z == 234 && Saved.Dist == 70);
			TestTrue(TEXT("navigation saves selection and ordered groups before teardown"), Saved.Selected.Num() == 2 && Saved.Selected.Contains(4) && Saved.Selected.Contains(5) && Saved.Groups.FindRef(3) == TArray<int32>{5, 4});
		}
		TestTrue(TEXT("presentation drops old room selection and groups"), Scene.Controller->Selected.IsEmpty() && Scene.Controller->Groups.Members(3).IsEmpty());
		Reader.Clear(); IFileManager::Get().DeleteDirectory(*Directory, false, false);
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession27NoStorageViewTest, "ThreeCrossroads.Session.SameMatchWithoutStorage", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession27NoStorageViewTest::RunTest(const FString&)
{
	using namespace TCSession27Lifecycle;
	TCSession27Lifecycle::FScene Scene; if (!Scene.Ready(*this)) return false;
	Scene.Playing(); ATCGameMode& M = *Scene.Match;
	// A regular file cannot contain the view directory, independent of user privileges or chmod semantics.
	const FString Directory = Scratch(); IFileManager::Get().MakeDirectory(*Directory, true);
	const FString Blocker = FPaths::Combine(Directory, TEXT("blocked-view-parent"));
	if (!TestTrue(TEXT("controlled failed-write parent is a regular file"), FFileHelper::SaveStringToFile(TEXT("session27"), *Blocker))) return false;
	M.Controls.ViewMemory.Open(Blocker, TEXT("ws://source.invalid/ws"), TEXT("room27a"), TEXT("test-only-room27a"));
	TestTrue(TEXT("failed-write store is configured rather than silently skipped"), M.Controls.ViewMemory.IsOpen());
	TestFalse(TEXT("actual filesystem view write fails at the controlled parent"), M.Controls.ViewMemory.Write(FTCMatchView::Capture(*Scene.Controller, 27)));
	FTCMatchView Absent;
	TestFalse(TEXT("failed write leaves no readable view"), M.Controls.ViewMemory.Read(Absent));
	// Both original stores may throw: receiveStart still uses matchView() for this already-held identity.
	FAccess::Start(M, *Start());
	TestTrue(TEXT("same match resumes from current view even when storage is unavailable"), M.Controls.bResumePending);
	TestTrue(TEXT("same match preserves current camera and identity in memory"), M.Controls.ResumeView.MatchId == 27 && M.Controls.ResumeView.X == 123 && M.Controls.ResumeView.Z == 234 && M.Controls.ResumeView.Dist == 70);
	TestTrue(TEXT("same match preserves selection and ordered group members in memory"), M.Controls.ResumeView.Selected.Num() == 2 && M.Controls.ResumeView.Selected.Contains(4) && M.Controls.ResumeView.Selected.Contains(5) && M.Controls.ResumeView.Groups.FindRef(3) == TArray<int32>{5, 4});
	FAccess::Start(M, *Start(28));
	TestFalse(TEXT("a different match cannot reuse the live view without a matching saved identity"), M.Controls.bResumePending);
	IFileManager::Get().Delete(*Blocker, false, true);
	return true;
}
#endif
