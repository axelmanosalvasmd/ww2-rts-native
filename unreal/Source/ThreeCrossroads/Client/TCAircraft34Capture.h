// Opt-in live aircraft capture. Consumes actual draws and never supplies match data.
#pragma once
#include "CoreMinimal.h"
class UWorld;
struct FTCFxAircraftDraw;
namespace TCAircraft34
{
 inline float CaptureDistance() { return 180.f; }
 struct FCapture
 {
  FString Prefix;
  bool Initialized=false;
  int32 Shot=0, Serial=0;
  float Age=0;
  void Frame(UWorld* World, const TArray<FTCFxAircraftDraw>& Aircraft, float Dt);
 };
}
