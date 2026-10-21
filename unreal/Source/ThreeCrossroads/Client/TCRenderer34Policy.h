// Process-local renderer experiments. No settings or match state changes.
#pragma once
#include <cmath>
namespace TCRenderer34
{
    struct FSamples
    {
        int Count = 0;
        double Next = 0;
        bool Take(bool Enabled, double Seconds)
        {
            if (!Enabled || !std::isfinite(Seconds) || Seconds < 0 || Seconds < Next || Count >= 100) return false;
            ++Count;
            Next = Seconds + 3;
            return true;
        }
    };
    inline double ScreenPercent(bool FullResolution, double Normal) { return FullResolution ? 100.0 : Normal; }
    inline bool ReliefLow(bool World, bool Low, bool HighRelief) { return World || (Low && !HighRelief); }
}
