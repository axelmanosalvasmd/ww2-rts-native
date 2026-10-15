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
}

#if defined(TC_TERRAIN_CONTRACT_CORE_ONLY)
int main()
{
	int Failures = 0;
	for (const auto& Result : { TCTerrainFormulas::ReliefRules(), TCTerrainFormulas::PaintRules(), TCTerrainFormulas::DecalRules() })
		for (const auto& Error : Result.Errors) { std::fprintf(stderr, "%s\n", Error.c_str()); Failures++; }
	std::printf("terrain reference formulas: 3 checks, %d failures\n", Failures);
	return Failures ? 1 : 0;
}
#else
#include "CoreMinimal.h"
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
