#include "World/TCDecals.h"
#include "World/TCGroundPaint.h"
#include <array>

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
		// Relief is continuous across cell edges. An adjacent dig, collapse or discovery can move the ground
		// beneath an existing picture without changing its owner cell. Refresh geometry only, retaining slot order
		// and the recipient's remembered cells. Crater corners can reach two cells beyond their centre.
		std::unordered_map<int, bool> ChangedSet;
		for (int C : Changed) if (C >= 0 && C < N) ChangedSet.emplace(C, true);
		auto Redrape = [&](FDecalSet& Set, bool bIsRubble)
		{
			bool bDirty = false;
			if (ChangedSet.empty() || bHydrate) return bDirty;
			const int Radius = bIsRubble ? 1 : 2;
			for (int Slot = 0; Slot < Set.Count(); Slot++)
			{
				const int C = Set.Cells[(size_t)Slot], X = C % W, Y = C / W;
				bool bNear = false;
				for (int Dy = -Radius; Dy <= Radius && !bNear; Dy++)
					for (int Dx = -Radius; Dx <= Radius; Dx++)
						if (X + Dx >= 0 && X + Dx < W && Y + Dy >= 0 && Y + Dy < H
							&& ChangedSet.find((Y + Dy) * W + X + Dx) != ChangedSet.end()) { bNear = true; break; }
				if (!bNear || !Ground.IsKnown(C)) continue;
				std::array<float, 75> Pos;
				std::array<float, 50> Uv;
				if (bIsRubble) RubbleQuad(C, W, Ground.MaterialAt(C), Height, Pos.data(), Uv.data());
				else CraterQuad(C, W, Height, Pos.data());
				float* Previous = &Set.Pos[(size_t)Slot * 75];
				if (!std::equal(Pos.begin(), Pos.end(), Previous))
				{
					std::copy(Pos.begin(), Pos.end(), Previous);
					bDirty = true;
				}
			}
			return bDirty;
		};
		const bool bRubbleDraped = Redrape(RubbleSet, true);
		const bool bCraterDraped = Redrape(CraterSet, false);
		return bRubble || bCrater || bRubbleDraped || bCraterDraped;
	}
}
