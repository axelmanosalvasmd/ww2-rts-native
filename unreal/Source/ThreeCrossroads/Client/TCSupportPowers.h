// Off-map support calls and area fire (shelling ground): a port of the reference's controls, availability and aims,
// for the facts this client receives. Ported from the pinned reference (FTCSupportPowers::ReferencePin):
//   shared/sim.js          SUPPORT (with the Classic and World Munitions prices and the plane flag), SUPPORT_TYPES,
//                          supCost, dropPop, isConstructionMode, CFG.paraRange, UNITS[type].w.salvo and .shellTerrain,
//                          UNITS.rocket.w.spread, and command 'support' and 'fireat' (only to predict the server's answer,
//                          never to stop a message);
//   client/availability.js availability({ t: 'support', kind }), cooldownSeconds, resources, population;
//   client/main.js         aimSupport, shellers, startArea, setAim's hints, cancelAim, defaultDir, home (the start's
//                          spawn, then the snapshot's home or world.home), the armed world click for a support kind and
//                          for 'area' (mousedown), the minimap click while targeting, aimShape and the aim preview (frame),
//                          strikeShape and syncStrikes (the public warnings), the minimap's strike outlines;
//   client/orders.js       dispatch, the fireat part of the 'house' branch (guns: shellTerrain or salvo);
//   client/hud.js          buildSupport and drawEcon (the support bar: tips, key badges, cost or cooldown, availability),
//                          SUPPORT_TIP, drawOrders' Shell area button (shown with a salvo unit selected);
//   client/keys.js         support:* (Z, C, V, B, N, U, P, I) and 'area' (Shift+B), match (modifiers exact), badge,
//                          and the recruit and building contexts' card letters that outrank them.
// The server validates and decides every call (shared/sim.js command 'support' and 'fireat'); a refusal comes back as
// { t: 'deny', cmd, reason }, which TCOrders::DenySentence words. PredictedDeny only says what this client expects.
//
// Recipient-known facts only: this seat's snapshot (sup cooldowns, mp, mun, mode, out, home, strikes) and the held unit
// rows (FTCMatchState::Units, the browser's lastSnap.units after client/world-state.js). Nothing is inferred about
// other seats' wallets or cooldowns. Strikes are public and are read as sent.
//
// Selection. Plans read the caller's Selected set and never add to it. Where the browser keeps its selection order (the
// ids of a fireat), the Selected set's iteration order stands in for it.
//
// Data. SUPPORT, the salvo and shellTerrain types, the texts and the key letters are written out in TCSupportPowers.cpp
// from the reference at ReferencePin (each value compared with the reference by a script run from the scratch area; see
// the change notes). Until FTCUnitTable::Reference names the same pin, every availability check refuses with a native
// reason (bNative), so no support call or area fire goes out: a moved pin cannot use stale numbers. The paratroopers'
// army check reads FTCRecruitment (ArmyUse and the recruit table's pop), which must be at its own pin too.
//
// One aim at a time. The browser has a single 'targeting' value: arming a support kind or 'area' replaces any other
// aim (ability, rally, build, dig), and arming any of those replaces this one. FTCSupportAim holds this module's aim;
// the caller keeps it exclusive with its other aims (cancel this one when another arms, and the others when a plan
// here has bArmAim).
//
// Not ported: sounds and voice barks (blip, bark), the Spanish texts, the HUD's look (icons are named, not drawn), the
// operative (FPS) seat, the facing gesture (it never applies to these aims).
#pragma once

#include "CoreMinimal.h"
#include "Client/TCOrders.h"

class FJsonObject;
class FTCMatchState;
class FTCUnitTable;
class FTCRecruitment;
struct FTCUnit;
struct FKey;

// One shared/sim.js SUPPORT entry, the fields the controls read.
struct FTCSupportDef
{
	FString Kind;            // 'recon', 'artillery', 'strafe', 'smoke', 'bombing', 'dive', 'para' or 'cover'
	FString Name;            // 'Recon Flight', ...
	int32 Cost = 0;          // MP price outside Classic and World
	int32 Mun = 0;           // Munitions price in Classic and World
	int32 Cd = 0;            // cooldown seconds after a call
	double Delay = 0;        // seconds of public warning before it lands
	bool bPoint = false;     // one click on the spot; otherwise two clicks: the centre, then the direction
	bool bPlane = false;     // arrives by plane (flak and fighter cover can shoot it down)
	double Len = 0, Width = 0; // the strip (long side along the direction); 0 for point kinds
	double Radius = 0;       // point kinds: the ring client/main.js draws (radius ?? blast ?? 4); 0 for strips
	TArray<FString> Units;   // what a drop brings (para: rifle, rifle, mg)
	FString KeyLabel;        // client/keys.js SUPPORT_KEYS: "Z", "C", ...
	FString Tip;             // the support bar button's title (client/hud.js buildSupport)
};

// availability()'s answer: ok, or the sentence the browser shows. bNative: this client's own reason (a pin mismatch, a
// seat the lobby has not confirmed, an army size it cannot work out), never one the browser would show.
struct FTCSupportCheck
{
	bool bOk = false;
	bool bNative = false;
	FString Reason;
};

// A support bar button (client/hud.js buildSupport and drawEcon). The browser never disables it: a click on a refused
// button explains the reason (FTCSupportPowers::ArmSupport does).
struct FTCSupportButton
{
	FString Kind;            // the click: ArmSupport(Kind)
	FString Icon;            // the glyph name (client/symbols.js: the support kind itself)
	FString Key;             // the badge: "Z", ...
	FString Tip;             // the title when available; a refused button shows Check.Reason instead
	// The value line: "12s" while cooling down, else "150 MP" (or "60 Mun" in Classic and World). Empty before the
	// first snapshot (the browser writes it in drawEcon, which needs one).
	FString Text;
	FTCSupportCheck Check;
};

// drawOrders' Shell area button. Its availability is always ok (the browser has no check for it).
struct FTCAreaButton
{
	bool bShown = false;     // the Orders panel shows and a held selected unit has a salvo weapon (any owner)
	FString Icon;            // 'barrage'
	FString Key;             // the badge: "⇧B"
	FString Tip;
};

// This module's aim: a support kind or 'area' (client/main.js targeting, aimCenter and aimMesh for these kinds).
enum class ETCSupportAimKind : uint8
{
	None,
	Support,   // SupportKind names it; a strip kind takes two clicks (bHasCenter after the first)
	Area,      // shell an area: the next left click on the ground sends a fireat for the selected salvo units
};
const TCHAR* LexToString(ETCSupportAimKind Kind);

struct FTCSupportAim
{
	ETCSupportAimKind Kind = ETCSupportAimKind::None;
	FString SupportKind;                       // Kind == Support: the SUPPORT kind
	bool bHasCenter = false;                   // a strip kind's first click pinned the centre (aimCenter)
	FVector2D Center = FVector2D::ZeroVector;
	FString Hint;                              // the hint line (client/main.js $('hint'))
	// The preview mesh as the browser keeps it between frames: made for one aim kind (MeshKind: the support kind or
	// 'area'), at MeshAt, its long side along MeshDir (three.js rotation.y = -MeshDir). A new kind makes a new mesh at
	// the origin with no rotation.
	FString MeshKind;
	bool bMeshPlaced = false;
	FVector2D MeshAt = FVector2D::ZeroVector;
	double MeshDir = 0;
	bool IsArmed() const { return Kind != ETCSupportAimKind::None; }
	// client/main.js cancelAim (and the next frame, which drops the mesh)
	void Cancel();
};

// One frame of the aim preview (client/main.js frame while targeting a support kind or 'area').
struct FTCSupportPreview
{
	bool bShown = false;      // an aim is armed
	bool bPlaced = false;     // the mesh has been put somewhere (else it sits at the map origin)
	FVector2D At = FVector2D::ZeroVector;
	bool bStrip = false;      // a strip: Len along Dir, Width across, an arrow past the far end (client/main.js aimMarker)
	double Len = 0, Width = 0, Dir = 0;
	double Radius = 0;        // a ring otherwise (point kinds, and 'area': UNITS.rocket.w.spread)
	FColor Color = FColor(0xf4, 0xdc, 0x90);
};

enum class ETCSupportPowerCommand : uint8
{
	None,       // nothing goes out (Note says why when the browser says anything)
	ArmAim,     // nothing is sent: the aim is armed (aimSupport, startArea)
	PinCenter,  // nothing is sent: a strip kind's first click pinned the centre
	Support,    // { t: 'support', kind, x, z } or { t: 'support', kind, x, z, dir }
	FireAt,     // { t: 'fireat', ids, x, z, queue }
};
const TCHAR* LexToString(ETCSupportPowerCommand Kind);

struct FTCSupportPowerPlan
{
	ETCSupportPowerCommand Kind = ETCSupportPowerCommand::None;
	TSharedPtr<FJsonObject> Message;   // the one message to send; null when nothing goes out
	int32 Units = 0;                   // FireAt: ids named
	FString SupportKind;               // the support kind ('area' for area fire)
	// Where the browser puts its order marker: a point support at the click (0xffa030), a fireat at the click
	// (0xff4030). A strip support gets no marker (bMarker false; At is its centre).
	bool bMarker = false;
	FVector2D At = FVector2D(-1, -1);
	FColor Color = FColor(0xff, 0xa0, 0x30);
	double Dir = 0;                    // Support of a strip kind: the dir sent
	// The aim after this plan (the plan functions update FTCSupportAim themselves; these say what changed).
	bool bArmAim = false;              // the aim was armed (replace any other aim the caller holds)
	bool bCancelAim = false;           // the aim ended
	bool bKeepAim = false;             // the click was refused (no ground, or availability): the aim stays as it was
	// What the browser shows. bFeedback: a sentence flashed over the hint for 2 s with the error sound (feedback.show);
	// otherwise Note is the new hint line, or this client's own remark (bNative). Empty when the browser stays silent.
	FString Note;
	bool bFeedback = false;
	bool bNative = false;
	// The deny reason this client expects from shared/sim.js command ('cooldown', 'mp', 'mun', 'pop', 'airfield',
	// 'blocked'), empty when acceptance is expected or the client cannot tell. Only a prediction: the plan is sent
	// regardless, as the browser sends it. Not predicted: 'unseen' (the server's teamSees, which the fog this client
	// gets does not reproduce exactly), 'notVisible' and 'needs' for a fireat (they need the cell's health), and full
	// order queues ('queueFull').
	FString PredictedDeny;

	// The same message as an FTCOrderPlan for ATCPlayerController::SendPlan (Kind None: ETCOrderKind has no entry).
	// At is (-1, -1) when the browser puts no marker.
	FTCOrderPlan ToOrderPlan() const;
};

// One public strike warning (snapshot strikes: [kind, x, z, dir, t, owner]) with the shape client/main.js strikeShape
// gives it. At this pin only SUPPORT kinds enter the list (shared/sim.js command 'support').
struct FTCStrikeMark
{
	FString Kind;
	FVector2D At = FVector2D::ZeroVector;
	double Dir = 0;           // the strip's long side runs along (cos Dir, sin Dir) in map x and z
	double T = 0;             // seconds until it lands (0 once it is live); the browser animates the mark while T > 0
	int32 Owner = -1;
	bool bFoe = false;        // client/main.js foe(owner): drawn 0xd2321e (minimap '#ff3020'), else in the owner's colour
	bool bKnownKind = false;  // a SUPPORT kind; otherwise strikeShape's fallback ring of 4 m is given
	bool bStrip = false;
	double Len = 0, Width = 0, Radius = 0;
	bool bMinimap = false;    // the minimap outlines it (only kinds with a len: the strip kinds)
	FString Key;              // syncStrikes' identity, `${kind},${x},${z}`: a mark is kept while the same key comes
};

// One key chord of this module (client/keys.js, 'global' context, modifiers must match exactly).
enum class ETCSupportPowerKey : uint8
{
	None,
	Support,   // Z C V B N U P I: aimSupport(kind)
	Area,      // Shift+B: startArea
};

struct FTCSupportKeyMatch
{
	ETCSupportPowerKey Action = ETCSupportPowerKey::None;
	FString Kind;              // Support: the kind
	// The recruit and building contexts outrank this chord with a card letter: card N (Shift: cardMany N, five) in
	// client/keys.js order QWERTASDFGZXCVB. 0: no card shares the chord. client/main.js keydown falls back to this
	// module's action when hud.hasCard(N) is false, so the caller asks that first.
	int32 Card = 0;
};

class FTCSupportPowers
{
public:
	// The reference commit the support table was written from.
	static const TCHAR* const ReferencePin;
	// The kind's entry; null for anything not in SUPPORT.
	static const FTCSupportDef* Find(const FString& Kind);
	// Every kind, in SUPPORT_TYPES order (the support bar's order).
	static const TArray<FTCSupportDef>& All();
	// UNITS[type].w?.salvo (mortar, howitzer, rocket truck, destroyer, bomber) and .shellTerrain (the tanks).
	static bool IsSalvo(const FString& Type);
	static bool ShellsTerrain(const FString& Type);
	// client/main.js setAim hints (with the ". Right-click cancels" every aim gets), the second-click hint of a strip
	// kind, startArea's sentence with no shellers, and the Shell area button's texts.
	static const TCHAR* CenterHint();      // a support kind: "Click to set the center. Right-click cancels"
	static const TCHAR* RotateHint();      // after a strip kind's first click: "Move the mouse to rotate, then click to launch"
	static const TCHAR* AreaHint();
	static const TCHAR* NoShellersHint();
	static const TCHAR* AreaLabel();       // client/keys.js label('area'): "Shift+B"
	static const TCHAR* AreaBadge();       // badge('area'): "⇧B"
	static double AreaRadius();            // aimShape('area'): UNITS.rocket.w.spread
	// client/keys.js match for this module's chords. bCtrl: Ctrl or Command.
	static FTCSupportKeyMatch KeyAction(const FKey& Key, bool bShift, bool bCtrl, bool bAlt);

	// Message hooks, called next to FTCMatchState's. A start sets home to its spawn and forgets the last snapshot; a
	// snapshot before any start is ignored. Clear when the match ends.
	void ApplyStart(const FJsonObject& M);
	void ApplySnapshot(const FJsonObject& M);
	void Clear();
	bool HasSnapshot() const { return bHasSnapshot; }
	// client/main.js applySnapshot: if (s.out?.[me] && !lastSnap?.out?.[me]) ... cancelAim(). True for the snapshot in
	// which Seat went out (the first snapshot of a match counts when it already shows the seat out).
	bool WentOut(int32 Seat) const;

	// The snapshot's facts this module reads.
	bool ConstructionMode() const;                    // shared/sim.js isConstructionMode: mode.kind 'classic' or 'world'
	double Cooldown(const FString& Kind) const;       // s.sup?.[kind] ?? 0
	TOptional<FVector2D> Home() const { return HomeAt; } // client/main.js home (null before a start with a spawn)
	// shared/sim.js supCost of the snapshot: the price and whether it is Munitions (Classic and World) or MP.
	int32 Cost(const FString& Kind, bool* bOutMun = nullptr) const;

	// client/availability.js availability({ t: 'support', kind }).
	FTCSupportCheck Check(const FTCMatchState& State, const FTCUnitTable& Table, const FTCRecruitment& Recruit, const FString& Kind) const;
	// client/hud.js the support bar, in SUPPORT_TYPES order.
	TArray<FTCSupportButton> Buttons(const FTCMatchState& State, const FTCUnitTable& Table, const FTCRecruitment& Recruit) const;

	// client/main.js aimSupport (a support bar click or its key): refused with the sentence (bFeedback; any aim held stays
	// as it was), or the aim armed for Kind with CenterHint (replacing this module's aim, a centre included).
	FTCSupportPowerPlan ArmSupport(const FTCMatchState& State, const FTCUnitTable& Table, const FTCRecruitment& Recruit, const FString& Kind, FTCSupportAim& Aim) const;
	// client/main.js startArea (Shift+B, the Shell area button): with a selected own salvo unit the aim is armed with
	// AreaHint; otherwise the hint line says NoShellersHint and the aim is left as it was. Nothing while observing.
	FTCSupportPowerPlan ArmArea(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, FTCSupportAim& Aim) const;
	// client/main.js mousedown with this module's aim armed, the left button on the world (the right button and Esc
	// only cancel: FTCSupportAim::Cancel). No ground keeps the aim with the 'blocked' sentence. A support kind: a point
	// kind sends at the click; a strip kind pins the centre on the first click and sends on the second, its dir toward
	// the click (or from home, within 1.5 m of the centre); either keeps the aim with the sentence when availability
	// refuses. Area: the selected own salvo units shell the click (queue: bShift); without Shift the aim ends (even
	// when none is selected), with Shift it stays armed. The minimap never calls in support or shells: with this aim
	// armed its left button moves the camera and its right button cancels.
	FTCSupportPowerPlan AimClick(const FTCMatchState& State, const FTCUnitTable& Table, const FTCRecruitment& Recruit, const TSet<int32>& Selected, FTCSupportAim& Aim, bool bHasGround, FVector2D Ground, bool bShift) const;
	// client/main.js frame: the preview under the pointer, updating Aim's mesh state as the browser's mesh keeps it.
	FTCSupportPreview Preview(FTCSupportAim& Aim, bool bHasGround, FVector2D Ground) const;

	// client/hud.js drawOrders' Shell area button for the selection.
	FTCAreaButton AreaButton(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const;
	// client/main.js shellers: the selected own units with a salvo weapon, in the Selected set's order.
	TArray<int32> Shellers(const FTCMatchState& State, const TSet<int32>& Selected) const;
	// client/orders.js dispatch, the fireat part of a right-click on a house (kind() said 'house'; the caller found the
	// house and passes its cell centre): { t: 'fireat', ids, x, z } for the selected own troops whose weapon shells
	// terrain or fires salvos, without queue (the caller adds it, or folds this into { t: 'orders', queue, commands }
	// with the garrison and move parts). Null when there are none. OutGuns: how many were named.
	TSharedPtr<FJsonObject> HouseFireCommand(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, FVector2D HouseCenter, int32* OutGuns = nullptr) const;

	// The public strike warnings of the last snapshot, with their shapes.
	TArray<FTCStrikeMark> Strikes(const FTCMatchState& State) const;

private:
	bool bStarted = false, bHasSnapshot = false;
	TOptional<double> SnapMp, SnapMun;
	FString ModeKind;
	TArray<bool> OutSeats, PrevOutSeats;
	TMap<FString, double> Sup;
	TOptional<FVector2D> HomeAt;
	struct FStrikeRow { FString Kind; double X = 0, Z = 0, Dir = 0, T = 0; int32 Owner = -1; };
	TArray<FStrikeRow> StrikeRows;
	// every held airfield row's built as sent (the raw double; FTCUnit::Built is a float), for the World paratroopers
	TMap<int32, double> AirfieldBuilt;

	FTCSupportCheck Gate(const FTCMatchState& State, const FTCUnitTable& Table) const;
	double BuiltOf(const FTCUnit& U) const;
	FString PredictSupport(const FTCMatchState& State, const FTCRecruitment& Recruit, const FTCSupportDef& Def, FVector2D At) const;
	double DefaultDir(FVector2D At) const;
};
