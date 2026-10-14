// The language option, English or Spanish: a port of the reference's client/i18n.js at FTCLocale::ReferencePin (parity
// CLI-120, CLI-121 and the language part of CLI-124; gap 15 in the parity report). The owner kept the reference's
// Spanish and its key scheme: the dictionary is client/es.js, keyed by the English text the game writes, exported as
// it is to Content/Data/locale-es.json by unreal/Tools/export-locale.mjs (which also fingerprints client/i18n.js, so a
// pin that changes the rules stops the export until they are re-ported here).
//
// The game code keeps writing English. Every text that reaches the screen goes through TC_T (FTCLocale::T): in English
// it is the text itself; in Spanish it is i18n.js spanish() of it. Ported exactly from client/i18n.js:
//   - leading and trailing whitespace (JavaScript's \s) is kept around the translation; a text without an ASCII letter
//     is returned as it is;
//   - the whole text is looked up; else, ending in . ! or ?, the text without that mark, the mark put back after the
//     translation;
//   - else the patterns: a key with {0}..{9} matches any text in each hole (lazily, never across a sentence break: a
//     hole char may not be . ! or ? followed by a space), a hole followed by % matches only digits, '.' and ','
//     (greedily); the patterns are tried longest literal text first (a stable sort, so ties keep es.js order); each
//     hole's text is translated in turn, then put in the Spanish's {n} (an absent hole is empty);
//   - else the texts the game glues together: sentence by sentence (the first . ! or ? followed by a space and a
//     non-space), then around the first ': ' or ' · ', then a trailing ' (hotkey)'; each part translated in turn;
//   - else the text stays English;
//   - the memo: every answer is remembered, and so is the answer as its own translation (the reference's MutationObserver
//     sees its own Spanish come back); past 20,000 entries the memo is cleared. Its consequence is kept: once a Spanish
//     answer equals an English key ('Network' becomes 'Red', after which 'Red' stays 'Red'), that key stays as it is.
//   - case matters everywhere (JavaScript's Map and RegExp); unit model names and place names are simply not in the
//     dictionary, so they stay as they are.
// The language (client/i18n.js lang and setLang): the stored setting 'es' or 'en' (localStorage 'ww2rts-lang'; here
// [ThreeCrossroads] Language= in GameUserSettings.ini, the per-user settings the cover preview, edge scroll and pan
// speed already use), else Spanish when the system language starts with 'es', else English.
// Scenario text by language (CLI-121): SayText is client/alerts.js's say line, ObjectiveText client/objectives.js's
// objective label.
//
// Native (not in the reference):
//   - the HUD's glue: this client draws on one line what the browser shows as separate elements (each translated by
//     the page's observer on its own), so in Spanish such a line is translated element by element:
//       columns  '  |  ', else ' | ', else a run of two or more spaces between two texts, else the first ' [tag]'
//                (a hotkey or badge: 'Recon Flight [Z] 60 MP'), tried after the exact lookup and before the patterns
//                (so '{0} at {1}' cannot match across a gap: 'Red wins in 6:20 at this rate  |  ...' is 'A este ritmo,
//                Rojo gana en 6:20  |  ...', not '... a las this rate ...', and '{0} MP' cannot swallow a key);
//       tails    a trailing cooldown (' 21s') or ' #12' (hud.js: tr(name) + ' #' + id), else an order button's name,
//                ' (key)' and a value starting with a digit ('Unload (Shift+E) 0/7': hud.js orderBtn's title, <kbd>
//                and .val), tried after the reference's glue rules.
//     es.js has no key these change (no double space; its one key with ' | ' is exact), and the reference oracle in
//     locale-es.json still answers the same; locale-es.json's oracle.native holds the expected answers for the
//     native shapes, made by export-locale.mjs from the reference's dictionary with the same rules;
//   - TitleName: an order button's name as the head of its translated tooltip (see below);
//   - locale-es.json's oracle.names: every unit, ability, support, faction look, recruit group and tier name the HUD
//     can show, with the reference module's own Spanish for it. The browser keeps some English (es.js has no entry):
//     'HE Shell', '88 mm AP Round', 'Torpedo', 'Petard', 'Tiger', model names such as 'M4 Sherman'. So does this port;
//   - SetLanguage applies at once (the memo is emptied) where the browser reloads the page; texts a module computed and
//     kept in English are translated when they are next drawn through TC_T, as the browser's page re-renders them;
//   - -TCLang=en or -TCLang=es on the command line overrides the stored setting for that run (capture review);
//   - a run that is not a player's (automation tests, -unattended, commandlets) is English unless -TCLang says
//     otherwise, as the reference's tests stay English whatever the machine (i18n.js consults the browser's language
//     only on a real page). FTCLocale::FScopedLanguage sets the language for a test's scope.
// Not ported: the ?i18n=log miss list, document.title and the DOM observer itself (this client has no DOM: each HUD
// text is wrapped in TC_T where it is measured and drawn; logs and verification switches keep the English).
#pragma once

#include "CoreMinimal.h"

class FJsonObject;
class FJsonValue;

enum class ETCLanguage : uint8
{
	English,  // 'en'
	Spanish,  // 'es'
};

// JavaScript Map semantics for FString keys: case-sensitive (FString's own hash and == ignore case).
template <typename ValueType>
struct TTCCaseSensitiveKeyFuncs : TDefaultMapKeyFuncs<FString, ValueType, false>
{
	using KeyInitType = typename TDefaultMapKeyFuncs<FString, ValueType, false>::KeyInitType;
	static bool Matches(KeyInitType A, KeyInitType B) { return A.Equals(B, ESearchCase::CaseSensitive); }
	static uint32 GetKeyHash(KeyInitType Key) { return FCrc::StrCrc32(*Key); }
};
using FTCExactStringMap = TMap<FString, FString, FDefaultSetAllocator, TTCCaseSensitiveKeyFuncs<FString>>;

// The dictionary and the memo: i18n.js's exact map, pattern list and memo, and spanish().
class FTCLocaleTable
{
public:
	// i18n.js: if (memo.size > 20000) memo.clear()
	static constexpr int32 MemoLimit = 20000;

	// Reads locale-es.json's text (export-locale.mjs): its reference and its [English, Spanish] entries in es.js order.
	bool Load(const FString& JsonText, FString& OutError);
	bool LoadFile(const FString& Path, FString& OutError);
	bool IsLoaded() const { return bLoaded; }
	const FString& Reference() const { return Ref; }
	int32 NumExact() const { return Exact.Num(); }
	int32 NumPatterns() const { return Patterns.Num(); }
	int32 MemoSize() const { return Memo.Num(); }

	// i18n.js spanish(s): the Spanish of one text, its surrounding whitespace kept. Remembers its answers.
	FString Spanish(const FString& S);
	void ClearMemo() { Memo.Reset(); }

private:
	enum class EPiece : uint8 { Literal, Hole, Percent };
	struct FPiece { EPiece Kind = EPiece::Literal; FString Text; };
	struct FPattern
	{
		TArray<FPiece> Pieces;   // the key as literal runs and holes ('{n}%' is a Percent hole, then a literal from '%')
		TArray<int32> Order;     // the hole numbers in the order they appear in the key
		FString Es;              // the Spanish, with {n} where the translated holes go
		int32 Lit = 0;           // the key's length without its {n}: the more literal text, the earlier it is tried
	};
	bool MatchFrom(const FPattern& P, const FString& S, int32 Piece, int32 Pos, TArray<FString>& Caps) const;
	FString Core(const FString& S);
	// native glue (see the file comment): the HUD's columns and tags, and a trailing cooldown or ' #id'
	bool NativeColumns(const FString& S, FString& Out);
	bool NativeTail(const FString& S, FString& Out);

	bool bLoaded = false;
	FString Ref;
	FTCExactStringMap Exact;
	TArray<FPattern> Patterns;
	FTCExactStringMap Memo;
};

// What the language picker shows (client/index.html #langSel, filled by i18n.js): its title and aria-label, and the two
// options. The texts are the page's; the HUD draws them through TC_T as the page's observer translates them (in Spanish
// the title reads 'Idioma' and the English option 'Inglés').
struct FTCLanguagePicker
{
	FString Title;                 // "Language"
	struct FOption { ETCLanguage Language; FString Code; FString Label; };
	TArray<FOption> Options;       // en "English", es "Español"
	ETCLanguage Selected = ETCLanguage::English;
};

class FTCLocale
{
public:
	// The reference commit client/i18n.js and client/es.js were ported and exported from.
	static const TCHAR* const ReferencePin;
	// Where the choice is stored (GGameUserSettingsIni): [ThreeCrossroads] Language=es|en.
	static const TCHAR* const SettingsSection;
	static const TCHAR* const SettingsKey;

	// The language in use (resolved on first use, see the file comment).
	static ETCLanguage Language();
	static const TCHAR* Code(ETCLanguage L) { return L == ETCLanguage::Spanish ? TEXT("es") : TEXT("en"); }
	static const TCHAR* Code() { return Code(Language()); }
	// 'es' or 'en' exactly (i18n.js: stored === 'es' || stored === 'en').
	static bool ParseCode(const FString& Code, ETCLanguage& Out);
	// i18n.js lang: the stored 'es' or 'en', else on a real page Spanish when the system language starts with 'es',
	// else English.
	static ETCLanguage Resolve(const FString& Stored, bool bPage, const FString& SystemLanguage);
	// i18n.js setLang: stores the choice in the user settings and applies it (native: at once, no reload).
	static void SetLanguage(ETCLanguage L);
	// The stored choice in an ini file (GGameUserSettingsIni for the game), empty when there is none.
	static FString ReadStored(const FString& IniFile);
	// Stores the choice in an ini file and writes the file out (through the config system when it saves that file, as
	// it does GGameUserSettingsIni; directly otherwise). False when the file could not be written.
	static bool WriteStored(ETCLanguage L, const FString& IniFile);

	// i18n.js t(s): the text in the language in use.
	static FString T(const FString& S);
	static FString T(const TCHAR* S) { return T(FString(S)); }
	// The text in the given language (t() with lang fixed), through the shared table and memo.
	static FString Translate(const FString& S, ETCLanguage L);
	// i18n.js spanish(s), whatever the language in use (client/world-authoring-error.js uses it directly).
	static FString Spanish(const FString& S);
	// Native (slice 12): the name an order button's tooltip starts with, as the page shows it. The browser's button has
	// no name on its face (hud.js orderBtn: an icon, the key in <kbd>, the value in .val); its name is the head of its
	// title, which the page's observer translates whole: 'Mount up (Shift+Q): each selected squad climbs ...' reads
	// 'Montar (Shift+Q): cada pelotón ...'. This client draws that name as the button's text, so it takes it the same
	// way: the title translated (t()), then hud.js's own aria-label rule on it, tip.split(/[:(]/)[0].trim(). In English
	// it is the aria-label itself ('Mount up'). Give it the English title (the one the HUD shows on hover).
	static FString TitleName(const FString& Title);
	static FString TitleName(const FString& Title, ETCLanguage L);
	// The shared dictionary (Content/Data/locale-es.json), loaded on first use. When it cannot be read every text stays
	// English and LoadError says why.
	static bool IsLoaded();
	static FString LoadError();
	static FString DataPath();

	// client/alerts.js: a 'say' shot's line, sh.localized?.[lang] ?? sh.text (the page's lang: 'es' in Spanish, else
	// 'en'). False when neither gives a text. The alert feed then draws it through TC_T like every alert.
	static bool SayText(const FJsonObject& Shot, ETCLanguage L, FString& Out);
	static bool SayText(const FJsonObject& Shot, FString& Out) { return SayText(Shot, Language(), Out); }
	// client/objectives.js: a scenario objective's label, typeof text === 'string' ? text : text?.[lang] ?? text?.en ?? ''.
	static FString ObjectiveText(const TSharedPtr<FJsonValue>& Text, ETCLanguage L);
	static FString ObjectiveText(const TSharedPtr<FJsonValue>& Text) { return ObjectiveText(Text, Language()); }

	// The picker's title and options (texts in English: draw them through TC_T).
	static FTCLanguagePicker Picker();

	// Sets the language for a scope (tests), then puts back what was in use. Nothing is stored.
	class FScopedLanguage
	{
	public:
		explicit FScopedLanguage(ETCLanguage L);
		~FScopedLanguage();
	private:
		ETCLanguage Previous;
		bool bHadOverride;
	};
};

// The HUD's call: TC_T(TEXT("Save match")) or TC_T(SomeFString).
#define TC_T(Text) FTCLocale::T(Text)
