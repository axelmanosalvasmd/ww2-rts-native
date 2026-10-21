// Original atmosphere.js transition sequences, including High and Low and the
// exact three-second replacement boundary. The runtime uses this same core step.
#if defined(TC_ATMOS_TRANSITIONS_CORE_ONLY)
#define TC_ATMOS_CORE_ONLY
#endif
#include "World/TCAtmosphere.h"
#include <cstdio>
#include <string>

namespace TCAtmosTransitionTests
{
	#include "World/TCAtmosphere.TransitionOracle.inl"
	int Check()
	{
		std::string Kind;
		double Alpha = 0;
		int Failures = 0, Cases = 0, Frames = 0;
		for (const auto& S : WeatherTransitionOracle)
		{
			if (S.Case >= 0) { Kind = S.Kind; Alpha = S.Alpha; Cases++; }
			else
			{
				const std::string Want = TCAtmosCore::FallKind(S.Weather, S.Rain, S.MoodWeather);
				if (Want != S.Want) { std::fprintf(stderr, "source fallKind differs at frame %d\n", Frames); Failures++; }
				if (TCAtmosCore::AdvanceFall(Alpha, !Kind.empty(), Kind == Want, !Want.empty(), S.Dt)) { Kind = Want; Alpha = 0; }
			}
			if (Kind != S.Kind || !std::isfinite(Alpha) || std::abs(Alpha - S.Alpha) > 1e-14)
			{
				std::fprintf(stderr, "source weather frame %d: %s %.17g vs %s %.17g\n", Frames, Kind.c_str(), Alpha, S.Kind, S.Alpha);
				Failures++;
			}
			Frames++;
		}
		std::printf("weather source transitions: %d sequences, %d frames, %d failures\n", Cases, Frames, Failures);
		return Failures;
	}
}

#if defined(TC_ATMOS_TRANSITIONS_CORE_ONLY)
int main() { return TCAtmosTransitionTests::Check() ? 1 : 0; }
#elif WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAtmosphereTransitionsTest, "ThreeCrossroads.World.AtmosphereTransitions", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCAtmosphereTransitionsTest::RunTest(const FString& Parameters)
{
	TestEqual(TEXT("original High/Low weather transition frames"), TCAtmosTransitionTests::Check(), 0);
	return true;
}
#endif
