// Descriptor-only buffer inventory for attributing failed UAV/upload passes.
#pragma once

namespace TCRender27
{
	// A descriptor request never authorizes buffer binaries. Existing texture captures stay unchanged.
	constexpr int GpuBufferMode(bool bRequested, bool bTextureBinaries)
	{
		return bRequested ? (bTextureBinaries ? -1 : 1) : 0;
	}
}
