// Presentation emission and support-pass rules, in source metres and seconds.
#pragma once
#include <algorithm>
#include <cmath>
namespace TCFx31
{
inline double PassU(double Age, bool Bomber) { return (Age + (Bomber ? 1.0 : 0.0)) / 3.0; }
inline int CloudBurst(bool Low, double Radius, bool Fire) { return int(std::floor((Low ? 7 : 16) * std::pow(Radius / 7, 2) * (Fire ? .4 : 1) + .5)); }
inline double CloudRate(bool Low, double Radius) { return (Low ? 10 : 26) * std::pow(Radius / 7, 2) / 7.2; }
inline double DustChance(bool Low) { return Low ? .3 : .6; }
inline double WorkChance(bool Low) { return Low ? .2 : .45; }
inline double WreckFlames(bool Low) { return Low ? 3 : 6; }
inline double WreckSmoke(bool Low) { return Low ? 2.2 : 4.5; }
inline double WreckEmbers(bool Low) { return Low ? 0 : 3; }
inline double FireShare(bool Low, int Count) { return (Low ? .5 : 1) * std::min(1., 50. / Count); }
inline double DownAt(double Age, bool Bomber, double Random) { return std::max(Age + .05, 1.3 + Random * .15 - (Bomber ? 1 : 0)); }
struct FBomb { double Along = 0, Across = 0, Height = 0, VAlong = 0, VHeight = 0; };
// aircraft.js SUPPORT.bombing: six impacts 0.2s apart along 40m, falling for 0.45s.
inline bool BombAt(double Age, int Index, double ReleaseHeight, double Ground, FBomb& Out)
{
 const double Impact = Index * .2, Release = Impact - .45;
 if (Index <= 0 || Age < Release || Age >= Impact) return false;
 const double Tau = Age - Release, Start = (PassU(Release, true) - .5) * 140;
 const double End = (Index / 5. - .5) * 40, Gravity = 2 * (ReleaseHeight - Ground) / (.45 * .45);
 Out.Along = Start + (End - Start) * Tau / .45;
 Out.Height = ReleaseHeight - .5 * Gravity * Tau * Tau;
 Out.VAlong = (End - Start) / .45; Out.VHeight = -Gravity * Tau;
 return true;
}
}
