// aircraft.js fall motion, in source metres and radians. Presentation only, never game simulation.
#pragma once
#include <cmath>
namespace TCRender29Aircraft
{
 struct FFall
 {
  double X = 0, Y = 0, Z = 0, Heading = 0, Speed = 20, Vy = 0, Age = 0;
  double Roll = 0, RollRate = 2, Pitch = 0, Yaw = 1.3;
 };
 inline void Advance(FFall& F, double Dt)
 {
  F.Age += Dt; F.Vy -= 14 * Dt; F.Speed *= std::exp(-0.45 * Dt); F.Heading += F.Yaw * Dt;
  F.X += std::cos(F.Heading) * F.Speed * Dt; F.Z += std::sin(F.Heading) * F.Speed * Dt; F.Y += F.Vy * Dt;
  F.Roll += F.RollRate * Dt; F.RollRate += (F.RollRate < 0 ? -1 : 1) * 5 * Dt;
  F.Pitch += (-1.05 - F.Pitch) * (1 - std::exp(-2.4 * Dt));
 }
 inline bool Landed(const FFall& F, double Ground) { return F.Y <= Ground + 0.6 || F.Age > 3; }
}
