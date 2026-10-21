#pragma once
#include "CoreMinimal.h"
class ATCPlayerController;
class FJsonObject;
namespace TCSession33Graphics
{
	// Observational only. Called by the existing explicit 'consumers' diagnostic action.
	void Append(const ATCPlayerController& Controller, FJsonObject& Out);
}
