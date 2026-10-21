// Compile with -DTC_WORLD31_STANDALONE for source checks. Unreal builds the native acceptance fixtures instead.
#ifdef TC_WORLD31_STANDALONE
#define TC_APRON_CORE_ONLY
#define TC_RAIL_CORE_ONLY
#ifndef TC_GROUND_PAINT_CORE_ONLY
#define TC_GROUND_PAINT_CORE_ONLY
#endif
#include "World/TCWorld31Checks.inl"
#include "World/TCWorld30Memory.inl"
#include <cstdio>
int main()
{
	const int Horizon=TCWorld31Checks::Horizon();
	std::printf("native horizon=%d\n",Horizon);
	const int Apron=TCWorld31Checks::Apron(), Rail=TCWorld31Checks::Rail(), Memory=TCWorld30Memory::Check();
	std::printf("apron geometry/seams=%d, complete junction/station=%d, decal discovery/memory/privacy=%d\n",Apron,Rail,Memory);
	return Horizon || Apron || Rail || Memory;
}
#endif
