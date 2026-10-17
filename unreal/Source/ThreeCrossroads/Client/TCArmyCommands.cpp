#include "Client/TCArmyCommands.h"
#include "Client/TCRecruitment.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "InputCoreTypes.h"

const TCHAR* TCStance::Key(ETCStance Stance)
{
	switch (Stance)
	{
	case ETCStance::HoldPos: return TEXT("holdPos");
	case ETCStance::AutoRetreat: return TEXT("autoRetreat");
	default: return TEXT("holdFire");
	}
}

int32 TCStance::Bit(ETCStance Stance)
{
	// client/main.js STANCE_BIT, shared/sim.js stanceBits
	switch (Stance)
	{
	case ETCStance::HoldPos: return 4096;
	case ETCStance::AutoRetreat: return 8192;
	default: return 2048;
	}
}

const TCHAR* TCStance::Name(ETCStance Stance)
{
	switch (Stance)
	{
	case ETCStance::HoldPos: return TEXT("Hold position");
	case ETCStance::AutoRetreat: return TEXT("Auto-retreat");
	default: return TEXT("Hold fire");
	}
}

const TCHAR* TCStance::Tip(ETCStance Stance)
{
	// client/hud.js STANCE; the auto-retreat line writes Math.round(CFG.autoRetreat * 100) with CFG.autoRetreat 0.35
	switch (Stance)
	{
	case ETCStance::HoldPos: return TEXT("never move without an order, not even to cover");
	case ETCStance::AutoRetreat: return TEXT("run for home when below 35% strength");
	default: return TEXT("shoot only when given an attack order (snipers and guns stay hidden)");
	}
}

const TCHAR* TCStance::Label(ETCStance Stance)
{
	switch (Stance)
	{
	case ETCStance::HoldPos: return TEXT("Shift+G");
	case ETCStance::AutoRetreat: return TEXT("Shift+X");
	default: return TEXT("Shift+F");
	}
}

bool TCStance::FromKey(const FString& Key, ETCStance& Out)
{
	for (ETCStance S : All)
	{
		if (Key.Equals(TCStance::Key(S), ESearchCase::CaseSensitive)) { Out = S; return true; }
	}
	return false;
}

const TCHAR* LexToString(ETCArmyCommand Kind)
{
	switch (Kind)
	{
	case ETCArmyCommand::Retreat: return TEXT("retreat");
	case ETCArmyCommand::Stance: return TEXT("stance");
	case ETCArmyCommand::ArmRally: return TEXT("arm-rally");
	case ETCArmyCommand::Rally: return TEXT("rally");
	default: return TEXT("none");
	}
}

const TCHAR* LexToString(ETCArmyKey Key)
{
	switch (Key)
	{
	case ETCArmyKey::Retreat: return TEXT("retreat");
	case ETCArmyKey::HoldFire: return TEXT("stance:holdFire");
	case ETCArmyKey::HoldPos: return TEXT("stance:holdPos");
	case ETCArmyKey::AutoRetreat: return TEXT("stance:autoRetreat");
	case ETCArmyKey::Rally: return TEXT("rally");
	default: return TEXT("none");
	}
}

FTCOrderPlan FTCArmyPlan::ToOrderPlan() const
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
	TArray<TSharedPtr<FJsonValue>> ArmyIdArray(const TArray<int32>& Ids)
	{
		TArray<TSharedPtr<FJsonValue>> Out;
		Out.Reserve(Ids.Num());
		for (int32 Id : Ids) Out.Add(MakeShared<FJsonValueNumber>(Id));
		return Out;
	}

	// client/availability.js availability's opening gate, as FTCRecruitment::Gate words it for this client: no snapshot
	// yet, a spectator, or (this client's own case) a seat the lobby has not confirmed. Empty when orders may be shown.
	FString ArmyGate(const FTCMatchState& State, bool& bNative)
	{
		bNative = false;
		if (!State.bHasMatch || State.Snapshots == 0) return TEXT("Waiting for the army");
		if (State.bSpectator) return TEXT("You are spectating");
		if (!State.SeatConfirmed()) { bNative = true; return TEXT("Waiting for the lobby to confirm this seat"); }
		return FString();
	}

	// The recruit table (Makes) is generated at FTCRecruitment::ReferencePin; any other unit table means it may not
	// describe the units this client reads.
	bool ArmyRecruitTableMatches(const FTCUnitTable& Table, FString& OutWhy)
	{
		if (Table.Reference == FTCRecruitment::ReferencePin) return true;
		OutWhy = FString::Printf(TEXT("Rally: the recruit data is for reference %s, the unit table is for %s"), FTCRecruitment::ReferencePin,
			Table.Reference.IsEmpty() ? TEXT("an unknown reference") : *Table.Reference);
		return false;
	}

	// Selected own units whose type makes units, in the selection's order (client/main.js rallyAt: productionAccess and
	// UNITS[type].makes?.length; native selections hold only this seat's units, so productionAccess is ownership).
	// bRequireBuilding: client/orders.js selection also asks for a building.
	TArray<int32> ArmyProductionIds(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, bool bRequireBuilding)
	{
		TArray<int32> Out;
		for (int32 Id : Selected)
		{
			const FTCUnit* U = State.Units.Find(Id);
			if (!U || !State.IsOwn(*U) || !TCArmyCommands::MakesUnits(U->Type)) continue;
			if (bRequireBuilding)
			{
				const FTCUnitDef* D = Table.Find(U->Type);
				if (!D || !D->bBuilding) continue;
			}
			Out.Add(Id);
		}
		return Out;
	}

	TSharedRef<FJsonObject> ArmyRallyMessage(const TArray<int32>& Ids, FVector2D At)
	{
		// client/main.js rallyAt: { t: 'rally', ...(ids.length ? { ids } : {}), x, z }
		TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
		M->SetStringField(TEXT("t"), TEXT("rally"));
		if (Ids.Num()) M->SetArrayField(TEXT("ids"), ArmyIdArray(Ids));
		M->SetNumberField(TEXT("x"), At.X);
		M->SetNumberField(TEXT("z"), At.Y);
		return M;
	}
}

bool TCArmyCommands::ClassicLobby(const FTCMatchState& State)
{
	// client/main.js classicMode: ['classic', 'world'].includes(lobbyState?.mode) || !!lastStart?.map?.world
	return State.Mode == TEXT("classic") || State.Mode == TEXT("world") || State.Map.bWorld;
}

bool TCArmyCommands::TakesSelectionOrder(const FTCMatchState& State, const FTCUnit& U, const FTCUnitDef* Def)
{
	// shared/sim.js command mine: u.owner === slot && !u.operative && !structure && !rail && !u.riding (forced logistics
	// is not in the snapshot; see FTCArmyPlan::Eligible)
	return Def && State.IsOwn(U) && !(U.Flags & TCFlags::Operative) && !(U.Flags & TCFlags::Riding) && !Def->bStructure && !Def->bRail;
}

bool TCArmyCommands::MakesUnits(const FString& Type)
{
	const FTCRecruitDef* D = FTCRecruitment::Find(Type);
	return D && D->Makes.Num() > 0;
}

ETCArmyKey TCArmyCommands::KeyAction(const FKey& Key, bool bShift, bool bCtrl, bool bAlt)
{
	// client/keys.js match: code, shift, ctrl (or meta) and alt must all equal the binding's
	if (bCtrl || bAlt) return ETCArmyKey::None;
	if (!bShift) return Key == EKeys::R ? ETCArmyKey::Retreat : ETCArmyKey::None;
	if (Key == EKeys::F) return ETCArmyKey::HoldFire;
	if (Key == EKeys::G) return ETCArmyKey::HoldPos;
	if (Key == EKeys::X) return ETCArmyKey::AutoRetreat;
	if (Key == EKeys::H) return ETCArmyKey::Rally;
	return ETCArmyKey::None;
}

FTCArmyPlan TCArmyCommands::PlanRetreat(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected)
{
	FTCArmyPlan Plan;
	// client/main.js retreat: if (selected.size) sendCmd({ t: 'retreat', ids: [...selected] })
	if (!Selected.Num()) return Plan;
	TArray<int32> Ids;
	for (int32 Id : Selected)
	{
		Ids.Add(Id);
		const FTCUnit* U = State.Units.Find(Id);
		if (U && TakesSelectionOrder(State, *U, Table.Find(U->Type))) Plan.Eligible++;
	}
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("retreat"));
	M->SetArrayField(TEXT("ids"), ArmyIdArray(Ids));
	Plan.Kind = ETCArmyCommand::Retreat;
	Plan.Message = M;
	Plan.Units = Ids.Num();
	// shared/sim.js: if (!ids.some(mine)) return 'needs'. Convoy trucks are routed to their nearest supply source first
	// (shared/supply-convoys.js commandConvoy), and one without a route there refuses the whole order with 'blocked'.
	if (!Plan.Eligible) Plan.PredictedDeny = TEXT("needs");
	return Plan;
}

FTCArmyPlan TCArmyCommands::PlanStance(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, ETCStance Stance)
{
	FTCArmyPlan Plan;
	Plan.Stance = Stance;
	// client/main.js toggleStance: us = the selected units that are ours and not structures
	const int32 Bit = TCStance::Bit(Stance);
	TArray<int32> Ids;
	bool bAllHave = true;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		const FTCUnitDef* D = U ? Table.Find(U->Type) : nullptr;
		if (!U || !D || !State.IsOwn(*U) || D->bStructure) continue;
		Ids.Add(Id);
		bAllHave &= (U->Flags & Bit) != 0;
		if (TakesSelectionOrder(State, *U, D)) Plan.Eligible++;
	}
	if (!Ids.Num())
	{
		// the key path returns silently; a HUD button shows availability's sentence
		bool bNative = false;
		const FString Why = ArmyGate(State, bNative);
		Plan.Note = Why.IsEmpty() ? FString(TEXT("Select a unit")) : Why;
		Plan.bNative = bNative;
		return Plan;
	}
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("stance"));
	M->SetArrayField(TEXT("ids"), ArmyIdArray(Ids));
	M->SetStringField(TEXT("key"), TCStance::Key(Stance));
	M->SetBoolField(TEXT("on"), !bAllHave);
	Plan.Kind = ETCArmyCommand::Stance;
	Plan.Message = M;
	Plan.Units = Ids.Num();
	Plan.bStanceOn = !bAllHave;
	// shared/sim.js: const us = ids.map(mine).filter(Boolean); if (!us.length) return 'needs'
	if (!Plan.Eligible) Plan.PredictedDeny = TEXT("needs");
	return Plan;
}

FTCArmyPlan TCArmyCommands::PlanStance(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, ETCArmyKey Key)
{
	switch (Key)
	{
	case ETCArmyKey::HoldFire: return PlanStance(State, Table, Selected, ETCStance::HoldFire);
	case ETCArmyKey::HoldPos: return PlanStance(State, Table, Selected, ETCStance::HoldPos);
	case ETCArmyKey::AutoRetreat: return PlanStance(State, Table, Selected, ETCStance::AutoRetreat);
	default: return FTCArmyPlan();
	}
}

FTCArmyPlan TCArmyCommands::PlanArmRally(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected)
{
	FTCArmyPlan Plan;
	// client/main.js startRally: if (!lastSnap) return
	if (!State.bHasMatch || State.Snapshots == 0) return Plan;
	if (!ArmyRecruitTableMatches(Table, Plan.Note)) { Plan.bNative = true; return Plan; }
	if (ClassicLobby(State) && !ArmyProductionIds(State, Table, Selected, false).Num())
	{
		Plan.Note = TEXT("Select a Production Building to set its rally");
		return Plan;
	}
	// client/main.js setAim: if (observing()) return (no aim, no hint)
	if (State.bSpectator || !State.SeatConfirmed())
	{
		Plan.Note = State.bSpectator ? TEXT("Rally: spectators give no orders") : TEXT("Rally: this seat is not confirmed by the server yet");
		Plan.bNative = true;
		return Plan;
	}
	Plan.Kind = ETCArmyCommand::ArmRally;
	Plan.bArmAim = true;
	Plan.Note = RallyAimHint;
	return Plan;
}

FTCArmyPlan TCArmyCommands::PlanRallyAt(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, bool bHasGround, FVector2D Ground)
{
	FTCArmyPlan Plan;
	if (!bHasGround)
	{
		// client/main.js mousedown while targeting: if (!g) { feedback.show(denySentence('blocked')); return; }
		Plan.Note = TCOrders::DenySentence(TEXT("blocked"), TEXT("rally"));
		Plan.bKeepAim = true;
		return Plan;
	}
	if (!ArmyRecruitTableMatches(Table, Plan.Note)) { Plan.bNative = true; Plan.bCancelAim = true; return Plan; }
	const TArray<int32> Ids = ArmyProductionIds(State, Table, Selected, false);
	Plan.Kind = ETCArmyCommand::Rally;
	Plan.Message = ArmyRallyMessage(Ids, Ground);
	Plan.Units = Ids.Num();
	Plan.Eligible = Ids.Num();
	Plan.RallyBuildings = Ids.Num();
	Plan.At = Ground;
	Plan.bCancelAim = true;
	// shared/sim.js command 'rally', with the lobby's mode standing in for g.mode.kind (isConstructionMode):
	//   Classic and World name buildings, and with none named the list is empty and the answer is 'unseen';
	//   elsewhere, with none named, the army's rally needs a spot inside the map that is not impassable (the client
	//   cannot see the MOVE cell flag, so only the map's bounds are predicted).
	const double MW = State.Map.W * FTCMatchState::CellMetres, MH = State.Map.H * FTCMatchState::CellMetres;
	if (!Ids.Num())
	{
		if (ClassicLobby(State)) Plan.PredictedDeny = TEXT("unseen");
		else if (!(Ground.X >= 0 && Ground.Y >= 0 && Ground.X < MW && Ground.Y < MH)) Plan.PredictedDeny = TEXT("blocked");
	}
	return Plan;
}

TSharedPtr<FJsonObject> TCArmyCommands::RightClickRallyCommand(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, int32* OutBuildings)
{
	if (OutBuildings) *OutBuildings = 0;
	// client/orders.js dispatch: if (buildings.length && g) add({ t: 'rally', ids, x, z })
	FString Why;
	if (!Cursor.bHasGround || !ArmyRecruitTableMatches(Table, Why)) return nullptr;
	const TArray<int32> Ids = ArmyProductionIds(State, Table, Selected, true);
	if (!Ids.Num()) return nullptr;
	if (OutBuildings) *OutBuildings = Ids.Num();
	return ArmyRallyMessage(Ids, Cursor.Ground);
}

FTCArmyPlan TCArmyCommands::PlanRightClickRally(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bQueue)
{
	FTCArmyPlan Plan;
	int32 Buildings = 0;
	const TSharedPtr<FJsonObject> Command = RightClickRallyCommand(State, Table, Selected, Cursor, &Buildings);
	if (!Command.IsValid())
	{
		if (Cursor.bHasGround && Table.Reference != FTCRecruitment::ReferencePin) { ArmyRecruitTableMatches(Table, Plan.Note); Plan.bNative = true; }
		return Plan;
	}
	// client/orders.js: a single command goes out as { ...commands[0], queue }
	Command->SetBoolField(TEXT("queue"), bQueue);
	Plan.Kind = ETCArmyCommand::Rally;
	Plan.Message = Command;
	Plan.Units = Buildings;
	Plan.Eligible = Buildings;
	Plan.RallyBuildings = Buildings;
	Plan.At = Cursor.Ground;
	return Plan;
}

bool TCArmyCommands::ShowsOrdersPanel(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected)
{
	// client/hud.js drawOrders over selUnits (the selected ids that are held): the truck panel for an all-truck
	// selection, nothing for none or for all buildings
	int32 Held = 0;
	bool bAllTrucks = true, bAllBuildings = true;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (!U) continue;
		Held++;
		const FTCUnitDef* D = Table.Find(U->Type);
		bAllTrucks &= U->Type == TEXT("truck");
		bAllBuildings &= D && D->bBuilding;
	}
	return Held > 0 && !bAllTrucks && !bAllBuildings;
}

bool TCArmyCommands::ShowsRallyButton(const FTCMatchState& State, const FTCRecruitment& Recruit, const TSet<int32>& Selected)
{
	// client/hud.js update: drawRecruit runs outside Classic and World for every snapshot unless the selection is all
	// trucks. The browser then keeps the card it had (Rally button included) unless logistics is on, which empties it;
	// this client does not hold the logistics switch, so it hides the button with the card (FTCRecruitment::Card).
	if (!State.bHasMatch || !Recruit.HasSnapshot() || Recruit.Classic()) return false;
	int32 Held = 0;
	bool bAllTrucks = true;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (!U) continue;
		Held++;
		bAllTrucks &= U->Type == TEXT("truck");
	}
	return !(Held > 0 && bAllTrucks);
}

FTCStanceButton TCArmyCommands::StanceButton(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, ETCStance Stance)
{
	FTCStanceButton B;
	B.Stance = Stance;
	const int32 Bit = TCStance::Bit(Stance);
	bool bAnyOwnMobile = false;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (!U) continue;
		// client/hud.js: on = sel.filter((v) => v.flags & bit).length over every held selected unit
		B.Of++;
		if (U->Flags & Bit) B.On++;
		// client/availability.js: selected = own snapshot units named in ids; stance needs one that is not a structure
		const FTCUnitDef* D = Table.Find(U->Type);
		if (D && !D->bStructure && State.IsOwn(*U)) bAnyOwnMobile = true;
	}
	B.Text = !B.On ? FString(TEXT("Off")) : B.On == B.Of ? FString(TEXT("On")) : FString::Printf(TEXT("%d/%d"), B.On, B.Of);
	B.bLit = B.On > 0;
	B.Reason = ArmyGate(State, B.bNative);
	if (B.Reason.IsEmpty() && !bAnyOwnMobile) B.Reason = TEXT("Select a unit");
	B.bOk = B.Reason.IsEmpty();
	return B;
}

bool TCArmyCommands::ReadArmyRally(const FJsonObject& Snapshot, FVector2D& Out)
{
	const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
	if (!Snapshot.TryGetArrayField(TEXT("rally"), Arr) || !Arr || Arr->Num() < 2) return false;
	const TSharedPtr<FJsonValue>& X = (*Arr)[0];
	const TSharedPtr<FJsonValue>& Z = (*Arr)[1];
	if (!X.IsValid() || !Z.IsValid() || X->Type != EJson::Number || Z->Type != EJson::Number) return false;
	Out = FVector2D(X->AsNumber(), Z->AsNumber());
	return true;
}
