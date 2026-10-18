#include "WW2Core.h"
#include "Misc/AutomationTest.h"

#if WITH_WW2_FFI
#include "ww2_core.h"
#endif

#if WITH_DEV_AUTOMATION_TESTS
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FWW2QueryBuffersTest, "ThreeCrossroads.WW2Core.QueryBuffers",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FWW2QueryBuffersTest::RunTest(const FString&)
{
	TestFalse(TEXT("previous major rejected"), FWW2Core::AcceptsAbi(1, 99));
	TestFalse(TEXT("previous ABI 2.0 rejected"), FWW2Core::AcceptsAbi(2, 0));
	TestTrue(TEXT("ABI 2.1 accepted"), FWW2Core::AcceptsAbi(2, 1));
	TestTrue(TEXT("later additive minor accepted"), FWW2Core::AcceptsAbi(2, 2));
	TestFalse(TEXT("future breaking major rejected"), FWW2Core::AcceptsAbi(3, 1));
	TArray<UTF8CHAR> Bytes = { (UTF8CHAR)'x' };
#if WITH_WW2_FFI
	if (!TestTrue(TEXT("bridge ready"), FWW2Core::IsReady())) return false;
	using FQuery = int32_t (*)(const char*, size_t, char*, size_t, size_t*);
	const FQuery Calls[] = { ww2_authoring_validate_map, ww2_authoring_migrate_scenario, ww2_authoring_migrate_map,
		ww2_authoring_spawns_for, ww2_authoring_create_preview, ww2_authoring_find_path,
		ww2_authoring_house_kinds };
	// The blocking PlayMatch export is exercised on a worker by RustQueries.Authoring, including capacity discovery.
	for (FQuery Call : Calls)
	{
		char Buffer[8]; FMemory::Memset(Buffer, 'x', sizeof Buffer);
		size_t Count = 99;
		TestEqual(TEXT("null count refused"), Call("{", 1, Buffer, sizeof Buffer, nullptr), WW2_ERR_NULL_POINTER);
		TestEqual(TEXT("malformed JSON refused"), Call("{", 1, Buffer, sizeof Buffer, &Count), WW2_ERR_PARSE);
		TestEqual(TEXT("error reports zero bytes"), Count, (size_t)0);
		TestTrue(TEXT("failure writes no result bytes"), Buffer[0] == 'x' && Buffer[7] == 'x');
		TestFalse(TEXT("error is available on this thread"), FWW2Core::LastError().IsEmpty());
	}
	size_t Count = 0;
	TestEqual(TEXT("house metadata capacity probe"), ww2_authoring_house_kinds("{}", 2, nullptr, 0, &Count), WW2_ERR_CAPACITY);
	TestTrue(TEXT("capacity is bytes"), Count > 2);
	char Sentinel = 'x'; size_t Required = 0;
	TestEqual(TEXT("short buffer refused"), ww2_authoring_house_kinds("{}", 2, &Sentinel, 1, &Required), WW2_ERR_CAPACITY);
	TestEqual(TEXT("deterministic size"), Required, Count);
	TestEqual(TEXT("short output untouched"), Sentinel, 'x');
	const FTCHARToUTF8 Input(TEXT("{}"));
	TestEqual(TEXT("wrapper capacity and fill"), FWW2Core::AuthoringQuery(EWW2AuthoringQuery::HouseKinds,
		{ reinterpret_cast<const UTF8CHAR*>(Input.Get()), Input.Length() }, Bytes).Code, FWW2Core::Ok);
	TestEqual(TEXT("wrapper owns exactly the reported bytes"), (size_t)Bytes.Num(), Count);
	FWW2View View;
	TestEqual(TEXT("open"), View.Open().Code, FWW2Core::Ok);
	TestEqual(TEXT("empty recipient placement refused"), View.PlacementCheck({}, Bytes).Code, FWW2Core::ErrWrongState);
	TestEqual(TEXT("empty-recipient failure keeps output"), (size_t)Bytes.Num(), Count);
#else
	TestEqual(TEXT("query fallback without linked library"), FWW2Core::AuthoringQuery(EWW2AuthoringQuery::HouseKinds, {}, Bytes).Code, FWW2Core::ErrNotLinked);
	TestEqual(TEXT("fallback preserves output"), Bytes.Num(), 1);
#endif
	return true;
}
#endif
