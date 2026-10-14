#include "Client/TCCameraPawn.h"
#include "Camera/CameraComponent.h"
#include "Misc/AutomationTest.h"

ATCCameraPawn::ATCCameraPawn()
{
	PrimaryActorTick.bCanEverTick = false;
	RootComponent = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));
	Camera = CreateDefaultSubobject<UCameraComponent>(TEXT("Camera"));
	Camera->SetupAttachment(RootComponent);
	// The engine reads FieldOfView as horizontal at the camera's AspectRatio and, with MaintainYFOV, keeps the vertical
	// angle that implies for any window (FMinimalViewInfo::CalculateProjectionMatrixGivenViewRectangle). So the
	// horizontal angle that gives 42 degrees vertically at that aspect is set, and the axis is fixed here rather than
	// left to the LocalPlayer's ini setting.
	Camera->bOverrideAspectRatioAxisConstraint = true;
	Camera->SetAspectRatioAxisConstraint(EAspectRatioAxisConstraint::AspectRatio_MaintainYFOV);
	const float HalfY = FMath::DegreesToRadians(VerticalFovDegrees / 2.f);
	Camera->SetFieldOfView(FMath::RadiansToDegrees(2.f * FMath::Atan(FMath::Tan(HalfY) * Camera->AspectRatio)));
	SetActorEnableCollision(false);
}

void ATCCameraPawn::Place(FVector2D Focus, float GroundMetres, float Yaw, float Pitch, float Distance)
{
	const FVector Target(Focus.X * 100.f, Focus.Y * 100.f, GroundMetres * 100.f);
	const FRotator Look(-Pitch, Yaw, 0.f);
	SetActorLocationAndRotation(Target - Look.Vector() * Distance * 100.f, Look);
}

FTCPinhole::FTCPinhole(FVector Target, float Yaw, float Pitch, float Dist, double Aspect)
{
	const FRotationMatrix RM(FRotator(-Pitch, Yaw, 0.f));
	F = RM.GetUnitAxis(EAxis::X); R = RM.GetUnitAxis(EAxis::Y); U = RM.GetUnitAxis(EAxis::Z);
	Pos = Target - F * Dist;
	TanY = FMath::Tan(FMath::DegreesToRadians(ATCCameraPawn::VerticalFovDegrees / 2.0));
	TanX = TanY * Aspect;
}

bool FTCPinhole::Project(const FVector& P, double VX, double VY, FVector2D& Out) const
{
	const FVector D = P - Pos;
	const double Z = D | F;
	if (Z <= 0.1) return false;
	Out = FVector2D((1.0 + (D | R) / Z / TanX) / 2.0 * VX, (1.0 - (D | U) / Z / TanY) / 2.0 * VY);
	return true;
}

bool FTCPinhole::Ground(FVector2D S, double VX, double VY, double Height, FVector2D& Out) const
{
	const FVector Dir = F + R * ((2.0 * S.X / VX - 1.0) * TanX) + U * ((1.0 - 2.0 * S.Y / VY) * TanY);
	if (FMath::Abs(Dir.Z) < 1e-6) return false;
	const double T = (Height - Pos.Z) / Dir.Z;
	Out = FVector2D(Pos.X + Dir.X * T, Pos.Y + Dir.Y * T);
	return true;
}

namespace TCCameraFeel
{
	float PanSpeedFactor(int32 Index)
	{
		static const float Speeds[PanSpeedCount] = { 0.6f, 1.f, 1.6f };
		return Speeds[FMath::Clamp(Index, 0, PanSpeedCount - 1)];
	}

	const TCHAR* PanSpeedName(int32 Index)
	{
		static const TCHAR* const Names[PanSpeedCount] = { TEXT("Slow"), TEXT("Normal"), TEXT("Fast") };
		return Names[FMath::Clamp(Index, 0, PanSpeedCount - 1)];
	}

	float Smoothstep(float T)
	{
		T = FMath::Clamp(T, 0.f, 1.f);
		return T * T * (3.f - 2.f * T);
	}

	float EdgeZone(float W, float H)
	{
		return FMath::Min(48.f, FMath::Max(24.f, FMath::Min(W, H) * 0.03f));
	}

	float EdgeDepth(float FromEdge, float Reach, float Zone)
	{
		return FromEdge < Reach ? 0.3f + 0.7f * (1.f - FromEdge / Zone) : 0.f;
	}

	float GlideHeight(float Current, float Ground, float Dt)
	{
		return Current + (Ground - Current) * FMath::Min(1.f, Dt * HeightGlideRate);
	}

	float ZoomedDistance(float Dist, int32 Notch, float Widest)
	{
		return FMath::Min(Widest, FMath::Max(MinDistance, Dist * (1.f + FMath::Sign((float)Notch) * ZoomStep)));
	}

	bool ShiftToPixel(FVector2D Focus, float Height, float Yaw, float Pitch, float Dist, double VX, double VY, const FVector& P, FVector2D Pixel, FVector2D& OutFocus)
	{
		if (VX <= 0 || VY <= 0) return false;
		// client/camera.js shift: translate the ray through the pixel so it meets the anchor at the anchor's height
		const FTCPinhole V(FVector(Focus.X, Focus.Y, Height), Yaw, Pitch, Dist, VX / VY);
		FVector2D Hit;
		if (!V.Ground(Pixel, VX, VY, P.Z, Hit)) return false;
		OutFocus = Focus + FVector2D(P.X, P.Y) - Hit;
		return true;
	}

	float MiddleDragYawDegrees(float DxPixels)
	{
		return FMath::RadiansToDegrees(MiddleDragRadiansPerPixel * DxPixels);
	}
}

void FTCPointerEdge::Frame(bool bFocused, bool bHasPos, FVector2D NewPos, float W, float H, bool bPressed)
{
	// client/pointer.js: a blur (or a hidden tab) forgets the cursor until it moves again
	if (!bFocused) { Reset(); return; }
	const bool bInside = bHasPos && NewPos.X >= 0 && NewPos.Y >= 0 && NewPos.X < W && NewPos.Y < H;
	if (bHasPos && !bInside)
	{
		// a drag held past the edge (the press captured the mouse): pinned where it crossed
		if (Where == ETCPointerWhere::In) Leave(NewPos, true, W, H);
		Last = NewPos; bKnown = true;
		return;
	}
	if (!bHasPos)
	{
		// the window stopped reporting the cursor: it left the window (UE: the viewport's mouse-leave)
		if (Where == ETCPointerWhere::In) Leave(FVector2D::ZeroVector, false, W, H);
		bKnown = false;
		return;
	}
	const bool bMoved = bKnown && !NewPos.Equals(Last, 0.5);
	if (Where == ETCPointerWhere::In)
	{
		if (bMoved) Vel = NewPos - Pos;
		Pos = NewPos;
	}
	else if (bMoved || bPressed || (Where == ETCPointerWhere::Out && !NewPos.Equals(Last, 0.5)) || (Where == ETCPointerWhere::Out && !bKnown))
	{
		// seen moving (or pressing) in the window; or back in after leaving
		Where = ETCPointerWhere::In;
		Vel = FVector2D::ZeroVector;
		Pos = NewPos;
	}
	Last = NewPos;
	bKnown = true;
}

void FTCPointerEdge::Leave(FVector2D Exit, bool bExitKnown, float W, float H)
{
	// client/pointer.js leave: the side it went (the exit point when known), else the way it was last moving
	const double Dx = bExitKnown ? Exit.X - Pos.X : Vel.X, Dy = bExitKnown ? Exit.Y - Pos.Y : Vel.Y;
	double Px = Pos.X, Py = Pos.Y;
	if (Dx != 0 || Dy != 0)
	{
		double T = TNumericLimits<double>::Max();
		if (Dx > 0) T = (W - 1 - Pos.X) / Dx; else if (Dx < 0) T = -Pos.X / Dx;
		if (Dy > 0) T = FMath::Min(T, (H - 1 - Pos.Y) / Dy); else if (Dy < 0) T = FMath::Min(T, -Pos.Y / Dy);
		Px = Pos.X + Dx * T; Py = Pos.Y + Dy * T;
	}
	Px = FMath::Clamp(Px, 0.0, (double)W - 1); Py = FMath::Clamp(Py, 0.0, (double)H - 1);
	const double D[4] = { Px, W - 1 - Px, Py, H - 1 - Py };
	const double M = FMath::Min(FMath::Min(D[0], D[1]), FMath::Min(D[2], D[3]));
	if (M > 0.5)
	{
		if (M == D[0]) Px = 0; else if (M == D[1]) Px = W - 1; else if (M == D[2]) Py = 0; else Py = H - 1;
	}
	Pos = FVector2D(Px, Py);
	Where = ETCPointerWhere::Out;
}

FTCEdgePush FTCEdgeScroll::Step(const FTCEdgeInput& In, float Dt, bool bFollowing)
{
	FTCEdgePush Out;
	if (!In.bEnabled || !In.bActive || !In.bWorld || In.bBlocked || In.bButtons || In.bDragging || In.bMiddle || In.W <= 0 || In.H <= 0)
	{
		Reset();
		return Out;
	}
	// Over a HUD panel only the outer strip counts, so a button near the edge does not scroll the view while aimed at
	const float Zone = TCCameraFeel::EdgeZone(In.W, In.H);
	const float Reach = In.bOut || In.bOverView ? Zone : FMath::Max(6.f, Zone / 4.f);
	auto Depth = [&](float D) { return TCCameraFeel::EdgeDepth(D, Reach, Zone); };
	const float Rt = Depth(In.W - 1 - In.Pos.X) - Depth(In.Pos.X);
	const float Fw = Depth(In.Pos.Y) - Depth(In.H - 1 - In.Pos.Y);
	if (!Rt && !Fw) { Reset(); return Out; }
	if (bHold && bFollowing) return Out;
	bHold = false;
	Age += Dt;
	const float Ease = TCCameraFeel::Smoothstep(FMath::Min(1.f, Age / TCCameraFeel::EdgeEaseSeconds));
	Out.bPush = true;
	Out.Fw = Fw * Ease;
	Out.Rt = Rt * Ease;
	Out.Dir = FString(Fw > 0 ? TEXT("n") : Fw < 0 ? TEXT("s") : TEXT("")) + (Rt > 0 ? TEXT("e") : Rt < 0 ? TEXT("w") : TEXT(""));
	return Out;
}

bool FTCCameraIntro::Step(float Dt, FTCCameraPose& Out)
{
	if (!bActive) { Out = ToPose; return false; }
	Age = FMath::Min(TCCameraFeel::IntroSeconds, Age + Dt);
	const float T = Age / TCCameraFeel::IntroSeconds, E = TCCameraFeel::Smoothstep(T);
	if (T >= 1.f) { bActive = false; Out = ToPose; return false; }
	Out.Focus = FromPose.Focus + (ToPose.Focus - FromPose.Focus) * E;
	Out.Height = FromPose.Height + (ToPose.Height - FromPose.Height) * E;
	Out.Dist = FromPose.Dist + (ToPose.Dist - FromPose.Dist) * E;
	return true;
}

bool FTCCameraIntro::Skip(FTCCameraPose& Out)
{
	if (!bActive) return false;
	bActive = false;
	Out = ToPose;
	return true;
}

#if WITH_DEV_AUTOMATION_TESTS

// ThreeCrossroads.Camera.Feel: client/camera.js's numbers and rules, ported. Run:
//   UnrealEditor-Cmd ThreeCrossroads.uproject -nullrhi -ExecCmds="Automation RunTests ThreeCrossroads.Camera; Quit"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCCameraFeelTest, "ThreeCrossroads.Camera.Feel", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCCameraFeelTest::RunTest(const FString& Parameters)
{
	using namespace TCCameraFeel;
	// edgeZone: 3% of the short side within [24, 48] (32 px at 1920 x 1080, as camera.js's comment says, 32.4 exactly)
	TestEqual(TEXT("zone at 1920 x 1080"), EdgeZone(1920, 1080), 32.4f, 1e-4f);
	TestEqual(TEXT("zone floor"), EdgeZone(800, 600), 24.f);
	TestEqual(TEXT("zone ceiling"), EdgeZone(4000, 3000), 48.f);
	TestEqual(TEXT("full speed at the edge"), EdgeDepth(0, 32, 32), 1.f);
	TestEqual(TEXT("30% just inside the zone's inner border"), EdgeDepth(31.999f, 32, 32), 0.3f, 1e-3f);
	TestEqual(TEXT("nothing at the inner border"), EdgeDepth(32, 32, 32), 0.f);

	FTCEdgeInput In;
	In.bActive = In.bWorld = true;
	In.W = 1920; In.H = 1080;
	In.Pos = FVector2D(0, 540);
	FTCEdgeScroll E;
	FTCEdgePush P = E.Step(In, EdgeEaseSeconds / 2, false);
	TestTrue(TEXT("the left edge pushes"), P.bPush);
	TestEqual(TEXT("eased in: half the ease time gives half the speed"), P.Rt, -0.5f, 1e-4f);
	TestEqual(TEXT("left only"), P.Fw, 0.f);
	TestEqual(TEXT("direction west"), P.Dir, FString(TEXT("w")));
	P = E.Step(In, EdgeEaseSeconds / 2, false);
	TestEqual(TEXT("full speed once eased in"), P.Rt, -1.f, 1e-4f);
	In.Pos = FVector2D(1919, 0);
	P = E.Step(In, 1.f, false);
	TestTrue(TEXT("a corner pushes both ways"), P.Fw > 0.99f && P.Rt > 0.99f);
	TestEqual(TEXT("direction north-east"), P.Dir, FString(TEXT("ne")));
	// over a HUD panel only the outer strip counts: 10 px in is inside the 32.4 px zone but past its quarter (8.1 px)
	In.Pos = FVector2D(10, 540);
	In.bOverView = false;
	TestFalse(TEXT("over a panel, 10 px in does not push"), E.Step(In, 1.f, false).bPush);
	In.bOverView = true;
	P = E.Step(In, EdgeEaseSeconds / 2, false);
	TestTrue(TEXT("over the view it does"), P.bPush);
	TestEqual(TEXT("the ease restarted after the pause, on the ramp's depth 10 px in"), P.Rt, -0.5f * (0.3f + 0.7f * (1.f - 10.f / 32.4f)), 1e-4f);
	for (bool FTCEdgeInput::*Flag : { &FTCEdgeInput::bBlocked, &FTCEdgeInput::bButtons, &FTCEdgeInput::bDragging, &FTCEdgeInput::bMiddle })
	{
		FTCEdgeInput Stop = In;
		Stop.*Flag = true;
		TestFalse(TEXT("no push while blocked, a button is held, a drag or a rotation runs"), E.Step(Stop, 1.f, false).bPush);
	}
	FTCEdgeInput Off = In;
	Off.bEnabled = false;
	TestFalse(TEXT("the switch turns it off"), E.Step(Off, 1.f, false).bPush);
	// a follow that starts with the cursor in the zone waits until it leaves the zone
	E.HoldForFollow();
	TestFalse(TEXT("held while following"), E.Step(In, 1.f, true).bPush);
	FTCEdgeInput Middle = In;
	Middle.Pos = FVector2D(960, 540);
	TestFalse(TEXT("the middle of the window"), E.Step(Middle, 1.f, true).bPush);
	TestTrue(TEXT("a fresh push after that"), E.Step(In, 1.f, true).bPush);

	// client/pointer.js: none until seen moving, then in; leaving pins it to the border it was moving toward
	FTCPointerEdge Ptr;
	Ptr.Frame(true, true, FVector2D(900, 500), 1920, 1080);
	TestFalse(TEXT("a cursor never seen moving is not known"), Ptr.Active());
	Ptr.Frame(true, true, FVector2D(1900, 520), 1920, 1080);
	TestTrue(TEXT("seen moving: in"), Ptr.Where == ETCPointerWhere::In);
	Ptr.Frame(true, true, FVector2D(1910, 520), 1920, 1080);
	Ptr.Frame(true, false, FVector2D::ZeroVector, 1920, 1080);
	TestTrue(TEXT("left the window: out"), Ptr.Out());
	TestEqual(TEXT("pinned on the right border"), Ptr.Pos.X, 1919.0);
	TestEqual(TEXT("at the height it crossed"), Ptr.Pos.Y, 520.0);
	FTCEdgeInput Pinned;
	Pinned.bActive = Ptr.Active(); Pinned.bOut = Ptr.Out(); Pinned.bWorld = true; Pinned.Pos = Ptr.Pos; Pinned.W = 1920; Pinned.H = 1080;
	FTCEdgeScroll E2;
	TestEqual(TEXT("the pinned cursor keeps pushing right"), E2.Step(Pinned, 1.f, false).Rt, 1.f, 1e-4f);
	Ptr.Frame(true, true, FVector2D(1800, 600), 1920, 1080);
	TestTrue(TEXT("back in"), Ptr.Where == ETCPointerWhere::In);
	Ptr.Frame(false, true, FVector2D(1800, 600), 1920, 1080);
	TestFalse(TEXT("a blur forgets it"), Ptr.Active());
	Ptr.Frame(true, true, FVector2D(1800, 600), 1920, 1080);
	Ptr.Frame(true, true, FVector2D(1800, 600), 1920, 1080, true);
	TestTrue(TEXT("a press counts as seen"), Ptr.Active());
	Ptr.Frame(true, true, FVector2D(-40, 600), 1920, 1080);
	TestTrue(TEXT("a drag held past the left edge is out"), Ptr.Out() && Ptr.Pos.X == 0.0);

	// the intro: 2.5 s, eased; the halfway point is halfway; then the start view, and Skip does nothing more
	FTCCameraIntro Intro;
	FTCCameraPose From, To, Now;
	From.Focus = FVector2D(80, 80); From.Height = 0; From.Dist = 300;
	To.Focus = FVector2D(20, 140); To.Height = 5; To.Dist = 60;
	Intro.Begin(From, To);
	TestTrue(TEXT("gliding"), Intro.Step(IntroSeconds / 2, Now));
	TestEqual(TEXT("halfway distance"), Now.Dist, 180.f, 1e-3f);
	TestTrue(TEXT("halfway target"), Now.Focus.Equals(FVector2D(50, 110), 1e-3));
	TestEqual(TEXT("a quarter in is eased (15.6%)"), Smoothstep(0.25f), 0.15625f, 1e-6f);
	TestFalse(TEXT("done after 2.5 s"), Intro.Step(IntroSeconds / 2, Now));
	TestTrue(TEXT("at the start view"), Now.Focus.Equals(To.Focus) && Now.Dist == To.Dist && Now.Height == To.Height);
	TestFalse(TEXT("nothing to skip"), Intro.Skip(Now));
	Intro.Begin(From, To);
	TestTrue(TEXT("a key skips it"), Intro.Skip(Now) && Now.Dist == To.Dist && !Intro.IsActive());

	// the wheel: 10% a notch, 25 m minimum, the widest view maximum
	TestEqual(TEXT("out"), ZoomedDistance(100, 1, 400), 110.f, 1e-4f);
	TestEqual(TEXT("in"), ZoomedDistance(100, -1, 400), 90.f, 1e-4f);
	TestEqual(TEXT("25 m floor"), ZoomedDistance(26, -1, 400), 25.f);
	TestEqual(TEXT("widest ceiling"), ZoomedDistance(390, 1, 400), 400.f);
	TestEqual(TEXT("height glide, one frame at 60 fps"), GlideHeight(0, 10, 1.f / 60), 10.f * 4 / 60, 1e-4f);
	TestEqual(TEXT("height glide never overshoots"), GlideHeight(0, 10, 1.f), 10.f);
	TestEqual(TEXT("middle-drag: 100 px turn 0.6 rad"), MiddleDragYawDegrees(100), FMath::RadiansToDegrees(0.6f), 1e-4f);
	TestEqual(TEXT("pan speeds"), PanSpeedFactor(0) + PanSpeedFactor(1) + PanSpeedFactor(2), 3.2f, 1e-5f);

	// zoom at the cursor: after the shift the pixel's ray meets the anchor, at several poses and pixels
	const float Pitch = 54.43099f;
	const FVector Anchor(37, 52, 3.5);
	for (const FVector2D Pixel : { FVector2D(100, 200), FVector2D(1820, 1000), FVector2D(960, 540) })
		for (const float Yaw : { -90.f, 10.f, 145.f })
		{
			FVector2D Focus;
			TestTrue(TEXT("the shift is found"), ShiftToPixel(FVector2D(60, 60), 2.f, Yaw, Pitch, 75.f, 1920, 1080, Anchor, Pixel, Focus));
			const FTCPinhole V(FVector(Focus.X, Focus.Y, 2.f), Yaw, Pitch, 75.f, 1920.0 / 1080.0);
			FVector2D Hit, Back;
			TestTrue(TEXT("the ray meets the anchor's height"), V.Ground(Pixel, 1920, 1080, Anchor.Z, Hit));
			TestTrue(TEXT("the pixel looks at the anchor"), Hit.Equals(FVector2D(Anchor.X, Anchor.Y), 1e-3));
			TestTrue(TEXT("and the anchor projects back to the pixel"), V.Project(Anchor, 1920, 1080, Back) && Back.Equals(Pixel, 1e-2));
		}
	return true;
}

#endif
