// Held-view regression through the production receiveStart and recipient-ready restore paths. No socket or tick loop.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCSession28Diagnostics.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Engine/World.h"
#include "HAL/FileManager.h"
#include "Misc/CommandLine.h"
#include "Misc/FileHelper.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCSession28Failure
{
	// Lead-only test-access patch declares this automation friend. It never opens a transport.
	struct FAccess
	{
		static void Start(ATCGameMode& Match, const FJsonObject& Message) { Match.ControlsMessage(TEXT("start"), Message); }
		static void Frame(ATCGameMode& Match)
		{
			Match.ServerUrl = TEXT("invalid-session28-url");
			// Keep the periodic writer out of this deterministic restore-only step. Explicit SaveMatchView runs below.
			Match.Controls.LastViewWrite = TNumericLimits<double>::Max();
			Match.ControlsFrame();
		}
	};

	TSharedPtr<FJsonObject> Parse(const TCHAR* Text)
	{
		TSharedPtr<FJsonObject> Object;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Object);
		return Object;
	}

	TSharedPtr<FJsonObject> Start(int32 MatchId = 28)
	{
		auto Object = Parse(TEXT(R"({"t":"start","matchId":28,"you":0,"map":{"name":"default","w":2,"h":2,"rows":["..",".."]},"names":["Alder","Fern"],"teams":[0,1]})"));
		Object->SetNumberField(TEXT("matchId"), MatchId);
		return Object;
	}

	struct FScene
	{
		UWorld* World = UWorld::CreateWorld(EWorldType::Game, false);
		ATCGameMode* Match = World ? World->SpawnActor<ATCGameMode>() : nullptr;
		ATCPlayerController* Controller = World ? World->SpawnActor<ATCPlayerController>() : nullptr;
		FScene() { if (World && Controller) World->AddController(Controller); }
		~FScene() { if (World) World->DestroyWorld(false); }
		bool Ready(FAutomationTestBase& Test) const
		{
			return Test.TestNotNull(TEXT("isolated view world"), World)
				&& Test.TestNotNull(TEXT("production GameMode"), Match)
				&& Test.TestNotNull(TEXT("production controller"), Controller)
				&& Test.TestTrue(TEXT("restore finds the held controller"), World->GetFirstPlayerController() == Controller);
		}
		void RecipientReady()
		{
			const auto Snapshot = Parse(TEXT(R"({"t":"s","tick":2,"all":true,"units":[[4,"rifle",0,1,1,0,0,1,0,0,0,0,0,0,1],[5,"rifle",0,2,2,0,0,1,0,0,0,0,0,0,1]],"held":[2,1]})"));
			Match->State.ApplySnapshot(*Snapshot);
		}
		void Playing()
		{
			const auto Lobby = Parse(TEXT(R"({"t":"lobby","code":"room28a","state":"play","you":0,"amHost":true,"spectator":false,"role":"commander","mode":"conquest","players":[{"name":"Alder","team":0,"connected":true},{"name":"Fern","team":1,"connected":true}]})"));
			Match->State.ApplyLobby(*Lobby); Match->Controls.ApplyLobby(*Lobby);
			Match->State.ApplyStart(*Start()); RecipientReady();
			Controller->FocusOn(FVector2D(123, 234)); Controller->ZoomTo(70); Controller->SetViewYawForChecks(-37.25f);
			Controller->Selected = {4};
			TMap<int32, TArray<int32>> Groups; Groups.Add(3, {5, 4}); Groups.Add(8, {4, 5});
			Controller->Groups.Restore(Groups, Match->State);
			Match->Controls.bViewSeatActive = true;
		}
	};
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession28HeldFailureTest, "ThreeCrossroads.Session28.HeldFailedWriteContinuation", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession28HeldFailureTest::RunTest(const FString&)
{
	using namespace TCSession28Failure;
	TCSession28Failure::FScene Scene; if (!Scene.Ready(*this)) return false;
	Scene.Playing(); ATCGameMode& M = *Scene.Match; ATCPlayerController& PC = *Scene.Controller;
	FString Directory;
	FParse::Value(FCommandLine::Get(), TEXT("TCSession28Scratch="), Directory);
	if (Directory.IsEmpty()) Directory = FPaths::Combine(FPaths::AutomationTransientDir(), TEXT("Session28"));
	IFileManager::Get().MakeDirectory(*Directory, true);
	const FString Blocker = FPaths::Combine(Directory, TEXT("held-view-parent"));
	if (!TestTrue(TEXT("blocked parent is a regular file, independent of permissions"), FFileHelper::SaveStringToFile(TEXT("session28"), *Blocker))) return false;
	M.Controls.ViewMemory.Open(Blocker, TEXT("ws://source.invalid/ws"), TEXT("room28a"), TEXT("test-only-room28a"));
	const FVector2D HeldFocus = PC.ViewFocus(); const float HeldYaw = PC.ViewYaw(), HeldDistance = PC.ViewDistance();
	const TArray<int32> Group3 = PC.Groups.Members(3), Group8 = PC.Groups.Members(8);
	auto Held = [&]()
	{
		return PC.ViewFocus() == HeldFocus && PC.ViewYaw() == HeldYaw && PC.ViewDistance() == HeldDistance
			&& PC.Selected.Num() == 1 && PC.Selected.Contains(4) && PC.Groups.Members(3) == Group3 && PC.Groups.Members(8) == Group8;
	};
	M.SaveMatchView();
	FTCMatchView Unreadable;
	TestTrue(TEXT("failed production write cannot change the held input state"), Held());
	TestFalse(TEXT("blocked write leaves no readable fallback"), M.Controls.ViewMemory.Read(Unreadable));
	M.State.OnSocketOpened();
	FAccess::Start(M, *Start());
	TestTrue(TEXT("same identity prepares current view despite failed persistence"), M.Controls.bResumePending);
	TestTrue(TEXT("pending view contains selection and distinct group orders"), M.Controls.ResumeView.Selected == TArray<int32>{4}
		&& M.Controls.ResumeView.Groups.FindRef(3) == Group3 && M.Controls.ResumeView.Groups.FindRef(8) == Group8);
	if (!TestTrue(TEXT("same start passes the actual state parser"), M.State.ApplyStart(*Start()))) { IFileManager::Get().Delete(*Blocker, false, true); return false; }
	// No player ticks, camera schedule or scripted selection changes occur between these exact boundaries.
	TestEqual(TEXT("start waits for fresh recipient state"), M.State.Snapshots, 0);
	FAccess::Frame(M);
	TestTrue(TEXT("no restore before its first recipient snapshot"), M.Controls.bResumePending && Held());
	Scene.RecipientReady();
	TestTrue(TEXT("actual recipient parser supplies the first complete own-unit snapshot"), M.State.Snapshots == 1 && M.State.Units.Num() == 2 && M.State.SeatConfirmed());
	FAccess::Frame(M);
	TestFalse(TEXT("recipient-ready production path consumes the pending view"), M.Controls.bResumePending);
	TestTrue(TEXT("camera, selection and both ordered groups equal the exact pre-start state"), Held());
	FAccess::Start(M, *Start(29));
	TestFalse(TEXT("healthy new identity cannot resume the blocked old identity"), M.Controls.bResumePending);
	TestTrue(TEXT("observing a different identity has no camera or selection side effect"), Held());
	IFileManager::Get().Delete(*Blocker, false, true);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession28SeamObservationTest, "ThreeCrossroads.Session28.SeamObservation", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession28SeamObservationTest::RunTest(const FString&)
{
	TCSession28Failure::FScene Scene; if (!Scene.Ready(*this)) return false;
	Scene.Playing(); ATCGameMode& M = *Scene.Match; ATCPlayerController& PC = *Scene.Controller;
	const auto Before = TCSession28::ViewSeamState(M, PC, TEXT("write"), 28, false);
	TestFalse(TEXT("real failed-write result is explicit in the local observation"), Before->GetBoolField(TEXT("writeSucceeded")));
	TestEqual(TEXT("incoming identity is explicit"), Before->GetIntegerField(TEXT("incomingMatchId")), 28);
	TestEqual(TEXT("native camera yaw is observed without formatting loss"), Before->GetObjectField(TEXT("camera"))->GetNumberField(TEXT("yaw")), double(PC.ViewYaw()));
	const auto Groups = Before->GetObjectField(TEXT("groups"));
	const auto& Third = Groups->GetArrayField(TEXT("3"));
	TestTrue(TEXT("diagnostic retains group member order"), Third.Num() == 2 && Third[0]->AsNumber() == 5 && Third[1]->AsNumber() == 4);
	TestTrue(TEXT("view observation does not alter authority or selection"), M.State.CanOrder() && PC.Selected.Num() == 1 && PC.Selected.Contains(4));
	TestTrue(TEXT("local-only probe has no raw token, name, server or path field"), !Before->HasField(TEXT("token")) && !Before->HasField(TEXT("name")) && !Before->HasField(TEXT("server")) && !Before->HasField(TEXT("path")));
	return true;
}
#endif
