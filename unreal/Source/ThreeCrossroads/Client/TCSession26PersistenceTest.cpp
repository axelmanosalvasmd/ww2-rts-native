// Numeric server timestamps and reference load ordering. Locale expansion is intentionally not claimed here.
#include "Client/TCPersistence.h"
#include "Net/TCMatchState.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "HAL/PlatformMisc.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCSession26Persistence
{
	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> Object;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Object);
		return Object;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession26TimezoneTest, "ThreeCrossroads.Session.SaveTimestampZones", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession26TimezoneTest::RunTest(const FString&)
{
	// Exact source new Date(savedAt).toLocaleString([], options), en-US viewer, TZ per row.
	struct FCase { int64 Ms; int32 OffsetMinutes; const TCHAR* Zone; const TCHAR* Text; };
	const FCase Cases[] = {
		{1772953199999LL, -300, TEXT("America/New_York"), TEXT("Mar 8, 01:59 AM")},
		{1772953200000LL, -240, TEXT("America/New_York"), TEXT("Mar 8, 03:00 AM")},
		{1793512799999LL, -240, TEXT("America/New_York"), TEXT("Nov 1, 01:59 AM")},
		{1793512800000LL, -300, TEXT("America/New_York"), TEXT("Nov 1, 01:00 AM")},
		{1767225600000LL, 345, TEXT("Asia/Kathmandu"), TEXT("Jan 1, 05:45 AM")},
		{1767262500000LL, 840, TEXT("Pacific/Kiritimati"), TEXT("Jan 2, 12:15 AM")},
		{1767226500000LL, -660, TEXT("Pacific/Pago_Pago"), TEXT("Dec 31, 01:15 PM")},
		{-1, 0, TEXT("UTC"), TEXT("Dec 31, 11:59 PM")},
	};
	const FString ProcessZone = FPlatformMisc::GetEnvironmentVariable(TEXT("TZ"));
	for (const FCase& C : Cases)
	{
		const FTimespan Offset = FTimespan::FromMinutes(C.OffsetMinutes);
		TestEqual(TEXT("saved instant uses its own offset, including DST and day rollover"), FTCPersistence::SavedAtText(C.Ms, Offset), FString(C.Text));
		const auto O = MakeShared<FJsonObject>();
		O->SetArrayField(TEXT("names"), { MakeShared<FJsonValueString>(TEXT("Alder")) });
		O->SetStringField(TEXT("mode"), TEXT("conquest")); O->SetNumberField(TEXT("minutes"), 7);
		O->SetStringField(TEXT("kind"), TEXT("manual")); O->SetNumberField(TEXT("savedAt"), (double)C.Ms);
		int32 OffsetReads = 0;
		FTCSaveRow Row;
		TestTrue(TEXT("numeric source timestamp reads"), FTCPersistence::ReadRow(MakeShared<FJsonValueObject>(O), [&](int64 Received)
		{
			++OffsetReads; TestEqual(TEXT("zone lookup receives original epoch milliseconds"), Received, C.Ms); return Offset;
		}, Row));
		TestTrue(TEXT("row preserves exact saved instant and performs one offset lookup"), Row.bHasSavedAt && Row.SavedAtMs == C.Ms && OffsetReads == 1);
		TestEqual(TEXT("rendered save detail uses that timestamp"), Row.Detail, FString(TEXT("Alder · saved ")) + C.Text);
		// Lead repeats this case in fresh processes with the listed TZ. No process/global zone is changed by the test.
		if (ProcessZone == C.Zone)
		{
			TestEqual(TEXT("actual process timezone resolves offset at the saved instant"), FTCPersistence::LocalUtcOffsetAt(C.Ms), Offset);
			TestEqual(TEXT("actual process timezone renders the source fixture"), FTCPersistence::SavedAtText(C.Ms, FTCPersistence::LocalUtcOffsetAt(C.Ms)), FString(C.Text));
		}
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession26SaveLoadTest, "ThreeCrossroads.Session.SaveLoadOrdering", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession26SaveLoadTest::RunTest(const FString&)
{
	using namespace TCSession26Persistence;
	const FString Id = TEXT("save26-1767225600000-manual");
	const auto Lobby = Parse(TEXT(R"({"t":"lobby","code":"save26","state":"lobby","you":0,"amHost":true,"spectator":false,"mode":"conquest","players":[{"name":"Alder","connected":true,"team":0},{"name":"Fern","connected":true,"team":1}],"saves":[{"id":"save26-1767225600000-manual","kind":"manual","savedAt":1767225600000,"mode":"conquest","minutes":2,"names":["Alder","Fern"]}]})"));
	const auto Start = Parse(TEXT(R"({"t":"start","matchId":27,"you":0,"map":{"name":"default","w":2,"h":2,"rows":["..",".."]},"names":["Alder","Fern"],"teams":[0,1]})"));
	const auto Off = Parse(TEXT(R"({"t":"pause","paused":false})"));
	const auto On = Parse(TEXT(R"({"t":"pause","paused":true,"by":"Alder","reason":"host"})"));
	if (!TestTrue(TEXT("reference message fixtures parse"), Lobby && Start && Off && On)) return false;
	FTCMatchState S; FTCPersistence P;
	S.ApplyLobby(*Lobby); P.ApplyLobby(*Lobby);
	TestTrue(TEXT("reference host can inspect and load the listed save"), P.SaveList(S).bShown && P.SaveList(S).bLoadButtons);
	S.bAmHost = false;
	TestFalse(TEXT("guest cannot create a load request"), P.PlanLoad(S, Id).Message.IsValid());
	S.bAmHost = true;
	const FTCPersistencePlan Load = P.PlanLoad(S, Id);
	if (!TestTrue(TEXT("listed same-room load builds the actual wire command"), Load.Message.IsValid())) return false;
	TestEqual(TEXT("load command"), Load.Message->GetStringField(TEXT("t")), FString(TEXT("loadSave")));
	TestEqual(TEXT("load id"), Load.Message->GetStringField(TEXT("id")), Id);
	TestTrue(TEXT("valid source load expects a new match"), Load.Expect == ETCPersistenceExpect::Loaded);
	P.NoteSent(Load, true, 10);
	// server.js loadRoom/beginMatch/sendStart: pause off, lobby play, pause on, start, pause on.
	S.ApplyPause(*Off); Lobby->SetStringField(TEXT("state"), TEXT("play")); S.ApplyLobby(*Lobby); P.ApplyLobby(*Lobby);
	S.ApplyPause(*On);
	TestTrue(TEXT("source load is paused before its start"), S.bPaused);
	TestTrue(TEXT("load stays pending until a valid start"), P.Status().Outcome == ETCPersistenceOutcome::Pending);
	if (!TestTrue(TEXT("authoritative loaded start accepted"), S.ApplyStart(*Start))) return false;
	P.ApplyStart(*Start, 11);
	TestFalse(TEXT("start clears stale pause before the source resends it"), S.bPaused);
	S.ApplyPause(*On);
	TestTrue(TEXT("loaded match waits for host resume"), S.bPaused && !S.CanOrder());
	TestTrue(TEXT("load receipt records authoritative new identity"), P.Status().Outcome == ETCPersistenceOutcome::Loaded && P.Status().MatchId == 27);
	const FTCPersistencePlan Save = P.PlanSave(S);
	TestTrue(TEXT("paused host still saves and closes its menu"), Save.Message.IsValid() && Save.bCloseMenu && Save.Expect == ETCPersistenceExpect::Saved);
	P.NoteSent(Save, true, 12);
	const auto Saved = Parse(TEXT(R"({"t":"saved","id":"save26-1767225600001-manual"})"));
	P.ApplySaved(*Saved, S, 13);
	TestTrue(TEXT("save completion comes from saved acknowledgement"), P.Status().Outcome == ETCPersistenceOutcome::Saved && P.Status().Id == TEXT("save26-1767225600001-manual"));
	const auto Feedback = P.TakeFeedback();
	TestTrue(TEXT("source success sentence retains the source feedback blip"), Feedback.Num() == 1 && Feedback[0].bErrorSound && Feedback[0].bVisible && Feedback[0].Sentence == FTCPersistence::SavedSentence());
	S.ApplyPause(*Off); TestTrue(TEXT("host resume restores ordinary command eligibility"), S.CanOrder());
	// An old socket's pending request is not credited to an unrelated reconnect start.
	P.NoteSent(Load, true, 14); P.OnSocketOpened(15); P.ApplyStart(*Start, 16);
	TestTrue(TEXT("socket change leaves the old load unanswered"), P.Status().Outcome == ETCPersistenceOutcome::Unanswered);
	P.NoteSent(Save, true, 17);
	P.ApplyDeny(*Parse(TEXT(R"({"t":"deny","cmd":"move","reason":"blocked"})")), 18);
	TestTrue(TEXT("unrelated denial cannot answer a save"), P.Status().Outcome == ETCPersistenceOutcome::Pending);
	P.ApplyDeny(*Parse(TEXT(R"({"t":"deny","cmd":"save","reason":"save"})")), 19);
	TestTrue(TEXT("authoritative write refusal answers the save"), P.Status().Outcome == ETCPersistenceOutcome::Refused && P.Status().DenyReason == TEXT("save"));
	return true;
}
#endif
