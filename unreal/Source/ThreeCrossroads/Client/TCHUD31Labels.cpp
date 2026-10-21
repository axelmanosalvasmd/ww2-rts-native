#include "Client/TCHUD31Labels.h"

namespace
{
 struct FLabel { FBox2D Bounds; TFunction<void()> Paint; bool bRelocateCovered; };
 // Canvas painting runs on the game thread. These entries live for one HUD frame.
 UCanvas* LabelCanvas = nullptr;
 bool Collecting = false;
 TArray<FLabel> Labels;
 TArray<FBox2D> WorldInformation;
 FBox2D Viewport(FVector2D::ZeroVector,FVector2D::ZeroVector);
 FVector2D PaintOffset=FVector2D::ZeroVector;
}

bool TCHud31::WorldLabelVisible(const FBox2D& Label, const TArray<FBox2D>& Panels, float Gap)
{
 if (!Label.bIsValid) return false;
 for (const FBox2D& Panel : Panels)
  if (Panel.bIsValid && Label.Min.X < Panel.Max.X + Gap && Label.Max.X > Panel.Min.X - Gap &&
      Label.Min.Y < Panel.Max.Y + Gap && Label.Max.Y > Panel.Min.Y - Gap) return false;
 return true;
}

void TCHud31::BeginWorldLabels(UCanvas* Canvas)
{
 BeginWorldLabels(Canvas,FBox2D(FVector2D::ZeroVector,FVector2D::ZeroVector));
}

void TCHud31::BeginWorldLabels(UCanvas* Canvas, const FBox2D& InViewport)
{
 Labels.Reset(); WorldInformation.Reset(); PaintOffset=FVector2D::ZeroVector;
 Viewport=InViewport; LabelCanvas=Canvas; Collecting=true;
}

void TCHud31::ProtectWorldInformation(UCanvas* Canvas, const FBox2D& Bounds)
{
 if (Collecting && Canvas==LabelCanvas && Bounds.bIsValid) WorldInformation.Add(Bounds);
}

FVector2D TCHud31::WorldLabelOffset() { return PaintOffset; }

void TCHud31::SubmitWorldLabel(UCanvas* Canvas, const FBox2D& Bounds, TFunction<void()> Paint, bool bRelocateCovered)
{
 if (Collecting && Canvas == LabelCanvas) Labels.Add({Bounds, MoveTemp(Paint), bRelocateCovered});
 else Paint();
}

void TCHud31::FinishWorldLabels(const TArray<FBox2D>& Panels)
{
 Collecting = false; LabelCanvas = nullptr;
 TArray<FBox2D> Occupied = Panels;
 Occupied.Append(WorldInformation);
 for (const FLabel& Label : Labels)
 {
  // HQ anchors stay hidden under panels. Income/support may relocate, always outside every panel.
  if (!Label.bRelocateCovered && !WorldLabelVisible(Label.Bounds,Panels)) continue;
  auto Try=[&](FVector2D Offset)
  {
   const FBox2D Bounds(Label.Bounds.Min+Offset,Label.Bounds.Max+Offset);
   if (Viewport.Max.X>Viewport.Min.X && (Bounds.Min.X<Viewport.Min.X+6 || Bounds.Min.Y<Viewport.Min.Y+6 ||
       Bounds.Max.X>Viewport.Max.X-6 || Bounds.Max.Y>Viewport.Max.Y-6)) return false;
   if (!WorldLabelVisible(Bounds,Occupied)) return false;
   PaintOffset=Offset; Label.Paint(); PaintOffset=FVector2D::ZeroVector;
   Occupied.Add(Bounds); return true;
  };
  if (Try(FVector2D::ZeroVector)) continue;
  const double StepY=FMath::Max(24.0,Label.Bounds.Max.Y-Label.Bounds.Min.Y+12.0);
  const double StepX=Label.Bounds.Max.X-Label.Bounds.Min.X+12.0;
  // Bounded deterministic search, vertical first to keep the caption near its ground anchor.
  for (int Ring=1;Ring<=6;Ring++)
  {
   bool Placed=false;
   for (const FVector2D Direction : {FVector2D(0,-1),FVector2D(0,1),FVector2D(-1,0),FVector2D(1,0),
                                    FVector2D(-1,-1),FVector2D(1,-1),FVector2D(-1,1),FVector2D(1,1)})
    if (Try(FVector2D(Direction.X*StepX*Ring,Direction.Y*StepY*Ring))) { Placed=true; break; }
   if (Placed) break;
  }
 }
 Labels.Reset(); WorldInformation.Reset();
}
