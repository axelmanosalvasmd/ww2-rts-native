#pragma once

// Qualification is opt-in at every boundary and never permits loading a save.
namespace TCSession30
{
	// Match the authority exactly. In particular, never accept userinfo followed by a remote host.
	constexpr bool Loopback(const char* Url)
	{
		const char* Prefix = "ws://127.0.0.1:";
		while (*Prefix) if (*Url++ != *Prefix++) return false;
		int Port = 0, Digits = 0;
		while (*Url >= '0' && *Url <= '9')
		{
			Port = Port * 10 + (*Url++ - '0');
			if (++Digits > 5 || Port > 65535) return false;
		}
		if (!Digits || !Port) return false;
		if (!*Url) return true;
		const char* Path = "/ws";
		while (*Path) if (*Url++ != *Path++) return false;
		return !*Url;
	}
	constexpr bool Enabled(bool Development, bool Switch, bool Environment, bool Loopback)
	{
		return Development && Switch && Environment && Loopback;
	}
	constexpr bool Blocked(bool Rust, bool Qualification, bool Start, bool Restart, bool Load)
	{
		return Rust && (Load || (!Qualification && (Start || Restart)));
	}
}
