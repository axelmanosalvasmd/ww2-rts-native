#pragma once

#include "CoreMinimal.h"

class UCanvas;

namespace TCHud31
{
 // Physical viewport pixels. A small gap keeps tags from touching a panel edge.
 bool WorldLabelVisible(const FBox2D& Label, const TArray<FBox2D>& Panels, float Gap = 6.f);
 void BeginWorldLabels(UCanvas* Canvas);
 void SubmitWorldLabel(UCanvas* Canvas, const FBox2D& Bounds, TFunction<void()> Paint);
 void FinishWorldLabels(const TArray<FBox2D>& Panels);
}
