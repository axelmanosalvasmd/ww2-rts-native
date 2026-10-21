// Opt-in read-only renderer instrumentation, local log only.
#pragma once
#include "CoreTypes.h"
class UWorld;

struct FTCRender28Diagnostics
{
	// One frame of stock RDG transition logging at the existing settled tour pose.
	// Requires -TCRender28TransitionPose, -TCRender28TransitionPass and -TCRender28TransitionResource.
	// Pass/resource are bounded literal substrings from a previously inspected graph, never inverted or empty.
	static void CaptureTour(UWorld* World, const TCHAR* Pose);
};
