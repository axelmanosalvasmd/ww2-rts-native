// Opt-in local observations at real view lifecycle boundaries. Never writes, restores, sends or changes input state.
#pragma once
#include "CoreMinimal.h"
class ATCGameMode;
class ATCPlayerController;
class FJsonObject;

namespace TCSession28
{
	// Camera yaw is the native controller's degrees, as in ConsumerState. pendingView uses storage radians.
	THREECROSSROADS_API TSharedRef<FJsonObject> ViewSeamState(const ATCGameMode& Match, const ATCPlayerController& Controller,
		const FString& Phase, int32 IncomingMatchId, TOptional<bool> WriteSucceeded = {});
	// The lead hooks call this before Start, after capture, after the first recipient-ready restore and after writes.
	// Without -TCViewSeam28 it returns before building a snapshot. No address, token, nickname or filename is logged.
	THREECROSSROADS_API void LogViewSeam(const ATCGameMode& Match, const ATCPlayerController& Controller,
		const FString& Phase, int32 IncomingMatchId, TOptional<bool> WriteSucceeded = {});
}
