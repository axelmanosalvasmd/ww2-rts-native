// Unit abilities: a port of the reference's ability commands, for the facts this client actually receives.
// Ported from the pinned reference (FTCAbilities::ReferencePin):
//   client/main.js        PRIORITY, fKeyType (F picks one type), useAbility (instant abilities send now, aimed ones arm
//                         a targeting click), AIMED, throwAt and the aimed click in the world mousedown (the nearest
//                         squad throws; Shift queues), setAim's hints, aimShape's preview radii, FACTIONS names, facOf;
//   client/hud.js         drawOrders' ability buttons (which types, tip, icon, cooldown or Munitions text, the F label,
//                         the autocast look) and the right-click that toggles autocast;
//   client/availability.js availability({ t: 'ability', unit, queue }), cooldownSeconds, resources;
//   client/autocast.js    createAutocast: toggle (remembered per type, Classic and the other modes apart) and adopt (a
//                         new own unit takes the remembered choice once);
//   client/keys.js        the 'ability' binding (F);
//   shared/sim.js         UNITS[type].ab, AB_MUN, abCost, isConstructionMode, AUTO_FLAG, and command 'ability' and
//                         'autocast' (only to predict the server's answer, never to stop a message).
// The server still validates and decides every use (shared/sim.js command 'ability' and 'autocast'); a refusal comes back
// as { t: 'deny', cmd: 'ability', reason }, which TCOrders::DenySentence words.
//
// Recipient-known facts only. Everything here reads what this seat's snapshots carry: its own units' cooldowns (other
// rows carry 0), the public retreat bit, its own autocast bit (shared/sim.js playerRow), its own Munitions (mun) and
// the snapshot's mode. Nothing is inferred about units, wallets or orders the snapshot does not hold. The carried
// ammunition the server also checks (canFire, logistics) is not predicted.
//
// Selection. Plans read the controller's Selected set and never add to it. Where the browser keeps the order of its
// selection (the units named, the nearest-thrower tie), the Selected set's iteration order stands in for it.
//
// Data. The ability facts are generated from the reference into TCAbilitiesData.inl by
// unreal/Tools/export-ability-table.mjs. Until FTCUnitTable::Reference names the same pin, every availability check
// refuses with a native reason (bNative), so PlanUse, PlanKey and PlanAimClick send nothing and every button is greyed:
// a moved pin cannot silently use stale numbers. The autocast toggle and adoption carry no table facts and still work.
// Interim until the Rust core supplies these through its C ABI.
//
// Not ported: the Spanish texts (client/es.js), sounds (blip), the HUD's look, and the operative (FPS) view, which this
// client does not have (the browser skips autocast adoption while it is on).
#pragma once

#include "CoreMinimal.h"
#include "Client/TCOrders.h"

class FJsonObject;
class FTCMatchState;
class FTCUnitTable;
struct FTCUnit;
struct FKey;

// One unit type's ability (shared/sim.js UNITS[type].ab with an id other than 'none').
struct FTCAbilityDef
{
	FString Type;            // the unit type
	FString UnitName;        // UNITS[type].name (the HUD's name when the faction has none of its own)
	FString Id;              // 'grenade', 'suppress', 'ap', 'smoke', 'barrage', 'satchel' or 'ura'
	FString Name;            // ab.name: 'Grenade', 'HE Shell', 'Petard', ...
	int32 Cd = 0;            // ab.cd: cooldown seconds
	int32 ClassicMun = 0;    // abCost in Classic and World (ab.mun ?? AB_MUN[id] ?? 0); abilities are free elsewhere
	FString Icon;            // client/hud.js button icon ('smokeab' for smoke, else the id)
	bool bAimed = false;     // AIMED: used on a clicked spot (grenade, barrage, satchel)
	// The index in client/main.js PRIORITY: the type gets an Orders button and F may pick it. INDEX_NONE: neither
	// (at the pin, the gunboat's Torpedo and the destroyer's Shore Bombardment, which only autocast and the AI use).
	int32 Priority = INDEX_NONE;
	FString FactionNames[4]; // client/main.js FACTIONS[f].names[type]; empty: that faction has no name of its own
};

// availability()'s answer: ok, or the sentence the browser shows. bNative: the reason is this client's own (a pin
// mismatch, a seat the lobby has not confirmed), not one the browser would ever show.
struct FTCAbilityCheck
{
	bool bOk = false;
	bool bNative = false;
	FString Reason;
};

enum class ETCAbilityCommand : uint8
{
	None,      // nothing goes out (Note says why when the browser would say anything)
	ArmAim,    // nothing is sent: arm the ability aim; the next left click on the ground throws (PlanAimClick)
	Use,       // { t: 'ability', ids }: instant abilities of the ready selected units of one type
	Throw,     // { t: 'ability', ids: [id], x, z, queue }: the nearest able selected unit uses its aimed ability
	Autocast,  // { t: 'autocast', ids, on }
};
const TCHAR* LexToString(ETCAbilityCommand Kind);

// client/keys.js match for the ability chord. F alone is 'ability' in the global context; the recruit and building
// contexts (the Command Card letters) outrank it with card 9, and Shift+F is the hold-fire stance (TCArmyCommands).
enum class ETCAbilityKey : uint8
{
	None,
	Use,       // F: client/main.js actions.ability, useAbility(fKeyType())
};

struct FTCAbilityPlan
{
	ETCAbilityCommand Kind = ETCAbilityCommand::None;
	TSharedPtr<FJsonObject> Message;   // the one message to send; null when nothing goes out
	int32 Units = 0;                   // ids named in the message
	FString Type;                      // the unit type the plan is for (the aim's aimedUnit when ArmAim)
	FString AbilityId;                 // its ability id (the aim's kind when ArmAim)
	bool bAutocastOn = false;          // Autocast: the value sent
	FVector2D At = FVector2D(-1, -1);  // Throw: where the browser puts its marker
	FColor Color = FColor(0xff, 0xa0, 0x30); // client/main.js throwAt marker, 0xffa030
	// The aim after this plan (client/main.js setAim and cancelAim).
	bool bArmAim = false;              // ArmAim: aim AbilityId for Type, showing AimHint, previewing a ring of AimRadius
	bool bCancelAim = false;           // the aimed click was taken: the aim ends (even when no unit could throw)
	bool bKeepAim = false;             // the aimed click was refused (no ground, or availability): the aim stays
	FString AimHint;
	double AimRadius = 0;
	// What the browser shows (its feedback or hint), or for this client, why nothing was sent. Empty when the browser
	// stays silent.
	FString Note;
	bool bNative = false;              // Note is this client's own reason
	// The deny reason the server is expected to answer from what this seat's snapshot shows ('needs', 'cooldown',
	// 'retreating', 'mun', 'blocked'), empty when acceptance is expected. Only a prediction: the plan is sent
	// regardless, as the browser sends it. Carried ammunition ('ammo') and full order queues ('queueFull') are not
	// predicted (the snapshot does not carry them).
	FString PredictedDeny;

	// The same message as an FTCOrderPlan, for ATCPlayerController::SendPlan (Kind None: ETCOrderKind has no entry).
	FTCOrderPlan ToOrderPlan() const;
};

// One ability button of the Orders panel, as client/hud.js drawOrders shows it.
struct FTCAbilityButton
{
	FString Type;            // the unit type it is for (a click: PlanUse; a right-click: PlanAutocast)
	FString AbilityId, Icon;
	FString Tip;             // the title: "Grenade: Grenadiers, click where. 30s cooldown. Right-click: autocast on/off"
	FString Text;            // the value line: "12s" when none is ready, else "15 Mun" when it costs Munitions, else empty
	FString Key;             // "F" on the type F would use, else empty
	bool bAuto = false;      // the 'auto' look: autocast on for every selected unit of the type
	FTCAbilityCheck Check;   // availability({ t: 'ability', unit: Type }); refused buttons are greyed with the reason
};

// client/autocast.js: the player's remembered autocast choice per unit type, kept apart for Classic (where abilities
// cost Munitions) and the other modes, and the own units already brought in line this match. The browser keeps the
// choices in localStorage under TCAbilitiesData::AutocastStorageKey ('ww2-autocast'); Save and Load give the same JSON
// so the caller can keep it wherever this client keeps settings.
class FTCAutocastMemory
{
public:
	FTCAutocastMemory();
	// client/autocast.js toggle: on unless every listed unit already has it on; remembered for the type. Null (nothing
	// sent, nothing remembered) for an empty list.
	TSharedPtr<FJsonObject> Toggle(const FString& Type, const TArray<const FTCUnit*>& List, bool bClassic, bool* bOutOn = nullptr);
	// client/autocast.js adopt over the held rows: an own unit seen for the first time this match gets the remembered
	// choice for its type, once. Up to two messages ({ t: 'autocast', ids, on: true } then the same with on: false).
	// Ids go in ascending order (the server's answer does not depend on it). Nothing is adopted, and no unit is marked
	// seen, until the lobby has confirmed this seat.
	TArray<TSharedRef<FJsonObject>> Adopt(const FTCMatchState& State, bool bClassic);
	// client/autocast.js reset: a new match numbers its units from scratch (choices are kept).
	void Reset() { Fixed.Reset(); }
	// The remembered choice for a type, if any.
	TOptional<bool> Preference(const FString& Type, bool bClassic) const;
	// The choices as the browser stores them ('{"free":{"mg":true},"classic":{"rifle":false}}'), and back. Load keeps
	// what it does not understand, as the browser does; a document that is not a JSON object loads as no choices.
	FString Save() const;
	void Load(const FString& Json);

private:
	TSharedRef<FJsonObject> Prefs;
	TSet<int32> Fixed;
	TSharedPtr<FJsonObject> Bucket(bool bClassic) const;
	TSharedRef<FJsonObject> BucketForWrite(bool bClassic);
};

class FTCAbilities
{
public:
	// The reference commit the ability table was generated from.
	static const TCHAR* const ReferencePin;
	// The type's ability; null when it has none ('none' in the reference) or the type is unknown.
	static const FTCAbilityDef* Find(const FString& Type);
	// Every type with an ability, in the reference's UNIT_TYPES order.
	static const TArray<FTCAbilityDef>& All();
	// client/main.js PRIORITY.
	static const TArray<FString>& Priority();
	// AIMED, setAim's hint for it (with the ". Right-click cancels" every aim gets) and aimShape's preview radius.
	static bool IsAimed(const FString& AbilityId);
	static FString AimHint(const FString& AbilityId);
	static double AimRadius(const FString& AbilityId);
	// client/keys.js match for the ability chord (bCtrl: Ctrl or Command). Context precedence is the caller's: in the
	// recruit or building contexts F is a card letter first, when that card exists.
	static ETCAbilityKey KeyAction(const FKey& Key, bool bShift, bool bCtrl, bool bAlt);
	static const TCHAR* KeyLabel();
	static const TCHAR* AutocastStorageKey();

	// Message hooks, called next to FTCMatchState's. A start forgets the last snapshot and this match's adopted units
	// (client/main.js startGame: autocast.reset()); a snapshot before any start is ignored. Clear when the match ends.
	void ApplyStart(const FJsonObject& M);
	void ApplySnapshot(const FJsonObject& M);
	void Clear();
	bool HasSnapshot() const { return bHasSnapshot; }

	// The snapshot's facts this module reads.
	bool ConstructionMode() const;                 // shared/sim.js isConstructionMode: mode.kind is 'classic' or 'world'
	TOptional<double> Mun() const { return SnapMun; } // this seat's Munitions (Classic and World only)
	int32 Cost(const FString& Type) const;         // shared/sim.js abCost for the type's ability (0 without one)
	int32 FactionOf(int32 Slot) const;             // client/main.js facOf
	FString UnitName(const FString& Type, int32 Slot) const; // client/hud.js name(t, slot)

	// client/availability.js availability({ t: 'ability', unit: Type, queue: bQueue }) for the selection.
	FTCAbilityCheck Check(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FString& Type, bool bQueue = false) const;
	// client/main.js fKeyType: the first PRIORITY type in the selection that availability allows, else the first PRIORITY
	// type in the selection; empty when none.
	FString FKeyType(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const;
	// client/main.js useAbility(Type), for an ability button's click: an aimed ability arms the aim (even on cooldown:
	// a Shift click can queue it), an instant one sends for every ready selected unit of the type.
	FTCAbilityPlan PlanUse(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FString& Type) const;
	// The F key: useAbility(fKeyType()).
	FTCAbilityPlan PlanKey(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const;
	// client/main.js mousedown while an ability aim is armed (the left button; the right button and Esc only cancel):
	// no ground keeps the aim with the 'blocked' sentence; availability (queue: Shift) refused keeps the aim with its
	// reason; otherwise the aim ends and throwAt sends for the selected unit of Type with ability AbilityId nearest
	// Ground (ready, or any with Shift; never a retreating one). The minimap does not throw: with an ability aim armed
	// its left button moves the camera and its right button cancels.
	FTCAbilityPlan PlanAimClick(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FString& Type, const FString& AbilityId, bool bHasGround, FVector2D Ground, bool bShift) const;
	// client/hud.js drawOrders: the ability buttons, in PRIORITY order, for the types in the selection. Empty when the
	// Orders panel does not show (TCArmyCommands::ShowsOrdersPanel).
	TArray<FTCAbilityButton> Buttons(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const;
	// The right-click on an ability button: client/autocast.js toggle over the selected own units of Type, remembered
	// in the bucket client/main.js classicMode names (TCArmyCommands::ClassicLobby). Kind None when there are none.
	FTCAbilityPlan PlanAutocast(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FString& Type);
	// After each snapshot: client/autocast.js adopt (FTCAutocastMemory::Adopt) in the same bucket.
	TArray<TSharedRef<FJsonObject>> AdoptAutocast(const FTCMatchState& State);

	FTCAutocastMemory Autocast;

private:
	bool bStarted = false, bHasSnapshot = false;
	TArray<TOptional<int32>> Factions;
	TOptional<double> SnapMp, SnapMun;
	FString ModeKind;
	TArray<bool> OutSeats;

	FTCAbilityCheck Gate(const FTCMatchState& State, const FTCUnitTable& Table) const;
	FString PredictDeny(const FTCMatchState& State, const FTCUnitTable& Table, const TArray<int32>& Ids, bool bHasSpot, bool bQueue) const;
};
