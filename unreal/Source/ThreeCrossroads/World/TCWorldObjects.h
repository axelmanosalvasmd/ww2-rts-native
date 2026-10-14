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

	// The piece builders of client/structures.js, line for line. Out receives each put() in call order.
	class FPieceBuilder
	{
	public:
		FPieceBuilder(const FCellSource& InSrc, std::vector<FPiece>& InOut) : Src(InSrc), Out(InOut), W(InSrc.Width()), H(InSrc.Height()) {}
		// the footprint (structures.js C.foot) for isHouse, and the house tints for rubble (cell -> index, -1 none)
		const std::vector<unsigned char>* Foot = nullptr;
		const std::vector<int>* TintIndex = nullptr;
		const std::vector<FHouseTint>* Tints = nullptr;

		// ---------- houses (piece: a standing house, a ruin, or nothing when every cell has fallen) ----------
		void House(const FHouseRect& R)
		{
			int Standing = 0, Cells = 0;
			for (int Y = R.R.Y0; Y < R.R.Y1; Y++) for (int X = R.R.X0; X < R.R.X1; X++) { Cells++; if (Src.At(X, Y) == 'B') Standing++; }
			if (!Standing) return;
			Rect = R.R; bInRect = true;
			const FHouseSpec Sp = SpecOf(R);
			if (Standing < Cells) Ruin(R, Sp); else Standing_(R, Sp);
			bInRect = false;
		}

		// ---------- cells: one 32 x 32 part, builder by builder (rebuild: hedges, walls, trenches, wire, traps, mines,
		// hospitals, rubble, bridges) ----------
		void Chunk(int X0, int Y0, int X1, int Y1)
		{
			CX0 = X0; CY0 = Y0; CX1 = X1; CY1 = Y1;
			Hedges(); Walls(); Trenches(); Wire(); Traps(); Mines(); Hospitals(); Rubble(); Bridges();
		}

	private:
		const FCellSource& Src;
		std::vector<FPiece>& Out;
		int W, H;
		int CX0 = 0, CY0 = 0, CX1 = 0, CY1 = 0;
		int CurCell = -1;
		FCellRect Rect;
		bool bInRect = false;
		static constexpr int Dir4[4][2] = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } };
		static constexpr int Diag[4][2] = { { 1, 1 }, { 1, -1 }, { -1, 1 }, { -1, -1 } };

		unsigned char At(int X, int Y) const { return Src.At(X, Y); }
		double G(double X, double Z) const { return Src.Ground(X, Z); }
		bool Low() const { return Src.Low(); }
		int StageAt(int X, int Y) const { return Src.Stage(X, Y); }
		// structures.js put
		void Put(EPiece Kind, double X, double Y, double Z, double Sx, double Sy, double Sz, double Ry, const FRgb& C, double Rx = 0, double Rz = 0)
		{
			FPiece P;
			P.Kind = Kind;
			if (bInRect)
			{
				// the house cell nearest the piece: its fog dims it
				const int Cx = std::min(Rect.X1 - 1, std::max(Rect.X0, (int)std::floor(X / CellMetres)));
				const int Cz = std::min(Rect.Y1 - 1, std::max(Rect.Y0, (int)std::floor(Z / CellMetres)));
				P.Cell = Cz * W + Cx;
			}
			else P.Cell = CurCell;
			P.X = (float)X; P.Y = (float)Y; P.Z = (float)Z; P.Sx = (float)Sx; P.Sy = (float)Sy; P.Sz = (float)Sz;
			P.Ry = (float)Ry; P.Rx = (float)Rx; P.Rz = (float)Rz;
			P.R = (float)C.R; P.G = (float)C.G; P.B = (float)C.B;
			Out.push_back(P);
		}
		void Put(EPiece Kind, double X, double Y, double Z, double Sx, double Sy, double Sz, double Ry = 0, double Grey_ = 1, double Rx = 0, double Rz = 0)
		{
			Put(Kind, X, Y, Z, Sx, Sy, Sz, Ry, Grey(Grey_), Rx, Rz);
		}
		bool IsHouse(int X, int Y) const
		{
			if (X < 0 || Y < 0 || X >= W || Y >= H || !Foot || (*Foot)[(size_t)(Y * W + X)] != 1) return false;
			const unsigned char C = At(X, Y);
			return C == 'B' || C == 'R';
		}
		bool InRect(const FCellRect& R, int X, int Y) const { return X >= R.X0 && X < R.X1 && Y >= R.Y0 && Y < R.Y1; }

		// a piece in a house frame (frame P)
		void P(const FHouseFrame& F, EPiece Kind, double Sv, double Y, double Av, double Sx, double Sy, double Sz, const FRgb& C, double Ry = 0, double Rx = 0, double Rz = 0)
		{
			double X, Z;
			F.At(Sv, Av, X, Z);
			Put(Kind, X, Y, Z, Sx, Sy, Sz, F.Yaw + Ry, C, Rx, Rz);
		}
		void P(const FHouseFrame& F, EPiece Kind, double Sv, double Y, double Av, double Sx, double Sy, double Sz, double Grey_, double Ry = 0, double Rx = 0, double Rz = 0)
		{
			P(F, Kind, Sv, Y, Av, Sx, Sy, Sz, Grey(Grey_), Ry, Rx, Rz);
		}
		FHouseFrame Frame(const FCellRect& R) const { return FrameOf(R, [this](double X, double Z) { return G(X, Z); }); }

		static EPiece WallKind(const FHouseSpec& Sp) { return Sp.bWood ? EPiece::Wood : Sp.bStone ? EPiece::Stone : Sp.bBrick ? EPiece::Brick : EPiece::Wall; }
		static EPiece JagKind(const FHouseSpec& Sp) { return Sp.bWood ? EPiece::BrokenWood : Sp.bStone ? EPiece::BrokenStone : Sp.bBrick ? EPiece::BrokenBrick : EPiece::Broken; }

		struct FBay { int X, Y, Dx, Dy; bool bLong; double Px, Pz; };
		// exterior wall bays of a rectangle: one per cell edge that faces open ground
		std::vector<FBay> Bays(const FCellRect& R, const FHouseFrame& F) const
		{
			std::vector<FBay> List;
			for (int Y = R.Y0; Y < R.Y1; Y++) for (int X = R.X0; X < R.X1; X++) for (const auto& D : Dir4)
			{
				const int Nx = X + D[0], Ny = Y + D[1];
				if (InRect(R, Nx, Ny) || IsHouse(Nx, Ny)) continue;
				List.push_back(FBay{ X, Y, D[0], D[1], F.bAlongX ? D[1] != 0 : D[0] != 0, (X + 0.5 + D[0] * 0.5) * CellMetres, (Y + 0.5 + D[1] * 0.5) * CellMetres });
			}
			return List;
		}
		void WindowAt(EPiece Kind, const FBay& B, double Y, const FRgb& C, double Sx = 1, double Sy = 1) { Put(Kind, B.Px, Y, B.Pz, Sx, Sy, 1, YawOf(B.Dx, B.Dy), C); }

		// church tower inside one end of the nave, rising through the roof, with belfry openings and a slate spire
		void Tower(const FHouseFrame& F, const FHouseSpec& Sp, double End, double& OutAT, double& OutT)
		{
			const double L = F.L, S = F.S, Base = F.Base, T = std::min(S - 0.8, 3.4), AT = End * (L / 2 - T / 2 - 0.1), Top = Base + Sp.H + 0.4 * S * Sp.K + 3.2;
			P(F, EPiece::Stone, 0, (Base - 1 + Top) / 2, AT, T, Top - Base + 1, T, 1.02);
			P(F, EPiece::Stone, 0, Top + 0.15, AT, T + 0.3, 0.3, T + 0.3, 1.12);
			P(F, EPiece::Spire, 0, Top + 0.3, AT, T - 0.2, 5.2, T - 0.2, 1.0);
			for (const double Sd : { -1.0, 1.0 })
			{
				P(F, EPiece::Paint, Sd * (T / 2 + 0.02), Top - 1.3, AT, 0.08, 1.4, 0.8, Palettes().Dark);
				P(F, EPiece::Paint, 0, Top - 1.3, AT + Sd * (T / 2 + 0.02), 0.8, 1.4, 0.08, Palettes().Dark);
			}
			OutAT = AT; OutT = T;
		}
		// stone quoins up a rendered house's outside corners; corners that meet a neighbour (terraces) get none
		void Quoins(const FCellRect& R, const FHouseFrame& F, double Hh)
		{
			const double S = F.S, L = F.L, Base = F.Base;
			const int Corners[4][2] = { { -1, -1 }, { -1, 1 }, { 1, -1 }, { 1, 1 } };
			for (const auto& Cn : Corners)
			{
				const double Ss = Cn[0], Sa = Cn[1];
				double Wx, Wz;
				F.At(Ss * S / 2, Sa * L / 2, Wx, Wz);
				const int Gx = (int)std::floor(Wx / CellMetres + 0.5), Gz = (int)std::floor(Wz / CellMetres + 0.5);
				bool bParty = false;
				const int Off[4][2] = { { -1, -1 }, { 0, -1 }, { -1, 0 }, { 0, 0 } };
				for (const auto& O : Off)
				{
					const int X = Gx + O[0], Y = Gz + O[1];
					if (!InRect(R, X, Y) && IsHouse(X, Y)) bParty = true;
				}
				if (bParty) continue;
				int K = 0;
				for (double Y = Base + 0.55; Y < Base + Hh - 0.3; K++, Y += 0.44)
				{
					const double Ws = (K & 1) ? 0.62 : 0.36, Wa = (K & 1) ? 0.36 : 0.62;
					P(F, EPiece::Stone, Ss * (S / 2 - Ws / 2 + 0.035), Y + 0.2, Sa * (L / 2 - Wa / 2 + 0.035), Ws, 0.4, Wa, 1.12 + 0.12 * Rnd(Gx * 3 + K, Gz, 160));
				}
			}
		}
		// a works chimney at one end of the roof
		void Chimney(const FHouseFrame& F, const FHouseSpec& Sp)
		{
			const double L = F.L, Base = F.Base, A = (Sp.T(26) < 0.5 ? -1 : 1) * (L / 2 - 1.3), Top = Base + Sp.H + 10 + 3 * Sp.T(27), Mid = (Base + Top) / 2;
			P(F, EPiece::Brick, 0, Mid - 2, A, 1.7, Top - Base - 4, 1.7, Mul(Sp.Tint, 0.82));
			P(F, EPiece::Brick, 0, Top - 2.5, A, 1.45, 5, 1.45, Mul(Sp.Tint, 0.78));
			for (const double Y : { Top - 1.1, Top - 3.2 }) P(F, EPiece::Stone, 0, Y, A, 1.62, 0.22, 1.62, 0.55);
			P(F, EPiece::Paint, 0, Top + 0.02, A, 1.2, 0.06, 1.2, Palettes().Dark);
		}
		// a pit's headframe over the shaft: legs leaning in to a deck, braces, back-stays and the sheave wheels
		void Headframe(const FHouseFrame& F, const FHouseSpec& Sp)
		{
			const double L = F.L, S = F.S, Base = F.Base, End = Sp.T(19) < 0.5 ? -1 : 1, Span = L * 0.4, As = End * (L / 2 - Span / 2), Hw = std::min(S, Span) / 2 - 0.35;
			const double Top = Base + 12.5, Lean = 0.55, LegH = Top - Base, Tilt = std::atan2(Hw * (1 - Lean), LegH);
			const FRgb Steel = Rgb(0.62, 0.36, 0.3);
			for (const double Sx : { -1.0, 1.0 }) for (const double Sz : { -1.0, 1.0 })
				P(F, EPiece::Steel, Sx * Hw * (1 + Lean) / 2, Base + LegH / 2, As + Sz * Hw * (1 + Lean) / 2, 0.28, LegH + 0.3, 0.28, Steel, 0, -Sz * Tilt, Sx * Tilt);
			for (int I = 1; I <= 4; I++)
			{
				const double Y = Base + LegH * I / 4.6, K = Hw * (1 - (1 - Lean) * I / 4.6);
				for (const double S2 : { -1.0, 1.0 })
				{
					P(F, EPiece::Steel, 0, Y, As + S2 * K, 2 * K, 0.16, 0.16, Steel);
					P(F, EPiece::Steel, S2 * K, Y, As, 0.16, 0.16, 2 * K, Steel);
				}
			}
			P(F, EPiece::Steel, 0, Top + 0.1, As, 2 * Hw * Lean + 0.8, 0.25, 2 * Hw * Lean + 0.8, Mul(Steel, 0.8));
			const double Reach = L - Span, Stay = std::hypot(Reach, LegH * 0.9), Pitch = std::atan2(Reach, LegH * 0.9);
			for (const double Sx : { -1.0, 1.0 }) P(F, EPiece::Steel, Sx * Hw * 0.6, Base + LegH * 0.55, As - End * Reach / 2, 0.24, Stay, 0.24, Steel, 0, End * Pitch, 0);
			for (const double Sx : { -0.45, 0.45 }) for (int I = 0; I < 12; I++)
			{
				const double Ang = I / 12.0 * Pi * 2, Rr = 1.25;
				P(F, EPiece::Steel, Sx, Top + 1.5 + std::sin(Ang) * Rr, As + std::cos(Ang) * Rr, 0.12, 0.7, 0.14, Mul(Steel, 0.7), 0, -Ang, 0);
			}
			P(F, EPiece::Steel, 0, Top + 1.5, As, 1.4, 0.2, 0.2, Mul(Steel, 0.6));
		}
		// the winding house of a mine head
		void MineHead(const FCellRect& R, const FHouseSpec& Sp)
		{
			const FHouseFrame F = Frame(R);
			const double L = F.L, S = F.S, Base = F.Base, Hh = Sp.H, K = Sp.K, End = Sp.T(19) < 0.5 ? -1 : 1, Lh = L * 0.6, Ah = -End * (L - Lh) / 2, Oh = 0.35;
			P(F, EPiece::Brick, 0, Base - 1 + (Hh + 1) / 2, Ah, S, Hh + 1, Lh, Sp.Tint);
			P(F, EPiece::Stone, 0, Base - 1.55 + 2.1 / 2, Ah, S + 0.14, 2.1, Lh + 0.14, 0.85);
			P(F, EPiece::GableBrick, 0, Base + Hh, Ah, S, S * K, Lh, Sp.Tint);
			P(F, EPiece::Roof, 0, Base + Hh - 0.8 * K * Oh, Ah, S + 2 * Oh, (S + 2 * Oh) * K, Lh + 2 * Oh, Sp.Roof);
			for (const double S2 : { -1.0, 1.0 }) for (int I = 0; I < 3; I++) P(F, EPiece::Paint, S2 * (S / 2 + 0.03), Base + 2.6, Ah + (I - 1) * Lh / 3.4, 0.06, 2.4, 1.0, Lin(0x5a6470));
			P(F, EPiece::Stone, 0, Base - 0.2, End * (L - Lh) / 2, S - 0.4, 0.5, L - Lh - 0.4, 0.75);
			Headframe(F, Sp);
		}
		void Standing_(const FHouseRect& Rr, const FHouseSpec& Sp)
		{
			const FCellRect& R = Rr.R;
			if (Sp.bMine) { MineHead(R, Sp); return; }
			const FPalettes& Pal = Palettes();
			const FHouseFrame F = Frame(R);
			const double L = F.L, S = F.S, Base = F.Base, Hh = Sp.H, K = Sp.K, Oh = 0.35, Ph = Sp.bShed ? 0.2 : 0.55;
			P(F, WallKind(Sp), 0, Base - 1 + (Hh + 1) / 2, 0, S, Hh + 1, L, Sp.Tint);
			if (!Sp.bStone && !Sp.bWood) Quoins(R, F, Hh);
			P(F, EPiece::Stone, 0, Base - 1.55 + (Ph + 1.55) / 2, 0, S + 0.14, Ph + 1.55, L + 0.14, 0.85);
			if (Sp.bStone && !Sp.bChurch)
				for (int I = 1; I < Sp.Storeys; I++) P(F, EPiece::Paint, 0, Sp.bBlock ? Base + 0.05 + I * 3.1 : Base + Hh / 2 + 0.15, 0, S + 0.12, 0.2, L + 0.12, Pal.Dressed);
			if (Sp.bBlock)
			{
				P(F, EPiece::Stone, 0, Base + Hh - 0.25, 0, S + 0.24, 0.22, L + 0.24, 1.05);
				P(F, EPiece::Paint, 0, Base + Hh + 0.03, 0, S - 0.5, 0.06, L - 0.5, Lin(0x4a4640));
				for (const double Sd : { -1.0, 1.0 })
				{
					P(F, EPiece::Stone, Sd * (S / 2 - 0.15), Base + Hh + 0.25, 0, 0.3, 0.6, L, 0.95);
					P(F, EPiece::Stone, 0, Base + Hh + 0.25, Sd * (L / 2 - 0.15), S - 0.6, 0.6, 0.3, 0.95);
				}
				for (int I = 0; I < 2; I++)
				{
					const double Sv = (Sp.T(20 + I) - 0.5) * (S - 2), Av = (Sp.T(22 + I) - 0.5) * (L - 2);
					P(F, EPiece::Stone, Sv, Base + Hh + 0.6, Av, 0.7, 1.2, 1.1, 0.8);
					P(F, EPiece::Stone, Sv, Base + Hh + 1.26, Av, 0.85, 0.12, 1.25, 0.55);
				}
				if (Sp.bFactory) Chimney(F, Sp);
			}
			else
			{
				P(F, Sp.bWood ? EPiece::GableWood : Sp.bStone ? EPiece::GableStone : Sp.bBrick ? EPiece::GableBrick : EPiece::Gable, 0, Base + Hh, 0, S, S * K, L, Sp.Tint);
				const double Wr = S + 2 * Oh;
				P(F, EPiece::Roof, 0, Base + Hh - 0.8 * K * Oh, 0, Wr, Wr * K, L + 2 * Oh, Sp.Roof);
				if (!Sp.bChurch && !Sp.bWood && Sp.T(13) < 0.8)
				{
					const double Av = (Sp.T(14) < 0.5 ? -1 : 1) * std::max(0.0, L / 2 - 0.9), Sv = (Sp.T(15) < 0.5 ? -1 : 1) * 0.22 * S;
					const double Top = Base + Hh + 0.4 * K * (S - 2 * std::fabs(Sv)) + 1.0 + 0.3 * Sp.T(16), Bot = Base + Hh - 0.3;
					if (Sp.bBrick) P(F, EPiece::Brick, Sv, (Top + Bot) / 2, Av, 0.6, Top - Bot, 0.75, Mul(Sp.Tint, 0.9));
					else P(F, EPiece::Stone, Sv, (Top + Bot) / 2, Av, 0.6, Top - Bot, 0.75, 0.8);
					P(F, EPiece::Stone, Sv, Top + 0.06, Av, 0.78, 0.12, 0.92, 0.5);
					if (Sp.T(16) < 0.5) P(F, EPiece::Paint, Sv, Top + 0.27, Av, 0.16, 0.3, 0.16, Lin(0x7a4a36));
					else for (const double D : { -0.18, 0.18 }) P(F, EPiece::Paint, Sv, Top + 0.27, Av + D, 0.16, 0.3, 0.16, Lin(0x7a4a36));
				}
			}
			const std::vector<FBay> List = Bays(R, F);
			if (Sp.bChurch)
			{
				const double End = Sp.T(19) < 0.5 ? -1 : 1;
				double AT, T;
				Tower(F, Sp, End, AT, T);
				for (const FBay& B : List)
				{
					const double A = F.bAlongX ? B.Px - F.Cx : B.Pz - F.Cz;
					if (B.bLong && std::fabs(A - AT) > T / 2 + 0.4) WindowAt(EPiece::Pane, B, Base + 3.4, Lin(0x8a9098), 0.8, 2.0);
				}
				double Gx, Gz;
				F.At(0, End * (L / 2 + 0.5), Gx, Gz);
				const double Gg = std::max(Base, G(Gx, Gz));
				P(F, EPiece::Paint, 0, Gg + 1.45, End * (L / 2 + 0.02), 1.5, 2.8, 0.1, Lin(0x4a3220));
				P(F, EPiece::Stone, 0, Gg + 0.06, End * (L / 2 + 0.4), 2.2, 0.3, 0.8, 0.9);
				P(F, EPiece::Paint, 0, Base + Hh + 1.0, -End * (L / 2 + 0.03), 1.0, 1.0, 0.08, Lin(0x3a4250), 0, 0, Pi / 4);
				return;
			}
			if (Sp.bBarn)
			{
				const double End = Sp.T(19) < 0.5 ? -1 : 1, AE = End * (L / 2 + 0.02), Dw = std::min(2.8, S - 1.2), Dh = std::min(3.2, Hh - 0.6);
				double Gx, Gz;
				F.At(0, End * (L / 2 + 0.5), Gx, Gz);
				const double Gg = std::max(Base, G(Gx, Gz));
				P(F, EPiece::Paint, 0, Gg + Dh / 2 + 0.05, AE, Dw, Dh, 0.1, Lin(0x4a3626));
				for (const double Sd : { -1.0, 1.0 }) P(F, EPiece::Paint, 0, Gg + Dh / 2 + 0.05, AE + End * 0.04, std::hypot(Dw, Dh) - 0.25, 0.14, 0.08, Lin(0xd8cfb0), 0, 0, Sd * std::atan2(Dh, Dw));
				P(F, EPiece::Paint, 0, Gg + Dh + 0.14, AE + End * 0.04, Dw + 0.2, 0.16, 0.08, Lin(0xd8cfb0));
				P(F, EPiece::Paint, 0, Base + Hh + 0.75, AE, 1.1, 1.0, 0.1, Lin(0x3a2a1c));
				return;
			}
			if (Sp.bShed) { Shed(F, Sp, List); return; }
			// windows on every open bay and storey, a door on a long side
			std::vector<const FBay*> Longs;
			for (const FBay& B : List) if (B.bLong) Longs.push_back(&B);
			const FBay* Door = !Sp.bBlock && !Longs.empty() ? Longs[(size_t)std::floor(Sp.T(17) * Longs.size())] : nullptr;
			for (const FBay& B : List) for (int I = 0; I < Sp.Storeys; I++)
			{
				if (&B == Door && I == 0) continue;
				if (Rnd(B.X * 3 + B.Dx, B.Y * 3 + B.Dy, 30 + I) < 0.15) continue;
				const double Y = Sp.bBlock ? Base + 1.6 + I * 3.1 : I == 0 ? Base + 1.6 : Base + Hh - 1.45;
				WindowAt(Sp.bShutters ? EPiece::Window : EPiece::Pane, B, Y, Sp.Trim);
				if (Sp.bStone || Sp.bBrick) Put(EPiece::Paint, B.Px + B.Dx * 0.02, Y + 0.63, B.Pz + B.Dy * 0.02, 1.02, 0.2, 0.12, YawOf(B.Dx, B.Dy), Pal.Dressed);
			}
			if (Door)
			{
				const FBay& D = *Door;
				const double Yd = YawOf(D.Dx, D.Dy), Gg = std::max(Base, G(D.Px + D.Dx * 0.4, D.Pz + D.Dy * 0.4));
				Put(EPiece::Paint, D.Px, Gg + 1.05, D.Pz, 1.0, 2.0, 0.08, Yd, Mul(Sp.Trim, 0.75));
				Put(EPiece::Paint, D.Px + D.Dx * 0.045, Gg + 1.05, D.Pz + D.Dy * 0.045, 0.05, 1.96, 0.02, Yd, Mul(Sp.Trim, 0.45));
				for (const double Sd : { -1.0, 1.0 }) Put(EPiece::Stone, D.Px - D.Dy * Sd * 0.58, Gg + 1.05, D.Pz + D.Dx * Sd * 0.58, 0.16, 2.1, 0.16, Yd, 1.05);
				Put(EPiece::Stone, D.Px, Gg + 2.16, D.Pz, 1.32, 0.18, 0.18, Yd, 1.05);
				Put(EPiece::Stone, D.Px + D.Dx * 0.3, Gg + 0.06, D.Pz + D.Dy * 0.3, 1.4, 0.3, 0.6, Yd, 0.9);
			}
		}
		// a timber shed's trim: tarred corner posts and wall plates, a ledged and braced plank door, the odd window
		void Shed(const FHouseFrame& F, const FHouseSpec& Sp, const std::vector<FBay>& List)
		{
			const double L = F.L, S = F.S, Base = F.Base, Hh = Sp.H;
			const FRgb Pale = Rgb(1.25, 1.15, 1);
			for (const double Sd : { -1.0, 1.0 })
			{
				for (const double A : { -1.0, 1.0 }) P(F, EPiece::Dark, Sd * (S / 2 - 0.04), Base + Hh / 2, A * (L / 2 - 0.04), 0.2, Hh, 0.2, 0.8);
				P(F, EPiece::Dark, Sd * (S / 2 + 0.01), Base + Hh - 0.11, 0, 0.14, 0.22, L + 0.1, 0.8);
			}
			const FBay* Door = List.empty() ? nullptr : &List[(size_t)std::floor(Sp.T(17) * List.size())];
			for (const FBay& B : List) if (&B != Door && B.bLong && Rnd(B.X * 3 + B.Dx, B.Y * 3 + B.Dy, 30) < 0.4) WindowAt(EPiece::Pane, B, Base + Hh - 1.0, Lin(0x5a4a3a), 0.8, 0.6);
			if (!Door) return;
			const FBay& D = *Door;
			const double Yd = YawOf(D.Dx, D.Dy), Gg = std::max(Base, G(D.Px + D.Dx * 0.4, D.Pz + D.Dy * 0.4)), Dh = std::min(1.9, Hh - 0.45);
			const double X = D.Px + D.Dx * 0.03, Z = D.Pz + D.Dy * 0.03, Bx = D.Px + D.Dx * 0.07, Bz = D.Pz + D.Dy * 0.07;
			Put(EPiece::Dark, X, Gg + 0.05 + Dh / 2, Z, 0.95, Dh, 0.08, Yd, 0.7);
			for (const double Y : { 0.3, Dh - 0.25 }) Put(EPiece::Wood, Bx, Gg + 0.05 + Y, Bz, 0.95, 0.12, 0.06, Yd, Pale);
			Put(EPiece::Wood, Bx, Gg + 0.05 + Dh / 2, Bz, std::hypot(0.85, Dh - 0.55), 0.11, 0.05, Yd, Pale, 0, std::atan2(Dh - 0.55, 0.85));
		}
		// a house with fallen cells: standing cells keep jagged stubs and dropped slabs
		void Ruin(const FHouseRect& Rr, const FHouseSpec& Sp)
		{
			const FCellRect& R = Rr.R;
			std::vector<int> Keep;
			for (int Y = R.Y0; Y < R.Y1; Y++) for (int X = R.X0; X < R.X1; X++) if (At(X, Y) == 'B') Keep.push_back(Y * W + X);
			const EPiece Jag = JagKind(Sp);
			const FRgb Tint = Sp.Tint;
			for (int Y = R.Y0; Y < R.Y1; Y++) for (int X = R.X0; X < R.X1; X++)
			{
				if (At(X, Y) != 'B') continue;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz);
				const int Piles = 2 + (Rnd(X, Y, 40) < 0.45 ? 1 : 0);
				for (int K = 0; K < Piles; K++)
				{
					const double Ang = Rnd(X, Y, 41 + K) * 6.2832, Rad = CellMetres * (0.08 + 0.48 * Rnd(X, Y, 46 + K)), Len = 0.55 + 1.15 * Rnd(X, Y, 52 + K);
					const double Ht = 0.35 + std::min(2.4, Sp.H) * (0.12 + 0.38 * Rnd(X, Y, 58 + K)), Yaw = Ang + (Rnd(X, Y, 64 + K) - 0.5) * 0.8;
					Put(Jag, Cx + std::cos(Ang) * Rad, Gg - 0.12, Cz + std::sin(Ang) * Rad, Len, Ht, 0.26 + 0.12 * Rnd(X, Y, 70 + K), Yaw, Tint, (Rnd(X, Y, 74 + K) - 0.5) * 0.35, (Rnd(X, Y, 78 + K) - 0.5) * 0.25);
				}
				if (Rnd(X, Y, 82) < 0.7)
				{
					const double Ang = Rnd(X, Y, 83) * 6.2832, Rad = CellMetres * (0.1 + 0.4 * Rnd(X, Y, 84)), S = 0.35 + 0.55 * Rnd(X, Y, 85);
					Put(EPiece::Chunk, Cx + std::cos(Ang) * Rad, Gg + S * 0.15, Cz + std::sin(Ang) * Rad, S * 1.3, S * 0.55, S, Rnd(X, Y, 86) * 6.28, Tint, (Rnd(X, Y, 87) - 0.5) * 0.5, (Rnd(X, Y, 88) - 0.5) * 0.4);
				}
				for (int I = 0; I < 4; I++)
				{
					const int Dx = Dir4[I][0], Dy = Dir4[I][1], Nx = X + Dx, Ny = Y + Dy;
					if (std::find(Keep.begin(), Keep.end(), Ny * W + Nx) != Keep.end()) continue; // keep.has(ny * w + nx), as JavaScript indexes it
					if (Rnd(X, Y, 90 + I) < 0.42) continue;
					const double Slide = (Rnd(X, Y, 96 + I) - 0.5) * CellMetres * 0.55, Outw = CellMetres * (0.28 + 0.28 * Rnd(X, Y, 102 + I));
					const double Len = 0.45 + 0.95 * Rnd(X, Y, 108 + I), Ht = 0.4 + 1.5 * Rnd(X, Y, 114 + I), Yaw = YawOf(Dx, Dy) + (Rnd(X, Y, 120 + I) - 0.5) * 0.9;
					Put(Jag, Cx + Dx * Outw - Dy * Slide, Gg - 0.1, Cz + Dy * Outw + Dx * Slide, Len, Ht, 0.28, Yaw, Tint, (Rnd(X, Y, 126 + I) - 0.5) * 0.2, 0);
				}
			}
			if (Sp.bFactory || Sp.bMine)
			{
				// the chimney or the headframe stays up while the cell under its middle does
				const FHouseFrame F = Frame(R);
				const double A = Sp.bFactory ? (Sp.T(26) < 0.5 ? -1 : 1) * (F.L / 2 - 1.3) : (Sp.T(19) < 0.5 ? -1 : 1) * F.L * 0.3;
				double Fx, Fz;
				F.At(0, A, Fx, Fz);
				if (At((int)std::floor(Fx / CellMetres), (int)std::floor(Fz / CellMetres)) == 'B') { if (Sp.bFactory) Chimney(F, Sp); else Headframe(F, Sp); }
			}
			if (Sp.bChurch)
			{
				// the tower stays up while its own cells do
				const FHouseFrame F = Frame(R);
				const double End = Sp.T(19) < 0.5 ? -1 : 1, T = std::min(F.S - 0.8, 3.4), AT = End * (F.L / 2 - T / 2 - 0.1);
				double Tx, Tz;
				F.At(0, AT, Tx, Tz);
				const int X0 = (int)std::floor((Tx - T / 2) / CellMetres), X1 = (int)std::floor((Tx + T / 2 - 0.01) / CellMetres);
				const int Y0 = (int)std::floor((Tz - T / 2) / CellMetres), Y1 = (int)std::floor((Tz + T / 2 - 0.01) / CellMetres);
				bool bUp = true;
				for (int Y = Y0; Y <= Y1; Y++) for (int X = X0; X <= X1; X++) if (At(X, Y) != 'B') bUp = false;
				double A2, T2;
				if (bUp) Tower(F, Sp, End, A2, T2);
			}
		}

		// ---------- rubble ----------
		const FHouseTint* TintAt(int C) const
		{
			if (!TintIndex || !Tints || C < 0 || C >= (int)TintIndex->size()) return nullptr;
			const int I = (*TintIndex)[(size_t)C];
			return I >= 0 ? &(*Tints)[(size_t)I] : nullptr;
		}
		void Rubble()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'R') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz);
				const FHouseTint* House = TintAt(Y * W + X);
				if (Src.Spoil(X, Y))
				{
					// a mine's spoil tip: dark waste in overlapping cones, tallest in the middle of the heap
					int N = 0;
					for (int Dy = -1; Dy <= 1; Dy++) for (int Dx = -1; Dx <= 1; Dx++)
					{
						if (!(Dx || Dy)) continue;
						// spoil.has((y + dy) * w + x + dx): a flat index, as JavaScript reads it
						const int K = (Y + Dy) * W + X + Dx;
						if (K >= 0 && K < W * H && Src.Spoil(K % W, K / W) && At(X + Dx, Y + Dy) == 'R') N++;
					}
					const double S = 3.4 + 1.2 * Rnd(X, Y, 140), Gy = 0.36 + 0.08 * Rnd(X, Y, 142);
					Put(EPiece::Earth, Cx + (Rnd(X, Y, 143) - 0.5) * 0.8, Gg - 0.2, Cz + (Rnd(X, Y, 144) - 0.5) * 0.8, S, 0.7 + 0.32 * N + 0.4 * Rnd(X, Y, 141), S, Rnd(X, Y, 145) * 6.28, Rgb(Gy, Gy * 0.97, Gy * 1.02));
					if (!Low()) Put(EPiece::Chunk, Cx + (Rnd(X, Y, 146) - 0.5) * 1.6, Gg + 0.1, Cz + (Rnd(X, Y, 147) - 0.5) * 1.6, 0.6, 0.35, 0.5, Rnd(X, Y, 148) * 6.28, Rgb(0.5, 0.48, 0.47));
					continue;
				}
				const int N = (Low() ? 2 : 3) + (House && !Low() ? 1 : 0);
				for (int K = 0; K < N; K++)
				{
					if (Rnd(X, Y, K + 3) < 0.22) continue;
					const double Ang = Rnd(X, Y, K + 5) * 6.2832, Rad = CellMetres * 0.62 * Rnd(X, Y, K + 7), S = 0.4 + 0.7 * Rnd(X, Y, K), C = Rnd(X, Y, K + 17);
					const FRgb Tint = House && C < 0.5 ? (House->bWood ? Mul(House->Color, 0.6) : House->Color) : C < 0.78 ? Rgb(0.95, 0.93, 0.9) : Rgb(1.12, 0.66, 0.52);
					Put(EPiece::Chunk, Cx + std::cos(Ang) * Rad, Gg + S * 0.12, Cz + std::sin(Ang) * Rad, S * (1 + 0.4 * Rnd(X, Y, K + 21)), S * (0.45 + 0.3 * Rnd(X, Y, K + 25)), S * (0.9 + 0.4 * Rnd(X, Y, K + 29)),
						Rnd(X, Y, K + 33) * 6.28, Tint, (Rnd(X, Y, K + 37) - 0.5) * 0.6, (Rnd(X, Y, K + 41) - 0.5) * 0.6);
				}
				if (!Low() && Rnd(X, Y, 48) < 0.75)
				{
					const double Ang = Rnd(X, Y, 49) * 6.2832, Rad = CellMetres * (0.48 + 0.32 * Rnd(X, Y, 50)), Len = 0.7 + 1.1 * Rnd(X, Y, 51), Ht = 0.25 + 0.55 * Rnd(X, Y, 52);
					const EPiece Jag = House ? (House->bWood ? EPiece::BrokenWood : House->bStone ? EPiece::BrokenStone : House->bBrick ? EPiece::BrokenBrick : EPiece::Broken) : EPiece::Broken;
					const FRgb Tint = House ? (House->bWood ? Mul(House->Color, 0.7) : House->Color) : Rgb(0.95, 0.93, 0.9);
					Put(Jag, Cx + std::cos(Ang) * Rad, Gg - 0.05, Cz + std::sin(Ang) * Rad, Len, Ht, 0.22, Ang + (Rnd(X, Y, 53) - 0.5) * 0.8, Tint, (Rnd(X, Y, 54) - 0.5) * 0.5, (Rnd(X, Y, 55) - 0.5) * 0.3);
				}
				for (int I = 0; I < 4; I++)
				{
					const int Dx = Dir4[I][0], Dy = Dir4[I][1];
					const unsigned char Nb = At(X + Dx, Y + Dy);
					if (Nb == 'R' || Nb == 'B' || Nb == 'K' || Rnd(X, Y, 50 + I) > (House ? 0.34 : 0.18)) continue;
					const double Slide = (Rnd(X, Y, 62 + I) - 0.5) * CellMetres * 0.7, Outw = CellMetres * (0.35 + 0.28 * Rnd(X, Y, 66 + I));
					const double Len = 0.55 + 0.9 * Rnd(X, Y, 54 + I), Ht = House ? 0.45 + 0.9 * Rnd(X, Y, 58 + I) : 0.3 + 0.45 * Rnd(X, Y, 58 + I);
					const bool bStone = !House || House->bStone;
					Put(bStone ? EPiece::BrokenStone : House->bWood ? EPiece::BrokenWood : House->bBrick ? EPiece::BrokenBrick : EPiece::Broken,
						Cx + Dx * Outw - Dy * Slide, Gg - 0.08, Cz + Dy * Outw + Dx * Slide, Len, Ht, 0.28, YawOf(Dx, Dy) + (Rnd(X, Y, 70 + I) - 0.5) * 0.8,
						bStone ? Grey(0.95) : House->Wall, (Rnd(X, Y, 74 + I) - 0.5) * 0.25, 0);
				}
			}
		}

		// ---------- hedgerows: bocage earth banks topped with shrubs ----------
		void Hedges()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'H') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz), R1 = Rnd(X, Y, 80), Hb = 0.85 + 0.25 * R1;
				const int Hurt = std::min(2, StageAt(X, Y));
				const double Bank = Hb * (Hurt == 0 ? 1.0 : Hurt == 1 ? 0.85 : 0.6);
				Put(EPiece::Earth, Cx, Gg - 0.1, Cz, 2.4, Bank, 2.4, R1 * 6.28, Rgb(0.78, 0.9, 0.6));
				const bool Ex = At(X - 1, Y) == 'H' || At(X + 1, Y) == 'H', Ez = At(X, Y - 1) == 'H' || At(X, Y + 1) == 'H';
				const bool bAlongX = Ex != Ez ? Ex : Rnd(X, Y, 81) < 0.5;
				const double Top = Gg - 0.1 + Bank;
				const int N = Hurt == 2 ? 0 : Hurt == 1 || Low() ? 1 : 2;
				for (int I = 0; I < N; I++)
				{
					const double O = N == 1 ? 0 : (I ? 0.5 : -0.5) + (Rnd(X, Y, 82 + I) - 0.5) * 0.3, Side = (Rnd(X, Y, 84 + I) - 0.5) * 0.2;
					const double Rr = (Low() ? 1.0 : 0.8) + 0.2 * Rnd(X, Y, 86 + I), Wide = Low() ? 1.15 : 1, V = Rnd(X, Y, 88 + I);
					const double Sx = bAlongX ? Cx + O : Cx + Side, Sy = Top + Rr * 0.3, Sz = bAlongX ? Cz + Side : Cz + O;
					const FRgb Tint = Rgb(0.88 + 0.2 * V, 0.95 + 0.15 * Rnd(X, Y, 90 + I), 0.85 + 0.15 * V);
					const double Yaw = (bAlongX ? 0 : Pi / 2) + (V < 0.5 ? Pi : 0) + (V - 0.5) * 0.3;
					Put(EPiece::Shrub, Sx, Sy - Rr * 0.1, Sz, Rr * Wide * 0.82, Rr * 0.75, Rr * Wide * 0.82, V * 6.28, Mul(Tint, 0.75));
					Put(EPiece::Leaves, Sx, Sy, Sz, Rr * (0.9 + 0.3 * V), Rr * (0.85 + 0.35 * Rnd(X, Y, 92 + I)), Rr * Wide, Yaw, Mul(Tint, 0.95));
				}
			}
		}

		// ---------- walls: '#' from the map is a capped stone wall, '#' built in play is a sandbag wall ----------
		void Walls()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != '#') continue;
				CurCell = Y * W + X;
				int Conn[4][2], NConn = 0, DiagOn[4][2], NDiag = 0;
				for (const auto& D : Dir4) if (At(X + D[0], Y + D[1]) == '#') { Conn[NConn][0] = D[0]; Conn[NConn][1] = D[1]; NConn++; }
				for (const auto& D : Diag) if (At(X + D[0], Y + D[1]) == '#' && At(X + D[0], Y) != '#' && At(X, Y + D[1]) != '#') { DiagOn[NDiag][0] = D[0]; DiagOn[NDiag][1] = D[1]; NDiag++; }
				if (Src.Orig(X, Y) == '#') StoneWall(X, Y, Conn, NConn, DiagOn, NDiag);
				else BagWall(X, Y, Conn, NConn, DiagOn, NDiag);
			}
		}
		void StoneWall(int X, int Y, const int (*Conn)[2], int NConn, const int (*DiagOn)[2], int NDiag)
		{
			constexpr double WT = 0.55, WH = 1.0, Sink = 0.3;
			const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz), Tint = 0.85 + 0.25 * Rnd(X, Y, 100), Y0 = Gg - Sink, Cap = Tint * 1.18;
			const int Hurt = std::min(2, StageAt(X, Y));
			const double Kk = Hurt == 0 ? 1.0 : Hurt == 1 ? 0.74 : 0.48;
			auto PutW = [&](double Px, double Py, double Pz, double Sx, double Sy, double Sz, double Yaw, double C)
			{
				if (!Hurt) { Put(EPiece::Stone, Px, Py, Pz, Sx, Sy, Sz, Yaw, C); return; }
				if (Sy != 0.13) Put(EPiece::Stone, Px, Y0 + (Py - Y0) * Kk, Pz, Sx, Sy * Kk, Sz, Yaw, C);
				else if (Hurt < 2) Put(EPiece::Stone, Px, Y0 + (Py - Y0) * Kk, Pz, Sx, Sy, Sz, Yaw, C);
			};
			auto Run = [&](double Ox, double Oz, double Len, double Yaw, double Drop)
			{
				PutW(Cx + Ox, Y0 + WH / 2 - Drop, Cz + Oz, WT, WH, Len, Yaw, Tint);
				PutW(Cx + Ox, Y0 + WH + 0.065 - Drop, Cz + Oz, WT + 0.16, 0.13, Len + 0.02, Yaw, Cap);
			};
			if (NConn == 2 && Conn[0][0] == -Conn[1][0] && Conn[0][1] == -Conn[1][1] && !NDiag) { Run(0, 0, CellMetres, YawOf(Conn[0][0], Conn[0][1]), 0); return; }
			if (!NConn && !NDiag) { Run(0, 0, 1.6, Rnd(X, Y, 101) < 0.5 ? Pi / 2 : 0.0, 0); return; }
			for (int I = 0; I < NConn; I++) Run(Conn[I][0] * (0.5 + WT / 4), Conn[I][1] * (0.5 + WT / 4), 1 - WT / 2, YawOf(Conn[I][0], Conn[I][1]), 0);
			for (int I = 0; I < NDiag; I++) Run(DiagOn[I][0] * 0.5, DiagOn[I][1] * 0.5, std::sqrt(2.0), YawOf(DiagOn[I][0], DiagOn[I][1]), 0.015);
			if (NConn == 1 && !NDiag)
			{
				PutW(Cx, Y0 + (WH + 0.15) / 2, Cz, 0.75, WH + 0.15, 0.75, 0, Tint);
				PutW(Cx, Y0 + WH + 0.215, Cz, 0.9, 0.13, 0.9, 0, Cap);
			}
			else
			{
				PutW(Cx, Y0 + WH / 2, Cz, WT, WH, WT, 0, Tint);
				PutW(Cx, Y0 + WH + 0.07, Cz, WT + 0.16, 0.13, WT + 0.16, 0, Cap);
			}
		}
		void Bags(int X, int Y, int K, double Px, double Py, double Pz, double Yaw, double Len = 0.56)
		{
			const double J = Rnd(X * 7 + K, Y * 5 + K, 110);
			Put(EPiece::Bag, Px + (J - 0.5) * 0.05, Py, Pz + (Rnd(X, Y, 111 + K) - 0.5) * 0.05, Len * (0.95 + 0.1 * J), 0.28, 0.44, Yaw + (J - 0.5) * 0.2, PickOf(Palettes().BagTints, Rnd(X, Y, 112 + K)));
		}
		void BagWall(int X, int Y, const int (*Conn)[2], int NConn, const int (*DiagOn)[2], int NDiag)
		{
			const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz) - 0.04;
			int Dirs[8][2], NDirs = 0;
			for (int I = 0; I < NConn; I++) { Dirs[NDirs][0] = Conn[I][0]; Dirs[NDirs][1] = Conn[I][1]; NDirs++; }
			for (int I = 0; I < NDiag; I++) { Dirs[NDirs][0] = DiagOn[I][0]; Dirs[NDirs][1] = DiagOn[I][1]; NDirs++; }
			if (!NDirs)
			{
				const bool bX = Rnd(X, Y, 113) < 0.5;
				Dirs[0][0] = bX ? 1 : 0; Dirs[0][1] = bX ? 0 : 1; Dirs[1][0] = bX ? -1 : 0; Dirs[1][1] = bX ? 0 : -1; NDirs = 2;
			}
			const int Stage = StageAt(X, Y);
			int K = 0;
			for (int I = 0; I < NDirs; I++)
			{
				const int Dx = Dirs[I][0], Dy = Dirs[I][1];
				const double L = std::hypot((double)Dx, (double)Dy), Ux = Dx / L, Uz = Dy / L, Yaw = YawOf(Ux, Uz) + Pi / 2, Len = 0.56 * L;
				const bool bOwns = Dx > 0 || (Dx == 0 && Dy > 0);
				for (int C = 0; C < 3 - Stage; C++)
				{
					const double AtOwn[2] = { 0.5, 1 }, AtOne[1] = { 0.5 }, AtTwo[2] = { 0.25, 0.75 };
					const double* Ats = C == 1 ? (bOwns ? AtOwn : AtOne) : AtTwo;
					const int NAt = C == 1 ? (bOwns ? 2 : 1) : 2;
					const double Rows2[2] = { -0.2, 0.2 }, Rows1[1] = { 0 };
					const double* Rows = C == 0 ? Rows2 : Rows1;
					const int NRows = C == 0 ? 2 : 1;
					for (int A = 0; A < NAt; A++) for (int Ro = 0; Ro < NRows; Ro++)
					{
						const double Fv = Ats[A], Off = Rows[Ro];
						Bags(X, Y, K++, Cx + Ux * Fv * L - Uz * Off, Gg + 0.14 + C * 0.25, Cz + Uz * Fv * L + Ux * Off, Yaw, Len);
					}
				}
			}
			if (Stage < 2) Bags(X, Y, K++, Cx, Gg + 0.39, Cz, YawOf(Dirs[0][0], Dirs[0][1]) + Pi / 2);
		}

		// ---------- trenches: timber revetments, posts, duckboards, spoil parapets; MG nests get a ring of bags ----------
		void Trenches()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'T') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = Src.TrenchFloor(Cx, Cz), Top = Gg + TrenchDepth, Wall = (Gg + Top) / 2;
				bool Conn[4], bAny = false;
				for (int I = 0; I < 4; I++) { Conn[I] = At(X + Dir4[I][0], Y + Dir4[I][1]) == 'T'; bAny = bAny || Conn[I]; }
				for (int I = 0; I < 4; I++)
				{
					const int Dx = Dir4[I][0], Dy = Dir4[I][1];
					const double Yaw = YawOf(Dx, Dy);
					if (Conn[I])
					{
						for (const double S : { -1.0, 1.0 }) Put(EPiece::Wood, Cx + Dx * 0.75 - Dy * S * 0.5, Wall, Cz + Dy * 0.75 + Dx * S * 0.5, 0.08, TrenchDepth + 0.2, 0.5, Yaw, 0.9);
						if (!Low()) Put(EPiece::Duck, Cx + Dx * 0.5, Gg + 0.19, Cz + Dy * 0.5, 1, 1, 1, Yaw + Pi / 2);
					}
					else
					{
						Put(EPiece::Wood, Cx + Dx * 0.5, Wall, Cz + Dy * 0.5, 1.08, TrenchDepth + 0.2, 0.08, Yaw, 0.9);
						if (At(X + Dx, Y + Dy) != '#') Put(EPiece::Earth, Cx + Dx * 0.88, Top - 0.1, Cz + Dy * 0.88, 2.1, 0.5, 0.7, Yaw, Rgb(0.95, 0.88, 0.78));
					}
				}
				for (const auto& D : Diag) Put(EPiece::Dark, Cx + D[0] * 0.5, Wall + 0.1, Cz + D[1] * 0.5, 0.12, TrenchDepth + 0.4, 0.12, 0, 0.8);
				if (bAny) continue;
				if (!Low()) Put(EPiece::Duck, Cx, Gg + 0.19, Cz, 0.8, 1, 1, Rnd(X, Y, 120) < 0.5 ? 0 : Pi / 2);
				// a lone pit with sandbags around it is an MG nest
				int Fx = 0, Fz = 0, N = 0;
				for (const auto& D : Dir4) if (At(X + D[0], Y + D[1]) == '#') { Fx += D[0]; Fz += D[1]; N++; }
				for (const auto& D : Diag) if (At(X + D[0], Y + D[1]) == '#') { Fx += D[0]; Fz += D[1]; N++; }
				if (N < 3) continue;
				const double A0 = Fx || Fz ? std::atan2((double)Fz, (double)Fx) : 0.0;
				for (int C = 0; C < 2; C++) for (int I = -3; I <= 3 - C; I++)
				{
					const double A = A0 + I * 0.5 + C * 0.25;
					Bags(X, Y, 200 + C * 10 + I, Cx + std::cos(A) * 0.82, Top + 0.1 + C * 0.25, Cz + std::sin(A) * 0.82, -A + Pi / 2, 0.5);
				}
				for (const double S : { -1.0, 1.0 })
				{
					const double Bx = Cx - std::cos(A0) * 0.45 - std::sin(A0) * S * 0.3, Bz = Cz - std::sin(A0) * 0.45 + std::cos(A0) * S * 0.3;
					Put(EPiece::Wood, Bx, Gg + 0.15, Bz, 0.32, 0.3, 0.5, -A0, Rgb(0.7, 0.75, 0.6));
				}
			}
		}

		// ---------- barbed wire: posts and sagging strands from post to post ----------
		void Wire()
		{
			auto IsWire = [this](int X, int Y) { return At(X, Y) == 'X'; };
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (!IsWire(X, Y)) continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz);
				auto Tilt = [X, Y](int K) { return (Rnd(X, Y, K) - 0.5) * 0.14; };
				const int Spans[4][2] = { { 1, 0 }, { 0, 1 }, { 1, 1 }, { 1, -1 } };
				for (const auto& D : Spans)
				{
					if (!IsWire(X + D[0], Y + D[1]) || (D[0] && D[1] && (IsWire(X + D[0], Y) || IsWire(X, Y + D[1])))) continue;
					Put(EPiece::Wire, Cx, Gg, Cz, CellMetres * std::hypot((double)D[0], (double)D[1]), 1, 1, std::atan2((double)-D[1], (double)D[0]));
				}
				bool bNear = false;
				for (const auto& D : Dir4) if (IsWire(X + D[0], Y + D[1])) bNear = true;
				for (const auto& D : Diag) if (IsWire(X + D[0], Y + D[1])) bNear = true;
				if (bNear) Put(EPiece::Dark, Cx, Gg + 0.45, Cz, 0.1, 1.15, 0.1, Rnd(X, Y, 130) * 3, 0.9, Tilt(131), Tilt(132));
				else
				{
					const double A = Rnd(X, Y, 133) * Pi, Ux = std::cos(A), Uz = std::sin(A);
					Put(EPiece::Wire, Cx - Ux * 0.9, Gg, Cz - Uz * 0.9, 1.8, 1, 1, std::atan2(-Uz, Ux));
					for (const int S : { -1, 1 }) Put(EPiece::Dark, Cx + S * Ux * 0.9, Gg + 0.45, Cz + S * Uz * 0.9, 0.1, 1.15, 0.1, A, 0.9, Tilt(134 + S), Tilt(136 + S));
				}
			}
		}

		// ---------- tank traps: two steel hedgehogs per cell ----------
		void Traps()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'Y') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Flip = Rnd(X, Y, 140) < 0.5 ? 1 : -1;
				for (const int S : { -1, 1 })
				{
					const double Px = Cx + S * 0.45, Pz = Cz + S * 0.45 * Flip, Sc = 0.95 + 0.15 * Rnd(X, Y, 141 + S), R = Rnd(X, Y, 143 + S);
					Put(EPiece::Hedgehog, Px, G(Px, Pz), Pz, Sc, Sc, Sc, R * 6.28, Mul(Rgb(1.1, 0.86, 0.72), 0.85 + 0.25 * R));
				}
			}
		}

		// ---------- mines: a dark disc half sunk in the turf (the server only tells this side about its own) ----------
		void Mines()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (!Src.Mine(X, Y)) continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres;
				Put(EPiece::Dark, Cx, G(Cx, Cz) + 0.06, Cz, 0.7, 0.14, 0.7, Rnd(X, Y, 150) * 6.28, 0.7);
			}
		}

		// ---------- Field Hospital: a white ridge tent with a red cross on a board over the ridge ----------
		void Hospitals()
		{
			const FRgb Canvas = Rgb(1.25, 1.22, 1.12), Red = Rgb(0.9, 0.12, 0.1);
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'A') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz);
				Put(EPiece::Wall, Cx, Gg + 0.45, Cz, 1.9, 0.9, 1.9, 0, Canvas);
				Put(EPiece::Gable, Cx, Gg + 0.9, Cz, 1.9, 2.5, 1.9, 0, Canvas);
				Put(EPiece::Dark, Cx, Gg + 1.5, Cz, 0.08, 3, 0.08);
				Put(EPiece::Paint, Cx, Gg + 2.6, Cz, 0.9, 0.9, 0.06, 0, Rgb(1.3, 1.3, 1.25));
				for (const double S : { -1.0, 1.0 })
				{
					Put(EPiece::Paint, Cx, Gg + 2.6, Cz + S * 0.04, 0.62, 0.18, 0.02, 0, Red);
					Put(EPiece::Paint, Cx, Gg + 2.6, Cz + S * 0.04, 0.18, 0.62, 0.02, 0, Red);
				}
			}
		}

		// ---------- bridges: plank deck, edge beams, railings and stone cutwaters on the water sides ----------
		void Bridges()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != '=') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz), Top = Gg + 0.22;
				Put(Src.StoneDeck(X, Y) ? EPiece::Stone : EPiece::Wood, Cx, Top - 0.15, Cz, CellMetres, 0.3, CellMetres, 0, 1.0);
				for (const auto& D : Dir4)
				{
					const unsigned char Nb = At(X + D[0], Y + D[1]);
					if (Nb != 'W' && Nb != 'F') continue;
					const double Yaw = YawOf(D[0], D[1]);
					Put(EPiece::Dark, Cx + D[0] * 0.95, Top - 0.1, Cz + D[1] * 0.95, CellMetres, 0.34, 0.12, Yaw, 0.9);
					Put(EPiece::Rail, Cx + D[0] * 0.9, Top, Cz + D[1] * 0.9, 1, 1, 1, Yaw);
					if ((X + Y) % 2 == 0)
					{
						Put(EPiece::Stone, Cx + D[0], (Gg - 0.6 + Top + 0.3) / 2, Cz + D[1], 1.0, Top + 0.3 - (Gg - 0.6), 1.0, Pi / 4, 0.9);
						Put(EPiece::Stone, Cx + D[0], Top + 0.36, Cz + D[1], 1.1, 0.12, 1.1, Pi / 4, 1.1);
					}
				}
			}
		}
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
public:
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
	uint32 GhostHash = 0;
	bool bModelsDirty = true;
	int32 GhostsDrawn = 0, WrecksDrawn = 0, ModelInstances = 0, ModelsMissing = 0;
	double LastBuildMs = 0, WindowBuildMs = 0, WindowLayoutMs = 0, WindowFogMs = 0;
	int32 RegionsBuilt = 0;
};

#endif // TC_WORLD_CORE_ONLY
