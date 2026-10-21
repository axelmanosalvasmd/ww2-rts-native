#include "Client/TCProgression.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"

const TCHAR* const FTCProgression::ReferencePin = TEXT("e4e579a");

const TCHAR* LexToString(ETCTechKind Kind)
{
	switch (Kind)
	{
	case ETCTechKind::Inf: return TEXT("inf");
	case ETCTechKind::Guns: return TEXT("guns");
	case ETCTechKind::Armor: return TEXT("armor");
	default: return TEXT("tier");
	}
}

// Named, not anonymous: the module builds as one unity file, where other files' anonymous helpers would collide.
namespace TCProg
{
	// shared/tech.js TECH at FTCProgression::ReferencePin. tiers: the price of reaching tier 2 and tier 3; levels: the
	// price of the next level of a line, indexed by the line's current level. mun is charged only in Classic and World.
	struct FPrice { int32 Mp, Mun, Time; };
	constexpr FPrice TierPrices[] = { { 250, 50, 60 }, { 400, 100, 90 } };
	constexpr FPrice LevelPrices[] = { { 100, 25, 30 }, { 175, 50, 45 }, { 250, 75, 60 } };
	// TECH.lines, in key order (Object.keys: inf, guns, armor)
	const TCHAR* const LineNames[] = { TEXT("Infantry Weapons"), TEXT("Vehicle Guns"), TEXT("Vehicle Armor") };
	constexpr ETCTechKind Lines[] = { ETCTechKind::Inf, ETCTechKind::Guns, ETCTechKind::Armor };
	int32 LineIndex(ETCTechKind Kind) { return FMath::Clamp((int32)Kind - 1, 0, 2); }

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
	// Math.round: halves go up
	double JsRound(double V)
	{
		const double R = FMath::FloorToDouble(V);
		return V - R >= 0.5 ? R + 1 : R;
	}
	// TIER_NAMES[n]: an index past the list (or not a whole number) is undefined, which a template string prints
	FString TierName(double N)
	{
		if (N >= 0 && N == FMath::FloorToDouble(N) && N <= FTCProgression::TopTier) return FTCRecruitment::TierName((int32)N);
		return TEXT("undefined");
	}

	FTCRecruitCheck Yes() { FTCRecruitCheck C; C.bOk = true; return C; }
	FTCRecruitCheck No(const FString& Reason, bool bNative = false) { FTCRecruitCheck C; C.Reason = Reason; C.bNative = bNative; return C; }
}

bool FTCProgression::ParseKind(const FString& Name, ETCTechKind& Out)
{
	for (const ETCTechKind K : { ETCTechKind::Tier, ETCTechKind::Inf, ETCTechKind::Guns, ETCTechKind::Armor })
	{
		if (Name.Equals(LexToString(K), ESearchCase::CaseSensitive)) { Out = K; return true; }
	}
	return false;
}

const TCHAR* FTCProgression::LineName(ETCTechKind Kind)
{
	return IsLine(Kind) ? TCProg::LineNames[TCProg::LineIndex(Kind)] : TEXT("");
}

FString FTCProgression::Tooltip(ETCTechKind Kind)
{
	// client/hud.js drawTech b.title
	switch (Kind)
	{
	case ETCTechKind::Tier: return TEXT("The next HQ tier unlocks new buildings and units. Research pauses while you have no finished HQ");
	case ETCTechKind::Armor: return FString::Printf(TEXT("%d%% less damage taken by vehicles per level, for units already in the field too"), StepPercent);
	case ETCTechKind::Guns: return FString::Printf(TEXT("+%d%% damage for vehicles, planes and boats per level, for units already in the field too"), StepPercent);
	default: return FString::Printf(TEXT("+%d%% damage for infantry and their guns per level, for units already in the field too"), StepPercent);
	}
}

TSharedRef<FJsonObject> FTCProgression::Message(ETCTechKind Kind)
{
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("tech"));
	M->SetStringField(TEXT("kind"), LexToString(Kind));
	return M;
}

void FTCProgression::SetUnitTableReference(const FString& Reference)
{
	UnitTableReference = Reference;
	bReferenceMatches = Reference == ReferencePin;
}

void FTCProgression::Clear()
{
	// the match's facts only: the announcement baseline lives on, as client/hud.js techSeen does across matches
	bStarted = bHasSnapshot = false;
	SnapMp.Reset(); SnapMun.Reset(); bMunField = false;
	ModeKind.Reset(); bSuddenDeath = false; ModeSlot.Reset();
	OutSeats.Reset();
	bTech = bTechUnreadable = false;
	TechTier = 0;
	for (double& L : Armory) L = 0;
	LabJobs.Reset();
}

void FTCProgression::ResetAnnouncements()
{
	bSeen = false;
	SeenTier = 0;
	for (double& L : SeenArmory) L = 0;
	Announcements.Reset();
}

TArray<FString> FTCProgression::TakeAnnouncements()
{
	TArray<FString> Out = MoveTemp(Announcements);
	Announcements.Reset();
	return Out;
}

void FTCProgression::ApplyStart(const FJsonObject& M)
{
	// client/main.js start: lastSnap = null. Nothing in the start itself is read here (seats and teams are
	// FTCMatchState's).
	Clear();
	bStarted = true;
}

void FTCProgression::ApplySnapshot(const FJsonObject& M)
{
	if (!bStarted) return;
	bHasSnapshot = true;
	double D;
	SnapMp.Reset(); if (TCProg::NumField(M, TEXT("mp"), D)) SnapMp = D;     // snapshotFor always sends mp (Math.floor of the wallet)
	SnapMun.Reset(); if (TCProg::NumField(M, TEXT("mun"), D)) SnapMun = D;  // Classic and World only
	bMunField = M.HasField(TEXT("mun"));

	ModeKind.Reset(); bSuddenDeath = false; ModeSlot.Reset();
	const TSharedPtr<FJsonObject>* Obj;
	if (M.TryGetObjectField(TEXT("mode"), Obj) && Obj->IsValid())
	{
		(*Obj)->TryGetStringField(TEXT("kind"), ModeKind);
		(*Obj)->TryGetBoolField(TEXT("suddenDeath"), bSuddenDeath);
		if (TCProg::NumField(**Obj, TEXT("slot"), D)) ModeSlot = D;
	}
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	OutSeats.Reset();
	if (M.TryGetArrayField(TEXT("out"), Arr)) for (const TSharedPtr<FJsonValue>& V : *Arr) OutSeats.Add(V.IsValid() && V->Type == EJson::Boolean && V->AsBool());

	// tech: { tier, armory: { inf, guns, armor }, lab: [[kind, progress], ...] } in a match with tech, else absent. A
	// tech value of another shape is held as unreadable: the panel stays hidden and research is refused natively.
	bTech = bTechUnreadable = false;
	TechTier = 0;
	for (double& L : Armory) L = 0;
	LabJobs.Reset();
	const TSharedPtr<FJsonValue> TechValue = M.TryGetField(TEXT("tech"));
	if (TechValue.IsValid() && TechValue->Type != EJson::Null)
	{
		bTech = true;
		const TSharedPtr<FJsonObject>* T = nullptr;
		const TSharedPtr<FJsonObject>* LineLevels = nullptr;
		const TArray<TSharedPtr<FJsonValue>>* LabRows = nullptr;
		bool bRead = TechValue->TryGetObject(T) && T && T->IsValid() && TCProg::NumField(**T, TEXT("tier"), TechTier)
			&& (*T)->TryGetObjectField(TEXT("armory"), LineLevels) && LineLevels && LineLevels->IsValid()
			&& TCProg::NumField(**LineLevels, TEXT("inf"), Armory[0]) && TCProg::NumField(**LineLevels, TEXT("guns"), Armory[1]) && TCProg::NumField(**LineLevels, TEXT("armor"), Armory[2])
			&& (*T)->TryGetArrayField(TEXT("lab"), LabRows) && LabRows;
		if (bRead)
		{
			for (const TSharedPtr<FJsonValue>& V : *LabRows)
			{
				const TArray<TSharedPtr<FJsonValue>>* Row = nullptr;
				FTCResearchJob Entry;
				if (!V.IsValid() || !V->TryGetArray(Row) || !Row || Row->Num() < 2 || !(*Row)[0].IsValid() || (*Row)[0]->Type != EJson::String || !TCProg::Num((*Row)[1], Entry.Progress))
				{
					bRead = false;
					break;
				}
				Entry.Kind = (*Row)[0]->AsString();
				LabJobs.Add(MoveTemp(Entry));
			}
		}
		if (!bRead)
		{
			bTechUnreadable = true;
			TechTier = 0;
			for (double& L : Armory) L = 0;
			LabJobs.Reset();
		}
	}

	// client/hud.js drawTech: announce a higher tier, then each line that went up (TECH.lines order), against the last
	// snapshot's; a snapshot without tech clears what was seen
	const bool bReadable = bTech && !bTechUnreadable;
	if (bReadable && bSeen)
	{
		if (TechTier > SeenTier) Announcements.Add(TCProg::TierName(TechTier) + TEXT(" ready"));
		for (int32 I = 0; I < 3; I++)
		{
			if (Armory[I] > SeenArmory[I]) Announcements.Add(FString::Printf(TEXT("%s %s researched"), TCProg::LineNames[I], *TCProg::JsNumber(Armory[I])));
		}
	}
	bSeen = bReadable;
	SeenTier = bReadable ? TechTier : 0;
	for (int32 I = 0; I < 3; I++) SeenArmory[I] = bReadable ? Armory[I] : 0;
}

bool FTCProgression::Classic() const { return ModeKind == TEXT("classic") || ModeKind == TEXT("world"); }

bool FTCProgression::Allied(const FTCMatchState& State, int32 A, int32 B) const
{
	// shared/sim.js allied over the browser's players (teams.map(team => ({ team })), teams defaulting to the seat)
	if (ModeKind == TEXT("world") && A == -1 && B == -1) return true;
	const int32 Seats = State.Teams.Num() ? State.Teams.Num() : State.Names.Num();
	if (A < 0 || B < 0 || A >= Seats || B >= Seats) return false;
	const int32 TA = State.Teams.Num() ? State.Teams[A] : A, TB = State.Teams.Num() ? State.Teams[B] : B;
	return TA == TB;
}

bool FTCProgression::ProductionAccess(const FTCMatchState& State, int32 Owner) const
{
	// shared/sim.js productionAccess: your own, or in Horde an ally's (never the Horde seat's)
	const int32 Slot = State.You;
	if (Owner == Slot) return true;
	if (ModeKind != TEXT("horde")) return false;
	const bool bSlotIsHorde = ModeSlot.IsSet() && ModeSlot.GetValue() == Slot, bOwnerIsHorde = ModeSlot.IsSet() && ModeSlot.GetValue() == Owner;
	return !bSlotIsHorde && !bOwnerIsHorde && Allied(State, Owner, Slot);
}

double FTCProgression::LevelJs(ETCTechKind LineKind) const
{
	return IsLine(LineKind) ? Armory[TCProg::LineIndex(LineKind)] : 0;
}

int32 FTCProgression::Level(ETCTechKind LineKind) const
{
	return HasTech() ? (int32)LevelJs(LineKind) : 0;
}

const FTCResearchJob* FTCProgression::Job(ETCTechKind Kind) const
{
	// t.lab.find(([kind]) => kind === k): the first job of that kind
	const FString Name = LexToString(Kind);
	return LabJobs.FindByPredicate([&Name](const FTCResearchJob& J) { return J.Kind == Name; });
}

TOptional<FTCTechCost> FTCProgression::CostJs(ETCTechKind Kind, bool bClassic) const
{
	// shared/tech.js techCost: TECH.tiers[tier + 1] or TECH.levels[armory[kind]]; a key that is not there is null
	if (!bTech || bTechUnreadable) return {};
	const TCProg::FPrice* P = nullptr;
	if (Kind == ETCTechKind::Tier)
	{
		const double Next = TechTier + 1;
		for (int32 I = 0; I < (int32)UE_ARRAY_COUNT(TCProg::TierPrices); I++) if (Next == I + 2) P = &TCProg::TierPrices[I];
	}
	else
	{
		const double L = LevelJs(Kind);
		for (int32 I = 0; I < (int32)UE_ARRAY_COUNT(TCProg::LevelPrices); I++) if (L == I) P = &TCProg::LevelPrices[I];
	}
	if (!P) return {};
	FTCTechCost C;
	C.Mp = P->Mp; C.Mun = bClassic ? P->Mun : 0; C.Time = P->Time;
	return C;
}

TOptional<FTCTechCost> FTCProgression::NextCost(ETCTechKind Kind, bool bClassic) const
{
	return bHasSnapshot ? CostJs(Kind, bClassic) : TOptional<FTCTechCost>();
}

int32 FTCProgression::Labs(const FTCMatchState& State, ETCTechKind Kind) const
{
	// client/availability.js: facilities this seat has productionAccess to, of the lab's type, finished and standing
	const TCHAR* Type = IsLine(Kind) ? TEXT("armory") : TEXT("hq");
	int32 N = 0;
	for (const auto& Pair : State.Units)
	{
		const FTCUnit& U = Pair.Value;
		if (U.Type == Type && U.Built >= 1 && U.Hp > 0 && ProductionAccess(State, U.Owner)) N++;
	}
	return N;
}

FTCRecruitCheck FTCProgression::Gate(const FTCMatchState& State) const
{
	if (!bReferenceMatches)
	{
		return TCProg::No(FString::Printf(TEXT("Research data is for reference %s, the unit table is for %s"), ReferencePin,
			UnitTableReference.IsEmpty() ? TEXT("an unknown reference") : *UnitTableReference), true);
	}
	if (!bHasSnapshot) return TCProg::No(TEXT("Waiting for the army"));
	if (State.bSpectator || (OutSeats.IsValidIndex(State.You) && OutSeats[State.You])) return TCProg::No(TEXT("You are spectating"));
	if (!State.SeatConfirmed()) return TCProg::No(TEXT("Waiting for the lobby to confirm this seat"), true);
	return TCProg::Yes();
}

FTCRecruitCheck FTCProgression::CheckResearch(const FTCMatchState& State, ETCTechKind Kind) const
{
	const FTCRecruitCheck G = Gate(State);
	if (!G.bOk) return G;
	if (bTech && bTechUnreadable) return TCProg::No(TEXT("The research state in the last snapshot could not be read"), true);
	if (!bTech) return TCProg::No(TEXT("Unavailable in this mode"));
	const bool bClassic = Classic();
	if (bClassic && bSuddenDeath) return TCProg::No(TEXT("Not during Sudden Death"));
	const TOptional<FTCTechCost> Cost = CostJs(Kind, bClassic);
	if (!Cost.IsSet()) return TCProg::No(TEXT("Fully researched"));
	if (Job(Kind)) return TCProg::No(TEXT("Already researching"));
	const bool bLine = IsLine(Kind);
	// level 2 needs Company HQ, level 3 Battalion HQ
	if (bLine && LevelJs(Kind) >= TechTier) return TCProg::No(TEXT("Needs ") + TCProg::TierName(TechTier + 1));
	const int32 N = Labs(State, Kind);
	if (!N) return TCProg::No(bLine ? TEXT("Needs a finished Armory") : TEXT("Needs a finished HQ"));
	if (bLine && LabJobs.FilterByPredicate([](const FTCResearchJob& J) { return J.Kind != TEXT("tier"); }).Num() >= N) return TCProg::No(TEXT("Each Armory researches one line at a time"));
	// client/availability.js resources(s, mp, 0, mun): a missing mp compares false in JavaScript, a missing mun refuses
	if (SnapMp.IsSet() && SnapMp.GetValue() < Cost->Mp) return TCProg::No(FString::Printf(TEXT("Needs %d MP"), Cost->Mp));
	if (Cost->Mun && !(SnapMun.IsSet() && SnapMun.GetValue() >= Cost->Mun)) return TCProg::No(FString::Printf(TEXT("Needs %d munitions"), Cost->Mun));
	return TCProg::Yes();
}

FTCResearchPress FTCProgression::Research(const FTCMatchState& State, ETCTechKind Kind) const
{
	// client/hud.js drawTech onclick: attempt(action, () => { send(action); blip('recruit') }), else explain the reason
	FTCResearchPress R;
	R.Check = CheckResearch(State, Kind);
	if (!R.Check.bOk) return R;
	R.Message = Message(Kind);
	R.bBlip = true;
	return R;
}

FTCResearchPanel FTCProgression::Panel(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	FTCResearchPanel Out;
	if (!HasTech()) return Out;
	// sel: the selected ids this client holds a row for; exactly one, an own finished HQ or Armory
	const FTCUnit* Only = nullptr;
	int32 Count = 0;
	for (int32 Id : Selected) if (const FTCUnit* U = State.Units.Find(Id)) { Only = U; Count++; }
	if (Count != 1 || !(Only->Type == TEXT("hq") || Only->Type == TEXT("armory")) || Only->Owner != State.You || Only->Built < 1) return Out;
	Out.bShown = true;
	Out.Building = Only->Id;
	Out.BuildingType = Only->Type;
	Out.TierLabel = TCProg::TierName(TechTier);
	TArray<ETCTechKind> Kinds;
	if (Only->Type == TEXT("hq")) Kinds.Add(ETCTechKind::Tier);
	else for (const ETCTechKind K : TCProg::Lines) Kinds.Add(K);
	// the label prices Munitions when the snapshot carries mun at all (s.mun !== undefined), not by the mode as
	// availability does
	for (const ETCTechKind K : Kinds)
	{
		FTCResearchButton& B = Out.Buttons.AddDefaulted_GetRef();
		B.Kind = K;
		const TOptional<FTCTechCost> Cost = CostJs(K, bMunField);
		const FTCResearchJob* Running = Job(K);
		const FString What = K == ETCTechKind::Tier ? TEXT("Upgrade to ") + TCProg::TierName(TechTier + 1)
			: FString::Printf(TEXT("%s %s"), LineName(K), *TCProg::JsNumber(LevelJs(K) + 1));
		B.bMaxed = !Cost.IsSet();
		B.bResearching = Running != nullptr;
		B.Progress = Running ? Running->Progress : 0;
		if (!Cost.IsSet()) B.Text = K == ETCTechKind::Tier ? FString(TEXT("Top tier")) : FString::Printf(TEXT("%s %d/%d"), LineName(K), MaxLevel, MaxLevel);
		else if (Running) B.Text = What + TEXT(": ") + TCProg::JsNumber(TCProg::JsRound(Running->Progress * 100)) + TEXT("%");
		else
		{
			B.Text = What + FString::Printf(TEXT(": %d MP"), Cost->Mp) + (Cost->Mun ? FString::Printf(TEXT(" + %d Mun"), Cost->Mun) : FString())
				+ FString::Printf(TEXT(", %ds"), Cost->Time);
		}
		B.Tooltip = Tooltip(K);
		B.Check = CheckResearch(State, K);
	}
	return Out;
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCProgTest
{
	TSharedPtr<FJsonObject> Parse(const FString& Src)
	{
		TSharedPtr<FJsonObject> Obj;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Src), Obj);
		return Obj;
	}

	TArray<int32> Ints(const TCHAR* List)
	{
		TArray<FString> Parts;
		FString(List).ParseIntoArray(Parts, TEXT(","));
		TArray<int32> Out;
		for (const FString& P : Parts) Out.Add(FCString::Atoi(*P));
		return Out;
	}

	// a seat the lobby confirmed (or a spectator), seat 0, in a match being played
	void Seat(FTCMatchState& St, bool bWatching, const TArray<int32>& Teams)
	{
		St.bHasMatch = true; St.You = 0; St.LobbySeat = 0; St.bLobbySinceOpen = true; St.bSpectator = bWatching;
		St.RoomState = TEXT("play"); St.Winner = -2; St.Teams = Teams;
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
			U.Hp = (int32)R[7]->AsNumber(); U.Built = (float)R[14]->AsNumber();
			St.Units.Add(U.Id, U);
		}
	}

	FString Str(const TSharedPtr<FJsonObject>& M, const TCHAR* F) { FString S; if (M.IsValid()) M->TryGetStringField(F, S); return S; }

	const ETCTechKind Kinds[] = { ETCTechKind::Tier, ETCTechKind::Inf, ETCTechKind::Guns, ETCTechKind::Armor };
}

// Run (after a build): -ExecCmds="Automation RunTests ThreeCrossroads.Progression; Quit".
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCProgressionReferenceTest, "ThreeCrossroads.Progression.Reference", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCProgressionReferenceTest::RunTest(const FString& Parameters)
{
	// Generated from the reference at e4e579a by running client/hud.js drawTech (its source text, in jsdom) and
	// client/availability.js availability({ t: 'tech', kind }) for seat 0 on the snapshot and selection shown: whether
	// the panel shows, its tier label, its buttons' kinds and labels, and each kind's refusal (empty: available). Each
	// button's click was checked there to send { t: 'tech', kind } with the 'recruit' blip when available, and to show
	// the refusal otherwise; its tooltip to be the refusal, else the title below.
	struct FCase
	{
		const TCHAR* Name; const TCHAR* Snap; const TCHAR* Selected; const TCHAR* Teams; bool bWatching;
		bool bShown; const TCHAR* TierLabel; const TCHAR* ButtonKinds; const TCHAR* Texts; const TCHAR* Reasons[4]; // tier, inf, guns, armor
	};
	const FCase Cases[] = {
		{ TEXT("conquest HQ tier 1"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP, 60s"), { TEXT(""), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("conquest Armory tier 1"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("2"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("inf,guns,armor"), TEXT("Infantry Weapons 1: 100 MP, 30s|Vehicle Guns 1: 100 MP, 30s|Vehicle Armor 1: 100 MP, 30s"), { TEXT(""), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("Armory line at the tier"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":1,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("2"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("inf,guns,armor"), TEXT("Infantry Weapons 2: 175 MP, 45s|Vehicle Guns 1: 100 MP, 30s|Vehicle Armor 1: 100 MP, 30s"), { TEXT(""), TEXT("Needs Company HQ"), TEXT(""), TEXT("") } },
		{ TEXT("tier 2 lines"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":2,\"armory\":{\"inf\":2,\"guns\":1,\"armor\":0},\"lab\":[]}}"), TEXT("2"), TEXT("0,1"), false, true, TEXT("Company HQ"), TEXT("inf,guns,armor"), TEXT("Infantry Weapons 3: 250 MP, 60s|Vehicle Guns 2: 175 MP, 45s|Vehicle Armor 1: 100 MP, 30s"), { TEXT(""), TEXT("Needs Battalion HQ"), TEXT(""), TEXT("") } },
		{ TEXT("Armory maxed"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":3,\"armory\":{\"inf\":3,\"guns\":3,\"armor\":3},\"lab\":[]}}"), TEXT("2"), TEXT("0,1"), false, true, TEXT("Battalion HQ"), TEXT("inf,guns,armor"), TEXT("Infantry Weapons 3/3|Vehicle Guns 3/3|Vehicle Armor 3/3"), { TEXT("Fully researched"), TEXT("Fully researched"), TEXT("Fully researched"), TEXT("Fully researched") } },
		{ TEXT("top tier"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":3,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Battalion HQ"), TEXT("tier"), TEXT("Top tier"), { TEXT("Fully researched"), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("classic HQ short of mun"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"classic\"},\"mun\":10,\"fuel\":0,\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP + 50 Mun, 60s"), { TEXT("Needs 50 munitions"), TEXT("Needs 25 munitions"), TEXT("Needs 25 munitions"), TEXT("Needs 25 munitions") } },
		{ TEXT("classic HQ short of MP"), TEXT("{\"t\":\"s\",\"mp\":100,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"classic\"},\"mun\":999,\"fuel\":0,\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP + 50 Mun, 60s"), { TEXT("Needs 250 MP"), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("classic exact price"), TEXT("{\"t\":\"s\",\"mp\":250,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"classic\"},\"mun\":50,\"fuel\":0,\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP + 50 Mun, 60s"), { TEXT(""), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("classic without mun field"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"classic\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP, 60s"), { TEXT("Needs 50 munitions"), TEXT("Needs 25 munitions"), TEXT("Needs 25 munitions"), TEXT("Needs 25 munitions") } },
		{ TEXT("world Armory"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"world\"},\"mun\":999,\"fuel\":0,\"world\":{\"cap\":40},\"tech\":{\"tier\":2,\"armory\":{\"inf\":1,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("2"), TEXT("0,1"), false, true, TEXT("Company HQ"), TEXT("inf,guns,armor"), TEXT("Infantry Weapons 2: 175 MP + 50 Mun, 45s|Vehicle Guns 1: 100 MP + 25 Mun, 30s|Vehicle Armor 1: 100 MP + 25 Mun, 30s"), { TEXT(""), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("conquest with a mun field"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"mun\":0,\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP + 50 Mun, 60s"), { TEXT(""), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("tier researching"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[[\"tier\",0.3]]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 30%"), { TEXT("Already researching"), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("armor researching"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[[\"armor\",0.7]]}}"), TEXT("2"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("inf,guns,armor"), TEXT("Infantry Weapons 1: 100 MP, 30s|Vehicle Guns 1: 100 MP, 30s|Vehicle Armor 1: 70%"), { TEXT(""), TEXT("Each Armory researches one line at a time"), TEXT("Each Armory researches one line at a time"), TEXT("Already researching") } },
		{ TEXT("one Armory busy"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":2,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[[\"tier\",0.5],[\"inf\",0.1]]}}"), TEXT("2"), TEXT("0,1"), false, true, TEXT("Company HQ"), TEXT("inf,guns,armor"), TEXT("Infantry Weapons 1: 10%|Vehicle Guns 1: 100 MP, 30s|Vehicle Armor 1: 100 MP, 30s"), { TEXT("Already researching"), TEXT("Already researching"), TEXT("Each Armory researches one line at a time"), TEXT("Each Armory researches one line at a time") } },
		{ TEXT("two Armories"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1],[3,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":2,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[[\"inf\",0.1]]}}"), TEXT("2"), TEXT("0,1"), false, true, TEXT("Company HQ"), TEXT("inf,guns,armor"), TEXT("Infantry Weapons 1: 10%|Vehicle Guns 1: 100 MP, 30s|Vehicle Armor 1: 100 MP, 30s"), { TEXT(""), TEXT("Already researching"), TEXT(""), TEXT("") } },
		{ TEXT("Armory site"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,0.5]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":2,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("2"), TEXT("0,1"), false, false, TEXT(""), TEXT(""), TEXT(""), { TEXT(""), TEXT("Needs a finished Armory"), TEXT("Needs a finished Armory"), TEXT("Needs a finished Armory") } },
		{ TEXT("no HQ standing"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("2"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("inf,guns,armor"), TEXT("Infantry Weapons 1: 100 MP, 30s|Vehicle Guns 1: 100 MP, 30s|Vehicle Armor 1: 100 MP, 30s"), { TEXT("Needs a finished HQ"), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("no tech"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"}}"), TEXT("1"), TEXT("0,1"), false, false, TEXT(""), TEXT(""), TEXT(""), { TEXT("Unavailable in this mode"), TEXT("Unavailable in this mode"), TEXT("Unavailable in this mode"), TEXT("Unavailable in this mode") } },
		{ TEXT("sudden death"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"classic\",\"suddenDeath\":true},\"mun\":999,\"fuel\":0,\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP + 50 Mun, 60s"), { TEXT("Not during Sudden Death"), TEXT("Not during Sudden Death"), TEXT("Not during Sudden Death"), TEXT("Not during Sudden Death") } },
		{ TEXT("watching"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), true, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP, 60s"), { TEXT("You are spectating"), TEXT("You are spectating"), TEXT("You are spectating"), TEXT("You are spectating") } },
		{ TEXT("out"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[true,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP, 60s"), { TEXT("You are spectating"), TEXT("You are spectating"), TEXT("You are spectating"), TEXT("You are spectating") } },
		{ TEXT("enemy HQ"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1],[9,\"hq\",1,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("9"), TEXT("0,1"), false, false, TEXT(""), TEXT(""), TEXT(""), { TEXT(""), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("two selected"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1,2"), TEXT("0,1"), false, false, TEXT(""), TEXT(""), TEXT(""), { TEXT(""), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("horde ally labs"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[5,\"hq\",1,0,0,0,0,100,0,0,0,0,0,0,1],[6,\"armory\",1,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"horde\",\"slot\":2},\"tech\":{\"tier\":2,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("5"), TEXT("0,0,1"), false, false, TEXT(""), TEXT(""), TEXT(""), { TEXT(""), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("horde attacker labs"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[5,\"hq\",2,0,0,0,0,100,0,0,0,0,0,0,1],[6,\"armory\",2,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"horde\",\"slot\":2},\"tech\":{\"tier\":2,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT(""), TEXT("0,0,0"), false, false, TEXT(""), TEXT(""), TEXT(""), { TEXT("Needs a finished HQ"), TEXT("Needs a finished Armory"), TEXT("Needs a finished Armory"), TEXT("Needs a finished Armory") } },
		{ TEXT("conquest ally labs"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[5,\"hq\",1,0,0,0,0,100,0,0,0,0,0,0,1],[6,\"armory\",1,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":2,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT(""), TEXT("0,0"), false, false, TEXT(""), TEXT(""), TEXT(""), { TEXT("Needs a finished HQ"), TEXT("Needs a finished Armory"), TEXT("Needs a finished Armory"), TEXT("Needs a finished Armory") } },
		{ TEXT("dead HQ row"), TEXT("{\"t\":\"s\",\"mp\":999,\"out\":[false,false,false],\"units\":[[1,\"hq\",0,0,0,0,0,0,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1]],\"mode\":{\"kind\":\"conquest\"},\"tech\":{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}}"), TEXT("1"), TEXT("0,1"), false, true, TEXT("Platoon HQ"), TEXT("tier"), TEXT("Upgrade to Company HQ: 250 MP, 60s"), { TEXT("Needs a finished HQ"), TEXT(""), TEXT(""), TEXT("") } },
		{ TEXT("no snapshot"), nullptr, TEXT(""), TEXT("0,1"), false, false, TEXT(""), TEXT(""), TEXT(""), { TEXT("Waiting for the army"), TEXT("Waiting for the army"), TEXT("Waiting for the army"), TEXT("Waiting for the army") } },
	};
	// client/hud.js drawTech b.title, as the reference wrote them (tier, inf, guns, armor)
	const TCHAR* const Titles[] = {
		TEXT("The next HQ tier unlocks new buildings and units. Research pauses while you have no finished HQ"),
		TEXT("+10% damage for infantry and their guns per level, for units already in the field too"),
		TEXT("+10% damage for vehicles, planes and boats per level, for units already in the field too"),
		TEXT("10% less damage taken by vehicles per level, for units already in the field too"),
	};
	for (int32 I = 0; I < 4; I++) TestEqual(FString::Printf(TEXT("title of %s"), LexToString(TCProgTest::Kinds[I])), FTCProgression::Tooltip(TCProgTest::Kinds[I]), FString(Titles[I]));

	for (const FCase& C : Cases)
	{
		const FString Name(C.Name);
		FTCMatchState St;
		TCProgTest::Seat(St, C.bWatching, TCProgTest::Ints(C.Teams));
		FTCProgression P;
		P.SetUnitTableReference(FTCProgression::ReferencePin);
		P.ApplyStart(*TCProgTest::Parse(TEXT("{\"t\":\"start\"}")));
		if (C.Snap)
		{
			const TSharedPtr<FJsonObject> S = TCProgTest::Parse(C.Snap);
			if (!TestTrue(Name + TEXT(": snapshot parses"), S.IsValid())) continue;
			TCProgTest::Rows(St, *S);
			P.ApplySnapshot(*S);
		}
		TSet<int32> Sel;
		for (int32 Id : TCProgTest::Ints(C.Selected)) Sel.Add(Id);

		const FTCResearchPanel Shown = P.Panel(St, Sel);
		TestEqual(Name + TEXT(": panel shown"), Shown.bShown, C.bShown);
		if (Shown.bShown)
		{
			TestEqual(Name + TEXT(": tier label"), Shown.TierLabel, FString(C.TierLabel));
			TArray<FString> GotKinds, GotTexts;
			for (const FTCResearchButton& B : Shown.Buttons) { GotKinds.Add(LexToString(B.Kind)); GotTexts.Add(B.Text); }
			TestEqual(Name + TEXT(": buttons"), FString::Join(GotKinds, TEXT(",")), FString(C.ButtonKinds));
			TestEqual(Name + TEXT(": labels"), FString::Join(GotTexts, TEXT("|")), FString(C.Texts));
			for (const FTCResearchButton& B : Shown.Buttons)
			{
				const FString Want(C.Reasons[(int32)B.Kind]);
				TestEqual(Name + TEXT(": button availability ") + LexToString(B.Kind), B.Check.Reason, Want);
				TestEqual(Name + TEXT(": tooltip ") + LexToString(B.Kind), B.ShownTooltip(), Want.IsEmpty() ? FString(Titles[(int32)B.Kind]) : Want);
			}
		}
		for (int32 I = 0; I < 4; I++)
		{
			const FString Want(C.Reasons[I]), What = Name + TEXT(": ") + LexToString(TCProgTest::Kinds[I]);
			const FTCRecruitCheck Got = P.CheckResearch(St, TCProgTest::Kinds[I]);
			TestEqual(What + TEXT(" availability"), Got.Reason, Want);
			TestEqual(What + TEXT(" ok"), Got.bOk, Want.IsEmpty());
			TestFalse(What + TEXT(" is a browser sentence"), Got.bNative);
			const FTCResearchPress Press = P.Research(St, TCProgTest::Kinds[I]);
			if (Want.IsEmpty())
			{
				TestTrue(What + TEXT(" sends with the blip"), Press.Check.bOk && Press.bBlip && Press.Message.IsValid());
				TestEqual(What + TEXT(" message t"), TCProgTest::Str(Press.Message, TEXT("t")), FString(TEXT("tech")));
				TestEqual(What + TEXT(" message kind"), TCProgTest::Str(Press.Message, TEXT("kind")), FString(LexToString(TCProgTest::Kinds[I])));
			}
			else TestTrue(What + TEXT(" shows the refusal and sends nothing"), !Press.Check.bOk && !Press.bBlip && !Press.Message.IsValid() && Press.Check.Reason == Want);
		}
	}

	// client/hud.js drawTech's announcements over a run of Conquest snapshots on one page (null: a snapshot without tech)
	struct FStep { const TCHAR* Tech; const TCHAR* Announced; };
	const FStep Steps[] = {
		{ TEXT("{\"tier\":1,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}"), TEXT("") },
		{ TEXT("{\"tier\":2,\"armory\":{\"inf\":0,\"guns\":1,\"armor\":0},\"lab\":[]}"), TEXT("Company HQ ready|Vehicle Guns 1 researched") },
		{ TEXT("{\"tier\":2,\"armory\":{\"inf\":0,\"guns\":1,\"armor\":0},\"lab\":[]}"), TEXT("") },
		{ nullptr, TEXT("") },
		{ TEXT("{\"tier\":3,\"armory\":{\"inf\":1,\"guns\":1,\"armor\":1},\"lab\":[]}"), TEXT("") },
		{ TEXT("{\"tier\":3,\"armory\":{\"inf\":2,\"guns\":1,\"armor\":2},\"lab\":[]}"), TEXT("Infantry Weapons 2 researched|Vehicle Armor 2 researched") },
		{ TEXT("{\"tier\":2,\"armory\":{\"inf\":0,\"guns\":0,\"armor\":0},\"lab\":[]}"), TEXT("") },
	};
	FTCMatchState St;
	TCProgTest::Seat(St, false, { 0, 1 });
	FTCProgression P;
	P.SetUnitTableReference(FTCProgression::ReferencePin);
	P.ApplyStart(*TCProgTest::Parse(TEXT("{\"t\":\"start\"}")));
	int32 StepIndex = 0;
	for (const FStep& Step : Steps)
	{
		const FString Tech = Step.Tech ? FString(TEXT(",\"tech\":")) + Step.Tech : FString();
		P.ApplySnapshot(*TCProgTest::Parse(FString::Printf(TEXT("{\"t\":\"s\",\"mp\":999,\"mode\":{\"kind\":\"conquest\"},\"out\":[false,false],\"units\":[]%s}"), *Tech)));
		TestEqual(FString::Printf(TEXT("announcements after snapshot %d"), StepIndex++), FString::Join(P.TakeAnnouncements(), TEXT("|")), FString(Step.Announced));
	}
	return true;
}

// This client's own refusals and the parts the browser has no counterpart for.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCProgressionNativeTest, "ThreeCrossroads.Progression.Native", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCProgressionNativeTest::RunTest(const FString& Parameters)
{
	const TCHAR* Start = TEXT("{\"t\":\"start\"}");
	const TCHAR* Snap = TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":999,\"mun\":999,\"fuel\":0,\"out\":[false,false],")
		TEXT("\"tech\":{\"tier\":2,\"armory\":{\"inf\":1,\"guns\":0,\"armor\":2},\"lab\":[[\"guns\",0.4]]},")
		TEXT("\"units\":[[1,\"hq\",0,0,0,0,0,100,0,0,0,0,0,0,1],[2,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,1],[3,\"armory\",0,0,0,0,0,100,0,0,0,0,0,0,0.5]]}");
	auto Ready = [&](FTCMatchState& St, FTCProgression& P, const TCHAR* Body)
	{
		TCProgTest::Seat(St, false, { 0, 1 });
		P.SetUnitTableReference(FTCProgression::ReferencePin);
		P.ApplyStart(*TCProgTest::Parse(Start));
		const TSharedPtr<FJsonObject> S = TCProgTest::Parse(Body);
		TCProgTest::Rows(St, *S);
		P.ApplySnapshot(*S);
	};

	// the kinds and the wire message
	for (const ETCTechKind K : TCProgTest::Kinds)
	{
		ETCTechKind Back = ETCTechKind::Tier;
		TestTrue(FString::Printf(TEXT("'%s' parses back"), LexToString(K)), FTCProgression::ParseKind(LexToString(K), Back) && Back == K);
		const TSharedRef<FJsonObject> M = FTCProgression::Message(K);
		TestTrue(FString::Printf(TEXT("{ t: 'tech', kind: '%s' } and nothing else"), LexToString(K)), M->Values.Num() == 2 && TCProgTest::Str(M, TEXT("t")) == TEXT("tech") && TCProgTest::Str(M, TEXT("kind")) == LexToString(K));
	}
	ETCTechKind Unused = ETCTechKind::Tier;
	TestFalse(TEXT("kind names are case sensitive"), FTCProgression::ParseKind(TEXT("Tier"), Unused));
	TestFalse(TEXT("no kind for an empty name"), FTCProgression::ParseKind(FString(), Unused));
	TestEqual(TEXT("line names"), FString::Printf(TEXT("%s|%s|%s|%s"), FTCProgression::LineName(ETCTechKind::Tier), FTCProgression::LineName(ETCTechKind::Inf),
		FTCProgression::LineName(ETCTechKind::Guns), FTCProgression::LineName(ETCTechKind::Armor)), FString(TEXT("|Infantry Weapons|Vehicle Guns|Vehicle Armor")));

	{
		// the state the panel and the HUD read: Classic charges Munitions; the site does not count as a lab
		FTCMatchState St; FTCProgression P;
		Ready(St, P, Snap);
		TestTrue(TEXT("has tech"), P.HasTech());
		TestEqual(TEXT("tier"), P.Tier(), 2);
		TestTrue(TEXT("levels"), P.Level(ETCTechKind::Inf) == 1 && P.Level(ETCTechKind::Guns) == 0 && P.Level(ETCTechKind::Armor) == 2 && P.Level(ETCTechKind::Tier) == 0);
		TestTrue(TEXT("one job"), P.Lab().Num() == 1 && P.Job(ETCTechKind::Guns) && P.Job(ETCTechKind::Guns)->Progress == 0.4 && !P.Job(ETCTechKind::Inf));
		const TOptional<FTCTechCost> Tier3 = P.NextCost(ETCTechKind::Tier, true);
		TestTrue(TEXT("Battalion HQ: 400 MP, 100 Mun, 90 s"), Tier3.IsSet() && Tier3->Mp == 400 && Tier3->Mun == 100 && Tier3->Time == 90);
		const TOptional<FTCTechCost> Inf2 = P.NextCost(ETCTechKind::Inf, false);
		TestTrue(TEXT("Infantry Weapons 2 outside Classic: 175 MP, no Mun, 45 s"), Inf2.IsSet() && Inf2->Mp == 175 && Inf2->Mun == 0 && Inf2->Time == 45);
		TestTrue(TEXT("Vehicle Armor 3: 250 MP"), P.NextCost(ETCTechKind::Armor, true).IsSet() && P.NextCost(ETCTechKind::Armor, true)->Mp == 250);
		TestEqual(TEXT("one finished Armory"), P.Labs(St, ETCTechKind::Armor), 1);
		TestEqual(TEXT("one HQ"), P.Labs(St, ETCTechKind::Tier), 1);
		TestEqual(TEXT("the one Armory is busy"), P.CheckResearch(St, ETCTechKind::Inf).Reason, FString(TEXT("Each Armory researches one line at a time")));
		TestEqual(TEXT("armor 2 at Company HQ: the tier comes before the busy Armory"), P.CheckResearch(St, ETCTechKind::Armor).Reason, FString(TEXT("Needs Battalion HQ")));
		TestTrue(TEXT("the tier is open"), P.CheckResearch(St, ETCTechKind::Tier).bOk);
		const FTCResearchPanel Armory = P.Panel(St, TSet<int32>{ 2 });
		TestTrue(TEXT("Armory panel"), Armory.bShown && Armory.Building == 2 && Armory.BuildingType == TEXT("armory") && Armory.TierLabel == TEXT("Company HQ") && Armory.Buttons.Num() == 3);
		if (Armory.Buttons.Num() == 3)
		{
			TestEqual(TEXT("running line"), Armory.Buttons[1].Text, FString(TEXT("Vehicle Guns 1: 40%")));
			TestTrue(TEXT("running line state"), Armory.Buttons[1].bResearching && Armory.Buttons[1].Progress == 0.4 && !Armory.Buttons[1].bMaxed);
			TestEqual(TEXT("Classic label prices Munitions"), Armory.Buttons[2].Text, FString(TEXT("Vehicle Armor 3: 250 MP + 75 Mun, 60s")));
		}
		TestFalse(TEXT("no panel for an Armory site"), P.Panel(St, TSet<int32>{ 3 }).bShown);
		TestFalse(TEXT("no panel for an id not held"), P.Panel(St, TSet<int32>{ 77 }).bShown);
		TestTrue(TEXT("an id not held does not count"), P.Panel(St, TSet<int32>{ 1, 77 }).bShown);
		TestTrue(TEXT("the press sends"), P.Research(St, ETCTechKind::Tier).Message.IsValid());
	}
	{
		// another pin: refused natively, nothing sent; the panel still shows (so the HUD can say why)
		FTCMatchState St; FTCProgression P;
		Ready(St, P, Snap);
		P.SetUnitTableReference(TEXT("0000000"));
		const FTCRecruitCheck Pin = P.CheckResearch(St, ETCTechKind::Tier);
		TestTrue(TEXT("another pin refuses natively"), !Pin.bOk && Pin.bNative && Pin.Reason.Contains(TEXT("0000000")));
		TestFalse(TEXT("another pin sends nothing"), P.Research(St, ETCTechKind::Tier).Message.IsValid());
		const FTCResearchPanel Hq = P.Panel(St, TSet<int32>{ 1 });
		TestTrue(TEXT("the panel carries the native refusal"), Hq.bShown && Hq.Buttons.Num() == 1 && Hq.Buttons[0].Check.bNative);
	}
	{
		// a seat the lobby has not confirmed
		FTCMatchState St; FTCProgression P;
		Ready(St, P, Snap);
		St.bLobbySinceOpen = false;
		const FTCRecruitCheck Seat0 = P.CheckResearch(St, ETCTechKind::Tier);
		TestTrue(TEXT("unconfirmed seat refuses natively"), !Seat0.bOk && Seat0.bNative);
		TestFalse(TEXT("unconfirmed seat sends nothing"), P.Research(St, ETCTechKind::Tier).Message.IsValid());
	}
	{
		// a snapshot before any start is ignored; a start forgets the last snapshot
		FTCMatchState St; FTCProgression P;
		TCProgTest::Seat(St, false, { 0, 1 });
		P.SetUnitTableReference(FTCProgression::ReferencePin);
		P.ApplySnapshot(*TCProgTest::Parse(Snap));
		TestFalse(TEXT("no snapshot before a start"), P.HasSnapshot() || P.HasTech());
		TestEqual(TEXT("waiting before a start"), P.CheckResearch(St, ETCTechKind::Tier).Reason, FString(TEXT("Waiting for the army")));
		Ready(St, P, Snap);
		TestTrue(TEXT("held after a start"), P.HasTech());
		P.ApplyStart(*TCProgTest::Parse(Start));
		TestTrue(TEXT("a start forgets the snapshot"), !P.HasSnapshot() && !P.HasTech() && !P.NextCost(ETCTechKind::Tier, false).IsSet());
		P.ApplySnapshot(*TCProgTest::Parse(Snap));
		P.Clear();
		TestTrue(TEXT("Clear forgets the snapshot"), !P.HasSnapshot() && P.ReferenceMatches());
		TestEqual(TEXT("waiting after Clear"), P.CheckResearch(St, ETCTechKind::Tier).Reason, FString(TEXT("Waiting for the army")));
	}
	{
		// a tech value of another shape: refused natively, no panel, no announcement, and the baseline is dropped
		FTCMatchState St; FTCProgression P;
		Ready(St, P, Snap);
		P.TakeAnnouncements();
		const TCHAR* Bad[] = {
			TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":999,\"mun\":999,\"out\":[false,false],\"tech\":{\"tier\":\"3\",\"armory\":{\"inf\":1,\"guns\":0,\"armor\":2},\"lab\":[]},\"units\":[]}"),
			TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":999,\"mun\":999,\"out\":[false,false],\"tech\":{\"tier\":3,\"armory\":{\"inf\":1,\"guns\":0},\"lab\":[]},\"units\":[]}"),
			TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":999,\"mun\":999,\"out\":[false,false],\"tech\":{\"tier\":3,\"armory\":{\"inf\":1,\"guns\":0,\"armor\":2},\"lab\":[[1,0.5]]},\"units\":[]}"),
			TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":999,\"mun\":999,\"out\":[false,false],\"tech\":7,\"units\":[]}"),
		};
		int32 BadIndex = 0;
		for (const TCHAR* Body : Bad)
		{
			const FString What = FString::Printf(TEXT("unreadable tech %d"), BadIndex++);
			P.ApplySnapshot(*TCProgTest::Parse(Body));
			const FTCRecruitCheck Got = P.CheckResearch(St, ETCTechKind::Tier);
			TestTrue(What + TEXT(": refused natively"), !Got.bOk && Got.bNative);
			TestFalse(What + TEXT(": no tech"), P.HasTech());
			TestFalse(What + TEXT(": no panel"), P.Panel(St, TSet<int32>{ 1 }).bShown);
			TestEqual(What + TEXT(": nothing announced"), P.TakeAnnouncements().Num(), 0);
		}
		P.ApplySnapshot(*TCProgTest::Parse(TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"classic\"},\"mp\":999,\"mun\":999,\"out\":[false,false],\"tech\":{\"tier\":3,\"armory\":{\"inf\":3,\"guns\":3,\"armor\":3},\"lab\":[]},\"units\":[]}")));
		TestEqual(TEXT("after unreadable tech, the next readable one is a new baseline"), P.TakeAnnouncements().Num(), 0);
	}
	{
		// the baseline outlives a match, as the browser's HUD does; ResetAnnouncements is a page load
		FTCMatchState St; FTCProgression P;
		Ready(St, P, Snap); // tier 2
		P.TakeAnnouncements();
		P.Clear();
		P.ApplyStart(*TCProgTest::Parse(Start));
		P.ApplySnapshot(*TCProgTest::Parse(TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":0,\"out\":[false,false],\"tech\":{\"tier\":3,\"armory\":{\"inf\":1,\"guns\":0,\"armor\":2},\"lab\":[]},\"units\":[]}")));
		TestEqual(TEXT("a new match compares with the last one's tech"), FString::Join(P.TakeAnnouncements(), TEXT("|")), FString(TEXT("Battalion HQ ready")));
		P.ResetAnnouncements();
		P.ApplySnapshot(*TCProgTest::Parse(TEXT("{\"t\":\"s\",\"mode\":{\"kind\":\"conquest\"},\"mp\":0,\"out\":[false,false],\"tech\":{\"tier\":3,\"armory\":{\"inf\":3,\"guns\":3,\"armor\":3},\"lab\":[]},\"units\":[]}")));
		TestEqual(TEXT("after a reset nothing is announced"), P.TakeAnnouncements().Num(), 0);
	}
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
