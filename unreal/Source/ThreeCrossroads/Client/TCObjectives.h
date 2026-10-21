// What the match is about and how it ended, as the reference client shows it (parity gaps 4 and 10):
//  - the score strip per mode, with its rate, catch-up and notice line and the weather line (client/hud.js drawScores,
//    client/endgame.js, client/weather-view.js): CLI-64, CLI-66, CLI-29;
//  - the authored capture points: ring, flag, progress arc, contested, cut and locked marks, the flip ring, the label
//    and the needs link (client/objectives.js, client/markers.js capturePoint, client/main.js startGame and the
//    minimap): CLI-78, CLI-75;
//  - the goal line (Tutorial mode.goal, scenario objectives; client/objectives.js): CLI-125, CLI-126; since round 14
//    the text, its box (objectives.css), the collapse banner and the damaged buildings' smoke are FTCScenarioText's
//    (Client/TCScenarioText.h) and GoalText here returns its text;
//  - the end of the match: the epilogue stamp, reason line, slow-down, camera glide and sounds (client/epilogue.js),
//    the lobby's result line (client/main.js renderLobby) and the match report (client/report.js): CLI-28, CLI-20,
//    CLI-21.
// A model only: no drawing, no rules. Everything comes from FTCMatchState, which is this recipient's own messages, so
// nothing here shows what the snapshot did not deliver. FTCMatchState owns one (State.Objectives) and feeds it on every
// start, snapshot, lobby and clear; the HUD and the renderer only read it.
#pragma once

#include "CoreMinimal.h"

class FJsonObject;
class FTCMatchState;
class FTCUnitTable;
struct FTCMatchResult;
struct FTCAirRow;

// Reference constants this model needs (shared/sim.js CFG, TICK; client/markers.js; client/epilogue.js).
namespace TCObjectiveRules
{
	constexpr double Tick = 1.0 / 20.0;            // TICK
	constexpr double VpToWin = 1200.0;             // CFG.vpToWin
	constexpr double CatchupMax = 6.0;             // CFG.catchupMax
	constexpr double CatchupPer = 60.0;            // CFG.catchupPer
	constexpr int32 HordeReveal = 3;               // CFG.horde.reveal
	constexpr double MunPerVp = 1.5;               // CFG.classic.munPerVp
	constexpr double PointRadius = 8.0;            // CFG.pointRadius (metres)
	constexpr double CellMetres = 2.0;             // CELL
	constexpr int32 ProgressSegments = 64;         // markers.js PROG_SEGS
	constexpr double FlipTime = 0.6;               // objectives.js FLIP_TIME
	constexpr double TagBusy = 2.5;                // markers.js: the label steps aside this long after progress moves
	constexpr double RateWindow = 8.0, RateMin = 2.0, NoticeLife = 6.0; // endgame.js
	constexpr double Slow = 0.35, EaseIn = 0.6, Glide = 2.2, View = 70.0; // epilogue.js
	constexpr int32 BunkerHpPer = 3000, HqHpPer = 3000; // UNITS.bunker / UNITS.hq hpPer at e4e579a (used without a table)
}

// One authored capture point as the world and the minimap draw it.
struct FTCCapturePoint
{
	int32 MapIndex = -1;                 // index into the start's map.points
	int32 RowIndex = -1;                 // index into the snapshot's points (the client's filtered order)
	FVector2D Centre = FVector2D::ZeroVector;      // metres
	FString Label;                       // "Radio post <middle dot> +1.5 MP/s", "<star> 2<times> VP", "+3 Mun/s"
	bool bHasLink = false;               // map.points[needs] exists: the minimap's dashed link while locked
	FVector2D LinkTo = FVector2D::ZeroVector;
	// The last delivered row (FTCPointRow) and what markers.js capturePoint set() derives from it.
	bool bHasRow = false;
	int32 Owner = -1, Capper = -1;
	double Progress = 0.0;
	bool bContested = false, bCut = false, bLocked = false;
	int32 RingSlot = -1;                 // the ring's owner colour (a seat), -1 the neutral chalk ring
	int32 FlagSlot = -1;                 // the flag's owner colour, -1 the neutral flag
	int32 ArcSlot = -1;                  // owner, else capper: the progress arc's colour
	int32 ArcSegments = 0;               // of TCObjectiveRules::ProgressSegments
	bool bArcVisible = false;            // a colour, some progress, and not a point already held in full
	bool bTagVisible = true;             // the label steps aside while the point is being taken or drained
	double FlippedAt = -1e30;            // when the owner last changed (client clock)
	double LastProgress = -1.0, BusyUntil = 0.0;
	bool CutTag() const { return bCut; }         // "Cut off: no supply"
	bool LockTag() const { return bLocked; }     // "Locked: take the linked point first"
	bool MinimapLink() const { return bLocked && bHasLink; }
};

// One frame of a point's animation (markers.js capturePoint frame, objectives.js frame).
struct FTCPointRingFrame
{
	bool bDanger = false;                // the brass inner ring while contested
	float DangerOpacity = 0.f;
	bool bFlip = false;                  // the brass ring that runs outward once the point changes hands
	float FlipOpacity = 0.f, FlipScale = 1.f;
};

// The score strip (hud.js drawScores): an optional lead block (every mode but Conquest) and one block per team.
struct FTCScoreSeat
{
	int32 Slot = -1, Faction = 0;
	FString Name;
	bool bYou = false;
	FString Held;                        // "24 pts, 2 held", "2 held", "Out", ""
	bool bHeldDanger = false;
	FString Net;                         // "Offline", "AI", "48 ms", ""
	bool bNetDanger = false;
	FString NetTitle;
};
struct FTCScoreTeam
{
	int32 Team = 0, ColourSlot = -1;     // the bar takes the first member's colour
	TArray<FTCScoreSeat> Seats;
	FString Role;                        // "Defending", "Attacking", "Horde"
	bool bBar = false;
	double Fraction = 0.0;               // 0..1 when bBar
	FString U0, Num, U;                  // the value line: "Structures left" "2 / 3"; "" "24 / 1200" "VP"
	bool bU0Danger = false;
};
struct FTCScoreLead
{
	bool bShown = false;
	FString Mode, Clock, Title, Threat;
	bool bModeDanger = false;
	bool bNextWave = false;              // the host's "Send next wave" button during a Horde break
};
struct FTCScoreStrip
{
	bool bValid = false;                 // a snapshot of this match arrived
	FString Kind;
	FTCScoreLead Lead;
	TArray<FTCScoreTeam> Teams;
	// endgame.js: the line under the strip and the banner under that
	FString Rate, Bonus, Notice;
	bool bLineShown = false;
	FString OutBanner;
	// weather-view.js, shown once a snapshot carried the weather
	bool bWeather = false, bWeatherTurning = false;
	FString Weather, WeatherTitle;
};

// endgame.js state, kept across snapshots.
struct FTCMatchNotices
{
	FString Rate, Bonus, Notice;
	bool bOut = false, bWatching = false;
	FString OutBanner() const
	{
		return bWatching ? FString(TEXT("Spectating: the whole map is in view")) : bOut ? FString(TEXT("You are out. Watching with your team's vision")) : FString();
	}
};

enum class ETCOutcome : uint8 { None, Victory, Defeat, Draw };

// epilogue.js: the first snapshot with a winner and an end starts the ending.
struct FTCEpilogue
{
	bool bActive = false;
	ETCOutcome Outcome = ETCOutcome::None;
	FString Word, Why;                   // "Victory" / "Defeat" / "Draw"; the reason line
	FVector2D EndAt = FVector2D::ZeroVector;
	double StartedAt = 0.0, Elapsed = 0.0, Speed = 1.0;
	bool bHasFrom = false;
	FVector2D From = FVector2D::ZeroVector;
	double FromDist = 0.0;
};
struct FTCEpilogueCamera
{
	double X = 0.0, Z = 0.0, Dist = 0.0;
};
// A sound the presentation should play (the reference's existing library names), with its options.
struct FTCSoundCue
{
	FString Name;
	float Delay = 0.f, Gain = 1.f, Rate = 1.f;
};

// report.js: the lobby card after a match played to the end.
struct FTCReportSide
{
	int32 Team = 0, ColourSlot = -1;
	TArray<int32> Slots;
	FString Name, Legend;                // "Player 1 & AI 2"; "Player 1: 24 victory points (won)"
	bool bWon = false;
	TArray<FVector2D> Series;            // (seconds, value) per timeline sample
};
struct FTCReportRow
{
	int32 Slot = -1;
	FString Name;
	bool bMe = false;
	int32 Values[7] = { 0, 0, 0, 0, 0, 0, 0 };
};
struct FTCMatchReport
{
	bool bShown = false;
	FString Sub;                         // "decided on victory points after 12 min 3 s", "overrun on wave 7 after 9 min"
	FString What;                        // "Victory points" or "Points held"
	double Top = 1.0, End = 1.0;         // chart value and time ranges
	FString TopLabel, EndLabel;          // "1200", "12 min 3 s"
	TArray<FTCReportSide> Sides;         // in drawing order: the winners last (on top)
	TArray<FTCReportRow> Rows;           // per seat, as the result lists them
	static const TCHAR* Column(int32 I);  // "Kills" ... "Planes downed"
};

class FTCObjectives
{
public:
	// The client clock in seconds. Tests drive it; the game uses FPlatformTime::Seconds.
	TFunction<double()> Clock;
	double Now() const;

	// ----- fed by FTCMatchState -----
	void Reset();                                        // a new match or no match: points, notices, epilogue
	void OnStart(const FTCMatchState& S);
	void OnSnapshot(const FTCMatchState& S);
	void OnLobby(const FTCMatchState& S);                // the result line, the report; a lobby resets the ending
	// endgame.js frame: an expired notice clears. Call once per frame (or any time before reading Notices).
	void Tick();

	// ----- capture points (CLI-78, CLI-75) -----
	const TArray<FTCCapturePoint>& Points() const { return CapturePoints; }
	FTCPointRingFrame RingFrame(int32 Index) const;
	// Ring geometry in metres (markers.js capturePoint): the owner ring, the progress arc's radius, the flip ring.
	static double RingRadius() { return TCObjectiveRules::PointRadius - 0.3; }
	static double ArcRadius() { return RingRadius() - 0.9; }

	// ----- score strip (CLI-64, CLI-66, CLI-29) -----
	// Table: the unit table for bunker and HQ health (null: the e4e579a values in TCObjectiveRules).
	FTCScoreStrip ScoreStrip(const FTCMatchState& S, const FTCUnitTable* Table = nullptr) const;
	const FTCMatchNotices& Notices() const { return Notice; }
	// The goal line (objectives.js): "Objective: ..." from mode.goal, else the scenario's active and complete objectives,
	// one per line with a tick or a bullet. Empty: no line. FTCScenarioText::GoalText's text (one source).
	static FString GoalText(const FTCMatchState& S);
	// The weather line and its tooltip (weather-view.js). False before a snapshot brought the weather.
	static bool WeatherLine(const FTCMatchState& S, FString& OutText, FString& OutTitle);
	// hud.js drawEcon's income line, "+4.1/s (upkeep <minus sign>0.1)". Empty when the snapshot has no income (a spectator).
	static FString EconomyLine(const FTCMatchState& S);
	// hud.js drawAir's state text for one plane: "On station 40s", "Rearming 12s", "Ready".
	static FString AirStateText(const FTCAirRow& Row);
	// shared/sim.js winVp: CFG.vpToWin * seats / teams.
	static double WinVp(const TArray<int32>& Teams);

	// ----- the end of the match (CLI-28, CLI-20, CLI-21) -----
	const FTCEpilogue& Epilogue() const { return Ending; }
	// epilogue.js frame: real seconds in, screen seconds out; moves Cam during the 2.2 s glide. Dt unchanged when no
	// ending plays.
	double EpilogueFrame(double Dt, FTCEpilogueCamera& Cam);
	// Sounds due (the epilogue's stamp, the Horde horn), oldest first; the queue is emptied. At most 16 are kept.
	TArray<FTCSoundCue> TakeSounds();
	// client/main.js renderLobby: "Match ended by the host", the Horde overrun line, "Draw", "Victory" or "<names> win(s)".
	static FString ResultLine(const FTCMatchResult& R, int32 Me);
	const FTCMatchReport& Report() const { return MatchReport; }
	static FTCMatchReport BuildReport(const FTCMatchResult& R, int32 Me);

	// JavaScript number formatting, for strings that must read as the browser's: String(n) and n.toFixed(d).
	static FString JsNumber(double V);
	static FString JsToFixed(double V, int32 Digits);

private:
	TArray<FTCCapturePoint> CapturePoints;
	FTCMatchNotices Notice;
	FTCEpilogue Ending;
	FTCMatchReport MatchReport;
	TArray<FTCSoundCue> Sounds;
	// endgame.js state
	struct FSample { double Seconds = 0.0; TMap<int32, double> Totals; };
	TArray<FSample> Samples;
	bool bHasLeader = false, bSuddenDeath = false;
	int32 Leader = 0;
	TOptional<bool> HordeOn;
	FString LastKind;
	TOptional<int32> LastTick;
	FString PendingNotice;
	double NoticeUntil = 0.0;

	void ResetNotices();
	void Announce(const FString& Text);
	void QueueSound(const TCHAR* Name, float Delay, float Gain, float Rate);
	void UpdateNotices(const FTCMatchState& S);
	void UpdateEpilogue(const FTCMatchState& S);
	void UpdatePoints(const FTCMatchState& S);
};

// Mode coverage, a verification aid (native, not in the reference; round 16): what this client showed of its match's
// mode, as LogTC "modes: ..." lines when it changed, so one live run per mode can be summed up from the log alone
// (unreal/Tools/modes/summarize.mjs). It reads only what the HUD reads (the score strip FTCObjectives::ScoreStrip, the
// goal box FTCScenarioText::GoalText, the 'say' lines as the alert feed words them, the winner, the end and the
// epilogue stamp) and changes nothing.
//   modes: start: mode <kind> on '<map>' (<w>x<h>), seat <you> of <n> (team <t>), teams [...], lobby mode <m>, <k> authored point(s)
//   modes: strip: <lead mode> <clock> - <title> [<threat>] | team <t> <role> <u0> <num> <u> (<names>) | ... [| line: ...]
//            when its labels change (not its clock or numbers), and every 60 s of match time
//   modes: goal: <text>                      the goal box, when it changes ("modes: goal: (none)" when it goes)
//   modes: say at tick <n>: <text> [| shown: <the reader's language>]
//   modes: decided at tick <n>: winner team <w> (<outcome for this seat>), reason <r>; stamp '<word>' '<why>'
//   modes: summary (<why>): mode <kind>, <snapshots> snapshot(s), strip lines <a>, goal lines <b>, say lines <c>, decided <...>
class FTCModeWatch
{
public:
	static constexpr int32 PeriodTicks = 1200;   // a full strip line at least every 60 s of match time (20 ticks a second)
	void Reset();
	void OnStart(const FTCMatchState& S);
	// After FTCMatchState::ApplySnapshot. Raw: the same message (its 'say' shots' text and localized), or null.
	void OnSnapshot(const FTCMatchState& S, const FTCUnitTable* Table, const FJsonObject* Raw);
	// The match left the screen (a lobby, a leave, a new room). Logs the summary once per match.
	void OnClear(const TCHAR* Why);
	TArray<FString> TakeLog() { TArray<FString> Out = MoveTemp(Pending); Pending.Reset(); return Out; }

	bool Started() const { return bStarted; }
	int32 StripLines() const { return Strips; }
	int32 GoalLines() const { return Goals; }
	int32 SayLines() const { return Says; }
	bool Decided() const { return bDecided; }
	const FString& Kind() const { return ModeKind; }

	// One line of the strip as drawn (English, the HUD translates when it draws): lead, teams, the notice line.
	static FString StripText(const FTCScoreStrip& Strip, const FTCMatchState& S);
	// The strip's labels without the numbers that move every second (clock, VP, health): a change here is a new line.
	static FString StripShape(const FTCScoreStrip& Strip);

private:
	bool bStarted = false, bDecided = false, bSummarized = false;
	int32 MatchId = -1, Snapshots = 0, Strips = 0, Goals = 0, Says = 0, LastStripTick = -1000000000;
	FString ModeKind, LastShape, LastGoal, DecidedText;
	TArray<FString> Pending;
};
