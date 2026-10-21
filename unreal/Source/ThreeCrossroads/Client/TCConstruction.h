// Construction: a port of the reference's builder card, build orders and building placement, for the facts this client
// actually receives. Ported from the pinned reference (FTCConstruction::ReferencePin):
//   client/hud.js          drawClassicCard (the Engineers' Build card in Classic and World), the Build menu's building
//                          buttons outside Classic (menuHTML 'build' and drawOrders 'bld:'), BUILD_ROLE;
//   client/availability.js availability for { t: 'build' }, placementState and rememberPlacementTerrain (the placement
//                          view built from this player's snapshot and map only);
//   client/main.js         builders, owns, canPlace, startBuild, footAt, setAim (the hint), the armed building click
//                          (mousedown) and its preview (frame), the terrain layers kept from the start and its cells;
//   client/world-state.js  the held unit rows and their order; client/logistics.js decodeLogistics (enabled);
//   shared/sim.js          placementCheck with teamSees (sight from the received units only), footprint, footCenter,
//                          canStamp, onCoast, railYard, worldRegionAt (the client's region list), los, buildKinds,
//                          builderTypes, productionAccess, allied; shared/world-layers.js composeWorldCell and
//                          migrateWorldMap; shared/world-territories.js insideKnownRegion; client/keys.js BUILD_KEYS.
// Nothing here is a rule of its own and nothing asks the server or the Rust core: the server decides every build
// again (shared/sim.js command 'build'); a refusal comes back as { t: 'deny' }, which TCOrders::DenySentence words.
//
// Data. The buildable kinds, their costs, sizes, tiers, keys and roles, every unit type's sight, and the terrain flags
// are generated from the reference into Client/TCConstructionData.inl by unreal/Tools/export-build-table.mjs. Until
// SetUnitTableReference names the same pin, every check refuses with a native reason (bNative).
//
// The browser's own failures are kept, not fixed: its placement view has no seat -1 (World's local defenders, its
// neutral buildings) and no row for a type it does not know, so teamSees throws when it meets one of those units before
// one of this side that sees the spot, and canStamp throws on an unknown type standing alive. Where the browser would
// throw, FTCPlacement::bThrows is set: the click then does nothing at all (no sentence, no order, the aim stays armed),
// and the preview keeps its last colour.
//
// Not ported: the Build menu's fortification and trench buttons (dig, entrench), assist and repair (a right-click with
// builders), Cancel of a site, tooltips' layout, the 3D aim shape. The operative (FPS) seat is not modelled.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCRecruitment.h"

class FJsonObject;
class FJsonValue;
class FTCMatchState;
namespace TCBuild { struct FPlacementView; }

// One buildable kind (shared/sim.js UNITS, shared/tech.js tierOf, client/keys.js BUILD_KEYS, client/hud.js BUILD_ROLE).
struct FTCBuildDef
{
	FString Kind, Name;
	int32 Cost = 0, BuildTime = 0, Size = 1, Tier = 1;
	FString Needs;   // the finished building it needs first (empty: none)
	bool bCoast = false;
	FString Key;     // the key that arms it in Classic and World (empty: none)
	FString Role;    // BUILD_ROLE (empty: none)
};

// availability()'s answer, as for recruiting: ok, or the sentence the browser shows (bNative: this client's own).
using FTCBuildCheck = FTCRecruitCheck;

// One button of the Engineers' Build card (Classic and World) or of the Build menu outside Classic.
struct FTCBuildButton
{
	FString Kind, Name;
	FString Key;           // the Build card's key badge (empty: none); the Build menu shows none
	FString Cost;          // Build card: "150 MP, 30s"; Build menu: "150 MP"
	FString Tip;           // the button's title before availability replaces it with the reason
	FString Note;          // Build card: the role when the check passes, else its reason (the Build menu has none)
	FString Tab;           // Build menu: "base" or "defense" (the Build card has none)
	bool bBroke = false;   // Build card: !(mp >= cost), the browser's 'broke' look
	bool bDanger = false;  // Build card: it needs a building this player has no finished one of (the note turns red)
	FTCBuildCheck Check;   // availability({ t: 'build', kind }); a click arms the aim regardless (Arm explains)
};

// The Engineers' Build card (client/hud.js drawClassicCard: Classic or World, no single own building selected, an own
// Engineer selected).
struct FTCBuildCard
{
	bool bShown = false;
	TArray<FTCBuildButton> Buttons;
};

// The building buttons of the Build menu outside Classic (client/hud.js menuHTML 'build'). bOffered: the Build menu
// exists for this selection (drawOrders: something selected, not all buildings or all trucks, a builder squad among it).
struct FTCBuildMenu
{
	bool bOffered = false;
	TArray<FTCBuildButton> Buttons;
};

// client/main.js footAt: shared/sim.js placementCheck on this player's placement view.
struct FTCPlacement
{
	bool bOk = false;
	FString Reason;        // the server's code when refused: blocked, notVisible, territory, rail, coast
	// The browser would throw here (see the file comment): no sentence, no order, nothing redrawn.
	bool bThrows = false;
	// This client could not judge (no placement view yet, a reference pin mismatch): NativeReason says why. Reason is
	// then 'notVisible', which is what footAt answers without a view.
	bool bNative = false;
	FString NativeReason;
	// Where the preview stands and where the order builds: the footprint's centre once the footprint is known, else the
	// click. bHasFootprint: Cell, Size and Cells are set (the top-left cell, the side in cells, every cell).
	FVector2D At = FVector2D::ZeroVector;
	bool bHasFootprint = false;
	int32 Cell = -1, Size = 0;
	TArray<int32> Cells;
	int32 Node = -1;       // a depot: the index of the resource node it takes in the snapshot's nodes (-1: none)
	// client/main.js frame: the preview's colour (0x7acb82 ok, 0xdc4a3c refused)
	FColor PreviewColor() const { return bOk ? FColor(0x7a, 0xcb, 0x82) : FColor(0xdc, 0x4a, 0x3c); }
};

// client/main.js startBuild: refused with the reason, or the aim is armed with the hint (setAim).
struct FTCBuildArm
{
	FTCBuildCheck Check;
	bool bArm = false;
	FString Hint;          // "Click to set the center. Right-click cancels" (as the browser words it per kind)
};

// client/main.js mousedown with a building armed: what the click does.
struct FTCBuildClick
{
	FTCBuildCheck Check;   // availability first (refused: Feedback is its reason); not asked when there is no ground
	FTCPlacement Placement;
	FString Feedback;      // the sentence shown (empty: none)
	TSharedPtr<FJsonObject> Message; // { t: 'build', ids, kind, x, z, queue } when sent
	bool bKeepAim = true;  // false: cancelAim (a plain click that sent the order); Shift keeps it armed
	FVector2D Marker = FVector2D(-1, -1); // where the order's marker goes (0xe8c860), when sent
	bool bThrows = false;  // the browser throws (nothing happens at all)
};

class FTCConstruction
{
public:
	// The reference commit the generated construction data came from.
	static const TCHAR* const ReferencePin;
	static const FTCBuildDef* Find(const FString& Kind);
	// shared/sim.js buildKinds(classic, skirmish, world)
	static TArray<FString> BuildKinds(bool bClassic, bool bSkirmish, bool bWorld);
	// shared/sim.js builderTypes(classic)
	static TArray<FString> BuilderTypes(bool bClassic);
	// The kind client/keys.js binds to a key letter ("J" -> depot) in the 'classic' key context, or empty.
	static FString KindForKey(const FString& Key);

	// FTCUnitTable::Reference after it loads: every check stays refused until it equals ReferencePin.
	void SetUnitTableReference(const FString& Reference);
	bool ReferenceMatches() const { return bReferenceMatches; }

	// Message hooks, called after FTCMatchState's own (it must have accepted the start): a start keeps the map's terrain
	// layers and forgets the last snapshot; a snapshot before any start is ignored. Clear when the match ends.
	void ApplyStart(const FJsonObject& M);
	void ApplySnapshot(const FJsonObject& M);
	void Clear();
	bool HasSnapshot() const { return bHasSnapshot; }

	// client/availability.js availability({ t: 'build', kind }) for the selection.
	FTCBuildCheck CheckBuild(const FTCMatchState& State, const FString& Kind, const TSet<int32>& Selected) const;
	// client/hud.js drawClassicCard's Build card for this selection (bShown false when the browser shows another card).
	FTCBuildCard BuildCard(const FTCMatchState& State, const TSet<int32>& Selected) const;
	// client/hud.js menuHTML('build'): the building buttons outside Classic.
	FTCBuildMenu BuildMenu(const FTCMatchState& State, const TSet<int32>& Selected) const;
	// client/main.js startBuild (a Build card or menu click, or a 'classic' build key).
	FTCBuildArm Arm(const FTCMatchState& State, const FString& Kind, const TSet<int32>& Selected) const;
	// client/main.js footAt(kind, at): the preview under the pointer, and what the click would build.
	FTCPlacement Placement(const FTCMatchState& State, const FString& Kind, const FVector2D& At) const;
	// client/main.js mousedown with Kind armed: bHasGround false is a click off the ground ("blocked"). bShift queues
	// the build behind the builders' orders and keeps the aim armed.
	FTCBuildClick Click(const FTCMatchState& State, const FString& Kind, bool bHasGround, const FVector2D& Ground, bool bShift, const TSet<int32>& Selected) const;
	// client/main.js builders(): the selected held units of a builder type (by the lobby's mode) that are not retreating.
	TArray<int32> Builders(const FTCMatchState& State, const TSet<int32>& Selected) const;

private:
	// A held unit row as the browser keeps it: unsmoothed snapshot values, as JSON numbers (NaN where a row lacks one,
	// which compares as JavaScript's undefined does). An unknown or missing type is kept: the browser keeps it too.
	struct FRow
	{
		FString Type;
		double Owner = 0, X = 0, Z = 0, Hp = 0, Built = 1;
		int32 Flags = 0;
	};
	// A start's teams entry (null in the JSON: bNull) or a seat number standing in for it.
	struct FTeam { bool bNull = false; double V = 0; };
	struct FRegion
	{
		bool bTeam = false; double Team = 0;
		bool bRuns = false; TArray<FVector3d> Runs; // [row, first column, end column)
		bool bBounds = false; double Bounds[4] = { 0, 0, 0, 0 };
	};
	struct FSmoke { double X = 0, Z = 0, R = 0; };

	bool bReferenceMatches = false;
	FString UnitTableReference;

	// the start
	bool bStarted = false;
	int32 W = 0, H = 0;
	bool bMapWorld = false, bNavalMap = false;
	TArray<FTeam> Teams;              // client/main.js teams: m.teams, or every seat its own team
	TArray<uint8> Ground, Objects;    // the browser's terrain.groundGrid and objectGrid (0: undefined, 255: not one character)
	TSet<int64> RailYard;             // World: rememberPlacementTerrain's railYard (every track cell and station yard seen)
	bool bRailYard = false;           // the browser's railYard exists once a rail or station cell arrived

	// the last snapshot
	bool bHasSnapshot = false;
	double SnapMp = 0;                // NaN when a snapshot has none
	FString ModeKind;                 // empty: no mode, or no kind
	bool bMode = false, bSuddenDeath = false;
	bool bModeSlot = false; double ModeSlot = 0;
	bool bTech = false; double TechTier = 0;
	bool bLogistics = false;
	TArray<bool> OutSeats;
	TArray<FVector2D> NodeRows;       // carried over when a snapshot leaves nodes out (client/world-state.js)
	TArray<FSmoke> Smokes;
	bool bHasWorld = false;           // the snapshot has world (World Conquest)
	TArray<FRegion> Regions;
	TMap<int32, FRow> Rows;
	TArray<int32> RowOrder;           // client/world-state.js Map order

	void ApplyCells(const TArray<TSharedPtr<FJsonValue>>& Cells);
	bool SnapshotClassic() const;
	FTeam TeamOfSeat(double Seat, bool& bOutExists) const;
	bool TeamsEqual(const FTeam& A, const FTeam& B) const;
	FTeam MyTeam(const FTCMatchState& State) const;
	bool SnapshotSkirmish() const;
	FTCBuildCheck Gate(const FTCMatchState& State) const;
	bool Allied(double A, double B) const;
	bool ProductionAccess(const FTCMatchState& State, double Owner) const;
	bool LobbyClassic(const FTCMatchState& State) const;
	bool Owns(const FTCMatchState& State, const FString& Type) const;

	friend struct TCBuild::FPlacementView;
};
