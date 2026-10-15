// See TCLocale.h. Ported from client/i18n.js at FTCLocale::ReferencePin; the dictionary is client/es.js as exported by
// unreal/Tools/export-locale.mjs.
#include "Client/TCLocale.h"

#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Misc/App.h"
#include "Misc/CommandLine.h"
#include "Misc/ConfigCacheIni.h"
#include "Misc/FileHelper.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "HAL/PlatformMisc.h"
#include "HAL/FileManager.h"
#include "Misc/AutomationTest.h"
#include "Algo/StableSort.h"
#include "CoreGlobals.h"
#include "ThreeCrossroads.h"

const TCHAR* const FTCLocale::ReferencePin = TEXT("e4e579a");
const TCHAR* const FTCLocale::SettingsSection = TEXT("ThreeCrossroads");
const TCHAR* const FTCLocale::SettingsKey = TEXT("Language");

namespace TCLocaleText
{
	// JavaScript's \s (the RegExp WhiteSpace and LineTerminator set)
	static bool IsSpace(TCHAR C)
	{
		return C == 0x09 || C == 0x0A || C == 0x0B || C == 0x0C || C == 0x0D || C == 0x20 || C == 0xA0 || C == 0x1680
			|| (C >= 0x2000 && C <= 0x200A) || C == 0x2028 || C == 0x2029 || C == 0x202F || C == 0x205F || C == 0x3000 || C == 0xFEFF;
	}
	// [.!?]
	static bool IsStop(TCHAR C) { return C == TEXT('.') || C == TEXT('!') || C == TEXT('?'); }
	// \d (ASCII only without the u flag)
	static bool IsDigit(TCHAR C) { return C >= TEXT('0') && C <= TEXT('9'); }
	// /[A-Za-z]/
	static bool HasAsciiLetter(const FString& S)
	{
		for (const TCHAR C : S) if ((C >= TEXT('A') && C <= TEXT('Z')) || (C >= TEXT('a') && C <= TEXT('z'))) return true;
		return false;
	}
	// /\{(\d)\}/ at I: the digit, else -1
	static int32 HoleAt(const FString& S, int32 I)
	{
		return I + 2 < S.Len() && S[I] == TEXT('{') && IsDigit(S[I + 1]) && S[I + 2] == TEXT('}') ? S[I + 1] - TEXT('0') : -1;
	}
	static bool HasHole(const FString& S)
	{
		for (int32 I = 0; I + 2 < S.Len(); I++) if (HoleAt(S, I) >= 0) return true;
		return false;
	}
	// the text at Pos starts with Lit (case-sensitive)
	static bool LiteralAt(const FString& S, int32 Pos, const FString& Lit)
	{
		return Pos + Lit.Len() <= S.Len() && FCString::Strncmp(*S + Pos, *Lit, Lit.Len()) == 0;
	}
	// i18n.js: p.es.replace(/\{(\d)\}/g, (_, n) => args[n] ?? '')
	static FString Fill(const FString& Es, const FString (&Args)[10])
	{
		FString Out;
		Out.Reserve(Es.Len() + 16);
		for (int32 I = 0; I < Es.Len();)
		{
			const int32 N = HoleAt(Es, I);
			if (N >= 0) { Out += Args[N]; I += 3; }
			else Out.AppendChar(Es[I++]);
		}
		return Out;
	}
}

bool FTCLocaleTable::LoadFile(const FString& Path, FString& OutError)
{
	FString Text;
	if (!FFileHelper::LoadFileToString(Text, *Path)) { OutError = FString::Printf(TEXT("cannot read %s"), *Path); return false; }
	return Load(Text, OutError);
}

bool FTCLocaleTable::Load(const FString& JsonText, FString& OutError)
{
	using namespace TCLocaleText;
	bLoaded = false;
	Ref.Reset(); Exact.Reset(); Patterns.Reset(); Memo.Reset();
	TSharedPtr<FJsonObject> Root;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(JsonText), Root) || !Root.IsValid()) { OutError = TEXT("not JSON"); return false; }
	Root->TryGetStringField(TEXT("reference"), Ref);
	const TArray<TSharedPtr<FJsonValue>>* Entries = nullptr;
	if (!Root->TryGetArrayField(TEXT("entries"), Entries)) { OutError = TEXT("no entries"); return false; }
	for (int32 I = 0; I < Entries->Num(); I++)
	{
		const TArray<TSharedPtr<FJsonValue>>* Pair = nullptr;
		FString En, Es;
		if (!(*Entries)[I].IsValid() || !(*Entries)[I]->TryGetArray(Pair) || Pair->Num() != 2
			|| (*Pair)[0]->Type != EJson::String || (*Pair)[1]->Type != EJson::String)
		{
			OutError = FString::Printf(TEXT("entry %d is not an [English, Spanish] pair of strings"), I);
			return false;
		}
		En = (*Pair)[0]->AsString(); Es = (*Pair)[1]->AsString();
		// for (const [en, es] of Object.entries(ES)) { if (!/\{\d\}/.test(en)) { exact.set(en, es); continue; } ... }
		if (!HasHole(En)) { Exact.Add(En, Es); continue; }
		FPattern P;
		P.Es = Es;
		P.Lit = En.Len();
		FString Lit;
		for (int32 C = 0; C < En.Len();)
		{
			const int32 N = HoleAt(En, C);
			if (N < 0) { Lit.AppendChar(En[C++]); continue; }
			// .replace(/\{(\d)\}(%?)/g, ...): a hole before % is a number ('([\\d.,]+)%'), else HOLE; the % stays literal
			P.Lit -= 3;
			if (!Lit.IsEmpty()) { P.Pieces.Add({ EPiece::Literal, MoveTemp(Lit) }); Lit.Reset(); }
			P.Order.Add(N);
			C += 3;
			if (C < En.Len() && En[C] == TEXT('%')) { P.Pieces.Add({ EPiece::Percent, FString() }); Lit.AppendChar(TEXT('%')); C++; }
			else P.Pieces.Add({ EPiece::Hole, FString() });
		}
		if (!Lit.IsEmpty()) P.Pieces.Add({ EPiece::Literal, MoveTemp(Lit) });
		Patterns.Add(MoveTemp(P));
	}
	// patterns.sort((a, b) => b.lit - a.lit): Array.prototype.sort is stable
	Algo::StableSort(Patterns, [](const FPattern& A, const FPattern& B) { return A.Lit > B.Lit; });
	bLoaded = true;
	return true;
}

// new RegExp(`^${src}$`, 's').exec(s), where src is the key's literal runs and its holes: HOLE is
// ((?:(?![.!?] ).)+?) (lazy: the shortest first), a number hole ([\d.,]+) before its % (greedy: the longest first).
// Backtracking in the same order as the RegExp gives the same groups.
bool FTCLocaleTable::MatchFrom(const FPattern& P, const FString& S, int32 Piece, int32 Pos, TArray<FString>& Caps) const
{
	using namespace TCLocaleText;
	if (Piece == P.Pieces.Num()) return Pos == S.Len();
	const FPiece& Pc = P.Pieces[Piece];
	switch (Pc.Kind)
	{
	case EPiece::Literal:
		return LiteralAt(S, Pos, Pc.Text) && MatchFrom(P, S, Piece + 1, Pos + Pc.Text.Len(), Caps);
	case EPiece::Hole:
		for (int32 End = Pos + 1; End <= S.Len(); End++)
		{
			// (?![.!?] ) before each character the hole takes (the look-ahead may read past the hole)
			const int32 At = End - 1;
			if (IsStop(S[At]) && At + 1 < S.Len() && S[At + 1] == TEXT(' ')) return false;
			Caps.Add(S.Mid(Pos, End - Pos));
			if (MatchFrom(P, S, Piece + 1, End, Caps)) return true;
			Caps.Pop();
		}
		return false;
	case EPiece::Percent:
	{
		int32 Run = Pos;
		while (Run < S.Len() && (IsDigit(S[Run]) || S[Run] == TEXT('.') || S[Run] == TEXT(','))) Run++;
		for (int32 End = Run; End > Pos; End--)
		{
			Caps.Add(S.Mid(Pos, End - Pos));
			if (MatchFrom(P, S, Piece + 1, End, Caps)) return true;
			Caps.Pop();
		}
		return false;
	}
	}
	return false;
}

// i18n.js core(s): s has no surrounding whitespace and holds a letter
FString FTCLocaleTable::Core(const FString& S)
{
	using namespace TCLocaleText;
	const int32 Len = S.Len();
	// exact.get(s) ?? (/[.!?]$/.test(s) ? exact.get(s.slice(0, -1)) : undefined), the full stop put back
	if (const FString* Hit = Exact.Find(S)) return *Hit;
	if (Len > 0 && IsStop(S[Len - 1]))
		if (const FString* Hit = Exact.Find(S.LeftChop(1))) return *Hit + S.Right(1);
	// native: the HUD's columns and tags, before any pattern can match across them (see FTCLocaleTable::NativeColumns)
	{
		FString Out;
		if (NativeColumns(S, Out)) return Out;
	}
	for (const FPattern& P : Patterns)
	{
		TArray<FString> Caps;
		if (!MatchFrom(P, S, 0, 0, Caps)) continue;
		// p.order.forEach((n, i) => { args[n] = spanish(m[i + 1]); }): in hole order
		FString Args[10];
		for (int32 I = 0; I < P.Order.Num(); I++) Args[P.Order[I]] = Spanish(Caps[I]);
		return Fill(P.Es, Args);
	}
	// /^(.+?[.!?]) (\S.*)$/s: the first sentence break that has a character before it and a non-space after it
	for (int32 I = 1; I + 2 < Len; I++)
		if (IsStop(S[I]) && S[I + 1] == TEXT(' ') && !IsSpace(S[I + 2]))
		{
			const FString A = Spanish(S.Left(I + 1));
			const FString B = Spanish(S.Mid(I + 2));
			return A + TEXT(" ") + B;
		}
	// /^(.+?)(: | · )(.+)$/s: the first ': ' or ' · ' with text on both sides
	static const FString Colon = TEXT(": ");
	static const FString Dot = FString(TEXT(" ")) + TCHAR(0x00B7) + TEXT(" ");
	for (int32 K = 1; K < Len; K++)
	{
		const FString* Sep = LiteralAt(S, K, Colon) && K + Colon.Len() < Len ? &Colon : LiteralAt(S, K, Dot) && K + Dot.Len() < Len ? &Dot : nullptr;
		if (!Sep) continue;
		const FString A = Spanish(S.Left(K));
		const FString B = Spanish(S.Mid(K + Sep->Len()));
		return A + *Sep + B;
	}
	// /^(.+) \(([^()]+)\)$/s: the parenthesis closing the text, the one opening it (no parenthesis between), a space
	// before that and text before the space
	if (Len >= 5 && S[Len - 1] == TEXT(')'))
	{
		int32 J = Len - 2;
		while (J >= 0 && S[J] != TEXT('(') && S[J] != TEXT(')')) J--;
		if (J >= 2 && S[J] == TEXT('(') && J < Len - 2 && S[J - 1] == TEXT(' '))
		{
			const FString A = Spanish(S.Left(J - 1));
			const FString B = Spanish(S.Mid(J + 1, Len - 2 - J));
			return A + TEXT(" (") + B + TEXT(")");
		}
	}
	// native: a trailing cooldown or ' #id' (see FTCLocaleTable::NativeTail)
	{
		FString Out;
		if (NativeTail(S, Out)) return Out;
	}
	return S;
}

// Native (not in the reference; the browser never writes these shapes). This client's HUD draws on one line what the
// browser shows as separate elements, so each element is translated by itself, as the browser's DOM observer
// translates each text node. Tried after the exact lookup and before the patterns, so a pattern such as '{0} at {1}' or
// '{0} MP' cannot match across two elements:
//   columns: '  |  ', else ' | ', else a run of two or more spaces between two texts (the HUD's column gaps; es.js has
//            no key with a double space, and its one key with ' | ' is exact, which is looked up first);
//   a tag:   the first ' [tag]' that ends the text or is followed by a space and more text (a hotkey or a badge, which
//            the browser draws in its own element: hud.js <kbd>, the autocast mark), as in 'Recon Flight [Z] 60 MP'.
bool FTCLocaleTable::NativeColumns(const FString& S, FString& Out)
{
	const int32 Len = S.Len();
	int32 At = S.Find(TEXT("  |  "), ESearchCase::CaseSensitive), Sep = 5;
	if (At <= 0) { At = S.Find(TEXT(" | "), ESearchCase::CaseSensitive); Sep = 3; }
	if (At <= 0)
	{
		At = -1;
		for (int32 I = 1; I + 1 < Len; I++)
			if (S[I] == TEXT(' ') && S[I + 1] == TEXT(' ')) { At = I; break; }
		if (At > 0) { Sep = 0; while (At + Sep < Len && S[At + Sep] == TEXT(' ')) Sep++; }
	}
	// each element in turn, left to right as JavaScript evaluates them (the memo can make the order matter)
	if (At > 0 && At + Sep < Len)
	{
		const FString A = Spanish(S.Left(At));
		const FString B = Spanish(S.Mid(At + Sep));
		Out = A + S.Mid(At, Sep) + B;
		return true;
	}
	for (int32 I = 1; I + 3 < Len; I++)
	{
		if (S[I] != TEXT(' ') || S[I + 1] != TEXT('[')) continue;
		int32 J = I + 2;
		while (J < Len && S[J] != TEXT(']') && S[J] != TEXT('[')) J++;
		if (J >= Len || S[J] != TEXT(']') || J == I + 2) continue;
		const bool bEnd = J == Len - 1, bMore = J + 2 < Len && S[J + 1] == TEXT(' ');
		if (!bEnd && !bMore) continue;
		const FString Name = Spanish(S.Left(I));
		const FString Tag = Spanish(S.Mid(I + 2, J - I - 2));
		const FString More = bEnd ? FString() : TEXT(" ") + Spanish(S.Mid(J + 2));
		Out = Name + TEXT(" [") + Tag + TEXT("]") + More;
		return true;
	}
	return false;
}

//   tails, after the reference's own glue rules (which never match these endings): a trailing cooldown ' 21s' or
//   ' 2.5s' (its own element in the browser's ability button), a trailing ' #12' (hud.js: tr(name) + ' #' + id), then
//   an order button's name, ' (key)' and a value that starts with a digit ('Unload (Shift+E) 0/7', 'Mount up (Shift+Q)
//   4 free': hud.js orderBtn keeps the name in the button's title, the key in its <kbd> and the value in .val, three
//   elements the page's observer translates one by one). /^(.+) \(([^()]+)\) (\d.*)$/s, the name greedy: the last such
//   ' (key) ' splits it. (Slice 12.)
bool FTCLocaleTable::NativeTail(const FString& S, FString& Out)
{
	using namespace TCLocaleText;
	const int32 Len = S.Len();
	if (Len >= 4 && S[Len - 1] == TEXT('s') && IsDigit(S[Len - 2]))
	{
		int32 J = Len - 2;
		while (J >= 0 && IsDigit(S[J])) J--;
		if (J >= 1 && S[J] == TEXT('.') && IsDigit(S[J - 1])) { J--; while (J >= 0 && IsDigit(S[J])) J--; }
		if (J >= 1 && S[J] == TEXT(' '))
		{
			Out = Spanish(S.Left(J)) + S.Mid(J);
			return true;
		}
	}
	if (Len >= 4 && IsDigit(S[Len - 1]))
	{
		int32 J = Len - 1;
		while (J >= 0 && IsDigit(S[J])) J--;
		if (J >= 2 && S[J] == TEXT('#') && S[J - 1] == TEXT(' '))
		{
			Out = Spanish(S.Left(J - 1)) + S.Mid(J - 1);
			return true;
		}
	}
	// I is the '(' (a space before it and at least one character before that), K its ')' with no parenthesis between
	// and at least one character inside, then a space and a digit
	for (int32 I = Len - 1; I >= 2; I--)
	{
		if (S[I] != TEXT('(') || S[I - 1] != TEXT(' ')) continue;
		int32 K = I + 1;
		while (K < Len && S[K] != TEXT('(') && S[K] != TEXT(')')) K++;
		if (K >= Len || S[K] != TEXT(')') || K == I + 1) continue;
		if (K + 2 >= Len || S[K + 1] != TEXT(' ') || !IsDigit(S[K + 2])) continue;
		const FString Name = Spanish(S.Left(I - 1));
		const FString Key = Spanish(S.Mid(I + 1, K - I - 1));
		const FString Value = Spanish(S.Mid(K + 2));
		Out = Name + TEXT(" (") + Key + TEXT(") ") + Value;
		return true;
	}
	return false;
}

FString FTCLocaleTable::Spanish(const FString& S)
{
	using namespace TCLocaleText;
	// if (typeof s !== 'string' || !/[A-Za-z]/.test(s)) return s
	if (!HasAsciiLetter(S)) return S;
	if (const FString* Hit = Memo.Find(S)) return *Hit;
	// /^(\s*)([\s\S]*?)(\s*)$/
	int32 B = 0, E = S.Len();
	while (B < E && IsSpace(S[B])) B++;
	while (E > B && IsSpace(S[E - 1])) E--;
	const FString Out = S.Left(B) + Core(S.Mid(B, E - B)) + S.Mid(E);
	if (Memo.Num() > MemoLimit) Memo.Reset();
	Memo.Add(S, Out);
	Memo.Add(Out, Out); // our own Spanish comes back through the observer: leave it be
	return Out;
}

// ---------------------------------------------------------------------------------------------------------------------

namespace
{
	struct FTCLocaleState
	{
		FCriticalSection Lock;
		FTCLocaleTable Table;
		bool bTriedLoad = false;
		FString Error;
		bool bResolved = false;
		ETCLanguage Language = ETCLanguage::English;
		bool bOverride = false;
		ETCLanguage Override = ETCLanguage::English;
	};
	FTCLocaleState& LocaleState()
	{
		static FTCLocaleState State;
		return State;
	}
	// with the lock held
	FTCLocaleTable* LoadedTable(FTCLocaleState& St)
	{
		if (!St.bTriedLoad)
		{
			St.bTriedLoad = true;
			if (St.Table.LoadFile(FTCLocale::DataPath(), St.Error))
			{
				if (St.Table.Reference() != FTCLocale::ReferencePin)
					UE_LOG(LogTC, Warning, TEXT("locale: %s is from reference %s, the i18n port from %s (node unreal/Tools/export-locale.mjs)"), *FTCLocale::DataPath(), *St.Table.Reference(), FTCLocale::ReferencePin);
				UE_LOG(LogTC, Log, TEXT("locale: %d exact texts and %d patterns from reference %s"), St.Table.NumExact(), St.Table.NumPatterns(), *St.Table.Reference());
			}
			else UE_LOG(LogTC, Error, TEXT("locale: %s; every text stays English"), *St.Error);
		}
		return St.Table.IsLoaded() ? &St.Table : nullptr;
	}
}

FString FTCLocale::DataPath()
{
	return FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Data/locale-es.json"));
}

bool FTCLocale::IsLoaded()
{
	FTCLocaleState& St = LocaleState();
	FScopeLock Guard(&St.Lock);
	return LoadedTable(St) != nullptr;
}

FString FTCLocale::LoadError()
{
	FTCLocaleState& St = LocaleState();
	FScopeLock Guard(&St.Lock);
	LoadedTable(St);
	return St.Error;
}

bool FTCLocale::ParseCode(const FString& Code, ETCLanguage& Out)
{
	if (Code.Equals(TEXT("es"), ESearchCase::CaseSensitive)) { Out = ETCLanguage::Spanish; return true; }
	if (Code.Equals(TEXT("en"), ESearchCase::CaseSensitive)) { Out = ETCLanguage::English; return true; }
	return false;
}

ETCLanguage FTCLocale::Resolve(const FString& Stored, bool bPage, const FString& SystemLanguage)
{
	// stored === 'es' || stored === 'en' ? stored : page && navigator.language?.startsWith('es') ? 'es' : 'en'
	ETCLanguage L;
	if (ParseCode(Stored, L)) return L;
	return bPage && SystemLanguage.StartsWith(TEXT("es"), ESearchCase::CaseSensitive) ? ETCLanguage::Spanish : ETCLanguage::English;
}

ETCLanguage FTCLocale::Language()
{
	FTCLocaleState& St = LocaleState();
	FScopeLock Guard(&St.Lock);
	if (St.bOverride) return St.Override;
	if (!St.bResolved)
	{
		St.bResolved = true;
		FString Arg;
		const TCHAR* Why;
		if (FParse::Value(FCommandLine::Get(), TEXT("TCLang="), Arg) && ParseCode(Arg, St.Language)) Why = TEXT("-TCLang");
		else if (GIsAutomationTesting || FApp::IsUnattended() || IsRunningCommandlet()) { St.Language = ETCLanguage::English; Why = TEXT("not a player's run"); }
		else
		{
			const FString Stored = GConfig ? ReadStored(GGameUserSettingsIni) : FString();
			St.Language = Resolve(Stored, true, FPlatformMisc::GetDefaultLanguage());
			Why = Stored.IsEmpty() ? TEXT("system language") : TEXT("stored setting");
		}
		UE_LOG(LogTC, Log, TEXT("locale: language %s (%s; reference %s)"), Code(St.Language), Why, ReferencePin);
	}
	return St.Language;
}

FString FTCLocale::ReadStored(const FString& IniFile)
{
	FString Value;
	if (GConfig) GConfig->GetString(SettingsSection, SettingsKey, Value, IniFile);
	return Value;
}

bool FTCLocale::WriteStored(ETCLanguage L, const FString& IniFile)
{
	if (!GConfig) return false;
	FConfigFile* Cached = GConfig->FindConfigFile(IniFile);
	if (Cached && !Cached->NoSave)
	{
		// GameUserSettings.ini: the config system saves it (Saved/Config/<platform>/ in the editor, the user's
		// settings folder in a packaged game)
		GConfig->SetString(SettingsSection, SettingsKey, Code(L), IniFile);
		return GConfig->Flush(false, IniFile);
	}
	// a file outside the game's config hierarchy, which the config system reads but never saves: written here
	FConfigFile File;
	if (FPaths::FileExists(IniFile)) File.Read(IniFile);
	File.SetString(SettingsSection, SettingsKey, Code(L));
	if (!File.Write(IniFile, false)) return false;
	if (Cached) Cached->SetString(SettingsSection, SettingsKey, Code(L)); // the cached copy agrees with the file
	return true;
}

void FTCLocale::SetLanguage(ETCLanguage L)
{
	// localStorage.setItem(KEY, l); location.reload(): stored, then in use at once
	if (!WriteStored(L, GGameUserSettingsIni)) UE_LOG(LogTC, Warning, TEXT("locale: the user settings could not hold the language; %s for this run only"), Code(L));
	FTCLocaleState& St = LocaleState();
	FScopeLock Guard(&St.Lock);
	St.bResolved = true;
	St.Language = L;
	St.Table.ClearMemo();
	UE_LOG(LogTC, Log, TEXT("locale: language %s (picked)"), Code(L));
}

FString FTCLocale::Spanish(const FString& S)
{
	FTCLocaleState& St = LocaleState();
	FScopeLock Guard(&St.Lock);
	FTCLocaleTable* Table = LoadedTable(St);
	return Table ? Table->Spanish(S) : S;
}

FString FTCLocale::Translate(const FString& S, ETCLanguage L)
{
	return L == ETCLanguage::Spanish ? Spanish(S) : S;
}

FString FTCLocale::T(const FString& S)
{
	return Translate(S, Language());
}

FString FTCLocale::TitleName(const FString& Title, ETCLanguage L)
{
	using namespace TCLocaleText;
	// the title as the page shows it (the observer translates the whole title attribute), then hud.js orderBtn's
	// aria-label rule on it: tip.split(/[:(]/)[0].trim()
	const FString Shown = Translate(Title, L);
	int32 Cut = Shown.Len();
	for (int32 I = 0; I < Shown.Len(); I++)
		if (Shown[I] == TEXT(':') || Shown[I] == TEXT('(')) { Cut = I; break; }
	// String.prototype.trim: the same whitespace and line terminators as \s
	int32 B = 0, E = Cut;
	while (B < E && IsSpace(Shown[B])) B++;
	while (E > B && IsSpace(Shown[E - 1])) E--;
	return Shown.Mid(B, E - B);
}

FString FTCLocale::TitleName(const FString& Title)
{
	return TitleName(Title, Language());
}

FString FTCLocale::OrderNote(const FString& Note, ETCLanguage L)
{
	if (L != ETCLanguage::Spanish) return Note;
	// availability.js: no('Select a unit') for an order that needs a selection
	static const TCHAR* const Empty[] = { TEXT(": nothing selected"), TEXT(": select units first") };
	for (const TCHAR* E : Empty)
		if (Note.EndsWith(E, ESearchCase::CaseSensitive))
			return Translate(Note.LeftChop(FCString::Strlen(E)), L) + TEXT(": ") + Translate(TEXT("Select a unit"), L);
	if (Note.StartsWith(TEXT("Not sent: "), ESearchCase::CaseSensitive)) return Translate(Note.Mid(10), L);
	int32 At = Note.Find(TEXT(" not sent: "), ESearchCase::CaseSensitive);
	if (At > 0) return Translate(Note.Left(At), L) + TEXT(": ") + Translate(Note.Mid(At + 11), L);
	// a sent order or lobby request: its name ('Retreat sent: 3 units, ...', 'Start sent: conquest on ..., 3 seats', 'Add
	// AI sent (room had 2 seats)'); the lobby's Add AI is the button the browser labels '+ Add AI player'
	At = Note.Find(TEXT(" sent: "), ESearchCase::CaseSensitive);
	if (At <= 0) At = Note.Find(TEXT(" sent ("), ESearchCase::CaseSensitive);
	if (At > 0)
	{
		const FString Head = Note.Left(At);
		return Translate(Head == TEXT("Add AI") ? FString(TEXT("+ Add AI player")) : Head, L);
	}
	return Translate(Note, L);
}

FTCLanguagePicker FTCLocale::Picker()
{
	// client/index.html: <select id="langSel" aria-label="Language" title="Language">; i18n.js fills the options
	FTCLanguagePicker P;
	P.Title = TEXT("Language");
	P.Options.Add({ ETCLanguage::English, TEXT("en"), TEXT("English") });
	P.Options.Add({ ETCLanguage::Spanish, TEXT("es"), FString(TEXT("Espa")) + TCHAR(0x00F1) + TEXT("ol") });
	P.Selected = Language();
	return P;
}

namespace
{
	// a JSON value as the string a template literal or textContent would show, for the values these fields carry
	bool ShownString(const TSharedPtr<FJsonValue>& V, FString& Out)
	{
		if (!V.IsValid() || V->IsNull()) return false;
		if (V->Type == EJson::String) { Out = V->AsString(); return true; }
		if (V->Type == EJson::Boolean) { Out = V->AsBool() ? TEXT("true") : TEXT("false"); return true; }
		if (V->Type != EJson::Number) return false; // an object or array: not a line this port draws
		// String(n): whole numbers without a fraction; other numbers as Unreal writes them (JavaScript's shortest
		// round-trip form is not reproduced: no field carries one)
		const double N = V->AsNumber();
		Out = FMath::IsFinite(N) && N == FMath::RoundToDouble(N) && FMath::Abs(N) < 1e15 ? FString::Printf(TEXT("%lld"), (long long)N) : FString::SanitizeFloat(N);
		return true;
	}
}

bool FTCLocale::SayText(const FJsonObject& Shot, ETCLanguage L, FString& Out)
{
	// sh.localized?.[document.documentElement?.lang ?? 'en'] ?? sh.text
	const TSharedPtr<FJsonValue> Localized = Shot.TryGetField(TEXT("localized"));
	if (Localized.IsValid() && Localized->Type == EJson::Object && Localized->AsObject().IsValid()
		&& ShownString(Localized->AsObject()->TryGetField(Code(L)), Out)) return true;
	return ShownString(Shot.TryGetField(TEXT("text")), Out);
}

FString FTCLocale::ObjectiveText(const TSharedPtr<FJsonValue>& Text, ETCLanguage L)
{
	// typeof o.text === 'string' ? o.text : o.text?.[lang] ?? o.text?.en ?? ''
	FString Out;
	if (!Text.IsValid()) return Out;
	if (Text->Type == EJson::String) return Text->AsString();
	if (Text->Type != EJson::Object || !Text->AsObject().IsValid()) return Out;
	const TSharedPtr<FJsonObject> O = Text->AsObject();
	if (ShownString(O->TryGetField(Code(L)), Out)) return Out;
	if (ShownString(O->TryGetField(TEXT("en")), Out)) return Out;
	return FString();
}

FTCLocale::FScopedLanguage::FScopedLanguage(ETCLanguage L)
{
	FTCLocaleState& St = LocaleState();
	FScopeLock Guard(&St.Lock);
	Previous = St.Override;
	bHadOverride = St.bOverride;
	St.bOverride = true;
	St.Override = L;
}

FTCLocale::FScopedLanguage::~FScopedLanguage()
{
	FTCLocaleState& St = LocaleState();
	FScopeLock Guard(&St.Lock);
	St.bOverride = bHadOverride;
	St.Override = Previous;
}

// ---------------------------------------------------------------------------------------------------------------------
#if WITH_DEV_AUTOMATION_TESTS

namespace
{
	bool ReadPairs(const FJsonObject& O, const TCHAR* Field, TArray<TPair<FString, FString>>& Out)
	{
		const TArray<TSharedPtr<FJsonValue>>* A = nullptr;
		if (!O.TryGetArrayField(Field, A)) return false;
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const TArray<TSharedPtr<FJsonValue>>* P = nullptr;
			if (!V.IsValid() || !V->TryGetArray(P) || P->Num() != 2) return false;
			Out.Emplace((*P)[0]->AsString(), (*P)[1]->AsString());
		}
		return true;
	}
}

// The reference's own client/i18n.js answers (run under Node by export-locale.mjs, stored as the oracle in
// locale-es.json), and a few written out here from the same run (node, reference e4e579a, language stored 'es').
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLocaleReferenceTest, "ThreeCrossroads.Locale.Reference", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCLocaleReferenceTest::RunTest(const FString& Parameters)
{
	FString Text, Error;
	if (!TestTrue(TEXT("locale-es.json reads"), FFileHelper::LoadFileToString(Text, *FTCLocale::DataPath()))) return false;
	FTCLocaleTable Table;
	if (!TestTrue(FString::Printf(TEXT("locale-es.json loads (%s)"), *Error), Table.Load(Text, Error))) return false;
	TestEqualSensitive(TEXT("exported from the ported reference"), Table.Reference(), FString(FTCLocale::ReferencePin));
	TSharedPtr<FJsonObject> Root;
	FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root);
	if (!TestTrue(TEXT("locale-es.json is an object"), Root.IsValid())) return false;
	// es.js at e4e579a: 1,405 entries, 228 of them patterns (the export counts them with i18n.js's own test)
	TestEqual(TEXT("exact texts"), Table.NumExact(), (int32)Root->GetNumberField(TEXT("exact")));
	TestEqual(TEXT("patterns"), Table.NumPatterns(), (int32)Root->GetNumberField(TEXT("patterns")));
	TestEqual(TEXT("exact texts at e4e579a"), Table.NumExact(), 1177);
	TestEqual(TEXT("patterns at e4e579a"), Table.NumPatterns(), 228);

	// written out from the reference run
	static const TCHAR* const Fixed[][2] = {
		{ TEXT("Save match"), TEXT("Guardar partida") },                                     // exact
		{ TEXT("Save match."), TEXT("Guardar partida.") },                                   // without its full stop
		{ TEXT("Language"), TEXT("Idioma") },
		{ TEXT("Ammunition: 12.5%"), TEXT("Munici\u00f3n: 12.5%") },                          // a number hole before %
		{ TEXT("Ammunition: 1,5%"), TEXT("Munici\u00f3n: 1,5%") },
		{ TEXT("Overrun on wave 3: 40 kills in 2:15"), TEXT("Arrollados en la oleada 3: 40 bajas causadas en 2:15") }, // lazy {2}{3}
		{ TEXT("Objective: Hold the bridge"), TEXT("Objetivo: Hold the bridge") },          // a hole's miss stays English
		{ TEXT("Objective: Load"), TEXT("Objetivo: Cargar") },                              // a hole translated in turn
		{ TEXT("Carrying 2 of 3 squads"), TEXT("Lleva 2 de 3 pelotones") },
		{ TEXT("Save match. Load"), TEXT("Guardar partida. Cargar") },                      // sentence by sentence
		{ TEXT("Save match: Load"), TEXT("Guardar partida: Cargar") },
		{ TEXT("Save match \u00b7 Load"), TEXT("Guardar partida \u00b7 Cargar") },
		{ TEXT("Save match (Load)"), TEXT("Guardar partida (Cargar)") },                    // a trailing (hotkey)
		{ TEXT("  Load\n"), TEXT("  Cargar\n") },                                           // whitespace kept
		{ TEXT("load"), TEXT("load") },                                                     // case matters
		{ TEXT("M4 Sherman"), TEXT("M4 Sherman") },                                         // model names kept
		{ TEXT("Road"), TEXT("Camino") },
		{ TEXT("123"), TEXT("123") },                                                       // no letter
	};
	for (const auto& F : Fixed)
	{
		Table.ClearMemo();
		TestEqualSensitive(FString::Printf(TEXT("spanish(%s)"), F[0]), Table.Spanish(F[0]), FString(F[1]));
	}

	const TSharedPtr<FJsonObject>* Oracle = nullptr;
	if (!TestTrue(TEXT("the oracle is there"), Root->TryGetObjectField(TEXT("oracle"), Oracle))) return false;
	TArray<TPair<FString, FString>> Sequence, Isolated;
	if (!TestTrue(TEXT("oracle.sequence reads"), ReadPairs(**Oracle, TEXT("sequence"), Sequence))) return false;
	if (!TestTrue(TEXT("oracle.isolated reads"), ReadPairs(**Oracle, TEXT("isolated"), Isolated))) return false;
	TestTrue(TEXT("the oracle has samples"), Sequence.Num() > 2000 && Isolated.Num() > 500);
	// one memo through the whole sequence, as the reference module kept it
	Table.ClearMemo();
	int32 Bad = 0;
	for (const TPair<FString, FString>& P : Sequence)
	{
		const FString Got = Table.Spanish(P.Key);
		if (!Got.Equals(P.Value, ESearchCase::CaseSensitive) && ++Bad <= 10)
			AddError(FString::Printf(TEXT("sequence: '%s' gave '%s', the reference '%s'"), *P.Key, *Got, *P.Value));
	}
	TestEqual(TEXT("sequence samples that differ from the reference"), Bad, 0);
	// a fresh memo per sample
	Bad = 0;
	for (const TPair<FString, FString>& P : Isolated)
	{
		Table.ClearMemo();
		const FString Got = Table.Spanish(P.Key);
		if (!Got.Equals(P.Value, ESearchCase::CaseSensitive) && ++Bad <= 10)
			AddError(FString::Printf(TEXT("isolated: '%s' gave '%s', the reference '%s'"), *P.Key, *Got, *P.Value));
	}
	TestEqual(TEXT("isolated samples that differ from the reference"), Bad, 0);
	// the memo's consequence, as the reference shows it (node, e4e579a): 'Red' -> 'Rojo', 'Network' -> 'Red', then
	// 'Red' -> 'Red' and 'Conquest: Red' -> 'Conquista: Red'
	Table.ClearMemo();
	TestEqualSensitive(TEXT("Red on a fresh memo"), Table.Spanish(TEXT("Red")), FString(TEXT("Rojo")));
	TestEqualSensitive(TEXT("Network"), Table.Spanish(TEXT("Network")), FString(TEXT("Red")));
	TestEqualSensitive(TEXT("Red after Network"), Table.Spanish(TEXT("Red")), FString(TEXT("Red")));
	TestEqualSensitive(TEXT("a hole holding Red after Network"), Table.Spanish(TEXT("Conquest: Red")), FString(TEXT("Conquista: Red")));
	TestTrue(TEXT("the memo holds the answers and the answers as their own"), Table.MemoSize() > 0);
	AddInfo(FString::Printf(TEXT("%d sequence and %d isolated samples against the reference's i18n.js"), Sequence.Num(), Isolated.Num()));

	// English: t() is the text itself
	int32 English = 0;
	for (const TPair<FString, FString>& P : Sequence) English += FTCLocale::Translate(P.Key, ETCLanguage::English).Equals(P.Key, ESearchCase::CaseSensitive);
	TestEqual(TEXT("English answers every sample unchanged"), English, Sequence.Num());
	return true;
}

// The native glue (this client's HUD lines: columns, a trailing [tag] or cooldown) against locale-es.json's
// oracle.native, which export-locale.mjs made from es.js with a port of i18n.js that answers every reference sample as
// the reference does; and the mixed lines slice 10 found, written out.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLocaleNativeTest, "ThreeCrossroads.Locale.Native", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCLocaleNativeTest::RunTest(const FString& Parameters)
{
	FString Text, Error;
	if (!TestTrue(TEXT("locale-es.json reads"), FFileHelper::LoadFileToString(Text, *FTCLocale::DataPath()))) return false;
	FTCLocaleTable Table;
	if (!TestTrue(FString::Printf(TEXT("locale-es.json loads (%s)"), *Error), Table.Load(Text, Error))) return false;
	TSharedPtr<FJsonObject> Root;
	FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root);
	const TSharedPtr<FJsonObject>* Oracle = nullptr;
	const TArray<TSharedPtr<FJsonValue>>* Native = nullptr;
	if (!TestTrue(TEXT("oracle.native is there"), Root.IsValid() && Root->TryGetObjectField(TEXT("oracle"), Oracle) && (*Oracle)->TryGetArrayField(TEXT("native"), Native))) return false;
	int32 Bad = 0, ByGlue = 0;
	for (const TSharedPtr<FJsonValue>& V : *Native)
	{
		const TArray<TSharedPtr<FJsonValue>>& Row = V->AsArray();
		if (!TestTrue(TEXT("a native sample is [text, with the glue, without it]"), Row.Num() == 3)) return false;
		const FString In = Row[0]->AsString(), Want = Row[1]->AsString();
		ByGlue += Want != Row[2]->AsString();
		Table.ClearMemo();
		const FString Got = Table.Spanish(In);
		if (!Got.Equals(Want, ESearchCase::CaseSensitive) && ++Bad <= 10) AddError(FString::Printf(TEXT("native: '%s' gave '%s', expected '%s'"), *In, *Got, *Want));
	}
	TestEqual(TEXT("native samples that differ"), Bad, 0);
	TestTrue(TEXT("the glue changes some of them"), ByGlue > 20);
	// written out (from the same run): the score strip's win rate no longer reads 'a las this rate', MP is MO, the
	// support bar's names translate with their key, a cooldown and the autocast mark stay
	static const TCHAR* const Fixed[][2] = {
		{ TEXT("Chalk wins in 6:20 at this rate  |  Blue catch-up +0.4 MP/s each"), TEXT("A este ritmo, Tiza gana en 6:20  |  Azul recuperación +0.4 MO/s cada uno") },
		{ TEXT("Recruit  |  150 MP  |  army 3/12"), TEXT("Reclutar  |  150 MO  |  army 3/12") },
		{ TEXT("Recon Flight [Z] "), TEXT("Vuelo de reconocimiento [Z] ") },
		{ TEXT("Grenade 21s [auto]"), TEXT("Granada 21s [auto]") },
		{ TEXT("Recon Flight [Z] 60 MP"), TEXT("Vuelo de reconocimiento [Z] 60 MO") },               // '{0} MP' no longer swallows the key
		{ TEXT("Rifle Squad #1: Ammunition: 100%"), TEXT("Pelot\u00f3n de fusileros #1: Munici\u00f3n: 100%") }, // hud.js tr(name) + ' #' + id
		{ TEXT("Three Crossroads | Find a match"), TEXT("Three Crossroads | Buscar partida") }, // es.js's exact key with ' | ' is still exact
	};
	for (const auto& F : Fixed)
	{
		Table.ClearMemo();
		TestEqualSensitive(FString::Printf(TEXT("spanish(%s)"), F[0]), Table.Spanish(F[0]), FString(F[1]));
	}
	AddInfo(FString::Printf(TEXT("%d native HUD shapes, %d of them changed by the native glue"), Native->Num(), ByGlue));

	// slice 12: every name the HUD can show (units, abilities, support calls, faction looks, recruit groups, tiers), with
	// the reference module's own Spanish; the glue never changes one, and the ones the browser keeps English stay
	const TArray<TSharedPtr<FJsonValue>>* Names = nullptr;
	if (!TestTrue(TEXT("oracle.names is there"), (*Oracle)->TryGetArrayField(TEXT("names"), Names))) return false;
	int32 BadName = 0, KeptName = 0;
	for (const TSharedPtr<FJsonValue>& V : *Names)
	{
		const TArray<TSharedPtr<FJsonValue>>& Row = V->AsArray();
		if (!TestTrue(TEXT("a name is [English, Spanish, source]"), Row.Num() == 3)) return false;
		const FString In = Row[0]->AsString(), Want = Row[1]->AsString();
		KeptName += In.Equals(Want, ESearchCase::CaseSensitive);
		Table.ClearMemo();
		const FString Got = Table.Spanish(In);
		if (!Got.Equals(Want, ESearchCase::CaseSensitive) && ++BadName <= 10)
			AddError(FString::Printf(TEXT("name '%s' (%s) gave '%s', the reference '%s'"), *In, *Row[2]->AsString(), *Got, *Want));
	}
	TestEqual(TEXT("names that differ from the reference"), BadName, 0);
	TestTrue(TEXT("the names cover the unit, ability, support and faction tables"), Names->Num() > 150);
	AddInfo(FString::Printf(TEXT("%d names against the reference's i18n.js, %d of them kept English by the reference itself"), Names->Num(), KeptName));

	// slice 12, written out (node, e4e579a): the shapes slice 11 still logged in English and what the call sites draw
	static const TCHAR* const Slice12[][2] = {
		{ TEXT("Unload (Shift+E) 0/7"), TEXT("Descargar (Shift+E) 0/7") },                         // name, (key), value
		{ TEXT("Mount up (Shift+Q) 7 free"), TEXT("Mount up (Shift+Q) 7 libres") },                // es.js has no 'Mount up'
		{ TEXT("Montar (Shift+Q) 7 free"), TEXT("Montar (Shift+Q) 7 libres") },                    // with TitleName
		{ TEXT("HE Shell [A]"), TEXT("HE Shell [A]") },                                            // es.js has no 'HE Shell'
		{ TEXT("88 mm AP Round [A]"), TEXT("88 mm AP Round [A]") },
		{ TEXT("Grenade (F) 21s [A]"), TEXT("Granada (F) 21s [A]") },
		{ TEXT("Control group  1: 3   2: 5"), TEXT("Grupo de control  1: 3   2: 5") },
		{ TEXT("Ctrl+1-9: Set a control group; Shift adds to it"), TEXT("Ctrl+1-9: Asigna un grupo de control; Shift añade a él") },
		{ TEXT("Recruit  |  361 MP  |  6/12 units"), TEXT("Reclutar  |  361 MO  |  6/12 unidades") },
		{ TEXT("361 MP"), TEXT("361 MO") },
		{ TEXT("Flamethrower Team  180 MP"), TEXT("Equipo lanzallamas  180 MO") },               // the faction's name
		{ TEXT("Medics  120 MP"), TEXT("Sanitarios  120 MO") },
		{ TEXT("M4 Sherman  380 MP"), TEXT("M4 Sherman  380 MO") },                               // a model name stays
	};
	for (const auto& F : Slice12)
	{
		Table.ClearMemo();
		TestEqualSensitive(FString::Printf(TEXT("spanish(%s)"), F[0]), Table.Spanish(F[0]), FString(F[1]));
	}

	// FTCLocale::TitleName: the head of the tooltip as the page shows it (the shared table, the language set here)
	static const TCHAR* const MountTip = TEXT("Mount up (Shift+Q): each selected squad climbs into or onto the nearest selected truck, halftrack or tank with a free seat");
	static const TCHAR* const UnloadTip = TEXT("Unload (Shift+E): every squad aboard gets out beside its carrier. To board, right-click your own truck, halftrack, boat or tank with infantry selected");
	static const TCHAR* const ShellTip = TEXT("HE Shell: M4 Sherman, click where. 40s cooldown. Right-click: autocast on/off");
	if (TestTrue(TEXT("the shared dictionary loads"), FTCLocale::IsLoaded()))
	{
		FTCLocale::FScopedLanguage Es(ETCLanguage::Spanish);
		TestEqualSensitive(TEXT("TitleName Mount up in Spanish"), FTCLocale::TitleName(MountTip), FString(TEXT("Montar")));
		TestEqualSensitive(TEXT("TitleName Unload in Spanish"), FTCLocale::TitleName(UnloadTip), FString(TEXT("Descargar")));
		TestEqualSensitive(TEXT("TitleName HE Shell in Spanish (the browser's title keeps it)"), FTCLocale::TitleName(ShellTip), FString(TEXT("HE Shell")));
		TestEqualSensitive(TEXT("TitleName Mount up in English"), FTCLocale::TitleName(MountTip, ETCLanguage::English), FString(TEXT("Mount up")));
		TestEqualSensitive(TEXT("TitleName without a colon or parenthesis"), FTCLocale::TitleName(TEXT("  Save match  "), ETCLanguage::Spanish), FString(TEXT("Guardar partida")));
		TestEqualSensitive(TEXT("TitleName of nothing"), FTCLocale::TitleName(FString(), ETCLanguage::Spanish), FString());
	}
	return true;
}

// The language choice, its storage, the scenario texts by language and TC_T.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLocaleSettingsTest, "ThreeCrossroads.Locale.Settings", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCLocaleSettingsTest::RunTest(const FString& Parameters)
{
	// i18n.js lang
	TestTrue(TEXT("stored es"), FTCLocale::Resolve(TEXT("es"), false, TEXT("en-US")) == ETCLanguage::Spanish);
	TestTrue(TEXT("stored en beats a Spanish system"), FTCLocale::Resolve(TEXT("en"), true, TEXT("es-ES")) == ETCLanguage::English);
	TestTrue(TEXT("nothing stored, Spanish system, a page"), FTCLocale::Resolve(TEXT(""), true, TEXT("es-MX")) == ETCLanguage::Spanish);
	TestTrue(TEXT("nothing stored, Spanish system, not a page"), FTCLocale::Resolve(TEXT(""), false, TEXT("es-MX")) == ETCLanguage::English);
	TestTrue(TEXT("'ES' is not a stored language"), FTCLocale::Resolve(TEXT("ES"), true, TEXT("en-US")) == ETCLanguage::English);
	TestTrue(TEXT("'spanish' is not a stored language"), FTCLocale::Resolve(TEXT("spanish"), true, TEXT("es")) == ETCLanguage::Spanish);
	TestTrue(TEXT("an English system"), FTCLocale::Resolve(TEXT(""), true, TEXT("en-GB")) == ETCLanguage::English);

	// the stored choice survives a restart: written, the file dropped from the config cache, read back from disk (a file
	// of its own under Saved/Automation: the player's GameUserSettings.ini is not touched by the test)
	const FString Ini = FConfigCacheIni::NormalizeConfigIniPath(FPaths::Combine(FPaths::AutomationTransientDir(), TEXT("TCLocaleSettings.ini")));
	IFileManager::Get().Delete(*Ini, false, true, true);
	if (TestTrue(TEXT("a settings file is made"), FFileHelper::SaveStringToFile(TEXT("[ThreeCrossroads]\n"), *Ini)))
	{
		TestEqualSensitive(TEXT("nothing stored yet"), FTCLocale::ReadStored(Ini), FString());
		for (const ETCLanguage L : { ETCLanguage::Spanish, ETCLanguage::English })
		{
			TestTrue(TEXT("the choice is stored"), FTCLocale::WriteStored(L, Ini));
			GConfig->Flush(true, Ini);
			TestEqualSensitive(TEXT("the choice reads back after a restart"), FTCLocale::ReadStored(Ini), FString(FTCLocale::Code(L)));
			FString OnDisk;
			FFileHelper::LoadFileToString(OnDisk, *Ini);
			TestTrue(TEXT("the file holds it"), OnDisk.Contains(FString::Printf(TEXT("Language=%s"), FTCLocale::Code(L))));
			GConfig->Flush(true, Ini);
		}
		IFileManager::Get().Delete(*Ini, false, true, true);
	}

	// TC_T follows the language in use
	TestTrue(TEXT("the dictionary loads"), FTCLocale::IsLoaded());
	{
		FTCLocale::FScopedLanguage Es(ETCLanguage::Spanish);
		TestTrue(TEXT("Spanish in use"), FTCLocale::Language() == ETCLanguage::Spanish);
		TestEqualSensitive(TEXT("TC_T in Spanish"), TC_T(TEXT("Save match")), FString(TEXT("Guardar partida")));
		TestEqualSensitive(TEXT("TC_T keeps an unknown text"), TC_T(FString(TEXT("Zorblat"))), FString(TEXT("Zorblat")));
		const FTCLanguagePicker P = FTCLocale::Picker();
		TestEqualSensitive(TEXT("the picker's title in Spanish"), TC_T(P.Title), FString(TEXT("Idioma")));
		TestEqualSensitive(TEXT("the English option in Spanish"), TC_T(P.Options[0].Label), FString(TEXT("Ingl\u00e9s")));
		TestEqualSensitive(TEXT("the Spanish option"), TC_T(P.Options[1].Label), FString(TEXT("Espa\u00f1ol")));
		TestTrue(TEXT("the picker shows the language in use"), P.Selected == ETCLanguage::Spanish);
		{
			FTCLocale::FScopedLanguage En(ETCLanguage::English);
			TestEqualSensitive(TEXT("TC_T in English"), TC_T(TEXT("Save match")), FString(TEXT("Save match")));
		}
		TestTrue(TEXT("the scope puts the language back"), FTCLocale::Language() == ETCLanguage::Spanish);
	}

	// CLI-121: say lines and objectives by language
	auto Parse = [](const TCHAR* Json) { TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(FString(Json)), O); return O; };
	const TSharedPtr<FJsonObject> Say = Parse(TEXT("{\"k\":\"say\",\"text\":\"Hold the bridge\",\"localized\":{\"en\":\"Hold the bridge!\",\"es\":\"\u00a1Mantened el puente!\"}}"));
	FString Line;
	TestTrue(TEXT("say in Spanish"), FTCLocale::SayText(*Say, ETCLanguage::Spanish, Line) && Line.Equals(TEXT("\u00a1Mantened el puente!"), ESearchCase::CaseSensitive));
	TestTrue(TEXT("say in English reads localized.en"), FTCLocale::SayText(*Say, ETCLanguage::English, Line) && Line.Equals(TEXT("Hold the bridge!"), ESearchCase::CaseSensitive));
	const TSharedPtr<FJsonObject> SayEnOnly = Parse(TEXT("{\"k\":\"say\",\"text\":\"Hold\",\"localized\":{\"en\":\"Hold!\",\"es\":null}}"));
	TestTrue(TEXT("say without Spanish falls back to text, not localized.en"), FTCLocale::SayText(*SayEnOnly, ETCLanguage::Spanish, Line) && Line.Equals(TEXT("Hold"), ESearchCase::CaseSensitive));
	const TSharedPtr<FJsonObject> SayBare = Parse(TEXT("{\"k\":\"say\"}"));
	TestFalse(TEXT("say without any text"), FTCLocale::SayText(*SayBare, ETCLanguage::Spanish, Line));
	const TSharedPtr<FJsonObject> Obj = Parse(TEXT("{\"a\":\"Plain\",\"b\":{\"en\":\"Take the hill\",\"es\":\"Tomad la colina\"},\"c\":{\"en\":\"Only English\"},\"d\":{\"fr\":\"x\"},\"e\":7}"));
	TestEqualSensitive(TEXT("objective string"), FTCLocale::ObjectiveText(Obj->TryGetField(TEXT("a")), ETCLanguage::Spanish), FString(TEXT("Plain")));
	TestEqualSensitive(TEXT("objective in Spanish"), FTCLocale::ObjectiveText(Obj->TryGetField(TEXT("b")), ETCLanguage::Spanish), FString(TEXT("Tomad la colina")));
	TestEqualSensitive(TEXT("objective in English"), FTCLocale::ObjectiveText(Obj->TryGetField(TEXT("b")), ETCLanguage::English), FString(TEXT("Take the hill")));
	TestEqualSensitive(TEXT("objective falls back to en"), FTCLocale::ObjectiveText(Obj->TryGetField(TEXT("c")), ETCLanguage::Spanish), FString(TEXT("Only English")));
	TestEqualSensitive(TEXT("objective with neither"), FTCLocale::ObjectiveText(Obj->TryGetField(TEXT("d")), ETCLanguage::Spanish), FString());
	TestEqualSensitive(TEXT("objective that is a number"), FTCLocale::ObjectiveText(Obj->TryGetField(TEXT("e")), ETCLanguage::Spanish), FString());
	TestEqualSensitive(TEXT("objective absent"), FTCLocale::ObjectiveText(Obj->TryGetField(TEXT("z")), ETCLanguage::Spanish), FString());
	return true;
}


// Round 15: the HUD's texts that stayed English in Spanish (the selection panel with nothing selected, the logistics rows'
// heading and cut, the last order's note) now read the browser's words from client/es.js.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLocaleOrderNoteTest, "ThreeCrossroads.Locale.OrderNote", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCLocaleOrderNoteTest::RunTest(const FString& Parameters)
{
	if (!TestTrue(TEXT("locale-es.json loads"), FTCLocale::IsLoaded())) return false;
	const ETCLanguage Es = ETCLanguage::Spanish, En = ETCLanguage::English;
	const FString Sent = TEXT("Retreat sent: 3 units, 'retreat' message, 61 bytes");
	TestEqualSensitive(TEXT("English is the note itself"), FTCLocale::OrderNote(Sent, En), Sent);
	TestEqualSensitive(TEXT("a sent order: its name"), FTCLocale::OrderNote(Sent, Es), FString(TEXT("Retirada")));
	TestEqualSensitive(TEXT("nothing selected"), FTCLocale::OrderNote(TEXT("Stop: nothing selected"), Es), FString(TEXT("Detener: Selecciona una unidad")));
	TestEqualSensitive(TEXT("select units first"), FTCLocale::OrderNote(TEXT("Attack-move: select units first"), Es), FString(TEXT("Avance de ataque: Selecciona una unidad")));
	TestEqualSensitive(TEXT("not sent: the reason"), FTCLocale::OrderNote(TEXT("Stop not sent: Select a unit"), Es), FString(TEXT("Detener: Selecciona una unidad")));
	TestEqualSensitive(TEXT("a lobby request"), FTCLocale::OrderNote(TEXT("Start sent: conquest on seelow-heights, 3 seats"), Es), FString(TEXT("Empezar")));
	TestEqualSensitive(TEXT("Add AI: the browser's button"), FTCLocale::OrderNote(TEXT("Add AI sent (room had 2 seats)"), Es), FString(TEXT("+ A")) + TCHAR(0x00F1) + TEXT("adir jugador IA"));
	TestEqualSensitive(TEXT("Not sent: the reason"), FTCLocale::OrderNote(TEXT("Not sent: Select a unit"), Es), FString(TEXT("Selecciona una unidad")));
	// the selection panel with nothing selected (hud.js drawSelection's heading and the Controls sheet's two lines) and
	// the logistics rows' heading (hud.js drawLogistics), drawn through TC_T as columns
	TestEqualSensitive(TEXT("nothing selected row"), FTCLocale::Translate(TEXT("Selection  |  Select a unit or building  |  Select units in a box"), Es),
		FString(TEXT("Selecci")) + TCHAR(0x00F3) + TEXT("n  |  Selecciona una unidad o edificio  |  Selecciona unidades dentro de un recuadro"));
	TestEqualSensitive(TEXT("logistics heading"), FTCLocale::Translate(TEXT("Logistics"), Es), FString(TEXT("Log")) + TCHAR(0x00ED) + TEXT("stica"));
	return true;
}


// These are native/report strings for which the pinned es.js has no key. Keep the English source faithfully.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLocaleHudMissingTest, "ThreeCrossroads.Locale.HudMissingKeys", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCLocaleHudMissingTest::RunTest(const FString& Parameters)
{
	FTCLocale::FScopedLanguage Es(ETCLanguage::Spanish);
	for (const TCHAR* Text : {TEXT("decided when the clock ran out"), TEXT("decided when the last bunker fell"), TEXT("decided when the last Production Building fell"), TEXT("decided when the last defending structure fell"), TEXT("decided when the bridge was taken"), TEXT("(won)"), TEXT("Last match:"), TEXT("You are watching this room."), TEXT("A match is running: joining it...")})
		TestEqual(Text, TC_T(FString(Text)), FString(Text));
	return true;
}

#endif
