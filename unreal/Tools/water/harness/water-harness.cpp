// Standalone check of the engine-free water (TCWaterCore, World/TCWater.h) and apron (TCApronCore, World/TCApron.h)
// against the Node oracle (../oracle.mjs). For each case directory: reads input.txt, rebuilds what the reference built
// from the same inputs, and writes it next to the oracle's files as port.*.bin; compare.mjs holds the two together.
// Build and run: ./run.sh (g++ or clang++, C++17, no engine).
#define TC_WATER_CORE_ONLY 1
#define TC_APRON_CORE_ONLY 1
#include "World/TCWater.h"
#include "World/TCApron.h"
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <unordered_map>

template <typename T>
static void Write(const std::string& Path, const std::vector<T>& V)
{
	std::ofstream F(Path, std::ios::binary);
	F.write(reinterpret_cast<const char*>(V.data()), (std::streamsize)(V.size() * sizeof(T)));
}

struct FHeights
{
	std::unordered_map<std::string, double> Map;
	static std::string Key(double X, double Z) { char B[96]; std::snprintf(B, sizeof B, "%.17g %.17g", X, Z); return B; }
	void Add(double X, double Z, double V) { Map[Key(X, Z)] = V; }
	mutable int Missing = 0;
	double At(double X, double Z) const
	{
		auto It = Map.find(Key(X, Z));
		if (It == Map.end()) { Missing++; return 0; }
		return It->second;
	}
};

struct FStep { std::string Name; std::vector<std::tuple<int, char, bool, int>> Cells; FHeights H; bool bCreate = false; std::vector<int8_t> Frozen; };

static std::vector<int> Ints(std::istringstream& S) { std::vector<int> V; int X; while (S >> X) V.push_back(X); return V; }

int main(int Argc, char** Argv)
{
	if (Argc < 3) { std::fprintf(stderr, "usage: water-harness <oracle dir> <case> [<case>...]\n"); return 2; }
	const std::string Root = Argv[1];
	// the two noise textures
	Write(Root + "/port.water-noise.bin", TCWaterCore::NoiseTexture());
	const TCApronCore::FGroundNoise Noise = TCApronCore::BuildNoise();
	Write(Root + "/port.ground-noise.bin", TCApronCore::NoiseTexture(Noise));
	int Failures = 0;
	for (int A = 2; A < Argc; A++)
	{
		const std::string Dir = Root + "/" + Argv[A];
		std::ifstream In(Dir + "/input.txt");
		if (!In) { std::fprintf(stderr, "%s: no input.txt\n", Argv[A]); Failures++; continue; }
		int W = 0, H = 0, World = 0;
		std::vector<std::string> Phys, Grid, Orig;
		std::vector<int> Levels, State, Biome;
		std::vector<int8_t> Frozen;
		bool bStartFrozen = false;
		std::string Known;
		std::vector<std::pair<int, int>> Spawns, Points;
		FHeights Apron, Water0;
		std::vector<FStep> Steps;
		std::string Line;
		while (std::getline(In, Line))
		{
			std::istringstream S(Line);
			std::string Tag;
			S >> Tag;
			if (Tag == "size") S >> W >> H >> World;
			else if (Tag == "phys") { std::string R; S >> R; Phys.push_back(R); }
			else if (Tag == "grid") { std::string R; S >> R; Grid.push_back(R); }
			else if (Tag == "orig") { std::string R; S >> R; Orig.push_back(R); }
			else if (Tag == "levels") Levels = Ints(S);
			else if (Tag == "frozen") { for (int V : Ints(S)) Frozen.push_back((int8_t)V); bStartFrozen = true; }
			else if (Tag == "known") S >> Known;
			else if (Tag == "state") State = Ints(S);
			else if (Tag == "biome") { std::string Rest; std::getline(S, Rest); if (Rest.find("none") == std::string::npos) { std::istringstream R(Rest); Biome = Ints(R); } }
			else if (Tag == "spawns" || Tag == "points")
			{
				std::string P;
				while (S >> P) { const size_t K = P.find(','); (Tag == "spawns" ? Spawns : Points).push_back({ std::atoi(P.substr(0, K).c_str()), std::atoi(P.substr(K + 1).c_str()) }); }
			}
			else if (Tag == "happron" || Tag == "hwater")
			{
				std::string X, Z, V;
				S >> X >> Z >> V;
				FHeights& T = Tag == "happron" ? Apron : Steps.empty() ? Water0 : Steps.back().H;
				T.Add(std::strtod(X.c_str(), nullptr), std::strtod(Z.c_str(), nullptr), std::strtod(V.c_str(), nullptr));
			}
			else if (Tag == "step") { Steps.emplace_back(); S >> Steps.back().Name; }
			else if (Tag == "create") { Steps.back().bCreate = true; for (int V : Ints(S)) Steps.back().Frozen.push_back((int8_t)V); }
			else if (Tag == "cell")
			{
				int C; std::string Ch, Lv;
				S >> C >> Ch >> Lv;
				Steps.back().Cells.emplace_back(C, Ch[0], Lv != "none", Lv != "none" ? std::atoi(Lv.c_str()) : 0);
			}
		}
		const int N = W * H;
		std::vector<uint8_t> Ph(N), Gr(N), Or(N), Kn(N);
		for (int C = 0; C < N; C++) { Ph[C] = Phys[C / W][C % W]; Gr[C] = Grid[C / W][C % W]; Or[C] = Orig[C / W][C % W]; Kn[C] = Known[C] == '1'; }

		// ---------- apron ----------
		TCApronCore::FApron Ap;
		Ap.Build(W, H);
		Ap.Shape([&](double X, double Z) { return Apron.At(X, Z); }, [&](int C) { return TCWaterCore::IsWet(Ph[C]); });
		TCApronCore::FCellSource Src;
		Src.W = W; Src.H = H;
		Src.Grid = [&](int C) { return Gr[C]; };
		Src.Physical = [&](int C) { return Ph[C]; };
		Src.Orig = [&](int C) { return Or[C]; };
		Src.Level = [&](int C) { return Levels[C]; };
		Src.State = [&](int C) { return State[C]; };
		if (!Biome.empty()) Src.Biome = [&](int C) { return Biome[C] == 255 ? -1 : Biome[C]; };
		Src.Spawns = Spawns; Src.Points = Points;
		const std::vector<uint8_t> Fld = TCApronCore::Fields(Src);
		std::vector<uint8_t> Cells((size_t)N * 4);
		for (int C = 0; C < N; C++) TCApronCore::CellTexel(TCApronCore::CellMaterial(Src, Fld, Noise, C), &Cells[(size_t)C * 4]);
		Write(Dir + "/port.apron0.pos.bin", Ap.Position);
		Write(Dir + "/port.apron0.nor.bin", Ap.Normal);
		Write(Dir + "/port.apron0.index.bin", Ap.Index);
		Write(Dir + "/port.apron0.cells.bin", Cells);
		Write(Dir + "/port.apron0.cloud.bin", std::vector<uint32_t>{ (uint32_t)Ap.CloudCount });
		if (Apron.Missing) { std::fprintf(stderr, "%s: apron asked %d heights the reference did not\n", Argv[A], Apron.Missing); Failures++; }

		// ---------- water ----------
		TCWaterCore::FWater Wt;
		const FHeights* Heights = &Water0;
		Wt.Char = [&](int C) { return Ph[C]; };
		if (World) Wt.Unknown = [&](int C) { return !Kn[C]; };
		Wt.GroundHeight = [&](double X, double Z) { return Heights->At(X, Z); };
		auto Dump = [&](int I)
		{
			const std::string P = Dir + "/port.water" + std::to_string(I);
			if (!Wt.Valid()) { Write(P + ".none", std::vector<uint8_t>()); return; }
			Write(P + ".pos.bin", Wt.Pos); Write(P + ".flow.bin", Wt.Flow); Write(P + ".nor.bin", Wt.Normal);
			Write(P + ".index.bin", Wt.Index); Write(P + ".mask.bin", Wt.Mask);
		};
		auto Create = [&](const std::vector<int8_t>& Fz) { return Wt.Create(W, H, [&](int C) { return Fz[C]; }, [](int) { return true; }); };
		if (bStartFrozen) Create(Frozen);
		Dump(0);
		for (size_t I = 0; I < Steps.size(); I++)
		{
			FStep& St = Steps[I];
			Heights = &St.H;
			for (const auto& [C, Ch, bLv, Lv] : St.Cells)
			{
				// main.js applyCells: the grid and levels first; a World cell becomes known
				Ph[C] = (uint8_t)Ch; Gr[C] = (uint8_t)Ch; Kn[C] = 1;
				if (bLv) Levels[C] = Lv;
			}
			if (St.bCreate) Create(St.Frozen);
			else if (Wt.Valid())
			{
				for (const auto& [C, Ch, bLv, Lv] : St.Cells) Wt.Changed(C, (uint8_t)Ch, bLv);
				Wt.Tick();
			}
			Dump((int)I + 1);
			if (St.H.Missing) { std::fprintf(stderr, "%s step %s: water asked %d heights the reference did not\n", Argv[A], St.Name.c_str(), St.H.Missing); Failures++; }
		}
		if (Water0.Missing) { std::fprintf(stderr, "%s: water asked %d heights the reference did not\n", Argv[A], Water0.Missing); Failures++; }
		std::printf("%s: %dx%d, apron %d columns, water %s, %zu steps\n", Argv[A], W, H, Ap.N, Wt.Valid() ? "yes" : "none", Steps.size());
	}
	return Failures ? 1 : 0;
}
