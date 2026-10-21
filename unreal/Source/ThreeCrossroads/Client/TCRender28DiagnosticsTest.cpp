#include "Client/TCRender28Policy.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender28TransitionRequestTest, "ThreeCrossroads.Render28.TransitionRequest",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRender28TransitionRequestTest::RunTest(const FString&)
{
	TestTrue(TEXT("observed pass and resource names are literal filters"),
		TCRender28::LiteralFilter(TEXT("ClearBuffer")) && TCRender28::LiteralFilter(TEXT("NumCulledLightsGrid")));
	for (const TCHAR* Invalid : {TEXT(""), TEXT("None"), TEXT("nOnE"), TEXT("!SceneDepthZ"), TEXT("*"),
		TEXT("Scene?"), TEXT("Scene\nDepth"), TEXT("\"Scene\""), TEXT(" Scene"), TEXT("Scene ")})
		TestFalse(FString::Printf(TEXT("unbounded or special filter refused: %s"), Invalid), TCRender28::LiteralFilter(Invalid));
	TestFalse(TEXT("oversized filter refused"), TCRender28::LiteralFilter(*FString::ChrN(129, TEXT('a'))));
	TCRender28::FTransitionBudget Budget;
	TestFalse(TEXT("ordinary tour does not arm logging"), Budget.Take(false, true));
	TestFalse(TEXT("earlier pose does not spend the request"), Budget.Take(true, false));
	TestTrue(TEXT("named pose gets one request"), Budget.Take(true, true));
	TestFalse(TEXT("repeated pose and replacement worlds cannot extend the process budget"), Budget.Take(true, true));
	return true;
}
#endif
