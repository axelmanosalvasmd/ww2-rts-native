// Game sound: effects, the player's unit voices, alert and UI sounds and the battle beds, under one volume control
// (parity gap 9: CLI-115 to CLI-119). A port of the pinned reference client at TCAudioRules::ReferencePin:
//
//   client/audio.js         the mix table (MIX, UI, ALERTS), place() (distance from the camera's focus, quieter as the
//                           camera pulls back, pan by yaw), launch() (gaps, voices per effect, the 32-voice cap and its
//                           quietest-first stealing, pitch and level spread, boost, ambience ducking, loops with their
//                           own fade), refresh(), bed(), environment() and its four synthesized loops, voice() (one line
//                           at a time, 2.5 s apart, a different line than last time, either take), alert() (per-kind gap
//                           and the voice line 600 ms later), ui(), start() and end() (load at match start, the
//                           speaker by seat, match_start only when loading took under 4 s), the volume (square curve,
//                           default 80%, M back to the volume before muting, saved);
//   client/audio/index.json the 35 effects (length, boost, loop) and the voice folders us, de, ru (two speakers each,
//                           move 4, attack 3, fire 2, retreat 2 and lost 1 lines, two takes); the UK speaks American;
//   client/fx.js            which sound each delivered shot makes and where: the GUNS table's sound per shooter type, a
//                           burst recording once per burst (mg 1.25 s, smg 0.7 s per shooter), a rifle crack per soldier
//                           with his flash, the gun heard at the shooter only when it is in this snapshot's visible set
//                           (else where the rounds land), impacts (blast_small for shells, a ricochet off armor),
//                           blasts, bombs, salvos and their whistles timed to end as the round lands, collapses
//                           (lighter where a hedge or tree became open ground), smoke shells and new smoke clouds,
//                           wrecks (vehicle_destroyed and a 14 s fire loop), the artillery strike whistle, flak;
//   client/main.js          the air shots (support planes, shoot-downs, flak, anti-air fire), the kill marks that make a
//                           removed vehicle a wreck, the tower that topples, and the UI blip;
//   client/projectiles.js   which flight launches and contacts are fresh (each once, within 0.2 s and 0.25 s);
//   client/battle-sound.js  the tank engine bed from moving vehicles, digging and building now and then;
//   client/engine-presentation.js environmentalMix (wind, rain, water and woodland from known cells near the camera);
//   client/alerts.js        sound(): the same alert kind sounds at most every 1.5 s from the feed.
//
// Privacy: every cue comes from this recipient's own snapshot. A shooter's sound is placed at the shooter only when its
// id is in the snapshot's units; otherwise it plays where the delivered shot lands, exactly as fx.js does. Nothing is
// inferred about hidden units; water and woodland count only cells this side has seen or explored.
//
// The pure parts (FTCAudioMixer, FTCAudioCues and TCAudioRules) hold no engine objects, so the automation tests drive
// them directly. UTCAudio (a world subsystem) plays the mixer's sounds through Unreal audio components: the reference
// computes every gain and pan itself (Web Audio gain and stereo panner nodes), so each component is unattenuated and is
// placed around the listener at the azimuth whose stereo pan law equals the reference's (Unreal's equal-power pan
// between the front pair at -30 and +30 degrees: azimuth = pan * 30 degrees).
//
// Differences, recorded:
//   - a tracer's flight time (the delay before its impact sound) is measured from the shooter's position at 1 m over
//     flat ground, not from the drawn muzzle on the terrain: the browser's muzzle and height field are not here;
//   - the browser's random numbers are Math.random; here an FRandomStream (seeded per run, fixed in tests);
//   - the ambient.mp3 loop is imported (it is in the library) but, as in the reference at the pin, never played.
#pragma once

#include "CoreMinimal.h"
#include "Math/RandomStream.h"
#include "Subsystems/WorldSubsystem.h"
#include "Engine/StreamableManager.h"
#include "TCAudio.generated.h"

class UAudioComponent;
class USoundSubmix;
class USoundWaveProcedural;
class USoundWave;
class FTCMatchState;
class FTCUnitTable;
class FTCSettingsStore;
struct FTCShot;
struct FTCSoundCue;

enum class ETCAudioBus : uint8 { Sfx, Amb, Voice, Ui };

namespace TCAudioRules
{
	THREECROSSROADS_API extern const TCHAR* const ReferencePin;

	constexpr int32 MaxPlaying = 32;          // audio.js MAX_PLAYING
	constexpr double VoiceGap = 2.5;          // audio.js VOICE_GAP
	constexpr double VoiceBusGain = 1.2;      // buses.voice.gain
	constexpr double AlertVoiceDelay = 0.6;   // alert(): setTimeout(..., 600)
	constexpr double FeedSoundGap = 1.5;      // alerts.js SOUND_GAP
	constexpr double DefaultVolume = 0.8;     // firstVolume when nothing is saved
	constexpr double MatchStartWithin = 4.0;  // start(): match_start only if loading took less
	constexpr double RefreshEvery = 0.1;      // listener(): refresh() at most every 100 ms
	constexpr double BattleEvery = 0.2;       // battle-sound.js: engines, digging and building
	constexpr double AmbienceEvery = 0.4;     // main.js ambienceTimer
	constexpr double DuckLevel = 0.32;        // duckAmbience
	constexpr int32 DigFlag = 16, BuildFlag = 128;  // shared/sim.js unitFlags (battle-sound.js)
	constexpr int32 CellMetres = 2;           // shared/sim.js CELL
	constexpr int32 MaxDelayed = 768;         // fx.js EVN

	// audio.js MIX entry. Zero means "not set" for Range, Max and Gap, as the reference's falsy checks read them.
	struct FMix
	{
		double Gain = 1.0, Range = 0.0;
		int32 Max = 0;
		double Gap = 0.0, Pitch = 0.0, Vol = 0.15;
	};
	THREECROSSROADS_API const FMix* FindMix(const FString& Name);
	// client/audio/index.json sfx
	struct FClip
	{
		const TCHAR* Id;
		double Dur, Boost;
		bool bLoop;
	};
	THREECROSSROADS_API TConstArrayView<FClip> Clips();
	THREECROSSROADS_API const FClip* FindClip(const FString& Id);
	// audio.js ALERTS: sound, gap between two of the kind, and the voice line that follows (or null)
	struct FAlert
	{
		const TCHAR* Kind;
		const TCHAR* Sound;
		double Gap;
		const TCHAR* Voice;
	};
	THREECROSSROADS_API const FAlert* FindAlert(const FString& Kind);
	// audio.js UI
	struct FUi
	{
		const TCHAR* Kind;
		double Gain, Gap;
	};
	THREECROSSROADS_API const FUi* FindUi(const FString& Kind);

	// Voices (index.json voice): the folder a faction speaks from (FACS; one without voices speaks American), its
	// speakers in sorted order, the lines per kind and the file stem of line Line (0-based), take Take (0-based).
	THREECROSSROADS_API const TCHAR* VoiceFolder(int32 Faction);
	THREECROSSROADS_API TArray<FString> Speakers(const FString& Folder);
	THREECROSSROADS_API int32 LineCount(const FString& Kind);
	constexpr int32 Takes = 2;
	THREECROSSROADS_API FString VoiceStem(const FString& Speaker, const FString& Kind, int32 Line, int32 Take);
	THREECROSSROADS_API TArray<FString> VoiceStems(const FString& Speaker);
	// Where the import tool puts them (unreal/Tools/import-reference-audio.sh)
	THREECROSSROADS_API FString SfxAssetPath(const FString& Id);
	THREECROSSROADS_API FString VoiceAssetPath(const FString& Folder, const FString& Stem);

	// the slider feels even when gain follows its square
	inline double Curve(double V) { return V * V; }

	// The listener: the camera's focus (reference metres), its distance and the browser's yaw in radians.
	struct FEar
	{
		double X = 0.0, Z = 0.0, Dist = 85.0, Yaw = 0.0;
	};
	// This client's camera yaw (degrees) as the browser's (TCCameraFeel: Yaw = -90 - degrees(yaw)).
	THREECROSSROADS_API double BrowserYaw(double UnrealYawDegrees);
	struct FPlace
	{
		double Gain = 1.0, Pan = 0.0;
	};
	// audio.js place(): no position is a UI sound at full level in the middle
	THREECROSSROADS_API FPlace Place(const FEar& Ear, const FVector2D* Pos, double Range);

	// engine-presentation.js environmentalMix: the four beds' targets
	struct FBedTarget
	{
		double Gain = 0.0, Pan = 0.0;
	};
	struct FEnvironment
	{
		FBedTarget Wind{ 0.055, 0.0 }, Water, Woodland, Rain;
	};
	enum class ECell : uint8 { None, Water, Woodland };
	// Classify(cx, cz) says what a known cell holds (None for an unknown or plain one); W and H are the grid's size.
	THREECROSSROADS_API FEnvironment EnvironmentMix(int32 W, int32 H, TFunctionRef<ECell(int32, int32)> Classify, const FEar* Camera,
		const FString& Weather, const double* Wx, int32 WxNum);
}

// audio.play's options: gain, rate, delay and offset (seconds), dur (a loop that plays that long), and the overrides
// a caller may spread over launch's arguments (bus, mix, the name gaps and caps count under).
struct FTCAudioPlayOptions
{
	double Gain = 1.0, Rate = 1.0, Delay = 0.0, Offset = 0.0, Dur = 0.0;
	TOptional<ETCAudioBus> Bus;
	TOptional<TCAudioRules::FMix> Mix;
	FString Name;
};

// AudioParam.setTargetAtTime: Value moves toward Target with time constant Tau.
struct FTCAudioParam
{
	double Value = 0.0, Target = 0.0, Tau = 0.1;
	void To(double InTarget, double InTau) { Target = InTarget; Tau = InTau; }
	void Snap(double V) { Value = Target = V; }
	void Step(double Dt);
};

// One launched sound (audio.js launch's p).
struct FTCAudioSound
{
	int32 Handle = 0;
	FString Name, Key;            // what gaps and caps count under; the loaded file
	ETCAudioBus Bus = ETCAudioBus::Sfx;
	bool bHasPos = false;
	FVector2D Pos = FVector2D::ZeroVector;
	TCAudioRules::FMix Mix;
	double Level = 0.0, Base = 0.0;
	FTCAudioParam Gain, Pan;
	double Rate = 1.0, StartAt = 0.0, Offset = 0.0, EndAt = 0.0;
	double LoopFor = 0.0;         // > 0: a loop with its own fade in and out (fire_loop for a wreck)
	bool bDone = false;           // stopped (fading) or ended; no longer counts as playing
	double FadeFrom = 0.0, FadeLen = 0.0;
	// the loop's own envelope and the stop fade at Now
	double Envelope(double Now) const;
};

// A loop driven by positions (the tank engine) or by the environment (four synthesized beds).
struct FTCAudioBed
{
	int32 Handle = 0;
	FString Name;                 // tank_engine, or amb:wind, amb:water, amb:woodland, amb:rain
	ETCAudioBus Bus = ETCAudioBus::Sfx;
	bool bSynth = false;
	FTCAudioParam Gain, Pan, Rate;
	double StartOffset = 0.0;
	bool bDone = false;
	double FadeFrom = 0.0, FadeLen = 0.0;
	double StopFactor(double Now) const;
};

// A launch or stop as the mixer decided it (for the tests' comparison with client/audio.js run under Node).
struct FTCAudioTraceRow
{
	bool bStart = true;
	FString Key;
	double At = 0.0, Offset = 0.0, Rate = 1.0, Gain = 0.0, Pan = 0.0;
	bool bLoop = false;
};

// mulberry32: the seeded generator the tests and the Node oracle both use in place of Math.random
struct FTCAudioMulberry
{
	uint32 A = 0;
	double Next()
	{
		A += 0x6D2B79F5u;
		uint32 T = (A ^ (A >> 15)) * (1u | A);
		T = (T + ((T ^ (T >> 7)) * (61u | T))) ^ T;
		return (double)(T ^ (T >> 14)) / 4294967296.0;
	}
};

// What a cue selector hands its sounds to: the mixer in the game, a recorder in the tests.
class ITCAudioSink
{
public:
	virtual ~ITCAudioSink() = default;
	virtual void Play(const FString& Name, const FVector2D* Pos, const FTCAudioPlayOptions& Opts) = 0;
	virtual void Bed(const FString& Name, TConstArrayView<FVector2D> Points) = 0;
	// audio.length: seconds a loaded effect lasts (0 until it has loaded)
	virtual double Length(const FString& Name) const = 0;
};

// client/audio.js without Web Audio: every decision it makes, with the sounds it would play as data.
class THREECROSSROADS_API FTCAudioMixer : public ITCAudioSink
{
public:
	explicit FTCAudioMixer(int32 Seed = 0x7c3a);
	// replaces Math.random (tests); unset, a seeded FRandomStream
	void SetRandom(TFunction<double()> Source) { RandomSource = MoveTemp(Source); }
	void Reseed(int32 Seed) { Random.Initialize(Seed); }
	TArray<FTCAudioTraceRow>* Trace = nullptr;

	// ----- what the engine side reports -----
	// a file has loaded (Key: an effect id such as 'rifle', or 'voice:<stem>'); its length in seconds
	void SetLoaded(const FString& Key, double Duration) { Loaded.Add(Key, Duration); }
	bool IsLoaded(const FString& Key) const { return Loaded.Contains(Key); }
	// ctx.state === 'running': an audio device is there to play to
	void SetRunning(bool bOn) { bRunning = bOn; }
	bool IsRunning() const { return bRunning; }
	void Advance(double Dt);       // the audio clock: settles every parameter, ends finished sounds, runs due calls
	void AdvanceTo(double T);      // the same, to an exact clock time (tests)
	double Now() const { return Clock; }

	// ----- audio.js -----
	// start(): forget the last match, pick nothing yet; returns the match token the loading continues with
	int32 BeginMatch(int32 Faction, int32 Slot);
	// the speaker this seat uses (names[slot % names.length] of the faction's folder); empty if the folder has none
	FString ChooseVoice(int32 Token, FString& OutFolder);
	// loading done (effects, then this speaker's lines): the ambience starts, and match_start plays if it took < 4 s
	void FinishLoading(int32 Token);
	bool IsStarted() const { return bStarted; }
	int32 MatchToken() const { return Match; }
	void End();                    // back to the lobby: everything fades out over 0.4 s

	void InitVolume(TOptional<double> Saved);  // firstVolume, then bind()'s setVolume(volume)
	void InitPreferences(const FTCSettingsStore& Preferences); // shared parseFloat and legacy ww2-muted fallback
	void SetVolume(double V);
	void ToggleMute();             // M: mute, or back to the volume before muting
	double Volume() const { return VolumeLevel; }
	bool TakeVolumeChanged() { const bool b = bVolumeChanged; bVolumeChanged = false; return b; }

	void Listener(double X, double Z, double Dist, double Yaw);
	const TCAudioRules::FEar& Ear() const { return EarAt; }
	int32 PlaySound(const FString& Name, const FVector2D* Pos, const FTCAudioPlayOptions& Opts = FTCAudioPlayOptions());
	void Voice(const FString& Kind);   // 'move' | 'attack' | 'retreat' | 'fire' | 'lost'
	void Alert(const FString& Kind);   // 'attack' | 'base' | 'pointWon' | 'pointLost' | 'unitLost' | 'air' | 'ready' | 'event'
	void Ui(const FString& Kind);      // 'click' | 'recruit' | 'error' (anything else clicks)
	void Ambience() { Environment(EnvState); }
	void Environment(const TCAudioRules::FEnvironment& Targets);

	// ITCAudioSink
	virtual void Play(const FString& Name, const FVector2D* Pos, const FTCAudioPlayOptions& Opts) override { PlaySound(Name, Pos, Opts); }
	virtual void Bed(const FString& Name, TConstArrayView<FVector2D> Points) override;
	virtual double Length(const FString& Name) const override;

	// ----- what the engine side plays -----
	const TArray<FTCAudioSound>& Sounds() const { return Playing; }
	const TArray<FTCAudioBed>& Beds() const { return BedList; }
	double MasterGain() const { return Master.Value; }
	double BusGain(ETCAudioBus Bus) const;
	int32 LivePlaying() const;
	const FString& VoiceName() const { return VoiceSpeaker; }
	const FString& VoiceFolder() const { return VoiceFolderName; }
	FString Stats() const;

private:
	int32 Launch(const FString& Key, ETCAudioBus Bus, const FVector2D* Pos, const TCAudioRules::FMix& Mix, double Gain, double Rate, double Delay,
		double Offset, double Dur, const FString& Name);
	void Stop(FTCAudioSound& P, double Fade);
	void DuckAmbience(double Seconds);
	void Refresh();
	double Rand() { return RandomSource ? RandomSource() : Random.FRand(); }
	double Rnd(double A) { return 1.0 + (Rand() * 2.0 - 1.0) * A; }
	FTCAudioBed* FindBed(const FString& Name);

	FRandomStream Random;
	TFunction<double()> RandomSource;
	TMap<FString, double> Loaded;
	bool bRunning = false, bStarted = false;
	double Clock = 0.0;
	int32 Match = 0, NextHandle = 1;
	int32 Faction = 0, Slot = 0;
	double MatchStartedAt = 0.0;
	FString VoiceFolderName, VoiceSpeaker;
	TArray<FTCAudioSound> Playing;
	TArray<FTCAudioBed> BedList;
	TMap<FString, double> LastStart, LastAlert;
	TMap<FString, int32> LastLineIndex;
	double LastLineAt = -9.0, VoiceUntil = 0.0;
	TCAudioRules::FEar EarAt;
	double EarRefreshedAt = 0.0;  // audio.js earAt starts at 0
	double VolumeLevel = TCAudioRules::DefaultVolume, MutedFrom = TCAudioRules::DefaultVolume;
	bool bVolumeChanged = false;
	FTCAudioParam Master, AmbGain;
	double AmbientDuckUntil = 0.0;
	TCAudioRules::FEnvironment EnvState;
	struct FPendingVoice { double At; FString Kind; };
	TArray<FPendingVoice> PendingVoices;
};

// ----- cue selection (fx.js, main.js, projectiles.js, battle-sound.js) -----

// A delivered shot, with the fields the sounds read. Presence matters: F and T are absent when the server removed
// them for this recipient (the shooter or target is not in its view).
struct FTCAudioShot
{
	FString K, Kind, Effect, Id;
	bool bHasPos = false;
	double X = 0.0, Z = 0.0;
	TOptional<int32> F, T, N;
	int32 Flight = 0, Seq = -1;
	double Time = 0.0;
	bool bHit = false, bHitPresent = false, bKill = false, bTerminal = false, bLocal = false, bPhysical = false;
	static FTCAudioShot From(const FTCShot& S);
};

// A unit in this snapshot (the visible set), with what the sounds read of its type.
struct FTCAudioUnit
{
	int32 Id = 0, Owner = -1, Flags = 0, Alive = 1;
	FString Type;
	double X = 0.0, Z = 0.0;
	bool bKnown = false, bInfantry = false, bStructure = false, bAir = false; // bKnown: UNITS[type] exists
	float Radius = 1.f;
};

// A UNITS entry as the cue selector reads it (bKnown: UNITS[type] exists).
struct FTCAudioType
{
	bool bKnown = false, bInfantry = false, bStructure = false, bAir = false;
	float Radius = 1.f;
};

struct FTCAudioStrike { FString Kind; double X = 0.0, Z = 0.0, T = 0.0; };
struct FTCAudioSmoke { double X = 0.0, Z = 0.0, R = 0.0; TOptional<int32> Id; };
struct FTCAudioFire { double X = 0.0, Z = 0.0; TCHAR Kind = '.'; };

// One snapshot as the sounds see it.
struct FTCAudioSnapshot
{
	int32 Tick = 0;
	TConstArrayView<FTCAudioShot> Shots;
	const TMap<int32, FTCAudioUnit>* Units = nullptr;   // this snapshot's units: the seen set
	TFunction<bool(const FString&, FTCAudioType&)> TypeOf;
	TFunction<bool(double, double)> OpenedCell;          // a cell this snapshot turned into open ground ('.')
	TFunction<bool(const FTCAudioShot&)> OwnShot;        // fps.ownShot: the companion's own shots (none here yet)
	TConstArrayView<FTCAudioStrike> Strikes;
	TConstArrayView<FTCAudioSmoke> Smokes;
	TConstArrayView<FTCAudioFire> Fires;
};

class THREECROSSROADS_API FTCAudioCues
{
public:
	explicit FTCAudioCues(int32 Seed = 0x51a7);
	void SetRandom(TFunction<double()> Source) { RandomSource = MoveTemp(Source); }
	void Reseed(int32 Seed) { Random.Initialize(Seed); }
	void Reset();
	// main.js applySnapshot's sound part, in its order: kill marks, air shots, projectiles, fx.js snapshot, removals
	void Snapshot(const FTCAudioSnapshot& S, ITCAudioSink& Sink);
	// fx.js update: the effects clock and its delayed sounds
	void Frame(double Dt, ITCAudioSink& Sink);
	// battle-sound.js battleFrame (the listener is the caller's): Now in seconds; Me the seat
	void Battle(double Now, const TMap<int32, FTCAudioUnit>& Units, int32 Me, ITCAudioSink& Sink);
	double Clock() const { return FxClock; }
	int32 Delayed() const { return Events.Num(); }
	struct FGun;    // fx.js GUNS entry (TCAudio.cpp)

private:
	struct FEvent
	{
		double Due = 0.0;
		bool bImpact = false;
		FString Name;
		FVector2D Pos = FVector2D::ZeroVector;
		double Offset = 0.0, Gain = 0.0;
		const FGun* Style = nullptr;
		int32 Flags = 0;
	};
	void Fx(const FString& Name, double X, double Z, double Gap, ITCAudioSink& Sink, const FTCAudioPlayOptions* Opts = nullptr);
	void Later(double Delay, const FString& Name, double X, double Z, double Offset = 0.0, double Gain = 0.0);
	void LaterImpact(double Delay, double X, double Z, const FGun* Style, int32 Flags);
	void Whistle(double Delay, double Flight, double X, double Z, double Gain, const ITCAudioSink& Sink);
	void Impact(double X, double Z, const FGun* Style, int32 Flags, ITCAudioSink& Sink);
	void DirectFire(const FTCAudioShot& Sh, const FTCAudioUnit* From, const FTCAudioUnit* To, const FTCAudioSnapshot& S, ITCAudioSink& Sink);
	void Salvo(const FTCAudioShot& Sh, const FTCAudioUnit* From, ITCAudioSink& Sink);
	void AntiAir(const FTCAudioShot& Sh, const FTCAudioUnit* From, const FTCAudioUnit* To, ITCAudioSink& Sink);
	void ProjectileLaunch(const FTCAudioShot& Sh, ITCAudioSink& Sink);
	void ProjectileContact(const FTCAudioShot& Sh, ITCAudioSink& Sink);
	bool Fresh(int32 Id, int32 Sequence);
	double Rand() { return RandomSource ? RandomSource() : Random.FRand(); }
	double Rr(double A, double B) { return A + (B - A) * Rand(); }

	FRandomStream Random;
	TFunction<double()> RandomSource;
	double FxClock = 0.0;
	TMap<FString, double> LastPlay;
	TMap<int64, double> LastBurst;
	TArray<FEvent> Events;
	TSet<FString> StrikeSounds;
	TSet<int64> Clouds;
	TMap<int32, int32> FlightEvents;
	TArray<int32> FlightOrder;
	int32 ProjTick = -1;
	TMap<int32, FTCAudioUnit> Held;     // the units the client holds (last snapshot's)
	TSet<int32> Killed;
	struct FThrow { double X = 0.0, Z = 0.0; bool bSatchel = false; double T = -1e9; };
	FThrow Throws[12];                  // fx.js throws: the last 12 throws seen
	int32 ThrowNext = 0;
	// battle-sound.js
	double BattleTick = -1e9;
	TMap<int32, FVector2D> BattleSeen;
	TMap<int32, double> Foley;
};

// The adapters from what Net/TCMatchState decoded (read only).
namespace TCAudioView
{
	// this snapshot's units with their type's flags (shared/sim.js UNITS via the unit table) and alive men
	THREECROSSROADS_API TMap<int32, FTCAudioUnit> Units(const FTCMatchState& S, const FTCUnitTable& Table);
	THREECROSSROADS_API TArray<FTCAudioShot> Shots(const FTCMatchState& S);
	THREECROSSROADS_API TArray<FTCAudioFire> Fires(const FTCMatchState& S);
	// environmentalMix from the map's known cells and the public weather
	THREECROSSROADS_API TCAudioRules::FEnvironment Environment(const FTCMatchState& S, const TCAudioRules::FEar& Ear);
}

// One of the reference's four synthesized environment loops (audio.js environmentalBuffer and the bed's
// BiquadFilterNode): wind, water, woodland or rain, 8 s at Rate, filtered, as floats in [-1, 1].
namespace TCAudioRules
{
	THREECROSSROADS_API TArray<float> RenderNoise(const FString& Kind, int32 Rate);
}

// The client's sound. The lead wires it: MatchStarted / ReturnedToLobby from the room state, Snapshot after each
// applied snapshot, Frame once per frame with the camera, Voice from orders (client/orders.js feedback voices, retreat
// 'retreat', fire-at 'attack', rail 'move'), Blip for the UI tones, FeedAlert from the alert feed, Cues for
// FTCObjectives::TakeSounds, SetVolume / ToggleMute from the menu slider and the M key.
UCLASS()
class THREECROSSROADS_API UTCAudio : public UTickableWorldSubsystem
{
	GENERATED_BODY()
public:
	virtual bool ShouldCreateSubsystem(UObject* Outer) const override;
	virtual void Initialize(FSubsystemCollectionBase& Collection) override;
	virtual void Deinitialize() override;
	virtual void Tick(float DeltaTime) override;
	virtual TStatId GetStatId() const override;
	virtual bool IsTickableWhenPaused() const override { return true; }

	// the match began for this seat (faction index 0 USA, 1 Germany, 2 USSR, 3 UK; slot is the seat): load and start
	void MatchStarted(int32 Faction, int32 Slot);
	void ReturnedToLobby();
	// after FTCMatchState::ApplySnapshot: the sounds of this snapshot's shots, wrecks and smoke
	void Snapshot(const FTCMatchState& S, const FTCUnitTable& Table);
	// once per frame: the listener (focus in reference metres, distance in metres, this client's yaw in degrees), the
	// battle beds, the environment every 0.4 s and the delayed effect sounds (Dt: the effects' frame time)
	void Frame(float Dt, const FTCMatchState& S, const FTCUnitTable& Table, FVector2D Focus, float Distance, float YawDegrees);

	void Voice(const FString& Kind) { Asked.FindOrAdd(TEXT("voice:") + Kind)++; Mixer.Voice(Kind); }
	void Alert(const FString& Kind) { Mixer.Alert(Kind); }
	// alerts.js sound(kind): at most every 1.5 s per kind, then alert()
	void FeedAlert(const FString& Kind);
	void Ui(const FString& Kind) { Asked.FindOrAdd(TEXT("ui:") + Kind)++; Mixer.Ui(Kind); }
	// main.js blip(): a string kind, or a tone number (any number clicks)
	void Blip(const FString& KindOrTone);
	void Cues(TConstArrayView<FTCSoundCue> Sounds);
	int32 Play(const FString& Name, TOptional<FVector2D> Pos, const FTCAudioPlayOptions& Opts = FTCAudioPlayOptions());
	void SetVolume(double V);
	void ToggleMute();
	double Volume() const { return Mixer.Volume(); }
	FString Stats() const { return Mixer.Stats(); }
	// What was asked since the last call, for the log: the cue selector's sounds by name (fx.js, main.js, battle-sound.js;
	// asked even when the mixer is suspended and plays nothing), the UI, voice and alert calls, and the components
	// started. "" when nothing was asked.
	FString TakeCueReport();
	// Components started since this subsystem began (each also logged as "audio: play").
	int32 StartedCount() const { return Starts; }
	// Actual async callbacks and loaded waves, separate from mixer started state or audible output.
	struct FBankState
	{
		int32 Generation = 0, Effects = 0, VoiceLines = 0, ExpectedVoiceLines = 0;
		FString Folder, Speaker;
		bool bCallbackCompleted = false;
		bool FullyLoaded() const { return bCallbackCompleted && Effects == TCAudioRules::Clips().Num() && VoiceLines == ExpectedVoiceLines; }
	};
	FBankState BankState() const;

	FTCAudioMixer& GetMixer() { return Mixer; }

private:
#if WITH_DEV_AUTOMATION_TESTS
	friend struct FTCRows30Access;
#endif
#if WITH_DEV_AUTOMATION_TESTS
	friend class FTCSession26AudioBankCommand;
#endif
	void LoadAssets(int32 Token);
	void OnSfxLoaded(int32 Token);
	void OnVoicesLoaded(int32 Token);
	void Sync();
	UAudioComponent* MakeComponent(USoundBase* Sound);
	void Place(UAudioComponent* C, double Pan) const;

	UPROPERTY() TObjectPtr<USoundSubmix> MasterDynamics;
	FTCAudioMixer Mixer;
	FTCAudioCues Cue;
	FStreamableManager Streamable;
	TSharedPtr<FStreamableHandle> SfxHandle, VoiceHandle;
	UPROPERTY() TMap<FString, TObjectPtr<USoundWave>> Waves;
	UPROPERTY() TMap<int32, TObjectPtr<USoundWaveProcedural>> Noise; // per environment bed
	TMap<FString, TSharedPtr<TArray<int16>, ESPMode::ThreadSafe>> NoiseLoops;
	USoundWaveProcedural* MakeNoise(const FString& Kind);
	UPROPERTY() TMap<int32, TObjectPtr<UAudioComponent>> Components;
	TSet<int32> Started;
	TMap<FString, double> FeedLast;
	double MatchLoadStartedAt = 0.0;
	int32 CompletedLoadToken = -1;
	double AmbienceTimer = 0.0;
	double BattleViewAt = -1e9;
	bool bWarnedMissing = false;
	bool bWarnedNoDevice = false;
	TMap<FString, int32> Asked;
	int32 Starts = 0, StartsReported = 0;
	FVector ListenerAt = FVector::ZeroVector, ListenerFront = FVector::ForwardVector, ListenerRight = FVector::RightVector;
};
