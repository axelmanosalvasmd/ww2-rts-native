// The engine-free relief (Source/ThreeCrossroads/World/TCRelief.h) on one case of the oracle's inputs; see run.sh.
//   relief-harness <case.in> <outdir> <name>
// <case.in>: "W H low edits" then lines: levels, grid, roads, known, frozen (each W*H values), then for an incremental
// case the base case's five lines and the edited cell list. Writes, for the Reference layout with relief.js queries:
// <name>.ref.{pos,nrm,uv,paint,scar,idx,hat,hits,water}.bin, and for the game's layout (32-cell chunks, drawn picks):
// <name>.game.{hat,hits,mesh-hat,mesh-hits}.bin (mesh-*: brute force over the drawn non-rock triangles) and, for an
// incremental case, whether the chunked incremental result equals a chunked fresh build.
#include "World/TCRelief.h"
#include <cmath>
#include <cstdio>
#include <fstream>
#include <iostream>
#include <limits>
#include <sstream>
#include <string>
#include <vector>

using namespace TCRelief;

struct FIn { int W = 0, H = 0, Low = 0; std::vector<int8_t> Lv, Fz; std::vector<uint8_t> Gr, Rd, Kn; };

static void ReadLines(std::istream& In, FIn& F)
{
	const size_t Nn = (size_t)F.W * F.H;
	F.Lv.resize(Nn); F.Fz.resize(Nn);
	for (size_t I = 0; I < Nn; I++) { int V; In >> V; F.Lv[I] = (int8_t)V; }
	std::string G, R, K; In >> G >> R >> K;
	F.Gr.assign(G.begin(), G.end()); F.Rd.resize(Nn); F.Kn.resize(Nn);
	for (size_t I = 0; I < Nn; I++) { F.Rd[I] = R[I] == '1'; F.Kn[I] = K[I] == '1'; }
	for (size_t I = 0; I < Nn; I++) { int V; In >> V; F.Fz[I] = (int8_t)V; }
}
static void Apply(FRelief& R, const FIn& F, bool bFrozen)
{
	R.InLevel = F.Lv; R.InGrid = F.Gr; R.InRoad = F.Rd; R.InKnown = F.Kn;
	if (bFrozen) R.InFrozen = F.Fz;
}
template <typename T> static void Write(const std::string& Path, const std::vector<T>& V)
{
	std::ofstream O(Path, std::ios::binary);
	O.write((const char*)V.data(), (std::streamsize)(V.size() * sizeof(T)));
}
template <typename T> static std::vector<T> Read(const std::string& Path)
{
	std::ifstream I(Path, std::ios::binary | std::ios::ate);
	std::vector<T> V((size_t)I.tellg() / sizeof(T));
	I.seekg(0); I.read((char*)V.data(), (std::streamsize)(V.size() * sizeof(T)));
	return V;
}

// every drawn ground triangle (rock models excluded: paint 0.98) bucketed by the cells it overlaps
struct FMeshIndex
{
	int W, H;
	std::vector<double> T; // 9 per triangle
	std::vector<std::vector<int>> Cells;
	explicit FMeshIndex(const FRelief& R) : W(R.W), H(R.H), Cells((size_t)R.W * R.H)
	{
		for (int P = 0; P < R.NumParts(); P++)
		{
			const FPart& Pt = R.Part(P);
			for (size_t K = 0; K < Pt.Idx.size(); K += 3)
			{
				const uint32_t A = Pt.Idx[K];
				const float Rock = Pt.Paint[(size_t)A * 4];
				if (Rock > 0 && Rock < 1) continue;
				const int Id = (int)(T.size() / 9);
				double Mn[2] = { 1e30, 1e30 }, Mx[2] = { -1e30, -1e30 };
				for (int V = 0; V < 3; V++)
				{
					const uint32_t Ix = Pt.Idx[K + V];
					for (int D = 0; D < 3; D++) T.push_back(Pt.Pos[(size_t)Ix * 3 + D]);
					Mn[0] = std::min(Mn[0], (double)Pt.Pos[(size_t)Ix * 3]); Mx[0] = std::max(Mx[0], (double)Pt.Pos[(size_t)Ix * 3]);
					Mn[1] = std::min(Mn[1], (double)Pt.Pos[(size_t)Ix * 3 + 2]); Mx[1] = std::max(Mx[1], (double)Pt.Pos[(size_t)Ix * 3 + 2]);
				}
				const int X0 = std::max(0, (int)std::floor(Mn[0] / Cell)), X1 = std::min(W - 1, (int)std::floor(Mx[0] / Cell));
				const int Z0 = std::max(0, (int)std::floor(Mn[1] / Cell)), Z1 = std::min(H - 1, (int)std::floor(Mx[1] / Cell));
				for (int Z = Z0; Z <= Z1; Z++) for (int X = X0; X <= X1; X++) Cells[(size_t)(Z * W + X)].push_back(Id);
			}
		}
	}
	// the highest upward-facing triangle over (x, z), NaN if none
	double Top(double X, double Z) const
	{
		const int Cx = std::min(W - 1, std::max(0, (int)std::floor(X / Cell))), Cz = std::min(H - 1, std::max(0, (int)std::floor(Z / Cell)));
		double Best = -1e30;
		for (const int Id : Cells[(size_t)(Cz * W + Cx)])
		{
			const double* P = &T[(size_t)Id * 9];
			const double Bx = P[3] - P[0], Bz = P[5] - P[2], Dx = P[6] - P[0], Dz = P[8] - P[2];
			const double Det = Bx * Dz - Bz * Dx;
			if (Det >= -1e-10) continue;
			const double Px = X - P[0], Pz = Z - P[2];
			const double S = (Px * Dz - Pz * Dx) / Det, Tt = (Bx * Pz - Bz * Px) / Det;
			if (S >= -1e-7 && Tt >= -1e-7 && S + Tt <= 1 + 1e-7) Best = std::max(Best, P[1] + (P[4] - P[1]) * S + (P[7] - P[1]) * Tt);
		}
		return Best > -1e29 ? Best : std::numeric_limits<double>::quiet_NaN();
	}
	// the first hit of a ray over every indexed triangle (both faces count: a ray from below a lip is not a pick)
	bool First(const double O[3], const double D[3], double Out[3]) const
	{
		double Best = 1e30;
		for (size_t K = 0; K < T.size(); K += 9)
		{
			const double* A = &T[K]; const double* B = A + 3; const double* C = A + 6;
			const double E1[3] = { B[0] - A[0], B[1] - A[1], B[2] - A[2] }, E2[3] = { C[0] - A[0], C[1] - A[1], C[2] - A[2] };
			const double Pv[3] = { D[1] * E2[2] - D[2] * E2[1], D[2] * E2[0] - D[0] * E2[2], D[0] * E2[1] - D[1] * E2[0] };
			const double Det = E1[0] * Pv[0] + E1[1] * Pv[1] + E1[2] * Pv[2];
			if (Det < 1e-12) continue; // front faces only (counter-clockwise seen from the ray origin)
			const double Inv = 1 / Det, Tv[3] = { O[0] - A[0], O[1] - A[1], O[2] - A[2] };
			const double U = (Tv[0] * Pv[0] + Tv[1] * Pv[1] + Tv[2] * Pv[2]) * Inv;
			if (U < -1e-9 || U > 1 + 1e-9) continue;
			const double Q[3] = { Tv[1] * E1[2] - Tv[2] * E1[1], Tv[2] * E1[0] - Tv[0] * E1[2], Tv[0] * E1[1] - Tv[1] * E1[0] };
			const double V = (D[0] * Q[0] + D[1] * Q[1] + D[2] * Q[2]) * Inv;
			if (V < -1e-9 || U + V > 1 + 1e-9) continue;
			const double Tt = (E2[0] * Q[0] + E2[1] * Q[1] + E2[2] * Q[2]) * Inv;
			if (Tt > 0 && Tt < Best) Best = Tt;
		}
		if (Best >= 1e30) return false;
		for (int I = 0; I < 3; I++) Out[I] = O[I] + D[I] * Best;
		return true;
	}
};

static void Queries(const FRelief& R, const std::vector<double>& Pts, const std::vector<double>& Rays, std::vector<double>& Hat, std::vector<double>& Hits)
{
	Hat.resize(Pts.size() / 2);
	for (size_t I = 0; I < Hat.size(); I++) Hat[I] = R.HAt(Pts[I * 2], Pts[I * 2 + 1]);
	Hits.assign(Rays.size() / 2, std::numeric_limits<double>::quiet_NaN());
	for (size_t I = 0; I < Rays.size() / 6; I++)
	{
		double Out[3];
		if (R.Ray(&Rays[I * 6], &Rays[I * 6 + 3], std::numeric_limits<double>::infinity(), Out)) for (int K = 0; K < 3; K++) Hits[I * 3 + K] = Out[K];
	}
}

static bool SameParts(const FRelief& A, const FRelief& B)
{
	if (A.NumParts() != B.NumParts()) return false;
	for (int P = 0; P < A.NumParts(); P++)
	{
		const FPart& X = A.Part(P); const FPart& Y = B.Part(P);
		if (X.Pos != Y.Pos || X.Nrm != Y.Nrm || X.Idx != Y.Idx || X.Paint != Y.Paint || X.Scar != Y.Scar || X.Uv != Y.Uv) { std::fprintf(stderr, "part %d differs\n", P); return false; }
	}
	return true;
}

int main(int Argc, char** Argv)
{
	if (Argc < 5) { std::fprintf(stderr, "usage: relief-harness <case.in> <oracle dir> <out dir> <name>\n"); return 2; }
	std::ifstream In(Argv[1]);
	const std::string Oracle = Argv[2], OutDir = Argv[3], Name = Argv[4];
	FIn F, Base;
	int Edits = 0;
	In >> F.W >> F.H >> F.Low >> Edits;
	ReadLines(In, F);
	std::vector<int> EditCells;
	if (Edits)
	{
		Base.W = F.W; Base.H = F.H; Base.Low = F.Low;
		ReadLines(In, Base);
		int Count; In >> Count;
		EditCells.resize((size_t)Count);
		for (int& C : EditCells) In >> C;
	}
	const std::vector<double> Pts = Read<double>(Oracle + "/" + Name + ".pts.bin"), Rays = Read<double>(Oracle + "/" + Name + ".rays.bin");
	const std::string P = OutDir + "/" + Name;
	// 1. Reference layout and queries
	{
		FRelief R;
		R.Reset(F.W, F.H, F.Low != 0, 0, false);
		if (Edits) { Apply(R, Base, true); R.Build(); Apply(R, F, false); R.Update(EditCells); }
		else { Apply(R, F, true); R.Build(); }
		std::vector<float> Pos, Nrm, Uv, Paint, Scar; std::vector<uint32_t> Idx;
		for (int K = 0; K < R.NumParts(); K++)
		{
			const FPart& Pt = R.Part(K);
			const uint32_t Base0 = (uint32_t)(Pos.size() / 3);
			Pos.insert(Pos.end(), Pt.Pos.begin(), Pt.Pos.end()); Nrm.insert(Nrm.end(), Pt.Nrm.begin(), Pt.Nrm.end()); Uv.insert(Uv.end(), Pt.Uv.begin(), Pt.Uv.end());
			Paint.insert(Paint.end(), Pt.Paint.begin(), Pt.Paint.end()); Scar.insert(Scar.end(), Pt.Scar.begin(), Pt.Scar.end());
			for (const uint32_t I : Pt.Idx) Idx.push_back(I + Base0);
		}
		Write(P + ".ref.pos.bin", Pos); Write(P + ".ref.nrm.bin", Nrm); Write(P + ".ref.uv.bin", Uv); Write(P + ".ref.paint.bin", Paint); Write(P + ".ref.scar.bin", Scar); Write(P + ".ref.idx.bin", Idx);
		std::vector<double> Hat, Hits, Water;
		Queries(R, Pts, Rays, Hat, Hits);
		for (int C = 0; C < F.W * F.H; C++) Water.push_back(R.WaterAt((C % F.W + 0.5) * Cell, (C / F.W + 0.5) * Cell));
		Write(P + ".ref.hat.bin", Hat); Write(P + ".ref.hits.bin", Hits); Write(P + ".ref.water.bin", Water);
		std::printf("ref %d vertices %d triangles %d cliff edges\n", R.Stats.Vertices, R.Stats.Triangles, R.Stats.CliffEdges);
	}
	// 2. the game's layout: 32-cell chunks, drawn picks
	{
		FRelief R;
		R.Reset(F.W, F.H, F.Low != 0, 32, true);
		int Fresh = -1;
		if (Edits)
		{
			Apply(R, Base, true); R.Build(); R.TakeRebuilt(); Apply(R, F, false); R.Update(EditCells);
			const int Rebuilt = (int)R.TakeRebuilt().size();
			FRelief Q;
			Q.Reset(F.W, F.H, F.Low != 0, 32, true);
			Apply(Q, Base, true); Q.InLevel = F.Lv; Q.InGrid = F.Gr; Q.InRoad = F.Rd; Q.InKnown = F.Kn; // fresh from the new state, old frozen lines
			// relief.js reads roads only for dirty cells on update; a fresh build reads them all, so compare on the cells read
			Q.Build();
			Fresh = SameParts(R, Q) ? 1 : 0;
			std::printf("game incremental: %d chunks rebuilt of %d, equals a fresh build: %d\n", Rebuilt, R.NumParts(), Fresh);
		}
		else { Apply(R, F, true); R.Build(); }
		std::vector<double> Hat, Hits;
		Queries(R, Pts, Rays, Hat, Hits);
		const FMeshIndex Mesh(R);
		std::vector<double> MeshHat(Pts.size() / 2), MeshHits(Rays.size() / 2, std::numeric_limits<double>::quiet_NaN());
		for (size_t I = 0; I < MeshHat.size(); I++) MeshHat[I] = Mesh.Top(Pts[I * 2], Pts[I * 2 + 1]);
		const size_t RayN = std::min<size_t>(Rays.size() / 6, 200);
		for (size_t I = 0; I < RayN; I++) { double O[3]; if (Mesh.First(&Rays[I * 6], &Rays[I * 6 + 3], O)) for (int K = 0; K < 3; K++) MeshHits[I * 3 + K] = O[K]; }
		MeshHits.resize(RayN * 3);
		Write(P + ".game.hat.bin", Hat); Write(P + ".game.hits.bin", Hits); Write(P + ".game.mesh-hat.bin", MeshHat); Write(P + ".game.mesh-hits.bin", MeshHits);
		std::vector<int8_t> Flags((size_t)F.W * F.H);
		for (int C = 0; C < F.W * F.H; C++) Flags[(size_t)C] = (int8_t)((R.IsRagged(C) ? 1 : 0) | (R.IsShoved(C) ? 2 : 0));
		Write(P + ".game.flags.bin", Flags);
		std::printf("game %d vertices %d triangles %d chunks fresh %d\n", R.Stats.Vertices, R.Stats.Triangles, R.NumParts(), Fresh);
	}
	return 0;
}
