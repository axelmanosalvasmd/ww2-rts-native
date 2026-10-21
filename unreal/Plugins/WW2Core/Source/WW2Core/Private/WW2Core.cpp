#include "WW2Core.h"
#include "WW2ViewReplay.h"
#include "HAL/PlatformTLS.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Modules/ModuleManager.h"
#include <type_traits>

DEFINE_LOG_CATEGORY(LogWW2Core);

#if WITH_WW2_FFI
#include "ww2_core.h"

static_assert(WW2_ABI_VERSION == FWW2Core::WrapperAbiVersion, "ww2_core.h is another interface version than WW2Core is written for: update the wrapper with the install");
static_assert(WW2_ABI_MINOR_VERSION >= FWW2Core::RequiredAbiMinorVersion, "ww2_core.h lacks the approved ABI 2.1 queries");
using FJsonQuery = int32_t (*)(const char*, size_t, char*, size_t, size_t*);
static_assert(std::is_same_v<decltype(&ww2_abi_minor_version), uint32_t (*)(void)>);
static_assert(std::is_same_v<decltype(&ww2_view_placement_check), int32_t (*)(ww2_view*, const char*, size_t, char*, size_t, size_t*)>);
static_assert(std::is_same_v<decltype(&ww2_authoring_validate_map), FJsonQuery>
	&& std::is_same_v<decltype(&ww2_authoring_migrate_scenario), FJsonQuery>
	&& std::is_same_v<decltype(&ww2_authoring_migrate_map), FJsonQuery>
	&& std::is_same_v<decltype(&ww2_authoring_spawns_for), FJsonQuery>
	&& std::is_same_v<decltype(&ww2_authoring_create_preview), FJsonQuery>
	&& std::is_same_v<decltype(&ww2_authoring_find_path), FJsonQuery>
	&& std::is_same_v<decltype(&ww2_authoring_house_kinds), FJsonQuery>
	&& std::is_same_v<decltype(&ww2_authoring_play_match), FJsonQuery>, "approved JSON query signature changed");
static_assert(WW2_OK == FWW2Core::Ok && WW2_ERR_PANIC == FWW2Core::ErrPanic && WW2_ERR_NULL_POINTER == FWW2Core::ErrNullPointer
	&& WW2_ERR_BAD_ARGUMENT == FWW2Core::ErrBadArgument && WW2_ERR_CAPACITY == FWW2Core::ErrCapacity && WW2_ERR_PARSE == FWW2Core::ErrParse
	&& WW2_ERR_WRONG_STATE == FWW2Core::ErrWrongState && WW2_ERR_POISONED == FWW2Core::ErrPoisoned, "ww2_core.h status codes moved");
static_assert(WW2_SHAPE_LINE == FWW2Core::ShapeLine && WW2_SHAPE_BLOCK == FWW2Core::ShapeBlock && WW2_SHAPE_COLUMN == FWW2Core::ShapeColumn
	&& WW2_SHAPE_WEDGE == FWW2Core::ShapeWedge, "ww2_core.h shape numbers moved");
// ABI 1's formation structs, unchanged in ABI 2, on x86_64 as the header declares it (Rust #[repr(C)] follows the same C rules). The wrapper copies field by
// field, so these pin what the library reads and writes rather than any Unreal struct's layout.
static_assert(sizeof(ww2_formation_item) == 32 && alignof(ww2_formation_item) == 8 && offsetof(ww2_formation_item, id) == 0
	&& offsetof(ww2_formation_item, back) == 4 && offsetof(ww2_formation_item, x) == 8 && offsetof(ww2_formation_item, z) == 16
	&& offsetof(ww2_formation_item, size) == 24, "ww2_formation_item layout is not ABI 1's");
static_assert(sizeof(ww2_formation_spot) == 24 && offsetof(ww2_formation_spot, id) == 0 && offsetof(ww2_formation_spot, x) == 8
	&& offsetof(ww2_formation_spot, z) == 16, "ww2_formation_spot layout is not ABI 1's");
// ABI 2's unit row: 152 bytes on x86_64 Linux and arm64 macOS (docs/contracts/rust-unreal-abi.md, "Rows"), kind[64]
// zero-terminated and zero-padded, eight doubles, then four uint32. FWW2View::Units copies it field by field.
static_assert(sizeof(ww2_view_unit) == 152 && alignof(ww2_view_unit) == 8, "ww2_view_unit is not ABI 2's 152 bytes");
static_assert(sizeof(((ww2_view_unit*)nullptr)->kind) == 64, "ww2_view_unit kind is not char[64]");
static_assert(offsetof(ww2_view_unit, id) == 0 && offsetof(ww2_view_unit, owner) == 4 && offsetof(ww2_view_unit, kind) == 8
	&& offsetof(ww2_view_unit, x) == 72 && offsetof(ww2_view_unit, z) == 80 && offsetof(ww2_view_unit, rot) == 88
	&& offsetof(ww2_view_unit, aim) == 96 && offsetof(ww2_view_unit, hp) == 104 && offsetof(ww2_view_unit, supp) == 112
	&& offsetof(ww2_view_unit, cd) == 120 && offsetof(ww2_view_unit, built) == 128 && offsetof(ww2_view_unit, target) == 136
	&& offsetof(ww2_view_unit, cover) == 140 && offsetof(ww2_view_unit, flags) == 144 && offsetof(ww2_view_unit, stars) == 148,
	"ww2_view_unit field offsets are not ABI 2's");
static_assert(sizeof(ww2_view_unit::id) == 4 && sizeof(ww2_view_unit::owner) == 4 && sizeof(ww2_view_unit::target) == 4
	&& sizeof(ww2_view_unit::stars) == 4, "ww2_view_unit integer widths moved");
static_assert(sizeof(size_t) == 8 && sizeof(double) == 8, "ABI 1 and 2 are 64-bit only");
#endif

namespace
{
	struct FProbe
	{
		EWW2CoreState State = EWW2CoreState::NotLinked;
		uint32 Library = 0;
		uint32 Minor = 0;
	};

	// Asked once: the library's version cannot change while the process runs.
	const FProbe& Probe()
	{
		static const FProbe P = []
		{
			FProbe R;
#if WITH_WW2_FFI
			R.Library = ww2_abi_version();
			// A major mismatch stops before calling an additive symbol. Old major-2 archives without the minor symbol
			// fail at link time, as the retained previous-library rejection check demonstrates.
			if (R.Library == FWW2Core::WrapperAbiVersion) R.Minor = ww2_abi_minor_version();
			R.State = FWW2Core::AcceptsAbi(R.Library, R.Minor) ? EWW2CoreState::Ready : EWW2CoreState::AbiMismatch;
#endif
			return R;
		}();
		return P;
	}

	bool Gate(FWW2Result& R)
	{
		switch (FWW2Core::State())
		{
		case EWW2CoreState::Ready:
			return true;
		case EWW2CoreState::AbiMismatch:
			R.Code = FWW2Core::ErrRefused;
			R.Error = FString::Printf(TEXT("ww2-ffi refused: library %u.%u, header %u.%u, wrapper requires %u.%u"), FWW2Core::LibraryAbiVersion(), FWW2Core::LibraryAbiMinorVersion(), FWW2Core::HeaderAbiVersion(), FWW2Core::HeaderAbiMinorVersion(), FWW2Core::WrapperAbiVersion, FWW2Core::RequiredAbiMinorVersion);
			return false;
		default:
			R.Code = FWW2Core::ErrNotLinked;
#if WITH_WW2_FFI
			R.Error = TEXT("ww2-ffi not linked");
#else
			R.Error = FString(TEXT("built without ww2-ffi: ")) + TEXT(WW2_FFI_MISSING);
#endif
			return false;
		}
	}

	// The library's message for a failed call, or a note when it gave none.
	void Fail(FWW2Result& R, int32 Code)
	{
		R.Code = Code;
		R.Error = FWW2Core::LastError();
		if (R.Error.IsEmpty()) R.Error = FString::Printf(TEXT("ww2-ffi returned %d without a message"), Code);
	}

#if WITH_WW2_FFI
	FWW2Result JsonCall(TConstArrayView<UTF8CHAR> Json, TArray<UTF8CHAR>& Out, TFunctionRef<int32(char*, size_t, size_t*)> Call)
	{
		FWW2Result R;
		if (Json.Num() > FWW2Core::MaxQueryInputBytes)
			return { FWW2Core::ErrBadArgument, TEXT("JSON query input exceeds 16 MiB") };
		size_t Needed = 0;
		const int32 ProbeCode = Call(nullptr, 0, &Needed);
		if (ProbeCode == WW2_OK) return { FWW2Core::ErrContract, TEXT("JSON capacity probe succeeded without reporting a required buffer") };
		if (ProbeCode != WW2_ERR_CAPACITY) { Fail(R, ProbeCode); return R; }
		if (!Needed) return { FWW2Core::ErrContract, TEXT("JSON query reported zero output bytes") };
		if (Needed > (size_t)MAX_int32)
			return { FWW2Core::ErrRange, TEXT("JSON query output does not fit an Unreal byte array") };
		TArray<UTF8CHAR> Buffer;
		Buffer.SetNumUninitialized((int32)Needed);
		size_t Written = 0;
		const int32 Code = Call(reinterpret_cast<char*>(Buffer.GetData()), Needed, &Written);
		if (Code != WW2_OK) { Fail(R, Code); return R; }
		if (Written != Needed) return { FWW2Core::ErrContract, TEXT("deterministic JSON query changed size between calls") };
		Out = MoveTemp(Buffer);
		return R;
	}
#endif
}

EWW2CoreState FWW2Core::State() { return Probe().State; }

const TCHAR* FWW2Core::StateName(EWW2CoreState S)
{
	switch (S)
	{
	case EWW2CoreState::Ready: return TEXT("ready");
	case EWW2CoreState::AbiMismatch: return TEXT("refused (interface version mismatch)");
	default: return TEXT("not linked");
	}
}

uint32 FWW2Core::LibraryAbiVersion() { return Probe().Library; }
uint32 FWW2Core::LibraryAbiMinorVersion() { return Probe().Minor; }

uint32 FWW2Core::HeaderAbiMinorVersion()
{
#if WITH_WW2_FFI
	return WW2_ABI_MINOR_VERSION;
#else
	return 0;
#endif
}

uint32 FWW2Core::HeaderAbiVersion()
{
#if WITH_WW2_FFI
	return WW2_ABI_VERSION;
#else
	return 0;
#endif
}

uint64 FWW2Core::RulesHash()
{
#if WITH_WW2_FFI
	if (IsReady()) return ww2_rules_hash();
#endif
	return 0;
}

FString FWW2Core::NativeRevision()
{
#if WITH_WW2_FFI
	return TEXT(WW2_FFI_NATIVE_REV);
#else
	return FString();
#endif
}

FString FWW2Core::RustcRelease()
{
#if WITH_WW2_FFI
	return TEXT(WW2_FFI_RUSTC);
#else
	return FString();
#endif
}

FString FWW2Core::LibrarySha256()
{
#if WITH_WW2_FFI
	return TEXT(WW2_FFI_LIB_SHA256);
#else
	return FString();
#endif
}

FString FWW2Core::Describe()
{
#if WITH_WW2_FFI
	return FString::Printf(TEXT("ww2-ffi %s: library interface version %u.%u, header %u.%u, wrapper requires %u.%u; rules hash 0x%016llx; native %s, rustc %s, %s, libww2_ffi.a sha256 %s"),
		StateName(State()), LibraryAbiVersion(), LibraryAbiMinorVersion(), HeaderAbiVersion(), HeaderAbiMinorVersion(), WrapperAbiVersion, RequiredAbiMinorVersion, (unsigned long long)RulesHash(), TEXT(WW2_FFI_NATIVE_REV), TEXT(WW2_FFI_RUSTC),
		TEXT(WW2_FFI_TARGET), *FString(TEXT(WW2_FFI_LIB_SHA256)).Left(12));
#else
	return FString(TEXT("ww2-ffi not linked: ")) + TEXT(WW2_FFI_MISSING);
#endif
}

FString FWW2Core::LastError()
{
#if WITH_WW2_FFI
	if (!IsReady()) return FString(); // A version mismatch never calls a non-version export.
	// Length first (capacity 0), then the bytes; the library never changes the stored message while reading it.
	size_t Len = 0;
	const int32 First = ww2_last_error(nullptr, 0, &Len);
	if (Len == 0) return First == WW2_OK ? FString() : FString::Printf(TEXT("(ww2_last_error returned %d)"), First);
	if (Len > (size_t)MAX_int32) return TEXT("(ww2_last_error: message too long)");
	TArray<UTF8CHAR> Buf;
	Buf.SetNumUninitialized((int32)Len);
	size_t Got = 0;
	const int32 Code = ww2_last_error(reinterpret_cast<char*>(Buf.GetData()), Len, &Got);
	if (Code != WW2_OK || Got != Len) return FString::Printf(TEXT("(ww2_last_error returned %d, %llu of %llu bytes)"), Code, (unsigned long long)Got, (unsigned long long)Len);
	return FString::ConstructFromPtrSize(Buf.GetData(), (int32)Len);
#else
	return FString();
#endif
}

FWW2Result FWW2Core::SlotSize(bool bInfantry, double Radius, double& Out)
{
	FWW2Result R;
	if (!Gate(R)) return R;
#if WITH_WW2_FFI
	double Size = 0;
	const int32 Code = ww2_formation_slot_size(bInfantry ? 1 : 0, Radius, &Size);
	if (Code != WW2_OK) { Fail(R, Code); return R; }
	Out = Size;
#endif
	return R;
}

FWW2Result FWW2Core::FormationSpots(TConstArrayView<FWW2FormationItem> Items, double AtX, double AtZ, double Face, double Reach, int32 Shape, double Spread,
	TArray<FWW2FormationSpot>& Out)
{
	Out.Reset();
	FWW2Result R;
	if (!Gate(R)) return R;
#if WITH_WW2_FFI
	const int32 N = Items.Num();
	// Zeroed, so the padding after `back` is defined; then field by field.
	TArray<ww2_formation_item> In;
	In.SetNumZeroed(N);
	for (int32 I = 0; I < N; I++)
	{
		In[I].id = Items[I].Id;
		In[I].back = Items[I].bBack ? 1 : 0;
		In[I].x = Items[I].X;
		In[I].z = Items[I].Z;
		In[I].size = Items[I].Size;
	}
	TArray<ww2_formation_spot> Spots;
	Spots.SetNumZeroed(N);
	size_t Len = SIZE_MAX; // the library must overwrite it on success and on failure
	const int32 Code = ww2_formation_spots(N ? In.GetData() : nullptr, (size_t)N, AtX, AtZ, Face, Reach, Shape, Spread, N ? Spots.GetData() : nullptr, (size_t)N, &Len);
	if (Code != WW2_OK)
	{
		Fail(R, Code);
		if (Len != 0) R.Error += FString::Printf(TEXT(" (and out_len was %llu, not 0)"), (unsigned long long)Len);
		return R;
	}
	if (Len > (size_t)N)
	{
		R.Code = ErrContract;
		R.Error = FString::Printf(TEXT("ww2_formation_spots reported %llu spots for %d items"), (unsigned long long)Len, N);
		return R;
	}
	Out.SetNumUninitialized((int32)Len);
	for (int32 I = 0; I < (int32)Len; I++) Out[I] = { Spots[I].id, Spots[I].x, Spots[I].z };
#endif
	return R;
}

const TCHAR* FWW2Core::CodeName(int32 Code)
{
	switch (Code)
	{
	case Ok: return TEXT("WW2_OK");
	case ErrPanic: return TEXT("WW2_ERR_PANIC");
	case ErrNullPointer: return TEXT("WW2_ERR_NULL_POINTER");
	case ErrBadArgument: return TEXT("WW2_ERR_BAD_ARGUMENT");
	case ErrCapacity: return TEXT("WW2_ERR_CAPACITY");
	case ErrParse: return TEXT("WW2_ERR_PARSE");
	case ErrWrongState: return TEXT("WW2_ERR_WRONG_STATE");
	case ErrPoisoned: return TEXT("WW2_ERR_POISONED");
	case ErrNotLinked: return TEXT("not linked");
	case ErrRefused: return TEXT("refused (interface version)");
	case ErrContract: return TEXT("outside the contract");
	case ErrRange: return TEXT("out of range");
	case ErrWrongThread: return TEXT("wrong thread");
	case ErrClosed: return TEXT("view not open");
	default: return TEXT("unknown code");
	}
}

// FWW2View

FWW2View::~FWW2View() { Release(); }

FWW2View::FWW2View(FWW2View&& Other) : Handle(Other.Handle), OwnerThread(Other.OwnerThread), bPoisoned(Other.bPoisoned)
{
	Other.Handle = nullptr;
	Other.bPoisoned = false;
}

FWW2View& FWW2View::operator=(FWW2View&& Other)
{
	if (this != &Other)
	{
		Release();
		Handle = Other.Handle; OwnerThread = Other.OwnerThread; bPoisoned = Other.bPoisoned;
		Other.Handle = nullptr;
		Other.bPoisoned = false;
	}
	return *this;
}

void FWW2View::Release()
{
	if (!Handle) return;
#if WITH_WW2_FFI
	// A poisoned handle still accepts free (the only call it accepts). A null handle is never passed.
	const int32 Code = ww2_view_free(Handle);
	if (Code != WW2_OK) UE_LOG(LogWW2Core, Error, TEXT("ww2_view_free returned %d (%s): %s"), Code, FWW2Core::CodeName(Code), *FWW2Core::LastError());
#endif
	Handle = nullptr;
	bPoisoned = false;
}

FWW2Result FWW2View::Close()
{
	FWW2Result R;
#if WITH_WW2_FFI
	if (Handle)
	{
		const int32 Code = ww2_view_free(Handle);
		Handle = nullptr;
		bPoisoned = false;
		if (Code != WW2_OK) Fail(R, Code);
	}
#endif
	return R;
}

void FWW2View::Adopt() { OwnerThread = FPlatformTLS::GetCurrentThreadId(); }

void FWW2View::Note(int32 Code)
{
	if (Code == FWW2Core::ErrPanic || Code == FWW2Core::ErrPoisoned)
	{
		if (!bPoisoned) UE_LOG(LogWW2Core, Error, TEXT("ww2 view poisoned (%d %s): only ww2_view_free is called from now on"), Code, FWW2Core::CodeName(Code));
		bPoisoned = true;
	}
}

bool FWW2View::Enter(FWW2Result& R) const
{
	if (!Gate(R)) return false;
	if (!Handle) { R.Code = FWW2Core::ErrClosed; R.Error = TEXT("the view is not open"); return false; }
	if (bPoisoned) { R.Code = FWW2Core::ErrPoisoned; R.Error = TEXT("the view is poisoned by an earlier panic; only Close is allowed"); return false; }
	if (OwnerThread != FPlatformTLS::GetCurrentThreadId())
	{
		R.Code = FWW2Core::ErrWrongThread;
		R.Error = FString::Printf(TEXT("the view belongs to thread %u, called from thread %u (Adopt() hands it over)"), OwnerThread, FPlatformTLS::GetCurrentThreadId());
		return false;
	}
	return true;
}

FWW2Result FWW2View::Open()
{
	FWW2Result R;
	if (!Gate(R)) return R;
	Release();
#if WITH_WW2_FFI
	ww2_view* New = nullptr;
	const int32 Code = ww2_view_new(&New);
	if (Code != WW2_OK)
	{
		Fail(R, Code);
		if (New) { R.Code = FWW2Core::ErrContract; R.Error += TEXT(" (and the handle was not null)"); }
		return R;
	}
	if (!New) { R.Code = FWW2Core::ErrContract; R.Error = TEXT("ww2_view_new succeeded without a handle"); return R; }
	Handle = New;
	OwnerThread = FPlatformTLS::GetCurrentThreadId();
	bPoisoned = false;
#endif
	return R;
}

FWW2Result FWW2View::Apply(TConstArrayView<UTF8CHAR> Utf8)
{
	FWW2Result R;
	if (!Enter(R)) return R;
#if WITH_WW2_FFI
	const int32 Code = ww2_view_apply(Handle, Utf8.Num() ? reinterpret_cast<const char*>(Utf8.GetData()) : nullptr, (size_t)Utf8.Num());
	if (Code != WW2_OK) { Note(Code); Fail(R, Code); }
#endif
	return R;
}

FWW2Result FWW2View::Apply(const FString& Message)
{
	const FTCHARToUTF8 Utf8(*Message, Message.Len());
	return Apply(TConstArrayView<UTF8CHAR>(reinterpret_cast<const UTF8CHAR*>(Utf8.Get()), Utf8.Length()));
}

FWW2Result FWW2View::UnitCount(int32& Out)
{
	FWW2Result R;
	if (!Enter(R)) return R;
#if WITH_WW2_FFI
	size_t N = 0;
	const int32 Code = ww2_view_unit_count(Handle, &N);
	if (Code != WW2_OK) { Note(Code); Fail(R, Code); return R; }
	if (N > (size_t)MAX_int32) { R.Code = FWW2Core::ErrRange; R.Error = FString::Printf(TEXT("%llu units do not fit an Unreal array"), (unsigned long long)N); return R; }
	Out = (int32)N;
#endif
	return R;
}

FWW2Result FWW2View::Units(TArray<FWW2ViewUnit>& Out)
{
	FWW2Result R;
	if (!Enter(R)) return R;
#if WITH_WW2_FFI
	int32 Count = 0;
	R = UnitCount(Count);
	if (!R.IsOk()) return R;
	// Zeroed rows of exactly the count; the library writes all of them or none. Nothing reaches Out until every row has
	// been checked, so a failure leaves Out as it was.
	TArray<ww2_view_unit> Rows;
	Rows.SetNumZeroed(Count);
	size_t Len = SIZE_MAX;
	const int32 Code = ww2_view_units(Handle, Count ? Rows.GetData() : nullptr, (size_t)Count, &Len);
	if (Code != WW2_OK) { Note(Code); Fail(R, Code); return R; }
	if (Len != (size_t)Count)
	{
		R.Code = FWW2Core::ErrContract;
		R.Error = FString::Printf(TEXT("ww2_view_units wrote %llu rows after ww2_view_unit_count said %d"), (unsigned long long)Len, Count);
		return R;
	}
	TArray<FWW2ViewUnit> Got;
	Got.SetNum(Count);
	for (int32 I = 0; I < Count; I++)
	{
		const ww2_view_unit& U = Rows[I];
		int32 KindLen = 0;
		while (KindLen < (int32)sizeof U.kind && U.kind[KindLen] != 0) KindLen++;
		if (KindLen == (int32)sizeof U.kind)
		{
			R.Code = FWW2Core::ErrContract;
			R.Error = FString::Printf(TEXT("ww2_view_units row %d (id %u): kind has no terminating zero in 64 bytes"), I, U.id);
			return R;
		}
		if (I > 0 && U.id <= Rows[I - 1].id)
		{
			R.Code = FWW2Core::ErrContract;
			R.Error = FString::Printf(TEXT("ww2_view_units rows out of id order at %d: %u after %u"), I, U.id, Rows[I - 1].id);
			return R;
		}
		FWW2ViewUnit& V = Got[I];
		V.Id = U.id; V.Owner = U.owner;
		const FUTF8ToTCHAR Kind(U.kind, KindLen);
		V.Kind = FString::ConstructFromPtrSize(Kind.Get(), Kind.Length());
		V.X = U.x; V.Z = U.z; V.Rot = U.rot; V.Aim = U.aim; V.Hp = U.hp; V.Supp = U.supp; V.Cd = U.cd; V.Built = U.built;
		V.Target = U.target; V.Cover = U.cover; V.Flags = U.flags; V.Stars = U.stars;
	}
	Out = MoveTemp(Got);
#endif
	return R;
}

FWW2Result FWW2View::NeedsResync(bool& Out)
{
	FWW2Result R;
	if (!Enter(R)) return R;
#if WITH_WW2_FFI
	uint8 Flag = 0xAA; // the library must write 0 or 1
	const int32 Code = ww2_view_needs_resync(Handle, &Flag);
	if (Code != WW2_OK) { Note(Code); Fail(R, Code); return R; }
	if (Flag > 1) { R.Code = FWW2Core::ErrContract; R.Error = FString::Printf(TEXT("ww2_view_needs_resync wrote %u"), (unsigned)Flag); return R; }
	Out = Flag != 0;
#endif
	return R;
}

FWW2Result FWW2Core::AuthoringQuery(EWW2AuthoringQuery Query, TConstArrayView<UTF8CHAR> Json, TArray<UTF8CHAR>& Out)
{
	FWW2Result R;
	if (!Gate(R)) return R;
	if (Query == EWW2AuthoringQuery::PlayMatch && IsInGameThread())
		return { ErrWrongThread, TEXT("play_match blocks, including capacity discovery; run it on a worker thread") };
#if WITH_WW2_FFI
	FJsonQuery Call = nullptr;
	switch (Query)
	{
	case EWW2AuthoringQuery::ValidateMap: Call = ww2_authoring_validate_map; break;
	case EWW2AuthoringQuery::MigrateScenario: Call = ww2_authoring_migrate_scenario; break;
	case EWW2AuthoringQuery::MigrateMap: Call = ww2_authoring_migrate_map; break;
	case EWW2AuthoringQuery::SpawnsFor: Call = ww2_authoring_spawns_for; break;
	case EWW2AuthoringQuery::CreatePreview: Call = ww2_authoring_create_preview; break;
	case EWW2AuthoringQuery::FindPath: Call = ww2_authoring_find_path; break;
	case EWW2AuthoringQuery::HouseKinds: Call = ww2_authoring_house_kinds; break;
	case EWW2AuthoringQuery::PlayMatch: Call = ww2_authoring_play_match; break;
	default: return { ErrBadArgument, TEXT("unknown authoring query") };
	}
	return JsonCall(Json, Out, [&](char* Buffer, size_t Capacity, size_t* Count)
	{
		return Call(Json.Num() ? reinterpret_cast<const char*>(Json.GetData()) : nullptr, (size_t)Json.Num(), Buffer, Capacity, Count);
	});
#else
	return R;
#endif
}

FWW2Result FWW2View::PlacementCheck(TConstArrayView<UTF8CHAR> Json, TArray<UTF8CHAR>& Out)
{
	FWW2Result R;
	if (!Enter(R)) return R;
#if WITH_WW2_FFI
	return JsonCall(Json, Out, [&](char* Buffer, size_t Capacity, size_t* Count)
	{
		const int32 Code = ww2_view_placement_check(Handle, Json.Num() ? reinterpret_cast<const char*>(Json.GetData()) : nullptr,
			(size_t)Json.Num(), Buffer, Capacity, Count);
		Note(Code);
		return Code;
	});
#else
	return R;
#endif
}

class FWW2CoreModule : public IModuleInterface
{
public:
	virtual void StartupModule() override
	{
		switch (FWW2Core::State())
		{
		case EWW2CoreState::Ready: UE_LOG(LogWW2Core, Log, TEXT("%s"), *FWW2Core::Describe()); break;
		case EWW2CoreState::AbiMismatch: UE_LOG(LogWW2Core, Error, TEXT("%s. The bridge refuses every call; rebuild with unreal/Tools/ww2-ffi.sh build."), *FWW2Core::Describe()); break;
		default: UE_LOG(LogWW2Core, Warning, TEXT("%s"), *FWW2Core::Describe()); break;
		}
		// -WW2ViewSelfCheck: replay a recorded protocol stream and the edge checks through the view at startup. The
		// client runs the same check itself before its shadow mode (TC_RUST_VIEW=1).
		if (FParse::Param(FCommandLine::Get(), TEXT("WW2ViewSelfCheck")))
		{
			FString Summary;
			const bool bPassed = FWW2ViewReplay::StartupSelfCheck(Summary);
			if (bPassed) { UE_LOG(LogWW2Core, Log, TEXT("%s"), *Summary); }
			else { UE_LOG(LogWW2Core, Error, TEXT("%s"), *Summary); }
		}
	}
};

IMPLEMENT_MODULE(FWW2CoreModule, WW2Core);
