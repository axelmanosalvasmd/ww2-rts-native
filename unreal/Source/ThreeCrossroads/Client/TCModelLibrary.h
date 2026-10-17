// The reference game's unit models as imported by unreal/Tools/models/import-models.py: which static meshes draw a
// unit type of a faction, how its moving parts hang together (sockets), which materials they take, and the
// per-instance custom data the materials read (owner color, vehicle paint, fade, damage).
//
// Reads Content/Models/ModelCatalog.json, which the importer writes next to the assets. Nothing here loads a mesh
// until a component is set up, so the mapping can be checked without the assets (see the automation test).
//
// Built for instanced static meshes: one component per part mesh (and per faded or solid material set), every unit
// an instance whose custom data carries its colors. ATCUnitRenderer draws with it (TCUnitModels.cpp); the import and
// the material contract are in unreal/Tools/models/README.md and the header of import-models.py.
#pragma once
#define TC_HAS_MODEL_SELECTION 1
struct FTCPinhole;

#include "CoreMinimal.h"
#include "UObject/WeakObjectPtrTemplates.h"

class UInstancedStaticMeshComponent;
class UStaticMesh;

// The custom data floats every model material reads (M_TCModel, M_TCModelFaded, M_TCModelTwoSided).
namespace TCModelData
{
	enum : int32
	{
		OwnerR = 0, OwnerG, OwnerB,       // the owner's seat color, linear (client/main.js COLORS)
		VehicleR, VehicleG, VehicleB,     // vehicle paint, linear: the faction's own unless repainted (FACTIONS[].vehicle)
		Fade,                             // 1 solid; below 1 only the faded material shows it (camouflage 0.45, ghosts 0.35)
		Damage,                           // 0 intact, 1 the reference's wreck (flat 0x1d1b18)
		Count
	};
}

// One moving part: a static mesh, where it hangs and what it is painted with.
struct FTCModelPart
{
	FName Name;                         // hull, turret, mount_0, man_0_near ... (the reference's object names)
	FName Parent;                       // NAME_None for a part on the unit's root
	FName Socket;                       // the parent mesh's socket this part hangs on (NAME_None at the root)
	FTransform Rest;                    // where the part sits relative to the unit root at rest (cm)
	FSoftObjectPath Mesh;
	TArray<FSoftObjectPath> Materials;       // per mesh slot
	TArray<FSoftObjectPath> FadedMaterials;  // the dithered-fade versions, same slots
	TArray<FName> Sockets;              // sockets the mesh carries (children's pivots, muzzle points)
	bool bHidden = false;               // far level of detail and other parts the reference builds hidden
};

struct FTCModelEntry
{
	FString Key;                        // "type|faction" plus any variant
	FString Type;
	int32 Faction = 0;
	FString Variant;                    // kit, pose, lod or look variant joined by '|', empty for the default model
	FString Look;                       // vehicle, naval, gun, soldier, structure
	float Scale = 1.f;
	TArray<FTCModelPart> Parts;         // parents before their children
	int32 FindPart(FName Name) const;
};

struct FTCModelQuery
{
	FString Type;
	int32 Faction = 0;
	FString Variant;
};

// How Resolve found the model.
enum class ETCModelMatch : uint8
{
	None,       // nothing for this type
	Exact,      // the type and faction asked for
	Borrowed,   // the reference's own stand-in: a UK vehicle or gun without a model of its own uses the American one
	            // in British paint (client/unit-models.js UK_MODELS), a medic squad the engineer figure
	Substitute, // not what the reference draws: another faction's model, so something shows while assets are missing
};

// Motion vectors for moving instances (round 17). The engine's default anti-aliasing here is temporal (TSR); an
// instance written with no previous transform has no velocity, so a moving soldier, gun or ring is blended with the
// ground it left over the following frames and draws smeared and see-through (round 16's Conquest spectator pose:
// the AI's squads crossing the centre were barely visible, the browser's 4x multisampled frame has no history). Each
// component keeps per-instance previous transforms, matched frame to frame by a caller's stable key (a unit id and
// part, a soldier, a body); an instance without last frame's key, or one that jumped further than JumpCm, starts still.
// -TCNoMotionVectors writes every instance still (the behaviour before round 17; for an A/B on one build).
class THREECROSSROADS_API FTCInstanceHistory
{
public:
	static constexpr double JumpCm = 1000.0; // 10 m in one frame is a jump (a unit's eased glide never moves that far)
	// The previous-frame transform for each of Cur (world space), from Last by key; Cur's own where there is none.
	// Returns how many matched a key of the last frame.
	static int32 PreviousFor(const TArray<FTransform>& Cur, const TArray<uint64>& Keys, const TMap<uint64, FTransform>& Last, TArray<FTransform>& OutPrev);
	// Writes Cur into Ism (world space) with previous transforms, replacing every instance when the count changed.
	// Keys may be empty (or short): those instances start still.
	void Write(UInstancedStaticMeshComponent* Ism, const TArray<FTransform>& Cur, const TArray<uint64>& Keys);
	void Reset() { Last.Reset(); }
	// What the last Write calls did since ResetCounts (the status line and tests): instances, matched, moving.
	int32 Written = 0, Matched = 0, Moving = 0;
	void ResetCounts() { Written = Matched = Moving = 0; }
	// Instance keys: a unit's part (part index) or its men (man index), a ring, a body. 0 is no key (starts still).
	static constexpr uint64 NoKey = 0;
	void SelectionBounds(int32 UnitId, const FTCPinhole& View, double W, double H, TArray<FBox2D>& Out) const;
	static uint64 PartKey(int32 UnitId, int32 Part) { return (1ull << 60) | (uint64(uint32(UnitId)) << 24) | (uint64(Part & 0xFFFF) << 4) | 0; }
	static uint64 ManKey(int32 UnitId, int32 Man) { return (1ull << 60) | (uint64(uint32(UnitId)) << 24) | (uint64(Man & 0xFFFF) << 4) | 1; }
	static uint64 RingKey(int32 UnitId, bool bSelection) { return (1ull << 60) | (uint64(uint32(UnitId)) << 24) | (bSelection ? 3 : 2); }
	static uint64 BodyKey(uint32 Serial) { return (1ull << 62) | Serial; }
private:
	TMap<TWeakObjectPtr<UInstancedStaticMeshComponent>, TMap<uint64, FTransform>> Last;
};

class THREECROSSROADS_API FTCModelLibrary
{
public:
	// Content/Models/ModelCatalog.json
	bool Load(FString& OutError);
	bool LoadFromString(const FString& Json, FString& OutError);
	bool IsLoaded() const { return Entries.Num() > 0; }
	int32 Num() const { return Entries.Num(); }
	const FString& ReferenceCommit() const { return Reference; }

	const FTCModelEntry* Find(const FString& Key) const;
	const FTCModelEntry* Resolve(const FTCModelQuery& Query, ETCModelMatch* OutMatch = nullptr) const;

	// The reference's stand-in rules (client/unit-models.js buildModel), exposed for tests and the renderer.
	static bool HasOwnUkModel(const FString& Type);
	static FString FigureType(const FString& Type) { return Type == TEXT("medic") ? FString(TEXT("engineer")) : Type; }

	// Colors from the catalog (the importer reads them from the pinned client/main.js).
	FLinearColor SeatColor(int32 Seat) const;      // COLORS[seat] ?? 0xdddddd
	FLinearColor VehiclePaint(int32 Faction) const;

	// The custom data floats for one instance (TCModelData order). Out must hold TCModelData::Count floats.
	static void WriteCustomData(TArrayView<float> Out, const FLinearColor& Owner, const FLinearColor& Vehicle, float Fade = 1.f, float Damage = 0.f);
	TArray<float> CustomData(int32 OwnerSeat, int32 Faction, float Fade = 1.f, float Damage = 0.f) const;

	// Where a part is drawn: the unit's transform, the part's rest place, and per-part turns about the part's own pivot
	// (a turret's traverse is a yaw on "turret"; a part inherits its parent's turn).
	static FTransform PartTransform(const FTCModelEntry& Entry, int32 PartIndex, const FTransform& Unit, const TMap<FName, FQuat>& Turns = {});

	// Point an instanced static mesh component at one part: its mesh, its solid or faded materials, room for the
	// custom data, no collision. Loads the assets synchronously the first time. False if the mesh is missing.
	static bool SetupComponent(UInstancedStaticMeshComponent* Component, const FTCModelPart& Part, bool bFaded);

	// Fades the reference uses (client/main.js): camouflaged units and remembered enemy buildings.
	float CamouflageFade = 0.45f;
	float GhostFade = 0.35f;

private:
	TArray<FTCModelEntry> Entries;
	TMap<FString, int32> ByKey;
	TMap<FString, TArray<int32>> ByType; // type -> entries, sorted by faction then variant
	TArray<FLinearColor> Seats;
	TArray<FLinearColor> VehiclePaints;
	FLinearColor Neutral = FLinearColor(0.723055f, 0.723055f, 0.723055f); // 0xdddddd
	FString Reference;
};
