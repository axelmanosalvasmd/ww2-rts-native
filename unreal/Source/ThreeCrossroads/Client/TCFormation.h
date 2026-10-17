// Where each selected unit is sent by a plain right-click move: shared/formation.js facingSpots and slotSize, with the
// browser's default Formation menu (line, spread 1, snap to trenches) from client/main.js formation and snapToTrench.
// The server still decides every outcome; this only proposes the per-unit spots the browser would send.
//
// Spots and Slot are what the client uses: the Rust core through the WW2Core plugin (ww2-ffi, ABI v1) when it is linked
// and ready, else the interim C++ port below (FacingSpots, SlotSize), which stays as the fallback on platforms without
// the library and as the baseline the automation test compares with. SnapToTrench is client code in the browser too
// and stays here. The facing angle the caller passes still comes from Unreal's atan2, not V8's (see TCOrders.cpp).
#pragma once

#include "CoreMinimal.h"

struct FTCFormationItem
{
	int32 Id = 0;
	double X = 0, Z = 0, Size = 3;
	bool bBack = false;
	bool bInfantry = false;
};

struct FTCSpot
{
	int32 Id = 0;
	double X = 0, Z = 0;
};

enum class ETCFormationSource : uint8
{
	RustCore,    // ww2_formation_spots / ww2_formation_slot_size through WW2Core
	InterimPort, // the C++ port in this file
};

const TCHAR* LexToString(ETCFormationSource Source);

namespace TCFormation
{
	// The Rust core when WW2Core is ready, else the interim port. Source, when given, says which answered.
	double Slot(bool bInfantry, double Radius, ETCFormationSource* Source = nullptr);
	// facingSpots in the default line at spread 1. Ids must be non-negative (they cross as uint32); a negative one, a
	// bridge that is not ready, or a bridge error falls back to the interim port with a warning.
	TArray<FTCSpot> Spots(const TArray<FTCFormationItem>& Items, double AtX, double AtZ, double Face, double Reach, ETCFormationSource* Source = nullptr);

	// The interim C++ port (line shape, spread 1 only).
	double SlotSize(bool bInfantry, double Radius);
	// line shape, spread 1 (the reference Formation menu defaults)
	TArray<FTCSpot> FacingSpots(const TArray<FTCFormationItem>& Items, double AtX, double AtZ, double Face, double Reach);
	// infantry spots within 3 m of a trench cell nobody else took step into it; IsTrench reads the known terrain
	void SnapToTrench(TArray<FTCSpot>& Spots, const TArray<FTCFormationItem>& Items, TFunctionRef<bool(int32, int32)> IsTrench);
}
