// Original #controlsSheet content and CSS layout. The sheet is outside #hud's zoom.
#include "Client/TCHUD.h"
#include "Client/TCStats.h"
#include "Client/TCLocale.h"

namespace TCHud26Controls
{
 struct FRow { FString Keys, Description; };
 struct FSection { FString Title; TArray<FRow> Rows; };
#include "Client/TCHUD26ControlsData.inl"
 struct FRun { FString Shown; bool bKey=false, bSpace=false; };
 constexpr float BodyPx=14, BodyLine=18.890625f, KeyPx=13, KeyLine=16.890625f;
 constexpr float HeadingLine=21.59375f;
 float Snap(float Value) { return FMath::FloorToFloat(Value*64)/64; }
 FBox2D Box(float X,float Y,float W,float H) { return FBox2D(FVector2D(X,Y),FVector2D(X+W,Y+H)); }

 TArray<FRun> Runs(const FString& Html)
 {
  TArray<FRun> Out;
  bool Space=false;
  auto Plain=[&](const FString& English)
  {
   const FString Shown=TC_T(English);
   FString Word;
   auto Flush=[&]() { if (!Word.IsEmpty()) { Out.Add({Word,false,Space}); Word.Reset(); Space=false; } };
   for (TCHAR Ch : Shown) { if (FChar::IsWhitespace(Ch)) { Flush(); Space=true; } else Word.AppendChar(Ch); }
   Flush();
  };
  int32 At=0;
  while (At<Html.Len())
  {
   const int32 Open=Html.Find(TEXT("<kbd>"),ESearchCase::CaseSensitive,ESearchDir::FromStart,At);
   if (Open==INDEX_NONE) { Plain(Html.Mid(At)); break; }
   Plain(Html.Mid(At,Open-At));
   const int32 Close=Html.Find(TEXT("</kbd>"),ESearchCase::CaseSensitive,ESearchDir::FromStart,Open+5);
   if (Close==INDEX_NONE) break; // the generated, pinned markup is balanced
   Out.Add({TC_T(Html.Mid(Open+5,Close-Open-5)),true,Space}); Space=false; At=Close+6;
  }
  return Out;
 }

 float RunWidth(const FRun& Run,const FTCMeasure& Measure)
 { return Measure(Run.Shown,Run.bKey?KeyPx:BodyPx,Run.bKey,0)+(Run.bKey?10.f:0.f); }
 float Natural(const FString& Html,const FTCMeasure& Measure)
 {
  float Width=0;
  for (const FRun& Run : Runs(Html)) Width+=RunWidth(Run,Measure)+(Run.bSpace?Measure(TEXT(" "),BodyPx,false,0):0);
  return Width;
 }

 // Translate original text nodes separately, as i18n.js does, and preserve the original key caps.
 float Flow(FTCPaint* Paint,const FString& Html,float X,float Y,float W,const FTCMeasure& Measure,FLinearColor Colour,const FBox2D& Clip)
 {
  float Pen=0,LineY=0;
  for (const FRun& Run : Runs(Html))
  {
   const float Width=RunWidth(Run,Measure),Space=Pen>0 && Run.bSpace?Measure(TEXT(" "),BodyPx,false,0):0;
   if (Pen>0 && Pen+Space+Width>W+.01f) { Pen=0; LineY+=BodyLine; }
   else Pen+=Space;
   if (Paint)
   {
    if (Run.bKey)
    {
     FTCPaintItem& Frame=Paint->Frame(X+Pen,Y+LineY,Width,KeyLine+2,TCCss::LineSoft,1,TEXT("controls:keycap")); Frame.Clip=Clip;
    }
    FTCPaintItem& Text=Paint->Text(Run.Shown,X+Pen+(Run.bKey?5:0),Y+LineY+(Run.bKey?1:0),Run.bKey?KeyPx:BodyPx,Run.bKey?TCCss::Dim:Colour,Run.bKey,ETCPaintAlign::Left,TEXT("controls:body"));
    Text.Size.Y=Run.bKey?KeyLine:BodyLine; Text.Clip=Clip;
   }
   Pen+=Width;
  }
  return LineY+BodyLine;
 }
}

FTCHudFit::FControlsLayout FTCHudFit::ControlsDialog(FVector2D View,const FTCMeasure& Measure,float Scroll)
{
 using namespace TCHud26Controls;
 FControlsLayout Out;
 const bool Narrow=View.X<=560;
 const float Pad=Narrow?12:16,Gap=Narrow?12:14;
 const int32 Columns=Narrow?1:2;
 const float W=Narrow?float(View.X)-16:FMath::Min(720.f,float(View.X)-24);
 const float MaxH=Snap(Narrow?float(View.Y)-16:FMath::Min(float(View.Y)*.82f,760.f));
 const float HeaderH=(Narrow?20:24)+30.890625f+1,AfterHeader=Narrow?10:12;
 // Source uses scrollbar-width:thin (10px on this original Chromium capture).
 const float Scrollbar=10,ColumnW=(W-2-2*Pad-Scrollbar-(Columns-1)*24)/Columns;
 TArray<float> KeyWidths,Heights;
 for (const FSection& Section : SourceSections)
 {
  float KeyWidth=92;
  for (const FRow& Row : Section.Rows) KeyWidth=FMath::Max(KeyWidth,Natural(Row.Keys,Measure));
  KeyWidth=FMath::Min(KeyWidth,ColumnW-10);
  float Height=HeadingLine+6;
  for (const FRow& Row : Section.Rows)
   Height+=FMath::Max(Flow(nullptr,Row.Keys,0,0,KeyWidth,Measure,TCCss::Dim,FBox2D(ForceInit)),Flow(nullptr,Row.Description,0,0,ColumnW-KeyWidth-10,Measure,TCCss::Text,FBox2D(ForceInit)))+5;
  KeyWidths.Add(KeyWidth); Heights.Add(Height-5);
 }
 float GridH=0;
 for (int32 I=0;I<Heights.Num();I+=Columns)
 {
  float Height=0; for (int32 K=I;K<FMath::Min(I+Columns,Heights.Num());K++) Height=FMath::Max(Height,Heights[K]);
  GridH+=Height+(I?Gap:0);
 }
 const float FullH=HeaderH+AfterHeader+GridH+Pad,H=FMath::Min(MaxH,FullH+2);
 const float X=(View.X-W)/2,Y=(View.Y-H)/2;
 Out.Bounds=Box(X,Y,W,H); Out.Viewport=Box(X+1,Y+1,W-2,H-2);
 Out.ScrollMax=FMath::Max(0.f,FullH-H); Scroll=FMath::Clamp(Scroll,0.f,Out.ScrollMax);
 Out.Paint.Rect(0,0,View.X,View.Y,TCCss::Rgba(8,10,11,.72f),TEXT("controls:backdrop"));
 Out.Paint.Rect(X,Y,W,H,TCCss::PanelSolid,TEXT("controls:panel")); Out.Paint.Frame(X,Y,W,H,TCCss::Line,1);
 const FBox2D BodyClip=Box(X+1,Y+1+HeaderH,W-2-Scrollbar,H-2-HeaderH);
 float RowY=Y+1+HeaderH+AfterHeader-Scroll;
 for (int32 I=0;I<SourceSections.Num();I+=Columns)
 {
  float RowH=0;
  for (int32 K=I;K<FMath::Min(I+Columns,SourceSections.Num());K++)
  {
   const FSection& Section=SourceSections[K]; const float Left=X+1+Pad+(K-I)*(ColumnW+24);
   FTCPaintItem& Title=Out.Paint.Text(TC_T(Section.Title),Left,RowY,16,TCCss::Brass,true,ETCPaintAlign::Left,TEXT("controls:section:title")); Title.Size.Y=HeadingLine; Title.Clip=BodyClip;
   float TextY=RowY+HeadingLine+6;
   for (const FRow& Row : Section.Rows)
   {
    const float KeyH=Flow(&Out.Paint,Row.Keys,Left,TextY,KeyWidths[K],Measure,TCCss::Dim,BodyClip);
    const float BodyH=Flow(&Out.Paint,Row.Description,Left+KeyWidths[K]+10,TextY,ColumnW-KeyWidths[K]-10,Measure,TCCss::Text,BodyClip);
    TextY+=FMath::Max(KeyH,BodyH)+5;
   }
   RowH=FMath::Max(RowH,Heights[K]);
  }
  RowY+=RowH+Gap;
 }
 // The original header stays above scrolled rows. Close retains its literal Esc key cap.
 Out.Paint.Rect(X+1,Y+1,W-2-Scrollbar,HeaderH,TCCss::PanelSolid,TEXT("controls:header"));
 Out.Paint.Rect(X+1,Y+HeaderH,W-2-Scrollbar,1,TCCss::LineSoft);
 FTCPaintItem& Title=Out.Paint.Text(TC_T(FString(TEXT("Controls"))),X+1+Pad,Y+1+(HeaderH-28.34375f)/2,21,TCCss::Text,true,ETCPaintAlign::Left,TEXT("controls:title")); Title.Size.Y=28.34375f;
 const float ButtonW=Natural(TEXT("Close <kbd>Esc</kbd>"),Measure)+22;
 Out.Close=Box(X+W-1-Scrollbar-Pad-ButtonW,Y+1+(Narrow?10:12),ButtonW,30.890625f);
 Out.Paint.Rect(Out.Close.Min.X,Out.Close.Min.Y,ButtonW,Out.Close.GetSize().Y,TCCss::Hex(0x24282b),TEXT("controls:close"));
 Out.Paint.Frame(Out.Close.Min.X,Out.Close.Min.Y,ButtonW,Out.Close.GetSize().Y,TCCss::LineSoft,1);
 Flow(&Out.Paint,TEXT("Close <kbd>Esc</kbd>"),Out.Close.Min.X+11,Out.Close.Min.Y+6,ButtonW-22,Measure,TCCss::Text,Out.Close);
 if (Out.ScrollMax>0)
 {
  const float Thumb=FMath::Max(20.f,(H-2)*(H-2)/FullH);
  Out.Paint.Rect(X+W-6,Y+1+(H-2-Thumb)*Scroll/Out.ScrollMax,5,Thumb,TCCss::Rgba(176,164,122,.45f),TEXT("controls:scroll"));
 }
 Out.Paint.Panels.Add(Box(0,0,View.X,View.Y));
 return Out;
}
