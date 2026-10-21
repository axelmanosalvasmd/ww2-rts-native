// UI layout and feedback clocks only. Shared with the standalone regression.
#pragma once
#include <algorithm>
namespace TCHud30
{
 struct FScoreSizing { double Width, Inner, Team; bool SeparateLead; };
 inline FScoreSizing ScoreSizing(double ViewW,double Lead,int Teams,double Border)
 {
  const double Width=std::max(8+2*Border,ViewW-660), Inner=Width-8-2*Border;
  const double Team=std::min(Inner,std::clamp((Inner-Lead)/std::max(1,Teams),174.,240.));
  return {std::min(Width,Lead+Teams*Team+8+2*Border),Inner,Team,Lead>0 && Lead+Teams*Team>Inner+.01};
 }
 enum class EHint { Empty, Armed, Refusal };
 inline EHint HintLayer(bool Armed,bool Sent,double Age)
 { return !Sent && Age>=0 && Age<2 ? EHint::Refusal : Armed ? EHint::Armed : EHint::Empty; }
}
