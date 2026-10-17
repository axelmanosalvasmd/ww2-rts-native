#include "Client/TCPlayerController.h"
#include "Client/TCGraphics.h"
#include "Client/TCPersistence.h"
#include "Misc/App.h"
#include "Client/TCAudio.h"
#include "Client/TCLocale.h"
#include "Client/TCGameMode.h"
#include "Client/TCCameraPawn.h"
#include "Client/TCHUD.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCRecruitment.h"
#include "Client/TCArmyCommands.h"
#include "Client/TCConstruction.h"
#include "Client/TCLogistics.h"
#include "Client/TCSupportOrders.h"
#include "Client/TCAbilities.h"
#include "Client/TCFieldworks.h"
#include "Client/TCCoverPreview.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCPersistence.h"
#include "World/TCTerrain.h"
#include "World/TCWorldObjects.h"
#include "Net/TCMatchState.h"
#include "Net/TCConnection.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "Engine/LocalPlayer.h"
#include "Engine/GameViewportClient.h"
#include "Slate/SceneViewport.h" // UGameViewportClient::Viewport is an FSceneViewport (Engine module)
#include "Dom/JsonObject.h"
#include "Algo/AllOf.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "Policies/CondensedJsonPrintPolicy.h"
#include "InputKeyEventArgs.h"
#include "HAL/PlatformTime.h"
#include "Misc/ConfigCacheIni.h"

namespace
{
	// client/camera.js: E turns the view clockwise seen from above at 1.6 rad/s (TCCameraFeel holds the pan rate, the
	// wheel's step and the 25 m minimum). Without a measured widest view the old 400 m bound holds.
	const float YawRateDegrees = FMath::RadiansToDegrees(TCCameraFeel::YawRateRadians);
	constexpr float MaxDistance = 400.f;
	constexpr float StartDistance = 60.f; // client/main.js receiveStart
	// client/selection.js pick: how near (pixels) a click must land to a unit's drawn point
	constexpr float PickInfantry = 32.f, PickVehicle = 45.f, PickBuilding = 60.f;
	// client/camera.js wide: a 4% margin of the window around the whole map, and never closer than 60 m
	constexpr float FitMargin = 0.04f, MinWide = 60.f;
	// the modes the lobby's Mode button cycles through: the ones this client has been verified to draw
	const TCHAR* const CycledModes[] = { TEXT("conquest"), TEXT("world") };
	// client/keys.js binds the top-row digits only
	const FKey* const DigitKeys[FTCControlGroups::GroupCount] = { &EKeys::One, &EKeys::Two, &EKeys::Three, &EKeys::Four, &EKeys::Five, &EKeys::Six, &EKeys::Seven, &EKeys::Eight, &EKeys::Nine };

	// Why the server would drop an order now (server.js: only a seat in play, unpaused, before a winner)
	const TCHAR* WhyNoOrders(const FTCMatchState& S)
	{
		return S.RoomState != TEXT("play") ? TEXT("the room is not in play")
			: S.bPaused ? TEXT("the match is paused")
			: S.Winner >= -1 ? TEXT("the match is decided")
			: TEXT("this seat is not confirmed by the server yet");
	}

	// client/main.js worldMode: the start's map is a World Conquest map, or the room's mode is 'world'
	bool WorldMode(const FTCMatchState& S) { return S.Map.bWorld || S.Mode == TEXT("world"); }

	// shared/sim.js UNITS with naval: true (the unit table this client loads has no naval column)
	bool IsNaval(const FString& Type) { return Type == TEXT("lcvp") || Type == TEXT("gunboat") || Type == TEXT("destroyer"); }

	TArray<TSharedPtr<FJsonValue>> IdValues(const TArray<int32>& Ids)
	{
		TArray<TSharedPtr<FJsonValue>> Out;
		for (const int32 Id : Ids) Out.Add(MakeShared<FJsonValueNumber>(Id));
		return Out;
	}

	FString WireText(const TSharedRef<FJsonObject>& Msg)
	{
		FString Text;
		const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Text);
		FJsonSerializer::Serialize(Msg, Writer);
		return Text;
	}
}

const TCHAR* LexToString(ETCUiAction Action)
{
	switch (Action)
	{
	case ETCUiAction::AddAi: return TEXT("add AI");
	case ETCUiAction::Tutorial: return TEXT("tutorial");
	case ETCUiAction::MatchControl: return TEXT("match control");
	case ETCUiAction::NextMode: return TEXT("mode");
	case ETCUiAction::Start: return TEXT("start");
	case ETCUiAction::End: return TEXT("end match");
	case ETCUiAction::Stop: return TEXT("stop");
	case ETCUiAction::AttackMove: return TEXT("attack-move");
	case ETCUiAction::Recruit: return TEXT("recruit");
	case ETCUiAction::CancelJob: return TEXT("cancel queued recruit");
	case ETCUiAction::RecruitGroup: return TEXT("recruit group");
	case ETCUiAction::Retreat: return TEXT("retreat");
	case ETCUiAction::Stance: return TEXT("stance");
	case ETCUiAction::Rally: return TEXT("rally");
	case ETCUiAction::Build: return TEXT("build");
	case ETCUiAction::BuildMenu: return TEXT("build menu");
	case ETCUiAction::Resume: return TEXT("resume deliveries");
	case ETCUiAction::SupplyOverlay: return TEXT("supply overlay");
	case ETCUiAction::TruckSelect: return TEXT("truck selection");
	case ETCUiAction::Ability: return TEXT("ability");
	case ETCUiAction::Autocast: return TEXT("autocast");
	case ETCUiAction::MountUp: return TEXT("mount up");
	case ETCUiAction::Unload: return TEXT("unload");
	case ETCUiAction::CancelSite: return TEXT("cancel site");
	case ETCUiAction::Fort: return TEXT("fortification");
	case ETCUiAction::Entrench: return TEXT("entrenchment");
	case ETCUiAction::TrenchMenu: return TEXT("trench menu");
	case ETCUiAction::Formation: return TEXT("formation");
	case ETCUiAction::Support: return TEXT("support");
	case ETCUiAction::Area: return TEXT("shell area");
	case ETCUiAction::CoverToggle: return TEXT("cover preview");
	case ETCUiAction::LobbySetting: return TEXT("lobby setting");
	case ETCUiAction::SaveMatch: return TEXT("save match");
	case ETCUiAction::LoadSave: return TEXT("load save");
	case ETCUiAction::PauseMatch: return TEXT("pause or resume");
	case ETCUiAction::Research: return TEXT("research");
	case ETCUiAction::Recover: return TEXT("restore");
	case ETCUiAction::MatchOption: return TEXT("match option");
	case ETCUiAction::Destroy: return TEXT("destroy");
	case ETCUiAction::Cover: return TEXT("take cover");
	case ETCUiAction::Rail: return TEXT("send by rail");
	case ETCUiAction::Army: return TEXT("select the army");
	case ETCUiAction::Idle: return TEXT("next idle unit");
	case ETCUiAction::IdleAll: return TEXT("all idle units");
	case ETCUiAction::IdleEngineer: return TEXT("next idle Engineer");
	case ETCUiAction::Follow: return TEXT("follow");
	case ETCUiAction::Alert: return TEXT("newest alert");
	case ETCUiAction::Home: return TEXT("home");
	case ETCUiAction::ControlsSheet: return TEXT("controls sheet");
	case ETCUiAction::EdgeScroll: return TEXT("edge scroll");
	case ETCUiAction::PanSpeed: return TEXT("pan speed");
	case ETCUiAction::Language: return TEXT("language");
	case ETCUiAction::Volume: return TEXT("volume");
	case ETCUiAction::Graphics: return TEXT("graphics");
	case ETCUiAction::Gore: return TEXT("gore");
	case ETCUiAction::UiScale: return TEXT("ui scale");
	default: return TEXT("none");
	}
}

const TCHAR* LexToString(ETCAim Aim)
{
	switch (Aim)
	{
	case ETCAim::AttackMove: return TEXT("attack-move");
	case ETCAim::Rally: return TEXT("rally");
	case ETCAim::Build: return TEXT("build");
	case ETCAim::Ability: return TEXT("ability");
	case ETCAim::Field: return TEXT("field works");
	case ETCAim::Support: return TEXT("support");
	case ETCAim::Rail: return TEXT("rail");
	case ETCAim::Spawn: return TEXT("spawn");
	default: return TEXT("none");
	}
}

FString FTCUiCommand::Describe() const
{
	switch (Action)
	{
	case ETCUiAction::Recruit: return FString::Printf(TEXT("recruit %s%s"), *Unit, Building ? *FString::Printf(TEXT(" from %d"), Building) : TEXT(""));
	case ETCUiAction::CancelJob: return FString::Printf(TEXT("cancel job %d at %d"), Job, Building);
	case ETCUiAction::RecruitGroup: return FString::Printf(TEXT("recruit group %d"), Group);
	case ETCUiAction::Build: return FString::Printf(TEXT("build %s"), *Unit);
	case ETCUiAction::Stance: return FString::Printf(TEXT("stance %s"), *Unit);
	case ETCUiAction::Ability: return FString::Printf(TEXT("ability of %s"), *Unit);
	case ETCUiAction::Autocast: return FString::Printf(TEXT("autocast of %s"), *Unit);
	case ETCUiAction::Fort: return FString::Printf(TEXT("fortification %s"), *Unit);
	case ETCUiAction::Entrench: return FString::Printf(TEXT("entrenchment %s"), *Unit);
	case ETCUiAction::Formation: return FString::Printf(TEXT("formation %s"), *Unit);
	case ETCUiAction::Support: return FString::Printf(TEXT("support %s"), *Unit);
	case ETCUiAction::LobbySetting: return FString::Printf(TEXT("lobby %s%s%s"), *Unit, Group >= 0 ? *FString::Printf(TEXT(" of seat %d"), Group) : TEXT(""), Job < 0 ? TEXT(" (previous)") : TEXT(""));
	case ETCUiAction::LoadSave: return FString::Printf(TEXT("load save %s"), *Unit);
	case ETCUiAction::Research: return FString::Printf(TEXT("research %s"), *Unit);
	case ETCUiAction::MatchOption: return FString::Printf(TEXT("option %s%s%s"), *Unit, Group >= 0 ? *FString::Printf(TEXT(" of seat %d"), Group) : TEXT(""), Job < 0 ? TEXT(" (previous)") : TEXT(""));
	case ETCUiAction::MatchControl: return FString::Printf(TEXT("control %s%s%s"), *Unit, Group >= 0 ? *FString::Printf(TEXT(" of seat %d"), Group) : TEXT(""), Job < 0 ? TEXT(" (previous)") : TEXT(""));
	default: return LexToString(Action);
	}
}

static_assert(ATCPlayerController::ServerMessageLimit == FTCConnection::MaxSendBytes, "one message limit");

ATCPlayerController::ATCPlayerController()
{
	bShowMouseCursor = true;
	bEnableClickEvents = false;
	DefaultMouseCursor = EMouseCursor::Crosshairs;
}

void ATCPlayerController::BeginPlay()
{
	Super::BeginPlay();
	FInputModeGameAndUI InputMode;
	InputMode.SetLockMouseToViewportBehavior(EMouseLockMode::DoNotLock);
	InputMode.SetHideCursorDuringCapture(false);
	SetInputMode(InputMode);
	// client/camera.js init: Edge scroll and Pan speed as saved (ww2-edge, ww2-pan; here the user settings file)
	if (GConfig)
	{
		GConfig->GetBool(TEXT("ThreeCrossroads"), TEXT("EdgeScroll"), bEdgeScroll, GGameUserSettingsIni);
		int32 Saved = 1;
		if (GConfig->GetInt(TEXT("ThreeCrossroads"), TEXT("PanSpeed"), Saved, GGameUserSettingsIni) && Saved >= 0 && Saved < TCCameraFeel::PanSpeedCount) PanSpeed = Saved;
	}
	// client/hud.js: the UI scale as saved ('ww2-ui-scale' through the range input's rules); client/gfx.js: the level as
	// saved, its engine side (screen percentage, shadow map) now and on every change (perf.js renderScale, light.js)
	UiScalePercent = FTCSettingsStore::Game().UIScale();
	if (IsLocalController() && GetWorld() && GetWorld()->IsGameWorld())
	{
		FTCGraphics::Get().ApplyEngine();
		GraphicsChangedHandle = FTCGraphics::Get().OnChanged.AddWeakLambda(this, [](ETCGraphicsLevel) { FTCGraphics::Get().ApplyEngine(); });
	}
	UE_LOG(LogTC, Log, TEXT("settings: UI scale %d%%, graphics %s, gore %s"), UiScalePercent, LexToString(FTCGraphics::Get().Level()), FTCGraphics::Get().Gore() ? TEXT("on") : TEXT("off"));
}

ATCGameMode* ATCPlayerController::Mode() const { return GetWorld() ? GetWorld()->GetAuthGameMode<ATCGameMode>() : nullptr; }

bool ATCPlayerController::GroundAt(FVector2D Screen, FVector2D& Out) const
{
	ATCGameMode* M = Mode();
	FVector Origin, Dir;
	if (!M || !M->Terrain || M->Terrain->MapW() <= 0 || !DeprojectScreenPositionToWorld(Screen.X, Screen.Y, Origin, Dir)) return false;
	if (M->Terrain->RayToGround(Origin, Dir, Out)) return true;
	// client/camera.js groundAt: past the map's edge (or over ground this side has not discovered) the ray falls back
	// to the flat plane at height 0, and the order's spots are kept 1 m inside the map (client/main.js formation).
	if (Dir.Z > -1e-6) return false;
	const double T = -Origin.Z / Dir.Z;
	Out = FVector2D((Origin.X + Dir.X * T) / 100.0, (Origin.Y + Dir.Y * T) / 100.0);
	return true;
}

bool ATCPlayerController::CursorPixels(FVector2D& Out, bool bEvenOutside) const
{
	if (bEvenOutside)
	{
		// UGameViewportClient::GetMousePosition refuses negative positions; the viewport itself keeps tracking the
		// cursor past the window's edge while the press holds the capture (FInputModeGameAndUI captures during a press)
		const ULocalPlayer* LP = GetLocalPlayer();
		if (LP && LP->ViewportClient && LP->ViewportClient->Viewport)
		{
			FIntPoint P;
			LP->ViewportClient->Viewport->GetMousePos(P);
			Out = FVector2D(P);
			return true;
		}
	}
	float X, Y;
	if (!GetMousePosition(X, Y)) return false;
	Out = FVector2D(X, Y);
	return true;
}

bool ATCPlayerController::WindowFocused() const
{
	const ULocalPlayer* LP = GetLocalPlayer();
	return LP && LP->ViewportClient && LP->ViewportClient->Viewport && LP->ViewportClient->Viewport->IsForegroundWindow();
}

void ATCPlayerController::StartView(FVector2D Spawn, FVector2D MapMetres)
{
	Focus = Spawn;
	// client/main.js receiveStart: cam.yaw = atan2(sx - MW/2, sz - MH/2). The browser's camera sits at
	// (sin yaw, cos yaw) from what it looks at; this one sits at -(cos Yaw, sin Yaw) (ATCCameraPawn::Place), so
	// Yaw = -90 degrees - yaw. A spawn on the centre gives yaw 0 in both (atan2(0, 0) is 0 in JS and FMath).
	const double BrowserYaw = FMath::Atan2(Spawn.X - MapMetres.X / 2.0, Spawn.Y - MapMetres.Y / 2.0);
	Yaw = -90.f - (float)FMath::RadiansToDegrees(BrowserYaw);
	Distance = StartDistance;
	Wide.Key.Reset();
	bBandStale = true; // round 14: camera.js startIntro measures the band anew
	// client/camera.js startIntro: the glide from the widest view begins once that view is measured (UpdateCamera);
	// a follow, a middle-drag and the zoom's anchor end; selection.reset forgets the idle walk
	Intro = FTCCameraIntro();
	bIntroPending = true;
	CancelFollow();
	bMiddleDrag = false;
	bHeightKnown = false;
	EdgeScroll.Reset();
	IdleCursor.Reset();
	DoubleClick.Reset();
	bPlanWarned = false;
}

void ATCPlayerController::FocusOn(FVector2D Metres)
{
	SkipIntro();
	CancelFollow();
	Focus = Metres;
}

bool ATCPlayerController::SkipIntro()
{
	bIntroPending = false;
	FTCCameraPose P;
	if (!Intro.Skip(P)) return false;
	Focus = P.Focus; TargetHeight = P.Height; Distance = P.Dist;
	return true;
}

void ATCPlayerController::CheckIntroSkip()
{
	bIntroDropRight = bIntroDropWheel = false;
	IntroKeys.Reset();
	if (!Intro.IsActive()) return;
	// every keyboard key and mouse button (client/main.js keydown and introPress); F1 and F2 are read before the skip
	static const TArray<FKey> Keys = []()
	{
		TArray<FKey> All, Out;
		EKeys::GetAllKeys(All);
		for (const FKey& K : All)
			if (K.IsValid() && !K.IsGamepadKey() && !K.IsTouch() && !K.IsGesture() && !K.IsAxis1D() && !K.IsAxis2D() && !K.IsAxis3D() && !K.IsVirtual() && K != EKeys::F1 && K != EKeys::F2) Out.Add(K);
		return Out;
	}();
	bool bAny = false;
	for (const FKey& K : Keys)
		if (WasInputKeyJustPressed(K)) { bAny = true; if (!K.IsMouseButton()) IntroKeys.Add(K); }
	bIntroDropWheel = WasInputKeyJustPressed(EKeys::MouseScrollUp) || WasInputKeyJustPressed(EKeys::MouseScrollDown);
	bIntroDropRight = WasInputKeyJustPressed(EKeys::RightMouseButton);
	if (!bAny && !bIntroDropWheel) return;
	SkipIntro();
	// a key that ended the glide is not acted on, nor held for the camera until it is pressed again
	for (const FKey& K : IntroKeys) ChordKeys.Add(K);
	UE_LOG(LogTC, Log, TEXT("camera intro skipped by %s"), IntroKeys.Num() ? TEXT("a key") : bIntroDropRight ? TEXT("a right press (dropped)") : bIntroDropWheel ? TEXT("the wheel (dropped)") : TEXT("a press"));
}

void ATCPlayerController::UpdateWideView(const ATCGameMode& M, int32 VX, int32 VY)
{
	// client/camera.js wide(): the distance and target at which the whole map, at this heading, fits the window with a
	// 4% margin on every side and sits in its middle. Measured again when the map, the drawn ground, the heading or
	// the window changes. The browser fits the clear band between its HUD panels; this HUD has no panels across the
	// view, so the band is the whole window.
	const ATCTerrain& T = *M.Terrain;
	const float W = T.MapW() * FTCMatchState::CellMetres, H = T.MapH() * FTCMatchState::CellMetres;
	if (W <= 0 || H <= 0 || VX <= 0 || VY <= 0) { Wide = FWideView(); return; }
	// round 14: the band as last measured (camera.js band(); a resize measures it again)
	if (bBandStale || BandViewport != FIntPoint(VX, VY)) TakeBand(VX, VY);
	const FString Key = FString::Printf(TEXT("%g,%g,%g,%d,%d,%d,%g,%g"), W, H, Yaw, VX, VY, T.CellsDrawn(), Band.X, Band.Y);
	if (Key == Wide.Key) return;
	const FVector Corners[4] = {
		FVector(0, 0, T.HeightAt(0, 0)), FVector(W, 0, T.HeightAt(W - 0.01f, 0)),
		FVector(W, H, T.HeightAt(W - 0.01f, H - 0.01f)), FVector(0, H, T.HeightAt(0, H - 0.01f)) };
	const FVector Middle(W / 2, H / 2, T.HeightAt(W / 2, H / 2));
	const double Aspect = double(VX) / VY, RoomX = VX * (1 - 2 * FitMargin), RoomY = (Band.Y - Band.X) - 2.0 * VY * FitMargin;
	struct FBox { bool bOk = true; double X0 = 1e30, X1 = -1e30, Y0 = 1e30, Y1 = -1e30; };
	auto Box = [&](double Dist)
	{
		const FTCPinhole V(Middle, Yaw, Pitch, Dist, Aspect);
		FBox B;
		for (const FVector& C : Corners)
		{
			FVector2D S;
			if (!V.Project(C, VX, VY, S)) { B.bOk = false; continue; }
			B.X0 = FMath::Min(B.X0, S.X); B.X1 = FMath::Max(B.X1, S.X); B.Y0 = FMath::Min(B.Y0, S.Y); B.Y1 = FMath::Max(B.Y1, S.Y);
		}
		return B;
	};
	double Lo = 20, Hi = FMath::Max(800.0, FMath::Sqrt(double(W) * W + double(H) * H) * 3);
	for (int32 I = 0; I < 24; I++)
	{
		const double Mid = (Lo + Hi) / 2;
		const FBox B = Box(Mid);
		if (B.bOk && B.X1 - B.X0 <= RoomX && B.Y1 - B.Y0 <= RoomY) Hi = Mid; else Lo = Mid;
	}
	const double Dist = FMath::Max<double>(MinWide, Hi);
	const FBox B = Box(Dist);
	// slide the target so the box's middle lands on the band's middle (perspective puts the near edge lower):
	// camera.js shift(middle, innerWidth - boxMidX, innerHeight / 2 + bandMid - boxMidY)
	FVector2D Hit;
	FVector2D Target(Middle.X, Middle.Y);
	if (FTCPinhole(Middle, Yaw, Pitch, Dist, Aspect).Ground(FVector2D(VX - (B.X0 + B.X1) / 2, VY / 2.0 + (Band.X + Band.Y) / 2.0 - (B.Y0 + B.Y1) / 2), VX, VY, Middle.Z, Hit))
		Target += FVector2D(Middle.X, Middle.Y) - Hit;
	Wide.bValid = true;
	Wide.Dist = (float)Dist;
	Wide.Focus = FVector2D(FMath::Clamp(Target.X, 0.0, (double)W), FMath::Clamp(Target.Y, 0.0, (double)H));
	Wide.Key = Key;
}

FString ATCPlayerController::FramingCheck() const
{
	ATCGameMode* M = Mode();
	int32 VX = 0, VY = 0;
	GetViewportSize(VX, VY);
	if (!M || !M->Terrain || M->Terrain->MapW() <= 0 || VX <= 0 || VY <= 0) return TEXT("no map or viewport");
	const ATCTerrain& T = *M->Terrain;
	const float W = T.MapW() * FTCMatchState::CellMetres, H = T.MapH() * FTCMatchState::CellMetres;
	const FTCPinhole V(FVector(Focus.X, Focus.Y, bHeightKnown ? TargetHeight : T.HeightAt(Focus.X, Focus.Y)), Yaw, Pitch, Distance, double(VX) / VY);
	FString Out = FString::Printf(TEXT("viewport %dx%d"), VX, VY);
	const FVector2D Corners[4] = { FVector2D(0, 0), FVector2D(W, 0), FVector2D(W, H), FVector2D(0, H) };
	for (const FVector2D& C : Corners)
	{
		const float Z = T.HeightAt(FMath::Min(C.X, W - 0.01f), FMath::Min(C.Y, H - 0.01f));
		FVector2D E(-1, -1), Mo(-1, -1);
		ProjectWorldLocationToScreen(ATCTerrain::ToWorld(C.X, C.Y, Z), E);
		V.Project(FVector(C.X, C.Y, Z), VX, VY, Mo);
		Out += FString::Printf(TEXT(" | (%.0f,%.0f) engine (%.0f,%.0f) model (%.0f,%.0f)"), C.X, C.Y, E.X, E.Y, Mo.X, Mo.Y);
	}
	return Out;
}

void ATCPlayerController::ClampFocus(const ATCGameMode& M)
{
	if (!M.Terrain || M.Terrain->MapW() <= 0) return;
	const float W = M.Terrain->MapW() * FTCMatchState::CellMetres, H = M.Terrain->MapH() * FTCMatchState::CellMetres;
	if (Wide.bValid)
	{
		// client/camera.js clamp: the target stays on the map; zoomed out past half the widest view it is drawn toward
		// the widest view's target, so the widest view is the whole map, centred, not a corner of it with the land past
		// the edge filling the rest
		const float Free = FMath::Clamp(2.f * (1.f - Distance / Wide.Dist), 0.f, 1.f);
		Focus.X = FMath::Min(Wide.Focus.X + (W - Wide.Focus.X) * Free, FMath::Max(Wide.Focus.X * (1.f - Free), (float)Focus.X));
		Focus.Y = FMath::Min(Wide.Focus.Y + (H - Wide.Focus.Y) * Free, FMath::Max(Wide.Focus.Y * (1.f - Free), (float)Focus.Y));
	}
	else
	{
		Focus.X = FMath::Clamp(Focus.X, 0.f, W);
		Focus.Y = FMath::Clamp(Focus.Y, 0.f, H);
	}
}

void ATCPlayerController::FrameOn(FVector2D Metres)
{
	// client/camera.js frame: the point in the middle of the clear band, no farther out than the widest view. This
	// HUD lays no panels across the view, so the band is the whole window and its middle is the target itself.
	ATCGameMode* M = Mode();
	Anchor.bActive = false;
	Focus = Metres;
	// round 14: camera.js frame: band(true), then the point in the band's middle (two passes: the target's height
	// follows the ground under it, which moves in the first)
	int32 VX = 0, VY = 0;
	GetViewportSize(VX, VY);
	if (M && M->Terrain && M->Terrain->MapW() > 0 && VX > 0 && VY > 0) { TakeBand(VX, VY); UpdateWideView(*M, VX, VY); }
	if (Wide.bValid) Distance = FMath::Min(Distance, Wide.Dist);
	if (M && M->Terrain)
	{
		const FVector P(Metres.X, Metres.Y, M->Terrain->HeightAt(Metres.X, Metres.Y));
		for (int32 Pass = 0; Pass < 2; Pass++)
		{
			TargetHeight = M->Terrain->HeightAt(Focus.X, Focus.Y);
			FVector2D NewFocus;
			if (VX > 0 && VY > 0 && TCCameraFeel::ShiftToPixel(Focus, TargetHeight, Yaw, Pitch, Distance, VX, VY, P, FVector2D(VX / 2.0, (Band.X + Band.Y) / 2.0), NewFocus)) Focus = NewFocus;
			ClampFocus(*M);
		}
		TargetHeight = M->Terrain->HeightAt(Focus.X, Focus.Y);
		bHeightKnown = true;
	}
}

void ATCPlayerController::TakeBand(int32 VX, int32 VY)
{
	// camera.js band(): the HUD's band, or the whole view when it leaves less than 30% of the height
	const float Top = FMath::Max(0.f, (float)LiveBand.X), Bottom = LiveBand.Y > 0 ? FMath::Min((float)VY, (float)LiveBand.Y) : (float)VY;
	Band = Bottom - Top >= VY * 0.3f ? FVector2D(Top, Bottom) : FVector2D(0, VY);
	BandViewport = FIntPoint(VX, VY);
	bBandStale = false;
}

void ATCPlayerController::SolveAnchor(int32 VX, int32 VY)
{
	ATCGameMode* M = Mode();
	if (!Anchor.bActive || !M || VX <= 0 || VY <= 0) return;
	FVector2D NewFocus;
	if (TCCameraFeel::ShiftToPixel(Focus, TargetHeight, Yaw, Pitch, Distance, VX, VY, Anchor.Point, Anchor.Pixel, NewFocus))
	{
		Focus = NewFocus;
		ClampFocus(*M);
	}
}

void ATCPlayerController::WheelZoom(int32 Notch, bool bHasCursor, FVector2D Mouse, int32 VX, int32 VY)
{
	// client/camera.js wheel: the ground under the cursor (seen at the pose drawn last frame), one 10% step, then that
	// point held under the cursor for 0.4 s while the target's height settles. A follow keeps the unit centred instead.
	ATCGameMode* M = Mode();
	FVector2D G;
	const bool bPoint = bHasCursor && GroundAt(Mouse, G);
	// round 14: camera.js wheel: band(true), the panels may have changed height since the last zoom
	if (M && M->Terrain && M->Terrain->MapW() > 0) { TakeBand(VX, VY); UpdateWideView(*M, VX, VY); }
	const float Farthest = Wide.bValid ? Wide.Dist : MaxDistance;
	Distance = TCCameraFeel::ZoomedDistance(Distance, Notch, FMath::Max(TCCameraFeel::MinDistance, Farthest));
	Anchor.bActive = false;
	if (!bPoint || FollowId || !M || !M->Terrain || M->Terrain->MapW() <= 0) return;
	const float W = M->Terrain->MapW() * FTCMatchState::CellMetres, H = M->Terrain->MapH() * FTCMatchState::CellMetres;
	// GroundAt falls back to the flat plane at 0 past the map's edge, as client/camera.js groundAt does
	const bool bOnMap = G.X >= 0 && G.Y >= 0 && G.X <= W && G.Y <= H;
	Anchor.bActive = true;
	Anchor.Point = FVector(G.X, G.Y, bOnMap ? M->Terrain->HeightAt(FMath::Min((float)G.X, W - 0.01f), FMath::Min((float)G.Y, H - 0.01f)) : 0.f);
	Anchor.Pixel = Mouse;
	Anchor.Left = TCCameraFeel::AnchorSeconds;
	SolveAnchor(VX, VY);
}

void ATCPlayerController::UpdateCamera(float Dt, bool bFocused)
{
	ATCGameMode* M = Mode();
	ATCCameraPawn* Cam = Cast<ATCCameraPawn>(GetPawn());
	EdgeDir.Reset();
	if (!M || !Cam || !M->Terrain) return;
	// round 14: in the lobby the backdrop's drifting camera (client/lobby-view.js place), else the match's 42 degrees
	if (!M->State.bHasMatch && M->Backdrop.PlaceCamera(Cam)) return;
	M->Backdrop.RestoreCamera(Cam);
	const ATCTerrain& T = *M->Terrain;
	// client/main.js keydown: a key whose press ran a shortcut (KeyChord) is not held for the camera. The press itself
	// is read after the camera moves this frame (UpdateCommandKeys), so a key starts panning or turning one frame
	// after it goes down, never on a press that a shortcut takes. The Controls sheet takes every key while open.
	for (auto It = ChordKeys.CreateIterator(); It; ++It) if (!IsInputKeyDown(*It)) It.RemoveCurrent();
	auto Held = [this](const FKey& K) { return !ModalSheetOpen() && IsInputKeyDown(K) && !WasInputKeyJustPressed(K) && !ChordKeys.Contains(K); };
	int32 VX = 0, VY = 0;
	GetViewportSize(VX, VY);
	if (T.MapW() > 0) UpdateWideView(*M, VX, VY); else Wide = FWideView();
	if (!bHeightKnown) { TargetHeight = T.HeightAt(Focus.X, Focus.Y); bHeightKnown = true; }

	// The pointer as edge scrolling reads it (client/pointer.js): only while this window has the focus, only once the
	// cursor has been seen moving (or pressing) in it, and pinned to the border it left across. The viewport keeps
	// tracking a captured drag past the window's edge; an uncaptured cursor that left reads (-1, -1).
	FVector2D Raw;
	const bool bRaw = bFocused && CursorPixels(Raw, true) && !Raw.Equals(FVector2D(-1, -1));
	const bool bPressed = WasInputKeyJustPressed(EKeys::LeftMouseButton) || WasInputKeyJustPressed(EKeys::RightMouseButton) || WasInputKeyJustPressed(EKeys::MiddleMouseButton);
	PointerEdge.Frame(bFocused, bRaw, Raw, VX, VY, bPressed);
	FVector2D Mouse;
	const bool bHasCursor = bFocused && CursorPixels(Mouse, false);
	const ATCHUD* Hud = Cast<ATCHUD>(GetHUD());
	auto OverView = [Hud](FVector2D P) { return !Hud || !(Hud->PanelAt(P) || Hud->Minimap.Contains(P)); };
	const bool bOverView = bHasCursor && OverView(Mouse);

	// client/camera.js startIntro and update: from the widest view to the start view over 2.5 s, nothing else moving
	// the camera meanwhile. It begins once the widest view is measured for this map and window.
	if (bIntroPending && Wide.bValid)
	{
		bIntroPending = false;
		// client/main.js receiveStart: frame(sx, sz), then the sweep from the whole map
		Distance = FMath::Min(Distance, Wide.Dist);
		ClampFocus(*M);
		TargetHeight = T.HeightAt(Focus.X, Focus.Y);
		FTCCameraPose From, To;
		To.Focus = Focus; To.Height = TargetHeight; To.Dist = Distance;
		From.Focus = Wide.Focus; From.Height = T.HeightAt(Wide.Focus.X, Wide.Focus.Y); From.Dist = FMath::Max(Wide.Dist, Distance);
		Intro.Begin(From, To);
		UE_LOG(LogTC, Log, TEXT("camera intro: from the widest view (%.1f, %.1f) at %.0f m to the start view (%.1f, %.1f) at %.0f m over %.1f s"),
			From.Focus.X, From.Focus.Y, From.Dist, To.Focus.X, To.Focus.Y, To.Dist, TCCameraFeel::IntroSeconds);
	}
	if (Intro.IsActive())
	{
		FTCCameraPose P;
		Intro.Step(Dt, P);
		Focus = P.Focus; TargetHeight = P.Height; Distance = P.Dist;
		EdgeScroll.Reset();
		Cam->Place(Focus, TargetHeight, Yaw, Pitch, Distance);
		return;
	}

	// the wheel over the view (the HUD's panels keep their own wheel; a notch that ended the intro is dropped)
	const int32 Notch = (WasInputKeyJustPressed(EKeys::MouseScrollDown) ? 1 : 0) - (WasInputKeyJustPressed(EKeys::MouseScrollUp) ? 1 : 0);
	if (Notch && bHasCursor && !bIntroDropWheel && !ModalSheetOpen()) if (ATCHUD* H=Cast<ATCHUD>(GetHUD())) H->ScrollSelection(Mouse,Notch);
	if (Notch && M->StatsSheet.IsOpen() && !bControlsSheetOpen) M->StatsSheet.Scroll(Notch * 48.f); // round 16: the stats sheet scrolls
	if (Notch && !bIntroDropWheel && !ModalSheetOpen() && (!bHasCursor || bOverView)) WheelZoom(Notch, bOverView, Mouse, VX, VY);

	// client/camera.js beginMiddle, moveMiddle, stopDrag: a middle press on the view turns it while held
	if (bMiddleDrag && (!IsInputKeyDown(EKeys::MiddleMouseButton) || !bFocused || ModalSheetOpen())) { bMiddleDrag = false; Anchor.bActive = false; }
	if (!bMiddleDrag && WasInputKeyJustPressed(EKeys::MiddleMouseButton) && bOverView && !ModalSheetOpen())
	{
		bMiddleDrag = true;
		MiddleX = Mouse.X;
		Anchor.bActive = false;
	}
	else if (bMiddleDrag)
	{
		FVector2D Now;
		if (CursorPixels(Now, true) && !Now.Equals(FVector2D(-1, -1)) && Now.X != MiddleX)
		{
			Yaw += TCCameraFeel::MiddleDragYawDegrees(Now.X - MiddleX);
			MiddleX = Now.X;
			Anchor.bActive = false;
		}
	}

	FVector2D Pan(0, 0);
	const bool bW = Held(EKeys::W) || Held(EKeys::Up), bS = Held(EKeys::S) || Held(EKeys::Down);
	const bool bD = Held(EKeys::D) || Held(EKeys::Right), bA = Held(EKeys::A) || Held(EKeys::Left);
	Pan.X = (bW ? 1 : 0) - (bS ? 1 : 0);
	Pan.Y = (bD ? 1 : 0) - (bA ? 1 : 0);
	// client/camera.js edgePush: no push while a button drags, a gesture or a rotation runs or the sheet is open
	FTCEdgeInput In;
	In.bEnabled = bEdgeScroll;
	In.bActive = PointerEdge.Active();
	In.bOut = PointerEdge.Out();
	In.bOverView = PointerEdge.Out() || OverView(PointerEdge.Pos);
	In.bWorld = M->State.bHasMatch;
	In.bBlocked = ModalSheetOpen();
	In.bButtons = IsInputKeyDown(EKeys::LeftMouseButton) || IsInputKeyDown(EKeys::MiddleMouseButton);
	In.bDragging = bDragging || Formation.IsGesturing();
	In.bMiddle = bMiddleDrag;
	In.Pos = PointerEdge.Pos;
	In.W = VX; In.H = VY;
	const FTCEdgePush Push = EdgeScroll.Step(In, Dt, FollowId != 0);
	EdgeDir = Push.Dir;
	Pan.X = FMath::Clamp(Pan.X + Push.Fw, -1.0, 1.0);
	Pan.Y = FMath::Clamp(Pan.Y + Push.Rt, -1.0, 1.0);
	// any camera pan ends a follow; a followed unit that is no longer held (dead, or out of sight) ends it too
	if (bW || bS || bD || bA || Pan.X != 0 || Pan.Y != 0) CancelFollow();
	if (FollowId)
	{
		const FTCUnit* U = M->State.Units.Find(FollowId);
		FVector2D At;
		if (!U) CancelFollow();
		else
		{
			if (!M->Units || !M->Units->DrawnAt(FollowId, At)) At = FVector2D(U->X, U->Z);
			Focus = At;
			Anchor.bActive = false;
		}
	}
	const float Rad = FMath::DegreesToRadians(Yaw);
	const FVector2D Fwd(FMath::Cos(Rad), FMath::Sin(Rad)), Right(-FMath::Sin(Rad), FMath::Cos(Rad));
	Focus += (Fwd * Pan.X + Right * Pan.Y) * Distance * TCCameraFeel::PanRate * TCCameraFeel::PanSpeedFactor(PanSpeed) * Dt;
	// client/camera.js wheel bounds: no closer than 25 m, no farther than the widest view (the whole map); without a
	// measured view (no map, or no window) the old 400 m bound holds
	const float Farthest = Wide.bValid ? Wide.Dist : MaxDistance;
	Distance = FMath::Clamp(Distance, TCCameraFeel::MinDistance, FMath::Max(TCCameraFeel::MinDistance, Farthest));
	ClampFocus(*M);
	// The browser's yaw grows with E; this Yaw is -90 minus that (StartView), so E lowers it.
	// client/keys.js rotateLeft and rotateRight are Q and E with no modifier; Shift+Q and Shift+E are Mount up and
	// Unload (their press is a chord key above)
	if (Held(EKeys::Q)) Yaw += YawRateDegrees * Dt;
	if (Held(EKeys::E)) Yaw -= YawRateDegrees * Dt;
	// client/camera.js update: the target's height follows the ground under it, eased
	TargetHeight = TCCameraFeel::GlideHeight(TargetHeight, T.HeightAt(Focus.X, Focus.Y), Dt);
	if (Anchor.bActive)
	{
		SolveAnchor(VX, VY);
		Anchor.Left -= Dt;
		if (Anchor.Left <= 0) Anchor.bActive = false;
	}
	Cam->Place(Focus, TargetHeight, Yaw, Pitch, Distance);
}

void ATCPlayerController::PlayerTick(float DeltaTime)
{
	Super::PlayerTick(DeltaTime);
	// client/light.js renderFrame: watchFps(min(0.1, dt), notice) on every frame drawn over a match's ground
	if (ATCGameMode* GM = Mode())
		if (GM->State.bHasMatch && GM->Terrain)
		{
			FString Notice;
			if (FTCGraphics::Get().WatchFps(FTCGraphics::ClampFrame(FApp::GetDeltaTime()), &Notice)) { GraphicsNotice = Notice; GraphicsNoticeAt = GetWorld()->GetRealTimeSeconds(); }
		}
	const bool bFocused = WindowFocused();
	CheckIntroSkip();
	UpdateCamera(DeltaTime, bFocused);
	ATCGameMode* M = Mode();
	HoverEnemy = 0;
	if (M) Groups.Sync(M->State); // a new match or seat empties the groups, a reconnect to the same match keeps them
	if (M && M->State.bHasMatch && M->Units && M->Terrain)
	{
		PruneSelection(*M);
		UpdatePointer(bFocused);
		UpdateGroupKeys();
		// client/main.js frame with a dig or entrench aim: the marker or cells under the pointer and the hint, once a
		// frame (it keeps the aim mesh's state as the browser's frame does)
		if (Aim == ETCAim::Field && FieldAim.IsArmed())
		{
			FieldPreview = M->Field.Preview(M->State, FieldAim, bPointerGround, PointerGround, Selected, PositionOf());
			AimHint = FieldAim.Hint;
		}
		else FieldPreview = FTCFieldPreview();
		// client/main.js frame: the support or area aim's mesh under the pointer (Preview also drops it when no such aim
		// is armed), hidden while a facing gesture has set a facing; then the formation preview of the gesture
		PowerPreview = M->Powers.Preview(PowerAim, bPointerGround, PointerGround);
		if (Aim == ETCAim::Support) AimHint = PowerAim.Hint;
		FacingPreview = Formation.Preview(M->State, M->UnitTable, Selected);
		if (FacingPreview.bVisible) PowerPreview.bShown = false;
		// client/cover-preview.js frame (dt capped at 0.1 s): the marks around the pointer while own infantry is selected
		FTCCoverKnown Known;
		GatherCover(Known);
		FTCCoverPointer P;
		FVector2D Mouse;
		const bool bIn = CursorPixels(Mouse, false);
		const ATCHUD* Hud = Cast<ATCHUD>(GetHUD());
		// a pinned pointer (verification, no mouse offscreen) counts as a pointer over the view
		P.bInside = bPointerPinned || (bFocused && bIn);
		P.bOverView = bPointerPinned || (bIn && Hud && !Hud->PanelAt(Mouse) && !Hud->Minimap.Contains(Mouse));
		P.bLeftDown = IsInputKeyDown(EKeys::LeftMouseButton);
		P.bAimAllows = Aim == ETCAim::None || Aim == ETCAim::AttackMove;
		P.bGroundHit = bPointerGround;
		P.Ground = PointerGround;
		M->Cover.Frame(FMath::Min(DeltaTime, 0.1f), Known, P);
	}
	else
	{
		CancelDrag();
		CancelAim();
		Formation.CancelGesture();
		FieldPreview = FTCFieldPreview();
		PowerPreview = FTCSupportPreview();
		FacingPreview = FTCFormationPreview();
		bBuildMenuOpen = bTrenchMenuOpen = false;
		bPointerGround = false;
		RecruitAutoFor = 0;
		bControlsSheetOpen = false; // client/main.js: the sheet belongs to the match's HUD
		CancelFollow();
		UpdateCommandKeys(); // round 15: client/main.js keydown runs in the lobby too (M mutes there)
		FVector2D Mouse;
		if (WasInputKeyJustPressed(EKeys::LeftMouseButton) && CursorPixels(Mouse, false)) HandleHudPress(true, false, Mouse); // the lobby's buttons
	}
	bWasFocused = bFocused;
}

void ATCPlayerController::PruneSelection(const ATCGameMode& M)
{
	// client/main.js applySnapshot and client/selection.js rows: a unit that left the held set (dead, or hidden by
	// fog), is not ours, has no hp left or climbed into a carrier is no longer selected. Right after a start nothing is
	// held yet, so a reconnect to the same match keeps its selection until the first snapshot says otherwise.
	for (auto It = Selected.CreateIterator(); It; ++It)
	{
		const FTCUnit* U = M.State.Units.Find(*It);
		if (U ? !M.State.IsOwn(*U) || U->Hp <= 0 || (U->Flags & TCFlags::Riding) : M.State.Snapshots > 0) It.RemoveCurrent();
	}
	// client/main.js: the attack-move aim needs a selection. A rally aim does not (outside Classic it sets the army's
	// rally point), and a building's aim stays armed: its click then explains that no builder is selected.
	if (!Selected.Num() && Aim == ETCAim::AttackMove) { Aim = ETCAim::None; AimHint.Reset(); }
}

bool ATCPlayerController::HandleHudPress(bool bLeft, bool bRight, FVector2D Mouse)
{
	const ATCHUD* Hud = Cast<ATCHUD>(GetHUD());
	if (!Hud || (!bLeft && !bRight)) return false;
	FTCUiCommand Command;
	if (bLeft && Hud->CommandAt(Mouse, Command))
	{
		RunCommand(Command, IsInputKeyDown(EKeys::LeftShift) || IsInputKeyDown(EKeys::RightShift));
		return true;
	}
	// client/hud.js drawOrders: a right click on an ability button switches its autocast (oncontextmenu)
	if (bRight && Hud->CommandAt(Mouse, Command) && Command.Action == ETCUiAction::Ability)
	{
		FTCUiCommand Auto(ETCUiAction::Autocast);
		Auto.Unit = Command.Unit;
		RunCommand(Auto);
		return true;
	}
	// a right click on a lobby setting's button steps back (the browser's select lists every choice; this HUD cycles)
	if (bRight && Hud->CommandAt(Mouse, Command) && (Command.Action == ETCUiAction::LobbySetting || Command.Action == ETCUiAction::MatchControl || Command.Action == ETCUiAction::Volume || Command.Action == ETCUiAction::UiScale || (Command.Action == ETCUiAction::MatchOption && Command.Unit == LexToString(ETCMatchOption::Weather))))
	{
		Command.Job = -1;
		RunCommand(Command);
		return true;
	}
	// a press on a panel's background (or a right press on another button) stops there, as on the browser's HUD
	return Hud->PanelAt(Mouse) || Hud->CommandAt(Mouse, Command);
}

bool ATCPlayerController::InputKey(const FInputKeyEventArgs& Params)
{
	// round 15: a lobby or directory text field being typed in takes the key presses (Client/TCMatchControls.cpp)
	if (Params.Event == IE_Pressed || Params.Event == IE_Repeat || Params.Event == IE_Released)
		if (ATCGameMode* M = Mode())
			if (M->ControlsKey(Params.Key, Params.Event != IE_Released, IsInputKeyDown(EKeys::LeftShift) || IsInputKeyDown(EKeys::RightShift))) return true;
	return Super::InputKey(Params);
}

bool ATCPlayerController::PressButton(ETCUiAction Action)
{
	return PressCommand(FTCUiCommand(Action));
}

bool ATCPlayerController::PressCommand(const FTCUiCommand& Command, bool bShift)
{
	SkipIntro();
	const ATCHUD* Hud = Cast<ATCHUD>(GetHUD());
	FVector2D At;
	FTCUiCommand Hit;
	if (Hud && Hud->CommandCentre(Command, At) && Hud->CommandAt(At, Hit) && Hit.Action == Command.Action)
	{
		UE_LOG(LogTC, Log, TEXT("button '%s' pressed at its drawn centre (%.0f, %.0f)%s; buttons on screen: %s"), *Hit.Describe(), At.X, At.Y, bShift ? TEXT(" with Shift") : TEXT(""), *Hud->ButtonLabels());
		return RunCommand(Hit, bShift);
	}
	UE_LOG(LogTC, Log, TEXT("button '%s': not drawn this frame (no renderer or not shown), run directly"), *Command.Describe());
	return RunCommand(Command, bShift);
}

bool ATCPlayerController::RightPressCommand(const FTCUiCommand& Command)
{
	SkipIntro();
	const ATCHUD* Hud = Cast<ATCHUD>(GetHUD());
	FVector2D At;
	FTCUiCommand Hit;
	if (Hud && Hud->CommandCentre(Command, At) && Hud->CommandAt(At, Hit) && Hit.Action == Command.Action)
	{
		UE_LOG(LogTC, Log, TEXT("button '%s' right-pressed at its drawn centre (%.0f, %.0f)"), *Hit.Describe(), At.X, At.Y);
		return HandleHudPress(false, true, At);
	}
	UE_LOG(LogTC, Log, TEXT("button '%s': not drawn this frame (no renderer or not shown), its right click run directly"), *Command.Describe());
	if (Command.Action == ETCUiAction::LobbySetting || (Command.Action == ETCUiAction::MatchOption && Command.Unit == LexToString(ETCMatchOption::Weather)))
	{
		FTCUiCommand Back = Command;
		Back.Job = -1;
		return RunCommand(Back);
	}
	if (Command.Action != ETCUiAction::Ability) return false;
	FTCUiCommand Auto(ETCUiAction::Autocast);
	Auto.Unit = Command.Unit;
	return RunCommand(Auto);
}

void ATCPlayerController::UpdatePointer(bool bFocused)
{
	ATCGameMode* M = Mode();
	const bool bAdd = IsInputKeyDown(EKeys::LeftShift) || IsInputKeyDown(EKeys::RightShift);
	const bool bCtrl = IsInputKeyDown(EKeys::LeftControl) || IsInputKeyDown(EKeys::RightControl);
	FVector2D Mouse;
	const bool bInside = CursorPixels(Mouse, false);
	if (bInside && M) HoverEnemy = PickAt(Mouse, [M](const FTCUnit& U, const FTCUnitDef&) { return M->State.IsFoe(U.Owner); });
	if (!bPointerPinned) bPointerGround = bInside && GroundAt(Mouse, PointerGround);

	UpdateCommandKeys();

	// A press goes to the minimap first (client/main.js: its mousedown stops the event), then the HUD's buttons and
	// panels, and only then to the world. A box drag in progress keeps its own release.
	const bool bLeftPress = WasInputKeyJustPressed(EKeys::LeftMouseButton) && bInside && !bDragging;
	// client/camera.js introPress: a right press that ended the intro is dropped (its order was aimed at the glide)
	const bool bRightPress = WasInputKeyJustPressed(EKeys::RightMouseButton) && bInside && !bIntroDropRight;
	if (bRightPress) if (ATCHUD* H=Cast<ATCHUD>(GetHUD())) H->StopRecruiting();
	// round 16: the stats sheet is modal (showModal): a press reaches its controls or, outside it, closes it
	if (M && M->StatsSheet.IsOpen() && !bControlsSheetOpen)
	{
		if (bLeftPress) M->StatsSheetPress(Mouse, false);
		if (bRightPress) M->StatsSheetPress(Mouse, true);
		return;
	}
	if (bControlsSheetOpen)
	{
		// round 14: the onboarding sheet closes on any press on it, and that press goes no further
		if ((bLeftPress || bRightPress) && OnboardingPress()) return;
		// the Controls sheet is modal (client/main.js showModal): presses reach only the HUD (its Close), never the
		// minimap or the world
		if (bLeftPress) HandleHudPress(true, false, Mouse);
		if (bRightPress) HandleHudPress(false, true, Mouse);
		return;
	}
	bool bTakenLeft = false, bTakenRight = false;
	// client/main.js minimap mousedown: button 0 with Alt (and no intro) pings there, before any aim or camera move
	const bool bAltDown = IsInputKeyDown(EKeys::LeftAlt) || IsInputKeyDown(EKeys::RightAlt);
	if (bLeftPress && bAltDown && !IntroActive())
		if (const ATCHUD* PingHud = Cast<ATCHUD>(GetHUD()))
			if (PingHud->Minimap.Contains(Mouse)) { PingAt(Mouse, true); bTakenLeft = true; }
	if (bLeftPress && !bTakenLeft) bTakenLeft = MinimapPress(Mouse, false, bCtrl, bAdd).bConsumed;
	if (bRightPress) bTakenRight = MinimapPress(Mouse, true, bCtrl, bAdd).bConsumed;
	if (ATCHUD* Hud = Cast<ATCHUD>(GetHUD()))
	{
		if (Hud->Minimap.IsDragging())
		{
			// client/main.js mousemove: the camera follows the pointer while the left button stays down; blur ends it
			FVector2D Now;
			FVector2D Ground;
			if (bWasFocused && !bFocused) Hud->Minimap.CancelDrag();
			else if (CursorPixels(Now, true) && Hud->Minimap.MouseMove(Now, IsInputKeyDown(EKeys::LeftMouseButton), &Ground)) FocusOn(Ground);
		}
	}
	if (!bTakenLeft && bLeftPress) bTakenLeft = HandleHudPress(true, false, Mouse);
	if (!bTakenRight && bRightPress) bTakenRight = HandleHudPress(false, true, Mouse);
	// client/main.js mousedown on the view: a facing gesture in progress ends first
	if ((!bTakenLeft && bLeftPress) || (!bTakenRight && bRightPress)) Formation.CancelGesture();
	// client/main.js mousedown on the view: button 0 with Alt, no aim and no intro: pings.send at the ground there
	if (!bTakenLeft && bLeftPress && bAltDown && Aim == ETCAim::None && !IntroActive()) { PingAt(Mouse, false); bTakenLeft = true; }
	if (!bTakenLeft && bLeftPress)
	{
		// the armed attack-move's click starts a facing gesture on the ground under it (beginFacing(.., true))
		if (Aim == ETCAim::AttackMove) AttackMovePress(Mouse, bAdd, bCtrl);
		else if (Aim != ETCAim::None)
		{
			// client/main.js mousedown while targeting: the aim takes the left click, never a selection
			FVector2D G;
			const bool bG = GroundAt(Mouse, G);
			AimClick(bG, G, bAdd);
		}
		else { bDragging = true; bDragMoved = false; DragFrom = DragTo = Mouse; PressTime = FPlatformTime::Seconds(); }
	}
	if (bDragging)
	{
		// The release can come outside the window: the box then reaches to where the cursor is, as the browser's
		// mouseup does, instead of the drag waiting for a release it was already given.
		FVector2D Now;
		if (CursorPixels(Now, true)) DragTo = Now;
		if (FVector2D::Distance(DragFrom, DragTo) > DragThreshold)
		{
			if (!bDragMoved) CancelFollow(); // client/main.js mousemove: a box drag ends a follow
			bDragMoved = true;
		}
		if (bWasFocused && !bFocused) CancelDrag(); // the window lost the focus mid-drag: the browser's blur drops the drag
		else if (WasInputKeyJustReleased(EKeys::LeftMouseButton))
		{
			const bool bBox = bDragMoved;
			CancelDrag();
			if (bBox) { BoxSelect(DragFrom, DragTo, bAdd); DoubleClick.Reset(); }
			else
			{
				ClickSelect(DragTo, bAdd);
				// client/main.js dblclick (after both clicks ran): the clicked type, on the screen or with Ctrl the map
				const bool bMapWide = bCtrl || IsInputKeyDown(EKeys::LeftCommand) || IsInputKeyDown(EKeys::RightCommand); // ctrlKey || metaKey
				if (DoubleClick.Click(PressTime, DragFrom)) DoubleClickSelect(DragTo, bMapWide);
			}
		}
		else if (!IsInputKeyDown(EKeys::LeftMouseButton)) CancelDrag(); // the release never reached us (keys flushed)
	}
	// client/main.js: a right click with an aim armed only disarms it; otherwise a move starts the facing gesture and
	// any other order goes at once
	if (!bTakenRight && bRightPress) RightPressWorld(Mouse, bAdd, bCtrl);
	if (Formation.IsGesturing())
	{
		// client/main.js mousemove and mouseup for the gesture's button; blur ends it (cancelInput)
		const ETCGestureButton B = Formation.CurrentGesture().Button;
		const FKey& Key = B == ETCGestureButton::Right ? EKeys::RightMouseButton : EKeys::LeftMouseButton;
		FVector2D Now;
		if (!CursorPixels(Now, true)) Now = GestureLast;
		if (bGesturePinned) GestureMove(GesturePinnedPixel, !GesturePinnedPixel.Equals(GestureLast, 0.01), bCtrl, true); // verification: PinGesturePointer
		else if (bWasFocused && !bFocused) Formation.CancelGesture();
		else if (WasInputKeyJustReleased(Key)) GestureRelease(B, Now, bAdd, bCtrl);
		else GestureMove(Now, !Now.Equals(GestureLast, 0.01), bCtrl, IsInputKeyDown(Key));
	}
}

bool ATCPlayerController::OtherRightClickKind(const FTCCursor& Cursor) const
{
	// client/orders.js kind: 'assist' (and repair), 'entrench' (joining planned works) and 'house' come before 'move';
	// 'board' and 'attack' are FTCFormationControls::WouldMove's own, and an all-truck selection never reaches 'house'
	const ATCGameMode* M = Mode();
	if (!M) return false;
	if (Cursor.bHouse && !Cursor.Enemy) return true;
	if (M->Support.PlanRightClick(M->State, M->UnitTable, Selected, Cursor, false).Kind != ETCSupportOrder::None) return true;
	const FTCFieldClick Join = M->Field.PlanJoin(M->State, Selected, Cursor.bHasGround, Cursor.Ground, false);
	return Join.Message.IsValid() || Join.bThrows;
}

bool ATCPlayerController::RightPressWorld(FVector2D Pixel, bool bShift, bool bCtrl)
{
	SkipIntro();
	ATCGameMode* M = Mode();
	if (!M || !M->State.bHasMatch) return false;
	if (Aim != ETCAim::None) { CancelAim(TEXT("Aim cancelled")); return false; }
	// if (e.button !== 2 || !selected.size || !lastSnap) return
	if (!Selected.Num() || M->State.Snapshots <= 0) return false;
	const FTCCursor C = CursorAt(Pixel);
	if (FTCFormationControls::WouldMove(M->State, M->UnitTable, Selected, C, OtherRightClickKind(C))
		&& Formation.BeginGesture(ETCGestureButton::Right, Pixel, C, false, bShift, bCtrl))
	{
		GestureLast = Pixel;
		UE_LOG(LogTC, Log, TEXT("facing gesture: right press at pixel (%.0f, %.0f), ground (%.1f, %.1f)%s%s"), Pixel.X, Pixel.Y, C.Ground.X, C.Ground.Y, bShift ? TEXT(", Shift") : TEXT(""), bCtrl ? TEXT(", Ctrl") : TEXT(""));
		return true;
	}
	IssueOrder(C, bCtrl, bShift);
	return false;
}

bool ATCPlayerController::AttackMovePress(FVector2D Pixel, bool bShift, bool bCtrl)
{
	SkipIntro();
	FTCCursor C;
	if (!GroundAt(Pixel, C.Ground)) { NoteOrder(false, TCOrders::DenySentence(TEXT("blocked"), FString())); return false; }
	C.bHasGround = true;
	if (!Formation.BeginGesture(ETCGestureButton::Left, Pixel, C, true, bShift, bCtrl)) return false;
	GestureLast = Pixel;
	UE_LOG(LogTC, Log, TEXT("facing gesture: attack-move press at pixel (%.0f, %.0f), ground (%.1f, %.1f)"), Pixel.X, Pixel.Y, C.Ground.X, C.Ground.Y);
	return true;
}

void ATCPlayerController::GestureMove(FVector2D Pixel, bool bMoved, bool bCtrl, bool bHeld)
{
	if (!Formation.IsGesturing()) return;
	FVector2D G;
	const bool bG = GroundAt(Pixel, G);
	Formation.UpdateGesture(Pixel, bG, G, bMoved, bCtrl, bHeld);
	GestureLast = Pixel;
}

int32 ATCPlayerController::GestureRelease(ETCGestureButton Button, FVector2D Pixel, bool bShift, bool bCtrl)
{
	ATCGameMode* M = Mode();
	if (!M || !Formation.IsGesturing()) return 0;
	FVector2D G;
	const bool bG = GroundAt(Pixel, G);
	FTCFacingRelease Out;
	if (!Formation.ReleaseGesture(Button, Pixel, bG, G, bShift, bCtrl, M->State, M->UnitTable, Selected, Out)) return 0;
	UE_LOG(LogTC, Log, TEXT("facing gesture: release at pixel (%.0f, %.0f): %s%s%s"), Pixel.X, Pixel.Y,
		Out.Facing.bSet ? *FString::Printf(TEXT("facing %.4f rad, width %.1f m"), Out.Facing.Face, Out.Facing.Reach) : TEXT("no facing (a click)"),
		Out.bAttack ? TEXT(", attack-move") : TEXT(""), Out.bQueue ? TEXT(", queued") : TEXT(""));
	// the attack-move aim's gesture disarms the aim (its cursor is the ground alone: no unit or works is under it)
	if (Out.bCancelAim) CancelAim();
	TGuardValue<bool> GroundOnly(bCursorGroundOnly, Out.bCancelAim);
	return IssueOrder(Out.Cursor, Out.bAttack, Out.bQueue, Out.Facing);
}

void ATCPlayerController::GatherCover(FTCCoverKnown& Out) const
{
	const ATCGameMode* M = Mode();
	if (!M) return;
	FTCCoverSources Src;
	Src.PositionOf = PositionOf();
	// the per-cell damage states and wrecks this side holds (client/cover-preview.js reads terrain.state and hulks)
	M->WorldObjects.FillCoverSources(Src);
	FTCCoverPreview::Gather(M->State, Selected, Src, Out);
}

void ATCPlayerController::FlashCover(FVector2D At)
{
	ATCGameMode* M = Mode();
	if (!M || At.X < 0) return;
	FTCCoverKnown Known;
	GatherCover(Known);
	M->Cover.Flash(At.X, At.Y, Known);
}

void ATCPlayerController::UpdateCommandKeys()
{
	// client/keys.js: the keys the table binds (and F1, F2), each matched with exactly the modifiers held now (Ctrl or
	// Command); the digits are UpdateGroupKeys'. A key that ended the intro this frame is not acted on.
	const bool bCtrl = IsInputKeyDown(EKeys::LeftControl) || IsInputKeyDown(EKeys::RightControl) || IsInputKeyDown(EKeys::LeftCommand) || IsInputKeyDown(EKeys::RightCommand);
	const bool bShift = IsInputKeyDown(EKeys::LeftShift) || IsInputKeyDown(EKeys::RightShift);
	const bool bAlt = IsInputKeyDown(EKeys::LeftAlt) || IsInputKeyDown(EKeys::RightAlt);
	// client/main.js keydown: Escape ends a facing gesture whatever else it does
	if (WasInputKeyJustPressed(EKeys::Escape)) Formation.CancelGesture();
	for (const FKey& K : TCKeys::PolledKeys())
	{
		if (FTCControlGroups::DigitOf(K) || IntroKeys.Contains(K) || !WasInputKeyJustPressed(K)) continue;
		if (KeyChord(K, bShift, bCtrl, bAlt)) ChordKeys.Add(K);
	}
}

TArray<FTCUiCommand> ATCPlayerController::LetteredCards() const
{
	TArray<FTCUiCommand> Out;
	const ATCGameMode* M = Mode();
	if (!M || !M->State.bHasMatch || !TCArmyCommands::ClassicLobby(M->State)) return Out;
	// client/hud.js lettered: ctx.classic() and the card has [data-train] buttons, in the order they are drawn
	const FTCCommandCard Card = M->Recruit.Card(M->State, Selected);
	if (Card.Kind != ETCCardKind::Building) return Out;
	for (const FTCRecruitGroup& G : Card.Groups)
		for (const FTCRecruitCard& C : G.Cards)
		{
			FTCUiCommand Cmd(ETCUiAction::Recruit);
			Cmd.Unit = C.Type;
			Cmd.Building = C.Action.bHasFrom ? C.Action.From : Card.Building;
			Out.Add(Cmd);
		}
	return Out;
}

FTCPositionOf ATCPlayerController::PositionOf() const
{
	const ATCGameMode* M = Mode();
	return [M](int32 Id, FVector2D& Out)
	{
		const FTCUnit* U = M ? M->State.Units.Find(Id) : nullptr;
		if (!U || !ATCUnitRenderer::IsDrawn(*U)) return false;
		if (!M->Units || !M->Units->DrawnAt(Id, Out)) Out = FVector2D(U->X, U->Z);
		return true;
	};
}

bool ATCPlayerController::RunAbilityPlan(const FTCAbilityPlan& Plan)
{
	const FTCAbilityDef* D = Plan.Type.IsEmpty() ? nullptr : FTCAbilities::Find(Plan.Type);
	const FString Name = D ? FString::Printf(TEXT("%s (%s)"), *D->Name, *D->UnitName) : FString(TEXT("Ability"));
	if (Plan.bArmAim)
	{
		if (!SetAim(ETCAim::Ability, Plan.AimHint)) return false;
		AimAbilityType = Plan.Type;
		AimAbilityId = Plan.AbilityId;
		AimAbilityRadius = Plan.AimRadius;
		UE_LOG(LogTC, Log, TEXT("%s aim armed (radius %.1f m)"), *Name, Plan.AimRadius);
		return true;
	}
	if (Plan.bCancelAim && Aim == ETCAim::Ability) CancelAim();
	if (!Plan.Message.IsValid())
	{
		if (!Plan.Note.IsEmpty()) NoteOrder(false, FString::Printf(TEXT("%s: %s%s"), *Name, *Plan.Note, Plan.bNative ? TEXT(" (this client's own check)") : TEXT("")));
		return false;
	}
	if (!Plan.PredictedDeny.IsEmpty()) UE_LOG(LogTC, Log, TEXT("%s: the server is expected to answer '%s' (client prediction only; sent as the browser sends it)"), *Name, *Plan.PredictedDeny);
	const FString What = Plan.Kind == ETCAbilityCommand::Autocast ? FString::Printf(TEXT("Autocast %s %s"), *Name, Plan.bAutocastOn ? TEXT("on") : TEXT("off"))
		: Plan.Kind == ETCAbilityCommand::Throw ? FString::Printf(TEXT("%s thrown"), *Name) : Name;
	bool bSent = false;
	SendPlan(Plan.ToOrderPlan(), *What, &bSent);
	if (bSent) Feedback(TEXT("click")); // client/main.js useAbility blip(880), the aimed throw blip(760), autocast blip()
	return bSent;
}

bool ATCPlayerController::RunSupportPlan(const FTCSupportPlan& Plan, const TCHAR* What)
{
	if (!Plan.Message.IsValid())
	{
		// client/main.js unload and mountUp send nothing, silently, when nothing qualifies; a reason is shown only when
		// the plan has one (a site card's refusal, or this client's own check)
		if (Plan.Note.IsEmpty()) { UE_LOG(LogTC, Log, TEXT("%s: nothing to send (the browser is silent here)"), What); }
		else NoteOrder(false, FString::Printf(TEXT("%s: %s%s"), What, *Plan.Note, Plan.bNative ? TEXT(" (this client's own check)") : TEXT("")));
		return false;
	}
	if (!Plan.PredictedDeny.IsEmpty()) UE_LOG(LogTC, Log, TEXT("%s: the server is expected to answer '%s' (client prediction only; sent as the browser sends it)"), What, *Plan.PredictedDeny);
	bool bSent = false;
	SendPlan(Plan.ToOrderPlan(), What, &bSent);
	if (bSent) Feedback(TEXT("click")); // client/main.js mountUp blip(600), unload blip(520); hud.js site cancel blip(300)
	if (bSent && Plan.bClearSelection) Selected.Reset(); // client/hud.js drawClassicCard: ctx.selected.clear()
	return bSent;
}

TArray<ETCKeyContext> ATCPlayerController::KeyContexts(bool bLettered) const
{
	// client/main.js keyContexts: [classic ? 'classic' : 'army', recruit or building, targeting]. Recruit mode (Tab
	// outside Classic) is not in this client, so its context never shows.
	TArray<ETCKeyContext> Out;
	const ATCGameMode* M = Mode();
	Out.Add(M && TCArmyCommands::ClassicLobby(M->State) ? ETCKeyContext::Classic : ETCKeyContext::Army);
	if (bLettered) Out.Add(ETCKeyContext::Building);
	if (Aim != ETCAim::None) Out.Add(ETCKeyContext::Targeting);
	return Out;
}

bool ATCPlayerController::KeyChord(const FKey& Key, bool bShift, bool bCtrl, bool bAlt)
{
	ATCGameMode* M = Mode();
	if (!M) return false;
	const FString Code = TCKeys::CodeOf(Key);
	if (Code.IsEmpty()) return false;
	// client/main.js keydown (round 15, TCKeys::Route): F1 and F2 come before everything while the match HUD shows (a
	// press here is never a repeat); then an open Controls sheet takes every key; then the table
	const ETCKeyRoute Route = TCKeys::Route(Code, M->State.bHasMatch, false, bControlsSheetOpen, M->StatsSheet.IsOpen());
	// round 16: Esc closes the stats sheet (the dialog's cancel), in a match or not
	if (Route == ETCKeyRoute::Swallowed && Code == TEXT("Escape") && M->StatsSheet.IsOpen() && !bControlsSheetOpen) { M->StatsSheetTag(TEXT("stats:close"), false); return true; }
	if (!M->State.bHasMatch)
	{
		// the lobby: the table too, where only M ('mute') does anything in this client
		const FTCKeyBinding* B = Route == ETCKeyRoute::Table ? TCKeys::Match(Code, bShift, bCtrl, bAlt, KeyContexts(false)) : nullptr;
		return B && TCKeys::RunsInLobby(B->Id) && RunBinding(*B, bShift);
	}
	if (Route == ETCKeyRoute::ControlsSheet) { RunUi(ETCUiAction::ControlsSheet); return true; }
	if (Route == ETCKeyRoute::StatsToggle)
	{
		M->ToggleStats();
		UE_LOG(LogTC, Log, TEXT("stats overlay (F2): %s"), *M->StatsOverlay().MenuLabel());
		return true;
	}
	// the Controls sheet is modal: Esc closes it (the dialog's cancel) and every other key stops there
	if (Route == ETCKeyRoute::Swallowed)
	{
		if (Code == TEXT("Escape")) RunUi(ETCUiAction::ControlsSheet);
		return true;
	}
	// client/keys.js match over keyContexts: targeting outranks a building's card letters, which outrank the base
	// table; a letter with no card under it keeps its usual meaning, camera keys included (hud.hasCard)
	if (ATCHUD* H=Cast<ATCHUD>(GetHUD()); H && H->RecruitKey(Code,bShift,bCtrl,bAlt)) return true;
	const TArray<FTCUiCommand> Cards = LetteredCards();
	TArray<ETCKeyContext> Contexts = KeyContexts(Cards.Num() > 0);
	const FTCKeyBinding* B = TCKeys::Match(Code, bShift, bCtrl, bAlt, Contexts);
	if (B && (B->Id.StartsWith(TEXT("card:")) || B->Id.StartsWith(TEXT("cardMany:"))))
	{
		int32 Colon = INDEX_NONE;
		B->Id.FindChar(TEXT(':'), Colon);
		const int32 Index = FCString::Atoi(*B->Id.Mid(Colon + 1)) - 1;
		if (Cards.IsValidIndex(Index))
		{
			UE_LOG(LogTC, Log, TEXT("card letter %s: card %d, %s"), *B->Label, Index + 1, *Cards[Index].Describe());
			RunCommand(Cards[Index], B->bShift); // Shift: cardMany, buy five (as many as the limits allow)
			return true;
		}
		Contexts.Remove(ETCKeyContext::Building);
		Contexts.Remove(ETCKeyContext::Recruit);
		B = TCKeys::Match(Code, bShift, bCtrl, bAlt, Contexts);
	}
	return B && RunBinding(*B, bShift);
}

bool ATCPlayerController::RunBinding(const FTCKeyBinding& B, bool bShift)
{
	ATCGameMode* M = Mode();
	if (!M) return false;
	const FString& Id = B.Id;
	// the camera's own keys stay held for UpdateCamera (client/main.js keeps them in its keys set)
	if (Id.StartsWith(TEXT("pan")) || Id.StartsWith(TEXT("rotate"))) return false;
	FString Kind, Value;
	Id.Split(TEXT(":"), &Kind, &Value);
	if (Kind.IsEmpty()) Kind = Id;
	if (Id == TEXT("stop")) { RunUi(ETCUiAction::Stop); return true; }
	if (Id == TEXT("retreat")) { RunUi(ETCUiAction::Retreat); return true; }
	if (Id == TEXT("destroy")) { RunUi(ETCUiAction::Destroy); return true; }
	if (Id == TEXT("ability")) { RunAbilityPlan(M->Abilities.PlanKey(M->State, M->UnitTable, Selected)); return true; } // useAbility(fKeyType())
	if (Id == TEXT("amove")) { RunUi(ETCUiAction::AttackMove); return true; }
	if (Id == TEXT("area")) { RunUi(ETCUiAction::Area); return true; }
	if (Id == TEXT("cover")) { RunCommand(FTCUiCommand(ETCUiAction::Cover), false); return true; } // takeCover(): never queued from the key
	if (Kind == TEXT("stance")) { FTCUiCommand C(ETCUiAction::Stance); C.Unit = Value; RunCommand(C); return true; }
	if (Id == TEXT("formation")) { RunFormation(ETCFormationControl::Cycle); return true; }
	if (Id == TEXT("tighten")) { RunFormation(ETCFormationControl::Tighten); return true; }
	if (Id == TEXT("spread")) { RunFormation(ETCFormationControl::Spread); return true; }
	if (Id == TEXT("mute"))
	{
		// client/main.js toggleMute: audio.toggleMute() (mute, or back to the volume before muting; saved)
		UTCAudio* A = GetWorld() ? GetWorld()->GetSubsystem<UTCAudio>() : nullptr;
		if (!A) { UE_LOG(LogTC, Log, TEXT("audio (M): no sound subsystem in this run")); return true; }
		A->ToggleMute();
		// round 15: the browser shows nothing for M but the menu's slider following; this client's Volume button follows
		UE_LOG(LogTC, Log, TEXT("audio (M): %s"), *VolumeLabel());
		return true;
	}
	if (Id == TEXT("unload")) { RunUi(ETCUiAction::Unload); return true; }
	if (Id == TEXT("rail")) { RunUi(ETCUiAction::Rail); return true; }
	if (Id == TEXT("mountUp")) { RunUi(ETCUiAction::MountUp); return true; }
	if (Id == TEXT("alert")) { RunUi(ETCUiAction::Alert); return true; }
	if (Id == TEXT("alertHistory"))
	{
		if (ToggleAlertHistory) ToggleAlertHistory(); else NoteOrder(false, TEXT("Alert history (Alt+H): not in this client yet"));
		return true;
	}
	if (Id == TEXT("alertPrevious") || Id == TEXT("alertNext"))
	{
		// client/main.js: navigate(1) visits the previous (older) alert, navigate(-1) the next; the camera jumps there
		FVector2D At;
		if (!VisitAlert) NoteOrder(false, TEXT("Alert history (Alt+Left, Alt+Right): not in this client yet"));
		else if (VisitAlert(Id == TEXT("alertPrevious") ? 1 : -1, At)) FocusOn(At);
		return true;
	}
	if (Id == TEXT("follow")) { RunUi(ETCUiAction::Follow); return true; }
	if (Id == TEXT("rally")) { RunUi(ETCUiAction::Rally); return true; }
	if (Id == TEXT("home")) { RunUi(ETCUiAction::Home); return true; }
	if (Id == TEXT("clear")) { Selected.Reset(); return true; }
	if (Id == TEXT("cancelAim")) { CancelAim(TEXT("Aim cancelled")); return true; }
	if (Id == TEXT("army")) { RunUi(ETCUiAction::Army); return true; }
	if (Id == TEXT("idle")) { RunUi(ETCUiAction::Idle); return true; }
	if (Id == TEXT("idleAll")) { RunUi(ETCUiAction::IdleAll); return true; }
	if (Id == TEXT("idleEngineer")) { RunUi(ETCUiAction::IdleEngineer); return true; }
	if (Kind == TEXT("support")) { FTCUiCommand C(ETCUiAction::Support); C.Unit = Value; RunCommand(C); return true; }
	if (Kind == TEXT("fort")) { FTCUiCommand C(ETCUiAction::Fort); C.Unit = Value; RunCommand(C); return true; }
	if (Kind == TEXT("entrench")) { FTCUiCommand C(ETCUiAction::Entrench); C.Unit = Value; RunCommand(C); return true; }
	if (Kind == TEXT("build")) { FTCUiCommand C(ETCUiAction::Build); C.Unit = Value; RunCommand(C); return true; }
	if (Id == TEXT("recruitMode"))
	{
		// client/main.js recruitMode: Classic explains its letters; recruit mode itself is not in this client
		if (TCArmyCommands::ClassicLobby(M->State)) ShowFeedback(TEXT("Select a Production Building and press the letters on its cards"), 4.0);
		else NoteOrder(false, TEXT("Recruit mode (Tab): not in this client yet"));
		return true;
	}
	// group chords are UpdateGroupKeys'; 'recruitOff' needs recruit mode
	return false;
}

bool ATCPlayerController::SetAim(ETCAim Kind, const FString& Hint, const FString& BuildKind)
{
	ATCGameMode* M = Mode();
	// client/main.js setAim: a spectator, an operative or a player who is out arms nothing
	if (!M || M->State.bSpectator || !M->State.SeatConfirmed()) return false;
	Formation.CancelGesture(); // setAim: clearFacing()
	if (Kind != ETCAim::Support) PowerAim.Cancel(); // a support or area aim is armed by FTCSupportPowers first
	Aim = Kind;
	AimBuild = Kind == ETCAim::Build ? BuildKind : FString();
	AimAbilityType.Reset(); AimAbilityId.Reset(); AimAbilityRadius = 0;
	if (Kind != ETCAim::Field) FieldAim.Cancel(); // a field aim is armed by FTCFieldworks::StartDig or StartEntrench first
	AimHint = Hint;
	NoteOrder(true, Hint);
	return true;
}

void ATCPlayerController::CancelAim(const TCHAR* Note)
{
	const bool bWas = Aim != ETCAim::None;
	Aim = ETCAim::None;
	AimBuild.Reset();
	AimHint.Reset();
	AimAbilityType.Reset(); AimAbilityId.Reset(); AimAbilityRadius = 0;
	FieldAim.Cancel();
	PowerAim.Cancel();
	Formation.CancelGesture(); // cancelAim: clearFacing()
	if (ATCGameMode* M = Mode()) M->Controls.DisarmSpawn(); // round 15: a right click stops the sandbox spawn aim
	if (bWas && Note) NoteOrder(false, Note);
}

bool ATCPlayerController::AimClickAt(FVector2D Screen, bool bShift, FVector2D* OutGround)
{
	SkipIntro();
	FVector2D G;
	const bool bG = GroundAt(Screen, G);
	if (OutGround) *OutGround = bG ? G : FVector2D(-1, -1);
	return AimClick(bG, G, bShift);
}

bool ATCPlayerController::AimClick(bool bHasGround, FVector2D Ground, bool bShift)
{
	ATCGameMode* M = Mode();
	if (!M || !M->State.bHasMatch || Aim == ETCAim::None) return false;
	const FTCMatchState& S = M->State;
	// client/main.js mousedown: with no ground under the click every aim says 'blocked' and stays armed
	if (!bHasGround && Aim != ETCAim::Rally && Aim != ETCAim::Build && Aim != ETCAim::Ability && Aim != ETCAim::Field && Aim != ETCAim::Support)
	{
		NoteOrder(false, TCOrders::DenySentence(TEXT("blocked"), FString()));
		return false;
	}
	switch (Aim)
	{
	case ETCAim::AttackMove:
	{
		// a click with no drag (the mouse path starts the facing gesture instead, AttackMovePress): the plain click's
		// formation as the gesture's release sends it, then the aim is spent
		CancelAim();
		FTCCursor C;
		C.bHasGround = true; C.Ground = Ground;
		TGuardValue<bool> GroundOnly(bCursorGroundOnly, true);
		return IssueOrder(C, true, bShift) > 0;
	}
	case ETCAim::Rally:
	{
		const FTCArmyPlan Plan = TCArmyCommands::PlanRallyAt(S, M->UnitTable, Selected, bHasGround, Ground);
		if (Plan.bKeepAim || !Plan.Message.IsValid())
		{
			if (Plan.bCancelAim) CancelAim();
			NoteOrder(false, Plan.Note.IsEmpty() ? FString(TEXT("Rally: nothing to send")) : Plan.Note);
			return false;
		}
		CancelAim();
		const FString What = Plan.RallyBuildings ? FString::Printf(TEXT("Rally for %d building%s"), Plan.RallyBuildings, Plan.RallyBuildings == 1 ? TEXT("") : TEXT("s")) : FString(TEXT("Army rally point"));
		if (!Plan.PredictedDeny.IsEmpty()) UE_LOG(LogTC, Log, TEXT("rally: the server is expected to answer '%s' (client prediction only; sent as the browser sends it)"), *Plan.PredictedDeny);
		bool bSent = false;
		SendPlan(Plan.ToOrderPlan(), *What, &bSent);
		if (bSent) Feedback(TEXT("click")); // client/main.js rallyAt: blip(620)
		return bSent;
	}
	case ETCAim::Build:
	{
		const FTCBuildClick Click = M->Build.Click(S, AimBuild, bHasGround, Ground, bShift, Selected);
		if (Click.bThrows)
		{
			// the browser's placement check throws here (FTCConstruction's file comment): nothing happens at all
			UE_LOG(LogTC, Warning, TEXT("build %s at (%.1f, %.1f): the browser's placement check would throw here; nothing done, the aim stays"), *AimBuild, Ground.X, Ground.Y);
			return false;
		}
		if (!Click.Message.IsValid())
		{
			NoteOrder(false, FString::Printf(TEXT("Build %s: %s%s"), *AimBuild, *Click.Feedback, Click.Placement.bNative ? TEXT(" (this client's own check)") : TEXT("")));
			return false;
		}
		const FTCBuildDef* D = FTCConstruction::Find(AimBuild);
		const FString What = FString::Printf(TEXT("Build %s%s"), D ? *D->Name : *AimBuild, bShift ? TEXT(" (queued)") : TEXT(""));
		if (!Click.bKeepAim) CancelAim();
		FTCOrderPlan Plan;
		Plan.Message = Click.Message;
		const TArray<TSharedPtr<FJsonValue>>* Ids = nullptr;
		Plan.Units = Click.Message->TryGetArrayField(TEXT("ids"), Ids) && Ids ? Ids->Num() : 0;
		Plan.At = Click.Marker;
		Plan.Color = FColor(0xe8, 0xc8, 0x60); // client/main.js marker for a building
		bool bSent = false;
		SendPlan(Plan, *What, &bSent);
		return bSent;
	}
	case ETCAim::Ability:
		// client/main.js mousedown with an ability aim: no ground or a refusal keeps the aim; otherwise the nearest able
		// selected unit throws and the aim ends
		return RunAbilityPlan(M->Abilities.PlanAimClick(S, M->UnitTable, Selected, AimAbilityType, AimAbilityId, bHasGround, Ground, bShift));
	case ETCAim::Support:
		// client/main.js mousedown with a support kind or 'area' armed (FTCSupportPowers::AimClick keeps or ends the aim)
		return RunPowerPlan(M->Powers.AimClick(S, M->UnitTable, M->Recruit, Selected, PowerAim, bHasGround, Ground, bShift));
	case ETCAim::Spawn:
		// round 15: client/main.js mousedown, kind 'spawn': { t: 'spawn', unit, enemy, x, z }; the aim stays armed
		if (M->ControlsSpawnAt(Ground)) { LastOrderAt = Ground; LastOrderTime = GetWorld()->GetTimeSeconds(); LastOrderColor = FColor(0xe8, 0xc8, 0x60); return true; }
		return false;
	case ETCAim::Rail:
	{
		// client/main.js railTo: the ground units selected now, the aim ends, then { t: 'rail', ids, x, z } (no queue)
		const TArray<int32> Ids = Railers();
		CancelAim();
		if (!Ids.Num()) return false;
		TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
		O->SetStringField(TEXT("t"), TEXT("rail"));
		O->SetArrayField(TEXT("ids"), IdValues(Ids));
		O->SetNumberField(TEXT("x"), Ground.X);
		O->SetNumberField(TEXT("z"), Ground.Y);
		FTCOrderPlan Plan;
		Plan.Message = O;
		Plan.Units = Ids.Num();
		Plan.At = Ground;
		Plan.Color = FColor(0x9d, 0xd0, 0xff); // client/main.js marker(g.x, g.z, 0x9dd0ff)
		bool bSent = false;
		SendPlan(Plan, TEXT("Send by rail"), &bSent);
		if (bSent) Feedback(TEXT("click"), TEXT("move")); // client/main.js railTo: blip(600); bark('move')
		return bSent;
	}
	case ETCAim::Field:
	{
		const FTCFieldClick Click = M->Field.Click(S, FieldAim, bHasGround, Ground, bShift, Selected, PositionOf());
		if (Click.bThrows)
		{
			UE_LOG(LogTC, Warning, TEXT("field works click at (%.1f, %.1f): the browser would throw here; nothing done"), Ground.X, Ground.Y);
			return false;
		}
		// FTCFieldworks::Click pinned the centre, re-armed (Shift digs) or spent the aim
		if (FieldAim.IsArmed()) AimHint = FieldAim.Hint;
		else { Aim = ETCAim::None; AimHint.Reset(); }
		if (!Click.Message.IsValid())
		{
			if (Click.bPinnedCenter) NoteOrder(true, FString::Printf(TEXT("Centre pinned at (%.1f, %.1f): %s"), Ground.X, Ground.Y, *FieldAim.Hint));
			else NoteOrder(false, Click.Feedback.IsEmpty() ? FString(TEXT("Field works: nothing to send")) : Click.Feedback);
			return false;
		}
		FTCOrderPlan Plan;
		Plan.Message = Click.Message;
		Plan.Units = Click.Units;
		Plan.At = Click.Marker;
		Plan.Color = Click.MarkerColor;
		bool bSent = false;
		SendPlan(Plan, *Click.What, &bSent);
		return bSent;
	}
	default:
		return false;
	}
}

bool ATCPlayerController::ScreenPoint(const ATCGameMode& M, const FTCUnit& U, const FTCUnitDef* Def, FVector2D& Out) const
{
	// client/main.js screenOf: the drawn spot 1 m over the ground, aircraft AIR_ALT higher; only points in front count
	FVector2D At;
	if (!M.Units->DrawnAt(U.Id, At)) At = FVector2D(U.X, U.Z);
	const float Up = M.Terrain->HeightAt(At.X, At.Y) + 1.f + ATCUnitRenderer::LiftMetres(Def);
	return ProjectWorldLocationToScreen(ATCTerrain::ToWorld(At.X, At.Y, Up), Out);
}

bool ATCPlayerController::ScreenOf(int32 Id, FVector2D& Out) const
{
	ATCGameMode* M = Mode();
	const FTCUnit* U = M && M->Units && M->Terrain ? M->State.Units.Find(Id) : nullptr;
	return U && ATCUnitRenderer::IsDrawn(*U) && ScreenPoint(*M, *U, M->UnitTable.Find(U->Type), Out);
}

int32 ATCPlayerController::PickAt(FVector2D Screen, TFunctionRef<bool(const FTCUnit&, const FTCUnitDef&)> Test, float Radius) const
{
	ATCGameMode* M = Mode();
	if (!M || !M->Units || !M->Terrain) return 0;
	int32 Best = 0;
	float BestD = TNumericLimits<float>::Max();
	for (const auto& Pair : M->State.Units)
	{
		const FTCUnit& U = Pair.Value;
		const FTCUnitDef* Def = M->UnitTable.Find(U.Type);
		// riders are never picked (client/main.js pick); parked planes are not drawn here, so not either; a garrisoned
		// squad is picked at its position in the house, as the browser's screenOf does (round 16)
		if (!Def || (!ATCUnitRenderer::IsDrawn(U) && !(U.Flags & TCFlags::Garrisoned)) || !Test(U, *Def)) continue;
		FVector2D P;
		if (!ScreenPoint(*M, U, Def, P)) continue;
		const float D = FVector2D::Distance(P, Screen);
		if (D < (Radius > 0 ? Radius : Def->bInfantry ? PickInfantry : PickVehicle) && D < BestD) { BestD = D; Best = U.Id; }
	}
	return Best;
}

FTCCursor ATCPlayerController::CursorAt(FVector2D Screen) const
{
	FTCCursor C;
	ATCGameMode* M = Mode();
	if (!M) return C;
	const FTCMatchState& S = M->State;
	C.bHasGround = GroundAt(Screen, C.Ground);
	C.Enemy = PickAt(Screen, [&S](const FTCUnit& U, const FTCUnitDef&) { return S.IsFoe(U.Owner); });
	C.Friend = PickAt(Screen, [&S](const FTCUnit& U, const FTCUnitDef& D) { return !S.IsFoe(U.Owner) && !D.bStructure && !D.bAir; });
	C.Building = PickAt(Screen, [&S](const FTCUnit& U, const FTCUnitDef& D) { return !S.IsFoe(U.Owner) && D.bBuilding; }, PickBuilding);
	FTCHousePick House;
	if (HouseAt(Screen, House)) { C.bHouse = true; C.HouseCell = House.Cell; C.House = House.Center; }
	return C;
}

bool ATCPlayerController::HouseAt(FVector2D Screen, FTCHousePick& Out) const
{
	ATCGameMode* M = Mode();
	FVector Origin, Dir;
	if (!M || !M->WorldObjects.HasMap() || !DeprojectScreenPositionToWorld(Screen.X, Screen.Y, Origin, Dir)) return false;
	return M->WorldObjects.PickHouse(Origin, Dir, M->Terrain, Out);
}

bool ATCPlayerController::Pickable(const ATCGameMode& M, const FTCUnit& U, const FTCUnitDef* Def) const
{
	// client/selection.js own and mobileOwn: ours, alive, not a structure, and drawn (ATCUnitRenderer::IsDrawn leaves
	// out planes parked at base and riders) or garrisoned (round 16: its roof bar is drawn and picks it)
	return Def && M.State.IsOwn(U) && U.Hp > 0 && !Def->bStructure && (ATCUnitRenderer::IsDrawn(U) || (U.Flags & TCFlags::Garrisoned));
}

bool ATCPlayerController::RoofBounds(const ATCGameMode& M, const FTCUnit& U, const FTCUnitDef* Def, TArray<FBox2D>& Out) const
{
	Out.Reset();
	if (!(U.Flags & TCFlags::Garrisoned) || !Def || !M.Terrain || !PlayerCameraManager) return false;
	FVector2D At;
	if (!M.Units || !M.Units->DrawnAt(U.Id, At)) At = FVector2D(U.X, U.Z);
	const FRotationMatrix Cam(PlayerCameraManager->GetCameraRotation());
	const FVector BarAt = ATCTerrain::ToWorld(At.X, At.Y, M.Terrain->HeightAt(At.X, At.Y) + (float)TCOrders::RoofBarHeight);
	const double Frac = Def->MaxHp() > 0 ? (double)U.Hp / Def->MaxHp() : 1.0;
	Out = TCOrders::RoofBarScreenBounds(BarAt, Cam.GetUnitAxis(EAxis::Y), Cam.GetUnitAxis(EAxis::Z), TCOrders::RoofBarRects(Frac, U.Supp, U.Stars),
		[this](const FVector& W, FVector2D& Px) { return ProjectWorldLocationToScreen(W, Px); });
	return Out.Num() > 0;
}

bool ATCPlayerController::Boxable(const ATCGameMode& M, const FTCUnit& U, const FTCUnitDef* Def) const
{
	// client/selection.js box: a box takes the army only (trains run themselves, and supply trucks stay on their runs:
	// a move that includes a truck switches it to manual, shared/supply-convoys.js, until a logisticsResume); with the
	// logistics selection switched on it takes only own trucks
	return Pickable(M, U, Def) && FTCLogistics::BoxSelectable(M.State, M.UnitTable, U, M.Logistics.SelectionModeActive());
}

int32 ATCPlayerController::PickSelectable(FVector2D Screen) const
{
	ATCGameMode* M = Mode();
	if (!M || !M->Units || !M->Terrain) return 0;
	// client/selection.js pick: the nearest drawn unit within its type's radius, mobile units first, buildings only
	// when no mobile unit is near. Other structures (a bunker) are never picked.
	auto Nearest = [&](bool bBuildings)
	{
		int32 Best = 0;
		float BestD = TNumericLimits<float>::Max();
		for (const auto& Pair : M->State.Units)
		{
			const FTCUnit& U = Pair.Value;
			const FTCUnitDef* Def = M->UnitTable.Find(U.Type);
			const bool bTest = bBuildings ? Def && Def->bBuilding && M->State.IsOwn(U) && U.Hp > 0 && ATCUnitRenderer::IsDrawn(U) : Pickable(*M, U, Def);
			if (!bTest) continue;
			// round 16: a garrisoned squad has only its roof bar's points (client/selection-view.js, radius 6 px)
			TArray<FBox2D> Roof;
			if (U.Flags & TCFlags::Garrisoned)
			{
				if (!RoofBounds(*M, U, Def, Roof)) continue;
				const float D = (float)TCOrders::RoofPickDistance(Roof, Screen);
				if (D < TCOrders::RoofPickRadius && D < BestD) { BestD = D; Best = U.Id; }
				continue;
			}
			FVector2D P;
			const bool bRoot = ScreenPoint(*M, U, Def, P);
			const float D = bRoot ? FVector2D::Distance(P, Screen) : TNumericLimits<float>::Max();
			const float Radius = Def->bBuilding ? PickBuilding : Def->bInfantry ? PickInfantry : PickVehicle;
			if (D < Radius && D < BestD) { BestD = D; Best = U.Id; }
			TArray<FBox2D> Meshes;
			ModelBounds(U, Meshes);
			for (const FBox2D& Bounds : Meshes)
			{
				const double Edge = TCSelectionBounds::Distance(Bounds, Screen);
				if (Edge < 6 && Edge < BestD) { BestD = Edge; Best = U.Id; }
			}
		}
		return Best;
	};
	const int32 Hit = Nearest(false);
	return Hit ? Hit : Nearest(true);
}

void ATCPlayerController::ClickSelect(FVector2D Screen, bool bAdd)
{
	const int32 Hit = PickSelectable(Screen);
	if (!bAdd) Selected.Reset();
	if (Hit) { if (bAdd && Selected.Contains(Hit)) Selected.Remove(Hit); else Selected.Add(Hit); }
}

void ATCPlayerController::DoubleClickSelect(FVector2D Screen, bool bMapWide)
{
	// client/main.js dblclick: selection.pick, then selection.doubleClick (nothing when no unit is under it). A unit
	// this client does not draw (a garrison) has no point on the screen, so only the map-wide form takes it.
	const int32 Hit = PickSelectable(Screen);
	if (!Hit) return;
	TArray<int32> Ids;
	if (!TCSelectionKeys::DoubleClick(KeyUnits(!bMapWide), Hit, bMapWide, Ids)) return;
	ReplaceSelection(Ids, false);
	const ATCGameMode* M = Mode();
	const FTCUnit* U = M ? M->State.Units.Find(Hit) : nullptr;
	UE_LOG(LogTC, Log, TEXT("double-click on %s %d: %d of that type selected %s"), U ? *U->Type : TEXT("unit"), Hit, Ids.Num(), bMapWide ? TEXT("on the whole map") : TEXT("on the screen"));
}

TArray<FTCKeyUnit> ATCPlayerController::KeyUnits(bool bScreen) const
{
	TArray<FTCKeyUnit> Out;
	const ATCGameMode* M = Mode();
	if (!M) return Out;
	int32 VX = 0, VY = 0;
	GetViewportSize(VX, VY);
	for (const auto& Pair : M->State.Units)
	{
		const FTCUnit& U = Pair.Value;
		// only this seat's own units can be selected (FTCMatchState::IsOwn: a confirmed seat's rows)
		if (!M->State.IsOwn(U)) continue;
		const FTCUnitDef* D = M->UnitTable.Find(U.Type);
		FTCKeyUnit K;
		K.Id = U.Id; K.Type = U.Type; K.bOwn = true; K.Hp = U.Hp; K.Flags = U.Flags;
		K.bDef = D != nullptr;
		if (D) { K.bStructure = D->bStructure; K.bBuilding = D->bBuilding; K.bRail = D->bRail; K.bAir = D->bAir; }
		int32 PlanKind = 0;
		if (PlanKindOf && PlanKindOf(U.Id, PlanKind)) K.PlanKind = PlanKind;
		if (!M->Units || !M->Units->DrawnAt(U.Id, K.At)) K.At = FVector2D(U.X, U.Z);
		if (bScreen && D && M->Units && M->Terrain && ATCUnitRenderer::IsDrawn(U))
		{
			FVector2D P;
			K.bOnScreen = ScreenPoint(*M, U, D, P) && P.X >= 0 && P.Y >= 0 && P.X <= VX && P.Y <= VY;
		}
		// round 16: a garrisoned squad is on the screen when its roof bar is (client/selection.js doubleClick)
		TArray<FBox2D> Roof;
		if (bScreen && (U.Flags & TCFlags::Garrisoned) && RoofBounds(*M, U, D, Roof)) K.bOnScreen = TCOrders::RoofBoxHits(Roof, FBox2D(FVector2D(0, 0), FVector2D(VX, VY)));
		Out.Add(MoveTemp(K));
	}
	return Out;
}

void ATCPlayerController::ReplaceSelection(const TArray<int32>& Ids, bool bCentre)
{
	Selected.Reset();
	for (const int32 Id : Ids) Selected.Add(Id);
	FVector2D C;
	if (bCentre && TCSelectionKeys::Centre(KeyUnits(false), Ids, C)) FocusOn(C); // centerSelection ends a follow
}

TArray<int32> ATCPlayerController::Railers() const
{
	TArray<int32> Out;
	const ATCGameMode* M = Mode();
	if (!M) return Out;
	for (const int32 Id : Selected)
	{
		const FTCUnit* U = M->State.Units.Find(Id);
		const FTCUnitDef* D = U ? M->UnitTable.Find(U->Type) : nullptr;
		if (U && D && M->State.IsOwn(*U) && !D->bStructure && !D->bAir && !D->bRail && !IsNaval(U->Type)) Out.Add(Id);
	}
	return Out;
}

FString ATCPlayerController::FollowChip() const
{
	const ATCGameMode* M = Mode();
	const FTCUnit* U = M && FollowId ? M->State.Units.Find(FollowId) : nullptr;
	if (!U) return FString();
	// client/camera.js showFollow: 'Following ' and the unit's name (the faction's own names are not in this client)
	const FTCUnitDef* D = M->UnitTable.Find(U->Type);
	return FString::Printf(TEXT("Following %s"), D && !D->Name.IsEmpty() ? *D->Name : *U->Type);
}

bool ATCPlayerController::StartFollow()
{
	ATCGameMode* M = Mode();
	if (!M || !M->State.bHasMatch) return false;
	// client/main.js followSelected: a second Shift+Space stops following
	if (FollowId) { CancelFollow(); UE_LOG(LogTC, Log, TEXT("follow off")); return true; }
	int32 Id = 0;
	for (const int32 Sel : Selected) if (M->State.Units.Contains(Sel)) { Id = Sel; break; }
	if (!Id && M->State.bSpectator)
	{
		// watching: the unit under the pointer, else the one nearest the view's centre
		FVector2D Mouse;
		auto Any = [](const FTCUnit&, const FTCUnitDef&) { return true; };
		if (CursorPixels(Mouse, false)) Id = PickAt(Mouse, Any);
		int32 VX = 0, VY = 0;
		GetViewportSize(VX, VY);
		if (!Id && VX > 0 && VY > 0) Id = PickAt(FVector2D(VX / 2.0, VY / 2.0), Any, TNumericLimits<float>::Max());
	}
	if (!Id) return false;
	// client/camera.js follow: the anchor goes, and a cursor already in the edge zone waits until it leaves
	FollowId = Id;
	Anchor.bActive = false;
	EdgeScroll.HoldForFollow();
	UE_LOG(LogTC, Log, TEXT("%s"), *FollowChip());
	return true;
}

void ATCPlayerController::CancelInput()
{
	// client/main.js cancelInput: keys.clear(), drag = null, clearFacing(), rig.stopDrag()
	CancelDrag();
	Formation.CancelGesture();
	bMiddleDrag = false;
	Anchor.bActive = false;
	for (const FKey& K : TCKeys::PolledKeys()) if (IsInputKeyDown(K)) ChordKeys.Add(K);
}

void ATCPlayerController::BoxSelect(FVector2D A, FVector2D B, bool bAdd)
{
	SkipIntro();
	ATCGameMode* M = Mode();
	const FVector2D Lo(FMath::Min(A.X, B.X), FMath::Min(A.Y, B.Y)), Hi(FMath::Max(A.X, B.X), FMath::Max(A.Y, B.Y));
	if (!bAdd) Selected.Reset();
	for (const auto& Pair : M->State.Units)
	{
		const FTCUnit& U = Pair.Value;
		const FTCUnitDef* Def = M->UnitTable.Find(U.Type);
		FVector2D S;
		if (!Boxable(*M, U, Def)) continue;
		// round 16: a garrisoned squad is boxed by its roof bar (client/selection.js inside, bounds against the box)
		TArray<FBox2D> Roof;
		if (U.Flags & TCFlags::Garrisoned) { if (RoofBounds(*M, U, Def, Roof) && TCOrders::RoofBoxHits(Roof, FBox2D(Lo, Hi))) Selected.Add(U.Id); continue; }
		if (ScreenPoint(*M, U, Def, S) && S.X >= Lo.X && S.X <= Hi.X && S.Y >= Lo.Y && S.Y <= Hi.Y) Selected.Add(U.Id);
		TArray<FBox2D> Meshes;
		ModelBounds(U, Meshes);
		for (const FBox2D& Bounds : Meshes) if (TCSelectionBounds::Intersects(Bounds, FBox2D(Lo, Hi))) { Selected.Add(U.Id); break; }
	}
}

void ATCPlayerController::SelectOwn(const TArray<int32>& Ids)
{
	ATCGameMode* M = Mode();
	Selected.Reset();
	if (!M) return;
	for (int32 Id : Ids) if (const FTCUnit* U = M->State.Units.Find(Id)) if (M->State.IsOwn(*U)) Selected.Add(Id);
}

void ATCPlayerController::NoteOrder(bool bSent, const FString& Note)
{
	bLastOrderSent = bSent;
	LastOrderNote = Note;
	LastOrderNoteTime = GetWorld()->GetTimeSeconds();
	if (bSent) { UE_LOG(LogTC, Log, TEXT("%s"), *Note); }
	else { UE_LOG(LogTC, Warning, TEXT("%s"), *Note); }
}

int32 ATCPlayerController::SendPlan(const FTCOrderPlan& Plan, const TCHAR* What, bool* bOutSent)
{
	if (bOutSent) *bOutSent = false;
	ATCGameMode* M = Mode();
	if (!M || !Plan.Message.IsValid()) return 0;
	if (!M->State.CanOrder())
	{
		// The server would drop the order without a word (server.js: only a seat in play, unpaused, before a winner)
		NoteOrder(false, FString::Printf(TEXT("%s not sent: %s"), What, WhyNoOrders(M->State)));
		return 0;
	}
	// Measured here, in UTF-8 bytes as the socket and the server count them (FTCConnection::TrySend writes it the same
	// condensed way), so the note can say how big the order was and suggest a smaller selection.
	FString Text;
	const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Text);
	if (!FJsonSerializer::Serialize(Plan.Message.ToSharedRef(), Writer))
	{
		NoteOrder(false, FString::Printf(TEXT("%s not sent: the order for %d units could not be written"), What, Plan.Units));
		return 0;
	}
	const int32 Bytes = FTCConnection::Utf8Length(Text);
	if (Bytes > ServerMessageLimit)
	{
		NoteOrder(false, FString::Printf(TEXT("%s not sent: %d units make a %d-byte order, over the server's %d-byte message limit; select fewer"), What, Plan.Units, Bytes, ServerMessageLimit));
		return 0;
	}
	if (!M->SendCommand(Plan.Message.ToSharedRef()))
	{
		NoteOrder(false, FString::Printf(TEXT("%s not sent: %s (%s)"), What, *M->LastSendProblem(), *M->ConnectionStatus()));
		return 0;
	}
	// Sent is not accepted: a refusal comes back as a deny, which the HUD shows in the browser's words
	if (bOutSent) *bOutSent = true;
	if (Plan.At.X >= 0)
	{
		LastOrderAt = Plan.At;
		LastOrderColor = Plan.Color;
		LastOrderTime = GetWorld()->GetTimeSeconds();
	}
	FString Where;
	if (Plan.Kind == ETCOrderKind::Attack) Where = FString::Printf(TEXT(" on unit %d at (%.1f, %.1f)"), Plan.TargetId, Plan.At.X, Plan.At.Y);
	else if (Plan.At.X >= 0) Where = FString::Printf(TEXT(" to (%.1f, %.1f)"), Plan.At.X, Plan.At.Y);
	FString Wire;
	Plan.Message->TryGetStringField(TEXT("t"), Wire);
	NoteOrder(true, FString::Printf(TEXT("%s sent: %d units%s, '%s' message, %d bytes"), What, Plan.Units, *Where, *Wire, Bytes));
	// the message itself (unit ids and coordinates only), cut short for a big formation
	UE_LOG(LogTC, Log, TEXT("  wire: %s%s"), *Text.Left(240), Text.Len() > 240 ? TEXT("...") : TEXT(""));
	return Plan.Units;
}

int32 ATCPlayerController::IssueOrder(const FTCCursor& Cursor, bool bAttack, bool bQueue, const FTCMoveFacing& Facing)
{
	ATCGameMode* M = Mode();
	if (!M || !M->State.bHasMatch) return 0;
	const FTCMatchState& S = M->State;
	// client/orders.js dispatch: the troops' order (a supply run when every troop is a truck and a friendly unit or
	// building is under the pointer; otherwise attack or move), then a rally for the selected own Production Buildings
	// to the ground, sent as one command, or as { t: 'orders', queue, commands } when there are several
	TSet<int32> Troops;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = S.Units.Find(Id);
		const FTCUnitDef* D = U ? M->UnitTable.Find(U->Type) : nullptr;
		if (U && D && S.IsOwn(*U) && !D->bStructure) Troops.Add(Id);
	}
	FTCOrderPlan Plan;
	FString What;
	// the order goes to ground with no unit under it (a move, a join, a rally): the cover preview flashes there
	bool bGroundOrder = false;
	// client/orders.js dispatch: ctx.feedback(at, color, tone, voice), main.js blip(tone) (any tone clicks) and
	// bark(voice); tone 660 and the move voice unless the order says otherwise
	const TCHAR* Voice = TEXT("move");
	const FTCLogisticsOrder Supply = FTCLogistics::PlanSupply(S, M->UnitTable, Troops, Cursor, bQueue);
	if (Supply.Kind == ETCLogisticsOrderKind::Supply)
	{
		Plan.Message = Supply.Message;
		Plan.Units = Supply.Trucks.Num();
		Plan.At = Supply.At;
		Plan.Color = Supply.Color;
		const FTCUnit* T = S.Units.Find(Supply.Target);
		Voice = nullptr; // supply: tone 600, no voice
		What = FString::Printf(TEXT("Supply run to %s %d%s"), T ? *T->Type : TEXT("unit"), Supply.Target, Supply.bRelief ? TEXT(" (relief: an allied Supply Cache, held for its owner)") : TEXT(""));
	}
	else if (!Supply.Why.IsEmpty()) { NoteOrder(false, TEXT("Supply not sent: ") + Supply.Why); return 0; }
	else if (Troops.Num())
	{
		// client/orders.js kind, after supply (and escort, which this client does not build): boarding an own carrier,
		// assisting or repairing a friendly building, joining a planned entrenchment, then attack or move
		// (an attack-move gesture's cursor is its ground alone: client/main.js beginFacing({ ground: g }, e, true))
		// client/orders.js kind 'escort', right after supply: every troop an aircraft and a friendly unit under the
		// pointer (FTCCursor::Friend: not a foe, not a structure, not an aircraft)
		int32 Planes = 0;
		for (const int32 Id : Troops)
		{
			const FTCUnit* U = S.Units.Find(Id);
			const FTCUnitDef* D = U ? M->UnitTable.Find(U->Type) : nullptr;
			Planes += D && D->bAir ? 1 : 0;
		}
		const FTCUnit* EscortOf = !bCursorGroundOnly && Cursor.Friend && Planes == Troops.Num() ? S.Units.Find(Cursor.Friend) : nullptr;
		const FTCSupportPlan Support = bCursorGroundOnly || EscortOf ? FTCSupportPlan() : M->Support.PlanRightClick(S, M->UnitTable, Selected, Cursor, bQueue);
		FTCFieldClick Join;
		if (Support.Kind == ETCSupportOrder::None && !bCursorGroundOnly && !EscortOf) Join = M->Field.PlanJoin(S, Selected, Cursor.bHasGround, Cursor.Ground, bQueue);
		if (Support.Kind == ETCSupportOrder::Blocked) { NoteOrder(false, FString::Printf(TEXT("Not sent: %s%s"), *Support.Note, Support.bNative ? TEXT(" (this client's own check)") : TEXT(""))); return 0; }
		if (Join.bThrows) { UE_LOG(LogTC, Warning, TEXT("right click: the browser's entrenchment join would throw here; nothing done")); return 0; }
		if (EscortOf)
		{
			// { t: 'escort', ids: planes, target } with the click's queue flag; the marker on the escorted unit
			TArray<int32> Ids;
			for (const int32 Id : Troops) Ids.Add(Id);
			TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
			O->SetStringField(TEXT("t"), TEXT("escort"));
			O->SetArrayField(TEXT("ids"), IdValues(Ids));
			O->SetNumberField(TEXT("target"), EscortOf->Id);
			O->SetBoolField(TEXT("queue"), bQueue);
			Plan.Message = O;
			Plan.Kind = ETCOrderKind::Move;
			Plan.Units = Ids.Num();
			Plan.TargetId = EscortOf->Id;
			if (!M->Units || !M->Units->DrawnAt(EscortOf->Id, Plan.At)) Plan.At = FVector2D(EscortOf->X, EscortOf->Z);
			Plan.Color = FColor(0x9d, 0xd0, 0xff);
			Voice = nullptr; // escort: tone 600, no voice
			What = FString::Printf(TEXT("Escort %s %d"), *EscortOf->Type, EscortOf->Id);
		}
		else if (Support.Kind != ETCSupportOrder::None)
		{
			Plan = Support.ToOrderPlan();
			// board: tone 600, the move voice stays; assist (and repair, the same order): tone 600, no voice
			if (Support.Kind != ETCSupportOrder::Board) Voice = nullptr;
			const FTCUnit* T = S.Units.Find(Support.TargetId);
			What = FString::Printf(TEXT("%s %s %d"), Support.Kind == ETCSupportOrder::Board ? TEXT("Board") : Support.Kind == ETCSupportOrder::Repair ? TEXT("Repair") : TEXT("Assist"), T ? *T->Type : TEXT("unit"), Support.TargetId);
			if (!Support.PredictedDeny.IsEmpty()) UE_LOG(LogTC, Log, TEXT("%s: the server is expected to answer '%s' (client prediction only; sent as the browser sends it)"), *What, *Support.PredictedDeny);
		}
		else if (Join.Message.IsValid())
		{
			Plan.Message = Join.Message;
			Plan.Units = Join.Units;
			Plan.At = Join.Marker;
			Plan.Color = Join.MarkerColor;
			What = Join.What;
			bGroundOrder = true;
		}
		else if (Cursor.bHouse && !Cursor.Enemy && !bCursorGroundOnly && !Algo::AllOf(Troops, [&S](int32 Id) { const FTCUnit* U = S.Units.Find(Id); return U && U->Type == TEXT("truck"); }))
		{
			// client/orders.js order 'house' (after attack, before move): the garrisoning troops go in, the guns that
			// shell terrain or fire salvos fire at it, the rest move to its cell centre in the Formation menu's
			// formation; one command with its queue flag, or { t: 'orders', queue, commands } in that order
			int32 Inf = 0, Guns = 0;
			const TSharedPtr<FJsonObject> Garrison = FTCWorldObjects::GarrisonCommand(S, M->UnitTable, Selected, Cursor.House, &Inf);
			const TSharedPtr<FJsonObject> Fire = M->Powers.HouseFireCommand(S, M->UnitTable, Selected, Cursor.House, &Guns);
			TSet<int32> Rest = Troops;
			for (const TSharedPtr<FJsonObject>& Part : { Garrison, Fire })
			{
				const TArray<TSharedPtr<FJsonValue>>* Ids = nullptr;
				if (Part.IsValid() && Part->TryGetArrayField(TEXT("ids"), Ids)) for (const TSharedPtr<FJsonValue>& V : *Ids) Rest.Remove((int32)V->AsNumber());
			}
			TArray<TSharedPtr<FJsonValue>> Parts;
			if (Garrison.IsValid()) Parts.Add(MakeShared<FJsonValueObject>(Garrison));
			if (Fire.IsValid()) Parts.Add(MakeShared<FJsonValueObject>(Fire));
			const TArray<FTCFormationTroop> Movers = FTCFormationControls::Troops(S, M->UnitTable, Rest, ETCTroopFilter::Orders);
			if (Rest.Num() && Movers.Num())
			{
				TSharedRef<FJsonObject> Move = MakeShared<FJsonObject>();
				Move->SetStringField(TEXT("t"), TEXT("move"));
				Move->SetArrayField(TEXT("orders"), Formation.Rows(S, Movers, Cursor.House, FTCMoveFacing()));
				Parts.Add(MakeShared<FJsonValueObject>(Move));
			}
			if (!Parts.Num()) return 0;
			if (Parts.Num() == 1) { Plan.Message = Parts[0]->AsObject(); Plan.Message->SetBoolField(TEXT("queue"), bQueue); }
			else
			{
				Plan.Message = MakeShared<FJsonObject>();
				Plan.Message->SetStringField(TEXT("t"), TEXT("orders"));
				Plan.Message->SetBoolField(TEXT("queue"), bQueue);
				Plan.Message->SetArrayField(TEXT("commands"), Parts);
			}
			Plan.Kind = ETCOrderKind::Move;
			Plan.Units = Inf + Guns + Movers.Num();
			Plan.At = Cursor.House;
			Plan.Color = Guns && !Inf ? FColor(0xff, 0x40, 0x30) : FColor(0x9d, 0xd0, 0xff);
			Voice = nullptr; // house: tone 560, no voice
			const int32 Type = M->WorldObjects.HouseTypeAt(Cursor.HouseCell);
			What = FString::Printf(TEXT("House at cell %d (%s): %d garrison, %d fire at it, %d move"), Cursor.HouseCell,
				Type >= 0 ? ANSI_TO_TCHAR(TCWorldCore::HouseTypes[Type].Name) : TEXT("type unknown"), Inf, Guns, Movers.Num());
			// client/main.js orders feedback: the house target has no id, so the cover preview flashes at it
			bGroundOrder = true;
		}
		else if (FTCFormationControls::WouldMove(S, M->UnitTable, Troops, Cursor, false))
		{
			// client/orders.js order 'move' with the Formation menu's shape, spread, together and snap, and the facing
			// gesture's facing and width when it set one (FTCFormationControls: the Rust core's spots when linked)
			ETCFormationSource Source = ETCFormationSource::InterimPort;
			const TArray<FTCFormationTroop> Marching = FTCFormationControls::Troops(S, M->UnitTable, Troops, ETCTroopFilter::Orders);
			if (Marching.Num()) Formation.Formation(S, Marching, Cursor.Ground, Facing, &Source);
			Plan = Formation.PlanMove(S, M->UnitTable, Troops, Cursor.Ground, bAttack, bQueue, Facing);
			if (!Plan.Message.IsValid()) return 0;
			if (Plan.Kind == ETCOrderKind::AttackMove) Voice = TEXT("attack"); // attack-move: tone 500, the attack voice
			What = FString::Printf(TEXT("%s (%s%s, spread %.2f%s, spots from the %s)"), Plan.Kind == ETCOrderKind::AttackMove ? TEXT("Attack-move") : TEXT("Move"), LexToString(Formation.Settings.Shape),
				Facing.bSet ? *FString::Printf(TEXT(" facing %.3f rad, width %.1f m"), Facing.Face, Facing.Reach) : TEXT(""), Formation.Settings.Spread, Formation.Settings.bTogether ? TEXT(", together") : TEXT(""),
				Source == ETCFormationSource::RustCore ? TEXT("Rust core") : TEXT("C++ port"));
			bGroundOrder = true;
		}
		else
		{
			Plan = TCOrders::Plan(S, M->UnitTable, Troops, Cursor, bAttack, bQueue);
			if (Plan.Kind == ETCOrderKind::Unsupported) { NoteOrder(false, TEXT("Not sent: the browser would order ") + Plan.Why); return 0; }
			// the attack's trucks move in the Formation menu's formation (client/orders.js ctx.formation(trucks, g))
			FString T;
			const TArray<TSharedPtr<FJsonValue>>* Parts = nullptr;
			if (Plan.Kind == ETCOrderKind::Attack && Cursor.bHasGround && Plan.Message.IsValid() && Plan.Message->TryGetStringField(TEXT("t"), T) && T == TEXT("orders") && Plan.Message->TryGetArrayField(TEXT("commands"), Parts))
			{
				TArray<FTCFormationTroop> Trucks = FTCFormationControls::Troops(S, M->UnitTable, Troops, ETCTroopFilter::Orders);
				Trucks.RemoveAll([](const FTCFormationTroop& Tr) { return !Tr.bTruck; });
				for (const TSharedPtr<FJsonValue>& Part : *Parts)
				{
					const TSharedPtr<FJsonObject> O = Part.IsValid() ? Part->AsObject() : nullptr;
					FString PT;
					if (O.IsValid() && O->TryGetStringField(TEXT("t"), PT) && PT == TEXT("move")) O->SetArrayField(TEXT("orders"), Formation.Rows(S, Trucks, Cursor.Ground, FTCMoveFacing()));
				}
			}
			What = Plan.Kind == ETCOrderKind::Attack ? TEXT("Attack") : Plan.Kind == ETCOrderKind::AttackMove ? TEXT("Attack-move") : TEXT("Move");
			if (Plan.Kind == ETCOrderKind::Attack || Plan.Kind == ETCOrderKind::AttackMove) Voice = TEXT("attack"); // tone 440 or 500
			bGroundOrder = Plan.Kind != ETCOrderKind::Attack;
		}
	}
	int32 Buildings = 0;
	const TSharedPtr<FJsonObject> Rally = TCArmyCommands::RightClickRallyCommand(S, M->UnitTable, Selected, Cursor, &Buildings);
	if (Rally.IsValid())
	{
		if (Plan.Message.IsValid())
		{
			// the troops' command (or its parts) then the rally, in one group; the parts carry no queue of their own
			TArray<TSharedPtr<FJsonValue>> Parts;
			FString T;
			const TArray<TSharedPtr<FJsonValue>>* Existing = nullptr;
			if (Plan.Message->TryGetStringField(TEXT("t"), T) && T == TEXT("orders") && Plan.Message->TryGetArrayField(TEXT("commands"), Existing)) Parts = *Existing;
			else { Plan.Message->RemoveField(TEXT("queue")); Parts.Add(MakeShared<FJsonValueObject>(Plan.Message)); }
			Parts.Add(MakeShared<FJsonValueObject>(Rally));
			TSharedRef<FJsonObject> Group = MakeShared<FJsonObject>();
			Group->SetStringField(TEXT("t"), TEXT("orders"));
			Group->SetBoolField(TEXT("queue"), bQueue);
			Group->SetArrayField(TEXT("commands"), Parts);
			Plan.Message = Group;
			Plan.Units += Buildings;
			What += FString::Printf(TEXT(" and a rally for %d building%s"), Buildings, Buildings == 1 ? TEXT("") : TEXT("s"));
		}
		else
		{
			const FTCArmyPlan R = TCArmyCommands::PlanRightClickRally(S, M->UnitTable, Selected, Cursor, bQueue);
			if (!R.Message.IsValid()) return 0;
			Plan = R.ToOrderPlan();
			Voice = nullptr; // a rally alone: tone 620, no voice
			bGroundOrder = true; // client/orders.js: a rally alone puts its feedback at the ground
			What = FString::Printf(TEXT("Rally for %d building%s"), Buildings, Buildings == 1 ? TEXT("") : TEXT("s"));
		}
	}
	else if (!Plan.Message.IsValid())
	{
		// nothing selected that takes orders, or no ground: as the browser, silent (a recruit table from another pin
		// says why no rally could be built)
		const FTCArmyPlan R = TCArmyCommands::PlanRightClickRally(S, M->UnitTable, Selected, Cursor, bQueue);
		if (R.bNative && !R.Note.IsEmpty()) NoteOrder(false, R.Note);
		return 0;
	}
	if (bQueue) What += TEXT(" (queued)");
	bool bSent = false;
	const int32 Named = SendPlan(Plan, *What, &bSent);
	if (bSent) Feedback(TEXT("click"), Voice);
	// client/main.js orders feedback: if (at.id === undefined) coverPreview.flash(at.x, at.z)
	if (bSent && bGroundOrder) FlashCover(Cursor.bHouse && Plan.At == Cursor.House ? Cursor.House : Cursor.bHasGround ? Cursor.Ground : Plan.At);
	return Named;
}

bool ATCPlayerController::RunPowerPlan(const FTCSupportPowerPlan& Plan)
{
	const FTCSupportDef* D = Plan.SupportKind.IsEmpty() ? nullptr : FTCSupportPowers::Find(Plan.SupportKind);
	const FString Name = Plan.SupportKind == TEXT("area") ? FString(TEXT("Shell area")) : D ? D->Name : FString(TEXT("Support"));
	if (Plan.bArmAim)
	{
		// FTCSupportPowers armed PowerAim; the controller makes it the one aim (setAim: nothing while observing)
		if (!SetAim(ETCAim::Support, Plan.Note)) { PowerAim.Cancel(); return false; }
		UE_LOG(LogTC, Log, TEXT("%s aim armed: '%s'"), *Name, *Plan.Note);
		return true;
	}
	if (Plan.bCancelAim && Aim == ETCAim::Support) CancelAim();
	if (Plan.Kind == ETCSupportPowerCommand::PinCenter)
	{
		AimHint = PowerAim.Hint;
		NoteOrder(true, FString::Printf(TEXT("%s: centre pinned at (%.1f, %.1f): %s"), *Name, Plan.At.X, Plan.At.Y, *PowerAim.Hint));
		return false;
	}
	if (!Plan.Message.IsValid())
	{
		// the browser's sentence (a refusal flashed over the hint), the hint line (no shellers), or silence
		if (!Plan.Note.IsEmpty()) NoteOrder(false, FString::Printf(TEXT("%s: %s%s"), *Name, *Plan.Note, Plan.bNative ? TEXT(" (this client's own check)") : TEXT("")));
		else UE_LOG(LogTC, Log, TEXT("%s: nothing to send (the browser is silent here)"), *Name);
		return false;
	}
	if (!Plan.PredictedDeny.IsEmpty()) UE_LOG(LogTC, Log, TEXT("%s: the server is expected to answer '%s' (client prediction only; sent as the browser sends it)"), *Name, *Plan.PredictedDeny);
	const FString What = Plan.Kind == ETCSupportPowerCommand::FireAt ? FString::Printf(TEXT("Shell area by %d unit%s%s"), Plan.Units, Plan.Units == 1 ? TEXT("") : TEXT("s"), PowerAim.IsArmed() ? TEXT(" (aim kept: Shift)") : TEXT(""))
		: Plan.Message->HasField(TEXT("dir")) ? FString::Printf(TEXT("%s called in at (%.1f, %.1f), direction %.3f rad"), *Name, Plan.At.X, Plan.At.Y, Plan.Dir) : FString::Printf(TEXT("%s called in"), *Name);
	bool bSent = false;
	SendPlan(Plan.ToOrderPlan(), *What, &bSent);
	// client/main.js: area fire blip(440) and bark('attack'); a support call blip(520)
	if (bSent) Feedback(TEXT("click"), Plan.Kind == ETCSupportPowerCommand::FireAt ? TEXT("attack") : nullptr);
	return bSent;
}

bool ATCPlayerController::RunFormation(ETCFormationControl Control)
{
	ATCGameMode* M = Mode();
	if (!M || !M->State.bHasMatch) return false;
	// reform's up-the-screen facing: the ground under the view's centre and 100 px above it (client/main.js groundAt)
	TOptional<double> Up;
	int32 VX = 0, VY = 0;
	GetViewportSize(VX, VY);
	FVector2D G0, G1;
	if (VX > 0 && VY > 0 && GroundAt(FVector2D(VX / 2.0, VY / 2.0), G0) && GroundAt(FVector2D(VX / 2.0, VY / 2.0 - 100.0), G1)) Up = FTCFormationControls::UpScreenFace(G0, G1);
	const FTCFormationResult R = Formation.Run(Control, M->State, M->UnitTable, Selected, Up);
	if (!R.bDone) return false;
	// client/hud.js: one Orders submenu open at a time
	if (Control == ETCFormationControl::Menu && Formation.bMenuOpen) bBuildMenuOpen = bTrenchMenuOpen = false;
	UE_LOG(LogTC, Log, TEXT("formation control '%s': shape %s, spread %.2f, together %s, snap %s, menu %s%s"), FTCFormationControls::Key(Control), LexToString(Formation.Settings.Shape), Formation.Settings.Spread,
		Formation.Settings.bTogether ? TEXT("on") : TEXT("off"), Formation.Settings.bSnap ? TEXT("on") : TEXT("off"), Formation.bMenuOpen ? TEXT("open") : TEXT("closed"),
		R.Feedback.IsEmpty() ? TEXT("") : *FString::Printf(TEXT("; feedback '%s'"), *R.Feedback));
	if (!R.Feedback.IsEmpty()) NoteOrder(true, R.Feedback);
	if (!R.Message.IsValid())
	{
		if (!R.Why.IsEmpty()) UE_LOG(LogTC, Log, TEXT("re-form: nothing sent, %s (the browser is silent here)"), *R.Why);
		return true;
	}
	// reform: sendCmd({ t: 'move', orders, face }) with no marker or sound of its own
	FTCOrderPlan Plan;
	Plan.Kind = ETCOrderKind::Move;
	Plan.Message = R.Message;
	Plan.Units = R.Units;
	SendPlan(Plan, *FString::Printf(TEXT("Re-form where they stand (%s, spacing %.2fx)"), LexToString(Formation.Settings.Shape), Formation.Settings.Spread));
	return true;
}

FString ATCPlayerController::LanguageLabel() const
{
	const FTCLanguagePicker P = FTCLocale::Picker();
	const FTCLanguagePicker::FOption* O = P.Options.FindByPredicate([&P](const FTCLanguagePicker::FOption& X) { return X.Language == P.Selected; });
	return FString::Printf(TEXT("%s: %s"), *P.Title, O ? *O->Label : FTCLocale::Code(P.Selected));
}

FString ATCPlayerController::UiScaleLabel() const
{
	// round 15 (lead): "UI scale: 100%" as the Volume button writes its value, so TC_T's ': ' rule draws es.js's
	// 'UI scale' (Escala de la interfaz) before the number; "UI scale 100%" matched no key and stayed English
	return FString::Printf(TEXT("%s: %s"), FTCSettingsStore::UIScaleTitle(), *FTCSettingsStore::UIScaleLabel(UiScalePercent));
}

FString ATCPlayerController::VolumeLabel() const
{
	const UTCAudio* A = GetWorld() ? GetWorld()->GetSubsystem<UTCAudio>() : nullptr;
	return A ? FString::Printf(TEXT("Volume: %d%%"), FMath::RoundToInt(A->Volume() * 100.0)) : FString(TEXT("Volume: no sound"));
}

void ATCPlayerController::Feedback(const TCHAR* Ui, const TCHAR* Voice)
{
	UTCAudio* A = GetWorld() ? GetWorld()->GetSubsystem<UTCAudio>() : nullptr;
	if (!A) return;
	if (Ui) A->Blip(Ui);
	if (Voice) A->Voice(Voice);
	UE_LOG(LogTC, Verbose, TEXT("feedback sound: ui %s, voice %s"), Ui ? Ui : TEXT("-"), Voice ? Voice : TEXT("-"));
}

bool ATCPlayerController::PingAt(FVector2D Pixel, bool bMinimap)
{
	ATCGameMode* M = Mode();
	if (!M || !M->State.bHasMatch) return false;
	FVector2D G;
	if (bMinimap)
	{
		// client/main.js minimap mousedown: rig.cancelFollow(), then mmToWorld (not clamped)
		const ATCHUD* Hud = Cast<ATCHUD>(GetHUD());
		CancelFollow();
		if (!Hud || !Hud->Minimap.PixelToWorld(Pixel, G)) return false;
	}
	// client/main.js groundAt: nothing is sent when the pixel finds no ground
	else if (!GroundAt(Pixel, G)) return false;
	return M->SendPing(G, bMinimap ? TEXT("Alt+click on the minimap") : TEXT("Alt+click on the view"));
}

int32 ATCPlayerController::IssueStop()
{
	ATCGameMode* M = Mode();
	if (!M || !M->State.bHasMatch) return 0;
	if (!Selected.Num()) { NoteOrder(false, TEXT("Stop: nothing selected")); return 0; }
	bool bSent = false;
	const int32 Named = SendPlan(TCOrders::PlanStop(Selected), TEXT("Stop"), &bSent);
	if (bSent) Feedback(TEXT("click")); // client/main.js stop: blip(330)
	return Named;
}

bool ATCPlayerController::OnboardingPress()
{
	ATCGameMode* M = Mode();
	if (!M || !bControlsSheetOpen || !M->Onboarding.OnPointerDown()) return false;
	bControlsSheetOpen = false;
	M->Onboarding.OnClosed();
	UE_LOG(LogTC, Log, TEXT("Controls sheet closed by a press (onboarding: seen is stored)"));
	return true;
}

bool ATCPlayerController::RunCommand(const FTCUiCommand& Command, bool bShift)
{
	ATCGameMode* M = Mode();
	if (!M) return false;
	const ETCUiAction Action = Command.Action;
	const FTCMatchState& S = M->State;
	auto Send = [&](TSharedRef<FJsonObject> Msg, const FString& Note)
	{
		const bool bSent = M->SendCommand(Msg);
		NoteOrder(bSent, bSent ? Note : FString::Printf(TEXT("%s not sent: %s"), *Note, *M->LastSendProblem()));
		return bSent;
	};
	auto Simple = [](const TCHAR* T) { TSharedRef<FJsonObject> O = MakeShared<FJsonObject>(); O->SetStringField(TEXT("t"), T); return O; };
	// The server ignores lobby commands from anyone but the host (server.js); the buttons only show for the host, and
	// a switch that runs one anyway says why it does nothing.
	const bool bLobbyHost = S.bAmHost && !S.bSpectator && S.RoomState != TEXT("play");
	switch (Action)
	{
	case ETCUiAction::AddAi:
		if (!bLobbyHost || S.RoomState != TEXT("lobby")) { NoteOrder(false, TEXT("Add AI: only the host, in the lobby")); return false; }
		return Send(Simple(TEXT("addAi")), FString::Printf(TEXT("Add AI sent (room had %d seats)"), S.LobbyPlayers));
	case ETCUiAction::NextMode:
	{
		if (!bLobbyHost) { NoteOrder(false, TEXT("Mode: only the host, outside a match")); return false; }
		int32 Next = 0;
		for (int32 I = 0; I < UE_ARRAY_COUNT(CycledModes); I++) if (S.Mode == CycledModes[I]) Next = (I + 1) % UE_ARRAY_COUNT(CycledModes);
		TSharedRef<FJsonObject> O = Simple(TEXT("mode"));
		O->SetStringField(TEXT("v"), CycledModes[Next]);
		return Send(O, FString::Printf(TEXT("Mode %s asked (was %s)"), CycledModes[Next], *S.Mode));
	}
	case ETCUiAction::Start:
		if (!bLobbyHost) { NoteOrder(false, TEXT("Start: only the host, outside a match")); return false; }
		// client/main.js renderLobby disables Start when the seats do not fit (server.js 'start' drops it without a word)
		if (M->Lobby.InLobby() && !M->Lobby.StartAllowed()) { NoteOrder(false, FString::Printf(TEXT("%s: not sent, the browser disables it: %s"), *M->Lobby.StartLabel(), *M->Lobby.StartLine())); return false; }
		return Send(Simple(TEXT("start")), FString::Printf(TEXT("Start sent: %s on %s, %d seats"), *S.Mode, *S.LobbyMap, S.LobbyPlayers));
	case ETCUiAction::End:
		if (!S.bAmHost || S.RoomState != TEXT("play")) { NoteOrder(false, TEXT("End match: only the host, during a match")); return false; }
		return Send(Simple(TEXT("end")), FString::Printf(TEXT("End match sent (match %d)"), S.MatchId));
	case ETCUiAction::LobbySetting:
	{
		// client/main.js renderLobby's selects: one change sends one message; the server answers with a lobby, or not at
		// all when it drops it (FTCLobbySettings keeps it pending until a lobby shows the value)
		int32 KindIndex = -1;
		for (int32 K = 0; K <= (int32)ETCLobbySetting::Level; K++) if (Command.Unit == LexToString((ETCLobbySetting)K)) KindIndex = K;
		if (KindIndex < 0) { NoteOrder(false, FString::Printf(TEXT("Lobby: unknown setting '%s'"), *Command.Unit)); return false; }
		const FTCLobbyRequest R = M->Lobby.PlanStep((ETCLobbySetting)KindIndex, Command.Group, Command.Job < 0 ? -1 : 1);
		if (!R.Message.IsValid()) { NoteOrder(false, R.Note); UE_LOG(LogTC, Log, TEXT("lobby %s: nothing sent: %s"), *Command.Describe(), *R.Note); return false; }
		const bool bSent = M->SendCommand(R.Message.ToSharedRef());
		if (bSent) M->Lobby.NoteSent(R, FPlatformTime::Seconds());
		NoteOrder(bSent, bSent ? R.Note : FString::Printf(TEXT("%s not sent: %s"), *R.Note, *M->LastSendProblem()));
		UE_LOG(LogTC, Log, TEXT("lobby %s: %s (%s), pending %d"), *Command.Describe(), *R.Note, bSent ? TEXT("sent") : *M->LastSendProblem(), M->Lobby.PendingCount());
		return bSent;
	}
	case ETCUiAction::SaveMatch:
	case ETCUiAction::LoadSave:
	{
		// client/main.js saveBtn and the saves panel's Load buttons (FTCPersistence: who may, and what the server will do)
		const double Now = FPlatformTime::Seconds();
		const FTCPersistencePlan P = Action == ETCUiAction::SaveMatch ? M->Persistence.PlanSave(S) : M->Persistence.PlanLoad(S, Command.Unit);
		const bool bSent = P.Message.IsValid() && M->SendCommand(P.Message.ToSharedRef());
		M->Persistence.NoteSent(P, bSent, Now);
		const FString What = Action == ETCUiAction::SaveMatch ? FString(TEXT("Save match")) : FString::Printf(TEXT("Load %s"), *Command.Unit);
		const FString Note = !P.Message.IsValid() ? FString::Printf(TEXT("%s: %s"), *What, *P.Note) : bSent ? FString::Printf(TEXT("%s sent: %s"), *What, *P.Note) : FString::Printf(TEXT("%s not sent: %s"), *What, *M->LastSendProblem());
		NoteOrder(bSent, Note);
		UE_LOG(LogTC, Log, TEXT("%s; expect %s; status %s"), *Note, LexToString(P.Expect), *M->Persistence.Status().Describe());
		return bSent;
	}
	case ETCUiAction::PauseMatch:
	{
		// client/main.js pauseBtn: the host (a watching host too: 'pause' and 'resume' are observerControls), in play
		if (!S.bAmHost || S.RoomState != TEXT("play")) { NoteOrder(false, TEXT("Pause: only the host, during a match")); return false; }
		const bool bResume = S.bPaused;
		return Send(Simple(bResume ? TEXT("resume") : TEXT("pause")), FString::Printf(TEXT("%s sent at tick %d"), bResume ? TEXT("Resume match") : TEXT("Pause match"), S.Tick));
	}
	case ETCUiAction::Research:
	{
		// client/hud.js drawTech onclick: attempt(action, () => { send(action); blip('recruit') }), else the reason
		ETCTechKind Kind;
		if (!FTCProgression::ParseKind(Command.Unit, Kind)) { NoteOrder(false, FString::Printf(TEXT("Research: unknown kind '%s'"), *Command.Unit)); return false; }
		const FTCResearchPress P = M->Progress.Research(S, Kind);
		if (!P.Message.IsValid())
		{
			NoteOrder(false, FString::Printf(TEXT("Research %s: %s%s"), *Command.Unit, *P.Check.Reason, P.Check.bNative ? TEXT(" (this client's own check)") : TEXT("")));
			Feedback(TEXT("error")); // client/hud.js attempt: ctx.act('explain', reason), feedback.show plays the error blip
			UE_LOG(LogTC, Log, TEXT("research %s: refused here, nothing sent: %s%s"), *Command.Unit, *P.Check.Reason, P.Check.bNative ? TEXT(" (native)") : TEXT(""));
			return false;
		}
		const bool bSent = Send(P.Message.ToSharedRef(), FString::Printf(TEXT("Research %s sent"), *Command.Unit));
		if (bSent && P.bBlip) Feedback(TEXT("recruit"));
		UE_LOG(LogTC, Log, TEXT("research %s: %s at tick %d (MP %d)%s"), *Command.Unit, bSent ? TEXT("sent") : *M->LastSendProblem(), S.Tick, S.Mp, bSent && P.bBlip ? TEXT(", recruit blip") : TEXT(""));
		return bSent;
	}
	case ETCUiAction::Recover:
	{
		// client/hud.js the Restore card's onclick: send({ t: 'recover' }); the server may still refuse (a deny)
		const FTCWorldRecoverResult R = M->WorldRec.Recover(S, Selected);
		if (!R.bSend || !R.Message.IsValid())
		{
			NoteOrder(false, FString::Printf(TEXT("Restore: %s%s"), *R.Reason, R.bNative ? TEXT(" (this client's own check)") : TEXT("")));
			UE_LOG(LogTC, Log, TEXT("restore: refused here, nothing sent: %s"), *R.Reason);
			return false;
		}
		const FTCWorldRecoveryCard C = M->WorldRec.Card(S, Selected);
		const bool bSent = Send(R.Message.ToSharedRef(), FString::Printf(TEXT("%s sent (%s)"), *C.Name, *C.Cost));
		UE_LOG(LogTC, Log, TEXT("restore: %s %s at tick %d (MP %d)"), *C.Name, bSent ? TEXT("sent") : *M->LastSendProblem(), S.Tick, S.Mp);
		return bSent;
	}
	case ETCUiAction::MatchOption:
	{
		// client/main.js and weather-view.js: the Weather select, the Sandbox checkbox and a seat's Remove button; one
		// change sends one message, and the server answers with a lobby or says nothing (FTCMatchOptions keeps it pending)
		FTCOptionRequest R;
		if (Command.Unit == LexToString(ETCMatchOption::Weather)) R = M->Options.PlanWeatherStep(Command.Job < 0 ? -1 : 1);
		else if (Command.Unit == LexToString(ETCMatchOption::Sandbox)) R = M->Options.PlanSandboxToggle();
		else if (Command.Unit == LexToString(ETCMatchOption::Kick)) R = M->Options.PlanKick(Command.Group);
		else { NoteOrder(false, FString::Printf(TEXT("Lobby: unknown option '%s'"), *Command.Unit)); return false; }
		if (!R.Message.IsValid()) { NoteOrder(false, R.Note); UE_LOG(LogTC, Log, TEXT("%s: nothing sent: %s"), *Command.Describe(), *R.Note); return false; }
		const bool bSent = M->SendCommand(R.Message.ToSharedRef());
		if (bSent) M->Options.NoteSent(R, FPlatformTime::Seconds());
		NoteOrder(bSent, bSent ? R.Note : FString::Printf(TEXT("%s not sent: %s"), *R.Note, *M->LastSendProblem()));
		UE_LOG(LogTC, Log, TEXT("%s: %s (%s), pending %d"), *Command.Describe(), *R.Note, bSent ? TEXT("sent") : *M->LastSendProblem(), M->Options.PendingCount());
		return bSent;
	}
	case ETCUiAction::Stop:
		return IssueStop() > 0;
	case ETCUiAction::AttackMove:
	{
		// client/main.js amove: selected.size && setAim('amove')
		if (!S.bHasMatch || !Selected.Num()) { NoteOrder(false, TEXT("Attack-move: select units first")); return false; }
		return SetAim(ETCAim::AttackMove, TEXT("Click where to attack-move. Right-click cancels"));
	}
	case ETCUiAction::Retreat:
	{
		// client/main.js retreat: the whole selection; the server keeps its own movable units ('needs' when none)
		if (!S.bHasMatch) return false;
		if (!Selected.Num()) { NoteOrder(false, TEXT("Retreat: nothing selected")); return false; }
		const FTCArmyPlan Plan = TCArmyCommands::PlanRetreat(S, M->UnitTable, Selected);
		if (!Plan.PredictedDeny.IsEmpty()) UE_LOG(LogTC, Log, TEXT("retreat: none of the %d selected takes it as this client sees them; the server is expected to answer '%s'"), Plan.Units, *Plan.PredictedDeny);
		bool bSent = false;
		SendPlan(Plan.ToOrderPlan(), TEXT("Retreat"), &bSent);
		if (bSent) Feedback(TEXT("click"), TEXT("retreat")); // client/main.js retreat: blip(260); bark('retreat')
		return bSent;
	}
	case ETCUiAction::Stance:
	{
		ETCStance Stance;
		if (!S.bHasMatch || !TCStance::FromKey(Command.Unit, Stance)) return false;
		// client/main.js toggleStance: on for the own selected non-structure units, unless every one has it already
		const FTCArmyPlan Plan = TCArmyCommands::PlanStance(S, M->UnitTable, Selected, Stance);
		if (!Plan.Message.IsValid()) { NoteOrder(false, FString::Printf(TEXT("%s: %s"), TCStance::Name(Stance), *Plan.Note)); return false; }
		bool bSent = false;
		SendPlan(Plan.ToOrderPlan(), *FString::Printf(TEXT("%s %s"), TCStance::Name(Stance), Plan.bStanceOn ? TEXT("on") : TEXT("off")), &bSent);
		if (bSent) Feedback(TEXT("click")); // client/main.js toggleStance: blip(480)
		return bSent;
	}
	case ETCUiAction::Rally:
	{
		// client/main.js startRally: arm the rally aim, or say why not
		const FTCArmyPlan Plan = TCArmyCommands::PlanArmRally(S, M->UnitTable, Selected);
		if (Plan.bArmAim) return SetAim(ETCAim::Rally, FString(TCArmyCommands::RallyAimHint));
		if (!Plan.Note.IsEmpty()) NoteOrder(false, Plan.Note);
		return false;
	}
	case ETCUiAction::Build:
	{
		// client/main.js startBuild: availability first (its sentence when refused), then the aim
		if (!S.bHasMatch) return false;
		const FTCBuildArm Arm = M->Build.Arm(S, Command.Unit, Selected);
		const FTCBuildDef* D = FTCConstruction::Find(Command.Unit);
		if (!Arm.bArm) { NoteOrder(false, FString::Printf(TEXT("Build %s: %s%s"), D ? *D->Name : *Command.Unit, *Arm.Check.Reason, Arm.Check.bNative ? TEXT(" (this client's own check)") : TEXT(""))); return false; }
		return SetAim(ETCAim::Build, Arm.Hint, Command.Unit);
	}
	case ETCUiAction::BuildMenu:
		bBuildMenuOpen = !bBuildMenuOpen;
		bTrenchMenuOpen = false;
		Formation.bMenuOpen = false;
		return true;
	case ETCUiAction::TrenchMenu:
		bTrenchMenuOpen = !bTrenchMenuOpen;
		bBuildMenuOpen = false;
		Formation.bMenuOpen = false;
		return true;
	case ETCUiAction::Formation:
	{
		// client/hud.js data-m="form" and data-f: the Form menu button and its shape, spacing and switch buttons
		const ETCFormationControl Control = FTCFormationControls::ParseKey(Command.Unit);
		return Control != ETCFormationControl::None && RunFormation(Control);
	}
	case ETCUiAction::Support:
		// client/main.js aimSupport: availability first (its sentence when refused), then the aim
		if (!S.bHasMatch) return false;
		return RunPowerPlan(M->Powers.ArmSupport(S, M->UnitTable, M->Recruit, Command.Unit, PowerAim));
	case ETCUiAction::Area:
		// client/main.js startArea: the aim with a selected own salvo unit, else the hint line says what to select
		if (!S.bHasMatch) return false;
		return RunPowerPlan(M->Powers.ArmArea(S, M->UnitTable, Selected, PowerAim));
	case ETCUiAction::CoverToggle:
	{
		// client/cover-preview.js menu button: on = !on, kept as localStorage 'ww2-cover' is (here the user settings file)
		M->Cover.SetEnabled(!M->Cover.IsEnabled());
		if (GConfig) GConfig->SetBool(TEXT("ThreeCrossroads"), TEXT("CoverPreview"), M->Cover.IsEnabled(), GGameUserSettingsIni);
		NoteOrder(true, FTCCoverPreview::MenuLabel(M->Cover.IsEnabled()));
		return true;
	}
	case ETCUiAction::Fort:
	case ETCUiAction::Entrench:
	{
		// client/main.js startDig and startEntrench: availability first (its sentence when refused), then the aim. A
		// line fort other than the trench arms a line made of it (startDig does that itself).
		if (!S.bHasMatch) return false;
		const FTCFieldArm Arm = Action == ETCUiAction::Fort ? M->Field.StartDig(S, Command.Unit, Selected, FieldAim) : M->Field.StartEntrench(S, Command.Unit, TEXT("trench"), Selected, FieldAim);
		if (!Arm.bArm)
		{
			// refused: the browser's sentence; allowed but not armed (observing): silent, as setAim
			if (!Arm.Check.bOk) NoteOrder(false, FString::Printf(TEXT("%s: %s%s"), *Command.Describe(), Arm.Feedback.IsEmpty() ? *Arm.Check.Reason : *Arm.Feedback, Arm.Check.bNative ? TEXT(" (this client's own check)") : TEXT("")));
			return false;
		}
		bBuildMenuOpen = bTrenchMenuOpen = false;
		if (!SetAim(ETCAim::Field, FieldAim.Hint)) { FieldAim.Cancel(); return false; }
		return true;
	}
	case ETCUiAction::Ability:
		if (!S.bHasMatch) return false;
		return RunAbilityPlan(M->Abilities.PlanUse(S, M->UnitTable, Selected, Command.Unit));
	case ETCUiAction::Autocast:
		if (!S.bHasMatch) return false;
		return RunAbilityPlan(M->Abilities.PlanAutocast(S, M->UnitTable, Selected, Command.Unit));
	case ETCUiAction::MountUp:
		if (!S.bHasMatch) return false;
		return RunSupportPlan(M->Support.PlanMountUp(S, M->UnitTable, Selected), TEXT("Mount up"));
	case ETCUiAction::Unload:
		if (!S.bHasMatch) return false;
		return RunSupportPlan(M->Support.PlanUnload(S, M->UnitTable, Selected), TEXT("Unload"));
	case ETCUiAction::CancelSite:
		if (!S.bHasMatch) return false;
		return RunSupportPlan(M->Support.PlanCancelSite(S, M->UnitTable, Selected, M->Recruit.Classic()), TEXT("Cancel site"));
	case ETCUiAction::Resume:
	{
		// client/hud.js data-truck-resume: the selected own trucks go back to automatic deliveries
		const FTCLogisticsOrder O = FTCLogistics::PlanResume(S, Selected);
		if (!O.Message.IsValid()) { NoteOrder(false, TEXT("Resume deliveries: ") + (O.Why.IsEmpty() ? FString(TEXT("select trucks")) : O.Why)); return false; }
		FTCOrderPlan Plan;
		Plan.Message = O.Message;
		Plan.Units = O.Trucks.Num();
		bool bSent = false;
		SendPlan(Plan, TEXT("Resume deliveries"), &bSent);
		return bSent;
	}
	case ETCUiAction::SupplyOverlay:
		M->Logistics.ToggleOverlay();
		NoteOrder(true, FString::Printf(TEXT("Supply overlay %s"), M->Logistics.OverlaySwitch() ? TEXT("on") : TEXT("off")));
		return true;
	case ETCUiAction::TruckSelect:
		M->Logistics.ToggleSelectionMode();
		NoteOrder(true, FString::Printf(TEXT("Truck selection %s: a drag now takes %s"), M->Logistics.SelectionSwitch() ? TEXT("on") : TEXT("off"), M->Logistics.SelectionModeActive() ? TEXT("only your trucks") : TEXT("your army without trucks")));
		return true;
	case ETCUiAction::Destroy:
	{
		// client/main.js actions.destroy: if (selected.size) sendCmd({ t: 'destroy', ids: [...selected] }) (no refund)
		if (!S.bHasMatch || !Selected.Num()) return false;
		TSharedRef<FJsonObject> O = Simple(TEXT("destroy"));
		O->SetArrayField(TEXT("ids"), IdValues(Selected.Array()));
		FTCOrderPlan Plan;
		Plan.Message = O;
		Plan.Units = Selected.Num();
		bool bSent = false;
		SendPlan(Plan, TEXT("Destroy (no refund)"), &bSent);
		if (bSent) Feedback(TEXT("click")); // client/main.js actions.destroy: blip(110)
		return bSent;
	}
	case ETCUiAction::Cover:
	{
		// client/main.js takeCover: nothing without a selection; availability({ t: 'cover' }) says why it cannot; then
		// { t: 'cover', ids: [...selected], queue } (the key never queues, the Orders button's Shift does)
		if (!S.bHasMatch || !Selected.Num()) return false;
		FString Why;
		if (S.Snapshots <= 0) Why = TEXT("Waiting for the army");
		else if (S.bSpectator) Why = TEXT("You are spectating");
		else
		{
			int32 Squads = 0, Retreating = 0;
			for (const int32 Id : Selected)
			{
				const FTCUnit* U = S.Units.Find(Id);
				const FTCUnitDef* D = U ? M->UnitTable.Find(U->Type) : nullptr;
				if (!U || !D || !S.IsOwn(*U) || !D->bInfantry) continue;
				Squads++;
				Retreating += (U->Flags & TCFlags::Retreating) ? 1 : 0;
			}
			if (!Squads) Why = TEXT("Select an infantry squad");
			else if (Squads == Retreating) Why = TCOrders::DenySentence(TEXT("retreating"), FString());
		}
		if (!Why.IsEmpty()) { NoteOrder(false, TEXT("Take cover: ") + Why); return false; }
		TSharedRef<FJsonObject> O = Simple(TEXT("cover"));
		O->SetArrayField(TEXT("ids"), IdValues(Selected.Array()));
		O->SetBoolField(TEXT("queue"), bShift);
		FTCOrderPlan Plan;
		Plan.Message = O;
		Plan.Units = Selected.Num();
		bool bSent = false;
		SendPlan(Plan, bShift ? TEXT("Take cover (queued)") : TEXT("Take cover"), &bSent);
		if (bSent) Feedback(TEXT("click")); // client/main.js takeCover: blip(420)
		return bSent;
	}
	case ETCUiAction::Rail:
		// client/main.js startRail: World Conquest only (silent elsewhere); the aim, or the hint line says what to select
		if (!S.bHasMatch) return false;
		if (!WorldMode(S)) { UE_LOG(LogTC, Log, TEXT("send by rail: World Conquest only (the browser does nothing here)")); return false; }
		if (!Railers().Num()) { NoteOrder(false, TEXT("Select ground units near one of your stations to send them by rail")); return false; }
		return SetAim(ETCAim::Rail, TEXT("Click the destination: they ride to your station nearest it. Right-click cancels"));
	case ETCUiAction::Army:
	{
		// client/selection.js army: the whole army replaces the selection (no trucks, trains, riders or parked planes)
		if (!S.bHasMatch) return false;
		const TArray<int32> Ids = TCSelectionKeys::Army(KeyUnits(false));
		ReplaceSelection(Ids, false);
		UE_LOG(LogTC, Log, TEXT("select the army: %d units"), Ids.Num());
		return Ids.Num() > 0;
	}
	case ETCUiAction::Idle:
	case ETCUiAction::IdleAll:
	case ETCUiAction::IdleEngineer:
	{
		// client/selection.js findIdle: '.' walks the idle ground units and centres the one chosen, Shift+'.' takes them
		// all, ',' walks the idle Engineers
		if (!S.bHasMatch) return false;
		if (!PlanKindOf && !bPlanWarned)
		{
			bPlanWarned = true;
			UE_LOG(LogTC, Warning, TEXT("idle keys: the snapshot's plans are not decoded in this client (PlanKindOf is not wired), so idle is judged by the flags alone and a moving unit can count as idle"));
		}
		const FTCIdleResult R = IdleCursor.Find(KeyUnits(false), Action == ETCUiAction::IdleAll, Action == ETCUiAction::IdleEngineer);
		if (R.bReplace) ReplaceSelection(R.Ids, R.bCentre);
		UE_LOG(LogTC, Log, TEXT("%s: %d selected%s"), LexToString(Action), R.Ids.Num(), R.bCentre ? TEXT(", centred") : TEXT(""));
		return R.Ids.Num() > 0;
	}
	case ETCUiAction::Follow:
		return StartFollow();
	case ETCUiAction::Alert:
	{
		// round 15: a click on the lobby report's summary (report.js details) or on the alert feed (alerts.js init's
		// mousedown, the history's summary, Previous Alert, Next Alert and its entries: FTCAlerts::Click). hooks.jump
		// cancels a follow, then moves the camera.
		if (Command.Unit == TEXT("report:toggle")) { M->EndReport.Toggle(); UE_LOG(LogTC, Log, TEXT("match report %s"), M->EndReport.IsOpen() ? TEXT("opened") : TEXT("closed")); return true; }
		if (!S.bHasMatch) return false;
		if (!Command.Unit.IsEmpty())
		{
			FVector2D At;
			bool bJump = false;
			if (!M->AlertFeed.Click(Command.Unit, At, bJump)) return false;
			M->bAlertHistoryOpen = M->AlertFeed.bHistoryOpen;
			UE_LOG(LogTC, Log, TEXT("alert click %s%s"), *Command.Unit, bJump ? *FString::Printf(TEXT(": look at (%.1f, %.1f)"), At.X, At.Y) : TEXT(""));
			if (bJump) { CancelFollow(); FocusOn(At); }
			return true;
		}
		// client/main.js actions.alert: the newest alert's point, else the selection's centre; a follow ends
		CancelFollow();
		FVector2D At;
		if (NewestAlertAt && NewestAlertAt(At)) { FocusOn(At); return true; }
		if (!TCSelectionKeys::Centre(KeyUnits(false), Selected.Array(), At)) return false;
		FocusOn(At);
		return true;
	}
	case ETCUiAction::Home:
	{
		// client/main.js actions.home: World Conquest goes to the own HQ (else home), other modes to home (the start's
		// spawn: the snapshot's later 'home' is not decoded here); in Classic the HQ is then selected
		if (!S.bHasMatch) return false;
		int32 Hq = 0;
		for (const auto& Pair : S.Units)
			if (S.IsOwn(Pair.Value) && Pair.Value.Type == TEXT("hq") && (!Hq || Pair.Key < Hq)) Hq = Pair.Key;
		const FTCUnit* HqUnit = Hq ? S.Units.Find(Hq) : nullptr;
		const FVector2D Target = WorldMode(S) && HqUnit ? FVector2D(HqUnit->X, HqUnit->Z) : S.Spawn;
		SkipIntro();
		CancelFollow();
		FrameOn(Target);
		if (TCArmyCommands::ClassicLobby(S) && Hq) { Selected.Reset(); Selected.Add(Hq); }
		UE_LOG(LogTC, Log, TEXT("home: (%.1f, %.1f)%s"), Target.X, Target.Y, TCArmyCommands::ClassicLobby(S) && Hq ? *FString::Printf(TEXT(", HQ %d selected"), Hq) : TEXT(""));
		return true;
	}
	case ETCUiAction::ControlsSheet:
		// client/main.js openControls (cancelInput first) and closeControls
		bControlsSheetOpen = !bControlsSheetOpen;
		if (bControlsSheetOpen) CancelInput();
		else M->Onboarding.OnClosed(); // round 14: the dialog's close event (onboarding stores 'seen')
		UE_LOG(LogTC, Log, TEXT("Controls sheet %s"), bControlsSheetOpen ? TEXT("open (F1 or Esc closes it)") : TEXT("closed"));
		return true;
	case ETCUiAction::MatchControl:
		if (ATCHUD* H=Cast<ATCHUD>(GetHUD()); H && H->RunPanelCommand(Command,bShift,IsInputKeyDown(EKeys::LeftControl)||IsInputKeyDown(EKeys::RightControl))) return true;
		return M->RunControl(Command, Command.Job < 0); // round 15 (Client/TCMatchControls.cpp)
	case ETCUiAction::Tutorial:
		// client/public-lobby.js tutorial button: a new private room whose hello carries { tutorial: true }
		if (S.RoomState == TEXT("play")) { NoteOrder(false, TEXT("Tutorial: leave the match first")); return false; }
		M->EnterTutorialRoom();
		NoteOrder(true, TEXT("Tutorial: a new private room"));
		return true;
	case ETCUiAction::EdgeScroll:
		bEdgeScroll = !bEdgeScroll;
		if (GConfig) GConfig->SetBool(TEXT("ThreeCrossroads"), TEXT("EdgeScroll"), bEdgeScroll, GGameUserSettingsIni);
		NoteOrder(true, EdgeScrollLabel());
		return true;
	case ETCUiAction::PanSpeed:
		PanSpeed = (PanSpeed + 1) % TCCameraFeel::PanSpeedCount;
		if (GConfig) GConfig->SetInt(TEXT("ThreeCrossroads"), TEXT("PanSpeed"), PanSpeed, GGameUserSettingsIni);
		NoteOrder(true, PanSpeedLabel());
		return true;
	case ETCUiAction::Language:
	{
		// client/i18n.js setLang: the other language, stored ([ThreeCrossroads] Language in the user settings) and in
		// use at once (the browser reloads the page; every HUD text is drawn through TC_T, so the next frame shows it)
		const ETCLanguage Next = FTCLocale::Language() == ETCLanguage::Spanish ? ETCLanguage::English : ETCLanguage::Spanish;
		FTCLocale::SetLanguage(Next);
		NoteOrder(true, LanguageLabel());
		UE_LOG(LogTC, Log, TEXT("language picked: %s (%s)"), FTCLocale::Code(Next), *TC_T(LanguageLabel()));
		return true;
	}
	case ETCUiAction::Graphics:
		// client/light.js gfxBtn.onclick: gfx.set(gfx.low ? 'high' : 'low') (saved; everything listening relights)
		FTCGraphics::Get().Toggle();
		NoteOrder(true, FTCGraphics::MenuLabel(FTCGraphics::Get().Level()));
		return true;
	case ETCUiAction::Gore:
		FTCGraphics::Get().SetGore(!FTCGraphics::Get().Gore());
		NoteOrder(true, FTCGraphics::GoreLabel(FTCGraphics::Get().Gore()));
		return true;
	case ETCUiAction::UiScale:
	{
		// client/hud.js uiScale: the slider's value (75..150, step 5) applied on release and saved; this HUD steps it
		const int32 Step = FTCSettingsStore::UIScaleStep;
		UiScalePercent = FMath::Clamp(UiScalePercent + (Command.Job < 0 ? -Step : Step), FTCSettingsStore::UIScaleMin, FTCSettingsStore::UIScaleMax);
		FTCSettingsStore::Game().SetUIScale(UiScalePercent);
		UiScalePercent = FTCSettingsStore::Game().UIScale();
		NoteOrder(true, UiScaleLabel());
		return true;
	}
	case ETCUiAction::Volume:
	{
		// client/index.html's Volume slider (0 to 100, audio.setVolume, saved); this HUD steps it by 10 points
		UTCAudio* A = GetWorld() ? GetWorld()->GetSubsystem<UTCAudio>() : nullptr;
		if (!A) { NoteOrder(false, TEXT("Volume: no sound in this run")); return false; }
		const int32 Now = FMath::RoundToInt(A->Volume() * 100.0);
		const int32 Next = Command.Job < 0 ? (Now <= 0 ? 100 : FMath::Max(0, (Now - 1) / 10 * 10)) : (Now >= 100 ? 0 : FMath::Min(100, Now / 10 * 10 + 10));
		A->SetVolume(Next / 100.0);
		NoteOrder(true, VolumeLabel());
		return true;
	}
	case ETCUiAction::RecruitGroup:
		// client/hud.js: a group tab shows its cards and hides the others; a second click closes it
		RecruitGroupOpen = RecruitGroupOpen == Command.Group ? -1 : Command.Group;
		return true;
	case ETCUiAction::Recruit:
	case ETCUiAction::CancelJob:
	{
		if (!S.bHasMatch) { NoteOrder(false, TEXT("Recruit: no match")); return false; }
		const FTCRecruitment& R = M->Recruit;
		FTCBuyResult Res;
		FString What;
		if (Action == ETCUiAction::Recruit)
		{
			// client/hud.js buy: a card names its building in Classic; outside Classic recruitAction asks the first
			// selected building that makes the unit, else the server chooses
			FTCBuy Buy;
			if (Command.Building) { Buy.Unit = Command.Unit; Buy.From = Command.Building; Buy.bHasFrom = true; }
			else Buy = R.RecruitAction(S, Command.Unit, Selected);
			const FTCRecruitDef* D = FTCRecruitment::Find(Command.Unit);
			What = FString::Printf(TEXT("Recruit %s"), D ? *D->Name : *Command.Unit);
			Res = R.Buy(S, Buy, bShift);
		}
		else
		{
			What = FString::Printf(TEXT("Cancel queued recruit (job %d)"), Command.Job);
			Res = R.CancelProduction(S, Command.Building, Command.Job);
		}
		// refused by the browser's availability check: its sentence, and nothing is sent (client/hud.js attempt)
		if (!Res.Check.bOk) { NoteOrder(false, FString::Printf(TEXT("%s: %s%s"), *What, *Res.Check.Reason, Res.Check.bNative ? TEXT(" (this client's own check)") : TEXT(""))); Feedback(TEXT("error")); return false; }
		if (!S.CanOrder()) { NoteOrder(false, FString::Printf(TEXT("%s not sent: %s"), *What, WhyNoOrders(S))); return false; }
		int32 Sent = 0;
		FString Wire;
		for (const TSharedRef<FJsonObject>& Msg : Res.Messages)
		{
			if (!M->SendCommand(Msg)) break;
			if (!Sent) Wire = WireText(Msg);
			Sent++;
		}
		if (Sent < Res.Messages.Num())
		{
			NoteOrder(false, FString::Printf(TEXT("%s: %d of %d sent, then %s"), *What, Sent, Res.Messages.Num(), *M->LastSendProblem()));
			return Sent > 0;
		}
		// sent is not accepted: a refusal comes back as a deny, shown in the browser's words
		NoteOrder(true, FString::Printf(TEXT("%s sent%s: '%s' message%s"), *What, Sent > 1 ? *FString::Printf(TEXT(" x%d"), Sent) : TEXT(""), *Wire, Sent > 1 ? TEXT("s") : TEXT("")));
		if (Action == ETCUiAction::Recruit) Feedback(TEXT("recruit")); // client/hud.js buy: ctx.act('blip', 'recruit') after the sends
		return true;
	}
	default:
		return false;
	}
}

void ATCPlayerController::UpdateGroupKeys()
{
	// client/keys.js: Digit1..Digit9 with the exact modifiers of each binding (Ctrl or Command: set; Shift, or Ctrl and
	// Shift: append; Alt: transfer; none: recall)
	const bool bCtrl = IsInputKeyDown(EKeys::LeftControl) || IsInputKeyDown(EKeys::RightControl) || IsInputKeyDown(EKeys::LeftCommand) || IsInputKeyDown(EKeys::RightCommand);
	const bool bShift = IsInputKeyDown(EKeys::LeftShift) || IsInputKeyDown(EKeys::RightShift);
	const bool bAlt = IsInputKeyDown(EKeys::LeftAlt) || IsInputKeyDown(EKeys::RightAlt);
	if (bControlsSheetOpen) return;
	for (int32 N = 1; N <= FTCControlGroups::GroupCount; N++)
		if (WasInputKeyJustPressed(*DigitKeys[N - 1]) && !IntroKeys.Contains(*DigitKeys[N - 1])) GroupChord(N, bCtrl, bShift, bAlt);
}

FTCGroupResult ATCPlayerController::GroupChord(int32 Group, bool bCtrl, bool bShift, bool bAlt)
{
	SkipIntro();
	ATCGameMode* M = Mode();
	if (!M) return FTCGroupResult();
	const FVector2D Before = Focus;
	const ETCGroupOp Op = FTCControlGroups::OpForChord(bCtrl, bShift, bAlt);
	const FTCGroupResult R = Groups.Apply(Group, Op, M->State, M->UnitTable, Selected, FPlatformTime::Seconds());
	// client/main.js group actions: Ctrl+N keeps a copy of the Formation menu, recalling N brings it back
	Formation.OnGroupChord(Op, Group);
	LastGroup = R;
	if (R.bCentre) FocusOn(R.Centre); // client/selection.js: a second recall within 300 ms centres the camera
	if (R.Op != ETCGroupOp::None) NoteOrder(R.bApplied, R.Note);
	TArray<FString> Sizes;
	for (int32 G = 1; G <= FTCControlGroups::GroupCount; G++) if (Groups.Members(G).Num()) Sizes.Add(FString::Printf(TEXT("%d:%d"), G, Groups.Members(G).Num()));
	UE_LOG(LogTC, Log, TEXT("group chord %s%s%s%d -> %s %s: taken %d, members %d, selected %d%s; groups [%s]"), bCtrl ? TEXT("Ctrl+") : TEXT(""), bShift ? TEXT("Shift+") : TEXT(""), bAlt ? TEXT("Alt+") : TEXT(""), Group,
		LexToString(R.Op), R.bApplied ? TEXT("applied") : TEXT("not applied"), R.Taken, R.Members, Selected.Num(),
		R.bCentre ? *FString::Printf(TEXT(", camera centred (%.1f, %.1f) -> (%.1f, %.1f)"), Before.X, Before.Y, R.Centre.X, R.Centre.Y) : TEXT(""), *FString::Join(Sizes, TEXT(" ")));
	return R;
}

FTCMinimapPress ATCPlayerController::MinimapPress(FVector2D Pixel, bool bRight, bool bCtrl, bool bShift)
{
	SkipIntro();
	FTCMinimapPress P;
	ATCGameMode* M = Mode();
	ATCHUD* Hud = Cast<ATCHUD>(GetHUD());
	if (!M || !Hud || !M->State.bHasMatch) return P;
	if (Aim == ETCAim::Rally && Hud->Minimap.Contains(Pixel))
	{
		// client/main.js minimap mousedown with the rally aim: left sets the rally there, right cancels, and the camera
		// does not move. The minimap resolves the press as an armed right press, which gives the clamped ground (and
		// nothing for a square still showing another match) without pointing the camera.
		P = Hud->Minimap.MouseDown(Pixel, ETCMinimapButton::Right, M->State, M->UnitTable, true);
		if (P.Action == ETCMinimapAction::CancelAim)
		{
			if (bRight) CancelAim(TEXT("Aim cancelled"));
			else AimClick(true, P.Ground, bShift);
		}
		UE_LOG(LogTC, Log, TEXT("minimap %s press at pixel (%.0f, %.0f) with the rally aim -> ground (%.1f, %.1f): %s"), bRight ? TEXT("right") : TEXT("left"), Pixel.X, Pixel.Y, P.Ground.X, P.Ground.Y,
			P.Action != ETCMinimapAction::CancelAim ? TEXT("ignored (the image is from another match)") : bRight ? TEXT("rally aim cancelled") : TEXT("rally set there"));
		return P;
	}
	P = Hud->Minimap.MouseDown(Pixel, bRight ? ETCMinimapButton::Right : ETCMinimapButton::Left, M->State, M->UnitTable, Aim != ETCAim::None);
	if (!P.bConsumed) return P;
	const FVector2D Before = Focus;
	switch (P.Action)
	{
	case ETCMinimapAction::CameraTarget: FocusOn(P.Ground); break;
	case ETCMinimapAction::CancelAim: CancelAim(TEXT("Aim cancelled")); break;
	// client/main.js: a right press orders the selection at that ground (selected.size && lastSnap)
	case ETCMinimapAction::Order: if (Selected.Num()) IssueOrder(P.Cursor, bCtrl, bShift); break;
	default: break;
	}
	UE_LOG(LogTC, Log, TEXT("minimap %s press at pixel (%.0f, %.0f) -> ground (%.1f, %.1f): %s%s"), bRight ? TEXT("right") : TEXT("left"), Pixel.X, Pixel.Y, P.Ground.X, P.Ground.Y,
		P.Action == ETCMinimapAction::CameraTarget ? *FString::Printf(TEXT("camera (%.1f, %.1f) -> (%.1f, %.1f)"), Before.X, Before.Y, Focus.X, Focus.Y)
		: P.Action == ETCMinimapAction::Order ? *FString::Printf(TEXT("order for %d selected (enemy %d, friend %d, building %d under it)"), Selected.Num(), P.Cursor.Enemy, P.Cursor.Friend, P.Cursor.Building)
		: P.Action == ETCMinimapAction::CancelAim ? TEXT("aim disarmed") : TEXT("ignored (the image is from another match)"),
		bRight && bCtrl ? TEXT(" (Ctrl: attack-move)") : TEXT(""));
	return P;
}

bool ATCPlayerController::InjectKey(const FKey& Key, bool bPressed)
{
	// the engine's own simulated-input event (bIsSimulatedInput), through the same InputKey the viewport calls. No input
	// device: APlayerController::InputKey filters by device only with bFilterInputByPlatformUser, off by default and
	// not set by this project.
	// a verification key must be acted on: the intro a real key would end first is ended here
	if (bPressed) SkipIntro();
	const FInputKeyEventArgs Args = FInputKeyEventArgs::CreateSimulated(Key, bPressed ? IE_Pressed : IE_Released, bPressed ? 1.f : 0.f);
	return InputKey(Args);
}
