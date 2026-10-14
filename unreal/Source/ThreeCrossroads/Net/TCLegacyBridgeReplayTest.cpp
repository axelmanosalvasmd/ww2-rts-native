// ThreeCrossroads.Net.LegacyBridgeReplay: the Great Bridge tick-8400 recipient captures (release
// legacy-great-bridge-8400-efbd55c7, native capture commit efbd55c78b5bac981e57296032b64b2ed128d446, source reference
// e4e579a) fed line by line, unchanged, from the first payload through this client's receive path: the Rust view
// (TC_DECODER=rust, the default: FTCRustViewShadow::Feed before the parse, as FTCConnection::OnText runs before
// OnMessage), FTCMatchState, then FTCAlerts and FTCCombatEffects as ATCGameMode::OnMessage and OnCombatSnapshot drive
// them. Four streams: native-slot0, native-slot1, source-slot0, source-slot1 (seats 0 and 1 from their start packets).
//
// The bundle is not in the repository. Point the test at the extracted folder (the one holding manifest.json and
// streams/) with, in this order: the TC_BRIDGE_REPLAY_DIR environment variable, -TCBridgeReplay=<dir> on the command
// line, or the first line of <project>/Saved/BridgeReplay/path.txt. Without one the test warns that it did NOT run.
// unreal/Tools/bridge-replay/run.sh verifies the manifest's SHA-256 hashes, extracts the bundle into a scratch folder
// and runs this test. Results (per stream summary, the tick-8400 report, a dump of the applied client state and the
// native against source comparison per seat) go to TC_BRIDGE_REPLAY_OUT, else <project>/Saved/BridgeReplay/out.
//
// Limits: a file replay, not a live socket (no TCP, no reconnect, no timing); the alert and effect clocks are the
// snapshot's tick / 20 s; nothing is on screen; the alert feed runs its own capture point block (bPointAlerts), as
// client/alerts.js does, where the game mode raises those through TakePointAlerts. The capture's constructor random
// streams differ between source and native (the README's wx note): that is reported, not hidden.
#include "Net/TCMatchState.h"
#include "Net/TCRustViewShadow.h"
#include "Client/TCAlerts.h"
#include "Client/TCEffects.h"
#include "Client/TCUnitTable.h"
#include "Client/TCLocale.h"
#include "Client/TCObjectives.h"
#include "ThreeCrossroads.h"
#include "WW2Core.h"
#include "Misc/AutomationTest.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "HAL/FileManager.h"
#include "HAL/PlatformMisc.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCBridgeReplay
{
	const TCHAR* const Streams[] = { TEXT("native-slot0"), TEXT("native-slot1"), TEXT("source-slot0"), TEXT("source-slot1") };
	constexpr int32 CollapseTick = 8400;
	const TCHAR* const SayEnglish = TEXT("The great bridge is down. The way over now is the ford to the north or the rail bridge to the south.");
	// reference/ww2-rts client/es.js:658 (the key is the English line)
	const TCHAR* const SaySpanish = TEXT("El gran puente ha ca\u00EDdo. Ahora se cruza por el vado del norte o por el puente del ferrocarril del sur.");

	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> M;
		TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);
		return FJsonSerializer::Deserialize(Reader, M) && M.IsValid() ? M : nullptr;
	}

	FString N(double V) { return FTCObjectives::JsNumber(V); }
	FString B(bool V) { return V ? TEXT("1") : TEXT("0"); }
	template <typename T> FString Opt(const TOptional<T>& V) { return V.IsSet() ? N((double)V.GetValue()) : FString(TEXT("-")); }

	uint64 Fnv(const uint8* Data, int64 Len)
	{
		uint64 H = 0xcbf29ce484222325ull;
		for (int64 I = 0; I < Len; I++) { H ^= Data[I]; H *= 0x100000001b3ull; }
		return H;
	}
	template <typename T> FString Hash(const TArray<T>& A) { return FString::Printf(TEXT("%016llx"), (unsigned long long)Fnv((const uint8*)A.GetData(), (int64)A.Num() * sizeof(T))); }
	FString HashBits(const TBitArray<>& A)
	{
		TArray<uint8> Bytes;
		for (int32 I = 0; I < A.Num(); I++) Bytes.Add(A[I] ? 1 : 0);
		return Hash(Bytes);
	}

	// Where the extracted bundle is: TC_BRIDGE_REPLAY_DIR, -TCBridgeReplay=, or Saved/BridgeReplay/path.txt.
	FString BundleDir(FString& How)
	{
		FString Dir = FPlatformMisc::GetEnvironmentVariable(TEXT("TC_BRIDGE_REPLAY_DIR"));
		if (!Dir.IsEmpty()) { How = TEXT("TC_BRIDGE_REPLAY_DIR"); return Dir; }
		if (FParse::Value(FCommandLine::Get(), TEXT("TCBridgeReplay="), Dir) && !Dir.IsEmpty()) { How = TEXT("-TCBridgeReplay="); return Dir; }
		const FString File = FPaths::ProjectSavedDir() / TEXT("BridgeReplay") / TEXT("path.txt");
		TArray<FString> Lines;
		if (FFileHelper::LoadFileToStringArray(Lines, *File) && Lines.Num() && !Lines[0].TrimStartAndEnd().IsEmpty()) { How = File; return Lines[0].TrimStartAndEnd(); }
		return FString();
	}

	FString OutDir()
	{
		const FString Dir = FPlatformMisc::GetEnvironmentVariable(TEXT("TC_BRIDGE_REPLAY_OUT"));
		return Dir.IsEmpty() ? FPaths::ProjectSavedDir() / TEXT("BridgeReplay") / TEXT("out") : Dir;
	}

	// One section of the applied state: a name and its lines.
	struct FPart { FString Name; TArray<FString> Lines; };

	FString UnitLine(const FTCUnit& U)
	{
		TArray<FString> Tail;
		for (float T : U.Tail) Tail.Add(N(T));
		return FString::Printf(TEXT("%d %s owner %d x %s z %s rot %s aim %s hp %d supp %d target %d cover %d cd %d flags %d stars %d built %s tail [%s]"),
			U.Id, *U.Type, U.Owner, *N(U.X), *N(U.Z), *N(U.Rot), *N(U.Aim), U.Hp, U.Supp, U.Target, U.Cover, U.Cd, U.Flags, U.Stars, *N(U.Built), *FString::Join(Tail, TEXT(",")));
	}

	FString SectionLine(const FTCCellSection& S)
	{
		return FString::Printf(TEXT("section %s structure %s state %s hp %s maxHp %s material %s anchor %s"), *S.Id, *S.StructureId, *S.State,
			S.bHasHp ? *N(S.Hp) : TEXT("-"), S.bHasMaxHp ? *N(S.MaxHp) : TEXT("-"), *S.Material, *B(S.bAnchor));
	}

	FString CellLine(const FTCMatchState& S, int32 C)
	{
		const uint8* St = S.CellStates.Find(C);
		const FTCCellData* D = S.CellData.Find(C);
		const FTCCellSection* Sec = S.Sections.Find(C);
		return FString::Printf(TEXT("%d char %c level %d state %s%s%s"), C, (TCHAR)S.Map.Chars[C], (int32)S.Map.Levels[C], St ? *FString::FromInt(*St) : TEXT("-"),
			D ? *FString::Printf(TEXT(" ground %s object %s material %s"), *D->Ground, *D->Object, *D->Material) : TEXT(""),
			Sec ? *(TEXT(" ") + SectionLine(*Sec)) : TEXT(""));
	}

	FString ShotLine(const FTCShot& Sh)
	{
		return FString::Printf(TEXT("k %s kind %s effect %s material %s name %s id %s pos %s y %s shooter %s/%s target %s/%s flight %d seq %d m %d time %s at %s until %s expires %s gravity %s blast %s v %s,%s,%s kill %s terminal %s hit %s explosive %s local %s"),
			*Sh.K, *Sh.Kind, *Sh.Effect, *Sh.Material, *Sh.Name, *Sh.Id, Sh.bHasPos ? *FString::Printf(TEXT("%s,%s"), *N(Sh.X), *N(Sh.Z)) : TEXT("-"), Sh.bHasY ? *N(Sh.Y) : TEXT("-"),
			Sh.bHasShooter ? *FString::FromInt(Sh.F) : TEXT("-"), Sh.bHasShooterOwner ? *FString::FromInt(Sh.Fo) : TEXT("-"),
			Sh.bHasTarget ? *FString::FromInt(Sh.T) : TEXT("-"), Sh.bHasTargetOwner ? *FString::FromInt(Sh.To) : TEXT("-"),
			Sh.Flight, Sh.Seq, Sh.M, *N(Sh.Time), *N(Sh.At), *N(Sh.Until), *N(Sh.Expires), *N(Sh.Gravity), *N(Sh.Blast), *N(Sh.V.X), *N(Sh.V.Y), *N(Sh.V.Z),
			*B(Sh.bKill), *B(Sh.bTerminal), *B(Sh.bHit), *B(Sh.bExplosive), *B(Sh.bLocal));
	}

	FString FallingLine(const FTCFalling& F)
	{
		return FString::Printf(TEXT("%s kind %s material %s section %s x %s y %s z %s dir %s tilt %s mass %s gravity %s drag %s time %s born %s at %s until %s impulse %s v %s,%s,%s"),
			*F.Id, *F.Kind, *F.Material, *F.Section, *N(F.X), *N(F.Y), *N(F.Z), *N(F.Dir), *N(F.Tilt), *N(F.Mass), *N(F.Gravity), *N(F.Drag), *N(F.Time), *N(F.Born),
			*N(F.At), *N(F.Until), *N(F.Impulse), *N(F.V.X), *N(F.V.Y), *N(F.V.Z));
	}

	// The client state applied so far, section by section, every number written as JavaScript's String(n) writes it
	// (so 0 and 0.0, 2400 and 2400.0 read from the wire are the same text when the decoder holds the same double).
	TArray<FPart> Dump(const FTCMatchState& S, const FTCAlerts& A, const FTCCombatEffects& Fx)
	{
		TArray<FPart> P;
		auto Add = [&P](const TCHAR* Name) -> TArray<FString>& { P.Add({ Name, {} }); return P.Last().Lines; };
		{
			TArray<FString>& L = Add(TEXT("lobby"));
			L.Add(FString::Printf(TEXT("room %s state %s mode %s map %s seat %d host %s spectator %s players %d confirmed %s"), *S.RoomCode, *S.RoomState, *S.Mode, *S.LobbyMap, S.LobbySeat,
				*B(S.bAmHost), *B(S.bSpectator), S.LobbyPlayers, *B(S.SeatConfirmed())));
		}
		{
			TArray<FString>& L = Add(TEXT("match"));
			TArray<FString> Teams, Factions, Spawns;
			for (int32 T : S.Teams) Teams.Add(FString::FromInt(T));
			for (int32 F : S.Factions) Factions.Add(FString::FromInt(F));
			for (const TOptional<FVector2D>& Sp : S.Spawns) Spawns.Add(Sp.IsSet() ? FString::Printf(TEXT("%s,%s"), *N(Sp->X), *N(Sp->Y)) : FString(TEXT("null")));
			L.Add(FString::Printf(TEXT("match %d you %d tick %d winner %d mp %d sandbox %s snapshots %d paused %s"), S.MatchId, S.You, S.Tick, S.Winner, S.Mp, *B(S.bSandbox), S.Snapshots, *B(S.bPaused)));
			L.Add(FString::Printf(TEXT("names [%s] teams [%s] factions [%s] spawn %s,%s spawns [%s]"), *FString::Join(S.Names, TEXT(",")), *FString::Join(Teams, TEXT(",")), *FString::Join(Factions, TEXT(",")),
				*N(S.Spawn.X), *N(S.Spawn.Y), *FString::Join(Spawns, TEXT(" "))));
		}
		{
			TArray<FString>& L = Add(TEXT("units"));
			TArray<int32> Ids;
			S.Units.GenerateKeyArray(Ids);
			Ids.Sort();
			for (int32 Id : Ids) L.Add(UnitLine(S.Units[Id]));
			TArray<int32> Bad = S.UnreadableIds.Array();
			Bad.Sort();
			for (int32 Id : Bad) L.Add(FString::Printf(TEXT("unreadable %d"), Id));
			L.Add(FString::Printf(TEXT("from rust view %s"), *B(S.UnitsFromRustView())));
		}
		{
			TArray<FString>& L = Add(TEXT("map"));
			L.Add(FString::Printf(TEXT("%s %dx%d world %s chars %s levels %s known %s (%d)"), *S.Map.Name, S.Map.W, S.Map.H, *B(S.Map.bWorld), *Hash(S.Map.Chars), *Hash(S.Map.Levels), *HashBits(S.Map.Known), S.Map.Known.CountSetBits()));
			L.Add(FString::Printf(TEXT("fog lifted %s visible %s explored %s"), *B(S.bFogLifted), *Hash(S.FogVisible), *Hash(S.FogExplored)));
		}
		{
			TArray<FString>& L = Add(TEXT("cells.last"));
			L.Add(FString::Printf(TEXT("rows %d skipped %d"), S.LastCellRows.Num(), S.LastCellRowsSkipped));
			for (int32 C : S.LastCellRows) L.Add(CellLine(S, C));
		}
		{
			TArray<FString>& L = Add(TEXT("cells.all"));
			TSet<int32> All;
			for (const auto& Pair : S.CellStates) All.Add(Pair.Key);
			for (const auto& Pair : S.Sections) All.Add(Pair.Key);
			TArray<int32> Cells = All.Array();
			Cells.Sort();
			for (int32 C : Cells) L.Add(CellLine(S, C));
		}
		{
			TArray<FString>& L = Add(TEXT("points"));
			for (const FTCPointRow& R : S.Points) L.Add(FString::Printf(TEXT("owner %d capper %d progress %s contested %s cut %s locked %s"), R.Owner, R.Capper, *N(R.Progress), *B(R.bContested), *B(R.bCut), *B(R.bLocked)));
			TArray<FString> Vp, On, Ping, Out;
			for (int32 V : S.Vp) Vp.Add(FString::FromInt(V));
			for (const TOptional<bool>& O : S.Online) On.Add(!O.IsSet() ? TEXT("?") : *B(O.GetValue()));
			for (const TOptional<int32>& Pg : S.Ping) Ping.Add(Opt(Pg));
			for (bool O : S.OutSeats) Out.Add(B(O));
			L.Add(FString::Printf(TEXT("vp [%s] online [%s] ping [%s] out [%s]"), *FString::Join(Vp, TEXT(",")), *FString::Join(On, TEXT(",")), *FString::Join(Ping, TEXT(",")), *FString::Join(Out, TEXT(","))));
		}
		{
			TArray<FString>& L = Add(TEXT("plans"));
			for (const FTCPlan& Pl : S.Plans)
			{
				TArray<FString> Path;
				for (const FVector2D& V : Pl.Path) Path.Add(FString::Printf(TEXT("%s,%s"), *N(V.X), *N(V.Y)));
				L.Add(FString::Printf(TEXT("%d kind %d to %s,%s path %s"), Pl.Id, Pl.Kind, *N(Pl.Tx), *N(Pl.Tz), *FString::Join(Path, TEXT(" "))));
			}
			for (const FTCWaitingOrders& W : S.WaitingOrders)
			{
				TArray<FString> O;
				for (const FTCQueuedOrder& Q : W.Orders) O.Add(FString::Printf(TEXT("%d@%s,%s"), Q.Kind, *N(Q.X), *N(Q.Z)));
				L.Add(FString::Printf(TEXT("orders %d: %s"), W.Id, *FString::Join(O, TEXT(" "))));
			}
			for (const FTCGhost& G : S.Ghosts) L.Add(FString::Printf(TEXT("ghost %d %s owner %d at %s,%s built %s"), G.Id, *G.Type, G.Owner, *N(G.X), *N(G.Z), *N(G.Built)));
			for (const FTCAirRow& R : S.Air) L.Add(FString::Printf(TEXT("air %d state %d fuel %d ammo %d timer %d"), R.Id, R.State, R.Fuel, R.Ammo, R.Timer));
		}
		{
			TArray<FString>& L = Add(TEXT("shots"));
			for (const FTCShot& Sh : S.Shots) L.Add(ShotLine(Sh));
		}
		{
			TArray<FString>& L = Add(TEXT("flights"));
			for (const FTCFlight& F : S.Flights) L.Add(FString::Printf(TEXT("%d seq %d %s %s at %s,%s,%s v %s,%s,%s g %s time %s launched %s expires %s until %s shooter %s"), F.Flight, F.Seq, *F.Kind, *F.Effect,
				*N(F.X), *N(F.Y), *N(F.Z), *N(F.V.X), *N(F.V.Y), *N(F.V.Z), *N(F.Gravity), *N(F.Time), *N(F.Launched), *N(F.Expires), *N(F.Until), F.bHasShooter ? *FString::Printf(TEXT("%d/%d"), F.F, F.Fo) : TEXT("-")));
			TArray<FString> Fires, Mines;
			for (int32 C : S.Fires) Fires.Add(FString::FromInt(C));
			for (int32 C : S.FoundMines) Mines.Add(FString::FromInt(C));
			L.Add(FString::Printf(TEXT("fires [%s] foundMines [%s]"), *FString::Join(Fires, TEXT(",")), *FString::Join(Mines, TEXT(","))));
		}
		{
			TArray<FString>& L = Add(TEXT("falling"));
			for (const FTCFalling& F : S.Falling) L.Add(FallingLine(F));
		}
		{
			TArray<FString>& L = Add(TEXT("weather"));
			L.Add(FString::Printf(TEXT("now %s next %s in %d from snapshot %s"), *S.Weather.Now, *S.Weather.Next, S.Weather.SecondsLeft, *B(S.bWeatherFromSnapshot)));
		}
		{
			TArray<FString>& L = Add(TEXT("wx"));
			L.Add(S.Wx.bKnown ? FString::Printf(TEXT("[%s, %s, %s, %s]"), *N(S.Wx.Rain), *N(S.Wx.Wet), *N(S.Wx.WindAngle), *N(S.Wx.WindStrength)) : FString(TEXT("-")));
		}
		{
			TArray<FString>& L = Add(TEXT("economy"));
			L.Add(FString::Printf(TEXT("inc %s fuelInc %s upkeep %s mun %s fuel %s army income %s"), *Opt(S.Inc), *Opt(S.FuelInc), *Opt(S.Upkeep), *Opt(S.Mun), *Opt(S.Fuel), *N(S.ArmyIncome)));
		}
		{
			TArray<FString>& L = Add(TEXT("mode"));
			const FTCModeRow& M = S.ModeRow;
			TArray<FString> Bunkers;
			for (int32 V : M.Bunkers) Bunkers.Add(FString::FromInt(V));
			L.Add(FString::Printf(TEXT("present %s kind %s defender %s attacker %s timeLeft %s total %s wave %s left %s slot %s step %s steps %s sudden %s active %s bunkers [%s] profile %s next %s goal %s boss %s %d/%d"),
				*B(M.bPresent), *M.Kind, *Opt(M.DefenderTeam), *Opt(M.AttackerTeam), *Opt(M.TimeLeft), *Opt(M.Total), *Opt(M.Wave), *Opt(M.Left), *Opt(M.Slot), *Opt(M.Step), *Opt(M.Steps),
				*B(M.bSuddenDeath), *B(M.bActive), *FString::Join(Bunkers, TEXT(",")), *M.Profile, *M.NextProfile, *M.Goal, *B(M.bBoss), M.BossHp, M.BossMax));
			L.Add(FString::Printf(TEXT("world present %s owned %s goal %s total %s regions %d"), *B(S.WorldScore.bPresent), *Opt(S.WorldScore.Owned), *Opt(S.WorldScore.Goal), *Opt(S.WorldScore.Total), S.WorldScore.RegionTeams.Num()));
			L.Add(FString::Printf(TEXT("end present %s reason %s at %s,%s"), *B(S.End.bPresent), *S.End.Reason, *N(S.End.X), *N(S.End.Z)));
			L.Add(FString::Printf(TEXT("scenario %s"), *B(S.bHasScenario)));
			for (const FTCScenarioObjective& O : S.Scenario) L.Add(FString::Printf(TEXT("objective %s %s %s"), *O.Id, *O.State, *O.Text));
			L.Add(FString::Printf(TEXT("dropped over cap %d, malformed skipped %d"), S.DroppedOverCap, S.SkippedMalformed));
		}
		{
			TArray<FString>& L = Add(TEXT("alerts"));
			const TArray<FTCAlertHistoryEntry>& H = A.History().Entries();
			for (int32 I = H.Num() - 1; I >= 0; I--)
				L.Add(FString::Printf(TEXT("history %d %s \"%s\" at %s time %s"), H[I].Id, *H[I].Kind, *H[I].Text, H[I].bHasAt ? *FString::Printf(TEXT("%s,%s"), *N(H[I].At.X), *N(H[I].At.Y)) : TEXT("-"), *N(H[I].Time)));
			for (const FTCAlertLine& Ln : A.Lines()) L.Add(FString::Printf(TEXT("line %s \"%s\""), *Ln.Alert.Kind, *Ln.Alert.Text));
		}
		{
			TArray<FString>& L = Add(TEXT("effects"));
			const FTCCombatEffects::FStats& C = Fx.Stats();
			L.Add(FString::Printf(TEXT("launches %d hidden %d contacts %d direct %d blasts %d wrecks %d throws %d salvos %d air %d collapses %d dropped %d"), C.Launches, C.LaunchesHidden, C.Contacts, C.DirectFire,
				C.Blasts, C.Wrecks, C.Throws, C.Salvos, C.AirShots, C.Collapses, C.Dropped));
			L.Add(FString::Printf(TEXT("particles %d debris bodies %d section poses %d"), Fx.Particles().Num(), Fx.Debris.Count(), Fx.Sections().Num()));
			TArray<FString> Ids;
			Fx.Sections().GenerateKeyArray(Ids);
			Ids.Sort();
			for (const FString& Id : Ids)
			{
				const FTCDebrisRow& R = Fx.Sections()[Id];
				L.Add(FString::Printf(TEXT("section pose %s kind %s material %s at %s,%s,%s tilt %s dir %s"), *Id, *R.Kind, *R.Material, *N(R.X), *N(R.Y), *N(R.Z), *N(R.Tilt), *N(R.Dir)));
			}
		}
		return P;
	}

	FString JoinParts(const TArray<FPart>& Parts)
	{
		FString Out;
		for (const FPart& P : Parts) { Out += FString::Printf(TEXT("[%s] %d\n"), *P.Name, P.Lines.Num()); for (const FString& L : P.Lines) Out += L + TEXT("\n"); }
		return Out;
	}

	struct FStreamResult
	{
		FString Stem;
		bool bRead = false;
		int64 Bytes = 0;
		int32 Payloads = 0, ParseErrors = 0, NoType = 0;
		TMap<FString, int32> Types;
		int32 Starts = 0, StartsRefused = 0, SnapshotsApplied = 0, SnapshotsIgnored = 0, Resyncs = 0, FirstTick = -1, LastTick = -1, TickGaps = 0;
		int32 CellRowsSkipped = 0;
		FTCRustViewShadow::FCounts Rust;
		bool bRustRan = false, bRustAuthoritative = false;
		int32 RustMessages = 0, RustApplyErrors = 0;
		FString RustFirstError, RustMode;
		// the tick-8400 packet
		bool bHas8400 = false;
		TArray<FString> Report;           // the human lines
		TArray<FString> SayEn, SayEs, SayDrawnEn, SayDrawnEs;
		int32 Collapse = 0, SayIndex = -1, FirstCollapseIndex = -1, CellRows = 0, BridgeDown = 0, BridgeSections = 0, Falling = 0, FxPoses = 0, FxContacts = 0, FxParticles = 0;
		bool bCell5812 = false, bCell5946 = false;
		TArray<FString> CollapseSections, FallingIds, Sounds;
		int32 BridgeKnownBefore = -1, BridgeTick = -1;        // bridge sections the client knew at the snapshot before
		TMap<FString, int32> CharMoves;                       // the 8400 rows' cell characters, "before -> after"
		TArray<FString> History;                              // the feed's history at 8400: "time kind text | es"
		TArray<FPart> State;              // the dump at tick 8400
	};

	FStreamResult RunStream(const FString& Dir, const FString& Stem, const FTCUnitTable& Table)
	{
		FStreamResult R;
		R.Stem = Stem;
		const FString Path = Dir / TEXT("streams") / (Stem + TEXT(".jsonl"));
		TArray<FString> Lines;
		R.bRead = FFileHelper::LoadFileToStringArray(Lines, *Path);
		R.Bytes = IFileManager::Get().FileSize(*Path);
		if (!R.bRead) return R;

		FTCMatchState State;
		FTCRustViewShadow Rust; // declared after the state: it detaches first
		R.bRustRan = FWW2Core::IsReady() && Rust.BeginMode(ETCDecoder::Rust, &State, true, false);
		FTCAlerts AlertsEn, AlertsEs;
		FTCCombatEffects Fx;
		FTCFxContext FxCtx;
		FxCtx.Table = &Table;
		const FTCMeasure Measure = FTCPaint::ApproxMeasure();

		for (int32 L = 0; L < Lines.Num(); L++)
		{
			const FString& Text = Lines[L];
			if (Text.IsEmpty() && L == Lines.Num() - 1) continue; // the file's last newline
			R.Payloads++;
			// FTCConnection::OnMessage: OnText (the Rust view) first, unchanged, then the parse and OnMessage
			if (R.bRustRan && Rust.IsActive()) Rust.Feed(Text);
			const TSharedPtr<FJsonObject> M = Parse(Text);
			if (!M) { R.ParseErrors++; continue; }
			FString T;
			if (!M->TryGetStringField(TEXT("t"), T)) { R.NoType++; continue; }
			R.Types.FindOrAdd(T)++;
			if (T == TEXT("lobby")) State.ApplyLobby(*M);
			else if (T == TEXT("pause")) State.ApplyPause(*M);
			else if (T == TEXT("deny")) State.ApplyDeny(*M);
			else if (T == TEXT("start"))
			{
				const bool bOk = State.ApplyStart(*M);
				R.Starts++;
				if (!bOk) { R.StartsRefused++; if (R.bRustRan) Rust.AfterStart(State, false, false, R.Starts); continue; }
				if (R.bRustRan) Rust.AfterStart(State, true, false, R.Starts);
				R.CellRowsSkipped += State.LastCellRowsSkipped;
				// ATCGameMode::OnCombatStart: the alert feed's match, then the effects
				AlertsEn.StartMatch(State); AlertsEs.StartMatch(State);
				Fx.Start(State);
			}
			else if (T == TEXT("s"))
			{
				const int32 Before = State.Snapshots;
				const TArray<uint8> CharsBefore = State.Map.Chars;
				int32 BridgeBefore = 0;
				for (const auto& Pair : State.Sections) if (Pair.Value.StructureId.StartsWith(TEXT("map:bridge:"))) BridgeBefore++;
				const int32 TickBefore = State.Tick;
				const bool bDrift = State.ApplySnapshot(*M);
				if (R.bRustRan) Rust.AfterSnapshot(State, bDrift);
				if (State.Snapshots == Before) { R.SnapshotsIgnored++; continue; }
				R.SnapshotsApplied++;
				if (bDrift) R.Resyncs++;
				R.CellRowsSkipped += State.LastCellRowsSkipped;
				if (R.LastTick >= 0 && State.Tick != R.LastTick + 2) R.TickGaps++;
				if (R.FirstTick < 0) R.FirstTick = State.Tick;
				R.LastTick = State.Tick;
				// ATCGameMode::OnCombatSnapshot: alerts.js snapshot, then fx.js, projectiles.js and debris.js
				const double Clock = State.Tick / 20.0;
				FTCAlertContext Ctx;
				Ctx.Now = Clock;
				Ctx.OnScreen = [](double, double) { return false; };
				Ctx.Table = &Table;
				TArray<FTCAlertPush> En, Es;
				{
					FTCLocale::FScopedLanguage Lang(ETCLanguage::English);
					AlertsEn.Frame(Clock);
					En = AlertsEn.Snapshot(State, M.Get(), Ctx);
					AlertsEn.TakeSounds();
				}
				{
					FTCLocale::FScopedLanguage Lang(ETCLanguage::Spanish);
					AlertsEs.Frame(Clock);
					Es = AlertsEs.Snapshot(State, M.Get(), Ctx);
					AlertsEs.TakeSounds();
				}
				const int32 ContactsBefore = Fx.Stats().Collapses;
				Fx.Snapshot(State, M.Get(), FxCtx, Clock);
				if (State.Tick == CollapseTick)
				{
					R.bHas8400 = true;
					for (const FTCAlertPush& A : En) if (A.Kind == TEXT("event")) R.SayEn.Add(A.Text);
					for (const FTCAlertPush& A : Es) if (A.Kind == TEXT("event")) R.SayEs.Add(A.Text);
					// how the feed draws its newest line (alerts.css, the approximate measure), in each language
					FTCAlertViewInput In;
					In.Now = Clock + 0.5; In.Right = 1272.f; In.MinimapTop = 528.f;
					{ FTCLocale::FScopedLanguage Lang(ETCLanguage::English); R.SayDrawnEn = AlertsEn.Layout(In, Measure).TextsOf(TEXT("alert:text-newest")); }
					{ FTCLocale::FScopedLanguage Lang(ETCLanguage::Spanish); R.SayDrawnEs = AlertsEs.Layout(In, Measure).TextsOf(TEXT("alert:text-newest")); }
					for (int32 I = 0; I < State.Shots.Num(); I++)
					{
						const FTCShot& Sh = State.Shots[I];
						if (Sh.K == TEXT("say") && R.SayIndex < 0) R.SayIndex = I;
						if (Sh.K != TEXT("collapse")) continue;
						if (R.FirstCollapseIndex < 0) R.FirstCollapseIndex = I;
						R.Collapse++;
						R.CollapseSections.Add(Sh.Id);
					}
					R.CellRows = State.LastCellRows.Num();
					R.BridgeKnownBefore = BridgeBefore; R.BridgeTick = TickBefore;
					for (int32 C : State.LastCellRows) R.CharMoves.FindOrAdd(FString::Printf(TEXT("'%c' -> '%c'"), (TCHAR)CharsBefore[C], (TCHAR)State.Map.Chars[C]))++;
					const TArray<FTCAlertHistoryEntry>& H = AlertsEn.History().Entries();
					for (int32 I = H.Num() - 1; I >= 0; I--)
						R.History.Add(FString::Printf(TEXT("%s s %s \"%s\" | es \"%s\""), *N(H[I].Time), *H[I].Kind, *H[I].Text, *FTCLocale::Translate(H[I].Text, ETCLanguage::Spanish)));
					R.bCell5812 = State.LastCellRows.Contains(5812);
					R.bCell5946 = State.LastCellRows.Contains(5946);
					for (const auto& Pair : State.Sections)
					{
						if (!Pair.Value.StructureId.StartsWith(TEXT("map:bridge:"))) continue;
						R.BridgeSections++;
						if (Pair.Value.IsDown()) R.BridgeDown++;
					}
					R.Falling = State.Falling.Num();
					for (const FTCFalling& F : State.Falling) R.FallingIds.Add(F.Id);
					R.FxPoses = Fx.Sections().Num();
					R.FxContacts = Fx.Stats().Collapses - ContactsBefore;
					R.FxParticles = Fx.Particles().Num();
					R.Sounds = Fx.TakeSounds();
					R.State = Dump(State, AlertsEn, Fx);
				}
				else Fx.TakeSounds();
				// two frames between snapshots (the snapshot interval is 0.1 s)
				Fx.Frame(FxCtx, Clock + 0.05, 0.05f);
				Fx.Frame(FxCtx, Clock + 0.1, 0.05f);
				Fx.TakeSounds();
			}
		}
		if (R.bRustRan)
		{
			R.Rust = Rust.Totals();
			R.RustMessages = Rust.MessageCount();
			R.RustApplyErrors = Rust.ApplyErrorCount();
			R.RustFirstError = Rust.FirstApplyError();
			R.bRustAuthoritative = Rust.IsAuthoritative();
			R.RustMode = FTCRustViewShadow::DecoderName(Rust.Mode());
			Rust.End();
		}
		else R.RustMode = FString(TEXT("not run: ")) + FWW2Core::Describe();
		return R;
	}

	FString Summary(const FStreamResult& R)
	{
		TArray<FString> Types;
		TArray<FString> Keys;
		R.Types.GenerateKeyArray(Keys);
		Keys.Sort();
		for (const FString& K : Keys) Types.Add(FString::Printf(TEXT("%s %d"), *K, R.Types[K]));
		TArray<FString> L;
		L.Add(FString::Printf(TEXT("stream %s: %lld bytes, %d payloads read (%s), parse errors %d, no type %d"), *R.Stem, (long long)R.Bytes, R.Payloads, *FString::Join(Types, TEXT(", ")), R.ParseErrors, R.NoType));
		L.Add(FString::Printf(TEXT("starts %d (refused %d); snapshots applied %d, ignored (no match) %d; first tick %d, last tick %d, tick gaps %d; resync verdicts %d; cell rows skipped %d"),
			R.Starts, R.StartsRefused, R.SnapshotsApplied, R.SnapshotsIgnored, R.FirstTick, R.LastTick, R.TickGaps, R.Resyncs, R.CellRowsSkipped));
		L.Add(FString::Printf(TEXT("rust view (%s, authoritative at the end %s): messages fed %d, apply errors %d%s; snapshots %d, adopted %d (rows %lld), refused %d, not fed %d, agree %d, disagree %d, view resync %d, unrepresentable %lld"),
			*R.RustMode, *B(R.bRustAuthoritative), R.RustMessages, R.RustApplyErrors, R.RustFirstError.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (first: %s)"), *R.RustFirstError),
			R.Rust.Snapshots, R.Rust.Adopted, (long long)R.Rust.RowsAdopted, R.Rust.Refused, R.Rust.NotFed, R.Rust.Agree, R.Rust.Disagree, R.Rust.RustResync, (long long)R.Rust.Unrepresentable));
		if (R.bHas8400)
		{
			L.Add(FString::Printf(TEXT("tick %d: say lines in the feed (en) [%s]"), CollapseTick, *FString::Join(R.SayEn, TEXT(" | "))));
			L.Add(FString::Printf(TEXT("tick %d: say lines in the feed (es) [%s]; drawn through TC_T (es): [%s]; drawn (en): [%s]"), CollapseTick, *FString::Join(R.SayEs, TEXT(" | ")),
				*FString::Join(R.SayDrawnEs, TEXT(" | ")), *FString::Join(R.SayDrawnEn, TEXT(" | "))));
			L.Add(FString::Printf(TEXT("tick %d: collapse rows %d (say at shot %d, first collapse at shot %d); changed-cell rows applied %d (5812 %s, 5946 %s); bridge sections known %d, at zero hp or failed %d"),
				CollapseTick, R.Collapse, R.SayIndex, R.FirstCollapseIndex, R.CellRows, *B(R.bCell5812), *B(R.bCell5946), R.BridgeSections, R.BridgeDown));
			TMap<FString, int32> SoundCount;
			for (const FString& Snd : R.Sounds) SoundCount.FindOrAdd(Snd)++;
			TArray<FString> Sounds, Moves;
			for (const auto& Pair : SoundCount) Sounds.Add(FString::Printf(TEXT("%s x%d"), *Pair.Key, Pair.Value));
			for (const auto& Pair : R.CharMoves) Moves.Add(FString::Printf(TEXT("%s x%d"), *Pair.Key, Pair.Value));
			L.Add(FString::Printf(TEXT("tick %d: falling rows %d; effects: section poses started %d, collapse contacts shown %d, particles %d, sounds [%s]"),
				CollapseTick, R.Falling, R.FxPoses, R.FxContacts, R.FxParticles, *FString::Join(Sounds, TEXT(", "))));
			L.Add(FString::Printf(TEXT("tick %d: bridge sections the client knew at tick %d (before this packet) %d; the 86 rows' cell characters [%s]"),
				CollapseTick, R.BridgeTick, R.BridgeKnownBefore, *FString::Join(Moves, TEXT(", "))));
			for (const FString& H : R.History) L.Add(FString::Printf(TEXT("tick %d: alert history %s"), CollapseTick, *H));
		}
		else L.Add(FString::Printf(TEXT("tick %d: NOT reached"), CollapseTick));
		return FString::Join(L, TEXT("\n"));
	}

	// Native against source for one seat, part by part: the names of the parts that differ, with their first differing line.
	TArray<FString> Compare(const FStreamResult& A, const FStreamResult& B2, int32& OutEqualParts)
	{
		TArray<FString> Out;
		OutEqualParts = 0;
		for (const FPart& P : A.State)
		{
			const FPart* Q = B2.State.FindByPredicate([&P](const FPart& X) { return X.Name == P.Name; });
			if (!Q) { Out.Add(FString::Printf(TEXT("%s: only in %s"), *P.Name, *A.Stem)); continue; }
			if (P.Lines == Q->Lines) { OutEqualParts++; continue; }
			int32 I = 0;
			while (I < P.Lines.Num() && I < Q->Lines.Num() && P.Lines[I] == Q->Lines[I]) I++;
			int32 Differ = 0;
			for (int32 K = 0; K < FMath::Max(P.Lines.Num(), Q->Lines.Num()); K++) if (!P.Lines.IsValidIndex(K) || !Q->Lines.IsValidIndex(K) || P.Lines[K] != Q->Lines[K]) Differ++;
			Out.Add(FString::Printf(TEXT("%s: %d of %d/%d lines differ; first at line %d:\n    %s: %s\n    %s: %s"), *P.Name, Differ, P.Lines.Num(), Q->Lines.Num(), I + 1,
				*A.Stem, P.Lines.IsValidIndex(I) ? *P.Lines[I] : TEXT("(none)"), *B2.Stem, Q->Lines.IsValidIndex(I) ? *Q->Lines[I] : TEXT("(none)")));
		}
		return Out;
	}

	const FPart* PartOf(const FStreamResult& R, const TCHAR* Name) { return R.State.FindByPredicate([Name](const FPart& P) { return P.Name == Name; }); }
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLegacyBridgeReplayTest, "ThreeCrossroads.Net.LegacyBridgeReplay", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCLegacyBridgeReplayTest::RunTest(const FString& Parameters)
{
	using namespace TCBridgeReplay;

	// The decoder reads 0 and 0.0, 2400 and 2400.0 as the same number (the README's spelling differences).
	{
		const TSharedPtr<FJsonObject> A = Parse(TEXT("{\"t\":\"s\",\"v\":[0,2400,3.7]}")), B2 = Parse(TEXT("{\"t\":\"s\",\"v\":[0.0,2400.0,3.70]}"));
		bool bSame = A && B2;
		if (bSame)
		{
			const TArray<TSharedPtr<FJsonValue>>& X = A->GetArrayField(TEXT("v"));
			const TArray<TSharedPtr<FJsonValue>>& Y = B2->GetArrayField(TEXT("v"));
			for (int32 I = 0; I < 3; I++) bSame &= X[I]->Type == EJson::Number && Y[I]->Type == EJson::Number && X[I]->AsNumber() == Y[I]->AsNumber();
		}
		TestTrue(TEXT("0 and 0.0, 2400 and 2400.0 parse to the same JSON number"), bSame);
	}

	FString How;
	const FString Dir = BundleDir(How);
	if (Dir.IsEmpty() || !IFileManager::Get().DirectoryExists(*(Dir / TEXT("streams"))))
	{
		AddWarning(FString::Printf(TEXT("Great Bridge bundle not configured (%s); the replay was NOT run. Set TC_BRIDGE_REPLAY_DIR to the extracted great-bridge-8400 folder (unreal/Tools/bridge-replay/run.sh)."),
			Dir.IsEmpty() ? TEXT("no TC_BRIDGE_REPLAY_DIR, -TCBridgeReplay= or Saved/BridgeReplay/path.txt") : *FString::Printf(TEXT("%s has no streams/"), *Dir)));
		return true;
	}
	AddInfo(FString::Printf(TEXT("bundle %s (from %s)"), *Dir, *How));

	// The manifest's byte counts (run.sh checks the SHA-256 hashes before the run)
	TMap<FString, int64> ManifestBytes;
	{
		FString Text;
		const TSharedPtr<FJsonObject> Man = FFileHelper::LoadFileToString(Text, *(Dir / TEXT("manifest.json"))) ? Parse(Text) : nullptr;
		const TArray<TSharedPtr<FJsonValue>>* Assets = nullptr;
		if (TestTrue(TEXT("manifest.json reads"), Man.IsValid() && Man->TryGetArrayField(TEXT("assets"), Assets)))
			for (const TSharedPtr<FJsonValue>& V : *Assets)
			{
				const TSharedPtr<FJsonObject>* O = nullptr;
				FString P; double Bytes = 0;
				if (V->TryGetObject(O) && (*O)->TryGetStringField(TEXT("path"), P) && (*O)->TryGetNumberField(TEXT("bytes"), Bytes)) ManifestBytes.Add(P, (int64)Bytes);
			}
	}

	FTCUnitTable Table;
	FString Err;
	if (!TestTrue(TEXT("units.json loads"), Table.Load(Err))) { AddError(Err); return false; }
	TestTrue(TEXT("WW2Core (the Rust view) is linked in this build"), FWW2Core::IsReady());

	const FString Out = OutDir();
	IFileManager::Get().MakeDirectory(*Out, true);
	TMap<FString, FStreamResult> Results;
	TArray<FString> AllSummary;
	AllSummary.Add(FString::Printf(TEXT("Great Bridge tick-8400 Unreal decoder replay; bundle %s; WW2Core: %s"), *Dir, *FWW2Core::Describe()));
	for (const TCHAR* Stem : Streams)
	{
		FStreamResult R = RunStream(Dir, Stem, Table);
		const FString S = FString(Stem);
		if (!TestTrue(FString::Printf(TEXT("%s: stream reads"), Stem), R.bRead)) continue;
		if (const int64* Want = ManifestBytes.Find(FString::Printf(TEXT("streams/%s.jsonl"), Stem))) TestEqual(FString::Printf(TEXT("%s: bytes as the manifest"), Stem), R.Bytes, *Want);
		else AddError(FString::Printf(TEXT("%s: not in the manifest"), Stem));
		const FString Sum = Summary(R);
		AddInfo(Sum);
		AllSummary.Add(Sum);
		FFileHelper::SaveStringToFile(Sum + TEXT("\n"), *(Out / (S + TEXT(".summary.txt"))), FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM);
		FFileHelper::SaveStringToFile(JoinParts(R.State), *(Out / (S + TEXT(".state-8400.txt"))), FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM);

		// What the bundle's README states, checked on this client's applied state
		TestEqual(FString::Printf(TEXT("%s: parse errors"), Stem), R.ParseErrors + R.NoType, 0);
		TestEqual(FString::Printf(TEXT("%s: one start, taken"), Stem), R.Starts - R.StartsRefused, 1);
		TestEqual(FString::Printf(TEXT("%s: snapshots applied"), Stem), R.SnapshotsApplied, 4200);
		TestEqual(FString::Printf(TEXT("%s: snapshots ignored"), Stem), R.SnapshotsIgnored, 0);
		TestEqual(FString::Printf(TEXT("%s: last tick"), Stem), R.LastTick, CollapseTick);
		TestEqual(FString::Printf(TEXT("%s: tick gaps"), Stem), R.TickGaps, 0);
		TestEqual(FString::Printf(TEXT("%s: resync verdicts"), Stem), R.Resyncs, 0);
		TestEqual(FString::Printf(TEXT("%s: unreadable cell rows"), Stem), R.CellRowsSkipped, 0);
		if (R.bRustRan)
		{
			TestEqual(FString::Printf(TEXT("%s: rust apply errors"), Stem), R.RustApplyErrors, 0);
			TestEqual(FString::Printf(TEXT("%s: rust fed every payload"), Stem), R.RustMessages, R.Payloads);
			TestEqual(FString::Printf(TEXT("%s: rust adopted every snapshot"), Stem), R.Rust.Adopted, R.SnapshotsApplied);
			TestEqual(FString::Printf(TEXT("%s: rust refused or not fed"), Stem), R.Rust.Refused + R.Rust.NotFed, 0);
			TestEqual(FString::Printf(TEXT("%s: rust disagreements with the C++ decoder"), Stem), R.Rust.Disagree, 0);
			TestEqual(FString::Printf(TEXT("%s: rust resync verdicts"), Stem), R.Rust.RustResync, 0);
			TestTrue(FString::Printf(TEXT("%s: still authoritative"), Stem), R.bRustAuthoritative);
		}
		if (!TestTrue(FString::Printf(TEXT("%s: tick %d applied"), Stem, CollapseTick), R.bHas8400)) continue;
		TestEqual(FString::Printf(TEXT("%s: one say line in the feed"), Stem), R.SayEn.Num(), 1);
		if (R.SayEn.Num() == 1) TestEqualSensitive(FString::Printf(TEXT("%s: the say line (en)"), Stem), R.SayEn[0], FString(SayEnglish));
		// Spanish: the packet's localized has only en, so the feed takes sh.text and draws it through TC_T (es.js)
		if (R.SayEs.Num() == 1)
		{
			TestEqualSensitive(FString::Printf(TEXT("%s: the say line pushed in Spanish (sh.text)"), Stem), R.SayEs[0], FString(SayEnglish));
			TestEqualSensitive(FString::Printf(TEXT("%s: TC_T gives es.js's line"), Stem), FTCLocale::Translate(R.SayEs[0], ETCLanguage::Spanish), FString(SaySpanish));
		}
		else AddError(FString::Printf(TEXT("%s: %d say lines in the Spanish feed"), Stem, R.SayEs.Num()));
		TestEqual(FString::Printf(TEXT("%s: collapse rows"), Stem), R.Collapse, 84);
		TestTrue(FString::Printf(TEXT("%s: say before the collapse rows"), Stem), R.SayIndex >= 0 && R.SayIndex < R.FirstCollapseIndex);
		TestEqual(FString::Printf(TEXT("%s: changed-cell rows applied"), Stem), R.CellRows, 86);
		TestTrue(FString::Printf(TEXT("%s: cells 5812 and 5946 applied"), Stem), R.bCell5812 && R.bCell5946);
		TestEqual(FString::Printf(TEXT("%s: bridge sections at zero hp or failed"), Stem), R.BridgeDown, 84);
		TestEqual(FString::Printf(TEXT("%s: falling rows"), Stem), R.Falling, 84);
		TestEqual(FString::Printf(TEXT("%s: no bridge section known before tick %d"), Stem, CollapseTick), R.BridgeKnownBefore, 0);
		TestEqual(FString::Printf(TEXT("%s: falling sections the effects started"), Stem), R.FxPoses, 84);
		TestEqual(FString::Printf(TEXT("%s: collapse contacts the effects showed"), Stem), R.FxContacts, 84);
		Results.Add(S, MoveTemp(R));
	}

	// Native against source, per seat, on the applied state at tick 8400
	for (int32 Seat = 0; Seat < 2; Seat++)
	{
		const FStreamResult* Na = Results.Find(FString::Printf(TEXT("native-slot%d"), Seat));
		const FStreamResult* So = Results.Find(FString::Printf(TEXT("source-slot%d"), Seat));
		if (!Na || !So || !Na->bHas8400 || !So->bHas8400) { AddError(FString::Printf(TEXT("seat %d: no comparison (a stream did not reach tick %d)"), Seat, CollapseTick)); continue; }
		int32 Equal = 0;
		const TArray<FString> Diff = Compare(*Na, *So, Equal);
		const FString Text = FString::Printf(TEXT("seat %d, native against source at tick %d: %d of %d parts equal; %s"), Seat, CollapseTick, Equal, Na->State.Num(),
			Diff.Num() ? *(TEXT("differ:\n  ") + FString::Join(Diff, TEXT("\n  "))) : TEXT("no difference"));
		AddInfo(Text);
		AllSummary.Add(Text);
		FFileHelper::SaveStringToFile(Text + TEXT("\n"), *(Out / FString::Printf(TEXT("compare-slot%d.txt"), Seat)), FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM);
		// the bridge's own parts must agree: the cells applied at 8400 and every cell known, the collapse and say shots,
		// the falling rows, the alert feed and the effects
		for (const TCHAR* Part : { TEXT("cells.last"), TEXT("cells.all"), TEXT("shots"), TEXT("falling"), TEXT("alerts"), TEXT("effects") })
		{
			const FPart* P = PartOf(*Na, Part);
			const FPart* Q = PartOf(*So, Part);
			TestTrue(FString::Printf(TEXT("seat %d: %s equal (native, source)"), Seat, Part), P && Q && P->Lines == Q->Lines);
		}
	}
	FFileHelper::SaveStringToFile(FString::Join(AllSummary, TEXT("\n")) + TEXT("\n"), *(Out / TEXT("summary.txt")), FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM);
	AddInfo(FString::Printf(TEXT("results in %s"), *Out));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMatchStateCellRowsTest, "ThreeCrossroads.Net.CellRows", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

// FTCMatchState's cell rows beyond the character and level, as client/main.js applyCells keeps them: the state byte
// (st ?? 0, a Uint8Array store), ground, object and material, and the structural section (kept until a later row
// brings one; a start empties them), with 0 / 0.0 and 2400 / 2400.0 read alike.
bool FTCMatchStateCellRowsTest::RunTest(const FString& Parameters)
{
	using namespace TCBridgeReplay;
	FTCMatchState S;
	S.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","state":"play","you":0,"spectator":false,"players":[{"name":"A"}]})")));
	const TCHAR* Start = TEXT(R"({"t":"start","matchId":1,"you":0,"names":["A"],"teams":[0],"map":{"name":"t","w":3,"h":1,"rows":["=.."]},"cells":[[0,"=",0,8,{"ground":"W","object":"=","material":"stone","section":{"id":"map:bridge:0:0","structureId":"map:bridge:0","state":"intact","hp":2400,"maxHp":2400,"material":"stone","anchor":true}}]]})");
	TestTrue(TEXT("start taken"), S.ApplyStart(*Parse(Start)));
	TestEqual(TEXT("start: one cell row"), S.LastCellRows.Num(), 1);
	TestTrue(TEXT("start: the section is kept"), S.Sections.Contains(0) && S.Sections[0].State == TEXT("intact") && S.Sections[0].Hp == 2400.0 && S.Sections[0].bAnchor && !S.Sections[0].IsDown());
	TestTrue(TEXT("start: state 8"), S.CellStates.Contains(0) && S.CellStates[0] == 8);

	// the collapse row, spelled as the native capture spells it (0.0, 2400.0), plus a state past 255, a negative one, a
	// row without data and an unreadable row
	S.ApplySnapshot(*Parse(TEXT(R"({"t":"s","tick":2,"cells":[[0,"W",0,0,{"ground":"W","object":".","material":"water","section":{"id":"map:bridge:0:0","structureId":"map:bridge:0","state":"failed","hp":0.0,"maxHp":2400.0,"material":"stone","anchor":true}}],[1,"F",0,258],[2,".",0,-1,{"ground":".","object":".","material":"soil"}],[99999,"x"],"junk"]})")));
	TestEqual(TEXT("collapse: rows applied in order"), S.LastCellRows, TArray<int32>({ 0, 1, 2 }));
	TestEqual(TEXT("collapse: rows skipped"), S.LastCellRowsSkipped, 2);
	TestTrue(TEXT("collapse: the cell is water"), S.Map.Chars[0] == 'W');
	const FTCCellSection* Down = S.Sections.Find(0);
	TestTrue(TEXT("collapse: the section is down"), Down && Down->IsDown() && Down->State == TEXT("failed") && Down->bHasHp && Down->Hp == 0.0 && Down->MaxHp == 2400.0);
	TestTrue(TEXT("collapse: state 258 wraps to 2, -1 to 255, absent state 0"), S.CellStates[1] == 2 && S.CellStates[2] == 255 && S.CellStates[0] == 0);
	TestTrue(TEXT("collapse: data kept"), S.CellData.Contains(0) && S.CellData[0].Material == TEXT("water") && !S.CellData.Contains(1));

	// the same row spelled as the source capture spells it (0, 2400): the same section
	FTCMatchState T;
	T.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","state":"play","you":0,"spectator":false,"players":[{"name":"A"}]})")));
	T.ApplyStart(*Parse(Start));
	T.ApplySnapshot(*Parse(TEXT(R"({"t":"s","tick":2,"cells":[[0,"W",0,0,{"ground":"W","object":".","material":"water","section":{"id":"map:bridge:0:0","structureId":"map:bridge:0","state":"failed","hp":0,"maxHp":2400,"material":"stone","anchor":true}}]]})")));
	TestEqual(TEXT("0 / 0.0 and 2400 / 2400.0: the same section"), SectionLine(T.Sections[0]), SectionLine(S.Sections[0]));

	// a later row without a section keeps the last one (terrain.structuralSections.set only when data.section)
	S.ApplySnapshot(*Parse(TEXT(R"({"t":"s","tick":4,"cells":[[0,"W",0,1,{"ground":"W","object":".","material":"water"}]]})")));
	TestTrue(TEXT("no section in the row: kept"), S.Sections.Contains(0) && S.Sections[0].IsDown() && S.CellStates[0] == 1);
	S.ApplySnapshot(*Parse(TEXT(R"({"t":"s","tick":6})")));
	TestEqual(TEXT("a snapshot without cells: no rows applied"), S.LastCellRows.Num(), 0);
	TestTrue(TEXT("a snapshot without cells: sections kept"), S.Sections.Num() == 1);

	// a new start empties them (startGame: a new structuralSections map)
	S.ApplyStart(*Parse(TEXT(R"({"t":"start","matchId":2,"you":0,"names":["A"],"teams":[0],"map":{"name":"t","w":3,"h":1,"rows":["..."]}})")));
	TestTrue(TEXT("new start: emptied"), S.Sections.Num() == 0 && S.CellStates.Num() == 0 && S.CellData.Num() == 0 && S.LastCellRows.Num() == 0);
	S.ClearMatch();
	TestTrue(TEXT("clear: emptied"), S.Sections.Num() == 0 && S.CellStates.Num() == 0);
	return true;
}

#endif // WITH_DEV_AUTOMATION_TESTS
