#include "Client/TCLobbySettings.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"

namespace TCLobbySettingsLocal
{
	// client/index.html modeSel options, in order; server.js 'mode' accepts exactly these. Names: client/main.js MODE_NAMES.
	const TCHAR* const Modes[] = { TEXT("conquest"), TEXT("assault"), TEXT("annihilation"), TEXT("classic"), TEXT("horde"), TEXT("tutorial"), TEXT("world") };
	const TCHAR* const ModeNames[] = { TEXT("Conquest"), TEXT("Assault"), TEXT("Annihilation"), TEXT("Classic"), TEXT("Horde"), TEXT("Tutorial"), TEXT("World Conquest") };
	// client/main.js MODE_INFO, in the order above
	const TCHAR* const ModeInfos[] = {
		TEXT("Capture and hold points to earn victory points. First side to the VP goal wins."),
		TEXT("One team defends a fortified command bunker. Everyone else attacks and must destroy it before the clock runs out."),
		TEXT("Every side starts with a fortified command bunker. Destroy every enemy bunker: last side standing wins. No clock."),
		TEXT("Build a base with engineers and train an army. Destroy every enemy HQ, Barracks, Motor Pool and Airfield."),
		TEXT("Co-op: everyone shares one HQ and defends one command bunker against waves that keep growing. The next wave comes when the last one is dead. How far can you get?"),
		TEXT("Learn to play: Sergeant Hollis walks you from a glider landing to a bridge, one order at a time. Friends can join as co-op."),
		TEXT("Scout a hidden continent, destroy regional military bases, then claim regions with infantry. Build your own bases on owned territory. Your side wins by owning 60% of the regions or by outlasting every rival nation. No clock. Saves itself every 2 minutes."),
	};
	// shared/sim.js CFG.armies keys; the labels are client/main.js armySel's, built from CFG.armies' pop and income
	// (standard 1/1, large 2.5/3, massive 5/6, endless 5/20). server.js 'army' accepts exactly these keys.
	const TCHAR* const Armies[] = { TEXT("standard"), TEXT("large"), TEXT("massive"), TEXT("endless") };
	const TCHAR* const ArmyLabels[] = { TEXT("Standard"), TEXT("Large: 2.5x units, 3x income"), TEXT("Massive: 5x units, 6x income"), TEXT("Endless: 5x units, 20x income") };
	// client/index.html worldSizeSel; server.js 'worldSize' accepts exactly these
	const TCHAR* const WorldSizes[] = { TEXT("huge"), TEXT("massive") };
	const TCHAR* const WorldSizeLabels[] = { TEXT("Huge: 64 regions, 1,024 \u00D7 1,024 m"), TEXT("Massive: 128 regions, 2,048 \u00D7 1,024 m") };
	// shared/ai.js AI_LEVEL_NAMES (Object.keys(AI_LEVELS)); labels client/main.js level()
	const TCHAR* const Levels[] = { TEXT("easy"), TEXT("normal"), TEXT("hard") };
	const TCHAR* const LevelNames[] = { TEXT("Easy"), TEXT("Normal"), TEXT("Hard") };
	// client/main.js FACTIONS names, by faction index (shared/sim.js FACTION_COUNT: USA, Germany, USSR, UK)
	const TCHAR* const Factions[] = { TEXT("USA"), TEXT("Germany"), TEXT("USSR"), TEXT("UK") };
	static_assert((int32)UE_ARRAY_COUNT(Factions) == FTCLobbySettings::FactionCount, "one name per faction");
	static_assert(UE_ARRAY_COUNT(ModeInfos) == UE_ARRAY_COUNT(Modes) && UE_ARRAY_COUNT(ModeNames) == UE_ARRAY_COUNT(Modes), "one name and line per mode");

	template <int32 N>
	int32 IndexOf(const TCHAR* const (&List)[N], const FString& Value)
	{
		for (int32 I = 0; I < N; I++) if (Value == List[I]) return I;
		return INDEX_NONE;
	}

	// Typed reads: FJsonObject's TryGet*Field convert between strings, numbers and booleans, and the server's types
	// are part of what is checked here.
	bool Str(const FJsonObject& O, const TCHAR* Name, FString& Out)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(Name);
		if (!V.IsValid() || V->Type != EJson::String) return false;
		Out = V->AsString();
		return true;
	}
	bool Flag(const FJsonObject& O, const TCHAR* Name)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(Name);
		return V.IsValid() && V->Type == EJson::Boolean && V->AsBool();
	}
	// An integer from Min to Max (the server's Number.isInteger and range checks); false otherwise
	bool Int(const TSharedPtr<FJsonValue>& V, int32 Min, int32 Max, int32& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		const double D = V->AsNumber();
		if (!FMath::IsFinite(D) || D != FMath::FloorToDouble(D) || D < Min || D > Max) return false;
		Out = (int32)D;
		return true;
	}
	bool Int(const FJsonObject& O, const TCHAR* Name, int32 Min, int32 Max, int32& Out) { return Int(O.TryGetField(Name), Min, Max, Out); }
	// A list of strings; entries of another type are skipped. False when the field is not an array.
	bool Strings(const FJsonObject& O, const TCHAR* Name, TArray<FString>& Out)
	{
		Out.Reset();
		const TSharedPtr<FJsonValue> V = O.TryGetField(Name);
		if (!V.IsValid() || V->Type != EJson::Array) return false;
		for (const TSharedPtr<FJsonValue>& E : V->AsArray()) if (E.IsValid() && E->Type == EJson::String) Out.Add(E->AsString());
		return true;
	}

	bool AsciiWord(TCHAR C) { return (C >= 'a' && C <= 'z') || (C >= 'A' && C <= 'Z') || (C >= '0' && C <= '9') || C == '_'; }
}

const TCHAR* LexToString(ETCLobbySetting Setting)
{
	return FTCLobbySettings::MessageType(Setting);
}

const FTCLobbyChoice* FTCLobbyControl::Find(const FString& Value) const
{
	for (const FTCLobbyChoice& C : Choices) if (C.Value == Value) return &C;
	return nullptr;
}

const TCHAR* FTCLobbySettings::MessageType(ETCLobbySetting Kind)
{
	switch (Kind)
	{
	case ETCLobbySetting::Mode: return TEXT("mode");
	case ETCLobbySetting::Map: return TEXT("map");
	case ETCLobbySetting::WorldSize: return TEXT("worldSize");
	case ETCLobbySetting::Army: return TEXT("army");
	case ETCLobbySetting::Defender: return TEXT("defender");
	case ETCLobbySetting::Team: return TEXT("team");
	case ETCLobbySetting::Faction: return TEXT("faction");
	case ETCLobbySetting::Level: return TEXT("level");
	}
	return TEXT("?");
}

FString FTCLobbySettings::ModeName(const FString& Mode)
{
	const int32 I = TCLobbySettingsLocal::IndexOf(TCLobbySettingsLocal::Modes, Mode);
	return I == INDEX_NONE ? Mode : FString(TCLobbySettingsLocal::ModeNames[I]);
}

FString FTCLobbySettings::ModeInfo(const FString& Mode)
{
	const int32 I = TCLobbySettingsLocal::IndexOf(TCLobbySettingsLocal::Modes, Mode);
	return I == INDEX_NONE ? FString() : FString(TCLobbySettingsLocal::ModeInfos[I]);
}

FString FTCLobbySettings::PrettyMap(const FString& Name)
{
	using TCLobbySettingsLocal::AsciiWord;
	// n.replace(/-/g, ' ').replace(/\b\w/g, c => c.toUpperCase()).replace(/\bXl\b/, 'XL')
	FString Out = Name.Replace(TEXT("-"), TEXT(" "));
	bool bPrevWord = false;
	for (int32 I = 0; I < Out.Len(); I++)
	{
		const TCHAR C = Out[I];
		const bool bWord = AsciiWord(C);
		if (bWord && !bPrevWord && C >= 'a' && C <= 'z') Out[I] = (TCHAR)(C - 'a' + 'A');
		bPrevWord = bWord;
	}
	for (int32 I = 0; I + 1 < Out.Len(); I++)
	{
		if (Out[I] != 'X' || Out[I + 1] != 'l') continue;
		if ((I > 0 && AsciiWord(Out[I - 1])) || (I + 2 < Out.Len() && AsciiWord(Out[I + 2]))) continue;
		Out[I + 1] = TEXT('L');
		break; // no g flag: the first one only
	}
	return Out;
}

FString FTCLobbySettings::ArmyLabel(const FString& Army)
{
	const int32 I = TCLobbySettingsLocal::IndexOf(TCLobbySettingsLocal::Armies, Army);
	return I == INDEX_NONE ? Army : FString(TCLobbySettingsLocal::ArmyLabels[I]);
}

FString FTCLobbySettings::WorldSizeLabel(const FString& Size)
{
	const int32 I = TCLobbySettingsLocal::IndexOf(TCLobbySettingsLocal::WorldSizes, Size);
	return I == INDEX_NONE ? Size : FString(TCLobbySettingsLocal::WorldSizeLabels[I]);
}

FString FTCLobbySettings::LevelName(const FString& Level)
{
	const int32 I = TCLobbySettingsLocal::IndexOf(TCLobbySettingsLocal::Levels, Level);
	return I == INDEX_NONE ? Level : FString(TCLobbySettingsLocal::LevelNames[I]);
}

FString FTCLobbySettings::FactionName(int32 Faction)
{
	return Faction >= 0 && Faction < FactionCount ? FString(TCLobbySettingsLocal::Factions[Faction]) : FString::Printf(TEXT("faction %d"), Faction);
}

FString FTCLobbySettings::TeamName(int32 Team)
{
	return FString::Printf(TEXT("Team %d"), Team + 1);
}

void FTCLobbySettings::Reset()
{
	Current = FTCLobbyRoom();
	bHasLobby = false;
	Pending.Reset();
}

bool FTCLobbySettings::ApplyLobby(const FJsonObject& M)
{
	using namespace TCLobbySettingsLocal;
	FString T;
	if (!Str(M, TEXT("t"), T) || T != TEXT("lobby")) return false;

	// Read into a new room, so a malformed message changes nothing.
	FTCLobbyRoom R;
	R.HostKind = FTCConnection::HostKindOf(M, &R.Protocol);
	R.bSandbox = Flag(M, TEXT("sandbox"));
	Str(M, TEXT("weather"), R.Weather);
	for (const TCHAR* Field : { TEXT("spectators"), TEXT("operatives") })
	{
		const TSharedPtr<FJsonValue> V = M.TryGetField(Field);
		const int32 Count = V.IsValid() && V->Type == EJson::Array ? V->AsArray().Num() : -1;
		if (FCString::Strcmp(Field, TEXT("spectators")) == 0) R.Spectators = Count; else R.Operatives = Count;
	}
	const TSharedPtr<FJsonValue> PlayersValue = M.TryGetField(TEXT("players"));
	if (!Str(M, TEXT("state"), R.State) || !Str(M, TEXT("mode"), R.Mode) || !PlayersValue.IsValid() || PlayersValue->Type != EJson::Array) return false;
	for (const TSharedPtr<FJsonValue>& V : PlayersValue->AsArray())
	{
		if (!V.IsValid() || V->Type != EJson::Object) return false;
		const FJsonObject& P = *V->AsObject();
		FTCLobbySeat Seat;
		Str(P, TEXT("name"), Seat.Name);
		Str(P, TEXT("role"), Seat.Role);
		if (!Int(P, TEXT("team"), 0, MaxPlayers - 1, Seat.Team)) Seat.Team = -1;
		if (!Int(P, TEXT("faction"), 0, FactionCount - 1, Seat.Faction)) Seat.Faction = -1;
		Seat.bAi = Flag(P, TEXT("ai"));
		Seat.bConnected = Flag(P, TEXT("connected"));
		if (Seat.bAi) Str(P, TEXT("level"), Seat.Level); // null for a human
		R.Seats.Add(MoveTemp(Seat));
	}
	Str(M, TEXT("code"), R.Code);
	if (!Str(M, TEXT("role"), R.Role)) R.Role = TEXT("commander");
	Str(M, TEXT("mapName"), R.MapName);
	Str(M, TEXT("worldSize"), R.WorldSize);
	Str(M, TEXT("army"), R.Army);
	if (!Int(M, TEXT("defenderTeam"), 0, MaxPlayers - 1, R.DefenderTeam)) R.DefenderTeam = -1;
	Strings(M, TEXT("maps"), R.Maps);
	R.bHasHordeMaps = Strings(M, TEXT("hordeMaps"), R.HordeMaps);
	if (!Int(M, TEXT("spawns"), 0, 1 << 16, R.Spawns)) R.Spawns = -1;
	if (!Int(M, TEXT("you"), -1, MaxPlayers - 1, R.You)) R.You = -1;
	if (!Int(M, TEXT("host"), -1, MaxPlayers - 1, R.Host)) R.Host = -1;
	R.bAmHost = Flag(M, TEXT("amHost"));
	R.bSpectator = Flag(M, TEXT("spectator"));
	R.bListed = Flag(M, TEXT("listed"));
	const TSharedPtr<FJsonValue> Result = M.TryGetField(TEXT("result"));
	R.bHasResult = Result.IsValid() && Result->Type == EJson::Object;

	Current = MoveTemp(R);
	bHasLobby = true;

	// A match took the room: whatever was asked is moot. Otherwise a request is done once the lobby shows its value.
	if (Current.State != TEXT("lobby")) { Pending.Reset(); return true; }
	for (int32 I = Pending.Num() - 1; I >= 0; I--)
	{
		const FPending& P = Pending[I];
		if (IsSeatSetting(P.Kind) && (!Current.Seats.IsValidIndex(P.Slot) || Current.Seats[P.Slot].Name != P.SeatName)) { Pending.RemoveAt(I); continue; } // the seat moved or left
		FString Shown;
		if (CurrentValue(P.Kind, P.Slot, Shown) && Shown == P.Value) { ConfirmedCount++; Pending.RemoveAt(I); }
	}
	return true;
}

bool FTCLobbySettings::CurrentValue(ETCLobbySetting Kind, int32 Slot, FString& Out) const
{
	Out.Reset();
	if (!bHasLobby) return false;
	const FTCLobbySeat* Seat = Current.Seats.IsValidIndex(Slot) ? &Current.Seats[Slot] : nullptr;
	switch (Kind)
	{
	case ETCLobbySetting::Mode: Out = Current.Mode; break;
	case ETCLobbySetting::Map: Out = Current.MapName; break;
	case ETCLobbySetting::WorldSize: Out = Current.WorldSize; break;
	case ETCLobbySetting::Army: Out = Current.Army; break;
	case ETCLobbySetting::Defender: if (Current.DefenderTeam >= 0) Out = FString::FromInt(Current.DefenderTeam); break;
	case ETCLobbySetting::Team: if (Seat && Seat->Team >= 0) Out = FString::FromInt(Seat->Team); break;
	case ETCLobbySetting::Faction: if (Seat && Seat->Faction >= 0) Out = FString::FromInt(Seat->Faction); break;
	case ETCLobbySetting::Level: if (Seat && Seat->bAi) Out = Seat->Level; break;
	}
	return !Out.IsEmpty();
}

const FTCLobbySettings::FPending* FTCLobbySettings::FindPending(ETCLobbySetting Kind, int32 Slot) const
{
	const int32 Key = IsSeatSetting(Kind) ? Slot : -1;
	for (const FPending& P : Pending) if (P.Kind == Kind && P.Slot == Key) return &P;
	return nullptr;
}

FString FTCLobbySettings::RoomLock() const
{
	// server.js: an operative's messages return before the lobby handlers; every setting needs room.state !== 'play'
	// (the browser's selects need state 'lobby', the only other state).
	if (Current.Role == TEXT("operative")) return TEXT("an operative changes no lobby settings");
	if (Current.State != TEXT("lobby")) return TEXT("a match is running: settings change in the lobby");
	return FString();
}

TArray<int32> FTCLobbySettings::TeamsInUse() const
{
	// client/main.js teamIds: [...new Set(m.players.map(p => p.team))].sort((a, b) => a - b)
	TArray<int32> Teams;
	for (const FTCLobbySeat& S : Current.Seats) if (S.Team >= 0) Teams.AddUnique(S.Team);
	Teams.Sort();
	return Teams;
}

FTCLobbyControl FTCLobbySettings::Control(ETCLobbySetting Kind, int32 Slot) const
{
	using namespace TCLobbySettingsLocal;
	FTCLobbyControl C;
	C.Kind = Kind;
	const bool bSeatSetting = IsSeatSetting(Kind);
	C.Slot = bSeatSetting ? Slot : -1;
	switch (Kind)
	{
	case ETCLobbySetting::Mode: C.Title = TEXT("Mode"); break;
	case ETCLobbySetting::Map: C.Title = TEXT("Map"); break;
	case ETCLobbySetting::WorldSize: C.Title = TEXT("World size"); break;
	case ETCLobbySetting::Army: C.Title = TEXT("Army size"); break;
	case ETCLobbySetting::Defender: C.Title = TEXT("Defender"); break;
	case ETCLobbySetting::Team: C.Title = TEXT("Team"); break;
	case ETCLobbySetting::Faction: C.Title = TEXT("Faction"); break;
	case ETCLobbySetting::Level: C.Title = TEXT("AI difficulty"); break;
	}
	if (!bHasLobby) { C.Why = TEXT("no lobby yet"); return C; }
	const FTCLobbySeat* Seat = bSeatSetting && Current.Seats.IsValidIndex(Slot) ? &Current.Seats[Slot] : nullptr;
	if (bSeatSetting && !Seat) { C.Why = FString::Printf(TEXT("the room has no seat %d"), Slot + 1); return C; }

	const FString& Mode = Current.Mode;
	const bool bHorde = Mode == TEXT("horde"), bWorld = Mode == TEXT("world");
	CurrentValue(Kind, Slot, C.CurrentValue);

	// Which controls the browser shows (client/main.js renderLobby), and their options.
	switch (Kind)
	{
	case ETCLobbySetting::Mode:
		C.bShown = true;
		for (int32 I = 0; I < UE_ARRAY_COUNT(Modes); I++) C.Choices.Add({ Modes[I], -1, ModeNames[I] });
		break;
	case ETCLobbySetting::Map:
		C.bShown = !bWorld; // World Conquest generates its map: the row is hidden
		if (!C.bShown) C.Why = TEXT("World Conquest makes its own map: pick the world size");
		for (const FString& Name : Current.Maps)
		{
			FTCLobbyChoice Choice{ Name, -1, PrettyMap(Name) };
			// Horde: only maps with defender spawns (the browser disables the rest; server.js drops them)
			if (bHorde && !Current.HordeMaps.Contains(Name)) { Choice.bEnabled = false; Choice.Why = TEXT("no defender spawns: Horde cannot be played here"); }
			C.Choices.Add(MoveTemp(Choice));
		}
		break;
	case ETCLobbySetting::WorldSize:
		C.bShown = bWorld;
		if (!C.bShown) C.Why = TEXT("only World Conquest has a world size");
		for (int32 I = 0; I < UE_ARRAY_COUNT(WorldSizes); I++) C.Choices.Add({ WorldSizes[I], -1, WorldSizeLabels[I] });
		break;
	case ETCLobbySetting::Army:
		C.bShown = true;
		for (int32 I = 0; I < UE_ARRAY_COUNT(Armies); I++)
		{
			FTCLobbyChoice Choice{ Armies[I], -1, ArmyLabels[I] };
			if (bHorde && Choice.Value == TEXT("endless")) { Choice.bEnabled = false; Choice.Why = TEXT("Horde never runs Endless"); }
			C.Choices.Add(MoveTemp(Choice));
		}
		break;
	case ETCLobbySetting::Defender:
		C.bShown = Mode == TEXT("assault");
		if (!C.bShown) C.Why = TEXT("only Assault has a defending team");
		for (int32 Team : TeamsInUse())
		{
			TArray<FString> Names;
			for (const FTCLobbySeat& S : Current.Seats) if (S.Team == Team) Names.Add(S.Name);
			C.Choices.Add({ FString::FromInt(Team), Team, FString::Printf(TEXT("Team %d defends (%s)"), Team + 1, *FString::Join(Names, TEXT(", "))) });
		}
		break;
	case ETCLobbySetting::Team:
		C.bShown = true;
		for (int32 I = 0; I < MaxPlayers; I++) C.Choices.Add({ FString::FromInt(I), I, TeamName(I) });
		break;
	case ETCLobbySetting::Faction:
		C.bShown = true;
		for (int32 I = 0; I < FactionCount; I++) C.Choices.Add({ FString::FromInt(I), I, FactionName(I) });
		break;
	case ETCLobbySetting::Level:
		C.bShown = Seat->bAi; // only an AI has a difficulty
		if (!C.bShown) C.Why = TEXT("only an AI seat has a difficulty");
		for (int32 I = 0; I < UE_ARRAY_COUNT(Levels); I++) C.Choices.Add({ Levels[I], -1, LevelNames[I] });
		break;
	}
	if (IsRustHost()) for (FTCLobbyChoice& Choice : C.Choices)
	{
		const FString Why = RustChoiceRefusal(Kind, Choice.Value);
		if (!Why.IsEmpty()) { Choice.bEnabled = false; Choice.Why = Why; }
	}
	for (FTCLobbyChoice& Choice : C.Choices) Choice.bCurrent = !C.CurrentValue.IsEmpty() && Choice.Value == C.CurrentValue;
	if (const FTCLobbyChoice* Shown = C.Find(C.CurrentValue)) C.CurrentLabel = Shown->Label;
	else if (Kind == ETCLobbySetting::Map) C.CurrentLabel = PrettyMap(C.CurrentValue); // 'world', or a map not listed
	else if (Kind == ETCLobbySetting::Defender && Current.DefenderTeam >= 0) C.CurrentLabel = FString::Printf(TEXT("Team %d defends"), Current.DefenderTeam + 1);
	else C.CurrentLabel = C.CurrentValue;
	if (const FPending* P = FindPending(Kind, Slot)) { C.bPending = true; C.PendingValue = P->Value; C.PendingSentAt = P->SentAt; }
	if (!C.bShown) return C;

	// Who may change it now: server.js's handler, narrowed where the browser disables a select the server would take.
	const FString Lock = RoomLock();
	if (!Lock.IsEmpty()) { C.Why = Lock; return C; }
	const bool bHost = Current.bAmHost;
	switch (Kind)
	{
	case ETCLobbySetting::Mode:
	case ETCLobbySetting::WorldSize:
	case ETCLobbySetting::Army:
	case ETCLobbySetting::Defender:
		C.bEditable = bHost;
		if (!bHost) C.Why = FString::Printf(TEXT("only the host changes the %s"), *C.Title.ToLower());
		break;
	case ETCLobbySetting::Map:
		// server.js 'map' returns without a change in Tutorial (the tutorial is scripted for its own map); the browser
		// leaves the select enabled, and the pick is silently dropped
		C.bEditable = bHost && Mode != TEXT("tutorial");
		C.Why = !bHost ? TEXT("only the host changes the map") : C.bEditable ? FString() : TEXT("the tutorial is played on its own map");
		break;
	case ETCLobbySetting::Team:
		// server.js takes a team from the host in any mode; the browser locks teams in Horde (everyone defends together)
		C.bEditable = bHost && !bHorde;
		C.Why = !bHost ? TEXT("only the host sets teams") : C.bEditable ? FString() : TEXT("Horde: everyone defends on one team");
		break;
	case ETCLobbySetting::Faction:
		// you pick your own faction; the host picks the AIs' (server.js: host ? p.ai || p === me : p === me)
		C.bEditable = (Current.You >= 0 && Slot == Current.You) || (bHost && Seat->bAi);
		if (!C.bEditable) C.Why = Seat->bAi ? TEXT("only the host picks an AI's faction") : TEXT("each player picks their own faction");
		break;
	case ETCLobbySetting::Level:
		C.bEditable = bHost;
		if (!bHost) C.Why = TEXT("only the host sets an AI's difficulty");
		break;
	}
	return C;
}

TArray<FTCLobbyControl> FTCLobbySettings::RoomControls() const
{
	// client/index.html: the Map row (or World size in World Conquest), then Mode, Army size, and Assault's defender
	return { Control(ETCLobbySetting::Map), Control(ETCLobbySetting::WorldSize), Control(ETCLobbySetting::Mode), Control(ETCLobbySetting::Army), Control(ETCLobbySetting::Defender) };
}

TArray<FTCLobbyControl> FTCLobbySettings::SeatControls(int32 Slot) const
{
	return { Control(ETCLobbySetting::Team, Slot), Control(ETCLobbySetting::Faction, Slot), Control(ETCLobbySetting::Level, Slot) };
}

FTCLobbyRequest FTCLobbySettings::Plan(ETCLobbySetting Kind, int32 Slot, const FString& Value) const
{
	FTCLobbyRequest R;
	R.Kind = Kind;
	R.Slot = IsSeatSetting(Kind) ? Slot : -1;
	R.Value = Value;
	const FTCLobbyControl C = Control(Kind, Slot);
	const FString What = R.Slot >= 0 && Current.Seats.IsValidIndex(R.Slot)
		? FString::Printf(TEXT("%s (seat %d, %s)"), *C.Title, R.Slot + 1, *Current.Seats[R.Slot].Name)
		: C.Title;
	if (R.Slot >= 0 && Current.Seats.IsValidIndex(R.Slot)) R.SeatName = Current.Seats[R.Slot].Name;
	if (!C.bShown) { R.Note = FString::Printf(TEXT("%s: %s"), *What, *C.Why); return R; }
	if (!C.bEditable) { R.Note = FString::Printf(TEXT("%s: %s"), *What, *C.Why); return R; }
	const FTCLobbyChoice* Choice = C.Find(Value);
	if (!Choice) { R.Note = FString::Printf(TEXT("%s: '%s' is not one of its choices"), *What, *Value); return R; }
	if (!Choice->bEnabled) { R.Note = FString::Printf(TEXT("%s: %s, %s"), *What, *Choice->Label, *Choice->Why); return R; }
	if (C.bPending && C.PendingValue == Value) { R.Note = FString::Printf(TEXT("%s: %s already asked, waiting for the lobby"), *What, *Choice->Label); return R; }
	if (Choice->bCurrent && !C.bPending) { R.Note = FString::Printf(TEXT("%s is already %s"), *What, *Choice->Label); return R; }

	// The browser's messages (client/main.js onchange handlers), field for field.
	TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
	O->SetStringField(TEXT("t"), MessageType(Kind));
	switch (Kind)
	{
	case ETCLobbySetting::Map: O->SetStringField(TEXT("name"), Value); break;            // { t: 'map', name }
	case ETCLobbySetting::WorldSize: O->SetStringField(TEXT("size"), Value); break;      // { t: 'worldSize', size }
	case ETCLobbySetting::Mode:
	case ETCLobbySetting::Army: O->SetStringField(TEXT("v"), Value); break;              // { t, v }
	case ETCLobbySetting::Defender: O->SetNumberField(TEXT("v"), Choice->Number); break; // { t: 'defender', v: +value }
	case ETCLobbySetting::Team:
	case ETCLobbySetting::Faction:                                                       // { t, slot, v: +value }
		O->SetNumberField(TEXT("slot"), R.Slot);
		O->SetNumberField(TEXT("v"), Choice->Number);
		break;
	case ETCLobbySetting::Level:                                                         // { t: 'level', slot, v: 'easy' }
		O->SetNumberField(TEXT("slot"), R.Slot);
		O->SetStringField(TEXT("v"), Value);
		break;
	}
	R.Message = O;
	R.Note = FString::Printf(TEXT("%s %s asked (was %s)"), *What, *Choice->Label, C.CurrentLabel.IsEmpty() ? TEXT("unknown") : *C.CurrentLabel);
	return R;
}

FTCLobbyRequest FTCLobbySettings::PlanStep(ETCLobbySetting Kind, int32 Slot, int32 Direction) const
{
	const FTCLobbyControl C = Control(Kind, Slot);
	if (!C.bShown || !C.bEditable || C.Choices.Num() == 0) return Plan(Kind, Slot, C.CurrentValue);
	const FString Displayed = C.bPending ? C.PendingValue : C.CurrentValue;
	const int32 N = C.Choices.Num(), Step = Direction < 0 ? -1 : 1;
	int32 At = C.Choices.IndexOfByPredicate([&Displayed](const FTCLobbyChoice& Ch) { return Ch.Value == Displayed; });
	if (At == INDEX_NONE) At = Step > 0 ? -1 : 0;
	for (int32 K = 1; K <= N; K++)
	{
		const FTCLobbyChoice& Next = C.Choices[((At + Step * K) % N + N) % N];
		if (Next.bEnabled && Next.Value != Displayed) return Plan(Kind, Slot, Next.Value);
	}
	return Plan(Kind, Slot, Displayed); // nothing else to pick: says why
}

void FTCLobbySettings::NoteSent(const FTCLobbyRequest& Request, double Now)
{
	if (!Request.Message.IsValid()) return;
	const int32 Key = IsSeatSetting(Request.Kind) ? Request.Slot : -1;
	Pending.RemoveAll([&Request, Key](const FPending& P) { return P.Kind == Request.Kind && P.Slot == Key; });
	FString Shown;
	if (CurrentValue(Request.Kind, Key, Shown) && Shown == Request.Value) return; // back to the shown value: nothing to wait for
	FPending P;
	P.Kind = Request.Kind; P.Slot = Key; P.Value = Request.Value; P.SeatName = Request.SeatName; P.SentAt = Now;
	Pending.Add(MoveTemp(P));
}

FString FTCLobbySettings::Matchup() const
{
	// client/main.js: "3v3", "2v2v2", "1v1", or FFA when nobody shares a team
	const int32 N = Current.Seats.Num();
	TArray<int32> Teams, Sizes; // teams in first-seen order (a JS Set keeps insertion order)
	for (const FTCLobbySeat& S : Current.Seats)
	{
		const int32 I = Teams.Find(S.Team);
		if (I == INDEX_NONE) { Teams.Add(S.Team); Sizes.Add(1); }
		else Sizes[I]++;
	}
	if (N == 1) return TEXT("solo test");
	if (Sizes.Num() == 1) return TEXT("co-op");
	bool bAllSolo = true;
	for (int32 K : Sizes) bAllSolo &= K == 1;
	if (bAllSolo && N > 2) return FString::Printf(TEXT("%d-way FFA"), N);
	TArray<FString> Parts;
	for (int32 K : Sizes) Parts.Add(FString::FromInt(K));
	return FString::Join(Parts, TEXT("v"));
}

FString FTCLobbySettings::StartLabel() const
{
	const FString& Mode = Current.Mode;
	const int32 N = Current.Seats.Num();
	FString What;
	if (Mode == TEXT("horde")) What = FString::Printf(TEXT("horde, %d defender%s"), N, N == 1 ? TEXT("") : TEXT("s"));
	else if (Mode == TEXT("assault")) What = TEXT("assault");
	else if (Mode == TEXT("world")) What = TEXT("World Conquest ") + Matchup();
	else if (Mode == TEXT("classic") || Mode == TEXT("annihilation")) What = Mode + TEXT(" ") + Matchup();
	else What = Matchup();
	return FString::Printf(TEXT("%s: %s"), Current.bHasResult ? TEXT("Play again") : TEXT("Start"), *What);
}

namespace TCLobbySettingsLocal
{
	// client/main.js: tooMany = n > (m.mode === 'world' ? COLORS.length : m.spawns ?? 3); server.js 'start' needs
	// players.length <= seats(room), which is MAX_PLAYERS (6) in World and the lobby's spawns otherwise
	int32 SeatLimit(const FTCLobbyRoom& R) { return R.Mode == TEXT("world") ? FTCLobbySettings::MaxPlayers : R.Spawns >= 0 ? R.Spawns : 3; }
	// server.js assaultReady
	bool AssaultReady(const FTCLobbyRoom& R)
	{
		if (R.Mode != TEXT("assault")) return true;
		bool bDefends = false, bAttacks = false;
		for (const FTCLobbySeat& S : R.Seats) { bDefends |= S.Team == R.DefenderTeam; bAttacks |= S.Team != R.DefenderTeam; }
		return bDefends && bAttacks;
	}
}

// R1 binds the committed public catalogue at native d2911267, including the bounded El Alamein path.
// A custom name advertised by a host cannot widen this list. Update only with a reviewed host matrix.
bool FTCLobbySettings::RustMapAllowed(const FString& Name)
{
	static const TCHAR* const Maps[] = { TEXT("ardennes-crossing"), TEXT("bastogne"), TEXT("bocage"), TEXT("crater-field"), TEXT("crossroads-village"), TEXT("default"), TEXT("el-alamein"), TEXT("helms-deep"), TEXT("hill-112"), TEXT("hill-112-xl"), TEXT("hot-gates"), TEXT("hurtgen-forest"), TEXT("island-towns"), TEXT("kasserine-pass"), TEXT("king-of-the-hill"), TEXT("monte-cassino"), TEXT("monte-cassino-xl"), TEXT("no-mans-land"), TEXT("pegasus-bridge"), TEXT("pegasus-bridge-xl"), TEXT("prokhorovka"), TEXT("river-towns"), TEXT("seawall"), TEXT("seawall-xl"), TEXT("seelow-heights"), TEXT("six-fronts"), TEXT("stalingrad-factory"), TEXT("test-arena"), TEXT("the-great-bridge"), TEXT("the-polder"), TEXT("three-islands"), TEXT("tutorial"), TEXT("twin-valleys") };
	return TCLobbySettingsLocal::IndexOf(Maps, Name) != INDEX_NONE;
}

FString FTCLobbySettings::RustChoiceRefusal(ETCLobbySetting Kind, const FString& Value)
{
	switch (Kind)
	{
	case ETCLobbySetting::Mode: if (Value != TEXT("conquest")) return TEXT("Rust host: Conquest/Default only."); break;
	case ETCLobbySetting::Map: if (!RustMapAllowed(Value)) return TEXT("Rust host: choose a committed public map."); break;
	case ETCLobbySetting::Army: if (Value != TEXT("standard")) return TEXT("Rust host: Standard army only."); break;
	case ETCLobbySetting::Team: if (Value != TEXT("0") && Value != TEXT("1")) return TEXT("Rust host: one commander on Team 1 and one on Team 2."); break;
	case ETCLobbySetting::Level: return TEXT("Rust host: no AI commanders.");
	case ETCLobbySetting::WorldSize:
	case ETCLobbySetting::Defender: return TEXT("Rust host: Conquest/Default only.");
	default: break;
	}
	return FString();
}

FString FTCLobbySettings::RustCommandRefusal(const FJsonObject& Message) const
{
	if (!IsRustHost()) return FString();
	FString Type, Value;
	if (!TCLobbySettingsLocal::Str(Message, TEXT("t"), Type)) return FString();
	if (Type == TEXT("start")) return RustStartRefusal();
	if (Type == TEXT("addAi")) return TEXT("Rust host: no AI commanders.");
	if (Type == TEXT("spectate")) return TEXT("Rust host: no spectators.");
	if (Type == TEXT("sit") && Current.State != TEXT("lobby")) return TEXT("Rust host: no late join or start during a match. Reconnect with the same seat token.");
	if (Type == TEXT("fps")) return TEXT("Rust host: no operatives.");
	if (Type == TEXT("sandbox"))
	{
		const TSharedPtr<FJsonValue> V = Message.TryGetField(TEXT("v"));
		if (!V.IsValid() || V->Type != EJson::Boolean || V->AsBool()) return TEXT("Rust host: Sandbox must be off.");
	}
	for (ETCLobbySetting Kind : { ETCLobbySetting::Mode, ETCLobbySetting::Map, ETCLobbySetting::Army, ETCLobbySetting::WorldSize, ETCLobbySetting::Defender, ETCLobbySetting::Level, ETCLobbySetting::Team })
	{
		if (Type != MessageType(Kind)) continue;
		if (Kind == ETCLobbySetting::Team)
		{
			int32 Team = -1;
			if (TCLobbySettingsLocal::Int(Message, TEXT("v"), 0, 1, Team)) return FString();
			return TEXT("Rust host: one commander on Team 1 and one on Team 2.");
		}
		TCLobbySettingsLocal::Str(Message, Kind == ETCLobbySetting::Map ? TEXT("name") : TEXT("v"), Value);
		return RustChoiceRefusal(Kind, Value);
	}
	return FString();
}

FString FTCLobbySettings::RustStartRefusal() const
{
	if (!IsRustHost()) return FString();
	if (Current.Protocol != FTCConnection::ProtocolVersion) return TEXT("Rust host: unsupported lobby protocol version.");
	if (Current.State != TEXT("lobby")) return TEXT("Rust host: no late join or start during a match. Reconnect with the same seat token.");
	if (Current.bListed) return TEXT("Rust host: trusted private rooms only.");
	if (Current.Role != TEXT("commander") || Current.Operatives != 0) return TEXT("Rust host: no operatives.");
	for (const FTCLobbySeat& S : Current.Seats) if (S.Role != TEXT("commander")) return TEXT("Rust host: no operatives.");
	if (Current.bSpectator || Current.Spectators != 0) return TEXT("Rust host: no spectators.");
	if (const FString Why = RustChoiceRefusal(ETCLobbySetting::Mode, Current.Mode); !Why.IsEmpty()) return Why;
	if (!RustMapAllowed(Current.MapName) || !Current.Maps.Contains(Current.MapName) || Current.Spawns < 2) return TEXT("Rust host: choose a committed public map with at least 2 spawns.");
	if (const FString Why = RustChoiceRefusal(ETCLobbySetting::Army, Current.Army); !Why.IsEmpty()) return Why;
	if (Current.bSandbox) return TEXT("Rust host: Sandbox must be off.");
	static const TCHAR* const Weather[] = { TEXT("map"), TEXT("clear"), TEXT("fog"), TEXT("rain"), TEXT("mud"), TEXT("snow"), TEXT("random") };
	if (TCLobbySettingsLocal::IndexOf(Weather, Current.Weather) == INDEX_NONE) return TEXT("Rust host: choose an existing weather option.");
	for (const FTCLobbySeat& S : Current.Seats) if (S.bAi) return TEXT("Rust host: no AI commanders.");
	if (Current.Seats.Num() != 2) return TEXT("Rust host: exactly 2 human commanders required.");
	const int32 A = Current.Seats[0].Team, B = Current.Seats[1].Team;
	if (!((A == 0 && B == 1) || (A == 1 && B == 0))) return TEXT("Rust host: one commander on Team 1 and one on Team 2.");
	if (!Current.bAmHost || Current.You != Current.Host || !Current.Seats.IsValidIndex(Current.You)) return TEXT("Waiting for the host to start...");
	return FString();
}

bool FTCLobbySettings::StartAllowed() const
{
	if (IsRustHost()) return RustStartRefusal().IsEmpty();
	const int32 N = Current.Seats.Num();
	return bHasLobby && N > 0 && N <= TCLobbySettingsLocal::SeatLimit(Current) && TCLobbySettingsLocal::AssaultReady(Current);
}

FString FTCLobbySettings::StartLine() const
{
	if (!bHasLobby) return FString();
	if (IsRustHost()) return RustStartRefusal();
	const int32 N = Current.Seats.Num(), Limit = TCLobbySettingsLocal::SeatLimit(Current);
	const bool bHorde = Current.Mode == TEXT("horde");
	if (N > Limit) return bHorde ? FString::Printf(TEXT("Horde seats %d defenders: remove a player."), Limit) : FString::Printf(TEXT("This map has %d spawns: pick a bigger map or remove players."), Limit);
	if (!TCLobbySettingsLocal::AssaultReady(Current)) return TEXT("Assault needs players on the defending team and on another team.");
	if (!Current.bAmHost) return TEXT("Waiting for the host to start...");
	return N == 1 ? FString::Printf(TEXT("Send the invite link, or add an AI %s."), bHorde ? TEXT("teammate") : TEXT("opponent")) : FString();
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "Policies/CondensedJsonPrintPolicy.h"

namespace TCLobbySettingsTest
{
	// Lobby messages exactly as the pinned reference server (e4e579a) sent them to an isolated copy's host, guest and
	// spectator (one AI seated; only publicUrl removed, it named the scratch port). In the same run the server answered
	// with a lobby showing the change for each of these message shapes, sent as written here: team, faction (the
	// guest's own, the host's for the AI), level (the AI), army, map, mode, defender, worldSize with 'size'. It answered
	// nothing at all to: team 6, faction 4, the host setting a human guest's faction, a human seat's level, the guest's
	// team, mode, army and AI level, a spectator's army, a map not listed, Endless in Horde, a map without defender
	// spawns in Horde (ardennes-crossing), a map in Tutorial, and the army during play. It took a team in Horde, which
	// the browser (and so this client) locks.
	const TCHAR* const ConquestHost = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":true,"spectators":[],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"ls37364","state":"lobby","you":0,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"huge","defenderTeam":0,"army":"standard","weather":"map","sandbox":false,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":1,"faction":1,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":2,"level":"normal"}]})json");
	const TCHAR* const ConquestGuest = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":false,"spectators":[],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"ls37364","state":"lobby","you":1,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"huge","defenderTeam":0,"army":"standard","weather":"map","sandbox":false,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":1,"faction":1,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":2,"level":"normal"}]})json");
	const TCHAR* const AssaultHost = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":true,"spectators":[],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"ls37364","state":"lobby","you":0,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"hill-112-xl","spawns":6,"mode":"assault","worldSize":"huge","defenderTeam":0,"army":"large","weather":"map","sandbox":false,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":1,"faction":3,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":1,"level":"hard"}]})json");
	const TCHAR* const HordeHost = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":true,"spectators":[],"listed":false,"saves":[],"hordeMaps":["bastogne","bocage","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","kasserine-pass","monte-cassino","monte-cassino-xl","pegasus-bridge","pegasus-bridge-xl","seawall","seawall-xl","seelow-heights","stalingrad-factory","the-great-bridge"],"hordeBest":null,"t":"lobby","code":"ls37364","state":"lobby","you":0,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"hill-112-xl","spawns":5,"mode":"horde","worldSize":"huge","defenderTeam":1,"army":"large","weather":"map","sandbox":false,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":1,"faction":3,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":1,"level":"hard"}]})json");
	const TCHAR* const WorldHost = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":true,"spectators":[],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"ls37364","state":"lobby","you":0,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"world","spawns":6,"mode":"world","worldSize":"massive","defenderTeam":1,"army":"large","weather":"map","sandbox":false,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":0,"faction":3,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":1,"level":"hard"}]})json");
	const TCHAR* const ConquestSpectator = TEXT(R"json({"role":"commander","operatives":[],"spectator":true,"amHost":false,"spectators":["Watcher"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"ls37364","state":"lobby","you":-1,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"massive","defenderTeam":1,"army":"large","weather":"map","sandbox":false,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":0,"faction":3,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":1,"level":"hard"}]})json");
	const TCHAR* const PlayHost = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":true,"spectators":["Watcher"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"ls37364","state":"play","you":0,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"massive","defenderTeam":1,"army":"large","weather":"map","sandbox":false,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":0,"faction":3,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":1,"level":"hard"}]})json");
	const TCHAR* const EndedHost = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":true,"spectators":["Watcher"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"ls37364","state":"lobby","you":0,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"massive","defenderTeam":1,"army":"large","weather":"map","sandbox":false,"result":{"ended":true,"you":null},"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":0,"faction":3,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":1,"level":"hard"}]})json");
	// client/main.js prettyMap run in node over every map the server listed, and three edge cases
	const TCHAR* const PrettyMaps[][2] = { { TEXT("ardennes-crossing"), TEXT("Ardennes Crossing") }, { TEXT("bastogne"), TEXT("Bastogne") }, { TEXT("bocage"), TEXT("Bocage") }, { TEXT("crater-field"), TEXT("Crater Field") }, { TEXT("crossroads-village"), TEXT("Crossroads Village") }, { TEXT("default"), TEXT("Default") }, { TEXT("el-alamein"), TEXT("El Alamein") }, { TEXT("helms-deep"), TEXT("Helms Deep") }, { TEXT("hill-112"), TEXT("Hill 112") }, { TEXT("hill-112-xl"), TEXT("Hill 112 XL") }, { TEXT("hot-gates"), TEXT("Hot Gates") }, { TEXT("hurtgen-forest"), TEXT("Hurtgen Forest") }, { TEXT("island-towns"), TEXT("Island Towns") }, { TEXT("kasserine-pass"), TEXT("Kasserine Pass") }, { TEXT("king-of-the-hill"), TEXT("King Of The Hill") }, { TEXT("monte-cassino"), TEXT("Monte Cassino") }, { TEXT("monte-cassino-xl"), TEXT("Monte Cassino XL") }, { TEXT("no-mans-land"), TEXT("No Mans Land") }, { TEXT("pegasus-bridge"), TEXT("Pegasus Bridge") }, { TEXT("pegasus-bridge-xl"), TEXT("Pegasus Bridge XL") }, { TEXT("prokhorovka"), TEXT("Prokhorovka") }, { TEXT("river-towns"), TEXT("River Towns") }, { TEXT("seawall"), TEXT("Seawall") }, { TEXT("seawall-xl"), TEXT("Seawall XL") }, { TEXT("seelow-heights"), TEXT("Seelow Heights") }, { TEXT("six-fronts"), TEXT("Six Fronts") }, { TEXT("stalingrad-factory"), TEXT("Stalingrad Factory") }, { TEXT("test-arena"), TEXT("Test Arena") }, { TEXT("the-great-bridge"), TEXT("The Great Bridge") }, { TEXT("the-polder"), TEXT("The Polder") }, { TEXT("three-islands"), TEXT("Three Islands") }, { TEXT("tutorial"), TEXT("Tutorial") }, { TEXT("twin-valleys"), TEXT("Twin Valleys") }, { TEXT("xl-xl"), TEXT("XL Xl") }, { TEXT("a--b"), TEXT("A  B") }, { TEXT("x1-xl"), TEXT("X1 XL") } };

	TSharedPtr<FJsonObject> Parse(const TCHAR* Src)
	{
		TSharedPtr<FJsonObject> Obj;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(FString(Src)), Obj);
		return Obj;
	}

	// The message as FTCConnection::TrySend writes it (condensed)
	FString Wire(const FTCLobbyRequest& R)
	{
		if (!R.Message.IsValid()) return TEXT("(nothing sent)");
		FString Out;
		TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
		FJsonSerializer::Serialize(R.Message.ToSharedRef(), Writer);
		return Out;
	}
}

// Run (after a build): -ExecCmds="Automation RunTests ThreeCrossroads.LobbySettings; Quit"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLobbySettingsReferenceTest, "ThreeCrossroads.LobbySettings.Reference", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCLobbySettingsReferenceTest::RunTest(const FString& Parameters)
{
	using namespace TCLobbySettingsTest;
	auto Load = [this](FTCLobbySettings& L, const TCHAR* Src, const TCHAR* What)
	{
		const TSharedPtr<FJsonObject> Obj = Parse(Src);
		TestTrue(FString::Printf(TEXT("%s parses"), What), Obj.IsValid() && L.ApplyLobby(*Obj));
	};
	auto Sends = [this](const FTCLobbyRequest& R, const TCHAR* Expected) { TestEqual(FString::Printf(TEXT("sends %s (%s)"), Expected, *R.Note), Wire(R), FString(Expected)); };
	auto Holds = [this](const FTCLobbyRequest& R, const TCHAR* What)
	{
		TestFalse(FString::Printf(TEXT("%s: nothing sent (%s)"), What, *R.Note), R.Message.IsValid());
		TestFalse(FString::Printf(TEXT("%s: says why"), What), R.Note.IsEmpty());
	};

	// The host in Conquest: seats Host (0), Guest (1) and AI 1 (2, normal), on teams 0, 1, 2 and factions 0, 1, 2
	{
		FTCLobbySettings L;
		TestFalse(TEXT("nothing known before a lobby"), L.Control(ETCLobbySetting::Mode).bShown);
		Load(L, ConquestHost, TEXT("conquest host"));
		const FTCLobbyRoom& R = L.Room();
		TestEqual(TEXT("mode"), R.Mode, FString(TEXT("conquest")));
		TestEqual(TEXT("map"), R.MapName, FString(TEXT("default")));
		TestEqual(TEXT("maps listed"), R.Maps.Num(), 33);
		TestEqual(TEXT("spawns"), R.Spawns, 3);
		TestTrue(TEXT("seat, host"), R.You == 0 && R.Host == 0 && R.bAmHost && !R.bSpectator);
		TestFalse(TEXT("no horde maps outside Horde"), R.bHasHordeMaps);
		TestEqual(TEXT("seats"), R.Seats.Num(), 3);
		if (R.Seats.Num() == 3)
		{
			TestTrue(TEXT("seat factions"), R.Seats[0].Faction == 0 && R.Seats[1].Faction == 1 && R.Seats[2].Faction == 2);
			TestTrue(TEXT("seat teams"), R.Seats[0].Team == 0 && R.Seats[1].Team == 1 && R.Seats[2].Team == 2);
			TestTrue(TEXT("levels: AI normal, human none"), R.Seats[2].bAi && R.Seats[2].Level == TEXT("normal") && R.Seats[0].Level.IsEmpty());
		}
		const FTCLobbyControl Map = L.Control(ETCLobbySetting::Map);
		TestTrue(TEXT("map shown and editable"), Map.bShown && Map.bEditable);
		TestEqual(TEXT("map label"), Map.CurrentLabel, FString(TEXT("Default")));
		TestEqual(TEXT("map choices"), Map.Choices.Num(), 33);
		TestFalse(TEXT("world size hidden"), L.Control(ETCLobbySetting::WorldSize).bShown);
		TestFalse(TEXT("defender hidden"), L.Control(ETCLobbySetting::Defender).bShown);
		TestFalse(TEXT("a human has no difficulty"), L.Control(ETCLobbySetting::Level, 0).bShown);
		TestTrue(TEXT("the host sets the AI's difficulty"), L.Control(ETCLobbySetting::Level, 2).bEditable);
		TestEqual(TEXT("faction label"), L.Control(ETCLobbySetting::Faction, 1).CurrentLabel, FString(TEXT("Germany")));

		// seat 1 is on team 1 already: the browser's select sends only on a change (integration fix: the first in-engine
		// run showed this case asked for the shown value; team 0 is a change server.js 'team' takes from the host)
		Holds(L.Plan(ETCLobbySetting::Team, 1, 1), TEXT("the team already shown"));
		Sends(L.Plan(ETCLobbySetting::Team, 1, 0), TEXT("{\"t\":\"team\",\"slot\":1,\"v\":0}"));
		Holds(L.Plan(ETCLobbySetting::Team, 1, 6), TEXT("team 6 is out of range"));
		Sends(L.Plan(ETCLobbySetting::Faction, 2, 1), TEXT("{\"t\":\"faction\",\"slot\":2,\"v\":1}"));
		Holds(L.Plan(ETCLobbySetting::Faction, 1, 0), TEXT("the host picks a human guest's faction"));
		Holds(L.Plan(ETCLobbySetting::Faction, 0, 4), TEXT("faction 4 is out of range"));
		Sends(L.Plan(ETCLobbySetting::Level, 2, TEXT("hard")), TEXT("{\"t\":\"level\",\"slot\":2,\"v\":\"hard\"}"));
		Holds(L.Plan(ETCLobbySetting::Level, 1, TEXT("hard")), TEXT("a human's level"));
		Sends(L.Plan(ETCLobbySetting::Army, -1, TEXT("large")), TEXT("{\"t\":\"army\",\"v\":\"large\"}"));
		Holds(L.Plan(ETCLobbySetting::Army, -1, TEXT("standard")), TEXT("the army size already shown"));
		Sends(L.Plan(ETCLobbySetting::Map, -1, TEXT("hill-112-xl")), TEXT("{\"t\":\"map\",\"name\":\"hill-112-xl\"}"));
		Holds(L.Plan(ETCLobbySetting::Map, -1, TEXT("no-such-map")), TEXT("a map the server does not list"));
		Sends(L.Plan(ETCLobbySetting::Mode, -1, TEXT("assault")), TEXT("{\"t\":\"mode\",\"v\":\"assault\"}"));
		Holds(L.Plan(ETCLobbySetting::Team, 5, 1), TEXT("a seat that does not exist"));
		TestEqual(TEXT("start label"), L.StartLabel(), FString(TEXT("Start: 3-way FFA")));
		TestTrue(TEXT("start allowed"), L.StartAllowed());
		TestEqual(TEXT("start line"), L.StartLine(), FString());

		// Pending: sent, then shown by the next lobby (the assault capture holds army 'large')
		const FTCLobbyRequest Army = L.Plan(ETCLobbySetting::Army, -1, TEXT("large"));
		L.NoteSent(Army, 10.0);
		const FTCLobbyControl Asked = L.Control(ETCLobbySetting::Army);
		TestTrue(TEXT("army pending"), Asked.bPending && Asked.PendingValue == TEXT("large") && Asked.PendingSentAt == 10.0);
		Holds(L.Plan(ETCLobbySetting::Army, -1, TEXT("large")), TEXT("the value already asked"));
		Sends(L.Plan(ETCLobbySetting::Army, -1, TEXT("standard")), TEXT("{\"t\":\"army\",\"v\":\"standard\"}")); // picking the shown value again is a change
		Sends(L.PlanStep(ETCLobbySetting::Army, -1, 1), TEXT("{\"t\":\"army\",\"v\":\"massive\"}")); // steps from the asked value
		Load(L, AssaultHost, TEXT("assault host"));
		TestEqual(TEXT("army request shown"), L.PendingCount(), 0);
		TestEqual(TEXT("confirmed"), L.Confirmed(), 1);
		TestTrue(TEXT("defender shown in Assault"), L.Control(ETCLobbySetting::Defender).bShown);
		Sends(L.Plan(ETCLobbySetting::Defender, -1, 1), TEXT("{\"t\":\"defender\",\"v\":1}"));
		TestEqual(TEXT("defender label"), L.Control(ETCLobbySetting::Defender).CurrentLabel, FString(TEXT("Team 1 defends (Host)")));
		TestEqual(TEXT("assault start label"), L.StartLabel(), FString(TEXT("Start: assault")));

		// A request still waiting when the match starts is forgotten
		L.NoteSent(L.Plan(ETCLobbySetting::Mode, -1, TEXT("classic")), 11.0);
		TestEqual(TEXT("mode pending"), L.PendingCount(), 1);
		Load(L, PlayHost, TEXT("play host"));
		TestEqual(TEXT("a match forgets pending requests"), L.PendingCount(), 0);
		TestFalse(TEXT("nothing editable in play"), L.Control(ETCLobbySetting::Army).bEditable || L.Control(ETCLobbySetting::Team, 1).bEditable);
		Holds(L.Plan(ETCLobbySetting::Army, -1, TEXT("standard")), TEXT("army during play"));

		// A malformed lobby changes nothing
		TestFalse(TEXT("a lobby without players is refused"), L.ApplyLobby(*Parse(TEXT("{\"t\":\"lobby\",\"state\":\"lobby\",\"mode\":\"horde\"}"))));
		TestFalse(TEXT("another message is refused"), L.ApplyLobby(*Parse(TEXT("{\"t\":\"start\",\"state\":\"lobby\",\"mode\":\"horde\",\"players\":[]}"))));
		TestEqual(TEXT("still the play lobby"), L.Room().State, FString(TEXT("play")));
	}

	// The guest (seat 1) in the same Conquest room
	{
		FTCLobbySettings L;
		Load(L, ConquestGuest, TEXT("conquest guest"));
		Holds(L.Plan(ETCLobbySetting::Team, 1, 2), TEXT("the guest's own team"));
		Sends(L.Plan(ETCLobbySetting::Faction, 1, 3), TEXT("{\"t\":\"faction\",\"slot\":1,\"v\":3}"));
		Holds(L.Plan(ETCLobbySetting::Faction, 2, 1), TEXT("the guest picks an AI's faction"));
		Holds(L.Plan(ETCLobbySetting::Level, 2, TEXT("easy")), TEXT("the guest sets an AI's level"));
		Holds(L.Plan(ETCLobbySetting::Army, -1, TEXT("massive")), TEXT("the guest's army size"));
		Holds(L.Plan(ETCLobbySetting::Mode, -1, TEXT("classic")), TEXT("the guest's mode"));
		TestTrue(TEXT("the guest sees the map"), L.Control(ETCLobbySetting::Map).bShown && !L.Control(ETCLobbySetting::Map).bEditable);
		TestEqual(TEXT("guest start line"), L.StartLine(), FString(TEXT("Waiting for the host to start...")));
	}

	// The host in Horde: maps without defender spawns and Endless are off, teams are locked by the browser
	{
		FTCLobbySettings L;
		Load(L, HordeHost, TEXT("horde host"));
		TestTrue(TEXT("horde maps listed"), L.Room().bHasHordeMaps && L.Room().HordeMaps.Num() == 17);
		Holds(L.Plan(ETCLobbySetting::Army, -1, TEXT("endless")), TEXT("Endless in Horde"));
		Holds(L.Plan(ETCLobbySetting::Map, -1, TEXT("ardennes-crossing")), TEXT("a map without defender spawns in Horde"));
		Sends(L.Plan(ETCLobbySetting::Map, -1, TEXT("bastogne")), TEXT("{\"t\":\"map\",\"name\":\"bastogne\"}"));
		TestFalse(TEXT("teams locked in Horde"), L.Control(ETCLobbySetting::Team, 1).bEditable);
		Sends(L.PlanStep(ETCLobbySetting::Army, -1, 1), TEXT("{\"t\":\"army\",\"v\":\"massive\"}"));
		Sends(L.PlanStep(ETCLobbySetting::Army, -1, -1), TEXT("{\"t\":\"army\",\"v\":\"standard\"}"));
		TestEqual(TEXT("horde start label"), L.StartLabel(), FString(TEXT("Start: horde, 3 defenders")));
		TestTrue(TEXT("horde start allowed (5 seats)"), L.StartAllowed());
	}

	// The host in World Conquest: World size instead of the map
	{
		FTCLobbySettings L;
		Load(L, WorldHost, TEXT("world host"));
		TestFalse(TEXT("map hidden in World"), L.Control(ETCLobbySetting::Map).bShown);
		Holds(L.Plan(ETCLobbySetting::Map, -1, TEXT("default")), TEXT("a map in World"));
		const FTCLobbyControl Size = L.Control(ETCLobbySetting::WorldSize);
		TestTrue(TEXT("world size shown, massive"), Size.bShown && Size.bEditable && Size.CurrentValue == TEXT("massive"));
		Sends(L.Plan(ETCLobbySetting::WorldSize, -1, TEXT("huge")), TEXT("{\"t\":\"worldSize\",\"size\":\"huge\"}"));
		TestEqual(TEXT("world start label"), L.StartLabel(), FString(TEXT("Start: World Conquest 2v1")));
	}

	// A spectator: no seat, not the host
	{
		FTCLobbySettings L;
		Load(L, ConquestSpectator, TEXT("conquest spectator"));
		TestTrue(TEXT("spectator"), L.Room().bSpectator && L.Room().You == -1 && !L.Room().bAmHost);
		for (int32 Slot = 0; Slot < 3; Slot++) TestFalse(TEXT("no faction is the spectator's"), L.Control(ETCLobbySetting::Faction, Slot).bEditable);
		Holds(L.Plan(ETCLobbySetting::Army, -1, TEXT("massive")), TEXT("the spectator's army size"));
		TestEqual(TEXT("spectator start line"), L.StartLine(), FString(TEXT("Waiting for the host to start...")));
	}

	// Back in the lobby after the host's End
	{
		FTCLobbySettings L;
		Load(L, EndedHost, TEXT("ended host"));
		TestTrue(TEXT("result shown"), L.Room().bHasResult);
		TestEqual(TEXT("play again label"), L.StartLabel(), FString(TEXT("Play again: 2v1")));
		TestTrue(TEXT("editable again"), L.Control(ETCLobbySetting::Army).bEditable);
	}

	for (const auto& Pair : PrettyMaps) TestEqual(FString::Printf(TEXT("prettyMap %s"), Pair[0]), FTCLobbySettings::PrettyMap(Pair[0]), FString(Pair[1]));
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
