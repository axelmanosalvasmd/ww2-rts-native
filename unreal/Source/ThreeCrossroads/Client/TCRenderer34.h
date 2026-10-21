#pragma once
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
class UWorld;
namespace TCRenderer34
{
    inline bool FullResolution() { return FParse::Param(FCommandLine::Get(), TEXT("TCRenderer34FullResolution")); }
    inline bool HighRelief() { return FParse::Param(FCommandLine::Get(), TEXT("TCRenderer34HighRelief")); }
    inline bool NoWeatherMirror() { return FParse::Param(FCommandLine::Get(), TEXT("TCRenderer34NoWeatherMirror")); }
    void Tick(UWorld* World);
}
