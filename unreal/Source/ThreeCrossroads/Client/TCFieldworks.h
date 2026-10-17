// Fieldworks: a port of the reference's field fortifications for the facts this client actually receives: the Build
// menu's fortification buttons, the Trench patterns menu, the dig and entrench aims with their previews, the right-click
// that sends more squads to a planned pattern, and the ghost of the works still to dig. Ported from the pinned reference
// (FTCFieldworks::ReferencePin):
//   shared/sim.js          FORTS, CFG.fortBuilders, lineFort, fortBuilders/builderTypes, ENTRENCH, ENTRENCH_TYPES,
//                          entrenchPlan, segmentCost, squareDir, BUILDABLE_GROUND, fortCells, fillable, level0,
//                          supportedBridgePlan and bridgeSupportedAt (as they run on the browser's placement view, which
//                          has no structures), placementCheck (the FORTS branch), teamSees and los; the snapshot's works;
//   client/availability.js availability({ t: 'dig' }) and ({ t: 'entrench' }), placementState and
//                          rememberPlacementTerrain (the placement view and its remembered starting terrain);
//   client/main.js         diggers, startDig, startEntrench, entrenchName, entrenchSegments, entrenchSummary, TILE_COLOR,
//                          drawWorks and worksAt, nearestDigger, defaultDir('dig'), footAt, aimShape('dig'), the armed dig
//                          and entrench clicks (mousedown) and their previews (frame), the work bar's dig progress;
//   client/orders.js       kind 'entrench' and its { t: 'entrench', ids, join } command;
//   client/hud.js          menuHTML('build')'s fortification buttons, menuHTML('trench'), FORT_TIP, ENTRENCH_TIP,
//                          FORT_TAB, BUILD_TABS, the 'Digging' and 'Waiting for MP to dig' tags;
//   client/keys.js         the fort:* and entrench:line bindings, FORT_KEYS and FORT_BADGES.
// Nothing here is a rule of its own and nothing asks the server or the Rust core: the server decides every dig and
// entrench again (shared/sim.js commands 'dig' and 'entrench'); a refusal comes back as { t: 'deny' }.
//
// Data. FORTS and the button texts are written out here from the reference at ReferencePin (checked field by field
// against the reference by a script, see the change notes). Unit sight comes from the generated
// Client/TCConstructionData.inl. Until SetUnitTableReference names the same pin, every check refuses (bNative).
//
// What this client cannot know, and so does not claim (exact knowledge limits):
// - Positions. The browser takes the digger nearest a click, the diggers' centre ("back") and the drawn works from its
//   smoothed unit positions. Every call that needs them takes FTCPositionOf; without one the held snapshot positions
//   are used, which differ from the browser's while units move.
// - Selection order. The browser walks its selection Set in insertion order (the ids it sends, ties for the nearest
//   digger). TSet<int32> has no insertion order: the caller's iteration order is used.
// - Terrain. The placement view is built from FTCMatchState's map (characters and levels) plus the ground and object
//   layers and the starting terrain this class keeps from the start and every cells list. A layer value that is a
//   string of more than one character, or a JSON value that is not a string, is kept as "some other value"; the
//   server sends single characters. Levels come from FTCMatchState::Levels (int8), as FTCConstruction's do.
// - The server's own truth: mines it does not show this side, structures (a bridge's surviving sections), other
//   teams' works. The browser's view has none of these either; the server may still refuse (blocked, notVisible,
//   mp, tier, max, queueFull) and the refusal arrives as a deny.
// - Where the browser would throw (an unknown seat or unit type met by teamSees, Clear mines placed with a team and a
//   mine cell in the plan, since the view has no mine list), bThrows is set: nothing happens at all.
// - The browser's aim mesh is kept between frames: its rotation and its size (made for the fort kind armed when the
//   mesh was created, kept when another dig kind is armed without a frame in between) live in FTCFieldAim. A mesh is
//   recreated when the aim kind changes; Cancel drops it (the browser drops it on the next frame with no aim).
// - The browser's hint text and its 2 s feedback sentence are two layers; FTCFieldAim::Hint is the hint, and the
//   results' Feedback is the sentence the browser flashes over it.
// Not ported: the 3D shapes of the aim, cell layer and ghost (their sizes, places and colours are given), sounds and
// voice barks, the operative (FPS) seat, the facing gesture (right-drag) that can start from a right-click.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCRecruitment.h"

class FJsonObject;
class FJsonValue;
class FTCMatchState;
struct FTCUnit;
namespace TCField { struct FView; }

// availability()'s answer, as for recruiting and building: ok, or the sentence the browser shows (bNative: this
// client's own).
using FTCFieldCheck = FTCRecruitCheck;

// Where the caller draws a unit (client/main.js units.get(id).x, .z): false when the client draws no such unit.
using FTCPositionOf = TFunction<bool(int32 Id, FVector2D& Out)>;

// One FORTS kind (shared/sim.js), with its Build menu button (client/hud.js, client/keys.js).
struct FTCFortDef
{
	FString Kind, Name;
	int32 Cost = 0;
	TCHAR Ch = 0;          // the cell it makes (0: none, the concrete wall and gate become buildings)
	int32 N = 0;           // cells in one piece (0: the Gun Pit, which has its own shape)
	bool bNest = false, bSingle = false, bPoint = false, bFill = false, bClear = false, bAlong = false;
	TCHAR On = 0;          // the ground it goes on (the bridge: 'W'); 0: BUILDABLE_GROUND
	double Reach = -1;     // the builders work from this far (-1: none)
	double Range = 0;      // the Gun Pit's range multiplier for a weapon team in it (0: none)
	int32 Time = 0;        // seconds per cell (0: CFG.digTime)
	FString Unit;          // the building a finished cell becomes (wall, gate), empty: none
	int32 Tier = 0;        // the HQ tier it needs (0: none)
	bool bLine = false;    // lineFort(kind): drawn out as a line, so its button arms an entrench 'line'
	FString Tab;           // client/hud.js FORT_TAB ?? 'field'
	FString KeyLabel;      // client/keys.js FORT_KEYS ("T", "Shift+Y"), empty: no key
	FString Badge;         // FORT_BADGES ("T", "⇧Y"), empty: none
	FString Tip;           // FORT_TIP (empty for none, as the browser's '')
};

// One entrenchment pattern (shared/sim.js ENTRENCH, client/hud.js ENTRENCH_TIP).
struct FTCEntrenchDef
{
	FString Kind, Name, Tip;
};

// One segment of an entrenchment plan (shared/sim.js entrenchPlan): a fortification centred on X, Z running along Dir.
struct FTCFortSegment
{
	FString Kind;
	double X = 0, Z = 0, Dir = 0;
	double Front = 0;      // where a trench faces (atan2); the plan always sets it
};

// shared/sim.js placementCheck for a FORTS kind on this client's placement view.
struct FTCFortPlacement
{
	bool bOk = false;
	FString Reason;        // the server's code when refused: blocked or notVisible
	bool bThrows = false;  // the browser would throw here (see the file comment): nothing happens at all
	bool bNative = false;  // this client could not judge (no view, a pin mismatch, the map changed size): NativeReason
	FString NativeReason;
	FVector2D At = FVector2D::ZeroVector;
	double Dir = 0;        // the direction asked (the server squares a gate's itself)
	TArray<int32> Cells;   // the cells it would build, in the reference's order
	TArray<TCHAR> CellChars; // what each becomes (0: none)
	// client/main.js frame: the dig preview's colour (0x7acb82 ok, 0xdc4a3c refused)
	FColor PreviewColor() const { return bOk ? FColor(0x7a, 0xcb, 0x82) : FColor(0xdc, 0x4a, 0x3c); }
};

// A planned segment with its placement and price (client/main.js entrenchSegments, segmentCost).
struct FTCEntrenchPiece
{
	FTCFortSegment Segment;
	FTCFortPlacement Place;
	int32 Cost = 0;
	FColor Color;          // client/main.js TILE_COLOR[kind] ?? 0x60e070, for each of its cells
};

// One row of the snapshot's works (shared/sim.js snapshotFor): a segment still to dig of an allied pattern, or a work
// a squad is walking to or digging now. Project is the pattern's id while it still has segments to hand out, else -1.
struct FTCWork
{
	int32 Project = -1;
	bool bWire = false;
	double X = 0, Z = 0;
	double Dir = 0;        // a missing or null dir is 0, as placementCheck's default
	bool bDirNumber = true;// false: the row's dir was some other value (the browser computes NaN cells: nothing drawn)
	FString Kind;          // the 6th element, or wire/trench from bWire when it is missing
};

// The ghost of one work (client/main.js drawWorks): its cells, drawn faint, in its colour.
struct FTCWorkGhost
{
	FTCWork Work;
	FTCFortPlacement Place;
	FColor Color;
};

// One button of the Build menu's fortifications or of the Trench patterns menu (client/hud.js menuHTML).
struct FTCFieldButton
{
	FString Action;        // "fort:trench", "ent:line" (data-a)
	FString Kind;          // the fort kind or the pattern
	FString Name;
	FString Icon;          // the icon name (the fort kind, or "e_" and the pattern)
	FString Badge;         // the key badge on the button (empty: none)
	FString Tip;           // the button's title before availability replaces it with the reason
	FString Value;         // "30 MP" for a fort, "30/seg" for a pattern
	FString Tab;           // Build menu: "field", "defense" or "eng"; Trench menu: empty
	FTCFieldCheck Check;   // availability({ t: 'dig', kind }) or ({ t: 'entrench' }); a click still runs Start*
};

struct FTCFieldMenu
{
	bool bOffered = false; // client/hud.js drawOrders: the Build and Trench menu buttons exist for this selection
	TArray<FTCFieldButton> Buttons;
};

// The armed field aim (client/main.js targeting 'dig' or 'entrench', fortKind, entrenchKind, entrenchFort, aimCenter,
// the hint, and the aim mesh it keeps between frames). Owned by the caller; one at a time with its other aims.
enum class ETCFieldAim : uint8 { None, Dig, Entrench };
struct FTCFieldAim
{
	ETCFieldAim Kind = ETCFieldAim::None;
	FString FortKind = TEXT("trench");        // the dig's kind
	FString EntrenchKind = TEXT("line");      // the pattern
	FString EntrenchFort = TEXT("trench");    // what a line pattern is made of
	bool bHasCenter = false;                  // the first click pinned the centre (aimCenter)
	FVector2D Center = FVector2D::ZeroVector;
	FString Hint;                             // the hint line (client/main.js $('hint'))
	// the aim mesh (client/main.js aimMesh): the kind it was made for, the dig kind whose size it has, its rotation
	ETCFieldAim MeshKind = ETCFieldAim::None;
	FString MeshFort;
	bool bMeshPlaced = false;                 // the dig marker has been put somewhere (else it is at the origin)
	FVector2D MeshAt = FVector2D::ZeroVector;
	double MeshRotation = 0;                  // three.js rotation.y: the dig preview runs along -MeshRotation
	FColor MeshColor = FColor(0xf4, 0xdc, 0x90);
	TArray<FTCEntrenchPiece> MeshPieces;      // the entrench preview's cells, kept when a frame has no ground
	bool IsArmed() const { return Kind != ETCFieldAim::None; }
	// client/main.js cancelAim (and the frame after it, which drops the mesh)
	void Cancel();
};

// client/main.js startDig / startEntrench: refused with the sentence, or the aim armed with its hint.
struct FTCFieldArm
{
	FTCFieldCheck Check;
	bool bArm = false;
	FString Feedback;      // the sentence flashed when refused (empty: none)
};

// One frame of the aim preview (client/main.js frame, targeting 'dig' or 'entrench').
struct FTCFieldPreview
{
	bool bShown = false;   // a field aim is armed
	// dig: an oblong marker centred on MarkerAt, MarkerLen along MarkerDir, MarkerWidth across, an arrow past its far
	// end (client/main.js aimMarker); bMarkerPlaced false: no centre and no ground yet (the mesh is at the origin)
	bool bMarker = false, bMarkerPlaced = false;
	FVector2D MarkerAt = FVector2D::ZeroVector;
	double MarkerDir = 0, MarkerLen = 0, MarkerWidth = 0;
	FColor MarkerColor;
	FTCFortPlacement Placement; // dig: what footAt says for the marker (when placed)
	// entrench: the cells to paint, each piece in its colour
	TArray<FTCEntrenchPiece> Pieces;
	FString Hint;          // the hint line after this frame
	// the browser's frame would throw computing this (see the file comment): the mesh and hint keep their last state
	bool bThrows = false;
};

// What a click does with a field aim armed (client/main.js mousedown), or a right-click join (client/orders.js).
struct FTCFieldClick
{
	bool bThrows = false;  // the browser throws: nothing happens at all
	bool bPinnedCenter = false; // the first click of a dig or entrench: the centre is pinned, nothing sent
	FString Feedback;      // the sentence flashed (empty: none)
	TSharedPtr<FJsonObject> Message; // the command, when sent
	FVector2D Marker = FVector2D(-1, -1); // where the order's marker goes, when sent
	FColor MarkerColor = FColor(0xc8, 0xa0, 0x60); // client/main.js and orders.js: 0xc8a060 for digs and entrenchments
	int32 Units = 0;       // ids in the message
	FString What;          // a short log line: what was sent
};

// A squad's dig on its row (client/hud.js tags, client/main.js work bar).
struct FTCDigStatus
{
	bool bDigging = false;     // flag 16: the squad has a dig
	bool bWaitingForMp = false;// flag 1024 without 16: on a pattern, waiting for its turn or for manpower
	float Progress = 0.f;      // the work bar while digging: the row's tail value, 0..1 (0 when the row has none)
	FString Tag;               // "Digging", "Waiting for MP to dig" or empty
};

class FTCFieldworks
{
public:
	// The reference commit FORTS, the menus and the texts here come from.
	static const TCHAR* const ReferencePin;

	// ---------- the tables ----------
	static const TArray<FTCFortDef>& Forts();          // FORTS, in its order
	static const FTCFortDef* Find(const FString& Kind);
	static bool LineFort(const FString& Kind);          // shared/sim.js lineFort
	static TArray<FString> FortBuilders();              // CFG.fortBuilders
	static const TArray<FTCEntrenchDef>& Entrenchments(); // ENTRENCH_TYPES with names and tips
	static const FTCEntrenchDef* FindEntrenchment(const FString& Kind);
	// client/hud.js BUILD_TABS in order ("base" Base, "defense" Defenses, "field" Field works, "eng" Engineering)
	static const TArray<TPair<FString, FString>>& BuildTabs();
	// shared/sim.js segmentCost(kind, cells); 0 for an unknown kind
	static int32 SegmentCost(const FString& Kind, int32 Cells);
	// shared/sim.js entrenchPlan(pattern, a, b, back, fort): the segments, middle first (strongpoint: trenches, then
	// wire). Empty for an unknown pattern or a fort that is not a FORTS kind with cells in a line (the browser never
	// asks for one; the server refuses it).
	static TArray<FTCFortSegment> EntrenchPlan(const FString& Pattern, const FVector2D& A, const FVector2D& B, const FVector2D* Back, const FString& Fort);
	// The fort or entrench action client/keys.js binds to this chord in its 'global' context ("fort:trench",
	// "entrench:line"), or empty. KeyName is the key's name as EKeys names it ("T"). Overlay contexts win a chord they
	// share (recruit mode and a selected building's card letters take T and Shift+T): the caller decides that first.
	static FString ActionForChord(const FString& KeyName, bool bShift, bool bCtrl, bool bAlt);
	// client/hud.js tags and client/main.js work bar for one held unit row.
	static FTCDigStatus DigStatus(const FTCUnit& U);

	// FTCUnitTable::Reference after it loads: every check stays refused until it equals ReferencePin.
	void SetUnitTableReference(const FString& Reference);
	bool ReferenceMatches() const { return bReferenceMatches; }

	// Message hooks, called after FTCMatchState's own (it must have accepted the start and applied the snapshot): a start
	// keeps the map's layers and starting terrain and forgets the last snapshot; a snapshot before any start is ignored.
	// Clear when the match ends.
	void ApplyStart(const FJsonObject& M);
	void ApplySnapshot(const FJsonObject& M);
	void Clear();
	bool HasSnapshot() const { return bHasSnapshot; }

	// ---------- availability and menus ----------
	// client/availability.js availability({ t: 'dig', kind, queue }) for the selection.
	FTCFieldCheck CheckDig(const FTCMatchState& State, const FString& Kind, bool bQueue, const TSet<int32>& Selected) const;
	// client/availability.js availability({ t: 'entrench' }) for the selection.
	FTCFieldCheck CheckEntrench(const FTCMatchState& State, const TSet<int32>& Selected) const;
	// client/hud.js drawOrders: the selection gets the Build and Trench menu buttons (something selected, not all
	// buildings, not all trucks, a builder squad of any owner among the held rows).
	bool OffersFieldMenus(const TSet<int32>& Selected) const;
	// client/hud.js menuHTML('build'): the fortification buttons, each with its tab. FTCConstruction::BuildMenu gives the
	// building buttons of the same menu; the browser shows the BuildTabs that have a button, the chosen one or the first.
	FTCFieldMenu FortMenu(const FTCMatchState& State, const TSet<int32>& Selected) const;
	// client/hud.js menuHTML('trench'): the six patterns.
	FTCFieldMenu TrenchMenu(const FTCMatchState& State, const TSet<int32>& Selected) const;

	// ---------- aims ----------
	// client/main.js startDig(kind) (a Build menu fort button or a fort key): a line fort other than the trench arms an
	// entrench 'line' made of it instead. On success Aim is armed (the caller makes it its one aim).
	FTCFieldArm StartDig(const FTCMatchState& State, const FString& Kind, const TSet<int32>& Selected, FTCFieldAim& Aim) const;
	// client/main.js startEntrench(pattern, fort) (a Trench menu button: fort 'trench'; Shift+T: 'line').
	FTCFieldArm StartEntrench(const FTCMatchState& State, const FString& Pattern, const FString& Fort, const TSet<int32>& Selected, FTCFieldAim& Aim) const;
	// client/main.js frame with a field aim armed: the marker or cells under the pointer, and the hint. Updates Aim's
	// mesh state and hint as the browser's frame does.
	FTCFieldPreview Preview(const FTCMatchState& State, FTCFieldAim& Aim, bool bHasGround, const FVector2D& Ground, const TSet<int32>& Selected, const FTCPositionOf& PositionOf = FTCPositionOf()) const;
	// client/main.js mousedown (left button) with a field aim armed. bHasGround false is a click off the ground. Shift
	// queues (a dig keeps its aim armed; an entrench is always spent). Updates Aim (pinned centre, re-armed or cancelled).
	FTCFieldClick Click(const FTCMatchState& State, FTCFieldAim& Aim, bool bHasGround, const FVector2D& Ground, bool bShift, const TSet<int32>& Selected, const FTCPositionOf& PositionOf = FTCPositionOf()) const;

	// ---------- placement ----------
	// client/main.js footAt(kind, at, dir) for a FORTS kind: placementCheck with this side's team and sight.
	FTCFortPlacement FortPlacement(const FTCMatchState& State, const FString& Kind, const FVector2D& At, double Dir) const;
	// client/main.js entrenchSegments(a, b): the pattern's buildable segments with their cells and prices (empty without
	// diggers or a view). bThrows is set when the browser would throw computing it.
	TArray<FTCEntrenchPiece> EntrenchPieces(const FTCMatchState& State, const FString& Pattern, const FString& Fort, const FVector2D& A, const FVector2D& B, const TSet<int32>& Selected, const FTCPositionOf& PositionOf, bool& bOutThrows) const;
	// client/main.js entrenchSummary(segs): the hint's first part.
	FString EntrenchSummary(const FString& Pattern, const FString& Fort, const TArray<FTCEntrenchPiece>& Pieces, int32 Diggers) const;
	// client/main.js diggers(): the selected held rows of a fort builder type that are not retreating and are drawn.
	TArray<int32> Diggers(const TSet<int32>& Selected, const FTCPositionOf& PositionOf = FTCPositionOf()) const;

	// ---------- works ----------
	const TArray<FTCWork>& Works() const { return WorkRows; }
	// client/main.js drawWorks: the works that can still be placed (no sight asked, no team), with their cells.
	TArray<FTCWorkGhost> WorkGhosts(const FTCMatchState& State) const;
	// client/main.js worksAt(ground): the planned pattern nearest the ground within 5 m that still hands out segments.
	bool WorksAt(const FVector2D& Ground, int32& OutProject) const;
	// client/orders.js kind 'entrench' and its command: own selected non-structure units that include a fort builder,
	// right-clicking ground with a planned pattern under it. The browser checks this after a supply run (all trucks), an
	// escort (all planes), boarding an own carrier and assisting a building, and before attack, house and move: call it
	// in that place. No message (Units 0) when it does not apply.
	FTCFieldClick PlanJoin(const FTCMatchState& State, const TSet<int32>& Selected, bool bHasGround, const FVector2D& Ground, bool bShift) const;

private:
	// A held unit row as the browser keeps it (unsmoothed, JSON numbers, NaN where missing).
	struct FRow
	{
		FString Type;
		double Owner = 0, X = 0, Z = 0, Hp = 0, Built = 1;
		int32 Flags = 0;
	};
	struct FTeam { bool bNull = false; double V = 0; };
	struct FSmoke { double X = 0, Z = 0, R = 0; };

	bool bReferenceMatches = false;
	FString UnitTableReference;

	// the start
	bool bStarted = false;
	int32 W = 0, H = 0;
	bool bMapWorld = false;
	TArray<FTeam> Teams;
	// the browser's terrain.groundGrid and objectGrid: 0 undefined or null, 1 the empty string, 255 any other value that
	// is not one ASCII character, else the character
	TArray<uint8> Ground, Objects;
	// rememberPlacementTerrain: the starting characters ('?' for a World cell not yet seen) and levels (NaN where the
	// browser's levelOf gives NaN)
	TArray<uint8> InitialChars;
	TArray<double> InitialLevels;

	// the last snapshot
	bool bHasSnapshot = false;
	double SnapMp = 0;                // NaN when the snapshot has none
	FString ModeKind;
	bool bTech = false; double TechTier = 0;
	bool bSnapWorld = false;          // the snapshot has world (the view's g.world)
	TArray<bool> OutSeats;
	TArray<FSmoke> Smokes;
	TMap<int32, FRow> Rows;
	TArray<int32> RowOrder;
	TArray<FTCWork> WorkRows;

	void ApplyCells(const TArray<TSharedPtr<FJsonValue>>& Cells);
	FTeam TeamOfSeat(double Seat, bool& bOutExists) const;
	static bool TeamsEqual(const FTeam& A, const FTeam& B);
	FTeam MyTeam(const FTCMatchState& State) const;
	// s.mode?.kind is classic or world (case-sensitive, as the reference compares)
	bool SnapshotClassic() const { return ModeKind.Equals(TEXT("classic"), ESearchCase::CaseSensitive) || ModeKind.Equals(TEXT("world"), ESearchCase::CaseSensitive); }
	bool Observing(const FTCMatchState& State) const;
	FTCFieldCheck Gate(const FTCMatchState& State) const;
	// the placement view exists: false with a native reason in OutWhy (or empty: the browser's own "no view")
	bool ViewReady(const FTCMatchState& State, FString& OutWhy) const;
	FTCFortPlacement Place(const FTCMatchState& State, const FString& Kind, const FVector2D& At, double Dir, bool bTeam, bool bSight) const;
	bool PositionFor(int32 Id, const FTCPositionOf& PositionOf, FVector2D& Out) const;
	bool NearestDigger(const FVector2D& At, const TSet<int32>& Selected, const FTCPositionOf& PositionOf, FVector2D& OutPos, int32& OutId) const;
	double DefaultDigDir(const FString& Kind, const FVector2D& At, const TSet<int32>& Selected, const FTCPositionOf& PositionOf) const;
	FString EntrenchName(const FString& Pattern, const FString& Fort) const;

	friend struct TCField::FView;
};
