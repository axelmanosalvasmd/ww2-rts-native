// UI-only timing, with an injected monotonic millisecond clock. No simulation state or rules.
#pragma once
#include <algorithm>
#include <vector>

namespace TCHud26
{
 // hud.js:475-481 adds .hit and schedules a separate remove callback for every card letter.
 // A repeated press does not replace an earlier callback with a later deadline.
 class FLetterHit
 {
  std::vector<double> Removals;
  bool bHit=false;
 public:
  bool Active(double NowMs)
  {
   const auto End=std::remove_if(Removals.begin(),Removals.end(),[&](double At)
   { if (At>NowMs) return false; bHit=false; return true; });
   Removals.erase(End,Removals.end());
   return bHit;
  }
  void Press(double NowMs) { Active(NowMs); bHit=true; Removals.push_back(NowMs+140.0); }
 };

 // DOM elementFromPoint uses the next element at a shared right/bottom edge.
 inline bool Contains(double Left,double Top,double Right,double Bottom,double X,double Y)
 { return X>=Left && Y>=Top && X<Right && Y<Bottom; }
}
