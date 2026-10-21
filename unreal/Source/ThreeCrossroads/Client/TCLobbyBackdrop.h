// The lobby's backdrop (client/lobby-view.js, CLI-18): the selected map's real battlefield (painted ground, relief,
// houses, hedgerows, trees and water, built by the same modules as a match: ATCTerrain, FTCGroundPaint, FTCWorldObjects
// with its props and foliage, FTCWater) seen from above while the camera drifts slowly across it, behind the lobby,
// dimmed to half (index.html: the canvas fades to opacity 0.5 over 1.6 s, ease-out, over the overlay's #16191b), rendered
// at half resolution (lobby-view.js SCALE 0.5: r.ScreenPercentage 50 while it shows).
//
// The map comes from the server as the browser fetches it: GET /maps/<name> on the room's server (client/main.js
// previewMap; the public projection server.js mapForClient serves, the same as a start's map), cached per name. It is fed
// to the match modules as a start with no fog mask (an authored map is public: nothing here is a recipient's secret), on
// instances of their own, so the match's ground is never touched. World Conquest shows no backdrop (main.js hides it:
// the continent is unknown until played), and a match start hides it and frees everything (lobby-view.js hide()).
//
// The camera (lobby-view.js place): high over the map (0.36 x the larger side away), looking down at about 50 degrees
// (offset -0.25, +0.77, +0.59 of that distance in x, up, z), vertical field of view 40 degrees, its aim gliding along
// x = W/2 + sin(t / 140 s) x 0.14 W, z = H/2 + sin(t / 95 s + 1.3) x 0.14 H, never turning; it starts 20 s into the drift.
#pragma once

#include "CoreMinimal.h"

class UWorld;
class UCanvas;
class ATCTerrain;
class ATCCameraPawn;
class FTCMatchState;
class FTCWorldObjects;
class FTCWorldObjectsView;
class FTCGroundPaint;
class FTCWater;
class FJsonObject;

// Round 15 (hudfit): the backdrop's own light, client/lobby-view.js context() and build(): a HemisphereLight (sky
// #c9d6e4, ground #5a4a32, 0.9) and a DirectionalLight (#ffd6a8, 3.0) without shadows, placed at (W/2 - 70, 110,
// H/2 - 50) and aimed at (W/2, 0, H/2); no scene.environment; NeutralToneMapping at exposure 1 drawn straight to the
// canvas (no composer, so the fog mixes after tone mapping); THREE.Fog(#1a1d1f, 0.3 x the larger side, 0.9 x the
// larger side): the far side fades into the dark. Colours are linear (three's Color.setHex from the sRGB hex). The
// lighting actor takes it while the battlefield shows (ATCLighting::SetLobbyLook, wired by the game mode).
struct THREECROSSROADS_API FTCLobbyLook
{
	static constexpr uint32 SunHex = 0xffd6a8, SkyHex = 0xc9d6e4, GroundHex = 0x5a4a32, FogHex = 0x1a1d1f;
	static constexpr float SunIntensity = 3.f, HemiIntensity = 0.9f, FogNearShare = 0.3f, FogFarShare = 0.9f, Exposure = 1.f;
	FLinearColor Sun, Sky, Ground, Fog;
	float SunI = SunIntensity, HemiI = HemiIntensity, Env = 0.f;
	float FogNear = 0.f, FogFar = 0.f;     // metres from the camera (three's view depth)
	FVector SunDirThree = FVector::ZeroVector; // toward the sun, three's axes (x, y up, z), unit length
	bool bShadows = false;
	bool bHazeBeforeTone = false;            // lobby-view.js draws straight to its canvas
	// three.NeutralToneMapping (the match draws ACES filmic). Round 16 (hudfit): ATCLighting::SetLobbyLook passes it to
	// M_TCLookPost's Neutral (lead patch lobby-neutral-tonemapping.patch); with ACES the backdrop had more contrast than
	// the browser's (darker darks and foliage at the same pose)
	bool bNeutralToneMapping = true;
	// the look for a W x H metre map
	static FTCLobbyLook For(double W, double H);
};

class THREECROSSROADS_API FTCLobbyBackdrop
{
public:
	// client/lobby-view.js PERIOD, REACH, SCALE; the camera's distance as a share of the larger side; its offsets
	static constexpr double PeriodX = 140.0, PeriodZ = 95.0, Reach = 0.14, DistanceShare = 0.36, StartSeconds = 20.0;
	static constexpr float VerticalFovDegrees = 40.f, Opacity = 0.5f, FadeInSeconds = 1.6f, ScreenPercentage = 50.f;

	FTCLobbyBackdrop();
	~FTCLobbyBackdrop();

	// client/lobby-view.js show(name, map): the lobby shows this map (called for every lobby: the same map is a no-op).
	// ServerUrl is the room's ws:// URL; the map is fetched from its HTTP side. Mode 'world' hides it.
	void Show(UWorld* World, const FString& ServerUrl, const FString& MapName, const FString& Mode);
	// lobby-view.js show(): "Nothing runs on Graphics Low" (gfx.low hides it). The owner sets this from the Graphics
	// setting (Client/TCGraphics.h, ETCGraphicsLevel::Low); Show then hides instead.
	bool bGraphicsLow = false;
	// lobby-view.js hide(): a match starts or the lobby goes; everything built is freed
	void Hide();
	// Every frame in the lobby: builds what arrived, steps the drift and the modules
	void Tick(float DeltaSeconds);
	// Something is (being) shown: the controller asks PlaceCamera instead of its own camera
	bool IsActive() const { return bActive; }
	// Puts the camera where the drift is now (and its 40 degree view); false when nothing is built yet
	bool PlaceCamera(ATCCameraPawn* Cam) const;
	// Gives the camera its match view back (42 degrees vertical) and the screen percentage
	void RestoreCamera(ATCCameraPawn* Cam);
	// The lobby's background over the battlefield: #16191b at 1 - 0.5 x the fade-in (opaque while nothing is built)
	void DrawDim(UCanvas* Canvas) const;
	// 0 until the battlefield is up, then the canvas's CSS opacity transition (0 to 0.5 over 1.6 s, ease-out)
	float ShownOpacity() const;
	FString StatusLine() const;
	// Round 15: the backdrop's light (FTCLobbyLook) while its battlefield is built, else false: the lighting actor then
	// keeps (or goes back to) the match's mood
	bool Look(FTCLobbyLook& Out) const;

	// The drift's camera for a map of W x H metres at t seconds into it, with the ground height at the aim (reference
	// metres; X = x, Y = z, Z up). For tests and checks.
	static void DriftPose(double W, double H, double T, TFunctionRef<float(float, float)> Ground, FVector& OutEye, FVector& OutAim);
	// CSS ease-out (cubic-bezier(0, 0, 0.58, 1)) at progress X in [0, 1]
	static double EaseOut(double X);

private:
	TWeakObjectPtr<UWorld> World;
	bool bActive = false, bBuilt = false, bReady = false;
	FString Shown, ServerUrl, Problem;
	int32 Ticket = 0;
	TSharedPtr<bool> Alive;
	TMap<FString, TSharedPtr<FJsonObject>> Cache;
	TSharedPtr<FJsonObject> Pending;
	FString PendingName;
	double ShowAt = 0, BuiltAt = 0, ReadyAt = -1, Start = 0, BuildMs = 0;
	float SavedScreenPercentage = -1.f;
	TUniquePtr<FTCMatchState> State;
	TUniquePtr<FTCWorldObjects> Objects;
	TUniquePtr<FTCWorldObjectsView> View;
	TUniquePtr<FTCGroundPaint> Paint;
	TUniquePtr<FTCWater> Water;
	TWeakObjectPtr<ATCTerrain> Terrain;
	int32 Frames = 0;
	int32 BuildStep = 0;
	double StepMs[4] = { 0, 0, 0, 0 };
	TSharedPtr<FJsonObject> StartMsg;
	// -TCBackdropShotDir=<dir>: screenshots with the HUD 0.4 s, 3 s and 30 s after the battlefield is up
	FString ShotDir;
	bool bShotDirRead = false;
	int32 ShotsTaken = 0;

	// seconds into the drift now (StartSeconds held under -TCBackdropStill, lobby-view.js's still mode)
	double DriftSeconds() const;
	void Request(const FString& Name);
	void Build(const FString& Name, const TSharedPtr<FJsonObject>& Map);
	void Drop();
	FString HttpBase() const;
};
