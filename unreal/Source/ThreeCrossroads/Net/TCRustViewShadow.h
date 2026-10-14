// Staged adoption of the Rust recipient view (WW2Core FWW2View, ww2_view_* ABI 2), chosen with TC_DECODER in the
// environment (the field-ownership table is in DESIGN.md, "Unreal client: the Rust view as the unit source"):
//
//   TC_DECODER=cpp     the C++ decoder (FTCMatchState) alone; no Rust view is opened.
//   TC_DECODER=shadow  a fallback. Every server message the socket receives also goes, unchanged, to a Rust view
//                      running beside the C++ decoder. The C++ decoder drives the game, the presentation and the resync
//                      requests; after every start and snapshot the two are compared and counted.
//   TC_DECODER=rust    the default since slice 12 (lead decision after clean live rust matches: start, movement,
//                      combat, reconnect and match end, 0 refusals and 0 disagreements). The Rust view's unit rows drive the game and the presentation (FTCMatchState::Units) and its
//                      held-set verdict (ww2_view_needs_resync) decides the resync request. The C++ decoder still
//                      decodes every message: it supplies what ABI 2 does not expose (row tails, matched by id; fog;
//                      cells and the map; every other start, snapshot and lobby field) and keeps its own complete row
//                      set, which is compared with the view's rows on every snapshot as in shadow mode.
//   (unset)            rust. TC_RUST_VIEW=1 is the old name of shadow and TC_RUST_VIEW=0 of cpp; TC_DECODER wins. An
//                      unknown TC_DECODER value falls back to shadow and says so. Rust mode keeps its own fallbacks
//                      (a refused snapshot uses the C++ rows once; three in a row demote the session to shadow; a
//                      poisoned view leaves the C++ decoder alone; no Rust core or a failed self-check: cpp).
//
// Rust mode needs the match state (Begin(State)); Begin() without it runs a requested rust mode as shadow, and says so.
// Before the view opens, the WW2Core startup self-check runs (the recorded protocol-conquest-p0 stream when it is
// installed or staged, and the edge checks). If the bridge is not ready or the check fails, no view runs: the client
// uses the C++ decoder alone and the log says why.
//
// Resync (docs/contracts/rust-unreal-abi.md, ABI 2: "the transport sends resync when requested and controls its rate
// limit"): under rust, FTCMatchState::ApplySnapshot returns the view's needs_resync, and the game mode sends
// { t: 'resync' } at most once a second, as client/main.js applySnapshot does. When the view refuses a snapshot the C++
// decoder accepted (or its rows cannot be read), that snapshot uses the C++ rows and asks for a resync, so both decoders
// realign from the full resend; after RefusalsBeforeDemotion refusals in a row the session demotes to shadow (the C++
// rows drive, the view is still compared). A poisoned view (a caught panic) is closed and the C++ decoder drives alone.
//
// Agreement is judged at the precision the C++ decoder keeps (FTCUnit: int32 ids, float positions, int32 hp, supp,
// target, cover, cd, flags and stars, narrowed by FTCMatchState::NarrowInt), so a snapshot agrees when both hold the same
// ids, every row's kind, owner and fixed columns match after that narrowing, the Rust view accepted the message, and both
// reach the same held-set verdict. Rows where the narrowing drops wire precision are counted as information. Row tails,
// fog, cells and every other field are not in ABI 2's C interface and are not compared. The view is owned by the game
// thread.
#pragma once

#include "CoreMinimal.h"
#include "WW2Core.h"

class FTCMatchState;
struct FTCUnit;

// Which decoder's unit rows the client uses (TC_DECODER).
enum class ETCDecoder : uint8
{
	Cpp,     // the C++ decoder alone
	Shadow,  // the C++ decoder drives; the Rust view runs beside it and is compared
	Rust,    // the Rust view's rows drive; the C++ decoder supplies every field ABI 2 does not expose
};

// What the Rust view did with one snapshot's unit rows (FTCRustViewShadow::OnStateSnapshot).
enum class ETCUnitAdoption : uint8
{
	Kept,     // not the authority (cpp, shadow, demoted or off): the C++ rows and verdict stand
	NotFed,   // the authority, but no received message reached the view since the last start or snapshot (a
	          // snapshot applied without the socket, as tests do): the C++ rows and verdict stand
	Refused,  // the view refused this snapshot or could not be read: the C++ rows this once, and a resync is asked
	Adopted,  // the view's rows are the rows; its needs_resync is the verdict
};

class FTCRustViewShadow
{
public:
	FTCRustViewShadow() = default;
	~FTCRustViewShadow();
	FTCRustViewShadow(const FTCRustViewShadow&) = delete;
	FTCRustViewShadow& operator=(const FTCRustViewShadow&) = delete;

	// The decoder TC_DECODER (and the old TC_RUST_VIEW) ask for. OutNote: a line about how it was chosen.
	static ETCDecoder RequestedDecoder(FString* OutNote = nullptr, bool* bOutExplicit = nullptr);
	// The rule above, on given values (an empty string is unset).
	static ETCDecoder ParseDecoder(const FString& Decoder, const FString& LegacyRustView, FString& OutNote, bool& bOutExplicit);
	static const TCHAR* DecoderName(ETCDecoder Mode);
	// A Rust view is asked for (shadow or rust): the game mode then calls Begin and feeds it the socket's text.
	static bool Requested();

	// Runs the requested mode. Without the state a requested rust mode runs as shadow (logged).
	bool Begin();
	bool Begin(FTCMatchState& State);
	// The given mode (tests, and the two above). False, and the C++ decoder alone, when no view runs. bSelfCheck false
	// skips the startup self-check (tests that run it separately).
	bool BeginMode(ETCDecoder Requested, FTCMatchState* State, bool bExplicit = true, bool bSelfCheck = true);

	// The mode in effect: Cpp when no view runs; Shadow after a demotion.
	ETCDecoder Mode() const { return Effective; }
	ETCDecoder RequestedMode() const { return Requested_; }
	bool IsActive() const { return bActive; }
	// The view's rows are the client's unit rows.
	bool IsAuthoritative() const { return bActive && Effective == ETCDecoder::Rust && Attached != nullptr; }

	// Every text message of the socket, as received, before the C++ decoder sees it.
	void Feed(const FString& Text);
	// After the C++ decoder applied a start (bCppAccepted false: it refused the message).
	void AfterStart(const FTCMatchState& State, bool bCppAccepted, bool bReconnect, int32 StartNumber);
	// After the C++ decoder applied a snapshot; bDrift is what ApplySnapshot returned. Shadow compares here; under
	// rust the comparison already ran inside ApplySnapshot.
	void AfterSnapshot(const FTCMatchState& State, bool bDrift);

	// Called by FTCMatchState (the attached state only).
	void OnStateStart(bool bCppAccepted);
	// CppUnits / CppUnreadable / bCppDrift: the C++ decoder's own rows and verdict for this snapshot. On Adopted,
	// OutRows gets the view's rows as FTCUnits (the C++ decoder's iteration order for ids both hold, then the others by
	// ascending id; tails copied from the C++ row of the same id) and bOutResync the view's verdict. On Refused,
	// bOutResync is true. Otherwise OutRows is untouched and bOutResync is bCppDrift.
	ETCUnitAdoption OnStateSnapshot(int32 Tick, const TMap<int32, FTCUnit>& CppUnits, const TSet<int32>& CppUnreadable, bool bCppDrift,
		TMap<int32, FTCUnit>& OutRows, bool& bOutResync);
	// A snapshot the C++ decoder ignored (no match held).
	void OnStateSnapshotIgnored();

	// Logs the totals (and the current segment's) with a reason.
	void LogTotals(const TCHAR* Why) const;
	void End();

	static constexpr int32 RefusalsBeforeDemotion = 3;

	struct FCounts
	{
		int32 Snapshots = 0, Agree = 0, Disagree = 0;
		int32 RustRejected = 0;        // ww2_view_apply refused a message the C++ decoder used
		int32 CppIgnored = 0;          // snapshots the C++ decoder dropped (no match held), not compared
		int32 IdSetDiffer = 0, RowsDiffer = 0, ResyncDiffer = 0;
		int64 RowsCompared = 0, RowsAgree = 0, RowsNarrowed = 0;
		int32 Starts = 0, StartsAgree = 0;
		// rust mode
		int32 Adopted = 0, NotFed = 0, Refused = 0, RustResync = 0;
		int64 RowsAdopted = 0, TailsFromCpp = 0, Unrepresentable = 0; // ids above INT32_MAX cannot be an FTCUnit
	};
	const FCounts& Totals() const { return All; }
	int32 ApplyErrorCount() const { return ApplyErrors; }
	const FString& DemotedWhy() const { return Demoted; }

#if WITH_DEV_AUTOMATION_TESTS
	FWW2View& ViewForTest() { return View; }
#endif

private:
	void Compare(int32 Tick, const TMap<int32, FTCUnit>& CppUnits, const TSet<int32>& CppUnreadable, bool bCppDrift,
		const TArray<FWW2ViewUnit>& Rows, const FWW2Result& ReadUnits, const FWW2Result& ReadResync, bool bRustResync);
	void Demote(ETCDecoder To, const FString& Why);
	void Detach();

	FWW2View View;
	bool bActive = false;
	ETCDecoder Requested_ = ETCDecoder::Shadow, Effective = ETCDecoder::Cpp;
	FTCMatchState* Attached = nullptr;   // rust mode: the state whose rows the view supplies (not owned)
	FWW2Result LastApply;
	bool bPending = false;               // a message was fed that no start or snapshot of the attached state took yet
	bool bComparedInState = false;       // this snapshot was compared inside ApplySnapshot; AfterSnapshot skips it
	int32 RefusedRun = 0;
	FString Demoted;
	int32 Messages = 0, ApplyErrors = 0;
	FCounts All, Segment;
	int32 SegmentNumber = 0;
	FString SegmentLabel;
	int32 WarnedSnapshots = 0, WarnedNotFed = 0;
};
