// Recipient-delivered landmarks must reach the actual structure builder and survive later fog memory updates.
#if WITH_DEV_AUTOMATION_TESTS
#include "World/TCWorldObjects.h"
#include "World/TCTerrain.h"
#include "Net/TCMatchState.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld29LandmarkMemoryTest, "ThreeCrossroads.World.LandmarkMemory",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld29LandmarkMemoryTest::RunTest(const FString& Parameters)
{
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false);
	if (!TestNotNull(TEXT("landmark world"), World)) return false;
	FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
	Context.SetCurrentWorld(World);
	FTCMatchState State;
	State.Map.W = State.Map.H = 5; State.Map.bWorld = true;
	State.Map.Chars.Init('.', 25); State.Map.Levels.Init(0, 25); State.Map.Known.Init(false, 25);
	for (int32 C : { 11, 12 }) { State.Map.Chars[C] = '='; State.Map.Known[C] = true; }
	ATCTerrain* Terrain = World->SpawnActor<ATCTerrain>();
	FTCWorldObjects Objects;
	Terrain->SetCellSource(&Objects);
	TSharedPtr<FJsonObject> Start;
	FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TEXT(
		"{\"map\":{\"w\":5,\"h\":5,\"world\":true,\"rows\":[\".....\",\".....\",\".....\",\".....\",\".....\"]},"
		"\"cells\":[[11,\"=\",0,0,{\"landmark\":\"stone bridge\"}],[12,\"=\",0,0]]}")), Start);
	if (TestTrue(TEXT("delivered landmarks start"), Start.IsValid() && Objects.ApplyStart(*Start, State)))
	{
		Terrain->ResetFrom(State); Terrain->Flush(State); Objects.Update(*Terrain, nullptr);
		auto StoneDecks = [&]()
		{
			int32 Count = 0;
			Objects.ForEachPiece(0, [&](const TCWorldCore::FPiece& P)
			{
				TestTrue(TEXT("no structure originates on an unknown cell"), P.Cell >= 0 && State.Map.Known[P.Cell]);
				if (P.Kind == TCWorldCore::EPiece::Stone && FMath::IsNearlyEqual(P.Sy, .3f)) Count++;
			});
			return Count;
		};
		TestEqual(TEXT("discovered landmark makes its entire connected span stone"), StoneDecks(), 2);
		TSharedPtr<FJsonObject> Memory;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TEXT("{\"cells\":[[11,\"=\",0,0],[12,\"=\",0,0]]}")), Memory);
		Objects.ApplySnapshot(*Memory); Objects.Update(*Terrain, nullptr);
		TestEqual(TEXT("later cell rows without landmark metadata retain its look"), StoneDecks(), 2);
		Objects.Clear();
		TestFalse(TEXT("new view clears landmark memory"), Objects.HasMap());
	}
	GEngine->DestroyWorldContext(World); World->DestroyWorld(false);
	return true;
}
#endif
