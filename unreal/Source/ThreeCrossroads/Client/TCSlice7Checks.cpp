// Slice 1g verification switches of ATCGameMode (see TCGameMode.h): the lobby settings, save and load, and the house
// right click on the world objects. Each step waits for what the server sends (a lobby, 'saved', a start, a pause)
// rather than a fixed time, with a timeout that is logged when it runs out. They run the player's own paths: HUD
// buttons through the HUD's hit test at their drawn centre (PressCommand, RightPressCommand), the house right click
// through the controller's world press at a computed pixel (RightPressWorld: the cursor's house comes from the house
// pick). Selections before a step are set directly; the socket drop is FTCConnection::SimulateDrop. None of this is a
// physical mouse or keyboard.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "Client/TCUnitRenderer.h"
#include "World/TCTerrain.h"
#include "Net/TCConnection.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "UnrealClient.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"

namespace TCSlice7Local
{
	FTCUiCommand SettingCommand(ETCLobbySetting Kind, int32 Slot)
	{
		FTCUiCommand C(ETCUiAction::LobbySetting);
		C.Unit = LexToString(Kind);
		C.Group = Slot;
		return C;
	}
	FString DescribeControl(const FTCLobbyControl& C)
	{
		return FString::Printf(TEXT("%s%s = '%s' (%s)%s%s%s"), *C.Title, C.Slot >= 0 ? *FString::Printf(TEXT("[seat %d]"), C.Slot) : TEXT(""), *C.CurrentValue, *C.CurrentLabel,
			C.bShown ? TEXT("") : TEXT(" hidden"), C.bEditable ? TEXT(" editable") : *FString::Printf(TEXT(" locked: %s"), *C.Why), C.bPending ? *FString::Printf(TEXT(", pending '%s'"), *C.PendingValue) : TEXT(""));
	}
}

void ATCGameMode::RunSlice7Switches(double Now)
{
	using namespace TCSlice7Local;
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!PC || !(bLobbyCheck || bSaveCheck || bGarrisonCheck || bResearchCheck || bRecoverCheck || bOptionsCheck)) return; // the runner below also runs slice 1h steps
	auto Add = [this](const FString& What, TFunction<bool()> Ready, TFunction<void()> Run, double Timeout = 10.0) { WaitSteps.Add({ What, MoveTemp(Ready), MoveTemp(Run), Timeout }); };
	auto Note = [PC]() { return FString::Printf(TEXT("%s note: '%s'"), PC->bLastOrderSent ? TEXT("sent") : TEXT("not sent"), *PC->LastOrderNote); };
	auto Room = [this]()
	{
		const FTCLobbyRoom& R = Lobby.Room();
		TArray<FString> Seats;
		for (const FTCLobbySeat& S : R.Seats) Seats.Add(FString::Printf(TEXT("%s team %d faction %d%s"), *S.Name, S.Team, S.Faction, S.bAi ? *FString::Printf(TEXT(" AI %s"), *S.Level) : TEXT("")));
		return FString::Printf(TEXT("room %s %s: mode %s, map %s, world size %s, army %s, defender %d, you %d, host %d [%s]; start '%s' (%s)%s"), *R.Code, *R.State, *R.Mode, *R.MapName, *R.WorldSize, *R.Army,
			R.DefenderTeam, R.You, R.Host, *FString::Join(Seats, TEXT("; ")), *Lobby.StartLabel(), Lobby.StartAllowed() ? TEXT("enabled") : TEXT("disabled"),
			Lobby.StartLine().IsEmpty() ? TEXT("") : *FString::Printf(TEXT(", line '%s'"), *Lobby.StartLine()));
	};
	// one press of a setting's button (left: the next choice, right: the previous), then the lobby that shows it
	auto Setting = [this, PC, Add, Note](ETCLobbySetting Kind, TFunction<int32()> Slot, bool bBack)
	{
		const FString What = FString::Printf(TEXT("lobby check: %s %s%s"), bBack ? TEXT("right press") : TEXT("press"), LexToString(Kind), bBack ? TEXT(" (previous)") : TEXT(""));
		TSharedRef<int32> Before = MakeShared<int32>(0);
		TSharedRef<FString> Asked = MakeShared<FString>();
		Add(What, nullptr, [this, PC, Kind, Slot, bBack, Before, Asked, Note, What]()
		{
			const int32 S = Slot();
			const FTCLobbyControl C0 = Lobby.Control(Kind, S);
			*Before = Lobby.Confirmed();
			const FTCUiCommand Cmd = SettingCommand(Kind, S);
			if (bBack) PC->RightPressCommand(Cmd); else PC->PressCommand(Cmd);
			const FTCLobbyControl C1 = Lobby.Control(Kind, S);
			*Asked = C1.bPending ? C1.PendingValue : FString();
			UE_LOG(LogTC, Log, TEXT("%s: before %s; %s; pending %d"), *What, *TCSlice7Local::DescribeControl(C0), *Note(), Lobby.PendingCount());
		});
		Add(FString::Printf(TEXT("lobby check: the lobby showing %s"), LexToString(Kind)), [this]() { return Lobby.PendingCount() == 0; }, [this, Kind, Slot, Before, Asked]()
		{
			const FTCLobbyControl C = Lobby.Control(Kind, Slot());
			UE_LOG(LogTC, Log, TEXT("lobby check: %s asked '%s': now %s; confirmed %d -> %d, pending %d"), LexToString(Kind), **Asked, *TCSlice7Local::DescribeControl(C), *Before, Lobby.Confirmed(), Lobby.PendingCount());
		}, 4.0);
	};
	// presses a room setting's button until Done holds (each press waits for its lobby), at most Tries presses
	auto StepUntil = [this, PC, Add](ETCLobbySetting Kind, FString Goal, TFunction<bool()> Done, int32 Tries)
	{
		struct FLoop { static void Push(ATCGameMode* G, ATCPlayerController* P, ETCLobbySetting K, FString T, TFunction<bool()> D, int32 Left, bool bFirst)
		{
			ATCGameMode::FWaitStep Step;
			Step.What = FString::Printf(TEXT("lobby check: %s toward %s"), LexToString(K), *T);
			Step.Ready = [G]() { return G->Lobby.PendingCount() == 0; };
			Step.Timeout = bFirst ? 1.0 : 4.0;
			Step.Run = [G, P, K, T, D, Left]()
			{
				const FTCLobbyControl C = G->Lobby.Control(K, -1);
				if (D() || Left <= 0) { UE_LOG(LogTC, Log, TEXT("lobby check: %s %s %s: %s"), LexToString(K), D() ? TEXT("reached") : TEXT("NOT reached"), *T, *TCSlice7Local::DescribeControl(C)); return; }
				P->PressCommand(TCSlice7Local::SettingCommand(K, -1));
				UE_LOG(LogTC, Log, TEXT("lobby check: %s pressed from '%s', note '%s'"), LexToString(K), *C.CurrentValue, *P->LastOrderNote);
				Push(G, P, K, T, D, Left - 1, false);
			};
			G->WaitSteps.Insert(MoveTemp(Step), 0);
		} };
		Add(FString::Printf(TEXT("lobby check: step %s toward %s"), LexToString(Kind), *Goal), nullptr, [this, PC, Kind, Goal, Done, Tries]() { FLoop::Push(this, PC, Kind, Goal, Done, Tries, true); });
	};
	auto StepTo = [this, StepUntil](ETCLobbySetting Kind, FString Target, int32 Tries)
	{
		StepUntil(Kind, FString::Printf(TEXT("'%s'"), *Target), [this, Kind, Target]() { return Lobby.Control(Kind, -1).CurrentValue == Target; }, Tries);
	};

	// -TCLobbyCheck
	if (bLobbyCheck && !bLobbyScriptMade && Lobby.InLobby() && LobbySeenAt >= 0 && Now - LobbySeenAt >= 1.0 && State.LobbyPlayers >= FMath::Max(1, HostSeats))
	{
		bLobbyScriptMade = true;
		const bool bHost = Lobby.Room().bAmHost && !State.bSpectator;
		UE_LOG(LogTC, Log, TEXT("lobby check: as %s; %s"), bHost ? TEXT("host") : TEXT("guest"), *Room());
		for (const FTCLobbyControl& C : Lobby.RoomControls()) UE_LOG(LogTC, Log, TEXT("lobby check: room control %s"), *DescribeControl(C));
		for (int32 I = 0; I < Lobby.Room().Seats.Num(); I++) for (const FTCLobbyControl& C : Lobby.SeatControls(I)) UE_LOG(LogTC, Log, TEXT("lobby check: seat control %s"), *DescribeControl(C));
		TFunction<int32()> Own = [this]() { return Lobby.Room().You; };
		if (bHost)
		{
			// the guest's own steps first (its faction changes), so both run against the same room
			TSharedRef<TArray<int32>> Factions = MakeShared<TArray<int32>>();
			for (const FTCLobbySeat& S : Lobby.Room().Seats) Factions->Add(S.Faction);
			Add(TEXT("lobby check: the guest's faction change"), [this, Factions]()
			{
				const TArray<FTCLobbySeat>& Seats = Lobby.Room().Seats;
				for (int32 I = 0; I < Seats.Num() && I < Factions->Num(); I++) if (I != Lobby.Room().You && !Seats[I].bAi && Seats[I].Faction != (*Factions)[I]) return true;
				return false;
			}, [this, Room]() { UE_LOG(LogTC, Log, TEXT("lobby check: after the guest: %s"), *Room()); }, 40.0);
			Add(TEXT("lobby check: Add AI twice through the hit test"), nullptr, [PC]() { PC->PressButton(ETCUiAction::AddAi); PC->PressButton(ETCUiAction::AddAi); });
			const int32 Want = State.LobbyPlayers + 2;
			Add(TEXT("lobby check: the AI seats in the lobby"), [this, Want]() { return State.LobbyPlayers >= Want; }, [this, Room]() { UE_LOG(LogTC, Log, TEXT("lobby check: %s"), *Room()); });
			TFunction<int32()> FirstAi = [this]() { for (int32 I = 0; I < Lobby.Room().Seats.Num(); I++) if (Lobby.Room().Seats[I].bAi) return I; return -1; };
			TFunction<int32()> NoSeat = []() { return -1; };
			Setting(ETCLobbySetting::Map, NoSeat, false);
			Setting(ETCLobbySetting::Army, NoSeat, false);
			Setting(ETCLobbySetting::Army, NoSeat, true);
			StepTo(ETCLobbySetting::Mode, TEXT("assault"), 7);
			Setting(ETCLobbySetting::Defender, NoSeat, false);
			Setting(ETCLobbySetting::Team, FirstAi, false);
			Setting(ETCLobbySetting::Faction, FirstAi, false);
			Setting(ETCLobbySetting::Level, FirstAi, false);
			Setting(ETCLobbySetting::Faction, Own, false);
			Setting(ETCLobbySetting::Team, Own, false);
			StepTo(ETCLobbySetting::Mode, TEXT("world"), 7);
			Setting(ETCLobbySetting::WorldSize, NoSeat, false);
			Setting(ETCLobbySetting::WorldSize, NoSeat, false);
			StepTo(ETCLobbySetting::Mode, LobbyCheckMode, 7);
			// leaving Tutorial on the way set its map (server.js 'mode'): step the Map until the seats fit and Start is enabled
			StepUntil(ETCLobbySetting::Map, TEXT("a map the seats fit"), [this]() { return Lobby.StartAllowed(); }, 34);
			Add(TEXT("lobby check: summary, then Start through the hit test"), nullptr, [this, PC, Room, Note]()
			{
				UE_LOG(LogTC, Log, TEXT("lobby check: final %s; confirmed %d, pending %d"), *Room(), Lobby.Confirmed(), Lobby.PendingCount());
				for (const FTCLobbyControl& C : Lobby.RoomControls()) UE_LOG(LogTC, Log, TEXT("lobby check: final room control %s"), *DescribeControl(C));
				for (int32 I = 0; I < Lobby.Room().Seats.Num(); I++) for (const FTCLobbyControl& C : Lobby.SeatControls(I)) if (C.bShown) UE_LOG(LogTC, Log, TEXT("lobby check: final seat control %s"), *DescribeControl(C));
				const ATCHUD* Hud = Cast<ATCHUD>(PC->GetHUD());
				UE_LOG(LogTC, Log, TEXT("lobby check: buttons on screen: %s"), Hud ? *Hud->ButtonLabels() : TEXT("no HUD"));
				PC->PressButton(ETCUiAction::Start);
				UE_LOG(LogTC, Log, TEXT("lobby check: Start: %s"), *Note());
			}, 8.0);
			Add(TEXT("lobby check: the match"), [this]() { return State.bHasMatch; }, [this]()
			{
				UE_LOG(LogTC, Log, TEXT("lobby check: match %d started: mode %s, map '%s' %dx%d, seat %d, names [%s]"), State.MatchId, *State.Mode, *State.Map.Name, State.Map.W, State.Map.H, State.You, *FString::Join(State.Names, TEXT(", ")));
			}, 20.0);
		}
		else
		{
			Setting(ETCLobbySetting::Faction, Own, false);
			Setting(ETCLobbySetting::Map, []() { return -1; }, false);
			Setting(ETCLobbySetting::Team, Own, false);
			Add(TEXT("lobby check: guest summary"), nullptr, [this, Room, PC]()
			{
				const ATCHUD* Hud = Cast<ATCHUD>(PC->GetHUD());
				UE_LOG(LogTC, Log, TEXT("lobby check: guest final %s; confirmed %d, pending %d; buttons on screen: %s"), *Room(), Lobby.Confirmed(), Lobby.PendingCount(), Hud ? *Hud->ButtonLabels() : TEXT("no HUD"));
			});
		}
	}

	// -TCSaveCheck
	if (bSaveCheck && !bSaveScriptMade && State.CanOrder() && State.Snapshots >= 3 && State.bAmHost)
	{
		bSaveScriptMade = true;
		TSharedRef<int32> SavedTick = MakeShared<int32>(-1), Opens = MakeShared<int32>(0), LoadedTick = MakeShared<int32>(-1), Starts = MakeShared<int32>(0);
		TSharedRef<FString> FirstId = MakeShared<FString>(), SecondId = MakeShared<FString>();
		TSharedRef<double> Mark = MakeShared<double>(0.0);
		auto Rows = [this]()
		{
			TArray<FString> L;
			for (const FTCSaveRow& R : Persistence.Rows()) L.Add(FString::Printf(TEXT("%s ['%s' | '%s']"), *R.Id, *R.Title, *R.Detail));
			return FString::Printf(TEXT("%d row(s): %s"), L.Num(), *FString::Join(L, TEXT("; ")));
		};
		UE_LOG(LogTC, Log, TEXT("save check: match %d at tick %d, mode %s; save button %s"), State.MatchId, State.Tick, *State.Mode, Persistence.SaveButton(State).bShown ? TEXT("shown") : TEXT("not shown"));
		Add(TEXT("save check: Pause match through the hit test"), nullptr, [PC, Note]() { PC->PressCommand(FTCUiCommand(ETCUiAction::PauseMatch)); UE_LOG(LogTC, Log, TEXT("save check: pause: %s"), *Note()); });
		Add(TEXT("save check: paused"), [this]() { return State.bPaused; }, [this, PC, Note, SavedTick]()
		{
			*SavedTick = State.Tick;
			UE_LOG(LogTC, Log, TEXT("save check: paused at tick %d ('%s'); Save match through the hit test"), State.Tick, *State.PauseText);
			PC->PressCommand(FTCUiCommand(ETCUiAction::SaveMatch));
			UE_LOG(LogTC, Log, TEXT("save check: save: %s; status %s"), *Note(), *Persistence.Status().Describe());
		});
		Add(TEXT("save check: 'saved' and the lobby's row"), [this]() { return Persistence.Status().Outcome == ETCPersistenceOutcome::Saved && Persistence.FindRow(Persistence.LastSavedId()); }, [this, PC, Rows, FirstId, SavedTick, Note]()
		{
			*FirstId = Persistence.LastSavedId();
			UE_LOG(LogTC, Log, TEXT("save check: saved %s at tick %d (paused at %d); %s; status %s"), **FirstId, State.Tick, *SavedTick, *Rows(), *Persistence.Status().Describe());
			PC->PressCommand(FTCUiCommand(ETCUiAction::PauseMatch));
			UE_LOG(LogTC, Log, TEXT("save check: resume: %s"), *Note());
		});
		Add(TEXT("save check: running, 60 ticks past the save"), [this, SavedTick]() { return !State.bPaused && State.Tick >= *SavedTick + 60; }, [this, PC, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("save check: running at tick %d; a second Save match (not paused)"), State.Tick);
			PC->PressCommand(FTCUiCommand(ETCUiAction::SaveMatch));
			UE_LOG(LogTC, Log, TEXT("save check: save: %s"), *Note());
		}, 20.0);
		Add(TEXT("save check: the second row"), [this, FirstId]() { return Persistence.Status().Outcome == ETCPersistenceOutcome::Saved && Persistence.LastSavedId() != *FirstId && Persistence.FindRow(Persistence.LastSavedId()); }, [this, PC, Rows, SecondId, Note]()
		{
			*SecondId = Persistence.LastSavedId();
			UE_LOG(LogTC, Log, TEXT("save check: saved %s at tick %d; %s; newest first: %s"), **SecondId, State.Tick, *Rows(), Persistence.Rows().Num() && Persistence.Rows()[0].Id == *SecondId ? TEXT("yes") : TEXT("NO"));
			PC->PressButton(ETCUiAction::End);
			UE_LOG(LogTC, Log, TEXT("save check: End match: %s"), *Note());
		});
		Add(TEXT("save check: back in the lobby"), [this]() { return !State.bHasMatch && State.RoomState == TEXT("lobby") && Lobby.InLobby(); }, [this, Rows, Mark]()
		{
			const FTCSaveListView V = Persistence.SaveList(State);
			UE_LOG(LogTC, Log, TEXT("save check: lobby: saves panel %s, heading '%s', load buttons %s, %s"), V.bShown ? TEXT("shown") : TEXT("hidden"), *V.Heading, V.bLoadButtons ? TEXT("yes") : TEXT("no"), *Rows());
			FString Shot;
			if (FParse::Value(FCommandLine::Get(), TEXT("TCSaveShotFile="), Shot)) { FScreenshotRequest::RequestScreenshot(Shot, true, false); UE_LOG(LogTC, Log, TEXT("save check: lobby screenshot requested: %s"), *Shot); }
			*Mark = FPlatformTime::Seconds();
		}, 15.0);
		Add(TEXT("save check: Load the first (paused) save through the hit test"), [Mark]() { return FPlatformTime::Seconds() - *Mark >= 1.0; }, [this, PC, FirstId, Note, Starts]()
		{
			*Starts = StartsReceived;
			FTCUiCommand Load(ETCUiAction::LoadSave);
			Load.Unit = *FirstId;
			PC->PressCommand(Load);
			UE_LOG(LogTC, Log, TEXT("save check: load: %s; status %s"), *Note(), *Persistence.Status().Describe());
		});
		Add(TEXT("save check: the loaded match"), [this, Starts]() { return Persistence.Status().Outcome == ETCPersistenceOutcome::Loaded && State.bHasMatch && StartsReceived > *Starts && State.Tick > 0; }, [this, SavedTick, LoadedTick, Mark]()
		{
			*LoadedTick = State.Tick;
			*Mark = FPlatformTime::Seconds();
			UE_LOG(LogTC, Log, TEXT("save check: loaded: %s; match %d at tick %d (saved at %d), %s ('%s'), %d units held"), *Persistence.Status().Describe(), State.MatchId, State.Tick, *SavedTick,
				State.bPaused ? TEXT("paused") : TEXT("RUNNING"), *State.PauseText, State.Units.Num());
		}, 20.0);
		Add(TEXT("save check: 3 s later, still paused"), [Mark]() { return FPlatformTime::Seconds() - *Mark >= 3.0; }, [this, LoadedTick, Opens, Starts]()
		{
			*Starts = StartsReceived;
			UE_LOG(LogTC, Log, TEXT("save check: 3 s after the load: tick %d (was %d), %s; dropping the socket (opens %d)"), State.Tick, *LoadedTick, State.bPaused ? TEXT("paused") : TEXT("RUNNING"), Connection ? Connection->Opens() : 0);
			*Opens = Connection ? Connection->Opens() : 0;
			if (Connection) Connection->SimulateDrop();
		});
		Add(TEXT("save check: the reconnect"), [this, Opens, Starts]() { return Connection && Connection->Opens() > *Opens && StartsReceived > *Starts && State.bHasMatch && State.SeatConfirmed() && State.Tick > 0; }, [this, Rows, LoadedTick]()
		{
			UE_LOG(LogTC, Log, TEXT("save check: reconnected (opens %d): match %d at tick %d (loaded at %d), %s ('%s'); %s; status %s"), Connection->Opens(), State.MatchId, State.Tick, *LoadedTick,
				State.bPaused ? TEXT("paused") : TEXT("RUNNING"), *State.PauseText, *Rows(), *Persistence.Status().Describe());
		}, 20.0);
		Add(TEXT("save check: Resume through the hit test"), [this]() { return State.bHasMatch; }, [PC, Note]() { PC->PressCommand(FTCUiCommand(ETCUiAction::PauseMatch)); UE_LOG(LogTC, Log, TEXT("save check: resume: %s"), *Note()); });
		Add(TEXT("save check: running past the saved tick"), [this, LoadedTick]() { return !State.bPaused && State.Tick >= *LoadedTick + 40; }, [this, SavedTick, LoadedTick]()
		{
			UE_LOG(LogTC, Log, TEXT("save check: running again: tick %d, loaded at %d, saved at %d; done"), State.Tick, *LoadedTick, *SavedTick);
		}, 20.0);
	}

	// -TCGarrisonCheck
	if (bGarrisonCheck && !bGarrisonScriptMade && State.CanOrder() && State.Snapshots >= 3 && WorldObjects.HouseCount() > 0 && WorldView.SectionsDrawn() > 0 && Terrain)
	{
		double CX, CZ;
		const TArray<int32> Army = ArmyIds(CX, CZ);
		if (!Army.Num()) return;
		if (bHostSandbox && !bGarrisonSpawnSent && State.bSandbox)
		{
			bGarrisonSpawnSent = true;
			TSharedRef<FJsonObject> Sp = MakeShared<FJsonObject>();
			Sp->SetStringField(TEXT("t"), TEXT("spawn")); Sp->SetStringField(TEXT("unit"), TEXT("mortar")); Sp->SetNumberField(TEXT("x"), CX + 3.0); Sp->SetNumberField(TEXT("z"), CZ);
			UE_LOG(LogTC, Log, TEXT("garrison check: sandbox spawn of an own mortar at (%.1f, %.1f): %s"), CX + 3.0, CZ, SendCommand(Sp) ? TEXT("sent") : *LastSendProblemText);
			return;
		}
		if (bGarrisonSpawnSent && !Army.ContainsByPredicate([this](int32 Id) { const FTCUnit* U = State.Units.Find(Id); return U && FTCSupportPowers::IsSalvo(U->Type); })) return;
		bGarrisonScriptMade = true;
		// the nearest house cell (of a typed block) to the army
		int32 Best = -1;
		double BestD = 1e18;
		for (int32 C = 0; C < WorldObjects.MapW() * WorldObjects.MapH(); C++)
		{
			if (!WorldObjects.IsHouseCell(C) || WorldObjects.HouseTypeAt(C) < 0) continue;
			const FVector2D P(((C % WorldObjects.MapW()) + 0.5) * 2.0, ((C / WorldObjects.MapW()) + 0.5) * 2.0);
			const double D = FVector2D::Distance(P, FVector2D(CX, CZ));
			if (D < BestD) { BestD = D; Best = C; }
		}
		if (Best < 0) { UE_LOG(LogTC, Warning, TEXT("garrison check: no house cell with a type on this map")); return; }
		const FVector2D House(((Best % WorldObjects.MapW()) + 0.5) * 2.0, ((Best / WorldObjects.MapW()) + 0.5) * 2.0);
		const int32 Type = WorldObjects.HouseTypeAt(Best);
		UE_LOG(LogTC, Log, TEXT("garrison check: army of %d at (%.1f, %.1f); nearest house cell %d at (%.1f, %.1f), %.1f m away, a %s; %d houses, %d solids (%d pickable)"), Army.Num(), CX, CZ, Best, House.X, House.Y, BestD,
			ANSI_TO_TCHAR(TCWorldCore::HouseTypes[Type].Name), WorldObjects.HouseCount(), WorldObjects.ShapeCount(), WorldObjects.PickableCount());
		Add(TEXT("garrison check: select the army (directly), the camera on the house"), nullptr, [PC, Army, House]() { PC->SelectOwn(Army); PC->FocusOn(House); PC->ZoomTo(45.f); });
		TSharedRef<FVector2D> Pixel = MakeShared<FVector2D>(-1, -1);
		TSharedRef<double> Mark = MakeShared<double>(FPlatformTime::Seconds() + 1.5);
		Add(TEXT("garrison check: find the house's pixel"), [Mark]() { return FPlatformTime::Seconds() >= *Mark; }, [this, PC, House, Best, Pixel]()
		{
			// the pixel of a point over the house cell's centre, from 2 to 8 m up, whose ray the house pick takes
			const float G = Terrain->HeightAt(House.X, House.Y);
			for (float Up = 2.f; Up <= 8.f && Pixel->X < 0; Up += 1.f)
			{
				FVector2D Px;
				FTCHousePick Pick;
				if (PC->ProjectWorldLocationToScreen(ATCTerrain::ToWorld(House.X, House.Y, G + Up), Px) && PC->HouseAt(Px, Pick))
				{
					*Pixel = Px;
					UE_LOG(LogTC, Log, TEXT("garrison check: pixel (%.0f, %.0f) for %.0f m over the cell: the house pick says cell %d (target %d), centre (%.1f, %.1f), %.1f m along the ray"), Px.X, Px.Y, Up, Pick.Cell, Best, Pick.Center.X, Pick.Center.Y, Pick.DistanceMetres);
				}
			}
			if (Pixel->X < 0) UE_LOG(LogTC, Warning, TEXT("garrison check: no pixel over the house is taken by the house pick (no renderer, or hidden)"));
			if (!GarrisonShotFile.IsEmpty()) { FScreenshotRequest::RequestScreenshot(GarrisonShotFile, true, false); UE_LOG(LogTC, Log, TEXT("garrison check: screenshot requested: %s"), *GarrisonShotFile); }
		});
		TSharedRef<TArray<int32>> Before = MakeShared<TArray<int32>>();
		Add(TEXT("garrison check: the right press on the house through the world path"), nullptr, [this, PC, Pixel, Note, Before]()
		{
			for (const auto& P : State.Units) if (State.IsOwn(P.Value) && (P.Value.Flags & TCFlags::Garrisoned)) Before->Add(P.Key);
			if (Pixel->X < 0) { UE_LOG(LogTC, Warning, TEXT("garrison check: skipped, no pixel")); return; }
			const FTCCursor C = PC->CursorAt(*Pixel);
			UE_LOG(LogTC, Log, TEXT("garrison check: cursor at (%.0f, %.0f): house %s cell %d (%.1f, %.1f), enemy %d, friend %d, building %d, ground %s; %d selected, %d garrisoned before"), Pixel->X, Pixel->Y,
				C.bHouse ? TEXT("yes") : TEXT("no"), C.HouseCell, C.House.X, C.House.Y, C.Enemy, C.Friend, C.Building, C.bHasGround ? TEXT("yes") : TEXT("no"), PC->Selected.Num(), Before->Num());
			const bool bGesture = PC->RightPressWorld(*Pixel, false, false);
			UE_LOG(LogTC, Log, TEXT("garrison check: right press: %s; %s"), bGesture ? TEXT("a facing gesture started (WRONG: a house is not a move)") : TEXT("dispatched at once"), *Note());
		});
		Add(TEXT("garrison check: garrisoned units in a snapshot"), [this, Before]()
		{
			for (const auto& P : State.Units) if (State.IsOwn(P.Value) && (P.Value.Flags & TCFlags::Garrisoned) && !Before->Contains(P.Key)) return true;
			return false;
		}, [this]()
		{
			TArray<FString> In;
			for (const auto& P : State.Units)
				if (State.IsOwn(P.Value) && (P.Value.Flags & TCFlags::Garrisoned))
				{
					const TCWorldCore::FHouseType* H = FTCWorldObjects::GarrisonHouseType(P.Value.Flags);
					In.Add(FString::Printf(TEXT("%d %s in a %s at (%.1f, %.1f)"), P.Key, *P.Value.Type, H ? ANSI_TO_TCHAR(H->Name) : TEXT("?"), P.Value.X, P.Value.Z));
				}
			UE_LOG(LogTC, Log, TEXT("garrison check: tick %d: %d own unit(s) garrisoned: %s; denies %d (last %s/%s)"), State.Tick, In.Num(), *FString::Join(In, TEXT("; ")), State.Denies, *State.DenyCmd, *State.DenyReason);
		}, 45.0);
	}

	// the wait-step runner: a step runs once its condition holds (or its timeout runs out, which is logged), never two
	// steps in one frame, so the HUD has drawn the buttons a step presses
	if (WaitSteps.Num() && GFrameCounter > WaitStepFrame + 1)
	{
		if (WaitStepSince < 0) WaitStepSince = Now;
		const bool bReady = !WaitSteps[0].Ready || WaitSteps[0].Ready();
		const bool bLate = Now - WaitStepSince >= WaitSteps[0].Timeout;
		if (bReady || bLate)
		{
			const FWaitStep Step = WaitSteps[0];
			WaitSteps.RemoveAt(0);
			WaitStepSince = Now;
			WaitStepFrame = GFrameCounter;
			UE_LOG(LogTC, Log, TEXT("step (frame %llu): %s%s"), (unsigned long long)GFrameCounter, *Step.What, bReady ? TEXT("") : *FString::Printf(TEXT(" (TIMED OUT after %.0f s waiting; run anyway)"), Step.Timeout));
			if (Step.Run) Step.Run();
		}
	}
}
