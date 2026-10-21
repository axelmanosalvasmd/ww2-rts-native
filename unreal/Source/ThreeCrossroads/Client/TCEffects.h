// Combat effects from what this recipient's snapshots deliver (parity gap 6: CLI-110 to CLI-114), drawn with the
// reference's own effect pictures.
//
// Ported from the pinned reference client (reference/ww2-rts at e4e579a):
//   client/fx.js          createEffects: the FX particle presets and the emitters line by line (explode, muzzle,
//                         shotTracer, skyShot, burst, impact, strafeHit, launchRocket and its trail, projectileTrail and
//                         projectileRemove, worldContact, puffCloud and syncClouds, syncFires and updateFires, dustTrails,
//                         workDirt, burning and updateWrecks, collapse, fall and updateFalls, columns), the particle
//                         simulation (simulate: wind drag, gravity, ground bounces, spin), the per-frame draw values
//                         (size growth, colour over life, fade in and out, flipbooks, streaks, anchors, pull), the crater
//                         decals (scorch, fitDecal, updateDecals) and kick (screen shake); the GUNS / BAZOOKA styles and
//                         the FIRE table; the dispatch (snapshot, directFire, salvo, antiAir);
//   client/projectiles.js createProjectileView (flights extrapolated at most 0.15 s past the snapshot, launch and
//                         contact events deduplicated by flight and sequence);
//   client/debris.js      createDebrisView (falling sections and wreck motion interpolated between delivered poses,
//                         collapse contacts shown once and only when fresh);
//   client/rubble-decals.js the ground decal on every known rubble cell ('R');
//   client/main.js        applySnapshot: the air shots aircraft.js draws (airShot, SUPPORT_PLANES), the kill marks
//                         and removeUnit's wreck, chutes; the fires' cell centres and types;
//   client/aircraft.js    supportPlane, flak, aaBurst and shotDown, as simple passes, bursts and falls;
//   client/wind.js        setWind from the snapshot's wx.
// The look: every particle is a billboard of client/textures/fx-atlas.webp (8 x 8 cells), drawn in one sorted,
// premultiplied-alpha batch (UTCFxSpriteComponent with M_TCFxSprite, the port of fx.js's VERT and FRAG): smoke, dust,
// dirt and debris are lit by the sun and sky from the atlas normals and are translucent, so units show through thin
// smoke as in the browser; fire, fireballs, flashes and tracers glow. Craters and burn marks are fx-crater.webp grids
// hugging the ground (Modulate, M_TCFxCrater), rubble cells fx-rubble-materials-v1.webp (M_TCFxRubble). The assets
// are converted and imported by unreal/Tools/effects (Content/Effects). Still placeholders: the support planes,
// paratroopers' canopies and falling sections (aircraft.js and fx.js draw models there) as plain shapes.
// Sounds: the effects only list the names fx.js would play (TakeSounds); UTCAudio places and plays the same shots.
//
// Privacy (the browser's rule, client/fx.js header): effects at a shooter (muzzle flash, tracer, launch, thrown
// grenade, a salvo's rockets leaving the tube) only when the shooter is in this snapshot's held rows; impacts only for
// shots, contacts and flights this recipient was sent; a wreck only for a unit this client held in the snapshot before
// and saw a kill shot on; fires, smoke screens, falling sections and rubble only as delivered. Nothing is inferred from
// another seat's view, and nothing is drawn for a unit or cell this side does not hold.
//
// Interim: the decoder (Net/TCMatchState) does not keep a few shot fields fx.js reads (physical, n, r, dir,
// direction, impulse, text, localized) or the snapshot's smokes and wrecks rows, so FTCShotExtras and the effects read
// those from the same raw message, aligned with FTCMatchState::Shots. A change to the decoder is requested instead.
#pragma once

// round 14: FTCCombatEffects::Plume exists (Client/TCTutorial.cpp feeds it the damaged buildings' bursts)
#define TC_HAS_FX_PLUME 1

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "Camera/CameraModifier.h"
#include "Components/PrimitiveComponent.h"
#include "Commandlets/Commandlet.h"
#include "Net/TCMatchState.h"
#include "Client/TCModelLibrary.h"
#include "Client/TCRender29Aircraft.h"
#include "TCEffects.generated.h"

class FJsonObject;
class FTCUnitTable;
class UInstancedStaticMeshComponent;
class UStaticMesh;
class UMaterialInterface;
class ATCTerrain;
class ATCUnitRenderer;
class FTCAlerts;

// ---------------------------------------------------------------------------------------------------------------------
// The shot fields FTCShot does not carry, read from the raw snapshot in the decoder's order (it skips entries that are
// not objects and stops at TCMatchLimits::Shots), so Extras[i] belongs to State.Shots[i].
struct FTCShotExtra
{
	bool bPhysical = false;                  // the server's physical presentation (projectiles.js shows it instead)
	bool bHasN = false; int32 N = 0;         // salvo: shells
	bool bHasR = false; double R = 0.0;      // salvo: spread
	bool bHasDir = false; double Dir = 0.0;  // support plane heading, shot-down heading, debris direction
	bool bHasDirection = false; FVector Direction = FVector::ZeroVector; // a contact's velocity [x, y, z]
	bool bHasImpulse = false; double Impulse = 1.0;
	bool bHasHitField = false;               // 'hit' present at all (main.js airShot: sh.hit !== undefined)
	bool bHasText = false; FString Text;     // a scripted 'say' line (English)
	bool bHasLocalizedEn = false; FString LocalizedEn; // localized.en, which the browser prefers for English readers
	bool bHasLocalizedEs = false; FString LocalizedEs; // localized.es, for a Spanish reader (client/alerts.js localized[lang])
};

namespace TCShotExtras
{
	// Empty when Raw is null or carries no shots array.
	TArray<FTCShotExtra> Read(const FJsonObject* Raw);
}

// Reference constants (client/fx.js, client/projectiles.js, client/debris.js, client/main.js, client/aircraft.js).
namespace TCFxRules
{
	constexpr int32 Cap = 4096;              // fx.js CAP: particles alive at once
	constexpr double ExtrapolateMax = 0.15;  // projectiles.js: a flight is drawn at most this far past its snapshot
	constexpr double LaunchFresh = 0.2;      // projectiles.js: a launch older than this is not flashed
	constexpr double ContactFresh = 0.25;    // projectiles.js and debris.js: an older contact is not shown
	constexpr int32 EventMemory = 4096;      // projectiles.js and debris.js: remembered event ids
	constexpr double AirAltitude = 20.0;     // client/main.js AIR_ALT
	constexpr double ShakeMin = 3.2;         // fx.js kick: smaller blasts do not shake
	constexpr double PlaneRun = 3.0, PlaneSpan = 140.0; // aircraft.js RUN, SPAN
	constexpr double CellMetres = 2.0;       // CELL
	constexpr int32 DecalCap = 64;           // fx.js DCAP: craters and burn marks at once (the oldest is reused)
	constexpr int32 DecalGrid = 5;           // fx.js DG: each decal is a 5 x 5 grid fitted to the ground
	constexpr float DecalLife = 150.f;       // fx.js scorch: life (gfx High)
	constexpr int32 RubbleCap = 2048;        // rubble-decals.js CAPACITY
}

// What the presentation should draw: the placeholder look of one particle.
enum class ETCFxLook : uint8
{
	Flash,      // a short bright flash (muzzle, blast, flak)
	Fireball,   // an explosion's fireball
	Smoke,      // grey smoke (blasts, gun smoke, grass fires)
	DarkSmoke,  // black smoke (wrecks, burning houses, armour hits)
	Screen,     // a white smoke screen
	Dust,       // tan dust (blast rings, collapses, vehicle trails, builders)
	Clod,       // thrown earth
	Spark,      // sparks, embers
	Debris,     // masonry, wood or metal chips
	Tracer,     // a round in flight (stretched along its velocity)
	Rocket,     // a rocket or shell on an arc (stretched)
	Grenade,    // a lobbed grenade or satchel
	Flame,      // fire on the ground or on a wreck
	Scorch,     // a dark mark on the ground where a blast or fire was
	Plane,      // a support plane crossing, a plane going down
	Canopy,     // a paratrooper's chute
	Section,    // a falling building section (debris.js pose)
	Count
};

// client/fx.js FX: the particle presets, in its order. None: a placeholder shape (plane, canopy, section), not a sprite.
enum class ETCFxPreset : uint8
{
	None,
	Flash, Fireball, Flame, Burn, Muzzle, Tracer, TracerHot, TracerFaint, Rocket, Spark, Ember,
	Smoke, WreckSmoke, Gunsmoke, Screen, FireSmoke, Trail, Flak, AirSmoke, AirSmokeDark,
	Dust, RoadDust, Collapse, Plume, Kick,
	Clod, Masonry, Wood, Metal, Nade, Gib, Mist,
	Count
};

// One fx.js preset (pre({...})): lit 1 = smoke, dust, dirt and debris lit by the sun and sky (C0 -> C1 its linear colour
// over life); otherwise it glows: C0 -> C1 multiplies the picture and Add -> Add1 is how much of its hot part adds light.
// Cell = first atlas cell, Cells = pick one of n, Flip = a flipbook of n cells (once over the life, or looped at Fps),
// A = peak alpha, Grow = end size / start size (Curve: like a square root, else fast then slow), Drag pulls toward the
// wind (times Wind), Grav pulls down (negative rises), Pull moves it toward the camera by Pull x size, Len stretches it
// along its velocity (FixLen: always that long), Anchor stands it on its point, Fin = fade-in share of life, K = fade-out
// curve, Floor = bounces, Cover = a smoke screen (kept full size on Low; not used here), Rv = spin.
struct FTCFxPreset
{
	FVector3f C0 = FVector3f(1.f), C1 = FVector3f(1.f);
	float A = 1.f;
	bool bLit = false, bCover = false;
	float Add = 0.f, Add1 = 0.f;
	int32 Cell = 0, Cells = 1, Flip = 0;
	float Fps = 0.f, Grow = 1.f;
	bool bCurve = false;
	float Rv = 0.f, Drag = 0.f, Wind = 0.f, Grav = 0.f, Pull = 0.f, Len = 0.f;
	bool bFixLen = false, bAnchor = false;
	float Fin = 0.02f, K = 1.f;
	bool bFloor = false, bNoRot = false;
	ETCFxLook Look = ETCFxLook::Smoke;   // what the logs and tests count it as
};

namespace TCFxPresets
{
	const FTCFxPreset& Get(ETCFxPreset P);
	const TCHAR* Name(ETCFxPreset P);
	// fx.js AT: the atlas cells (8 x 8, numbered row by row from the top left, tools/build-fx-atlas.mjs)
	namespace AT
	{
		constexpr int32 Smoke = 0, Wisp = 4, Fire = 8, Flame = 12, Fireball = 24, Glow = 39, Plume = 40, Kick = 44, Soil = 48,
			Brick = 50, Stone = 52, Wood = 54, Metal = 55, Muzzle = 56, Tracer = 60, Spark = 61, Puff = 62, Ember = 63;
	}
}

// One particle: fx.js's 40-float record (S), with the preset it came from.
struct FTCFxParticle
{
	ETCFxLook Look = ETCFxLook::Smoke;
	ETCFxPreset Preset = ETCFxPreset::None;
	FVector P = FVector::ZeroVector;      // metres; X = map x, Y = map z, Z = up (Unreal axes, before x100)
	FVector V = FVector::ZeroVector;      // metres per second
	float Age = 0.f, Life = 1.f;          // a negative age: held back, not out yet (fx.js hold)
	float Size0 = 1.f, Size1 = 1.f;       // half the quad's width at birth and at the end (fx.js size, size * grow)
	FVector3f C0 = FVector3f(1.f), C1 = FVector3f(1.f);
	float Alpha = 1.f, Add0 = 0.f, Add1 = 0.f;
	float Rot = 0.f, Spin = 0.f, Drag = 0.f, Grav = 0.f, Pull = 0.f, Len = 0.f, Fin = 0.f, K = 1.f, Wind = 0.f;
	bool bFloor = false, bLit = false, bCurve = false, bFixLen = false, bAnchor = false;
	int32 Cell = 0, Flip = 0;
	float Fps = 0.f, Phase = 0.f;
	float GroundZ = 0.f;                  // the ground under it (fx.js S[35], refreshed now and then as it drifts)
	int32 Group = 0;                      // a smoke cloud's id, or minus a flight's id (fx.js S[29])
	// placeholder shapes only (Preset None)
	float Size = 1.f, Stretch = 0.f;
	FRotator ShapeRot = FRotator::ZeroRotator;
};

// Original catalog aircraft drawn for delivered support passes and shoot-downs, separate from sprite particles.
struct FTCFxAircraftDraw
{
 int32 Serial = 0, Owner = -1;
 FString Type;
 FVector P = FVector::ZeroVector; // Unreal axes, metres
 double Heading = 0, Roll = 0, Pitch = 0, PropAngle = 0, Scale = 1, ShadowScale = 1;
};

// What the sprite batch draws of one particle this frame: fx.js simulate's instance attributes (iPos, iCol, iMisc,
// iDir, iExt), in Unreal centimetres. The billboard itself is built per view by the sprite component.
struct FTCFxSpriteDraw
{
	FVector3f Center = FVector3f::ZeroVector; // cm
	float Size = 0.f;                         // cm, half the quad
	FVector3f Dir = FVector3f::ZeroVector;    // unit, when Len > 0 (a streak along the motion)
	float Len = 0.f;                          // cm
	float Rot = 0.f;                          // radians
	float Pull = 0.f;                         // cm toward the camera
	float Anchor = 0.f;                       // 1: stands on its point
	float GroundZ = 0.f;                      // cm
	uint8 CellA = 0, CellB = 0;
	float Blend = 0.f;                        // between the two flipbook cells
	FVector3f Colour = FVector3f(1.f);
	float Alpha = 0.f;
	float Mode = -1.f;                        // -1 lit, else the additive share
};

// A crater, burn mark or stain (fx.js decals): kind 0 crater, 1 burn mark, 2 stain.
struct FTCFxDecal
{
	bool bOn = false;
	float X = 0.f, Z = 0.f, R = 0.f, Rot = 0.f, Age = 0.f, Life = 0.f;
	uint8 Kind = 0;
	float Heights[TCFxRules::DecalGrid * TCFxRules::DecalGrid] = {}; // fitDecal: the ground + 5 cm at each grid point
	float Fade() const;                   // updateDecals: in over 0.3 s, out over the last 30 % of its life
};

// A projectile in flight at a frame (projectiles.js trail): position and velocity at the extrapolated time.
struct FTCFlightDraw
{
	int32 Flight = 0;
	FVector P = FVector::ZeroVector, V = FVector::ZeroVector; // metres, reference axes (x, y up, z) as delivered
	FString Kind, Effect;
};

// The projectile view (client/projectiles.js) on its own, so tests can compare it event by event with the browser.
class FTCProjectileView
{
public:
	struct FCallbacks
	{
		TFunction<void(const FTCFlightDraw&)> Trail;
		TFunction<void(int32 Flight)> Remove;
		TFunction<void(const FTCShot&)> Launch;
		TFunction<void(const FTCShot&)> Contact;
	};
	FCallbacks On;
	// clock: the client's seconds when the snapshot arrived.
	void Snapshot(const FTCMatchState& S, double Clock);
	void Frame(double Clock);
	void Reset();
	int32 Count() const { return Flights.Num(); }

private:
	bool Fresh(int32 Id, int32 Sequence);
	TMap<int32, FTCFlight> Flights;           // the rows held (projectiles.js flights)
	TArray<int32> FlightOrder;                // insertion order of Flights (a JavaScript Map's iteration order)
	TMap<int32, int32> Events;                // flight -> highest sequence seen
	TArray<int32> EventOrder;                 // first insertion order, for the 4096 trim
	double ServerTime = 0.0, Arrival = 0.0;
	int32 LastTick = -1;
};

// One delivered debris row (debris.js): a falling section, or a wreck's motion (wrecks[i][6]).
struct FTCDebrisRow
{
	FString Id, Kind, Material;
	bool bWreck = false;
	int32 WreckId = 0;
	double HullDir = 0.0;
	double X = 0.0, Y = 0.0, Z = 0.0, Tilt = 0.0, Dir = 0.0;
};

// fx.js updateSections: exact box pose after the Three.js to Unreal axis swap.
namespace TCFxSections
{
	FTransform Transform(const FTCDebrisRow& Row);
}

namespace TCFxLight
{
	FLinearColor Hemisphere(const FLinearColor& ColourTimesIntensity);
}

// The debris view (client/debris.js) on its own.
class FTCDebrisView
{
public:
	struct FCallbacks
	{
		TFunction<void(const FString& Id, const FTCDebrisRow& Row)> Pose;
		TFunction<void(const FString& Id)> Remove;
		TFunction<void(const FTCShot& Shot, const FTCShotExtra& Extra)> Contact;
	};
	FCallbacks On;
	// Wrecks: the snapshot's wreck rows as held (a snapshot without them keeps the last list, client/world-state.js).
	void Snapshot(const FTCMatchState& S, const TArray<FTCDebrisRow>& WreckMotion, const TArray<FTCShotExtra>& Extras, double Clock);
	void Frame(double Clock);
	void Reset();
	int32 Count() const { return Bodies.Num(); }

private:
	struct FBody { FTCDebrisRow Previous, Current; };
	TMap<FString, FBody> Bodies;
	TArray<FString> BodyOrder;
	TSet<FString> Events;
	TArray<FString> EventOrder;
	int32 LastTick = -1;
	double Arrival = 0.0, Duration = 0.1;
};

// The camera and ground the effects need from the presentation.
struct FTCFxContext
{
	TFunction<float(float X, float Z)> HeightAt;          // ground height in metres (0 when absent)
	TFunction<bool(int32 Id, FVector2D& Out)> DrawnAt;   // where a held unit is drawn (else its row)
	TFunction<TOptional<bool>(const FTCUnit&, int32, bool, FVector&)> WeaponMuzzle; // authoritative drawn model point, Unreal metres
	TFunction<void(const FString&, double, double, double, double)> TimedSound; // name, map x/z, delay, duration
	const FTCUnitTable* Table = nullptr;
	FVector2D CameraFocus = FVector2D::ZeroVector;        // metres, for the shake's distance falloff
	float CameraDistance = 60.f;
	bool bReduceMotion = false;
};

// The dispatch and the effects' own simulation: no engine objects, so tests drive it directly.
class FTCCombatEffects
{
public:
	FTCCombatEffects();
	FTCCombatEffects(const FTCCombatEffects&) = delete;             // the views' callbacks hold this object
	FTCCombatEffects& operator=(const FTCCombatEffects&) = delete;
	// A start: everything from the last match goes (client/main.js startGame: projectiles, debris and effects reset).
	void Start(const FTCMatchState& S);
	void Reset();
	// After FTCMatchState::ApplySnapshot. Raw: the same snapshot message (for FTCShotExtras, smokes and wrecks).
	void Snapshot(const FTCMatchState& S, const FJsonObject* Raw, const FTCFxContext& Ctx, double Clock);
	// Every frame: the projectile and debris views, delayed events, emitters and particles.
	void Frame(const FTCFxContext& Ctx, double Clock, float Dt);

	const TArray<FTCFxParticle>& Particles() const { return Live; }
	const TArray<FTCFlightDraw>& FlightsNow() const { return FlightDraws; }
	const TArray<FTCFxAircraftDraw>& AircraftNow() const { return AircraftDraws; }
	// fx.js simulate's draw pass: every sprite out now (held-back ones are not), unsorted (the view sorts them)
	void SpriteDraws(TArray<FTCFxSpriteDraw>& Out) const;
	const TArray<FTCFxDecal>& Decals() const { return DecalPool; }
	int32 DecalsOn(int32 Kind = -1) const;
	// the ground of a decal changed (dug, bombed): fx.js refits every 2 s
	void RefitDecals();
	const TMap<FString, FTCDebrisRow>& Sections() const { return SectionPoses; }
	// fx.js shake (metres of camera jitter), decaying.
	float Shake() const { return ShakeAmount; }
	FVector ShakeOffset(double Clock) const;
	// Sound names the reference would play now (no audio in this client: logged or ignored by the caller).
	TArray<FString> TakeSounds() { TArray<FString> Out = MoveTemp(Sounds); Sounds.Reset(); return Out; }
	FVector2D Wind() const { return WindXZ; }
	struct FCoverRing { double X, Z, Radius, Seconds; double Opacity() const { return FMath::Min(.85, .4 + Seconds / 150); } };
	const TArray<FCoverRing>& CoverRings() const { return Covers; }
	// Positions already resolved for this snapshot. Audio reuses them without advancing model barrel alternation.
	bool SnapshotMuzzle(int32 Id, int32 Index, FVector& Out) const
	{
		const FVector* P = SnapshotMuzzles.Find((int64(Id) << 32) | uint32(Index));
		if (!P) return false; Out = *P; return true;
	}

	// Counters for logs and tests, since this object was made (a start or reset does not clear them).
	struct FStats
	{
		int32 Launches = 0, LaunchesHidden = 0, Contacts = 0, DirectFire = 0, DirectFireHidden = 0, Blasts = 0, Wrecks = 0;
		int32 Throws = 0, Salvos = 0, AirShots = 0, Collapses = 0, Dropped = 0, ShooterNotHeld = 0;
	};
	const FStats& Stats() const { return Counters; }
	// Shooter-side effects made this match, by the shooter's id (tests check each against the snapshot's held rows).
	const TArray<TPair<int32, int32>>& ShooterLog() const { return ShooterEffects; } // (tick, shooter id)
	// Lead glue (slice 11): held units a delivered kill shot named that have now left the snapshot (main.js removeUnit
	// with v.killed), since the last call. The unit renderer drops a killed squad's men as bodies (UTCSoldierCrowd).
	TArray<int32> TakeKilledGone() { TArray<int32> Out = MoveTemp(KilledGone); KilledGone.Reset(); return Out; }
	// fx.js blastNear: one of the last 16 blasts, at most 0.6 s old on the effects' clock, within its size + 0.5 m of
	// (X, Z) (reference metres): its centre and size. unit-models.js createBodies throws a fresh body away from it.
	bool BlastNear(double X, double Z, double& OutX, double& OutZ, double& OutSize) const;
	// fx.js gore: a man torn by a blast (K, 0 to 1, how close he was): scraps, a red mist and a stain on the ground.
	// gfx.js gore is on by default; the menu's Gore switch (Client/TCGraphics.h) turns it off (round 14).
	void Gore(double X, double Z, double K = 1.0);
	// fx.js vehicleDamage: one puff of grey smoke from a damaged vehicle's engine deck (At: Unreal axes, metres)
	void VehicleDamage(const FVector& At, double Level);
	// round 14 (lead glue): fx.js plume(kind, x, y, z, radius), a roof-sized burst from one damaged building; 'smoke',
	// 'dark' or 'fire', reference metres (x, y up, z). objectives.js owns its cadence (FTCScenarioText::Frame).
	void Plume(const FString& Kind, double X, double Y, double Z, double Radius = 3.0);
	// engine-presentation.js vehicleDamageFrame for one ground vehicle this frame: Health = hp / (models * hpPer),
	// bKilled main.js v.killed, bDetailed animationInterest (root visible and its 25 m ball in view). Anchor gives the
	// engine deck point (unit-models.js vehicleDamageAnchor, Unreal axes in metres) when a puff is due.
	void VehicleDamageFrame(int32 Id, double Health, bool bKilled, bool bDetailed, float Dt, TFunctionRef<bool(FVector& OutAt)> Anchor);
	// aircraft.js tick and fx.js air: smoke behind the tail below 60 %, black smoke and nose fire below 30 %.
	void PlaneDamageFrame(int32 Id, double Health, bool bVisible, float Dt, const FVector& RootMetres, double Rot, double Tail, double Nose);
	// the damage cues of units that are gone
	void RetainDamage(const TSet<int32>& Present);
	bool WasKilled(int32 Id) const { return Killed.Contains(Id); }
	int32 DamagePuffs() const { return DamagePuffCount; }
	// lead glue (slice 12): the eased damage smoke level of a vehicle (0 none, 1 light, 2 heavy), for the combat shot
	double DamageLevel(int32 Id) const { const FDamageCue* C = DamageCues.Find(Id); return C ? C->Level : 0.0; }
	int32 BlastTosses() const { return TossCount; }
	void NoteToss() { TossCount++; }

	FTCProjectileView Projectiles;
	FTCDebrisView Debris;
	// debris.js poses for hulks, consumed by the world-object renderer along with section poses here.
	const FTCDebrisRow* WreckPose(int32 WreckId) const;
	int32 Seed = 0x7c2a;

private:
	TMap<FString, FTCDebrisRow> WreckPoses;
	TMap<int32, double> PlaneSmokeWait;
	static constexpr double Tau = 2.0 * UE_DOUBLE_PI;
	struct FStyle;
	static const FStyle& StyleOf(const FString& Kind);
	static const FStyle& Bazooka();

	// emission (fx.js emit and its helpers tint, hold, stretch)
	int32 Emit(ETCFxPreset Preset, const FVector& P, const FVector& V, float Size, float Life);
	int32 Shape(ETCFxLook Look, const FVector& P, const FVector& V, float Size, float Life); // a placeholder shape
	FTCFxParticle* Last();
	void Tint(int32 I, float K);
	void Hold(int32 I, float D);
	void Stretch(int32 I, float Len);
	float Rr(float A, float B) { return A + (B - A) * Rng.FRand(); }
	float Ground(float X, float Z) const;
	FVector At(double X, double Y, double Z) const { return FVector(X, Z, Y); } // reference (x, y up, z) -> Unreal axes
	void Scorch(double X, double Z, double R, uint8 Kind = 0);
	void FitDecal(FTCFxDecal& D) const;

	// fx.js pieces
	void Explode(double X, double Z, double Size, bool bDecal = true, bool bDirt = true, ETCFxPreset Debris = ETCFxPreset::Clod, double SmokeK = 1.0, const double* Y = nullptr, double Fire = 1.0);
	void Muzzle(const FVector& P, double Dx, double Dz, double S, const FStyle& St);
	void ShotTracer(const FVector& From, const FVector& To, const FStyle& St, int32 Flags);
	void SkyShot(const FVector& From, const FVector& To, const FStyle& St);
	void Burst(const FVector& P, double S = 1.0);
	void Impact(const FVector& P, const FStyle& St, int32 Flags);
	void StrafeHit(double X, double Z);
	void ProjectileTrail(const FTCFlightDraw& D);
	void ProjectileRemove(int32 Flight);
	void PuffCloud(int32 Id, float X, float Z, float R, bool bFire, bool bBurst);
	void Kick(double X, double Z, double Size);
	void Column(const FVector& P, double S, double Dur);
	void Collapse(double X, double Z, double R);
	void Burning(const FVector& P, double Big, double Fire, double Dur);
	void Wreck(const FTCUnit& U);
	void WorldContact(const FTCShot& Sh, const FTCShotExtra& Ex);
	void LaunchRocket(const FVector& From, const FVector& To, double T, double H);
	void DirectFire(const FTCShot& Sh, const FTCUnit* From, const FTCUnit* SeenTarget);
	void Salvo(const FTCShot& Sh, const FTCShotExtra& Ex, const FTCUnit* From);
	void AntiAir(const FTCShot& Sh, const FTCUnit* From, const FTCUnit* To, bool bBursts);
	void AirShot(const FTCShot& Sh, const FTCShotExtra& Ex, const FTCUnit* From, const FTCUnit* To);
	void SyncFires(const FTCMatchState& S);
	void SyncClouds(const FJsonObject* Raw);
	void UnitDust(const FTCMatchState& S);
	bool MuzzleOf(const FTCUnit& U, int32 K, double Tx, double Tz, FVector& Out, bool bBazooka = false);
	void NoteShooter(int32 Id);
	void Later(double Delay, TFunction<void()> Run);
	void Sound(const TCHAR* Name) { if (Sounds.Num() < 256) Sounds.Add(Name); }

	// per-frame pieces
	void RunEvents(float Dt);
	void UpdateEmitters(float Dt);
	void Simulate(float Dt);
	void UpdateDecals(float Dt);

	FRandomStream Rng;
	TArray<FTCFxParticle> Live;
	TArray<FTCFxDecal> DecalPool;
	float RefitT = 0.f;
	int32 SimTick = 0;
	TArray<FTCFlightDraw> FlightDraws;
	TMap<FString, FTCDebrisRow> SectionPoses;
	struct FEvent { double At; TFunction<void()> Run; };
	TArray<FEvent> Events;
	struct FFire { float X, Y, Z; TCHAR Kind; float Acc, Sa; bool bKeep; };
	TMap<int32, FFire> Fires;
	struct FWreckFire { FVector P; float T, Fire, Dur, Fa, Sa, Ea, Big; };
	TArray<FWreckFire> WreckFires;
	struct FColumn { FVector P; float S, T, Dur, Acc; };
	TArray<FColumn> Columns;
	struct FCloud { int32 Id; float X, Z, R, Acc; bool bKeep, bFire; };
	TMap<FString, FCloud> Clouds;
	int32 CloudSerial = 0;
	struct FRocket { FVector P0, V; float G, T, Dur, Acc; };
	TArray<FRocket> Rockets;
	struct FPass { FVector2D At; double Dir; FString Kind; float T; int32 Owner; TArray<float> Strafe; TArray<float> Flak; bool bDown = false; int32 Serial = 0; double HitAt = 0; bool bOpened = false; };
	TArray<FPass> Passes;
	TArray<FCoverRing> Covers;
	TMap<int32, int32> FixedGunSide;
	TMap<int64, FVector> SnapshotMuzzles;
	struct FFall { TCRender29Aircraft::FFall Motion; FString ModelType; int32 Owner = -1, Serial = 0; double Acc = 0; };
	TArray<FFall> Falls;
	TArray<FTCFxAircraftDraw> AircraftDraws;
	int32 NextAircraftSerial = 1;
	void BeginFall(const FVector& P, double Heading, double Speed, double Vy, const FString& Type, int32 Owner, double Pitch = 0, int32 Serial = 0);
	void DrawFall(const FFall& F, float Dt);
	struct FThrow { double X, Z; bool bSatchel; double T; };
	TArray<FThrow> Throws;
	struct FSalvoMark { double X, Z, R; bool bMortar; double T; };
	TArray<FSalvoMark> SalvoMarks;
	TMap<int32, double> LastBurst;
	TMap<int32, FTCUnit> PrevHeld;        // the rows held after the last snapshot (removeUnit's v)
	TSet<int32> Killed;                   // held units a delivered kill shot named (main.js v.killed)
	TArray<int32> KilledGone;             // of those, the ones that left the snapshot (TakeKilledGone)
	TArray<FTCDebrisRow> HeldWreckMotion; // wrecks carried over (world-state.js s.wrecks ??= last.wrecks)
	TArray<FString> Sounds;
	TArray<TPair<int32, int32>> ShooterEffects;
	FStats Counters;
	FTCFxContext Ctx_;                    // the context of the call in progress
	const TArray<FTCShotExtra>* CurrentExtras = nullptr; // this snapshot's extras while it is dispatched
	const FTCMatchState* State_ = nullptr;
	double ClockNow = 0.0;
	float ShakeAmount = 0.f;
	FVector2D WindXZ = FVector2D(0.55, 0.25);
	int32 TickNow = 0;
	float FrameDt = 0.f;                  // the step of the frame in progress (Frame), for the projectile streaks
	// fx.js blasts: the last 16 explosions (x, z metres, size, the effects' clock)
	struct FBlastMark { double X = 0, Z = 0, S = 0, T = -1e9; };
	FBlastMark Blasts[16];
	int32 BlastNext = 0;
	// engine-presentation.js damageCue per vehicle
	struct FDamageCue { int32 Stage = 0; double Level = 0, Wait = 0; };
	TMap<int32, FDamageCue> DamageCues;
	int32 DamagePuffCount = 0, TossCount = 0;
};

// ---------------------------------------------------------------------------------------------------------------------
// The presentation. Two primitive components draw what the views above compute, with the reference's pictures:
//   UTCFxSpriteComponent  every sprite, one batch per view: billboards turned to that view (fx.js VERT: plain, turned,
//                         stretched along the motion, standing on their point, pulled toward the camera), sorted back
//                         to front (glowing ones 1.5 m nearer, as fx.js sorts them), drawn with M_TCFxSprite;
//   UTCFxGroundComponent  triangles laid on the ground: the crater decals (M_TCFxCrater) and the rubble decals
//                         (M_TCFxRubble), one component each.
// The game thread hands each a new frame (a shared, immutable list); the render thread draws the latest one.

struct FTCFxGroundVertex
{
	FVector3f P = FVector3f::ZeroVector; // cm
	FVector2f Uv = FVector2f::ZeroVector;
	FVector2f Extra = FVector2f::ZeroVector; // craters: (fade, kind)
};
struct FTCFxGroundFrame
{
	TArray<FTCFxGroundVertex> Vertices;
	TArray<uint32> Indices;
};
struct FTCFxSpriteFrame
{
	TArray<FTCFxSpriteDraw> Sprites;
};

UCLASS()
class THREECROSSROADS_API UTCFxSpriteComponent : public UPrimitiveComponent
{
	GENERATED_BODY()
public:
	UTCFxSpriteComponent();
	void Publish(TArray<FTCFxSpriteDraw>&& Sprites);
	void SetFxMaterial(UMaterialInterface* M);
	int32 Published() const { return LastCount; }
	virtual FPrimitiveSceneProxy* CreateSceneProxy() override;
	virtual FBoxSphereBounds CalcBounds(const FTransform& LocalToWorld) const override;
	virtual void GetUsedMaterials(TArray<UMaterialInterface*>& OutMaterials, bool bGetDebugMaterials = false) const override;
	virtual int32 GetNumMaterials() const override { return 1; }
	virtual UMaterialInterface* GetMaterial(int32 ElementIndex) const override { return FxMaterial; }

	struct FMailbox
	{
		FCriticalSection Lock;
		TSharedPtr<const FTCFxSpriteFrame, ESPMode::ThreadSafe> Latest;
	};
private:
	UPROPERTY() TObjectPtr<UMaterialInterface> FxMaterial;
	TSharedPtr<FMailbox, ESPMode::ThreadSafe> Mail;
	int32 LastCount = 0;
};

UCLASS()
class THREECROSSROADS_API UTCFxGroundComponent : public UPrimitiveComponent
{
	GENERATED_BODY()
public:
	UTCFxGroundComponent();
	void Publish(FTCFxGroundFrame&& Frame);
	void SetFxMaterial(UMaterialInterface* M);
	int32 Published() const { return LastCount; }
	virtual FPrimitiveSceneProxy* CreateSceneProxy() override;
	virtual FBoxSphereBounds CalcBounds(const FTransform& LocalToWorld) const override;
	virtual void GetUsedMaterials(TArray<UMaterialInterface*>& OutMaterials, bool bGetDebugMaterials = false) const override;
	virtual int32 GetNumMaterials() const override { return 1; }
	virtual UMaterialInterface* GetMaterial(int32 ElementIndex) const override { return FxMaterial; }

	struct FMailbox
	{
		FCriticalSection Lock;
		TSharedPtr<const FTCFxGroundFrame, ESPMode::ThreadSafe> Latest;
	};
private:
	UPROPERTY() TObjectPtr<UMaterialInterface> FxMaterial;
	TSharedPtr<FMailbox, ESPMode::ThreadSafe> Mail;
	int32 LastCount = 0;
};

// The effects actor: feeds FTCCombatEffects, draws its sprites and decals, the known rubble cells, the placeholder
// shapes and the map pings' rings (FTCAlerts), and shakes the camera through UTCEffectsShake. Spawned once by the game
// mode; it reads the state it is handed and nothing else.
UCLASS()
class THREECROSSROADS_API ATCEffects : public AActor
{
	GENERATED_BODY()

public:
	ATCEffects();
	virtual void Tick(float DeltaSeconds) override;

	// Message hooks (after FTCMatchState applied the message).
	void OnStart(const FTCMatchState& S);
	void OnSnapshot(const FTCMatchState& S, const FJsonObject& Raw);
	void Reset();
	// Where to read the ground, the drawn units, the camera and the ping rings from (all optional).
	void Bind(const FTCMatchState* InState, const FTCUnitTable* InTable, ATCTerrain* InTerrain, ATCUnitRenderer* InUnits, FTCAlerts* InAlerts);

	FTCCombatEffects Fx;
	// Draws what Fx holds now (Tick does this every frame; the effects shot commandlet drives it with its own clock).
	void Present(double Clock) { Draw(Clock); }
	// sprites, decal and rubble cells, and shape instances drawn in the last frame
	int32 InstancesDrawn() const { return LastInstances; }
	int32 SpritesDrawn() const { return LastSprites; }
	int32 AircraftInstancesDrawn() const { return LastAircraftInstances; }
	int32 RubbleCells() const { return RubbleList.Num(); }
	// the effect materials were found (Content/Effects, unreal/Tools/effects)
	bool HasLook() const { return SpriteMaterial != nullptr && CraterMaterial != nullptr; }

private:
	FTCFxContext MakeContext() const;
	int32 AudioAuditionIndex = 0;
	double AudioAuditionNext = 0;
	UInstancedStaticMeshComponent* Batch(int32 Key, UStaticMesh* Mesh, const FLinearColor& Colour, bool bUnlit);
	void EnsureComponents();
	void UpdateLight();
	void SyncRubble();
	void Draw(double Clock);
	int32 DrawAircraftModels();
	FTCModelLibrary AircraftLibrary;
	FTCInstanceHistory AircraftHistory;
	bool bAircraftLibraryTried = false;
	int32 LastAircraftInstances = 0;
	UPROPERTY() TMap<FString, TObjectPtr<UInstancedStaticMeshComponent>> AircraftBatches;
	void ApplyShake(double Clock);
	void CaptureBridge(double Clock);
	// main.js frame loop: vehicleDamageFrame for every held ground vehicle (engine-presentation.js)
	void DamageFrame(float Dt);

	const FTCMatchState* State = nullptr;
	const FTCUnitTable* Table = nullptr;
	FTCAlerts* Alerts = nullptr;           // its lines and ping rings age every frame here
	TWeakObjectPtr<ATCTerrain> Terrain;
	TWeakObjectPtr<ATCUnitRenderer> UnitView;
	UPROPERTY() TMap<int32, TObjectPtr<UInstancedStaticMeshComponent>> Batches;
	UPROPERTY() TObjectPtr<UTCFxSpriteComponent> SpriteBatch;
	UPROPERTY() TObjectPtr<UTCFxGroundComponent> CraterBatch;
	UPROPERTY() TObjectPtr<UTCFxGroundComponent> RubbleBatch;
	UPROPERTY() TObjectPtr<UStaticMesh> SphereMesh;
	UPROPERTY() TObjectPtr<UStaticMesh> CubeMesh;
	UPROPERTY() TObjectPtr<UStaticMesh> CylinderMesh;
	UPROPERTY() TObjectPtr<UStaticMesh> ConeMesh;
	UPROPERTY() TObjectPtr<UMaterialInterface> BaseMaterial;
	UPROPERTY() TObjectPtr<UMaterialInterface> SectionMaterial;
	UPROPERTY() TObjectPtr<UMaterialInterface> SpriteMaterial;
	UPROPERTY() TObjectPtr<UMaterialInterface> CraterMaterial;
	UPROPERTY() TObjectPtr<UMaterialInterface> RubbleMaterial;
	UPROPERTY() TObjectPtr<class UMaterialInstanceDynamic> SpriteLight;
	UPROPERTY() TObjectPtr<class UTCEffectsShake> ShakeModifier;
	TArray<int32> RubbleList;              // rubble-decals.js cells, in slot order
	uint32 RubbleKey = 0;                  // the map cells the list was made from
	double LastLightAt = -1.0;
	int32 LastInstances = 0, LastSprites = 0;
	double LastSoundLog = 0.0;
	bool bWarnedLook = false;
	FString BridgeShotPrefix, BridgeView;
	double BridgeSeen = -1.0, BridgeCollapse = -1.0;
	int32 BridgeShot = 0;
	bool bBridgeCaptureRead = false;
};

// fx.js update: the camera jitters by the effects' shake (metres; nothing below 0.004, as the browser).
UCLASS()
class THREECROSSROADS_API UTCEffectsShake : public UCameraModifier
{
	GENERATED_BODY()

public:
	FVector Offset = FVector::ZeroVector; // centimetres, set each frame by ATCEffects
	virtual bool ModifyCamera(float DeltaTime, struct FMinimalViewInfo& InOutPOV) override;
};

// Pictures of the effects as the game draws them, for review against the browser: a scratch world with the reference's
// default light (as the unit-model comparison sets it), the units drawn by ATCUnitRenderer with their models, and
// scripted snapshots fed through FTCMatchState and the effects' own hooks, on the commandlet's clock.
//   UnrealEditor-Cmd ThreeCrossroads.uproject -run=TCEffectsShot -Scenes=<scenes.json> -Out=<dir>
//       -AllowCommandletRendering -RenderOffscreen
// scenes.json is unreal/Tools/effects/compare/scenes.json, which the browser half (fx-compare.mjs) plays through the
// pinned client/fx.js. Writes <dir>/<scene>-<moment>-unreal.png and <dir>/effects-shots.json (what was alive).
UCLASS()
class THREECROSSROADS_API UTCEffectsShotCommandlet : public UCommandlet
{
	GENERATED_BODY()
public:
	UTCEffectsShotCommandlet();
	virtual int32 Main(const FString& Params) override;
};
