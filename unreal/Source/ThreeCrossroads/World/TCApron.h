// The land past the map edge: an apron of ground around the playable area, so the camera never sees a void or a hard
// edge, wherever it points and however far out it zooms. A port of the pinned reference (ReferencePin), line for line:
//   client/apron.js   CLASS, ROWS and the other constants, the rolling value noise and its amplitude, the ring of
//                     columns (a twin pair at every cell boundary, a column every half metre, one per corner on the
//                     diagonal), shape() (edge profile, running sums, settling hills, low water, rolling land, the
//                     skirt and the strip tucked under the map's edge, the normals), the index order (ground rows, the
//                     strip, the skirt) and update() (reshape when an edge height or wet edge cell changed);
//   client/ground.js  what the apron reads of the map's paint: buildNoise (the 512 x 512 fBm table and its equalized
//                     copy, the apron's noise texture), LOOK's metres and flat colours (the tiles before textures load),
//                     BIOME_LOOKS' soil, bare and fields, the field blocks (fieldBlock) and cellAttrs' main material,
//                     secondary material and amount per cell (the apron's cell texture);
//   shared/sim.js     startState (a cell's state before the server mentions it), for the wear and shelling bits.
// The paint (FRAG_COLOR) and the fog over the apron (fogOverlayShader: the map's edge cells carry on outward, then
// unseen) are unreal/Tools/water/make-water-materials.py's M_TCApron.
//
// The engine-free part (namespace TCApronCore) compiles alone with TC_APRON_CORE_ONLY defined, so the standalone
// harness (unreal/Tools/water/harness) holds it to the Node oracle that runs the pinned apron.js and ground.js.
//
// The land past the edge for a player and a spectator (round 14, read from the pinned code and checked in the live
// browser): the apron is the same ground for everyone; only its fog overlay differs. fogOverlayShader samples the
// map's fog texture at the clamped edge (so the edge cells' look carries outward) and eases it to unseen (alpha 185)
// between 6 and 32 m past the edge. A player sees the edge in its own fog; a spectator of an authored map has no fog
// mask (fog.js: lifted, alpha 0), so the land is lit at the edge and darkens over those 26 m; on World the edge keeps
// the frontier's darkness. The flat grey band from the edge that the reference's browser spectator showed in slice 13
// is not that design: lobby-view.js rebuilds its backdrop when a lobby message arrives during a match (a spectator
// joining always gets one after its start, and so does everyone in the room), calls setFogMap(null) and so leaves
// every fog consumer the 1 x 1 empty fog with a size of 1 m: the overlay is then at unseen from the first metre past
// the edge (and the same rebuild empties the match's map pieces, structures.js's shared state). The apron here keeps
// the design; the side by sides avoid that rebuild in the browser (round 14 edge REPORT.md).
//
// Recipient knowledge: the apron reads the drawn ground's height along the map edge (the caller's height function,
// which never samples an undiscovered World cell) and the edge cells' characters and levels as this client knows them
// (an undiscovered cell reads as open ground at level 0, which is what the start map says of it, so nothing is
// revealed). The fog of war darkens it like the map's own ground.
#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <functional>
#include <vector>

namespace TCApronCore
{
	inline constexpr double Cell = 2.0;
	// ground.js material id -> apron class: 0 grass, 1 dirt, 2 mud, 3 field, 4 road, 5 water, 6 field turned 90 degrees
	inline constexpr int Class[10] = { 0, 1, 2, 3, 4, 5, 1, 2, 2, 6 };
	inline constexpr int RowCount = 20;
	inline constexpr double Rows[RowCount] = { 0, 0.5, 1, 2, 3.5, 6, 10, 16, 25, 38, 55, 80, 115, 165, 235, 330, 460, 640, 900, 1250 };
	inline constexpr double Column = 0.5, Twin = 1e-3, Skirt = 6, Lip = 0.3, LipDrop = 0.3, Settle = 48, Window = 24, CloudReach = 700;
	inline constexpr double Unseen = 185.0 / 255.0;

	inline double Smooth(double A, double B, double X) { const double T = std::min(1.0, std::max(0.0, (X - A) / (B - A))); return T * T * (3 - 2 * T); }
	inline double JsRound(double V) { return std::floor(V + 0.5); }

	// ---------- gentle rolling land (value noise, a fixed pattern) ----------
	inline double Hash(double X, double Z) { const double V = std::sin(X * 127.1 + Z * 311.7) * 43758.5453; return V - std::floor(V); }
	inline double ValueNoise(double X, double Z)
	{
		const double IX = std::floor(X), IZ = std::floor(Z), FX = X - IX, FZ = Z - IZ, SX = FX * FX * (3 - 2 * FX), SZ = FZ * FZ * (3 - 2 * FZ);
		const double A = Hash(IX, IZ), B = Hash(IX + 1, IZ), C = Hash(IX, IZ + 1), D = Hash(IX + 1, IZ + 1);
		return (A + (B - A) * SX) * (1 - SZ) + (C + (D - C) * SX) * SZ;
	}
	inline double Rolling(double X, double Z)
	{
		return 0.55 * (ValueNoise(X / 150, Z / 150) * 2 - 1) + 0.3 * (ValueNoise(X / 58 + 7.3, Z / 58 + 1.9) * 2 - 1) + 0.15 * (ValueNoise(X / 23 + 3.1, Z / 23 + 9.7) * 2 - 1);
	}
	// metres: nearly flat at the edge, up to about 4.5 m far out
	inline double Amplitude(double D) { return 0.22 * Smooth(0, 22, D) + 4.4 * Smooth(12, 330, D); }

	struct FCol { double X, Z, Nx, Nz, Len; };

	// One apron (apron.js createApron's closure, without the paint and the fog). Build once per map size, Shape with
	// the edge heights; positions are reference metres (x, y up, z), N columns x RowsPer rows.
	class FApron
	{
	public:
		int MapW = 0, MapH = 0, N = 0;
		static constexpr int K = RowCount, R = RowCount + 2; // row K of each column is the skirt, row K + 1 the strip under the map's edge
		double Wm = 0, Hm = 0;
		std::vector<FCol> Cols;
		std::vector<float> Position, Normal, Edge, Wet;
		std::vector<uint32_t> Index;
		size_t CloudCount = 0; // indices of the rows within CloudReach (the cloud shade's overlay)

		void Build(int InW, int InH)
		{
			MapW = InW; MapH = InH; Wm = InW * Cell; Hm = InH * Cell;
			Cols.clear();
			auto Side = [&](double X0, double Z0, double DX, double DZ, double Length, double NX, double NZ)
			{
				const int Cells = (int)JsRound(Length / Cell), Steps = (int)JsRound(Cell / Column);
				for (int Kc = 0; Kc < Cells; Kc++) for (int I = 0; I <= Steps; I++)
				{
					const double At = Kc * Cell + (I == 0 ? Twin : I == Steps ? Cell - Twin : I * Column);
					Cols.push_back({ X0 + DX * At, Z0 + DZ * At, NX, NZ, 1.0 });
				}
			};
			const double Sqrt2 = std::sqrt(2.0);
			auto Corner = [&](double X, double Z, double NX, double NZ) { Cols.push_back({ X, Z, NX, NZ, Sqrt2 }); };
			Corner(0, 0, -1, -1);
			Side(0, 0, 1, 0, Wm, 0, -1); Corner(Wm, 0, 1, -1);
			Side(Wm, 0, 0, 1, Hm, 1, 0); Corner(Wm, Hm, 1, 1);
			Side(Wm, Hm, -1, 0, Wm, 0, 1); Corner(0, Hm, -1, 1);
			Side(0, Hm, 0, -1, Hm, -1, 0);
			N = (int)Cols.size();
			Position.assign((size_t)N * R * 3, 0.f); Normal.assign((size_t)N * R * 3, 0.f);
			Edge.assign(N, 0.f); Wet.assign(N, 0.f);
			// triangles: the ground rows first (the cloud shade uses the first rows only), then the strip under the
			// map's edge (it covers any crack between the two meshes seen from the map side), then the skirt
			int CloudRows = K - 1;
			for (int I = 0; I < K; I++) if (Rows[I] > CloudReach) { CloudRows = I; break; }
			Index.clear();
			Index.reserve(((size_t)N * (K - 1) + 2 * (size_t)N) * 6);
			CloudCount = 0;
			for (int Kr = 0; Kr < K - 1; Kr++)
			{
				if (Kr == CloudRows) CloudCount = Index.size();
				for (int C = 0; C < N; C++)
				{
					const uint32_t A = C * R + Kr, B = ((C + 1) % N) * R + Kr, A2 = A + 1, B2 = B + 1;
					Index.insert(Index.end(), { A, B, A2, B, B2, A2 });
				}
			}
			if (!CloudCount) CloudCount = Index.size();
			for (int C = 0; C < N; C++)
			{
				const uint32_t A = C * R + K + 1, B = ((C + 1) % N) * R + K + 1, A2 = C * R, B2 = ((C + 1) % N) * R;
				Index.insert(Index.end(), { A, B, A2, B, B2, A2 });
			}
			for (int C = 0; C < N; C++)
			{
				const uint32_t A = C * R, B = ((C + 1) % N) * R, S = A + K, S2 = B + K; // the wall faces outward
				Index.insert(Index.end(), { S, A, S2, S2, A, B });
			}
		}

		// the map cell a column belongs to (apron.js wetRow)
		int ColumnCell(int C) const
		{
			const int X = std::min(MapW - 1, std::max(0, (int)std::floor(Cols[C].X / Cell))), Z = std::min(MapH - 1, std::max(0, (int)std::floor(Cols[C].Z / Cell)));
			return Z * MapW + X;
		}
		double ClampX(double X) const { return std::min(Wm, std::max(0.0, X)); }
		double ClampZ(double Z) const { return std::min(Hm, std::max(0.0, Z)); }

		// apron.js update(): true when an edge height or a wet edge cell differs from what the apron was shaped with
		template <typename HeightFn, typename WetFn>
		bool Differs(HeightFn HAt, WetFn WetCell) const
		{
			for (int C = 0; C < N; C++)
			{
				const double Hh = HAt(ClampX(Cols[C].X), ClampZ(Cols[C].Z));
				if (std::fabs(Hh - Edge[C]) > 1e-4 || (WetCell(ColumnCell(C)) ? 1.f : 0.f) != Wet[C]) return true;
			}
			return false;
		}

		// apron.js shape(): heights from the edge profile outward. HAt(x, z): the drawn ground in metres; WetCell(c):
		// the cell's character is W, F or =.
		template <typename HeightFn, typename WetFn>
		void Shape(HeightFn HAt, WetFn WetCell)
		{
			for (int C = 0; C < N; C++) { Edge[C] = (float)HAt(ClampX(Cols[C].X), ClampZ(Cols[C].Z)); Wet[C] = WetCell(ColumnCell(C)) ? 1.f : 0.f; }
			std::vector<double> EdgeSum((size_t)3 * N + 1, 0.0), WetSum((size_t)3 * N + 1, 0.0);
			for (int I = 0; I < 3 * N; I++) { EdgeSum[I + 1] = EdgeSum[I] + Edge[I % N]; WetSum[I + 1] = WetSum[I] + Wet[I % N]; }
			auto Mean = [&](const std::vector<double>& Sum, int C, int Rr) { return (Sum[C + Rr + N + 1] - Sum[C - Rr + N]) / (2 * Rr + 1); };
			for (int C = 0; C < N; C++)
			{
				const FCol& Col = Cols[C];
				for (int Kr = 0; Kr < K; Kr++)
				{
					const double D = Rows[Kr], Euclid = D * Col.Len;
					const int Rr = (int)std::min<double>(N - 1, std::min(JsRound(Window / 0.4), JsRound(Euclid * 1.2 / 0.4)));
					const double E = Rr ? Mean(EdgeSum, C, Rr) : Edge[C], Wv = Rr ? Mean(WetSum, C, Rr) : Wet[C], T = Smooth(0, Settle, Euclid);
					const double X = Col.X + Col.Nx * D, Z = Col.Z + Col.Nz * D;
					const double Hh = (E > 0 ? E * (1 - T) : E * (1 - T * (1 - Wv))) + Amplitude(Euclid) * (1 - Wv) * Rolling(X, Z);
					Set((size_t)C * R + Kr, X, Hh, Z);
				}
				const size_t Row0 = (size_t)C * R * 3;
				const double InX = ClampX(Col.X - Col.Nx * Lip), InZ = ClampZ(Col.Z - Col.Nz * Lip);
				Set((size_t)C * R + K, Col.X, (double)Position[Row0 + 1] - Skirt, Col.Z);
				Set((size_t)C * R + K + 1, InX, HAt(InX, InZ) - LipDrop, InZ);
			}
			// normals from the neighbours in the grid: along the ring and outward (one-sided at the rim)
			auto P = [&](int C, int Kr, int O) { return (double)Position[(((size_t)((C + N) % N)) * R + Kr) * 3 + O]; };
			for (int C = 0; C < N; C++) for (int Kr = 0; Kr < K; Kr++)
			{
				const int K0 = std::max(0, Kr - 1), K1 = std::min(K - 1, Kr + 1);
				const double Ux = P(C + 1, Kr, 0) - P(C - 1, Kr, 0), Uy = P(C + 1, Kr, 1) - P(C - 1, Kr, 1), Uz = P(C + 1, Kr, 2) - P(C - 1, Kr, 2);
				const double Vx = P(C, K1, 0) - P(C, K0, 0), Vy = P(C, K1, 1) - P(C, K0, 1), Vz = P(C, K1, 2) - P(C, K0, 2);
				double Nx = Uy * Vz - Uz * Vy, Ny = Uz * Vx - Ux * Vz, Nz = Ux * Vy - Uy * Vx;
				if (Ny < 0) { Nx = -Nx; Ny = -Ny; Nz = -Nz; }
				const double Len = std::hypot(Nx, Ny, Nz), Inv = 1 / (Len != 0 ? Len : 1);
				const size_t O = ((size_t)C * R + Kr) * 3;
				Normal[O] = (float)(Nx * Inv); Normal[O + 1] = (float)(Ny * Inv); Normal[O + 2] = (float)(Nz * Inv);
			}
			for (int C = 0; C < N; C++) for (const int Row : { K, K + 1 }) for (int O = 0; O < 3; O++) Normal[((size_t)C * R + Row) * 3 + O] = Normal[(size_t)C * R * 3 + O]; // lit like the edge
		}

	private:
		void Set(size_t V, double X, double Y, double Z) { Position[V * 3] = (float)X; Position[V * 3 + 1] = (float)Y; Position[V * 3 + 2] = (float)Z; }
	};

	// ---------- ground.js: the noise tables ----------
	inline constexpr int NoiseSize = 512;
	struct FGroundNoise { std::vector<float> NA, NP; };
	// ground.js buildNoise: one periodic 512 x 512 fBm table (NA in [-1, 1]) and an equalized copy (NP uniform in [0, 1])
	inline FGroundNoise BuildNoise()
	{
		const int NS = NoiseSize;
		FGroundNoise Out;
		std::vector<float>& NA = Out.NA;
		NA.assign((size_t)NS * NS, 0.f);
		double Seed = 1234567;
		// (seed * 1103515245 + 12345) >>> 0, in doubles as JavaScript does it (the product is rounded first)
		auto Rand = [&Seed]() { const double V = Seed * 1103515245.0 + 12345.0; Seed = std::fmod(V, 4294967296.0); return Seed / 4294967296.0; };
		double Amp = 1;
		for (int O = 0; O < 6; O++, Amp *= 0.55)
		{
			const int G = 4 << O;
			const double Step = (double)NS / G;
			std::vector<float> Lat((size_t)G * G);
			for (float& L : Lat) L = (float)(Rand() * 2 - 1);
			for (int Y = 0; Y < NS; Y++)
			{
				const double Fy = Y / Step, Yi = std::floor(Fy), Ty = Fy - Yi, Sy = Ty * Ty * (3 - 2 * Ty);
				const int R0 = (int)Yi * G, R1 = (((int)Yi + 1) % G) * G;
				for (int X = 0; X < NS; X++)
				{
					const double Fx = X / Step, Xi = std::floor(Fx), Tx = Fx - Xi, Sx = Tx * Tx * (3 - 2 * Tx);
					const int XI = (int)Xi, X1 = (XI + 1) % G;
					const double Top = Lat[R0 + XI] + ((double)Lat[R0 + X1] - Lat[R0 + XI]) * Sx, Bot = Lat[R1 + XI] + ((double)Lat[R1 + X1] - Lat[R1 + XI]) * Sx;
					float& Slot = NA[(size_t)Y * NS + X];
					Slot = (float)((double)Slot + Amp * (Top + (Bot - Top) * Sy));
				}
			}
		}
		double Mx = 0;
		for (const float V : NA) Mx = std::max(Mx, std::fabs((double)V));
		for (float& V : NA) V = (float)((double)V / Mx);
		const int Bins = 2048;
		std::vector<float> Hist(Bins, 0.f);
		auto Bin = [&](float V) { return std::min(Bins - 1, (int)(((double)V + 1) / 2 * Bins)); };
		for (const float V : NA) Hist[Bin(V)] += 1.f;
		for (int I = 1; I < Bins; I++) Hist[I] = (float)((double)Hist[I] + Hist[I - 1]);
		Out.NP.assign(NA.size(), 0.f);
		for (size_t I = 0; I < NA.size(); I++) Out.NP[I] = (float)((double)Hist[Bin(NA[I])] / (double)NA.size());
		return Out;
	}
	// apron.js noiseTex: R the equalized patch noise, G the fBm (0..1), B 0, A 255
	inline std::vector<uint8_t> NoiseTexture(const FGroundNoise& Noise)
	{
		const size_t Nn = Noise.NA.size();
		std::vector<uint8_t> Data(Nn * 4, 0);
		for (size_t I = 0; I < Nn; I++)
		{
			Data[I * 4] = (uint8_t)(int)((double)Noise.NP[I] * 255);
			Data[I * 4 + 1] = (uint8_t)(int)(((double)Noise.NA[I] * 0.5 + 0.5) * 255);
			Data[I * 4 + 3] = 255;
		}
		return Data;
	}

	// ---------- ground.js: per-cell ground materials ----------
	enum EMat { Grass = 0, Dirt = 1, Mud = 2, Field = 3, Road = 4, WaterMat = 5, Rubble = 6, Shell = 7, Earth = 8, FieldV = 9 };
	// LOOK: metres per repeat and the flat colour (texture average times mul, then saturation) the tiles show before the
	// textures load. Index 6 of the apron's tiles is FIELD_V, the field again.
	inline constexpr double LookMetres[6] = { 9, 8, 7, 10, 8, 12 };
	inline void FlatColor(int M, double Out[3])
	{
		static const double Avg[6][3] = { { 107, 103, 44 }, { 160, 117, 72 }, { 87, 64, 43 }, { 103, 70, 44 }, { 162, 136, 109 }, { 57, 83, 82 } };
		static const double Mul[6][3] = { { 1.02, 1.06, 1.35 }, { 0.95, 1, 1.04 }, { 1, 1, 1 }, { 0.95, 0.95, 1 }, { 0.9, 0.9, 0.88 }, { 0.95, 1, 1.08 } };
		static const double Sat[6] = { 0.78, 0.5, 0.72, 0.5, 0.62, 1 };
		const double Rr = Avg[M][0] * Mul[M][0], Gg = Avg[M][1] * Mul[M][1], Bb = Avg[M][2] * Mul[M][2], Y = 0.3 * Rr + 0.59 * Gg + 0.11 * Bb;
		Out[0] = Y + (Rr - Y) * Sat[M]; Out[1] = Y + (Gg - Y) * Sat[M]; Out[2] = Y + (Bb - Y) * Sat[M];
	}
	// BIOME_LOOKS (shared/world-layers.js BIOMES order): soil, bare, fields
	struct FBiomeLook { int Soil; double Bare, Fields; };
	inline const FBiomeLook BiomeLooks[6] = { { Dirt, 0.08, 0.5 }, { Mud, 0.12, 0.03 }, { Mud, 0.22, 0.02 }, { Dirt, 0.24, 0.05 }, { Dirt, 0.0, 0.18 }, { Dirt, 0.2, 0.28 } };

	inline double Rnd(double X, double Y, double Kk = 0) { const double V = std::sin(X * 127.1 + Y * 311.7 + Kk * 74.7) * 43758.5453; return V - std::floor(V); }
	// shared/sim.js cellNoise, startWear, packState and startState
	inline double CellNoise(int C)
	{
		uint32_t H = (uint32_t)(C + 1) * 0x9e3779b1u;
		H ^= H >> 15;
		H *= 0x85ebca6bu;
		return (double)(H ^ (H >> 13)) / 4294967296.0;
	}
	inline uint8_t StartState(uint8_t Ch, int C)
	{
		const double Wear = Ch == 'M' || Ch == 'F' ? 0.2 + 0.6 * CellNoise(C) : Ch == '+' ? 0.3 + 0.3 * CellNoise(C) : 0;
		return (uint8_t)std::min(3, (int)std::floor(Wear * 4));
	}

	// What the classification reads of a cell. Grid: terrain.grid (an own mine reads 'N'); Physical: with a mine's ground
	// and object; Orig: map.rows (the start map's rows; World: as discovered); Biome: -1 for none.
	struct FCellSource
	{
		int W = 0, H = 0;
		std::function<uint8_t(int)> Grid, Physical, Orig;
		std::function<int(int)> Level, State, Biome;
		std::vector<std::pair<int, int>> Spawns, Points; // cells
	};

	// ground.js fieldsOf: per cell 0, FIELD or FIELD_V
	inline std::vector<uint8_t> Fields(const FCellSource& S)
	{
		const int W = S.W, H = S.H, BW = 10, BH = 8;
		std::vector<uint8_t> F((size_t)W * H, 0);
		struct FKeep { int X, Y, R; };
		std::vector<FKeep> Keep;
		for (const auto& P : S.Spawns) Keep.push_back({ P.first, P.second, 9 });
		for (const auto& P : S.Points) Keep.push_back({ P.first, P.second, 5 });
		for (int By = 0; By * BH < H; By++) for (int Bx = 0; Bx * BW < W; Bx++)
		{
			const int X0 = Bx * BW + 1, Y0 = By * BH + 1, X1 = std::min(W - 1, Bx * BW + BW - 1), Y1 = std::min(H - 1, By * BH + BH - 1);
			const int Bi = S.Biome ? S.Biome(std::min(H - 1, By * BH + 4) * W + std::min(W - 1, Bx * BW + 5)) : -1;
			const double Share = Bi >= 0 && Bi < 6 ? BiomeLooks[Bi].Fields : 0.24;
			if (Rnd(Bx, By, 7) > Share || X1 - X0 < 4 || Y1 - Y0 < 3) continue;
			bool bKept = false;
			for (const FKeep& P : Keep) if (P.X > X0 - P.R && P.X < X1 + P.R - 1 && P.Y > Y0 - P.R && P.Y < Y1 + P.R - 1) { bKept = true; break; }
			if (bKept) continue;
			bool bOpen = true;
			for (int Y = Y0; Y < Y1 && bOpen; Y++) for (int X = X0; X < X1; X++) if (S.Orig(Y * W + X) != '.') { bOpen = false; break; }
			if (!bOpen) continue;
			const uint8_t Dir = Rnd(Bx, By, 8) < 0.5 ? Field : FieldV;
			for (int Y = Y0; Y < Y1; Y++) for (int X = X0; X < X1; X++) F[(size_t)Y * W + X] = Dir;
		}
		return F;
	}

	struct FCellMat { uint8_t Prim = Grass, Sec = Dirt; float Amt = 0.f; };
	// ground.js cellAttrs: a cell's main material, secondary material and amount (the part the apron reads)
	inline FCellMat CellMaterial(const FCellSource& S, const std::vector<uint8_t>& FieldsOf, const FGroundNoise& Noise, int I)
	{
		const int W = S.W, H = S.H, X = I % W, Y = I / W;
		const uint8_t Ch = S.Physical(I);
		const int L = S.Level(I), St = S.State(I), Worn = St & 3;
		auto At = [&](int AX, int AY) -> uint8_t { return AX >= 0 && AY >= 0 && AX < W && AY < H ? S.Grid(AY * W + AX) : 0; };
		auto NAt = [&](double Xx, double Yy) { return (double)Noise.NA[(size_t)((((int)Yy) & 511) << 9 | (((int)Xx) & 511))]; };
		int P = Grass, Sc = Dirt;
		double A = 0;
		if (Ch == '.')
		{
			bool bWet = false, bTown = false;
			for (int DY = -1; DY <= 1; DY++) for (int DX = -1; DX <= 1; DX++)
			{
				const uint8_t C = At(X + DX, Y + DY);
				if (C == 'W' || C == 'F' || C == '=') bWet = true;
				const int OX = X + DX, OY = Y + DY;
				if (OX >= 0 && OY >= 0 && OX < W && OY < H && S.Orig(OY * W + OX) == 'B') bTown = true;
			}
			const int Bi = S.Biome ? S.Biome(I) : -1;
			const FBiomeLook* Look = Bi >= 0 && Bi < 6 ? &BiomeLooks[Bi] : nullptr;
			const uint8_t Fld = FieldsOf[I];
			if (Fld) { P = Fld; Sc = Grass; A = 0.3; }
			else if (bWet) { Sc = Mud; A = 0.55; }
			else if (bTown) { P = Road; Sc = Grass; A = 0.3; }
			else if (L < 0) { Sc = Mud; A = std::min(0.6, 0.25 + 0.12 * -L); }
			else { A = 0.1 + 0.3 * (NAt(X * 3 + 41, Y * 3 + 77) * 0.5 + 0.5); if (Look) { Sc = Look->Soil; A += Look->Bare; } }
			if (Worn) { Sc = Mud; A = std::max(A, 0.24 * Worn); }
			const int Shelled = St >> 5 & 3;
			if (Shelled && !Fld) { Sc = Shell; A = std::max(A, 0.22 + 0.16 * Shelled); }
			else if (Shelled) A = std::max(A, 0.3 + 0.15 * Shelled);
		}
		else if (Ch == 'B' || Ch == 'K') { P = Dirt; Sc = Road; A = 0.3; }
		else if (Ch == 'R') { P = Dirt; Sc = Grass; A = 0.35; }
		else if (Ch == '+') { P = Grass; Sc = Worn ? Mud : Dirt; A = 0.16 + 0.12 * Worn; }
		else if (Ch == 'T') { P = Earth; Sc = Mud; A = 0.3; }
		else if (Ch == 'W' || Ch == '=') P = WaterMat;
		else if (Ch == 'F') { P = WaterMat; Sc = Road; A = 0.6 - 0.13 * Worn; }
		else if (Ch == 'H') { Sc = Mud; A = 0.35; }
		else if (Ch == 'X') { Sc = Mud; A = 0.45; }
		else if (Ch == 'Y' || Ch == 'Q') A = 0.4;
		else if (Ch == '#') A = 0.5;
		else if (Ch == 'D') { P = Road; Sc = Worn ? Shell : Dirt; A = 0.12 + 0.2 * Worn; }
		else if (Ch == 'M') { P = Mud; A = 0.42 - 0.12 * Worn; }
		else if (Ch == 'N') A = 0.3;
		else if (Ch == 'O') { Sc = Mud; A = 0.5; }
		else if (Ch == 'A') { P = Dirt; A = 0.2; }
		if (!A) Sc = P;
		FCellMat M;
		M.Prim = (uint8_t)P; M.Sec = (uint8_t)Sc; M.Amt = (float)A;
		return M;
	}
	// apron.js readCells: one texel per cell, R the main class, G the secondary class, B the amount, A 255
	inline void CellTexel(const FCellMat& M, uint8_t Out[4])
	{
		Out[0] = (uint8_t)Class[M.Prim]; Out[1] = (uint8_t)Class[M.Sec];
		Out[2] = (uint8_t)std::min(255.0, JsRound((double)M.Amt * 255)); Out[3] = 255;
	}
}

#ifndef TC_APRON_CORE_ONLY

#include "CoreMinimal.h"
#include "UObject/StrongObjectPtr.h"
#include "World/TCWater.h" // FTCCellFog
#include "World/TCFogFrontier.h"

class AActor;
class UWorld;
class UProceduralMeshComponent;
class UMaterialInterface;
class UMaterialInstanceDynamic;
class UTexture;
class UTexture2D;
class FTCMatchState;
class FTCWorldObjects;
class FTCFogFrontier;

// The apron as this client draws it (client/apron.js through main.js): one procedural mesh around the map with the
// M_TCApron material (/Game/Water), its own fog of war texture and cell texture. Feed it next to ATCTerrain:
// ResetFrom after the terrain's ResetFrom, MarkCells with the same cells, Flush after the terrain's Flush.
class THREECROSSROADS_API FTCApron
{
public:
	static const TCHAR* const ReferencePin;
	static const TCHAR* const MaterialPath;

	// out of line: the held objects' types are complete only in TCApron.cpp
	FTCApron();
	~FTCApron();
	FTCApron(const FTCApron&) = delete;
	FTCApron& operator=(const FTCApron&) = delete;

	void Attach(UWorld* World);
	void Detach();
	bool IsAttached() const { return Actor.IsValid(); }

	// The drawn ground (relief.hAt): reference metres in, metres out. Wire it to ATCTerrain::HeightAt; unset, the cell
	// levels times 2.5 m are used (a flat step per cell).
	TFunction<float(float X, float Z)> GroundHeight;
	// The fog of war's multiplier (1, 0.55, 0.32) is raised to this before it dims the colour: 1 multiplies linear
	// colour (FTCWorldObjectsView's FogMap); 2.2 matches a ground that scales sRGB bytes (the placeholder ATCTerrain).
	float FogGamma = 1.f;
	// Optional: the map's cell states and physical cells (FTCWorldObjects); without it, each cell is in its start state.
	const FTCWorldObjects* Objects = nullptr;
	// Optional (round 14): the fog of war as fog.js keeps it (World's frontier darkening, the cells under units lit
	// until the next snapshot, the quarter-second fade). Set, the apron reads its texture, as fogOverlayShader reads the
	// map's own fog texture; unset, it keeps its own three-look fog (FTCCellFog).
	void SetFogSource(const FTCFogFrontier* InSource);

	// The six tiles the paint blends (grass, dirt, mud, field, road, water) and the metres each covers. Until set,
	// the flat colours the reference shows before its textures load. bSRGB: the textures decode sRGB (M_TCApron encodes
	// them back, as the reference blends sRGB bytes). bRawPhotos: they are client/textures' photos as imported, so
	// M_TCApron tints them as ground.js buildTiles does (LOOK contrast, multiplier, saturation); false: already tinted.
	void SetTiles(const TArray<UTexture*>& Tiles, const TArray<float>& Metres, bool bSRGB, bool bRawPhotos);
	// Slice 13 lead glue: the paint's tinted tiles as bytes (FTCGroundPaint::ApronTiles: ground.js buildTiles output, raw
	// bytes as three's DataTexture, wrapped, with a mip chain as generateMipmaps), the LOOK metres; SetTiles(raw, tinted).
	void SetTileBytes(const TArray<TArray<uint8>>& Rgba, const TArray<int32>& Sizes);

	void ResetFrom(const FTCMatchState& State);
	void MarkCells(const FTCMatchState& State, const TSet<int32>& Shaped, const TSet<int32>& Fogged);
	void Flush(const FTCMatchState& State, bool bRevealAll);

	int32 Columns() const { return Core.N; }
	int32 Shapes() const { return ShapeCount; }
	const TCApronCore::FApron& CoreState() const { return Core; }
	UProceduralMeshComponent* MeshComponent() const { return Mesh.Get(); }
	AActor* HolderActor() const { return Actor.Get(); } // slice 13 lead glue: for FTCAtmosphere::AddGroundActor
	UMaterialInstanceDynamic* MaterialInstance() const { return MID.Get(); }

private:
	TCApronCore::FApron Core;
	TCApronCore::FCellSource Source;
	std::vector<uint8_t> FieldCells;
	TArray<uint8> Chars, Orig;
	TArray<int8> Levels;
	TBitArray<> Known;
	TArray<uint8> CellData;
	int32 W = 0, H = 0, ShapeCount = 0;
	bool bWorld = false, bEdgeDirty = false, bCellsDirty = false, bFogAll = true, bLastReveal = false;
	TSet<int32> CellsChanged, FogCells;
	FTCCellFog Fog;
	const FTCFogFrontier* FogSource = nullptr;
	TWeakObjectPtr<UTexture> BoundFog;
	UTexture* FogTexture() const;

	TWeakObjectPtr<AActor> Actor;
	TStrongObjectPtr<UProceduralMeshComponent> Mesh;
	FTCFogOverlay FogOverlay;
	TStrongObjectPtr<UMaterialInterface> Material;
	TStrongObjectPtr<UMaterialInstanceDynamic> MID;
	TStrongObjectPtr<UTexture2D> CellTex, NoiseTex;
	TArray<TStrongObjectPtr<UTexture>> TileTex;
	TArray<float> TileMetres;
	bool bTileSRGB = false, bTileTint = false;

	float Height(float X, float Z) const;
	bool WetCell(int32 C) const;
	void CopyCells(const FTCMatchState& State, const TSet<int32>* Cells);
	void ReadCells(const TSet<int32>* Cells);
	void SendMesh(bool bCreate);
	void BindMaterial();
	void MakeFlatTiles();
};

#endif // TC_APRON_CORE_ONLY
