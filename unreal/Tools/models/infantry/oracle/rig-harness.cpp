// Oracle side B: the C++ port (unreal/Source/ThreeCrossroads/Client/TCInfantryRig.cpp, compiled without the engine)
// solving the same cases as dump-reference.mjs and printing the same JSON fields. Reads cases.txt on stdin.
#define TC_INFANTRY_RIG_STANDALONE 1
#include "../../../../Source/ThreeCrossroads/Client/TCInfantryRig.cpp"
#include <cstdio>
#include <string>
#include <iostream>
#include <sstream>

using namespace TCInfantryRigCore;

static void PV(const FV& P) { std::printf("[%.17g,%.17g,%.17g]", P.X, P.Y, P.Z); }
static void PM(const FM& M) { std::printf("["); for (int I = 0; I < 16; I++) std::printf(I ? ",%.17g" : "%.17g", M.E[I]); std::printf("]"); }
static const char* ThingName(EThing T)
{
	switch (T) { case EThing::Can: return "can"; case EThing::Round: return "round"; case EThing::Clip: return "clip"; case EThing::Bomb: return "bomb";
	case EThing::Binos: return "binos"; case EThing::Belt: return "belt"; case EThing::Shovel: return "shovel"; default: return "none"; }
}

int main()
{
	std::string Line;
	while (std::getline(std::cin, Line))
	{
		if (Line.empty()) continue;
		std::istringstream In(Line);
		std::string Type, KStr, PhaseStr;
		int Fac = 0, I = 0;
		In >> Type >> Fac >> I >> KStr >> PhaseStr;
		FCtx Ctx;
		Ctx.Type = ParseType(Type.c_str());
		Ctx.Fac = Fac;
		Ctx.Job = SoldierKit(Ctx.Type, I, Fac);
		EPose Pose = EPose::Stand;
		int DigIndex = 0;
		bool bPhase = PhaseStr != "none";
		double Phase = bPhase ? std::stod(PhaseStr) : 0;
		if (KStr == "fallen") Pose = EPose::Fallen;
		else if (KStr == "dig") { Pose = EPose::Dig; DigIndex = static_cast<int>(Phase); }
		else Pose = static_cast<EPose>(std::stoi(KStr));
		const FRig R = Solve(Ctx, Pose, DigIndex, bPhase, Phase);
		FM Skin[NumDeform];
		SkinMatrices(R, Skin);
		FM Bones[NumBones];
		bool bValid[NumBones];
		BoneTransforms(R, Bones, bValid);
		std::printf("{\"job\":\"%s%s\",\"legs\":[", BaseName(Ctx.Job.Base), Ctx.Job.Variant ? std::string(1, Ctx.Job.Variant).c_str() : "");
		for (int S = 0; S < 2; S++)
		{
			const FLeg& L = R.Legs[S];
			std::printf(S ? ",[" : "["); PV(L.Hip); std::printf(","); PV(L.Knee); std::printf(","); PV(L.Ankle); std::printf(","); PV(L.Toe); std::printf(","); PV(L.Pole); std::printf("]");
		}
		std::printf("],\"arms\":[");
		for (int S = 0; S < 2; S++)
		{
			const FArm& A = R.Arms[S];
			std::printf(S ? ",[" : "["); PV(A.Sho); std::printf(","); PV(A.Elbow); std::printf(","); PV(A.Hand); std::printf(",[%.17g,0,0]]", A.Err);
		}
		std::printf("],\"B\":"); PM(R.Body.B); std::printf(",\"U\":"); PM(R.Body.U); std::printf(",\"H\":"); PM(R.H); std::printf(",\"M0\":"); PM(R.M0);
		std::printf(",\"W\":"); if (R.bW) PM(R.W); else std::printf("null");
		std::printf(",\"sling\":"); if (R.bSling) PM(R.Sling); else std::printf("null");
		std::printf(",\"things\":[");
		for (int T = 0; T < R.NumThings; T++) { std::printf(T ? ",{\"kind\":\"%s\",\"m\":" : "{\"kind\":\"%s\",\"m\":", ThingName(R.Things[T].Kind)); PM(R.Things[T].M); std::printf("}"); }
		std::printf("],\"skin\":[");
		for (int B = 0; B < NumDeform; B++) { if (B) std::printf(","); PM(Skin[B]); }
		std::printf("],\"grips\":["); PM(Bones[BGripL]); std::printf(","); PM(Bones[BGripR]);
		std::printf("],\"muzzle\":"); if (R.bW) PV(Muzzle(R)); else std::printf("null");
		std::printf(",\"bones\":[");
		for (int B = 0; B < NumBones; B++) { if (B) std::printf(","); if (bValid[B]) PM(Bones[B]); else std::printf("null"); }
		std::printf("]}\n");
	}
	return 0;
}
