// Approved ABI 2.1 IF-04/IF-12 adapters. Simulation units: metres, radians, x east and z south.
// No authoritative live map, new editor UI, World admission, table transport or IF-11 adoption is introduced here.
#pragma once

#include "CoreMinimal.h"
#include "WW2Core.h"

class FTCMatchState;
class FJsonObject;
class FJsonValue;

// Closed, case-sensitive contract vocabulary. A new provider reason fails decoding until reviewed.
enum class ETCRustPlacementReason : uint8 { None, Blocked, NotVisible, Territory, Rail, Coast };

struct FTCRustPlacementCell
{
	int32 Cell = -1;
	// Building entries are integers. Fort entries are [cell, character], including an explicit null character.
	bool bFort = false;
	TOptional<FString> Character;
};

struct FTCRustPlacement
{
	bool bOk = false, bHasCells = false;
	ETCRustPlacementReason Reason = ETCRustPlacementReason::None;

	FVector2D At = FVector2D::ZeroVector;
	TOptional<int32> Cell;
	TArray<FTCRustPlacementCell> Cells;
	TSharedPtr<FJsonObject> Node;
};

class FTCRustQueries
{
public:
	// Advisory lobby hash diagnostic. Empty for an absent reference hash or an exact match.
	// It covers the provider's formation-only hash, not the unexported static tables.
	static FString RulesWarning(const FJsonObject& Lobby);
	static const TCHAR* ReasonCode(ETCRustPlacementReason Reason);
	// Query the actual adopted client view; no handle escapes. On any failure Out is unchanged, and callers may use
	// their existing C++ mirror. World is deliberately left on that fallback, preserving its remembered terrain.
	// Direct footAt includes Team. Entrenchment/work previews omit it. Neither choice is silently changed here.
	static FWW2Result Placement(const FTCMatchState& State, const FString& Kind, const FVector2D& At,
		TOptional<double> Direction, TOptional<int32> Team, FTCRustPlacement& Out);
	static FWW2Result DecodePlacement(const FString& Json, FTCRustPlacement& Out);

	// Raw JSON retains u64 literals without an Unreal double round trip. Output is explicit-length JSON copied into
	// caller-owned Unreal storage. CreatePreview creates no handle or retained allocation and requires no free.
	// These accept only an author's own or public map document, never a live authoritative hidden map.
	static FWW2Result AuthoringJson(EWW2AuthoringQuery Query, const FString& Json, FString& Out);
	static FWW2Result Authoring(EWW2AuthoringQuery Query, const FString& Json, TSharedPtr<FJsonValue>& Out);
	// Synchronous worker-only helper. A zero-tick request still constructs the match. Both capacity and fill simulate.
	static FWW2Result PlayMatch(const FString& MapJson, const FString& OptionsJson, uint64 Seed, int32 MaxTicks, FString& Out);
};
