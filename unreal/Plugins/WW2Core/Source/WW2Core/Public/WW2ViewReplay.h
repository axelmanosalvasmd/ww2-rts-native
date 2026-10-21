// Self-check of the recipient view (ww2_view_*, ABI 2) inside Unreal, shared by the automation tests and the startup
// check the client runs before it opens a Rust view (TC_DECODER=shadow or rust, Net/TCRustViewShadow.h).
//
// Replay: the recorded server-message streams of native rust/crates/core/tests/fixtures (harness/protocol-record.mjs,
// installed decompressed by unreal/Tools/ww2-ffi.sh) go through FWW2View::Apply exactly as recorded, one line per
// message. After every message the view's rows are compared with two references:
//  - the browser oracle's report for that message (<stream>.expected.jsonl, harness/view-check.mjs): ids in order and
//    the held-set mismatch flag (resync) are compared directly;
//  - an independent row model built here with Unreal's JSON parser from the same messages (a start resets it; a
//    snapshot with all, or without held, replaces the set; gone removes; rows upsert). The model is first anchored to
//    the oracle: its ids must equal the oracle's and the FNV-1a canonical hash of its complete raw rows (tails
//    included) must equal the oracle's units hash. Then every fixed column of every ww2_view row must equal the
//    model's raw row bit for bit (doubles by their bits, ids and flags at full uint32 width, the kind string).
// The oracle's seen, explored, cells and terrain fields are not readable through ABI 2's C interface and are not
// compared here (the Rust workspace test compares them).
//
// Edges: empty, invalid and out-of-range input, wrong state, full-width (uint32) ids, the 63-byte kind limit with no
// partial writes, held-set mismatch, reconnect reset, close and move. All through FWW2View on the calling thread.
#pragma once

#include "CoreMinimal.h"

struct WW2CORE_API FWW2ReplayReport
{
	FString Stream;
	int32 Messages = 0, ExpectedLines = 0, Starts = 0, Snapshots = 0;
	int32 ApplyErrors = 0;          // ww2_view_apply did not return WW2_OK
	int32 IdMismatches = 0;         // ww2_view ids differ from the oracle's ids
	int32 CountMismatches = 0;      // ww2_view_unit_count differs from the rows returned
	int32 ResyncMismatches = 0;     // ww2_view_needs_resync differs from the oracle's resync
	int32 ModelIdMismatches = 0;    // the independent model's ids differ from the oracle's (the model is wrong)
	int32 ModelHashMismatches = 0;  // the model's raw rows do not hash to the oracle's units hash (the model is wrong)
	int32 RowMismatches = 0;        // a ww2_view row's fixed columns differ from the model's raw row
	int32 ReadErrors = 0;           // a wrapper read (count, units, resync) failed
	int64 RowsCompared = 0;         // ww2_view rows compared column by column, summed over messages
	int32 ResyncTrue = 0;           // messages after which the oracle says resync
	int32 FinalUnits = 0;
	TArray<FString> Problems;       // the first few problems, for the log
	FString Fatal;                  // the replay could not run (missing file, unreadable line, view not open)

	bool Passed() const;
	FString Summary() const;
};

// Whether a stream's two files (<stem>.jsonl and <stem>.expected.jsonl) are in a folder.
enum class EWW2Fixtures : uint8
{
	Present,  // both
	Absent,   // neither (or no folder): not installed, or not staged into a package
	Partial,  // one of the two: a broken install or staging, always an error
};

struct WW2CORE_API FWW2ViewReplay
{
	// The four recorded streams, by stem (protocol-conquest-p0 ...).
	static TArray<FString> Streams();
	// The stream the startup self-check replays (unreal/Tools/tc-package-linux.sh stages exactly its two files).
	static FString SelfCheckStream() { return TEXT("protocol-conquest-p0"); }
	// Where ww2-ffi.sh installs them for this platform, below the plugin's folder (in a package: the staged copy
	// beside the executable's project folder); empty where nothing is installed or supported.
	static FString ProtocolDir();
	static EWW2Fixtures FindFixtures(const FString& Dir, const FString& Stem);
	// Replays one stream through a fresh FWW2View on the calling thread (from ProtocolDir, or from Dir).
	static bool Run(const FString& Stem, FWW2ReplayReport& Out);
	static bool RunIn(const FString& Dir, const FString& Stem, FWW2ReplayReport& Out);
	// The edge checks. Lines gets one line per check; Failures the ones that failed.
	static bool RunEdges(TArray<FString>& Lines, TArray<FString>& Failures);
	// The startup self-check: the bridge is ready, SelfCheckStream replays clean and every edge check passes. When the
	// stream's files are absent from ProtocolDir (a package built before they were staged), the replay is skipped with
	// a log line and the edge checks decide; a partial pair fails. Summary gets a one-line result.
	static bool StartupSelfCheck(FString& Summary);
	static bool StartupSelfCheckIn(const FString& Dir, FString& Summary, bool* bOutReplaySkipped = nullptr);
	// FNV-1a 64 of a JSON value in the harness's canonical form (harness/PROTOCOL.md), seeded by Hash.
	static void CanonicalHash(uint64& Hash, const TSharedPtr<class FJsonValue>& Value);
};
