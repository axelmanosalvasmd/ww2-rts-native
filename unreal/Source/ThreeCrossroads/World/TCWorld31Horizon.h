// Presentation-only far rim. Recipient-known edge sampling and the exact-source apron core stay unchanged.
#pragma once
#include "World/TCApron.h"
#include <array>

namespace TCApronNative
{
	inline double Reach(const TCApronCore::FApron& A)
	{
		// Cover the widest map view plus edge panning and ultrawide frustum corners.
		return std::max(1250., 8.*std::hypot(A.Wm,A.Hm)+1000.);
	}
	inline std::array<float,3> Position(const TCApronCore::FApron& A, size_t V)
	{
		std::array<float,3> P={A.Position[V*3], A.Position[V*3+1], A.Position[V*3+2]};
		if (V % A.R == A.K-1)
		{
			const auto& Col=A.Cols[V/A.R];
			P[0]=(float)(Col.X+Col.Nx*Reach(A));
			P[2]=(float)(Col.Z+Col.Nz*Reach(A));
		}
		return P;
	}
}
