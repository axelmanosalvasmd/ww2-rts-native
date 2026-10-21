#include "Client/TCHUD31Labels.h"
#include "Engine/Canvas.h"
#include "CanvasItem.h"
#include "GlobalRenderResources.h"

void TCHud31::DrawWorldLabelAssociation(UCanvas* Canvas,const FLinearColor& Colour)
{
 if (!Canvas || !Canvas->Canvas || Colour.A<=0.01f) return;
 const auto& Link=WorldLabelAssociation();
 auto Line=[&](FVector2D A,FVector2D B)
 {
  const FVector2D D=B-A;
  const double Length=FMath::Sqrt(D.X*D.X+D.Y*D.Y);
  if (Length<.01) return;
  auto Stroke=[&](double Width,const FLinearColor& C)
  {
   const FVector2D N(-D.Y*Width/(2*Length),D.X*Width/(2*Length));
   TArray<FCanvasUVTri> Tris;
   auto Triangle=[&](FVector2D P0,FVector2D P1,FVector2D P2)
   {
    FCanvasUVTri T; T.V0_Pos=P0; T.V1_Pos=P1; T.V2_Pos=P2;
    T.V0_Color=T.V1_Color=T.V2_Color=C; Tris.Add(T);
   };
   Triangle(A-N,B-N,B+N); Triangle(A-N,B+N,A+N);
   FCanvasTriangleItem Item(Tris,GWhiteTexture); Item.BlendMode=SE_BLEND_Translucent;
   Canvas->DrawItem(Item);
  };
  // Translucent triangles retain the caption fade, unlike opaque Canvas line items.
  Stroke(3.5,FLinearColor(.015f,.02f,.025f,Colour.A*.9f)); Stroke(1.5,Colour);
 };
 for (const auto& Segment:Link.Segments) Line(Segment.Start,Segment.End);
 if (Link.bEndpoint)
 {
  // Hollow ground diamond, never a unit badge or a filled target obscuring the ground.
  const FVector2D A=Link.Anchor;
  Line(A+FVector2D(0,-4),A+FVector2D(4,0)); Line(A+FVector2D(4,0),A+FVector2D(0,4));
  Line(A+FVector2D(0,4),A+FVector2D(-4,0)); Line(A+FVector2D(-4,0),A+FVector2D(0,-4));
 }
}
