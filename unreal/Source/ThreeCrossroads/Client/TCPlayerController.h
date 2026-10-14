// RTS input: pan (WASD, arrows, screen edges), zoom (wheel), rotate (Q/E), left click or drag to select your own units,
// right click to order them (client/orders.js dispatch: an all-truck selection over a friendly unit or building gets a
// supply run, otherwise attack the enemy under the pointer, else move in the browser's default formation; Ctrl+right
// click attack-moves; selected own Production Buildings get a rally to the ground in the same message), Shift queues an
// order. Keys are matched as client/keys.js matches them (the modifiers must be exactly the binding's): X stops, G arms
// an attack-move, R retreats, Shift+F, Shift+G and Shift+X switch hold fire, hold position and auto-retreat,
// Shift+H arms a rally, and in Classic and World J, K, L, O and Y arm a building. Slice 1e: F uses an ability, Shift+Q
// mounts up, Shift+E unloads, T, Shift+T and Shift+Y, U, I, O, J, K, L, M arm field works, and in Classic a selected
// finished Production Building's card letters (client/keys.js 'building' context) buy its cards first; the right click
// also boards an own carrier, assists or repairs a friendly building and joins planned works (client/orders.js kind).
// Slice 1f: Shift+V cycles the formation shape, [ and ] tighten and spread it (the selection re-forms where it stands),
// the Orders panel's Form menu sets shape, spacing, March together and Snap to trenches; a right press that would move
// starts the facing gesture (client/main.js beginFacing): a drag sets the facing and the width, shown as the formation
// preview, and the release dispatches the move. An armed attack-move's left press does the same. Z, C, V, B, N, U, P and
// I arm the support calls (the support bar's buttons too) and Shift+B the area fire of the selected salvo units.
// One aim is armed at a time (client/main.js targeting): attack-move, rally, a building, an aimed ability, a dig or
// entrenchment, or a support call or area fire; a left click spends it, a right click or Esc cancels it. Digits 1 to 9 are control groups (client/keys.js: N recalls, twice centres,
// Ctrl+N sets, Shift+N or Ctrl+Shift+N appends, Alt+N moves the selection's army into N). A press on the minimap goes
// to it first (left: point the camera there, drag to keep pointing; right: order the selection at that ground), then
// the HUD's buttons and panels; only a press on neither reaches the world.
//
// Gap 13 (client/keys.js, client/camera.js, client/selection.js): every key is matched through the keys.js table
// (TCKeys::Match with the context ranks: targeting over a building's card letters over the base table), and the table
// adds Delete (destroy, no refund), Shift+C (take cover), Shift+N (World Conquest: send by rail, a click aims it),
// Space (the newest alert through NewestAlertAt, else the selection's centre), Shift+Space (follow the selection, any
// camera pan ends it), H (home; in Classic it selects the HQ), Ctrl+A (the army), '.', Shift+'.' and ',' (idle units),
// Alt+H and Alt+Left/Right (the alert history hooks), F1 (the Controls sheet: while open it takes the keys and the
// world's presses; F1 or Esc closes it) and M and F2 (said to be missing here: no audio, no stats overlay). A left
// double-click selects the own units of the clicked type on the screen (Ctrl or Command: on the whole map). The camera
// glides in from the widest view at a new match (any key or press skips it; a skipping right press or wheel notch is
// dropped), the wheel zooms at the point under the cursor, a middle-drag turns the view, edge scrolling ramps over a
// 24 to 48 px zone and eases in (on and off and the pan speed are saved settings), the target's height glides over the
// ground, and a right click by an all-aircraft selection on a friendly unit sends an escort.
//
// Verification switches (ATCGameMode) reach the same functions the mouse and keys use (PressButton, PressCommand,
// MinimapPress, GroupChord, AimClick, BoxSelect) or, for keys, InjectKey, which feeds a key event into this controller's engine input so the
// real key polling runs. None of them is a physical mouse or keyboard, and the logs say which path ran.
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/PlayerController.h"
#include "Client/TCOrders.h"
#include "Client/TCControlGroups.h"
#include "Client/TCMinimap.h"
#include "Client/TCFieldworks.h"
#include "Client/TCFormationControls.h"
#include "Client/TCSupportPowers.h"
#include "Client/TCKeys.h"
#include "Client/TCCameraPawn.h"
#include "TCPlayerController.generated.h"

struct FTCHousePick;

struct FTCCoverKnown;

class ATCGameMode;
struct FTCAbilityPlan;
struct FTCSupportPlan;
struct FTCUnit;
struct FTCUnitDef;

// What a HUD button (or its verification switch) asks for. Each one sends a reference message, or arms an order.
enum class ETCUiAction : uint8
{
	None,
	AddAi,       // lobby, host: { t: 'addAi' }
	NextMode,    // lobby, host: { t: 'mode', v } cycling the modes this client draws (conquest, world)
	Start,       // lobby, host: { t: 'start' }
	End,         // match, host: { t: 'end' } (the room goes back to the lobby)
	Stop,        // match: { t: 'stop', ids } for the selection (the X key)
	AttackMove,  // match: arm an attack-move for the next left click (the G key)
	Recruit,     // match: a recruit card, { t: 'buy', unit, from? } (client/hud.js buy; Shift buys up to five)
	CancelJob,   // match, Classic and World: a waiting queue place, { t: 'cancelProduction', id, job }
	RecruitGroup,// match: open (or close) one group of recruit cards (client/hud.js group tabs)
	Retreat,     // match: { t: 'retreat', ids } for the selection (the R key)
	Stance,      // match: { t: 'stance', ids, key, on } for the own selected units (Unit: the stance key; Shift+F/G/X)
	Rally,       // match: arm the rally aim (Shift+H, the recruit card's Rally button)
	Build,       // match: arm the placement of a building (Unit: its kind; the Build menu, the Build card, Classic keys)
	BuildMenu,   // match: open or close the Orders panel's Build menu (client/hud.js menuBtn 'build')
	Resume,      // match: { t: 'logisticsResume', ids } for the selected own trucks (Resume deliveries)
	SupplyOverlay, // match: the logistics overlay on or off (client/hud.js toolbar)
	TruckSelect, // match: the logistics drag selection on or off (a drag then takes only own trucks)
	Ability,     // match: an ability button (Unit: the unit type): client/main.js useAbility, instant or aimed
	Autocast,    // match: the right click on an ability button (Unit: the unit type): { t: 'autocast', ids, on }
	MountUp,     // match: { t: 'board', ids } for the selection (Shift+Q, the Mount up button)
	Unload,      // match: { t: 'unload', ids } for the selected carriers (Shift+E, the Unload button)
	CancelSite,  // match: { t: 'cancel', id } for the selected own site (the site card's Cancel)
	Fort,        // match: arm a fortification's dig (Unit: the FORTS kind; the Build menu, T and Shift+Y.. keys)
	Entrench,    // match: arm an entrenchment pattern (Unit: the pattern; the Trench menu, Shift+T for 'line')
	TrenchMenu,  // match: open or close the Orders panel's Trench menu (client/hud.js menuBtn 'trench')
	Formation,   // match: a Formation control (Unit: FTCFormationControls::Key, "menu" for the Form button, "shape:wedge", ...)
	Support,     // match: a support bar button (Unit: the SUPPORT kind): client/main.js aimSupport
	Area,        // match: the Shell area button (Shift+B): client/main.js startArea
	CoverToggle, // match: the cover preview on or off (client/cover-preview.js menu button, persisted as ww2-cover is)
	// Slice 1g
	LobbySetting,// lobby: one setting's control (Unit: LexToString(ETCLobbySetting), Group: the seat or -1, Job: -1 for the
	             // previous choice, else the next): FTCLobbySettings::PlanStep, the browser's select change
	SaveMatch,   // match, host: { t: 'save' } (client/main.js saveBtn; FTCPersistence::PlanSave)
	LoadSave,    // lobby, host: { t: 'loadSave', id } (Unit: the save id; FTCPersistence::PlanLoad)
	PauseMatch,  // match, host: { t: 'pause' } or { t: 'resume' } as the pause state says (client/main.js pauseBtn)
	// Slice 1h
	Research,    // match: a research button (Unit: 'tier', 'inf', 'guns' or 'armor'): { t: 'tech', kind } (FTCProgression)
	Recover,     // match, World: the Restore HQ or Restore Engineer card: { t: 'recover' } (FTCWorldRecovery::Recover)
	MatchOption, // lobby: Weather, Sandbox or a seat's Remove (Unit: LexToString(ETCMatchOption), Group: the seat for
	             // Remove, Job: -1 for the previous weather): FTCMatchOptions
	// Gap 13 (client/keys.js and client/main.js actions)
	Destroy,     // match: { t: 'destroy', ids } for the selection, no refund (Delete)
	Cover,       // match: { t: 'cover', ids, queue } for the selection (Shift+C queue false; the button's Shift queues)
	Rail,        // match, World Conquest: arm the send-by-rail aim (Shift+N)
	Army,        // match: select the whole army (Ctrl+A)
	Idle,        // match: the next idle ground unit, centred ('.')
	IdleAll,     // match: every idle ground unit (Shift+'.')
	IdleEngineer,// match: the next idle Engineer, centred (',')
	Follow,      // match: follow the selection, or stop following (Shift+Space)
	Alert,       // match: the newest alert (NewestAlertAt), else the selection's centre (Space)
	Home,        // match: go home; in Classic select the HQ (H)
	ControlsSheet, // match: open or close the Controls sheet (F1)
	EdgeScroll,  // the menu's Edge scroll switch, saved (client/camera.js ww2-edge)
	PanSpeed,    // the menu's Pan speed: Slow, Normal, Fast, saved (client/camera.js ww2-pan)
	// Slice 10
	Language,    // the language picker (client/index.html #langSel, i18n.js setLang): English or Spanish, saved
	Volume,      // the menu's Volume (client/audio.js bind): left click 10 points up, right click 10 down, saved
};
const TCHAR* LexToString(ETCUiAction Action);

// One HUD button's command: the action and what it names (a card's unit and building, a queue place's job, a group).
struct FTCUiCommand
{
	ETCUiAction Action = ETCUiAction::None;
	FString Unit;       // Recruit: the unit type; Build: the building kind; Stance: the stance key
	int32 Building = 0; // Recruit: the building asked (from), 0 lets the client choose as the browser does; CancelJob: the building
	int32 Job = 0;      // CancelJob: the waiting job
	int32 Group = -1;   // RecruitGroup: the group index in the card (FTCRecruitGroup::Group)
	FTCUiCommand() = default;
	explicit FTCUiCommand(ETCUiAction InAction) : Action(InAction) {}
	FString Describe() const;
};

// The armed targeting aim (client/main.js targeting). Only one at a time: arming one replaces another.
enum class ETCAim : uint8
{
	None,
	AttackMove,  // G: the next left click attack-moves the selection
	Rally,       // Shift+H: the next left click (world or minimap) sets the rally (client/main.js rallyAt)
	Build,       // a building: the next left click places it (AimBuild names it; Shift keeps the aim)
	Ability,     // an aimed ability (AimAbilityType, AimAbilityId): the next left click on the ground throws it
	Field,       // a dig or an entrenchment (FieldAim): the first click pins the centre, the second sends
	Support,     // a support call or area fire (PowerAim, FTCSupportPowers): a point kind or 'area' takes one click, a
	             // strip kind two (the centre, then the direction)
	Rail,        // Shift+N (World Conquest): the next left click sends the selected ground units by rail (client/main.js railTo)
};
const TCHAR* LexToString(ETCAim Aim);

UCLASS()
class THREECROSSROADS_API ATCPlayerController : public APlayerController
{
	GENERATED_BODY()

public:
	// client/selection.js selectionDragged: a press that travels more than this many pixels is a box, not a click.
	static constexpr float DragThreshold = 6.f;
	// server.js: the WebSocket server's maxPayload. A bigger message would never reach the rules.
	static constexpr int32 ServerMessageLimit = 16 * 1024;

	ATCPlayerController();
	virtual void BeginPlay() override;
	virtual void PlayerTick(float DeltaTime) override;

	// The right-click order for what lies under the pointer (TCOrders::Plan). bAttack: attack-move instead of move.
	// Returns the units named in the message that was handed to the socket, 0 when nothing was sent; LastOrderNote
	// says which order and why. Facing: a facing gesture's facing and width (client/orders.js options.face, .reach), unset
	// for a plain click. A move is built with the Formation menu's settings (FTCFormationControls::PlanMove).
	int32 IssueOrder(const FTCCursor& Cursor, bool bAttack, bool bQueue, const FTCMoveFacing& Facing = FTCMoveFacing());
	// A right-click on bare ground (or an attack-move there).
	int32 IssueMoveAt(FVector2D Ground, bool bAttack = false) { FTCCursor C; C.bHasGround = true; C.Ground = Ground; return IssueOrder(C, bAttack, false); }
	// The X key: stop the selection. Returns the ids sent.
	int32 IssueStop();
	// A left click with an aim armed (client/main.js mousedown while targeting): attack-move, rally or a building at the
	// ground under it. bHasGround false: the click found no ground (the browser says 'blocked' and keeps the aim).
	// bShift: Shift was held (queues; a building's aim stays armed). True when a message was handed to the socket.
	bool AimClick(bool bHasGround, FVector2D Ground, bool bShift);
	// The same left click at a viewport pixel: the ground under it is picked as the mouse's is (verification switches
	// press the view there). OutGround: the ground found.
	bool AimClickAt(FVector2D Screen, bool bShift, FVector2D* OutGround = nullptr);
	// Verification only: hold the pointer's ground at Ground (no mouse exists offscreen), so the placement preview is
	// drawn there by the HUD's own code. Cleared by ReleasePointerGround.
	void PinPointerGround(FVector2D Ground) { bPointerPinned = bPointerGround = true; PointerGround = Ground; }
	void ReleasePointerGround() { bPointerPinned = false; }
	// Disarm the aim (right click, Esc); Note, when given, is shown as the last order's note.
	void CancelAim(const TCHAR* Note = nullptr);
	// A key chord as client/keys.js matches it (Key with exactly these modifiers): the polled keys call this.
	// Returns false when the chord is not a command this client binds.
	bool KeyChord(const FKey& Key, bool bShift, bool bCtrl, bool bAlt);
	// The facing gesture (client/main.js beginFacing, mousemove, mouseup). The mouse path calls these every frame;
	// verification switches call them at computed pixels (not a physical mouse).
	// A right press on the world at Pixel (client/main.js mousedown, button 2): with an aim armed it cancels the aim;
	// otherwise, with a selection, a press whose order would be a move (FTCFormationControls::WouldMove) starts the
	// gesture and any other order is dispatched at once. True when a gesture started.
	bool RightPressWorld(FVector2D Pixel, bool bShift, bool bCtrl);
	// A left press on the world with the attack-move aim armed: the gesture starts on the ground under Pixel (no ground:
	// 'blocked', the aim stays). True when a gesture started.
	bool AttackMovePress(FVector2D Pixel, bool bShift, bool bCtrl);
	// The pointer is at Pixel this frame (bMoved: it moved since the last frame, which also takes Ctrl as the browser's
	// mousemove does; bHeld: the gesture's button is still down; a move without it cancels).
	void GestureMove(FVector2D Pixel, bool bMoved, bool bCtrl, bool bHeld);
	// The gesture's button came up at Pixel: the order is dispatched (with the drag's facing when it set one). Returns
	// the units named in what was sent (0: nothing, or no gesture of that button).
	int32 GestureRelease(ETCGestureButton Button, FVector2D Pixel, bool bShift, bool bCtrl);
	// Verification only: while pinned, each frame's gesture update reads this pixel with the button held (no mouse
	// exists offscreen, and the polled buttons are up), so the preview is computed and drawn by the frame's own code.
	// A pinned gesture ends only through GestureRelease, CancelGesture (Esc, a press, an aim) or the window's blur.
	void PinGesturePointer(FVector2D Pixel) { bGesturePinned = true; GesturePinnedPixel = Pixel; }
	void ReleaseGesturePointer() { bGesturePinned = false; }
	// A Formation control (the Form menu's buttons, Shift+V, [ and ]): settings, menu and the re-form order.
	bool RunFormation(ETCFormationControl Control);
	// The cover preview's view of this client (FTCCoverPreview::Gather with the drawn positions). Out points into the
	// match state: use it before the next message.
	void GatherCover(FTCCoverKnown& Out) const;
	// A box selection between two viewport pixels (client/selection.js box), as a mouse drag ends it.
	void BoxSelect(FVector2D A, FVector2D B, bool bAdd);
	// client/feedback.js show: a sentence over the hint line (the HUD's order note line, which keeps it for its own
	// 4 s rather than the browser's Seconds), always with the error blip (createFeedback's errorSound).
	void ShowFeedback(const FString& Sentence, double /*Seconds*/) { NoteOrder(true, Sentence); Feedback(TEXT("error")); }
	// A HUD button or its key. False when it did nothing (the note says why).
	bool RunUi(ETCUiAction Action) { return RunCommand(FTCUiCommand(Action)); }
	// A HUD button with what it names (recruit cards, queue places, group tabs). bShift: Shift was held (a recruit card
	// then buys up to five, client/hud.js buy).
	bool RunCommand(const FTCUiCommand& Command, bool bShift = false);
	// A verification switch pressing the HUD button that matches Command (same action and unit, building or group):
	// through the HUD's hit test at its drawn centre when the HUD drew it last frame, else straight to RunCommand.
	bool PressCommand(const FTCUiCommand& Command, bool bShift = false);
	// The same with the right button (an ability button: its autocast, client/hud.js oncontextmenu): through the HUD's
	// hit test at its drawn centre when drawn, else the right click's command straight to RunCommand.
	bool RightPressCommand(const FTCUiCommand& Command);
	// A control group chord (client/keys.js): the digit keys call this, as do verification switches. Recall replaces
	// the selection; a second recall within 300 ms centres the camera on the group.
	FTCGroupResult GroupChord(int32 Group, bool bCtrl, bool bShift, bool bAlt);
	// A press on the minimap at a viewport pixel (the mouse path calls this for every left and right press). Left points
	// the camera at that ground and starts a drag; right orders the selection there (bCtrl attack-moves, bShift
	// queues), or disarms an armed attack-move. bConsumed: the press was on the minimap and the world must not see it.
	FTCMinimapPress MinimapPress(FVector2D Pixel, bool bRight, bool bCtrl, bool bShift);
	// Verification only: feed one key event into this controller's engine input (APlayerController::InputKey), as the
	// viewport does for a physical key, so the next PlayerTick's key polling sees it. Not a physical keyboard.
	bool InjectKey(const FKey& Key, bool bPressed);
	// A verification switch pressing a HUD button: through the HUD's hit test at the button's drawn centre when the HUD
	// drew it last frame, else straight to RunUi (no renderer). The log says which.
	bool PressButton(ETCUiAction Action);
	// What a right-click at this viewport pixel would see (client/main.js mousedown cursor).
	FTCCursor CursorAt(FVector2D Screen) const;
	// client/main.js houseAt: the house cell whose drawn placeholder solids the ray through this viewport pixel meets
	// first (FTCWorldObjects::PickHouse against the drawn ground). False with no map or no house there.
	bool HouseAt(FVector2D Screen, FTCHousePick& Out) const;
	// Where a held unit is drawn on screen; false when it is not drawn or behind the camera.
	bool ScreenOf(int32 Id, FVector2D& Out) const;
	void SelectOwn(const TArray<int32>& Ids);
	// Point the camera at Metres (the minimap, a group's double tap, verification switches): ends the intro, a follow
	// and a zoom's anchor, as client/main.js does for every shortcut that moves the camera.
	void FocusOn(FVector2D Metres);
	// client/epilogue.js frame: the decided match's camera glide sets the target and the distance each frame (the next
	// frame clamps them as the wheel's zoom is clamped). Ends the intro and a follow.
	void EpilogueCamera(FVector2D Metres, float Dist) { FocusOn(Metres); Anchor.bActive = false; Distance = Dist; }
	// The browser's opening view of a new match (client/main.js receiveStart): look at the spawn from 60 m, from the
	// side away from the map's centre, so the centre lies ahead. Spawn and MapMetres (width, height) in reference metres.
	void StartView(FVector2D Spawn, FVector2D MapMetres);
	// Zoom to Metres (a verification switch); the next frame clamps it as the wheel's zoom is clamped. Ends the intro.
	void ZoomTo(float Metres) { SkipIntro(); Distance = Metres; }
	FVector2D ViewFocus() const { return Focus; }
	float ViewYaw() const { return Yaw; }
	// A verification switch's heading (degrees, the camera's yaw); the player turns with Q, E and the middle button.
	void SetViewYawForChecks(float Degrees) { SkipIntro(); Yaw = Degrees; }
	float ViewDistance() const { return Distance; }
	// The widest view (client/camera.js wide): the distance and focus at which the whole map fits the window with a 4%
	// margin, and whether it has been measured for the current map, heading and window.
	float WideDistance() const { return Wide.Dist; }
	FVector2D WideFocus() const { return Wide.Focus; }
	bool HasWideView() const { return Wide.bValid; }
	// The map's corners on screen as the engine projects them and as the widest-view fit's model does, at the camera's
	// pose now, with the viewport size: the evidence that the fit measures what is drawn.
	FString FramingCheck() const;

	// Gap 13 hooks, wired by ATCGameMode (WireController) to its alert feed and history and the decoded plan rows. Each
	// is optional; an unwired hook makes its key fall back as described.
	// The newest alert's ground point (client/alerts.js newest). Unwired or none: Space centres the selection.
	TFunction<bool(FVector2D& OutAt)> NewestAlertAt;
	// client/alerts.js toggleHistory (Alt+H) and navigate (Alt+Left: Step +1, the previous; Alt+Right: Step -1, the
	// next), which returns the visited alert's point. Unwired: the key says the alert history is not in this client.
	TFunction<void()> ToggleAlertHistory;
	TFunction<bool(int32 Step, FVector2D& OutAt)> VisitAlert;
	// The plan kind of an own unit from the snapshot's 'plans' rows (shared/sim.js planOf; 0 none), false when none is
	// held. ATCGameMode wires it to the snapshot's decoded plan rows (FTCMatchState::Plans). Unwired: the idle keys judge
	// by flags alone and a moving unit can be taken as idle; the log says so once a match.
	TFunction<bool(int32 Id, int32& OutKind)> PlanKindOf;
	// client/camera.js follow: the followed unit (0: none) and the chip's text ('Following Rifle Squad', empty: none).
	int32 FollowedId() const { return FollowId; }
	FString FollowChip() const;
	// The Controls sheet (F1) is open: the HUD draws TCKeys::ControlsSheet() while this is set.
	bool bControlsSheetOpen = false;
	// The camera intro is gliding (client/camera.js rig.intro).
	bool IntroActive() const { return Intro.IsActive(); }
	// The menu's settings (client/camera.js preferences): the button labels 'Edge scroll: On' and 'Pan speed: Normal'.
	bool EdgeScrollOn() const { return bEdgeScroll; }
	int32 PanSpeedIndex() const { return PanSpeed; }
	FString EdgeScrollLabel() const { return FString::Printf(TEXT("Edge scroll: %s"), bEdgeScroll ? TEXT("On") : TEXT("Off")); }
	FString PanSpeedLabel() const { return FString::Printf(TEXT("Pan speed: %s"), TCCameraFeel::PanSpeedName(PanSpeed)); }
	// "Language: English" or "Language: Español" (the picker's title and the option in use; drawn through TC_T)
	FString LanguageLabel() const;
	// "Volume: 80%" (client/index.html: the menu's Volume slider and its number)
	FString VolumeLabel() const;
	// client/main.js blip(kind) and bark(voice) (client/audio.js ui and voice): Ui 'click', 'recruit' or 'error' (a
	// numeric tone clicks), Voice 'move', 'attack', 'retreat', 'fire' or 'lost'; either may be null. Nothing plays
	// without the sound subsystem or an audio device.
	void Feedback(const TCHAR* Ui, const TCHAR* Voice = nullptr);
	// client/main.js mousedown with Alt (button 0, no aim, no intro) on the view, or on the minimap: pings.send at the
	// ground under Pixel. bMinimap: the pixel is on the minimap (its mmToWorld, unclamped). True when a ping was sent.
	bool PingAt(FVector2D Pixel, bool bMinimap);
	// This frame's edge push direction ('n', 'se', ...; empty: none), for a cursor that shows it.
	FString EdgeDir;

	TSet<int32> Selected;
	// Control groups 1 to 9 for this match and seat (emptied by a new match, kept by a reconnect to the same one).
	FTCControlGroups Groups;
	// The recruit card's open group (index into FTCCommandCard::Groups, -1: all closed) and the building whose
	// selection last opened one (client/hud.js drawRecruit opens the group a selected building makes most of, once).
	int32 RecruitGroupOpen = 0;
	int32 RecruitAutoFor = 0;
	// The last group operation, for the HUD and logs.
	FTCGroupResult LastGroup;
	bool bDragging = false;
	bool bDragMoved = false; // the press has travelled past DragThreshold: a box (it stays one, as in the browser)
	FVector2D DragFrom, DragTo;
	// The armed aim, the building it places (Aim == Build) and its hint (client/main.js setAim), for the HUD.
	ETCAim Aim = ETCAim::None;
	FString AimBuild, AimHint;
	// The Orders panel's Build menu is open (client/hud.js menu === 'build'), or its Trench menu (menu === 'trench'):
	// one menu at a time.
	bool bBuildMenuOpen = false;
	bool bTrenchMenuOpen = false;
	// Aim == Ability: the unit type and ability id (client/main.js aimedUnit and targeting) and the preview ring's radius
	FString AimAbilityType, AimAbilityId;
	double AimAbilityRadius = 0;
	// Aim == Field: the dig or entrench aim with its mesh state (FTCFieldworks keeps it as the browser's frame does),
	// and this frame's preview of it (FTCFieldworks::Preview, run once a frame in PlayerTick) for the HUD.
	FTCFieldAim FieldAim;
	FTCFieldPreview FieldPreview;
	// Aim == Support: the support call or area aim (FTCSupportPowers keeps it as the browser's targeting, aimCenter and
	// aim mesh) and this frame's preview of it for the HUD.
	FTCSupportAim PowerAim;
	FTCSupportPreview PowerPreview;
	// The Formation menu's settings, its control group copies, the units' last facings and the facing gesture
	// (client/main.js fm, groupForm, faced, facingGesture), and this frame's formation preview for the HUD.
	FTCFormationControls Formation;
	FTCFormationPreview FacingPreview;
	// Where this client draws a unit (the renderer's smoothed point, else the snapshot's), as client/main.js units.get
	// gives FTCFieldworks.
	FTCPositionOf PositionOf() const;
	// client/keys.js keyContexts 'building': Classic (lobby) with a finished own Production Building's lettered cards.
	// The cards in reading order (their purchase commands), empty when that context is not active.
	TArray<FTCUiCommand> LetteredCards() const;
	// The ground under the pointer this frame (for the placement preview), when the pointer is over the view.
	bool bPointerGround = false;
	FVector2D PointerGround = FVector2D::ZeroVector;
	bool bPointerPinned = false;
	// The enemy unit under the pointer this frame (0: none), for the HUD's hover line.
	int32 HoverEnemy = 0;
	// The point of the last order that was sent (the ground, or the attacked unit), its colour and when (world
	// seconds), for the HUD's click ring.
	FVector2D LastOrderAt = FVector2D(-1, -1);
	FColor LastOrderColor = FColor(0x5f, 0xa4, 0xf2);
	double LastOrderTime = -100.0;
	// What the last order or button did: sent (to how many units) or why nothing was sent. Empty until the first one.
	FString LastOrderNote;
	bool bLastOrderSent = false;
	double LastOrderNoteTime = -100.0;

private:
	struct FWideView { bool bValid = false; float Dist = 400.f; FVector2D Focus = FVector2D(80, 80); FString Key; };

	ATCGameMode* Mode() const;
	bool GroundAt(FVector2D Screen, FVector2D& Out) const;
	// The cursor in viewport pixels. With bEvenOutside, the position the viewport tracks while a button holds the
	// mouse captured, which can lie outside the window (negative or past the size).
	bool CursorPixels(FVector2D& Out, bool bEvenOutside) const;
	bool WindowFocused() const;
	bool ScreenPoint(const ATCGameMode& M, const FTCUnit& U, const FTCUnitDef* Def, FVector2D& Out) const;
	// client/main.js pick: the nearest held, drawn unit passing Test within Radius pixels (0: 45 for vehicles, 32 for
	// infantry) of Screen. Squads riding in a carrier are never picked.
	int32 PickAt(FVector2D Screen, TFunctionRef<bool(const FTCUnit&, const FTCUnitDef&)> Test, float Radius = 0.f) const;
	bool Pickable(const ATCGameMode& M, const FTCUnit& U, const FTCUnitDef* Def) const;
	bool Boxable(const ATCGameMode& M, const FTCUnit& U, const FTCUnitDef* Def) const;
	void ClickSelect(FVector2D Screen, bool bAdd);
	void PruneSelection(const ATCGameMode& M);
	void UpdatePointer(bool bFocused);
	// A left press on a HUD button runs it; a left or right press on a HUD panel is swallowed. True when the HUD took it.
	bool HandleHudPress(bool bLeft, bool bRight, FVector2D Mouse);
	void UpdateGroupKeys();
	// The command keys (stop, attack-move, retreat, stances, rally, Classic builds, Esc), by exact chord.
	void UpdateCommandKeys();
	// Carries out an ability plan (FTCAbilities): arms its aim, sends its message or notes why nothing went out.
	bool RunAbilityPlan(const FTCAbilityPlan& Plan);
	// Sends a support order plan (FTCSupportOrders) or notes why nothing went out; clears the selection when the plan
	// says the browser does (the Classic site card's Cancel).
	bool RunSupportPlan(const FTCSupportPlan& Plan, const TCHAR* What);
	// Carries out a support call or area fire plan (FTCSupportPowers): arms PowerAim, pins a strip's centre, sends, or
	// notes why nothing went out.
	bool RunPowerPlan(const FTCSupportPowerPlan& Plan);
	// The controller's own planners that claim a right click before a move (assist, repair, board, join planned works):
	// FTCFormationControls::WouldMove's bOtherKind.
	bool OtherRightClickKind(const FTCCursor& Cursor) const;
	// A ground order went out at At: the cover preview flashes there (client/main.js orders feedback, at.id undefined).
	void FlashCover(FVector2D At);
	// Arm an aim (client/main.js setAim: nothing while observing).
	bool SetAim(ETCAim Kind, const FString& Hint, const FString& BuildKind = FString());
	void CancelDrag() { bDragging = bDragMoved = false; }
	void NoteOrder(bool bSent, const FString& Note);
	// Hands Plan's message to the socket when orders are acted on, notes what happened. Returns the units named in a
	// message that went out (0 otherwise); bOutSent says whether it went out (a rally can name no unit).
	int32 SendPlan(const FTCOrderPlan& Plan, const TCHAR* What, bool* bOutSent = nullptr);
	void UpdateCamera(float Dt, bool bFocused);
	void UpdateWideView(const ATCGameMode& M, int32 VX, int32 VY);
	// Gap 13 helpers
	// Runs one keys.js binding (Shift as the chord had it). False for the camera's own keys (pan, rotate), which stay held.
	bool RunBinding(const FTCKeyBinding& B, bool bShift);
	// client/main.js keyContexts: 'classic' or 'army', 'building' with lettered cards, 'targeting' with an aim armed.
	TArray<ETCKeyContext> KeyContexts(bool bLettered) const;
	// The held own units as the selection keys read them; bScreen also measures who is on the screen.
	TArray<FTCKeyUnit> KeyUnits(bool bScreen) const;
	// client/selection.js pick: the nearest own mobile unit within its radius, else the nearest own building (0: none).
	int32 PickSelectable(FVector2D Screen) const;
	// client/selection.js doubleClick at a viewport pixel.
	void DoubleClickSelect(FVector2D Screen, bool bMapWide);
	// Replace the selection and, with bCentre, centre the camera on it (client/main.js centerSelection).
	void ReplaceSelection(const TArray<int32>& Ids, bool bCentre);
	// client/camera.js clamp (the target stays on the map; drawn toward the widest view's target past half of it).
	void ClampFocus(const ATCGameMode& M);
	// client/camera.js frame: the target on (x, z), no farther than the widest view.
	void FrameOn(FVector2D Metres);
	// client/camera.js solveAnchor: the anchored point back under its pixel.
	void SolveAnchor(int32 VX, int32 VY);
	// client/camera.js wheel at the cursor (Notch +1 out, -1 in).
	void WheelZoom(int32 Notch, bool bHasCursor, FVector2D Mouse, int32 VX, int32 VY);
	// client/camera.js skipIntro (also a pending intro that has not begun): true when one was ended.
	bool SkipIntro();
	// Any key or press during the intro ends it (client/main.js keydown, introPress); the keys that did it are not
	// acted on, a skipping right press or wheel notch is dropped.
	void CheckIntroSkip();
	void CancelFollow() { FollowId = 0; Anchor.bActive = false; }
	bool StartFollow();
	// client/main.js cancelInput (the sheet opening): drag, gesture, middle-drag and the held keys let go.
	void CancelInput();
	// Rail eligibility (client/main.js railers): own selected ground units, no structure, aircraft, ship or train.
	TArray<int32> Railers() const;

	FVector2D Focus = FVector2D(80, 80);
	// yaw -90: x to the right, rows down the screen, the browser's yaw 0. Pitch is the browser's PITCH, 0.95 rad.
	float Yaw = -90.f, Pitch = 54.43099f, Distance = 85.f;
	bool bWasFocused = false;
	// client/main.js keydown: a chord that ran a command takes its key out of the held set, so a shortcut sharing a
	// camera key (Shift+Q, Shift+E, a card letter on W, A, S, D, Q or E) does not also pan or turn. Until released.
	TSet<FKey> ChordKeys;
	FWideView Wide;
	// the pointer's pixel at the last gesture update (client/main.js mousemove fires only when it moves)
	FVector2D GestureLast = FVector2D(-1, -1);
	// IssueOrder is dispatching an attack-move gesture's or aim click's cursor, which holds only its ground
	// (client/main.js beginFacing({ ground: g }, e, true)): no assist, board or join is judged under it
	bool bCursorGroundOnly = false;
	bool bGesturePinned = false;
	FVector2D GesturePinnedPixel = FVector2D::ZeroVector;

	// Gap 13 camera state (client/camera.js): the target's glided height, the follow, the zoom anchor, the middle-drag,
	// the intro (pending until the widest view is measured), edge scrolling and its pointer, the saved settings.
	float TargetHeight = 0.f;
	bool bHeightKnown = false;
	int32 FollowId = 0;
	struct FAnchor { bool bActive = false; FVector Point = FVector::ZeroVector; FVector2D Pixel = FVector2D::ZeroVector; float Left = 0.f; };
	FAnchor Anchor;
	bool bMiddleDrag = false;
	float MiddleX = 0.f;
	FTCCameraIntro Intro;
	bool bIntroPending = false;
	FTCPointerEdge PointerEdge;
	FTCEdgeScroll EdgeScroll;
	bool bEdgeScroll = true;
	int32 PanSpeed = 1;
	// this frame's presses that ended the intro and are therefore not acted on
	bool bIntroDropRight = false, bIntroDropWheel = false;
	TSet<FKey> IntroKeys;
	// the selection keys' state: the idle walk, the double-click pair, the last left press's time
	FTCIdleCursor IdleCursor;
	FTCDoubleClick DoubleClick;
	double PressTime = 0;
	bool bPlanWarned = false;
};
