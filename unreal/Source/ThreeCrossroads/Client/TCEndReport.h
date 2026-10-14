// The end of the match as the reference lobby shows it after a match played to the end (parity CLI-21, with the result
// line of CLI-20), and the small canvas draw list the round 15 alerts scope draws its HUD pieces through.
//
// Ported from the pinned reference client (reference/ww2-rts at e4e579a):
//   client/report.js   renderReport: the "Match report" details card (open), its summary line ("decided <how> after
//                      <clock>", or "overrun on wave <n> after <clock>"), the race chart (a 132 px canvas: three grid
//                      lines, the value and time labels, one line per side over the 10 s samples, winners drawn last
//                      and 3 px wide), the "<what> over time" legend in team order with a swatch per side and "(won)",
//                      and the story table per player (Player, Kills, Losses, Built, Captures, Manpower spent, Support
//                      calls, Planes downed) with the reader's own row marked; hidden for a host-ended match (no story);
//                      the card opens again for a new report (keyed by endTick, names, you, colours and me);
//   client/main.js     renderLobby: the result line above it (#result, 600 24px brass), and COLORS by slot;
//   client/index.html  #overlay .card (min(920px, 100%), padding 22px 26px 18px, gap 12px) and the report's own CSS
//                      (report.js style()): sizes, colours, gaps and borders.
// Text: every text node goes through TC_T (client/i18n.js spanish(), Client/TCLocale.h) as the page's observer
// translates each node on its own; the canvas labels report.js draws are not translated (it calls no t()).
// Data: only FTCMatchState::LastMatch (the lobby message's result), built into FTCMatchReport by FTCObjectives; nothing
// else is read, so the card shows what this reader's lobby message delivered.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCObjectives.h"

class UCanvas;
class UFont;
class FTCMatchState;
struct FTCMatchResult;

// ---------------------------------------------------------------------------------------------------------------------
// A draw list in UI pixels: what a HUD piece wants on screen, built by a pure layout function (tests read it without a
// canvas) and painted by FTCPaint::Draw on the HUD's canvas. Hits are the clickable rectangles with a tag the
// controller understands; Panels are rectangles that must keep a click from reaching the world.
enum class ETCPaintKind : uint8 { Rect, Gradient, Frame, Line, Text };
enum class ETCPaintAlign : uint8 { Left, Centre, Right };

struct FTCPaintItem
{
	ETCPaintKind Kind = ETCPaintKind::Rect;
	FVector2D Pos = FVector2D::ZeroVector;   // Rect, Gradient, Frame: the top left; Line: from; Text: the anchor (top)
	FVector2D Size = FVector2D::ZeroVector;  // Rect, Gradient, Frame: width and height; Line: to
	FLinearColor Colour = FLinearColor::White, Colour2 = FLinearColor::White; // Gradient: top, bottom
	float Thickness = 1.f;                   // Frame and Line
	FString Text;                            // already in the language in use
	float FontPx = 14.f;
	bool bBold = false;
	float Tracking = 0.f;                    // CSS letter-spacing in pixels
	ETCPaintAlign Align = ETCPaintAlign::Left;
	FString Role;                            // what the item is, for tests and logs ('alert:text', 'report:th', ...)
};

struct FTCPaintHit
{
	FBox2D Rect = FBox2D(ForceInit);
	FString Tag;     // 'alert:at:<x>,<z>', 'alert:history', 'alert:nav:1', 'alert:entry:<id>', 'report:toggle'
	FString Label;   // English, for logs and the HUD's button list
};

// Text width in pixels for a text at a CSS font size; tests use Approx, the HUD the canvas font.
using FTCMeasure = TFunction<float(const FString& Text, float FontPx, bool bBold, float Tracking)>;

class FTCPaint
{
public:
	TArray<FTCPaintItem> Items;
	TArray<FTCPaintHit> Hits;
	TArray<FBox2D> Panels;

	FTCPaintItem& Rect(float X, float Y, float W, float H, const FLinearColor& C, const TCHAR* Role = TEXT(""));
	FTCPaintItem& Gradient(float X, float Y, float W, float H, const FLinearColor& Top, const FLinearColor& Bottom, const TCHAR* Role = TEXT(""));
	FTCPaintItem& Frame(float X, float Y, float W, float H, const FLinearColor& C, float Thickness = 1.f, const TCHAR* Role = TEXT(""));
	FTCPaintItem& Line(FVector2D A, FVector2D B, const FLinearColor& C, float Thickness = 1.f, const TCHAR* Role = TEXT(""));
	FTCPaintItem& Text(const FString& Shown, float X, float Y, float FontPx, const FLinearColor& C, bool bBold = false, ETCPaintAlign Align = ETCPaintAlign::Left, const TCHAR* Role = TEXT(""));
	void Hit(const FBox2D& R, const FString& Tag, const FString& Label) { Hits.Add({ R, Tag, Label }); }
	// Multiply the alpha of the items from index First on (a line fading out); move them (a line sliding in).
	void Fade(int32 First, float Alpha);
	void Shift(int32 First, FVector2D By);
	void Append(const FTCPaint& Other);
	// The texts with a role, in order (tests).
	TArray<FString> TextsOf(const FString& Role) const;
	const FTCPaintItem* Find(const FString& Role) const;

	// A rough measure for tests and headless runs: 0.5 em per character, 0.55 em bold, plus the tracking.
	static FTCMeasure ApproxMeasure();
	// The HUD font's measure (GetTextSize at the scale that gives FontPx); Font may not be null.
	static FTCMeasure CanvasMeasure(UFont* Font);
	// CSS white-space: nowrap; overflow: hidden; text-overflow: ellipsis: the text cut to MaxW with a trailing '...'.
	static FString Ellipsis(const FString& Text, float MaxW, float FontPx, bool bBold, const FTCMeasure& Measure);
	// CSS white-space: normal: words wrapped to MaxW (a word longer than the line breaks anywhere, overflow-wrap).
	static TArray<FString> Wrap(const FString& Text, float MaxW, float FontPx, bool bBold, const FTCMeasure& Measure);
	// Paint on the canvas at the canvas's current transform (the HUD's UI scale is already applied there).
	void Draw(UCanvas* Canvas, UFont* Font) const;
};

// CSS colours of the reference HUD (client/index.html :root).
namespace TCCss
{
	FLinearColor Rgba(int32 R, int32 G, int32 B, float A = 1.f);
	FLinearColor Hex(uint32 Rgb, float A = 1.f);
	extern const FLinearColor Text, Dim, Brass, RedHi, Line, LineSoft, PanelSolid, Panel;
	// client/main.js COLORS (and report.js PALETTE): the player colours by slot
	FLinearColor PlayerColour(int32 Slot); // '#888' for a slot without one
	// CSS 'ease' (cubic-bezier(0.25, 0.1, 0.25, 1)) and 'ease-out' (0, 0, 0.58, 1) and 'ease-in-out' (0.42, 0, 0.58, 1)
	float Ease(float T);
	float EaseOut(float T);
	float EaseInOut(float T);
}

// ---------------------------------------------------------------------------------------------------------------------
// report.js and the lobby card's result line.
struct FTCEndReportInput
{
	bool bResult = false;          // m.result exists: the result line shows
	FString ResultLine;            // FTCObjectives::ResultLine (English)
	FTCMatchReport Report;         // FTCObjectives::BuildReport (bShown false: the report is hidden)
	FString Key;                   // report.js key: a new one opens the card again
};

namespace TCEndReportRules
{
	constexpr float CardWidth = 920.f, CardPadTop = 22.f, CardPadSide = 26.f, CardPadBottom = 18.f, CardGap = 12.f;
	constexpr float ResultPx = 24.f, ResultLine = 1.1f;
	constexpr float SummaryPx = 17.f, SummaryLine = 1.2f, SubPx = 14.f, SubGap = 8.f;
	constexpr float TopMargin = 8.f, TopBottom = 10.f, TopGap = 14.f;          // .rp-top margin 8px 0 10px, gap 14px
	constexpr float ChartH = 132.f, ChartMinW = 240.f;
	constexpr float ChartFontPx = 13.f;                                         // FONT: 500 13px
	constexpr float ChartL = 12.f, ChartR = 10.f, ChartT = 10.f, ChartB = 22.f;
	constexpr float LegendPx = 14.f, LegendLine = 1.25f, LegendGap = 5.f, Swatch = 12.f, SwatchGap = 8.f;
	constexpr float TablePx = 14.f, TableLine = 1.2f, CellPadX = 6.f, CellPadY = 4.f, NameMax = 190.f;
}

class FTCEndReport
{
public:
	// main.js renderLobby's inputs from the lobby state: the result line and the report as FTCObjectives built them.
	static FTCEndReportInput FromState(const FTCMatchState& S);
	// report.js renderReport's key (endTick, names, you, colours, me): a new key opens the card again.
	static FString KeyOf(const FTCMatchResult& R, int32 Me);
	// Keep the details open state across frames (report.js: the card keeps whether it is open until a new report).
	void Sync(const FTCEndReportInput& In);
	void Toggle() { bOpen = !bOpen; }
	bool IsOpen() const { return bOpen; }
	// The card at (X, Y), Width wide (the lobby card's inner width plus padding, at most 920): the result line, then the
	// report when shown. OutHeight: how tall it came out. Hits: 'report:toggle' on the summary.
	FTCPaint Layout(const FTCEndReportInput& In, float X, float Y, float Width, const FTCMeasure& Measure, float* OutHeight = nullptr) const;
	// The chart alone (report.js draw) in a W by H box at (X, Y), for tests.
	static void Chart(FTCPaint& P, const FTCMatchReport& R, float X, float Y, float W, float H, const FTCMeasure& Measure);

private:
	FString OpenKey;
	bool bOpen = true;
};
