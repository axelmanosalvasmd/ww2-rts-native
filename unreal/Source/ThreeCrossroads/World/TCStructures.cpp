// client/structures.js at e4e579a. The original builders moved out of TCWorldObjects without remodeling.
// This file is engine-free so source-oracle harnesses and Unreal run the same implementation.
#include "World/TCStructures.h"
#include "World/TCWorldObjects.h"

namespace TCStructures
{
	using namespace TCWorldCore;
	// The piece builders of client/structures.js, line for line. Out receives each put() in call order.
	class FBuilder
	{
	public:
		FBuilder(const FCellSource& InSrc, std::vector<FPiece>& InOut) : Src(InSrc), Out(InOut), W(InSrc.Width()), H(InSrc.Height()) {}
		// the footprint (structures.js C.foot) for isHouse, and the house tints for rubble (cell -> index, -1 none)
		const std::vector<unsigned char>* Foot = nullptr;
		const std::vector<int>* TintIndex = nullptr;
		const std::vector<FHouseTint>* Tints = nullptr;

		// ---------- houses (piece: a standing house, a ruin, or nothing when every cell has fallen) ----------
		void House(const FHouseRect& R)
		{
			int Standing = 0, Cells = 0;
			for (int Y = R.R.Y0; Y < R.R.Y1; Y++) for (int X = R.R.X0; X < R.R.X1; X++) { Cells++; if (Src.At(X, Y) == 'B') Standing++; }
			if (!Standing) return;
			Rect = R.R; bInRect = true;
			const FHouseSpec Sp = SpecOf(R);
			if (Standing < Cells) Ruin(R, Sp); else Standing_(R, Sp);
			bInRect = false;
		}

		// ---------- cells: one 32 x 32 part, builder by builder (rebuild: hedges, walls, trenches, wire, traps, mines,
		// hospitals, rubble, bridges) ----------
		void Chunk(int X0, int Y0, int X1, int Y1)
		{
			CX0 = X0; CY0 = Y0; CX1 = X1; CY1 = Y1;
			Hedges(); Walls(); Trenches(); Wire(); Traps(); Mines(); Hospitals(); Rubble(); Bridges();
		}

	private:
		const FCellSource& Src;
		std::vector<FPiece>& Out;
		int W, H;
		int CX0 = 0, CY0 = 0, CX1 = 0, CY1 = 0;
		int CurCell = -1;
		FCellRect Rect;
		bool bInRect = false;
		static constexpr int Dir4[4][2] = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } };
		static constexpr int Diag[4][2] = { { 1, 1 }, { 1, -1 }, { -1, 1 }, { -1, -1 } };

		unsigned char At(int X, int Y) const { return Src.At(X, Y); }
		double G(double X, double Z) const { return Src.Ground(X, Z); }
		bool Low() const { return Src.Low(); }
		int StageAt(int X, int Y) const { return Src.Stage(X, Y); }
		// structures.js put
		void Put(EPiece Kind, double X, double Y, double Z, double Sx, double Sy, double Sz, double Ry, const FRgb& C, double Rx = 0, double Rz = 0)
		{
			FPiece P;
			P.Kind = Kind;
			if (bInRect)
			{
				// the house cell nearest the piece: its fog dims it
				const int Cx = std::min(Rect.X1 - 1, std::max(Rect.X0, (int)std::floor(X / CellMetres)));
				const int Cz = std::min(Rect.Y1 - 1, std::max(Rect.Y0, (int)std::floor(Z / CellMetres)));
				P.Cell = Cz * W + Cx;
			}
			else P.Cell = CurCell;
			P.X = (float)X; P.Y = (float)Y; P.Z = (float)Z; P.Sx = (float)Sx; P.Sy = (float)Sy; P.Sz = (float)Sz;
			P.Ry = (float)Ry; P.Rx = (float)Rx; P.Rz = (float)Rz;
			P.R = (float)C.R; P.G = (float)C.G; P.B = (float)C.B;
			Out.push_back(P);
		}
		void Put(EPiece Kind, double X, double Y, double Z, double Sx, double Sy, double Sz, double Ry = 0, double Grey_ = 1, double Rx = 0, double Rz = 0)
		{
			Put(Kind, X, Y, Z, Sx, Sy, Sz, Ry, Grey(Grey_), Rx, Rz);
		}
		bool IsHouse(int X, int Y) const
		{
			if (X < 0 || Y < 0 || X >= W || Y >= H || !Foot || (*Foot)[(size_t)(Y * W + X)] != 1) return false;
			const unsigned char C = At(X, Y);
			return C == 'B' || C == 'R';
		}
		bool InRect(const FCellRect& R, int X, int Y) const { return X >= R.X0 && X < R.X1 && Y >= R.Y0 && Y < R.Y1; }

		// a piece in a house frame (frame P)
		void P(const FHouseFrame& F, EPiece Kind, double Sv, double Y, double Av, double Sx, double Sy, double Sz, const FRgb& C, double Ry = 0, double Rx = 0, double Rz = 0)
		{
			double X, Z;
			F.At(Sv, Av, X, Z);
			Put(Kind, X, Y, Z, Sx, Sy, Sz, F.Yaw + Ry, C, Rx, Rz);
		}
		void P(const FHouseFrame& F, EPiece Kind, double Sv, double Y, double Av, double Sx, double Sy, double Sz, double Grey_, double Ry = 0, double Rx = 0, double Rz = 0)
		{
			P(F, Kind, Sv, Y, Av, Sx, Sy, Sz, Grey(Grey_), Ry, Rx, Rz);
		}
		FHouseFrame Frame(const FCellRect& R) const { return FrameOf(R, [this](double X, double Z) { return G(X, Z); }); }

		static EPiece WallKind(const FHouseSpec& Sp) { return Sp.bWood ? EPiece::Wood : Sp.bStone ? EPiece::Stone : Sp.bBrick ? EPiece::Brick : EPiece::Wall; }
		static EPiece JagKind(const FHouseSpec& Sp) { return Sp.bWood ? EPiece::BrokenWood : Sp.bStone ? EPiece::BrokenStone : Sp.bBrick ? EPiece::BrokenBrick : EPiece::Broken; }

		struct FBay { int X, Y, Dx, Dy; bool bLong; double Px, Pz; };
		// exterior wall bays of a rectangle: one per cell edge that faces open ground
		std::vector<FBay> Bays(const FCellRect& R, const FHouseFrame& F) const
		{
			std::vector<FBay> List;
			for (int Y = R.Y0; Y < R.Y1; Y++) for (int X = R.X0; X < R.X1; X++) for (const auto& D : Dir4)
			{
				const int Nx = X + D[0], Ny = Y + D[1];
				if (InRect(R, Nx, Ny) || IsHouse(Nx, Ny)) continue;
				List.push_back(FBay{ X, Y, D[0], D[1], F.bAlongX ? D[1] != 0 : D[0] != 0, (X + 0.5 + D[0] * 0.5) * CellMetres, (Y + 0.5 + D[1] * 0.5) * CellMetres });
			}
			return List;
		}
		void WindowAt(EPiece Kind, const FBay& B, double Y, const FRgb& C, double Sx = 1, double Sy = 1) { Put(Kind, B.Px, Y, B.Pz, Sx, Sy, 1, YawOf(B.Dx, B.Dy), C); }

		// church tower inside one end of the nave, rising through the roof, with belfry openings and a slate spire
		void Tower(const FHouseFrame& F, const FHouseSpec& Sp, double End, double& OutAT, double& OutT)
		{
			const double L = F.L, S = F.S, Base = F.Base, T = std::min(S - 0.8, 3.4), AT = End * (L / 2 - T / 2 - 0.1), Top = Base + Sp.H + 0.4 * S * Sp.K + 3.2;
			P(F, EPiece::Stone, 0, (Base - 1 + Top) / 2, AT, T, Top - Base + 1, T, 1.02);
			P(F, EPiece::Stone, 0, Top + 0.15, AT, T + 0.3, 0.3, T + 0.3, 1.12);
			P(F, EPiece::Spire, 0, Top + 0.3, AT, T - 0.2, 5.2, T - 0.2, 1.0);
			for (const double Sd : { -1.0, 1.0 })
			{
				P(F, EPiece::Paint, Sd * (T / 2 + 0.02), Top - 1.3, AT, 0.08, 1.4, 0.8, Palettes().Dark);
				P(F, EPiece::Paint, 0, Top - 1.3, AT + Sd * (T / 2 + 0.02), 0.8, 1.4, 0.08, Palettes().Dark);
			}
			OutAT = AT; OutT = T;
		}
		// stone quoins up a rendered house's outside corners; corners that meet a neighbour (terraces) get none
		void Quoins(const FCellRect& R, const FHouseFrame& F, double Hh)
		{
			const double S = F.S, L = F.L, Base = F.Base;
			const int Corners[4][2] = { { -1, -1 }, { -1, 1 }, { 1, -1 }, { 1, 1 } };
			for (const auto& Cn : Corners)
			{
				const double Ss = Cn[0], Sa = Cn[1];
				double Wx, Wz;
				F.At(Ss * S / 2, Sa * L / 2, Wx, Wz);
				const int Gx = (int)std::floor(Wx / CellMetres + 0.5), Gz = (int)std::floor(Wz / CellMetres + 0.5);
				bool bParty = false;
				const int Off[4][2] = { { -1, -1 }, { 0, -1 }, { -1, 0 }, { 0, 0 } };
				for (const auto& O : Off)
				{
					const int X = Gx + O[0], Y = Gz + O[1];
					if (!InRect(R, X, Y) && IsHouse(X, Y)) bParty = true;
				}
				if (bParty) continue;
				int K = 0;
				for (double Y = Base + 0.55; Y < Base + Hh - 0.3; K++, Y += 0.44)
				{
					const double Ws = (K & 1) ? 0.62 : 0.36, Wa = (K & 1) ? 0.36 : 0.62;
					P(F, EPiece::Stone, Ss * (S / 2 - Ws / 2 + 0.035), Y + 0.2, Sa * (L / 2 - Wa / 2 + 0.035), Ws, 0.4, Wa, 1.12 + 0.12 * Rnd(Gx * 3 + K, Gz, 160));
				}
			}
		}
		// a works chimney at one end of the roof
		void Chimney(const FHouseFrame& F, const FHouseSpec& Sp)
		{
			const double L = F.L, Base = F.Base, A = (Sp.T(26) < 0.5 ? -1 : 1) * (L / 2 - 1.3), Top = Base + Sp.H + 10 + 3 * Sp.T(27), Mid = (Base + Top) / 2;
			P(F, EPiece::Brick, 0, Mid - 2, A, 1.7, Top - Base - 4, 1.7, Mul(Sp.Tint, 0.82));
			P(F, EPiece::Brick, 0, Top - 2.5, A, 1.45, 5, 1.45, Mul(Sp.Tint, 0.78));
			for (const double Y : { Top - 1.1, Top - 3.2 }) P(F, EPiece::Stone, 0, Y, A, 1.62, 0.22, 1.62, 0.55);
			P(F, EPiece::Paint, 0, Top + 0.02, A, 1.2, 0.06, 1.2, Palettes().Dark);
		}
		// a pit's headframe over the shaft: legs leaning in to a deck, braces, back-stays and the sheave wheels
		void Headframe(const FHouseFrame& F, const FHouseSpec& Sp)
		{
			const double L = F.L, S = F.S, Base = F.Base, End = Sp.T(19) < 0.5 ? -1 : 1, Span = L * 0.4, As = End * (L / 2 - Span / 2), Hw = std::min(S, Span) / 2 - 0.35;
			const double Top = Base + 12.5, Lean = 0.55, LegH = Top - Base, Tilt = std::atan2(Hw * (1 - Lean), LegH);
			const FRgb Steel = Rgb(0.62, 0.36, 0.3);
			for (const double Sx : { -1.0, 1.0 }) for (const double Sz : { -1.0, 1.0 })
				P(F, EPiece::Steel, Sx * Hw * (1 + Lean) / 2, Base + LegH / 2, As + Sz * Hw * (1 + Lean) / 2, 0.28, LegH + 0.3, 0.28, Steel, 0, -Sz * Tilt, Sx * Tilt);
			for (int I = 1; I <= 4; I++)
			{
				const double Y = Base + LegH * I / 4.6, K = Hw * (1 - (1 - Lean) * I / 4.6);
				for (const double S2 : { -1.0, 1.0 })
				{
					P(F, EPiece::Steel, 0, Y, As + S2 * K, 2 * K, 0.16, 0.16, Steel);
					P(F, EPiece::Steel, S2 * K, Y, As, 0.16, 0.16, 2 * K, Steel);
				}
			}
			P(F, EPiece::Steel, 0, Top + 0.1, As, 2 * Hw * Lean + 0.8, 0.25, 2 * Hw * Lean + 0.8, Mul(Steel, 0.8));
			const double Reach = L - Span, Stay = std::hypot(Reach, LegH * 0.9), Pitch = std::atan2(Reach, LegH * 0.9);
			for (const double Sx : { -1.0, 1.0 }) P(F, EPiece::Steel, Sx * Hw * 0.6, Base + LegH * 0.55, As - End * Reach / 2, 0.24, Stay, 0.24, Steel, 0, End * Pitch, 0);
			for (const double Sx : { -0.45, 0.45 }) for (int I = 0; I < 12; I++)
			{
				const double Ang = I / 12.0 * Pi * 2, Rr = 1.25;
				P(F, EPiece::Steel, Sx, Top + 1.5 + std::sin(Ang) * Rr, As + std::cos(Ang) * Rr, 0.12, 0.7, 0.14, Mul(Steel, 0.7), 0, -Ang, 0);
			}
			P(F, EPiece::Steel, 0, Top + 1.5, As, 1.4, 0.2, 0.2, Mul(Steel, 0.6));
		}
		// the winding house of a mine head
		void MineHead(const FCellRect& R, const FHouseSpec& Sp)
		{
			const FHouseFrame F = Frame(R);
			const double L = F.L, S = F.S, Base = F.Base, Hh = Sp.H, K = Sp.K, End = Sp.T(19) < 0.5 ? -1 : 1, Lh = L * 0.6, Ah = -End * (L - Lh) / 2, Oh = 0.35;
			P(F, EPiece::Brick, 0, Base - 1 + (Hh + 1) / 2, Ah, S, Hh + 1, Lh, Sp.Tint);
			P(F, EPiece::Stone, 0, Base - 1.55 + 2.1 / 2, Ah, S + 0.14, 2.1, Lh + 0.14, 0.85);
			P(F, EPiece::GableBrick, 0, Base + Hh, Ah, S, S * K, Lh, Sp.Tint);
			P(F, EPiece::Roof, 0, Base + Hh - 0.8 * K * Oh, Ah, S + 2 * Oh, (S + 2 * Oh) * K, Lh + 2 * Oh, Sp.Roof);
			for (const double S2 : { -1.0, 1.0 }) for (int I = 0; I < 3; I++) P(F, EPiece::Paint, S2 * (S / 2 + 0.03), Base + 2.6, Ah + (I - 1) * Lh / 3.4, 0.06, 2.4, 1.0, Lin(0x5a6470));
			P(F, EPiece::Stone, 0, Base - 0.2, End * (L - Lh) / 2, S - 0.4, 0.5, L - Lh - 0.4, 0.75);
			Headframe(F, Sp);
		}
		void Standing_(const FHouseRect& Rr, const FHouseSpec& Sp)
		{
			const FCellRect& R = Rr.R;
			if (Sp.bMine) { MineHead(R, Sp); return; }
			const FPalettes& Pal = Palettes();
			const FHouseFrame F = Frame(R);
			const double L = F.L, S = F.S, Base = F.Base, Hh = Sp.H, K = Sp.K, Oh = 0.35, Ph = Sp.bShed ? 0.2 : 0.55;
			P(F, WallKind(Sp), 0, Base - 1 + (Hh + 1) / 2, 0, S, Hh + 1, L, Sp.Tint);
			if (!Sp.bStone && !Sp.bWood) Quoins(R, F, Hh);
			P(F, EPiece::Stone, 0, Base - 1.55 + (Ph + 1.55) / 2, 0, S + 0.14, Ph + 1.55, L + 0.14, 0.85);
			if (Sp.bStone && !Sp.bChurch)
				for (int I = 1; I < Sp.Storeys; I++) P(F, EPiece::Paint, 0, Sp.bBlock ? Base + 0.05 + I * 3.1 : Base + Hh / 2 + 0.15, 0, S + 0.12, 0.2, L + 0.12, Pal.Dressed);
			if (Sp.bBlock)
			{
				P(F, EPiece::Stone, 0, Base + Hh - 0.25, 0, S + 0.24, 0.22, L + 0.24, 1.05);
				P(F, EPiece::Paint, 0, Base + Hh + 0.03, 0, S - 0.5, 0.06, L - 0.5, Lin(0x4a4640));
				for (const double Sd : { -1.0, 1.0 })
				{
					P(F, EPiece::Stone, Sd * (S / 2 - 0.15), Base + Hh + 0.25, 0, 0.3, 0.6, L, 0.95);
					P(F, EPiece::Stone, 0, Base + Hh + 0.25, Sd * (L / 2 - 0.15), S - 0.6, 0.6, 0.3, 0.95);
				}
				for (int I = 0; I < 2; I++)
				{
					const double Sv = (Sp.T(20 + I) - 0.5) * (S - 2), Av = (Sp.T(22 + I) - 0.5) * (L - 2);
					P(F, EPiece::Stone, Sv, Base + Hh + 0.6, Av, 0.7, 1.2, 1.1, 0.8);
					P(F, EPiece::Stone, Sv, Base + Hh + 1.26, Av, 0.85, 0.12, 1.25, 0.55);
				}
				if (Sp.bFactory) Chimney(F, Sp);
			}
			else
			{
				P(F, Sp.bWood ? EPiece::GableWood : Sp.bStone ? EPiece::GableStone : Sp.bBrick ? EPiece::GableBrick : EPiece::Gable, 0, Base + Hh, 0, S, S * K, L, Sp.Tint);
				const double Wr = S + 2 * Oh;
				P(F, EPiece::Roof, 0, Base + Hh - 0.8 * K * Oh, 0, Wr, Wr * K, L + 2 * Oh, Sp.Roof);
				if (!Sp.bChurch && !Sp.bWood && Sp.T(13) < 0.8)
				{
					const double Av = (Sp.T(14) < 0.5 ? -1 : 1) * std::max(0.0, L / 2 - 0.9), Sv = (Sp.T(15) < 0.5 ? -1 : 1) * 0.22 * S;
					const double Top = Base + Hh + 0.4 * K * (S - 2 * std::fabs(Sv)) + 1.0 + 0.3 * Sp.T(16), Bot = Base + Hh - 0.3;
					if (Sp.bBrick) P(F, EPiece::Brick, Sv, (Top + Bot) / 2, Av, 0.6, Top - Bot, 0.75, Mul(Sp.Tint, 0.9));
					else P(F, EPiece::Stone, Sv, (Top + Bot) / 2, Av, 0.6, Top - Bot, 0.75, 0.8);
					P(F, EPiece::Stone, Sv, Top + 0.06, Av, 0.78, 0.12, 0.92, 0.5);
					if (Sp.T(16) < 0.5) P(F, EPiece::Paint, Sv, Top + 0.27, Av, 0.16, 0.3, 0.16, Lin(0x7a4a36));
					else for (const double D : { -0.18, 0.18 }) P(F, EPiece::Paint, Sv, Top + 0.27, Av + D, 0.16, 0.3, 0.16, Lin(0x7a4a36));
				}
			}
			const std::vector<FBay> List = Bays(R, F);
			if (Sp.bChurch)
			{
				const double End = Sp.T(19) < 0.5 ? -1 : 1;
				double AT, T;
				Tower(F, Sp, End, AT, T);
				for (const FBay& B : List)
				{
					const double A = F.bAlongX ? B.Px - F.Cx : B.Pz - F.Cz;
					if (B.bLong && std::fabs(A - AT) > T / 2 + 0.4) WindowAt(EPiece::Pane, B, Base + 3.4, Lin(0x8a9098), 0.8, 2.0);
				}
				double Gx, Gz;
				F.At(0, End * (L / 2 + 0.5), Gx, Gz);
				const double Gg = std::max(Base, G(Gx, Gz));
				P(F, EPiece::Paint, 0, Gg + 1.45, End * (L / 2 + 0.02), 1.5, 2.8, 0.1, Lin(0x4a3220));
				P(F, EPiece::Stone, 0, Gg + 0.06, End * (L / 2 + 0.4), 2.2, 0.3, 0.8, 0.9);
				P(F, EPiece::Paint, 0, Base + Hh + 1.0, -End * (L / 2 + 0.03), 1.0, 1.0, 0.08, Lin(0x3a4250), 0, 0, Pi / 4);
				return;
			}
			if (Sp.bBarn)
			{
				const double End = Sp.T(19) < 0.5 ? -1 : 1, AE = End * (L / 2 + 0.02), Dw = std::min(2.8, S - 1.2), Dh = std::min(3.2, Hh - 0.6);
				double Gx, Gz;
				F.At(0, End * (L / 2 + 0.5), Gx, Gz);
				const double Gg = std::max(Base, G(Gx, Gz));
				P(F, EPiece::Paint, 0, Gg + Dh / 2 + 0.05, AE, Dw, Dh, 0.1, Lin(0x4a3626));
				for (const double Sd : { -1.0, 1.0 }) P(F, EPiece::Paint, 0, Gg + Dh / 2 + 0.05, AE + End * 0.04, std::hypot(Dw, Dh) - 0.25, 0.14, 0.08, Lin(0xd8cfb0), 0, 0, Sd * std::atan2(Dh, Dw));
				P(F, EPiece::Paint, 0, Gg + Dh + 0.14, AE + End * 0.04, Dw + 0.2, 0.16, 0.08, Lin(0xd8cfb0));
				P(F, EPiece::Paint, 0, Base + Hh + 0.75, AE, 1.1, 1.0, 0.1, Lin(0x3a2a1c));
				return;
			}
			if (Sp.bShed) { Shed(F, Sp, List); return; }
			// windows on every open bay and storey, a door on a long side
			std::vector<const FBay*> Longs;
			for (const FBay& B : List) if (B.bLong) Longs.push_back(&B);
			const FBay* Door = !Sp.bBlock && !Longs.empty() ? Longs[(size_t)std::floor(Sp.T(17) * Longs.size())] : nullptr;
			for (const FBay& B : List) for (int I = 0; I < Sp.Storeys; I++)
			{
				if (&B == Door && I == 0) continue;
				if (Rnd(B.X * 3 + B.Dx, B.Y * 3 + B.Dy, 30 + I) < 0.15) continue;
				const double Y = Sp.bBlock ? Base + 1.6 + I * 3.1 : I == 0 ? Base + 1.6 : Base + Hh - 1.45;
				WindowAt(Sp.bShutters ? EPiece::Window : EPiece::Pane, B, Y, Sp.Trim);
				if (Sp.bStone || Sp.bBrick) Put(EPiece::Paint, B.Px + B.Dx * 0.02, Y + 0.63, B.Pz + B.Dy * 0.02, 1.02, 0.2, 0.12, YawOf(B.Dx, B.Dy), Pal.Dressed);
			}
			if (Door)
			{
				const FBay& D = *Door;
				const double Yd = YawOf(D.Dx, D.Dy), Gg = std::max(Base, G(D.Px + D.Dx * 0.4, D.Pz + D.Dy * 0.4));
				Put(EPiece::Paint, D.Px, Gg + 1.05, D.Pz, 1.0, 2.0, 0.08, Yd, Mul(Sp.Trim, 0.75));
				Put(EPiece::Paint, D.Px + D.Dx * 0.045, Gg + 1.05, D.Pz + D.Dy * 0.045, 0.05, 1.96, 0.02, Yd, Mul(Sp.Trim, 0.45));
				for (const double Sd : { -1.0, 1.0 }) Put(EPiece::Stone, D.Px - D.Dy * Sd * 0.58, Gg + 1.05, D.Pz + D.Dx * Sd * 0.58, 0.16, 2.1, 0.16, Yd, 1.05);
				Put(EPiece::Stone, D.Px, Gg + 2.16, D.Pz, 1.32, 0.18, 0.18, Yd, 1.05);
				Put(EPiece::Stone, D.Px + D.Dx * 0.3, Gg + 0.06, D.Pz + D.Dy * 0.3, 1.4, 0.3, 0.6, Yd, 0.9);
			}
		}
		// a timber shed's trim: tarred corner posts and wall plates, a ledged and braced plank door, the odd window
		void Shed(const FHouseFrame& F, const FHouseSpec& Sp, const std::vector<FBay>& List)
		{
			const double L = F.L, S = F.S, Base = F.Base, Hh = Sp.H;
			const FRgb Pale = Rgb(1.25, 1.15, 1);
			for (const double Sd : { -1.0, 1.0 })
			{
				for (const double A : { -1.0, 1.0 }) P(F, EPiece::Dark, Sd * (S / 2 - 0.04), Base + Hh / 2, A * (L / 2 - 0.04), 0.2, Hh, 0.2, 0.8);
				P(F, EPiece::Dark, Sd * (S / 2 + 0.01), Base + Hh - 0.11, 0, 0.14, 0.22, L + 0.1, 0.8);
			}
			const FBay* Door = List.empty() ? nullptr : &List[(size_t)std::floor(Sp.T(17) * List.size())];
			for (const FBay& B : List) if (&B != Door && B.bLong && Rnd(B.X * 3 + B.Dx, B.Y * 3 + B.Dy, 30) < 0.4) WindowAt(EPiece::Pane, B, Base + Hh - 1.0, Lin(0x5a4a3a), 0.8, 0.6);
			if (!Door) return;
			const FBay& D = *Door;
			const double Yd = YawOf(D.Dx, D.Dy), Gg = std::max(Base, G(D.Px + D.Dx * 0.4, D.Pz + D.Dy * 0.4)), Dh = std::min(1.9, Hh - 0.45);
			const double X = D.Px + D.Dx * 0.03, Z = D.Pz + D.Dy * 0.03, Bx = D.Px + D.Dx * 0.07, Bz = D.Pz + D.Dy * 0.07;
			Put(EPiece::Dark, X, Gg + 0.05 + Dh / 2, Z, 0.95, Dh, 0.08, Yd, 0.7);
			for (const double Y : { 0.3, Dh - 0.25 }) Put(EPiece::Wood, Bx, Gg + 0.05 + Y, Bz, 0.95, 0.12, 0.06, Yd, Pale);
			Put(EPiece::Wood, Bx, Gg + 0.05 + Dh / 2, Bz, std::hypot(0.85, Dh - 0.55), 0.11, 0.05, Yd, Pale, 0, std::atan2(Dh - 0.55, 0.85));
		}
		// a house with fallen cells: standing cells keep jagged stubs and dropped slabs
		void Ruin(const FHouseRect& Rr, const FHouseSpec& Sp)
		{
			const FCellRect& R = Rr.R;
			std::vector<int> Keep;
			for (int Y = R.Y0; Y < R.Y1; Y++) for (int X = R.X0; X < R.X1; X++) if (At(X, Y) == 'B') Keep.push_back(Y * W + X);
			const EPiece Jag = JagKind(Sp);
			const FRgb Tint = Sp.Tint;
			for (int Y = R.Y0; Y < R.Y1; Y++) for (int X = R.X0; X < R.X1; X++)
			{
				if (At(X, Y) != 'B') continue;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz);
				const int Piles = 2 + (Rnd(X, Y, 40) < 0.45 ? 1 : 0);
				for (int K = 0; K < Piles; K++)
				{
					const double Ang = Rnd(X, Y, 41 + K) * 6.2832, Rad = CellMetres * (0.08 + 0.48 * Rnd(X, Y, 46 + K)), Len = 0.55 + 1.15 * Rnd(X, Y, 52 + K);
					const double Ht = 0.35 + std::min(2.4, Sp.H) * (0.12 + 0.38 * Rnd(X, Y, 58 + K)), Yaw = Ang + (Rnd(X, Y, 64 + K) - 0.5) * 0.8;
					Put(Jag, Cx + std::cos(Ang) * Rad, Gg - 0.12, Cz + std::sin(Ang) * Rad, Len, Ht, 0.26 + 0.12 * Rnd(X, Y, 70 + K), Yaw, Tint, (Rnd(X, Y, 74 + K) - 0.5) * 0.35, (Rnd(X, Y, 78 + K) - 0.5) * 0.25);
				}
				if (Rnd(X, Y, 82) < 0.7)
				{
					const double Ang = Rnd(X, Y, 83) * 6.2832, Rad = CellMetres * (0.1 + 0.4 * Rnd(X, Y, 84)), S = 0.35 + 0.55 * Rnd(X, Y, 85);
					Put(EPiece::Chunk, Cx + std::cos(Ang) * Rad, Gg + S * 0.15, Cz + std::sin(Ang) * Rad, S * 1.3, S * 0.55, S, Rnd(X, Y, 86) * 6.28, Tint, (Rnd(X, Y, 87) - 0.5) * 0.5, (Rnd(X, Y, 88) - 0.5) * 0.4);
				}
				for (int I = 0; I < 4; I++)
				{
					const int Dx = Dir4[I][0], Dy = Dir4[I][1], Nx = X + Dx, Ny = Y + Dy;
					if (std::find(Keep.begin(), Keep.end(), Ny * W + Nx) != Keep.end()) continue; // keep.has(ny * w + nx), as JavaScript indexes it
					if (Rnd(X, Y, 90 + I) < 0.42) continue;
					const double Slide = (Rnd(X, Y, 96 + I) - 0.5) * CellMetres * 0.55, Outw = CellMetres * (0.28 + 0.28 * Rnd(X, Y, 102 + I));
					const double Len = 0.45 + 0.95 * Rnd(X, Y, 108 + I), Ht = 0.4 + 1.5 * Rnd(X, Y, 114 + I), Yaw = YawOf(Dx, Dy) + (Rnd(X, Y, 120 + I) - 0.5) * 0.9;
					Put(Jag, Cx + Dx * Outw - Dy * Slide, Gg - 0.1, Cz + Dy * Outw + Dx * Slide, Len, Ht, 0.28, Yaw, Tint, (Rnd(X, Y, 126 + I) - 0.5) * 0.2, 0);
				}
			}
			if (Sp.bFactory || Sp.bMine)
			{
				// the chimney or the headframe stays up while the cell under its middle does
				const FHouseFrame F = Frame(R);
				const double A = Sp.bFactory ? (Sp.T(26) < 0.5 ? -1 : 1) * (F.L / 2 - 1.3) : (Sp.T(19) < 0.5 ? -1 : 1) * F.L * 0.3;
				double Fx, Fz;
				F.At(0, A, Fx, Fz);
				if (At((int)std::floor(Fx / CellMetres), (int)std::floor(Fz / CellMetres)) == 'B') { if (Sp.bFactory) Chimney(F, Sp); else Headframe(F, Sp); }
			}
			if (Sp.bChurch)
			{
				// the tower stays up while its own cells do
				const FHouseFrame F = Frame(R);
				const double End = Sp.T(19) < 0.5 ? -1 : 1, T = std::min(F.S - 0.8, 3.4), AT = End * (F.L / 2 - T / 2 - 0.1);
				double Tx, Tz;
				F.At(0, AT, Tx, Tz);
				const int X0 = (int)std::floor((Tx - T / 2) / CellMetres), X1 = (int)std::floor((Tx + T / 2 - 0.01) / CellMetres);
				const int Y0 = (int)std::floor((Tz - T / 2) / CellMetres), Y1 = (int)std::floor((Tz + T / 2 - 0.01) / CellMetres);
				bool bUp = true;
				for (int Y = Y0; Y <= Y1; Y++) for (int X = X0; X <= X1; X++) if (At(X, Y) != 'B') bUp = false;
				double A2, T2;
				if (bUp) Tower(F, Sp, End, A2, T2);
			}
		}

		// ---------- rubble ----------
		const FHouseTint* TintAt(int C) const
		{
			if (!TintIndex || !Tints || C < 0 || C >= (int)TintIndex->size()) return nullptr;
			const int I = (*TintIndex)[(size_t)C];
			return I >= 0 ? &(*Tints)[(size_t)I] : nullptr;
		}
		void Rubble()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'R') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz);
				const FHouseTint* House = TintAt(Y * W + X);
				if (Src.Spoil(X, Y))
				{
					// a mine's spoil tip: dark waste in overlapping cones, tallest in the middle of the heap
					int N = 0;
					for (int Dy = -1; Dy <= 1; Dy++) for (int Dx = -1; Dx <= 1; Dx++)
					{
						if (!(Dx || Dy)) continue;
						// spoil.has((y + dy) * w + x + dx): a flat index, as JavaScript reads it
						const int K = (Y + Dy) * W + X + Dx;
						if (K >= 0 && K < W * H && Src.Spoil(K % W, K / W) && At(X + Dx, Y + Dy) == 'R') N++;
					}
					const double S = 3.4 + 1.2 * Rnd(X, Y, 140), Gy = 0.36 + 0.08 * Rnd(X, Y, 142);
					Put(EPiece::Earth, Cx + (Rnd(X, Y, 143) - 0.5) * 0.8, Gg - 0.2, Cz + (Rnd(X, Y, 144) - 0.5) * 0.8, S, 0.7 + 0.32 * N + 0.4 * Rnd(X, Y, 141), S, Rnd(X, Y, 145) * 6.28, Rgb(Gy, Gy * 0.97, Gy * 1.02));
					if (!Low()) Put(EPiece::Chunk, Cx + (Rnd(X, Y, 146) - 0.5) * 1.6, Gg + 0.1, Cz + (Rnd(X, Y, 147) - 0.5) * 1.6, 0.6, 0.35, 0.5, Rnd(X, Y, 148) * 6.28, Rgb(0.5, 0.48, 0.47));
					continue;
				}
				const int N = (Low() ? 2 : 3) + (House && !Low() ? 1 : 0);
				for (int K = 0; K < N; K++)
				{
					if (Rnd(X, Y, K + 3) < 0.22) continue;
					const double Ang = Rnd(X, Y, K + 5) * 6.2832, Rad = CellMetres * 0.62 * Rnd(X, Y, K + 7), S = 0.4 + 0.7 * Rnd(X, Y, K), C = Rnd(X, Y, K + 17);
					const FRgb Tint = House && C < 0.5 ? (House->bWood ? Mul(House->Color, 0.6) : House->Color) : C < 0.78 ? Rgb(0.95, 0.93, 0.9) : Rgb(1.12, 0.66, 0.52);
					Put(EPiece::Chunk, Cx + std::cos(Ang) * Rad, Gg + S * 0.12, Cz + std::sin(Ang) * Rad, S * (1 + 0.4 * Rnd(X, Y, K + 21)), S * (0.45 + 0.3 * Rnd(X, Y, K + 25)), S * (0.9 + 0.4 * Rnd(X, Y, K + 29)),
						Rnd(X, Y, K + 33) * 6.28, Tint, (Rnd(X, Y, K + 37) - 0.5) * 0.6, (Rnd(X, Y, K + 41) - 0.5) * 0.6);
				}
				if (!Low() && Rnd(X, Y, 48) < 0.75)
				{
					const double Ang = Rnd(X, Y, 49) * 6.2832, Rad = CellMetres * (0.48 + 0.32 * Rnd(X, Y, 50)), Len = 0.7 + 1.1 * Rnd(X, Y, 51), Ht = 0.25 + 0.55 * Rnd(X, Y, 52);
					const EPiece Jag = House ? (House->bWood ? EPiece::BrokenWood : House->bStone ? EPiece::BrokenStone : House->bBrick ? EPiece::BrokenBrick : EPiece::Broken) : EPiece::Broken;
					const FRgb Tint = House ? (House->bWood ? Mul(House->Color, 0.7) : House->Color) : Rgb(0.95, 0.93, 0.9);
					Put(Jag, Cx + std::cos(Ang) * Rad, Gg - 0.05, Cz + std::sin(Ang) * Rad, Len, Ht, 0.22, Ang + (Rnd(X, Y, 53) - 0.5) * 0.8, Tint, (Rnd(X, Y, 54) - 0.5) * 0.5, (Rnd(X, Y, 55) - 0.5) * 0.3);
				}
				for (int I = 0; I < 4; I++)
				{
					const int Dx = Dir4[I][0], Dy = Dir4[I][1];
					const unsigned char Nb = At(X + Dx, Y + Dy);
					if (Nb == 'R' || Nb == 'B' || Nb == 'K' || Rnd(X, Y, 50 + I) > (House ? 0.34 : 0.18)) continue;
					const double Slide = (Rnd(X, Y, 62 + I) - 0.5) * CellMetres * 0.7, Outw = CellMetres * (0.35 + 0.28 * Rnd(X, Y, 66 + I));
					const double Len = 0.55 + 0.9 * Rnd(X, Y, 54 + I), Ht = House ? 0.45 + 0.9 * Rnd(X, Y, 58 + I) : 0.3 + 0.45 * Rnd(X, Y, 58 + I);
					const bool bStone = !House || House->bStone;
					Put(bStone ? EPiece::BrokenStone : House->bWood ? EPiece::BrokenWood : House->bBrick ? EPiece::BrokenBrick : EPiece::Broken,
						Cx + Dx * Outw - Dy * Slide, Gg - 0.08, Cz + Dy * Outw + Dx * Slide, Len, Ht, 0.28, YawOf(Dx, Dy) + (Rnd(X, Y, 70 + I) - 0.5) * 0.8,
						bStone ? Grey(0.95) : House->Wall, (Rnd(X, Y, 74 + I) - 0.5) * 0.25, 0);
				}
			}
		}

		// ---------- hedgerows: bocage earth banks topped with shrubs ----------
		void Hedges()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'H') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz), R1 = Rnd(X, Y, 80), Hb = 0.85 + 0.25 * R1;
				const int Hurt = std::min(2, StageAt(X, Y));
				const double Bank = Hb * (Hurt == 0 ? 1.0 : Hurt == 1 ? 0.85 : 0.6);
				Put(EPiece::Earth, Cx, Gg - 0.1, Cz, 2.4, Bank, 2.4, R1 * 6.28, Rgb(0.78, 0.9, 0.6));
				const bool Ex = At(X - 1, Y) == 'H' || At(X + 1, Y) == 'H', Ez = At(X, Y - 1) == 'H' || At(X, Y + 1) == 'H';
				const bool bAlongX = Ex != Ez ? Ex : Rnd(X, Y, 81) < 0.5;
				const double Top = Gg - 0.1 + Bank;
				const int N = Hurt == 2 ? 0 : Hurt == 1 || Low() ? 1 : 2;
				for (int I = 0; I < N; I++)
				{
					const double O = N == 1 ? 0 : (I ? 0.5 : -0.5) + (Rnd(X, Y, 82 + I) - 0.5) * 0.3, Side = (Rnd(X, Y, 84 + I) - 0.5) * 0.2;
					const double Rr = (Low() ? 1.0 : 0.8) + 0.2 * Rnd(X, Y, 86 + I), Wide = Low() ? 1.15 : 1, V = Rnd(X, Y, 88 + I);
					const double Sx = bAlongX ? Cx + O : Cx + Side, Sy = Top + Rr * 0.3, Sz = bAlongX ? Cz + Side : Cz + O;
					const FRgb Tint = Rgb(0.88 + 0.2 * V, 0.95 + 0.15 * Rnd(X, Y, 90 + I), 0.85 + 0.15 * V);
					const double Yaw = (bAlongX ? 0 : Pi / 2) + (V < 0.5 ? Pi : 0) + (V - 0.5) * 0.3;
					Put(EPiece::Shrub, Sx, Sy - Rr * 0.1, Sz, Rr * Wide * 0.82, Rr * 0.75, Rr * Wide * 0.82, V * 6.28, Mul(Tint, 0.75));
					Put(EPiece::Leaves, Sx, Sy, Sz, Rr * (0.9 + 0.3 * V), Rr * (0.85 + 0.35 * Rnd(X, Y, 92 + I)), Rr * Wide, Yaw, Mul(Tint, 0.95));
				}
			}
		}

		// ---------- walls: '#' from the map is a capped stone wall, '#' built in play is a sandbag wall ----------
		void Walls()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != '#') continue;
				CurCell = Y * W + X;
				int Conn[4][2], NConn = 0, DiagOn[4][2], NDiag = 0;
				for (const auto& D : Dir4) if (At(X + D[0], Y + D[1]) == '#') { Conn[NConn][0] = D[0]; Conn[NConn][1] = D[1]; NConn++; }
				for (const auto& D : Diag) if (At(X + D[0], Y + D[1]) == '#' && At(X + D[0], Y) != '#' && At(X, Y + D[1]) != '#') { DiagOn[NDiag][0] = D[0]; DiagOn[NDiag][1] = D[1]; NDiag++; }
				if (Src.Orig(X, Y) == '#') StoneWall(X, Y, Conn, NConn, DiagOn, NDiag);
				else BagWall(X, Y, Conn, NConn, DiagOn, NDiag);
			}
		}
		void StoneWall(int X, int Y, const int (*Conn)[2], int NConn, const int (*DiagOn)[2], int NDiag)
		{
			constexpr double WT = 0.55, WH = 1.0, Sink = 0.3;
			const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz), Tint = 0.85 + 0.25 * Rnd(X, Y, 100), Y0 = Gg - Sink, Cap = Tint * 1.18;
			const int Hurt = std::min(2, StageAt(X, Y));
			const double Kk = Hurt == 0 ? 1.0 : Hurt == 1 ? 0.74 : 0.48;
			auto PutW = [&](double Px, double Py, double Pz, double Sx, double Sy, double Sz, double Yaw, double C)
			{
				if (!Hurt) { Put(EPiece::Stone, Px, Py, Pz, Sx, Sy, Sz, Yaw, C); return; }
				if (Sy != 0.13) Put(EPiece::Stone, Px, Y0 + (Py - Y0) * Kk, Pz, Sx, Sy * Kk, Sz, Yaw, C);
				else if (Hurt < 2) Put(EPiece::Stone, Px, Y0 + (Py - Y0) * Kk, Pz, Sx, Sy, Sz, Yaw, C);
			};
			auto Run = [&](double Ox, double Oz, double Len, double Yaw, double Drop)
			{
				PutW(Cx + Ox, Y0 + WH / 2 - Drop, Cz + Oz, WT, WH, Len, Yaw, Tint);
				PutW(Cx + Ox, Y0 + WH + 0.065 - Drop, Cz + Oz, WT + 0.16, 0.13, Len + 0.02, Yaw, Cap);
			};
			if (NConn == 2 && Conn[0][0] == -Conn[1][0] && Conn[0][1] == -Conn[1][1] && !NDiag) { Run(0, 0, CellMetres, YawOf(Conn[0][0], Conn[0][1]), 0); return; }
			if (!NConn && !NDiag) { Run(0, 0, 1.6, Rnd(X, Y, 101) < 0.5 ? Pi / 2 : 0.0, 0); return; }
			for (int I = 0; I < NConn; I++) Run(Conn[I][0] * (0.5 + WT / 4), Conn[I][1] * (0.5 + WT / 4), 1 - WT / 2, YawOf(Conn[I][0], Conn[I][1]), 0);
			for (int I = 0; I < NDiag; I++) Run(DiagOn[I][0] * 0.5, DiagOn[I][1] * 0.5, std::sqrt(2.0), YawOf(DiagOn[I][0], DiagOn[I][1]), 0.015);
			if (NConn == 1 && !NDiag)
			{
				PutW(Cx, Y0 + (WH + 0.15) / 2, Cz, 0.75, WH + 0.15, 0.75, 0, Tint);
				PutW(Cx, Y0 + WH + 0.215, Cz, 0.9, 0.13, 0.9, 0, Cap);
			}
			else
			{
				PutW(Cx, Y0 + WH / 2, Cz, WT, WH, WT, 0, Tint);
				PutW(Cx, Y0 + WH + 0.07, Cz, WT + 0.16, 0.13, WT + 0.16, 0, Cap);
			}
		}
		void Bags(int X, int Y, int K, double Px, double Py, double Pz, double Yaw, double Len = 0.56)
		{
			const double J = Rnd(X * 7 + K, Y * 5 + K, 110);
			Put(EPiece::Bag, Px + (J - 0.5) * 0.05, Py, Pz + (Rnd(X, Y, 111 + K) - 0.5) * 0.05, Len * (0.95 + 0.1 * J), 0.28, 0.44, Yaw + (J - 0.5) * 0.2, PickOf(Palettes().BagTints, Rnd(X, Y, 112 + K)));
		}
		void BagWall(int X, int Y, const int (*Conn)[2], int NConn, const int (*DiagOn)[2], int NDiag)
		{
			const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz) - 0.04;
			int Dirs[8][2], NDirs = 0;
			for (int I = 0; I < NConn; I++) { Dirs[NDirs][0] = Conn[I][0]; Dirs[NDirs][1] = Conn[I][1]; NDirs++; }
			for (int I = 0; I < NDiag; I++) { Dirs[NDirs][0] = DiagOn[I][0]; Dirs[NDirs][1] = DiagOn[I][1]; NDirs++; }
			if (!NDirs)
			{
				const bool bX = Rnd(X, Y, 113) < 0.5;
				Dirs[0][0] = bX ? 1 : 0; Dirs[0][1] = bX ? 0 : 1; Dirs[1][0] = bX ? -1 : 0; Dirs[1][1] = bX ? 0 : -1; NDirs = 2;
			}
			const int Stage = StageAt(X, Y);
			int K = 0;
			for (int I = 0; I < NDirs; I++)
			{
				const int Dx = Dirs[I][0], Dy = Dirs[I][1];
				const double L = std::hypot((double)Dx, (double)Dy), Ux = Dx / L, Uz = Dy / L, Yaw = YawOf(Ux, Uz) + Pi / 2, Len = 0.56 * L;
				const bool bOwns = Dx > 0 || (Dx == 0 && Dy > 0);
				for (int C = 0; C < 3 - Stage; C++)
				{
					const double AtOwn[2] = { 0.5, 1 }, AtOne[1] = { 0.5 }, AtTwo[2] = { 0.25, 0.75 };
					const double* Ats = C == 1 ? (bOwns ? AtOwn : AtOne) : AtTwo;
					const int NAt = C == 1 ? (bOwns ? 2 : 1) : 2;
					const double Rows2[2] = { -0.2, 0.2 }, Rows1[1] = { 0 };
					const double* Rows = C == 0 ? Rows2 : Rows1;
					const int NRows = C == 0 ? 2 : 1;
					for (int A = 0; A < NAt; A++) for (int Ro = 0; Ro < NRows; Ro++)
					{
						const double Fv = Ats[A], Off = Rows[Ro];
						Bags(X, Y, K++, Cx + Ux * Fv * L - Uz * Off, Gg + 0.14 + C * 0.25, Cz + Uz * Fv * L + Ux * Off, Yaw, Len);
					}
				}
			}
			if (Stage < 2) Bags(X, Y, K++, Cx, Gg + 0.39, Cz, YawOf(Dirs[0][0], Dirs[0][1]) + Pi / 2);
		}

		// ---------- trenches: timber revetments, posts, duckboards, spoil parapets; MG nests get a ring of bags ----------
		void Trenches()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'T') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = Src.TrenchFloor(Cx, Cz), Top = Gg + TrenchDepth, Wall = (Gg + Top) / 2;
				bool Conn[4], bAny = false;
				for (int I = 0; I < 4; I++) { Conn[I] = At(X + Dir4[I][0], Y + Dir4[I][1]) == 'T'; bAny = bAny || Conn[I]; }
				for (int I = 0; I < 4; I++)
				{
					const int Dx = Dir4[I][0], Dy = Dir4[I][1];
					const double Yaw = YawOf(Dx, Dy);
					if (Conn[I])
					{
						for (const double S : { -1.0, 1.0 }) Put(EPiece::Wood, Cx + Dx * 0.75 - Dy * S * 0.5, Wall, Cz + Dy * 0.75 + Dx * S * 0.5, 0.08, TrenchDepth + 0.2, 0.5, Yaw, 0.9);
						if (!Low()) Put(EPiece::Duck, Cx + Dx * 0.5, Gg + 0.19, Cz + Dy * 0.5, 1, 1, 1, Yaw + Pi / 2);
					}
					else
					{
						Put(EPiece::Wood, Cx + Dx * 0.5, Wall, Cz + Dy * 0.5, 1.08, TrenchDepth + 0.2, 0.08, Yaw, 0.9);
						if (At(X + Dx, Y + Dy) != '#') Put(EPiece::Earth, Cx + Dx * 0.88, Top - 0.1, Cz + Dy * 0.88, 2.1, 0.5, 0.7, Yaw, Rgb(0.95, 0.88, 0.78));
					}
				}
				for (const auto& D : Diag) Put(EPiece::Dark, Cx + D[0] * 0.5, Wall + 0.1, Cz + D[1] * 0.5, 0.12, TrenchDepth + 0.4, 0.12, 0, 0.8);
				if (bAny) continue;
				if (!Low()) Put(EPiece::Duck, Cx, Gg + 0.19, Cz, 0.8, 1, 1, Rnd(X, Y, 120) < 0.5 ? 0 : Pi / 2);
				// a lone pit with sandbags around it is an MG nest
				int Fx = 0, Fz = 0, N = 0;
				for (const auto& D : Dir4) if (At(X + D[0], Y + D[1]) == '#') { Fx += D[0]; Fz += D[1]; N++; }
				for (const auto& D : Diag) if (At(X + D[0], Y + D[1]) == '#') { Fx += D[0]; Fz += D[1]; N++; }
				if (N < 3) continue;
				const double A0 = Fx || Fz ? std::atan2((double)Fz, (double)Fx) : 0.0;
				for (int C = 0; C < 2; C++) for (int I = -3; I <= 3 - C; I++)
				{
					const double A = A0 + I * 0.5 + C * 0.25;
					Bags(X, Y, 200 + C * 10 + I, Cx + std::cos(A) * 0.82, Top + 0.1 + C * 0.25, Cz + std::sin(A) * 0.82, -A + Pi / 2, 0.5);
				}
				for (const double S : { -1.0, 1.0 })
				{
					const double Bx = Cx - std::cos(A0) * 0.45 - std::sin(A0) * S * 0.3, Bz = Cz - std::sin(A0) * 0.45 + std::cos(A0) * S * 0.3;
					Put(EPiece::Wood, Bx, Gg + 0.15, Bz, 0.32, 0.3, 0.5, -A0, Rgb(0.7, 0.75, 0.6));
				}
			}
		}

		// ---------- barbed wire: posts and sagging strands from post to post ----------
		void Wire()
		{
			auto IsWire = [this](int X, int Y) { return At(X, Y) == 'X'; };
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (!IsWire(X, Y)) continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz);
				auto Tilt = [X, Y](int K) { return (Rnd(X, Y, K) - 0.5) * 0.14; };
				const int Spans[4][2] = { { 1, 0 }, { 0, 1 }, { 1, 1 }, { 1, -1 } };
				for (const auto& D : Spans)
				{
					if (!IsWire(X + D[0], Y + D[1]) || (D[0] && D[1] && (IsWire(X + D[0], Y) || IsWire(X, Y + D[1])))) continue;
					Put(EPiece::Wire, Cx, Gg, Cz, CellMetres * std::hypot((double)D[0], (double)D[1]), 1, 1, std::atan2((double)-D[1], (double)D[0]));
				}
				bool bNear = false;
				for (const auto& D : Dir4) if (IsWire(X + D[0], Y + D[1])) bNear = true;
				for (const auto& D : Diag) if (IsWire(X + D[0], Y + D[1])) bNear = true;
				if (bNear) Put(EPiece::Dark, Cx, Gg + 0.45, Cz, 0.1, 1.15, 0.1, Rnd(X, Y, 130) * 3, 0.9, Tilt(131), Tilt(132));
				else
				{
					const double A = Rnd(X, Y, 133) * Pi, Ux = std::cos(A), Uz = std::sin(A);
					Put(EPiece::Wire, Cx - Ux * 0.9, Gg, Cz - Uz * 0.9, 1.8, 1, 1, std::atan2(-Uz, Ux));
					for (const int S : { -1, 1 }) Put(EPiece::Dark, Cx + S * Ux * 0.9, Gg + 0.45, Cz + S * Uz * 0.9, 0.1, 1.15, 0.1, A, 0.9, Tilt(134 + S), Tilt(136 + S));
				}
			}
		}

		// ---------- tank traps: two steel hedgehogs per cell ----------
		void Traps()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'Y') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Flip = Rnd(X, Y, 140) < 0.5 ? 1 : -1;
				for (const int S : { -1, 1 })
				{
					const double Px = Cx + S * 0.45, Pz = Cz + S * 0.45 * Flip, Sc = 0.95 + 0.15 * Rnd(X, Y, 141 + S), R = Rnd(X, Y, 143 + S);
					Put(EPiece::Hedgehog, Px, G(Px, Pz), Pz, Sc, Sc, Sc, R * 6.28, Mul(Rgb(1.1, 0.86, 0.72), 0.85 + 0.25 * R));
				}
			}
		}

		// ---------- mines: a dark disc half sunk in the turf (the server only tells this side about its own) ----------
		void Mines()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (!Src.Mine(X, Y)) continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres;
				Put(EPiece::Dark, Cx, G(Cx, Cz) + 0.06, Cz, 0.7, 0.14, 0.7, Rnd(X, Y, 150) * 6.28, 0.7);
			}
		}

		// ---------- Field Hospital: a white ridge tent with a red cross on a board over the ridge ----------
		void Hospitals()
		{
			const FRgb Canvas = Rgb(1.25, 1.22, 1.12), Red = Rgb(0.9, 0.12, 0.1);
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != 'A') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz);
				Put(EPiece::Wall, Cx, Gg + 0.45, Cz, 1.9, 0.9, 1.9, 0, Canvas);
				Put(EPiece::Gable, Cx, Gg + 0.9, Cz, 1.9, 2.5, 1.9, 0, Canvas);
				Put(EPiece::Dark, Cx, Gg + 1.5, Cz, 0.08, 3, 0.08);
				Put(EPiece::Paint, Cx, Gg + 2.6, Cz, 0.9, 0.9, 0.06, 0, Rgb(1.3, 1.3, 1.25));
				for (const double S : { -1.0, 1.0 })
				{
					Put(EPiece::Paint, Cx, Gg + 2.6, Cz + S * 0.04, 0.62, 0.18, 0.02, 0, Red);
					Put(EPiece::Paint, Cx, Gg + 2.6, Cz + S * 0.04, 0.18, 0.62, 0.02, 0, Red);
				}
			}
		}

		// ---------- bridges: plank deck, edge beams, railings and stone cutwaters on the water sides ----------
		void Bridges()
		{
			for (int Y = CY0; Y < CY1; Y++) for (int X = CX0; X < CX1; X++)
			{
				if (At(X, Y) != '=') continue;
				CurCell = Y * W + X;
				const double Cx = (X + 0.5) * CellMetres, Cz = (Y + 0.5) * CellMetres, Gg = G(Cx, Cz), Top = Gg + 0.22;
				Put(Src.StoneDeck(X, Y) ? EPiece::Stone : EPiece::Wood, Cx, Top - 0.15, Cz, CellMetres, 0.3, CellMetres, 0, 1.0);
				for (const auto& D : Dir4)
				{
					const unsigned char Nb = At(X + D[0], Y + D[1]);
					if (Nb != 'W' && Nb != 'F') continue;
					const double Yaw = YawOf(D[0], D[1]);
					Put(EPiece::Dark, Cx + D[0] * 0.95, Top - 0.1, Cz + D[1] * 0.95, CellMetres, 0.34, 0.12, Yaw, 0.9);
					Put(EPiece::Rail, Cx + D[0] * 0.9, Top, Cz + D[1] * 0.9, 1, 1, 1, Yaw);
					if ((X + Y) % 2 == 0)
					{
						Put(EPiece::Stone, Cx + D[0], (Gg - 0.6 + Top + 0.3) / 2, Cz + D[1], 1.0, Top + 0.3 - (Gg - 0.6), 1.0, Pi / 4, 0.9);
						Put(EPiece::Stone, Cx + D[0], Top + 0.36, Cz + D[1], 1.1, 0.12, 1.1, Pi / 4, 1.1);
					}
				}
			}
		}
	};

	void House(const FCellSource& Source, const FHouseRect& Rect, std::vector<FPiece>& Out, const FContext& Context)
	{
		FBuilder B(Source, Out);
		B.Foot = Context.Foot; B.TintIndex = Context.TintIndex; B.Tints = Context.Tints;
		B.House(Rect);
	}

	void Chunk(const FCellSource& Source, int X0, int Y0, int X1, int Y1, std::vector<FPiece>& Out, const FContext& Context)
	{
		FBuilder B(Source, Out);
		B.Foot = Context.Foot; B.TintIndex = Context.TintIndex; B.Tints = Context.Tints;
		B.Chunk(X0, Y0, X1, Y1);
	}
}
