// Slice 1f verification switches of ATCGameMode (see TCGameMode.h): the Formation menu and facing gesture, support
// calls and area fire, and the cover preview. They run the player's own paths: simulated key events through the
// engine's input (ATCPlayerController::InjectKey), HUD buttons through the HUD's hit test at their drawn centre
// (PressCommand), world clicks through the controller's ground pick at a computed pixel (AimClickAt), and the facing
// gesture through the controller's press, per-frame move and release functions at computed pixels (RightPressWorld,
// AttackMovePress, PinGesturePointer, GestureRelease). Selections before a step are set directly. Raw probes past the
// client's checks are marked as such in the log. None of this is a physical mouse or keyboard.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "Client/TCUnitRenderer.h"
#include "World/TCTerrain.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "UnrealClient.h"
#include "Misc/Paths.h"

namespace TCSlice6Local
{
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
	FString CoverCounts(const TArray<FTCCoverGlyph>& Glyphs)
	{
		int32 Heavy = 0, Light = 0, Open = 0, Chevrons = 0;
		for (const FTCCoverGlyph& G : Glyphs)
		{
			if (G.bChevron) Chevrons++;
			else if (G.Kind == ETCCoverKind::Heavy) Heavy++;
			else if (G.Kind == ETCCoverKind::Light) Light++;
			else Open++;
		}
		return FString::Printf(TEXT("%d glyphs (heavy %d, light %d, open %d, chevrons %d)"), Glyphs.Num(), Heavy, Light, Open, Chevrons);
	}
}

void ATCGameMode::RunSlice6Switches(double Now)
{
	using namespace TCSlice6Local;
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!PC || !(bFormationCheck || bPowersCheck || bCoverCheck)) return;
	if (!(State.CanOrder() && State.Snapshots >= 3 && Recruit.HasSnapshot() && Powers.HasSnapshot())) return;
	const FVector2D Centre(State.Map.W * FTCMatchState::CellMetres / 2, State.Map.H * FTCMatchState::CellMetres / 2);
	auto Key = [PC](const FKey& K, bool bDown) { PC->InjectKey(K, bDown); };
	auto Note = [PC]() { return FString::Printf(TEXT("%s note: '%s'"), PC->bLastOrderSent ? TEXT("sent") : TEXT("not sent"), *PC->LastOrderNote); };
	auto Deny = [this]() { return FString::Printf(TEXT("denies %d, last refusal %s/%s ('%s')"), State.Denies, *State.DenyCmd, *State.DenyReason, *TCOrders::DenySentence(State.DenyReason, State.DenyCmd)); };
	// a key chord through simulated key events: the modifier down, the key down, then both up 0.1 s later
	auto Chord = [this, Key](double At, const FKey& K, const FKey* Modifier, const FString& What)
	{
		Script.Add({ At, FString::Printf(TEXT("simulated %s%s down (%s)"), Modifier ? *(Modifier->ToString() + TEXT("+")) : TEXT(""), *K.ToString(), *What), [Key, K, Modifier]() { if (Modifier) Key(*Modifier, true); Key(K, true); } });
		Script.Add({ At + 0.1, FString::Printf(TEXT("simulated %s up"), *K.ToString()), [Key, K, Modifier]() { Key(K, false); if (Modifier) Key(*Modifier, false); } });
	};
	// the viewport pixel of a ground point, when it is on screen
	auto PixelOf = [this, PC](FVector2D G, FVector2D& Out)
	{
		int32 VX = 0, VY = 0;
		PC->GetViewportSize(VX, VY);
		return Terrain && PC->ProjectWorldLocationToScreen(ATCTerrain::ToWorld(G.X, G.Y, Terrain->HeightAt(G.X, G.Y)), Out) && Out.X >= 0 && Out.Y >= 0 && Out.X < VX && Out.Y < VY;
	};
	auto ClickGround = [PC, PixelOf](FVector2D Target, bool bShift, const TCHAR* What)
	{
		FVector2D Screen, Picked(-1, -1);
		bool bSent;
		const bool bOnScreen = PixelOf(Target, Screen);
		if (bOnScreen) bSent = PC->AimClickAt(Screen, bShift, &Picked);
		else { bSent = PC->AimClick(true, Target, bShift); Picked = Target; }
		UE_LOG(LogTC, Log, TEXT("%s: %sclick for (%.1f, %.1f) %s, ground (%.1f, %.1f): %s; aim now %s; note '%s'"), What, bShift ? TEXT("Shift+") : TEXT(""), Target.X, Target.Y,
			bOnScreen ? *FString::Printf(TEXT("at pixel (%.0f, %.0f) through the ground pick"), Screen.X, Screen.Y) : TEXT("off screen, given directly"), Picked.X, Picked.Y, bSent ? TEXT("sent") : TEXT("nothing sent"),
			LexToString(PC->Aim), *PC->LastOrderNote);
		return bSent;
	};
	auto Toward = [Centre](FVector2D From, double Metres) { return From + (Centre - From).GetSafeNormal() * Metres; };
	auto CentreOf = [this](const TArray<int32>& Ids)
	{
		FVector2D Sum(0, 0); int32 N = 0;
		for (int32 Id : Ids) if (const FTCUnit* U = State.Units.Find(Id)) { Sum += FVector2D(U->X, U->Z); N++; }
		return N ? Sum / N : FVector2D(-1, -1);
	};
	const double T0 = Script.Num() ? Script.Last().At + 0.5 : (ScriptT0 >= 0 ? Now - ScriptT0 : 0.0);

	// -TCFormationCheck
	if (bFormationCheck && !bFormationScriptMade)
	{
		double CX, CZ;
		const TArray<int32> Army = ArmyIds(CX, CZ);
		if (Army.Num() < 3) return;
		bFormationScriptMade = true;
		const FVector2D Home(CX, CZ);
		auto Settings = [PC]()
		{
			const FTCFormationSettings& F = PC->Formation.Settings;
			return FString::Printf(TEXT("shape %s, spread %.2f, together %s, snap %s, menu %s"), LexToString(F.Shape), F.Spread, F.bTogether ? TEXT("on") : TEXT("off"), F.bSnap ? TEXT("on") : TEXT("off"),
				PC->Formation.bMenuOpen ? TEXT("open") : TEXT("closed"));
		};
		auto Spread = [this, Army, CentreOf]()
		{
			// how far the army stands from its centre: a re-form at a wider spacing should push it out
			const FVector2D C = CentreOf(Army);
			double Sum = 0; int32 N = 0;
			for (int32 Id : Army) if (const FTCUnit* U = State.Units.Find(Id)) { Sum += FVector2D::Distance(FVector2D(U->X, U->Z), C); N++; }
			return FString::Printf(TEXT("tick %d: army of %d centred (%.1f, %.1f), mean %.2f m from its centre"), State.Tick, N, C.X, C.Y, N ? Sum / N : -1.0);
		};
		// the spots the last gesture's preview proposed, to compare with where the units end up
		TSharedRef<TMap<int32, FVector2D>> Proposed = MakeShared<TMap<int32, FVector2D>>();
		auto ToSpots = [this, Proposed]()
		{
			double Sum = 0, Max = 0; int32 N = 0;
			for (const auto& P : *Proposed) if (const FTCUnit* U = State.Units.Find(P.Key)) { const double D = FVector2D::Distance(FVector2D(U->X, U->Z), P.Value); Sum += D; Max = FMath::Max(Max, D); N++; }
			return FString::Printf(TEXT("tick %d: %d of %d units held, distance to the proposed spots mean %.2f m, max %.2f m"), State.Tick, N, Proposed->Num(), N ? Sum / N : -1.0, Max);
		};
		auto PreviewReport = [PC, Proposed]()
		{
			const FTCFormationPreview& P = PC->FacingPreview;
			Proposed->Reset();
			for (const FTCSpot& S : P.Spots) Proposed->Add(S.Id, FVector2D(S.X, S.Z));
			return FString::Printf(TEXT("gesture %s, preview %s: %d spots, %d lines, %d arrows, facing %.4f rad, width %.1f m, colour #%02x%02x%02x"), PC->Formation.IsGesturing() ? TEXT("active") : TEXT("none"),
				P.bVisible ? TEXT("visible") : TEXT("hidden"), P.Spots.Num(), P.Lines.Num(), P.Arrows.Num(), P.Face, P.Reach, P.Color.R, P.Color.G, P.Color.B);
		};
		const FVector2D A = Toward(Home, 18.0), Side = (A - Home).GetSafeNormal().GetRotated(90.0);
		const FVector2D B = A + Side * 9.0;
		Script.Add({ T0, FString::Printf(TEXT("formation check: select the army of %d (set directly)"), Army.Num()), [PC, Army, Settings, Spread]()
		{
			PC->SelectOwn(Army);
			UE_LOG(LogTC, Log, TEXT("formation check: %s; %s"), *Settings(), *Spread());
		} });
		Chord(T0 + 0.3, EKeys::V, &EKeys::LeftShift, TEXT("cycle the formation"));
		Script.Add({ T0 + 0.8, TEXT("formation check: open the Form menu through the hit test"), [PC, Settings, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("formation check: after Shift+V: %s; %s"), *Settings(), *Note());
			FTCUiCommand C(ETCUiAction::Formation); C.Unit = TEXT("menu");
			PC->PressCommand(C);
			UE_LOG(LogTC, Log, TEXT("formation check: after the Form button: %s"), *Settings());
		} });
		Script.Add({ T0 + 1.1, TEXT("formation check: the Form menu's Wedge and March together (twice) through the hit test"), [PC, Settings, Note]()
		{
			FTCUiCommand C(ETCUiAction::Formation);
			C.Unit = TEXT("shape:wedge"); PC->PressCommand(C);
			UE_LOG(LogTC, Log, TEXT("formation check: after Wedge: %s; %s"), *Settings(), *Note());
			C.Unit = TEXT("together"); PC->PressCommand(C);
			UE_LOG(LogTC, Log, TEXT("formation check: after March together: %s"), *Settings());
			C.Unit = TEXT("together"); PC->PressCommand(C);
			UE_LOG(LogTC, Log, TEXT("formation check: after March together again: %s"), *Settings());
		} });
		Chord(T0 + 1.5, EKeys::RightBracket, nullptr, TEXT("spread: re-form wider where they stand"));
		Script.Add({ T0 + 2.0, TEXT("formation check: note"), [Settings, Note, Spread]() { UE_LOG(LogTC, Log, TEXT("formation check: after ]: %s; %s; %s"), *Settings(), *Note(), *Spread()); } });
		Script.Add({ T0 + 5.5, TEXT("formation check: report"), [Spread, Deny]() { UE_LOG(LogTC, Log, TEXT("formation check: 3.5 s after ]: %s; %s"), *Spread(), *Deny()); } });
		Chord(T0 + 5.7, EKeys::LeftBracket, nullptr, TEXT("tighten: re-form closer"));
		Script.Add({ T0 + 6.2, TEXT("formation check: note"), [Settings, Note]() { UE_LOG(LogTC, Log, TEXT("formation check: after [: %s; %s"), *Settings(), *Note()); } });
		Script.Add({ T0 + 9.5, TEXT("formation check: report"), [Spread, Deny]() { UE_LOG(LogTC, Log, TEXT("formation check: 3.3 s after [: %s; %s"), *Spread(), *Deny()); } });
		// the right-drag: a press where the move would go, the pointer dragged 9 m to the side, the preview, the release
		Script.Add({ T0 + 9.7, TEXT("formation check: right press 18 m toward the centre (the gesture starts)"), [PC, Army, A, PixelOf]()
		{
			PC->SelectOwn(Army);
			FVector2D P;
			if (!PixelOf(A, P)) { UE_LOG(LogTC, Warning, TEXT("formation check: (%.1f, %.1f) is not on screen"), A.X, A.Y); return; }
			const bool bGesture = PC->RightPressWorld(P, false, false);
			PC->PinGesturePointer(P);
			UE_LOG(LogTC, Log, TEXT("formation check: right press at pixel (%.0f, %.0f): %s"), P.X, P.Y, bGesture ? TEXT("facing gesture started (the order would be a move)") : TEXT("dispatched at once"));
		} });
		Script.Add({ T0 + 9.9, TEXT("formation check: drag a third of the way"), [PC, A, B, PixelOf]() { FVector2D P; if (PixelOf(A + (B - A) / 3, P)) PC->PinGesturePointer(P); } });
		Script.Add({ T0 + 10.1, TEXT("formation check: drag two thirds"), [PC, A, B, PixelOf]() { FVector2D P; if (PixelOf(A + (B - A) * 2 / 3, P)) PC->PinGesturePointer(P); } });
		Script.Add({ T0 + 10.3, TEXT("formation check: drag to 9 m to the side"), [PC, B, PixelOf, PreviewReport]()
		{
			FVector2D P;
			if (PixelOf(B, P)) PC->PinGesturePointer(P);
			UE_LOG(LogTC, Log, TEXT("formation check: during the drag: %s"), *PreviewReport());
		} });
		Script.Add({ T0 + 10.6, TEXT("formation check: preview, screenshot"), [this, PC, PreviewReport]()
		{
			UE_LOG(LogTC, Log, TEXT("formation check: before the release: %s"), *PreviewReport());
			if (!FormationShotFile.IsEmpty()) { FScreenshotRequest::RequestScreenshot(FormationShotFile, true, false); UE_LOG(LogTC, Log, TEXT("formation check: screenshot of the drag requested: %s"), *FormationShotFile); }
		} });
		Script.Add({ T0 + 11.4, TEXT("formation check: release the right button at the drag's end"), [PC, B, PixelOf, Note, PreviewReport]()
		{
			UE_LOG(LogTC, Log, TEXT("formation check: at the release: %s"), *PreviewReport());
			FVector2D P;
			PixelOf(B, P);
			PC->ReleaseGesturePointer();
			const int32 N = PC->GestureRelease(ETCGestureButton::Right, P, false, false);
			UE_LOG(LogTC, Log, TEXT("formation check: release: %d named; %s; gesture %s"), N, *Note(), PC->Formation.IsGesturing() ? TEXT("still active") : TEXT("ended"));
		} });
		for (int32 I = 1; I <= 3; I++) Script.Add({ T0 + 11.4 + 2.5 * I, TEXT("formation check: report"), [ToSpots, Deny]() { UE_LOG(LogTC, Log, TEXT("formation check: after the faced move: %s; %s"), *ToSpots(), *Deny()); } });
		// a plain right click: the gesture starts and ends without a drag (no facing)
		const FVector2D A2 = A + (Home - A).GetSafeNormal() * 8.0;
		Script.Add({ T0 + 19.2, TEXT("formation check: a plain right click (press and release, no drag)"), [PC, A2, PixelOf, Note]()
		{
			FVector2D P;
			if (!PixelOf(A2, P)) return;
			const bool bGesture = PC->RightPressWorld(P, false, false);
			const int32 N = PC->GestureRelease(ETCGestureButton::Right, P, false, false);
			UE_LOG(LogTC, Log, TEXT("formation check: plain right click at pixel (%.0f, %.0f): gesture %s, %d named; %s"), P.X, P.Y, bGesture ? TEXT("started") : TEXT("not started"), N, *Note());
		} });
		Script.Add({ T0 + 21.5, TEXT("formation check: report"), [Deny, Spread]() { UE_LOG(LogTC, Log, TEXT("formation check: after the plain click: %s; %s"), *Spread(), *Deny()); } });
		// the armed attack-move's left press starts the gesture too
		Chord(T0 + 21.7, EKeys::G, nullptr, TEXT("arm the attack-move"));
		const FVector2D A3 = A + Side * 4.0, B3 = A3 - (A - Home).GetSafeNormal() * 6.0;
		Script.Add({ T0 + 22.1, TEXT("formation check: attack-move press and drag"), [PC, A3, B3, PixelOf]()
		{
			FVector2D P, Q;
			if (!PixelOf(A3, P) || !PixelOf(B3, Q)) return;
			UE_LOG(LogTC, Log, TEXT("formation check: aim %s before the press"), LexToString(PC->Aim));
			const bool bGesture = PC->AttackMovePress(P, false, false);
			PC->PinGesturePointer(Q);
			UE_LOG(LogTC, Log, TEXT("formation check: attack-move press at pixel (%.0f, %.0f): %s; dragged to (%.0f, %.0f)"), P.X, P.Y, bGesture ? TEXT("gesture started") : TEXT("no gesture"), Q.X, Q.Y);
		} });
		Script.Add({ T0 + 22.5, TEXT("formation check: attack-move release"), [PC, B3, PixelOf, Note, PreviewReport]()
		{
			UE_LOG(LogTC, Log, TEXT("formation check: attack-move drag: %s"), *PreviewReport());
			FVector2D Q;
			PixelOf(B3, Q);
			PC->ReleaseGesturePointer();
			const int32 N = PC->GestureRelease(ETCGestureButton::Left, Q, false, false);
			UE_LOG(LogTC, Log, TEXT("formation check: attack-move release: %d named; %s; aim now %s"), N, *Note(), LexToString(PC->Aim));
		} });
		Script.Add({ T0 + 25.0, TEXT("formation check: report"), [ToSpots, Deny]() { UE_LOG(LogTC, Log, TEXT("formation check: after the attack-move drag: %s; %s"), *ToSpots(), *Deny()); } });
		// Esc ends a gesture: nothing is sent
		Script.Add({ T0 + 25.2, TEXT("formation check: a right press and drag, then Esc"), [this, PC, A, B, PixelOf]()
		{
			FVector2D P, Q;
			if (!PixelOf(A, P) || !PixelOf(B, Q)) return;
			PC->RightPressWorld(P, false, false);
			PC->PinGesturePointer(Q);
			UE_LOG(LogTC, Log, TEXT("formation check: gesture %s before Esc; denies %d"), PC->Formation.IsGesturing() ? TEXT("active") : TEXT("none"), State.Denies);
		} });
		Chord(T0 + 25.5, EKeys::Escape, nullptr, TEXT("cancel the gesture"));
		Script.Add({ T0 + 25.9, TEXT("formation check: after Esc"), [PC, Note]()
		{
			PC->ReleaseGesturePointer();
			UE_LOG(LogTC, Log, TEXT("formation check: after Esc: gesture %s, preview %s; %s"), PC->Formation.IsGesturing() ? TEXT("active") : TEXT("ended"), PC->FacingPreview.bVisible ? TEXT("visible") : TEXT("hidden"), *Note());
		} });
		// the control group copy: Ctrl+1 saves the menu, Shift+V changes it, 1 brings the saved one back
		Chord(T0 + 26.1, EKeys::One, &EKeys::LeftControl, TEXT("set group 1 (keeps a copy of the Formation menu)"));
		Chord(T0 + 26.5, EKeys::V, &EKeys::LeftShift, TEXT("cycle the formation again"));
		Script.Add({ T0 + 26.9, TEXT("formation check: report"), [Settings]() { UE_LOG(LogTC, Log, TEXT("formation check: after Ctrl+1 and Shift+V: %s"), *Settings()); } });
		Chord(T0 + 27.1, EKeys::One, nullptr, TEXT("recall group 1 (restores its Formation menu)"));
		Script.Add({ T0 + 27.5, TEXT("formation check: report"), [Settings]() { UE_LOG(LogTC, Log, TEXT("formation check: after recalling 1: %s"), *Settings()); } });
		// the client's refusal: a re-form needs two own units (the browser shows the spacing and sends nothing)
		Script.Add({ T0 + 27.7, TEXT("formation check: select one unit (directly)"), [PC, Army]() { PC->SelectOwn({ Army[0] }); } });
		Chord(T0 + 27.9, EKeys::RightBracket, nullptr, TEXT("spread with one unit selected: nothing to send"));
		Script.Add({ T0 + 28.3, TEXT("formation check: report"), [Settings, Note, Deny]() { UE_LOG(LogTC, Log, TEXT("formation check: ] with one unit: %s; %s; %s"), *Settings(), *Note(), *Deny()); } });
		// the server's refusal: a move that names only the HQ (a structure is never 'mine' to move)
		Script.Add({ T0 + 28.5, TEXT("formation check: raw move of the HQ past the client's check"), [this, A, Deny]()
		{
			const TArray<int32> Hq = OwnOfTypes({ TEXT("hq") });
			if (!Hq.Num()) { UE_LOG(LogTC, Warning, TEXT("formation check: no HQ held")); return; }
			TSharedRef<FJsonObject> O = Raw(TEXT("move"));
			TArray<TSharedPtr<FJsonValue>> Row = { MakeShared<FJsonValueNumber>(Hq[0]), MakeShared<FJsonValueNumber>(A.X), MakeShared<FJsonValueNumber>(A.Y) };
			O->SetArrayField(TEXT("orders"), { MakeShared<FJsonValueArray>(Row) });
			O->SetNumberField(TEXT("face"), 0.5);
			O->SetBoolField(TEXT("queue"), false);
			UE_LOG(LogTC, Log, TEXT("formation check: raw { t: 'move', orders: [[%d, %.1f, %.1f]], face: 0.5 } (past the client's check): %s; before: %s"), Hq[0], A.X, A.Y, SendCommand(O) ? TEXT("sent") : *LastSendProblemText, *Deny());
		} });
		Script.Add({ T0 + 30.0, TEXT("formation check: report"), [Deny]() { UE_LOG(LogTC, Log, TEXT("formation check: after the raw move: %s"), *Deny()); } });
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("formation check: script for an army of %d around (%.1f, %.1f); the drag from (%.1f, %.1f) to (%.1f, %.1f)"), Army.Num(), Home.X, Home.Y, A.X, A.Y, B.X, B.Y);
	}

	// -TCPowersCheck: a sandbox, so an own mortar can be spawned for the area fire
	if (bPowersCheck && !bPowersScriptMade)
	{
		double CX, CZ;
		const TArray<int32> Army = ArmyIds(CX, CZ);
		const TArray<int32> Mortars = OwnOfTypes({ TEXT("mortar") });
		if (!Army.Num()) return;
		if (!Mortars.Num())
		{
			if (!bPowersSpawnSent && State.bSandbox)
			{
				bPowersSpawnSent = true;
				const FVector2D At = Toward(FVector2D(CX, CZ), 4.0);
				TSharedRef<FJsonObject> Sp = Raw(TEXT("spawn"));
				Sp->SetStringField(TEXT("unit"), TEXT("mortar")); Sp->SetNumberField(TEXT("x"), At.X); Sp->SetNumberField(TEXT("z"), At.Y);
				UE_LOG(LogTC, Log, TEXT("powers check: sandbox spawn of an own mortar at (%.1f, %.1f): %s"), At.X, At.Y, SendCommand(Sp) ? TEXT("sent") : *LastSendProblemText);
			}
			if (State.bSandbox) return;
		}
		bPowersScriptMade = true;
		const FVector2D Home(CX, CZ);
		TArray<int32> Rifles = Army;
		Rifles.RemoveAll([this](int32 Id) { const FTCUnit* U = State.Units.Find(Id); return !U || FTCSupportPowers::IsSalvo(U->Type); });
		TArray<int32> All = Army;
		for (int32 Id : Mortars) All.AddUnique(Id);
		auto Bar = [this]()
		{
			TArray<FString> L;
			for (const FTCSupportButton& B : Powers.Buttons(State, UnitTable, Recruit))
				L.Add(FString::Printf(TEXT("%s [%s] '%s'%s"), *B.Kind, *B.Key, *B.Text, B.Check.bOk ? TEXT("") : *FString::Printf(TEXT(" refused '%s'"), *B.Check.Reason)));
			return FString::Join(L, TEXT(", "));
		};
		auto Wallet = [this]()
		{
			TArray<FString> Cds;
			for (const FTCSupportDef& D : FTCSupportPowers::All()) if (Powers.Cooldown(D.Kind) > 0) Cds.Add(FString::Printf(TEXT("%s %.1f"), *D.Kind, Powers.Cooldown(D.Kind)));
			TArray<FString> Ks;
			for (const FTCStrikeMark& K : Powers.Strikes(State)) Ks.Add(FString::Printf(TEXT("%s of seat %d at (%.1f, %.1f) dir %.3f t %.1f%s%s"), *K.Kind, K.Owner, K.At.X, K.At.Y, K.Dir, K.T, K.bFoe ? TEXT(" foe") : TEXT(""), K.bStrip ? TEXT(" strip") : TEXT(" ring")));
			return FString::Printf(TEXT("tick %d, MP %.0f; cooldowns [%s]; strikes [%s]"), State.Tick, Recruit.Mp(), *FString::Join(Cds, TEXT(", ")), *FString::Join(Ks, TEXT("; ")));
		};
		auto MortarRow = [this, Mortars]()
		{
			const FTCUnit* U = Mortars.Num() ? State.Units.Find(Mortars[0]) : nullptr;
			return U ? FString::Printf(TEXT("mortar %d at (%.1f, %.1f) target %d aim %.2f cd %d"), U->Id, U->X, U->Z, U->Target, U->Aim, U->Cd) : FString(TEXT("no mortar held"));
		};
		const FVector2D A = Toward(Home, 30.0), Side = (A - Home).GetSafeNormal().GetRotated(90.0);
		Script.Add({ T0, FString::Printf(TEXT("powers check: select the army and the mortar (%d, set directly)"), All.Num()), [PC, All, Bar, Wallet]()
		{
			PC->SelectOwn(All);
			UE_LOG(LogTC, Log, TEXT("powers check: support bar [%s]; %s"), *Bar(), *Wallet());
		} });
		Chord(T0 + 0.2, EKeys::Z, nullptr, TEXT("aim the recon flight"));
		Script.Add({ T0 + 0.6, TEXT("powers check: the recon's centre through the ground pick, the pointer held 10 m to the side"), [this, PC, A, Side, ClickGround]()
		{
			UE_LOG(LogTC, Log, TEXT("powers check: after Z: aim %s (%s), hint '%s'"), LexToString(PC->Aim), *PC->PowerAim.SupportKind, *PC->AimHint);
			ClickGround(A, false, TEXT("powers check: recon, first click (the centre)"));
			PC->PinPointerGround(A + Side * 10.0);
		} });
		Script.Add({ T0 + 1.0, TEXT("powers check: the strip preview, screenshot"), [this, PC]()
		{
			const FTCSupportPreview& P = PC->PowerPreview;
			UE_LOG(LogTC, Log, TEXT("powers check: preview %s%s at (%.1f, %.1f), %s %.0f x %.0f m, direction %.3f rad; hint '%s'"), P.bShown ? TEXT("shown") : TEXT("hidden"), P.bPlaced ? TEXT(", placed") : TEXT(""),
				P.At.X, P.At.Y, P.bStrip ? TEXT("strip") : TEXT("ring"), P.bStrip ? P.Len : P.Radius, P.Width, P.Dir, *PC->AimHint);
			if (!PowersShotFile.IsEmpty()) { FScreenshotRequest::RequestScreenshot(PowersShotFile, true, false); UE_LOG(LogTC, Log, TEXT("powers check: screenshot of the strip preview requested: %s"), *PowersShotFile); }
		} });
		Script.Add({ T0 + 1.6, TEXT("powers check: the recon's second click (the direction)"), [PC, A, Side, ClickGround, Note]()
		{
			PC->ReleasePointerGround();
			ClickGround(A + Side * 10.0, false, TEXT("powers check: recon, second click"));
			UE_LOG(LogTC, Log, TEXT("powers check: recon: %s"), *Note());
		} });
		Script.Add({ T0 + 3.0, TEXT("powers check: report, screenshot of the warning"), [this, Wallet, Deny]()
		{
			UE_LOG(LogTC, Log, TEXT("powers check: after the recon: %s; %s"), *Wallet(), *Deny());
			if (!PowersShotFile.IsEmpty())
			{
				const FString Shot = FPaths::GetPath(PowersShotFile) / FPaths::GetBaseFilename(PowersShotFile) + TEXT("-strike.png");
				FScreenshotRequest::RequestScreenshot(Shot, true, false);
				UE_LOG(LogTC, Log, TEXT("powers check: screenshot of the strike warning requested: %s"), *Shot);
			}
		} });
		Chord(T0 + 3.4, EKeys::Z, nullptr, TEXT("the recon again: on cooldown in the snapshot, refused by the client"));
		Script.Add({ T0 + 3.8, TEXT("powers check: note"), [PC, Note]() { UE_LOG(LogTC, Log, TEXT("powers check: Z on cooldown: %s; aim %s"), *Note(), LexToString(PC->Aim)); } });
		// smoke through the support bar's button, twice in one frame: the second is the server's refusal
		const FVector2D S0 = Toward(Home, 20.0) - Side * 8.0;
		Script.Add({ T0 + 4.0, TEXT("powers check: the smoke barrage's button through the hit test, two clicks, then the same again in this frame"), [PC, S0, Side, ClickGround, Note]()
		{
			for (int32 I = 0; I < 2; I++)
			{
				FTCUiCommand C(ETCUiAction::Support); C.Unit = TEXT("smoke");
				const bool bArmed = PC->PressCommand(C);
				UE_LOG(LogTC, Log, TEXT("powers check: smoke button %d: %s; aim %s"), I + 1, bArmed ? TEXT("armed") : TEXT("not armed"), LexToString(PC->Aim));
				if (!bArmed) { UE_LOG(LogTC, Log, TEXT("powers check: %s"), *Note()); continue; }
				ClickGround(S0, false, TEXT("powers check: smoke, first click"));
				ClickGround(S0 + Side * 6.0, false, TEXT("powers check: smoke, second click"));
			}
		} });
		Script.Add({ T0 + 6.0, TEXT("powers check: report"), [Wallet, Deny]() { UE_LOG(LogTC, Log, TEXT("powers check: after the smoke: %s; %s"), *Wallet(), *Deny()); } });
		// a point kind: fighter cover (I), one click, when the wallet allows it; otherwise the client says why
		Chord(T0 + 6.2, EKeys::I, nullptr, TEXT("aim the fighter cover"));
		Script.Add({ T0 + 6.6, TEXT("powers check: the fighter cover's click"), [PC, Home, ClickGround, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("powers check: after I: aim %s, %s"), LexToString(PC->Aim), *Note());
			if (PC->Aim == ETCAim::Support) ClickGround(Home, false, TEXT("powers check: fighter cover, one click"));
		} });
		Chord(T0 + 7.0, EKeys::U, nullptr, TEXT("aim the dive bomber"));
		Script.Add({ T0 + 7.4, TEXT("powers check: after U"), [PC, Note]() { UE_LOG(LogTC, Log, TEXT("powers check: after U: aim %s, %s"), LexToString(PC->Aim), *Note()); } });
		Chord(T0 + 7.6, EKeys::Escape, nullptr, TEXT("cancel any aim"));
		Script.Add({ T0 + 8.2, TEXT("powers check: report; select the rifle squads only (directly)"), [PC, Rifles, Wallet, Deny]()
		{
			UE_LOG(LogTC, Log, TEXT("powers check: %s; %s"), *Wallet(), *Deny());
			PC->SelectOwn(Rifles);
		} });
		Chord(T0 + 8.4, EKeys::B, &EKeys::LeftShift, TEXT("shell area with no salvo unit selected: the client's hint, no aim"));
		Script.Add({ T0 + 8.8, TEXT("powers check: note; select the army and the mortar (directly)"), [PC, All, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("powers check: Shift+B without a mortar: %s; aim %s"), *Note(), LexToString(PC->Aim));
			PC->SelectOwn(All);
		} });
		const FVector2D F0 = Toward(Home, 27.0);
		Script.Add({ T0 + 9.1, TEXT("powers check: the Shell area button through the hit test, a Shift click (queued, the aim stays), then a plain click"), [PC, F0, Side, ClickGround, MortarRow]()
		{
			const bool bArmed = PC->PressCommand(FTCUiCommand(ETCUiAction::Area));
			UE_LOG(LogTC, Log, TEXT("powers check: Shell area button: %s, hint '%s'; %s"), bArmed ? TEXT("armed") : TEXT("not armed"), *PC->AimHint, *MortarRow());
			if (!bArmed) return;
			ClickGround(F0 + Side * 3.0, true, TEXT("powers check: shell area, Shift+click"));
			ClickGround(F0, false, TEXT("powers check: shell area, click"));
		} });
		for (int32 I = 1; I <= 2; I++) Script.Add({ T0 + 9.1 + 2.0 * I, TEXT("powers check: report"), [MortarRow, Deny]() { UE_LOG(LogTC, Log, TEXT("powers check: after the area fire: %s; %s"), *MortarRow(), *Deny()); } });
		// the server's refusal: riflemen cannot shell open ground ('blocked')
		Script.Add({ T0 + 13.4, TEXT("powers check: raw fireat by the rifle squads past the client's check"), [this, Rifles, Home, Deny]()
		{
			TSharedRef<FJsonObject> O = Raw(TEXT("fireat"));
			O->SetArrayField(TEXT("ids"), IdValues(Rifles));
			O->SetNumberField(TEXT("x"), Home.X + 6.0); O->SetNumberField(TEXT("z"), Home.Y);
			O->SetBoolField(TEXT("queue"), false);
			UE_LOG(LogTC, Log, TEXT("powers check: raw { t: 'fireat', ids: %d rifle squads, x: %.1f, z: %.1f } (past the client's check): %s; before: %s"), Rifles.Num(), Home.X + 6.0, Home.Y, SendCommand(O) ? TEXT("sent") : *LastSendProblemText, *Deny());
		} });
		Script.Add({ T0 + 15.0, TEXT("powers check: report; Shift+B then a right press on the minimap (cancels)"), [this, PC, Deny, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("powers check: after the raw fireat: %s"), *Deny());
			PC->RunUi(ETCUiAction::Area);
			const ATCHUD* Hud = Cast<ATCHUD>(PC->GetHUD());
			FVector2D Px;
			if (Hud && Hud->Minimap.WorldToPixel(FVector2D(State.Map.W, State.Map.H), Px))
			{
				const FTCMinimapPress P = PC->MinimapPress(Px, true, false, false);
				UE_LOG(LogTC, Log, TEXT("powers check: minimap right press at (%.0f, %.0f) with the area aim: %s; aim now %s; %s"), Px.X, Px.Y, P.bConsumed ? TEXT("taken") : TEXT("not taken"), LexToString(PC->Aim), *Note());
			}
			else UE_LOG(LogTC, Log, TEXT("powers check: no minimap drawn; aim %s"), LexToString(PC->Aim));
		} });
		Script.Add({ T0 + 17.0, TEXT("powers check: report"), [Wallet, Deny]() { UE_LOG(LogTC, Log, TEXT("powers check: %s; %s"), *Wallet(), *Deny()); } });
		// a point kind once the wallet allows it: fighter cover (I, 120 MP), one click through the ground pick
		Script.Add({ T0 + 17.2, TEXT("powers check: wait for 120 MP, then I and one click"), [this, Wallet]()
		{
			const double Due = (120.0 - Recruit.Mp()) / 4.0; // shared/sim.js CFG.mpBase: 4 MP a second at the start
			UE_LOG(LogTC, Log, TEXT("powers check: MP %.0f, about %.1f s to 120 (%s)"), Recruit.Mp(), FMath::Max(0.0, Due), *Wallet());
		} });
		for (int32 I = 0; I < 6; I++)
		{
			Script.Add({ T0 + 18.0 + 1.5 * I, TEXT("powers check: fighter cover when affordable"), [this, PC]()
			{
				if (PC->Aim == ETCAim::Support || Powers.Cooldown(TEXT("cover")) > 0 || Recruit.Mp() < 120) return;
				PC->InjectKey(EKeys::I, true);
				UE_LOG(LogTC, Log, TEXT("powers check: simulated I down (MP %.0f)"), Recruit.Mp());
			} });
			Script.Add({ T0 + 18.1 + 1.5 * I, TEXT("powers check: simulated I up"), [PC]() { PC->InjectKey(EKeys::I, false); } });
		}
		Script.Add({ T0 + 27.5, TEXT("powers check: the fighter cover's click"), [PC, Home, ClickGround, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("powers check: before the click: aim %s (%s); %s"), LexToString(PC->Aim), *PC->PowerAim.SupportKind, *Note());
			if (PC->Aim == ETCAim::Support) ClickGround(Home, false, TEXT("powers check: fighter cover, one click"));
		} });
		Script.Add({ T0 + 29.5, TEXT("powers check: report"), [Wallet, Deny]() { UE_LOG(LogTC, Log, TEXT("powers check: final: %s; %s"), *Wallet(), *Deny()); } });
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("powers check: script for %d units (%d mortars) around (%.1f, %.1f)"), All.Num(), Mortars.Num(), Home.X, Home.Y);
	}

	// -TCCoverCheck
	if (bCoverCheck && !bCoverScriptMade)
	{
		double CX, CZ;
		const TArray<int32> Army = ArmyIds(CX, CZ);
		TArray<int32> Inf = Army;
		Inf.RemoveAll([this](int32 Id) { const FTCUnit* U = State.Units.Find(Id); const FTCUnitDef* D = U ? UnitTable.Find(U->Type) : nullptr; return !D || !D->bInfantry; });
		if (Inf.Num() < 2) return;
		bCoverScriptMade = true;
		const FVector2D Home = CentreOf(Inf), Spot = Toward(Home, 6.0);
		auto Report = [this]()
		{
			const FTCCoverView& V = Cover.View();
			return FString::Printf(TEXT("cover %s, view %s (cursor ground %s %.2f, through %s %.2f; flash ground %s %.2f, through %s %.2f); cursor marks: ground %s, through %s; flash marks: ground %s"),
				Cover.IsEnabled() ? TEXT("on") : TEXT("off"), V.bVisible ? TEXT("visible") : TEXT("hidden"), V.bCursorGround ? TEXT("on") : TEXT("off"), V.CursorGroundOpacity, V.bCursorThrough ? TEXT("on") : TEXT("off"), V.CursorThroughOpacity,
				V.bFlashGround ? TEXT("on") : TEXT("off"), V.FlashGroundOpacity, V.bFlashThrough ? TEXT("on") : TEXT("off"), V.FlashThroughOpacity,
				*CoverCounts(Cover.CursorMarks().Ground), *CoverCounts(Cover.CursorMarks().Through), *CoverCounts(Cover.FlashMarks().Ground));
		};
		Script.Add({ T0, FString::Printf(TEXT("cover check: select %d infantry units (directly), hold the pointer's ground 6 m toward the centre"), Inf.Num()), [PC, Inf, Spot, Report]()
		{
			PC->SelectOwn(Inf);
			PC->PinPointerGround(Spot);
			UE_LOG(LogTC, Log, TEXT("cover check: pointer at (%.1f, %.1f); %s"), Spot.X, Spot.Y, *Report());
		} });
		Script.Add({ T0 + 0.6, TEXT("cover check: report, screenshot"), [this, Report]()
		{
			UE_LOG(LogTC, Log, TEXT("cover check: %s"), *Report());
			if (!CoverShotFile.IsEmpty()) { FScreenshotRequest::RequestScreenshot(CoverShotFile, true, false); UE_LOG(LogTC, Log, TEXT("cover check: screenshot requested: %s"), *CoverShotFile); }
		} });
		// a move: its feedback flashes the marks at the destination
		const FVector2D Dest = Toward(Home, 14.0);
		Script.Add({ T0 + 1.4, TEXT("cover check: a right click 14 m toward the centre (press and release)"), [PC, Dest, PixelOf, Note, Report]()
		{
			FVector2D P;
			if (!PixelOf(Dest, P)) { UE_LOG(LogTC, Warning, TEXT("cover check: the destination is not on screen")); return; }
			PC->RightPressWorld(P, false, false);
			const int32 N = PC->GestureRelease(ETCGestureButton::Right, P, false, false);
			UE_LOG(LogTC, Log, TEXT("cover check: move: %d named; %s; %s"), N, *Note(), *Report());
		} });
		Script.Add({ T0 + 1.7, TEXT("cover check: report during the flash"), [Report]() { UE_LOG(LogTC, Log, TEXT("cover check: during the flash: %s"), *Report()); } });
		Script.Add({ T0 + 3.6, TEXT("cover check: report after the flash"), [Report]() { UE_LOG(LogTC, Log, TEXT("cover check: after the flash: %s"), *Report()); } });
		Script.Add({ T0 + 3.8, TEXT("cover check: the cover preview switch through the hit test (off)"), [PC, Note]() { PC->PressCommand(FTCUiCommand(ETCUiAction::CoverToggle)); UE_LOG(LogTC, Log, TEXT("cover check: %s"), *Note()); } });
		Script.Add({ T0 + 4.2, TEXT("cover check: report"), [Report]() { UE_LOG(LogTC, Log, TEXT("cover check: switched off: %s"), *Report()); } });
		Script.Add({ T0 + 4.4, TEXT("cover check: the switch again (on)"), [PC, Note]() { PC->PressCommand(FTCUiCommand(ETCUiAction::CoverToggle)); UE_LOG(LogTC, Log, TEXT("cover check: %s"), *Note()); } });
		Script.Add({ T0 + 4.8, TEXT("cover check: report; release the pointer"), [PC, Report]() { UE_LOG(LogTC, Log, TEXT("cover check: switched on: %s"), *Report()); PC->ReleasePointerGround(); } });
		Script.Add({ T0 + 5.2, TEXT("cover check: report; clear the selection"), [PC, Report]() { UE_LOG(LogTC, Log, TEXT("cover check: no pointer (no mouse offscreen): %s"), *Report()); PC->Selected.Reset(); } });
		Script.Add({ T0 + 5.6, TEXT("cover check: report"), [Report]() { UE_LOG(LogTC, Log, TEXT("cover check: nothing selected: %s"), *Report()); } });
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("cover check: script for %d infantry around (%.1f, %.1f)"), Inf.Num(), Home.X, Home.Y);
	}
}
