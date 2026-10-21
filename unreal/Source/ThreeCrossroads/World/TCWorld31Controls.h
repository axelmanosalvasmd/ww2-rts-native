// Startup-only renderer isolation. Each process changes one feature; no preference or match state is written.
#pragma once
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"

namespace TCWorld31Controls
{
	inline bool NoWater() { return FParse::Param(FCommandLine::Get(), TEXT("TCWorld31NoWater")); }
	inline bool LightGridCap() { return FParse::Param(FCommandLine::Get(), TEXT("TCWorld31LightGridCap")); }
	inline bool NoScatter() { return FParse::Param(FCommandLine::Get(), TEXT("TCWorld31NoScatter")); }
	inline bool NoShadows() { return FParse::Param(FCommandLine::Get(), TEXT("TCWorld31NoShadows")); }
}
