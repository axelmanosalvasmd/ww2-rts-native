// The standalone check of the painted ground's engine-free core (Source/ThreeCrossroads/World/TCGroundPaint.h) against
// the Node oracle (../ground-oracle.mjs). For every paint the oracle recorded it replays the same start and cell rows
// through the core and compares:
//   attrs   every cell attribute (cellAttrs, biomeTint, near, depth, the field map), byte for byte;
//   raster  every canvas byte before the vector overlays, byte for byte (Reference knowledge mode, as the browser);
//   ops     the overlays (shell holes, trench cuts, contour lines) and the decals, written as JSON for compare.mjs.
// Native knowledge mode is run too: on World maps it must not depend on unknown cells (the placeholder rows, levels and
// layers of every unknown cell are scrambled and the canvas must not change).
//   ground-harness <oracle dir> <out dir> [--tiles <dir>] [--images <case>,...]
#include "World/TCGroundPaint.h"
#include "World/TCGroundPaint.Tiles.inl"
#include <cstdio>
#include <fstream>
#include <map>
#include <sstream>
#include <set>

using namespace TCGroundPaintCore;

struct FPaintStep { std::vector<FCellRow> Rows; int K = 0; bool bStart = false; };
struct FCase { std::string Name; FStartMap Map; std::vector<FPaintStep> Steps; };

static std::vector<uint8_t> ReadFile(const std::string& P)
{
	std::ifstream F(P, std::ios::binary);
	return std::vector<uint8_t>((std::istreambuf_iterator<char>(F)), std::istreambuf_iterator<char>());
}

static std::vector<FCase> ReadInput(const std::string& Path)
{
	std::ifstream In(Path);
	std::vector<FCase> Cases;
	std::string Line;
	FCase* Cur = nullptr;
	std::vector<FCellRow> Pending;
	bool bStarted = false;
	while (std::getline(In, Line))
	{
		std::istringstream L(Line);
		std::string Tag; L >> Tag;
		if (Tag == "CASE") { Cases.emplace_back(); Cur = &Cases.back(); L >> Cur->Name; Pending.clear(); bStarted = false; }
		else if (Tag == "MAP") { int Wd; L >> Cur->Map.W >> Cur->Map.H >> Wd; Cur->Map.bWorld = Wd != 0; }
		else if (Tag == "ROW") Cur->Map.Rows.push_back(Line.substr(4));
		else if (Tag == "HEI") Cur->Map.Heights.push_back(Line.substr(4));
		else if (Tag == "LG") Cur->Map.LayerGround.push_back(Line.substr(3));
		else if (Tag == "LO") Cur->Map.LayerObjects.push_back(Line.substr(3));
		else if (Tag == "SPAWN") { std::string X; L >> X; FStartMap::FXY S; if (X == "null") S.bNull = true; else { S.X = std::stoi(X); L >> S.Y; } Cur->Map.Spawns.push_back(S); }
		else if (Tag == "POINT") { FStartMap::FXY S; L >> S.X >> S.Y; Cur->Map.Points.push_back(S); }
		else if (Tag == "BIOMEMAP") { int B; while (L >> B) Cur->Map.Biome.push_back(B); }
		else if (Tag == "CELL")
		{
			FCellRow R; std::string Ch, Lv, St, G, O, B, M; int D;
			L >> R.C >> Ch >> Lv >> St >> D >> G >> O >> B >> M;
			R.Ch = (uint8_t)Ch[0];
			if (Lv != "-") { R.bLevel = true; R.Level = std::stoi(Lv); }
			if (St != "-") { R.bState = true; R.State = std::stoi(St); }
			R.bData = D != 0;
			if (R.bData) { R.Ground = G == "-" ? 0 : (uint8_t)G[0]; R.Object = O == "-" ? 0 : (uint8_t)O[0]; R.Biome = B == "-" ? -1 : std::stoi(B); if (M != "-") { R.bMaterial = true; R.Material = M; } }
			Pending.push_back(R);
		}
		else if (Tag == "PAINT") { FPaintStep S; L >> S.K; S.Rows.swap(Pending); S.bStart = !bStarted; bStarted = true; Cur->Steps.push_back(std::move(S)); }
		else if (Tag == "STEP" || Tag == "END") {}
	}
	return Cases;
}

// the material tiles the game ships (World/TCGroundPaint.Tiles.inl), for the textured run against the browser's canvas
static bool LoadTiles(int P, FTile Out[MatCount - 1])
{
	for (int M = 0; M < MatCount - 1; M++)
	{
		const int T = TileSize(M, P);
		const TCGroundPaintTiles::FEntry* Found = nullptr;
		for (const auto& E : TCGroundPaintTiles::Entries) if (E.Material == M && E.Size == T) Found = &E;
		if (!Found) { fprintf(stderr, "missing tile %s-%d\n", Look(M).Tex, T); return false; }
		Out[M].T = T; Out[M].Data.resize((size_t)T * T * 4);
		for (int I = 0; I < T * T; I++) { for (int K = 0; K < 3; K++) Out[M].Data[(size_t)I * 4 + K] = Found->Rgb[I * 3 + K]; Out[M].Data[(size_t)I * 4 + 3] = 255; }
	}
	return true;
}

static void WritePpm(const std::string& Path, const std::vector<uint8_t>& Rgba, int W, int H)
{
	FILE* F = fopen(Path.c_str(), "wb");
	fprintf(F, "P6\n%d %d\n255\n", W, H);
	std::vector<uint8_t> Row((size_t)W * 3);
	for (int Y = 0; Y < H; Y++) { for (int X = 0; X < W; X++) for (int K = 0; K < 3; K++) Row[(size_t)X * 3 + K] = Rgba[((size_t)Y * W + X) * 4 + K]; fwrite(Row.data(), 1, Row.size(), F); }
	fclose(F);
}

static std::string Num(double V) { char B[48]; snprintf(B, sizeof B, "%.9g", V); return B; }

static void WriteOps(const std::string& Path, const std::vector<FDraw>& Ops)
{
	std::ofstream O(Path);
	O << "[";
	for (size_t I = 0; I < Ops.size(); I++)
	{
		const FDraw& D = Ops[I];
		O << (I ? "," : "") << "{\"op\":\"" << (D.bStroke ? "stroke" : "fill") << "\",\"gradient\":" << (D.bGradient ? 1 : 0) << ",\"g\":[";
		for (int K = 0; K < 6; K++) O << (K ? "," : "") << Num(D.G[K]);
		O << "],\"stops\":[";
		for (size_t K = 0; K < D.Stops.size(); K++) O << (K ? "," : "") << "[" << Num(D.Stops[K].At) << "," << (int)D.Stops[K].R << "," << (int)D.Stops[K].G << "," << (int)D.Stops[K].B << "," << Num(D.Stops[K].A) << "]";
		O << "],\"lineWidth\":" << Num(D.LineWidth) << ",\"path\":[";
		for (size_t K = 0; K < D.Path.size(); K++)
		{
			const FPathOp& Op = D.Path[K];
			const int Nv = Op.Kind == 'A' ? 5 : 2;
			O << (K ? "," : "") << "[\"" << Op.Kind << "\"";
			for (int V = 0; V < Nv; V++) O << "," << Num(Op.V[V]);
			O << "]";
		}
		O << "],\"clip\":";
		if (D.bClip) O << "[" << Num(D.Clip[0]) << "," << Num(D.Clip[1]) << "," << Num(D.Clip[2]) << "," << Num(D.Clip[3]) << "]"; else O << "null";
		O << "}";
	}
	O << "]";
}

// shared/sim.js heightAt over the core's current levels (the oracle's decals stand on it too)
static double HeightAt(const FGround& G, double X, double Z)
{
	const int W = G.MapW(), H = G.MapH();
	const double Fx = std::min((double)W - 1, std::max(0.0, X / 2 - 0.5)), Fz = std::min((double)H - 1, std::max(0.0, Z / 2 - 0.5));
	const int X0 = (int)std::floor(Fx), Z0 = (int)std::floor(Fz), X1 = std::min(W - 1, X0 + 1), Z1 = std::min(H - 1, Z0 + 1);
	const double Tx = Fx - X0, Tz = Fz - Z0;
	auto Hh = [&](int Cx, int Cz) { return (double)G.LevelAt(Cz * W + Cx); };
	return (Hh(X0, Z0) * (1 - Tx) * (1 - Tz) + Hh(X1, Z0) * Tx * (1 - Tz) + Hh(X0, Z1) * (1 - Tx) * Tz + Hh(X1, Z1) * Tx * Tz) * 2.5;
}

struct FDecals { FDecalSet Rubble, Craters; };
static void UpdateDecals(const FGround& G, FDecals& D, const std::vector<int>& Changed, bool bWorld)
{
	const int W = G.MapW(), N = W * G.MapH();
	auto HAt = [&G](double X, double Z) { return HeightAt(G, X, Z); };
	D.Rubble.Update(RubbleLook(Changed, N), [&](int C) { return G.IsKnown(C) && G.ObjectAt(C) == 'R'; },
		[&](int Slot, int C) { RubbleQuad(C, W, G.MaterialAt(C), HAt, &D.Rubble.Pos[(size_t)Slot * 75], &D.Rubble.Uv[(size_t)Slot * 50]); });
	if (bWorld)
		D.Craters.Update(CraterLook(Changed, W, N), [&](int C) { return G.IsKnown(C) && CraterWanted(C, W, G.MapH(), [&G](int Cc) { return G.IsKnown(Cc) ? G.CharAt(Cc) : (uint8_t)0; }); },
			[&](int Slot, int C) { CraterQuad(C, W, HAt, &D.Craters.Pos[(size_t)Slot * 75]); });
}
static void WriteDecals(const std::string& Path, const FDecals& D, bool bWorld)
{
	std::ofstream O(Path);
	auto Arr = [&O](const std::vector<float>& V, size_t Count, double Scale)
	{
		O << "[";
		for (size_t I = 0; I < Count; I++) O << (I ? "," : "") << Num(std::round(V[I] * Scale) / Scale);
		O << "]";
	};
	O << "{\"rubble\":{\"count\":" << D.Rubble.Count() << ",\"positions\":";
	Arr(D.Rubble.Pos, (size_t)D.Rubble.Count() * 75, 1e5);
	O << ",\"uvs\":";
	Arr(D.Rubble.Uv, (size_t)D.Rubble.Count() * 50, 1e6);
	O << "}";
	if (bWorld) { O << ",\"craters\":{\"count\":" << D.Craters.Count() << ",\"positions\":"; Arr(D.Craters.Pos, (size_t)D.Craters.Count() * 75, 1e5); O << "}"; }
	O << "}";
}

static std::vector<int> TouchedRubble(const FGround& G, const std::vector<FCellRow>& Rows)
{
	// main.js applyCells: the changed cells, then rubble round any level change
	std::vector<int> Out; std::set<int> Seen;
	const int W = G.MapW(), H = G.MapH();
	for (const auto& R : Rows) if (Seen.insert(R.C).second) Out.push_back(R.C);
	for (const auto& R : Rows) if (R.bLevel)
	{
		const int X = R.C % W, Y = R.C / W;
		for (int Dz = -1; Dz <= 1; Dz++) for (int Dx = -1; Dx <= 1; Dx++)
		{
			const int Nx = X + Dx, Ny = Y + Dz;
			if (Nx >= 0 && Nx < W && Ny >= 0 && Ny < H && G.ObjectAt(Ny * W + Nx) == 'R' && Seen.insert(Ny * W + Nx).second) Out.push_back(Ny * W + Nx);
		}
	}
	return Out;
}

int main(int Argc, char** Argv)
{
	if (Argc < 3) { fprintf(stderr, "usage: ground-harness <oracle dir> <out dir> [--tiles <dir>] [--images a,b] [--only a,b]\n"); return 2; }
	setvbuf(stdout, nullptr, _IOLBF, 0);
	const std::string Dir = Argv[1], Out = Argv[2];
	std::string TilesDir, Images, Only;
	for (int I = 3; I + 1 < Argc; I += 2)
	{
		if (std::string(Argv[I]) == "--tiles") TilesDir = Argv[I + 1];
		if (std::string(Argv[I]) == "--images") Images = "," + std::string(Argv[I + 1]) + ",";
		if (std::string(Argv[I]) == "--only") Only = "," + std::string(Argv[I + 1]) + ",";
	}
	std::vector<FCase> Cases = ReadInput(Dir + "/input.txt");
	long DeferredBad = 0, TexturedBad = 0, Paints = 0, AttrBad = 0, RasterBadPaints = 0, RasterBadBytes = 0, ScrambleBad = 0;
	for (FCase& Case : Cases)
	{
		if (!Only.empty() && Only.find("," + Case.Name + ",") == std::string::npos) continue;
		FGround G; G.Mode = EKnownMode::Reference;
		FGround Nat; Nat.Mode = EKnownMode::Native;
		FGround Scr; Scr.Mode = EKnownMode::Native;
		FGround Tex; Tex.Mode = EKnownMode::Reference; // textured, as the browser paints once its pictures load
		FGround Def; Def.Mode = EKnownMode::Reference; Def.bDeferred = true; // the game's frame adapter: queued, 2 tiles a frame
		FDecals D; D.Rubble.Init(2048, true); D.Craters.Init(1536, false);
		FTile Tiles[MatCount - 1];
		bool bTiles = false;
		const bool bImages = Images.find("," + Case.Name + ",") != std::string::npos;
		for (const FPaintStep& S : Case.Steps)
		{
			if (S.bStart)
			{
				G.Start(Case.Map, S.Rows, nullptr);
				const int N = Case.Map.W * Case.Map.H;
				std::vector<int> All; for (int C = 0; C < N; C++) if (G.ObjectAt(C) == 'R') All.push_back(C);
				UpdateDecals(G, D, All, false);
				if (Case.Map.bWorld) { std::vector<int> Cr; for (int C = 0; C < N; C++) if (G.CharAt(C) == '+') Cr.push_back(C); UpdateDecals(G, D, {}, false); D.Craters.Update(Cr, [&](int C) { return CraterWanted(C, Case.Map.W, Case.Map.H, [&G](int Cc) { return G.CharAt(Cc); }); }, [&](int Slot, int C) { CraterQuad(C, Case.Map.W, [&G](double X, double Z) { return HeightAt(G, X, Z); }, &D.Craters.Pos[(size_t)Slot * 75]); }); }
				if (!TilesDir.empty()) bTiles = LoadTiles(G.PixelsPerCell(), Tiles);
				Nat.Start(Case.Map, S.Rows, bTiles ? Tiles : nullptr);
				if (bTiles) Tex.Start(Case.Map, S.Rows, Tiles);
				Def.Start(Case.Map, S.Rows, nullptr);
				if (Case.Map.bWorld)
				{
					// every cell no row describes gets a scrambled placeholder: Native must paint the same canvas
					FStartMap M2 = Case.Map;
					std::set<int> Rowed; for (const auto& R : S.Rows) Rowed.insert(R.C);
					for (const auto& St : Case.Steps) for (const auto& R : St.Rows) Rowed.insert(R.C);
					const char Junk[] = "WBR+TFDMH#OX";
					if (M2.LayerGround.size() != (size_t)M2.H) M2.LayerGround.assign((size_t)M2.H, std::string((size_t)M2.W, '.'));
					if (M2.LayerObjects.size() != (size_t)M2.H) M2.LayerObjects.assign((size_t)M2.H, std::string((size_t)M2.W, '.'));
					if (M2.Heights.size() != (size_t)M2.H) M2.Heights.assign((size_t)M2.H, std::string((size_t)M2.W, '0'));
					for (int C = 0; C < N; C++) if (!Rowed.count(C))
					{
						const int X = C % M2.W, Y = C / M2.W, K = (int)(Rnd(X, Y, 99) * 12);
						M2.Rows[(size_t)Y][(size_t)X] = Junk[K]; M2.LayerGround[(size_t)Y][(size_t)X] = K < 6 ? 'W' : 'M';
						M2.LayerObjects[(size_t)Y][(size_t)X] = K < 4 ? 'B' : '.'; M2.Heights[(size_t)Y][(size_t)X] = (char)('0' + K % 4);
					}
					if (M2.Biome.empty()) { M2.Biome.assign((size_t)N, 255); for (int C = 0; C < N; C++) if (!Rowed.count(C)) M2.Biome[(size_t)C] = C % 6; }
					Scr.Start(M2, S.Rows, bTiles ? Tiles : nullptr);
				}
			}
			else
			{
				G.Apply(S.Rows);
				UpdateDecals(G, D, TouchedRubble(G, S.Rows), Case.Map.bWorld);
				Nat.Apply(S.Rows);
				if (bTiles) Tex.Apply(S.Rows);
				Def.Apply(S.Rows);
				int Frames = 0;
				while (Def.PendingTiles()) { Def.PaintPending(2, nullptr); Frames++; }
				if (Def.RasterOnly() != G.RasterOnly()) { DeferredBad++; printf("  %s.%d deferred painting (%d frames) differs from the direct paint\n", Case.Name.c_str(), S.K, Frames); }
				else printf("  deferred: %d frames of 2 tiles, same raster\n", Frames);
				if (Case.Map.bWorld) Scr.Apply(S.Rows);
			}
			Paints++;
			const std::string Base = Dir + "/" + Case.Name + "." + std::to_string(S.K);
			// attributes
			const auto& A = G.Attributes();
			std::vector<uint8_t> Mine;
			auto Put = [&Mine](const void* P, size_t Bytes) { const uint8_t* B = (const uint8_t*)P; Mine.insert(Mine.end(), B, B + Bytes); };
			const size_t N = A.Prim.size();
			Put(A.Prim.data(), N); Put(A.Sec.data(), N); Put(A.Amt.data(), N * 4); Put(A.Lev.data(), N * 4); Put(A.Ov.data(), N); Put(A.Scar.data(), N);
			Put(A.Near.data(), N); Put(A.Depth.data(), N * 4); Put(A.Char.data(), N); Put(A.Tr.data(), N * 4); Put(A.Tg.data(), N * 4); Put(A.Tb.data(), N * 4); Put(G.FieldMap().data(), N);
			const std::vector<uint8_t> Theirs = ReadFile(Base + ".attrs");
			if (Mine != Theirs)
			{
				AttrBad++;
				const char* Names[] = { "prim", "sec", "amt", "lev", "ov", "scar", "near", "depth", "char", "tr", "tg", "tb", "fields" };
				const size_t Sz[] = { 1, 1, 4, 4, 1, 1, 1, 4, 1, 4, 4, 4, 1 };
				size_t Off = 0;
				for (int K = 0; K < 13; K++)
				{
					long Bad = 0; long First = -1;
					for (size_t I = 0; I < N; I++) if (Off + (I + 1) * Sz[K] <= Theirs.size() && std::memcmp(&Mine[Off + I * Sz[K]], &Theirs[Off + I * Sz[K]], Sz[K])) { if (First < 0) First = (long)I; Bad++; }
					if (Bad) printf("  %s.%d attrs %s: %ld cells differ (first %ld)\n", Case.Name.c_str(), S.K, Names[K], Bad, First);
					Off += N * Sz[K];
				}
			}
			// raster
			const std::vector<uint8_t> Ref = ReadFile(Base + ".raster");
			const std::vector<uint8_t>& Ras = G.RasterOnly();
			long Bad = 0, MaxD = 0, FirstBad = -1;
			if (Ref.size() != Ras.size()) { printf("  %s.%d raster size %zu vs %zu\n", Case.Name.c_str(), S.K, Ras.size(), Ref.size()); Bad = 1; }
			else for (size_t I = 0; I < Ras.size(); I++) if (Ras[I] != Ref[I]) { if (FirstBad < 0) FirstBad = (long)I; Bad++; MaxD = std::max(MaxD, (long)std::abs((int)Ras[I] - (int)Ref[I])); }
			if (Bad) { RasterBadPaints++; RasterBadBytes += Bad; printf("  %s.%d raster: %ld bytes differ (max %ld, first pixel %ld,%ld)\n", Case.Name.c_str(), S.K, Bad, MaxD, FirstBad >= 0 ? (FirstBad / 4) % G.CanvasW() : -1, FirstBad >= 0 ? (FirstBad / 4) / G.CanvasW() : -1); }
			WriteOps(Out + "/" + Case.Name + "." + std::to_string(S.K) + ".native-ops.json", G.LastOverlays());
			WriteDecals(Out + "/" + Case.Name + "." + std::to_string(S.K) + ".native-decals.json", D, Case.Map.bWorld);
			printf("%s.%d: P=%d %s%s attrs %s raster %s\n", Case.Name.c_str(), S.K, G.PixelsPerCell(), G.LastKind.c_str(), G.LastKind == "tiles" ? (" " + std::to_string(G.LastTiles)).c_str() : "",
				Mine == Theirs ? "exact" : "DIFFER", Bad ? "DIFFER" : "exact");
			if (Case.Map.bWorld)
			{
				const bool bSame = Nat.Canvas() == Scr.Canvas();
				if (!bSame) ScrambleBad++;
				long Unk = 0; for (int C = 0; C < Case.Map.W * Case.Map.H; C++) if (!Nat.IsKnown(C)) Unk++;
				printf("  native: %ld unknown cells; scrambled placeholders %s\n", Unk, bSame ? "change nothing" : "CHANGE THE CANVAS");
			}
			// the textured canvas against the browser's (overlays included there): exact wherever no overlay reaches
			// (a pixel and its 8 neighbours untouched by the native overlays), CIE76 Delta E on the overlay pixels
			const std::vector<uint8_t> Br = bTiles ? ReadFile(TilesDir + "/" + Case.Name + "." + std::to_string(S.K) + ".browser.rgba") : std::vector<uint8_t>();
			if (bTiles && Br.size() == Tex.Canvas().size())
			{
				const int CW = Tex.CanvasW(), CH = Tex.CanvasH();
				const auto& Ras = Tex.RasterOnly(); const auto& Can = Tex.Canvas();
				auto Touched = [&](int X, int Y) { const size_t I = ((size_t)Y * CW + X) * 4; return Ras[I] != Can[I] || Ras[I + 1] != Can[I + 1] || Ras[I + 2] != Can[I + 2]; };
				long Plain = 0, PlainBad = 0, Near = 0; double SumDe = 0, MaxDe = 0; std::vector<float> Des;
				auto Lab = [](const uint8_t* C, double Out[3])
				{
					double V[3];
					for (int K = 0; K < 3; K++) { const double U = C[K] / 255.0; V[K] = U <= 0.04045 ? U / 12.92 : std::pow((U + 0.055) / 1.055, 2.4); }
					const double X = (0.4124 * V[0] + 0.3576 * V[1] + 0.1805 * V[2]) / 0.95047, Y = 0.2126 * V[0] + 0.7152 * V[1] + 0.0722 * V[2], Z = (0.0193 * V[0] + 0.1192 * V[1] + 0.9505 * V[2]) / 1.08883;
					auto F = [](double T) { return T > 216.0 / 24389 ? std::cbrt(T) : (24389.0 / 27 * T + 16) / 116; };
					Out[0] = 116 * F(Y) - 16; Out[1] = 500 * (F(X) - F(Y)); Out[2] = 200 * (F(Y) - F(Z));
				};
				for (int Y = 0; Y < CH; Y++) for (int X = 0; X < CW; X++)
				{
					bool bNear = false;
					for (int Dy = -1; Dy <= 1 && !bNear; Dy++) for (int Dx = -1; Dx <= 1; Dx++) { const int Nx = X + Dx, Ny = Y + Dy; if (Nx >= 0 && Ny >= 0 && Nx < CW && Ny < CH && Touched(Nx, Ny)) { bNear = true; break; } }
					const size_t I = ((size_t)Y * CW + X) * 4;
					if (!bNear) { Plain++; if (Ras[I] != Br[I] || Ras[I + 1] != Br[I + 1] || Ras[I + 2] != Br[I + 2]) PlainBad++; continue; }
					Near++;
					double A[3], B[3]; Lab(&Can[I], A); Lab(&Br[I], B);
					const double De = std::sqrt((A[0] - B[0]) * (A[0] - B[0]) + (A[1] - B[1]) * (A[1] - B[1]) + (A[2] - B[2]) * (A[2] - B[2]));
					SumDe += De; MaxDe = std::max(MaxDe, De); Des.push_back((float)De);
				}
				std::sort(Des.begin(), Des.end());
				printf("  textured vs browser: %ld plain pixels, %ld differ; %ld overlay pixels, Delta E mean %.2f p95 %.2f max %.1f\n", Plain, PlainBad, Near,
					Near ? SumDe / Near : 0.0, Des.empty() ? 0.0 : (double)Des[(size_t)(Des.size() * 0.95)], MaxDe);
				if (PlainBad) TexturedBad++;
				if (bImages) WritePpm(Out + "/" + Case.Name + "." + std::to_string(S.K) + ".textured.ppm", Tex.Canvas(), CW, CH);
			}
			if (bImages)
			{
				WritePpm(Out + "/" + Case.Name + "." + std::to_string(S.K) + ".reference-mode.ppm", G.Canvas(), G.CanvasW(), G.CanvasH());
				WritePpm(Out + "/" + Case.Name + "." + std::to_string(S.K) + ".native.ppm", Nat.Canvas(), Nat.CanvasW(), Nat.CanvasH());
			}
		}
	}
	printf("RESULT %ld paints: attrs differ in %ld, raster differs in %ld (%ld bytes), World scramble changed %ld, textured plain pixels differ in %ld, deferred differs in %ld\n", Paints, AttrBad, RasterBadPaints, RasterBadBytes, ScrambleBad, TexturedBad, DeferredBad);
	return AttrBad || RasterBadPaints || ScrambleBad || TexturedBad || DeferredBad ? 1 : 0;
}
