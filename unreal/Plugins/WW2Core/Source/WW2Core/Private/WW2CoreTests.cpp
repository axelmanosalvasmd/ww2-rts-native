// Automation tests of the linked Rust core, run inside the engine against the library the module linked:
//   UnrealEditor-Cmd ThreeCrossroads.uproject -nullrhi -ExecCmds="Automation RunTests ThreeCrossroads.WW2Core; Quit"
// Built without the library they check the not-linked path instead.
#include "WW2Core.h"
#include "WW2FormationFixture.h"
#include "WW2ViewReplay.h"
#include "Async/Async.h"
#include "Misc/AutomationTest.h"
#include "HAL/FileManager.h"
#include "Misc/Paths.h"

#if WITH_WW2_FFI
#include "ww2_core.h"
#endif

#if WITH_DEV_AUTOMATION_TESTS

namespace
{
	constexpr EAutomationTestFlags WW2TestFlags = EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter;

	// RULES_HASH as rust/crates/core/src/rules.rs defines it, from the constants of the pinned shared/formation.js
	// (e4e579a): FNV-1a 64 over each SHAPES name and a zero byte, then the little-endian bits of slotSize's 3 and 5, its
	// 1.5 m margin, the 1.5 m rank gap, the 10-unit line rank, the 3x stretch limit and the comparator's 100 steps.
	uint64 ExpectedRulesHash()
	{
		uint64 Hash = 0xCBF29CE484222325ull;
		auto Byte = [&Hash](uint8 B) { Hash = (Hash ^ B) * 0x00000100000001B3ull; };
		for (const char* Name : { "line", "block", "column", "wedge" })
		{
			for (const char* C = Name; *C; C++) Byte((uint8)*C);
			Byte(0);
		}
		for (double V : { 3.0, 5.0, 1.5, 1.5, 10.0, 3.0, 100.0 })
		{
			const uint64 B = FWW2FormationFixture::Bits(V);
			for (int32 I = 0; I < 8; I++) Byte((uint8)(B >> (8 * I)));
		}
		return Hash;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FWW2CoreBridgeTest, "ThreeCrossroads.WW2Core.Bridge", WW2TestFlags)

bool FWW2CoreBridgeTest::RunTest(const FString& Parameters)
{
	AddInfo(FWW2Core::Describe());
#if WITH_WW2_FFI
	TestEqual(TEXT("state"), FString(FWW2Core::StateName(FWW2Core::State())), FString(FWW2Core::StateName(EWW2CoreState::Ready)));
	TestEqual(TEXT("ww2_abi_version() is the header's WW2_ABI_VERSION"), FWW2Core::LibraryAbiVersion(), FWW2Core::HeaderAbiVersion());
	TestEqual(TEXT("ww2_abi_version() is the wrapper's version"), FWW2Core::LibraryAbiVersion(), FWW2Core::WrapperAbiVersion);
	TestEqual(TEXT("ww2_abi_version() is 2 (PR #16 at 03a49ac)"), FWW2Core::LibraryAbiVersion(), 2u);
	TestEqual(TEXT("wrapper written for ABI 2"), FWW2Core::WrapperAbiVersion, 2u);
	TestTrue(TEXT("provider supports the adopted minor revision"), FWW2Core::LibraryAbiMinorVersion() >= FWW2Core::RequiredAbiMinorVersion);
	TestTrue(TEXT("header declares the adopted minor revision"), FWW2Core::HeaderAbiMinorVersion() >= FWW2Core::RequiredAbiMinorVersion);
	const uint64 Hash = FWW2Core::RulesHash();
	AddInfo(FString::Printf(TEXT("ww2_rules_hash() = 0x%016llx, computed here from shared/formation.js = 0x%016llx"), (unsigned long long)Hash, (unsigned long long)ExpectedRulesHash()));
	TestEqual(TEXT("ww2_rules_hash() is the FNV-1a of the pinned formation constants"), Hash, ExpectedRulesHash());
	TestEqual(TEXT("ww2_rules_hash() is 0xed0ee23d32a621d1 (julio-ffi-ready-005)"), Hash, 0xed0ee23d32a621d1ull);
	TestEqual(TEXT("native revision is a full commit id"), FWW2Core::NativeRevision().Len(), 40);
	TestEqual(TEXT("archive sha256 recorded"), FWW2Core::LibrarySha256().Len(), 64);
	TestTrue(TEXT("Math.PI bits"), FWW2FormationFixture::Bits(3.141592653589793) == 0x400921fb54442d18ull);
#else
	TestTrue(TEXT("state is not linked"), FWW2Core::State() == EWW2CoreState::NotLinked);
	TestEqual(TEXT("no library version"), FWW2Core::LibraryAbiVersion(), 0u);
	TestEqual(TEXT("no rules hash"), FWW2Core::RulesHash(), (uint64)0);
	double Size = -1;
	const FWW2Result Slot = FWW2Core::SlotSize(true, 0.5, Size);
	TestEqual(TEXT("slot size says not linked"), Slot.Code, FWW2Core::ErrNotLinked);
	TestEqual(TEXT("slot size output untouched"), Size, -1.0);
	TArray<FWW2FormationSpot> Out;
	Out.Add({ 9, 1, 2 });
	const FWW2Result Spots = FWW2Core::FormationSpots({ FWW2FormationItem{ 1, false, 0, 0, 3 } }, 0, 0, 0, 0, FWW2Core::ShapeLine, 1, Out);
	TestEqual(TEXT("formation spots say not linked"), Spots.Code, FWW2Core::ErrNotLinked);
	TestTrue(TEXT("the message says why"), Spots.Error.StartsWith(TEXT("built without ww2-ffi: ")));
	TestEqual(TEXT("no spots"), Out.Num(), 0);
	AddInfo(Spots.Error);
	FWW2View View;
	TestEqual(TEXT("a view says not linked"), View.Open().Code, FWW2Core::ErrNotLinked);
	TestFalse(TEXT("no view handle"), View.IsOpen());
#endif
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FWW2CoreErrorsTest, "ThreeCrossroads.WW2Core.Errors", WW2TestFlags)

bool FWW2CoreErrorsTest::RunTest(const FString& Parameters)
{
#if WITH_WW2_FFI
	if (!TestTrue(TEXT("bridge ready"), FWW2Core::IsReady())) return false;
	// Through the wrapper: an unknown shape is the library's own refusal, with its message.
	TArray<FWW2FormationSpot> Out;
	Out.Add({ 9, 1, 2 });
	const FWW2FormationItem One{ 1, false, 0, 0, 3 };
	FWW2Result R = FWW2Core::FormationSpots({ One }, 0, 0, 0, 0, 9, 1, Out);
	TestEqual(TEXT("unknown shape 9: code"), R.Code, FWW2Core::ErrBadArgument);
	TestEqual(TEXT("unknown shape 9: message"), R.Error, FString(TEXT("unknown shape 9: expected a WW2_SHAPE_* from 0 to 3")));
	TestEqual(TEXT("unknown shape 9: no spots"), Out.Num(), 0);
	R = FWW2Core::FormationSpots({}, 0, 0, 0, 0, FWW2Core::ShapeWedge, 1, Out);
	TestTrue(TEXT("no items: ok, no spots"), R.IsOk() && Out.Num() == 0);

	// The raw entry points, for what the wrapper never passes: null pointers and short buffers. Sentinels show that
	// nothing is written on failure.
	ww2_formation_item Item{};
	Item.id = 1; Item.size = 3;
	ww2_formation_spot Spot{ 77, 7.0, 7.0 };
	size_t Len = 5;
	auto Message = []() { return FWW2Core::LastError(); };
	auto SpotUntouched = [&Spot]() { return Spot.id == 77 && Spot.x == 7.0 && Spot.z == 7.0; };
	TestEqual(TEXT("capacity 0 for 1 item"), ww2_formation_spots(&Item, 1, 0, 0, 0, 0, WW2_SHAPE_LINE, 1, &Spot, 0, &Len), WW2_ERR_CAPACITY);
	TestEqual(TEXT("capacity: message"), Message(), FString(TEXT("out has room for 0 spots but there are 1 items")));
	TestTrue(TEXT("capacity: out_len 0, spot untouched"), Len == 0 && SpotUntouched());
	Len = 5;
	TestEqual(TEXT("null items"), ww2_formation_spots(nullptr, 1, 0, 0, 0, 0, WW2_SHAPE_LINE, 1, &Spot, 1, &Len), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("null items: message"), Message(), FString(TEXT("items is null")));
	TestTrue(TEXT("null items: out_len 0, spot untouched"), Len == 0 && SpotUntouched());
	TestEqual(TEXT("null out"), ww2_formation_spots(&Item, 1, 0, 0, 0, 0, WW2_SHAPE_LINE, 1, nullptr, 1, &Len), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("null out: message"), Message(), FString(TEXT("out is null")));
	TestEqual(TEXT("null out_len"), ww2_formation_spots(&Item, 1, 0, 0, 0, 0, WW2_SHAPE_LINE, 1, &Spot, 1, nullptr), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("null out_len: message"), Message(), FString(TEXT("out_len is null")));
	TestTrue(TEXT("null out_len: spot untouched"), SpotUntouched());
	Len = 5;
	TestEqual(TEXT("null and 0 items: ok"), ww2_formation_spots(nullptr, 0, 0, 0, 0, 0, WW2_SHAPE_WEDGE, 1, nullptr, 0, &Len), WW2_OK);
	TestEqual(TEXT("null and 0 items: out_len 0"), Len, (size_t)0);
	TestEqual(TEXT("slot size, null out"), ww2_formation_slot_size(1, 0.5, nullptr), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("slot size, null out: message"), Message(), FString(TEXT("out is null")));

	// ww2_last_error: the length first, a short buffer is refused whole, and null out_len is an error.
	size_t MsgLen = 0;
	TestEqual(TEXT("last error: capacity 0 asks the length"), ww2_last_error(nullptr, 0, &MsgLen), WW2_ERR_CAPACITY);
	TestEqual(TEXT("last error: length of 'out is null'"), MsgLen, (size_t)11);
	char Short[4] = { 'x', 'x', 'x', 'x' };
	TestEqual(TEXT("last error: short buffer"), ww2_last_error(Short, sizeof Short, &MsgLen), WW2_ERR_CAPACITY);
	TestTrue(TEXT("last error: short buffer untouched"), Short[0] == 'x' && Short[3] == 'x');
	TestEqual(TEXT("last error: null out_len"), ww2_last_error(Short, sizeof Short, nullptr), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("last error: reading kept it"), Message(), FString(TEXT("out is null")));

	// The message is per thread: a failure on another thread neither shows here nor replaces this thread's message.
	const FString Other = Async(EAsyncExecution::Thread, []()
	{
		const FString Before = FWW2Core::LastError();
		size_t L = 0;
		ww2_formation_spots(nullptr, 2, 0, 0, 0, 0, WW2_SHAPE_LINE, 1, nullptr, 2, &L);
		return FString::Printf(TEXT("[%s] then [%s]"), *Before, *FWW2Core::LastError());
	}).Get();
	TestEqual(TEXT("other thread: empty, then its own message"), Other, FString(TEXT("[] then [items is null]")));
	TestEqual(TEXT("this thread's message unchanged"), Message(), FString(TEXT("out is null")));
	AddInfo(TEXT("WW2_ERR_PANIC: no entry point can be made to panic from outside (the crate's own test covers the catch_unwind boundary); the view wrapper's reaction is checked in ThreeCrossroads.WW2Core.ViewEdges by feeding it the code"));
#else
	AddInfo(FString(TEXT("not linked, nothing to call: ")) + FWW2Core::Describe());
#endif
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FWW2CoreFixtureTest, "ThreeCrossroads.WW2Core.Fixture", WW2TestFlags)

bool FWW2CoreFixtureTest::RunTest(const FString& Parameters)
{
#if WITH_WW2_FFI
	FWW2FormationFixture Fx;
	FString Error;
	const FString Path = FWW2FormationFixture::InstalledPath();
	if (!TestTrue(FString::Printf(TEXT("reference fixture loads (%s): %s"), *Path, *Error), Fx.Load(Path, Error)))
	{
		AddError(Error);
		return false;
	}
	AddInfo(FString::Printf(TEXT("fixture %s: recorded by %s; %d ranges, %d written cases, %d slot sizes, %d faces (not in the C interface)"), *Path, *Fx.Runtime, Fx.Ranges.Num(), Fx.Written.Num(), Fx.Slots.Num(), Fx.Faces));

	int32 SlotDiffer = 0;
	for (const FWW2FixtureSlot& S : Fx.Slots)
	{
		double Size = 0;
		const FWW2Result R = FWW2Core::SlotSize(S.bInfantry, S.Radius, Size);
		if (!R.IsOk() || FWW2FormationFixture::Bits(Size) != FWW2FormationFixture::Bits(S.Size))
		{
			SlotDiffer++;
			AddError(FString::Printf(TEXT("slotSize(%s, %.17g): JavaScript %.17g, ww2-ffi %.17g (%d %s)"), S.bInfantry ? TEXT("infantry") : TEXT("vehicle"), S.Radius, S.Size, Size, R.Code, *R.Error));
		}
	}
	AddInfo(FString::Printf(TEXT("slot sizes: %d of %d bit-equal to the JavaScript"), Fx.Slots.Num() - SlotDiffer, Fx.Slots.Num()));

	int32 WrittenDiffer = 0, WrittenSpots = 0;
	for (const FWW2FixtureCase& W : Fx.Written)
	{
		TArray<FWW2FormationSpot> Out;
		const FWW2Result R = FWW2Core::FormationSpots(W.Items, W.AtX, W.AtZ, W.Face, W.Reach, W.Shape, W.Spread, Out);
		WrittenSpots += W.Spots.Num();
		const bool bSame = R.IsOk() && FWW2FormationFixture::SameSpots(Out, W.Spots);
		if (!bSame)
		{
			WrittenDiffer++;
			AddError(FString::Printf(TEXT("case %s: ww2-ffi %d spots (%d %s), JavaScript %d"), *W.Label, Out.Num(), R.Code, *R.Error, W.Spots.Num()));
		}
		if (W.Items.Num() <= 1 || W.Label == TEXT("line.click") || W.Label == TEXT("spread0.click") || W.Label == TEXT("empty"))
		{
			FString Shown;
			for (const FWW2FormationSpot& S : Out) Shown += FString::Printf(TEXT(" [%u %.17g %.17g]"), S.Id, S.X, S.Z);
			AddInfo(FString::Printf(TEXT("case %s: %d units -> %d spots%s: %s"), *W.Label, W.Items.Num(), Out.Num(), *Shown, bSame ? TEXT("bit-equal") : TEXT("DIFFERS")));
		}
	}
	AddInfo(FString::Printf(TEXT("written cases: %d of %d bit-equal to the JavaScript (%d spots)"), Fx.Written.Num() - WrittenDiffer, Fx.Written.Num(), WrittenSpots));

	for (const FWW2FixtureRange& Range : Fx.Ranges)
	{
		if (!FWW2FormationFixture::CanRebuild(Range))
		{
			AddInfo(FString::Printf(TEXT("range %s (%d cases, styles %s): not rebuilt here, its placement draws V8's cos and sin"), *Range.Label, Range.Count, *FString::Join(Range.Styles, TEXT(","))));
			continue;
		}
		FWW2Mulberry32 Rand(Range.Seed);
		int32 Differ = 0, Checked = 0, InputsChecked = 0, Units = 0, Spots = 0, Large = 0;
		for (int32 I = 0; I < Range.Count; I++)
		{
			const FWW2FixtureCase Case = FWW2FormationFixture::MakeScatterCase(Rand, Range.Fewest, Range.Most);
			const FString Label = FString::Printf(TEXT("%s#%d"), *Range.Label, I);
			if (const FWW2FixtureCase* W = Fx.Written.FindByPredicate([&Label](const FWW2FixtureCase& C) { return C.Label == Label; }))
			{
				InputsChecked++;
				if (!TestTrue(FString::Printf(TEXT("%s: the generator here builds the harness's inputs"), *Label), FWW2FormationFixture::SameInputs(Case, *W))) return false;
			}
			TArray<FWW2FormationSpot> Out;
			const FWW2Result R = FWW2Core::FormationSpots(Case.Items, Case.AtX, Case.AtZ, Case.Face, Case.Reach, Case.Shape, Case.Spread, Out);
			Checked++;
			Units += Case.Items.Num();
			Spots += Out.Num();
			for (const FWW2FormationItem& U : Case.Items) Large += U.Id > (uint32)MAX_int32;
			if (!R.IsOk() || FWW2FormationFixture::HashSpots(Out) != Range.Hashes[I])
			{
				if (++Differ <= 5) AddError(FString::Printf(TEXT("%s: ww2-ffi hash 0x%016llx (%d %s), JavaScript 0x%016llx"), *Label, (unsigned long long)FWW2FormationFixture::HashSpots(Out), R.Code, *R.Error, (unsigned long long)Range.Hashes[I]));
			}
		}
		AddInfo(FString::Printf(TEXT("range %s: %d of %d seeded cases bit-equal to the JavaScript (%d units, %d spots, %d ids above INT32_MAX; %d written-out inputs matched the generator)"),
			*Range.Label, Checked - Differ, Checked, Units, Spots, Large, InputsChecked));
		TestEqual(FString::Printf(TEXT("range %s differing cases"), *Range.Label), Differ, 0);
	}
	TestEqual(TEXT("slot sizes differing"), SlotDiffer, 0);
	TestEqual(TEXT("written cases differing"), WrittenDiffer, 0);
#else
	AddInfo(FString(TEXT("not linked, no fixture run: ")) + FWW2Core::Describe());
#endif
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FWW2CoreViewReplayTest, "ThreeCrossroads.WW2Core.ViewReplay", WW2TestFlags)

bool FWW2CoreViewReplayTest::RunTest(const FString& Parameters)
{
#if WITH_WW2_FFI
	if (!TestTrue(TEXT("bridge ready"), FWW2Core::IsReady())) return false;
	AddInfo(FString::Printf(TEXT("protocol fixtures: %s"), *FWW2ViewReplay::ProtocolDir()));
	int32 Messages = 0;
	int64 Rows = 0;
	for (const FString& Stem : FWW2ViewReplay::Streams())
	{
		FWW2ReplayReport Report;
		const bool bPassed = FWW2ViewReplay::Run(Stem, Report);
		AddInfo(Report.Summary());
		for (const FString& P : Report.Problems) AddError(P);
		TestTrue(FString::Printf(TEXT("%s replays clean"), *Stem), bPassed);
		Messages += Report.Messages;
		Rows += Report.RowsCompared;
	}
	AddInfo(FString::Printf(TEXT("all streams: %d messages, %lld ww2_view rows compared column by column"), Messages, (long long)Rows));
#else
	AddInfo(FString(TEXT("not linked, no replay: ")) + FWW2Core::Describe());
#endif
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FWW2CoreViewEdgesTest, "ThreeCrossroads.WW2Core.ViewEdges", WW2TestFlags)

bool FWW2CoreViewEdgesTest::RunTest(const FString& Parameters)
{
#if WITH_WW2_FFI
	if (!TestTrue(TEXT("bridge ready"), FWW2Core::IsReady())) return false;
	AddInfo(FString::Printf(TEXT("ww2_view_unit: %d bytes, kind[%d] at %d, target at %d"), (int32)sizeof(ww2_view_unit), (int32)sizeof(((ww2_view_unit*)nullptr)->kind),
		(int32)offsetof(ww2_view_unit, kind), (int32)offsetof(ww2_view_unit, target)));

	// Through the wrapper: empty, invalid and full-width input (shared with the startup self-check).
	TArray<FString> Lines, Failures;
	FWW2ViewReplay::RunEdges(Lines, Failures);
	for (const FString& L : Lines) AddInfo(L);
	for (const FString& F : Failures) AddError(F);
	TestEqual(TEXT("wrapper edge checks failing"), Failures.Num(), 0);

	// The raw entry points, for arguments the wrapper never passes. Sentinels show nothing is written on failure.
	TestEqual(TEXT("ww2_view_new(null)"), ww2_view_new(nullptr), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("ww2_view_free(null) is accepted"), ww2_view_free(nullptr), WW2_OK);
	TestEqual(TEXT("ww2_view_apply(null view)"), ww2_view_apply(nullptr, "{}", 2), WW2_ERR_NULL_POINTER);
	ww2_view* Raw = nullptr;
	if (!TestEqual(TEXT("ww2_view_new"), ww2_view_new(&Raw), WW2_OK) || !TestNotNull(TEXT("handle"), Raw)) return false;
	size_t Len = 77;
	TestEqual(TEXT("units: null out_len"), ww2_view_units(Raw, nullptr, 0, nullptr), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("unit_count: null out"), ww2_view_unit_count(Raw, nullptr), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("needs_resync: null out"), ww2_view_needs_resync(Raw, nullptr), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("apply: null json with length 3"), ww2_view_apply(Raw, nullptr, 3), WW2_ERR_NULL_POINTER);
	TestEqual(TEXT("apply: null json with length 0 is a zero-byte message"), ww2_view_apply(Raw, nullptr, 0), WW2_ERR_PARSE);
	AddInfo(FString::Printf(TEXT("zero-byte message: %s"), *FWW2Core::LastError()));
	TestEqual(TEXT("empty view: count query with null out"), ww2_view_units(Raw, nullptr, 0, &Len), WW2_OK);
	TestEqual(TEXT("empty view: 0 rows"), Len, (size_t)0);
	{
		// Over 16 MiB: refused before any read.
		TArray<char> Huge;
		Huge.Init(' ', 16 * 1024 * 1024 + 1);
		TestEqual(TEXT("a message over 16 MiB"), ww2_view_apply(Raw, Huge.GetData(), (size_t)Huge.Num()), WW2_ERR_BAD_ARGUMENT);
		AddInfo(FString::Printf(TEXT("over 16 MiB: %s"), *FWW2Core::LastError()));
	}
	auto ApplyText = [Raw](const char* Text) { return ww2_view_apply(Raw, Text, FCStringAnsi::Strlen(Text)); };
	TestEqual(TEXT("start"), ApplyText("{\"t\":\"start\",\"map\":{\"w\":1,\"h\":1}}"), WW2_OK);
	TestEqual(TEXT("three units, ids at full width"), ApplyText("{\"t\":\"s\",\"held\":[3,2147483646],\"units\":["
		"[4294967295,\"rifle\",0,1,2,0,0,100,0,0,0,0,0,0,1],[2147483648,\"tank\",1,3,4,0,0,100,0,0,0,0,0,0,1],[1,\"mg\",0,5,6,0,0,100,0,0,0,0,0,0,1]]}"), WW2_OK);
	Len = 77;
	TestEqual(TEXT("nonempty view: count query with null out"), ww2_view_units(Raw, nullptr, 0, &Len), WW2_ERR_CAPACITY);
	TestEqual(TEXT("count query reports 3"), Len, (size_t)3);
	ww2_view_unit Buf[3];
	FMemory::Memset(Buf, 0x5A, sizeof Buf);
	ww2_view_unit Before[3];
	FMemory::Memcpy(Before, Buf, sizeof Buf);
	Len = 77;
	TestEqual(TEXT("capacity 2 for 3 rows"), ww2_view_units(Raw, Buf, 2, &Len), WW2_ERR_CAPACITY);
	TestEqual(TEXT("capacity 2: out_len 3"), Len, (size_t)3);
	TestTrue(TEXT("capacity 2: no partial write"), FMemory::Memcmp(Buf, Before, sizeof Buf) == 0);
	TestEqual(TEXT("capacity 3"), ww2_view_units(Raw, Buf, 3, &Len), WW2_OK);
	TestTrue(TEXT("rows ascending at full width"), Len == 3 && Buf[0].id == 1 && Buf[1].id == 2147483648u && Buf[2].id == 4294967295u);
	bool bPadded = true;
	for (const ww2_view_unit& U : Buf)
	{
		const int32 N = FCStringAnsi::Strlen(U.kind);
		for (int32 I = N; I < 64; I++) bPadded &= U.kind[I] == 0;
	}
	TestTrue(TEXT("kind is zero-terminated and zero-padded to 64 bytes"), bPadded);
	TestEqual(TEXT("free"), ww2_view_free(Raw), WW2_OK);

	// Thread rule: another thread is refused before the library is called, until it adopts the view.
	FWW2View View;
	if (!TestEqual(TEXT("open"), View.Open().Code, FWW2Core::Ok)) return false;
	TestEqual(TEXT("start"), View.Apply(FString(TEXT("{\"t\":\"start\",\"map\":{\"w\":1,\"h\":1}}"))).Code, FWW2Core::Ok);
	const FString Other = Async(EAsyncExecution::Thread, [&View]()
	{
		int32 C = -1;
		const FWW2Result Refused = View.UnitCount(C);
		View.Adopt();
		const FWW2Result Adopted = View.UnitCount(C);
		return FString::Printf(TEXT("%d %d %d"), Refused.Code, Adopted.Code, C);
	}).Get();
	TestEqual(TEXT("another thread: refused, then adopted and answered 0 units"), Other, FString::Printf(TEXT("%d %d 0"), FWW2Core::ErrWrongThread, FWW2Core::Ok));
	int32 Count = -1;
	TestEqual(TEXT("the first thread is refused after the hand-over"), View.UnitCount(Count).Code, FWW2Core::ErrWrongThread);
	View.Adopt();
	TestEqual(TEXT("and answers again after adopting it back"), View.UnitCount(Count).Code, FWW2Core::Ok);

	// Poisoning: after a panic code only free is called. (No input makes the library panic; the code is fed in.) The
	// wrapper logs the poisoning once as an error; this test expects exactly that one line.
	AddExpectedMessagePlain(TEXT("ww2 view poisoned (-1 WW2_ERR_PANIC)"), ELogVerbosity::Error, EAutomationExpectedMessageFlags::Contains, 1);
	View.NoteLibraryCodeForTest(FWW2Core::ErrPanic);
	TestTrue(TEXT("poisoned"), View.IsPoisoned());
	TestEqual(TEXT("poisoned: apply refused"), View.Apply(FString(TEXT("{\"t\":\"x\"}"))).Code, FWW2Core::ErrPoisoned);
	TArray<FWW2ViewUnit> Rows;
	Rows.SetNum(1);
	TestEqual(TEXT("poisoned: units refused"), View.Units(Rows).Code, FWW2Core::ErrPoisoned);
	TestEqual(TEXT("poisoned: rows untouched"), Rows.Num(), 1);
	bool bResync = false;
	TestEqual(TEXT("poisoned: needs_resync refused"), View.NeedsResync(bResync).Code, FWW2Core::ErrPoisoned);
	TArray<UTF8CHAR> QueryOut = { (UTF8CHAR)'x' };
	TestEqual(TEXT("poisoned: placement refused before calling Rust"), View.PlacementCheck({}, QueryOut).Code, FWW2Core::ErrPoisoned);
	TestEqual(TEXT("poisoned: placement output untouched"), QueryOut.Num(), 1);
	TestEqual(TEXT("poisoned: free still accepted"), View.Close().Code, FWW2Core::Ok);
	TestFalse(TEXT("closed"), View.IsOpen());
	TestEqual(TEXT("closed: placement refused"), View.PlacementCheck({}, QueryOut).Code, FWW2Core::ErrClosed);
#else
	AddInfo(FString(TEXT("not linked, nothing to call: ")) + FWW2Core::Describe());
#endif
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FWW2CoreSelfCheckFixturesTest, "ThreeCrossroads.WW2Core.SelfCheckFixtures", WW2TestFlags)

// The startup self-check with its recorded stream installed, absent (a package that does not stage it: skipped with a
// log line, the edge checks decide) and half there (an error).
bool FWW2CoreSelfCheckFixturesTest::RunTest(const FString& Parameters)
{
#if WITH_WW2_FFI
	if (!TestTrue(TEXT("bridge ready"), FWW2Core::IsReady())) return false;
	const FString Stem = FWW2ViewReplay::SelfCheckStream();
	const FString Installed = FWW2ViewReplay::ProtocolDir();
	TestEqual(TEXT("the install has the self-check stream"), (int32)FWW2ViewReplay::FindFixtures(Installed, Stem), (int32)EWW2Fixtures::Present);

	FString Summary;
	bool bSkipped = true;
	TestTrue(TEXT("installed: the self-check passes"), FWW2ViewReplay::StartupSelfCheckIn(Installed, Summary, &bSkipped));
	TestFalse(TEXT("installed: the replay ran"), bSkipped);
	TestTrue(TEXT("installed: the summary names the replay's pass"), Summary.Contains(Stem + TEXT(": PASS")));
	AddInfo(Summary.Left(400));

	const FString Root = FPaths::ConvertRelativePathToFull(FPaths::AutomationTransientDir() / TEXT("WW2CoreSelfCheck"));
	IFileManager::Get().DeleteDirectory(*Root, false, true);
	const FString Missing = Root / TEXT("absent");
	TestEqual(TEXT("absent folder: absent"), (int32)FWW2ViewReplay::FindFixtures(Missing, Stem), (int32)EWW2Fixtures::Absent);
	TestEqual(TEXT("no folder: absent"), (int32)FWW2ViewReplay::FindFixtures(FString(), Stem), (int32)EWW2Fixtures::Absent);
	TestTrue(TEXT("absent: the self-check passes on the edge checks"), FWW2ViewReplay::StartupSelfCheckIn(Missing, Summary, &bSkipped));
	TestTrue(TEXT("absent: the replay was skipped"), bSkipped);
	TestTrue(TEXT("absent: the summary says skipped"), Summary.Contains(TEXT("SKIPPED")) && Summary.Contains(TEXT("Edges: ")));
	AddInfo(Summary.Left(400));

	const FString Half = Root / TEXT("partial");
	IFileManager::Get().MakeDirectory(*Half, true);
	TestTrue(TEXT("partial: copy only the messages"), IFileManager::Get().Copy(*(Half / (Stem + TEXT(".jsonl"))), *(Installed / (Stem + TEXT(".jsonl")))) == COPY_OK);
	TestEqual(TEXT("partial: partial"), (int32)FWW2ViewReplay::FindFixtures(Half, Stem), (int32)EWW2Fixtures::Partial);
	TestFalse(TEXT("partial: the self-check fails"), FWW2ViewReplay::StartupSelfCheckIn(Half, Summary, &bSkipped));
	TestFalse(TEXT("partial: not a skip"), bSkipped);
	TestTrue(TEXT("partial: the summary says partial"), Summary.Contains(TEXT("partial install or staging")));

	TestTrue(TEXT("whole copy: add the oracle"), IFileManager::Get().Copy(*(Half / (Stem + TEXT(".expected.jsonl"))), *(Installed / (Stem + TEXT(".expected.jsonl")))) == COPY_OK);
	TestTrue(TEXT("a copied pair replays as the install does"), FWW2ViewReplay::StartupSelfCheckIn(Half, Summary, &bSkipped) && !bSkipped);
	IFileManager::Get().DeleteDirectory(*Root, false, true);
#else
	AddInfo(FString(TEXT("not linked, no self-check: ")) + FWW2Core::Describe());
#endif
	return true;
}

#endif
