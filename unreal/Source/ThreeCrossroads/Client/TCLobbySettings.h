// The lobby's settings: the room's mode, map (World size in World Conquest), army size and Assault defender, and each
// seat's team, faction and AI difficulty. Read from the lobby message and changed with the messages the server takes,
// ported from the pinned reference (e4e579a):
//   server.js          lobby() (the message and its fields), the 'mode', 'map', 'worldSize', 'army', 'defender',
//                      'team', 'faction' and 'level' handlers (who may send them, when, and which values), seats(),
//                      assaultReady, newPlayer; an operative's messages stop before these handlers.
//   client/main.js     renderLobby (which selects the browser shows and enables, their options and labels, the Start
//                      button's text and the line under it), the selects' onchange messages, FACTIONS (names),
//                      AI_LEVELS, COLORS (six seats, "Team 1" to "Team 6"), MODE_NAMES, MODE_INFO, prettyMap.
//   client/index.html  the Mode and World size options, in order.
//   shared/sim.js      CFG.armies (the Army size labels), MAX_PLAYERS 6, FACTION_COUNT 4; shared/ai.js AI_LEVEL_NAMES.
//
// Facts come only from the lobby message: nothing here is guessed or kept from an earlier room. Every message built is
// one the browser sends from its lobby form, with the same fields and value types. The server answers none of them
// with a deny: a change it takes comes back as a new lobby for everyone, one it drops (not the host, a match running, a
// value it does not list) gets no answer at all. So a sent request stays pending until a lobby shows its value, and
// one the server dropped stays pending until it is replaced, the room starts a match or the socket reopens. The age of
// a pending request (SentAt) is for the presentation to judge; this module never decides it was refused.
//
// Not here, because the lobby message does not carry it: the map's preview, size, spawns and capture points (the
// browser fetches /maps/<name>.json over HTTP for its preview; this client does not), weather (lobby 'weather', the
// browser's own weather view), sandbox (FTCMatchState::bLobbySandbox), saves, operatives and the public listing.
// ApplyLobby logs nothing: the caller decides what a refused message is worth saying.
// Rust hosts additionally follow owner decisions 090 R1 and 097. The reference path stays unchanged.
#pragma once

#include "CoreMinimal.h"
#include "Net/TCConnection.h"

class FJsonObject;

// One lobby setting. Room settings have no seat; seat settings name one (the seat's index in the lobby's players).
enum class ETCLobbySetting : uint8
{
	Mode,       // { t: 'mode', v }: 'conquest', 'assault', 'annihilation', 'classic', 'horde', 'tutorial', 'world'
	Map,        // { t: 'map', name }: one of the lobby's maps (not shown in World Conquest)
	WorldSize,  // { t: 'worldSize', size }: 'huge' or 'massive' (World Conquest only)
	Army,       // { t: 'army', v }: 'standard', 'large', 'massive', 'endless' (never Endless in Horde)
	Defender,   // { t: 'defender', v }: the team that defends (Assault only)
	Team,       // { t: 'team', slot, v }: 0 to 5
	Faction,    // { t: 'faction', slot, v }: 0 USA, 1 Germany, 2 USSR, 3 UK
	Level,      // { t: 'level', slot, v }: an AI seat's difficulty, 'easy', 'normal' or 'hard'
};
const TCHAR* LexToString(ETCLobbySetting Setting);

// One seat as the lobby lists it (server.js lobby players).
struct FTCLobbySeat
{
	FString Name;
	int32 Team = -1;          // -1: the lobby's value was not an integer from 0 to 5
	int32 Faction = -1;       // -1: the lobby's value was not an integer from 0 to 3
	bool bAi = false;
	bool bConnected = false;  // an AI counts as connected (server.js: connected(q) || !!q.ai)
	FString Role = TEXT("commander"); // optional seat role; reference players are commanders
	FString Level;            // an AI's difficulty (the server sends 'normal' when unset); empty for a human
};

// The room as the last lobby message described it.
struct FTCLobbyRoom
{
	ETCHostKind HostKind = ETCHostKind::Unknown;
	int32 Protocol = -1;
	bool bSandbox = false;
	int32 Spectators = -1, Operatives = -1; // -1 when absent or malformed
	FString Weather;
	FString Code;
	FString State;              // 'lobby' or 'play' (server.js room.state)
	FString Role;               // 'commander' or 'operative'
	FString Mode;
	FString MapName;            // 'world' in World Conquest (server.js sends that instead of room.mapName)
	FString WorldSize;          // 'huge' unless the host picked 'massive'
	FString Army;               // 'standard' unless the host picked another
	int32 DefenderTeam = 0;
	TArray<FString> Maps;       // the map files the server lists, sorted (server.js listMaps)
	bool bHasHordeMaps = false; // sent only in Horde mode: the maps with defender spawns
	TArray<FString> HordeMaps;
	int32 Spawns = -1;          // server.js seats(room): how many players this map seats in this mode; -1 when absent
	int32 You = -1;             // this client's seat, -1 when it has none (a spectator or an operative)
	int32 Host = -1;            // the host's seat, -1 when nobody seated is human (then a spectator hosts)
	bool bAmHost = false;       // this client is the host (it may be a spectator: server.js isHost)
	bool bSpectator = false;
	bool bListed = false;       // a public room: seats are capped at Spawns
	bool bHasResult = false;    // the last match's result is shown (the Start button reads "Play again")
	TArray<FTCLobbySeat> Seats;
};

// One option of a setting, as the browser's select lists it.
struct FTCLobbyChoice
{
	FString Value;            // the wire value: the string sent, or for a numeric setting its number as text
	int32 Number = -1;        // Team, Faction, Defender: the number sent; -1 for string settings
	FString Label;            // the option's text in the browser
	bool bCurrent = false;    // the lobby's value
	bool bEnabled = true;     // the browser offers it (it disables Horde's maps without defender spawns, and Endless)
	FString Why;              // when not enabled: why
};

// One setting's control: whether it shows, whether this client may change it now, and its options.
struct FTCLobbyControl
{
	ETCLobbySetting Kind = ETCLobbySetting::Mode;
	int32 Slot = -1;          // seat settings: the seat; -1 for room settings
	FString Title;            // the browser's label ('Mode', 'Map', 'World size', 'Army size', ...)
	bool bShown = false;      // the browser shows it for this room, mode and seat
	bool bEditable = false;   // this client may change it now: the server would act on it, and the browser enables it
	FString Why;              // shown and not editable: why, in this client's words
	FString CurrentValue;     // the lobby's value (wire form), empty when unknown
	FString CurrentLabel;
	TArray<FTCLobbyChoice> Choices;
	// A request sent and not yet shown by a lobby (FTCLobbySettings::NoteSent).
	bool bPending = false;
	FString PendingValue;
	double PendingSentAt = 0.0;

	const FTCLobbyChoice* Find(const FString& Value) const;
};

// What a change would send.
struct FTCLobbyRequest
{
	ETCLobbySetting Kind = ETCLobbySetting::Mode;
	int32 Slot = -1;
	FString Value;                  // the value asked, in wire form
	TSharedPtr<FJsonObject> Message; // the one message to send; null when nothing goes out
	FString Note;                   // what was asked, or why nothing goes out
	FString SeatName;               // seat settings: the seat's name when planned (a kick can move seats)
};

class FTCLobbySettings
{
public:
	static constexpr const TCHAR* ReferencePin = TEXT("e4e579a");
	static constexpr int32 MaxPlayers = 6;    // shared/sim.js MAX_PLAYERS (also client/main.js COLORS.length)
	static constexpr int32 FactionCount = 4;  // shared/sim.js FACTION_COUNT

	// Reads a lobby message. False when it is not one or is malformed (missing state, mode or players, a seat that
	// is not an object): nothing changes then. A lobby in play forgets every pending request; any other lobby
	// resolves the pending requests whose value it shows.
	bool ApplyLobby(const FJsonObject& Message);
	// Forgets the room (a new server or room): nothing is known until the next lobby.
	void Reset();
	// A new socket opened: pending requests are forgotten, since the server may never have read them.
	void OnSocketOpened() { Pending.Reset(); }

	bool HasLobby() const { return bHasLobby; }
	const FTCLobbyRoom& Room() const { return Current; }
	bool IsRustHost() const { return bHasLobby && Current.HostKind == ETCHostKind::Rust; }
	bool InLobby() const { return bHasLobby && Current.State == TEXT("lobby"); }
	// Requests the lobby has shown since this object was made, for logs.
	int32 Confirmed() const { return ConfirmedCount; }

	// One control. Seat settings need a seat that exists; Level shows only for an AI seat.
	FTCLobbyControl Control(ETCLobbySetting Kind, int32 Slot = -1) const;
	// The room's controls in the browser's order: Map, World size, Mode, Army size, Defender (all five, with bShown).
	TArray<FTCLobbyControl> RoomControls() const;
	// A seat's controls in the browser's order: Team, Faction, Level (all three, with bShown).
	TArray<FTCLobbyControl> SeatControls(int32 Slot) const;

	// The message that changes Kind (of Slot) to Value, or why nothing goes out. Nothing goes out for a value already
	// shown unless another value is pending (the browser's select sends on every change, so picking the shown value
	// again after asking for another is a change).
	FTCLobbyRequest Plan(ETCLobbySetting Kind, int32 Slot, const FString& Value) const;
	FTCLobbyRequest Plan(ETCLobbySetting Kind, int32 Slot, int32 Number) const { return Plan(Kind, Slot, FString::FromInt(Number)); }
	// For a cycling button: the next (Direction > 0) or previous enabled choice after the one displayed (the pending
	// value if there is one, else the lobby's), wrapping around.
	FTCLobbyRequest PlanStep(ETCLobbySetting Kind, int32 Slot, int32 Direction = 1) const;
	// Remember a request that was sent (only when its Message actually went out), until a lobby shows it.
	void NoteSent(const FTCLobbyRequest& Request, double Now);
	int32 PendingCount() const { return Pending.Num(); }

	// client/main.js renderLobby, the Start button: its text ("Start: 1v1", "Play again: horde, 2 defenders", ...),
	// whether the browser enables it, and the line under it (empty when there is nothing to say). The server checks
	// the same seat count and Assault teams (server.js 'start'), and drops a start that fails them without an answer.
	FString StartLabel() const;
	bool StartAllowed() const;
	FString StartLine() const;
	// Owner decisions 090 R1 and 097. Checks the roster, then refuses the unverified required host profile.
	// No new readiness requirement and no inference from proto/rules/version.
	FString RustStartRefusal() const;
	// Shared guard for button and unattended send paths. Reference returns empty for every message.
	FString RustCommandRefusal(const FJsonObject& Message) const;
	static bool RustMapAllowed(const FString& Name);
	static FString RustChoiceRefusal(ETCLobbySetting Kind, const FString& Value);
	// "solo test", "co-op", "3-way FFA" or the team sizes ("2v1"), from the seats' teams.
	FString Matchup() const;

	// Labels, as the browser writes them. Unknown values come back as they are.
	static FString ModeName(const FString& Mode);
	static FString ModeInfo(const FString& Mode);   // the browser's line under the Mode select (empty if unknown)
	static FString PrettyMap(const FString& Name);  // client/main.js prettyMap: 'hill-112-xl' is 'Hill 112 XL'
	static FString ArmyLabel(const FString& Army);
	static FString WorldSizeLabel(const FString& Size);
	static FString LevelName(const FString& Level);
	static FString FactionName(int32 Faction);
	static FString TeamName(int32 Team);
	// The message type ('t') of a setting.
	static const TCHAR* MessageType(ETCLobbySetting Kind);
	static bool IsSeatSetting(ETCLobbySetting Kind) { return Kind == ETCLobbySetting::Team || Kind == ETCLobbySetting::Faction || Kind == ETCLobbySetting::Level; }

private:
	struct FPending
	{
		ETCLobbySetting Kind = ETCLobbySetting::Mode;
		int32 Slot = -1;
		FString Value, SeatName;
		double SentAt = 0.0;
	};
	FTCLobbyRoom Current;
	bool bHasLobby = false;
	TArray<FPending> Pending;
	int32 ConfirmedCount = 0;

	const FPending* FindPending(ETCLobbySetting Kind, int32 Slot) const;
	// The lobby's value of a setting, in wire form; false when the room or seat does not have it.
	bool CurrentValue(ETCLobbySetting Kind, int32 Slot, FString& Out) const;
	// Why nothing may be changed in this room now, or empty.
	FString RoomLock() const;
	TArray<int32> TeamsInUse() const;
};
