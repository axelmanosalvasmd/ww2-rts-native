// Control groups 1 to 9: a port of the reference's client/selection.js group, prune and reset, with the chords of
// client/keys.js (Digit1..Digit9: recall; Ctrl: set; Shift or Ctrl+Shift: append; Alt: transfer) and client/main.js
// transferGroup. Pure client state over FTCMatchState and the selection set the controller owns: nothing is sent to
// the server and nothing here decides what a unit may do. Interim until the Rust core supplies this through its C ABI.
//
// Reference rules kept here:
//   - A group never holds a supply truck (set, append, transfer and prune all drop them): trucks stay on their runs.
//   - Set and append take the selection's own living units (buildings and trains included, as the browser's rows);
//     transfer takes only the army (client/selection.js selectable: no structure, train, truck, parked plane or rider)
//     and removes those units from every other group.
//   - Members are pruned before every operation: a unit that is dead, no longer ours or a truck leaves its groups.
//   - Recall replaces the selection with the members that can be selected now (client/selection.js selectableOwn):
//     mobile units not parked at their airfield or riding in a carrier, and own buildings. Riders and parked planes
//     stay in the group and come back with it later. Recalling an empty group clears the selection, as the browser.
//   - The same group recalled again within 300 ms (client/selection.js group) asks for the camera to centre on it.
//   - Groups belong to one match and seat: a new match (or a different seat) empties them, a reconnect to the same
//     match keeps them (client/main.js receiveStart resume).
#pragma once

#include "CoreMinimal.h"

class FTCMatchState;
class FTCUnitTable;
struct FTCUnit;
struct FTCUnitDef;
struct FKey;

enum class ETCGroupOp : uint8
{
	None,      // the chord is not a group chord (Alt+Ctrl, Alt+Shift, ...: client/keys.js binds none of them)
	Recall,    // Digit N
	Set,       // Ctrl+N: the group becomes the eligible selection (an empty selection empties it)
	Append,    // Shift+N or Ctrl+Shift+N: the eligible selection joins the group
	Transfer,  // Alt+N: the selection's army joins the group and leaves every other group
};
const TCHAR* LexToString(ETCGroupOp Op);

// What one operation did, for the HUD note and the camera.
struct FTCGroupResult
{
	ETCGroupOp Op = ETCGroupOp::None;
	int32 Group = 0;
	// False when nothing was touched: not a group number 1..9, no operation, or no confirmed seat (a spectator, or a
	// fresh client whose lobby has not arrived yet). The selection and the groups are then exactly as they were.
	bool bApplied = false;
	// Set, Append, Transfer: the selected units written into the group this time. Recall: the units now selected.
	int32 Taken = 0;
	// The group's size after the operation (Recall: including members that cannot be selected right now).
	int32 Members = 0;
	// Recall only: a second recall of the same group within DoubleTapSeconds; Centre is the mean of the recalled
	// units' snapshot positions (reference metres, x and z). The browser centres on its drawn (eased) positions.
	bool bCentre = false;
	FVector2D Centre = FVector2D::ZeroVector;
	FString Note;
};

class FTCControlGroups
{
public:
	static constexpr int32 GroupCount = 9;
	static constexpr double DoubleTapSeconds = 0.3; // client/selection.js: now - lastRecall <= 300 (ms)

	// client/keys.js match: the operation for a digit pressed with these modifiers (bCtrl: Ctrl or Command).
	static ETCGroupOp OpForChord(bool bCtrl, bool bShift, bool bAlt);
	// The group a key names: 1..9 for the top-row digits (client/keys.js binds Digit1..Digit9 only), else 0.
	static int32 DigitOf(const FKey& Key);

	// Reference eligibility tests, exposed so the HUD or controller can agree with them.
	// client/selection.js own (minus horde allies' buildings, which this client does not model): ours and alive.
	static bool IsOwnAlive(const FTCMatchState& State, const FTCUnit& U);
	// May stay in (or enter, by set or append) a group: own, alive, not a supply truck.
	static bool CanBeMember(const FTCMatchState& State, const FTCUnit& U);
	// client/selection.js selectableOwn: what a recall selects.
	static bool IsRecallable(const FTCMatchState& State, const FTCUnit& U, const FTCUnitDef* Def);
	// client/selection.js selectable: what a transfer moves.
	static bool IsArmy(const FTCMatchState& State, const FTCUnit& U, const FTCUnitDef* Def);

	// Follow the match: a new match id or seat, or no match at all, empties every group. Every operation calls it;
	// calling it each frame as well is cheap and keeps Members() right for a HUD.
	void Sync(const FTCMatchState& State);
	// client/selection.js prune: duplicates, units no longer ours or alive, and trucks leave their groups. A member
	// missing from the held units is dropped only once a snapshot of this start has arrived (right after a reconnect's
	// start nothing is held yet) and when it is not a held row that could not be read. Does nothing without a
	// confirmed seat, as then no unit counts as ours.
	void Prune(const FTCMatchState& State);

	// The one entry point: Group 1..9, Op from OpForChord. Selected is the controller's selection set: Recall replaces
	// it, the other operations only read it. NowSeconds: a monotonic clock in seconds (FPlatformTime::Seconds), for
	// the double tap.
	FTCGroupResult Apply(int32 Group, ETCGroupOp Op, const FTCMatchState& State, const FTCUnitTable& Table, TSet<int32>& Selected, double NowSeconds);
	FTCGroupResult Recall(int32 Group, const FTCMatchState& State, const FTCUnitTable& Table, TSet<int32>& Selected, double NowSeconds) { return Apply(Group, ETCGroupOp::Recall, State, Table, Selected, NowSeconds); }
	FTCGroupResult Assign(int32 Group, const FTCMatchState& State, const FTCUnitTable& Table, TSet<int32>& Selected) { return Apply(Group, ETCGroupOp::Set, State, Table, Selected, 0.0); }
	FTCGroupResult Add(int32 Group, const FTCMatchState& State, const FTCUnitTable& Table, TSet<int32>& Selected) { return Apply(Group, ETCGroupOp::Append, State, Table, Selected, 0.0); }
	FTCGroupResult Transfer(int32 Group, const FTCMatchState& State, const FTCUnitTable& Table, TSet<int32>& Selected) { return Apply(Group, ETCGroupOp::Transfer, State, Table, Selected, 0.0); }

	// The ids stored for a group, in the order they joined (empty for a number outside 1..9). Not pruned: call Prune
	// (or any operation) first for a current count.
	const TArray<int32>& Members(int32 Group) const;
	// client/selection.js reset: every group empty, the double tap forgotten.
	void Reset();

private:
	TArray<int32> Groups[GroupCount];
	int32 LastGroup = 0;
	double LastRecall = TNumericLimits<double>::Lowest();
	bool bHeld = false;
	int32 HeldMatch = -1, HeldSeat = -1;
};
