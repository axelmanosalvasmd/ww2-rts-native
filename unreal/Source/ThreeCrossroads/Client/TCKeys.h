// The keyboard: a port of the reference's client/keys.js table (every binding with its DOM code, label, help line,
// context and exact modifiers), its match() with the context ranks (targeting over the card letters over the base
// table), the F1 Controls sheet's text (client/index.html #controlsSheet) and the selection the keys make
// (client/selection.js army, idle and findIdle, doubleClick). Pure client logic: nothing here reads the socket, and the
// selection helpers read only what FTCKeyUnit carries, which the controller fills from what this seat's snapshot holds.
// Interim until the Rust core supplies input tables through its C ABI (IF-13).
//
// Round 15 (alerts scope): F1, F2 and M exactly as client/main.js's keydown binds them (TCKeys::Route): F1 and F2 are
// read before everything while the match HUD shows (F1 toggles the Controls sheet, also on a key repeat; F2 toggles the
// stats overlay, not on a repeat); while the Controls sheet or the stats sheet is open every other key stops there;
// the rest goes through the table, where M ('mute', any context, no modifier: Shift+M is 'fort:demine') toggles the
// audio, also in the lobby (the browser's handler does not ask for a match), never on a repeat. F2's target is ported
// here with it: client/stats.js (FTCStatsOverlay: the stats, presets, positions, layouts, sizes, backgrounds and the
// amber and red warnings, the box and its placement under the HUD's panels, saved as 'ww2-stats' JSON) and
// client/perf.js (FTCPerf: the one-second roll of the frame, network and ping numbers). The stats sheet (Menu > Stats
// overlay) is not drawn in this client: its open state is an input here, false until it exists.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCEndReport.h"

struct FKey;
class FJsonObject;

// client/keys.js contexts: 'global' always; 'army' outside Classic or 'classic' in it; the overlays 'targeting' (an aim
// is armed), 'recruit' (recruit mode) and 'building' (Classic with a finished own Production Building's lettered cards).
enum class ETCKeyContext : uint8 { Global, Army, Classic, Targeting, Recruit, Building };
const TCHAR* LexToString(ETCKeyContext Context);

struct FTCKeyBinding
{
	FString Id;     // 'stop', 'stance:holdFire', 'card:3', 'group:set:4', ...
	FString Code;   // the DOM KeyboardEvent.code: 'KeyX', 'Digit1', 'Space', 'Period', ...
	FString Label;  // what a button's key badge says: 'X', 'Shift+B', 'Ctrl+A'
	FString Help;
	ETCKeyContext Context = ETCKeyContext::Global;
	bool bShift = false, bCtrl = false, bAlt = false; // exactly these modifiers (Ctrl: Ctrl or Command)
};

// One line of the Controls sheet: the keys or gesture, and what it does.
struct FTCSheetRow { FString Keys, Text; };
struct FTCSheetSection { FString Title; TArray<FTCSheetRow> Rows; };

namespace TCKeys
{
	// client/keys.js bindings, in its order (173 at the pin).
	const TArray<FTCKeyBinding>& Bindings();
	// client/keys.js rank: targeting 2, recruit and building 1, the rest 0.
	int32 Rank(ETCKeyContext Context);
	// client/keys.js match: the highest-ranked binding among the active contexts (global is always active) whose code
	// and three modifiers all agree; the first such in table order on a tie. Null when none.
	const FTCKeyBinding* Match(const FString& Code, bool bShift, bool bCtrl, bool bAlt, TConstArrayView<ETCKeyContext> Active);
	// client/keys.js label and badge (a leading 'Shift+' drawn as the shift arrow).
	FString Label(const FString& Id);
	FString Badge(const FString& Id);
	// The DOM code of an engine key ('KeyA' for EKeys::A, 'Backquote' for EKeys::Tilde, 'F1'), empty when keys.js
	// names no such code and it is not F1 or F2; KeyOf is the reverse.
	FString CodeOf(const FKey& Key);
	FKey KeyOf(const FString& Code);
	// Every engine key that some binding names, once, plus F1 and F2 (the keys the controller polls).
	const TArray<FKey>& PolledKeys();
	// client/index.html #controlsSheet: the four sections (Mouse, Camera, Units and orders, Recruiting and building).
	const TArray<FTCSheetSection>& ControlsSheet();
	// The table as one text, a line per binding ('id|code|label|help|context|shift ctrl alt'), and the FNV-1a hash of
	// a text's UTF-8 bytes: the automation test compares both with the same computed from keys.js.
	FString TableText();
	uint32 Fnv1a(const FString& Text);
}

// client/main.js keydown, before the table: what a key press does.
enum class ETCKeyRoute : uint8
{
	ControlsSheet,  // F1 while the match HUD shows: open or close the Controls sheet
	StatsToggle,    // F2 while the match HUD shows, not a repeat: stats.toggle()
	Swallowed,      // F2 repeated, or any key while the Controls sheet or the stats sheet is open
	Table,          // through keys.js match (a repeated key then does nothing: main.js returns on e.repeat)
};

namespace TCKeys
{
	// Code: the DOM code; bHudShown: the match HUD is shown (#hud not hidden); bRepeat: the browser's e.repeat.
	ETCKeyRoute Route(const FString& Code, bool bHudShown, bool bRepeat, bool bControlsOpen, bool bStatsSheetOpen);
	// A table binding that runs without a match (the lobby): only 'mute' does anything there in this client.
	bool RunsInLobby(const FString& Id);
}

// client/perf.js stat: the numbers of the last one-second roll (null in JavaScript: an unset optional).
struct FTCPerfStats
{
	double Fps = 0, Ms = 0, Worst = 0, Cpu = 0;
	int64 Calls = 0, Tris = 0;
	int32 Units = 0, Fx = 0, Corpses = 0;
	TOptional<double> Mem, Ping, Jitter, Loss, SrvTick, SrvEvery;
	double Snaps = 0, Kbs = 0, Gap = 0;
	int32 Bytes = 0;
	double Scale = 1;
	FString Gfx = TEXT("high");
};

// client/perf.js: frames, snapshots and latency pings rolled once a second. Times in milliseconds on one clock.
class FTCPerf
{
public:
	static constexpr double LostAfter = 3000.0; // LOST_AFTER
	static constexpr int32 Pings = 30;          // PINGS
	// perf.net(n): one snapshot of n bytes arrived
	void Net(int32 Bytes, double NowMs);
	// perf.pinged(c): a latency ping stamped c went out
	void Pinged(double C);
	// perf.pong(m): the round trip for a pong this client sent (m.c), and the server's [p95 tick ms, snapEvery] (m.srv)
	TOptional<double> Pong(const FJsonObject& M, double NowMs);
	// perf.frame: once per frame. StartMs: this frame's start (start to start is the frame, waits included); BusyMs:
	// the game code's time in it; the counts of this frame. True when the second rolled (Stats() changed).
	bool Frame(double StartMs, double NowMs, double BusyMs, int64 Calls, int64 Tris, int32 Units, int32 Fx, int32 Corpses);
	const FTCPerfStats& Stats() const { return Stat; }
	FTCPerfStats& MutableStats() { return Stat; }
	void Reset() { *this = FTCPerf(); }

private:
	FTCPerfStats Stat;
	int32 Frames = 0, Snaps = 0;
	double Busy = 0, Worst = 0, LastStart = 0, Bytes = 0, GapMs = 0, LastSnap = 0, Since = -1;
	TMap<double, double> Out; // c -> sent
	TArray<bool> Done;        // answered (false) or lost (true), the last PINGS
	TArray<double> Rtts;
	void RollPings(double NowMs);
};

// client/stats.js STATS: one number of the overlay.
struct FTCStatDef
{
	const TCHAR* Group;
	const TCHAR* Id;
	const TCHAR* Tag;   // the short name in the box
	const TCHAR* Label; // the sheet's name for it
	const TCHAR* Hint;  // the sheet's hint, or null
};

struct FTCStatsSettings
{
	bool bShow = false;
	TArray<FString> Stats = { TEXT("fps"), TEXT("ping"), TEXT("loss") }; // PRESETS.Basic
	FString Pos = TEXT("tl"), Layout = TEXT("list"), Size = TEXT("m"), Bg = TEXT("panel");
	bool bWarn = true;
};

class FTCStatsOverlay
{
public:
	static const TArray<FTCStatDef>& Defs();
	// PRESETS: Basic, Performance, Network, Everything
	static TArray<FString> Preset(const FString& Name);
	// stats.js load(storage): the saved JSON (or none) over DEFAULTS; bForced: '?perf' (show every number, save nothing)
	void Load(const TOptional<FString>& Saved, bool bForced = false);
	// JSON.stringify(settings), what save() writes (an empty optional when forced: nothing is saved)
	TOptional<FString> SaveText() const;
	// the F2 key and the sheet's switch: settings.show = !settings.show, then save
	void Toggle() { S.bShow = !S.bShow; }
	const FTCStatsSettings& Settings() const { return S; }
	FTCStatsSettings& MutableSettings() { return S; }
	// The menu button's text: 'Stats overlay: On' or 'Off'
	FString MenuLabel() const { return FString::Printf(TEXT("Stats overlay: %s"), S.bShow ? TEXT("On") : TEXT("Off")); }
	// The box's rows: the tag, tr(show(s)) and the warning (0 fine, 1 amber, 2 red; 0 with warnings off).
	struct FRow { FString Tag, Value; int32 Warn = 0; };
	TArray<FRow> Rows(const FTCPerfStats& Stat) const;
	// show(s) in English for one stat id, and its warn(s) band
	static FString Show(const FString& Id, const FTCPerfStats& Stat);
	static int32 Warn(const FString& Id, const FTCPerfStats& Stat);
	// The box as stats.css draws it, in UI pixels: under the panel its position belongs to (TopTl, TopTc, TopTr: those
	// panels' bottoms, or a negative value for none: 52 px), dropped below the score strip when it would run into it.
	FTCPaint Layout(const FTCPerfStats& Stat, float TopTl, float TopTc, float TopTr, const FBox2D& Strip, float LeftOfTr, float RightOfTl, float ViewW, const FTCMeasure& Measure) const;

private:
	FTCStatsSettings S;
	bool bForcedAll = false;
};

// One held unit as the selection keys see it (client/selection.js reads v.owner, v.type, v.hp, v.flags, v.plan and
// UNITS[v.type]). The controller fills it from FTCMatchState, its unit table, the drawn positions and the screen.
struct FTCKeyUnit
{
	int32 Id = 0;
	FString Type;
	bool bOwn = false;          // FTCMatchState::IsOwn
	int32 Hp = 0, Flags = 0;
	bool bDef = false;          // the unit table knows the type
	bool bStructure = false, bBuilding = false, bRail = false, bAir = false;
	// The kind of the unit's plan row (shared/sim.js planOf: 0 none, 1 move, ... 9 board), -1 when no row is known.
	int32 PlanKind = -1;
	// Drawn (not garrisoned, riding or parked) with a point in front of the camera inside the viewport.
	bool bOnScreen = false;
	FVector2D At = FVector2D::ZeroVector; // where it is drawn (reference metres)
};

namespace TCSelectionKeys
{
	// client/selection.js: own (ours and alive), mobileOwn (not a structure, not a plane at base, not riding),
	// armyUnit (not a structure, train or supply truck), selectable (army and mobile), selectableOwn (mobile or an own
	// building).
	bool Own(const FTCKeyUnit& U);
	bool MobileOwn(const FTCKeyUnit& U);
	bool ArmyUnit(const FTCKeyUnit& U);
	bool Selectable(const FTCKeyUnit& U);
	bool SelectableOwn(const FTCKeyUnit& U);
	// client/selection.js army (Ctrl+A): every selectable unit, in the order given.
	TArray<int32> Army(TConstArrayView<FTCKeyUnit> Units);
	// client/selection.js idle: army ground units (no aircraft; with bEngineers only 'engineer') that are not
	// retreating, digging, garrisoned, building or riding (flags 1, 16, 32, 128, 262144) and whose plan is none (no row,
	// or kind 0); by id.
	TArray<int32> Idle(TConstArrayView<FTCKeyUnit> Units, bool bEngineers);
	// client/selection.js doubleClick: every selectableOwn unit of the picked unit's type, on the screen or (bMapWide,
	// Ctrl or Command) on the whole map. False, and Out untouched, when the picked unit is not selectableOwn.
	bool DoubleClick(TConstArrayView<FTCKeyUnit> Units, int32 Picked, bool bMapWide, TArray<int32>& Out);
	// client/main.js centerSelection: the mean drawn position of these units. False for none.
	bool Centre(TConstArrayView<FTCKeyUnit> Units, const TArray<int32>& Ids, FVector2D& Out);
}

// client/selection.js findIdle: '.' and ',' walk their own cursor through the idle list; Shift+'.' takes all of it.
struct FTCIdleResult
{
	bool bReplace = false;   // the selection becomes Ids (an empty findIdle(all) clears it; an empty walk leaves it)
	bool bCentre = false;    // the camera centres on Ids (the walk's one unit)
	TArray<int32> Ids;
};

class FTCIdleCursor
{
public:
	FTCIdleResult Find(TConstArrayView<FTCKeyUnit> Units, bool bAll, bool bEngineers);
	// client/selection.js reset (a new match)
	void Reset() { Army = Engineers = -1; }

private:
	int32 Army = -1, Engineers = -1;
};

// The browser's dblclick for the left button: a second click soon after the first and near it. Browsers take both
// limits from the desktop's settings; these are common defaults (500 ms, and within 5 px on each axis), not values
// read from this machine's settings.
class FTCDoubleClick
{
public:
	static constexpr double Seconds = 0.5;
	static constexpr double Pixels = 5.0;
	// A completed left click (a press and release that was not a box) pressed at At at PressTime (seconds). True when
	// it completes a double-click; a third click then starts a new pair.
	bool Click(double PressTime, FVector2D At);
	void Reset() { bHas = false; }

private:
	bool bHas = false;
	double Time = 0;
	FVector2D Where = FVector2D::ZeroVector;
};
