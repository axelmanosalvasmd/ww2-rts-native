#pragma once
#include "Client/TCHUD31Labels.h"

namespace TCHud34Test
{
 template<class FCheck> void Run(FCheck Check)
 {
  for (double Scale : {1.0,1.5})
  {
   auto Box=[&](double X,double Y,double W,double H)
   { return FBox2D(FVector2D(X,Y)*Scale,FVector2D(X+W,Y+H)*Scale); };
   const FBox2D Icon=Box(450,280,50,25), Panel=Box(850,10,420,90);
   int Icons=0,Labels=0;
   TCHud31::BeginWorldLabels(nullptr,Box(0,0,1280,720));
   TCHud31::ProtectWorldInformation(nullptr,Icon);
   TCHud31::SubmitWorldInformation(nullptr,Icon,[&](){++Icons;});
   TCHud31::SubmitWorldInformation(nullptr,Box(900,45,45,30),[&](){Icons+=10;});
   // A badge can straddle a panel while its centre remains outside it.
   TCHud31::SubmitWorldInformation(nullptr,Box(825,45,35,30),[&](){Icons+=100;});
   Check("icons await current-frame panel bounds",Icons==0);
   TCHud31::SubmitWorldLabel(nullptr,Box(450,280,140,30),[&]()
   {
    const auto Offset=TCHud31::WorldLabelOffset();
    Check("income caption offsets clear of unit badge",TCHud31::WorldLabelVisible(FBox2D(Box(450,280,140,30).Min+Offset,Box(450,280,140,30).Max+Offset),{Icon}));
    ++Labels;
   });
   // The ground association crosses the badge unless unit bounds also clip leaders.
   TCHud31::SubmitAnchoredWorldLabel(nullptr,Box(350,200,100,25),FVector2D(550,360)*Scale,true,[&]()
   {
    for (const auto& Segment:TCHud31::WorldLabelAssociation().Segments)
     for (int I=0;I<=100;I++)
     {
      const auto P=Segment.Start+(Segment.End-Segment.Start)*(I/100.0);
      Check("leader stays clear of unit badge",!(P.X>Icon.Min.X && P.X<Icon.Max.X && P.Y>Icon.Min.Y && P.Y<Icon.Max.Y));
     }
    ++Labels;
   });
   TCHud31::FinishWorldLabels({Panel});
   Check("covered and partially covered plane badges are culled",Icons==1);
   Check("income and support captions both survive",Labels==2);
   TCHud31::FinishWorldLabels({});
   Check("billboards are never replayed",Icons==1);
   TCHud31::BeginWorldLabels(nullptr,Box(0,0,1280,720));
   TCHud31::SubmitWorldInformation(nullptr,Box(900,45,45,30),[&](){Icons+=10;});
   TCHud31::FinishWorldLabels({});
   Check("badge returns immediately when the panel disappears",Icons==11);
  }
  int Immediate=0;
  TCHud31::SubmitWorldInformation(nullptr,FBox2D(FVector2D(0,0),FVector2D(10,10)),[&](){++Immediate;});
  Check("HUD-hidden and standalone painters remain immediate",Immediate==1);
 }
}
