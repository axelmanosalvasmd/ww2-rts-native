// The commander's camera: a point on the ground it looks at, a yaw, a pitch and a distance. The controller moves it.
//
// Below the pawn, the camera's feel as pure client logic (a port of client/camera.js and the pointer state of
// client/pointer.js it reads), so it can be tested without a window: the pinhole model the widest-view fit and the
// zoom anchor use, edge scrolling (zone, ramp, ease, corners, a cursor pinned where it left the window), the match
// intro glide, the target height's glide over the ground and the wheel's step. Nothing here reads match state.
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Pawn.h"
#include "TCCameraPawn.generated.h"

class UCameraComponent;

UCLASS()
class THREECROSSROADS_API ATCCameraPawn : public APawn
{
	GENERATED_BODY()

public:
	// The browser's camera sees 42 degrees top to bottom whatever the window's shape (client/main.js PerspectiveCamera).
	static constexpr float VerticalFovDegrees = 42.f;

	ATCCameraPawn();
	// Focus in reference metres; GroundMetres is the target's height (client/camera.js cam.y). Yaw and Pitch in
	// degrees, Distance in metres.
	void Place(FVector2D Focus, float GroundMetres, float Yaw, float Pitch, float Distance);

	UPROPERTY() TObjectPtr<UCameraComponent> Camera;
};

// A pinhole view in reference metres (X = x, Y = z, Z up), posed as ATCCameraPawn::Place poses the camera, for the
// widest-view fit and the zoom anchor, which need the projection at poses the camera is not in yet.
struct THREECROSSROADS_API FTCPinhole
{
	FVector Pos, F, R, U;
	double TanX = 1, TanY = 1;
	FTCPinhole(FVector Target, float Yaw, float Pitch, float Dist, double Aspect);
	// The screen pixel of a point; false behind the near plane (the browser's s.z > -1).
	bool Project(const FVector& P, double VX, double VY, FVector2D& Out) const;
	// The point at height Height under the screen pixel S; false when the ray runs level.
	bool Ground(FVector2D S, double VX, double VY, double Height, FVector2D& Out) const;
};

namespace TCCameraFeel
{
	// client/camera.js: Q/E turn at 1.6 rad/s; a pan covers 1.1 view distances a second at the Normal speed; a wheel
	// notch zooms by 10%; the camera comes no closer than 25 m; middle-drag turns 0.006 rad per pixel; the target's
	// height glides over the ground at 4 per second; a zoom keeps the point under the cursor for 0.4 s.
	constexpr float YawRateRadians = 1.6f;
	constexpr float PanRate = 1.1f;
	constexpr float ZoomStep = 0.1f;
	constexpr float MinDistance = 25.f;
	constexpr float MiddleDragRadiansPerPixel = 0.006f;
	constexpr float HeightGlideRate = 4.f;
	constexpr float AnchorSeconds = 0.4f;
	// client/camera.js EDGE_EASE and the intro's length
	constexpr float EdgeEaseSeconds = 0.15f;
	constexpr float IntroSeconds = 2.5f;
	// client/camera.js PAN and SPEEDS: the menu's pan speed (saved as ww2-pan, 0 1 2)
	constexpr int32 PanSpeedCount = 3;
	float PanSpeedFactor(int32 Index);
	const TCHAR* PanSpeedName(int32 Index);

	// t * t * (3 - 2t) on [0, 1] (the intro and the edge push ease)
	float Smoothstep(float T);
	// client/camera.js edgeZone: 3% of the window's short side, kept within 24 to 48 px (32 px at 1920 x 1080)
	float EdgeZone(float W, float H);
	// client/camera.js edgePush depth: 30% at the zone's inner border up to 100% at the edge; 0 past Reach
	float EdgeDepth(float FromEdge, float Reach, float Zone);
	// cam.y += (ground - cam.y) * min(1, dt * 4)
	float GlideHeight(float Current, float Ground, float Dt);
	// client/camera.js wheel: one notch (+1 out, -1 in) from Dist, kept within [25 m, Widest]
	float ZoomedDistance(float Dist, int32 Notch, float Widest);
	// client/camera.js shift: the focus that makes the screen pixel Pixel look at the ground point P (metres, Z up), at
	// this pose. False when the ray through the pixel runs level.
	bool ShiftToPixel(FVector2D Focus, float Height, float Yaw, float Pitch, float Dist, double VX, double VY, const FVector& P, FVector2D Pixel, FVector2D& OutFocus);
	// The browser's yaw (radians, the camera at (sin yaw, cos yaw) from its target) and this camera's (degrees, at
	// -(cos Yaw, sin Yaw)): Yaw = -90 - degrees(yaw). A middle-drag of Dx pixels turns the browser's yaw by -0.006 Dx.
	float MiddleDragYawDegrees(float DxPixels);
}

// client/pointer.js where, for edge scrolling: 'none' (not known: the window has no focus, or the cursor has not moved
// in it since), 'in' (seen inside the window), 'out' (it left across an edge and stays pinned to that border, so the
// push goes on until it comes back, the window loses the focus or the match ends).
enum class ETCPointerWhere : uint8 { None, In, Out };

class THREECROSSROADS_API FTCPointerEdge
{
public:
	// One frame. bHasPos: the window reports the cursor at Pos (viewport pixels; a captured drag may report a pixel past
	// the size). bPressed: a mouse button went down this frame (client/pointer.js mousedown also counts as seen).
	void Frame(bool bFocused, bool bHasPos, FVector2D Pos, float W, float H, bool bPressed = false);
	void Reset() { Where = ETCPointerWhere::None; bKnown = false; Vel = FVector2D::ZeroVector; }
	ETCPointerWhere Where = ETCPointerWhere::None;
	// The cursor as edge scrolling reads it: the real one inside the window, the pinned one when out.
	FVector2D Pos = FVector2D::ZeroVector;
	bool Active() const { return Where != ETCPointerWhere::None; }
	bool Out() const { return Where == ETCPointerWhere::Out; }

private:
	// client/pointer.js leave: pinned on the border along the way it was moving, or the nearest border
	void Leave(FVector2D Exit, bool bExitKnown, float W, float H);
	FVector2D Last = FVector2D::ZeroVector, Vel = FVector2D::ZeroVector;
	bool bKnown = false;
};

// What edge scrolling reads this frame (client/camera.js edgePush's conditions).
struct FTCEdgeInput
{
	bool bEnabled = true;    // the menu's Edge scroll switch (saved as ww2-edge)
	bool bActive = false;    // FTCPointerEdge::Active
	bool bOut = false;       // FTCPointerEdge::Out
	bool bOverView = true;   // the cursor is over the view, not a HUD panel (out counts as over the view)
	bool bWorld = false;     // a match is shown
	bool bBlocked = false;   // a sheet or menu covers the board (the Controls sheet)
	bool bButtons = false;   // the left or middle button is held (pointer buttons & 5)
	bool bDragging = false;  // a box drag or a facing gesture
	bool bMiddle = false;    // a middle-drag rotation
	FVector2D Pos = FVector2D::ZeroVector;
	float W = 0, H = 0;
};

struct FTCEdgePush
{
	bool bPush = false;
	float Fw = 0, Rt = 0;    // forward (up the screen) and right, eased, each within [-1, 1]
	FString Dir;             // 'n', 'se', ...: the pushed direction (client/pointer.js cursor sprite)
};

class THREECROSSROADS_API FTCEdgeScroll
{
public:
	// client/camera.js edgePush. bFollowing: a follow is on (a push that was already there when it started waits until
	// the cursor leaves the zone; a fresh push after that ends the follow, which the caller does).
	FTCEdgePush Step(const FTCEdgeInput& In, float Dt, bool bFollowing);
	// client/camera.js follow: edgeHold = true
	void HoldForFollow() { bHold = true; }
	void Reset() { Age = 0; bHold = false; }
	float EasedAge() const { return Age; }

private:
	float Age = 0;
	bool bHold = false;
};

// The camera's pose as client/camera.js cam holds it (yaw apart): target, its height and the distance.
struct FTCCameraPose
{
	FVector2D Focus = FVector2D::ZeroVector;
	float Height = 0;
	float Dist = 60;
};

// client/camera.js startIntro and update: the camera glides from the widest view to the start view over 2.5 s, eased;
// skipIntro jumps to the end.
class THREECROSSROADS_API FTCCameraIntro
{
public:
	void Begin(const FTCCameraPose& From, const FTCCameraPose& To) { bActive = true; Age = 0; FromPose = From; ToPose = To; }
	bool IsActive() const { return bActive; }
	// Advances the glide; Out is the pose for this frame. Returns false (and Out the start view) once it is over.
	bool Step(float Dt, FTCCameraPose& Out);
	// Ends the glide: true when it was running, Out the start view.
	bool Skip(FTCCameraPose& Out);
	const FTCCameraPose& From() const { return FromPose; }
	const FTCCameraPose& To() const { return ToPose; }

private:
	bool bActive = false;
	float Age = 0;
	FTCCameraPose FromPose, ToPose;
};
