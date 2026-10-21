#pragma once
#include "CoreMinimal.h"
namespace TCLook30
{
 // client/hud.js tagsOf. Count comes only from the recipient-delivered CARGO flag bits.
 inline FString CargoLabel(int32 Count,int32 Capacity)
 {
  return Capacity>1 && Capacity<=3 ? FString::Printf(TEXT("Carrying %d of %d squads"),Count,Capacity) : FString(TEXT("Carrying a squad"));
 }
}
