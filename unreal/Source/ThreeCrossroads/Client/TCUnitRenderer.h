// Unit drawing. Two paths:
//   - the reference game's own models (Content/Models, imported by unreal/Tools/models from the pinned builders; see
//     Client/TCModelLibrary.h and TCUnitModels.cpp): one instanced component per part mesh, every unit an instance whose
//     custom data carries its owner color and faction paint; the reference's presentation motion (Client/TCUnitMotion.h:
//     the hull settling and the boats' swell, the kaiju's stride, the planes' bank and propellers); soldiers animated
//     as the reference's animate() does for a squad (FTCInfantrySquadAnimation: posture, aim, dig, moveSquad, the near/far
//     swap at LOD.high, 110 m) and drawn by UTCSoldierCrowd (near men skinned on the GPU from the ported pose solver, far
//     men as the reference's own far figure, camouflaged men dithered to 0.45, fallen men as bodies with the reference's
//     corpse rules); the owner ring under every unit;
//   - placeholders from engine primitives (men as small cylinders, vehicles as boxes, structures as larger boxes,
//     aircraft as cones aloft, colored by owner seat) for every type not yet adopted, or when an asset is missing.
// Which types take the model path is decided per batch (ModelBatches below; -TCModels= overrides it for a run).
// Positions ease toward each snapshot's row; nothing here decides where a unit really is.
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "Client/TCModelLibrary.h"
#include "Client/TCInfantryRig.h"
#include "Client/TCUnitMotion.h"
#include "TCUnitRenderer.generated.h"

class UInstancedStaticMeshComponent;
class UStaticMesh;
class UMaterialInterface;
class UMaterialInstanceDynamic;
class UTexture2D;
class FTCMatchState;
class FTCUnitTable;
class ATCTerrain;
struct FTCUnit;
struct FTCUnitDef;

// What the model path drew in the last frame (for the status line, the tests and the live evidence).
struct FTCUnitModelStats
{
	int32 ModelUnits = 0;        // units drawn with their reference model
	int32 PlaceholderUnits = 0;  // units drawn as placeholders (type not adopted, or no model for it)
	int32 MissingModel = 0;      // of those: adopted but the catalog had nothing to draw
	int32 PartInstances = 0;     // static mesh part instances (vehicles, guns, structures, far soldiers)
	int32 CrowdMen = 0;          // near soldiers skinned from the pose solver
	int32 FarMen = 0;            // far soldiers (the reference's far-away figure)
	int32 FadedMen = 0;          // of those: camouflaged, dithered to the reference's 0.45
	int32 Corpses = 0;           // fallen men drawn (unit-models.js createBodies)
	int32 MovingHulls = 0;       // units drawn with presentation motion (hull settle, swell, stride, bank)
	int32 WheelHulls = 0;        // of those: tracked hulls drawn with wheel spin and track scroll (FTCWheelTracks)
	float WheelTravelMax = 0.f;  // the most ground any of them has rolled (m, wheel-motion.js wheelTravel.x)
	int32 Rings = 0;             // owner and selection rings
	int32 Matched = 0;           // round 17: instances written with last frame's transform (FTCInstanceHistory)
	int32 Moving = 0;            // of those: moved since the last frame (they carry a motion vector)
	int32 Components = 0;        // instanced components alive (draw batches)
	int32 Figures = 0;           // soldier figures made (near keyframe textures and far figures)
};

UCLASS()
class THREECROSSROADS_API ATCUnitRenderer : public AActor
{
	GENERATED_BODY()

public:
	ATCUnitRenderer();

	void Draw(const FTCMatchState& State, const FTCUnitTable& Table, const ATCTerrain* Terrain, const TSet<int32>& Selected, float DeltaSeconds);
	// Forget every unit (a new match or the lobby): eased positions, squads, motion and the bodies on the ground.
	void Reset();
	// main.js v.killed: units a delivered kill shot named that then left the snapshot (FTCCombatEffects::TakeKilledGone).
	// A squad among them drops its men where they stood when it leaves; any other squad that leaves just vanishes.
	void NoteKilled(const TArray<int32>& Ids) { for (const int32 Id : Ids) KilledIds.Add(Id); }
	// The eased position drawn for a unit (reference metres), for picking what the player sees.
	bool DrawnAt(int32 Id, FVector2D& Out) const;
	int32 InstancesDrawn() const { return LastInstances; }

	static FLinearColor SeatColor(int32 Owner);
	// Whether a unit is drawn at all: not inside a building or a carrier, not a plane parked at its base. Picking uses
	// the same rule, so nothing invisible can be clicked or boxed.
	static bool IsDrawn(const FTCUnit& U);
	// How far above the ground a unit is drawn (aircraft fly AirAltitude up), in metres.
	static float LiftMetres(const FTCUnitDef* Def);
	static constexpr float AirAltitude = 20.f; // client/main.js AIR_ALT
	// client/unit-models.js LOD.high: beyond this camera distance (metres) soldiers and crew guns swap to the far model
	static constexpr float LodMetres = 110.f;
	// The swap distance in use: LOD.high (110 m) on Graphics High, LOD.low (80 m) on Low (client/unit-models.js LOD,
	// Client/TCGraphics.h).
	static float CurrentLodMetres();
	// Wheels and tracks (Client/TCUnitMotion.h FTCWheelTracks): loaded, how many hulls carry them, and why not.
	bool WheelTracksLoaded() const { return WheelTracks.IsLoaded(); }
	int32 WheelTrackHulls() const { return WheelTracks.NumHulls(); }
	const FString& WheelTracksError() const { return WheelTracksWhy; }

	// ----- the model path -----
	// The adoption batches, in the order they were brought in. A type takes the model path when its batch is enabled.
	enum EModelBatch : uint8 { BatchPilot, BatchVehicles, BatchInfantry, BatchAirNaval, BatchStructures, BatchCount };
	static const TCHAR* BatchName(int32 Batch);
	static int32 BatchOf(const FString& Type); // INDEX_NONE for a type no batch adopts
	// Enabled batches: all by default; -TCModels=none|all|pilot,vehicles,... narrows it (read once per renderer).
	bool IsBatchEnabled(int32 Batch) const { return Batch >= 0 && Batch < BatchCount && (EnabledBatches & (1u << Batch)); }
	// An explicit choice (tests, the comparison commandlet): wins over the default and -TCModels=.
	void SetEnabledBatches(uint32 Mask) { EnabledBatches = Mask; bBatchesSet = true; }
	bool UsesModel(const FString& Type) const { return IsBatchEnabled(BatchOf(Type)); }
	// Where the camera is (Unreal cm), for the level-of-detail swap. Unset: the first player's camera.
	void SetViewPoint(const FVector& Eye) { ViewOverride = Eye; }
	void ClearViewPoint() { ViewOverride.Reset(); }
	const FTCModelLibrary& Models() const { return Library; }
	bool ModelsLoaded() const { return Library.IsLoaded(); }
	const FString& ModelLoadError() const { return LibraryError; }
	const FTCUnitModelStats& ModelStats() const { return Stats; }
	// One line for the status log: how many units take which path, and why the model path is off when it is.
	FString ModelStatusLine() const;
	// The near/far state the last frame chose for a squad or crew gun (false when unknown).
	bool IsFar(int32 Id) const { const FShown* S = Shown.Find(Id); return S && S->bFar; }
	// The soldiers' crowd (near, far, faded and fallen men), for the tests and the status line.
	const UTCSoldierCrowd* SoldierCrowd() const { return Crowd; }
	// The motion a unit was drawn with in the last frame (null for squads and units without a model).
	const FTCPartMotion* MotionOf(int32 Id) const { return UnitMotion.Find(Id); }

private:
	enum EKind { Man, Vehicle, Structure, Air, Ring, KindCount };
	UInstancedStaticMeshComponent* Batch(int32 Owner, EKind Kind);

	struct FShown { float X = 0, Z = 0, Rot = 0, Aim = 0; bool bFar = false; bool bAimSet = false; };
	TMap<int32, FShown> Shown;
	TMap<int32, TObjectPtr<UInstancedStaticMeshComponent>> Batches; // key: (owner + 2) * KindCount + kind

	UPROPERTY() TObjectPtr<UStaticMesh> CylinderMesh;
	UPROPERTY() TObjectPtr<UStaticMesh> CubeMesh;
	UPROPERTY() TObjectPtr<UStaticMesh> ConeMesh;
	UPROPERTY() TObjectPtr<UMaterialInterface> BaseMaterial;
	UPROPERTY() TArray<TObjectPtr<UInstancedStaticMeshComponent>> Owned;
	int32 LastInstances = 0;

	// ----- the model path (TCUnitModels.cpp) -----
	// One frame's instances for one component: transforms and, flat, their custom data floats.
	struct FFrameBatch { TArray<FTransform> Transforms; TArray<float> Data; int32 Floats = 0; TArray<uint64> Keys; }; // Keys: FTCInstanceHistory
	// The browser's squad presentation state (unit-models.js animate() for one squad, kept between frames).
	struct FSquad
	{
		FTCInfantrySquadAnimation Anim;
	};
	// One squad's frame, between its animation (before the parts, so a crew gun and its crew swap level of detail
	// together) and drawing its men.
	struct FSquadMen
	{
		TArray<int32, TInlineAllocator<16>> Near, Far; // catalog parts man_K_near and man_K_far, by K
		int32 Men = 0, Alive = 0;
		FString FigureType;
	};

	void EnsureModels();
	bool DrawModel(const FTCMatchState& State, const FTCUnit& U, const FTCUnitDef* Def, FShown& S, const ATCTerrain* Terrain, bool bSelected, float Dt, const FVector& Eye);
	// unit-models.js animate() for a squad or crew: false when the entry has no men
	bool AnimateSquad(const FTCModelEntry& Entry, const FTCUnit& U, const FTCUnitDef* Def, int32 Faction, FShown& S, const FTransform& Root, const ATCTerrain* Terrain, float Dt, const FVector& Eye, FSquadMen& Out);
	void DrawSquadMen(const FTCModelEntry& Entry, const FTCUnit& U, int32 Faction, const FShown& S, const FTransform& Root, const FSquadMen& Men, const TArray<float>& Custom);
	void AddRing(float Radius, const FLinearColor& Color, float Opacity, int32 ColorKey, const FVector& At, uint64 Key = 0);
	// markers.js overlayMat rings are toneMapped: false; the game's post (M_TCLookPost) tone maps everything, so a
	// ring's color is the ACES pre-image of its color at the light's exposure (ATCLighting), redone when it changes
	void UpdateRingLooks();
	FLinearColor RingColorFor(const FLinearColor& Display) const;
	UInstancedStaticMeshComponent* PartComponent(const FTCModelPart& Part, bool bFaded);
	// A new part component (PartComponent, before its shadow twin is made): a catalog hull with wheels takes its copy
	// carrying the reference's wheel and track data and the moving master, with the path table bound and two more
	// floats of custom data (travel, turn). False when the part keeps the catalog's mesh (no copy, faded, or the
	// wheel data is off: -TCNoWheelTracks, or WheelTracks.json missing).
	bool ApplyWheelTracks(UInstancedStaticMeshComponent* Ism, const FTCModelPart& Part, bool bFaded);
	// One part instance's custom data: the catalog's floats, then (wheel copies only) wheel-motion.js wheelTravel.
	void AppendPartData(FFrameBatch& B, const UInstancedStaticMeshComponent* Ism, const TArray<float>& Custom, const FTCPartMotion& Motion);
	// round 17: every instance written with its previous transform (motion vectors for the temporal anti-aliasing)
	FTCInstanceHistory History;
	FTCInstanceHistory TwinHistory;
	UStaticMesh* RingMesh(float Radius);
	void FlushModelFrame();
	// Every 5 s of real time, one log line: the model status and the frame time since the last line (regression
	// awareness for the live check, not a benchmark).
	void ReportFrame();
	double ReportAt = 0, FrameSum = 0, FrameMax = 0;
	FString DrawnList; // this frame's "id type owner x z near|far" for each unit drawn with its model (ReportFrame)
	int32 FrameCount = 0;

	FTCModelLibrary Library;
	FString LibraryError;
	bool bModelsTried = false;
	bool bBatchesSet = false;
	uint32 EnabledBatches = (1u << BatchCount) - 1;
	TOptional<FVector> ViewOverride;
	FTCUnitModelStats Stats;
	TMap<int32, FSquad> Squads;
	FTCUnitMotion UnitMotion;                     // vehicles, boats, the kaiju and planes (Client/TCUnitMotion.h)
	FTCWheelTracks WheelTracks;                   // the hull copies that turn their wheels and run their tracks
	bool bWheelTracksTried = false;
	FString WheelTracksWhy;
	double WheelLogAt = 0;
	TMap<FString, TObjectPtr<UMaterialInstanceDynamic>> WheelMids; // per motion material: its MID with TrackPaths bound
	TSet<int32> KilledIds;                        // NoteKilled, until the unit leaves the snapshot
	UPROPERTY() TObjectPtr<UTCSoldierCrowd> Crowd; // every soldier drawn: near, far, faded and fallen
	TMap<FString, TObjectPtr<UInstancedStaticMeshComponent>> PartBatches; // mesh path, plus "|faded"
	TMap<int32, TObjectPtr<UInstancedStaticMeshComponent>> RingBatches;   // radius in cm * 1000 + color key
	TMap<int32, TObjectPtr<UStaticMesh>> RingMeshes;                      // radius in cm
	struct FRingLook { TObjectPtr<UMaterialInstanceDynamic> Mid; FLinearColor Display; };
	TMap<int32, FRingLook> RingLooks;                                     // by ring batch key
	float RingExposure = -1.f;                                            // the exposure RingLooks were made for (0: none)
	TWeakObjectPtr<class ATCLighting> LightingSeen;
	TMap<UInstancedStaticMeshComponent*, FFrameBatch> Frame;

	UPROPERTY() TArray<TObjectPtr<UObject>> ModelObjects; // everything the model path made or loaded, kept alive
};
