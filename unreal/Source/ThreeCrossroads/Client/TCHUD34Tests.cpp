#include "Client/TCHUD34Tests.inl"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHud34WorldInformationTest,"ThreeCrossroads.HUD34.WorldInformationExclusion",EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHud34WorldInformationTest::RunTest(const FString&)
{
 TCHud34Test::Run([&](const char* Name,bool Ok){TestTrue(UTF8_TO_TCHAR(Name),Ok);});
 return !HasAnyErrors();
}
#endif
