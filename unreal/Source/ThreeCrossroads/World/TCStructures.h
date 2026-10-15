// The pinned client/structures.js piece builders, independent of placement and engine rendering.
// Models remain the source-exported piece/* meshes. Inputs are recipient-known cells only.
#pragma once

#include <vector>

namespace TCWorldCore
{
	class FCellSource;
	struct FPiece;
	struct FHouseRect;
	struct FHouseTint;
}

namespace TCStructures
{
	// structures.js C.foot and C.tint, shared by house and cell builders during one rebuild.
	struct FContext
	{
		const std::vector<unsigned char>* Foot = nullptr;
		const std::vector<int>* TintIndex = nullptr;
		const std::vector<TCWorldCore::FHouseTint>* Tints = nullptr;
	};

	// Appends put() instances in the source's call order. A completely fallen house emits nothing.
	void House(const TCWorldCore::FCellSource& Source, const TCWorldCore::FHouseRect& Rect,
		std::vector<TCWorldCore::FPiece>& Out, const FContext& Context = {});
	// structures.js rebuild: hedges, walls, trenches, wire, traps, mines, hospitals, rubble, bridges.
	void Chunk(const TCWorldCore::FCellSource& Source, int X0, int Y0, int X1, int Y1,
		std::vector<TCWorldCore::FPiece>& Out, const FContext& Context = {});
}
