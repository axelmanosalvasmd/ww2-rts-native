// Slice 13 lead: the 33-map height oracle. docs/contracts/terrain.md's rendered-ground limits, checked on the ground the
// game draws (TCRelief in ATCTerrain's layout: 32-cell chunks, HeightAt on the drawn triangles) for every authored map at
// the pin, against shared/sim.js heightAt and levelAt evaluated by the pinned code
// (Tools/relief/fixtures/heights-oracle-33-maps.json.gz, made by unreal/Tools/terrain/ground-oracle.mjs) and the
// relief inputs the reference client builds (Tools/relief/fixtures/relief-oracle.json.gz, unreal/Tools/relief/relief-oracle.mjs).
//   - the client's levels equal levelAt at every cell centre and at 1,000 seeded points per map; heightAt at a centre is
//     the level x 2.5 m;
//   - every cell centre within 0.1 m of its level x 2.5 m, except banks (a wet cell within one cell) and trenches;
//     cells whose drawn vertices a crater lip shoved are counted apart (relief.js draws them shoved too) and checked
//     in the reference query mode (relief.js hAt);
//   - trench cells 0.9 m lower, road cells 0.1 m lower (0.1 m);
//   - water lines: frozen level x 2.5 + 0.06 m on every wet cell; fords 0.15 m below the line (1 mm); bridges at deck
//     height (0.1 m);
//   - steps of two or more levels drawn as vertical cliffs: for every such neighbour pair, the drawn ground drops by at
//     least 85% of the step within 0.1 m horizontally somewhere within 1 m of the shared edge (three lines across it).
// Small formula checks run both as Unreal automation and as a standalone C++ binary. Expected
// values were recorded from the unmodified e4e579a relief and decal builders under Node.
#if defined(TC_TERRAIN_CONTRACT_CORE_ONLY)
#ifndef TC_WORLD_CORE_ONLY
#define TC_WORLD_CORE_ONLY
#endif
#define TC_WATER_CORE_ONLY
#define TC_RAIL_CORE_ONLY
#define TC_FOG_FRONTIER_CORE_ONLY
#endif
#include "World/TCFogFrontier.h"
#include "World/TCStructures.h"
#include "World/TCWorldObjects.h"
#include "World/TCWater.h"
#include "World/TCRailways.h"
#include "World/TCRelief.h"
#include "World/TCGroundPaint.h"
#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

namespace TCTerrainFormulas
{
	struct FChecks
	{
		std::vector<std::string> Errors;
		void Require(bool Good, const char* What) { if (!Good) Errors.emplace_back(What); }
		void Near(double Got, double Expected, double Tolerance, const char* What)
		{
			if (!std::isfinite(Got) || std::abs(Got - Expected) > Tolerance)
			{
				char Detail[192]; std::snprintf(Detail, sizeof Detail, "%s: %.17g vs %.17g", What, Got, Expected);
				Errors.emplace_back(Detail);
			}
		}
	};

	FChecks FogSamplingRules()
	{
		using namespace TCFogFrontierCore;
		FChecks T;
		#include "World/TCFogFrontier.Sampling.inl"
		// Independent fog.js frontier arithmetic, including diagonal distances and the four-cell limit.
		const int FrontierOracle[][2] = { {-1,0}, {1,244}, {2,221}, {4,174}, {5,153},
			{8,96}, {9,81}, {10,66}, {13,33}, {16,11}, {17,6}, {20,0}, {25,0} };
		for (const auto& S : FrontierOracle) T.Require(FrontierAlpha(S[0]) == S[1], "source radial frontier smoothstep, not a cell outline");
		for (const auto& S : FogSamplingOracle)
		{
			T.Near(MistHash(S[0], S[1]), S[4], 1e-11, "source mist hash");
			T.Near(MistNoise(S[0], S[1]), S[5], 1e-11, "source Hermite value noise");
			T.Near(Mist(S[3], S[0], S[1], S[2]), S[6], 1e-11, "source drifting fog alpha");
		}
		const uint8_t Pixels[] = { 0, 120, 185, 255 };
		const double Sampling[][3] = { { 1, 1, 0 }, { 3, 1, 120 }, { 1, 3, 185 }, { 3, 3, 255 },
			{ 2, 2, 140 }, { 2, 1, 60 }, { 1, 2, 92.5 }, { -7, -8, 0 }, { 9, 9, 255 },
			{ -3, 2, 92.5 }, { 9, 2, 187.5 }, { 2.5, 1.5, 126.875 } };
		for (const auto& S : Sampling) T.Near(SampleAlpha(Pixels, 2, 2, S[0], S[1]), S[2] / 255, 1e-14, "LinearFilter and ClampToEdge at texel centres, boundaries and outside map");
		T.Near(SampleAlpha(nullptr, 0, 0, 0, 0), 0, 0, "empty map has no fog");
		for (const double A : { 0.0, 120.0 / 255, 1.0 })
		{
			T.Near(OuterAlpha(A, 0), A, 1e-14, "outer overlay meets map exactly");
			T.Near(OuterAlpha(A, 6), A, 1e-14, "outer vision carries six metres");
			T.Near(OuterAlpha(A, 19), (A + 185.0 / 255) / 2, 1e-14, "outer transition midpoint");
			T.Near(OuterAlpha(A, 32), 185.0 / 255, 1e-14, "outer overlay reaches unseen");
			T.Near(OuterAlpha(A, 900), 185.0 / 255, 1e-14, "outer water stays fogged");
		}
		T.Near(Composite(.7, 120.0 / 255), .389042675893887, 1e-14, "overlay blends linear lit colour toward shade");
		T.Near(Composite(.7, 1), 10.0 / 255, 1e-14, "opaque frontier meets unknown shade with no light seam");
		T.Near(Composite(.7, 1, true), .0392, 1e-14, "pieces retain literal surfaces.js shade");
		T.Near(Composite(.7, 0), .7, 0, "clear water unchanged by fog");
		TCRelief::FRelief Relief;
		Relief.Reset(2, 1, true, 1, true);
		Relief.InGrid = {'W', '.'}; Relief.InKnown = {1, 0};
		Relief.Build();
		const TCRelief::FPart Seal = Relief.FrontierSeal(0);
		T.Require(!Seal.Idx.empty(), "known river edge joins the unknown placeholder with no sky crack");
		for (const int Owner : Seal.Owner) T.Require(Owner == 0, "frontier seal reads a known cell only");
		for (size_t K = 1; K < Seal.Pos.size() / 3; K += 2)
		{
			T.Near(Seal.Pos[K * 3], 2, 0, "placeholder join uses the exact cell edge");
			T.Near(Seal.Pos[K * 3 + 1], 0, 0, "placeholder join uses the public level zero");
		}
		T.Require(Relief.FrontierSeal(1).Idx.empty(), "undiscovered cells contribute no secret geometry");
		Relief.InKnown = {1, 1}; Relief.Build();
		T.Require(Relief.FrontierSeal(0).Idx.empty(), "a discovered neighbour removes the seal");
		// The same alpha state drives both ground/water overlay and instanced surfaces throughout a fade.
		uint8_t Seen[] = { 0, 1 }, Ever[] = { 1, 1 };
		FFog F; F.Start(2, 1, Seen, Ever, false, false);
		F.Snapshot({ 0, 1 }, {}); F.Frame(.05, false);
		T.Require(F.Bytes[0] == 83 && F.Bytes[1] == 37, "fade uses alpha per second before byte rounding");
		T.Near(SampleAlpha(F.Bytes.data(),2,1,2,1), 60.0 / 255, 1e-14, "a fading cell boundary is bilinear, without nearest-cell steps");
		F.Frame(.20, false);
		T.Require(F.Bytes[0] == 0 && F.Bytes[1] == 120, "quarter-second fade settles exactly");
		return T;
	}

	FChecks ReliefRules()
	{
		FChecks T;
		for (const bool Low : { false, true }) for (int Kind = 0; Kind < 4; Kind++)
		{
			TCRelief::FRelief R;
			R.Reset(7, 7, Low, 0, false);
			for (int Y = 0; Y < 7; Y++) for (int X = 0; X < 7; X++)
			{
				const int C = Y * 7 + X;
				R.InKnown[(size_t)C] = 1;
				R.InGrid[(size_t)C] = Kind == 0 && X == 3 && Y == 3 ? 'T' : '.';
				R.InRoad[(size_t)C] = Kind == 0 && Y == 3 && (X == 2 || X == 4);
				R.InLevel[(size_t)C] = (int8_t)(Kind == 0 ? 2 : Kind == 1 ? (X < 3 ? 0 : 1)
					: Kind == 2 ? (X < 3 ? 0 : 3) : ((X < 3) == (Y < 3) ? 0 : 2));
				R.InFrozen[(size_t)C] = R.InLevel[(size_t)C];
			}
			R.Build();
			if (Kind == 0)
			{
				T.Near(R.HAt(1, 1), 5, 1e-7, "centre = level x 2.5");
				T.Near(R.HAt(5, 7), 4.9, 1e-7, "road sink 0.1");
				T.Near(R.HAt(7, 7), 4.099999904632568, 1e-7, "trench sink 0.9");
				T.Near(R.HAt(7, 6), 4.974999904632568, 1e-7, "isolated trench keeps its shoulder");
			}
			if (Kind == 1)
			{
				const double Xs[] = { 5, 5.5, 5.95, 6.05, 6.5, 7 };
				const double Ys[] = { 0, 0.46875, 1.171875, 1.328125, 2.03125, 2.5 };
				for (int I = 0; I < 6; I++) T.Near(R.HAt(Xs[I], 7), Ys[I], 1e-7, "connected one-step ramp");
				T.Require(R.Stats.CliffEdges == 0, "one step has no cliff");
			}
			if (Kind == 2)
			{
				T.Require(R.Stats.CliffEdges == 7, "three-step face is sealed along all seven cells");
				T.Require(R.Stats.Vertices == (Low ? 4462 : 11278), "reference fractured-rock geometry in each graphics variant");
				T.Near(R.HAt(5.95, 7), 0, 1e-7, "cliff low side stays flat");
				T.Near(R.HAt(6.5, 7), 7.5, 1e-7, "cliff high side stays flat");
			}
			if (Kind == 3)
			{
				T.Near(R.HAt(5.95, 5.95), 0, 1e-7, "disconnected diagonal low corner");
				T.Near(R.HAt(6.05, 5.95), 5, 1e-7, "disconnected diagonal east corner");
				T.Near(R.HAt(5.95, 6.05), 5, 1e-7, "disconnected diagonal south corner");
				T.Near(R.HAt(6.05, 6.05), 0, 1e-7, "no diagonal ramp across cliffs");
			}
		}
		return T;
	}

	FChecks PaintRules()
	{
		using namespace TCGroundPaintCore;
		FChecks T;
		const double Cases[][3] = { { 0, 0, 0 }, { .3, .3, .5 }, { .3, .25, .87090625 },
			{ .3, .35, .12909375 }, { .3, 0, 1 }, { .3, 1, 0 } };
		for (const auto& C : Cases) T.Near(SecondaryWeight(C[0], C[1]), C[2], 1e-14, "reference noise-patch coverage");
		T.Require(FGround::PixelsPerCell(110, 120, false, false) == 17, "High paint texel budget");
		T.Require(FGround::PixelsPerCell(110, 120, false, true) == 10, "Low paint texel budget");
		FStartMap Map; Map.W = 7; Map.H = 5;
		Map.Rows = { ".W.....", "..BD...", ".......", ".......", "......." };
		FGround G; G.Start(Map, {}, nullptr);
		std::vector<uint8_t> Roads; G.CopyRoadMask(Roads);
		T.Require(Roads.size() == 35 && !Roads[1] && !Roads[9] && Roads[10] && Roads[17], "roads follow painted water, house, road and yard decisions");
		T.Require(G.Attributes().Sec[0] == Mud, "mud beside water");
		T.Near(G.Attributes().Amt[0], .55, 1e-7, "wet-bank mud coverage");
		FCellRow Wear; Wear.C = 28; Wear.Ch = '.'; Wear.bState = true; Wear.State = 3;
		G.Apply({ Wear });
		T.Require(G.Attributes().Sec[28] == Mud, "three quarters wear exposes mud");
		T.Near(G.Attributes().Amt[28], .72, 1e-7, "worn coverage");
		Wear.State = 3 << 5; G.Apply({ Wear });
		T.Require(G.Attributes().Sec[28] == Shell && G.Attributes().Scar[28] == 1, "damage stage three makes a blast scar");
		T.Near(G.Attributes().Amt[28], .7, 1e-7, "damage-stage coverage");
		Wear.State = 4; G.Apply({ Wear });
		T.Require(G.Attributes().Char[28] == 1, "burned ground receives char tint");
		// The source paints public placeholders beneath its filtered frontier. Native
		// keeps knowledge separate and normalizes undescribed inputs before painting.
		FStartMap Public; Public.W = 12; Public.H = 10; Public.bWorld = true;
		Public.Rows.assign(10, std::string(12, '.'));
		Public.Heights.assign(10, std::string(12, '0'));
		std::vector<FCellRow> Delivered;
		for (int C = 0; C < 5; C++)
		{
			FCellRow R; R.C = 5 * 12 + C; R.Ch = C == 4 ? 'W' : '.';
			R.bLevel = R.bState = R.bData = true; R.Ground = R.Ch; R.Object = '.'; R.Biome = 255;
			Delivered.push_back(R);
		}
		FStartMap Scrambled = Public;
		Scrambled.Rows.assign(10, std::string(12, '+')); Scrambled.Heights.assign(10, std::string(12, '4'));
		Scrambled.LayerGround.assign(10, std::string(12, 'W')); Scrambled.LayerObjects.assign(10, std::string(12, 'R'));
		Scrambled.Biome.assign(120, 3);
		FGround Source; Source.Mode = EKnownMode::Reference; Source.Start(Public, Delivered, nullptr);
		FGround Native; Native.Start(Scrambled, Delivered, nullptr);
		T.Require(Native.Canvas() == Source.Canvas(), "native frontier underpaint equals source public-placeholder raster");
		T.Require(!Native.IsKnown(66) && Native.IsKnown(64), "painting a placeholder never discovers it");
		T.Require(Native.CharAt(66) == '.' && Native.LevelAt(66) == 0 && Native.ObjectAt(66) == '.', "unknown input cannot replace the public placeholder");
		return T;
	}

	FChecks DecalRules()
	{
		using namespace TCGroundPaintCore;
		FChecks T;
		const auto Height = [](double X, double Z) { return X * .1 - Z * .2; };
		float Pos[75], Uv[50]; RubbleQuad(12, 7, "wood", Height, Pos, Uv);
		T.Near(Pos[0], 10.119999885559082, 1e-7, "rubble stays inside its cell");
		T.Near(Pos[1], .6129999756813049, 1e-7, "rubble drapes 2.5 cm above relief");
		T.Near(Pos[74], 3.880000114440918, 1e-7, "rubble far corner");
		T.Near(Uv[0], .001, 1e-7, "wood atlas inset");
		T.Near(Uv[1], .499, 1e-7, "deterministic atlas quarter-turn");
		T.Require(RubbleQuadrant("brick") == 0 && RubbleQuadrant("concrete") == 1
			&& RubbleQuadrant("wood") == 2 && RubbleQuadrant("road") == 3, "reference material quadrants");

		FStartMap Map; Map.W = Map.H = 64; Map.bWorld = true; Map.Rows.assign(64, std::string(64, '+'));
		FGround G; G.Mode = EKnownMode::Reference; G.Start(Map, {}, nullptr);
		TCDecals::FKnownCells D; D.Reset(); D.Update(G, true, {}, Height);
		T.Require(D.Craters().Count() == 1536, "reference World crater capacity");
		const int First[] = { 0, 1, 2, 65, 3, 67, 5, 68, 6, 7, 71, 73, 76, 77, 78, 79 };
		for (int I = 0; I < 16; I++) T.Require(D.Craters().Cells[(size_t)I] == First[I], "hydration uses update neighbour order");
		const int Last[] = { 3087, 3088, 3089, 3092, 3098, 3099, 3107, 3108 };
		for (int I = 0; I < 8; I++) T.Require(D.Craters().Cells[(size_t)(1528 + I)] == Last[I], "capacity retains the same reference cells");
		T.Near(D.Craters().Pos[0], 3.531231641769409, 1e-7, "crater radius, rotation and jitter");
		T.Near(D.Craters().Pos[1], .19017401337623596, 1e-7, "crater drapes 5 cm above relief");

		Map.W = Map.H = 2; Map.Rows.assign(2, "++"); G.Mode = EKnownMode::Native; G.Start(Map, {}, nullptr);
		D.Reset(); D.Update(G, true, {}, Height);
		T.Require(D.Craters().Count() == 0, "unknown World craters never allocate pictures");
		FCellRow Cell; Cell.C = 0; Cell.Ch = '+'; G.Apply({ Cell }); D.Update(G, false, { 0 }, Height);
		T.Require(D.Craters().Cells == std::vector<int>{ 0 }, "a discovered crater gains a picture");
		Cell.Ch = 'W'; G.Apply({ Cell }); D.Update(G, false, { 0 }, Height);
		T.Require(D.Craters().Count() == 0, "a flooded crater loses its picture");
		Map.bWorld = false; G.Start(Map, {}, nullptr); D.Reset(); D.Update(G, true, {}, Height);
		T.Require(D.Craters().Count() == 0, "authored maps keep painted shell holes without World crater pictures");
		return T;
	}
	// Recorded by reference-contracts.mjs from the unmodified pin. Nine looks, High/Low and four damage footprints.
	FChecks StructureRules()
	{
		using namespace TCWorldCore;
		FChecks T;
		struct FExpected { const char* Name; int Type, Nx, Ny, X0, Y0; bool Low; int Damage, Total; int Counts[PieceCount]; double Sums[6]; };
		static const FExpected Cases[] = {
			{ "shed", 0, 2, 3, 1, 1, false, 0, 19, { 0, 1, 4, 0, 7, 0, 1, 0, 0, 1, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 82.000000238418579, 29.95800906419754, 82.759999871253967, 13.811208672821522, 13.14239090681076, 12.411557164043188 } },
			{ "shed", 0, 2, 3, 1, 1, false, 1, 27, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 21, 0, 0, 0, 0, 6, 0, 0, 0, 0, 0 }, { 107.01818132400513, -1.1214751861989498, 126.46199154853821, 33.669998824596405, 28.490000605583191, 22.015000581741333 } },
			{ "shed", 0, 2, 3, 1, 1, false, 2, 29, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 21, 0, 0, 0, 0, 8, 0, 0, 0, 0, 0 }, { 118.43034672737122, -0.75919222645461559, 141.64218235015869, 34.47999894618988, 29.140000581741333, 22.890000462532043 } },
			{ "shed", 0, 2, 3, 1, 1, false, 3, 27, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 17, 0, 0, 0, 0, 0 }, { 109.02859210968018, 1.4007061943411827, 137.90769052505493, 26.739999532699585, 22.520000398159027, 18.519999980926514 } },
			{ "shed", 0, 2, 3, 1, 1, true, 0, 19, { 0, 1, 4, 0, 7, 0, 1, 0, 0, 1, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 82.000000238418579, 29.95800906419754, 82.759999871253967, 13.811208672821522, 13.14239090681076, 12.411557164043188 } },
			{ "shed", 0, 2, 3, 1, 1, true, 1, 25, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0 }, { 100.55030107498169, -1.1861094385385513, 121.06264781951904, 31.979998826980591, 27.060000598430634, 20.910000562667847 } },
			{ "shed", 0, 2, 3, 1, 1, true, 2, 24, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 19, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0 }, { 96.889854431152344, -1.0118923056870699, 119.87137460708618, 29.97999894618988, 25.620000541210175, 20.160000443458557 } },
			{ "shed", 0, 2, 3, 1, 1, true, 3, 15, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0 }, { 59.221808671951294, 0.57704457640647888, 79.895585298538208, 15.499999523162842, 13.620000302791595, 11.25 } },
			{ "barn", 0, 3, 3, 2, 1, false, 0, 9, { 0, 1, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 63, 24.401030480861664, 60.220002174377441, 5.5208455733954906, 5.1819240022450686, 4.573455554433167 } },
			{ "barn", 0, 3, 3, 2, 1, false, 1, 36, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 25, 0, 0, 0, 0, 11, 0, 0, 0, 0, 0 }, { 254.59546160697937, 2.4154570978134871, 173.18401741981506, 33.439999580383301, 28.420000374317169, 22.38400012254715 } },
			{ "barn", 0, 3, 3, 2, 1, false, 2, 48, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0 }, { 351.13029336929321, 3.725147906690836, 237.81092512607574, 43.069999635219574, 35.810000449419022, 28.316000014543533 } },
			{ "barn", 0, 3, 3, 2, 1, false, 3, 42, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 13, 0, 0, 0, 0, 29, 0, 0, 0, 0, 0 }, { 300.24821925163269, 6.4899644274264574, 213.46635687351227, 33.434999883174896, 28.040000140666962, 23.565999716520309 } },
			{ "barn", 0, 3, 3, 2, 1, true, 0, 9, { 0, 1, 1, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 63, 24.401030480861664, 60.220002174377441, 5.5208455733954906, 5.1819240022450686, 4.573455554433167 } },
			{ "barn", 0, 3, 3, 2, 1, true, 1, 34, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 25, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0 }, { 245.70815682411194, 2.035533158108592, 167.2677960395813, 31.539999604225159, 26.560000360012054, 20.584000170230865 } },
			{ "barn", 0, 3, 3, 2, 1, true, 2, 39, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 0, 11, 0, 0, 0, 0, 0 }, { 285.11114120483398, 2.4522895514965057, 198.42139494419098, 36.629999577999115, 30.540000438690186, 24.044000089168549 } },
			{ "barn", 0, 3, 3, 2, 1, true, 3, 21, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 15, 0, 0, 0, 0, 0 }, { 148.13219904899597, 3.1148964297026396, 113.65602195262909, 17.629999816417694, 15.060000091791153, 12.763999909162521 } },
			{ "plaster", 1, 3, 3, 2, 1, false, 0, 83, { 1, 59, 0, 0, 0, 1, 0, 0, 0, 1, 0, 18, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 567.61499810218811, 272.24155007302761, 409.70000028610229, 74.760498397052288, 71.37007544003427, 70.633598817512393 } },
			{ "plaster", 1, 3, 3, 2, 1, false, 1, 36, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 25, 0, 0, 0, 0, 0, 0, 0, 11, 0, 0, 0, 0, 0 }, { 254.59546160697937, 2.4154570978134871, 173.18401741981506, 36.579999327659607, 33.140000581741333, 26.960000276565552 } },
			{ "plaster", 1, 3, 3, 2, 1, false, 2, 48, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0 }, { 351.13029336929321, 3.725147906690836, 237.81092512607574, 49.049999177455902, 43.410000801086426, 35.34000027179718 } },
			{ "plaster", 1, 3, 3, 2, 1, false, 3, 42, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 13, 0, 0, 0, 0, 0, 0, 0, 29, 0, 0, 0, 0, 0 }, { 300.24821925163269, 6.4899644274264574, 213.46635687351227, 42.639999389648438, 37.440000653266907, 31.579999923706055 } },
			{ "plaster", 1, 3, 3, 2, 1, true, 0, 83, { 1, 59, 0, 0, 0, 1, 0, 0, 0, 1, 0, 18, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 567.61499810218811, 272.24155007302761, 409.70000028610229, 74.760498397052288, 71.37007544003427, 70.633598817512393 } },
			{ "plaster", 1, 3, 3, 2, 1, true, 1, 34, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 25, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0 }, { 245.70815682411194, 2.035533158108592, 167.2677960395813, 34.679999351501465, 31.280000567436218, 25.160000324249268 } },
			{ "plaster", 1, 3, 3, 2, 1, true, 2, 39, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 0, 0, 0, 0, 11, 0, 0, 0, 0, 0 }, { 285.11114120483398, 2.4522895514965057, 198.42139494419098, 39.839999318122864, 35.380000650882721, 28.740000247955322 } },
			{ "plaster", 1, 3, 3, 2, 1, true, 3, 21, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 15, 0, 0, 0, 0, 0 }, { 148.13219904899597, 3.1148964297026396, 113.65602195262909, 21.199999690055847, 18.860000312328339, 16.059999942779541 } },
			{ "brick", 1, 3, 3, 1, 1, false, 0, 108, { 0, 58, 0, 2, 0, 0, 0, 0, 1, 1, 0, 21, 0, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 541.62000298500061, 383.24473276734352, 524.41500318050385, 90.307943038642406, 90.336760379374027, 89.66067062318325 } },
			{ "brick", 1, 3, 3, 1, 1, false, 1, 37, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 0, 0, 0, 0, 0, 8, 0, 0, 0, 0, 0 }, { 182.52896690368652, -0.47892815992236137, 170.20306301116943, 30.560000419616699, 26.460001051425934, 25.359999537467957 } },
			{ "brick", 1, 3, 3, 1, 1, false, 2, 43, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 0, 0, 15, 0, 0, 0, 0, 0 }, { 219.36585688591003, 0.71710023656487465, 212.02316272258759, 38.840000331401825, 31.08000111579895, 28.739999353885651 } },
			{ "brick", 1, 3, 3, 1, 1, false, 3, 38, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 0, 0, 0, 0, 0, 27, 0, 0, 0, 0, 0 }, { 189.71548318862915, 3.5246751680970192, 193.77148592472076, 39.430000066757202, 27.810000836849213, 24.079999268054962 } },
			{ "brick", 1, 3, 3, 1, 1, true, 0, 108, { 0, 58, 0, 2, 0, 0, 0, 0, 1, 1, 0, 21, 0, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 541.62000298500061, 383.24473276734352, 524.41500318050385, 90.307943038642406, 90.336760379374027, 89.66067062318325 } },
			{ "brick", 1, 3, 3, 1, 1, true, 1, 35, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0 }, { 176.06108665466309, -0.54356241226196289, 164.80371928215027, 28.320000410079956, 25.140000998973846, 24.319999575614929 } },
			{ "brick", 1, 3, 3, 1, 1, true, 2, 33, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 24, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0 }, { 161.37831211090088, -0.26852789148688316, 168.2200893163681, 27.81000030040741, 24.210000872612, 23.159999549388885 } },
			{ "brick", 1, 3, 3, 1, 1, true, 3, 19, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 14, 0, 0, 0, 0, 0 }, { 89.549510717391968, 1.5859788693487644, 104.82699739933014, 18.660000026226044, 14.460000395774841, 13.059999644756317 } },
			{ "stone", 2, 3, 3, 1, 1, false, 0, 57, { 0, 8, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 21, 26, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 286.62000298500061, 230.89335384964943, 269.41500318050385, 29.4488869830966, 29.889704324305058, 29.691614672541618 } },
			{ "stone", 2, 3, 3, 1, 1, false, 1, 37, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 29, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 0, 0 }, { 182.52896690368652, -0.47892815992236137, 170.20306301116943, 35.149999558925629, 33.399999141693115, 30.599999725818634 } },
			{ "stone", 2, 3, 3, 1, 1, false, 2, 43, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 0, 0, 0, 15, 0, 0, 0, 0, 0 }, { 219.36585688591003, 0.71710023656487465, 212.02316272258759, 41.359999537467957, 38.349999308586121, 35.329999566078186 } },
			{ "stone", 2, 3, 3, 1, 1, false, 3, 38, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 11, 0, 0, 0, 0, 0, 0, 27, 0, 0, 0, 0, 0 }, { 189.71548318862915, 3.5246751680970192, 193.77148592472076, 37.119999647140503, 33.279999732971191, 30.729999482631683 } },
			{ "stone", 2, 3, 3, 1, 1, true, 0, 57, { 0, 8, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 21, 26, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 286.62000298500061, 230.89335384964943, 269.41500318050385, 29.4488869830966, 29.889704324305058, 29.691614672541618 } },
			{ "stone", 2, 3, 3, 1, 1, true, 1, 35, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0, 0, 0 }, { 176.06108665466309, -0.54356241226196289, 164.80371928215027, 33.249999582767487, 31.599999189376831, 28.959999740123749 } },
			{ "stone", 2, 3, 3, 1, 1, true, 2, 33, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 24, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0 }, { 161.37831211090088, -0.26852789148688316, 168.2200893163681, 31.689999639987946, 29.559999465942383, 27.349999666213989 } },
			{ "stone", 2, 3, 3, 1, 1, true, 3, 19, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 14, 0, 0, 0, 0, 0 }, { 89.549510717391968, 1.5859788693487644, 104.82699739933014, 18.389999806880951, 17.049999892711639, 16.109999716281891 } },
			{ "church", 3, 3, 4, 1, 1, false, 0, 18, { 0, 5, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0, 4, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 89.999999761581421, 109.81000036001205, 96.209999442100525, 8.749329999089241, 9.0584023930132389, 9.7219378603622317 } },
			{ "church", 3, 3, 4, 1, 1, false, 1, 50, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 41, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0 }, { 245.09642958641052, -2.1369726217817515, 287.70917320251465, 51.819998145103455, 49.899999976158142, 46.060000777244568 } },
			{ "church", 3, 3, 4, 1, 1, false, 2, 57, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 37, 0, 0, 0, 0, 0, 0, 20, 0, 0, 0, 0, 0 }, { 290.02452635765076, -0.27119813929311931, 335.83700788021088, 59.039998292922974, 54.460000157356262, 50.030000448226929 } },
			{ "church", 3, 3, 4, 1, 1, false, 3, 49, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 37, 0, 0, 0, 0, 0 }, { 235.18567323684692, 3.6481934497132897, 291.143195271492, 50.139999032020569, 44.440000295639038, 40.96999979019165 } },
			{ "church", 3, 3, 4, 1, 1, true, 0, 18, { 0, 5, 0, 0, 0, 0, 0, 1, 0, 1, 1, 0, 4, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 89.999999761581421, 109.81000036001205, 96.209999442100525, 8.749329999089241, 9.0584023930132389, 9.7219378603622317 } },
			{ "church", 3, 3, 4, 1, 1, true, 1, 48, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 40, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 0, 0 }, { 238.62854933738708, -2.201606874121353, 282.30982947349548, 49.7399982213974, 47.899999976158142, 44.220000743865967 } },
			{ "church", 3, 3, 4, 1, 1, true, 2, 45, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 33, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0 }, { 221.69230556488037, -1.4028145007323474, 276.0696085691452, 46.409998595714569, 43.550000071525574, 40.210000395774841 } },
			{ "church", 3, 3, 4, 1, 1, true, 3, 24, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 19, 0, 0, 0, 0, 0 }, { 108.00059342384338, 1.6160134812816978, 151.66929590702057, 24.029999554157257, 22.170000076293945, 20.869999825954437 } },
			{ "factory", 4, 4, 6, 1, 1, false, 0, 141, { 0, 68, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 34, 36, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 853.1344780921936, 561.50314211845398, 1133.7891280651093, 109.00029689911753, 109.49600427690893, 108.39909579884261 } },
			{ "factory", 4, 4, 6, 1, 1, false, 1, 94, { 0, 2, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 73, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0 }, { 544.56037473678589, 68.577195552410558, 702.03983068466187, 74.552984126843512, 66.163614817894995, 63.99032876919955 } },
			{ "factory", 4, 4, 6, 1, 1, false, 2, 123, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 79, 0, 0, 0, 0, 0, 44, 0, 0, 0, 0, 0 }, { 741.46371519565582, -1.3798416760400869, 978.22643840312958, 111.78000104427338, 87.480003297328949, 80.279998183250427 } },
			{ "factory", 4, 4, 6, 1, 1, false, 3, 104, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 27, 0, 0, 0, 0, 0, 77, 0, 0, 0, 0, 0 }, { 616.8275533914566, 6.9746327550965361, 835.79334461688995, 110.03000020980835, 74.850002348423004, 63.679997980594635 } },
			{ "factory", 4, 4, 6, 1, 1, true, 0, 141, { 0, 68, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 34, 36, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 853.1344780921936, 561.50314211845398, 1133.7891280651093, 109.00029689911753, 109.49600427690893, 108.39909579884261 } },
			{ "factory", 4, 4, 6, 1, 1, true, 1, 92, { 0, 2, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 72, 0, 0, 0, 0, 0, 15, 0, 0, 0, 0, 0 }, { 538.09249448776245, 68.512561300070956, 696.6404869556427, 72.312984117306769, 64.843614765442908, 62.950328807346523 } },
			{ "factory", 4, 4, 6, 1, 1, true, 2, 97, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 0, 0, 0, 0, 0, 28, 0, 0, 0, 0, 0 }, { 581.22808682918549, -2.877058633370325, 773.14784348011017, 82.830000936985016, 70.050002634525299, 66.379998683929443 } },
			{ "factory", 4, 4, 6, 1, 1, true, 3, 48, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0, 39, 0, 0, 0, 0, 0 }, { 276.78595125675201, 3.4543599560856819, 382.19252073764801, 48.840000092983246, 35.460001051425934, 31.139999091625214 } },
			{ "mine", 5, 4, 6, 1, 1, false, 0, 59, { 0, 2, 0, 1, 0, 0, 0, 0, 1, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 48, 0, 0, 0 }, { 353.99999928474426, 524.15453359484673, 327.99999928474426, 29.737450197339058, 19.276626572012901, 17.032176330685616 } },
			{ "mine", 5, 4, 6, 1, 1, false, 1, 137, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 73, 0, 0, 0, 0, 0, 16, 0, 48, 0, 0, 0 }, { 802.56037473678589, 494.46969833713956, 903.93983221054077, 97.084000796079636, 78.372002944350243, 73.819999024271965 } },
			{ "mine", 5, 4, 6, 1, 1, false, 2, 171, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 79, 0, 0, 0, 0, 0, 44, 0, 48, 0, 0, 0 }, { 1029.4637151956558, 498.10580266447505, 1196.6264396905899, 136.70400080084801, 101.95200370252132, 92.339998289942741 } },
			{ "mine", 5, 4, 6, 1, 1, false, 3, 104, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 27, 0, 0, 0, 0, 0, 77, 0, 0, 0, 0, 0 }, { 616.8275533914566, 6.9746327550965361, 835.79334461688995, 110.03000020980835, 74.850002348423004, 63.679997980594635 } },
			{ "mine", 5, 4, 6, 1, 1, true, 0, 59, { 0, 2, 0, 1, 0, 0, 0, 0, 1, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 48, 0, 0, 0 }, { 353.99999928474426, 524.15453359484673, 327.99999928474426, 29.737450197339058, 19.276626572012901, 17.032176330685616 } },
			{ "mine", 5, 4, 6, 1, 1, true, 1, 135, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 72, 0, 0, 0, 0, 0, 15, 0, 48, 0, 0, 0 }, { 796.09249448776245, 494.40506408479996, 898.54048848152161, 94.844000786542892, 77.052002891898155, 72.779999062418938 } },
			{ "mine", 5, 4, 6, 1, 1, true, 2, 145, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 0, 0, 0, 0, 0, 28, 0, 48, 0, 0, 0 }, { 869.22808682918549, 496.60858570714481, 991.5478447675705, 107.75400069355965, 84.522003039717674, 78.439998790621758 } },
			{ "mine", 5, 4, 6, 1, 1, true, 3, 48, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0, 39, 0, 0, 0, 0, 0 }, { 276.78595125675201, 3.4543599560856819, 382.19252073764801, 48.840000092983246, 35.460001051425934, 31.139999091625214 } },
			{ "block", 2, 5, 5, 1, 1, false, 0, 79, { 0, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 33, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 568.00330400466919, 306.97000056505203, 572.88737392425537, 40.694716721773148, 41.288844615221024, 40.095620051026344 } },
			{ "block", 2, 5, 5, 1, 1, false, 1, 96, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 77, 0, 0, 0, 0, 0, 0, 19, 0, 0, 0, 0, 0 }, { 658.19738292694092, -0.23894426110200584, 655.90248489379883, 91.19999885559082, 86.499997735023499, 78.979999303817749 } },
			{ "block", 2, 5, 5, 1, 1, false, 2, 128, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 80, 0, 0, 0, 0, 0, 0, 48, 0, 0, 0, 0, 0 }, { 909.79620897769928, 5.6641581402509473, 893.11100673675537, 123.6399986743927, 112.93999791145325, 102.9899987578392 } },
			{ "block", 2, 5, 5, 1, 1, false, 3, 104, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 26, 0, 0, 0, 0, 0, 0, 78, 0, 0, 0, 0, 0 }, { 713.24116861820221, 12.341367745946627, 727.47592067718506, 102.539999127388, 89.399999380111694, 81.529998600482941 } },
			{ "block", 2, 5, 5, 1, 1, true, 0, 79, { 0, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 33, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 }, { 568.00330400466919, 306.97000056505203, 572.88737392425537, 40.694716721773148, 41.288844615221024, 40.095620051026344 } },
			{ "block", 2, 5, 5, 1, 1, true, 1, 94, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 76, 0, 0, 0, 0, 0, 0, 18, 0, 0, 0, 0, 0 }, { 651.72950267791748, -0.30357851344160736, 650.50314116477966, 89.299998879432678, 84.699997782707214, 77.339999318122864 } },
			{ "block", 2, 5, 5, 1, 1, true, 2, 99, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 0, 0, 0, 0, 0, 0, 30, 0, 0, 0, 0, 0 }, { 695.50527322292328, 2.2123465773183852, 694.302649974823, 95.069998919963837, 88.189998209476471, 80.769999086856842 } },
			{ "block", 2, 5, 5, 1, 1, true, 3, 48, { 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 39, 0, 0, 0, 0, 0 }, { 317.58396661281586, 5.6712492750957608, 337.8716185092926, 46.959999561309814, 42.089999735355377, 39.089999318122864 } },
		};
		struct FSource final : FCellSource
		{
			std::vector<unsigned char> Original, Live;
			bool bLow = false;
			FSource() : Original(256, '.'), Live(256, '.') {}
			int Width() const override { return 16; }
			int Height() const override { return 16; }
			unsigned char At(int X, int Y) const override { return X >= 0 && X < 16 && Y >= 0 && Y < 16 ? Live[(size_t)Y * 16 + X] : 0; }
			unsigned char Orig(int X, int Y) const override { return X >= 0 && X < 16 && Y >= 0 && Y < 16 ? Original[(size_t)Y * 16 + X] : 0; }
			int Stage(int, int) const override { return 0; }
			bool Mine(int, int) const override { return false; }
			bool StoneDeck(int, int) const override { return false; }
			bool Spoil(int, int) const override { return false; }
			double Ground(double X, double Z) const override { return X * .03 - Z * .02; }
			bool Low() const override { return bLow; }
		};
		for (const auto& E : Cases)
		{
			FSource Source; Source.bLow = E.Low;
			std::vector<unsigned char> Foot(256, 0);
			for (int Y = E.Y0; Y < E.Y0 + E.Ny; Y++) for (int X = E.X0; X < E.X0 + E.Nx; X++)
			{
				const int C = Y * 16 + X;
				Source.Original[(size_t)C] = 'B'; Foot[(size_t)C] = 1;
				const bool Fallen = E.Damage == 3 || (E.Damage == 2 && (X + Y) % 2 == 0) || (E.Damage == 1 && X == E.X0 && Y == E.Y0);
				Source.Live[(size_t)C] = Fallen ? 'R' : 'B';
			}
			const auto Rects = Layout(BlocksIn(Foot, 16, 16, [&](int C) { return C == E.Y0 * 16 + E.X0 ? E.Type : -1; }, nullptr));
			std::vector<int> TintIndex;
			std::vector<FHouseTint> Tints;
			HouseTints(Rects, 16, 16, TintIndex, Tints);
			std::vector<FPiece> Pieces;
			const TCStructures::FContext Context{ &Foot, &TintIndex, &Tints };
			for (const auto& Rect : Rects) TCStructures::House(Source, Rect, Pieces, Context);
			TCStructures::Chunk(Source, 0, 0, 16, 16, Pieces, Context);
			int Counts[PieceCount] = {}; double Sums[6] = {};
			for (const auto& P : Pieces)
			{
				Counts[(int)P.Kind]++;
				Sums[0] += P.X; Sums[1] += P.Y; Sums[2] += P.Z; Sums[3] += P.R; Sums[4] += P.G; Sums[5] += P.B;
			}
			const size_t Before = T.Errors.size();
			T.Require((int)Pieces.size() == E.Total, "source structure piece count");
			for (int I = 0; I < PieceCount; I++) T.Require(Counts[I] == E.Counts[I], PieceName((EPiece)I));
			for (int I = 0; I < 6; I++) T.Near(Sums[I], E.Sums[I], .001, "source placement/tint sums");
			for (size_t I = Before; I < T.Errors.size(); I++) T.Errors[I].insert(0, std::string(E.Name) + (E.Low ? " Low " : " High ") + std::to_string(E.Damage) + ": ");
		}
		return T;
	}

	FChecks BreachRules()
	{
		FChecks T;
		// Recorded from unmodified unit-models.js breachGeometry at e4e579a. Includes attribute interpolation,
		// wall cuts, adjacent holes, complete removal, indexed topology and a translated/rotated model.
		using namespace TCStructures;
		struct FCase { std::vector<float> In; std::vector<FColumn> Holes; double Matrix[16]; std::vector<float> Expected; };
		const FCase Cases[] = {
			// none
			{ { -1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,3.f,1.f,-1.f,0.f,1.f,0.f,1.f,0.f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,-1.f,1.f,3.f,0.f,1.f,0.f,0.f,1.f }, {  }, { 1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1 },
			  { -1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,3.f,1.f,-1.f,0.f,1.f,0.f,1.f,0.f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,-1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,-1.f,1.f,3.f,0.f,1.f,0.f,0.f,1.f } },
			// hole
			{ { -1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,3.f,1.f,-1.f,0.f,1.f,0.f,1.f,0.f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,-1.f,1.f,3.f,0.f,1.f,0.f,0.f,1.f }, { {0,2,0,2} }, { 1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1 },
			  { -1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f,-1.f,0.f,1.f,0.f,0.25f,0.f,0.f,1.f,0.f,0.f,1.f,0.f,0.25f,0.25f,2.f,1.f,-1.f,0.f,1.f,0.f,0.75f,0.f,3.f,1.f,-1.f,0.f,1.f,0.f,1.f,0.f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,2.f,1.f,-1.f,0.f,1.f,0.f,0.75f,0.f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,2.f,1.f,2.f,0.f,1.f,0.f,0.75f,0.75f,0.f,1.f,-1.f,0.f,1.f,0.f,0.25f,0.f,2.f,1.f,-1.f,0.f,1.f,0.f,0.75f,0.f,2.f,1.f,0.f,0.f,1.f,0.f,0.75f,0.25f,0.f,1.f,-1.f,0.f,1.f,0.f,0.25f,0.f,2.f,1.f,0.f,0.f,1.f,0.f,0.75f,0.25f,0.f,1.f,0.f,0.f,1.f,0.f,0.25f,0.25f,-1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f,0.f,0.f,1.f,0.f,0.25f,0.25f,0.f,1.f,3.f,0.f,1.f,0.f,0.25f,1.f,-1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f,3.f,0.f,1.f,0.f,0.25f,1.f,-1.f,1.f,3.f,0.f,1.f,0.f,0.f,1.f,2.f,1.f,2.f,0.f,1.f,0.f,0.75f,0.75f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,2.f,1.f,3.f,0.f,1.f,0.f,0.75f,1.f,2.f,1.f,2.f,0.f,1.f,0.f,0.75f,0.75f,2.f,1.f,3.f,0.f,1.f,0.f,0.75f,1.f,0.f,1.f,3.f,0.f,1.f,0.f,0.25f,1.f,2.f,1.f,2.f,0.f,1.f,0.f,0.75f,0.75f,0.f,1.f,3.f,0.f,1.f,0.f,0.25f,1.f,0.f,1.f,2.f,0.f,1.f,0.f,0.25f,0.75f } },
			// two
			{ { -1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,3.f,1.f,-1.f,0.f,1.f,0.f,1.f,0.f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,-1.f,1.f,3.f,0.f,1.f,0.f,0.f,1.f }, { {0,2,0,2},{2,4,0,2} }, { 1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1 },
			  { -1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f,-1.f,0.f,1.f,0.f,0.25f,0.f,0.f,1.f,0.f,0.f,1.f,0.f,0.25f,0.25f,2.f,1.f,-1.f,0.f,1.f,0.f,0.75f,0.f,3.f,1.f,-1.f,0.f,1.f,0.f,1.f,0.f,3.f,1.f,0.f,0.f,1.f,0.f,1.f,0.25f,2.f,1.f,-1.f,0.f,1.f,0.f,0.75f,0.f,3.f,1.f,0.f,0.f,1.f,0.f,1.f,0.25f,2.f,1.f,0.f,0.f,1.f,0.f,0.75f,0.25f,3.f,1.f,2.f,0.f,1.f,0.f,1.f,0.75f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,2.f,1.f,2.f,0.f,1.f,0.f,0.75f,0.75f,0.f,1.f,-1.f,0.f,1.f,0.f,0.25f,0.f,2.f,1.f,-1.f,0.f,1.f,0.f,0.75f,0.f,2.f,1.f,0.f,0.f,1.f,0.f,0.75f,0.25f,0.f,1.f,-1.f,0.f,1.f,0.f,0.25f,0.f,2.f,1.f,0.f,0.f,1.f,0.f,0.75f,0.25f,0.f,1.f,0.f,0.f,1.f,0.f,0.25f,0.25f,-1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f,0.f,0.f,1.f,0.f,0.25f,0.25f,0.f,1.f,3.f,0.f,1.f,0.f,0.25f,1.f,-1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f,3.f,0.f,1.f,0.f,0.25f,1.f,-1.f,1.f,3.f,0.f,1.f,0.f,0.f,1.f,2.f,1.f,2.f,0.f,1.f,0.f,0.75f,0.75f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,2.f,1.f,3.f,0.f,1.f,0.f,0.75f,1.f,2.f,1.f,2.f,0.f,1.f,0.f,0.75f,0.75f,2.f,1.f,3.f,0.f,1.f,0.f,0.75f,1.f,0.f,1.f,3.f,0.f,1.f,0.f,0.25f,1.f,2.f,1.f,2.f,0.f,1.f,0.f,0.75f,0.75f,0.f,1.f,3.f,0.f,1.f,0.f,0.25f,1.f,0.f,1.f,2.f,0.f,1.f,0.f,0.25f,0.75f } },
			// wall
			{ { -1.f,-1.f,1.f,0.f,1.f,0.f,0.f,0.f,3.f,-1.f,1.f,0.f,1.f,0.f,1.f,0.f,3.f,3.f,1.f,0.f,1.f,0.f,1.f,1.f,-1.f,3.f,1.f,0.f,1.f,0.f,0.f,1.f }, { {0,2,0,2} }, { 1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1 },
			  { -1.f,-1.f,1.f,0.f,1.f,0.f,0.f,0.f,0.f,-1.f,1.f,0.f,1.f,0.f,0.25f,0.f,0.f,0.f,1.f,0.f,1.f,0.f,0.25f,0.25f,2.f,-1.f,1.f,0.f,1.f,0.f,0.75f,0.f,3.f,-1.f,1.f,0.f,1.f,0.f,1.f,0.f,3.f,3.f,1.f,0.f,1.f,0.f,1.f,1.f,2.f,-1.f,1.f,0.f,1.f,0.f,0.75f,0.f,3.f,3.f,1.f,0.f,1.f,0.f,1.f,1.f,2.f,2.f,1.f,0.f,1.f,0.f,0.75f,0.75f,-1.f,-1.f,1.f,0.f,1.f,0.f,0.f,0.f,0.f,0.f,1.f,0.f,1.f,0.f,0.25f,0.25f,0.f,3.f,1.f,0.f,1.f,0.f,0.25f,1.f,-1.f,-1.f,1.f,0.f,1.f,0.f,0.f,0.f,0.f,3.f,1.f,0.f,1.f,0.f,0.25f,1.f,-1.f,3.f,1.f,0.f,1.f,0.f,0.f,1.f,2.f,2.f,1.f,0.f,1.f,0.f,0.75f,0.75f,3.f,3.f,1.f,0.f,1.f,0.f,1.f,1.f,2.f,3.f,1.f,0.f,1.f,0.f,0.75f,1.f } },
			// gone
			{ { -1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,3.f,1.f,-1.f,0.f,1.f,0.f,1.f,0.f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,-1.f,1.f,3.f,0.f,1.f,0.f,0.f,1.f }, { {-2,4,-2,4} }, { 1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1 },
			  {  } },
			// rotated
			{ { -1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,3.f,1.f,-1.f,0.f,1.f,0.f,1.f,0.f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,-1.f,1.f,3.f,0.f,1.f,0.f,0.f,1.f }, { {0,2,0,2} }, { 0.7648421872844885,0,0.64421768723769102,0,0,1,0,0,-0.64421768723769102,0,0.7648421872844885,0,1,0,0.5,1 },
			  { 0.46517089009284973f,1.f,-1.f,0.f,1.f,0.f,0.36629271507263184f,0.f,3.f,1.f,-1.f,0.f,1.f,0.f,1.f,0.f,3.f,1.f,2.0094552040100098f,0.f,1.f,0.f,1.f,0.75236380100250244f,-1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,0.41110667586326599f,1.f,-1.f,0.f,1.f,0.f,0.35277667641639709f,0.f,-0.3548465371131897f,1.f,-0.3548465371131897f,0.f,1.f,0.f,0.16128836572170258f,0.16128836572170258f,1.7311687469482422f,1.f,0.50304561853408813f,0.f,1.f,0.f,0.68279218673706055f,0.37576138973236084f,3.f,1.f,2.0094552040100098f,0.f,1.f,0.f,1.f,0.75236380100250244f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,1.7311687469482422f,1.f,0.50304561853408813f,0.f,1.f,0.f,0.68279218673706055f,0.37576138973236084f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,1.0645395517349243f,1.f,1.0645395517349243f,0.f,1.f,0.f,0.51613491773605347f,0.51613491773605347f,1.2194058895111084f,1.f,3.f,0.f,1.f,0.f,0.5548514723777771f,1.f,-1.f,1.f,3.f,0.f,1.f,0.f,0.f,1.f,-1.f,1.f,0.3650285005569458f,0.f,1.f,0.f,0.f,0.34125712513923645f,-1.f,1.f,-1.f,0.f,1.f,0.f,0.f,0.f,-0.3548465371131897f,1.f,-0.3548465371131897f,0.f,1.f,0.f,0.16128836572170258f,0.16128836572170258f,-1.f,1.f,0.18855875730514526f,0.f,1.f,0.f,0.f,0.29713967442512512f,1.0645395517349243f,1.f,1.0645395517349243f,0.f,1.f,0.f,0.51613491773605347f,0.51613491773605347f,3.f,1.f,3.f,0.f,1.f,0.f,1.f,1.f,1.2194058895111084f,1.f,3.f,0.f,1.f,0.f,0.5548514723777771f,1.f,1.0645395517349243f,1.f,1.0645395517349243f,0.f,1.f,0.f,0.51613491773605347f,0.51613491773605347f,1.2194058895111084f,1.f,3.f,0.f,1.f,0.f,0.5548514723777771f,1.f,0.20148433744907379f,1.f,1.7914810180664062f,0.f,1.f,0.f,0.30037108063697815f,0.69787025451660156f } },
		};
		for (const FCase& C : Cases)
		{
			const auto Out = BreachTriangles(C.In, 8, {0,1,2,0,2,3}, C.Matrix, C.Holes);
			T.Require(Out.size() == C.Expected.size(), "breach triangle count equals source");
			for (size_t I = 0; I < std::min(Out.size(), C.Expected.size()); I++) T.Near(Out[I], C.Expected[I], 2e-6, "breach vertex attribute equals source");
		}
		const auto Columns = FailedColumns({15,1,3,5,7,9,11,13,17,19,21}, 8);
		T.Require(Columns.size() == 9, "breaches cap at nine sorted failed section cells");
		T.Near(Columns[0].X0, 2, 0, "first failed column x");
		T.Near(Columns[8].Z0, 4, 0, "ninth failed column z");
		return T;
	}
	FChecks WaterRules()
	{
		using namespace TCWaterCore;
		FChecks T;
		#include "World/TCWater.LightingOracle.inl"
		for (const auto& S : WaterLightOracle)
		{
			T.Near(Fresnel(S[5]), S[7], 1e-13, "r186 dielectric Fresnel attenuates direct diffuse");
			T.Near(GgxVisibility(S[1]*S[1], S[2], S[3]), S[8], 1e-10, "source correlated Smith visibility");
			T.Near(GgxDistribution(S[1]*S[1], S[4]), S[9], 1e-9, "source GGX distribution");
			T.Near(DirectBrdf(S[0], S[1], S[2], S[3], S[4], S[5], S[6]), S[10], 1e-9, "source direct BRDF and DFG multiple-scattering energy");
		}
		for (const auto& S : WaterVogelOracle)
		{
			double X, Y; VogelOffset((int)S[0], S[1], X, Y);
			T.Near(X, S[2], 1e-13, "source five-tap Vogel disk x");
			T.Near(Y, S[3], 1e-13, "source five-tap Vogel disk y");
		}
		for (const auto& S : WaterRotationOracle) T.Near(ShadowRotation(S[0], S[1]), S[2], 1e-12, "source per-pixel PCF rotation");
		T.Near(BilinearShadow(0, 1, 1, 1, .25, .75), .8125, 0, "comparison filtering interpolates visibility, not raw depth");
		T.Near(BilinearShadow(0, 1, 1, 0, .5, .5), .5, 0, "source bilinear comparison midpoint");
		T.Near(BilinearShadow(1, 1, 1, 1, .37, .91), 1, 0, "unoccluded shadow samples conserve direct light");
		// Independent native atlas cases around the biased comparison boundary.
		// A wrong bias sign incorrectly lights the first two occluded samples.
		const double AtlasCases[][4] = { {.5,.125,.625,0}, {.5,.125,.5625,0}, {.5,.125,.75,1}, {.75,0,.375,1} };
		for (const auto& S : AtlasCases)
			T.Near(ShadowReceiverDepth(S[0],S[1]) < S[2] ? 1 : 0,S[3],0,"native shadow receiver bias and strict depth comparison");
		for (const int Resolution : {917,987,2048})
		{
			const double SourceTexel=.03125, CascadeRadius=31.0, SourceRadius=4.0;
			const double NativeRadius=ShadowAtlasRadius(SourceRadius,SourceTexel,Resolution,CascadeRadius);
			T.Near(NativeRadius*(2*CascadeRadius/Resolution),SourceRadius*SourceTexel,1e-14,"shadow disk conversion preserves the source world-space radius after atlas rounding");
		}
		T.Near(ShadowAtlasRadius(4,.03125,917,0),4,0,"missing native cascade keeps the source fallback radius");
		// Apply the native common haze to the complete translucent composite. It must
		// recover source per-surface fog for every colour, depth pair and alpha here.
		for (double A : { 0.0, .6, .86, 1.0 }) for (double Fw : { 0.0, .1, .65 })
			for (double Fb : { 0.0, .2, .8, .99 })
			{
				const double C=.08, Bed=.12, Haze=.5;
				const double Native=(A*HazeCompensated(C,Haze,Fw,Fb)+(1-A)*Bed)*(1-Fb)+Haze*Fb;
				const double Source=A*(C*(1-Fw)+Haze*Fw)+(1-A)*(Bed*(1-Fb)+Haze*Fb);
				T.Near(Native,Source,2e-14,"water fog depth conserves the complete source alpha composite");
			}
		T.Near(HazeCompensated(.08,.5,1,1),.08,0,"fully hazed bed and water need no inversion");
		T.Near(HazeCompensated(.01,.5,.1,.8),-1.705,1e-14,"shadowed water requires signed radiance until the common haze blend");
		T.Near(PresentationTime(1017.25, 8.f, false), 17.25, 0, "High water clock wraps at 1000 seconds");
		T.Near(PresentationTime(99, 8.f, true), 8, 0, "Low water clock holds its previous value");
		T.Near(PresentationTime(99, 0.f, true), 0, 0, "fresh Low water starts at zero");
		T.Near(PresentationTime(99, 8.f, true, 1037.5), 37.5, 0, "matched capture override wraps on Low too");
		T.Near(PresentationTime(99, 8.f, false, std::numeric_limits<double>::quiet_NaN()), 99, 0, "nonfinite override keeps the normal clock");
		std::vector<std::string> Grid{ ".......", "..WFW..", "..W=W..", "..WWW..", "......." };
		FWater Water;
		Water.Char = [&](int C) { return (uint8_t)Grid[(size_t)C / 7][(size_t)C % 7]; };
		Water.GroundHeight = [](double, double) { return 5.0; };
		T.Require(Water.Create(7, 5, [](int) { return 2; }, [](int) { return true; }), "wet cells create one water mesh");
		const int Samples[][6] = { { 0, 0, 1, 13, 0, 0 }, { 2, 1, 180, 122, 39, 19 }, { 3, 1, 212, 143, 80, 39 },
			{ 3, 2, 251, 166, 23, 80 }, { 4, 3, 132, 99, 0, 7 }, { 6, 4, 0, 8, 0, 0 } };
		for (const auto& S : Samples) for (int K = 0; K < 4; K++)
			T.Require(Water.Mask[((size_t)(S[1] * 4 + 2) * 28 + S[0] * 4 + 2) * 4 + K] == S[K + 2], "shore/depth/ford/bridge blur bytes");
		auto Fnv = [](const std::vector<uint8_t>& Bytes) { uint64_t H = 14695981039346656037ull; for (const uint8_t B : Bytes) H = (H ^ B) * 1099511628211ull; return H; };
		T.Require(Fnv(Water.Mask) == 0x330f6544df79c011ull, "complete source shoreline mask");
		T.Require(Fnv(NoiseTexture()) == 0xe313c47455b37e6aull, "complete source ripple/foam texture");
		const double Flow[] = { 0.68931359052658081, 0.68931359052658081, 0.68880760669708252, 0.6898428201675415, 0.68880760669708252, 0.6898428201675415, 0.68931359052658081, 0.68931359052658081 };
		for (int I = 0; I < 8; I++) T.Near(Water.Flow[(size_t)20 + I], Flow[I], 1e-7, "smoothed flow on water corners");
		T.Near(Water.WaterAt(7, 5), 5.06, 1e-7, "frozen level plus water lift");
		Grid[0][3] = 'F'; Water.Changed(3, 'F', true); Water.Tick();
		T.Near(Water.WaterAt(7, 1), 5.06, 1e-7, "a flooded crater retains the original line");
		// A late World discovery must not read a hidden level while freezing the water line.
		std::vector<uint8_t> Known(35, 0); Known[8] = 1;
		Grid.assign(5, "......."); Grid[1][1] = 'W';
		FWater Late; int HiddenReads = 0;
		Late.Char = Water.Char; Late.Unknown = [&](int C) { return !Known[(size_t)C]; };
		Late.Create(7, 5, [&](int C) { if (!Known[(size_t)C]) HiddenReads++; return C == 8 ? 2 : 3; }, [&](int C) { return Known[(size_t)C] != 0; });
		T.Require(HiddenReads == 0, "unknown levels never read");
		Known[26] = 1; Grid[3][5] = 'W'; Late.Changed(26, 'W', true); Late.Tick();
		T.Near(Late.WaterAt(11, 7), .06, 1e-7, "later discoveries retain creation-time zero placeholders");
		return T;
	}

	FChecks RailwayRules()
	{
		using namespace TCRailCore;
		FChecks T;
		FModel Rail; Rail.Reset(8); Rail.Cell_(18, 255, true, 4);
		const auto Height = [](double X, double Z) { return X * .1 - Z * .2; };
		const auto NoCentre = [](int, double&, double&) { return false; };
		Rail.Frame(.1, Height, NoCentre);
		T.Require(Rail.BallastM.size() == 8 && Rail.RailsM.size() == 16 && Rail.TiesM.size() == 16, "all eight direction bits draw their own halves, rails and sleepers");
		T.Require(Rail.BuiltStations.empty() && Rail.Waiting == 1, "station waits for its delivered region centre");
		const double Ballast[][16] = {
			{ 0, -1.595945564811438e-17, -2.2999999523162842, 0, -0.013930520974099636, 0.13930520415306091, -2.9143353486511685e-18, 0, 1.0447890758514404, 0.10447890311479568, 0, 0, 5.5, -0.40000000596046448, 5, 1 },
			{ 1.6263456344604492, 0, -1.6263456344604492, 0, 0.0069825653918087482, 0.13965131342411041, 0.0069825653918087482, 0, 1.0327765941619873, -0.10327766090631485, 1.0327765941619873, 0, 5.5, -0.5, 5.5, 1 },
			{ 2.2999999523162842, 0, 0, 0, 0, 0.13728129863739014, 0.027456259354948997, 0, 0, -0.20592194795608521, 1.0296096801757812, 0, 5, -0.55000001192092896, 5.5, 1 },
			{ 1.6263456344604492, 0, 1.6263456344604492, 0, -0.020542871206998825, 0.1369524747133255, 0.020542871206998825, 0, -1.0128177404403687, -0.30384531617164612, 1.0128177404403687, 0, 4.5, -0.60000002384185791, 5.5, 1 },
			{ 0, 1.595945564811438e-17, 2.2999999523162842, 0, -0.013930520974099636, 0.13930520415306091, 9.7144515068291504e-19, 0, -1.0447890758514404, -0.10447890311479568, 0, 0, 4.5, -0.5, 5, 1 },
			{ -1.6263456344604492, 0, 1.6263456344604492, 0, 0.0069825653918087482, 0.13965131342411041, 0.0069825653918087482, 0, -1.0327765941619873, 0.10327766090631485, -1.0327765941619873, 0, 4.5, -0.40000000596046448, 4.5, 1 },
			{ -2.2999999523162842, 0, -2.8166876285969881e-16, 0, -3.3624219816032876e-18, 0.13728129863739014, 0.027456259354948997, 0, 1.2609082007082264e-16, 0.20592194795608521, -1.0296096801757812, 0, 5, -0.34999999403953552, 4.5, 1 },
			{ -1.6263456344604492, 0, -1.6263456344604492, 0, -0.020542871206998825, 0.1369524747133255, 0.020542871206998825, 0, 1.0128177404403687, 0.30384531617164612, -1.0128177404403687, 0, 5.5, -0.30000001192092896, 4.5, 1 },
		};
		for (int I = 0; I < 8; I++) for (int K = 0; K < 16; K++) T.Near(Rail.BallastM[(size_t)I].E[K], Ballast[I][K], 2e-6, "source track scale, slope, yaw and lift");
		const auto Centre = [](int R, double& X, double& Z) { X = 6; Z = 4; return R == 4; };
		T.Require(!Rail.Frame(.2, Height, Centre), "rebuild throttle retains half-second window");
		T.Require(Rail.Frame(.3, Height, Centre), "station is retried at half a second");
		T.Require(Rail.BuiltStations.size() == 1 && Rail.BuiltStations[0].Parts.size() == 13, "station platform, house, canopy and water tower");
		if (!Rail.BuiltStations.empty()) T.Near(Rail.BuiltStations[0].Yaw, 4.7123889803846897, 1e-12, "station faces away from known town");
		Rail.Cell_(18, 5, true, 4); Rail.Frame(.5, Height, Centre);
		T.Require(Rail.BallastM.size() == 2 && Rail.BuiltStations.size() == 1 && Rail.Rebuilds == 2, "direction change replaces track without duplicating a station");
		Rail.Cell_(18, 0, false, 0); Rail.Frame(.5, Height, Centre);
		T.Require(Rail.BallastM.size() == 2 && Rail.Rebuilds == 2, "missing metadata preserves explored track as the source does");
		return T;
	}

}

#if defined(TC_TERRAIN_CONTRACT_CORE_ONLY)
int main()
{
	int Failures = 0;
	for (const auto& Result : { TCTerrainFormulas::FogSamplingRules(), TCTerrainFormulas::ReliefRules(), TCTerrainFormulas::PaintRules(), TCTerrainFormulas::DecalRules(), TCTerrainFormulas::StructureRules(), TCTerrainFormulas::BreachRules(), TCTerrainFormulas::WaterRules(), TCTerrainFormulas::RailwayRules() })
		for (const auto& Error : Result.Errors) { std::fprintf(stderr, "%s\n", Error.c_str()); Failures++; }
	std::printf("terrain reference formulas: 8 checks, %d failures\n", Failures);
	return Failures ? 1 : 0;
}
#else
#include "CoreMinimal.h"
#include "World/TCTerrain.h"
#include "Net/TCMatchState.h"
#include "Misc/AutomationTest.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "HAL/FileManager.h"
#include "HAL/PlatformMisc.h"
#include "Misc/Compression.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "World/TCRelief.h"
#include <cmath>

#include "Engine/StaticMesh.h"
#include "StaticMeshResources.h"
#include "Client/TCModelLibrary.h"
#include "Engine/Texture2D.h"
#include "Engine/World.h"
#include "Engine/Engine.h"
#include "UObject/UObjectIterator.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "ProceduralMeshComponent.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/Material.h"

#if WITH_DEV_AUTOMATION_TESTS

namespace TCTerrainContract
{
	int32 LevelOfChar(TCHAR Ch) { return Ch >= 'a' ? 96 - (int32)Ch : (int32)Ch - '0'; } // shared/sim.js levelOf
	TSharedPtr<FJsonObject> ReadGz(FAutomationTestBase& T, const FString& Path)
	{
		TArray<uint8> Gz;
		if (!FFileHelper::LoadFileToArray(Gz, *Path) || Gz.Num() < 18) { T.AddError(FString::Printf(TEXT("cannot read %s"), *Path)); return nullptr; }
		const uint32 Size = (uint32)Gz[Gz.Num() - 4] | (uint32)Gz[Gz.Num() - 3] << 8 | (uint32)Gz[Gz.Num() - 2] << 16 | (uint32)Gz[Gz.Num() - 1] << 24;
		TArray<uint8> Raw;
		Raw.SetNumUninitialized((int64)Size + 1);
		if (!FCompression::UncompressMemory(NAME_Gzip, Raw.GetData(), Size, Gz.GetData(), Gz.Num())) { T.AddError(FString::Printf(TEXT("cannot gunzip %s"), *Path)); return nullptr; }
		Raw[Size] = 0;
		TSharedPtr<FJsonObject> Root;
		if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(FString(UTF8_TO_TCHAR((const ANSICHAR*)Raw.GetData()))), Root) || !Root.IsValid()) { T.AddError(FString::Printf(TEXT("cannot parse %s"), *Path)); return nullptr; }
		return Root;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFogSamplingContract, "ThreeCrossroads.Terrain.FogSampling", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCFogSamplingContract::RunTest(const FString& Parameters)
{
	const auto Result = TCTerrainFormulas::FogSamplingRules();
	for (const auto& Error : Result.Errors) AddError(UTF8_TO_TCHAR(Error.c_str()));
	return Result.Errors.empty();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFogOverlayContract, "ThreeCrossroads.Terrain.FogOverlay", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCFogOverlayContract::RunTest(const FString& Parameters)
{
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false);
	if (!TestNotNull(TEXT("isolated overlay world"), World)) return false;
	FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
	Context.SetCurrentWorld(World);
	FTCMatchState State;
	State.Map.W = State.Map.H = 8; State.Map.bWorld = true;
	State.Map.Chars.Init('.', 64); State.Map.Levels.Init(0, 64); State.Map.Known.Init(false, 64);
	State.Map.Known[27] = true;
	State.FogVisible.Init(false, 64); State.FogExplored.Init(false, 64);
	State.FogVisible[27] = State.FogExplored[27] = true;
	ATCTerrain* Terrain = World->SpawnActor<ATCTerrain>();
	Terrain->ResetFrom(State);
	FTCFogFrontier Fog;
	Fog.Attach(World); Fog.Start(State);
	const FTCFogFrontier* Previous = TCFogLook::Active();
	TCFogLook::SetActive(&Fog); Fog.Frame(0, false, State);
	if (UTexture2D* Alpha = Fog.AlphaTexture())
	{
		TestEqual(TEXT("raw alpha is linearly filtered"), Alpha->Filter, TF_Bilinear);
		TestFalse(TEXT("alpha is data, no sRGB conversion"), Alpha->SRGB);
		TestEqual(TEXT("outside vision clamps to the edge"), Alpha->AddressX, TA_Clamp);
		TestEqual(TEXT("outside vision clamps north/south"), Alpha->AddressY, TA_Clamp);
	}
	else AddError(TEXT("raw alpha texture missing"));
	TestEqual(TEXT("every unknown cell has a placeholder"), Fog.UnknownCovered(), 63);
	if (UProceduralMeshComponent* Underlay = Fog.UnknownGroundForTests())
		TestTrue(TEXT("filtered frontier keeps the public ground paint beneath unknown pixels"), Underlay->GetMaterial(0) == Terrain->GroundMaterialInstance());
	else AddError(TEXT("public placeholder ground missing"));
	UProceduralMeshComponent* Overlay = Fog.UnknownOverlayForTests();
	if (TestNotNull(TEXT("fog also covers unknown water after translucency"), Overlay))
	{
		TestEqual(TEXT("fog draws after water priority -1"), Overlay->TranslucencySortPriority, 1);
		TestEqual(TEXT("overlay is visual only"), Overlay->GetCollisionEnabled(), ECollisionEnabled::NoCollision);
		const FProcMeshSection* Section = Overlay->GetProcMeshSection(0);
		if (TestNotNull(TEXT("unknown cells have fog triangles"), Section)) TestEqual(TEXT("eight rows and the split known row"), Section->ProcIndexBuffer.Num(), 9 * 6);
		if (UMaterialInstanceDynamic* Mid = Cast<UMaterialInstanceDynamic>(Overlay->GetMaterial(0)))
		{
			UTexture* Bound = nullptr;
			TestTrue(TEXT("overlay binds raw alpha"), Mid->GetTextureParameterValue(FMaterialParameterInfo(TEXT("FogAlpha")), Bound));
			TestTrue(TEXT("overlay reads the actual recipient alpha"), Bound == Fog.AlphaTexture());
		}
		else AddError(TEXT("reference fog overlay material missing"));
	}
	// Priority orders only primitives in the same translucency pass. A later water pass bypasses fog.
	for (const TCHAR* Path : { FTCFogOverlay::MaterialPath, FTCWater::MaterialPath, TEXT("/Game/Terrain/Materials/M_TCRubbleDecal.M_TCRubbleDecal") })
		if (UMaterialInterface* Material = LoadObject<UMaterialInterface>(nullptr, Path))
			TestEqual(*FString::Printf(TEXT("%s composites before tone mapping with fog"), Path), Material->GetMaterial()->TranslucencyPass, MTP_BeforeDOF);
		else AddError(FString::Printf(TEXT("fog pass material missing: %s"), Path));
	if (UMaterialInterface* Material = LoadObject<UMaterialInterface>(nullptr, FTCWater::MaterialPath))
	{
		TestTrue(TEXT("source Standard water shading uses an unlit carrier for signed fog compensation"), Material->GetMaterial()->GetShadingModels().HasShadingModel(MSM_Unlit));
		TestTrue(TEXT("negative intermediate water radiance survives until the common bed haze"), Material->GetMaterial()->bAllowNegativeEmissiveColor != 0);
	}
	TestEqual(TEXT("raw lookup uses current alpha, not the brightness multiplier"), TCFogLook::AlphaByte(State, false, 27), (uint8)Fog.AlphaNow(27));
	TestEqual(TEXT("lifted raw alpha is clear"), TCFogLook::AlphaByte(State, true, 27), (uint8)0);
	// Test the actual scenery adapter. Replacing its shader without replacing its old multiplier
	// texture would make every clear spectator tree opaque fog grey.
	TSharedPtr<FJsonObject> Start;
	FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TEXT("{\"map\":{\"w\":8,\"h\":8,\"world\":true,\"rows\":[\"HHHHHHHH\",\"HHHHHHHH\",\"HHHHHHHH\",\"HHHHHHHH\",\"HHHHHHHH\",\"HHHHHHHH\",\"HHHHHHHH\",\"HHHHHHHH\"]}}")), Start);
	State.Map.Chars.Init('H', 64);
	ATCTerrain* Ground = World->SpawnActor<ATCTerrain>(); Ground->ResetFrom(State); Ground->Flush(State);
	FTCWorldObjects Objects; FTCWorldObjectsView View;
	if (TestTrue(TEXT("scenery fixture starts"), Start.IsValid() && Objects.ApplyStart(*Start, State)) && TestTrue(TEXT("scenery fixture attaches"), View.Attach(World)))
	{
		Objects.Update(*Ground, nullptr); View.Draw(Objects, State, Ground);
		int Bound = 0;
		for (TObjectIterator<UInstancedStaticMeshComponent> It; It; ++It)
			if (It->GetWorld() == World)
				for (int M = 0; M < It->GetNumMaterials(); M++)
					if (UMaterialInterface* Material = It->GetMaterial(M))
					{
						UTexture* Texture = nullptr;
						if (Material->GetTextureParameterValue(FMaterialParameterInfo(TEXT("FogMap")), Texture))
						{
							Bound++;
							TestTrue(TEXT("scenery shares the live faded raw-alpha texture"), Texture == Fog.AlphaTexture());
						}
					}
		TestTrue(TEXT("at least one real scenery material is checked"), Bound > 0);
	}
	View.Detach();
	Fog.Detach(); TCFogLook::SetActive(Previous); GEngine->DestroyWorldContext(World); World->DestroyWorld(false);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainRampRulesTest, "ThreeCrossroads.Terrain.RampRules", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainRampRulesTest::RunTest(const FString& Parameters)
{
	const auto Result = TCTerrainFormulas::ReliefRules();
	for (const auto& Error : Result.Errors) AddError(UTF8_TO_TCHAR(Error.c_str()));
	return Result.Errors.empty();
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainPaintRulesTest, "ThreeCrossroads.Terrain.PaintRules", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainPaintRulesTest::RunTest(const FString& Parameters)
{
	const auto Result = TCTerrainFormulas::PaintRules();
	for (const auto& Error : Result.Errors) AddError(UTF8_TO_TCHAR(Error.c_str()));
	return Result.Errors.empty();
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainDecalRulesTest, "ThreeCrossroads.Terrain.DecalRules", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainDecalRulesTest::RunTest(const FString& Parameters)
{
	const auto Result = TCTerrainFormulas::DecalRules();
	for (const auto& Error : Result.Errors) AddError(UTF8_TO_TCHAR(Error.c_str()));
	return Result.Errors.empty();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainStructureRulesTest, "ThreeCrossroads.Terrain.StructureRules", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainStructureRulesTest::RunTest(const FString& Parameters)
{
	const auto Result = TCTerrainFormulas::StructureRules();
	for (const auto& Error : Result.Errors) AddError(UTF8_TO_TCHAR(Error.c_str()));
	return Result.Errors.empty();
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainBreachRulesTest, "ThreeCrossroads.Terrain.BuildingBreaches", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainBreachRulesTest::RunTest(const FString& Parameters)
{
	const auto Result = TCTerrainFormulas::BreachRules();
	for (const auto& Error : Result.Errors) AddError(UTF8_TO_TCHAR(Error.c_str()));
	return Result.Errors.empty();
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainWaterRulesTest, "ThreeCrossroads.Terrain.WaterRules", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainWaterRulesTest::RunTest(const FString& Parameters)
{
	const auto Result = TCTerrainFormulas::WaterRules();
	for (const auto& Error : Result.Errors) AddError(UTF8_TO_TCHAR(Error.c_str()));
	return Result.Errors.empty();
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainRailwayRulesTest, "ThreeCrossroads.Terrain.RailwayRules", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainRailwayRulesTest::RunTest(const FString& Parameters)
{
	const auto Result = TCTerrainFormulas::RailwayRules();
	for (const auto& Error : Result.Errors) AddError(UTF8_TO_TCHAR(Error.c_str()));
	return Result.Errors.empty();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainWaterDiscoveryTest, "ThreeCrossroads.Terrain.WaterDiscovery", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainWaterDiscoveryTest::RunTest(const FString& Parameters)
{
	FTCMatchState State;
	State.Map.W = 7; State.Map.H = 5; State.Map.bWorld = true;
	State.Map.Chars.Init('.', 35); State.Map.Levels.Init(0, 35); State.Map.Known.Init(false, 35);
	State.Map.Known[8] = true; State.Map.Chars[8] = 'W'; State.Map.Levels[8] = 2;
	FTCWater Water;
	Water.ResetFrom(State);
	TestTrue(TEXT("initial received line is level two plus lift"), FMath::IsNearlyEqual(Water.WaterAt(3, 3), 5.06f));
	State.Map.Known[26] = true; State.Map.Chars[26] = 'W'; State.Map.Levels[26] = 3;
	Water.MarkCells(State, TSet<int32>{ 26 }, TSet<int32>{ 26 });
	Water.Flush(State, 0, false);
	// The unmodified water.js emits Float32 0.05999999865889549 here, even though the delivered level is 3.
	TestTrue(TEXT("late World cell retains the creation-time placeholder line"), FMath::IsNearlyEqual(Water.WaterAt(11, 7), .06f));
	TestTrue(TEXT("a new body does not move the original body's line"), FMath::IsNearlyEqual(Water.WaterAt(3, 3), 5.06f));
	Water.SetPresentationTime(1037.5);
	Water.Flush(State, 99, false);
	TestEqual(TEXT("matched renderer clock is presentation only"), Water.TimeSent(), 37.5f);
	Water.ResetFrom(State);
	TestEqual(TEXT("new map clears the last sent clock before a Low frame"), Water.TimeSent(), 0.f);
	Water.SetPresentationTime(-1);
	Water.Detach();
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainBuildingMeshTest, "ThreeCrossroads.Terrain.BuildingMesh", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainBuildingMeshTest::RunTest(const FString& Parameters)
{
	UStaticMesh* Source = LoadObject<UStaticMesh>(nullptr, TEXT("/Game/Models/Meshes/hq/SM_hq_0_body.SM_hq_0_body"));
	if (!TestNotNull(TEXT("original source HQ mesh"), Source)) return false;
	TestTrue(TEXT("cooked building retains buffers for source triangle clipping"), Source->bAllowCPUAccess);
	const FTransform Root(FQuat::Identity, FVector(500, 500, 0));
	bool Empty = false;
	UStaticMesh* Mesh = TCStructures::BreachMesh(Source, {{4,6,4,6}}, Root, GetTransientPackage(), Empty);
	if (!TestNotNull(TEXT("source HQ clipped at a delivered two-metre section"), Mesh)) return false;
	TestFalse(TEXT("one breach leaves the other sections standing"), Empty);
	TestEqual(TEXT("source material slots retained"), Mesh->GetStaticMaterials().Num(), Source->GetStaticMaterials().Num());
	const auto& Lod = Mesh->GetRenderData()->LODResources[0];
	const auto& Original = Source->GetRenderData()->LODResources[0];
	// UE 5.8's fast builder pads FStaticMeshBuildVertex UVs to eight channels. Every source channel must survive.
	TestTrue(TEXT("all source UV channels retained"), Lod.VertexBuffers.StaticMeshVertexBuffer.GetNumTexCoords() >= Original.VertexBuffers.StaticMeshVertexBuffer.GetNumTexCoords());
	TestEqual(TEXT("source UV precision retained"), Lod.VertexBuffers.StaticMeshVertexBuffer.GetUseFullPrecisionUVs(), Original.VertexBuffers.StaticMeshVertexBuffer.GetUseFullPrecisionUVs());
	UStaticMesh* Uncut = TCStructures::BreachMesh(Source, {{1000,1002,1000,1002}}, Root, GetTransientPackage(), Empty);
	if (TestNotNull(TEXT("nonintersecting section leaves source attributes intact"), Uncut))
	{
		const auto& Copy = Uncut->GetRenderData()->LODResources[0];
		TestEqual(TEXT("nonintersecting cut keeps every triangle"), Copy.IndexBuffer.GetNumIndices(), Original.IndexBuffer.GetNumIndices());
		int32 BadUVs = 0;
		for (uint32 I = 0; I < FMath::Min(Copy.IndexBuffer.GetNumIndices(), Original.IndexBuffer.GetNumIndices()); I++)
			for (uint32 C = 0; C < Original.VertexBuffers.StaticMeshVertexBuffer.GetNumTexCoords(); C++)
				if (!Copy.VertexBuffers.StaticMeshVertexBuffer.GetVertexUV(Copy.IndexBuffer.GetIndex(I), C).Equals(
					Original.VertexBuffers.StaticMeshVertexBuffer.GetVertexUV(Original.IndexBuffer.GetIndex(I), C), 1e-6f)) BadUVs++;
		TestEqual(TEXT("every source shader UV value survives the engine build"), BadUVs, 0);
	}
	int32 Intruding = 0;
	for (uint32 I = 0; I < Lod.IndexBuffer.GetNumIndices(); I += 3)
	{
		FVector Centre = FVector::ZeroVector;
		for (uint32 K = 0; K < 3; K++) Centre += Root.TransformPosition(FVector(Lod.VertexBuffers.PositionVertexBuffer.VertexPosition(Lod.IndexBuffer.GetIndex(I + K)))) / 3.0;
		if (Centre.X > 400.001 && Centre.X < 599.999 && Centre.Y > 400.001 && Centre.Y < 599.999) Intruding++;
	}
	TestEqual(TEXT("no drawn triangle remains inside the failed section"), Intruding, 0);
	TestNull(TEXT("fully clipped model has no placeholder mesh"), TCStructures::BreachMesh(Source, {{-100,100,-100,100}}, Root, GetTransientPackage(), Empty));
	TestTrue(TEXT("complete removal is distinct from a missing source asset"), Empty);
	FTCMatchState State; State.Map.W = 8;
	FTCCellSection Section; Section.StructureId = TEXT("building:17"); Section.State = TEXT("failed"); State.Sections.Add(18, Section);
	FTCModelPart Part; Part.Name = TEXT("body"); Part.Mesh = FSoftObjectPath(Source); FTCModelPart Out;
	TCStructures::FBuildingBreaches Cache;
	TestTrue(TEXT("live building resolves clipped source"), Cache.PartFor(Part, 17, State, Root, GetTransientPackage(), Out));
	TestTrue(TEXT("failed section changes the drawn mesh"), Out.Mesh != Part.Mesh);
	const FSoftObjectPath First = Out.Mesh;
	Cache.PartFor(Part, 17, State, Root, GetTransientPackage(), Out);
	TestTrue(TEXT("unchanged section reuses its clipped geometry"), Out.Mesh == First);
	State.Sections[18].State = TEXT("standing"); State.Sections[18].Hp = 0; State.Sections[18].bHasHp = true;
	Cache.PartFor(Part, 17, State, Root, GetTransientPackage(), Out);
	TestTrue(TEXT("only source state failed clips; repair restores original"), Out.Mesh == Part.Mesh);
	Cache.Reset();
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainHeightOracle33Test, "ThreeCrossroads.Terrain.HeightOracle33", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainHeightOracle33Test::RunTest(const FString& Parameters)
{
	using namespace TCTerrainContract;
	const FString FixtureDir = FPaths::Combine(FPaths::ProjectDir(), TEXT("Tools/relief/fixtures"));
	const TSharedPtr<FJsonObject> Heights = ReadGz(*this, FPaths::Combine(FixtureDir, TEXT("heights-oracle-33-maps.json.gz")));
	const TSharedPtr<FJsonObject> Relief = ReadGz(*this, FPaths::Combine(FixtureDir, TEXT("relief-oracle.json.gz")));
	if (!Heights || !Relief) return false;
	TestEqual(TEXT("height oracle format"), Heights->GetStringField(TEXT("format")), FString(TEXT("tc-heights-oracle-v1")));
	TMap<FString, TSharedPtr<FJsonObject>> Cases;
	for (const TSharedPtr<FJsonValue>& C : Relief->GetArrayField(TEXT("cases"))) Cases.Add(C->AsObject()->GetStringField(TEXT("name")), C->AsObject());

	// one map's gameplay heights: per cell centre levelAt (the integer level) and heightAt (metres); points (x, z, level)
	struct FMapOracle { FString Name; int32 W = 0, H = 0; TArray<double> CentreLevel, CentreHeight; TArray<FVector3d> Points; };
	struct FTally { int64 Centres = 0, Plain = 0, PlainBad = 0, Bank = 0, Shoved = 0, Trench = 0, TrenchBad = 0, Road = 0, RoadBad = 0, Wet = 0, WetBad = 0, Ford = 0, FordBad = 0, Bridge = 0, BridgeBad = 0,
		LevelBad = 0, CentreSimBad = 0, ShovedRefBad = 0, Samples = 0, SampleBad = 0, Outside = 0, CliffPairs = 0, CliffBad = 0, CliffEdgesDrawn = 0; double PlainMax = 0, ShovedMax = 0, ShovedRefMax = 0, CliffMinRatio = 1e9; };
	int32 Errors = 0;
	auto Fail = [&](const FString& S) { if (Errors++ < 20) AddError(S); };
	auto CheckMap = [&](const FMapOracle& O, FTally& T, int32& Maps)
	{
		const FString& Name = O.Name;
		const TSharedPtr<FJsonObject>* Found = Cases.Find(Name);
		if (!Found) { Fail(FString::Printf(TEXT("%s: no relief inputs in the fixture"), *Name)); return; }
		const FJsonObject& C = **Found;
		const int32 W = O.W, H = O.H, N = W * H;
		if (W != (int32)C.GetNumberField(TEXT("w")) || H != (int32)C.GetNumberField(TEXT("h"))) { Fail(Name + TEXT(": sizes differ")); return; }
		const FString Lv = C.GetStringField(TEXT("levels")), Gr = C.GetStringField(TEXT("grid")), Rd = C.GetStringField(TEXT("roads")), Fz = C.GetStringField(TEXT("frozen"));
		TCRelief::FRelief R;
		R.Reset(W, H, false, 32, true); // ATCTerrain: ChunkCells 32, drawn picks (an authored map is not Low)
		for (int32 I = 0; I < N; I++)
		{
			R.InLevel[(size_t)I] = (int8_t)LevelOfChar(Lv[I]);
			R.InGrid[(size_t)I] = (uint8_t)Gr[I];
			R.InRoad[(size_t)I] = Rd[I] == '1';
			R.InKnown[(size_t)I] = 1;
			R.InFrozen[(size_t)I] = (int8_t)LevelOfChar(Fz[I]);
		}
		R.Build();
		Maps++;
		T.CliffEdgesDrawn += R.Stats.CliffEdges;
		auto Wet = [&](int32 X, int32 Y) { return X >= 0 && Y >= 0 && X < W && Y < H && (Gr[Y * W + X] == 'W' || Gr[Y * W + X] == 'F' || Gr[Y * W + X] == '='); };
		// cells a crater lip shoved: the drawn centre moves with the lip (as relief.js draws it); the reference's query
		// mode (relief.js hAt, which reads the unshoved lattice) is built too, to check the contract there
		TUniquePtr<TCRelief::FRelief> RefMode;
		for (int32 I = 0; I < N && !RefMode; I++) if (R.IsShoved(I))
		{
			RefMode = MakeUnique<TCRelief::FRelief>();
			RefMode->Reset(W, H, false, 0, false);
			RefMode->InLevel = R.InLevel; RefMode->InGrid = R.InGrid; RefMode->InRoad = R.InRoad; RefMode->InKnown = R.InKnown; RefMode->InFrozen = R.InFrozen;
			RefMode->Build();
		}
		if (O.CentreLevel.Num() != N) { Fail(Name + TEXT(": centre count")); return; }
		for (int32 Y = 0; Y < H; Y++) for (int32 X = 0; X < W; X++)
		{
			const int32 I = Y * W + X;
			const double SimH = O.CentreHeight[I], SimL = O.CentreLevel[I];
			const int32 Level = R.InLevel[(size_t)I];
			T.Centres++;
			if (Level != (int32)SimL) { T.LevelBad++; Fail(FString::Printf(TEXT("%s %d,%d: client level %d, levelAt %g"), *Name, X, Y, Level, SimL)); }
			const double Nominal = Level * TCRelief::LevelHeight;
			if (FMath::Abs(SimH - Nominal) > 1e-9) { T.CentreSimBad++; Fail(FString::Printf(TEXT("%s %d,%d: heightAt %g, level x 2.5 %g"), *Name, X, Y, SimH, Nominal)); }
			const double Cx = (X + 0.5) * TCRelief::Cell, Cz = (Y + 0.5) * TCRelief::Cell, Got = R.HAt(Cx, Cz);
			const TCHAR Ch = Gr[I];
			const bool bRoad = Rd[I] == '1';
			bool bBank = false;
			for (int32 Dy = -1; Dy <= 1; Dy++) for (int32 Dx = -1; Dx <= 1; Dx++) if (Wet(X + Dx, Y + Dy)) bBank = true;
			if (Ch == 'W' || Ch == 'F')
			{
				// the body's line: a frozen level x 2.5 + 0.06 m, never above this cell's own frozen level
				const double Line = R.WaterAt(Cx, Cz), Steps = (Line - TCRelief::WaterLift) / TCRelief::LevelHeight;
				const bool bLine = std::isfinite(Line) && FMath::Abs(Steps - FMath::RoundToDouble(Steps)) < 1e-4 && Line <= R.InFrozen[(size_t)I] * TCRelief::LevelHeight + TCRelief::WaterLift + 1e-4;
				if (Ch == 'F')
				{
					T.Ford++;
					const double Want = FMath::Min(Nominal - (bRoad ? 0.1 : 0.0), Line - 0.15);
					if (!bLine || FMath::Abs(Got - Want) > 1e-3) { T.FordBad++; Fail(FString::Printf(TEXT("%s ford %d,%d at %.4f, line %.4f"), *Name, X, Y, Got, Line)); }
				}
				else
				{
					T.Wet++;
					if (!bLine || Got > Line + 1e-4) { T.WetBad++; Fail(FString::Printf(TEXT("%s water %d,%d bed %.4f, line %.4f"), *Name, X, Y, Got, Line)); }
				}
				continue;
			}
			if (Ch == '=')
			{
				T.Bridge++;
				if (FMath::Abs(Got - (Nominal - (bRoad ? 0.1 : 0.0))) > 0.1) { T.BridgeBad++; Fail(FString::Printf(TEXT("%s bridge %d,%d at %.3f, deck %.3f"), *Name, X, Y, Got, Nominal)); }
				continue;
			}
			if (bBank) { T.Bank++; continue; }
			if (Ch == 'T')
			{
				T.Trench++;
				if (FMath::Abs(Got - (Nominal - 0.9)) > 0.1) { T.TrenchBad++; Fail(FString::Printf(TEXT("%s trench %d,%d at %.3f, level %.2f"), *Name, X, Y, Got, Nominal)); }
				continue;
			}
			if (bRoad)
			{
				T.Road++;
				if (FMath::Abs(Got - (Nominal - 0.1)) > 0.1 + 1e-9) { T.RoadBad++; Fail(FString::Printf(TEXT("%s road %d,%d at %.3f, level %.2f"), *Name, X, Y, Got, Nominal)); }
				continue;
			}
			if (R.IsShoved(I))
			{
				T.Shoved++; T.ShovedMax = FMath::Max(T.ShovedMax, FMath::Abs(Got - Nominal));
				const double RefD = RefMode ? FMath::Abs(RefMode->HAt(Cx, Cz) - Nominal) : 1e9;
				T.ShovedRefMax = FMath::Max(T.ShovedRefMax, RefD);
				if (RefD > 0.1) { T.ShovedRefBad++; Fail(FString::Printf(TEXT("%s crater-lip centre %d,%d: the reference query mode reads %.3f, level %.2f"), *Name, X, Y, RefMode ? RefMode->HAt(Cx, Cz) : -1.0, Nominal)); }
				continue;
			}
			T.Plain++;
			const double D = FMath::Abs(Got - Nominal);
			T.PlainMax = FMath::Max(T.PlainMax, D);
			if (D > 0.1) { T.PlainBad++; Fail(FString::Printf(TEXT("%s centre %d,%d at %.3f, level %.2f"), *Name, X, Y, Got, Nominal)); }
		}
		// levelAt at the oracle's seeded points: the containing cell's level
		for (const FVector3d& Pt : O.Points)
		{
			const double X = Pt.X, Z = Pt.Y, SimL = Pt.Z;
			// points outside the map (Julio's edge and outside probes): levelAt there is the sim's own rule, not a cell's
			if (X < 0 || Z < 0 || X >= W * TCRelief::Cell || Z >= H * TCRelief::Cell) { T.Outside++; continue; }
			const int32 Cx = FMath::Clamp((int32)FMath::FloorToDouble(X / TCRelief::Cell), 0, W - 1), Cz = FMath::Clamp((int32)FMath::FloorToDouble(Z / TCRelief::Cell), 0, H - 1);
			T.Samples++;
			if (R.InLevel[(size_t)(Cz * W + Cx)] != (int32)SimL) { T.SampleBad++; Fail(FString::Printf(TEXT("%s levelAt(%.3f, %.3f) %g, client cell level %d"), *Name, X, Z, SimL, (int32)R.InLevel[(size_t)(Cz * W + Cx)])); }
		}
		// cliffs: every east and south neighbour pair two or more levels apart
		for (int32 Y = 0; Y < H; Y++) for (int32 X = 0; X < W; X++)
			for (int32 Dir = 0; Dir < 2; Dir++)
			{
				const int32 X2 = X + (Dir == 0), Y2 = Y + (Dir == 1);
				if (X2 >= W || Y2 >= H) continue;
				const int32 Step = FMath::Abs(R.InLevel[(size_t)(Y * W + X)] - R.InLevel[(size_t)(Y2 * W + X2)]);
				if (Step < 2) continue;
				T.CliffPairs++;
				// the steepest 0.1 m of the drawn ground on three lines across the shared edge (its middle and 0.5 m either
				// side), within 1 m of it: the rim is ragged and the face set back, so the drop is not always on the line
				double Jump = 0;
				for (const double Side : { -0.5, 0.0, 0.5 })
				{
					auto At = [&](double U)
					{
						const double Px = Dir == 0 ? (X + 1) * TCRelief::Cell + U : (X + 0.5) * TCRelief::Cell + Side;
						const double Pz = Dir == 1 ? (Y + 1) * TCRelief::Cell + U : (Y + 0.5) * TCRelief::Cell + Side;
						return R.HAt(Px, Pz);
					};
					for (int32 K = 0; K <= 95; K++) { const double U = -1.0 + K * 0.02; Jump = FMath::Max(Jump, FMath::Abs(At(U + 0.1) - At(U))); }
				}
				const double Ratio = Jump / (Step * TCRelief::LevelHeight);
				T.CliffMinRatio = FMath::Min(T.CliffMinRatio, Ratio);
				if (Ratio < 0.85) { T.CliffBad++; Fail(FString::Printf(TEXT("%s cliff %d,%d to %d,%d (%d levels): steepest drop %.3f m in 0.1 m"), *Name, X, Y, X2, Y2, Step, Jump)); }
			}
	};
	auto Report = [&](const TCHAR* Source, const FTally& T, int32 Maps, int64 WantSamples)
	{
		AddInfo(FString::Printf(TEXT("height oracle (%s): %d maps, %lld centres; levels off %lld, heightAt off %lld; levelAt points in the map %lld off %lld (outside the map %lld, not compared)"), Source, Maps, T.Centres, T.LevelBad, T.CentreSimBad, T.Samples, T.SampleBad, T.Outside));
		AddInfo(FString::Printf(TEXT("contract (%s): plain %lld (max %.2e m, over 0.1 m %lld); banks %lld; crater-lip cells %lld (drawn max %.3f m off, apart; the query mode max %.3f m, over 0.1 m %lld); trench %lld (bad %lld); road %lld (bad %lld); water %lld (bad %lld); ford %lld (bad %lld); bridge %lld (bad %lld)"),
			Source, T.Plain, T.PlainMax, T.PlainBad, T.Bank, T.Shoved, T.ShovedMax, T.ShovedRefMax, T.ShovedRefBad, T.Trench, T.TrenchBad, T.Road, T.RoadBad, T.Wet, T.WetBad, T.Ford, T.FordBad, T.Bridge, T.BridgeBad));
		AddInfo(FString::Printf(TEXT("cliffs (%s): %lld neighbour pairs 2+ levels apart, %lld drawn cliff edges, the steepest 0.1 m of each holds at least %.3f of its step (under 0.85: %lld)"), Source, T.CliffPairs, T.CliffEdgesDrawn, T.CliffMinRatio, T.CliffBad));
		TestEqual(FString::Printf(TEXT("%s: maps"), Source), Maps, 33);
		TestEqual(FString::Printf(TEXT("%s: cell centres (Julio's fixture count)"), Source), T.Centres, (int64)661799);
		TestEqual(FString::Printf(TEXT("%s: points"), Source), T.Samples + T.Outside, WantSamples);
		TestEqual(FString::Printf(TEXT("%s: cliff pairs (the oracle's count)"), Source), T.CliffPairs, (int64)4031);
		TestEqual(FString::Printf(TEXT("%s: drawn cliff edges"), Source), T.CliffEdgesDrawn, T.CliffPairs);
		TestTrue(FString::Printf(TEXT("%s: trench, road, ford and bridge centres were checked"), Source), T.Trench > 0 && T.Road > 0 && T.Ford > 0 && T.Bridge > 0);
	};

	// 1. the bundled oracle (shared/sim.js evaluated by unreal/Tools/terrain/ground-oracle.mjs)
	{
		FTally T;
		int32 Maps = 0;
		for (const TSharedPtr<FJsonValue>& Mv : Heights->GetArrayField(TEXT("maps")))
		{
			const FJsonObject& M = *Mv->AsObject();
			FMapOracle O;
			O.Name = FPaths::GetBaseFilename(M.GetStringField(TEXT("file")));
			O.W = (int32)M.GetNumberField(TEXT("w")); O.H = (int32)M.GetNumberField(TEXT("h"));
			for (const TSharedPtr<FJsonValue>& Cv : M.GetArrayField(TEXT("centres"))) { const TArray<TSharedPtr<FJsonValue>>& A = Cv->AsArray(); O.CentreHeight.Add(A[0]->AsNumber()); O.CentreLevel.Add(A[1]->AsNumber()); }
			for (const TSharedPtr<FJsonValue>& Sv : M.GetArrayField(TEXT("samples"))) { const TArray<TSharedPtr<FJsonValue>>& A = Sv->AsArray(); O.Points.Add(FVector3d(A[0]->AsNumber(), A[1]->AsNumber(), A[3]->AsNumber())); }
			CheckMap(O, T, Maps);
		}
		Report(TEXT("bundled sim.js oracle"), T, Maps, 33000);
	}
	// 2. Julio's maintained fixtures (docs/fixtures/terrain/source-authored on native main, a81b4f2), when present: the
	// repository's own copy, or -TCTerrainFixtures=<dir> / TC_TERRAIN_FIXTURES
	FString Julio = FPlatformMisc::GetEnvironmentVariable(TEXT("TC_TERRAIN_FIXTURES"));
	FParse::Value(FCommandLine::Get(), TEXT("TCTerrainFixtures="), Julio);
	if (Julio.IsEmpty()) Julio = FPaths::Combine(FPaths::ProjectDir(), TEXT("../docs/fixtures/terrain/source-authored"));
	TArray<FString> Files;
	IFileManager::Get().FindFiles(Files, *FPaths::Combine(Julio, TEXT("*.json")), true, false);
	if (Files.Num() == 0) AddInfo(FString::Printf(TEXT("Julio's terrain fixtures: none at %s (set -TCTerrainFixtures=<dir>); only the bundled oracle was checked"), *Julio));
	else
	{
		FTally T;
		int32 Maps = 0;
		Files.Sort();
		for (const FString& F : Files)
		{
			FString Text;
			TSharedPtr<FJsonObject> Root;
			if (!FFileHelper::LoadFileToString(Text, *FPaths::Combine(Julio, F)) || !FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root) || !Root.IsValid()) { Fail(TEXT("cannot read ") + F); continue; }
			FMapOracle O;
			O.Name = Root->GetStringField(TEXT("mapKey"));
			const TSharedPtr<FJsonObject> Map = Root->GetObjectField(TEXT("map"));
			O.W = (int32)Map->GetNumberField(TEXT("w")); O.H = (int32)Map->GetNumberField(TEXT("h"));
			// centres: [cell, x, z, rawLevel, levelMetres, heightAtMetres]; points: [x, z, rawLevel, levelMetres, heightAtMetres]
			for (const TSharedPtr<FJsonValue>& Cv : Root->GetArrayField(TEXT("centres"))) { const TArray<TSharedPtr<FJsonValue>>& A = Cv->AsArray(); O.CentreLevel.Add(A[3]->AsNumber()); O.CentreHeight.Add(A[5]->AsNumber()); }
			for (const TSharedPtr<FJsonValue>& Pv : Root->GetArrayField(TEXT("points"))) { const TArray<TSharedPtr<FJsonValue>>& A = Pv->AsArray(); O.Points.Add(FVector3d(A[0]->AsNumber(), A[1]->AsNumber(), A[2]->AsNumber())); }
			CheckMap(O, T, Maps);
		}
		Report(TEXT("Julio's fixtures"), T, Maps, 33000);
	}
	return Errors == 0;
}

#endif

#endif // TC_TERRAIN_CONTRACT_CORE_ONLY
