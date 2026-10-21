// The reference fixture for formation spots: rust/crates/core/tests/fixtures/formation.txt from the native repo,
// written by harness/formation-fixture.mjs from the pinned shared/formation.js (e4e579a) under x64 Node, installed
// beside the library by unreal/Tools/ww2-ffi.sh. Numbers in it are IEEE 754 bits, so comparisons here are bit for bit.
//
// Seeded ranges hold one hash per case; a range can be rebuilt here only when its placement style is "scatter" alone,
// because the other styles place units with V8's cos and sin, which neither Unreal nor the linked library offers yet.
#pragma once

#include "CoreMinimal.h"
#include "WW2Core.h"

struct FWW2FixtureCase
{
	FString Label;
	int32 Shape = FWW2Core::ShapeLine; // WW2_SHAPE_*
	double Spread = 1, Reach = 0, Face = 0, AtX = 0, AtZ = 0;
	TArray<FWW2FormationItem> Items;
	TArray<FWW2FormationSpot> Spots; // what the JavaScript returned (written cases only)
	int32 DeclaredItems = 0, DeclaredSpots = 0;
};

struct FWW2FixtureRange
{
	FString Label;
	uint32 Seed = 0;
	int32 Count = 0, Fewest = 0, Most = 0;
	TArray<FString> Styles;
	TArray<uint64> Hashes;
};

struct FWW2FixtureSlot
{
	bool bInfantry = false;
	double Radius = 0, Size = 0;
};

// The harness's `seeded` (mulberry32 in wrapping u32 arithmetic).
struct WW2CORE_API FWW2Mulberry32
{
	uint32 State = 0;
	explicit FWW2Mulberry32(uint32 Seed) : State(Seed) {}
	double Next();
	double Below(double N);
	int32 Index(int32 N);
};

struct WW2CORE_API FWW2FormationFixture
{
	FString Runtime; // the fixture's node, v8 and arch lines
	TArray<FWW2FixtureRange> Ranges;
	TArray<FWW2FixtureCase> Written;
	TArray<FWW2FixtureSlot> Slots;
	int32 Faces = 0; // normalizeFace lines: counted, not checked (not in the C interface)

	// Where ww2-ffi.sh installs the fixture for this platform; empty where nothing is installed or supported.
	static FString InstalledPath();
	bool Load(const FString& Path, FString& Error);

	static bool CanRebuild(const FWW2FixtureRange& Range);
	// The harness's makeCase for a scatter-only range (CanRebuild).
	static FWW2FixtureCase MakeScatterCase(FWW2Mulberry32& Rand, int32 Fewest, int32 Most);
	// The harness's hashSpots: FNV-1a 64 over each spot's id (as a double), x and z bits.
	static uint64 HashSpots(TConstArrayView<FWW2FormationSpot> Spots);
	// The bits the fixture records: JavaScript has one NaN.
	static uint64 Bits(double X);
	static bool SameInputs(const FWW2FixtureCase& A, const FWW2FixtureCase& B);
	static bool SameSpots(TConstArrayView<FWW2FormationSpot> A, TConstArrayView<FWW2FormationSpot> B);
};
