// Logistics: the recipient-visible stock and supply model, and the delivery controls, ported from the pinned reference
// (FTCLogistics::ReferencePin):
//   client/logistics.js   decodeLogistics, reserveLabels, truckLabels, storeForUnit, storeLabels, logisticsIndicator,
//                         overlayItems, createLogisticsAlerts (EVENT_TEXT);
//   client/hud.js         drawLogistics (the Logistics card), drawOrders (the trucks' Resume deliveries button), the
//                         logistics overlay and selection toggles and their visibility, tagsOf (the supply tag);
//   client/orders.js      kind and dispatch for the 'supply' order (a right-click with only trucks selected);
//   client/selection.js   box (the logistics drag selects only own mobile trucks, the ordinary drag never does);
//   client/main.js        startGame (what a start resets), applySnapshot (who the rows are decoded for, when alerts
//                         run), knownSupplyPosition, drawLogisticsRoutes and the minimap's logistics layer (colours).
//
// What the server sends (shared/supply-convoys.js logisticsSnapshot, at the pin), whole in every snapshot of a seated
// player and only when the match has logistics ('logistics' is absent otherwise, and server.js watcherSnapshot deletes
// it for spectators):
//   enabled: true;
//   units:   the receiver's own units' reserves { id, ammo (fraction or null), provisions, fuel|null, emergency|null,
//            warning|null, forced, stranded, supply, cut, grace|null };
//   regions: World Conquest only, [regionId, rate] for the receiver's team's regions;
//   trucks:  the receiver's own trucks { id, state, manual, hold, cargo { ammo, provisions, fuel }, destination|null,
//            route [{ x, z }] };
//   stores:  active stores owned by the receiver's team { id, owner, x, z, source, stock, capacity }. stock is
//            availableStock(store, receiver): what this receiver may draw (its own bucket less what it reserved for
//            relief recipients, plus relief other payers reserved for it), never another player's private bucket;
//   events:  the receiver's own alerts { id, type, count, x?, z?, message }.
// The client keeps only what the browser keeps: own units' and trucks' rows whose unit row this client holds as its own,
// stores whose owner is not a foe and that have a position. Before this client's seat is confirmed (a spectator, or a
// start that came before its lobby) nothing is kept, so no row can be shown as anyone's before ownership is known.
//
// Controls (the server still validates and decides everything; refusals come back as { t: 'deny' } and
// TCOrders::DenySentence words them):
//   { t: 'supply', ids, target, queue }: the selected own trucks deliver to a non-foe unit or building under the
//      cursor. Delivering to an allied (not own) Supply Cache is the reference's relief: the unloaded quantities are
//      reserved for that cache's owner (shared/supply-convoys.js deliver, donation).
//   { t: 'logisticsResume', ids }: the selected own trucks go back to automatic deliveries.
#pragma once

#include "CoreMinimal.h"

class FJsonObject;
class FJsonValue;
class FTCMatchState;
class FTCUnitTable;
struct FTCUnit;
struct FTCCursor;

// One JSON value as the reference client's JavaScript reads it: nullish (== null), truthiness, Number.isFinite,
// amount() and the relational operators' ToNumber. Missing is JavaScript's undefined (the field was absent).
struct FTCJsValue
{
	enum class EKind : uint8 { Missing, Null, Bool, Number, String, Array, Object };
	EKind Kind = EKind::Missing;
	bool bBool = false;
	double Number = 0;
	FString String;
	TSharedPtr<FJsonObject> Obj; // Object only

	static FTCJsValue From(const TSharedPtr<FJsonValue>& V);
	static FTCJsValue Field(const FJsonObject& O, const TCHAR* Name);
	// A field of this value: an object's field, else undefined (what JavaScript reads from a primitive or an array).
	FTCJsValue Get(const TCHAR* Name) const;

	bool IsNullish() const { return Kind == EKind::Missing || Kind == EKind::Null; }
	bool IsFiniteNumber() const;                    // Number.isFinite
	bool Truthy() const;                            // JavaScript truthiness
	// ToNumber as a relational comparison against a number applies it. Exact for undefined, null, booleans, numbers and
	// strings; arrays and objects give NaN (the browser would go through their string form; the reference sends none).
	double ToNumber() const;
	double Amount() const;                          // client/logistics.js amount: max(0, finite number else 0)
	bool IsPosition() const;                        // client/logistics.js position: an object with finite x and z
	// A key with JavaScript Map semantics (SameValueZero): 1 and 1.0 match, 0 and -0 match, '1' and 1 do not, and
	// every object or array is its own key.
	FString MapKey() const;
};

// A { ammo, provisions, fuel } object: truck cargo, store stock or capacity.
struct FTCSupplies
{
	FTCJsValue Ammo, Provisions, Fuel;
	static FTCSupplies From(const FTCJsValue& V);   // a missing or non-object value reads as all undefined
};

// One own unit's reserves (logistics.units).
struct FTCLogisticsReserve
{
	int32 Id = 0;
	FTCJsValue Ammo, Provisions, Fuel, Emergency, Warning, Forced, Stranded, Supply, Cut, Grace;
};

struct FTCRoutePoint
{
	bool bValid = false; // client/logistics.js position(): finite x and z
	double X = 0, Z = 0;
};

// One own truck (logistics.trucks).
struct FTCLogisticsTruck
{
	int32 Id = 0;
	FTCJsValue State, Manual, Hold;
	FTCSupplies Cargo;
	bool bDestination = false; // position(row.destination)
	double DestX = 0, DestZ = 0;
	TArray<FTCRoutePoint> Route; // a missing, null or non-array route reads as empty
};

// One non-foe store this client may see (logistics.stores). Stock is what this client may draw from it.
struct FTCLogisticsStore
{
	FTCJsValue Id;          // 'source:<unit id>', 'source:<slot>', 'cache:<unit id>' or 'support:<unit id>' at the pin
	int32 Owner = -1;
	double X = 0, Z = 0;
	bool bSource = false;   // row.source truthy: an HQ or spawn source (unlimited), else a finite store
	FTCSupplies Stock, Capacity;
};

// One grouped alert, as createLogisticsAlerts update returns it: the HUD's Alerts show Text at (X, Z).
struct FTCLogisticsAlert
{
	FString Key, Text;      // Text already carries ": <count>"
	double X = 0, Z = 0;
	double Count = 0;
};

// One block of the Logistics card (hud.js drawLogistics): a selected unit and its lines. The browser titles it with
// the unit's name and '#<id>' (faction names are the HUD's).
struct FTCLogisticsEntry
{
	int32 Id = 0;
	FString Type;
	TArray<FString> Lines;
};

struct FTCLogisticsCard
{
	bool bVisible = false;  // at least one selected unit has lines
	bool bTrucks = false;   // every selected unit is a truck: the hint and the Resume deliveries button follow
	TArray<FTCLogisticsEntry> Entries;
	static const TCHAR* const Hint;         // "Right-click an allied unit or Supply Cache to deliver carried cargo"
	static const TCHAR* const ResumeLabel;  // "Resume deliveries"
};

// What the logistics overlay draws (overlayItems): known stores, and the selected own trucks' fully known routes.
struct FTCSupplyRoute
{
	int32 Id = 0;
	TArray<FVector2D> Points; // metres; the first is the truck when its row is held
	bool bManual = false, bHold = false;
};
struct FTCLogisticsOverlay
{
	TArray<FTCLogisticsStore> Stores;
	TArray<FTCSupplyRoute> Routes;
};

enum class ETCLogisticsOrderKind : uint8
{
	None,        // not a logistics click (or nothing to send); the ordinary order path applies
	Supply,      // { t: 'supply', ids, target, queue }
	Resume,      // { t: 'logisticsResume', ids }
	Unsupported, // the browser would send something this client does not build (Why says what)
};
const TCHAR* LexToString(ETCLogisticsOrderKind Kind);

struct FTCLogisticsOrder
{
	ETCLogisticsOrderKind Kind = ETCLogisticsOrderKind::None;
	TSharedPtr<FJsonObject> Message;
	TArray<int32> Trucks;       // the truck ids named in the message
	int32 Target = 0;           // Supply: the unit or building delivered to
	bool bRelief = false;       // Supply: the target is an allied player's Supply Cache (the server's relief)
	FVector2D At = FVector2D(-1, -1); // where the marker goes (the target)
	FColor Color = FColor(0xa8, 0xd6, 0x80); // client/orders.js supply feedback colour
	FString Why;
};

class FTCLogistics
{
public:
	static const TCHAR* const ReferencePin;

	// Message hooks. ApplyStart after FTCMatchState::ApplyStart (client/main.js startGame: alerts reset, data cleared,
	// logistics selection off; the overlay switch is kept). ApplySnapshot after FTCMatchState::ApplySnapshot, so the
	// unit rows the ownership check reads are this snapshot's; ignored while State holds no match. Clear when the match
	// ends (ClearMatch).
	void ApplyStart();
	void ApplySnapshot(const FJsonObject& M, const FTCMatchState& State);
	void Clear();

	bool HasSnapshot() const { return bHasSnapshot; }
	bool Enabled() const { return bEnabled; }   // the last snapshot's logistics.enabled === true
	// The toolbar's overlay and logistics-selection buttons are shown (hud.js: data.enabled && !ctx.watching).
	bool ControlsVisible(const FTCMatchState& State) const;
	void ToggleOverlay() { bOverlay = !bOverlay; }
	bool OverlaySwitch() const { return bOverlay; }
	bool OverlayActive() const { return bOverlay && bEnabled; }
	void ToggleSelectionMode() { bSelectionMode = !bSelectionMode; }
	bool SelectionSwitch() const { return bSelectionMode; }
	bool SelectionModeActive() const { return bSelectionMode && bEnabled; } // client/main.js logisticsSelection()

	// The decoded rows, in the order the browser's Maps hold them.
	const TArray<FTCLogisticsReserve>& Reserves() const { return ReserveRows; }
	const TArray<FTCLogisticsTruck>& Trucks() const { return TruckRows; }
	const TArray<FTCLogisticsStore>& Stores() const { return StoreRows; }
	const FTCLogisticsReserve* FindReserve(int32 UnitId) const;
	const FTCLogisticsTruck* FindTruck(int32 UnitId) const;
	// client/logistics.js storeForUnit: the store of the unit's owner within 0.5 m of it (an HQ, a cache, a halftrack).
	const FTCLogisticsStore* StoreForUnit(const FTCUnit& Unit) const;
	// World Conquest: an own region's supply rate as sent. False when the region has none.
	bool RegionRate(int32 RegionId, FTCJsValue& Out) const;
	// The minimap's tint for a World region while the overlay is on: red cut off, amber weakened, green full. False when
	// the overlay is off, logistics is off or the region has no rate.
	bool RegionTint(int32 RegionId, FColor& Out) const;

	// client/logistics.js label functions (English, as the reference writes them). A null row gives no lines.
	static TArray<FString> ReserveLabels(const FTCLogisticsReserve* Row);
	static TArray<FString> TruckLabels(const FTCLogisticsTruck* Row);
	static TArray<FString> StoreLabels(const FTCLogisticsStore* Row);
	// client/logistics.js logisticsIndicator: '', 'Stranded', 'Mandatory withdrawal', 'Cut off from supply' or
	// 'Low supplies'. IndicatorFor reads the unit's own reserve row.
	static FString Indicator(const FTCLogisticsReserve* Row);
	FString IndicatorFor(int32 UnitId) const { return Indicator(FindReserve(UnitId)); }
	// client/main.js supplyFlag colour for a non-empty indicator: amber for low supplies, red otherwise.
	static FColor IndicatorColor(const FString& Indication);
	// The minimap's red rings: own units whose reserve row says cut off, among the units State holds.
	TArray<int32> CutOffUnits(const FTCMatchState& State) const;

	// Alerts from the snapshots since the last call (createLogisticsAlerts update; run only for a confirmed seat that is
	// not out, at the snapshot's tick / 20 seconds).
	TArray<FTCLogisticsAlert> TakeAlerts();

	// hud.js drawLogistics for the selection, in the order given (the browser's selection order).
	FTCLogisticsCard Card(const FTCMatchState& State, const TArray<int32>& SelectedInOrder) const;
	// client/logistics.js overlayItems. Known says whether a map position may be drawn; KnownByFog is the browser's
	// knownSupplyPosition (no fog, or the cell seen or explored), without the browser's between-snapshot lit cells.
	FTCLogisticsOverlay Overlay(const FTCMatchState& State, const TSet<int32>& Selected, TFunctionRef<bool(double X, double Z)> Known) const;
	static bool KnownByFog(const FTCMatchState& State, double X, double Z);
	// drawLogisticsRoutes and the minimap colours.
	static FColor RouteColor(bool bManual) { return bManual ? FColor(0xe8, 0xc8, 0x60) : FColor(0x95, 0xc8, 0x88); }
	static FColor StoreRingColor(bool bSource) { return bSource ? FColor(0xe8, 0xc8, 0x60) : FColor(0x95, 0xc8, 0x88); }
	static FColor MinimapStoreColor(bool bSource) { return bSource ? FColor(0xe8, 0xc8, 0x60) : FColor(0xa8, 0xd6, 0x80); }
	static FColor MinimapRouteColor() { return FColor(0xa8, 0xd6, 0x80); }
	static FColor MinimapCutColor() { return FColor(0xd4, 0x57, 0x4a); }

	// client/selection.js box: with the logistics selection on, only own mobile trucks; otherwise own mobile army units
	// that are not trucks or trains. bLogisticsMode is SelectionModeActive().
	static bool BoxSelectable(const FTCMatchState& State, const FTCUnitTable& Table, const FTCUnit& Unit, bool bLogisticsMode);

	// client/orders.js: a right-click with only trucks among the own selected troops, over a non-foe unit (first) or a
	// non-foe building. None when the click is not a supply click.
	static FTCLogisticsOrder PlanSupply(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bQueue);
	// hud.js: the trucks' Resume deliveries button is shown when every selected unit is a truck.
	static bool ResumeShown(const FTCMatchState& State, const TSet<int32>& Selected);
	// hud.js data-truck-resume / data-logistics-resume: { t: 'logisticsResume', ids: the selected own trucks }.
	static FTCLogisticsOrder PlanResume(const FTCMatchState& State, const TSet<int32>& Selected);

private:
	bool bHasSnapshot = false, bEnabled = false, bOverlay = false, bSelectionMode = false;
	TArray<FTCLogisticsReserve> ReserveRows;
	TMap<int32, int32> ReserveIndex;
	TArray<FTCLogisticsTruck> TruckRows;
	TMap<int32, int32> TruckIndex;
	TArray<FTCLogisticsStore> StoreRows;
	TMap<FString, int32> StoreIndex;
	TArray<TPair<int32, FTCJsValue>> RegionRows;
	TMap<int32, int32> RegionIndex;
	// createLogisticsAlerts: event ids already seen (in arrival order, for the browser's trim), group keys' last alert
	TSet<FString> Seen;
	TArray<FString> SeenOrder;
	TArray<TPair<FString, double>> Recent;
	TArray<FTCLogisticsAlert> PendingAlerts;
	bool bWarnedMalformed = false;

	void ResetData();
	void UpdateAlerts(const TArray<TSharedPtr<FJsonValue>>& Events, double Time);
};
