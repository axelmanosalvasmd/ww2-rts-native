// Where this client's seat token comes from. The token is the seat's key: whoever sends it in hello takes the seat
// (reference server.js matches it before anything else, even before a spectate request). So it never goes on the
// command line (the engine logs the command line, and ps shows it to every local user) and is never logged.
// Sources, first match wins:
//   spectating            a fresh random token for this process only, never stored: a watcher holds no seat, and a
//                         stored seat token would take that seat instead of watching
//   -TCTokenFile=<path>   an absolute path; read, or created with a fresh token when missing or empty
//   TC_TOKEN              environment variable holding the token itself
//   default               Saved/TCTokens/<room>[.<seat>]-<hash of server, room and seat>.token
// One identity per server, room and seat label (-TCSeat, like the browser's #room&seat=), so rooms and servers never
// share a token. On Unix, files are owner-only (0600, default directory 0700) and held under an exclusive lock while
// the store lives: a second client on the same identity is refused instead of silently taking the first one's seat.
#pragma once

#include "CoreMinimal.h"

struct FTCTokenRequest
{
	FString Server;      // the ws:// URL, as given
	FString Room;        // already validated (FTCConnection::ValidRoom)
	FString Seat;        // optional label, 1 to 32 of A-Z a-z 0-9 _ -
	FString TokenFile;   // optional absolute path
	bool bSpectate = false;
};

class FTCTokenStore
{
public:
	FTCTokenStore() = default;
	~FTCTokenStore();
	FTCTokenStore(const FTCTokenStore&) = delete;
	FTCTokenStore& operator=(const FTCTokenStore&) = delete;

	// Fills OutToken, or returns false with a reason in OutError (neither ever contains a token).
	bool Resolve(const FTCTokenRequest& Request, FString& OutToken, FString& OutError);
	// Where the token came from, for the log: a path, the variable name or "one-off". Never the token.
	const FString& Source() const { return SourceText; }

	static bool ValidToken(const FString& Token);  // 16 to 40 of A-Z a-z 0-9 _ - (the server keeps 40)
	static bool ValidSeat(const FString& Seat);
	static FString NewToken();                     // 40 hex digits from the platform's random source; empty on failure
	static FString DefaultPath(const FString& Server, const FString& Room, const FString& Seat);
	static const TCHAR* EnvVar() { return TEXT("TC_TOKEN"); }

private:
	bool FromFile(const FString& Path, bool bExplicit, FString& OutToken, FString& OutError);
	void Release();

	int32 LockFd = -1;   // Unix: the open, locked token file
	FString SourceText;
};
