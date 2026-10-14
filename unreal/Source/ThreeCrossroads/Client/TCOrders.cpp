#include "Client/TCOrders.h"
#include "Client/TCFormation.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"

const TCHAR* LexToString(ETCOrderKind Kind)
{
	switch (Kind)
	{
	case ETCOrderKind::Move: return TEXT("move");
	case ETCOrderKind::AttackMove: return TEXT("attack-move");
	case ETCOrderKind::Attack: return TEXT("attack");
	case ETCOrderKind::Stop: return TEXT("stop");
	case ETCOrderKind::Unsupported: return TEXT("unsupported");
	default: return TEXT("none");
	}
}

namespace
{
	struct FTroop { const FTCUnit* U; const FTCUnitDef* D; };

	TArray<TSharedPtr<FJsonValue>> IdArray(const TArray<int32>& Ids)
	{
		TArray<TSharedPtr<FJsonValue>> Out;
		for (int32 Id : Ids) Out.Add(MakeShared<FJsonValueNumber>(Id));
		return Out;
	}

	// client/main.js formation(sel, g): a plain click faces the way the units travel (from their centre to the click),
	// in the default line at spread 1, infantry snapped into trenches, every spot kept 1 m inside the map.
	TArray<TSharedPtr<FJsonValue>> Formation(const FTCMatchState& State, const TArray<FTroop>& Troops, FVector2D G)
	{
		TArray<FTCFormationItem> Items;
		double CX = 0, CZ = 0;
		for (const FTroop& T : Troops)
		{
			Items.Add({ T.U->Id, T.U->X, T.U->Z, TCFormation::Slot(T.D->bInfantry, T.D->Radius), T.D->bBack, T.D->bInfantry });
			CX += T.U->X; CZ += T.U->Z;
		}
		TArray<TSharedPtr<FJsonValue>> Orders;
		if (!Items.Num()) return Orders;
		CX /= Items.Num(); CZ /= Items.Num();
		// Unreal's atan2 (the platform's libm), not V8's: it can differ from the browser's in the last bit, and so move a
		// spot by as much. The Rust core's math module is not in the C interface yet (ABI v1 has formation calls only).
		const double Face = FMath::Atan2(G.Y - CZ, G.X - CX);
		// slotSize and facingSpots from the Rust core when WW2Core is ready, else the interim port (TCFormation.h)
		TArray<FTCSpot> Spots = TCFormation::Spots(Items, G.X, G.Y, Face, 0);
		const FTCMap& Map = State.Map;
		TCFormation::SnapToTrench(Spots, Items, [&Map](int32 X, int32 Y)
		{
			return X >= 0 && Y >= 0 && X < Map.W && Y < Map.H && Map.Known[Y * Map.W + X] && Map.Chars[Y * Map.W + X] == 'T';
		});
		const double MW = Map.W * FTCMatchState::CellMetres, MH = Map.H * FTCMatchState::CellMetres;
		for (const FTCSpot& S : Spots)
		{
			TArray<TSharedPtr<FJsonValue>> Row;
			Row.Add(MakeShared<FJsonValueNumber>(S.Id));
			Row.Add(MakeShared<FJsonValueNumber>(FMath::Clamp(S.X, 1.0, MW - 1)));
			Row.Add(MakeShared<FJsonValueNumber>(FMath::Clamp(S.Z, 1.0, MH - 1)));
			Orders.Add(MakeShared<FJsonValueArray>(Row));
		}
		return Orders;
	}

	TSharedRef<FJsonObject> MoveCommand(const TCHAR* T, TArray<TSharedPtr<FJsonValue>> Orders)
	{
		TSharedRef<FJsonObject> C = MakeShared<FJsonObject>();
		C->SetStringField(TEXT("t"), T);
		C->SetArrayField(TEXT("orders"), Orders);
		return C;
	}
}

FTCOrderPlan TCOrders::Plan(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bAttack, bool bQueue)
{
	FTCOrderPlan Plan;
	// client/orders.js selection: troops are the own selected units that are not structures. This slice never sends a
	// move to aircraft or trains (they get no move in the native client yet), and riders are already deselected.
	TArray<FTroop> Troops, Trucks, Planes;
	bool bAnyInfantry = false, bOwnBuilding = false;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		const FTCUnitDef* D = U ? Table.Find(U->Type) : nullptr;
		if (!U || !D || !State.IsOwn(*U)) continue;
		if (D->bStructure) { bOwnBuilding |= D->bBuilding; continue; }
		Troops.Add({ U, D });
		if (U->Type == TEXT("truck")) Trucks.Add({ U, D });
		if (D->bAir) Planes.Add({ U, D });
		bAnyInfantry |= D->bInfantry;
	}
	if (!Troops.Num())
	{
		// client/orders.js: selected Production Buildings get a rally point instead
		if (bOwnBuilding && Cursor.bHasGround) { Plan.Kind = ETCOrderKind::Unsupported; Plan.Why = TEXT("a rally point for the selected building (not in this slice yet)"); }
		return Plan;
	}

	// client/orders.js kind, in its order. Assist, entrench and garrison need data this client does not hold yet (the
	// builder types, planned works, building meshes): those clicks fall through to attack or move. See README.
	const FTCUnit* FriendU = Cursor.Friend ? State.Units.Find(Cursor.Friend) : nullptr;
	const FTCUnitDef* FriendD = FriendU ? Table.Find(FriendU->Type) : nullptr;
	if (Trucks.Num() == Troops.Num() && (Cursor.Friend || Cursor.Building))
	{
		Plan.Kind = ETCOrderKind::Unsupported; Plan.Why = TEXT("a supply run for the trucks (not in this slice yet)"); return Plan;
	}
	if (Cursor.Friend && Planes.Num() == Troops.Num())
	{
		Plan.Kind = ETCOrderKind::Unsupported; Plan.Why = TEXT("an escort for the planes (not in this slice yet)"); return Plan;
	}
	if (FriendU && FriendD && FriendU->Owner == State.You && FriendD->Carries > 0 && bAnyInfantry)
	{
		Plan.Kind = ETCOrderKind::Unsupported; Plan.Why = FString::Printf(TEXT("boarding the %s (not in this slice yet)"), *FriendD->Name); return Plan;
	}
	const FTCUnit* Enemy = Trucks.Num() == Troops.Num() ? nullptr : (Cursor.Enemy ? State.Units.Find(Cursor.Enemy) : nullptr);

	auto Movable = [](const FTroop& T) { return !T.D->bAir && !T.D->bRail; };
	TArray<TSharedRef<FJsonObject>> Commands;
	if (Enemy)
	{
		// order 'attack': every troop but the trucks attacks; trucks move to the ground under the click
		TArray<int32> Fighters;
		for (const FTroop& T : Troops) if (T.U->Type != TEXT("truck")) Fighters.Add(T.U->Id);
		TSharedRef<FJsonObject> A = MakeShared<FJsonObject>();
		A->SetStringField(TEXT("t"), TEXT("attack"));
		A->SetArrayField(TEXT("ids"), IdArray(Fighters));
		A->SetNumberField(TEXT("target"), Enemy->Id);
		Commands.Add(A);
		Plan.Units = Fighters.Num();
		TArray<FTroop> MovingTrucks = Trucks.FilterByPredicate(Movable);
		if (MovingTrucks.Num() && Cursor.bHasGround)
		{
			Commands.Add(MoveCommand(TEXT("move"), Formation(State, MovingTrucks, Cursor.Ground)));
			Plan.Units += MovingTrucks.Num();
		}
		Plan.Kind = ETCOrderKind::Attack; Plan.TargetId = Enemy->Id; Plan.At = FVector2D(Enemy->X, Enemy->Z);
		Plan.Color = FColor(0xff, 0x40, 0x30);
	}
	else
	{
		if (!Cursor.bHasGround) return Plan;
		// order 'move': an attack-move unless every troop is a truck; trucks then get a plain move of their own
		const bool bAmove = Trucks.Num() != Troops.Num() && bAttack;
		TArray<FTroop> Marching;
		for (const FTroop& T : Troops) if (Movable(T) && !(bAmove && T.U->Type == TEXT("truck"))) Marching.Add(T);
		if (!Marching.Num())
		{
			Plan.Kind = ETCOrderKind::Unsupported;
			Plan.Why = FString::Printf(TEXT("none of the %d selected units takes a move order in this slice (aircraft and trains)"), Troops.Num());
			return Plan;
		}
		TSharedRef<FJsonObject> Main = MoveCommand(bAmove ? TEXT("amove") : TEXT("move"), Formation(State, Marching, Cursor.Ground));
		Main->SetBoolField(TEXT("together"), true); // the Formation menu's default: march at the slowest unit's pace
		Commands.Add(Main);
		Plan.Units = Marching.Num();
		TArray<FTroop> MovingTrucks = Trucks.FilterByPredicate(Movable);
		if (bAmove && MovingTrucks.Num())
		{
			Commands.Add(MoveCommand(TEXT("move"), Formation(State, MovingTrucks, Cursor.Ground)));
			Plan.Units += MovingTrucks.Num();
		}
		Plan.Kind = bAmove ? ETCOrderKind::AttackMove : ETCOrderKind::Move;
		Plan.At = Cursor.Ground;
		if (bAmove) Plan.Color = FColor(0xff, 0x9a, 0x40);
	}
	if (Commands.Num() == 1)
	{
		Commands[0]->SetBoolField(TEXT("queue"), bQueue);
		Plan.Message = Commands[0];
	}
	else
	{
		TArray<TSharedPtr<FJsonValue>> Parts;
		for (const TSharedRef<FJsonObject>& C : Commands) Parts.Add(MakeShared<FJsonValueObject>(C));
		TSharedRef<FJsonObject> Group = MakeShared<FJsonObject>();
		Group->SetStringField(TEXT("t"), TEXT("orders"));
		Group->SetBoolField(TEXT("queue"), bQueue);
		Group->SetArrayField(TEXT("commands"), Parts);
		Plan.Message = Group;
	}
	return Plan;
}

FTCOrderPlan TCOrders::PlanStop(const TSet<int32>& Selected)
{
	FTCOrderPlan Plan;
	if (!Selected.Num()) return Plan;
	TSharedRef<FJsonObject> S = MakeShared<FJsonObject>();
	S->SetStringField(TEXT("t"), TEXT("stop"));
	S->SetArrayField(TEXT("ids"), IdArray(Selected.Array()));
	Plan.Kind = ETCOrderKind::Stop;
	Plan.Message = S;
	Plan.Units = Selected.Num();
	return Plan;
}

FString TCOrders::DenySentence(const FString& Reason, const FString& Cmd)
{
	// client/availability.js DENY_SENTENCES, all 28 codes at the pin. A code missing here gets the browser's fallback
	// sentence; the raw code stays in the log.
	static const TMap<FString, FString> Sentences = {
		{ TEXT("mp"), TEXT("Not enough manpower") }, { TEXT("mun"), TEXT("Not enough munitions") }, { TEXT("fuel"), TEXT("Not enough fuel") },
		{ TEXT("ammo"), TEXT("Not enough carried ammunition") }, { TEXT("pop"), TEXT("The army is at its limit") },
		{ TEXT("cooldown"), TEXT("That order is still on cooldown") }, { TEXT("unseen"), TEXT("That target is not visible") },
		{ TEXT("notVisible"), TEXT("That spot is not visible") }, { TEXT("blocked"), TEXT("That spot is blocked or uneven") },
		{ TEXT("needs"), TEXT("The required unit or building is missing") }, { TEXT("max"), TEXT("That unit or position is at its limit") },
		{ TEXT("suddenDeath"), TEXT("Not during Sudden Death") }, { TEXT("queueFull"), TEXT("The training queue is full") },
		{ TEXT("ordersFull"), TEXT("This unit already has 8 queued orders") }, { TEXT("retreating"), TEXT("That squad is retreating") },
		{ TEXT("noBuilders"), TEXT("Select a builder squad") }, { TEXT("noCover"), TEXT("No cover within reach") },
		{ TEXT("notWaiting"), TEXT("That recruit is no longer waiting") },
		{ TEXT("scenario"), TEXT("This scenario cannot start with the selected sides, factions or mode") },
		{ TEXT("territory"), TEXT("Build inside territory your team owns") }, { TEXT("coast"), TEXT("A Shipyard needs open water beside it") },
		{ TEXT("tier"), TEXT("Needs a higher HQ tier") }, { TEXT("rail"), TEXT("Too close to the railway or a station") },
		{ TEXT("save"), TEXT("The match could not be saved or loaded") },
		// CFG.rail.reach (30) and CFG.paraRange (300) at the pin, written into the browser's sentences
		{ TEXT("station"), TEXT("Rail moves start within 30 m of one of your stations and go to another station on your land") },
		{ TEXT("railCut"), TEXT("No open line: every region on the way must be yours and its bridges standing") },
		{ TEXT("airfield"), TEXT("Paratroopers fly from one of your finished Airfields, at most 300 m away") },
		{ TEXT("shore"), TEXT("Too far from the shore to land") },
	};
	// The server answers queueFull for both a full training queue (buy) and a full order queue (every other command).
	const FString Code = Reason == TEXT("queueFull") && !Cmd.IsEmpty() && Cmd != TEXT("buy") ? FString(TEXT("ordersFull")) : Reason;
	const FString* S = Sentences.Find(Code);
	return S ? *S : FString(TEXT("That order cannot happen"));
}
