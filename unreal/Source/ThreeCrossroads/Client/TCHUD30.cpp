#include "Client/TCStats.h"
#include "Client/TCHUD.h"
#include "Client/TCLocale.h"
#include "Engine/Canvas.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"

FTCHudFit::FBottomLayout FTCHudFit::PortraitGallery(const FTCMeasure& Measure)
{
 FBottomLayout L;
 const float X=280,Y=240,W=620,H=350;
 L.Paint.Rect(X,Y,W,H,TCCss::Hex(0x191c1e),TEXT("portraits:gallery"));
 L.Paint.Frame(X,Y,W,H,TCCss::Line);
 L.Paint.Text(TEXT("Reference model portraits"),X+10,Y+8,16,TCCss::Brass,true);
 const TCHAR* Types[]={TEXT("rifle"),TEXT("tank"),TEXT("howitzer"),TEXT("fighter"),TEXT("destroyer")};
 for (int32 Faction=0;Faction<3;Faction++) for (int32 Family=0;Family<5;Family++)
 {
  const float Left=X+10+Family*120,Top=Y+40+Faction*100;
  FIcon I; I.Key=Types[Family]; I.Box=FBox2D(FVector2D(Left,Top),FVector2D(Left+112,Top+78));
  I.bPortrait=true; I.Owner=Faction; I.Faction=Faction; L.Icons.Add(I);
  L.Paint.Rect(Left,Top,112,78,TCCss::Hex(0x24282b));
  L.Paint.Text(FTCPaint::Ellipsis(FString::Printf(TEXT("%s / %d"),Types[Family],Faction+1),112,12,false,Measure),Left,Top+79,12,TCCss::Text);
 }
 return L;
}

void ATCHUD::DrawPortraitGallery(UFont* Font)
{
 if (!Canvas || !FParse::Param(FCommandLine::Get(),TEXT("TCHudPortraitGallery"))) return;
 // Acceptance-only sheet. It uses the same portrait queue and original-symbol fallback as the selection/recruit cards.
 DrawBottomLayout(FTCHudFit::PortraitGallery(FTCType::IsLoaded()?FTCType::Measure():FTCPaint::ApproxMeasure()),Font);
}
