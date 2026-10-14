#include "Client/TCOrders.h"
#include "ThreeCrossroads.h"
#include "Client/TCFormation.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "Policies/CondensedJsonPrintPolicy.h"
#include "Client/TCLocale.h"
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCFormationControls.h"
#include "World/TCTerrain.h"
#include "UnrealClient.h"
#include "World/TCWorldObjects.h"

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

// ---------------------------------------------------------------------------------------------------------------------
// Round 16: message encoding shared by the dispatch paths

TSharedPtr<FJsonObject> TCOrders::Wrap(const TArray<TSharedPtr<FJsonObject>>& Commands, bool bQueue)
{
	TArray<TSharedPtr<FJsonObject>> Parts;
	for (const TSharedPtr<FJsonObject>& C : Commands) if (C.IsValid()) Parts.Add(C);
	if (!Parts.Num()) return nullptr;
	if (Parts.Num() == 1)
	{
		// { ...commands[0], queue }: the spread keeps the command's own fields first, the flag last
		TSharedPtr<FJsonObject> One = MakeShared<FJsonObject>(*Parts[0]);
		One->SetBoolField(TEXT("queue"), bQueue);
		return One;
	}
	TArray<TSharedPtr<FJsonValue>> Values;
	for (const TSharedPtr<FJsonObject>& C : Parts) Values.Add(MakeShared<FJsonValueObject>(C));
	TSharedPtr<FJsonObject> Group = MakeShared<FJsonObject>();
	Group->SetStringField(TEXT("t"), TEXT("orders"));
	Group->SetBoolField(TEXT("queue"), bQueue);
	Group->SetArrayField(TEXT("commands"), Values);
	return Group;
}

TSharedPtr<FJsonObject> TCOrders::EscortCommand(const TArray<int32>& Planes, int32 Target)
{
	if (!Planes.Num() || !Target) return nullptr;
	TSharedPtr<FJsonObject> O = MakeShared<FJsonObject>();
	O->SetStringField(TEXT("t"), TEXT("escort"));
	O->SetArrayField(TEXT("ids"), IdArray(Planes));
	O->SetNumberField(TEXT("target"), Target);
	return O;
}

TSharedPtr<FJsonObject> TCOrders::RailCommand(const TArray<int32>& Ids, double X, double Z)
{
	if (!Ids.Num()) return nullptr;
	TSharedPtr<FJsonObject> O = MakeShared<FJsonObject>();
	O->SetStringField(TEXT("t"), TEXT("rail"));
	O->SetArrayField(TEXT("ids"), IdArray(Ids));
	O->SetNumberField(TEXT("x"), X);
	O->SetNumberField(TEXT("z"), Z);
	return O;
}

FString TCOrders::ToJson(const TSharedPtr<FJsonObject>& Message)
{
	if (!Message.IsValid()) return FString();
	FString Out;
	const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> W = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
	FJsonSerializer::Serialize(Message.ToSharedRef(), W);
	return Out;
}

// ---------------------------------------------------------------------------------------------------------------------
// Round 16: client/markers.js planLayer

uint32 TCOrders::PlanHex(int32 Kind)
{
	// PLAN_COLORS = { 1: 0x5fa4f2, 2: 0xf0932f, 3: 0xf4f2ea, 4: 0xe5483b, 5: 0xe5483b, 6: 0xe5483b, 7: 0xe6c350, 8: 0xe6c350, 9: 0x5fa4f2 }
	switch (Kind)
	{
	case 2: return 0xf0932f;
	case 3: return 0xf4f2ea;
	case 4: case 5: case 6: return 0xe5483b;
	case 7: case 8: return 0xe6c350;
	default: return 0x5fa4f2;
	}
}

namespace
{
	// markers.js ORDER_DASH, ARROW_LEN, ARROW_HALF, MOVE_COLOR, ATTACK_COLOR
	constexpr double ArrowLen = 1.7, ArrowHalf = 0.72;
	constexpr uint32 MoveHex = 0x5fa4f2, AttackHex = 0xe5483b;

	// The planLayer closure: P (the polyline being built, as points), the colour set on both batches, and the output.
	struct FPlanLayer
	{
		TArray<FVector2D> P;
		uint32 Hex = MoveHex;
		float Alpha = 1.f;
		TArray<FTCMarkPrim>& Out;
		explicit FPlanLayer(TArray<FTCMarkPrim>& InOut) : Out(InOut) {}

		void Color(uint32 H, float A) { Hex = H; Alpha = A; }
		static double Hypot(double X, double Z) { return FMath::Sqrt(X * X + Z * Z); }
		// trimEnd: cut length d off the end of the first n points; returns how many remain
		int32 TrimEnd(int32 N, double D)
		{
			while (N >= 2 && D > 0)
			{
				const FVector2D A = P[N - 2], B = P[N - 1];
				const double L = Hypot(B.X - A.X, B.Y - A.Y);
				if (L <= D) { N--; D -= L; continue; }
				P[N - 1] = FVector2D(B.X + ((A.X - B.X) / L) * D, B.Y + ((A.Y - B.Y) / L) * D);
				D = 0;
			}
			return N;
		}
		int32 TrimStart(int32 N, double D)
		{
			int32 I = 0;
			while (N - I >= 2 && D > 0)
			{
				const FVector2D A = P[I], B = P[I + 1];
				const double L = Hypot(B.X - A.X, B.Y - A.Y);
				if (L <= D) { I++; D -= L; continue; }
				P[I] = FVector2D(A.X + ((B.X - A.X) / L) * D, A.Y + ((B.Y - A.Y) / L) * D);
				D = 0;
			}
			if (I) { for (int32 K = I; K < N; K++) P[K - I] = P[K]; N -= I; }
			return N;
		}
		double LengthOf(int32 N) const
		{
			double L = 0;
			for (int32 I = 1; I < N; I++) L += Hypot(P[I].X - P[I - 1].X, P[I].Y - P[I - 1].Y);
			return L;
		}
		void Stroke(bool bDashed, int32 N, double Width)
		{
			FTCMarkPrim M;
			M.Op = FTCMarkPrim::EOp::Line; M.bDashed = bDashed; M.Hex = Hex; M.Alpha = Alpha; M.Width = Width;
			for (int32 I = 0; I < N; I++) M.Points.Add(P[I]);
			Out.Add(MoveTemp(M));
		}
		void Arrow(FVector2D Tip, FVector2D Dir, double Len, double Half)
		{
			FTCMarkPrim M;
			M.Op = FTCMarkPrim::EOp::Arrow; M.Hex = Hex; M.Alpha = Alpha; M.At = Tip; M.Dir = Dir; M.Len = Len; M.Half = Half;
			Out.Add(MoveTemp(M));
		}
		void Loop(FVector2D At, double R, double Width)
		{
			FTCMarkPrim M;
			M.Op = FTCMarkPrim::EOp::Ring; M.Hex = Hex; M.Alpha = Alpha; M.At = At; M.Radius = R; M.Width = Width;
			Out.Add(MoveTemp(M));
		}
		// leg: from a unit (starting at its ring) to a goal, cut short at the goal's ring, ending in an arrowhead
		void Leg(bool bDashed, int32 N, double FromR, double ToR, double Width)
		{
			N = TrimStart(N, FromR);
			if (LengthOf(N) < ToR + ArrowLen * 0.8) return;
			N = TrimEnd(N, ToR);
			const FVector2D T = P[N - 1], D = T - P[N - 2];
			N = TrimEnd(N, ArrowLen * 0.85);
			if (N >= 2 && LengthOf(N) > 0.2) Stroke(bDashed, N, Width);
			Arrow(T, D, ArrowLen, ArrowHalf); // arrowheads always go in the solid batch
		}
		void Two(FVector2D A, FVector2D B) { P.SetNum(2); P[0] = A; P[1] = B; }
		// a rally point: a ring, a pole and a pennant drawn on the ground
		void Flag(FVector2D At)
		{
			Color(MoveHex, 0.95f);
			Loop(At, 1.1, 0.2);
			Two(At, FVector2D(At.X, At.Y - 3.2));
			Stroke(false, 2, 0.2);
			Arrow(FVector2D(At.X + 1.9, At.Y - 2.5), FVector2D(1, 0), 1.9, 0.6);
		}
	};
}

TArray<FTCMarkPrim> TCOrders::Markers(const TArray<int32>& Selected, const TMap<int32, FTCMarkUnit>& Units, const FVector2D* ArmyRally)
{
	TArray<FTCMarkPrim> Out;
	FPlanLayer L(Out);
	if (ArmyRally) L.Flag(*ArmyRally);
	for (const int32 Id : Selected)
	{
		const FTCMarkUnit* V = Units.Find(Id);
		if (!V) continue;
		const bool bPlan = V->PlanKind != 0;
		const double FromR = V->Radius + 0.5;
		if (V->bRally)
		{
			L.Color(MoveHex, 0.75f);
			L.Two(V->At, V->Rally);
			L.Leg(true, 2, FromR + 1, 1.5, 0.26);
			L.Flag(V->Rally);
		}
		// what it is shooting at right now (ordered or picked by itself), or the enemy it was told to attack
		const FTCMarkUnit* Shot = V->Target ? Units.Find(V->Target) : nullptr;
		const bool bT = Shot || V->PlanKind == 4;
		const FVector2D TAt = Shot ? Shot->At : V->PlanTo;
		const double TR = bT ? (Shot ? Shot->Radius : 1.5 /* UNITS.rifle.radius at the pin (Content/Data/units.json) */) + 1.2 : 0.0;
		const bool bOnOrder = bT && V->PlanKind == 4 && FPlanLayer::Hypot(TAt.X - V->PlanTo.X, TAt.Y - V->PlanTo.Y) < 3;
		if (bPlan)
		{
			const uint32 H = PlanHex(V->PlanKind);
			L.P.SetNum(1); L.P[0] = V->At;
			L.P.Append(V->Path);
			int32 N = L.P.Num();
			L.Color(H, 0.9f);
			if (V->PlanKind > 4)
			{
				// the route, then a dashed leg to whatever it is set on
				N = L.TrimStart(N, FromR);
				if (N >= 2 && L.LengthOf(N) > 0.5) L.Stroke(false, N, 0.34);
				const FVector2D Last = L.P[N - 1];
				L.Two(Last, V->PlanTo);
				L.Color(H, 0.8f); L.Leg(true, 2, 0, 1.5, 0.28);
			}
			else L.Leg(false, N, FromR, V->PlanKind == 4 ? (bOnOrder ? TR : 2.7) + 0.3 : 1.2, 0.34);
			if (V->PlanKind != 4) { L.Color(H, 0.95f); L.Loop(V->PlanTo, 1.0, 0.22); }
		}
		FVector2D Q = bPlan ? V->PlanTo : V->At;
		double QR = bPlan ? 1.0 : FromR;
		for (const FTCQueuedOrder& O : V->Orders)
		{
			const uint32 H = PlanHex(O.Kind);
			L.Color(H, 0.8f);
			L.Two(Q, FVector2D(O.X, O.Z));
			L.Leg(true, 2, QR, 1.2, 0.28);
			L.Color(H, 0.95f); L.Loop(FVector2D(O.X, O.Z), 1.0, 0.22);
			Q = FVector2D(O.X, O.Z); QR = 1.0;
		}
		if (bT)
		{
			if (!bOnOrder)
			{
				L.Color(AttackHex, 0.85f);
				L.Two(V->At, TAt);
				L.Leg(true, 2, FromR, TR + 0.3, 0.26);
			}
			L.Color(AttackHex, 0.95f); L.Loop(TAt, TR, 0.24);
		}
	}
	return Out;
}

TMap<int32, FTCMarkUnit> TCOrders::MarkUnits(const FTCMatchState& State, const FTCUnitTable& Table, const TArray<int32>& Selected,
	TFunctionRef<bool(int32, FVector2D&)> DrawnAt, TFunctionRef<bool(int32, FVector2D&)> RallyOf)
{
	TMap<int32, FTCMarkUnit> Out;
	auto Make = [&](int32 Id) -> FTCMarkUnit*
	{
		if (FTCMarkUnit* Have = Out.Find(Id)) return Have;
		const FTCUnit* U = State.Units.Find(Id);
		if (!U) return nullptr;
		FTCMarkUnit& M = Out.Add(Id);
		M.Id = Id;
		M.At = FVector2D(U->X, U->Z);
		DrawnAt(Id, M.At);
		const FTCUnitDef* D = Table.Find(U->Type);
		M.Radius = D ? D->Radius : 1.5;
		M.Target = U->Target;
		return &M;
	};
	TSet<int32> Wanted;
	for (const int32 Id : Selected) if (FTCMarkUnit* M = Make(Id)) { Wanted.Add(Id); if (M->Target) Make(M->Target); }
	// main.js: v.plan from s.plans, v.orders from s.orders (own units; the snapshot only carries the seat's own)
	for (const FTCPlan& P : State.Plans)
		if (Wanted.Contains(P.Id)) { FTCMarkUnit& M = Out[P.Id]; M.PlanKind = P.Kind; M.PlanTo = FVector2D(P.Tx, P.Tz); M.Path = P.Path; }
	for (const FTCWaitingOrders& W : State.WaitingOrders)
		if (Wanted.Contains(W.Id)) Out[W.Id].Orders = W.Orders;
	for (const int32 Id : Wanted)
	{
		FVector2D R;
		if (RallyOf(Id, R)) { FTCMarkUnit& M = Out[Id]; M.bRally = true; M.Rally = R; }
	}
	return Out;
}

// ---------------------------------------------------------------------------------------------------------------------
// Round 16: a garrisoned squad's roof bar

TArray<TCOrders::FBarRect> TCOrders::RoofBarRects(double HpFrac, int32 Supp, int32 Stars)
{
	TArray<FBarRect> Out;
	const double F = FMath::Max(0.0, HpFrac);
	Out.Add({ 0.0, 0.0, 2.4, 0.42 });                       // barBg
	Out.Add({ -1.15 * (1 - F), 0.07, 2.3 * F, 0.2 });        // hpBar
	if (Supp > 0) Out.Add({ -1.15 * (1 - Supp / 100.0), -0.11, 2.3 * Supp / 100.0, 0.1 }); // suppBar
	// stars: 0.32 squares turned 45 degrees at x -0.5, 0, 0.5, y 0.42 (their box corners turned: 0.32 * sqrt 2 across)
	const double Diamond = 0.32 * FMath::Sqrt(2.0);
	for (int32 I = 0; I < FMath::Clamp(Stars, 0, 3); I++) Out.Add({ -0.5 + 0.5 * I, 0.42, Diamond, Diamond });
	Out.Add({ -1.98, 0.08, 1.45, 1.45 });                    // markers.js symbolBadge
	return Out;
}

double TCOrders::RoofPickDistance(const TArray<FBox2D>& Bounds, FVector2D Pixel)
{
	double Best = TNumericLimits<double>::Max();
	for (const FBox2D& B : Bounds)
	{
		const double DX = FMath::Max3(B.Min.X - Pixel.X, 0.0, Pixel.X - B.Max.X), DY = FMath::Max3(B.Min.Y - Pixel.Y, 0.0, Pixel.Y - B.Max.Y);
		Best = FMath::Min(Best, FMath::Sqrt(DX * DX + DY * DY));
	}
	return Best;
}

bool TCOrders::RoofBoxHits(const TArray<FBox2D>& Bounds, const FBox2D& Box)
{
	// selection.js inside: a bounds point overlaps the box
	for (const FBox2D& B : Bounds)
		if (B.Max.X >= Box.Min.X && B.Min.X <= Box.Max.X && B.Max.Y >= Box.Min.Y && B.Min.Y <= Box.Max.Y) return true;
	return false;
}

TArray<FBox2D> TCOrders::RoofBarScreenBounds(const FVector& Anchor, const FVector& Right, const FVector& Up, const TArray<FBarRect>& Rects,
	TFunctionRef<bool(const FVector&, FVector2D&)> Project)
{
	TArray<FBox2D> Out;
	for (const FBarRect& R : Rects)
	{
		FBox2D B(ForceInit);
		bool bAll = true;
		for (int32 K = 0; K < 4 && bAll; K++)
		{
			const double X = R.X + ((K & 1) ? R.W : -R.W) / 2, Y = R.Y + ((K & 2) ? R.H : -R.H) / 2;
			FVector2D Px;
			bAll = Project(Anchor + Right * (X * 100.0) + Up * (Y * 100.0), Px);
			if (bAll) B += Px;
		}
		if (bAll && B.bIsValid) Out.Add(B);
	}
	return Out;
}

FString TCOrders::GarrisonTag(int32 Flags)
{
	const TCWorldCore::FHouseType* H = (Flags & TCFlags::Garrisoned) ? FTCWorldObjects::GarrisonHouseType(Flags) : nullptr;
	if (!H) return FString();
	// hud.js: `Garrisoned: ${CFG.houses[...].name}`, translated as a whole ('Garrisoned: {0}' in es.js)
	return FTCLocale::T(FString::Printf(TEXT("Garrisoned: %s"), ANSI_TO_TCHAR(H->Name)));
}

// ---------------------------------------------------------------------------------------------------------------------
// Round 16: the -TCOrdersCheck= live script

bool FTCOrdersCheck::Parse(const FString& Script, const FString& InShotDir)
{
	Steps.Reset(); Next = 0; StepAt = -1; ShotDir = InShotDir;
	TArray<FString> Parts;
	Script.ParseIntoArray(Parts, TEXT(";"));
	for (const FString& Raw : Parts)
	{
		// [delay][@cond]:action[:arg]
		FStep S;
		FString Head, Rest;
		if (!Raw.Split(TEXT(":"), &Head, &Rest)) { Head = Raw; Rest.Reset(); }
		FString D = Head, C;
		if (Head.Split(TEXT("@"), &D, &C)) S.Cond = C;
		S.Delay = D.IsEmpty() ? 0.0 : FCString::Atod(*D);
		if (!Rest.Split(TEXT(":"), &S.Action, &S.Arg)) S.Action = Rest;
		if (S.Action.IsEmpty()) continue;
		Steps.Add(S);
	}
	return Steps.Num() > 0;
}

namespace
{
	TArray<int32> OwnWhere(const ATCGameMode& M, TFunctionRef<bool(const FTCUnit&, const FTCUnitDef&)> Test)
	{
		TArray<int32> Out;
		for (const auto& P : M.State.Units)
		{
			const FTCUnitDef* D = M.UnitTable.Find(P.Value.Type);
			if (D && M.State.IsOwn(P.Value) && P.Value.Hp > 0 && Test(P.Value, *D)) Out.Add(P.Key);
		}
		Out.Sort();
		return Out;
	}
	FVector2D Centre(const ATCGameMode& M, const TSet<int32>& Ids)
	{
		FVector2D C(0, 0); int32 N = 0;
		for (const int32 Id : Ids) if (const FTCUnit* U = M.State.Units.Find(Id)) { C += FVector2D(U->X, U->Z); N++; }
		return N ? C / N : FVector2D(-1, -1);
	}
	bool PixelOfGround(const ATCGameMode& M, const ATCPlayerController& PC, FVector2D G, double Up, FVector2D& Out)
	{
		if (!M.Terrain) return false;
		return PC.ProjectWorldLocationToScreen(ATCTerrain::ToWorld(G.X, G.Y, M.Terrain->HeightAt(G.X, G.Y) + Up), Out);
	}
	FString IdList(const TSet<int32>& Ids)
	{
		TArray<int32> A = Ids.Array(); A.Sort();
		TArray<FString> S; for (int32 Id : A) S.Add(FString::FromInt(Id));
		return FString::Join(S, TEXT(","));
	}
}

bool FTCOrdersCheck::Ready(const ATCGameMode& M, const ATCPlayerController& PC, const FStep& S) const
{
	const FString& C = S.Cond;
	if (C.IsEmpty()) return true;
	if (C == TEXT("play")) return M.State.CanOrder() && M.State.Snapshots >= 3;
	if (C == TEXT("garrisoned")) return OwnWhere(M, [](const FTCUnit& U, const FTCUnitDef&) { return (U.Flags & TCFlags::Garrisoned) != 0; }).Num() > 0;
	if (C == TEXT("out")) // every remembered unit has left its house
	{
		for (const int32 Id : Remember) { const FTCUnit* U = M.State.Units.Find(Id); if (U && (U->Flags & TCFlags::Garrisoned)) return false; }
		return true;
	}
	if (C == TEXT("queued")) // a selected unit has waiting orders
	{
		for (const FTCWaitingOrders& W : M.State.WaitingOrders) if (PC.Selected.Contains(W.Id) && W.Orders.Num()) return true;
		return false;
	}
	if (C == TEXT("planes")) return OwnWhere(M, [](const FTCUnit&, const FTCUnitDef& D) { return D.bAir; }).Num() > 0;
	if (C == TEXT("air")) return OwnWhere(M, [](const FTCUnit& U, const FTCUnitDef& D) { return D.bAir && !(U.Flags & TCClientFlags::PlaneAtBase); }).Num() > 0;
	if (C == TEXT("atstation")) // a selected ground unit within CFG.rail.reach (30 m) of an own station
	{
		if (!M.WorldRec.IsWorld()) return false;
		for (const FTCWorldStation& St : M.WorldRec.Snapshot().Stations)
		{
			const FTCWorldRegion* R = M.WorldRec.FindRegion(St.Region);
			if (!R || R->Team != M.WorldRec.MyTeam()) continue;
			for (const int32 Id : PC.Selected) if (const FTCUnit* U = M.State.Units.Find(Id)) if (FVector2D::Distance(FVector2D(U->X, U->Z), FVector2D(St.X, St.Z)) <= 30.0) return true;
		}
		return false;
	}
	if (C == TEXT("recovery")) return M.WorldRec.IsWorld() && M.WorldRec.Snapshot().bRecoveryAvailable;
	if (C == TEXT("captured")) // every region a capture step went for is the team's, or 45 s have passed
	{
		if (CaptureAt >= 0 && FPlatformTime::Seconds() - CaptureAt > 45.0) return true;
		for (const double Id : CaptureIds) { const FTCWorldRegion* R = M.WorldRec.FindRegion(Id); if (!R || R->Team != M.WorldRec.MyTeam()) return false; }
		return true;
	}
	if (C == TEXT("nobase")) // no living Regional Military Base this side sees near the guesses, or 90 s have passed
	{
		if (AssaultAt >= 0 && FPlatformTime::Seconds() - AssaultAt > 90.0) return true;
		for (const auto& P : M.State.Units)
			if (P.Value.Type == TEXT("worldbase") && P.Value.Hp > 0)
				for (const FVector2D& G : Guesses) if (FVector2D::Distance(G, FVector2D(P.Value.X, P.Value.Z)) < 50.0) return false;
		return true;
	}
	if (C == TEXT("train")) // an own train exists (a rail move was adopted)
		return OwnWhere(M, [](const FTCUnit&, const FTCUnitDef& D) { return D.bRail; }).Num() > 0;
	return true;
}

FString FTCOrdersCheck::Summary(const ATCGameMode& M, const ATCPlayerController& PC) const
{
	int32 Garr = 0, Waiting = 0, WithPlan = 0;
	TArray<FString> Kinds;
	for (const auto& P : M.State.Units) if (M.State.IsOwn(P.Value) && (P.Value.Flags & TCFlags::Garrisoned)) Garr++;
	for (const FTCWaitingOrders& W : M.State.WaitingOrders) if (PC.Selected.Contains(W.Id))
	{
		Waiting += W.Orders.Num();
		for (const FTCQueuedOrder& O : W.Orders) Kinds.Add(FString::Printf(TEXT("%d@%.0f,%.0f"), O.Kind, O.X, O.Z));
	}
	TArray<FString> PlanKinds;
	for (const FTCPlan& P : M.State.Plans) if (PC.Selected.Contains(P.Id) && P.Kind) { WithPlan++; PlanKinds.Add(FString::Printf(TEXT("%d:%d"), P.Id, P.Kind)); }
	int32 Trains = 0, Riding = 0;
	TMap<FString, int32> Types;
	for (const auto& P : M.State.Units) if (M.State.IsOwn(P.Value) && P.Value.Hp > 0)
	{
		Trains += P.Value.Type == TEXT("train"); Riding += (P.Value.Flags & TCFlags::Riding) != 0;
		Types.FindOrAdd(P.Value.Type)++;
	}
	TArray<FString> TypeList;
	for (const auto& P : Types) TypeList.Add(FString::Printf(TEXT("%s %d"), *P.Key, P.Value));
	TypeList.Sort();
	FString World;
	if (M.WorldRec.IsWorld())
	{
		int32 Own = 0;
		for (const FTCWorldRegion& R : M.WorldRec.Snapshot().Regions) Own += R.Team == M.WorldRec.MyTeam();
		World = FString::Printf(TEXT("; world: own regions %d, stations known %d, recovery %s"), Own, M.WorldRec.Snapshot().Stations.Num(), M.WorldRec.Snapshot().bRecoveryAvailable ? TEXT("available") : TEXT("not offered"));
	}
	TArray<FString> Sends, Refuses;
	for (const auto& P : Sent) Sends.Add(FString::Printf(TEXT("%s %d"), *P.Key, P.Value));
	for (const auto& P : Refused) Refuses.Add(FString::Printf(TEXT("%s %d"), *P.Key, P.Value));
	return FString::Printf(TEXT("tick %d; selected [%s]; own garrisoned %d, trains %d, riding %d; own units {%s}%s; selected with a plan %d [%s]; waiting orders of the selection %d [%s]; sent {%s}; refused {%s}; last note '%s'"),
		M.State.Tick, *IdList(PC.Selected), Garr, Trains, Riding, *FString::Join(TypeList, TEXT(", ")), *World, WithPlan, *FString::Join(PlanKinds, TEXT(" ")), Waiting, *FString::Join(Kinds, TEXT(" ")),
		*FString::Join(Sends, TEXT(", ")), *FString::Join(Refuses, TEXT(", ")), *PC.LastOrderNote);
}

FString FTCOrdersCheck::Run(ATCGameMode& M, ATCPlayerController& PC, const FStep& S)
{
	const bool bShift = S.Arg.EndsWith(TEXT("shift"));
	FString Arg = S.Arg;
	if (bShift) { Arg.RemoveFromEnd(TEXT("shift")); Arg.RemoveFromEnd(TEXT(":")); Arg.RemoveFromEnd(TEXT(",")); }
	if (S.Action == TEXT("select"))
	{
		// select:army | select:air | select:garrisoned | select:remember | select:<type>,<type>
		TArray<int32> Ids;
		if (Arg == TEXT("army")) Ids = OwnWhere(M, [](const FTCUnit& U, const FTCUnitDef& D) { return !D.bStructure && !D.bAir && !D.bRail && U.Type != TEXT("truck") && !(U.Flags & (TCFlags::Riding | TCFlags::Garrisoned)); });
		// planes, parked at their base or not (a direct selection, as a control group recalls them; a pick never takes a
		// parked plane)
		else if (Arg == TEXT("air")) Ids = OwnWhere(M, [](const FTCUnit&, const FTCUnitDef& D) { return D.bAir; });
		else if (Arg == TEXT("garrisoned")) Ids = OwnWhere(M, [](const FTCUnit& U, const FTCUnitDef&) { return (U.Flags & TCFlags::Garrisoned) != 0; });
		else if (Arg == TEXT("remember")) Ids = Remember;
		else { TArray<FString> Types; Arg.ParseIntoArray(Types, TEXT(",")); Ids = OwnWhere(M, [&Types](const FTCUnit& U, const FTCUnitDef&) { return Types.Contains(U.Type) && !(U.Flags & TCFlags::Riding); }); }
		PC.SelectOwn(Ids);
		return FString::Printf(TEXT("select %s: %d unit(s)"), *Arg, PC.Selected.Num());
	}
	if (S.Action == TEXT("selectone"))
	{
		// selectone:<type>: the own unit of that type with the lowest id (the browser half picks its first one too)
		const TArray<int32> Ids = OwnWhere(M, [&Arg](const FTCUnit& U, const FTCUnitDef&) { return U.Type == Arg && !(U.Flags & (TCFlags::Riding | TCFlags::Garrisoned)); });
		PC.SelectOwn(Ids.Num() ? TArray<int32>{ Ids[0] } : TArray<int32>());
		return FString::Printf(TEXT("select one %s: [%s]"), *Arg, *IdList(PC.Selected));
	}
	if (S.Action == TEXT("remember")) { Remember = PC.Selected.Array(); Remember.Sort(); return FString::Printf(TEXT("remember [%s]"), *IdList(PC.Selected)); }
	if (S.Action == TEXT("focus"))
	{
		// focus:sel | focus:house (the nearest typed house cell to the selection) | focus:x,z
		FVector2D At = Centre(M, PC.Selected);
		if (Arg == TEXT("house"))
		{
			const FTCWorldObjects& W = M.WorldObjects;
			int32 Best = -1; double BestD = 1e18;
			for (int32 C = 0; C < W.MapW() * W.MapH(); C++)
			{
				if (!W.IsHouseCell(C) || W.HouseTypeAt(C) < 0) continue;
				const FVector2D P(((C % W.MapW()) + 0.5) * 2.0, ((C / W.MapW()) + 0.5) * 2.0);
				const double D = FVector2D::Distance(P, At);
				if (D < BestD) { BestD = D; Best = C; }
			}
			if (Best < 0) return TEXT("focus house: no house cell on this map");
			House = FVector2D(((Best % W.MapW()) + 0.5) * 2.0, ((Best / W.MapW()) + 0.5) * 2.0);
			At = House;
			PC.FocusOn(At); PC.ZoomTo(45.f);
			return FString::Printf(TEXT("focus house cell %d at (%.1f, %.1f), %.1f m from the selection, a %s"), Best, House.X, House.Y, BestD, ANSI_TO_TCHAR(TCWorldCore::HouseTypes[W.HouseTypeAt(Best)].Name));
		}
		if (Arg == TEXT("train"))
		{
			// the own train (drawn where the renderer has it), else the own station nearest the remembered units
			const TArray<int32> Trains = OwnWhere(M, [](const FTCUnit&, const FTCUnitDef& D) { return D.bRail; });
			if (Trains.Num()) { const FTCUnit& T = M.State.Units[Trains[0]]; At = FVector2D(T.X, T.Z); if (M.Units) M.Units->DrawnAt(T.Id, At); }
			else if (M.WorldRec.IsWorld())
			{
				const TSet<int32> R(Remember);
				const FVector2D From = Centre(M, R);
				double BestD = 1e18;
				for (const FTCWorldStation& St : M.WorldRec.Snapshot().Stations)
					if (FVector2D::Distance(From, FVector2D(St.X, St.Z)) < BestD) { BestD = FVector2D::Distance(From, FVector2D(St.X, St.Z)); At = FVector2D(St.X, St.Z); }
			}
			PC.FocusOn(At);
			return FString::Printf(TEXT("focus train%s (%.1f, %.1f)"), Trains.Num() ? TEXT("") : TEXT(" (none: a station)"), At.X, At.Y);
		}
		if (Arg.StartsWith(TEXT("+")) && House.X >= 0) { FString X, Z; Arg.Mid(1).Split(TEXT(","), &X, &Z); At = House + FVector2D(FCString::Atod(*X), FCString::Atod(*Z)); } // from the anchor
		else if (Arg != TEXT("sel")) { FString X, Z; Arg.Split(TEXT(","), &X, &Z); At = FVector2D(FCString::Atod(*X), FCString::Atod(*Z)); }
		PC.FocusOn(At); PC.ZoomTo(50.f);
		return FString::Printf(TEXT("focus (%.1f, %.1f)"), At.X, At.Y);
	}
	if (S.Action == TEXT("zoom")) { PC.ZoomTo(FCString::Atof(*Arg)); return TEXT("zoom ") + Arg; }
	if (S.Action == TEXT("anchor"))
	{
		// anchor: later move steps count their offsets from the selection's centre now (not from where it walks to)
		House = Centre(M, PC.Selected);
		return FString::Printf(TEXT("anchor (%.1f, %.1f)"), House.X, House.Y);
	}
	if (S.Action == TEXT("garrison"))
	{
		// the right press on the house through the world path: a pixel over the house cell the house pick takes
		if (House.X < 0) return TEXT("garrison: no house focused");
		FVector2D Px(-1, -1);
		for (double Up = 2.0; Up <= 8.0 && Px.X < 0; Up += 1.0)
		{
			FVector2D P; FTCHousePick Pick;
			if (PixelOfGround(M, PC, House, Up, P) && PC.HouseAt(P, Pick)) Px = P;
		}
		if (Px.X < 0) return TEXT("garrison: no pixel over the house is taken by the house pick");
		const FTCCursor C = PC.CursorAt(Px);
		const bool bGesture = PC.RightPressWorld(Px, bShift, false);
		if (bGesture) PC.GestureRelease(ETCGestureButton::Right, Px, bShift, false);
		return FString::Printf(TEXT("right press%s on the house at pixel (%.0f, %.0f): cursor house %s cell %d, enemy %d, friend %d; %s"), bShift ? TEXT(" with Shift") : TEXT(""), Px.X, Px.Y,
			C.bHouse ? TEXT("yes") : TEXT("no"), C.HouseCell, C.Enemy, C.Friend, bGesture ? TEXT("a facing gesture (released at once)") : TEXT("dispatched at once"));
	}
	if (S.Action == TEXT("move"))
	{
		// move:dx,dz[,shift] metres from the house focused (else the selection's centre): a right press and release
		FString X, Z; Arg.Split(TEXT(","), &X, &Z);
		const FVector2D From = House.X >= 0 ? House : Centre(M, PC.Selected);
		const FVector2D G = From + FVector2D(FCString::Atod(*X), FCString::Atod(*Z));
		FVector2D Px;
		if (!PixelOfGround(M, PC, G, 0.0, Px)) return TEXT("move: the ground point is off the screen");
		const FVector2D Back = PC.CursorAt(Px).Ground;
		const bool bGesture = PC.RightPressWorld(Px, bShift, false);
		const int32 N = bGesture ? PC.GestureRelease(ETCGestureButton::Right, Px, bShift, false) : -1;
		return FString::Printf(TEXT("right press and release%s at (%.1f, %.1f), pixel (%.0f, %.0f), ground picked (%.1f, %.1f): %s"), bShift ? TEXT(" with Shift") : TEXT(""), G.X, G.Y, Px.X, Px.Y, Back.X, Back.Y,
			bGesture ? *FString::Printf(TEXT("the gesture sent to %d unit(s)"), N) : TEXT("dispatched at the press"));
	}
	if (S.Action == TEXT("escort"))
	{
		// a right press on an own ground unit (not selected, drawn) with only planes selected
		const TArray<int32> Ground = OwnWhere(M, [&PC](const FTCUnit& U, const FTCUnitDef& D) { return !D.bAir && !D.bStructure && !PC.Selected.Contains(U.Id) && ATCUnitRenderer::IsDrawn(U); });
		for (const int32 Id : Ground)
		{
			FVector2D Px;
			if (!PC.ScreenOf(Id, Px)) continue;
			const FTCCursor C = PC.CursorAt(Px);
			if (C.Friend != Id) continue;
			const bool bGesture = PC.RightPressWorld(Px, bShift, false);
			if (bGesture) PC.GestureRelease(ETCGestureButton::Right, Px, bShift, false);
			Remember = { Id };
			return FString::Printf(TEXT("right press%s on own %s %d at pixel (%.0f, %.0f) (cursor friend %d)%s"), bShift ? TEXT(" with Shift") : TEXT(""), *M.State.Units[Id].Type, Id, Px.X, Px.Y, C.Friend, bGesture ? TEXT(", a gesture (WRONG for an escort)") : TEXT(""));
		}
		return TEXT("escort: no own ground unit on the screen under its own pick");
	}
	if (S.Action == TEXT("pickroof") || S.Action == TEXT("boxroof"))
	{
		// the roof bar of the first own garrisoned squad: the pixel of its bar group's centre, then a click or a small box
		const TArray<int32> G = OwnWhere(M, [](const FTCUnit& U, const FTCUnitDef&) { return (U.Flags & TCFlags::Garrisoned) != 0; });
		if (!G.Num()) return S.Action + TEXT(": no own garrisoned squad");
		const FTCUnit& U = M.State.Units[G[0]];
		FVector2D Px;
		if (!PixelOfGround(M, PC, FVector2D(U.X, U.Z), TCOrders::RoofBarHeight, Px)) return S.Action + TEXT(": the roof bar is off the screen");
		if (S.Action == TEXT("pickroof")) PC.ClickSelectForChecks(Px, bShift);
		else PC.BoxSelect(Px - FVector2D(3, 3), Px + FVector2D(3, 3), bShift);
		Remember = PC.Selected.Array(); Remember.Sort();
		return FString::Printf(TEXT("%s on %s %d's roof bar at pixel (%.0f, %.0f): selected [%s] (%s)"), S.Action == TEXT("pickroof") ? TEXT("left click") : TEXT("box"), *U.Type, U.Id, Px.X, Px.Y, *IdList(PC.Selected),
			*TCOrders::GarrisonTag(U.Flags));
	}
	if (S.Action == TEXT("gostation"))
	{
		// a plain right-click move of the selection to the own station nearest it (the click path, IssueMoveAt)
		if (!M.WorldRec.IsWorld()) return TEXT("gostation: not a World match");
		const FVector2D From = Centre(M, PC.Selected);
		FVector2D Best(-1, -1); double BestD = 1e18;
		for (const FTCWorldStation& St : M.WorldRec.Snapshot().Stations)
		{
			const FTCWorldRegion* R = M.WorldRec.FindRegion(St.Region);
			const double D = FVector2D::Distance(FVector2D(St.X, St.Z), From);
			if (R && R->Team == M.WorldRec.MyTeam() && D < BestD) { BestD = D; Best = FVector2D(St.X, St.Z); }
		}
		if (Best.X < 0) return TEXT("gostation: no own station known");
		const FVector2D To = Best + FVector2D(6, 6);
		const int32 N = PC.IssueMoveAt(To);
		PC.FocusOn(To);
		return FString::Printf(TEXT("gostation: move of %d unit(s) to (%.1f, %.1f), beside the own station at (%.1f, %.1f), %.0f m away"), N, To.X, To.Y, Best.X, Best.Y, BestD);
	}
	if (S.Action == TEXT("rail"))
	{
		// Shift+N (client/keys.js rail), then a click at the own station farthest from the selection (rail:far), or at a
		// point dx,dz from the selection's centre
		const ETCAim AimBefore = PC.Aim;
		const bool bKey = PC.KeyChord(EKeys::N, true, false, false);
		const ETCAim AimAfter = PC.Aim;
		const FVector2D From = Centre(M, PC.Selected);
		FVector2D G = From;
		FString Where;
		if (Arg == TEXT("far") && M.WorldRec.IsWorld())
		{
			// the region of the own station nearest the selection is where the train starts (shared/sim.js rail
			// origin); the click goes to the nearest other own region, at its station when one is known
			double OriginRegion = NAN, NearD = 1e18;
			for (const FTCWorldStation& St : M.WorldRec.Snapshot().Stations)
			{
				const FTCWorldRegion* R = M.WorldRec.FindRegion(St.Region);
				const bool bOwn = R && R->Team == M.WorldRec.MyTeam();
				const double D = FVector2D::Distance(FVector2D(St.X, St.Z), From);
				Where += FString::Printf(TEXT(" station of region %.0f at (%.0f, %.0f) %s %.0f m;"), St.Region, St.X, St.Z, bOwn ? TEXT("own") : TEXT("not own"), D);
				if (bOwn && D < NearD) { NearD = D; OriginRegion = St.Region; }
			}
			double BestD = 1e18;
			for (const FTCWorldRegion& R : M.WorldRec.Snapshot().Regions)
			{
				if (R.Team != M.WorldRec.MyTeam() || R.Id == OriginRegion || !R.FinitePosition()) continue;
				FVector2D At(R.X, R.Z);
				for (const FTCWorldStation& St : M.WorldRec.Snapshot().Stations) if (St.Region == R.Id) At = FVector2D(St.X, St.Z);
				const double D = FVector2D::Distance(At, From);
				Where += FString::Printf(TEXT(" own region %.0f '%s' (%s) at (%.0f, %.0f) %.0f m;"), R.Id, *R.Name, *R.Kind, At.X, At.Y, D);
				if (D < BestD) { BestD = D; G = At; }
			}
		}
		else if (!Arg.IsEmpty()) { FString X, Z; Arg.Split(TEXT(","), &X, &Z); G += FVector2D(FCString::Atod(*X), FCString::Atod(*Z)); }
		const bool bSent = PC.AimClick(true, G, false);
		return FString::Printf(TEXT("Shift+N %s (aim %s before, %s after the key, %s after the click), then the click at (%.1f, %.1f): %s;%s"), bKey ? TEXT("taken") : TEXT("not bound"),
			LexToString(AimBefore), LexToString(AimAfter), LexToString(PC.Aim), G.X, G.Y, bSent ? TEXT("sent") : TEXT("nothing sent"), *Where);
	}
	if (S.Action == TEXT("spawnset"))
	{
		// spawnset:<type>: the sandbox bar's unit select stepped (its own control) to <type>, and its side set to Mine
		int32 Tries = 0;
		FTCUiCommand Unit(ETCUiAction::MatchControl);
		Unit.Unit = TEXT("SandboxUnit");
		while (FString(M.Controls.SandboxUnit().Type) != Arg && Tries++ < 40) M.RunControl(Unit, false);
		if (M.Controls.SandboxEnemy()) { FTCUiCommand Side(ETCUiAction::MatchControl); Side.Unit = TEXT("SandboxSide"); M.RunControl(Side, false); }
		return FString::Printf(TEXT("spawnset: sandbox unit %s after %d step(s), side %s"), M.Controls.SandboxUnit().Type, Tries, M.Controls.SandboxEnemy() ? TEXT("enemy") : TEXT("mine"));
	}
	if (S.Action == TEXT("neighbours"))
	{
		// neighbours:<n> (World Conquest sandbox): the regions lie on a 128 m grid (shared/world-conquest.js); n units of
		// the sandbox type spawned (the Spawn aim's map clicks) at each of the three grid neighbours of the own home
		// region toward the map's centre, which also shows this side where their claim points and bases are
		if (!M.WorldRec.IsWorld()) return TEXT("neighbours: not a World match");
		const FVector2D From = Centre(M, PC.Selected);
		const FTCWorldRegion* Home = nullptr; double HomeD = 1e18;
		for (const FTCWorldRegion& R : M.WorldRec.Snapshot().Regions)
			if (R.Team == M.WorldRec.MyTeam() && R.FinitePosition() && FVector2D::Distance(FVector2D(R.X, R.Z), From) < HomeD) { HomeD = FVector2D::Distance(FVector2D(R.X, R.Z), From); Home = &R; }
		if (!Home) return TEXT("neighbours: no own region known");
		const double Size = M.WorldObjects.MapW() > 0 ? M.WorldObjects.MapW() * 2.0 : 1024.0;
		const double SX = Home->X < Size / 2 ? 1 : -1, SZ = Home->Z < Size / 2 ? 1 : -1;
		Guesses = { FVector2D(Home->X + SX * 128, Home->Z), FVector2D(Home->X, Home->Z + SZ * 128), FVector2D(Home->X + SX * 128, Home->Z + SZ * 128) };
		// neighbours:<n>,wide: the next ring along both edges as well (more regions, so more likely a station region)
		if (Arg.EndsWith(TEXT(",wide"))) { Guesses.Add(FVector2D(Home->X + SX * 256, Home->Z)); Guesses.Add(FVector2D(Home->X, Home->Z + SZ * 256)); }
		const int32 N = FMath::Clamp(Arg.IsEmpty() ? 3 : FCString::Atoi(*Arg), 1, 8);
		FTCUiCommand Arm(ETCUiAction::MatchControl);
		Arm.Unit = TEXT("SandboxSpawn");
		const bool bArmed = M.RunControl(Arm, false);
		int32 Sends = 0;
		for (const FVector2D& G : Guesses) for (int32 K = 0; bArmed && K < N; K++) Sends += PC.AimClick(true, G, false) ? 1 : 0;
		if (bArmed) PC.CancelAim();
		TArray<FString> At;
		for (const FVector2D& G : Guesses) At.Add(FString::Printf(TEXT("(%.0f, %.0f)"), G.X, G.Y));
		return FString::Printf(TEXT("neighbours: home region %.0f '%s' at (%.0f, %.0f), map %.0f m; %d %s spawn message(s) for %s"), Home->Id, *Home->Name, Home->X, Home->Z, Size,
			Sends, M.Controls.SandboxUnit().Type, *FString::Join(At, TEXT(" ")));
	}
	if (S.Action == TEXT("attackbase"))
	{
		// the nearest living Regional Military Base this side sees near a guess: the own vehicles within 70 m of it are
		// selected, the camera goes there and the base gets a right press (client/orders.js 'attack')
		AssaultAt = AssaultAt < 0 ? FPlatformTime::Seconds() : AssaultAt;
		const FTCUnit* Base = nullptr; double BestD = 1e18;
		for (const auto& P : M.State.Units)
		{
			if (P.Value.Type != TEXT("worldbase") || P.Value.Hp <= 0) continue;
			for (const FVector2D& G : Guesses)
			{
				const double D = FVector2D::Distance(G, FVector2D(P.Value.X, P.Value.Z));
				if (D < 50.0 && D < BestD) { BestD = D; Base = &P.Value; }
			}
		}
		if (!Base) return TEXT("attackbase: no living base seen near the guesses");
		const FVector2D At(Base->X, Base->Z);
		const TArray<int32> Ids = OwnWhere(M, [At](const FTCUnit& U, const FTCUnitDef& D) { return !D.bInfantry && !D.bStructure && !D.bAir && !D.bRail && FVector2D::Distance(At, FVector2D(U.X, U.Z)) < 70.0; });
		PC.SelectOwn(Ids);
		PC.FocusOn(At); PC.ZoomTo(50.f);
		FVector2D Px;
		if (!PC.ScreenOf(Base->Id, Px)) return FString::Printf(TEXT("attackbase: base %d at (%.0f, %.0f) not on the screen yet"), Base->Id, At.X, At.Y);
		const FTCCursor C = PC.CursorAt(Px);
		const bool bGesture = PC.RightPressWorld(Px, false, false);
		if (bGesture) PC.GestureRelease(ETCGestureButton::Right, Px, false, false);
		return FString::Printf(TEXT("attackbase: %d vehicle(s) selected, right press on base %d (hp %d) at pixel (%.0f, %.0f), cursor enemy %d%s"), PC.Selected.Num(), Base->Id, Base->Hp, Px.X, Px.Y, C.Enemy,
			bGesture ? TEXT(" (a gesture, released)") : TEXT(""));
	}
	if (S.Action == TEXT("capture"))
	{
		// capture[:radius] (World Conquest sandbox): the sandbox spawn ({ t: 'spawn', unit, enemy, x, z }, this seat's
		// own unit, as the Spawn button's aim sends it) twice on the claim point of every region not the team's within
		// radius metres (default 220) of the selection, at most 8 regions, so a second own station comes within reach
		if (!M.WorldRec.IsWorld()) return TEXT("capture: not a World match");
		const double Radius = Arg.IsEmpty() ? 220.0 : FCString::Atod(*Arg);
		const FVector2D From = Centre(M, PC.Selected);
		TArray<TPair<double, const FTCWorldRegion*>> Near;
		for (const FTCWorldRegion& R : M.WorldRec.Snapshot().Regions)
		{
			if (R.Team == M.WorldRec.MyTeam() || !R.FinitePosition()) continue;
			const double D = FVector2D::Distance(FVector2D(R.X, R.Z), From);
			if (D <= Radius) Near.Add({ D, &R });
		}
		Near.Sort([](const TPair<double, const FTCWorldRegion*>& A, const TPair<double, const FTCWorldRegion*>& B) { return A.Key < B.Key; });
		CaptureIds.Reset();
		TArray<FString> Did;
		int32 Sends = 0;
		// the sandbox bar's Spawn button (its default unit, the Rifle Squad, on this seat's side), then map clicks
		FTCUiCommand Arm(ETCUiAction::MatchControl);
		Arm.Unit = TEXT("SandboxSpawn");
		const bool bArmed = M.RunControl(Arm, false);
		for (int32 I = 0; bArmed && I < FMath::Min(8, Near.Num()); I++)
		{
			const FTCWorldRegion& R = *Near[I].Value;
			CaptureIds.Add(R.Id);
			for (int32 K = 0; K < 2; K++) Sends += PC.AimClick(true, FVector2D(R.X, R.Z), false) ? 1 : 0;
			Did.Add(FString::Printf(TEXT("region %.0f '%s' (%s, team %.0f) at (%.0f, %.0f) %.0f m"), R.Id, *R.Name, *R.Kind, R.Team, R.X, R.Z, Near[I].Key));
		}
		if (bArmed) PC.CancelAim();
		CaptureAt = FPlatformTime::Seconds();
		return FString::Printf(TEXT("capture: spawn aim %s, %d spawn message(s) sent for %d region(s): %s"), bArmed ? TEXT("armed") : TEXT("NOT armed"), Sends, CaptureIds.Num(), *FString::Join(Did, TEXT("; ")));
	}
	if (S.Action == TEXT("key"))
	{
		// key:<Key>[,shift] through the key chord (client/keys.js match)
		const bool bOk = PC.KeyChord(FKey(*Arg), bShift, false, false);
		return FString::Printf(TEXT("key %s%s: %s"), bShift ? TEXT("Shift+") : TEXT(""), *Arg, bOk ? TEXT("bound") : TEXT("not bound"));
	}
	if (S.Action == TEXT("press"))
	{
		// press:recover | press:rail: the HUD button through its hit test; press:pause (round 17, poses): the host's
		// Pause/Resume (client/main.js pauseBtn), so both clients of a side by side can shoot the same frozen scene
		const ETCUiAction A = Arg == TEXT("recover") ? ETCUiAction::Recover : Arg == TEXT("rail") ? ETCUiAction::Rail : Arg == TEXT("pause") ? ETCUiAction::PauseMatch : ETCUiAction::None;
		if (A == ETCUiAction::None) return TEXT("press: unknown button ") + Arg;
		return FString::Printf(TEXT("press %s: %s"), *Arg, PC.PressButton(A) ? TEXT("done") : TEXT("did nothing"));
	}
	if (S.Action == TEXT("shot"))
	{
		const FString File = FPaths::Combine(ShotDir.IsEmpty() ? FPaths::ProjectSavedDir() / TEXT("Screenshots") : ShotDir, Arg + TEXT(".png"));
		FScreenshotRequest::RequestScreenshot(File, true, false);
		// round 17 (poses): the camera as drawn, so unreal/Tools/compare/scene-reference.mjs can set the browser's to it
		return FString::Printf(TEXT("screenshot %s | camera (%.2f, %.2f) yaw %.2f distance %.2f m"), *File, PC.ViewFocus().X, PC.ViewFocus().Y, PC.ViewYaw(), PC.ViewDistance());
	}
	if (S.Action == TEXT("quit")) { FPlatformMisc::RequestExit(false, TEXT("TCOrdersCheck")); return TEXT("quit"); }
	return TEXT("log");
}

void FTCOrdersCheck::Frame(ATCGameMode& M, ATCPlayerController& PC)
{
	if (!Steps.Num()) return;
	const double Now = FPlatformTime::Seconds();
	// every deny the server sends is counted with its command and reason (FTCMatchState keeps the last one)
	if (M.State.Denies != LastDenies)
	{
		if (M.State.Denies > LastDenies) Refused.FindOrAdd(M.State.DenyCmd + TEXT("/") + M.State.DenyReason) += M.State.Denies - LastDenies;
		UE_LOG(LogTC, Log, TEXT("orders check: deny %s / %s (%s)"), *M.State.DenyCmd, *M.State.DenyReason, *TCOrders::DenySentence(M.State.DenyReason, M.State.DenyCmd));
		LastDenies = M.State.Denies;
	}
	if (Next >= Steps.Num()) return;
	const FStep& S = Steps[Next];
	if (StepAt < 0) StepAt = Now;
	if (Now - StepAt < S.Delay || !Ready(M, PC, S)) return;
	Next++;
	StepAt = Now;
	const FString Did = Run(M, PC, S);
	UE_LOG(LogTC, Log, TEXT("orders check %d/%d: %s | %s"), Next, Steps.Num(), *Did, *Summary(M, PC));
}

// ---------------------------------------------------------------------------------------------------------------------
// Round 16 tests
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"

namespace TCOrdersTest
{
// generated by queue-oracle.mjs from queue-cases.json against the pinned client/markers.js (see the evidence tools)
static const TCHAR* QueueCasesJson = TEXT(R"JSON([{"name":"move-with-queue","selected":[1],"units":[{"id":1,"type":"rifle","x":10,"z":10,"plan":{"kind":1,"tx":30,"tz":10,"path":[20,12,30,10]},"orders":[{"kind":1,"x":40,"z":20},{"kind":2,"x":44,"z":40},{"kind":9,"x":60,"z":41}]}]},{"name":"queue-only-idle","selected":[2],"units":[{"id":2,"type":"mg","x":5,"z":5,"plan":null,"orders":[{"kind":1,"x":25,"z":5},{"kind":4,"x":25.5,"z":6}]}]},{"name":"attack-on-order","selected":[3],"units":[{"id":3,"type":"tank","x":0,"z":0,"tgt":9,"plan":{"kind":4,"tx":30,"tz":30,"path":[15,14,29,29]},"orders":[]},{"id":9,"type":"at","x":31,"z":30.5,"plan":null,"orders":[]}]},{"name":"attack-unseen","selected":[4],"units":[{"id":4,"type":"rifle","x":0,"z":50,"plan":{"kind":4,"tx":40,"tz":50,"path":[20,50,40,50]},"orders":[{"kind":1,"x":60,"z":60}]}]},{"name":"auto-target","selected":[5],"units":[{"id":5,"type":"mg","x":10,"z":10,"tgt":6,"plan":null,"orders":[]},{"id":6,"type":"rifle","x":30,"z":12,"plan":null,"orders":[]}]},{"name":"fireat-route","selected":[7],"units":[{"id":7,"type":"mortar","x":0,"z":0,"plan":{"kind":5,"tx":60,"tz":10,"path":[8,0,16,2]},"orders":[{"kind":5,"x":70,"z":20}]}]},{"name":"retreat-dig-build","selected":[8,10,11],"units":[{"id":8,"type":"rifle","x":0,"z":0,"plan":{"kind":3,"tx":-40,"tz":0,"path":[-20,0,-40,0]},"orders":[]},{"id":10,"type":"engineer","x":5,"z":5,"plan":{"kind":7,"tx":9,"tz":9,"path":[9,9]},"orders":[{"kind":8,"x":20,"z":20}]},{"id":11,"type":"engineer","x":5,"z":5,"plan":{"kind":8,"tx":5.5,"tz":5.5,"path":[5.5,5.5]},"orders":[]}]},{"name":"short-legs","selected":[12],"units":[{"id":12,"type":"rifle","x":0,"z":0,"plan":{"kind":1,"tx":2,"tz":0,"path":[2,0]},"orders":[{"kind":1,"x":3,"z":0},{"kind":1,"x":9,"z":0}]}]},{"name":"building-rally-and-army-rally","selected":[13,99],"rally":{"x":-10,"z":-10},"units":[{"id":13,"type":"barracks","x":50,"z":50,"plan":null,"orders":[],"rally":{"x":70,"z":40}}]},{"name":"garrisoned-queued","selected":[14],"units":[{"id":14,"type":"rifle","x":22,"z":22,"garr":true,"plan":null,"orders":[{"kind":1,"x":40,"z":22},{"kind":9,"x":60,"z":30}]}]},{"name":"ability-unknown-type","selected":[15],"units":[{"id":15,"type":"nosuchtype","x":0,"z":0,"plan":{"kind":6,"tx":12,"tz":5,"path":[]},"orders":[]}]}])JSON");
static const TCHAR* QueueExpectedJson = TEXT(R"JSON([{"name":"move-with-queue","radius":{"1":1.5},"prims":[{"op":"line","b":"solid","hex":6268146,"a":0.9,"w":0.34,"pts":[11.961161,10.392232,20,12,27.406364,10.518727]},{"op":"arrow","b":"solid","hex":6268146,"a":0.9,"x":28.823303,"z":10.235339,"dx":8.823303,"dz":-1.764661,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":30,"z":10},{"op":"line","b":"dashed","hex":6268146,"a":0.8,"w":0.28,"pts":[30.707107,10.707107,38.129703,18.129703]},{"op":"arrow","b":"solid","hex":6268146,"a":0.8,"x":39.151472,"z":19.151472,"dx":8.444365,"dz":8.444365,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":40,"z":20},{"op":"line","b":"dashed","hex":15766319,"a":0.8,"w":0.28,"pts":[40.196116,20.980581,43.481273,37.406364]},{"op":"arrow","b":"solid","hex":15766319,"a":0.8,"x":43.764661,"z":38.823303,"dx":3.568545,"dz":17.842723,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":15766319,"a":0.95,"r":1,"w":0.22,"x":44,"z":40},{"op":"line","b":"dashed","hex":6268146,"a":0.8,"w":0.28,"pts":[44.998053,40.062378,57.360151,40.835009]},{"op":"arrow","b":"solid","hex":6268146,"a":0.8,"x":58.802337,"z":40.925146,"dx":13.804284,"dz":0.862768,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":60,"z":41}]},{"name":"queue-only-idle","radius":{"2":1.3},"prims":[{"op":"line","b":"dashed","hex":6268146,"a":0.8,"w":0.28,"pts":[6.8,5,22.355,5]},{"op":"arrow","b":"solid","hex":6268146,"a":0.8,"x":23.8,"z":5,"dx":17,"dz":0,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":25,"z":5},{"op":"ring","b":"solid","hex":15026235,"a":0.95,"r":1,"w":0.22,"x":25.5,"z":6}]},{"name":"attack-on-order","radius":{"3":2.5,"9":1.8},"prims":[{"op":"line","b":"solid","hex":15026235,"a":0.9,"w":0.34,"pts":[2.193166,2.046955,15,14,25.7624,25.531143]},{"op":"arrow","b":"solid","hex":15026235,"a":0.9,"x":26.74835,"z":26.587518,"dx":11.74835,"dz":12.587518,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":15026235,"a":0.95,"r":3,"w":0.24,"x":31,"z":30.5}]},{"name":"attack-unseen","radius":{"4":1.5},"prims":[{"op":"line","b":"solid","hex":15026235,"a":0.9,"w":0.34,"pts":[2,50,20,50,35.555,50]},{"op":"arrow","b":"solid","hex":15026235,"a":0.9,"x":37,"z":50,"dx":17,"dz":0,"len":1.7,"half":0.72},{"op":"line","b":"dashed","hex":6268146,"a":0.8,"w":0.28,"pts":[40.894427,50.447214,57.63424,58.81712]},{"op":"arrow","b":"solid","hex":6268146,"a":0.8,"x":58.926687,"z":59.463344,"dx":18.03226,"dz":9.01613,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":60,"z":60},{"op":"ring","b":"solid","hex":15026235,"a":0.95,"r":2.7,"w":0.24,"x":40,"z":50}]},{"name":"auto-target","radius":{"5":1.3,"6":1.5},"prims":[{"op":"line","b":"dashed","hex":15026235,"a":0.85,"w":0.26,"pts":[11.791067,10.179107,25.57706,11.557706]},{"op":"arrow","b":"solid","hex":15026235,"a":0.85,"x":27.014888,"z":11.701489,"dx":15.223821,"dz":1.522382,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":15026235,"a":0.95,"r":2.7,"w":0.24,"x":30,"z":12}]},{"name":"fireat-route","radius":{"7":1.4},"prims":[{"op":"line","b":"solid","hex":15026235,"a":0.9,"w":0.34,"pts":[1.9,0,8,0,16,2]},{"op":"line","b":"dashed","hex":15026235,"a":0.8,"w":0.28,"pts":[16,2,57.102503,9.473182]},{"op":"arrow","b":"solid","hex":15026235,"a":0.8,"x":58.524195,"z":9.731672,"dx":42.524195,"dz":7.731672,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":15026235,"a":0.95,"r":1,"w":0.22,"x":60,"z":10},{"op":"line","b":"dashed","hex":15026235,"a":0.8,"w":0.28,"pts":[60.707107,10.707107,68.129703,18.129703]},{"op":"arrow","b":"solid","hex":15026235,"a":0.8,"x":69.151472,"z":19.151472,"dx":8.444365,"dz":8.444365,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":15026235,"a":0.95,"r":1,"w":0.22,"x":70,"z":20}]},{"name":"retreat-dig-build","radius":{"8":1.5,"10":1.4,"11":1.4},"prims":[{"op":"line","b":"solid","hex":16052970,"a":0.9,"w":0.34,"pts":[-2,0,-20,0,-37.355,0]},{"op":"arrow","b":"solid","hex":16052970,"a":0.9,"x":-38.8,"z":0,"dx":-18.8,"dz":0,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":16052970,"a":0.95,"r":1,"w":0.22,"x":-40,"z":0},{"op":"line","b":"solid","hex":15123280,"a":0.9,"w":0.34,"pts":[6.343503,6.343503,9,9]},{"op":"ring","b":"solid","hex":15123280,"a":0.95,"r":1,"w":0.22,"x":9,"z":9},{"op":"line","b":"dashed","hex":15123280,"a":0.8,"w":0.28,"pts":[9.707107,9.707107,18.129703,18.129703]},{"op":"arrow","b":"solid","hex":15123280,"a":0.8,"x":19.151472,"z":19.151472,"dx":9.444365,"dz":9.444365,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":15123280,"a":0.95,"r":1,"w":0.22,"x":20,"z":20},{"op":"ring","b":"solid","hex":15123280,"a":0.95,"r":1,"w":0.22,"x":5.5,"z":5.5}]},{"name":"short-legs","radius":{"12":1.5},"prims":[{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":2,"z":0},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":3,"z":0},{"op":"line","b":"dashed","hex":6268146,"a":0.8,"w":0.28,"pts":[4,0,6.355,0]},{"op":"arrow","b":"solid","hex":6268146,"a":0.8,"x":7.8,"z":0,"dx":3.8,"dz":0,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":9,"z":0}]},{"name":"building-rally-and-army-rally","radius":{"13":3},"prims":[{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1.1,"w":0.2,"x":-10,"z":-10},{"op":"line","b":"solid","hex":6268146,"a":0.95,"w":0.2,"pts":[-10,-10,-10,-13.2]},{"op":"arrow","b":"solid","hex":6268146,"a":0.95,"x":-8.1,"z":-12.5,"dx":1,"dz":0,"len":1.9,"half":0.6},{"op":"line","b":"dashed","hex":6268146,"a":0.75,"w":0.26,"pts":[54.024922,47.987539,67.365912,41.317044]},{"op":"arrow","b":"solid","hex":6268146,"a":0.75,"x":68.658359,"z":40.67082,"dx":14.633437,"dz":-7.316718,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1.1,"w":0.2,"x":70,"z":40},{"op":"line","b":"solid","hex":6268146,"a":0.95,"w":0.2,"pts":[70,40,70,36.8]},{"op":"arrow","b":"solid","hex":6268146,"a":0.95,"x":71.9,"z":37.5,"dx":1,"dz":0,"len":1.9,"half":0.6}]},{"name":"garrisoned-queued","radius":{"14":1.5},"prims":[{"op":"line","b":"dashed","hex":6268146,"a":0.8,"w":0.28,"pts":[24,22,37.355,22]},{"op":"arrow","b":"solid","hex":6268146,"a":0.8,"x":38.8,"z":22,"dx":14.8,"dz":0,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":40,"z":22},{"op":"line","b":"dashed","hex":6268146,"a":0.8,"w":0.28,"pts":[40.928477,22.371391,57.544179,29.017672]},{"op":"arrow","b":"solid","hex":6268146,"a":0.8,"x":58.885828,"z":29.554331,"dx":17.957351,"dz":7.182941,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":6268146,"a":0.95,"r":1,"w":0.22,"x":60,"z":30}]},{"name":"ability-unknown-type","radius":{"15":1.5},"prims":[{"op":"line","b":"dashed","hex":15026235,"a":0.8,"w":0.28,"pts":[0,0,9.281538,3.867308]},{"op":"arrow","b":"solid","hex":15026235,"a":0.8,"x":10.615385,"z":4.423077,"dx":10.615385,"dz":4.423077,"len":1.7,"half":0.72},{"op":"ring","b":"solid","hex":15026235,"a":0.95,"r":1,"w":0.22,"x":12,"z":5}]}])JSON");
	TSharedPtr<FJsonValue> ParseJson(const TCHAR* Text)
	{
		TSharedPtr<FJsonValue> V;
		const TSharedRef<TJsonReader<TCHAR>> R = TJsonReaderFactory<TCHAR>::Create(Text);
		FJsonSerializer::Deserialize(R, V);
		return V;
	}
	FString HexOf(uint32 H) { return FString::Printf(TEXT("%06x"), H); }
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCOrdersQueueMarkersTest, "ThreeCrossroads.Orders.QueueMarkers", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCOrdersQueueMarkersTest::RunTest(const FString& Parameters)
{
	using namespace TCOrdersTest;
	// The cases are client/main.js unit states (v.x, v.z, v.tgt, v.plan, v.orders, v.rally); the expected primitives are
	// what the pinned client/markers.js planLayer().draw handed its batches for them (queue-oracle.mjs), with each unit's
	// UNITS radius as the pin has it.
	const TSharedPtr<FJsonValue> Cases = ParseJson(QueueCasesJson), Expected = ParseJson(QueueExpectedJson);
	if (!TestTrue(TEXT("fixture parses"), Cases.IsValid() && Expected.IsValid())) return false;
	const TArray<TSharedPtr<FJsonValue>>& CA = Cases->AsArray();
	const TArray<TSharedPtr<FJsonValue>>& EA = Expected->AsArray();
	TestEqual(TEXT("case count"), CA.Num(), EA.Num());
	int32 Prims = 0;
	for (int32 I = 0; I < FMath::Min(CA.Num(), EA.Num()); I++)
	{
		const TSharedPtr<FJsonObject> C = CA[I]->AsObject(), E = EA[I]->AsObject();
		const FString Name = C->GetStringField(TEXT("name"));
		const TSharedPtr<FJsonObject> Radius = E->GetObjectField(TEXT("radius"));
		TMap<int32, FTCMarkUnit> Units;
		for (const TSharedPtr<FJsonValue>& UV : C->GetArrayField(TEXT("units")))
		{
			const TSharedPtr<FJsonObject> U = UV->AsObject();
			FTCMarkUnit M;
			M.Id = (int32)U->GetNumberField(TEXT("id"));
			M.At = FVector2D(U->GetNumberField(TEXT("x")), U->GetNumberField(TEXT("z")));
			M.Radius = Radius->GetNumberField(FString::FromInt(M.Id));
			double Tgt = 0; U->TryGetNumberField(TEXT("tgt"), Tgt); M.Target = (int32)Tgt;
			const TSharedPtr<FJsonObject>* Plan = nullptr;
			if (U->TryGetObjectField(TEXT("plan"), Plan))
			{
				M.PlanKind = (int32)(*Plan)->GetNumberField(TEXT("kind"));
				M.PlanTo = FVector2D((*Plan)->GetNumberField(TEXT("tx")), (*Plan)->GetNumberField(TEXT("tz")));
				const TArray<TSharedPtr<FJsonValue>>& Path = (*Plan)->GetArrayField(TEXT("path"));
				for (int32 K = 0; K + 1 < Path.Num(); K += 2) M.Path.Add(FVector2D(Path[K]->AsNumber(), Path[K + 1]->AsNumber()));
			}
			for (const TSharedPtr<FJsonValue>& OV : U->GetArrayField(TEXT("orders")))
			{
				const TSharedPtr<FJsonObject> O = OV->AsObject();
				FTCQueuedOrder Q; Q.Kind = (int32)O->GetNumberField(TEXT("kind")); Q.X = O->GetNumberField(TEXT("x")); Q.Z = O->GetNumberField(TEXT("z"));
				M.Orders.Add(Q);
			}
			const TSharedPtr<FJsonObject>* Rally = nullptr;
			if (U->TryGetObjectField(TEXT("rally"), Rally)) { M.bRally = true; M.Rally = FVector2D((*Rally)->GetNumberField(TEXT("x")), (*Rally)->GetNumberField(TEXT("z"))); }
			Units.Add(M.Id, M);
		}
		TArray<int32> Sel;
		for (const TSharedPtr<FJsonValue>& S : C->GetArrayField(TEXT("selected"))) Sel.Add((int32)S->AsNumber());
		FVector2D Army;
		const TSharedPtr<FJsonObject>* AR = nullptr;
		const bool bArmy = C->TryGetObjectField(TEXT("rally"), AR);
		if (bArmy) Army = FVector2D((*AR)->GetNumberField(TEXT("x")), (*AR)->GetNumberField(TEXT("z")));
		const TArray<FTCMarkPrim> Got = TCOrders::Markers(Sel, Units, bArmy ? &Army : nullptr);
		const TArray<TSharedPtr<FJsonValue>>& Want = E->GetArrayField(TEXT("prims"));
		if (!TestEqual(*FString::Printf(TEXT("%s: primitive count"), *Name), Got.Num(), Want.Num())) continue;
		for (int32 K = 0; K < Got.Num(); K++)
		{
			const TSharedPtr<FJsonObject> W = Want[K]->AsObject();
			const FTCMarkPrim& G = Got[K];
			const FString Op = W->GetStringField(TEXT("op")), Tag = FString::Printf(TEXT("%s #%d %s"), *Name, K, *Op);
			const TCHAR* GotOp = G.Op == FTCMarkPrim::EOp::Line ? TEXT("line") : G.Op == FTCMarkPrim::EOp::Arrow ? TEXT("arrow") : TEXT("ring");
			TestEqual(*(Tag + TEXT(" op")), FString(GotOp), Op);
			TestEqual(*(Tag + TEXT(" batch")), FString(G.bDashed ? TEXT("dashed") : TEXT("solid")), W->GetStringField(TEXT("b")));
			TestEqual(*(Tag + TEXT(" colour")), HexOf(G.Hex), HexOf((uint32)W->GetNumberField(TEXT("hex"))));
			TestEqual(*(Tag + TEXT(" alpha")), (double)G.Alpha, W->GetNumberField(TEXT("a")), 1e-6);
			if (Op == TEXT("line"))
			{
				TestEqual(*(Tag + TEXT(" width")), G.Width, W->GetNumberField(TEXT("w")), 1e-9);
				const TArray<TSharedPtr<FJsonValue>>& Pts = W->GetArrayField(TEXT("pts"));
				if (TestEqual(*(Tag + TEXT(" points")), G.Points.Num() * 2, Pts.Num()))
					for (int32 P = 0; P < G.Points.Num(); P++)
					{
						TestEqual(*FString::Printf(TEXT("%s point %d x"), *Tag, P), G.Points[P].X, Pts[P * 2]->AsNumber(), 2e-6);
						TestEqual(*FString::Printf(TEXT("%s point %d z"), *Tag, P), G.Points[P].Y, Pts[P * 2 + 1]->AsNumber(), 2e-6);
					}
			}
			else if (Op == TEXT("arrow"))
			{
				TestEqual(*(Tag + TEXT(" tip x")), G.At.X, W->GetNumberField(TEXT("x")), 2e-6);
				TestEqual(*(Tag + TEXT(" tip z")), G.At.Y, W->GetNumberField(TEXT("z")), 2e-6);
				TestEqual(*(Tag + TEXT(" dx")), G.Dir.X, W->GetNumberField(TEXT("dx")), 2e-6);
				TestEqual(*(Tag + TEXT(" dz")), G.Dir.Y, W->GetNumberField(TEXT("dz")), 2e-6);
				TestEqual(*(Tag + TEXT(" len")), G.Len, W->GetNumberField(TEXT("len")), 1e-9);
				TestEqual(*(Tag + TEXT(" half")), G.Half, W->GetNumberField(TEXT("half")), 1e-9);
			}
			else
			{
				TestEqual(*(Tag + TEXT(" radius")), G.Radius, W->GetNumberField(TEXT("r")), 2e-6);
				TestEqual(*(Tag + TEXT(" width")), G.Width, W->GetNumberField(TEXT("w")), 1e-9);
				TestEqual(*(Tag + TEXT(" x")), G.At.X, W->GetNumberField(TEXT("x")), 2e-6);
				TestEqual(*(Tag + TEXT(" z")), G.At.Y, W->GetNumberField(TEXT("z")), 2e-6);
			}
			Prims++;
		}
	}
	AddInfo(FString::Printf(TEXT("%d cases, %d primitives equal to the pinned markers.js"), CA.Num(), Prims));

	// MarkUnits: the client's state as planLayer reads it (plans, waiting orders, targets, rallies, a garrisoned unit)
	FTCUnitTable Table;
	FString Err;
	if (TestTrue(TEXT("unit table loads"), Table.Load(Err)))
	{
		FTCMatchState St;
		FTCUnit A; A.Id = 1; A.Type = TEXT("rifle"); A.X = 10; A.Z = 10; A.Target = 7; A.Flags = TCFlags::Garrisoned;
		FTCUnit B; B.Id = 7; B.Type = TEXT("tank"); B.X = 30; B.Z = 30;
		FTCUnit H; H.Id = 3; H.Type = TEXT("barracks"); H.X = 50; H.Z = 50;
		St.Units.Add(1, A); St.Units.Add(7, B); St.Units.Add(3, H);
		FTCPlan P; P.Id = 1; P.Kind = 1; P.Tx = 20; P.Tz = 10; P.Path = { FVector2D(20, 10) };
		St.Plans.Add(P);
		FTCWaitingOrders W; W.Id = 1; W.Orders = { { 1, 40.0, 10.0 }, { 9, 50.0, 12.0 } };
		St.WaitingOrders.Add(W);
		const TMap<int32, FTCMarkUnit> MU = TCOrders::MarkUnits(St, Table, { 1, 3 },
			[](int32 Id, FVector2D& Out) { if (Id == 1) { Out = FVector2D(11, 10); return true; } return false; },
			[](int32 Id, FVector2D& Out) { if (Id == 3) { Out = FVector2D(60, 40); return true; } return false; });
		TestTrue(TEXT("the selected units and the target are read"), MU.Contains(1) && MU.Contains(3) && MU.Contains(7));
		if (MU.Contains(1) && MU.Contains(7) && MU.Contains(3))
		{
			TestEqual(TEXT("the drawn position wins"), MU[1].At, FVector2D(11, 10));
			TestEqual(TEXT("the garrisoned squad keeps its plan"), MU[1].PlanKind, 1);
			TestEqual(TEXT("its waiting orders"), MU[1].Orders.Num(), 2);
			TestEqual(TEXT("UNITS radius"), MU[7].Radius, (double)Table.Find(TEXT("tank"))->Radius);
			TestTrue(TEXT("the building's rally"), MU[3].bRally && MU[3].Rally == FVector2D(60, 40));
			TestFalse(TEXT("the target is not drawn as selected"), MU[7].bRally || MU[7].Orders.Num() > 0);
		}
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCOrdersEncodingTest, "ThreeCrossroads.Orders.Encoding", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCOrdersEncodingTest::RunTest(const FString& Parameters)
{
	// client/orders.js dispatch send: one command takes queue as its last field; several go in { t: 'orders' }
	const TSharedPtr<FJsonObject> Escort = TCOrders::EscortCommand({ 41, 42 }, 9);
	TestEqualSensitive(TEXT("escort, queued"), TCOrders::ToJson(TCOrders::Wrap({ Escort }, true)), FString(TEXT(R"({"t":"escort","ids":[41,42],"target":9,"queue":true})")));
	TestEqualSensitive(TEXT("escort, not queued"), TCOrders::ToJson(TCOrders::Wrap({ Escort }, false)), FString(TEXT(R"({"t":"escort","ids":[41,42],"target":9,"queue":false})")));
	TestFalse(TEXT("Wrap leaves the part without a queue field"), Escort->HasField(TEXT("queue")));
	TestFalse(TEXT("no planes: no escort"), TCOrders::EscortCommand({}, 9).IsValid());
	// a house click with squads and a mortar: garrison and fireat in one group (client/orders.js order 'house')
	TSharedPtr<FJsonObject> G = MakeShared<FJsonObject>();
	G->SetStringField(TEXT("t"), TEXT("garrison")); G->SetArrayField(TEXT("ids"), { MakeShared<FJsonValueNumber>(5) }); G->SetNumberField(TEXT("x"), 23); G->SetNumberField(TEXT("z"), 41);
	TSharedPtr<FJsonObject> F = MakeShared<FJsonObject>();
	F->SetStringField(TEXT("t"), TEXT("fireat")); F->SetArrayField(TEXT("ids"), { MakeShared<FJsonValueNumber>(6) }); F->SetNumberField(TEXT("x"), 23); F->SetNumberField(TEXT("z"), 41);
	TestEqualSensitive(TEXT("garrison + fireat, queued"), TCOrders::ToJson(TCOrders::Wrap({ G, nullptr, F }, true)),
		FString(TEXT(R"({"t":"orders","queue":true,"commands":[{"t":"garrison","ids":[5],"x":23,"z":41},{"t":"fireat","ids":[6],"x":23,"z":41}]})")));
	TestEqualSensitive(TEXT("garrison alone, queued"), TCOrders::ToJson(TCOrders::Wrap({ G }, true)), FString(TEXT(R"({"t":"garrison","ids":[5],"x":23,"z":41,"queue":true})")));
	TestFalse(TEXT("nothing to send"), TCOrders::Wrap({ nullptr }, true).IsValid());
	// client/main.js railTo: no queue field at all
	TestEqualSensitive(TEXT("rail"), TCOrders::ToJson(TCOrders::RailCommand({ 3, 4 }, 120.5, 64)), FString(TEXT(R"({"t":"rail","ids":[3,4],"x":120.5,"z":64})")));
	TestFalse(TEXT("no railers: nothing"), TCOrders::RailCommand({}, 1, 1).IsValid());
	// stop: every selected id
	TestEqualSensitive(TEXT("stop"), TCOrders::ToJson(TCOrders::PlanStop(TSet<int32>{ 8 }).Message), FString(TEXT(R"({"t":"stop","ids":[8]})")));
	// shared/sim.js enqueueOrder refuses a ninth waiting order with 'queueFull'; the browser says so for any command but buy
	TestEqualSensitive(TEXT("a full order queue"), TCOrders::DenySentence(TEXT("queueFull"), TEXT("move")), FString(TEXT("This unit already has 8 queued orders")));
	TestEqualSensitive(TEXT("a full training queue"), TCOrders::DenySentence(TEXT("queueFull"), TEXT("buy")), FString(TEXT("The training queue is full")));
	TestEqualSensitive(TEXT("rail: no station"), TCOrders::DenySentence(TEXT("station"), TEXT("rail")), FString(TEXT("Rail moves start within 30 m of one of your stations and go to another station on your land")));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCOrdersRoofBarTest, "ThreeCrossroads.Orders.RoofBar", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCOrdersRoofBarTest::RunTest(const FString& Parameters)
{
	// main.js bars for a garrisoned squad at full health, not suppressed, no stars: background, health bar, badge
	const TArray<TCOrders::FBarRect> R = TCOrders::RoofBarRects(1.0, 0, 0);
	TestEqual(TEXT("three meshes"), R.Num(), 3);
	TestEqual(TEXT("full health bar width"), R[1].W, 2.3);
	TestEqual(TEXT("health bar centred"), R[1].X, 0.0);
	const TArray<TCOrders::FBarRect> Hurt = TCOrders::RoofBarRects(0.5, 40, 2);
	TestEqual(TEXT("hurt, suppressed, two stars: six meshes"), Hurt.Num(), 6);
	TestEqual(TEXT("half health shifts left"), Hurt[1].X, -0.575);
	TestEqual(TEXT("suppression bar"), Hurt[2].W, 2.3 * 0.4, 1e-12);
	// a camera looking straight down -Z... here a plain orthographic projection: 10 px per metre, screen y down
	auto Ortho = [](const FVector& P, FVector2D& Out) { Out = FVector2D(500 + P.Y / 10.0, 300 - P.Z / 10.0); return true; };
	const FVector Anchor(0, 0, 750), Right(0, 1, 0), Up(0, 0, 1);
	const TArray<FBox2D> B = TCOrders::RoofBarScreenBounds(Anchor, Right, Up, R, Ortho);
	TestEqual(TEXT("every mesh projects"), B.Num(), 3);
	TestEqual(TEXT("background bounds x0"), B[0].Min.X, 500 - 12.0, 1e-9);
	TestEqual(TEXT("background bounds y1"), B[0].Max.Y, 225 + 2.1, 1e-9);
	TestEqual(TEXT("over the bar: distance 0"), TCOrders::RoofPickDistance(B, FVector2D(500, 225)), 0.0);
	TestTrue(TEXT("5 px above the badge picks (radius 6)"), TCOrders::RoofPickDistance(B, FVector2D(480.2, 225 - 0.8 - 7.25 - 5)) < TCOrders::RoofPickRadius);
	TestTrue(TEXT("20 px right of the bar does not"), TCOrders::RoofPickDistance(B, FVector2D(532, 225)) >= TCOrders::RoofPickRadius);
	TestTrue(TEXT("a box over the bar takes it"), TCOrders::RoofBoxHits(B, FBox2D(FVector2D(505, 220), FVector2D(540, 260))));
	TestFalse(TEXT("a box below the house does not"), TCOrders::RoofBoxHits(B, FBox2D(FVector2D(480, 280), FVector2D(540, 320))));
	auto Behind = [](const FVector&, FVector2D&) { return false; };
	TestEqual(TEXT("behind the camera: no points"), TCOrders::RoofBarScreenBounds(Anchor, Right, Up, R, Behind).Num(), 0);

	// hud.js: 'Garrisoned: <house>' (flag 32, house type in bits 20 to 22), es.js 'Garrisoned: {0}' with the house's name
	const int32 Brick = TCFlags::Garrisoned | (1 << 20);
	TestEqualSensitive(TEXT("not garrisoned: no tag"), TCOrders::GarrisonTag(1 << 20), FString());
	{
		FTCLocale::FScopedLanguage En(ETCLanguage::English);
		TestEqualSensitive(TEXT("English tag"), TCOrders::GarrisonTag(Brick), FString(TEXT("Garrisoned: brick house")));
	}
	if (FTCLocale::IsLoaded())
	{
		FTCLocale::FScopedLanguage Es(ETCLanguage::Spanish);
		TestEqualSensitive(TEXT("Spanish tag"), TCOrders::GarrisonTag(Brick), FString(TEXT("Guarnecido: casa de ladrillo")));
		TestEqualSensitive(TEXT("Spanish tag, a shed"), TCOrders::GarrisonTag(TCFlags::Garrisoned), FString(TEXT("Guarnecido: cobertizo de madera")));
	}
	else AddError(TEXT("the es.js dictionary did not load"));
	return true;
}
#endif
