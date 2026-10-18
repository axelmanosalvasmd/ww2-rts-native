// ATCHUD's slice 9 drawing (see TCHUD.h): the score strip, capture points, plan lines, health bars, alert feed and
// history, the epilogue's stamp, the Controls sheet and the lobby's match report. Everything is read from FTCMatchState
// (this recipient's own messages) and FTCObjectives, which is built from it; nothing is inferred for what a snapshot
// did not deliver. Canvas drawing only: placeholder presentation in the browser's colours, not its art.
#include "Client/TCHUD.h"
#include "Client/TCStats.h"
#include "Client/TCMarkers.h"
#include "ThreeCrossroads.h"
#include "Client/TCLocale.h"
#include "Client/TCGameMode.h"
#include "Client/TCObjectives.h"
#include "Client/TCEndReport.h"
#include "Client/TCBossBar.h"
#include "Client/TCAlerts.h"
#include "Client/TCKeys.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCArmyCommands.h"
#include "Client/TCOrders.h"
#include "Client/TCRecruitment.h"
#include "World/TCTerrain.h"
#include "Engine/Canvas.h"
#include "Engine/Font.h"
#include "CanvasItem.h"
#include "GlobalRenderResources.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Camera/PlayerCameraManager.h"

namespace TCHudSlice9
{
	constexpr float LineH = 16.f, PadPx = 6.f;
	const FLinearColor TextC(FColor(0xe2, 0xdf, 0xd3)), DimC(0.72f, 0.72f, 0.68f), Brass(FColor(0xd6, 0xb2, 0x5e)), Red(FColor(0xee, 0x8a, 0x7b));
	const FLinearColor Plate(FColor(25, 28, 30, 235)), Hairline(FColor(176, 164, 122, 117));
	// round 17: index.html #scores: one card on --panel-solid (rgba(25, 28, 30, 0.96)), its blocks split by a 1 px
	// --line-soft rule (#scores > div + div { border-left })
	const FLinearColor PanelSolid = TCCss::Rgba(25, 28, 30, 0.96f), LineSoft = TCCss::Rgba(176, 164, 122, 0.24f);
	// client/markers.js: the neutral ring and flag, the brass flip and danger rings, the white track and arc
	const FColor NeutralRing(0xe6, 0xe0, 0xd0), NeutralFlag(0x8f, 0x8a, 0x7c), BrassRing(0xd2, 0xa8, 0x49), White(0xf4, 0xf2, 0xea);
	// client/markers.js PLAN_COLORS: blue move, orange attack-move, white retreat, red attack, yellow dig and build
	FLinearColor PlanColour(int32 Kind)
	{
		switch (Kind)
		{
		case 2: return FLinearColor(FColor(0xf0, 0x93, 0x2f));
		case 3: return FLinearColor(FColor(0xf4, 0xf2, 0xea));
		case 4: case 5: case 6: return FLinearColor(FColor(0xe5, 0x48, 0x3b));
		case 7: case 8: return FLinearColor(FColor(0xe6, 0xc3, 0x50));
		default: return FLinearColor(FColor(0x5f, 0xa4, 0xf2));
		}
	}
	const FLinearColor AttackColour(FColor(0xe5, 0x48, 0x3b));
	FLinearColor WithAlpha(FLinearColor C, float A) { C.A = A; return C; }
	// client/main.js barY: the bar's height over the unit's ground, in metres
	float BarY(const FString& Type, const FTCUnitDef* D)
	{
		if (D && D->bAir) return ATCUnitRenderer::AirAltitude + 2.5f;
		if (Type == TEXT("bunker") || Type == TEXT("tower") || Type == TEXT("hq") || Type == TEXT("barracks") || Type == TEXT("motorpool") || Type == TEXT("shipyard")) return 7.5f;
		if (Type == TEXT("destroyer") || Type == TEXT("kaiju")) return 24.f;
		if (Type == TEXT("gunboat")) return 5.f;
		if (Type == TEXT("depot") || Type == TEXT("pillbox")) return 4.5f;
		if (Type == TEXT("tiger") || Type == TEXT("churchill") || Type == TEXT("medium")) return 4.2f;
		return D && !D->bInfantry ? 3.4f : 2.4f;
	}
	// hud.js clock and alert-history's time: m:ss
	FString Clock(double Seconds)
	{
		const int32 S = FMath::Max(0, (int32)Seconds);
		return FString::Printf(TEXT("%d:%02d"), S / 60, S % 60);
	}
	FVector2D Along(FVector2D A, FVector2D B) { const FVector2D D = B - A; const double L = D.Size(); return L > 1e-6 ? D / L : FVector2D(1, 0); }
}

void ATCHUD::FillQuad(const FVector2D (&P)[4], const FLinearColor& Colour)
{
	if (!Canvas) return;
	// the canvas drops triangles of one winding: order the corners as the minimap's discs are (a positive cross
	// product in pixels, y down), whichever way the camera sees the quad
	const double Cross = (P[1].X - P[0].X) * (P[2].Y - P[0].Y) - (P[1].Y - P[0].Y) * (P[2].X - P[0].X);
	const FVector2D Q[4] = { P[0], Cross >= 0 ? P[1] : P[3], P[2], Cross >= 0 ? P[3] : P[1] };
	FCanvasTriangleItem A(Q[0], Q[1], Q[2], GWhiteTexture), B(Q[0], Q[2], Q[3], GWhiteTexture);
	A.SetColor(Colour); B.SetColor(Colour);
	A.BlendMode = B.BlendMode = SE_BLEND_Translucent;
	Canvas->DrawItem(A);
	Canvas->DrawItem(B);
}

TArray<FVector2D> FTCHudFit::QueueDashSpans(double Length, double Period, double Lit)
{
	TArray<FVector2D> Spans;
	if (!FMath::IsFinite(Length) || !FMath::IsFinite(Period) || !FMath::IsFinite(Lit) || Length <= 0 || Period <= 0 || Lit <= 0) return Spans;
	Lit = FMath::Min(Lit, Period);
	for (double End = Length; End > 0; End -= Period) Spans.Emplace(FMath::Max(0.0, End - Lit), End);
	return Spans;
}

double FTCHudFit::QueueDashAlpha(double FromGoal, double PixelWidth, double Period, double Lit)
{
	if (!FMath::IsFinite(FromGoal) || !FMath::IsFinite(PixelWidth) || !FMath::IsFinite(Period) || !FMath::IsFinite(Lit) || Period <= 0 || Lit <= 0) return 0;
	double Phase = FMath::Fmod(FromGoal, Period);
	if (Phase < 0) Phase += Period; // GLSL mod, including negative distances.
	const double Aa = FMath::Max(PixelWidth, 1e-3), On = FMath::Min(Lit, Period);
	return TCMarkersCore::GlslSmoothstep(0, Aa, Phase) * (1 - TCMarkersCore::GlslSmoothstep(On - Aa, On, Phase));
}

double FTCHudFit::QueueDashPixelWidth(FVector2D Along, FVector2D Across)
{
	const double Det = Along.X * Across.Y - Along.Y * Across.X;
	if (Along.ContainsNaN() || Across.ContainsNaN() || FMath::Abs(Det) < 1e-9) return 1e-3;
	// Invert the screen Jacobian: u changes by one metre along the ribbon and stays constant across it.
	// Perspective can skew those axes, so the reciprocal length of Along alone is insufficient.
	return FMath::Max((FMath::Abs(Across.X) + FMath::Abs(Across.Y)) / FMath::Abs(Det), 1e-3);
}

void ATCHUD::GroundDashes(const ATCTerrain& T, FVector2D A, FVector2D B, const FLinearColor& Colour, float Thickness, float Period, float Lit)
{
	const double L = FVector2D::Distance(A, B);
	if (L < 0.05) return;
	const FVector2D D = (B - A) / L;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M) return;
	FTCMarkerPainter Painter = TCMarkersHud::Painter(Canvas, *M);
	Painter.SetDepthTest(false); // markers.js planLayer draws queued orders through models.
	TCMarkersCore::FRibbon Ribbon;
	Ribbon.W = Thickness; Ribbon.Min = 1.1; Ribbon.Y = 0.12; Ribbon.Step = 1.6;
	const FColor Hex = Colour.ToFColorSRGB();
	TCMarkersCore::FLook Look;
	Look.Hex = (uint32(Hex.R) << 16) | (uint32(Hex.G) << 8) | Hex.B;
	// Count from the goal, then paint each lit span with overlay.js's rim and soft edge. A solid canvas stroke of
	// the same outer width made the whole 2.2px minimum bright, instead of only the narrow reference core.
	for (const FVector2D& Span : FTCHudFit::QueueDashSpans(L, Period, Lit))
	{
		auto Pixel = [&](FVector2D At)
		{
			return Project(ATCTerrain::ToWorld(At.X, At.Y, T.HeightAt(At.X, At.Y) + Ribbon.Y));
		};
		const double Mid = (Span.X + Span.Y) / 2;
		const FVector2D At = A + D * Mid, Across(-D.Y, D.X);
		const FVector P0 = Pixel(At - D * .5), P1 = Pixel(At + D * .5);
		const FVector S0 = Pixel(At - Across * .5), S1 = Pixel(At + Across * .5);
		const FVector2D Px(P1.X - P0.X, P1.Y - P0.Y);
		const FVector2D Sx(S1.X - S0.X, S1.Y - S0.Y);
		// overlay.js fwidth(u) = |du/dx| + |du/dy|, in metres per screen pixel.
		const double Aa = P0.Z > 0 && P1.Z > 0 && S0.Z > 0 && S1.Z > 0 ? FTCHudFit::QueueDashPixelWidth(Px, Sx) : 1e-3;
		TArray<double, TInlineAllocator<12>> Cuts;
		Cuts.Add(Span.X); Cuts.Add(Span.Y);
		const double FullStart = Span.Y - FMath::Min(double(Lit), double(Period));
		for (int32 K = 0; K <= 4; K++)
		{
			// Quarter-pixel slices only at the fading ends; the lit interior stays one ribbon segment.
			for (double S : {FullStart + K * Aa / 4, Span.Y - K * Aa / 4})
				if (S > Span.X + 1e-7 && S < Span.Y - 1e-7) Cuts.AddUnique(S);
		}
		Cuts.Sort();
		for (int32 K = 1; K < Cuts.Num(); K++)
		{
			Look.Opacity = Colour.A * FTCHudFit::QueueDashAlpha(L - (Cuts[K - 1] + Cuts[K]) / 2, Aa, Period, Lit);
			const FVector2D From = A + D * Cuts[K - 1], To = A + D * Cuts[K];
			const double Points[] = { From.X, From.Y, To.X, To.Y };
			Painter.Line(Points, 2, Ribbon, Look);
		}
	}
}

namespace
{
	// Chromium lays out line boxes in 1/64 screen pixels and snaps CSS borders to device pixels under zoom.
	float CssLine(float Px, float Scale) { return FMath::FloorToFloat(Px * Scale * 64.f) / (64.f * Scale); }
	float CssBorder(float Scale) { return FMath::Max(1.f, FMath::FloorToFloat(Scale)) / Scale; }
	float ScoreSmallOffset(float Scale)
	{
		// Barlow's hhea ascender/descender are 1000/-200 per 1000 units. Blink rounds the font metrics and floors
		// the baseline to a device pixel. .sc-val aligns its 13px/1.35 labels with the 19px/1 numbers by baseline.
		auto Baseline = [Scale](float Px, float Line)
		{
			return FMath::FloorToFloat((CssLine(Line, Scale) * Scale + FMath::RoundToFloat(Px * Scale) - FMath::RoundToFloat(Px * .2f * Scale)) / 2.f);
		};
		return (Baseline(19, 19) - Baseline(13, 17.55f)) / Scale;
	}
}

FTCPaint FTCHudFit::NoticeCard(const FString& Shown, float ViewW, float Top, FLinearColor Colour, const FTCMeasure& Measure, float Scale, bool bEliminated)
{
	// endgame.css: normal whitespace and wrapping, 13px/1.35, max-width: var(--vw) - 660px.
	FTCPaint P;
	const float Border = CssBorder(Scale), Pad = 24.f + 2 * Border, Line = CssLine(17.55f, Scale);
	const float NaturalW = FMath::CeilToFloat(Measure(Shown, 13, false, 0) * Scale * 64.f) / (Scale * 64.f);
	const float W = FMath::Max(Pad, FMath::Min(ViewW - 660.f, NaturalW + Pad));
	const TArray<FString> Lines = FTCPaint::Wrap(Shown, W - Pad + 0.001f, 13, false, Measure);
	const float H = Lines.Num() * Line + 6.f + 2 * Border, X = (ViewW - W) / 2;
	P.Rect(X, Top, W, H, bEliminated ? TCCss::Rgba(92, 34, 28, .92f) : TCCss::Panel, TEXT("notice:panel"));
	P.Frame(X, Top, W, H, bEliminated ? TCCss::Rgba(200, 72, 59, .8f) : TCCss::LineSoft, Border);
	for (int32 I = 0; I < Lines.Num(); I++) P.Text(Lines[I], ViewW / 2, Top + Border + 3 + I * Line, 13, Colour, false, ETCPaintAlign::Centre, TEXT("notice:text")).Size.Y = Line;
	P.Panels.Add(FBox2D(FVector2D(X,Top),FVector2D(X+W,Top+H)));
	return P;
}

FTCPaint FTCHudFit::MatchNoticeCard(const FString& Rate, const FString& Bonus, const FString& Notice, float ViewW, float Top, const FTCMeasure& Measure, float Scale)
{
	// endgame.css: separate flex spans, 16px horizontal / 2px row gaps; the notice takes its own full row.
	FTCPaint P;
	struct FSpan { FString Text; float W; FLinearColor Colour; };
	TArray<FSpan> Spans;
	auto Add = [&](const FString& Text, FLinearColor Colour)
	{
		if (!Text.IsEmpty()) Spans.Add({Text, FMath::CeilToFloat(Measure(Text, 13, false, 0) * Scale * 64.f) / (Scale * 64.f), Colour});
	};
	Add(Rate, TCCss::Text); Add(Bonus, TCCss::Hex(0xa9b37b)); Add(Notice, TCCss::Brass);
	if (!Spans.Num()) return P;
	const float Border = CssBorder(Scale), Pad = 24.f + 2 * Border, Line = CssLine(17.55f, Scale);
	float Natural = 0;
	for (const FSpan& Span : Spans) Natural += Span.W;
	Natural += (Spans.Num() - 1) * 16.f;
	const float W = FMath::Max(Pad, FMath::Min(ViewW - 660.f, Natural + Pad)), Inner = W - Pad, X = (ViewW - W) / 2;
	float Y = Top + Border + 3;
	const int32 Inline = Spans.Num() - (Notice.IsEmpty() ? 0 : 1);
	auto DrawSpan = [&](const FSpan& Span, float Centre, float Width)
	{
		const TArray<FString> Lines = FTCPaint::Wrap(Span.Text, Width + .001f, 13, false, Measure);
		for (int32 I = 0; I < Lines.Num(); I++) P.Text(Lines[I], Centre, Y + I * Line, 13, Span.Colour, false, ETCPaintAlign::Centre, TEXT("notice:text")).Size.Y = Line;
		return Lines.Num() * Line;
	};
	for (int32 I = 0; I < Inline; )
	{
		if (I + 1 < Inline && Spans[I].W + 16 + Spans[I + 1].W <= Inner)
		{
			const float Total = Spans[I].W + 16 + Spans[I + 1].W;
			DrawSpan(Spans[I], ViewW / 2 - Total / 2 + Spans[I].W / 2, Spans[I].W);
			DrawSpan(Spans[I + 1], ViewW / 2 + Total / 2 - Spans[I + 1].W / 2, Spans[I + 1].W);
			Y += Line + 2; I += 2;
		}
		else { Y += DrawSpan(Spans[I], ViewW / 2, FMath::Min(Inner, Spans[I].W)) + 2; I++; }
	}
	if (!Notice.IsEmpty()) Y += DrawSpan(Spans.Last(), ViewW / 2, Inner) + 2;
	const float H = Y - 2 + Border + 3 - Top;
	FTCPaint Card;
	Card.Rect(X, Top, W, H, TCCss::Panel, TEXT("notice:panel"));
	Card.Frame(X, Top, W, H, TCCss::LineSoft, Border);
	Card.Append(P);
	Card.Panels.Add(FBox2D(FVector2D(X, Top), FVector2D(X + W, Top + H)));
	return Card;
}

FTCPaint FTCHudFit::ScoreCard(const FTCScoreStrip& S, float ViewW, const FTCMeasure& Measure, bool bNextWave, float Scale)
{
	// index.html #scores, .sc-lead, .sc-team. Flex items keep their width and wrap as whole blocks.
	FTCPaint P;
	const float Border = CssBorder(Scale), SmallH = CssLine(17.55f, Scale), ModeH = CssLine(15.6f, Scale), ClockH = CssLine(27.3f, Scale);
	const float SmallOffset = ScoreSmallOffset(Scale), ValueH = FMath::Max(19.f, SmallOffset + SmallH);
	const float LeadW = S.Lead.bShown ? (S.Kind == TEXT("horde") || S.Kind == TEXT("assault") ? 216.f : 150.f) : 0.f;
	const float TeamW = FMath::Clamp((ViewW - 668.f - LeadW) / FMath::Max(1, S.Teams.Num()), 174.f, 240.f);
	const float W = FMath::Max(8.f + 2 * Border, FMath::Min(ViewW - 660.f, LeadW + S.Teams.Num() * TeamW + 8.f + 2 * Border));
	const float X0 = (ViewW - W) / 2, Y0 = 8.f, Inner = W - 8.f - 2 * Border;
	struct FBlock { int32 Team; float W, H, Y; };
	TArray<FBlock> B;
	float LeadH = ModeH + (S.Lead.Clock.IsEmpty() ? 0.f : ClockH);
	if (!S.Lead.Threat.IsEmpty()) LeadH += SmallH;
	if (bNextWave) LeadH += 4.f + 2.f + 4.f + 17.55f;
	if (LeadW > 0) B.Add({-1, LeadW, LeadH, 0});
	for (int32 I = 0; I < S.Teams.Num(); I++)
	{
		const FTCScoreTeam& T = S.Teams[I];
		float H = 0;
		for (const FTCScoreSeat& Seat : T.Seats) H += 22.f + ((!Seat.Held.IsEmpty() || !Seat.Net.IsEmpty()) ? SmallH : 0.f);
		if (!T.Role.IsEmpty()) H += SmallH;
		if (T.bBar) H += 10.f;
		// .sc-val is a baseline-aligned flex row. Empty spans collapse, but its 4px margin remains.
		H += 4.f + (T.Num.IsEmpty() ? (T.U0.IsEmpty() && T.U.IsEmpty() ? 0.f : SmallH) : (T.U0.IsEmpty() && T.U.IsEmpty() ? 19.f : ValueH));
		B.Add({I, TeamW, H, 0});
	}
	struct FRow { int32 First, End; float W, H, Y; };
	TArray<FRow> Rows;
	float Y = Y0 + 8.f + Border;
	for (int32 First = 0; First < B.Num(); )
	{
		int32 End = First;
		float Rw = 0, Rh = 0;
		while (End < B.Num() && (End == First || Rw + B[End].W <= Inner + 0.01f))
		{ Rw += B[End].W; Rh = FMath::Max(Rh, B[End].H); End++; }
		Rows.Add({First, End, Rw, Rh, Y});
		Y += Rh + (End < B.Num() ? 8.f : 0.f);
		First = End;
	}
	if (S.bWeather) Y += 8.f + Border + 5.f + ModeH;
	const float H = Y - Y0 + 9.f + Border;
	P.Rect(X0, Y0, W, H, TCCss::PanelSolid, TEXT("scores:panel"));
	P.Frame(X0, Y0, W, H, TCCss::Line, Border);
	for (const FRow& R : Rows)
	{
		float X = X0 + 4.f + Border + (Inner - R.W) / 2;
		for (int32 I = R.First; I < R.End; I++)
		{
			const FBlock& F = B[I];
			if (I > 0) P.Rect(X, R.Y, Border, R.H, TCCss::LineSoft);
			if (F.Team < 0)
			{
				float Ty = R.Y + (R.H - F.H) / 2;
				P.Text(TC_T(S.Lead.Mode), X + F.W / 2, Ty, 13, S.Lead.bModeDanger ? TCCss::RedHi : TCCss::Dim, false, ETCPaintAlign::Centre).Size.Y = ModeH;
				Ty += ModeH;
				if (!S.Lead.Clock.IsEmpty()) { P.Text(TC_T(S.Lead.Clock), X + F.W / 2, Ty, 26, TCCss::Brass, true, ETCPaintAlign::Centre).Size.Y = ClockH; Ty += ClockH; }
				if (bNextWave)
				{
					const FString Label = TC_T(FString(TEXT("Send next wave")));
					const float Bw = Measure(Label, 13, false, 0) + 22.f, Bh = 23.55f;
					const float Bx = X + (F.W - Bw) / 2;
					P.Rect(Bx, Ty + 4.f, Bw, Bh, TCCss::Hex(0x24282b));
					P.Frame(Bx, Ty + 4.f, Bw, Bh, TCCss::LineSoft);
					P.Text(Label, Bx + 11.f, Ty + 7.f, 13, TCCss::Text).Size.Y = 17.55f;
					P.Hit(FBox2D(FVector2D(Bx, Ty + 4), FVector2D(Bx + Bw, Ty + 4 + Bh)), TEXT("scores:nextwave"), Label);
					Ty += 4 + Bh;
				}
				if (!S.Lead.Threat.IsEmpty()) P.Text(TC_T(S.Lead.Threat), X + F.W / 2, Ty, 13, TCCss::Brass, false, ETCPaintAlign::Centre).Size.Y = 17.55f;
			}
			else
			{
				const FTCScoreTeam& T = S.Teams[F.Team];
				const float Tx = X + 12.f + (I > 0 ? Border : 0.f), Content = F.W - 24.f - (I > 0 ? Border : 0.f);
				float Ty = R.Y;
				for (const FTCScoreSeat& Seat : T.Seats)
				{
					P.Rect(Tx, Ty + 5, 12, 12, ATCUnitRenderer::SeatColor(Seat.Slot));
					// Original hud.js MARKS, rasterized by the icon exporter. Transparent geometry carries its slot.
					P.Rect(Tx+18,Ty+2,18,18,FLinearColor(0,0,0,0),*FString::Printf(TEXT("scores:faction:%d"),Seat.Faction));
					const FString You = Seat.bYou ? TC_T(FString(TEXT("you"))) : FString();
					const float YouW = You.IsEmpty() ? 0.f : Measure(You, 13, false, 0) + 6;
					// i18n.js also observes .sc-name text nodes (for example the server's "Horde" becomes "Horda").
					const FString Name = FTCPaint::Ellipsis(TC_T(Seat.Name), Content - 42 - YouW, 15, true, Measure);
					P.Text(Name, Tx + 42, Ty, 15, TCCss::Text, true).Size.Y = 22;
					if (Seat.bYou) P.Text(You, Tx + 48 + Measure(Name, 15, true, 0), Ty, 13, TCCss::Brass).Size.Y = 22;
					Ty += 22;
					if (!Seat.Held.IsEmpty() || !Seat.Net.IsEmpty())
					{
						const FString Net = TC_T(Seat.Net);
						const float NetW = Net.IsEmpty() ? 0 : Measure(Net, 13, false, 0) + 8;
						P.Text(FTCPaint::Ellipsis(TC_T(Seat.Held), Content - 18 - NetW, 13, false, Measure), Tx + 18, Ty, 13, Seat.bHeldDanger ? TCCss::RedHi : TCCss::Dim).Size.Y = 17.55f;
						P.Text(Net, Tx + Content, Ty, 13, Seat.bNetDanger ? TCCss::RedHi : TCCss::Dim, false, ETCPaintAlign::Right).Size.Y = 17.55f;
						Ty += SmallH;
					}
				}
				if (!T.Role.IsEmpty()) { P.Text(TC_T(T.Role), Tx, Ty, 13, TCCss::Dim).Size.Y = SmallH; Ty += SmallH; }
				if (T.bBar)
				{
					P.Rect(Tx, Ty + 6, Content, 4, TCCss::Rgba(226,223,211,0.1f));
					P.Rect(Tx, Ty + 6, Content * T.Fraction, 4, ATCUnitRenderer::SeatColor(T.ColourSlot));
					Ty += 10;
				}
				Ty += 4;
				const FString U0 = TC_T(T.U0), Num = TC_T(T.Num), U = TC_T(T.U);
				const float A = Measure(U0,13,false,0), N = Measure(Num,19,true,0), C = Measure(U,13,false,0);
				const float Total = A + N + C + ((!U0.IsEmpty() && (!Num.IsEmpty() || !U.IsEmpty())) ? 5 : 0) + ((!Num.IsEmpty() && !U.IsEmpty()) ? 5 : 0);
				float Vx = Tx + (Content - Total) / 2;
				const float SmallY = Ty + (Num.IsEmpty() ? 0.f : SmallOffset);
				if (!U0.IsEmpty()) { P.Text(U0,Vx,SmallY,13,T.bU0Danger ? TCCss::RedHi : TCCss::Dim).Size.Y = 17.55f; Vx += A+5; }
				if (!Num.IsEmpty()) { P.Text(Num,Vx,Ty,19,TCCss::Brass,true).Size.Y = 19.f; Vx += N+5; }
				if (!U.IsEmpty()) P.Text(U,Vx,SmallY,13,TCCss::Dim).Size.Y = 17.55f;
			}
			X += F.W;
		}
	}
	if (S.bWeather)
	{
		const float Ty = Y0 + H - 9.f - Border - Border - 5.f - ModeH;
		P.Rect(X0 + 4 + Border, Ty, Inner, Border, TCCss::LineSoft);
		P.Text(FTCPaint::Ellipsis(TC_T(S.Weather), Inner - 24.f, 13, false, Measure), ViewW / 2, Ty + Border + 5, 13, S.bWeatherTurning ? TCCss::Brass : TCCss::Dim, false, ETCPaintAlign::Centre).Size.Y = ModeH;
	}
	P.Panels.Add(FBox2D(FVector2D(X0,Y0),FVector2D(X0+W,Y0+H)));
	return P;
}

float ATCHUD::DrawScoreStrip(UFont* Font, UFont* Big, FTCHudFit::FBottomLayout Sandbox)
{
	using namespace TCHudSlice9;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	const FTCMatchState& S = M->State;
	const FTCScoreStrip Strip = S.Objectives.ScoreStrip(S, &M->UnitTable);
	StripCard = FBox2D(ForceInit);
	if (!Strip.bValid) { DrawBottomLayout(Sandbox,Font); return 8.f; }
	const FTCMeasure Measure = FTCType::IsLoaded() ? FTCType::Measure() : FTCPaint::CanvasMeasure(Font);
	const FTCControlButton NextWave = M->Controls.NextWaveButton();
	const FTCPaint Scores = FTCHudFit::ScoreCard(Strip, Canvas->ClipX, Measure, Strip.Lead.bShown && NextWave.bShown, UiNow);
	// index.html puts #sandbox before #top. Hidden middle controls do not receive clicks through the score card.
	FTCHudFit::BehindPanels(Sandbox,Scores.Panels);
	DrawBottomLayout(Sandbox,Font);
	FTCType::Draw(Scores, Canvas, Font, UiNow);
	for (const FTCPaintItem& I : Scores.Items)
	 if (I.Role.StartsWith(TEXT("scores:faction:"))) DrawHudIcon(TEXT("faction-")+I.Role.RightChop(15),FBox2D(I.Pos,I.Pos+I.Size),FLinearColor::White);
	const FTCPaintItem* Card = Scores.Find(TEXT("scores:panel"));
	const float X0 = Card->Pos.X, Y0 = Card->Pos.Y, H = Card->Size.Y, W = Card->Size.X;
	StripCard = ToScreen(FBox2D(Card->Pos, Card->Pos + Card->Size));
	Panels.Add(StripCard);
	for (const FTCPaintHit& Hit : Scores.Hits)
	{
		FTCUiCommand Cmd(ETCUiAction::MatchControl);
		Cmd.Unit = LexToString(ETCMatchControl::NextWave);
		Buttons.Add({ToScreen(Hit.Rect), Cmd, NextWave.Label, NextWave.bEnabled});
	}
	float Y = Y0 + H + 4.f;
	auto Centred = [&](const FString& L, const FLinearColor& C, bool bEliminated = false)
	{
		if (L.IsEmpty()) return;
		const FTCPaint P = FTCHudFit::NoticeCard(TC_T(L), Canvas->ClipX, Y, C, Measure, UiNow, bEliminated);
		FTCType::Draw(P, Canvas, Font, UiNow);
		for (const FBox2D& B : P.Panels) Panels.Add(ToScreen(B));
		if (const FTCPaintItem* Notice = P.Find(TEXT("notice:panel"))) Y = Notice->Pos.Y + Notice->Size.Y + 4.f;
	};
	// client/endgame.js: the rate, catch-up and notice line, then the out banner; weather-view.js; objectives.js goal
	if (Strip.bLineShown)
	{
		const FTCPaint P = FTCHudFit::MatchNoticeCard(TC_T(Strip.Rate), TC_T(Strip.Bonus), TC_T(Strip.Notice), Canvas->ClipX, Y, Measure, UiNow);
		FTCType::Draw(P, Canvas, Font, UiNow);
		for (const FBox2D& B : P.Panels) Panels.Add(ToScreen(B));
		if (const FTCPaintItem* Notice = P.Find(TEXT("notice:panel"))) Y = Notice->Pos.Y + Notice->Size.Y + 4.f;
	}
	Centred(Strip.OutBanner, TextC, true);
	// Weather is the final row inside #scores.
	// round 15: hud.js drawBoss, #boss in #top under the strip (Client/TCBossBar.h): the Kaiju's health while one lives
	M->BossBar.Snapshot(S, FPlatformTime::Seconds());
	// The boss is placed after #buy has been measured, so an open card cannot cover it at large zoom.
	// round 14: objectives.js's goal box and collapse banner as objectives.css draws them (FTCScenarioText), under the
	// strip and its lines
	float Bottom = Y - 4.f;
	if (FTCType::IsLoaded())
	{
		// objectives.css: 600 16px/1.35 for the goal, 500 14px/1.35 for the banner.
		auto TextMeasure = [](float Px, bool Bold) { return [Px, Bold](const FString& T) { return FVector2D(FTCType::Width(T, Px, Bold), Px * 1.35f / 1.15f); }; };
		FTCTextBox Goal, Banner;
		Bottom = M->ScenarioText.Place(Y - 4.f, Canvas->ClipX, Canvas->ClipY, TextMeasure(16.f, true), TextMeasure(14.f, false), Goal, Banner);
		FTCPaint P;
		auto Box = [&P](const FTCTextBox& B, const FTCTextBoxStyle& S, float Px, bool Bold)
		{
			if (!B.bShown) return;
			P.Rect(B.Min.X, B.Min.Y, B.Size.X, B.Size.Y, S.Plate);
			P.Frame(B.Min.X, B.Min.Y, B.Size.X, B.Size.Y, S.Border);
			for (int32 I = 0; I < B.Lines.Num(); I++) P.Text(B.Lines[I], B.LineX[I], B.FirstLineY + I * B.LineHeight, Px, S.Text, Bold).Size.Y = B.LineHeight;
		};
		Box(Goal, FTCTextBoxStyle::Goal(), 16.f, true);
		Box(Banner, FTCTextBoxStyle::Banner(), 14.f, false);
		FTCType::Draw(P, Canvas, Font, UiNow);
	}
	else Bottom = M->ScenarioText.Draw(Canvas, Big, Font, Y - 4.f);
	// round 15: client/stats.js, F2's overlay (Client/TCKeys.h). Round 16: drawn by DrawStatsBox after the match
	// controls (the top-left box goes under them, #util), from what the strip leaves here: top centre under #top
	StatsTopTc = FMath::Max(Y, Bottom + 4.f) - 4.f;
	TopStackBottomPx = StatsTopTc * UiNow;
	StatsStrip = FBox2D(FVector2D(X0, Y0), FVector2D(X0 + W, Y0 + H));
	return FMath::Max(Y, Bottom + 4.f);
}

void ATCHUD::DrawCapturePoints(UFont* Font)
{
	using namespace TCHudSlice9;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !M->Terrain) return;
	const ATCTerrain& T = *M->Terrain;
	const FTCObjectives& O = M->State.Objectives;
	const TArray<FTCCapturePoint>& Points = O.Points();
	for (int32 I = 0; I < Points.Num(); I++)
	{
		const FTCCapturePoint& P = Points[I];
		const FVector Centre = Project(ATCTerrain::ToWorld(P.Centre.X, P.Centre.Y, T.HeightAt(P.Centre.X, P.Centre.Y)));
		if (Centre.Z <= 0 || Centre.X < -Canvas->ClipX || Centre.X > 2 * Canvas->ClipX || Centre.Y < -Canvas->ClipY || Centre.Y > 2 * Canvas->ClipY) continue;
		// round 17 (markers): markers.js capturePoint's rings as overlay.js draws them (TCMarkersHud): the ring 0.42 m
		// wide over a faint wash (chalk while neutral, the owner's colour once held), the progress track and arc, the
		// contested and flip brass rings
		{
			FTCMarkerPainter Marks = TCMarkersHud::Painter(Canvas, *M);
			TCMarkersHud::DrawCapturePoint(Marks, P, O.RingFrame(I));
		}
		// the flag: a pole and a 2.2 x 1.4 m cloth at 7.2 m in the owner's colour (markers.js flag)
		const float G = T.HeightAt(P.Centre.X, P.Centre.Y);
		const FVector Foot = Project(ATCTerrain::ToWorld(P.Centre.X, P.Centre.Y, G)), Top = Project(ATCTerrain::ToWorld(P.Centre.X, P.Centre.Y, G + 7.9f));
		if (Foot.Z > 0 && Top.Z > 0)
		{
			DrawLine(Foot.X, Foot.Y, Top.X, Top.Y, FLinearColor(0.15f, 0.13f, 0.1f), 2.f);
			const FVector C0 = Project(ATCTerrain::ToWorld(P.Centre.X, P.Centre.Y, G + 6.5f)), C1 = Project(ATCTerrain::ToWorld(P.Centre.X + 2.2f, P.Centre.Y, G + 6.5f));
			const FVector C2 = Project(ATCTerrain::ToWorld(P.Centre.X + 2.2f, P.Centre.Y, G + 7.9f));
			if (C0.Z > 0 && C1.Z > 0 && C2.Z > 0)
			{
				const FVector2D Q[4] = { FVector2D(C0.X, C0.Y), FVector2D(C1.X, C1.Y), FVector2D(C2.X, C2.Y), FVector2D(Top.X, Top.Y) };
				FillQuad(Q, P.FlagSlot >= 0 ? ATCUnitRenderer::SeatColor(P.FlagSlot) : FLinearColor(NeutralFlag));
			}
			FTCMarkerPainter Labels = TCMarkersHud::Painter(Canvas, *M);
			TCMarkersHud::DrawCaptureLabels(Labels, P, *M, Font);
		}
	}
}

void ATCHUD::DrawPlans()
{
	using namespace TCHudSlice9;
	// client/markers.js planLayer draw (round 16: TCOrders::Markers, checked against the pinned markers.js): the army
	// rally flag, then for each selected unit its production rally (a dashed leg and a flag), its route in the plan's
	// colour with an arrowhead and a ring at the goal, its waiting orders (Shift) as dashed legs with arrowheads and
	// rings, and the line to what it is shooting at with a ring around that. Garrisoned units draw theirs from the house.
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !M->Terrain || !PC->PlayerCameraManager) return;
	const FVector2D* Army = M->bArmyRally ? &M->ArmyRally : nullptr;
	if (!PC->Selected.Num() && !Army) return;
	// round 17 (markers): makeUnit's weapon range rings for a small selection (rangeRings), dashed, over everything
	{
		FTCMarkerPainter Marks = TCMarkersHud::Painter(Canvas, *M);
		TCMarkersHud::DrawRangeRings(Marks, *M, PC->Selected);
	}
	const ATCTerrain& T = *M->Terrain;
	const TArray<int32> Sel = PC->Selected.Array();
	const TMap<int32, FTCMarkUnit> Units = TCOrders::MarkUnits(M->State, M->UnitTable, Sel,
		[M](int32 Id, FVector2D& Out) { return M->Units && M->Units->DrawnAt(Id, Out); },
		[M](int32 Id, FVector2D& Out) { const FTCProductionQueue* Q = M->Recruit.QueueOf(Id); if (!Q || Q->Rally.X < 0) return false; Out = Q->Rally; return true; });
	const FVector Right = FRotationMatrix(PC->PlayerCameraManager->GetCameraRotation()).GetUnitAxis(EAxis::Y);
	auto Ground = [&T](FVector2D P) { return ATCTerrain::ToWorld(P.X, P.Y, T.HeightAt(P.X, P.Y) + 0.12f); };
	// a ribbon's width in pixels where it lies (markers.js: w metres, never under 2 x 1.1 px)
	auto Pixels = [&](FVector2D At, double Width)
	{
		const FVector C = Project(Ground(At)), E = Project(Ground(At) + Right * 100.f);
		const float PxPerM = C.Z > 0 && E.Z > 0 ? (float)FVector2D::Distance(FVector2D(C.X, C.Y), FVector2D(E.X, E.Y)) : 0.f;
		return FMath::Max(2.2f, (float)Width * PxPerM);
	};
	for (const FTCMarkPrim& P : TCOrders::Markers(Sel, Units, Army))
	{
		const FLinearColor C = WithAlpha(FLinearColor(FColor((P.Hex >> 16) & 255, (P.Hex >> 8) & 255, P.Hex & 255)), P.Alpha);
		if (P.Op == FTCMarkPrim::EOp::Line && P.Points.Num() >= 2)
		{
			if (P.bDashed) GroundDashes(T, P.Points[0], P.Points.Last(), C, (float)P.Width, 2.2f, 1.3f); // ORDER_DASH
			else GroundLine(T, P.Points, false, C, Pixels(P.Points.Last(), P.Width), 0.12f);
		}
		else if (P.Op == FTCMarkPrim::EOp::Ring) GroundRing(T, P.At, (float)P.Radius, C, Pixels(P.At, P.Width));
		else if (P.Op == FTCMarkPrim::EOp::Arrow)
		{
			// overlay.js arrow: two triangles split along the axis, tip to the base corners
			const double L = P.Dir.Size();
			if (L <= 0) continue;
			const FVector2D D = P.Dir / L, N(-D.Y, D.X), Base = P.At - D * P.Len;
			const FVector Tip = Project(Ground(P.At)), A = Project(Ground(Base + N * P.Half)), Mid = Project(Ground(Base)), B = Project(Ground(Base - N * P.Half));
			if (Tip.Z <= 0 || A.Z <= 0 || Mid.Z <= 0 || B.Z <= 0) continue;
			const FVector2D Q[4] = { FVector2D(Tip.X, Tip.Y), FVector2D(A.X, A.Y), FVector2D(Mid.X, Mid.Y), FVector2D(B.X, B.Y) };
			FillQuad(Q, C);
		}
	}
}

void ATCHUD::DrawHealthBars()
{
	using namespace TCHudSlice9;
	// client/main.js: a bar over a unit only when it is selected, hovered, hurt or suppressed, and always over a
	// garrisoned squad: its roof bar, 7.5 m over the ground at the house (round 16, TCOrders::RoofBarHeight), is what
	// selects it; the health in the owner's colour, orange under half, red under a quarter; the suppression bar under
	// it, red from 90
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !M->Terrain || !PC->PlayerCameraManager) return;
	TArray<TCMarkersHud::FUnitBillboard> Billboards;
	const FVector Right = FRotationMatrix(PC->PlayerCameraManager->GetCameraRotation()).GetUnitAxis(EAxis::Y);
	for (const auto& Pair : M->State.Units)
	{
		const FTCUnit& U = Pair.Value;
		const FTCUnitDef* D = M->UnitTable.Find(U.Type);
		const bool bGarr = (U.Flags & TCFlags::Garrisoned) != 0;
		if (!D || D->MaxHp() <= 0 || (!ATCUnitRenderer::IsDrawn(U) && !bGarr)) continue;
		const float Frac = FMath::Max(0.f, (float)U.Hp / D->MaxHp());
		const bool bSel = PC->Selected.Contains(U.Id);
		// round 17 (markers): the veterancy stars sit in the bar group and show whether or not the bar does
		const bool bBar = TCMarkersCore::HealthBarShown(Frac, U.Supp, bSel, PC->HoverEnemy == U.Id, bGarr);
		// The reference role badge remains visible when a healthy unit has no bar.
		FVector2D At(U.X, U.Z);
		if (M->Units) M->Units->DrawnAt(U.Id, At);
		const FVector W = ATCTerrain::ToWorld(At.X, At.Y, M->Terrain->HeightAt(At.X, At.Y) + (bGarr ? (float)TCOrders::RoofBarHeight : BarY(U.Type, D)));
		const FVector C = Project(W), E = Project(W + Right * 120.f);
		if (C.Z <= 0 || E.Z <= 0 || C.X < 0 || C.Y < 0 || C.X > Canvas->ClipX || C.Y > Canvas->ClipY) continue;
		const float BadgePxPerM = (float)FVector2D::Distance(FVector2D(C.X, C.Y), FVector2D(E.X, E.Y)) / 1.2f;
		const float PxPerM = BadgePxPerM; // main.js bars and stars keep their world dimensions
		const float BgW = 2.4f * PxPerM, BgH = 0.42f * PxPerM;
		// round 17: not under the score strip's card (last frame's, screen pixels); the stars alone neither
		if (bShowHUD && StripCard.bIsValid && StripCard.Intersect(FBox2D(FVector2D(C.X - BgW / 2, C.Y - BgH / 2), FVector2D(C.X + BgW / 2, C.Y + BgH / 2)))) continue;
		auto& Billboard = Billboards.AddDefaulted_GetRef();
		Billboard.Type = U.Type; Billboard.Owner = U.Owner; Billboard.Centre = FVector2D(C.X, C.Y);
		Billboard.PxPerMetre = PxPerM; Billboard.World = W; Billboard.Health = Frac;
		Billboard.Suppression = U.Supp; Billboard.Cover = U.Cover; Billboard.Stars = U.Stars;
		Billboard.bHealthBar = bBar; Billboard.bGarrisoned = bGarr;
	}
	TCMarkersHud::DrawUnitBillboards(Canvas, Billboards);
}

namespace
{
    // Other session glue may be integrated after this scope. Use each available banner without depending on it.
    template<class T> TArray<FTCRoomBanner> AlertRoomBanners(const T& Mode, double Now)
    {
        TArray<FTCRoomBanner> Banners;
        if constexpr (requires { Mode.ConnectionNotice(); })
        {
            const auto Notice=Mode.ConnectionNotice();
            const FString Connection=Notice.Text(Now);
            if (!Connection.IsEmpty()) Banners.Add({Connection,true});
            if (!Notice.RoomLine().IsEmpty()) Banners.Add({Notice.RoomLine(),true});
        }
        if constexpr (requires { Mode.Controls.TerminalNotice; })
            if (!Mode.Controls.TerminalNotice.IsEmpty()) Banners.Add({Mode.Controls.TerminalNotice,true});
        if (Mode.State.bPaused && !Mode.State.PauseText.IsEmpty()) Banners.Add({Mode.State.PauseText,false});
        return Banners;
    }
}

namespace
{
    double AlertPresentationTime(const FTCAlerts& Feed)
    {
        // Verification uses the same alerts-only clock for the cards and minimap pulses.
        static const double Age=[]
        {
            double Value=-1;
            if (FParse::Value(FCommandLine::Get(),TEXT("TCHudAlertAge="),Value))
                UE_LOG(LogTC,Log,TEXT("hud: verification holds the latest loss presentation at age %.3f seconds"),Value);
            return Value;
        }();
        if (Age>=0) for (const FTCAlertLine& L : Feed.Lines()) if (L.IsLoss()) return L.Born+Age;
        return FPlatformTime::Seconds();
    }
}

void ATCHUD::DrawAlertFeed(UFont* Font)
{
	// round 15: client/alerts.css and alerts.js as FTCAlerts::Layout draws them (Client/TCAlerts.h): the loss column,
	// the routine lines, the Alert history box over the minimap; each line and button is a hit for the controller
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !Canvas) return;
	FTCAlertViewInput In;
	In.Now = FPlatformTime::Seconds(); // the alert clock (alerts.js now(); ATCGameMode::OnCombatSnapshot)
    // Opt-in presentation fixture. It changes only this HUD feed, never the match or the received envelopes.
    int32 FixtureTick=0;
    if (!bLossFixtureDrawn && FParse::Value(FCommandLine::Get(),TEXT("TCHudLossFixture="),FixtureTick) && FixtureTick>0 && M->State.Tick>=FixtureTick)
    {
        bLossFixtureDrawn=true;
        for (const TCHAR* Type : {TEXT("rifle"),TEXT("tank"),TEXT("mortar"),TEXT("medic"),TEXT("mg"),TEXT("attacker"),TEXT("flamer"),TEXT("medium")})
        {
            const FString Text=FTCAlerts::UnitName(M->State,&M->UnitTable,Type,M->State.You)+TEXT(" lost");
            M->AlertFeed.Push(TEXT("unitLost"),Text,true,FVector2D(122,117),In.Now);
        }
        UE_LOG(LogTC,Log,TEXT("hud: synthetic presentation fixture, eight simultaneous losses at tick %d, history %d"),M->State.Tick,M->AlertFeed.History().Entries().Num());
    }
    In.Now=AlertPresentationTime(M->AlertFeed);
	In.Right = Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Max.X) : Canvas->ClipX - 8.f;
	In.MinimapTop = Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Min.Y) : Canvas->ClipY - 200.f;
	In.ViewW = Canvas->ClipX; In.ViewH = Canvas->ClipY; In.UiScale = UiNow;
	// Retained collision metadata is diagnostic only. The source alert column keeps its bottom/right anchors.
	for (const FBox2D& B : Panels) In.Avoid.Add(FBox2D(B.Min/UiNow,B.Max/UiNow));
	if (Minimap.Rect().bIsValid) In.Avoid.Add(FBox2D(Minimap.Rect().Min/UiNow-FVector2D(5,5),Minimap.Rect().Max/UiNow+FVector2D(5,5)));
	if (RecruitBox.bIsValid) In.Avoid.Add(FBox2D(RecruitBox.Min/UiNow,RecruitBox.Max/UiNow));
    if (CornerBox.bIsValid) In.Avoid.Add(FBox2D(CornerBox.Min/UiNow,CornerBox.Max/UiNow));
    // These banners paint after the HUD at physical scale 1. Reserve their actual measured boxes first.
    const TArray<FTCRoomBanner> Banners=AlertRoomBanners(*M,FPlatformTime::Seconds());
    const float BannerTop=TopStackBottomPx>=0?FMath::CeilToFloat(TopStackBottomPx+8.f):16.f;
    const FTCPaint BannerPaint=FTCRoomBanners::Layout(Banners,BannerTop,Canvas->ClipX*UiNow,FTCPaint::CanvasMeasure(Font));
    for (const FTCPaintItem& I : BannerPaint.Items) if (I.Kind==ETCPaintKind::Rect && I.Role.StartsWith(TEXT("banner:"))) In.Avoid.Add(FBox2D(I.Pos/UiNow,(I.Pos+I.Size)/UiNow));
	float Mx = 0, My = 0;
	if (APlayerController* PC = GetOwningPlayerController(); PC && PC->GetMousePosition(Mx, My)) { In.bMouse = true; In.Mouse = FVector2D(ToUi(Mx), ToUi(My)); }
	M->AlertFeed.Frame(In.Now);
	const FTCPaint P = M->AlertFeed.Layout(In, (FTCType::IsLoaded() ? FTCType::Measure() : FTCPaint::CanvasMeasure(Font)), FTCType::IsLoaded() ? FTCType::HeavyMeasure() : FTCMeasure());
	if (FixtureTick>0 && bLossFixtureDrawn)
	{
	 static int32 LastCards=-1, LastRoutine=-1;
	 const int32 Cards=P.TextsOf(TEXT("alert:loss-heading")).Num();
     const int32 Routine=P.TextsOf(TEXT("alert:text")).Num()+P.TextsOf(TEXT("alert:text-newest")).Num();
	 if (Cards!=LastCards || Routine!=LastRoutine)
	 {
	  LastCards=Cards; LastRoutine=Routine;
	  UE_LOG(LogTC,Log,TEXT("hud: loss fixture presentation: %d cards, lines [%s], history %d, occupied %d"),Cards,
	   *FString::Join(P.TextsOf(TEXT("alert:loss-detail")),TEXT(" | ")),M->AlertFeed.History().Entries().Num(),In.Avoid.Num());
      for (const FTCAlertLine& L : M->AlertFeed.Lines())
          UE_LOG(LogTC,Log,TEXT("hud: held alert %s age %.3f: %s"),*L.Alert.Kind,In.Now-L.Born,*L.Alert.Text);
      for (const FTCPaintItem& I : P.Items)
          if (I.Role==TEXT("alert:loss") || I.Role==TEXT("alert:loss-edge") || I.Role==TEXT("alert:line") || I.Role==TEXT("alert:text-newest") || I.Role==TEXT("alert:history"))
              UE_LOG(LogTC,Log,TEXT("hud: alert box %s (%.3f,%.3f) (%.3f,%.3f) alpha %.3f text %s"),*I.Role,I.Pos.X,I.Pos.Y,I.Size.X,I.Size.Y,I.Colour.A,*I.Text);
	 }
	}
    // Keep source alpha and anchors. Suppress only economy glyph fragments beneath near-solid cards;
    // otherwise Canvas' independent text batches can leave readable cost/call-in text in the foreground.
    DeferredEconomy.Paint.TextOcclusions.Reset();
    for (const FTCPaintItem& Item : P.Items)
        if ((Item.Role==TEXT("alert:loss") && Item.Colour.A>=.96f) ||
            (Item.Role==TEXT("alert:history") && Item.Colour.A>=.93f) ||
            (Item.Role==TEXT("alert:line") && Item.Colour.A>=.9f))
            DeferredEconomy.Paint.TextOcclusions.Add(FBox2D(Item.Pos,Item.Pos+Item.Size));
    DrawBottomLayout(DeferredEconomy,Font);
    DeferredEconomy=FTCHudFit::FBottomLayout();
	FTCType::Draw(P, Canvas, Font, UiNow);
	// round 17: the history box (alert-history.js #alertHistory) for the sandbox panel's place next frame
	AlertHistoryBox = FBox2D(ForceInit);
	if (const FTCPaintItem* H = P.Find(TEXT("alert:history"))) AlertHistoryBox = FBox2D(H->Pos, H->Pos + H->Size);
	for (const FTCPaintHit& H : P.Hits)
    {
        FTCUiCommand C(ETCUiAction::Alert); C.Unit=H.Tag; Buttons.Add({ToScreen(H.Rect),C,H.Label,true});
        if (H.Tag.StartsWith(TEXT("alert:line:"))) OfferTooltip(TEXT("Space or click: look there"),ToScreen(H.Rect));
        else if (H.Tag.StartsWith(TEXT("alert:entry:")))
        {
            const int32 Id=FCString::Atoi(*H.Tag.RightChop(12));
            const FTCAlertHistoryEntry* Entry=M->AlertFeed.History().Entries().FindByPredicate([Id](const FTCAlertHistoryEntry& E) { return E.Id==Id; });
            if (Entry) OfferTooltip(Entry->bHasAt?FString::Printf(TEXT("Recorded location: %.1f, %.1f"),Entry->At.X,Entry->At.Y):FString(TEXT("No recorded location")),ToScreen(H.Rect));
        }
    }
	for (const FBox2D& B : P.Panels) Panels.Add(ToScreen(B));
}

void ATCHUD::DrawEpilogue(UFont* Font, UFont* Big)
{
	using namespace TCHudSlice9;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !M->State.bHasMatch) return;
	const FTCEpilogue& E = M->State.Objectives.Epilogue();
	if (!E.bActive) return;
	// client/epilogue.js stamp: centred at 30% of the height, the word in brass (Victory), red (Defeat) or the text
	// colour (Draw), the reason under it, fading in over 0.5 s
	const float Fade = (float)FMath::Clamp(E.Elapsed / 0.5, 0.0, 1.0);
	const FLinearColor WordC = E.Outcome == ETCOutcome::Victory ? Brass : E.Outcome == ETCOutcome::Defeat ? Red : TextC;
	float WW = 0, WH = 0, RW = 0, RH = 0;
	LabelSize(E.Word, WW, WH, Big, 2.f);
	LabelSize(E.Why, RW, RH, Font, 1.f);
	const float W = FMath::Max3(320.f, WW + 64.f, RW + 64.f), H = WH + RH + 34.f;
	const float X = (Canvas->ClipX - W) / 2, Y = Canvas->ClipY * 0.3f - H / 2;
	DrawRect(WithAlpha(Plate, 0.92f * Fade), X, Y, W, H);
	const FLinearColor Edge = WithAlpha(E.Outcome == ETCOutcome::Defeat ? FLinearColor(FColor(200, 72, 59)) : Hairline, Fade);
	DrawLine(X, Y, X + W, Y, Edge); DrawLine(X + W, Y, X + W, Y + H, Edge); DrawLine(X + W, Y + H, X, Y + H, Edge); DrawLine(X, Y + H, X, Y, Edge);
	DrawLabel(E.Word, WithAlpha(WordC, Fade), X + (W - WW) / 2, Y + 14.f, Big, 2.f);
	DrawLabel(E.Why, WithAlpha(TextC, Fade), X + (W - RW) / 2, Y + 22.f + WH, Font, 1.f);
}

void ATCHUD::DrawControlsSheet(UFont* Font, UFont* Big)
{
	using namespace TCHudSlice9;
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!PC || !PC->bControlsSheetOpen) return;
	// client/index.html #controlsSheet: the four sections, two columns; F1 or Esc closes it
	const TArray<FTCSheetSection>& Sheet = TCKeys::ControlsSheet();
	const float W = FMath::Min(1180.f, Canvas->ClipX - 40.f), ColW = W / 2;
	int32 LeftRows = 0, RightRows = 0;
	for (int32 I = 0; I < Sheet.Num(); I++) (I < 2 ? LeftRows : RightRows) += Sheet[I].Rows.Num() + 2;
	const float H = FMath::Min(Canvas->ClipY - 40.f, (FMath::Max(LeftRows, RightRows) + 2) * 14.f + 2 * PadPx + 20.f);
	const float X0 = (Canvas->ClipX - W) / 2, Y0 = (Canvas->ClipY - H) / 2;
	AddPanel(X0, Y0, W, H, WithAlpha(Plate, 0.96f));
	DrawLabel(TEXT("Controls  (F1 or Esc closes)"), Brass, X0 + PadPx, Y0 + PadPx, Big, 1.f);
	float Yc[2] = { Y0 + PadPx + 26.f, Y0 + PadPx + 26.f };
	for (int32 I = 0; I < Sheet.Num(); I++)
	{
		const int32 Col = I < 2 ? 0 : 1;
		const float X = X0 + PadPx + Col * ColW;
		float& Y = Yc[Col];
		DrawLabel(Sheet[I].Title, Brass, X, Y, Font, 1.f); Y += 15.f;
		for (const FTCSheetRow& R : Sheet[I].Rows)
		{
			if (Y > Y0 + H - 16.f) break;
			DrawLabel(R.Keys, TextC, X, Y, Font, 1.f);
			DrawLabel(R.Text, DimC, X + 150.f, Y, Font, 1.f);
			Y += 14.f;
		}
		Y += 8.f;
	}
}

float ATCHUD::LobbyWidthBesideReport(UFont* Font) const
{
	const ATCGameMode* M = GetWorld() ? GetWorld()->GetAuthGameMode<ATCGameMode>() : nullptr;
	if (!M || !Canvas || !Font) return LobbyPanelW;
	const FTCEndReportInput In = FTCEndReport::FromState(M->State);
	if (!In.Report.bShown) return LobbyPanelW;
	const FTCMeasure Measure = (FTCType::IsLoaded() ? FTCType::Measure() : FTCPaint::CanvasMeasure(Font));
	// index.html #overlay .card: min(920px, 100%); the table's natural width within it, never under its min-content
	const float Want = FMath::Max(FTCEndReport::MinWidth(In, Measure), FMath::Min(TCEndReportRules::CardWidth, FTCEndReport::NaturalWidth(In, Measure)));
	const float Room = Canvas->ClipX - 8.f - 8.f - 8.f; // the lobby's left margin, the gap, the right margin
	return FMath::Clamp(Room - Want, LobbyPanelMinW, LobbyPanelW);
}

void ATCHUD::DrawMatchReport(UFont* Font, float Top, float LobbyBottom, float X0)
{
	// round 15: client/main.js renderLobby's result line and client/report.js's card (Client/TCEndReport.h), beside the
	// lobby panel: hidden without a result; the report hidden for a host-ended match; its summary opens and closes it
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !Canvas) return;
	const FTCEndReportInput In = FTCEndReport::FromState(M->State);
	M->EndReport.Sync(In);
	const FTCMeasure Measure = (FTCType::IsLoaded() ? FTCType::Measure() : FTCPaint::CanvasMeasure(Font));
	float W = FMath::Min(TCEndReportRules::CardWidth, Canvas->ClipX - X0 - 8.f);
	// round 15 (lead): too narrow beside the lobby panel: under it, across the window (the browser's report sits in the
	// lobby card's own column). Round 16: too narrow means narrower than the story table's min-content (its headers
	// broken at their words, LobbyWidthBesideReport has already narrowed the lobby panel for it), so no column is cut
	const float Need = FMath::Max(480.f, FTCEndReport::MinWidth(In, Measure));
	if (W < Need && LobbyBottom > 0.f) { X0 = 8.f; Top = LobbyBottom + 8.f; W = FMath::Min(TCEndReportRules::CardWidth, Canvas->ClipX - 16.f); }
	if (W < 300.f || (!In.bResult && !In.Report.bShown)) return;
	float Height = 0.f;
	const FTCPaint P = M->EndReport.Layout(In, X0, Top, W, Measure, &Height);
	if (Height > 0.f && (ReportLogged.X != X0 || ReportLogged.Y != W))
	{
		ReportLogged = FVector2D(X0, W);
		UE_LOG(LogTC, Log, TEXT("hud: match report at x %.0f, %.0f px wide (table min-content %.0f, natural %.0f px), %.0f px tall, window %.0f x %.0f, %s"),
			X0, W, FTCEndReport::MinWidth(In, Measure), FTCEndReport::NaturalWidth(In, Measure), Height, Canvas->ClipX, Canvas->ClipY, X0 > 8.f ? TEXT("beside the lobby") : TEXT("under the lobby"));
	}
	FTCType::Draw(P, Canvas, Font, UiNow);
	for (const FTCPaintHit& H : P.Hits) { FTCUiCommand C(ETCUiAction::Alert); C.Unit = H.Tag; Buttons.Add({ ToScreen(H.Rect), C, H.Label, true }); }
	for (const FBox2D& B : P.Panels) Panels.Add(ToScreen(B));
}

void ATCHUD::DrawMinimapPoints()
{
	using namespace TCHudSlice9;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !Minimap.Rect().bIsValid || !Canvas) return;
	const FTCMatchState& S = M->State;
	FVector2D Px, Edge;
	const float Cs=Minimap.CanvasScale();

	for (const FTCCapturePoint& P : S.Objectives.Points())
	{
		if (!Minimap.WorldToPixel(P.Centre, Px) || !Minimap.WorldToPixel(P.Centre + FVector2D(TCObjectiveRules::PointRadius, 0), Edge)) continue;
		const float Rad = FVector2D::Distance(Px, Edge);
		// a disc in the owner's colour at 60% (neutral: light grey at 40%), outlined in black
		const FLinearColor Fill = P.Owner >= 0 ? WithAlpha(ATCUnitRenderer::SeatColor(P.Owner), 0.6f) : FLinearColor(FColor(0xdd, 0xdd, 0xdd, 0x66));
		constexpr int32 Segs = 16;
		for (int32 K = 0; K < Segs; K++)
		{
			const double A0 = 2 * PI * K / Segs, A1 = 2 * PI * (K + 1) / Segs;
			const FVector2D B0 = Px + FVector2D(FMath::Cos(A0), FMath::Sin(A0)) * Rad, B1 = Px + FVector2D(FMath::Cos(A1), FMath::Sin(A1)) * Rad;
			FCanvasTriangleItem Tri(Px, B0, B1, GWhiteTexture);
			Tri.SetColor(Fill);
			Tri.BlendMode = SE_BLEND_Translucent;
			Canvas->DrawItem(Tri);
			DrawLine(B0.X, B0.Y, B1.X, B1.Y, FLinearColor::Black, Cs);
		}
		// cut off from its HQ: a red cross through it
		if (P.bCut)
		{
			const float R = Rad * 0.7f;
			const FLinearColor C(FColor(0xd0, 0x36, 0x2c));
			DrawLine(Px.X - R, Px.Y - R, Px.X + R, Px.Y + R, C, 3*Cs);
			DrawLine(Px.X + R, Px.Y - R, Px.X - R, Px.Y + R, C, 3*Cs);
		}
		// locked for this side: dashed to the point it needs
		FVector2D To;
		if (P.MinimapLink() && Minimap.WorldToPixel(P.LinkTo, To))
		{
			const double L = FVector2D::Distance(Px, To);
			const FVector2D D = L > 0 ? (To - Px) / L : FVector2D::ZeroVector;
			for (double T = 0; T < L; T += 10.0*Cs)
			{
				const FVector2D A = Px + D * T, B = Px + D * FMath::Min(L, T + 6.0*Cs);
				DrawLine(A.X, A.Y, B.X, B.Y, Brass, 2*Cs);
			}
		}
	}
	// the start's spawns as squares in each seat's colour (World sends none for another team's)
	for (int32 I = 0; I < S.Spawns.Num(); I++)
		if (S.Spawns[I].IsSet() && Minimap.WorldToPixel(S.Spawns[I].GetValue(), Px))
		{
			FVector2D Q[4]; const FVector2D At=S.Spawns[I].GetValue();
			const FVector2D D[4]={{-5,-5},{5,-5},{5,5},{-5,5}};
			for (int32 K=0; K<4; K++) Minimap.WorldToPixel(At+D[K],Q[K]);
			FillQuad(Q,ATCUnitRenderer::SeatColor(I));
			for (int32 K=0; K<4; K++) DrawLine(Q[K].X,Q[K].Y,Q[(K+1)%4].X,Q[(K+1)%4].Y,FLinearColor::Black,Cs);
		}}

void ATCHUD::DrawMinimapAlertRings()
{
 ATCGameMode* M=GetWorld()->GetAuthGameMode<ATCGameMode>();
 if (!M || !Minimap.Rect().bIsValid || !Canvas) return;
	// round 15: client/alerts.js drawPings: each alert line's mark, a spreading ring and a steady one, dark under the
	// kind's colour (4 px, then 2 px)
	const FLinearColor Ink(FColor(0x15, 0x16, 0x0f));
	for (const FTCAlertRing& R : M->AlertFeed.MinimapRings(AlertPresentationTime(M->AlertFeed)))
	{
        FLinearColor Under=Ink, Over=R.Colour; Under.A=Over.A=R.Alpha;
        Minimap.DrawRing(Canvas,R.At,R.RadiusPx,4,Under);
        Minimap.DrawRing(Canvas,R.At,R.RadiusPx,2,Over);
	}
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHudReferenceLayoutTest, "ThreeCrossroads.HUD.ReferenceLayout", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHudReferenceLayoutTest::RunTest(const FString& Parameters)
{
	const TArray<FVector2D> Dashes = FTCHudFit::QueueDashSpans(10);
	TestEqual(TEXT("10m queued leg, ORDER_DASH period 2.2m"), Dashes.Num(), 5);
	if (Dashes.Num() == 5)
	{
		TestTrue(TEXT("the final 1.3m of the goal is lit"), Dashes[0].Equals(FVector2D(8.7, 10), 1e-6));
		TestTrue(TEXT("0.9m gap, measured back from the goal"), Dashes[1].Equals(FVector2D(6.5, 7.8), 1e-6));
		TestTrue(TEXT("the starting dash clips at the leg's origin"), Dashes.Last().Equals(FVector2D(0, 1.2), 1e-6));
	}
	TestEqual(TEXT("no dashed geometry for an empty leg"), FTCHudFit::QueueDashSpans(0).Num(), 0);
	TestEqual(TEXT("invalid period cannot loop"), FTCHudFit::QueueDashSpans(10, 0, 1.3).Num(), 0);
	TestEqual(TEXT("dash fades to zero at the goal"), FTCHudFit::QueueDashAlpha(0, .1), 0.0);
	TestEqual(TEXT("dash first half pixel"), FTCHudFit::QueueDashAlpha(.05, .1), .5, 1e-6);
	TestEqual(TEXT("dash lit core"), FTCHudFit::QueueDashAlpha(.6, .1), 1.0);
	TestEqual(TEXT("dash final half pixel"), FTCHudFit::QueueDashAlpha(1.25, .1), .5, 1e-6);
	TestEqual(TEXT("dash gap"), FTCHudFit::QueueDashAlpha(1.5, .1), 0.0);
	TestEqual(TEXT("GLSL periodic phase"), FTCHudFit::QueueDashAlpha(2.25, .1), .5, 1e-6);
	TestEqual(TEXT("invalid alpha period"), FTCHudFit::QueueDashAlpha(.6, .1, 0), 0.0);
	TestEqual(TEXT("ten screen pixels per metre"), FTCHudFit::QueueDashPixelWidth(FVector2D(10, 0), FVector2D(0, 3)), .1, 1e-9);
	TestEqual(TEXT("perspective skew includes both ribbon axes"), FTCHudFit::QueueDashPixelWidth(FVector2D(10, 5), FVector2D(3, 8)), 11.0 / 65.0, 1e-9);
	TestEqual(TEXT("reversing a leg preserves fwidth"), FTCHudFit::QueueDashPixelWidth(FVector2D(-10, -5), FVector2D(-3, -8)), 11.0 / 65.0, 1e-9);
	TestEqual(TEXT("edge-on projection has a finite fallback"), FTCHudFit::QueueDashPixelWidth(FVector2D(10, 0), FVector2D(3, 0)), .001);
	TestEqual(TEXT("initial seated band"), FTCHudFit::InitialCameraBand(720, 1, true), FVector2D(27, 662));
	TestEqual(TEXT("a spectator has no recruit bar"), FTCHudFit::InitialCameraBand(720, 1, false), FVector2D(27, 720));
	TestEqual(TEXT("150% start band uses snapped borders"), FTCHudFit::InitialCameraBand(720, 1.5f, false), FVector2D(39.5, 720));
	const FTCMeasure Measure = FTCType::IsLoaded() ? FTCType::Measure() : FTCPaint::ApproxMeasure();
	// Browser alerts.css, with the same two diagnostic losses as the paired captures.
	if (FTCType::IsLoaded())
	{
		FTCLocale::FScopedLanguage Lang(ETCLanguage::English);
		FTCAlerts Feed;
		Feed.Push(TEXT("unitLost"), TEXT("Anti-Tank Rifle Squad destroyed"), true, FVector2D(80, 80), 0);
		Feed.Push(TEXT("unitLost"), TEXT("Self-Propelled Anti-Aircraft Gun destroyed"), true, FVector2D(80, 80), 0);
		for (const float Scale : {1.f, 1.5f})
		{
			FTCAlertViewInput In;
			In.Now = 1; In.UiScale = Scale; In.ViewW = 1280 / Scale; In.ViewH = 720 / Scale;
			In.Right = In.ViewW - 8; In.MinimapTop = In.ViewH - 8 - 184;
			const FTCPaint P = Feed.Layout(In, Measure, FTCType::HeavyMeasure());
			const FTCPaintItem* Loss = P.Find(TEXT("alert:loss")), *History = P.Find(TEXT("alert:history"));
			TestNotNull(TEXT("loss card"), Loss);
			if (Loss)
			{
				TestEqual(TEXT("browser loss column top"), static_cast<float>(Loss->Pos.Y * Scale), Scale == 1 ? 297.640625f : 89.4375f, .03f);
				TestEqual(TEXT("browser loss column left"), static_cast<float>(Loss->Pos.X * Scale), Scale == 1 ? 892.f : 698.f, .03f);
			}
			TestTrue(TEXT("history stays above the scaled minimap frame"), History && FMath::IsNearlyEqual(static_cast<float>((History->Pos.Y + History->Size.Y) * Scale), Scale == 1 ? 515.f : 412.5f, .03f));
		}
	}
	FTCScoreStrip S;
	S.bValid = true; S.Kind = TEXT("horde"); S.bWeather = true; S.Weather = TEXT("Snow: sight -10%, infantry -10%, vehicles -15%");
	S.Lead.bShown = true; S.Lead.Mode = TEXT("Wave 0"); S.Lead.Clock = TEXT("1 left");
	FTCScoreTeam T; T.ColourSlot = 0; T.bBar = true; T.Fraction = 1; T.U0 = TEXT("Bunker"); T.Num = TEXT("100%");
	FTCScoreSeat Seat; Seat.Slot = 0; Seat.Name = TEXT("HudLookHost"); Seat.Held = TEXT("0 held"); T.Seats.Add(Seat); S.Teams.Add(T);
	T = FTCScoreTeam(); Seat = FTCScoreSeat(); Seat.Name = TEXT("Horde"); T.Seats.Add(Seat); T.Role = TEXT("Horde"); S.Teams.Add(T);
	{
		FTCLocale::FScopedLanguage Es(ETCLanguage::Spanish);
		const FTCPaint P = FTCHudFit::ScoreCard(S, 1280, Measure, false);
		int32 Translated = 0;
		for (const FTCPaintItem& I : P.Items) if (I.Kind == ETCPaintKind::Text && I.Text == TEXT("Horda")) Translated++;
		TestEqual(TEXT("the browser observes both the Horde seat name and its role"), Translated, 2);
	}
	// endgame.css keeps the rate in #top, 4 CSS px after #scores. The tied line is ordinary rate text,
	// not a world-label tooltip. Round 19 browser DOM, Conquest/Default, two seats, clear weather.
	for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
	for (float Scale : {1.f,1.5f})
	{
		FTCLocale::FScopedLanguage Lang(Language);
		FTCScoreStrip Conquest; Conquest.Kind=TEXT("conquest"); Conquest.bWeather=true; Conquest.Weather=TEXT("Clear: no effect");
		for (const TCHAR* Name : {TEXT("Alder"),TEXT("Fern")})
		{
			FTCScoreTeam Team; Team.bBar=true; Team.Num=TEXT("0 / 1200"); Team.U=TEXT("VP");
			FTCScoreSeat Player; Player.Name=Name; Player.Held=TEXT("0 pts, 0 held"); Team.Seats.Add(Player); Conquest.Teams.Add(Team);
		}
		const FTCPaint Scores=FTCHudFit::ScoreCard(Conquest,1280/Scale,Measure,false,Scale);
		const FBox2D Card=Scores.Panels[0];
		const FTCPaint Rate=FTCHudFit::MatchNoticeCard(TC_T(FString(TEXT("Scores tied"))),TEXT(""),TEXT(""),1280/Scale,Card.Max.Y+4,Measure,Scale);
		const FBox2D Notice=Rate.Panels[0];
		TestEqual(TEXT("tied rate appears exactly once"),Rate.TextsOf(TEXT("notice:text")),TArray<FString>{TC_T(FString(TEXT("Scores tied")))});
		TestEqual(TEXT("tied rate follows the score stack with the browser gap"),(float)(Notice.Min.Y-Card.Max.Y),4.f,.01f);
		TestEqual(TEXT("tied rate is centred on the viewport"),(float)Notice.GetCenter().X*Scale,640.f,.01f);
		if (Language==ETCLanguage::English)
		{
			TestEqual(TEXT("English tied rate width against browser DOM"),(float)Notice.GetSize().X*Scale,Scale==1?83.65625f:124.46875f,.04f);
			TestEqual(TEXT("tied rate top against browser DOM"),(float)Notice.Min.Y*Scale,Scale==1?135.6875f:322.640625f,.04f);
		}
	}
	// Browser DOM fixture at 1280x720, UI 100% and 150% (round-18/hudlook/browser-en).
	for (const float Scale : {1.f, 1.5f})
	{
		const FTCPaint P = FTCHudFit::ScoreCard(S, 1280.f / Scale, Measure, false, Scale);
		const FTCPaintItem* Card = P.Find(TEXT("scores:panel"));
		TestNotNull(TEXT("score card"), Card);
		if (!Card) continue;
		TestTrue(TEXT("CSS max-width, including border and padding"), FMath::IsNearlyEqual(Card->Size.X * Scale, Scale == 1 ? 620.f : 290.f, .05f));
		TestTrue(TEXT("card stays centred"), FMath::IsNearlyEqual((Card->Pos.X + Card->Size.X / 2) * Scale, 640.f, .05f));
		TestEqual(TEXT("browser flex card height, including empty span collapse and CSS zoom"), static_cast<float>(Card->Size.Y * Scale), Scale == 1 ? 175.234375f : 335.65625f, .03f);
	}
	{
		FTCLocale::FScopedLanguage Lang(ETCLanguage::English);
		const FString Text = TEXT("The last of the wave are marked through the fog");
		const FTCPaint P = FTCHudFit::NoticeCard(Text,1280.f/1.5f,0,TCCss::Text,Measure,1.5f);
		TestTrue(TEXT("a status notice wraps instead of ellipsizing"), P.TextsOf(TEXT("notice:text")).Num() > 1);
		TestEqual(TEXT("all status notice words survive"), FString::Join(P.TextsOf(TEXT("notice:text")),TEXT(" ")), Text);
		if (FTCType::IsLoaded())
		{
			const FTCPaint Wide = FTCHudFit::NoticeCard(Text,1280,0,TCCss::Text,Measure);
			TestEqual(TEXT("natural-width toast stays one line at UI 100%"), Wide.TextsOf(TEXT("notice:text")).Num(), 1);
			TestEqual(TEXT("Barlow toast width against browser DOM"), static_cast<float>(Wide.Find(TEXT("notice:panel"))->Size.X), 269.4375f, .04f);
			TestEqual(TEXT("browser 150% wrapped toast height"), static_cast<float>(P.Find(TEXT("notice:panel"))->Size.Y * 1.5f), 63.625f, .03f);
			for (const float Scale : {1.f, 1.5f})
			{
				const FTCPaint Flex = FTCHudFit::MatchNoticeCard(TEXT("Rate short"), TEXT("Bonus short"), TEXT("Notice short"), 1280 / Scale, 0, Measure, Scale);
				const FTCPaintItem* Card = Flex.Find(TEXT("notice:panel"));
				TestEqual(TEXT("browser match strip flex width"), static_cast<float>(Card->Size.X * Scale), Scale == 1 ? 235.453125f : 290.f, .04f);
				TestEqual(TEXT("notice on its own row, after the rate and bonus"), static_cast<float>(Card->Size.Y * Scale), Scale == 1 ? 45.09375f : 66.625f, .03f);
			}
			const FTCPaint Narrow = FTCHudFit::MatchNoticeCard(TEXT("Team 1 +1 VP/s"), TEXT("Catch-up: team 2 +0.5 VP/s"), TEXT("A rather long notice that should wrap over more than one line"), 1280 / 1.5f, 0, Measure, 1.5f);
			TestEqual(TEXT("narrow browser flex rows wrap as separate spans"), static_cast<float>(Narrow.Find(TEXT("notice:panel"))->Size.Y * 1.5f), 122.25f, .03f);
		}
	}
	return true;
}
#endif
