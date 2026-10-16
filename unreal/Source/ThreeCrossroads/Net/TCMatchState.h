// What this client holds of the match, decoded from the reference WebSocket protocol (reference/ww2-rts server.js and
// shared/sim.js snapshotFor/unitDelta/terrainFor/fogFor). Recipient-visible data only: no rules run here.
//
// Unit rows have two possible sources (TC_DECODER, Net/TCRustViewShadow.h). With no unit authority attached (cpp and
// shadow) the rows below are this file's own decode. With TC_DECODER=rust the Rust recipient view (ww2_view_*) is
// attached: ApplySnapshot still decodes the rows itself (for the tails ABI 2 does not expose, and for the comparison),
// then replaces Units with the view's rows and returns the view's held-set verdict. Everything else (map, cells, fog,
// start and snapshot fields, lobby) is this file's decode in every mode (the field-ownership table in DESIGN.md,
// "Unreal client: the Rust view as the unit source").
#pragma once

#include "CoreMinimal.h"
#include "Client/TCObjectives.h"

class FJsonObject;
class FJsonValue;
class FTCRustViewShadow;

// One unit snapshot row (shared/sim.js UNIT_ROW): id, type, owner, x, z, rot, aim, hp, supp, target, cover, cd, flags,
// stars, built, then the tail (ground vehicles: speed, vx, vz, travel direction; a digging squad: progress).
struct FTCUnit
{
	int32 Id = 0;
	FString Type;
	int32 Owner = -1;
	float X = 0.f, Z = 0.f, Rot = 0.f, Aim = 0.f;
	int32 Hp = 0, Supp = 0, Target = 0, Cover = 0, Cd = 0, Flags = 0, Stars = 0;
	float Built = 1.f;
	TArray<float> Tail;
};

// Reference unit flags read by the client (shared/sim.js unitFlags).
namespace TCFlags
{
	constexpr int32 Retreating = 1;
	constexpr int32 Garrisoned = 32;
	constexpr int32 Riding = 262144;
	constexpr int32 Operative = 131072;
}

// One seat in the room as the lobby lists it (server.js lobby players).
struct FTCLobbyPlayer
{
	FString Name, Level;
	int32 Team = 0;
	bool bAi = false, bConnected = false;
};

// One cell's structural section as a cell row's data carries it (shared/sim.js cell data: section { id, structureId,
// state, hp, maxHp, material, anchor }). client/main.js applyCells keeps it per cell (terrain.structuralSections.set),
// replacing it only when a later row brings a section again; a start empties the map.
struct FTCCellSection
{
	FString Id, StructureId, State, Material;
	double Hp = 0.0, MaxHp = 0.0;
	bool bHasHp = false, bHasMaxHp = false, bAnchor = false;
	// shared/sim.js failWorldSection sets state 'failed' and hp 0 together; either one read here counts as down
	bool IsDown() const { return State == TEXT("failed") || (bHasHp && Hp <= 0.0); }
};

// A cell row's data besides the section (client/main.js applyCells: groundGrid, objectGrid, materials).
struct FTCCellData
{
	FString Ground, Object, Material;
};

struct FTCMap
{
	FString Name;
	FString Mood;               // client/moods.js: authored map mood, after weather and before name inference
	int32 W = 0, H = 0;
	bool bWorld = false;          // World Conquest: only cells this player's side has discovered are known
	TArray<uint8> Chars;          // the cell's terrain character, W * H
	TArray<int8> Levels;          // height level per cell (CFG.levelHeight metres each)
	TBitArray<> Known;            // cells this client may draw: every cell of an authored map, discovered World cells
	bool IsValidCell(int32 C) const { return C >= 0 && C < W * H; }
};

// ---------------------------------------------------------------------------------------------------------------------
// Snapshot and start fields beyond units, cells and fog (parity gap 3: CLI-64, 66 to 68, 75 to 81, 109 to 113, 125,
// 126; gap 10: CLI-20, 21, 28). Each is decoded the way the reference client reads it (client/main.js applySnapshot,
// hud.js, endgame.js, epilogue.js, objectives.js, weather-view.js, projectiles.js, debris.js), from this recipient's
// own message only: nothing here is inferred or filled in from another seat's view. A field the message does not carry
// reads as the reference reads a missing field (noted per field). Every list is capped (TCMatchLimits) and every field
// is dropped by ApplyStart and ClearMatch.

namespace TCMatchLimits
{
	constexpr int32 Seats = 64;          // per-seat lists (vp, online, ping, out, names in a result)
	constexpr int32 Points = 256;        // capture points (start map.points and snapshot points)
	constexpr int32 Plans = 8192;        // own unit plans and waiting orders
	constexpr int32 PlanPath = 512;      // waypoints kept per plan
	constexpr int32 QueuedOrders = 64;   // waiting orders kept per unit
	constexpr int32 Ghosts = 4096;
	constexpr int32 Shots = 2048;        // shot events per snapshot (they are not carried over)
	constexpr int32 Flights = 2048;
	constexpr int32 Fires = 65536;
	constexpr int32 Falling = 1024;
	constexpr int32 Air = 512;
	constexpr int32 FoundMines = 65536;
	constexpr int32 Objectives = 64;     // scenario objectives
	constexpr int32 Text = 1024;         // characters kept of any free text (goal, objective, name, reason)
	constexpr int32 Timeline = 4096;     // samples of a result's timeline
}

// start map.points[i] (the authored capture points): x, y in cells, kind ('radio', 'depot' or none), mp and vp per
// second (the browser's defaults when absent: 1), needs (an index into map.points, -1 none).
struct FTCMapPoint
{
	int32 X = 0, Y = 0;
	FString Kind;
	double Mp = 1.0, Vp = 1.0;
	int32 Needs = -1;
};

// snapshot points[i]: [owner, capper, progress, contested, cut, locked] (shared/sim.js snapshotFor points). Contested is
// set by the server only when two teams' on-point units are visible to this recipient.
struct FTCPointRow
{
	int32 Owner = -1, Capper = -1;
	double Progress = 0.0;
	bool bContested = false, bCut = false, bLocked = false;
};

// snapshot plans: [id, kind, target x, target z, ...path x, z] for this seat's own non-structure units (client/main.js:
// kept only for a unit held). Kinds (shared/sim.js planOf, client/markers.js PLAN_COLORS): 0 none, 1 move, 2 attack-move,
// 3 retreat, 4 attack, 5 fire at, 6 ability, 7 dig, 8 build, 9 board or enter.
struct FTCPlan
{
	int32 Id = 0, Kind = 0;
	double Tx = 0.0, Tz = 0.0;
	TArray<FVector2D> Path;
};

// snapshot orders: [id, count, kind, x, z, ...] waiting orders of this seat's own units (client/main.js: only a held
// unit whose owner is this seat, and at most count entries that are complete).
struct FTCQueuedOrder
{
	int32 Kind = 0;
	double X = 0.0, Z = 0.0;
};
struct FTCWaitingOrders
{
	int32 Id = 0;
	TArray<FTCQueuedOrder> Orders;
};

// snapshot ghosts: [id, type, owner, x, z, built], enemy buildings this side remembers under fog (construction modes).
// client/main.js applyGhosts skips an id that is also a live row.
struct FTCGhost
{
	int32 Id = 0, Owner = -1;
	FString Type;
	double X = 0.0, Z = 0.0, Built = 1.0;
};

// One delivered shot event (snapshot shots[]). The server removes f / t (and fo / to) when this recipient cannot see the
// shooter or target, so the bHas flags say what was delivered; nothing is reconstructed. Fields the client uses: k, kind,
// effect, material, x, y, z, f, fo, t, to, flight, seq, time, at, kill, terminal, hit, explosive, local, m (the man an
// operative's bullet hit), name and id (a collapse), v, gravity, until, expires, blast.
struct FTCShot
{
	FString K, Kind, Effect, Material, Name, Id;
	bool bHasPos = false, bHasY = false;
	double X = 0.0, Y = 0.0, Z = 0.0;
	bool bHasShooter = false, bHasShooterOwner = false, bHasTarget = false, bHasTargetOwner = false;
	int32 F = 0, Fo = -1, T = 0, To = -1;
	int32 Flight = 0, Seq = -1, M = -1;
	double Time = 0.0, At = 0.0, Until = 0.0, Expires = 0.0, Gravity = 0.0, Blast = 0.0;
	FVector V = FVector::ZeroVector;
	bool bKill = false, bTerminal = false, bHit = false, bExplosive = false, bLocal = false;
};

// snapshot flights[]: a projectile in flight where this recipient's team sees it (shared/sim.js flightsFor). f / fo only
// when the shooter is seen. client/projectiles.js extrapolates from (x, y, z, v, gravity, time) up to until / expires.
struct FTCFlight
{
	int32 Flight = 0, Seq = 0;
	FString Kind, Effect;
	double X = 0.0, Y = 0.0, Z = 0.0, Gravity = 0.0, Time = 0.0, Launched = 0.0, Expires = 0.0, Until = 0.0;
	FVector V = FVector::ZeroVector;
	bool bHasShooter = false;
	int32 F = 0, Fo = -1;
};

// snapshot falling[]: a building section coming down where this side sees it (shared/debris-motion.js debrisRow plus
// section, at, until, impulse). client/debris.js interpolates x, y, z and tilt between delivered poses.
struct FTCFalling
{
	FString Id, Kind, Material, Section;
	double X = 0.0, Y = 0.0, Z = 0.0, Dir = 0.0, Tilt = 0.0, Mass = 0.0, Gravity = 0.0, Drag = 0.0, Time = 0.0, Born = 0.0;
	double At = 0.0, Until = 0.0, Impulse = 0.0;
	FVector V = FVector::ZeroVector;
};

// snapshot air: [id, state, fuel s, ammo, rearm s] for this seat's own planes (hud.js drawAir keeps the held ones).
// State: 0 at base, 1 flying out, 2 on station, 3 heading home, 4 rearming.
struct FTCAirRow
{
	int32 Id = 0, State = 0, Fuel = 0, Ammo = 0, Timer = 0;
};

// Public weather: [now] or, in the last 10 s before it turns, [now, next, seconds left] (shared/weather.js weatherRow).
struct FTCWeatherRow
{
	FString Now = TEXT("clear"), Next;
	int32 SecondsLeft = 0;
	bool HasNext() const { return !Next.IsEmpty(); }
};

// snapshot wx: [rain 0-1, how wet the ground is 0-1, wind direction, wind strength 0-1].
struct FTCWx
{
	bool bKnown = false;
	double Rain = 0.0, Wet = 0.0, WindAngle = 0.0, WindStrength = 0.0;
};

// snapshot mode (shared/sim.js modeRow); absent in Conquest. Optional numbers keep "absent" apart from 0.
struct FTCModeRow
{
	bool bPresent = false;
	FString Kind = TEXT("conquest");
	TOptional<int32> DefenderTeam, AttackerTeam, TimeLeft, Total, Wave, Left, Slot, Step, Steps;
	bool bSuddenDeath = false, bActive = false;
	TArray<int32> Bunkers;            // per team (annihilation)
	FString Profile, NextProfile, Goal;
	bool bBoss = false;
	int32 BossHp = 0, BossMax = 0;    // Horde's Kaiju bar [hp, max]
};

// snapshot end: { reason, x, z } once there is a winner ('vp', 'bunkers', 'structures', 'timer', 'hq', 'tutorial',
// 'world', 'nations' or 'draw').
struct FTCEndRow
{
	bool bPresent = false;
	FString Reason;
	double X = 0.0, Z = 0.0;
};

// The few World Conquest facts the score strip reads (hud.js drawScores): owned, goal, total and each region's team.
struct FTCWorldScore
{
	bool bPresent = false;
	TOptional<int32> Owned, Goal, Total;
	TArray<int32> RegionTeams;        // -1 for a region with no team
};

// One scenario objective delivered to this recipient (shared/scenarios.js scenarioFor; client/objectives.js reads
// state and text, English).
struct FTCScenarioObjective
{
	FString Id, State, Text;
};

// The lobby's result (server.js finishMatch / holdEnding; shared/story.js storyResult): a host-ended match carries only
// ended; a decided one the winner, teams, names, reason, at, endTick, story, timeline, winVp, mode, maybe horde, and
// you (this seat's own outcome).
struct FTCStoryRow
{
	bool bPresent = false;            // null for a seat added after the match
	int32 Kills = 0, Losses = 0, Built = 0, Captures = 0, MpSpent = 0, SupportCalls = 0, PlanesDowned = 0;
};
struct FTCTimelineSample
{
	double T = 0.0;
	TArray<double> Vp, Points;        // per team; Vp in Conquest, Points (held) in the other modes
};
struct FTCMatchResult
{
	bool bPresent = false, bEnded = false;
	bool bHasWinner = false;
	int32 Winner = -1;                // a team id, -1 a draw
	TArray<int32> Teams;              // per seat; -1 for null (a seat added after the match)
	TArray<FString> Names;
	FString Reason, Mode;
	bool bHasAt = false;
	FVector2D At = FVector2D::ZeroVector;
	int32 EndTick = 0;
	bool bHasStory = false;
	TArray<FTCStoryRow> Story;
	TArray<FTCTimelineSample> Timeline;
	TOptional<double> WinVp;
	bool bHorde = false;
	int32 HordeWave = 0, HordeKills = 0, HordeTime = 0;
	bool bHordeRecord = false, bHordeBest = false;
	int32 HordeBestWave = 0;
	bool bHasYou = false;
	FString YouOutcome;               // 'victory', 'defeat' or 'draw'
	int32 YouTeam = -1;
};

class FTCMatchState
{
public:
	// The reference constants the client needs to place things (shared/sim.js CELL, CFG.levelHeight).
	static constexpr float CellMetres = 2.f;
	static constexpr float LevelMetres = 2.5f;

	// lobby
	FString RoomCode, RoomState, Mode;
	bool bAmHost = false, bSpectator = false;   // bSpectator may be seeded from -TCSpectate before the first lobby
	int32 LobbySeat = -1, LobbyPlayers = 0;
	// A lobby arrived on the current socket. A fresh client joining a running match gets its start before its lobby
	// (server.js hello: lobby() awaits listMaps(), sendStart does not), and a spectator's start names seat 0.
	bool bLobbySinceOpen = false;
	FString LastResult;                         // the lobby's result line (client/main.js renderLobby), empty when none
	FString LobbyMap;                           // the lobby's mapName ('world' for World Conquest)
	bool bLobbySandbox = false;                 // the room's Sandbox switch (server.js 'sandbox')
	// Round 17 (hudui): server.js lobby() hordeBest, the room's Horde record for this map, team size and army size
	// (recordOf: { wave, kills, time in s, names }), null outside Horde or with no record yet; main.js renderLobby
	// writes it after the mode's description. bSet false: null.
#define TC_HAS_LOBBY_HORDE_BEST 1
	struct FLobbyHordeBest { bool bSet = false; int32 Wave = 0; int32 Time = 0; TArray<FString> Names; };
	FLobbyHordeBest LobbyHordeBest;
	TArray<FTCLobbyPlayer> Roster;              // the seats, in seat order
	TArray<FString> Spectators;

	// match
	bool bHasMatch = false;
	int32 MatchId = -1, You = -1, Tick = 0, Winner = -2, Mp = 0;
	bool bSandbox = false;
	FVector2D Spawn = FVector2D::ZeroVector;
	TArray<FString> Names;
	TArray<int32> Teams;
	FTCMap Map;
	bool bFogLifted = false;      // a spectator of an authored map gets no fog mask
	TArray<uint8> FogVisible, FogExplored;
	TMap<int32, FTCUnit> Units;
	// Ids the server says this client holds whose row could not be read: they count toward the held check (so one bad
	// row does not ask for a resend every second) but are not drawn. Never also in Units.
	TSet<int32> UnreadableIds;
	bool bPaused = false;
	FString PauseText, LastDeny;
	// The last refusal (server.js: { t: 'deny', cmd, reason }), and how many came this match.
	FString DenyCmd, DenyReason;
	int32 Denies = 0;
	int32 Snapshots = 0, Resyncs = 0;

	// What changed since the presentation last looked: cells whose terrain or knowledge changed, and fog cells that
	// flipped. The presentation clears them after reading.
	TSet<int32> DirtyCells;
	TSet<int32> FogDirtyCells;
	bool bMapReset = false;

	// ----- cell rows beyond the character and level (client/main.js applyCells) -----
	// terrain.state[cell] = st ?? 0 for every cell a row named (a Uint8Array: the value wraps modulo 256). Cells no row
	// named keep the map's start state, which shared/sim.js startState derives and this decoder does not (absent here).
	TMap<int32, uint8> CellStates;
	// terrain.groundGrid / objectGrid / materials for cells whose row carried data
	TMap<int32, FTCCellData> CellData;
	// terrain.structuralSections: the last section a row brought per cell (kept when a later row has none)
	TMap<int32, FTCCellSection> Sections;
	// The cells the last start or snapshot applied, in row order (valid rows only), and rows it skipped as unreadable.
	TArray<int32> LastCellRows;
	int32 LastCellRowsSkipped = 0;

	// ----- start fields beyond the map (server.js sendStart) -----
	TArray<int32> Factions;                     // per seat
	TArray<TOptional<FVector2D>> Spawns;        // per seat; World sends null for another team's spawn
	TArray<FTCMapPoint> MapPoints;              // map.points, as authored (unfiltered)
	FString StartMode;                          // the lobby mode known when this start arrived (client/main.js lobbyState.mode)

	// ----- snapshot fields (shared/sim.js snapshotFor, server.js sendSeat extras) -----
	TArray<FTCPointRow> Points;                 // absent: none (objectives.js s.points ?? [])
	TArray<int32> Vp;                           // per seat; absent: none (hud.js s.vp?.[i] ?? 0)
	TArray<TOptional<bool>> Online;             // per seat; absent or not a boolean: unknown
	TArray<TOptional<int32>> Ping;              // per seat round trip in ms, -1 an AI; null: unknown
	TArray<bool> OutSeats;                      // per seat
	TArray<FTCPlan> Plans;                      // own units held, every snapshot (spectators get none)
	TArray<FTCWaitingOrders> WaitingOrders;     // own units held
	TArray<FTCGhost> Ghosts;                    // absent: none (client/main.js applyGhosts(list ?? []))
	TArray<FTCShot> Shots;                      // this snapshot's events only
	TArray<FTCFlight> Flights;
	TArray<int32> Fires;                        // burning cells (valid cell indexes only)
	TArray<FTCFalling> Falling;
	FTCWeatherRow Weather;                      // from the start, then each snapshot that carries an array (else kept)
	bool bWeatherFromSnapshot = false;          // weather-view.js shows its line only once a snapshot brought weather
	FTCWx Wx;                                   // kept when a snapshot does not carry it (main.js: if (s.wx))
	TOptional<double> Inc, FuelInc, Upkeep;     // income per second and upkeep; absent for spectators
	TOptional<int32> Mun, Fuel;                 // construction modes only
	TArray<FTCAirRow> Air;                      // own planes held
	FTCModeRow ModeRow;
	FTCWorldScore WorldScore;
	TArray<FTCScenarioObjective> Scenario;      // absent: none
	bool bHasScenario = false;
	TArray<int32> FoundMines;                   // enemy mine cells this team's builders found (valid cells only)
	double ArmyIncome = 1.0;                    // snapshot army.income (endgame.js: ?? CFG.armies.standard.income)
	FTCEndRow End;
	// How many entries a cap dropped this match (TCMatchLimits), and malformed entries skipped. Logged once each.
	int32 DroppedOverCap = 0, SkippedMalformed = 0;

	// ----- lobby result (server.js lobby: result) -----
	FTCMatchResult LastMatch;                   // the lobby's result, while one is shown (null clears it)

	// Score strip, capture points, notices, the epilogue and the result report, kept current by ApplyStart,
	// ApplySnapshot, ApplyLobby and ClearMatch. The HUD and the renderer read it (Client/TCObjectives.h).
	FTCObjectives Objectives;

	// Room state. A lobby that is not in play ends any match held (ClearMatch), as the browser hides the match.
	void ApplyLobby(const FJsonObject& M);
	// Drops everything about the match (units, map, fog, pause, winner, deny), keeping MatchId so a later start can
	// still tell whether it is the same match. Sets bMapReset so the presentation clears its ground too.
	void ClearMatch();
	// A new socket opened: the seat is unconfirmed until its lobby arrives.
	void OnSocketOpened() { bLobbySinceOpen = false; }
	void ApplyPause(const FJsonObject& M);
	void ApplyDeny(const FJsonObject& M);
	// A start replaces everything held. False when the message is malformed (nothing is changed then).
	bool ApplyStart(const FJsonObject& M);
	// Returns true when the held units disagree with the server's held [count, xor of ids]: ask for a resend. Under
	// TC_DECODER=rust the verdict is the Rust view's (ww2_view_needs_resync), or true when the view refused this
	// snapshot (both decoders then realign from the full resend); see SetUnitAuthority.
	bool ApplySnapshot(const FJsonObject& M);

	// TC_DECODER=rust: the Rust view becomes the source of Units (FTCRustViewShadow::Begin(State) attaches it, End and
	// its destructor detach it). Null: this decoder's own rows. The authority must have applied the same message, as
	// received, before ApplyStart or ApplySnapshot runs (FTCConnection::OnText comes before OnMessage).
	void SetUnitAuthority(FTCRustViewShadow* Authority) { UnitAuthority = Authority; }
	FTCRustViewShadow* GetUnitAuthority() const { return UnitAuthority; }
	// Where the last snapshot's Units came from: false this decoder, true the Rust view.
	bool UnitsFromRustView() const { return bUnitsFromRust; }

	// The C++ decoder's narrowing of a wire number into an int32 column: truncation toward zero inside int32's range,
	// clamped outside it (0 for NaN). The Rust rows go through the same narrowing, so both sources give equal FTCUnits.
	static int32 NarrowInt(double V);
	// An FTCUnit from the 15 fixed columns (id, owner, x, z, rot, aim, hp, supp, target, cover, cd, flags, stars, built
	// as doubles, and the type), narrowed as the C++ decoder narrows them. The tail is left empty.
	static void FillUnit(FTCUnit& Out, const FString& Type, const double Columns[15]);

	// This client commands seat You: a lobby on this socket names the same seat and no spectator flag, or (a reconnect,
	// whose start comes before its lobby) a lobby already did so for this very match and seat. The server always gives
	// the same match's seat back to its token (server.js hello matches the token first; seats are only freed in the
	// lobby), so only a fresh client or a new match waits for the lobby.
	bool SeatConfirmed() const
	{
		return bHasMatch && You >= 0 && !bSpectator && LobbySeat == You && (bLobbySinceOpen || (ConfirmedMatch == MatchId && ConfirmedSeat == You));
	}
	bool IsOwn(const FTCUnit& U) const { return SeatConfirmed() && U.Owner == You; }
	// client/main.js foe: a seat on another team than this client's ((teams[slot] ?? slot) !== (teams[me] ?? me)).
	// Only meaningful once the seat is confirmed: nothing is a foe to a spectator or an unconfirmed seat.
	bool IsFoe(int32 Owner) const
	{
		if (!SeatConfirmed()) return false;
		const int32 Mine = Teams.IsValidIndex(You) && Teams[You] >= 0 ? Teams[You] : You;
		return (Teams.IsValidIndex(Owner) && Teams[Owner] >= 0 ? Teams[Owner] : Owner) != Mine;
	}
	// Orders would be acted on: the server silently drops them while paused, after a winner, and outside play.
	bool CanOrder() const { return SeatConfirmed() && RoomState == TEXT("play") && !bPaused && Winner < -1; }
	// client/main.js teams[slot] ?? slot (a start without teams makes every seat its own team).
	int32 TeamOf(int32 Slot) const { return Teams.IsValidIndex(Slot) && Teams[Slot] >= 0 ? Teams[Slot] : Slot; }
	// The decided match: a winner (team id, -1 a draw) in the last snapshot.
	bool IsDecided() const { return bHasMatch && Winner >= -1; }
	// One line naming the snapshot fields above, for logs.
	FString DescribeExtras() const;
	bool CellVisible(int32 C) const { return bFogLifted || (FogVisible.IsValidIndex(C) && FogVisible[C]); }
	bool CellExplored(int32 C) const { return bFogLifted || (FogExplored.IsValidIndex(C) && FogExplored[C]); }

	// Reference levelOf: a digit is a level, a letter below 'a' counts down (shared/sim.js levelOf).
	static int8 LevelOf(TCHAR Ch);
	// Calls Run(start, count) for each run of differing cells in a packRuns string (shared/sim.js unpackRuns).
	static bool UnpackRuns(const FString& Str, TFunctionRef<void(int32, int32)> Run);

private:
	TSet<int32> WarnedUnreadable;   // unit ids already reported unreadable this match (one log line each)
	int32 ConfirmedMatch = -1, ConfirmedSeat = -1; // the match and seat a lobby on some socket confirmed
	int32 LoggedPointOwners = 0;    // a hash of the point owners last logged, so only a flip is logged
	bool bLoggedFirstExtras = false, bLoggedDecided = false, bWarnedCap = false, bWarnedMalformed = false;
	FString LoggedWeather;
	FTCRustViewShadow* UnitAuthority = nullptr; // TC_DECODER=rust (not owned)
	bool bUnitsFromRust = false;
	// While Units holds the Rust view's rows, the C++ decoder's own complete row set (and its unreadable ids) is kept
	// here, so its next delta applies to its own rows, its verdict stays its own and its tails carry over by id.
	TMap<int32, FTCUnit> CppRows;
	TSet<int32> CppUnreadable;
	bool bCppRowsAside = false;
	void DropRowsAside() { CppRows.Reset(); CppUnreadable.Reset(); bCppRowsAside = false; bUnitsFromRust = false; }
	bool ApplyStartDecode(const FJsonObject& M);
	// [count, xor] of the rows this decoder holds against the snapshot's held; false without held.
	bool HeldMismatch(const FJsonObject& M) const;
	void NoteSeatConfirmation();
	void ApplyCells(const TArray<TSharedPtr<FJsonValue>>& Cells);
	void ResetCells();
	static bool ParseUnit(const TArray<TSharedPtr<FJsonValue>>& Row, FTCUnit& Out);
	void ResetExtras();
	void ApplyStartExtras(const FJsonObject& M);
	void ApplySnapshotExtras(const FJsonObject& M);
	void ApplyLobbyResult(const FJsonObject& M);
	void NoteCap(int32 Dropped);
	void NoteMalformed();
	void LogExtrasTransitions();
};
