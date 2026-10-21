#include "Client/TCHUD31Labels.h"

namespace
{
 struct FLabel { FBox2D Bounds; TFunction<void()> Paint; };
 // Canvas painting runs on the game thread. These entries live for one HUD frame.
 UCanvas* LabelCanvas = nullptr;
 bool Collecting = false;
 TArray<FLabel> Labels;
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
 Labels.Reset(); LabelCanvas = Canvas; Collecting = true;
}

void TCHud31::SubmitWorldLabel(UCanvas* Canvas, const FBox2D& Bounds, TFunction<void()> Paint)
{
 if (Collecting && Canvas == LabelCanvas) Labels.Add({Bounds, MoveTemp(Paint)});
 else Paint();
}

void TCHud31::FinishWorldLabels(const TArray<FBox2D>& Panels)
{
 Collecting = false; LabelCanvas = nullptr;
 TArray<FBox2D> Occupied = Panels;
 for (const FLabel& Label : Labels)
  if (WorldLabelVisible(Label.Bounds, Occupied))
  {
   Label.Paint(); Occupied.Add(Label.Bounds);
  }
 Labels.Reset();
}
