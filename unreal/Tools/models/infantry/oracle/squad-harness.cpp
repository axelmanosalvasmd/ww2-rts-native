// The C++ port of moveSquad (TCInfantryRig.cpp TCInfantryRigCore::MoveSquad, compiled without the engine) replaying
// squad-reference.mjs's scenario.txt from stdin, printing the same per-step rows.
#define TC_INFANTRY_RIG_STANDALONE 1
#include "../../../../Source/ThreeCrossroads/Client/TCInfantryRig.cpp"
#include <cstdio>
#include <iostream>
#include <vector>

using namespace TCInfantryRigCore;

int main()
{
	int Id = 0, Count = 0;
	std::cin >> Id >> Count;
	std::vector<FManIn> Men(Count);
	for (FManIn& M : Men) std::cin >> M.SlotX >> M.SlotZ >> M.Index >> M.Scale;
	FSquadState Sq;
	double Dt;
	while (std::cin >> Dt)
	{
		FSquadIn V;
		int Visible, Garr, Trench, MoveFire, Target, HasAim;
		std::cin >> V.X >> V.Z >> V.Yaw >> Visible >> Garr >> Trench >> V.Flags >> V.Supp >> V.Cover >> MoveFire >> Target >> HasAim >> V.Aim;
		V.Id = Id; V.bVisible = Visible; V.bGarrisoned = Garr; V.bTrench = Trench; V.bMoveFire = MoveFire; V.bTarget = Target; V.bAim = HasAim;
		for (FManIn& M : Men) { int Vis, Refill; std::cin >> Vis >> Refill; M.bVisible = Vis; if (Refill) M.bRefill = true; }
		MoveSquad(Sq, V, Men.data(), Count, Dt);
		std::printf("[");
		for (int I = 0; I < Count; I++)
		{
			const FManIn& M = Men[I];
			std::printf("%s[%.17g,%.17g,%.17g,%.17g,%.17g,%.17g,%.17g,%.17g,%.17g]", I ? "," : "", M.X, M.Z, M.MYaw, M.Phase, M.Blend, M.Speed, M.LocalX, M.LocalZ, M.LocalYaw);
		}
		std::printf("]\n");
	}
	return 0;
}
