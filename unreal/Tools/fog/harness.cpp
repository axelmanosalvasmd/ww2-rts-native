// Standalone check of TCFogFrontierCore (World/TCFogFrontier.h, engine-free part) against the Node oracle that runs
// the unmodified client/fog.js. g++ -std=c++17; reads harness-input.txt (to-input.mjs).
#define TC_FOG_FRONTIER_CORE_ONLY
#include "TCFogFrontier.h"
#include <cstdio>
#include <fstream>
#include <iostream>
#include <string>
using namespace TCFogFrontierCore;
static std::ifstream In;
static std::vector<int> ReadArr() { int n; In >> n; std::vector<int> a(n); for (int& v : a) In >> v; return a; }
int main(int argc, char** argv)
{
	In.open(argc > 1 ? argv[1] : "harness-input.txt");
	long checked = 0, off = 0;
	int nb; In >> nb;
	int tableOff = 0;
	for (int i = 0; i < nb; i++) { std::string t; int s; In >> t >> s; if (BuildingSize(t.c_str()) != s) { tableOff++; std::printf("building %s: table %d, reference %d\n", t.c_str(), BuildingSize(t.c_str()), s); } }
	int ours = (int)(sizeof(BuildingSizes) / sizeof(BuildingSizes[0]));
	std::printf("building table: %d reference entries, %d here, %d differ\n", nb, ours, tableOff);
	int ns; In >> ns;
	for (int si = 0; si < ns; si++)
	{
		std::string name; int w, h, opaque, offF, hasKey; In >> name >> w >> h >> opaque >> offF >> hasKey;
		std::vector<uint8_t> seen(w * h, 0), expl(w * h, 0);
		if (hasKey) { for (int c : ReadArr()) seen[c] = 1; for (int c : ReadArr()) expl[c] = 1; }
		FFog F; F.Start(w, h, hasKey ? seen.data() : nullptr, hasKey ? expl.data() : nullptr, offF, opaque);
		long sOff = 0, sChk = 0; double worst = 0;
		auto cmp = [&](const std::vector<int>& want, auto got, const char* what, int step) {
			for (int c = 0; c < w * h; c++) { const int g = got(c); sChk++; if (g != want[c]) { if (sOff < 5) std::printf("  %s step %d cell %d: got %d want %d\n", what, step, c, g, want[c]); sOff++; worst = std::max(worst, (double)std::abs(g - want[c])); } }
		};
		cmp(ReadArr(), [&](int c) { return (int)F.Bytes[c]; }, "initial alpha", -1);
		cmp(ReadArr(), [&](int c) { return F.At(c); }, "initial look", -1);
		cmp(ReadArr(), [&](int c) { return F.MinimapAlpha(c); }, "initial minimap", -1);
		int nsteps; In >> nsteps;
		for (int k = 0; k < nsteps; k++)
		{
			std::vector<int> flips = ReadArr();
			int nu; In >> nu;
			std::vector<FFootprint> units;
			for (int u = 0; u < nu; u++) { std::string t; double x, z; In >> t >> x >> z; const int b = BuildingSize(t.c_str()); units.push_back(Footprint(x, z, b > 0 ? b : 1)); }
			F.Snapshot(flips, units);
			int lift, nf; In >> lift >> nf;
			for (int f = 0; f < nf; f++) { double dt; In >> dt; F.Frame(dt, lift); cmp(ReadArr(), [&](int c) { return (int)F.Bytes[c]; }, "frame alpha", k); }
			F.Frame(1.0, lift);
			cmp(ReadArr(), [&](int c) { return (int)F.Bytes[c]; }, "settled alpha", k);
			cmp(ReadArr(), [&](int c) { return F.At(c); }, "look", k);
			cmp(ReadArr(), [&](int c) { return F.MinimapAlpha(c); }, "minimap", k);
		}
		std::printf("%-32s %3dx%-3d steps %2d: %8ld values, %ld off (worst %g)\n", name.c_str(), w, h, nsteps, sChk, sOff, worst);
		checked += sChk; off += sOff;
	}
	std::printf("TOTAL %ld values, %ld off\n", checked, off);
	return off || tableOff ? 1 : 0;
}
