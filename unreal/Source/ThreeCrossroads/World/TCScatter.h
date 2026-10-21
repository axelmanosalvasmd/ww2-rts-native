// The map's scenery props as the pinned reference scatters them (ReferencePin): trees, woods, bushes, meadow grass,
// standing wheat, rocks, fences, haystacks and supply heaps. A port of
//   client/props.js   hash, fraction, level, pathSegments, distanceToSegment, visible, kindFor, candidates
//                     (candidateBatches without its yields), place (the instance matrix and tints) and createProps'
//                     KINDS and meshesFor (which imported mesh draws a kind);
//   client/ground.js  BIOME_LOOKS, fieldCells (fieldBlock, fieldBatches, fieldKeep);
//   shared/sim.js     CELL, CFG.reinforceRadius, CFG.pointRadius, levelOf.
//
// Candidates come from the start's map (and, on a World map, the cells this side has discovered); visible() filters
// them by the live grid, heights and supply nodes, as the browser does. Nothing here reads server state beyond the
// recipient's own start and cells.
//
// The engine-free part (namespace TCScatterCore) compiles alone with TC_WORLD_CORE_ONLY defined, so a standalone
// harness and the Node oracle (unreal/Tools/world/scenery-oracle.mjs) can check it.
#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <string>
#include <vector>

namespace TCScatterCore
{
	inline constexpr double CellMetres = 2.0; // shared/sim.js CELL
	inline constexpr double Pi = 3.14159265358979323846;
	// client/ground.js rnd (the same formula as client/structures.js and main.js, a copy of its own there too)
	inline double Rnd(double X, double Y, double K)
	{
		const double V = std::sin(X * 127.1 + Y * 311.7 + K * 74.7) * 43758.5453;
		return V - std::floor(V);
	}
	// three.js Matrix4.compose(position, a turn about y, scale): column-major elements
	inline void ComposeYaw(double X, double Y, double Z, double Sx, double Sy, double Sz, double Yaw, double Out[16])
	{
		const double Qy = std::sin(Yaw / 2), Qw = std::cos(Yaw / 2), Y2 = Qy + Qy, Yy = Qy * Y2, Wy = Qw * Y2;
		Out[0] = (1 - Yy) * Sx; Out[1] = 0; Out[2] = -Wy * Sx; Out[3] = 0;
		Out[4] = 0; Out[5] = Sy; Out[6] = 0; Out[7] = 0;
		Out[8] = Wy * Sz; Out[9] = 0; Out[10] = (1 - Yy) * Sz; Out[11] = 0;
		Out[12] = X; Out[13] = Y; Out[14] = Z; Out[15] = 1;
	}
	inline constexpr double ReinforceRadius = 15; // shared/sim.js CFG.reinforceRadius
	inline constexpr double PointRadius = 8;      // shared/sim.js CFG.pointRadius

	// props.js KINDS (deciduous draws the broadleaf tree)
	enum class EProp : unsigned char { Deciduous, Poplar, Pine, Bush, Grass, Crop, Rocks, Fence, Haystack, Supplies, Count };
	inline constexpr int PropCount = (int)EProp::Count;
	inline const char* PropName(EProp K)
	{
		static const char* const Names[PropCount] = { "deciduous", "poplar", "pine", "bush", "grass", "crop", "rocks", "fence", "haystack", "supplies" };
		return (int)K < PropCount ? Names[(int)K] : "?";
	}
	inline int PropOfName(const std::string& Name)
	{
		for (int I = 0; I < PropCount; I++) if (Name == PropName((EProp)I)) return I;
		return -1;
	}
	inline bool PropTall(EProp K) { return K == EProp::Deciduous || K == EProp::Poplar || K == EProp::Pine || K == EProp::Fence || K == EProp::Haystack; }
	inline bool PropTree(EProp K) { return K == EProp::Deciduous || K == EProp::Poplar || K == EProp::Pine; }
	// props.js place: leafy kinds take their own shade of green
	inline bool PropLeafy(EProp K) { return K != EProp::Rocks && K != EProp::Fence && K != EProp::Haystack && K != EProp::Supplies; }

	// client/ground.js BIOME_LOOKS: the scenery mix a World Conquest region reads (255: none)
	struct FBiomeLook
	{
		const char* Name;
		double Fields, Trees, Pine, Bush, Rocks, Grass, Hay;
		bool bOrchard, bReeds, bDry;
	};
	inline constexpr int BiomeCount = 6;
	inline const FBiomeLook* BiomeLook(int Biome)
	{
		static const FBiomeLook Looks[BiomeCount] = {
			{ "farmland", 0.5, 0.7, 0.12, 1.2, 0.5, 1, 1, false, false, false },
			{ "pine", 0.03, 3, 0.92, 0.7, 1.5, 0.6, 0, false, false, false },
			{ "marsh", 0.02, 0.45, 0, 1.6, 0.1, 2.6, 0, false, true, false },
			{ "highland", 0.05, 0.5, 0.75, 0.6, 9, 0.9, 0, false, false, false },
			{ "orchard", 0.18, 0.8, 0.05, 1, 0.3, 1.4, 0.4, true, false, false },
			{ "steppe", 0.28, 0.15, 0.2, 0.45, 0.8, 2, 0.6, false, false, true },
		};
		return Biome >= 0 && Biome < BiomeCount ? &Looks[Biome] : nullptr;
	}

	// props.js hash: integer-only placement seeds (Math.imul, >>> on 32 bits)
	inline uint32_t Hash(int X, int Y, int Salt = 0)
	{
		uint32_t N = ((uint32_t)X ^ 0x9e3779b9u) * 0x85ebca6bu ^ ((uint32_t)Y ^ (uint32_t)Salt) * 0xc2b2ae35u;
		N ^= N >> 16; N *= 0x7feb352du;
		N ^= N >> 15; N *= 0x846ca68bu;
		return N ^ (N >> 16);
	}
	inline double Fraction(int X, int Y, int Salt) { return Hash(X, Y, Salt) / 4294967296.0; }
	// shared/sim.js levelOf: '0'..'9' up, 'a'.. down
	inline int LevelOf(unsigned char Ch) { return Ch >= 'a' ? 96 - (int)Ch : Ch >= '0' && Ch <= '9' ? Ch - '0' : 0; }

	struct FCellXY { int X = 0, Y = 0; };
	struct FSegment { double Ax = 0, Az = 0, Bx = 0, Bz = 0; };

	// The start's map as props.js candidates() reads it (lastStart.map; on a World map with its discovered cells).
	struct FScatterMap
	{
		int W = 0, H = 0;
		std::vector<std::string> Rows;      // the map file's rows (a World map: the cells discovered so far)
		std::vector<FCellXY> Spawns;        // map.spawns (null entries left out)
		std::vector<FCellXY> Points;        // map.points
		std::vector<unsigned char> Biome;   // map.biome (empty: none; 255: no region look)
		bool bWorld = false;
		std::vector<unsigned char> Discovered; // map.discovered (World maps)
		char RowAt(int X, int Y) const { return Y >= 0 && Y < (int)Rows.size() && X >= 0 && X < (int)Rows[(size_t)Y].size() ? Rows[(size_t)Y][(size_t)X] : 0; }
		const FBiomeLook* LookAt(int X, int Y) const { const int I = Y * W + X; return Biome.empty() || I < 0 || I >= (int)Biome.size() ? nullptr : BiomeLook(Biome[(size_t)I]); }
	};

	// One placement candidate (props.js candidates list entry).
	struct FCandidate
	{
		EProp Kind = EProp::Grass;
		int Cx = 0, Cy = 0;
		double X = 0, Z = 0;
		uint32_t Seed = 0;
		double Angle = 0, Scale = 1;
		int AnchorX = -1, AnchorY = -1;     // -1: none
		int Biome = 255;                    // 255: none
		bool bOrchard = false, bWood = false;
	};

	// props.js pathSegments: each spawn to its two nearest points, each point to its two nearest others (once)
	inline std::vector<FSegment> PathSegments(const FScatterMap& M)
	{
		std::vector<FSegment> Out;
		auto Add = [&Out](const FCellXY& A, const FCellXY& B) { Out.push_back(FSegment{ (A.X + 0.5) * CellMetres, (A.Y + 0.5) * CellMetres, (B.X + 0.5) * CellMetres, (B.Y + 0.5) * CellMetres }); };
		struct FD { int I; double D; };
		auto Nearest = [&M](const FCellXY& P, int Skip)
		{
			std::vector<FD> L;
			for (int I = 0; I < (int)M.Points.size(); I++) if (I != Skip) L.push_back(FD{ I, std::pow(P.X - M.Points[(size_t)I].X, 2.0) + std::pow(P.Y - M.Points[(size_t)I].Y, 2.0) });
			std::stable_sort(L.begin(), L.end(), [](const FD& A, const FD& B) { return A.D != B.D ? A.D < B.D : A.I < B.I; });
			if (L.size() > 2) L.resize(2);
			return L;
		};
		for (const FCellXY& Sp : M.Spawns) for (const FD& N : Nearest(Sp, -1)) Add(Sp, M.Points[(size_t)N.I]);
		std::vector<long long> Links;
		for (int I = 0; I < (int)M.Points.size(); I++)
			for (const FD& N : Nearest(M.Points[(size_t)I], I))
			{
				const long long Key = (long long)std::min(I, N.I) * 1000000 + std::max(I, N.I);
				if (std::find(Links.begin(), Links.end(), Key) != Links.end()) continue;
				Links.push_back(Key);
				Add(M.Points[(size_t)I], M.Points[(size_t)N.I]);
			}
		return Out;
	}
	inline double DistanceToSegment(double X, double Z, const FSegment& S)
	{
		const double Dx = S.Bx - S.Ax, Dz = S.Bz - S.Az, Len2 = Dx * Dx + Dz * Dz;
		const double T = std::max(0.0, std::min(1.0, ((X - S.Ax) * Dx + (Z - S.Az) * Dz) / (Len2 != 0 ? Len2 : 1)));
		return std::hypot(X - S.Ax - Dx * T, Z - S.Az - Dz * T);
	}

	// client/ground.js fieldCells: per cell 0 (no field), 1 (a field) or 2 (a field ploughed at right angles)
	inline std::vector<unsigned char> FieldCells(const FScatterMap& M)
	{
		constexpr int BW = 10, BH = 8;
		const int W = M.W, H = M.H;
		std::vector<unsigned char> F((size_t)W * (size_t)H, 0);
		struct FKeep { int X, Y; double R; };
		std::vector<FKeep> Keep;
		for (const FCellXY& P : M.Spawns) Keep.push_back(FKeep{ P.X, P.Y, 9 });
		for (const FCellXY& P : M.Points) Keep.push_back(FKeep{ P.X, P.Y, 5 });
		for (int By = 0; By * BH < H; By++) for (int Bx = 0; Bx * BW < W; Bx++)
		{
			const int X0 = Bx * BW + 1, Y0 = By * BH + 1, X1 = std::min(W - 1, Bx * BW + BW - 1), Y1 = std::min(H - 1, By * BH + BH - 1);
			const FBiomeLook* Look = M.LookAt(std::min(W - 1, Bx * BW + 5), std::min(H - 1, By * BH + 4));
			if (Rnd(Bx, By, 7) > (Look ? Look->Fields : 0.24) || X1 - X0 < 4 || Y1 - Y0 < 3) continue;
			bool bKept = false;
			for (const FKeep& P : Keep) if (P.X > X0 - P.R && P.X < X1 + P.R - 1 && P.Y > Y0 - P.R && P.Y < Y1 + P.R - 1) bKept = true;
			if (bKept) continue;
			bool bOpen = true;
			for (int Y = Y0; Y < Y1 && bOpen; Y++) for (int X = X0; X < X1; X++) if (M.RowAt(X, Y) != '.') { bOpen = false; break; }
			if (!bOpen) continue;
			const unsigned char Dir = Rnd(Bx, By, 8) < 0.5 ? 1 : 2;
			for (int Y = Y0; Y < Y1; Y++) for (int X = X0; X < X1; X++) F[(size_t)(Y * W + X)] = Dir;
		}
		return F;
	}

	// props.js candidates(map)
	inline std::vector<FCandidate> Candidates(const FScatterMap& M)
	{
		const int W = M.W, H = M.H;
		std::vector<FCandidate> List;
		if (W <= 0 || H <= 0) return List;
		const std::vector<FSegment> Paths = PathSegments(M);
		std::vector<unsigned char> Occupied((size_t)W * (size_t)H, 0);
		struct FZone { double X, Z, R; };
		std::vector<FZone> Zones;
		for (const FCellXY& P : M.Spawns) Zones.push_back(FZone{ (P.X + 0.5) * CellMetres, (P.Y + 0.5) * CellMetres, ReinforceRadius + 4 });
		for (const FCellXY& P : M.Points) Zones.push_back(FZone{ (P.X + 0.5) * CellMetres, (P.Y + 0.5) * CellMetres, PointRadius + 4 });
		auto Clear = [&](int X, int Y)
		{
			if (X < 2 || Y < 2 || X >= W - 2 || Y >= H - 2) return false;
			for (int Dy = -1; Dy <= 1; Dy++) for (int Dx = -1; Dx <= 1; Dx++) if (M.RowAt(X + Dx, Y + Dy) != '.') return false;
			return true;
		};
		auto Corridor = [&](double X, double Z, double Margin) { for (const FSegment& P : Paths) if (DistanceToSegment(X, Z, P) < 3 + Margin) return true; return false; };
		// angle < -1e9: props.js's undefined (a seeded heading)
		auto Add = [&](EProp Kind, int Cx, int Cy, double Angle, int Ax, int Ay, double Scale) -> bool
		{
			const int Key = Cy * W + Cx;
			if (M.bWorld && !(Key >= 0 && Key < (int)M.Discovered.size() && M.Discovered[(size_t)Key])) return false;
			if ((Key >= 0 && Key < W * H && Occupied[(size_t)Key]) || !Clear(Cx, Cy)) return false;
			const uint32_t Seed = Hash(Cx, Cy, 71);
			const double Jitter = Kind == EProp::Fence || Kind == EProp::Poplar ? 0 : 0.36;
			const double X = (Cx + 0.5) * CellMetres + (Fraction(Cx, Cy, 81) - 0.5) * Jitter, Z = (Cy + 0.5) * CellMetres + (Fraction(Cx, Cy, 82) - 0.5) * Jitter;
			const double Margin = PropTall(Kind) ? 1.4 : 0.7;
			for (const FZone& P : Zones) if (std::hypot(X - P.X, Z - P.Z) < P.R + Margin) return false;
			if (PropTall(Kind) && Corridor(X, Z, Margin)) return false;
			FCandidate C;
			C.Kind = Kind; C.Cx = Cx; C.Cy = Cy; C.X = X; C.Z = Z; C.Seed = Seed;
			C.Angle = Angle < -1e9 ? Fraction(Cx, Cy, 83) * Pi * 2 : Angle;
			C.Scale = Scale; C.AnchorX = Ax; C.AnchorY = Ay;
			const int B = M.Biome.empty() ? 255 : M.Biome[(size_t)Key];
			C.Biome = B < 255 ? B : 255;
			List.push_back(C);
			Occupied[(size_t)Key] = 1;
			return true;
		};
		constexpr double Seeded = -2e9;

		// summed hedge counts for the 17 x 17 country-density query
		const int Stride = W + 1;
		std::vector<uint32_t> Hedge((size_t)(W + 1) * (size_t)(H + 1), 0);
		for (int Y = 0; Y < H; Y++) for (int X = 0; X < W; X++)
		{
			const int I = (Y + 1) * Stride + X + 1;
			Hedge[(size_t)I] = (M.RowAt(X, Y) == 'H' ? 1 : 0) + Hedge[(size_t)(I - 1)] + Hedge[(size_t)(I - Stride)] - Hedge[(size_t)(I - Stride - 1)];
		}
		auto Country = [&](int X, int Y)
		{
			const int X0 = std::max(0, X - 8), X1 = std::min(W, X + 9), Y0 = std::max(0, Y - 8), Y1 = std::min(H, Y + 9);
			const double N = (double)Hedge[(size_t)(Y1 * Stride + X1)] - Hedge[(size_t)(Y0 * Stride + X1)] - Hedge[(size_t)(Y1 * Stride + X0)] + Hedge[(size_t)(Y0 * Stride + X0)];
			return std::min(1.0, N / 45);
		};
		auto BesideHedge = [&](int X, int Y)
		{
			for (int Dy = -2; Dy <= 2; Dy++) for (int Dx = -2; Dx <= 2; Dx++)
				if (std::max(std::abs(Dx), std::abs(Dy)) == 2 && M.RowAt(X + Dx, Y + Dy) == 'H') return true;
			return false;
		};

		// fence sections follow selected straight hedge runs, two cells to one side
		const int Steps[2][2] = { { 1, 0 }, { 0, 1 } };
		for (const auto& St : Steps)
		{
			const int Dx = St[0], Dy = St[1];
			for (int Y = 0; Y < H; Y++) for (int X = 0; X < W; X++)
			{
				if (M.RowAt(X, Y) != 'H' || M.RowAt(X - Dx, Y - Dy) == 'H') continue;
				int Length = 0;
				while (M.RowAt(X + Dx * Length, Y + Dy * Length) == 'H') Length++;
				if (Length < 3 || Fraction(X, Y, 90 + Dy) > 0.38) continue;
				const int Side = Fraction(X, Y, 92 + Dy) < 0.5 ? -1 : 1;
				for (int Start = 0; Start < Length;)
				{
					const int Run = std::min(Length - Start, 3 + (int)(Hash(X + Start * Dx, Y + Start * Dy, 94) % 6));
					std::vector<size_t> RunList; // indices into List
					for (int K = 0; K < Run; K++)
					{
						const int Ax = X + Dx * (Start + K), Ay = Y + Dy * (Start + K);
						const int Cx = Ax - Dy * Side * 2, Cy = Ay + Dx * Side * 2;
						if (Add(EProp::Fence, Cx, Cy, Dy ? Pi / 2 : 0, Ax, Ay, 1)) RunList.push_back(List.size() - 1);
					}
					// short fragments beside clearings or structures would look like stray posts
					std::vector<size_t> Fragment;
					std::vector<size_t> Drop;
					auto Trim = [&]() { if (Fragment.size() < 3) Drop.insert(Drop.end(), Fragment.begin(), Fragment.end()); Fragment.clear(); };
					for (const size_t I : RunList)
					{
						if (!Fragment.empty())
						{
							const FCandidate& Prev = List[Fragment.back()];
							if (std::abs(List[I].Cx - Prev.Cx) + std::abs(List[I].Cy - Prev.Cy) != 1) Trim();
						}
						Fragment.push_back(I);
					}
					Trim();
					// list.splice(list.indexOf(c), 1) for each dropped candidate (the run's own entries, at the list's end)
					std::sort(Drop.begin(), Drop.end());
					for (auto It = Drop.rbegin(); It != Drop.rend(); ++It)
					{
						Occupied[(size_t)(List[*It].Cy * W + List[*It].Cx)] = 0;
						List.erase(List.begin() + (std::ptrdiff_t)*It);
					}
					Start += Run + 2 + (int)(Hash(X + Start, Y, 96) % 4);
				}
			}
		}

		// one merged crate and barrel cluster beside selected house groups
		{
			std::vector<unsigned char> Seen((size_t)W * (size_t)H, 0);
			const int Dir4[4][2] = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } };
			for (int Y = 0; Y < H; Y++) for (int X = 0; X < W; X++)
			{
				if (M.RowAt(X, Y) != 'B' || Seen[(size_t)(Y * W + X)]) continue;
				std::vector<FCellXY> Q{ FCellXY{ X, Y } }, House;
				Seen[(size_t)(Y * W + X)] = 1;
				while (!Q.empty())
				{
					const FCellXY P = Q.back();
					Q.pop_back();
					House.push_back(P);
					for (const auto& D : Dir4)
					{
						const int Nx = P.X + D[0], Ny = P.Y + D[1], Key = Ny * W + Nx;
						if (M.RowAt(Nx, Ny) == 'B' && !Seen[(size_t)Key]) { Seen[(size_t)Key] = 1; Q.push_back(FCellXY{ Nx, Ny }); }
					}
				}
				if (Fraction(X, Y, 100) > 0.55) continue;
				struct FSpot { int Cx, Cy, Ax, Ay; uint32_t Hh; };
				std::vector<FSpot> Spots;
				for (const FCellXY& A : House) for (const auto& D : Dir4) Spots.push_back(FSpot{ A.X + D[0] * 2, A.Y + D[1] * 2, A.X, A.Y, Hash(A.X + D[0] * 2, A.Y + D[1] * 2, 101) });
				std::stable_sort(Spots.begin(), Spots.end(), [](const FSpot& A, const FSpot& B) { return A.Hh < B.Hh; });
				for (const FSpot& P : Spots) if (Add(EProp::Supplies, P.Cx, P.Cy, Seeded, P.Ax, P.Ay, 1)) break;
			}
		}

		// orchards: plots of small fruit trees in rows, three cells apart, kept whole (not thinned by the tree cap)
		if (!M.Biome.empty())
			for (int Y = 3; Y < H - 3; Y++) for (int X = 3; X < W - 3; X++)
			{
				const int Bx = X / 15, By = Y / 11;
				const FBiomeLook* Look = M.LookAt(X, Y);
				if (X % 3 || Y % 3 || !(Look && Look->bOrchard) || Fraction(Bx, By, 150) > 0.45 || X - Bx * 15 < 2 || Y - By * 11 < 2) continue;
				if (Add(EProp::Deciduous, X, Y, Seeded, -1, -1, 0.5 + Fraction(X, Y, 151) * 0.12)) List.back().bOrchard = true;
			}

		// poplars in short orderly rows along field boundaries
		for (int Y = 3; Y < H - 3; Y++) for (int X = 3; X < W - 3; X++)
		{
			if (Country(X, Y) < 0.15 || Fraction(X, Y, 105) > 0.0012) continue;
			const bool bVertical = (Hash(X, Y, 106) & 1) != 0;
			const int N = 3 + (int)(Hash(X, Y, 107) % 3);
			for (int K = 0; K < N; K++) Add(EProp::Poplar, X + (bVertical ? 0 : K * 2), Y + (bVertical ? K * 2 : 0), 0, -1, -1, 0.92 + Fraction(X, Y, 108) * 0.12);
		}

		// standing wheat on about half of the ploughed fields, a patch per cell in the furrows' direction
		{
			const std::vector<unsigned char> Fields = FieldCells(M);
			for (int Y = 2; Y < H - 2; Y++) for (int X = 2; X < W - 2; X++)
			{
				const unsigned char F = Fields[(size_t)(Y * W + X)];
				if (F && Fraction(X / 10, Y / 8, 130) < 0.5) Add(EProp::Crop, X, Y, F == 2 ? Pi / 2 : 0, -1, -1, 0.92 + Fraction(X, Y, 131) * 0.16);
			}
		}

		int Hay = 0;
		const int HayCap = M.bWorld ? 400 : std::min(16, std::max(3, (int)std::floor(W * H / 2200.0 + 0.5)));
		for (int Y = 2; Y < H - 2; Y++) for (int X = 2; X < W - 2; X++)
		{
			if (!Clear(X, Y) || Occupied[(size_t)(Y * W + X)]) continue;
			const double D = Country(X, Y);
			const bool bNearPath = Corridor((X + 0.5) * CellMetres, (Y + 0.5) * CellMetres, 0.7);
			const FBiomeLook* Look = M.LookAt(X, Y);
			if (Fraction(X, Y, 111) < (0.013 + D * 0.065) * (Look ? Look->Trees : 1)
				&& Add(Fraction(X, Y, 112) < (Look ? Look->Pine : 0.23) ? EProp::Pine : EProp::Deciduous, X, Y, Seeded, -1, -1, 0.86 + Fraction(X, Y, 113) * 0.25)) continue;
			const double BushChance = (0.031 + D * 0.06) * (BesideHedge(X, Y) ? 2 : 1) * (bNearPath ? 0.4 : 1) * (Look ? Look->Bush : 1);
			if (Fraction(X, Y, 118) < BushChance && Add(EProp::Bush, X, Y, Seeded, -1, -1, 0.8 + Fraction(X, Y, 114) * 0.3)) continue;
			if (Fraction(X, Y, 115) < 0.004 * (bNearPath ? 0.3 : 1) * (Look ? Look->Rocks : 1)
				&& Add(EProp::Rocks, X, Y, Seeded, -1, -1, (0.8 + Fraction(X, Y, 116) * 0.3) * (Look && Look->Rocks > 2 ? 1.5 : 1))) continue;
			if (Hay < HayCap && (Look ? Fraction(X, Y, 117) < 0.0012 * Look->Hay * (1 + 3 * D) : D > 0.22 && Fraction(X, Y, 117) < 0.0012) && Add(EProp::Haystack, X, Y, Seeded, -1, -1, 1)) Hay++;
		}
		// meadow grass on the open cells left over, thicker in hedged country, kept off the paths
		for (int Y = 2; Y < H - 2; Y++) for (int X = 2; X < W - 2; X++)
		{
			const FBiomeLook* Look = M.LookAt(X, Y);
			if (Occupied[(size_t)(Y * W + X)] || Fraction(X, Y, 120) > (0.1 + 0.16 * Country(X, Y)) * (Look ? Look->Grass : 1)) continue;
			if (!Corridor((X + 0.5) * CellMetres, (Y + 0.5) * CellMetres, 0.4)) Add(EProp::Grass, X, Y, Seeded, -1, -1, 0.8 + Fraction(X, Y, 121) * 0.5);
		}
		// the scattered trees past the cap (by seed) are dropped; orchards are kept whole
		std::vector<size_t> Trees;
		for (size_t I = 0; I < List.size(); I++) if (PropTree(List[I].Kind) && !List[I].bOrchard) Trees.push_back(I);
		std::stable_sort(Trees.begin(), Trees.end(), [&List](size_t A, size_t B) { return List[A].Seed < List[B].Seed; });
		std::vector<unsigned char> Capped(List.size(), 0);
		const size_t TreeCap = M.bWorld ? 3000 : 1200;
		for (size_t I = TreeCap; I < Trees.size(); I++) Capped[Trees[I]] = 1;
		// woods ('O' cells): a tree on about one cell in three, smaller than a field tree; capped separately
		std::vector<FCandidate> Wood;
		for (int Y = 0; Y < H; Y++) for (int X = 0; X < W; X++)
		{
			if (M.RowAt(X, Y) != 'O' || Fraction(X, Y, 140) > 0.3) continue;
			const FBiomeLook* Look = M.LookAt(X, Y);
			FCandidate C;
			C.Kind = Fraction(X, Y, 141) < (Look ? std::max(0.3, Look->Pine) : 0.3) ? EProp::Pine : EProp::Deciduous;
			C.Cx = X; C.Cy = Y;
			C.X = (X + 0.5) * CellMetres + (Fraction(X, Y, 142) - 0.5) * 1.2;
			C.Z = (Y + 0.5) * CellMetres + (Fraction(X, Y, 143) - 0.5) * 1.2;
			C.Seed = Hash(X, Y, 144);
			C.Angle = Fraction(X, Y, 145) * Pi * 2;
			C.Scale = 0.55 + Fraction(X, Y, 146) * 0.3;
			C.bWood = true;
			Wood.push_back(C);
		}
		std::stable_sort(Wood.begin(), Wood.end(), [](const FCandidate& A, const FCandidate& B) { return A.Seed < B.Seed; });
		std::vector<FCandidate> Out;
		Out.reserve(List.size() + Wood.size());
		for (size_t I = 0; I < List.size(); I++) if (!Capped[I]) Out.push_back(List[I]);
		const size_t WoodCap = M.bWorld ? 3000 : 1500;
		for (size_t I = 0; I < Wood.size() && I < WoodCap; I++) Out.push_back(Wood[I]);
		return Out;
	}

	// What the live filter reads: the physical grid, the height levels, the supply nodes and the railway cells.
	class FLiveSource
	{
	public:
		virtual ~FLiveSource() = default;
		virtual int Width() const = 0;
		virtual int Height() const = 0;
		virtual unsigned char At(int X, int Y) const = 0; // terrain.physicalGrid, 0 off the map
		virtual int Level(int X, int Y) const = 0;        // level(heights, x, y): 0 off the map
		virtual double Ground(double X, double Z) const = 0;
		virtual bool Skip(int Cell) const { return false; } // client/railways.js has(cell)
		virtual bool Low() const { return false; }
	};

	// props.js visible: whether a candidate shows on the live grid
	inline bool Visible(const FCandidate& C, const FLiveSource& Live, const std::vector<std::pair<double, double>>& Nodes)
	{
		const bool bLow = Live.Low();
		if (C.bWood) return Live.At(C.Cx, C.Cy) == 'O' && !(bLow && (C.Seed & 1));
		if (bLow && (C.Seed & 1) && C.Kind != EProp::Crop) return false;
		if (C.Cx < 2 || C.Cy < 2 || C.Cx >= Live.Width() - 2 || C.Cy >= Live.Height() - 2) return false;
		for (int Dy = -1; Dy <= 1; Dy++) for (int Dx = -1; Dx <= 1; Dx++) if (Live.At(C.Cx + Dx, C.Cy + Dy) != '.') return false;
		const int Lv = Live.Level(C.Cx, C.Cy);
		if (Live.Level(C.Cx + 1, C.Cy) != Lv || Live.Level(C.Cx - 1, C.Cy) != Lv || Live.Level(C.Cx, C.Cy + 1) != Lv || Live.Level(C.Cx, C.Cy - 1) != Lv) return false;
		for (const auto& N : Nodes) if ((C.X - N.first) * (C.X - N.first) + (C.Z - N.second) * (C.Z - N.second) < 16) return false;
		if (C.AnchorX >= 0) { if (Live.At(C.AnchorX, C.AnchorY) != (C.Kind == EProp::Fence ? 'H' : 'B')) return false; }
		return true;
	}
	// props.js kindFor: a deciduous tree above level 0 becomes a pine for one seed in three
	inline EProp KindFor(const FCandidate& C, const FLiveSource& Live)
	{
		return C.Kind == EProp::Deciduous && !C.bOrchard && Live.Level(C.Cx, C.Cy) > 0 && (C.Seed >> 3) % 3 == 0 ? EProp::Pine : C.Kind;
	}

	// Where one prop stands, as drawn (props.js place): the kind drawn (bShown false: not shown), the instance matrix
	// (three.js column-major, reference metres) and the leaf and bark tints.
	struct FPlaced
	{
		bool bShown = false;
		EProp Kind = EProp::Grass;
		double M[16] = {};
		double Col[3] = { 1, 1, 1 };
		double Bark = 1;
		// the same placement as position, scale and yaw (the matrix is TRS with a turn about y only)
		double X = 0, Y = 0, Z = 0, Sx = 1, Sy = 1, Sz = 1, Yaw = 0;
	};
	inline FPlaced Place(const FCandidate& C, const FLiveSource& Live, const std::vector<std::pair<double, double>>& Nodes)
	{
		FPlaced P;
		if (C.Kind == EProp::Grass && Live.Low()) return P;
		if (Live.Skip(C.Cy * Live.Width() + C.Cx)) return P;
		if (!Visible(C, Live, Nodes)) return P;
		const EProp Kind = KindFor(C, Live);
		const bool bCrop = Kind == EProp::Crop, bLeafy = PropLeafy(Kind);
		auto F = [&C](int K) { return ((C.Seed >> K) & 255) / 255.0; };
		const FBiomeLook* Look = BiomeLook(C.Biome);
		const bool bReeds = Look && Look->bReeds && Kind == EProp::Grass;
		const double Girth = (bLeafy && !bCrop ? 0.9 + 0.2 * F(4) : 1) * (bReeds ? 0.75 : 1), Tall = (bLeafy ? 0.88 + 0.24 * F(12) : 1) * (bReeds ? 1.9 : 1);
		P.X = C.X; P.Y = Live.Ground(C.X, C.Z) - 0.035; P.Z = C.Z;
		P.Sx = C.Scale * Girth; P.Sy = C.Scale * Tall; P.Sz = C.Scale * Girth; P.Yaw = C.Angle;
		ComposeYaw(P.X, P.Y, P.Z, P.Sx, P.Sy, P.Sz, P.Yaw, P.M);
		const double Tint = 0.88 + ((C.Seed >> 8) % 101) / 600.0;
		auto Set = [&P](double R, double G, double B) { P.Col[0] = R; P.Col[1] = G; P.Col[2] = B; };
		if (!bLeafy) Set(Tint, Tint, Tint);
		else if (bCrop) Set(Tint * 1.3, Tint * 1.06, Tint * 0.56);
		else if (C.bOrchard) Set(Tint * 1.02, Tint * 1.12, Tint * 0.7);
		else if (Kind == EProp::Deciduous && (C.Seed >> 16) % 9 == 0) Set(Tint * 1.08, Tint * 1.0, Tint * 0.72);
		else if (Look && Look->bDry && (Kind == EProp::Grass || Kind == EProp::Bush)) Set(Tint * 1.3, Tint * 1.12, Tint * 0.6);
		else if (bReeds) Set(Tint * 0.96, Tint * 1.02, Tint * 0.7);
		else Set(Tint * (0.94 + F(18) * 0.1), Tint * (0.97 + F(20) * 0.05), Tint * (0.86 + F(22) * 0.12));
		P.Bark = Tint;
		P.Kind = Kind;
		P.bShown = true;
		return P;
	}
}

#ifndef TC_WORLD_CORE_ONLY

#include "CoreMinimal.h"

// Where the scenery props' imported models are (Content/Models, the models export's props/* and foliage/*).
struct THREECROSSROADS_API FTCScatter
{
	static const TCHAR* const ReferencePin;
	// The model catalog key that draws a prop kind (props.js meshesFor: deciduous is the broadleaf tree).
	static const TCHAR* CatalogKey(TCScatterCore::EProp Kind);
};

#endif // TC_WORLD_CORE_ONLY
