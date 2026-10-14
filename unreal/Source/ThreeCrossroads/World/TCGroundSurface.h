// The one ground surface the client draws, places units on and picks against. Engine-free (plain C++ templates, no
// Unreal includes) so its numbers can be checked by a standalone harness.
//
// Each cell is drawn as a fan of 8 triangles around its centre over a half-cell grid of nodes: the 4 corners, the 4
// edge midpoints and the centre. A node's height is the mean level of the known cells touching it (1 at a centre, 2 at
// an edge midpoint, 4 at a corner, fewer at the map border). On a map where every cell is known that equals the
// reference heightAt (shared/sim.js) at every node, so cell centres keep their simulation height. Between nodes the
// surface is the planar triangle the mesh draws, never the bilinear patch, so units, clicks and the drawn ground agree.
//
// World Conquest: cells this side has not discovered never enter a node's mean, so known ground is not pulled toward
// the placeholder level 0 the server fills unknown cells with. A query inside an unknown cell (a unit that stepped
// past the sent cells) uses known nodes where the cell touches known ground, and elsewhere the mean level of its known
// neighbours (0 when it has none). Only data the client was sent is read.
#pragma once

namespace TCGround
{
	inline constexpr float CellMetres = 2.f;   // shared/sim.js CELL
	inline constexpr float LevelMetres = 2.5f; // shared/sim.js CFG.levelHeight

	inline int FloorToInt(float V) { const int I = (int)V; return V < (float)I ? I - 1 : I; }
	inline int ClampInt(int V, int Lo, int Hi) { return V < Lo ? Lo : V > Hi ? Hi : V; }
	inline float ClampFloat(float V, float Lo, float Hi) { return V < Lo ? Lo : V > Hi ? Hi : V; }

	// Level(Cell) -> level as a number, Known(Cell) -> whether this client may use it. Cells are Y * W + X.
	template <typename LevelFn, typename KnownFn>
	struct TSurface
	{
		int W = 0, H = 0;
		LevelFn Level;
		KnownFn Known;

		TSurface(int InW, int InH, LevelFn InLevel, KnownFn InKnown) : W(InW), H(InH), Level(InLevel), Known(InKnown) {}

		// Mean level of the known cells touching half-cell node (I, J), I in [0, 2W], J in [0, 2H]. False when none is known.
		bool NodeLevel(int I, int J, float& Out) const
		{
			if (W <= 0 || H <= 0) return false;
			I = ClampInt(I, 0, 2 * W); J = ClampInt(J, 0, 2 * H);
			// an odd index is a cell centre column (one cell), an even one is a cell edge (the cells on both sides)
			const int XA = (I & 1) ? (I - 1) / 2 : I / 2 - 1, XB = (I & 1) ? XA : I / 2;
			const int ZA = (J & 1) ? (J - 1) / 2 : J / 2 - 1, ZB = (J & 1) ? ZA : J / 2;
			float Sum = 0.f;
			int N = 0;
			for (int Z = ZA; Z <= ZB; Z++)
			{
				if (Z < 0 || Z >= H) continue;
				for (int X = XA; X <= XB; X++)
				{
					if (X < 0 || X >= W) continue;
					const int C = Z * W + X;
					if (!Known(C)) continue;
					Sum += (float)Level(C);
					N++;
				}
			}
			if (!N) return false;
			Out = Sum / (float)N;
			return true;
		}

		// The level an unknown cell stands in for: the mean of its known 8 neighbours, 0 when it has none.
		float GuessLevel(int CX, int CZ) const
		{
			float Sum = 0.f;
			int N = 0;
			for (int Z = CZ - 1; Z <= CZ + 1; Z++) for (int X = CX - 1; X <= CX + 1; X++)
			{
				if (X < 0 || Z < 0 || X >= W || Z >= H || (X == CX && Z == CZ)) continue;
				const int C = Z * W + X;
				if (Known(C)) { Sum += (float)Level(C); N++; }
			}
			return N ? Sum / (float)N : 0.f;
		}

		// Node level as cell (CX, CZ) draws it: a node no known cell touches takes the cell's own level (or its guess).
		float NodeFor(int CX, int CZ, int I, int J) const
		{
			float L;
			if (NodeLevel(I, J, L)) return L;
			const int C = CZ * W + CX;
			return Known(C) ? (float)Level(C) : GuessLevel(CX, CZ);
		}

		// Ground level (in levels, not metres) at reference metres (X, Z), clamped to the map like heightAt.
		float LevelAt(float X, float Z) const
		{
			if (W <= 0 || H <= 0) return 0.f;
			const float U = ClampFloat(X / CellMetres, 0.f, (float)W), V = ClampFloat(Z / CellMetres, 0.f, (float)H);
			const int CX = ClampInt(FloorToInt(U), 0, W - 1), CZ = ClampInt(FloorToInt(V), 0, H - 1);
			// offsets from the cell centre in half cells: -1 at the west/north edge, +1 at the east/south edge
			const float DX = (U - (float)CX) * 2.f - 1.f, DZ = (V - (float)CZ) * 2.f - 1.f;
			const int SX = DX < 0.f ? -1 : 1, SZ = DZ < 0.f ? -1 : 1;
			const float S = DX * (float)SX, T = DZ * (float)SZ; // 0 at the centre, 1 at the edge on this side
			const int CI = 2 * CX + 1, CJ = 2 * CZ + 1;
			const float Hc = NodeFor(CX, CZ, CI, CJ);
			const float Hx = NodeFor(CX, CZ, CI + SX, CJ);      // midpoint of the east or west edge
			const float Hz = NodeFor(CX, CZ, CI, CJ + SZ);      // midpoint of the north or south edge
			const float Hk = NodeFor(CX, CZ, CI + SX, CJ + SZ); // the corner
			// the sub-quad is split along its centre-to-corner diagonal, as the mesh fan draws it
			return S >= T ? Hc + S * (Hx - Hc) + T * (Hk - Hx) : Hc + T * (Hz - Hc) + S * (Hk - Hz);
		}

		float HeightAt(float X, float Z) const { return LevelAt(X, Z) * LevelMetres; }
	};

	template <typename LevelFn, typename KnownFn>
	TSurface<LevelFn, KnownFn> MakeSurface(int W, int H, LevelFn Level, KnownFn Known) { return TSurface<LevelFn, KnownFn>(W, H, Level, Known); }

	// The 9 nodes of a cell in mesh order (offsets in half cells from its north-west corner): the centre, then the
	// boundary going round. Fan triangle K is (centre, ring K, ring K + 1).
	inline constexpr int CellNodeCount = 9;
	inline constexpr int CellTriangleCount = 8;
	inline constexpr int NodeDI[CellNodeCount] = { 1, 0, 1, 2, 2, 2, 1, 0, 0 };
	inline constexpr int NodeDJ[CellNodeCount] = { 1, 0, 0, 0, 1, 2, 2, 2, 1 };
}
