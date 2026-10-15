#include "World/TCDecals.h"
#include "World/TCGroundPaint.h"

namespace TCDecals
{
	void FKnownCells::Reset()
	{
		RubbleSet.Init(2048, true);
		CraterSet.Init(1536, false);
	}

	bool FKnownCells::Update(const TCGroundPaintCore::FGround& Ground, bool bHydrate,
		const std::vector<int>& Changed, const std::function<double(double, double)>& Height)
	{
		using namespace TCGroundPaintCore;
		const int W = Ground.MapW(), H = Ground.MapH(), N = W * H;
		if (!N) return false;
		std::vector<int> RubbleCells, CraterCells;
		if (bHydrate)
		{
			for (int C = 0; C < N; C++) if (Ground.IsKnown(C))
			{
				if (Ground.ObjectAt(C) == 'R') RubbleCells.push_back(C);
				if (Ground.CharAt(C) == '+') CraterCells.push_back(C);
			}
		}
		else { RubbleCells = Changed; CraterCells = Changed; }
		const bool bRubble = RubbleSet.Update(RubbleLook(RubbleCells, N),
			[&](int C) { return Ground.IsKnown(C) && Ground.ObjectAt(C) == 'R'; },
			[&](int Slot, int C) { RubbleQuad(C, W, Ground.MaterialAt(C), Height,
				&RubbleSet.Pos[(size_t)Slot * 75], &RubbleSet.Uv[(size_t)Slot * 50]); });
		const std::function<uint8_t(int)> KnownChar = [&](int C) { return Ground.IsKnown(C) ? Ground.CharAt(C) : (uint8_t)0; };
		// Lasting crater pictures are World-only in main.js. Authored shell holes stay in the paint.
		const bool bCrater = Ground.IsWorld() && CraterSet.Update(CraterLook(CraterCells, W, N),
			[&](int C) { return Ground.IsKnown(C) && CraterWanted(C, W, H, KnownChar); },
			[&](int Slot, int C) { CraterQuad(C, W, Height, &CraterSet.Pos[(size_t)Slot * 75]); });
		return bRubble || bCrater;
	}
}
