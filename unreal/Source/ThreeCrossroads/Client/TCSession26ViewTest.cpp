// Calls the production controller restore and group filtering against actual decoded identity changes.
#include "Client/TCPersistence.h"
#include "Client/TCPlayerController.h"
#include "Net/TCMatchState.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Engine/World.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession26ViewIdentityTest, "ThreeCrossroads.Session.ViewIdentityChanges", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession26ViewIdentityTest::RunTest(const FString&)
{
	auto Parse = [](const FString& Text)
	{
		TSharedPtr<FJsonObject> Object; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Object); return Object;
	};
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false);
	if (!TestNotNull(TEXT("isolated restore world"), World)) return false;
	ATCPlayerController* PC = World->SpawnActor<ATCPlayerController>();
	if (!TestNotNull(TEXT("production controller"), PC)) { World->DestroyWorld(false); return false; }
	FTCMatchState S;
	const auto Lobby = Parse(TEXT(R"({"t":"lobby","state":"play","code":"view26","mode":"conquest","amHost":true,"you":0,"spectator":false,"players":[{"name":"Alder","team":0},{"name":"Fern","team":1}]})"));
	const auto Start = Parse(TEXT(R"({"t":"start","matchId":26,"you":0,"map":{"name":"default","w":2,"h":2,"rows":["..",".."]},"names":["Alder","Fern"],"teams":[0,1]})"));
	S.ApplyLobby(*Lobby);
	if (!TestTrue(TEXT("decoded start establishes this match and seat"), S.ApplyStart(*Start) && S.SeatConfirmed())) { World->DestroyWorld(false); return false; }
	auto RecipientUnits = [&]()
	{
		S.Snapshots = 1;
		for (int32 Id : {4, 5, 6, 7})
		{
			FTCUnit U; U.Id = Id; U.Type = TEXT("rifle"); U.Owner = Id == 6 ? 1 : 0; U.Hp = Id == 7 ? 0 : 1;
			S.Units.Add(Id, U);
		}
	};
	RecipientUnits();
	FTCMatchView V; V.MatchId = 26; V.X = 123; V.Z = 234; V.Yaw = 0.2; V.Dist = 70;
	V.Selected = {4, 5, 6, 7}; V.Groups.Add(3, {5, 4, 6, 7});
	PC->RestoreMatchView(V, S);
	TestEqual(TEXT("matching identity restores camera"), PC->ViewFocus(), FVector2D(123, 234));
	TestEqual(TEXT("matching identity restores distance"), PC->ViewDistance(), 70.f);
	TestTrue(TEXT("selection keeps only own live recipient units"), PC->Selected.Num() == 2 && PC->Selected.Contains(4) && PC->Selected.Contains(5));
	TestTrue(TEXT("groups keep original order while rejecting foreign and dead units"), PC->Groups.Members(3) == TArray<int32>{5, 4});
	S.OnSocketOpened();
	TestTrue(TEXT("same-match same-seat confirmation survives ordinary reconnect"), S.ApplyStart(*Start) && S.SeatConfirmed());
	RecipientUnits(); PC->RestoreMatchView(V, S);
	TestTrue(TEXT("same identity can restore after fresh recipient state"), PC->ViewFocus() == FVector2D(123, 234) && PC->Groups.Members(3) == TArray<int32>{5, 4});
	Start->SetNumberField(TEXT("matchId"), 27);
	TestTrue(TEXT("healthy restart or loaded match changes actual decoded identity"), S.ApplyStart(*Start));
	PC->Groups.Sync(S); PC->Selected.Reset(); PC->FocusOn(FVector2D(9, 8)); PC->ZoomTo(55);
	PC->RestoreMatchView(V, S);
	TestTrue(TEXT("old view cannot move camera, select or restore groups into new identity"), PC->ViewFocus() == FVector2D(9, 8) && PC->ViewDistance() == 55 && PC->Selected.IsEmpty() && PC->Groups.Members(3).IsEmpty());
	// A new seat has no ownership of the former seat's ids, even if a stale view names this identity.
	Lobby->SetNumberField(TEXT("you"), 1); S.ApplyLobby(*Lobby); Start->SetNumberField(TEXT("you"), 1);
	TestTrue(TEXT("different seat starts under its own decoded authority"), S.ApplyStart(*Start) && S.SeatConfirmed());
	RecipientUnits(); V.MatchId = 27; V.Selected = {4, 5}; V.Groups[3] = {5, 4};
	PC->RestoreMatchView(V, S);
	TestTrue(TEXT("former-seat ids are rejected for selection and ordered groups"), PC->Selected.IsEmpty() && PC->Groups.Members(3).IsEmpty());
	World->DestroyWorld(false);
	return true;
}
#endif
