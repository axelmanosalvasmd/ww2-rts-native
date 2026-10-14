// The battlefield's light and color, as the reference draws them: client/moods.js (the mood table and the weather
// looks), client/light.js (sun, hemisphere sky fill, the outdoor environment, distance haze, tone mapping) and the
// part of client/atmosphere.js that eases the weather into that light. three.js r186 is reproduced as the reference
// uses it: linear working color, ACES filmic tone mapping at the mood's exposure, sRGB output and the haze as
// background (unreal/Config/TCPostProcess.md, unreal/Tools/look/README.md). Where the haze mixes depends on the
// browser's graphics level (round 14): on High, light.js draws through an EffectComposer (the GTAO pass) into a linear
// half-float target, where three r186 gives the materials no tone mapping and a linear output (WebGLPrograms), so
// fog_fragment mixes the linear haze into linear light and OutputPass tone-maps the result, the haze included; on Low
// (and ?ao=0) the scene draws straight to the canvas and the haze mixes after tone mapping, in output space. The game
// draws High's way; -TCHazeAfterTone draws Low's.
//
// How each three light maps here:
//   sun (DirectionalLight)        a movable directional light, the mood's color and intensity in lux (three's
//                                 physically based lights and the engine's both give a white Lambert surface E/pi),
//                                 its shadow darkness as ShadowAmount; aimed once per match (light.js aimSun)
//   HemisphereLight + environment one sky light whose cubemap is built here: the outdoor environment times its
//                                 intensity plus the linear-in-height radiance whose diffuse irradiance is exactly
//                                 the hemisphere light's (three r186 gives both to Lambert and Standard materials)
//   model materials (M_TCModel*)  take no engine sky light (their Ambient Occlusion is 0); their 'TCLookIndirect'
//                                 node adds three's own hemisphere, environment diffuse and environment reflection
//                                 from MPC_TCLook and three's PMREM atlas (unreal/Tools/look/make-look-assets.py)
//   tone mapping, haze, sky       M_TCLookPost replaces the engine tonemapper; exposure is locked at 1 and bloom,
//                                 vignette, grain, fringe, flare, motion blur, GI and screen-space reflections are off
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "Commandlets/Commandlet.h"
#include "Engine/Scene.h"
#include "Client/TCLobbyBackdrop.h"
#include "TCLighting.generated.h"

class UDirectionalLightComponent;
class USkyLightComponent;
class UPostProcessComponent;
class UMaterialInstanceDynamic;
class UMaterialInterface;
class UMaterialParameterCollection;
class UTextureCube;

// One row of client/moods.js MOODS; colors are linear (three's Color.setHex from the sRGB hex).
struct THREECROSSROADS_API FTCMood
{
	FName Name;
	float SunUp = 42.f;              // degrees
	uint32 SunHex = 0xffffff, TopHex = 0xffffff, BottomHex = 0xffffff, HazeHex = 0xffffff;
	FLinearColor Sun, Top, Bottom, Haze;
	float SunI = 3.2f, HemiI = 1.25f, HazeK = 1.f, Shadow = 0.95f, Soft = 1.f, Exposure = 1.f, Clouds = 0.f;
	bool bMist = false;
	FName Weather;                   // 'snow' or 'dust' falling (NAME_None: clear)
};

// One row of client/moods.js WEATHER_LOOK (and atmosphere.js's eased 'cur'/'goal').
struct THREECROSSROADS_API FTCWeatherLook
{
	float Haze = 1.f, Sun = 1.f, Shadow = 1.f, Clouds = 1.f, Wet = 0.f, Sheen = 0.f, Mud = 0.f, Cover = 0.f, Banks = 0.f;
	FLinearColor HazeTo = FLinearColor::White;
	float HazeMix = 0.f;
};

namespace TCLook
{
	constexpr const TCHAR* ReferencePin = TEXT("e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
	// the assets unreal/Tools/look/make-look-assets.py makes
	constexpr const TCHAR* PostPath = TEXT("/Game/Models/Materials/Look/M_TCLookPost.M_TCLookPost");
	constexpr const TCHAR* CollectionPath = TEXT("/Game/Models/Materials/Look/MPC_TCLook.MPC_TCLook");
	constexpr const TCHAR* LambertPath = TEXT("/Game/Models/Materials/Look/M_TCLookLambert.M_TCLookLambert");
	constexpr const TCHAR* GroundLambertPath = TEXT("/Game/Models/Materials/Look/M_TCGroundLambert.M_TCGroundLambert");
	constexpr const TCHAR* EmissivePath = TEXT("/Game/Models/Materials/Look/M_TCLookEmissive.M_TCLookEmissive");
	constexpr const TCHAR* LinePath = TEXT("/Game/Models/Materials/Look/M_TCLookLine.M_TCLookLine");

	// three's Color.setHex (sRGB bytes to linear) and getHex
	FLinearColor HexLinear(uint32 Hex);
	uint32 LinearHex(const FLinearColor& C);
	// three r186 sRGBTransferOETF, per channel, no tone mapping
	FLinearColor ToDisplay(const FLinearColor& Linear);
	// three r186 ACESFilmicToneMapping (exposure / 0.6, the RRT and ODT fit, clamped), linear in and out
	FLinearColor ACESFilmic(const FLinearColor& Linear, float Exposure);
	// ACESFilmic then the sRGB OETF: what three writes to the canvas, 0-1
	FLinearColor ToneMapped(const FLinearColor& Linear, float Exposure);

	const TArray<FTCMood>& Moods();
	const FTCMood& DefaultMood(); // 'day'
	const FTCMood* FindMood(FName Name);
	// shared/weather.js mapMood: the map's own mood, else by its name and key; 'warm' when no rule matches
	FName MapMood(const FString& MapName, const FString& MapKey);
	// client/moods.js moodFor: an explicit mood, snow weather, rain or mud (overcast), the map's mood, else 'day'
	FName MoodFor(const FString& MapName, const FString& MapKey, const FString& WeatherKind, FName Override = NAME_None);
	FTCWeatherLook WeatherLook(const FString& Kind);
	// atmosphere.js blend(): what the look eases toward for a weather and a shower (rain, wet 0-1), and its haze
	void WeatherGoal(const FTCMood& M, const FString& Kind, float Rain, float Wet, FTCWeatherLook& OutGoal, FLinearColor& OutHaze);
	// atmosphere.js applyMood: the row light.js setMood receives (haze through getHex, as three passes it)
	FTCMood Applied(const FTCMood& M, const FTCWeatherLook& Cur, const FLinearColor& HazeNow);
	// light.js setMood: scene.environmentIntensity
	float EnvironmentIntensity(float SunI);
	// light.js aimSun: the direction toward the sun, three's axes, for a view yaw (radians, the browser's)
	FVector SunDirectionThree(float SunUpDegrees, float Yaw);
	// three (x, y up, z) to Unreal (X, Y, Z up), as the model export maps positions
	inline FVector ThreeToUnreal(const FVector& V) { return FVector(V.X, V.Z, V.Y); }
	// light.js outdoorEnvironment: the radiance the PMREM is filtered from, for a direction in three's axes
	FLinearColor OutdoorEnvironment(const FVector& DirThree);
	// the sky light's radiance from a direction (three's axes): the environment (turned by -EnvYaw about y, as three's
	// envMapRotation does) times EnvIntensity, plus the hemisphere light as a radiance linear in height,
	// L = I/pi ((top + bottom)/2 + 3/4 (top - bottom) y), whose cosine irradiance is mix(bottom, top, (n.y + 1)/2) I
	FLinearColor SkyRadiance(const FVector& DirThree, const FTCMood& M, float EnvIntensity, float EnvYaw);
	// the direction of a cube face texel's centre, Unreal axes (TextureCompressorModule's face table)
	FVector CubeTexelDirection(int32 Face, int32 X, int32 Y, int32 Size);
	// what the sky light should give a white Lambert surface facing NormalUnreal (numerically integrated SkyRadiance)
	FLinearColor ExpectedDiffuse(const FVector& NormalUnreal, const FTCMood& M, float EnvIntensity, float EnvYaw);
	// a transient float cubemap of SkyRadiance for the sky light
	UTextureCube* MakeSkyCube(const FTCMood& M, float EnvIntensity, float EnvYaw, int32 Size = 64);
	// exposure locked at 1 and the effects three does not draw turned off; Post replaces the tonemapper
	void LockPostProcess(FPostProcessSettings& S, UMaterialInterface* Post, float AmbientOcclusion);
	// M_TCLookPost's parameters: exposure, haze (fog) distances in metres, the haze color (linear in; set both in the
	// output space, Haze, and linear, HazeLinear) and where it mixes: bHazeBeforeTone true is Graphics High's composer
	// (linear light, before the tone curve, the background tone-mapped too), false the direct path (after the curve)
	UMaterialInstanceDynamic* MakePost(UObject* Outer);
	// bNeutral (round 16, hudfit): three's NeutralToneMapping in place of ACES filmic (M_TCLookPost's Neutral; the lobby
	// backdrop, client/lobby-view.js); every other caller keeps ACES filmic
	void SetPost(UMaterialInstanceDynamic* Post, float Exposure, float FogNear, float FogFar, const FLinearColor& HazeLinear, bool bHazeBeforeTone = false, bool bNeutral = false);
	// the haze the reference shows where the fog is full (or nothing was drawn), as display bytes 0-255
	FLinearColor HazeOnScreen(const FLinearColor& HazeLinear, float Exposure, bool bHazeBeforeTone);
	// MPC_TCLook: the hemisphere and environment the model materials add
	void SetCollection(UWorld* World, const FTCMood& M, float EnvIntensity, float EnvYaw, bool bOn = true);
	// the model masters add their own indirect light only without static lighting (else the engine's sky light reaches them)
	bool IndirectInMaterials();
	// every look asset loads (false names the missing ones)
	bool AssetsPresent(FString& Missing);
	// Units and the sun's shadow. The reference's unit models never receive shadows (three's receiveShadow is false on
	// every mesh unit-models.js builds) and its soldiers cast none either (bakeMeshes(..., shadow false), drawSoldiers'
	// pools); hulls, guns and structures cast. Here unit models and soldiers are drawn on lighting channel 1, lit by an
	// unshadowed copy of the sun (MakeUnitSun), and a hidden twin of every casting unit part casts the shadow on channel
	// 0 (the ground, the map pieces and the scenery, lit by the shadowed sun). Units stay on channel 0 in a world without
	// that second sun (another tool's scratch world), so they are never left unlit.
	constexpr int32 UnitChannel = 1;
	bool HasUnitSun(const UWorld* World);
	// the unshadowed sun for the units, matching Sun (direction, colour, intensity), registered in World
	UDirectionalLightComponent* MakeUnitSun(UObject* Outer, UWorld* World, const UDirectionalLightComponent* Sun);
	void MatchUnitSun(UDirectionalLightComponent* UnitSun, const UDirectionalLightComponent* Sun);
}

// The game's light: spawned once by the game mode in place of the engine-default sun, sky atmosphere and real-time
// sky light. It follows the match (StartMatch), the weather (SetWeather, SetShower) and the camera (fog distance).
UCLASS()
class THREECROSSROADS_API ATCLighting : public AActor
{
	GENERATED_BODY()

public:
	ATCLighting();
	// main.js at a match start: atmosphere.js start (the mood for the map and its weather, no easing in) and light.js
	// aimSun (the sun up and to the right of the opening view). BrowserYaw: radians, the browser's (camera.js yaw;
	// ATCCameraPawn's Yaw in degrees = -90 - degrees(BrowserYaw)). Override: a mood name ('?mood=' in the browser).
	void StartMatch(const FString& MapName, const FString& MapKey, const FString& WeatherNow, float BrowserYaw, FName Override = NAME_None);
	// atmosphere.js setWeather: eases toward the new weather (about 8 s)
	void SetWeather(const FString& Kind);
	// snapshot wx: rain and wet ground, 0-1
	void SetShower(float Rain, float Wet);
	// camera.js cam.dist in metres; the haze follows it (light.js followView). Without it the distance is read from
	// the view: along the camera's forward ray to GroundZ (cm).
	void SetCameraDistance(float Metres) { CameraDistance = Metres; }
	// Round 15 (hudfit): client/lobby-view.js's own light while the lobby backdrop shows (FTCLobbyLook: hemisphere and
	// sun without shadows, no environment, its fog from 0.3 to 0.9 x the map's larger side mixed after the tone curve);
	// null gives the mood back. The game mode passes it every frame (FTCLobbyBackdrop::Look). Not ported: three's
	// NeutralToneMapping (M_TCLookPost's curve stays ACES filmic, at exposure 1).
	void SetLobbyLook(const FTCLobbyLook* L);
	bool HasLobbyLook() const { return bLobbyLook; }
	void SetGroundZ(float Cm) { GroundZ = Cm; }
	// the look the lights show now
	const FTCMood& Shown() const { return Look; }
	FName MoodName() const { return Base.Name; }
	float FogNear() const { return LastFogNear; }
	float FogFar() const { return LastFogFar; }
	// where the haze mixes (TCLook::SetPost): true unless -TCHazeAfterTone. The graphics level sets it: light.js
	// renderFrame draws High through the composer (true) and Low straight to the canvas (false); a -TCHazeAfterTone
	// on the command line keeps the direct path's order at every level.
	bool HazeBeforeTone() const { return bHazeBeforeTone; }
	void SetHazeBeforeTone(bool bBefore) { bHazeBeforeTone = bBefore && !bHazeAfterToneForced; }
	// The engine sky light's captured diffuse for a white surface facing up, against what its cube should give
	// (TCLook::ExpectedDiffuse), and how many captures it took. The map pieces, scenery and soldiers take their sky
	// fill (light.js hemisphere and environment) from this capture; a capture made before the transient cube's
	// render resource exists comes back black, so it is checked and redone until it holds the cube's light.
	bool SkyCaptured() const { return bSkyCaptured; }
	FLinearColor SkyCapturedUp() const { return SkyGotUp; }
	FLinearColor SkyExpectedUp() const { return SkyWantUp; }
	int32 SkyCaptureTries() const { return SkyTries; }

	virtual void BeginPlay() override;
	virtual void Tick(float DeltaSeconds) override;

	UPROPERTY() TObjectPtr<UDirectionalLightComponent> Sun;
	UPROPERTY() TObjectPtr<UDirectionalLightComponent> UnitSun; // the same sun without shadows, lighting channel 1 only (TCLook::UnitChannel)
	UPROPERTY() TObjectPtr<USkyLightComponent> Sky;
	UPROPERTY() TObjectPtr<UPostProcessComponent> Post;
	UPROPERTY() TObjectPtr<UMaterialInstanceDynamic> PostMaterial;
	UPROPERTY() TObjectPtr<UTextureCube> SkyCube;

private:
	void Apply(bool bForceSky);
	void AimSun(float BrowserYaw);
	void CheckSkyCapture();
	FTCMood Base, Look;
	bool bLobbyLook = false;
	FTCLobbyLook LobbyLook;
	FTCWeatherLook Cur, Goal;
	FLinearColor HazeNow = FLinearColor::White, HazeGoal = FLinearColor::White;
	FString WeatherKind = TEXT("clear");
	float Rain = 0.f, Wet = 0.f, Yaw = 0.f, CameraDistance = -1.f, GroundZ = 0.f;
	float SkyEnv = -1.f, LastFogNear = 0.f, LastFogFar = 0.f;
	bool bOutputLinear = false, bViewportLogged = false, bHazeBeforeTone = true, bHazeAfterToneForced = false;
	int32 EnvLevel = -1; // the graphics level (Client/TCGraphics.h) the environment was last applied for
	int32 bAoApplied = -1; // round 14 lead glue: the AO pass the post settings were last given (1 High, 0 Low)
	FLinearColor SkyTop, SkyBottom;
	// the sky light capture check (CheckSkyCapture): pending until the capture holds at least half the cube's light
	bool bSkyCheck = false, bSkyCaptured = false;
	int32 SkyTries = 0;
	double SkyCheckAt = 0.0;
	FLinearColor SkyWantUp = FLinearColor::Black, SkyGotUp = FLinearColor::Black;
};

// -run=TCLookCompare -Cameras=<unreal/Tools/look output>/cameras.json -Out=<dir> -Light=look|before|gamebefore
// Renders every asset and view of the reference's cameras.json through ATCUnitRenderer, as -run=TCModelCompare does,
// under one of three lights: 'look' (this file's), 'before' (TCModelCompare's stand-in light) or 'gamebefore'
// (TCModelCompare -GameLight, ATCGameMode::SpawnScene's light before this slice). Writes <asset>-<view>-unreal-<light>.png,
// the unit's silhouette (<asset>-<view>-mask-unreal.png, from scene depth with the ground hidden) and the ground plate
// (<asset>-<view>-plate-unreal-<light>.png, the unit not drawn). ':man' assets draw the squad at one man's hit points.
UCLASS()
class UTCLookCompareCommandlet : public UCommandlet
{
	GENERATED_BODY()
public:
	UTCLookCompareCommandlet();
	virtual int32 Main(const FString& Params) override;
};

// -run=TCWorldLook -Out=<dir> [-Views=x,z,dist,yaw;...] [-Suffix=look] [-Ao=0]
// The native half of unreal/Tools/world/compare (village.mjs): the reference's map pieces and scenery on the default
// map (the scenery fixture's first case, Tools/world/fixtures/scenery-oracle.json.gz: default.json as drawn) through
// FTCWorldObjects and FTCWorldObjectsView with the world materials, on the viewer's flat grass (MeshLambert 0x6c6c45),
// lit as ATCLighting lights the game (sun, sky light cube, MPC_TCLook, M_TCLookPost) in the default mood with the sun
// aimed for yaw 0, from camera.js's camera (fov 42, pitch 0.95). three's fog stays at setupLight's 50 to 300 m there
// (the page renders without followView), and no AO pass. Writes default-<x>-<z>-<dist>-unreal-<suffix>.png.
UCLASS()
class UTCWorldLookCommandlet : public UCommandlet
{
	GENERATED_BODY()
public:
	UTCWorldLookCommandlet();
	virtual int32 Main(const FString& Params) override;
};
