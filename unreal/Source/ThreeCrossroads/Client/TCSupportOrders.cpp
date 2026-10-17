#include "Client/TCSupportOrders.h"
#include "Client/TCArmyCommands.h"
#include "Client/TCConstruction.h"
#include "Client/TCRecruitment.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "InputCoreTypes.h"

const TCHAR* LexToString(ETCSupportOrder Kind)
{
	switch (Kind)
	{
	case ETCSupportOrder::Blocked: return TEXT("blocked");
	case ETCSupportOrder::Assist: return TEXT("assist");
	case ETCSupportOrder::Repair: return TEXT("repair");
	case ETCSupportOrder::Board: return TEXT("board");
	case ETCSupportOrder::MountUp: return TEXT("mount-up");
	case ETCSupportOrder::Unload: return TEXT("unload");
	case ETCSupportOrder::CancelSite: return TEXT("cancel-site");
	default: return TEXT("none");
	}
}

const TCHAR* LexToString(ETCSupportKey Key)
{
	switch (Key)
	{
	case ETCSupportKey::Unload: return TEXT("unload");
	case ETCSupportKey::MountUp: return TEXT("mountUp");
	default: return TEXT("none");
	}
}

FTCOrderPlan FTCSupportPlan::ToOrderPlan() const
{
	FTCOrderPlan Plan;
	Plan.Kind = ETCOrderKind::None;
	Plan.Message = Message;
	Plan.Units = Units;
	Plan.TargetId = TargetId;
	Plan.At = At;
	Plan.Color = Color;
	Plan.Why = Note;
	return Plan;
}

// Every helper name here carries the Support prefix: the module builds in unity files next to TCOrders.cpp and
// TCArmyCommands.cpp, whose anonymous helpers would otherwise collide.
namespace
{
	struct FSupportTroop { const FTCUnit* U; const FTCUnitDef* D; };

	TArray<TSharedPtr<FJsonValue>> SupportIdArray(const TArray<int32>& Ids)
	{
		TArray<TSharedPtr<FJsonValue>> Out;
		Out.Reserve(Ids.Num());
		for (int32 Id : Ids) Out.Add(MakeShared<FJsonValueNumber>(Id));
		return Out;
	}

	// The selected ids this client holds a row and a table entry for, in the selection's order (client/hud.js selUnits
	// over held rows; an unknown type would throw in the browser's UNITS lookups, here it is left out).
	TArray<FSupportTroop> SupportHeld(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected)
	{
		TArray<FSupportTroop> Out;
		for (int32 Id : Selected)
		{
			const FTCUnit* U = State.Units.Find(Id);
			const FTCUnitDef* D = U ? Table.Find(U->Type) : nullptr;
			if (U && D) Out.Add({ U, D });
		}
		return Out;
	}

	// JavaScript Math.round for the non-negative percentages shown here.
	int32 SupportJsRound(double V) { return (int32)FMath::FloorToDouble(V + 0.5); }

	// FTCConstruction's builder lists are generated at its pin; another unit table may name other types.
	bool SupportBuilderTableMatches(const FTCUnitTable& Table, FString& OutWhy)
	{
		if (Table.Reference == FTCConstruction::ReferencePin) return true;
		OutWhy = FString::Printf(TEXT("the builder data is for reference %s, the unit table is for %s"), FTCConstruction::ReferencePin,
			Table.Reference.IsEmpty() ? TEXT("an unknown reference") : *Table.Reference);
		return false;
	}

	// UNITS[type].naval from the recruit table (generated at FTCRecruitment::ReferencePin). False when unknown.
	bool SupportNaval(const FTCUnitTable& Table, const FString& Type, bool& bOutNaval, FString& OutWhy)
	{
		if (Table.Reference != FTCRecruitment::ReferencePin)
		{
			OutWhy = FString::Printf(TEXT("the naval flags are for reference %s, the unit table is for %s"), FTCRecruitment::ReferencePin,
				Table.Reference.IsEmpty() ? TEXT("an unknown reference") : *Table.Reference);
			return false;
		}
		const FTCRecruitDef* R = FTCRecruitment::Find(Type);
		if (!R) { OutWhy = FString::Printf(TEXT("the recruit table has no row for '%s'"), *Type); return false; }
		bOutNaval = R->bNaval;
		return true;
	}

	// shared/sim.js command mine(), as this seat sees it, plus the squad's retreat flag.
	bool SupportMine(const FTCMatchState& State, const FSupportTroop& T) { return TCArmyCommands::TakesSelectionOrder(State, *T.U, T.D); }
	bool SupportRetreating(const FTCUnit& U) { return (U.Flags & TCFlags::Retreating) != 0; }
}

void FTCSupportOrders::ApplyStart(const FJsonObject& /*M*/)
{
	// server.js sendStart: the snapshots after a start send rows against an empty "sent" map
	Rows.Reset();
	bStarted = true;
}

void FTCSupportOrders::Clear()
{
	Rows.Reset();
	bStarted = false;
}

void FTCSupportOrders::ApplySnapshot(const FJsonObject& M)
{
	if (!bStarted) return;
	// client/world-state.js, as FTCMatchState::ApplySnapshot applies it: all (or no held) replaces, gone removes,
	// rows upsert
	const TArray<TSharedPtr<FJsonValue>>* Held = nullptr;
	const bool bHasHeld = M.TryGetArrayField(TEXT("held"), Held) && Held && Held->Num() >= 2;
	bool bAll = false;
	M.TryGetBoolField(TEXT("all"), bAll);
	if (bAll || !bHasHeld) Rows.Reset();
	const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
	if (M.TryGetArrayField(TEXT("gone"), Arr) && Arr)
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr) if (V.IsValid() && V->Type == EJson::Number) Rows.Remove((int32)V->AsNumber());
	}
	if (!M.TryGetArrayField(TEXT("units"), Arr) || !Arr) return;
	for (const TSharedPtr<FJsonValue>& V : *Arr)
	{
		const TArray<TSharedPtr<FJsonValue>>* Row = nullptr;
		if (!V.IsValid() || !V->TryGetArray(Row) || !Row || Row->Num() < 2) continue;
		const TSharedPtr<FJsonValue>& IdV = (*Row)[0];
		if (!IdV.IsValid() || IdV->Type != EJson::Number) continue;
		const int32 Id = (int32)IdV->AsNumber();
		FRowBuilt R;
		if (!(*Row)[1].IsValid() || !(*Row)[1]->TryGetString(R.Type)) { Rows.Remove(Id); continue; }
		// client/main.js applySnapshot: built ?? 1 (shared/sim.js unitRow: u.built ?? 1, the raw double)
		TSharedPtr<FJsonValue> B;
		if (Row->Num() > 14) B = (*Row)[14];
		if (!B.IsValid() || B->Type == EJson::Null || B->Type == EJson::None) R.Built = 1;
		else if (B->Type == EJson::Number) R.Built = B->AsNumber();
		else { Rows.Remove(Id); continue; } // not a number the browser could compare: fall back to the held row
		Rows.Add(Id, MoveTemp(R));
	}
}

double FTCSupportOrders::BuiltOf(const FTCUnit& U) const
{
	const FRowBuilt* R = Rows.Find(U.Id);
	return R && R->Type == U.Type ? R->Built : (double)U.Built;
}

FTCSupportPlan FTCSupportOrders::PlanRightClick(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bQueue) const
{
	FTCSupportPlan Plan;
	// client/orders.js selection: troops are the own selected units that are not structures
	TArray<FSupportTroop> Troops;
	for (const FSupportTroop& T : SupportHeld(State, Table, Selected))
	{
		if (State.IsOwn(*T.U) && !T.D->bStructure) Troops.Add(T);
	}
	if (!Troops.Num()) return Plan;

	// client/orders.js kind, in its order. Supply (every troop a truck) and escort (every troop a plane, a friend under
	// the pointer) come first and are not this module's.
	const bool bAllTrucks = !Troops.ContainsByPredicate([](const FSupportTroop& T) { return T.U->Type != TEXT("truck"); });
	if (bAllTrucks) return Plan;
	const bool bAllPlanes = !Troops.ContainsByPredicate([](const FSupportTroop& T) { return !T.D->bAir; });
	if (Cursor.Friend && bAllPlanes) return Plan;

	// 'board': an own carrier under the pointer and infantry among the troops. The infantry climb in.
	const FTCUnit* FriendU = Cursor.Friend ? State.Units.Find(Cursor.Friend) : nullptr;
	const FTCUnitDef* FriendD = FriendU ? Table.Find(FriendU->Type) : nullptr;
	if (FriendU && FriendD && State.SeatConfirmed() && FriendU->Owner == State.You && FriendD->Carries > 0
		&& Troops.ContainsByPredicate([](const FSupportTroop& T) { return T.D->bInfantry; }))
	{
		TArray<int32> Ids;
		int32 Able = 0, Retreating = 0;
		for (const FSupportTroop& T : Troops)
		{
			if (!T.D->bInfantry) continue;
			Ids.Add(T.U->Id);
			if (SupportMine(State, T)) { if (SupportRetreating(*T.U)) Retreating++; else Able++; }
		}
		TSharedRef<FJsonObject> C = MakeShared<FJsonObject>();
		C->SetStringField(TEXT("t"), TEXT("board"));
		C->SetArrayField(TEXT("ids"), SupportIdArray(Ids));
		C->SetNumberField(TEXT("target"), FriendU->Id);
		C->SetBoolField(TEXT("queue"), bQueue);
		Plan.Kind = ETCSupportOrder::Board;
		Plan.Message = C;
		Plan.Units = Ids.Num();
		Plan.TargetId = FriendU->Id;
		Plan.At = FVector2D(FriendU->X, FriendU->Z);
		Plan.Color = FColor(0x9d, 0xd0, 0xff);
		// shared/sim.js command 'board' with a target. The squads already walking to it are not in the snapshot, so a
		// carrier that shows a free seat can still be full ('max').
		if (FriendU->Hp <= 0) Plan.PredictedDeny = TEXT("unseen");
		else if (!Able) Plan.PredictedDeny = Retreating ? TEXT("retreating") : TEXT("needs");
		else if (FriendD->Carries - TCCargo::Count(FriendU->Flags) <= 0) Plan.PredictedDeny = TEXT("max");
		return Plan;
	}

	// 'assist': an allied building under the pointer that is unfinished or below its hpPer, and builders among the
	// troops (builderTypes(classicMode()): Engineers in Classic and World, the fort builders otherwise)
	const FTCUnit* B = Cursor.Building ? State.Units.Find(Cursor.Building) : nullptr;
	const FTCUnitDef* BD = B ? Table.Find(B->Type) : nullptr;
	if (!B || !BD) return Plan;
	const double Built = BuiltOf(*B);
	if (!(Built < 1 || B->Hp < BD->HpPer)) return Plan;
	FString Why;
	if (!SupportBuilderTableMatches(Table, Why))
	{
		// whether any troop is a builder decides between assist and the attack or move below it: do not guess
		Plan.Kind = ETCSupportOrder::Blocked;
		Plan.Note = TEXT("Assist or repair: ") + Why;
		Plan.bNative = true;
		return Plan;
	}
	const TArray<FString> Can = FTCConstruction::BuilderTypes(TCArmyCommands::ClassicLobby(State));
	TArray<int32> Eng;
	int32 Able = 0, Retreating = 0;
	for (const FSupportTroop& T : Troops)
	{
		if (!Can.Contains(T.U->Type)) continue;
		Eng.Add(T.U->Id);
		if (SupportMine(State, T)) { if (SupportRetreating(*T.U)) Retreating++; else Able++; }
	}
	if (!Eng.Num()) return Plan;
	TSharedRef<FJsonObject> C = MakeShared<FJsonObject>();
	C->SetStringField(TEXT("t"), TEXT("assist"));
	C->SetArrayField(TEXT("ids"), SupportIdArray(Eng));
	C->SetNumberField(TEXT("id"), B->Id);
	C->SetBoolField(TEXT("queue"), bQueue);
	Plan.Kind = Built < 1 ? ETCSupportOrder::Assist : ETCSupportOrder::Repair;
	Plan.Message = C;
	Plan.Units = Eng.Num();
	Plan.TargetId = B->Id;
	Plan.At = FVector2D(B->X, B->Z);
	Plan.Color = FColor(0xe8, 0xc8, 0x60);
	// shared/sim.js command 'assist'. The server picks builders by the snapshot's mode (isConstructionMode), the browser
	// by the lobby's (classicMode); this prediction uses the browser's list. Waiting orders (queueFull) are not held.
	if (B->Hp <= 0) Plan.PredictedDeny = TEXT("unseen");
	else if (!Able) Plan.PredictedDeny = Retreating ? TEXT("retreating") : TEXT("noBuilders");
	return Plan;
}

FTCSupportPlan FTCSupportOrders::PlanMountUp(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const
{
	FTCSupportPlan Plan;
	if (!Selected.Num()) return Plan;
	// client/main.js mountUp: { t: 'board', ids: [...selected] }, the whole selection; the sim pairs squads and carriers
	const TArray<int32> Ids = Selected.Array();
	TSharedRef<FJsonObject> C = MakeShared<FJsonObject>();
	C->SetStringField(TEXT("t"), TEXT("board"));
	C->SetArrayField(TEXT("ids"), SupportIdArray(Ids));
	Plan.Kind = ETCSupportOrder::MountUp;
	Plan.Message = C;
	Plan.Units = Ids.Num();

	// shared/sim.js command 'board' without a target: carriers are the named own living land carriers, squads the named
	// infantry mine() takes that are not retreating
	int32 Carriers = 0, Seats = 0, Able = 0, Retreating = 0;
	bool bKnown = true;
	for (const FSupportTroop& T : SupportHeld(State, Table, Selected))
	{
		if (State.IsOwn(*T.U) && T.U->Hp > 0 && T.D->Carries > 0)
		{
			bool bNaval = false;
			FString Why;
			if (!SupportNaval(Table, T.U->Type, bNaval, Why)) bKnown = false;
			else if (!bNaval) { Carriers++; Seats += T.D->Carries - TCCargo::Count(T.U->Flags); }
		}
		if (T.D->bInfantry && SupportMine(State, T)) { if (SupportRetreating(*T.U)) Retreating++; else Able++; }
	}
	if (!bKnown) return Plan; // no prediction without the naval flags
	if (!Carriers) Plan.PredictedDeny = TEXT("needs");
	else if (!Able) Plan.PredictedDeny = Retreating ? TEXT("retreating") : TEXT("needs");
	else if (Seats <= 0) Plan.PredictedDeny = TEXT("max"); // squads already walking to a carrier are not held
	return Plan;
}

FTCSupportPlan FTCSupportOrders::PlanUnload(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const
{
	FTCSupportPlan Plan;
	// client/main.js unload: [...selected].filter(id => UNITS[units.get(id)?.type]?.carries)
	TArray<int32> Ids;
	int32 Loaded = 0;
	for (const FSupportTroop& T : SupportHeld(State, Table, Selected))
	{
		if (T.D->Carries <= 0) continue;
		Ids.Add(T.U->Id);
		if (SupportMine(State, T) && (T.U->Flags & TCCargo::Flag)) Loaded++;
	}
	if (!Ids.Num()) return Plan;
	TSharedRef<FJsonObject> C = MakeShared<FJsonObject>();
	C->SetStringField(TEXT("t"), TEXT("unload"));
	C->SetArrayField(TEXT("ids"), SupportIdArray(Ids));
	Plan.Kind = ETCSupportOrder::Unload;
	Plan.Message = C;
	Plan.Units = Ids.Num();
	// shared/sim.js command 'unload': carriers mine() takes with cargo. A boat out of reach of the shore ('shore')
	// needs the server's nearestFree and is not predicted.
	if (!Loaded) Plan.PredictedDeny = TEXT("needs");
	return Plan;
}

FTCSupportPlan FTCSupportOrders::PlanCancelSite(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, bool bSnapshotClassic) const
{
	FTCSupportPlan Plan;
	const FTCSiteCard Card = SiteCard(State, Table, Selected, bSnapshotClassic);
	if (!Card.bCancel)
	{
		Plan.Kind = ETCSupportOrder::Blocked;
		Plan.Note = TEXT("Cancel: no Cancel button for this selection (select one of your own unfinished buildings)");
		Plan.bNative = true;
		return Plan;
	}
	// client/hud.js: { t: 'cancel', id }
	TSharedRef<FJsonObject> C = MakeShared<FJsonObject>();
	C->SetStringField(TEXT("t"), TEXT("cancel"));
	C->SetNumberField(TEXT("id"), Card.Building);
	Plan.Kind = ETCSupportOrder::CancelSite;
	Plan.Message = C;
	Plan.Units = 1;
	Plan.TargetId = Card.Building;
	Plan.bClearSelection = Card.bClassicCard;
	return Plan;
}

FTCCarrierButton FTCSupportOrders::MountUpButton(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const
{
	FTCCarrierButton Button;
	Button.Label = MountUpLabel;
	Button.Tip = FString::Printf(TEXT("Mount up (%s): each selected squad climbs into or onto the nearest selected truck, halftrack or tank with a free seat"), MountUpLabel);
	if (!TCArmyCommands::ShowsOrdersPanel(State, Table, Selected)) return Button;
	// client/hud.js drawOrders: mount = some infantry && some carrier that is not naval; seats over those carriers
	const TArray<FSupportTroop> Sel = SupportHeld(State, Table, Selected);
	const bool bInfantry = Sel.ContainsByPredicate([](const FSupportTroop& T) { return T.D->bInfantry; });
	bool bLandCarrier = false;
	int32 Seats = 0;
	for (const FSupportTroop& T : Sel)
	{
		if (T.D->Carries <= 0) continue;
		bool bNaval = false;
		FString Why;
		if (!SupportNaval(Table, T.U->Type, bNaval, Why))
		{
			Button.Reason = TEXT("Mount up: ") + Why;
			Button.bNative = true;
			return Button;
		}
		if (bNaval) continue;
		bLandCarrier = true;
		Seats += T.D->Carries - TCCargo::Count(T.U->Flags);
	}
	Button.bShown = bInfantry && bLandCarrier;
	if (!Button.bShown) return Button;
	Button.Text = FString::Printf(TEXT("%d free"), Seats);
	Button.bOk = Seats != 0;
	if (!Button.bOk) Button.Reason = TEXT("No free seats");
	return Button;
}

FTCCarrierButton FTCSupportOrders::UnloadButton(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected) const
{
	FTCCarrierButton Button;
	Button.Label = UnloadLabel;
	Button.Tip = FString::Printf(TEXT("Unload (%s): every squad aboard gets out beside its carrier. To board, right-click your own truck, halftrack, boat or tank with infantry selected"), UnloadLabel);
	if (!TCArmyCommands::ShowsOrdersPanel(State, Table, Selected)) return Button;
	// client/hud.js drawOrders: carry = some carrier; n squads aboard of the carriers' seats
	int32 On = 0, Of = 0;
	bool bCarrier = false;
	for (const FSupportTroop& T : SupportHeld(State, Table, Selected))
	{
		bCarrier |= T.D->Carries > 0;
		On += TCCargo::Count(T.U->Flags);
		Of += T.D->Carries;
	}
	Button.bShown = bCarrier;
	if (!bCarrier) return Button;
	Button.Text = FString::Printf(TEXT("%d/%d"), On, Of);
	Button.bOk = On != 0;
	if (!Button.bOk) Button.Reason = TEXT("No squad on board");
	return Button;
}

FTCSiteCard FTCSupportOrders::SiteCard(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, bool bSnapshotClassic) const
{
	FTCSiteCard Card;
	// client/hud.js update draws the cards from a snapshot
	if (!State.bHasMatch || State.Snapshots == 0) return Card;
	int32 HeldCount = 0;
	for (int32 Id : Selected) if (State.Units.Contains(Id)) HeldCount++;
	const TArray<FSupportTroop> Sel = SupportHeld(State, Table, Selected);
	// selected.length === 1 over held rows; a held row of an unknown type is left out of Sel but still counts
	if (HeldCount != 1 || Sel.Num() != 1 || !Sel[0].D->bBuilding) return Card;
	const FTCUnit& U = *Sel[0].U;
	const bool bOwn = State.IsOwn(U);
	const double Built = BuiltOf(U);
	Card.Building = U.Id;
	Card.Built = Built;
	Card.bClassicCard = bSnapshotClassic;
	if (bSnapshotClassic)
	{
		// drawClassicCard: an own building still going up gets the site card (a finished one is FTCRecruitment's)
		if (!bOwn || !(Built < 1)) return Card;
		Card.bShown = true;
		Card.bCancel = true;
		Card.CancelText = TEXT("Cancel");
		Card.CancelSub = TEXT("75% back");
		Card.CancelTip = TEXT("Cancel the building and get 75% of its cost back");
		Card.Status = FString::Printf(TEXT("Under construction %d%%"), SupportJsRound(Built * 100));
		Card.Hint = TEXT("Right-click it with Engineers to help");
		return Card;
	}
	// drawRecruit's facility line: any single selected building; Cancel for an own one still going up
	Card.bShown = true;
	Card.bCancel = Built < 1 && bOwn;
	if (Card.bCancel) Card.CancelText = TEXT("Cancel (75% back)");
	const FString Progress = Built < 1 ? FString::Printf(TEXT("%d%% built"), SupportJsRound(Built * 100)) : FString(TEXT("Ready"));
	Card.Status = FString::Printf(TEXT("%s: %s . %d HP. %s"), *Sel[0].D->Name, *Progress, U.Hp,
		Built < 1 ? TEXT("Right-click with builders to assist") : TEXT("Right-click with builders to repair"));
	return Card;
}

ETCSupportKey FTCSupportOrders::KeyAction(const FKey& Key, bool bShift, bool bCtrl, bool bAlt)
{
	// client/keys.js match: code, shift, ctrl (or meta) and alt must all equal the binding's
	if (!bShift || bCtrl || bAlt) return ETCSupportKey::None;
	if (Key == EKeys::E) return ETCSupportKey::Unload;
	if (Key == EKeys::Q) return ETCSupportKey::MountUp;
	return ETCSupportKey::None;
}
