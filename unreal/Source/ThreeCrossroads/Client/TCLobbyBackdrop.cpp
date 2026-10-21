// The lobby's backdrop (client/lobby-view.js). See Client/TCLobbyBackdrop.h.
#include "Client/TCLobbyBackdrop.h"
#include "ThreeCrossroads.h"
#include "Client/TCCameraPawn.h"
#include "Net/TCMatchState.h"
#include "World/TCTerrain.h"
#include "World/TCWorldObjects.h"
#include "World/TCGroundPaint.h"
#include "World/TCWater.h"
#include "Client/TCGraphics.h"
#include "Camera/CameraComponent.h"
#include "Engine/World.h"
#include "Engine/Canvas.h"
#include "CanvasItem.h"
#include "HAL/IConsoleManager.h"
#include "HAL/PlatformTime.h"
#include "HttpModule.h"
#include "Interfaces/IHttpRequest.h"
#include "Interfaces/IHttpResponse.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Misc/AutomationTest.h"
#include "Misc/Paths.h"
#include "UnrealClient.h"

namespace
{
	// index.html #overlay background
	const FColor Overlay(0x16, 0x19, 0x1b);
	// client/main.js: only map names the server accepts (server.js MAP_NAME) are asked for
	bool SafeMapName(const FString& N)
	{
		if (N.IsEmpty() || N.Len() > 64) return false;
		for (const TCHAR C : N) if (!FChar::IsAlnum(C) && C != '-' && C != '_') return false;
		return true;
	}
}

FTCLobbyBackdrop::FTCLobbyBackdrop() : Alive(MakeShared<bool>(true)) {}

FTCLobbyBackdrop::~FTCLobbyBackdrop()
{
	*Alive = false;
	Drop();
}

double FTCLobbyBackdrop::EaseOut(double X)
{
	// cubic-bezier(0, 0, 0.58, 1): solve x(s) = X for s by bisection, return y(s)
	X = FMath::Clamp(X, 0.0, 1.0);
	auto Bez = [](double S, double P1, double P2) { const double U = 1 - S; return 3 * U * U * S * P1 + 3 * U * S * S * P2 + S * S * S; };
	double Lo = 0, Hi = 1;
	for (int32 I = 0; I < 40; I++) { const double M = (Lo + Hi) / 2; if (Bez(M, 0.0, 0.58) < X) Lo = M; else Hi = M; }
	return Bez((Lo + Hi) / 2, 0.0, 1.0);
}

void FTCLobbyBackdrop::DriftPose(double W, double H, double T, TFunctionRef<float(float, float)> Ground, FVector& OutEye, FVector& OutAim)
{
	// lobby-view.js place(t)
	const double X = W / 2 + FMath::Sin(T / PeriodX * PI * 2) * W * Reach;
	const double Z = H / 2 + FMath::Sin(T / PeriodZ * PI * 2 + 1.3) * H * Reach;
	const double Y = Ground((float)X, (float)Z), D = FMath::Max(W, H) * DistanceShare;
	OutAim = FVector(X, Z, Y);
	OutEye = FVector(X - D * 0.25, Z + D * 0.59, Y + D * 0.77);
}

FString FTCLobbyBackdrop::HttpBase() const
{
	// ws://host:port/ws -> http://host:port (wss -> https)
	FString U = ServerUrl;
	FString Scheme = TEXT("http://");
	if (U.RemoveFromStart(TEXT("wss://"))) Scheme = TEXT("https://");
	else U.RemoveFromStart(TEXT("ws://"));
	int32 Slash;
	if (U.FindChar('/', Slash)) U.LeftInline(Slash);
	return U.IsEmpty() ? FString() : Scheme + U;
}

FTCLobbyLook FTCLobbyLook::For(double W, double H)
{
	FTCLobbyLook L;
	auto Hex = [](uint32 C) { return FLinearColor(FColor((C >> 16) & 255, (C >> 8) & 255, C & 255)); };
	L.Sun = Hex(SunHex); L.Sky = Hex(SkyHex); L.Ground = Hex(GroundHex); L.Fog = Hex(FogHex);
	const double Side = FMath::Max(W, H);
	L.FogNear = (float)(Side * FogNearShare); L.FogFar = (float)(Side * FogFarShare);
	// lobby-view.js: sun.position (W/2 - 70, 110, H/2 - 50), sun.target (W/2, 0, H/2)
	L.SunDirThree = FVector(-70.0, 110.0, -50.0).GetSafeNormal();
	return L;
}

bool FTCLobbyBackdrop::Look(FTCLobbyLook& Out) const
{
	if (!bActive || !bBuilt || !Terrain.IsValid() || !State.IsValid()) return false;
	// lobby-view.js build(): the fog and the sun are set once the world is added to the scene
	Out = FTCLobbyLook::For(State->Map.W * FTCMatchState::CellMetres, State->Map.H * FTCMatchState::CellMetres);
	return true;
}

void FTCLobbyBackdrop::Show(UWorld* InWorld, const FString& InServerUrl, const FString& MapName, const FString& Mode)
{
	// client/main.js previewMap: World Conquest shows no battlefield (lobbyView.hide())
	if (bGraphicsLow || Mode == TEXT("world") || MapName == TEXT("world") || !SafeMapName(MapName)) { if (bActive) Hide(); return; }
	if (Shown == MapName && bActive) return;
	Hide();
	World = InWorld; ServerUrl = InServerUrl; Shown = MapName; bActive = true; ShowAt = FPlatformTime::Seconds();
	// half resolution while it shows (lobby-view.js SCALE: soft, and a quarter of the pixels)
	if (IConsoleVariable* Cv = IConsoleManager::Get().FindConsoleVariable(TEXT("r.ScreenPercentage")))
	{
		if (SavedScreenPercentage < 0) SavedScreenPercentage = Cv->GetFloat();
		Cv->Set(ScreenPercentage, ECVF_SetByCode);
	}
	if (const TSharedPtr<FJsonObject>* Hit = Cache.Find(MapName)) { Pending = *Hit; PendingName = MapName; }
	else Request(MapName);
}

void FTCLobbyBackdrop::Request(const FString& Name)
{
	const FString Base = HttpBase();
	if (Base.IsEmpty()) { Problem = TEXT("no server address"); return; }
	const FString Url = FString::Printf(TEXT("%s/maps/%s"), *Base, *Name);
	TSharedRef<IHttpRequest, ESPMode::ThreadSafe> Req = FHttpModule::Get().CreateRequest();
	Req->SetURL(Url);
	Req->SetVerb(TEXT("GET"));
	const int32 MyTicket = ++Ticket;
	TWeakPtr<bool> WeakAlive = Alive;
	Req->OnProcessRequestComplete().BindLambda([this, WeakAlive, MyTicket, Name, Url](FHttpRequestPtr, FHttpResponsePtr Resp, bool bOk)
	{
		const TSharedPtr<bool> A = WeakAlive.Pin();
		if (!A || !*A) return;
		if (!bOk || !Resp.IsValid() || Resp->GetResponseCode() != 200)
		{
			Problem = FString::Printf(TEXT("GET %s failed (%d)"), *Url, Resp.IsValid() ? Resp->GetResponseCode() : 0);
			UE_LOG(LogTC, Warning, TEXT("lobby backdrop: %s"), *Problem);
			return;
		}
		TSharedPtr<FJsonObject> Map;
		if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Resp->GetContentAsString()), Map) || !Map.IsValid())
		{
			Problem = FString::Printf(TEXT("GET %s: not a map"), *Url);
			return;
		}
		Cache.Add(Name, Map);
		// the host picked another map (or a match started) meanwhile: kept in the cache only
		if (MyTicket != Ticket || Shown != Name || !bActive) return;
		Pending = Map; PendingName = Name;
		UE_LOG(LogTC, Log, TEXT("lobby backdrop: map %s fetched from %s (%d bytes)"), *Name, *Url, Resp->GetContentLength());
	});
	Req->ProcessRequest();
}

void FTCLobbyBackdrop::Build(const FString& Name, const TSharedPtr<FJsonObject>& Map)
{
	// lobby-view.js builds the world in steps, one per idle moment, so the form keeps answering: here one step a frame
	// (the start's decode and the houses; the ground paint; the relief; the water), each timed in the log
	UWorld* W = World.Get();
	if (!W || !Map.IsValid()) return;
	const double T0 = FPlatformTime::Seconds();
	if (BuildStep == 0)
	{
		// the map as a start with no fog mask: an authored map is public (setFogMap(null): no fog over the houses)
		StartMsg = MakeShared<FJsonObject>();
		StartMsg->SetStringField(TEXT("t"), TEXT("start"));
		StartMsg->SetNumberField(TEXT("matchId"), -1000 - Ticket);
		StartMsg->SetObjectField(TEXT("map"), Map);
		StartMsg->SetNumberField(TEXT("you"), 0);
		StartMsg->SetArrayField(TEXT("names"), {});
		StartMsg->SetArrayField(TEXT("teams"), {});
		State = MakeUnique<FTCMatchState>();
		if (!State->ApplyStart(*StartMsg) || State->Map.bWorld) { Problem = FString::Printf(TEXT("map %s could not be read as a start"), *Name); State.Reset(); Pending.Reset(); return; }
		FActorSpawnParameters P;
		P.ObjectFlags |= RF_Transient;
		ATCTerrain* T = W->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity, P);
		if (!T) { Problem = TEXT("the terrain actor could not be spawned"); Pending.Reset(); return; }
		Terrain = T;
		Objects = MakeUnique<FTCWorldObjects>();
		View = MakeUnique<FTCWorldObjectsView>();
		Paint = MakeUnique<FTCGroundPaint>();
		Water = MakeUnique<FTCWater>();
		View->Attach(W);
		Paint->Attach(W);
		Water->Attach(W);
		TWeakObjectPtr<ATCTerrain> WeakT = T;
		Water->GroundHeight = [WeakT](float X, float Z) { return WeakT.IsValid() ? WeakT->HeightAt(X, Z) : 0.f; };
		// the match's order (ATCGameMode start): objects, the cell source, the paint, the relief, the paint bound, water
		Objects->ApplyStart(*StartMsg, *State);
		T->SetCellSource(Objects.Get());
		T->SetPaintSource(Paint.Get());
	}
	else if (BuildStep == 1) Paint->ApplyStart(*StartMsg, *State);
	else if (BuildStep == 2) { Terrain->ResetFrom(*State); State->bMapReset = false; Paint->Bind(Terrain->GroundMaterialInstance()); }
	else if (BuildStep == 3) Water->ResetFrom(*State);
	StepMs[BuildStep] = (FPlatformTime::Seconds() - T0) * 1000.0;
	if (++BuildStep < 4) return;
	Pending.Reset(); StartMsg.Reset();
	bBuilt = true; BuiltAt = FPlatformTime::Seconds(); ReadyAt = -1; Frames = 0;
	// start mid-drift
	Start = BuiltAt - StartSeconds;
	BuildMs = StepMs[0] + StepMs[1] + StepMs[2] + StepMs[3];
	UE_LOG(LogTC, Log, TEXT("lobby backdrop: %s (%d x %d cells) set up in four frames: start and houses %.1f ms, ground paint %.1f ms, relief %.1f ms, water %.1f ms; its pieces go up over the next frames"),
		*Name, State->Map.W, State->Map.H, StepMs[0], StepMs[1], StepMs[2], StepMs[3]);
}

void FTCLobbyBackdrop::Tick(float DeltaSeconds)
{
	if (!bActive) return;
	if (Pending.IsValid() && !bBuilt)
	{
		// lobby-view.js waits 250 ms after show() so the form paints first
		if (FPlatformTime::Seconds() - ShowAt >= 0.25) { TSharedPtr<FJsonObject> Map = Pending; Build(PendingName, Map); }
	}
	if (!bBuilt || !Terrain.IsValid()) return;
	ATCTerrain& T = *Terrain;
	T.Flush(*State);
	Paint->Tick(*State, DeltaSeconds, false, [&T](float X, float Z) { return T.HeightAt(X, Z); });
	Water->Flush(*State, DriftSeconds(), false);
	Objects->Update(T, nullptr);
	View->Draw(*Objects, *State, &T);
	Frames++;
	// up once every chunk and piece is built (lobby-view.js: the canvas turns 'on' with the first frame of the built world)
	if (ReadyAt < 0 && T.ChunksQueued() == 0 && T.ChunksMeshed() > 0 && View->Queued() == 0 && Frames > 2)
	{
		ReadyAt = FPlatformTime::Seconds();
		UE_LOG(LogTC, Log, TEXT("lobby backdrop: %s up %.2f s after it was set up (%d frames): %d chunks, %d houses, %d props, water %d triangles"),
			*Shown, ReadyAt - BuiltAt, Frames, T.ChunksMeshed(), Objects->HouseCount(), Objects->PropsShown(), Water->TrianglesDrawn());
	}
	if (!bShotDirRead) { bShotDirRead = true; FParse::Value(FCommandLine::Get(), TEXT("TCBackdropShotDir="), ShotDir); }
	static const double ShotTimes[] = { 0.4, 3.0, 30.0 };
	if (!ShotDir.IsEmpty() && ReadyAt >= 0 && ShotsTaken < 3 && FPlatformTime::Seconds() - ReadyAt >= ShotTimes[ShotsTaken])
	{
		const FString File = FPaths::Combine(ShotDir, FString::Printf(TEXT("warmap-lobby-%s-%04.1fs.png"), *Shown, ShotTimes[ShotsTaken]));
		ShotsTaken++;
		FScreenshotRequest::RequestScreenshot(File, true, false);
		UE_LOG(LogTC, Log, TEXT("lobby backdrop shot: %s | %s"), *File, *StatusLine());
	}
}

double FTCLobbyBackdrop::DriftSeconds() const
{
	// round 16: lobby-view.js still mode (gl.still: reduced motion, or a software renderer such as the headless
	// browser's SwiftShader) draws one frame where the drift would be 20 s in; -TCBackdropStill asks the same here, so a
	// capture lines up with the browser's still frame
	static const bool bStill = FParse::Param(FCommandLine::Get(), TEXT("TCBackdropStill"));
	return bStill ? StartSeconds : FPlatformTime::Seconds() - Start;
}

bool FTCLobbyBackdrop::PlaceCamera(ATCCameraPawn* Cam) const
{
	if (!Cam || !bBuilt || !Terrain.IsValid() || !State) return false;
	const double W = State->Map.W * FTCMatchState::CellMetres, H = State->Map.H * FTCMatchState::CellMetres;
	FVector Eye, Aim;
	const ATCTerrain* T = Terrain.Get();
	DriftPose(W, H, DriftSeconds(), [T](float X, float Z) { return T->HeightAt(X, Z); }, Eye, Aim);
	const FVector E = ATCTerrain::ToWorld(Eye.X, Eye.Y, Eye.Z), A = ATCTerrain::ToWorld(Aim.X, Aim.Y, Aim.Z);
	Cam->SetActorLocationAndRotation(E, (A - E).Rotation());
	if (Cam->Camera)
	{
		const float Hfov = FMath::RadiansToDegrees(2.f * FMath::Atan(FMath::Tan(FMath::DegreesToRadians(VerticalFovDegrees / 2.f)) * Cam->Camera->AspectRatio));
		if (!FMath::IsNearlyEqual(Cam->Camera->FieldOfView, Hfov, 1e-3f)) Cam->Camera->SetFieldOfView(Hfov);
	}
	return true;
}

void FTCLobbyBackdrop::RestoreCamera(ATCCameraPawn* Cam)
{
	if (!Cam || !Cam->Camera) return;
	const float Hfov = FMath::RadiansToDegrees(2.f * FMath::Atan(FMath::Tan(FMath::DegreesToRadians(ATCCameraPawn::VerticalFovDegrees / 2.f)) * Cam->Camera->AspectRatio));
	if (!FMath::IsNearlyEqual(Cam->Camera->FieldOfView, Hfov, 1e-3f)) Cam->Camera->SetFieldOfView(Hfov);
}

float FTCLobbyBackdrop::ShownOpacity() const
{
	if (!bActive || ReadyAt < 0) return 0.f;
	return Opacity * (float)EaseOut((FPlatformTime::Seconds() - ReadyAt) / FadeInSeconds);
}

void FTCLobbyBackdrop::DrawDim(UCanvas* Canvas) const
{
	if (!Canvas) return;
	FLinearColor C(Overlay);
	C.A = 1.f - ShownOpacity();
	FCanvasTileItem Tile(FVector2D::ZeroVector, FVector2D(Canvas->ClipX, Canvas->ClipY), C);
	Tile.BlendMode = SE_BLEND_Translucent;
	Canvas->DrawItem(Tile);
}

void FTCLobbyBackdrop::Drop()
{
	if (View) View->Detach();
	if (Paint) Paint->Detach();
	if (Water) Water->Detach();
	if (Terrain.IsValid()) Terrain->Destroy();
	View.Reset(); Paint.Reset(); Water.Reset(); Objects.Reset(); State.Reset();
	Terrain.Reset();
	bBuilt = false; ReadyAt = -1; Frames = 0; ShotsTaken = 0; BuildStep = 0; StartMsg.Reset();
}

void FTCLobbyBackdrop::Hide()
{
	Ticket++;
	Pending.Reset();
	Drop();
	if (bActive) UE_LOG(LogTC, Log, TEXT("lobby backdrop: %s hidden"), *Shown);
	bActive = false; Shown.Reset(); Problem.Reset();
	if (SavedScreenPercentage >= 0)
	{
		// round 15 (gfxlow glue): back to the Graphics level's percentage (perf.js renderScale), not the one saved when the
		// backdrop showed: a change of level while it showed (the lobby's menu, or the automatic drop) would be undone
		if (IConsoleVariable* Cv = IConsoleManager::Get().FindConsoleVariable(TEXT("r.ScreenPercentage"))) Cv->Set((float)FTCGraphics::Get().Budget().ScreenPercentage(1.0), ECVF_SetByCode);
		SavedScreenPercentage = -1.f;
	}
}

FString FTCLobbyBackdrop::StatusLine() const
{
	if (!bActive) return TEXT("lobby backdrop: off");
	return FString::Printf(TEXT("lobby backdrop: %s, %s, opacity %.2f, chunks %d queued %d, houses %d, props %d%s"), *Shown,
		ReadyAt >= 0 ? TEXT("up") : bBuilt ? TEXT("building") : Pending.IsValid() ? TEXT("map arrived") : TEXT("fetching"), ShownOpacity(),
		Terrain.IsValid() ? Terrain->ChunksMeshed() : 0, Terrain.IsValid() ? Terrain->ChunksQueued() : 0, Objects ? Objects->HouseCount() : 0, Objects ? Objects->PropsShown() : 0,
		Problem.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *Problem));
}

#if WITH_DEV_AUTOMATION_TESTS

// ThreeCrossroads.LobbyBackdrop.Drift: client/lobby-view.js place(t) and index.html's fade, ported.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLobbyBackdropDriftTest, "ThreeCrossroads.LobbyBackdrop.Drift", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCLobbyBackdropDriftTest::RunTest(const FString& Parameters)
{
	FVector Eye, Aim;
	// Seelow Heights, 220 x 240 m, flat: at t = 0 the aim is W/2 and H/2 + sin(1.3) x 0.14 H
	FTCLobbyBackdrop::DriftPose(220, 240, 0, [](float, float) { return 0.f; }, Eye, Aim);
	TestEqual(TEXT("aim x at t 0"), Aim.X, 110.0, 1e-9);
	TestEqual(TEXT("aim z at t 0"), Aim.Y, 120 + FMath::Sin(1.3) * 240 * 0.14, 1e-9);
	const double D = 240 * 0.36;
	TestEqual(TEXT("eye west of the aim by 0.25 d"), Eye.X, Aim.X - 0.25 * D, 1e-9);
	TestEqual(TEXT("eye south of the aim by 0.59 d"), Eye.Y, Aim.Y + 0.59 * D, 1e-9);
	TestEqual(TEXT("eye up 0.77 d"), Eye.Z, 0.77 * D, 1e-9);
	const double Pitch = FMath::RadiansToDegrees(FMath::Atan2(0.77, FMath::Sqrt(0.25 * 0.25 + 0.59 * 0.59)));
	TestTrue(TEXT("looking down at about 50 degrees"), Pitch > 49.5 && Pitch < 50.5);
	// a quarter of the east-west period later the aim is at its east reach
	// round 15: lobby-view.js's own light and fog, not the match's mood
	{
		const FTCLobbyLook L = FTCLobbyLook::For(220, 240);
		TestEqual(TEXT("fog near: 0.3 x the larger side"), L.FogNear, 72.f, 1e-4f);
		TestEqual(TEXT("fog far: 0.9 x the larger side"), L.FogFar, 216.f, 1e-4f);
		TestFalse(TEXT("no shadows"), L.bShadows);
		TestFalse(TEXT("the fog mixes after the tone curve (no composer)"), L.bHazeBeforeTone);
		TestEqual(TEXT("sun 3.0"), L.SunI, 3.f);
		TestEqual(TEXT("hemisphere 0.9"), L.HemiI, 0.9f);
		TestEqual(TEXT("no environment"), L.Env, 0.f);
		TestTrue(TEXT("fog colour #1a1d1f (linear)"), L.Fog.Equals(FLinearColor(FColor(0x1a, 0x1d, 0x1f)), 1e-6f));
		TestTrue(TEXT("the sun is up, west and north of the aim"), L.SunDirThree.Y > 0.7 && L.SunDirThree.X < 0 && L.SunDirThree.Z < 0);
		TestEqual(TEXT("sun direction is a unit vector"), L.SunDirThree.Size(), 1.0, 1e-9);
	}
	FTCLobbyBackdrop::DriftPose(220, 240, 35, [](float, float) { return 2.5f; }, Eye, Aim);
	TestEqual(TEXT("east reach"), Aim.X, 110 + 220 * 0.14, 1e-9);
	TestEqual(TEXT("aim on the ground"), Aim.Z, 2.5, 1e-9);
	// index.html: transition opacity 1.6 s ease-out
	TestEqual(TEXT("ease-out start"), FTCLobbyBackdrop::EaseOut(0), 0.0, 1e-9);
	TestEqual(TEXT("ease-out end"), FTCLobbyBackdrop::EaseOut(1), 1.0, 1e-6);
	TestTrue(TEXT("ease-out is ahead of linear halfway"), FTCLobbyBackdrop::EaseOut(0.5) > 0.6);
	return true;
}

#endif
