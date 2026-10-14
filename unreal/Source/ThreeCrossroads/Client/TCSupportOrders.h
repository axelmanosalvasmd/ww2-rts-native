// Support orders: builders assisting a site or repairing a building, cancelling an own site, and carriers (boarding by
// right-click, Mount up, Unload). A port of the pinned reference (e4e579a), for the facts this client receives:
//   client/orders.js   kind and dispatch, the 'board' and 'assist' branches (and their place in kind's order);
//   client/main.js     unload, mountUp, the 'unload' and 'mountUp' actions, builders (classicMode for builderTypes);
//   client/keys.js     'unload' (Shift+E) and 'mountUp' (Shift+Q), global context, exact modifiers (match);
//   client/hud.js      drawOrders (the Mount up and Unload buttons, their text and availability), drawClassicCard (a
//                      site's Cancel, "75% back", which clears the selection), drawRecruit (the facility line and its
//                      "Cancel (75% back)" button outside Classic and World);
//   shared/sim.js      command 'assist', 'board', 'unload' and 'cancel' (only for PredictedDeny), CARGO_FLAG and
//                      CARGO_SHIFT, unitRow's built (the raw double, not rounded).
// The server validates and decides everything; a refusal comes back as { t: 'deny', cmd, reason }, which
// TCOrders::DenySentence words. The PredictedDeny fields only say what this client expects from those rules, for notes
// and logs: they never stop a message the browser would send. The browser has no availability check for these four
// commands (client/availability.js), so neither does this module.
//
// Data. Builder types come from FTCConstruction::BuilderTypes (generated at FTCConstruction::ReferencePin) and the naval
// flag from FTCRecruitment::Find (generated at FTCRecruitment::ReferencePin). Carries and hpPer come from FTCUnitTable
// (Content/Data/units.json). When the unit table names a different pin than a table a plan needs, the plan is refused
// with a native reason (bNative) instead of guessing.
//
// Precision. FTCUnit::Built is a float, but the server sends the raw double u.built: with one Engineer the last tick of
// a site is for example 0.9999999999999842 (hq) or 0.9999999999999897 (depot), which a float reads as 1. The browser
// still sees a site there (assist, Cancel). FTCSupportOrders keeps each held row's built as the double it came in as
// (ApplySnapshot), and every check here reads BuiltOf, never FTCUnit::Built directly.
//
// Selection restrictions are kept: every plan reads the caller's Selected set, never adds to it, and names no unit
// outside it. Interim until the Rust core supplies these through its C ABI.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCOrders.h"

class FJsonObject;
class FTCMatchState;
class FTCUnitTable;
struct FTCUnit;
struct FKey;

// shared/sim.js CARGO_FLAG and CARGO_SHIFT: a carrier with squads in it, and how many (flags >> 23 & 3).
namespace TCCargo
{
	constexpr int32 Flag = 524288;
	constexpr int32 Shift = 23;
	// client/hud.js: v.flags & CARGO_FLAG ? v.flags >> CARGO_SHIFT & 3 : 0
	inline int32 Count(int32 Flags) { return (Flags & Flag) ? (Flags >> Shift) & 3 : 0; }
}

enum class ETCSupportOrder : uint8
{
	None,        // not this module's order: the caller goes on to its other orders (attack, move, ...)
	Blocked,     // this module's order, but nothing goes out (Note says why; bNative when it is this client's reason)
	Assist,      // { t: 'assist', ids, id, queue }: builders join an unfinished site
	Repair,      // the same message for a finished building below its hpPer
	Board,       // { t: 'board', ids, target, queue }: a right-click on an own carrier with infantry selected
	MountUp,     // { t: 'board', ids }: Shift+Q or the Mount up button, no target, no queue
	Unload,      // { t: 'unload', ids }: Shift+E or the Unload button
	CancelSite,  // { t: 'cancel', id }: the site card's Cancel button
};
const TCHAR* LexToString(ETCSupportOrder Kind);

// One chord of this module (client/keys.js, 'global' context, modifiers must match exactly).
enum class ETCSupportKey : uint8
{
	None,
	Unload,   // Shift+E
	MountUp,  // Shift+Q
};
const TCHAR* LexToString(ETCSupportKey Key);

struct FTCSupportPlan
{
	ETCSupportOrder Kind = ETCSupportOrder::None;
	TSharedPtr<FJsonObject> Message;  // the one message to send; null when nothing goes out
	int32 Units = 0;                  // ids named in the message (cancel: 1, the site)
	int32 TargetId = 0;               // Assist and Repair: the building; Board: the carrier; CancelSite: the site
	FVector2D At = FVector2D(-1, -1); // where the browser puts its marker (right-click orders only)
	// client/orders.js dispatch: assist 0xe8c860, board 0x9dd0ff. Button and key orders have no marker.
	FColor Color = FColor(0x9d, 0xd0, 0xff);
	// client/hud.js drawClassicCard: the site card's Cancel also clears the selection (ctx.selected.clear()). The
	// facility line's Cancel outside Classic and World does not.
	bool bClearSelection = false;
	// Blocked: why nothing goes out. Otherwise empty, or this client's own remark (bNative) the browser would not show.
	FString Note;
	bool bNative = false;
	// The deny reason this client expects from shared/sim.js command ('unseen', 'needs', 'noBuilders', 'retreating',
	// 'max'), empty when acceptance is expected or the client cannot tell. Only a prediction: the plan is sent
	// regardless, as the browser sends it.
	FString PredictedDeny;

	// The same message as an FTCOrderPlan for ATCPlayerController::SendPlan (Message, Units, At, Color; Kind is None,
	// ETCOrderKind has no entries for these commands). Why carries Note.
	FTCOrderPlan ToOrderPlan() const;
};

// A carrier button of the Orders panel (client/hud.js drawOrders) with its availability. The browser never disables
// these (feedback.js setAvailability only sets aria-disabled): a click sends even when bOk is false.
struct FTCCarrierButton
{
	bool bShown = false;   // the Orders panel shows (TCArmyCommands::ShowsOrdersPanel) and the button's own condition
	bool bOk = false;
	FString Text;          // Mount up: "N free"; Unload: "n/of"
	FString Reason;        // 'No free seats' or 'No squad on board' when not bOk; this client's reason when bNative
	bool bNative = false;
	FString Label;         // the key badge: "Shift+Q" or "Shift+E"
	FString Tip;           // the button's title when bOk
};

// A site's Cancel button: drawClassicCard's (Classic and World) or drawRecruit's facility line (other modes).
struct FTCSiteCard
{
	bool bShown = false;        // exactly one selected held building (drawRecruit) or own building (drawClassicCard)
	int32 Building = 0;
	double Built = 1;           // the exact double (FTCSupportOrders::BuiltOf)
	bool bClassicCard = false;  // drawn by drawClassicCard (the snapshot's mode is Classic or World)
	bool bCancel = false;       // the Cancel button is there: own, built < 1
	FString CancelText;         // "Cancel" (its small line: CancelSub) or "Cancel (75% back)"
	FString CancelSub;          // "75% back" on the Classic card, empty otherwise
	FString CancelTip;          // Classic card: the button's title; empty for the facility line (it has none)
	// The words around it. Classic card: Status "Under construction 45%", Hint "Right-click it with Engineers to help"
	// (only for a site). Facility line: Status "Barracks: 45% built . 750 HP. Right-click with builders to assist".
	FString Status, Hint;
};

class FTCSupportOrders
{
public:
	// Message hooks, called after FTCMatchState's own: a start forgets every row; a snapshot keeps each unit row's built
	// as sent, and drops rows the way client/world-state.js does (all, or no held, replaces; gone removes). Clear
	// when the match ends.
	void ApplyStart(const FJsonObject& M);
	void ApplySnapshot(const FJsonObject& M);
	void Clear();

	// The held unit's built exactly as the server sent it (unitRow: u.built ?? 1), else FTCUnit::Built.
	double BuiltOf(const FTCUnit& U) const;

	// client/orders.js dispatch for a right-click (world or minimap): Board or Assist/Repair when kind() picks them, in
	// its order (supply, escort, board, assist), else None (the caller carries on: supply, escort, entrench, attack,
	// house, move). Selected may hold anything; only own non-structure units are troops. bQueue: Shift. The message is
	// the single command with its queue flag, as the browser sends it when nothing else is added; the caller may fold
	// it with a rally into { t: 'orders', queue, commands } (the server's dispatchOrderGroups accepts both parts).
	FTCSupportPlan PlanRightClick(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bQueue) const;

	// client/main.js mountUp: { t: 'board', ids: [...selected] } for any non-empty selection, whole and unfiltered.
	FTCSupportPlan PlanMountUp(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const;
	// client/main.js unload: { t: 'unload', ids } with the selected held units whose type carries; nothing when none.
	FTCSupportPlan PlanUnload(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const;
	// The site card's Cancel: { t: 'cancel', id } for the card's building (SiteCard with bCancel), else Blocked.
	// bSnapshotClassic: FTCRecruitment::Classic() (which card the browser draws).
	FTCSupportPlan PlanCancelSite(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, bool bSnapshotClassic) const;

	// client/hud.js drawOrders's Mount up and Unload buttons.
	FTCCarrierButton MountUpButton(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const;
	FTCCarrierButton UnloadButton(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const;
	// The selected site's card and its Cancel (see FTCSiteCard). Shown only once a snapshot came this match.
	FTCSiteCard SiteCard(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, bool bSnapshotClassic) const;

	// client/keys.js match for this module's two chords. bCtrl: Ctrl or Command. The browser's overlay contexts win
	// first: in recruit mode or with a lettered building card, Shift+Q and Shift+E buy five from cards 1 and 3
	// (cardMany), and the caller must ask those before this.
	static ETCSupportKey KeyAction(const FKey& Key, bool bShift, bool bCtrl, bool bAlt);
	static constexpr const TCHAR* UnloadLabel = TEXT("Shift+E");
	static constexpr const TCHAR* MountUpLabel = TEXT("Shift+Q");

private:
	struct FRowBuilt { FString Type; double Built = 1; };
	TMap<int32, FRowBuilt> Rows;
	bool bStarted = false;
};
