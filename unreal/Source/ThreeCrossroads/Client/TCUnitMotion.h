// Unit motion: the pinned browser client's presentation-only movement of unit models, ported for ATCUnitRenderer.
//   client/model-motion.js   moveModel: the hull settling on the move (pitch on acceleration and the track's rhythm,
//                            roll in turns) for land vehicles, and the swell for boats (pitch, roll, heave)
//   client/wheel-motion.js   moveWheels and wheelAngle: the ground the wheels rolled and the hull's turn, which turn
//                            the authored wheels about their axles
//   client/track-motion.js   the track path table and trackVertex: track links run round their loop by that travel
//   client/unit-models.js    stride: the kaiju's legs, bob, sway and tail; createBodies: the corpses (cap, life, fade,
//                            sink, the toss beside a blast, the near/far swap)
//   client/aircraft.js       tick: a plane's bank into turns and its wobble, the propellers' spin
//   client/main.js           corpse(): a man who falls lies where he stood, facing as he faced
// Soldiers themselves are TCInfantryRig (the pose solver, the crowd skinning and animate() for a squad); this file
// also draws them (UTCSoldierCrowd): near men skinned from the solver's keyframes, far men from the reference's own
// far figure and its morph targets (Content/Models/Motion/FarFigures.bin, unreal/Tools/motion/export-far-figures.mjs),
// camouflaged men dithered to the reference's 0.45, and the corpses with their fade.
//
// The arithmetic lives in namespace TCUnitMotionCore (TCUnitMotion.cpp), in doubles and three.js axes, compiled on
// its own (TC_UNIT_MOTION_STANDALONE) by the Node oracle in unreal/Tools/motion/oracle.
// Axes: reference (x, y, z) metres -> Unreal (x, z, y) * 100. A three.js turn of a about axis (ax, ay, az) is an Unreal
// FQuat about (ax, az, ay) by -a (the axis swap is a reflection), so rotation.y = -rot is an Unreal yaw of rot.
#pragma once

#include "CoreMinimal.h"
#include "Components/SceneComponent.h"
#include "Client/TCInfantryRig.h"
#include "Client/TCModelLibrary.h"
#include "TCUnitMotion.generated.h"

struct FTCModelEntry;
struct FTCAircraftSpec
{
	double Nose = 0, Tail = 0, Ext = 0;
	FSoftObjectPath Shadow;
};
class UInstancedStaticMeshComponent;
class UMaterialInterface;
class UMaterialInstanceDynamic;
class UStaticMesh;
class UTexture2D;

// ---------------------------------------------------------------------------------------------------------------------
// Vehicles, boats, the kaiju and planes

// What one frame of motion does to a unit's parts.
struct THREECROSSROADS_API FTCPartMotion
{
	// Between the unit root and every part hung on the root: model-motion.js visualBody (hull pitch, roll, heave) or
	// aircraft.js's plane body (bank and wobble). Identity for everything else.
	FTransform Body = FTransform::Identity;
	// Moves of named parts in their parent's frame: the rotation turns the part about its own pivot, the translation
	// shifts its pivot. The kaiju's hull, head (turret), tail and legs; a plane's propellers (prop_N).
	TMap<FName, FTransform> Parts;
	// wheel-motion.js: the ground rolled (m, signed) and the hull's turn (rad) since the unit was first drawn: the
	// two floats of the reference's per-instance wheelTravel attribute.
	double WheelTravel = 0, WheelTurn = 0;
	double Heading = 0;
	// unit-models.js vehicleDamageAnchor: the rear engine deck point above the hull (the hull's bounds without the
	// turret and mounts: 15 % in from the back, 18 cm over the top, mid width), carried by the hull motion, in the
	// world (cm). Set by the renderer each frame it draws the unit; the damage smoke (TCEffects) rises from it.
	bool bDamageAnchor = false;
	FVector DamageAnchor = FVector::ZeroVector;
};

// The motion state of every unit drawn with a model (not squads, which TCInfantryRig animates). Plain C++, kept by
// the renderer between frames.
class THREECROSSROADS_API FTCUnitMotion
{
public:
	FTCUnitMotion();
	~FTCUnitMotion();
	FTCUnitMotion(const FTCUnitMotion&) = delete;
	FTCUnitMotion& operator=(const FTCUnitMotion&) = delete;

	// One frame for one unit, after its root is placed as main.js places it: Root is the root's world position (cm;
	// Z the ground or water under it, plus the flight altitude for planes), Rot the eased heading (the snapshot's rot;
	// three's rotation.y is -Rot), bVisible root.visible (false while riding or parked), Dt the frame's seconds.
	// Now is a running clock in seconds (aircraft.js's wobble reads performance.now()).
	const FTCPartMotion& Update(int32 Id, const FString& Type, const FVector& Root, double Rot, bool bVisible, float Dt, double Now);
	const FTCPartMotion* Find(int32 Id) const;
	// The damage anchor of a unit drawn this frame (FTCPartMotion::DamageAnchor), world cm.
	void SetDamageAnchor(int32 Id, const FVector& WorldCm);
	// vehicleDamageAnchor's cached local point (v.damageAnchorLocal), per model (unit space, cm, at rest)
	TMap<FString, FVector> LocalDamageAnchors;
	// Drop the state of units that are gone (a reused id starts afresh).
	void Forget(int32 Id);
	void Retain(const TSet<int32>& Present);
	void Reset();

	// Which motions a type gets (client/model-motion.js LAND and SEA, unit-models.js kaiju, aircraft.js planes).
	static bool HasHullMotion(const FString& Type);
	static bool IsSea(const FString& Type);
	static bool IsPlane(const FString& Type);

	// FTCModelLibrary::PartTransform with the motion applied: a root part sits on Body, every part takes its own move
	// (Parts) before its turn (Turns: the turret's traverse), and children inherit both, as three.js nests them.
	static FTransform PartTransform(const FTCModelEntry& Entry, int32 PartIndex, const FTransform& Unit, const TMap<FName, FQuat>& Turns, const FTCPartMotion& Motion);

	// wheel-motion.js wheelAngle: how far a wheel of radius Radius (m) has turned about its local z axle; Side is the
	// track side it rolls with (wheelSide, else its pivot's z). Reference sign (three's rotation about +z).
	static double WheelAngle(double Radius, double Side, double Travel, double Turn);
	// unit-models.js deckSeats: source hull vertices in body-space centimetres, excluding turret and mounts.
	static TArray<FTransform> RiderSeats(const TArray<FVector>& HullVertices);
	static bool HasDeckRiders(const FString& Type);
	// aircraft.js tick: advances smokeT only while visible and hurt, returns 0, light smoke (1) or heavy/fire (2).
	static int32 PlaneDamage(double& Wait, double Health, bool bVisible, bool bLow, double Dt);
	// Reproducible source builders' dimensions, shadow canvases and body-space vertices (Tools/motion/export-aircraft).
	static const FTCAircraftSpec* AircraftSpec(const FString& Type, int32 Faction);
	static const TArray<FTransform>& DeckFor(const FString& Type, int32 Faction);

private:
	struct FState;
	TMap<int32, FState*> States;
};

// client/track-motion.js: the native track path table. Each distinct track loop (a closed polyline in the hull's xy
// plane, metres) is sampled at 1024 points by arc length into one row of a float table; a track link vertex then runs
// round its loop by the wheels' travel. CPU readback here; the texture (1024 x 128 RGBA32F, the reference's layout)
// is what a material would read (unreal/Tools/motion/wheel-track.hlsl).
class THREECROSSROADS_API FTCTrackPaths
{
public:
	static constexpr int32 Samples = 1024, Rows = 128;
	FTCTrackPaths();
	// register(loop): the row for this loop (the same points reuse their row), INDEX_NONE when the table is full.
	int32 Register(const TArray<FVector2D>& Points);
	int32 Num() const { return Lengths.Num(); }
	double LengthOf(int32 Row) const { return Lengths.IsValidIndex(Row) ? Lengths[Row] : 0.0; }
	// frame(id, phase): the point on the loop and its unit direction (x, y, dx, dy)
	FVector4 Frame(int32 Row, double Phase) const;
	// trackVertex: where a vertex at Position (hull metres) with trackData (phase, row, side, floor) is drawn after
	// Travel and Turn. A vertex with row < 0 does not move.
	FVector TrackVertex(const FVector& Position, const FVector4& TrackData, double Travel, double Turn) const;
	// The table as a texture for a material (no mips, nearest, linear).
	UTexture2D* CreateTexture() const;
	const TArray<float>& Pixels() const { return Table; }

private:
	TArray<float> Table; // Samples * Rows * 4, float32 as the reference's Float32Array
	TArray<double> Lengths;
	TArray<TArray<FVector2D>> Keys;
};

// The tracked hulls drawn with wheel spin and track scroll (client/wheel-motion.js wheelMesh and wheelMaterial,
// client/track-motion.js): unreal/Tools/motion/make-wheel-tracks.py writes Content/Models/Motion/WheelTracks.json and,
// per catalog hull with wheels, a copy of that hull carrying the reference's wheel and track data in UV4-7 on the
// model master M_TCModelTracks (world position offset and the turned normal). Here: which catalog mesh has such a
// copy, the loops registered in the rows the tool gave them (the shared path table, the reference's 1024 x 128
// RGBA32F layout) and that table as a texture for the material's TrackPaths parameter. Per instance the hull carries
// FTCPartMotion WheelTravel and WheelTurn in custom data 8 and 9 (after the catalog's 8 floats).
class THREECROSSROADS_API FTCWheelTracks
{
public:
	static constexpr int32 TravelFloat = 8, TurnFloat = 9, Floats = 10;
	static const TCHAR* const TextureParameter; // "TrackPaths"
	struct FHull
	{
		FSoftObjectPath Mesh;               // /Game/Models/Motion/Tracks/SM_<n>_hull_Tracks
		TArray<FSoftObjectPath> Materials;  // MI_TCModelTracks_* by slot
		TArray<int32> Loops;                // the rows of its own loops
		int32 WheelVertices = 0, LinkVertices = 0;
	};
	// Reads the file (default Content/Models/Motion/WheelTracks.json) and registers its loops; false (with the reason)
	// when it is missing or a loop does not land in its row.
	bool Load(FString& OutError, const FString& File = FString());
	bool IsLoaded() const { return bLoaded; }
	const FHull* Find(const FSoftObjectPath& CatalogMesh) const { return Hulls.Find(CatalogMesh.ToString()); }
	int32 NumHulls() const { return Hulls.Num(); }
	const FTCTrackPaths& Paths() const { return Table; }
	// The table as a texture (made once; the caller keeps it referenced).
	UTexture2D* Texture();

private:
	bool bLoaded = false;
	TMap<FString, FHull> Hulls;
	FTCTrackPaths Table;
	TWeakObjectPtr<UTexture2D> Made;
};

// ---------------------------------------------------------------------------------------------------------------------
// Soldiers drawn: near, far, camouflaged, fallen

// One man to draw this frame.
struct THREECROSSROADS_API FTCSoldierDraw
{
	FString FigureType;            // the figure (FTCModelLibrary::FigureType: medics wear the engineer figure)
	int32 Faction = 0;             // his own faction (the kit he wears)
	int32 Slot = 0;                // his place in the squad: picks his kit (UTCInfantryRigLibrary::SoldierKit)
	FTransform Man;                // where he stands (FTCInfantrySquadAnimation::ManTransform), scale included
	FTCInfantryBlend Blend;        // his keyframe weights and pose node (FTCInfantrySquadAnimation::Men)
	FLinearColor Owner = FLinearColor::White; // the owner's seat color, linear (the far figure's arm band)
	float Fade = 1.f;              // 1 solid; 0.45 while camouflaged (main.js flags & 256)
	bool bFar = false;             // the squad's level of detail (FTCInfantrySquadAnimation::IsFar)
	int32 UnitId = 0;              // with ManIndex: lets the man fall where he stood when the squad loses him
	int32 ManIndex = INDEX_NONE;
	bool bDeckRider = false;       // hull passengers do not create squad casualty records
};

// unit-models.js CORPSES
struct FTCCorpseRules
{
	int32 Cap = 200, Soft = 184;
	float Life = 25.f, Fade = 1.5f, Sink = 0.45f;
};

struct FTCSoldierCrowdStats
{
	int32 NearMen = 0, FarMen = 0, FadedMen = 0;
	int32 Corpses = 0, NearCorpses = 0, FarCorpses = 0;
	int32 NearFigures = 0, FarFigures = 0, Components = 0;
	int32 Refused = 0; // AddMan calls without a figure (no kit, no far figure data, a missing asset)
};

// Draws soldiers as instanced crowds. Attach it to the renderer actor and register it; per frame call BeginFrame,
// AddMan for every living man, SquadAlive for every squad, SquadGone for squads that left, then EndFrame.
UCLASS()
class THREECROSSROADS_API UTCSoldierCrowd : public USceneComponent
{
	GENERATED_BODY()

public:
	UTCSoldierCrowd();

	// Reads Content/Models/Infantry/Source/kits.json and Content/Models/Motion/FarFigures.bin and loads the materials.
	// Near men need the first, far men the second; either may be missing (AddMan then refuses those men).
	bool Load(FString& OutError);
	bool HasNear() const { return Kits.Num() > 0; }
	bool HasFar() const { return FarData.Num() > 0; }
	// Near men drawn in the model masters' look (unreal/Tools/motion/make-crowd-look.py), not the atlas-only materials.
	bool HasModelLook() const { return bModelLook; }

	void BeginFrame();
	// False when the man has no figure here: the caller draws its own fallback (the catalog's rest figure).
	bool AddMan(const FTCSoldierDraw& Man);
	// main.js: the men past the squad's new alive count fall where they stood (the last AddMan of each). The first call
	// for a squad only records its count. Reinforcements (a higher count) come back without bodies.
	void SquadAlive(int32 UnitId, int32 Alive);
	// A squad left the snapshot; when it was killed (main.js removeUnit, v.killed) its men still standing fall.
	void SquadGone(int32 UnitId, bool bKilled);
	// unit-models.js bodies.add: a body without a squad (a man given by figure, place and facing).
	void AddCorpse(const FString& FigureType, int32 Faction, int32 Slot, const FLinearColor& Owner, const FVector& Where, double Yaw, float Scale);
	// Ages the bodies, picks near or far for each by its own distance, writes every instance. GroundCm: the ground
	// height (cm) at a world point (cm). BlastNear (optional): fx.blastNear, a blast within reach of (x, z) metres in
	// the last moments, giving its centre and size; a fresh body beside one is thrown and tumbles.
	void EndFrame(float Dt, const FVector& Eye, TFunctionRef<double(double X, double Y)> GroundCm, float LodMetres = 110.f,
		TFunction<bool(double X, double Z, double& OutX, double& OutZ, double& OutSize)> BlastNear = nullptr);
	void ClearCorpses();

	// Verification: every far figure's keyframe matrices applied to its standing build, against the exported morph
	// targets (positions in cm, normals as the angle between them in radians). False without far figures.
	bool CheckFarKeys(double& OutWorstPositionCm, double& OutWorstNormalRad, int32& OutFigures) const;

	const FTCSoldierCrowdStats& Stats() const { return Last; }
	int32 BlastTosses() const { return Tosses; }
	int32 NumCorpses() const;
	FTCCorpseRules Rules;
	// round 17: the men and bodies are written with last frame's transforms (motion vectors; Client/TCModelLibrary.h)
	FTCInstanceHistory History;
	// Corpse yaw jitter and tosses draw from this (main.js uses Math.random); fixed for tests.
	FRandomStream Random = FRandomStream(0x5eed);

private:
	struct FNearFigure
	{
		bool bValid = false;
		UTexture2D* Keys = nullptr;
		UInstancedStaticMeshComponent* Body[2] = {};  // solid, faded
		UInstancedStaticMeshComponent* Crown[2] = {};
		TArray<TPair<UInstancedStaticMeshComponent*, float>> Kit[2]; // component, bone index
	};
	struct FFarShape
	{
		int32 Vertices = 0;
		float NearMinY = 0.f, FarMinY = 0.f;
		TArray<float> Positions, Normals; // NumKeys * Vertices * 3, three space metres
		TArray<float> Base, Tint;         // Vertices * 3, linear
		TArray<float> Materials;          // Vertices, reference matId including baked grime
		TArray<uint16> Indices;
	};
	struct FFarFigure
	{
		bool bValid = false;
		UTexture2D* Keys = nullptr;
		UStaticMesh* Mesh = nullptr;
		UInstancedStaticMeshComponent* Draw[2] = {}; // solid, faded
		UInstancedStaticMeshComponent* Riders = nullptr;
		UInstancedStaticMeshComponent* RiderShadow = nullptr;
	};
	struct FBatch { TArray<FTransform> Transforms; TArray<float> Data; int32 Floats = 0; TArray<uint64> Keys; };
	struct FStood { bool bSet = false; FString FigureType; int32 Faction = 0, Slot = 0; FLinearColor Owner; FTransform Man; };
	struct FSquadRecord { int32 Alive = -1; TArray<FStood> Men; };
	struct FCorpse
	{
		FString FigureType; int32 Faction = 0, Slot = 0; FLinearColor Owner; float Scale = 1.f;
		double X = 0, Y = 0, Z = 0, Yaw = 0, Age = 0, Out = -1; // reference metres and radians; Out < 0: not fading
		bool bFar = false, bTossed = false, bFly = false;
		uint32 Serial = 0; // FTCInstanceHistory::BodyKey
		double VX = 0, VY = 0, VZ = 0, RX = 0, RZ = 0, WX = 0, WZ = 0;
	};

	FNearFigure* NearFigure(const FString& FigureType, int32 Faction, int32 Slot);
	FFarFigure* FarFigure(const FString& FigureType, int32 Faction, int32 Slot);
	UInstancedStaticMeshComponent* MakeBatch(UStaticMesh* Mesh, int32 Floats, const TArray<UMaterialInterface*>& Materials);
	void Add(UInstancedStaticMeshComponent* Ism, const FTransform& Xf, const float* Data, int32 Floats, uint64 Key);
	bool DrawNear(const FString& FigureType, int32 Faction, int32 Slot, const FTransform& Xf, const TArray<float>& Blend12, float Fade, bool bForceFaded, uint64 Key);
	bool DrawFar(const FString& FigureType, int32 Faction, int32 Slot, const FTransform& Xf, const TArray<float>& Blend12, float Fade, const FLinearColor& Owner, bool bForceFaded, uint64 Key, bool bDeckRider = false);
	void Fall(const FStood& Man);
	void DropCorpse(int32 Index);
	bool LoadFarFigures(FString& OutError);

	struct FKitAttachment { FName Bone; FString Mesh; };
	struct FKit { TArray<int32> Sections; TArray<FKitAttachment> Attachments; };
	TMap<FString, FKit> Kits;
	TMap<FString, TArray<FString>> KitMaterials;
	TMap<FString, FFarShape> FarData;
	TMap<FString, FNearFigure> NearFigures;
	TMap<FString, FFarFigure> FarFigures;
	TMap<int32, FSquadRecord> Squads;
	TArray<FCorpse> Corpses;
	uint32 NextSerial = 1;                     // FCorpse::Serial
	int32 Tosses = 0;                          // bodies thrown by a blast since this crowd was made
	TWeakObjectPtr<class ATCEffects> EffectsSeen; // the game's effects (fx.js blastNear and gore) when no BlastNear is passed
	TMap<UInstancedStaticMeshComponent*, FBatch> Frame;
	TArray<UInstancedStaticMeshComponent*> AllBatches;
	FTCSoldierCrowdStats Now, Last;
	bool bLoaded = false;

	UPROPERTY(Transient) TObjectPtr<UMaterialInterface> CrowdFaded;
	UPROPERTY(Transient) TObjectPtr<UMaterialInterface> CrowdKitFaded;
	UPROPERTY(Transient) TObjectPtr<UMaterialInterface> CrowdLookFaded;    // make-crowd-look.py: the model look, faded
	UPROPERTY(Transient) TObjectPtr<UMaterialInterface> CrowdKitLookFaded;
	bool bModelLook = false;  // near men in the model masters' look (make-crowd-look.py) rather than the atlas-only one
	UPROPERTY(Transient) TObjectPtr<UMaterialInterface> FarSolid;
	UPROPERTY(Transient) TObjectPtr<UMaterialInterface> FarFaded;
	UPROPERTY(Transient) TArray<TObjectPtr<UObject>> Owned; // components, textures, meshes and materials made here
};

// The custom data floats of the crowd components (the materials in Content/Models/Infantry and Content/Models/Motion)
namespace TCCrowdData
{
	enum : int32
	{
		Keys = 0,       // 0-3: the four heaviest keyframes, 4-7: their weights, 8: lean, 9: height scale, 10-11: shift
		Bone = 12,      // the kit mesh's anchor bone (kit components)
		Fade = 13,      // the dithered fade (faded components): 1 solid, camouflage 0.45, a corpse's last 1.5 s
		OwnerR = 14, OwnerG, OwnerB, // the far figure's arm band color, linear
		NearSolidBody = 12, NearSolidKit = 13, Faded = 14, Far = 17,
	};
}
