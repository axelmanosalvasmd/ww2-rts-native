// Slice 1h verification switches of ATCGameMode (see TCGameMode.h): HQ tier research, World recovery and region
// objectives, and the lobby's Weather, Sandbox and seat removal. Each step waits for what the server sends (a snapshot,
// a deny, a lobby, a start) rather than a fixed time, with a timeout that is logged when it runs out; the steps run on
// the wait-step runner in TCSlice7Checks.cpp. HUD buttons are pressed through the HUD's hit test at their drawn centre
// (PressCommand, RightPressCommand); a button the HUD does not draw is run directly and logged as such. Selections are
// set directly, sandbox spawns and the raw probes are sent straight to the socket (a probe is how a server refusal is
// reached when the client's own check would send nothing). None of this is a physical mouse or keyboard.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "Client/TCOrders.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "Dom/JsonObject.h"
#include "UnrealClient.h"

namespace TCSlice8Local
{
	FTCUiCommand OptionCommand(ETCMatchOption Kind, int32 Slot = -1)
	{
		FTCUiCommand C(ETCUiAction::MatchOption);
		C.Unit = LexToString(Kind);
		C.Group = Slot;
		return C;
	}
	FString DescribeOption(const FTCOptionControl& C)
	{
		return FString::Printf(TEXT("%s%s = '%s' (%s)%s%s%s"), *C.Title, C.Slot >= 0 ? *FString::Printf(TEXT("[seat %d %s]"), C.Slot, *C.SeatName) : TEXT(""), *C.CurrentValue, *C.CurrentLabel,
			C.bShown ? TEXT("") : TEXT(" hidden"), C.bEditable ? TEXT(" editable") : *FString::Printf(TEXT(" locked: %s"), *C.Why), C.bPending ? *FString::Printf(TEXT(", pending '%s'"), *C.PendingValue) : TEXT(""));
	}
	FString DescribePanel(const FTCResearchPanel& P)
	{
		if (!P.bShown) return TEXT("no research panel");
		TArray<FString> B;
		for (const FTCResearchButton& X : P.Buttons)
			B.Add(FString::Printf(TEXT("[%s] '%s' %s%s"), LexToString(X.Kind), *X.Text, X.Check.bOk ? TEXT("available") : *FString::Printf(TEXT("refused (%s%s)"), *X.Check.Reason, X.Check.bNative ? TEXT(", native") : TEXT("")),
				X.bResearching ? *FString::Printf(TEXT(", researching %.2f"), X.Progress) : TEXT("")));
		return FString::Printf(TEXT("%s %d, %s: %s"), *P.BuildingType, P.Building, *P.TierLabel, *FString::Join(B, TEXT("; ")));
	}
	FString DescribeCard(const FTCWorldRecoveryCard& C)
	{
		static const TCHAR* Names[] = { TEXT("Shown"), TEXT("NoSnapshot"), TEXT("NotConstruction"), TEXT("TrucksOnly"), TEXT("Building"), TEXT("Engineers"), TEXT("NotOffered") };
		const int32 D = (int32)C.Decision;
		return FString::Printf(TEXT("decision %s%s"), D >= 0 && D < UE_ARRAY_COUNT(Names) ? Names[D] : TEXT("?"),
			C.bShown ? *FString::Printf(TEXT(": '%s' %s (%s)%s, key %s"), *C.Name, *C.Cost, *C.Sub, C.bDisabled ? TEXT(" disabled") : TEXT(""), *C.Key) : TEXT(""));
	}
}

void ATCGameMode::RunSlice8Switches(double Now)
{
	using namespace TCSlice8Local;
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!PC || !(bResearchCheck || bRecoverCheck || bOptionsCheck)) return;
	auto Add = [this](const FString& What, TFunction<bool()> Ready, TFunction<void()> Run, double Timeout = 10.0) { WaitSteps.Add({ What, MoveTemp(Ready), MoveTemp(Run), Timeout }); };
	auto Note = [PC]() { return FString::Printf(TEXT("%s note: '%s'"), PC->bLastOrderSent ? TEXT("sent") : TEXT("not sent"), *PC->LastOrderNote); };
	auto OwnOf = [this](const TCHAR* Type)
	{
		TArray<int32> Ids;
		for (const auto& P : State.Units) if (State.IsOwn(P.Value) && P.Value.Hp > 0 && P.Value.Type == Type) Ids.Add(P.Key);
		Ids.Sort();
		return Ids;
	};
	auto Deny = [this]() { return FString::Printf(TEXT("denies %d, last '%s'/'%s': '%s'"), State.Denies, *State.DenyCmd, *State.DenyReason, *TCOrders::DenySentence(State.DenyReason, State.DenyCmd)); };
	// a raw command straight to the socket, then the deny it draws (or the timeout)
	auto Probe = [this, Add, Deny](const FString& What, TFunction<TSharedRef<FJsonObject>()> Make, TFunction<FString()> Predict)
	{
		TSharedRef<int32> Before = MakeShared<int32>(0);
		Add(FString::Printf(TEXT("probe: raw %s"), *What), nullptr, [this, What, Make, Predict, Before]()
		{
			*Before = State.Denies;
			const TSharedRef<FJsonObject> O = Make();
			UE_LOG(LogTC, Log, TEXT("probe: raw %s (this client's own check: %s): %s"), *What, *Predict(), SendCommand(O) ? TEXT("sent") : *LastSendProblemText);
		});
		Add(FString::Printf(TEXT("probe: the server's answer to %s"), *What), [this, Before]() { return State.Denies > *Before; }, [What, Deny]()
		{
			UE_LOG(LogTC, Log, TEXT("probe: %s answered: %s"), *What, *Deny());
		}, 5.0);
	};

	// -TCResearchCheck
	if (bResearchCheck && !bResearchScriptMade && State.CanOrder() && State.Snapshots >= 3 && Progress.HasTech() && OwnOf(TEXT("hq")).Num())
	{
		bResearchScriptMade = true;
		TFunction<void()> SelectHq = [this, PC, OwnOf]()
		{
			const TArray<int32> Hq = OwnOf(TEXT("hq"));
			PC->Selected.Reset();
			if (Hq.Num()) PC->Selected.Add(Hq[0]);
		};
		SelectHq();
		UE_LOG(LogTC, Log, TEXT("research check: mode %s, tick %d, MP %d, HQ %d selected directly; panel %s"), *State.Mode, State.Tick, State.Mp, PC->Selected.Num() ? *PC->Selected.CreateConstIterator() : 0, *DescribePanel(Progress.Panel(State, PC->Selected)));
		FTCUiCommand Tier(ETCUiAction::Research);
		Tier.Unit = LexToString(ETCTechKind::Tier);
		TSharedRef<bool> bSentFirst = MakeShared<bool>(false);
		Add(TEXT("research check: the tier button, first press"), nullptr, [this, PC, Tier, Note, SelectHq, bSentFirst]()
		{
			SelectHq();
			const FTCRecruitCheck C = Progress.CheckResearch(State, ETCTechKind::Tier);
			*bSentFirst = PC->PressCommand(Tier);
			UE_LOG(LogTC, Log, TEXT("research check: first press at MP %d (check: %s): %s; job %s"), State.Mp, C.bOk ? TEXT("available") : *C.Reason, *Note(), Progress.Job(ETCTechKind::Tier) ? TEXT("held") : TEXT("none"));
		});
		Probe(TEXT("{ t: 'tech', kind: 'inf' } with no Armory"), []() { TSharedRef<FJsonObject> O = MakeShared<FJsonObject>(); O->SetStringField(TEXT("t"), TEXT("tech")); O->SetStringField(TEXT("kind"), TEXT("inf")); return O; },
			[this]() { const FTCRecruitCheck C = Progress.CheckResearch(State, ETCTechKind::Inf); return C.bOk ? FString(TEXT("available")) : C.Reason; });
		Add(TEXT("research check: the tier affordable, or its job already running"), [this]() { return Progress.Job(ETCTechKind::Tier) != nullptr || Progress.CheckResearch(State, ETCTechKind::Tier).bOk; }, [this, PC, Tier, Note, SelectHq]()
		{
			SelectHq();
			if (Progress.Job(ETCTechKind::Tier)) { UE_LOG(LogTC, Log, TEXT("research check: the tier job is already running (sent by the first press)")); return; }
			UE_LOG(LogTC, Log, TEXT("research check: panel before the press: %s"), *DescribePanel(Progress.Panel(State, PC->Selected)));
			PC->PressCommand(Tier);
			UE_LOG(LogTC, Log, TEXT("research check: press at MP %d: %s"), State.Mp, *Note());
		}, 150.0);
		Add(TEXT("research check: the tier job in a snapshot"), [this]() { return Progress.Job(ETCTechKind::Tier) != nullptr; }, [this, PC, SelectHq, Deny]()
		{
			SelectHq();
			const FTCResearchJob* J = Progress.Job(ETCTechKind::Tier);
			UE_LOG(LogTC, Log, TEXT("research check: tick %d: lab job %s progress %.3f, MP %d, tier %d; %s; panel %s"), State.Tick, J ? *J->Kind : TEXT("none"), J ? J->Progress : 0.0, State.Mp, Progress.Tier(), *Deny(), *DescribePanel(Progress.Panel(State, PC->Selected)));
		}, 10.0);
		Add(TEXT("research check: the tier button again while it researches"), nullptr, [this, PC, Tier, Note, SelectHq]()
		{
			SelectHq();
			const int32 Before = State.Denies;
			PC->PressCommand(Tier);
			UE_LOG(LogTC, Log, TEXT("research check: second press: %s (denies %d -> %d)"), *Note(), Before, State.Denies);
		});
		Add(TEXT("research check: the new tier"), [this]() { return Progress.Tier() >= 2; }, [this, PC, SelectHq]()
		{
			SelectHq();
			UE_LOG(LogTC, Log, TEXT("research check: tick %d: tier %d; panel %s"), State.Tick, Progress.Tier(), *DescribePanel(Progress.Panel(State, PC->Selected)));
		}, 80.0);
	}

	// -TCRecoverCheck
	if (bRecoverCheck && !bRecoverScriptMade && State.CanOrder() && State.Snapshots >= 3 && WorldRec.IsWorld())
	{
		bRecoverScriptMade = true;
		auto World = [this, OwnOf]()
		{
			const FTCWorldSnapshot& W = WorldRec.Snapshot();
			const FTCWorldScoreLead L = WorldRec.ScoreLead();
			TArray<FString> Teams;
			for (const FTCWorldTeamScore& T : WorldRec.TeamScores()) Teams.Add(FString::Printf(TEXT("team %.0f %s %s %s"), T.Team, *T.Label, *T.Number, *T.Unit));
			int32 Mine = 0, Locked = 0, Unknown = 0;
			for (const FTCWorldRegion& R : W.Regions)
			{
				Mine += R.Team == WorldRec.MyTeam();
				Locked += R.bLocked;
				// every region sent must lie on cells this side knows (the server sends only discovered regions)
				const int32 CX = FMath::FloorToInt(R.X / FTCMatchState::CellMetres), CY = FMath::FloorToInt(R.Z / FTCMatchState::CellMetres);
				const int32 Cell = CY * State.Map.W + CX;
				if (!R.FinitePosition() || !State.Map.Known.IsValidIndex(Cell) || !State.Map.Known[Cell]) Unknown++;
			}
			return FString::Printf(TEXT("tick %d, MP %.0f: '%s' '%s'; [%s]; %d region(s) sent (%d this team's, %d locked, %d on unknown cells), %d station(s); recovery %s (hq %s, engineer %s); own hq %d, engineers %d"),
				State.Tick, W.Mp, *L.Mode, *L.Clock, *FString::Join(Teams, TEXT("; ")), W.Regions.Num(), Mine, Locked, Unknown, W.Stations.Num(), W.bRecovery ? (W.bRecoveryAvailable ? TEXT("available") : TEXT("not available")) : TEXT("absent"),
				*W.RecoverHqText, *W.RecoverEngineerText, OwnOf(TEXT("hq")).Num(), OwnOf(TEXT("engineer")).Num());
		};
		PC->Selected.Reset();
		UE_LOG(LogTC, Log, TEXT("recover check: %s; card with nothing selected: %s"), *World(), *DescribeCard(WorldRec.Card(State, PC->Selected)));
		Add(TEXT("recover check: Restore pressed with no card shown"), nullptr, [this, PC, Note]()
		{
			PC->Selected.Reset();
			const int32 Before = State.Denies;
			PC->PressCommand(FTCUiCommand(ETCUiAction::Recover));
			UE_LOG(LogTC, Log, TEXT("recover check: %s (denies %d -> %d)"), *Note(), Before, State.Denies);
		});
		Probe(TEXT("{ t: 'recover' } while the HQ and Engineers live"), []() { TSharedRef<FJsonObject> O = MakeShared<FJsonObject>(); O->SetStringField(TEXT("t"), TEXT("recover")); return O; },
			[this, PC]() { return DescribeCard(WorldRec.Card(State, PC->Selected)); });
		// enemy tanks on the own Engineers until the snapshot offers recovery (again on the survivors every 25 s)
		auto Spawn = [this, OwnOf]()
		{
			int32 Sent = 0;
			for (int32 Id : OwnOf(TEXT("engineer")))
				if (const FTCUnit* U = State.Units.Find(Id))
					for (int32 K = 0; K < 3; K++)
					{
						TSharedRef<FJsonObject> Sp = MakeShared<FJsonObject>();
						Sp->SetStringField(TEXT("t"), TEXT("spawn")); Sp->SetStringField(TEXT("unit"), TEXT("medium")); Sp->SetBoolField(TEXT("enemy"), true);
						Sp->SetNumberField(TEXT("x"), U->X + 6.0 * FMath::Cos(K * 2.1)); Sp->SetNumberField(TEXT("z"), U->Z + 6.0 * FMath::Sin(K * 2.1));
						Sent += SendCommand(Sp);
					}
			UE_LOG(LogTC, Log, TEXT("recover check: sandbox %s: %d enemy medium tank(s) sent onto %d own Engineer(s)"), State.bSandbox ? TEXT("on") : TEXT("OFF (spawns are refused)"), Sent, OwnOf(TEXT("engineer")).Num());
		};
		for (int32 Round = 0; Round < 4; Round++)
			Add(FString::Printf(TEXT("recover check: spawn round %d, or recovery offered"), Round + 1), Round == 0 ? TFunction<bool()>() : TFunction<bool()>([this]() { return WorldRec.Snapshot().bRecoveryAvailable; }), [this, Spawn, World]()
			{
				if (WorldRec.Snapshot().bRecoveryAvailable) { UE_LOG(LogTC, Log, TEXT("recover check: recovery offered: %s"), *World()); return; }
				Spawn();
			}, 25.0);
		TSharedRef<double> MpBefore = MakeShared<double>(0);
		TSharedRef<int32> DeniesBefore = MakeShared<int32>(0), HqBefore = MakeShared<int32>(0), EngBefore = MakeShared<int32>(0);
		Add(TEXT("recover check: recovery offered"), [this]() { return WorldRec.Snapshot().bRecoveryAvailable; }, [this, PC, World]()
		{
			PC->Selected.Reset();
			UE_LOG(LogTC, Log, TEXT("recover check: %s; card %s"), *World(), *DescribeCard(WorldRec.Card(State, PC->Selected)));
		}, 30.0);
		Add(TEXT("recover check: the Restore card through the hit test"), nullptr, [this, PC, Note, MpBefore, DeniesBefore, HqBefore, EngBefore, OwnOf]()
		{
			PC->Selected.Reset();
			*MpBefore = WorldRec.Snapshot().Mp; *DeniesBefore = State.Denies; *HqBefore = OwnOf(TEXT("hq")).Num(); *EngBefore = OwnOf(TEXT("engineer")).Num();
			const FTCWorldRecoveryCard C = WorldRec.Card(State, PC->Selected);
			PC->PressCommand(FTCUiCommand(ETCUiAction::Recover));
			UE_LOG(LogTC, Log, TEXT("recover check: card %s; %s"), *DescribeCard(C), *Note());
		});
		Add(TEXT("recover check: the server's answer"), [this, MpBefore, DeniesBefore]() { return State.Denies > *DeniesBefore || WorldRec.Snapshot().Mp < *MpBefore - 1; }, [this, World, MpBefore, DeniesBefore, HqBefore, EngBefore, OwnOf, Deny]()
		{
			UE_LOG(LogTC, Log, TEXT("recover check: MP %.0f -> %.0f, hq %d -> %d, engineers %d -> %d, %s%s; %s"), *MpBefore, WorldRec.Snapshot().Mp, *HqBefore, OwnOf(TEXT("hq")).Num(), *EngBefore, OwnOf(TEXT("engineer")).Num(),
				*Deny(), State.Denies > *DeniesBefore ? TEXT(" (REFUSED)") : TEXT(" (accepted: MP spent)"), *World());
		}, 8.0);
		Add(TEXT("recover check: a later snapshot"), nullptr, [this, World, PC]()
		{
			UE_LOG(LogTC, Log, TEXT("recover check: later: %s; card %s"), *World(), *DescribeCard(WorldRec.Card(State, PC->Selected)));
		}, 12.0);
	}

	// -TCOptionsCheck
	if (bOptionsCheck && !bOptionsScriptMade && Options.InLobby() && LobbySeenAt >= 0 && Now - LobbySeenAt >= 1.0 && State.LobbyPlayers >= FMath::Max(1, HostSeats))
	{
		bOptionsScriptMade = true;
		const bool bHost = Options.AmHost() && !State.bSpectator;
		auto LogOptions = [this](const FString& Why)
		{
			TArray<FString> Kicks;
			for (const FTCOptionControl& K : Options.KickControls()) Kicks.Add(DescribeOption(K));
			UE_LOG(LogTC, Log, TEXT("options check: %s: %s; %s; seats [%s]; pending %d, confirmed %d"), *Why, *DescribeOption(Options.Control(ETCMatchOption::Weather)), *DescribeOption(Options.Control(ETCMatchOption::Sandbox)),
				*FString::Join(Kicks, TEXT("; ")), Options.PendingCount(), Options.Confirmed());
		};
		LogOptions(bHost ? TEXT("as host") : TEXT("as guest"));
		auto Press = [this, PC, Add, Note, LogOptions](FTCUiCommand Cmd, bool bBack, const FString& What)
		{
			TSharedRef<int32> Before = MakeShared<int32>(0);
			Add(FString::Printf(TEXT("options check: %s"), *What), nullptr, [this, PC, Cmd, bBack, What, Note, Before]()
			{
				*Before = Options.Confirmed();
				const bool bSent = bBack ? PC->RightPressCommand(Cmd) : PC->PressCommand(Cmd);
				UE_LOG(LogTC, Log, TEXT("options check: %s: %s (%s), pending %d"), *What, *Note(), bSent ? TEXT("sent") : TEXT("nothing sent"), Options.PendingCount());
			});
			Add(FString::Printf(TEXT("options check: the lobby after %s"), *What), [this]() { return Options.PendingCount() == 0; }, [LogOptions, What, Before, this]()
			{
				LogOptions(FString::Printf(TEXT("after %s (confirmed %d -> %d)"), *What, *Before, Options.Confirmed()));
			}, 4.0);
		};
		if (bHost)
		{
			// with a guest seated, its refusals run first in the same lobby (they send nothing, so there is no message to
			// wait for: the host gives it 4 s)
			const double MadeAt = Now;
			if (HostSeats > 1) Add(TEXT("options check: 4 s for the guest's own presses"), [MadeAt]() { return FPlatformTime::Seconds() - MadeAt >= 4.0; }, nullptr, 6.0);
			Press(OptionCommand(ETCMatchOption::Weather), false, TEXT("Weather press (next)"));
			Press(OptionCommand(ETCMatchOption::Weather), true, TEXT("Weather right press (previous)"));
			Press(OptionCommand(ETCMatchOption::Sandbox), false, TEXT("Sandbox press (on)"));
			Press(OptionCommand(ETCMatchOption::Sandbox), false, TEXT("Sandbox press (off)"));
			const int32 SeatsBefore = State.LobbyPlayers;
			Add(TEXT("options check: Add AI through the hit test"), nullptr, [PC]() { PC->PressButton(ETCUiAction::AddAi); });
			TSharedRef<int32> AiSeat = MakeShared<int32>(-1);
			Add(TEXT("options check: the AI seat in the lobby"), [this, SeatsBefore]() { return Options.Seats().Num() > SeatsBefore; }, [this, AiSeat, LogOptions]()
			{
				for (int32 I = Options.Seats().Num() - 1; I >= 0 && *AiSeat < 0; I--) if (Options.Seats()[I].bAi) *AiSeat = I;
				LogOptions(FString::Printf(TEXT("AI seat %d"), *AiSeat));
			});
			Add(TEXT("options check: Remove on the AI seat"), nullptr, [this, PC, AiSeat, Note]()
			{
				const int32 Seats = Options.Seats().Num();
				PC->PressCommand(OptionCommand(ETCMatchOption::Kick, *AiSeat));
				UE_LOG(LogTC, Log, TEXT("options check: Remove seat %d: %s; %d seats, pending %d"), *AiSeat, *Note(), Seats, Options.PendingCount());
			});
			Add(TEXT("options check: the lobby without that seat"), [this]() { return Options.PendingCount() == 0; }, [LogOptions]() { LogOptions(TEXT("after Remove")); }, 4.0);
			Add(TEXT("options check: Remove on the own (connected) seat"), nullptr, [this, PC, Note]()
			{
				const int32 Own = State.LobbySeat;
				PC->PressCommand(OptionCommand(ETCMatchOption::Kick, Own));
				UE_LOG(LogTC, Log, TEXT("options check: Remove own seat %d: %s; pending %d"), Own, *Note(), Options.PendingCount());
			});
			Press(OptionCommand(ETCMatchOption::Weather), false, TEXT("Weather press toward fog (1)"));
			Press(OptionCommand(ETCMatchOption::Weather), false, TEXT("Weather press toward fog (2)"));
			Add(TEXT("options check: Add AI for the match"), nullptr, [PC]() { PC->PressButton(ETCUiAction::AddAi); });
			Add(TEXT("options check: Start"), [this, SeatsBefore]() { return Options.Seats().Num() > SeatsBefore; }, [this, PC, LogOptions]()
			{
				LogOptions(TEXT("before Start"));
				PC->PressButton(ETCUiAction::Start);
				UE_LOG(LogTC, Log, TEXT("options check: Start: '%s'"), *PC->LastOrderNote);
			}, 5.0);
		}
		else
		{
			// a guest: each control is locked (the HUD greys it); a press runs the same check and sends nothing
			for (const ETCMatchOption K : { ETCMatchOption::Weather, ETCMatchOption::Sandbox })
				Add(FString::Printf(TEXT("options check: guest presses %s"), LexToString(K)), nullptr, [PC, K, Note, this]()
				{
					const bool bSent = PC->PressCommand(OptionCommand(K));
					UE_LOG(LogTC, Log, TEXT("options check: guest %s: %s (%s), pending %d"), LexToString(K), *Note(), bSent ? TEXT("SENT") : TEXT("nothing sent"), Options.PendingCount());
				});
			Add(TEXT("options check: guest presses Remove on the host's seat"), nullptr, [PC, Note, this]()
			{
				const bool bSent = PC->PressCommand(OptionCommand(ETCMatchOption::Kick, 0));
				UE_LOG(LogTC, Log, TEXT("options check: guest Remove seat 0: %s (%s), pending %d"), *Note(), bSent ? TEXT("SENT") : TEXT("nothing sent"), Options.PendingCount());
			});
			Add(TEXT("options check: guest sees the host's fog"), [this]() { return Options.WeatherChoice() == TEXT("fog"); }, [LogOptions]() { LogOptions(TEXT("guest, after the host's changes")); }, 60.0);
		}
	}
}
