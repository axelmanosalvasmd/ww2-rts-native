#include "Client/TCUnitRenderer.Visibility.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "CoreMinimal.h"
#include "Misc/AutomationTest.h"
#include "Client/TCModelLibrary.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCBodyControlTest, "ThreeCrossroads.Models.BodyVisibilityControl",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCBodyControlTest::RunTest(const FString&)
{
	struct FBatch { TArray<FTransform> Transforms; TArray<float> Data; TArray<uint64> Keys; int32 Floats = 2; };
	FBatch Original;
	Original.Keys = {FTCInstanceHistory::PartKey(8, 0), FTCInstanceHistory::ManKey(4, 0), FTCInstanceHistory::ManKey(4, 1),
		FTCInstanceHistory::RingKey(8, false), FTCInstanceHistory::RingKey(4, true), FTCInstanceHistory::BodyKey(1)};
	for (int32 I = 0; I < Original.Keys.Num(); I++)
	{
		Original.Transforms.Add(FTransform(FVector(I, 0, 0))); Original.Data.Add(I); Original.Data.Add(100 + I);
	}
	FBatch Normal = Original;
	TCBodyControl::FControl{}.Filter(Normal);
	TestEqual(TEXT("normal play retains every instance"), Normal.Keys, Original.Keys);
	TestEqual(TEXT("normal play retains every custom float"), Normal.Data, Original.Data);
	FBatch Gun = Original;
	TCBodyControl::FControl{8, -1}.Filter(Gun);
	TestEqual(TEXT("gun control retains the actual gun and both source rings"), Gun.Keys,
		TArray<uint64>{Original.Keys[0], Original.Keys[3], Original.Keys[4]});
	TestEqual(TEXT("custom data stays paired with the filtered submitted transforms"), Gun.Data, TArray<float>{0,100,3,103,4,104});
	FBatch Man = Original;
	TCBodyControl::FControl{4, 1}.Filter(Man);
	TestEqual(TEXT("slot control retains one man's body/kit key and both rings"), Man.Keys,
		TArray<uint64>{Original.Keys[2], Original.Keys[3], Original.Keys[4]});
	TestTrue(TEXT("the selected man keeps its original world transform"), Man.Transforms[0].Equals(Original.Transforms[2]));
	return true;
}
#endif
