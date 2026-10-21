#include "Client/TCAbilities.h"
#include "Client/TCArmyCommands.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "Policies/CondensedJsonPrintPolicy.h"
#include "InputCoreTypes.h"
#include "Algo/StableSort.h"
#include <cmath>
#include <limits>

#include "Client/TCAbilitiesData.inl"

const TCHAR* const FTCAbilities::ReferencePin = TCAbilitiesData::ReferencePin;

// KeyAction matches EKeys::F; a pin that moves the binding must be ported here.
static_assert(TCAbilitiesData::KeyLetter == TEXT('F'), "client/keys.js moved the 'ability' binding: port it into FTCAbilities::KeyAction");

const TCHAR* LexToString(ETCAbilityCommand Kind)
{
	switch (Kind)
	{
	case ETCAbilityCommand::ArmAim: return TEXT("arm-ability");
	case ETCAbilityCommand::Use: return TEXT("ability");
	case ETCAbilityCommand::Throw: return TEXT("ability-at");
	case ETCAbilityCommand::Autocast: return TEXT("autocast");
	default: return TEXT("none");
	}
}

FTCOrderPlan FTCAbilityPlan::ToOrderPlan() const
{
	FTCOrderPlan Plan;
	Plan.Kind = ETCOrderKind::None;
	Plan.Message = Message;
	Plan.Units = Units;
	Plan.At = At;
	Plan.Color = Color;
	Plan.Why = Note;
	return Plan;
}

namespace
{
	constexpr int32 AbilityRetreatingFlag = TCAbilitiesData::RetreatingFlag;
	constexpr int32 AbilityAutoFlag = TCAbilitiesData::AutoFlag;

	struct FAbilityTable
	{
		TArray<FTCAbilityDef> Defs;
		TMap<FString, int32> Index;
		TArray<FString> Priority;
		FAbilityTable()
		{
			for (const TCHAR* T : TCAbilitiesData::Priority) Priority.Add(T);
			for (const TCAbilitiesData::FAbilityRow& R : TCAbilitiesData::Abilities)
			{
				FTCAbilityDef D;
				D.Type = R.Type; D.UnitName = R.UnitName; D.Id = R.Id; D.Name = R.Name; D.Cd = R.Cd; D.ClassicMun = R.ClassicMun; D.Icon = R.Icon;
				for (const TCAbilitiesData::FAimRow& A : TCAbilitiesData::Aimed) D.bAimed |= D.Id == A.Id;
				D.Priority = Priority.IndexOfByKey(D.Type);
				for (int32 F = 0; F < 4; F++) D.FactionNames[F] = R.FactionNames[F];
				Index.Add(D.Type, Defs.Num());
				Defs.Add(MoveTemp(D));
			}
		}
	};
	const FAbilityTable& AbilityTableData() { static const FAbilityTable T; return T; }

	const TCAbilitiesData::FAimRow* AbilityAimRow(const FString& Id)
	{
		for (const TCAbilitiesData::FAimRow& A : TCAbilitiesData::Aimed) if (Id == A.Id) return &A;
		return nullptr;
	}

	bool AbilityNum(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return FMath::IsFinite(Out);
	}

	TArray<TSharedPtr<FJsonValue>> AbilityIdArray(const TArray<int32>& Ids)
	{
		TArray<TSharedPtr<FJsonValue>> Out;
		Out.Reserve(Ids.Num());
		for (int32 Id : Ids) Out.Add(MakeShared<FJsonValueNumber>(Id));
		return Out;
	}

	// V8's Math.hypot (a scaled, compensated sum), so the nearest-thrower order breaks near ties as the browser does
	double AbilityHypot(double A, double B)
	{
		A = std::fabs(A); B = std::fabs(B);
		if (FMath::IsNaN(A) || FMath::IsNaN(B)) return (A == HUGE_VAL || B == HUGE_VAL) ? HUGE_VAL : std::numeric_limits<double>::quiet_NaN();
		const double Max = FMath::Max(A, B);
		if (Max == HUGE_VAL) return HUGE_VAL;
		if (Max == 0) return 0;
		double Sum = 0, Compensation = 0;
		for (const double V : { A, B })
		{
			const double N = V / Max;
			const double Summand = N * N - Compensation;
			const double Preliminary = Sum + Summand;
			Compensation = (Preliminary - Sum) - Summand;
			Sum = Preliminary;
		}
		return std::sqrt(Sum) * Max;
	}

	// client/availability.js cooldownSeconds: whole seconds, rounded up (the rows already carry Math.ceil of the cd)
	int32 AbilityCooldownSeconds(int32 Cd) { return FMath::Max(0, Cd); }

	FTCAbilityCheck AbilityYes() { FTCAbilityCheck C; C.bOk = true; return C; }
	FTCAbilityCheck AbilityNo(const FString& Reason, bool bNative = false) { FTCAbilityCheck C; C.Reason = Reason; C.bNative = bNative; return C; }

	TSharedRef<FJsonObject> AbilityAutocastMessage(const TArray<int32>& Ids, bool bOn)
	{
		TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
		M->SetStringField(TEXT("t"), TEXT("autocast"));
		M->SetArrayField(TEXT("ids"), AbilityIdArray(Ids));
		M->SetBoolField(TEXT("on"), bOn);
		return M;
	}

	// shared/sim.js command's mine() as this seat sees it (TCArmyCommands::TakesSelectionOrder). Forced logistics only
	// concerns convoy trucks, which have no ability.
	bool AbilityMine(const FTCMatchState& State, const FTCUnitTable& Tbl, const FTCUnit* U, bool& bKnown)
	{
		bKnown = true;
		if (!U) return false;
		const FTCUnitDef* D = Tbl.Find(U->Type);
		if (!D) { bKnown = false; return false; }
		return TCArmyCommands::TakesSelectionOrder(State, *U, D);
	}
}

// ---------- the table ----------

const FTCAbilityDef* FTCAbilities::Find(const FString& Type)
{
	const int32* I = AbilityTableData().Index.Find(Type);
	return I ? &AbilityTableData().Defs[*I] : nullptr;
}

const TArray<FTCAbilityDef>& FTCAbilities::All() { return AbilityTableData().Defs; }
const TArray<FString>& FTCAbilities::Priority() { return AbilityTableData().Priority; }
bool FTCAbilities::IsAimed(const FString& AbilityId) { return AbilityAimRow(AbilityId) != nullptr; }

FString FTCAbilities::AimHint(const FString& AbilityId)
{
	// client/main.js setAim: the kind's hint (anything else: 'Click to set the center'), then '. Right-click cancels'
	const TCAbilitiesData::FAimRow* A = AbilityAimRow(AbilityId);
	return FString(A ? A->Hint : TEXT("Click to set the center")) + TCAbilitiesData::AimHintSuffix;
}

double FTCAbilities::AimRadius(const FString& AbilityId)
{
	// client/main.js aimShape: the aimed abilities' rings (any other kind there is drawn otherwise)
	const TCAbilitiesData::FAimRow* A = AbilityAimRow(AbilityId);
	return A ? A->Radius : 0;
}

ETCAbilityKey FTCAbilities::KeyAction(const FKey& Key, bool bShift, bool bCtrl, bool bAlt)
{
	// client/keys.js match: code, shift, ctrl (or meta) and alt must all equal the binding's
	return Key == EKeys::F && !bShift && !bCtrl && !bAlt ? ETCAbilityKey::Use : ETCAbilityKey::None;
}

const TCHAR* FTCAbilities::KeyLabel() { return TCAbilitiesData::KeyLabel; }
const TCHAR* FTCAbilities::AutocastStorageKey() { return TCAbilitiesData::AutocastStorageKey; }

// ---------- the snapshot ----------

void FTCAbilities::Clear()
{
	bStarted = bHasSnapshot = false;
	Factions.Reset();
	SnapMp.Reset(); SnapMun.Reset();
	ModeKind.Reset();
	OutSeats.Reset();
}

void FTCAbilities::ApplyStart(const FJsonObject& M)
{
	// client/main.js startGame: autocast.reset(), factions = m.factions ?? [], lastSnap = null
	Clear();
	Autocast.Reset();
	bStarted = true;
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	if (M.TryGetArrayField(TEXT("factions"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			double F;
			Factions.Add(AbilityNum(V, F) ? TOptional<int32>((int32)F) : TOptional<int32>());
		}
	}
}

void FTCAbilities::ApplySnapshot(const FJsonObject& M)
{
	if (!bStarted) return;
	bHasSnapshot = true;
	// shared/sim.js snapshotFor sends these whole in every snapshot: mp always, mun in Classic and World only
	double D;
	SnapMp.Reset(); if (AbilityNum(M.TryGetField(TEXT("mp")), D)) SnapMp = D;
	SnapMun.Reset(); if (AbilityNum(M.TryGetField(TEXT("mun")), D)) SnapMun = D;
	ModeKind.Reset();
	const TSharedPtr<FJsonObject>* Mode;
	if (M.TryGetObjectField(TEXT("mode"), Mode) && Mode->IsValid()) (*Mode)->TryGetStringField(TEXT("kind"), ModeKind);
	OutSeats.Reset();
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	if (M.TryGetArrayField(TEXT("out"), Arr)) for (const TSharedPtr<FJsonValue>& V : *Arr) OutSeats.Add(V.IsValid() && V->Type == EJson::Boolean && V->AsBool());
}

bool FTCAbilities::ConstructionMode() const { return ModeKind == TEXT("classic") || ModeKind == TEXT("world"); }

int32 FTCAbilities::Cost(const FString& Type) const
{
	// shared/sim.js abCost: isConstructionMode(snapshot) ? ab.mun ?? AB_MUN[ab.id] ?? 0 : 0
	const FTCAbilityDef* D = Find(Type);
	return D && ConstructionMode() ? D->ClassicMun : 0;
}

int32 FTCAbilities::FactionOf(int32 Slot) const
{
	// client/main.js facOf: factions[slot] ?? (slot < 0 ? 0 : slot % 3)
	if (Factions.IsValidIndex(Slot) && Factions[Slot].IsSet()) return Factions[Slot].GetValue();
	return Slot < 0 ? 0 : Slot % 3;
}

FString FTCAbilities::UnitName(const FString& Type, int32 Slot) const
{
	// client/hud.js name: look(slot).names[t] ?? UNITS[t].name. A faction outside FACTIONS (the browser would throw)
	// falls back to the reference name.
	const FTCAbilityDef* D = Find(Type);
	if (!D) return Type;
	const int32 F = FactionOf(Slot);
	return F >= 0 && F < 4 && !D->FactionNames[F].IsEmpty() ? D->FactionNames[F] : D->UnitName;
}

// ---------- availability ----------

FTCAbilityCheck FTCAbilities::Gate(const FTCMatchState& State, const FTCUnitTable& Tbl) const
{
	if (Tbl.Reference != ReferencePin)
	{
		return AbilityNo(FString::Printf(TEXT("Ability data is for reference %s, the unit table is for %s"), ReferencePin,
			Tbl.Reference.IsEmpty() ? TEXT("an unknown reference") : *Tbl.Reference), true);
	}
	// client/availability.js: if (!s) 'Waiting for the army'; if (action.watching || s.out?.[action.slot]) 'You are spectating'
	if (!bHasSnapshot || !State.bHasMatch) return AbilityNo(TEXT("Waiting for the army"));
	if (State.bSpectator || (OutSeats.IsValidIndex(State.You) && OutSeats[State.You])) return AbilityNo(TEXT("You are spectating"));
	if (!State.SeatConfirmed()) return AbilityNo(TEXT("Waiting for the lobby to confirm this seat"), true);
	return AbilityYes();
}

FTCAbilityCheck FTCAbilities::Check(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected, const FString& Type, bool bQueue) const
{
	const FTCAbilityCheck G = Gate(State, Tbl);
	if (!G.bOk) return G;
	// crew: the selected own units of the type; active: those not retreating
	int32 Crew = 0, Active = 0, MinCd = MAX_int32;
	bool bAllCooling = true;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (!U || U->Owner != State.You || U->Type != Type) continue;
		Crew++;
		if (U->Flags & AbilityRetreatingFlag) continue;
		Active++;
		bAllCooling &= U->Cd > 0;
		MinCd = FMath::Min(MinCd, U->Cd);
	}
	if (!Crew) return AbilityNo(TEXT("Select a squad"));
	if (!Active) return AbilityNo(TCOrders::DenySentence(TEXT("retreating"), TEXT("ability")));
	const FTCAbilityDef* D = Find(Type);
	// a queued aimed ability is checked (cooldown, Munitions) when its turn comes
	if (bQueue && D && D->bAimed) return AbilityYes();
	if (bAllCooling) return AbilityNo(FString::Printf(TEXT("Cooldown %d s"), AbilityCooldownSeconds(MinCd)));
	// resources(s, 0, 0, abCost(s, ab)): s.mp < 0, then the Munitions (fuel 0 never refuses)
	if (SnapMp.IsSet() && SnapMp.GetValue() < 0) return AbilityNo(TEXT("Needs 0 MP"));
	const int32 Price = Cost(Type);
	if (Price && !(SnapMun.IsSet() && SnapMun.GetValue() >= Price)) return AbilityNo(FString::Printf(TEXT("Needs %d munitions"), Price));
	return AbilityYes();
}

FString FTCAbilities::FKeyType(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected) const
{
	// sel: every held selected unit (any owner), as client/main.js reads them from its units map
	TSet<FString> Types;
	for (int32 Id : Selected) if (const FTCUnit* U = State.Units.Find(Id)) Types.Add(U->Type);
	for (const FString& T : Priority()) if (Types.Contains(T) && Check(State, Tbl, Selected, T).bOk) return T;
	for (const FString& T : Priority()) if (Types.Contains(T)) return T;
	return FString();
}

// ---------- plans ----------

FString FTCAbilities::PredictDeny(const FTCMatchState& State, const FTCUnitTable& Tbl, const TArray<int32>& Ids, bool bHasSpot, bool bQueue) const
{
	// shared/sim.js command 'ability' over what this seat's snapshot shows: the reason of the last unit refused, unless
	// one was used. The carried-ammunition check (canFire) and a full order queue cannot be seen and are skipped.
	FString Reason = TEXT("needs");
	bool bUsed = false;
	double Wallet = SnapMun.IsSet() ? SnapMun.GetValue() : std::numeric_limits<double>::quiet_NaN();
	for (int32 Id : Ids)
	{
		const FTCUnit* U = State.Units.Find(Id);
		bool bKnown = true;
		const bool bMine = AbilityMine(State, Tbl, U, bKnown);
		if (!bKnown) return FString();
		const FTCAbilityDef* D = bMine ? Find(U->Type) : nullptr;
		const bool bAimed = D && D->bAimed, bRetreating = bMine && (U->Flags & AbilityRetreatingFlag);
		if (bQueue && bMine && !bRetreating && bHasSpot && bAimed) { bUsed = true; continue; }
		const int32 Price = bMine ? Cost(U->Type) : 0;
		if (!bMine || U->Cd > 0 || bRetreating || !(Wallet >= Price || !Price))
		{
			Reason = !bMine ? TEXT("needs") : U->Cd > 0 ? TEXT("cooldown") : bRetreating ? TEXT("retreating") : TEXT("mun");
			continue;
		}
		if (bAimed)
		{
			if (!bHasSpot) { Reason = TEXT("blocked"); continue; }
			bUsed = true; // paid when it is thrown
			continue;
		}
		// payAb, then the instant abilities; a unit without one ('none') changes nothing
		if (Price) Wallet -= Price;
		if (D) bUsed = true;
	}
	return bUsed ? FString() : Reason;
}

FTCAbilityPlan FTCAbilities::PlanUse(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected, const FString& Type) const
{
	FTCAbilityPlan Plan;
	Plan.Type = Type;
	// client/main.js useAbility: if (!type) return
	if (Type.IsEmpty()) return Plan;
	const FTCAbilityDef* D = Find(Type);
	if (!D)
	{
		Plan.Note = FString::Printf(TEXT("Ability: %s has none"), *Type);
		Plan.bNative = true;
		return Plan;
	}
	Plan.AbilityId = D->Id;
	// explainUnavailable(available({ t: 'ability', unit: type, queue: !!AIMED[id] }))
	const FTCAbilityCheck C = Check(State, Tbl, Selected, Type, D->bAimed);
	if (!C.bOk) { Plan.Note = C.Reason; Plan.bNative = C.bNative; return Plan; }
	// ready: the selected units of the type that are not retreating and, for an instant ability, off cooldown
	TArray<int32> Ready;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (U && U->Type == Type && (D->bAimed || U->Cd == 0) && !(U->Flags & AbilityRetreatingFlag)) Ready.Add(Id);
	}
	if (!Ready.Num()) return Plan;
	if (D->bAimed)
	{
		// setAim(id, type): the next left click throws (the gate above already refused an observer)
		Plan.Kind = ETCAbilityCommand::ArmAim;
		Plan.bArmAim = true;
		Plan.AimHint = AimHint(D->Id);
		Plan.AimRadius = AimRadius(D->Id);
		Plan.Note = Plan.AimHint;
		return Plan;
	}
	// sendCmd({ t: 'ability', ids: ready.map(v => v.id) })
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("ability"));
	M->SetArrayField(TEXT("ids"), AbilityIdArray(Ready));
	Plan.Kind = ETCAbilityCommand::Use;
	Plan.Message = M;
	Plan.Units = Ready.Num();
	Plan.PredictedDeny = PredictDeny(State, Tbl, Ready, false, false);
	return Plan;
}

FTCAbilityPlan FTCAbilities::PlanKey(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected) const
{
	return PlanUse(State, Tbl, Selected, FKeyType(State, Tbl, Selected));
}

FTCAbilityPlan FTCAbilities::PlanAimClick(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected, const FString& Type, const FString& AbilityId, bool bHasGround, FVector2D Ground, bool bShift) const
{
	FTCAbilityPlan Plan;
	Plan.Type = Type;
	Plan.AbilityId = AbilityId;
	// client/main.js mousedown while targeting: if (!g) { feedback.show(denySentence('blocked')); return; }
	if (!bHasGround)
	{
		Plan.Note = TCOrders::DenySentence(TEXT("blocked"), FString());
		Plan.bKeepAim = true;
		return Plan;
	}
	// if (AIMED[kind]) { if (explainUnavailable(available({ t: 'ability', unit: type, queue: e.shiftKey }))) return; ...
	const FTCAbilityCheck C = Check(State, Tbl, Selected, Type, bShift);
	if (!C.bOk) { Plan.Note = C.Reason; Plan.bNative = C.bNative; Plan.bKeepAim = true; return Plan; }
	// ... cancelAim(); throwAt(g, kind, type, e.shiftKey)
	Plan.bCancelAim = true;
	// throwAt: the selected units of the type with this ability, ready (or any with Shift), not retreating; the nearest
	// to the click (a stable sort keeps the selection's order on a tie)
	TArray<const FTCUnit*> Who;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		const FTCAbilityDef* D = U ? Find(U->Type) : nullptr;
		if (U && U->Type == Type && D && D->Id == AbilityId && (bShift || U->Cd == 0) && !(U->Flags & AbilityRetreatingFlag)) Who.Add(U);
	}
	if (!Who.Num()) return Plan;
	Algo::StableSort(Who, [&Ground](const FTCUnit* A, const FTCUnit* B)
	{
		return AbilityHypot(A->X - Ground.X, A->Z - Ground.Y) < AbilityHypot(B->X - Ground.X, B->Z - Ground.Y);
	});
	// sendCmd({ t: 'ability', ids: [who[0].id], x: g.x, z: g.z, queue }); marker(g.x, g.z, 0xffa030)
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("ability"));
	M->SetArrayField(TEXT("ids"), AbilityIdArray({ Who[0]->Id }));
	M->SetNumberField(TEXT("x"), Ground.X);
	M->SetNumberField(TEXT("z"), Ground.Y);
	M->SetBoolField(TEXT("queue"), bShift);
	Plan.Kind = ETCAbilityCommand::Throw;
	Plan.Message = M;
	Plan.Units = 1;
	Plan.At = Ground;
	Plan.Color = FColor((uint8)((TCAbilitiesData::ThrowMarker >> 16) & 0xff), (uint8)((TCAbilitiesData::ThrowMarker >> 8) & 0xff), (uint8)(TCAbilitiesData::ThrowMarker & 0xff));
	Plan.PredictedDeny = PredictDeny(State, Tbl, { Who[0]->Id }, true, bShift);
	return Plan;
}

TArray<FTCAbilityButton> FTCAbilities::Buttons(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected) const
{
	TArray<FTCAbilityButton> Out;
	if (!TCArmyCommands::ShowsOrdersPanel(State, Tbl, Selected)) return Out;
	// client/hud.js drawOrders over sel (every held selected unit): types = PRIORITY.filter(t => sel.some(v => v.type === t))
	TArray<const FTCUnit*> Sel;
	for (int32 Id : Selected) if (const FTCUnit* U = State.Units.Find(Id)) Sel.Add(U);
	const FString FType = FKeyType(State, Tbl, Selected);
	for (const FString& T : Priority())
	{
		const FTCAbilityDef* D = Find(T);
		TArray<const FTCUnit*> OfType;
		for (const FTCUnit* U : Sel) if (U->Type == T) OfType.Add(U);
		if (!D || !OfType.Num()) continue;
		FTCAbilityButton B;
		B.Type = T;
		B.AbilityId = D->Id;
		B.Icon = D->Icon;
		// `${ab.name}: ${name(t)}${AIMED.has(ab.id) ? ', click where' : ''}. ${ab.cd}s cooldown. Right-click: autocast on/off`
		B.Tip = FString::Printf(TEXT("%s: %s%s. %ds cooldown. Right-click: autocast on/off"), *D->Name, *UnitName(T, State.You), D->bAimed ? TEXT(", click where") : TEXT(""), D->Cd);
		// us: the ones not retreating, when there are any (the squads the reason sentence counts), else all of them
		TArray<const FTCUnit*> Us;
		for (const FTCUnit* U : OfType) if (!(U->Flags & AbilityRetreatingFlag)) Us.Add(U);
		if (!Us.Num()) Us = OfType;
		int32 Ready = 0, MinCd = MAX_int32;
		for (const FTCUnit* U : Us) { Ready += U->Cd == 0; MinCd = FMath::Min(MinCd, U->Cd); }
		const int32 Price = Cost(T);
		B.Text = !Ready ? FString::Printf(TEXT("%ds"), AbilityCooldownSeconds(MinCd)) : Price ? FString::Printf(TEXT("%d Mun"), Price) : FString();
		B.Check = Check(State, Tbl, Selected, T);
		B.Key = T == FType ? FString(KeyLabel()) : FString();
		B.bAuto = true;
		for (const FTCUnit* U : OfType) B.bAuto &= (U->Flags & AbilityAutoFlag) != 0;
		Out.Add(MoveTemp(B));
	}
	return Out;
}

FTCAbilityPlan FTCAbilities::PlanAutocast(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected, const FString& Type)
{
	FTCAbilityPlan Plan;
	Plan.Type = Type;
	if (const FTCAbilityDef* D = Find(Type)) Plan.AbilityId = D->Id;
	// client/main.js actions.autocast: the selected units of the type that are this seat's
	TArray<const FTCUnit*> List;
	TArray<int32> Ids;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (U && U->Type == Type && State.IsOwn(*U)) { List.Add(U); Ids.Add(Id); }
	}
	bool bOn = false;
	const TSharedPtr<FJsonObject> M = Autocast.Toggle(Type, List, TCArmyCommands::ClassicLobby(State), &bOn);
	if (!M.IsValid()) return Plan;
	Plan.Kind = ETCAbilityCommand::Autocast;
	Plan.Message = M;
	Plan.Units = Ids.Num();
	Plan.bAutocastOn = bOn;
	// shared/sim.js command 'autocast': list = ids.map(mine).filter(u => u && UNITS[u.type].ab.id !== 'none'); none: 'needs'
	bool bAny = false;
	for (const FTCUnit* U : List)
	{
		bool bKnown = true;
		if (AbilityMine(State, Tbl, U, bKnown) && Find(U->Type)) bAny = true;
		if (!bKnown) { bAny = true; break; } // cannot tell: no prediction
	}
	if (!bAny) Plan.PredictedDeny = TEXT("needs");
	return Plan;
}

TArray<TSharedRef<FJsonObject>> FTCAbilities::AdoptAutocast(const FTCMatchState& State)
{
	// client/main.js applySnapshot: if (!operative) autocast.adopt(s.units, me, classicMode())
	return Autocast.Adopt(State, TCArmyCommands::ClassicLobby(State));
}

// ---------- remembered autocast choices (client/autocast.js) ----------

FTCAutocastMemory::FTCAutocastMemory() : Prefs(MakeShared<FJsonObject>()) {}

TSharedPtr<FJsonObject> FTCAutocastMemory::Bucket(bool bClassic) const
{
	const TSharedPtr<FJsonValue> V = Prefs->TryGetField(bClassic ? TEXT("classic") : TEXT("free"));
	const TSharedPtr<FJsonObject>* O = nullptr;
	if (V.IsValid() && V->Type == EJson::Object && V->TryGetObject(O) && O && O->IsValid()) return *O;
	return nullptr;
}

TSharedRef<FJsonObject> FTCAutocastMemory::BucketForWrite(bool bClassic)
{
	// prefs[classic ? 'classic' : 'free'] ??= {} (a stored value that is not an object is replaced; the browser would
	// throw on writing to it)
	if (const TSharedPtr<FJsonObject> B = Bucket(bClassic)) return B.ToSharedRef();
	TSharedRef<FJsonObject> B = MakeShared<FJsonObject>();
	const TSharedPtr<FJsonObject> Field = B;
	Prefs->SetObjectField(bClassic ? TEXT("classic") : TEXT("free"), Field);
	return B;
}

TOptional<bool> FTCAutocastMemory::Preference(const FString& Type, bool bClassic) const
{
	const TSharedPtr<FJsonObject> B = Bucket(bClassic);
	if (!B.IsValid()) return TOptional<bool>();
	const TSharedPtr<FJsonValue> V = B->TryGetField(Type);
	if (V.IsValid() && V->Type == EJson::Boolean) return TOptional<bool>(V->AsBool());
	return TOptional<bool>();
}

TSharedPtr<FJsonObject> FTCAutocastMemory::Toggle(const FString& Type, const TArray<const FTCUnit*>& List, bool bClassic, bool* bOutOn)
{
	// toggle(type, list, classic): if (!list.length) return null; on = !list.every(v => v.flags & AUTO_FLAG)
	if (!List.Num()) return nullptr;
	bool bAll = true;
	TArray<int32> Ids;
	for (const FTCUnit* U : List) { bAll &= (U->Flags & AbilityAutoFlag) != 0; Ids.Add(U->Id); }
	const bool bOn = !bAll;
	BucketForWrite(bClassic)->SetBoolField(Type, bOn);
	if (bOutOn) *bOutOn = bOn;
	return AbilityAutocastMessage(Ids, bOn);
}

TArray<TSharedRef<FJsonObject>> FTCAutocastMemory::Adopt(const FTCMatchState& State, bool bClassic)
{
	TArray<TSharedRef<FJsonObject>> Out;
	if (!State.SeatConfirmed()) return Out;
	const TSharedRef<FJsonObject> Want = BucketForWrite(bClassic);
	TArray<int32> Ids;
	State.Units.GetKeys(Ids);
	Ids.Sort();
	TArray<int32> On, Off;
	for (int32 Id : Ids)
	{
		const FTCUnit& U = State.Units.FindChecked(Id);
		if (U.Owner != State.You || Fixed.Contains(Id)) continue;
		Fixed.Add(Id);
		// typeof want[type] === 'boolean' && !!(row[flags] & AUTO_FLAG) !== want[type]
		const TSharedPtr<FJsonValue> V = Want->TryGetField(U.Type);
		if (!V.IsValid() || V->Type != EJson::Boolean) continue;
		const bool bWant = V->AsBool();
		if (((U.Flags & AbilityAutoFlag) != 0) != bWant) (bWant ? On : Off).Add(Id);
	}
	if (On.Num()) Out.Add(AbilityAutocastMessage(On, true));
	if (Off.Num()) Out.Add(AbilityAutocastMessage(Off, false));
	return Out;
}

FString FTCAutocastMemory::Save() const
{
	FString Out;
	const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
	FJsonSerializer::Serialize(Prefs, Writer);
	return Out;
}

void FTCAutocastMemory::Load(const FString& Json)
{
	// JSON.parse(storage.getItem(KEY) ?? '{}') || {}, with a bad document read as {}
	TSharedPtr<FJsonObject> O;
	if (!Json.IsEmpty() && FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Json), O) && O.IsValid()) Prefs = O.ToSharedRef();
	else Prefs = MakeShared<FJsonObject>();
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Client/TCAbilitiesCases.inl"

namespace
{
	TSharedPtr<FJsonObject> AbilityTestParse(const FString& Src)
	{
		TSharedPtr<FJsonObject> Obj;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Src), Obj);
		return Obj;
	}

	FString AbilityTestText(const TSharedPtr<FJsonObject>& O)
	{
		if (!O.IsValid()) return TEXT("null");
		FString Out;
		const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
		FJsonSerializer::Serialize(O.ToSharedRef(), Writer);
		return Out;
	}

	bool AbilityTestObjectsEqual(const TSharedPtr<FJsonObject>& A, const TSharedPtr<FJsonObject>& B);

	// the same JSON value: numbers exactly, objects whatever their key order
	bool AbilityTestValuesEqual(const TSharedPtr<FJsonValue>& A, const TSharedPtr<FJsonValue>& B)
	{
		if (!A.IsValid() || !B.IsValid()) return A.IsValid() == B.IsValid();
		if (A->Type != B->Type) return false;
		switch (A->Type)
		{
		case EJson::Number: return A->AsNumber() == B->AsNumber();
		case EJson::String: return A->AsString() == B->AsString();
		case EJson::Boolean: return A->AsBool() == B->AsBool();
		case EJson::Object: return AbilityTestObjectsEqual(A->AsObject(), B->AsObject());
		case EJson::Array:
		{
			const TArray<TSharedPtr<FJsonValue>>& X = A->AsArray();
			const TArray<TSharedPtr<FJsonValue>>& Y = B->AsArray();
			if (X.Num() != Y.Num()) return false;
			for (int32 I = 0; I < X.Num(); I++) if (!AbilityTestValuesEqual(X[I], Y[I])) return false;
			return true;
		}
		default: return true;
		}
	}

	bool AbilityTestObjectsEqual(const TSharedPtr<FJsonObject>& A, const TSharedPtr<FJsonObject>& B)
	{
		if (!A.IsValid() || !B.IsValid()) return A.IsValid() == B.IsValid();
		if (A->Values.Num() != B->Values.Num()) return false;
		for (const auto& Pair : A->Values)
		{
			const TSharedPtr<FJsonValue> Other = B->TryGetField(*Pair.Key);
			if (!Other.IsValid() || !AbilityTestValuesEqual(Pair.Value, Other)) return false;
		}
		return true;
	}

	const TArray<TSharedPtr<FJsonValue>>& AbilityTestArray(const FJsonObject& O, const TCHAR* Field)
	{
		static const TArray<TSharedPtr<FJsonValue>> Empty;
		const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
		return O.TryGetArrayField(Field, Arr) && Arr ? *Arr : Empty;
	}

	FTCUnit AbilityTestUnit(const TArray<TSharedPtr<FJsonValue>>& Row)
	{
		// shared/sim.js UNIT_ROW: id, type, owner, x, z, rot, aim, hp, supp, target, cover, cd, flags, stars, built
		FTCUnit U;
		U.Id = (int32)Row[0]->AsNumber(); U.Type = Row[1]->AsString(); U.Owner = (int32)Row[2]->AsNumber();
		U.X = (float)Row[3]->AsNumber(); U.Z = (float)Row[4]->AsNumber(); U.Hp = (int32)Row[7]->AsNumber();
		U.Cd = (int32)Row[TCAbilitiesData::RowCd]->AsNumber(); U.Flags = (int32)Row[TCAbilitiesData::RowFlags]->AsNumber();
		U.Built = (float)Row[14]->AsNumber();
		return U;
	}

	// a seat the lobby confirmed (or a spectator), in a match that is being played
	void AbilityTestSeat(FTCMatchState& State, int32 Me, bool bWatching)
	{
		State.bHasMatch = true; State.You = Me; State.LobbySeat = Me; State.bLobbySinceOpen = true; State.bSpectator = bWatching;
		State.RoomState = TEXT("play");
	}

	void AbilityTestRows(FTCMatchState& State, const TArray<TSharedPtr<FJsonValue>>& Rows)
	{
		for (const TSharedPtr<FJsonValue>& R : Rows) { const FTCUnit U = AbilityTestUnit(R->AsArray()); State.Units.Add(U.Id, U); }
	}

	// One generated case's world: the seat, the start's factions, the snapshot (its rows and the facts FTCAbilities
	// reads) and the selection in its order.
	struct FAbilityTestWorld
	{
		FTCMatchState State;
		FTCAbilities Abilities;
		TSet<int32> Selected;
		explicit FAbilityTestWorld(const FJsonObject& Case)
		{
			bool bWatching = false;
			Case.TryGetBoolField(TEXT("watching"), bWatching);
			AbilityTestSeat(State, (int32)Case.GetNumberField(TEXT("me")), bWatching);
			for (const TSharedPtr<FJsonValue>& T : AbilityTestArray(Case, TEXT("teams"))) State.Teams.Add((int32)T->AsNumber());
			TSharedRef<FJsonObject> Start = MakeShared<FJsonObject>();
			Start->SetArrayField(TEXT("factions"), AbilityTestArray(Case, TEXT("factions")));
			Abilities.ApplyStart(*Start);
			const TSharedPtr<FJsonObject>* Snap = nullptr;
			if (Case.TryGetObjectField(TEXT("snap"), Snap) && Snap && Snap->IsValid())
			{
				State.Snapshots = 1;
				AbilityTestRows(State, AbilityTestArray(**Snap, TEXT("units")));
				Abilities.ApplySnapshot(**Snap);
			}
			for (const TSharedPtr<FJsonValue>& V : AbilityTestArray(Case, TEXT("selected"))) Selected.Add((int32)V->AsNumber());
		}
	};
}

// Run (after a build): -ExecCmds="Automation RunTests ThreeCrossroads.Abilities; Quit". The Reference cases' expected
// answers come from the pinned reference's own code (unreal/Tools/export-ability-table.mjs).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAbilitiesReferenceTest, "ThreeCrossroads.Abilities.Reference", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCAbilitiesReferenceTest::RunTest(const FString& Parameters)
{
	FTCUnitTable Tbl;
	FString TableError;
	if (!TestTrue(TEXT("unit table loads (Content/Data/units.json)"), Tbl.Load(TableError))) return false;
	TestEqual(TEXT("unit table is at the ability pin"), Tbl.Reference, FString(FTCAbilities::ReferencePin));
	TestEqual(TEXT("cases are at the ability pin"), FString(TCAbilitiesCases::ReferencePin), FString(FTCAbilities::ReferencePin));

	int32 Ran = 0;
	for (const TCHAR* Src : TCAbilitiesCases::Cases)
	{
		const TSharedPtr<FJsonObject> Case = AbilityTestParse(Src);
		if (!TestTrue(TEXT("a case parses"), Case.IsValid())) continue;
		const FString Kind = Case->GetStringField(TEXT("case"));
		const FString Name = Kind + TEXT(": ") + Case->GetStringField(TEXT("name"));
		auto Label = [&Name](const TCHAR* What) { return Name + TEXT(" (") + What + TEXT(")"); };
		const TSharedPtr<FJsonObject>* ExpectObj = nullptr;
		Case->TryGetObjectField(TEXT("expect"), ExpectObj);
		const TSharedPtr<FJsonObject> Expect = ExpectObj ? *ExpectObj : nullptr;
		Ran++;

		// the one message the reference sent (or none), and what it showed
		auto CheckSent = [&](const FTCAbilityPlan& P)
		{
			const TArray<TSharedPtr<FJsonValue>>& Sent = AbilityTestArray(*Expect, TEXT("sent"));
			if (!Sent.Num()) { TestTrue(*Label(TEXT("nothing sent")), !P.Message.IsValid()); return; }
			const bool bSame = P.Message.IsValid() && AbilityTestObjectsEqual(P.Message, Sent[0]->AsObject());
			TestTrue(*FString::Printf(TEXT("%s: sent %s, want %s"), *Label(TEXT("message")), *AbilityTestText(P.Message), *AbilityTestText(Sent[0]->AsObject())), bSame);
		};
		auto CheckFeedback = [&](const FTCAbilityPlan& P)
		{
			const TArray<TSharedPtr<FJsonValue>>& Feedback = AbilityTestArray(*Expect, TEXT("feedback"));
			if (Feedback.Num()) { TestEqual(*Label(TEXT("feedback")), P.Note, Feedback[0]->AsString()); TestTrue(*Label(TEXT("feedback is the browser's")), !P.bNative); }
		};

		if (Kind == TEXT("availability"))
		{
			FAbilityTestWorld W(*Case);
			bool bQueue = false;
			Case->TryGetBoolField(TEXT("queue"), bQueue);
			const FTCAbilityCheck C = W.Abilities.Check(W.State, Tbl, W.Selected, Case->GetStringField(TEXT("unit")), bQueue);
			TestTrue(*Label(TEXT("ok")), C.bOk == Expect->GetBoolField(TEXT("ok")));
			TestEqual(*Label(TEXT("reason")), C.Reason, Expect->GetStringField(TEXT("reason")));
			TestTrue(*Label(TEXT("not native")), !C.bNative);
		}
		else if (Kind == TEXT("fkey"))
		{
			FAbilityTestWorld W(*Case);
			TestEqual(*Label(TEXT("type")), W.Abilities.FKeyType(W.State, Tbl, W.Selected), Expect->GetStringField(TEXT("type")));
		}
		else if (Kind == TEXT("use"))
		{
			FAbilityTestWorld W(*Case);
			const FString Unit = Case->GetStringField(TEXT("unit"));
			const FTCAbilityPlan P = Unit.IsEmpty() ? W.Abilities.PlanKey(W.State, Tbl, W.Selected) : W.Abilities.PlanUse(W.State, Tbl, W.Selected, Unit);
			TestEqual(*Label(TEXT("type")), P.Type, Expect->GetStringField(TEXT("type")));
			CheckSent(P);
			CheckFeedback(P);
			const TArray<TSharedPtr<FJsonValue>>& Aims = AbilityTestArray(*Expect, TEXT("aims"));
			TestTrue(*Label(TEXT("aim armed")), P.bArmAim == (Aims.Num() > 0));
			if (Aims.Num())
			{
				TestEqual(*Label(TEXT("aim kind")), P.AbilityId, Aims[0]->AsObject()->GetStringField(TEXT("kind")));
				TestEqual(*Label(TEXT("aimed unit")), P.Type, Aims[0]->AsObject()->GetStringField(TEXT("unit")));
				TestTrue(*Label(TEXT("aim kind")), P.Kind == ETCAbilityCommand::ArmAim);
			}
			if (!Aims.Num() && !AbilityTestArray(*Expect, TEXT("sent")).Num() && !AbilityTestArray(*Expect, TEXT("feedback")).Num())
			{
				TestTrue(*Label(TEXT("silent")), P.Note.IsEmpty() && P.Kind == ETCAbilityCommand::None);
			}
		}
		else if (Kind == TEXT("throw"))
		{
			FAbilityTestWorld W(*Case);
			bool bShift = false;
			Case->TryGetBoolField(TEXT("shift"), bShift);
			const TSharedPtr<FJsonObject>* G = nullptr;
			if (!TestTrue(*Label(TEXT("ground")), Case->TryGetObjectField(TEXT("ground"), G) && G && G->IsValid())) continue;
			const FVector2D Ground((*G)->GetNumberField(TEXT("x")), (*G)->GetNumberField(TEXT("z")));
			const FTCAbilityPlan P = W.Abilities.PlanAimClick(W.State, Tbl, W.Selected, Case->GetStringField(TEXT("unit")), Case->GetStringField(TEXT("kind")), true, Ground, bShift);
			CheckSent(P);
			CheckFeedback(P);
			const bool bCancelled = AbilityTestArray(*Expect, TEXT("aims")).Num() > 0;
			TestTrue(*Label(TEXT("aim ends")), P.bCancelAim == bCancelled && P.bKeepAim == !bCancelled);
			const TArray<TSharedPtr<FJsonValue>>& Marks = AbilityTestArray(*Expect, TEXT("marks"));
			if (Marks.Num())
			{
				const TArray<TSharedPtr<FJsonValue>>& M = Marks[0]->AsArray();
				TestTrue(*Label(TEXT("marker at the click")), P.At.X == M[0]->AsNumber() && P.At.Y == M[1]->AsNumber());
				const uint32 Rgb = (uint32)M[2]->AsNumber();
				TestEqual(*Label(TEXT("marker colour")), P.Color, FColor((uint8)(Rgb >> 16), (uint8)(Rgb >> 8), (uint8)Rgb));
			}
		}
		else if (Kind == TEXT("buttons"))
		{
			FAbilityTestWorld W(*Case);
			const TArray<FTCAbilityButton> Bs = W.Abilities.Buttons(W.State, Tbl, W.Selected);
			const TArray<TSharedPtr<FJsonValue>>& Want = AbilityTestArray(*Case, TEXT("expect"));
			if (!TestEqual(*Label(TEXT("buttons")), Bs.Num(), Want.Num())) continue;
			for (int32 I = 0; I < Bs.Num(); I++)
			{
				const FTCAbilityButton& B = Bs[I];
				const TSharedPtr<FJsonObject> E = Want[I]->AsObject();
				TestEqual(*Label(TEXT("button type")), B.Type, E->GetStringField(TEXT("type")));
				TestEqual(*Label(TEXT("button icon")), B.Icon, E->GetStringField(TEXT("icon")));
				TestEqual(*Label(TEXT("button tip")), B.Tip, E->GetStringField(TEXT("tip")));
				TestEqual(*Label(TEXT("button text")), B.Text, E->GetStringField(TEXT("text")));
				TestEqual(*Label(TEXT("button key")), B.Key, E->GetStringField(TEXT("key")));
				TestTrue(*Label(TEXT("button auto look")), B.bAuto == E->GetBoolField(TEXT("auto")));
				TestTrue(*Label(TEXT("button ok")), B.Check.bOk == E->GetBoolField(TEXT("ok")));
				TestEqual(*Label(TEXT("button reason")), B.Check.Reason, E->GetStringField(TEXT("reason")));
			}
		}
		else if (Kind == TEXT("autocast"))
		{
			FTCAutocastMemory Mem;
			int32 Step = 0;
			for (const TSharedPtr<FJsonValue>& SV : AbilityTestArray(*Case, TEXT("steps")))
			{
				const TSharedPtr<FJsonObject> S = SV->AsObject();
				const FString Op = S->GetStringField(TEXT("op"));
				const FString StepName = FString::Printf(TEXT("%s step %d %s"), *Name, ++Step, *Op);
				bool bClassic = false;
				S->TryGetBoolField(TEXT("classic"), bClassic);
				TArray<TSharedRef<FJsonObject>> Got;
				if (Op == TEXT("toggle"))
				{
					TArray<FTCUnit> Units;
					for (const TSharedPtr<FJsonValue>& R : AbilityTestArray(*S, TEXT("units"))) Units.Add(AbilityTestUnit(R->AsArray()));
					TArray<const FTCUnit*> List;
					for (const FTCUnit& U : Units) List.Add(&U);
					bool bOn = false;
					const TSharedPtr<FJsonObject> M = Mem.Toggle(S->GetStringField(TEXT("type")), List, bClassic, &bOn);
					const TSharedPtr<FJsonValue> Result = S->TryGetField(TEXT("result"));
					if (Result.IsValid() && Result->Type == EJson::Boolean) TestTrue(*(StepName + TEXT(" result")), M.IsValid() && bOn == Result->AsBool());
					else TestTrue(*(StepName + TEXT(" result")), !M.IsValid());
					if (M.IsValid()) Got.Add(M.ToSharedRef());
					// the browser writes its storage on every toggle with a list
					FString Storage;
					if (S->TryGetStringField(TEXT("storage"), Storage))
					{
						TestTrue(*FString::Printf(TEXT("%s storage: %s, want %s"), *StepName, *Mem.Save(), *Storage), AbilityTestObjectsEqual(AbilityTestParse(Mem.Save()), AbilityTestParse(Storage)));
					}
				}
				else if (Op == TEXT("adopt"))
				{
					FTCMatchState St;
					AbilityTestSeat(St, (int32)S->GetNumberField(TEXT("me")), false);
					AbilityTestRows(St, AbilityTestArray(*S, TEXT("units")));
					Got = Mem.Adopt(St, bClassic);
				}
				else if (Op == TEXT("reset")) Mem.Reset();
				const TArray<TSharedPtr<FJsonValue>>& Sent = AbilityTestArray(*S, TEXT("sent"));
				if (!TestEqual(*(StepName + TEXT(" messages")), Got.Num(), Sent.Num())) continue;
				for (int32 I = 0; I < Got.Num(); I++)
				{
					TestTrue(*FString::Printf(TEXT("%s message: %s, want %s"), *StepName, *AbilityTestText(Got[I]), *AbilityTestText(Sent[I]->AsObject())), AbilityTestObjectsEqual(Got[I], Sent[I]->AsObject()));
				}
			}
		}
		else AddError(FString::Printf(TEXT("unknown case kind %s"), *Kind));
	}
	TestEqual(TEXT("cases run"), Ran, (int32)UE_ARRAY_COUNT(TCAbilitiesCases::Cases));
	return true;
}

// What the generated cases cannot show: the table as the port reads it, this client's own refusals, the server
// predictions and the autocast memory's storage.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAbilitiesNativeTest, "ThreeCrossroads.Abilities.Native", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCAbilitiesNativeTest::RunTest(const FString& Parameters)
{
	FTCUnitTable Tbl;
	FString TableError;
	if (!TestTrue(TEXT("unit table loads (Content/Data/units.json)"), Tbl.Load(TableError))) return false;

	// the table at the pin: 17 types with an ability, 15 with a button; the gunboat and destroyer have none
	TestEqual(TEXT("types with an ability"), FTCAbilities::All().Num(), 17);
	TestEqual(TEXT("types with a button"), FTCAbilities::Priority().Num(), 15);
	const FTCAbilityDef* Gunboat = FTCAbilities::Find(TEXT("gunboat"));
	TestTrue(TEXT("gunboat: Torpedo, no button"), Gunboat && Gunboat->Name == TEXT("Torpedo") && Gunboat->Priority == INDEX_NONE);
	TestTrue(TEXT("sniper has no ability"), FTCAbilities::Find(TEXT("sniper")) == nullptr);
	const FTCAbilityDef* Medium = FTCAbilities::Find(TEXT("medium"));
	TestTrue(TEXT("medium tank: HE Shell is aimed as a grenade"), Medium && Medium->Id == TEXT("grenade") && Medium->bAimed && Medium->Name == TEXT("HE Shell"));
	const FTCAbilityDef* Rocket = FTCAbilities::Find(TEXT("rocket"));
	TestTrue(TEXT("rocket barrage is free everywhere"), Rocket && Rocket->ClassicMun == 0 && Rocket->Priority == 14);
	TestEqual(TEXT("grenade hint"), FTCAbilities::AimHint(TEXT("grenade")), FString(TEXT("Click where to throw. Right-click cancels")));
	TestEqual(TEXT("barrage ring: the rocket's spread"), FTCAbilities::AimRadius(TEXT("barrage")), 6.0);
	TestTrue(TEXT("F uses an ability"), FTCAbilities::KeyAction(EKeys::F, false, false, false) == ETCAbilityKey::Use);
	TestTrue(TEXT("Shift+F does not"), FTCAbilities::KeyAction(EKeys::F, true, false, false) == ETCAbilityKey::None);
	TestTrue(TEXT("Ctrl+F does not"), FTCAbilities::KeyAction(EKeys::F, false, true, false) == ETCAbilityKey::None);

	// a Classic snapshot in a lobby that says skirmish, seat 0 confirmed, 15 Munitions: two ready MG teams (10 each: the
	// first pays, so the server takes the order), a ready rifle squad (a grenade costs 15), a foe's rifle squad, an HQ
	const TSharedPtr<FJsonObject> Snap = AbilityTestParse(TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":100,\"mun\":15,\"out\":[false,false]}"));
	auto Make = [&](FTCMatchState& St, FTCAbilities& Ab)
	{
		AbilityTestSeat(St, 0, false);
		St.Mode = TEXT("skirmish");
		St.Snapshots = 1;
		Ab.ApplyStart(FJsonObject());
		Ab.ApplySnapshot(*Snap);
		FTCUnit A; A.Id = 1; A.Type = TEXT("mg"); A.Owner = 0; St.Units.Add(1, A);
		FTCUnit B = A; B.Id = 2; B.X = 3; St.Units.Add(2, B);
		FTCUnit Foe; Foe.Id = 9; Foe.Type = TEXT("rifle"); Foe.Owner = 1; Foe.X = 10; Foe.Z = 10; St.Units.Add(9, Foe);
		FTCUnit Rifle; Rifle.Id = 3; Rifle.Type = TEXT("rifle"); Rifle.Owner = 0; St.Units.Add(3, Rifle);
		FTCUnit Hq; Hq.Id = 4; Hq.Type = TEXT("hq"); Hq.Owner = 0; St.Units.Add(4, Hq);
	};
	{
		FTCMatchState St; FTCAbilities Ab; Make(St, Ab);
		const FTCAbilityPlan P = Ab.PlanUse(St, Tbl, TSet<int32>{ 1, 2 }, TEXT("mg"));
		TestTrue(TEXT("two MG teams suppress"), P.Kind == ETCAbilityCommand::Use && P.Units == 2 && P.PredictedDeny.IsEmpty());
		TestTrue(TEXT("the plan's order plan carries the message"), P.ToOrderPlan().Message == P.Message && P.ToOrderPlan().Units == 2);
		TestEqual(TEXT("Classic price"), Ab.Cost(TEXT("mg")), 10);

		// the browser names the nearest selected rifle even when it is not this seat's; the server refuses it
		const FTCAbilityPlan T = Ab.PlanAimClick(St, Tbl, TSet<int32>{ 3, 9 }, TEXT("rifle"), TEXT("grenade"), true, FVector2D(10, 10), false);
		TestTrue(TEXT("nearest selected rifle is named"), T.Kind == ETCAbilityCommand::Throw && T.Message.IsValid() && AbilityTestText(T.Message).Contains(TEXT("\"ids\":[9]")));
		TestEqual(TEXT("a rifle that is not ours: 'needs'"), T.PredictedDeny, FString(TEXT("needs")));

		// an all-building selection shows no Orders panel, so no ability buttons
		TestEqual(TEXT("no buttons for a building"), Ab.Buttons(St, Tbl, TSet<int32>{ 4 }).Num(), 0);

		// autocast: the selected own MG teams, remembered in the bucket for the lobby's mode (skirmish: 'free')
		const FTCAbilityPlan A = Ab.PlanAutocast(St, Tbl, TSet<int32>{ 1, 2, 9 }, TEXT("mg"));
		TestTrue(TEXT("autocast on for both MG teams"), A.Kind == ETCAbilityCommand::Autocast && A.bAutocastOn && A.Units == 2 && A.PredictedDeny.IsEmpty());
		TestTrue(TEXT("remembered outside Classic"), Ab.Autocast.Preference(TEXT("mg"), false) == TOptional<bool>(true) && !Ab.Autocast.Preference(TEXT("mg"), true).IsSet());
		TestEqual(TEXT("nothing to toggle"), (int32)Ab.PlanAutocast(St, Tbl, TSet<int32>{ 9 }, TEXT("rifle")).Kind, (int32)ETCAbilityCommand::None);
	}
	{
		// this client's own refusals: another pin's unit table, a seat the lobby has not confirmed
		FTCMatchState St; FTCAbilities Ab; Make(St, Ab);
		FTCUnitTable Other = Tbl;
		Other.Reference = TEXT("0000000");
		const FTCAbilityCheck Pin = Ab.Check(St, Other, TSet<int32>{ 1 }, TEXT("mg"));
		TestTrue(TEXT("another pin refuses natively"), !Pin.bOk && Pin.bNative);
		St.bLobbySinceOpen = false;
		const FTCAbilityCheck Seat = Ab.Check(St, Tbl, TSet<int32>{ 1 }, TEXT("mg"));
		TestTrue(TEXT("unconfirmed seat refuses natively"), !Seat.bOk && Seat.bNative && Seat.Reason == TEXT("Waiting for the lobby to confirm this seat"));
		// adopt waits for the seat, then brings the units in line once
		Ab.Autocast.Load(TEXT("{\"free\":{\"mg\":true},\"other\":1}"));
		TestEqual(TEXT("no adoption before the seat is confirmed"), Ab.AdoptAutocast(St).Num(), 0);
		St.bLobbySinceOpen = true;
		const TArray<TSharedRef<FJsonObject>> On = Ab.AdoptAutocast(St);
		TestTrue(TEXT("adopted once the seat is confirmed"), On.Num() == 1 && AbilityTestText(On[0]).Contains(TEXT("\"ids\":[1,2]")));
		TestEqual(TEXT("and only once"), Ab.AdoptAutocast(St).Num(), 0);
		TestTrue(TEXT("unknown keys are kept"), Ab.Autocast.Save().Contains(TEXT("\"other\":1")));
		Ab.Autocast.Load(TEXT("not json"));
		TestEqual(TEXT("a bad document is no choices"), Ab.Autocast.Save(), FString(TEXT("{}")));
	}
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
