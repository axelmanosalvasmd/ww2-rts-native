// The paper and the desk of the war map, painted on the CPU as client/map-view.js paints its two canvases. See
// Client/TCWarMap.h. The random sequence, every colour, size and the order of the drawing calls follow map-view.js
// paint() and paintDesk() line for line, so the specks, stains, ripples and tree circles land where the browser puts them.
#include "Client/TCWarMap.h"
#include <cmath>

namespace TCWarMapPaint
{
	double FMulberry::Next()
	{
		// map-view.js mulberry: a |= 0; a = (a + 0x6d2b79f5) | 0; t = imul(a ^ (a >>> 15), 1 | a);
		// t = (t + imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296
		A = A + 0x6d2b79f5u;
		uint32 T = (A ^ (A >> 15)) * (1u | A);
		T = (T + (T ^ (T >> 7)) * (61u | T)) ^ T;
		return double(T ^ (T >> 14)) / 4294967296.0;
	}

	FRgba Css(uint8 R, uint8 G, uint8 B, double A) { return FRgba{ double(R), double(G), double(B), A }; }

	// CSS rgb() with fractional channels (paintDesk's planks): rounded to the nearest byte
	static uint8 CssByte(double V) { return (uint8)FMath::Clamp((int32)FMath::FloorToDouble(V + 0.5), 0, 255); }

	void FPaintCanvas::Init(int32 InW, int32 InH, FRgba Clear)
	{
		W = InW; H = InH;
		Rgba.SetNumUninitialized(W * H * 4);
		for (int32 I = 0; I < W * H; I++)
		{
			Rgba[I * 4] = (uint8)Clear.R; Rgba[I * 4 + 1] = (uint8)Clear.G; Rgba[I * 4 + 2] = (uint8)Clear.B; Rgba[I * 4 + 3] = (uint8)FMath::RoundToInt(Clear.A * 255);
		}
		Subpaths.Reset();
	}

	void FPaintCanvas::MoveTo(double X, double Y) { Subpaths.AddDefaulted_GetRef().Add(FVector2D(X, Y)); }

	void FPaintCanvas::LineTo(double X, double Y)
	{
		if (!Subpaths.Num()) { MoveTo(X, Y); return; }
		Subpaths.Last().Add(FVector2D(X, Y));
	}

	void FPaintCanvas::QuadTo(double Cx, double Cy, double X, double Y)
	{
		if (!Subpaths.Num()) MoveTo(Cx, Cy);
		const FVector2D P0 = Subpaths.Last().Last(), C(Cx, Cy), P1(X, Y);
		constexpr int32 N = 16;
		for (int32 I = 1; I <= N; I++)
		{
			const double T = double(I) / N, S = 1 - T;
			Subpaths.Last().Add(P0 * (S * S) + C * (2 * S * T) + P1 * (T * T));
		}
	}

	void FPaintCanvas::Arc(double Cx, double Cy, double R, double A0, double A1)
	{
		// Canvas2D arc: a line from the current point to the arc's start, then the arc (clockwise on screen)
		const int32 N = FMath::Clamp((int32)FMath::CeilToDouble(FMath::Abs(A1 - A0) * R / 0.5), 16, 512);
		for (int32 I = 0; I <= N; I++)
		{
			const double A = A0 + (A1 - A0) * I / N;
			const FVector2D P(Cx + FMath::Cos(A) * R, Cy + FMath::Sin(A) * R);
			if (I == 0 && !Subpaths.Num()) MoveTo(P.X, P.Y); else LineTo(P.X, P.Y);
		}
	}

	void FPaintCanvas::Begin(double X0, double Y0, double X1, double Y1)
	{
		const int32 A = FMath::Clamp((int32)FMath::FloorToDouble(X0) - 1, 0, W), B = FMath::Clamp((int32)FMath::CeilToDouble(X1) + 1, 0, W);
		const int32 C = FMath::Clamp((int32)FMath::FloorToDouble(Y0) - 1, 0, H), D = FMath::Clamp((int32)FMath::CeilToDouble(Y1) + 1, 0, H);
		BX0 = A; BY0 = C; BW = B - A; BH = D - C;
		Acc.Reset();
		Acc.SetNumZeroed(FMath::Max(0, (BW + 2) * BH));
	}

	// One edge of a polygon into the accumulation buffer: the signed area it sweeps in each cell (font-rs accumulate),
	// x clipped to the box exactly (a piece outside runs along the box's side), y clipped by the loop.
	void FPaintCanvas::Edge(FVector2D A, FVector2D B)
	{
		if (BW <= 0 || BH <= 0) return;
		A -= FVector2D(BX0, BY0); B -= FVector2D(BX0, BY0);
		// split at x = 0 and x = BW so each piece lies on one side, then clamp
		double Ts[4] = { 0, 1, 1, 1 }; int32 Nt = 1;
		const double Dx = B.X - A.X;
		if (Dx != 0)
		{
			for (const double Bound : { 0.0, double(BW) })
			{
				const double T = (Bound - A.X) / Dx;
				if (T > 0 && T < 1) Ts[Nt++] = T;
			}
		}
		Ts[Nt++] = 1;
		if (Nt == 4 && Ts[1] > Ts[2]) Swap(Ts[1], Ts[2]);
		const int32 Stride = BW + 2;
		for (int32 K = 0; K + 1 < Nt; K++)
		{
			FVector2D P0 = A + (B - A) * Ts[K], P1 = A + (B - A) * Ts[K + 1];
			P0.X = FMath::Clamp(P0.X, 0.0, (double)BW); P1.X = FMath::Clamp(P1.X, 0.0, (double)BW);
			if (P0.Y == P1.Y) continue;
			double Dir = 1;
			if (P0.Y > P1.Y) { Swap(P0, P1); Dir = -1; }
			const double Dxdy = (P1.X - P0.X) / (P1.Y - P0.Y);
			double X = P0.X;
			if (P0.Y < 0) X -= P0.Y * Dxdy;
			const int32 YStart = FMath::Max(0, (int32)FMath::FloorToDouble(P0.Y)), YEnd = FMath::Min(BH, (int32)FMath::CeilToDouble(P1.Y));
			for (int32 Y = YStart; Y < YEnd; Y++)
			{
				float* Row = Acc.GetData() + Y * Stride;
				const double Dy = FMath::Min(double(Y + 1), P1.Y) - FMath::Max(double(Y), P0.Y);
				const double XNext = X + Dxdy * Dy;
				const double D = Dy * Dir;
				const double X0 = FMath::Min(X, XNext), X1 = FMath::Max(X, XNext);
				const double X0f = FMath::FloorToDouble(X0);
				const int32 X0i = (int32)X0f;
				const double X1c = FMath::CeilToDouble(X1);
				const int32 X1i = (int32)X1c;
				if (X1i <= X0i + 1)
				{
					const double Xmf = 0.5 * (X + XNext) - X0f;
					Row[X0i] += float(D - D * Xmf);
					Row[X0i + 1] += float(D * Xmf);
				}
				else
				{
					const double S = 1.0 / (X1 - X0);
					const double X0fr = X0 - X0f;
					const double A0 = 0.5 * S * (1 - X0fr) * (1 - X0fr);
					const double X1fr = X1 - X1c + 1;
					const double Am = 0.5 * S * X1fr * X1fr;
					Row[X0i] += float(D * A0);
					if (X1i == X0i + 2) Row[X0i + 1] += float(D * (1 - A0 - Am));
					else
					{
						const double A1 = S * (1.5 - X0fr);
						Row[X0i + 1] += float(D * (A1 - A0));
						for (int32 Xi = X0i + 2; Xi < X1i - 1; Xi++) Row[Xi] += float(D * S);
						const double A2 = A1 + (X1i - X0i - 3) * S;
						Row[X1i - 1] += float(D * (1 - A2 - Am));
					}
					Row[X1i] += float(D * Am);
				}
				X = XNext;
			}
		}
	}

	// A closed polygon, turned to a positive area so overlapping pieces of one stroke add up (and clamp at full)
	void FPaintCanvas::Polygon(const TArray<FVector2D>& P)
	{
		if (P.Num() < 3) return;
		double Area = 0;
		for (int32 I = 0; I < P.Num(); I++) { const FVector2D& A = P[I]; const FVector2D& B = P[(I + 1) % P.Num()]; Area += A.X * B.Y - B.X * A.Y; }
		if (Area == 0) return;
		for (int32 I = 0; I < P.Num(); I++)
		{
			const int32 J = (I + 1) % P.Num();
			if (Area > 0) Edge(P[I], P[J]); else Edge(P[J], P[I]);
		}
	}

	void FPaintCanvas::Blend(int32 X, int32 Y, const FRgba& C, double Coverage)
	{
		const double A = C.A * Coverage;
		if (A <= 0) return;
		uint8* D = Rgba.GetData() + (Y * W + X) * 4;
		// source-over on display values (a browser canvas)
		D[0] = (uint8)FMath::Clamp(FMath::RoundToInt(C.R * A + D[0] * (1 - A)), 0, 255);
		D[1] = (uint8)FMath::Clamp(FMath::RoundToInt(C.G * A + D[1] * (1 - A)), 0, 255);
		D[2] = (uint8)FMath::Clamp(FMath::RoundToInt(C.B * A + D[2] * (1 - A)), 0, 255);
		D[3] = (uint8)FMath::Clamp(FMath::RoundToInt(255.0 * (A + D[3] / 255.0 * (1 - A))), 0, 255);
	}

	template <typename F>
	void FPaintCanvas::Resolve(F&& ColourAt)
	{
		const int32 Stride = BW + 2;
		for (int32 Y = 0; Y < BH; Y++)
		{
			const float* Row = Acc.GetData() + Y * Stride;
			double S = 0;
			for (int32 X = 0; X < BW; X++)
			{
				S += Row[X];
				const double Cov = FMath::Min(1.0, FMath::Abs(S));
				if (Cov > 1e-4) Blend(BX0 + X, BY0 + Y, ColourAt(BX0 + X, BY0 + Y), Cov);
			}
		}
	}

	void FPaintCanvas::StrokeSubpath(const TArray<FVector2D>& In, double Width)
	{
		// butt caps, miter joins (limit 10, else bevel): Canvas2D's defaults
		TArray<FVector2D> Pts;
		for (const FVector2D& P : In) if (!Pts.Num() || !P.Equals(Pts.Last(), 1e-9)) Pts.Add(P);
		const double Hw = Width / 2;
		for (int32 I = 0; I + 1 < Pts.Num(); I++)
		{
			const FVector2D D = (Pts[I + 1] - Pts[I]).GetSafeNormal(), N(-D.Y * Hw, D.X * Hw);
			Polygon({ Pts[I] + N, Pts[I + 1] + N, Pts[I + 1] - N, Pts[I] - N });
		}
		for (int32 I = 1; I + 1 < Pts.Num(); I++)
		{
			const FVector2D D1 = (Pts[I] - Pts[I - 1]).GetSafeNormal(), D2 = (Pts[I + 1] - Pts[I]).GetSafeNormal();
			const double Cross = D1.X * D2.Y - D1.Y * D2.X;
			if (FMath::Abs(Cross) < 1e-9) continue;
			const FVector2D N1(-D1.Y * Hw, D1.X * Hw), N2(-D2.Y * Hw, D2.X * Hw);
			// the outer side: where the two offsets part (a right turn on screen opens the left offsets, and so on)
			const double Side = Cross > 0 ? -1.0 : 1.0;
			const FVector2D V = Pts[I], A = V + N1 * Side, B = V + N2 * Side;
			const double CosTheta = FMath::Clamp(-(D1 | D2), -1.0, 1.0); // angle between the two segments at the joint
			const double Theta = FMath::Acos(CosTheta);
			const double MiterRatio = 1.0 / FMath::Max(1e-9, FMath::Sin(Theta / 2));
			if (MiterRatio <= 10.0)
			{
				const FVector2D Mid = ((N1 + N2) * Side).GetSafeNormal();
				Polygon({ V, A, V + Mid * (Hw * MiterRatio), B });
			}
			else Polygon({ V, A, B });
		}
	}

	void FPaintCanvas::Stroke(const FRgba& Colour, double Width)
	{
		double X0 = 1e30, Y0 = 1e30, X1 = -1e30, Y1 = -1e30;
		for (const TArray<FVector2D>& S : Subpaths) for (const FVector2D& P : S) { X0 = FMath::Min(X0, P.X); Y0 = FMath::Min(Y0, P.Y); X1 = FMath::Max(X1, P.X); Y1 = FMath::Max(Y1, P.Y); }
		if (X0 > X1) return;
		// a miter reaches at most 10 half widths out
		const double M = Width * 5 + 1;
		Begin(X0 - M, Y0 - M, X1 + M, Y1 + M);
		for (const TArray<FVector2D>& S : Subpaths) StrokeSubpath(S, Width);
		Resolve([&](int32, int32) { return Colour; });
	}

	void FPaintCanvas::FillRect(double X, double Y, double RW, double RH, const FRgba& Colour)
	{
		if (RW < 0) { X += RW; RW = -RW; }
		if (RH < 0) { Y += RH; RH = -RH; }
		if (RW <= 0 || RH <= 0) return;
		Begin(X, Y, X + RW, Y + RH);
		Polygon({ FVector2D(X, Y), FVector2D(X + RW, Y), FVector2D(X + RW, Y + RH), FVector2D(X, Y + RH) });
		Resolve([&](int32, int32) { return Colour; });
	}

	void FPaintCanvas::FillRectRadial(double X, double Y, double RW, double RH, double Cx, double Cy, double R0, double R1, const TArray<TPair<double, FRgba>>& Stops)
	{
		if (RW <= 0 || RH <= 0 || !Stops.Num()) return;
		Begin(X, Y, X + RW, Y + RH);
		Polygon({ FVector2D(X, Y), FVector2D(X + RW, Y), FVector2D(X + RW, Y + RH), FVector2D(X, Y + RH) });
		Resolve([&](int32 Px, int32 Py)
		{
			// concentric circles: the share t of the way from r0 to r1, padded at both ends; stops interpolated linearly
			const double D = FMath::Sqrt(FMath::Square(Px + 0.5 - Cx) + FMath::Square(Py + 0.5 - Cy));
			const double T = FMath::Clamp((D - R0) / FMath::Max(1e-9, R1 - R0), 0.0, 1.0);
			if (T <= Stops[0].Key) return Stops[0].Value;
			for (int32 I = 1; I < Stops.Num(); I++)
				if (T <= Stops[I].Key)
				{
					const double K = (T - Stops[I - 1].Key) / FMath::Max(1e-9, Stops[I].Key - Stops[I - 1].Key);
					const FRgba& A = Stops[I - 1].Value; const FRgba& B = Stops[I].Value;
					return FRgba{ A.R + (B.R - A.R) * K, A.G + (B.G - A.G) * K, A.B + (B.B - A.B) * K, A.A + (B.A - A.A) * K };
				}
			return Stops.Last().Value;
		});
	}

	void FPaintCanvas::StrokeRect(double X, double Y, double RW, double RH, const FRgba& Colour, double Width)
	{
		// the rectangle's outline as four bands that meet square at the corners (miter joins of right angles)
		const double Hw = Width / 2;
		Begin(X - Hw, Y - Hw, X + RW + Hw, Y + RH + Hw);
		Polygon({ FVector2D(X - Hw, Y - Hw), FVector2D(X + RW + Hw, Y - Hw), FVector2D(X + RW + Hw, Y + Hw), FVector2D(X - Hw, Y + Hw) });
		Polygon({ FVector2D(X - Hw, Y + RH - Hw), FVector2D(X + RW + Hw, Y + RH - Hw), FVector2D(X + RW + Hw, Y + RH + Hw), FVector2D(X - Hw, Y + RH + Hw) });
		Polygon({ FVector2D(X - Hw, Y + Hw), FVector2D(X + Hw, Y + Hw), FVector2D(X + Hw, Y + RH - Hw), FVector2D(X - Hw, Y + RH - Hw) });
		Polygon({ FVector2D(X + RW - Hw, Y + Hw), FVector2D(X + RW + Hw, Y + Hw), FVector2D(X + RW + Hw, Y + RH - Hw), FVector2D(X + RW - Hw, Y + RH - Hw) });
		Resolve([&](int32, int32) { return Colour; });
	}

	void FPaintCanvas::FillRectShadowed(double X, double Y, double RW, double RH, const FRgba& Fill, const FRgba& Shadow, double Blur)
	{
		// Canvas2D shadows: the shape's alpha blurred with a Gaussian of sigma = shadowBlur / 2, in shadowColor, drawn
		// under the shape. A rectangle's blur is separable: the product of two erf profiles.
		const double Sigma = FMath::Max(1e-6, Blur / 2), K = 1.0 / (Sigma * FMath::Sqrt(2.0));
		const double Reach = Sigma * 4;
		const int32 X0 = FMath::Clamp((int32)FMath::FloorToDouble(X - Reach), 0, W), X1 = FMath::Clamp((int32)FMath::CeilToDouble(X + RW + Reach), 0, W);
		const int32 Y0 = FMath::Clamp((int32)FMath::FloorToDouble(Y - Reach), 0, H), Y1 = FMath::Clamp((int32)FMath::CeilToDouble(Y + RH + Reach), 0, H);
		TArray<double> Gx, Gy;
		Gx.SetNum(FMath::Max(0, X1 - X0)); Gy.SetNum(FMath::Max(0, Y1 - Y0));
		for (int32 I = X0; I < X1; I++) { const double P = I + 0.5; Gx[I - X0] = 0.5 * (std::erf((P - X) * K) - std::erf((P - X - RW) * K)); }
		for (int32 J = Y0; J < Y1; J++) { const double P = J + 0.5; Gy[J - Y0] = 0.5 * (std::erf((P - Y) * K) - std::erf((P - Y - RH) * K)); }
		const FRgba S{ Shadow.R, Shadow.G, Shadow.B, Shadow.A * Fill.A };
		for (int32 J = Y0; J < Y1; J++) for (int32 I = X0; I < X1; I++) Blend(I, J, S, Gx[I - X0] * Gy[J - Y0]);
		FillRect(X, Y, RW, RH, Fill);
	}

	int32 PixelsPerCell(int32 Wc, int32 Hc)
	{
		// map-view.js: PX = max(2, min(12, floor(2048 / max(w, h))))
		return FMath::Max(2, FMath::Min(12, 2048 / FMath::Max(1, FMath::Max(Wc, Hc))));
	}

	void PaintPaper(const FPaperInput& In, FPaintCanvas& C)
	{
		const int32 w = In.W, h = In.H, PX = PixelsPerCell(w, h), Wp = w * PX, Hp = h * PX;
		const FRgba PAPER = Css(0xe8, 0xde, 0xc3), INK = Css(0x3b, 0x32, 0x26), CONTOUR = Css(0x9a, 0x6b, 0x3c), WATER = Css(0x9f, 0xbc, 0xc4), WOOD = Css(0xb5, 0xc1, 0x94);
		const FRgba GRID = Css(60, 90, 130, 0.22);
		const bool bAllKnown = In.Known.Num() != w * h;
		auto Known = [&](int32 X, int32 Y) { return bAllKnown || In.Known[Y * w + X]; };
		// an unknown cell is the '.' the server's World map holds for it
		auto At = [&](int32 X, int32 Y) -> TCHAR { if (X < 0 || Y < 0 || X >= w || Y >= h) return 0; return Known(X, Y) ? (TCHAR)In.Chars[Y * w + X] : TEXT('.'); };
		FMulberry Rnd(7);
		C.Init(Wp, Hp);
		C.FillRect(0, 0, Wp, Hp, PAPER);
		const double Specks = double(Wp) * Hp / 60;
		for (int32 I = 0; I < Specks; I++)
		{
			const double A = Rnd.Next() * 0.06;
			const double X = Rnd.Next() * Wp, Y = Rnd.Next() * Hp, RW = 1 + Rnd.Next() * 2, RH = 1 + Rnd.Next() * 2;
			C.FillRect(X, Y, RW, RH, Css(90, 70, 40, A));
		}
		for (int32 I = 0; I < 6; I++) // coffee and damp stains
		{
			const double X = Rnd.Next() * Wp, Y = Rnd.Next() * Hp, R = (0.05 + Rnd.Next() * 0.12) * FMath::Max(Wp, Hp);
			C.FillRectRadial(X - R, Y - R, R * 2, R * 2, X, Y, R * 0.2, R, { { 0.0, Css(150, 110, 60, 0.05) }, { 0.85, Css(150, 110, 60, 0.09) }, { 1.0, Css(150, 110, 60, 0) } });
		}
		// water and woods as washes, then houses, roads and the rest in ink
		for (int32 Y = 0; Y < h; Y++) for (int32 X = 0; X < w; X++)
		{
			const TCHAR Ch = At(X, Y);
			if (Ch == 'W' || Ch == 'F') C.FillRect(X * PX, Y * PX, PX, PX, WATER);
			else if (Ch == 'O' || Ch == 'H') C.FillRect(X * PX, Y * PX, PX, PX, WOOD);
			else if (Ch == 'M') C.FillRect(X * PX, Y * PX, PX, PX, Css(120, 95, 60, 0.25));
		}
		const double Thin = FMath::Max(1.0, PX * 0.12);
		for (int32 Y = 0; Y < h; Y += 2) for (int32 X = 0; X < w; X += 3) // ripples along the water
			if (At(X, Y) == 'W' && Rnd.Next() < 0.5)
			{
				const double Xp = X * PX, Yp = Y * PX + PX / 2.0;
				C.BeginPath(); C.MoveTo(Xp, Yp); C.QuadTo(Xp + PX * 0.5, Yp - PX * 0.4, Xp + PX, Yp); C.QuadTo(Xp + PX * 1.5, Yp + PX * 0.4, Xp + PX * 2, Yp);
				C.Stroke(Css(0x5f, 0x7f, 0x88), Thin);
			}
		for (int32 Y = 0; Y < h; Y++) for (int32 X = 0; X < w; X++) // little tree circles in the woods
			if (At(X, Y) == 'O' && Rnd.Next() < 0.55)
			{
				const double Cx = (X + 0.2 + Rnd.Next() * 0.6) * PX, Cy = (Y + 0.2 + Rnd.Next() * 0.6) * PX, R = PX * (0.25 + Rnd.Next() * 0.15);
				C.BeginPath(); C.Arc(Cx, Cy, R, 0, PI * 2);
				C.Stroke(Css(0x5e, 0x6d, 0x3e), Thin);
			}
		// contours: an ink line wherever the ground steps a level (never on or beside ground this side does not know)
		C.BeginPath();
		for (int32 Y = 0; Y < h; Y++) for (int32 X = 0; X < w; X++)
		{
			if (!Known(X, Y)) continue;
			const int8 L = In.Level[Y * w + X];
			if (X + 1 < w && Known(X + 1, Y) && In.Level[Y * w + X + 1] != L) { C.MoveTo((X + 1) * PX, Y * PX); C.LineTo((X + 1) * PX, (Y + 1) * PX); }
			if (Y + 1 < h && Known(X, Y + 1) && In.Level[(Y + 1) * w + X] != L) { C.MoveTo(X * PX, (Y + 1) * PX); C.LineTo((X + 1) * PX, (Y + 1) * PX); }
		}
		C.Stroke(CONTOUR, FMath::Max(1.0, PX * 0.18));
		for (int32 Y = 0; Y < h; Y++) for (int32 X = 0; X < w; X++)
		{
			const TCHAR Ch = At(X, Y);
			const double Xp = X * PX, Yp = Y * PX;
			if (Ch == 'D' || Ch == '=')
			{
				const FRgba F = Ch == '=' ? INK : Css(0x8a, 0x6e, 0x4c);
				C.FillRect(Xp, Yp + PX * 0.3, PX, PX * 0.4, F); C.FillRect(Xp + PX * 0.3, Yp, PX * 0.4, PX, F);
			}
			else if (Ch == 'B' || Ch == 'K') C.FillRect(Xp - 0.5, Yp - 0.5, PX + 1, PX + 1, INK);
			else if (Ch == 'R' || Ch == '#' || Ch == 'Q') C.FillRect(Xp + PX * 0.15, Yp + PX * 0.15, PX * 0.7, PX * 0.7, Css(59, 50, 38, 0.55));
			else if (Ch == 'T')
			{
				C.BeginPath(); C.MoveTo(Xp, Yp + PX * 0.5); C.LineTo(Xp + PX * 0.33, Yp + PX * 0.2); C.LineTo(Xp + PX * 0.66, Yp + PX * 0.8); C.LineTo(Xp + PX, Yp + PX * 0.5);
				C.Stroke(INK, FMath::Max(1.0, PX * 0.15));
			}
			else if (Ch == 'X' || Ch == 'Y')
			{
				C.BeginPath(); C.MoveTo(Xp, Yp); C.LineTo(Xp + PX, Yp + PX); C.MoveTo(Xp + PX, Yp); C.LineTo(Xp, Yp + PX);
				C.Stroke(INK, FMath::Max(1.0, PX * 0.1));
			}
		}
		// a blue map grid every 10 cells and an inked border
		C.BeginPath();
		for (int32 X = 0; X <= w; X += 10) { C.MoveTo(X * PX, 0); C.LineTo(X * PX, Hp); }
		for (int32 Y = 0; Y <= h; Y += 10) { C.MoveTo(0, Y * PX); C.LineTo(Wp, Y * PX); }
		C.Stroke(GRID, 1);
		C.StrokeRect(PX, PX, Wp - 2 * PX, Hp - 2 * PX, INK, FMath::Max(2.0, PX * 0.4));
	}

	void PaintDesk(FPaintCanvas& C, int32 S, double Span, double Fx, double Fz)
	{
		// dark wooden planks with grain, and the paper's soft shadow in the middle (the map covers 1/span of it)
		FMulberry Rnd(3);
		const double Plank = S / 14.0;
		C.Init(S, S);
		C.FillRect(0, 0, S, S, Css(0x3a, 0x27, 0x17));
		for (int32 I = 0; I < 14; I++)
		{
			const double R = 52 + Rnd.Next() * 14, G = 35 + Rnd.Next() * 9, B = 21 + Rnd.Next() * 6;
			C.FillRect(0, I * Plank + 1, S, Plank - 2, Css(CssByte(R), CssByte(G), CssByte(B)));
			for (int32 K = 0; K < 9; K++)
			{
				const double Y = I * Plank + Rnd.Next() * Plank;
				C.BeginPath(); C.MoveTo(0, Y);
				for (int32 X = 0; X <= S; X += 64) C.LineTo(X, Y + FMath::Sin(X / (90.0 + K * 20) + K) * 3);
				C.Stroke(Css(20, 12, 6, 0.35), 1.2);
			}
		}
		const double Mw = S / Span * Fx, Mh = S / Span * Fz;
		C.FillRectShadowed((S - Mw) / 2 + Mw * 0.01, (S - Mh) / 2 + Mh * 0.02, Mw, Mh, Css(0, 0, 0, 0.6), Css(0, 0, 0, 0.75), FMath::Max(Mw, Mh) * 0.06);
	}
}
