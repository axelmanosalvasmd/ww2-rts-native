// Production-kernel checks shared by the native fixture and the engine-free executable.
#pragma once
#include "World/TCApron.h"
#include "World/TCWorld31Horizon.h"
#include "World/TCRailways.h"

namespace TCWorld31Checks
{
	inline int Apron()
	{
		TCApronCore::FApron A; A.Build(80, 80);
		auto Height=[](double X,double Z){return .015*X+.008*Z;};
		A.Shape(Height,[](int){return false;});
		for (uint32_t I : A.Index) if (I >= A.Position.size()/3) return 1;
		for (float V : A.Position) if (!std::isfinite(V)) return 2;
		for (int C=0; C<A.N; C++)
		{
			const size_t I=(size_t)C*A.R*3;
			if (std::abs(A.Position[I+1]-Height(A.Cols[C].X,A.Cols[C].Z))>1e-5) return 3;
			const size_t Outer=I+(A.K-1)*3;
			if (std::max(std::abs(A.Position[Outer]-A.Cols[C].X),std::abs(A.Position[Outer+2]-A.Cols[C].Z))<1249.9) return 4;
		}
		if (A.Index.size() != (size_t)A.N*(A.K+1)*6) return 5;
		return 0;
	}
	inline int Horizon()
	{
		TCApronCore::FApron A; A.Build(256,256);
		A.Shape([](double,double){return 0.;},[](int){return false;});
		for (int C=0; C<A.N; C++)
		{
			const size_t V=(size_t)C*A.R+A.K-1;
			const auto P=TCApronNative::Position(A,V);
			if (std::max(std::abs(P[0]-A.Cols[C].X),std::abs(P[2]-A.Cols[C].Z)) < TCApronNative::Reach(A)-.1) return 1;
			const auto Edge=TCApronNative::Position(A,(size_t)C*A.R);
			if (Edge[0]!=A.Position[(size_t)C*A.R*3] || Edge[1]!=A.Position[(size_t)C*A.R*3+1] || Edge[2]!=A.Position[(size_t)C*A.R*3+2]) return 2;
		}
		return 0;
	}
	inline int Rail()
	{
		TCRailCore::FModel M; M.Reset(24);
		M.Cell_(299,1,false,0); M.Cell_(300,21,true,0); M.Cell_(301,16,false,0); M.Cell_(324,64,false,0);
		M.Frame(1,[](double,double){return 0.;},[](int Id,double& X,double& Z){X=24; Z=8; return Id==0;});
		if (M.Halves().size()!=6 || M.BuiltStations.size()!=1 || M.Track.count(302)) return 1;
		bool Platform=false,House=false,Roof=false,Tower=false;
		for (const auto& P : M.BuiltStations[0].Parts)
		{
			Platform |= P.Mat==TCRailCore::Platform; House |= P.Mat==TCRailCore::Brick;
			Roof |= P.Mat==TCRailCore::Roof; Tower |= P.Mat==TCRailCore::Tank && P.bCylinder;
		}
		return Platform && House && Roof && Tower ? 0 : 2;
	}
}
