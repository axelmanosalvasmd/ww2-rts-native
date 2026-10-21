// Source atmosphere component histories, not an aligned match or a rendered test.
// The standalone branch checks observation clocks. The engine branch feeds the
// actual atmosphere adapter with the same weather goals and frame deltas.
#if defined(TC_WORLD26_HISTORY_CORE_ONLY)
#define TC_ATMOS_CORE_ONLY
#endif
#include "World/TCAtmosphere.h"
#include <cstdio>
#include <string>

namespace TCWorld26HistoryTests
{
	#include "World/TCWorld26WeatherHistory.inl"
	int Clocks()
	{
		TCAtmosCore::FRenderHistory History;
		int Case = -1, Failures = 0;
		std::string Weather;
		double Rain = 0, Wet = 0;
		for (const auto& S : World26HistorySteps)
		{
			if (Case != S.Case)
			{
				Case = S.Case; History.Start();
				Weather = World26HistoryCases[Case].Weather; Rain = Wet = 0;
			}
			if (Weather != S.Weather || Rain != S.Rain || Wet != S.Wet) History.GoalChanged();
			Weather = S.Weather; Rain = S.Rain; Wet = S.Wet;
			for (int I = 0; I < S.Frames; I++) History.Frame(S.Dt);
			if (std::abs(History.Seconds - S.Seconds) > 1e-10 || std::abs(History.GoalSeconds - S.GoalSeconds) > 1e-10)
			{
				std::fprintf(stderr, "history clocks case %d tick %d: %.17g/%.17g vs %.17g/%.17g\n",
					Case, S.Tick, History.Seconds, History.GoalSeconds, S.Seconds, S.GoalSeconds);
				Failures++;
			}
		}
		const auto LastStarts = History.Starts;
		History.Start(); History.Frame(0);
		if (History.Starts != LastStarts + 1 || History.Frames != 1 || History.Seconds != 0 || History.GoalSeconds != 0) Failures++;
		History.Frame(10001.25); // Observation does not wrap with atmosphere's 10000 s particle clock.
		if (History.Seconds != 10001.25) Failures++;
		History.GoalChanged();
		if (History.Seconds != 10001.25 || History.GoalSeconds != 0) Failures++;
		std::printf("weather render-history clocks: %zu source cases, %zu checkpoints, %d failures (no engine look execution)\n",
			sizeof(World26HistoryCases) / sizeof(*World26HistoryCases), sizeof(World26HistorySteps) / sizeof(*World26HistorySteps), Failures);
		return Failures;
	}
}

#if defined(TC_WORLD26_HISTORY_CORE_ONLY)
int main() { return TCWorld26HistoryTests::Clocks() ? 1 : 0; }
#elif WITH_DEV_AUTOMATION_TESTS
#include "World/TCTerrain.h"
#include "Net/TCMatchState.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld26WeatherHistoryTest, "ThreeCrossroads.World.WeatherRenderHistory", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld26WeatherHistoryTest::RunTest(const FString& Parameters)
{
	using namespace TCWorld26HistoryTests;
	TestEqual(TEXT("observation clocks retain consumed dt rather than snapshot or held particle time"), Clocks(), 0);
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("WeatherRenderHistory"));
	if (!TestNotNull(TEXT("scratch world"), World)) return false;
	FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
	Context.SetCurrentWorld(World);
	World->InitializeActorsForPlay(FURL());
	ATCTerrain* Terrain = World->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity);
	if (!TestNotNull(TEXT("component ground"), Terrain))
	{
		GEngine->DestroyWorldContext(World); World->DestroyWorld(false); return false;
	}
	FTCMatchState State;
	State.bHasMatch = true;
	State.Map.W = State.Map.H = 4;
	State.Map.Chars.Init('.', 16); State.Map.Levels.Init(0, 16); State.Map.Known.Init(true, 16);
	State.FogVisible.Init(true, 16); State.FogExplored.Init(true, 16);
	// The fixture is one component: flat ground and three original map moods. It
	// makes no claim about those maps' terrain, received wires or visible pixels.
	for (int Case = 0; Case < (int)(sizeof(World26HistoryCases) / sizeof(World26HistoryCases[0])); Case++)
	{
		const auto& C = World26HistoryCases[Case];
		FTCGraphicsLevelScope Level(C.Low ? ETCGraphicsLevel::Low : ETCGraphicsLevel::High);
		State.Map.Name = UTF8_TO_TCHAR(C.Map);
		State.Weather.Now = UTF8_TO_TCHAR(C.Weather);
		State.Wx.bKnown = true; State.Wx.Rain = State.Wx.Wet = 0;
		Terrain->ResetFrom(State);
		while (Terrain->ChunksQueued()) Terrain->Flush(State);
		FTCAtmosphere View;
		TestTrue(TEXT("atmosphere attached"), View.Attach(World));
		View.StartMatch(State, UTF8_TO_TCHAR(C.Map), Terrain);
		TestTrue(TEXT("source materials available"), View.MaterialsLoaded());
		for (const auto& S : World26HistorySteps)
		{
			if (S.Case != Case) continue;
			State.Weather.Now = UTF8_TO_TCHAR(S.Weather);
			State.Wx.Rain = S.Rain; State.Wx.Wet = S.Wet; State.Tick = S.Tick;
			for (int I = 0; I < S.Frames; I++) View.Update((float)S.Dt, State, Terrain, nullptr, FVector2D(4, 4));
			const FTCWeatherLook& L = View.Look();
			const double Actual[] = { L.Haze, L.Sun, L.Shadow, L.Clouds, L.Wet, L.Sheen, L.Mud, L.Cover, L.Banks };
			for (int I = 0; I < 9; I++)
				TestTrue(*FString::Printf(TEXT("%hs tick %d source look[%d] %.9f vs %.9f"), C.Name, S.Tick, I, Actual[I], S.Look[I]), FMath::Abs(Actual[I] - S.Look[I]) <= 5e-5);
			const double Haze[] = { View.HazeNow.R, View.HazeNow.G, View.HazeNow.B };
			for (int I = 0; I < 3; I++) TestTrue(TEXT("source linear haze colour"), FMath::Abs(Haze[I] - S.Haze[I]) <= 5e-5);
			TestTrue(TEXT("consumed frame seconds"), FMath::Abs(View.RenderHistory().Seconds - S.Seconds) <= 1e-10);
			if (S.Frames > 0)
			{
				TestTrue(TEXT("seconds since actual goal change"), FMath::Abs(View.RenderHistory().GoalSeconds - S.GoalSeconds) <= 1e-10);
				TestEqual(TEXT("source bank visibility threshold after update"), View.BanksOn, S.Banks);
				if (View.BankMat)
				{
					float Opacity = -1;
					TestTrue(TEXT("actual bank material opacity"), View.BankMat->GetScalarParameterValue(FMaterialParameterInfo(TEXT("Opacity")), Opacity));
					TestTrue(TEXT("source bank opacity on the material"), FMath::Abs(Opacity - 0.34 * S.Look[8]) <= 2e-5);
				}
			}
		}
		const double PreviousParticleTime = View.T;
		View.StartMatch(State, UTF8_TO_TCHAR(C.Map), Terrain);
		TestTrue(TEXT("a repeated source start preserves the particle clock"), View.T == PreviousParticleTime);
		TestTrue(TEXT("a new presentation starts at its delivered weather"), View.Cur.Haze == View.Goal.Haze && View.Cur.Banks == View.Goal.Banks);
		TestTrue(TEXT("a new start clears match-local observation history"), View.RenderHistory().Frames == 0 && View.RenderHistory().Seconds == 0 && View.RenderHistory().Starts == 2);
		View.Detach();
	}
	GEngine->DestroyWorldContext(World); World->DestroyWorld(false);
	return true;
}
#endif
