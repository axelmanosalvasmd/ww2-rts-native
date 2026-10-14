// The stats overlay's sheet (Menu > Stats overlay) and the frame-number probe, round 16 (controls scope).
//
// Ported from the pinned reference client (reference/ww2-rts at e4e579a):
//   client/stats.js    the sheet half of createStats: the stat checkboxes grouped by STATS[].group with each label and
//                      hint, the Quick pick presets (PRESETS), the four Look selects (CHOICES: Position, Layout, Size,
//                      Background), the 'Show the overlay' and 'Color warnings' switches, and the handlers: a pick that
//                      leaves any stat checked also shows the overlay, a preset sets its stats and shows it, every change
//                      saves 'ww2-stats'; Close, Esc and a click on the dimmed backdrop close the sheet; open() is a
//                      no-op while open and runs beforeOpen (drop any drag or aim) first; the menu's button reads
//                      'Stats overlay: On' or 'Off'.
//   client/index.html  #statsSheet's markup: the header (h2 'Stats overlay', the Close button with its Esc key cap), the
//                      top row (the show switch with its F2 key cap, 'Quick pick' and the preset buttons), the picks,
//                      'Look' and its four labelled selects, the warnings switch; and #menu's statsBtn with its title.
//   client/stats.css   #statsSheet: width min(700px, 100vw - 24px), max-height min(86vh, 720px), padding 16px, a 12 px
//                      grid gap, the sticky header (12px 16px, a soft bottom line), the 3-column picks (gap 12px 20px),
//                      the legends (600 16px brass), the hints (13px/1.2 dim), the 4-column Look grid (gap 8px 12px),
//                      15 px brass checkboxes, a light backdrop (rgba(8, 10, 11, 0.3)); at 640 px and narrower one column
//                      of picks and two of Look. Buttons, selects and key caps from index.html's base rules.
// The overlay box itself, its settings (FTCStatsOverlay) and the numbers (FTCPerf, client/perf.js) are in
// Client/TCKeys.h since round 15; this file draws the sheet that changes those settings.
//
// Text: every text goes through TC_T (client/i18n.js spanish()), as the page's observer translates the sheet's nodes.
// A select in this canvas HUD steps through its options on a click (a right click steps back), as the lobby's selects do.
//
// The probe (FTCStatUnitProbe): with -TCStatUnitProbe, once per perf.js roll, one log line puts the overlay's frame
// numbers beside the engine's own 'stat unit' numbers (FStatUnitData: Frame, Game, Draw, RHIT, GPU), and the mean of
// the engine's raw frame times over the same frames, so the two can be compared in one run (no benchmark).
#pragma once

#include "CoreMinimal.h"
#include "Client/TCEndReport.h"
#include "Client/TCKeys.h"
#include "Client/TCLocale.h"

struct FStatUnitData;

namespace TCStatsSheetRules
{
	// stats.css and index.html, CSS pixels (the dialog is outside #hud: never zoomed by the UI scale)
	constexpr float MaxW = 700.f, SideRoom = 24.f, MaxH = 720.f, MaxHShare = 0.86f, NarrowAt = 640.f;
	constexpr float Pad = 16.f, Gap = 12.f, HeadPadY = 12.f, Border = 1.f;
	constexpr float BodyPx = 14.f, BodyLine = 1.35f, TitlePx = 21.f, H3Px = 16.f, LegendPx = 16.f, LegendPadBottom = 4.f;
	constexpr float SmallPx = 13.f, SmallLine = 1.2f, KbdPx = 13.f, KbdLine = 1.3f, KbdPadX = 4.f;
	constexpr float Check = 15.f, CheckGap = 8.f, TopGapX = 16.f, TopGapY = 10.f, PresetGap = 4.f, PresetRowGap = 8.f;
	constexpr float ButtonPadX = 10.f, ButtonPadY = 5.f, PresetPadX = 9.f, PresetPadY = 3.f, SelectPadX = 8.f, SelectPadY = 5.f;
	constexpr float PicksGapX = 20.f, PicksGapY = 12.f, FieldGap = 5.f, LookGapX = 12.f, LookGapY = 8.f, LookLabelGap = 3.f;
}

// One Look select: its settings key, the label over it and its options (value, text) in CHOICES order.
struct FTCStatChoice
{
	const TCHAR* Key;
	const TCHAR* Label;
	TArray<TPair<FString, FString>> Options;
};

class FTCStatsSheet
{
public:
	// CHOICES with index.html's labels: Position, Layout, Size, Background
	static const TArray<FTCStatChoice>& Choices();
	// The text a select shows for a value ('Top left'), English
	static FString ChoiceText(const FString& Key, const FString& Value);
	// STATS' groups in their first-seen order: Performance, Network, Game
	static TArray<FString> Groups();
	// main.js statsBtn's title
	static const TCHAR* MenuTitle() { return TEXT("Frame rate, ping, loss and other numbers over the game (F2 shows or hides)"); }

	bool IsOpen() const { return bOpen; }
	// stats.open(): nothing while open; else the sheet opens (the caller ran beforeOpen). True when it opened.
	bool Open();
	void Close() { bOpen = false; ScrollY = 0.f; }

	enum class EPress : uint8 { None, Changed, Closed };
	// A control of the sheet by its tag ('stats:show', 'stats:warn', 'stats:pick:<id>', 'stats:preset:<name>',
	// 'stats:select:<key>', 'stats:close'), as stats.js's handlers do. bBack steps a select back. Changed: the caller
	// saves (Overlay.SaveText() into 'ww2-stats').
	EPress Press(const FString& Tag, bool bBack, FTCStatsOverlay& Overlay);
	// A press at a pixel against the last drawn sheet (Remember): a control runs Press; inside the sheet nothing
	// happens; outside it (the dimmed backdrop) the sheet closes. OutTag: what was pressed ('stats:backdrop' outside).
	EPress PressAt(FVector2D Pixel, bool bBack, FTCStatsOverlay& Overlay, FString* OutTag = nullptr);

	// The sheet as stats.css lays it out in a ViewW by ViewH window, centred as a modal dialog, with the backdrop.
	// Mouse: the pointer (hover look on the buttons), or negative for none. Hits: every control with its tag; Panels:
	// the dialog box. Texts are already in the language in use.
	FTCPaint Layout(const FTCStatsSettings& S, float ViewW, float ViewH, const FTCMeasure& Measure, FVector2D Mouse, ETCLanguage Lang) const;
	FTCPaint Layout(const FTCStatsSettings& S, float ViewW, float ViewH, const FTCMeasure& Measure, FVector2D Mouse = FVector2D(-1, -1)) const { return Layout(S, ViewW, ViewH, Measure, Mouse, FTCLocale::Language()); }
	// The dialog's box in the last layout (zero size before any)
	static FBox2D DialogBox(const FTCPaint& P);
	// Keep the drawn layout's hits and box (screen pixels) for PressAt
	void Remember(const FTCPaint& P) { Hits = P.Hits; Box = DialogBox(P); }
	const TArray<FTCPaintHit>& LastHits() const { return Hits; }
	// .sheet { overflow: auto }: past max-height the sheet scrolls under its sticky header; a wheel notch scrolls it
	// (Pixels down, negative up), held between the top and the end of the content
	void Scroll(float Pixels) { ScrollY = FMath::Clamp(ScrollY + Pixels, 0.f, MaxScroll); }
	float ScrollOffset() const { return ScrollY; }

private:
	bool bOpen = false;
	TArray<FTCPaintHit> Hits;
	FBox2D Box = FBox2D(ForceInit);
	float ScrollY = 0.f;
	mutable float MaxScroll = 0.f; // the last layout's overflow
};

// The frame-number probe: the overlay's numbers against the engine's 'stat unit' over the same frames.
class FTCStatUnitProbe
{
public:
	// Once per frame. Unit: the viewport's stat unit data while 'stat unit' is shown (null otherwise: nothing is kept).
	// Returns the line to log when the overlay's numbers rolled since the last call (empty otherwise).
	FString Frame(const FTCPerfStats& Perf, const FStatUnitData* Unit, bool bUnitShown);
	// The line itself (tests): the overlay's fps, frame, worst and script time, then stat unit's smoothed Frame, Game,
	// Draw, RHIT and GPU, and the mean and worst of its raw frame times over Count frames.
	static FString Line(const FTCPerfStats& Perf, double UnitFrame, double UnitGame, double UnitDraw, double UnitRhit, double UnitGpu, double RawMean, double RawWorst, int32 Count);

private:
	double LastFps = -1, LastMs = -1, LastWorst = -1, LastCpu = -1;
	double RawSum = 0, RawWorst = 0;
	int32 RawCount = 0;
	bool bPrimed = false;
};
