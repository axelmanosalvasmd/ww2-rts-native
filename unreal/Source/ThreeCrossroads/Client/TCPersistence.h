// Saved matches as a client sees them: a port of the reference's save, list, load and autosave at FTCPersistence::
// ReferencePin. The server owns every byte of a save (server/saves.js: the sim as a gzipped v8 structured clone plus a
// JSON meta file per save, under the server's SAVES_DIR); no client ever reads, writes or parses a save file, so this
// module has no file format and no native ABI. It speaks only the reference WebSocket protocol:
//
//   client -> server  { t: 'save' }            the host, in a match (client/main.js saveBtn; server.js 'save')
//                     { t: 'loadSave', id }    the host, in the lobby (client/main.js saveList buttons; 'loadSave')
//   server -> client  { t: 'saved', id }       only to the saving socket (server.js 'save')
//                     { t: 'deny', cmd, reason } cmd 'save' or 'loadSave', reason 'save' when the write or read fails
//                     lobby.saves              every lobby message: this room's saves, newest first (server.js lobby)
//
// Ported from:
//   server/saves.js  validSaveId (the id pattern), AUTOSAVES_KEPT and SAVES_KEPT, listSaves' order (savedAt, newest
//                    first) and its per-room listing (ids start with the room code and '-');
//   server.js        lobby()'s saves rows ({ id, kind, savedAt, mode, minutes, names, world }), the 'save' and
//                    'loadSave' branches (host, room state, id checks), saveRoom (meta: minutes = floor(tick * TICK / 60),
//                    names without ' (AI)', world: { total, owned per team }), loadRoom and beginMatch (a loaded match
//                    starts paused by the host), AUTOSAVE (World Conquest only, every 2 minutes of play since the last
//                    save of any kind, never once there is a winner);
//   client/main.js   renderLobby's saves panel and rows (MODE_NAMES, the name line, the muted line, when()), the saveBtn
//                    in renderMatchMenu (shown to the host only) and its click (send, then close the menu),
//                    connection.on('saved') and sendCmd's gates (operatives send nothing but fps, ping, resync and name
//                    while in play; 'save' is one of the observerControls a spectator may send);
//   client/feedback.js show (the sentence over the hint for 2 s, always with the error blip);
//   client/index.html  the texts ('Save match' and its title, 'Saved matches', 'Load') and where they live: the hint
//                    and the match menu are inside #hud, which the lobby hides.
//   client/availability.js DENY_SENTENCES.save ("The match could not be saved or loaded"; TCOrders::DenySentence has it).
//
// Measured against the pinned server (a scratch copy of e4e579a driven in-process as its test-saves.mjs does):
//   - a host's save in play answers { t: 'saved', id } to the host, then a lobby (state 'play') to everyone with the new
//     row; a non-host's save in play falls through to the rules and comes back { t: 'deny', cmd: 'save', reason:
//     'blocked' } (the browser never sends it: the button is the host's);
//   - an autosave sends nothing: its row appears in the next lobby broadcast after its files land (a 4 MB World save
//     finished after the 'end' that followed it, so that lobby did not list it yet);
//   - a load in the lobby with an id that fails validSaveId, names another room, or comes from a non-host is dropped
//     without a word; a valid id of this room whose file is missing answers deny loadSave 'save', then a lobby;
//   - a good load sends everyone, in order: pause(off), lobby(state 'play'), pause(on, by the host, reason 'host'),
//     start (a new matchId), pause(on). The match resumes on its saved tick and waits for the host's 'resume'.
//
// Not ported: the Spanish texts (client/es.js), the browser's locale (when() follows the viewer's locale; this client
// writes en-US, "Oct 12, 03:45 PM", checked against Node 26's ICU), Date.parse of a string savedAt (the server writes a
// number; a string shows "Invalid Date" here), and the HTML escaping (text is drawn, not parsed). The browser keeps no
// pending state for a save or load; Status() is this client's own record (native) for logs and verification switches.
#pragma once

#include "CoreMinimal.h"
#include "Misc/Timespan.h"

class FJsonObject;
class FJsonValue;
class FTCMatchState;

// room-session.js matchStorage: one view per server/room/token, including process restarts.
// Storage filenames contain only a digest. No token or match view is written to a log.
struct FTCMatchView
{
	int32 MatchId = -1;
	double X = 0, Z = 0, Y = 0, Yaw = 0, Dist = 85;
	bool bHasY = false;
	TArray<int32> Selected;
	TMap<int32, TArray<int32>> Groups;
	bool IsValid() const;
	TSharedPtr<FJsonObject> Json() const;
	static bool Parse(const FJsonObject& Object, FTCMatchView& Out);
};

class FTCMatchViewMemory
{
public:
	void Open(const FString& Directory, const FString& Server, const FString& Room, const FString& Token);
	bool Read(FTCMatchView& Out) const;
	bool Write(const FTCMatchView& View) const;
	void Clear() const;
	bool IsOpen() const { return !File.IsEmpty(); }
private:
	FString File;
};

// One row of the lobby's saves panel (server.js lobby saves; client/main.js renderLobby saveList).
struct FTCSaveRow
{
	FString Id;              // the Load button's id: String(s.id) as the browser's data-save carries it
	FString Kind;            // String(s.kind): 'auto' or 'manual' from this server
	bool bAuto = false;      // s.kind === 'auto' ("autosave" in the muted line, else "saved")
	bool bHasSavedAt = false;// savedAt gave a date (else the muted line ends in "Invalid Date")
	int64 SavedAtMs = 0;     // epoch milliseconds (Date.now() when the meta was written)
	FString Mode;            // String(s.mode): 'conquest', 'world', ... ("undefined" when absent)
	FString Minutes;         // String(s.minutes): whole minutes of play when saved
	TArray<FString> Names;   // the seats' names in seat order (AIs without ' (AI)')
	bool bWorld = false;     // s.world is truthy: World Conquest's regions are named
	FString RegionsTotal;    // String(s.world.total)
	TArray<FString> RegionsOwned; // String() of each team's region count, in the order sent
	FString Title;           // the name line: "World Conquest, 12 min, 3 / 2 of 64 regions"
	FString Detail;          // the muted line: "Ana, AI 1 · autosave Oct 12, 03:45 PM"
};

// The lobby's saves panel (client/index.html #saves).
struct FTCSaveListView
{
	bool bShown = false;     // in the lobby (state 'lobby') with at least one row
	FString Heading;         // "Saved matches"
	bool bLoadButtons = false; // the reference host gets a Load button; an unverified Rust profile keeps rows only
	FString LoadLabel;       // "Load"
	TArray<FTCSaveRow> Rows; // newest first, as the server sent them
};

// The match menu's Save match button (client/index.html #saveBtn; renderMatchMenu).
struct FTCSaveButton
{
	bool bShown = false;     // the HUD is up (a match held, room state 'play') and this client is the host
	FString Label;           // "Save match"
	FString Tip;             // "Save this match to load it again later from this room's lobby"
};

enum class ETCPersistenceCommand : uint8
{
	None,
	Save,        // { t: 'save' }
	LoadSave,    // { t: 'loadSave', id }
};
const TCHAR* LexToString(ETCPersistenceCommand Command);

// What server.js will do with a message, read from the facts this client holds. Only a prediction: the plan goes out
// as the browser sends it (or not at all when the browser would not have sent it).
enum class ETCPersistenceExpect : uint8
{
	Nothing,     // nothing goes out
	Saved,       // { t: 'saved', id } then a lobby with the row; deny save 'save' if the write fails or a save (an
	             // autosave too) is still being written
	Loaded,      // pause, lobby (play), pause by the host, start, pause; deny loadSave 'save' and a lobby if the save
	             // cannot be read (or the room started meanwhile)
	Ignored,     // the server drops it without a word (server.js 'loadSave' conditions)
};
const TCHAR* LexToString(ETCPersistenceExpect Expect);

struct FTCPersistencePlan
{
	ETCPersistenceCommand Kind = ETCPersistenceCommand::None;
	TSharedPtr<FJsonObject> Message; // the one message to send; null when nothing goes out
	FString SaveId;                  // LoadSave: the id named
	bool bCloseMenu = false;         // the save button closes the match menu whether or not the message went out
	ETCPersistenceExpect Expect = ETCPersistenceExpect::Nothing;
	FString Note;                    // this client's remark (native): why nothing goes out, or what is expected
};

// This client's record of its last save or load request (native; the browser keeps none).
enum class ETCPersistenceOutcome : uint8
{
	None,
	Pending,     // sent, no answer yet (an ignored load never gets one)
	Saved,       // 'saved' came (Id is the server's)
	Loaded,      // a start came while the load was pending (in the lobby only the host, this client, starts matches)
	Refused,     // a deny for the command came (DenyReason)
	Unanswered,  // the socket changed before an answer: the server answers the socket that asked
};
const TCHAR* LexToString(ETCPersistenceOutcome Outcome);

struct FTCPersistenceStatus
{
	ETCPersistenceCommand Request = ETCPersistenceCommand::None;
	ETCPersistenceOutcome Outcome = ETCPersistenceOutcome::None;
	FString Id;              // LoadSave: the id asked; Save: the id the server answered
	FString DenyReason;
	int32 MatchId = -1;      // Loaded: the start's matchId
	double SentAt = -1, AnsweredAt = -1;
	FString Describe() const;
};

// One feedback.show: the sentence replaces the hint line for Seconds and the error blip plays.
struct FTCPersistenceFeedback
{
	FString Sentence;
	bool bErrorSound = true;  // feedback.show always calls blip('error'), a success included
	bool bVisible = true;     // the hint is inside #hud: while the lobby hides the HUD only the sound is noticed
	double Seconds = 2.0;
};

class FTCPersistence
{
public:
	// The reference commit this module was written from.
	static const TCHAR* const ReferencePin;
	// server/saves.js and server.js constants.
	static constexpr int32 AutosavesKept = 3;        // newest 'auto' saves kept per room
	static constexpr int32 SavesKept = 10;           // newest 'manual' saves kept per room
	static constexpr int32 AutosaveMinutes = 2;      // server.js AUTOSAVE
	static constexpr int32 AutosaveTicks = 2400;     // AUTOSAVE * 60 / TICK (TICK = 1 / 20 s)

	// Texts (client/index.html, client/main.js).
	static const TCHAR* SaveLabel();       // "Save match"
	static const TCHAR* SaveTip();         // "Save this match to load it again later from this room's lobby"
	static const TCHAR* ListHeading();     // "Saved matches"
	static const TCHAR* LoadLabel();       // "Load"
	static const TCHAR* SavedSentence();   // "Match saved. Load it from this room's lobby"
	static const TCHAR* FailedSentence();  // DENY_SENTENCES.save: "The match could not be saved or loaded"
	// client/main.js MODE_NAMES[mode] ?? mode.
	static FString ModeName(const FString& Mode);
	// server/saves.js validSaveId: /^[a-z0-9]{3,12}-\d{13}-(auto|manual)$/i.
	static bool ValidSaveId(const FString& Id);
	// server.js tickRooms: whether the room saves itself (g.mode.kind 'world'). The lobby's mode says it: shared/sim.js
	// setupWorld runs exactly when the room's mode is 'world', the mode cannot change in play, and a load restores the
	// room's mode with the match. Nothing announces an autosave; its row arrives with a later lobby.
	static bool Autosaves(const FString& LobbyMode) { return LobbyMode == TEXT("world"); }
	// client/main.js when(t) in en-US with the viewer's UTC offset at that moment: "Oct 12, 03:45 PM".
	static FString SavedAtText(int64 Ms, FTimespan UtcOffset);
	// The local zone's offset from UTC at that moment (daylight saving included), from the C library's localtime.
	static FTimespan LocalUtcOffsetAt(int64 Ms);
	// Reads one lobby saves entry as renderLobby draws it. False where the browser's renderLobby would throw (an entry
	// that is not an object, names that are not an array, a truthy world without an owned array); OutWhy says which.
	static bool ReadRow(const TSharedPtr<FJsonValue>& Entry, TFunctionRef<FTimespan(int64)> UtcOffsetAt, FTCSaveRow& Out, FString* OutWhy = nullptr);

	// Message hooks, called next to FTCMatchState's (after it, so State holds this lobby's room state and host flag).
	// A lobby replaces the rows (m.saves ?? []). A malformed list keeps the rows as they were (the browser throws in
	// renderLobby before it redraws them) and sets LastMalformed.
	void ApplyLobby(const FJsonObject& M);
	// { t: 'saved', id }: the feedback sentence (the browser ignores the id; it is kept here for logs).
	void ApplySaved(const FJsonObject& M, const FTCMatchState& State, double Now);
	// { t: 'deny', cmd, reason }: only cmd 'save' and 'loadSave' concern this module (the sentence is the generic
	// refusal's, TCOrders::DenySentence; nothing is added here so it is not shown twice).
	void ApplyDeny(const FJsonObject& M, double Now);
	// A start: a pending load is answered (Loaded, with the start's matchId).
	void ApplyStart(const FJsonObject& M, double Now);
	// A new socket opened: a pending request can no longer be answered.
	void OnSocketOpened(double Now);

	// The lobby's saves panel and the match menu's Save match button.
	FTCSaveListView SaveList(const FTCMatchState& State) const;
	FTCSaveButton SaveButton(const FTCMatchState& State) const;
	const TArray<FTCSaveRow>& Rows() const { return SaveRows; }
	const FTCSaveRow* FindRow(const FString& Id) const;
	// The lobby's role: an operative in play sends nothing but fps, ping, resync and name.
	bool IsOperative() const { return Role == TEXT("operative"); }
	// The last lobby's saves list could not be read as the browser reads it (empty when it could).
	const FString& LastMalformed() const { return Malformed; }

	// The Save match click (client/main.js saveBtn.onclick): { t: 'save' } and close the menu. Nothing goes out (Note
	// says why) when the button is not drawn, or for an operative in play (sendCmd refuses; the menu still closes).
	FTCPersistencePlan PlanSave(const FTCMatchState& State) const;
	// A Load click on the row with this id (client/main.js saveList button): { t: 'loadSave', id }. Nothing goes out
	// when the panel or the button is not drawn (not the lobby, not the host) or no row has the id. Expect is Ignored
	// when server.js will drop it (an id that fails validSaveId or does not start with this room's code and '-').
	// Rust load is refused until the required R1 launch profile is verified, like Start and Restart.
	FTCPersistencePlan PlanLoad(const FTCMatchState& State, const FString& Id) const;
	// The caller sent the plan's message (bSent: the socket took it). A request that went out becomes Pending.
	void NoteSent(const FTCPersistencePlan& Plan, bool bSent, double Now);

	const FTCPersistenceStatus& Status() const { return LastRequest; }
	const FString& LastSavedId() const { return SavedId; }
	// The feedback flashes since the last call, oldest first.
	TArray<FTCPersistenceFeedback> TakeFeedback();
	// Whether the browser's hint line is on screen (the HUD is up): a match is held and the room is in play.
	static bool HudShown(const FTCMatchState& State);

private:
	TArray<FTCSaveRow> SaveRows;
	FString Role, Malformed, SavedId;
	FTCPersistenceStatus LastRequest;
	TArray<FTCPersistenceFeedback> Pending;
	bool bRustHost = false;
};

// ---------------------------------------------------------------------------------------------------------------------
// Saved preferences (CLI-124): the reference keeps each in the browser's localStorage under its own key; this client
// keeps them in one place, the user settings file (GGameUserSettingsIni, section [ThreeCrossroads]), one ini key per
// reference key. The keys this client already saved before keep their ini names and formats (EdgeScroll, PanSpeed,
// CoverPreview, Volume, Language), so nothing saved earlier is lost. Values the reference stores as strings are stored
// as the same strings ('low', 'off', '1', '95'), and read back by the reference's own rule (the Read* functions).
class THREECROSSROADS_API FTCSettingsStore
{
public:
	static const TCHAR* const Section;   // "ThreeCrossroads"
	struct FEntry
	{
		const TCHAR* Reference;  // the localStorage key ('ww2-gfx')
		const TCHAR* Ini;        // the ini key ('Graphics')
		const TCHAR* Source;     // where the reference reads and writes it
		const TCHAR* Rule;       // how the reference reads it back
	};
	static TArrayView<const FEntry> Entries();
	static const FEntry* Find(const FString& ReferenceKey);

	// The game's store over the user settings file.
	static FTCSettingsStore& Game();
	// A store over another ini file (tests); empty: memory only.
	explicit FTCSettingsStore(const FString& IniFile);

	// The raw stored value of a reference key (unset: empty optional), as the ini holds it.
	TOptional<FString> Get(const FString& ReferenceKey) const;
	// Store a value (the reference's setItem) and flush the file. False for a key not in the table or a failed write.
	bool Set(const FString& ReferenceKey, const FString& Value);
	bool Remove(const FString& ReferenceKey);
	// Writes the file now (Set does it already).
	bool Flush() const;
	const FString& File() const { return Ini; }

	// The reference's read rules, from a stored value (or none).
	// hud.js: scale.value = getItem('ww2-ui-scale') ?? 100, through <input type=range min=75 max=150 step=5>: a value
	// that is not an HTML float becomes the midpoint 112.5, then clamped to [75, 150], then snapped to 75 + 5k (a tie goes
	// up). Measured in Chromium 152 (round-14 evidence, ui-scale-chromium.txt).
	static int32 ReadUIScale(const TOptional<FString>& Stored);
	static constexpr int32 UIScaleMin = 75, UIScaleMax = 150, UIScaleStep = 5, UIScaleDefault = 100;
	static FString UIScaleLabel(int32 Percent) { return FString::Printf(TEXT("%d%%"), Percent); } // #uiScaleVal
	static const TCHAR* UIScaleTitle() { return TEXT("UI scale"); }
	static const TCHAR* UIScaleTip() { return TEXT("Size of the panels, buttons and text over the battlefield"); }
	// audio.js: parseFloat(getItem('ww2-volume')) clamped to [0, 1] when finite, else 0 if 'ww2-muted' is '1', else 0.8
	static double ReadVolume(const TOptional<FString>& Stored, const TOptional<FString>& Muted);
	// i18n.js: 'es' or 'en' when stored, else the system language (es* -> 'es'), else 'en'
	static FString ReadLanguage(const TOptional<FString>& Stored, const FString& SystemLanguage);
	// camera.js: edge on unless '0'; pan /^[012]$/ else 1
	static bool ReadEdge(const TOptional<FString>& Stored);
	static int32 ReadPan(const TOptional<FString>& Stored);
	// cover-preview.js on unless '0'; gfx.js 'low' is Low; gore on unless 'off'; main.js controls seen when '1'
	static bool ReadOnUnless(const TOptional<FString>& Stored, const TCHAR* Off) { return !Stored.IsSet() || *Stored != Off; }
	static bool ReadIs(const TOptional<FString>& Stored, const TCHAR* On) { return Stored.IsSet() && *Stored == On; }

	// This client's typed view of the keys it already saved before (their ini formats: bools as True/False).
	int32 UIScale() const { return ReadUIScale(Get(TEXT("ww2-ui-scale"))); }
	bool SetUIScale(int32 Percent);       // the slider's change: the sanitized value as its string
	// The scale the HUD draws at (hud.js --ui = value / 100)
	static float UIScaleFactor(int32 Percent) { return Percent / 100.f; }

private:
	FString Ini;
	bool bMemoryOnly = false;
	TMap<FString, FString> Memory;
};
