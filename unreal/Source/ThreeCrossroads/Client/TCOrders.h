// What a right-click (or an armed attack-move click, or the Stop key) sends: a port of the reference's
// client/orders.js kind and dispatch for the order kinds this slice carries (attack, move, attack-move, stop), with the
// browser's default Formation menu (client/main.js formation). The server still validates and decides everything
// (shared/sim.js command); a refusal comes back as { t: 'deny', cmd, reason }, which DenySentence words as the browser
// does (client/availability.js DENY_SENTENCES). Interim until the Rust core supplies order building through its C ABI.
#pragma once

#include "CoreMinimal.h"

class FJsonObject;
class FTCMatchState;
class FTCUnitTable;

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
	// client/availability.js denySentence: the sentence the browser shows for a refusal.
	FString DenySentence(const FString& Reason, const FString& Cmd);
}
