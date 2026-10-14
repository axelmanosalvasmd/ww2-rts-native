// Infantry figures from the reference pose solver (reference/ww2-rts client/models/infantry.js, infantry-authored.js,
// squad-motion.js and the soldier part of unit-models.js), ported operation for operation to C++ in TCInfantryRig.cpp.
//
// Assets: unreal/Content/Models/Infantry (built by unreal/Tools/models/infantry, see its README.md). SK_Infantry is the
// Blender-authored body on a flat 31-bone skeleton: root, the 15 deform bones (Blender's thigh.L is thigh_L), and the
// kit anchors head_gear, grip_L/R, weapon, sling, item_0-3, spine_0-3 and hose_0-1, each with a socket of its name. Every attachment of a figure is a static mesh in its anchor's frame;
// kits.json says which mesh hangs on which anchor for each (type, faction, job).
//
// Two ways to draw a man, both fed by the same solver:
//   - near or few: USkeletalMeshComponent + UTCInfantryAnimInstance (this file) with the kit meshes attached to the
//     anchor bones by name;
//   - crowds: the 29 keyframes of a figure baked to a bone-matrix texture (BakeKeyframeTexture), drawn as instanced
//     static meshes whose material skins on the GPU from per-instance custom data (CrowdCustomData). This is the
//     reference's own scheme (one instanced draw per figure, posture and gait as blended keyframes) with 31 bone
//     matrices per keyframe instead of 7,580 vertex positions. README.md explains the choice.
//
// Axes: Unreal component space, centimetres, +X forward, +Y right, +Z up (the project's ToWorld(x, z, y) * 100).
#pragma once

#include "CoreMinimal.h"
#include "Animation/AnimInstance.h"
#include "Kismet/BlueprintFunctionLibrary.h"
#include "TCInfantryRig.generated.h"

class UTexture2D;
class USkeletalMesh;

// The reference's postures (0 stand, 1 crouch, 2 prone, 3 retreat) plus the corpse and the dig stroke.
UENUM(BlueprintType)
enum class ETCInfantryState : uint8
{
	Stand,
	Crouch,
	Prone,
	Retreat,
	Fallen,
	Dig,
};

// One solved pose of one man.
USTRUCT(BlueprintType)
struct THREECROSSROADS_API FTCInfantryPose
{
	GENERATED_BODY()

	// The job the kit gives this man (infantry.js soldierKit), e.g. "leader", "rifleB", "feeder".
	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	FString Job;

	// Component-space transform of every bone, in UTCInfantryRigLibrary::BoneNames() order.
	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	TArray<FTransform> Bones;

	// False for anchors this job does not use (their transform has zero scale).
	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	TArray<bool> BoneUsed;

	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	bool bHasMuzzle = false;

	// Where the weapon's muzzle is (the reference's muzzle flash point), component space.
	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	FVector Muzzle = FVector::ZeroVector;
};

// One man's motion state, the inputs of unit-models.js animate() for his figure.
USTRUCT(BlueprintType)
struct THREECROSSROADS_API FTCInfantryMotion
{
	GENERATED_BODY()

	// The squad's eased posture weights sq.w: stand, crouch, prone, retreat.
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	FVector4 PostureWeights = FVector4(1, 0, 0, 0);

	// The man's walk blend (motion.blend, 0 standing to 1 at 0.7 m/s and over).
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	float Moving = 0.f;

	// The squad's aim blend (moving fire).
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	float Aim = 0.f;

	// The gait phase, 0 to 1 per two steps (motion.phase).
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	float Phase = 0.f;

	// The dig blend (sq.work) and the dig stroke phase (sq.workT + motion.seed).
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	float Work = 0.f;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	float WorkPhase = 0.f;

	// A corpse: the fallen build, nothing else.
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	bool bFallen = false;
};

// The weights and whole-body transform a motion state gives a figure's keyframes.
USTRUCT(BlueprintType)
struct THREECROSSROADS_API FTCInfantryBlend
{
	GENERATED_BODY()

	// Weight of each of the 29 keyframes (UTCInfantryRigLibrary::NumKeys). Key 0 takes 1 minus the others' sum.
	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	TArray<float> KeyWeights;

	// unit-models.js pose node: lean (radians about the man's right axis), height scale and shift (cm, X and Z).
	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	float Lean = 0.f;

	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	float HeightScale = 1.f;

	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	FVector2D Shift = FVector2D::ZeroVector;

	// The man node's offset (cm, X and Z, before his figure scale) that keeps the muzzle point fixed between postures.
	UPROPERTY(BlueprintReadOnly, Category = "Infantry")
	FVector2D ManOffset = FVector2D::ZeroVector;
};

UCLASS()
class THREECROSSROADS_API UTCInfantryRigLibrary : public UBlueprintFunctionLibrary
{
	GENERATED_BODY()

public:
	static constexpr int32 NumBones = 31;
	static constexpr int32 NumKeys = 29;
	// Bone matrix texture layout: three RGBA32F texels per bone (the columns of its 4x3 matrix), one row per keyframe.
	static constexpr int32 TexelsPerBone = 3;

	// The job a man of a squad has: infantry.js soldierKit(type, i, fac). "medic" uses the engineer figure.
	UFUNCTION(BlueprintPure, Category = "Infantry")
	static FString SoldierKit(const FString& UnitType, int32 Slot, int32 Faction);

	UFUNCTION(BlueprintPure, Category = "Infantry")
	static TArray<FName> BoneNames();

	// The scale unit-models.js gives a man of this unit type (line infantry 1.1, everyone else 1.35).
	UFUNCTION(BlueprintPure, Category = "Infantry")
	static float FigureScale(const FString& UnitType);

	// Solve one pose. Phase < 0 holds the posture; Phase in [0, 1) walks it (stand: aimed walk, crouch: crouched walk,
	// prone: crawl, retreat: run). DigKey picks the stroke (0 to 3) for Pose == Dig.
	UFUNCTION(BlueprintCallable, Category = "Infantry")
	static bool SolvePose(const FString& UnitType, int32 Faction, int32 Slot, ETCInfantryState Pose, float Phase, int32 DigKey, FTCInfantryPose& Out);

	// unit-models.js animate() for one man: keyframe weights and the whole-body pose transform.
	UFUNCTION(BlueprintPure, Category = "Infantry")
	static FTCInfantryBlend BlendFor(const FString& UnitType, const FTCInfantryMotion& Motion);

	// The 29 keyframes of a figure as skinning matrices (pose times inverse bind pose), Unreal space, row-vector
	// FMatrix44f, NumKeys * NumBones of them. Posture keys are stored undone by their pose node (unit-models.js
	// poseMorphs), so applying FTCInfantryBlend's pose node restores them, as in the reference.
	static bool BakeKeyframes(const FString& UnitType, int32 Faction, int32 Slot, TArray<FMatrix44f>& OutSkin);

	// The same keyframes as component-space bone transforms (for the skeletal path), undone like BakeKeyframes.
	static bool BakeKeyframeTransforms(const FString& UnitType, int32 Faction, int32 Slot, TArray<FTransform>& OutBones);

	// BakeKeyframes in a float texture: NumBones * TexelsPerBone wide, NumKeys tall, PF_A32B32G32R32F, no mips.
	UFUNCTION(BlueprintCallable, Category = "Infantry")
	static UTexture2D* BakeKeyframeTexture(const FString& UnitType, int32 Faction, int32 Slot);

	// Per-instance custom data for the crowd material (M_InfantryCrowd): the four heaviest keyframes and their
	// renormalized weights, then lean, height scale and shift (cm). 12 floats.
	UFUNCTION(BlueprintPure, Category = "Infantry")
	static TArray<float> CrowdCustomData(const FTCInfantryBlend& Blend);
};

// client/squad-motion.js moveSquad: each man of a squad eases toward his formation slot with his own response time,
// keeps his shoulders clear of his neighbours, turns toward where he walks (or his target when firing on the move),
// and advances his gait phase by the ground he actually covers. The squad position stays the only gameplay position.
// Plain C++ (no reflection): feed FTCInfantryMotion from Moving, Phase and the squad's posture weights.
struct THREECROSSROADS_API FTCInfantrySquadInput
{
	FVector2D Position = FVector2D::ZeroVector; // the squad root, cm (Unreal X, Y)
	double Yaw = 0;                              // the squad root's rotation about the vertical, reference sign (three.js rotation.y)
	bool bVisible = true, bGarrisoned = false, bTrench = false;
	int32 Id = 0;          // unit id (seeds each man's own response)
	int32 Flags = 0;       // snapshot flags (bit 0: retreating)
	int32 Cover = 0;       // 2: in a trench line
	double Suppression = 0;
	bool bMoveFire = false, bHasTarget = false, bHasAim = false;
	double Aim = 0;        // snapshot aim angle (reference sign)
};
struct THREECROSSROADS_API FTCInfantrySquadMan
{
	// inputs
	FVector2D Slot = FVector2D::ZeroVector; // formation slot in the squad frame, cm, already times SLOT_SCALE
	int32 Index = 0;
	bool bVisible = true;
	float Scale = 1.1f;    // UTCInfantryRigLibrary::FigureScale
	bool bRefill = false;  // walking over to a fallen man's place
	// state and outputs (reference metres and radians, as squad-motion.js keeps them)
	bool bInit = false, bStride = false;
	double X = 0, Z = 0, Yaw = 0, Phase = 0, Seed = 0, Moving = 0, Speed = 0, Stride = 0;
	FVector2D Local = FVector2D::ZeroVector; // the man's offset in the squad frame, cm
	double LocalYaw = 0;
};
struct THREECROSSROADS_API FTCInfantrySquadMotion
{
	TArray<FTCInfantrySquadMan> Men;
	bool bHasLast = false, bHidden = false, bTrenched = false;
	double LastX = 0, LastZ = 0, Speed = 0;
	void Move(const FTCInfantrySquadInput& In, float Elapsed);
};

// client/unit-models.js animate() for one squad, whole: the near/far swap (LOD with 4 m of hysteresis, measured from
// the camera to the squad in 3D), the posture goal and its 0.3 s blend, the aim blend of moving fire, the dig work and
// its stroke clock, the return from suspension (a squad off screen or hidden snaps to its posture, its men to their
// slots, their gait phase to the ground covered meanwhile), moveSquad, each man's keyframe weights and pose node, his
// place on the ground (and 0.6 m down in a trench line he is not seated in), and his muzzle point (u.muzzle).
// The arithmetic is TCInfantryRig.cpp's TCInfantryRigCore::AnimateSquad, checked against the reference animate() under
// Node (unreal/Tools/motion/oracle). Plain C++: one per squad, kept between frames.
struct THREECROSSROADS_API FTCInfantrySquadFrame
{
	FVector Root = FVector::ZeroVector;  // the squad root where it is drawn (Unreal cm; Z is the ground under it)
	double Yaw = 0;                      // root.rotation.y, reference sign (a unit facing rot has -rot)
	FVector Eye = FVector::ZeroVector;   // the camera (Unreal cm)
	bool bVisible = true;                // root.visible: not riding a carrier
	bool bDetailed = true;               // main.js animationInterest: on screen (25 m ball) or selected
	bool bGarrisoned = false;            // v.garr
	bool bTrench = false;                // v.trench: men seated in trench cells (the native ground is not carved yet)
	// The reference draws a squad in a trench line (cover 2) 0.6 m down, on the floor its terrain carves there. The
	// native terrain does not carve trenches yet, so the sink is off until it does (else the men stand in the ground).
	bool bCarvedTrenches = false;
	float LodMetres = 110.f;             // unit-models.js LOD.high (LOD.low = 80 on Low graphics)
	int32 Id = 0, Flags = 0, Cover = 0;
	float Suppression = 0.f;
	bool bHasTarget = false, bMoveFire = false, bHasAim = true;
	double Aim = 0;                      // the snapshot aim angle, reference sign
	// the ground height (cm) under a world point (cm); unset: flat at the root
	TFunction<double(double X, double Y)> GroundCm;
};
struct THREECROSSROADS_API FTCInfantryManPose
{
	bool bVisible = true;                // man.visible: alive (index below the alive count) and not garrisoned
	FVector Local = FVector::ZeroVector; // man.position in the root's frame (Unreal cm, includes the muzzle offset)
	double LocalYaw = 0;                 // man.rotation.y in the root's frame, reference sign
	float Scale = 1.1f;                  // man.scale (UTCInfantryRigLibrary::FigureScale of the unit type)
	FTCInfantryBlend Blend;              // keyframe weights and pose node for the crowd material or the anim instance
	bool bHasMuzzle = false;
	FVector Muzzle = FVector::ZeroVector; // u.muzzle: in the man node's frame, cm before his scale
	FString Job;                         // his kit (UTCInfantryRigLibrary::SoldierKit)
};
class THREECROSSROADS_API FTCInfantrySquadAnimation
{
public:
	FTCInfantrySquadAnimation();
	~FTCInfantrySquadAnimation();
	FTCInfantrySquadAnimation(const FTCInfantrySquadAnimation&);
	FTCInfantrySquadAnimation& operator=(const FTCInfantrySquadAnimation&);

	// The men of a squad of UnitType (medic squads use the engineer figure, as buildModel does): their formation slots
	// in the squad frame (cm, already times SLOT_SCALE: the catalog's man_K_near rest places), all alive.
	void Setup(const FString& UnitType, int32 Faction, const TArray<FVector2D>& SlotsCm, float Scale);
	bool IsSetUp(const FString& UnitType, int32 Faction, int32 Men) const;
	// main.js: man.visible = i < alive && !garrisoned
	void SetAlive(int32 Alive);
	// One frame. False while suspended (Men() keeps the last pose).
	bool Animate(const FTCInfantrySquadFrame& In, float DeltaSeconds);
	bool IsFar() const;
	const TArray<FTCInfantryManPose>& Men() const { return Out; }
	// A man's world transform: root, his place and turn, his scale.
	FTransform ManTransform(int32 Index, const FTransform& Root) const;
	// The muzzle point in the world (u.muzzle through man.matrixWorld), false without a weapon.
	bool MuzzleWorld(int32 Index, const FTransform& Root, FVector& OutWorld) const;

private:
	struct FState;
	FState* State = nullptr;
	TArray<FTCInfantryManPose> Out;
	FString Type;
	int32 Fac = -1;
};

// Drives SK_Infantry from the solver: blends the figure's keyframes with the weights unit-models.js would give them,
// applies the pose node, and writes the result as the component-space pose. No animation graph or assets needed.
UCLASS(Transient, Blueprintable)
class THREECROSSROADS_API UTCInfantryAnimInstance : public UAnimInstance
{
	GENERATED_BODY()

public:
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	FString UnitType = TEXT("rifle");

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	int32 Faction = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	int32 Slot = 0;

	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category = "Infantry")
	FTCInfantryMotion Motion;

	// Re-bake the keyframes after changing UnitType, Faction or Slot.
	UFUNCTION(BlueprintCallable, Category = "Infantry")
	void RebuildFigure();

	// The keyframes in use (NumKeys * NumBones component-space transforms) and the figure's identity.
	const TArray<FTransform>& Keyframes() const { return Keys; }
	FString FigureKey() const { return BakedFor; }

protected:
	virtual void NativeInitializeAnimation() override;
	virtual FAnimInstanceProxy* CreateAnimInstanceProxy() override;

private:
	friend struct FTCInfantryAnimInstanceProxy;
	TArray<FTransform> Keys;
	FString BakedFor;
};
