// What a right-click (or an armed attack-move click, or the Stop key) sends: a port of the reference's
// client/orders.js kind and dispatch for the order kinds this slice carries (attack, move, attack-move, stop), with the
// browser's default Formation menu (client/main.js formation). The server still validates and decides everything
// (shared/sim.js command); a refusal comes back as { t: 'deny', cmd, reason }, which DenySentence words as the browser
// does (client/availability.js DENY_SENTENCES). Interim until the Rust core supplies order building through its C ABI.
//
// Round 16 (orders scope) adds, from the same pin:
//   - TCOrders::Wrap, EscortCommand, RailCommand: the exact messages client/orders.js dispatch and client/main.js
//     railTo send (one command with its queue flag, or { t: 'orders', queue, commands });
//   - TCOrders::Markers: client/markers.js planLayer draw (the selected units' routes, queued orders, targets and rally
//     flags) as a list of ground primitives with the reference's trims, arrowheads, widths, colours and alphas, checked
//     against the pinned markers.js itself (test ThreeCrossroads.Orders.QueueMarkers);
//   - TCOrders::RoofBar*: client/main.js and client/selection-view.js for a garrisoned squad: its bar drawn 7.5 m over
//     the ground at the house and always shown, and the bar's meshes as the squad's only selection points;
//   - TCOrders::GarrisonTag: client/hud.js 'Garrisoned: <house>' in the language in use;
//   - FTCOrdersCheck: the -TCOrdersCheck= live script (escort, garrison, roof-bar pick, ungarrison, Shift queues, rail,
//     recover), which presses the same controller paths the mouse and keys use and counts adopted and refused commands.
//     Actions: select, selectone, remember, anchor, focus (sel, house, train, x,z or +dx,dz from the anchor), zoom,
//     garrison, move, escort, pickroof, boxroof, gostation, rail, key, press, shot, quit, and for a World sandbox match
//     spawnset, neighbours, attackbase and capture (the sandbox bar's own controls, then map clicks), so a second own
//     station can be won before a rail move. Conditions: play, garrisoned, out, queued, planes, air, atstation,
//     recovery, captured, nobase, train.
#pragma once

#include "CoreMinimal.h"
#include "Net/TCMatchState.h"

class FJsonObject;
class FJsonValue;
class FTCMatchState;
class FTCUnitTable;
class ATCGameMode;
class ATCPlayerController;

// What lies under the pointer (client/main.js mousedown cursor): the ground point, and the nearest picked unit of each
// kind within its pick radius (0: none). Picking itself is the controller's (it needs the screen).
struct FTCCursor
{
	bool bHasGround = false;
	FVector2D Ground = FVector2D::ZeroVector;
	int32 Enemy = 0;     // a foe's unit
	int32 Friend = 0;    // a non-foe unit that is not a structure or an aircraft
	int32 Building = 0;  // a non-foe building (radius 60 px)
	// client/main.js houseAt: a house cell under the pointer (FTCWorldObjects::PickHouse), its centre in metres
	bool bHouse = false;
	int32 HouseCell = -1;
	FVector2D House = FVector2D::ZeroVector;
};

enum class ETCOrderKind : uint8
{
	None,        // nothing to order (no own troops selected, or no ground under a move)
	Move,
	AttackMove,
	Attack,
	Stop,
	Unsupported, // the browser would send an order this slice does not build yet (supply, escort, board, rally)
};
const TCHAR* LexToString(ETCOrderKind Kind);

struct FTCOrderPlan
{
	ETCOrderKind Kind = ETCOrderKind::None;
	// The one message to send: a single command with its queue flag, or { t: 'orders', queue, commands } when the
	// click makes more than one (an attack with trucks in the selection), exactly as client/orders.js dispatch.
	TSharedPtr<FJsonObject> Message;
	int32 Units = 0;            // units named in the message
	int32 TargetId = 0;         // Attack: the enemy unit
	FVector2D At = FVector2D(-1, -1); // where the marker goes (the ground, or the enemy's position)
	FColor Color = FColor(0x5f, 0xa4, 0xf2); // client/main.js MOVE_COLOR; attack 0xff4030, attack-move 0xff9a40
	FString Why;                // Unsupported or None: what the browser would have done, or why nothing goes out
};

namespace TCOrders
{
	// client/orders.js dispatch. bAttack: attack-move instead of move (the armed G order, or Ctrl held). bQueue: Shift,
	// appended to the units' order queues. Selected may hold anything; only own non-structure units are troops.
	FTCOrderPlan Plan(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bAttack, bool bQueue);
	// client/main.js stop: { t: 'stop', ids: [...selected] } (the server keeps only its own movable units).
	FTCOrderPlan PlanStop(const TSet<int32>& Selected);
	FTCOrderPlan PlanDestroy(const TSet<int32>& Selected);
	TArray<int32> RailUnits(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected);
	// client/availability.js denySentence: the sentence the browser shows for a refusal.
	FString DenySentence(const FString& Reason, const FString& Cmd);

	// client/orders.js dispatch's send: one command gets the queue flag itself, several go as
	// { t: 'orders', queue, commands } (the parts keep no queue of their own). Null for no command.
	TSharedPtr<FJsonObject> Wrap(const TArray<TSharedPtr<FJsonObject>>& Commands, bool bQueue);
	// client/orders.js order 'escort': { t: 'escort', ids: planes, target } (the queue flag is Wrap's).
	TSharedPtr<FJsonObject> EscortCommand(const TArray<int32>& Planes, int32 Target);
	// client/main.js railTo: { t: 'rail', ids, x, z }, never queued (the browser sends no queue field).
	TSharedPtr<FJsonObject> RailCommand(const TArray<int32>& Ids, double X, double Z);
	// The JSON text a message goes out as (condensed, field order as built), for the tests and the logs.
	FString ToJson(const TSharedPtr<FJsonObject>& Message);
}

// ---------------------------------------------------------------------------------------------------------------------
// Queue and plan markers: client/markers.js planLayer().draw(selected, units, rally).
// One primitive per call the reference makes into its two batches: lines (solid or the dashed batch, ORDER_DASH 2.2 m
// dashes with 1.3 m gaps counted from the far end), filled arrowheads and rings, each with the hex colour and alpha set
// before it. Positions are reference metres on the ground (x, z).
struct FTCMarkPrim
{
	enum class EOp : uint8 { Line, Arrow, Ring };
	EOp Op = EOp::Line;
	bool bDashed = false;
	uint32 Hex = 0x5fa4f2;
	float Alpha = 1.f;
	double Width = 0.0;          // Line, Ring: the ribbon width in metres (markers.js o.w)
	TArray<FVector2D> Points;    // Line: the polyline
	FVector2D At = FVector2D::ZeroVector;  // Arrow: the tip; Ring: the centre
	FVector2D Dir = FVector2D::ZeroVector; // Arrow: the direction it points (not normalised, as passed)
	double Len = 0.0, Half = 0.0;          // Arrow: length and half width
	double Radius = 0.0;                   // Ring
};

// What planLayer reads of a unit (client/main.js v): its drawn position, UNITS radius (1.5 when the type is unknown),
// the target it shoots at (v.tgt), its plan, its waiting orders and its production rally.
struct FTCMarkUnit
{
	int32 Id = 0;
	FVector2D At = FVector2D::ZeroVector;
	double Radius = 1.5;
	int32 Target = 0;
	int32 PlanKind = 0;          // 0: no plan (v.plan null or kind 0)
	FVector2D PlanTo = FVector2D::ZeroVector;
	TArray<FVector2D> Path;
	TArray<FTCQueuedOrder> Orders;
	bool bRally = false;
	FVector2D Rally = FVector2D::ZeroVector;
};

namespace TCOrders
{
	// client/markers.js PLAN_COLORS (MOVE_COLOR for a kind it does not list)
	uint32 PlanHex(int32 Kind);
	// planLayer draw: Selected in the selection's order; Units holds every selected unit and every unit one of them
	// targets (a missing id draws nothing, as units.get does); ArmyRally: the snapshot's army rally (null: none).
	TArray<FTCMarkPrim> Markers(const TArray<int32>& Selected, const TMap<int32, FTCMarkUnit>& Units, const FVector2D* ArmyRally);
	// The client's state as planLayer reads it: the drawn (eased) positions from DrawnAt (else the snapshot's), the
	// snapshot's plans and waiting orders, and RallyOf for a production building's rally (false: none). Garrisoned
	// units are included (the browser draws their plans from the house).
	TMap<int32, FTCMarkUnit> MarkUnits(const FTCMatchState& State, const FTCUnitTable& Table, const TArray<int32>& Selected,
		TFunctionRef<bool(int32, FVector2D&)> DrawnAt, TFunctionRef<bool(int32, FVector2D&)> RallyOf);

	// ----- a garrisoned squad's roof bar (client/main.js makeUnit bars, frame; client/selection-view.js) -----
	// The bar group sits RoofBarHeight over the ground at the unit, faces the camera, and is always shown while the
	// squad is inside; it is the squad's only selection point (no model point, no position point).
	inline constexpr double RoofBarHeight = 7.5;
	// One visible mesh of the bar group, in the group's billboard plane (metres; x right, y up, centre and size).
	struct FBarRect { double X = 0, Y = 0, W = 0, H = 0; };
	// The meshes visible for a garrisoned unit (main.js: the background 2.4 x 0.42, the health bar 2.3 * hp fraction x
	// 0.2 at y 0.07, the suppression bar while suppressed, the veterancy stars, the unit badge 1.45 x 1.45 at
	// (-1.98, 0.08)); HpFrac in [0, 1], Supp 0 to 100, Stars 0 to 3.
	TArray<FBarRect> RoofBarRects(double HpFrac, int32 Supp, int32 Stars);
	// client/selection-view.js: a mesh point picks within 6 px of its screen bounds (radius 6)
	inline constexpr double RoofPickRadius = 6.0;
	// client/selection.js pick and box for one unit's screen points (bounds x0, y0, x1, y1 in pixels): the distance from
	// the pixel to the nearest bounds (0 inside), and whether a box overlaps any of them.
	double RoofPickDistance(const TArray<FBox2D>& Bounds, FVector2D Pixel);
	bool RoofBoxHits(const TArray<FBox2D>& Bounds, const FBox2D& Box);
	// selection-view.js selectionPoints for a garrisoned unit: each rect's four corners around Anchor (Unreal cm, the bar
	// group's centre) along the camera's Right and Up (unit vectors), projected by Project (false: behind the camera,
	// that rect is clipped away), as screen bounds. Empty when nothing projects.
	TArray<FBox2D> RoofBarScreenBounds(const FVector& Anchor, const FVector& Right, const FVector& Up, const TArray<FBarRect>& Rects,
		TFunctionRef<bool(const FVector&, FVector2D&)> Project);
	// client/hud.js tags: 'Garrisoned: <CFG.houses[flags >> 20 & 7].name>' through tr (empty when not garrisoned).
	FString GarrisonTag(int32 Flags);
}

// ---------------------------------------------------------------------------------------------------------------------
// -TCOrdersCheck=<steps>: a live verification script for this scope, run by ATCGameMode every frame (OrdersCheckFrame).
// Steps are separated by ';', each "[delay][@cond]:action[:arg]". It only presses what the player could: the
// controller's selection, right press and release, key chords and aim clicks; the server decides, and every deny that
// comes back is counted with its command and reason. The log lines start with "orders check".
class FTCOrdersCheck
{
public:
	bool Parse(const FString& Script, const FString& ShotDir);
	bool Active() const { return Steps.Num() > 0 && Next < Steps.Num(); }
	void Frame(ATCGameMode& M, ATCPlayerController& PC);
	// Called for every message the game mode hands to the socket (counts what this client sent, by t).
	void NoteSent(const FString& T) { if (Steps.Num()) Sent.FindOrAdd(T)++; }
private:
	struct FStep { double Delay = 0; FString Cond, Action, Arg; };
	TArray<FStep> Steps;
	int32 Next = 0;
	double StepAt = -1;
	FString ShotDir;
	TMap<FString, int32> Sent;
	int32 DeniesAtStart = -1;
	TMap<FString, int32> Refused; // "cmd/reason"
	int32 LastDenies = 0;
	TArray<int32> Remember;      // ids a step picked (the garrison, the escorted unit)
	FVector2D House = FVector2D(-1, -1);
	TArray<double> CaptureIds;   // World regions a 'capture' step spawned infantry on (sandbox)
	double CaptureAt = -1;
	TArray<FVector2D> Guesses;   // the World regions next to home a 'neighbours' step sent units to (grid guesses)
	double AssaultAt = -1;
	bool Ready(const ATCGameMode& M, const ATCPlayerController& PC, const FStep& S) const;
	FString Run(ATCGameMode& M, ATCPlayerController& PC, const FStep& S);
	FString Summary(const ATCGameMode& M, const ATCPlayerController& PC) const;
};
