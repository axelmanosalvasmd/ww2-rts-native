// Graphics High or Low, the gore switch and the one-time automatic drop to Low: a port of the pinned reference's
// client/gfx.js (reference/ww2-rts @ e4e579a) and what each of its consumers promises per level, as one API the rest
// of this client queries (CLI-122, CLI-123).
//
//   client/gfx.js           level 'high' | 'low', saved as localStorage 'ww2-gfx' ('low' is Low, anything else High);
//                           set() saves and notifies; gore on unless 'ww2-gore' is 'off'; watchFps(dt): above 1/45 s a
//                           frame adds dt to a slow clock, a faster frame takes 2 dt off it (not below 0); at 5 s it
//                           saves 'ww2-gfx-auto' = '1', sets Low and calls onDrop with the notice. It never runs again
//                           once that key exists, nor while the level is Low.
//   client/light.js         applyGfx: Low drops scene.environment, uses BasicShadowMap (hard) at 1024 instead of
//                           PCFShadowMap at 2048 with radius 2.5 * mood.soft; renderFrame calls watchFps(min(0.1, dt))
//                           only while a match's ground exists, and draws High through the GTAO composer, Low straight;
//                           notice(): the message in #status for 6 s (translated); the menu's gfx and gore buttons
//   client/perf.js          renderScale: pixel ratio min(1, 0.75 * devicePixelRatio) on Low, min(devicePixelRatio, 1.5)
//   client/fx.js            CAP 4096 particles (Low 1600), DCAP 64 craters (Low 24) living 150 s (Low 60), no screen shake
//                           on Low (kick), about half the particles emitted, no flipbook frame blending, smoke 0.85 size
//   client/atmosphere.js    applyGfx: Low hides the cloud shadows (cloudBoard, cloudApron) and the birds, keeps rain and
//                           snow at ceil(n / 3) particles and drops the dust fall; mist 18 (High 36), banks 26 (High 64)
//   client/unit-models.js   LOD { high: 110, low: 80 } metres (soldiers and crew guns swap to the far figure)
//   client/model-textures.js modelTexturesOn: the texture layers only on High; infantry-material.js uInfantryDetail
//                           (the atlas normal map) only on High
//   client/water.js         Low freezes the water's drift; client/props.js Low draws no grass and no prop shadows;
//                           client/ground.js 2048 texels across the map (Low 1280); relief.js and structures.js build
//                           their low variants; lobby-view.js hides the lobby backdrop; aircraft.js: smoke drips every
//                           0.07 s (High 0.035), crashed planes throw no chunks, half the flak bursts, damage smoke at
//                           twice the interval; objectives.js one fire source per burning objective (High 2)
//
// Storage: through FTCSettingsStore (Client/TCPersistence.h), the reference's keys mapped to the user settings file.
// What is native here: the engine side of a level (ApplyEngine: screen percentage and the sun's shadow resolution and
// filtering as console variables), and the subscriber list as a multicast delegate.
//
// Round 15 (Graphics Low completed): a change of level is applied in the middle of a match, as gfx.onChange does in the
// browser. OnChanged is the event; the world builders hold an FTCGraphicsWatch on it (World/TCWorldObjects: props.js
// refresh and structures.js rebuild, with the prop and piece shadows; World/TCWater: water.js setLow; World/TCAtmosphere:
// atmosphere.js applyGfx; World/TCRelief via ATCTerrain: relief.js setLow and rebuildGeometry) and apply the new level at
// their next update, the same frame or the next. ground.js reads gfx.low only in createGround, at a start: the
// ground's texels follow the level from the next start (World/TCGroundPaint), as in the browser.
// -TCGraphicsAt=<level>@<seconds>[,...] (verification): switches the level at those match seconds (RunSchedule, called
// by the game mode once a frame with the match clock); the switch is not saved.
#pragma once

#include "CoreMinimal.h"

enum class ETCGraphicsLevel : uint8 { High, Low };
const TCHAR* LexToString(ETCGraphicsLevel Level); // "high" | "low", as gfx.level

// What one level promises (the reference's numbers per consumer, above).
struct THREECROSSROADS_API FTCGraphicsBudget
{
	ETCGraphicsLevel Level = ETCGraphicsLevel::High;
	// light.js
	bool bEnvironment = true;       // scene.environment (the PMREM sky lighting the models)
	bool bSoftShadows = true;       // PCFShadowMap; Low BasicShadowMap
	int32 ShadowMapSize = 2048;     // sun.shadow.mapSize
	float ShadowRadiusPerSoft = 2.5f; // sun.shadow.radius = 2.5 * mood.soft (Low: 1, ShadowRadius)
	float ShadowRadius = 0.f;       // Low only: the fixed radius
	bool bAmbientOcclusion = true;  // the GTAO composer pass
	// fx.js
	int32 ParticleCap = 4096;
	int32 CraterCap = 64;
	float CraterLife = 150.f;
	bool bScreenShake = true;
	bool bFlipbookBlend = true;
	float LitSmokeScale = 1.f;      // Low 0.85 for lit smoke and dust that is not a screen
	// atmosphere.js
	bool bCloudShadows = true;
	bool bBirds = true;
	bool bDustFall = true;
	int32 MistCap = 36;
	int32 BankCap = 64;
	// unit-models.js
	float LodMetres = 110.f;
	// model-textures.js, infantry-material.js
	bool bModelTextures = true;
	bool bInfantryDetail = true;
	// water.js, props.js, ground.js, relief.js, structures.js, lobby-view.js
	bool bWaterMotion = true;
	bool bGrass = true;
	bool bPropShadows = true;
	int32 GroundTexels = 2048;
	bool bReliefLow = false;
	bool bStructuresLow = false;
	bool bLobbyBackdrop = true;
	// aircraft.js, objectives.js
	float PlaneSmokeDrip = 0.035f;
	bool bPlaneChunks = true;
	float FlakBurstShare = 1.f;
	float DamageSmokeIntervalScale = 1.f;
	int32 ObjectiveFireSources = 2;

	static FTCGraphicsBudget For(ETCGraphicsLevel Level);
	// atmosphere.js fall.geometry.setDrawRange(0, low ? ceil(n / 3) : n)
	int32 FallParticles(int32 N) const { return Level == ETCGraphicsLevel::Low ? (N + 2) / 3 : N; }
	// perf.js renderScale: the pixel ratio the reference renders at for a device pixel ratio, rounded to 0.01
	double PixelRatio(double DevicePixelRatio) const;
	// The same as an engine screen percentage: the native window already renders device pixels, so the reference's
	// ratio over the device's (100 on High up to a ratio of 1.5, 75 on Low up to 1.33).
	double ScreenPercentage(double DevicePixelRatio) const;
};

DECLARE_MULTICAST_DELEGATE_OneParam(FTCGraphicsChanged, ETCGraphicsLevel);
DECLARE_MULTICAST_DELEGATE_OneParam(FTCGoreChanged, bool);
class FTCGraphics;

// A subscription to FTCGraphics::OnChanged (gfx.onChange) for the world builders, which apply a new level at their next
// update: TakeChanged is true once after any number of change events. It subscribes at its first TakeChanged (so a
// builder held by a class default object never touches the settings); copies subscribe on their own; the subscription
// ends with the object.
class THREECROSSROADS_API FTCGraphicsWatch
{
public:
	explicit FTCGraphicsWatch(FTCGraphics* Source = nullptr) : Graphics(Source) {} // null: FTCGraphics::Get()
	FTCGraphicsWatch(const FTCGraphicsWatch& Other) : Graphics(Other.Graphics) {}
	FTCGraphicsWatch& operator=(const FTCGraphicsWatch&) { return *this; } // keeps its own subscription
	~FTCGraphicsWatch();
	bool TakeChanged();                                   // a change event arrived since the last call
	int32 Events() const { return Count.IsValid() ? *Count : 0; }
	bool Subscribed() const { return Handle.IsValid(); }
private:
	FTCGraphics* Graphics = nullptr;
	TSharedPtr<int32> Count;
	int32 Taken = 0;
	FDelegateHandle Handle;
};

// Holds the game's level at Level for a scope (tests whose expectations are one level's: the saved level of the machine
// must not decide them), switching without saving and back at the end.
struct THREECROSSROADS_API FTCGraphicsLevelScope
{
	explicit FTCGraphicsLevelScope(ETCGraphicsLevel Level);
	~FTCGraphicsLevelScope();
	ETCGraphicsLevel Was;
};

class THREECROSSROADS_API FTCGraphics
{
public:
	// The game's one instance (reads the saved level and gore the first time it is asked).
	static FTCGraphics& Get();
	// A free-standing instance over its own settings file (tests); Ini empty: memory only.
	explicit FTCGraphics(const FString& Ini);

	ETCGraphicsLevel Level() const { return Current; }
	bool IsLow() const { return Current == ETCGraphicsLevel::Low; }
	const FTCGraphicsBudget& Budget() const { return Current == ETCGraphicsLevel::Low ? LowBudget : HighBudget; }
	// gfx.set: saves and notifies, only on a change (bSave false: notifies without saving, for tests and -TCGraphicsAt)
	void Set(ETCGraphicsLevel Level) { Set(Level, true); }
	void Set(ETCGraphicsLevel Level, bool bSave);
	// Change events so far (each Set that changed the level).
	int32 Changes() const { return ChangeCount; }
	void Toggle() { Set(IsLow() ? ETCGraphicsLevel::High : ETCGraphicsLevel::Low); }
	bool Gore() const { return bGore; }
	void SetGore(bool bOn);
	// gfx.onChange / onGoreChange
	FTCGraphicsChanged OnChanged;
	FTCGoreChanged OnGoreChanged;

	// gfx.js watchFps, called once per rendered frame of a match with the frame's seconds (light.js clamps it to 0.1:
	// ClampFrame). True when it switched to Low by itself this call; OutNotice then holds the English notice.
	bool WatchFps(double Dt, FString* OutNotice = nullptr);
	static double ClampFrame(double Dt) { return FMath::Min(0.1, Dt); }
	double SlowSeconds() const { return SlowFor; }
	bool AutoDropDone() const;

	// Texts (client/light.js, client/gfx.js); the HUD translates them through TCLocale (es.js has every one).
	static FString MenuLabel(ETCGraphicsLevel Level);      // "Graphics: Low" | "Graphics: High"
	static const TCHAR* MenuTip();                         // gfxBtn.title
	static FString GoreLabel(bool bOn);                    // "Gore: On" | "Gore: Off" (es.js 'Gore: {0}')
	static const TCHAR* GoreTip();                         // goreBtn.title
	static const TCHAR* DropNotice();                      // watchFps's onDrop message
	static constexpr double NoticeSeconds = 6.0;           // light.js notice(): cleared after 6 s
	static constexpr double SlowFrame = 1.0 / 45.0, SlowLimit = 5.0;

	// The native engine side of the level: r.ScreenPercentage from the reference's pixel ratio, the sun's shadow map
	// resolution (r.Shadow.MaxResolution) and filtering (r.ShadowQuality 1, unfiltered, on Low as BasicShadowMap;
	// 3 on High). Returns the console lines it set, for logs. Call in a game world only (not in the editor).
	TArray<FString> ApplyEngine(double DevicePixelRatio = 1.0) const;

	// -TCGraphicsAt=low@60,high@120: the scheduled switches (read from the command line the first time Get() is asked).
	// RunSchedule(match seconds) applies each one once its time has come, without saving. ParseSchedule is the reader.
	void RunSchedule(double MatchSeconds);
	static TArray<TPair<double, ETCGraphicsLevel>> ParseSchedule(const FString& Spec);
	int32 ScheduledLeft() const { return Schedule.Num(); }
	// Settings keys (client/gfx.js), through FTCSettingsStore.
	static const TCHAR* const LevelKey;   // 'ww2-gfx'
	static const TCHAR* const AutoKey;    // 'ww2-gfx-auto'
	static const TCHAR* const GoreKey;    // 'ww2-gore'
	static const TCHAR* const ReferencePin;

private:
#if WITH_DEV_AUTOMATION_TESTS
	friend struct FTCRows30Access;
#endif
	FTCGraphics();
	void Load();
	FString Ini;
	bool bMemoryOnly = false;
	TMap<FString, FString> Memory;
	TOptional<FString> Read(const TCHAR* Key) const;
	void Write(const TCHAR* Key, const FString& Value);
	ETCGraphicsLevel Current = ETCGraphicsLevel::High;
	bool bGore = true;
	double SlowFor = 0;
	int32 ChangeCount = 0;
	FTCGraphicsBudget HighBudget, LowBudget;
	TArray<TPair<double, ETCGraphicsLevel>> Schedule;
};
