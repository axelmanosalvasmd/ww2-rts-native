// The relief: the ground mesh the client draws, places units on and picks against, ported line for line from the
// pinned reference's client/relief.js (createRelief) and client/water-levels.js. Engine-free (standard C++ only), so
// unreal/Tools/relief/harness checks it against the reference outside Unreal.
//
// What it draws (relief.js): each cell is a 5 x 5 node lattice (quarter cells). Cell centres keep their simulation
// height. Only connected neighbours (a level step under 2) share a ramp: cubic Hermite patches through the cell's control
// nodes with limited tangents, so consecutive rising cells slope through their centres and flat tops stay flat. Steps of
// 2 or more levels stay apart: the two sides keep separate vertices, a sealed rock face (bands of rock triangles with a
// warped, set-back profile) joins them, and fractured rock models (6 lumpy icosahedra cut by cleavage planes, two
// detail levels) are stacked on it in strata, with crest boulders, scree at the foot and rock piles in the notches and
// around the teeth of diagonal runs. Banks carve each water body below its frozen line (fords 0.15 m, channels 0.42 m
// plus 0.38 m per row inward), trenches ('T') sink their inner nodes 0.9 m and run on through connected trench cells,
// roads sink 0.1 m through a shallow centre fan, crater and rubble lips ('+', 'R') are bitten and shoved. Flat rows merge
// into runs; rough cells get a centre fan while a triangle budget lasts.
//
// Recipient knowledge (not in relief.js, which draws every cell of start.map): a cell this client does not know is never
// drawn and never shapes known ground. It joins no ramp, makes no cliff, holds no water, road, trench or scar, and the
// known side of its edge stays at its own level, as at the map border. A query inside an unknown cell reads a flat cell
// at the mean level of its known neighbours (0 when none). On a map where every cell is known (every authored map) the
// result is the reference's, vertex for vertex.
//
// Ground queries: HeightAt and Ray read the drawn triangles where the surface is reshaped (ragged cliff rims, rock faces
// and, in Drawn mode, cells whose vertices a crater lip shoved) and the node lattice elsewhere, where the lattice is
// exactly the drawn triangles. Rock models (paint 0.98) are decoration and never ground, as in the reference. In Reference
// mode (bDrawnPicks false) the queries are relief.js hAt and mesh.raycast exactly (they read shoved lips unshoved).
#pragma once

#include <cstdint>
#include <unordered_map>
#include <vector>

namespace TCRelief
{
	namespace ReliefPlan { struct FWall; }
	inline constexpr double Cell = 2.0;          // shared/sim.js CELL
	inline constexpr double LevelHeight = 2.5;   // shared/sim.js CFG.levelHeight
	inline constexpr double TrenchDepth = 0.9;   // relief.js TRENCH_DEPTH
	inline constexpr double WaterLift = 0.06;    // water-levels.js WATER_LIFT
	inline constexpr int S = 4, Stride = 25;     // nodes per cell side - 1, nodes per cell

	// One part of the drawn mesh (a row of cells in Reference layout, a square chunk otherwise). Positions are reference
	// metres (x east, y up, z south); per vertex: normal, uv (x / map width, 1 - z / map height), relief paint (rock, lip,
	// foot, damp: relief-material.js reliefPaint), scar, the cell the vertex belongs to (its fog) and whether it is a
	// detail vertex (its triangles answer ground queries).
	struct FPart
	{
		std::vector<float> Pos, Nrm, Uv, Paint, Scar;
		std::vector<uint32_t> Idx;
		std::vector<int32_t> Owner;
		std::vector<uint8_t> Detail;
		// detail triangles (first index into Idx) per cell they overlap
		std::unordered_map<int, std::vector<uint32_t>> Picks;
		int Vertices() const { return (int)(Pos.size() / 3); }
		void Clear();
	};

	struct FStats { int Vertices = 0, Triangles = 0, CliffEdges = 0, CellsUpdated = 0, PartsRebuilt = 0; };

	class FRelief
	{
	public:
		// Inputs, W * H each, written by the caller before Build / Update: the cell's level, its physical terrain
		// character (client/main.js terrain.physicalGrid), the road mask (ground.js prim === ROAD, main.js isRoad),
		// whether this client knows the cell, and the frozen levels the water lines come from (water-levels.js reads
		// map.heights once, when the relief is created).
		std::vector<int8_t> InLevel, InFrozen;
		std::vector<uint8_t> InGrid, InRoad, InKnown;

		// Layout: ChunkCells 0 emits one part per row with the reference's runs (the harness compares that vertex for
		// vertex); otherwise square chunks of ChunkCells, one quad per flat cell (bMergeFlatRuns, same surface).
		void Reset(int InW, int InH, bool bInLow, int InChunkCells, bool bInDrawnPicks);
		void Build();
		// Cells whose level, character, road or knowledge changed (relief.js update).
		void Update(const std::vector<int>& Cells);
		// Parts emitted since the last call.
		std::vector<int> TakeRebuilt();

		int W = 0, H = 0, ChunkCells = 0, PartsX = 1, PartsY = 1;
		bool bLow = false, bDrawnPicks = true;
		// relief.js merges flat cells of a row into one quad (up to 32 cells); its paint is a texture. The game's chunks
		// carry each cell's colour and fog per vertex, so there every flat cell keeps its own quad (same surface).
		bool bMergeFlatRuns = true;
		int NumParts() const { return (int)Parts.size(); }
		const FPart& Part(int I) const { return Parts[(size_t)I]; }
		int PartOfCell(int C) const;
		FStats Stats;

		// relief.js hAt (metres), waterAt (NaN away from water)
		double HAt(double X, double Z) const;
		double WaterAt(double X, double Z) const;
		// relief.js mesh.raycast: the first ground hit (metres) of a ray from O along D (any length), within MaxT
		bool Ray(const double O[3], const double D[3], double MaxT, double Out[3]) const;
		// the drawn ground's lowest and highest heights so far (pick bounds)
		double MinY = 0, MaxY = 0;

		// cell state the queries and tests read
		bool IsRagged(int C) const { return Ragged[(size_t)C] != 0; }
		bool IsShoved(int C) const { return Shoved[(size_t)C] != 0; }
		bool IsWet(int C) const { return Wet[(size_t)C] != 0; }
		int CliffEdgesNow() const;

	private:
		int N = 0, QW = 1;
		std::vector<int8_t> Levels;
		std::vector<uint8_t> Roads, NX, NZ, Flat, Ragged, Torn, Shoved, Fan, Wet, Bridges, Depth;
		std::vector<uint16_t> Masks;
		std::vector<float> Heights, Normals, Controls, RoadCorners, Tangents, WL;
		struct FRowPlan { int Triangles = 0, CliffEdges = 0; std::vector<int> Rough; };
		std::vector<FRowPlan> Rows;
		std::vector<FPart> Parts;
		std::vector<int> Rebuilt;
		double Out[6] = {};

		// relief.js, function for function
		bool Kn(int C) const { return InKnown.empty() || InKnown[(size_t)C] != 0; }
		bool Joined(int A, int B) const;
		bool Cliff(int A, int B) const;
		int IdAt(int X, int Y) const;
		uint8_t Ch(int C) const { return Kn(C) ? InGrid[(size_t)C] : (uint8_t)'.'; }
		void ReadCell(int C);
		void ReadQuad(int X, int Y);
		void ReadWater();
		void Sample(double U, double V, int Owner, bool bNearWet);
		void ControlCell(int C);
		void TangentCell(int C);
		void BaseSample(double U, double V, int Owner);
		void RoadSample(double U, double V, int Owner, double* Dst) const;
		void WallsOf(int Z, int X0, int X1, int& CliffEdges, std::vector<ReliefPlan::FWall>& OutWalls) const;
		struct FTouch { int Span = 0; bool bScar = false; double Gx = 0, Gz = 0; int MinL = 0; bool bInside = false; };
		FTouch Touch(double WU, double WV) const;
		bool ScarRun(double WU, double WV, double& Ax, double& Az) const;
		void LipShift(double WU, double WV, double Shift[2]) const;
		void ChewScar(int X, int Z, int Off);
		void BuildCell(int C);
		void CornerShift(int X, int Z, double OutV[4]) const;
		void EdgeShift(double U, double V, double Shift[2]) const;
		void PlanRow(int Z);
		void Fans();
		void EmitPart(int P);
		double Lattice(int C, double U, double V) const;
		bool Mixed(int CX, int CZ) const;
		double GuessLevel(int C) const;
	};

	// relief.js rockModels(detail): 6 models, each a triangle list (x, y, z per corner), detail 0 and 1
	const std::vector<std::vector<double>>& RockModels(int Detail);

	// client/ground.js cellAttrs, the cells it gives prim ROAD: 'D' cells, and open ground ('.') beside a house of the
	// map rows (Rows) that is not a field (Fields, ground.js fieldsOf) and has no water (W, F, =) among its 8 neighbours
	// in Grid. Unknown cells (Known 0) are never roads.
	void RoadMask(int W, int H, const std::vector<uint8_t>& Grid, const std::vector<uint8_t>& Rows,
		const std::vector<uint8_t>& Fields, const std::vector<uint8_t>& Known, std::vector<uint8_t>& Out);
}
