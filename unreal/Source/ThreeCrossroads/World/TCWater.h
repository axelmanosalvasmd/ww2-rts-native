// Water: one see-through mesh over river (W), ford (F) and bridge (=) cells, glossy, with a slow drift; water that
// reaches the map edge carries on past it. A port of the pinned reference (ReferencePin), line for line:
//   client/water-levels.js  WET, WATER_LIFT, createWaterLevels (levels frozen once, read: bodies by four-neighbour
//                           connectivity, each body's line at its lowest non-bridge cell, banks at the lowest line among
//                           their eight neighbours, raised spans);
//   client/water.js         blurWindow, bfs, noiseTexture (the 128 x 128 ripple table), the mask (4 texels a cell: R
//                           coverage, G depth guess, B fords, A bridges; tiles of 16 cells redone within the blur's
//                           reach), the flow per vertex, the corner grid with its drape over raised spans, the strips and
//                           corner blocks that carry water 900 m past the map edge, rebuild's change rules, changed()
//                           and tick();
//   client/main.js          waterOptions (World Conquest: water runs on under ground this side has not discovered).
// The shader (FRAG_COLOR, FRAG_ROUGH, FRAG_NORMAL, FRAG_SHEEN) is unreal/Tools/water/make-water-materials.py's
// M_TCWater.
//
// The engine-free part (namespace TCWaterCore) compiles alone with TC_WATER_CORE_ONLY defined, so the standalone
// harness (unreal/Tools/water/harness) holds it to the Node oracle that runs the pinned water.js on all 33 maps and a
// World Conquest start (unreal/Tools/water/oracle.mjs). Units are the reference's: metres, y up, x east, z south.
//
// Recipient knowledge: the mesh reads only the cells this client was told (the caller's Char returns '.' for a World
// cell this side has not discovered, as the start map does) and never an unknown cell's level. Where known water meets
// undiscovered ground the shore is not drawn (the reference's OPEN rule): that uses the known water only. Levels
// freeze when the mesh is created, including the public zero placeholder for undiscovered World cells. Discovering
// a later cell never moves that frozen line, as in createWaterLevels. Hidden levels are never read.
#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <functional>
#include <limits>
#include <map>
#include <set>
#include <string>
#include <vector>

namespace TCWaterCore
{
	// water.js tick: Low keeps the last value; High wraps the presentation clock every 1000 seconds.
	// A finite nonnegative override is used only by matched renderer captures.
	inline float PresentationTime(double Now, float Previous, bool Low, double Fixed = -1.0)
	{
		if (std::isfinite(Fixed) && Fixed >= 0.0) return (float)std::fmod(Fixed, 1000.0);
		return Low ? Previous : (float)std::fmod(Now, 1000.0);
	}
	inline constexpr double Cell = 2.0;          // shared/sim.js CELL
	inline constexpr double LevelMetres = 2.5;   // CFG.levelHeight
	inline constexpr double Lift = 0.06;         // WATER_LIFT: the surface sits this far above a flat bank
	inline constexpr int Sub = 4;                // mask texels per cell
	inline constexpr double EdgeReach = 900.0;   // metres water carries on past the map edge
	inline constexpr int Tile = 16, Reach = 3;   // mask tiles (cells) and the widest blur's reach in cells
	inline constexpr int OpenReach = 2;          // undiscovered cells beyond known water that still count as water
	inline constexpr int NoiseSize = 128;

	// three r186 common/lights_physical_pars_fragment and shadowmap_pars_fragment.
	// The material uses these same equations in TCWater.Shader.inl. Values use linear light.
	inline double Fresnel(double DotVH)
	{
		const double F = std::exp2((-5.55473 * DotVH - 6.98316) * DotVH);
		return 0.04 * (1.0 - F) + F;
	}
	inline double GgxVisibility(double Alpha, double NL, double NV)
	{
		const double A2 = Alpha * Alpha;
		const double GV = NL * std::sqrt(A2 + (1.0 - A2) * NV * NV);
		const double GL = NV * std::sqrt(A2 + (1.0 - A2) * NL * NL);
		return 0.5 / std::max(GV + GL, 1e-6);
	}
	inline double GgxDistribution(double Alpha, double NH)
	{
		const double A2 = Alpha * Alpha, D = NH * NH * (A2 - 1.0) + 1.0;
		return A2 / (3.14159265358979323846 * D * D);
	}
	inline double DirectBrdf(double Colour, double Roughness, double NL, double NV, double NH, double VH, double DfgEnergy)
	{
		const double F = Fresnel(VH), Alpha = Roughness * Roughness;
		const double Compensation = 1.0 + 0.04 * (1.0 / DfgEnergy - 1.0);
		return Colour * (1.0 - F) / 3.14159265358979323846
			+ F * GgxVisibility(Alpha, NL, NV) * GgxDistribution(Alpha, NH) * Compensation;
	}
	inline void VogelOffset(int Sample, double Phi, double& X, double& Y)
	{
		const double R = std::sqrt((Sample + 0.5) / 5.0), A = Sample * 2.399963229728653 + Phi;
		X = std::cos(A) * R; Y = std::sin(A) * R;
	}
	inline double ShadowRotation(double PixelX, double PixelY)
	{
		auto Fract = [](double V) { return V - std::floor(V); };
		return Fract(52.9829189 * Fract(PixelX * .06711056 + PixelY * .00583715)) * 6.28318530717958647692;
	}
	// Hardware bilinear depth comparison, emulated because the native atlas exposes a point sampler.
	inline double BilinearShadow(double A, double B, double C, double D, double X, double Y)
	{
		return (A + (B - A) * X) * (1 - Y) + (C + (D - C) * X) * Y;
	}
	// ForwardShadowingCommon.ush compares 1-Z < sampledDepth-bias.x. The
	// comparison filter takes the biased receiver depth on its left-hand side.
	inline double ShadowReceiverDepth(double ShadowZ, double Bias)
	{
		return 1.0 - ShadowZ + Bias;
	}
	inline double ShadowAtlasRadius(double SourceRadius, double SourceTexel, int Resolution, double CascadeRadius)
	{
		return CascadeRadius > 0 ? SourceRadius * (SourceTexel * Resolution / (2 * CascadeRadius)) : SourceRadius;
	}

	// The native post pass sees the opaque bed depth. Move only the water term to its
	// own fog depth before that common linear-light blend. The bed term is unchanged.
	inline double HazeCompensated(double Colour, double Haze, double WaterFog, double BedFog)
	{
		return BedFog < .9999 ? (Colour * (1 - WaterFog) + Haze * (WaterFog - BedFog)) / (1 - BedFog) : Colour;
	}

	inline bool IsWet(uint8_t Ch) { return Ch == 'W' || Ch == 'F' || Ch == '='; }
	inline double SmoothStep(double X, double Min, double Max) // three MathUtils.smoothstep
	{
		if (X <= Min) return 0.0;
		if (X >= Max) return 1.0;
		X = (X - Min) / (Max - Min);
		return X * X * (3.0 - 2.0 * X);
	}
	inline const int N4[4][2] = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } };

	// water-levels.js read(grid): what one look at the cells gives
	struct FRead
	{
		std::vector<uint8_t> Wet, Bridges, Raised;
		std::vector<int32_t> Comp;
		std::vector<double> CompLevel;  // metres
		std::vector<float> Wl;          // metres, NaN away from water (Float32Array)
	};

	// water-levels.js createWaterLevels + read. Levels are the frozen levels; Char(c) the cell's terrain character.
	template <typename CharFn>
	FRead ReadLevels(int W, int H, const std::vector<int8_t>& Levels, CharFn Char)
	{
		const int N = W * H;
		FRead R;
		R.Wet.assign(N, 0); R.Bridges.assign(N, 0); R.Comp.assign(N, -1); R.Raised.assign(N, 0);
		for (int C = 0; C < N; C++) { const uint8_t Ch = Char(C); R.Wet[C] = IsWet(Ch) ? 1 : 0; R.Bridges[C] = Ch == '=' ? 1 : 0; }
		std::vector<int32_t> Queue(N);
		const double Inf = std::numeric_limits<double>::infinity();
		for (int C = 0; C < N; C++)
		{
			if (!R.Wet[C] || R.Comp[C] >= 0) continue;
			const int Id = (int)R.CompLevel.size();
			int Head = 0, Tail = 1;
			double Water = Inf, Any = Inf;
			Queue[0] = C; R.Comp[C] = Id;
			while (Head < Tail)
			{
				const int K = Queue[Head++], X = K % W, Y = K / W;
				Any = std::min(Any, (double)Levels[K]);
				if (!R.Bridges[K]) Water = std::min(Water, (double)Levels[K]);
				for (int Side = 0; Side < 4; Side++)
				{
					if ((Side == 0 && X == 0) || (Side == 1 && X == W - 1) || (Side == 2 && Y == 0) || (Side == 3 && Y == H - 1)) continue;
					const int Next = K + (Side == 0 ? -1 : Side == 1 ? 1 : Side == 2 ? -W : W);
					if (!R.Wet[Next] || R.Comp[Next] >= 0) continue;
					R.Comp[Next] = Id; Queue[Tail++] = Next;
				}
			}
			R.CompLevel.push_back((Water == Inf ? Any : Water) * LevelMetres);
		}
		R.Wl.assign(N, std::numeric_limits<float>::quiet_NaN());
		for (int C = 0; C < N; C++)
		{
			if (R.Wet[C])
			{
				R.Wl[C] = (float)R.CompLevel[R.Comp[C]];
				R.Raised[C] = Levels[C] * LevelMetres > (double)R.Wl[C] ? 1 : 0;
				continue;
			}
			const int X = C % W, Y = C / W;
			for (int DY = -1; DY <= 1; DY++) for (int DX = -1; DX <= 1; DX++)
			{
				const int NX = X + DX, NY = Y + DY, K = NY * W + NX;
				// !(wl[c] <= level): true while wl[c] is NaN
				if (NX >= 0 && NY >= 0 && NX < W && NY < H && R.Wet[K] && !((double)R.Wl[C] <= R.CompLevel[R.Comp[K]])) R.Wl[C] = (float)R.CompLevel[R.Comp[K]];
			}
		}
		return R;
	}

	// Separable box blur of radius r run 3 times over a window of a w x h image of 0s and 1s, edges repeated: the
	// window's values times (2r + 1)^6 (water.js blurWindow).
	struct FBlur { std::vector<int32_t> A; int Wx0 = 0, Wy0 = 0, Ww = 0; };
	template <typename PickFn>
	FBlur BlurWindow(PickFn Pick, int W, int H, int R, int X0, int Y0, int X1, int Y1)
	{
		const int Wx0 = std::max(0, X0 - 3 * R), Wy0 = std::max(0, Y0 - 3 * R), Wx1 = std::min(W, X1 + 3 * R), Wy1 = std::min(H, Y1 + 3 * R);
		const int Ww = Wx1 - Wx0, Wh = Wy1 - Wy0;
		FBlur Out;
		std::vector<int32_t>& A = Out.A;
		A.assign((size_t)Ww * Wh, 0);
		std::vector<int32_t> B((size_t)Ww * Wh, 0);
		for (int Y = 0; Y < Wh; Y++) for (int X = 0; X < Ww; X++) A[(size_t)Y * Ww + X] = Pick(Wx0 + X, Wy0 + Y) ? 1 : 0;
		for (int Pass = 0; Pass < 3; Pass++)
		{
			for (int Y = 0; Y < Wh; Y++)
			{
				const size_t O = (size_t)Y * Ww;
				int32_t S = 0;
				for (int K = -R; K <= R; K++) S += A[O + std::min(Ww - 1, std::max(0, K))];
				for (int X = 0; X < Ww; X++)
				{
					B[O + X] = S;
					S += A[O + std::min(Ww - 1, X + R + 1)] - A[O + std::max(0, X - R)];
				}
			}
			for (int X = 0; X < Ww; X++)
			{
				int32_t S = 0;
				for (int K = -R; K <= R; K++) S += B[(size_t)std::min(Wh - 1, std::max(0, K)) * Ww + X];
				for (int Y = 0; Y < Wh; Y++)
				{
					A[(size_t)Y * Ww + X] = S;
					S += B[(size_t)std::min(Wh - 1, Y + R + 1) * Ww + X] - B[(size_t)std::max(0, Y - R) * Ww + X];
				}
			}
		}
		Out.Wx0 = Wx0; Out.Wy0 = Wy0; Out.Ww = Ww;
		return Out;
	}

	// Breadth-first step counts from the seed cells through cells that allow(c); -1 where it never reached.
	template <typename AllowFn>
	std::vector<int32_t> Bfs(int W, int H, const std::vector<int32_t>& Seeds, AllowFn Allow)
	{
		std::vector<int32_t> D((size_t)W * H, -1), Q((size_t)W * H);
		int Head = 0, Tail = 0;
		for (const int C : Seeds) { D[C] = 0; Q[Tail++] = C; }
		while (Head < Tail)
		{
			const int C = Q[Head++], X = C % W, Y = (C - X) / W;
			for (const auto& Dd : N4)
			{
				const int NX = X + Dd[0], NY = Y + Dd[1], Nn = NY * W + NX;
				if (NX < 0 || NY < 0 || NX >= W || NY >= H || D[Nn] >= 0 || !Allow(Nn)) continue;
				D[Nn] = D[C] + 1; Q[Tail++] = Nn;
			}
		}
		return D;
	}

	// water.js noiseTexture: tileable ripples made of whole-number waves. R height, G/B slope (x, z), A slower blotches.
	inline std::vector<uint8_t> NoiseTexture()
	{
		const int N = NoiseSize;
		std::vector<uint8_t> Data((size_t)N * N * 4, 0);
		double Seed = 9173;
		auto Rand = [&Seed]() { Seed = std::fmod(Seed * 16807.0, 2147483647.0); return Seed / 2147483647.0; };
		const double Pi = 3.14159265358979323846;
		struct FWave { double Kx, Kz, A, P; };
		auto Waves = [&](int Count, double KMin, double KMax)
		{
			std::vector<FWave> Out;
			while ((int)Out.size() < Count)
			{
				// JS Math.round: halves go up
				const double Kx = std::floor((Rand() * 2 - 1) * KMax + 0.5), Kz = std::floor((Rand() * 2 - 1) * KMax + 0.5), K = std::hypot(Kx, Kz);
				if (K >= KMin && K <= KMax) Out.push_back({ Kx, Kz, 1 / K, Rand() * Pi * 2 });
			}
			return Out;
		};
		const std::vector<FWave> Ripple = Waves(16, 2, 9), Blot = Waves(8, 1, 4);
		std::vector<float> Hh((size_t)N * N, 0.f), GX((size_t)N * N, 0.f), GZ((size_t)N * N, 0.f), Bb((size_t)N * N, 0.f);
		for (int Z = 0; Z < N; Z++) for (int X = 0; X < N; X++)
		{
			const size_t I = (size_t)Z * N + X;
			for (const FWave& Wv : Ripple)
			{
				const double T = (Pi * 2 * (Wv.Kx * X + Wv.Kz * Z)) / N + Wv.P;
				Hh[I] = (float)((double)Hh[I] + Wv.A * std::sin(T));
				GX[I] = (float)((double)GX[I] + Wv.A * Wv.Kx * std::cos(T));
				GZ[I] = (float)((double)GZ[I] + Wv.A * Wv.Kz * std::cos(T));
			}
			for (const FWave& Wv : Blot) Bb[I] = (float)((double)Bb[I] + Wv.A * std::sin((Pi * 2 * (Wv.Kx * X + Wv.Kz * Z)) / N + Wv.P));
		}
		auto Range = [](const std::vector<float>& Arr, double& Lo, double& Hi)
		{
			Lo = std::numeric_limits<double>::infinity(); Hi = -Lo;
			for (const float V : Arr) { Lo = std::min(Lo, (double)V); Hi = std::max(Hi, (double)V); }
		};
		double H0, H1, B0, B1;
		Range(Hh, H0, H1); Range(Bb, B0, B1);
		double G = 0;
		for (size_t I = 0; I < (size_t)N * N; I++) G = std::max(G, std::max(std::fabs((double)GX[I]), std::fabs((double)GZ[I])));
		auto Byte = [](double V) { return (uint8_t)(int)V; }; // Uint8Array stores truncate (values stay in 0..255)
		for (size_t I = 0; I < (size_t)N * N; I++)
		{
			Data[I * 4] = Byte(((Hh[I] - H0) / (H1 - H0)) * 255);
			Data[I * 4 + 1] = Byte((0.5 + (0.5 * GX[I]) / G) * 255);
			Data[I * 4 + 2] = Byte((0.5 + (0.5 * GZ[I]) / G) * 255);
			Data[I * 4 + 3] = Byte(((Bb[I] - B0) / (B1 - B0)) * 255);
		}
		return Data;
	}

	// One body of water state (water.js createWater's closure). Feed it with Create, then Changed() for each cell row
	// and Tick() once a frame; Geometry() and MaskDirty() say what to send to the GPU.
	class FWater
	{
	public:
		// Char(c): the cell's terrain character as this client knows it ('.' for an undiscovered World cell).
		std::function<uint8_t(int)> Char;
		// Unknown(c): a World cell this side has not discovered; empty on authored maps (water.js `unknown`).
		std::function<bool(int)> Unknown;
		// GroundHeight(x, z): the drawn ground in metres (main.js hAt, client/relief.js); empty: the corner average.
		std::function<double(double, double)> GroundHeight;
		// GroundLevel(c): the cell's current level (groundAt / 2.5), used only without GroundHeight.
		std::function<int(int)> GroundLevel;

		int W = 0, H = 0;
		std::vector<int8_t> Frozen;     // water-levels.js levels: read once

		// geometry, reference metres (x, y up, z): (W + 1)(H + 1) corners, then the strips past the edge
		std::vector<float> Pos, Normal, Flow;
		std::vector<uint32_t> Index;
		std::vector<float> Wl;            // the last read's water lines (NaN away from water), for WaterAt
		std::vector<uint8_t> Mask;        // (W * 4) x (H * 4) RGBA
		std::set<int> MaskTiles;          // tiles redone since the caller last took them (TakeMaskTiles)
		bool GeometryChanged = false;     // since the caller last looked (TakeGeometry)
		int Rebuilds = 0, GeometryBuilds = 0;

		// water.js createWater: false (and nothing kept) when no cell is wet, as it returns null. Levels(c) is the
		// level frozen for each cell this client knows (Known(c)); unknown cells freeze at the public placeholder 0.
		template <typename LevelFn, typename KnownFn>
		bool Create(int InW, int InH, LevelFn Level, KnownFn Known)
		{
			W = InW; H = InH;
			const int N = W * H;
			bool Any = false;
			for (int C = 0; C < N && !Any; C++) Any = IsWet(Char(C));
			if (!Any) { W = H = 0; return false; }
			Frozen.assign(N, 0);
			for (int C = 0; C < N; C++) if (Known(C)) Frozen[C] = (int8_t)Level(C);
			MWs = W * Sub; MHs = H * Sub; Tw = (W + Tile - 1) / Tile;
			Mask.assign((size_t)MWs * MHs * 4, 0);
			WetA.assign(N, 0); BridgesA.assign(N, 0); FordsA.assign(N, 0); OpenA.assign(N, 0); RaisedA.assign(N, 0);
			Dirty = Regeo = false;
			Rebuild();
			return true;
		}
		bool Valid() const { return W > 0 && H > 0; }
		// water.js changed(): one cell row [cell, char, level?] already written into the grid
		void Changed(int C, uint8_t Ch, bool bHasLevel)
		{
			if (C < 0 || C >= W * H) return;
			if (IsWet(Ch) || WetA[C] || OpenA[C]) Dirty = true;
			// a dent on or beside a span that lies on the ground moves that ground
			if (bHasLevel && Near(RaisedA, C)) Dirty = Regeo = true;
		}
		// water.js tick(): rebuild when something changed. True if it rebuilt.
		bool Tick() { if (!Dirty) return false; Rebuild(); return true; }

		// client/relief.js waterAt: the water line at (x, z) (NaN away from water)
		double WaterAt(double X, double Z) const
		{
			if (!Valid() || Wl.empty()) return std::numeric_limits<double>::quiet_NaN();
			const int C = std::min(H - 1, std::max(0, (int)std::floor(Z / Cell))) * W + std::min(W - 1, std::max(0, (int)std::floor(X / Cell)));
			return (double)Wl[C] + Lift;
		}
		int MaskW() const { return MWs; }
		int MaskH() const { return MHs; }
		int TilesWide() const { return Tw; }
		// a tile's texel rectangle in the mask
		void TileRect(int T, int& X0, int& Y0, int& X1, int& Y1) const
		{
			X0 = (T % Tw) * Tile * Sub; Y0 = (T / Tw) * Tile * Sub; X1 = std::min(MWs, X0 + Tile * Sub); Y1 = std::min(MHs, Y0 + Tile * Sub);
		}

	private:
		int MWs = 0, MHs = 0, Tw = 0;
		std::vector<uint8_t> WetA, BridgesA, FordsA, OpenA, RaisedA;
		bool Dirty = false, Regeo = false;

		bool Near(const std::vector<uint8_t>& Arr, int C) const
		{
			const int X = C % W, Y = (C - X) / W;
			for (int DY = -1; DY <= 1; DY++) for (int DX = -1; DX <= 1; DX++)
			{
				const int NX = X + DX, NY = Y + DY;
				if (NX >= 0 && NY >= 0 && NX < W && NY < H && Arr[NY * W + NX]) return true;
			}
			return false;
		}
		double GroundAt(int C) const { return (GroundLevel ? GroundLevel(C) : 0) * LevelMetres; }

		std::set<int> TilesNear(const std::vector<int>& Cells) const
		{
			std::set<int> Tiles;
			for (const int C : Cells)
			{
				const int X = C % W, Y = (C - X) / W;
				for (int TY = std::max(0, Y - Reach) / Tile; TY <= std::min(H - 1, Y + Reach) / Tile; TY++)
					for (int TX = std::max(0, X - Reach) / Tile; TX <= std::min(W - 1, X + Reach) / Tile; TX++) Tiles.insert(TY * Tw + TX);
			}
			return Tiles;
		}
		// one mask channel in the given tiles: cells where Pick(c), softened by a blur of radius r texels
		void Channel(int K, const std::vector<uint8_t>& Pick, int R, const std::set<int>& Tiles)
		{
			const double Scale = std::pow((double)(2 * R + 1), 6.0);
			auto CellPick = [&](int X, int Y) { return Pick[(size_t)(Y / Sub) * W + X / Sub] != 0; };
			for (const int T : Tiles)
			{
				int X0, Y0, X1, Y1;
				TileRect(T, X0, Y0, X1, Y1);
				const FBlur B = BlurWindow(CellPick, MWs, MHs, R, X0, Y0, X1, Y1);
				for (int Y = Y0; Y < Y1; Y++) for (int X = X0; X < X1; X++)
				{
					const double V = std::min(255.0, (double)B.A[(size_t)(Y - B.Wy0) * B.Ww + X - B.Wx0] / Scale * 255 + 0.5);
					Mask[((size_t)Y * MWs + X) * 4 + K] = (uint8_t)(int)V;
				}
			}
		}

		void Rebuild()
		{
			Dirty = false;
			Rebuilds++;
			const int N = W * H;
			std::vector<uint8_t> NextWet(N), NextBridges(N), NextFords(N);
			std::vector<int> ChangedCells;
			bool bWetChanged = false, bBridgesChanged = false;
			for (int C = 0; C < N; C++)
			{
				const uint8_t Ch = Char(C);
				NextWet[C] = IsWet(Ch) ? 1 : 0; NextBridges[C] = Ch == '=' ? 1 : 0; NextFords[C] = Ch == 'F' ? 1 : 0;
				if (NextWet[C] != WetA[C]) bWetChanged = true;
				if (NextBridges[C] != BridgesA[C]) bBridgesChanged = true;
			}
			// the shore follows open water: known water and the undiscovered cells next to it
			std::vector<uint8_t> NextOpen = NextWet;
			if (Unknown) for (int C = 0; C < N; C++)
			{
				if (!NextWet[C]) continue;
				const int X = C % W, Y = (C - X) / W;
				for (int DY = -OpenReach; DY <= OpenReach; DY++) for (int DX = -OpenReach; DX <= OpenReach; DX++)
				{
					const int NX = X + DX, NY = Y + DY, K = NY * W + NX;
					if (NX >= 0 && NY >= 0 && NX < W && NY < H && !NextWet[K] && Unknown(K)) NextOpen[K] = 1;
				}
			}
			for (int C = 0; C < N; C++) if (NextWet[C] != WetA[C] || NextBridges[C] != BridgesA[C] || NextFords[C] != FordsA[C] || NextOpen[C] != OpenA[C]) ChangedCells.push_back(C);
			const bool bRelevel = Regeo;
			Regeo = false;
			if (ChangedCells.empty() && !bRelevel) return;
			WetA = NextWet; BridgesA = NextBridges; FordsA = NextFords; OpenA = NextOpen;
			if (!ChangedCells.empty())
			{
				const std::set<int> Tiles = TilesNear(ChangedCells);
				Channel(0, OpenA, 2, Tiles);    // R: coverage, edge at about 0.3 (a third of a cell into the bank)
				Channel(1, OpenA, 4, Tiles);    // G: how far from the bank, read as depth
				Channel(2, FordsA, 2, Tiles);   // B: fords
				Channel(3, BridgesA, 2, Tiles); // A: bridges
				MaskTiles.insert(Tiles.begin(), Tiles.end());
			}
			if (bWetChanged || bBridgesChanged || bRelevel) RebuildGeometry();
		}

		void RebuildGeometry()
		{
			GeometryBuilds++;
			GeometryChanged = true;
			const int N = W * H;
			const FRead Water = ReadLevels(W, H, Frozen, Char);
			const std::vector<int32_t>& Comp = Water.Comp;
			const std::vector<double>& CompLevel = Water.CompLevel;
			Wl = Water.Wl;
			RaisedA = Water.Raised;

			// flow: toward one end of each body (the end on the map edge, if only one is), slower far from the banks
			std::vector<int32_t> Dry;
			for (int C = 0; C < N; C++) if (!WetA[C]) Dry.push_back(C);
			const std::vector<int32_t> Shore = Bfs(W, H, Dry, [this](int K) { return WetA[K] != 0; });
			std::vector<int32_t> Dist(N, -1);
			auto OnEdge = [this, N](int C) { return C % W == 0 || C % W == W - 1 || C < W || C >= N - W; };
			std::vector<std::vector<int32_t>> Members(CompLevel.size());
			for (int C = 0; C < N; C++) if (Comp[C] >= 0) Members[Comp[C]].push_back(C);
			std::vector<int32_t> Steps(N, -1), Queue(N);
			auto ReachFrom = [&](int Seed, int Id, const std::vector<int32_t>& List)
			{
				for (const int K : List) Steps[K] = -1;
				int Head = 0, Tail = 0;
				Steps[Seed] = 0; Queue[Tail++] = Seed;
				while (Head < Tail)
				{
					const int C = Queue[Head++], X = C % W, Y = (C - X) / W;
					for (const auto& Dd : N4)
					{
						const int NX = X + Dd[0], NY = Y + Dd[1], K = NY * W + NX;
						if (NX < 0 || NY < 0 || NX >= W || NY >= H || Steps[K] >= 0 || Comp[K] != Id) continue;
						Steps[K] = Steps[C] + 1; Queue[Tail++] = K;
					}
				}
			};
			for (int Id = 0; Id < (int)CompLevel.size(); Id++)
			{
				const std::vector<int32_t>& List = Members[Id];
				if (List.empty()) continue;
				const int Start = List[0];
				auto Far = [&]() { int Best = Start; for (const int K : List) if (Steps[K] > Steps[Best]) Best = K; return Best; };
				ReachFrom(Start, Id, List);
				const int A = Far();
				ReachFrom(A, Id, List);
				const int B = Far();
				if (OnEdge(B) && !OnEdge(A)) ReachFrom(B, Id, List);
				for (const int K : List) if (Steps[K] >= 0) Dist[K] = Steps[K];
			}
			std::vector<float> Fx(N, 0.f), Fz(N, 0.f);
			for (int C = 0; C < N; C++)
			{
				if (!WetA[C]) continue;
				const int X = C % W, Y = (C - X) / W;
				auto At = [&](int NX, int NY) { const int K = NY * W + NX; return NX >= 0 && NY >= 0 && NX < W && NY < H && Comp[K] == Comp[C] ? Dist[K] : Dist[C]; };
				const double Gx = At(X + 1, Y) - At(X - 1, Y), Gz = At(X, Y + 1) - At(X, Y - 1), Len = std::hypot(Gx, Gz);
				const double S = Shore[C] < 0 ? 0.25 : 1 - 0.75 * SmoothStep(Shore[C], 2, 8);
				if (Len > 0) { Fx[C] = (float)((-Gx / Len) * S); Fz[C] = (float)((-Gz / Len) * S); }
			}
			for (int Pass = 0; Pass < 3; Pass++)
			{
				std::vector<float> Nx2(N, 0.f), Nz2(N, 0.f);
				for (int C = 0; C < N; C++)
				{
					if (!WetA[C]) continue;
					const int X = C % W, Y = (C - X) / W;
					double Sx = 0, Sz = 0;
					int M = 0;
					for (int DY = -1; DY <= 1; DY++) for (int DX = -1; DX <= 1; DX++)
					{
						const int AX = X + DX, AY = Y + DY, K = AY * W + AX;
						if (AX >= 0 && AY >= 0 && AX < W && AY < H && Comp[K] == Comp[C]) { Sx += Fx[K]; Sz += Fz[K]; M++; }
					}
					Nx2[C] = (float)(Sx / M); Nz2[C] = (float)(Sz / M);
				}
				Fx.swap(Nx2); Fz.swap(Nz2);
			}

			const int W1 = W + 1, Verts = W1 * (H + 1);
			Pos.assign((size_t)Verts * 3, 0.f); Flow.assign((size_t)Verts * 2, 0.f);
			auto Used = [&](int CX, int CY) { return CX >= 0 && CY >= 0 && CX < W && CY < H && !std::isnan(Wl[CY * W + CX]); };
			const double Inf = std::numeric_limits<double>::infinity();
			for (int VY = 0; VY <= H; VY++) for (int VX = 0; VX <= W; VX++)
			{
				const size_t V = (size_t)VY * W1 + VX;
				if (!Used(VX - 1, VY - 1) && !Used(VX, VY - 1) && !Used(VX - 1, VY) && !Used(VX, VY))
				{
					Pos[V * 3] = (float)(VX * Cell); Pos[V * 3 + 1] = (float)Lift; Pos[V * 3 + 2] = (float)(VY * Cell);
					continue;
				}
				double Y = Inf, Sx = 0, Sz = 0, Ground = 0;
				int M = 0, Cells = 0;
				bool bDrape = false;
				for (int Corner = 0; Corner < 4; Corner++)
				{
					const int CX = VX - 1 + (Corner & 1), CY = VY - 1 + (Corner >> 1);
					if (CX < 0 || CY < 0 || CX >= W || CY >= H) continue;
					const int C = CY * W + CX;
					if ((double)Wl[C] < Y) Y = Wl[C];
					if (WetA[C]) { Sx += Fx[C]; Sz += Fz[C]; M++; }
					if (!GroundHeight) Ground += GroundAt(C);
					Cells++;
					bDrape = bDrape || RaisedA[C];
				}
				// the relief supplies the visible surface; the nominal corner average remains the fallback
				if (bDrape) Y = std::max(Y, GroundHeight ? GroundHeight(VX * Cell, VY * Cell) : Ground / Cells);
				Pos[V * 3] = (float)(VX * Cell); Pos[V * 3 + 1] = (float)((Y == Inf ? 0 : Y) + Lift); Pos[V * 3 + 2] = (float)(VY * Cell);
				if (M) { Flow[V * 2] = (float)(Sx / M); Flow[V * 2 + 1] = (float)(Sz / M); }
			}
			Index.clear();
			for (int Y = 0; Y < H; Y++) for (int X = 0; X < W; X++)
			{
				if (std::isnan(Wl[Y * W + X])) continue;
				const uint32_t A = (uint32_t)(Y * W1 + X), B = A + 1, C = A + W1, D = C + 1;
				Index.insert(Index.end(), { A, C, B, B, C, D });
			}

			// Water that reaches the map edge keeps going: a strip out from each edge cell that carries water, and a
			// block past each corner. The mask repeats its edge texels outward, so the shore and flow carry on.
			std::vector<double> ExtraPos, ExtraFlow;
			std::map<std::string, uint32_t> Outer;
			auto VertexAt = [&](double X, double Y, double Z, double FX, double FZ)
			{
				ExtraPos.insert(ExtraPos.end(), { X, Y, Z }); ExtraFlow.insert(ExtraFlow.end(), { FX, FZ });
				return (uint32_t)(Verts + ExtraPos.size() / 3 - 1);
			};
			auto OuterOf = [&](uint32_t V, int OX, int OZ)
			{
				const std::string Key = std::to_string(V) + ":" + std::to_string(OX) + ":" + std::to_string(OZ);
				auto It = Outer.find(Key);
				if (It != Outer.end()) return It->second;
				const uint32_t Made = VertexAt((double)Pos[V * 3] + OX * EdgeReach, Pos[V * 3 + 1], (double)Pos[V * 3 + 2] + OZ * EdgeReach, Flow[V * 2], Flow[V * 2 + 1]);
				Outer.emplace(Key, Made);
				return Made;
			};
			auto XZ = [&](uint32_t P, double& X, double& Z)
			{
				if (P < (uint32_t)Verts) { X = Pos[(size_t)P * 3]; Z = Pos[(size_t)P * 3 + 2]; }
				else { X = ExtraPos[(size_t)(P - Verts) * 3]; Z = ExtraPos[(size_t)(P - Verts) * 3 + 2]; }
			};
			// four vertices around a quad, in order; the first triangle must face up
			auto Quad = [&](uint32_t P, uint32_t Q, uint32_t R, uint32_t S)
			{
				double Px, Pz, Qx, Qz, Rx, Rz;
				XZ(P, Px, Pz); XZ(Q, Qx, Qz); XZ(R, Rx, Rz);
				const bool bFlip = (Qz - Pz) * (Rx - Px) - (Qx - Px) * (Rz - Pz) < 0;
				if (bFlip) Index.insert(Index.end(), { P, R, Q, P, S, R });
				else Index.insert(Index.end(), { P, Q, R, P, R, S });
			};
			auto EdgeCell = [&](int X, int Y) { return !std::isnan(Wl[Y * W + X]); };
			for (int X = 0; X < W; X++)
			{
				// arguments in JavaScript's order: the outer vertices are made left to right
				if (EdgeCell(X, 0)) { const uint32_t P = X, Q = X + 1, R = OuterOf(X + 1, 0, -1), S = OuterOf(X, 0, -1); Quad(P, Q, R, S); }
				const uint32_t Sv = (uint32_t)(H * W1 + X);
				if (EdgeCell(X, H - 1)) { const uint32_t R = OuterOf(Sv + 1, 0, 1), S = OuterOf(Sv, 0, 1); Quad(Sv, Sv + 1, R, S); }
			}
			for (int Y = 0; Y < H; Y++)
			{
				const uint32_t V = (uint32_t)(Y * W1);
				if (EdgeCell(0, Y)) { const uint32_t R = OuterOf(V + W1, -1, 0), S = OuterOf(V, -1, 0); Quad(V, V + W1, R, S); }
				if (EdgeCell(W - 1, Y)) { const uint32_t R = OuterOf(V + W + W1, 1, 0), S = OuterOf(V + W, 1, 0); Quad(V + W, V + W + W1, R, S); }
			}
			const int Corners[4][4] = { { 0, 0, -1, -1 }, { W - 1, 0, 1, -1 }, { W - 1, H - 1, 1, 1 }, { 0, H - 1, -1, 1 } };
			for (const auto& Cn : Corners)
			{
				const int OX = Cn[2], OZ = Cn[3];
				if (!EdgeCell(Cn[0], Cn[1])) continue;
				const uint32_t V = (uint32_t)((OZ < 0 ? 0 : H) * W1 + (OX < 0 ? 0 : W));
				const double X = Pos[(size_t)V * 3], Z = Pos[(size_t)V * 3 + 2], Yv = Pos[(size_t)V * 3 + 1], F0 = Flow[(size_t)V * 2], F1 = Flow[(size_t)V * 2 + 1];
				const uint32_t Q = VertexAt(X + OX * EdgeReach, Yv, Z, F0, F1);
				const uint32_t R = VertexAt(X + OX * EdgeReach, Yv, Z + OZ * EdgeReach, F0, F1);
				const uint32_t S = VertexAt(X, Yv, Z + OZ * EdgeReach, F0, F1);
				Quad(V, Q, R, S);
			}
			for (const double V : ExtraPos) Pos.push_back((float)V);
			for (const double V : ExtraFlow) Flow.push_back((float)V);
			Normal.assign(Pos.size(), 0.f);
			for (size_t I = 1; I < Normal.size(); I += 3) Normal[I] = 1.f;
		}
	};
}

#ifndef TC_WATER_CORE_ONLY

#include "CoreMinimal.h"
#include "UObject/StrongObjectPtr.h"
#include "Client/TCGraphics.h"

class AActor;
class UWorld;
class UProceduralMeshComponent;
class UMaterialInterface;
class UMaterialInstanceDynamic;
class ATCLighting;
class UTexture2D;
class FTCMatchState;

// One texel per cell of the fog of war, as ATCTerrain dims the ground: 1 in sight, 0.55 explored, 0.32 never seen
// (stored / 255), bilinear, so water and apron fade the way the ground does. Shared by FTCWater and FTCApron.
class THREECROSSROADS_API FTCCellFog
{
public:
	// out of line: the held texture's type is complete only in TCWater.cpp
	FTCCellFog();
	~FTCCellFog();
	FTCCellFog(const FTCCellFog&) = delete;
	FTCCellFog& operator=(const FTCCellFog&) = delete;
	void Reset(int32 W, int32 H);
	// recomputes the given cells (all when Cells is null); true if a texel changed
	bool Update(const FTCMatchState& State, bool bRevealAll, const TSet<int32>* Cells);
	UTexture2D* Texture() const { return Tex.Get(); }
	int32 Value(int32 C) const { return Data.IsValidIndex(C) ? Data[C] : 0; }
	static uint8 Level(const FTCMatchState& State, bool bRevealAll, int32 C);

private:
	TStrongObjectPtr<UTexture2D> Tex;
	TArray<uint8> Data;
	int32 W = 0, H = 0;
	void Upload();
};

// The water as this client draws it (client/water.js through main.js): built at a start, changed by the cells the
// snapshots bring, drawn as one translucent procedural mesh with the M_TCWater material (/Game/Water). Feed it next to
// ATCTerrain: ResetFrom after the terrain's ResetFrom, MarkCells with the same cells, Flush after the terrain's Flush.
class THREECROSSROADS_API FTCWater
{
public:
	static const TCHAR* const ReferencePin;
	static const TCHAR* const MaterialPath;

	// out of line: the held objects' types are complete only in TCWater.cpp
	FTCWater();
	~FTCWater();
	FTCWater(const FTCWater&) = delete;
	FTCWater& operator=(const FTCWater&) = delete;

	// Spawns the actor that holds the mesh (once per world). Detach drops it and everything held.
	void Attach(UWorld* World);
	void Detach();
	bool IsAttached() const { return Actor.IsValid(); }

	// The ground the water drapes its raised spans on (main.js hAt): reference metres in, metres out. Wire it to the
	// drawn terrain (ATCTerrain::HeightAt); unset, the corner average of the cell levels is used, as water.js does.
	TFunction<float(float X, float Z)> GroundHeight;
	// The fog of war's multiplier (1, 0.55, 0.32) is raised to this before it dims the colour: 1 multiplies linear
	// colour (FTCWorldObjectsView's FogMap); 2.2 matches a ground that scales sRGB bytes (the placeholder ATCTerrain).
	float FogGamma = 1.f;

	// A start: everything is rebuilt from the map this client was sent (main.js startGame: water?.dispose(); createWater).
	void ResetFrom(const FTCMatchState& State);
	// Cells whose terrain, level or knowledge changed (the terrain's DirtyCells) and cells whose fog changed.
	void MarkCells(const FTCMatchState& State, const TSet<int32>& Shaped, const TSet<int32>& Fogged);
	// Once a frame: rebuilds what changed (water.tick), sends the mask tiles and the mesh, the fog and the time.
	// bRevealAll: the authored match is decided and the fog is lifted (ATCTerrain::IsEndRevealed).
	void Flush(const FTCMatchState& State, double NowSeconds, bool bRevealAll);

	// client/relief.js waterAt: the water line at reference metres, NaN away from water (boats ride on it)
	float WaterAt(float X, float Z) const;
	bool HasWater() const { return Core.Valid(); }
	int32 TrianglesDrawn() const { return Triangles; }
	int32 GeometryBuilds() const { return Core.GeometryBuilds; }
	const TCWaterCore::FWater& CoreState() const { return Core; }
	UProceduralMeshComponent* MeshComponent() const { return Mesh.Get(); }
	UMaterialInstanceDynamic* MaterialInstance() const { return MID.Get(); }
	// Round 15 (Graphics Low): the level the material shows (water.js WATER_LOW), the clock it last sent (it stands
	// still on Low), and how many changes of level it applied in the middle of a match.
	bool DrawnLow() const { return bDrawnLow; }
	float TimeSent() const { return LastTime; }
	int32 LevelSwitches() const { return LowSwitches; }
	// Comparison-only clock, matching a browser water material held at the same uTime. No simulation state changes.
	// Negative clears the override. Normal play keeps water.js's running High / held Low clock.
	void SetPresentationTime(double Seconds) { FixedTime = Seconds; }

private:
	TCWaterCore::FWater Core;
	FTCGraphicsWatch GfxWatch;
	bool bDrawnLow = false;
	float LastTime = 0.f;
	double FixedTime = -1.0;
	int32 LowSwitches = 0;
	TArray<uint8> Chars;            // what the core reads: known cells' characters, '.' elsewhere
	TArray<int8> Levels;            // current levels of known cells
	TBitArray<> Known;
	int32 W = 0, H = 0, Triangles = 0;
	bool bWorld = false, bFogAll = true;
	TSet<int32> FogCells;
	FTCCellFog Fog;
	bool bLastReveal = false;

	TWeakObjectPtr<AActor> Actor;
	TWeakObjectPtr<ATCLighting> Lighting;
	TStrongObjectPtr<UProceduralMeshComponent> Mesh;
	TStrongObjectPtr<UMaterialInterface> Material;
	TStrongObjectPtr<UMaterialInstanceDynamic> MID;
	TStrongObjectPtr<UTexture2D> MaskTex, NoiseTex;

	void CopyCells(const FTCMatchState& State, const TSet<int32>* Cells);
	void TryCreate(const FTCMatchState& State);
	void SendGeometry();
	void SendMask(bool bAll);
	void BindMaterial();
};

#endif // TC_WATER_CORE_ONLY
