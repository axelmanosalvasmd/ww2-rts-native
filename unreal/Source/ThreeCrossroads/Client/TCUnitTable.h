// The client's view of the reference UNITS table: only what drawing and order layout need. Loaded from
// Content/Data/units.json, which unreal/Tools/export-unit-table.mjs writes from the pinned reference. Interim until the
// Rust core supplies these through its C ABI.
#pragma once

#include "CoreMinimal.h"

// Reference unit flags this client reads beyond Net's TCFlags (shared/sim.js unitFlags).
namespace TCClientFlags
{
	constexpr int32 PlaneAtBase = 512; // an aircraft on the ground at its airfield: the browser does not draw or pick it
}

struct FTCUnitDef
{
	FString Name;
	float Radius = 1.f;
	bool bInfantry = false, bStructure = false, bAir = false, bRail = false, bBack = false;
	bool bBuilding = false; // a structure the player can click (HQ, depot, ...); a bunker is a structure but not a building
	int32 Models = 1, HpPer = 0;
	int32 Carries = 0;      // squads it carries (client/orders.js kind: a right-click on an own carrier boards it)
	// What a fresh unit of this type starts with (shared/sim.js spawnUnit: models * hpPer), for the HUD's health bar.
	int32 MaxHp() const { return Models * HpPer; }
};

class FTCUnitTable
{
public:
	bool Load(FString& OutError);
	const FTCUnitDef* Find(const FString& Type) const { return Defs.Find(Type); }
	FString Reference;
	int32 Num() const { return Defs.Num(); }

private:
	TMap<FString, FTCUnitDef> Defs;
};
