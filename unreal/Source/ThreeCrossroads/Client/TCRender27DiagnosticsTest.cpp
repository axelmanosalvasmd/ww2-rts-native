#include "Client/TCRender27GpuPolicy.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender27BufferDescriptorsTest, "ThreeCrossroads.Render27.BufferDescriptors",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRender27BufferDescriptorsTest::RunTest(const FString&)
{
	TestEqual(TEXT("ordinary descriptor requests still ignore buffers"), TCRender27::GpuBufferMode(false, false), 0);
	TestEqual(TEXT("existing filtered texture binaries still ignore buffers"), TCRender27::GpuBufferMode(false, true), 0);
	TestEqual(TEXT("explicit buffer inventory captures descriptors only"), TCRender27::GpuBufferMode(true, false), 1);
	TestEqual(TEXT("combining buffer inventory and binary capture is refused"), TCRender27::GpuBufferMode(true, true), -1);
	return true;
}
#endif
