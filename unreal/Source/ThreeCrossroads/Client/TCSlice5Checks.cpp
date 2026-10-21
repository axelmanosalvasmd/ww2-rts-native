// Slice 1e verification switches of ATCGameMode (see TCGameMode.h): support orders, abilities and fieldworks. They
// run the player's own paths: simulated key events through the engine's input (ATCPlayerController::InjectKey), HUD
// buttons through the HUD's hit test at their drawn centre (PressCommand, RightPressCommand), world clicks through
// the controller's ground pick at a computed pixel (AimClickAt) or the right click's screen pick (CursorAt). Selections
// before a step are set directly. Raw probes past the client's checks are marked as such in the log. None of this is
// a physical mouse or keyboard.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCArmyCommands.h"
#include "World/TCTerrain.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "UnrealClient.h"

namespace
{
	constexpr int32 AutocastFlag = 16384; // shared/sim.js AUTO_FLAG (TCAbilitiesData::AutoFlag)

	TSharedRef<FJsonObject> Raw(const TCHAR* T)
	{
		TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
		O->SetStringField(TEXT("t"), T);
		return O;
	}
	TArray<TSharedPtr<FJsonValue>> IdValues(const TArray<int32>& Ids)
	{
		TArray<TSharedPtr<FJsonValue>> V;
		for (int32 Id : Ids) V.Add(MakeShared<FJsonValueNumber>(Id));
		return V;
	}
}

void ATCGameMode::AdoptAutocast()
{
	// client/autocast.js adopt after each snapshot: an own unit seen for the first time takes the remembered choice
	for (const TSharedRef<FJsonObject>& Msg : Abilities.AdoptAutocast(State))
	{
		bool bOn = false;
		const TArray<TSharedPtr<FJsonValue>>* Ids = nullptr;
		Msg->TryGetBoolField(TEXT("on"), bOn);
		Msg->TryGetArrayField(TEXT("ids"), Ids);
		const bool bSent = State.CanOrder() && SendCommand(Msg);
		UE_LOG(LogTC, Log, TEXT("autocast adopt: %d new unit%s take the remembered %s: %s"), Ids ? Ids->Num() : 0, Ids && Ids->Num() == 1 ? TEXT("") : TEXT("s"), bOn ? TEXT("on") : TEXT("off"),
			bSent ? TEXT("sent") : TEXT("not sent (no orders now)"));
	}
}

void ATCGameMode::RunSlice5Switches(double Now)
{
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!PC || !(bSupportCheck || bAbilityCheck || bFieldCheck || bLetterCheck)) return;
	const bool bReady = State.CanOrder() && State.Snapshots >= 3 && Recruit.HasSnapshot() && Build.HasSnapshot() && Abilities.HasSnapshot() && Field.HasSnapshot();
	if (!bReady) return;
	const FVector2D Centre(State.Map.W * FTCMatchState::CellMetres / 2, State.Map.H * FTCMatchState::CellMetres / 2);
	auto Key = [PC](const FKey& K, bool bDown) { PC->InjectKey(K, bDown); };
	auto Note = [PC]() { return FString::Printf(TEXT("%s note: '%s'"), PC->bLastOrderSent ? TEXT("sent") : TEXT("not sent"), *PC->LastOrderNote); };
	auto Deny = [this]() { return FString::Printf(TEXT("denies %d, last refusal %s/%s ('%s')"), State.Denies, *State.DenyCmd, *State.DenyReason, *TCOrders::DenySentence(State.DenyReason, State.DenyCmd)); };
	auto Chord = [this, Key](double At, const FKey& K, bool bShift, const FString& What)
	{
		Script.Add({ At, FString::Printf(TEXT("simulated %s%s down (%s)"), bShift ? TEXT("Shift+") : TEXT(""), *K.ToString(), *What), [Key, K, bShift]() { if (bShift) Key(EKeys::LeftShift, true); Key(K, true); } });
		Script.Add({ At + 0.1, FString::Printf(TEXT("simulated %s%s up"), bShift ? TEXT("Shift+") : TEXT(""), *K.ToString()), [Key, K, bShift]() { Key(K, false); if (bShift) Key(EKeys::LeftShift, false); } });
	};
	// A left click with an aim armed at a ground point: through the ground pick at the point's pixel when it is on
	// screen, else at the point itself. Logs where the ground pick landed.
	auto ClickGround = [this, PC](FVector2D Target, bool bShift, const TCHAR* What)
	{
		FVector2D Screen, Picked(-1, -1);
		int32 VX = 0, VY = 0;
		PC->GetViewportSize(VX, VY);
		bool bSent;
		const bool bOnScreen = Terrain && PC->ProjectWorldLocationToScreen(ATCTerrain::ToWorld(Target.X, Target.Y, Terrain->HeightAt(Target.X, Target.Y)), Screen) && Screen.X >= 0 && Screen.Y >= 0 && Screen.X < VX && Screen.Y < VY;
		if (bOnScreen) bSent = PC->AimClickAt(Screen, bShift, &Picked);
		else { bSent = PC->AimClick(true, Target, bShift); Picked = Target; }
		UE_LOG(LogTC, Log, TEXT("%s: %sclick for (%.1f, %.1f) %s, ground (%.1f, %.1f): %s; aim now %s; note '%s'"), What, bShift ? TEXT("Shift+") : TEXT(""), Target.X, Target.Y,
			bOnScreen ? *FString::Printf(TEXT("at pixel (%.0f, %.0f) through the ground pick"), Screen.X, Screen.Y) : TEXT("off screen, given directly"), Picked.X, Picked.Y, bSent ? TEXT("sent") : TEXT("nothing sent"),
			LexToString(PC->Aim), *PC->LastOrderNote);
		return bSent;
	};
	// A right click on bare ground: the screen pick at its pixel when on screen, else the ground named directly.
	auto RightClickGround = [this, PC](FVector2D Target, bool bShift, const TCHAR* What)
	{
		FVector2D Screen;
		int32 VX = 0, VY = 0;
		PC->GetViewportSize(VX, VY);
		FTCCursor C;
		const bool bOnScreen = Terrain && PC->ProjectWorldLocationToScreen(ATCTerrain::ToWorld(Target.X, Target.Y, Terrain->HeightAt(Target.X, Target.Y)), Screen) && Screen.X >= 0 && Screen.Y >= 0 && Screen.X < VX && Screen.Y < VY;
		if (bOnScreen) C = PC->CursorAt(Screen);
		if (!C.bHasGround) { C = FTCCursor(); C.bHasGround = true; C.Ground = Target; }
		const int32 N = PC->IssueOrder(C, false, bShift);
		UE_LOG(LogTC, Log, TEXT("%s: right click for (%.1f, %.1f) %s, ground (%.1f, %.1f), friend %d, building %d: %d named; note '%s'"), What, Target.X, Target.Y,
			bOnScreen ? *FString::Printf(TEXT("at pixel (%.0f, %.0f) through the screen pick"), Screen.X, Screen.Y) : TEXT("off screen, given directly"), C.Ground.X, C.Ground.Y, C.Friend, C.Building, N, *PC->LastOrderNote);
		return N;
	};
	auto Toward = [Centre](FVector2D From, double Metres) { return From + (Centre - From).GetSafeNormal() * Metres; };
	auto CentreOf = [this](const TArray<int32>& Ids)
	{
		FVector2D Sum(0, 0); int32 N = 0;
		for (int32 Id : Ids) if (const FTCUnit* U = State.Units.Find(Id)) { Sum += FVector2D(U->X, U->Z); N++; }
		return N ? Sum / N : FVector2D(-1, -1);
	};
	const double T0 = Script.Num() ? Script.Last().At + 0.5 : (ScriptT0 >= 0 ? Now - ScriptT0 : 0.0);

	// -TCAbilityCheck
	if (bAbilityCheck && !bAbilityScriptMade)
	{
		double CX, CZ;
		const TArray<int32> Army = ArmyIds(CX, CZ);
		if (Army.Num() < 2) return;
		bAbilityScriptMade = true;
		auto Buttons = [this, PC]()
		{
			TArray<FString> L;
			for (const FTCAbilityButton& B : Abilities.Buttons(State, UnitTable, PC->Selected))
				L.Add(FString::Printf(TEXT("%s:%s%s%s%s"), *B.Type, *B.AbilityId, B.Key.IsEmpty() ? TEXT("") : TEXT(" [F]"), B.Text.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" '%s'"), *B.Text),
					B.Check.bOk ? TEXT("") : *FString::Printf(TEXT(" refused '%s'"), *B.Check.Reason)));
			return FString::Join(L, TEXT(", "));
		};
		auto Cds = [this, Army]()
		{
			TArray<FString> L;
			for (int32 Id : Army) if (const FTCUnit* U = State.Units.Find(Id)) L.Add(FString::Printf(TEXT("%s %d cd %d%s"), *U->Type, Id, U->Cd, (U->Flags & AutocastFlag) ? TEXT(" auto") : TEXT("")));
			return FString::Printf(TEXT("tick %d: %s"), State.Tick, *FString::Join(L, TEXT(", ")));
		};
		Script.Add({ T0, FString::Printf(TEXT("ability check: select the army of %d (set directly)"), Army.Num()), [PC, Army, Buttons, Cds, this]()
		{
			PC->SelectOwn(Army);
			UE_LOG(LogTC, Log, TEXT("ability check: buttons [%s]; F picks '%s'; %s"), *Buttons(), *Abilities.FKeyType(State, UnitTable, PC->Selected), *Cds());
		} });
		Chord(T0 + 0.3, EKeys::F, false, TEXT("use the ability F picks"));
		Script.Add({ T0 + 0.8, TEXT("ability check: after F"), [PC, Note, ClickGround, Toward, CentreOf, Army]()
		{
			UE_LOG(LogTC, Log, TEXT("ability check: after F: aim %s (%s %s, radius %.1f), hint '%s'; %s"), LexToString(PC->Aim), *PC->AimAbilityType, *PC->AimAbilityId, PC->AimAbilityRadius, *PC->AimHint, *Note());
			if (PC->Aim == ETCAim::Ability) ClickGround(Toward(CentreOf(Army), 10.0), false, TEXT("ability check: the aimed ability's click 10 m toward the centre"));
		} });
		Script.Add({ T0 + 2.5, TEXT("ability check: report"), [Cds, Deny]() { UE_LOG(LogTC, Log, TEXT("ability check: %s; %s"), *Cds(), *Deny()); } });
		// every instant ability in the selection through its Orders button, then the same at once (before the next
		// snapshot shows the cooldown: the server's refusal), then after a snapshot (the client's refusal)
		Script.Add({ T0 + 2.7, TEXT("ability check: press each instant ability's Orders button through the hit test"), [this, PC, Note]()
		{
			for (const FTCAbilityButton& B : Abilities.Buttons(State, UnitTable, PC->Selected))
			{
				if (FTCAbilities::IsAimed(B.AbilityId)) continue;
				FTCUiCommand C(ETCUiAction::Ability); C.Unit = B.Type;
				const bool bOk = PC->PressCommand(C);
				UE_LOG(LogTC, Log, TEXT("ability check: %s button: %s; %s"), *B.Type, bOk ? TEXT("sent") : TEXT("nothing sent"), *Note());
				const bool bAgain = PC->PressCommand(C);
				UE_LOG(LogTC, Log, TEXT("ability check: %s button again in the same frame (no snapshot since): %s; %s"), *B.Type, bAgain ? TEXT("sent") : TEXT("nothing sent"), *Note());
			}
		} });
		Script.Add({ T0 + 4.2, TEXT("ability check: report, press the instant buttons again"), [this, PC, Note, Cds, Deny]()
		{
			UE_LOG(LogTC, Log, TEXT("ability check: %s; %s"), *Cds(), *Deny());
			for (const FTCAbilityButton& B : Abilities.Buttons(State, UnitTable, PC->Selected))
			{
				if (FTCAbilities::IsAimed(B.AbilityId)) continue;
				FTCUiCommand C(ETCUiAction::Ability); C.Unit = B.Type;
				const bool bOk = PC->PressCommand(C);
				UE_LOG(LogTC, Log, TEXT("ability check: %s button once the cooldown is in the snapshot: %s; %s"), *B.Type, bOk ? TEXT("sent") : TEXT("nothing sent"), *Note());
			}
		} });
		Script.Add({ T0 + 4.6, TEXT("ability check: right press the first ability button (autocast)"), [this, PC, Note]()
		{
			const TArray<FTCAbilityButton> B = Abilities.Buttons(State, UnitTable, PC->Selected);
			if (!B.Num()) { UE_LOG(LogTC, Warning, TEXT("ability check: no ability button")); return; }
			FTCUiCommand C(ETCUiAction::Ability); C.Unit = B[0].Type;
			const bool bOk = PC->RightPressCommand(C);
			UE_LOG(LogTC, Log, TEXT("ability check: autocast right press on %s: %s; %s; remembered: %s"), *B[0].Type, bOk ? TEXT("taken") : TEXT("not taken"), *Note(), *Abilities.Autocast.Save());
		} });
		Script.Add({ T0 + 6.2, TEXT("ability check: report the autocast bits, right press it again"), [this, PC, Note, Cds]()
		{
			UE_LOG(LogTC, Log, TEXT("ability check: after autocast: %s"), *Cds());
			const TArray<FTCAbilityButton> B = Abilities.Buttons(State, UnitTable, PC->Selected);
			if (!B.Num()) return;
			FTCUiCommand C(ETCUiAction::Ability); C.Unit = B[0].Type;
			PC->RightPressCommand(C);
			UE_LOG(LogTC, Log, TEXT("ability check: autocast again: %s; remembered: %s"), *Note(), *Abilities.Autocast.Save());
		} });
		Script.Add({ T0 + 7.8, TEXT("ability check: report, clear the selection (directly)"), [PC, Cds]() { UE_LOG(LogTC, Log, TEXT("ability check: after the second autocast: %s"), *Cds()); PC->Selected.Reset(); } });
		Chord(T0 + 8.0, EKeys::F, false, TEXT("F with nothing selected: refused by the client"));
		Script.Add({ T0 + 8.4, TEXT("ability check: report"), [Note]() { UE_LOG(LogTC, Log, TEXT("ability check: F with nothing selected: %s"), *Note()); } });
		Script.Add({ T0 + 8.6, TEXT("ability check: raw probe past the client's check: an ability for the HQ (it has none)"), [this, Deny]()
		{
			const TArray<int32> Hq = OwnOfTypes({ TEXT("hq") });
			if (!Hq.Num()) return;
			TSharedRef<FJsonObject> O = Raw(TEXT("ability"));
			O->SetArrayField(TEXT("ids"), IdValues(Hq));
			UE_LOG(LogTC, Log, TEXT("ability check: raw { t: 'ability', ids: [%d] } (past the client's check): %s; before: %s"), Hq[0], SendCommand(O) ? TEXT("sent") : *LastSendProblemText, *Deny());
		} });
		Script.Add({ T0 + 10.0, TEXT("ability check: report"), [Deny]() { UE_LOG(LogTC, Log, TEXT("ability check: after the raw probe: %s"), *Deny()); } });
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("ability check: script for an army of %d"), Army.Num());
	}

	// -TCSupportCheck: an own carrier is spawned first in a sandbox (client/main.js sandbox spawn)
	if (bSupportCheck && !bSupportScriptMade)
	{
		const TArray<int32> Carriers = OwnOfTypes({ TEXT("halftrack") });
		const TArray<int32> Inf = OwnOfTypes({ TEXT("rifle") });
		if (!Carriers.Num())
		{
			if (!bSupportSiteSent && State.bSandbox && Inf.Num())
			{
				bSupportSiteSent = true;
				const FVector2D At = Toward(CentreOf(Inf), 8.0);
				TSharedRef<FJsonObject> Sp = Raw(TEXT("spawn"));
				Sp->SetStringField(TEXT("unit"), TEXT("halftrack")); Sp->SetNumberField(TEXT("x"), At.X); Sp->SetNumberField(TEXT("z"), At.Y);
				UE_LOG(LogTC, Log, TEXT("support check: sandbox spawn of an own halftrack at (%.1f, %.1f): %s"), At.X, At.Y, SendCommand(Sp) ? TEXT("sent") : *LastSendProblemText);
			}
			return;
		}
		if (Inf.Num() < 2) return;
		bSupportScriptMade = true;
		const int32 Carrier = Carriers[0], Rifle = Inf[0], Rifle2 = Inf[1];
		auto Cargo = [this, Carrier, Rifle, Rifle2]()
		{
			const FTCUnit* C = State.Units.Find(Carrier);
			const FTCUnit* R1 = State.Units.Find(Rifle);
			const FTCUnit* R2 = State.Units.Find(Rifle2);
			return FString::Printf(TEXT("tick %d: halftrack %d carries %d; rifle %d %s, rifle %d %s"), State.Tick, Carrier, C ? TCCargo::Count(C->Flags) : -1,
				Rifle, R1 ? ((R1->Flags & TCFlags::Riding) ? TEXT("riding") : TEXT("on foot")) : TEXT("not held (riding squads leave the rows)"),
				Rifle2, R2 ? ((R2->Flags & TCFlags::Riding) ? TEXT("riding") : TEXT("on foot")) : TEXT("not held"));
		};
		auto Buttons = [this, PC]()
		{
			const FTCCarrierButton A = Support.MountUpButton(State, UnitTable, PC->Selected), B = Support.UnloadButton(State, UnitTable, PC->Selected);
			return FString::Printf(TEXT("Mount up %s '%s'%s, Unload %s '%s'%s"), A.bShown ? TEXT("shown") : TEXT("hidden"), *A.Text, A.bOk ? TEXT("") : *FString::Printf(TEXT(" ('%s')"), *A.Reason),
				B.bShown ? TEXT("shown") : TEXT("hidden"), *B.Text, B.bOk ? TEXT("") : *FString::Printf(TEXT(" ('%s')"), *B.Reason));
		};
		Script.Add({ T0, TEXT("support check: select a rifle squad and the halftrack (directly)"), [PC, Rifle, Carrier, Cargo, Buttons]()
		{
			PC->SelectOwn({ Rifle, Carrier });
			UE_LOG(LogTC, Log, TEXT("support check: %s; buttons: %s; camera yaw %.2f"), *Cargo(), *Buttons(), PC->ViewYaw());
		} });
		Chord(T0 + 0.3, EKeys::Q, true, TEXT("mount up"));
		Script.Add({ T0 + 0.6, TEXT("support check: note"), [Note, PC]() { UE_LOG(LogTC, Log, TEXT("support check: after Shift+Q: %s; camera yaw %.2f"), *Note(), PC->ViewYaw()); } });
		for (int32 I = 1; I <= 3; I++) Script.Add({ T0 + 0.6 + 2.0 * I, TEXT("support check: report"), [Cargo, Deny]() { UE_LOG(LogTC, Log, TEXT("support check: %s; %s"), *Cargo(), *Deny()); } });
		Script.Add({ T0 + 6.8, TEXT("support check: select the halftrack (directly)"), [PC, Carrier]() { PC->SelectOwn({ Carrier }); } });
		Script.Add({ T0 + 7.0, TEXT("support check: press Unload through the hit test"), [PC, Note, Buttons]()
		{
			UE_LOG(LogTC, Log, TEXT("support check: buttons: %s"), *Buttons());
			PC->PressCommand(FTCUiCommand(ETCUiAction::Unload));
			UE_LOG(LogTC, Log, TEXT("support check: Unload button: %s"), *Note());
		} });
		for (int32 I = 1; I <= 2; I++) Script.Add({ T0 + 7.0 + 2.0 * I, TEXT("support check: report"), [Cargo, Deny]() { UE_LOG(LogTC, Log, TEXT("support check: %s; %s"), *Cargo(), *Deny()); } });
		Script.Add({ T0 + 11.5, TEXT("support check: select the second rifle squad (directly), right click the halftrack (board)"), [this, PC, Rifle2, Carrier, Note]()
		{
			PC->SelectOwn({ Rifle2 });
			RightClickUnit(Carrier, TEXT("support check: board"));
			UE_LOG(LogTC, Log, TEXT("support check: board: %s"), *Note());
		} });
		for (int32 I = 1; I <= 3; I++) Script.Add({ T0 + 11.5 + 2.0 * I, TEXT("support check: report"), [Cargo, Deny]() { UE_LOG(LogTC, Log, TEXT("support check: %s; %s"), *Cargo(), *Deny()); } });
		Script.Add({ T0 + 18.0, TEXT("support check: select the halftrack (directly)"), [PC, Carrier]() { PC->SelectOwn({ Carrier }); } });
		Chord(T0 + 18.2, EKeys::E, true, TEXT("unload"));
		Script.Add({ T0 + 18.6, TEXT("support check: note"), [Note, PC]() { UE_LOG(LogTC, Log, TEXT("support check: after Shift+E: %s; camera yaw %.2f"), *Note(), PC->ViewYaw()); } });
		Script.Add({ T0 + 21.0, TEXT("support check: report, select only the HQ (directly)"), [this, PC, Cargo, Deny]()
		{
			UE_LOG(LogTC, Log, TEXT("support check: %s; %s"), *Cargo(), *Deny());
			const TArray<int32> Hq = OwnOfTypes({ TEXT("hq") });
			if (Hq.Num()) PC->SelectOwn({ Hq[0] });
		} });
		Chord(T0 + 21.2, EKeys::Q, true, TEXT("mount up with only the HQ: the browser sends it, the server refuses"));
		Chord(T0 + 21.6, EKeys::E, true, TEXT("unload with only the HQ: nothing to send"));
		Script.Add({ T0 + 23.0, TEXT("support check: report"), [Note, Deny]() { UE_LOG(LogTC, Log, TEXT("support check: after Shift+Q and Shift+E with the HQ: %s; %s"), *Note(), *Deny()); } });
		// assist and cancel: a site placed by one rifle squad, the other right-clicks it, then the site card's Cancel
		const FString SiteKind = SupportBuild.IsEmpty() ? FString(TEXT("supplycache")) : SupportBuild;
		Script.Add({ T0 + 23.2, FString::Printf(TEXT("support check: place a %s site with one rifle squad"), *SiteKind), [this, PC, Rifle, SiteKind, CentreOf, Note]()
		{
			PC->SelectOwn({ Rifle });
			FVector2D At;
			if (!FindPlacement(SiteKind, CentreOf({ Rifle }), At)) { UE_LOG(LogTC, Warning, TEXT("support check: no accepted spot for %s"), *SiteKind); return; }
			FTCUiCommand C(ETCUiAction::Build); C.Unit = SiteKind;
			PC->RunCommand(C);
			SupportSite = At;
			const bool bSent = PC->AimClick(true, At, false);
			SupportSiteAt = Recruit.Mp();
			UE_LOG(LogTC, Log, TEXT("support check: build %s at (%.1f, %.1f): %s; %s; MP %.0f"), *SiteKind, At.X, At.Y, bSent ? TEXT("sent") : TEXT("nothing sent"), *Note(), Recruit.Mp());
		} });
		auto SiteId = [this, SiteKind]()
		{
			int32 Best = 0; double D = 1e9;
			for (int32 Id : OwnOfTypes({ SiteKind })) if (const FTCUnit* U = State.Units.Find(Id)) { const double Dd = FVector2D::Distance(FVector2D(U->X, U->Z), SupportSite); if (Dd < D) { D = Dd; Best = Id; } }
			return D < 6 ? Best : 0;
		};
		Script.Add({ T0 + 25.5, TEXT("support check: select the second rifle squad (directly), right click the site (assist)"), [this, PC, Rifle2, SiteId, Note]()
		{
			const int32 Site = SiteId();
			const FTCUnit* U = Site ? State.Units.Find(Site) : nullptr;
			if (!U) { UE_LOG(LogTC, Warning, TEXT("support check: no site held near (%.1f, %.1f)"), SupportSite.X, SupportSite.Y); return; }
			UE_LOG(LogTC, Log, TEXT("support check: site %d built %.6f (exact %.17g)"), Site, U->Built, Support.BuiltOf(*U));
			PC->SelectOwn({ Rifle2 });
			RightClickUnit(Site, TEXT("support check: assist"));
			UE_LOG(LogTC, Log, TEXT("support check: assist: %s"), *Note());
		} });
		Script.Add({ T0 + 28.0, TEXT("support check: report, select the site (directly)"), [this, PC, SiteId, Deny]()
		{
			const int32 Site = SiteId();
			const FTCUnit* U = Site ? State.Units.Find(Site) : nullptr;
			if (!U) { UE_LOG(LogTC, Warning, TEXT("support check: the site is gone")); return; }
			UE_LOG(LogTC, Log, TEXT("support check: site %d built %.17g after the assist; %s"), Site, Support.BuiltOf(*U), *Deny());
			PC->SelectOwn({ Site });
		} });
		Script.Add({ T0 + 28.3, TEXT("support check: press the site's Cancel through the hit test"), [this, PC, Note]()
		{
			const FTCSiteCard Card = Support.SiteCard(State, UnitTable, PC->Selected, Recruit.Classic());
			UE_LOG(LogTC, Log, TEXT("support check: site card: %s, %s card, cancel '%s', status '%s'"), Card.bShown ? TEXT("shown") : TEXT("hidden"), Card.bClassicCard ? TEXT("Classic") : TEXT("facility"), *Card.CancelText, *Card.Status);
			SupportSiteAt = Recruit.Mp();
			PC->PressCommand(FTCUiCommand(ETCUiAction::CancelSite));
			UE_LOG(LogTC, Log, TEXT("support check: Cancel: %s; selected now %d"), *Note(), PC->Selected.Num());
		} });
		Script.Add({ T0 + 30.0, TEXT("support check: report, raw cancel of the HQ past the client's check"), [this, SiteId, Deny]()
		{
			UE_LOG(LogTC, Log, TEXT("support check: after Cancel: site %s, MP %.0f (was %.0f); %s"), SiteId() ? TEXT("still held") : TEXT("gone"), Recruit.Mp(), SupportSiteAt, *Deny());
			const TArray<int32> Hq = OwnOfTypes({ TEXT("hq") });
			if (!Hq.Num()) return;
			TSharedRef<FJsonObject> O = Raw(TEXT("cancel"));
			O->SetNumberField(TEXT("id"), Hq[0]);
			UE_LOG(LogTC, Log, TEXT("support check: raw { t: 'cancel', id: %d } (a finished building, past the client's check): %s"), Hq[0], SendCommand(O) ? TEXT("sent") : *LastSendProblemText);
		} });
		Script.Add({ T0 + 31.5, TEXT("support check: report"), [Deny]() { UE_LOG(LogTC, Log, TEXT("support check: after the raw cancel: %s"), *Deny()); } });
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("support check: script with halftrack %d, rifle squads %d and %d"), Carrier, Rifle, Rifle2);
	}

	// -TCLetterCheck: Classic, one finished own Production Building selected (the HQ): its card letters outrank the base
	// table (client/keys.js 'building' context), so Q buys card 1 and does not turn the camera, Shift+E buys five of
	// card 3 when there is one, else it is Unload (nothing to send for an HQ)
	if (bLetterCheck && !bLetterScriptMade && TCArmyCommands::ClassicLobby(State))
	{
		const TArray<int32> Hq = OwnOfTypes({ TEXT("hq") });
		const FTCUnit* H = Hq.Num() ? State.Units.Find(Hq[0]) : nullptr;
		if (!H || Support.BuiltOf(*H) < 1) return;
		bLetterScriptMade = true;
		auto Queue = [this, Hq, PC]()
		{
			const FTCProductionQueue* Q = Recruit.QueueOf(Hq[0]);
			return FString::Printf(TEXT("MP %.0f, HQ queue %d, camera yaw %.2f, %d lettered cards"), Recruit.Mp(), Q ? Q->Types.Num() : 0, PC->ViewYaw(), PC->LetteredCards().Num());
		};
		Script.Add({ T0, TEXT("letter check: select the HQ (directly)"), [PC, Hq]() { PC->SelectOwn({ Hq[0] }); } });
		Script.Add({ T0 + 0.3, TEXT("letter check: report the cards"), [PC, Queue]()
		{
			TArray<FString> L;
			for (const FTCUiCommand& C : PC->LetteredCards()) L.Add(C.Describe());
			UE_LOG(LogTC, Log, TEXT("letter check: cards [%s]; %s"), *FString::Join(L, TEXT(", ")), *Queue());
		} });
		Chord(T0 + 0.5, EKeys::Q, false, TEXT("card 1"));
		Script.Add({ T0 + 1.2, TEXT("letter check: report"), [Note, Queue]() { UE_LOG(LogTC, Log, TEXT("letter check: after Q: %s; %s"), *Note(), *Queue()); } });
		Chord(T0 + 1.4, EKeys::W, false, TEXT("card 2"));
		Script.Add({ T0 + 2.1, TEXT("letter check: report"), [Note, Queue]() { UE_LOG(LogTC, Log, TEXT("letter check: after W: %s; %s"), *Note(), *Queue()); } });
		Chord(T0 + 2.3, EKeys::E, true, TEXT("Shift+E: no card 3 on an HQ, so the base table's Unload"));
		Script.Add({ T0 + 3.0, TEXT("letter check: report"), [Note, Queue]() { UE_LOG(LogTC, Log, TEXT("letter check: after Shift+E: %s; %s"), *Note(), *Queue()); } });
		if (ScriptT0 < 0) ScriptT0 = Now;
	}

	// -TCFieldCheck
	if (bFieldCheck && !bFieldScriptMade)
	{
		const TArray<int32> Diggers = OwnOfTypes(FTCFieldworks::FortBuilders());
		if (Diggers.Num() < 2) return;
		bFieldScriptMade = true;
		const FVector2D Home = CentreOf(Diggers);
		const FVector2D Far = Home + (Centre - Home) * 0.95; // near the map's centre: never seen yet
		auto Works = [this]()
		{
			TArray<FString> L;
			for (const FTCWork& W : Field.Works()) L.Add(FString::Printf(TEXT("%s%s at (%.1f, %.1f)"), *W.Kind, W.Project >= 0 ? *FString::Printf(TEXT(" of pattern %d"), W.Project) : TEXT(""), W.X, W.Z));
			return FString::Printf(TEXT("%d works [%s]"), L.Num(), *FString::Join(L, TEXT("; ")));
		};
		auto Digs = [this, Diggers]()
		{
			TArray<FString> L;
			for (int32 Id : Diggers) if (const FTCUnit* U = State.Units.Find(Id)) { const FTCDigStatus D = FTCFieldworks::DigStatus(*U); L.Add(FString::Printf(TEXT("%d '%s'%s"), Id, *D.Tag, D.bDigging ? *FString::Printf(TEXT(" %.0f%%"), D.Progress * 100.f) : TEXT(""))); }
			return FString::Printf(TEXT("tick %d, MP %.0f: %s"), State.Tick, Recruit.Mp(), *FString::Join(L, TEXT(", ")));
		};
		Script.Add({ T0, FString::Printf(TEXT("field check: select the %d builder squads (set directly)"), Diggers.Num()), [this, PC, Diggers, Works]()
		{
			PC->SelectOwn(Diggers);
			TArray<FString> L;
			for (const FTCFieldButton& B : Field.FortMenu(State, PC->Selected).Buttons) L.Add(FString::Printf(TEXT("%s%s"), *B.Kind, B.Check.bOk ? TEXT("") : *FString::Printf(TEXT(" ('%s')"), *B.Check.Reason)));
			UE_LOG(LogTC, Log, TEXT("field check: menus %s; fort buttons [%s]; %s"), Field.OffersFieldMenus(PC->Selected) ? TEXT("offered") : TEXT("not offered"), *FString::Join(L, TEXT(", ")), *Works());
		} });
		Chord(T0 + 0.3, EKeys::T, false, TEXT("arm the trench"));
		Script.Add({ T0 + 0.7, TEXT("field check: a dig far toward the centre (unseen): two clicks"), [PC, ClickGround, Far]()
		{
			UE_LOG(LogTC, Log, TEXT("field check: after T: aim %s, hint '%s'"), LexToString(PC->Aim), *PC->AimHint);
			ClickGround(Far, false, TEXT("field check: unseen dig, first click"));
			ClickGround(Far + FVector2D(4, 0), false, TEXT("field check: unseen dig, second click"));
		} });
		Script.Add({ T0 + 1.2, TEXT("field check: cancel the aim (Esc), arm the trench again (T)"), [Key]() { Key(EKeys::Escape, true); Key(EKeys::Escape, false); } });
		Chord(T0 + 1.4, EKeys::T, false, TEXT("arm the trench again"));
		Script.Add({ T0 + 1.8, TEXT("field check: a dig 8 m from the builders toward the centre: two clicks"), [PC, ClickGround, Toward, Home, Note]()
		{
			const FVector2D A = Toward(Home, 8.0);
			ClickGround(A, false, TEXT("field check: dig, first click (the centre)"));
			UE_LOG(LogTC, Log, TEXT("field check: preview after the first click: marker %s at (%.1f, %.1f), %d cells, %s"), PC->FieldPreview.bMarkerPlaced ? TEXT("placed") : TEXT("not placed"),
				PC->FieldPreview.MarkerAt.X, PC->FieldPreview.MarkerAt.Y, PC->FieldPreview.Placement.Cells.Num(), PC->FieldPreview.Placement.bOk ? TEXT("ok") : *PC->FieldPreview.Placement.Reason);
			ClickGround(A + (A - Home).GetSafeNormal().GetRotated(90.0) * 4.0, false, TEXT("field check: dig, second click (the direction)"));
			UE_LOG(LogTC, Log, TEXT("field check: dig: %s"), *Note());
		} });
		Script.Add({ T0 + 4.0, TEXT("field check: report"), [Works, Digs, Deny]() { UE_LOG(LogTC, Log, TEXT("field check: %s; %s; %s"), *Works(), *Digs(), *Deny()); } });
		Chord(T0 + 4.2, EKeys::T, true, TEXT("arm a trench line"));
		Script.Add({ T0 + 4.6, TEXT("field check: a trench line across the approach 14 m out: two clicks"), [this, PC, ClickGround, Toward, Home, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("field check: after Shift+T: aim %s, hint '%s'"), LexToString(PC->Aim), *PC->AimHint);
			const FVector2D Mid = Toward(Home, 14.0), Side = (Mid - Home).GetSafeNormal().GetRotated(90.0);
			ClickGround(Mid - Side * 5.0, false, TEXT("field check: line, first click"));
			// the preview the browser draws before the second click, with the pointer held at its end
			PC->PinPointerGround(Mid + Side * 5.0);
			if (!FieldShotFile.IsEmpty()) { FScreenshotRequest::RequestScreenshot(FieldShotFile, true, false); UE_LOG(LogTC, Log, TEXT("field check: screenshot of the armed line preview requested: %s"), *FieldShotFile); }
		} });
		Script.Add({ T0 + 5.6, TEXT("field check: the line's second click"), [PC, ClickGround, Toward, Home, Note]()
		{
			int32 Cells = 0;
			for (const FTCEntrenchPiece& P : PC->FieldPreview.Pieces) Cells += P.Place.Cells.Num();
			UE_LOG(LogTC, Log, TEXT("field check: line preview: %d segments, %d cells; hint '%s'"), PC->FieldPreview.Pieces.Num(), Cells, *PC->AimHint);
			PC->ReleasePointerGround();
			const FVector2D Mid = Toward(Home, 14.0), Side = (Mid - Home).GetSafeNormal().GetRotated(90.0);
			ClickGround(Mid + Side * 5.0, false, TEXT("field check: line, second click"));
			UE_LOG(LogTC, Log, TEXT("field check: line: %s"), *Note());
		} });
		Script.Add({ T0 + 8.0, TEXT("field check: report, open the Trench menu through the hit test"), [this, PC, Works, Digs, Deny]()
		{
			UE_LOG(LogTC, Log, TEXT("field check: %s; %s; %s"), *Works(), *Digs(), *Deny());
			PC->PressCommand(FTCUiCommand(ETCUiAction::TrenchMenu));
			TArray<FString> L;
			for (const FTCFieldButton& B : Field.TrenchMenu(State, PC->Selected).Buttons) L.Add(FString::Printf(TEXT("%s %s%s"), *B.Kind, *B.Value, B.Check.bOk ? TEXT("") : *FString::Printf(TEXT(" ('%s')"), *B.Check.Reason)));
			UE_LOG(LogTC, Log, TEXT("field check: Trench menu %s: [%s]"), PC->bTrenchMenuOpen ? TEXT("open") : TEXT("closed"), *FString::Join(L, TEXT(", ")));
		} });
		// a Ring dug by one squad: more segments than diggers, so the pattern keeps segments to hand out (a join target)
		Script.Add({ T0 + 8.4, TEXT("field check: one builder squad (directly); press the Ring pattern through the hit test, two clicks 12 m out on the other side"), [PC, Diggers, ClickGround, Toward, Home]()
		{
			PC->SelectOwn({ Diggers[0] });
			FTCUiCommand C(ETCUiAction::Entrench); C.Unit = TEXT("ring");
			PC->PressCommand(C);
			UE_LOG(LogTC, Log, TEXT("field check: after Ring: aim %s, hint '%s'"), LexToString(PC->Aim), *PC->AimHint);
			const FVector2D Mid = Toward(Home, -12.0), Side = (Mid - Home).GetSafeNormal().GetRotated(90.0);
			ClickGround(Mid, false, TEXT("field check: ring, first click (the centre)"));
			ClickGround(Mid + Side * 6.0, false, TEXT("field check: ring, second click (the radius)"));
		} });
		Script.Add({ T0 + 11.0, TEXT("field check: report"), [Works, Digs, Deny]() { UE_LOG(LogTC, Log, TEXT("field check: %s; %s; %s"), *Works(), *Digs(), *Deny()); } });
		Script.Add({ T0 + 11.2, TEXT("field check: one builder squad (directly) right-clicks a planned pattern's segment (join)"), [this, PC, Diggers, RightClickGround, Note]()
		{
			for (const FTCWork& W : Field.Works())
				if (W.Project >= 0)
				{
					PC->SelectOwn({ Diggers.Last() });
					int32 Project = -1;
					const bool bAt = Field.WorksAt(FVector2D(W.X, W.Z), Project);
					UE_LOG(LogTC, Log, TEXT("field check: works at (%.1f, %.1f): %s"), W.X, W.Z, bAt ? *FString::Printf(TEXT("pattern %d"), Project) : TEXT("none"));
					RightClickGround(FVector2D(W.X, W.Z), false, TEXT("field check: join"));
					return;
				}
			UE_LOG(LogTC, Warning, TEXT("field check: no planned pattern with segments left to hand out"));
		} });
		Script.Add({ T0 + 13.0, TEXT("field check: report, raw dig at an unseen spot past the client's check"), [this, Diggers, Far, Works, Digs, Deny]()
		{
			UE_LOG(LogTC, Log, TEXT("field check: %s; %s; %s"), *Works(), *Digs(), *Deny());
			TSharedRef<FJsonObject> O = Raw(TEXT("dig"));
			O->SetArrayField(TEXT("ids"), IdValues({ Diggers[0] }));
			O->SetStringField(TEXT("kind"), TEXT("trench"));
			O->SetNumberField(TEXT("x"), Far.X); O->SetNumberField(TEXT("z"), Far.Y); O->SetNumberField(TEXT("dir"), 0);
			UE_LOG(LogTC, Log, TEXT("field check: raw { t: 'dig', kind: 'trench', x: %.1f, z: %.1f } (past the client's check): %s"), Far.X, Far.Y, SendCommand(O) ? TEXT("sent") : *LastSendProblemText);
		} });
		for (int32 I = 1; I <= 6; I++) Script.Add({ T0 + 13.0 + 4.0 * I, TEXT("field check: report"), [Works, Digs, Deny]() { UE_LOG(LogTC, Log, TEXT("field check: %s; %s; %s"), *Works(), *Digs(), *Deny()); } });
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("field check: script for %d builder squads around (%.1f, %.1f); unseen spot (%.1f, %.1f)"), Diggers.Num(), Home.X, Home.Y, Far.X, Far.Y);
	}
}
