// ATCHUD's slice 9 drawing (see TCHUD.h): the score strip, capture points, plan lines, health bars, alert feed and
// history, the epilogue's stamp, the Controls sheet and the lobby's match report. Everything is read from FTCMatchState
// (this recipient's own messages) and FTCObjectives, which is built from it; nothing is inferred for what a snapshot
// did not deliver. Canvas drawing only: placeholder presentation in the browser's colours, not its art.
#include "Client/TCHUD.h"
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

void ATCHUD::GroundDashes(const ATCTerrain& T, FVector2D A, FVector2D B, const FLinearColor& Colour, float Thickness, float Dash, float GapM)
{
	const double L = FVector2D::Distance(A, B);
	if (L < 0.05) return;
	const FVector2D D = (B - A) / L;
	// dashes counted from the far end, so they stay put near the goal (markers.js uEnd)
	for (double E = L; E > 0; E -= Dash + GapM)
	{
		const double S = FMath::Max(0.0, E - Dash);
		GroundLine(T, { A + D * S, A + D * E }, false, Colour, Thickness, 0.12f);
	}
}

float ATCHUD::DrawScoreStrip(UFont* Font, UFont* Big)
{
	using namespace TCHudSlice9;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	const FTCMatchState& S = M->State;
	const FTCScoreStrip Strip = S.Objectives.ScoreStrip(S, &M->UnitTable);
	StripCard = FBox2D(ForceInit);
	if (!Strip.bValid) return 8.f;
	auto Width = [&](const FString& L, UFont* F) { float TW = 0, TH = 0; LabelSize(L, TW, TH, F, 1.f); return TW; };
	// client/hud.js drawScores: a lead block (every mode but Conquest) and a block per team; the styles size the lead
	// 150 px (216 in Assault and Horde)
	const float BlockW = 236.f, LeadW = Strip.Lead.bShown ? (Strip.Kind == TEXT("assault") || Strip.Kind == TEXT("horde") ? 216.f : 150.f) : 0.f, Gap = 4.f;
	const int32 N = Strip.Teams.Num();
	float W = LeadW + (LeadW > 0 ? Gap : 0.f) + N * BlockW + FMath::Max(0, N - 1) * Gap;
	// round 16: never reaching the top left corner's Menu panel (#util; the browser's #scores is narrower still: at most
	// var(--vw) - 660px, its blocks wrapping)
	W = FMath::Min(W, CornerRight > 0.f ? FTCHudFit::StripMaxWidth(Canvas->ClipX, CornerRight) : Canvas->ClipX - 16.f);
	const float Bw = N ? (W - LeadW - (LeadW > 0 ? Gap : 0.f) - FMath::Max(0, N - 1) * Gap) / N : 0.f;
	// block heights: per seat two lines, then role, bar and value
	float H = 0.f;
	for (const FTCScoreTeam& T : Strip.Teams)
		H = FMath::Max(H, T.Seats.Num() * 2 * LineH + (T.Role.IsEmpty() ? 0.f : LineH) + (T.bBar ? 10.f : 0.f) + LineH + 2 * PadPx);
	// round 16: hud.js .sc-next, Horde's Send next wave in the lead block under the clock (margin-top 4px), for the host
	// between waves (FTCMatchControls::NextWaveButton; a click is the match control 'NextWave')
	const FTCControlButton NextWave = M->Controls.NextWaveButton();
	const bool bNextWave = Strip.Lead.bShown && NextWave.bShown;
	if (Strip.Lead.bShown) H = FMath::Max(H, 3 * LineH + 8.f + 2 * PadPx + (bNextWave ? LineH + PadPx + 4.f : 0.f));
	const float X0 = (Canvas->ClipX - W) / 2, Y0 = 6.f;
	float X = X0;
	// round 17 (hudui): one card under every block, as the browser's #scores, where each block had its own plate with
	// the world showing through the 4 px between them (round 16's stray line and ticks along the team block's edge);
	// the gaps keep their place and hold the rule. The card is kept in screen pixels so the world's bars and labels
	// under it are left out next frame (DrawHealthBars, DrawCapturePoints): the 0.96 card all but hides them in the
	// browser, where this canvas showed them as faint ghosts through the 0.92 plates
	AddPanel(X0, Y0, W, H, PanelSolid);
	StripCard = ToScreen(FBox2D(FVector2D(X0, Y0), FVector2D(X0 + W, Y0 + H)));
	auto Rule = [&](float At) { DrawRect(LineSoft, At, Y0 + PadPx, 1.f, H - 2 * PadPx); };
	if (Strip.Lead.bShown)
	{
		const FTCScoreLead& L = Strip.Lead;
		DrawLabel(L.Mode, L.bModeDanger ? Red : TextC, X + PadPx, Y0 + PadPx, Font, 1.f);
		if (!L.Clock.IsEmpty()) DrawLabel(L.Clock, Brass, X + PadPx, Y0 + PadPx + LineH, Big, 1.f);
		if (!L.Threat.IsEmpty()) DrawLabel(L.Threat, Red, X + PadPx, Y0 + PadPx + 2 * LineH + 8.f, Font, 1.f);
		if (bNextWave)
		{
			FTCUiCommand Cmd(ETCUiAction::MatchControl);
			Cmd.Unit = LexToString(ETCMatchControl::NextWave);
			Button(Font, X + PadPx, Y0 + PadPx + 3 * LineH + 8.f + 4.f, NextWave.Label, Cmd, NextWave.bEnabled,
				NextWave.bEnabled ? FLinearColor(0.18f, 0.24f, 0.16f, 0.9f) : FLinearColor(0.16f, 0.16f, 0.16f, 0.85f), 0.f, LeadW - 2 * PadPx);
		}
		X += LeadW + Gap;
	}
	for (const FTCScoreTeam& T : Strip.Teams)
	{
		if (X > X0 + 0.5f) Rule(X - Gap / 2 - 0.5f);
		float Y = Y0 + PadPx;
		for (const FTCScoreSeat& Seat : T.Seats)
		{
			DrawRect(ATCUnitRenderer::SeatColor(Seat.Slot), X + PadPx, Y + 3.f, 10.f, 10.f);
			DrawLabel(Seat.Name, TextC, X + PadPx + 14.f, Y, Font, 1.f);
			if (Seat.bYou) DrawLabel(TEXT("you"), Brass, X + PadPx + 20.f + Width(Seat.Name, Font), Y, Font, 1.f);
			Y += LineH;
			DrawLabel(Seat.Held, Seat.bHeldDanger ? Red : DimC, X + PadPx + 14.f, Y, Font, 1.f);
			if (!Seat.Net.IsEmpty()) DrawLabel(Seat.Net, Seat.bNetDanger ? Red : DimC, X + Bw - PadPx - Width(Seat.Net, Font), Y, Font, 1.f);
			Y += LineH;
		}
		if (!T.Role.IsEmpty()) { DrawLabel(T.Role, Brass, X + PadPx, Y, Font, 1.f); Y += LineH; }
		if (T.bBar)
		{
			DrawRect(FLinearColor(0.f, 0.f, 0.f, 0.6f), X + PadPx, Y + 2.f, Bw - 2 * PadPx, 6.f);
			DrawRect(ATCUnitRenderer::SeatColor(T.ColourSlot), X + PadPx, Y + 2.f, (Bw - 2 * PadPx) * (float)T.Fraction, 6.f);
			Y += 10.f;
		}
		const FString Val = FString::Join(TArray<FString>({ T.U0, T.Num, T.U }).FilterByPredicate([](const FString& Q) { return !Q.IsEmpty(); }), TEXT(" "));
		DrawLabel(Val, T.bU0Danger ? Red : TextC, X + PadPx, Y, Font, 1.f);
		X += Bw + Gap;
	}
	float Y = Y0 + H + 4.f;
	auto Centred = [&](const FString& L, const FLinearColor& C, UFont* F)
	{
		if (L.IsEmpty()) return;
		const float TW = Width(L, F);
		AddPanel((Canvas->ClipX - TW) / 2 - PadPx, Y, TW + 2 * PadPx, LineH + 2.f, FLinearColor(0.f, 0.f, 0.f, 0.5f));
		DrawLabel(L, C, (Canvas->ClipX - TW) / 2, Y + 1.f, F, 1.f);
		Y += LineH + 4.f;
	};
	// client/endgame.js: the rate, catch-up and notice line, then the out banner; weather-view.js; objectives.js goal
	if (Strip.bLineShown)
		Centred(FString::Join(TArray<FString>({ Strip.Rate, Strip.Bonus, Strip.Notice }).FilterByPredicate([](const FString& Q) { return !Q.IsEmpty(); }), TEXT("  |  ")), TextC, Font);
	Centred(Strip.OutBanner, Red, Font);
	if (Strip.bWeather) Centred(Strip.Weather, Strip.bWeatherTurning ? Brass : DimC, Font);
	// round 15: hud.js drawBoss, #boss in #top under the strip (Client/TCBossBar.h): the Kaiju's health while one lives
	M->BossBar.Snapshot(S, FPlatformTime::Seconds());
	if (M->BossBar.Shown())
	{
		float BossBottom = Y;
		const FTCPaint Boss = M->BossBar.Layout(Canvas->ClipX / 2, Y, Canvas->ClipX, FPlatformTime::Seconds(), FTCPaint::CanvasMeasure(Font), &BossBottom);
		Boss.Draw(Canvas, Font);
		for (const FBox2D& B : Boss.Panels) Panels.Add(ToScreen(B));
		Y = BossBottom + 4.f;
	}
	// round 14: objectives.js's goal box and collapse banner as objectives.css draws them (FTCScenarioText), under the
	// strip and its lines
	const float Bottom = M->ScenarioText.Draw(Canvas, Big, Font, Y - 4.f);
	// round 15: client/stats.js, F2's overlay (Client/TCKeys.h). Round 16: drawn by DrawStatsBox after the match
	// controls (the top-left box goes under them, #util), from what the strip leaves here: top centre under #top
	StatsTopTc = FMath::Max(Y, Bottom + 4.f) - 4.f;
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
			// the label steps aside while the point is being taken; the cut and locked tags stay
			TArray<FString> Tags;
			if (P.bTagVisible && !P.Label.IsEmpty()) Tags.Add(P.Label);
			if (P.CutTag()) Tags.Add(TEXT("Cut off: no supply"));
			if (P.LockTag()) Tags.Add(TEXT("Locked: take the linked point first"));
			float Ly = Top.Y - 6.f - Tags.Num() * LineH;
			for (const FString& Tg : Tags)
			{
				float TW = 0, TH = 0;
				LabelSize(Tg, TW, TH, Font, 1.f);
				// round 17: a tag under the score strip's card is left out (see DrawScoreStrip)
				if (StripCard.bIsValid && StripCard.Intersect(FBox2D(FVector2D(Top.X - TW / 2 - 4.f, Ly - 1.f), FVector2D(Top.X + TW / 2 + 4.f, Ly - 1.f + LineH)))) { Ly += LineH; continue; }
				DrawRect(WithAlpha(Plate, 0.8f), Top.X - TW / 2 - 4.f, Ly - 1.f, TW + 8.f, LineH);
				{ FLinearColor Lc = Tg == P.Label ? TextC : Red; Lc.A *= M->WarMap.LabelFade(); DrawLabel(Tg, Lc, Top.X - TW / 2, Ly, Font, 1.f); } // round 14: fadeLabels
				Ly += LineH;
			}
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
			if (P.bDashed) GroundDashes(T, P.Points[0], P.Points.Last(), C, Pixels(P.Points.Last(), P.Width), 2.2f, 1.3f); // ORDER_DASH
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
		const bool bBar = bSel || PC->HoverEnemy == U.Id || 2.3f * Frac < 2.299f || U.Supp > 0 || bGarr;
		if (!bBar && U.Stars <= 0) continue;
		FVector2D At(U.X, U.Z);
		if (M->Units) M->Units->DrawnAt(U.Id, At);
		const FVector W = ATCTerrain::ToWorld(At.X, At.Y, M->Terrain->HeightAt(At.X, At.Y) + (bGarr ? (float)TCOrders::RoofBarHeight : BarY(U.Type, D)));
		const FVector C = Project(W), E = Project(W + Right * 120.f);
		if (C.Z <= 0 || E.Z <= 0 || C.X < 0 || C.Y < 0 || C.X > Canvas->ClipX || C.Y > Canvas->ClipY) continue;
		const float PxPerM = FMath::Clamp((float)FVector2D::Distance(FVector2D(C.X, C.Y), FVector2D(E.X, E.Y)) / 1.2f, 4.f, 30.f);
		const float BgW = 2.4f * PxPerM, BgH = FMath::Max(3.f, 0.42f * PxPerM), BarW = 2.3f * PxPerM;
		// round 17: not under the score strip's card (last frame's, screen pixels); the stars alone neither
		if (StripCard.bIsValid && StripCard.Intersect(FBox2D(FVector2D(C.X - BgW / 2, C.Y - BgH / 2), FVector2D(C.X + BgW / 2, C.Y + BgH / 2)))) continue;
		if (!bBar) { TCMarkersHud::DrawStars(Canvas, FVector2D(C.X, C.Y), PxPerM, U.Stars); continue; }
		DrawRect(FLinearColor(FColor(0x11, 0x11, 0x11)), C.X - BgW / 2, C.Y - BgH / 2, BgW, BgH);
		const FLinearColor HpC = Frac > 0.5f ? ATCUnitRenderer::SeatColor(U.Owner) : Frac > 0.25f ? FLinearColor(FColor(0xe0, 0x8a, 0x2a)) : FLinearColor(FColor(0xd0, 0x2a, 0x1a));
		DrawRect(HpC, C.X - BarW / 2, C.Y - 0.07f * PxPerM - FMath::Max(1.5f, 0.2f * PxPerM) / 2, BarW * FMath::Min(1.f, Frac), FMath::Max(1.5f, 0.2f * PxPerM));
		if (U.Supp > 0)
			DrawRect(U.Supp >= 90 ? FLinearColor(FColor(0xff, 0x3b, 0x2a)) : FLinearColor(FColor(0xff, 0xd2, 0x3a)), C.X - BarW / 2, C.Y + 0.11f * PxPerM - FMath::Max(1.f, 0.1f * PxPerM) / 2,
				BarW * FMath::Min(1.f, U.Supp / 100.f), FMath::Max(1.f, 0.1f * PxPerM));
		TCMarkersHud::DrawStars(Canvas, FVector2D(C.X, C.Y), PxPerM, U.Stars); // over the bar (renderOrder 4)
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
	In.Right = Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Max.X) : Canvas->ClipX - 8.f;
	In.MinimapTop = Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Min.Y) : Canvas->ClipY - 200.f;
	In.ViewW = Canvas->ClipX; In.ViewH = Canvas->ClipY;
	// round 15 (lead): at a large UI scale the recruit card (a tall column beside the minimap, hudfit) reaches above the
	// minimap under the feed's span; the feed then stands on the card instead of the minimap, so neither covers the other
	if (RecruitBox.bIsValid && ToUi((float)RecruitBox.Max.X) > In.Right - TCAlertLook::BoxMax && ToUi((float)RecruitBox.Min.Y) < In.MinimapTop)
		In.MinimapTop = ToUi((float)RecruitBox.Min.Y);
	float Mx = 0, My = 0;
	if (APlayerController* PC = GetOwningPlayerController(); PC && PC->GetMousePosition(Mx, My)) { In.bMouse = true; In.Mouse = FVector2D(ToUi(Mx), ToUi(My)); }
	M->AlertFeed.Frame(In.Now);
	const FTCPaint P = M->AlertFeed.Layout(In, FTCPaint::CanvasMeasure(Font));
	P.Draw(Canvas, Font);
	// round 17: the history box (alert-history.js #alertHistory) for the sandbox panel's place next frame
	AlertHistoryBox = FBox2D(ForceInit);
	if (const FTCPaintItem* H = P.Find(TEXT("alert:history"))) AlertHistoryBox = FBox2D(H->Pos, H->Pos + H->Size);
	for (const FTCPaintHit& H : P.Hits) { FTCUiCommand C(ETCUiAction::Alert); C.Unit = H.Tag; Buttons.Add({ ToScreen(H.Rect), C, H.Label, true }); }
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
	const FTCMeasure Measure = FTCPaint::CanvasMeasure(Font);
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
	const FTCMeasure Measure = FTCPaint::CanvasMeasure(Font);
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
	P.Draw(Canvas, Font);
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
	// the start's spawns as squares in each seat's colour (World sends none for another team's)
	for (int32 I = 0; I < S.Spawns.Num(); I++)
		if (S.Spawns[I].IsSet() && Minimap.WorldToPixel(S.Spawns[I].GetValue(), Px))
		{
			DrawRect(FLinearColor::Black, Px.X - 3.5f, Px.Y - 3.5f, 7.f, 7.f);
			DrawRect(ATCUnitRenderer::SeatColor(I), Px.X - 2.5f, Px.Y - 2.5f, 5.f, 5.f);
		}
	for (const FTCCapturePoint& P : S.Objectives.Points())
	{
		if (!Minimap.WorldToPixel(P.Centre, Px) || !Minimap.WorldToPixel(P.Centre + FVector2D(TCObjectiveRules::PointRadius, 0), Edge)) continue;
		const float Rad = FMath::Max(3.f, (float)FVector2D::Distance(Px, Edge));
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
			DrawLine(B0.X, B0.Y, B1.X, B1.Y, FLinearColor::Black, 1.f);
		}
		// cut off from its HQ: a red cross through it
		if (P.bCut)
		{
			const float R = Rad * 0.7f;
			const FLinearColor C(FColor(0xd0, 0x36, 0x2c));
			DrawLine(Px.X - R, Px.Y - R, Px.X + R, Px.Y + R, C, 2.f);
			DrawLine(Px.X + R, Px.Y - R, Px.X - R, Px.Y + R, C, 2.f);
		}
		// locked for this side: dashed to the point it needs
		FVector2D To;
		if (P.MinimapLink() && Minimap.WorldToPixel(P.LinkTo, To))
		{
			const double L = FVector2D::Distance(Px, To);
			const FVector2D D = L > 0 ? (To - Px) / L : FVector2D::ZeroVector;
			for (double T = 0; T < L; T += 10.0)
			{
				const FVector2D A = Px + D * T, B = Px + D * FMath::Min(L, T + 6.0);
				DrawLine(A.X, A.Y, B.X, B.Y, Brass, 2.f);
			}
		}
	}
	// round 15: client/alerts.js drawPings: each alert line's mark, a spreading ring and a steady one, dark under the
	// kind's colour (4 px, then 2 px)
	const FLinearColor Ink(FColor(0x15, 0x16, 0x0f));
	for (const FTCAlertRing& R : M->AlertFeed.MinimapRings(FPlatformTime::Seconds()))
	{
		if (R.Alpha <= 0.f || !Minimap.WorldToPixel(R.At, Px)) continue;
		constexpr int32 Segs = 24;
		for (int32 Pass = 0; Pass < 2; Pass++)
			for (int32 K = 0; K < Segs; K++)
			{
				const double A0 = 2 * PI * K / Segs, A1 = 2 * PI * (K + 1) / Segs;
				const FVector2D B0 = Px + FVector2D(FMath::Cos(A0), FMath::Sin(A0)) * R.RadiusPx, B1 = Px + FVector2D(FMath::Cos(A1), FMath::Sin(A1)) * R.RadiusPx;
				FLinearColor C = Pass == 0 ? Ink : R.Colour;
				C.A = R.Alpha;
				DrawLine(B0.X, B0.Y, B1.X, B1.Y, C, Pass == 0 ? 4.f : 2.f);
			}
	}
}
