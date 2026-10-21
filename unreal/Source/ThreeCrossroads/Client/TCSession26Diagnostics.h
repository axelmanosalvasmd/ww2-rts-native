// Opt-in integration-check snapshot of consumed state. Never sends, saves, restores or loads a preference.
#pragma once
#include "CoreMinimal.h"
class ATCGameMode;
class ATCPlayerController;
class FJsonObject;

namespace TCSession26
{
	THREECROSSROADS_API TSharedRef<FJsonObject> ConsumerState(const ATCGameMode& Match, const ATCPlayerController& Controller);
}
