// The painted ground: what every ground pixel looks like, as the pinned reference paints it (ReferencePin). A port of
//   client/ground.js     LOOK, BIOME_LOOKS (ground tint and soil), rnd, buildNoise (the 512 x 512 fBm and its equalized
//                        copy), fieldCells (fieldBlock, fieldKeep), cellAttrs (each cell's main and secondary material,
//                        how much of the secondary shows, scars, char, region tint), biomeTint (13-cell box blur),
//                        createCellNoise, createScarSampler, scarField, scarShape, scarWeights (the torn blast mark),
//                        raster (the per-pixel blend of the four nearest cells at a noise-warped position, the region
//                        tint, the blast mark, the broad light sweeps, the pale water line), overlays and shellHole
//                        (shell holes, trench cuts, contour lines near cliffs), paint/tilePaint (only the 4 x 4-cell
//                        tiles round changed cells are repainted), createGround's pixels per cell;
//   client/main.js       startGame and applyCells, the parts the ground reads (grid, state, groundGrid, objectGrid,
//                        materials, map.rows on World maps, rememberLook's biome, setLevel);
//   client/fog.js        the per-cell fog look, faded within a quarter second (here as a multiplier: 1 seen, 0.55
//                        explored, 0.32 never seen, the native client's convention), sampled with bilinear filtering;
//   client/surfaces.js   the fog's drifting mist (in the ground material, unreal/Tools/terrain/import-ground.py);
//   client/rubble-decals.js and client/crater-decals.js: where each lasting rubble and crater picture lies.
//
// The engine-free part (namespace TCGroundPaintCore) compiles alone with TC_GROUND_PAINT_CORE_ONLY defined, so the
// standalone harness (unreal/Tools/terrain/harness) can hold it to the reference run under Node (ground-oracle.mjs):
// every cell attribute and every raster byte are compared exactly. Arithmetic follows the JavaScript: doubles, Float32
// where the reference stores into a Float32Array, Uint8ClampedArray rounding (half to even) and V8's sin (fdlibm, so
// the per-cell noise has the same last bits). The textured look uses the reference's own tiles, made by its buildTiles
// in a browser (unreal/Tools/terrain/make-ground-tiles.mjs) and kept as bytes in World/TCGroundPaint.Tiles.inl.
//
// Recipient knowledge: the painter reads only what this client was sent (the start's map and cells, later cell rows).
// On a World Conquest map a cell no row has described yet is unknown: its pixels are not painted from anything (a flat
// fog shade), no known pixel blends toward it, no blast mark or contour line reads it, and a field needs its whole
// block known. KnownMode::Reference reproduces the browser instead (it paints World's placeholder '.' cells as open
// ground); the harness uses it to hold the port to the oracle on every pixel.
//
// Axes: canvas pixel (i, j) covers reference metres x in [i, i + 1) * 2 / P, z in [j, j + 1) * 2 / P; row 0 is the
// north (z = 0) edge. In Unreal, U = X / (W * 200 cm), V = Y / (H * 200 cm) (World/TCTerrain.h axes).
#pragma once

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <deque>
#include <functional>
#include <string>
#include <unordered_map>
#include <vector>

namespace TCGroundPaintCore
{
	inline constexpr double CellMetres = 2.0; // shared/sim.js CELL

	// ---------- V8's Math.sin (fdlibm s_sin.c, k_sin.c, k_cos.c, e_rem_pio2.c), so rnd() keeps its last bits ----------
	namespace Fdlibm
	{
		inline int32_t Hi(double X) { uint64_t B; std::memcpy(&B, &X, 8); return (int32_t)(B >> 32); }
		inline double Make(int32_t H, uint32_t L) { const uint64_t B = ((uint64_t)(uint32_t)H << 32) | L; double X; std::memcpy(&X, &B, 8); return X; }
		inline double KernelSin(double X, double Y, int Iy)
		{
			const double Half = 5.00000000000000000000e-01, S1 = -1.66666666666666324348e-01, S2 = 8.33333333332248946124e-03, S3 = -1.98412698298579493134e-04,
				S4 = 2.75573137070700676789e-06, S5 = -2.50507602534068634195e-08, S6 = 1.58969099521155010221e-10;
			const int32_t Ix = Hi(X) & 0x7fffffff;
			if (Ix < 0x3e400000 && (int)X == 0) return X;
			const double Z = X * X, V = Z * X, R = S2 + Z * (S3 + Z * (S4 + Z * (S5 + Z * S6)));
			if (Iy == 0) return X + V * (S1 + Z * R);
			return X - ((Z * (Half * Y - V * R) - Y) - V * S1);
		}
		inline double KernelCos(double X, double Y)
		{
			const double One = 1.0, C1 = 4.16666666666666019037e-02, C2 = -1.38888888888741095749e-03, C3 = 2.48015872894767294178e-05,
				C4 = -2.75573143513906633035e-07, C5 = 2.08757232129817482790e-09, C6 = -1.13596475577881948265e-11;
			const int32_t Ix = Hi(X) & 0x7fffffff;
			if (Ix < 0x3e400000 && (int)X == 0) return One;
			const double Z = X * X, R = Z * (C1 + Z * (C2 + Z * (C3 + Z * (C4 + Z * (C5 + Z * C6)))));
			if (Ix < 0x3FD33333) return One - (0.5 * Z - (Z * R - X * Y));
			const double Qx = Ix > 0x3fe90000 ? 0.28125 : Make(Ix - 0x00200000, 0);
			const double Hz = 0.5 * Z - Qx, A = One - Qx;
			return A - (Hz - (Z * R - X * Y));
		}
		// |x| up to 2^19 pi/2 (the arguments here stay far below); false beyond, where the caller falls back
		inline bool RemPio2(double X, double* Y, int& N)
		{
			static const int32_t Npio2Hw[] = { 0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB, 0x402C463A, 0x402F6A7A,
				0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB, 0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C,
				0x4042106C, 0x4042D97C, 0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB };
			const double Half = 0.5, InvPio2 = 6.36619772367581382433e-01, Pio2_1 = 1.57079632673412561417e+00, Pio2_1t = 6.07710050650619224932e-11,
				Pio2_2 = 6.07710050630396597660e-11, Pio2_2t = 2.02226624879595063154e-21, Pio2_3 = 2.02226624871116645580e-21, Pio2_3t = 8.47842766036889956997e-32;
			const int32_t Hx = Hi(X), Ix = Hx & 0x7fffffff;
			double Z, W, T, R, Fn;
			if (Ix <= 0x3fe921fb) { Y[0] = X; Y[1] = 0; N = 0; return true; }
			if (Ix < 0x4002d97c)
			{
				if (Hx > 0)
				{
					Z = X - Pio2_1;
					if (Ix != 0x3ff921fb) { Y[0] = Z - Pio2_1t; Y[1] = (Z - Y[0]) - Pio2_1t; }
					else { Z -= Pio2_2; Y[0] = Z - Pio2_2t; Y[1] = (Z - Y[0]) - Pio2_2t; }
					N = 1; return true;
				}
				Z = X + Pio2_1;
				if (Ix != 0x3ff921fb) { Y[0] = Z + Pio2_1t; Y[1] = (Z - Y[0]) + Pio2_1t; }
				else { Z += Pio2_2; Y[0] = Z + Pio2_2t; Y[1] = (Z - Y[0]) + Pio2_2t; }
				N = -1; return true;
			}
			if (Ix > 0x413921fb) return false;
			T = std::fabs(X);
			const int K = (int)(T * InvPio2 + Half);
			Fn = (double)K; R = T - Fn * Pio2_1; W = Fn * Pio2_1t;
			if (K < 32 && Ix != Npio2Hw[K - 1]) Y[0] = R - W;
			else
			{
				const int J = Ix >> 20;
				Y[0] = R - W;
				int I = J - ((Hi(Y[0]) >> 20) & 0x7ff);
				if (I > 16)
				{
					T = R; W = Fn * Pio2_2; R = T - W; W = Fn * Pio2_2t - ((T - R) - W); Y[0] = R - W;
					I = J - ((Hi(Y[0]) >> 20) & 0x7ff);
					if (I > 49) { T = R; W = Fn * Pio2_3; R = T - W; W = Fn * Pio2_3t - ((T - R) - W); Y[0] = R - W; }
				}
			}
			Y[1] = (R - Y[0]) - W;
			if (Hx < 0) { Y[0] = -Y[0]; Y[1] = -Y[1]; N = -K; return true; }
			N = K; return true;
		}
		inline double Sin(double X)
		{
			const int32_t Ix = Hi(X) & 0x7fffffff;
			if (Ix <= 0x3FE921FB) return KernelSin(X, 0.0, 0);
			if (Ix >= 0x7ff00000) return X - X;
			double Y[2]; int N = 0;
			if (!RemPio2(X, Y, N)) return std::sin(X);
			switch (N & 3)
			{
			case 0: return KernelSin(Y[0], Y[1], 1);
			case 1: return KernelCos(Y[0], Y[1]);
			case 2: return -KernelSin(Y[0], Y[1], 1);
			default: return -KernelCos(Y[0], Y[1]);
			}
		}
		inline double Cos(double X)
		{
			const int32_t Ix = Hi(X) & 0x7fffffff;
			if (Ix <= 0x3FE921FB) return KernelCos(X, 0.0);
			if (Ix >= 0x7ff00000) return X - X;
			double Y[2]; int N = 0;
			if (!RemPio2(X, Y, N)) return std::cos(X);
			switch (N & 3)
			{
			case 0: return KernelCos(Y[0], Y[1]);
			case 1: return -KernelSin(Y[0], Y[1], 1);
			case 2: return -KernelCos(Y[0], Y[1]);
			default: return KernelSin(Y[0], Y[1], 1);
			}
		}
	}

	// V8's Math.hypot for two numbers: scaled by the larger, Kahan-summed, sqrt times the scale
	inline double JsHypot(double A, double B)
	{
		if (std::isnan(A) || std::isnan(B)) return std::isinf(A) || std::isinf(B) ? INFINITY : NAN;
		const double Aa = std::fabs(A), Ab = std::fabs(B), Max = std::max(Aa, Ab);
		if (Max == INFINITY) return INFINITY;
		if (Max == 0) return 0;
		double Sum = 0, Comp = 0;
		for (const double V : { Aa, Ab })
		{
			const double Nn = V / Max, Summand = Nn * Nn - Comp, Pre = Sum + Summand;
			Comp = (Pre - Sum) - Summand;
			Sum = Pre;
		}
		return std::sqrt(Sum) * Max;
	}

	// ---------- JavaScript number helpers ----------
	// Uint8ClampedArray store: clamp, round half to even, NaN to 0
	inline uint8_t ClampU8(double X)
	{
		if (!(X > 0)) return 0;
		if (X >= 255) return 255;
		const double F = std::floor(X);
		if (F + 0.5 < X) return (uint8_t)(F + 1);
		if (X < F + 0.5) return (uint8_t)F;
		const int I = (int)F;
		return (uint8_t)((I & 1) ? I + 1 : I);
	}
	// x | 0 for the small numbers here: truncation toward zero
	inline int ToInt(double X) { return (int)X; }
	inline float F32(double X) { return (float)X; }

	// client/ground.js rnd (the same formula as main.js)
	inline double Rnd(double X, double Y, double K = 0)
	{
		const double V = Fdlibm::Sin(X * 127.1 + Y * 311.7 + K * 74.7) * 43758.5453;
		return V - std::floor(V);
	}

	// ---------- material looks ----------
	enum EMat : uint8_t { Grass = 0, Dirt = 1, Mud = 2, Field = 3, Road = 4, Water = 5, Rubble = 6, Shell = 7, Earth = 8, FieldV = 9, MatCount = 10 };
	struct FLook { const char* Tex; double Metres; double Mul[3]; double Sat; double Con; double Avg[3]; };
	inline const FLook& Look(int M)
	{
		static const FLook Looks[9] = {
			{ "grass", 9, { 1.02, 1.06, 1.35 }, 0.78, 0.9, { 107, 103, 44 } },
			{ "dirt", 8, { 0.95, 1, 1.04 }, 0.5, 1, { 160, 117, 72 } },
			{ "mud", 7, { 1, 1, 1 }, 0.72, 1, { 87, 64, 43 } },
			{ "field", 10, { 0.95, 0.95, 1 }, 0.5, 0.55, { 103, 70, 44 } },
			{ "road", 8, { 0.9, 0.9, 0.88 }, 0.62, 1, { 162, 136, 109 } },
			{ "water", 12, { 0.95, 1, 1.08 }, 1, 1, { 57, 83, 82 } },
			{ "rubble", 6, { 0.92, 0.92, 0.92 }, 0.7, 1, { 135, 105, 86 } },
			{ "shelled", 7, { 1, 1, 1 }, 0.72, 1, { 81, 63, 53 } },
			{ "earth", 5, { 1.1, 1.1, 1.1 }, 0.75, 1, { 69, 49, 34 } },
		};
		return Looks[M == FieldV ? (int)Field : M];
	}
	// the flat colour a material shows until its texture loads (and if it never does)
	inline void FlatColor(int M, uint8_t Out[3])
	{
		const FLook& L = Look(M);
		const double R = L.Avg[0] * L.Mul[0], G = L.Avg[1] * L.Mul[1], B = L.Avg[2] * L.Mul[2], Y = 0.3 * R + 0.59 * G + 0.11 * B;
		Out[0] = ClampU8(Y + (R - Y) * L.Sat); Out[1] = ClampU8(Y + (G - Y) * L.Sat); Out[2] = ClampU8(Y + (B - Y) * L.Sat);
	}
	// BIOME_LOOKS, the parts the ground reads: tint, soil and how much open ground is bare or ploughed
	struct FBiomeGround { double Tint[3]; int Soil; double Bare; double Fields; };
	inline const FBiomeGround* BiomeGround(int B)
	{
		static const FBiomeGround Looks[6] = {
			{ { 1.02, 1.03, 0.9 }, Dirt, 0.08, 0.5 },   // farmland
			{ { 0.78, 0.9, 0.86 }, Mud, 0.12, 0.03 },   // pine
			{ { 0.84, 0.94, 0.82 }, Mud, 0.22, 0.02 },  // marsh
			{ { 1.04, 0.96, 0.84 }, Dirt, 0.24, 0.05 }, // highland
			{ { 0.96, 1.08, 0.86 }, Dirt, 0, 0.18 },    // orchard
			{ { 1.26, 1.1, 0.7 }, Dirt, 0.2, 0.28 },    // steppe
		};
		return B >= 0 && B < 6 ? &Looks[B] : nullptr;
	}
	inline constexpr int Tile = 4;       // cells per repaint tile
	inline constexpr double Warp = 0.32; // how far (in cells) the blend edges wander
	inline constexpr double Foam[3] = { 184, 178, 146 };

	// ---------- noise: the periodic 512 x 512 fBm table and its equalized copy ----------
	inline constexpr int NS = 512, NM = 511;
	struct FNoise
	{
		std::vector<float> NA, NP;
		FNoise()
		{
			// rand(): (seed * 1103515245 + 12345) >>> 0 in doubles (the product passes 2^53 and rounds first)
			double Seed = 1234567;
			auto Rand = [&Seed]() { Seed = std::fmod(Seed * 1103515245.0 + 12345.0, 4294967296.0); return Seed / 4294967296.0; };
			NA.assign((size_t)NS * NS, 0.f);
			double Amp = 1;
			for (int O = 0; O < 6; O++, Amp *= 0.55)
			{
				const int G = 4 << O, Step = NS / G;
				std::vector<float> Lat((size_t)G * G);
				for (float& L : Lat) L = F32(Rand() * 2 - 1);
				for (int Y = 0; Y < NS; Y++)
				{
					const double Fy = (double)Y / Step, Yi = std::floor(Fy), Ty = Fy - Yi, Sy = Ty * Ty * (3 - 2 * Ty);
					const int R0 = (int)Yi * G, R1 = (((int)Yi + 1) % G) * G;
					for (int X = 0; X < NS; X++)
					{
						const double Fx = (double)X / Step, Xi = std::floor(Fx), Tx = Fx - Xi, Sx = Tx * Tx * (3 - 2 * Tx);
						const int X0 = (int)Xi, X1 = (X0 + 1) % G;
						const double Top = (double)Lat[(size_t)(R0 + X0)] + ((double)Lat[(size_t)(R0 + X1)] - (double)Lat[(size_t)(R0 + X0)]) * Sx;
						const double Bot = (double)Lat[(size_t)(R1 + X0)] + ((double)Lat[(size_t)(R1 + X1)] - (double)Lat[(size_t)(R1 + X0)]) * Sx;
						float& V = NA[(size_t)(Y * NS + X)];
						V = F32((double)V + Amp * (Top + (Bot - Top) * Sy));
					}
				}
			}
			double Mx = 0;
			for (const float V : NA) Mx = std::max(Mx, (double)std::fabs(V));
			for (float& V : NA) V = F32((double)V / Mx);
			constexpr int Bins = 2048;
			std::vector<float> Hist(Bins, 0.f);
			auto Bin = [](float V) { return std::min(Bins - 1, ToInt(((double)V + 1) / 2 * Bins)); };
			for (const float V : NA) Hist[(size_t)Bin(V)] += 1.f;
			for (int I = 1; I < Bins; I++) Hist[(size_t)I] = F32((double)Hist[(size_t)I] + (double)Hist[(size_t)(I - 1)]);
			NP.resize(NA.size());
			for (size_t I = 0; I < NA.size(); I++) NP[I] = F32((double)Hist[(size_t)Bin(NA[I])] / (double)NA.size());
		}
		float A(int X, int Y) const { return NA[(size_t)(((Y & NM) << 9) | (X & NM))]; }
		static const FNoise& Get() { static const FNoise N; return N; }
	};

	// ---------- the material tiles (client/ground.js buildTiles output) ----------
	// T x T pixels, RGBA, already tinted (mul, saturation, contrast); a power of two. Empty Data: the flat colour.
	struct FTile { int T = 0; std::vector<uint8_t> Data; };
	// buildTiles' size for a material at P pixels a cell: 2 ** round(log2(max(8, metres / CELL * P)))
	inline int TileSize(int M, int P)
	{
		const double V = std::max(8.0, Look(M).Metres / CellMetres * P);
		return 1 << (int)std::floor(std::log2(V) + 0.5);
	}

	// ---------- the start and its cell rows, as client/main.js keeps them ----------
	struct FStartMap
	{
		int W = 0, H = 0;
		bool bWorld = false;
		std::vector<std::string> Rows, Heights, LayerGround, LayerObjects; // empty: absent
		struct FXY { int X = 0, Y = 0; bool bNull = false; };
		std::vector<FXY> Spawns, Points;
		std::vector<int> Biome;                                // map.biome (empty: none)
		std::vector<std::pair<int, std::string>> LayerMaterials, LayerObjectMaterials; // layers.materials, objectMaterials
	};
	// [cell, char, level, state, data]; data's ground, object, biome and material as far as the ground needs them
	struct FCellRow
	{
		int C = -1;
		uint8_t Ch = '.';
		bool bLevel = false; int Level = 0;
		bool bState = false; int State = 0;
		bool bData = false;
		uint8_t Ground = 0, Object = 0; // 0: absent in the data
		int Biome = -1;                 // -1: absent
		bool bMaterial = false; std::string Material;
	};

	enum class EKnownMode : uint8_t { Native, Reference };

	// shared/sim.js levelOf
	inline int LevelOf(uint8_t Ch) { return Ch >= 'a' ? 96 - (int)Ch : Ch >= '0' && Ch <= '9' ? Ch - '0' : 0; }
	// startState: wear in quarters only (startWear: mud and fords 0.2-0.8, craters 0.3-0.6, by cellNoise)
	inline uint8_t StartState(uint8_t Ch, int C)
	{
		uint32_t H = (uint32_t)(C + 1) * 0x9e3779b1u;
		H ^= H >> 15; H *= 0x85ebca6bu;
		const double N = (double)(H ^ (H >> 13)) / 4294967296.0;
		const double Wear = Ch == 'M' || Ch == 'F' ? 0.2 + 0.6 * N : Ch == '+' ? 0.3 + 0.3 * N : 0;
		return (uint8_t)std::min(3.0, std::floor(Wear * 4));
	}
	inline bool BaseSurface(uint8_t Ch) { return Ch == '.' || Ch == 'D' || Ch == '+' || Ch == 'M' || Ch == 'W' || Ch == 'F'; }

	// ---------- the vector overlays (shell holes, trench cuts, contour lines), as ground.js draws them ----------
	struct FStop { double At; uint8_t R, G, B; double A; };
	struct FPathOp { char Kind; double V[5]; }; // 'M' x y, 'L' x y, 'A' x y r a0 a1
	struct FDraw
	{
		bool bStroke = false;
		// fill: a radial gradient (two circles) or a flat colour; stroke: a flat colour
		bool bGradient = false;
		double G[6] = {};               // x0 y0 r0 x1 y1 r1
		std::vector<FStop> Stops;       // gradient stops, or one stop for a flat colour
		double LineWidth = 1;
		std::vector<FPathOp> Path;
		bool bClip = false; double Clip[4] = {};
	};

	// ---------- the painter ----------
	class FGround
	{
	public:
		EKnownMode Mode = EKnownMode::Native;
		// pixels per cell (createGround): High graphics' 2048-pixel budget, at least 8 (2 on World maps), at most 24
		static int PixelsPerCell(int W, int H, bool bWorld, bool bLow = false)
		{
			return std::max(bWorld ? 2 : 8, std::min(24, (int)std::floor((bLow ? 1280.0 : 2048.0) / std::max(W, H))));
		}

		// ----- inputs -----
		// A start: the map as the start sends it, and its cells (this side's memory). Paints everything.
		void Start(const FStartMap& Map, const std::vector<FCellRow>& Cells, const FTile* Tiles /* MatCount - 1 or null */, bool bLow = false)
		{
			W = Map.W; H = Map.H; bWorld = Map.bWorld; N = W * H;
			P = PixelsPerCell(W, H, bWorld, bLow);
			Spawns.clear(); Points.clear();
			for (const auto& S : Map.Spawns) if (!S.bNull) Spawns.push_back(S);
			for (const auto& S : Map.Points) Points.push_back(S);
			// worldLayers(map): the ground and object layers (a map without them derives both from its rows)
			Grid.assign((size_t)N, '.');
			for (int Y = 0; Y < H && Y < (int)Map.Rows.size(); Y++) for (int X = 0; X < W && X < (int)Map.Rows[(size_t)Y].size(); X++) Grid[(size_t)(Y * W + X)] = (uint8_t)Map.Rows[(size_t)Y][(size_t)X];
			StartRows = Grid;
			GroundL.resize((size_t)N); ObjectL.resize((size_t)N);
			const bool bLayers = (int)Map.LayerGround.size() == H && (int)Map.LayerObjects.size() == H;
			for (int C = 0; C < N; C++)
			{
				const uint8_t Ch = Grid[(size_t)C];
				if (bLayers)
				{
					const int X = C % W, Y = C / W;
					const std::string& G = Map.LayerGround[(size_t)Y]; const std::string& O = Map.LayerObjects[(size_t)Y];
					GroundL[(size_t)C] = X < (int)G.size() ? (uint8_t)G[(size_t)X] : '.';
					const uint8_t Ob = X < (int)O.size() ? (uint8_t)O[(size_t)X] : '.';
					ObjectL[(size_t)C] = Ob == 'N' ? '.' : Ob;
				}
				else { GroundL[(size_t)C] = BaseSurface(Ch) ? Ch : Ch == '=' ? 'W' : '.'; ObjectL[(size_t)C] = BaseSurface(Ch) || Ch == 'N' ? '.' : Ch; }
			}
			// worldLayers materials: a legacy row names an object's material where the object layer holds one; rubble
			// without one is stone (the rubble decals' atlas quadrant reads it)
			Material.assign((size_t)N, std::string());
			HasMaterial.assign((size_t)N, 0);
			for (const auto& M : Map.LayerMaterials) if (M.first >= 0 && M.first < N && ObjectL[(size_t)M.first] != '.') { Material[(size_t)M.first] = M.second; HasMaterial[(size_t)M.first] = 1; }
			for (const auto& M : Map.LayerObjectMaterials) if (M.first >= 0 && M.first < N && ObjectL[(size_t)M.first] != '.') { Material[(size_t)M.first] = M.second; HasMaterial[(size_t)M.first] = 1; }
			for (int C = 0; C < N; C++) if (ObjectL[(size_t)C] == 'R' && !HasMaterial[(size_t)C]) { Material[(size_t)C] = "stone"; HasMaterial[(size_t)C] = 1; }
			State.resize((size_t)N);
			for (int C = 0; C < N; C++) State[(size_t)C] = StartState(Grid[(size_t)C], C);
			Level.assign((size_t)N, 0);
			if ((int)Map.Heights.size() == H)
				for (int Y = 0; Y < H; Y++) for (int X = 0; X < W && X < (int)Map.Heights[(size_t)Y].size(); X++) Level[(size_t)(Y * W + X)] = LevelOf((uint8_t)Map.Heights[(size_t)Y][(size_t)X]);
			Biome.clear(); BiomeCells.clear(); TintFresh = true;
			if ((int)Map.Biome.size() == N) { Biome.resize((size_t)N); for (int C = 0; C < N; C++) Biome[(size_t)C] = (uint8_t)std::max(0, std::min(255, Map.Biome[(size_t)C])); }
			Known.assign((size_t)N, bWorld ? 0 : 1);
			// createGround: the fields of the map as the start sent it (World maps redo them once its cells are in)
			Fields = FieldsOf();
			for (const FCellRow& R : Cells) ApplyRowData(R, true);
			UseTiles(Tiles);
			Attrs.Valid = false;
			Paint(nullptr);
		}
		// A snapshot's cell rows (client/main.js applyCells, the ground's part): repaints the tiles round them.
		void Apply(const std::vector<FCellRow>& Rows)
		{
			if (!N) return;
			std::vector<int> Cells;
			for (const FCellRow& R : Rows) if (R.C >= 0 && R.C < N) { ApplyRowData(R, false); Cells.push_back(R.C); }
			if (!Cells.empty()) Paint(&Cells);
		}
		// Textures arrived (or left): everything is painted again with them.
		void SetTiles(const FTile* Tiles) { UseTiles(Tiles); if (N) { CancelPending(); FullPaint(); } }

		// The game's frame adapter (createGround's frames option): with it, the changed tiles of a snapshot are queued
		// (merged into what is still waiting, first come first painted, everything when over half the map changed) and
		// PaintPending paints them a few per frame, always from the latest attributes. Without it, Apply paints at once.
		bool bDeferred = false;
		int PendingTiles() const { return (int)PendingOrder.size() - PendingHead; }
		// Paints waiting tiles until MaxTiles are done or OutOfTime() says the frame's budget is spent (checked after each).
		int PaintPending(int MaxTiles, const std::function<bool()>& OutOfTime)
		{
			int Painted = 0;
			while (PendingHead < (int)PendingOrder.size())
			{
				const int T = PendingOrder[(size_t)PendingHead++];
				PendingMark[(size_t)T] = 0;
				TilePaint({ T });
				if (++Painted >= MaxTiles || (OutOfTime && OutOfTime())) break;
			}
			if (PendingHead >= (int)PendingOrder.size()) { PendingOrder.clear(); PendingHead = 0; }
			return Painted;
		}

		// ----- outputs -----
		int MapW() const { return W; }
		bool IsWorld() const { return bWorld; }
		int MapH() const { return H; }
		int PixelsPerCell() const { return P; }
		int CanvasW() const { return W * P; }
		int CanvasH() const { return H * P; }
		const std::vector<uint8_t>& Canvas() const { return Pixels; }  // RGBA, row 0 north
		const std::vector<uint8_t>& RasterOnly() const { return Raster; } // the same before the vector overlays
		// The pixel rectangles painted since the last call (x0, y0, x1, y1, exclusive), merged per paint.
		std::vector<std::array<int, 4>> TakeDirtyRects() { std::vector<std::array<int, 4>> Out; Out.swap(DirtyRects); return Out; }
		bool IsKnown(int C) const { return C >= 0 && C < N && (Mode == EKnownMode::Reference || Known[(size_t)C]); }
		uint8_t CharAt(int C) const { return C >= 0 && C < N ? Grid[(size_t)C] : 0; }
		uint8_t ObjectAt(int C) const { return C >= 0 && C < N ? ObjectL[(size_t)C] : 0; }
		int LevelAt(int C) const { return C >= 0 && C < N ? Level[(size_t)C] : 0; }
		// client/rubble-decals.js materialAt: the cell's material name, empty when the client holds none
		const std::string& MaterialAt(int C) const { static const std::string None; return C >= 0 && C < N && HasMaterial[(size_t)C] ? Material[(size_t)C] : None; }
		const std::vector<FDraw>& LastOverlays() const { return OverlayLog; }
		bool Textured() const { return bTextured; }
		// the last paint: 'full' or 'tiles', and how many tiles
		std::string LastKind;
		int LastTiles = 0;

		// The cell attributes (cellAttrs), for the harness and the material diagnostics.
		struct FAttrs
		{
			bool Valid = false;
			std::vector<uint8_t> Prim, Sec, Ov, Key, Scar, Near, Char;
			std::vector<float> Amt, Lev, Depth, Tr, Tg, Tb;
		};
		const FAttrs& Attributes() const { return Attrs; }
		const std::vector<uint8_t>& FieldMap() const { return Fields; }

		// ---------- implementation ----------
	private:
		int W = 0, H = 0, N = 0, P = 8;
		bool bWorld = false;
		std::vector<uint8_t> Grid, StartRows, GroundL, ObjectL, State, Known, Fields, HasMaterial;
		std::vector<int> Level;
		std::vector<uint8_t> Biome; // empty: no map.biome
		std::vector<int> BiomeCells;
		std::vector<std::string> Material;
		std::vector<FStartMap::FXY> Spawns, Points;
		FAttrs Attrs;
		std::vector<float> TintR, TintG, TintB;
		bool TintFresh = true;
		std::vector<uint8_t> Pixels, Raster;
		std::vector<std::array<int, 4>> DirtyRects;
		std::vector<int> PendingOrder;  // queued tiles in first-queued order (a Set in the browser)
		std::vector<uint8_t> PendingMark;
		int PendingHead = 0;
		void CancelPending() { PendingOrder.clear(); PendingHead = 0; PendingMark.assign((size_t)(((W + Tile - 1) / Tile) * ((H + Tile - 1) / Tile)), 0); }
		void Queue(int T) { if (!PendingMark[(size_t)T]) { PendingMark[(size_t)T] = 1; PendingOrder.push_back(T); } }
		std::vector<FDraw> OverlayLog;
		// tiles in use
		bool bTextured = false;
		std::vector<uint8_t> TData[MatCount];
		int TMask[MatCount] = {}, TShift[MatCount] = {}, TOx[MatCount] = {}, TOy[MatCount] = {};
		bool TSwap[MatCount] = {};

		bool Unknown(int C) const { return Mode == EKnownMode::Native && !Known[(size_t)C]; }

		void ApplyRowData(const FCellRow& R, bool bStart)
		{
			const int C = R.C;
			if (C < 0 || C >= N) return;
			if (R.bData)
			{
				GroundL[(size_t)C] = R.Ground; ObjectL[(size_t)C] = R.Object; // absent fields stay absent (undefined in the browser)
				if (R.bMaterial) { Material[(size_t)C] = R.Material; HasMaterial[(size_t)C] = 1; } else { Material[(size_t)C].clear(); HasMaterial[(size_t)C] = 0; }
			}
			if (bWorld)
			{
				Known[(size_t)C] = 1;
				// rememberLook: a discovered cell's region look
				if (R.bData && R.Biome >= 0)
				{
					if (Biome.empty()) Biome.assign((size_t)N, 255);
					if (Biome[(size_t)C] != (uint8_t)R.Biome) { Biome[(size_t)C] = (uint8_t)R.Biome; BiomeCells.push_back(C); }
				}
			}
			Grid[(size_t)C] = R.Ch;
			State[(size_t)C] = (uint8_t)(R.bState ? R.State : 0);
			if (R.bLevel) Level[(size_t)C] = R.Level;
			(void)bStart;
		}

		// map.rows as the ground reads it: the start's rows on an authored map, the cells known so far on a World map
		uint8_t RowAt(int X, int Y) const
		{
			if (X < 0 || Y < 0 || X >= W || Y >= H) return 0;
			const int C = Y * W + X;
			return bWorld ? Grid[(size_t)C] : StartRows[(size_t)C];
		}

		// fieldCells: some 10 x 8 blocks of open ground get a crop field (3: FIELD, 9: FIELD_V)
		std::vector<uint8_t> FieldsOf() const
		{
			constexpr int BW = 10, BH = 8;
			std::vector<uint8_t> F((size_t)N, 0);
			for (int By = 0; By * BH < H; By++) for (int Bx = 0; Bx * BW < W; Bx++)
			{
				const int X0 = Bx * BW + 1, Y0 = By * BH + 1, X1 = std::min(W - 1, Bx * BW + BW - 1), Y1 = std::min(H - 1, By * BH + BH - 1);
				const int Sample = std::min(H - 1, By * BH + 4) * W + std::min(W - 1, Bx * BW + 5);
				const FBiomeGround* L = Biome.empty() || Unknown(Sample) ? nullptr : BiomeGround(Biome[(size_t)Sample]);
				if (Rnd(Bx, By, 7) > (L ? L->Fields : 0.24) || X1 - X0 < 4 || Y1 - Y0 < 3) continue;
				bool bKept = false;
				for (const auto& S : Spawns) if (S.X > X0 - 9 && S.X < X1 + 9 - 1 && S.Y > Y0 - 9 && S.Y < Y1 + 9 - 1) bKept = true;
				for (const auto& S : Points) if (S.X > X0 - 5 && S.X < X1 + 5 - 1 && S.Y > Y0 - 5 && S.Y < Y1 + 5 - 1) bKept = true;
				if (bKept) continue;
				bool bOpen = true;
				for (int Y = Y0; Y < Y1 && bOpen; Y++) for (int X = X0; X < X1; X++)
					if (RowAt(X, Y) != '.' || Unknown(Y * W + X)) { bOpen = false; break; } // Native: a field waits for its whole block
				if (!bOpen) continue;
				const uint8_t Dir = Rnd(Bx, By, 8) < 0.5 ? (uint8_t)Field : (uint8_t)FieldV;
				for (int Y = Y0; Y < Y1; Y++) for (int X = X0; X < X1; X++) F[(size_t)(Y * W + X)] = Dir;
			}
			return F;
		}

		// ---------- cellAttrs ----------
		uint8_t At(int X, int Y) const
		{
			if (X < 0 || Y < 0 || X >= W || Y >= H) return 0;
			const int C = Y * W + X;
			return Unknown(C) ? 0 : Grid[(size_t)C];
		}
		void CellAttrs(const std::vector<int>* Changed)
		{
			FAttrs& A = Attrs;
			if (!Changed)
			{
				A.Prim.assign((size_t)N, 0); A.Sec.assign((size_t)N, 0); A.Amt.assign((size_t)N, 0.f); A.Lev.assign((size_t)N, 0.f);
				A.Ov.assign((size_t)N, 0); A.Key.assign((size_t)N, 0); A.Scar.assign((size_t)N, 0); A.Char.assign((size_t)N, 0);
				A.Tr.assign((size_t)N, 0.f); A.Tg.assign((size_t)N, 0.f); A.Tb.assign((size_t)N, 0.f);
			}
			BiomeTint();
			const FNoise& Nz = FNoise::Get();
			auto One = [&](int I)
			{
				const int X = I % W, Y = I / W;
				const uint8_t Object = Grid[(size_t)I];
				uint8_t Ch = Object;
				if (Object == 'N') { const uint8_t O = ObjectL[(size_t)I]; Ch = O && O != '.' ? O : GroundL[(size_t)I] ? GroundL[(size_t)I] : '.'; }
				const double L = Level[(size_t)I];
				A.Ov[(size_t)I] = A.Scar[(size_t)I] = A.Char[(size_t)I] = 0;
				const int St = State[(size_t)I], Worn = St & 3;
				A.Lev[(size_t)I] = F32(L);
				int Pm = Grass, S = Dirt;
				double Am = 0;
				if (Unknown(I)) { A.Prim[(size_t)I] = Grass; A.Sec[(size_t)I] = Grass; A.Amt[(size_t)I] = 0.f; A.Key[(size_t)I] = 0; A.Tr[(size_t)I] = A.Tg[(size_t)I] = A.Tb[(size_t)I] = 1.f; return; }
				const uint8_t Fl = Fields[(size_t)I];
				if (Ch == '.')
				{
					bool bWet = false, bTown = false;
					for (int Dy = -1; Dy <= 1; Dy++) for (int Dx = -1; Dx <= 1; Dx++)
					{
						const uint8_t C2 = At(X + Dx, Y + Dy);
						if (C2 == 'W' || C2 == 'F' || C2 == '=') bWet = true;
						const int Nx = X + Dx, Ny = Y + Dy;
						if (Nx >= 0 && Ny >= 0 && Nx < W && Ny < H && !Unknown(Ny * W + Nx) && RowAt(Nx, Ny) == 'B') bTown = true;
					}
					const FBiomeGround* Lk = Biome.empty() ? nullptr : BiomeGround(Biome[(size_t)I]);
					if (Fl) { Pm = Fl; S = Grass; Am = 0.3; }
					else if (bWet) { S = Mud; Am = 0.55; }
					else if (bTown) { Pm = Road; S = Grass; Am = 0.3; }
					else if (L < 0) { S = Mud; Am = std::min(0.6, 0.25 + 0.12 * -L); }
					else { Am = 0.1 + 0.3 * ((double)Nz.A(X * 3 + 41, Y * 3 + 77) * 0.5 + 0.5); if (Lk) { S = Lk->Soil; Am += Lk->Bare; } }
					if (Worn) { S = Mud; Am = std::max(Am, 0.24 * Worn); }
					const int Shelled = (St >> 5) & 3;
					if (Shelled && !Fl) { S = Shell; Am = std::max(Am, 0.22 + 0.16 * Shelled); }
					else if (Shelled) Am = std::max(Am, 0.3 + 0.15 * Shelled);
					if (Shelled == 3) A.Scar[(size_t)I] = 1;
					if (St & 4) { A.Scar[(size_t)I] = 1; A.Char[(size_t)I] = 1; }
				}
				else if (Ch == 'B' || Ch == 'K') { Pm = Dirt; S = Road; Am = 0.3; }
				else if (Ch == 'R') { Pm = Dirt; S = Grass; Am = 0.35; A.Scar[(size_t)I] = 2; }
				else if (Ch == '+') { Pm = Grass; S = Worn ? Mud : Dirt; Am = 0.16 + 0.12 * Worn; A.Ov[(size_t)I] = 1; A.Scar[(size_t)I] = 1; }
				else if (Ch == 'T')
				{
					Pm = Earth; S = Mud; Am = 0.3; uint8_t Ov = 16;
					static const int Dirs[4][2] = { { 0, -1 }, { 1, 0 }, { 0, 1 }, { -1, 0 } };
					for (int K = 0; K < 4; K++) if (At(X + Dirs[K][0], Y + Dirs[K][1]) == 'T') Ov |= (uint8_t)(1 << K);
					A.Ov[(size_t)I] = Ov;
				}
				else if (Ch == 'W' || Ch == '=') Pm = Water;
				else if (Ch == 'F') { Pm = Water; S = Road; Am = 0.6 - 0.13 * Worn; }
				else if (Ch == 'H') { S = Mud; Am = 0.35; }
				else if (Ch == 'X') { S = Mud; Am = 0.45; }
				else if (Ch == 'Y' || Ch == 'Q') Am = 0.4;
				else if (Ch == '#') Am = 0.5;
				else if (Ch == 'D') { Pm = Road; S = Worn ? Shell : Dirt; Am = 0.12 + 0.2 * Worn; }
				else if (Ch == 'M') { Pm = Mud; Am = 0.42 - 0.12 * Worn; }
				else if (Ch == 'N') Am = 0.3;
				else if (Ch == 'O') { S = Mud; Am = 0.5; }
				else if (Ch == 'A') { Pm = Dirt; Am = 0.2; }
				if (!Am) S = Pm;
				A.Prim[(size_t)I] = (uint8_t)Pm; A.Sec[(size_t)I] = (uint8_t)S; A.Amt[(size_t)I] = F32(Am); A.Key[(size_t)I] = (uint8_t)(Pm | S << 4);
				const double K = Pm == Water ? 0 : Pm == Road || Pm == Dirt || Pm == Earth ? 0.4 : 1;
				A.Tr[(size_t)I] = F32(1 + ((double)TintR[(size_t)I] - 1) * K);
				A.Tg[(size_t)I] = F32(1 + ((double)TintG[(size_t)I] - 1) * K);
				A.Tb[(size_t)I] = F32(1 + ((double)TintB[(size_t)I] - 1) * K);
			};
			if (Changed) for (const int I : *Changed) One(I);
			else for (int I = 0; I < N; I++) One(I);
			// near: within three cells of a scar (the blast mark's pass runs there)
			if (Changed)
			{
				for (const int I : *Changed)
				{
					const int X = I % W, Y = I / W;
					A.Near[(size_t)I] = 0;
					for (int Ny = std::max(0, Y - 3); Ny <= std::min(H - 1, Y + 3) && !A.Near[(size_t)I]; Ny++)
						for (int Nx = std::max(0, X - 3); Nx <= std::min(W - 1, X + 3); Nx++) if (A.Scar[(size_t)(Ny * W + Nx)]) { A.Near[(size_t)I] = 1; break; }
				}
			}
			else
			{
				A.Near.assign((size_t)N, 0);
				for (int Y = 0; Y < H; Y++) for (int X = 0; X < W; X++)
				{
					if (!A.Scar[(size_t)(Y * W + X)]) continue;
					for (int Ny = std::max(0, Y - 3); Ny <= std::min(H - 1, Y + 3); Ny++) for (int Nx = std::max(0, X - 3); Nx <= std::min(W - 1, X + 3); Nx++) A.Near[(size_t)(Ny * W + Nx)] = 1;
				}
			}
			// depth: how many cells of scar lie between a scar cell and open ground
			if (!Changed) A.Depth.assign((size_t)N, 0.f);
			auto Depth = [&](int I)
			{
				const int X = I % W, Y = I / W;
				A.Depth[(size_t)I] = 0.f;
				if (!A.Scar[(size_t)I]) return;
				double Best = 8;
				for (int Dy = -6; Dy <= 6; Dy++)
				{
					const int Ny = Y + Dy;
					for (int Dx = -6; Dx <= 6; Dx++)
					{
						const int Nx = X + Dx;
						const bool bIn = Nx >= 0 && Ny >= 0 && Nx < W && Ny < H;
						if (bIn && A.Scar[(size_t)(Ny * W + Nx)]) continue;
						if (bIn && Unknown(Ny * W + Nx)) continue; // Native: unknown ground is neither scar nor clean
						const double D = JsHypot((double)Dx, (double)Dy);
						if (D < Best) Best = D;
					}
				}
				A.Depth[(size_t)I] = F32(Best);
			};
			if (Changed) for (const int I : *Changed) Depth(I);
			else for (int I = 0; I < N; I++) Depth(I);
			A.Valid = true;
		}

		// ---------- biomeTint: a 13-cell box blur of the known cells' looks, redone round newly arrived biomes ----------
		void BiomeTint()
		{
			if (Biome.empty()) { TintR.assign((size_t)N, 1.f); TintG = TintR; TintB = TintR; TintFresh = true; return; }
			const bool bFresh = TintFresh || (int)TintR.size() != N;
			if (bFresh) { TintR.assign((size_t)N, 1.f); TintG.assign((size_t)N, 1.f); TintB.assign((size_t)N, 1.f); }
			TintFresh = false;
			if (!bFresh && BiomeCells.empty()) return;
			constexpr int R = 6;
			int X0 = 0, Y0 = 0, X1 = W - 1, Y1 = H - 1;
			if (!bFresh)
			{
				X0 = W; Y0 = H; X1 = 0; Y1 = 0;
				for (const int C : BiomeCells) { const int X = C % W, Y = C / W; X0 = std::min(X0, X); X1 = std::max(X1, X); Y0 = std::min(Y0, Y); Y1 = std::max(Y1, Y); }
				X0 = std::max(0, X0 - R); Y0 = std::max(0, Y0 - R); X1 = std::min(W - 1, X1 + R); Y1 = std::min(H - 1, Y1 + R);
			}
			BiomeCells.clear();
			const int Ax = std::max(0, X0 - R), Ay = std::max(0, Y0 - R), Bx = std::min(W - 1, X1 + R), By = std::min(H - 1, Y1 + R);
			const int SW = Bx - Ax + 2, SH = By - Ay + 2;
			std::vector<float> Sw((size_t)SW * SH, 0.f), Sr((size_t)SW * SH, 0.f), Sg((size_t)SW * SH, 0.f), Sb((size_t)SW * SH, 0.f);
			for (int Y = Ay; Y <= By; Y++) for (int X = Ax; X <= Bx; X++)
			{
				const int C = Y * W + X;
				const FBiomeGround* L = Unknown(C) ? nullptr : BiomeGround(Biome[(size_t)C]);
				const size_t I = (size_t)((Y - Ay + 1) * SW + X - Ax + 1), Up = I - SW;
				Sw[I] = F32((L ? 1.0 : 0.0) + (double)Sw[I - 1] + (double)Sw[Up] - (double)Sw[Up - 1]);
				Sr[I] = F32((L ? L->Tint[0] : 0.0) + (double)Sr[I - 1] + (double)Sr[Up] - (double)Sr[Up - 1]);
				Sg[I] = F32((L ? L->Tint[1] : 0.0) + (double)Sg[I - 1] + (double)Sg[Up] - (double)Sg[Up - 1]);
				Sb[I] = F32((L ? L->Tint[2] : 0.0) + (double)Sb[I - 1] + (double)Sb[Up] - (double)Sb[Up - 1]);
			}
			auto Box = [SW](const std::vector<float>& T, int Qx0, int Qy0, int Qx1, int Qy1)
			{
				return (double)T[(size_t)(Qy1 * SW + Qx1)] - (double)T[(size_t)(Qy0 * SW + Qx1)] - (double)T[(size_t)(Qy1 * SW + Qx0)] + (double)T[(size_t)(Qy0 * SW + Qx0)];
			};
			for (int Y = Y0; Y <= Y1; Y++) for (int X = X0; X <= X1; X++)
			{
				const int Qx0 = std::max(Ax, X - R) - Ax, Qy0 = std::max(Ay, Y - R) - Ay, Qx1 = std::min(Bx, X + R) - Ax + 1, Qy1 = std::min(By, Y + R) - Ay + 1;
				const double K = Box(Sw, Qx0, Qy0, Qx1, Qy1);
				const size_t C = (size_t)(Y * W + X);
				if (K < 0.5) { TintR[C] = TintG[C] = TintB[C] = 1.f; continue; }
				TintR[C] = F32(Box(Sr, Qx0, Qy0, Qx1, Qy1) / K); TintG[C] = F32(Box(Sg, Qx0, Qy0, Qx1, Qy1) / K); TintB[C] = F32(Box(Sb, Qx0, Qy0, Qx1, Qy1) / K);
			}
		}

		// ---------- the blast mark (createCellNoise, createScarSampler, scarField, scarShape, scarWeights) ----------
		struct FNear
		{
			std::vector<double> Scars; // x, y, kind, depth
			std::vector<double> Clean; // x, y
			int Material = -1;
			std::unordered_map<uint64_t, std::array<double, 4>> Shapes; // n, run ax, run ay (NaN: no run), flags (1 big, 2 mid)
		};
		// Per paint: each cell's neighbourhood (built once, shared by every pixel that looks up from it) and the cell
		// noise lattice (rnd per lattice point; a direct-mapped cache, the browser memoises the same values).
		struct FScarCache
		{
			int Ox = 0, Oy = 0, SW = 0, SH = 0;
			std::vector<int> HoodIndex;     // (y + 4) * SW + (x + 4) -> index into Hoods, -1 none yet
			std::deque<FNear> Hoods;     // a deque: references stay valid as it grows
			std::vector<int64_t> LatKey;    // direct-mapped lattice cache
			std::vector<double> LatVal;
			void Reset(int InW, int InH)
			{
				SW = InW + 8; SH = InH + 8; Ox = 4; Oy = 4;
				HoodIndex.assign((size_t)SW * SH, -1);
				Hoods.clear();
				if (LatKey.empty()) { LatKey.assign((size_t)1 << 16, INT64_MIN); LatVal.assign((size_t)1 << 16, 0); }
				else std::fill(LatKey.begin(), LatKey.end(), INT64_MIN);
			}
		};
		FScarCache Scars;
		double CellNoise(double U, double V, int K)
		{
			const double Fx0 = std::floor(U), Fy0 = std::floor(V), Tx = U - Fx0, Ty = V - Fy0;
			const double Sx = Tx * Tx * (3 - 2 * Tx), Sy = Ty * Ty * (3 - 2 * Ty);
			auto Sample = [&](double X, double Y)
			{
				const int64_t Key = ((int64_t)((int)X + 8192) << 24) | ((int64_t)((int)Y + 8192) << 8) | (int64_t)K;
				const size_t Slot = (size_t)(((uint64_t)Key * 0x9E3779B97F4A7C15ull) >> 48);
				if (Scars.LatKey[Slot] == Key) return Scars.LatVal[Slot];
				const double R = Rnd(X, Y, K);
				Scars.LatKey[Slot] = Key; Scars.LatVal[Slot] = R;
				return R;
			};
			const double A = Sample(Fx0, Fy0), B = Sample(Fx0 + 1, Fy0), C = Sample(Fx0, Fy0 + 1), D = Sample(Fx0 + 1, Fy0 + 1);
			return (A + (B - A) * Sx) * (1 - Sy) + (C + (D - C) * Sx) * Sy;
		}
		FNear& Nearby(double U, double V)
		{
			const int X0 = ToInt(U), Y0 = ToInt(V);
			const bool bDense = X0 + Scars.Ox >= 0 && Y0 + Scars.Oy >= 0 && X0 + Scars.Ox < Scars.SW && Y0 + Scars.Oy < Scars.SH;
			const size_t Di = bDense ? (size_t)((Y0 + Scars.Oy) * Scars.SW + X0 + Scars.Ox) : 0;
			if (bDense && Scars.HoodIndex[Di] >= 0) return Scars.Hoods[(size_t)Scars.HoodIndex[Di]];
			FNear R;
			for (int Y = Y0 - 3; Y <= Y0 + 3; Y++)
			{
				double Left = NAN, Middle = NAN, Right = NAN;
				for (int X = X0 - 3; X <= X0 + 3; X++)
				{
					const bool bIn = X >= 0 && Y >= 0 && X < W && Y < H;
					const int Cell = Y * W + X, Kind = bIn ? Attrs.Scar[(size_t)Cell] : 0;
					if (Kind)
					{
						R.Scars.insert(R.Scars.end(), { X + 0.5, Y + 0.5, (double)Kind, (double)Attrs.Depth[(size_t)Cell] });
						R.Material = R.Material == -1 ? Kind : R.Material == Kind ? Kind : 0;
					}
					else if (bIn && Unknown(Cell)) continue; // Native: unknown ground is not clean ground
					else if (X < X0) Left = X + 0.5;
					else if (X == X0) Middle = X + 0.5;
					else if (std::isnan(Right)) Right = X + 0.5;
				}
				for (const double Cx : { Left, Middle, Right }) if (!std::isnan(Cx)) { R.Clean.push_back(Cx); R.Clean.push_back(Y + 0.5); }
			}
			// far outside the map (a warp never reaches there): built every time, not kept
			if (!bDense) { Scars.Hoods.push_back(std::move(R)); FNear& Tmp = Scars.Hoods.back(); return Tmp; }
			Scars.HoodIndex[Di] = (int)Scars.Hoods.size();
			Scars.Hoods.push_back(std::move(R));
			return Scars.Hoods.back();
		}
		struct FField { double Shell, Rubble, DScar, DClean; };
		FField ScarField(double U, double V)
		{
			double ShellW = 0, RubbleW = 0, ScarSq = 81, CleanSq = 81;
			const FNear& Nb = Nearby(U, V);
			for (size_t I = 0; I < Nb.Clean.size(); I += 2)
			{
				const double Dx = U - Nb.Clean[I], Dy = V - Nb.Clean[I + 1], D = Dx * Dx + Dy * Dy;
				if (D < CleanSq) CleanSq = D;
			}
			const double Reach2 = 2.4 * 2.4;
			for (size_t I = 0; I < Nb.Scars.size(); I += 4)
			{
				const double Dx = U - Nb.Scars[I], Dy = V - Nb.Scars[I + 1], Sq = Dx * Dx + Dy * Dy;
				if (Sq < ScarSq) ScarSq = Sq;
				if (Nb.Material || Sq >= Reach2) continue;
				const double Inf0 = 1 - JsHypot(Dx, Dy) / 2.4, Inf = Inf0 * Inf0;
				if (Nb.Scars[I + 2] == 1) ShellW += Inf; else RubbleW += Inf;
			}
			if (Nb.Material > 0 && ScarSq < Reach2) { ShellW = Nb.Material == 1 ? 1 : 0; RubbleW = Nb.Material == 2 ? 1 : 0; }
			return { ShellW, RubbleW, std::sqrt(ScarSq), std::sqrt(CleanSq) };
		}
		struct FShape { int N; bool bRun; double Ax, Ay; bool bBig, bMid; };
		FShape ScarShape(FNear& Nb, double U, double V, double& NearestSq)
		{
			uint64_t Mask = 0;
			NearestSq = INFINITY;
			for (size_t I = 0; I < Nb.Scars.size(); I += 4)
			{
				const double Dx = U - Nb.Scars[I], Dy = V - Nb.Scars[I + 1], Sq = Dx * Dx + Dy * Dy;
				if (Sq < NearestSq) NearestSq = Sq;
				if (Sq <= 3.2 * 3.2) Mask |= (uint64_t)1 << (I / 4);
			}
			auto It = Nb.Shapes.find(Mask);
			if (It != Nb.Shapes.end()) { const auto& S = It->second; return { (int)S[0], !std::isnan(S[1]), S[1], S[2], ((int)S[3] & 1) != 0, ((int)S[3] & 2) != 0 }; }
			double Mass = 0, Sx = 0, Sy = 0, Sxx = 0, Syy = 0, Sxy = 0;
			int Cnt = 0;
			for (size_t I = 0; I < Nb.Scars.size(); I += 4)
			{
				if (!(Mask & ((uint64_t)1 << (I / 4)))) continue;
				const double Cx = Nb.Scars[I], Cy = Nb.Scars[I + 1];
				Cnt++; Sx += Cx; Sy += Cy; Sxx += Cx * Cx; Syy += Cy * Cy; Sxy += Cx * Cy;
				if (Nb.Scars[I + 3] > Mass) Mass = Nb.Scars[I + 3];
			}
			bool bRun = false; double Ax = NAN, Ay = NAN;
			if (Cnt >= 5)
			{
				const double Mx = Sx / Cnt, My = Sy / Cnt;
				const double Cxx = Sxx / Cnt - Mx * Mx, Cyy = Syy / Cnt - My * My, Cxy = Sxy / Cnt - Mx * My;
				const double Tr = Cxx + Cyy, Det = Cxx * Cyy - Cxy * Cxy;
				const double Disc = std::max(0.0, Tr * Tr * 0.25 - Det);
				const double Major = Tr * 0.5 + std::sqrt(Disc), Minor = std::max(0.0, Tr * 0.5 - std::sqrt(Disc));
				if (Major > 2.2 && Minor < 1.65 && Minor < Major / 2.4)
				{
					const double Ang = 0.5 * std::atan2(2 * Cxy, Cxx - Cyy);
					bRun = true; Ax = Fdlibm::Cos(Ang); Ay = Fdlibm::Sin(Ang);
				}
			}
			const bool bBig = !bRun && Mass >= 3, bMid = !bRun && !bBig && Mass >= 1.8;
			Nb.Shapes.emplace(Mask, std::array<double, 4>{ (double)Cnt, bRun ? Ax : NAN, Ay, (double)((bBig ? 1 : 0) | (bMid ? 2 : 0)) });
			return { Cnt, bRun, Ax, Ay, bBig, bMid };
		}
		struct FScar { double Cover, Shell, Rubble; };
		FScar ScarWeights(double U, double V)
		{
			double NearestSq;
			FShape S = ScarShape(Nearby(U, V), U, V, NearestSq);
			const double Reach = S.bRun ? JsHypot(0.34, 0.42) : 1.4142135623730951 * ((S.bBig ? 0.78 : S.bMid ? 0.16 : 0.04) * 1.7 + (S.bBig ? 0.62 : 0));
			const double ReachAll = 2.4 + Reach + 1e-9;
			if (!S.N || NearestSq > ReachAll * ReachAll) return { 0, 0, 0 };
			double Nu, Nv, AmpW, BiteAmp;
			if (S.bRun)
			{
				const double Sa = U * S.Ax + V * S.Ay, Ta = -U * S.Ay + V * S.Ax;
				const double AlongN = CellNoise(Sa * 0.18 + 2, Ta * 0.18 + 1, 31) * 2 - 1;
				const double AcrossN = CellNoise(Sa * 0.31 + 5, 8.2, 32) * 2 - 1;
				Nu = U + S.Ax * AlongN * 0.34 + (-S.Ay) * AcrossN * 0.42;
				Nv = V + S.Ay * AlongN * 0.34 + S.Ax * AcrossN * 0.42;
				AmpW = 0.42; BiteAmp = 0.22;
			}
			else
			{
				const double Wp = S.bBig ? 0.78 : S.bMid ? 0.16 : 0.04;
				AmpW = S.bBig ? 1.35 : S.bMid ? 0.34 : 0.14;
				BiteAmp = S.bBig ? 0.62 : S.bMid ? 0.1 : 0.05;
				auto Slow = [this](double Pu, double Pv, int K) { return CellNoise(Pu * 0.26 + K, Pv * 0.24 + K * 1.3, K) * 2 - 1; };
				auto Fast = [this](double Pu, double Pv, int K) { return CellNoise(Pu * 0.62 + K * 2, Pv * 0.57 + 3, K) * 2 - 1; };
				const double Rip = S.bBig ? 0.62 : 0;
				Nu = U + Slow(U + 2.2, V + 5.1, 4) * Wp + Fast(U, V, 6) * Wp * 0.7
					+ Fdlibm::Sin(V * 2.6 + U * 0.7) * Fdlibm::Sin(U * 1.15 + V * 0.45 + 1.7) * Rip;
				Nv = V + Slow(U + 8.4, V + 1.3, 5) * Wp + Fast(U + 4, V, 7) * Wp * 0.7
					+ Fdlibm::Sin(U * 2.35 + V * 0.55) * Fdlibm::Sin(V * 1.25 + U * 0.4 + 0.6) * Rip;
			}
			const FField F = ScarField(Nu, Nv);
			const double Wave = CellNoise(U * 0.45 + 1.7, V * 0.45 + 4.2, 11) * 2 - 1;
			const double Bite = CellNoise(U * 1.7 + 3.0, V * 1.4 + 6.0, 8) * 2 - 1;
			const double Inset = F.DClean - F.DScar;
			const double Shifted = Inset - 0.08 + AmpW * Wave + BiteAmp * Bite;
			double Cover = Shifted <= 0 ? 0 : Shifted >= 0.65 ? 1 : Shifted / 0.65;
			Cover = Cover * Cover * (3 - 2 * Cover);
			if (!S.bRun && !S.bBig && F.DScar < (S.bMid ? 0.28 : 0.34)) Cover = 1;
			if ((S.bBig && Inset > 2.05) || (S.bRun && Inset > 1.45))
			{
				const double Fleck = CellNoise(U * 3.1 + 4, V * 3.1 + 2, 19);
				Cover = std::max(Cover, Fleck > 0.93 ? 0.5 : 1.0);
			}
			if (Cover > 1) Cover = 1;
			const double Tot = F.Shell + F.Rubble;
			return Tot && Cover > 0 ? FScar{ Cover, F.Shell / Tot, F.Rubble / Tot } : FScar{ 0, 0, 0 };
		}

		// ---------- tiles ----------
		void UseTiles(const FTile* Tiles)
		{
			bTextured = false;
			if (Tiles)
			{
				bTextured = true;
				for (int M = 0; M < MatCount - 1; M++) if (Tiles[M].T != TileSize(M, P) || (int)Tiles[M].Data.size() != Tiles[M].T * Tiles[M].T * 4) bTextured = false;
			}
			for (int M = 0; M < MatCount; M++)
			{
				const int Src = M == FieldV ? (int)Field : M;
				if (bTextured)
				{
					const int T = Tiles[Src].T;
					TData[M] = Tiles[Src].Data;
					TMask[M] = T - 1; TShift[M] = (int)std::round(std::log2(T));
					const int Ox = (int)std::floor(Rnd(Src, 3) * T), Oy = (int)std::floor(Rnd(Src, 5) * T);
					// FIELD_V samples the field tile transposed, with its offsets swapped
					TOx[M] = M == FieldV ? Oy : Ox; TOy[M] = M == FieldV ? Ox : Oy;
				}
				else
				{
					uint8_t C[3]; FlatColor(M, C);
					TData[M] = { C[0], C[1], C[2], 255 };
					TMask[M] = TShift[M] = TOx[M] = TOy[M] = 0;
				}
				TSwap[M] = M == FieldV;
			}
		}
		int TQ(int M, int I, int J) const
		{
			return (TSwap[M] ? ((((I + TOy[M]) & TMask[M]) << TShift[M]) | ((J + TOx[M]) & TMask[M]))
				: ((((J + TOy[M]) & TMask[M]) << TShift[M]) | ((I + TOx[M]) & TMask[M]))) << 2;
		}

		// ---------- raster ----------
		void RasterRect(int X0, int Y0, int RW, int RH)
		{
			const FNoise& Nz = FNoise::Get();
			const FAttrs& A = Attrs;
			const bool bTinted = !Biome.empty();
			const int CW = W * P;
			std::vector<float> CU((size_t)RW), CPt((size_t)RW);
			std::vector<int> CWx((size_t)RW), CWy((size_t)RW), CP0((size_t)RW), CP1((size_t)RW), CB((size_t)RW);
			for (int C = 0; C < RW; C++)
			{
				const double U = (X0 + C + 0.5) / P, Px = U * 6 + 57;
				CU[(size_t)C] = F32(U); CWx[(size_t)C] = ToInt(U * 16 + 1101) & NM; CWy[(size_t)C] = ToInt(U * 16 + 311) & NM;
				CP0[(size_t)C] = (int)std::floor(Px) & NM; CP1[(size_t)C] = ((int)std::floor(Px) + 1) & NM; CPt[(size_t)C] = F32(Px - std::floor(Px));
				CB[(size_t)C] = ToInt(U * 2.3 + 900) & NM;
			}
			float Wt[MatCount] = {};
			for (int J = Y0; J < Y0 + RH; J++)
			{
				const double V = (J + 0.5) / P, Py = V * 6 + 213;
				const int RWx = (ToInt(V * 16 + 37) & NM) << 9, RWy = (ToInt(V * 16 + 1211) & NM) << 9, RB = (ToInt(V * 2.3 + 431) & NM) << 9;
				const int RP0 = ((int)std::floor(Py) & NM) << 9, RP1 = (((int)std::floor(Py) + 1) & NM) << 9;
				const double RPt = Py - std::floor(Py);
				for (int C = 0; C < RW; C++)
				{
					const int I = X0 + C;
					const double U = CU[(size_t)C];
					uint8_t* Out = &Raster[((size_t)J * CW + I) * 4];
					const int Gx = U <= 0 ? 0 : U >= W ? W - 1 : ToInt(U), Gy = V <= 0 ? 0 : V >= H ? H - 1 : ToInt(V);
					const int Own = Gy * W + Gx;
					if (Unknown(Own)) { Out[0] = Out[1] = Out[2] = 10; Out[3] = 255; continue; } // never painted from anything
					const double Fx = U + (double)Nz.NA[(size_t)(RWx | CWx[(size_t)C])] * Warp - 0.5, Fy = V + (double)Nz.NA[(size_t)(RWy | CWy[(size_t)C])] * Warp - 0.5;
					int Ix0 = (int)std::floor(Fx), Iy0 = (int)std::floor(Fy);
					const double Tx = Fx - Ix0, Ty = Fy - Iy0;
					int Ix1 = Ix0 + 1, Iy1 = Iy0 + 1;
					Ix0 = std::min(std::max(Ix0, 0), W - 1); Ix1 = std::min(std::max(Ix1, 0), W - 1);
					Iy0 = std::min(std::max(Iy0, 0), H - 1); Iy1 = std::min(std::max(Iy1, 0), H - 1);
					int C00 = Iy0 * W + Ix0, C01 = Iy0 * W + Ix1, C10 = Iy1 * W + Ix0, C11 = Iy1 * W + Ix1;
					if (Unknown(C00)) C00 = Own;
					if (Unknown(C01)) C01 = Own;
					if (Unknown(C10)) C10 = Own;
					if (Unknown(C11)) C11 = Own;
					const double Pa = Nz.NP[(size_t)(RP0 | CP0[(size_t)C])], Pb = Nz.NP[(size_t)(RP0 | CP1[(size_t)C])], Pc = Nz.NP[(size_t)(RP1 | CP0[(size_t)C])], Pd = Nz.NP[(size_t)(RP1 | CP1[(size_t)C])];
					const double Ptx = CPt[(size_t)C];
					const double Pn = (Pa + (Pb - Pa) * Ptx) * (1 - RPt) + (Pc + (Pd - Pc) * Ptx) * RPt;
					double R = 0, G = 0, Bl = 0, WWater = 0;
					const uint8_t K00 = A.Key[(size_t)C00];
					if (K00 == A.Key[(size_t)C01] && K00 == A.Key[(size_t)C10] && K00 == A.Key[(size_t)C11])
					{
						const int Pm = A.Prim[(size_t)C00], Q = A.Sec[(size_t)C00];
						const double Am = ((double)A.Amt[(size_t)C00] * (1 - Tx) + (double)A.Amt[(size_t)C01] * Tx) * (1 - Ty) + ((double)A.Amt[(size_t)C10] * (1 - Tx) + (double)A.Amt[(size_t)C11] * Tx) * Ty;
						double S = 0;
						if (Am > 0) { S = (Am - Pn) * 5.5 + 0.5; S = S < 0 ? 0 : S > 1 ? 1 : S * S * (3 - 2 * S); }
						if (S < 1) { const uint8_t* D = &TData[Pm][(size_t)TQ(Pm, I, J)]; const double F = 1 - S; R = F * D[0]; G = F * D[1]; Bl = F * D[2]; if (Pm == Water) WWater = F; }
						if (S > 0) { const uint8_t* D = &TData[Q][(size_t)TQ(Q, I, J)]; R += S * D[0]; G += S * D[1]; Bl += S * D[2]; if (Q == Water) WWater += S; }
					}
					else
					{
						double Aw = (1 - Tx) * (1 - Ty), Bw = Tx * (1 - Ty), Cw = (1 - Tx) * Ty, Ew = Tx * Ty;
						Aw *= Aw; Bw *= Bw; Cw *= Cw; Ew *= Ew;
						const double Sum = Aw + Bw + Cw + Ew;
						const int Cs[4] = { C00, C01, C10, C11 };
						const double Ws[4] = { Aw, Bw, Cw, Ew };
						for (int K = 0; K < 4; K++)
						{
							const int Ci = Cs[K];
							const double Wc = Ws[K] / Sum, Am = A.Amt[(size_t)Ci];
							if (Am > 0)
							{
								double S = (Am - Pn) * 5.5 + 0.5;
								S = S < 0 ? 0 : S > 1 ? 1 : S * S * (3 - 2 * S);
								Wt[A.Sec[(size_t)Ci]] = F32((double)Wt[A.Sec[(size_t)Ci]] + Wc * S);
								Wt[A.Prim[(size_t)Ci]] = F32((double)Wt[A.Prim[(size_t)Ci]] + Wc * (1 - S));
							}
							else Wt[A.Prim[(size_t)Ci]] = F32((double)Wt[A.Prim[(size_t)Ci]] + Wc);
						}
						WWater = Wt[Water];
						for (int M = 0; M < MatCount; M++)
						{
							const double Wm = Wt[M];
							if (Wm == 0) continue;
							Wt[M] = 0;
							if (Wm < 0.004) continue;
							const uint8_t* D = &TData[M][(size_t)TQ(M, I, J)];
							R += Wm * D[0]; G += Wm * D[1]; Bl += Wm * D[2];
						}
					}
					const double W00 = (1 - Tx) * (1 - Ty), W01 = Tx * (1 - Ty), W10 = (1 - Tx) * Ty, W11 = Tx * Ty;
					if (bTinted)
					{
						R *= (double)A.Tr[(size_t)C00] * W00 + (double)A.Tr[(size_t)C01] * W01 + (double)A.Tr[(size_t)C10] * W10 + (double)A.Tr[(size_t)C11] * W11;
						G *= (double)A.Tg[(size_t)C00] * W00 + (double)A.Tg[(size_t)C01] * W01 + (double)A.Tg[(size_t)C10] * W10 + (double)A.Tg[(size_t)C11] * W11;
						Bl *= (double)A.Tb[(size_t)C00] * W00 + (double)A.Tb[(size_t)C01] * W01 + (double)A.Tb[(size_t)C10] * W10 + (double)A.Tb[(size_t)C11] * W11;
					}
					if (A.Near[(size_t)Own])
					{
						const FScar Sc = ScarWeights(U, V);
						if (Sc.Cover > 0.015)
						{
							const double Cover = Sc.Cover;
							const uint8_t* Sd = &TData[Shell][(size_t)TQ(Shell, I, J)];
							const uint8_t* Rd = &TData[Rubble][(size_t)TQ(Rubble, I, J)];
							const double Burn = (double)A.Char[(size_t)C00] * W00 + (double)A.Char[(size_t)C01] * W01 + (double)A.Char[(size_t)C10] * W10 + (double)A.Char[(size_t)C11] * W11;
							const double Shade = (0.7 + 0.3 * (1 - Cover * Cover)) * (1 - 0.5 * Burn);
							const double Sr = Sc.Shell * Sd[0] * Shade + Sc.Rubble * Rd[0];
							const double Sg = Sc.Shell * Sd[1] * Shade + Sc.Rubble * Rd[1];
							const double Sb = Sc.Shell * Sd[2] * Shade * 1.06 + Sc.Rubble * Rd[2];
							const double Ink = Cover * (0.35 + 0.65 * Cover), Keep = 1 - Ink;
							R = R * Keep + Sr * Ink; G = G * Keep + Sg * Ink; Bl = Bl * Keep + Sb * Ink;
						}
					}
					const double Br = 1 + 0.14 * (double)Nz.NA[(size_t)(RB | CB[(size_t)C])];
					R *= Br; G *= Br; Bl *= Br;
					if (WWater > 0.06 && WWater < 0.94)
					{
						double Kk = 1 - std::fabs(WWater - 0.5) * 2.2;
						if (Kk > 0) { Kk *= Kk * 0.45; R += (Foam[0] - R) * Kk; G += (Foam[1] - G) * Kk; Bl += (Foam[2] - Bl) * Kk; }
					}
					Out[0] = ClampU8(R); Out[1] = ClampU8(G); Out[2] = ClampU8(Bl); Out[3] = 255;
				}
			}
		}

		// ---------- overlays: shell holes, trench cuts, contour lines near cliffs ----------
		void Overlays(int Cx0, int Cy0, int Cx1, int Cy1, const double* Clip, std::vector<FDraw>& Out) const
		{
			const FAttrs& A = Attrs;
			Cx0 = std::max(0, Cx0); Cy0 = std::max(0, Cy0); Cx1 = std::min(W - 1, Cx1); Cy1 = std::min(H - 1, Cy1);
			auto WithClip = [Clip](FDraw& D) { if (Clip) { D.bClip = true; std::memcpy(D.Clip, Clip, sizeof(D.Clip)); } };
			std::vector<std::array<int, 3>> Holes, Cuts;
			for (int Y = Cy0; Y <= Cy1; Y++) for (int X = Cx0; X <= Cx1; X++)
			{
				const int F = A.Ov[(size_t)(Y * W + X)];
				if (F == 1) Holes.push_back({ X, Y, F }); else if (F & 16) Cuts.push_back({ X, Y, F });
			}
			auto HoleAt = [&](int X, int Y) { return X >= 0 && Y >= 0 && X < W && Y < H && A.Ov[(size_t)(Y * W + X)] == 1; };
			for (const auto& Hl : Holes)
			{
				const int X = Hl[0], Y = Hl[1];
				const bool bBeside = HoleAt(X + 1, Y) || HoleAt(X - 1, Y) || HoleAt(X, Y + 1) || HoleAt(X, Y - 1);
				if (bBeside && Rnd(X, Y, 21) < 0.82) continue;
				const double Rad = (0.08 + 0.14 * Rnd(X, Y, 24)) * P;
				const double Cx = (X + 0.5 + (Rnd(X, Y, 22) - 0.5) * 0.36) * P, Cy = (Y + 0.5 + (Rnd(X, Y, 23) - 0.5) * 0.36) * P;
				FDraw D;
				D.bGradient = true;
				const double G[6] = { Cx + Rad * 0.2, Cy - Rad * 0.2, Rad * 0.04, Cx, Cy, Rad };
				std::memcpy(D.G, G, sizeof(G));
				D.Stops = { { 0, 0x1a, 0x14, 0x0e, 1 }, { 0.7, 0x3a, 0x2e, 0x22, 1 }, { 1, 70, 54, 36, 0 } };
				D.Path.push_back({ 'A', { Cx, Cy, Rad, 0, 6.283185307179586 } });
				WithClip(D);
				Out.push_back(std::move(D));
			}
			if (!Cuts.empty())
			{
				static const int Dirs[4][2] = { { 0, -1 }, { 1, 0 }, { 0, 1 }, { -1, 0 } };
				std::vector<FPathOp> Path;
				for (const auto& Cu : Cuts)
				{
					const int X = Cu[0], Y = Cu[1], F = Cu[2];
					const double Cx = (X + 0.5) * P, Cy = (Y + 0.5) * P;
					Path.push_back({ 'M', { Cx, Cy } });
					if (!(F & 15)) Path.push_back({ 'L', { Cx + 0.01, Cy } });
					for (int K = 0; K < 4; K++) if (F & (1 << K)) { Path.push_back({ 'M', { Cx, Cy } }); Path.push_back({ 'L', { Cx + Dirs[K][0] * P * 0.5, Cy + Dirs[K][1] * P * 0.5 } }); }
				}
				const struct { uint8_t R, G, B; double A; double Width; } Strokes[3] = {
					{ 58, 44, 28, 0.85, P * 0.8 }, { 0x2b, 0x22, 0x18, 1, P * 0.5 }, { 128, 102, 66, 0.5, std::max(1.0, P * 0.07) } };
				for (const auto& S : Strokes)
				{
					FDraw D; D.bStroke = true; D.Stops = { { 0, S.R, S.G, S.B, S.A } }; D.LineWidth = S.Width; D.Path = Path; WithClip(D);
					Out.push_back(std::move(D));
				}
			}
			// contour ink only near true cliffs (a step of two levels or more between neighbouring cell centres)
			auto Lv = [&](int X, int Y) { return (double)A.Lev[(size_t)(std::min(H - 1, std::max(0, Y)) * W + std::min(W - 1, std::max(0, X)))]; };
			auto Hidden = [&](int X, int Y) { return Unknown(std::min(H - 1, std::max(0, Y)) * W + std::min(W - 1, std::max(0, X))); };
			std::vector<FPathOp> Path;
			for (int Y = std::max(-1, Cy0 - 1); Y <= Cy1; Y++) for (int X = std::max(-1, Cx0 - 1); X <= Cx1; X++)
			{
				const double Av = Lv(X, Y), Bv = Lv(X + 1, Y), Cv = Lv(X + 1, Y + 1), Dv = Lv(X, Y + 1);
				const double Lo = std::min(std::min(Av, Bv), std::min(Cv, Dv)), Hi = std::max(std::max(Av, Bv), std::max(Cv, Dv));
				if (Lo == Hi) continue;
				if (std::max(std::max(std::fabs(Av - Bv), std::fabs(Bv - Cv)), std::max(std::fabs(Cv - Dv), std::fabs(Dv - Av))) < 2) continue;
				if (Hidden(X, Y) || Hidden(X + 1, Y) || Hidden(X + 1, Y + 1) || Hidden(X, Y + 1)) continue; // Native: no line through unknown ground
				const double Ox = (X + 0.5) * P, Oy = (Y + 0.5) * P;
				for (double T = Lo + 0.5; T < Hi; T++)
				{
					std::vector<std::array<double, 2>> Pts;
					if ((Av > T) != (Bv > T)) Pts.push_back({ (T - Av) / (Bv - Av), 0 });
					if ((Bv > T) != (Cv > T)) Pts.push_back({ 1, (T - Bv) / (Cv - Bv) });
					if ((Dv > T) != (Cv > T)) Pts.push_back({ (T - Dv) / (Cv - Dv), 1 });
					if ((Av > T) != (Dv > T)) Pts.push_back({ 0, (T - Av) / (Dv - Av) });
					auto Seg = [&](const std::array<double, 2>& Pp, const std::array<double, 2>& Q)
					{
						Path.push_back({ 'M', { Ox + Pp[0] * P, Oy + Pp[1] * P } }); Path.push_back({ 'L', { Ox + Q[0] * P, Oy + Q[1] * P } });
					};
					if (Pts.size() == 2) Seg(Pts[0], Pts[1]);
					else if (Pts.size() == 4)
					{
						const bool bMid = (Av + Bv + Cv + Dv) / 4 > T, bAUp = Av > T;
						if (bMid == bAUp) { Seg(Pts[0], Pts[1]); Seg(Pts[2], Pts[3]); } else { Seg(Pts[0], Pts[3]); Seg(Pts[1], Pts[2]); }
					}
				}
			}
			if (!Path.empty())
			{
				FDraw D; D.bStroke = true; D.Stops = { { 0, 48, 40, 22, 0.55 } }; D.LineWidth = std::max(1.2, P * 0.12); D.Path = std::move(Path); WithClip(D);
				Out.push_back(std::move(D));
			}
		}

		// ---------- a small canvas rasteriser for those overlays (source-over, anti-aliased by coverage) ----------
		void Blend(int X, int Y, double R, double G, double B, double Alpha)
		{
			if (Alpha <= 0) return;
			uint8_t* Px = &Pixels[((size_t)Y * W * P + X) * 4];
			const double K = std::min(1.0, Alpha);
			Px[0] = ClampU8(Px[0] + (R - Px[0]) * K); Px[1] = ClampU8(Px[1] + (G - Px[1]) * K); Px[2] = ClampU8(Px[2] + (B - Px[2]) * K);
		}
		static double SegDist(double Px, double Py, double Ax, double Ay, double Bx, double By)
		{
			const double Dx = Bx - Ax, Dy = By - Ay, L2 = Dx * Dx + Dy * Dy;
			const double T = L2 > 0 ? std::max(0.0, std::min(1.0, ((Px - Ax) * Dx + (Py - Ay) * Dy) / L2)) : 0;
			return std::hypot(Px - Ax - Dx * T, Py - Ay - Dy * T);
		}
		void Draw(const FDraw& D)
		{
			const int CW = W * P, CH = H * P;
			double Bx0 = 1e18, By0 = 1e18, Bx1 = -1e18, By1 = -1e18, Pad = D.bStroke ? D.LineWidth * 0.5 + 1 : 1;
			for (const FPathOp& Op : D.Path)
			{
				const double R = Op.Kind == 'A' ? Op.V[2] : 0;
				Bx0 = std::min(Bx0, Op.V[0] - R); By0 = std::min(By0, Op.V[1] - R); Bx1 = std::max(Bx1, Op.V[0] + R); By1 = std::max(By1, Op.V[1] + R);
			}
			int X0 = std::max(0, (int)std::floor(Bx0 - Pad)), Y0 = std::max(0, (int)std::floor(By0 - Pad)), X1 = std::min(CW, (int)std::ceil(Bx1 + Pad)), Y1 = std::min(CH, (int)std::ceil(By1 + Pad));
			if (D.bClip) { X0 = std::max(X0, (int)D.Clip[0]); Y0 = std::max(Y0, (int)D.Clip[1]); X1 = std::min(X1, (int)(D.Clip[0] + D.Clip[2])); Y1 = std::min(Y1, (int)(D.Clip[1] + D.Clip[3])); }
			if (X0 >= X1 || Y0 >= Y1) return;
			if (D.bStroke)
			{
				// segments: each moveTo/lineTo pair (the paths here are only those); round caps and joins make the union
				std::vector<std::array<double, 4>> Segs;
				double Mx = 0, My = 0;
				for (const FPathOp& Op : D.Path)
				{
					if (Op.Kind == 'M') { Mx = Op.V[0]; My = Op.V[1]; }
					else if (Op.Kind == 'L') { Segs.push_back({ Mx, My, Op.V[0], Op.V[1] }); Mx = Op.V[0]; My = Op.V[1]; }
				}
				// coverage of the union of round-capped segments, built segment by segment over each one's own box
				const double Half = D.LineWidth * 0.5;
				const FStop& S = D.Stops[0];
				const int BW = X1 - X0, BH = Y1 - Y0;
				std::vector<float> Cov((size_t)BW * BH, 0.f);
				for (const auto& Sg : Segs)
				{
					const int Sx0 = std::max(X0, (int)std::floor(std::min(Sg[0], Sg[2]) - Half - 1)), Sx1 = std::min(X1, (int)std::ceil(std::max(Sg[0], Sg[2]) + Half + 1));
					const int Sy0 = std::max(Y0, (int)std::floor(std::min(Sg[1], Sg[3]) - Half - 1)), Sy1 = std::min(Y1, (int)std::ceil(std::max(Sg[1], Sg[3]) + Half + 1));
					for (int Y = Sy0; Y < Sy1; Y++) for (int X = Sx0; X < Sx1; X++)
					{
						const double C = std::max(0.0, std::min(1.0, Half - SegDist(X + 0.5, Y + 0.5, Sg[0], Sg[1], Sg[2], Sg[3]) + 0.5));
						float& Cell = Cov[(size_t)((Y - Y0) * BW + X - X0)];
						if (C > Cell) Cell = (float)C;
					}
				}
				for (int Y = Y0; Y < Y1; Y++) for (int X = X0; X < X1; X++)
				{
					const float C = Cov[(size_t)((Y - Y0) * BW + X - X0)];
					if (C > 0) Blend(X, Y, S.R, S.G, S.B, S.A * C);
				}
				return;
			}
			// a filled circle with a two-circle radial gradient (canvas createRadialGradient: the largest t whose circle covers the point)
			const FPathOp& Arc = D.Path[0];
			for (int Y = Y0; Y < Y1; Y++) for (int X = X0; X < X1; X++)
			{
				const double Px = X + 0.5, Py = Y + 0.5;
				const double Cov = std::max(0.0, std::min(1.0, Arc.V[2] - std::hypot(Px - Arc.V[0], Py - Arc.V[1]) + 0.5));
				if (Cov <= 0) continue;
				double T = 1;
				if (D.bGradient)
				{
					const double Cdx = D.G[3] - D.G[0], Cdy = D.G[4] - D.G[1], Dr = D.G[5] - D.G[2];
					const double Pdx = Px - D.G[0], Pdy = Py - D.G[1];
					const double Qa = Cdx * Cdx + Cdy * Cdy - Dr * Dr, Qb = Pdx * Cdx + Pdy * Cdy + D.G[2] * Dr, Qc = Pdx * Pdx + Pdy * Pdy - D.G[2] * D.G[2];
					if (std::fabs(Qa) < 1e-12) T = Qc / (2 * Qb);
					else { const double Disc = Qb * Qb - Qa * Qc; if (Disc < 0) continue; T = (Qb + std::sqrt(Disc)) / Qa; if (D.G[2] + T * Dr < 0) T = (Qb - std::sqrt(Disc)) / Qa; }
					T = std::max(0.0, std::min(1.0, T));
				}
				const FStop* Lo = &D.Stops[0]; const FStop* Hi = &D.Stops.back();
				for (size_t K = 1; K < D.Stops.size(); K++) if (T <= D.Stops[K].At) { Lo = &D.Stops[K - 1]; Hi = &D.Stops[K]; break; }
				const double F = Hi->At > Lo->At ? (T - Lo->At) / (Hi->At - Lo->At) : 0;
				const double R = Lo->R + (Hi->R - Lo->R) * F, G = Lo->G + (Hi->G - Lo->G) * F, B = Lo->B + (Hi->B - Lo->B) * F, Al = Lo->A + (Hi->A - Lo->A) * F;
				Blend(X, Y, R, G, B, Al * Cov);
			}
		}

		// ---------- painting (fullPaint, tilePaint, paint) ----------
		void FullPaint()
		{
			CancelPending();
			const int CW = W * P, CH = H * P;
			Raster.assign((size_t)CW * CH * 4, 0);
			Scars.Reset(W, H);
			RasterRect(0, 0, CW, CH);
			Pixels = Raster;
			OverlayLog.clear();
			Overlays(0, 0, W - 1, H - 1, nullptr, OverlayLog);
			for (const FDraw& D : OverlayLog) Draw(D);
			DirtyRects.push_back({ 0, 0, CW, CH });
			LastKind = "full"; LastTiles = 0;
		}
		void TilePaint(const std::vector<int>& Tiles)
		{
			const int CW = W * P, CH = H * P, TW = (W + Tile - 1) / Tile;
			int Rx0 = CW, Ry0 = CH, Rx1 = 0, Ry1 = 0;
			OverlayLog.clear();
			for (const int K : Tiles)
			{
				const int Tx = K % TW, Ty = K / TW, X = Tx * Tile * P, Y = Ty * Tile * P;
				const int TWp = std::min(Tile * P, CW - X), THp = std::min(Tile * P, CH - Y);
				RasterRect(X, Y, TWp, THp);
				for (int J = Y; J < Y + THp; J++) std::memcpy(&Pixels[((size_t)J * CW + X) * 4], &Raster[((size_t)J * CW + X) * 4], (size_t)TWp * 4);
				const double Clip[4] = { (double)X, (double)Y, (double)TWp, (double)THp };
				const size_t First = OverlayLog.size();
				Overlays(Tx * Tile - 2, Ty * Tile - 2, Tx * Tile + Tile + 1, Ty * Tile + Tile + 1, Clip, OverlayLog);
				for (size_t I = First; I < OverlayLog.size(); I++) Draw(OverlayLog[I]);
				Rx0 = std::min(Rx0, X); Ry0 = std::min(Ry0, Y); Rx1 = std::max(Rx1, X + TWp); Ry1 = std::max(Ry1, Y + THp);
			}
			if (Rx0 < Rx1) DirtyRects.push_back({ Rx0, Ry0, Rx1, Ry1 });
			LastKind = "tiles"; LastTiles = (int)Tiles.size();
		}
		// paint(S, grid, cells): the attributes of what changed, then the tiles whose pixels can differ
		void Paint(const std::vector<int>* CellsIn)
		{
			std::vector<int> Cells;
			if (CellsIn) Cells = *CellsIn;
			// World fields follow the ground and biomes discovered so far (the cells whose field flipped repaint too)
			if (bWorld)
			{
				std::vector<uint8_t> Now = FieldsOf();
				if (CellsIn)
				{
					// refreshFields: the blocks of the changed cells in first-seen order, each block's cells row by row
					constexpr int BW = 10, BH = 8;
					std::vector<int> Blocks;
					std::unordered_map<int, bool> SeenBlock;
					for (const int C : *CellsIn) { const int B = (C / W / BH) * W + (C % W) / BW; if (SeenBlock.emplace(B, true).second) Blocks.push_back(B); }
					for (const int B : Blocks)
					{
						const int Bx = B % W, By = B / W;
						const int X0 = Bx * BW + 1, Y0 = By * BH + 1, X1 = std::min(W - 1, Bx * BW + BW - 1), Y1 = std::min(H - 1, By * BH + BH - 1);
						for (int Y = Y0; Y < Y1; Y++) for (int X = X0; X < X1; X++) { const int I = Y * W + X; if (Now[(size_t)I] != Fields[(size_t)I]) Cells.push_back(I); }
					}
				}
				Fields.swap(Now);
			}
			const bool bPrev = Attrs.Valid;
			std::vector<int> Affected;
			struct FBefore { uint8_t Prim, Sec; float Amt, Lev; uint8_t Ov, Scar, Near; float Depth; uint8_t Char; float Tr, Tg, Tb; };
			std::vector<FBefore> Before;
			if (bPrev && CellsIn)
			{
				// a Set in the browser: first-seen order, which is the order the dirty tiles are painted in
				std::vector<uint8_t> Mark((size_t)N, 0);
				for (const int C : Cells)
				{
					const int X = C % W, Y = C / W;
					for (int Ny = std::max(0, Y - 7); Ny <= std::min(H - 1, Y + 7); Ny++) for (int Nx = std::max(0, X - 7); Nx <= std::min(W - 1, X + 7); Nx++)
						if (!Mark[(size_t)(Ny * W + Nx)]) { Mark[(size_t)(Ny * W + Nx)] = 1; Affected.push_back(Ny * W + Nx); }
				}
				for (const int I : Affected)
					Before.push_back({ Attrs.Prim[(size_t)I], Attrs.Sec[(size_t)I], Attrs.Amt[(size_t)I], Attrs.Lev[(size_t)I], Attrs.Ov[(size_t)I], Attrs.Scar[(size_t)I], Attrs.Near[(size_t)I], Attrs.Depth[(size_t)I], Attrs.Char[(size_t)I], Attrs.Tr[(size_t)I], Attrs.Tg[(size_t)I], Attrs.Tb[(size_t)I] });
				CellAttrs(&Affected);
			}
			else CellAttrs(nullptr);
			Scars.Reset(W, H);
			if (!bPrev || !CellsIn) { FullPaint(); return; }
			const int TW = (W + Tile - 1) / Tile, TH = (H + Tile - 1) / Tile, R = 3;
			std::vector<uint8_t> DirtyMark((size_t)TW * TH, 0);
			std::vector<int> DirtyList;
			const FAttrs& A = Attrs;
			for (size_t K = 0; K < Affected.size(); K++)
			{
				const int I = Affected[K];
				const FBefore& O = Before[K];
				if (A.Prim[(size_t)I] == O.Prim && A.Sec[(size_t)I] == O.Sec && A.Amt[(size_t)I] == O.Amt && A.Lev[(size_t)I] == O.Lev && A.Ov[(size_t)I] == O.Ov && A.Scar[(size_t)I] == O.Scar
					&& A.Near[(size_t)I] == O.Near && A.Depth[(size_t)I] == O.Depth && A.Char[(size_t)I] == O.Char && A.Tr[(size_t)I] == O.Tr && A.Tg[(size_t)I] == O.Tg && A.Tb[(size_t)I] == O.Tb) continue;
				const int X = I % W, Y = I / W;
				for (int Ty = std::max(0, Y - R) / Tile; Ty <= std::min(H - 1, Y + R) / Tile; Ty++)
					for (int Tx = std::max(0, X - R) / Tile; Tx <= std::min(W - 1, X + R) / Tile; Tx++)
						if (!DirtyMark[(size_t)(Ty * TW + Tx)]) { DirtyMark[(size_t)(Ty * TW + Tx)] = 1; DirtyList.push_back(Ty * TW + Tx); }
			}
			if (DirtyList.empty()) { LastKind = "clean"; LastTiles = 0; return; }
			if (bDeferred)
			{
				// queuePaint: over half the map changed, so every tile goes in (after the changed ones)
				if ((double)DirtyList.size() > TW * TH * 0.5) for (int T = 0; T < TW * TH; T++) if (!DirtyMark[(size_t)T]) { DirtyMark[(size_t)T] = 1; DirtyList.push_back(T); }
				for (const int T : DirtyList) Queue(T);
				LastKind = "queued"; LastTiles = (int)DirtyList.size();
				return;
			}
			if ((double)DirtyList.size() > TW * TH * 0.5) { FullPaint(); return; }
			TilePaint(DirtyList);
		}

	};

	// ---------- client/rubble-decals.js and crater-decals.js: where each picture lies ----------
	// rubble: one 5 x 5 grid inside its cell (0.06 to 0.94), 2.5 cm over the ground, its atlas quadrant by material and
	// one of four turns; positions are reference metres (x, height, z)
	inline int RubbleQuadrant(const std::string& M)
	{
		return M == "wood" ? 2 : M == "stone" || M == "concrete" || M == "steel" ? 1 : M == "soil" || M == "road" ? 3 : 0;
	}
	inline void RubbleQuad(int Cell, int W, const std::string& Material, const std::function<double(double, double)>& HAt, float Pos[75], float Uv[50])
	{
		const int Cx = Cell % W, Cy = Cell / W, Q = RubbleQuadrant(Material);
		const int Turn = (int)((((uint32_t)(Cell + 1) * 2654435761u) >> 30) & 3);
		for (int Z = 0; Z <= 4; Z++) for (int X = 0; X <= 4; X++)
		{
			const int I = Z * 5 + X;
			const double Fx = X / 4.0, Fz = Z / 4.0;
			const double Px = (Cx + 0.06 + Fx * 0.88) * CellMetres, Pz = (Cy + 0.06 + Fz * 0.88) * CellMetres;
			Pos[I * 3] = F32(Px); Pos[I * 3 + 1] = F32(HAt(Px, Pz) + 0.025); Pos[I * 3 + 2] = F32(Pz);
			double U = Fx, V = 1 - Fz;
			for (int R = 0; R < Turn; R++) { const double Nu = V, Nv = 1 - U; U = Nu; V = Nv; }
			Uv[I * 2] = F32((Q % 2) * 0.5 + 0.001 + U * 0.498);
			Uv[I * 2 + 1] = F32((Q < 2 ? 0.5 : 0) + 0.001 + V * 0.498);
		}
	}
	// crater-decals.js hash
	inline double CraterHash(int C, int Salt)
	{
		uint32_t N = ((uint32_t)C ^ 0x9e3779b9u) * 0x85ebca6bu ^ (uint32_t)Salt * 0xc2b2ae35u;
		N ^= N >> 16; N *= 0x7feb352du; N ^= N >> 15;
		return N / 4294967296.0;
	}
	// a crater beside an earlier one (in scan order) is left out about half the time
	inline bool CraterWanted(int C, int W, int H, const std::function<uint8_t(int)>& CharOf)
	{
		const int X = C % W, Y = C / W;
		auto Crater = [&](int Px, int Py) { return Px >= 0 && Py >= 0 && Px < W && Py < H && CharOf(Py * W + Px) == '+'; };
		if (!Crater(X, Y)) return false;
		return !((Crater(X - 1, Y) || Crater(X, Y - 1)) && CraterHash(C, 3) < 0.5);
	}
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

	// the crater picture: a turned square 1.5 to 2.1 m across each way from a jittered centre, 5 cm over the ground
	inline void CraterQuad(int C, int W, const std::function<double(double, double)>& HAt, float Pos[75])
	{
		const int X = C % W, Y = C / W;
		const double R = 1.5 + 0.6 * CraterHash(C, 1), Turn = CraterHash(C, 2) * 3.141592653589793 * 2, Co = Fdlibm::Cos(Turn), Si = Fdlibm::Sin(Turn);
		const double Cx = (X + 0.5 + (CraterHash(C, 4) - 0.5) * 0.4) * CellMetres, Cz = (Y + 0.5 + (CraterHash(C, 5) - 0.5) * 0.4) * CellMetres;
		for (int J = 0; J < 5; J++) for (int I = 0; I < 5; I++)
		{
			const double U = (I / 4.0 - 0.5) * 2 * R, V = (J / 4.0 - 0.5) * 2 * R, Px = Cx + U * Co - V * Si, Pz = Cz + U * Si + V * Co;
			const int K = (J * 5 + I) * 3;
			Pos[K] = F32(Px); Pos[K + 1] = F32(HAt(Px, Pz) + 0.05); Pos[K + 2] = F32(Pz);
		}
	}
}

#ifndef TC_GROUND_PAINT_CORE_ONLY
#include "CoreMinimal.h"
#include "UObject/StrongObjectPtr.h"

class FJsonObject;
class FJsonValue;
class FTCMatchState;
class AActor;
class UWorld;
class UTexture2D;
class UMaterialInterface;
class UMaterialInstanceDynamic;
class UProceduralMeshComponent;

// The painted ground for the client: feed it next to FTCMatchState (ApplyStart after a start the match state took,
// ApplySnapshot after every snapshot, MarkFog with the fog cells that flipped, Clear when the match is dropped) and
// Tick it every frame of a match after the ground's own Flush.
//
// The material interface the ground mesh (relief) uses, generated by unreal/Tools/terrain/import-ground.py:
//   /Game/Terrain/Materials/MF_TCGroundPaint  material function. Outputs Paint (the painted ground, linear RGB, from
//       the canvas texture at absolute world position / GroundMapSize), FogDim (1 seen, 0.55 explored, 0.32 never
//       seen, bilinear between cell centres, with surfaces.js's drifting mist) and PaintFogged (the two multiplied).
//       Its parameters, set by Bind: GroundPaint (texture), GroundFog (texture), GroundMapSize (map metres, x and y),
//       GroundFogOn, GroundFogTime, GroundMist.
//   /Game/Terrain/Materials/M_TCGroundPaint    lit, specular 0, roughness 1 (a MeshLambertMaterial with the paint as its
//       map, as relief-material.js starts from): base colour = PaintFogged. Usable as it is on the current ground mesh;
//       relief's own material adds its paint on top of MF_TCGroundPaint's outputs.
// Any material instance that uses MF_TCGroundPaint is kept current by Bind (textures swap when the map size changes,
// the mist's clock runs).
class THREECROSSROADS_API FTCGroundPaint
{
public:
	static const TCHAR* const ReferencePin;
	static const TCHAR* const GroundMaterialPath;
	static const TCHAR* const CraterMaterialPath;
	static const TCHAR* const RubbleMaterialPath;

	FTCGroundPaint();
	~FTCGroundPaint();

	// Spawns the actor that holds the decals and loads the materials (once per world). False without a world.
	bool Attach(UWorld* World);
	// Destroys that actor and lets go of every texture and material.
	void Detach();
	// A start: the map and its cells replace everything, the whole ground is painted (client/main.js startGame). False,
	// with nothing changed, when the start's map cannot be read.
	bool ApplyStart(const FJsonObject& M, const FTCMatchState& State);
	// A snapshot's cell rows: their attributes now, their tiles queued (painted by Tick within the budget).
	void ApplySnapshot(const FJsonObject& M);
	// No match: textures and decals are dropped.
	void Clear();
	// Fog flips (FTCMatchState::FogDirtyCells, before it is reset).
	void MarkFog(const TSet<int32>& Cells);
	// Every frame of a match: paints queued tiles (BudgetMs, TilesPerFrame), uploads what changed (all mips), fades
	// the fog toward each cell's look (a quarter second at most), and lays the rubble and crater decals on the ground
	// as drawn. GroundHeight: reference metres (x, z) -> height in metres (ATCTerrain::HeightAt).
	void Tick(const FTCMatchState& State, float DeltaSeconds, bool bEndRevealed, TFunctionRef<float(float, float)> GroundHeight);

	// ---------- the material interface ----------
	UMaterialInterface* GroundMaterial() const;
	// A dynamic instance of M_TCGroundPaint, bound (kept current by Tick).
	UMaterialInstanceDynamic* MakeGroundMaterial(UObject* Outer);
	// Binds any material instance built on MF_TCGroundPaint (kept current by Tick until it is collected).
	void Bind(UMaterialInstanceDynamic* Mid);
	UTexture2D* PaintTexture() const { return PaintTex.Get(); }
	UTexture2D* FogTexture() const { return FogTex.Get(); }
	// Slice 13 lead glue: the six tinted tiles this map is painted from (grass, dirt, mud, field, road, water: the
	// LOOK order) as RGBA bytes with their sizes, for the apron (client/ground.js groundLook().tiles, which apron.js
	// samples). False without a map or without the tile bytes (the apron keeps its flat colours then).
	bool ApronTiles(TArray<TArray<uint8>>& OutRgba, TArray<int32>& OutSizes);

	// ---------- state, for checks and the status line ----------
	bool HasMap() const { return Core.MapW() > 0; }
	int32 PixelsPerCell() const { return Core.PixelsPerCell(); }
	// Round 15: the start painted at Graphics Low's texel count (ground.js gfx.low ? 1280 : 2048).
	bool StartedLow() const { return bStartedLow; }
	int32 TilesQueued() const { return Core.PendingTiles(); }
	bool Textured() const { return Core.Textured(); }
	int32 RubbleDecals() const { return Rubble.Count(); }
	int32 CraterDecals() const { return Craters.Count(); }
	const TCGroundPaintCore::FGround& Painter() const { return Core; }
	FString StatusLine() const;
	// The painted canvas as RGBA bytes (row 0 north), for captures and checks.
	const std::vector<uint8_t>& CanvasPixels() const { return Core.Canvas(); }

	float BudgetMs = 6.f;     // client/main.js createGround: budgetMs 6
	int32 TilesPerFrame = 2;  // tilesPerFrame 2
	bool bMist = true;

private:
	void LoadAssets();
	bool LoadTiles(int32 P, TCGroundPaintCore::FTile Out[TCGroundPaintCore::MatCount - 1]);
	void MakeTextures();
	void UploadDirty();
	void BuildMips(int32 X0, int32 Y0, int32 X1, int32 Y1);
	void FogTick(const FTCMatchState& State, float DeltaSeconds, bool bEndRevealed);
	void DecalTick(TFunctionRef<float(float, float)> GroundHeight);
	void RebindAll();
	static void ReadCellRow(const TArray<TSharedPtr<FJsonValue>>& Row, TCGroundPaintCore::FCellRow& Out, int32 Cells);

	TCGroundPaintCore::FGround Core;
	TCGroundPaintCore::FDecalSet Rubble, Craters;
	TWeakObjectPtr<AActor> Actor;
	TWeakObjectPtr<UProceduralMeshComponent> DecalMesh;
	TStrongObjectPtr<UTexture2D> PaintTex, FogTex;
	TStrongObjectPtr<UMaterialInterface> GroundMat, CraterMat, RubbleMat;
	TStrongObjectPtr<UMaterialInstanceDynamic> CraterMid, RubbleMid;
	TArray<TWeakObjectPtr<UMaterialInstanceDynamic>> Bound;
	bool bAssetsLoaded = false;
	FString TileError;
	// the canvas's mip chain (level 0 is the core's canvas), RGBA, filtered in linear light
	TArray<TArray<uint8>> Mips;
	TArray<FIntPoint> MipSizes;
	// fog: the look each cell is fading toward and where it is now (0..1), and the cells still moving
	TArray<float> FogWant, FogNow;
	TArray<uint8> FogBytes;
	TArray<int32> FogActive;
	TBitArray<> FogBusy;
	TSet<int32> FogPending;
	bool bFogAll = true, bLastReveal = false;
	double Clock = 0;
	// decals: what changed since the last Tick (the cells the browser's update(changed) is given)
	TArray<int32> DecalChanged;
	bool bDecalsAll = true, bDecalMeshDirty = true;
	int32 RubbleSectionVerts = -1, CraterSectionVerts = -1;
	// the start's identity (a reconnect to the same match keeps nothing special here: the ground repaints in full)
	int32 Starts = 0, Snapshots = 0, RowsApplied = 0;
	bool bStartedLow = false;
	double LastPaintMs = 0, LastUploadMs = 0;
};
#endif // TC_GROUND_PAINT_CORE_ONLY
