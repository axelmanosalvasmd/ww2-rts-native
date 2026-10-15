// The lobby's and the match's session controls (parity CLI-14, CLI-15, CLI-16, CLI-25, CLI-26, CLI-27, and the room
// box of CLI-02): Watch as a spectator / Take a seat, the Name field, the invite link and its Copy button, the Room
// box (Join, New), the match menu (Pause or Resume, Save, Restart, End, Hand <name> to AI, Leave, with the browser's
// second-click confirm), Horde's Send next wave, and the sandbox panel (unit, side, Spawn armed per click, right click
// stops). Ported from the pinned reference (FTCMatchControls::ReferencePin):
//   client/main.js     sendCmd and observerControls (what an observer or an operative may send in play), the name
//                      field (16 characters, default SoldierNN, saved as ww2-name, { t: 'name' } on change), roomLink
//                      and the invite link (publicUrl when the page was opened on a local address), the Copy button
//                      (Copied / Press Ctrl+C for 3 s), goRoom and New room (5 base-36 characters), watchBtn
//                      ({ t: 'sit' } or { t: 'spectate' }, its label, title and when it hides), renderMatchMenu (which
//                      buttons the host and the others see, Hand <name> to AI per offline seat in play), confirmClick
//                      (a second click within 3 s), the 'left' line, the sandbox panel (startGame: shown in a sandbox
//                      match unless watching; sbUnit lists UNITS without structures; sbSide Enemy or Mine; sbGo arms the
//                      spawn aim; each click sends { t: 'spawn', unit, enemy, x, z } and stays armed).
//   client/hud.js      Horde's Send next wave ({ t: 'nextwave' }, title 'Skip the break', shown to the host between
//                      waves).
//   client/index.html  labels and titles of these controls.
//   server.js          the handlers: name (cleanName), spectate (seated, lobby, fewer than 8 watching), sit (not seated,
//                      lobby, a seat free), restart, end, pause, resume, save, handAi (an offline human seat, in play),
//                      leave (seated, in play: 'left' then the socket closes), nextwave (Horde, between waves, not
//                      paused), spawn (sandbox); lobby() fields (spectator, amHost, listed, spawns, publicUrl, code).
// Every message is one the browser sends, with its fields. Owner decision (round 15): the sandbox panel never offers
// the Supply Truck; a truck spawned in a sandbox crashes the pinned server (shared/supply-convoys.js), so PlanSpawn
// refuses it too. Texts are the reference's English; the HUD draws them through TC_T (client/es.js).
#pragma once

#include "CoreMinimal.h"
#include "Engine/GameViewportClient.h"
#include "Client/TCEndReport.h"
#include "TCMatchControls.generated.h"

class FJsonObject;

enum class ETCMatchControl : uint8
{
	Watch,        // lobby: { t: 'spectate' } or { t: 'sit' }
	Rename,       // the Name field: { t: 'name', name }
	CopyLink,     // the invite link's Copy (local: the clipboard)
	JoinRoom,     // the Room box's Join (local: go to that room)
	NewRoom,      // the Room box's New (local: a new private room)
	Menu,         // match: the Menu button opens or closes the menu (local)
	Pause,        // match menu, host: { t: 'pause' } or { t: 'resume' }
	Save,         // match menu, host: { t: 'save' }
	Restart,      // match menu, host, confirmed: { t: 'restart' }
	End,          // match menu, host, confirmed: { t: 'end' }
	HandAi,       // match menu, host, per offline seat: { t: 'handAi', slot }
	Leave,        // match menu, a seated player, confirmed: { t: 'leave' }
	NextWave,     // Horde, host, between waves: { t: 'nextwave' }
	SandboxUnit,  // sandbox panel: the unit select (local)
	SandboxSide,  // sandbox panel: Enemy or Mine (local)
	SandboxSpawn, // sandbox panel: arms the spawn aim (local); each map click then sends { t: 'spawn', ... }
};
const TCHAR* LexToString(ETCMatchControl Kind);
bool LexFromString(ETCMatchControl& Out, const TCHAR* Text);

// One control as the browser shows it.
struct FTCControlButton
{
	ETCMatchControl Kind = ETCMatchControl::Watch;
	int32 Slot = -1;          // HandAi: the seat
	FString Label;            // the button's text now (a confirm button reads 'Click again to confirm' while armed)
	FString Title;            // its tooltip (the browser's title attribute), empty when it has none
	bool bShown = false;
	bool bEnabled = true;     // shown but a click would send nothing: false, with Why
	FString Why;
	bool bPending = false;    // Watch, Rename: asked, not yet shown by a lobby
};

// What a click does.
struct FTCControlRequest
{
	ETCMatchControl Kind = ETCMatchControl::Watch;
	int32 Slot = -1;
	TSharedPtr<FJsonObject> Message; // the one message to send; null when nothing goes out
	FString Note;                    // what happened, or why nothing goes out
	bool bArmed = false;             // a confirm button's first click: nothing sent, the label changes for 3 s
	bool bCloseMenu = false;         // the browser closes the menu after this click
	FString Room;                    // JoinRoom, NewRoom: the room to open (the seat label is kept by the caller)
};

// One unit of the sandbox panel's select (client/main.js sbUnit, from shared/sim.js UNITS).
struct FTCSandboxUnit
{
	const TCHAR* Type;
	const TCHAR* Name;
};

class FTCMatchControls
{
public:
	static constexpr const TCHAR* ReferencePin = TEXT("e4e579a");
	static constexpr double ConfirmSeconds = 3.0;   // confirmClick: Date.now() - armed < 3000
	static constexpr double CopySeconds = 3.0;      // the Copy button's label returns after 3 s
	static constexpr int32 NameMax = 16;            // <input id=name maxlength=16>
	static constexpr int32 MaxPlayers = 6;          // COLORS.length (shared/sim.js MAX_PLAYERS)
	static constexpr int32 MaxSpectators = 8;       // server.js MAX_SPECTATORS

	// --- what the client knows ---
	// Reads a lobby message; false (nothing changes) when it is not one or is malformed. A lobby in the 'lobby' state
	// closes the menu, clears the pause and disarms the confirms and the spawn aim (renderLobby's reset).
	bool ApplyLobby(const FJsonObject& Message);
	void Reset();
	// A new socket opened: pending requests are forgotten (the server may never have read them).
	void OnSocketOpened() { bWatchPending = false; PendingName.Reset(); }
	// The start message: a sandbox match shows the panel unless this client watches (startGame).
	void OnStart(bool bSandbox);
	// The 'pause' message (receivePause).
	void SetPaused(bool bInPaused) { bPaused = bInPaused; }
	// From each snapshot: this seat is out of the match (lastSnap.out[me]); Horde's wave on the map (mode.active).
	void SetOut(bool bInOut) { bOut = bInOut; }
	void SetHorde(bool bIsHorde, bool bWaveActive) { bHorde = bIsHorde; bWaveOn = bWaveActive; }
	// The 'left' message: this seat was handed to an AI and the socket closed (the server's answer to Leave).
	void OnLeft() { bLeft = true; }

	bool HasLobby() const { return bHasLobby; }
	bool InLobby() const { return bHasLobby && State == TEXT("lobby"); }
	bool InPlay() const { return bHasLobby && State == TEXT("play"); }
	bool Watching() const { return bWatching; }
	bool AmHost() const { return bHasLobby && bAmHost; }
	bool Operative() const { return Role == TEXT("operative"); }
	bool Paused() const { return bPaused; }
	bool Left() const { return bLeft; }
	const FString& RoomCode() const { return Code; }
	const FString& PublicUrl() const { return PublicUrlText; }
	// observing(): an operative, a spectator, or a seat that is out
	bool Observing() const { return Operative() || bWatching || bOut; }
	// client/main.js sendCmd: whether a message of this type goes out now (false: the browser drops it silently)
	bool Sendable(const FString& Type) const;
	// seatLimit: World 6; a public room the map's spawns (3 when absent), at most 6; else 6
	int32 SeatLimit() const;

	// --- Watch as a spectator / Take a seat ---
	FTCControlButton WatchButton() const;
	FTCControlRequest PlanWatch() const;

	// --- the Name field ---
	// What the field holds (set from the saved name, a -TCName or the default; changed by PlanRename).
	const FString& Name() const { return FieldName; }
	void SetFieldName(const FString& Name) { FieldName = Name.Left(NameMax); }
	// The field's first value: the given name cut to 16, else the saved one, else SoldierNN (NN from 10 to 99).
	static FString InitialName(const FString& Given, const FString& Saved, int32 RandomTwoDigits);
	// server.js cleanName: what the server keeps and shows in the lobby
	static FString CleanName(const FString& Name);
	// The field changed to Typed (cut to 16 as the input does): { t: 'name', name } when it differs from what the
	// field held; the field takes the new value either way.
	FTCControlRequest PlanRename(const FString& Typed);
	FTCControlButton NameField() const;

	// --- the invite link ---
	// roomLink(base) = base + '/play#' + room. Base: the lobby's publicUrl when this client reached the server on a
	// local address (localhost, 127.x, [::1]) and the server has one; else the server's own origin.
	FString InviteLink(const FString& SocketUrl, const FString& Room) const;
	static FString RoomLink(const FString& Base, const FString& Room) { return Base + TEXT("/play#") + Room; }
	static FString Origin(const FString& SocketUrl);
	static bool IsLocalHost(const FString& SocketUrl);
	// The Copy button: copies Link through Clipboard; 'Copied' when it took the link, else 'Press Ctrl+C' (the
	// browser's fallback). The label returns to 'Copy' after 3 s.
	// Clipboard: the system clipboard (set by the game's glue: copy, then read back); unset, nothing is copied.
	static TFunction<bool(const FString&)> Clipboard;
	FTCControlRequest CopyLink(const FString& Link, double Now);
	void NoteCopied(bool bOk, double Now) { CopiedAt = Now; bCopyOk = bOk; }
	FTCControlButton CopyButton(double Now) const;

	// --- the Room box ---
	// goRoom: the typed code trimmed and lowercased; 3 to 12 letters or digits goes there, anything else puts the
	// current room back in the box and goes nowhere.
	static FTCControlRequest PlanJoinRoom(const FString& Typed, const FString& CurrentRoom);
	// New: Math.random().toString(36).slice(2, 7), five base-36 characters
	static FTCControlRequest PlanNewRoom(TFunctionRef<int32(int32)> RandomBelow);
	static FTCControlRequest PlanNewRoom();

	// --- the match menu ---
	bool MenuOpen() const { return bMenuOpen; }
	FTCControlButton MenuButton() const;
	FTCControlRequest ToggleMenu();
	void CloseMenu() { bMenuOpen = false; }
	// The menu's buttons in client/index.html's order: Pause, Save, Restart, End, Hand <name> to AI (per offline seat),
	// Leave (all of them, with bShown).
	TArray<FTCControlButton> MenuButtons(double Now) const;
	FTCControlButton MenuControl(ETCMatchControl Kind, int32 Slot, double Now) const;
	// A click on one of them. The confirm buttons (Restart, End, Leave) arm on the first click and send on a second
	// within 3 s, as confirmClick.
	FTCControlRequest PressMenu(ETCMatchControl Kind, int32 Slot, double Now);

	// --- Horde: Send next wave ---
	FTCControlButton NextWaveButton() const;
	FTCControlRequest PlanNextWave() const;

	// --- the sandbox panel ---
	static TArrayView<const FTCSandboxUnit> SandboxUnits();         // the panel's list (no structures, no truck)
	static TArrayView<const FTCSandboxUnit> ReferenceSandboxUnits(); // client/main.js sbUnit as the reference fills it
	bool SandboxShown() const { return bSandboxMatch && !bWatching; }
	const FTCSandboxUnit& SandboxUnit() const;
	bool SandboxEnemy() const { return bSpawnEnemy; }
	bool SpawnArmed() const { return bSpawnArmed; }
	// The selects: a cycling button each (Direction > 0 next, < 0 previous); Unit by type.
	void StepSandboxUnit(int32 Direction = 1);
	bool SetSandboxUnit(const FString& Type);
	void SetSandboxEnemy(bool bEnemy) { bSpawnEnemy = bEnemy; }
	// Spawn: arms the aim (setAim('spawn')); a right click disarms (DisarmSpawn).
	FTCControlRequest ArmSpawn();
	void DisarmSpawn() { bSpawnArmed = false; }
	// A left click on the ground at (X, Z) metres while armed: { t: 'spawn', unit, enemy, x, z }; stays armed.
	FTCControlRequest PlanSpawn(double X, double Z) const;
	TArray<FTCControlButton> SandboxButtons() const;

	// The line the browser shows after Leave ('left').
	static FString LeftLine() { return TEXT("You left the match; an AI took over your army. Reload to rejoin the lobby when the match is over."); }

	// Remember a request that went out (Watch and Rename show pending until a lobby confirms them).
	void NoteSent(const FTCControlRequest& Request);
	int32 Confirmed() const { return ConfirmedCount; }

	struct FSeat { FString Name; bool bAi = false; bool bConnected = false; };
	const TArray<FSeat>& Seats() const { return SeatList; }
	const TArray<FString>& Spectators() const { return SpectatorNames; }

private:
	bool bHasLobby = false;
	FString State, Role, Mode, Code, PublicUrlText;
	int32 You = -1, Host = -1, Spawns = -1;
	bool bAmHost = false, bWatching = false, bListed = false, bRustHost = false;
	TArray<FSeat> SeatList;
	TArray<FString> SpectatorNames;

	bool bPaused = false, bOut = false, bHorde = false, bWaveOn = false, bLeft = false;
	bool bMenuOpen = false;
	double RestartArmedAt = -1e9, EndArmedAt = -1e9, LeaveArmedAt = -1e9;
	double CopiedAt = -1e9;
	bool bCopyOk = false;
	FString FieldName;

	bool bSandboxMatch = false, bSpawnEnemy = true, bSpawnArmed = false;
	int32 SandboxIndex = 0;

	bool bWatchPending = false, bWatchAskedSpectate = false;
	FString PendingName;
	int32 ConfirmedCount = 0;

	double& ArmedAt(ETCMatchControl Kind);
	double ArmedAtConst(ETCMatchControl Kind) const;
	static TSharedRef<FJsonObject> Msg(const TCHAR* Type);
};

// ---------------------------------------------------------------------------------------------------------------------
// What the HUD draws for these controls: rows of text and buttons. A button's Key names what a click does
// (ATCGameMode::RunControl, through ETCUiAction::MatchControl with FTCUiCommand::Unit = Key and Group = Slot): a
// LexToString(ETCMatchControl) for the controls above, 'Field.<name>' for a text field, 'Browse' and 'Browser.<what>'
// for the room directory (Client/TCRoomBrowser.h).
struct FTCControlItem
{
	FString Label;
	FString Key;
	int32 Slot = -1;
	bool bEnabled = true;
	bool bOn = false;      // drawn pressed (an armed Spawn, the field being typed in, a checked box)
	FString Hover;         // the browser's title, or why it is disabled
};
struct FTCControlRow
{
	enum ETone : uint8 { Plain, Dim, Warn, Good, Heading };
	FString Text;          // before the buttons (or alone)
	ETone Tone = Plain;
	TArray<FTCControlItem> Items;
};

// The text inputs: the lobby's Name and Room code, the directory's nickname, match name and room code. This client
// draws on a canvas, so a field is typed into after a click on it; Enter or a click elsewhere ends the typing (the
// browser's change event), Escape ends it and puts the old text back.
// Round 16 (controls): the characters an <input> takes. With the window's text input wired (bCharInput: the glue's
// Slate input processor hands every typed character to Char, after the desktop's dead keys and compose, so 'á', 'ñ',
// '¿' and any other text arrive as typed), the keys only end, cancel, delete and paste; without it (tests, a run with no
// text input) the key names give letters, digits, space, '-', '_' and '.' as before. maxlength counts UTF-16 code units
// as the browser does, and a character outside the BMP (two units) is never split. Ctrl+V and Shift+Insert paste the
// clipboard as Chromium's text field inserts it: line breaks become spaces (TextFieldInputType's beforetextinserted),
// the text is cut to the room maxlength leaves, never inside a surrogate pair. The caret stays at the end (no caret
// keys, no selection).
enum class ETCTextField : uint8 { None, Name, RoomCode, Nickname, MatchTitle, JoinCode };
const TCHAR* LexToString(ETCTextField Field);
struct FTCTextFields
{
	ETCTextField Active = ETCTextField::None;
	FString Buffer;          // the active field's text while typing
	FString Before;          // its text when the typing began (Escape)
	// the values (Name lives in FTCMatchControls::Name)
	FString RoomCode, Nickname, MatchTitle, JoinCode;
	bool bPublic = true;     // the directory's Visibility select ('public' is its first option)

	static int32 MaxLen(ETCTextField Field);  // the inputs' maxlength: 16, 12, 16, 48, 12
	bool bCharInput = false; // characters come from the window's text input (Char), not from key names
	bool bCtrl = false;      // Ctrl is held, as its own key events say (Ctrl+V)
	// The system clipboard's text (the glue sets FPlatformApplicationMisc::ClipboardPaste; unset: nothing to paste)
	static TFunction<FString()> ClipboardText;
	void Begin(ETCTextField Field, const FString& Current);
	// A typed character while a field is active: control characters (Enter, Escape, Backspace and Tab arrive here too)
	// are not text; a character that would pass maxlength is dropped. True when it was taken (a field is active).
	bool Char(TCHAR C);
	// A paste into the active field (Chromium's rule above). Returns how many UTF-16 units went in.
	int32 Paste(const FString& Text);
	// Chromium's LimitLength: at most N units, one less when the cut would split a surrogate pair
	static FString LimitLength(const FString& Text, int32 N);
	// A key while typing. True when the key was taken (every key is, while a field is active: the game's hotkeys do not
	// fire under typing); OutDone is set to the field when Enter ends it, OutCancelled when Escape does.
	bool Key(const FName& KeyName, bool bShift, ETCTextField& OutDone, bool& OutCancelled);
	// The text a field shows (with the caret while typing).
	FString Shown(ETCTextField Field, const FString& Value) const;
	FString& Value(ETCTextField Field);
private:
	TCHAR PendingLead = 0;   // the first half of a surrogate pair, until its second half arrives
};

// One step of the -TCControlsCheck verification script (ATCGameMode::ControlsFrame): wait Delay seconds after the
// previous step and until Cond holds, then do Action with Arg.
struct FTCControlsStep
{
	double Delay = 0.0;
	FString Cond, Action, Arg;
};

// Round 16 (controls): client/index.html #roomBanners as client/connection.css draws it: fixed, centred, its top 8 px
// under #top (the score strip; 16 px with the HUD hidden: main.js positionRoomBanners), at most min(510px, 100vw - 40px)
// wide, one box per banner (each as wide as its text, centred, 4 px apart): padding 6px 12px, a 1 px border (red for
// #connectionBanner), rgba(25,28,30,0.95), 14px/1.35 text centred and wrapped. Screen pixels (outside #hud's zoom).
// The pause banner's text is main.js's: "Paused by <name>" or "Waiting for <name> (<n> s)" (FTCMatchState::PauseText).
struct FTCRoomBanner
{
	FString Text;            // English; drawn through TC_T
	bool bConnection = false; // #connectionBanner (red border); else #pauseBanner
};
struct FTCRoomBanners
{
	static FTCPaint Layout(const TArray<FTCRoomBanner>& Banners, float Top, float ViewW, const FTCMeasure& Measure);
};

// Round 16 (controls): the game's viewport client, only to hand the window's typed characters (InputChar: SDL's text
// input, after dead keys and compose) to the active text field; everything else is the engine's. Named in
// Config/DefaultEngine.ini ([/Script/Engine.Engine] GameViewportClientClassName). Defined in Client/TCMatchControls.cpp.
UCLASS()
class THREECROSSROADS_API UTCGameViewportClient : public UGameViewportClient
{
	GENERATED_BODY()
public:
	virtual bool InputChar(FViewport* InViewport, int32 ControllerId, TCHAR Character) override;
};
