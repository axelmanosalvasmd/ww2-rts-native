// Presentation arithmetic shared by the unit view and standalone regression.
#pragma once
#include <algorithm>
#include <cmath>

namespace TCUnits31
{
 struct FSnapshotEase
 {
  double GapMs = 100, Previous = 0;
  bool HasPrevious = false;
  void Arrived(double Seconds)
  {
   if (HasPrevious) GapMs += (std::clamp((Seconds - Previous) * 1000., 60., 400.) - GapMs) * .2;
   Previous = Seconds;
   HasPrevious = true;
  }
  double Factor(double Dt) const { return 1 - std::exp(-Dt * 1000 / GapMs); }
 };
 inline double Waterline(double Ground, double Water)
 {
  return std::max(Ground, std::isfinite(Water) && Water != 0 ? Water : Ground);
 }
 // Keep aircraft identifiable above a busy battlefield without masking ground controls.
 constexpr double AirframeScale = .65;
 struct FGroundPose
 {
  double Pitch = 0, Roll = 0, Lift = 0, X = 0, Z = 0;
  bool Suspended = true;
  template<class FHeight> bool Update(double Px, double Pz, double Heading, double Radius, bool Tracked, double Dt, FHeight Height)
  {
   const double Long=std::clamp(Radius*.8,.5,8.), Wide=std::clamp(Radius*.48,.3,5.);
   const double C=std::cos(Heading), S=std::sin(Heading), Ground=Height(Px,Pz);
   auto At=[&](double A,double B){return Height(Px+C*A-S*B,Pz+S*A+C*B);};
   const double Front=At(Long,0), Rear=At(-Long,0), Right=At(0,Wide), Left=At(0,-Wide);
   if (!std::isfinite(Ground+Front+Rear+Right+Left)) { Suspended=true; return false; }
   constexpr double Limit=3.14159265358979323846/3;
   const double GoalPitch=std::clamp(std::atan2(Front-Rear,2*Long),-Limit,Limit);
   const double GoalRoll=std::clamp(-std::atan((Right-Left)/(2*Wide)*std::cos(GoalPitch)),-Limit,Limit);
   const bool Returned=Suspended || std::hypot(Px-X,Pz-Z)>8;
   const double Blend=Returned?1:1-std::exp(-std::clamp(std::isfinite(Dt)?Dt:0.,0.,.1)*(Tracked?12:16));
   const double P=Pitch+(GoalPitch-Pitch)*Blend, R=Roll+(GoalRoll-Roll)*Blend;
   // Sample the tilted contact plane at half-metre intervals. This bridges small dips and keeps tracks above crests.
   const int NX=static_cast<int>(std::ceil(2*Long/.5)), NZ=static_cast<int>(std::ceil(2*Wide/.5));
   double Support=0;
   for (int I=0;I<=NX;I++) for (int J=0;J<=NZ;J++)
   {
    const double A=-Long+2*Long*I/NX, B=-Wide+2*Wide*J/NZ;
    const double Hx=A*std::cos(P)+B*std::sin(P)*std::sin(R), Hz=B*std::cos(R);
    const double Hy=A*std::sin(P)-B*std::cos(P)*std::sin(R), H=At(Hx,Hz);
    if (!std::isfinite(H)) { Suspended=true; return false; }
    Support=std::max(Support,H-Ground-Hy);
   }
   Pitch=P; Roll=R; Lift=Returned || Support>Lift?Support:Lift+(Support-Lift)*Blend;
   X=Px; Z=Pz; Suspended=false;
   return true;
  }
 };
}
