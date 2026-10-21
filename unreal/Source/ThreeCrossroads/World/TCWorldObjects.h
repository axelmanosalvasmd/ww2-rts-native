// The map's objects as this client knows them: houses (standing, ruined), walls, hedges, rubble, trenches, wire, traps,
// mines, field hospitals and bridges, drawn with the reference's own piece models (Content/Models/Meshes/piece_*, the
// models export of client/structures.js KINDS) and laid out exactly as the reference lays them; the scenery props
// (World/TCScatter.h); the wrecks and the ghosts of remembered enemy buildings drawn with the unit models; plus the
// house pick the garrison order needs and the per-cell state and wreck list the cover preview needs. A port of the
// pinned reference (ReferencePin):
//   client/main.js       startGame terrain (grid, groundGrid, objectGrid, physicalGrid, mineGrid, state), applyCells,
//                        rememberLook (World landmarks and spoil tips), the snapshot's wrecks (hulks: the unit's model in
//                        the wreck material), applyGhosts (remembered enemy buildings at 0.35 opacity), setNodes,
//                        houseAt (the right-click house pick: nearest pickable piece, then 0.4 m on along the ray, a 'B'
//                        cell) and minimapCursor's house;
//   client/world-state.js a snapshot without wrecks keeps the last list;
//   client/structures.js every map piece builder, line for line: rnd, rectangles, isHouse, isFoot, footOf, blocksIn,
//                        relayBlocks, layout, frame, spec (with its palettes), bays, tower, piece, quoins, chimney,
//                        headframe, mineHead, house, shed, ruin, rubble, hedges, walls (stoneWall, bags, bagWall),
//                        trenches, wire, traps, mines, hospitals, stoneDecks, bridges, put/compose, the KINDS pick and
//                        shadow flags, and rebuild's parts (one per house, one per 32 x 32 cells) and change rules;
//   client/railways.js   has(cell): the scenery skips a known railway cell;
//   shared/sim.js        CELL, CFG.houses, houseKindsWhere, cellNoise, startWear, packState/startState, unitFlags bits
//                        20-22 (a garrison's house type), logCell/seenWorldCell rows [cell, char, level, state, data];
//   shared/world-layers.js BASE_SURFACES, composeWorldCell, migrateWorldMap (the start's ground and object layers).
//
// What it reads (recipient-visible data only): the start's map (rows, layers, heights, buildings, spawns, points,
// biome, world), its cells, every snapshot's cells, wrecks, ghosts and supply nodes. Nothing else: no unit rows, no
// rules, no server state. World Conquest cells this side has not discovered are '.' and carry no object, so nothing is
// drawn or picked there; landmarks and spoil tips come only with a discovered cell's data, as in the browser. Wrecks
// and ghosts are the rows this side was sent (its memory).
//
// Pieces are the reference's own instances: kind, centre, size, yaw, tilts and tint, exactly as put() makes them (the
// Node oracle unreal/Tools/world/scenery-oracle.mjs and the World.PiecesOracle test hold the two to the same numbers).
// The house pick raycasts the hull of every pickable piece (KINDS pick: true), as houseAt raycasts their meshes.
// Fog dims every piece by its cell like the ground (ATCTerrain::CellColor's 1, 0.55 and 0.32), through a fog texture
// the world materials sample (unreal/Tools/world/make-world-materials.py).
//
// Axes: reference metres x (east), y (up), z (south); Unreal X = x * 100, Y = z * 100, Z = y * 100 (World/TCTerrain.h).
//
// The engine-free part (namespace TCWorldCore) compiles alone with TC_WORLD_CORE_ONLY defined, so a standalone harness
// can check it against the reference JavaScript.
#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <utility>
#include <vector>
#include "World/TCStructures.h"

namespace TCWorldCore
{
	inline constexpr double CellMetres = 2.0; // shared/sim.js CELL
	inline constexpr double Pi = 3.14159265358979323846;

	// client/structures.js rnd: a stable 0-1 number per cell and key (Math.sin; see the change notes on last bits).
	inline double Rnd(double X, double Y, double K)
	{
		const double V = std::sin(X * 127.1 + Y * 311.7 + K * 74.7) * 43758.5453;
		return V - std::floor(V);
	}
	// client/structures.js yawOf: the yaw that points a piece's local +z along (Dx, Dz).
	inline double YawOf(double Dx, double Dz) { return std::atan2(Dx, Dz); }

	// shared/sim.js cellNoise (Math.imul and >>> on 32 bits).
	inline double CellNoise(int C)
	{
		uint32_t H = (uint32_t)(C + 1) * 0x9e3779b1u;
		H ^= H >> 15;
		H *= 0x85ebca6bu;
		return (double)(H ^ (H >> 13)) / 4294967296.0;
	}
	// shared/world-layers.js BASE_SURFACES and composeWorldCell (without the mine: callers keep it apart).
	inline bool IsBaseSurface(unsigned char Ch) { return Ch == '.' || Ch == 'D' || Ch == '+' || Ch == 'M' || Ch == 'W' || Ch == 'F'; }
	inline unsigned char Compose(unsigned char Ground, unsigned char Object) { return Object && Object != '.' ? Object : Ground; }
	// shared/sim.js startWear and startState: the state a cell the server has not mentioned is in (wear only).
	inline double StartWear(unsigned char Ch, int C)
	{
		return Ch == 'M' || Ch == 'F' ? 0.2 + 0.6 * CellNoise(C) : Ch == '+' ? 0.3 + 0.3 * CellNoise(C) : 0.0;
	}
	inline unsigned char StartState(unsigned char Ch, int C)
	{
		const double Q = std::floor(StartWear(Ch, C) * 4);
		return (unsigned char)(Q < 3 ? Q : 3);
	}
	// The damage stage the server packs into bits 3-4 of a cell's state: 0 whole, 1 damaged, 2 nearly gone.
	inline int StageOf(unsigned char State) { return (State >> 3) & 3; }

	// shared/sim.js CFG.houses. Min: the block size that makes the type (NoMin: only a map's buildings list makes it);
	// Mul and Worn: incoming accuracy on a squad inside while its cell is whole and when it is about to come down;
	// Vision: replaces CFG.garrisonVision for a squad inside (0: none).
	struct FHouseType
	{
		const char* Name;
		int Min;
		double Hp, Mul, Worn, Vision;
	};
	inline constexpr int NoMin = 0x7fffffff;
	inline constexpr int HouseTypeCount = 6;
	inline constexpr FHouseType HouseTypes[HouseTypeCount] = {
		{ "wooden shed", 0, 250, 0.55, 0.85, 0 }, { "brick house", 11, 400, 0.35, 0.7, 0 }, { "stone building", 24, 650, 0.25, 0.55, 0 },
		{ "church", NoMin, 700, 0.25, 0.55, 1.6 }, { "factory", NoMin, 1000, 0.2, 0.45, 1.4 }, { "mine head", NoMin, 800, 0.25, 0.5, 1.5 },
	};
	inline constexpr double GarrisonVision = 1.25; // shared/sim.js CFG.garrisonVision
	// The index of a CFG.houses name, or -1.
	inline int HouseTypeOf(const char* Name)
	{
		for (int I = 0; I < HouseTypeCount; I++)
		{
			const char* A = HouseTypes[I].Name;
			const char* B = Name;
			while (*A && *A == *B) { A++; B++; }
			if (!*A && !*B) return I;
		}
		return -1;
	}
	// CFG.houses.findLastIndex(h => cells >= h.min)
	inline int DefaultHouseType(size_t Cells)
	{
		int Kind = -1;
		for (int I = 0; I < HouseTypeCount; I++) if (HouseTypes[I].Min != NoMin && Cells >= (size_t)HouseTypes[I].Min) Kind = I;
		return Kind;
	}

	// shared/sim.js houseKindsWhere: over the cells where IsB(c), each connected block (4-neighbours, breadth first from
	// its first cell) that holds one of Seeds (null: every cell) gets a type: the last cell in visiting order that
	// Named(c) (>= 0) names, else by its size. Each(cells, type) per block.
	template <typename IsFn, typename NamedFn, typename EachFn>
	void HouseKindsWhere(IsFn IsB, int N, int W, NamedFn Named, const std::vector<int>* Seeds, EachFn Each)
	{
		if (N <= 0 || W <= 0) return;
		std::vector<unsigned char> Seen((size_t)N, 0);
		std::vector<int> Q;
		auto Visit = [&](int C0)
		{
			if (C0 < 0 || C0 >= N || !IsB(C0) || Seen[(size_t)C0]) return;
			Q.clear();
			Q.push_back(C0);
			Seen[(size_t)C0] = 1;
			for (size_t I = 0; I < Q.size(); I++)
			{
				const int Cur = Q[I];
				const int Near[4] = { Cur - 1, Cur + 1, Cur - W, Cur + W };
				for (const int N2 : Near)
				{
					if (N2 < 0 || N2 >= N || Seen[(size_t)N2] || !IsB(N2) || (std::abs(N2 - Cur) == 1 && N2 / W != Cur / W)) continue;
					Seen[(size_t)N2] = 1;
					Q.push_back(N2);
				}
			}
			int Kind = DefaultHouseType(Q.size());
			for (const int Cell : Q) { const int Named2 = Named(Cell); if (Named2 >= 0) Kind = Named2; }
			Each(Q, Kind);
		};
		if (Seeds) { for (const int C : *Seeds) Visit(C); }
		else for (int C = 0; C < N; C++) Visit(C);
	}

	// A rectangle of cells, X1 and Y1 exclusive.
	struct FCellRect
	{
		int X0 = 0, Y0 = 0, X1 = 0, Y1 = 0;
		bool operator==(const FCellRect& O) const { return X0 == O.X0 && Y0 == O.Y0 && X1 == O.X1 && Y1 == O.Y1; }
	};

	// client/structures.js rectangles: greedy rectangles in scan order (run right, then down while the whole row fits).
	template <typename OkFn>
	std::vector<FCellRect> Rectangles(int X0, int Y0, int X1, int Y1, OkFn Ok)
	{
		std::vector<FCellRect> Out;
		const int Wd = X1 - X0, Hd = Y1 - Y0;
		if (Wd <= 0 || Hd <= 0) return Out;
		std::vector<unsigned char> Used((size_t)Wd * (size_t)Hd, 0);
		auto Free = [&](int X, int Y) { return Ok(X, Y) && !Used[(size_t)(Y - Y0) * (size_t)Wd + (size_t)(X - X0)]; };
		for (int Y = Y0; Y < Y1; Y++) for (int X = X0; X < X1; X++)
		{
			if (!Free(X, Y)) continue;
			int Xe = X + 1;
			while (Xe < X1 && Free(Xe, Y)) Xe++;
			int Ye = Y + 1;
			auto RowFree = [&](int Yy) { for (int Xx = X; Xx < Xe; Xx++) if (!Free(Xx, Yy)) return false; return true; };
			while (Ye < Y1 && RowFree(Ye)) Ye++;
			for (int Yy = Y; Yy < Ye; Yy++) for (int Xx = X; Xx < Xe; Xx++) Used[(size_t)(Yy - Y0) * (size_t)Wd + (size_t)(Xx - X0)] = 1;
			FCellRect R;
			R.X0 = X; R.Y0 = Y; R.X1 = Xe; R.Y1 = Ye;
			Out.push_back(R);
		}
		return Out;
	}

	// One block of the house footprint and its game type (an index into HouseTypes).
	struct FHouseBlock
	{
		FCellRect R;
		int Type = 0;
	};
	// client/structures.js layout kinds (undefined is House).
	enum class EHouseLook : unsigned char { House, Church, Barn, Block, Factory, Mine };
	// One house as client/structures.js layout gives it: its rectangle, look and type (0 timber, 1 brick, 2 stone).
	struct FHouseRect
	{
		FCellRect R;
		EHouseLook Look = EHouseLook::House;
		int Type = 0;
	};

	// client/structures.js blocksIn over the footprint (Foot[c] == 1): the rectangles of the footprint cells in Cells
	// (ascending, whole connected blocks; null: all of it), each with its block's type, in scan order of their corner.
	template <typename NamedFn>
	std::vector<FHouseBlock> BlocksIn(const std::vector<unsigned char>& Foot, int W, int H, NamedFn Named, const std::vector<int>* Cells)
	{
		std::vector<FHouseBlock> Out;
		const int N = W * H;
		if (N <= 0 || (int)Foot.size() != N || (Cells && Cells->empty())) return Out;
		std::vector<int> KindOf((size_t)N, -2);
		HouseKindsWhere([&Foot](int C) { return Foot[(size_t)C] == 1; }, N, W, Named, Cells,
			[&KindOf](const std::vector<int>& Q, int Kind) { for (const int C : Q) KindOf[(size_t)C] = Kind; });
		int X0 = 0, Y0 = 0, X1 = W, Y1 = H;
		if (Cells)
		{
			X0 = W; X1 = 0;
			for (const int C : *Cells) { X0 = std::min(X0, C % W); X1 = std::max(X1, C % W + 1); }
			Y0 = Cells->front() / W;
			Y1 = Cells->back() / W + 1;
		}
		// with Cells, the cells it names are exactly the ones the block walk above typed
		const std::vector<FCellRect> Rects = Rectangles(X0, Y0, X1, Y1, [&](int X, int Y)
		{
			const size_t C = (size_t)Y * (size_t)W + (size_t)X;
			return Foot[C] == 1 && (!Cells || KindOf[C] != -2);
		});
		for (const FCellRect& R : Rects)
		{
			FHouseBlock B;
			B.R = R;
			B.Type = KindOf[(size_t)R.Y0 * (size_t)W + (size_t)R.X0];
			Out.push_back(B);
		}
		return Out;
	}

	// client/structures.js relayBlocks: the footprint cells among Changed whose footing (IsFoot) flipped are flipped in
	// Foot, and the blocks of every connected footprint a flipped cell touched, before or after, are redone; the rest
	// are kept. False (nothing changed) when no cell flipped.
	template <typename IsFootFn, typename NamedFn>
	bool RelayBlocks(std::vector<unsigned char>& Foot, int W, int H, const std::vector<int>& Changed, IsFootFn IsFoot, NamedFn Named, std::vector<FHouseBlock>& Blocks)
	{
		const int N = W * H;
		std::vector<int> Flips;
		for (const int C : Changed) if (C >= 0 && C < N && Foot[(size_t)C] != (IsFoot(C) ? 1 : 0)) Flips.push_back(C);
		if (Flips.empty()) return false;
		auto Reach = [&](std::vector<unsigned char>& Mark, std::vector<int>& List)
		{
			Mark.assign((size_t)N, 0);
			List.clear();
			auto Add = [&](int Nb, int From)
			{
				if (Nb >= 0 && Nb < N && Foot[(size_t)Nb] == 1 && !Mark[(size_t)Nb] && (std::abs(Nb - From) != 1 || Nb / W == From / W)) { Mark[(size_t)Nb] = 1; List.push_back(Nb); }
			};
			for (const int C : Flips) { const int Near[5] = { C, C - 1, C + 1, C - W, C + W }; for (const int Nb : Near) Add(Nb, C); }
			for (size_t I = 0; I < List.size(); I++) { const int Cur = List[I]; const int Near[4] = { Cur - 1, Cur + 1, Cur - W, Cur + W }; for (const int Nb : Near) Add(Nb, Cur); }
		};
		std::vector<unsigned char> Before, After;
		std::vector<int> BeforeList, AfterList;
		Reach(Before, BeforeList);
		for (const int C : Flips) Foot[(size_t)C] ^= 1;
		Reach(After, AfterList);
		std::sort(AfterList.begin(), AfterList.end());
		std::vector<FHouseBlock> Next;
		for (const FHouseBlock& B : Blocks) if (!Before[(size_t)B.R.Y0 * (size_t)W + (size_t)B.R.X0]) Next.push_back(B);
		if (!AfterList.empty()) { const std::vector<FHouseBlock> Made = BlocksIn(Foot, W, H, Named, &AfterList); Next.insert(Next.end(), Made.begin(), Made.end()); }
		std::sort(Next.begin(), Next.end(), [W](const FHouseBlock& A, const FHouseBlock& B) { return A.R.Y0 * W + A.R.X0 < B.R.Y0 * W + B.R.X0; });
		Blocks = std::move(Next);
		return true;
	}

	// client/structures.js layout: churches (the biggest long stone blocks, one more per 25, 40 cells apart), barns, the
	// map's landmarks and terraces: the pieces drawn as one house each.
	inline std::vector<FHouseRect> Layout(const std::vector<FHouseBlock>& Blocks)
	{
		struct FWork { FCellRect R; EHouseLook Look; bool bKind; int Type; };
		std::vector<FWork> Rects;
		Rects.reserve(Blocks.size());
		for (const FHouseBlock& B : Blocks) Rects.push_back(FWork{ B.R, EHouseLook::House, false, B.Type });
		auto Short = [](const FCellRect& R) { return std::min(R.X1 - R.X0, R.Y1 - R.Y0); };
		auto Long = [](const FCellRect& R) { return std::max(R.X1 - R.X0, R.Y1 - R.Y0); };
		auto Area = [](const FCellRect& R) { return (R.X1 - R.X0) * (R.Y1 - R.Y0); };
		if (Rects.size() >= 4)
		{
			const size_t Want = 1 + Rects.size() / 25;
			std::vector<FWork*> Cand, Churches;
			for (FWork& R : Rects) { const int S = Short(R.R), L = Long(R.R); if (R.Type == 2 && S >= 3 && S <= 4 && L > S) Cand.push_back(&R); }
			std::stable_sort(Cand.begin(), Cand.end(), [&](const FWork* A, const FWork* B)
			{
				const int Da = Area(A->R), Db = Area(B->R);
				if (Da != Db) return Da > Db;
				return Rnd(A->R.X0, A->R.Y0, 7) < Rnd(B->R.X0, B->R.Y0, 7);
			});
			for (FWork* R : Cand)
			{
				if (Churches.size() >= Want) break;
				bool bFar = true;
				for (const FWork* C : Churches) { const int Dx = C->R.X0 - R->R.X0, Dy = C->R.Y0 - R->R.Y0; if (Dx * Dx + Dy * Dy < 1600) bFar = false; }
				if (bFar) { R->Look = EHouseLook::Church; R->bKind = true; Churches.push_back(R); }
			}
		}
		for (FWork& R : Rects) if (R.Type == 0 && Short(R.R) >= 3 && Rnd(R.R.X0, R.R.Y0, 8) < 0.5) { R.Look = EHouseLook::Barn; R.bKind = true; }
		for (FWork& R : Rects) if (R.Type > 2)
		{
			R.Look = R.Type == 3 ? EHouseLook::Church : R.Type == 4 ? EHouseLook::Factory : R.Type == 5 ? EHouseLook::Mine : EHouseLook::Block;
			R.bKind = true;
			R.Type = 2;
		}
		std::vector<FHouseRect> Out;
		for (FWork& R : Rects)
		{
			const int S = Short(R.R), L = Long(R.R);
			const bool bAlongX = R.R.X1 - R.R.X0 >= R.R.Y1 - R.R.Y0, bWorks = R.Look == EHouseLook::Factory || R.Look == EHouseLook::Mine;
			if (S >= 5 && !bWorks) { R.Look = EHouseLook::Block; R.bKind = true; }
			const bool bBlock = R.Look == EHouseLook::Block;
			std::vector<int> Parts;
			if (bWorks || R.Look == EHouseLook::Church || R.Look == EHouseLook::Barn || L < 4 || (bBlock && L < 6) || (!bBlock && L == 4 && Rnd(R.R.X0, R.R.Y0, 11) < 0.4)) Parts.push_back(L);
			else for (int Left = L, I = 0; Left > 0; I++)
			{
				const double Q = Rnd(R.R.X0 + I, R.R.Y0, 12);
				const int K = bBlock ? (Left <= 5 ? Left : Left == 6 ? 3 : Left == 7 ? 3 + (Q < 0.5 ? 1 : 0) : 3 + (int)std::floor(Q * 3))
					: (Left <= 3 ? Left : Left == 4 ? 2 : 2 + (Q < 0.5 ? 1 : 0));
				Parts.push_back(K);
				Left -= K;
			}
			int O = 0;
			for (const int K : Parts)
			{
				FHouseRect H;
				H.R = R.R;
				if (bAlongX) { H.R.X0 = R.R.X0 + O; H.R.X1 = R.R.X0 + O + K; }
				else { H.R.Y0 = R.R.Y0 + O; H.R.Y1 = R.R.Y0 + O + K; }
				H.Look = R.Look;
				H.Type = R.Type;
				Out.push_back(H);
				O += K;
			}
		}
		return Out;
	}

	// ---------- colours (client/structures.js lin, mul, pick and the house palettes) ----------
	// A linear RGB colour; instance tints go above 1 (the reference multiplies its textures by them).
	struct FRgb
	{
		double R = 1, G = 1, B = 1;
	};
	inline FRgb Rgb(double R, double G, double B) { FRgb C; C.R = R; C.G = G; C.B = B; return C; }
	inline FRgb Grey(double V) { return Rgb(V, V, V); }
	inline FRgb Mul(const FRgb& C, double K) { return Rgb(C.R * K, C.G * K, C.B * K); }
	inline bool SameRgb(const FRgb& A, const FRgb& B) { return A.R == B.R && A.G == B.G && A.B == B.B; }
	// three.js ColorManagement: new THREE.Color(hex) reads sRGB and keeps linear (SRGBToLinear)
	inline double SrgbToLinear(double C) { return C < 0.04045 ? C * 0.0773993808 : std::pow(C * 0.9478672986 + 0.0521327014, 2.4); }
	inline FRgb Lin(unsigned Hex) { return Rgb(SrgbToLinear(((Hex >> 16) & 255) / 255.0), SrgbToLinear(((Hex >> 8) & 255) / 255.0), SrgbToLinear((Hex & 255) / 255.0)); }
	template <size_t N>
	const FRgb& PickOf(const FRgb (&List)[N], double R) { const size_t I = (size_t)std::floor(R * N); return List[I < N - 1 ? I : N - 1]; }

	struct FPalettes
	{
		FRgb Plaster[6] = { Rgb(1, 0.95, 0.85), Rgb(1.06, 0.9, 0.66), Rgb(1, 0.86, 0.8), Rgb(0.95, 0.95, 0.95), Rgb(1.02, 0.92, 0.74), Rgb(0.9, 0.9, 0.86) };
		FRgb Roofs[5] = { Rgb(1, 1, 1), Rgb(0.86, 0.72, 0.62), Rgb(1.08, 0.86, 0.72), Rgb(0.62, 0.64, 0.7), Rgb(0.95, 0.8, 0.66) };
		FRgb Trim[6] = { Lin(0x7d8f6a), Lin(0x6a7d8f), Lin(0x7a3a30), Lin(0x6b4e32), Lin(0x4f7a78), Lin(0xd8cfb0) };
		FRgb Barn[3] = { Rgb(1.15, 0.62, 0.48), Rgb(0.78, 0.76, 0.74), Rgb(0.95, 0.8, 0.62) };
		FRgb Stone[4] = { Rgb(1, 0.97, 0.9), Rgb(0.9, 0.88, 0.84), Rgb(1.06, 1, 0.88), Rgb(0.95, 0.9, 0.82) };
		FRgb Brick[5] = { Rgb(1, 1, 1), Rgb(0.86, 0.8, 0.78), Rgb(1.1, 1, 0.86), Rgb(0.8, 0.72, 0.7), Rgb(1.05, 0.9, 0.84) };
		FRgb Slate[3] = { Rgb(0.6, 0.92, 1.6), Rgb(0.52, 0.8, 1.4), Rgb(0.66, 0.95, 1.5) };
		FRgb Shingle[3] = { Rgb(0.6, 0.8, 1.15), Rgb(0.5, 0.68, 1.0), Rgb(0.68, 0.86, 1.2) };
		FRgb Shed[4] = { Rgb(1.5, 1.32, 1.12), Rgb(1.25, 1.25, 1.25), Rgb(1.6, 1.0, 0.8), Rgb(1.3, 1.1, 0.85) };
		FRgb BagTints[4] = { Rgb(1, 1, 1), Rgb(0.92, 0.9, 0.84), Rgb(1.05, 1.02, 0.92), Rgb(0.92, 0.96, 0.82) };
		FRgb Dressed = Rgb(0.78, 0.74, 0.64);
		FRgb BrickDust = Rgb(1.12, 0.66, 0.52);
		FRgb Dark = Lin(0x1e1c1a);
		FRgb Church = Rgb(1.04, 1, 0.92);
	};
	inline const FPalettes& Palettes() { static const FPalettes P; return P; }

	// client/structures.js spec: one house's look from its own corner cell, shared by the standing house and its ruins.
	struct FHouseSpec
	{
		EHouseLook Look = EHouseLook::House;
		bool bChurch = false, bBarn = false, bBlock = false, bFactory = false, bMine = false;
		bool bWood = false, bShed = false, bStone = false, bBrick = false, bShutters = false;
		int Storeys = 1;
		double H = 3.6;  // wall height above the base, metres
		double K = 1.0;  // roof pitch: ridge height = 0.4 * span * K
		FRgb Tint, Roof, Trim;
		int X0 = 0, Y0 = 0;
		double T(int Key) const { return Rnd(X0, Y0, Key); }
	};
	inline FHouseSpec SpecOf(const FHouseRect& R)
	{
		const FPalettes& Pal = Palettes();
		FHouseSpec Sp;
		Sp.X0 = R.R.X0; Sp.Y0 = R.R.Y0;
		Sp.Look = R.Look;
		const int Type = R.Type;
		Sp.bChurch = R.Look == EHouseLook::Church; Sp.bBarn = R.Look == EHouseLook::Barn; Sp.bFactory = R.Look == EHouseLook::Factory;
		Sp.bMine = R.Look == EHouseLook::Mine; Sp.bBlock = R.Look == EHouseLook::Block || Sp.bFactory;
		Sp.bWood = Type == 0; Sp.bStone = Type == 2 && !Sp.bFactory && !Sp.bMine; Sp.bShed = Sp.bWood && !Sp.bBarn;
		Sp.bBrick = Sp.bFactory || Sp.bMine || (Type == 1 && !Sp.bBlock && Sp.T(25) < 0.5);
		Sp.Storeys = Sp.bChurch || Sp.bWood || Sp.bMine ? 1 : Sp.bBlock ? 2 + (int)std::floor(Sp.T(9) * 2) : Sp.bStone || Sp.T(1) < 0.55 ? 2 : 1;
		Sp.H = Sp.bChurch ? 6.2 : Sp.bMine ? 5.2 : Sp.bBarn ? 4.4 + 0.6 * Sp.T(2) : Sp.bShed ? 2.5 + 0.5 * Sp.T(2) : Sp.bBlock ? Sp.Storeys * 3.1 + 0.5
			: Sp.Storeys == 2 ? (Sp.bStone ? 6.1 : 5.6) + 0.9 * Sp.T(2) : 3.6 + 0.8 * Sp.T(2);
		Sp.K = Sp.bChurch ? 1.25 : Sp.bBarn ? 1.3 : Sp.bShed ? 0.7 + 0.25 * Sp.T(10) : 0.85 + 0.3 * Sp.T(10);
		Sp.Tint = Sp.bBarn ? PickOf(Pal.Barn, Sp.T(4)) : Sp.bShed ? PickOf(Pal.Shed, Sp.T(4)) : Sp.bChurch ? Pal.Church : Sp.bStone ? PickOf(Pal.Stone, Sp.T(4))
			: Sp.bBrick ? PickOf(Pal.Brick, Sp.T(4)) : PickOf(Pal.Plaster, Sp.T(4));
		Sp.Roof = Sp.bWood ? PickOf(Pal.Shingle, Sp.T(5)) : Sp.bStone ? PickOf(Pal.Slate, Sp.T(5)) : PickOf(Pal.Roofs, Sp.T(5));
		Sp.Trim = PickOf(Pal.Trim, Sp.T(6));
		Sp.bShutters = Type == 1 && !Sp.bBlock && Sp.T(18) < (Sp.bBrick ? 0.5 : 0.7);
		return Sp;
	}

	// client/structures.js frame: S runs across the ridge, A along it; At(S, A) is world x, z. Base: the lowest ground
	// at the rectangle's cell corners.
	struct FHouseFrame
	{
		double L = 0, S = 0, Base = 0, Yaw = 0, Cx = 0, Cz = 0;
		bool bAlongX = false;
		void At(double Sv, double Av, double& OutX, double& OutZ) const
		{
			if (bAlongX) { OutX = Cx + Av; OutZ = Cz - Sv; }
			else { OutX = Cx + Sv; OutZ = Cz + Av; }
		}
	};
	template <typename GroundFn>
	FHouseFrame FrameOf(const FCellRect& R, GroundFn Ground)
	{
		FHouseFrame F;
		const int Nx = R.X1 - R.X0, Ny = R.Y1 - R.Y0;
		F.bAlongX = Nx > Ny || (Nx == Ny && Rnd(R.X0, R.Y0, 3) < 0.5);
		F.L = (F.bAlongX ? Nx : Ny) * CellMetres;
		F.S = (F.bAlongX ? Ny : Nx) * CellMetres;
		F.Cx = (R.X0 + R.X1) / 2.0 * CellMetres;
		F.Cz = (R.Y0 + R.Y1) / 2.0 * CellMetres;
		double Base = 1e300;
		for (int Y = R.Y0; Y <= R.Y1; Y++) for (int X = R.X0; X <= R.X1; X++) Base = std::min(Base, (double)Ground(X * CellMetres, Y * CellMetres));
		F.Base = Base;
		F.Yaw = F.bAlongX ? Pi / 2 : 0.0;
		return F;
	}

	// ---------- pieces (client/structures.js KINDS and put) ----------
	// One InstancedMesh per kind in the reference, one imported mesh per kind here (Content/Models/Meshes/piece_*).
	enum class EPiece : unsigned char
	{
		Wall, Stone, Wood, Brick, Dark, Gable, GableWood, GableStone, GableBrick, Roof, Spire, Window, Pane, Paint,
		Broken, BrokenStone, BrokenBrick, BrokenWood, Earth, Shrub, Leaves, Bag, Chunk, Hedgehog, Steel, Wire, Rail, Duck, Count
	};
	inline constexpr int PieceCount = (int)EPiece::Count;
	inline const char* PieceName(EPiece K)
	{
		static const char* const Names[PieceCount] = { "wall", "stone", "wood", "brick", "dark", "gable", "gableWood", "gableStone", "gableBrick", "roof", "spire",
			"window", "pane", "paint", "broken", "brokenStone", "brokenBrick", "brokenWood", "earth", "shrub", "leaves", "bag", "chunk", "hedgehog", "steel", "wire", "rail", "duck" };
		return (int)K < PieceCount ? Names[(int)K] : "?";
	}
	inline int PieceOfName(const char* Name)
	{
		for (int I = 0; I < PieceCount; I++)
		{
			const char* A = PieceName((EPiece)I);
			const char* B = Name;
			while (*A && *A == *B) { A++; B++; }
			if (!*A && !*B) return I;
		}
		return -1;
	}
	// KINDS pick: houseAt() can click it
	inline bool PiecePick(EPiece K)
	{
		switch (K)
		{
		case EPiece::Wall: case EPiece::Stone: case EPiece::Wood: case EPiece::Brick: case EPiece::Gable: case EPiece::GableWood: case EPiece::GableStone:
		case EPiece::GableBrick: case EPiece::Roof: case EPiece::Spire: case EPiece::Window: case EPiece::Pane: case EPiece::Paint: return true;
		default: return false;
		}
	}
	// KINDS shadow (default yes) and lowShadow (false: not on Low)
	inline bool PieceShadow(EPiece K, bool bLow)
	{
		if (K == EPiece::Window || K == EPiece::Pane || K == EPiece::Paint || K == EPiece::Wire || K == EPiece::Duck) return false;
		return !(bLow && (K == EPiece::Bag || K == EPiece::Chunk || K == EPiece::Rail));
	}

	// One piece as client/structures.js put() places it: centre, size (x across, y up, z along the yaw), yaw, tilts and
	// a linear tint; Cell is the cell whose fog dims it and whose knowledge it stands on (the cell built, or the house
	// cell nearest it).
	struct FPiece
	{
		EPiece Kind = EPiece::Stone;
		int Cell = -1;
		float X = 0, Y = 0, Z = 0, Sx = 1, Sy = 1, Sz = 1, Ry = 0, Rx = 0, Rz = 0;
		float R = 1, G = 1, B = 1;
	};
	// client/structures.js compose: scale, then the tilts (Euler 'YXZ': rx, ry, rz), then move; a three.js Matrix4's
	// column-major elements.
	inline void ComposeMatrix(double X, double Y, double Z, double Sx, double Sy, double Sz, double Ry, double Rx, double Rz, double Out[16])
	{
		const double C1 = std::cos(Rx / 2), C2 = std::cos(Ry / 2), C3 = std::cos(Rz / 2), S1 = std::sin(Rx / 2), S2 = std::sin(Ry / 2), S3 = std::sin(Rz / 2);
		const double Qx = S1 * C2 * C3 + C1 * S2 * S3, Qy = C1 * S2 * C3 - S1 * C2 * S3, Qz = C1 * C2 * S3 - S1 * S2 * C3, Qw = C1 * C2 * C3 + S1 * S2 * S3;
		const double X2 = Qx + Qx, Y2 = Qy + Qy, Z2 = Qz + Qz, Xx = Qx * X2, Xy = Qx * Y2, Xz = Qx * Z2, Yy = Qy * Y2, Yz = Qy * Z2, Zz = Qz * Z2;
		const double Wx = Qw * X2, Wy = Qw * Y2, Wz = Qw * Z2;
		Out[0] = (1 - (Yy + Zz)) * Sx; Out[1] = (Xy + Wz) * Sx; Out[2] = (Xz - Wy) * Sx; Out[3] = 0;
		Out[4] = (Xy - Wz) * Sy; Out[5] = (1 - (Xx + Zz)) * Sy; Out[6] = (Yz + Wx) * Sy; Out[7] = 0;
		Out[8] = (Xz + Wy) * Sz; Out[9] = (Yz - Wx) * Sz; Out[10] = (1 - (Xx + Yy)) * Sz; Out[11] = 0;
		Out[12] = X; Out[13] = Y; Out[14] = Z; Out[15] = 1;
	}
	inline void PieceMatrix(const FPiece& P, double Out[16]) { ComposeMatrix(P.X, P.Y, P.Z, P.Sx, P.Sy, P.Sz, P.Ry, P.Rx, P.Rz, Out); }
	// The same rotation as a quaternion (x, y, z, w), reference axes.
	inline void PieceQuat(const FPiece& P, double Q[4])
	{
		const double C1 = std::cos(P.Rx / 2.0), C2 = std::cos(P.Ry / 2.0), C3 = std::cos(P.Rz / 2.0), S1 = std::sin(P.Rx / 2.0), S2 = std::sin(P.Ry / 2.0), S3 = std::sin(P.Rz / 2.0);
		Q[0] = S1 * C2 * C3 + C1 * S2 * S3; Q[1] = C1 * S2 * C3 - S1 * C2 * S3; Q[2] = C1 * C2 * S3 - S1 * S2 * C3; Q[3] = C1 * C2 * C3 + S1 * S2 * S3;
	}

	// ---------- pick solids ----------
	struct FP3
	{
		double X = 0, Y = 0, Z = 0;
	};
	inline FP3 P3(double X, double Y, double Z) { FP3 P; P.X = X; P.Y = Y; P.Z = Z; return P; }
	inline FP3 Sub(const FP3& A, const FP3& B) { return P3(A.X - B.X, A.Y - B.Y, A.Z - B.Z); }
	inline double Dot(const FP3& A, const FP3& B) { return A.X * B.X + A.Y * B.Y + A.Z * B.Z; }

	// One convex solid: up to 8 corners and 6 planar faces (corner indices in order around the face), the hull of one
	// pickable piece as its InstancedMesh draws it. Cell: the piece's cell.
	struct FObjShape
	{
		EPiece Kind = EPiece::Stone;
		bool bPick = false;
		int Cell = -1;
		unsigned char NumVerts = 0, NumFaces = 0;
		FP3 V[8];
		unsigned char FaceCount[6] = {};
		unsigned char Face[6][4] = {};
	};
	inline void SetFace(FObjShape& Sh, int F, int A, int B, int C, int D = -1)
	{
		Sh.Face[F][0] = (unsigned char)A; Sh.Face[F][1] = (unsigned char)B; Sh.Face[F][2] = (unsigned char)C;
		Sh.FaceCount[F] = D < 0 ? 3 : 4;
		if (D >= 0) Sh.Face[F][3] = (unsigned char)D;
	}
	inline FP3 Apply(const double M[16], double X, double Y, double Z) { return P3(M[0] * X + M[4] * Y + M[8] * Z + M[12], M[1] * X + M[5] * Y + M[9] * Z + M[13], M[2] * X + M[6] * Y + M[10] * Z + M[14]); }
	// A piece's hull in world metres: a box (BoxGeometry, and the window and pane's own bounds), the gable (GABLE:
	// apex 0.4), the roof (ROOF: its slabs' top, apex 0.4 + ROOF_T) or the spire (PYRAMID).
	inline FObjShape ShapeOf(const FPiece& P)
	{
		double M[16];
		PieceMatrix(P, M);
		FObjShape Sh;
		Sh.Kind = P.Kind; Sh.bPick = PiecePick(P.Kind); Sh.Cell = P.Cell;
		auto Box = [&](double X0, double Y0, double Z0, double X1, double Y1, double Z1)
		{
			Sh.NumVerts = 8; Sh.NumFaces = 6;
			for (int I = 0; I < 8; I++) Sh.V[I] = Apply(M, (I & 1) ? X1 : X0, (I & 2) ? Y1 : Y0, (I & 4) ? Z1 : Z0);
			SetFace(Sh, 0, 0, 2, 6, 4); SetFace(Sh, 1, 1, 3, 7, 5); SetFace(Sh, 2, 0, 1, 5, 4);
			SetFace(Sh, 3, 2, 3, 7, 6); SetFace(Sh, 4, 0, 1, 3, 2); SetFace(Sh, 5, 4, 5, 7, 6);
		};
		auto Prism = [&](double Apex)
		{
			Sh.NumVerts = 6; Sh.NumFaces = 5;
			const double Lx[3] = { -0.5, 0.5, 0.0 }, Ly[3] = { 0.0, 0.0, Apex };
			for (int I = 0; I < 6; I++) Sh.V[I] = Apply(M, Lx[I % 3], Ly[I % 3], I < 3 ? -0.5 : 0.5);
			SetFace(Sh, 0, 0, 1, 2); SetFace(Sh, 1, 3, 4, 5); SetFace(Sh, 2, 0, 1, 4, 3); SetFace(Sh, 3, 1, 2, 5, 4); SetFace(Sh, 4, 0, 2, 5, 3);
		};
		switch (P.Kind)
		{
		case EPiece::Gable: case EPiece::GableWood: case EPiece::GableStone: case EPiece::GableBrick: Prism(0.4); break;
		case EPiece::Roof: Prism(0.428); break;
		case EPiece::Spire:
		{
			Sh.NumVerts = 5; Sh.NumFaces = 5;
			const double Lx[5] = { -0.5, 0.5, 0.5, -0.5, 0.0 }, Ly[5] = { 0, 0, 0, 0, 1 }, Lz[5] = { -0.5, -0.5, 0.5, 0.5, 0.0 };
			for (int I = 0; I < 5; I++) Sh.V[I] = Apply(M, Lx[I], Ly[I], Lz[I]);
			SetFace(Sh, 0, 0, 1, 2, 3); SetFace(Sh, 1, 0, 1, 4); SetFace(Sh, 2, 1, 2, 4); SetFace(Sh, 3, 2, 3, 4); SetFace(Sh, 4, 3, 0, 4);
			break;
		}
		case EPiece::Window: Box(-0.76, -0.595, -0.005, 0.76, 0.55, 0.125); break;
		case EPiece::Pane: Box(-0.45, -0.595, -0.005, 0.45, 0.55, 0.125); break;
		default: Box(-0.5, -0.5, -0.5, 0.5, 0.5, 0.5); break;
		}
		return Sh;
	}
	inline FP3 Centroid(const FObjShape& Sh)
	{
		FP3 C;
		for (int I = 0; I < Sh.NumVerts; I++) { C.X += Sh.V[I].X; C.Y += Sh.V[I].Y; C.Z += Sh.V[I].Z; }
		const double K = Sh.NumVerts ? 1.0 / Sh.NumVerts : 0.0;
		return P3(C.X * K, C.Y * K, C.Z * K);
	}
	// A face's unit normal pointing out of the solid (Newell's method), or zero for a degenerate face.
	inline FP3 FaceNormal(const FObjShape& Sh, int F, const FP3& Center)
	{
		FP3 N;
		const int Cnt = Sh.FaceCount[F];
		for (int I = 0; I < Cnt; I++)
		{
			const FP3& A = Sh.V[Sh.Face[F][I]];
			const FP3& B = Sh.V[Sh.Face[F][(I + 1) % Cnt]];
			N.X += (A.Y - B.Y) * (A.Z + B.Z);
			N.Y += (A.Z - B.Z) * (A.X + B.X);
			N.Z += (A.X - B.X) * (A.Y + B.Y);
		}
		const double Len = std::sqrt(Dot(N, N));
		if (!(Len > 1e-12)) return FP3();
		N = P3(N.X / Len, N.Y / Len, N.Z / Len);
		if (Dot(N, Sub(Sh.V[Sh.Face[F][0]], Center)) < 0) N = P3(-N.X, -N.Y, -N.Z);
		return N;
	}
	// The distance along a ray (Dir of unit length) to where it enters the solid from outside, as a front-face-only
	// raycast (three.js FrontSide) finds it: false when it misses, passes behind, or starts inside.
	inline bool RayHit(const FObjShape& Sh, const FP3& Origin, const FP3& Dir, double& OutT)
	{
		if (!Sh.NumFaces) return false;
		const FP3 Center = Centroid(Sh);
		double TIn = -1e300, TOut = 1e300;
		for (int F = 0; F < Sh.NumFaces; F++)
		{
			const FP3 N = FaceNormal(Sh, F, Center);
			if (N.X == 0 && N.Y == 0 && N.Z == 0) continue;
			const double Denom = Dot(N, Dir), Dist = Dot(N, Sub(Origin, Sh.V[Sh.Face[F][0]]));
			if (std::fabs(Denom) < 1e-12) { if (Dist > 0) return false; continue; }
			const double T = -Dist / Denom;
			if (Denom < 0) TIn = std::max(TIn, T); else TOut = std::min(TOut, T);
			if (TIn > TOut) return false;
		}
		if (TIn < 0 || TIn > TOut) return false;
		OutT = TIn;
		return true;
	}
	// Axis-aligned bounds, for culling.
	struct FBounds
	{
		FP3 Lo = P3(1e300, 1e300, 1e300), Hi = P3(-1e300, -1e300, -1e300);
		bool IsEmpty() const { return Lo.X > Hi.X; }
		void Add(const FObjShape& Sh)
		{
			for (int I = 0; I < Sh.NumVerts; I++)
			{
				Lo.X = std::min(Lo.X, Sh.V[I].X); Lo.Y = std::min(Lo.Y, Sh.V[I].Y); Lo.Z = std::min(Lo.Z, Sh.V[I].Z);
				Hi.X = std::max(Hi.X, Sh.V[I].X); Hi.Y = std::max(Hi.Y, Sh.V[I].Y); Hi.Z = std::max(Hi.Z, Sh.V[I].Z);
			}
		}
		// The ray (unit Dir) meets the box at a distance no greater than MaxT.
		bool RayMeets(const FP3& O, const FP3& D, double MaxT) const
		{
			if (IsEmpty()) return false;
			double T0 = 0, T1 = MaxT;
			const double Os[3] = { O.X, O.Y, O.Z }, Ds[3] = { D.X, D.Y, D.Z }, Ls[3] = { Lo.X, Lo.Y, Lo.Z }, Hs[3] = { Hi.X, Hi.Y, Hi.Z };
			for (int A = 0; A < 3; A++)
			{
				if (std::fabs(Ds[A]) < 1e-15) { if (Os[A] < Ls[A] || Os[A] > Hs[A]) return false; continue; }
				double Ta = (Ls[A] - Os[A]) / Ds[A], Tb = (Hs[A] - Os[A]) / Ds[A];
				if (Ta > Tb) std::swap(Ta, Tb);
				T0 = std::max(T0, Ta); T1 = std::min(T1, Tb);
				if (T0 > T1) return false;
			}
			return true;
		}
	};

	// What the piece builders read about the map: the physical grid (shared/world-layers.js composeWorldCell of ground
	// and object, without mines), the map file's rows (client/main.js lastStart.map.rows), the damage stage, the mine
	// grid, stone bridge spans and World spoil tips, and the ground height. Off the map: char 0 and false.
	class FCellSource
	{
	public:
		virtual ~FCellSource() = default;
		virtual int Width() const = 0;
		virtual int Height() const = 0;
		virtual unsigned char At(int X, int Y) const = 0;
		virtual unsigned char Orig(int X, int Y) const = 0;
		virtual int Stage(int X, int Y) const = 0;
		virtual bool Mine(int X, int Y) const = 0;
		virtual bool StoneDeck(int X, int Y) const = 0;
		virtual bool Spoil(int X, int Y) const = 0;
		virtual double Ground(double X, double Z) const = 0;
		// client/structures.js trenches: hAt there is the carved floor (client/relief.js); a ground that is not carved
		// answers its height less TRENCH_DEPTH
		virtual double TrenchFloor(double X, double Z) const { return Ground(X, Z); }
		// client/gfx.js low
		virtual bool Low() const { return false; }
	};
	inline constexpr double TrenchDepth = 0.9; // client/relief.js TRENCH_DEPTH

	// What a house leaves on its cells for the rubble there (client/structures.js piece: C.tint).
	struct FHouseTint
	{
		FRgb Color, Wall;
		bool bWood = false, bStone = false, bBrick = false;
		bool operator==(const FHouseTint& O) const { return SameRgb(Color, O.Color) && SameRgb(Wall, O.Wall) && bWood == O.bWood && bStone == O.bStone && bBrick == O.bBrick; }
	};
	inline FHouseTint HouseTintOf(const FHouseSpec& Sp)
	{
		FHouseTint T;
		T.Color = Sp.bBrick ? Palettes().BrickDust : Sp.Tint;
		T.Wall = Sp.Tint; T.bWood = Sp.bWood; T.bStone = Sp.bStone; T.bBrick = Sp.bBrick;
		return T;
	}

	// Placement keeps this facade; the source builders live in the separately owned TCStructures module.
	class FPieceBuilder
	{
	public:
		FPieceBuilder(const FCellSource& InSource, std::vector<FPiece>& InOut) : Source(InSource), Out(InOut) {}
		const std::vector<unsigned char>* Foot = nullptr;
		const std::vector<int>* TintIndex = nullptr;
		const std::vector<FHouseTint>* Tints = nullptr;
		void House(const FHouseRect& Rect) { TCStructures::House(Source, Rect, Out, { Foot, TintIndex, Tints }); }
		void Chunk(int X0, int Y0, int X1, int Y1) { TCStructures::Chunk(Source, X0, Y0, X1, Y1, Out, { Foot, TintIndex, Tints }); }
	private:
		const FCellSource& Source;
		std::vector<FPiece>& Out;
	};

	// The house tints of a layout (client/structures.js piece sets C.tint for every house rectangle, standing or not).
	inline void HouseTints(const std::vector<FHouseRect>& Rects, int W, int H, std::vector<int>& OutIndex, std::vector<FHouseTint>& OutTints)
	{
		OutIndex.assign((size_t)W * (size_t)H, -1);
		OutTints.clear();
		for (const FHouseRect& R : Rects)
		{
			const int I = (int)OutTints.size();
			OutTints.push_back(HouseTintOf(SpecOf(R)));
			for (int Y = R.R.Y0; Y < R.R.Y1; Y++) for (int X = R.R.X0; X < R.R.X1; X++) if (X >= 0 && Y >= 0 && X < W && Y < H) OutIndex[(size_t)(Y * W + X)] = I;
		}
	}

	// Every piece of a whole map in the reference's order (client/structures.js rebuild without a change: the houses in
	// layout order, then each 32 x 32 part in turn). Named(c): the CFG.houses index a map building names at c, or -1.
	// For checks against the reference; FTCWorldObjects keeps the same parts and redoes only what a change reaches.
	template <typename NamedFn>
	void BuildAllPieces(const FCellSource& Src, NamedFn Named, std::vector<FPiece>& Out, std::vector<FHouseRect>* OutRects = nullptr)
	{
		const int W = Src.Width(), H = Src.Height();
		std::vector<unsigned char> Foot((size_t)W * (size_t)H, 0);
		for (int Y = 0; Y < H; Y++) for (int X = 0; X < W; X++) if (Src.Orig(X, Y) == 'B' || Src.At(X, Y) == 'B') Foot[(size_t)(Y * W + X)] = 1;
		const std::vector<FHouseRect> Rects = Layout(BlocksIn(Foot, W, H, Named, nullptr));
		std::vector<int> TintIndex;
		std::vector<FHouseTint> Tints;
		HouseTints(Rects, W, H, TintIndex, Tints);
		FPieceBuilder B(Src, Out);
		B.Foot = &Foot; B.TintIndex = &TintIndex; B.Tints = &Tints;
		for (const FHouseRect& R : Rects) B.House(R);
		constexpr int Part = 32;
		const int Cw = (W + Part - 1) / Part, Ch = (H + Part - 1) / Part;
		for (int K = 0; K < Cw * Ch; K++)
		{
			const int X0 = (K % Cw) * Part, Y0 = (K / Cw) * Part;
			B.Chunk(X0, Y0, std::min(W, X0 + Part), std::min(H, Y0 + Part));
		}
		if (OutRects) *OutRects = Rects;
	}
}

#ifndef TC_WORLD_CORE_ONLY

#include "CoreMinimal.h"
#include "UObject/StrongObjectPtr.h"
#include "Client/TCCoverPreview.h"
#include "Client/TCModelLibrary.h"
#include "Client/TCGraphics.h"
#include "World/TCScatter.h"

class AActor;
class UWorld;
class UHierarchicalInstancedStaticMeshComponent;
class UInstancedStaticMeshComponent;
class UMaterialInterface;
class UMaterialInstanceDynamic;
class UStaticMesh;
class UTexture2D;
class FJsonObject;
class FJsonValue;
class FTCMatchState;
class FTCUnitTable;
class ATCTerrain;

// One wreck this client holds (shared/sim.js wreckRow: id, type, owner, x, z, rot, then debris motion while it moves).
struct FTCWreckRow
{
	int32 Id = 0;
	FString Type;
	int32 Owner = -1;
	double X = 0, Z = 0, Rot = 0;
	bool bMoving = false; // a debris body is still flying (row[6]); drawn where the row puts it
};

// A house the pointer is over (client/main.js houseAt): the cell and its centre, reference metres.
struct FTCHousePick
{
	int32 Cell = -1;
	FVector2D Center = FVector2D::ZeroVector;
	double DistanceMetres = 0; // from the ray's origin to the piece it hit
};

// The map's objects as this client knows them (see the file comment). Feed it next to FTCMatchState: ApplyStart after
// a start FTCMatchState accepted, ApplySnapshot after every snapshot, Clear when the match is dropped. Update after the
// ground's Flush each frame lays out the pieces and props the view draws and the pick reads.
class THREECROSSROADS_API FTCWorldObjects
{
public:
	static const TCHAR* const ReferencePin;

	// A start: everything held is replaced from its map and cells, the wrecks are emptied (client/main.js startGame:
	// hulks.clear, lastSnap = null). State is the match state that already took this start (its room, seat, match id
	// and map tell a reconnect from a new match). False, with nothing changed, for a start FTCMatchState would refuse.
	bool ApplyStart(const FJsonObject& M, const FTCMatchState& State);
	// A snapshot: its cells (this side's terrain memory), its wrecks (absent: the last list stays) and its supply nodes
	// (absent: unchanged; props.js setNodes keeps scenery off them).
	void ApplySnapshot(const FJsonObject& M);
	// No match: everything is dropped and the view clears.
	void Clear();
	// Lays out what changed since the last call, on the ground as drawn (call after ATCTerrain::Flush, so heights are
	// current). Table: unused since the wrecks draw their own models (kept for the callers).
	void Update(const ATCTerrain& Ground, const FTCUnitTable* Table);

	bool HasMap() const { return W > 0 && H > 0; }
	int32 MapW() const { return W; }
	int32 MapH() const { return H; }
	bool IsWorld() const { return bWorld; }

	// ---------- cells ----------
	// The terrain character the server sent for a cell (client/main.js terrain.grid), 0 off the map.
	uint8 Char(int32 C) const { return Grid.IsValidIndex(C) ? Grid[C] : 0; }
	// The cell as built, with a known mine's ground and object (terrain.physicalGrid), 0 off the map.
	uint8 Physical(int32 C) const;
	// The per-cell state bytes (client/main.js terrain.state): wear (bits 0-1), burnt (2), damage stage (3-4), scar (5-6).
	TArrayView<const uint8> CellStates() const { return States; }
	uint8 CellState(int32 C) const { return States.IsValidIndex(C) ? States[C] : 0; }
	// A house cell (terrain.grid 'B'): what a garrison order may name.
	bool IsHouseCell(int32 C) const { return Char(C) == 'B'; }
	// The CFG.houses index of a house cell's block (shared/sim.js houseKinds over this client's footprint), -1 if none.
	int32 HouseTypeAt(int32 C) const { return HouseType.IsValidIndex(C) ? HouseType[C] : -1; }
	// The height level of a cell (map.heights, then each cell row's level), 0 off the map.
	int32 LevelAt(int32 X, int32 Y) const { return X >= 0 && Y >= 0 && X < W && Y < H ? Levels[Y * W + X] : 0; }

	// ---------- wrecks and ghosts ----------
	const TArray<FTCWreckRow>& Wrecks() const { return WreckRows; }
	// The same rows as the cover preview reads them (client/cover-preview.js scan: id, x, z), in snapshot order.
	TArrayView<const FTCCoverWreck> CoverWrecks() const;
	// Points the cover preview at the cell states and wrecks held here (FTCCoverSources::CellState, Wrecks). The views
	// stay valid until the next ApplyStart, ApplySnapshot or Clear.
	void FillCoverSources(FTCCoverSources& Out) const;

	// ---------- picking ----------
	// client/main.js houseAt: the nearest pickable piece the ray (Unreal cm, any length) meets, unless the drawn
	// ground is nearer; 0.4 m on along the ray from there is the cell, and it must be a house cell. Ground may be null
	// (nothing hides the pieces then). The pieces are those of the last Update.
	bool PickHouse(const FVector& Origin, const FVector& Dir, const ATCTerrain* Ground, FTCHousePick& Out) const;
	// client/main.js minimapCursor: the house cell under a ground point in reference metres (no ray).
	bool HouseUnder(FVector2D Metres, FTCHousePick& Out) const;
	// client/orders.js dispatch, the garrison part of a right-click on a house: { t: 'garrison', ids, x, z } for the
	// selected own troops (not structures) whose type garrisons (FTCCoverPreview::ReferenceUnitFacts), in the Selected
	// set's order, without queue (the caller adds it or folds this into { t: 'orders', queue, commands } with
	// FTCSupportPowers::HouseFireCommand and a move for the rest). Null when there are none.
	static TSharedPtr<FJsonObject> GarrisonCommand(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, FVector2D HouseCenter, int32* OutCount = nullptr);
	// shared/sim.js unitFlags bits 20-22: the CFG.houses type a garrisoned unit (flag 32) is in, null otherwise.
	static const TCWorldCore::FHouseType* GarrisonHouseType(int32 Flags);

	// ---------- for the view ----------
	// Moves whenever everything must be drawn again (a start or Clear).
	uint32 Generation() const { return Gen; }
	// The last start was a reconnect to the match already held (same room, match, seat, map): what is drawn may stay up
	// while it is rebuilt.
	bool LastStartKept() const { return bLastKept; }
	int32 ChunkCount() const { return ChunksX * ChunksY; }
	int32 ChunksWide() const { return ChunksX; }
	int32 ChunksHigh() const { return ChunksY; }
	// The chunk (32 x 32 cells) a cell's pieces are drawn in, -1 off the map.
	int32 ChunkOfCell(int32 C) const { return C >= 0 && C < W * H ? ChunkOf(C % W, C / W) : -1; }
	// Chunks whose pieces changed since the last call.
	void TakeDirtyChunks(TSet<int32>& Into);
	// Chunks whose props changed since the last call.
	void TakeDirtyPropChunks(TSet<int32>& Into);
	// The wrecks changed since the last call.
	bool TakeWrecksDirty() { const bool B = bWrecksDirty; bWrecksDirty = false; return B; }
	// Every piece drawn in a chunk: its houses (by their corner cell) and its other cells' pieces, in the reference's
	// order within each.
	void ForEachPiece(int32 Chunk, TFunctionRef<void(const TCWorldCore::FPiece&)> Fn) const;
	// Every prop shown in a chunk (by its cell), with where it stands.
	void ForEachProp(int32 Chunk, TFunctionRef<void(const TCScatterCore::FCandidate&, const TCScatterCore::FPlaced&)> Fn) const;
	// Every piece of a chunk as a solid: pickable pieces carry their hull, the rest only their kind and cell.
	void ForEachShape(int32 Chunk, TFunctionRef<void(const TCWorldCore::FObjShape&)> Fn) const;

	// ---------- counts, for logs and checks ----------
	int32 HouseCount() const { return Houses.Num(); }
	int32 ShapeCount() const;     // pieces
	int32 PickableCount() const;  // pieces the house pick can hit
	int32 CandidateCount() const { return (int32)Candidates.size(); }
	int32 PropsShown() const;
	// Cells whose character differs from FTCMatchState's map (0 when both decoded the same messages).
	int32 GridMismatches(const FTCMatchState& State) const;
	// Milliseconds the last Update spent (layout, pieces and props), for the status line.
	double LastUpdateMs() const { return UpdateMs; }
	// Round 15: the Graphics level the pieces and props are laid out at (client/structures.js C.low, props.js gfx.low).
	// A change of level (FTCGraphics::OnChanged) relays every piece and places every prop again at the next Update, as
	// structures.js rebuild() and props.js refresh() do on gfx.onChange.
	bool LaidOutLow() const { return bLow; }
	int32 LevelSwitches() const { return LevelSwitchCount; }

private:
#if WITH_DEV_AUTOMATION_TESTS
	friend struct FTCRows30Access;
#endif
	class FSource;
	class FLive;
	friend class FTCWorldObjectsView;
	struct FHouseEntry
	{
		TCWorldCore::FHouseRect Rect;
		uint64 Key = 0;
		int32 Chunk = 0;
		std::vector<TCWorldCore::FPiece> Pieces;
		TArray<TCWorldCore::FObjShape> Picks;
	};
	static constexpr int32 ChunkCells = 32;

	// One cell row into the grids; its cell, or -1 when FTCMatchState would skip it too.
	int32 ApplyCellRow(const TArray<TSharedPtr<FJsonValue>>& Row);
	void RememberLook(int32 Cell, const FJsonObject& Data);
	uint8 Orig(int32 C) const;
	bool IsFoot(int32 C) const { const uint8 O = Orig(C); return O == 'B' || Physical(C) == 'B'; }
	int32 NamedType(int32 C) const { const int32* K = Named.Find(C); return K ? *K : -1; }
	int32 ChunkOf(int32 X, int32 Y) const { return (Y / ChunkCells) * ChunksX + X / ChunkCells; }
	void MarkChunksNear(int32 C, TSet<int32>& Into) const;
	void RecomputeStoneDecks(TSet<int32>& Changed);
	void RebuildHouseTypes();
	void RebuildChunkBounds(int32 Chunk);
	int32 StoneCells() const;
	// scenery (props.js)
	void BuildCandidates();
	void PlaceProps(const FLive& Live, const TArray<int32>* Cells, int32 RadiusCells);

	int32 W = 0, H = 0, ChunksX = 0, ChunksY = 0;
	bool bWorld = false;
	TArray<uint8> Grid, GroundL, ObjectL, StartChars, States;
	TArray<int8> Levels;
	TMap<int32, int32> Named;       // cell -> CFG.houses index from the map's buildings and World landmarks
	TSet<int32> Landmarks, SpoilCells;
	TArray<int32> StoneBridgeSeeds; // cells of 'stone bridge' buildings
	TArray<uint8> StoneDeckMask;
	int32 NamedCount = 0, LaidNamedCount = -1; // client/structures.js landmarks (map.buildings length)

	TArray<FTCWreckRow> WreckRows;
	TArray<FTCCoverWreck> CoverRows;
	bool bWrecksDirty = false;

	// what changed since the last Update
	TArray<int32> Pending;          // every cell row applied
	TArray<int32> Shaped;           // cells whose type, physical type or level changed (props.js refresh)
	bool bWhole = true;

	// houses (client/structures.js parts: foot, blocks, rects, houses, tint)
	std::vector<unsigned char> Foot;
	std::vector<TCWorldCore::FHouseBlock> Blocks;
	std::vector<TCWorldCore::FHouseRect> Rects;
	std::vector<int> TintIndex;
	std::vector<TCWorldCore::FHouseTint> Tints;
	TArray<FHouseEntry> Houses;
	TArray<int32> HouseType;        // per cell: CFG.houses index, -1
	TArray<TArray<int32>> ChunkHouses;
	TArray<std::vector<TCWorldCore::FPiece>> ChunkPieces;
	TArray<TArray<TCWorldCore::FObjShape>> ChunkPicks;
	TArray<TCWorldCore::FBounds> ChunkPick;
	TSet<int32> DirtyChunks;
	TArray<uint8> NearMask;         // scratch: cells within 2 of a changed cell

	// scenery (client/props.js): the start's spawns and points, World regions, discovery and railway cells, the
	// supply nodes; the candidates and where each stands
	TArray<TCScatterCore::FCellXY> Spawns, Points;
	TArray<uint8> Biome, Discovered;
	TSet<int32> RailCells;
	TArray<TPair<double, double>> Nodes;
	std::vector<TCScatterCore::FCandidate> Candidates;
	std::vector<TCScatterCore::FPlaced> Placed;
	TArray<int32> CandidateAt;      // cell -> candidate index, -1
	TArray<TArray<int32>> ChunkCandidates;
	TSet<int32> DirtyPropChunks;
	TArray<TPair<double, double>> NodesChanged;
	bool bNodesChanged = false, bDiscoveryDue = false;
	double CandidatesBuiltAt = -1e9;
	double UpdateMs = 0;

	uint32 Gen = 0;
	bool bLastKept = false;
	// Graphics Low (round 15)
	bool bLow = false;
	int32 LevelSwitchCount = 0;
	FTCGraphicsWatch GfxWatch;
	FString BuiltRoom, BuiltMap;
	int32 BuiltMatch = -1, BuiltYou = -1;
	bool bBuiltSpectator = false;
};

// Draws FTCWorldObjects with the reference's own models on an actor of its own:
//   pieces and props: one hierarchical instanced static mesh component per kind and per 64 x 64 cell region (built
//   from the imported piece_*, prop_* and foliage_* meshes), each instance carrying its reference tint (and a roof's
//   tile scale) as custom data; the world materials (unreal/Tools/world/make-world-materials.py, /Game/World) multiply
//   the tint and sample a one-texel-per-cell fog texture, so fog flips cost a texture update, not a rebuild;
//   level of detail: the reference's own Low graphics set is the far level (Low drops the grass and every odd-seed
//   prop but crops: those draw only within ScenicDetailMetres), and small trim (windows, panes, painted boards, wire,
//   duckboards, sandbags, rubble chunks) is culled past TrimDetailMetres; the HISM's cluster tree culls the rest;
//   wrecks (client/main.js hulks) and the ghosts of remembered enemy buildings (applyGhosts): the unit models
//   (FTCModelLibrary), wrecks in the reference's wreck colour, ghosts at its 0.35 fade.
// Rebuilds are spread over frames (MaxChunksPerFrame regions at most), so a big change never stalls a frame.
class THREECROSSROADS_API FTCWorldObjectsView
{
	friend class FTCWreckPresentationTest;
	friend class FTCLook30Ghost;
public:
	// Set before the first Draw to isolate structures in review fixtures.
	bool bDrawScatter = true;
	// Spawns the actor that holds the components (once per world) and loads the model catalog and world materials.
	bool Attach(UWorld* World);
	// Destroys that actor (call from the owner's EndPlay; the world's teardown also takes it).
	void Detach();
	// Every frame of a match, after FTCWorldObjects::Update: rebuilds at most MaxChunksPerFrame changed regions (all
	// of them again after a start or Clear), updates the fog texture, and redraws wrecks and ghosts when they changed.
	void Draw(FTCWorldObjects& Model, const FTCMatchState& State, const ATCTerrain* Ground);
	// Fog flips (FTCMatchState::FogDirtyCells, before it is reset): their texels are rewritten.
	void MarkFog(const FTCWorldObjects& Model, const TSet<int32>& FogCells);
	// Empties every component now (the match was dropped); the next Draw starts over.
	void Reset();
	// Components holding instances (pieces, props, wrecks and ghosts), for the status line.
	int32 SectionsDrawn() const;
	// Triangles of every instance drawn at full detail (instances times their mesh's triangles), for the status line.
	int32 TrianglesDrawn() const;
	int32 Queued() const { return Queue.Num(); }
	int32 InstancesDrawn() const;
	// A line for the status log: components, instances by layer, materials, fog, ghosts, wrecks.
	FString StatusLine() const;

	// Round 15: whether a piece or prop kind casts shadows at a level (structures.js flush: shadow and lowShadow;
	// props.js refresh: never grass and crops, nothing on Low). The view sets this on its components and again on every
	// change of level (FTCGraphics::OnChanged), so the shadows follow a switch in the middle of a match.
	static bool PieceCastsShadow(TCWorldCore::EPiece Kind, bool bLow) { return TCWorldCore::PieceShadow(Kind, bLow); }
	static bool PropCastsShadow(TCScatterCore::EProp Kind, bool bLow) { return !bLow && Kind != TCScatterCore::EProp::Grass && Kind != TCScatterCore::EProp::Crop; }
	// Components whose shadow flag the last change of level turned (for logs and tests), and the level they show.
	int32 ShadowFlagsTurned() const { return ShadowTurned; }
	bool ShadowsLow() const { return bShadowsLow; }

	int32 MaxChunksPerFrame = 8;
	float ScenicDetailMetres = 190.f;
	float TrimDetailMetres = 380.f;
	bool bShadows = true;

private:
	struct FKindAsset
	{
		UStaticMesh* Mesh = nullptr;
		TArray<UMaterialInterface*> Materials;
		int32 Triangles = 0;
		bool bShadow = true, bTrim = false, bLoaded = false, bWorldMaterials = false;
	};
	struct FComp
	{
		TWeakObjectPtr<UHierarchicalInstancedStaticMeshComponent> Comp;
		int32 Count = 0;
		int32 Triangles = 0;
	};
	void LoadAssets();
	void ApplyShadowLevel(bool bLow);
	const FKindAsset& PieceAsset(TCWorldCore::EPiece Kind);
	const FKindAsset& PropAsset(TCScatterCore::EProp Kind);
	bool MaterialsFor(const FTCModelPart& Part, FKindAsset& Out);
	UHierarchicalInstancedStaticMeshComponent* Component(uint64 Key, const FKindAsset& Asset, float CullMetres);
	void BuildRegion(const FTCWorldObjects& Model, int32 Region);
	void ClearAll();
	void EnsureFog(const FTCWorldObjects& Model);
	void WriteFog(const FTCWorldObjects& Model, const FTCMatchState& State, const ATCTerrain* Ground, const TSet<int32>* Cells);
	void DrawModels(const FTCWorldObjects& Model, const FTCMatchState& State, const ATCTerrain* Ground);
	int32 RegionOfChunk(const FTCWorldObjects& Model, int32 Chunk) const;

	TWeakObjectPtr<AActor> Actor;
	TCStructures::FBuildingBreaches BuildingBreaches;
	FTCModelLibrary Library;
	FString LibraryError;
	bool bAssetsLoaded = false;
	TArray<FKindAsset> PieceAssets, PropAssets;
	FTCGraphicsWatch GfxWatch;
	bool bShadowsLow = false;
	int32 ShadowTurned = 0;
	// lead fix (slice 12): FKindAsset's raw mesh and material pointers are no garbage collector references. A kind no
	// component used yet (rubble, a crater's piece, first seen mid-match) could find its mesh or dynamic material
	// collected, and SetStaticMesh read freed memory (both live clients crashed at once in a slice 12 match). These
	// hold every loaded mesh and material until Detach.
	TArray<TStrongObjectPtr<UObject>> KeepLoaded;
	TMap<FSoftObjectPath, TWeakObjectPtr<UMaterialInstanceDynamic>> Mids; // world material -> its dynamic instance
	int32 MissingWorldMaterials = 0;
	TMap<uint64, FComp> Comps;
	TSet<int32> Queue;                // regions
	uint32 DrawnGen = 0xffffffffu;
	int32 RegionsX = 0, RegionsY = 0;
	// fog
	TWeakObjectPtr<UTexture2D> FogTexture;
	TArray<uint8> FogBytes;
	TSet<int32> FogPending;
	bool bFogAll = true, bLastReveal = false;
	int32 FogW = 0, FogH = 0;
	// wrecks and ghosts
	TMap<FString, TWeakObjectPtr<UInstancedStaticMeshComponent>> ModelComps;
	TMap<UInstancedStaticMeshComponent*, TWeakObjectPtr<UInstancedStaticMeshComponent>> ModelShadowTwins;
	uint32 GhostHash = 0;
	bool bModelsDirty = true;
	int32 GhostsDrawn = 0, WrecksDrawn = 0, ModelInstances = 0, ModelsMissing = 0;
	double LastBuildMs = 0, WindowBuildMs = 0, WindowLayoutMs = 0, WindowFogMs = 0;
	int32 RegionsBuilt = 0;
};

#endif // TC_WORLD_CORE_ONLY
