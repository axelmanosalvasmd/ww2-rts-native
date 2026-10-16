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
class UCanvas;
class UFont;

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

// Round 17 (hudui): the reference's typeface. client/index.html sets every word in 'Barlow Semi Condensed' (weights
// 400, 500, 600 and 700 from the Google Fonts css2 link at the pin; body text 500, bold 600). The native HUD drew the
// sheet with the engine's Roboto at 1.2 em per glyph box, about 15% wider. FTCType reads the same font files the
// browser downloads (Content/Fonts/BarlowSemiCondensed/*.ttf, fetched and checked by unreal/Tools/fonts/fetch-barlow.sh;
// SIL Open Font License, OFL.txt beside them) and measures as Chromium lays text out: each glyph's advance (hmtx) plus
// the font's GPOS pair kerning (the 'kern' feature, PairPos formats 1 and 2), in CSS pixels. Draw places every glyph at
// those positions with the same files as a runtime font, so the drawn width is the measured one. Without the files
// (IsLoaded false) the callers keep the HUD font and its measure.
class THREECROSSROADS_API FTCTypeFace
{
public:
	// The parts of a TrueType file that set widths: unitsPerEm, cmap (format 4 or 12), hmtx, GPOS pair kerning and GSUB 'tnum'
	bool Parse(const TArray<uint8>& Bytes, FString* OutProblem = nullptr);
	bool IsValid() const { return Upm > 0 && Advances.Num() > 0; }
	int32 Glyph(uint32 CodePoint) const { const uint16* G = Cmap.Find(CodePoint); return G ? *G : 0; }
	// The glyph index.html draws: Glyph with 'tnum' applied (body's font-variant-numeric: tabular-nums)
	int32 Shaped(uint32 CodePoint) const { const int32 G = Glyph(CodePoint); const uint16* T = Tabular.Find((uint16)G); return T ? *T : G; }
	// font units
	int32 Advance(int32 Glyph) const { return Advances.Num() ? Advances[FMath::Clamp(Glyph, 0, Advances.Num() - 1)] : 0; }
	int32 Kerning(int32 Left, int32 Right) const;
	int32 UnitsPerEm() const { return Upm; }
	// The pen position before each character and after the last (Out has Len + 1 entries), CSS pixels at FontPx with
	// letter-spacing Tracking after every character
	void Pens(const FString& Text, float FontPx, float Tracking, TArray<float>& Out, bool bTabular = true) const;
	float Width(const FString& Text, float FontPx, float Tracking = 0.f, bool bTabular = true) const;

private:
	struct FPairSub
	{
		uint8 Format = 0;
		TMap<uint16, int32> Coverage;                 // glyph to coverage index
		TArray<TMap<uint16, int16>> Pairs;            // format 1: per coverage index, second glyph to x advance
		TMap<uint16, uint16> Class1, Class2;          // format 2: class definitions (absent: class 0)
		int32 Class1Count = 0, Class2Count = 0;
		TArray<int16> Matrix;                         // format 2: Class1Count x Class2Count x advances
	};
	int32 Upm = 0;
	TMap<uint32, uint16> Cmap;
	TArray<uint16> Advances;
	TArray<TArray<FPairSub>> KernLookups;            // the 'kern' feature's lookups in lookup-list order
	TMap<uint16, uint16> Tabular;                    // GSUB 'tnum': glyph to its tabular form
};

class THREECROSSROADS_API FTCType
{
public:
	// index.html's weights: 500 the body, 600 the bold (legends, labels, key caps); the files' typeface names
	static constexpr int32 BodyWeight = 500, BoldWeight = 600, HeadingWeight = 700; // 700: an h2's default bold (Draw: a ':title' role)
	// The folder the files are read from (Content/Fonts/BarlowSemiCondensed) and the four files' names
	static FString Folder();
	static const TArray<TPair<int32, FString>>& Files();
	// Loads the files once (the first call); false when any is missing or unreadable (logged once)
	static bool IsLoaded();
	static const FTCTypeFace* Face(bool bBold);
	static const FTCTypeFace* HeadingFace();
	static void Pens(const FTCTypeFace& Face, const FString& Text, float Px, float Tracking, TArray<float>& Out, bool bTabular = true);
	static float Width(const FString& Text, float Px, bool bBold = false, float Tracking = 0.f);
	// The runtime font over the same files (typefaces 'Medium' and 'SemiBold'), or null
	static UFont* Font();
	// Chromium's widths: FTCTypeFace::Width with the weight bBold picks
	static FTCMeasure Measure();
	// CSS font-weight 700 (loss heading/detail and the boss name).
	static FTCMeasure HeavyMeasure();
    // Source browser symbols missing from Barlow/Droid are converted into the shared HUD atlas.
    static float HudSymbolAdvance(uint32 CodePoint); // em advance, -1 when not in the atlas
    static bool DrawHudSymbol(uint32 CodePoint, UCanvas* Canvas, FVector2D Pos, float Px, FLinearColor Colour);
    static bool HasGlyph(uint32 CodePoint, bool bBold = false);
	// Paint P on the canvas: every item as FTCPaint::Draw does, texts in this face at the measured glyph positions.
	// Oversample: the canvas's scale (the UI scale inside BeginUi), so the glyphs are made at the size they show.
	static void Draw(const FTCPaint& P, UCanvas* Canvas, UFont* Fallback, float Oversample = 1.f, bool bTabular = true);
};

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
//
// Round 17 (hudui): the windows now line up. perf.js's worst is the longest frame start to frame start in the roll's
// window, the frame that rolled included (its own start minus the one before). stat unit's RawFrameTime is that same
// interval (FApp's current minus last time) but is set in DrawStatsHUD, after the HUD drew, so the HUD read the
// previous frame's value; and the probe cleared its window on a roll before adding the frame that rolled. So a long
// frame that closed a window (the usual case: a hitch is what pushes the span past 1000 ms) counted in the overlay's
// window and in the probe's next one (round 16's "Worst disagrees with stat unit in some windows"). Frame now takes
// this frame's own interval (FApp delta, the value stat unit reads later in the same frame) and adds it before the roll.
class FTCStatUnitProbe
{
public:
	// Once per frame, after ATCGameMode's perf.js frame. Unit: the viewport's stat unit data while 'stat unit' is shown
	// (null otherwise: nothing is kept). FrameMs: this frame's start to start in ms (FApp::GetCurrentTime() minus
	// FApp::GetLastTime(), what stat unit's RawFrameTime becomes this frame); negative: Unit's RawFrameTime (the last
	// frame's, as before). Returns the line to log when the overlay's numbers rolled since the last call (else empty).
	FString Frame(const FTCPerfStats& Perf, const FStatUnitData* Unit, bool bUnitShown, double FrameMs = -1.0);
	// The line itself (tests): the overlay's fps, frame, worst and script time, then stat unit's smoothed Frame, Game,
	// Draw, RHIT and GPU, and the mean and worst of its raw frame times over Count frames.
	static FString Line(const FTCPerfStats& Perf, double UnitFrame, double UnitGame, double UnitDraw, double UnitRhit, double UnitGpu, double RawMean, double RawWorst, int32 Count);

private:
	double LastFps = -1, LastMs = -1, LastWorst = -1, LastCpu = -1;
	double RawSum = 0, RawWorst = 0;
	int32 RawCount = 0;
	bool bPrimed = false;
};
