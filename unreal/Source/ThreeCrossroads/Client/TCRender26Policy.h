// Bounded, opt-in renderer receipts. This policy has no engine or rendering dependency.
#pragma once
#include <cmath>

namespace TCRender26
{
	struct FReceiptBudget
	{
		static constexpr int Limit = 64;
		int Count = 0;
		double Next = 0;

		bool Take(bool Enabled, double Seconds)
		{
			if (!Enabled || !std::isfinite(Seconds) || Seconds < 0 || Count >= Limit || Seconds < Next) return false;
			++Count;
			Next = Seconds + 1.0; // no catch-up burst after a blocked frame
			return true;
		}
		static bool Inventory(int Sample) { return Sample == 2 || Sample == 10; }
	};
}
