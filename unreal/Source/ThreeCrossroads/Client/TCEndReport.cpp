#include "Client/TCEndReport.h"
#include "Client/TCStats.h"
#include "Client/TCLocale.h"
#include "Net/TCMatchState.h"
#include "Engine/Canvas.h"
#include "Engine/Font.h"
#include "CanvasItem.h"
#include "GlobalRenderResources.h"

// ---------------------------------------------------------------------------------------------------------------------
// CSS colours and timing curves

FLinearColor TCCss::Rgba(int32 R, int32 G, int32 B, float A)
{
	FLinearColor C(FColor((uint8)R, (uint8)G, (uint8)B));
	C.A = A;
	return C;
}
FLinearColor TCCss::Hex(uint32 Rgb, float A) { return Rgba((Rgb >> 16) & 255, (Rgb >> 8) & 255, Rgb & 255, A); }
const FLinearColor TCCss::Text = TCCss::Hex(0xe2dfd3), TCCss::Dim = TCCss::Hex(0xa6a292), TCCss::Brass = TCCss::Hex(0xd6b25e), TCCss::RedHi = TCCss::Hex(0xee8a7b);
const FLinearColor TCCss::Line = TCCss::Rgba(176, 164, 122, 0.46f), TCCss::LineSoft = TCCss::Rgba(176, 164, 122, 0.24f);
const FLinearColor TCCss::PanelSolid = TCCss::Rgba(25, 28, 30, 0.96f), TCCss::Panel = TCCss::Rgba(25, 28, 30, 0.86f);

FLinearColor TCCss::PlayerColour(int32 Slot)
{
	// client/main.js COLORS = [0x3b73d6, 0xcc3a2e, 0xece6d6, 0xe2832b, 0x9b5cd4, 0x35b6c0]; report.js colors[i] ?? '#888'
	static const uint32 Colours[] = { 0x3b73d6, 0xcc3a2e, 0xece6d6, 0xe2832b, 0x9b5cd4, 0x35b6c0 };
	return Slot >= 0 && Slot < 6 ? Hex(Colours[Slot]) : Hex(0x888888);
}

namespace
{
	// a CSS cubic-bezier(x1, y1, x2, y2) timing function at progress T (Newton steps on x, then y)
	float Bezier(float T, float X1, float Y1, float X2, float Y2)
	{
		T = FMath::Clamp(T, 0.f, 1.f);
		auto Coord = [](float S, float A, float B) { const float U = 1 - S; return 3 * U * U * S * A + 3 * U * S * S * B + S * S * S; };
		auto Slope = [](float S, float A, float B) { const float U = 1 - S; return 3 * U * U * A + 6 * U * S * (B - A) + 3 * S * S * (1 - B); };
		float S = T;
		for (int32 I = 0; I < 8; I++)
		{
			const float D = Slope(S, X1, X2);
			if (FMath::Abs(D) < 1e-6f) break;
			S = FMath::Clamp(S - (Coord(S, X1, X2) - T) / D, 0.f, 1.f);
		}
		return Coord(S, Y1, Y2);
	}
}
float TCCss::Ease(float T) { return Bezier(T, 0.25f, 0.1f, 0.25f, 1.f); }
float TCCss::EaseOut(float T) { return Bezier(T, 0.f, 0.f, 0.58f, 1.f); }
float TCCss::EaseInOut(float T) { return Bezier(T, 0.42f, 0.f, 0.58f, 1.f); }

// ---------------------------------------------------------------------------------------------------------------------
// FTCPaint

FTCPaintItem& FTCPaint::Rect(float X, float Y, float W, float H, const FLinearColor& C, const TCHAR* Role)
{
	FTCPaintItem& I = Items.AddDefaulted_GetRef();
	I.Kind = ETCPaintKind::Rect; I.Pos = FVector2D(X, Y); I.Size = FVector2D(W, H); I.Colour = C; I.Role = Role;
	return I;
}
FTCPaintItem& FTCPaint::Gradient(float X, float Y, float W, float H, const FLinearColor& Top, const FLinearColor& Bottom, const TCHAR* Role)
{
	FTCPaintItem& I = Items.AddDefaulted_GetRef();
	I.Kind = ETCPaintKind::Gradient; I.Pos = FVector2D(X, Y); I.Size = FVector2D(W, H); I.Colour = Top; I.Colour2 = Bottom; I.Role = Role;
	return I;
}
FTCPaintItem& FTCPaint::Frame(float X, float Y, float W, float H, const FLinearColor& C, float Thickness, const TCHAR* Role)
{
	FTCPaintItem& I = Items.AddDefaulted_GetRef();
	I.Kind = ETCPaintKind::Frame; I.Pos = FVector2D(X, Y); I.Size = FVector2D(W, H); I.Colour = C; I.Thickness = Thickness; I.Role = Role;
	return I;
}
FTCPaintItem& FTCPaint::Line(FVector2D A, FVector2D B, const FLinearColor& C, float Thickness, const TCHAR* Role)
{
	FTCPaintItem& I = Items.AddDefaulted_GetRef();
	I.Kind = ETCPaintKind::Line; I.Pos = A; I.Size = B; I.Colour = C; I.Thickness = Thickness; I.Role = Role;
	return I;
}
FTCPaintItem& FTCPaint::Text(const FString& Shown, float X, float Y, float FontPx, const FLinearColor& C, bool bBold, ETCPaintAlign Align, const TCHAR* Role)
{
	FTCPaintItem& I = Items.AddDefaulted_GetRef();
	I.Kind = ETCPaintKind::Text; I.Pos = FVector2D(X, Y); I.Text = TC_T(Shown); I.FontPx = FontPx; I.Colour = C; I.bBold = bBold; I.Align = Align; I.Role = Role;
	I.Size = FVector2D(0, FontPx * 1.35f); // the line box (CSS line-height); a caller sets its own
	return I;
}

void FTCPaint::Fade(int32 First, float Alpha)
{
	for (int32 I = FMath::Max(0, First); I < Items.Num(); I++) { Items[I].Colour.A *= Alpha; Items[I].Colour2.A *= Alpha; }
}
void FTCPaint::Shift(int32 First, FVector2D By)
{
	for (int32 I = FMath::Max(0, First); I < Items.Num(); I++)
	{
		Items[I].Pos += By;
		if (Items[I].Clip.bIsValid) Items[I].Clip = FBox2D(Items[I].Clip.Min + By, Items[I].Clip.Max + By);
		if (Items[I].Kind == ETCPaintKind::Line) Items[I].Size += By;
	}
}
void FTCPaint::Append(const FTCPaint& Other)
{
	Items.Append(Other.Items); Hits.Append(Other.Hits); Panels.Append(Other.Panels);
}
TArray<FString> FTCPaint::TextsOf(const FString& Role) const
{
	TArray<FString> Out;
	for (const FTCPaintItem& I : Items) if (I.Kind == ETCPaintKind::Text && I.Role == Role) Out.Add(I.Text);
	return Out;
}
const FTCPaintItem* FTCPaint::Find(const FString& Role) const
{
	return Items.FindByPredicate([&Role](const FTCPaintItem& I) { return I.Role == Role; });
}

FTCMeasure FTCPaint::ApproxMeasure()
{
	return [](const FString& T, float Px, bool bBold, float Tracking) { return T.Len() * (Px * (bBold ? 0.55f : 0.5f) + Tracking); };
}

namespace
{
	// The HUD font's glyph box at scale 1, and the scale that draws a CSS font size with it. CSS font-size is the em
	// box; the engine's max char height is about 1.2 em for its Roboto faces.
	float FontScale(UFont* Font, float Px)
	{
		const float H = Font ? FMath::Max(1.f, Font->GetMaxCharHeight()) : 12.f;
		return Px * 1.2f / H;
	}
}

FTCMeasure FTCPaint::CanvasMeasure(UFont* Font)
{
	if (FTCType::IsLoaded()) return FTCType::Measure();
	return [Font](const FString& T, float Px, bool bBold, float Tracking)
	{
		if (!Font || T.IsEmpty()) return 0.f;
		int32 W = 0, H = 0;
		Font->GetStringHeightAndWidth(T, H, W);
		return W * FontScale(Font, Px) + (bBold ? 0.6f : 0.f) + T.Len() * Tracking;
	};
}

FString FTCPaint::Ellipsis(const FString& Text, float MaxW, float FontPx, bool bBold, const FTCMeasure& Measure)
{
	if (Measure(Text, FontPx, bBold, 0.f) <= MaxW) return Text;
	const FString Dots = TEXT("…");
	int32 Lo = 0, Hi = Text.Len();
	while (Lo < Hi) // the longest head that fits with the ellipsis
	{
		const int32 Mid = (Lo + Hi + 1) / 2;
		if (Measure(Text.Left(Mid) + Dots, FontPx, bBold, 0.f) <= MaxW) Lo = Mid; else Hi = Mid - 1;
	}
	return Text.Left(Lo) + Dots;
}

TArray<FString> FTCPaint::Wrap(const FString& Text, float MaxW, float FontPx, bool bBold, const FTCMeasure& Measure)
{
	TArray<FString> Words, Out;
	Text.ParseIntoArray(Words, TEXT(" "), true);
	FString Cur;
	for (FString W : Words)
	{
		const FString Try = Cur.IsEmpty() ? W : Cur + TEXT(" ") + W;
		if (Measure(Try, FontPx, bBold, 0.f) <= MaxW || (Cur.IsEmpty() && Measure(W, FontPx, bBold, 0.f) <= MaxW)) { Cur = Try; continue; }
		if (!Cur.IsEmpty()) { Out.Add(Cur); Cur.Reset(); }
		// overflow-wrap: anywhere: a word wider than the line breaks where it must
		while (Measure(W, FontPx, bBold, 0.f) > MaxW && W.Len() > 1)
		{
			int32 N = W.Len() - 1;
			while (N > 1 && Measure(W.Left(N), FontPx, bBold, 0.f) > MaxW) N--;
			Out.Add(W.Left(N)); W.RightChopInline(N);
		}
		Cur = W;
	}
	if (!Cur.IsEmpty() || !Out.Num()) Out.Add(Cur);
	return Out;
}

void FTCPaint::Draw(UCanvas* Canvas, UFont* Font) const
{
	if (!Canvas) return;
	// FTCType sends non-text items back here. Only redirect a draw list that actually contains text.
	if (FTCType::IsLoaded() && Items.ContainsByPredicate([](const FTCPaintItem& I) { return I.Kind == ETCPaintKind::Text; }))
	{ FTCType::Draw(*this, Canvas, Font); return; }
	for (const FTCPaintItem& I : Items)
	{
		if (I.Colour.A <= 0.f && (I.Kind != ETCPaintKind::Gradient || I.Colour2.A <= 0.f)) continue;
		switch (I.Kind)
		{
		case ETCPaintKind::Rect:
		{
            FVector2D Lo=I.Pos, Hi=I.Pos+I.Size;
            if (I.Clip.bIsValid) { Lo.X=FMath::Max(Lo.X,I.Clip.Min.X); Lo.Y=FMath::Max(Lo.Y,I.Clip.Min.Y); Hi.X=FMath::Min(Hi.X,I.Clip.Max.X); Hi.Y=FMath::Min(Hi.Y,I.Clip.Max.Y); }
            if (Hi.X<=Lo.X || Hi.Y<=Lo.Y) break;
            FCanvasTileItem T(Lo, GWhiteTexture, Hi-Lo, I.Colour);
			T.BlendMode = SE_BLEND_Translucent;
			Canvas->DrawItem(T);
			break;
		}
		case ETCPaintKind::Gradient:
		{
			// a vertical linear-gradient, one strip per pixel row
			const int32 Rows = FMath::Max(1, FMath::RoundToInt(I.Size.Y));
			for (int32 R = 0; R < Rows; R++)
			{
				const float K = Rows > 1 ? (R + 0.5f) / Rows : 0.5f;
				FCanvasTileItem T(FVector2D(I.Pos.X, I.Pos.Y + I.Size.Y * R / Rows), GWhiteTexture, FVector2D(I.Size.X, I.Size.Y / Rows), FMath::Lerp(I.Colour, I.Colour2, K));
				T.BlendMode = SE_BLEND_Translucent;
				Canvas->DrawItem(T);
			}
			break;
		}
		case ETCPaintKind::Frame:
		{
			const float T = I.Thickness;
			const FVector2D P = I.Pos, S = I.Size;
			const FBox2D Sides[] = { FBox2D(P, P + FVector2D(S.X, T)), FBox2D(P + FVector2D(0, S.Y - T), P + S), FBox2D(P + FVector2D(0, T), P + FVector2D(T, S.Y - T)), FBox2D(P + FVector2D(S.X - T, T), P + FVector2D(S.X, S.Y - T)) };
            for (FBox2D B : Sides)
            {
                if (I.Clip.bIsValid) B=B.Overlap(I.Clip);
                if (!B.bIsValid || B.GetSize().X<=0 || B.GetSize().Y<=0) continue;
                FCanvasTileItem Tile(B.Min, GWhiteTexture, B.GetSize(), I.Colour);
				Tile.BlendMode = SE_BLEND_Translucent;
				Canvas->DrawItem(Tile);
			}
			break;
		}
		case ETCPaintKind::Line:
		{
			// an axis-aligned line as a tile (the canvas's line batch draws without the colour's alpha)
			if (I.Pos.Y == I.Size.Y || I.Pos.X == I.Size.X)
			{
				const FVector2D Lo(FMath::Min(I.Pos.X, I.Size.X), FMath::Min(I.Pos.Y, I.Size.Y)), Hi(FMath::Max(I.Pos.X, I.Size.X), FMath::Max(I.Pos.Y, I.Size.Y));
				const FVector2D From = I.Pos.Y == I.Size.Y ? FVector2D(Lo.X, Lo.Y - I.Thickness / 2) : FVector2D(Lo.X - I.Thickness / 2, Lo.Y);
				const FVector2D Size = I.Pos.Y == I.Size.Y ? FVector2D(Hi.X - Lo.X, I.Thickness) : FVector2D(I.Thickness, Hi.Y - Lo.Y);
				FCanvasTileItem T(From, GWhiteTexture, Size, I.Colour);
				T.BlendMode = SE_BLEND_Translucent;
				Canvas->DrawItem(T);
				break;
			}
			FCanvasLineItem L(I.Pos, I.Size);
			L.SetColor(I.Colour);
			L.LineThickness = I.Thickness;
			Canvas->DrawItem(L);
			break;
		}
		case ETCPaintKind::Text:
		{
			if (!Font || I.Text.IsEmpty()) break;
			const float Scale = FontScale(Font, I.FontPx);
			const FTCMeasure M = CanvasMeasure(Font);
			const float W = M(I.Text, I.FontPx, I.bBold, I.Tracking);
			float X = I.Pos.X - (I.Align == ETCPaintAlign::Centre ? W / 2 : I.Align == ETCPaintAlign::Right ? W : 0.f);
			// the glyph box (1.2 em) centred in the line box, as CSS centres the text in its line-height
			const float GlyphH = I.FontPx * 1.2f, LineH = I.Size.Y > 0 ? I.Size.Y : GlyphH;
			const float Y = I.Pos.Y + (LineH - GlyphH) / 2;
			auto Put = [&](const FString& S, float Px)
			{
				FCanvasTextItem T(FVector2D(Px, Y), FText::FromString(S), Font, I.Colour);
				T.Scale = FVector2D(Scale, Scale);
				T.BlendMode = SE_BLEND_Translucent;
				Canvas->DrawItem(T);
				if (I.bBold) { T.Position.X += 0.6f; Canvas->DrawItem(T); } // font-weight 600 and 700: the one HUD face, doubled
			};
			if (I.Tracking <= 0.f) Put(I.Text, X);
			else
				for (int32 C = 0; C < I.Text.Len(); C++)
				{
					const FString Ch = I.Text.Mid(C, 1);
					Put(Ch, X);
					X += M(Ch, I.FontPx, I.bBold, I.Tracking);
				}
			break;
		}
		}
	}
}

// ---------------------------------------------------------------------------------------------------------------------
// FTCEndReport

FString FTCEndReport::KeyOf(const FTCMatchResult& R, int32 Me)
{
	// report.js: JSON.stringify([r.endTick, r.names, r.you, colors, me]) (the colours never change here)
	return FString::Printf(TEXT("%d|%s|%s:%s:%d|%d"), R.EndTick, *FString::Join(R.Names, TEXT("\x1f")), R.bHasYou ? TEXT("you") : TEXT("null"), *R.YouOutcome, R.YouTeam, Me);
}

FTCEndReportInput FTCEndReport::FromState(const FTCMatchState& S)
{
	FTCEndReportInput In;
	In.bResult = S.LastMatch.bPresent;
	if (In.bResult) In.ResultLine = FTCObjectives::ResultLine(S.LastMatch, S.LobbySeat);
	In.Report = S.Objectives.Report();
	In.Key = In.Report.bShown ? KeyOf(S.LastMatch, S.LobbySeat) : FString();
	return In;
}

void FTCEndReport::Sync(const FTCEndReportInput& In)
{
	// report.js: a hidden report clears the key; a new key builds the card again, open
	if (!In.Report.bShown) { OpenKey.Reset(); return; }
	if (In.Key != OpenKey) { OpenKey = In.Key; bOpen = true; }
}

void FTCEndReport::Chart(FTCPaint& P, const FTCMatchReport& R, float X0, float Y0, float CW, float H, const FTCMeasure& Measure)
{
	using namespace TCEndReportRules;
	// report.js draw: the strip, three grid lines, the labels, then the sides (the winners last, so on top)
	const float W = FMath::Max(ChartMinW, CW);
	P.Rect(X0, Y0, W, H, TCCss::Hex(0x15181a), TEXT("report:chart"));
	const double Top = R.Top > 0 ? R.Top : 1.0, End = FMath::Max(1.0, R.End);
	const float L = FMath::CeilToFloat(Measure(R.TopLabel, ChartFontPx, false, 0.f)) + ChartL, Pw = W - L - ChartR, Ph = H - ChartT - ChartB;
	auto Xof = [&](double T) { return X0 + L + (float)(T / End) * Pw; };
	auto Yof = [&](double V) { return Y0 + ChartT + Ph - (float)(V / Top) * Ph; };
	for (double V : { 0.0, Top / 2, Top })
	{
		const float Y = FMath::RoundToFloat(Yof(V) - Y0) + 0.5f + Y0;
		P.Line(FVector2D(X0 + L, Y), FVector2D(X0 + L + Pw, Y), TCCss::Rgba(226, 223, 211, 0.14f), 1.f, TEXT("report:grid"));
	}
	const FLinearColor Light = TCCss::Hex(0xe2dfd3);
	// canvas text, not translated: textBaseline 'middle' for the values, 'alphabetic' for the times
	auto Middle = [&](const FString& S, float X, float Y, ETCPaintAlign A) { FTCPaintItem& I = P.Text(S, X, Y - ChartFontPx * 0.6f, ChartFontPx, Light, false, A, TEXT("report:axis")); I.Size.Y = ChartFontPx * 1.2f; };
	auto Baseline = [&](const FString& S, float X, float Y, ETCPaintAlign A) { FTCPaintItem& I = P.Text(S, X, Y - ChartFontPx * 0.95f, ChartFontPx, Light, false, A, TEXT("report:axis")); I.Size.Y = ChartFontPx * 1.2f; };
	Middle(TEXT("0"), X0 + L - 6, Yof(0), ETCPaintAlign::Right);
	Middle(R.TopLabel, X0 + L - 6, Yof(Top) + 2, ETCPaintAlign::Right);
	Baseline(TEXT("0 min"), X0 + L, Y0 + H - 6, ETCPaintAlign::Left);
	Baseline(R.EndLabel, X0 + L + Pw, Y0 + H - 6, ETCPaintAlign::Right);
	for (const FTCReportSide& S : R.Sides) // FTCMatchReport keeps the sides in drawing order already
		for (int32 K = 1; K < S.Series.Num(); K++)
			P.Line(FVector2D(Xof(S.Series[K - 1].X), Yof(S.Series[K - 1].Y)), FVector2D(Xof(S.Series[K].X), Yof(S.Series[K].Y)), TCCss::PlayerColour(S.ColourSlot), S.bWon ? 3.f : 2.f, S.bWon ? TEXT("report:line-won") : TEXT("report:line"));
}

void FTCEndReport::TableColumns(const FTCMatchReport& R, const FTCMeasure& Measure, TArray<FString>& Heads, TArray<float>& Content, TArray<float>& MinC)
{
	using namespace TCEndReportRules;
	Heads = { TC_T(FString(TEXT("Player"))) };
	for (int32 K = 0; K < 7; K++) Heads.Add(TC_T(FString(FTCMatchReport::Column(K))));
	Content.Reset();
	MinC.Reset();
	for (int32 C = 0; C < Heads.Num(); C++) Content.Add(Measure(Heads[C], TablePx, true, 0.f));
	for (const FTCReportRow& Row : R.Rows)
	{
		const FString N = TC_T(Row.Name);
		Content[0] = FMath::Max(Content[0], FMath::Min(NameMax, Swatch + SwatchGap + Measure(N, TablePx, Row.bMe, 0.f)));
		for (int32 K = 0; K < 7; K++) Content[K + 1] = FMath::Max(Content[K + 1], Measure(FString::FromInt(Row.Values[K]), TablePx, Row.bMe, 0.f));
	}
	for (int32 C = 0; C < Content.Num(); C++)
	{
		float Mw = 0.f;
		TArray<FString> Words;
		Heads[C].ParseIntoArray(Words, TEXT(" "));
		for (const FString& Wd : Words) Mw = FMath::Max(Mw, Measure(Wd, TablePx, true, 0.f));
		if (C > 0) for (const FTCReportRow& Row : R.Rows) Mw = FMath::Max(Mw, Measure(FString::FromInt(Row.Values[C - 1]), TablePx, Row.bMe, 0.f));
		else Mw = Content[0]; // a name has no space to break at: its min-content is the whole (capped) name
		MinC.Add(FMath::Min(Mw, Content[C]));
	}
}

float FTCEndReport::MinWidth(const FTCEndReportInput& In, const FTCMeasure& Measure)
{
	using namespace TCEndReportRules;
	if (!In.Report.bShown) return 0.f;
	TArray<FString> Heads; TArray<float> Content, MinC;
	TableColumns(In.Report, Measure, Heads, Content, MinC);
	float Sum = 2 * CardPadSide;
	for (float V : MinC) Sum += V + 2 * CellPadX;
	return Sum;
}

float FTCEndReport::NaturalWidth(const FTCEndReportInput& In, const FTCMeasure& Measure)
{
	using namespace TCEndReportRules;
	if (!In.Report.bShown) return 0.f;
	TArray<FString> Heads; TArray<float> Content, MinC;
	TableColumns(In.Report, Measure, Heads, Content, MinC);
	float Sum = 2 * CardPadSide;
	for (float V : Content) Sum += V + 2 * CellPadX;
	return Sum;
}

FTCPaint FTCEndReport::Layout(const FTCEndReportInput& In, float X0, float Y0, float Width, const FTCMeasure& Measure, float* OutHeight) const
{
	using namespace TCEndReportRules;
	FTCPaint P;
	const float CardW = FMath::Min(CardWidth, Width);
	const int32 Back = P.Items.Num();
	P.Rect(X0, Y0, CardW, 0, TCCss::PanelSolid, TEXT("report:card"));
	P.Frame(X0, Y0, CardW, 0, TCCss::Line, 1.f, TEXT("report:card-frame"));
	const float X = X0 + CardPadSide, W = CardW - 2 * CardPadSide;
	float Y = Y0 + CardPadTop;
	bool bFirst = true;
	if (In.bResult)
	{
		// main.js renderLobby: #result, 600 24px/1.1, brass
		FTCPaintItem& I = P.Text(TC_T(In.ResultLine), X, Y, ResultPx, TCCss::Brass, true, ETCPaintAlign::Left, TEXT("report:result"));
		I.Size.Y = ResultPx * ResultLine;
		Y += I.Size.Y;
		bFirst = false;
	}
	const FTCMatchReport& R = In.Report;
	if (R.bShown)
	{
		if (!bFirst) Y += CardGap;
		// <summary>Match report<span class="rp-sub">decided ... after ...</span></summary>, the details' marker first
		const float SumH = SummaryPx * SummaryLine;
		const FString Marker = bOpen ? TEXT("▼") : TEXT("▶");
		const FString Title = TC_T(FString(TEXT("Match report")));
		FTCPaintItem& M = P.Text(Marker, X, Y, SummaryPx * 0.7f, TCCss::Text, false, ETCPaintAlign::Left, TEXT("report:marker"));
		M.Size.Y = SumH;
		const float TitleX = X + SummaryPx * 0.95f;
		FTCPaintItem& T = P.Text(Title, TitleX, Y, SummaryPx, TCCss::Text, true, ETCPaintAlign::Left, TEXT("report:summary"));
		T.Size.Y = SumH;
		const float SubX = TitleX + Measure(Title, SummaryPx, true, 0.f) + SubGap;
		const FString Sub = FTCPaint::Ellipsis(TC_T(R.Sub), X + W - SubX, SubPx, false, Measure);
		FTCPaintItem& Su = P.Text(Sub, SubX, Y + 1.f, SubPx, TCCss::Dim, false, ETCPaintAlign::Left, TEXT("report:sub"));
		Su.Size.Y = SumH;
		P.Hit(FBox2D(FVector2D(X, Y), FVector2D(X + W, Y + SumH)), TEXT("report:toggle"), TEXT("Match report"));
		Y += SumH;
		if (bOpen)
		{
			// .rp-top: the chart (3fr) and the legend (2fr), 14 px apart, 8 px under the summary
			Y += TopMargin;
			const float ChartW = (W - TopGap) * 3.f / 5.f, LegendX = X + ChartW + TopGap, LegendW = W - ChartW - TopGap;
			P.Frame(X - 1, Y - 1, ChartW + 2, ChartH + 2, TCCss::LineSoft, 1.f, TEXT("report:chart-frame"));
			Chart(P, R, X, Y, ChartW, ChartH, Measure);
			// <div class="muted">{what} over time</div><ul class="rp-legend"> in team order
			float Ly = Y;
			FTCPaintItem& Lt = P.Text(TC_T(FString::Printf(TEXT("%s over time"), *R.What)), LegendX, Ly, 14.f, TCCss::Dim, false, ETCPaintAlign::Left, TEXT("report:legend-title"));
			Lt.Size.Y = 14.f * 1.35f;
			Ly += Lt.Size.Y + 4.f;
			TArray<const FTCReportSide*> Order;
			for (const FTCReportSide& S : R.Sides) Order.Add(&S);
			Order.StableSort([](const FTCReportSide& A, const FTCReportSide& B) { return A.Team < B.Team; });
			const float LineH = LegendPx * LegendLine;
			for (int32 K = 0; K < Order.Num(); K++)
			{
				if (K) Ly += LegendGap;
				P.Rect(LegendX - 1, Ly + (LineH - Swatch) / 2 + 1 - 1, Swatch + 2, Swatch + 2, TCCss::Rgba(0, 0, 0, 0.45f), TEXT("report:swatch-ring"));
				P.Rect(LegendX, Ly + (LineH - Swatch) / 2 + 1, Swatch, Swatch, TCCss::PlayerColour(Order[K]->ColourSlot), TEXT("report:swatch"));
				const TArray<FString> Lines = FTCPaint::Wrap(TC_T(Order[K]->Legend), LegendW - Swatch - SwatchGap, LegendPx, false, Measure);
				for (const FString& L : Lines)
				{
					FTCPaintItem& Li = P.Text(L, LegendX + Swatch + SwatchGap, Ly, LegendPx, TCCss::Text, false, ETCPaintAlign::Left, TEXT("report:legend"));
					Li.Size.Y = LineH;
					Ly += LineH;
				}
			}
			Y = FMath::Max(Y + ChartH + 1, Ly) + TopBottom;
			// the story table: Player and the seven COLUMNS, numbers right-aligned; the reader's row marked
			const float RowH = TablePx * TableLine + 2 * CellPadY;
			// table-layout: auto at width 100%: each column its widest content plus padding, the room left shared out in
			// proportion to those widths. Round 15 (lead): narrower than that (the card beside the lobby panel in a
			// 1280 px window), each column gives way down to its min-content width (its longest word: the header
			// wraps), in proportion to what it can give, as CSS's auto layout does; the names ellipsize as before
			TArray<FString> Heads;
			TArray<float> Content, MinC;
			TableColumns(R, Measure, Heads, Content, MinC);
			TArray<FString> Names;
			for (const FTCReportRow& Row : R.Rows) Names.Add(TC_T(Row.Name));
			float Sum = 0.f, SumContent = 0.f, SumMin = 0.f;
			for (int32 C = 0; C < Content.Num(); C++) { Sum += Content[C] + 2 * CellPadX; SumContent += Content[C]; SumMin += MinC[C] + 2 * CellPadX; }
			TArray<float> ColW, ColX;
			float Cx = X;
			for (int32 C = 0; C < Content.Num(); C++)
			{
				float Wc = Content[C] + 2 * CellPadX + (Sum < W && SumContent > 0 ? (W - Sum) * Content[C] / SumContent : 0.f);
				if (Sum > W && Sum > SumMin) Wc = MinC[C] + 2 * CellPadX + FMath::Max(0.f, W - SumMin) * (Content[C] - MinC[C]) / (Sum - SumMin);
				ColX.Add(Cx); ColW.Add(Wc); Cx += Wc;
			}
			TArray<TArray<FString>> HeadLines;
			int32 HeadRows = 1;
			for (int32 C = 0; C < Heads.Num(); C++)
			{
				HeadLines.Add(Measure(Heads[C], TablePx, true, 0.f) <= ColW[C] - 2 * CellPadX + 0.5f ? TArray<FString>{ Heads[C] } : FTCPaint::Wrap(Heads[C], ColW[C] - 2 * CellPadX, TablePx, true, Measure));
				HeadRows = FMath::Max(HeadRows, HeadLines.Last().Num());
			}
			for (int32 C = 0; C < Heads.Num(); C++)
			{
				const bool bLeft = C == 0;
				// th: vertical-align middle across the header row
				float Hy = Y + CellPadY + (HeadRows - HeadLines[C].Num()) * TablePx * TableLine / 2;
				for (const FString& HL : HeadLines[C])
				{
					FTCPaintItem& H = P.Text(HL, bLeft ? ColX[C] + CellPadX : ColX[C] + ColW[C] - CellPadX, Hy, TablePx, TCCss::Dim, true, bLeft ? ETCPaintAlign::Left : ETCPaintAlign::Right, TEXT("report:th"));
					H.Size.Y = TablePx * TableLine;
					Hy += TablePx * TableLine;
				}
			}
			Y += RowH + (HeadRows - 1) * TablePx * TableLine;
			P.Rect(X, Y, W, 1.f, TCCss::LineSoft, TEXT("report:th-rule"));
			Y += 1.f;
			for (int32 Ri = 0; Ri < R.Rows.Num(); Ri++)
			{
				const FTCReportRow& Row = R.Rows[Ri];
				if (Ri) { P.Rect(X, Y, W, 1.f, TCCss::Rgba(176, 164, 122, 0.12f), TEXT("report:row-rule")); Y += 1.f; }
				if (Row.bMe) P.Rect(X, Y, W, RowH, TCCss::Rgba(214, 178, 94, 0.08f), TEXT("report:me"));
				const float Ty = Y + CellPadY, Th = TablePx * TableLine;
				P.Rect(ColX[0] + CellPadX, Ty + (Th - Swatch) / 2 + 1 - 1, Swatch + 2, Swatch + 2, TCCss::Rgba(0, 0, 0, 0.45f), TEXT("report:swatch-ring"));
				P.Rect(ColX[0] + CellPadX + 1, Ty + (Th - Swatch) / 2 + 1, Swatch, Swatch, TCCss::PlayerColour(Row.Slot), TEXT("report:swatch"));
				const FString Name = FTCPaint::Ellipsis(Names[Ri], FMath::Min(NameMax, ColW[0] - 2 * CellPadX) - Swatch - SwatchGap, TablePx, Row.bMe, Measure);
				FTCPaintItem& N = P.Text(Name, ColX[0] + CellPadX + Swatch + SwatchGap, Ty, TablePx, TCCss::Text, Row.bMe, ETCPaintAlign::Left, Row.bMe ? TEXT("report:name-me") : TEXT("report:name"));
				N.Size.Y = Th;
				for (int32 K = 0; K < 7; K++)
				{
					FTCPaintItem& V = P.Text(FString::FromInt(Row.Values[K]), ColX[K + 1] + ColW[K + 1] - CellPadX, Ty, TablePx, TCCss::Text, Row.bMe, ETCPaintAlign::Right, TEXT("report:td"));
					V.Size.Y = Th;
				}
				Y += RowH;
			}
		}
	}
	Y += CardPadBottom;
	const float H = Y - Y0;
	P.Items[Back].Size.Y = H;
	P.Items[Back + 1].Size.Y = H;
	P.Panels.Add(FBox2D(FVector2D(X0, Y0), FVector2D(X0 + CardW, Y0 + H)));
	if (!In.bResult && !R.bShown) { P.Items.Reset(); P.Hits.Reset(); P.Panels.Reset(); }
	if (OutHeight) *OutHeight = P.Items.Num() ? H : 0.f;
	return P;
}

// ---------------------------------------------------------------------------------------------------------------------
// Automation tests

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Dom/JsonObject.h"

namespace TCEndReportTest
{
	TSharedPtr<FJsonObject> Parse(const FString& S)
	{
		TSharedPtr<FJsonObject> O;
		const TSharedRef<TJsonReader<>> R = TJsonReaderFactory<>::Create(S);
		FJsonSerializer::Deserialize(R, O);
		return O;
	}
	// a lobby message carrying a match result, as server.js sends it after a match (shapes from shared/story.js)
	FTCMatchState Lobby(const FString& Result, int32 You)
	{
		FTCMatchState St;
		St.LobbySeat = You;
		St.ApplyLobby(*Parse(FString::Printf(TEXT("{\"t\":\"lobby\",\"state\":\"lobby\",\"you\":%d,\"players\":[],\"result\":%s}"), You, *Result)));
		return St;
	}
	FString Story(int32 Kills, int32 Losses, int32 Built, int32 Captures, int32 Mp, int32 Support, int32 Planes)
	{
		return FString::Printf(TEXT("{\"kills\":%d,\"losses\":%d,\"built\":%d,\"captures\":%d,\"mpSpent\":%d,\"supportCalls\":%d,\"planesDowned\":%d}"), Kills, Losses, Built, Captures, Mp, Support, Planes);
	}
	// a 2 v 2 Conquest decided on victory points at 12:03 (tick 14460), seat 0's view
	FString Conquest()
	{
		FString Tl;
		for (int32 I = 0; I <= 72; I++)
		{
			const int32 A = FMath::Min(1200, I * 17), B = FMath::Min(1100, I * 15);
			Tl += FString::Printf(TEXT("%s{\"t\":%d,\"vp\":[%d,%d]}"), I ? TEXT(",") : TEXT(""), I * 10, A, B);
		}
		Tl += TEXT(",{\"t\":723,\"vp\":[1200,1085]}");
		return FString::Printf(TEXT("{\"winner\":0,\"teams\":[0,1,0,1],\"names\":[\"Alder\",\"Birch\",\"AI 1\",\"AI 2\"],\"reason\":\"vp\",\"at\":{\"x\":97,\"z\":37},\"endTick\":14460,")
			TEXT("\"story\":[%s,%s,%s,%s],\"timeline\":[%s],\"winVp\":1200,\"mode\":\"conquest\",\"you\":{\"outcome\":\"victory\",\"team\":0}}"),
			*Story(31, 12, 4, 6, 2350, 5, 1), *Story(18, 27, 2, 3, 2100, 2, 0), *Story(22, 15, 3, 5, 2290, 3, 2), *Story(9, 30, 1, 2, 1980, 1, 0), *Tl);
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCEndReportLayoutTest, "ThreeCrossroads.EndReport.Layout", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCEndReportLayoutTest::RunTest(const FString&)
{
	using namespace TCEndReportTest;
	FTCLocale::FScopedLanguage En(ETCLanguage::English);
	const FTCMeasure M = FTCPaint::ApproxMeasure();
	{
		const FTCMatchState St = Lobby(Conquest(), 0);
		const FTCEndReportInput In = FTCEndReport::FromState(St);
		FTCEndReport Card;
		Card.Sync(In);
		float H = 0;
		const FTCPaint P = Card.Layout(In, 0, 0, 920, M, &H);
		TestEqual(TEXT("result line"), P.TextsOf(TEXT("report:result")), TArray<FString>{ TEXT("Victory") });
		TestEqual(TEXT("summary"), P.TextsOf(TEXT("report:summary")), TArray<FString>{ TEXT("Match report") });
		TestEqual(TEXT("sub line (report.js: decided on victory points after clock(endTick / 20))"), P.TextsOf(TEXT("report:sub")), TArray<FString>{ TEXT("decided on victory points after 12 min 3 s") });
		TestEqual(TEXT("legend title"), P.TextsOf(TEXT("report:legend-title")), TArray<FString>{ TEXT("Victory points over time") });
		TestEqual(TEXT("legend in team order with (won)"), P.TextsOf(TEXT("report:legend")), TArray<FString>{ TEXT("Alder & AI 1: 1200 victory points (won)"), TEXT("Birch & AI 2: 1085 victory points") });
		TestEqual(TEXT("table head"), P.TextsOf(TEXT("report:th")), TArray<FString>{ TEXT("Player"), TEXT("Kills"), TEXT("Losses"), TEXT("Built"), TEXT("Captures"), TEXT("Manpower spent"), TEXT("Support calls"), TEXT("Planes downed") });
		TestEqual(TEXT("my row marked (r.you && i === me)"), P.TextsOf(TEXT("report:name-me")), TArray<FString>{ TEXT("Alder") });
		TestEqual(TEXT("other rows"), P.TextsOf(TEXT("report:name")), TArray<FString>{ TEXT("Birch"), TEXT("AI 1"), TEXT("AI 2") });
		const TArray<FString> Td = P.TextsOf(TEXT("report:td"));
		TestEqual(TEXT("28 numbers"), Td.Num(), 28);
		if (Td.Num() == 28) TestEqual(TEXT("seat 0's story"), FString::Join(TArray<FString>(Td.GetData(), 7), TEXT(",")), FString(TEXT("31,12,4,6,2350,5,1")));
		TestEqual(TEXT("axis labels: 0, top, 0 min, clock(end)"), P.TextsOf(TEXT("report:axis")), TArray<FString>{ TEXT("0"), TEXT("1200"), TEXT("0 min"), TEXT("12 min 3 s") });
		int32 Grid = 0, Won = 0, Lost = 0;
		for (const FTCPaintItem& I : P.Items) { Grid += I.Role == TEXT("report:grid"); Won += I.Role == TEXT("report:line-won"); Lost += I.Role == TEXT("report:line"); }
		TestEqual(TEXT("three grid lines"), Grid, 3);
		TestEqual(TEXT("the winners' line: 73 legs"), Won, 73);
		TestEqual(TEXT("the other line: 73 legs"), Lost, 73);
		// the winners are drawn last (on top), 3 px wide, in their first seat's colour (slot 0 blue)
		const FTCPaintItem* LastLine = nullptr;
		for (const FTCPaintItem& I : P.Items) if (I.Kind == ETCPaintKind::Line && I.Role.StartsWith(TEXT("report:line"))) LastLine = &I;
		TestTrue(TEXT("winner last, 3 px, blue"), LastLine && LastLine->Role == TEXT("report:line-won") && LastLine->Thickness == 3.f && LastLine->Colour.Equals(TCCss::Hex(0x3b73d6)));
		// chart geometry: L = ceil(measureText('1200')) + 12 = 26 + 12 with the test measure; the end at L + pw
		const FTCPaintItem* Chart = P.Find(TEXT("report:chart"));
		TestTrue(TEXT("chart 132 px tall, 3fr of the inner width"), Chart && Chart->Size.Y == 132.f && FMath::IsNearlyEqual(Chart->Size.X, (868.f - 14.f) * 0.6f, 0.01f));
		TestTrue(TEXT("height"), H > 300.f && H < 520.f);
		TestEqual(TEXT("one hit: the summary toggles"), P.Hits.Num(), 1);
		Card.Toggle();
		const FTCPaint Closed = Card.Layout(In, 0, 0, 920, M);
		TestEqual(TEXT("closed: no table"), Closed.TextsOf(TEXT("report:th")).Num(), 0);
		Card.Sync(In);
		TestFalse(TEXT("the same report stays closed"), Card.IsOpen());
		const FTCMatchState St3 = Lobby(Conquest(), 3);
		Card.Sync(FTCEndReport::FromState(St3));
		TestTrue(TEXT("a new key opens it again"), Card.IsOpen());
	}
	{
		// a match the host ended: the result line only, no report (report.js hides it without a story)
		const FTCMatchState St = Lobby(TEXT("{\"ended\":true}"), 0);
		const FTCEndReportInput In = FTCEndReport::FromState(St);
		FTCEndReport Card;
		Card.Sync(In);
		const FTCPaint P = Card.Layout(In, 0, 0, 920, M);
		TestEqual(TEXT("host ended: the line"), P.TextsOf(TEXT("report:result")), TArray<FString>{ TEXT("Match ended by the host") });
		TestEqual(TEXT("host ended: no summary"), P.TextsOf(TEXT("report:summary")).Num(), 0);
	}
	{
		// Horde: points held, "overrun on wave n", the Horde seat's legend
		const FString Horde = FString::Printf(TEXT("{\"winner\":1,\"teams\":[0,0,1],\"names\":[\"Ann\",\"Bo\",\"Horde\"],\"reason\":\"structures\",\"endTick\":10900,\"mode\":\"horde\",\"story\":[%s,%s,%s],")
			TEXT("\"timeline\":[{\"t\":0,\"points\":[0,0]},{\"t\":540,\"points\":[0,0]}],\"winVp\":null,\"horde\":{\"wave\":7,\"kills\":120,\"time\":545,\"names\":[\"Ann\",\"Bo\"],\"record\":true,\"best\":null},")
			TEXT("\"you\":{\"outcome\":\"defeat\",\"team\":0}}"), *Story(40, 3, 0, 0, 900, 2, 0), *Story(80, 5, 1, 0, 1200, 3, 1), *Story(0, 120, 0, 0, 0, 0, 0));
		const FTCMatchState St = Lobby(Horde, 1);
		const FTCEndReportInput In = FTCEndReport::FromState(St);
		FTCEndReport Card;
		Card.Sync(In);
		const FTCPaint P = Card.Layout(In, 0, 0, 920, M);
		TestEqual(TEXT("horde line"), P.TextsOf(TEXT("report:result")), TArray<FString>{ TEXT("Overrun on wave 7: 120 kills in 9:05, a new record") });
		TestEqual(TEXT("horde sub"), P.TextsOf(TEXT("report:sub")), TArray<FString>{ TEXT("overrun on wave 7 after 9 min 5 s") });
		TestEqual(TEXT("horde legend title"), P.TextsOf(TEXT("report:legend-title")), TArray<FString>{ TEXT("Points held over time") });
		TestEqual(TEXT("horde axis (top is max(1, ...))"), P.TextsOf(TEXT("report:axis")), TArray<FString>{ TEXT("0"), TEXT("1"), TEXT("0 min"), TEXT("9 min") });
		TestEqual(TEXT("seat 1 marked"), P.TextsOf(TEXT("report:name-me")), TArray<FString>{ TEXT("Bo") });
	}
	{
		// the helpers: ellipsis and wrapping with the test measure (7 px per character at 14 px)
		TestEqual(TEXT("ellipsis"), FTCPaint::Ellipsis(TEXT("abcdefghij"), 50, 14, false, M), FString(TEXT("abcdef…")));
		TestEqual(TEXT("fits"), FTCPaint::Ellipsis(TEXT("abc"), 50, 14, false, M), FString(TEXT("abc")));
		TestEqual(TEXT("wrap"), FTCPaint::Wrap(TEXT("one two three four"), 70, 14, false, M), TArray<FString>{ TEXT("one two"), TEXT("three four") });
		TestTrue(TEXT("css ease ends"), FMath::IsNearlyEqual(TCCss::Ease(0.f), 0.f, 1e-4f) && FMath::IsNearlyEqual(TCCss::Ease(1.f), 1.f, 1e-4f) && TCCss::Ease(0.5f) > 0.75f && TCCss::Ease(0.5f) < 0.85f);
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCEndReportSpanishTest, "ThreeCrossroads.EndReport.Spanish", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCEndReportSpanishTest::RunTest(const FString&)
{
	using namespace TCEndReportTest;
	// client/es.js, keyed by the English text node (i18n.js spanish); the canvas labels stay as report.js draws them
	FTCLocale::FScopedLanguage Es(ETCLanguage::Spanish);
	const FTCMatchState St = Lobby(Conquest(), 0);
	const FTCEndReportInput In = FTCEndReport::FromState(St);
	FTCEndReport Card;
	Card.Sync(In);
	const FTCPaint P = Card.Layout(In, 0, 0, 920, FTCPaint::ApproxMeasure());
	TestEqual(TEXT("summary"), P.TextsOf(TEXT("report:summary")), TArray<FString>{ TEXT("Informe de la partida") });
	TestEqual(TEXT("legend title ('{0} over time')"), P.TextsOf(TEXT("report:legend-title")), TArray<FString>{ TEXT("Puntos de victoria a lo largo del tiempo") });
	// round 15 (lead): a header the column cannot hold wraps at its words (CSS auto table layout); this approximate
	// measure is wider than the browser's font, so the Spanish heads may wrap here where the browser keeps one line:
	// each column's lines are joined back (one anchor X per column)
	auto Heads = [](const FTCPaint& Pa)
	{
		TArray<FString> Out;
		float LastX = -1e9f;
		for (const FTCPaintItem& It : Pa.Items)
			if (It.Kind == ETCPaintKind::Text && It.Role == TEXT("report:th"))
			{
				if (Out.Num() && FMath::IsNearlyEqual(It.Pos.X, LastX, 0.01f)) Out.Last() += TEXT(" ") + It.Text; else Out.Add(It.Text);
				LastX = It.Pos.X;
			}
		return Out;
	};
	const TArray<FString> EsHeads = { TEXT("Jugador"), TEXT("Bajas causadas"), TEXT("Pérdidas"), TEXT("Construido"), TEXT("Capturas"), TEXT("Mano de obra gastada"), TEXT("Llamadas de apoyo"), TEXT("Aviones derribados") };
	TestEqual(TEXT("table head"), Heads(P), EsHeads);
	{
		// the card beside the lobby panel in a 1280 px window (556 px): the columns give way to their longest word, the
		// heads wrap, and nothing reaches past the card's right edge
		// heads (in Spanish, the longest words with their padding need about 606 px with this measure: the table runs
		// past the card, as CSS lets a table wider than its min-content do; in English it fits)
		const FTCPaint N = Card.Layout(In, 0, 0, 556, FTCPaint::ApproxMeasure());
		TestEqual(TEXT("narrow: table head, joined"), Heads(N), EsHeads);
		FTCLocale::FScopedLanguage EnNarrow(ETCLanguage::English);
		FTCEndReport CardEn;
		CardEn.Sync(In);
		const FTCPaint NE = CardEn.Layout(In, 0, 0, 556, FTCPaint::ApproxMeasure());
		TestEqual(TEXT("narrow, English: table head, joined"), Heads(NE), TArray<FString>{ TEXT("Player"), TEXT("Kills"), TEXT("Losses"), TEXT("Built"), TEXT("Captures"), TEXT("Manpower spent"), TEXT("Support calls"), TEXT("Planes downed") });
		float Right = 0.f;
		for (const FTCPaintItem& It : NE.Items) if (It.Kind == ETCPaintKind::Text && (It.Role == TEXT("report:th") || It.Role == TEXT("report:td"))) Right = FMath::Max(Right, It.Pos.X);
		TestTrue(TEXT("narrow, English: the right-aligned cells end inside the card"), Right <= 556.f);
	}
	{
		// round 16 (hudfit): the card's min-content width (the HUD narrows the lobby panel down to 480 px so the card
		// beside it gets at least this): at that width every Spanish cell ends inside the card, and in a 1280 px window
		// it fits beside a 480 px lobby panel (1280 - 24 - 480 = 776 px)
		const FTCMeasure Approx = FTCPaint::ApproxMeasure();
		const float MinW = FTCEndReport::MinWidth(In, Approx), NatW = FTCEndReport::NaturalWidth(In, Approx);
		AddInfo(FString::Printf(TEXT("es: card min-content %.0f px, natural %.0f px"), MinW, NatW));
		TestTrue(TEXT("min-content under natural"), MinW > 0.f && MinW <= NatW);
		TestTrue(TEXT("min-content fits beside a 480 px lobby panel in 1280 px"), MinW <= 1280.f - 24.f - 480.f);
		const FTCPaint F = Card.Layout(In, 0, 0, MinW, Approx);
		float Right = 0.f;
		for (const FTCPaintItem& It : F.Items)
			if (It.Kind == ETCPaintKind::Text && (It.Role == TEXT("report:th") || It.Role == TEXT("report:td")))
				Right = FMath::Max(Right, It.Align == ETCPaintAlign::Right ? It.Pos.X : It.Pos.X + Approx(It.Text, It.FontPx, It.bBold, 0.f));
		TestTrue(TEXT("at its min-content width every Spanish cell ends inside the card's padding"), Right <= MinW - TCEndReportRules::CardPadSide + 0.5f);
		TestEqual(TEXT("at its min-content width: table head, joined"), Heads(F), EsHeads);
		// no report, no table: no width asked
		FTCEndReportInput None;
		TestEqual(TEXT("no report: 0"), FTCEndReport::MinWidth(None, Approx), 0.f);
	}
	TestEqual(TEXT("canvas labels are not translated"), P.TextsOf(TEXT("report:axis")), TArray<FString>{ TEXT("0"), TEXT("1200"), TEXT("0 min"), TEXT("12 min 3 s") });
	// each text goes through spanish() exactly once, as the observer does: the expected values are the locale's own
	TestEqual(TEXT("result"), P.TextsOf(TEXT("report:result")), TArray<FString>{ FTCLocale::T(TEXT("Victory")) });
	TestEqual(TEXT("sub"), P.TextsOf(TEXT("report:sub")), TArray<FString>{ FTCLocale::T(TEXT("decided on victory points after 12 min 3 s")) });
	TestEqual(TEXT("legend"), P.TextsOf(TEXT("report:legend")).Num() > 0 ? P.TextsOf(TEXT("report:legend"))[0] : FString(), FTCLocale::T(TEXT("Alder & AI 1: 1200 victory points (won)")));
	AddInfo(FString::Printf(TEXT("es: result '%s', sub '%s', legend '%s'"), *P.TextsOf(TEXT("report:result"))[0], *P.TextsOf(TEXT("report:sub"))[0], *P.TextsOf(TEXT("report:legend"))[0]));
	return true;
}
// The capture switch (round 15 evidence): -TCAlertsShotDir=<dir> [-TCAlertsBackdrop=<png>] renders the alert feed, the
// history box, the boss bar, the report card and the stats overlay through this canvas code into PNG files, offscreen
// (not under -nullrhi). The data are the synthesized messages of the tests here and in TCAlerts.cpp, not a live match.
#include "Client/TCAlerts.h"
#include "Client/TCBossBar.h"
#include "Client/TCKeys.h"
#include "Engine/TextureRenderTarget2D.h"
#include "Engine/Texture2D.h"
#include "Engine/Engine.h"
#include "CanvasTypes.h"
#include "TextureResource.h"
#include "ImageUtils.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Misc/App.h"
#include "RenderingThread.h"

namespace TCEndReportTest
{
	bool Capture(FAutomationTestBase& T, const FString& File, int32 W, int32 H, UTexture2D* Backdrop, float Dim, const TFunction<void(UCanvas*)>& Paint)
	{
		UTextureRenderTarget2D* RT = NewObject<UTextureRenderTarget2D>(GetTransientPackage());
		RT->RenderTargetFormat = RTF_RGBA8; // gamma-space blending with the 2.2 display gamma, as the game viewport's canvas
		RT->TargetGamma = 2.2f;
		RT->ClearColor = FLinearColor(0.02f, 0.025f, 0.03f, 1.f);
		RT->InitAutoFormat(W, H);
		RT->UpdateResourceImmediate(true);
		FTextureRenderTargetResource* Res = RT->GameThread_GetRenderTargetResource();
		if (!Res) { T.AddError(TEXT("no render target resource")); return false; }
		FCanvas C(Res, nullptr, FGameTime::GetTimeSinceAppStart(), GMaxRHIFeatureLevel);
		UCanvas* UC = NewObject<UCanvas>(GetTransientPackage());
		UC->Init(W, H, nullptr, &C);
		UC->Update();
		if (Backdrop)
		{
			FCanvasTileItem Tile(FVector2D::ZeroVector, Backdrop->GetResource(), FVector2D(W, H), FLinearColor::White);
			Tile.BlendMode = SE_BLEND_Opaque;
			UC->DrawItem(Tile);
		}
		if (Dim > 0) { FCanvasTileItem D(FVector2D::ZeroVector, GWhiteTexture, FVector2D(W, H), FLinearColor(0, 0, 0, Dim)); D.BlendMode = SE_BLEND_Translucent; UC->DrawItem(D); }
		Paint(UC);
		C.Flush_GameThread(true);
		FlushRenderingCommands();
		TArray<FColor> Px;
		if (!Res->ReadPixels(Px) || Px.Num() != W * H) { T.AddError(TEXT("ReadPixels failed")); return false; }
		for (FColor& P : Px) P.A = 255;
		TArray64<uint8> Png;
		FImageUtils::PNGCompressImageArray(W, H, TArrayView64<const FColor>(Px.GetData(), Px.Num()), Png);
		const bool bOk = FFileHelper::SaveArrayToFile(Png, *File);
		T.AddInfo(FString::Printf(TEXT("capture %s: %s (%d x %d)"), bOk ? TEXT("written") : TEXT("FAILED"), *File, W, H));
		return bOk;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCEndReportCaptureTest, "ThreeCrossroads.EndReport.Capture", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCEndReportCaptureTest::RunTest(const FString&)
{
	using namespace TCEndReportTest;
	FString Dir, BackPath;
	if (!FParse::Value(FCommandLine::Get(), TEXT("-TCAlertsShotDir="), Dir)) { AddInfo(TEXT("no -TCAlertsShotDir: nothing captured")); return true; }
	if (!FApp::CanEverRender() || !GEngine || !GEngine->GetSmallFont()) { AddInfo(TEXT("no renderer (-nullrhi): nothing captured")); return true; }
	IFileManager::Get().MakeDirectory(*Dir, true);
	UFont* Font = GEngine->GetSmallFont();
	const FTCMeasure M = FTCPaint::CanvasMeasure(Font);
	UTexture2D* Back = FParse::Value(FCommandLine::Get(), TEXT("-TCAlertsBackdrop="), BackPath) ? FImageUtils::ImportFileAsTexture2D(BackPath) : nullptr;
	if (Back) { Back->UpdateResource(); FlushRenderingCommands(); }
	AddInfo(FString::Printf(TEXT("backdrop: %s; font %s, max char height %.1f"), Back ? *BackPath : TEXT("none"), *Font->GetName(), Font->GetMaxCharHeight()));
	const int32 W = 1280, H = 720;
	// the minimap's place in the reference HUD (index.html: --mm 184px, --gap 8px, bottom right), outlined as a guide
	const FBox2D Mini(FVector2D(W - 8 - 184, H - 8 - 184), FVector2D(W - 8, H - 8));
	auto Guide = [&Mini](FTCPaint& P) { P.Frame(Mini.Min.X - 5, Mini.Min.Y - 5, Mini.GetSize().X + 10, Mini.GetSize().Y + 10, TCCss::Rgba(0, 0, 0, 0.7f), 5.f, TEXT("guide")); };
	auto Feed = [&](FTCAlerts& A, double Now) { FTCAlertViewInput In; In.Now = Now; In.Right = Mini.Max.X; In.MinimapTop = Mini.Min.Y; In.ViewW = W; In.ViewH = H; return A.Layout(In, M); };
	auto Push = [](FTCAlerts& A, double T)
	{
		A.Push(TEXT("attack"), TEXT("M4 Sherman under attack"), true, FVector2D(60, 60), T);
		A.Push(TEXT("pointWon"), TEXT("Point captured"), true, FVector2D(50, 50), T + 0.5);
		A.Push(TEXT("ready"), TEXT("Rifle Squad ready"), true, FVector2D(24, 30), T + 1.0);
		A.Push(TEXT("unitLost"), TEXT("3 units lost"), true, FVector2D(120, 120), T + 1.5);
		A.Push(TEXT("base"), TEXT("Our base is under attack!"), true, FVector2D(22, 22), T + 2.0);
	};
	for (ETCLanguage L : { ETCLanguage::English, ETCLanguage::Spanish })
	{
		FTCLocale::FScopedLanguage Lang(L);
		const FString Code = L == ETCLanguage::Spanish ? TEXT("es") : TEXT("en");
		{
			FTCAlerts A;
			Push(A, 100.0);
			FTCPaint P; Guide(P); P.Append(Feed(A, 106.0)); // the base alert after its pulses
			Capture(*this, Dir / FString::Printf(TEXT("alerts-feed-loss-base-routine-synthetic-%s.png"), *Code), W, H, Back, 0.f, [&](UCanvas* C) { P.Draw(C, Font); });
		}
		{
			FTCAlerts A;
			Push(A, 100.0);
			A.Push(TEXT("air"), TEXT("Enemy Strafing Run incoming"), true, FVector2D(125, 125), 120.0);
			A.Push(TEXT("pointLost"), TEXT("Point cut off: it pays nothing until the road to it is open"), true, FVector2D(80, 20), 120.5);
			A.Navigate(1); A.Navigate(1);
			FTCPaint P; Guide(P); P.Append(Feed(A, 121.0));
			Capture(*this, Dir / FString::Printf(TEXT("alerts-feed-history-open-synthetic-%s.png"), *Code), W, H, Back, 0.f, [&](UCanvas* C) { P.Draw(C, Font); });
		}
		{
			FTCMatchState St;
			St.ApplyStart(*Parse(TEXT(R"({"t":"start","matchId":7,"you":0,"spawn":{"x":20,"z":20},"spawns":[{"x":20,"z":20},{"x":180,"z":180}],"names":["Ann","Horde"],"teams":[0,1],"factions":[0,1],"map":{"name":"synthetic","w":4,"h":4,"rows":["....","....","....","...."],"points":[]}})")));
			St.ApplySnapshot(*Parse(TEXT(R"({"t":"s","tick":40,"mp":100,"units":[],"held":[0,0],"gone":[],"points":[],"shots":[],"mode":{"kind":"horde","wave":10,"left":14,"active":true,"boss":[4321,6000],"slot":1}})")));
			FTCBossBar B;
			B.Snapshot(St, 1.0);
			const FTCPaint P = B.Layout(W / 2.f, 96.f, W, 2.0, M);
			Capture(*this, Dir / FString::Printf(TEXT("alerts-boss-bar-wave10-73pct-synthetic-%s.png"), *Code), W, 220, Back, 0.f, [&](UCanvas* C) { P.Draw(C, Font); });
		}
		{
			const FTCMatchState St = Lobby(Conquest(), 0);
			const FTCEndReportInput In = FTCEndReport::FromState(St);
			FTCEndReport Card; Card.Sync(In);
			float Hh = 0;
			const FTCPaint P = Card.Layout(In, (W - 920) / 2.f, 40.f, 920.f, M, &Hh);
			Capture(*this, Dir / FString::Printf(TEXT("alerts-end-report-conquest-2v2-synthetic-%s.png"), *Code), W, 560, Back, 0.55f, [&](UCanvas* C) { P.Draw(C, Font); });
		}
	}
	{
		FTCStatsOverlay O;
		O.Load(FString(TEXT("{\"show\":true,\"stats\":[\"fps\",\"frame\",\"worst\",\"ping\",\"loss\",\"snaps\",\"server\"]}")));
		FTCPerfStats St;
		St.Fps = 58.4; St.Ms = 17.1; St.Worst = 41.5; St.Ping = 48; St.Loss = 0; St.Snaps = 9.9; St.SrvTick = 31;
		const FTCPaint P = O.Layout(St, -1, -1, -1, FBox2D(FVector2D(420, 6), FVector2D(860, 90)), W, 0, W, M);
		Capture(*this, Dir / TEXT("alerts-stats-overlay-f2-synthetic.png"), 480, 260, Back, 0.f, [&](UCanvas* C) { P.Draw(C, Font); });
	}
	return true;
}
#endif
