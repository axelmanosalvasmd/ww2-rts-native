// One bounded, local transition-log request. No rendering or engine dependency.
#pragma once

namespace TCRender28
{
	template <typename Char> constexpr bool LiteralFilter(const Char* Value)
	{
		if (!Value || !Value[0] || Value[0] == '!' || Value[0] == ' ') return false;
		int Length = 0;
		for (; Value[Length]; ++Length)
		{
			const Char C = Value[Length];
			if (Length >= 128 || C < ' ' || C == 127 || C == '*' || C == '?' || C == '"') return false;
		}
		if (Value[Length - 1] == ' ') return false;
		// RenderGraphPrivate.cpp treats None as an empty filter and ! as inversion.
		if (Length == 4 && (Value[0] == 'n' || Value[0] == 'N') && (Value[1] == 'o' || Value[1] == 'O')
			&& (Value[2] == 'n' || Value[2] == 'N') && (Value[3] == 'e' || Value[3] == 'E')) return false;
		return true;
	}

	struct FTransitionBudget
	{
		bool Attempted = false;
		bool Take(bool Requested, bool TargetMatches)
		{
			if (!Requested || !TargetMatches || Attempted) return false;
			Attempted = true;
			return true;
		}
	};
}
