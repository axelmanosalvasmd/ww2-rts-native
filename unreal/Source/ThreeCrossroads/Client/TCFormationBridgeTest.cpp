// ThreeCrossroads.Formation.Bridge: the client's formation entry points (TCFormation::Spots and Slot) against the
// reference fixture installed with the Rust core, and the interim C++ port measured against the same fixture. Run:
//   UnrealEditor-Cmd ThreeCrossroads.uproject -nullrhi -ExecCmds="Automation RunTests ThreeCrossroads.Formation; Quit"
#include "Client/TCFormation.h"
#include "WW2Core.h"
#include "WW2FormationFixture.h"
#include "Misc/AutomationTest.h"

#if WITH_DEV_AUTOMATION_TESTS

namespace
{
	// The interim port carries ids as int32; the fixture's are uint32, so they cross as the same 32 bits.
	TArray<FTCFormationItem> ToClient(TConstArrayView<FWW2FormationItem> Items)
	{
		TArray<FTCFormationItem> Out;
		for (const FWW2FormationItem& U : Items) Out.Add({ (int32)U.Id, U.X, U.Z, U.Size, U.bBack, false });
		return Out;
	}

	TArray<FWW2FormationSpot> FromClient(const TArray<FTCSpot>& Spots)
	{
		TArray<FWW2FormationSpot> Out;
		for (const FTCSpot& S : Spots) Out.Add({ (uint32)S.Id, S.X, S.Z });
		return Out;
	}

	bool AllIdsFitInt32(TConstArrayView<FWW2FormationItem> Items)
	{
		for (const FWW2FormationItem& U : Items) if (U.Id > (uint32)MAX_int32) return false;
		return true;
	}

	// What the interim port covers and the client asks for: the line at spread 1.
	bool InterimShape(const FWW2FixtureCase& C) { return C.Shape == FWW2Core::ShapeLine && FWW2FormationFixture::Bits(C.Spread) == FWW2FormationFixture::Bits(1.0); }
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFormationBridgeTest, "ThreeCrossroads.Formation.Bridge", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCFormationBridgeTest::RunTest(const FString& Parameters)
{
	AddInfo(FWW2Core::Describe());
	const bool bReady = FWW2Core::IsReady();
	const ETCFormationSource Expected = bReady ? ETCFormationSource::RustCore : ETCFormationSource::InterimPort;

	// Routing: the client's entry points answer from the core when it is ready, else from the interim port, and say so.
	const TArray<FTCFormationItem> Squad = { { 11, 100, 100, 3, false, true }, { 12, 104, 101, 5.9, false, false }, { 13, 98, 96, 3, true, true } };
	ETCFormationSource Source = ETCFormationSource::InterimPort;
	const TArray<FTCSpot> Got = TCFormation::Spots(Squad, 120, 80, 0.7, 0, &Source);
	TestEqual(TEXT("Spots answers from"), FString(LexToString(Source)), FString(LexToString(Expected)));
	TestEqual(TEXT("one spot per unit"), Got.Num(), Squad.Num());
	double Size = 0;
	TCFormation::Slot(true, 0.5, &Source);
	TestEqual(TEXT("Slot answers from"), FString(LexToString(Source)), FString(LexToString(Expected)));
	TestEqual(TEXT("Slot(infantry, 0.5)"), TCFormation::Slot(true, 0.5), 3.0, 0.0);
	TestEqual(TEXT("Slot(vehicle, 2.2)"), TCFormation::Slot(false, 2.2), TCFormation::SlotSize(false, 2.2), 0.0);
	if (bReady)
	{
		TArray<FWW2FormationSpot> Direct;
		TArray<FWW2FormationItem> In;
		for (const FTCFormationItem& U : Squad) In.Add({ (uint32)U.Id, U.bBack, U.X, U.Z, U.Size });
		TestTrue(TEXT("the bridge call succeeds"), FWW2Core::FormationSpots(In, 120, 80, 0.7, 0, FWW2Core::ShapeLine, 1, Direct).IsOk());
		TestTrue(TEXT("Spots is ww2_formation_spots, bit for bit"), FWW2FormationFixture::SameSpots(FromClient(Got), Direct));
		// A negative id cannot cross as uint32: the interim port answers, with a warning in the log.
		TArray<FTCFormationItem> Odd = Squad;
		Odd[1].Id = -5;
		AddExpectedMessagePlain(TEXT("does not fit the interface's uint32"), ELogVerbosity::Warning, EAutomationExpectedMessageFlags::Contains, 1);
		const TArray<FTCSpot> Fallback = TCFormation::Spots(Odd, 120, 80, 0.7, 0, &Source);
		TestEqual(TEXT("negative id: interim port"), FString(LexToString(Source)), FString(LexToString(ETCFormationSource::InterimPort)));
		TestTrue(TEXT("negative id: the interim port's spots"), FWW2FormationFixture::SameSpots(FromClient(Fallback), FromClient(TCFormation::FacingSpots(Odd, 120, 80, 0.7, 0))));
	}
	else
	{
		TestTrue(TEXT("not ready: Spots is the interim port, bit for bit"), FWW2FormationFixture::SameSpots(FromClient(Got), FromClient(TCFormation::FacingSpots(Squad, 120, 80, 0.7, 0))));
		AddInfo(TEXT("no fixture comparison: the fixture is installed with the library"));
		return true;
	}

	FWW2FormationFixture Fx;
	FString Error;
	if (!TestTrue(FString::Printf(TEXT("reference fixture loads: %s"), *Error), Fx.Load(FWW2FormationFixture::InstalledPath(), Error))) return false;

	// slotSize: the client's Slot (the core) and the interim port, against the JavaScript.
	int32 SlotCore = 0, SlotInterim = 0;
	for (const FWW2FixtureSlot& S : Fx.Slots)
	{
		SlotCore += FWW2FormationFixture::Bits(TCFormation::Slot(S.bInfantry, S.Radius)) != FWW2FormationFixture::Bits(S.Size);
		SlotInterim += FWW2FormationFixture::Bits(TCFormation::SlotSize(S.bInfantry, S.Radius)) != FWW2FormationFixture::Bits(S.Size);
	}
	AddInfo(FString::Printf(TEXT("slotSize, %d cases: client (Rust core) differs from the JavaScript in %d, interim port in %d"), Fx.Slots.Num(), SlotCore, SlotInterim));
	TestEqual(TEXT("client slot sizes differing from the JavaScript"), SlotCore, 0);

	// facingSpots, line at spread 1 (what the client sends): written cases spot by spot, the scatter range by hash.
	int32 Cases = 0, CoreDiffer = 0, InterimDiffer = 0, ClickCases = 0, ClickInterimDiffer = 0;
	TArray<FString> InterimExamples;
	auto Compare = [&](const FWW2FixtureCase& C, TFunctionRef<bool(TConstArrayView<FWW2FormationSpot>)> MatchesJs)
	{
		const TArray<FTCFormationItem> Items = ToClient(C.Items);
		Cases++;
		const bool bClick = FWW2FormationFixture::Bits(C.Reach) == FWW2FormationFixture::Bits(0.0);
		ClickCases += bClick;
		if (AllIdsFitInt32(C.Items))
		{
			ETCFormationSource From = ETCFormationSource::InterimPort;
			const TArray<FTCSpot> Client = TCFormation::Spots(Items, C.AtX, C.AtZ, C.Face, C.Reach, &From);
			if (From != ETCFormationSource::RustCore || !MatchesJs(FromClient(Client))) { CoreDiffer++; AddError(FString::Printf(TEXT("%s: the client's spots differ from the JavaScript"), *C.Label)); }
		}
		if (!MatchesJs(FromClient(TCFormation::FacingSpots(Items, C.AtX, C.AtZ, C.Face, C.Reach))))
		{
			InterimDiffer++;
			ClickInterimDiffer += bClick;
			if (InterimExamples.Num() < 8) InterimExamples.Add(FString::Printf(TEXT("%s (%d units, reach %g)"), *C.Label, C.Items.Num(), C.Reach));
		}
	};
	for (const FWW2FixtureCase& W : Fx.Written)
		if (InterimShape(W)) Compare(W, [&W](TConstArrayView<FWW2FormationSpot> S) { return FWW2FormationFixture::SameSpots(S, W.Spots); });
	const int32 WrittenCases = Cases;
	for (const FWW2FixtureRange& Range : Fx.Ranges)
	{
		if (!FWW2FormationFixture::CanRebuild(Range)) continue;
		FWW2Mulberry32 Rand(Range.Seed);
		for (int32 I = 0; I < Range.Count; I++)
		{
			FWW2FixtureCase C = FWW2FormationFixture::MakeScatterCase(Rand, Range.Fewest, Range.Most);
			C.Label = FString::Printf(TEXT("%s#%d"), *Range.Label, I);
			const uint64 Hash = Range.Hashes[I];
			if (InterimShape(C)) Compare(C, [Hash](TConstArrayView<FWW2FormationSpot> S) { return FWW2FormationFixture::HashSpots(S) == Hash; });
		}
	}
	AddInfo(FString::Printf(TEXT("facingSpots, line at spread 1: %d cases (%d written, %d seeded), %d with no drag (the client's plain click)"), Cases, WrittenCases, Cases - WrittenCases, ClickCases));
	AddInfo(FString::Printf(TEXT("client (Rust core) differs from the JavaScript in %d; the interim C++ port in %d (%d with no drag)%s%s"), CoreDiffer, InterimDiffer, ClickInterimDiffer,
		InterimExamples.Num() ? TEXT(", e.g. ") : TEXT(""), *FString::Join(InterimExamples, TEXT(", "))));
	TestEqual(TEXT("client formations differing from the JavaScript"), CoreDiffer, 0);
	return true;
}

#endif
