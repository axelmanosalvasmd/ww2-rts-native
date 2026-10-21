#pragma once

#include "CoreMinimal.h"

class UCanvas;

namespace TCHud31
{
 // Physical viewport pixels. A small gap keeps tags from touching a panel edge.
 bool WorldLabelVisible(const FBox2D& Label, const TArray<FBox2D>& Panels, float Gap = 6.f);
 void BeginWorldLabels(UCanvas* Canvas);
 void BeginWorldLabels(UCanvas* Canvas, const FBox2D& Viewport);
 // Unit information stays in place. Captions find a clear nearby position instead.
 void ProtectWorldInformation(UCanvas* Canvas, const FBox2D& Bounds);
 // Valid only inside the queued paint callback, in physical viewport pixels.
 FVector2D WorldLabelOffset();
 void SubmitWorldLabel(UCanvas* Canvas, const FBox2D& Bounds, TFunction<void()> Paint, bool bRelocateCovered = true);
 void FinishWorldLabels(const TArray<FBox2D>& Panels);
}
