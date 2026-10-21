// The Unreal side of the Rust core's C interface (ww2-ffi, docs/contracts/rust-unreal-abi.md in the native repo).
//
// This plugin is the only code that includes ww2_core.h or calls a ww2_* function (its automation tests also call the C
// entry points directly, for arguments the wrapper never passes). It copies Unreal arrays into the C structs, calls the
// library with caller-owned buffers (the library allocates nothing the caller frees except the view handle, which
// FWW2View owns), turns status codes into FWW2Result with the library's message, and checks the interface version once
// at startup.
//
// Status: ABI 2.1 consumer at approved native 8f37717c: formation, recipient units, placement and detached authoring
// queries. The pinned provider also has additive minor-2 exports, which this consumer does not adopt.
// Linux x86_64 only; Win64 and macOS arm64 build without the library
// (State() == NotLinked). Values are simulation units (metres, radians, x east, z south); nothing here converts to
// Unreal space. The view exposes only the fixed unit columns in ABI 2: fog, cells, regions, HUD fields, tables and the
// other rules queries beyond placement are later slices, so the client's C++ decoder stays. The client runs the view
// beside it (TC_DECODER=shadow) or takes its unit rows from the view (TC_DECODER=rust, the default); see the client's
// Net/TCRustViewShadow.h.
//
// Threads: the formation calls may run on any thread; the library's last error message is per thread. A view handle
// is used by one thread at a time: FWW2View binds to the thread that opened it and refuses calls from any other thread
// until that thread calls Adopt().
#pragma once

#include "CoreMinimal.h"

DECLARE_LOG_CATEGORY_EXTERN(LogWW2Core, Log, All);

enum class EWW2CoreState : uint8
{
	// Built without the library (no install for this platform). Every call returns ErrNotLinked.
	NotLinked,
	// Linked, major matches the header/wrapper, and the library supports the required additive minor revision.
	Ready,
	// Linked, but major differs or the minor is too old. Every call returns ErrRefused; no query is called.
	AbiMismatch,
};

struct ww2_view; // the library's opaque recipient view (ww2_core.h); only FWW2View holds one

// A unit to place: ww2_formation_item, in Unreal types.
struct FWW2FormationItem
{
	uint32 Id = 0;
	bool bBack = false;
	double X = 0, Z = 0;
	double Size = 3;
};

// Where one unit should stand: ww2_formation_spot.
struct FWW2FormationSpot
{
	uint32 Id = 0;
	double X = 0, Z = 0;
};

// One fixed-column unit row of a recipient view: ww2_view_unit, in Unreal types and at full width (uint32 ids, double
// columns). The row's wire tail (vehicle motion, digging progress) is not exposed by ABI 2.
struct FWW2ViewUnit
{
	uint32 Id = 0;
	int32 Owner = 0;
	FString Kind; // the wire type name (UTF-8 in the library, at most 63 bytes)
	double X = 0, Z = 0, Rot = 0, Aim = 0, Hp = 0, Supp = 0, Cd = 0, Built = 0;
	uint32 Target = 0, Cover = 0, Flags = 0, Stars = 0;
};

// A call's status: the library's code (0 or a negative WW2_ERR_*), or one of this wrapper's own codes below.
struct FWW2Result
{
	int32 Code = 0;
	FString Error;
	bool IsOk() const { return Code == 0; }
};

// IF-12 works only on an author's document, never a private authoritative map obtained from a live room.
// This is an interface layer, not an editor UI. CreatePreview returns JSON and creates no persistent handle.
enum class EWW2AuthoringQuery : uint8
{
	ValidateMap, MigrateScenario, MigrateMap, SpawnsFor, CreatePreview, FindPath, HouseKinds, PlayMatch,
};

class WW2CORE_API FWW2Core
{
public:
	// The interface version this wrapper is written against. A header with another WW2_ABI_VERSION does not compile.
	static constexpr uint32 WrapperAbiVersion = 2;
	static constexpr uint32 RequiredAbiMinorVersion = 1;
	static constexpr int32 MaxQueryInputBytes = 16 * 1024 * 1024;
	// Pure version gate, also used by the previous-version regression. Major must match exactly; minors are additive.
	static bool AcceptsAbi(uint32 Major, uint32 Minor) { return Major == WrapperAbiVersion && Minor >= RequiredAbiMinorVersion; }

	// The library's status codes (ww2_core.h; checked against the header at compile time when linked).
	static constexpr int32 Ok = 0;
	static constexpr int32 ErrPanic = -1;
	static constexpr int32 ErrNullPointer = -2;
	static constexpr int32 ErrBadArgument = -3;
	static constexpr int32 ErrCapacity = -4;
	static constexpr int32 ErrParse = -5;      // ABI 2: not valid UTF-8, JSON or server message; the view is unchanged
	static constexpr int32 ErrWrongState = -6; // ABI 2: a snapshot before any start
	static constexpr int32 ErrPoisoned = -7;   // ABI 2: an earlier panic poisoned the handle; only free is allowed
	// This wrapper's codes, never returned by the library.
	static constexpr int32 ErrNotLinked = -1000;
	static constexpr int32 ErrRefused = -1001;
	static constexpr int32 ErrContract = -1002; // the library answered outside its documented contract
	static constexpr int32 ErrRange = -1003;    // a value does not fit the C type (more than SIZE_MAX items)
	static constexpr int32 ErrWrongThread = -1004; // wrong recipient owner, or blocking PlayMatch on the game thread
	static constexpr int32 ErrClosed = -1005;   // a view that is not open (never opened, closed, or moved from)

	// WW2_SHAPE_*: the Formation menu's shapes.
	static constexpr int32 ShapeLine = 0;
	static constexpr int32 ShapeBlock = 1;
	static constexpr int32 ShapeColumn = 2;
	static constexpr int32 ShapeWedge = 3;

	static EWW2CoreState State();
	static bool IsReady() { return State() == EWW2CoreState::Ready; }
	static const TCHAR* StateName(EWW2CoreState S);
	// One line for logs: what is linked (commit, toolchain, versions, rules hash), or why nothing is.
	static FString Describe();

	// ww2_abi_version() and the header's WW2_ABI_VERSION; 0 when not linked.
	static uint32 LibraryAbiVersion();
	static uint32 HeaderAbiVersion();
	static uint32 LibraryAbiMinorVersion();
	static uint32 HeaderAbiMinorVersion();
	// ww2_rules_hash(); 0 when not ready.
	static uint64 RulesHash();
	// The native commit, rustc release and archive sha256 recorded by ww2-ffi.sh; empty when not linked.
	static FString NativeRevision();
	static FString RustcRelease();
	static FString LibrarySha256();

	// ww2_formation_slot_size: slotSize from shared/formation.js.
	static FWW2Result SlotSize(bool bInfantry, double Radius, double& Out);
	// ww2_formation_spots: facingSpots from shared/formation.js. Out is replaced by the spots, rank by rank from the
	// front; it can be empty on success (no items, or a degenerate spread). Shape is passed through unchecked, so an
	// unknown shape comes back as the library's ErrBadArgument.
	static FWW2Result FormationSpots(TConstArrayView<FWW2FormationItem> Items, double AtX, double AtZ, double Face, double Reach,
		int32 Shape, double Spread, TArray<FWW2FormationSpot>& Out);
	// Two real calls: capacity discovery, then fill. UTF-8 JSON, explicit lengths, no terminator or returned allocation.
	// Input/output storage never overlap at the C boundary. Out changes only on complete success.
	// PlayMatch is synchronous, including its capacity probe. It is refused on the game thread before either call.
	static FWW2Result AuthoringQuery(EWW2AuthoringQuery Query, TConstArrayView<UTF8CHAR> Json, TArray<UTF8CHAR>& Out);
	// ww2_last_error for the calling thread: the message of the last failed library call, or empty.
	static FString LastError();
	// A status code's name (WW2_ERR_* or this wrapper's), for logs.
	static const TCHAR* CodeName(int32 Code);
};

// One recipient view (ww2_view *): exactly what the server told this client, decoded by the Rust core. Move-only; it
// owns its handle and frees it exactly once (Close or the destructor). Rules it enforces, from the contract:
//  - Ownership: Open() creates the handle, Close() and the destructor release it with ww2_view_free; a moved-from view
//    is closed. Nothing else ever frees it.
//  - Poisoning: once the library answers ErrPanic or ErrPoisoned, the view is poisoned. Every later call except Close
//    returns ErrPoisoned without calling the library; Close still frees it.
//  - No partial writes: Units() replaces Out only on success; on any failure Out is left as it was.
//  - Threads: the handle belongs to the thread that opened it. A call from another thread returns ErrWrongThread and
//    never reaches the library. Adopt() hands the view to the calling thread (the caller guarantees the old owner has
//    stopped using it). Close and the destructor free from any thread, since they end every use.
//  - Version: nothing is called unless FWW2Core::IsReady() (an ABI mismatch refuses every view call, as before).
class WW2CORE_API FWW2View
{
public:
	FWW2View() = default;
	~FWW2View();
	FWW2View(FWW2View&& Other);
	FWW2View& operator=(FWW2View&& Other);
	FWW2View(const FWW2View&) = delete;
	FWW2View& operator=(const FWW2View&) = delete;

	// ww2_view_new. An open view is closed first. The calling thread becomes the owner.
	FWW2Result Open();
	// ww2_view_free, once; a closed view is a no-op. Returns the library's status for the free.
	FWW2Result Close();
	bool IsOpen() const { return Handle != nullptr; }
	bool IsPoisoned() const { return bPoisoned; }
	// Hand the view to the calling thread.
	void Adopt();

	// ww2_view_apply with one complete server message, as received. The FString form sends its UTF-8 encoding (for a
	// message that arrived as valid UTF-8 these are the same bytes).
	FWW2Result Apply(TConstArrayView<UTF8CHAR> Utf8);
	FWW2Result Apply(const FString& Message);
	// ww2_view_unit_count.
	FWW2Result UnitCount(int32& Out);
	// ww2_view_unit_count, then ww2_view_units into a buffer of that size: every row in ascending id order. Out is
	// replaced only on success.
	FWW2Result Units(TArray<FWW2ViewUnit>& Out);
	// ww2_view_needs_resync: the latest snapshot's held count or id XOR disagrees with the rows held.
	FWW2Result NeedsResync(bool& Out);
	// IF-04 uses only delivered recipient state. Same ownership, poison and thread rules as every other view call.
	// World and unsupported recipient cases return errors; they never become invented placement reason values.
	FWW2Result PlacementCheck(TConstArrayView<UTF8CHAR> Json, TArray<UTF8CHAR>& Out);

#if WITH_DEV_AUTOMATION_TESTS
	// The library's panic path cannot be reached from outside (no input makes ww2-ffi panic), so the tests feed this
	// wrapper the code a caught panic returns, to check how it reacts. Not for game code.
	void NoteLibraryCodeForTest(int32 Code) { Note(Code); }
#endif

private:
	// The gate every view call passes: bridge ready, open, not poisoned, on the owning thread.
	bool Enter(FWW2Result& R) const;
	// Remembers a poisoning code from the library.
	void Note(int32 Code);
	void Release();

	ww2_view* Handle = nullptr;
	uint32 OwnerThread = 0;
	bool bPoisoned = false;
};
