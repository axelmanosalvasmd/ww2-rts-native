#include "Client/TCMatchControls.h"
#include "Client/TCLocale.h"
#include "Net/TCConnection.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Math/UnrealMathUtility.h"
#include "Containers/StringConv.h"
#include "Misc/AutomationTest.h"

namespace TCCtlL
{
// BEGIN GENERATED UNITS (round-15 lobby tools/write-units.mjs: shared/sim.js UNITS in Object.keys order without
// structures, as client/main.js fills #sbUnit; reference e4e579a)
	const FTCSandboxUnit ReferenceUnits[] = {
		{ TEXT("rifle"), TEXT("Rifle Squad") },
		{ TEXT("mg"), TEXT("MG Team") },
		{ TEXT("at"), TEXT("AT Gun") },
		{ TEXT("tank"), TEXT("Light Tank") },
		{ TEXT("rocket"), TEXT("Rocket Launcher") },
		{ TEXT("flak"), TEXT("Flak Gun") },
		{ TEXT("fighter"), TEXT("Fighter") },
		{ TEXT("attacker"), TEXT("Ground-attack Plane") },
		{ TEXT("flaktrack"), TEXT("Mobile Flak") },
		{ TEXT("mortar"), TEXT("Mortar Team") },
		{ TEXT("sniper"), TEXT("Sniper") },
		{ TEXT("armoredcar"), TEXT("Armored Car") },
		{ TEXT("medium"), TEXT("Medium Tank") },
		{ TEXT("halftrack"), TEXT("Halftrack") },
		{ TEXT("lorry"), TEXT("Troop Truck") },
		{ TEXT("train"), TEXT("Train") },
		{ TEXT("lcvp"), TEXT("Landing Craft") },
		{ TEXT("gunboat"), TEXT("Gunboat") },
		{ TEXT("destroyer"), TEXT("Destroyer") },
		{ TEXT("medic"), TEXT("Medic Team") },
		{ TEXT("tankdestroyer"), TEXT("Tank Destroyer") },
		{ TEXT("howitzer"), TEXT("Field Howitzer") },
		{ TEXT("flamer"), TEXT("Flamethrower Squad") },
		{ TEXT("bomber"), TEXT("Bomber") },
		{ TEXT("ranger"), TEXT("Ranger Squad") },
		{ TEXT("tiger"), TEXT("Tiger") },
		{ TEXT("conscript"), TEXT("Conscripts") },
		{ TEXT("churchill"), TEXT("Churchill") },
		{ TEXT("commando"), TEXT("Commandos") },
		{ TEXT("kaiju"), TEXT("Kaiju") },
		{ TEXT("engineer"), TEXT("Engineer Squad") },
		{ TEXT("truck"), TEXT("Supply Truck") },
	};
// END GENERATED UNITS

	// the units the owner keeps out of the sandbox panel (round 15): a spawned Supply Truck crashes the pinned server
	bool Excluded(const TCHAR* Type) { return FCString::Strcmp(Type, TEXT("truck")) == 0; }
	const TArray<FTCSandboxUnit>& Panel()
	{
		static const TArray<FTCSandboxUnit> L = [] { TArray<FTCSandboxUnit> A; for (const FTCSandboxUnit& U : ReferenceUnits) if (!Excluded(U.Type)) A.Add(U); return A; }();
		return L;
	}

	// client/main.js observerControls, and what an operative may send in play
	const TCHAR* const ObserverControls[] = { TEXT("fps"), TEXT("ping"), TEXT("resync"), TEXT("name"), TEXT("pause"), TEXT("resume"), TEXT("restart"), TEXT("end"), TEXT("leave"), TEXT("handAi"), TEXT("save") };
	const TCHAR* const OperativeControls[] = { TEXT("fps"), TEXT("ping"), TEXT("resync"), TEXT("name") };

	bool Str(const FJsonObject& O, const TCHAR* K, FString& Out)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(K);
		if (!V.IsValid() || V->Type != EJson::String) return false;
		Out = V->AsString();
		return true;
	}
	bool Flag(const FJsonObject& O, const TCHAR* K)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(K);
		return V.IsValid() && V->Type == EJson::Boolean && V->AsBool();
	}
	int32 Int(const FJsonObject& O, const TCHAR* K, int32 Default)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(K);
		if (!V.IsValid() || V->Type != EJson::Number) return Default;
		const double D = V->AsNumber();
		return D == FMath::FloorToDouble(D) && FMath::Abs(D) < 1e9 ? (int32)D : Default;
	}
	bool JsSpace(TCHAR C)
	{
		return C == ' ' || C == '\t' || C == '\n' || C == '\r' || C == 0x0B || C == 0x0C || C == 0xA0 || C == 0xFEFF || C == 0x1680
			|| (C >= 0x2000 && C <= 0x200A) || C == 0x2028 || C == 0x2029 || C == 0x202F || C == 0x205F || C == 0x3000;
	}
	FString JsTrim(const FString& S)
	{
		int32 A = 0, B = S.Len();
		while (A < B && JsSpace(S[A])) A++;
		while (B > A && JsSpace(S[B - 1])) B--;
		return S.Mid(A, B - A);
	}
	// the host part of a socket URL (without the port), empty when it is not ws:// or wss://
	FString HostOf(const FString& SocketUrl, FString* OutAuthority = nullptr, bool* OutTls = nullptr)
	{
		FString Rest;
		bool bTls = false;
		if (SocketUrl.StartsWith(TEXT("ws://"), ESearchCase::IgnoreCase)) Rest = SocketUrl.Mid(5);
		else if (SocketUrl.StartsWith(TEXT("wss://"), ESearchCase::IgnoreCase)) { Rest = SocketUrl.Mid(6); bTls = true; }
		else return FString();
		int32 Cut = Rest.Len();
		for (int32 I = 0; I < Rest.Len(); I++) if (Rest[I] == '/' || Rest[I] == '?' || Rest[I] == '#') { Cut = I; break; }
		const FString Authority = Rest.Left(Cut);
		if (OutAuthority) *OutAuthority = Authority;
		if (OutTls) *OutTls = bTls;
		if (Authority.StartsWith(TEXT("["))) { int32 Close = INDEX_NONE; return Authority.FindChar(']', Close) ? Authority.Left(Close + 1) : Authority; }
		int32 Colon = INDEX_NONE;
		return Authority.FindChar(':', Colon) ? Authority.Left(Colon) : Authority;
	}
}

const TCHAR* LexToString(ETCMatchControl Kind)
{
	switch (Kind)
	{
	case ETCMatchControl::Watch: return TEXT("Watch");
	case ETCMatchControl::Rename: return TEXT("Rename");
	case ETCMatchControl::CopyLink: return TEXT("CopyLink");
	case ETCMatchControl::JoinRoom: return TEXT("JoinRoom");
	case ETCMatchControl::NewRoom: return TEXT("NewRoom");
	case ETCMatchControl::Menu: return TEXT("Menu");
	case ETCMatchControl::Pause: return TEXT("Pause");
	case ETCMatchControl::Save: return TEXT("Save");
	case ETCMatchControl::Restart: return TEXT("Restart");
	case ETCMatchControl::End: return TEXT("End");
	case ETCMatchControl::HandAi: return TEXT("HandAi");
	case ETCMatchControl::Leave: return TEXT("Leave");
	case ETCMatchControl::NextWave: return TEXT("NextWave");
	case ETCMatchControl::SandboxUnit: return TEXT("SandboxUnit");
	case ETCMatchControl::SandboxSide: return TEXT("SandboxSide");
	case ETCMatchControl::SandboxSpawn: return TEXT("SandboxSpawn");
	}
	return TEXT("?");
}

bool LexFromString(ETCMatchControl& Out, const TCHAR* Text)
{
	for (uint8 I = 0; I <= (uint8)ETCMatchControl::SandboxSpawn; I++)
		if (FCString::Strcmp(LexToString((ETCMatchControl)I), Text) == 0) { Out = (ETCMatchControl)I; return true; }
	return false;
}

TFunction<bool(const FString&)> FTCMatchControls::Clipboard;

TSharedRef<FJsonObject> FTCMatchControls::Msg(const TCHAR* Type)
{
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), Type);
	return M;
}

bool FTCMatchControls::ApplyLobby(const FJsonObject& M)
{
	FString T, NewState;
	if (!TCCtlL::Str(M, TEXT("t"), T) || T != TEXT("lobby") || !TCCtlL::Str(M, TEXT("state"), NewState)) return false;
	const TArray<TSharedPtr<FJsonValue>>* Players = nullptr;
	if (!M.TryGetArrayField(TEXT("players"), Players) || !Players) return false;
	TArray<FSeat> NewSeats;
	for (const TSharedPtr<FJsonValue>& V : *Players)
	{
		if (!V.IsValid() || V->Type != EJson::Object) return false;
		const FJsonObject& P = *V->AsObject();
		FSeat S;
		TCCtlL::Str(P, TEXT("name"), S.Name);
		S.bAi = TCCtlL::Flag(P, TEXT("ai"));
		S.bConnected = TCCtlL::Flag(P, TEXT("connected"));
		NewSeats.Add(MoveTemp(S));
	}
	TArray<FString> NewSpectators;
	const TArray<TSharedPtr<FJsonValue>>* Watchers = nullptr;
	if (M.TryGetArrayField(TEXT("spectators"), Watchers) && Watchers)
		for (const TSharedPtr<FJsonValue>& V : *Watchers) if (V.IsValid() && V->Type == EJson::String) NewSpectators.Add(V->AsString());

	bHasLobby = true;
	bRustHost = FTCConnection::HostKindOf(M) == ETCHostKind::Rust;
	State = NewState;
	SeatList = MoveTemp(NewSeats);
	SpectatorNames = MoveTemp(NewSpectators);
	if (!TCCtlL::Str(M, TEXT("role"), Role)) Role = TEXT("commander");
	if (!TCCtlL::Str(M, TEXT("mode"), Mode)) Mode.Reset();
	if (!TCCtlL::Str(M, TEXT("code"), Code)) Code.Reset();
	if (!TCCtlL::Str(M, TEXT("publicUrl"), PublicUrlText)) PublicUrlText.Reset();
	You = TCCtlL::Int(M, TEXT("you"), -1);
	Host = TCCtlL::Int(M, TEXT("host"), -1);
	Spawns = TCCtlL::Int(M, TEXT("spawns"), -1);
	bAmHost = TCCtlL::Flag(M, TEXT("amHost"));
	bListed = TCCtlL::Flag(M, TEXT("listed"));
	bWatching = TCCtlL::Flag(M, TEXT("spectator")); // renderLobby: watching = !!m.spectator

	// pending requests: Watch is done once the lobby shows the side asked; Rename once this client's name shows
	if (State != TEXT("lobby")) bWatchPending = false;
	else if (bWatchPending && bWatching == bWatchAskedSpectate) { bWatchPending = false; ConfirmedCount++; }
	if (!PendingName.IsEmpty())
	{
		const FString Want = CleanName(PendingName);
		const bool bShown = (You >= 0 && SeatList.IsValidIndex(You) && SeatList[You].Name == Want) || (bWatching && SpectatorNames.Contains(Want));
		if (bShown) { PendingName.Reset(); ConfirmedCount++; }
	}
	// renderLobby, back in the lobby: the menu closes, the pause clears, the match's aims go (and the confirm labels
	// return with their timers)
	if (State == TEXT("lobby"))
	{
		bMenuOpen = false; bPaused = false; bSpawnArmed = false; bSandboxMatch = false; bOut = false; bHorde = false; bWaveOn = false;
		RestartArmedAt = EndArmedAt = LeaveArmedAt = -1e9;
	}
	return true;
}

void FTCMatchControls::Reset()
{
	const FString KeepName = FieldName;
	const int32 KeepUnit = SandboxIndex;
	const bool bKeepEnemy = bSpawnEnemy;
	*this = FTCMatchControls();
	FieldName = KeepName; SandboxIndex = KeepUnit; bSpawnEnemy = bKeepEnemy; // form values outlive a room
}

void FTCMatchControls::OnStart(bool bSandbox)
{
	bSandboxMatch = bSandbox;
	bSpawnArmed = false;  // a new match clears aims
	bMenuOpen = false;
	bOut = false;
}

bool FTCMatchControls::Sendable(const FString& Type) const
{
	if (!InPlay()) return true;
	auto In = [&](TArrayView<const TCHAR* const> L) { for (const TCHAR* S : L) if (Type == S) return true; return false; };
	if (Operative() && !In(TCCtlL::OperativeControls)) return false;
	if (Observing() && !In(TCCtlL::ObserverControls)) return false;
	return true;
}

int32 FTCMatchControls::SeatLimit() const
{
	if (Mode == TEXT("world")) return MaxPlayers;
	return bListed ? FMath::Min(MaxPlayers, Spawns >= 0 ? Spawns : 3) : MaxPlayers;
}

// --- Watch ---------------------------------------------------------------------------------------------------------

FTCControlButton FTCMatchControls::WatchButton() const
{
	FTCControlButton B;
	B.Kind = ETCMatchControl::Watch;
	B.Label = bWatching ? TEXT("Take a seat") : TEXT("Watch as a spectator");
	B.Title = Mode == TEXT("world") ? TEXT("Spectators share one team's explored world and command nothing")
		: TEXT("Spectators see the whole map and command nothing. With only AI players seated, a spectator hosts");
	B.bShown = bHasLobby && !Operative() && InLobby() && !(bWatching && SeatList.Num() >= SeatLimit());
	B.bPending = bWatchPending;
	if (!B.bShown) B.Why = !bHasLobby ? TEXT("no lobby yet") : Operative() ? TEXT("an operative has no seat to give up") : !InLobby() ? TEXT("only in the lobby") : TEXT("no seat is free");
	return B;
}

FTCControlRequest FTCMatchControls::PlanWatch() const
{
	FTCControlRequest R;
	R.Kind = ETCMatchControl::Watch;
	const FTCControlButton B = WatchButton();
	if (!B.bShown) { R.Note = B.Why; return R; }
	const TCHAR* Type = bWatching ? TEXT("sit") : TEXT("spectate");
	if (!Sendable(Type)) { R.Note = TEXT("sendCmd drops it"); return R; }
	R.Message = Msg(Type);
	R.Note = bWatching ? TEXT("take a seat") : TEXT("watch as a spectator");
	return R;
}

// --- Name ----------------------------------------------------------------------------------------------------------

FString FTCMatchControls::InitialName(const FString& Given, const FString& Saved, int32 RandomTwoDigits)
{
	// $('name').value = entry.get('name')?.slice(0, 16) || localStorage 'ww2-name' || 'Soldier' + (10..99)
	if (!Given.IsEmpty()) return Given.Left(NameMax);
	if (!Saved.IsEmpty()) return Saved;
	return FString::Printf(TEXT("Soldier%d"), FMath::Clamp(RandomTwoDigits, 10, 99));
}

FString FTCMatchControls::CleanName(const FString& Name)
{
	// server.js: String(n || '').replace(/[<>&"']/g, '').trim().slice(0, 16) || 'Soldier'
	FString S;
	for (const TCHAR C : Name) if (!(C == '<' || C == '>' || C == '&' || C == '"' || C == '\'')) S.AppendChar(C);
	S = TCCtlL::JsTrim(S).Left(NameMax);
	return S.IsEmpty() ? FString(TEXT("Soldier")) : S;
}

FTCControlRequest FTCMatchControls::PlanRename(const FString& Typed)
{
	FTCControlRequest R;
	R.Kind = ETCMatchControl::Rename;
	const FString V = Typed.Left(NameMax);
	// the input's change event fires only when the value changed
	if (V == FieldName) { R.Note = TEXT("the name did not change"); return R; }
	FieldName = V;
	if (!Sendable(TEXT("name"))) { R.Note = TEXT("sendCmd drops it"); return R; }
	R.Message = Msg(TEXT("name"));
	R.Message->SetStringField(TEXT("name"), V);
	R.Note = FString::Printf(TEXT("rename to '%s' (the server keeps '%s')"), *V, *CleanName(V));
	return R;
}

FTCControlButton FTCMatchControls::NameField() const
{
	FTCControlButton B;
	B.Kind = ETCMatchControl::Rename;
	B.Label = FieldName;
	B.Title = TEXT("Name");
	B.bShown = true;
	B.bPending = !PendingName.IsEmpty();
	return B;
}

// --- invite link ---------------------------------------------------------------------------------------------------

FString FTCMatchControls::Origin(const FString& SocketUrl)
{
	FString Authority;
	bool bTls = false;
	if (TCCtlL::HostOf(SocketUrl, &Authority, &bTls).IsEmpty()) return FString();
	return (bTls ? TEXT("https://") : TEXT("http://")) + Authority;
}

bool FTCMatchControls::IsLocalHost(const FString& SocketUrl)
{
	// /^(localhost|127\.|\[::1\])/.test(location.hostname)
	const FString H = TCCtlL::HostOf(SocketUrl);
	return H.StartsWith(TEXT("localhost"), ESearchCase::CaseSensitive) || H.StartsWith(TEXT("127."), ESearchCase::CaseSensitive) || H.StartsWith(TEXT("[::1]"), ESearchCase::CaseSensitive);
}

FString FTCMatchControls::InviteLink(const FString& SocketUrl, const FString& Room) const
{
	const FString Own = Origin(SocketUrl);
	if (Own.IsEmpty()) return FString();
	// before the first lobby the page shows roomLink(location.origin); renderLobby swaps in publicUrl when local
	const bool bPublic = bHasLobby && IsLocalHost(SocketUrl) && !PublicUrlText.IsEmpty();
	return RoomLink(bPublic ? PublicUrlText : Own, Room);
}

FTCControlRequest FTCMatchControls::CopyLink(const FString& Link, double Now)
{
	FTCControlRequest R;
	R.Kind = ETCMatchControl::CopyLink;
	bool bOk = false;
	if (!Link.IsEmpty() && Clipboard) bOk = Clipboard(Link);
	NoteCopied(bOk, Now);
	R.Note = bOk ? TEXT("copied the invite link") : TEXT("the clipboard did not take the link: Press Ctrl+C");
	return R;
}

FTCControlButton FTCMatchControls::CopyButton(double Now) const
{
	FTCControlButton B;
	B.Kind = ETCMatchControl::CopyLink;
	B.Title = TEXT("Copy the invite link");
	B.bShown = true;
	B.Label = Now - CopiedAt < CopySeconds ? (bCopyOk ? TEXT("Copied") : TEXT("Press Ctrl+C")) : TEXT("Copy");
	return B;
}

// --- Room box ------------------------------------------------------------------------------------------------------

FTCControlRequest FTCMatchControls::PlanJoinRoom(const FString& Typed, const FString& CurrentRoom)
{
	FTCControlRequest R;
	R.Kind = ETCMatchControl::JoinRoom;
	// goRoom: code.trim().toLowerCase(), then /^[a-z0-9]{3,12}$/ (the box holds at most 12 characters)
	FString Code = TCCtlL::JsTrim(Typed.Left(12)).ToLower();
	bool bOk = Code.Len() >= 3 && Code.Len() <= 12;
	for (const TCHAR C : Code) bOk = bOk && ((C >= 'a' && C <= 'z') || (C >= '0' && C <= '9'));
	if (!bOk) { R.Note = FString::Printf(TEXT("not a room code: the box shows %s again"), *CurrentRoom); return R; }
	R.Room = Code;
	R.Note = FString::Printf(TEXT("go to room %s"), *Code);
	return R;
}

FTCControlRequest FTCMatchControls::PlanNewRoom(TFunctionRef<int32(int32)> RandomBelow)
{
	static const TCHAR* const Digits = TEXT("0123456789abcdefghijklmnopqrstuvwxyz");
	FString Code;
	for (int32 I = 0; I < 5; I++) Code.AppendChar(Digits[FMath::Clamp(RandomBelow(36), 0, 35)]);
	FTCControlRequest R = PlanJoinRoom(Code, FString());
	R.Kind = ETCMatchControl::NewRoom;
	R.Note = FString::Printf(TEXT("a new private room %s"), *Code);
	return R;
}

FTCControlRequest FTCMatchControls::PlanNewRoom()
{
	return PlanNewRoom([](int32 N) { return FMath::RandRange(0, N - 1); });
}

// --- match menu ----------------------------------------------------------------------------------------------------

FTCControlButton FTCMatchControls::MenuButton() const
{
	FTCControlButton B;
	B.Kind = ETCMatchControl::Menu;
	B.Label = TEXT("Menu");
	B.Title = TEXT("Menu: restart, end or leave the match");
	B.bShown = InPlay();
	return B;
}

FTCControlRequest FTCMatchControls::ToggleMenu()
{
	FTCControlRequest R;
	R.Kind = ETCMatchControl::Menu;
	bMenuOpen = !bMenuOpen;
	R.Note = bMenuOpen ? TEXT("menu open") : TEXT("menu closed");
	return R;
}

double& FTCMatchControls::ArmedAt(ETCMatchControl Kind)
{
	return Kind == ETCMatchControl::Restart ? RestartArmedAt : Kind == ETCMatchControl::End ? EndArmedAt : LeaveArmedAt;
}

double FTCMatchControls::ArmedAtConst(ETCMatchControl Kind) const
{
	return Kind == ETCMatchControl::Restart ? RestartArmedAt : Kind == ETCMatchControl::End ? EndArmedAt : Kind == ETCMatchControl::Leave ? LeaveArmedAt : -1e9;
}

FTCControlButton FTCMatchControls::MenuControl(ETCMatchControl Kind, int32 Slot, double Now) const
{
	FTCControlButton B;
	B.Kind = Kind;
	B.Slot = Slot;
	const bool bHost = AmHost();
	const TCHAR* Type = TEXT("");
	switch (Kind)
	{
	case ETCMatchControl::Pause:
		B.bShown = bHost; B.Label = bPaused ? TEXT("Resume match") : TEXT("Pause match"); Type = bPaused ? TEXT("resume") : TEXT("pause");
		break;
	case ETCMatchControl::Save:
		B.bShown = bHost; B.Label = TEXT("Save match"); B.Title = TEXT("Save this match to load it again later from this room's lobby"); Type = TEXT("save");
		break;
	case ETCMatchControl::Restart:
		B.bShown = bHost; B.Label = TEXT("Restart match"); Type = TEXT("restart");
		break;
	case ETCMatchControl::End:
		B.bShown = bHost; B.Label = TEXT("End match (back to lobby)"); Type = TEXT("end");
		break;
	case ETCMatchControl::Leave:
		B.bShown = !bWatching; B.Label = TEXT("Leave game"); Type = TEXT("leave"); // a spectator has no army to hand over
		break;
	case ETCMatchControl::HandAi:
		B.bShown = bHost && InPlay() && SeatList.IsValidIndex(Slot) && !SeatList[Slot].bAi && !SeatList[Slot].bConnected;
		B.Label = FString::Printf(TEXT("Hand %s to AI"), SeatList.IsValidIndex(Slot) ? *SeatList[Slot].Name : TEXT("?"));
		Type = TEXT("handAi");
		break;
	default:
		return B;
	}
	if ((Kind == ETCMatchControl::Restart || Kind == ETCMatchControl::End || Kind == ETCMatchControl::Leave) && Now - ArmedAtConst(Kind) < ConfirmSeconds)
		B.Label = TEXT("Click again to confirm");
	if (!B.bShown) B.Why = Kind == ETCMatchControl::Leave ? TEXT("a spectator has no army to hand over") : Kind == ETCMatchControl::HandAi ? TEXT("only the host, in a match, for an offline player") : TEXT("only the host");
	else if (!Sendable(Type)) { B.bEnabled = false; B.Why = TEXT("an operative sends no match controls"); }
	else if (bRustHost && Kind == ETCMatchControl::HandAi)
	{
		B.bEnabled = false;
		B.Why = TEXT("Rust host: no AI commanders.");
	}
	return B;
}

TArray<FTCControlButton> FTCMatchControls::MenuButtons(double Now) const
{
	TArray<FTCControlButton> L;
	L.Add(MenuControl(ETCMatchControl::Pause, -1, Now));
	L.Add(MenuControl(ETCMatchControl::Save, -1, Now));
	L.Add(MenuControl(ETCMatchControl::Restart, -1, Now));
	L.Add(MenuControl(ETCMatchControl::End, -1, Now));
	for (int32 I = 0; I < SeatList.Num(); I++) L.Add(MenuControl(ETCMatchControl::HandAi, I, Now));
	L.Add(MenuControl(ETCMatchControl::Leave, -1, Now));
	return L;
}

FTCControlRequest FTCMatchControls::PressMenu(ETCMatchControl Kind, int32 Slot, double Now)
{
	FTCControlRequest R;
	R.Kind = Kind;
	R.Slot = Slot;
	const FTCControlButton B = MenuControl(Kind, Slot, Now);
	if (!B.bShown || !B.bEnabled) { R.Note = B.Why.IsEmpty() ? FString(TEXT("not a menu button")) : B.Why; return R; }
	auto Out = [&](const TCHAR* Type)
	{
		if (!Sendable(Type)) { R.Note = TEXT("sendCmd drops it"); return; }
		R.Message = Msg(Type);
		R.Note = FString::Printf(TEXT("sent %s"), Type);
	};
	switch (Kind)
	{
	case ETCMatchControl::Pause:
		Out(bPaused ? TEXT("resume") : TEXT("pause"));
		R.bCloseMenu = true;
		break;
	case ETCMatchControl::Save:
		Out(TEXT("save"));
		R.bCloseMenu = true;
		break;
	case ETCMatchControl::HandAi:
		Out(TEXT("handAi"));
		if (R.Message.IsValid()) R.Message->SetNumberField(TEXT("slot"), Slot);
		break;
	case ETCMatchControl::Restart:
	case ETCMatchControl::End:
	case ETCMatchControl::Leave:
	{
		double& At = ArmedAt(Kind);
		if (Now - At < ConfirmSeconds)
		{
			At = -1e9;
			Out(Kind == ETCMatchControl::Restart ? TEXT("restart") : Kind == ETCMatchControl::End ? TEXT("end") : TEXT("leave"));
			R.bCloseMenu = true;
		}
		else
		{
			At = Now;
			R.bArmed = true;
			R.Note = TEXT("click again within 3 s to confirm");
		}
		break;
	}
	default:
		break;
	}
	if (R.bCloseMenu) bMenuOpen = false;
	return R;
}

// --- Horde ---------------------------------------------------------------------------------------------------------

FTCControlButton FTCMatchControls::NextWaveButton() const
{
	FTCControlButton B;
	B.Kind = ETCMatchControl::NextWave;
	B.Label = TEXT("Send next wave");
	B.Title = TEXT("Skip the break");
	// hud.js: show(button, !on && ctx.host), on Horde's score strip during a match
	B.bShown = InPlay() && bHorde && !bWaveOn && AmHost();
	if (B.bShown && !Sendable(TEXT("nextwave"))) { B.bEnabled = false; B.Why = TEXT("an observer's clicks send nothing in a match"); }
	if (!B.bShown) B.Why = !bHorde ? TEXT("Horde only") : bWaveOn ? TEXT("a wave is on the map") : TEXT("only the host");
	return B;
}

FTCControlRequest FTCMatchControls::PlanNextWave() const
{
	FTCControlRequest R;
	R.Kind = ETCMatchControl::NextWave;
	const FTCControlButton B = NextWaveButton();
	if (!B.bShown || !B.bEnabled) { R.Note = B.Why; return R; }
	R.Message = Msg(TEXT("nextwave"));
	R.Note = TEXT("send the next wave");
	return R;
}

// --- sandbox -------------------------------------------------------------------------------------------------------

TArrayView<const FTCSandboxUnit> FTCMatchControls::SandboxUnits() { return TCCtlL::Panel(); }
TArrayView<const FTCSandboxUnit> FTCMatchControls::ReferenceSandboxUnits() { return MakeArrayView(TCCtlL::ReferenceUnits, UE_ARRAY_COUNT(TCCtlL::ReferenceUnits)); }

const FTCSandboxUnit& FTCMatchControls::SandboxUnit() const
{
	const TArrayView<const FTCSandboxUnit> L = SandboxUnits();
	return L[FMath::Clamp(SandboxIndex, 0, L.Num() - 1)];
}

void FTCMatchControls::StepSandboxUnit(int32 Direction)
{
	const int32 N = SandboxUnits().Num();
	SandboxIndex = ((SandboxIndex + (Direction < 0 ? -1 : 1)) % N + N) % N;
}

bool FTCMatchControls::SetSandboxUnit(const FString& Type)
{
	const TArrayView<const FTCSandboxUnit> L = SandboxUnits();
	for (int32 I = 0; I < L.Num(); I++) if (Type == L[I].Type) { SandboxIndex = I; return true; }
	return false;
}

FTCControlRequest FTCMatchControls::ArmSpawn()
{
	FTCControlRequest R;
	R.Kind = ETCMatchControl::SandboxSpawn;
	if (!SandboxShown()) { R.Note = TEXT("no sandbox panel"); return R; }
	bSpawnArmed = true;
	R.Note = FString::Printf(TEXT("spawn armed: %s, %s; click the map once per unit, right-click stops"), SandboxUnit().Type, bSpawnEnemy ? TEXT("enemy") : TEXT("mine"));
	return R;
}

FTCControlRequest FTCMatchControls::PlanSpawn(double X, double Z) const
{
	FTCControlRequest R;
	R.Kind = ETCMatchControl::SandboxSpawn;
	if (!bSpawnArmed || !SandboxShown()) { R.Note = TEXT("the spawn aim is not armed"); return R; }
	const FTCSandboxUnit& U = SandboxUnit();
	if (TCCtlL::Excluded(U.Type)) { R.Note = TEXT("supply trucks are not spawned (owner decision)"); return R; }
	if (!Sendable(TEXT("spawn"))) { R.Note = TEXT("sendCmd drops it: this client is observing"); return R; }
	R.Message = Msg(TEXT("spawn"));
	R.Message->SetStringField(TEXT("unit"), U.Type);
	R.Message->SetBoolField(TEXT("enemy"), bSpawnEnemy);
	R.Message->SetNumberField(TEXT("x"), X);
	R.Message->SetNumberField(TEXT("z"), Z);
	R.Note = FString::Printf(TEXT("spawn %s (%s) at %.1f, %.1f"), U.Type, bSpawnEnemy ? TEXT("enemy") : TEXT("mine"), X, Z);
	return R;
}

TArray<FTCControlButton> FTCMatchControls::SandboxButtons() const
{
	TArray<FTCControlButton> L;
	FTCControlButton Unit; Unit.Kind = ETCMatchControl::SandboxUnit; Unit.Label = SandboxUnit().Name; Unit.bShown = SandboxShown();
	FTCControlButton Side; Side.Kind = ETCMatchControl::SandboxSide; Side.Label = bSpawnEnemy ? TEXT("Enemy") : TEXT("Mine"); Side.bShown = SandboxShown();
	FTCControlButton Go; Go.Kind = ETCMatchControl::SandboxSpawn; Go.Label = TEXT("Spawn"); Go.Title = TEXT("Then click the map, once per unit. Right-click stops"); Go.bShown = SandboxShown();
	Go.bPending = bSpawnArmed; // drawn pressed while the aim is armed
	L.Add(Unit); L.Add(Side); L.Add(Go);
	return L;
}

void FTCMatchControls::NoteSent(const FTCControlRequest& R)
{
	if (!R.Message.IsValid()) return;
	FString T;
	R.Message->TryGetStringField(TEXT("t"), T);
	if (T == TEXT("spectate") || T == TEXT("sit")) { bWatchPending = true; bWatchAskedSpectate = T == TEXT("spectate"); }
	else if (T == TEXT("name")) R.Message->TryGetStringField(TEXT("name"), PendingName);
}


// --- text fields ---------------------------------------------------------------------------------------------------

const TCHAR* LexToString(ETCTextField Field)
{
	switch (Field)
	{
	case ETCTextField::Name: return TEXT("Name");
	case ETCTextField::RoomCode: return TEXT("RoomCode");
	case ETCTextField::Nickname: return TEXT("Nickname");
	case ETCTextField::MatchTitle: return TEXT("MatchTitle");
	case ETCTextField::JoinCode: return TEXT("JoinCode");
	default: return TEXT("None");
	}
}

int32 FTCTextFields::MaxLen(ETCTextField Field)
{
	switch (Field)
	{
	case ETCTextField::Name: case ETCTextField::Nickname: return 16;
	case ETCTextField::RoomCode: case ETCTextField::JoinCode: return 12;
	case ETCTextField::MatchTitle: return 48;
	default: return 0;
	}
}

FString& FTCTextFields::Value(ETCTextField Field)
{
	switch (Field)
	{
	case ETCTextField::RoomCode: return RoomCode;
	case ETCTextField::Nickname: return Nickname;
	case ETCTextField::MatchTitle: return MatchTitle;
	case ETCTextField::JoinCode: return JoinCode;
	default: return Buffer;
	}
}

TFunction<FString()> FTCTextFields::ClipboardText;

void FTCTextFields::Begin(ETCTextField Field, const FString& Current)
{
	Active = Field;
	Before = Current;
	Buffer = LimitLength(Current, MaxLen(Field));
	PendingLead = 0;
}

FString FTCTextFields::LimitLength(const FString& Text, int32 N)
{
	int32 Len = FMath::Min(FMath::Max(N, 0), Text.Len());
	if (Len == Text.Len()) return Text;
	if (Len > 0 && StringConv::IsHighSurrogate(Text[Len - 1])) Len--; // U16_IS_LEAD(string[new_length - 1])
	return Text.Left(Len);
}

bool FTCTextFields::Char(TCHAR C)
{
	if (Active == ETCTextField::None) return false;
	// the window's text input sends a character outside the BMP as its two UTF-16 halves, one after the other
	if (StringConv::IsHighSurrogate(C)) { PendingLead = C; return true; }
	FString Typed;
	if (StringConv::IsLowSurrogate(C)) { if (!PendingLead) return true; Typed.AppendChar(PendingLead); Typed.AppendChar(C); }
	else if (C < 0x20 || C == 0x7F) { PendingLead = 0; return true; } // Enter, Escape, Backspace, Tab: keys, not text
	else Typed.AppendChar(C);
	PendingLead = 0;
	// maxlength: typing past it inserts nothing (and a pair goes in whole or not at all)
	if (Buffer.Len() + Typed.Len() <= MaxLen(Active)) Buffer += Typed;
	return true;
}

int32 FTCTextFields::Paste(const FString& Text)
{
	if (Active == ETCTextField::None) return 0;
	// TextFieldInputType::HandleBeforeTextInsertedEvent: "\r\n", '\r' and '\n' each become a space, then LimitLength to
	// what maxlength leaves
	FString T = Text.Replace(TEXT("\r\n"), TEXT(" ")).Replace(TEXT("\r"), TEXT(" ")).Replace(TEXT("\n"), TEXT(" "));
	T = LimitLength(T, MaxLen(Active) - Buffer.Len());
	Buffer += T;
	return T.Len();
}

bool FTCTextFields::Key(const FName& KeyName, bool bShift, ETCTextField& OutDone, bool& OutCancelled)
{
	OutDone = ETCTextField::None;
	OutCancelled = false;
	if (Active == ETCTextField::None) return false;
	const FString K = KeyName.ToString();
	if (K == TEXT("Enter")) { OutDone = Active; Active = ETCTextField::None; return true; }
	if (K == TEXT("Escape")) { OutDone = Active; OutCancelled = true; Buffer = Before; Active = ETCTextField::None; return true; }
	if (K == TEXT("BackSpace"))
	{
		// one character back: both halves of a surrogate pair
		const int32 N = Buffer.Len() >= 2 && StringConv::IsLowSurrogate(Buffer[Buffer.Len() - 1]) && StringConv::IsHighSurrogate(Buffer[Buffer.Len() - 2]) ? 2 : 1;
		Buffer.LeftChopInline(N);
		return true;
	}
	// Ctrl+V (Command+V elsewhere) and Shift+Insert paste
	if ((bCtrl && K == TEXT("V")) || (bShift && K == TEXT("Insert")))
	{
		const FString Clip = ClipboardText ? ClipboardText() : FString();
		Paste(Clip);
		return true;
	}
	// the window's text input types the characters (Char); a key with Ctrl held types nothing
	if (bCharInput || bCtrl) return true;
	TCHAR C = 0;
	static const TCHAR* const DigitNames[] = { TEXT("Zero"), TEXT("One"), TEXT("Two"), TEXT("Three"), TEXT("Four"), TEXT("Five"), TEXT("Six"), TEXT("Seven"), TEXT("Eight"), TEXT("Nine") };
	if (K.Len() == 1 && K[0] >= 'A' && K[0] <= 'Z') C = bShift ? K[0] : (TCHAR)(K[0] - 'A' + 'a');
	for (int32 D = 0; D < 10 && !C; D++) if (K == DigitNames[D] || K == FString::Printf(TEXT("NumPad%s"), DigitNames[D])) C = (TCHAR)('0' + D);
	if (!C && K == TEXT("SpaceBar")) C = ' ';
	if (!C && (K == TEXT("Hyphen") || K == TEXT("Subtract"))) C = bShift ? '_' : '-';
	if (!C && K == TEXT("Underscore")) C = '_';
	if (!C && (K == TEXT("Period") || K == TEXT("Decimal"))) C = '.';
	if (C && Buffer.Len() < MaxLen(Active)) Buffer.AppendChar(C);
	return true; // the game's hotkeys stay quiet while typing
}

FString FTCTextFields::Shown(ETCTextField Field, const FString& Current) const
{
	return Active == Field ? Buffer + TEXT("|") : Current;
}

// ---------------------------------------------------------------------------------------------------------------------
// The glue (round 15, lobby): ATCGameMode, ATCHUD and ATCPlayerController members declared by the lead patch
// (lead-lobby-glue.patch) and defined here, so the patch stays small. Without the patch this file compiles on its own.
FTCPaint FTCRoomBanners::Layout(const TArray<FTCRoomBanner>& Banners, float Top, float ViewW, const FTCMeasure& Measure)
{
	FTCPaint P;
	const float Px = 14.f, Lh = Px * 1.35f, PadX = 12.f, PadY = 6.f, Gap = 4.f;
	const float MaxW = FMath::Max(0.f, FMath::Min(510.f, ViewW - 40.f));
	float Y = Top;
	for (const FTCRoomBanner& B : Banners)
	{
		if (B.Text.IsEmpty()) continue;
		const FString Shown = TC_T(B.Text);
		const TArray<FString> Lines = FTCPaint::Wrap(Shown, FMath::Max(1.f, MaxW - 2 * PadX - 2.f), Px, false, Measure);
		float TextW = 0.f;
		for (const FString& L : Lines) TextW = FMath::Max(TextW, Measure(L, Px, false, 0.f));
		const float W = FMath::Min(MaxW, TextW + 2 * PadX + 2.f), H = Lines.Num() * Lh + 2 * PadY + 2.f;
		const float X = FMath::RoundToFloat((ViewW - W) / 2);
		P.Rect(X, Y, W, H, TCCss::Rgba(25, 28, 30, 0.95f), B.bConnection ? TEXT("banner:connection") : TEXT("banner:pause"));
		P.Frame(X, Y, W, H, B.bConnection ? TCCss::Rgba(200, 72, 59, 0.8f) : TCCss::Rgba(176, 164, 122, 0.46f), 1.f, TEXT("banner:frame"));
		for (int32 K = 0; K < Lines.Num(); K++) P.Text(Lines[K], X + W / 2, Y + 1.f + PadY + K * Lh, Px, TCCss::Text, false, ETCPaintAlign::Centre, TEXT("banner:text")).Size.Y = Lh;
		Y += H + Gap;
	}
	return P;
}

#include "Client/TCGameMode.h"
#if defined(TC_HAS_LOBBY_GLUE)
#include "Client/TCHUD.h"
#include "Client/TCPlayerController.h"
#include "Client/TCPersistence.h"
#include "Client/TCLocale.h"
#include "ThreeCrossroads.h"
#include "Engine/Canvas.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "UnrealClient.h"
#include "InputCoreTypes.h"
#include "Net/TCConnection.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "Policies/CondensedJsonPrintPolicy.h"
#include "HAL/PlatformApplicationMisc.h"
#include "Framework/Application/SlateApplication.h"
#include "Engine/GameViewportClient.h"
#include "Engine/Console.h"

// Round 16 (controls): the window's text input for the text fields. SDL's text input (dead keys, compose and the
// keyboard layout already applied) reaches the game viewport as InputChar; while a field is active the character goes
// to it (ATCGameMode::Form) and nowhere else. Config/DefaultEngine.ini names this class as the game's viewport client.
bool UTCGameViewportClient::InputChar(FViewport* InViewport, int32 ControllerId, TCHAR Character)
{
	ATCGameMode* M = GetWorld() ? GetWorld()->GetAuthGameMode<ATCGameMode>() : nullptr;
	if (M && M->Form.Active != ETCTextField::None && !(ViewportConsole && ViewportConsole->ConsoleState != NAME_None))
	{
		M->Form.Char(Character);
		return true;
	}
	return Super::InputChar(InViewport, ControllerId, Character);
}

namespace TCCtlGlue
{
	constexpr float Line = 16.f, Pad = 6.f;
	const FLinearColor Text(1.f, 1.f, 1.f), Dim(0.75f, 0.78f, 0.72f), Warn(1.f, 0.6f, 0.2f), Good(0.75f, 0.9f, 1.f);
	const FLinearColor Panel(0.f, 0.f, 0.f, 0.55f), ButtonFill(0.18f, 0.24f, 0.16f, 0.9f), CardOff(0.16f, 0.16f, 0.16f, 0.85f), TabOpen(0.32f, 0.40f, 0.22f, 0.95f);
	FTCControlItem Item(const FString& Label, const FString& Key, const FString& Hover = FString(), bool bEnabled = true, bool bOn = false, int32 Slot = -1)
	{
		FTCControlItem I; I.Label = Label; I.Key = Key; I.Hover = Hover; I.bEnabled = bEnabled; I.bOn = bOn; I.Slot = Slot;
		return I;
	}
	FTCControlRow Row(const FString& Words, FTCControlRow::ETone Tone = FTCControlRow::Plain, TArray<FTCControlItem> Items = {})
	{
		FTCControlRow R; R.Text = Words; R.Tone = Tone; R.Items = MoveTemp(Items);
		return R;
	}
	FTCControlItem Button(const FTCControlButton& B)
	{
		return Item(B.Label, LexToString(B.Kind), B.bEnabled ? B.Title : B.Why, B.bEnabled, B.bPending, B.Slot);
	}
	bool FieldFromKey(const FString& Key, ETCTextField& Out)
	{
		if (!Key.StartsWith(TEXT("Field."))) return false;
		const FString F = Key.Mid(6);
		for (uint8 I = 1; I <= (uint8)ETCTextField::JoinCode; I++) if (F == LexToString((ETCTextField)I)) { Out = (ETCTextField)I; return true; }
		return false;
	}
}

void ATCGameMode::ControlsSwitches(const TCHAR* Cmd, FTCConnection::FSettings& Settings)
{
	// the Copy button's clipboard (navigator.clipboard.writeText), read back to tell Copied from Press Ctrl+C
	// round 16: the text fields' characters from the window's text input, and their paste from the clipboard
	FTCTextFields::ClipboardText = []() { FString T; FPlatformApplicationMisc::ClipboardPaste(T); return T; };
	Form.bCharInput = GEngine && GEngine->GameViewport && GEngine->GameViewport->IsA<UTCGameViewportClient>();
	UE_LOG(LogTC, Log, TEXT("lobby controls: text fields take %s; Ctrl+V and Shift+Insert paste"), Form.bCharInput ? TEXT("the window's text input (any character)") : TEXT("key names only (no text input)"));
	FTCMatchControls::Clipboard = [](const FString& Text)
	{
		FPlatformApplicationMisc::ClipboardCopy(*Text);
		FString Back;
		FPlatformApplicationMisc::ClipboardPaste(Back);
		return Back == Text;
	};
	// the Name field: -TCName (the page's ?name=), else the saved name (ww2-name), else SoldierNN
	FString Given;
	FParse::Value(Cmd, TEXT("TCName="), Given);
	const TOptional<FString> Saved = FTCSettingsStore::Game().Get(TEXT("ww2-name"));
	Settings.Name = FTCMatchControls::InitialName(Given, Saved.IsSet() ? Saved.GetValue() : FString(), FMath::RandRange(10, 99));
	Controls.SetFieldName(Settings.Name);
	Form.Nickname = Saved.IsSet() ? Saved.GetValue() : FString(); // public-lobby.js: the nickname box shows the saved name
	// client/room-session.js roomAddress: the code is lowercased (CLI-02), so this client and the browser reach one room
	if (FTCRoomBrowser::ValidCode(Settings.Room)) Settings.Room = Settings.Room.ToLower();
	// client/main.js: every hello carries listing; -TCListPublic and -TCListTitle= are the page's ?public=true&title=
	if (!Settings.Listing.IsValid())
	{
		FTCRoomListing L;
		L.bPublic = FParse::Param(Cmd, TEXT("TCListPublic"));
		FString Title;
		if (FParse::Value(Cmd, TEXT("TCListTitle="), Title, false) && !Title.IsEmpty()) L.Title = Title.Replace(TEXT("_"), TEXT(" "));
		Settings.Listing = L.ToJson();
	}
	// -TCBrowse: open the room directory first (public-lobby.html); no socket until a room is picked
	bControlsDeferConnect = bRoomBrowserOpen = FParse::Param(Cmd, TEXT("TCBrowse"));
	if (bRoomBrowserOpen) Rooms.Session.StartShown(Settings.Room); // round 16: Close opens the command line's room
	// -TCControlsCheck="<delay>[@<cond>]:<action>[:<arg>];...": a verification script (each step waits its delay after
	// the previous one and its condition), -TCControlsShotDir=<dir> for its shots
	FString ScriptText;
	if (FParse::Value(Cmd, TEXT("TCControlsCheck="), ScriptText, false))
	{
		TArray<FString> Parts;
		ScriptText.ParseIntoArray(Parts, TEXT(";"));
		for (const FString& P : Parts)
		{
			TArray<FString> F;
			P.ParseIntoArray(F, TEXT(":"), false);
			if (F.Num() < 2) continue;
			FString D = F[0], C;
			int32 At = INDEX_NONE;
			if (D.FindChar('@', At)) { C = D.Mid(At + 1); D = D.Left(At); }
			FTCControlsStep St;
			St.Delay = FCString::Atod(*D); St.Cond = C; St.Action = F[1];
			for (int32 K = 2; K < F.Num(); K++) St.Arg += (K > 2 ? TEXT(":") : TEXT("")) + F[K];
			ControlsScript.Add(St);
		}
		UE_LOG(LogTC, Log, TEXT("controls check: %d step(s)"), ControlsScript.Num());
	}
	FParse::Value(Cmd, TEXT("TCControlsShotDir="), ControlsShotDir);
	UE_LOG(LogTC, Log, TEXT("lobby controls: reference %s / %s; name '%s', room %s%s"), FTCMatchControls::ReferencePin, FTCRoomBrowser::ReferencePin, *Settings.Name, *Settings.Room, bRoomBrowserOpen ? TEXT(", the room directory first") : TEXT(""));
}

void ATCGameMode::ControlsMessage(const FString& T, const FJsonObject& M)
{
	if (T == TEXT("lobby"))
	{
		const int32 Before = Controls.Confirmed();
		const bool bWasWatching = Controls.Watching();
		if (!Controls.ApplyLobby(M)) { UE_LOG(LogTC, Warning, TEXT("lobby controls: this lobby message could not be read")); return; }
		if (Controls.Confirmed() != Before) UE_LOG(LogTC, Log, TEXT("lobby controls: %d request(s) shown by this lobby"), Controls.Confirmed() - Before);
		if (bWasWatching != Controls.Watching()) UE_LOG(LogTC, Log, TEXT("lobby controls: %s"), Controls.Watching() ? TEXT("watching as a spectator") : TEXT("seated"));
		if (Form.RoomCode.IsEmpty()) Form.RoomCode = Controls.RoomCode();
	}
	else if (T == TEXT("start"))
	{
		bool bSandbox = false;
		M.TryGetBoolField(TEXT("sandbox"), bSandbox);
		Controls.OnStart(bSandbox);
		if (bSandbox) UE_LOG(LogTC, Log, TEXT("lobby controls: a sandbox match: the sandbox panel%s"), Controls.SandboxShown() ? TEXT("") : TEXT(" stays hidden (watching)"));
	}
	else if (T == TEXT("s"))
	{
		const TArray<TSharedPtr<FJsonValue>>* Out = nullptr;
		const int32 You = State.You;
		if (M.TryGetArrayField(TEXT("out"), Out) && Out && Out->IsValidIndex(You) && (*Out)[You].IsValid() && (*Out)[You]->Type == EJson::Boolean) Controls.SetOut((*Out)[You]->AsBool());
		const TSharedPtr<FJsonObject>* ModeObj = nullptr;
		if (M.TryGetObjectField(TEXT("mode"), ModeObj) && ModeObj && ModeObj->IsValid())
		{
			FString Kind;
			bool bActive = false;
			(*ModeObj)->TryGetStringField(TEXT("kind"), Kind);
			(*ModeObj)->TryGetBoolField(TEXT("active"), bActive);
			Controls.SetHorde(Kind == TEXT("horde"), bActive);
		}
	}
	else if (T == TEXT("pause"))
	{
		bool bPausedNow = false;
		M.TryGetBoolField(TEXT("paused"), bPausedNow);
		Controls.SetPaused(bPausedNow);
	}
	else if (T == TEXT("left"))
	{
		// client/main.js connection.on('left'): the overlay (the lobby) shows again with its line, the HUD hides
		Controls.OnLeft();
		State.ClearMatch();
		ClearMatchPresentation();
		UE_LOG(LogTC, Log, TEXT("lobby controls: left: %s"), *FTCMatchControls::LeftLine());
	}
}

void ATCGameMode::ControlsOpened()
{
	Controls.OnSocketOpened();
}

bool ATCGameMode::ControlsSend(const FTCControlRequest& R)
{
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!R.Message.IsValid())
	{
		UE_LOG(LogTC, Log, TEXT("lobby controls: %s: nothing sent (%s)"), LexToString(R.Kind), *R.Note);
		if (PC && !R.Note.IsEmpty()) PC->ControlsNote(R.bArmed, R.Note);
		return false;
	}
	const bool bSent = SendCommand(R.Message.ToSharedRef());
	if (bSent) Controls.NoteSent(R);
	FString Wire;
	const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> W = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Wire);
	FJsonSerializer::Serialize(R.Message.ToSharedRef(), W);
	UE_LOG(LogTC, Log, TEXT("lobby controls: %s: %s %s (%s)"), LexToString(R.Kind), bSent ? TEXT("sent") : TEXT("not sent"), *Wire, bSent ? *R.Note : *LastSendProblem());
	if (PC) PC->ControlsNote(bSent, bSent ? R.Note : FString::Printf(TEXT("%s not sent: %s"), *R.Note, *LastSendProblem()));
	return bSent;
}

FString ATCGameMode::ControlsRoom() const
{
	return !State.RoomCode.IsEmpty() ? State.RoomCode : BaseSettings.Room;
}

void ATCGameMode::ControlsCommitField(ETCTextField Which, bool bCancelled)
{
	if (Which == ETCTextField::None) return;
	const FString V = Form.Buffer;
	if (bCancelled) { UE_LOG(LogTC, Log, TEXT("lobby controls: typing in %s cancelled"), LexToString(Which)); return; }
	switch (Which)
	{
	case ETCTextField::Name:
	{
		const FTCControlRequest R = Controls.PlanRename(V);
		if (!R.Message.IsValid()) break;
		FTCSettingsStore::Game().Set(TEXT("ww2-name"), Controls.Name()); // client/main.js: localStorage ww2-name
		BaseSettings.Name = Controls.Name();
		if (Connection) Connection->SetName(Controls.Name()); // the next hello (a reconnect) says it too
		ControlsSend(R);
		break;
	}
	case ETCTextField::RoomCode:
		Form.RoomCode = V;
		ControlsGoRoom(FTCMatchControls::PlanJoinRoom(V, ControlsRoom()));
		break;
	case ETCTextField::Nickname: Form.Nickname = V; break;
	case ETCTextField::MatchTitle:
		Form.MatchTitle = V;
		ControlsEnter(FTCRoomBrowser::Create(FTCRoomBrowser::NewCode(), Form.MatchTitle, Form.bPublic, Form.Nickname), false); // Enter submits the form
		break;
	case ETCTextField::JoinCode:
		Form.JoinCode = V;
		ControlsEnter(FTCRoomBrowser::JoinCode(Form.JoinCode, Form.Nickname), false);
		break;
	default: break;
	}
}

bool ATCGameMode::ControlsKey(const FKey& Key, bool bPressed, bool bShift)
{
	// Shift as its own events say (an injected Shift is not yet in the input state when the next key arrives)
	if (Key == EKeys::LeftShift || Key == EKeys::RightShift) bControlsShift = bPressed;
	if (Key == EKeys::LeftControl || Key == EKeys::RightControl || Key == EKeys::LeftCommand || Key == EKeys::RightCommand) Form.bCtrl = bPressed; // round 16: Ctrl+V
	if (Form.Active == ETCTextField::None || !bPressed) return false;
	bShift = bShift || bControlsShift;
	if (Key.IsModifierKey()) return false;
	if (Key.IsMouseButton())
	{
		// a click elsewhere ends the typing (the input's change event on blur); the click itself goes on
		const ETCTextField F = Form.Active;
		Form.Active = ETCTextField::None;
		ControlsCommitField(F, false);
		return false;
	}
	ETCTextField Done = ETCTextField::None;
	bool bCancelled = false;
	const bool bTaken = Form.Key(Key.GetFName(), bShift, Done, bCancelled);
	if (Done != ETCTextField::None) ControlsCommitField(Done, bCancelled);
	return bTaken;
}

void ATCGameMode::ControlsGoRoom(const FTCControlRequest& R)
{
	if (R.Room.IsEmpty())
	{
		Form.RoomCode = ControlsRoom(); // goRoom: the box shows the current room again
		UE_LOG(LogTC, Log, TEXT("lobby controls: %s"), *R.Note);
		return;
	}
	FTCRoomEntry E;
	E.bGo = true;
	E.Room = R.Room;
	E.Name = Controls.Name();
	E.Note = R.Note;
	ControlsEnter(E, true); // the seat label stays (goRoom keeps &seat=)
}

bool ATCGameMode::ControlsEnter(const FTCRoomEntry& E, bool bKeepSeat)
{
	if (!E.bGo) { UE_LOG(LogTC, Log, TEXT("lobby controls: directory: %s"), *E.Note); NoticeText = E.Note; return false; }
	// the browser loads /play?name=..#room: a new socket to the same server, with the page's hello fields
	FTCConnection::FSettings S = BaseSettings;
	S.Room = E.Room;
	S.Name = E.Name.IsEmpty() ? Controls.Name() : E.Name;
	S.Listing = E.Listing.ToJson(); // main.js reads the defaults when the page passes none
	S.bSpectate = false;
	FTCTokenRequest R = BaseTokenRequest;
	R.Room = S.Room;
	R.bSpectate = false;
	R.TokenFile.Reset(); // room-session.js: a token per room
	if (!bKeepSeat) R.Seat.Reset();
	if (E.bListingGiven) FTCSettingsStore::Game().Set(TEXT("ww2-name"), S.Name); // public-lobby.js enter() saves the nickname
	UE_LOG(LogTC, Log, TEXT("lobby controls: %s: leaving room %s for room %s (listing public %s, title '%s', tutorial %s)"), *E.Note, *ControlsRoom(), *S.Room,
		E.Listing.bPublic ? TEXT("true") : TEXT("false"), *E.Listing.Title, E.Listing.bTutorial ? TEXT("true") : TEXT("false"));
	if (Connection) { Connection->OnMessage = nullptr; Connection->OnOpened = nullptr; Connection->OnText = nullptr; Connection->Stop(); Connection.Reset(); }
	RustShadow.End();
	TokenStore.Reset();
	State.ClearMatch();
	State.RoomState.Reset(); State.RoomCode.Reset(); State.Roster.Reset(); State.Spectators.Reset();
	State.bSpectator = false; State.bAmHost = false; State.LobbySeat = -1;
	ClearMatchPresentation();
	Controls.Reset();
	Controls.SetFieldName(S.Name);
	Form.RoomCode = S.Room;
	bRoomBrowserOpen = false;
	bControlsDeferConnect = false;
	bQuickPlayPending = false;
	Rooms.Cancel();
	Rooms.Session.Entered(); // round 16: the picked room's socket opens; the directory has nothing to return to
	BaseSettings.Room = S.Room;
	BaseSettings.Name = S.Name;
	BaseSettings.Listing = S.Listing; // the page's URL (?public=&title=) a Back to it would load again
	BaseTokenRequest = R;
	return Connect(S, R);
}

void ATCGameMode::ControlsFrame()
{
	const double Now = FPlatformTime::Seconds();
	if (bRoomBrowserOpen)
	{
		const int32 AnswersBefore = Rooms.Answers();
		if (Rooms.RefreshDue(Now) && !bQuickPlayPending) Rooms.Request(ServerUrl, Now);
		if (bQuickPlayPending && !Rooms.Loading() && Rooms.Answers() != QuickPlayAnswers)
		{
			bQuickPlayPending = false;
			const FTCRoomEntry E = Rooms.QuickPlay(Rooms.IsOnline(), FTCRoomBrowser::NewCode(), Form.Nickname);
			UE_LOG(LogTC, Log, TEXT("lobby controls: %s"), *E.Note);
			ControlsEnter(E, false);
		}
		if (Rooms.Answers() != AnswersBefore) UE_LOG(LogTC, Log, TEXT("room directory: %s"), *Rooms.StatusLine());
	}
	// the verification script
	if (ControlsStep < ControlsScript.Num())
	{
		const FTCControlsStep& St = ControlsScript[ControlsStep];
		if (ControlsStepAt < 0) ControlsStepAt = Now;
		const FString& C = St.Cond;
		const bool bReady = Now - ControlsStepAt >= St.Delay && (C.IsEmpty()
			|| (C == TEXT("lobby") && Controls.InLobby()) || (C == TEXT("play") && Controls.InPlay() && State.Snapshots > 0)
			|| (C == TEXT("watching") && Controls.Watching()) || (C == TEXT("seated") && Controls.HasLobby() && !Controls.Watching())
			|| (C == TEXT("break") && Controls.NextWaveButton().bShown) || (C == TEXT("wave") && Controls.InPlay() && !Controls.NextWaveButton().bShown)
			|| (C == TEXT("rooms") && Rooms.HasAnswer()) || (C == TEXT("offline") && Controls.Seats().ContainsByPredicate([](const FTCMatchControls::FSeat& P) { return !P.bAi && !P.bConnected; }))
			|| (C == TEXT("left") && Controls.Left()) || (C == TEXT("nolobby") && !Controls.HasLobby())
			|| (C == TEXT("alert") && AlertFeed.Lines().ContainsByPredicate([](const FTCAlertLine& L) { return L.Alert.bHasAt; }))
			// round 16: 'humans<N>': at least N connected human seats in the lobby (a host waits for its guests to start)
			|| (C.StartsWith(TEXT("humans")) && Controls.Seats().FilterByPredicate([](const FTCMatchControls::FSeat& P) { return !P.bAi && P.bConnected; }).Num() >= FCString::Atoi(*C.Mid(6))));
		if (bReady)
		{
			ControlsStep++;
			ControlsStepAt = Now;
			ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
			FString Did;
			if (St.Action == TEXT("press") && PC)
			{
				FTCUiCommand Cmd(ETCUiAction::MatchControl);
				FString Key = St.Arg;
				int32 Slot = -1, Hash = INDEX_NONE;
				if (Key.FindChar('#', Hash)) { Slot = FCString::Atoi(*Key.Mid(Hash + 1)); Key = Key.Left(Hash); }
				Cmd.Unit = Key; Cmd.Group = Slot;
				const ATCHUD* Hud = Cast<ATCHUD>(PC->GetHUD());
				FVector2D At;
				const bool bDrawn = Hud && Hud->CommandCentre(Cmd, At);
				PC->PressCommand(Cmd);
				Did = FString::Printf(TEXT("press %s%s (%s)"), *Key, Slot >= 0 ? *FString::Printf(TEXT(" #%d"), Slot) : TEXT(""), bDrawn ? TEXT("through the hit test") : TEXT("not drawn: straight to the command"));
			}
			else if (St.Action == TEXT("type") && PC && Form.bCharInput && FSlateApplication::IsInitialized())
			{
				// round 16: each character through Slate's own text input path (FSlateApplication::ProcessKeyCharEvent, as
				// the window's SDL text input sends it), so 'José' and 'ñ' go the way a typed accent does; '+' is a space
				const FString Text = St.Arg.Replace(TEXT("+"), TEXT(" "));
				int32 ViaSlate = 0, ViaViewport = 0;
				for (const TCHAR Ch : Text)
				{
					// Slate's character event goes to the widget with the keyboard focus (the game viewport when the
					// window has it); an offscreen window may have none, then the viewport client's InputChar, which
					// the focused viewport would call
					const FString Was = Form.Buffer;
					FSlateApplication::Get().ProcessKeyCharEvent(FCharacterEvent(Ch, FModifierKeysState(), 0, false));
					if (Form.Buffer != Was) { ViaSlate++; continue; }
					if (GEngine && GEngine->GameViewport) GEngine->GameViewport->InputChar(GEngine->GameViewport->Viewport, 0, Ch);
					ViaViewport++;
				}
				Did = FString::Printf(TEXT("typed '%s' as text input, %d character(s) through Slate's focus, %d through the viewport client (field %s: '%s')"), *Text, ViaSlate, ViaViewport, LexToString(Form.Active), *Form.Buffer);
			}
			else if (St.Action == TEXT("paste") && PC)
			{
				// round 16: Ctrl+V as keys (LeftControl down, V, LeftControl up) into the engine's input
				PC->InjectKey(EKeys::LeftControl, true); PC->InjectKey(EKeys::V, true); PC->InjectKey(EKeys::V, false); PC->InjectKey(EKeys::LeftControl, false);
				Did = FString::Printf(TEXT("Ctrl+V (field %s: '%s')"), LexToString(Form.Active), *Form.Buffer);
			}
			else if (St.Action == TEXT("type") && PC)
			{
				for (const TCHAR Ch : St.Arg)
				{
					FKey K;
					bool bUpper = false;
					if (Ch >= 'a' && Ch <= 'z') K = FKey(FName(FString::Chr(Ch - 'a' + 'A')));
					else if (Ch >= 'A' && Ch <= 'Z') { K = FKey(FName(FString::Chr(Ch))); bUpper = true; }
					else if (Ch >= '0' && Ch <= '9') { static const TCHAR* const N[] = { TEXT("Zero"), TEXT("One"), TEXT("Two"), TEXT("Three"), TEXT("Four"), TEXT("Five"), TEXT("Six"), TEXT("Seven"), TEXT("Eight"), TEXT("Nine") }; K = FKey(N[Ch - '0']); }
					else if (Ch == ' ' || Ch == '+') K = EKeys::SpaceBar;
					else if (Ch == '-') K = EKeys::Hyphen;
					else continue;
					if (bUpper) PC->InjectKey(EKeys::LeftShift, true);
					PC->InjectKey(K, true); PC->InjectKey(K, false);
					if (bUpper) PC->InjectKey(EKeys::LeftShift, false);
				}
				Did = FString::Printf(TEXT("typed '%s' (field %s: '%s')"), *St.Arg, LexToString(Form.Active), *Form.Buffer);
			}
			else if (St.Action == TEXT("erase") && PC)
			{
				for (int32 K = FCString::Atoi(*St.Arg); K > 0; K--) { PC->InjectKey(EKeys::BackSpace, true); PC->InjectKey(EKeys::BackSpace, false); }
				Did = FString::Printf(TEXT("Backspace x%s (field %s: '%s')"), *St.Arg, LexToString(Form.Active), *Form.Buffer);
			}
			else if (St.Action == TEXT("key") && PC) { const FKey K(*St.Arg); PC->InjectKey(K, true); PC->InjectKey(K, false); Did = TEXT("key ") + St.Arg; }
			else if (St.Action == TEXT("spawn") && PC)
			{
				FString X, Z;
				St.Arg.Split(TEXT(","), &X, &Z);
				const bool bOk = PC->AimClick(true, FVector2D(FCString::Atod(*X), FCString::Atod(*Z)), false);
				Did = FString::Printf(TEXT("map click at (%s, %s) with the spawn aim: %s"), *X, *Z, bOk ? TEXT("sent") : TEXT("nothing sent"));
			}
			else if (St.Action == TEXT("spawnhome") && PC)
			{
				// a map click with the spawn aim, dx and dz metres from this seat's spawn (the start's)
				FString X, Z;
				St.Arg.Split(TEXT(","), &X, &Z);
				const FVector2D G(State.Spawn.X + FCString::Atod(*X), State.Spawn.Y + FCString::Atod(*Z));
				const bool bOk = PC->AimClick(true, G, false);
				Did = FString::Printf(TEXT("map click at (%.1f, %.1f) with the %s aim: %s"), G.X, G.Y, LexToString(PC->Aim), bOk ? TEXT("sent") : TEXT("nothing sent"));
			}
			else if (St.Action == TEXT("lobby"))
			{
				// a lobby setting as its HUD button sends it (FTCLobbySettings), for setting a run up: lobby:Mode:horde
				FString K, V;
				St.Arg.Split(TEXT(":"), &K, &V);
				bool bFound = false;
				for (int32 I = 0; I <= (int32)ETCLobbySetting::Level; I++)
					if (K == LexToString((ETCLobbySetting)I))
					{
						const FTCLobbyRequest R = Lobby.Plan((ETCLobbySetting)I, -1, V);
						const bool bSent = R.Message.IsValid() && SendCommand(R.Message.ToSharedRef());
						if (bSent) Lobby.NoteSent(R, Now);
						Did = FString::Printf(TEXT("lobby %s %s: %s (%s)"), *K, *V, bSent ? TEXT("sent") : TEXT("not sent"), *R.Note);
						bFound = true;
					}
				if (K == TEXT("Sandbox"))
				{
					const FTCOptionRequest R = Options.PlanSandbox(V == TEXT("true"));
					const bool bSent = R.Message.IsValid() && SendCommand(R.Message.ToSharedRef());
					if (bSent) Options.NoteSent(R, Now);
					Did = FString::Printf(TEXT("sandbox %s: %s (%s)"), *V, bSent ? TEXT("sent") : TEXT("not sent"), *R.Note);
					bFound = true;
				}
				if (!bFound) Did = TEXT("unknown lobby setting ") + K;
			}
			else if (St.Action == TEXT("start") && PC) { PC->PressButton(ETCUiAction::Start); Did = TEXT("Start pressed"); }
			else if (St.Action == TEXT("addai") && PC) { PC->PressButton(ETCUiAction::AddAi); Did = TEXT("Add AI pressed"); }
			else if (St.Action == TEXT("shot"))
			{
				const FString File = FPaths::Combine(ControlsShotDir.IsEmpty() ? FPaths::ProjectSavedDir() / TEXT("Screenshots") : ControlsShotDir, St.Arg + TEXT(".png"));
				FScreenshotRequest::RequestScreenshot(File, true, false);
				Did = TEXT("screenshot ") + File;
			}
			else if (St.Action == TEXT("alert") && PC)
			{
				// round 16: a click on the alert feed through the HUD's hit test: 'line' (the newest line with a place),
				// 'history' (its summary), 'prev' / 'next' (Previous Alert, Next Alert), 'entry' (the second newest
				// history entry with a place, else the newest)
				FString Tag;
				if (St.Arg == TEXT("line"))
				{
					int32 Best = -1;
					for (const FTCAlertLine& L : AlertFeed.Lines()) if (L.Alert.bHasAt && L.Serial > Best) Best = L.Serial;
					if (Best >= 0) Tag = FString::Printf(TEXT("alert:line:%d"), Best);
				}
				else if (St.Arg == TEXT("history")) Tag = TEXT("alert:history");
				else if (St.Arg == TEXT("prev")) Tag = TEXT("alert:nav:1");
				else if (St.Arg == TEXT("next")) Tag = TEXT("alert:nav:-1");
				else if (St.Arg == TEXT("entry"))
				{
					TArray<int32> Ids;
					for (const FTCAlertHistoryEntry& E : AlertFeed.History().Entries()) if (E.bHasAt) Ids.Add(E.Id);
					if (Ids.Num()) Tag = FString::Printf(TEXT("alert:entry:%d"), Ids.Num() > 1 ? Ids[Ids.Num() - 2] : Ids.Last());
				}
				if (Tag.IsEmpty()) Did = TEXT("alert click ") + St.Arg + TEXT(": nothing to click (no such line or entry)");
				else
				{
					FTCUiCommand Cmd(ETCUiAction::Alert);
					Cmd.Unit = Tag;
					const ATCHUD* Hud = Cast<ATCHUD>(PC->GetHUD());
					FVector2D At;
					const bool bDrawn = Hud && Hud->CommandCentre(Cmd, At);
					const FVector2D Before = PC->ViewFocus();
					const bool bRan = PC->PressCommand(Cmd);
					const FVector2D After = PC->ViewFocus();
					Did = FString::Printf(TEXT("alert click %s (%s, %s): camera focus (%.1f, %.1f) -> (%.1f, %.1f); history %s, %d entr%s, selected %d"), *Tag, bDrawn ? *FString::Printf(TEXT("drawn at (%.0f, %.0f)"), At.X, At.Y) : TEXT("not drawn"),
						bRan ? TEXT("ran") : TEXT("did nothing"), Before.X, Before.Y, After.X, After.Y, AlertFeed.bHistoryOpen ? TEXT("open") : TEXT("closed"), AlertFeed.History().Entries().Num(),
						AlertFeed.History().Entries().Num() == 1 ? TEXT("y") : TEXT("ies"), AlertFeed.History().Selected());
				}
			}
			else if (St.Action == TEXT("recruit") && PC)
			{
				// round 16: a recruit card button (hud.js [data-unit] onclick) through the HUD's hit test; its 'ready' alert
				// has a place, so the alert feed gets a line to click
				FTCUiCommand Cmd(ETCUiAction::Recruit);
				Cmd.Unit = St.Arg;
				const ATCHUD* Hud = Cast<ATCHUD>(PC->GetHUD());
				FVector2D At;
				const bool bDrawn = Hud && Hud->CommandCentre(Cmd, At);
				const bool bRan = PC->PressCommand(Cmd);
				Did = FString::Printf(TEXT("recruit %s (%s, %s)"), *St.Arg, bDrawn ? *FString::Printf(TEXT("drawn at (%.0f, %.0f)"), At.X, At.Y) : TEXT("not drawn"), bRan ? TEXT("ran") : TEXT("did nothing"));
			}
			else if (St.Action == TEXT("rightclick") && PC)
			{
				// round 16: a right press on the world at the window's centre (client/main.js mousedown button 2): with
				// an aim armed it cancels the aim, the spawn aim included
				int32 VX = 0, VY = 0;
				PC->GetViewportSize(VX, VY);
				const bool bArmedBefore = Controls.SpawnArmed();
				const FString AimBefore = LexToString(PC->Aim);
				PC->RightPressWorld(FVector2D(VX / 2.0, VY / 2.0), false, false);
				Did = FString::Printf(TEXT("right press at (%d, %d): aim %s -> %s, sandbox spawn armed %s -> %s"), VX / 2, VY / 2, *AimBefore, LexToString(PC->Aim),
					bArmedBefore ? TEXT("yes") : TEXT("no"), Controls.SpawnArmed() ? TEXT("yes") : TEXT("no"));
			}
			else if (St.Action == TEXT("exec") && GEngine)
			{
				// round 16: a console command (the stats probe's 'stat unit')
				const FString Cmd = St.Arg.Replace(TEXT("_"), TEXT(" "));
				GEngine->Exec(GetWorld(), *Cmd);
				Did = TEXT("console: ") + Cmd;
			}
#if defined(TC_HAS_STATS_GLUE)
			else if (St.Action == TEXT("stats") || St.Action == TEXT("statsback"))
			{
				// round 16: a control of the stats sheet by its tag ('show', 'pick:worst', 'preset:Everything', 'select:pos',
				// 'warn', 'close'), pressed at its drawn centre; 'backdrop' clicks outside the sheet
				bool bOk = false;
				if (St.Arg == TEXT("backdrop")) bOk = StatsSheetPress(FVector2D(4, 4), false);
				else bOk = StatsSheetTag(TEXT("stats:") + St.Arg, St.Action == TEXT("statsback"));
				Did = FString::Printf(TEXT("stats sheet %s%s: %s; sheet %s, overlay %s, settings %s"), *St.Arg, St.Action == TEXT("statsback") ? TEXT(" (right)") : TEXT(""), bOk ? TEXT("done") : TEXT("nothing"),
					StatsSheet.IsOpen() ? TEXT("open") : TEXT("closed"), *StatsOverlay().MenuLabel(), Stats.SaveText().IsSet() ? *Stats.SaveText().GetValue() : TEXT("(forced)"));
			}
#endif
			else if (St.Action == TEXT("clip"))
			{
				// round 16: put a text on the system clipboard (as another program's copy would), '_' for spaces; 'save'
				// keeps what the clipboard held and 'restore' puts it back
				static FString Kept;
				if (St.Arg == TEXT("save")) { FPlatformApplicationMisc::ClipboardPaste(Kept); Did = FString::Printf(TEXT("clipboard kept (%d characters)"), Kept.Len()); }
				else if (St.Arg == TEXT("restore")) { FPlatformApplicationMisc::ClipboardCopy(*Kept); Did = TEXT("clipboard restored"); }
				else { const FString T = St.Arg.Replace(TEXT("_"), TEXT(" ")); FPlatformApplicationMisc::ClipboardCopy(*T); Did = FString::Printf(TEXT("clipboard set to '%s'"), *T); }
			}
			else if (St.Action == TEXT("drop") && Connection) { Connection->SimulateDrop(); Did = TEXT("socket dropped"); }
			else if (St.Action == TEXT("stop") && Connection) { Connection->Stop(); Did = TEXT("connection stopped (this client goes offline)"); }
			else if (St.Action == TEXT("quit")) { FPlatformMisc::RequestExit(false, TEXT("TCControlsCheck")); Did = TEXT("quit"); }
			else Did = TEXT("log");
			TArray<FString> Menu;
			for (const FTCControlButton& B : Controls.MenuButtons(Now)) if (B.bShown) Menu.Add(B.Label);
			UE_LOG(LogTC, Log, TEXT("controls check %d/%d: %s | state %s, %s, host %s, watch button '%s' %s, menu %s [%s], next wave %s, sandbox %s (%s, %s, armed %s), directory %s, %d row(s)"),
				ControlsStep, ControlsScript.Num(), *Did, Controls.HasLobby() ? (Controls.InLobby() ? TEXT("lobby") : TEXT("play")) : TEXT("none"), Controls.Watching() ? TEXT("watching") : TEXT("seated"),
				Controls.AmHost() ? TEXT("yes") : TEXT("no"), *Controls.WatchButton().Label, Controls.WatchButton().bShown ? TEXT("shown") : TEXT("hidden"), Controls.MenuOpen() ? TEXT("open") : TEXT("closed"), *FString::Join(Menu, TEXT(", ")),
				Controls.NextWaveButton().bShown ? TEXT("shown") : TEXT("hidden"), Controls.SandboxShown() ? TEXT("shown") : TEXT("hidden"), Controls.SandboxUnit().Type, Controls.SandboxEnemy() ? TEXT("enemy") : TEXT("mine"),
				Controls.SpawnArmed() ? TEXT("yes") : TEXT("no"), bRoomBrowserOpen ? *Rooms.StatusLine() : TEXT("closed"), Rooms.Rows().Num());
		}
	}
}

bool ATCGameMode::ControlsSpawnAt(FVector2D Ground)
{
	// client/main.js mousedown with the spawn aim: { t: 'spawn', unit, enemy, x, z }, a marker, and the aim stays
	return ControlsSend(Controls.PlanSpawn(Ground.X, Ground.Y));
}

bool ATCGameMode::RunControl(const FTCUiCommand& Command, bool bBack)
{
	const double Now = FPlatformTime::Seconds();
	const FString& Key = Command.Unit;
	ETCTextField Typed = ETCTextField::None;
	// a click elsewhere ends the typing first (the input's change event)
	if (Form.Active != ETCTextField::None && !(TCCtlGlue::FieldFromKey(Key, Typed) && Typed == Form.Active))
	{
		const ETCTextField F = Form.Active;
		Form.Active = ETCTextField::None;
		ControlsCommitField(F, false);
	}
	if (TCCtlGlue::FieldFromKey(Key, Typed))
	{
		if (Form.Active == Typed) return true;
		Form.Begin(Typed, Typed == ETCTextField::Name ? Controls.Name() : Form.Value(Typed));
		UE_LOG(LogTC, Log, TEXT("lobby controls: typing in %s"), LexToString(Typed));
		return true;
	}
	ETCMatchControl Kind;
	if (LexFromString(Kind, *Key))
	{
		ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
		switch (Kind)
		{
		case ETCMatchControl::Watch: return ControlsSend(Controls.PlanWatch());
		case ETCMatchControl::CopyLink:
		{
			const FTCControlRequest R = Controls.CopyLink(Controls.InviteLink(ServerUrl, ControlsRoom()), Now);
			UE_LOG(LogTC, Log, TEXT("lobby controls: Copy: %s (%s)"), *R.Note, *Controls.InviteLink(ServerUrl, ControlsRoom()));
			return true;
		}
		case ETCMatchControl::JoinRoom: ControlsGoRoom(FTCMatchControls::PlanJoinRoom(Form.RoomCode.IsEmpty() ? ControlsRoom() : Form.RoomCode, ControlsRoom())); return true;
		case ETCMatchControl::NewRoom: ControlsGoRoom(FTCMatchControls::PlanNewRoom()); return true;
		case ETCMatchControl::Menu: { const FTCControlRequest R = Controls.ToggleMenu(); UE_LOG(LogTC, Log, TEXT("lobby controls: %s"), *R.Note); return true; }
		case ETCMatchControl::Pause: case ETCMatchControl::Save: case ETCMatchControl::Restart: case ETCMatchControl::End: case ETCMatchControl::HandAi: case ETCMatchControl::Leave:
			return ControlsSend(Controls.PressMenu(Kind, Command.Group, Now));
		case ETCMatchControl::NextWave: return ControlsSend(Controls.PlanNextWave());
		case ETCMatchControl::SandboxUnit: Controls.StepSandboxUnit(bBack ? -1 : 1); UE_LOG(LogTC, Log, TEXT("lobby controls: sandbox unit %s"), Controls.SandboxUnit().Type); return true;
		case ETCMatchControl::SandboxSide: Controls.SetSandboxEnemy(!Controls.SandboxEnemy()); UE_LOG(LogTC, Log, TEXT("lobby controls: sandbox side %s"), Controls.SandboxEnemy() ? TEXT("enemy") : TEXT("mine")); return true;
		case ETCMatchControl::SandboxSpawn:
		{
			const FTCControlRequest R = Controls.ArmSpawn();
			UE_LOG(LogTC, Log, TEXT("lobby controls: %s"), *R.Note);
			if (Controls.SpawnArmed() && PC && !PC->ArmSpawnAim(FString::Printf(TEXT("Spawn %s (%s): click the map, once per unit. Right-click stops"), Controls.SandboxUnit().Name, Controls.SandboxEnemy() ? TEXT("Enemy") : TEXT("Mine"))))
			{
				Controls.DisarmSpawn(); // setAim arms nothing while observing
				UE_LOG(LogTC, Log, TEXT("lobby controls: the spawn aim was not armed (observing)"));
			}
			return Controls.SpawnArmed();
		}
		default: return false;
		}
	}
#if defined(TC_HAS_STATS_GLUE)
	// round 16: $('statsBtn').onclick = () => { menuOpen(false); stats.open(); }
	if (Key == TEXT("Stats.Open"))
	{
		if (Controls.MenuOpen()) Controls.ToggleMenu();
		if (ATCPlayerController* SPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController())) SPC->OpenStatsSheet();
		return true;
	}
#endif
	// the room directory (client/public-lobby.js)
	if (Key == TEXT("Browse"))
	{
		// round 16: the browser leaves /play for / (index.html <a href="/">): the room's socket closes with the page,
		// and the directory opens none (public-lobby.js); Close opens the room again
		const FString Room = ControlsRoom();
		if (Rooms.Session.Open(Connection.IsValid(), Room) == FTCDirectorySession::EStep::CloseSocket)
		{
			UE_LOG(LogTC, Log, TEXT("room directory: leaving room %s: its socket closes (%s), as the browser's page does when it goes to the directory"), *Room, *ConnectionStatus());
			Connection->OnMessage = nullptr; Connection->OnOpened = nullptr; Connection->OnText = nullptr; Connection->Stop(); Connection.Reset();
			RustShadow.End();
			TokenStore.Reset();
			State.ClearMatch();
			State.RoomState.Reset(); State.RoomCode.Reset(); State.Roster.Reset(); State.Spectators.Reset();
			State.bSpectator = false; State.bAmHost = false; State.LobbySeat = -1;
			ClearMatchPresentation();
			Controls.Reset();
			Controls.SetFieldName(BaseSettings.Name);
			BaseSettings.Room = Room;
			bControlsDeferConnect = true;
		}
		bRoomBrowserOpen = true; NoticeText.Reset(); Rooms.Request(ServerUrl, Now);
		UE_LOG(LogTC, Log, TEXT("room directory: open (%s)"), Connection.IsValid() ? TEXT("a socket is still open") : TEXT("no game socket"));
		return true;
	}
	if (Key == TEXT("Browser.Close"))
	{
		const FTCDirectorySession::EStep Step = Rooms.Session.Close(Connection.IsValid());
		bRoomBrowserOpen = false; Rooms.Cancel(); bQuickPlayPending = false;
		UE_LOG(LogTC, Log, TEXT("room directory: closed%s"), Step == FTCDirectorySession::EStep::OpenRoom || bControlsDeferConnect ? *FString::Printf(TEXT(": back to room %s, a new socket"), *BaseSettings.Room) : TEXT(""));
		if (Step == FTCDirectorySession::EStep::OpenRoom || bControlsDeferConnect) { bControlsDeferConnect = false; Connect(BaseSettings, BaseTokenRequest); }
		return true;
	}
	if (Key == TEXT("Browser.Refresh")) { NoticeText.Reset(); return Rooms.Request(ServerUrl, Now); }
	if (Key == TEXT("Browser.Filter")) { Rooms.StepModeFilter(bBack ? -1 : 1); return true; }
	if (Key == TEXT("Browser.OpenOnly")) { Rooms.SetOpenOnly(!Rooms.OpenOnly()); return true; }
	if (Key == TEXT("Browser.Visibility")) { Form.bPublic = !Form.bPublic; return true; }
	if (Key == TEXT("Browser.Tutorial")) return ControlsEnter(FTCRoomBrowser::Tutorial(FTCRoomBrowser::NewCode(), Form.Nickname), false);
	if (Key == TEXT("Browser.Create")) return ControlsEnter(FTCRoomBrowser::Create(FTCRoomBrowser::NewCode(), Form.MatchTitle, Form.bPublic, Form.Nickname), false);
	if (Key == TEXT("Browser.JoinCode")) return ControlsEnter(FTCRoomBrowser::JoinCode(Form.JoinCode, Form.Nickname), false);
	if (Key == TEXT("Browser.Quick"))
	{
		// Quick Play: refresh, then the best joinable Conquest room or a new public one
		NoticeText.Reset();
		bQuickPlayPending = true;
		QuickPlayAnswers = Rooms.Answers();
		Rooms.Request(ServerUrl, Now);
		return true;
	}
	if (Key.StartsWith(TEXT("Browser.Join:"))) return ControlsEnter(Rooms.Join(Key.Mid(13), Form.Nickname), false);
	UE_LOG(LogTC, Warning, TEXT("lobby controls: unknown control '%s'"), *Key);
	return false;
}

TArray<FTCControlRow> ATCGameMode::LobbyControlRows() const
{
	using namespace TCCtlGlue;
	TArray<FTCControlRow> Rows;
	const double Now = FPlatformTime::Seconds();
	if (Controls.Left()) Rows.Add(Row(FTCMatchControls::LeftLine(), FTCControlRow::Warn));
	// client/index.html Players: the Name field, then (after the roster) Watch as a spectator / Take a seat
	Rows.Add(Row(TEXT("Name"), FTCControlRow::Plain, { Item(Form.Shown(ETCTextField::Name, Controls.Name()), TEXT("Field.Name"), TEXT("Name"), true, Form.Active == ETCTextField::Name) }));
	const FTCControlButton W = Controls.WatchButton();
	if (W.bShown) Rows.Add(Row(FString(), FTCControlRow::Plain, { Item(W.bPending ? W.Label + TEXT(" (asked)") : W.Label, TEXT("Watch"), W.Title) }));
	// Invite: the link and Copy; the Room box: Join and New
	Rows.Add(Row(TEXT("Invite"), FTCControlRow::Heading));
	Rows.Add(Row(TEXT("Send this link to friends. It always opens this room."), FTCControlRow::Dim));
	const FTCControlButton C = Controls.CopyButton(Now);
	Rows.Add(Row(Controls.InviteLink(ServerUrl, ControlsRoom()) + TEXT("  "), FTCControlRow::Good, { Item(C.Label, TEXT("CopyLink"), C.Title) }));
	Rows.Add(Row(TEXT("Room"), FTCControlRow::Plain, {
		Item(Form.Shown(ETCTextField::RoomCode, Form.RoomCode.IsEmpty() ? ControlsRoom() : Form.RoomCode), TEXT("Field.RoomCode"), TEXT("Type a room code and press Join; leave it as main for the usual room"), true, Form.Active == ETCTextField::RoomCode),
		Item(TEXT("Join"), TEXT("JoinRoom"), TEXT("Join the room code typed on the left")),
		Item(TEXT("New"), TEXT("NewRoom"), TEXT("A private room with a random code")),
		Item(TEXT("Browse matches"), TEXT("Browse")) }));
	return Rows;
}

TArray<FTCControlRow> ATCGameMode::BrowserRows() const
{
	using namespace TCCtlGlue;
	TArray<FTCControlRow> Rows;
	Rows.Add(Row(TEXT("MULTIPLAYER HEADQUARTERS"), FTCControlRow::Dim));
	Rows.Add(Row(TEXT("Three Crossroads"), FTCControlRow::Heading, { Item(TEXT("Close"), TEXT("Browser.Close"), FString()) }));
	Rows.Add(Row(TEXT("WW2 tactics. Take ground. Hold the line."), FTCControlRow::Dim));
	Rows.Add(Row(TEXT("Your name"), FTCControlRow::Plain, {
		Item(Form.Shown(ETCTextField::Nickname, Form.Nickname.IsEmpty() ? FString(TEXT("Soldier")) : Form.Nickname), TEXT("Field.Nickname"), TEXT("Your name"), true, Form.Active == ETCTextField::Nickname),
		Item(TEXT("Quick Play"), TEXT("Browser.Quick"), FString(), !bQuickPlayPending),
		Item(TEXT("Tutorial"), TEXT("Browser.Tutorial"), TEXT("New here? Learn the basics in about ten minutes")) }));
	Rows.Add(Row(TEXT("Join an open Conquest lobby, or start one. No account needed."), FTCControlRow::Dim));
	if (!NoticeText.IsEmpty()) Rows.Add(Row(NoticeText, FTCControlRow::Warn));
	else if (bQuickPlayPending && Rooms.Loading()) Rows.Add(Row(TEXT("Connecting…"), FTCControlRow::Dim));
	Rows.Add(Row(TEXT("Browse matches"), FTCControlRow::Heading, { Item(TEXT("Refresh"), TEXT("Browser.Refresh"), FString(), !Rooms.Loading()) }));
	Rows.Add(Row(TEXT("Mode"), FTCControlRow::Plain, {
		Item(FTCRoomBrowser::ModeFilterLabel(Rooms.ModeFilter()), TEXT("Browser.Filter"), TEXT("Mode")),
		Item(TEXT("Open seats only"), TEXT("Browser.OpenOnly"), FString(), true, Rooms.OpenOnly()) })); // the checkbox: drawn pressed when checked
	Rows.Add(Row(Rooms.StatusLine(), Rooms.IsOnline() ? FTCControlRow::Good : FTCControlRow::Dim));
	Rows.Add(Row(TEXT("Match / map  |  Mode  |  Seats  |  Status"), FTCControlRow::Dim));
	for (const FTCRoomRow& R : Rooms.Rows())
		Rows.Add(Row(FString::Printf(TEXT("%s  %s  |  %s  |  %s  %s  |  %s  "), *R.Title, *R.MapLine, *R.Mode, *R.Seats, *R.Counts, *R.Status), R.bOpen ? FTCControlRow::Good : FTCControlRow::Plain,
			{ Item(R.JoinLabel, TEXT("Browser.Join:") + R.Code, R.JoinAria, R.bJoinEnabled) }));
	if (Rooms.EmptyShown()) Rows.Add(Row(Rooms.EmptyLine(), FTCControlRow::Dim));
	Rows.Add(Row(TEXT("Create a match"), FTCControlRow::Heading));
	Rows.Add(Row(TEXT("Match name"), FTCControlRow::Plain, { Item(Form.Shown(ETCTextField::MatchTitle, Form.MatchTitle.IsEmpty() ? FString(TEXT("Open skirmish")) : Form.MatchTitle), TEXT("Field.MatchTitle"), TEXT("Match name"), true, Form.Active == ETCTextField::MatchTitle) }));
	Rows.Add(Row(TEXT("Visibility"), FTCControlRow::Plain, { Item(Form.bPublic ? TEXT("Public: shown in the match browser") : TEXT("Unlisted: share the room link"), TEXT("Browser.Visibility"), TEXT("Visibility")) }));
	Rows.Add(Row(TEXT("Choose the map, mode, teams and AI inside the room. Unlisted is not password-protected."), FTCControlRow::Dim, { Item(TEXT("Create match"), TEXT("Browser.Create")) }));
	Rows.Add(Row(TEXT("Have a room code?"), FTCControlRow::Heading));
	Rows.Add(Row(TEXT("Room code"), FTCControlRow::Plain, {
		Item(Form.Shown(ETCTextField::JoinCode, Form.JoinCode.IsEmpty() ? FString(TEXT("e.g. crossroads")) : Form.JoinCode), TEXT("Field.JoinCode"), TEXT("Room code"), true, Form.Active == ETCTextField::JoinCode),
		Item(TEXT("Join room"), TEXT("Browser.JoinCode")) }));
	Rows.Add(Row(TEXT("Existing room links still work. A new code creates an unlisted room."), FTCControlRow::Dim));
	Rows.Add(Row(TEXT("Best played on a desktop with a mouse and keyboard."), FTCControlRow::Dim));
	Rows.Add(Row(TEXT("Live rooms refresh every 5 seconds."), FTCControlRow::Dim));
	return Rows;
}

TArray<FTCControlRow> ATCGameMode::MatchControlRows() const
{
	using namespace TCCtlGlue;
	TArray<FTCControlRow> Rows;
	const double Now = FPlatformTime::Seconds();
	const FTCControlButton MenuB = Controls.MenuButton();
	if (!MenuB.bShown) return Rows;
	Rows.Add(Row(FString(), FTCControlRow::Plain, { Item(MenuB.Label, TEXT("Menu"), MenuB.Title, true, Controls.MenuOpen()) }));
	if (Controls.MenuOpen())
	{
		for (const FTCControlButton& B : Controls.MenuButtons(Now)) if (B.bShown) Rows.Add(Row(FString(), FTCControlRow::Plain, { Button(B) }));
#if defined(TC_HAS_STATS_GLUE)
		// round 16: #menu's statsBtn ('Stats overlay: On' or 'Off', its title), for every reader of the menu
		Rows.Add(Row(FString(), FTCControlRow::Plain, { Item(const_cast<ATCGameMode*>(this)->StatsOverlay().MenuLabel(), TEXT("Stats.Open"), FTCStatsSheet::MenuTitle()) }));
#endif
	}
	if (Controls.SandboxShown())
	{
		TArray<FTCControlItem> Items;
		for (const FTCControlButton& B : Controls.SandboxButtons()) Items.Add(Button(B));
		Rows.Add(Row(TEXT("Sandbox"), FTCControlRow::Plain, Items));
	}
	const FTCControlButton NW = Controls.NextWaveButton();
	if (NW.bShown) Rows.Add(Row(FString(), FTCControlRow::Plain, { Button(NW) }));
	return Rows;
}

float ATCHUD::DrawControlRows(UFont* Font, float X, float Y, float W, const TArray<FTCControlRow>& Rows)
{
	using namespace TCCtlGlue;
	if (!Rows.Num()) return Y;
	const float RowH = Line + Pad + 4.f;
	auto Measure = [&](const FString& Lb) { float TW = 0, TH = 0; LabelSize(Lb, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	float H = 2 * Pad + Line; // the hover line
	for (const FTCControlRow& R : Rows) H += R.Items.Num() ? RowH : Line;
	float Wide = W;
	for (const FTCControlRow& R : Rows)
	{
		float RW = (R.Text.IsEmpty() ? 0.f : Measure(R.Text)) + 2 * Pad;
		for (const FTCControlItem& It : R.Items) RW += Measure(It.Label) + Pad;
		Wide = FMath::Max(Wide, RW);
	}
	AddPanel(X, Y, Wide, H, Panel);
	FVector2D Mouse(-1, -1);
	float MX, MY;
	if (GetOwningPlayerController() && GetOwningPlayerController()->GetMousePosition(MX, MY)) Mouse = FVector2D(MX, MY) / UiNow;
	FString Hover;
	float RowY = Y + Pad;
	for (const FTCControlRow& R : Rows)
	{
		const FLinearColor Colour = R.Tone == FTCControlRow::Dim ? Dim : R.Tone == FTCControlRow::Warn ? Warn : R.Tone == FTCControlRow::Good ? Good : Text;
		if (!R.Items.Num()) { DrawLabel(R.Text, Colour, X + 6.f, RowY, Font, 1.f); RowY += Line; continue; }
		float BX = X + 6.f;
		if (!R.Text.IsEmpty()) { DrawLabel(R.Text, Colour, BX, RowY + 2.f + Pad / 2, Font, 1.f); BX += Measure(R.Text) - Pad; }
		for (const FTCControlItem& It : R.Items)
		{
			// round 16 (hudfit): the match menu also holds the HUD's settings ('Setting.<what>' keys, ATCHUD::MenuSettingRows),
			// whose clicks are the HUD's own actions; every other key is a match control as before
			const FTCUiCommand Cmd = ATCHUD::ControlCommand(It.Key, It.Slot);
			const float Left = BX;
			BX = ATCHUD::Button(Font, BX, RowY + 2.f, It.Label, Cmd, It.bEnabled, !It.bEnabled ? CardOff : It.bOn ? TabOpen : ButtonFill);
			if (FBox2D(FVector2D(Left, RowY + 2.f), FVector2D(BX - Pad, RowY + 2.f + Line + Pad)).IsInsideOrOn(Mouse)) Hover = It.Hover;
		}
		RowY += RowH;
	}
	if (!Hover.IsEmpty()) DrawLabel(Hover, Good, X + 6.f, RowY, Font, 1.f);
	return Y + H + 4.f;
}

FVector2D ATCHUD::MeasureMatchControls(UFont* Font)
{
	// the same size DrawControlRows gives its panel (W = 0: as wide as the widest row)
	using namespace TCCtlGlue;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M) return FVector2D::ZeroVector;
	const TArray<FTCControlRow> Rows = M->MatchControlRows();
	if (!Rows.Num()) return FVector2D::ZeroVector;
	auto Measure = [&](const FString& Lb) { float TW = 0, TH = 0; LabelSize(Lb, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	float H = 2 * Pad + Line, Wide = 0.f;
	for (const FTCControlRow& R : Rows)
	{
		H += R.Items.Num() ? Line + Pad + 4.f : Line;
		float RW = (R.Text.IsEmpty() ? 0.f : Measure(R.Text)) + 2 * Pad;
		for (const FTCControlItem& It : R.Items) RW += Measure(It.Label) + Pad;
		Wide = FMath::Max(Wide, RW);
	}
	return FVector2D(Wide, H);
}

void ATCHUD::DrawMatchControls(UFont* Font, float Top)
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M) return;
	DrawControlRows(Font, 8.f, Top, 0.f, M->MatchControlRows());
}

#if defined(TC_HAS_ROOM_BANNERS)
void ATCHUD::DrawRoomBanners(UFont* Font)
{
	// round 16: #roomBanners (main.js positionRoomBanners: 8 px under #top's bottom in screen pixels while the HUD shows,
	// else 16 px). Round 17 (hudui): #connectionBanner first while the connection waits to try again (connection.js
	// 'retry', main.js's two sentences, FTCHudFit::ConnectionBannerText), then the pause banner.
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !Canvas || !Font) return;
	TArray<FTCRoomBanner> Banners;
	bool bFull = false;
	const int32 Left = M->ConnectionRetryLeft(&bFull);
	if (Left > 0) Banners.Add(FTCRoomBanner{ FTCHudFit::ConnectionBannerText(bFull, Left), true });
	if (M->State.bPaused && !M->State.PauseText.IsEmpty()) Banners.Add(FTCRoomBanner{ M->State.PauseText, false });
	if (!Banners.Num()) return;
	// #top holds the strip and its centred lines. EndUi has reset UiNow, so use the saved physical bottom.
	const bool bHud = bShowHUD && M->State.bHasMatch && TopStackBottomPx >= 0.f;
	const float Top = bHud ? FMath::CeilToFloat(TopStackBottomPx + 8.f) : 16.f;
	const FTCPaint P = FTCRoomBanners::Layout(Banners, Top, Canvas->ClipX, FTCPaint::CanvasMeasure(Font));
	P.Draw(Canvas, Font);
}
#endif
#endif // TC_HAS_LOBBY_GLUE

// ---------------------------------------------------------------------------------------------------------------------
#if WITH_DEV_AUTOMATION_TESTS

// BEGIN GENERATED FIXTURES (round-15 lobby tools/write-control-fixtures.mjs: lobbies recorded live from an isolated
// reference server at e4e579a by protocol-probe.mjs, the messages it sent, and client/i18n.js spanish() under node)
namespace TCCtlFx
{
	const TCHAR* const HostLobby = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":false,\"amHost\":true,\"spectators\":[],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"lobby\",\"you\":0,\"host\":0,\"mapName\":\"default\",\"spawns\":3,\"publicUrl\":\"https://public.example\",\"mode\":\"conquest\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":false,\"result\":null,\"players\":[{\"name\":\"Alder\",\"connected\":true,\"ai\":false,\"team\":0,\"faction\":0,\"level\":null},{\"name\":\"Birch\",\"connected\":true,\"ai\":false,\"team\":1,\"faction\":1,\"level\":null}]}");
	const TCHAR* const GuestWatching = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":true,\"amHost\":false,\"spectators\":[\"Birch\"],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"lobby\",\"you\":-1,\"host\":0,\"mapName\":\"default\",\"spawns\":3,\"publicUrl\":\"https://public.example\",\"mode\":\"conquest\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":false,\"result\":null,\"players\":[{\"name\":\"Alder\",\"connected\":true,\"ai\":false,\"team\":0,\"faction\":0,\"level\":null}]}");
	const TCHAR* const GuestSeated = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":false,\"amHost\":false,\"spectators\":[],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"lobby\",\"you\":1,\"host\":0,\"mapName\":\"default\",\"spawns\":3,\"publicUrl\":\"https://public.example\",\"mode\":\"conquest\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":false,\"result\":null,\"players\":[{\"name\":\"Alder\",\"connected\":true,\"ai\":false,\"team\":0,\"faction\":0,\"level\":null},{\"name\":\"Birch\",\"connected\":true,\"ai\":false,\"team\":1,\"faction\":1,\"level\":null}]}");
	const TCHAR* const GuestRenamed = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":false,\"amHost\":false,\"spectators\":[],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"lobby\",\"you\":1,\"host\":0,\"mapName\":\"default\",\"spawns\":3,\"publicUrl\":\"https://public.example\",\"mode\":\"conquest\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":false,\"result\":null,\"players\":[{\"name\":\"Alder\",\"connected\":true,\"ai\":false,\"team\":0,\"faction\":0,\"level\":null},{\"name\":\"Birch  co\",\"connected\":true,\"ai\":false,\"team\":1,\"faction\":1,\"level\":null}]}");
	const TCHAR* const GuestPlay = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":false,\"amHost\":false,\"spectators\":[],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"play\",\"you\":1,\"host\":0,\"mapName\":\"bastogne\",\"spawns\":5,\"publicUrl\":\"https://public.example\",\"mode\":\"horde\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":true,\"result\":null,\"players\":[{\"name\":\"Alder\",\"connected\":true,\"ai\":false,\"team\":0,\"faction\":0,\"level\":null},{\"name\":\"Birch  co\",\"connected\":true,\"ai\":false,\"team\":1,\"faction\":1,\"level\":null}]}");
	const TCHAR* const HostPlayOffline = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":false,\"amHost\":true,\"spectators\":[],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"play\",\"you\":0,\"host\":0,\"mapName\":\"bastogne\",\"spawns\":5,\"publicUrl\":\"https://public.example\",\"mode\":\"horde\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":true,\"result\":null,\"players\":[{\"name\":\"Alder\",\"connected\":true,\"ai\":false,\"team\":0,\"faction\":0,\"level\":null},{\"name\":\"Birch  co\",\"connected\":false,\"ai\":false,\"team\":1,\"faction\":1,\"level\":null}]}");
	const TCHAR* const HostPlayHanded = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":false,\"amHost\":true,\"spectators\":[],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"play\",\"you\":0,\"host\":0,\"mapName\":\"bastogne\",\"spawns\":5,\"publicUrl\":\"https://public.example\",\"mode\":\"horde\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":true,\"result\":null,\"players\":[{\"name\":\"Alder\",\"connected\":true,\"ai\":false,\"team\":0,\"faction\":0,\"level\":null},{\"name\":\"Birch  co (AI)\",\"connected\":true,\"ai\":true,\"team\":1,\"faction\":1,\"level\":\"normal\"}]}");
	const TCHAR* const WatcherPlay = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":true,\"amHost\":false,\"spectators\":[\"Cedar\"],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"play\",\"you\":-1,\"host\":0,\"mapName\":\"bastogne\",\"spawns\":5,\"publicUrl\":\"https://public.example\",\"mode\":\"horde\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":true,\"result\":null,\"players\":[{\"name\":\"Alder\",\"connected\":true,\"ai\":false,\"team\":0,\"faction\":0,\"level\":null},{\"name\":\"Birch  co (AI)\",\"connected\":true,\"ai\":true,\"team\":1,\"faction\":1,\"level\":\"normal\"}]}");
	const TCHAR* const WatcherHostPlay = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":true,\"amHost\":true,\"spectators\":[\"Cedar\"],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"play\",\"you\":-1,\"host\":-1,\"mapName\":\"bastogne\",\"spawns\":5,\"publicUrl\":\"https://public.example\",\"mode\":\"horde\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":true,\"result\":null,\"players\":[{\"name\":\"Alder (AI)\",\"connected\":true,\"ai\":true,\"team\":0,\"faction\":0,\"level\":\"normal\"},{\"name\":\"Birch  co (AI)\",\"connected\":true,\"ai\":true,\"team\":1,\"faction\":1,\"level\":\"normal\"}]}");
	const TCHAR* const WatcherEnded = TEXT("{\"role\":\"commander\",\"operatives\":[],\"spectator\":true,\"amHost\":true,\"spectators\":[\"Cedar\"],\"listed\":false,\"hordeBest\":null,\"t\":\"lobby\",\"code\":\"lp7vh54r\",\"state\":\"lobby\",\"you\":-1,\"host\":-1,\"mapName\":\"bastogne\",\"spawns\":5,\"publicUrl\":\"https://public.example\",\"mode\":\"horde\",\"worldSize\":\"huge\",\"defenderTeam\":0,\"army\":\"standard\",\"weather\":\"map\",\"sandbox\":true,\"result\":{\"ended\":true,\"you\":null},\"players\":[{\"name\":\"Alder (AI)\",\"connected\":true,\"ai\":true,\"team\":0,\"faction\":0,\"level\":\"normal\"},{\"name\":\"Birch  co (AI)\",\"connected\":true,\"ai\":true,\"team\":1,\"faction\":1,\"level\":\"normal\"}]}");
	const TCHAR* const Sent[] = {
		TEXT("{\"t\":\"spectate\"}"),
		TEXT("{\"t\":\"sit\"}"),
		TEXT("{\"t\":\"name\",\"name\":\" <B>irch & co\"}"),
		TEXT("{\"t\":\"restart\"}"),
		TEXT("{\"t\":\"mode\",\"v\":\"horde\"}"),
		TEXT("{\"t\":\"sandbox\",\"v\":true}"),
		TEXT("{\"t\":\"start\"}"),
		TEXT("{\"t\":\"nextwave\"}"),
		TEXT("{\"t\":\"pause\"}"),
		TEXT("{\"t\":\"resume\"}"),
		TEXT("{\"t\":\"spawn\",\"unit\":\"rifle\",\"enemy\":false,\"x\":100,\"z\":100}"),
		TEXT("{\"t\":\"spawn\",\"unit\":\"mg\",\"enemy\":true,\"x\":120,\"z\":100}"),
		TEXT("{\"t\":\"spawn\",\"unit\":\"hq\",\"enemy\":false,\"x\":100,\"z\":100}"),
		TEXT("{\"t\":\"handAi\",\"slot\":1}"),
		TEXT("{\"t\":\"leave\"}"),
		TEXT("{\"t\":\"end\"}"),
	};
	struct FEs { const TCHAR* En; const TCHAR* Es; };
	const FEs Spanish[] = {
		{ TEXT("Watch as a spectator"), TEXT("Mirar como espectador") },
		{ TEXT("Take a seat"), TEXT("Ocupar un puesto") },
		{ TEXT("Spectators see the whole map and command nothing. With only AI players seated, a spectator hosts"), TEXT("Los espectadores ven todo el mapa y no controlan nada. Si solo hay jugadores IA sentados, un espectador hace de anfitri\u00F3n") },
		{ TEXT("Spectators share one team's explored world and command nothing"), TEXT("Spectators share one team's explored world and command nothing") },
		{ TEXT("Name"), TEXT("Nombre") },
		{ TEXT("Copy"), TEXT("Copiar") },
		{ TEXT("Copied"), TEXT("Copiado") },
		{ TEXT("Press Ctrl+C"), TEXT("Pulsa Ctrl+C") },
		{ TEXT("Copy the invite link"), TEXT("Copia el enlace de invitaci\u00F3n") },
		{ TEXT("Invite"), TEXT("Invitar") },
		{ TEXT("Send this link to friends. It always opens this room."), TEXT("Env\u00EDa este enlace a tus amigos. Siempre abre esta sala.") },
		{ TEXT("Room"), TEXT("Sala") },
		{ TEXT("Join"), TEXT("Unirse") },
		{ TEXT("New"), TEXT("Nueva") },
		{ TEXT("Join the room code typed on the left"), TEXT("Unirse a la sala escrita a la izquierda") },
		{ TEXT("A private room with a random code"), TEXT("Una sala privada con un c\u00F3digo aleatorio") },
		{ TEXT("Menu"), TEXT("Men\u00FA") },
		{ TEXT("Menu: restart, end or leave the match"), TEXT("Men\u00FA: reiniciar, terminar o abandonar la partida") },
		{ TEXT("Pause match"), TEXT("Pausar partida") },
		{ TEXT("Resume match"), TEXT("Reanudar partida") },
		{ TEXT("Save match"), TEXT("Guardar partida") },
		{ TEXT("Save this match to load it again later from this room's lobby"), TEXT("Guarda esta partida para cargarla m\u00E1s tarde desde la sala de esta partida") },
		{ TEXT("Restart match"), TEXT("Reiniciar partida") },
		{ TEXT("End match (back to lobby)"), TEXT("Terminar partida (volver a la sala)") },
		{ TEXT("Leave game"), TEXT("Abandonar juego") },
		{ TEXT("Click again to confirm"), TEXT("Haz clic otra vez para confirmar") },
		{ TEXT("Hand Birch to AI"), TEXT("Pasar a Birch a la IA") },
		{ TEXT("Send next wave"), TEXT("Send next wave") },
		{ TEXT("Skip the break"), TEXT("Skip the break") },
		{ TEXT("Sandbox"), TEXT("Sandbox") },
		{ TEXT("Enemy"), TEXT("Enemigo") },
		{ TEXT("Mine"), TEXT("Mina") },
		{ TEXT("Spawn"), TEXT("Punto de inicio") },
		{ TEXT("Then click the map, once per unit. Right-click stops"), TEXT("Then click the map, once per unit. Right-click stops") },
		{ TEXT("Rifle Squad"), TEXT("Pelot\u00F3n de fusileros") },
		{ TEXT("Medium Tank"), TEXT("Tanque medio") },
		{ TEXT("You left the match; an AI took over your army. Reload to rejoin the lobby when the match is over."), TEXT("Saliste de la partida; una IA tom\u00F3 tu ej\u00E9rcito. Recarga para volver a la sala cuando termine la partida.") },
		{ TEXT("Browse matches"), TEXT("Ver partidas") },
		{ TEXT("Quick Play"), TEXT("Partida r\u00E1pida") },
		{ TEXT("Tutorial"), TEXT("Tutorial") },
		{ TEXT("Refresh"), TEXT("Actualizar") },
		{ TEXT("All modes"), TEXT("Todos los modos") },
		{ TEXT("Open seats only"), TEXT("Solo con puestos libres") },
		{ TEXT("Connecting\u2026"), TEXT("Conectando\u2026") },
		{ TEXT("5 public matches \u00B7 Live"), TEXT("5 partidas p\u00FAblicas \u00B7 En vivo") },
		{ TEXT("1 public match \u00B7 Live"), TEXT("1 partida p\u00FAblica \u00B7 En vivo") },
		{ TEXT("Offline"), TEXT("Desconectado") },
		{ TEXT("Match / map"), TEXT("Partida / mapa") },
		{ TEXT("Mode"), TEXT("Modo") },
		{ TEXT("Seats"), TEXT("Puestos") },
		{ TEXT("Status"), TEXT("Estado") },
		{ TEXT("In progress"), TEXT("En curso") },
		{ TEXT("Open"), TEXT("Abierta") },
		{ TEXT("Full"), TEXT("Completo") },
		{ TEXT("2 humans"), TEXT("2 humanos") },
		{ TEXT("1 human \u00B7 2 AI"), TEXT("1 humano \u00B7 2 IA") },
		{ TEXT("Join Friday night"), TEXT("Join Friday night") },
		{ TEXT("No matches fit these filters."), TEXT("Ninguna partida cumple estos filtros.") },
		{ TEXT("No public matches yet. Create one and invite the first players."), TEXT("A\u00FAn no hay partidas p\u00FAblicas. Crea una e invita a los primeros jugadores.") },
		{ TEXT("Cannot reach the match server. Retry with Refresh."), TEXT("No se puede conectar con el servidor. Reintenta con Actualizar.") },
		{ TEXT("Create a match"), TEXT("Crear una partida") },
		{ TEXT("Match name"), TEXT("Nombre de la partida") },
		{ TEXT("Visibility"), TEXT("Visibilidad") },
		{ TEXT("Public: shown in the match browser"), TEXT("P\u00FAblica: aparece en el buscador de partidas") },
		{ TEXT("Unlisted: share the room link"), TEXT("Oculta: comparte el enlace de la sala") },
		{ TEXT("Create match"), TEXT("Crear partida") },
		{ TEXT("Have a room code?"), TEXT("\u00BFTienes un c\u00F3digo de sala?") },
		{ TEXT("Room code"), TEXT("C\u00F3digo de sala") },
		{ TEXT("Join room"), TEXT("Unirse a la sala") },
		{ TEXT("Your name"), TEXT("Tu nombre") },
		{ TEXT("Open skirmish"), TEXT("Escaramuza abierta") },
		{ TEXT("The directory is unavailable or still loading. Please try again."), TEXT("El directorio no est\u00E1 disponible o a\u00FAn est\u00E1 cargando. Int\u00E9ntalo de nuevo.") },
		{ TEXT("World Conquest"), TEXT("World Conquest") },
		{ TEXT("Horde"), TEXT("Horda") },
	};
}
// END GENERATED FIXTURES

#include "Client/TCLocale.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "Policies/CondensedJsonPrintPolicy.h"

namespace TCCtlT
{
	TSharedPtr<FJsonObject> Parse(const FString& Src)
	{
		TSharedPtr<FJsonObject> O;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Src), O);
		return O;
	}
	FString Wire(const TSharedPtr<FJsonObject>& M)
	{
		if (!M.IsValid()) return TEXT("(nothing)");
		FString Out;
		const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> W = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
		FJsonSerializer::Serialize(M.ToSharedRef(), W);
		return Out;
	}
	// a recorded lobby with some fields changed (the cases the probe did not meet)
	FString Edit(const TCHAR* Src, TFunctionRef<void(FJsonObject&)> Change)
	{
		const TSharedPtr<FJsonObject> O = Parse(Src);
		Change(*O);
		return Wire(O);
	}
	// the probe's sent message with this start (the browser's JSON.stringify)
	FString Sent(const TCHAR* Prefix)
	{
		for (const TCHAR* S : TCCtlFx::Sent) if (FString(S).StartsWith(Prefix)) return S;
		return TEXT("(not recorded)");
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMatchControlsReferenceTest, "ThreeCrossroads.MatchControls.Reference", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCMatchControlsReferenceTest::RunTest(const FString& Parameters)
{
	auto Load = [this](FTCMatchControls& C, const FString& Src, const TCHAR* What)
	{
		const TSharedPtr<FJsonObject> O = TCCtlT::Parse(Src);
		TestTrue(FString::Printf(TEXT("%s: a recorded lobby applies"), What), O.IsValid() && C.ApplyLobby(*O));
	};
	auto Sends = [this](const FTCControlRequest& R, const FString& Expected, const TCHAR* What)
	{
		TestEqual(FString::Printf(TEXT("%s sends the browser's message (%s)"), What, *R.Note), TCCtlT::Wire(R.Message), Expected);
	};
	auto Silent = [this](const FTCControlRequest& R, const TCHAR* What)
	{
		TestFalse(FString::Printf(TEXT("%s: nothing sent (%s)"), What, *R.Note), R.Message.IsValid());
	};
	auto Shown = [](const TArray<FTCControlButton>& L, ETCMatchControl K, int32 Slot = -1)
	{
		for (const FTCControlButton& B : L) if (B.Kind == K && B.Slot == Slot) return B.bShown;
		return false;
	};

	// --- Watch as a spectator / Take a seat (spectate, sit) ---
	{
		FTCMatchControls C;
		TestFalse(TEXT("no Watch button before a lobby"), C.WatchButton().bShown);
		Silent(C.PlanWatch(), TEXT("Watch before a lobby"));
		Load(C, TCCtlFx::HostLobby, TEXT("host"));
		FTCControlButton W = C.WatchButton();
		TestTrue(TEXT("seated: Watch as a spectator"), W.bShown && W.Label == TEXT("Watch as a spectator"));
		TestEqual(TEXT("its title"), W.Title, FString(TEXT("Spectators see the whole map and command nothing. With only AI players seated, a spectator hosts")));
		const FTCControlRequest R = C.PlanWatch();
		Sends(R, TCCtlT::Sent(TEXT("{\"t\":\"spectate\"")), TEXT("Watch"));
		C.NoteSent(R);
		TestTrue(TEXT("asked, pending"), C.WatchButton().bPending);
		Load(C, TCCtlFx::GuestWatching, TEXT("guest watching"));
		W = C.WatchButton();
		TestTrue(TEXT("watching: Take a seat, no longer pending"), W.bShown && W.Label == TEXT("Take a seat") && !W.bPending && C.Confirmed() == 1);
		TestTrue(TEXT("watching"), C.Watching());
		Sends(C.PlanWatch(), TCCtlT::Sent(TEXT("{\"t\":\"sit\"")), TEXT("Take a seat"));
		Load(C, TCCtlFx::GuestSeated, TEXT("guest seated"));
		TestEqual(TEXT("seated again"), C.WatchButton().Label, FString(TEXT("Watch as a spectator")));
		Load(C, TCCtlFx::GuestPlay, TEXT("guest in play"));
		TestFalse(TEXT("hidden in play"), C.WatchButton().bShown);
		Silent(C.PlanWatch(), TEXT("Watch in play"));
		// World Conquest's title; an operative never sees it; a public room at its seat limit hides Take a seat
		Load(C, TCCtlT::Edit(TCCtlFx::HostLobby, [](FJsonObject& O) { O.SetStringField(TEXT("mode"), TEXT("world")); }), TEXT("world"));
		TestEqual(TEXT("world title"), C.WatchButton().Title, FString(TEXT("Spectators share one team's explored world and command nothing")));
		Load(C, TCCtlT::Edit(TCCtlFx::HostLobby, [](FJsonObject& O) { O.SetStringField(TEXT("role"), TEXT("operative")); }), TEXT("operative"));
		TestFalse(TEXT("operative: hidden"), C.WatchButton().bShown);
		Load(C, TCCtlT::Edit(TCCtlFx::GuestWatching, [](FJsonObject& O) { O.SetBoolField(TEXT("listed"), true); O.SetNumberField(TEXT("spawns"), 1); }), TEXT("public, full"));
		TestEqual(TEXT("seat limit of a public room"), C.SeatLimit(), 1);
		TestFalse(TEXT("watching a full public room: no Take a seat"), C.WatchButton().bShown);
		Load(C, TCCtlT::Edit(TCCtlFx::GuestWatching, [](FJsonObject& O) { O.SetBoolField(TEXT("listed"), true); O.RemoveField(TEXT("spawns")); }), TEXT("public, no spawns"));
		TestEqual(TEXT("public room without spawns: 3"), C.SeatLimit(), 3);
		TestTrue(TEXT("a seat is free"), C.WatchButton().bShown);
	}

	// --- Name ---
	{
		TestEqual(TEXT("a given name is cut to 16"), FTCMatchControls::InitialName(TEXT("Abcdefghijklmnopqrst"), TEXT("Saved"), 42), FString(TEXT("Abcdefghijklmnop")));
		TestEqual(TEXT("else the saved one"), FTCMatchControls::InitialName(FString(), TEXT("Saved"), 42), FString(TEXT("Saved")));
		TestEqual(TEXT("else SoldierNN"), FTCMatchControls::InitialName(FString(), FString(), 42), FString(TEXT("Soldier42")));
		FTCMatchControls C;
		C.SetFieldName(TEXT("Birch"));
		Load(C, TCCtlFx::GuestSeated, TEXT("guest seated"));
		Silent(C.PlanRename(TEXT("Birch")), TEXT("an unchanged name"));
		const FTCControlRequest R = C.PlanRename(TEXT(" <B>irch & co"));
		Sends(R, TCCtlT::Sent(TEXT("{\"t\":\"name\"")), TEXT("Rename"));
		C.NoteSent(R);
		TestTrue(TEXT("rename pending"), C.NameField().bPending);
		Load(C, TCCtlFx::GuestRenamed, TEXT("renamed"));
		TestFalse(TEXT("rename shown by the lobby"), C.NameField().bPending);
		TestEqual(TEXT("cleanName as the server keeps it"), FTCMatchControls::CleanName(TEXT(" <B>irch & co")), C.Seats()[1].Name);
		TestEqual(TEXT("cleanName of nothing"), FTCMatchControls::CleanName(TEXT(" <> ")), FString(TEXT("Soldier")));
		TestEqual(TEXT("the field holds 16"), C.PlanRename(TEXT("12345678901234567890")).Message->GetStringField(TEXT("name")), FString(TEXT("1234567890123456")));
		Load(C, TCCtlFx::GuestPlay, TEXT("guest in play"));
		TestTrue(TEXT("a name goes out in play too"), C.PlanRename(TEXT("Other")).Message.IsValid());
	}

	// --- invite link and Copy ---
	{
		FTCMatchControls C;
		TestEqual(TEXT("before a lobby: this server"), C.InviteLink(TEXT("ws://127.0.0.1:39405/ws"), TEXT("abc")), FString(TEXT("http://127.0.0.1:39405/play#abc")));
		Load(C, TCCtlFx::HostLobby, TEXT("host"));
		TestEqual(TEXT("local address: the server's publicUrl"), C.InviteLink(TEXT("ws://127.0.0.1:39405/ws"), TEXT("abc")), FString(TEXT("https://public.example/play#abc")));
		TestEqual(TEXT("localhost too"), C.InviteLink(TEXT("ws://localhost:3000/ws"), TEXT("abc")), FString(TEXT("https://public.example/play#abc")));
		TestEqual(TEXT("[::1] too"), C.InviteLink(TEXT("ws://[::1]:3000/ws"), TEXT("abc")), FString(TEXT("https://public.example/play#abc")));
		TestEqual(TEXT("another address: that origin"), C.InviteLink(TEXT("ws://192.168.1.5:3000/ws"), TEXT("abc")), FString(TEXT("http://192.168.1.5:3000/play#abc")));
		TestEqual(TEXT("tls"), C.InviteLink(TEXT("wss://game.example/ws"), TEXT("abc")), FString(TEXT("https://game.example/play#abc")));
		Load(C, TCCtlT::Edit(TCCtlFx::HostLobby, [](FJsonObject& O) { O.SetStringField(TEXT("publicUrl"), FString()); }), TEXT("no publicUrl"));
		TestEqual(TEXT("no publicUrl: this server"), C.InviteLink(TEXT("ws://127.0.0.1:39405/ws"), TEXT("abc")), FString(TEXT("http://127.0.0.1:39405/play#abc")));
		TestEqual(TEXT("Copy"), C.CopyButton(0).Label, FString(TEXT("Copy")));
		C.NoteCopied(true, 10.0);
		TestEqual(TEXT("Copied for 3 s"), C.CopyButton(12.9).Label, FString(TEXT("Copied")));
		TestEqual(TEXT("then Copy"), C.CopyButton(13.0).Label, FString(TEXT("Copy")));
		C.NoteCopied(false, 20.0);
		TestEqual(TEXT("the fallback"), C.CopyButton(21.0).Label, FString(TEXT("Press Ctrl+C")));
	}

	// --- Room box ---
	{
		const FTCControlRequest J = FTCMatchControls::PlanJoinRoom(TEXT("  CrossRoads "), TEXT("abc"));
		TestEqual(TEXT("goRoom lowercases and trims"), J.Room, FString(TEXT("crossroads")));
		TestTrue(TEXT("goRoom refuses a short code"), FTCMatchControls::PlanJoinRoom(TEXT("ab"), TEXT("abc")).Room.IsEmpty());
		TestTrue(TEXT("goRoom refuses a dash"), FTCMatchControls::PlanJoinRoom(TEXT("ab-cd"), TEXT("abc")).Room.IsEmpty());
		int32 K = 0;
		TestEqual(TEXT("New: five base-36 characters"), FTCMatchControls::PlanNewRoom([&](int32 N) { return (K++ * 7) % N; }).Room, FString(TEXT("07elsz")).Left(5));
		const FString Fresh = FTCMatchControls::PlanNewRoom().Room;
		TestTrue(TEXT("a new room code is valid"), Fresh.Len() == 5);
		TestFalse(TEXT("the room box sends nothing itself"), J.Message.IsValid());
	}

	// --- the match menu: host with an offline seat, then handed to an AI ---
	{
		FTCMatchControls C;
		Load(C, TCCtlFx::HostPlayOffline, TEXT("host, a seat offline"));
		TestTrue(TEXT("Menu shown in play"), C.MenuButton().bShown);
		TestEqual(TEXT("Menu title"), C.MenuButton().Title, FString(TEXT("Menu: restart, end or leave the match")));
		C.ToggleMenu();
		TestTrue(TEXT("menu open"), C.MenuOpen());
		TArray<FTCControlButton> L = C.MenuButtons(0);
		TestTrue(TEXT("host: Pause, Save, Restart, End, Leave"), Shown(L, ETCMatchControl::Pause) && Shown(L, ETCMatchControl::Save) && Shown(L, ETCMatchControl::Restart) && Shown(L, ETCMatchControl::End) && Shown(L, ETCMatchControl::Leave));
		TestTrue(TEXT("Hand <offline seat> to AI"), Shown(L, ETCMatchControl::HandAi, 1) && !Shown(L, ETCMatchControl::HandAi, 0));
		TestEqual(TEXT("its label"), C.MenuControl(ETCMatchControl::HandAi, 1, 0).Label, FString::Printf(TEXT("Hand %s to AI"), *C.Seats()[1].Name));
		TestEqual(TEXT("menu order"), (int32)L.Num(), 4 + C.Seats().Num() + 1);
		const FTCControlRequest H = C.PressMenu(ETCMatchControl::HandAi, 1, 1.0);
		Sends(H, TCCtlT::Sent(TEXT("{\"t\":\"handAi\"")), TEXT("Hand to AI"));
		TestTrue(TEXT("Hand to AI keeps the menu open"), C.MenuOpen());
		Silent(C.PressMenu(ETCMatchControl::HandAi, 0, 1.0), TEXT("Hand an online seat"));
		// Pause and Resume follow the pause message, and close the menu
		Sends(C.PressMenu(ETCMatchControl::Pause, -1, 2.0), TCCtlT::Sent(TEXT("{\"t\":\"pause\"")), TEXT("Pause match"));
		TestFalse(TEXT("Pause closes the menu"), C.MenuOpen());
		C.SetPaused(true);
		TestEqual(TEXT("paused: Resume match"), C.MenuControl(ETCMatchControl::Pause, -1, 0).Label, FString(TEXT("Resume match")));
		Sends(C.PressMenu(ETCMatchControl::Pause, -1, 2.5), TCCtlT::Sent(TEXT("{\"t\":\"resume\"")), TEXT("Resume match"));
		TestEqual(TEXT("Save"), TCCtlT::Wire(C.PressMenu(ETCMatchControl::Save, -1, 3.0).Message), FString(TEXT("{\"t\":\"save\"}")));
		// confirmClick: a second click within 3 s
		C.ToggleMenu();
		FTCControlRequest R = C.PressMenu(ETCMatchControl::Restart, -1, 10.0);
		TestTrue(TEXT("Restart: the first click arms"), R.bArmed && !R.Message.IsValid() && C.MenuOpen());
		TestEqual(TEXT("armed label"), C.MenuControl(ETCMatchControl::Restart, -1, 12.9).Label, FString(TEXT("Click again to confirm")));
		TestEqual(TEXT("the label returns after 3 s"), C.MenuControl(ETCMatchControl::Restart, -1, 13.0).Label, FString(TEXT("Restart match")));
		TestEqual(TEXT("End is not armed by Restart"), C.MenuControl(ETCMatchControl::End, -1, 11.0).Label, FString(TEXT("End match (back to lobby)")));
		R = C.PressMenu(ETCMatchControl::Restart, -1, 12.9);
		Sends(R, TCCtlT::Sent(TEXT("{\"t\":\"restart\"")), TEXT("Restart, confirmed"));
		TestFalse(TEXT("confirmed: the menu closes"), C.MenuOpen());
		TestEqual(TEXT("and the label is back"), C.MenuControl(ETCMatchControl::Restart, -1, 13.0).Label, FString(TEXT("Restart match")));
		TestTrue(TEXT("a click 3 s after arming arms again"), C.PressMenu(ETCMatchControl::End, -1, 20.0).bArmed && C.PressMenu(ETCMatchControl::End, -1, 23.0).bArmed);
		Sends(C.PressMenu(ETCMatchControl::End, -1, 24.0), TCCtlT::Sent(TEXT("{\"t\":\"end\"")), TEXT("End, confirmed"));
		C.PressMenu(ETCMatchControl::Leave, -1, 30.0);
		Sends(C.PressMenu(ETCMatchControl::Leave, -1, 31.0), TCCtlT::Sent(TEXT("{\"t\":\"leave\"")), TEXT("Leave, confirmed"));
		Load(C, TCCtlFx::HostPlayHanded, TEXT("host, the seat handed"));
		TestFalse(TEXT("no Hand to AI once the seat is an AI"), Shown(C.MenuButtons(0), ETCMatchControl::HandAi, 1));
	}
	// a guest, a watcher, a watcher that hosts
	{
		FTCMatchControls G;
		Load(G, TCCtlFx::GuestPlay, TEXT("guest in play"));
		const TArray<FTCControlButton> L = G.MenuButtons(0);
		TestTrue(TEXT("guest: only Leave"), !Shown(L, ETCMatchControl::Pause) && !Shown(L, ETCMatchControl::Save) && !Shown(L, ETCMatchControl::Restart) && !Shown(L, ETCMatchControl::End) && Shown(L, ETCMatchControl::Leave));
		Silent(G.PressMenu(ETCMatchControl::Restart, -1, 1.0), TEXT("Restart by a guest"));
		FTCMatchControls W;
		Load(W, TCCtlFx::WatcherPlay, TEXT("watcher in play"));
		TestFalse(TEXT("a watcher has no Leave"), Shown(W.MenuButtons(0), ETCMatchControl::Leave));
		Silent(W.PressMenu(ETCMatchControl::Leave, -1, 1.0), TEXT("Leave by a watcher"));
		FTCMatchControls WH;
		Load(WH, TCCtlFx::WatcherHostPlay, TEXT("watcher hosting"));
		const TArray<FTCControlButton> HL = WH.MenuButtons(0);
		TestTrue(TEXT("a watching host: host controls, no Leave"), Shown(HL, ETCMatchControl::Restart) && Shown(HL, ETCMatchControl::End) && !Shown(HL, ETCMatchControl::Leave));
		TestTrue(TEXT("observer controls go out"), WH.Sendable(TEXT("end")) && WH.Sendable(TEXT("restart")) && WH.Sendable(TEXT("handAi")) && WH.Sendable(TEXT("name")));
		TestFalse(TEXT("orders do not"), WH.Sendable(TEXT("move")) || WH.Sendable(TEXT("nextwave")) || WH.Sendable(TEXT("spawn")));
		// Horde: the button shows for the host between waves; a watching host's click sends nothing (sendCmd)
		WH.SetHorde(true, false);
		TestTrue(TEXT("watching host: Send next wave shown, disabled"), WH.NextWaveButton().bShown && !WH.NextWaveButton().bEnabled);
		Silent(WH.PlanNextWave(), TEXT("next wave by a watching host"));
		Load(WH, TCCtlFx::WatcherEnded, TEXT("back in the lobby"));
		TestFalse(TEXT("lobby: no menu, no next wave"), WH.MenuButton().bShown || WH.MenuOpen() || WH.NextWaveButton().bShown);
	}

	// --- Horde: Send next wave ---
	{
		FTCMatchControls C;
		Load(C, TCCtlFx::HostPlayOffline, TEXT("host in play"));
		TestFalse(TEXT("not Horde: no button"), C.NextWaveButton().bShown);
		C.SetHorde(true, false);
		const FTCControlButton B = C.NextWaveButton();
		TestTrue(TEXT("Horde break, host: shown"), B.bShown && B.bEnabled && B.Label == TEXT("Send next wave") && B.Title == TEXT("Skip the break"));
		Sends(C.PlanNextWave(), TCCtlT::Sent(TEXT("{\"t\":\"nextwave\"")), TEXT("Send next wave"));
		C.SetHorde(true, true);
		TestFalse(TEXT("a wave on the map: hidden"), C.NextWaveButton().bShown);
		Silent(C.PlanNextWave(), TEXT("next wave during a wave"));
		FTCMatchControls G;
		Load(G, TCCtlFx::GuestPlay, TEXT("guest"));
		G.SetHorde(true, false);
		TestFalse(TEXT("a guest: hidden"), G.NextWaveButton().bShown);
	}

	// --- the sandbox panel ---
	{
		const TArrayView<const FTCSandboxUnit> Ref = FTCMatchControls::ReferenceSandboxUnits();
		const TArrayView<const FTCSandboxUnit> Panel = FTCMatchControls::SandboxUnits();
		TestEqual(TEXT("the reference lists 32 units"), Ref.Num(), 32);
		TestEqual(TEXT("the panel drops only the truck"), Panel.Num(), Ref.Num() - 1);
		bool bTruck = false;
		for (const FTCSandboxUnit& U : Panel) bTruck |= FCString::Strcmp(U.Type, TEXT("truck")) == 0;
		TestFalse(TEXT("no Supply Truck in the panel"), bTruck);
		FTCMatchControls C;
		Load(C, TCCtlFx::HostPlayOffline, TEXT("host in play"));
		TestFalse(TEXT("not a sandbox match: no panel"), C.SandboxShown());
		C.OnStart(true);
		TestTrue(TEXT("sandbox match: the panel"), C.SandboxShown());
		TestEqual(TEXT("first unit"), FString(C.SandboxUnit().Type), FString(Ref[0].Type));
		TestEqual(TEXT("Enemy by default"), C.SandboxButtons()[1].Label, FString(TEXT("Enemy")));
		Silent(C.PlanSpawn(100, 100), TEXT("a click before Spawn"));
		C.SetSandboxEnemy(false);
		C.ArmSpawn();
		TestTrue(TEXT("armed"), C.SpawnArmed());
		Sends(C.PlanSpawn(100, 100), TCCtlT::Sent(TEXT("{\"t\":\"spawn\",\"unit\":\"rifle\"")), TEXT("spawn own rifle"));
		TestTrue(TEXT("stays armed"), C.SpawnArmed());
		TestTrue(TEXT("mg by type"), C.SetSandboxUnit(TEXT("mg")));
		C.SetSandboxEnemy(true);
		Sends(C.PlanSpawn(120, 100), TCCtlT::Sent(TEXT("{\"t\":\"spawn\",\"unit\":\"mg\"")), TEXT("spawn enemy MG"));
		TestFalse(TEXT("truck cannot be picked"), C.SetSandboxUnit(TEXT("truck")));
		TestFalse(TEXT("nor a structure"), C.SetSandboxUnit(TEXT("hq")));
		C.SetSandboxUnit(TEXT("rifle"));
		C.StepSandboxUnit(-1);
		TestEqual(TEXT("before the first: the last of the panel"), FString(C.SandboxUnit().Type), FString(Panel.Last().Type));
		C.StepSandboxUnit(1);
		TestEqual(TEXT("wraps"), FString(C.SandboxUnit().Type), FString(TEXT("rifle")));
		const TSharedPtr<FJsonObject> Odd = C.PlanSpawn(12.25, -3.5).Message;
		TestTrue(TEXT("x and z as numbers"), Odd.IsValid() && Odd->GetNumberField(TEXT("x")) == 12.25 && Odd->GetNumberField(TEXT("z")) == -3.5);
		C.SetOut(true);
		Silent(C.PlanSpawn(1, 1), TEXT("spawn while out (observing)"));
		C.SetOut(false);
		C.DisarmSpawn();
		Silent(C.PlanSpawn(1, 1), TEXT("after a right click"));
		C.ArmSpawn();
		C.OnStart(true);
		TestFalse(TEXT("a new match disarms"), C.SpawnArmed());
		FTCMatchControls W;
		Load(W, TCCtlFx::WatcherPlay, TEXT("watcher"));
		W.OnStart(true);
		TestFalse(TEXT("a watcher has no panel"), W.SandboxShown());
		Silent(W.ArmSpawn(), TEXT("arm while watching"));
	}

	// --- Spanish: client/i18n.js spanish() answered under node, here through TC_T ---
	{
		FTCLocale::FScopedLanguage Es(ETCLanguage::Spanish);
		for (const TCCtlFx::FEs& E : TCCtlFx::Spanish) TestEqual(FString::Printf(TEXT("Spanish of '%s'"), E.En), TC_T(E.En), FString(E.Es));
	}
	return true;
}

#endif

#if WITH_DEV_AUTOMATION_TESTS
#include "Net/TCConnection.h"

// ThreeCrossroads.MatchControls.TextInput (round 16): the text fields take what the reference's inputs take. Expected
// values from the pinned code's own rules run outside the engine (round-16/controls/logs): headless Chromium's
// <input maxlength> for typed and inserted text (tools/paste-oracle.mjs: accents, the maxlength cut, line breaks to
// spaces, a surrogate pair never split) and node for server.js cleanName, JSON.stringify and the UTF-8 size of the
// rename message (logs/name-oracle.log).
// ThreeCrossroads.MatchControls.RoomBanners (round 16): connection.css #roomBanners: centred, 8 px under the strip, each
// box as wide as its text plus 12 px padding and a 1 px border, 4 px apart, at most min(510px, 100vw - 40px), wrapped.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMatchControlsRoomBannersTest, "ThreeCrossroads.MatchControls.RoomBanners", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCMatchControlsRoomBannersTest::RunTest(const FString& Parameters)
{
	const FTCMeasure M = FTCPaint::ApproxMeasure(); // 7 px a character at 14 px
	const FTCPaint P = FTCRoomBanners::Layout({ FTCRoomBanner{ TEXT("Paused by Alder"), false } }, 100.f, 1280.f, M);
	const FTCPaintItem* Box = P.Find(TEXT("banner:pause"));
	if (!TestNotNull(TEXT("the pause box"), Box)) return false;
	const float TextW = M(TEXT("Paused by Alder"), 14.f, false, 0.f);
	TestEqual(TEXT("width: text, 2 x 12 px padding, 2 x 1 px border"), (float)Box->Size.X, TextW + 26.f);
	TestEqual(TEXT("height: one 18.9 px line, 2 x 6 px, 2 x 1 px"), (float)Box->Size.Y, 14.f * 1.35f + 14.f, 0.01f);
	TestEqual(TEXT("centred"), (float)(Box->Pos.X + Box->Size.X / 2), 640.f, 0.51f);
	TestEqual(TEXT("top"), (float)Box->Pos.Y, 100.f);
	TestEqual(TEXT("text"), P.TextsOf(TEXT("banner:text")), TArray<FString>{ TEXT("Paused by Alder") });
	// two banners: the connection one first with its red border, 4 px between; a long text wraps inside 510 px
	const FString Long = FString::ChrN(40, 'a') + TEXT(" ") + FString::ChrN(40, 'b') + TEXT(" ") + FString::ChrN(40, 'c');
	const FTCPaint Q = FTCRoomBanners::Layout({ FTCRoomBanner{ TEXT("Connection lost"), true }, FTCRoomBanner{ Long, false } }, 16.f, 1280.f, M);
	const FTCPaintItem* C = Q.Find(TEXT("banner:connection"));
	const FTCPaintItem* W = Q.Find(TEXT("banner:pause"));
	if (!TestNotNull(TEXT("both boxes"), C) || !TestNotNull(TEXT("both boxes"), W)) return false;
	TestEqual(TEXT("4 px apart"), (float)W->Pos.Y, (float)(C->Pos.Y + C->Size.Y + 4.f), 0.01f);
	TestTrue(TEXT("no wider than 510 px"), W->Size.X <= 510.f + 0.01f);
	TestEqual(TEXT("each 280 px word on its own line (484 px inside)"), Q.TextsOf(TEXT("banner:text")).Num(), 1 + 3);
	const FTCPaintItem* Frame = nullptr;
	for (const FTCPaintItem& I : Q.Items) if (I.Role == TEXT("banner:frame")) { Frame = &I; break; }
	TestTrue(TEXT("red border on the connection banner"), Frame && FMath::IsNearlyEqual(Frame->Colour.R, TCCss::Rgba(200, 72, 59, 0.8f).R));
	// a narrow window: min(510, 100vw - 40)
	const FTCPaint N = FTCRoomBanners::Layout({ FTCRoomBanner{ Long, false } }, 16.f, 300.f, M);
	TestTrue(TEXT("narrow: 260 px at most"), N.Find(TEXT("banner:pause"))->Size.X <= 260.f + 0.01f);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMatchControlsTextInputTest, "ThreeCrossroads.MatchControls.TextInput", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCMatchControlsTextInputTest::RunTest(const FString& Parameters)
{
	auto Typed = [](ETCTextField F, const FString& Start, const FString& Chars)
	{
		FTCTextFields T;
		T.bCharInput = true;
		T.Begin(F, Start);
		for (const TCHAR C : Chars) T.Char(C);
		return T.Buffer;
	};
	auto Pasted = [](ETCTextField F, const FString& Start, const FString& Text)
	{
		FTCTextFields T;
		T.Begin(F, Start);
		T.Paste(Text);
		return T.Buffer;
	};
	// paste-oracle.log, case by case (f: maxlength 16, the Name and nickname; t: 48, the match name; c: 12, a code)
	TestEqual(TEXT("accents typed"), Typed(ETCTextField::Name, TEXT(""), TEXT("José Peña ¿Sí?")), FString(TEXT("José Peña ¿Sí?")));
	TestEqual(TEXT("typed past maxlength"), Typed(ETCTextField::Nickname, TEXT("Soldado Ñandú"), TEXT("áéíóú")), FString(TEXT("Soldado Ñandúáéí")));
	TestEqual(TEXT("paste with CRLF, LF and CR"), Pasted(ETCTextField::MatchTitle, TEXT("Partida"), TEXT(" uno\r\ndos\nTres\rcuatro")), FString(TEXT("Partida uno dos Tres cuatro")));
	TestEqual(TEXT("paste cut at maxlength"), Pasted(ETCTextField::Name, TEXT("José"), TEXT("abcdefghijklmnopqrstuvwxyz")), FString(TEXT("Joséabcdefghijkl")));
	TestEqual(TEXT("paste: a pair at the cut stays out"), Pasted(ETCTextField::JoinCode, TEXT("abcdefghijk"), TEXT("\U0001F600x")), FString(TEXT("abcdefghijk")));
	TestEqual(TEXT("paste: a pair that fits goes in"), Pasted(ETCTextField::JoinCode, TEXT("abcdefghij"), TEXT("\U0001F600x")), FString(TEXT("abcdefghij\U0001F600")));
	TestEqual(TEXT("paste a newline"), Pasted(ETCTextField::MatchTitle, TEXT(""), TEXT("línea 1\nlínea 2")), FString(TEXT("línea 1 línea 2")));
	// typed control characters are keys, not text; a pair typed as its two halves; a lone second half is dropped
	TestEqual(TEXT("control characters"), Typed(ETCTextField::Name, TEXT("a"), TEXT("\b\r\t\x1b\x7f" "b")), FString(TEXT("ab")));
	TestEqual(TEXT("a typed pair"), Typed(ETCTextField::RoomCode, TEXT("ab"), TEXT("\U0001F600c")), FString(TEXT("ab\U0001F600c")));
	TestEqual(TEXT("a typed pair with no room for both halves"), Typed(ETCTextField::RoomCode, TEXT("abcdefghijk"), TEXT("\U0001F600")), FString(TEXT("abcdefghijk")));
	FString Lone; Lone.AppendChar((TCHAR)0xDE00);
	TestEqual(TEXT("a lone second half"), Typed(ETCTextField::Name, TEXT("a"), Lone), FString(TEXT("a")));
	TestEqual(TEXT("LimitLength"), FTCTextFields::LimitLength(TEXT("ab\U0001F600"), 3) + TEXT("|") + FTCTextFields::LimitLength(TEXT("abc"), 5) + TEXT("|") + FTCTextFields::LimitLength(TEXT("abc"), -1), FString(TEXT("ab|abc|")));
	// keys: Backspace takes a whole pair; Ctrl+V and Shift+Insert paste the clipboard; with the text input wired a
	// letter key types nothing (the character comes from Char); without it the key names still type
	{
		FTCTextFields T;
		T.bCharInput = true;
		T.Begin(ETCTextField::Name, TEXT("a\U0001F600"));
		ETCTextField Done; bool bCancel = false;
		T.Key(FName(TEXT("BackSpace")), false, Done, bCancel);
		TestEqual(TEXT("Backspace takes the pair"), T.Buffer, FString(TEXT("a")));
		TestTrue(TEXT("a letter key is taken"), T.Key(FName(TEXT("B")), false, Done, bCancel));
		TestEqual(TEXT("and types nothing with the text input"), T.Buffer, FString(TEXT("a")));
		const TFunction<FString()> Was = FTCTextFields::ClipboardText;
		FTCTextFields::ClipboardText = []() { return FString(TEXT("María\nJosé")); };
		T.bCtrl = true;
		T.Key(FName(TEXT("V")), false, Done, bCancel);
		TestEqual(TEXT("Ctrl+V"), T.Buffer, FString(TEXT("aMaría José")));
		T.bCtrl = false;
		T.Key(FName(TEXT("Insert")), true, Done, bCancel);
		TestEqual(TEXT("Shift+Insert, cut at 16"), T.Buffer, FString(TEXT("aMaría JoséMaría")));
		FTCTextFields::ClipboardText = Was;
		T.Key(FName(TEXT("Enter")), false, Done, bCancel);
		TestTrue(TEXT("Enter ends it"), Done == ETCTextField::Name && !bCancel);
		FTCTextFields K;
		K.Begin(ETCTextField::Name, TEXT(""));
		K.Key(FName(TEXT("A")), true, Done, bCancel);
		K.Key(FName(TEXT("One")), false, Done, bCancel);
		TestEqual(TEXT("no text input: key names still type"), K.Buffer, FString(TEXT("A1")));
		K.bCtrl = true;
		K.Key(FName(TEXT("C")), false, Done, bCancel);
		TestEqual(TEXT("no text input: Ctrl+C types nothing"), K.Buffer, FString(TEXT("A1")));
	}
	// the rename message (main.js { t: 'name', name }), its bytes and what server.js cleanName keeps (name-oracle.log)
	struct FCase { const TCHAR* Typed; const TCHAR* Wire; int32 Bytes; const TCHAR* Kept; };
	const FCase Cases[] = {
		{ TEXT("José Peña"), TEXT("{\"t\":\"name\",\"name\":\"José Peña\"}"), 33, TEXT("José Peña") },
		{ TEXT("  Ñandú <b>  "), TEXT("{\"t\":\"name\",\"name\":\"  Ñandú <b>  \"}"), 37, TEXT("Ñandú b") },
		{ TEXT(" Ana　"), TEXT("{\"t\":\"name\",\"name\":\" Ana　\"}"), 30, TEXT("Ana") },
		{ TEXT("ÁÉÍÓÚáéíóúñÑ¿¡ü"), TEXT("{\"t\":\"name\",\"name\":\"ÁÉÍÓÚáéíóúñÑ¿¡ü\"}"), 52, TEXT("ÁÉÍÓÚáéíóúñÑ¿¡ü") },
	};
	for (const FCase& C : Cases)
	{
		FTCMatchControls Ctl;
		Ctl.SetFieldName(TEXT("Soldier"));
		TSharedPtr<FJsonObject> Lobby;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TCCtlFx::GuestSeated), Lobby);
		if (Lobby.IsValid()) Ctl.ApplyLobby(*Lobby);
		const FTCControlRequest R = Ctl.PlanRename(C.Typed);
		if (!TestTrue(FString::Printf(TEXT("rename '%s' planned"), C.Typed), R.Message.IsValid())) continue;
		FString Wire;
		FJsonSerializer::Serialize(R.Message.ToSharedRef(), TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Wire));
		TestEqual(FString::Printf(TEXT("wire of '%s' equals JSON.stringify"), C.Typed), Wire, FString(C.Wire));
		TestEqual(FString::Printf(TEXT("UTF-8 bytes of '%s'"), C.Typed), FTCConnection::Utf8Length(Wire), C.Bytes);
		TestEqual(FString::Printf(TEXT("cleanName of '%s'"), C.Typed), FTCMatchControls::CleanName(C.Typed), FString(C.Kept));
	}
	return true;
}
#endif
