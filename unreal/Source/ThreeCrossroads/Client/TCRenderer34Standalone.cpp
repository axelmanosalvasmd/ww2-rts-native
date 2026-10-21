// Standalone regression for the opt-in sampler's duration and default-preserving controls.
#if defined(TC_RENDERER34_STANDALONE)
#include "TCRenderer34Policy.h"
#include <cassert>
#include <limits>
int main()
{
    TCRenderer34::FSamples S;
    assert(!S.Take(false, 0));
    assert(!S.Take(true, std::numeric_limits<double>::quiet_NaN()));
    assert(S.Take(true, 0));
    assert(!S.Take(true, 2.99));
    assert(S.Take(true, 3));
    assert(S.Take(true, 120));
    assert(!S.Take(true, 120.01));
    TCRenderer34::FSamples Capped;
    for (int I = 0; I < 100; ++I) assert(Capped.Take(true, I * 3));
    assert(!Capped.Take(true, 300));
    assert(TCRenderer34::ScreenPercent(false, 75) == 75);
    assert(TCRenderer34::ScreenPercent(true, 75) == 100);
    assert(TCRenderer34::ReliefLow(false, true, false));
    assert(!TCRenderer34::ReliefLow(false, true, true));
    assert(TCRenderer34::ReliefLow(true, true, true));
}
#endif
