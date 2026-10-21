// Continuous source Volume input, in percentage points. The caller persists each changed value.
#pragma once
#include <algorithm>
#include <cmath>
namespace TCLook30
{
 struct FVolumeDrag
 {
  bool Active = false;
  int Percent = 80;
  double Left = 0, Width = 0;
  bool Begin(double X, double InLeft, double InRight)
  {
   Cancel();
   if (!std::isfinite(X) || !std::isfinite(InLeft) || !std::isfinite(InRight) || InRight <= InLeft) return false;
   Left = InLeft; Width = InRight - InLeft; Active = true;
   Sample(X); return true;
  }
  bool Move(double X, bool Held)
  {
   if (!Held) Cancel();
   if (!Active || !std::isfinite(X)) return false;
   const int Before = Percent; Sample(X); return Before != Percent;
  }
  void Cancel() { Active = false; }
 private:
  void Sample(double X) { Percent = int(std::floor(std::clamp((X - Left) / Width, 0.0, 1.0) * 100 + .5)); }
 };
}
