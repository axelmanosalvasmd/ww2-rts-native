#include "Client/TCHUD31Labels.h"

namespace
{
 struct FLabel { FBox2D Bounds; TFunction<void()> Paint; bool bRelocateCovered; FVector2D Anchor; bool bAnchorPublic=false; };
 // Canvas painting runs on the game thread. These entries live for one HUD frame.
 UCanvas* LabelCanvas = nullptr;
 bool Collecting = false;
 TArray<FLabel> Labels;
 struct FInformation { FBox2D Bounds; TFunction<void()> Paint; };
 TArray<FInformation> Information;
 TArray<FBox2D> WorldInformation;
 FBox2D Viewport(FVector2D::ZeroVector,FVector2D::ZeroVector);
 TCHud31::FWorldLabelAssociation Association;
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
 Labels.Reset(); Information.Reset(); WorldInformation.Reset(); PaintOffset=FVector2D::ZeroVector; Association={};
 Viewport=InViewport; LabelCanvas=Canvas; Collecting=true;
}

void TCHud31::ProtectWorldInformation(UCanvas* Canvas, const FBox2D& Bounds)
{
 if (Collecting && Canvas==LabelCanvas && Bounds.bIsValid) WorldInformation.Add(Bounds);
}

FVector2D TCHud31::WorldLabelOffset() { return PaintOffset; }

void TCHud31::SubmitWorldInformation(UCanvas* Canvas, const FBox2D& Bounds, TFunction<void()> Paint)
{
 if (Collecting && Canvas==LabelCanvas) Information.Add({Bounds,MoveTemp(Paint)});
 else Paint();
}

void TCHud31::SubmitWorldLabel(UCanvas* Canvas, const FBox2D& Bounds, TFunction<void()> Paint, bool bRelocateCovered)
{
 if (Collecting && Canvas == LabelCanvas) Labels.Add({Bounds, MoveTemp(Paint), bRelocateCovered, FVector2D::ZeroVector, false});
 else Paint();
}

const TCHud31::FWorldLabelAssociation& TCHud31::WorldLabelAssociation() { return Association; }

void TCHud31::SubmitAnchoredWorldLabel(UCanvas* Canvas,const FBox2D& Bounds,FVector2D GroundAnchor,
 bool bAnchorPublic,TFunction<void()> Paint)
{
 if (Collecting && Canvas==LabelCanvas) Labels.Add({Bounds,MoveTemp(Paint),true,GroundAnchor,bAnchorPublic});
 else Paint();
}

namespace
{
 // Liang-Barsky interval inside a rectangle. Clipping never invents a substitute endpoint.
 bool InsideInterval(FVector2D A,FVector2D B,const FBox2D& Box,double& Lo,double& Hi)
 {
  Lo=0; Hi=1;
  const FVector2D D=B-A;
  auto Axis=[&](double P,double V,double Min,double Max)
  {
   if (V==0) return P>=Min && P<=Max;
   double T0=(Min-P)/V,T1=(Max-P)/V;
   if (T0>T1) {const double T=T0; T0=T1; T1=T;}
   Lo=FMath::Max(Lo,T0); Hi=FMath::Min(Hi,T1); return Lo<=Hi;
  };
  return Box.bIsValid && Axis(A.X,D.X,Box.Min.X,Box.Max.X) && Axis(A.Y,D.Y,Box.Min.Y,Box.Max.Y);
 }
 void Associate(const FLabel& Label,const FBox2D& Placed,const TArray<FBox2D>& Blocks)
 {
  Association={};
  if (!Label.bAnchorPublic || Label.Anchor.X<Viewport.Min.X+6 || Label.Anchor.Y<Viewport.Min.Y+6 ||
      Label.Anchor.X>Viewport.Max.X-6 || Label.Anchor.Y>Viewport.Max.Y-6) return;
  const FVector2D A=Label.Anchor;
  const FVector2D Start(FMath::Max(Placed.Min.X,FMath::Min(A.X,Placed.Max.X)),
                        FMath::Max(Placed.Min.Y,FMath::Min(A.Y,Placed.Max.Y)));
  // The original lifted tag also needs a ground association, even without collision relocation.
  if (Start.X==A.X && Start.Y==A.Y) return;
  Association.Anchor=A;
  Association.bEndpoint=TCHud31::WorldLabelVisible(FBox2D(A-FVector2D(6,6),A+FVector2D(6,6)),Blocks,0);
  TArray<TCHud31::FWorldLabelSegment> Pieces={{Start,A}};
  for (const FBox2D& Block:Blocks)
  {
   TArray<TCHud31::FWorldLabelSegment> Next;
   const FBox2D Padded(Block.Min-FVector2D(2,2),Block.Max+FVector2D(2,2));
   for (const auto& Piece:Pieces)
   {
    double Lo,Hi;
    if (!InsideInterval(Piece.Start,Piece.End,Padded,Lo,Hi)) {Next.Add(Piece);continue;}
    const FVector2D D=Piece.End-Piece.Start;
    if (Lo>0) Next.Add({Piece.Start,Piece.Start+D*Lo});
    if (Hi<1) Next.Add({Piece.Start+D*Hi,Piece.End});
   }
   Pieces=MoveTemp(Next);
  }
  Association.Segments=MoveTemp(Pieces);
 }
}

void TCHud31::FinishWorldLabels(const TArray<FBox2D>& Panels)
{
 Collecting = false; LabelCanvas = nullptr;
 // Preserve the submitted cross-unit layer order. Covered billboards stay at their world anchors.
 for (const auto& Item:Information) if (WorldLabelVisible(Item.Bounds,Panels)) Item.Paint();
 Information.Reset();
 TArray<FBox2D> LinkBlocks=Panels;
 LinkBlocks.Append(WorldInformation);
 TArray<FBox2D> Occupied = Panels;
 Occupied.Append(WorldInformation);
 TArray<FBox2D> AnchorSpaces;
 for (const auto& Label:Labels) if (Label.bAnchorPublic)
  AnchorSpaces.Add(FBox2D(Label.Anchor-FVector2D(6,6),Label.Anchor+FVector2D(6,6)));
 for (const FLabel& Label : Labels)
 {
  // HQ anchors stay hidden under panels. Income/support may relocate, always outside every panel.
  if (!Label.bRelocateCovered && !WorldLabelVisible(Label.Bounds,Panels)) continue;
  auto Try=[&](FVector2D Offset)
  {
   const FBox2D Bounds(Label.Bounds.Min+Offset,Label.Bounds.Max+Offset);
   if (Viewport.Max.X>Viewport.Min.X && (Bounds.Min.X<Viewport.Min.X+6 || Bounds.Min.Y<Viewport.Min.Y+6 ||
       Bounds.Max.X>Viewport.Max.X-6 || Bounds.Max.Y>Viewport.Max.Y-6)) return false;
   if (!WorldLabelVisible(Bounds,Occupied) || !WorldLabelVisible(Bounds,AnchorSpaces)) return false;
   PaintOffset=Offset; Associate(Label,Bounds,LinkBlocks); Label.Paint(); Association={}; PaintOffset=FVector2D::ZeroVector;
   Occupied.Add(Bounds); LinkBlocks.Add(Bounds); return true;
  };
  if (Try(FVector2D::ZeroVector)) continue;
  const double StepY=FMath::Max(24.0,Label.Bounds.Max.Y-Label.Bounds.Min.Y+12.0);
  const double StepX=Label.Bounds.Max.X-Label.Bounds.Min.X+12.0;
  // Bounded deterministic search, vertical first to keep the caption near its ground anchor.
  bool Placed=false;
  for (int Ring=1;Ring<=6;Ring++)
  {
   for (const FVector2D Direction : {FVector2D(0,-1),FVector2D(0,1),FVector2D(-1,0),FVector2D(1,0),
                                    FVector2D(-1,-1),FVector2D(1,-1),FVector2D(-1,1),FVector2D(1,1)})
    if (Try(FVector2D(Direction.X*StepX*Ring,Direction.Y*StepY*Ring))) { Placed=true; break; }
   if (Placed) break;
  }
  if (Placed) continue;
  // Wide complete translations can exceed the radial search. Test obstacle edges and choose
  // the closest legal placement in the actual physical viewport, without shrinking the caption.
  const FVector2D Size=Label.Bounds.Max-Label.Bounds.Min;
  TArray<double> X={Viewport.Min.X+6,Viewport.Max.X-6-Size.X,Label.Bounds.Min.X};
  TArray<double> Y={Viewport.Min.Y+6,Viewport.Max.Y-6-Size.Y,Label.Bounds.Min.Y};
  TArray<FBox2D> PlacementBlocks=Occupied; PlacementBlocks.Append(AnchorSpaces);
  for (const auto& Block:PlacementBlocks)
  {
   X.Add(Block.Min.X-6-Size.X); X.Add(Block.Max.X+6);
   Y.Add(Block.Min.Y-6-Size.Y); Y.Add(Block.Max.Y+6);
  }
  double Best=1.e100; FVector2D BestOffset;
  for (double Px:X) for (double Py:Y)
  {
   const FVector2D Min(Px,Py),Max=Min+Size,Offset=Min-Label.Bounds.Min;
   const double Distance=Offset.X*Offset.X+Offset.Y*Offset.Y;
   if (Distance>=Best || Min.X<Viewport.Min.X+6 || Min.Y<Viewport.Min.Y+6 ||
       Max.X>Viewport.Max.X-6 || Max.Y>Viewport.Max.Y-6 || !WorldLabelVisible(FBox2D(Min,Max),PlacementBlocks)) continue;
   Best=Distance; BestOffset=Offset;
  }
  if (Best<1.e100) Try(BestOffset);
 }
 Labels.Reset(); WorldInformation.Reset();
}
