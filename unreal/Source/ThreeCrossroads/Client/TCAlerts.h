// Alerts worked out from two snapshots in a row, and the map pings (parity gap 7: CLI-85, CLI-60; the history of
// CLI-86 as alert-history.js keeps it).
//
// Ported from the pinned reference client (reference/ww2-rts at e4e579a):
//   client/alerts.js        snapshot (say lines; your units and buildings, an allied HQ or bunker under attack; base
//                           under attack; capture points won, lost, under attack and cut off; units lost, merged within
//                           2 s and 30 m; enemy air incoming; a unit out of a queue, a building finished), push (four
//                           routine lines and two losses, each kind's life, the sound gap), frame, newest, drawPings;
//   client/alert-history.js the match-local history (100 entries, newest first; a duplicate or older tick never
//                           raises alerts again; a new match id empties it);
//   client/pings.js         receive (a teammate's { t: 'ping', from, x, z } relay: a ring for 4 s and a "Ping from"
//                           line), send ({ t: 'ping', x, z }), the ring's look in the world and on the minimap;
//   client/main.js          the hooks alerts.init gets (friend, unit names by faction, player names, point positions,
//                           home) and when alerts.snapshot runs (not for a spectator or a seat that is out).
// Not here: World region alerts (FTCWorldRecovery raises them) and logistics alerts (FTCLogistics). The point block
// is ported and can be switched off where the game mode already raises point alerts (bPointAlerts). Since round 15
// those alerts join this feed's lines and history through Mirror (ATCGameMode::PushAlert), so the feed draws all of
// them as alerts.js does.
//
// Round 15 (the presentation, client/alerts.css with alerts.js push, frame, drawPings, place and the history box):
//   - #alerts sits over the minimap's top right (its frame's 5 px and an 8 px gap above it), a column aligned right
//     with 3 px gaps: the loss column (#lossAlerts, newest first: "UNIT LOST", the detail at 20 px, the hint "Space or
//     click: look there", 380 px wide in deep red with a light left edge), then the routine lines newest first (the
//     newest semi-bold with the stronger border; while a loss shows only two of them), then the Alert history box;
//   - each kind's colour (light red for attack, unit lost, point lost and air; brass for a point won; the base alert
//     white and bold on red, pulsing four half-cycles), the routine line's ellipsis at 320 px (loss details wrap), the 0.18 s (0.22 s for a loss)
//     slide-in, each kind's life (event 15 s, base 10 s, unit lost 12 s, the rest 6 s) and the 0.6 s fade;
//   - a click on a line looks at its spot (hooks.jump), on the history's summary opens or closes it, on Previous
//     Alert or Next Alert navigates (navigate(1), navigate(-1)), on an entry selects and visits it;
//   - the minimap pings (drawPings): a ring spreading from 4 to 18 px every 1.2 s and a steady 4 px mark, dark under
//     the kind's colour, fading with the line;
//   - texts: each element's text through TC_T as the page's observer translates it (the heading after tr(), then
//     upper-cased by CSS text-transform).
//
// Privacy: only what this recipient's snapshots carry: its own and allied unit rows, shots that name its side, public
// points and strikes. Nothing under fog raises an alert, because nothing under fog is in the rows compared. A unit
// that leaves the rows counts as lost only if it was this seat's own (own units are always sent while they exist).
#pragma once

#include "CoreMinimal.h"
#include "Net/TCMatchState.h"
#include "Client/TCWorldRecovery.h"
#include "Client/TCEndReport.h"

class FJsonObject;
class FTCUnitTable;

// client/alerts.js constants
namespace TCAlertRules
{
	constexpr double Life = 6.0;          // LIFE: seconds a line stays up
	constexpr double Fade = 0.6;          // FADE: the last part of that it spends fading out
	constexpr int32 MaxLines = 4, MaxLosses = 2;
	constexpr double AttackEvery = 20.0;  // "under attack" at most once per this many seconds per area
	constexpr double Area = 30.0;         // metres: the same area (groups hits and losses)
	constexpr double Merge = 2.0;         // seconds: a loss this soon after another nearby one joins its line
	constexpr double SoundGap = 1.5;
	constexpr double AirMargin = 25.0;
	constexpr double HomeRadius = 20.0;   // a retreating unit that vanishes this close to home went home
	constexpr double BaseRadius = 40.0;   // a hit this close to the spawn, or on an own HQ or bunker: base under attack
	constexpr int32 HistoryLimit = 100;
	// LIVES: event 15 s, base 10 s, unitLost 12 s, the rest LIFE
	double LifeOf(const FString& Kind);
	// COLOR: the minimap ping colour per kind (signal red, brass, chalk)
	FLinearColor ColourOf(const FString& Kind);
}

// One alert raised: what the game mode's feed gets (ATCGameMode::PushAlert) and what the history records.
struct FTCAlertPush
{
	FString Kind;             // 'attack', 'base', 'unitLost', 'pointWon', 'pointLost', 'air', 'ready', 'event', 'ping'
	FString Text;
	bool bHasAt = false;      // a 'say' line may come without a place
	FVector2D At = FVector2D::ZeroVector;
	int32 N = 1;              // units a loss line counts
	double Time = 0.0;        // match seconds (alert-history.js time)
};

// A line showing (alerts.js lines): routine lines and the loss column, newest first.
struct FTCAlertLine
{
	int32 Serial = 0;
	FTCAlertPush Alert;
	double Born = 0.0;        // the alert clock's seconds
	bool IsLoss() const { return Alert.Kind == TEXT("unitLost"); }
};

struct FTCAlertHistoryEntry
{
	int32 Id = 0;
	FString Kind, Text;
	bool bHasAt = false;
	FVector2D At = FVector2D::ZeroVector;
	double Time = 0.0;
};

// client/alert-history.js
class FTCAlertHistory
{
public:
	explicit FTCAlertHistory(int32 InLimit = TCAlertRules::HistoryLimit) : Limit(InLimit) {}
	// A new match id empties it (false: the same match, kept). MatchId < 0: unknown, always a new one.
	bool Start(int32 MatchId);
	void Reset();
	// False for a tick not after the last accepted one.
	bool AcceptTick(int32 Tick);
	const FTCAlertHistoryEntry& Add(const FTCAlertPush& A);
	const FTCAlertHistoryEntry* Select(int32 Id);
	// +1 the older, -1 the newer, among the entries with a place; from the newest when nothing is selected.
	const FTCAlertHistoryEntry* Navigate(int32 Direction);
	const TArray<FTCAlertHistoryEntry>& Entries() const { return List; }
	int32 Selected() const { return Cursor; }

private:
	TArray<FTCAlertHistoryEntry> List; // newest first
	int32 Limit, Serial = 0, Cursor = 0, Match = INT32_MIN, LastTick = -1;
	bool bMatchKnown = false;
};

// A relayed ping's ring (client/pings.js).
struct FTCPingRing
{
	double X = 0.0, Z = 0.0, Born = 0.0;
	int32 From = -1;
};
struct FTCPingLook
{
	float Radius = 2.f;       // metres: the world ring (clickRing radius 1, scaled 2 + phase * 6)
	float Opacity = 0.f;      // fade * (1 - phase) * 0.9
	float MinimapPx = 4.f;    // the minimap ring's radius in pixels (4 + phase * 14)
	float MinimapAlpha = 0.f; // fade * (1 - phase)
};

class FTCPings
{
public:
	static constexpr double Life = 4.0, FadeTime = 0.6, Period = 1.2;
	// client/pings.js send: { t: 'ping', x, z }, or null for a spot that is not a finite number.
	static TSharedPtr<FJsonObject> MakeSend(double X, double Z);
	// client/pings.js receive's checks: from an integer seat >= 0, x and z finite. Adds the ring.
	bool Receive(const FJsonObject& M, double Now, FTCPingRing& Out);
	void Frame(double Now);
	void Reset() { RingList.Reset(); }
	const TArray<FTCPingRing>& Rings() const { return RingList; }
	bool LookOf(const FTCPingRing& R, double Now, FTCPingLook& Out) const;
	bool Active(double Now) const;

private:
	TArray<FTCPingRing> RingList;
};

// alerts.css and alerts.js place(): sizes in CSS pixels
namespace TCAlertLook
{
	constexpr float MinimapFrame = 5.f, MinimapGap = 8.f;      // place(): clear the minimap's 5 px frame, then 8 px
	constexpr float BoxMax = 380.f, BoxSide = 20.f, Gap = 3.f; // #alerts max-width min(380px, vw - 20px), gap 3px
	constexpr float FontPx = 14.f, LineHeight = 1.35f, PadX = 10.f, PadY = 3.f, LineMax = 320.f;
	constexpr float LossW = 380.f, LossSide = 24.f, LossGap = 6.f, LossBottom = 8.f, LossPadX = 14.f, LossPadY = 10.f, LossEdge = 4.f;
	constexpr float HeadingPx = 13.f, HeadingTrack = 0.1f, DetailPx = 20.f, DetailLine = 1.15f, HintPx = 12.f, HintTop = 3.f, LossInner = 2.f;
	constexpr float HistoryW = 320.f, HistoryPadY = 4.f, HistoryPadX = 10.f, NavPad = 4.f, NavGap = 4.f, ButtonPadX = 10.f, ButtonPadY = 5.f;
	constexpr float ListPad = 4.f, EntryPad = 5.f, EntryMargin = 2.f, ListMax = 250.f, ListVh = 0.30f;
	constexpr double SlideIn = 0.18, LossSlideIn = 0.22, BasePulse = 0.9; // alert-in, loss-in, alert-base (4 alternate)
	constexpr int32 LinesWithLoss = 2;  // #alerts:has(> #lossAlerts:not(:empty)) > .alert:nth-of-type(n+4) { display: none }
}

// Where the feed is drawn and what the pointer is over (UI pixels; the clock is the alert clock).
struct FTCAlertViewInput
{
	double Now = 0.0;
	float Right = 0.f;        // the minimap's right edge
	float MinimapTop = 0.f;   // the minimap's top edge (the box sits 13 px above it)
	float ViewW = 1280.f, ViewH = 720.f;
	float UiScale = 1.f;      // CSS zoom, for device-pixel borders and 1/64 px line boxes
	// Native presentation correction: keep whole cards inside the viewport and clear of existing HUD panels.
	TArray<FBox2D> Avoid;
	bool bMouse = false;
	FVector2D Mouse = FVector2D::ZeroVector; // :hover
	FString Focus;            // the hit tag the keyboard (Alt+Left/Right) is on, drawn with the brass outline
};

// One minimap mark of a line (drawPings): world metres, a radius in minimap pixels.
struct FTCAlertRing
{
	FVector2D At = FVector2D::ZeroVector;
	float RadiusPx = 4.f, Alpha = 1.f;
	FLinearColor Colour = FLinearColor::White;
};

// Where alerts are raised from and how the screen is asked (the hooks alerts.init gets).
struct FTCAlertContext
{
	double Now = 0.0;                                   // the alert clock (seconds), as alerts.js now()
	TFunction<bool(double X, double Z)> OnScreen;       // QUIET_ON_SCREEN: no "under attack" for a spot on screen
	TCWorldRecoveryCore::FAttackQuiet* Quiet = nullptr; // the throttle shared with other under-attack alerts; null: own
	const FTCUnitTable* Table = nullptr;                // UNITS (names, structure, building, infantry)
};

class FTCAlerts
{
public:
	// The capture point block of alerts.js. ATCGameMode::TakePointAlerts raises those already in the live client.
	bool bPointAlerts = true;

	// main.js startGame: alerts.startMatch(matchId) and lastSnap = null; home from the start's spawn.
	void StartMatch(const FTCMatchState& S);
	// alerts.js reset (and the pings').
	void Reset();
	// main.js applySnapshot: alerts.snapshot(s, lastSnap) for a confirmed seat that is not out, then lastSnap = s.
	// Raw: the same snapshot message (its strikes, queues, home and the say lines' text). Returns the alerts raised,
	// in the browser's order; each is also in Lines() and History().
	TArray<FTCAlertPush> Snapshot(const FTCMatchState& S, const FJsonObject* Raw, const FTCAlertContext& Ctx);
	// pings.js receive: a teammate's ping relay ({ t: 'ping', from, x, z }); false when malformed. Out: the "Ping from"
	// alert (also in Lines() and History()).
	bool ReceivePing(const FJsonObject& M, const FTCMatchState& S, double Now, FTCAlertPush& Out);
	// alerts.push for alerts raised elsewhere (alerts.js export push(kind, x, z, text)).
	FTCAlertPush Push(const FString& Kind, const FString& Text, bool bHasAt, FVector2D At, double Now, int32 N = 1);
	// alerts.js frame and pings.js frame: lines and rings past their life go.
	void Frame(double Now);
	// Round 15: an alert the game mode raised (ATCGameMode::PushAlert, which also plays its sound) joins the lines and
	// the history as alerts.js push would. The echo of this feed's own Push or ReceivePing (the same kind and text,
	// pushed this frame) is recognised and not added twice. Tick: the snapshot tick it came with.
	void Mirror(const FString& Kind, const FString& Text, FVector2D At, int32 Tick, double Now);

	// Round 15: the Alert history box (alerts.js toggleHistory, navigate: navigating opens it).
	bool bHistoryOpen = false;
	void ToggleHistory() { bHistoryOpen = !bHistoryOpen; }
	const FTCAlertHistoryEntry* Navigate(int32 Direction);
	// A click on a hit of Layout: 'alert:line:<serial>' (look there), 'alert:history', 'alert:nav:<1|-1>',
	// 'alert:entry:<id>'. True when the tag was this feed's; bOutJump with OutAt when the camera should go there.
	bool Click(const FString& Tag, FVector2D& OutAt, bool& bOutJump);
	// The feed as alerts.css draws it, in UI pixels; hits carry the tags Click takes.
	FTCPaint Layout(const FTCAlertViewInput& In, const FTCMeasure& Measure, const FTCMeasure& HeavyMeasure = FTCMeasure()) const;
	// drawPings: the minimap marks of the lines with a place (not pings), newest first, at Now.
	TArray<FTCAlertRing> MinimapRings(double Now) const;
	// alerts.js frame's opacity and the slide-in (alert-in or loss-in) at Now: opacity and the x offset in pixels.
	static float ShownAlpha(const FTCAlertLine& L, double Now, float* OutSlide = nullptr);

	const TArray<FTCAlertLine>& Lines() const { return LineList; }
	// The line's opacity now (the last FADE seconds fade out).
	static float Alpha(const FTCAlertLine& L, double Now);
	// alerts.js newest: a loss first, then the newest line with a place.
	bool Newest(FVector2D& Out) const;
	bool Pinging() const { return LineList.Num() > 0; }
	FTCAlertHistory& History() { return Hist; }
	const FTCAlertHistory& History() const { return Hist; }
	const FTCPings& Pings() const { return PingRings; }
	// Sounds alerts.js would play (audio.alert(kind) after the 1.5 s gap, 'ui:click' for a ping); no audio here.
	TArray<FString> TakeSounds() { TArray<FString> Out = MoveTemp(Sounds); Sounds.Reset(); return Out; }

	// main.js unitName: the faction's name for the type (client/main.js FACTIONS), else UNITS[type].name.
	static FString UnitName(const FTCMatchState& S, const FTCUnitTable* Table, const FString& Type, int32 Owner);
	// client/main.js FACTIONS[faction].names[type]; null when that faction has no own name for it.
	static const TCHAR* FactionUnitName(int32 Faction, const FString& Type);

private:
	struct FRow { FString Type; int32 Owner = -1; double X = 0, Z = 0; int32 Hp = 0, Flags = 0; double Built = 1; };
	struct FStrikeKey { FString Kind; double X = 0, Z = 0; };
	void Sound(const FString& Kind, double Now);
	void AddLine(const FTCAlertPush& A, double Now);
	void UnderAttack(FVector2D At, const FString& Text, const TCHAR* Kind, const FTCAlertContext& Ctx, TArray<FTCAlertPush>& Out);
	void ReadOrder(const FTCMatchState& S, const FJsonObject* Raw);
	double HistoryTime() const;

	FTCAlertHistory Hist;
	FTCPings PingRings;
	TArray<FTCAlertLine> LineList;
	int32 LineSerial = 0;
	TMap<FString, double> LastSound;
	TArray<FString> Sounds;
	struct FEcho { FString Kind, Text; double Now = 0.0; };
	TArray<FEcho> Echoes;          // this feed's own pushes, waiting for ATCGameMode::PushAlert's Mirror
	TCWorldRecoveryCore::FAttackQuiet OwnQuiet;
	double MatchTime = 0.0;        // alerts.js matchTime (the last accepted snapshot's tick / 20)
	int32 LastSnapTick = 0;        // main.js lastSnap.tick (hooks.matchTime), 0 without one
	// main.js lastSnap: the rows, points, strikes and queue heads of the last snapshot, in the browser's row order
	bool bHasPrev = false;
	TArray<int32> PrevOrder;
	TMap<int32, FRow> PrevRows;
	TArray<FTCPointRow> PrevPoints;
	TArray<FStrikeKey> PrevStrikes;
	TArray<TPair<int32, FString>> PrevQueueHeads;
	// client/world-state.js row order (a JavaScript Map's insertion order), kept from the raw messages
	TArray<int32> RowOrder;
	TSet<int32> RowSet;
	// the rows' x and z as delivered (doubles: FTCUnit keeps floats, and a group's middle must match the browser's)
	TMap<int32, FVector2D> RowXZ;
	TMap<int32, double> RowBuilt;
	FVector2D PosOf(const FTCUnit& U) const { const FVector2D* P = RowXZ.Find(U.Id); return P ? *P : FVector2D(U.X, U.Z); }
	double BuiltOf(const FTCUnit& U) const { const double* B = RowBuilt.Find(U.Id); return B ? *B : U.Built; }
	bool bHasHome = false;
	FVector2D Home = FVector2D::ZeroVector;
	int32 MatchSeen = INT32_MIN;
};
