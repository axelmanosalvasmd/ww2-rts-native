#pragma once
#include <cstdint>
#include <string_view>

// lobby.rules is advisory. No hash outcome changes admission or authoritative rules.
namespace TCShared31
{
	enum class EHashCheck { Absent, Match, Mismatch, Malformed, Unavailable };
	inline EHashCheck CompareRulesHash(bool Ready, std::uint64_t Local, bool Present, std::string_view Remote)
	{
		if (!Present) return EHashCheck::Absent;
		if (!Ready) return EHashCheck::Unavailable;
		if (Remote.size() != 16) return EHashCheck::Malformed;
		std::uint64_t Value = 0;
		for (char C : Remote)
		{
			unsigned Digit;
			if (C >= '0' && C <= '9') Digit = C - '0';
			else if (C >= 'a' && C <= 'f') Digit = C - 'a' + 10;
			else if (C >= 'A' && C <= 'F') Digit = C - 'A' + 10;
			else return EHashCheck::Malformed;
			Value = (Value << 4) | Digit;
		}
		return Value == Local ? EHashCheck::Match : EHashCheck::Mismatch;
	}
}
