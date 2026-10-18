#include "Client/TCRustQueries.h"
#include "Net/TCMatchState.h"
#include "Net/TCRustViewShadow.h"
#include "Async/Async.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Misc/AutomationTest.h"

#if WITH_DEV_AUTOMATION_TESTS
namespace TCRustQueryTest
{
	constexpr auto Flags = EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter;
	FString Map(int32 Size)
	{
		TArray<FString> Rows;
		for (int32 Y = 0; Y < Size; Y++) Rows.Add(TEXT("\"") + FString::ChrN(Size, '.') + TEXT("\""));
		return FString::Printf(TEXT("{\"name\":\"ABI fixture\",\"w\":%d,\"h\":%d,\"rows\":[%s],\"spawns\":[{\"x\":4,\"y\":4},{\"x\":%d,\"y\":%d}],\"points\":[{\"x\":8,\"y\":8}]}"),
			Size, Size, *FString::Join(Rows, TEXT(",")), Size - 5, Size - 5);
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRustPlacementResultTest, "ThreeCrossroads.RustQueries.PlacementResult", TCRustQueryTest::Flags)
bool FTCRustPlacementResultTest::RunTest(const FString&)
{
	FTCRustPlacement Out;
	for (ETCRustPlacementReason Reason : { ETCRustPlacementReason::Blocked, ETCRustPlacementReason::NotVisible,
		ETCRustPlacementReason::Territory, ETCRustPlacementReason::Rail, ETCRustPlacementReason::Coast })
	{
		const FString Json = FString::Printf(TEXT("{\"ok\":false,\"reason\":\"%s\",\"x\":9,\"z\":9}"), FTCRustQueries::ReasonCode(Reason));
		TestTrue(TEXT("closed reason accepted"), FTCRustQueries::DecodePlacement(Json, Out).IsOk());
		TestTrue(TEXT("reason retained"), Out.Reason == Reason);
	}
	Out.At = { 77, 88 };
	TestEqual(TEXT("new reason requires review"), FTCRustQueries::DecodePlacement(TEXT(R"({"ok":false,"reason":"future","x":1,"z":2})"), Out).Code, FWW2Core::ErrContract);
	TestEqual(TEXT("failed decode leaves output"), Out.At.X, 77.0);
	TestEqual(TEXT("successful result must omit reason"), FTCRustQueries::DecodePlacement(TEXT(R"({"ok":true,"reason":"blocked","x":1,"z":2})"), Out).Code, FWW2Core::ErrContract);
	TestTrue(TEXT("fort null and letter preserved"), FTCRustQueries::DecodePlacement(TEXT(R"({"ok":true,"x":9,"z":9,"cells":[[51,null],[52,"T"]]})"), Out).IsOk());
	if (TestEqual(TEXT("two ordered pairs"), Out.Cells.Num(), 2))
	{
		TestTrue(TEXT("explicit null character"), Out.Cells[0].bFort && !Out.Cells[0].Character.IsSet());
		TestEqual(TEXT("second cell"), Out.Cells[1].Cell, 52);
		TestEqual(TEXT("source character"), Out.Cells[1].Character.Get(FString()), FString(TEXT("T")));
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRustRecipientPlacementTest, "ThreeCrossroads.RustQueries.RecipientPlacement", TCRustQueryTest::Flags)
bool FTCRustRecipientPlacementTest::RunTest(const FString&)
{
	FTCMatchState State;
	FTCRustViewShadow Rust;
	FTCRustPlacement Result;
	TestEqual(TEXT("no recipient has an explicit fallback"), FTCRustQueries::Placement(State, TEXT("barracks"), {9,9}, {}, 0, Result).Code, FWW2Core::ErrWrongState);
	if (!FWW2Core::IsReady()) { AddInfo(FWW2Core::Describe()); return true; }
	if (!TestTrue(TEXT("real Rust recipient attached"), Rust.BeginMode(ETCDecoder::Rust, &State, true, false))) return false;
	auto Receive = [&](const FString& Text)
	{
		TSharedPtr<FJsonObject> M;
		if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), M)) return false;
		Rust.Feed(Text);
		if (M->GetStringField(TEXT("t")) == TEXT("start"))
		{
			const bool bOk = State.ApplyStart(*M); Rust.AfterStart(State, bOk, false, 1); return bOk;
		}
		const bool bDrift = State.ApplySnapshot(*M); Rust.AfterSnapshot(State, bDrift); return !bDrift;
	};
	TestTrue(TEXT("start received"), Receive(TEXT("{\"t\":\"start\",\"you\":0,\"teams\":[0,1],\"map\":") + TCRustQueryTest::Map(16) + TEXT("}")));
	TestTrue(TEXT("own observer received"), Receive(TEXT(R"({"t":"s","tick":1,"all":true,"held":[1,1],"units":[[1,"rifle",0,1,1,0,0,100,0,0,0,0,0,0,1]]})")));
	if (!TestTrue(TEXT("actual capacity/fill through adopted view"), FTCRustQueries::Placement(State, TEXT("barracks"), {9,9}, {}, 0, Result).IsOk())) return false;
	TestTrue(TEXT("source fixture is buildable"), Result.bOk);
	TestEqual(TEXT("source top-left cell"), Result.Cell.Get(-1), 51);
	TestEqual(TEXT("source footprint"), Result.Cells.Num(), 9);
	TestTrue(TEXT("delivered enemy enters footprint"), Receive(TEXT(R"({"t":"s","tick":2,"held":[2,3],"units":[[2,"rifle",1,9,9,0,0,100,0,0,0,0,0,0,1]]})")));
	TestTrue(TEXT("query follows received delta"), FTCRustQueries::Placement(State, TEXT("barracks"), {9,9}, {}, 0, Result).IsOk());
	TestTrue(TEXT("occupancy blocks"), !Result.bOk && Result.Reason == ETCRustPlacementReason::Blocked);
	const int32 Code = Async(EAsyncExecution::Thread, [&]()
	{
		FTCRustPlacement Other;
		return FTCRustQueries::Placement(State, TEXT("barracks"), {9,9}, {}, 0, Other).Code;
	}).Get();
	TestEqual(TEXT("recipient query cannot cross owner thread"), Code, FWW2Core::ErrWrongThread);
	State.Map.bWorld = true;
	TestEqual(TEXT("World retains remembered-terrain fallback"), FTCRustQueries::Placement(State, TEXT("barracks"), {9,9}, {}, 0, Result).Code, FWW2Core::ErrBadArgument);
	State.Map.bWorld = false;
	Rust.End();
	TestEqual(TEXT("end detaches query owner"), FTCRustQueries::Placement(State, TEXT("barracks"), {9,9}, {}, 0, Result).Code, FWW2Core::ErrWrongState);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRustAuthoringTest, "ThreeCrossroads.RustQueries.Authoring", TCRustQueryTest::Flags)
bool FTCRustAuthoringTest::RunTest(const FString&)
{
	if (!FWW2Core::IsReady())
	{
		FString Out = TEXT("keep");
		TestEqual(TEXT("unlinked authoring fallback"), FTCRustQueries::AuthoringJson(EWW2AuthoringQuery::HouseKinds, TEXT("{}"), Out).Code, FWW2Core::ErrNotLinked);
		TestEqual(TEXT("fallback keeps output"), Out, FString(TEXT("keep")));
		return true;
	}
	const FString Map = TCRustQueryTest::Map(32);
	TSharedPtr<FJsonValue> Answer;
	TestTrue(TEXT("validate actual authored document"), FTCRustQueries::Authoring(EWW2AuthoringQuery::ValidateMap, Map, Answer).IsOk());
	TestTrue(TEXT("validation success is JSON null"), Answer.IsValid() && Answer->IsNull());
	TestTrue(TEXT("scenario migration"), FTCRustQueries::Authoring(EWW2AuthoringQuery::MigrateScenario, Map, Answer).IsOk());
	TestTrue(TEXT("map migration"), FTCRustQueries::Authoring(EWW2AuthoringQuery::MigrateMap, Map, Answer).IsOk());
	TestTrue(TEXT("spawn filter"), FTCRustQueries::Authoring(EWW2AuthoringQuery::SpawnsFor, TEXT("{\"map\":") + Map + TEXT(",\"mode\":\"conquest\"}"), Answer).IsOk());
	if (TestTrue(TEXT("spawn array"), Answer.IsValid() && Answer->Type == EJson::Array)) TestEqual(TEXT("two usable spawns"), Answer->AsArray().Num(), 2);
	TestTrue(TEXT("preview returns JSON, no handle"), FTCRustQueries::Authoring(EWW2AuthoringQuery::CreatePreview, TEXT("{\"map\":") + Map + TEXT(",\"seed\":19,\"options\":{\"players\":2}}"), Answer).IsOk());
	TestTrue(TEXT("preview is a setup object"), Answer.IsValid() && Answer->Type == EJson::Object);
	TestTrue(TEXT("authored path"), FTCRustQueries::Authoring(EWW2AuthoringQuery::FindPath, TEXT("{\"map\":") + Map + TEXT(",\"from\":{\"x\":9,\"z\":9},\"to\":{\"x\":33,\"z\":33}}"), Answer).IsOk());
	TestTrue(TEXT("house metadata"), FTCRustQueries::Authoring(EWW2AuthoringQuery::HouseKinds, TEXT("{}"), Answer).IsOk());
	FString Out = TEXT("keep");
	TestEqual(TEXT("zero-tick simulation refused on game thread before capacity probe"), FTCRustQueries::PlayMatch(Map, TEXT("{\"players\":2}"), 77, 0, Out).Code, FWW2Core::ErrWrongThread);
	TestEqual(TEXT("game-thread refusal keeps output"), Out, FString(TEXT("keep")));
	TestEqual(TEXT("negative limit is refused, not clamped"), FTCRustQueries::PlayMatch(Map, TEXT("{}"), 77, -1, Out).Code, FWW2Core::ErrBadArgument);
	TestEqual(TEXT("over-ceiling limit is refused, not clamped"), FTCRustQueries::PlayMatch(Map, TEXT("{}"), 77, 36001, Out).Code, FWW2Core::ErrBadArgument);
	const FWW2Result R = Async(EAsyncExecution::Thread, [&]() { return FTCRustQueries::PlayMatch(Map, TEXT("{\"players\":2}"), MAX_uint64, 0, Out); }).Get();
	TestTrue(TEXT("worker constructs and reports zero-tick setup"), R.IsOk());
	TestTrue(TEXT("u64 seed retains all digits"), Out.Contains(TEXT("18446744073709551615")));
	if (FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Out), Answer) && Answer.IsValid() && Answer->Type == EJson::Object)
	{
		TestEqual(TEXT("zero means no tick"), Answer->AsObject()->GetIntegerField(TEXT("ticks")), 0);
		TestTrue(TEXT("zero limit reports timeout"), Answer->AsObject()->GetBoolField(TEXT("timedOut")));
	}
	else AddError(TEXT("worker result is not a JSON object"));
	return true;
}
#endif
