// The Tutorial (parity gap 11: CLI-125, CLI-127, and the Tutorial entry of CLI-01 and CLI-04), ported from the pinned
// reference (reference/ww2-rts at TCTutorialData::ReferencePin):
//   client/public-lobby.js, .html  the Tutorial button ("New here? Learn the basics in about ten minutes"): a new room
//                                  with a random code (six random bytes in hex) and the listing { public: false, title:
//                                  'Tutorial', tutorial: true }, which client/main.js sends in its hello; server.js then
//                                  makes a private Tutorial room on the tutorial map and starts it once its player is in;
//   shared/tutorial.js             STEPS, Sergeant Hollis's script (goal, the cell the lines point to, the lines, the
//                                  manpower, the enemies spawned), written out by Tools/tutorial/export-tutorial.mjs.
//                                  The client never decides a step: the server's snapshot says mode.step, mode.steps and
//                                  mode.goal, and its 'say' shots carry the lines (client/alerts.js shows them as
//                                  'event' alerts with a ping and a jump; FTCAlerts does that here). The table is used to
//                                  check what arrives against the reference script (FTCTutorialProgress) and by the
//                                  verification driver below; nothing in it is shown in place of the server's words;
//   client/main.js                 receiveStart: the Controls sheet opens at a match start (not for an operative) until
//                                  it has been dismissed once (localStorage 'ww2-controls-seen'); while it opens that way
//                                  a pointer press anywhere on it closes it and is swallowed (CLI-127).
// What the HUD shows for it lives in FTCObjectives (the score strip's "Tutorial" and "n / 9", hud.js drawScores) and
// FTCScenarioText (the "Objective: ..." box, objectives.js and objectives.css).
//
// Native, not in the reference:
//   - FTCTutorialProgress logs the steps and lines as they arrive and checks them against the script (a verification
//     aid: the browser has no such log);
//   - FTCControlsOnboarding stores 'ww2-controls-seen' as [ThreeCrossroads] ControlsSeen=1 in GameUserSettings.ini (the
//     per-user settings the language and volume use); an unattended run (automation, -unattended verification runs) does
//     not open it unless -TCOnboarding asks, so earlier verification captures keep their view;
//   - FTCTutorialDriver (the -TCTutorialDrive verification switch) plays the Tutorial from this seat's own snapshot only
//     (its own rows, the enemy rows it is sent, mode.step, its cooldowns), sending the browser's order messages: it is
//     the port of the verification policy Tools/tutorial/policy.mjs, not a reference module.
#pragma once

#include "CoreMinimal.h"
#include "Dom/JsonObject.h"

class FTCMatchState;
class FTCUnitTable;

// shared/tutorial.js spawn: [type, x, y, { hp, go, house }]
struct FTCTutorialSpawn
{
	FString Type;
	FIntPoint At = FIntPoint::ZeroValue;
	double Hp = 0.0;                     // share of full strength, 0: full
	bool bGo = false;                    // attack-move to Go
	FIntPoint Go = FIntPoint::ZeroValue;
	bool bHouse = false;                 // garrison the house at House
	FIntPoint House = FIntPoint::ZeroValue;
};

// One step of shared/tutorial.js STEPS (its done() is the server's).
struct FTCTutorialStep
{
	FString Goal;
	FIntPoint At = FIntPoint::ZeroValue; // the cell its lines point to (the alert's ping and jump)
	TArray<FString> Say;
	int32 Mp = 0;                        // manpower every player of the side gets at least
	TArray<FTCTutorialSpawn> Spawn;
};

namespace TCTutorialData
{
	extern const TCHAR* const ReferencePin;
	constexpr double Cell = 2.0;         // CELL
	const TArray<FTCTutorialStep>& Steps();
	// tutorial.js cell([x, y]): the centre of a cell in metres
	FVector2D CellCentre(FIntPoint C);
	// The step whose goal this is, -1 when none
	int32 StepOfGoal(const FString& Goal);
	// The step and line index of a line of the script, false when it is not one
	bool FindLine(const FString& Text, int32& OutStep, int32& OutLine);
}

// The lobby's Tutorial entry.
namespace TCTutorialEntry
{
	// client/public-lobby.html #tutorial: its text and title
	const TCHAR* Label();                // "Tutorial"
	const TCHAR* Title();                // "New here? Learn the basics in about ten minutes"
	// client/public-lobby.js code(): six random bytes in lower-case hex (12 characters, a valid room code)
	FString NewRoomCode();
	// client/public-lobby.js tutorial onclick: { public: false, title: 'Tutorial', tutorial: true }
	TSharedRef<FJsonObject> Listing();
	// server.js hello: msg.listing?.tutorial === true
	bool IsTutorialListing(const FJsonObject* Listing);
	// -TCTutorial: a new private Tutorial room, as the button makes, unless -TCRoom= names the room. Fills the room and
	// the listing hello will carry; false (nothing changed) without the switch.
	bool FromCommandLine(const TCHAR* Cmd, FString& InOutRoom, TSharedPtr<FJsonObject>& OutListing);
}

// What the snapshots said about the Tutorial, as it arrived.
struct FTCTutorialStepSeen
{
	int32 Step = -1, Steps = 0;
	FString Goal;
	int32 Tick = 0;                      // the snapshot that first carried this step
	bool bReferenceGoal = false;         // the goal is the script's for this step
	TArray<FString> Said;                // the 'say' lines that arrived while this step showed (English, the shot's text);
	                                     // a step's lines leave one tick after it begins, so a one-tick step's lines come
	                                     // under the next step (LinesOf keeps them by the step they belong to)
	TArray<FString> Shown;               // the same lines as the alert feed draws them (localized[lang] ?? text, TC_T)
};

class FTCTutorialProgress
{
public:
	void Reset();
	void OnStart(const FTCMatchState& S);
	// After FTCMatchState::ApplySnapshot. Raw: the same snapshot message (its say shots' text and localized).
	void OnSnapshot(const FTCMatchState& S, const FJsonObject* Raw);

	bool IsTutorial() const { return bTutorial; }
	int32 Step() const { return CurStep; }
	int32 Steps() const { return CurSteps; }
	const FString& Goal() const { return CurGoal; }
	const TArray<FTCTutorialStepSeen>& Seen() const { return History; }
	bool IsFinished() const { return bFinished; }
	int32 Winner() const { return WinnerTeam; }
	const FString& EndReason() const { return Reason; }
	// Lines for the log since the last call (step changes, lines, the end), oldest first.
	TArray<FString> TakeLog() { TArray<FString> Out = MoveTemp(Pending); Pending.Reset(); return Out; }
	// The script lines that arrived for one script step, in arrival order (a line is the script's when its text is).
	const TArray<FString>& LinesOf(int32 Step) const;
	// The whole run against the script: the steps the snapshots showed in increasing order up to the last, each goal the
	// script's, every step's lines once and in order, nothing else said, won with reason 'tutorial'. A step that begins
	// and ends between two snapshots (the server sends one every few ticks) is never shown: the browser never shows it
	// either; its lines still arrive, and the verdict names it.
	FString Verdict() const;
	bool MatchesScript() const;
	int32 LinesSaid() const;
	TArray<int32> StepsNotShown() const;

private:
	bool bTutorial = false, bFinished = false;
	int32 CurStep = -1, CurSteps = 0, WinnerTeam = -2, MatchId = -1;
	FString CurGoal, Reason;
	TArray<FTCTutorialStepSeen> History;
	TArray<TArray<FString>> ScriptLines; // by script step
	TArray<FString> OtherLines;          // 'say' lines that are not the script's
	TArray<FString> Pending;
	TOptional<int32> LastTick;
};

// CLI-127: the Controls sheet at the first match.
class FTCControlsOnboarding
{
public:
	static const TCHAR* const SettingsSection; // ThreeCrossroads
	static const TCHAR* const SettingsKey;     // ControlsSeen (localStorage 'ww2-controls-seen')
	// Storage (tests replace these; the game reads and writes GGameUserSettingsIni).
	TFunction<bool()> ReadSeen;
	TFunction<void()> WriteSeen;
	// An unattended run opens it only when asked (-TCOnboarding); tests set this.
	TOptional<bool> AllowUnattended;
	// main.js receiveStart: true when the sheet should open now as onboarding (not an operative, not seen yet).
	bool OnStart(bool bOperative);
	bool IsOnboarding() const { return bOnboarding; }
	// The sheet opened another way (F1): no onboarding.
	void OnOpenedByHand() { if (!bOnboarding) bManual = true; }
	// controlsSheet pointerdown while onboarding: close it and swallow the press. True: the caller closes the sheet
	// and drops this press (and its release).
	bool OnPointerDown();
	// The sheet closed (any way: the press above, F1, Esc, the close button). Onboarding stores 'seen'.
	void OnClosed();
	bool Seen() const;

private:
	bool bOnboarding = false, bManual = false;
};

// -TCTutorialDrive: plays the Tutorial from this seat's snapshot (see the file comment).
class FTCTutorialDriver
{
public:
	// The commands to send after this snapshot (empty when there is nothing to do, or this is not a Tutorial in play).
	TArray<TSharedRef<FJsonObject>> Decide(const FTCMatchState& S, const FTCUnitTable* Table);
	void Reset() { Memo = FMemo(); Tally.Reset(); TallyRefused.Reset(); }
	int32 Sent() const { return Memo.Count; }
	// The step the last Decide saw (-2 before the first).
	int32 Step() const { return Memo.Step; }
	// Verification tally of what this seat really sent, by message type: Note() each command after the send, then
	// TakeTally() gives "sent 3 (move 2, buy 1); not sent 1 (amove 1)" for the commands noted since the last call ("" for
	// none). The mode driver logs it per Tutorial step, so a run shows a seated client walking the steps itself.
	void Note(const FString& Type, bool bSent) { (bSent ? Tally : TallyRefused).FindOrAdd(Type)++; }
	FString TakeTally();

private:
	TMap<FString, int32> Tally, TallyRefused;
	struct FMemo
	{
		int32 Step = -2, Count = 0;
		int32 SentAt = -1000000000, StepAt = 0;
		bool bBuy = false, bPlace = false, bRetreat = false;
		TMap<int32, int32> Nade;         // unit id -> the tick its grenade was asked
	} Memo;
};
