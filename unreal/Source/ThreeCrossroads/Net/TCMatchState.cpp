#include "Net/TCMatchState.h"
#include "Net/TCRustViewShadow.h"
#include "Client/TCLocale.h"
#include "ThreeCrossroads.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"

namespace
{
	constexpr int32 MaxSide = 4096;

	bool Num(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return FMath::IsFinite(Out);
	}

	int32 IntOr(const TSharedPtr<FJsonValue>& V, int32 Fallback)
	{
		double D;
		return Num(V, D) ? (int32)FMath::Clamp(D, -2147483648.0, 2147483647.0) : Fallback;
	}
}

// The readers for the snapshot fields beyond units (a named namespace: unity builds join files that define their own
// Truthy or NumOr in anonymous ones). Member functions open it with a function-scope using-directive.
namespace TCMatchDecode
{
	double NumOr(const TSharedPtr<FJsonValue>& V, double Fallback)
	{
		double D;
		return Num(V, D) ? D : Fallback;
	}

	// JavaScript truthiness of a delivered value (a 0/1 flag, a boolean).
	bool Truthy(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid()) return false;
		switch (V->Type)
		{
		case EJson::Boolean: return V->AsBool();
		case EJson::Number: { const double D = V->AsNumber(); return D != 0.0 && D == D; }
		case EJson::String: return !V->AsString().IsEmpty();
		case EJson::Array: case EJson::Object: return true;
		default: return false;
		}
	}

	FString Clip(const FString& S) { return S.Len() > TCMatchLimits::Text ? S.Left(TCMatchLimits::Text) : S; }

	// A string field, or a number written as JavaScript's String(n) would (ids that may be either).
	FString StringOf(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid()) return FString();
		if (V->Type == EJson::String) return Clip(V->AsString());
		if (V->Type == EJson::Number) return FTCObjectives::JsNumber(V->AsNumber());
		return FString();
	}

	TSharedPtr<FJsonValue> Field(const FJsonObject& O, const TCHAR* Name) { return O.TryGetField(Name); }
	double NumField(const FJsonObject& O, const TCHAR* Name, double Fallback) { return NumOr(Field(O, Name), Fallback); }
	FString StrField(const FJsonObject& O, const TCHAR* Name) { return StringOf(Field(O, Name)); }
	bool HasNum(const FJsonObject& O, const TCHAR* Name, double& Out) { return Num(Field(O, Name), Out); }

	const TArray<TSharedPtr<FJsonValue>>* ArrayOf(const TSharedPtr<FJsonValue>& V)
	{
		const TArray<TSharedPtr<FJsonValue>>* A = nullptr;
		return V.IsValid() && V->Type == EJson::Array && V->TryGetArray(A) ? A : nullptr;
	}
	const FJsonObject* ObjectOf(const TSharedPtr<FJsonValue>& V)
	{
		return V.IsValid() && V->Type == EJson::Object && V->AsObject().IsValid() ? V->AsObject().Get() : nullptr;
	}

	FVector Vec3(const TSharedPtr<FJsonValue>& V)
	{
		const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(V);
		if (!A || A->Num() < 3) return FVector::ZeroVector;
		return FVector(NumOr((*A)[0], 0.0), NumOr((*A)[1], 0.0), NumOr((*A)[2], 0.0));
	}

	// [count, xor] style hash of point owners, only to notice a change for the log.
	int32 OwnersHash(const TArray<FTCPointRow>& Points)
	{
		uint32 H = 2166136261u;
		for (const FTCPointRow& P : Points) { H = (H ^ (uint32)(P.Owner + 7)) * 16777619u; }
		return (int32)(H ^ (uint32)Points.Num());
	}
}

int8 FTCMatchState::LevelOf(TCHAR Ch)
{
	if (Ch >= TEXT('a')) return (int8)FMath::Clamp(96 - (int32)Ch, -127, 127);
	if (Ch >= TEXT('0') && Ch <= TEXT('9')) return (int8)(Ch - TEXT('0'));
	return 0;
}

bool FTCMatchState::UnpackRuns(const FString& Str, TFunctionRef<void(int32, int32)> Run)
{
	// Each length is a little-endian base-32 varint in URL-safe base64 digits (32 and up: more follows), alternating
	// same, different, same, ...
	auto Digit = [](TCHAR C) -> int32
	{
		if (C >= 'A' && C <= 'Z') return C - 'A';
		if (C >= 'a' && C <= 'z') return C - 'a' + 26;
		if (C >= '0' && C <= '9') return C - '0' + 52;
		if (C == '-') return 62;
		if (C == '_') return 63;
		return -1;
	};
	int32 I = 0;
	int64 Cell = 0;
	bool bOn = false;
	const int32 N = Str.Len();
	while (I < N)
	{
		int64 V = 0, Scale = 1;
		int32 D;
		do
		{
			D = Digit(Str[I++]);
			if (D < 0 || Scale > (int64)1 << 40) return false;
			V += (D & 31) * Scale;
			Scale *= 32;
		} while ((D & 32) && I < N);
		if (V > MaxSide * MaxSide || Cell + V > (int64)MaxSide * MaxSide) return false; // past any map: malformed
		if (bOn && V) Run((int32)Cell, (int32)V);
		Cell += V;
		bOn = !bOn;
	}
	return true;
}

void FTCMatchState::ApplyLobby(const FJsonObject& M)
{
	M.TryGetStringField(TEXT("code"), RoomCode);
	M.TryGetStringField(TEXT("state"), RoomState);
	M.TryGetStringField(TEXT("mode"), Mode);
	M.TryGetBoolField(TEXT("amHost"), bAmHost);
	M.TryGetBoolField(TEXT("spectator"), bSpectator);
	double Seat = -1;
	if (M.TryGetNumberField(TEXT("you"), Seat)) LobbySeat = (int32)Seat;
	const TArray<TSharedPtr<FJsonValue>>* Players;
	if (M.TryGetArrayField(TEXT("players"), Players))
	{
		LobbyPlayers = Players->Num();
		Roster.Reset();
		for (const TSharedPtr<FJsonValue>& V : *Players)
		{
			FTCLobbyPlayer P;
			const TSharedPtr<FJsonObject>* O;
			if (V.IsValid() && V->TryGetObject(O))
			{
				(*O)->TryGetStringField(TEXT("name"), P.Name);
				(*O)->TryGetStringField(TEXT("level"), P.Level); // null for a human: stays empty
				(*O)->TryGetBoolField(TEXT("ai"), P.bAi);
				(*O)->TryGetBoolField(TEXT("connected"), P.bConnected);
				double Team;
				if ((*O)->TryGetNumberField(TEXT("team"), Team)) P.Team = (int32)Team;
			}
			Roster.Add(MoveTemp(P));
		}
	}
	M.TryGetStringField(TEXT("mapName"), LobbyMap);
	M.TryGetBoolField(TEXT("sandbox"), bLobbySandbox);
	// round 17: the Horde record (hordeBest: an object, or null)
	LobbyHordeBest = FLobbyHordeBest();
	const TSharedPtr<FJsonObject>* Best;
	if (M.TryGetObjectField(TEXT("hordeBest"), Best) && Best->IsValid())
	{
		double Wave = 0, Time = 0;
		LobbyHordeBest.bSet = (*Best)->TryGetNumberField(TEXT("wave"), Wave);
		(*Best)->TryGetNumberField(TEXT("time"), Time);
		LobbyHordeBest.Wave = (int32)Wave; LobbyHordeBest.Time = (int32)Time;
		const TArray<TSharedPtr<FJsonValue>>* BestNames;
		if ((*Best)->TryGetArrayField(TEXT("names"), BestNames))
			for (const TSharedPtr<FJsonValue>& V : *BestNames) { FString N; if (V.IsValid() && V->TryGetString(N)) LobbyHordeBest.Names.Add(N); }
	}
	const TArray<TSharedPtr<FJsonValue>>* Watching;
	if (M.TryGetArrayField(TEXT("spectators"), Watching))
	{
		Spectators.Reset();
		for (const TSharedPtr<FJsonValue>& V : *Watching) { FString N; if (V.IsValid() && V->TryGetString(N)) Spectators.Add(N); }
	}
	bLobbySinceOpen = true;

	// The last match's result, shown in every lobby that carries one until the next match starts (client/main.js
	// renderLobby: the line, then report.js's card). A lobby with a null result (the match is on, or none was played)
	// hides it.
	ApplyLobbyResult(M);

	// server.js finishMatch: the room is back in the lobby (a winner after the 6 s hold, or the host's end). A client
	// that was offline then gets only this lobby on its reconnect, never a start.
	if (bHasMatch && RoomState != TEXT("play")) ClearMatch();
	NoteSeatConfirmation();
	Objectives.OnLobby(*this);
}

void FTCMatchState::ApplyLobbyResult(const FJsonObject& M)
{
	using namespace TCMatchDecode;
	LastMatch = FTCMatchResult();
	LastResult.Reset();
	const FJsonObject* R = ObjectOf(Field(M, TEXT("result")));
	if (!R) return;
	FTCMatchResult& Out = LastMatch;
	Out.bPresent = true;
	Out.bEnded = Truthy(Field(*R, TEXT("ended")));
	double D;
	Out.bHasWinner = HasNum(*R, TEXT("winner"), D);
	Out.Winner = Out.bHasWinner ? (int32)D : -1;
	if (const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(Field(*R, TEXT("teams"))))
		for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Seats; I++) Out.Teams.Add(IntOr((*A)[I], -1));
	if (const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(Field(*R, TEXT("names"))))
		for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Seats; I++) Out.Names.Add(StringOf((*A)[I]));
	Out.Reason = StrField(*R, TEXT("reason"));
	Out.Mode = StrField(*R, TEXT("mode"));
	if (const FJsonObject* At = ObjectOf(Field(*R, TEXT("at"))))
	{
		double X, Z;
		if (HasNum(*At, TEXT("x"), X) && HasNum(*At, TEXT("z"), Z)) { Out.bHasAt = true; Out.At = FVector2D(X, Z); }
	}
	Out.EndTick = (int32)NumField(*R, TEXT("endTick"), 0.0);
	if (const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(Field(*R, TEXT("story"))))
	{
		Out.bHasStory = true;
		for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Seats; I++)
		{
			FTCStoryRow Row;
			if (const FJsonObject* S = ObjectOf((*A)[I]))
			{
				Row.bPresent = true;
				Row.Kills = (int32)NumField(*S, TEXT("kills"), 0.0); Row.Losses = (int32)NumField(*S, TEXT("losses"), 0.0);
				Row.Built = (int32)NumField(*S, TEXT("built"), 0.0); Row.Captures = (int32)NumField(*S, TEXT("captures"), 0.0);
				Row.MpSpent = (int32)NumField(*S, TEXT("mpSpent"), 0.0); Row.SupportCalls = (int32)NumField(*S, TEXT("supportCalls"), 0.0);
				Row.PlanesDowned = (int32)NumField(*S, TEXT("planesDowned"), 0.0);
			}
			Out.Story.Add(Row);
		}
	}
	if (const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(Field(*R, TEXT("timeline"))))
	{
		// Keep the last samples when a timeline is longer than the cap: the report's legend reads the last one.
		const int32 From = FMath::Max(0, A->Num() - TCMatchLimits::Timeline);
		for (int32 I = From; I < A->Num(); I++)
		{
			const FJsonObject* S = ObjectOf((*A)[I]);
			if (!S) continue;
			FTCTimelineSample Sample;
			Sample.T = NumField(*S, TEXT("t"), 0.0);
			if (const TArray<TSharedPtr<FJsonValue>>* V = ArrayOf(Field(*S, TEXT("vp")))) for (int32 K = 0; K < V->Num() && K < TCMatchLimits::Seats; K++) Sample.Vp.Add(NumOr((*V)[K], 0.0));
			if (const TArray<TSharedPtr<FJsonValue>>* P = ArrayOf(Field(*S, TEXT("points")))) for (int32 K = 0; K < P->Num() && K < TCMatchLimits::Seats; K++) Sample.Points.Add(NumOr((*P)[K], 0.0));
			Out.Timeline.Add(MoveTemp(Sample));
		}
	}
	if (HasNum(*R, TEXT("winVp"), D)) Out.WinVp = D;
	if (const FJsonObject* H = ObjectOf(Field(*R, TEXT("horde"))))
	{
		Out.bHorde = true;
		Out.HordeWave = (int32)NumField(*H, TEXT("wave"), 0.0);
		Out.HordeKills = (int32)NumField(*H, TEXT("kills"), 0.0);
		Out.HordeTime = (int32)NumField(*H, TEXT("time"), 0.0);
		Out.bHordeRecord = Truthy(Field(*H, TEXT("record")));
		if (const FJsonObject* Best = ObjectOf(Field(*H, TEXT("best")))) { Out.bHordeBest = true; Out.HordeBestWave = (int32)NumField(*Best, TEXT("wave"), 0.0); }
	}
	if (const FJsonObject* YouRow = ObjectOf(Field(*R, TEXT("you"))))
	{
		Out.bHasYou = true;
		Out.YouOutcome = StrField(*YouRow, TEXT("outcome"));
		Out.YouTeam = (int32)NumField(*YouRow, TEXT("team"), -1.0);
	}
	// client/main.js renderLobby reads the result with its own seat, which in the lobby is the lobby's 'you'.
	LastResult = FTCObjectives::ResultLine(LastMatch, LobbySeat);
}

void FTCMatchState::NoteSeatConfirmation()
{
	if (bHasMatch && bLobbySinceOpen && You >= 0 && !bSpectator && LobbySeat == You) { ConfirmedMatch = MatchId; ConfirmedSeat = You; }
}

void FTCMatchState::ClearMatch()
{
	bHasMatch = false;
	You = -1; Tick = 0; Winner = -2; Mp = 0;
	bSandbox = false;
	Spawn = FVector2D::ZeroVector;
	Names.Reset(); Teams.Reset();
	Map = FTCMap();
	bFogLifted = false;
	FogVisible.Reset(); FogExplored.Reset();
	Units.Reset(); UnreadableIds.Reset(); WarnedUnreadable.Reset(); DropRowsAside();
	bPaused = false; PauseText.Reset(); LastDeny.Reset(); DenyCmd.Reset(); DenyReason.Reset(); Denies = 0;
	Snapshots = 0;
	ConfirmedMatch = -1; ConfirmedSeat = -1;
	DirtyCells.Reset(); FogDirtyCells.Reset();
	ResetCells();
	bMapReset = true;
	ResetExtras();
	Objectives.Reset();
}

void FTCMatchState::ResetCells()
{
	CellStates.Reset(); CellData.Reset(); Sections.Reset(); LastCellRows.Reset(); LastCellRowsSkipped = 0;
}

void FTCMatchState::ResetExtras()
{
	Factions.Reset(); Spawns.Reset(); MapPoints.Reset(); StartMode.Reset();
	Points.Reset(); Vp.Reset(); Online.Reset(); Ping.Reset(); OutSeats.Reset();
	Plans.Reset(); WaitingOrders.Reset(); Ghosts.Reset(); Shots.Reset(); Flights.Reset(); Fires.Reset(); Falling.Reset();
	Weather = FTCWeatherRow(); bWeatherFromSnapshot = false; Wx = FTCWx();
	Inc.Reset(); FuelInc.Reset(); Upkeep.Reset(); Mun.Reset(); Fuel.Reset();
	Air.Reset(); ModeRow = FTCModeRow(); WorldScore = FTCWorldScore(); Scenario.Reset(); bHasScenario = false;
	FoundMines.Reset(); ArmyIncome = 1.0; End = FTCEndRow();
	DroppedOverCap = 0; SkippedMalformed = 0;
	LoggedPointOwners = 0; bLoggedFirstExtras = false; bLoggedDecided = false; bWarnedCap = false; bWarnedMalformed = false;
	LoggedWeather.Reset();
}

void FTCMatchState::NoteCap(int32 Dropped)
{
	if (Dropped <= 0) return;
	DroppedOverCap += Dropped;
	if (!bWarnedCap) { bWarnedCap = true; UE_LOG(LogTC, Warning, TEXT("snapshot %d: a list was longer than this client keeps (TCMatchLimits); %d entries dropped (logged once a match)"), Tick, Dropped); }
}

void FTCMatchState::NoteMalformed()
{
	SkippedMalformed++;
	if (!bWarnedMalformed) { bWarnedMalformed = true; UE_LOG(LogTC, Warning, TEXT("snapshot %d: an unreadable entry in a snapshot list was skipped (logged once a match)"), Tick); }
}

void FTCMatchState::ApplyPause(const FJsonObject& M)
{
	M.TryGetBoolField(TEXT("paused"), bPaused);
	FString By, Reason;
	M.TryGetStringField(TEXT("by"), By);
	M.TryGetStringField(TEXT("reason"), Reason);
	double Left = 0;
	M.TryGetNumberField(TEXT("left"), Left);
	PauseText = !bPaused ? FString() : Reason == TEXT("drop") ? FString::Printf(TEXT("Waiting for %s (%d s)"), *By, (int32)Left) : FString::Printf(TEXT("Paused by %s"), *By);
}

void FTCMatchState::ApplyDeny(const FJsonObject& M)
{
	DenyCmd.Reset(); DenyReason.Reset();
	M.TryGetStringField(TEXT("cmd"), DenyCmd);
	M.TryGetStringField(TEXT("reason"), DenyReason);
	LastDeny = FString::Printf(TEXT("%s: %s"), *DenyCmd, *DenyReason);
	Denies++;
}

bool FTCMatchState::ApplyStart(const FJsonObject& M)
{
	const bool bAccepted = ApplyStartDecode(M);
	bUnitsFromRust = false;
	// TC_DECODER=rust: the view reset itself on the same start (ww2_view_apply); a start carries no unit rows, so both
	// hold none now. The authority notes whether it took the start too.
	if (UnitAuthority) UnitAuthority->OnStateStart(bAccepted);
	return bAccepted;
}

bool FTCMatchState::ApplyStartDecode(const FJsonObject& M)
{
	const TSharedPtr<FJsonObject>* MapObj;
	if (!M.TryGetObjectField(TEXT("map"), MapObj)) return false;
	const FJsonObject& JMap = **MapObj;
	double W = 0, H = 0;
	const TArray<TSharedPtr<FJsonValue>>* Rows;
	if (!JMap.TryGetNumberField(TEXT("w"), W) || !JMap.TryGetNumberField(TEXT("h"), H) || !JMap.TryGetArrayField(TEXT("rows"), Rows)) return false;
	if (W < 1 || H < 1 || W > MaxSide || H > MaxSide || Rows->Num() != (int32)H) return false;

	FTCMap NewMap;
	NewMap.W = (int32)W; NewMap.H = (int32)H;
	JMap.TryGetStringField(TEXT("name"), NewMap.Name);
	NewMap.bWorld = JMap.HasField(TEXT("world"));
	const int32 Count = NewMap.W * NewMap.H;
	NewMap.Chars.Init('.', Count);
	NewMap.Levels.Init(0, Count);
	NewMap.Known.Init(!NewMap.bWorld, Count); // an authored map is public; a World map is known only where discovered
	for (int32 Y = 0; Y < NewMap.H; Y++)
	{
		const FString Row = (*Rows)[Y].IsValid() ? (*Rows)[Y]->AsString() : FString();
		for (int32 X = 0; X < NewMap.W && X < Row.Len(); X++) NewMap.Chars[Y * NewMap.W + X] = (uint8)FMath::Clamp((int32)Row[X], 0, 127);
	}
	const TArray<TSharedPtr<FJsonValue>>* Heights;
	if (JMap.TryGetArrayField(TEXT("heights"), Heights) && Heights->Num() == NewMap.H)
	{
		for (int32 Y = 0; Y < NewMap.H; Y++)
		{
			const FString Row = (*Heights)[Y].IsValid() ? (*Heights)[Y]->AsString() : FString();
			for (int32 X = 0; X < NewMap.W && X < Row.Len(); X++) NewMap.Levels[Y * NewMap.W + X] = LevelOf(Row[X]);
		}
	}

	Map = MoveTemp(NewMap);
	ResetCells(); // a new terrain: client/main.js startGame makes a new structuralSections map and state array
	bHasMatch = true;
	double D;
	MatchId = M.TryGetNumberField(TEXT("matchId"), D) ? (int32)D : -1;
	You = M.TryGetNumberField(TEXT("you"), D) ? (int32)D : -1;
	M.TryGetBoolField(TEXT("sandbox"), bSandbox);
	Tick = 0; Winner = -2; Snapshots = 0; LastDeny.Reset(); DenyCmd.Reset(); DenyReason.Reset(); Denies = 0;
	// A pause still on is resent right after every start (server.js sendStart, beginMatch), but a resume broadcast
	// while this client was away is not: clear it here or a stale "Paused" outlives the reconnect.
	bPaused = false; PauseText.Reset();
	LastResult.Reset(); LastMatch = FTCMatchResult(); // the lobby that ends this match brings the next result
	const TSharedPtr<FJsonObject>* SpawnObj;
	double SX = 0, SZ = 0;
	if (M.TryGetObjectField(TEXT("spawn"), SpawnObj) && (*SpawnObj)->TryGetNumberField(TEXT("x"), SX) && (*SpawnObj)->TryGetNumberField(TEXT("z"), SZ)) Spawn = FVector2D(SX, SZ);
	Names.Reset(); Teams.Reset();
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	if (M.TryGetArrayField(TEXT("names"), Arr)) for (const auto& V : *Arr) Names.Add(V.IsValid() ? V->AsString() : FString());
	if (M.TryGetArrayField(TEXT("teams"), Arr)) for (const auto& V : *Arr) Teams.Add(IntOr(V, -1));

	// The start replaces every unit held: snapshots then send rows against an empty "sent" map (server.js sendStart).
	Units.Reset(); UnreadableIds.Reset(); WarnedUnreadable.Reset(); DropRowsAside();
	DirtyCells.Reset(); FogDirtyCells.Reset();
	bMapReset = true;

	const int32 Count2 = Map.W * Map.H;
	FogVisible.Init(0, Count2);
	FogExplored.Init(0, Count2);
	const TSharedPtr<FJsonObject>* Fog;
	bFogLifted = !M.TryGetObjectField(TEXT("fog"), Fog);
	if (!bFogLifted)
	{
		FString V, E;
		(*Fog)->TryGetStringField(TEXT("v"), V);
		(*Fog)->TryGetStringField(TEXT("e"), E);
		auto Mark = [Count2](TArray<uint8>& Into) { return [&Into, Count2](int32 Start, int32 N) { for (int32 C = Start; C < Start + N && C < Count2; C++) if (C >= 0) Into[C] = 1; }; };
		if (!UnpackRuns(V, Mark(FogVisible)) || !UnpackRuns(E, Mark(FogExplored))) UE_LOG(LogTC, Warning, TEXT("start: malformed fog runs"));
	}

	if (M.TryGetArrayField(TEXT("cells"), Arr)) ApplyCells(*Arr);
	DirtyCells.Reset(); // the map reset redraws everything anyway
	NoteSeatConfirmation(); // a match start: its lobby came first on this socket (server.js beginMatch awaits lobby)
	ResetExtras();
	ApplyStartExtras(M);
	Objectives.Reset();
	Objectives.OnStart(*this);
	return true;
}

void FTCMatchState::ApplyStartExtras(const FJsonObject& M)
{
	using namespace TCMatchDecode;
	StartMode = Mode; // client/main.js startGame reads lobbyState.mode: the last lobby's mode, if any came yet
	if (const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(Field(M, TEXT("factions"))))
		for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Seats; I++) Factions.Add(IntOr((*A)[I], 0));
	if (const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(Field(M, TEXT("spawns"))))
		for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Seats; I++)
		{
			TOptional<FVector2D> Sp;
			double X, Z;
			if (const FJsonObject* O = ObjectOf((*A)[I])) if (HasNum(*O, TEXT("x"), X) && HasNum(*O, TEXT("z"), Z)) Sp = FVector2D(X, Z);
			Spawns.Add(Sp);
		}
	// client/weather-view.js start: row = Array.isArray(r) ? r : ['clear']
	if (const TArray<TSharedPtr<FJsonValue>>* W = ArrayOf(Field(M, TEXT("weather"))))
	{
		Weather.Now = W->Num() > 0 ? StringOf((*W)[0]) : FString();
		if (Weather.Now.IsEmpty()) Weather.Now = TEXT("clear");
		Weather.Next = W->Num() > 1 ? StringOf((*W)[1]) : FString();
		Weather.SecondsLeft = W->Num() > 2 ? IntOr((*W)[2], 0) : 0;
	}
	if (const FJsonObject* MapObj = ObjectOf(Field(M, TEXT("map"))))
		if (const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(Field(*MapObj, TEXT("points"))))
		{
			NoteCap(A->Num() - TCMatchLimits::Points);
			for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Points; I++)
			{
				FTCMapPoint P;
				if (const FJsonObject* O = ObjectOf((*A)[I]))
				{
					P.X = (int32)NumField(*O, TEXT("x"), 0.0);
					P.Y = (int32)NumField(*O, TEXT("y"), 0.0);
					P.Kind = StrField(*O, TEXT("kind"));
					P.Mp = NumField(*O, TEXT("mp"), 1.0);   // client/main.js p.mp ?? 1
					P.Vp = NumField(*O, TEXT("vp"), 1.0);   // p.vp ?? 1
					P.Needs = (int32)NumField(*O, TEXT("needs"), -1.0);
				}
				else NoteMalformed();
				MapPoints.Add(MoveTemp(P)); // kept in place even when unreadable: snapshot rows index this order
			}
		}
}

void FTCMatchState::ApplyCells(const TArray<TSharedPtr<FJsonValue>>& Cells)
{
	using namespace TCMatchDecode;
	// Each entry is [cell, char, level, state, data] (shared/sim.js logCell / terrainFor), from this player's memory.
	// client/main.js applyCells: grid, level (when present), state (st ?? 0) and, from data, ground, object, material
	// and the structural section.
	LastCellRows.Reset();
	LastCellRowsSkipped = 0;
	for (const TSharedPtr<FJsonValue>& Entry : Cells)
	{
		const TArray<TSharedPtr<FJsonValue>>* Row;
		if (!Entry.IsValid() || !Entry->TryGetArray(Row) || Row->Num() < 2) { LastCellRowsSkipped++; continue; }
		const int32 C = IntOr((*Row)[0], -1);
		FString Ch;
		if (!Map.IsValidCell(C) || !(*Row)[1].IsValid() || !(*Row)[1]->TryGetString(Ch) || Ch.IsEmpty()) { LastCellRowsSkipped++; continue; }
		Map.Chars[C] = (uint8)FMath::Clamp((int32)Ch[0], 0, 127);
		double Lv;
		if (Row->Num() > 2 && Num((*Row)[2], Lv)) Map.Levels[C] = (int8)FMath::Clamp((int32)Lv, -127, 127);
		Map.Known[C] = true;
		DirtyCells.Add(C);
		LastCellRows.Add(C);
		// a Uint8Array store: ToUint8 (NaN and absent 0, truncation, modulo 256)
		double St = 0.0;
		double Wrapped = Row->Num() > 3 && Num((*Row)[3], St) ? FMath::Fmod(FMath::TruncToDouble(St), 256.0) : 0.0;
		if (Wrapped < 0.0) Wrapped += 256.0;
		const uint8 State = (uint8)Wrapped;
		CellStates.Add(C, State);
		const FJsonObject* Data = Row->Num() > 4 ? ObjectOf((*Row)[4]) : nullptr;
		if (!Data) continue;
		FTCCellData& D = CellData.FindOrAdd(C);
		D.Ground = StrField(*Data, TEXT("ground")); D.Object = StrField(*Data, TEXT("object")); D.Material = StrField(*Data, TEXT("material"));
		if (const FJsonObject* S = ObjectOf(Field(*Data, TEXT("section"))))
		{
			FTCCellSection Sec;
			Sec.Id = StrField(*S, TEXT("id")); Sec.StructureId = StrField(*S, TEXT("structureId"));
			Sec.State = StrField(*S, TEXT("state")); Sec.Material = StrField(*S, TEXT("material"));
			Sec.bHasHp = HasNum(*S, TEXT("hp"), Sec.Hp);
			Sec.bHasMaxHp = HasNum(*S, TEXT("maxHp"), Sec.MaxHp);
			Sec.bAnchor = Truthy(Field(*S, TEXT("anchor")));
			Sections.Add(C, MoveTemp(Sec));
		}
	}
}

int32 FTCMatchState::NarrowInt(double V)
{
	if (V != V) return 0;
	if (V <= -2147483649.0) return MIN_int32;
	if (V >= 2147483648.0) return MAX_int32;
	return (int32)V;
}

void FTCMatchState::FillUnit(FTCUnit& Out, const FString& Type, const double Columns[15])
{
	const double* V = Columns;
	Out.Type = Type;
	Out.Id = NarrowInt(V[0]); Out.Owner = NarrowInt(V[2]);
	Out.X = V[3]; Out.Z = V[4]; Out.Rot = V[5]; Out.Aim = V[6];
	Out.Hp = NarrowInt(V[7]); Out.Supp = NarrowInt(V[8]); Out.Target = NarrowInt(V[9]); Out.Cover = NarrowInt(V[10]);
	Out.Cd = NarrowInt(V[11]); Out.Flags = NarrowInt(V[12]); Out.Stars = NarrowInt(V[13]); Out.Built = V[14];
	Out.Tail.Reset();
}

bool FTCMatchState::ParseUnit(const TArray<TSharedPtr<FJsonValue>>& Row, FTCUnit& Out)
{
	if (Row.Num() < 15) return false;
	double V[15] = {}; // column 1 is the type, read below
	for (int32 I = 0; I < 15; I++)
	{
		if (I == 1) continue;
		if (!Num(Row[I], V[I])) return false;
	}
	FString Type;
	if (!Row[1].IsValid() || !Row[1]->TryGetString(Type)) return false;
	FillUnit(Out, Type, V);
	for (int32 I = 15; I < Row.Num(); I++) { double T; if (Num(Row[I], T)) Out.Tail.Add(T); }
	return true;
}

bool FTCMatchState::ApplySnapshot(const FJsonObject& M)
{
	if (!bHasMatch)
	{
		if (UnitAuthority) UnitAuthority->OnStateSnapshotIgnored();
		return false;
	}
	// The C++ decoder applies this snapshot to its own rows (they were set aside while Units held the Rust view's).
	if (bCppRowsAside)
	{
		Units = MoveTemp(CppRows); UnreadableIds = MoveTemp(CppUnreadable);
		CppRows.Reset(); CppUnreadable.Reset(); bCppRowsAside = false;
	}
	bUnitsFromRust = false;
	Snapshots++;
	double D;
	if (M.TryGetNumberField(TEXT("tick"), D)) Tick = (int32)D;
	if (M.TryGetNumberField(TEXT("mp"), D)) Mp = (int32)D;
	Winner = M.TryGetNumberField(TEXT("winner"), D) ? (int32)D : -2;

	// Unit rows, as client/world-state.js applies them: all (or no held) replaces, gone removes, rows upsert.
	const TArray<TSharedPtr<FJsonValue>>* Held = nullptr;
	const bool bHasHeld = M.TryGetArrayField(TEXT("held"), Held) && Held->Num() >= 2;
	bool bAll = false;
	M.TryGetBoolField(TEXT("all"), bAll);
	if (bAll || !bHasHeld) { Units.Reset(); UnreadableIds.Reset(); }
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	if (M.TryGetArrayField(TEXT("gone"), Arr)) for (const auto& V : *Arr) { const int32 Id = IntOr(V, 0); Units.Remove(Id); UnreadableIds.Remove(Id); }
	if (M.TryGetArrayField(TEXT("units"), Arr))
	{
		for (const auto& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			if (!V.IsValid() || !V->TryGetArray(Row)) continue;
			FTCUnit U;
			if (ParseUnit(*Row, U)) { const int32 Id = U.Id; UnreadableIds.Remove(Id); Units.Add(Id, MoveTemp(U)); continue; }
			// The browser stores any row (client/world-state.js). Keep the last good row for this id if there is one,
			// else remember the id so the held check still agrees with the server.
			double IdNum;
			if (Row->Num() < 1 || !Num((*Row)[0], IdNum)) continue;
			const int32 Id = NarrowInt(IdNum);
			if (!Units.Contains(Id)) UnreadableIds.Add(Id);
			bool bAlreadyWarned = false;
			WarnedUnreadable.Add(Id, &bAlreadyWarned);
			if (!bAlreadyWarned) UE_LOG(LogTC, Warning, TEXT("snapshot %d: unit %d row unreadable (%d fields): %s"), Tick, Id, Row->Num(), Units.Contains(Id) ? TEXT("keeping its last row") : TEXT("not drawn"));
		}
	}

	// This decoder's held verdict on its own rows. TC_DECODER=rust: the Rust view's rows replace them now, before
	// anything reads Units (the plans, orders, ghosts and planes below keep only units held), and its verdict is the
	// one returned. The tails ABI 2 does not expose stay this decoder's, matched by id.
	const bool bCppDrift = HeldMismatch(M);
	bool bDrift = bCppDrift;
	if (UnitAuthority)
	{
		bool bAuthorityResync = false;
		TMap<int32, FTCUnit> RustRows;
		const ETCUnitAdoption Adopted = UnitAuthority->OnStateSnapshot(Tick, Units, UnreadableIds, bCppDrift, RustRows, bAuthorityResync);
		if (Adopted == ETCUnitAdoption::Adopted)
		{
			CppRows = MoveTemp(Units); CppUnreadable = MoveTemp(UnreadableIds);
			Units = MoveTemp(RustRows); UnreadableIds.Reset();
			bCppRowsAside = bUnitsFromRust = true;
		}
		if (Adopted == ETCUnitAdoption::Adopted || Adopted == ETCUnitAdoption::Refused) bDrift = bAuthorityResync;
	}

	// Terrain this player's side learned (its own memory), and fog cells that flipped since the last fog.
	LastCellRows.Reset(); LastCellRowsSkipped = 0;
	if (M.TryGetArrayField(TEXT("cells"), Arr)) ApplyCells(*Arr);
	FString FogDelta;
	if (!bFogLifted && M.TryGetStringField(TEXT("fog"), FogDelta))
	{
		const int32 Count = Map.W * Map.H;
		const bool bOk = UnpackRuns(FogDelta, [this, Count](int32 Start, int32 N)
		{
			for (int32 C = FMath::Max(0, Start); C < Start + N && C < Count; C++)
			{
				FogVisible[C] ^= 1;
				if (FogVisible[C]) FogExplored[C] = 1;
				FogDirtyCells.Add(C);
			}
		});
		if (!bOk) UE_LOG(LogTC, Warning, TEXT("snapshot %d: malformed fog delta"), Tick);
	}

	// Everything else the snapshot carries, after the units: plans, orders, ghosts and planes are kept only for units
	// held now, as client/main.js applies them after its rows.
	ApplySnapshotExtras(M);
	Objectives.OnSnapshot(*this);
	LogExtrasTransitions();
	return bDrift;
}

bool FTCMatchState::HeldMismatch(const FJsonObject& M) const
{
	const TArray<TSharedPtr<FJsonValue>>* Held = nullptr;
	if (!M.TryGetArrayField(TEXT("held"), Held) || Held->Num() < 2) return false;
	int32 Xor = 0;
	for (const auto& Pair : Units) Xor ^= Pair.Key;
	for (int32 Id : UnreadableIds) Xor ^= Id;
	return Units.Num() + UnreadableIds.Num() != IntOr((*Held)[0], -1) || Xor != IntOr((*Held)[1], 0);
}

void FTCMatchState::ApplySnapshotExtras(const FJsonObject& M)
{
	using namespace TCMatchDecode;
	using Arr = TArray<TSharedPtr<FJsonValue>>;
	auto Seats = [this](const Arr* A) { if (A) NoteCap(A->Num() - TCMatchLimits::Seats); return A ? FMath::Min(A->Num(), TCMatchLimits::Seats) : 0; };

	// points: [owner, capper, progress, contested, cut, locked] (objectives.js destructures with 0 defaults)
	Points.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("points"))))
	{
		NoteCap(A->Num() - TCMatchLimits::Points);
		for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Points; I++)
		{
			FTCPointRow P;
			const Arr* R = ArrayOf((*A)[I]);
			if (R && R->Num() >= 1)
			{
				P.Owner = IntOr((*R)[0], -1);
				P.Capper = R->Num() > 1 ? IntOr((*R)[1], -1) : -1;
				P.Progress = R->Num() > 2 ? NumOr((*R)[2], 0.0) : 0.0;
				P.bContested = R->Num() > 3 && Truthy((*R)[3]);
				P.bCut = R->Num() > 4 && Truthy((*R)[4]);
				P.bLocked = R->Num() > 5 && Truthy((*R)[5]);
			}
			else NoteMalformed();
			Points.Add(P); // kept in place: the row index is the point
		}
	}

	// vp (floored per seat), online, ping (server.js sendSeat extras: AI -1, unknown null), out
	Vp.Reset(); Online.Reset(); Ping.Reset(); OutSeats.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("vp")))) for (int32 I = 0, N = Seats(A); I < N; I++) Vp.Add(IntOr((*A)[I], 0));
	if (const Arr* A = ArrayOf(Field(M, TEXT("online"))))
		for (int32 I = 0, N = Seats(A); I < N; I++)
		{
			TOptional<bool> O;
			if ((*A)[I].IsValid() && (*A)[I]->Type == EJson::Boolean) O = (*A)[I]->AsBool();
			Online.Add(O);
		}
	if (const Arr* A = ArrayOf(Field(M, TEXT("ping"))))
		for (int32 I = 0, N = Seats(A); I < N; I++)
		{
			TOptional<int32> P;
			double D;
			if (Num((*A)[I], D)) P = (int32)FMath::Clamp(D, -1.0, 99999.0);
			Ping.Add(P);
		}
	if (const Arr* A = ArrayOf(Field(M, TEXT("out")))) for (int32 I = 0, N = Seats(A); I < N; I++) OutSeats.Add(Truthy((*A)[I]));

	// plans: [id, kind, tx, tz, ...path]; client/main.js keeps a plan only for a unit it holds
	Plans.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("plans"))))
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const Arr* R = ArrayOf(V);
			double Id;
			if (!R || R->Num() < 4 || !Num((*R)[0], Id)) { NoteMalformed(); continue; }
			if (!Units.Contains((int32)Id)) continue;
			if (Plans.Num() >= TCMatchLimits::Plans) { NoteCap(1); continue; }
			FTCPlan P;
			P.Id = (int32)Id; P.Kind = IntOr((*R)[1], 0); P.Tx = NumOr((*R)[2], 0.0); P.Tz = NumOr((*R)[3], 0.0);
			for (int32 K = 4; K + 1 < R->Num() && P.Path.Num() < TCMatchLimits::PlanPath; K += 2) P.Path.Add(FVector2D(NumOr((*R)[K], 0.0), NumOr((*R)[K + 1], 0.0)));
			Plans.Add(MoveTemp(P));
		}

	// orders: [id, count, kind, x, z, ...]; only a held unit of this seat, at most count whole entries
	WaitingOrders.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("orders"))))
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const Arr* R = ArrayOf(V);
			double Id;
			if (!R || R->Num() < 2 || !Num((*R)[0], Id)) { NoteMalformed(); continue; }
			const FTCUnit* U = Units.Find((int32)Id);
			if (!U || U->Owner != You) continue;
			if (WaitingOrders.Num() >= TCMatchLimits::Plans) { NoteCap(1); continue; }
			FTCWaitingOrders W;
			W.Id = (int32)Id;
			const int32 Count = IntOr((*R)[1], 0), PointsLen = R->Num() - 2;
			for (int32 I = 0; I < Count && I * 3 + 2 < PointsLen && W.Orders.Num() < TCMatchLimits::QueuedOrders; I++)
				W.Orders.Add({ IntOr((*R)[2 + I * 3], 0), NumOr((*R)[3 + I * 3], 0.0), NumOr((*R)[4 + I * 3], 0.0) });
			WaitingOrders.Add(MoveTemp(W));
		}

	// ghosts: [id, type, owner, x, z, built]; a live row of the same id wins (applyGhosts)
	Ghosts.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("ghosts"))))
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const Arr* R = ArrayOf(V);
			double Id;
			if (!R || R->Num() < 5 || !Num((*R)[0], Id)) { NoteMalformed(); continue; }
			if (Units.Contains((int32)Id)) continue;
			if (Ghosts.Num() >= TCMatchLimits::Ghosts) { NoteCap(1); continue; }
			FTCGhost G;
			G.Id = (int32)Id; G.Type = StringOf((*R)[1]); G.Owner = IntOr((*R)[2], -1);
			G.X = NumOr((*R)[3], 0.0); G.Z = NumOr((*R)[4], 0.0); G.Built = R->Num() > 5 ? NumOr((*R)[5], 1.0) : 1.0;
			Ghosts.Add(MoveTemp(G));
		}

	// shots: this snapshot's events, as delivered (f / t only where the server left them)
	Shots.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("shots"))))
	{
		NoteCap(A->Num() - TCMatchLimits::Shots);
		for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Shots; I++)
		{
			const FJsonObject* O = ObjectOf((*A)[I]);
			if (!O) { NoteMalformed(); continue; }
			FTCShot S;
			S.K = StrField(*O, TEXT("k")); S.Kind = StrField(*O, TEXT("kind")); S.Effect = StrField(*O, TEXT("effect"));
			S.Material = StrField(*O, TEXT("material")); S.Name = StrField(*O, TEXT("name")); S.Id = StrField(*O, TEXT("id"));
			double X, Y, Z, D;
			S.bHasPos = HasNum(*O, TEXT("x"), X) && HasNum(*O, TEXT("z"), Z);
			if (S.bHasPos) { S.X = X; S.Z = Z; }
			S.bHasY = HasNum(*O, TEXT("y"), Y);
			if (S.bHasY) S.Y = Y;
			// shared/sim.js: a shooter or target id the recipient cannot see is deleted; 0 means none (if (s.f && ...))
			if (HasNum(*O, TEXT("f"), D) && D != 0) { S.bHasShooter = true; S.F = (int32)D; }
			if (HasNum(*O, TEXT("fo"), D)) { S.bHasShooterOwner = true; S.Fo = (int32)D; }
			if (HasNum(*O, TEXT("t"), D) && D != 0) { S.bHasTarget = true; S.T = (int32)D; }
			if (HasNum(*O, TEXT("to"), D)) { S.bHasTargetOwner = true; S.To = (int32)D; }
			S.Flight = (int32)NumField(*O, TEXT("flight"), 0.0); S.Seq = (int32)NumField(*O, TEXT("seq"), -1.0); S.M = (int32)NumField(*O, TEXT("m"), -1.0);
			S.Time = NumField(*O, TEXT("time"), 0.0); S.At = NumField(*O, TEXT("at"), 0.0); S.Until = NumField(*O, TEXT("until"), 0.0);
			S.Expires = NumField(*O, TEXT("expires"), 0.0); S.Gravity = NumField(*O, TEXT("gravity"), 0.0); S.Blast = NumField(*O, TEXT("blast"), 0.0);
			S.V = Vec3(Field(*O, TEXT("v")));
			S.bKill = Truthy(Field(*O, TEXT("kill"))); S.bTerminal = Truthy(Field(*O, TEXT("terminal"))); S.bHit = Truthy(Field(*O, TEXT("hit")));
			S.bExplosive = Truthy(Field(*O, TEXT("explosive"))); S.bLocal = Truthy(Field(*O, TEXT("local")));
			Shots.Add(MoveTemp(S));
		}
	}

	// flights: projectiles this team sees (projectiles.js); f / fo only with a seen shooter
	Flights.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("flights"))))
	{
		NoteCap(A->Num() - TCMatchLimits::Flights);
		for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Flights; I++)
		{
			const FJsonObject* O = ObjectOf((*A)[I]);
			double Id;
			if (!O || !HasNum(*O, TEXT("flight"), Id)) { NoteMalformed(); continue; }
			FTCFlight F;
			F.Flight = (int32)Id; F.Seq = (int32)NumField(*O, TEXT("seq"), 0.0);
			F.Kind = StrField(*O, TEXT("kind")); F.Effect = StrField(*O, TEXT("effect"));
			F.X = NumField(*O, TEXT("x"), 0.0); F.Y = NumField(*O, TEXT("y"), 0.0); F.Z = NumField(*O, TEXT("z"), 0.0);
			F.V = Vec3(Field(*O, TEXT("v"))); F.Gravity = NumField(*O, TEXT("gravity"), 0.0);
			F.Time = NumField(*O, TEXT("time"), 0.0); F.Launched = NumField(*O, TEXT("launched"), 0.0);
			F.Expires = NumField(*O, TEXT("expires"), 0.0); F.Until = NumField(*O, TEXT("until"), 0.0);
			double D;
			if (HasNum(*O, TEXT("f"), D)) { F.bHasShooter = true; F.F = (int32)D; F.Fo = (int32)NumField(*O, TEXT("fo"), -1.0); }
			Flights.Add(MoveTemp(F));
		}
	}

	// fires: burning cells this team sees
	Fires.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("fires"))))
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const int32 C = IntOr(V, -1);
			if (!Map.IsValidCell(C)) { NoteMalformed(); continue; }
			if (Fires.Num() >= TCMatchLimits::Fires) { NoteCap(1); continue; }
			Fires.Add(C);
		}

	// falling: building sections coming down where this team sees them
	Falling.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("falling"))))
	{
		NoteCap(A->Num() - TCMatchLimits::Falling);
		for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Falling; I++)
		{
			const FJsonObject* O = ObjectOf((*A)[I]);
			if (!O) { NoteMalformed(); continue; }
			FTCFalling F;
			F.Id = StrField(*O, TEXT("id")); F.Kind = StrField(*O, TEXT("kind")); F.Material = StrField(*O, TEXT("material")); F.Section = StrField(*O, TEXT("section"));
			F.X = NumField(*O, TEXT("x"), 0.0); F.Y = NumField(*O, TEXT("y"), 0.0); F.Z = NumField(*O, TEXT("z"), 0.0);
			F.Dir = NumField(*O, TEXT("dir"), 0.0); F.Tilt = NumField(*O, TEXT("tilt"), 0.0); F.Mass = NumField(*O, TEXT("mass"), 0.0);
			F.Gravity = NumField(*O, TEXT("gravity"), 0.0); F.Drag = NumField(*O, TEXT("drag"), 0.0); F.Time = NumField(*O, TEXT("time"), 0.0);
			F.Born = NumField(*O, TEXT("born"), 0.0); F.At = NumField(*O, TEXT("at"), 0.0); F.Until = NumField(*O, TEXT("until"), 0.0);
			F.Impulse = NumField(*O, TEXT("impulse"), 0.0); F.V = Vec3(Field(*O, TEXT("v")));
			Falling.Add(MoveTemp(F));
		}
	}

	// weather: only an array changes it (weather-view.js snapshot); wx: only when present (main.js if (s.wx))
	if (const Arr* W = ArrayOf(Field(M, TEXT("weather"))))
	{
		Weather.Now = W->Num() > 0 ? StringOf((*W)[0]) : FString();
		if (Weather.Now.IsEmpty()) Weather.Now = TEXT("clear");
		Weather.Next = W->Num() > 1 ? StringOf((*W)[1]) : FString();
		Weather.SecondsLeft = W->Num() > 2 ? IntOr((*W)[2], 0) : 0;
		bWeatherFromSnapshot = true;
	}
	if (const Arr* W = ArrayOf(Field(M, TEXT("wx"))))
	{
		Wx.bKnown = true;
		Wx.Rain = W->Num() > 0 ? NumOr((*W)[0], 0.0) : 0.0; Wx.Wet = W->Num() > 1 ? NumOr((*W)[1], 0.0) : 0.0;
		Wx.WindAngle = W->Num() > 2 ? NumOr((*W)[2], 0.0) : 0.0; Wx.WindStrength = W->Num() > 3 ? NumOr((*W)[3], 0.0) : 0.0;
	}

	// economy: per snapshot; a spectator's snapshot has none (server.js watcherSnapshot deletes them)
	double D;
	Inc.Reset(); FuelInc.Reset(); Upkeep.Reset(); Mun.Reset(); Fuel.Reset();
	if (HasNum(M, TEXT("inc"), D)) Inc = D;
	if (HasNum(M, TEXT("fuelInc"), D)) FuelInc = D;
	if (HasNum(M, TEXT("upkeep"), D)) Upkeep = D;
	if (HasNum(M, TEXT("mun"), D)) Mun = (int32)D;
	if (HasNum(M, TEXT("fuel"), D)) Fuel = (int32)D;
	ArmyIncome = 1.0;
	if (const FJsonObject* Army = ObjectOf(Field(M, TEXT("army")))) ArmyIncome = NumField(*Army, TEXT("income"), 1.0);

	// air: [id, state, fuel, ammo, timer]; hud.js drawAir lists only planes held
	Air.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("air"))))
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const Arr* R = ArrayOf(V);
			double Id;
			if (!R || R->Num() < 2 || !Num((*R)[0], Id)) { NoteMalformed(); continue; }
			if (!Units.Contains((int32)Id)) continue;
			if (Air.Num() >= TCMatchLimits::Air) { NoteCap(1); continue; }
			FTCAirRow Row;
			Row.Id = (int32)Id; Row.State = IntOr((*R)[1], 0);
			Row.Fuel = R->Num() > 2 ? IntOr((*R)[2], 0) : 0; Row.Ammo = R->Num() > 3 ? IntOr((*R)[3], 0) : 0; Row.Timer = R->Num() > 4 ? IntOr((*R)[4], 0) : 0;
			Air.Add(Row);
		}

	// mode (modeRow): absent in Conquest
	ModeRow = FTCModeRow();
	if (const FJsonObject* O = ObjectOf(Field(M, TEXT("mode"))))
	{
		FTCModeRow& R = ModeRow;
		R.bPresent = true;
		const FString Kind = StrField(*O, TEXT("kind"));
		if (!Kind.IsEmpty()) R.Kind = Kind;
		auto Opt = [O](const TCHAR* Name, TOptional<int32>& Into) { double V; if (HasNum(*O, Name, V)) Into = (int32)V; };
		Opt(TEXT("defenderTeam"), R.DefenderTeam); Opt(TEXT("attackerTeam"), R.AttackerTeam); Opt(TEXT("timeLeft"), R.TimeLeft);
		Opt(TEXT("total"), R.Total); Opt(TEXT("wave"), R.Wave); Opt(TEXT("left"), R.Left); Opt(TEXT("slot"), R.Slot);
		Opt(TEXT("step"), R.Step); Opt(TEXT("steps"), R.Steps);
		R.bSuddenDeath = Truthy(Field(*O, TEXT("suddenDeath")));
		R.bActive = Truthy(Field(*O, TEXT("active")));
		if (const Arr* B = ArrayOf(Field(*O, TEXT("bunkers")))) for (int32 I = 0; I < B->Num() && I < TCMatchLimits::Seats; I++) R.Bunkers.Add(IntOr((*B)[I], 0));
		R.Profile = StrField(*O, TEXT("profile")); R.NextProfile = StrField(*O, TEXT("nextProfile")); R.Goal = StrField(*O, TEXT("goal"));
		if (const Arr* B = ArrayOf(Field(*O, TEXT("boss"))))
			if (B->Num() >= 2) { R.bBoss = true; R.BossHp = IntOr((*B)[0], 0); R.BossMax = IntOr((*B)[1], 0); }
	}

	// world: the score strip's owned / goal / total and region teams (the rest is read by Client/TCWorldRecovery)
	WorldScore = FTCWorldScore();
	if (const FJsonObject* O = ObjectOf(Field(M, TEXT("world"))))
	{
		WorldScore.bPresent = true;
		if (HasNum(*O, TEXT("owned"), D)) WorldScore.Owned = (int32)D;
		if (HasNum(*O, TEXT("goal"), D)) WorldScore.Goal = (int32)D;
		if (HasNum(*O, TEXT("total"), D)) WorldScore.Total = (int32)D;
		if (const Arr* Regions = ArrayOf(Field(*O, TEXT("regions"))))
			for (int32 I = 0; I < Regions->Num() && I < TCMatchLimits::Ghosts; I++)
			{
				const FJsonObject* Region = ObjectOf((*Regions)[I]);
				WorldScore.RegionTeams.Add(Region ? (int32)NumField(*Region, TEXT("team"), -1.0) : -1);
			}
	}

	// scenario: { version, objectives: [{ id, state, text (a string or { en, es }), ... }] }
	Scenario.Reset(); bHasScenario = false;
	if (const FJsonObject* O = ObjectOf(Field(M, TEXT("scenario"))))
	{
		bHasScenario = true;
		if (const Arr* A = ArrayOf(Field(*O, TEXT("objectives"))))
		{
			NoteCap(A->Num() - TCMatchLimits::Objectives);
			for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Objectives; I++)
			{
				const FJsonObject* Obj = ObjectOf((*A)[I]);
				if (!Obj) { NoteMalformed(); continue; }
				FTCScenarioObjective S;
				S.Id = StrField(*Obj, TEXT("id")); S.State = StrField(*Obj, TEXT("state"));
				// objectives.js: typeof text === 'string' ? text : text?.[lang] ?? text?.en ?? '' (FTCLocale::ObjectiveText, in
				// the language in use: English in tests and unattended runs unless -TCLang says otherwise)
				S.Text = Clip(FTCLocale::ObjectiveText(Field(*Obj, TEXT("text"))));
				Scenario.Add(MoveTemp(S));
			}
		}
	}

	// foundMines: enemy mine cells this team's builders found
	FoundMines.Reset();
	if (const Arr* A = ArrayOf(Field(M, TEXT("foundMines"))))
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const int32 C = IntOr(V, -1);
			if (!Map.IsValidCell(C)) { NoteMalformed(); continue; }
			if (FoundMines.Num() >= TCMatchLimits::FoundMines) { NoteCap(1); continue; }
			FoundMines.Add(C);
		}

	// end: { reason, x, z } while there is a winner
	End = FTCEndRow();
	if (const FJsonObject* O = ObjectOf(Field(M, TEXT("end"))))
	{
		End.bPresent = true;
		End.Reason = StrField(*O, TEXT("reason"));
		End.X = NumField(*O, TEXT("x"), 0.0); End.Z = NumField(*O, TEXT("z"), 0.0);
	}
}

FString FTCMatchState::DescribeExtras() const
{
	using namespace TCMatchDecode;
	TArray<FString> Owners, VpText, OnlineText, PingText;
	for (const FTCPointRow& P : Points) Owners.Add(FString::Printf(TEXT("%d%s%s"), P.Owner, P.bContested ? TEXT("!") : TEXT(""), P.Owner < 0 && P.Capper >= 0 ? *FString::Printf(TEXT("<%d %.1f"), P.Capper, P.Progress) : TEXT("")));
	for (int32 V : Vp) VpText.Add(FString::FromInt(V));
	for (const TOptional<bool>& O : Online) OnlineText.Add(!O.IsSet() ? TEXT("?") : O.GetValue() ? TEXT("on") : TEXT("off"));
	for (const TOptional<int32>& P : Ping) PingText.Add(!P.IsSet() ? TEXT("-") : P.GetValue() == -1 ? TEXT("AI") : FString::FromInt(P.GetValue()));
	auto Opt = [](const TOptional<double>& V) { return V.IsSet() ? FTCObjectives::JsNumber(V.GetValue()) : FString(TEXT("-")); };
	return FString::Printf(TEXT("tick %d, mode %s, points [%s], vp [%s], online [%s], ping [%s], plans %d, orders %d, ghosts %d, shots %d, flights %d, fires %d, falling %d, weather %s%s, wx %s, inc %s, fuelInc %s, upkeep %s, air %d, scenario %s, foundMines %d, winner %s%s"),
		Tick, *ModeRow.Kind, *FString::Join(Owners, TEXT(",")), *FString::Join(VpText, TEXT(",")), *FString::Join(OnlineText, TEXT(",")), *FString::Join(PingText, TEXT(",")),
		Plans.Num(), WaitingOrders.Num(), Ghosts.Num(), Shots.Num(), Flights.Num(), Fires.Num(), Falling.Num(), *Weather.Now,
		Weather.HasNext() ? *FString::Printf(TEXT(" -> %s in %d s"), *Weather.Next, Weather.SecondsLeft) : TEXT(""),
		Wx.bKnown ? *FString::Printf(TEXT("[%s, %s, %s, %s]"), *FTCObjectives::JsNumber(Wx.Rain), *FTCObjectives::JsNumber(Wx.Wet), *FTCObjectives::JsNumber(Wx.WindAngle), *FTCObjectives::JsNumber(Wx.WindStrength)) : TEXT("-"),
		*Opt(Inc), *Opt(FuelInc), *Opt(Upkeep), Air.Num(), bHasScenario ? *FString::FromInt(Scenario.Num()) : TEXT("-"), FoundMines.Num(),
		Winner >= -1 ? *FString::FromInt(Winner) : TEXT("none"), End.bPresent ? *FString::Printf(TEXT(" (%s at %.1f, %.1f)"), *End.Reason, End.X, End.Z) : TEXT(""));
}

void FTCMatchState::LogExtrasTransitions()
{
	using namespace TCMatchDecode;
	// Low volume on purpose: the first snapshot of a match, a capture point changing hands, the weather turning, the
	// match being decided.
	const int32 Hash = OwnersHash(Points);
	const FString WeatherNow = Weather.Now + TEXT("|") + Weather.Next;
	if (!bLoggedFirstExtras)
	{
		bLoggedFirstExtras = true; LoggedPointOwners = Hash; LoggedWeather = WeatherNow;
		UE_LOG(LogTC, Log, TEXT("snapshot fields: %s"), *DescribeExtras());
	}
	else
	{
		if (Hash != LoggedPointOwners) { LoggedPointOwners = Hash; UE_LOG(LogTC, Log, TEXT("capture points changed hands: %s"), *DescribeExtras()); }
		if (WeatherNow != LoggedWeather) { LoggedWeather = WeatherNow; UE_LOG(LogTC, Log, TEXT("weather: %s%s"), *Weather.Now, Weather.HasNext() ? *FString::Printf(TEXT(", turning to %s in %d s"), *Weather.Next, Weather.SecondsLeft) : TEXT("")); }
	}
	if (!bLoggedDecided && Winner >= -1)
	{
		bLoggedDecided = true;
		UE_LOG(LogTC, Log, TEXT("match decided: %s; %s"), *Objectives.Epilogue().Word, *DescribeExtras());
	}
}
