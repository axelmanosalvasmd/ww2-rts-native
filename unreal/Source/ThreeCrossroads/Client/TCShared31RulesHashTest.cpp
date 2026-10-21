#include "Client/TCShared31RulesHash.h"

namespace
{
	template <typename Check> void HashCases(Check Test)
	{
		using namespace TCShared31;
		constexpr std::uint64_t Hash = 0xed0ee23d32a621d1ull;
		Test("absent reference hash is not a mismatch", CompareRulesHash(true, Hash, false, "") == EHashCheck::Absent);
		Test("matching full-width hash", CompareRulesHash(true, Hash, true, "ed0ee23d32a621d1") == EHashCheck::Match);
		Test("one-bit mismatch above double integer precision", CompareRulesHash(true, Hash, true, "ed0ee23d32a621d0") == EHashCheck::Mismatch);
		Test("hex digits are case insensitive", CompareRulesHash(true, Hash, true, "ED0EE23D32A621D1") == EHashCheck::Match);
		Test("empty present hash is malformed", CompareRulesHash(true, Hash, true, "") == EHashCheck::Malformed);
		Test("short hash is malformed", CompareRulesHash(true, Hash, true, "ed0ee23d32a621d") == EHashCheck::Malformed);
		Test("long hash is malformed", CompareRulesHash(true, Hash, true, "0ed0ee23d32a621d1") == EHashCheck::Malformed);
		Test("non-hex hash is malformed", CompareRulesHash(true, Hash, true, "ed0ee23d32a621dz") == EHashCheck::Malformed);
		Test("unlinked core cannot claim match", CompareRulesHash(false, Hash, true, "ed0ee23d32a621d1") == EHashCheck::Unavailable);
		Test("missing hash stays absent without core", CompareRulesHash(false, 0, false, "") == EHashCheck::Absent);
		Test("leading zeroes preserved", CompareRulesHash(true, 1, true, "0000000000000001") == EHashCheck::Match);
	}
}

#ifdef TC_SHARED31_STANDALONE
#include <cstdio>
int main()
{
	int Failed = 0;
	HashCases([&](const char* Name, bool Ok) { std::printf("%s %s\n", Ok ? "PASS" : "FAIL", Name); Failed += !Ok; });
	return Failed ? 1 : 0;
}
#else
#include "Client/TCRustQueries.h"
#include "Dom/JsonObject.h"
#include "Misc/AutomationTest.h"
#if WITH_DEV_AUTOMATION_TESTS
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCShared31RulesHashTest, "ThreeCrossroads.RustQueries.RulesHash", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCShared31RulesHashTest::RunTest(const FString&)
{
	HashCases([&](const char* Name, bool Ok) { TestTrue(UTF8_TO_TCHAR(Name), Ok); });
	FJsonObject Lobby;
	TestTrue(TEXT("reference absent hash has no warning"), FTCRustQueries::RulesWarning(Lobby).IsEmpty());
	Lobby.SetStringField(TEXT("rules"), TEXT("not-a-rules-hash"));
	TestFalse(TEXT("malformed field gives visible warning text"), FTCRustQueries::RulesWarning(Lobby).IsEmpty());
	if (FWW2Core::IsReady())
	{
		Lobby.SetStringField(TEXT("rules"), FString::Printf(TEXT("%016llx"), (unsigned long long)FWW2Core::RulesHash()));
		TestTrue(TEXT("actual linked core matches"), FTCRustQueries::RulesWarning(Lobby).IsEmpty());
		Lobby.SetStringField(TEXT("rules"), FString::Printf(TEXT("%016llx"), (unsigned long long)(FWW2Core::RulesHash() ^ 1)));
		TestTrue(TEXT("actual linked core mismatch warns"), FTCRustQueries::RulesWarning(Lobby).Contains(TEXT("previews may disagree")));
	}
	Lobby.SetNumberField(TEXT("rules"), 123);
	TestFalse(TEXT("numeric field is not silently absent"), FTCRustQueries::RulesWarning(Lobby).IsEmpty());
	return true;
}
#endif
#endif
