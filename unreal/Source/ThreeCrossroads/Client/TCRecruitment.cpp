#include "Client/TCRecruitment.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Algo/StableSort.h"
#include <limits>

const TCHAR* const FTCRecruitment::ReferencePin = TEXT("e4e579a");

const TCHAR* LexToString(ETCCardKind Kind)
{
	switch (Kind)
	{
	case ETCCardKind::Recruit: return TEXT("recruit");
	case ETCCardKind::Building: return TEXT("building");
	case ETCCardKind::Site: return TEXT("site");
	case ETCCardKind::NotPorted: return TEXT("not ported");
	default: return TEXT("hidden");
	}
}

namespace
{
	constexpr int32 AnyFaction = FTCRecruitDef::AnyFaction;

	// shared/sim.js UNITS, the recruit fields, in UNIT_TYPES order: type, name, cost, classicCost (-1: none), fuel, pop,
	// max (-1: none), faction, classic, naval, structure, building, air, infantry, train (0: none), tierOf, makes.
	// Generated from the reference at FTCRecruitment::ReferencePin (read-only) by importing shared/sim.js UNITS and
	// shared/tech.js tierOf. When the pin moves, regenerate the rows with unreal/Tools/export-recruit-table.mjs rather than edit them by hand.
	struct FRow
	{
		const TCHAR* Type; const TCHAR* Name;
		int32 Cost, ClassicCost, Fuel; double Pop; int32 Max, Faction;
		bool bClassic, bNaval, bStructure, bBuilding, bAir, bInfantry;
		int32 Train, Tier; const TCHAR* Makes;
	};
	const FRow Rows[] = {
	{ TEXT("rifle"), TEXT("Rifle Squad"), 100, -1, 0, 1, -1, AnyFaction, false, false, false, false, false, true, 15, 1, TEXT("") },
	{ TEXT("mg"), TEXT("MG Team"), 150, -1, 0, 1, -1, AnyFaction, false, false, false, false, false, true, 18, 1, TEXT("") },
	{ TEXT("at"), TEXT("AT Gun"), 200, -1, 0, 1, -1, AnyFaction, false, false, false, false, false, true, 22, 2, TEXT("") },
	{ TEXT("tank"), TEXT("Light Tank"), 300, 200, 60, 1, -1, AnyFaction, false, false, false, false, false, false, 35, 2, TEXT("") },
	{ TEXT("rocket"), TEXT("Rocket Launcher"), 250, 170, 50, 1, -1, AnyFaction, false, false, false, false, false, false, 30, 3, TEXT("") },
	{ TEXT("flak"), TEXT("Flak Gun"), 200, -1, 0, 1, -1, AnyFaction, false, false, false, false, false, true, 20, 2, TEXT("") },
	{ TEXT("fighter"), TEXT("Fighter"), 280, 200, 40, 1, -1, AnyFaction, false, false, false, false, true, false, 30, 3, TEXT("") },
	{ TEXT("attacker"), TEXT("Ground-attack Plane"), 340, 240, 70, 1, -1, AnyFaction, false, false, false, false, true, false, 35, 3, TEXT("") },
	{ TEXT("flaktrack"), TEXT("Mobile Flak"), 260, 200, 30, 1, -1, AnyFaction, false, false, false, false, false, false, 30, 2, TEXT("") },
	{ TEXT("mortar"), TEXT("Mortar Team"), 180, -1, 0, 1, -1, AnyFaction, false, false, false, false, false, true, 20, 2, TEXT("") },
	{ TEXT("sniper"), TEXT("Sniper"), 160, -1, 0, 1, -1, AnyFaction, false, false, false, false, false, true, 20, 1, TEXT("") },
	{ TEXT("armoredcar"), TEXT("Armored Car"), 220, 160, 25, 1, -1, AnyFaction, false, false, false, false, false, false, 25, 2, TEXT("") },
	{ TEXT("medium"), TEXT("Medium Tank"), 380, 260, 90, 1, -1, AnyFaction, false, false, false, false, false, false, 40, 3, TEXT("") },
	{ TEXT("halftrack"), TEXT("Halftrack"), 180, 140, 20, 1, -1, AnyFaction, false, false, false, false, false, false, 22, 2, TEXT("") },
	{ TEXT("lorry"), TEXT("Troop Truck"), 110, 80, 15, 1, -1, AnyFaction, false, false, false, false, false, false, 15, 1, TEXT("") },
	{ TEXT("train"), TEXT("Train"), 0, -1, 0, 0, -1, -1, true, false, false, false, false, false, 0, 1, TEXT("") },
	{ TEXT("lcvp"), TEXT("Landing Craft"), 140, 120, 15, 1, -1, AnyFaction, false, true, false, false, false, false, 20, 2, TEXT("") },
	{ TEXT("gunboat"), TEXT("Gunboat"), 220, 170, 40, 1, -1, AnyFaction, false, true, false, false, false, false, 25, 2, TEXT("") },
	{ TEXT("destroyer"), TEXT("Destroyer"), 700, 520, 180, 1, 2, AnyFaction, false, true, false, false, false, false, 75, 3, TEXT("") },
	{ TEXT("medic"), TEXT("Medic Team"), 120, -1, 0, 1, -1, AnyFaction, false, false, false, false, false, true, 15, 1, TEXT("") },
	{ TEXT("tankdestroyer"), TEXT("Tank Destroyer"), 320, 220, 80, 1, -1, AnyFaction, false, false, false, false, false, false, 40, 3, TEXT("") },
	{ TEXT("howitzer"), TEXT("Field Howitzer"), 300, -1, 0, 1, -1, AnyFaction, false, false, false, false, false, true, 30, 3, TEXT("") },
	{ TEXT("flamer"), TEXT("Flamethrower Squad"), 180, -1, 0, 1, -1, AnyFaction, false, false, false, false, false, true, 20, 1, TEXT("") },
	{ TEXT("bomber"), TEXT("Bomber"), 420, 300, 100, 1, -1, AnyFaction, false, false, false, false, true, false, 45, 3, TEXT("") },
	{ TEXT("ranger"), TEXT("Ranger Squad"), 200, -1, 0, 1, -1, 0, false, false, false, false, false, true, 20, 3, TEXT("") },
	{ TEXT("tiger"), TEXT("Tiger"), 560, 420, 150, 1, 1, 1, false, false, false, false, false, false, 50, 3, TEXT("") },
	{ TEXT("conscript"), TEXT("Conscripts"), 80, -1, 0, 0.75, -1, 2, false, false, false, false, false, true, 12, 1, TEXT("") },
	{ TEXT("churchill"), TEXT("Churchill"), 480, 360, 130, 1, 1, 3, false, false, false, false, false, false, 50, 3, TEXT("") },
	{ TEXT("commando"), TEXT("Commandos"), 190, -1, 0, 1, -1, 3, false, false, false, false, false, true, 22, 3, TEXT("") },
	{ TEXT("bunker"), TEXT("Command Bunker"), 0, -1, 0, 1, -1, -1, false, false, true, false, false, false, 0, 1, TEXT("") },
	{ TEXT("kaiju"), TEXT("Kaiju"), 1500, -1, 0, 1, -1, -1, false, false, false, false, false, false, 0, 1, TEXT("") },
	{ TEXT("engineer"), TEXT("Engineer Squad"), 60, -1, 0, 1, -1, AnyFaction, true, false, false, false, false, true, 12, 1, TEXT("") },
	{ TEXT("hq"), TEXT("HQ"), 200, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 1, TEXT("engineer,rifle") },
	{ TEXT("depot"), TEXT("Supply Depot"), 60, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 1, TEXT("") },
	{ TEXT("barracks"), TEXT("Barracks"), 150, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 1, TEXT("mg,mortar,sniper,medic,flak,flamer,ranger,conscript,commando") },
	{ TEXT("motorpool"), TEXT("Motor Pool"), 200, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 2, TEXT("at,howitzer,halftrack,lorry,armoredcar,flaktrack,tank,medium,tankdestroyer,rocket,tiger,churchill") },
	{ TEXT("airfield"), TEXT("Airfield"), 250, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 3, TEXT("fighter,attacker,bomber") },
	{ TEXT("shipyard"), TEXT("Shipyard"), 150, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 2, TEXT("lcvp,gunboat,destroyer") },
	{ TEXT("flakpos"), TEXT("Flak Emplacement"), 100, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 1, TEXT("") },
	{ TEXT("worldbase"), TEXT("Regional Military Base"), 0, -1, 0, 1, -1, -1, true, false, true, true, false, false, 0, 1, TEXT("") },
	{ TEXT("truck"), TEXT("Supply Truck"), 0, -1, 0, 0, -1, -1, true, false, false, false, false, false, 0, 1, TEXT("") },
	{ TEXT("armory"), TEXT("Armory"), 150, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 2, TEXT("") },
	{ TEXT("pillbox"), TEXT("Pillbox"), 200, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 2, TEXT("") },
	{ TEXT("tower"), TEXT("Scout Tower"), 120, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 1, TEXT("") },
	{ TEXT("wall"), TEXT("Concrete Wall"), 30, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 1, TEXT("") },
	{ TEXT("gate"), TEXT("Gate"), 50, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 1, TEXT("") },
	{ TEXT("supplycache"), TEXT("Supply Cache"), 60, -1, 0, 1, -1, -1, false, false, true, true, false, false, 0, 1, TEXT("") },
	};

	// shared/sim.js CFG.popCap and CFG.classic.popCap: the army limit before army size (snapshot army.pop) scales it.
	constexpr double SkirmishPopCap = 12, ClassicPopCap = 24;
	// client/hud.js: the card groups, the support weapons group, and the order of the cards (types not listed go last,
	// in table order).
	const TCHAR* const GroupNames[] = { TEXT("Infantry"), TEXT("Support weapons"), TEXT("Vehicles"), TEXT("Aircraft"), TEXT("Naval") };
	const TCHAR* const SupportWeapons[] = { TEXT("mg"), TEXT("mortar"), TEXT("at"), TEXT("flak"), TEXT("howitzer") };
	const TCHAR* const CardOrder[] = { TEXT("rifle"), TEXT("conscript"), TEXT("ranger"), TEXT("commando"), TEXT("flamer"), TEXT("sniper"), TEXT("medic"),
		TEXT("engineer"), TEXT("mg"), TEXT("mortar"), TEXT("at"), TEXT("howitzer"), TEXT("flak"), TEXT("halftrack"), TEXT("lorry"), TEXT("armoredcar"),
		TEXT("flaktrack"), TEXT("tank"), TEXT("medium"), TEXT("tankdestroyer"), TEXT("tiger"), TEXT("churchill"), TEXT("rocket"), TEXT("lcvp"),
		TEXT("gunboat"), TEXT("destroyer"), TEXT("fighter"), TEXT("attacker"), TEXT("bomber") };
	// shared/tech.js TIER_NAMES
	const TCHAR* const TierNames[] = { TEXT(""), TEXT("Platoon HQ"), TEXT("Company HQ"), TEXT("Battalion HQ") };
	// client/hud.js drawQueue and availability: a Classic building trains five at most
	constexpr int32 QueuePlaces = 5;

	// Generated from client/unit-roles.js. Regenerate with unreal/Tools/export-hud-roles.mjs (lead tool patch).
	// BEGIN GENERATED SOURCE ROLES e4e579a
	struct FRole { const TCHAR* Type; const TCHAR* Text; };
	const FRole SourceRoles[] = {
		{ TEXT("rifle"), TEXT("Captures, all-round") },
		{ TEXT("mg"), TEXT("Pins infantry, sets up") },
		{ TEXT("at"), TEXT("Kills tanks, sets up") },
		{ TEXT("tank"), TEXT("Kills infantry, weak rear") },
		{ TEXT("rocket"), TEXT("Rocket salvos, breaks garrisons") },
		{ TEXT("flak"), TEXT("Shoots down enemy air support") },
		{ TEXT("fighter"), TEXT("Clears the sky, escorts") },
		{ TEXT("attacker"), TEXT("Rockets and bombs on ground targets") },
		{ TEXT("flaktrack"), TEXT("Flak that keeps up with the army") },
		{ TEXT("mortar"), TEXT("Arcing fire, out-ranges MGs") },
		{ TEXT("sniper"), TEXT("One shot, one kill; hides when still") },
		{ TEXT("armoredcar"), TEXT("Fast scout and raider") },
		{ TEXT("medium"), TEXT("Mainline tank") },
		{ TEXT("halftrack"), TEXT("Carries one squad; infantry reinforce beside it") },
		{ TEXT("lorry"), TEXT("Carries three squads fast; unarmed, thin-skinned") },
		{ TEXT("lcvp"), TEXT("Carries one squad over water, lands it on a beach") },
		{ TEXT("gunboat"), TEXT("Fast; hunts boats, torpedoes ships") },
		{ TEXT("destroyer"), TEXT("Shells whatever your side spots, 120 m (max 2)") },
		{ TEXT("medic"), TEXT("Unarmed; heals squads out of the fight") },
		{ TEXT("tankdestroyer"), TEXT("Long gun, hunts tanks; weak vs infantry") },
		{ TEXT("howitzer"), TEXT("Heavy shells at 95 m; slow, sets up, blind up close") },
		{ TEXT("flamer"), TEXT("Burns out houses and trenches; short range") },
		{ TEXT("bomber"), TEXT("Drops bomb sticks on crowds and dug-in positions") },
		{ TEXT("ranger"), TEXT("Elite, bazookas, satchel charges") },
		{ TEXT("tiger"), TEXT("Heavy tank, thick front armor (max 1)") },
		{ TEXT("conscript"), TEXT("Cheap waves, Ura! sprint") },
		{ TEXT("churchill"), TEXT("Slow infantry tank, thickest front armor (max 1)") },
		{ TEXT("commando"), TEXT("Raiders, Stens and satchels; hide when still") },
		{ TEXT("engineer"), TEXT("Builds depots, weak rifles") },
	};
	// END GENERATED SOURCE ROLES

	struct FTable
	{
		TArray<FTCRecruitDef> Defs;
		TMap<FString, int32> Index;
		FTable()
		{
			for (const FRow& R : Rows)
			{
				FTCRecruitDef D;
				D.Type = R.Type; D.Name = R.Name; D.Cost = R.Cost; D.ClassicCost = R.ClassicCost; D.Fuel = R.Fuel; D.Pop = R.Pop; D.Max = R.Max;
				D.Faction = R.Faction; D.bClassic = R.bClassic; D.bNaval = R.bNaval; D.bStructure = R.bStructure; D.bBuilding = R.bBuilding;
				D.bAir = R.bAir; D.bInfantry = R.bInfantry; D.Train = R.Train; D.Tier = R.Tier;
				FString(R.Makes).ParseIntoArray(D.Makes, TEXT(","));
				Index.Add(D.Type, Defs.Num());
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
	bool NumField(const FJsonObject& O, const TCHAR* Name, double& Out) { return Num(O.TryGetField(Name), Out); }

	// How JavaScript writes a number into a sentence (String(n)), for the values these sentences hold.
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

	// JavaScript's Math.min and Math.max: NaN anywhere gives NaN.
	double Inf() { return std::numeric_limits<double>::infinity(); }
	double JsMin(std::initializer_list<double> Vs) { double M = Inf(); for (double V : Vs) { if (FMath::IsNaN(V)) return V; if (V < M) M = V; } return M; }
	double JsMax(double A, double B) { return FMath::IsNaN(A) || FMath::IsNaN(B) ? FMath::IsNaN(A) ? A : B : A > B ? A : B; }
	// Math.round for the non-negative values it rounds here
	double JsRound(double V) { return FMath::FloorToDouble(V + 0.5); }

	// shared/sim.js canBuild
	bool CanBuild(const FTCRecruitDef& D, int32 Faction) { return D.Faction == AnyFaction || D.Faction == Faction; }
	// client/hud.js groupOf and rank
	int32 GroupOf(const FTCRecruitDef& D)
	{
		if (D.bNaval) return 4;
		if (D.bAir) return 3;
		for (const TCHAR* T : SupportWeapons) if (D.Type == T) return 1;
		return D.bInfantry ? 0 : 2;
	}
	int32 Rank(const FString& Type)
	{
		for (int32 I = 0; I < (int32)UE_ARRAY_COUNT(CardOrder); I++) if (Type == CardOrder[I]) return I;
		const int32* At = Table().Index.Find(Type);
		return (int32)UE_ARRAY_COUNT(CardOrder) + (At ? *At : Table().Defs.Num());
	}

	FTCRecruitCheck Yes() { FTCRecruitCheck C; C.bOk = true; return C; }
	FTCRecruitCheck No(const FString& Reason, bool bNative = false) { FTCRecruitCheck C; C.Reason = Reason; C.bNative = bNative; return C; }
}

const FTCRecruitDef* FTCRecruitment::Find(const FString& Type)
{
	const int32* At = Table().Index.Find(Type);
	return At ? &Table().Defs[*At] : nullptr;
}

FString FTCRecruitment::UnitTip(const FString& Type, const FString& DisplayName, const FString& Extra)
{
	const FTCRecruitDef* D=Find(Type);
	if (!D) return DisplayName+Extra;
	FString Tip=DisplayName;
	if (!DisplayName.Equals(D->Name,ESearchCase::CaseSensitive)) Tip+=TEXT(" (")+D->Name+TEXT(")");
	for (const FRole& R : SourceRoles) if (Type.Equals(R.Type,ESearchCase::CaseSensitive)) { Tip+=TEXT(": ")+FString(R.Text); break; }
	return Tip+Extra;
}

FString FTCRecruitment::CardTip(const FTCRecruitCard& Card, const FString& DisplayName) const
{
	// feedback.js replaces an unavailable card's whole tooltip with its refusal reason.
	if (!Card.Check.bOk) return Card.Check.Reason;
	const FTCRecruitDef* D=Find(Card.Type);
	if (!D) return DisplayName;
	const FTCPrice P=PriceOf(Card.Type);
	const FString Extra=Classic()
		? FString::Printf(TEXT(". %d MP%s, trains in %ds"),P.Mp,P.Fuel?*FString::Printf(TEXT(" + %d Fuel"),P.Fuel):TEXT(""),D->Train)
		: FString::Printf(TEXT(". %d MP"),D->Cost);
	return UnitTip(Card.Type,DisplayName,Extra);
}

const TCHAR* FTCRecruitment::GroupName(int32 Group)
{
	return Group >= 0 && Group < (int32)UE_ARRAY_COUNT(GroupNames) ? GroupNames[Group] : TEXT("");
}

FString FTCRecruitment::TierName(int32 Tier)
{
	// TIER_NAMES[n] is undefined past the last tier; the template string would print "undefined"
	return Tier >= 0 && Tier < (int32)UE_ARRAY_COUNT(TierNames) ? FString(TierNames[Tier]) : FString(TEXT("undefined"));
}

TSharedRef<FJsonObject> FTCBuy::ToMessage() const
{
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("buy"));
	M->SetStringField(TEXT("unit"), Unit);
	if (bHasFrom) M->SetNumberField(TEXT("from"), From);
	return M;
}

void FTCRecruitment::SetUnitTableReference(const FString& Reference)
{
	UnitTableReference = Reference;
	bReferenceMatches = Reference == ReferencePin;
}

void FTCRecruitment::Clear()
{
	bStarted = bHasSnapshot = false;
	Factions.Reset();
	bNaval = false;
	SnapMp = 0; SnapFuel.Reset();
	ModeKind.Reset(); bSuddenDeath = false; ModeSlot.Reset();
	TechTier.Reset(); ArmyPop.Reset(); WorldCap.Reset(); bWorldRecovery = false;
	OutSeats.Reset(); Queues.Reset(); Jobs.Reset();
	RowOrder.Reset(); RowIds.Reset();
}

void FTCRecruitment::ApplyStart(const FJsonObject& M)
{
	// client/main.js start: factions = m.factions ?? [], lastSnap = null, the held rows reset; naval is
	// lastStart.map.naval === true
	Clear();
	bStarted = true;
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	if (M.TryGetArrayField(TEXT("factions"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			double F;
			Factions.Add(Num(V, F) ? TOptional<int32>((int32)F) : TOptional<int32>());
		}
	}
	const TSharedPtr<FJsonObject>* Map;
	if (M.TryGetObjectField(TEXT("map"), Map) && Map->IsValid())
	{
		const TSharedPtr<FJsonValue> Naval = (*Map)->TryGetField(TEXT("naval"));
		bNaval = Naval.IsValid() && Naval->Type == EJson::Boolean && Naval->AsBool();
	}
}

void FTCRecruitment::ApplySnapshot(const FJsonObject& M)
{
	if (!bStarted) return;
	bHasSnapshot = true;
	double D;
	SnapMp = NumField(M, TEXT("mp"), D) ? D : 0; // snapshotFor always sends mp (Math.floor of the wallet)
	SnapFuel.Reset(); if (NumField(M, TEXT("fuel"), D)) SnapFuel = D; // Classic and World only

	ModeKind.Reset(); bSuddenDeath = false; ModeSlot.Reset();
	const TSharedPtr<FJsonObject>* Obj;
	if (M.TryGetObjectField(TEXT("mode"), Obj) && Obj->IsValid())
	{
		(*Obj)->TryGetStringField(TEXT("kind"), ModeKind);
		(*Obj)->TryGetBoolField(TEXT("suddenDeath"), bSuddenDeath);
		if (NumField(**Obj, TEXT("slot"), D)) ModeSlot = D;
	}
	TechTier.Reset();
	if (M.TryGetObjectField(TEXT("tech"), Obj) && Obj->IsValid() && NumField(**Obj, TEXT("tier"), D)) TechTier = D;
	ArmyPop.Reset();
	if (M.TryGetObjectField(TEXT("army"), Obj) && Obj->IsValid() && NumField(**Obj, TEXT("pop"), D)) ArmyPop = D;
	WorldCap.Reset(); bWorldRecovery = false;
	if (M.TryGetObjectField(TEXT("world"), Obj) && Obj->IsValid())
	{
		if (NumField(**Obj, TEXT("cap"), D)) WorldCap = D;
		const TSharedPtr<FJsonObject>* Recovery;
		if ((*Obj)->TryGetObjectField(TEXT("recovery"), Recovery) && Recovery->IsValid()) (*Recovery)->TryGetBoolField(TEXT("available"), bWorldRecovery);
	}

	const TArray<TSharedPtr<FJsonValue>>* Arr;
	OutSeats.Reset();
	if (M.TryGetArrayField(TEXT("out"), Arr)) for (const TSharedPtr<FJsonValue>& V : *Arr) OutSeats.Add(V.IsValid() && V->Type == EJson::Boolean && V->AsBool());

	// queues: [id, progress, rally x, rally z, ...types]; the browser reads the first row of an id
	Queues.Reset();
	if (M.TryGetArrayField(TEXT("queues"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			double Id;
			if (!V.IsValid() || !V->TryGetArray(Row) || Row->Num() < 1 || !Num((*Row)[0], Id) || Queues.Contains((int32)Id)) continue;
			FTCProductionQueue Q;
			double P, RX, RZ;
			if (Row->Num() > 1 && Num((*Row)[1], P)) Q.Progress = P;
			if (Row->Num() > 3 && Num((*Row)[2], RX) && Num((*Row)[3], RZ) && RX >= 0) Q.Rally = FVector2D(RX, RZ);
			for (int32 I = 4; I < Row->Num(); I++) Q.Types.Add((*Row)[I].IsValid() && (*Row)[I]->Type == EJson::String ? (*Row)[I]->AsString() : FString());
			Queues.Add((int32)Id, MoveTemp(Q));
		}
	}
	// productionJobs: [id, [{ id, type, status, mp, fuel }, ...]], the receiver's own buildings
	Jobs.Reset();
	if (M.TryGetArrayField(TEXT("productionJobs"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			const TArray<TSharedPtr<FJsonValue>>* List;
			double Id;
			if (!V.IsValid() || !V->TryGetArray(Row) || Row->Num() < 2 || !Num((*Row)[0], Id) || Jobs.Contains((int32)Id) || !(*Row)[1].IsValid() || !(*Row)[1]->TryGetArray(List)) continue;
			TArray<FTCProductionJob>& Out = Jobs.Add((int32)Id);
			for (const TSharedPtr<FJsonValue>& J : *List)
			{
				const TSharedPtr<FJsonObject>* JO;
				if (!J.IsValid() || !J->TryGetObject(JO) || !JO->IsValid()) continue;
				FTCProductionJob Job;
				if (NumField(**JO, TEXT("id"), D)) Job.Id = (int32)D;
				(*JO)->TryGetStringField(TEXT("type"), Job.Type);
				(*JO)->TryGetStringField(TEXT("status"), Job.Status);
				if (NumField(**JO, TEXT("mp"), D)) Job.Mp = (int32)D;
				if (NumField(**JO, TEXT("fuel"), D)) Job.Fuel = (int32)D;
				Out.Add(MoveTemp(Job));
			}
		}
	}

	// The row order client/world-state.js keeps (a JavaScript Map), as FTCMatchState applies the rows: a full resend (or
	// no held) starts over, gone ids leave, a new id goes last and a known one keeps its place.
	const TArray<TSharedPtr<FJsonValue>>* Held = nullptr;
	bool bAll = false;
	M.TryGetBoolField(TEXT("all"), bAll);
	if (bAll || !(M.TryGetArrayField(TEXT("held"), Held) && Held->Num() >= 2)) { RowOrder.Reset(); RowIds.Reset(); }
	if (M.TryGetArrayField(TEXT("gone"), Arr))
	{
		TSet<int32> Gone;
		for (const TSharedPtr<FJsonValue>& V : *Arr) if (Num(V, D) && RowIds.Remove((int32)D)) Gone.Add((int32)D);
		if (Gone.Num()) RowOrder.RemoveAll([&Gone](int32 Id) { return Gone.Contains(Id); });
	}
	if (M.TryGetArrayField(TEXT("units"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			if (!V.IsValid() || !V->TryGetArray(Row) || Row->Num() < 1 || !Num((*Row)[0], D)) continue;
			bool bKnown = false;
			RowIds.Add((int32)D, &bKnown);
			if (!bKnown) RowOrder.Add((int32)D);
		}
	}
}

bool FTCRecruitment::Classic() const { return ModeKind == TEXT("classic") || ModeKind == TEXT("world"); }

bool FTCRecruitment::SkirmishBase() const
{
	return ModeKind.IsEmpty() || ModeKind == TEXT("conquest") || ModeKind == TEXT("assault") || ModeKind == TEXT("annihilation") || ModeKind == TEXT("horde");
}

int32 FTCRecruitment::FactionOf(int32 Slot) const
{
	if (Factions.IsValidIndex(Slot) && Factions[Slot].IsSet()) return Factions[Slot].GetValue();
	return Slot < 0 ? 0 : Slot % 3;
}

FTCPrice FTCRecruitment::PriceOf(const FString& Type) const
{
	FTCPrice P;
	const FTCRecruitDef* D = Find(Type);
	if (!D) return P;
	P.Mp = Classic() && D->ClassicCost >= 0 ? D->ClassicCost : D->Cost;
	P.Fuel = Classic() ? D->Fuel : 0;
	return P;
}

const FTCProductionQueue* FTCRecruitment::QueueOf(int32 BuildingId) const { return Queues.Find(BuildingId); }

bool FTCRecruitment::Allied(const FTCMatchState& State, int32 A, int32 B) const
{
	// shared/sim.js allied over the browser's players (teams.map(team => ({ team })), teams defaulting to the seat)
	if (ModeKind == TEXT("world") && A == -1 && B == -1) return true;
	const int32 Seats = State.Teams.Num() ? State.Teams.Num() : State.Names.Num();
	if (A < 0 || B < 0 || A >= Seats || B >= Seats) return false;
	const int32 TA = State.Teams.Num() ? State.Teams[A] : A, TB = State.Teams.Num() ? State.Teams[B] : B;
	return TA == TB;
}

bool FTCRecruitment::ProductionAccess(const FTCMatchState& State, int32 Owner) const
{
	// shared/sim.js productionAccess: your own, or in Horde an ally's (never the Horde seat's)
	const int32 Slot = State.You;
	if (Owner == Slot) return true;
	if (ModeKind != TEXT("horde")) return false;
	const bool bSlotIsHorde = ModeSlot.IsSet() && ModeSlot.GetValue() == Slot, bOwnerIsHorde = ModeSlot.IsSet() && ModeSlot.GetValue() == Owner;
	return !bSlotIsHorde && !bOwnerIsHorde && Allied(State, Owner, Slot);
}

TArray<int32> FTCRecruitment::ProductionBuildings(const FTCMatchState& State, const FString& Type) const
{
	// shared/sim.js productionBuildings over the snapshot's rows, in the browser's row order
	TArray<int32> Out;
	auto Consider = [&](int32 Id)
	{
		const FTCUnit* U = State.Units.Find(Id);
		const FTCRecruitDef* D = U ? Find(U->Type) : nullptr;
		if (!U || !D || U->Hp <= 0 || U->Built < 1 || !ProductionAccess(State, U->Owner) || !D->Makes.Num()) return;
		if (Type.IsEmpty() || D->Makes.Contains(Type) || (SkirmishBase() && D->Type == TEXT("barracks") && Type == TEXT("rifle"))) Out.Add(Id);
	};
	for (int32 Id : RowOrder) Consider(Id);
	// rows the order never saw (the hooks were wired late): after the rest, in map order
	for (const auto& Pair : State.Units) if (!RowIds.Contains(Pair.Key)) Consider(Pair.Key);
	return Out;
}

bool FTCRecruitment::PopCap(double& Out) const
{
	// client/availability.js: s.world?.cap ?? popCap(s); popCap of a snapshot (no players) is the World cap or the
	// mode's limit times the army size, rounded
	if (WorldCap.IsSet()) { Out = WorldCap.GetValue(); return true; }
	if (ModeKind == TEXT("world")) return false; // the browser would throw here (popCap reads g.players)
	Out = JsRound((Classic() ? ClassicPopCap : SkirmishPopCap) * (ArmyPop.IsSet() ? ArmyPop.GetValue() : 1.0));
	return true;
}

bool FTCRecruitment::PopTotal(const FTCMatchState& State, double& Out, FString& Unknown) const
{
	// client/availability.js popTotal: own units on the field (structures take none) plus every own queued recruit
	Out = 0;
	for (const auto& Pair : State.Units)
	{
		const FTCUnit& U = Pair.Value;
		if (U.Owner != State.You) continue;
		const FTCRecruitDef* D = Find(U.Type);
		if (!D) { Unknown = U.Type; return false; }
		if (!D->bStructure) Out += D->Pop;
	}
	for (const auto& Pair : State.Units)
	{
		if (Pair.Value.Owner != State.You) continue;
		if (const FTCProductionQueue* Q = Queues.Find(Pair.Key))
		{
			for (const FString& T : Q->Types)
			{
				const FTCRecruitDef* D = Find(T);
				if (!D) { Unknown = T; return false; }
				Out += D->Pop;
			}
		}
	}
	return true;
}

bool FTCRecruitment::ArmyUse(const FTCMatchState& State, double& OutPop, double& OutCap) const
{
	// only for a seat this client commands: a spectator's start names seat 0, whose army is not the watcher's
	FString Unknown;
	return bHasSnapshot && State.SeatConfirmed() && PopTotal(State, OutPop, Unknown) && PopCap(OutCap);
}

int32 FTCRecruitment::Have(const FTCMatchState& State, const FString& Type) const
{
	int32 N = 0;
	for (const auto& Pair : State.Units)
	{
		if (Pair.Value.Owner != State.You) continue;
		if (Pair.Value.Type == Type) N++;
		if (const FTCProductionQueue* Q = Queues.Find(Pair.Key)) for (const FString& T : Q->Types) if (T == Type) N++;
	}
	return N;
}

FTCRecruitCheck FTCRecruitment::Gate(const FTCMatchState& State) const
{
	if (!bReferenceMatches)
	{
		return No(FString::Printf(TEXT("Recruit data is for reference %s, the unit table is for %s"), ReferencePin,
			UnitTableReference.IsEmpty() ? TEXT("an unknown reference") : *UnitTableReference), true);
	}
	if (!bHasSnapshot) return No(TEXT("Waiting for the army"));
	if (State.bSpectator || (OutSeats.IsValidIndex(State.You) && OutSeats[State.You])) return No(TEXT("You are spectating"));
	if (!State.SeatConfirmed()) return No(TEXT("Waiting for the lobby to confirm this seat"), true);
	return Yes();
}

FTCBuy FTCRecruitment::RecruitAction(const FTCMatchState& State, const FString& Unit, const TSet<int32>& Selected) const
{
	FTCBuy A;
	A.Unit = Unit;
	if (!bHasSnapshot || !SkirmishBase()) return A;
	for (int32 Id : ProductionBuildings(State, Unit))
	{
		if (Selected.Contains(Id)) { A.From = Id; A.bHasFrom = true; break; }
	}
	return A;
}

FTCRecruitCheck FTCRecruitment::CheckBuy(const FTCMatchState& State, const FTCBuy& Action) const
{
	const FTCRecruitCheck G = Gate(State);
	if (!G.bOk) return G;
	const bool bClassic = Classic(), bSudden = bClassic && bSuddenDeath, bSkirmish = SkirmishBase();
	const FTCRecruitDef* Def = Find(Action.Unit);
	if (!Def || Def->bStructure || (Def->bClassic && !bClassic)) return No(TEXT("Unavailable in this mode"));
	if (Def->bNaval && !bNaval) return No(TEXT("Needs a map with a sea"));
	if (TechTier.IsSet() && Def->Tier > TechTier.GetValue()) return No(TEXT("Needs ") + TierName(Def->Tier));
	const FTCPrice Price = PriceOf(Action.Unit);
	if (SnapMp < Price.Mp) return No(FString::Printf(TEXT("Needs %d MP"), Price.Mp));
	if (Price.Fuel && !(SnapFuel.IsSet() && SnapFuel.GetValue() >= Price.Fuel)) return No(FString::Printf(TEXT("Needs %d fuel"), Price.Fuel));
	double Pop = 0, Cap = 0;
	FString Unknown;
	if (!PopTotal(State, Pop, Unknown)) return No(FString::Printf(TEXT("Unknown unit type '%s' in the army"), *Unknown), true);
	if (!PopCap(Cap)) return No(TEXT("The army limit is missing from the World snapshot"), true);
	if (Pop + Def->Pop > Cap) return No(FString::Printf(TEXT("Army at its limit (%s/%s)"), *JsNumber(Pop), *JsNumber(Cap)));
	if (Def->Max >= 0 && Have(State, Action.Unit) >= Def->Max) return No(TEXT("Maximum of this unit reached"));
	if (bClassic || bSkirmish)
	{
		if (bSudden) return No(TEXT("Not during Sudden Death"));
		TArray<int32> Makers;
		if (bSkirmish) Makers = ProductionBuildings(State, Action.Unit);
		else
		{
			for (const auto& Pair : State.Units)
			{
				const FTCRecruitDef* D = Pair.Value.Owner == State.You ? Find(Pair.Value.Type) : nullptr;
				if (D && Pair.Value.Built >= 1 && D->Makes.Contains(Action.Unit)) Makers.Add(Pair.Key);
			}
		}
		if (!Makers.Num())
		{
			const FTCRecruitDef* Maker = nullptr;
			for (const FTCRecruitDef& D : Table().Defs) if (D.Makes.Contains(Action.Unit)) { Maker = &D; break; }
			return No(FString::Printf(TEXT("Needs a %s"), Maker ? *Maker->Name : TEXT("Production Building")));
		}
		// a card on a selected building asks that building (from); the server refuses a full one rather than use another
		const bool bAny = Makers.ContainsByPredicate([&](int32 Id)
		{
			const FTCProductionQueue* Q = Queues.Find(Id);
			return (bSkirmish || (Q ? Q->Types.Num() : 0) < QueuePlaces) && (!Action.bHasFrom || Id == Action.From);
		});
		if (!bAny) return No(bSkirmish ? TEXT("Selected building cannot produce this unit") : TEXT("The training queue is full"));
	}
	return Yes();
}

FTCRecruitCheck FTCRecruitment::CheckCancel(const FTCMatchState& State, int32 BuildingId, int32 Job) const
{
	const FTCRecruitCheck G = Gate(State);
	if (!G.bOk) return G;
	if (Classic() && bSuddenDeath) return No(TEXT("Not during Sudden Death"));
	const FTCUnit* B = State.Units.Find(BuildingId);
	const TArray<FTCProductionJob>* List = Jobs.Find(BuildingId);
	const bool bOwnFinished = B && B->Owner == State.You && B->Built >= 1;
	const bool bWaiting = List && List->ContainsByPredicate([Job](const FTCProductionJob& J) { return J.Id == Job && J.Status == TEXT("waiting"); });
	return bOwnFinished && bWaiting ? Yes() : No(TEXT("That recruit is no longer waiting"));
}

double FTCRecruitment::BuyCountJs(const FTCMatchState& State, const FTCBuy& Action, double Want) const
{
	// client/availability.js buyCount, in JavaScript's arithmetic (NaN and Infinity included)
	if (!CheckBuy(State, Action).bOk) return 0;
	const FTCRecruitDef* Def = Find(Action.Unit);
	const FTCPrice Price = PriceOf(Action.Unit);
	double Pop = 0, Cap = 0;
	FString Unknown;
	if (!PopTotal(State, Pop, Unknown) || !PopCap(Cap)) return 0; // CheckBuy has refused these already
	int32 Room = 0;
	for (const auto& Pair : State.Units)
	{
		const FTCRecruitDef* D = Pair.Value.Owner == State.You ? Find(Pair.Value.Type) : nullptr;
		if (!D || Pair.Value.Built < 1 || !D->Makes.Contains(Action.Unit) || (Action.bHasFrom && Pair.Key != Action.From)) continue;
		const FTCProductionQueue* Q = Queues.Find(Pair.Key);
		Room += FMath::Max(0, QueuePlaces - (Q ? Q->Types.Num() : 0));
	}
	return JsMax(0, JsMin({
		Want,
		Price.Mp ? FMath::FloorToDouble(SnapMp / Price.Mp) : Want,
		Price.Fuel ? FMath::FloorToDouble((SnapFuel.IsSet() ? SnapFuel.GetValue() : 0) / Price.Fuel) : Want,
		FMath::FloorToDouble((Cap - Pop) / Def->Pop), // popUse 0 gives Infinity, or NaN at the limit, as in JavaScript
		(Def->Max >= 0 ? (double)Def->Max : Inf()) - Have(State, Action.Unit),
		Classic() ? (double)Room : Want }));
}

int32 FTCRecruitment::BuyCount(const FTCMatchState& State, const FTCBuy& Action, int32 Want) const
{
	const double N = BuyCountJs(State, Action, Want);
	return FMath::IsNaN(N) ? 0 : (int32)FMath::Clamp(N, 0.0, (double)MAX_int32);
}

FTCBuyResult FTCRecruitment::Buy(const FTCMatchState& State, const FTCBuy& Action, bool bMany) const
{
	// client/hud.js buy: attempt (the reason instead when refused), then Shift sends max(1, buyCount(.., 5)) copies
	FTCBuyResult R;
	R.Check = CheckBuy(State, Action);
	if (!R.Check.bOk) return R;
	const double N = bMany ? JsMax(1, BuyCountJs(State, Action, 5)) : 1;
	for (int32 I = 0; I < N; I++) R.Messages.Add(Action.ToMessage()); // NaN sends none, as the browser's loop
	return R;
}

FTCBuyResult FTCRecruitment::CancelProduction(const FTCMatchState& State, int32 BuildingId, int32 Job) const
{
	FTCBuyResult R;
	R.Check = CheckCancel(State, BuildingId, Job);
	if (!R.Check.bOk) return R;
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("cancelProduction"));
	M->SetNumberField(TEXT("id"), BuildingId);
	M->SetNumberField(TEXT("job"), Job);
	R.Messages.Add(M);
	return R;
}

TArray<FTCRecruitGroup> FTCRecruitment::Groups(const TArray<FString>& Types) const
{
	// client/hud.js groupsHTML: the non-empty groups in order, each one's types by rank
	TArray<FTCRecruitGroup> Out;
	for (int32 G = 0; G < (int32)UE_ARRAY_COUNT(GroupNames); G++)
	{
		TArray<FString> In = Types.FilterByPredicate([G](const FString& T) { const FTCRecruitDef* D = Find(T); return D && GroupOf(*D) == G; });
		if (!In.Num()) continue;
		Algo::StableSortBy(In, [](const FString& T) { return Rank(T); });
		FTCRecruitGroup& Group = Out.AddDefaulted_GetRef();
		Group.Group = G; Group.Name = GroupNames[G];
		for (const FString& T : In) { FTCRecruitCard& C = Group.Cards.AddDefaulted_GetRef(); C.Type = T; C.Name = Find(T)->Name; }
	}
	return Out;
}

FTCCommandCard FTCRecruitment::Card(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	FTCCommandCard Card;
	if (!bHasSnapshot) return Card; // the HUD draws its cards from snapshots only
	TArray<const FTCUnit*> Sel; // client/hud.js selUnits
	for (int32 Id : Selected) if (const FTCUnit* U = State.Units.Find(Id)) Sel.Add(U);
	if (Sel.Num() && !Sel.ContainsByPredicate([](const FTCUnit* U) { return U->Type != TEXT("truck"); }))
	{
		// hud.js update skips the card for an all-truck selection: it stays as it was, or the logistics card replaces it
		Card.Kind = ETCCardKind::NotPorted;
		Card.Why = TEXT("an all-truck selection keeps the last card or shows the logistics card");
		return Card;
	}
	const int32 Me = State.You;
	if (!Classic())
	{
		// client/hud.js buildCard and drawRecruit: every unit this side can recruit, always shown
		TArray<FString> Types;
		for (const FTCRecruitDef& D : Table().Defs)
		{
			if (D.Type != TEXT("truck") && D.Type != TEXT("supplycache") && CanBuild(D, FactionOf(Me)) && !D.bClassic && (!D.bNaval || bNaval)) Types.Add(D.Type);
		}
		Card.Kind = ETCCardKind::Recruit;
		Card.Groups = Groups(Types);
		for (FTCRecruitGroup& G : Card.Groups)
		{
			for (FTCRecruitCard& C : G.Cards)
			{
				const FTCRecruitDef* D = Find(C.Type);
				C.Cost = FString::Printf(TEXT("%d MP"), D->Cost);
				C.bBroke = SnapMp < D->Cost;
				C.Action = RecruitAction(State, C.Type, Selected);
				C.Check = CheckBuy(State, C.Action);
			}
		}
		// selecting one finished own Production Building suggests the group it makes most of
		const FTCRecruitDef* BD = Sel.Num() == 1 ? Find(Sel[0]->Type) : nullptr;
		if (BD && BD->bBuilding && Sel[0]->Built >= 1 && Sel[0]->Owner == Me && BD->Makes.Num())
		{
			int32 Best = -1, Most = 0;
			for (int32 I = 0; I < Card.Groups.Num(); I++)
			{
				const int32 N = Card.Groups[I].Cards.FilterByPredicate([BD](const FTCRecruitCard& C) { return BD->Makes.Contains(C.Type); }).Num();
				if (N > Most) { Most = N; Best = I; }
			}
			Card.SuggestedGroup = Best;
		}
		return Card;
	}

	// client/hud.js drawClassicCard
	const FTCRecruitDef* SD = Sel.Num() == 1 ? Find(Sel[0]->Type) : nullptr;
	const FTCUnit* Bld = SD && SD->bBuilding && Sel[0]->Owner == Me ? Sel[0] : nullptr;
	const bool bEngineers = !Bld && Sel.ContainsByPredicate([Me](const FTCUnit* U) { return U->Owner == Me && U->Type == TEXT("engineer"); });
	if (Bld && Bld->Built < 1)
	{
		Card.Kind = ETCCardKind::Site; Card.Building = Bld->Id; Card.Built = Bld->Built;
		return Card;
	}
	if (Bld)
	{
		Card.Kind = ETCCardKind::Building; Card.Building = Bld->Id;
		TArray<FString> Types;
		for (const FString& T : SD->Makes) { const FTCRecruitDef* D = Find(T); if (D && CanBuild(*D, FactionOf(Me))) Types.Add(T); }
		Card.Groups = Groups(Types);
		for (FTCRecruitGroup& G : Card.Groups)
		{
			for (FTCRecruitCard& C : G.Cards)
			{
				const FTCPrice P = PriceOf(C.Type);
				C.Cost = FString::Printf(TEXT("%d MP"), P.Mp);
				C.Sub = (P.Fuel ? FString::Printf(TEXT("%d Fuel, "), P.Fuel) : FString()) + FString::Printf(TEXT("%ds"), Find(C.Type)->Train);
				C.bBroke = SnapMp < P.Mp || (SnapFuel.IsSet() ? SnapFuel.GetValue() : 0) < P.Fuel;
				C.Action.Unit = C.Type; C.Action.From = Bld->Id; C.Action.bHasFrom = true;
				C.Check = CheckBuy(State, C.Action);
			}
		}
		// the queue line and its five places (hud.js drawQueue)
		const FTCProductionQueue* Q = Queues.Find(Bld->Id);
		const TArray<FTCProductionJob>* List = Jobs.Find(Bld->Id);
		const int32 Queued = Q ? Q->Types.Num() : 0;
		Card.Progress = Q ? Q->Progress : 0;
		Card.bTraining = Queued > 0;
		if (Queued)
		{
			const FTCRecruitDef* First = Find(Q->Types[0]);
			Card.QueueLine = FString::Printf(TEXT("Training %s, %s s left"), First ? *First->Name : *Q->Types[0],
				First ? *JsNumber(FMath::CeilToDouble(First->Train * (1 - Card.Progress))) : TEXT("NaN"));
		}
		else Card.QueueLine = TEXT("Idle: click a unit to train it");
		for (int32 I = 0; I < QueuePlaces; I++)
		{
			FTCQueueSlot& S = Card.Slots.AddDefaulted_GetRef();
			if (I >= Queued) continue;
			S.Type = Q->Types[I];
			S.bTraining = I == 0;
			const FTCProductionJob* Job = List && List->IsValidIndex(I) && (*List)[I].Type == S.Type ? &(*List)[I] : nullptr;
			if (!I || !Job || Job->Status != TEXT("waiting")) continue;
			S.Job = Job->Id; S.RefundMp = Job->Mp; S.RefundFuel = Job->Fuel;
			const FTCBuyResult Cancel = CancelProduction(State, Bld->Id, Job->Id);
			S.Check = Cancel.Check;
			TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
			M->SetStringField(TEXT("t"), TEXT("cancelProduction"));
			M->SetNumberField(TEXT("id"), Bld->Id);
			M->SetNumberField(TEXT("job"), Job->Id);
			S.Cancel = M;
		}
		return Card;
	}
	if (bEngineers) { Card.Kind = ETCCardKind::NotPorted; Card.Why = TEXT("the Engineers' build card"); return Card; }
	if (bWorldRecovery) { Card.Kind = ETCCardKind::NotPorted; Card.Why = TEXT("World recovery (Restore HQ or Engineer)"); return Card; }
	return Card;
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Client/TCLocale.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRecruitTooltipsTest, "ThreeCrossroads.Recruitment.Tooltips", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRecruitTooltipsTest::RunTest(const FString&)
{
 // Exact original hud.js unitTip and feedback.js replacement, with no added purchase instructions.
 TestEqual(TEXT("generic names show the original role"),FTCRecruitment::UnitTip(TEXT("rifle"),TEXT("Rifle Squad")),FString(TEXT("Rifle Squad: Captures, all-round")));
 TestEqual(TEXT("faction names retain the generic name in parentheses"),FTCRecruitment::UnitTip(TEXT("rifle"),TEXT("Grenadiers")),FString(TEXT("Grenadiers (Rifle Squad): Captures, all-round")));
 TestEqual(TEXT("a type without a role keeps its name"),FTCRecruitment::UnitTip(TEXT("hq"),TEXT("HQ")),FString(TEXT("HQ")));
 FTCRecruitment R;
 auto Parse=[](const FString& Text) { TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text),O); return O; };
 R.ApplyStart(*Parse(TEXT(R"({"factions":[0,1],"map":{}})")));
 FTCRecruitCard Card; Card.Type=TEXT("rifle"); Card.Check.bOk=true;
 R.ApplySnapshot(*Parse(TEXT(R"({"mode":{"kind":"conquest"}})")));
 const FString Recruit=R.CardTip(Card,TEXT("Rifle Squad"));
 TestEqual(TEXT("Conquest suffix is the original MP price"),Recruit,FString(TEXT("Rifle Squad: Captures, all-round. 100 MP")));
 R.ApplySnapshot(*Parse(TEXT(R"({"mode":{"kind":"classic"}})")));
 Card.Type=TEXT("tank");
 TestEqual(TEXT("Classic suffix has the source fuel and training sentence"),R.CardTip(Card,TEXT("M5 Stuart")),FString(TEXT("M5 Stuart (Light Tank): Kills infantry, weak rear. 200 MP + 60 Fuel, trains in 35s")));
 Card.Check.bOk=false; Card.Check.Reason=TEXT("The training queue is full");
 TestEqual(TEXT("an unavailable card shows only its original reason"),R.CardTip(Card,TEXT("M5 Stuart")),Card.Check.Reason);
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 {
  FTCLocale::FScopedLanguage Lang(Language);
  TestEqual(TEXT("the original English-keyed role and price translate together"),TC_T(Recruit),Language==ETCLanguage::English?Recruit:FString(TEXT("Pelotón de fusileros: Captura, polivalente. 100 MO")));
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRecruitPurchasesTest, "ThreeCrossroads.Recruitment.Purchases", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRecruitPurchasesTest::RunTest(const FString&)
{
 // The original availability.js and hud.js oracle exercises these same buys and the waiting-job cancellation.
 FTCMatchState S; S.bHasMatch=true; S.You=0; S.LobbySeat=0; S.bLobbySinceOpen=true;
 FTCRecruitment R; R.SetUnitTableReference(FTCRecruitment::ReferencePin);
 auto Parse=[](const FString& Text) { TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text),O); return O; };
 R.ApplyStart(*Parse(TEXT(R"({"factions":[0,1],"map":{"naval":true}})")));
 FTCBuy Rifle; Rifle.Unit=TEXT("rifle");
 TestEqual(TEXT("before any snapshot, a buy explains waiting"),R.Buy(S,Rifle,false).Check.Reason,FString(TEXT("Waiting for the army")));
 auto Snap=[&](const TCHAR* Text) { R.ApplySnapshot(*Parse(Text)); };
 Snap(TEXT(R"({"mp":476,"mode":{"kind":"conquest"},"units":[],"queues":[]})"));
 TestEqual(TEXT("Conquest still needs a production building"),R.CheckBuy(S,Rifle).Reason,FString(TEXT("Needs a HQ")));
 FTCUnit Hq; Hq.Id=1; Hq.Type=TEXT("hq"); Hq.Owner=0; Hq.Hp=100; Hq.Built=1; S.Units.Add(1,Hq);
 Rifle.From=1; Rifle.bHasFrom=true;
 Snap(TEXT(R"({"mp":500,"mode":{"kind":"classic"},"units":[[1]],"queues":[[1,0,-1,-1]]})"));
 const FTCBuyResult Five=R.Buy(S,Rifle,true),One=R.Buy(S,Rifle,false);
 TestTrue(TEXT("a click buys one and Shift buys five"),Five.Check.bOk && Five.Messages.Num()==5 && One.Messages.Num()==1);
 for (const TSharedRef<FJsonObject>& M : Five.Messages) TestTrue(TEXT("every card purchase names its selected building in the real buy envelope"),M->GetStringField(TEXT("t"))==TEXT("buy") && M->GetStringField(TEXT("unit"))==TEXT("rifle") && M->GetNumberField(TEXT("from"))==1);
 Snap(TEXT(R"({"mp":99,"mode":{"kind":"classic"},"units":[[1]],"queues":[]})"));
 const FTCBuyResult Broke=R.Buy(S,Rifle,true);
 TestTrue(TEXT("unavailable cards explain instead of sending purchases"),Broke.Messages.IsEmpty() && Broke.Check.Reason==TEXT("Needs 100 MP"));
 Snap(TEXT(R"({"mp":1000,"fuel":59,"mode":{"kind":"classic"},"units":[[1]],"queues":[]})"));
 FTCBuy Tank; Tank.Unit=TEXT("tank");
 TestEqual(TEXT("Classic prices check fuel before a missing maker"),R.CheckBuy(S,Tank).Reason,FString(TEXT("Needs 60 fuel")));
 Snap(TEXT(R"({"mp":1000,"mode":{"kind":"classic"},"units":[[1]],"queues":[[1,0.5,-1,-1,"rifle","rifle","rifle","rifle","rifle"]],"productionJobs":[[1,[{"id":11,"type":"rifle","status":"active","mp":100,"fuel":0},{"id":12,"type":"rifle","status":"waiting","mp":100,"fuel":0}]]]})"));
 TestEqual(TEXT("full production queue refuses the buy"),R.CheckBuy(S,Rifle).Reason,FString(TEXT("The training queue is full")));
 Hq.Id=2; S.Units.Add(2,Hq);
 TestEqual(TEXT("a selected full building does not redirect to a free producer"),R.CheckBuy(S,Rifle).Reason,FString(TEXT("The training queue is full")));
 const FTCBuyResult Cancel=R.CancelProduction(S,1,12);
 TestTrue(TEXT("a waiting job cancels through the real envelope"),Cancel.Check.bOk && Cancel.Messages.Num()==1 && Cancel.Messages[0]->GetStringField(TEXT("t"))==TEXT("cancelProduction") && Cancel.Messages[0]->GetNumberField(TEXT("id"))==1 && Cancel.Messages[0]->GetNumberField(TEXT("job"))==12);
 TestFalse(TEXT("the active job cannot be refunded"),R.CancelProduction(S,1,11).Check.bOk);
 Snap(TEXT(R"({"mp":1000,"mode":{"kind":"classic","suddenDeath":true},"units":[[1]],"queues":[]})"));
 TestEqual(TEXT("Sudden Death disables purchases"),R.CheckBuy(S,Rifle).Reason,FString(TEXT("Not during Sudden Death")));
 Snap(TEXT(R"({"mp":1000,"tech":{"tier":1},"mode":{"kind":"conquest"},"units":[[1]],"queues":[]})"));
 FTCBuy Fighter; Fighter.Unit=TEXT("fighter");
 TestEqual(TEXT("aircraft tiers use the source HQ name"),R.CheckBuy(S,Fighter).Reason,FString(TEXT("Needs Battalion HQ")));
 S.bSpectator=true;
 TestTrue(TEXT("a spectator cannot purchase"),R.Buy(S,Rifle,true).Messages.IsEmpty() && R.CheckBuy(S,Rifle).Reason==TEXT("You are spectating"));
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRecruitArmyCountTest, "ThreeCrossroads.Recruitment.ArmyCount", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRecruitArmyCountTest::RunTest(const FString&)
{
	// availability.js popTotal counts weighted field squads/vehicles plus queued recruits. Entity rows include
	// structures, while individual soldier models, health and control-group assignments do not change population.
	FTCMatchState S; S.bHasMatch=true; S.You=0; S.LobbySeat=0; S.bLobbySinceOpen=true;
	FTCRecruitment R;
	auto Parse=[](const FString& Text) { TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text),O); return O; };
	R.ApplyStart(*Parse(TEXT(R"({"factions":[0,1],"map":{}})")));
	auto Snap=[&](const FString& Tail) { R.ApplySnapshot(*Parse(TEXT("{\"mp\":100,\"units\":[],\"held\":[0,0],")+Tail+TEXT("}"))); };
	auto Unit=[&](int32 Id,const TCHAR* Type,int32 Owner=0) { FTCUnit U; U.Id=Id; U.Type=Type; U.Owner=Owner; U.Hp=100; S.Units.Add(Id,U); };
	Unit(1,TEXT("hq")); Unit(2,TEXT("barracks")); Unit(3,TEXT("rifle")); Unit(4,TEXT("rifle")); Unit(5,TEXT("mg"));
	Snap(TEXT("\"queues\":[]"));
	double Pop=0,Cap=0;
	TestTrue(TEXT("a confirmed seat has an army count"),R.ArmyUse(S,Pop,Cap));
	TestEqual(TEXT("five own entity rows"),S.Units.Num(),5);
	TestEqual(TEXT("HQ and barracks take no population, three field groups use three"),Pop,3.0);
	TestEqual(TEXT("standard skirmish cap"),Cap,12.0);
	S.Units[3].Hp=1;
	TestTrue(TEXT("casualties within a squad preserve its population"),R.ArmyUse(S,Pop,Cap)&&Pop==3.0);
	Unit(6,TEXT("conscript")); Unit(7,TEXT("truck")); Unit(8,TEXT("train")); Unit(9,TEXT("tank"),1);
	Snap(TEXT("\"queues\":[[1,0.4,-1,-1,\"rifle\",\"conscript\"]],\"army\":{\"pop\":1.5}"));
	TestTrue(TEXT("weights plus own queued recruits, allied/enemy rows excluded"),R.ArmyUse(S,Pop,Cap));
	TestEqual(TEXT("conscripts use 0.75, supply trucks and trains zero, queue uses 1.75"),Pop,5.5);
	TestEqual(TEXT("army size scales the skirmish cap"),Cap,18.0);
	Snap(TEXT("\"mode\":{\"kind\":\"classic\"},\"army\":{\"pop\":1.5}"));
	TestTrue(TEXT("Classic uses its 24 base cap"),R.ArmyUse(S,Pop,Cap)&&Cap==36.0);
	Snap(TEXT("\"mode\":{\"kind\":\"world\"},\"world\":{\"cap\":37}"));
	TestTrue(TEXT("World uses the recipient's delivered cap"),R.ArmyUse(S,Pop,Cap)&&Cap==37.0);
	S.bSpectator=true;
	TestFalse(TEXT("a spectator has no commanded population"),R.ArmyUse(S,Pop,Cap));
	return true;
}
#endif
