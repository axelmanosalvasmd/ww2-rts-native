// Shared by the native acceptance test and the standalone production-kernel check.
#include "World/TCGroundPaint.h"

namespace TCWorld30Memory
{
	inline int Check()
	{
		using namespace TCGroundPaintCore;
		FStartMap Map; Map.W = Map.H = 8; Map.bWorld = true;
		Map.Rows.assign(8, "........");
		// Deliberately non-public raw cells must stay absent until delivered.
		Map.Rows[3][4] = 'R'; Map.Rows[4][4] = '+';
		FGround Ground; Ground.Start(Map, {}, nullptr);
		TCDecals::FKnownCells Decals; Decals.Reset();
		auto Height = [](double X, double Z) { return .01 * X + .02 * Z; };
		Decals.Update(Ground, true, {}, Height);
		if (Decals.Rubble().Count() || Decals.Craters().Count()) return 1;
		FCellRow Rubble; Rubble.C = 27; Rubble.Ch = 'R'; Rubble.bData = true;
		Rubble.Ground = '.'; Rubble.Object = 'R'; Rubble.bMaterial = true; Rubble.Material = "brick";
		FCellRow Crater; Crater.C = 35; Crater.Ch = '+';
		Ground.Apply({Rubble, Crater});
		Decals.Update(Ground, false, {27,35}, Height);
		if (Decals.Rubble().Cells != std::vector<int>{27} || Decals.Craters().Cells != std::vector<int>{35}) return 2;
		const auto RubblePos = Decals.Rubble().Pos, CraterPos = Decals.Craters().Pos;
		// A later remembered cell row lacks metadata. The material and object remain.
		Rubble.bData = false; Rubble.bMaterial = false; Rubble.Ground = Rubble.Object = 0;
		Ground.Apply({Rubble, Crater});
		Decals.Update(Ground, false, {27,35}, Height);
		if (Decals.Rubble().Cells != std::vector<int>{27} || Decals.Craters().Cells != std::vector<int>{35}) return 3;
		if (RubblePos != Decals.Rubble().Pos || CraterPos != Decals.Craters().Pos) return 4;
		// A normal hydration after a graphics rebuild must preserve recipient memory.
		Decals.Reset(); Decals.Update(Ground, true, {}, Height);
		if (Decals.Rubble().Cells != std::vector<int>{27} || Decals.Craters().Cells != std::vector<int>{35}) return 5;
		if (Ground.IsKnown(28) || Ground.IsKnown(36)) return 6;
		return 0;
	}
}
