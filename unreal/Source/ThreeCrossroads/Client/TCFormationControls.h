// The browser's Formation menu, facing gesture and layout controls (reference e4e579a):
//   - client/main.js fm { shape, spread, together, snap }, formation(), snapToTrench(), faceOrder(), reform(),
//     setFormation(), cycleFormation, groupForm (a copy saved with Ctrl+N, restored by recalling N) and faced (the last
//     facing each unit was ordered to hold);
//   - client/main.js beginFacing, updateFacing, showFacing, clearFacing and the mouseup that dispatches a facing order;
//   - client/orders.js dispatch's 'move' branch with options { face, reach, attack } and the together flag;
//   - client/formation-preview.js show (slot boxes, rank ribbons, facing chevrons and the drag arrow);
//   - client/hud.js menuHTML('form') and its button states; client/keys.js formation (Shift+V), tighten ([), spread (]).
//
// Formation spots come from the Rust core (FWW2Core::FormationSpots, ww2_formation_spots, ABI v1) with the chosen
// shape and spread, and slot sizes from TCFormation::Slot (ww2_formation_slot_size). When the core is not ready, or
// answers outside its documented contract, a C++ port of shared/formation.js facingSpots in TCFormationControls.cpp
// answers instead (every shape and spread, V8's Array.prototype.sort, Math.round and Math.max semantics), with a
// warning. The client's own math (the plain click's facing angle, the port's cos and sin, the gesture's angle) is the
// platform libm, not V8's fdlibm: it can differ from the browser in the last bit (see the .cpp).
//
// Pure client state: nothing here decides an outcome. The server validates every message (shared/sim.js command).
// Coordinates are reference metres (x east, z south); screen positions are viewport pixels.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCFormation.h"
#include "Client/TCOrders.h"
#include "Client/TCControlGroups.h"

class FJsonObject;
class FJsonValue;
class FTCMatchState;
class FTCUnitTable;

// shared/formation.js SHAPES, in the menu's order. The values are WW2_SHAPE_* (FWW2Core::Shape*).
enum class ETCFormationShape : uint8
{
	Line = 0,
	Block = 1,
	Column = 2,
	Wedge = 3,
};
// "line", "block", "column", "wedge" (the reference's names, as fm.shape holds them)
const TCHAR* LexToString(ETCFormationShape Shape);

// client/main.js fm. The defaults are the browser's.
struct FTCFormationSettings
{
	ETCFormationShape Shape = ETCFormationShape::Line;
	double Spread = 1.0;    // 1 to 2.5 in quarter steps (reform)
	bool bTogether = true;  // move and amove carry together: true (march at the slowest unit's pace)
	bool bSnap = true;      // infantry spots step into a trench cell within 3 m
	bool operator==(const FTCFormationSettings& O) const { return Shape == O.Shape && Spread == O.Spread && bTogether == O.bTogether && bSnap == O.bSnap; }
	bool operator!=(const FTCFormationSettings& O) const { return !(*this == O); }
};

// One control of the menu or its keys. Key() gives the reference's data-f value ("shape:line", "tighten", "spread",
// "together", "snap"), plus "menu" for the Form button (data-m="form") and "cycle" for Shift+V.
enum class ETCFormationControl : uint8
{
	None,
	ShapeLine,
	ShapeBlock,
	ShapeColumn,
	ShapeWedge,
	Tighten,   // [ : spread - 0.25, then re-form the selection where it stands
	Spread,    // ] : spread + 0.25, then re-form
	Together,  // toggle fm.together
	Snap,      // toggle fm.snap
	Menu,      // open or close the Formation submenu of the Orders panel
	Cycle,     // Shift+V: the next shape
};

// A formation move's facing (client/orders.js options.face and options.reach): set by a right-drag (or an attack-move
// drag), unset for a plain click, which faces the way the units travel.
struct FTCMoveFacing
{
	bool bSet = false;
	double Face = 0;   // radians; 0 faces +x, pi/2 faces +z (the drag's atan2, sent as the message's face)
	double Reach = 0;  // the drag's ground length in metres
};

// A unit to place, read from the snapshot and the unit table.
struct FTCFormationTroop
{
	int32 Id = 0;
	double X = 0, Z = 0;
	double Size = 3;      // slotSize (TCFormation::Slot)
	double Radius = 1;    // UNITS[type].radius (the preview's slot box)
	bool bBack = false;   // client/main.js rearRank
	bool bInfantry = false;
	bool bAir = false;
	bool bTruck = false;  // type 'truck'
};

// Which selected units a reference function takes.
enum class ETCTroopFilter : uint8
{
	Orders,  // client/orders.js selection troops and showFacing: own and not a structure (aircraft and trains included)
	Mine,    // client/main.js myTroops: own, not a structure, not an aircraft (faced, reform)
};

// The mouse button that started a facing gesture: the right button for a move, the left one for an armed attack-move.
enum class ETCGestureButton : uint8 { Left, Right };

// client/main.js facingGesture.
struct FTCFacingGesture
{
	bool bActive = false;
	ETCGestureButton Button = ETCGestureButton::Right;
	FTCCursor Cursor;                 // the press's cursor (an attack-move drag: its ground only)
	FVector2D PressPixel = FVector2D::ZeroVector;
	bool bAttack = false;             // started by the armed attack-move
	bool bPressShift = false, bPressCtrl = false; // event: the modifiers at the press
	bool bCtrl = false;               // f.ctrl: Ctrl at the last mouse move (the preview's colour)
	bool bFacing = false;             // f.face is set
	double Face = 0, Reach = 0;
};

// What a gesture's release asks the controller to dispatch (client/main.js mouseup then orders.dispatch).
struct FTCFacingRelease
{
	FTCCursor Cursor;      // dispatch this press cursor (not the release point)
	FTCMoveFacing Facing;  // set when the drag gave a facing: the move branch uses it
	bool bAttack = false;  // the move branch attack-moves: options.attack || event.ctrlKey (IssueOrder's bAttack)
	bool bQueue = false;   // event.shiftKey
	bool bCancelAim = false; // the gesture was the armed attack-move: disarm it (cancelAim) before dispatching
};

// client/formation-preview.js show, ready for the HUD to draw over the ground (metres).
struct FTCFormationPreview
{
	struct FArrow { FVector2D Tip, Left, Right; }; // a filled arrowhead (overlay.js arrow)
	bool bVisible = false;
	TArray<FTCSpot> Spots;                         // what the release would send now (before the server)
	double Face = 0, Reach = 0;
	FVector2D At = FVector2D::ZeroVector;
	FColor Color = FColor(0x5f, 0xa4, 0xf2);       // MOVE_COLOR, or PLAN_COLORS[2] (0xf0932f) for an attack-move
	float Opacity = 0.9f;
	float LineWidth = 0.24f;                       // ribbon width in metres (the browser's w)
	TArray<TPair<FVector2D, FVector2D>> Lines;     // rank ribbons, slot boxes and the drag line
	TArray<FArrow> Arrows;                         // a chevron per slot and the drag's arrowhead
};

// One button of the Formation submenu or the Form menu button (client/hud.js orderBtn / menuBtn).
struct FTCFormationButton
{
	ETCFormationControl Control = ETCFormationControl::None;
	FString Key;    // data-f / data-m value (Key())
	FString Icon;   // the browser's icon name (f_line, f_tight, ...)
	FString Name;   // aria-label: the tip up to its first ':' or '('
	FString Kbd;    // the hotkey badge ("[", "]"), empty when none
	FString Value;  // the .val text: "On"/"Off", "1.25x", "Form", or empty
	bool bOn = false; // the 'on' class
	FString Tip;    // the title, word for word
};

// What a control did, for the HUD note, a blip and the socket.
struct FTCFormationResult
{
	bool bDone = false;            // the control is one this class runs (None is not)
	FString Feedback;              // feedback.show text ("Formation: wedge", "Formation spacing 1.25x"), or empty
	bool bBlip = false;            // the browser plays blip(700)
	TSharedPtr<FJsonObject> Message; // reform: { t: 'move', orders, face } to send, else null
	int32 Units = 0;               // ids in Message
	FString Why;                   // when Message is null after a reform: why (fewer than two own troops)
};

class FTCFormationControls
{
public:
	// client/main.js beginFacing: the press must travel this far (pixels) before the drag can set a facing.
	static constexpr double FacingPixels = 12.0;
	// updateFacing: a drag shorter than this on the ground (metres) sets no facing.
	static constexpr double FacingMetres = 2.0;
	// reform: the spacing's range and step
	static constexpr double SpreadMin = 1.0, SpreadMax = 2.5, SpreadStep = 0.25;

	FTCFormationSettings Settings;
	bool bMenuOpen = false;

	static const TCHAR* Key(ETCFormationControl Control);
	static ETCFormationControl ParseKey(const FString& Key);
	static ETCFormationControl ShapeControl(ETCFormationShape Shape);

	// ---- troops and spots ----

	// The selected units Filter takes, in the selection's iteration order (the browser's [...selected] order).
	static TArray<FTCFormationTroop> Troops(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, ETCTroopFilter Filter);
	// shared/formation.js facingSpots through the Rust core, else the C++ port. Source says which answered.
	static TArray<FTCSpot> FacingSpots(const TArray<FTCFormationItem>& Items, double AtX, double AtZ, double Face, double Reach,
		ETCFormationShape Shape, double Spread, ETCFormationSource* Source = nullptr);
	// The C++ port alone (verification and the fallback). Dx, Dz: cos and sin of the facing.
	static TArray<FTCSpot> PortSpots(const TArray<FTCFormationItem>& Items, double AtX, double AtZ, double Dx, double Dz, double Reach,
		ETCFormationShape Shape, double Spread);
	// client/main.js formation(sel, g, face, reach) with these settings: no facing faces from the troops' centre to G
	// with no reach; then facingSpots, snapToTrench when bSnap, and every spot kept in [1, size - 1] of the map.
	TArray<FTCSpot> Formation(const FTCMatchState& State, const TArray<FTCFormationTroop>& Troops, FVector2D G, const FTCMoveFacing& Facing,
		ETCFormationSource* Source = nullptr) const;
	// The same as wire rows [id, x, z] (a non-finite coordinate becomes null, as JSON.stringify writes NaN).
	TArray<TSharedPtr<FJsonValue>> Rows(const FTCMatchState& State, const TArray<FTCFormationTroop>& Troops, FVector2D G, const FTCMoveFacing& Facing) const;

	// ---- orders ----

	// client/orders.js kind() === 'move' for what this class can see: own troops exist, and the click is neither a
	// supply run (all trucks over a friend or building), an escort (all planes over a friend), boarding (an own carrier
	// with infantry selected), nor an attack (an enemy under it). bOtherKind: the controller's own planners claim the
	// click first (assist or repair, joining planned works, a house), which this class cannot judge.
	static bool WouldMove(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bOtherKind);
	// client/orders.js dispatch, order 'move': { t: 'move'|'amove', orders, face?, together? } for the marching troops,
	// and a plain move for the trucks of an attack-move, then the queue flag (one command, or { t: 'orders', queue,
	// commands }). bAttack: options.attack || event.ctrlKey; bQueue: event.shiftKey. Kind None when nothing to order.
	FTCOrderPlan PlanMove(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, FVector2D Ground,
		bool bAttack, bool bQueue, const FTCMoveFacing& Facing) const;

	// ---- the menu and its keys ----

	// Runs a control. Tighten and Spread change the spacing and re-form the own non-air selection where it stands,
	// facing as last ordered, else UpScreenFace (the browser's ground direction from the screen centre 100 px up the
	// screen), else 0: the result carries the message to send. The rest only change settings or the menu.
	FTCFormationResult Run(ETCFormationControl Control, const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected,
		TOptional<double> UpScreenFace);
	// The Form button (shown with the Orders panel) and, when bMenuOpen, the submenu's eight buttons.
	FTCFormationButton MenuButton() const;
	TArray<FTCFormationButton> SubmenuButtons() const;
	// The browser's up-the-screen facing from two ground points: G0 under the screen centre, G1 100 px above it.
	static double UpScreenFace(FVector2D G0, FVector2D G1);

	// ---- control groups ----

	// client/main.js group actions: Set saves a copy of the settings for Group, Recall restores a saved copy. Called
	// after every group chord, whether or not it changed the group (the browser does it unconditionally).
	void OnGroupChord(ETCGroupOp Op, int32 Group);

	// ---- facing gesture ----

	// beginFacing: replaces any gesture. Call it instead of dispatching when the right press WouldMove (Cursor from the
	// press), or with the left press of an armed attack-move (a cursor holding only its ground, bAttack true).
	// False (no gesture) when Cursor has no ground.
	bool BeginGesture(ETCGestureButton Button, FVector2D PressPixel, const FTCCursor& Cursor, bool bAttack, bool bShift, bool bCtrl);
	// mousemove and frame: bMouseMoved with bButtonHeld false cancels (the release never came); bMouseMoved also takes
	// bCtrl as f.ctrl. Then updateFacing at Pixel with the ground under it (bHasGround false: none).
	void UpdateGesture(FVector2D Pixel, bool bHasGround, FVector2D Ground, bool bMouseMoved, bool bCtrl, bool bButtonHeld);
	// mouseup of the gesture's button: a last updateFacing, the gesture ends, Out says what to dispatch, and when the
	// drag set a facing every own non-air selected unit remembers it (faced). False when no gesture of that button.
	bool ReleaseGesture(ETCGestureButton Button, FVector2D Pixel, bool bHasGround, FVector2D Ground, bool bShift, bool bCtrl,
		const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, FTCFacingRelease& Out);
	// clearFacing: Esc, a new press in the view, the window losing focus or the pointer leaving it.
	void CancelGesture() { Gesture = FTCFacingGesture(); }
	bool IsGesturing() const { return Gesture.bActive; }
	const FTCFacingGesture& CurrentGesture() const { return Gesture; }
	// showFacing, once a frame while gesturing: hidden until the drag sets a facing; a gesture whose selection has no
	// own troops left is cancelled.
	FTCFormationPreview Preview(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected);

	// faced: the facing a unit was last ordered to hold through this client (empty when none).
	TOptional<double> FacedOf(int32 Id) const;

private:
	FTCFacingGesture Gesture;
	TMap<int32, FTCFormationSettings> GroupForm;
	TMap<int32, double> Faced;
	void Remember(const TArray<FTCFormationTroop>& Troops, double Face);
};
