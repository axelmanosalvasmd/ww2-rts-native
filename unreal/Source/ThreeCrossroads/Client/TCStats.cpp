// The stats overlay's sheet and the frame-number probe (round 16, controls scope). See Client/TCStats.h.
#include "Client/TCStats.h"
#include "Client/TCObjectives.h"
#include "UnrealClient.h"

namespace TCStatsL
{
	// index.html's base colours the sheet uses besides TCCss's
	const FLinearColor Cell = TCCss::Hex(0x24282b), CellHi = TCCss::Hex(0x2e3337), CellLo = TCCss::Hex(0x15181a);
	const FLinearColor LineHot = TCCss::Rgba(214, 178, 94, 0.8f), Backdrop = TCCss::Rgba(8, 10, 11, 0.3f);
	// Chromium's checkbox in the light colour scheme (the page sets none): white with a #767676 edge, or filled with the
	// accent colour (brass) and a dark tick, since the accent is light
	const FLinearColor CheckOff = TCCss::Hex(0xffffff), CheckEdge = TCCss::Hex(0x767676), Tick = TCCss::Hex(0x101010);

	FString Num(double V) { return FTCObjectives::JsToFixed(V, 1); }
}

const TArray<FTCStatChoice>& FTCStatsSheet::Choices()
{
	// stats.js CHOICES, in its key order, with index.html's label for each select
	static const TArray<FTCStatChoice> C = {
		{ TEXT("pos"), TEXT("Position"), { { TEXT("tl"), TEXT("Top left") }, { TEXT("tc"), TEXT("Top center") }, { TEXT("tr"), TEXT("Top right") } } },
		{ TEXT("layout"), TEXT("Layout"), { { TEXT("list"), TEXT("List") }, { TEXT("line"), TEXT("One line") } } },
		{ TEXT("size"), TEXT("Size"), { { TEXT("s"), TEXT("Small") }, { TEXT("m"), TEXT("Medium") }, { TEXT("l"), TEXT("Large") } } },
		{ TEXT("bg"), TEXT("Background"), { { TEXT("panel"), TEXT("Panel") }, { TEXT("clear"), TEXT("See-through") }, { TEXT("none"), TEXT("None") } } },
	};
	return C;
}

FString FTCStatsSheet::ChoiceText(const FString& Key, const FString& Value)
{
	for (const FTCStatChoice& C : Choices())
		if (Key == C.Key)
			for (const TPair<FString, FString>& O : C.Options) if (O.Key == Value) return O.Value;
	return FString();
}

TArray<FString> FTCStatsSheet::Groups()
{
	TArray<FString> G;
	for (const FTCStatDef& D : FTCStatsOverlay::Defs()) G.AddUnique(D.Group);
	return G;
}

bool FTCStatsSheet::Open()
{
	if (bOpen) return false; // stats.open(): if (sheet.open) return
	bOpen = true;
	ScrollY = 0.f; // showModal shows the dialog from its top
	return true;
}

namespace
{
	FString& SettingOf(FTCStatsSettings& S, const FString& Key)
	{
		if (Key == TEXT("layout")) return S.Layout;
		if (Key == TEXT("size")) return S.Size;
		if (Key == TEXT("bg")) return S.Bg;
		return S.Pos;
	}
}

FTCStatsSheet::EPress FTCStatsSheet::Press(const FString& Tag, bool bBack, FTCStatsOverlay& Overlay)
{
	FTCStatsSettings& S = Overlay.MutableSettings();
	if (Tag == TEXT("stats:close") || Tag == TEXT("stats:backdrop")) { Close(); return EPress::Closed; }
	if (!bOpen) return EPress::None;
	// $('input[name="show"]').onchange, $('input[name="warn"]').onchange
	if (Tag == TEXT("stats:show")) { S.bShow = !S.bShow; return EPress::Changed; }
	if (Tag == TEXT("stats:warn")) { S.bWarn = !S.bWarn; return EPress::Changed; }
	if (Tag.StartsWith(TEXT("stats:select:")))
	{
		// select[name=key].onchange with the next (or, stepping back, the previous) option
		const FString Key = Tag.Mid(13);
		for (const FTCStatChoice& C : Choices())
		{
			if (Key != C.Key) continue;
			FString& V = SettingOf(S, Key);
			int32 I = C.Options.IndexOfByPredicate([&V](const TPair<FString, FString>& O) { return O.Key == V; });
			I = ((I < 0 ? 0 : I) + (bBack ? -1 : 1) + C.Options.Num()) % C.Options.Num();
			V = C.Options[I].Key;
			return EPress::Changed;
		}
		return EPress::None;
	}
	if (Tag.StartsWith(TEXT("stats:pick:")))
	{
		// .stat-picks onchange: the checked boxes, in STATS order; picking a number means you want to see it
		const FString Id = Tag.Mid(11);
		TArray<FString> Picked = S.Stats;
		if (Picked.Contains(Id)) Picked.Remove(Id); else Picked.Add(Id);
		S.Stats.Reset();
		for (const FTCStatDef& D : FTCStatsOverlay::Defs()) if (Picked.Contains(D.Id)) S.Stats.Add(D.Id);
		if (S.Stats.Num()) S.bShow = true;
		return EPress::Changed;
	}
	if (Tag.StartsWith(TEXT("stats:preset:")))
	{
		const TArray<FString> P = FTCStatsOverlay::Preset(Tag.Mid(13));
		if (!P.Num()) return EPress::None;
		S.Stats = P; S.bShow = true;
		return EPress::Changed;
	}
	return EPress::None;
}

FTCStatsSheet::EPress FTCStatsSheet::PressAt(FVector2D Pixel, bool bBack, FTCStatsOverlay& Overlay, FString* OutTag)
{
	if (!bOpen) return EPress::None;
	for (const FTCPaintHit& H : Hits)
		if (H.Rect.IsInsideOrOn(Pixel))
		{
			if (OutTag) *OutTag = H.Tag;
			return Press(H.Tag, bBack, Overlay);
		}
	// sheet.addEventListener('click', (e) => { if (e.target === sheet) sheet.close(); }): the dialog element's own box
	// is the whole viewport behind ::backdrop, so a click outside the sheet's box closes it
	if (Box.bIsValid && Box.IsInsideOrOn(Pixel)) { if (OutTag) *OutTag = TEXT("stats:sheet"); return EPress::None; }
	if (OutTag) *OutTag = TEXT("stats:backdrop");
	return Press(TEXT("stats:backdrop"), bBack, Overlay);
}

FBox2D FTCStatsSheet::DialogBox(const FTCPaint& P)
{
	return P.Panels.Num() ? P.Panels[0] : FBox2D(ForceInit);
}

FTCPaint FTCStatsSheet::Layout(const FTCStatsSettings& S, float ViewW, float ViewH, const FTCMeasure& M, FVector2D Mouse, ETCLanguage Lang) const
{
	using namespace TCStatsSheetRules;
	using namespace TCStatsL;
	auto Tr = [Lang](const FString& Text) { return FTCLocale::Translate(Text, Lang); };
	const bool bNarrow = ViewW <= NarrowAt;
	const float W = FMath::Max(0.f, FMath::Min(MaxW, ViewW - SideRoom));
	const float MaxHeight = FMath::Min(ViewH * MaxHShare, MaxH);
	const float CW = W - 2 * Border - 2 * Pad; // the .sheet grid's content width
	const float X0 = Border + Pad, Lh = BodyPx * BodyLine, SmallLh = SmallPx * SmallLine, KbdH = KbdPx * KbdLine + 2.f;
	// first the sheet in its own coordinates (the dialog's top left at 0, 0), then moved to the window's centre
	FTCPaint C;
	auto Hover = [&Mouse](float X, float Y, float BW, float BH, FVector2D At) { return Mouse.X >= 0 && FBox2D(At + FVector2D(X, Y), At + FVector2D(X + BW, Y + BH)).IsInsideOrOn(Mouse); };
	TArray<TTuple<float, float, float, float, bool>> Buttons; // x, y, w, h, enabled: hover is judged after the move
	auto KbdW = [&M](const FString& K) { return M(K, KbdPx, true, 0.f) + 2 * KbdPadX + 2.f; };
	auto Kbd = [&](const FString& K, float X, float Y)
	{
		// kbd: 600 13px/1.3, padding 0 4px, a soft 1 px border, dim
		const float KW = KbdW(K);
		C.Frame(X, Y, KW, KbdH, TCCss::LineSoft, 1.f, TEXT("stats:kbd-frame"));
		FTCPaintItem& T = C.Text(K, X + 1.f + KbdPadX, Y + 1.f, KbdPx, TCCss::Dim, true, ETCPaintAlign::Left, TEXT("stats:kbd"));
		T.Size.Y = KbdPx * KbdLine;
		return KW;
	};
	auto CheckBox = [&](float X, float Y, bool bOn, const TCHAR* Role)
	{
		if (bOn)
		{
			C.Rect(X, Y, Check, Check, TCCss::Brass, Role);
			C.Line(FVector2D(X + 3.5f, Y + 7.5f), FVector2D(X + 6.5f, Y + 10.5f), Tick, 2.f, TEXT("stats:tick"));
			C.Line(FVector2D(X + 6.5f, Y + 10.5f), FVector2D(X + 11.5f, Y + 4.5f), Tick, 2.f, TEXT("stats:tick"));
		}
		else
		{
			C.Rect(X, Y, Check, Check, CheckOff, Role);
			C.Frame(X, Y, Check, Check, CheckEdge, 1.f, TEXT("stats:check-frame"));
		}
	};
	auto Label = [&](const FString& Text, float X, float Y, float Px, const FLinearColor& Col, bool bBold, float LineH, const TCHAR* Role)
	{
		FTCPaintItem& T = C.Text(Text, X, Y, Px, Col, bBold, ETCPaintAlign::Left, Role);
		T.Size.Y = LineH;
	};

	// header: h2 'Stats overlay' and the Close button with its Esc key cap, a soft line under them
	float Y = Border;
	const FString Title = Tr(TEXT("Stats overlay")), CloseT = Tr(TEXT("Close"));
	const float TitleH = TitlePx * BodyLine;
	const float CloseW = 2 * ButtonPadX + 2.f + M(CloseT, BodyPx, false, 0.f) + M(TEXT(" "), BodyPx, false, 0.f) + KbdW(TEXT("Esc"));
	const float CloseH = 2 * ButtonPadY + 2.f + FMath::Max(Lh, KbdH);
	const float HeadInner = FMath::Max(TitleH, CloseH), HeadH = 2 * HeadPadY + HeadInner + 1.f;
	Label(Title, X0, Y + HeadPadY + (HeadInner - TitleH) / 2, TitlePx, TCCss::Text, true, TitleH, TEXT("stats:title"));
	{
		const float BX = X0 + CW - CloseW, BY = Y + HeadPadY + (HeadInner - CloseH) / 2;
		Buttons.Add(MakeTuple(BX, BY, CloseW, CloseH, true));
		Label(CloseT, BX + 1.f + ButtonPadX, BY + 1.f + ButtonPadY, BodyPx, TCCss::Text, false, FMath::Max(Lh, KbdH), TEXT("stats:button"));
		Kbd(TEXT("Esc"), BX + 1.f + ButtonPadX + M(CloseT, BodyPx, false, 0.f) + M(TEXT(" "), BodyPx, false, 0.f), BY + 1.f + ButtonPadY + (FMath::Max(Lh, KbdH) - KbdH) / 2);
		C.Hit(FBox2D(FVector2D(BX, BY), FVector2D(BX + CloseW, BY + CloseH)), TEXT("stats:close"), TEXT("Close"));
	}
	Y += HeadH + Gap;

	// .stat-top: the show switch (600) with its F2 key cap; 'Quick pick' and the preset buttons, apart (space-between)
	{
		const FString ShowT = Tr(TEXT("Show the overlay")), QuickT = Tr(TEXT("Quick pick"));
		const float LeftW = Check + CheckGap + M(ShowT, BodyPx, true, 0.f) + CheckGap + KbdW(TEXT("F2"));
		const float LeftH = FMath::Max(Check, FMath::Max(Lh, KbdH));
		static const TCHAR* const PresetNames[] = { TEXT("Basic"), TEXT("Performance"), TEXT("Network"), TEXT("Everything") };
		TArray<float> PW;
		float RightW = M(QuickT, BodyPx, false, 0.f) + PresetRowGap;
		for (int32 I = 0; I < 4; I++) { PW.Add(2 * PresetPadX + 2.f + M(Tr(PresetNames[I]), BodyPx, false, 0.f)); RightW += PW[I] + (I ? PresetGap : 0.f); }
		const float PresetH = 2 * PresetPadY + 2.f + Lh;
		const bool bOneRow = LeftW + TopGapX + RightW <= CW;
		const float RowH = bOneRow ? FMath::Max(LeftH, PresetH) : 0.f;
		const float LY = Y + (bOneRow ? (RowH - LeftH) / 2 : 0.f);
		CheckBox(X0, LY + (LeftH - Check) / 2, S.bShow, TEXT("stats:check:show"));
		Label(ShowT, X0 + Check + CheckGap, LY + (LeftH - Lh) / 2, BodyPx, TCCss::Text, true, Lh, TEXT("stats:show"));
		Kbd(TEXT("F2"), X0 + Check + CheckGap + M(ShowT, BodyPx, true, 0.f) + CheckGap, LY + (LeftH - KbdH) / 2);
		C.Hit(FBox2D(FVector2D(X0, LY), FVector2D(X0 + LeftW, LY + LeftH)), TEXT("stats:show"), TEXT("Show the overlay"));
		const float RY = bOneRow ? Y + (RowH - PresetH) / 2 : Y + LeftH + TopGapY;
		float RX = bOneRow ? X0 + CW - RightW : X0;
		Label(QuickT, RX, RY + (PresetH - Lh) / 2, BodyPx, TCCss::Dim, false, Lh, TEXT("stats:muted"));
		RX += M(QuickT, BodyPx, false, 0.f) + PresetRowGap;
		for (int32 I = 0; I < 4; I++)
		{
			Buttons.Add(MakeTuple(RX, RY, PW[I], PresetH, true));
			Label(Tr(PresetNames[I]), RX + 1.f + PresetPadX, RY + 1.f + PresetPadY, BodyPx, TCCss::Text, false, Lh, TEXT("stats:preset"));
			C.Hit(FBox2D(FVector2D(RX, RY), FVector2D(RX + PW[I], RY + PresetH)), FString(TEXT("stats:preset:")) + PresetNames[I], PresetNames[I]);
			RX += PW[I] + PresetGap;
		}
		Y += bOneRow ? RowH : LeftH + TopGapY + PresetH;
	}
	Y += Gap;

	// .stat-picks: a fieldset per group (legend 600 16px brass), a label per stat (15 px box, the label, the hint under it)
	{
		const TArray<FString> Groups = FTCStatsSheet::Groups();
		const int32 Cols = bNarrow ? 1 : 3;
		const float ColW = (CW - (Cols - 1) * PicksGapX) / Cols, TextW = ColW - Check - CheckGap;
		const float LegendH = LegendPx * BodyLine + LegendPadBottom;
		float RowTop = Y, RowH = 0.f;
		for (int32 G = 0; G < Groups.Num(); G++)
		{
			const int32 Col = G % Cols;
			if (Col == 0 && G > 0) { RowTop += RowH + PicksGapY; RowH = 0.f; }
			const float X = X0 + Col * (ColW + PicksGapX);
			float FY = RowTop;
			Label(Tr(Groups[G]), X, FY, LegendPx, TCCss::Brass, true, LegendPx * BodyLine, TEXT("stats:legend"));
			FY += LegendH;
			bool bFirst = true;
			for (const FTCStatDef& D : FTCStatsOverlay::Defs())
			{
				if (Groups[G] != D.Group) continue;
				if (!bFirst) FY += FieldGap;
				bFirst = false;
				const TArray<FString> Lines = FTCPaint::Wrap(Tr(D.Label), TextW, BodyPx, false, M);
				const float Row1 = FMath::Max(Check, Lines.Num() * Lh);
				const float Top = FY;
				CheckBox(X, FY + (Row1 - Check) / 2, S.Stats.Contains(D.Id), TEXT("stats:check:pick"));
				for (int32 L = 0; L < Lines.Num(); L++) Label(Lines[L], X + Check + CheckGap, FY + (Row1 - Lines.Num() * Lh) / 2 + L * Lh, BodyPx, TCCss::Text, false, Lh, TEXT("stats:label"));
				FY += Row1;
				if (D.Hint)
					for (const FString& H : FTCPaint::Wrap(Tr(D.Hint), TextW, SmallPx, false, M))
					{
						Label(H, X + Check + CheckGap, FY, SmallPx, TCCss::Dim, false, SmallLh, TEXT("stats:hint"));
						FY += SmallLh;
					}
				C.Hit(FBox2D(FVector2D(X, Top), FVector2D(X + ColW, FY)), FString(TEXT("stats:pick:")) + D.Id, D.Label);
			}
			RowH = FMath::Max(RowH, FY - RowTop);
		}
		Y = RowTop + RowH + Gap;
	}

	// h3 'Look' (bold 16px brass), then the four labelled selects
	Label(Tr(TEXT("Look")), X0, Y, H3Px, TCCss::Brass, true, H3Px * BodyLine, TEXT("stats:h3"));
	Y += H3Px * BodyLine + Gap;
	{
		const int32 Cols = bNarrow ? 2 : 4;
		const float ColW = (CW - (Cols - 1) * LookGapX) / Cols, SelH = 2 * SelectPadY + 2.f + Lh;
		const float CellH = Lh + LookLabelGap + SelH;
		const TArray<FTCStatChoice>& Ch = Choices();
		for (int32 I = 0; I < Ch.Num(); I++)
		{
			const float X = X0 + (I % Cols) * (ColW + LookGapX), CY = Y + (I / Cols) * (CellH + LookGapY);
			Label(Tr(Ch[I].Label), X, CY, BodyPx, TCCss::Dim, false, Lh, TEXT("stats:look-label"));
			const float SY = CY + Lh + LookLabelGap;
			FTCStatsSettings Copy = S;
			const FString Shown = Tr(ChoiceText(Ch[I].Key, SettingOf(Copy, Ch[I].Key)));
			C.Rect(X, SY, ColW, SelH, CellLo, TEXT("stats:select"));
			C.Frame(X, SY, ColW, SelH, TCCss::LineSoft, 1.f, TEXT("stats:select-frame"));
			Label(FTCPaint::Ellipsis(Shown, ColW - 2 * SelectPadX - 2.f - 14.f, BodyPx, false, M), X + 1.f + SelectPadX, SY + 1.f + SelectPadY, BodyPx, TCCss::Text, false, Lh, TEXT("stats:select-value"));
			// the drop-down arrow at the right
			const float AX = X + ColW - 1.f - SelectPadX - 5.f, AY = SY + SelH / 2;
			C.Line(FVector2D(AX - 4.f, AY - 2.f), FVector2D(AX, AY + 2.f), TCCss::Text, 1.5f, TEXT("stats:arrow"));
			C.Line(FVector2D(AX, AY + 2.f), FVector2D(AX + 4.f, AY - 2.f), TCCss::Text, 1.5f, TEXT("stats:arrow"));
			C.Hit(FBox2D(FVector2D(X, SY), FVector2D(X + ColW, SY + SelH)), FString(TEXT("stats:select:")) + Ch[I].Key, Ch[I].Label);
		}
		Y += ((Ch.Num() + Cols - 1) / Cols) * CellH + ((Ch.Num() + Cols - 1) / Cols - 1) * LookGapY + Gap;
	}

	// the warnings switch
	{
		const TArray<FString> Lines = FTCPaint::Wrap(Tr(TEXT("Color warnings: amber, then red, when a number is bad")), CW - Check - CheckGap, BodyPx, false, M);
		const float RowH = FMath::Max(Check, Lines.Num() * Lh);
		CheckBox(X0, Y + (RowH - Check) / 2, S.bWarn, TEXT("stats:check:warn"));
		for (int32 L = 0; L < Lines.Num(); L++) Label(Lines[L], X0 + Check + CheckGap, Y + (RowH - Lines.Num() * Lh) / 2 + L * Lh, BodyPx, TCCss::Text, false, Lh, TEXT("stats:warn"));
		float WW = 0.f;
		for (const FString& L : Lines) WW = FMath::Max(WW, M(L, BodyPx, false, 0.f));
		C.Hit(FBox2D(FVector2D(X0, Y), FVector2D(X0 + Check + CheckGap + WW, Y + RowH)), TEXT("stats:warn"), TEXT("Color warnings"));
		Y += RowH;
	}
	const float FullH = Y + Pad + Border, H = FMath::Min(FullH, MaxHeight);

	// the backdrop, the dialog (centred), then the sheet moved there; past max-height the sheet scrolls in the
	// browser: what falls below the dialog's bottom is left out here (and so are its hits)
	FTCPaint P;
	const FVector2D At((ViewW - W) / 2, (ViewH - H) / 2);
	P.Rect(0, 0, ViewW, ViewH, Backdrop, TEXT("stats:backdrop"));
	P.Rect(At.X, At.Y, W, H, TCCss::PanelSolid, TEXT("stats:dialog"));
	P.Panels.Add(FBox2D(At, At + FVector2D(W, H)));
	// the sticky header's own background and its soft bottom line
	P.Rect(At.X + Border, At.Y + Border, W - 2 * Border, HeadH, TCCss::PanelSolid, TEXT("stats:header"));
	P.Rect(At.X + Border, At.Y + Border + HeadH - 1.f, W - 2 * Border, 1.f, TCCss::LineSoft, TEXT("stats:header-line"));
	// the scrolled part runs from under the sticky header to the dialog's bottom border; the header itself stays put.
	// The canvas has no clip, so an item that would cross either edge is left out (with its hit).
	MaxScroll = FMath::Max(0.f, FullH - H);
	const float Sy = FMath::Clamp(ScrollY, 0.f, MaxScroll), Cut = H - Border, HeadBottom = Border + HeadH;
	auto Fits = [&](float Top, float Bottom, bool bHeader) { return bHeader ? true : Top - Sy >= HeadBottom - 0.5f && Bottom - Sy <= Cut + 0.5f; };
	auto InHeader = [&](float Top) { return Top < HeadBottom; };
	for (const TTuple<float, float, float, float, bool>& B : Buttons)
	{
		const bool bHead = InHeader(B.Get<1>());
		if (!Fits(B.Get<1>(), B.Get<1>() + B.Get<3>(), bHead)) continue;
		const float By = B.Get<1>() - (bHead ? 0.f : Sy);
		const bool bHot = Hover(B.Get<0>(), By, B.Get<2>(), B.Get<3>(), At);
		P.Rect(At.X + B.Get<0>(), At.Y + By, B.Get<2>(), B.Get<3>(), bHot ? CellHi : Cell, TEXT("stats:button-face"));
		P.Frame(At.X + B.Get<0>(), At.Y + By, B.Get<2>(), B.Get<3>(), bHot ? LineHot : TCCss::LineSoft, 1.f, TEXT("stats:button-frame"));
	}
	const int32 First = P.Items.Num();
	for (const FTCPaintItem& I : C.Items)
	{
		const float Top = I.Kind == ETCPaintKind::Line ? FMath::Min(I.Pos.Y, I.Size.Y) : I.Pos.Y;
		const float Bottom = I.Kind == ETCPaintKind::Line ? FMath::Max(I.Pos.Y, I.Size.Y) : I.Pos.Y + FMath::Max(I.Size.Y, I.Kind == ETCPaintKind::Text ? I.FontPx * 1.2f : 0.f);
		const bool bHead = InHeader(Top);
		if (!Fits(Top, Bottom, bHead)) continue;
		FTCPaintItem& Added = P.Items.Add_GetRef(I);
		if (!bHead) { Added.Pos.Y -= Sy; if (I.Kind == ETCPaintKind::Line) Added.Size.Y -= Sy; }
	}
	P.Shift(First, At);
	for (const FTCPaintHit& Hh : C.Hits)
	{
		const bool bHead = InHeader(Hh.Rect.Min.Y);
		if (!Fits(Hh.Rect.Min.Y, Hh.Rect.Max.Y, bHead)) continue;
		const FVector2D Off = At - FVector2D(0, bHead ? 0.f : Sy);
		P.Hit(FBox2D(Hh.Rect.Min + Off, Hh.Rect.Max + Off), Hh.Tag, Hh.Label);
	}
	P.Frame(At.X, At.Y, W, H, TCCss::Line, 1.f, TEXT("stats:dialog-frame"));
	return P;
}

// ---------------------------------------------------------------------------------------------------------------------
// FTCStatUnitProbe

FString FTCStatUnitProbe::Line(const FTCPerfStats& Pf, double UnitFrame, double UnitGame, double UnitDraw, double UnitRhit, double UnitGpu, double RawMean, double RawWorst, int32 Count)
{
	using TCStatsL::Num;
	return FString::Printf(TEXT("stats probe: overlay FPS %s, Frame %s ms, Worst %s ms, JS %s ms | stat unit Frame %s ms, Game %s ms, Draw %s ms, RHIT %s ms, GPU %s ms | stat unit raw frames over the same %d frames: mean %s ms (%s fps), worst %s ms"),
		*Num(Pf.Fps), *Num(Pf.Ms), *Num(Pf.Worst), *Num(Pf.Cpu), *Num(UnitFrame), *Num(UnitGame), *Num(UnitDraw), *Num(UnitRhit), *Num(UnitGpu),
		Count, *Num(RawMean), *Num(RawMean > 0 ? 1000.0 / RawMean : 0.0), *Num(RawWorst));
}

FString FTCStatUnitProbe::Frame(const FTCPerfStats& Pf, const FStatUnitData* Unit, bool bUnitShown)
{
	const bool bRolled = Pf.Fps != LastFps || Pf.Ms != LastMs || Pf.Worst != LastWorst || Pf.Cpu != LastCpu;
	FString Out;
	if (bRolled)
	{
		if (bPrimed && Unit && bUnitShown && RawCount > 0)
			Out = Line(Pf, Unit->FrameTime, Unit->GameThreadTime, Unit->RenderThreadTime, Unit->RHITTime, Unit->GPUFrameTime[0], RawSum / RawCount, RawWorst, RawCount);
		LastFps = Pf.Fps; LastMs = Pf.Ms; LastWorst = Pf.Worst; LastCpu = Pf.Cpu;
		RawSum = RawWorst = 0; RawCount = 0;
		bPrimed = true;
	}
	// stat unit's RawFrameTime is the frame the engine just measured (FApp's delta); kept here for the next roll
	if (Unit && bUnitShown) { RawSum += Unit->RawFrameTime; RawWorst = FMath::Max(RawWorst, (double)Unit->RawFrameTime); RawCount++; }
	return Out;
}

// ---------------------------------------------------------------------------------------------------------------------
// The glue (round 16, controls): ATCHUD, ATCGameMode and ATCPlayerController members that the lead patch declares
// (round-16/controls/lead-patch), defined here so the patch stays small. Without the patch this file compiles alone.
#include "Client/TCGameMode.h"
#if defined(TC_HAS_STATS_GLUE)
#include "Client/TCHUD.h"
#include "Client/TCPlayerController.h"
#include "Client/TCPersistence.h"
#include "ThreeCrossroads.h"
#include "Engine/Canvas.h"
#include "Engine/Engine.h"
#include "Engine/GameViewportClient.h"
#include "Engine/World.h"

void ATCHUD::DrawStatsSheet(UFont* Font)
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !Canvas) return;
	// -TCStatUnitProbe: the overlay's frame numbers beside the engine's 'stat unit' once a roll
	static const bool bProbe = FParse::Param(FCommandLine::Get(), TEXT("TCStatUnitProbe"));
	if (bProbe && GEngine && GEngine->GameViewport)
	{
		const FString Line = M->StatUnitProbe.Frame(M->Perf.Stats(), GEngine->GameViewport->GetStatUnitData(), GEngine->GameViewport->IsStatEnabled(TEXT("Unit")));
		if (!Line.IsEmpty()) UE_LOG(LogTC, Log, TEXT("%s"), *Line);
	}
	if (!M->StatsSheet.IsOpen()) return;
	// a dialog outside #hud: screen pixels, never the UI scale (drawn after EndUi)
	FVector2D Mouse(-1, -1);
	float MX, MY;
	if (GetOwningPlayerController() && GetOwningPlayerController()->GetMousePosition(MX, MY)) Mouse = FVector2D(MX, MY);
	const FTCPaint P = M->StatsSheet.Layout(M->StatsOverlay().Settings(), Canvas->ClipX, Canvas->ClipY, FTCPaint::CanvasMeasure(Font), Mouse);
	P.Draw(Canvas, Font);
	M->StatsSheet.Remember(P);
	Panels.Append(P.Panels);
}

float ATCHUD::DrawStatsBox(UFont* Font, float CtlBottom, float CtlRight)
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !Canvas) return 0.f;
	const FTCStatsOverlay& O = M->StatsOverlay();
	if (!O.Settings().bShow) return 0.f;
	// stats.js place(): screen pixels in, HUD (UI) pixels out; under.tl is #util (here the match controls' panel),
	// under.tc #top, under.tr #econ (none in this HUD: 52 px); clear: #scores
	const float TopTl = CtlBottom > 0.f ? ToUi(CtlBottom) : -1.f;
	const FTCPaint Box = O.Layout(M->Perf.Stats(), TopTl, StatsTopTc, -1.f, StatsStrip, Canvas->ClipX, CtlRight > 0.f ? ToUi(CtlRight) : 0.f, Canvas->ClipX, FTCPaint::CanvasMeasure(Font));
	Box.Draw(Canvas, Font);
	float Bottom = 0.f;
	for (const FTCPaintItem& I : Box.Items) Bottom = FMath::Max(Bottom, (float)(I.Pos.Y + I.Size.Y));
	return O.Settings().Pos == TEXT("tl") && Bottom > 0.f ? Bottom * UiNow : 0.f;
}

bool ATCGameMode::StatsSheetPress(FVector2D Pixel, bool bBack)
{
	FString Tag;
	const FTCStatsSheet::EPress R = StatsSheet.PressAt(Pixel, bBack, StatsOverlay(), &Tag);
	if (R == FTCStatsSheet::EPress::Changed)
	{
		// changed(): save(), sync(), build()
		if (const TOptional<FString> Saved = Stats.SaveText()) FTCSettingsStore::Game().Set(TEXT("ww2-stats"), *Saved);
	}
	UE_LOG(LogTC, Log, TEXT("stats sheet: %s at (%.0f, %.0f)%s: %s; %s"), *Tag, Pixel.X, Pixel.Y, bBack ? TEXT(" (right)") : TEXT(""),
		R == FTCStatsSheet::EPress::Changed ? TEXT("changed") : R == FTCStatsSheet::EPress::Closed ? TEXT("closed") : TEXT("nothing"),
		Stats.SaveText().IsSet() ? *Stats.SaveText().GetValue() : TEXT("(forced, not saved)"));
	return R != FTCStatsSheet::EPress::None;
}

bool ATCGameMode::StatsSheetTag(const FString& Tag, bool bBack)
{
	// a verification step: the control drawn with this tag, pressed at its drawn centre when the sheet drew it
	for (const FTCPaintHit& H : StatsSheet.LastHits())
		if (H.Tag == Tag) return StatsSheetPress(H.Rect.GetCenter(), bBack);
	UE_LOG(LogTC, Log, TEXT("stats sheet: %s not drawn: run directly"), *Tag);
	const FTCStatsSheet::EPress R = StatsSheet.Press(Tag, bBack, StatsOverlay());
	if (R == FTCStatsSheet::EPress::Changed)
		if (const TOptional<FString> Saved = Stats.SaveText()) FTCSettingsStore::Game().Set(TEXT("ww2-stats"), *Saved);
	return R != FTCStatsSheet::EPress::None;
}

void ATCPlayerController::OpenStatsSheet()
{
	ATCGameMode* M = Mode();
	if (!M || M->StatsSheet.IsOpen()) return;
	// beforeOpen: cancelInput(); pointer.release() (any drag, gesture or aim in progress is dropped)
	CancelInput();
	M->StatsSheet.Open();
	UE_LOG(LogTC, Log, TEXT("stats sheet open (%s); Close, Esc or a click outside closes it"), *M->StatsOverlay().MenuLabel());
}

bool ATCPlayerController::ModalSheetOpen() const
{
	const ATCGameMode* M = GetWorld() ? GetWorld()->GetAuthGameMode<ATCGameMode>() : nullptr;
	return bControlsSheetOpen || (M && M->StatsSheet.IsOpen());
}
#endif // TC_HAS_STATS_GLUE

// ---------------------------------------------------------------------------------------------------------------------
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

// ThreeCrossroads.Stats.Sheet: the sheet's rows, controls and handlers against client/stats.js and index.html at the
// pin, its layout against stats.css (width, columns, gaps, the narrow media rule, centring, max-height), Spanish
// through client/es.js, and the probe's line. Run:
//   UnrealEditor-Cmd ThreeCrossroads.uproject -nullrhi -ExecCmds="Automation RunTests ThreeCrossroads.Stats; Quit"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCStatsSheetTest, "ThreeCrossroads.Stats.Sheet", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCStatsSheetTest::RunTest(const FString& Parameters)
{
	using namespace TCStatsSheetRules;
	const FTCMeasure M = FTCPaint::ApproxMeasure();
	auto TagsOf = [](const FTCPaint& P, const FString& Prefix) { TArray<FString> T; for (const FTCPaintHit& H : P.Hits) if (H.Tag.StartsWith(Prefix)) T.Add(H.Tag.Mid(Prefix.Len())); return T; };
	auto HitOf = [](const FTCPaint& P, const FString& Tag) { for (const FTCPaintHit& H : P.Hits) if (H.Tag == Tag) return H.Rect; return FBox2D(ForceInit); };

	// the rows: stats.js STATS (19 in three groups, labels and hints), PRESETS, CHOICES and index.html's texts
	TestEqual(TEXT("groups"), FString::Join(FTCStatsSheet::Groups(), TEXT(",")), FString(TEXT("Performance,Network,Game")));
	FTCStatsOverlay O;
	O.Load(TOptional<FString>());
	FTCPaint P = FTCStatsSheet().Layout(O.Settings(), 1280, 720, M, FVector2D(-1, -1), ETCLanguage::English);
	TestEqual(TEXT("one checkbox per stat, in STATS order"), FString::Join(TagsOf(P, TEXT("stats:pick:")), TEXT(",")),
		FString(TEXT("fps,frame,worst,cpu,calls,tris,scale,mem,ping,jitter,loss,snaps,gap,size,kbs,server,units,fx,corpses")));
	TestEqual(TEXT("legends"), FString::Join(P.TextsOf(TEXT("stats:legend")), TEXT(",")), FString(TEXT("Performance,Network,Game")));
	TestEqual(TEXT("labels"), FString::Join(P.TextsOf(TEXT("stats:label")), TEXT("|")), FString(TEXT("Frame rate|Frame time|Worst frame|Script time|Draw calls|Triangles|Render scale and graphics|Memory|Ping|Jitter|Loss|Updates per second|Longest update gap|Update size|Download rate|Server tick|Units|Effect particles|Bodies")));
	TestEqual(TEXT("hints (wrapped lines joined)"), FString::Join(P.TextsOf(TEXT("stats:hint")), TEXT(" ")),
		FString(TEXT("Average over the last second Longest in the last second: hitches Game code per frame Chrome and Edge only Round trip to the server How much the ping swings Pings unanswered after 3 s, last minute Game state messages received Longest wait in the last second: stalls Slowest 5% of ticks that send an update; over 40 ms it sends fewer")));
	TestEqual(TEXT("presets"), FString::Join(TagsOf(P, TEXT("stats:preset:")), TEXT(",")), FString(TEXT("Basic,Performance,Network,Everything")));
	TestEqual(TEXT("selects"), FString::Join(TagsOf(P, TEXT("stats:select:")), TEXT(",")), FString(TEXT("pos,layout,size,bg")));
	TestEqual(TEXT("select labels"), FString::Join(P.TextsOf(TEXT("stats:look-label")), TEXT(",")), FString(TEXT("Position,Layout,Size,Background")));
	TestEqual(TEXT("select values (DEFAULTS)"), FString::Join(P.TextsOf(TEXT("stats:select-value")), TEXT(",")), FString(TEXT("Top left,List,Medium,Panel")));
	TestEqual(TEXT("title"), FString::Join(P.TextsOf(TEXT("stats:title")), TEXT("")), FString(TEXT("Stats overlay")));
	TestEqual(TEXT("key caps"), FString::Join(P.TextsOf(TEXT("stats:kbd")), TEXT(",")), FString(TEXT("Esc,F2")));
	TestEqual(TEXT("switches"), FString::Join(P.TextsOf(TEXT("stats:show")), TEXT("")) + TEXT("|") + FString::Join(P.TextsOf(TEXT("stats:warn")), TEXT(" ")), FString(TEXT("Show the overlay|Color warnings: amber, then red, when a number is bad")));
	TestEqual(TEXT("h3"), FString::Join(P.TextsOf(TEXT("stats:h3")), TEXT("")), FString(TEXT("Look")));
	TestEqual(TEXT("the menu's title"), FString(FTCStatsSheet::MenuTitle()), FString(TEXT("Frame rate, ping, loss and other numbers over the game (F2 shows or hides)")));
	TestEqual(TEXT("the choices' texts"), FTCStatsSheet::ChoiceText(TEXT("bg"), TEXT("clear")) + TEXT("|") + FTCStatsSheet::ChoiceText(TEXT("layout"), TEXT("line")) + TEXT("|") + FTCStatsSheet::ChoiceText(TEXT("pos"), TEXT("tc")), FString(TEXT("See-through|One line|Top center")));

	// stats.css geometry at 1280 x 720: 700 px wide, centred, the backdrop over the window, three pick columns of
	// (700 - 2 - 32 - 40) / 3 and four Look columns of (666 - 36) / 4
	const FBox2D Box = FTCStatsSheet::DialogBox(P);
	TestTrue(TEXT("700 px wide"), FMath::IsNearlyEqual(Box.GetSize().X, 700.f));
	TestTrue(TEXT("centred across"), FMath::IsNearlyEqual(Box.Min.X, 290.f));
	TestTrue(TEXT("centred down"), FMath::IsNearlyEqual(Box.Min.Y + Box.GetSize().Y / 2, 360.f, 0.6f));
	TestTrue(TEXT("within max-height min(86vh, 720px)"), Box.GetSize().Y <= 720 * 0.86f + 0.01f);
	TestTrue(TEXT("the backdrop is rgba(8, 10, 11, 0.3) over the window"), P.Items[0].Size == FVector2D(1280, 720) && FMath::IsNearlyEqual(P.Items[0].Colour.A, 0.3f));
	const float ColW = (700.f - 2.f - 32.f - 40.f) / 3.f;
	const FBox2D Fps = HitOf(P, TEXT("stats:pick:fps")), Ping = HitOf(P, TEXT("stats:pick:ping")), Units = HitOf(P, TEXT("stats:pick:units"));
	TestTrue(TEXT("Performance in column 1"), FMath::IsNearlyEqual(Fps.Min.X, 290.f + 17.f) && FMath::IsNearlyEqual(Fps.GetSize().X, ColW, 0.01f));
	TestTrue(TEXT("Network in column 2"), FMath::IsNearlyEqual(Ping.Min.X, 290.f + 17.f + ColW + 20.f, 0.01f) && FMath::IsNearlyEqual(Ping.Min.Y, Fps.Min.Y));
	TestTrue(TEXT("Game in column 3"), FMath::IsNearlyEqual(Units.Min.X, 290.f + 17.f + 2 * (ColW + 20.f), 0.01f) && FMath::IsNearlyEqual(Units.Min.Y, Fps.Min.Y));
	const FBox2D Pos = HitOf(P, TEXT("stats:select:pos")), Bg = HitOf(P, TEXT("stats:select:bg"));
	TestTrue(TEXT("four Look columns in one row"), FMath::IsNearlyEqual(Pos.GetSize().X, (666.f - 36.f) / 4, 0.01f) && FMath::IsNearlyEqual(Bg.Min.Y, Pos.Min.Y) && FMath::IsNearlyEqual(Bg.Max.X, 290.f + 700.f - 17.f, 0.01f));
	const FBox2D Close = HitOf(P, TEXT("stats:close"));
	TestTrue(TEXT("Close at the header's right"), FMath::IsNearlyEqual(Close.Max.X, 290.f + 700.f - 17.f, 0.01f) && Close.Min.Y > Box.Min.Y && Close.Max.Y < Fps.Min.Y);
	TestTrue(TEXT("the warnings switch last"), HitOf(P, TEXT("stats:warn")).Min.Y > Pos.Max.Y);

	// @media (max-width: 640px): one column of picks, two of Look, width 100vw - 24px
	const FTCPaint N = FTCStatsSheet().Layout(O.Settings(), 600, 1400, M, FVector2D(-1, -1), ETCLanguage::English);
	const FBox2D NBox = FTCStatsSheet::DialogBox(N);
	TestTrue(TEXT("narrow: 576 px wide"), FMath::IsNearlyEqual(NBox.GetSize().X, 576.f));
	TestTrue(TEXT("narrow: one pick column"), FMath::IsNearlyEqual(HitOf(N, TEXT("stats:pick:ping")).Min.X, HitOf(N, TEXT("stats:pick:fps")).Min.X) && HitOf(N, TEXT("stats:pick:ping")).Min.Y > HitOf(N, TEXT("stats:pick:mem")).Max.Y);
	// one column is taller than max-height (720 px): the sheet scrolls; at the end of its scroll the Look grid shows
	FTCStatsSheet Narrow;
	Narrow.Open();
	Narrow.Remember(Narrow.Layout(O.Settings(), 600, 1400, M, FVector2D(-1, -1), ETCLanguage::English));
	TestTrue(TEXT("narrow: max-height 720 px"), FMath::IsNearlyEqual(NBox.GetSize().Y, 720.f));
	TestFalse(TEXT("narrow: the Look grid is below the fold at first"), N.Hits.ContainsByPredicate([](const FTCPaintHit& H) { return H.Tag == TEXT("stats:select:size"); }));
	Narrow.Scroll(10000.f);
	const FTCPaint NS = Narrow.Layout(O.Settings(), 600, 1400, M, FVector2D(-1, -1), ETCLanguage::English);
	TestTrue(TEXT("narrow: scrolled to the end"), Narrow.ScrollOffset() > 0.f);
	TestTrue(TEXT("narrow: two Look columns"), FMath::IsNearlyEqual(HitOf(NS, TEXT("stats:select:size")).Min.X, HitOf(NS, TEXT("stats:select:pos")).Min.X) && HitOf(NS, TEXT("stats:select:size")).Min.Y > HitOf(NS, TEXT("stats:select:pos")).Max.Y);
	TestTrue(TEXT("narrow: the header stays"), HitOf(NS, TEXT("stats:close")).bIsValid && FMath::IsNearlyEqual(HitOf(NS, TEXT("stats:close")).Min.Y, HitOf(N, TEXT("stats:close")).Min.Y));
	TestTrue(TEXT("narrow: the warnings switch shows at the end"), HitOf(NS, TEXT("stats:warn")).bIsValid && HitOf(NS, TEXT("stats:warn")).Max.Y <= NBox.Max.Y);
	// a short window: max-height cuts the sheet (the browser scrolls it); nothing is drawn or hit below the dialog
	const FTCPaint Short = FTCStatsSheet().Layout(O.Settings(), 1280, 400, M, FVector2D(-1, -1), ETCLanguage::English);
	const FBox2D SBox = FTCStatsSheet::DialogBox(Short);
	TestTrue(TEXT("short: 86vh"), FMath::IsNearlyEqual(SBox.GetSize().Y, 344.f, 0.01f));
	bool bInside = true;
	for (const FTCPaintHit& H : Short.Hits) bInside &= H.Rect.Max.Y <= SBox.Max.Y + 0.5f;
	TestTrue(TEXT("short: every hit inside the dialog"), bInside && !Short.Hits.ContainsByPredicate([](const FTCPaintHit& H) { return H.Tag == TEXT("stats:warn"); }));

	// the handlers (stats.js): a pick re-sorts in STATS order and shows; unpicking everything does not hide; presets
	FTCStatsSheet Sheet;
	TestTrue(TEXT("open"), Sheet.Open());
	TestFalse(TEXT("open() while open does nothing"), Sheet.Open());
	TestEqual(TEXT("show"), (int32)Sheet.Press(TEXT("stats:show"), false, O), (int32)FTCStatsSheet::EPress::Changed);
	TestTrue(TEXT("show on"), O.Settings().bShow);
	Sheet.Press(TEXT("stats:show"), false, O);
	Sheet.Press(TEXT("stats:pick:worst"), false, O);
	TestEqual(TEXT("worst added in STATS order"), FString::Join(O.Settings().Stats, TEXT(",")), FString(TEXT("fps,worst,ping,loss")));
	TestTrue(TEXT("a pick shows the overlay"), O.Settings().bShow);
	for (const TCHAR* Id : { TEXT("fps"), TEXT("worst"), TEXT("ping"), TEXT("loss") }) Sheet.Press(FString(TEXT("stats:pick:")) + Id, false, O);
	TestEqual(TEXT("nothing picked"), O.Settings().Stats.Num(), 0);
	TestTrue(TEXT("unpicking all keeps it shown (only a non-empty pick sets show)"), O.Settings().bShow);
	Sheet.Press(TEXT("stats:show"), false, O);
	Sheet.Press(TEXT("stats:preset:Network"), false, O);
	TestEqual(TEXT("Network preset"), FString::Join(O.Settings().Stats, TEXT(",")), FString(TEXT("ping,jitter,loss,snaps,gap,kbs,server")));
	TestTrue(TEXT("a preset shows"), O.Settings().bShow);
	Sheet.Press(TEXT("stats:select:pos"), false, O);
	TestEqual(TEXT("Position next"), O.Settings().Pos, FString(TEXT("tc")));
	Sheet.Press(TEXT("stats:select:pos"), true, O); Sheet.Press(TEXT("stats:select:pos"), true, O);
	TestEqual(TEXT("Position back twice wraps"), O.Settings().Pos, FString(TEXT("tr")));
	Sheet.Press(TEXT("stats:select:bg"), false, O);
	Sheet.Press(TEXT("stats:select:layout"), false, O);
	Sheet.Press(TEXT("stats:select:size"), true, O);
	Sheet.Press(TEXT("stats:warn"), false, O);
	TestEqual(TEXT("saved as JSON.stringify(settings)"), O.SaveText().Get(FString()),
		FString(TEXT("{\"show\":true,\"stats\":[\"ping\",\"jitter\",\"loss\",\"snaps\",\"gap\",\"kbs\",\"server\"],\"pos\":\"tr\",\"layout\":\"line\",\"size\":\"s\",\"bg\":\"clear\",\"warn\":false}")));
	FTCStatsOverlay Back;
	Back.Load(O.SaveText());
	TestEqual(TEXT("load(save) round trip"), Back.SaveText().Get(FString()), O.SaveText().Get(FString()));
	TestEqual(TEXT("menu label"), O.MenuLabel(), FString(TEXT("Stats overlay: On")));
	// clicks by pixel: a control, inside the sheet, the backdrop
	const FTCPaint L = Sheet.Layout(O.Settings(), 1280, 720, M, FVector2D(-1, -1), ETCLanguage::English);
	Sheet.Remember(L);
	FString Tag;
	TestEqual(TEXT("a click on Basic"), (int32)Sheet.PressAt(HitOf(L, TEXT("stats:preset:Basic")).GetCenter(), false, O, &Tag), (int32)FTCStatsSheet::EPress::Changed);
	TestEqual(TEXT("Basic"), FString::Join(O.Settings().Stats, TEXT(",")), FString(TEXT("fps,ping,loss")));
	TestEqual(TEXT("a click on the sheet's padding does nothing"), (int32)Sheet.PressAt(FTCStatsSheet::DialogBox(L).Min + FVector2D(3, 3), false, O, &Tag), (int32)FTCStatsSheet::EPress::None);
	TestEqual(TEXT("its tag"), Tag, FString(TEXT("stats:sheet")));
	TestEqual(TEXT("a click on the backdrop closes"), (int32)Sheet.PressAt(FVector2D(10, 10), false, O, &Tag), (int32)FTCStatsSheet::EPress::Closed);
	TestFalse(TEXT("closed"), Sheet.IsOpen());
	TestEqual(TEXT("closed: a press does nothing"), (int32)Sheet.PressAt(FVector2D(10, 10), false, O, &Tag), (int32)FTCStatsSheet::EPress::None);
	Sheet.Open();
	TestEqual(TEXT("Close"), (int32)Sheet.Press(TEXT("stats:close"), false, O), (int32)FTCStatsSheet::EPress::Closed);
	// -TCPerf ('?perf'): every number, nothing saved
	FTCStatsOverlay Forced;
	Forced.Load(TOptional<FString>(TEXT("{\"show\":false}")), true);
	Sheet.Open();
	Sheet.Press(TEXT("stats:warn"), false, Forced);
	TestFalse(TEXT("forced: never saved"), Forced.SaveText().IsSet());

	// Spanish: client/es.js through the shared table (what the page's observer shows)
	if (FTCLocale::IsLoaded())
	{
		const FTCPaint Es = FTCStatsSheet().Layout(O.Settings(), 1280, 720, M, FVector2D(-1, -1), ETCLanguage::Spanish);
		TestEqual(TEXT("es: title"), FString::Join(Es.TextsOf(TEXT("stats:title")), TEXT("")), FString(TEXT("Panel de estadísticas")));
		TestEqual(TEXT("es: legends"), FString::Join(Es.TextsOf(TEXT("stats:legend")), TEXT(",")), FString(TEXT("Rendimiento,Red,Juego")));
		TestEqual(TEXT("es: Look"), FString::Join(Es.TextsOf(TEXT("stats:h3")), TEXT("")) + TEXT(",") + FString::Join(Es.TextsOf(TEXT("stats:look-label")), TEXT(",")), FString(TEXT("Aspecto,Posición,Disposición,Tamaño,Fondo")));
		TestTrue(TEXT("es: first label"), Es.TextsOf(TEXT("stats:label")).Num() > 0 && Es.TextsOf(TEXT("stats:label"))[0] == TEXT("Fotogramas por segundo"));
		TestEqual(TEXT("es: menu label"), FTCLocale::Translate(O.MenuLabel(), ETCLanguage::Spanish), FString(TEXT("Panel de estadísticas: Sí")));
	}
	else AddWarning(TEXT("the shared Spanish table did not load: Spanish not checked"));

	// the probe's line, and its roll detection (nothing before 'stat unit' shows or before the first roll)
	FTCPerfStats Pf;
	Pf.Fps = 59.9; Pf.Ms = 16.7; Pf.Worst = 21.3; Pf.Cpu = 4.2;
	TestEqual(TEXT("probe line"), FTCStatUnitProbe::Line(Pf, 16.71, 4.33, 6.02, 1.2, 9.5, 16.7, 21.25, 60),
		FString(TEXT("stats probe: overlay FPS 59.9, Frame 16.7 ms, Worst 21.3 ms, JS 4.2 ms | stat unit Frame 16.7 ms, Game 4.3 ms, Draw 6.0 ms, RHIT 1.2 ms, GPU 9.5 ms | stat unit raw frames over the same 60 frames: mean 16.7 ms (59.9 fps), worst 21.3 ms")));
	FTCStatUnitProbe Probe;
	TestTrue(TEXT("probe: nothing without stat unit"), Probe.Frame(Pf, nullptr, false).IsEmpty());
	return true;
}
#endif
