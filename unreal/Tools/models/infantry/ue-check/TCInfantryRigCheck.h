// Verification only: compiled into a scratch copy of the module by run-ue-check.sh, never into the game.
#pragma once

#include "CoreMinimal.h"
#include "Commandlets/Commandlet.h"
#include "TCInfantryRigCheck.generated.h"

UCLASS()
class UTCInfantryRigCheckCommandlet : public UCommandlet
{
	GENERATED_BODY()

public:
	virtual int32 Main(const FString& Params) override;
};

// Verification only: renders the imported figures (skeletal path with socket kit, and the GPU-skinned crowd path)
// offscreen to a PNG and reports material compile errors. Needs a real RHI (run with -RenderOffscreen, not -NullRHI).
UCLASS()
class UTCInfantryRenderCheckCommandlet : public UCommandlet
{
	GENERATED_BODY()

public:
	virtual int32 Main(const FString& Params) override;
};
