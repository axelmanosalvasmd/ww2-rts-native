// Recruiting: a port of the reference's Command Card purchase path, for the facts this client actually receives.
// Ported from the pinned reference (FTCRecruitment::ReferencePin):
//   client/hud.js         buildCard and drawRecruit (the recruit card outside Classic), drawClassicCard and drawQueue
//                         (a selected Production Building's card in Classic and World), buy (Shift buys up to five);
//   client/availability.js recruitAction, availability for 'buy' and 'cancelProduction', buyCount, popTotal;
//   shared/sim.js         priceOf, popUse, popCap (snapshot form), canBuild, allied, productionAccess, productionBuildings,
//                         isConstructionMode, isSkirmishBaseMode;
//   shared/tech.js        tierOf, TIER_NAMES.
// The server still validates and decides every purchase (shared/sim.js command 'buy' and 'cancelProduction'); a refusal
// comes back as { t: 'deny' }, which TCOrders::DenySentence words. This class only says what the browser would show and
// send. It runs no rules of its own.
//
// Data. Content/Data/units.json does not carry the recruit facts (cost, Classic price and fuel, pop, max, faction,
// Classic-only, naval, training time, HQ tier, what each building makes), so they are a table in TCRecruitment.cpp,
// generated from the reference at ReferencePin. Until SetUnitTableReference names the same pin, every purchase is refused
// with a native reason (bNative), so a moved pin cannot silently price units with stale numbers. The per-match facts
// come from the start and every snapshot (ApplyStart, ApplySnapshot); unit rows come from FTCMatchState.
//
// The HUD supplies faction names and draws the card letters. UnitTip and CardTip preserve the source role text.
// ETCCardKind::NotPorted delegates the Engineers' build card, World recovery and truck logistics to the HUD's modules.
#pragma once

#include "CoreMinimal.h"

class FJsonObject;
class FTCMatchState;

// One reference UNITS entry, the fields recruiting reads.
struct FTCRecruitDef
{
	static constexpr int32 AnyFaction = -2; // the reference's faction: undefined (any side may field it)
	FString Type, Name;
	int32 Cost = 0;
	int32 ClassicCost = -1;  // Classic and World MP price (-1: the reference has none, Cost is used)
	int32 Fuel = 0;          // Classic and World fuel price
	double Pop = 1;          // popUse: how much of the army limit one takes
	int32 Max = -1;          // the most one player may field and queue (-1: no limit)
	int32 Faction = AnyFaction; // -1: no player's side; 0..3: USA, Germany, USSR, UK only
	bool bClassic = false, bNaval = false, bStructure = false, bBuilding = false, bAir = false, bInfantry = false;
	int32 Train = 0;         // Classic training seconds (0: never trained at a building)
	int32 Tier = 1;          // tierOf: the HQ tier it needs when the match has tech
	TArray<FString> Makes;   // a Production Building's units
};

// One purchase: { t: 'buy', unit, from? }. From names the building asked (a selected one); 0 lets the server choose.
struct FTCBuy
{
	FString Unit;
	int32 From = 0;
	bool bHasFrom = false;
	TSharedRef<FJsonObject> ToMessage() const;
};

// availability()'s answer: ok, or the sentence the browser shows. bNative: the reason is this client's own (data it
// does not hold, a seat not yet confirmed, a pin mismatch), not one the browser would ever show.
struct FTCRecruitCheck
{
	bool bOk = false;
	bool bNative = false;
	FString Reason;
};

struct FTCPrice { int32 Mp = 0, Fuel = 0; };

// What client/hud.js buy does with a click or a card letter: the reason it shows instead, or the messages it sends
// (one, or up to five identical ones with Shift).
struct FTCBuyResult
{
	FTCRecruitCheck Check;
	TArray<TSharedRef<FJsonObject>> Messages;
	bool bPending = false; // drawQueue's disabled waiting slot, until the next HUD snapshot update
};

// A training job of an own Production Building (snapshot productionJobs).
struct FTCProductionJob
{
	int32 Id = 0;
	FString Type, Status; // 'active' or 'waiting'
	int32 Mp = 0, Fuel = 0; // what a cancel refunds
};

// A Production Building's queue (snapshot queues: [id, progress 0-1, rally x, rally z or -1, ...queued types]).
struct FTCProductionQueue
{
	double Progress = 0; // as sent (rounded to 0.1), kept a double so the seconds left round as the browser's do
	FVector2D Rally = FVector2D(-1, -1);
	TArray<FString> Types;
};

// One card: the unit, its labels as the browser writes them, whether it can be bought, and the purchase it makes.
struct FTCRecruitCard
{
	FString Type, Name;
	FString Cost;          // "100 MP"
	FString Sub;           // Classic: "60 Fuel, 35s" (empty outside Classic)
	bool bBroke = false;   // the browser's 'broke' look: not enough MP (or fuel in Classic)
	FTCRecruitCheck Check;
	FTCBuy Action;
};

// A card group (hud.js GROUPS), only the non-empty ones, in order; its cards in hud.js ORDER.
struct FTCRecruitGroup
{
	int32 Group = 0;       // 0 Infantry, 1 Support weapons, 2 Vehicles, 3 Aircraft, 4 Naval
	FString Name;
	TArray<FTCRecruitCard> Cards;
};

// One of a Classic building's five queue places (hud.js drawQueue).
struct FTCQueueSlot
{
	FString Type;          // empty: a free place
	bool bTraining = false;
	int32 Job = 0;         // a waiting job that a click cancels (0: not cancelable)
	int32 RefundMp = 0, RefundFuel = 0;
	FTCRecruitCheck Check; // cancelProduction's availability when Job != 0
	TSharedPtr<FJsonObject> Cancel; // { t: 'cancelProduction', id, job } when Job != 0
	bool bPending = false;
};

enum class ETCCardKind : uint8
{
	Hidden,     // Classic or World with nothing that has a card selected
	Recruit,    // outside Classic: every unit this side can recruit
	Building,   // Classic or World: the selected finished own Production Building's units and queue
	Site,       // Classic or World: the selected own building is still being built
	NotPorted,  // the browser shows a card this client does not build yet (Why says which)
};
const TCHAR* LexToString(ETCCardKind Kind);

struct FTCCommandCard
{
	ETCCardKind Kind = ETCCardKind::Hidden;
	FString Why;            // NotPorted: what the browser would show
	int32 Building = 0;     // Building, Site: the selected building
	float Built = 1.f;      // Site: how far it is built
	TArray<FTCRecruitGroup> Groups;
	// Recruit: the group drawRecruit opens when one finished own Production Building is selected (the one with most of
	// its units), else -1. The browser opens it once, when that selection starts, and closes it again when it ends.
	int32 SuggestedGroup = -1;
	// Building: "Training Rifle Squad, 9 s left" or "Idle: click a unit to train it", and the five queue places.
	FString QueueLine;
	bool bTraining = false;
	double Progress = 0;
	TArray<FTCQueueSlot> Slots;
};

// client/hud.js setRecruit, recruitBack, pressCard and drawRecruit's facility transition.
// Kept outside the renderer so mouse, keys and native transition tests use the same state.
struct THREECROSSROADS_API FTCRecruitNavigation
{
	bool bLetters = false;
	int32 Group = -1;
	bool bAutoOpened = false;
	bool bTruckCard = false;
	FString FacilityKey;

	void SetRecruit(bool bOn, bool bClassic = false);
	void Back();
	void ToggleGroup(int32 Index) { Group = Group == Index ? -1 : Index; }
	void ObserveFacility(const FString& Key, int32 SuggestedGroup);
	// drawLogistics replaces the DOM card for enabled all-truck selections, then rebuilds it on exit.
	void ObserveTruckCard(bool bShown);
	// Index is zero based. A group consumes the key without a buy; a card returns its real purchase.
	bool PressCard(int32 Index, const FTCCommandCard& Card, TOptional<FTCBuy>& Purchase);
	int32 LetterCount(const FTCCommandCard& Card) const;
};

class FTCRecruitment
{
public:
	// The reference commit the built-in recruit table was generated from.
	static const TCHAR* const ReferencePin;
	static const FTCRecruitDef* Find(const FString& Type);
	static int32 SelectionRank(const FString& Type);
	// client/hud.js unitTip and client/unit-roles.js, before the original English-keyed translation.
	static FString UnitTip(const FString& Type, const FString& DisplayName, const FString& Extra = FString());
	FString CardTip(const FTCRecruitCard& Card, const FString& DisplayName) const;
	// hud.js GROUPS and shared/tech.js TIER_NAMES.
	static const TCHAR* GroupName(int32 Group);
	static FString TierName(int32 Tier);

	// FTCUnitTable::Reference after it loads: purchases stay refused until it equals ReferencePin.
	void SetUnitTableReference(const FString& Reference);
	bool ReferenceMatches() const { return bReferenceMatches; }

	// Message hooks, called next to FTCMatchState's. A start forgets the last snapshot (client/main.js lastSnap = null);
	// a snapshot before any start is ignored, as FTCMatchState ignores it. Clear when the match ends (ClearMatch).
	void ApplyStart(const FJsonObject& M);
	void ApplySnapshot(const FJsonObject& M);
	void Clear();
	bool HasSnapshot() const { return bHasSnapshot; }

	// The snapshot's mode: Classic and World train at buildings (isConstructionMode); the skirmish modes recruit from
	// Production Buildings instantly (isSkirmishBaseMode); other modes (tutorial, ...) spawn at the player's spawn.
	bool Classic() const;
	bool SkirmishBase() const;
	int32 FactionOf(int32 Slot) const;              // client/main.js facOf
	bool Naval() const { return bNaval; }           // the start's map.naval === true
	FTCPrice PriceOf(const FString& Type) const;    // shared/sim.js priceOf
	const FTCProductionQueue* QueueOf(int32 BuildingId) const;
	double Mp() const { return SnapMp; }
	TOptional<double> Fuel() const { return SnapFuel; }

	// client/availability.js recruitAction: outside Classic, a card asks the first selected building that can make the
	// unit (in the order this client received the rows), else the server picks one.
	FTCBuy RecruitAction(const FTCMatchState& State, const FString& Unit, const TSet<int32>& Selected) const;
	// client/availability.js availability({ t: 'buy' }).
	FTCRecruitCheck CheckBuy(const FTCMatchState& State, const FTCBuy& Action) const;
	// client/availability.js availability({ t: 'cancelProduction', id, job }).
	FTCRecruitCheck CheckCancel(const FTCMatchState& State, int32 BuildingId, int32 Job) const;
	// client/availability.js buyCount: how many one Shift purchase buys, up to Want (0 when CheckBuy refuses).
	int32 BuyCount(const FTCMatchState& State, const FTCBuy& Action, int32 Want) const;
	// client/hud.js buy: refused with the reason, or one message (five at most with bMany, as buyCount allows).
	FTCBuyResult Buy(const FTCMatchState& State, const FTCBuy& Action, bool bMany) const;
	// client/hud.js cancel of a waiting queued unit (drawQueue): refused with the reason, or the one message.
	FTCBuyResult CancelProduction(const FTCMatchState& State, int32 BuildingId, int32 Job) const;
	// Call only after the cancel was sent. The next snapshot re-enables the slot, as hud.js drawQueue does.
	void NoteCancelSent(int32 BuildingId, int32 Job) { PendingCancels.FindOrAdd(BuildingId).Add(Job); }
	// The Command Card the browser would draw for this selection, with each card's availability.
	FTCCommandCard Card(const FTCMatchState& State, const TSet<int32>& Selected) const;
	// The army against its limit as client/hud.js drawEcon shows it ("pop/cap units"): popTotal of own units on the
	// field and queued, and s.world?.cap ?? popCap(s). False when either is unknown (no snapshot yet, an unknown type,
	// a World snapshot without its cap) or the seat is not this client's to command (a spectator, an unconfirmed seat).
	bool ArmyUse(const FTCMatchState& State, double& OutPop, double& OutCap) const;

private:
	bool bReferenceMatches = false;
	FString UnitTableReference;
	bool bStarted = false, bHasSnapshot = false;
	TArray<TOptional<int32>> Factions;
	bool bNaval = false;
	// the last snapshot's scalars (the server sends them whole every snapshot)
	double SnapMp = 0;
	TOptional<double> SnapFuel;
	FString ModeKind;                 // empty: the snapshot has no mode, or no kind
	bool bSuddenDeath = false;
	TOptional<double> ModeSlot;       // Horde: the attacking seat
	TOptional<double> TechTier;       // set when the match has tech
	TOptional<double> ArmyPop, WorldCap;
	bool bWorldRecovery = false;
	TArray<bool> OutSeats;
	TMap<int32, FTCProductionQueue> Queues;
	TMap<int32, TArray<FTCProductionJob>> Jobs;
	TMap<int32, TSet<int32>> PendingCancels;
	// client/world-state.js row order (a Map: rows keep their place, new ones go last, a full resend restarts it)
	TArray<int32> RowOrder;
	TSet<int32> RowIds;

	FTCRecruitCheck Gate(const FTCMatchState& State) const;
	bool Allied(const FTCMatchState& State, int32 A, int32 B) const;
	bool ProductionAccess(const FTCMatchState& State, int32 Owner) const;
	TArray<int32> ProductionBuildings(const FTCMatchState& State, const FString& Type) const;
	bool PopCap(double& Out) const;
	bool PopTotal(const FTCMatchState& State, double& Out, FString& Unknown) const;
	int32 Have(const FTCMatchState& State, const FString& Type) const;
	TArray<FTCRecruitGroup> Groups(const TArray<FString>& Types) const;
	double BuyCountJs(const FTCMatchState& State, const FTCBuy& Action, double Want) const;
};
