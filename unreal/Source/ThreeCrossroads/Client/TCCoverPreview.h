// Cover preview: with own infantry selected, shields mark the explored ground around the cursor (green heavy, yellow
// light, red open) and small yellow chevrons point toward cover from direct fire on that side; a ground move order
// flashes the same marks at its destination. A port of the pinned reference (FTCCoverPreview::ReferencePin):
//   client/cover-preview.js  NONE/OPEN/LIGHT/HEAVY, solid, wall, walkable, houseEdge, cellCover, solidToward,
//                            vehicleCovers, vehicleToward, COLORS, SIZE, ALPHA, LIFT, CURSOR_OPACITY, XRAY, FLASH, RADIUS,
//                            ENEMY_RANGE, MODELLED, PER, VMAX, the glyph templates, scanUnit/scan, scanSel/selection,
//                            fill, frame, flash, start, build (graphics change) and the enabled switch;
//   shared/sim.js            CELL, TERRAIN and its MOVE/COVER/TRENCH bits, UNITS (infantry, air, garrisons, w, w.salvo);
//   client/main.js           the preview's context: terrain.grid, terrain.state, lastSnap.wrecks, fogOfWar.explored,
//                            the smoothed unit positions, me and foe, and the move feedback that calls flash.
// The rules mirror shared/sim.js coverMul and coverBehind as the browser can judge them; the server decides real cover.
// Nothing here is authoritative and nothing asks the server or the Rust core.
//
// What this client knows, and so all the preview reads (recipient-known data only):
// - Terrain: the server's terrain characters (FTCMatchState::Map.Chars, '0' for undiscovered World cells) and the
//   per-cell state the server sends with each changed cell (its 4th field, damage stage in bits 3-4). FTCMatchState does
//   not keep that state yet: pass it in FTCCoverSources::CellState when it does. Without it every cell counts as whole,
//   which is exactly the reference's starting state (shared/sim.js startState has no damage bits) and stays right until
//   a wall, hedge or house is shot up.
// - Scouting: only cells this side has explored get marks (fogOfWar.explored, FTCMatchState::FogExplored read raw: a
//   lifted fog has no explored cells in the browser either, so a spectator's lifted fog marks nothing).
// - Units: only the rows this client holds (the server sends what this side sees), at the positions the caller draws
//   them (the browser uses its smoothed positions). Wrecks come from the snapshot's wrecks list; FTCMatchState does not
//   decode it yet, so pass them in FTCCoverSources::Wrecks when it does (without them wrecks give no cover marks).
// - Unit types: the per-type facts the preview needs (infantry, air, garrisons, a direct-fire weapon) are not in
//   FTCUnitTable/units.json. ReferenceUnitFacts holds them, generated from shared/sim.js UNITS at ReferencePin by a
//   script and checked against it (see the change notes). A type it does not name is skipped, as the browser skips a
//   type UNITS does not have.
//
// Exact knowledge limits and differences from the browser:
// - Unit order. The browser walks its units Map in insertion order: that order picks the nearest shooter on an exact
//   distance tie, the first 48 hulls near the spot, and the two hull chevrons drawn on an open cell with no shooter.
//   This client does not know that order and walks units by ascending id (snapshot rows arrive in id order, so it is
//   the browser's order unless units left view and came back).
// - Caching. The browser rewrites the cursor marks only when a hash of their inputs changes (a missed change, such as a
//   team change, or a hash collision keeps stale marks there). This port recomputes them on every frame they show and
//   bumps Revision only when the result differs, so it shows what the browser shows after its next rewrite.
// - Scouting merge. The browser ORs the whole explored array into its own copy whenever the fog version moves; this
//   port ORs the cells a fill reads. The two agree while explored only grows within a match (FTCMatchState: it is
//   reset only by a start, and Start() here must follow every start).
// - Maths. Math.atan2, Math.cos, Math.sin and Math.hypot are V8's; this port uses the C library's. They can differ in
//   the last bit, which only matters for a point exactly on a threshold.
// - The browser's DOM: the menu button and localStorage ('ww2-cover', on unless '0') are the caller's. MenuLabel and
//   MenuTip give the texts; SetEnabled is the switch.
// Each glyph carries place, angle, size, colour, alpha and its outline. TCMarkers draws the ground layer with scene
// depth testing and the faint through-models layer without it, using the same overlay.js ribbon profile.
#pragma once

#include "CoreMinimal.h"

class FTCMatchState;

// client/cover-preview.js NONE, OPEN, LIGHT, HEAVY.
enum class ETCCoverKind : uint8
{
	None = 0,
	Open = 1,
	Light = 2,
	Heavy = 3,
};

// What the preview needs to know about a unit type (shared/sim.js UNITS): infantry, air, def.garrisons, and a weapon
// that shoots straight (def.w present and not a salvo).
struct FTCCoverUnitFacts
{
	bool bInfantry = false, bAir = false, bGarrisons = false, bDirectFire = false;
};

// One wreck from the snapshot (shared/sim.js snapshot wrecks rows [id, type, owner, x, z, rot]).
struct FTCCoverWreck
{
	int32 Id = 0;
	double X = 0, Z = 0;
};

// The map as this client knows it: terrain characters and, optionally, the per-cell server state. Views into the
// caller's arrays; they must outlive the calls that read them.
struct FTCCoverGrid
{
	int32 W = 0, H = 0;
	TArrayView<const uint8> Chars;   // W * H terrain characters (shared/sim.js TERRAIN keys)
	TArrayView<const uint8> State;   // W * H server cell states, or empty: every cell whole (stage 0)

	bool IsValid() const { return W > 0 && H > 0 && Chars.Num() == W * H; }
	// client/cover-preview.js at(x, y): the terrain character, 0 off the map (the browser's '').
	uint8 At(int32 X, int32 Y) const { return X < 0 || Y < 0 || X >= W || Y >= H ? 0 : Chars[Y * W + X]; }
	// client/cover-preview.js stage(x, y): 0 whole, 1 damaged, 2 nearly gone (bits 3-4 of the cell's state).
	int32 Stage(int32 X, int32 Y) const
	{
		const int32 C = Y * W + X;
		return State.IsValidIndex(C) ? (State[C] >> 3) & 3 : 0;
	}
};

// One unit as the preview reads it, already judged against this client's seat and the type facts.
struct FTCCoverUnit
{
	int32 Id = 0;
	double X = 0, Z = 0;
	bool bHull = false;     // a known, live (hp > 0), ground (not air) non-infantry unit: it can cover a squad
	bool bShooter = false;  // a known, live, ground unit with a direct-fire weapon, owned by a foe
};

// Everything one frame or flash reads (client/cover-preview.js ctx): the grid, explored cells, the units and wrecks,
// and what the selection is.
struct FTCCoverKnown
{
	FTCCoverGrid Grid;
	TArrayView<const uint8> Explored;  // W * H, nonzero for cells this side has explored
	TArray<FTCCoverUnit> Units;        // by ascending id
	TArray<FTCCoverWreck> Wrecks;      // in snapshot order
	bool bInfantrySelected = false;    // an own infantry unit is selected (scanSel)
	bool bGarrisonSelected = false;    // and one of them can garrison a house
};

// Where the caller draws a unit (client/main.js units.get(id).x, .z, smoothed): false when it draws no such unit.
using FTCCoverPositionOf = TFunction<bool(int32 Id, FVector2D& Out)>;
// The facts of a unit type: false for a type the caller does not know.
using FTCCoverFactsOf = TFunction<bool(const FString& Type, FTCCoverUnitFacts& Out)>;

// What the caller holds beyond FTCMatchState (each optional, see the file comment).
struct FTCCoverSources
{
	TArrayView<const uint8> CellState;  // W * H server cell states (cells entries' 4th field), empty: all whole
	TArrayView<const FTCCoverWreck> Wrecks;
	FTCCoverPositionOf PositionOf;      // unset: the held snapshot positions
	FTCCoverFactsOf FactsOf;            // unset: ReferenceUnitFacts
};

// One mark: a shield over a cell or a chevron toward cover.
struct FTCCoverGlyph
{
	bool bChevron = false;
	ETCCoverKind Kind = ETCCoverKind::Open;  // the shield's kind; a chevron always has the Light colour
	double X = 0, Z = 0;    // world metres (x east, z south), on the ground (draw LiftMetres above it)
	double Angle = 0;       // radians: a template point (Lx, Lz) lands at Place(Lx, Lz)
	double Size = 1;        // template scale
	FColor Color = FColor(0, 0, 0, 0); // sRGB (client/cover-preview.js COLORS; three.js turns it linear)
	float Alpha = 1.f;      // ALPHA[kind] times the fade toward the edge of the radius

	// client/cover-preview.js glyph: where a template point goes in the world (x, z).
	FVector2D Place(double Lx, double Lz) const
	{
		const double Ca = FMath::Cos(Angle), Sa = FMath::Sin(Angle);
		return FVector2D(X + Size * (Ca * Lz - Sa * Lx), Z + Size * (Sa * Lz + Ca * Lx));
	}
	bool operator==(const FTCCoverGlyph& O) const
	{
		return bChevron == O.bChevron && Kind == O.Kind && X == O.X && Z == O.Z && Angle == O.Angle && Size == O.Size && Color == O.Color && Alpha == O.Alpha;
	}
};

// The marks around one spot, in the reference's two layers: on the ground (depth tested) and through the models of
// houses, hedges, walls, traps, woods and rubble (no depth test, drawn at XrayOpacity of the layer's opacity).
struct FTCCoverMarks
{
	TArray<FTCCoverGlyph> Ground;
	TArray<FTCCoverGlyph> Through;
	int32 Revision = 0;   // moves whenever the marks change
	void Reset() { Ground.Reset(); Through.Reset(); }
};

// How to draw the two regions this frame (client/cover-preview.js frame: mats[0..3] visible and opacity).
struct FTCCoverView
{
	bool bVisible = false;                  // anything at all (mesh.visible)
	bool bCursorGround = false, bCursorThrough = false, bFlashGround = false, bFlashThrough = false;
	float CursorGroundOpacity = 0.f, CursorThroughOpacity = 0.f, FlashGroundOpacity = 0.f, FlashThroughOpacity = 0.f;
};

// The pointer as the browser tracks it for the preview (client/main.js mouse and targeting, cover-preview.js buttons
// and over).
struct FTCCoverPointer
{
	bool bInside = false;      // mouse.inside: the pointer is in the window and the page is not hidden
	bool bOverView = false;    // over: the last pointer event was over the game view, not a panel over it
	bool bLeftDown = false;    // buttons & 1
	bool bAimAllows = true;    // no aim armed, or the attack-move aim (targeting null or 'amove')
	bool bGroundHit = false;   // groundAt(mouse) found ground
	FVector2D Ground = FVector2D::ZeroVector; // that ground, world x and z
};

class FTCCoverPreview
{
public:
	static const TCHAR* const ReferencePin;

	// client/cover-preview.js constants.
	static constexpr double CellMetres = 2.0;            // shared/sim.js CELL
	static constexpr double LiftMetres = 0.14;           // LIFT: marks float this high over the ground
	static constexpr double CursorOpacity = 0.85;        // CURSOR_OPACITY
	static constexpr double XrayOpacity = 0.7;           // XRAY: the through-models layer, relative to its region
	static constexpr double FlashSeconds = 1.6;          // FLASH
	static constexpr double RadiusHigh = 6.5, RadiusLow = 5.2; // RADIUS: metres around the spot
	static constexpr double EnemyRange = 75.0;           // ENEMY_RANGE
	static constexpr int32 GroundCap = 300, ThroughCap = 120; // PER: glyphs per layer and region
	static constexpr int32 HullCap = 48;                 // VMAX
	static constexpr double ChevronOffset = 0.62;        // a chevron sits this far from the cell centre
	static constexpr double LineWidth = 0.18;            // template outlines: ribbon width in metres
	static constexpr double LineMinPixels = 1.15;        // and at least this many pixels
	static constexpr double FillAlpha = 0.3;             // the shield's inside (the overlay material's fill)

	// ---------- rules (client/cover-preview.js) ----------
	// The reference's TERRAIN flags of a terrain character (MOVE 1, COVER 4, TRENCH 8); false for a character
	// TERRAIN does not have (off the map, undiscovered World cells).
	static bool TerrainFlags(uint8 Ch, int32& OutFlags);
	// houseEdge: a house cell with ground next to it a squad can step onto.
	static bool HouseEdge(const FTCCoverGrid& G, int32 X, int32 Y);
	// cellCover: the cover the ground itself gives a squad in cell (X, Y), from every side.
	static ETCCoverKind CellCover(const FTCCoverGrid& G, int32 X, int32 Y, bool bGarrisons = true);
	// solidToward: from (Px, Pz) toward a shooter along (Ca, Sa), does something solid cover (terrain half of
	// coverBehind, with the client's damage stage)?
	static bool SolidToward(const FTCCoverGrid& G, double Px, double Pz, double Ca, double Sa);
	// vehicleCovers: a hull this far away covers (above 0.4 inside 3.4 m).
	static bool VehicleCovers(double D) { return D > 0 && (4 - D) / 1.5 > 0.4; }
	// vehicleToward: one of these hulls (x, z) close enough and no more than about 45 degrees off the bearing.
	static bool VehicleToward(TArrayView<const FVector2D> Hulls, double Px, double Pz, double Ca, double Sa);
	// MODELLED: cells with a model (house, hedge, wall, trap, rubble, woods, wreck) get their shield through it.
	static bool Modelled(uint8 Ch);
	// shared/sim.js UNITS facts at ReferencePin; false for a type it does not have.
	static bool ReferenceUnitFacts(const FString& Type, FTCCoverUnitFacts& Out);
	// COLORS: green heavy, yellow light (and every chevron), red open.
	static FColor KindColor(ETCCoverKind Kind);
	// The glyph templates in template space (x, z pairs): the shield's closed outline (its inside filled at
	// FillAlpha) and the chevron's open polyline.
	static TArrayView<const FVector2D> ShieldOutline();
	static TArrayView<const FVector2D> ChevronLine();
	// The menu button (client/cover-preview.js label and title).
	static FString MenuLabel(bool bOn);
	static const TCHAR* MenuTip();

	// ---------- what the client knows ----------
	// Reads this client's match state and selection into Out (scan, scanSel): units by ascending id at the caller's
	// drawn positions, judged against this seat (FTCMatchState::IsOwn, IsFoe). Out's views point into State and
	// Sources: keep both alive and unchanged while Out is used.
	static void Gather(const FTCMatchState& State, const TSet<int32>& Selected, const FTCCoverSources& Sources, FTCCoverKnown& Out);

	// ---------- the preview ----------
	// A new match (after the match state took its start): scouting starts from what the server says this side has
	// explored, which after a rejoin includes everything seen before. Clears both regions and ends a flash.
	void Start(const FTCCoverKnown& Known);
	// One frame (Dt in seconds; the browser passes its frame time capped at 0.1 s). Rewrites the cursor marks when they
	// show and returns how to draw both regions. Nothing shows before Start, while switched off, or while Known's grid is
	// missing or not the size the match started with (the browser keeps the start's width and height).
	const FTCCoverView& Frame(double Dt, const FTCCoverKnown& Known, const FTCCoverPointer& Pointer);
	// A ground move order's destination: the same marks there for a moment (main.js feedback for an order without a
	// target unit).
	void Flash(double X, double Z, const FTCCoverKnown& Known);
	// The graphics settings changed (the reference rebuilds its buffers on any change): the radius follows bLow, both
	// regions are emptied and a running flash ends.
	void OnGraphicsChanged(bool bLow);
	bool IsEnabled() const { return bEnabled; }
	void SetEnabled(bool bOn) { bEnabled = bOn; }
	bool IsStarted() const { return bStarted; }

	const FTCCoverMarks& CursorMarks() const { return Cursor; }
	const FTCCoverMarks& FlashMarks() const { return Flashed; }
	// How to draw, as the last Frame left it. Draw a layer only when it is on and its marks are not empty: Start, Flash
	// and OnGraphicsChanged change the marks without a new view, as in the browser, whose next frame always comes first.
	const FTCCoverView& View() const { return LastView; }
	double Radius() const { return bLowGraphics ? RadiusLow : RadiusHigh; }

private:
	// scan's results for one spot
	struct FScan
	{
		const FTCCoverUnit* Enemy = nullptr;
		TArray<FVector2D, TInlineAllocator<HullCap>> Hulls;
	};
	void ScanAt(const FTCCoverKnown& Known, double X, double Z, FScan& Out) const;
	void Fill(const FTCCoverKnown& Known, const FScan& Scan, int32 Cx, int32 Cy, FTCCoverMarks& Into);
	bool Usable(const FTCCoverKnown& Known) const;

	bool bEnabled = true;
	bool bLowGraphics = false;
	bool bStarted = false;
	int32 W = 0, H = 0;
	TArray<uint8> Seen;          // cells explored since Start
	int32 CurX = -99, CurY = -99, FlX = -99, FlY = -99;
	bool bCurOk = false;
	double FlT = FlashSeconds;   // seconds since the last flash began
	FTCCoverMarks Cursor, Flashed;
	FTCCoverView LastView;
};
