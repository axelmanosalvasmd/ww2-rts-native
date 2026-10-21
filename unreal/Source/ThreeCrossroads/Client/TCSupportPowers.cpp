#include "Client/TCSupportPowers.h"
#include "Client/TCArmyCommands.h"
#include "Client/TCRecruitment.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "InputCoreTypes.h"
#include <cmath>
#include <limits>

// Written out from the reference at e4e579a (shared/sim.js SUPPORT after its Classic Munitions and plane assignments,
// client/hud.js SUPPORT_TIP and buildSupport's title, client/keys.js SUPPORT_KEYS). When the pin moves, compare every
// row again and move ReferencePin with it.
const TCHAR* const FTCSupportPowers::ReferencePin = TEXT("e4e579a");

// Helpers live in a named namespace: this file may share a unity translation unit with the other client modules,
// whose anonymous-namespace helpers have the same plain names.
namespace TCSPI
{
	struct FSupportRow
	{
		const TCHAR* Kind;
		const TCHAR* Name;
		int32 Cost, Mun, Cd;
		double Delay;
		bool bPoint, bPlane;
		double Len, Width;   // strips (0 for point kinds)
		double Radius;       // point kinds: radius ?? blast ?? 4
		const TCHAR* Key;
		const TCHAR* Tip;
	};

	// shared/sim.js SUPPORT, SUPPORT_TYPES order. Radius: dive blast 5, para neither radius nor blast (4), cover radius 40.
	const FSupportRow Rows[] = {
		{ TEXT("recon"), TEXT("Recon Flight"), 60, 25, 45, 3, false, true, 80, 30, 0, TEXT("Z"),
			TEXT("Recon Flight (Z): Reveals a wide area for 15s. Aim: click the center, move to turn, click to call it in") },
		{ TEXT("artillery"), TEXT("Artillery Barrage"), 150, 60, 60, 5, false, false, 24, 10, 0, TEXT("C"),
			TEXT("Artillery Barrage (C): 10 shells on an area after a 5s warning (UK: 15, the 25-pounder doctrine). Aim: click the center, move to turn, click to call it in") },
		{ TEXT("strafe"), TEXT("Strafing Run"), 200, 80, 90, 5, false, true, 36, 8, 0, TEXT("V"),
			TEXT("Strafing Run (V): Plane rakes a line from your HQ outward. Aim: click the center, move to turn, click to call it in") },
		{ TEXT("smoke"), TEXT("Smoke Barrage"), 50, 20, 40, 3, false, false, 36, 14, 0, TEXT("B"),
			TEXT("Smoke Barrage (B): Smoke screen over an area for 20s: blocks sight both ways. Aim: click the center, move to turn, click to call it in") },
		{ TEXT("bombing"), TEXT("Bombing Run"), 250, 100, 120, 6, false, true, 40, 8, 0, TEXT("N"),
			TEXT("Bombing Run (N): A stick of heavy bombs along the line: flattens houses, kills tanks. Aim: click the center, move to turn, click to call it in") },
		{ TEXT("dive"), TEXT("Dive Bomber"), 180, 70, 75, 5, true, true, 0, 0, 5, TEXT("U"),
			TEXT("Dive Bomber (U): One heavy bomb, right on the spot: tanks, guns, houses. Click the spot to call it in") },
		{ TEXT("para"), TEXT("Paratroopers"), 260, 90, 90, 6, true, true, 0, 0, 4, TEXT("P"),
			TEXT("Paratroopers (P): Drops two rifle squads and an MG team where your side can see (3 toward pop). Click the spot to call it in") },
		{ TEXT("cover"), TEXT("Fighter Cover"), 120, 40, 90, 2, true, false, 0, 0, 40, TEXT("I"),
			TEXT("Fighter Cover (I): Fighters intercept the next enemy air strike over the area for 60s (not recon). Click the spot to call it in") },
	};
	// SUPPORT.para.units
	const TCHAR* const ParaUnits[] = { TEXT("rifle"), TEXT("rifle"), TEXT("mg") };

	// UNIT_TYPES with w.salvo, and with w.shellTerrain
	const TCHAR* const SalvoTypes[] = { TEXT("rocket"), TEXT("mortar"), TEXT("destroyer"), TEXT("howitzer"), TEXT("bomber") };
	const TCHAR* const ShellTerrainTypes[] = { TEXT("tank"), TEXT("medium"), TEXT("tankdestroyer"), TEXT("tiger"), TEXT("churchill") };

	// UNITS.rocket.w.spread (aimShape 'area'); CFG.paraRange; strikeShape's fallback ring
	constexpr double AreaRadius = 6;
	constexpr double ParaRange = 300;
	constexpr double FallbackStrikeRadius = 4;
	// client/main.js: a strip's second click within this of the centre takes defaultDir
	constexpr double TurnThreshold = 1.5;

	// client/keys.js: the recruit and building contexts' card letters, cards 1 to 15 in this order
	const TCHAR* const CardLetters = TEXT("QWERTASDFGZXCVB");

	// client/main.js markers: a point support 0xffa030, a fireat 0xff4030
	const FColor SupportMarker(0xff, 0xa0, 0x30);
	const FColor FireMarker(0xff, 0x40, 0x30);

	// client/main.js setAim and the targeting click
	const TCHAR* const CenterHintText = TEXT("Click to set the center. Right-click cancels");
	const TCHAR* const RotateHintText = TEXT("Move the mouse to rotate, then click to launch");
	const TCHAR* const AreaHintText = TEXT("Click the ground to shell (they keep firing until given another order). Right-click cancels");
	const TCHAR* const NoShellersText = TEXT("Select a mortar, howitzer, rocket truck, destroyer or bomber to shell an area");
	// client/hud.js drawOrders, the Shell area button (label('area') is Shift+B, badge('area') ⇧B)
	const TCHAR* const AreaLabelText = TEXT("Shift+B");
	const TCHAR* const AreaBadgeText = TEXT("⇧B");
	const TCHAR* const AreaTipText = TEXT("Shell area (Shift+B): click any ground, seen or not; mortars, howitzers, rocket trucks and ships move into range and keep firing on it until given another order, bombers drop every stick on it. Shift+click queues it");

	struct FTable
	{
		TArray<FTCSupportDef> Defs;
		TMap<FString, int32> Index;
		FTable()
		{
			for (const FSupportRow& R : Rows)
			{
				FTCSupportDef D;
				D.Kind = R.Kind; D.Name = R.Name; D.Cost = R.Cost; D.Mun = R.Mun; D.Cd = R.Cd; D.Delay = R.Delay;
				D.bPoint = R.bPoint; D.bPlane = R.bPlane; D.Len = R.Len; D.Width = R.Width; D.Radius = R.Radius;
				D.KeyLabel = R.Key; D.Tip = R.Tip;
				if (D.Kind == TEXT("para")) for (const TCHAR* T : ParaUnits) D.Units.Add(T);
				Index.Add(D.Kind, Defs.Num());
				Defs.Add(MoveTemp(D));
			}
		}
	};
	const FTable& Table() { static const FTable T; return T; }

	bool Num(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return FMath::IsFinite(Out);
	}

	// How JavaScript writes a number into a sentence or a key (String(n)), for the values these hold.
	FString JsNumber(double V)
	{
		if (FMath::IsNaN(V)) return TEXT("NaN");
		if (!FMath::IsFinite(V)) return V > 0 ? TEXT("Infinity") : TEXT("-Infinity");
		if (V == 0) return TEXT("0");
		if (V == FMath::FloorToDouble(V) && FMath::Abs(V) < 1e21) return FString::Printf(TEXT("%.0f"), V);
		FString S = FString::Printf(TEXT("%.15g"), V);
		if (FCString::Atod(*S) == V) return S;
		S = FString::Printf(TEXT("%.16g"), V);
		if (FCString::Atod(*S) == V) return S;
		return FString::Printf(TEXT("%.17g"), V);
	}

	// V8's Math.hypot for two numbers (a scaled, compensated sum), for the 1.5 m turn threshold
	double Hypot(double A, double B)
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

	// client/availability.js cooldownSeconds
	double CooldownSeconds(double Cd) { return FMath::Max(0.0, FMath::CeilToDouble(Cd)); }

	FTCSupportCheck Yes() { FTCSupportCheck C; C.bOk = true; return C; }
	FTCSupportCheck No(const FString& Reason, bool bNative = false) { FTCSupportCheck C; C.Reason = Reason; C.bNative = bNative; return C; }

	TArray<TSharedPtr<FJsonValue>> IdArray(const TArray<int32>& Ids)
	{
		TArray<TSharedPtr<FJsonValue>> Out;
		Out.Reserve(Ids.Num());
		for (int32 Id : Ids) Out.Add(MakeShared<FJsonValueNumber>(Id));
		return Out;
	}

	bool InList(const FString& Type, const TCHAR* const* List, int32 N)
	{
		for (int32 I = 0; I < N; I++) if (Type == List[I]) return true;
		return false;
	}

	void Refuse(FTCSupportPowerPlan& Plan, const FTCSupportCheck& C)
	{
		Plan.Note = C.Reason;
		Plan.bNative = C.bNative;
		Plan.bFeedback = true;
	}

	// shared/sim.js cellOf(g, num(x, w * CELL), num(z, h * CELL)) >= 0, for the map this client holds
	bool CellOnMap(const FTCMatchState& State, FVector2D At)
	{
		const double Cell = FTCMatchState::CellMetres, W = State.Map.W, H = State.Map.H;
		if (!FMath::IsFinite(At.X) || !FMath::IsFinite(At.Y) || W <= 0 || H <= 0) return false;
		const double X = FMath::Clamp(At.X, 0.0, W * Cell), Z = FMath::Clamp(At.Y, 0.0, H * Cell);
		const double CX = FMath::FloorToDouble(X / Cell), CY = FMath::FloorToDouble(Z / Cell);
		return CX >= 0 && CY >= 0 && CX < W && CY < H;
	}
}

const TCHAR* LexToString(ETCSupportAimKind Kind)
{
	switch (Kind)
	{
	case ETCSupportAimKind::Support: return TEXT("support");
	case ETCSupportAimKind::Area: return TEXT("area");
	default: return TEXT("none");
	}
}

const TCHAR* LexToString(ETCSupportPowerCommand Kind)
{
	switch (Kind)
	{
	case ETCSupportPowerCommand::ArmAim: return TEXT("arm-support");
	case ETCSupportPowerCommand::PinCenter: return TEXT("support-center");
	case ETCSupportPowerCommand::Support: return TEXT("support");
	case ETCSupportPowerCommand::FireAt: return TEXT("fireat");
	default: return TEXT("none");
	}
}

FTCOrderPlan FTCSupportPowerPlan::ToOrderPlan() const
{
	FTCOrderPlan Plan;
	Plan.Kind = ETCOrderKind::None;
	Plan.Message = Message;
	Plan.Units = Units;
	Plan.At = bMarker ? At : FVector2D(-1, -1);
	Plan.Color = Color;
	Plan.Why = Note;
	return Plan;
}

void FTCSupportAim::Cancel()
{
	Kind = ETCSupportAimKind::None;
	SupportKind.Reset();
	bHasCenter = false;
	Center = FVector2D::ZeroVector;
	Hint.Reset();
	MeshKind.Reset();
	bMeshPlaced = false;
	MeshAt = FVector2D::ZeroVector;
	MeshDir = 0;
}

// ---------- the table ----------

const FTCSupportDef* FTCSupportPowers::Find(const FString& Kind)
{
	const int32* I = TCSPI::Table().Index.Find(Kind);
	return I ? &TCSPI::Table().Defs[*I] : nullptr;
}

const TArray<FTCSupportDef>& FTCSupportPowers::All() { return TCSPI::Table().Defs; }
bool FTCSupportPowers::IsSalvo(const FString& Type) { return TCSPI::InList(Type, TCSPI::SalvoTypes, (int32)UE_ARRAY_COUNT(TCSPI::SalvoTypes)); }
bool FTCSupportPowers::ShellsTerrain(const FString& Type) { return TCSPI::InList(Type, TCSPI::ShellTerrainTypes, (int32)UE_ARRAY_COUNT(TCSPI::ShellTerrainTypes)); }
const TCHAR* FTCSupportPowers::CenterHint() { return TCSPI::CenterHintText; }
const TCHAR* FTCSupportPowers::RotateHint() { return TCSPI::RotateHintText; }
const TCHAR* FTCSupportPowers::AreaHint() { return TCSPI::AreaHintText; }
const TCHAR* FTCSupportPowers::NoShellersHint() { return TCSPI::NoShellersText; }
const TCHAR* FTCSupportPowers::AreaLabel() { return TCSPI::AreaLabelText; }
const TCHAR* FTCSupportPowers::AreaBadge() { return TCSPI::AreaBadgeText; }
double FTCSupportPowers::AreaRadius() { return TCSPI::AreaRadius; }

FTCSupportKeyMatch FTCSupportPowers::KeyAction(const FKey& Key, bool bShift, bool bCtrl, bool bAlt)
{
	// client/keys.js match: code, shift, ctrl (or meta) and alt must all equal the binding's. support:* have no
	// modifiers; 'area' is Shift+B. Both are 'global'.
	FTCSupportKeyMatch Out;
	if (bCtrl || bAlt) return Out;
	const FKey* Letters[] = { &EKeys::Z, &EKeys::C, &EKeys::V, &EKeys::B, &EKeys::N, &EKeys::U, &EKeys::P, &EKeys::I };
	static_assert(UE_ARRAY_COUNT(Letters) == UE_ARRAY_COUNT(TCSPI::Rows), "one key per SUPPORT kind, in SUPPORT_TYPES order");
	FString Letter;
	if (bShift)
	{
		if (Key != EKeys::B) return Out;
		Out.Action = ETCSupportPowerKey::Area;
		Letter = TEXT("B");
	}
	else
	{
		for (int32 I = 0; I < (int32)UE_ARRAY_COUNT(Letters); I++)
		{
			if (Key != *Letters[I]) continue;
			Out.Action = ETCSupportPowerKey::Support;
			Out.Kind = TCSPI::Rows[I].Kind;
			Letter = TCSPI::Rows[I].Key;
			break;
		}
		if (Out.Action == ETCSupportPowerKey::None) return Out;
	}
	// the card letter that outranks it in the recruit and building contexts (card:N, or cardMany:N with Shift)
	int32 Index = INDEX_NONE;
	if (FString(TCSPI::CardLetters).FindChar(Letter[0], Index)) Out.Card = Index + 1;
	return Out;
}

// ---------- the snapshot ----------

void FTCSupportPowers::Clear()
{
	bStarted = bHasSnapshot = false;
	SnapMp.Reset(); SnapMun.Reset();
	ModeKind.Reset();
	OutSeats.Reset(); PrevOutSeats.Reset();
	Sup.Reset();
	HomeAt.Reset();
	StrikeRows.Reset();
	AirfieldBuilt.Reset();
}

void FTCSupportPowers::ApplyStart(const FJsonObject& M)
{
	// client/main.js startGame: lastSnap = null (worldState.reset()), home = m.spawn
	Clear();
	bStarted = true;
	const TSharedPtr<FJsonObject>* Spawn = nullptr;
	double X, Z;
	if (M.TryGetObjectField(TEXT("spawn"), Spawn) && Spawn && Spawn->IsValid() && TCSPI::Num((*Spawn)->TryGetField(TEXT("x")), X) && TCSPI::Num((*Spawn)->TryGetField(TEXT("z")), Z))
	{
		HomeAt = FVector2D(X, Z);
	}
}

void FTCSupportPowers::ApplySnapshot(const FJsonObject& M)
{
	if (!bStarted) return;
	bHasSnapshot = true;
	// shared/sim.js snapshotFor sends these whole in every snapshot: mp always, mun in Classic and World only, sup
	// (every kind's cooldown, Math.ceil), strikes (public), mode, out
	double D;
	SnapMp.Reset(); if (TCSPI::Num(M.TryGetField(TEXT("mp")), D)) SnapMp = D;
	SnapMun.Reset(); if (TCSPI::Num(M.TryGetField(TEXT("mun")), D)) SnapMun = D;
	ModeKind.Reset();
	const TSharedPtr<FJsonObject>* Obj = nullptr;
	if (M.TryGetObjectField(TEXT("mode"), Obj) && Obj && Obj->IsValid()) (*Obj)->TryGetStringField(TEXT("kind"), ModeKind);
	PrevOutSeats = MoveTemp(OutSeats);
	OutSeats.Reset();
	const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
	if (M.TryGetArrayField(TEXT("out"), Arr) && Arr) for (const TSharedPtr<FJsonValue>& V : *Arr) OutSeats.Add(V.IsValid() && V->Type == EJson::Boolean && V->AsBool());
	Sup.Reset();
	if (M.TryGetObjectField(TEXT("sup"), Obj) && Obj && Obj->IsValid())
	{
		for (const auto& Pair : (*Obj)->Values) if (TCSPI::Num(Pair.Value, D)) Sup.Add(FString(*Pair.Key), D);
	}

	// client/main.js applySnapshot: if (s.home ?? s.world?.home) home = { x: p[0], z: p[1] } (World only, at the pin)
	TSharedPtr<FJsonValue> HomeV = M.TryGetField(TEXT("home"));
	if (!HomeV.IsValid() || HomeV->Type == EJson::Null)
	{
		HomeV.Reset();
		if (M.TryGetObjectField(TEXT("world"), Obj) && Obj && Obj->IsValid()) HomeV = (*Obj)->TryGetField(TEXT("home"));
	}
	const TArray<TSharedPtr<FJsonValue>>* HomeArr = nullptr;
	double HX, HZ;
	if (HomeV.IsValid() && HomeV->TryGetArray(HomeArr) && HomeArr && HomeArr->Num() >= 2 && TCSPI::Num((*HomeArr)[0], HX) && TCSPI::Num((*HomeArr)[1], HZ)) HomeAt = FVector2D(HX, HZ);

	// strikes: [kind, x, z, dir, t, owner]
	StrikeRows.Reset();
	if (M.TryGetArrayField(TEXT("strikes"), Arr) && Arr)
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row = nullptr;
			if (!V.IsValid() || !V->TryGetArray(Row) || !Row || Row->Num() < 6) continue;
			FStrikeRow S;
			double Owner;
			if (!(*Row)[0].IsValid() || !(*Row)[0]->TryGetString(S.Kind)) continue;
			if (!TCSPI::Num((*Row)[1], S.X) || !TCSPI::Num((*Row)[2], S.Z) || !TCSPI::Num((*Row)[3], S.Dir) || !TCSPI::Num((*Row)[4], S.T) || !TCSPI::Num((*Row)[5], Owner)) continue;
			S.Owner = (int32)Owner;
			StrikeRows.Add(MoveTemp(S));
		}
	}

	// Airfield rows' built as sent, held the way client/world-state.js holds rows (as FTCSupportOrders does): all (or
	// no held) replaces, gone removes, rows upsert.
	const TArray<TSharedPtr<FJsonValue>>* Held = nullptr;
	const bool bHasHeld = M.TryGetArrayField(TEXT("held"), Held) && Held && Held->Num() >= 2;
	bool bAll = false;
	M.TryGetBoolField(TEXT("all"), bAll);
	if (bAll || !bHasHeld) AirfieldBuilt.Reset();
	if (M.TryGetArrayField(TEXT("gone"), Arr) && Arr)
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr) if (V.IsValid() && V->Type == EJson::Number) AirfieldBuilt.Remove((int32)V->AsNumber());
	}
	if (M.TryGetArrayField(TEXT("units"), Arr) && Arr)
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row = nullptr;
			if (!V.IsValid() || !V->TryGetArray(Row) || !Row || Row->Num() < 2) continue;
			double IdD;
			if (!TCSPI::Num((*Row)[0], IdD)) continue;
			const int32 Id = (int32)IdD;
			FString Type;
			if (!(*Row)[1].IsValid() || !(*Row)[1]->TryGetString(Type) || Type != TEXT("airfield")) { AirfieldBuilt.Remove(Id); continue; }
			// unitRow: u.built ?? 1 (the raw double)
			TSharedPtr<FJsonValue> B;
			if (Row->Num() > 14) B = (*Row)[14];
			if (!B.IsValid() || B->Type == EJson::Null || B->Type == EJson::None) AirfieldBuilt.Add(Id, 1);
			else if (B->Type == EJson::Number) AirfieldBuilt.Add(Id, B->AsNumber());
			else AirfieldBuilt.Remove(Id);
		}
	}
}

bool FTCSupportPowers::WentOut(int32 Seat) const
{
	const bool bNow = OutSeats.IsValidIndex(Seat) && OutSeats[Seat];
	const bool bBefore = PrevOutSeats.IsValidIndex(Seat) && PrevOutSeats[Seat];
	return bHasSnapshot && bNow && !bBefore;
}

bool FTCSupportPowers::ConstructionMode() const { return ModeKind == TEXT("classic") || ModeKind == TEXT("world"); }

double FTCSupportPowers::Cooldown(const FString& Kind) const
{
	const double* C = Sup.Find(Kind);
	return C ? *C : 0;
}

int32 FTCSupportPowers::Cost(const FString& Kind, bool* bOutMun) const
{
	// shared/sim.js supCost: isConstructionMode(g) ? { cur: 'mun', cost: SUPPORT[k].mun } : { cur: 'mp', cost: SUPPORT[k].cost }
	const bool bMun = ConstructionMode();
	if (bOutMun) *bOutMun = bMun;
	const FTCSupportDef* D = Find(Kind);
	return D ? (bMun ? D->Mun : D->Cost) : 0;
}

double FTCSupportPowers::BuiltOf(const FTCUnit& U) const
{
	const double* B = U.Type == TEXT("airfield") ? AirfieldBuilt.Find(U.Id) : nullptr;
	return B ? *B : (double)U.Built;
}

double FTCSupportPowers::DefaultDir(FVector2D At) const
{
	// client/main.js defaultDir for a support kind: planes fly out from home
	return HomeAt.IsSet() ? std::atan2(At.Y - HomeAt->Y, At.X - HomeAt->X) : 0;
}

// ---------- availability ----------

FTCSupportCheck FTCSupportPowers::Gate(const FTCMatchState& State, const FTCUnitTable& Tbl) const
{
	if (Tbl.Reference != ReferencePin)
	{
		return TCSPI::No(FString::Printf(TEXT("Support data is for reference %s, the unit table is for %s"), ReferencePin,
			Tbl.Reference.IsEmpty() ? TEXT("an unknown reference") : *Tbl.Reference), true);
	}
	// client/availability.js: if (!s) 'Waiting for the army'; if (action.watching || s.out?.[action.slot]) 'You are spectating'
	if (!bHasSnapshot || !State.bHasMatch) return TCSPI::No(TEXT("Waiting for the army"));
	if (State.bSpectator || (OutSeats.IsValidIndex(State.You) && OutSeats[State.You])) return TCSPI::No(TEXT("You are spectating"));
	if (!State.SeatConfirmed()) return TCSPI::No(TEXT("Waiting for the lobby to confirm this seat"), true);
	return TCSPI::Yes();
}

FTCSupportCheck FTCSupportPowers::Check(const FTCMatchState& State, const FTCUnitTable& Tbl, const FTCRecruitment& Recruit, const FString& Kind) const
{
	const FTCSupportCheck G = Gate(State, Tbl);
	if (!G.bOk) return G;
	const FTCSupportDef* Def = Find(Kind);
	if (!Def) return TCSPI::No(FString::Printf(TEXT("Unknown support kind '%s'"), *Kind), true); // the browser would throw (supCost)
	// const cd = s.sup?.[action.kind] ?? 0; if (cd > 0) 'Cooldown N s'
	const double Cd = Cooldown(Kind);
	if (Cd > 0) return TCSPI::No(FString::Printf(TEXT("Cooldown %s s"), *TCSPI::JsNumber(TCSPI::CooldownSeconds(Cd))));
	// resources(s, cur === 'mp' ? cost : 0, 0, cur === 'mun' ? cost : 0)
	bool bMun = false;
	const int32 Price = Cost(Kind, &bMun);
	const int32 Mp = bMun ? 0 : Price, Mun = bMun ? Price : 0;
	if (SnapMp.IsSet() && SnapMp.GetValue() < Mp) return TCSPI::No(FString::Printf(TEXT("Needs %d MP"), Mp));
	if (Mun && !(SnapMun.IsSet() && SnapMun.GetValue() >= Mun)) return TCSPI::No(FString::Printf(TEXT("Needs %d munitions"), Mun));
	if (Kind != TEXT("para")) return TCSPI::Yes();
	// World: a finished own Airfield anywhere (the server also wants it within CFG.paraRange of the drop)
	if (ModeKind == TEXT("world"))
	{
		bool bField = false;
		for (const auto& Pair : State.Units)
		{
			const FTCUnit& U = Pair.Value;
			if (U.Owner == State.You && U.Type == TEXT("airfield") && BuiltOf(U) >= 1) { bField = true; break; }
		}
		if (!bField) return TCSPI::No(TCOrders::DenySentence(TEXT("airfield"), TEXT("support")));
	}
	// population(null, dropPop('para')): popTotal(own, queued) + need > (s.world?.cap ?? popCap(s))
	if (!Recruit.ReferenceMatches()) return TCSPI::No(TEXT("Paratroopers: the recruit data is not at the unit table's reference"), true);
	double Need = 0;
	for (const FString& T : Def->Units)
	{
		const FTCRecruitDef* R = FTCRecruitment::Find(T);
		if (!R) return TCSPI::No(FString::Printf(TEXT("Paratroopers: unknown unit type '%s'"), *T), true);
		Need += R->Pop;
	}
	double Pop = 0, Cap = 0;
	if (!Recruit.ArmyUse(State, Pop, Cap)) return TCSPI::No(TEXT("Paratroopers: this client cannot work out the army's size"), true);
	if (Pop + Need > Cap) return TCSPI::No(FString::Printf(TEXT("Army at its limit (%s/%s)"), *TCSPI::JsNumber(Pop), *TCSPI::JsNumber(Cap)));
	return TCSPI::Yes();
}

TArray<FTCSupportButton> FTCSupportPowers::Buttons(const FTCMatchState& State, const FTCUnitTable& Tbl, const FTCRecruitment& Recruit) const
{
	TArray<FTCSupportButton> Out;
	for (const FTCSupportDef& D : All())
	{
		FTCSupportButton B;
		B.Kind = D.Kind;
		B.Icon = D.Kind;
		B.Key = D.KeyLabel;
		B.Tip = D.Tip;
		B.Check = Check(State, Tbl, Recruit, D.Kind);
		if (bHasSnapshot)
		{
			// cd > 0 ? `${cooldownSeconds(cd)}s` : `${cost ?? '?'} ${cur === 'mun' ? 'Mun' : 'MP'}`
			const double Cd = Cooldown(D.Kind);
			bool bMun = false;
			const int32 Price = Cost(D.Kind, &bMun);
			B.Text = Cd > 0 ? FString::Printf(TEXT("%ss"), *TCSPI::JsNumber(TCSPI::CooldownSeconds(Cd))) : FString::Printf(TEXT("%d %s"), Price, bMun ? TEXT("Mun") : TEXT("MP"));
		}
		Out.Add(MoveTemp(B));
	}
	return Out;
}

// ---------- plans ----------

FString FTCSupportPowers::PredictSupport(const FTCMatchState& State, const FTCRecruitment& Recruit, const FTCSupportDef& Def, FVector2D At) const
{
	// shared/sim.js command 'support', over what this seat's snapshot shows. The server checks the coordinates (any
	// finite number is clamped onto the map), the cooldown, the price, then for a drop that the spot is seen ('unseen',
	// not predicted) and the army limit, then (World) an Airfield in range.
	if (Cooldown(Def.Kind) > 0) return TEXT("cooldown");
	bool bMun = false;
	const int32 Price = Cost(Def.Kind, &bMun);
	const TOptional<double>& Wallet = bMun ? SnapMun : SnapMp;
	if (!(Wallet.IsSet() && Wallet.GetValue() >= Price)) return bMun ? TEXT("mun") : TEXT("mp");
	if (!Def.Units.Num()) return FString();
	// popOf: units fielded and queued, plus own paratroopers still in the air (own strikes of a drop kind not yet live)
	double Pop = 0, Cap = 0, Need = 0;
	bool bKnown = Recruit.ReferenceMatches() && Recruit.ArmyUse(State, Pop, Cap);
	for (const FString& T : Def.Units)
	{
		const FTCRecruitDef* R = FTCRecruitment::Find(T);
		if (!R) { bKnown = false; break; }
		Need += R->Pop;
	}
	if (bKnown)
	{
		for (const FStrikeRow& S : StrikeRows) if (S.Owner == State.You && S.Kind == Def.Kind && S.T > 0) Pop += Need;
		if (Pop + Need > Cap) return TEXT("pop");
	}
	// paraField: outside World any spot; in World a finished own Airfield with health within CFG.paraRange
	if (ModeKind == TEXT("world"))
	{
		for (const auto& Pair : State.Units)
		{
			const FTCUnit& U = Pair.Value;
			if (U.Owner == State.You && U.Type == TEXT("airfield") && BuiltOf(U) >= 1 && U.Hp > 0 && TCSPI::Hypot(U.X - At.X, U.Z - At.Y) <= TCSPI::ParaRange) return FString();
		}
		return TEXT("airfield");
	}
	return FString();
}

FTCSupportPowerPlan FTCSupportPowers::ArmSupport(const FTCMatchState& State, const FTCUnitTable& Tbl, const FTCRecruitment& Recruit, const FString& Kind, FTCSupportAim& Aim) const
{
	FTCSupportPowerPlan Plan;
	Plan.SupportKind = Kind;
	// client/main.js aimSupport: if (explainUnavailable(available({ t: 'support', kind: k }))) return; setAim(k)
	const FTCSupportCheck C = Check(State, Tbl, Recruit, Kind);
	if (!C.bOk) { TCSPI::Refuse(Plan, C); return Plan; }
	// setAim(kind): a new aim, no centre; the mesh is kept until a frame sees another kind
	Aim.Kind = ETCSupportAimKind::Support;
	Aim.SupportKind = Kind;
	Aim.bHasCenter = false;
	Aim.Center = FVector2D::ZeroVector;
	Aim.Hint = TCSPI::CenterHintText;
	Plan.Kind = ETCSupportPowerCommand::ArmAim;
	Plan.bArmAim = true;
	Plan.Note = Aim.Hint;
	return Plan;
}

TArray<int32> FTCSupportPowers::Shellers(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	// client/main.js shellers: [...selected].map(id => units.get(id)).filter(v => v?.owner === me && UNITS[v.type].w?.salvo)
	TArray<int32> Out;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (U && State.IsOwn(*U) && IsSalvo(U->Type)) Out.Add(Id);
	}
	return Out;
}

FTCSupportPowerPlan FTCSupportPowers::ArmArea(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected, FTCSupportAim& Aim) const
{
	FTCSupportPowerPlan Plan;
	Plan.SupportKind = TEXT("area");
	if (Tbl.Reference != ReferencePin)
	{
		Plan.Note = Gate(State, Tbl).Reason;
		Plan.bNative = true;
		return Plan;
	}
	// startArea: shellers().length ? setAim('area') : $('hint').textContent = 'Select a mortar, ...'. Shellers are the
	// browser's owner === me; a seat the lobby has not confirmed is checked after (it has no own units here).
	bool bAnyOwnerMatch = false;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (U && U->Owner == State.You && IsSalvo(U->Type)) { bAnyOwnerMatch = true; break; }
	}
	if (!bAnyOwnerMatch)
	{
		Plan.Note = TCSPI::NoShellersText; // the hint line, not a flashed sentence
		return Plan;
	}
	// setAim: if (observing()) return (operative || watching || lastSnap.out[me]), silently
	if (State.bSpectator || (OutSeats.IsValidIndex(State.You) && OutSeats[State.You])) return Plan;
	if (!State.SeatConfirmed())
	{
		Plan.Note = TEXT("Waiting for the lobby to confirm this seat");
		Plan.bNative = true;
		return Plan;
	}
	Aim.Kind = ETCSupportAimKind::Area;
	Aim.SupportKind.Reset();
	Aim.bHasCenter = false;
	Aim.Center = FVector2D::ZeroVector;
	Aim.Hint = TCSPI::AreaHintText;
	Plan.Kind = ETCSupportPowerCommand::ArmAim;
	Plan.bArmAim = true;
	Plan.Note = Aim.Hint;
	return Plan;
}

FTCSupportPowerPlan FTCSupportPowers::AimClick(const FTCMatchState& State, const FTCUnitTable& Tbl, const FTCRecruitment& Recruit, const TSet<int32>& Selected, FTCSupportAim& Aim, bool bHasGround, FVector2D Ground, bool bShift) const
{
	FTCSupportPowerPlan Plan;
	if (!Aim.IsArmed())
	{
		Plan.Note = TEXT("No support or area aim is armed");
		Plan.bNative = true;
		return Plan;
	}
	Plan.SupportKind = Aim.Kind == ETCSupportAimKind::Area ? FString(TEXT("area")) : Aim.SupportKind;
	// if (!g) { feedback.show(denySentence('blocked')); return; }
	if (!bHasGround)
	{
		Plan.Note = TCOrders::DenySentence(TEXT("blocked"), FString());
		Plan.bFeedback = true;
		Plan.bKeepAim = true;
		return Plan;
	}

	if (Aim.Kind == ETCSupportAimKind::Area)
	{
		// const ids = shellers().map(v => v.id); if (!e.shiftKey) cancelAim();
		// if (ids.length) { sendCmd({ t: 'fireat', ids, x: g.x, z: g.z, queue: e.shiftKey }); marker(g.x, g.z, 0xff4030) }
		const TArray<int32> Ids = Shellers(State, Selected);
		if (!bShift) { Aim.Cancel(); Plan.bCancelAim = true; }
		if (!Ids.Num()) return Plan;
		TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
		M->SetStringField(TEXT("t"), TEXT("fireat"));
		M->SetArrayField(TEXT("ids"), TCSPI::IdArray(Ids));
		M->SetNumberField(TEXT("x"), Ground.X);
		M->SetNumberField(TEXT("z"), Ground.Y);
		M->SetBoolField(TEXT("queue"), bShift);
		Plan.Kind = ETCSupportPowerCommand::FireAt;
		Plan.Message = M;
		Plan.Units = Ids.Num();
		Plan.bMarker = true;
		Plan.At = Ground;
		Plan.Color = TCSPI::FireMarker;
		// shared/sim.js command 'fireat': a spot off the map is 'blocked'. The rest needs the cell's health or the
		// server's sight (see FTCSupportPowerPlan::PredictedDeny).
		if (!TCSPI::CellOnMap(State, Ground)) Plan.PredictedDeny = TEXT("blocked");
		return Plan;
	}

	const FTCSupportDef* Def = Find(Aim.SupportKind);
	if (!Def)
	{
		Plan.Note = FString::Printf(TEXT("Unknown support kind '%s'"), *Aim.SupportKind);
		Plan.bNative = true;
		Plan.bKeepAim = true;
		return Plan;
	}
	if (Def->bPoint)
	{
		// if (explainUnavailable(available({ t: 'support', kind }))) return; cancelAim();
		// sendCmd({ t: 'support', kind, x: g.x, z: g.z }); marker(g.x, g.z, 0xffa030)
		const FTCSupportCheck C = Check(State, Tbl, Recruit, Def->Kind);
		if (!C.bOk) { TCSPI::Refuse(Plan, C); Plan.bKeepAim = true; return Plan; }
		Aim.Cancel();
		Plan.bCancelAim = true;
		TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
		M->SetStringField(TEXT("t"), TEXT("support"));
		M->SetStringField(TEXT("kind"), Def->Kind);
		M->SetNumberField(TEXT("x"), Ground.X);
		M->SetNumberField(TEXT("z"), Ground.Y);
		Plan.Kind = ETCSupportPowerCommand::Support;
		Plan.Message = M;
		Plan.bMarker = true;
		Plan.At = Ground;
		Plan.Color = TCSPI::SupportMarker;
		Plan.PredictedDeny = PredictSupport(State, Recruit, *Def, Ground);
		return Plan;
	}
	// first click: pin the center, then the mouse rotates it
	if (!Aim.bHasCenter)
	{
		Aim.bHasCenter = true;
		Aim.Center = Ground;
		Aim.Hint = TCSPI::RotateHintText;
		Plan.Kind = ETCSupportPowerCommand::PinCenter;
		Plan.At = Ground;
		Plan.Note = Aim.Hint;
		return Plan;
	}
	// const c = aimCenter, dir = Math.hypot(g.x - c.x, g.z - c.z) > 1.5 ? Math.atan2(g.z - c.z, g.x - c.x) : defaultDir(kind, c)
	const FVector2D C0 = Aim.Center;
	const double Dir = TCSPI::Hypot(Ground.X - C0.X, Ground.Y - C0.Y) > TCSPI::TurnThreshold ? std::atan2(Ground.Y - C0.Y, Ground.X - C0.X) : DefaultDir(C0);
	// if (explainUnavailable(available({ t: 'support', kind }))) return; cancelAim();
	// sendCmd({ t: 'support', kind, x: c.x, z: c.z, dir })
	const FTCSupportCheck C = Check(State, Tbl, Recruit, Def->Kind);
	if (!C.bOk) { TCSPI::Refuse(Plan, C); Plan.bKeepAim = true; return Plan; }
	Aim.Cancel();
	Plan.bCancelAim = true;
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("support"));
	M->SetStringField(TEXT("kind"), Def->Kind);
	M->SetNumberField(TEXT("x"), C0.X);
	M->SetNumberField(TEXT("z"), C0.Y);
	M->SetNumberField(TEXT("dir"), Dir);
	Plan.Kind = ETCSupportPowerCommand::Support;
	Plan.Message = M;
	Plan.At = C0;
	Plan.Dir = Dir;
	Plan.PredictedDeny = PredictSupport(State, Recruit, *Def, C0);
	return Plan;
}

FTCSupportPreview FTCSupportPowers::Preview(FTCSupportAim& Aim, bool bHasGround, FVector2D Ground) const
{
	FTCSupportPreview P;
	if (!Aim.IsArmed()) { Aim.MeshKind.Reset(); Aim.bMeshPlaced = false; Aim.MeshAt = FVector2D::ZeroVector; Aim.MeshDir = 0; return P; }
	const FString Kind = Aim.Kind == ETCSupportAimKind::Area ? FString(TEXT("area")) : Aim.SupportKind;
	// if (aimMesh?.userData.kind !== targeting) a new mesh (at the origin, no rotation)
	if (Aim.MeshKind != Kind) { Aim.MeshKind = Kind; Aim.bMeshPlaced = false; Aim.MeshAt = FVector2D::ZeroVector; Aim.MeshDir = 0; }
	if (Aim.bHasCenter)
	{
		// the mesh sits on the centre; it turns toward the pointer once that is more than 1.5 m away
		Aim.MeshAt = Aim.Center;
		Aim.bMeshPlaced = true;
		if (bHasGround && TCSPI::Hypot(Ground.X - Aim.Center.X, Ground.Y - Aim.Center.Y) > TCSPI::TurnThreshold) Aim.MeshDir = std::atan2(Ground.Y - Aim.Center.Y, Ground.X - Aim.Center.X);
	}
	else if (bHasGround)
	{
		Aim.MeshAt = Ground;
		Aim.bMeshPlaced = true;
		Aim.MeshDir = DefaultDir(Ground);
	}
	P.bShown = true;
	P.bPlaced = Aim.bMeshPlaced;
	P.At = Aim.MeshAt;
	P.Dir = Aim.MeshDir;
	// aimShape: point kinds a ring (radius ?? blast ?? 4), 'area' a ring of UNITS.rocket.w.spread, strips len x width
	const FTCSupportDef* Def = Aim.Kind == ETCSupportAimKind::Support ? Find(Aim.SupportKind) : nullptr;
	if (Def && !Def->bPoint) { P.bStrip = true; P.Len = Def->Len; P.Width = Def->Width; }
	else P.Radius = Def ? Def->Radius : TCSPI::AreaRadius;
	return P;
}

FTCAreaButton FTCSupportPowers::AreaButton(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected) const
{
	FTCAreaButton B;
	B.Icon = TEXT("barrage");
	B.Key = TCSPI::AreaBadgeText;
	B.Tip = TCSPI::AreaTipText;
	if (!TCArmyCommands::ShowsOrdersPanel(State, Tbl, Selected)) return B;
	// drawOrders: shell = sel.some(v => UNITS[v.type].w?.salvo), over every held selected unit
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (U && IsSalvo(U->Type)) { B.bShown = true; break; }
	}
	return B;
}

TSharedPtr<FJsonObject> FTCSupportPowers::HouseFireCommand(const FTCMatchState& State, const FTCUnitTable& Tbl, const TSet<int32>& Selected, FVector2D HouseCenter, int32* OutGuns) const
{
	// client/orders.js selection: troops are the selected own units that are not structures;
	// guns = troops.filter(v => w?.shellTerrain || w?.salvo); add({ t: 'fireat', ids: guns.map(v => v.id), x: at.x, z: at.z })
	if (OutGuns) *OutGuns = 0;
	TArray<int32> Guns;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		const FTCUnitDef* D = U ? Tbl.Find(U->Type) : nullptr;
		if (!U || !D || !State.IsOwn(*U) || D->bStructure) continue;
		if (IsSalvo(U->Type) || ShellsTerrain(U->Type)) Guns.Add(Id);
	}
	if (!Guns.Num()) return nullptr;
	if (OutGuns) *OutGuns = Guns.Num();
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("fireat"));
	M->SetArrayField(TEXT("ids"), TCSPI::IdArray(Guns));
	M->SetNumberField(TEXT("x"), HouseCenter.X);
	M->SetNumberField(TEXT("z"), HouseCenter.Y);
	return M;
}

TArray<FTCStrikeMark> FTCSupportPowers::Strikes(const FTCMatchState& State) const
{
	TArray<FTCStrikeMark> Out;
	for (const FStrikeRow& S : StrikeRows)
	{
		FTCStrikeMark K;
		K.Kind = S.Kind;
		K.At = FVector2D(S.X, S.Z);
		K.Dir = S.Dir;
		K.T = S.T;
		K.Owner = S.Owner;
		K.bFoe = State.IsFoe(S.Owner);
		K.Key = FString::Printf(TEXT("%s,%s,%s"), *S.Kind, *TCSPI::JsNumber(S.X), *TCSPI::JsNumber(S.Z));
		// strikeShape: SUPPORT[kind].point ? { r: radius ?? blast ?? 4 } : SUPPORT[kind].len ? { len, width } : { r: 4 }
		if (const FTCSupportDef* D = Find(S.Kind))
		{
			K.bKnownKind = true;
			if (D->bPoint) K.Radius = D->Radius;
			else { K.bStrip = true; K.Len = D->Len; K.Width = D->Width; }
			K.bMinimap = D->Len > 0;
		}
		else K.Radius = TCSPI::FallbackStrikeRadius;
		Out.Add(MoveTemp(K));
	}
	return Out;
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCSPITest
{
	TSharedPtr<FJsonObject> Parse(const FString& Src)
	{
		TSharedPtr<FJsonObject> Obj;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Src), Obj);
		return Obj;
	}

	// a seat the lobby confirmed (or a spectator), in a match being played, teams [0, 1], a 100 x 100 cell map
	void Seat(FTCMatchState& St, bool bWatching)
	{
		St.bHasMatch = true; St.You = 0; St.LobbySeat = 0; St.bLobbySinceOpen = true; St.bSpectator = bWatching;
		St.RoomState = TEXT("play"); St.Winner = -2; St.Teams = { 0, 1 };
		St.Map.W = 100; St.Map.H = 100;
	}

	// the snapshot's unit rows into the held units (shared/sim.js UNIT_ROW: id, type, owner, x, z, rot, aim, hp, supp,
	// target, cover, cd, flags, stars, built), as FTCMatchState would hold them
	void Rows(FTCMatchState& St, const FJsonObject& Snap)
	{
		const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
		if (!Snap.TryGetArrayField(TEXT("units"), Arr) || !Arr) return;
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>& R = V->AsArray();
			FTCUnit U;
			U.Id = (int32)R[0]->AsNumber(); U.Type = R[1]->AsString(); U.Owner = (int32)R[2]->AsNumber();
			U.X = (float)R[3]->AsNumber(); U.Z = (float)R[4]->AsNumber(); U.Hp = (int32)R[7]->AsNumber();
			U.Cd = (int32)R[11]->AsNumber(); U.Flags = (int32)R[12]->AsNumber(); U.Built = (float)R[14]->AsNumber();
			St.Units.Add(U.Id, U);
		}
	}

	struct FWorld
	{
		FTCMatchState St;
		FTCRecruitment Recruit;
		FTCSupportPowers Powers;
		FWorld(const TCHAR* Snap, bool bWatching, const TCHAR* Start = TEXT("{\"t\":\"start\"}"))
		{
			Seat(St, bWatching);
			Recruit.SetUnitTableReference(FTCRecruitment::ReferencePin);
			const TSharedPtr<FJsonObject> S0 = Parse(Start);
			Recruit.ApplyStart(*S0);
			Powers.ApplyStart(*S0);
			if (!Snap) return;
			const TSharedPtr<FJsonObject> S = Parse(Snap);
			Rows(St, *S);
			St.Snapshots = 1;
			Recruit.ApplySnapshot(*S);
			Powers.ApplySnapshot(*S);
		}
	};

	FString Str(const TSharedPtr<FJsonObject>& M, const TCHAR* F) { FString S; if (M.IsValid()) M->TryGetStringField(F, S); return S; }
	double Number(const TSharedPtr<FJsonObject>& M, const TCHAR* F) { double D = std::numeric_limits<double>::quiet_NaN(); if (M.IsValid()) M->TryGetNumberField(F, D); return D; }
	TArray<int32> Ids(const TSharedPtr<FJsonObject>& M)
	{
		TArray<int32> Out;
		const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
		if (M.IsValid() && M->TryGetArrayField(TEXT("ids"), Arr) && Arr) for (const TSharedPtr<FJsonValue>& V : *Arr) Out.Add((int32)V->AsNumber());
		Out.Sort();
		return Out;
	}
}

// Run (after a build): -ExecCmds="Automation RunTests ThreeCrossroads.SupportPowers; Quit".
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSupportPowersReferenceTest, "ThreeCrossroads.SupportPowers.Reference", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCSupportPowersReferenceTest::RunTest(const FString& Parameters)
{
	FTCUnitTable Tbl;
	FString TableError;
	if (!TestTrue(TEXT("unit table loads (Content/Data/units.json)"), Tbl.Load(TableError))) return false;
	TestEqual(TEXT("unit table is at the support pin"), Tbl.Reference, FString(FTCSupportPowers::ReferencePin));
	// Generated from the reference at e4e579a: client/availability.js availability({ t: 'support', kind }) for seat 0
	// (teams [0, 1]) and client/hud.js drawEcon's value line, each run on the snapshot shown.
	struct FCase { const TCHAR* Name; const TCHAR* Snap; const TCHAR* Kind; bool bWatching, bOk; const TCHAR* Reason; const TCHAR* Text; };
	const FCase Cases[] = {
		{ TEXT("conquest artillery"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":200,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[1,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"strikes\":[]}"), TEXT("artillery"), false, true, TEXT(""), TEXT("150 MP") },
		{ TEXT("conquest bombing short"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":200,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[1,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"strikes\":[]}"), TEXT("bombing"), false, false, TEXT("Needs 250 MP"), TEXT("250 MP") },
		{ TEXT("conquest recon"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":200,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[1,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"strikes\":[]}"), TEXT("recon"), false, true, TEXT(""), TEXT("60 MP") },
		{ TEXT("cooldown"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":999,\"sup\":{\"recon\":0,\"artillery\":12,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[],\"strikes\":[]}"), TEXT("artillery"), false, false, TEXT("Cooldown 12 s"), TEXT("12s") },
		{ TEXT("exact price"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":150,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[],\"strikes\":[]}"), TEXT("artillery"), false, true, TEXT(""), TEXT("150 MP") },
		{ TEXT("classic artillery short of mun"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":100,\"mun\":59,\"fuel\":0,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[],\"strikes\":[]}"), TEXT("artillery"), false, false, TEXT("Needs 60 munitions"), TEXT("60 Mun") },
		{ TEXT("classic smoke"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":100,\"mun\":59,\"fuel\":0,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[],\"strikes\":[]}"), TEXT("smoke"), false, true, TEXT(""), TEXT("20 Mun") },
		{ TEXT("classic negative mp"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":-1,\"mun\":500,\"fuel\":0,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[],\"strikes\":[]}"), TEXT("cover"), false, false, TEXT("Needs 0 MP"), TEXT("40 Mun") },
		{ TEXT("para at the limit"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":999,\"army\":{\"pop\":0.4},\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[1,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1],[3,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1],[4,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[5,\"rifle\",1,0,0,0,0,100,0,0,0,0,0,0,1]],\"strikes\":[]}"), TEXT("para"), false, false, TEXT("Army at its limit (3/5)"), TEXT("260 MP") },
		{ TEXT("para with room"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":999,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[1,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1],[3,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"strikes\":[]}"), TEXT("para"), false, true, TEXT(""), TEXT("260 MP") },
		{ TEXT("world para no own airfield"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"world\"},\"mp\":999,\"mun\":999,\"fuel\":0,\"world\":{\"cap\":40},\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[7,\"airfield\",1,0,0,0,0,100,0,0,0,0,0,0,1]],\"strikes\":[]}"), TEXT("para"), false, false, TEXT("Paratroopers fly from one of your finished Airfields, at most 300 m away"), TEXT("90 Mun") },
		{ TEXT("world para airfield site"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"world\"},\"mp\":999,\"mun\":999,\"fuel\":0,\"world\":{\"cap\":40},\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[7,\"airfield\",0,0,0,0,0,100,0,0,0,0,0,0,0.9999999999999842]],\"strikes\":[]}"), TEXT("para"), false, false, TEXT("Paratroopers fly from one of your finished Airfields, at most 300 m away"), TEXT("90 Mun") },
		{ TEXT("world para airfield"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"world\"},\"mp\":999,\"mun\":999,\"fuel\":0,\"world\":{\"cap\":40},\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[7,\"airfield\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"strikes\":[]}"), TEXT("para"), false, true, TEXT(""), TEXT("90 Mun") },
		{ TEXT("world dive (no airfield needed)"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"world\"},\"mp\":999,\"mun\":999,\"fuel\":0,\"world\":{\"cap\":40},\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[7,\"airfield\",1,0,0,0,0,100,0,0,0,0,0,0,1]],\"strikes\":[]}"), TEXT("dive"), false, true, TEXT(""), TEXT("70 Mun") },
		{ TEXT("out"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":999,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[true,false],\"units\":[],\"strikes\":[]}"), TEXT("recon"), false, false, TEXT("You are spectating"), TEXT("60 MP") },
		{ TEXT("watching"), TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":200,\"sup\":{\"recon\":0,\"artillery\":0,\"strafe\":0,\"smoke\":0,\"bombing\":0,\"dive\":0,\"para\":0,\"cover\":0},\"out\":[false,false],\"units\":[[1,\"rifle\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"strikes\":[]}"), TEXT("recon"), true, false, TEXT("You are spectating"), TEXT("60 MP") },
		{ TEXT("no snapshot"), nullptr, TEXT("recon"), false, false, TEXT("Waiting for the army"), TEXT("") },
	};
	for (const FCase& C : Cases)
	{
		TCSPITest::FWorld W(C.Snap, C.bWatching);
		const FTCSupportCheck Got = W.Powers.Check(W.St, Tbl, W.Recruit, C.Kind);
		TestEqual(FString(C.Name) + TEXT(": ok"), Got.bOk, C.bOk);
		TestEqual(FString(C.Name) + TEXT(": reason"), Got.Reason, FString(C.Reason));
		TestFalse(FString(C.Name) + TEXT(": a browser sentence"), Got.bNative);
		const TArray<FTCSupportButton> Bar = W.Powers.Buttons(W.St, Tbl, W.Recruit);
		const FTCSupportButton* B = Bar.FindByPredicate([&C](const FTCSupportButton& X) { return X.Kind == C.Kind; });
		TestTrue(FString(C.Name) + TEXT(": button"), B != nullptr);
		if (B)
		{
			TestEqual(FString(C.Name) + TEXT(": value line"), B->Text, FString(C.Text));
			TestEqual(FString(C.Name) + TEXT(": button availability"), B->Check.Reason, FString(C.Reason));
		}
	}
	return true;
}

// The clicks, aims, keys, previews, strikes and predictions (client/main.js and client/keys.js, which need a browser to
// run), and this client's own refusals.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSupportPowersNativeTest, "ThreeCrossroads.SupportPowers.Native", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCSupportPowersNativeTest::RunTest(const FString& Parameters)
{
	FTCUnitTable Tbl;
	FString TableError;
	if (!TestTrue(TEXT("unit table loads (Content/Data/units.json)"), Tbl.Load(TableError))) return false;

	// the table and the chords
	TestEqual(TEXT("eight support kinds"), FTCSupportPowers::All().Num(), 8);
	TestEqual(TEXT("support bar order starts with recon"), FTCSupportPowers::All()[0].Kind, FString(TEXT("recon")));
	TestTrue(TEXT("mortar fires salvos"), FTCSupportPowers::IsSalvo(TEXT("mortar")) && !FTCSupportPowers::IsSalvo(TEXT("tank")));
	TestTrue(TEXT("tanks shell terrain"), FTCSupportPowers::ShellsTerrain(TEXT("tank")) && !FTCSupportPowers::ShellsTerrain(TEXT("mortar")));
	{
		const FTCSupportKeyMatch Z = FTCSupportPowers::KeyAction(EKeys::Z, false, false, false);
		TestTrue(TEXT("Z: recon, outranked by card 11"), Z.Action == ETCSupportPowerKey::Support && Z.Kind == TEXT("recon") && Z.Card == 11);
		const FTCSupportKeyMatch N = FTCSupportPowers::KeyAction(EKeys::N, false, false, false);
		TestTrue(TEXT("N: bombing, no card"), N.Action == ETCSupportPowerKey::Support && N.Kind == TEXT("bombing") && N.Card == 0);
		const FTCSupportKeyMatch SB = FTCSupportPowers::KeyAction(EKeys::B, true, false, false);
		TestTrue(TEXT("Shift+B: area, outranked by cardMany 15"), SB.Action == ETCSupportPowerKey::Area && SB.Card == 15);
		TestTrue(TEXT("Shift+Z: nothing"), FTCSupportPowers::KeyAction(EKeys::Z, true, false, false).Action == ETCSupportPowerKey::None);
		TestTrue(TEXT("Ctrl+B: nothing"), FTCSupportPowers::KeyAction(EKeys::B, false, true, false).Action == ETCSupportPowerKey::None);
		for (const FTCSupportDef& D : FTCSupportPowers::All())
		{
			const FKey K(*D.KeyLabel);
			TestEqual(FString(TEXT("key matches the label: ")) + D.Kind, FTCSupportPowers::KeyAction(K, false, false, false).Kind, D.Kind);
		}
	}

	// a Conquest snapshot with money, spawn (0, 0): seat 0 has a mortar (1), a tank (2), a rifle squad (3), an HQ (4)
	const TCHAR* Snap = TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":999,\"sup\":{\"artillery\":0,\"dive\":0},\"out\":[false,false],")
		TEXT("\"units\":[[1,\"mortar\",0,10,10,0,0,100,0,0,0,0,0,0,1],[2,\"tank\",0,12,10,0,0,100,0,0,0,0,0,0,1],[3,\"rifle\",0,14,10,0,0,100,0,0,0,0,0,0,1],[4,\"hq\",0,20,20,0,0,100,0,0,0,0,0,0,1]],")
		TEXT("\"strikes\":[[\"artillery\",10,20,0.5,3,1],[\"dive\",5,5,0,0,0]]}");
	const TCHAR* Start = TEXT("{\"t\":\"start\",\"spawn\":{\"x\":0,\"z\":0}}");
	{
		// a strip kind: arm, pin the centre, then a click 10 m south sends dir pi/2 from the centre, with no marker
		TCSPITest::FWorld W(Snap, false, Start);
		FTCSupportAim Aim;
		const FTCSupportPowerPlan A = W.Powers.ArmSupport(W.St, Tbl, W.Recruit, TEXT("artillery"), Aim);
		TestTrue(TEXT("artillery arms"), A.Kind == ETCSupportPowerCommand::ArmAim && A.bArmAim && Aim.Kind == ETCSupportAimKind::Support && Aim.Hint == FTCSupportPowers::CenterHint());
		const FTCSupportPowerPlan NoGround = W.Powers.AimClick(W.St, Tbl, W.Recruit, {}, Aim, false, FVector2D::ZeroVector, false);
		TestTrue(TEXT("no ground: 'blocked', aim kept"), NoGround.bKeepAim && NoGround.bFeedback && NoGround.Note == TCOrders::DenySentence(TEXT("blocked"), FString()) && Aim.IsArmed());
		const FTCSupportPowerPlan P = W.Powers.AimClick(W.St, Tbl, W.Recruit, {}, Aim, true, FVector2D(30, 40), false);
		TestTrue(TEXT("first click pins the centre"), P.Kind == ETCSupportPowerCommand::PinCenter && !P.Message.IsValid() && Aim.bHasCenter && Aim.Center == FVector2D(30, 40) && Aim.Hint == FTCSupportPowers::RotateHint());
		const FTCSupportPreview Turn = W.Powers.Preview(Aim, true, FVector2D(30, 50));
		TestTrue(TEXT("the strip turns toward the pointer"), Turn.bStrip && Turn.Len == 24 && Turn.Width == 10 && Turn.At == FVector2D(30, 40) && FMath::IsNearlyEqual(Turn.Dir, UE_DOUBLE_HALF_PI));
		const FTCSupportPreview Still = W.Powers.Preview(Aim, true, FVector2D(30.5, 40.5));
		TestTrue(TEXT("within 1.5 m it keeps its last turn"), FMath::IsNearlyEqual(Still.Dir, UE_DOUBLE_HALF_PI));
		const FTCSupportPowerPlan S = W.Powers.AimClick(W.St, Tbl, W.Recruit, {}, Aim, true, FVector2D(30, 50), true);
		TestTrue(TEXT("second click sends"), S.Kind == ETCSupportPowerCommand::Support && S.bCancelAim && !Aim.IsArmed() && !S.bMarker);
		TestEqual(TEXT("support: t"), TCSPITest::Str(S.Message, TEXT("t")), FString(TEXT("support")));
		TestEqual(TEXT("support: kind"), TCSPITest::Str(S.Message, TEXT("kind")), FString(TEXT("artillery")));
		TestEqual(TEXT("support: x is the centre"), TCSPITest::Number(S.Message, TEXT("x")), 30.0);
		TestEqual(TEXT("support: z is the centre"), TCSPITest::Number(S.Message, TEXT("z")), 40.0);
		TestTrue(TEXT("support: dir"), FMath::IsNearlyEqual(TCSPITest::Number(S.Message, TEXT("dir")), UE_DOUBLE_HALF_PI));
		TestFalse(TEXT("support: no queue"), S.Message.IsValid() && S.Message->HasField(TEXT("queue")));
		TestTrue(TEXT("strip support: no marker in the order plan"), S.ToOrderPlan().At == FVector2D(-1, -1));
		TestTrue(TEXT("acceptance expected"), S.PredictedDeny.IsEmpty());

		// within 1.5 m of the centre the dir comes from home (the start's spawn)
		W.Powers.ArmSupport(W.St, Tbl, W.Recruit, TEXT("smoke"), Aim);
		W.Powers.AimClick(W.St, Tbl, W.Recruit, {}, Aim, true, FVector2D(30, 40), false);
		const FTCSupportPowerPlan H = W.Powers.AimClick(W.St, Tbl, W.Recruit, {}, Aim, true, FVector2D(31, 40), false);
		TestTrue(TEXT("dir from home"), FMath::IsNearlyEqual(TCSPITest::Number(H.Message, TEXT("dir")), std::atan2(40.0, 30.0)));
	}
	{
		// a point kind: one click at the spot, the orange marker; a cooling kind keeps the aim with its sentence
		TCSPITest::FWorld W(Snap, false, Start);
		FTCSupportAim Aim;
		W.Powers.ArmSupport(W.St, Tbl, W.Recruit, TEXT("dive"), Aim);
		const FTCSupportPreview Ring = W.Powers.Preview(Aim, true, FVector2D(5, 6));
		TestTrue(TEXT("dive previews its blast ring"), Ring.bShown && !Ring.bStrip && Ring.Radius == 5 && Ring.At == FVector2D(5, 6));
		const FTCSupportPowerPlan D = W.Powers.AimClick(W.St, Tbl, W.Recruit, {}, Aim, true, FVector2D(5, 6), false);
		TestTrue(TEXT("dive sends at the click"), D.Kind == ETCSupportPowerCommand::Support && D.bMarker && D.At == FVector2D(5, 6) && D.Color == FColor(0xff, 0xa0, 0x30) && !Aim.IsArmed());
		TestTrue(TEXT("dive: no dir"), D.Message.IsValid() && !D.Message->HasField(TEXT("dir")) && TCSPITest::Number(D.Message, TEXT("x")) == 5.0);
		const TCHAR* Cooling = TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":999,\"sup\":{\"dive\":30},\"out\":[false,false],\"units\":[],\"strikes\":[]}");
		W.Powers.ApplySnapshot(*TCSPITest::Parse(Cooling));
		const FTCSupportPowerPlan Refused = W.Powers.ArmSupport(W.St, Tbl, W.Recruit, TEXT("dive"), Aim);
		TestTrue(TEXT("a cooling kind does not arm"), Refused.Kind == ETCSupportPowerCommand::None && Refused.bFeedback && Refused.Note == TEXT("Cooldown 30 s") && !Aim.IsArmed());
	}
	{
		// area fire: arming needs a selected own salvo unit; the click sends a fireat, Shift queues and stays armed
		TCSPITest::FWorld W(Snap, false, Start);
		FTCSupportAim Aim;
		const FTCSupportPowerPlan NoShell = W.Powers.ArmArea(W.St, Tbl, TSet<int32>{ 3 }, Aim);
		TestTrue(TEXT("rifles only: the hint, no aim"), NoShell.Kind == ETCSupportPowerCommand::None && NoShell.Note == FTCSupportPowers::NoShellersHint() && !NoShell.bFeedback && !Aim.IsArmed());
		const FTCSupportPowerPlan A = W.Powers.ArmArea(W.St, Tbl, TSet<int32>{ 1, 3 }, Aim);
		TestTrue(TEXT("a mortar arms the area aim"), A.bArmAim && Aim.Kind == ETCSupportAimKind::Area && Aim.Hint == FTCSupportPowers::AreaHint());
		TestEqual(TEXT("area preview ring"), W.Powers.Preview(Aim, true, FVector2D(50, 50)).Radius, 6.0);
		const FTCSupportPowerPlan Q = W.Powers.AimClick(W.St, Tbl, W.Recruit, TSet<int32>{ 1, 2, 3 }, Aim, true, FVector2D(50, 60), true);
		TestTrue(TEXT("Shift queues and keeps the aim"), Q.Kind == ETCSupportPowerCommand::FireAt && Aim.IsArmed() && !Q.bCancelAim && Q.Message.IsValid() && Q.Message->GetBoolField(TEXT("queue")));
		TestTrue(TEXT("only the mortar shells"), TCSPITest::Ids(Q.Message) == TArray<int32>{ 1 } && Q.Units == 1);
		TestTrue(TEXT("the red marker"), Q.bMarker && Q.Color == FColor(0xff, 0x40, 0x30) && Q.At == FVector2D(50, 60));
		const FTCSupportPowerPlan F = W.Powers.AimClick(W.St, Tbl, W.Recruit, TSet<int32>{ 1 }, Aim, true, FVector2D(50, 60), false);
		TestTrue(TEXT("a plain click ends the aim"), F.Kind == ETCSupportPowerCommand::FireAt && F.bCancelAim && !Aim.IsArmed() && !F.Message->GetBoolField(TEXT("queue")));
		TestEqual(TEXT("fireat: t"), TCSPITest::Str(F.Message, TEXT("t")), FString(TEXT("fireat")));
		TestEqual(TEXT("fireat: x"), TCSPITest::Number(F.Message, TEXT("x")), 50.0);
		W.Powers.ArmArea(W.St, Tbl, TSet<int32>{ 1 }, Aim);
		const FTCSupportPowerPlan Empty = W.Powers.AimClick(W.St, Tbl, W.Recruit, TSet<int32>{ 3 }, Aim, true, FVector2D(50, 60), false);
		TestTrue(TEXT("no shellers left: nothing sent, the aim still ends"), Empty.Kind == ETCSupportPowerCommand::None && !Empty.Message.IsValid() && !Aim.IsArmed());
		// shared/sim.js num clamps onto [0, w * CELL]: -5 lands in cell 0, but 200 (the far edge of 100 cells) is no cell
		W.Powers.ArmArea(W.St, Tbl, TSet<int32>{ 1 }, Aim);
		const FTCSupportPowerPlan Clamped = W.Powers.AimClick(W.St, Tbl, W.Recruit, TSet<int32>{ 1 }, Aim, true, FVector2D(-5, 60), false);
		TestTrue(TEXT("below zero is clamped onto the map"), Clamped.Kind == ETCSupportPowerCommand::FireAt && Clamped.PredictedDeny.IsEmpty());
		W.Powers.ArmArea(W.St, Tbl, TSet<int32>{ 1 }, Aim);
		const FTCSupportPowerPlan Off = W.Powers.AimClick(W.St, Tbl, W.Recruit, TSet<int32>{ 1 }, Aim, true, FVector2D(200, 60), false);
		TestEqual(TEXT("the far edge: 'blocked' expected"), Off.PredictedDeny, FString(TEXT("blocked")));

		TestTrue(TEXT("Shell area button with a mortar"), W.Powers.AreaButton(W.St, Tbl, TSet<int32>{ 1, 3 }).bShown);
		TestFalse(TEXT("no Shell area button without one"), W.Powers.AreaButton(W.St, Tbl, TSet<int32>{ 2, 3 }).bShown);
		TestFalse(TEXT("no Orders panel for a building"), W.Powers.AreaButton(W.St, Tbl, TSet<int32>{ 4 }).bShown);

		// a house: the tank (shellTerrain) and the mortar (salvo) shell it; the rifles and the HQ do not
		int32 Guns = 0;
		const TSharedPtr<FJsonObject> House = W.Powers.HouseFireCommand(W.St, Tbl, TSet<int32>{ 1, 2, 3, 4 }, FVector2D(41, 43), &Guns);
		TestTrue(TEXT("house: tank and mortar"), Guns == 2 && TCSPITest::Ids(House) == TArray<int32>({ 1, 2 }) && TCSPITest::Number(House, TEXT("z")) == 43.0 && !House->HasField(TEXT("queue")));
		TestFalse(TEXT("house: rifles alone shell nothing"), W.Powers.HouseFireCommand(W.St, Tbl, TSet<int32>{ 3 }, FVector2D(41, 43)).IsValid());

		// the public warnings: a foe's barrage strip, this seat's dive ring
		const TArray<FTCStrikeMark> Marks = W.Powers.Strikes(W.St);
		TestEqual(TEXT("two strikes"), Marks.Num(), 2);
		if (Marks.Num() == 2)
		{
			TestTrue(TEXT("foe's artillery strip"), Marks[0].bFoe && Marks[0].bStrip && Marks[0].Len == 24 && Marks[0].Width == 10 && Marks[0].bMinimap && Marks[0].Dir == 0.5 && Marks[0].T == 3);
			TestEqual(TEXT("strike key"), Marks[0].Key, FString(TEXT("artillery,10,20")));
			TestTrue(TEXT("own dive ring"), !Marks[1].bFoe && !Marks[1].bStrip && Marks[1].Radius == 5 && !Marks[1].bMinimap);
		}
	}
	{
		// World paratroopers: a finished Airfield 400 m away passes availability; the server wants one within 300 m
		const TCHAR* World = TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"world\"},\"mp\":999,\"mun\":999,\"world\":{\"cap\":40,\"home\":[0,0]},\"sup\":{\"para\":0},\"out\":[false,false],")
			TEXT("\"units\":[[7,\"airfield\",0,400,0,0,0,500,0,0,0,0,0,0,1]],\"strikes\":[]}");
		TCSPITest::FWorld W(World, false, Start);
		FTCSupportAim Aim;
		TestTrue(TEXT("para arms"), W.Powers.ArmSupport(W.St, Tbl, W.Recruit, TEXT("para"), Aim).bArmAim);
		const FTCSupportPowerPlan P = W.Powers.AimClick(W.St, Tbl, W.Recruit, {}, Aim, true, FVector2D(0, 10), false);
		TestTrue(TEXT("para sends"), P.Kind == ETCSupportPowerCommand::Support);
		TestEqual(TEXT("too far from the Airfield: 'airfield' expected"), P.PredictedDeny, FString(TEXT("airfield")));
		TestEqual(TEXT("World costs Munitions"), W.Powers.Cost(TEXT("para")), 90);
	}
	{
		// this client's own refusals: another pin, an unconfirmed seat
		TCSPITest::FWorld W(Snap, false, Start);
		FTCUnitTable Other = Tbl;
		Other.Reference = TEXT("0000000");
		const FTCSupportCheck Pin = W.Powers.Check(W.St, Other, W.Recruit, TEXT("recon"));
		TestTrue(TEXT("another pin refuses natively"), !Pin.bOk && Pin.bNative);
		FTCSupportAim Aim;
		TestFalse(TEXT("another pin arms no area aim"), W.Powers.ArmArea(W.St, Other, TSet<int32>{ 1 }, Aim).bArmAim);
		W.St.bLobbySinceOpen = false;
		const FTCSupportCheck Seat = W.Powers.Check(W.St, Tbl, W.Recruit, TEXT("recon"));
		TestTrue(TEXT("unconfirmed seat refuses natively"), !Seat.bOk && Seat.bNative);
		const FTCSupportPowerPlan Area = W.Powers.ArmArea(W.St, Tbl, TSet<int32>{ 1 }, Aim);
		TestTrue(TEXT("unconfirmed seat arms no area aim"), !Area.bArmAim && Area.bNative && !Aim.IsArmed());
		// the seat goes out: once, on the snapshot that first shows it
		TestFalse(TEXT("not out yet"), W.Powers.WentOut(0));
		const TCHAR* Out = TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":0,\"out\":[true,false],\"units\":[],\"strikes\":[]}");
		W.Powers.ApplySnapshot(*TCSPITest::Parse(Out));
		TestTrue(TEXT("went out"), W.Powers.WentOut(0) && !W.Powers.WentOut(1));
		W.Powers.ApplySnapshot(*TCSPITest::Parse(Out));
		TestFalse(TEXT("only once"), W.Powers.WentOut(0));
	}
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
