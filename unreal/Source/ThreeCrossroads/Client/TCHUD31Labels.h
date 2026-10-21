#pragma once

#include "CoreMinimal.h"

class UCanvas;
struct FLinearColor;

#define TC_HUD33_ASSOCIATION 1

namespace TCHud31
{
 struct FWorldLabelSegment { FVector2D Start,End; };
 struct FWorldLabelAssociation
 {
  FVector2D Anchor=FVector2D::ZeroVector;
  TArray<FWorldLabelSegment> Segments;
  bool bEndpoint=false;
 };
 // Valid only during a queued paint callback. No anchor is inferred from caption placement.
 const FWorldLabelAssociation& WorldLabelAssociation();
 void SubmitAnchoredWorldLabel(UCanvas* Canvas,const FBox2D& Bounds,FVector2D GroundAnchor,
  bool bAnchorPublic,TFunction<void()> Paint);
 void DrawWorldLabelAssociation(UCanvas* Canvas,const FLinearColor& Colour);
 // Physical viewport pixels. A small gap keeps tags from touching a panel edge.
 bool WorldLabelVisible(const FBox2D& Label, const TArray<FBox2D>& Panels, float Gap = 6.f);
 void BeginWorldLabels(UCanvas* Canvas);
 void BeginWorldLabels(UCanvas* Canvas, const FBox2D& Viewport);
 // Unit information stays in place. Captions find a clear nearby position instead.
 void ProtectWorldInformation(UCanvas* Canvas, const FBox2D& Bounds);
 // Billboards retain their world position and are culled against this frame's HUD panels.
 void SubmitWorldInformation(UCanvas* Canvas, const FBox2D& Bounds, TFunction<void()> Paint);
 // Valid only inside the queued paint callback, in physical viewport pixels.
 FVector2D WorldLabelOffset();
 void SubmitWorldLabel(UCanvas* Canvas, const FBox2D& Bounds, TFunction<void()> Paint, bool bRelocateCovered = true);
 void FinishWorldLabels(const TArray<FBox2D>& Panels);
}
