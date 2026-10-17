#include "Client/TCPersistence.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Misc/DateTime.h"
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <time.h>
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Misc/SecureHash.h"
#include "HAL/FileManager.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Policies/CondensedJsonPrintPolicy.h"
#if PLATFORM_UNIX
#include <sys/stat.h>
#endif

bool FTCMatchView::IsValid() const
{
	return MatchId >= 0 && FMath::IsFinite(X) && FMath::IsFinite(Z) && FMath::IsFinite(Yaw) && FMath::IsFinite(Dist) && (!bHasY || FMath::IsFinite(Y));
}

TSharedPtr<FJsonObject> FTCMatchView::Json() const
{
	if (!IsValid()) return nullptr;
	auto O = MakeShared<FJsonObject>(), C = MakeShared<FJsonObject>(), G = MakeShared<FJsonObject>();
	O->SetNumberField(TEXT("matchId"), MatchId);
	C->SetNumberField(TEXT("x"), X); C->SetNumberField(TEXT("z"), Z); C->SetNumberField(TEXT("yaw"), Yaw); C->SetNumberField(TEXT("dist"), Dist);
	if (bHasY) C->SetNumberField(TEXT("y"), Y);
	O->SetObjectField(TEXT("camera"), C);
	auto Ids = [](const TArray<int32>& Values) { TArray<TSharedPtr<FJsonValue>> A; for (int32 Id : Values) A.Add(MakeShared<FJsonValueNumber>(Id)); return A; };
	O->SetArrayField(TEXT("selected"), Ids(Selected));
	for (int32 N = 1; N <= 9; N++) if (const auto* Members = Groups.Find(N)) G->SetArrayField(FString::FromInt(N), Ids(*Members));
	O->SetObjectField(TEXT("groups"), G);
	return O;
}

bool FTCMatchView::Parse(const FJsonObject& O, FTCMatchView& Out)
{
	FTCMatchView V;
	double Id;
	const TSharedPtr<FJsonObject>* C = nullptr;
	if (!O.TryGetNumberField(TEXT("matchId"), Id) || !FMath::IsFinite(Id) || Id < 0 || Id > MAX_int32 || Id != FMath::FloorToDouble(Id) || !O.TryGetObjectField(TEXT("camera"), C) || !C || !C->IsValid()) return false;
	V.MatchId = (int32)Id;
	if (!(*C)->TryGetNumberField(TEXT("x"), V.X) || !(*C)->TryGetNumberField(TEXT("z"), V.Z) || !(*C)->TryGetNumberField(TEXT("yaw"), V.Yaw) || !(*C)->TryGetNumberField(TEXT("dist"), V.Dist)) return false;
	V.bHasY = (*C)->TryGetNumberField(TEXT("y"), V.Y);
	if (V.bHasY && !FMath::IsFinite(V.Y)) V.bHasY = false;
	if (!V.IsValid()) return false;
	auto Ids = [](const FJsonObject& From, const FString& Key) {
		TArray<int32> R; const TArray<TSharedPtr<FJsonValue>>* A = nullptr;
		if (From.TryGetArrayField(Key, A)) for (const auto& E : *A) { double N; if (E && E->Type == EJson::Number && E->TryGetNumber(N) && FMath::IsFinite(N) && N >= MIN_int32 && N <= MAX_int32 && N == FMath::FloorToDouble(N)) R.Add((int32)N); }
		return R;
	};
	V.Selected = Ids(O, TEXT("selected"));
	const TSharedPtr<FJsonObject>* G = nullptr;
	if (O.TryGetObjectField(TEXT("groups"), G) && G && G->IsValid()) for (int32 N = 1; N <= 9; N++) if ((*G)->HasTypedField<EJson::Array>(FString::FromInt(N))) V.Groups.Add(N, Ids(**G, FString::FromInt(N)));
	Out = MoveTemp(V); return true;
}

void FTCMatchViewMemory::Open(const FString& Directory, const FString& Server, const FString& Room, const FString& Token)
{
	File.Reset();
	if (Directory.IsEmpty() || Token.IsEmpty()) return;
	const FTCHARToUTF8 Key(*(Server + TEXT("\n") + Room + TEXT("\n") + Token));
	uint8 Hash[FSHA1::DigestSize]; FSHA1::HashBuffer(Key.Get(), Key.Length(), Hash);
	File = FPaths::Combine(Directory, BytesToHex(Hash, UE_ARRAY_COUNT(Hash)) + TEXT(".json"));
}

bool FTCMatchViewMemory::Read(FTCMatchView& Out) const
{
	if (File.IsEmpty() || IFileManager::Get().FileSize(*File) > 1024 * 1024) return false;
	FString Text; TSharedPtr<FJsonObject> O;
	return FFileHelper::LoadFileToString(Text, *File) && FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), O) && O && FTCMatchView::Parse(*O, Out);
}

bool FTCMatchViewMemory::Write(const FTCMatchView& View) const
{
	const auto O = View.Json();
	if (File.IsEmpty() || !O) return false;
	FString Text;
	if (!FJsonSerializer::Serialize(O.ToSharedRef(), TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Text))) return false;
	IFileManager::Get().MakeDirectory(*FPaths::GetPath(File), true);
#if PLATFORM_UNIX
	chmod(TCHAR_TO_UTF8(*FPaths::GetPath(File)), 0700);
#endif
	const FString Pending = File + TEXT(".writing");
	if (!FFileHelper::SaveStringToFile(Text, *Pending, FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM)) return false;
#if PLATFORM_UNIX
	chmod(TCHAR_TO_UTF8(*Pending), 0600);
#endif
	return IFileManager::Get().Move(*File, *Pending, true, true);
}

void FTCMatchViewMemory::Clear() const { if (!File.IsEmpty()) IFileManager::Get().Delete(*File); }

// Written from the reference at e4e579a (server/saves.js, server.js lobby/save/loadSave/saveRoom/loadRoom/tickRooms,
// client/main.js renderLobby and renderMatchMenu, client/feedback.js, client/index.html). When the pin moves, compare
// these again and move ReferencePin with them.
const TCHAR* const FTCPersistence::ReferencePin = TEXT("e4e579a");

// Helpers live in a named namespace: this file may share a unity translation unit with the other client modules.
namespace TCPersistenceDetail
{
	// Number.prototype.toString(10) (ECMA-262 Number::toString): the shortest digits that read back as the same double,
	// written plainly from 1e-6 up to 1e21 and with an exponent outside that.
	FString JsNumber(double V)
	{
		if (std::isnan(V)) return TEXT("NaN");
		if (!std::isfinite(V)) return V > 0 ? TEXT("Infinity") : TEXT("-Infinity");
		if (V == 0) return TEXT("0"); // -0 too
		const FString Sign = V < 0 ? TEXT("-") : TEXT("");
		const double A = std::fabs(V);
		if (A < 9007199254740992.0 && A == std::floor(A)) return Sign + FString::Printf(TEXT("%lld"), (long long)A);
		char Buf[48] = {};
		for (int P = 1; P <= 17; P++)
		{
			std::snprintf(Buf, sizeof(Buf), "%.*e", P - 1, A);
			if (std::strtod(Buf, nullptr) == A) break;
		}
		// Buf is d[.ddd]e[+-]x: the digits, then the exponent of the first one
		FString Digits;
		const char* C = Buf;
		for (; *C && *C != 'e'; C++) if (*C >= '0' && *C <= '9') Digits.AppendChar((TCHAR)*C);
		const int32 E = *C == 'e' ? std::atoi(C + 1) : 0;
		const int32 K = Digits.Len(), N = E + 1;
		if (K <= N && N <= 21) return Sign + Digits + FString::ChrN(N - K, TEXT('0'));
		if (0 < N && N <= 21) return Sign + Digits.Left(N) + TEXT(".") + Digits.Mid(N);
		if (-6 < N && N <= 0) return Sign + TEXT("0.") + FString::ChrN(-N, TEXT('0')) + Digits;
		const int32 X = N - 1;
		return Sign + Digits.Left(1) + (K > 1 ? TEXT(".") + Digits.Mid(1) : FString()) + TEXT("e") + (X >= 0 ? TEXT("+") : TEXT("-")) + FString::FromInt(FMath::Abs(X));
	}

	FString JsString(const TSharedPtr<FJsonValue>& V);

	// Array.prototype.join: undefined and null elements are empty strings.
	FString JsJoin(const TArray<TSharedPtr<FJsonValue>>& Items, const TCHAR* Separator)
	{
		FString Out;
		for (int32 I = 0; I < Items.Num(); I++)
		{
			if (I) Out += Separator;
			const TSharedPtr<FJsonValue>& V = Items[I];
			if (V.IsValid() && V->Type != EJson::None && V->Type != EJson::Null) Out += JsString(V);
		}
		return Out;
	}

	// String(value) for a JSON value; a missing one is undefined.
	FString JsString(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid()) return TEXT("undefined");
		switch (V->Type)
		{
		case EJson::None: return TEXT("undefined");
		case EJson::Null: return TEXT("null");
		case EJson::String: return V->AsString();
		case EJson::Number: return JsNumber(V->AsNumber());
		case EJson::Boolean: return V->AsBool() ? TEXT("true") : TEXT("false");
		case EJson::Array: return JsJoin(V->AsArray(), TEXT(","));
		case EJson::Object: return TEXT("[object Object]");
		}
		return TEXT("undefined");
	}

	bool JsTruthy(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid()) return false;
		switch (V->Type)
		{
		case EJson::Boolean: return V->AsBool();
		case EJson::Number: { const double D = V->AsNumber(); return D != 0 && !std::isnan(D); }
		case EJson::String: return !V->AsString().IsEmpty();
		case EJson::Array: case EJson::Object: return true;
		default: return false;
		}
	}

	// new Date(value).getTime() for what a JSON value can be: a number (TimeClip: whole milliseconds, at most 8.64e15
	// from the epoch), null (0), a boolean (0 or 1). A missing value is an Invalid Date; a string, object or array would be
	// Date.parse'd, which is not ported (the server writes Date.now(), a number).
	bool JsDateMs(const TSharedPtr<FJsonValue>& V, int64& OutMs)
	{
		if (!V.IsValid()) return false;
		double T = 0;
		switch (V->Type)
		{
		case EJson::Null: T = 0; break;
		case EJson::Boolean: T = V->AsBool() ? 1 : 0; break;
		case EJson::Number: T = V->AsNumber(); break;
		default: return false;
		}
		if (!std::isfinite(T) || std::fabs(T) > 8.64e15) return false;
		OutMs = (int64)std::trunc(T);
		return true;
	}

	bool AsciiEqualsNoCase(const FString& A, const TCHAR* B)
	{
		const int32 N = FCString::Strlen(B);
		if (A.Len() != N) return false;
		for (int32 I = 0; I < N; I++)
		{
			TCHAR X = A[I], Y = B[I];
			if (X >= 'A' && X <= 'Z') X = X - 'A' + 'a';
			if (Y >= 'A' && Y <= 'Z') Y = Y - 'A' + 'a';
			if (X != Y) return false;
		}
		return true;
	}

	TSharedRef<FJsonObject> Message(const TCHAR* T)
	{
		TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
		M->SetStringField(TEXT("t"), T);
		return M;
	}
}

const TCHAR* LexToString(ETCPersistenceCommand Command)
{
	switch (Command)
	{
	case ETCPersistenceCommand::None: return TEXT("none");
	case ETCPersistenceCommand::Save: return TEXT("save");
	case ETCPersistenceCommand::LoadSave: return TEXT("loadSave");
	}
	return TEXT("?");
}

const TCHAR* LexToString(ETCPersistenceExpect Expect)
{
	switch (Expect)
	{
	case ETCPersistenceExpect::Nothing: return TEXT("nothing");
	case ETCPersistenceExpect::Saved: return TEXT("saved");
	case ETCPersistenceExpect::Loaded: return TEXT("loaded");
	case ETCPersistenceExpect::Ignored: return TEXT("ignored");
	}
	return TEXT("?");
}

const TCHAR* LexToString(ETCPersistenceOutcome Outcome)
{
	switch (Outcome)
	{
	case ETCPersistenceOutcome::None: return TEXT("none");
	case ETCPersistenceOutcome::Pending: return TEXT("pending");
	case ETCPersistenceOutcome::Saved: return TEXT("saved");
	case ETCPersistenceOutcome::Loaded: return TEXT("loaded");
	case ETCPersistenceOutcome::Refused: return TEXT("refused");
	case ETCPersistenceOutcome::Unanswered: return TEXT("unanswered");
	}
	return TEXT("?");
}

FString FTCPersistenceStatus::Describe() const
{
	if (Request == ETCPersistenceCommand::None) return TEXT("no save or load asked");
	FString Out = FString::Printf(TEXT("%s %s"), LexToString(Request), LexToString(Outcome));
	if (!Id.IsEmpty()) Out += FString::Printf(TEXT(" (id %s)"), *Id);
	if (Outcome == ETCPersistenceOutcome::Refused) Out += FString::Printf(TEXT(", reason '%s'"), *DenyReason);
	if (Outcome == ETCPersistenceOutcome::Loaded) Out += FString::Printf(TEXT(", match %d"), MatchId);
	if (AnsweredAt >= 0 && SentAt >= 0) Out += FString::Printf(TEXT(", after %.2f s"), AnsweredAt - SentAt);
	return Out;
}

const TCHAR* FTCPersistence::SaveLabel() { return TEXT("Save match"); }
const TCHAR* FTCPersistence::SaveTip() { return TEXT("Save this match to load it again later from this room's lobby"); }
const TCHAR* FTCPersistence::ListHeading() { return TEXT("Saved matches"); }
const TCHAR* FTCPersistence::LoadLabel() { return TEXT("Load"); }
const TCHAR* FTCPersistence::SavedSentence() { return TEXT("Match saved. Load it from this room's lobby"); }
const TCHAR* FTCPersistence::FailedSentence() { return TEXT("The match could not be saved or loaded"); }

FString FTCPersistence::ModeName(const FString& Mode)
{
	// client/main.js MODE_NAMES
	struct FName2 { const TCHAR* Key; const TCHAR* Name; };
	static const FName2 Names[] = {
		{ TEXT("conquest"), TEXT("Conquest") }, { TEXT("assault"), TEXT("Assault") }, { TEXT("annihilation"), TEXT("Annihilation") },
		{ TEXT("classic"), TEXT("Classic") }, { TEXT("horde"), TEXT("Horde") }, { TEXT("tutorial"), TEXT("Tutorial") },
		{ TEXT("world"), TEXT("World Conquest") },
	};
	for (const FName2& N : Names) if (Mode.Equals(N.Key, ESearchCase::CaseSensitive)) return N.Name;
	return Mode;
}

bool FTCPersistence::ValidSaveId(const FString& Id)
{
	// /^[a-z0-9]{3,12}-\d{13}-(auto|manual)$/i: ASCII only (no u flag: /i folds nothing outside ASCII onto it), and $ is
	// the end of the string (no m flag)
	const int32 Len = Id.Len();
	auto Alnum = [](TCHAR C) { return (C >= 'a' && C <= 'z') || (C >= 'A' && C <= 'Z') || (C >= '0' && C <= '9'); };
	int32 I = 0;
	while (I < Len && Alnum(Id[I])) I++;
	if (I < 3 || I > 12 || I >= Len || Id[I] != '-') return false;
	I++;
	for (int32 K = 0; K < 13; K++, I++) if (I >= Len || Id[I] < '0' || Id[I] > '9') return false;
	if (I >= Len || Id[I] != '-') return false;
	const FString Tail = Id.Mid(I + 1);
	return TCPersistenceDetail::AsciiEqualsNoCase(Tail, TEXT("auto")) || TCPersistenceDetail::AsciiEqualsNoCase(Tail, TEXT("manual"));
}

FString FTCPersistence::SavedAtText(int64 Ms, FTimespan UtcOffset)
{
	// FDateTime holds years 1 to 9999; the browser would still write a month and day outside them (not ported)
	constexpr int64 FirstMs = -62135596800000LL, LastMs = 253402300799999LL;
	if (Ms < FirstMs || Ms > LastMs) return TEXT("Invalid Date");
	const int64 Ticks = FDateTime(1970, 1, 1).GetTicks() + Ms * ETimespan::TicksPerMillisecond + UtcOffset.GetTicks();
	if (Ticks < FDateTime::MinValue().GetTicks() || Ticks > FDateTime::MaxValue().GetTicks()) return TEXT("Invalid Date");
	const FDateTime D(Ticks);
	static const TCHAR* const Months[] = { TEXT("Jan"), TEXT("Feb"), TEXT("Mar"), TEXT("Apr"), TEXT("May"), TEXT("Jun"), TEXT("Jul"), TEXT("Aug"), TEXT("Sep"), TEXT("Oct"), TEXT("Nov"), TEXT("Dec") };
	const int32 H = D.GetHour(), H12 = H % 12 == 0 ? 12 : H % 12;
	// toLocaleString('en-US', { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
	return FString::Printf(TEXT("%s %d, %02d:%02d %s"), Months[D.GetMonth() - 1], D.GetDay(), H12, D.GetMinute(), H < 12 ? TEXT("AM") : TEXT("PM"));
}

FTimespan FTCPersistence::LocalUtcOffsetAt(int64 Ms)
{
	// floor to whole seconds: the zone's offset is the same through the second
	const int64 Seconds = Ms >= 0 ? Ms / 1000 : -((-Ms + 999) / 1000);
	const time_t T = (time_t)Seconds;
	struct tm L = {};
#if PLATFORM_WINDOWS
	if (localtime_s(&L, &T) != 0) return FTimespan::Zero();
#else
	if (!localtime_r(&T, &L)) return FTimespan::Zero();
#endif
	const int32 Year = L.tm_year + 1900, Month = L.tm_mon + 1, Second = FMath::Min(L.tm_sec, 59); // a leap second reads as :59
	if (!FDateTime::Validate(Year, Month, L.tm_mday, L.tm_hour, L.tm_min, Second, 0)) return FTimespan::Zero();
	const FDateTime Wall(Year, Month, L.tm_mday, L.tm_hour, L.tm_min, Second);
	return Wall - (FDateTime(1970, 1, 1) + FTimespan(Seconds * ETimespan::TicksPerSecond));
}

bool FTCPersistence::ReadRow(const TSharedPtr<FJsonValue>& Entry, TFunctionRef<FTimespan(int64)> UtcOffsetAt, FTCSaveRow& Out, FString* OutWhy)
{
	using namespace TCPersistenceDetail;
	Out = FTCSaveRow();
	auto Fail = [OutWhy](const TCHAR* Why) { if (OutWhy) *OutWhy = Why; return false; };
	// renderLobby reads s.names.join and s.world.owned.join: anything else throws there
	if (!Entry.IsValid() || Entry->Type != EJson::Object || !Entry->AsObject().IsValid()) return Fail(TEXT("a row that is not an object"));
	const FJsonObject& S = *Entry->AsObject();
	const TSharedPtr<FJsonValue> Names = S.TryGetField(TEXT("names"));
	if (!Names.IsValid() || Names->Type != EJson::Array) return Fail(TEXT("names is not an array"));
	const TSharedPtr<FJsonValue> World = S.TryGetField(TEXT("world"));
	Out.bWorld = JsTruthy(World);
	const TArray<TSharedPtr<FJsonValue>>* Owned = nullptr;
	if (Out.bWorld)
	{
		if (World->Type != EJson::Object || !World->AsObject().IsValid()) return Fail(TEXT("world has no owned array"));
		const TSharedPtr<FJsonValue> O = World->AsObject()->TryGetField(TEXT("owned"));
		if (!O.IsValid() || O->Type != EJson::Array) return Fail(TEXT("world has no owned array"));
		Owned = &O->AsArray();
		Out.RegionsTotal = JsString(World->AsObject()->TryGetField(TEXT("total")));
		for (const TSharedPtr<FJsonValue>& V : *Owned) Out.RegionsOwned.Add(V.IsValid() && V->Type != EJson::None && V->Type != EJson::Null ? JsString(V) : FString());
	}
	const TSharedPtr<FJsonValue> Kind = S.TryGetField(TEXT("kind"));
	Out.Id = JsString(S.TryGetField(TEXT("id")));
	Out.Kind = JsString(Kind);
	Out.bAuto = Kind.IsValid() && Kind->Type == EJson::String && Kind->AsString() == TEXT("auto");
	Out.Mode = JsString(S.TryGetField(TEXT("mode")));
	Out.Minutes = JsString(S.TryGetField(TEXT("minutes")));
	for (const TSharedPtr<FJsonValue>& V : Names->AsArray()) Out.Names.Add(V.IsValid() && V->Type != EJson::None && V->Type != EJson::Null ? JsString(V) : FString());
	Out.bHasSavedAt = JsDateMs(S.TryGetField(TEXT("savedAt")), Out.SavedAtMs);

	// `${MODE_NAMES[s.mode] ?? s.mode}, ${s.minutes} min${s.world ? `, ${s.world.owned.join(' / ')} of ${s.world.total} regions` : ''}`
	Out.Title = FString::Printf(TEXT("%s, %s min"), *ModeName(Out.Mode), *Out.Minutes);
	if (Out.bWorld) Out.Title += FString::Printf(TEXT(", %s of %s regions"), *JsJoin(*Owned, TEXT(" / ")), *Out.RegionsTotal);
	// `${s.names.join(', ')} · ${s.kind === 'auto' ? 'autosave' : 'saved'} ${when(s.savedAt)}`
	const FString When = Out.bHasSavedAt ? SavedAtText(Out.SavedAtMs, UtcOffsetAt(Out.SavedAtMs)) : FString(TEXT("Invalid Date"));
	Out.Detail = FString::Printf(TEXT("%s · %s %s"), *JsJoin(Names->AsArray(), TEXT(", ")), Out.bAuto ? TEXT("autosave") : TEXT("saved"), *When);
	return true;
}

void FTCPersistence::ApplyLobby(const FJsonObject& M)
{
	// server.js lobby: role: p.role ?? 'commander'
	Role = TEXT("commander");
	M.TryGetStringField(TEXT("role"), Role);
	// renderLobby: const saves = m.saves ?? []
	const TSharedPtr<FJsonValue> Saves = M.TryGetField(TEXT("saves"));
	if (!Saves.IsValid() || Saves->Type == EJson::None || Saves->Type == EJson::Null) { SaveRows.Reset(); Malformed.Reset(); return; }
	if (Saves->Type != EJson::Array) { Malformed = TEXT("saves is not an array"); return; }
	TArray<FTCSaveRow> Next;
	const TArray<TSharedPtr<FJsonValue>>& Entries = Saves->AsArray();
	for (int32 I = 0; I < Entries.Num(); I++)
	{
		FString Why;
		if (!ReadRow(Entries[I], [](int64 Ms) { return LocalUtcOffsetAt(Ms); }, Next.AddDefaulted_GetRef(), &Why))
		{
			Malformed = FString::Printf(TEXT("save %d: %s"), I, *Why);
			return; // the rows stay as they were drawn
		}
	}
	SaveRows = MoveTemp(Next);
	Malformed.Reset();
}

void FTCPersistence::ApplySaved(const FJsonObject& M, const FTCMatchState& State, double Now)
{
	FString Id;
	M.TryGetStringField(TEXT("id"), Id);
	SavedId = Id;
	if (LastRequest.Request == ETCPersistenceCommand::Save && LastRequest.Outcome == ETCPersistenceOutcome::Pending)
	{
		LastRequest.Outcome = ETCPersistenceOutcome::Saved;
		LastRequest.Id = Id;
		LastRequest.AnsweredAt = Now;
	}
	// client/main.js connection.on('saved', () => feedback.show(...))
	FTCPersistenceFeedback F;
	F.Sentence = SavedSentence();
	F.bVisible = HudShown(State);
	Pending.Add(MoveTemp(F));
}

void FTCPersistence::ApplyDeny(const FJsonObject& M, double Now)
{
	FString Cmd, Reason;
	M.TryGetStringField(TEXT("cmd"), Cmd);
	M.TryGetStringField(TEXT("reason"), Reason);
	if (LastRequest.Outcome != ETCPersistenceOutcome::Pending) return;
	const bool bMine = (Cmd == TEXT("save") && LastRequest.Request == ETCPersistenceCommand::Save)
		|| (Cmd == TEXT("loadSave") && LastRequest.Request == ETCPersistenceCommand::LoadSave);
	if (!bMine) return;
	LastRequest.Outcome = ETCPersistenceOutcome::Refused;
	LastRequest.DenyReason = Reason;
	LastRequest.AnsweredAt = Now;
}

void FTCPersistence::ApplyStart(const FJsonObject& M, double Now)
{
	if (LastRequest.Request != ETCPersistenceCommand::LoadSave || LastRequest.Outcome != ETCPersistenceOutcome::Pending) return;
	double Match = -1;
	M.TryGetNumberField(TEXT("matchId"), Match);
	LastRequest.Outcome = ETCPersistenceOutcome::Loaded;
	LastRequest.MatchId = (int32)Match;
	LastRequest.AnsweredAt = Now;
}

void FTCPersistence::OnSocketOpened(double Now)
{
	if (LastRequest.Outcome != ETCPersistenceOutcome::Pending) return;
	LastRequest.Outcome = ETCPersistenceOutcome::Unanswered;
	LastRequest.AnsweredAt = Now;
}

bool FTCPersistence::HudShown(const FTCMatchState& State)
{
	return State.bHasMatch && State.RoomState == TEXT("play");
}

FTCSaveListView FTCPersistence::SaveList(const FTCMatchState& State) const
{
	FTCSaveListView V;
	// $('saves').classList.toggle('hidden', !lobby || !saves.length); host ? Load button : nothing
	V.bShown = State.RoomState == TEXT("lobby") && SaveRows.Num() > 0;
	V.Heading = ListHeading();
	V.bLoadButtons = State.bAmHost;
	V.LoadLabel = LoadLabel();
	V.Rows = SaveRows;
	return V;
}

FTCSaveButton FTCPersistence::SaveButton(const FTCMatchState& State) const
{
	// renderMatchMenu: saveBtn hidden unless lobbyState.amHost; the menu lives in the HUD
	FTCSaveButton B;
	B.bShown = HudShown(State) && State.bAmHost;
	B.Label = SaveLabel();
	B.Tip = SaveTip();
	return B;
}

const FTCSaveRow* FTCPersistence::FindRow(const FString& Id) const
{
	return SaveRows.FindByPredicate([&Id](const FTCSaveRow& R) { return R.Id.Equals(Id, ESearchCase::CaseSensitive); });
}

FTCPersistencePlan FTCPersistence::PlanSave(const FTCMatchState& State) const
{
	FTCPersistencePlan P;
	if (!HudShown(State)) { P.Note = TEXT("no match menu outside a match"); return P; }
	if (!State.bAmHost) { P.Note = TEXT("Save match is the host's: the button is not drawn"); return P; }
	P.bCloseMenu = true; // saveBtn.onclick: sendCmd(...); menuOpen(false)
	if (IsOperative()) { P.Note = TEXT("an operative sends nothing in a match but fps, ping, resync and name"); return P; }
	P.Kind = ETCPersistenceCommand::Save;
	P.Message = TCPersistenceDetail::Message(TEXT("save"));
	P.Expect = ETCPersistenceExpect::Saved;
	P.Note = TEXT("expect 'saved' and a lobby listing it, or deny save 'save' (write failed, or another save still being written)");
	return P;
}

FTCPersistencePlan FTCPersistence::PlanLoad(const FTCMatchState& State, const FString& Id) const
{
	FTCPersistencePlan P;
	if (State.RoomState != TEXT("lobby") || SaveRows.Num() == 0) { P.Note = TEXT("the saves panel shows only in the lobby, with saves listed"); return P; }
	if (!State.bAmHost) { P.Note = TEXT("Load buttons are the host's"); return P; }
	const FTCSaveRow* Row = FindRow(Id);
	if (!Row) { P.Note = FString::Printf(TEXT("no listed save has the id '%s'"), *Id); return P; }
	P.Kind = ETCPersistenceCommand::LoadSave;
	P.SaveId = Row->Id;
	TSharedRef<FJsonObject> M = TCPersistenceDetail::Message(TEXT("loadSave"));
	M->SetStringField(TEXT("id"), Row->Id);
	P.Message = M;
	// server.js: msg.t === 'loadSave' && host && room.state === 'lobby' && validSaveId(msg.id) && msg.id.startsWith(room.code + '-')
	if (ValidSaveId(Row->Id) && !State.RoomCode.IsEmpty() && Row->Id.StartsWith(State.RoomCode + TEXT("-"), ESearchCase::CaseSensitive))
	{
		P.Expect = ETCPersistenceExpect::Loaded;
		P.Note = TEXT("expect pause, lobby (play), pause by the host, start, pause; or deny loadSave 'save' and a lobby");
	}
	else
	{
		P.Expect = ETCPersistenceExpect::Ignored;
		P.Note = FString::Printf(TEXT("the server drops this id without an answer (validSaveId %s, room '%s')"), ValidSaveId(Row->Id) ? TEXT("passes") : TEXT("fails"), *State.RoomCode);
	}
	return P;
}

void FTCPersistence::NoteSent(const FTCPersistencePlan& Plan, bool bSent, double Now)
{
	if (!bSent || !Plan.Message.IsValid() || Plan.Kind == ETCPersistenceCommand::None) return;
	LastRequest = FTCPersistenceStatus();
	LastRequest.Request = Plan.Kind;
	LastRequest.Outcome = ETCPersistenceOutcome::Pending;
	LastRequest.Id = Plan.SaveId;
	LastRequest.SentAt = Now;
}

TArray<FTCPersistenceFeedback> FTCPersistence::TakeFeedback()
{
	TArray<FTCPersistenceFeedback> Out = MoveTemp(Pending);
	Pending.Reset();
	return Out;
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCPersistenceTest
{
	TSharedPtr<FJsonObject> Parse(const FString& Src)
	{
		TSharedPtr<FJsonObject> Obj;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Src), Obj);
		return Obj;
	}
	FString Text(const TSharedPtr<FJsonObject>& O)
	{
		if (!O.IsValid()) return TEXT("null");
		FString Out;
		const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> W = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
		FJsonSerializer::Serialize(O.ToSharedRef(), W);
		return Out;
	}
	FTimespan Utc(int64) { return FTimespan::Zero(); }
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCPersistenceReferenceTest, "ThreeCrossroads.Persistence.Reference", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCPersistenceReferenceTest::RunTest(const FString& Parameters)
{
	using namespace TCPersistenceTest;
	// Rows: lobby saves entries the pinned server sent (scratch probe), plus edge cases, against the strings the browser's
	// own template code gave for them under TZ=UTC in en-US (Node 26).
	struct FCase { const TCHAR* Entry; const TCHAR* Title; const TCHAR* Detail; };
	const FCase Cases[] = {
		{ TEXT(R"({"id":"saveprobe-1791842925725-auto","kind":"auto","savedAt":1791842926515,"mode":"world","minutes":2,"names":["host-token","guest-token","AI 1"],"world":{"total":64,"owned":[1,1,1]}})"),
			TEXT("World Conquest, 2 min, 1 / 1 / 1 of 64 regions"), TEXT("host-token, guest-token, AI 1 · autosave Oct 12, 10:08 PM") },
		{ TEXT(R"({"id":"conq1-1791842955456-manual","kind":"manual","savedAt":1791842955472,"mode":"conquest","minutes":1,"names":["Ana","AI 1"]})"),
			TEXT("Conquest, 1 min"), TEXT("Ana, AI 1 · saved Oct 12, 10:09 PM") },
		{ TEXT(R"({"id":"abc-1767225600000-manual","kind":"manual","savedAt":1767225600000,"mode":"horde","minutes":95,"names":["Solo"]})"),
			TEXT("Horde, 95 min"), TEXT("Solo · saved Jan 1, 12:00 AM") },
		{ TEXT(R"({"id":"abc-1767268800000-auto","kind":"auto","savedAt":1767268800000,"mode":"skirmishX","minutes":7,"names":[],"world":{"total":40,"owned":[12,0,3]}})"),
			TEXT("skirmishX, 7 min, 12 / 0 / 3 of 40 regions"), TEXT(" · autosave Jan 1, 12:00 PM") },
		{ TEXT(R"({"id":"abc-1781003220000-manual","kind":"manual","savedAt":1781003220000,"names":["A","B"]})"),
			TEXT("undefined, undefined min"), TEXT("A, B · saved Jun 9, 11:07 AM") },
		{ TEXT(R"({"id":"x","kind":"manual","mode":"classic","minutes":3,"names":["A"]})"), TEXT("Classic, 3 min"), TEXT("A · saved Invalid Date") },
		{ TEXT(R"({"id":"x","kind":"manual","savedAt":null,"mode":"assault","minutes":3,"names":["A",null]})"), TEXT("Assault, 3 min"), TEXT("A,  · saved Jan 1, 12:00 AM") },
	};
	for (const FCase& C : Cases)
	{
		const TSharedPtr<FJsonObject> O = Parse(C.Entry);
		FTCSaveRow Row;
		FString Why;
		if (!TestTrue(FString::Printf(TEXT("row reads: %s"), C.Entry), FTCPersistence::ReadRow(MakeShared<FJsonValueObject>(O), &Utc, Row, &Why))) continue;
		TestEqual(TEXT("title"), Row.Title, FString(C.Title));
		TestEqual(TEXT("detail"), Row.Detail, FString(C.Detail));
	}

	// String(number) as JavaScript writes it (Node 26)
	TestEqual(TEXT("2**60"), TCPersistenceDetail::JsNumber(1152921504606846976.0), FString(TEXT("1152921504606847000")));
	TestEqual(TEXT("0.1"), TCPersistenceDetail::JsNumber(0.1), FString(TEXT("0.1")));
	TestEqual(TEXT("1e21"), TCPersistenceDetail::JsNumber(1e21), FString(TEXT("1e+21")));
	TestEqual(TEXT("1.5e-7"), TCPersistenceDetail::JsNumber(1.5e-7), FString(TEXT("1.5e-7")));
	TestEqual(TEXT("123.456"), TCPersistenceDetail::JsNumber(123.456), FString(TEXT("123.456")));
	TestEqual(TEXT("-42"), TCPersistenceDetail::JsNumber(-42), FString(TEXT("-42")));
	TestEqual(TEXT("2**53"), TCPersistenceDetail::JsNumber(9007199254740992.0), FString(TEXT("9007199254740992")));
	TestEqual(TEXT("1e-6"), TCPersistenceDetail::JsNumber(1e-6), FString(TEXT("0.000001")));
	TestEqual(TEXT("123e20"), TCPersistenceDetail::JsNumber(123e20), FString(TEXT("1.23e+22")));
	TestEqual(TEXT("5e-324"), TCPersistenceDetail::JsNumber(5e-324), FString(TEXT("5e-324")));

	// server/saves.js validSaveId
	TestTrue(TEXT("probe manual id"), FTCPersistence::ValidSaveId(TEXT("saveprobe-1791842921125-manual")));
	TestTrue(TEXT("probe auto id"), FTCPersistence::ValidSaveId(TEXT("saveprobe-1791842925725-auto")));
	TestTrue(TEXT("case-insensitive"), FTCPersistence::ValidSaveId(TEXT("SAVEPROBE-1791842921125-AUTO")));
	TestTrue(TEXT("another room's id is still valid"), FTCPersistence::ValidSaveId(TEXT("other-1234567890123-manual")));
	TestFalse(TEXT("nope"), FTCPersistence::ValidSaveId(TEXT("nope")));
	TestFalse(TEXT("room of 2"), FTCPersistence::ValidSaveId(TEXT("ab-1234567890123-auto")));
	TestFalse(TEXT("room of 13"), FTCPersistence::ValidSaveId(TEXT("abcdefghijklm-1234567890123-auto")));
	TestFalse(TEXT("12 digits"), FTCPersistence::ValidSaveId(TEXT("abc-123456789012-auto")));
	TestFalse(TEXT("trailing text"), FTCPersistence::ValidSaveId(TEXT("abc-1234567890123-autos")));
	TestFalse(TEXT("trailing newline"), FTCPersistence::ValidSaveId(TEXT("abc-1234567890123-auto\n")));

	// The probe's sequences, as this client takes them.
	FTCMatchState State;
	FTCPersistence P;
	State.RoomCode = TEXT("saveprobe"); State.RoomState = TEXT("play"); State.bHasMatch = true; State.bAmHost = true;
	P.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","role":"commander","state":"play","saves":[]})")));
	TestFalse(TEXT("no saves: no panel"), P.SaveList(State).bShown);
	TestTrue(TEXT("host in a match: Save match drawn"), P.SaveButton(State).bShown);
	FTCPersistencePlan Save = P.PlanSave(State);
	TestEqual(TEXT("save message"), Text(Save.Message), FString(TEXT(R"({"t":"save"})")));
	TestTrue(TEXT("save closes the menu"), Save.bCloseMenu);
	P.NoteSent(Save, true, 10.0);
	TestEqual(TEXT("save pending"), FString(LexToString(P.Status().Outcome)), FString(LexToString(ETCPersistenceOutcome::Pending)));
	P.ApplySaved(*Parse(TEXT(R"({"t":"saved","id":"saveprobe-1791842921125-manual"})")), State, 10.5);
	TestEqual(TEXT("saved"), FString(LexToString(P.Status().Outcome)), FString(LexToString(ETCPersistenceOutcome::Saved)));
	TestEqual(TEXT("saved id"), P.Status().Id, FString(TEXT("saveprobe-1791842921125-manual")));
	TArray<FTCPersistenceFeedback> F = P.TakeFeedback();
	if (TestEqual(TEXT("one flash"), F.Num(), 1))
	{
		TestEqual(TEXT("flash text"), F[0].Sentence, FString(FTCPersistence::SavedSentence()));
		TestTrue(TEXT("flash seen in a match"), F[0].bVisible);
		TestTrue(TEXT("flash blips"), F[0].bErrorSound);
	}

	// a guest has no Save match
	State.bAmHost = false;
	TestFalse(TEXT("guest: no Save match"), P.SaveButton(State).bShown);
	TestFalse(TEXT("guest: nothing sent"), P.PlanSave(State).Message.IsValid());
	State.bAmHost = true;
	// an operative in play: the menu closes, nothing goes out
	P.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","role":"operative","state":"play","saves":[]})")));
	{
		const FTCPersistencePlan Op = P.PlanSave(State);
		TestFalse(TEXT("operative: nothing sent"), Op.Message.IsValid());
		TestTrue(TEXT("operative: menu closes"), Op.bCloseMenu);
	}

	// back in the lobby: the list the probe's lobby carried, newest first
	State.bHasMatch = false; State.RoomState = TEXT("lobby");
	P.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","role":"commander","state":"lobby","saves":[{"id":"saveprobe-1791842925725-auto","kind":"auto","savedAt":1791842926515,"mode":"world","minutes":2,"names":["host-token","guest-token","AI 1"],"world":{"total":64,"owned":[1,1,1]}},{"id":"saveprobe-1791842921125-manual","kind":"manual","savedAt":1791842921716,"mode":"world","minutes":0,"names":["host-token","guest-token","AI 1"],"world":{"total":64,"owned":[1,1,1]}},{"id":"other-1234567890123-manual","kind":"manual","savedAt":1,"mode":"world","minutes":0,"names":[]}]})")));
	const FTCSaveListView List = P.SaveList(State);
	TestTrue(TEXT("panel shown in the lobby"), List.bShown);
	TestTrue(TEXT("host: Load buttons"), List.bLoadButtons);
	TestEqual(TEXT("three rows"), List.Rows.Num(), 3);
	TestFalse(TEXT("no Save match in the lobby"), P.SaveButton(State).bShown);
	FTCPersistencePlan Load = P.PlanLoad(State, TEXT("saveprobe-1791842921125-manual"));
	TestEqual(TEXT("load message"), Text(Load.Message), FString(TEXT(R"({"t":"loadSave","id":"saveprobe-1791842921125-manual"})")));
	TestEqual(TEXT("load expected"), FString(LexToString(Load.Expect)), FString(LexToString(ETCPersistenceExpect::Loaded)));
	TestEqual(TEXT("another room's id is dropped"), FString(LexToString(P.PlanLoad(State, TEXT("other-1234567890123-manual")).Expect)), FString(LexToString(ETCPersistenceExpect::Ignored)));
	TestFalse(TEXT("an unlisted id sends nothing"), P.PlanLoad(State, TEXT("saveprobe-1234567890123-manual")).Message.IsValid());
	// the missing file: deny loadSave 'save'
	P.NoteSent(Load, true, 20.0);
	P.ApplyDeny(*Parse(TEXT(R"({"t":"deny","cmd":"loadSave","reason":"save"})")), 20.1);
	TestEqual(TEXT("load refused"), FString(LexToString(P.Status().Outcome)), FString(LexToString(ETCPersistenceOutcome::Refused)));
	TestEqual(TEXT("refusal reason"), P.Status().DenyReason, FString(TEXT("save")));
	// the good load: pause, lobby (play), pause, start
	P.NoteSent(Load, true, 30.0);
	P.ApplyDeny(*Parse(TEXT(R"({"t":"deny","cmd":"buy","reason":"mp"})")), 30.05);
	TestEqual(TEXT("another command's refusal leaves it pending"), FString(LexToString(P.Status().Outcome)), FString(LexToString(ETCPersistenceOutcome::Pending)));
	P.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","role":"commander","state":"play","saves":[]})")));
	P.ApplyStart(*Parse(TEXT(R"({"t":"start","matchId":2,"you":0})")), 30.2);
	TestEqual(TEXT("loaded"), FString(LexToString(P.Status().Outcome)), FString(LexToString(ETCPersistenceOutcome::Loaded)));
	TestEqual(TEXT("loaded match"), P.Status().MatchId, 2);
	// a socket change while a save waits
	State.RoomState = TEXT("play"); State.bHasMatch = true;
	P.NoteSent(P.PlanSave(State), true, 40.0);
	P.OnSocketOpened(41.0);
	TestEqual(TEXT("unanswered after a reconnect"), FString(LexToString(P.Status().Outcome)), FString(LexToString(ETCPersistenceOutcome::Unanswered)));
	// a guest saving in play is refused by the rules, not by the save branch (probe: deny save 'blocked')
	P.NoteSent(P.PlanSave(State), true, 50.0);
	P.ApplyDeny(*Parse(TEXT(R"({"t":"deny","cmd":"save","reason":"blocked"})")), 50.1);
	TestEqual(TEXT("save refused"), FString(LexToString(P.Status().Outcome)), FString(LexToString(ETCPersistenceOutcome::Refused)));

	// a malformed list keeps the rows drawn (renderLobby throws before it redraws them)
	const int32 Before = P.Rows().Num();
	P.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","state":"lobby","saves":[{"id":"a","names":"x"}]})")));
	TestEqual(TEXT("malformed: rows kept"), P.Rows().Num(), Before);
	TestFalse(TEXT("malformed: reported"), P.LastMalformed().IsEmpty());
	P.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","state":"lobby"})")));
	TestEqual(TEXT("no saves field: no rows"), P.Rows().Num(), 0);
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS

// ---------------------------------------------------------------------------------------------------------------------
// FTCSettingsStore
#include "Misc/ConfigCacheIni.h"
#include "Misc/CString.h"
#include "Misc/Paths.h"
#include "GameFramework/GameUserSettings.h"

const TCHAR* const FTCSettingsStore::Section = TEXT("ThreeCrossroads");

TArrayView<const FTCSettingsStore::FEntry> FTCSettingsStore::Entries()
{
	// every localStorage key of the pinned client/ (fps and editor pages aside), CLI-124's list
	static const FEntry Table[] = {
		{ TEXT("ww2-name"), TEXT("Name"), TEXT("client/main.js $('name') change; public-lobby.js"), TEXT("the text, cut to 16 in the form") },
		{ TEXT("ww2-edge"), TEXT("EdgeScroll"), TEXT("client/camera.js edgeButton"), TEXT("on unless '0' (stored here as True/False)") },
		{ TEXT("ww2-pan"), TEXT("PanSpeed"), TEXT("client/camera.js panButton"), TEXT("/^[012]$/ else 1") },
		{ TEXT("ww2-capture"), TEXT("CaptureMode"), TEXT("client/pointer.js"), TEXT("one of MODES else 'fullscreen'") },
		{ TEXT("ww2-volume"), TEXT("Volume"), TEXT("client/audio.js setVolume"), TEXT("parseFloat clamped to [0, 1]") },
		{ TEXT("ww2-muted"), TEXT("Muted"), TEXT("client/audio.js (read only: an older key)"), TEXT("'1': start at volume 0 when no volume is saved") },
		{ TEXT("ww2-ui-scale"), TEXT("UIScale"), TEXT("client/hud.js uiScale onchange"), TEXT("range input 75..150 step 5, default 100") },
		{ TEXT("ww2-stats"), TEXT("StatsOverlay"), TEXT("client/stats.js (main.js createStats storage)"), TEXT("the overlay switch") },
		{ TEXT("ww2-cover"), TEXT("CoverPreview"), TEXT("client/cover-preview.js"), TEXT("on unless '0' (stored here as True/False)") },
		{ TEXT("ww2-controls-seen"), TEXT("ControlsSeen"), TEXT("client/main.js openControls onboarding"), TEXT("'1': the Controls sheet does not open by itself") },
		{ TEXT("ww2rts-lang"), TEXT("Language"), TEXT("client/i18n.js setLang"), TEXT("'es' or 'en', else the system language") },
		{ TEXT("ww2-autocast"), TEXT("Autocast"), TEXT("client/abilities.js (TCAbilitiesData::AutocastStorageKey)"), TEXT("JSON of the choices per type") },
		{ TEXT("ww2-gfx"), TEXT("Graphics"), TEXT("client/gfx.js set"), TEXT("'low' is Low, anything else High") },
		{ TEXT("ww2-gfx-auto"), TEXT("GraphicsAutoDropped"), TEXT("client/gfx.js watchFps"), TEXT("any value: the automatic drop has happened") },
		{ TEXT("ww2-gore"), TEXT("Gore"), TEXT("client/gfx.js setGore"), TEXT("on unless 'off'") },
	};
	return MakeArrayView(Table);
}

const FTCSettingsStore::FEntry* FTCSettingsStore::Find(const FString& ReferenceKey)
{
	for (const FEntry& E : Entries()) if (ReferenceKey == E.Reference) return &E;
	return nullptr;
}

FTCSettingsStore& FTCSettingsStore::Game()
{
	static FTCSettingsStore Store{ FString() };
	if (Store.Ini.IsEmpty() && GConfig && !GGameUserSettingsIni.IsEmpty()) { Store.Ini = GGameUserSettingsIni; Store.bMemoryOnly = false; }
	return Store;
}

FTCSettingsStore::FTCSettingsStore(const FString& IniFile) : Ini(IniFile), bMemoryOnly(IniFile.IsEmpty()) {}

namespace TCSettingsImpl
{
	// The game's user settings file is in the config cache and the config system saves it (Saved/Config in the editor,
	// the user's settings folder in a packaged game). Any other file (tests) is read and written here directly, as
	// FTCLocale::WriteStored does: the config system reads such a file but never saves it.
	FConfigFile* Cached(const FString& Ini)
	{
		FConfigFile* F = GConfig ? GConfig->FindConfigFile(Ini) : nullptr;
		return F && !F->NoSave ? F : nullptr;
	}
}

TOptional<FString> FTCSettingsStore::Get(const FString& ReferenceKey) const
{
	const FEntry* E = Find(ReferenceKey);
	if (!E) return {};
	if (bMemoryOnly || !GConfig) { const FString* V = Memory.Find(E->Ini); return V ? TOptional<FString>(*V) : TOptional<FString>(); }
	FString Value;
	if (TCSettingsImpl::Cached(Ini)) { if (GConfig->GetString(Section, E->Ini, Value, Ini)) return Value; return {}; }
	FConfigFile File;
	if (FPaths::FileExists(Ini)) File.Read(Ini);
	if (File.GetString(Section, E->Ini, Value)) return Value;
	return {};
}

bool FTCSettingsStore::Set(const FString& ReferenceKey, const FString& Value)
{
	const FEntry* E = Find(ReferenceKey);
	if (!E) return false;
	if (bMemoryOnly || !GConfig) { Memory.Add(E->Ini, Value); return true; }
	if (TCSettingsImpl::Cached(Ini))
	{
		GConfig->SetString(Section, E->Ini, *Value, Ini);
		return Flush();
	}
	FConfigFile File;
	if (FPaths::FileExists(Ini)) File.Read(Ini);
	File.SetString(Section, E->Ini, *Value);
	return File.Write(Ini, false);
}

bool FTCSettingsStore::Remove(const FString& ReferenceKey)
{
	const FEntry* E = Find(ReferenceKey);
	if (!E) return false;
	if (bMemoryOnly || !GConfig) return Memory.Remove(E->Ini) > 0;
	if (TCSettingsImpl::Cached(Ini))
	{
		GConfig->RemoveKey(Section, E->Ini, Ini);
		return Flush();
	}
	FConfigFile File;
	if (FPaths::FileExists(Ini)) File.Read(Ini);
	File.RemoveKeyFromSection(Section, E->Ini);
	return File.Write(Ini, false);
}

bool FTCSettingsStore::Flush() const
{
	if (bMemoryOnly || !GConfig || !TCSettingsImpl::Cached(Ini)) return true; // other files are written by Set
	// ValidateSettings deletes the entire branch on the next launch if the engine's version was never saved.
	// SaveSettings preserves that schema alongside our section, including when the player never opens Fullscreen.
	if (Ini == GGameUserSettingsIni)
		if (UGameUserSettings* Settings = UGameUserSettings::GetGameUserSettings()) Settings->SaveSettings();
	return GConfig->Flush(false, Ini);
}

namespace TCSettingsImpl
{
	// HTML "valid floating-point number": -? (digits (. digits)? | . digits) ([eE] [-+]? digits)?, nothing around it
	bool ParseHtmlFloat(const FString& S, double& Out)
	{
		int32 I = 0;
		const int32 N = S.Len();
		auto Digit = [&](int32 K) { return K < N && FChar::IsDigit(S[K]); };
		if (I < N && S[I] == TEXT('-')) I++;
		const int32 IntStart = I;
		while (Digit(I)) I++;
		const bool bInt = I > IntStart;
		bool bFrac = false;
		if (I < N && S[I] == TEXT('.'))
		{
			const int32 F = ++I;
			while (Digit(I)) I++;
			bFrac = I > F;
			if (!bFrac) return false;
		}
		if (!bInt && !bFrac) return false;
		if (I < N && (S[I] == TEXT('e') || S[I] == TEXT('E')))
		{
			I++;
			if (I < N && (S[I] == TEXT('-') || S[I] == TEXT('+'))) I++;
			const int32 E = I;
			while (Digit(I)) I++;
			if (I == E) return false;
		}
		if (I != N) return false;
		Out = FCString::Atod(*S);
		return FMath::IsFinite(Out);
	}
}

int32 FTCSettingsStore::ReadUIScale(const TOptional<FString>& Stored)
{
	const FString S = Stored.IsSet() ? *Stored : FString(TEXT("100"));
	double V = 0;
	if (!TCSettingsImpl::ParseHtmlFloat(S, V)) V = UIScaleMin + (UIScaleMax - UIScaleMin) / 2.0; // the range's default
	V = FMath::Clamp(V, (double)UIScaleMin, (double)UIScaleMax);
	// step mismatch: the nearest 75 + 5k, the greater one on a tie, within the range
	const double K = (V - UIScaleMin) / UIScaleStep, Lo = FMath::FloorToDouble(K);
	double Snapped = UIScaleMin + UIScaleStep * (K - Lo >= 0.5 ? Lo + 1 : Lo);
	if (Snapped > UIScaleMax) Snapped -= UIScaleStep;
	return (int32)Snapped;
}

bool FTCSettingsStore::SetUIScale(int32 Percent)
{
	return Set(TEXT("ww2-ui-scale"), FString::FromInt(ReadUIScale(FString::FromInt(Percent))));
}

double FTCSettingsStore::ReadVolume(const TOptional<FString>& Stored, const TOptional<FString>& Muted)
{
	// parseFloat: leading whitespace, then the longest prefix that is a number (Infinity is not finite: ignored)
	if (Stored.IsSet())
	{
		const FString T = Stored->TrimStart();
		int32 I = 0;
		const int32 N = T.Len();
		if (I < N && (T[I] == TEXT('+') || T[I] == TEXT('-'))) I++;
		const int32 Start = I;
		while (I < N && FChar::IsDigit(T[I])) I++;
		bool bDigits = I > Start;
		if (I < N && T[I] == TEXT('.')) { I++; const int32 F = I; while (I < N && FChar::IsDigit(T[I])) I++; bDigits |= I > F; }
		if (bDigits)
		{
			if (I < N && (T[I] == TEXT('e') || T[I] == TEXT('E')))
			{
				int32 J = I + 1;
				if (J < N && (T[J] == TEXT('+') || T[J] == TEXT('-'))) J++;
				const int32 E = J;
				while (J < N && FChar::IsDigit(T[J])) J++;
				if (J > E) I = J;
			}
			const double V = FCString::Atod(*T.Left(I));
			if (FMath::IsFinite(V)) return FMath::Clamp(V, 0.0, 1.0);
		}
	}
	return Muted.IsSet() && *Muted == TEXT("1") ? 0.0 : 0.8;
}

FString FTCSettingsStore::ReadLanguage(const TOptional<FString>& Stored, const FString& SystemLanguage)
{
	if (Stored.IsSet() && (*Stored == TEXT("es") || *Stored == TEXT("en"))) return *Stored;
	return SystemLanguage.StartsWith(TEXT("es")) ? TEXT("es") : TEXT("en");
}

bool FTCSettingsStore::ReadEdge(const TOptional<FString>& Stored)
{
	// the reference stores '1' / '0'; this client's EdgeScroll key holds True / False (GConfig SetBool)
	return !Stored.IsSet() || !(*Stored == TEXT("0") || Stored->Equals(TEXT("False"), ESearchCase::IgnoreCase));
}

int32 FTCSettingsStore::ReadPan(const TOptional<FString>& Stored)
{
	return Stored.IsSet() && Stored->Len() == 1 && (*Stored)[0] >= TEXT('0') && (*Stored)[0] <= TEXT('2') ? (*Stored)[0] - TEXT('0') : 1;
}
