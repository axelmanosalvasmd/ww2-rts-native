// Standalone check of the engine-free railways and atmosphere cores (World/TCRailways.h TCRailCore, World/TCAtmosphere.h
// TCAtmosCore) against the reference oracle (unreal/Tools/atmosphere/oracle.mjs): reads the fixture's inputs
// (to-input.mjs), writes what the port builds from them as JSON for compare.mjs. No engine.
#define TC_RAIL_CORE_ONLY
#define TC_ATMOS_CORE_ONLY
#include "World/TCRailways.h"
#include "World/TCAtmosphere.h"
#include <cstdio>
#include <fstream>
#include <map>
#include <sstream>
#include <string>

using namespace TCRailCore;
static double Wave(double X, double Z) { return 0.6 * std::sin(X * 0.13) + 0.4 * std::cos(Z * 0.09) + 0.002 * X - 0.35; }

struct FOut
{
	std::ostringstream S;
	bool First = true;
	void Num(double V) { char B[64]; std::snprintf(B, sizeof B, "%.9g", V); S << B; }
	void Arr(const std::vector<double>& V) { S << '['; for (size_t I = 0; I < V.size(); I++) { if (I) S << ','; Num(V[I]); } S << ']'; }
	void Mats(const std::vector<FMat4>& Ms) { std::vector<double> V; for (const FMat4& M : Ms) V.insert(V.end(), M.E, M.E + 16); Arr(V); }
};

int main(int Argc, char** Argv)
{
	if (Argc < 3) { std::fprintf(stderr, "usage: atmos-harness <input.txt> <output.json>\n"); return 2; }
	std::ifstream In(Argv[1]);
	std::string Tok;
	FOut O;
	O.S << '{';
	// ---------- railways
	In >> Tok; int W, H, Steps; In >> W >> H >> Steps;
	FModel Model; Model.Reset(W);
	O.S << "\"rail\":[";
	for (int I = 0; I < Steps; I++)
	{
		double Dt; int NC; In >> Dt >> NC;
		std::map<int, std::pair<double, double>> Centres;
		for (int K = 0; K < NC; K++) { int Id; double X, Z; In >> Id >> X >> Z; Centres[Id] = { X, Z }; }
		int NR; In >> NR;
		for (int K = 0; K < NR; K++) { int C, Rail, HasSt, St; In >> C >> Rail >> HasSt >> St; Model.Cell_(C, Rail, HasSt != 0, St); }
		Model.Frame(Dt, Wave, [&](int R, double& X, double& Z) { auto It = Centres.find(R); if (It == Centres.end()) return false; X = It->second.first; Z = It->second.second; return true; });
		if (I) O.S << ',';
		O.S << "{\"ballast\":"; O.Mats(Model.BallastM); O.S << ",\"ties\":"; O.Mats(Model.TiesM); O.S << ",\"rails\":"; O.Mats(Model.RailsM);
		O.S << ",\"stations\":[";
		for (size_t K = 0; K < Model.BuiltStations.size(); K++)
		{
			const FStation& St = Model.BuiltStations[K];
			if (K) O.S << ',';
			O.S << "{\"region\":" << St.Region << ",\"parts\":[";
			for (size_t P = 0; P < St.Parts.size(); P++) { if (P) O.S << ','; O.Arr(std::vector<double>(St.Parts[P].M.E, St.Parts[P].M.E + 16)); }
			O.S << "]}";
		}
		O.S << "],\"has\":" << Model.TrackOrder.size() << '}';
	}
	O.S << ']';
	// ---------- noise, falls
	{
		const std::vector<uint8_t> N = TCAtmosCore::NoiseTexture();
		O.S << ",\"noise\":["; for (size_t I = 0; I < N.size(); I++) { if (I) O.S << ','; O.S << (int)N[I]; } O.S << ']';
		O.S << ",\"falls\":{";
		for (int K = 0; K < 3; K++)
		{
			const TCAtmosCore::FFall& F = TCAtmosCore::Fall(K);
			std::vector<float> P, R; TCAtmosCore::FallData(F, P, R);
			if (K) O.S << ',';
			O.S << '"' << F.Kind << "\":{\"position\":"; O.Arr(std::vector<double>(P.begin(), P.end())); O.S << ",\"rand\":"; O.Arr(std::vector<double>(R.begin(), R.end())); O.S << '}';
		}
		O.S << '}';
	}
	// ---------- maps: mask, mist, banks
	In >> Tok; int NM; In >> NM;
	std::map<std::string, std::vector<std::string>> Rows;
	std::map<std::string, std::pair<int, int>> Size;
	O.S << ",\"maps\":[";
	for (int I = 0; I < NM; I++)
	{
		std::string File; int MW, MH; In >> File >> MW >> MH;
		std::vector<std::string> R(MH);
		for (int Y = 0; Y < MH; Y++) In >> R[Y];
		Rows[File] = R; Size[File] = { MW, MH };
		auto Ch = [&R](int X, int Y) { return R[Y][X]; };
		const std::vector<uint8_t> Mask = TCAtmosCore::GroundMask(MW, MH, Ch);
		const auto Mist = TCAtmosCore::MistSites(MW, MH, Ch, 2.0, Wave, 36);
		const auto Banks = TCAtmosCore::BankSites(MW * 2.0, MH * 2.0, Wave, 64);
		if (I) O.S << ',';
		O.S << "{\"file\":\"" << File << "\",\"mask\":["; for (size_t K = 0; K < Mask.size(); K++) { if (K) O.S << ','; O.S << (int)Mask[K]; } O.S << ']';
		auto Sheets = [&](const char* Name, const std::vector<TCAtmosCore::FSheet>& L)
		{
			O.S << ",\"" << Name << "\":[";
			for (size_t K = 0; K < L.size(); K++) { if (K) O.S << ','; O.Arr({ L[K].X, L[K].Z, L[K].Y, L[K].Yaw, L[K].SX, L[K].SZ, L[K].Ph }); }
			O.S << ']';
		};
		Sheets("mist", Mist); Sheets("banks", Banks);
		O.S << '}';
	}
	O.S << ']';
	// ---------- wind
	In >> Tok; int NW; In >> NW;
	O.S << ",\"wind\":[";
	for (int I = 0; I < NW; I++) { double A, V; In >> A >> V; TCAtmosCore::FWind Wd; Wd.Set(A, V); if (I) O.S << ','; O.Arr({ A, V, Wd.X, Wd.Z }); }
	O.S << ']';
	// ---------- running: poses over time (atmosphere.js update)
	In >> Tok; int NR; In >> NR;
	O.S << ",\"running\":[";
	for (int I = 0; I < NR; I++)
	{
		std::string File, Mood; int HasWind; double WA = 0, WV = 0; int NF;
		In >> File >> Mood >> HasWind; if (HasWind) In >> WA >> WV; In >> NF;
		const auto& R = Rows[File]; const int MW = Size[File].first, MH = Size[File].second;
		auto Ch = [&R](int X, int Y) { return R[Y][X]; };
		const double MWm = MW * 2.0, MHm = MH * 2.0;
		const bool bMist = Mood == "dawn";
		const auto Sites = bMist ? TCAtmosCore::MistSites(MW, MH, Ch, 2.0, Wave, 36) : std::vector<TCAtmosCore::FSheet>();
		const auto Banks = TCAtmosCore::BankSites(MWm, MHm, Wave, 64);
		const auto Flight = TCAtmosCore::Flights(MWm, MHm);
		TCAtmosCore::FWind Wd;
		double DX = 0, DY = 0, T = 0;
		if (I) O.S << ',';
		O.S << "{\"file\":\"" << File << "\",\"frames\":[";
		for (int K = 0; K < NF; K++)
		{
			double Tf, BankOpacity; In >> Tf >> BankOpacity;
			if (K == 3 && HasWind) Wd.Set(WA, WV);
			T = std::fmod(T + 0.25, 10000.0);
			TCAtmosCore::Drift(DX, DY, Wd.X, Wd.Z, 0.25);
			std::vector<FMat4> Birds, Mist, BankM;
			for (const auto& B : Flight)
			{
				double X, Z, Up, Yaw, Roll;
				TCAtmosCore::BirdPose(B, T, X, Z, Up, Yaw, Roll);
				Birds.push_back(Compose(X, Wave(X, Z) + Up, Z, QuatYXZ(0, Yaw, Roll), 1.3, 1.3, 1.3));
			}
			for (size_t S = 0; S < Sites.size() && S < 36; S++)
			{
				double X, Y, Z, Yaw, SX, SZ;
				TCAtmosCore::SheetPose(Sites[S], T, false, 0, X, Y, Z, Yaw, SX, SZ);
				Mist.push_back(Compose(X, Y, Z, QuatYXZ(0, Yaw, 0), SX, 1, SZ));
			}
			const double Lift = (1 - BankOpacity / 0.34) * 3;
			for (size_t S = 0; S < Banks.size() && S < 64; S++)
			{
				double X, Y, Z, Yaw, SX, SZ;
				TCAtmosCore::SheetPose(Banks[S], T, true, Lift, X, Y, Z, Yaw, SX, SZ);
				BankM.push_back(Compose(X, Y, Z, QuatYXZ(0, Yaw, 0), SX, 1, SZ));
			}
			if (K) O.S << ',';
			O.S << "{\"drift\":"; O.Arr({ DX, DY, std::fmod(DX * 1.6 + 0.37, 1.0), std::fmod(DY * 1.9 + 0.61, 1.0) });
			O.S << ",\"birds\":"; O.Mats(Birds); O.S << ",\"mist\":"; O.Mats(Mist); O.S << ",\"banks\":"; O.Mats(BankM);
			// the fall's velocity: the wind it started in (start: the default wind)
			O.S << ",\"vel\":{";
			for (int F = 0; F < 3; F++) { double V[3]; TCAtmosCore::FallVelocity(TCAtmosCore::Fall(F), 0.55, 0.25, V); if (F) O.S << ','; O.S << '"' << TCAtmosCore::Fall(F).Kind << "\":"; O.Arr({ V[0], V[1], V[2] }); }
			O.S << "}}";
		}
		O.S << "]}";
	}
	O.S << "]}";
	std::ofstream Out(Argv[2]);
	Out << O.S.str();
	std::printf("harness: wrote %s\n", Argv[2]);
	return 0;
}
