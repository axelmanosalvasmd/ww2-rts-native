// Known-cell decal slots from client/rubble-decals.js and client/crater-decals.js at e4e579a.
// Engine-free. Geometry and atlas formulas remain shared with the painted-ground reference harness.
#pragma once

#include <algorithm>
#include <cstdint>
#include <functional>
#include <unordered_map>
#include <vector>

namespace TCGroundPaintCore { class FGround; }

namespace TCDecals
{
	// The decal slots as rubble-decals.js and crater-decals.js keep them: one 25-vertex grid per cell, a removed cell's
	// slot takes the last one's vertices (so the draw order matches the browser's).
	struct FDecalSet
	{
		int Capacity = 0;
		bool bUv = false;
		std::vector<int> Cells;
		std::unordered_map<int, int> Slots;
		std::vector<float> Pos, Uv;
		void Init(int InCapacity, bool bInUv) { Capacity = InCapacity; bUv = bInUv; Cells.clear(); Slots.clear(); Pos.assign((size_t)Capacity * 75, 0.f); Uv.assign(bUv ? (size_t)Capacity * 50 : 0, 0.f); }
		int Count() const { return (int)Cells.size(); }
		// Look: the cells to recheck, in order. Wanted(c): it should have a picture; Write(slot, c): fill that slot.
		bool Update(const std::vector<int>& Look, const std::function<bool(int)>& Wanted, const std::function<void(int, int)>& Write)
		{
			bool bDirty = false;
			for (const int C : Look)
			{
				auto It = Slots.find(C);
				if (Wanted(C))
				{
					int Slot;
					if (It == Slots.end()) { if ((int)Cells.size() >= Capacity) continue; Slot = (int)Cells.size(); Cells.push_back(C); Slots.emplace(C, Slot); }
					else Slot = It->second;
					Write(Slot, C); bDirty = true;
				}
				else if (It != Slots.end())
				{
					const int Slot = It->second, Last = Cells.back();
					Cells.pop_back(); Slots.erase(It);
					if (Slot < (int)Cells.size())
					{
						Cells[(size_t)Slot] = Last; Slots[Last] = Slot;
						std::copy_n(&Pos[(size_t)Cells.size() * 75], 75, &Pos[(size_t)Slot * 75]);
						if (bUv) std::copy_n(&Uv[(size_t)Cells.size() * 50], 50, &Uv[(size_t)Slot * 50]);
					}
					bDirty = true;
				}
			}
			return bDirty;
		}
	};
	// rubble-decals.js update(changed): the changed cells themselves
	inline std::vector<int> RubbleLook(const std::vector<int>& Changed, int Cells)
	{
		std::vector<int> Out;
		for (const int C : Changed) if (C >= 0 && C < Cells) Out.push_back(C);
		return Out;
	}
	// crater-decals.js update(changed): each changed cell and its four neighbours by index (c - 1 wraps a row, as there),
	// each once, in first-seen order
	inline std::vector<int> CraterLook(const std::vector<int>& Changed, int W, int Cells)
	{
		std::vector<int> Out;
		std::unordered_map<int, bool> Seen;
		for (const int C : Changed) for (const int Nn : { C, C - 1, C + 1, C - W, C + W })
			if (Nn >= 0 && Nn < Cells && Seen.emplace(Nn, true).second) Out.push_back(Nn);
		return Out;
	}

	// main.js drawTerrain hydrates through the same update API as later changes. Craters expand the
	// candidate list to neighbours before allocating slots, including at the 1,536-picture limit.
	class FKnownCells
	{
	public:
		void Reset();
		bool Update(const TCGroundPaintCore::FGround& Ground, bool bHydrate, const std::vector<int>& Changed,
			const std::function<double(double, double)>& Height);
		const FDecalSet& Rubble() const { return RubbleSet; }
		const FDecalSet& Craters() const { return CraterSet; }
	private:
		FDecalSet RubbleSet, CraterSet;
	};
}
