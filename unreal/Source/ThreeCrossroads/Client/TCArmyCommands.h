// Retreat, rally and the three stances: a port of the reference's selection commands, for the facts this client holds.
// Ported from the pinned reference (e4e579a):
//   client/main.js       retreat, STANCE_BIT and toggleStance, startRally and rallyAt, the targeting clicks that end a
//                        rally aim (world and minimap mousedown);
//   client/orders.js     dispatch, the rally part (a right-click with Production Buildings selected);
//   client/keys.js       the R, Shift+F, Shift+G, Shift+X and Shift+H chords and match (modifiers must match exactly);
//   client/hud.js        drawOrders (when the Orders panel shows, the stance buttons' Off/On/n of N), drawRecruit (the
//                        Rally button on the recruit card), STANCE (names and tips);
//   client/availability.js availability({ t: 'stance' }).
// The server still validates and decides everything (shared/sim.js command 'retreat', 'stance' and 'rally', with
// shared/supply-convoys.js for trucks). Its refusal comes back as { t: 'deny', cmd, reason }, which
// TCOrders::DenySentence words. The Predicted* fields only say what this client expects from those rules, for notes
// and logs: they never stop a message the browser would send.
//
// Selection restrictions are kept as they are. Every plan reads the controller's Selected set and never adds to it,
// never selects anything and never names a unit outside it. The native selection holds only this seat's units
// (ATCPlayerController::PruneSelection), so the browser's Horde-ally buildings (productionAccess) cannot be in it and
// are never named here.
//
// Data. Whether a building makes units comes from FTCRecruitment's table (FTCRecruitDef::Makes), which is generated
// from the reference at FTCRecruitment::ReferencePin. Until FTCUnitTable::Reference names that pin, rally plans are
// refused with a native reason (bNative), as recruiting is. Interim until the Rust core supplies these through its C ABI.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCOrders.h"

class FJsonObject;
class FTCMatchState;
class FTCUnitTable;
class FTCRecruitment;
struct FTCUnit;
struct FTCUnitDef;
struct FKey;

// shared/sim.js STANCES, in order. Each is a per-unit switch the owner sees as a snapshot flag bit (stanceBits).
enum class ETCStance : uint8
{
	HoldFire,    // 'holdFire', bit 2048: shoot only on an attack order
	HoldPos,     // 'holdPos', bit 4096: never move unordered, not even to cover
	AutoRetreat, // 'autoRetreat', bit 8192: run for home below CFG.autoRetreat (35%) of full strength
};

namespace TCStance
{
	constexpr int32 Count = 3;
	constexpr ETCStance All[Count] = { ETCStance::HoldFire, ETCStance::HoldPos, ETCStance::AutoRetreat };
	// The wire key (shared/sim.js STANCES): 'holdFire', 'holdPos', 'autoRetreat'.
	const TCHAR* Key(ETCStance Stance);
	// client/main.js STANCE_BIT: the flag bit the owner's snapshot rows carry (shared/sim.js stanceBits).
	int32 Bit(ETCStance Stance);
	// client/hud.js STANCE: the button's name and tip, and client/keys.js label: its chord.
	const TCHAR* Name(ETCStance Stance);
	const TCHAR* Tip(ETCStance Stance);
	const TCHAR* Label(ETCStance Stance);
	bool FromKey(const FString& Key, ETCStance& Out);
}

// What a plan from this module is.
enum class ETCArmyCommand : uint8
{
	None,      // nothing goes out (Note says why when the browser would say anything)
	Retreat,   // { t: 'retreat', ids }
	Stance,    // { t: 'stance', ids, key, on }
	ArmRally,  // nothing is sent: arm the rally aim, the next left click on the ground (or minimap) sets the rally
	Rally,     // { t: 'rally', ids?, x, z }
};
const TCHAR* LexToString(ETCArmyCommand Kind);

// One key chord's army command (client/keys.js bindings and match). The chord must match exactly: R with Shift, or
// Shift+F with Ctrl, is no army command.
enum class ETCArmyKey : uint8
{
	None,
	Retreat,     // R
	HoldFire,    // Shift+F
	HoldPos,     // Shift+G
	AutoRetreat, // Shift+X
	Rally,       // Shift+H
};
const TCHAR* LexToString(ETCArmyKey Key);

struct FTCArmyPlan
{
	ETCArmyCommand Kind = ETCArmyCommand::None;
	TSharedPtr<FJsonObject> Message;  // the one message to send; null when nothing goes out
	int32 Units = 0;                  // ids named in the message
	// Of those, how many shared/sim.js command's mine() takes as this seat sees them: own, not a structure, train,
	// rider or operative's squad. For a stance a convoy truck under forced logistics is also refused by the server, a
	// fact the snapshot does not carry, so Eligible can be one high per such truck.
	int32 Eligible = 0;
	bool bStanceOn = false;           // Stance: the value sent (on unless every named unit already has it)
	ETCStance Stance = ETCStance::HoldFire;
	int32 RallyBuildings = 0;         // Rally: Production Buildings named (0: the army's rally point)
	FVector2D At = FVector2D(-1, -1); // where the browser puts its marker (rally only)
	FColor Color = FColor(0x9d, 0xd0, 0xff); // client/main.js rallyAt marker, 0x9dd0ff
	// The aim after this plan, for the controller's targeting state (client/main.js setAim and cancelAim).
	bool bArmAim = false;             // ArmRally: start aiming a rally
	bool bCancelAim = false;          // a rally click that went out ends the aim
	bool bKeepAim = false;            // a rally click with no ground under it keeps the aim (the browser stays armed)
	// What the browser shows (its hint or availability sentence), or for this client, why nothing was sent. Empty
	// when the browser stays silent and the plan sends.
	FString Note;
	bool bNative = false;             // Note is this client's own reason, not a sentence the browser shows
	// The deny reason the server is expected to answer ('needs', 'unseen', 'blocked'), empty when acceptance is
	// expected or the client cannot tell. Only a prediction: the plan is sent regardless, as the browser sends it.
	FString PredictedDeny;

	// The same message as an FTCOrderPlan, for ATCPlayerController::SendPlan (which reads only Message, Units, At,
	// Color and whether Kind is Attack). Kind is None: ETCOrderKind has no entries for these commands.
	FTCOrderPlan ToOrderPlan() const;
};

// A stance button as client/hud.js drawOrders shows it, with its availability.
struct FTCStanceButton
{
	ETCStance Stance = ETCStance::HoldFire;
	int32 On = 0;          // selected units that have it (the bit counts over every held selected unit)
	int32 Of = 0;          // held selected units
	FString Text;          // 'Off', 'On' or 'n/N'
	bool bLit = false;     // the 'on' look: at least one has it
	bool bOk = false;      // availability({ t: 'stance' })
	FString Reason;        // why not, in the browser's words, or this client's when bNative
	bool bNative = false;
};

namespace TCArmyCommands
{
	// The browser's own chords. Each label is client/keys.js label for its binding.
	constexpr const TCHAR* RetreatLabel = TEXT("R");
	constexpr const TCHAR* RallyLabel = TEXT("Shift+H");
	// client/main.js setAim('rally') hint, with the suffix every aim gets.
	constexpr const TCHAR* RallyAimHint = TEXT("Click where recruits should gather. Right-click cancels");

	// client/keys.js match over the bindings above. bCtrl: Ctrl or Command, as the browser's ctrlKey || metaKey.
	ETCArmyKey KeyAction(const FKey& Key, bool bShift, bool bCtrl, bool bAlt);

	// client/main.js retreat: { t: 'retreat', ids: [...selected] } for any non-empty selection. The whole selection is
	// named, as the browser names it; the server keeps its own movable units and answers 'needs' when there are none.
	FTCArmyPlan PlanRetreat(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected);

	// client/main.js toggleStance: the selected own non-structure units get the switch on, unless every one of them
	// already has it (then off). Nothing goes out when there are none (the browser stays silent; Note carries the
	// availability sentence for a HUD button press).
	FTCArmyPlan PlanStance(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, ETCStance Stance);
	FTCArmyPlan PlanStance(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, ETCArmyKey Key);

	// client/main.js startRally (the Shift+H key and the recruit card's Rally button): arm the rally aim, or say why
	// not. Needs a snapshot this match; in Classic and World (classicMode: the lobby's mode, or a World map) a selected
	// own Production Building. A spectator or unconfirmed seat cannot aim (setAim returns while observing).
	FTCArmyPlan PlanArmRally(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected);

	// client/main.js rallyAt: the left click (world or minimap) that ends a rally aim. Selected own Production
	// Buildings get their rally (ids); with none, the message sets the army's rally point. bHasGround false: the click
	// found no ground, the browser shows DenySentence('blocked') and stays armed (bKeepAim).
	FTCArmyPlan PlanRallyAt(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, bool bHasGround, FVector2D Ground);

	// client/orders.js dispatch, its rally part: with ground under the right-click, the selected own Production
	// Buildings (a building that makes units) get { t: 'rally', ids, x, z }. Null when there are none or no ground,
	// or the recruit table's pin differs. OutBuildings: how many were named. The browser sends it alone (with
	// queue) when no troops are selected, else as one part of { t: 'orders', queue, commands } after the troops'
	// command; the server's dispatchOrderGroups accepts 'rally' as a part.
	TSharedPtr<FJsonObject> RightClickRallyCommand(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, int32* OutBuildings = nullptr);
	// The no-troops case of the above as a whole plan: the single command with its queue flag (Shift), the marker at
	// the ground in 0x9dd0ff. Kind None when RightClickRallyCommand is null.
	FTCArmyPlan PlanRightClickRally(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bQueue);

	// client/hud.js drawOrders: the Orders panel (Retreat, Attack-move, Stop, the three stances, ...) shows for a
	// non-empty selection that is neither all buildings nor all trucks (all trucks get Stop and Resume deliveries).
	bool ShowsOrdersPanel(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected);
	// client/hud.js drawRecruit: the recruit card carries a Rally button. The card is drawn outside Classic and World
	// (the snapshot's mode, FTCRecruitment::Classic) once a snapshot has come, unless the selection is all trucks.
	bool ShowsRallyButton(const FTCMatchState& State, const FTCRecruitment& Recruit, const TSet<int32>& Selected);
	// A stance button's state and availability.
	FTCStanceButton StanceButton(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, ETCStance Stance);

	// The army's rally point from a snapshot (shared/sim.js snapshotFor rally: [x, z] rounded to 0.1, or null). The
	// server sends the field whole in every seat's snapshot (it is not a trimmed list) and leaves it out for a
	// spectator, so false (no rally) is the answer for a missing or null field. Building rallies come with the
	// snapshot's queues (FTCRecruitment::QueueOf(Id)->Rally).
	bool ReadArmyRally(const FJsonObject& Snapshot, FVector2D& Out);

	// client/main.js classicMode: the lobby's mode is Classic or World, or the start's map is a World map.
	bool ClassicLobby(const FTCMatchState& State);
	// shared/sim.js command's mine() as this seat can see it (see FTCArmyPlan::Eligible).
	bool TakesSelectionOrder(const FTCMatchState& State, const FTCUnit& U, const FTCUnitDef* Def);
	// A held unit's type makes units (FTCRecruitDef::Makes is not empty). False for an unknown type.
	bool MakesUnits(const FString& Type);
}
