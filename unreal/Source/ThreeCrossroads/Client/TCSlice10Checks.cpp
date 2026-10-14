// Slice 10 glue and verification switches of ATCGameMode (see TCGameMode.h). Glue: the sound subsystem (UTCAudio,
// Client/TCAudio.h) found and fed once per frame, and the map ping's send (client/pings.js send). Switches, for a live
// check where nobody is at the controls: a ping through the controller's Alt+click handler (ATCPlayerController::PingAt,
// the function the mouse path calls), a screenshot of the combat effects in view, the HUD's Language button pressed
// through the hit test with a screenshot, and a screenshot of the settings buttons. Selections are set directly. None
// of this is a physical mouse or keyboard, and none of it is a player.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "Client/TCAudio.h"
#include "Client/TCLocale.h"
#include "Client/TCEffects.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "HAL/PlatformTime.h"
#include "UnrealClient.h"

UTCAudio* ATCGameMode::Audio() const
{
	UWorld* W = GetWorld();
	return W ? W->GetSubsystem<UTCAudio>() : nullptr;
}

bool ATCGameMode::SendPing(FVector2D At, const TCHAR* How)
{
	// client/pings.js send: { t: 'ping', x, z } for a finite spot; server/map-pings.js checks the seat, the map's bounds
	// and 3 pings per 5 s, then relays { t: 'ping', from, x, z } to this team's other human players (not back to us)
	if (!State.bHasMatch) return false;
	const TSharedPtr<FJsonObject> Msg = FTCPings::MakeSend(At.X, At.Y);
	if (!Msg.IsValid()) { UE_LOG(LogTC, Log, TEXT("ping (%s): not a finite spot, nothing sent"), How); return false; }
	const bool bSent = SendCommand(Msg.ToSharedRef());
	UE_LOG(LogTC, Log, TEXT("ping sent (%s) at (%.1f, %.1f), tick %d: %s"), How, At.X, At.Y, State.Tick, bSent ? TEXT("handed to the socket") : *LastSendProblemText);
	return bSent;
}

void ATCGameMode::AudioFrame(float DeltaSeconds, ATCPlayerController* PC)
{
	// client/main.js frame: audio.listener(cam) (the camera's focus, distance and yaw), battleFrame every 0.2 s, the
	// ambience every 0.4 s, and fx.js update's delayed sounds on the effects' (epilogue-slowed) clock
	UTCAudio* A = Audio();
	if (!A) return;
	A->Frame(DeltaSeconds, State, UnitTable, PC ? PC->ViewFocus() : FVector2D::ZeroVector, PC ? PC->ViewDistance() : 85.f, PC ? PC->ViewYaw() : -90.f);
}

namespace TCSlice10Local
{
	// The densest spot of live effect particles (within 12 m of one another), sampled; false with fewer than MinCount
	bool DensestFx(const TArray<FTCFxParticle>& Live, int32 MinCount, FVector2D& Out, int32& OutCount)
	{
		TArray<FVector2D> Pts;
		for (const FTCFxParticle& P : Live)
			if (P.Look != ETCFxLook::Scorch && P.Look != ETCFxLook::Section && P.Look != ETCFxLook::Plane && P.Look != ETCFxLook::Canopy) Pts.Add(FVector2D(P.P.X, P.P.Y));
		const int32 Step = FMath::Max(1, Pts.Num() / 400);
		int32 Best = -1, BestN = 0;
		for (int32 I = 0; I < Pts.Num(); I += Step)
		{
			int32 N = 0;
			for (int32 J = 0; J < Pts.Num(); J += Step) N += FVector2D::DistSquared(Pts[I], Pts[J]) < 144.0 ? Step : 0;
			if (N > BestN) { BestN = N; Best = I; }
		}
		if (Best < 0 || BestN < MinCount) return false;
		Out = Pts[Best];
		OutCount = BestN;
		return true;
	}
	FString LookCounts(const TArray<FTCFxParticle>& Live)
	{
		static const TCHAR* Names[] = { TEXT("flash"), TEXT("fireball"), TEXT("smoke"), TEXT("dark smoke"), TEXT("screen"), TEXT("dust"), TEXT("clod"), TEXT("spark"), TEXT("debris"),
			TEXT("tracer"), TEXT("rocket"), TEXT("grenade"), TEXT("flame"), TEXT("scorch"), TEXT("plane"), TEXT("canopy"), TEXT("section") };
		static_assert(UE_ARRAY_COUNT(Names) == (int32)ETCFxLook::Count, "a name per look");
		int32 Counts[(int32)ETCFxLook::Count] = {};
		for (const FTCFxParticle& P : Live) Counts[(int32)P.Look]++;
		TArray<FString> Parts;
		for (int32 K = 0; K < (int32)ETCFxLook::Count; K++) if (Counts[K]) Parts.Add(FString::Printf(TEXT("%s %d"), Names[K], Counts[K]));
		return FString::Join(Parts, TEXT(", "));
	}
}

void ATCGameMode::RunSlice10Switches(double Now)
{
	using namespace TCSlice10Local;
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	ATCHUD* Hud = PC ? Cast<ATCHUD>(PC->GetHUD()) : nullptr;
	const bool bDrawn = Hud && Hud->HasDrawn();
	const double InMatch = MatchSeenAt >= 0 && State.bHasMatch ? Now - MatchSeenAt : -1;

	// the sounds asked and started since the last report, every 5 s of a match (the audio evidence in the log)
	static double LastCueReport = 0;
	if (State.bHasMatch && Now - LastCueReport >= 5.0)
	{
		LastCueReport = Now;
		if (UTCAudio* A = Audio())
		{
			const FString R = A->TakeCueReport();
			if (!R.IsEmpty()) UE_LOG(LogTC, Log, TEXT("audio cues at tick %d: %s"), State.Tick, *R);
		}
	}

	// -TCPingAfter: the map ping, once
	if (PingAfter >= 0 && !bPingSent && InMatch >= PingAfter && State.SeatConfirmed() && State.RoomState == TEXT("play") && State.Snapshots > 0 && PC)
	{
		bPingSent = true;
		int32 VX = 0, VY = 0;
		PC->GetViewportSize(VX, VY);
		bool bSent = false;
		if (bDrawn && VX > 0 && VY > 0)
		{
			// the Alt+left press handler at the view's centre pixel (the ground under it, client/main.js groundAt)
			bSent = PC->PingAt(FVector2D(VX / 2.0, VY / 2.0), false);
			UE_LOG(LogTC, Log, TEXT("ping check: Alt+click handler at pixel (%d, %d): %s"), VX / 2, VY / 2, bSent ? TEXT("sent") : TEXT("nothing sent (no ground there)"));
		}
		else
		{
			double CX, CZ;
			if (ArmyIds(CX, CZ).Num()) bSent = SendPing(FVector2D(CX, CZ), TEXT("verification, no viewport: the army's middle"));
			else UE_LOG(LogTC, Log, TEXT("ping check: no viewport and no army: nothing sent"));
		}
	}

	// -TCFxShotAfter: the camera on the densest live effects, then a screenshot
	if (FxShotAfter >= 0 && !bFxShot && InMatch >= FxShotAfter && Effects && PC && bDrawn)
	{
		FVector2D At;
		int32 N = 0;
		if (DensestFx(Effects->Fx.Particles(), 10, At, N))
		{
			bFxShot = true;
			PC->FocusOn(At);
			if (PC->ViewDistance() > 55.f) PC->ZoomTo(55.f);
			FxShotAt = Now + 0.12;
			UE_LOG(LogTC, Log, TEXT("fx shot: camera on (%.1f, %.1f) at tick %d, %d particles there; live: %s; flights in the air %d"), At.X, At.Y, State.Tick, N, *LookCounts(Effects->Fx.Particles()), Effects->Fx.FlightsNow().Num());
		}
	}
	if (FxShotAt > 0 && Now >= FxShotAt)
	{
		FxShotAt = -1;
		const FString File = FxShotFile.IsEmpty() ? FString(TEXT("tc-fx.png")) : FxShotFile;
		FScreenshotRequest::RequestScreenshot(File, true, false);
		UE_LOG(LogTC, Log, TEXT("fx shot: screenshot requested: %s (live: %s; instances drawn %d)"), *File, Effects ? *LookCounts(Effects->Fx.Particles()) : TEXT("-"), Effects ? Effects->InstancesDrawn() : 0);
	}

	// -TCLangToggleAfter: the Language button through the HUD's hit test (the Orders panel shows it in every match)
	if (LangToggleAfter >= 0 && !bLangToggled && InMatch >= LangToggleAfter && PC && bDrawn)
	{
		bLangToggled = true;
		const ETCLanguage Before = FTCLocale::Language();
		const bool bPressed = PC->PressCommand(FTCUiCommand(ETCUiAction::Language));
		UE_LOG(LogTC, Log, TEXT("language check: pressed %s; %s -> %s; the button now reads '%s', the Volume button '%s', the alert history title '%s'"), bPressed ? TEXT("through the hit test") : TEXT("(not drawn: straight to RunCommand)"),
			FTCLocale::Code(Before), FTCLocale::Code(), *TC_T(PC->LanguageLabel()), *TC_T(PC->VolumeLabel()), *TC_T(FString(TEXT("Alert history"))));
		LangShotAt = Now + 1.5;
	}
	if (LangShotAt > 0 && Now >= LangShotAt)
	{
		LangShotAt = -1;
		if (!LangShotFile.IsEmpty()) { FScreenshotRequest::RequestScreenshot(LangShotFile, true, false); UE_LOG(LogTC, Log, TEXT("language check: screenshot requested: %s (language %s)"), *LangShotFile, FTCLocale::Code()); }
	}

	// -TCSettingsShotAfter: the own army selected (directly) so the Orders show their buttons, then a screenshot
	if (SettingsShotAfter >= 0 && !bSettingsShot && InMatch >= SettingsShotAfter && PC && bDrawn && State.Snapshots > 0)
	{
		bSettingsShot = true;
		double CX, CZ;
		const TArray<int32> Ids = ArmyIds(CX, CZ);
		PC->SelectOwn(Ids);
		SettingsShotAt = Now + 1.5;
		UE_LOG(LogTC, Log, TEXT("settings shot: %d own units selected, language %s, buttons will be logged with the shot"), Ids.Num(), FTCLocale::Code());
	}
	if (SettingsShotAt > 0 && Now >= SettingsShotAt)
	{
		SettingsShotAt = -1;
		const FString File = SettingsShotFile.IsEmpty() ? FString(TEXT("tc-settings.png")) : SettingsShotFile;
		FScreenshotRequest::RequestScreenshot(File, true, false);
		TArray<FString> Shown;
		if (Hud) for (const FString& L : Hud->ButtonLabelList()) Shown.Add(TC_T(L));
		UE_LOG(LogTC, Log, TEXT("settings shot: screenshot requested: %s; buttons as drawn (%s): %s"), *File, FTCLocale::Code(), *FString::Join(Shown, TEXT(" | ")));
	}
}
