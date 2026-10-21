// World Conquest region objective and paid recovery. See TCWorldRecovery.h for what is ported from where.
#include "Client/TCWorldRecovery.h"
#include "Client/TCRecruitment.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"

const TCHAR* const FTCWorldRecovery::ReferencePin = TEXT("e4e579a");
const FColor FTCWorldRecovery::NeutralTeamColour(0xaa, 0xaa, 0xaa);
const FColor FTCWorldRecovery::LockedColour(0xd6, 0xb2, 0x5e);
const FColor FTCWorldRecovery::StationFrameColour(0x1b, 0x1b, 0x18);

namespace TCWR
{
	using namespace TCWorldRecoveryCore;

	// A JSON number as it is, anything else (missing, null, text) as NaN: the snapshot fields read here are numbers in
	// every reference message, and NaN makes the browser's comparisons with undefined come out the same (false).
	double Num(const TSharedPtr<FJsonValue>& V)
	{
		return V.IsValid() && V->Type == EJson::Number ? V->AsNumber() : NAN;
	}
	double Field(const FJsonObject& O, const TCHAR* Name) { return Num(O.TryGetField(Name)); }

	// JavaScript truthiness of a JSON value (undefined, null, false, 0, NaN and '' are falsy).
	bool Truthy(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid()) return false;
		switch (V->Type)
		{
		case EJson::Boolean: return V->AsBool();
		case EJson::Number: { const double D = V->AsNumber(); return !FMath::IsNaN(D) && D != 0; }
		case EJson::String: return !V->AsString().IsEmpty();
		case EJson::Array: case EJson::Object: return true;
		default: return false;
		}
	}
	bool Nullish(const TSharedPtr<FJsonValue>& V) { return !V.IsValid() || V->Type == EJson::None || V->Type == EJson::Null; }

	FString Js(double V) { return FString(UTF8_TO_TCHAR(JsNumber(V).c_str())); }

	// What a template literal writes for a JSON value (`${v}`).
	FString Text(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid() || V->Type == EJson::None) return TEXT("undefined");
		switch (V->Type)
		{
		case EJson::Null: return TEXT("null");
		case EJson::String: return V->AsString();
		case EJson::Number: return Js(V->AsNumber());
		case EJson::Boolean: return V->AsBool() ? TEXT("true") : TEXT("false");
		case EJson::Array:
		{
			// Array.prototype.join: null and undefined elements are empty
			TArray<FString> Parts;
			for (const TSharedPtr<FJsonValue>& E : V->AsArray()) Parts.Add(Nullish(E) ? FString() : Text(E));
			return FString::Join(Parts, TEXT(","));
		}
		default: return TEXT("[object Object]");
		}
	}

	const TArray<TSharedPtr<FJsonValue>>* Array(const TSharedPtr<FJsonValue>& V)
	{
		return V.IsValid() && V->Type == EJson::Array ? &V->AsArray() : nullptr;
	}
	const FJsonObject* Object(const TSharedPtr<FJsonValue>& V)
	{
		return V.IsValid() && V->Type == EJson::Object && V->AsObject().IsValid() ? V->AsObject().Get() : nullptr;
	}

	// SameValueZero, the browser's Map and Set key equality, for the numbers kept here.
	bool SameKey(double A, double B) { return A == B || (FMath::IsNaN(A) && FMath::IsNaN(B)); }

	FColor Rgb(uint32 Hex) { return FColor((Hex >> 16) & 0xff, (Hex >> 8) & 0xff, Hex & 0xff); }
	// client/main.js COLORS: blue, red, chalk, orange, violet, cyan
	const uint32 Colours[] = { 0x3b73d6, 0xcc3a2e, 0xece6d6, 0xe2832b, 0x9b5cd4, 0x35b6c0 };
	constexpr int32 ColourCount = UE_ARRAY_COUNT(Colours);
	// An array index the way JavaScript reads one: only a whole number from 0 up names an element.
	int32 IndexOf(double V, int32 Count) { return V >= 0 && V < Count && V == FMath::FloorToDouble(V) ? (int32)V : -1; }

	FTCWorldRegion ParseRegion(const FJsonObject& O)
	{
		FTCWorldRegion R;
		R.Id = Field(O, TEXT("id"));
		R.Name = Text(O.TryGetField(TEXT("name")));
		R.Kind = Text(O.TryGetField(TEXT("kind")));
		R.Team = Field(O, TEXT("team"));
		R.X = Field(O, TEXT("x"));
		R.Z = Field(O, TEXT("z"));
		R.Progress = Field(O, TEXT("progress"));
		R.Capper = Field(O, TEXT("capper"));
		R.bLocked = Truthy(O.TryGetField(TEXT("locked")));
		R.bContested = Truthy(O.TryGetField(TEXT("contested")));
		// shared/sim.js worldSnapshot: runs (an array, possibly empty) with a region map, else bounds [x0, z0, x1, z1].
		// knownTerritoryRuns writes whole cell numbers; a run that is not three numbers is left out.
		if (const TArray<TSharedPtr<FJsonValue>>* Runs = Array(O.TryGetField(TEXT("runs"))))
		{
			R.bRuns = true;
			for (const TSharedPtr<FJsonValue>& Run : *Runs)
			{
				const TArray<TSharedPtr<FJsonValue>>* Row = Array(Run);
				if (!Row || Row->Num() < 3) continue;
				const FRun Cells{ Num((*Row)[0]), Num((*Row)[1]), Num((*Row)[2]) };
				if (FMath::IsFinite(Cells.Row) && FMath::IsFinite(Cells.X0) && FMath::IsFinite(Cells.X1)) R.Runs.Add(Cells);
			}
		}
		if (const TArray<TSharedPtr<FJsonValue>>* Bounds = Array(O.TryGetField(TEXT("bounds"))))
		{
			R.bBounds = true;
			for (int32 I = 0; I < 4; I++) R.Bounds[I] = Bounds->IsValidIndex(I) ? Num((*Bounds)[I]) : NAN;
		}
		return R;
	}
}

const TCHAR* LexToString(ETCWorldAlertKind Kind)
{
	switch (Kind)
	{
	case ETCWorldAlertKind::PointLost: return TEXT("pointLost");
	case ETCWorldAlertKind::PointWon: return TEXT("pointWon");
	case ETCWorldAlertKind::Attack: return TEXT("attack");
	case ETCWorldAlertKind::UnderAttack: return TEXT("underAttack");
	}
	return TEXT("unknown");
}

TCWorldRecoveryCore::FRegionFacts FTCWorldRegion::Facts() const
{
	TCWorldRecoveryCore::FRegionFacts F;
	F.Team = Team; F.Progress = Progress; F.Capper = Capper; F.bContested = bContested;
	return F;
}

bool FTCWorldRegion::Contains(double AtX, double AtZ) const
{
	// insideKnownRegion: runs win over bounds whenever the region was sent with runs, even none
	if (bRuns) return TCWorldRecoveryCore::InsideRuns(Runs.GetData(), Runs.Num(), AtX, AtZ);
	return bBounds && TCWorldRecoveryCore::InsideBounds(Bounds, AtX, AtZ);
}

FColor FTCWorldRecovery::SlotColour(double Slot)
{
	// client/main.js look(slot).color: COLORS[slot] ?? 0xdddddd
	const int32 I = TCWR::IndexOf(Slot, TCWR::ColourCount);
	return I >= 0 ? TCWR::Rgb(TCWR::Colours[I]) : TCWR::Rgb(0xdddddd);
}

void FTCWorldRecovery::ParseWorld(const FJsonObject& World, FTCWorldSnapshot& Out)
{
	Out.bWorld = true;
	World.TryGetStringField(TEXT("size"), Out.Size);
	Out.Total = TCWR::Field(World, TEXT("total"));
	Out.Goal = TCWR::Field(World, TEXT("goal"));
	Out.Owned = TCWR::Field(World, TEXT("owned"));
	Out.Cap = TCWR::Field(World, TEXT("cap"));
	if (const TArray<TSharedPtr<FJsonValue>>* Regions = TCWR::Array(World.TryGetField(TEXT("regions"))))
	{
		for (const TSharedPtr<FJsonValue>& V : *Regions)
		{
			if (const FJsonObject* O = TCWR::Object(V)) Out.Regions.Add(TCWR::ParseRegion(*O));
		}
	}
	if (const TArray<TSharedPtr<FJsonValue>>* Stations = TCWR::Array(World.TryGetField(TEXT("stations"))))
	{
		Out.bStations = true;
		for (const TSharedPtr<FJsonValue>& V : *Stations)
		{
			const FJsonObject* O = TCWR::Object(V);
			if (!O) continue;
			FTCWorldStation S;
			S.Region = TCWR::Field(*O, TEXT("region")); S.X = TCWR::Field(*O, TEXT("x")); S.Z = TCWR::Field(*O, TEXT("z"));
			Out.Stations.Add(S);
		}
	}
	if (const FJsonObject* Recovery = TCWR::Object(World.TryGetField(TEXT("recovery"))))
	{
		Out.bRecovery = true;
		Out.bRecoveryAvailable = TCWR::Truthy(Recovery->TryGetField(TEXT("available")));
		Out.RecoverHqCost = TCWR::Field(*Recovery, TEXT("hq"));
		Out.RecoverEngineerCost = TCWR::Field(*Recovery, TEXT("engineer"));
		Out.RecoverHqText = TCWR::Text(Recovery->TryGetField(TEXT("hq")));
		Out.RecoverEngineerText = TCWR::Text(Recovery->TryGetField(TEXT("engineer")));
	}
}

void FTCWorldRecovery::ApplyStart(const FJsonObject& M)
{
	bStarted = true;
	bHasSnapshot = bHasPrevious = false; // client/main.js startGame: lastSnap = null
	Current = FTCWorldSnapshot();
	Previous = FTCWorldSnapshot();
	const double You = TCWR::Field(M, TEXT("you"));
	Me = FMath::IsFinite(You) ? (int32)You : -1;
	// teams = m.teams ?? names.map((_, i) => i)
	Teams.Reset();
	const TSharedPtr<FJsonValue> TeamsField = M.TryGetField(TEXT("teams"));
	if (!TCWR::Nullish(TeamsField))
	{
		if (const TArray<TSharedPtr<FJsonValue>>* List = TCWR::Array(TeamsField)) for (const TSharedPtr<FJsonValue>& V : *List) Teams.Add(TCWR::Num(V));
	}
	else if (const TArray<TSharedPtr<FJsonValue>>* Names = TCWR::Array(M.TryGetField(TEXT("names"))))
	{
		for (int32 I = 0; I < Names->Num(); I++) Teams.Add(I);
	}
	NameCount = 0;
	if (const TArray<TSharedPtr<FJsonValue>>* Names = TCWR::Array(M.TryGetField(TEXT("names")))) NameCount = Names->Num();
	// client/alert-history.js start(id): the same match id keeps its last tick (a reconnect replays nothing)
	const TSharedPtr<FJsonValue> IdField = M.TryGetField(TEXT("matchId"));
	const double Id = TCWR::Num(IdField);
	if (TCWR::Nullish(IdField) || !bAlertMatch || !(Id == AlertMatch))
	{
		bAlertMatch = !TCWR::Nullish(IdField);
		AlertMatch = Id;
		LastAlertTick = -1;
		PendingAlerts.Reset();
	}
}

void FTCWorldRecovery::Clear()
{
	bStarted = bHasSnapshot = bHasPrevious = false;
	Current = FTCWorldSnapshot();
	Previous = FTCWorldSnapshot();
	PendingAlerts.Reset();
}

void FTCWorldRecovery::ApplySnapshot(const FJsonObject& M, const FTCMatchState& State)
{
	if (!bStarted) return;
	FTCWorldSnapshot Next;
	if (const FJsonObject* Mode = TCWR::Object(M.TryGetField(TEXT("mode")))) Mode->TryGetStringField(TEXT("kind"), Next.ModeKind);
	Next.Mp = TCWR::Field(M, TEXT("mp"));
	Next.Tick = TCWR::Field(M, TEXT("tick"));
	Next.bWinner = !TCWR::Nullish(M.TryGetField(TEXT("winner")));
	if (const TArray<TSharedPtr<FJsonValue>>* Out = TCWR::Array(M.TryGetField(TEXT("out")))) for (const TSharedPtr<FJsonValue>& V : *Out) Next.Out.Add(TCWR::Truthy(V));
	const FJsonObject* World = TCWR::Object(M.TryGetField(TEXT("world")));
	if (World) ParseWorld(*World, Next);
	// s.home ?? s.world?.home, used when truthy (an [x, z] pair)
	TSharedPtr<FJsonValue> HomeField = M.TryGetField(TEXT("home"));
	if (TCWR::Nullish(HomeField) && World) HomeField = World->TryGetField(TEXT("home"));
	if (const TArray<TSharedPtr<FJsonValue>>* Home = TCWR::Array(HomeField))
	{
		Next.bHome = true;
		Next.Home = FVector2D(Home->IsValidIndex(0) ? TCWR::Num((*Home)[0]) : NAN, Home->IsValidIndex(1) ? TCWR::Num((*Home)[1]) : NAN);
	}

	bHasPrevious = bHasSnapshot;
	if (bHasSnapshot) Previous = MoveTemp(Current);
	Current = MoveTemp(Next);
	bHasSnapshot = true;
	RaiseAlerts(State);
}

double FTCWorldRecovery::TeamOf(double Slot) const
{
	// teams[slot] ?? slot
	const int32 I = TCWR::IndexOf(Slot, Teams.Num());
	return I >= 0 && FMath::IsFinite(Teams[I]) ? Teams[I] : Slot;
}

FColor FTCWorldRecovery::RegionTeamColour(double Team) const
{
	// regionTeamColor: team < 0 ? 0xaaaaaa : COLORS[teams.indexOf(team)] ?? 0xaaaaaa
	if (Team < 0) return NeutralTeamColour;
	const int32 First = Teams.IndexOfByPredicate([Team](double T) { return T == Team; });
	return First >= 0 && First < TCWR::ColourCount ? TCWR::Rgb(TCWR::Colours[First]) : NeutralTeamColour;
}

const FTCWorldRegion* FTCWorldRecovery::FindRegion(double Id) const
{
	for (const FTCWorldRegion& R : Current.Regions) if (R.Id == Id) return &R;
	return nullptr;
}

const FTCWorldRegion* FTCWorldRecovery::RegionAt(double X, double Z) const
{
	for (const FTCWorldRegion& R : Current.Regions) if (R.Contains(X, Z)) return &R;
	return nullptr;
}

FTCWorldScoreLead FTCWorldRecovery::ScoreLead() const
{
	FTCWorldScoreLead L;
	if (!bHasSnapshot || Current.ModeKind != TEXT("world")) return L;
	const FTCWorldSnapshot& S = Current;
	L.bShown = true;
	L.Mode = TEXT("World Conquest");
	const double Owned = FMath::IsNaN(S.Owned) ? 0 : S.Owned;
	const double Goal = !FMath::IsNaN(S.Goal) ? S.Goal : !FMath::IsNaN(S.Total) ? S.Total : 0;
	L.Clock = FString::Printf(TEXT("%s/%s regions"), *TCWR::Js(Owned), *TCWR::Js(Goal));
	L.Title = S.Goal < S.Total
		? FString::Printf(TEXT("Own %s of the %s regions, or be the last nation standing. Destroy military bases, then claim with infantry"), *TCWR::Js(S.Goal), *TCWR::Js(S.Total))
		: FString(TEXT("Your team must own every region. Destroy military bases, then claim with infantry"));
	return L;
}

TArray<FTCWorldTeamScore> FTCWorldRecovery::TeamScores() const
{
	TArray<FTCWorldTeamScore> Out;
	if (!bHasSnapshot || Current.ModeKind != TEXT("world")) return Out;
	const FTCWorldSnapshot& S = Current;
	const double Mine = MyTeam();
	const double Total = !FMath::IsNaN(S.Goal) ? S.Goal : !FMath::IsNaN(S.Total) ? S.Total : 0;
	// tids = [...new Set(teams)]; members(t): the seats (names' indices) whose teams entry is t
	for (const double T : Teams)
	{
		if (Out.ContainsByPredicate([T](const FTCWorldTeamScore& E) { return TCWR::SameKey(E.Team, T); })) continue;
		FTCWorldTeamScore& B = Out.AddDefaulted_GetRef();
		B.Team = T;
		for (int32 I = 0; I < NameCount; I++) if (Teams.IsValidIndex(I) && Teams[I] == T) B.Members.Add(I);
		for (const int32 Seat : B.Members)
		{
			const bool bOut = S.Out.IsValidIndex(Seat) && S.Out[Seat];
			B.Held.Add(bOut ? TEXT("Out") : TEXT(""));
			B.HeldDanger.Add(bOut);
		}
		B.Count = T == Mine ? (FMath::IsNaN(S.Owned) ? 0 : S.Owned) : (double)S.Regions.FilterByPredicate([T](const FTCWorldRegion& R) { return R.Team == T; }).Num();
		B.Total = Total;
		B.Fraction = Total != 0 && !FMath::IsNaN(Total) ? B.Count / Total : 0;
		B.Number = FString::Printf(TEXT("%s / %s"), *TCWR::Js(B.Count), *TCWR::Js(Total));
		B.Unit = TEXT("regions");
		B.Label = T == Mine ? TEXT("Owned") : TEXT("Discovered");
		B.Colour = SlotColour(B.Members.Num() ? B.Members[0] : NAN);
	}
	return Out;
}

FTCWorldRegionView FTCWorldRecovery::RegionView(int32 Index) const
{
	FTCWorldRegionView V;
	if (!Current.Regions.IsValidIndex(Index)) return V;
	const FTCWorldRegion& R = Current.Regions[Index];
	V.Index = Index;
	// world-regions.js: `${r.name} (${r.kind}): ${r.team === team() ? 'owned' : r.locked ? 'destroy military base' : 'infantry can claim'}`
	V.Text = FString::Printf(TEXT("%s (%s): %s"), *R.Name, *R.Kind, R.Team == MyTeam() ? TEXT("owned") : R.bLocked ? TEXT("destroy military base") : TEXT("infantry can claim"));
	V.Name = R.Name;
	V.At = FVector2D(R.X, R.Z);
	V.TeamColour = RegionTeamColour(R.Team);
	// cp.set(r.team >= 0 ? colour : null, r.capper >= 0 ? slotColor(r.capper) : null, r.progress ?? 0, !!r.contested)
	V.bOwner = R.Team >= 0;
	V.bCapper = R.Capper >= 0;
	V.CapperColour = V.bCapper ? SlotColour(R.Capper) : FColor::White;
	V.Progress = FMath::IsNaN(R.Progress) ? 0 : R.Progress;
	V.bContested = R.bContested;
	V.Arc = TCWorldRecoveryCore::RingArc(V.bOwner, V.bCapper, V.Progress);
	V.ArcColour = V.bOwner ? V.TeamColour : V.CapperColour;
	V.bLocked = R.bLocked;
	if (R.bRuns)
	{
		const double C = TCWorldRecoveryCore::CellMetres;
		for (const TCWorldRecoveryCore::FEdge& E : TCWorldRecoveryCore::TerritoryEdges(R.Runs.GetData(), R.Runs.Num()))
			V.Segments.Emplace(FVector2D(E.X0 * C, E.Y0 * C), FVector2D(E.X1 * C, E.Y1 * C));
	}
	else if (R.bBounds)
	{
		const double* B = R.Bounds;
		const FVector2D Corners[4] = { FVector2D(B[0], B[1]), FVector2D(B[2], B[1]), FVector2D(B[2], B[3]), FVector2D(B[0], B[3]) };
		for (int32 K = 0; K < 4; K++)
		{
			const FVector2D A = Corners[K], To = Corners[(K + 1) % 4];
			const double Len = std::hypot(To.X - A.X, To.Y - A.Y);
			const double Steps = FMath::Max(1.0, std::ceil(Len / TCWorldRecoveryCore::BoundsStep));
			const int32 N = FMath::IsFinite(Steps) ? (int32)Steps : 1;
			for (int32 I = 0; I < N; I++) V.Loop.Add(A + (To - A) * ((double)I / N));
		}
	}
	return V;
}

TArray<FTCWorldRegionView> FTCWorldRecovery::RegionViews() const
{
	TArray<FTCWorldRegionView> Out;
	if (!bHasSnapshot) return Out;
	for (int32 I = 0; I < Current.Regions.Num(); I++) Out.Add(RegionView(I));
	return Out;
}

TArray<FTCWorldStationView> FTCWorldRecovery::StationViews() const
{
	TArray<FTCWorldStationView> Out;
	if (!bHasSnapshot) return Out;
	for (const FTCWorldStation& St : Current.Stations)
	{
		// regionTeamColor(regions.find(q => q.id === st.region)?.team ?? -1)
		const FTCWorldRegion* R = FindRegion(St.Region);
		FTCWorldStationView V;
		V.At = FVector2D(St.X, St.Z);
		V.Fill = RegionTeamColour(R && !FMath::IsNaN(R->Team) ? R->Team : -1);
		Out.Add(V);
	}
	return Out;
}

void FTCWorldRecovery::RaiseAlerts(const FTCMatchState& State)
{
	// client/main.js: alerts are a commander's (!operative && !watching && !s.out?.[me]); this client also waits for
	// its seat to be confirmed, as the other native alert sources do.
	const FTCWorldSnapshot& S = Current;
	const bool bOutNow = S.Out.IsValidIndex(Me) && S.Out[Me];
	if (State.bSpectator || !State.SeatConfirmed() || bOutNow) return;
	// alert-history.js acceptTick: a duplicate or older tick raises nothing
	if (!FMath::IsFinite(S.Tick) || S.Tick <= LastAlertTick) return;
	LastAlertTick = S.Tick;
	if (S.bWinner) return;
	if (!bHasPrevious) return; // the first snapshot after a start has no baseline
	if (!S.bWorld) return;
	auto States = [](const TArray<FTCWorldRegion>& Regions)
	{
		std::vector<TCWorldRecoveryCore::FRegionState> Out;
		Out.reserve(Regions.Num());
		for (const FTCWorldRegion& R : Regions) Out.push_back({ R.Id, R.X, R.Z, R.Facts() });
		return Out;
	};
	const std::vector<TCWorldRecoveryCore::FRegionState> Was = States(Previous.Regions), Now = States(S.Regions);
	const std::vector<TCWorldRecoveryCore::FRaised> Raised = TCWorldRecoveryCore::RegionAlerts(Was.data(), (int)Was.size(), Now.data(), (int)Now.size(),
		MyTeam(), [this](double Slot) { return Friend(Slot); });
	for (const TCWorldRecoveryCore::FRaised& Hit : Raised)
	{
		const FTCWorldRegion& R = S.Regions[Hit.Index];
		FTCWorldAlert& Alert = PendingAlerts.AddDefaulted_GetRef();
		Alert.X = R.X; Alert.Z = R.Z; Alert.Tick = S.Tick;
		switch (Hit.Alert)
		{
		case TCWorldRecoveryCore::EAlert::Lost: Alert.Kind = ETCWorldAlertKind::PointLost; Alert.Text = TEXT("Region lost: ") + R.Name; break;
		case TCWorldRecoveryCore::EAlert::Claimed: Alert.Kind = ETCWorldAlertKind::PointWon; Alert.Text = TEXT("Region claimed: ") + R.Name; break;
		case TCWorldRecoveryCore::EAlert::Contested: Alert.Kind = ETCWorldAlertKind::Attack; Alert.Text = TEXT("Claim contested: ") + R.Name; break;
		default: Alert.Kind = ETCWorldAlertKind::UnderAttack; Alert.Text = TEXT("Region under attack: ") + R.Name; break;
		}
	}
}

TArray<FTCWorldAlert> FTCWorldRecovery::TakeAlerts()
{
	TArray<FTCWorldAlert> Out = MoveTemp(PendingAlerts);
	PendingAlerts.Reset();
	return Out;
}

FTCWorldRecoveryCard FTCWorldRecovery::Card(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	using TCWorldRecoveryCore::ECard;
	FTCWorldRecoveryCard C;
	// client/hud.js selUnits: the selected ids this client holds a row for
	TArray<const FTCUnit*> Sel;
	for (const int32 Id : Selected) if (const FTCUnit* U = State.Units.Find(Id)) Sel.Add(U);
	const bool bAllTrucks = Sel.Num() > 0 && !Sel.ContainsByPredicate([](const FTCUnit* U) { return U->Type != TEXT("truck"); });
	// UNITS[t].building, from the recruit table generated at the same pin
	const FTCRecruitDef* Def = Sel.Num() == 1 ? FTCRecruitment::Find(Sel[0]->Type) : nullptr;
	const bool bOwnBuilding = Def && Def->bBuilding && Sel[0]->Owner == Me;
	const bool bOwnEngineer = !bOwnBuilding && Sel.ContainsByPredicate([this](const FTCUnit* U) { return U->Owner == Me && U->Type == TEXT("engineer"); });
	const bool bConstruction = Current.ModeKind == TEXT("classic") || Current.ModeKind == TEXT("world");
	C.Decision = TCWorldRecoveryCore::CardDecision(bHasSnapshot, bConstruction, Sel.Num(), bAllTrucks, bOwnBuilding, bOwnEngineer,
		Current.bRecovery && Current.bRecoveryAvailable);
	if (C.Decision != ECard::Shown) return C;
	C.bShown = true;
	// missingHQ: no own HQ among the rows held (a site counts, as in the browser)
	bool bHq = false;
	for (const TPair<int32, FTCUnit>& P : State.Units) if (P.Value.Owner == Me && P.Value.Type == TEXT("hq")) { bHq = true; break; }
	C.bMissingHq = !bHq;
	C.CostValue = C.bMissingHq ? Current.RecoverHqCost : Current.RecoverEngineerCost;
	const FString CostText = C.bMissingHq ? Current.RecoverHqText : Current.RecoverEngineerText;
	C.Name = C.bMissingHq ? TEXT("Restore HQ") : TEXT("Restore Engineer");
	C.Cost = CostText + TEXT(" MP");
	C.Sub = TEXT("Deploys in friendly territory");
	C.bDisabled = Current.Mp < C.CostValue; // NaN on either side: enabled, as s.mp < undefined is false
	C.Key = FString::Printf(TEXT("recover:%s:%s"), C.bMissingHq ? TEXT("true") : TEXT("false"), *CostText);
	return C;
}

TSharedRef<FJsonObject> FTCWorldRecovery::RecoverMessage()
{
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("recover"));
	return M;
}

FTCWorldRecoverResult FTCWorldRecovery::Recover(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	FTCWorldRecoverResult R;
	R.bNative = true; // every refusal below is this client's: the browser has no button to press, or sends nothing
	if (FCString::Strcmp(FTCRecruitment::ReferencePin, ReferencePin) != 0)
	{
		R.Reason = FString::Printf(TEXT("The building table is for reference %s, World recovery for %s"), FTCRecruitment::ReferencePin, ReferencePin);
		return R;
	}
	const FTCWorldRecoveryCard C = Card(State, Selected);
	if (!C.bShown) { R.Reason = TEXT("No Restore card is shown"); return R; }
	if (C.bDisabled)
	{
		R.Reason = FString::Printf(TEXT("The %s button is disabled: %s MP is below its %s"), *C.Name, *TCWR::Js(Current.Mp), *C.Cost);
		return R;
	}
	// client/main.js sendCmd: an observer (watching, or this seat out) sends nothing during play
	if (State.bSpectator || (Current.Out.IsValidIndex(Me) && Current.Out[Me])) { R.Reason = TEXT("You are spectating"); return R; }
	if (!State.SeatConfirmed()) { R.Reason = TEXT("Waiting for the lobby to confirm this seat"); return R; }
	R.bNative = false;
	R.bSend = true;
	R.Message = RecoverMessage();
	return R;
}
