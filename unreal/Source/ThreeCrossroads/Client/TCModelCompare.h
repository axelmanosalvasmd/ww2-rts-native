// Side-by-side evidence for the unit models: renders units through ATCUnitRenderer (the game's own drawing path, fed a
// synthetic snapshot) from the cameras the reference viewer page used, so each picture pairs with a browser one.
//
//   UnrealEditor-Cmd ThreeCrossroads.uproject -run=TCModelCompare -Cameras=<cameras.json> -Out=<dir>
//       -AllowCommandletRendering -RenderOffscreen [-TCModels=all]
//
// cameras.json comes from unreal/Tools/models/compare/compare.html (it builds the same units with the pinned
// reference's builders and writes the cameras, the light and the ground it used). Writes <dir>/<asset>-<view>-unreal.png
// and <dir>/unreal-report.json.
#pragma once

#include "CoreMinimal.h"
#include "Commandlets/Commandlet.h"
#include "TCModelCompare.generated.h"

class FTCMatchState;

// Round 17: one pose of a side by side tour, the file both the native tour (-TCTourFile, ATCGameMode in
// TCSlice13Checks.cpp) and unreal/Tools/compare/tour-reference.mjs read: "name seconds x z distance yaw [hud]".
//   x: a number, "frontier", "unit:<type>" or "unit:<type>@<seat>" (round 17: that seat's unit, not the viewer's; a
//      spectator's default is seat 0), or "step:<n>" (the Tutorial: once the snapshot's step reaches n, 1-based, and
//      StepSettleSeconds more, the camera centres on shared/tutorial.js STEPS[n - 1].at, the cell its goal points to)
//   seconds: match time (the snapshot's tick / 20), or p<seconds> (round 17): that match time and the match paused (the
//      host's Pause), so both clients shoot the same frozen scene however late the browser gets there
//   distance: metres, or wF (the share F of the widest view)
struct FTCTourPose
{
	FString Name;
	double At = 0, X = 0, Z = 0, Dist = 0, Yaw = -90;
	bool bFrontier = false, bWideShare = false, bHud = false, bWhenPaused = false;
	FString UnitType;
	int32 UnitSeat = -1; // -1: the viewer's seat (seat 0 for a spectator)
	int32 Step = 0;      // 0: not a tutorial step pose
};

namespace TCTour
{
	constexpr double StepSettleSeconds = 3.0; // tour-reference.mjs: 3 s after the step is reached
	// One line ('#' starts a comment); false for a blank, a comment or a line of fewer than six fields.
	bool ParseLine(const FString& Line, FTCTourPose& Out);
	// The snapshot's tutorial step (mode.step, 0-based; -1 without one) has reached the pose's step n (1-based).
	inline bool StepReached(int32 PoseStep, int32 SnapshotStep) { return PoseStep > 0 && SnapshotStep >= PoseStep - 1; }
	// The centre of STEPS[n - 1].at in metres (x = at[0] * 2 + 1, z = at[1] * 2 + 1); false for a step the script lacks.
	bool StepFocus(int32 PoseStep, FVector2D& Out);
	// The seat a unit pose follows: its own, else the viewer's, a spectator's seat 0.
	int32 UnitSeatFor(const FTCTourPose& P, bool bSpectator, int32 You);
	// The living unit of the pose's type with the lowest id on that seat (tour-reference.mjs picks the same); -1 if none.
	// unit:vet: the living unit with the most stars of any seat (lowest id among equals; round 17).
	int32 PickUnit(const FTCTourPose& P, const FTCMatchState& State, int32 Seat, FVector2D& OutAt);
}

UCLASS()
class THREECROSSROADS_API UTCModelCompareCommandlet : public UCommandlet
{
	GENERATED_BODY()

public:
	UTCModelCompareCommandlet();
	virtual int32 Main(const FString& Params) override;
};
