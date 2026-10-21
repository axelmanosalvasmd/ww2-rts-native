// Verification only: compiled into a scratch copy of the module by unreal/Tools/motion/run.sh, never into the game.
#pragma once

#include "CoreMinimal.h"
#include "Commandlets/Commandlet.h"
#include "TCMotionCheck.generated.h"

// Renders the soldier crowd (near men in several postures, the far figures with their morphs, camouflaged men,
// bodies near and far, a fading body) and moving vehicles offscreen, checks the materials compile, the far figures'
// keyframes reproduce the exported morph targets, and writes a JSON report and PNG captures.
// Needs a real RHI (-RenderOffscreen -AllowCommandletRendering).
UCLASS()
class UTCMotionCheckCommandlet : public UCommandlet
{
	GENERATED_BODY()

public:
	virtual int32 Main(const FString& Params) override;
};
