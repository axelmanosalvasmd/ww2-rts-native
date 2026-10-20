// Source public-input relief and hidden-input invariance across discovery and
// terrain edits. The known-only production query gap is reported, never accepted
// or pinned as correct by these checks. This is not an actor/network reconnect.
#include "World/TCRelief.h"
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

namespace TCWorld26PublicGroundTests
{
	#include "World/TCWorld26PublicGround.inl"
	int Level(char C) { return C >= 'a' ? 96 - (int)C : C - '0'; }
	void Inputs(TCRelief::FRelief& R, const FWorld26PublicStage& S, bool bPublic, bool bHostile, int Stage)
	{
		for (int C = 0; C < 81; C++)
		{
			const bool Known = S.Known[C] == '1';
			R.InKnown[(size_t)C] = bPublic || Known;
			R.InGrid[(size_t)C] = !Known && bHostile ? (uint8_t)"WFT=+RBD"[(C + Stage) % 8] : (uint8_t)S.Grid[C];
			R.InLevel[(size_t)C] = (int8_t)(!Known && bHostile ? (C + Stage) % 11 - 5 : Level(S.Levels[C]));
			R.InRoad[(size_t)C] = !Known && bHostile ? 1 : S.Grid[C] == 'D';
			// As createWaterLevels does, freeze the initial public start, before
			// discovery or later crater/flood rows. Hostile unknowns are isolated.
			R.InFrozen[(size_t)C] = (int8_t)(!Known && bHostile ? 7 : C % 9 < 4 ? 1 : 0);
		}
	}
	bool SamePart(const TCRelief::FPart& A, const TCRelief::FPart& B)
	{
		return A.Pos == B.Pos && A.Nrm == B.Nrm && A.Uv == B.Uv && A.Paint == B.Paint && A.Scar == B.Scar
			&& A.Idx == B.Idx && A.Owner == B.Owner && A.Detail == B.Detail && A.Picks == B.Picks;
	}
	int Check()
	{
		int Failures = 0, Samples = 0, Stages = 0;
		double PublicError = 0, KnownQueryGap = 0;
		auto Require = [&](bool Good, const char* Message)
		{
			if (!Good) { std::fprintf(stderr, "public ground history: %s\n", Message); Failures++; }
		};
		TCRelief::FRelief Public, Known, Hostile;
		Public.Reset(9, 9, true, 0, false);
		Known.Reset(9, 9, true, 4, true); Hostile.Reset(9, 9, true, 4, true);
		for (const auto& S : World26PublicStages)
		{
			std::vector<int> Changed;
			if (Stages)
			{
				const auto& Prev = World26PublicStages[Stages - 1];
				for (int C = 0; C < 81; C++) if (S.Grid[C] != Prev.Grid[C] || S.Levels[C] != Prev.Levels[C] || S.Known[C] != Prev.Known[C]) Changed.push_back(C);
			}
			Inputs(Public, S, true, false, Stages); Inputs(Known, S, false, false, Stages); Inputs(Hostile, S, false, true, Stages);
			if (!Stages) { Public.Build(); Known.Build(); Hostile.Build(); }
			else
			{
				Public.Update(Changed); Known.Update(Changed);
				// Exercise even hostile hidden rows without granting knowledge.
				std::vector<int> HostileChanged = Changed;
				for (int C = 0; C < 81; C++) if (S.Known[C] != '1') HostileChanged.push_back(C);
				Hostile.Update(HostileChanged);
			}
			TCRelief::FRelief Restored;
			Restored.Reset(9, 9, true, 4, true); Inputs(Restored, S, false, false, Stages); Restored.Build();
			for (int P = 0; P < Known.NumParts(); P++)
			{
				Require(SamePart(Known.Part(P), Hostile.Part(P)), "unknown raw inputs changed known geometry");
				Require(SamePart(Known.FrontierSeal(P), Hostile.FrontierSeal(P)), "unknown raw inputs changed frontier seals");
				Require(SamePart(Known.Part(P), Restored.Part(P)), "incremental geometry differs from rebuild of retained public inputs");
				for (const int C : Known.Part(P).Owner) Require(S.Known[C] == '1', "a known mesh owns an undiscovered cell");
			}
			for (int I = 0; I < 36; I++)
			{
				const double X = World26PublicSamples[I][0], Z = World26PublicSamples[I][1];
				const double H = Public.HAt(X, Z), K = Known.HAt(X, Z);
				PublicError = std::max(PublicError, std::abs(H - S.Height[I]));
				Require(std::isfinite(H) && std::abs(H - S.Height[I]) <= 1e-6, "sanitized public-input HAt differs from original relief.js");
				Require(K == Hostile.HAt(X, Z), "unknown raw inputs changed a production query");
				Require(K == Restored.HAt(X, Z), "incremental query differs from rebuild of retained public inputs");
				KnownQueryGap = std::max(KnownQueryGap, std::abs(K - S.Height[I]));
				Samples++;
			}
			Stages++;
		}
		std::printf("{\"stages\":%d,\"sourcePublicQueries\":%d,\"publicMaxError\":%.17g,\"productionKnownOnlyGapMetres\":%.17g,\"failures\":%d,\"actorReconnectRun\":false,\"querySamplesAgree\":%s}\n",
			Stages, Samples, PublicError, KnownQueryGap, Failures, KnownQueryGap <= 1e-6 ? "true" : "false");
		return Failures;
	}
}

#if defined(TC_WORLD26_PUBLIC_CORE_ONLY)
int main() { return TCWorld26PublicGroundTests::Check() ? 1 : 0; }
#elif WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld26PublicGroundTest, "ThreeCrossroads.World.PublicGroundHistory", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld26PublicGroundTest::RunTest(const FString& Parameters)
{
	TestEqual(TEXT("source public inputs, hidden-input invariance and retained-input rebuild across terrain edits"), TCWorld26PublicGroundTests::Check(), 0);
	return true;
}
#endif
