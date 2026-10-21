// Local diagnostics only. No receipt is sent to a server or stored in player preferences.
#pragma once
#include "CoreTypes.h"
class UWorld;

struct FTCRender26Diagnostics
{
	// -TCRender26Receipts: at most 64 one-second samples per process in its ordinary log.
	// Add -TCRender26Allocations for two engine allocation inventories. No GPU readback is requested.
	void Tick(UWorld* World);
	// Lead tour adapter only: one opt-in stock DumpGPU request at a named, settled camera pose.
	// -TCRender27GpuBufferDescriptors includes buffer metadata in a descriptor-only capture, never binaries.
	static void CaptureTour(UWorld* World, const TCHAR* Pose);
};
