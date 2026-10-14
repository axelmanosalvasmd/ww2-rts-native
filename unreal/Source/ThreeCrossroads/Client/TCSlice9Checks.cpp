// Slice 9 verification switches of ATCGameMode (see TCGameMode.h): scripted capture play for a Conquest check, the
// decoded objectives logged for comparison with another recipient's copy, screenshots of a capture point, the
// epilogue's stamp and the result card, and the gap 13 keys through simulated key events. Selections are set directly
// and moves go through the right click's own path (ATCPlayerController::IssueMoveAt); keys go through the engine's input
// (ATCPlayerController::InjectKey). None of this is a physical mouse or keyboard, and none of it is a player.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCObjectives.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "HAL/PlatformTime.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "UnrealClient.h"

namespace TCSlice9Local
{
	FString PointRows(const FTCMatchState& S)
	{
		TArray<FString> Parts;
		for (const FTCPointRow& P : S.Points)
			Parts.Add(FString::Printf(TEXT("[%d,%d,%s,%d,%d,%d]"), P.Owner, P.Capper, *FTCObjectives::JsNumber(P.Progress), P.bContested ? 1 : 0, P.bCut ? 1 : 0, P.bLocked ? 1 : 0));
		return FString::Join(Parts, TEXT(","));
	}
	FString IntList(const TArray<int32>& L)
	{
		TArray<FString> Parts;
		for (int32 V : L) Parts.Add(FString::FromInt(V));
		return FString::Join(Parts, TEXT(","));
	}
}

void ATCGameMode::LogObjectives(bool bForce)
{
	if (!bObjectivesLog || !State.bHasMatch) return;
	// once per game second (snapshots arrive every few ticks), and on every owner change
	int32 Owners = 0;
	for (const FTCPointRow& P : State.Points) Owners = Owners * 31 + P.Owner + 2;
	const double Second = FMath::FloorToDouble(State.Tick / 20.0);
	if (!bForce && Second == LastObjectivesLogSecond && Owners == LastLoggedOwners) return;
	const bool bFlip = LastLoggedOwners != INT32_MIN && Owners != LastLoggedOwners;
	LastObjectivesLogSecond = Second;
	LastLoggedOwners = Owners;
	const FTCScoreStrip Strip = State.Objectives.ScoreStrip(State, &UnitTable);
	TArray<FString> Teams;
	for (const FTCScoreTeam& T : Strip.Teams) Teams.Add(FString::Printf(TEXT("team %d %s %s"), T.Team, *T.Num, *T.U));
	UE_LOG(LogTC, Log, TEXT("objectives: tick %d%s vp [%s] points [%s] winner %d%s | strip: %s | line: %s%s%s"), State.Tick, bFlip ? TEXT(" (owner change)") : TEXT(""),
		*TCSlice9Local::IntList(State.Vp), *TCSlice9Local::PointRows(State), State.Winner, State.End.bPresent ? *FString::Printf(TEXT(" end %s (%.1f, %.1f)"), *State.End.Reason, State.End.X, State.End.Z) : TEXT(""),
		*FString::Join(Teams, TEXT("; ")), *Strip.Rate, Strip.Bonus.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" | %s"), *Strip.Bonus), Strip.Notice.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" | notice: %s"), *Strip.Notice));
}

void ATCGameMode::RunSlice9Switches(double Now)
{
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!PC) return;
	const TCHAR* Cmd = FCommandLine::Get();

	// -TCCapturePlay: the own ground army in groups to the points this team does not own (nearest first). The optional
	// -TCCapturePlayAfter=S and -TCCapturePlayFor=S bound when it acts (seconds into the match).
	if (bCapturePlay && State.CanOrder() && State.Snapshots >= 3 && MatchSeenAt >= 0 && Now >= CapturePlayNext)
	{
		double After = 0, For = -1;
		FParse::Value(Cmd, TEXT("TCCapturePlayAfter="), After);
		FParse::Value(Cmd, TEXT("TCCapturePlayFor="), For);
		const double Into = Now - MatchSeenAt;
		CapturePlayNext = Now + 4.0;
		if (Into >= After && (For < 0 || Into < After + For))
		{
			double CX, CZ;
			TArray<int32> Army = ArmyIds(CX, CZ);
			const int32 Mine = State.TeamOf(State.You);
			TArray<const FTCCapturePoint*> Targets;
			for (const FTCCapturePoint& P : State.Objectives.Points())
				if (!(P.bHasRow && P.Owner >= 0 && State.TeamOf(P.Owner) == Mine)) Targets.Add(&P);
			const FVector2D From(CX, CZ);
			Targets.Sort([From](const FTCCapturePoint& A, const FTCCapturePoint& B) { return FVector2D::Distance(A.Centre, From) < FVector2D::Distance(B.Centre, From); });
			const int32 Groups = FMath::Min3(Targets.Num(), Army.Num(), 3);
			Army.Sort();
			const TSet<int32> Kept = PC->Selected; // the selection a screenshot or key check made stays
			TArray<FString> Sent;
			for (int32 G = 0; G < Groups; G++)
			{
				TArray<int32> Ids;
				for (int32 I = G; I < Army.Num(); I += Groups) Ids.Add(Army[I]);
				PC->SelectOwn(Ids);
				const int32 N = PC->IssueMoveAt(Targets[G]->Centre);
				Sent.Add(FString::Printf(TEXT("%d of %d to point %d (%.0f, %.0f)"), N, Ids.Num(), Targets[G]->RowIndex, Targets[G]->Centre.X, Targets[G]->Centre.Y));
			}
			PC->Selected = Kept;
			if (Groups) UE_LOG(LogTC, Log, TEXT("capture play at tick %d (+%.0f s): %s"), State.Tick, Into, *FString::Join(Sent, TEXT("; ")));
		}
	}

	// -TCPointShotAfter: the camera on a capture point, then a screenshot
	if (PointShotAfter >= 0 && !bPointShot && MatchSeenAt >= 0 && Now - MatchSeenAt >= PointShotAfter && State.Snapshots > 0 && State.Objectives.Points().Num())
	{
		const FTCCapturePoint* Pick = nullptr;
		const int32 Mine = State.TeamOf(State.You);
		for (const FTCCapturePoint& P : State.Objectives.Points()) if (P.bArcVisible) { Pick = &P; break; }
		if (!Pick) for (const FTCCapturePoint& P : State.Objectives.Points()) if (P.Owner >= 0 && State.TeamOf(P.Owner) == Mine) { Pick = &P; break; }
		if (!Pick)
			for (const FTCCapturePoint& P : State.Objectives.Points())
				if (!Pick || FVector2D::Distance(P.Centre, PC->ViewFocus()) < FVector2D::Distance(Pick->Centre, PC->ViewFocus())) Pick = &P;
		bPointShot = true;
		PC->FocusOn(Pick->Centre);
		PC->ZoomTo(45.f);
		PointShotAt = Now + 1.5;
		UE_LOG(LogTC, Log, TEXT("point shot: camera on point %d at (%.0f, %.0f): owner %d capper %d progress %s%s%s, label '%s'"), Pick->RowIndex, Pick->Centre.X, Pick->Centre.Y, Pick->Owner, Pick->Capper,
			*FTCObjectives::JsNumber(Pick->Progress), Pick->bArcVisible ? *FString::Printf(TEXT(", arc %d of 64"), Pick->ArcSegments) : TEXT(""), Pick->bContested ? TEXT(", contested") : TEXT(""), *Pick->Label);
	}
	if (PointShotAt > 0 && Now >= PointShotAt)
	{
		PointShotAt = -1;
		const FString File = PointShotFile.IsEmpty() ? FString(TEXT("tc-point.png")) : PointShotFile;
		FScreenshotRequest::RequestScreenshot(File, true, false);
		UE_LOG(LogTC, Log, TEXT("point shot: screenshot requested at tick %d: %s"), State.Tick, *File);
		LogObjectives(true);
	}

	// -TCPlanShotAfter: the own army selected (directly) and the camera on it, so the plan lines and bars are drawn, then a
	// screenshot to -TCPlanShotFile 1.5 s later; the selection is cleared after it
	double PlanShotAfter = -1;
	if (FParse::Value(Cmd, TEXT("TCPlanShotAfter="), PlanShotAfter) && PlanShotAt == 0 && MatchSeenAt >= 0 && Now - MatchSeenAt >= PlanShotAfter && State.CanOrder())
	{
		double CX, CZ;
		const TArray<int32> Army = ArmyIds(CX, CZ);
		if (Army.Num())
		{
			PC->SelectOwn(Army);
			PC->FocusOn(FVector2D(CX, CZ));
			PC->ZoomTo(50.f);
			PlanShotAt = Now + 1.5;
			int32 WithPlan = 0, Waiting = 0;
			for (int32 Id : Army) { WithPlan += PlanKinds.Contains(Id) && PlanKinds[Id] > 0; for (const FTCWaitingOrders& W : State.WaitingOrders) Waiting += W.Id == Id ? W.Orders.Num() : 0; }
			UE_LOG(LogTC, Log, TEXT("plan shot: %d own units selected (directly) at (%.0f, %.0f), %d with a plan row of a kind above 0, %d waiting orders; %d plan rows held in all"), Army.Num(), CX, CZ, WithPlan, Waiting, State.Plans.Num());
		}
	}
	if (PlanShotAt > 0 && Now >= PlanShotAt)
	{
		PlanShotAt = -1;
		FString File;
		if (!FParse::Value(Cmd, TEXT("TCPlanShotFile="), File)) File = TEXT("tc-plans.png");
		FScreenshotRequest::RequestScreenshot(File, true, false);
		UE_LOG(LogTC, Log, TEXT("plan shot: screenshot requested at tick %d: %s"), State.Tick, *File);
		PlanClearAt = Now + 1.0;
	}
	if (PlanClearAt > 0 && Now >= PlanClearAt) { PlanClearAt = -1; PC->Selected.Reset(); UE_LOG(LogTC, Log, TEXT("plan shot: selection cleared")); }

	// -TCEpilogueShotFile: the stamp, 1.2 s after it appeared
	if (!EpilogueShotFile.IsEmpty() && !bEpilogueShot && EpilogueSeenAt >= 0 && Now - EpilogueSeenAt >= 1.2 && State.bHasMatch)
	{
		bEpilogueShot = true;
		FScreenshotRequest::RequestScreenshot(EpilogueShotFile, true, false);
		UE_LOG(LogTC, Log, TEXT("epilogue shot: screenshot requested at tick %d: %s ('%s', '%s')"), State.Tick, *EpilogueShotFile, *State.Objectives.Epilogue().Word, *State.Objectives.Epilogue().Why);
	}

	// -TCResultShotFile: the lobby's result card after a decided match
	const bool bResultLobby = !State.bHasMatch && State.RoomState == TEXT("lobby") && State.LastMatch.bPresent;
	if (bResultLobby && ResultLobbyAt < 0)
	{
		ResultLobbyAt = Now;
		const FTCMatchReport& R = State.Objectives.Report();
		TArray<FString> Legend, Rows;
		for (const FTCReportSide& Sd : R.Sides) Legend.Add(Sd.Legend);
		for (const FTCReportRow& Rw : R.Rows) Rows.Add(FString::Printf(TEXT("%s%s %d/%d/%d/%d/%d/%d/%d"), *Rw.Name, Rw.bMe ? TEXT(" (you)") : TEXT(""), Rw.Values[0], Rw.Values[1], Rw.Values[2], Rw.Values[3], Rw.Values[4], Rw.Values[5], Rw.Values[6]));
		UE_LOG(LogTC, Log, TEXT("result lobby: line '%s', winner %d, reason %s, names [%s], you %s; report %s: '%s' | %s | rows %s"), *State.LastResult, State.LastMatch.Winner, *State.LastMatch.Reason,
			*FString::Join(State.LastMatch.Names, TEXT(", ")), *State.LastMatch.YouOutcome, R.bShown ? TEXT("shown") : TEXT("hidden"), *R.Sub, *FString::Join(Legend, TEXT("; ")), *FString::Join(Rows, TEXT("; ")));
	}
	if (!bResultLobby) ResultLobbyAt = -1;
	if (!ResultShotFile.IsEmpty() && !bResultShot && ResultLobbyAt >= 0 && Now - ResultLobbyAt >= 1.5)
	{
		bResultShot = true;
		FScreenshotRequest::RequestScreenshot(ResultShotFile, true, false);
		UE_LOG(LogTC, Log, TEXT("result shot: screenshot requested: %s"), *ResultShotFile);
	}
	double ExitAfterResult = -1;
	if (ResultLobbyAt >= 0 && FParse::Value(Cmd, TEXT("TCExitAfterResult="), ExitAfterResult) && Now - ResultLobbyAt >= ExitAfterResult)
	{
		UE_LOG(LogTC, Log, TEXT("exit after result: quitting"));
		ResultLobbyAt = -1e30; // once
		FPlatformMisc::RequestExit(false, TEXT("TCExitAfterResult"));
	}

	// -TCKeyCheck: the gap 13 keys through simulated key events, each followed by a report of what it did
	double KeyCheckAfter = 0;
	FParse::Value(Cmd, TEXT("TCKeyCheckAfter="), KeyCheckAfter);
	// -TCKeyCheckOnAlert: also wait for an alert raised in the last second, so Space has one on screen to visit
	const bool bAlertNow = !FParse::Param(Cmd, TEXT("TCKeyCheckOnAlert")) || (Alerts.Num() && GetWorld()->GetTimeSeconds() - Alerts[0].Born < 1.0);
	if (bKeyCheck && !bKeyScriptMade && State.CanOrder() && State.Snapshots >= 5 && MatchSeenAt >= 0 && Now - MatchSeenAt >= KeyCheckAfter && bAlertNow)
	{
		bKeyScriptMade = true;
		auto Key = [PC](const FKey& K, bool bDown) { PC->InjectKey(K, bDown); };
		const double T0 = ScriptT0 < 0 ? 0.0 : FMath::Max(Now - ScriptT0, Script.Num() ? Script.Last().At + 0.5 : 0.0);
		double T = T0;
		auto Report = [this, PC](const TCHAR* What)
		{
			UE_LOG(LogTC, Log, TEXT("key check: after %s: %d selected, focus (%.1f, %.1f) at %.1f m, follow '%s', sheet %s, alert history %s, note '%s'"), What, PC->Selected.Num(),
				PC->ViewFocus().X, PC->ViewFocus().Y, PC->ViewDistance(), *PC->FollowChip(), PC->bControlsSheetOpen ? TEXT("open") : TEXT("closed"), bAlertHistoryOpen ? TEXT("open") : TEXT("closed"), *PC->LastOrderNote);
		};
		auto Chord = [&](const FKey& K, const FKey& Mod, const TCHAR* What)
		{
			const bool bMod = Mod.IsValid();
			Script.Add({ T, FString::Printf(TEXT("key check: simulated %s down"), What), [Key, K, Mod, bMod]() { if (bMod) Key(Mod, true); Key(K, true); } });
			Script.Add({ T + 0.1, FString::Printf(TEXT("key check: simulated %s up"), What), [Key, K, Mod, bMod]() { Key(K, false); if (bMod) Key(Mod, false); } });
			// -TCKeyShotPrefix=path: a screenshot with the alert history open (Alt+H) and with the Controls sheet open (F1)
			FString Prefix;
			const FString Shot = FParse::Value(FCommandLine::Get(), TEXT("TCKeyShotPrefix="), Prefix) && (FCString::Strcmp(What, TEXT("Alt+H")) == 0 || FCString::Strcmp(What, TEXT("F1")) == 0)
				? Prefix + (FCString::Strcmp(What, TEXT("F1")) == 0 ? TEXT("-sheet.png") : TEXT("-history.png")) : FString();
			Script.Add({ T + 0.6, FString::Printf(TEXT("key check: report %s"), What), [Report, What, Shot]()
			{
				Report(What);
				if (!Shot.IsEmpty()) { FScreenshotRequest::RequestScreenshot(Shot, true, false); UE_LOG(LogTC, Log, TEXT("key check: screenshot requested: %s"), *Shot); }
			} });
			T += 1.0;
		};
		if (Alerts.Num()) UE_LOG(LogTC, Log, TEXT("key check: newest alert #%d '%s' at (%.1f, %.1f), %.1f s old"), Alerts[0].Id, *Alerts[0].Text, Alerts[0].At.X, Alerts[0].At.Y, GetWorld()->GetTimeSeconds() - Alerts[0].Born);
		Script.Add({ T, TEXT("key check: clear the selection (directly)"), [PC]() { PC->Selected.Reset(); } });
		T += 0.2;
		Chord(EKeys::SpaceBar, FKey(), TEXT("Space"));
		Chord(EKeys::H, EKeys::LeftAlt, TEXT("Alt+H"));
		Chord(EKeys::Left, EKeys::LeftAlt, TEXT("Alt+Left"));
		Chord(EKeys::H, EKeys::LeftAlt, TEXT("Alt+H again"));
		Chord(EKeys::A, EKeys::LeftControl, TEXT("Ctrl+A"));
		Chord(EKeys::Period, FKey(), TEXT("."));
		Chord(EKeys::Period, EKeys::LeftShift, TEXT("Shift+."));
		Chord(EKeys::A, EKeys::LeftControl, TEXT("Ctrl+A again"));
		Chord(EKeys::SpaceBar, EKeys::LeftShift, TEXT("Shift+Space"));
		Chord(EKeys::H, FKey(), TEXT("H"));
		Chord(EKeys::F1, FKey(), TEXT("F1"));
		Chord(EKeys::F1, FKey(), TEXT("F1 again"));
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("key check: script of 12 chords from +%.1f s"), T0);
	}
}
