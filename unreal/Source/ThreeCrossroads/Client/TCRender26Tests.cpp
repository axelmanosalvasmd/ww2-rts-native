#include "Client/TCRender26Policy.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include <limits>

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender26BudgetTest, "ThreeCrossroads.Render26.ReceiptBudget",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRender26BudgetTest::RunTest(const FString&)
{
	TCRender26::FReceiptBudget Budget;
	TestFalse(TEXT("ordinary play does not consume the diagnostic budget"), Budget.Take(false, 0));
	TestEqual(TEXT("disabled remains empty"), Budget.Count, 0);
	TestFalse(TEXT("negative clock is rejected"), Budget.Take(true, -1));
	TestFalse(TEXT("nonfinite clock is rejected"), Budget.Take(true, std::numeric_limits<double>::quiet_NaN()));
	TestFalse(TEXT("infinite clock is rejected"), Budget.Take(true, std::numeric_limits<double>::infinity()));
	TestTrue(TEXT("first enabled receipt is immediate"), Budget.Take(true, 0));
	TestFalse(TEXT("same frame cannot repeat"), Budget.Take(true, 0));
	TestFalse(TEXT("subsecond receipt is withheld"), Budget.Take(true, .99));
	TestTrue(TEXT("blocked frame takes only one sample"), Budget.Take(true, 10));
	TestFalse(TEXT("no catch-up burst after a blocked frame"), Budget.Take(true, 10));
	TestFalse(TEXT("backward clock cannot extend the budget"), Budget.Take(true, 9));
	for (int I = 11; I < 200; ++I) Budget.Take(true, I);
	TestEqual(TEXT("hard bounded process receipts"), Budget.Count, TCRender26::FReceiptBudget::Limit);
	TestFalse(TEXT("no samples after cap"), Budget.Take(true, 1000));
	int Inventories = 0;
	for (int I = 0; I < TCRender26::FReceiptBudget::Limit; ++I) Inventories += TCRender26::FReceiptBudget::Inventory(I) ? 1 : 0;
	TestEqual(TEXT("allocation inventory has exactly two opportunities"), Inventories, 2);
	return true;
}
#endif
