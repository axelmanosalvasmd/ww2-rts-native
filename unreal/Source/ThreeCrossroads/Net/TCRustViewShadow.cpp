#include "Net/TCRustViewShadow.h"
#include "Net/TCMatchState.h"
#include "ThreeCrossroads.h"
#include "WW2ViewReplay.h"
#include "Algo/BinarySearch.h"
#include "HAL/PlatformMisc.h"

namespace
{
	// Empty when the C++ row is the Rust row after the C++ decoder's narrowing; else the first column that differs.
	FString Differs(const FWW2ViewUnit& R, const FTCUnit& C)
	{
		if (C.Type != R.Kind) return FString::Printf(TEXT("kind rust '%s' C++ '%s'"), *R.Kind, *C.Type);
		if (C.Owner != R.Owner) return FString::Printf(TEXT("owner rust %d C++ %d"), R.Owner, C.Owner);
		const struct { double Rust; float Cpp; const TCHAR* Name; } Floats[] = {
			{ R.X, C.X, TEXT("x") }, { R.Z, C.Z, TEXT("z") }, { R.Rot, C.Rot, TEXT("rot") }, { R.Aim, C.Aim, TEXT("aim") }, { R.Built, C.Built, TEXT("built") } };
		for (const auto& F : Floats) if ((float)F.Rust != F.Cpp) return FString::Printf(TEXT("%s rust %.17g C++ %.9g"), F.Name, F.Rust, F.Cpp);
		const struct { double Rust; int32 Cpp; const TCHAR* Name; } Ints[] = {
			{ R.Hp, C.Hp, TEXT("hp") }, { R.Supp, C.Supp, TEXT("supp") }, { (double)R.Target, C.Target, TEXT("target") }, { (double)R.Cover, C.Cover, TEXT("cover") },
			{ R.Cd, C.Cd, TEXT("cd") }, { (double)R.Flags, C.Flags, TEXT("flags") }, { (double)R.Stars, C.Stars, TEXT("stars") } };
		for (const auto& I : Ints) if (FTCMatchState::NarrowInt(I.Rust) != I.Cpp) return FString::Printf(TEXT("%s rust %.17g C++ %d"), I.Name, I.Rust, I.Cpp);
		return FString();
	}

	// The C++ decoder keeps less than the wire sent for this row (information, not a disagreement).
	bool Narrowed(const FWW2ViewUnit& R)
	{
		for (double V : { R.X, R.Z, R.Rot, R.Aim, R.Built }) if ((double)(float)V != V) return true;
		for (double V : { R.Hp, R.Supp, R.Cd }) if (V != FMath::TruncToDouble(V) || (double)FTCMatchState::NarrowInt(V) != FMath::TruncToDouble(V)) return true;
		return R.Id > (uint32)MAX_int32 || R.Target > (uint32)MAX_int32 || R.Cover > (uint32)MAX_int32 || R.Flags > (uint32)MAX_int32 || R.Stars > (uint32)MAX_int32;
	}

	// One Rust row as the FTCUnit the C++ decoder would make of the same wire row (its tail aside).
	void ToUnit(const FWW2ViewUnit& R, FTCUnit& Out)
	{
		const double Columns[15] = { (double)R.Id, 0.0, (double)R.Owner, R.X, R.Z, R.Rot, R.Aim, R.Hp, R.Supp, (double)R.Target, (double)R.Cover, R.Cd,
			(double)R.Flags, (double)R.Stars, R.Built };
		FTCMatchState::FillUnit(Out, R.Kind, Columns);
	}

	FString CountsText(const FTCRustViewShadow::FCounts& C)
	{
		return FString::Printf(TEXT("snapshots %d: agree %d, disagree %d (rust rejected %d, id sets differ %d, rows differ %d, resync differs %d), not compared %d; rows compared %lld, agree %lld, narrowed by C++ %lld; starts %d, agree %d"),
			C.Snapshots, C.Agree, C.Disagree, C.RustRejected, C.IdSetDiffer, C.RowsDiffer, C.ResyncDiffer, C.CppIgnored, (long long)C.RowsCompared, (long long)C.RowsAgree,
			(long long)C.RowsNarrowed, C.Starts, C.StartsAgree);
	}

	FString AdoptionText(const FTCRustViewShadow::FCounts& C)
	{
		return FString::Printf(TEXT("snapshots adopted %d (rows %lld, tails from C++ %lld, ids above int32 dropped %lld), refused %d, not fed %d; resync asked by the view %d"),
			C.Adopted, (long long)C.RowsAdopted, (long long)C.TailsFromCpp, (long long)C.Unrepresentable, C.Refused, C.NotFed, C.RustResync);
	}
}

FTCRustViewShadow::~FTCRustViewShadow()
{
	Detach();
}

const TCHAR* FTCRustViewShadow::DecoderName(ETCDecoder Mode)
{
	switch (Mode)
	{
	case ETCDecoder::Cpp: return TEXT("cpp");
	case ETCDecoder::Shadow: return TEXT("shadow");
	case ETCDecoder::Rust: return TEXT("rust");
	}
	return TEXT("?");
}

ETCDecoder FTCRustViewShadow::ParseDecoder(const FString& Decoder, const FString& LegacyRustView, FString& OutNote, bool& bOutExplicit)
{
	const FString D = Decoder.TrimStartAndEnd().ToLower();
	const FString L = LegacyRustView.TrimStartAndEnd();
	bOutExplicit = true;
	if (!D.IsEmpty())
	{
		const FString Ignored = L.IsEmpty() ? FString() : FString::Printf(TEXT("; TC_RUST_VIEW=%s is ignored (TC_DECODER is set)"), *L);
		if (D == TEXT("rust")) { OutNote = TEXT("TC_DECODER=rust") + Ignored; return ETCDecoder::Rust; }
		if (D == TEXT("cpp")) { OutNote = TEXT("TC_DECODER=cpp") + Ignored; return ETCDecoder::Cpp; }
		if (D == TEXT("shadow")) { OutNote = TEXT("TC_DECODER=shadow") + Ignored; return ETCDecoder::Shadow; }
		OutNote = FString::Printf(TEXT("TC_DECODER=%s is not rust, cpp or shadow: using shadow%s"), *Decoder, *Ignored);
		return ETCDecoder::Shadow;
	}
	if (L == TEXT("1")) { OutNote = TEXT("TC_RUST_VIEW=1 (the old name of TC_DECODER=shadow)"); return ETCDecoder::Shadow; }
	if (L == TEXT("0")) { OutNote = TEXT("TC_RUST_VIEW=0 (the old name of TC_DECODER=cpp)"); return ETCDecoder::Cpp; }
	bOutExplicit = false;
	// slice 12 lead: rust is the default (cpp and shadow stay as fallbacks, see the header)
	OutNote = L.IsEmpty() ? FString(TEXT("TC_DECODER unset: rust (the default)")) : FString::Printf(TEXT("TC_DECODER unset, TC_RUST_VIEW=%s not 0 or 1: rust (the default)"), *L);
	return ETCDecoder::Rust;
}

ETCDecoder FTCRustViewShadow::RequestedDecoder(FString* OutNote, bool* bOutExplicit)
{
	FString Note;
	bool bExplicit = false;
	const ETCDecoder Mode = ParseDecoder(FPlatformMisc::GetEnvironmentVariable(TEXT("TC_DECODER")), FPlatformMisc::GetEnvironmentVariable(TEXT("TC_RUST_VIEW")), Note, bExplicit);
	if (OutNote) *OutNote = Note;
	if (bOutExplicit) *bOutExplicit = bExplicit;
	return Mode;
}

bool FTCRustViewShadow::Requested()
{
	FString Note;
	if (RequestedDecoder(&Note) != ETCDecoder::Cpp) return true;
	// The game mode calls Begin only when a view is asked for: say here, once, that none is.
	static bool bSaid = false;
	if (!bSaid) { bSaid = true; UE_LOG(LogTC, Log, TEXT("decoder: %s. The C++ decoder alone; no Rust view"), *Note); }
	return false;
}

bool FTCRustViewShadow::Begin()
{
	FString Note;
	bool bExplicit = false;
	ETCDecoder Mode = RequestedDecoder(&Note, &bExplicit);
	UE_LOG(LogTC, Log, TEXT("decoder: %s"), *Note);
	if (Mode == ETCDecoder::Rust)
	{
		// asked for by name it is an error of the caller; as the default it is only a note
		if (bExplicit) { UE_LOG(LogTC, Error, TEXT("decoder: TC_DECODER=rust needs the match state (FTCRustViewShadow::Begin(State)); this caller gave none, so the Rust view runs as shadow")); }
		else { UE_LOG(LogTC, Log, TEXT("decoder: the default rust needs the match state (Begin(State)); this caller gave none, so the Rust view runs as shadow")); }
		Mode = ETCDecoder::Shadow;
	}
	return BeginMode(Mode, nullptr, bExplicit);
}

bool FTCRustViewShadow::Begin(FTCMatchState& State)
{
	FString Note;
	bool bExplicit = false;
	const ETCDecoder Mode = RequestedDecoder(&Note, &bExplicit);
	UE_LOG(LogTC, Log, TEXT("decoder: %s"), *Note);
	return BeginMode(Mode, &State, bExplicit);
}

bool FTCRustViewShadow::BeginMode(ETCDecoder InRequested, FTCMatchState* State, bool bExplicit, bool bSelfCheck)
{
	End();
	All = FCounts(); Segment = FCounts();
	Messages = ApplyErrors = SegmentNumber = WarnedSnapshots = WarnedNotFed = RefusedRun = 0;
	Demoted.Reset(); LastApply = FWW2Result(); bPending = bComparedInState = false;
	Requested_ = InRequested;
	Effective = ETCDecoder::Cpp;
	if (InRequested == ETCDecoder::Cpp)
	{
		UE_LOG(LogTC, Log, TEXT("decoder: cpp. The C++ decoder alone; no Rust view"));
		return false;
	}
	if (!FWW2Core::IsReady())
	{
		// The default asks for rust on every platform; where the library is not linked that is expected, not an error.
		if (bExplicit) { UE_LOG(LogTC, Error, TEXT("decoder: %s was asked for, but the Rust core is not ready (%s): the C++ decoder alone"), DecoderName(InRequested), *FWW2Core::Describe()); }
		else { UE_LOG(LogTC, Log, TEXT("decoder: no Rust view (%s): the C++ decoder alone"), *FWW2Core::Describe()); }
		return false;
	}
	UE_LOG(LogTC, Log, TEXT("rust view shadow: %s requested; %s"), DecoderName(InRequested), *FWW2Core::Describe());
	if (bSelfCheck)
	{
		FString Summary;
		if (!FWW2ViewReplay::StartupSelfCheck(Summary))
		{
			UE_LOG(LogTC, Error, TEXT("rust view shadow: off, the startup self-check failed: %s. The C++ decoder alone"), *Summary);
			return false;
		}
		UE_LOG(LogTC, Log, TEXT("rust view shadow: %s"), *Summary);
	}
	const FWW2Result Opened = View.Open();
	if (!Opened.IsOk())
	{
		UE_LOG(LogTC, Error, TEXT("rust view shadow: off, ww2_view_new failed: %d %s. The C++ decoder alone"), Opened.Code, *Opened.Error);
		return false;
	}
	bActive = true;
	SegmentLabel = TEXT("before the first start");
	Effective = InRequested;
	if (InRequested == ETCDecoder::Rust)
	{
		if (!State)
		{
			Effective = ETCDecoder::Shadow;
			UE_LOG(LogTC, Error, TEXT("decoder: rust without a match state: running as shadow"));
		}
		else
		{
			Attached = State;
			State->SetUnitAuthority(this);
		}
	}
	if (Effective == ETCDecoder::Rust) { UE_LOG(LogTC, Log, TEXT("decoder: rust. The Rust view's unit rows and held verdict drive the client; the C++ decoder supplies row tails, fog, cells, the map and every other field, and is compared on every snapshot")); }
	else { UE_LOG(LogTC, Log, TEXT("decoder: shadow. Every received message also goes to ww2_view_apply; the C++ decoder drives the client")); }
	return true;
}

void FTCRustViewShadow::Detach()
{
	if (Attached && Attached->GetUnitAuthority() == this) Attached->SetUnitAuthority(nullptr);
	Attached = nullptr;
}

void FTCRustViewShadow::Demote(ETCDecoder To, const FString& Why)
{
	if (Effective == ETCDecoder::Rust)
		UE_LOG(LogTC, Error, TEXT("decoder: rust demoted to %s for this session: %s. The C++ rows drive the client from the next snapshot"), DecoderName(To), *Why);
	Demoted = Why;
	Detach();
	Effective = To;
}

void FTCRustViewShadow::Feed(const FString& Text)
{
	if (!bActive) return;
	Messages++;
	LastApply = View.Apply(Text);
	bPending = true;
	if (!LastApply.IsOk())
	{
		ApplyErrors++;
		// WRONG_STATE before any start is the documented answer to a snapshot with no match; log the first few of any.
		if (ApplyErrors <= 10) UE_LOG(LogTC, Warning, TEXT("rust view shadow: ww2_view_apply %d (%s): %s [%d bytes, message %d]"), LastApply.Code, FWW2Core::CodeName(LastApply.Code), *LastApply.Error, FTCHARToUTF8(*Text, Text.Len()).Length(), Messages);
		if (View.IsPoisoned())
		{
			UE_LOG(LogTC, Error, TEXT("rust view shadow: the view is poisoned; no Rust view from here, the C++ decoder alone"));
			Demote(ETCDecoder::Cpp, TEXT("the view is poisoned (a caught panic)"));
			LogTotals(TEXT("poisoned"));
			bActive = false;
			bPending = false;
			View.Close();
		}
	}
}

void FTCRustViewShadow::OnStateStart(bool bCppAccepted)
{
	if (!bActive) return;
	const bool bFed = bPending;
	bPending = false;
	if (!IsAuthoritative()) return;
	RefusedRun = 0;
	if (bFed && !LastApply.IsOk() && bCppAccepted)
		UE_LOG(LogTC, Warning, TEXT("decoder: rust: the view refused a start the C++ decoder took (%d %s); its snapshots will be refused until a start it takes"), LastApply.Code, *LastApply.Error);
}

void FTCRustViewShadow::OnStateSnapshotIgnored()
{
	bPending = false;
}

void FTCRustViewShadow::AfterStart(const FTCMatchState& State, bool bCppAccepted, bool bReconnect, int32 StartNumber)
{
	if (!bActive) return;
	bPending = false;
	if (SegmentNumber > 0 || Segment.Snapshots > 0) UE_LOG(LogTC, Log, TEXT("rust view shadow: segment %d (%s) closed: %s"), SegmentNumber, *SegmentLabel, *CountsText(Segment));
	Segment = FCounts();
	SegmentNumber = StartNumber;
	SegmentLabel = FString::Printf(TEXT("start #%d, match %d%s"), StartNumber, State.MatchId, bReconnect ? TEXT(", reconnect") : TEXT(""));
	int32 RustUnits = -1;
	bool bRustResync = true;
	const FWW2Result RC = View.UnitCount(RustUnits), RR = View.NeedsResync(bRustResync);
	const int32 CppUnits = State.Units.Num() + State.UnreadableIds.Num();
	const bool bAgree = LastApply.IsOk() == bCppAccepted && RC.IsOk() && RR.IsOk() && RustUnits == CppUnits && !bRustResync;
	for (FCounts* C : { &All, &Segment }) { C->Starts++; C->StartsAgree += bAgree ? 1 : 0; }
	UE_LOG(LogTC, Log, TEXT("rust view shadow: start #%d (match %d%s): rust apply %d (%s), C++ %s; units rust %d C++ %d; rust resync %d; %s; decoder %s"), StartNumber, State.MatchId,
		bReconnect ? TEXT(", same match: reconnect") : TEXT(""), LastApply.Code, FWW2Core::CodeName(LastApply.Code), bCppAccepted ? TEXT("accepted") : TEXT("refused"),
		RustUnits, CppUnits, bRustResync ? 1 : 0, bAgree ? TEXT("AGREE") : TEXT("DISAGREE"), DecoderName(Effective));
}

void FTCRustViewShadow::Compare(int32 Tick, const TMap<int32, FTCUnit>& CppUnits, const TSet<int32>& CppUnreadable, bool bCppDrift,
	const TArray<FWW2ViewUnit>& Rows, const FWW2Result& ReadUnits, const FWW2Result& ReadResync, bool bRustResync)
{
	const bool bApplied = LastApply.IsOk();
	const bool bRead = ReadUnits.IsOk() && ReadResync.IsOk();
	// Ids: the C++ decoder's rows plus the ids it holds without a readable row (they count toward its held check).
	TSet<uint32> CppIds;
	CppIds.Reserve(CppUnits.Num() + CppUnreadable.Num());
	for (const TPair<int32, FTCUnit>& P : CppUnits) CppIds.Add((uint32)P.Key);
	for (int32 Id : CppUnreadable) CppIds.Add((uint32)Id);
	int32 OnlyRust = 0, RowsAgree = 0, RowsDiffer = 0, RowsNarrowed = 0;
	FString FirstDiff;
	for (const FWW2ViewUnit& R : Rows)
	{
		if (Narrowed(R)) RowsNarrowed++;
		const FTCUnit* C = R.Id <= (uint32)MAX_int32 ? CppUnits.Find((int32)R.Id) : nullptr;
		if (!CppIds.Contains(R.Id)) { OnlyRust++; continue; }
		const FString D = C ? Differs(R, *C) : FString(TEXT("C++ holds the id without a readable row"));
		if (D.IsEmpty()) RowsAgree++;
		else { RowsDiffer++; if (FirstDiff.IsEmpty()) FirstDiff = FString::Printf(TEXT("unit %u: %s"), R.Id, *D); }
	}
	const int32 OnlyCpp = CppIds.Num() - (Rows.Num() - OnlyRust);
	const bool bIdsAgree = OnlyRust == 0 && OnlyCpp == 0;
	const bool bResyncAgree = bRustResync == bCppDrift;
	const bool bAgree = bApplied && bRead && bIdsAgree && RowsDiffer == 0 && bResyncAgree;
	for (FCounts* C : { &All, &Segment })
	{
		C->Snapshots++;
		(bAgree ? C->Agree : C->Disagree)++;
		C->RustRejected += bApplied ? 0 : 1;
		C->IdSetDiffer += bIdsAgree ? 0 : 1;
		C->RowsDiffer += RowsDiffer ? 1 : 0;
		C->ResyncDiffer += bResyncAgree ? 0 : 1;
		C->RowsCompared += Rows.Num() - OnlyRust;
		C->RowsAgree += RowsAgree;
		C->RowsNarrowed += RowsNarrowed;
	}
	// One line per snapshot only at Verbose (a view runs by default); the totals come with the status line and at
	// the end, and every disagreement is a warning (the first 20).
	UE_LOG(LogTC, Verbose, TEXT("rust view shadow: snapshot %d tick %d: %s; rows rust %d C++ %d, agree %d, differ %d, only rust %d, only C++ %d, narrowed by C++ %d; resync rust %d C++ %d; totals %d of %d agree"),
		All.Snapshots, Tick, bAgree ? TEXT("AGREE") : TEXT("DISAGREE"), Rows.Num(), CppIds.Num(), RowsAgree, RowsDiffer, OnlyRust, OnlyCpp, RowsNarrowed,
		bRustResync ? 1 : 0, bCppDrift ? 1 : 0, All.Agree, All.Snapshots);
	if (!bAgree && WarnedSnapshots++ < 20)
	{
		UE_LOG(LogTC, Warning, TEXT("rust view shadow: snapshot %d tick %d disagrees (decoder %s): apply %d %s%s; reads %d/%d %s%s; rows rust %d C++ %d, only rust %d, only C++ %d; resync rust %d C++ %d; %s"),
			All.Snapshots, Tick, DecoderName(Effective), LastApply.Code, FWW2Core::CodeName(LastApply.Code),
			bApplied ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *LastApply.Error), ReadUnits.Code, ReadResync.Code, *ReadUnits.Error, *ReadResync.Error,
			Rows.Num(), CppIds.Num(), OnlyRust, OnlyCpp, bRustResync ? 1 : 0, bCppDrift ? 1 : 0, FirstDiff.IsEmpty() ? TEXT("no row differs") : *FirstDiff);
	}
}

void FTCRustViewShadow::AfterSnapshot(const FTCMatchState& State, bool bDrift)
{
	if (!bActive) return;
	if (bComparedInState) { bComparedInState = false; return; } // rust: compared inside ApplySnapshot, before adoption
	bPending = false;
	if (!State.bHasMatch)
	{
		for (FCounts* C : { &All, &Segment }) C->CppIgnored++;
		UE_LOG(LogTC, Verbose, TEXT("rust view shadow: snapshot with no C++ match held (C++ ignores it; rust apply %d %s): not compared"), LastApply.Code, FWW2Core::CodeName(LastApply.Code));
		return;
	}
	TArray<FWW2ViewUnit> Rows;
	bool bRustResync = false;
	const FWW2Result RU = View.Units(Rows), RR = View.NeedsResync(bRustResync);
	Compare(State.Tick, State.Units, State.UnreadableIds, bDrift, Rows, RU, RR, bRustResync);
}

ETCUnitAdoption FTCRustViewShadow::OnStateSnapshot(int32 Tick, const TMap<int32, FTCUnit>& CppUnits, const TSet<int32>& CppUnreadable, bool bCppDrift,
	TMap<int32, FTCUnit>& OutRows, bool& bOutResync)
{
	bOutResync = bCppDrift;
	if (!IsAuthoritative()) return ETCUnitAdoption::Kept;
	const bool bFed = bPending;
	bPending = false;
	if (!bFed)
	{
		for (FCounts* C : { &All, &Segment }) C->NotFed++;
		if (WarnedNotFed++ < 5) UE_LOG(LogTC, Warning, TEXT("decoder: rust: snapshot tick %d reached the match state without reaching the view first: the C++ rows this once, not compared"), Tick);
		bComparedInState = true; // the view did not see this message: nothing to compare it with
		return ETCUnitAdoption::NotFed;
	}
	TArray<FWW2ViewUnit> Rows;
	bool bRustResync = false;
	const FWW2Result RU = View.Units(Rows), RR = View.NeedsResync(bRustResync);
	Compare(Tick, CppUnits, CppUnreadable, bCppDrift, Rows, RU, RR, bRustResync);
	bComparedInState = true;
	if (!LastApply.IsOk() || !RU.IsOk() || !RR.IsOk())
	{
		for (FCounts* C : { &All, &Segment }) C->Refused++;
		RefusedRun++;
		bOutResync = true;
		const FWW2Result& Why = !LastApply.IsOk() ? LastApply : !RU.IsOk() ? RU : RR;
		UE_LOG(LogTC, Warning, TEXT("decoder: rust: the view %s snapshot tick %d (%d %s: %s): the C++ rows this once, and a full resend is asked (%d in a row)"),
			LastApply.IsOk() ? TEXT("could not be read after") : TEXT("refused"), Tick, Why.Code, FWW2Core::CodeName(Why.Code), *Why.Error, RefusedRun);
		if (RefusedRun >= RefusalsBeforeDemotion)
			Demote(ETCDecoder::Shadow, FString::Printf(TEXT("the view refused %d snapshots in a row (last at tick %d)"), RefusedRun, Tick));
		return ETCUnitAdoption::Refused;
	}
	RefusedRun = 0;

	// The C++ decoder's iteration order for the ids both hold (so code that walks Units sees the same order in every
	// mode when the decoders agree), then the ids only the view holds, ascending (the view's order).
	TBitArray<> Taken(false, Rows.Num());
	OutRows.Reset();
	OutRows.Reserve(Rows.Num());
	int64 Tails = 0, Dropped = 0;
	auto Take = [&](int32 Index, const FTCUnit* Cpp)
	{
		Taken[Index] = true;
		FTCUnit U;
		ToUnit(Rows[Index], U);
		if (Cpp && Cpp->Tail.Num()) { U.Tail = Cpp->Tail; Tails++; }
		OutRows.Add(U.Id, MoveTemp(U));
	};
	for (const TPair<int32, FTCUnit>& P : CppUnits)
	{
		if (P.Key < 0) continue;
		const int32 Index = Algo::BinarySearchBy(Rows, (uint32)P.Key, [](const FWW2ViewUnit& R) { return R.Id; });
		if (Index != INDEX_NONE) Take(Index, &P.Value);
	}
	for (int32 I = 0; I < Rows.Num(); I++)
	{
		if (Taken[I]) continue;
		if (Rows[I].Id > (uint32)MAX_int32) { Dropped++; continue; }
		Take(I, CppUnits.Find((int32)Rows[I].Id));
	}
	for (FCounts* C : { &All, &Segment })
	{
		C->Adopted++;
		C->RowsAdopted += OutRows.Num();
		C->TailsFromCpp += Tails;
		C->Unrepresentable += Dropped;
		C->RustResync += bRustResync ? 1 : 0;
	}
	if (Dropped) UE_LOG(LogTC, Warning, TEXT("decoder: rust: snapshot tick %d: %lld unit ids above 2147483647 cannot be drawn (FTCUnit ids are int32)"), Tick, (long long)Dropped);
	bOutResync = bRustResync;
	return ETCUnitAdoption::Adopted;
}

void FTCRustViewShadow::LogTotals(const TCHAR* Why) const
{
	if (!bActive && All.Snapshots == 0) return;
	UE_LOG(LogTC, Log, TEXT("rust view shadow totals (%s): messages %d, apply errors %d; %s"), Why, Messages, ApplyErrors, *CountsText(All));
	UE_LOG(LogTC, Log, TEXT("rust view shadow: segment %d (%s) so far: %s"), SegmentNumber, *SegmentLabel, *CountsText(Segment));
	UE_LOG(LogTC, Log, TEXT("rust view decoder (%s): requested %s, in effect %s%s; %s"), Why, DecoderName(Requested_), bActive ? DecoderName(Effective) : TEXT("cpp (view off)"),
		Demoted.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (demoted: %s)"), *Demoted), *AdoptionText(All));
}

void FTCRustViewShadow::End()
{
	if (bActive || All.Snapshots > 0) LogTotals(TEXT("end"));
	Detach();
	View.Close();
	bActive = false;
	bPending = bComparedInState = false;
	Effective = ETCDecoder::Cpp;
}

// ---------------------------------------------------------------------------------------------------------------------
// Tests: UnrealEditor-Cmd ThreeCrossroads.uproject -nullrhi -ExecCmds="Automation RunTests ThreeCrossroads.Net; Quit"
// Each drives FTCMatchState and FTCRustViewShadow as the game mode does with a received message (OnText feeds the
// view, then OnMessage applies it to the state and calls AfterStart or AfterSnapshot). Built without the Rust core
// they only check that no view runs.

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCDecoderTest
{
	constexpr EAutomationTestFlags Flags = EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter;

	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> M;
		TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);
		return FJsonSerializer::Deserialize(Reader, M) ? M : nullptr;
	}

	// One client's decoder half. The state is declared first, so the view (which detaches from it) goes first.
	struct FClient
	{
		FTCMatchState State;
		FTCRustViewShadow Rust;
		int32 Starts = 0;
		bool Begin(ETCDecoder Mode) { return Rust.BeginMode(Mode, &State, true, false); }
		// The game mode's path for one message; returns ApplySnapshot's verdict (false for anything else).
		bool Receive(const FString& Text, bool bFeed = true)
		{
			if (bFeed && Rust.IsActive()) Rust.Feed(Text);
			const TSharedPtr<FJsonObject> M = Parse(Text);
			if (!M) return false;
			FString T;
			M->TryGetStringField(TEXT("t"), T);
			if (T == TEXT("lobby")) State.ApplyLobby(*M);
			else if (T == TEXT("pause")) State.ApplyPause(*M);
			else if (T == TEXT("deny")) State.ApplyDeny(*M);
			else if (T == TEXT("start")) { const bool bOk = State.ApplyStart(*M); Rust.AfterStart(State, bOk, false, ++Starts); }
			else if (T == TEXT("s")) { const bool bDrift = State.ApplySnapshot(*M); Rust.AfterSnapshot(State, bDrift); return bDrift; }
			return false;
		}
	};

	bool SameUnit(const FTCUnit& A, const FTCUnit& B)
	{
		return A.Id == B.Id && A.Type == B.Type && A.Owner == B.Owner && A.X == B.X && A.Z == B.Z && A.Rot == B.Rot && A.Aim == B.Aim && A.Hp == B.Hp
			&& A.Supp == B.Supp && A.Target == B.Target && A.Cover == B.Cover && A.Cd == B.Cd && A.Flags == B.Flags && A.Stars == B.Stars && A.Built == B.Built
			&& A.Tail == B.Tail;
	}

	// Units equal, in the same iteration order; empty when they are.
	FString UnitsDiffer(const TMap<int32, FTCUnit>& A, const TMap<int32, FTCUnit>& B)
	{
		TArray<int32> KA, KB;
		A.GenerateKeyArray(KA);
		B.GenerateKeyArray(KB);
		if (KA != KB) return FString::Printf(TEXT("ids or their order differ (%d vs %d units)"), KA.Num(), KB.Num());
		for (const TPair<int32, FTCUnit>& P : A) if (!SameUnit(P.Value, B[P.Key])) return FString::Printf(TEXT("unit %d differs"), P.Key);
		return FString();
	}

	// A wire unit row: the 15 fixed columns, then the tail.
	FString Row(int64 Id, const TCHAR* Kind, int32 Owner, double Hp, const TCHAR* Tail = TEXT(""))
	{
		return FString::Printf(TEXT("[%lld,\"%s\",%d,10.25,20.5,0.5,0.25,%s,3,0,0,1.5,0,1,1%s]"), Id, Kind, Owner, *FString::SanitizeFloat(Hp), Tail);
	}

	const TCHAR* Start = TEXT("{\"t\":\"start\",\"matchId\":7,\"you\":0,\"names\":[\"A\",\"B\"],\"teams\":[0,1],\"map\":{\"name\":\"t\",\"w\":4,\"h\":4,\"rows\":[\"....\",\"....\",\"....\",\"....\"]}}");
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCDecoderModesTest, "ThreeCrossroads.Net.DecoderModes", TCDecoderTest::Flags)

bool FTCDecoderModesTest::RunTest(const FString& Parameters)
{
	struct FCase { const TCHAR* Decoder; const TCHAR* Legacy; ETCDecoder Want; bool bExplicit; };
	const FCase Cases[] = {
		{ TEXT(""), TEXT(""), ETCDecoder::Rust, false },          // the default is rust (slice 12)
		{ TEXT("rust"), TEXT(""), ETCDecoder::Rust, true },
		{ TEXT(" Rust "), TEXT(""), ETCDecoder::Rust, true },
		{ TEXT("cpp"), TEXT(""), ETCDecoder::Cpp, true },
		{ TEXT("shadow"), TEXT(""), ETCDecoder::Shadow, true },
		{ TEXT("bogus"), TEXT(""), ETCDecoder::Shadow, true },
		{ TEXT(""), TEXT("1"), ETCDecoder::Shadow, true },        // the old switch
		{ TEXT(""), TEXT("0"), ETCDecoder::Cpp, true },
		{ TEXT(""), TEXT("yes"), ETCDecoder::Rust, false },
		{ TEXT("rust"), TEXT("0"), ETCDecoder::Rust, true },      // TC_DECODER wins
		{ TEXT("cpp"), TEXT("1"), ETCDecoder::Cpp, true },
	};
	for (const FCase& C : Cases)
	{
		FString Note;
		bool bExplicit = !C.bExplicit;
		const ETCDecoder Got = FTCRustViewShadow::ParseDecoder(C.Decoder, C.Legacy, Note, bExplicit);
		TestEqual(FString::Printf(TEXT("TC_DECODER='%s' TC_RUST_VIEW='%s' (%s)"), C.Decoder, C.Legacy, *Note), FString(FTCRustViewShadow::DecoderName(Got)), FString(FTCRustViewShadow::DecoderName(C.Want)));
		TestEqual(FString::Printf(TEXT("explicit: TC_DECODER='%s' TC_RUST_VIEW='%s'"), C.Decoder, C.Legacy), bExplicit, C.bExplicit);
	}

	// cpp: no view, nothing attached; the state decodes alone.
	TCDecoderTest::FClient Cpp;
	TestFalse(TEXT("cpp: Begin says no view"), Cpp.Begin(ETCDecoder::Cpp));
	TestFalse(TEXT("cpp: not active"), Cpp.Rust.IsActive());
	TestTrue(TEXT("cpp: mode cpp"), Cpp.Rust.Mode() == ETCDecoder::Cpp);
	TestNull(TEXT("cpp: no unit authority"), Cpp.State.GetUnitAuthority());
	if (!FWW2Core::IsReady())
	{
		TCDecoderTest::FClient R;
		TestFalse(TEXT("not linked: rust asked, no view"), R.Begin(ETCDecoder::Rust));
		TestNull(TEXT("not linked: no unit authority"), R.State.GetUnitAuthority());
		AddInfo(FString(TEXT("not linked: ")) + FWW2Core::Describe());
		return true;
	}

	// shadow: a view, nothing attached.
	TCDecoderTest::FClient Shadow;
	TestTrue(TEXT("shadow: Begin opens a view"), Shadow.Begin(ETCDecoder::Shadow));
	TestTrue(TEXT("shadow: mode shadow"), Shadow.Rust.Mode() == ETCDecoder::Shadow && !Shadow.Rust.IsAuthoritative());
	TestNull(TEXT("shadow: no unit authority"), Shadow.State.GetUnitAuthority());

	// rust: attached; End detaches.
	TCDecoderTest::FClient Rust;
	TestTrue(TEXT("rust: Begin opens a view"), Rust.Begin(ETCDecoder::Rust));
	TestTrue(TEXT("rust: authoritative"), Rust.Rust.IsAuthoritative());
	TestTrue(TEXT("rust: the state's unit authority"), Rust.State.GetUnitAuthority() == &Rust.Rust);
	Rust.Rust.End();
	TestNull(TEXT("rust: End detaches"), Rust.State.GetUnitAuthority());

	// rust without a state (the old Begin()): runs as shadow and says so.
	AddExpectedMessagePlain(TEXT("decoder: rust without a match state"), ELogVerbosity::Error, EAutomationExpectedMessageFlags::Contains, 1);
	FTCRustViewShadow Alone;
	TestTrue(TEXT("rust without a state: a view"), Alone.BeginMode(ETCDecoder::Rust, nullptr, true, false));
	TestTrue(TEXT("rust without a state: shadow"), Alone.Mode() == ETCDecoder::Shadow && !Alone.IsAuthoritative());
	Alone.End();

	// The destructor detaches too (the game mode's view member goes before its state member).
	FTCMatchState Outlives;
	{
		FTCRustViewShadow Scoped;
		Scoped.BeginMode(ETCDecoder::Rust, &Outlives, true, false);
		TestTrue(TEXT("scoped: attached"), Outlives.GetUnitAuthority() == &Scoped);
	}
	TestNull(TEXT("scoped: the destructor detached"), Outlives.GetUnitAuthority());
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCDecoderReplayTest, "ThreeCrossroads.Net.DecoderReplay", TCDecoderTest::Flags)

// The four recorded streams through three clients at once: cpp, shadow and rust. Shadow must never change what the C++
// decoder holds; rust must hold the Rust view's rows (ids equal to the browser oracle's, resync equal to its verdict)
// and still end up with exactly the client the C++ decoder makes (every FTCUnit field and the iteration order), with
// the map, fog and every other snapshot field from the C++ decoder.
bool FTCDecoderReplayTest::RunTest(const FString& Parameters)
{
	using namespace TCDecoderTest;
	if (!FWW2Core::IsReady()) { AddInfo(FString(TEXT("not linked, no replay: ")) + FWW2Core::Describe()); return true; }
	const FString Dir = FWW2ViewReplay::ProtocolDir();
	int64 RowsAdopted = 0;
	int32 Snapshots = 0;
	for (const FString& Stem : FWW2ViewReplay::Streams())
	{
		TArray<FString> Lines, Oracle;
		if (!TestTrue(FString::Printf(TEXT("%s: read"), *Stem), FFileHelper::LoadFileToStringArray(Lines, *(Dir / (Stem + TEXT(".jsonl"))))
			&& FFileHelper::LoadFileToStringArray(Oracle, *(Dir / (Stem + TEXT(".expected.jsonl")))) && Lines.Num() == Oracle.Num())) continue;
		FClient Cpp, Shadow, Rust;
		Cpp.Begin(ETCDecoder::Cpp);
		if (!TestTrue(TEXT("shadow begins"), Shadow.Begin(ETCDecoder::Shadow)) || !TestTrue(TEXT("rust begins"), Rust.Begin(ETCDecoder::Rust))) return false;
		int32 Compared = 0, IdsOff = 0, ResyncOff = 0, CppResyncOff = 0, ShadowChanged = 0, RustOff = 0, NotFromRust = 0, MapOff = 0, ExtrasOff = 0;
		FString First;
		auto Note = [&First](const FString& S) { if (First.IsEmpty()) First = S; };
		for (int32 L = 0; L < Lines.Num(); L++)
		{
			const bool bC = Cpp.Receive(Lines[L]), bS = Shadow.Receive(Lines[L]), bR = Rust.Receive(Lines[L]);
			const TSharedPtr<FJsonObject> M = Parse(Lines[L]), W = Parse(Oracle[L]);
			FString T;
			if (!M || !W || !M->TryGetStringField(TEXT("t"), T) || T != TEXT("s") || !Rust.State.bHasMatch) continue;
			Compared++;
			bool bWant = false;
			W->TryGetBoolField(TEXT("resync"), bWant);
			TArray<int32> WantIds;
			for (const TSharedPtr<FJsonValue>& V : W->GetArrayField(TEXT("ids"))) WantIds.Add((int32)V->AsNumber());
			TArray<int32> Ids;
			Rust.State.Units.GenerateKeyArray(Ids);
			Ids.Sort();
			if (!Rust.State.UnitsFromRustView()) { NotFromRust++; Note(FString::Printf(TEXT("line %d: rows not from the view"), L + 1)); }
			if (Ids != WantIds) { IdsOff++; Note(FString::Printf(TEXT("line %d: rust ids differ from the oracle's"), L + 1)); }
			if (bR != bWant) { ResyncOff++; Note(FString::Printf(TEXT("line %d: rust verdict %d, oracle %d"), L + 1, bR, bWant)); }
			if (bC != bWant) CppResyncOff++;
			if (bS != bC || !UnitsDiffer(Shadow.State.Units, Cpp.State.Units).IsEmpty()) { ShadowChanged++; Note(FString::Printf(TEXT("line %d: shadow changed the C++ client"), L + 1)); }
			const FString D = UnitsDiffer(Rust.State.Units, Cpp.State.Units);
			if (!D.IsEmpty()) { RustOff++; Note(FString::Printf(TEXT("line %d: rust client vs cpp client: %s"), L + 1, *D)); }
			if (Rust.State.FogVisible != Cpp.State.FogVisible || Rust.State.FogExplored != Cpp.State.FogExplored || Rust.State.Map.Chars != Cpp.State.Map.Chars
				|| Rust.State.Map.Levels != Cpp.State.Map.Levels || Rust.State.Map.Known != Cpp.State.Map.Known) { MapOff++; Note(FString::Printf(TEXT("line %d: map or fog differ"), L + 1)); }
			if (Rust.State.DescribeExtras() != Cpp.State.DescribeExtras() || Rust.State.Plans.Num() != Cpp.State.Plans.Num() || Rust.State.WaitingOrders.Num() != Cpp.State.WaitingOrders.Num()
				|| Rust.State.Air.Num() != Cpp.State.Air.Num() || Rust.State.Ghosts.Num() != Cpp.State.Ghosts.Num()) { ExtrasOff++; Note(FString::Printf(TEXT("line %d: snapshot fields differ"), L + 1)); }
		}
		const FTCRustViewShadow::FCounts& S = Shadow.Rust.Totals();
		const FTCRustViewShadow::FCounts& R = Rust.Rust.Totals();
		AddInfo(FString::Printf(TEXT("%s: %d messages, %d snapshots compared; shadow agree %d of %d; rust adopted %d (rows %lld), refused %d, not fed %d, agree %d of %d, resync %d"),
			*Stem, Lines.Num(), Compared, S.Agree, S.Snapshots, R.Adopted, (long long)R.RowsAdopted, R.Refused, R.NotFed, R.Agree, R.Snapshots, R.RustResync));
		if (!First.IsEmpty()) AddError(FString::Printf(TEXT("%s: %s"), *Stem, *First));
		TestTrue(FString::Printf(TEXT("%s: snapshots compared"), *Stem), Compared > 0);
		TestEqual(FString::Printf(TEXT("%s: rust rows not from the view"), *Stem), NotFromRust, 0);
		TestEqual(FString::Printf(TEXT("%s: rust ids vs the oracle"), *Stem), IdsOff, 0);
		TestEqual(FString::Printf(TEXT("%s: rust resync verdict vs the oracle"), *Stem), ResyncOff, 0);
		TestEqual(FString::Printf(TEXT("%s: cpp resync verdict vs the oracle"), *Stem), CppResyncOff, 0);
		TestEqual(FString::Printf(TEXT("%s: shadow changed what the C++ decoder holds"), *Stem), ShadowChanged, 0);
		TestEqual(FString::Printf(TEXT("%s: rust client vs cpp client (every field, order)"), *Stem), RustOff, 0);
		TestEqual(FString::Printf(TEXT("%s: map and fog from the C++ decoder"), *Stem), MapOff, 0);
		TestEqual(FString::Printf(TEXT("%s: other snapshot fields from the C++ decoder"), *Stem), ExtrasOff, 0);
		TestEqual(FString::Printf(TEXT("%s: shadow disagreements"), *Stem), S.Disagree, 0);
		TestEqual(FString::Printf(TEXT("%s: shadow compared every snapshot"), *Stem), S.Snapshots, Compared);
		TestEqual(FString::Printf(TEXT("%s: rust adopted every snapshot"), *Stem), R.Adopted, Compared);
		TestEqual(FString::Printf(TEXT("%s: rust compared every snapshot"), *Stem), R.Snapshots, Compared);
		TestEqual(FString::Printf(TEXT("%s: rust disagreements"), *Stem), R.Disagree, 0);
		TestEqual(FString::Printf(TEXT("%s: rust refused or not fed"), *Stem), R.Refused + R.NotFed, 0);
		TestEqual(FString::Printf(TEXT("%s: apply errors (shadow, rust)"), *Stem), Shadow.Rust.ApplyErrorCount() + Rust.Rust.ApplyErrorCount(), 0);
		TestTrue(FString::Printf(TEXT("%s: still authoritative"), *Stem), Rust.Rust.IsAuthoritative());
		RowsAdopted += R.RowsAdopted;
		Snapshots += Compared;
	}
	AddInfo(FString::Printf(TEXT("all streams: %d snapshots, %lld rows adopted from the Rust view"), Snapshots, (long long)RowsAdopted));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCDecoderRustRecoveryTest, "ThreeCrossroads.Net.DecoderRustRecovery", TCDecoderTest::Flags)

// Rust mode on hand-made messages: tails stay the C++ decoder's, the view's verdict is the one that asks for a resync,
// a refused snapshot falls back once and asks for a resync, three in a row demote to shadow, a snapshot that never
// reached the view is the C++ decoder's, a reconnect start resets both, and a poisoned view leaves the C++ decoder alone.
bool FTCDecoderRustRecoveryTest::RunTest(const FString& Parameters)
{
	using namespace TCDecoderTest;
	if (!FWW2Core::IsReady()) { AddInfo(FString(TEXT("not linked: ")) + FWW2Core::Describe()); return true; }
	AddExpectedMessagePlain(TEXT("decoder: rust:"), ELogVerbosity::Warning, EAutomationExpectedMessageFlags::Contains, 0);
	AddExpectedMessagePlain(TEXT("rust view shadow: ww2_view_apply"), ELogVerbosity::Warning, EAutomationExpectedMessageFlags::Contains, -1);
	AddExpectedMessagePlain(TEXT("rust view shadow: snapshot"), ELogVerbosity::Warning, EAutomationExpectedMessageFlags::Contains, -1);
	FClient C;
	if (!TestTrue(TEXT("rust begins"), C.Begin(ETCDecoder::Rust))) return false;
	C.Receive(Start);
	TestTrue(TEXT("start: a match"), C.State.bHasMatch);

	// Tails: unit 1 carries a vehicle tail. The view has no tails; the C++ decoder's stays, by id, across deltas.
	bool bDrift = C.Receive(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":1,\"units\":[%s,%s],\"held\":[2,3]}"), *Row(1, TEXT("tank"), 0, 87.5, TEXT(",3.5,1,0,0.5")), *Row(2, TEXT("rifle"), 1, 100)));
	TestFalse(TEXT("s1: no resync"), bDrift);
	TestTrue(TEXT("s1: rows from the view"), C.State.UnitsFromRustView());
	const FTCUnit* Tank = C.State.Units.Find(1);
	if (TestNotNull(TEXT("s1: unit 1"), Tank))
	{
		TestTrue(TEXT("s1: unit 1's tail from the C++ decoder"), Tank->Tail == TArray<float>({ 3.5f, 1.f, 0.f, 0.5f }));
		TestEqual(TEXT("s1: hp 87.5 narrowed as the C++ decoder narrows"), Tank->Hp, 87);
		TestEqual(TEXT("s1: kind"), Tank->Type, FString(TEXT("tank")));
		TestEqual(TEXT("s1: x"), Tank->X, 10.25f);
	}
	bDrift = C.Receive(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":2,\"units\":[%s],\"held\":[2,3]}"), *Row(2, TEXT("rifle"), 1, 50)));
	TestFalse(TEXT("s2: no resync"), bDrift);
	TestTrue(TEXT("s2: unit 1 keeps its tail without a new row"), C.State.Units.Contains(1) && C.State.Units[1].Tail.Num() == 4);
	TestTrue(TEXT("s2: unit 2's hp from the delta"), C.State.Units.Contains(2) && C.State.Units[2].Hp == 50);

	// The view's verdict decides. Id 2^31 fits the view (uint32) but not FTCUnit; held [1, -2^31] is the view's own
	// xor, so the view holds what the server says, while the C++ decoder (its id narrowed to 2^31-1) does not.
	bDrift = C.Receive(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":3,\"all\":true,\"units\":[%s],\"held\":[1,-2147483648]}"), *Row(2147483648ll, TEXT("mg"), 1, 10)));
	TestFalse(TEXT("s3: the view's verdict (no resync) wins over the C++ decoder's"), bDrift);
	TestEqual(TEXT("s3: the id above int32 is not drawn"), C.State.Units.Num(), 0);
	TestEqual(TEXT("s3: counted"), (int32)C.Rust.Totals().Unrepresentable, 1);
	TestEqual(TEXT("s3: the comparison saw the decoders disagree"), C.Rust.Totals().Disagree, 1);
	bDrift = C.Receive(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":4,\"all\":true,\"units\":[%s,%s],\"held\":[2,3]}"), *Row(1, TEXT("tank"), 0, 80), *Row(2, TEXT("rifle"), 1, 50)));
	TestFalse(TEXT("s4: a full resend realigns"), bDrift);
	TestTrue(TEXT("s4: from the view"), C.State.UnitsFromRustView() && C.State.Units.Num() == 2);

	// A held mismatch both decoders see: the view asks for the resync.
	bDrift = C.Receive(TEXT("{\"t\":\"s\",\"tick\":5,\"units\":[],\"held\":[5,0]}"));
	TestTrue(TEXT("s5: held mismatch asks for a resync"), bDrift);
	TestTrue(TEXT("s5: still the view's rows"), C.State.UnitsFromRustView());
	TestEqual(TEXT("s5: the view's resync counted"), C.Rust.Totals().RustResync, 1);

	// Refused: a held xor of 2^31 does not fit the view's int32 (WW2_ERR_PARSE); the C++ decoder clamps and goes on.
	const FString Refused = TEXT("{\"t\":\"s\",\"tick\":6,\"units\":[],\"held\":[2,2147483648]}");
	bDrift = C.Receive(Refused);
	TestTrue(TEXT("refused: a resync is asked"), bDrift);
	TestFalse(TEXT("refused: the C++ rows this once"), C.State.UnitsFromRustView());
	TestEqual(TEXT("refused: the C++ rows are its own"), C.State.Units.Num(), 2);
	TestEqual(TEXT("refused: counted"), C.Rust.Totals().Refused, 1);
	bDrift = C.Receive(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":7,\"all\":true,\"units\":[%s,%s],\"held\":[2,3]}"), *Row(1, TEXT("tank"), 0, 80), *Row(2, TEXT("rifle"), 1, 50)));
	TestFalse(TEXT("after a refusal: the full resend realigns"), bDrift);
	TestTrue(TEXT("after a refusal: the view's rows again"), C.State.UnitsFromRustView() && C.Rust.IsAuthoritative());

	// Not fed: a snapshot applied without the socket keeps the C++ rows and verdict, and is not compared.
	const int32 ComparedBefore = C.Rust.Totals().Snapshots;
	bDrift = C.Receive(TEXT("{\"t\":\"s\",\"tick\":8,\"units\":[],\"held\":[2,3]}"), false);
	TestFalse(TEXT("not fed: the C++ verdict"), bDrift);
	TestFalse(TEXT("not fed: the C++ rows"), C.State.UnitsFromRustView());
	TestEqual(TEXT("not fed: counted"), C.Rust.Totals().NotFed, 1);
	TestEqual(TEXT("not fed: not compared"), C.Rust.Totals().Snapshots, ComparedBefore);

	// Reconnect: a start for the same match resets both; the next snapshot is the view's again.
	C.Receive(Start);
	TestEqual(TEXT("reconnect: no units"), C.State.Units.Num(), 0);
	bDrift = C.Receive(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":9,\"units\":[%s],\"held\":[1,2]}"), *Row(2, TEXT("rifle"), 1, 40)));
	TestFalse(TEXT("reconnect: no resync"), bDrift);
	TestTrue(TEXT("reconnect: the view's rows"), C.State.UnitsFromRustView() && C.State.Units.Num() == 1);

	// Three refusals in a row demote the session to shadow: the C++ rows drive, the view is still compared.
	AddExpectedMessagePlain(TEXT("decoder: rust demoted to shadow"), ELogVerbosity::Error, EAutomationExpectedMessageFlags::Contains, 1);
	for (int32 I = 0; I < FTCRustViewShadow::RefusalsBeforeDemotion; I++) TestTrue(FString::Printf(TEXT("refusal %d asks for a resync"), I + 1), C.Receive(Refused));
	TestTrue(TEXT("demoted: shadow"), C.Rust.Mode() == ETCDecoder::Shadow && !C.Rust.IsAuthoritative() && C.Rust.IsActive());
	TestNull(TEXT("demoted: detached"), C.State.GetUnitAuthority());
	TestFalse(TEXT("demoted: why is kept"), C.Rust.DemotedWhy().IsEmpty());
	const int32 BeforeShadow = C.Rust.Totals().Snapshots, DisagreeBefore = C.Rust.Totals().Disagree;
	bDrift = C.Receive(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":10,\"all\":true,\"units\":[%s],\"held\":[1,2]}"), *Row(2, TEXT("rifle"), 1, 40)));
	TestFalse(TEXT("demoted: the C++ verdict"), bDrift);
	TestFalse(TEXT("demoted: the C++ rows"), C.State.UnitsFromRustView());
	TestEqual(TEXT("demoted: still compared, as shadow"), C.Rust.Totals().Snapshots, BeforeShadow + 1);
	TestEqual(TEXT("demoted: that comparison agrees"), C.Rust.Totals().Disagree, DisagreeBefore);

	// Poisoned: the view is closed and the C++ decoder drives alone.
	FClient P;
	if (!TestTrue(TEXT("poison: rust begins"), P.Begin(ETCDecoder::Rust))) return false;
	P.Receive(Start);
	P.Receive(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":1,\"units\":[%s],\"held\":[1,2]}"), *Row(2, TEXT("rifle"), 1, 40)));
	TestTrue(TEXT("poison: the view's rows first"), P.State.UnitsFromRustView());
	AddExpectedMessagePlain(TEXT("ww2 view poisoned (-1 WW2_ERR_PANIC)"), ELogVerbosity::Error, EAutomationExpectedMessageFlags::Contains, 1);
	AddExpectedMessagePlain(TEXT("rust view shadow: the view is poisoned"), ELogVerbosity::Error, EAutomationExpectedMessageFlags::Contains, 1);
	AddExpectedMessagePlain(TEXT("decoder: rust demoted to cpp"), ELogVerbosity::Error, EAutomationExpectedMessageFlags::Contains, 1);
	P.Rust.ViewForTest().NoteLibraryCodeForTest(FWW2Core::ErrPanic);
	bDrift = P.Receive(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":2,\"units\":[%s],\"held\":[1,2]}"), *Row(2, TEXT("rifle"), 1, 30)));
	TestFalse(TEXT("poison: the C++ verdict"), bDrift);
	TestFalse(TEXT("poison: the C++ rows"), P.State.UnitsFromRustView());
	TestTrue(TEXT("poison: hp from the C++ decoder"), P.State.Units.Contains(2) && P.State.Units[2].Hp == 30);
	TestTrue(TEXT("poison: no view, cpp"), !P.Rust.IsActive() && P.Rust.Mode() == ETCDecoder::Cpp);
	TestNull(TEXT("poison: detached"), P.State.GetUnitAuthority());
	return true;
}

#endif // WITH_DEV_AUTOMATION_TESTS
