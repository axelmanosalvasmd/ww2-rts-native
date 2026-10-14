// ATCHUD's slice 9 drawing (see TCHUD.h): the score strip, capture points, plan lines, health bars, alert feed and
// history, the epilogue's stamp, the Controls sheet and the lobby's match report. Everything is read from FTCMatchState
// (this recipient's own messages) and FTCObjectives, which is built from it; nothing is inferred for what a snapshot
// did not deliver. Canvas drawing only: placeholder presentation in the browser's colours, not its art.
#include "Client/TCHUD.h"
#include "Client/TCLocale.h"
#include "Client/TCGameMode.h"
#include "Client/TCObjectives.h"
#include "Client/TCKeys.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCArmyCommands.h"
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
	if (!Strip.bValid) return 8.f;
	auto Width = [&](const FString& L, UFont* F) { float TW = 0, TH = 0; LabelSize(L, TW, TH, F, 1.f); return TW; };
	// client/hud.js drawScores: a lead block (every mode but Conquest) and a block per team; the styles size the lead
	// 150 px (216 in Assault and Horde)
	const float BlockW = 236.f, LeadW = Strip.Lead.bShown ? (Strip.Kind == TEXT("assault") || Strip.Kind == TEXT("horde") ? 216.f : 150.f) : 0.f, Gap = 4.f;
	const int32 N = Strip.Teams.Num();
	float W = LeadW + (LeadW > 0 ? Gap : 0.f) + N * BlockW + FMath::Max(0, N - 1) * Gap;
	W = FMath::Min(W, Canvas->ClipX - 16.f);
	const float Bw = N ? (W - LeadW - (LeadW > 0 ? Gap : 0.f) - FMath::Max(0, N - 1) * Gap) / N : 0.f;
	// block heights: per seat two lines, then role, bar and value
	float H = 0.f;
	for (const FTCScoreTeam& T : Strip.Teams)
		H = FMath::Max(H, T.Seats.Num() * 2 * LineH + (T.Role.IsEmpty() ? 0.f : LineH) + (T.bBar ? 10.f : 0.f) + LineH + 2 * PadPx);
	if (Strip.Lead.bShown) H = FMath::Max(H, 3 * LineH + 8.f + 2 * PadPx);
	const float X0 = (Canvas->ClipX - W) / 2, Y0 = 6.f;
	float X = X0;
	if (Strip.Lead.bShown)
	{
		AddPanel(X, Y0, LeadW, H, Plate);
		const FTCScoreLead& L = Strip.Lead;
		DrawLabel(L.Mode, L.bModeDanger ? Red : TextC, X + PadPx, Y0 + PadPx, Font, 1.f);
		if (!L.Clock.IsEmpty()) DrawLabel(L.Clock, Brass, X + PadPx, Y0 + PadPx + LineH, Big, 1.f);
		if (!L.Threat.IsEmpty()) DrawLabel(L.Threat, Red, X + PadPx, Y0 + PadPx + 2 * LineH + 8.f, Font, 1.f);
		X += LeadW + Gap;
	}
	for (const FTCScoreTeam& T : Strip.Teams)
	{
		AddPanel(X, Y0, Bw, H, Plate);
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
	// round 14: objectives.js's goal box and collapse banner as objectives.css draws them (FTCScenarioText), under the
	// strip and its lines
	const float Bottom = M->ScenarioText.Draw(Canvas, Big, Font, Y - 4.f);
	return FMath::Max(Y, Bottom + 4.f);
}

void ATCHUD::DrawCapturePoints(UFont* Font)
{
	using namespace TCHudSlice9;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !M->Terrain) return;
	const ATCTerrain& T = *M->Terrain;
	const FTCObjectives& O = M->State.Objectives;
	const double R = FTCObjectives::RingRadius(), Ar = FTCObjectives::ArcRadius();
	auto Circle = [](FVector2D C, double Rad, int32 Segs, int32 Count)
	{
		// clockwise from north (markers.js ring), Count of Segs segments
		TArray<FVector2D> Pts;
		for (int32 K = 0; K <= Count; K++) { const double A = 2 * PI * K / Segs; Pts.Add(C + FVector2D(FMath::Sin(A), -FMath::Cos(A)) * Rad); }
		return Pts;
	};
	const TArray<FTCCapturePoint>& Points = O.Points();
	for (int32 I = 0; I < Points.Num(); I++)
	{
		const FTCCapturePoint& P = Points[I];
		const FVector Centre = Project(ATCTerrain::ToWorld(P.Centre.X, P.Centre.Y, T.HeightAt(P.Centre.X, P.Centre.Y)));
		if (Centre.Z <= 0 || Centre.X < -Canvas->ClipX || Centre.X > 2 * Canvas->ClipX || Centre.Y < -Canvas->ClipY || Centre.Y > 2 * Canvas->ClipY) continue;
		// the ring: chalk while neutral, the owner's colour once held (a point with no row yet draws neutral)
		const FLinearColor Ring = P.RingSlot >= 0 ? WithAlpha(ATCUnitRenderer::SeatColor(P.RingSlot), 0.95f) : WithAlpha(FLinearColor(NeutralRing), 0.8f);
		GroundLine(T, Circle(P.Centre, R, 48, 48), false, Ring, 2.5f, 0.12f);
		// the progress arc over its dim track, in the owner's (else the capper's) colour
		if (P.bArcVisible)
		{
			GroundLine(T, Circle(P.Centre, Ar, 48, 48), false, WithAlpha(FLinearColor(White), 0.22f), 2.f, 0.13f);
			GroundLine(T, Circle(P.Centre, Ar, TCObjectiveRules::ProgressSegments, P.ArcSegments), false, WithAlpha(ATCUnitRenderer::SeatColor(P.ArcSlot), 0.9f), 4.f, 0.14f);
		}
		const FTCPointRingFrame F = O.RingFrame(I);
		if (F.bDanger) GroundLine(T, Circle(P.Centre, R - 0.3, 48, 48), false, WithAlpha(FLinearColor(BrassRing), F.DangerOpacity), 3.f, 0.15f);
		if (F.bFlip) GroundLine(T, Circle(P.Centre, (R + 0.25) * F.FlipScale, 48, 48), false, WithAlpha(FLinearColor(BrassRing), F.FlipOpacity), 3.f, 0.15f);
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
	// client/markers.js planLayer draw: for each selected unit, its route in the plan's colour with a ring at the goal,
	// the waiting orders as dashed legs with rings, and the line to what it is shooting at with a ring around that
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !M->Terrain || !PC->Selected.Num()) return;
	const ATCTerrain& T = *M->Terrain;
	const FTCMatchState& S = M->State;
	TMap<int32, const FTCPlan*> Plans;
	TMap<int32, const FTCWaitingOrders*> Waiting;
	for (const FTCPlan& P : S.Plans) if (PC->Selected.Contains(P.Id)) Plans.Add(P.Id, &P);
	for (const FTCWaitingOrders& W : S.WaitingOrders) if (PC->Selected.Contains(W.Id)) Waiting.Add(W.Id, &W);
	auto Where = [&](const FTCUnit& U) { FVector2D P(U.X, U.Z); if (M->Units) M->Units->DrawnAt(U.Id, P); return P; };
	auto RadiusOf = [&](const FString& Type) { const FTCUnitDef* D = M->UnitTable.Find(Type); return D ? D->Radius : 1.5f; };
	auto Ring = [&](FVector2D At, float Rad, const FLinearColor& C) { GroundRing(T, At, Rad, C, 2.f); };
	auto Arrow = [&](FVector2D Tip, FVector2D Dir, const FLinearColor& C)
	{
		const FVector2D Side(-Dir.Y, Dir.X);
		GroundLine(T, { Tip - Dir * 1.4 + Side * 0.6, Tip, Tip - Dir * 1.4 - Side * 0.6 }, false, C, 2.5f, 0.12f);
	};
	for (const int32 Id : PC->Selected)
	{
		const FTCUnit* U = S.Units.Find(Id);
		if (!U || !ATCUnitRenderer::IsDrawn(*U)) continue;
		const FVector2D From = Where(*U);
		const float FromR = RadiusOf(U->Type) + 0.5f;
		const FTCPlan* const* PP = Plans.Find(Id);
		const FTCPlan* P = PP ? *PP : nullptr;
		// the target: a held unit it fires at, else an attack order's goal
		const FTCUnit* Tu = U->Target ? S.Units.Find(U->Target) : nullptr;
		const bool bHasT = Tu != nullptr || (P && P->Kind == 4);
		const FVector2D Tat = Tu ? Where(*Tu) : P ? FVector2D(P->Tx, P->Tz) : FVector2D::ZeroVector;
		const float Tr = bHasT ? (Tu ? RadiusOf(Tu->Type) : RadiusOf(TEXT("rifle"))) + 1.2f : 0.f;
		const bool bOnOrder = bHasT && P && P->Kind == 4 && FVector2D::Distance(Tat, FVector2D(P->Tx, P->Tz)) < 3.0;
		if (P && P->Kind)
		{
			const FLinearColor C = WithAlpha(PlanColour(P->Kind), 0.9f);
			TArray<FVector2D> Route = { From };
			Route.Append(P->Path);
			if (Route.Num() < 2) Route.Add(FVector2D(P->Tx, P->Tz));
			// start at the unit's ring
			Route[0] = From + Along(From, Route[1]) * FMath::Min((double)FromR, FVector2D::Distance(From, Route[1]));
			if (P->Kind > 4)
			{
				GroundLine(T, Route, false, C, 3.f, 0.12f);
				GroundDashes(T, Route.Last(), FVector2D(P->Tx, P->Tz), WithAlpha(C, 0.8f), 2.5f);
			}
			else
			{
				const double EndR = P->Kind == 4 ? (bOnOrder ? Tr : 2.7) + 0.3 : 1.2;
				const FVector2D Last = Route.Last(), Prev = Route[Route.Num() - 2];
				const FVector2D Dir = Along(Prev, Last);
				if (FVector2D::Distance(Prev, Last) > EndR) Route.Last() = Last - Dir * EndR;
				GroundLine(T, Route, false, C, 3.f, 0.12f);
				Arrow(Route.Last(), Dir, C);
			}
			if (P->Kind != 4) Ring(FVector2D(P->Tx, P->Tz), 1.f, WithAlpha(PlanColour(P->Kind), 0.95f));
		}
		FVector2D Q = P && P->Kind ? FVector2D(P->Tx, P->Tz) : From;
		if (const FTCWaitingOrders* const* W = Waiting.Find(Id))
			for (const FTCQueuedOrder& O : (*W)->Orders)
			{
				const FVector2D To(O.X, O.Z);
				GroundDashes(T, Q, To, WithAlpha(PlanColour(O.Kind), 0.8f), 2.5f);
				Ring(To, 1.f, WithAlpha(PlanColour(O.Kind), 0.95f));
				Q = To;
			}
		if (bHasT)
		{
			if (!bOnOrder) GroundDashes(T, From + Along(From, Tat) * FromR, Tat - Along(From, Tat) * (Tr + 0.3f), WithAlpha(AttackColour, 0.85f), 2.5f);
			Ring(Tat, Tr, WithAlpha(AttackColour, 0.95f));
		}
	}
}

void ATCHUD::DrawHealthBars()
{
	using namespace TCHudSlice9;
	// client/main.js: a bar over a unit only when it is selected, hovered, hurt or suppressed (a garrisoned squad's roof
	// bar is not drawn: this client draws no garrisoned squad); the health in the owner's colour, orange under half,
	// red under a quarter; the suppression bar under it, red from 90
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !M->Terrain || !PC->PlayerCameraManager) return;
	const FVector Right = FRotationMatrix(PC->PlayerCameraManager->GetCameraRotation()).GetUnitAxis(EAxis::Y);
	for (const auto& Pair : M->State.Units)
	{
		const FTCUnit& U = Pair.Value;
		const FTCUnitDef* D = M->UnitTable.Find(U.Type);
		if (!D || D->MaxHp() <= 0 || !ATCUnitRenderer::IsDrawn(U)) continue;
		const float Frac = FMath::Max(0.f, (float)U.Hp / D->MaxHp());
		const bool bSel = PC->Selected.Contains(U.Id);
		if (!(bSel || PC->HoverEnemy == U.Id || 2.3f * Frac < 2.299f || U.Supp > 0)) continue;
		FVector2D At(U.X, U.Z);
		if (M->Units) M->Units->DrawnAt(U.Id, At);
		const FVector W = ATCTerrain::ToWorld(At.X, At.Y, M->Terrain->HeightAt(At.X, At.Y) + BarY(U.Type, D));
		const FVector C = Project(W), E = Project(W + Right * 120.f);
		if (C.Z <= 0 || E.Z <= 0 || C.X < 0 || C.Y < 0 || C.X > Canvas->ClipX || C.Y > Canvas->ClipY) continue;
		const float PxPerM = FMath::Clamp((float)FVector2D::Distance(FVector2D(C.X, C.Y), FVector2D(E.X, E.Y)) / 1.2f, 4.f, 30.f);
		const float BgW = 2.4f * PxPerM, BgH = FMath::Max(3.f, 0.42f * PxPerM), BarW = 2.3f * PxPerM;
		DrawRect(FLinearColor(FColor(0x11, 0x11, 0x11)), C.X - BgW / 2, C.Y - BgH / 2, BgW, BgH);
		const FLinearColor HpC = Frac > 0.5f ? ATCUnitRenderer::SeatColor(U.Owner) : Frac > 0.25f ? FLinearColor(FColor(0xe0, 0x8a, 0x2a)) : FLinearColor(FColor(0xd0, 0x2a, 0x1a));
		DrawRect(HpC, C.X - BarW / 2, C.Y - 0.07f * PxPerM - FMath::Max(1.5f, 0.2f * PxPerM) / 2, BarW * FMath::Min(1.f, Frac), FMath::Max(1.5f, 0.2f * PxPerM));
		if (U.Supp > 0)
			DrawRect(U.Supp >= 90 ? FLinearColor(FColor(0xff, 0x3b, 0x2a)) : FLinearColor(FColor(0xff, 0xd2, 0x3a)), C.X - BarW / 2, C.Y + 0.11f * PxPerM - FMath::Max(1.f, 0.1f * PxPerM) / 2,
				BarW * FMath::Min(1.f, U.Supp / 100.f), FMath::Max(1.f, 0.1f * PxPerM));
	}
}

void ATCHUD::DrawAlertFeed(UFont* Font)
{
	using namespace TCHudSlice9;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M) return;
	const double Now = GetWorld()->GetTimeSeconds();
	const TArray<FTCAlertEntry>& A = M->AlertHistory();
	auto Colour = [](const FString& Kind) { return Kind == TEXT("pointWon") ? Brass : Kind == TEXT("pointLost") || Kind == TEXT("attack") ? Red : TextC; };
	const float Right = Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Max.X) : Canvas->ClipX - 8.f;
	float Bottom = Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Min.Y) - 6.f : Canvas->ClipY - 200.f;
	// client/alerts.js: up to four lines, each for 6 s, newest at the bottom over the minimap
	TArray<const FTCAlertEntry*> Shown;
	for (int32 I = 0; I < A.Num() && Shown.Num() < ATCGameMode::AlertLines; I++) if (Now - A[I].Born < ATCGameMode::AlertLineSeconds) Shown.Add(&A[I]);
	for (int32 I = 0; I < Shown.Num(); I++)
	{
		// client/alerts.js push: the text is tr(text) on its own and the hint is its own element, so each part is
		// translated by itself (the line put together would match no Spanish key)
		// (alerts.js:133, 136: the newest line's hint is 'Space or click: look there', and a click on it looks there)
		const FString L = I == 0 ? FString::Printf(TEXT("%s  (%s)"), *TC_T(Shown[I]->Text), *TC_T(FString(TEXT("Space or click: look there")))) : TC_T(Shown[I]->Text);
		float TW = 0, TH = 0;
		ShownSize(L, TW, TH, Font, 1.f);
		Bottom -= LineH + 4.f;
		AddPanel(Right - TW - 2 * PadPx, Bottom, TW + 2 * PadPx, LineH + 2.f, WithAlpha(Plate, 0.85f));
		DrawShown(L, Colour(Shown[I]->Kind), Right - TW - PadPx, Bottom + 1.f, Font, 1.f);
		if (I == 0) Buttons.Add({ ToScreen(FBox2D(FVector2D(Right - TW - 2 * PadPx, Bottom), FVector2D(Right, Bottom + LineH + 2.f))), FTCUiCommand(ETCUiAction::Alert), L, true });
	}
	// client/alert-history.js: Alt+H opens the match's alerts with their time; Alt+Left and Alt+Right visit them
	if (!M->bAlertHistoryOpen) return;
	const int32 Rows = FMath::Min(A.Num(), 14);
	const float W = 420.f, H = (Rows + 2) * LineH + 2 * PadPx;
	const float X = Right - W, Y = Bottom - H - 6.f;
	AddPanel(X, Y, W, H, Plate);
	DrawLabel(FString::Printf(TEXT("Alert history (%d)  Alt+Left/Right visit, Alt+H closes"), A.Num()), Brass, X + PadPx, Y + PadPx, Font, 1.f);
	if (!A.Num()) DrawLabel(TEXT("No alerts yet this match"), DimC, X + PadPx, Y + PadPx + LineH, Font, 1.f);
	for (int32 I = 0; I < Rows; I++)
	{
		const bool bCur = A[I].Id == M->AlertCursor;
		DrawShown(FString::Printf(TEXT("%s%s  %s"), bCur ? TEXT("> ") : TEXT("  "), *Clock(A[I].Tick / 20.0), *TC_T(A[I].Text)), bCur ? Brass : Colour(A[I].Kind), X + PadPx, Y + PadPx + (I + 1) * LineH, Font, 1.f);
	}
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

void ATCHUD::DrawMatchReport(UFont* Font, float Top)
{
	using namespace TCHudSlice9;
	// client/report.js: the lobby card after a match played to the end (hidden for a host-ended one): the result line, the
	// "decided ..." line, the chart of victory points (Conquest) or points held, its legend, and the story per player
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M) return;
	const FTCMatchState& S = M->State;
	const FTCMatchReport& R = S.Objectives.Report();
	if (!R.bShown) return;
	const float X0 = 716.f, W = FMath::Min(620.f, Canvas->ClipX - X0 - 8.f);
	if (W < 300.f) return;
	const float ChartH = 140.f, ColW = 62.f, NameW = W - 2 * PadPx - 7 * ColW;
	const float H = 2 * PadPx + 2 * LineH + 6.f + ChartH + 18.f + R.Sides.Num() * LineH + 6.f + (R.Rows.Num() + 1) * LineH;
	AddPanel(X0, Top, W, H, Plate);
	float Y = Top + PadPx;
	const FString Line = FTCObjectives::ResultLine(S.LastMatch, S.LobbySeat);
	DrawLabel(FString::Printf(TEXT("Last match: %s"), *Line), Line == TEXT("Victory") ? Brass : TextC, X0 + PadPx, Y, Font, 1.f); Y += LineH;
	DrawLabel(FString::Printf(TEXT("%s, %s"), *R.What, *R.Sub), DimC, X0 + PadPx, Y, Font, 1.f); Y += LineH + 6.f;
	// the chart: time across, value up, one line per side (the winners drawn last, on top)
	const float Cx = X0 + PadPx + 36.f, Cw = W - 2 * PadPx - 40.f, Cy = Y;
	DrawRect(FLinearColor(0.f, 0.f, 0.f, 0.35f), Cx, Cy, Cw, ChartH);
	DrawLabel(R.TopLabel, DimC, X0 + PadPx, Cy, Font, 1.f);
	DrawLabel(TEXT("0"), DimC, X0 + PadPx, Cy + ChartH - LineH, Font, 1.f);
	DrawLabel(R.EndLabel, DimC, Cx + Cw - 70.f, Cy + ChartH + 2.f, Font, 1.f);
	for (const FTCReportSide& Sd : R.Sides)
	{
		const FLinearColor C = ATCUnitRenderer::SeatColor(Sd.ColourSlot);
		for (int32 K = 1; K < Sd.Series.Num(); K++)
		{
			auto Px = [&](const FVector2D& V) { return FVector2D(Cx + Cw * (float)(V.X / R.End), Cy + ChartH - ChartH * (float)(R.Top > 0 ? V.Y / R.Top : 0.0)); };
			const FVector2D A = Px(Sd.Series[K - 1]), B = Px(Sd.Series[K]);
			DrawLine(A.X, A.Y, B.X, B.Y, C, Sd.bWon ? 2.5f : 1.5f);
		}
	}
	Y = Cy + ChartH + 18.f;
	for (const FTCReportSide& Sd : R.Sides)
	{
		DrawRect(ATCUnitRenderer::SeatColor(Sd.ColourSlot), X0 + PadPx, Y + 3.f, 10.f, 10.f);
		DrawLabel(Sd.Legend, Sd.bWon ? Brass : TextC, X0 + PadPx + 14.f, Y, Font, 1.f);
		Y += LineH;
	}
	Y += 6.f;
	static const TCHAR* const Short[] = { TEXT("Kills"), TEXT("Losses"), TEXT("Built"), TEXT("Capt."), TEXT("MP"), TEXT("Support"), TEXT("Planes") };
	DrawLabel(TEXT("Player"), DimC, X0 + PadPx, Y, Font, 1.f);
	for (int32 K = 0; K < 7; K++) DrawLabel(Short[K], DimC, X0 + PadPx + NameW + K * ColW, Y, Font, 1.f);
	Y += LineH;
	for (const FTCReportRow& Row : R.Rows)
	{
		DrawRect(ATCUnitRenderer::SeatColor(Row.Slot), X0 + PadPx, Y + 3.f, 10.f, 10.f);
		DrawLabel(Row.bMe ? FString::Printf(TEXT("%s (you)"), *Row.Name) : Row.Name, Row.bMe ? Brass : TextC, X0 + PadPx + 14.f, Y, Font, 1.f);
		for (int32 K = 0; K < 7; K++) DrawLabel(FString::FromInt(Row.Values[K]), TextC, X0 + PadPx + NameW + K * ColW, Y, Font, 1.f);
		Y += LineH;
	}
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
}
