#include "Net/TCToken.h"
#include "ThreeCrossroads.h"
#include "HAL/FileManager.h"
#include "HAL/PlatformMisc.h"
#include "Misc/FileHelper.h"
#include "Misc/Guid.h"
#include "Misc/Paths.h"
#include "Misc/SecureHash.h"

#if PLATFORM_UNIX
#include "Unix/UnixPlatformGuid.h"
#include <errno.h>
#include <fcntl.h>
#include <string.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>
#endif

namespace
{
	bool TokenChar(TCHAR C)
	{
		return (C >= 'A' && C <= 'Z') || (C >= 'a' && C <= 'z') || (C >= '0' && C <= '9') || C == '_' || C == '-';
	}

#if PLATFORM_UNIX
	FString Errno(int32 Code) { return FString::Printf(TEXT("%s (errno %d)"), UTF8_TO_TCHAR(strerror(Code)), Code); }
#endif
}

FTCTokenStore::~FTCTokenStore() { Release(); }

void FTCTokenStore::Release()
{
#if PLATFORM_UNIX
	if (LockFd >= 0) { ::close(LockFd); LockFd = -1; } // closing drops the flock
#endif
}

bool FTCTokenStore::ValidToken(const FString& Token)
{
	if (Token.Len() < 16 || Token.Len() > 40) return false;
	for (TCHAR C : Token) if (!TokenChar(C)) return false;
	return true;
}

bool FTCTokenStore::ValidSeat(const FString& Seat)
{
	if (Seat.Len() < 1 || Seat.Len() > 32) return false; // client/room-session.js roomAddress
	for (TCHAR C : Seat) if (!TokenChar(C)) return false;
	return true;
}

FString FTCTokenStore::NewToken()
{
	// Two random-only UUIDs: the first gives 32 hex digits (122 random bits), the second's first field 8 more (32 bits).
	// FGuid::NewGuid is not used on Unix: since 5.8 it makes a UUID v7, whose first 48 bits are the clock.
	FString Out;
	for (int32 I = 0; I < 2; I++)
	{
#if PLATFORM_UNIX
		const FGuid G = FUnixPlatformGuid::CreateGuidFromRandomOnly(); // getrandom()
#else
		FGuid G;
		FPlatformMisc::CreateGuid(G);
#endif
		if (!G.IsValid()) return FString();
		Out += G.ToString(EGuidFormats::Digits).ToLower();
	}
	return Out.Left(40);
}

FString FTCTokenStore::DefaultPath(const FString& Server, const FString& Room, const FString& Seat)
{
	FString Url = Server.TrimStartAndEnd().ToLower();
	while (Url.EndsWith(TEXT("/"))) Url.LeftChopInline(1);
	const FString Identity = Url + TEXT("\n") + Room + TEXT("\n") + Seat;
	FTCHARToUTF8 Utf8(*Identity);
	FMD5 Md5;
	Md5.Update(reinterpret_cast<const uint8*>(Utf8.Get()), Utf8.Length());
	uint8 Digest[16];
	Md5.Final(Digest);
	FString Hex;
	for (int32 I = 0; I < 6; I++) Hex += FString::Printf(TEXT("%02x"), Digest[I]);
	const FString Name = FString::Printf(TEXT("%s%s%s-%s.token"), *Room, Seat.IsEmpty() ? TEXT("") : TEXT("."), *Seat, *Hex);
	return FPaths::ConvertRelativePathToFull(FPaths::Combine(FPaths::ProjectSavedDir(), TEXT("TCTokens"), Name));
}

bool FTCTokenStore::Resolve(const FTCTokenRequest& Request, FString& OutToken, FString& OutError)
{
	Release();
	OutToken.Reset(); SourceText.Reset();
	if (!Request.Seat.IsEmpty() && !ValidSeat(Request.Seat)) { OutError = TEXT("-TCSeat must be 1 to 32 letters, digits, '_' or '-'"); return false; }

	if (Request.bSpectate)
	{
		OutToken = NewToken();
		if (OutToken.IsEmpty()) { OutError = TEXT("could not draw a random token"); return false; }
		SourceText = TEXT("one-off spectator token (not stored)");
		if (!Request.TokenFile.IsEmpty() || !FPlatformMisc::GetEnvironmentVariable(EnvVar()).IsEmpty())
			UE_LOG(LogTC, Log, TEXT("spectating: the seat token from -TCTokenFile / %s is not presented"), EnvVar());
		return true;
	}
	if (!Request.TokenFile.IsEmpty())
	{
		if (FPaths::IsRelative(Request.TokenFile)) { OutError = TEXT("-TCTokenFile must be an absolute path"); return false; }
		return FromFile(Request.TokenFile, true, OutToken, OutError);
	}
	FString Env = FPlatformMisc::GetEnvironmentVariable(EnvVar());
	Env.TrimStartAndEndInline();
	if (!Env.IsEmpty())
	{
		if (!ValidToken(Env)) { OutError = FString::Printf(TEXT("%s must be 16 to 40 letters, digits, '_' or '-'"), EnvVar()); return false; }
		OutToken = MoveTemp(Env);
		SourceText = FString::Printf(TEXT("environment variable %s"), EnvVar());
		return true;
	}
	return FromFile(DefaultPath(Request.Server, Request.Room, Request.Seat), false, OutToken, OutError);
}

bool FTCTokenStore::FromFile(const FString& Path, bool bExplicit, FString& OutToken, FString& OutError)
{
#if PLATFORM_UNIX
	if (!bExplicit)
	{
		// Saved/ may be shared or loosely permitted: the token directory itself is owner-only and never a symlink.
		const FString Dir = FPaths::GetPath(Path);
		IFileManager::Get().MakeDirectory(*FPaths::GetPath(Dir), true);
		if (::mkdir(TCHAR_TO_UTF8(*Dir), 0700) != 0 && errno != EEXIST) { OutError = FString::Printf(TEXT("cannot create %s: %s"), *Dir, *Errno(errno)); return false; }
		struct stat DirStat;
		if (::lstat(TCHAR_TO_UTF8(*Dir), &DirStat) != 0 || !S_ISDIR(DirStat.st_mode) || DirStat.st_uid != ::geteuid())
		{
			OutError = FString::Printf(TEXT("%s is not a directory owned by this user"), *Dir);
			return false;
		}
		if ((DirStat.st_mode & 077) && ::chmod(TCHAR_TO_UTF8(*Dir), 0700) != 0) UE_LOG(LogTC, Warning, TEXT("could not make %s owner-only"), *Dir);
	}
	// The default file is never followed through a symlink; an explicit one may be (a secret store, say), but must
	// still end at a regular file this user owns.
	const int32 Fd = ::open(TCHAR_TO_UTF8(*Path), O_RDWR | O_CREAT | O_CLOEXEC | (bExplicit ? 0 : O_NOFOLLOW), 0600);
	if (Fd < 0) { OutError = FString::Printf(TEXT("cannot open token file %s: %s"), *Path, *Errno(errno)); return false; }
	auto Fail = [&](const FString& Why) { ::close(Fd); OutError = Why; return false; };
	struct stat St;
	if (::fstat(Fd, &St) != 0 || !S_ISREG(St.st_mode) || St.st_uid != ::geteuid()) return Fail(FString::Printf(TEXT("token file %s is not a regular file owned by this user"), *Path));
	if (::flock(Fd, LOCK_EX | LOCK_NB) != 0)
	{
		const int32 Code = errno;
		if (Code == EWOULDBLOCK) return Fail(FString::Printf(TEXT("token file %s is in use by another client: give each client its own -TCSeat=<label> (or -TCTokenFile)"), *Path));
		return Fail(FString::Printf(TEXT("cannot lock token file %s: %s"), *Path, *Errno(Code)));
	}
	if ((St.st_mode & 077) != 0)
	{
		if (::fchmod(Fd, 0600) != 0) return Fail(FString::Printf(TEXT("token file %s is readable by others and could not be made owner-only"), *Path));
		UE_LOG(LogTC, Warning, TEXT("token file %s was readable by others: made owner-only. Treat its token as exposed."), *Path);
	}

	constexpr int32 MaxRead = 128;
	ANSICHAR Buf[MaxRead];
	const ssize_t Got = ::pread(Fd, Buf, MaxRead, 0);
	if (Got < 0) return Fail(FString::Printf(TEXT("cannot read token file %s: %s"), *Path, *Errno(errno)));
	FString Text;
	bool bReadable = Got < MaxRead;
	for (ssize_t I = 0; I < Got; I++) { if ((uint8)Buf[I] >= 128) bReadable = false; else Text.AppendChar((TCHAR)Buf[I]); }
	FMemory::Memzero(Buf, sizeof(Buf));
	Text.TrimStartAndEndInline();
	if (bReadable && ValidToken(Text))
	{
		OutToken = MoveTemp(Text);
	}
	else
	{
		if (!Text.IsEmpty() || !bReadable)
		{
			if (bExplicit) return Fail(FString::Printf(TEXT("token file %s does not hold a token (16 to 40 letters, digits, '_' or '-'); left unchanged"), *Path));
			UE_LOG(LogTC, Warning, TEXT("token file %s was unreadable: a new token replaces it (this identity's old seat is lost)"), *Path);
		}
		OutToken = NewToken();
		if (OutToken.IsEmpty()) return Fail(TEXT("could not draw a random token"));
		const FTCHARToUTF8 Out(*(OutToken + TEXT("\n")));
		if (::ftruncate(Fd, 0) != 0 || ::pwrite(Fd, Out.Get(), Out.Length(), 0) != (ssize_t)Out.Length() || ::fsync(Fd) != 0)
		{
			OutToken.Reset();
			return Fail(FString::Printf(TEXT("cannot write token file %s: %s"), *Path, *Errno(errno)));
		}
	}
	LockFd = Fd;
	SourceText = Path;
	return true;
#else
	// No owner-only permissions or lock here: the file lives in this user's project Saved folder.
	FString Text;
	if (FPaths::FileExists(Path) && !FFileHelper::LoadFileToString(Text, *Path)) { OutError = FString::Printf(TEXT("cannot read token file %s"), *Path); return false; }
	Text.TrimStartAndEndInline();
	if (ValidToken(Text)) { OutToken = MoveTemp(Text); SourceText = Path + TEXT(" (not locked on this platform)"); return true; }
	if (!Text.IsEmpty() && bExplicit) { OutError = FString::Printf(TEXT("token file %s does not hold a token; left unchanged"), *Path); return false; }
	OutToken = NewToken();
	if (OutToken.IsEmpty()) { OutError = TEXT("could not draw a random token"); return false; }
	if (!FFileHelper::SaveStringToFile(OutToken + TEXT("\n"), *Path)) { OutToken.Reset(); OutError = FString::Printf(TEXT("cannot write token file %s"), *Path); return false; }
	SourceText = Path + TEXT(" (not locked on this platform)");
	return true;
#endif
}
