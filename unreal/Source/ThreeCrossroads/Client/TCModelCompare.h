// Side-by-side evidence for the unit models: renders units through ATCUnitRenderer (the game's own drawing path, fed a
// synthetic snapshot) from the cameras the reference viewer page used, so each picture pairs with a browser one.
//
//   UnrealEditor-Cmd ThreeCrossroads.uproject -run=TCModelCompare -Cameras=<cameras.json> -Out=<dir>
//       -AllowCommandletRendering -RenderOffscreen [-TCModels=all]
//
// cameras.json comes from unreal/Tools/models/compare/compare.html (it builds the same units with the pinned
// reference's builders and writes the cameras, the light and the ground it used). Writes <dir>/<asset>-<view>-unreal.png
// and <dir>/unreal-report.json.
#pragma once

#include "CoreMinimal.h"
#include "Commandlets/Commandlet.h"
#include "TCModelCompare.generated.h"

UCLASS()
class THREECROSSROADS_API UTCModelCompareCommandlet : public UCommandlet
{
	GENERATED_BODY()

public:
	UTCModelCompareCommandlet();
	virtual int32 Main(const FString& Params) override;
};
