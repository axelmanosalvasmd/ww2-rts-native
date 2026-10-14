// The keyboard: a port of the reference's client/keys.js table (every binding with its DOM code, label, help line,
// context and exact modifiers), its match() with the context ranks (targeting over the card letters over the base
// table), the F1 Controls sheet's text (client/index.html #controlsSheet) and the selection the keys make
// (client/selection.js army, idle and findIdle, doubleClick). Pure client logic: nothing here reads the socket, and the
// selection helpers read only what FTCKeyUnit carries, which the controller fills from what this seat's snapshot holds.
// Interim until the Rust core supplies input tables through its C ABI (IF-13).
#pragma once

#include "CoreMinimal.h"

struct FKey;

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
