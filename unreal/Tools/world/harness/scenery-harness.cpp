// Standalone check of the engine-free map pieces (TCWorldCore, World/TCWorldObjects.h) and scenery (TCScatterCore,
// World/TCScatter.h) against the Node oracle. Reads to-input.mjs's lines, writes what the port places; compare.mjs
// holds that to the fixture. Build and run: ./run.sh (g++ or clang++, C++17, no engine).
#define TC_WORLD_CORE_ONLY 1
#include "World/TCWorldObjects.h"
#include "World/TCScatter.h"
#include <cstdio>
#include <fstream>
#include <map>
#include <set>
#include <sstream>
#include <string>

using namespace TCWorldCore;
using namespace TCScatterCore;

static double Wave(double X, double Z) { return 0.6 * std::sin(X * 0.13) + 0.4 * std::cos(Z * 0.09) + 0.002 * X; }

struct FCase
{
	int W = 0, H = 0;
	bool bLow = false, bPieces = false, bProps = false, bWorld = false;
	std::vector<std::string> Live, Orig, Rows, Heights;
	std::vector<int> Cells, Mines, Skip, Biome;
	std::vector<std::pair<int, std::pair<int, std::string>>> Bld; // x, (y, kind)
	std::vector<std::pair<double, double>> Nodes;
	std::vector<FCellXY> Spawns, Points;
	std::string Disc;
};

class FSrc : public FCellSource
{
public:
	const FCase& C;
	std::set<int> Stone, SpoilSet, MineSet;
	explicit FSrc(const FCase& In) : C(In)
	{
		for (const int M : C.Mines) MineSet.insert(M);
		std::vector<std::pair<int, int>> Q;
		for (const auto& B : C.Bld)
		{
			const int X = B.first, Y = B.second.first;
			if (B.second.second == "spoil") SpoilSet.insert(Y * C.W + X);
			if (B.second.second == "stone_bridge" && At(X, Y) == '=') { Q.push_back({ X, Y }); Stone.insert(Y * C.W + X); }
		}
		const int D[4][2] = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } };
		for (size_t I = 0; I < Q.size(); I++) for (const auto& Dd : D)
		{
			const int Nx = Q[I].first + Dd[0], Ny = Q[I].second + Dd[1], K = Ny * C.W + Nx;
			if (!Stone.count(K) && At(Nx, Ny) == '=') { Stone.insert(K); Q.push_back({ Nx, Ny }); }
		}
	}
	bool In(int X, int Y) const { return X >= 0 && Y >= 0 && X < C.W && Y < C.H; }
	int Width() const override { return C.W; }
	int Height() const override { return C.H; }
	unsigned char At(int X, int Y) const override { return In(X, Y) ? (unsigned char)C.Live[(size_t)Y][(size_t)X] : 0; }
	unsigned char Orig(int X, int Y) const override { return In(X, Y) ? (unsigned char)C.Orig[(size_t)Y][(size_t)X] : 0; }
	int Stage(int X, int Y) const override { return In(X, Y) && !C.Cells.empty() ? (C.Cells[(size_t)(Y * C.W + X)] >> 3) & 3 : 0; }
	bool Mine(int X, int Y) const override { return In(X, Y) && (C.Mines.empty() ? At(X, Y) == 'N' : MineSet.count(Y * C.W + X) > 0); }
	bool StoneDeck(int X, int Y) const override { return In(X, Y) && Stone.count(Y * C.W + X) > 0; }
	bool Spoil(int X, int Y) const override { return In(X, Y) && SpoilSet.count(Y * C.W + X) > 0; }
	double Ground(double X, double Z) const override { return Wave(X, Z); }
	bool Low() const override { return C.bLow; }
};

class FLive : public FLiveSource
{
public:
	const FCase& C;
	std::set<int> SkipSet;
	explicit FLive(const FCase& In) : C(In) { for (const int S : C.Skip) SkipSet.insert(S); }
	int Width() const override { return C.W; }
	int Height() const override { return C.H; }
	unsigned char At(int X, int Y) const override { return X >= 0 && Y >= 0 && X < C.W && Y < C.H ? (unsigned char)C.Live[(size_t)Y][(size_t)X] : 0; }
	int Level(int X, int Y) const override { return X >= 0 && Y >= 0 && Y < (int)C.Heights.size() && X < (int)C.Heights[(size_t)Y].size() ? LevelOf((unsigned char)C.Heights[(size_t)Y][(size_t)X]) : 0; }
	double Ground(double X, double Z) const override { return Wave(X, Z); }
	bool Skip(int Cell) const override { return SkipSet.count(Cell) > 0; }
	bool Low() const override { return C.bLow; }
};

int main(int Argc, char** Argv)
{
	if (Argc < 3) { std::fprintf(stderr, "usage: scenery-harness input.txt output.txt\n"); return 2; }
	std::ifstream In(Argv[1]);
	std::FILE* Out = std::fopen(Argv[2], "w");
	std::string Line;
	FCase C;
	int Index = 0;
	auto Ints = [](std::istringstream& Ss) { std::vector<int> V; int X; while (Ss >> X) V.push_back(X); return V; };
	while (std::getline(In, Line))
	{
		std::istringstream Ss(Line);
		std::string Tag;
		Ss >> Tag;
		auto Rest = [&Line, &Tag]() { return Line.size() > Tag.size() + 1 ? Line.substr(Tag.size() + 1) : std::string(); };
		if (Tag == "CASE") { C = FCase(); Ss >> Index; }
		else if (Tag == "DIM") { int L, P, Q, Wd; Ss >> C.W >> C.H >> L >> P >> Q >> Wd; C.bLow = L; C.bPieces = P; C.bProps = Q; C.bWorld = Wd; }
		else if (Tag == "LIVE") C.Live.push_back(Rest());
		else if (Tag == "ORIG") C.Orig.push_back(Rest());
		else if (Tag == "ROWS") C.Rows.push_back(Rest());
		else if (Tag == "HEI") C.Heights.push_back(Rest());
		else if (Tag == "CELLS") C.Cells = Ints(Ss);
		else if (Tag == "MINE") C.Mines = Ints(Ss);
		else if (Tag == "SKIP") C.Skip = Ints(Ss);
		else if (Tag == "BIOME") C.Biome = Ints(Ss);
		else if (Tag == "DISC") Ss >> C.Disc;
		else if (Tag == "BLD") { int X, Y; std::string K; Ss >> X >> Y >> K; C.Bld.push_back({ X, { Y, K } }); }
		else if (Tag == "NODE") { double X, Z; Ss >> X >> Z; C.Nodes.push_back({ X, Z }); }
		else if (Tag == "SPAWN") { FCellXY P; Ss >> P.X >> P.Y; C.Spawns.push_back(P); }
		else if (Tag == "POINT") { FCellXY P; Ss >> P.X >> P.Y; C.Points.push_back(P); }
		else if (Tag == "END")
		{
			std::fprintf(Out, "CASE %d\n", Index);
			if (C.bPieces)
			{
				FSrc Src(C);
				std::map<int, int> Named;
				for (const auto& B : C.Bld)
				{
					std::string K = B.second.second;
					for (char& Ch : K) if (Ch == '_') Ch = ' ';
					const int T = HouseTypeOf(K.c_str());
					if (T >= 0 && B.first >= 0 && B.second.first >= 0 && B.first < C.W && B.second.first < C.H) Named[B.second.first * C.W + B.first] = T;
				}
				std::vector<FPiece> Pieces;
				BuildAllPieces(Src, [&Named](int Cell) { const auto It = Named.find(Cell); return It == Named.end() ? -1 : It->second; }, Pieces);
				for (const FPiece& P : Pieces)
				{
					double M[16];
					PieceMatrix(P, M);
					std::fprintf(Out, "P %s", PieceName(P.Kind));
					for (const double V : M) std::fprintf(Out, " %.7g", (double)(float)V);
					std::fprintf(Out, " %.7g %.7g %.7g\n", (double)P.R, (double)P.G, (double)P.B);
				}
			}
			if (C.bProps)
			{
				FScatterMap M;
				M.W = C.W; M.H = C.H; M.Rows = C.Rows; M.Spawns = C.Spawns; M.Points = C.Points; M.bWorld = C.bWorld;
				for (const int B : C.Biome) M.Biome.push_back((unsigned char)B);
				for (const char Ch : C.Disc) M.Discovered.push_back(Ch == '1' ? 1 : 0);
				const std::vector<FCandidate> List = Candidates(M);
				for (const FCandidate& K : List)
					std::fprintf(Out, "C %s %d %d %.9g %.9g %u %.9g %.9g %d %d %d %d %d\n", PropName(K.Kind), K.Cx, K.Cy, K.X, K.Z, K.Seed, K.Angle, K.Scale, K.AnchorX, K.AnchorY, K.Biome, K.bOrchard ? 1 : 0, K.bWood ? 1 : 0);
				const FLive Live(C);
				for (const FCandidate& K : List)
				{
					const FPlaced P = Place(K, Live, C.Nodes);
					if (!P.bShown) { std::fprintf(Out, "L -\n"); continue; }
					std::fprintf(Out, "L %s", PropName(P.Kind));
					for (const double V : P.M) std::fprintf(Out, " %.9g", V);
					std::fprintf(Out, " %.9g %.9g %.9g %.9g\n", P.Col[0], P.Col[1], P.Col[2], P.Bark);
				}
			}
		}
	}
	std::fclose(Out);
	return 0;
}
