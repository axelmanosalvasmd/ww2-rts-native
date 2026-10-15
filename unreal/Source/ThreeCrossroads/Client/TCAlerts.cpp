#include "Client/TCAlerts.h"
#include "Client/TCAudio.h"
#include "Client/TCLocale.h"
#include "Client/TCGameMode.h"
#include "Client/TCEffects.h"
#include "Client/TCUnitTable.h"
#include "Client/TCSupportPowers.h"
#include "Net/TCMatchState.h"
#include "ThreeCrossroads.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"

// ---------------------------------------------------------------------------------------------------------------------
// constants and tables

double TCAlertRules::LifeOf(const FString& Kind)
{
	return Kind == TEXT("event") ? 15.0 : Kind == TEXT("base") ? 10.0 : Kind == TEXT("unitLost") ? 12.0 : Life;
}

FLinearColor TCAlertRules::ColourOf(const FString& Kind)
{
	// RED '#d4574a' for danger, BRASS '#d6b25e' for a point won and scripted events, CHALK '#e2dfd3' for the rest
	if (Kind == TEXT("attack") || Kind == TEXT("base") || Kind == TEXT("unitLost") || Kind == TEXT("pointLost") || Kind == TEXT("air")) return FLinearColor(FColor(0xd4, 0x57, 0x4a));
	if (Kind == TEXT("pointWon") || Kind == TEXT("event")) return FLinearColor(FColor(0xd6, 0xb2, 0x5e));
	return FLinearColor(FColor(0xe2, 0xdf, 0xd3));
}

const TCHAR* FTCAlerts::FactionUnitName(int32 Faction, const FString& Type)
{
	// client/main.js FACTIONS[f].names (USA, Germany, USSR, UK) at e4e579a
	struct FName_ { const TCHAR* Type; const TCHAR* Name; };
	static const FName_ Usa[] = { { TEXT("rifle"), TEXT("Rifle Squad") }, { TEXT("mg"), TEXT(".30 cal MG") }, { TEXT("at"), TEXT("57mm AT Gun") }, { TEXT("tank"), TEXT("M5 Stuart") }, { TEXT("rocket"), TEXT("T34 Calliope") }, { TEXT("ranger"), TEXT("Ranger Squad") }, { TEXT("bunker"), TEXT("Command Bunker") }, { TEXT("mortar"), TEXT("81mm Mortar") }, { TEXT("sniper"), TEXT("Sniper Team") }, { TEXT("armoredcar"), TEXT("M8 Greyhound") }, { TEXT("medium"), TEXT("M4 Sherman") }, { TEXT("flak"), TEXT("40mm Bofors") }, { TEXT("flaktrack"), TEXT("M16 Half-track") }, { TEXT("fighter"), TEXT("P-51 Mustang") }, { TEXT("attacker"), TEXT("P-47 Thunderbolt") }, { TEXT("halftrack"), TEXT("M3 Half-track") }, { TEXT("lorry"), TEXT("GMC CCKW") }, { TEXT("medic"), TEXT("Medics") }, { TEXT("lcvp"), TEXT("LCVP") }, { TEXT("gunboat"), TEXT("PT Boat") }, { TEXT("destroyer"), TEXT("Fletcher Destroyer") }, { TEXT("tankdestroyer"), TEXT("M10 Wolverine") }, { TEXT("howitzer"), TEXT("M2A1 105mm Howitzer") }, { TEXT("flamer"), TEXT("Flamethrower Team") }, { TEXT("bomber"), TEXT("B-25 Mitchell") } };
	static const FName_ Ger[] = { { TEXT("rifle"), TEXT("Grenadiers") }, { TEXT("mg"), TEXT("MG 42 Team") }, { TEXT("at"), TEXT("PaK 40") }, { TEXT("tank"), TEXT("Panzer II") }, { TEXT("rocket"), TEXT("Panzerwerfer") }, { TEXT("tiger"), TEXT("Tiger I") }, { TEXT("bunker"), TEXT("Command Bunker") }, { TEXT("mortar"), TEXT("GrW 34 Mortar") }, { TEXT("sniper"), TEXT("Scharfschützen") }, { TEXT("armoredcar"), TEXT("Sd.Kfz. 222") }, { TEXT("medium"), TEXT("Panzer IV") }, { TEXT("flak"), TEXT("Flak 38") }, { TEXT("flaktrack"), TEXT("Wirbelwind") }, { TEXT("fighter"), TEXT("Bf 109") }, { TEXT("attacker"), TEXT("Ju 87 Stuka") }, { TEXT("halftrack"), TEXT("Sd.Kfz. 251") }, { TEXT("lorry"), TEXT("Opel Blitz") }, { TEXT("medic"), TEXT("Sanitäter") }, { TEXT("lcvp"), TEXT("Sturmboot") }, { TEXT("gunboat"), TEXT("S-Boot") }, { TEXT("destroyer"), TEXT("Zerstörer 1936") }, { TEXT("tankdestroyer"), TEXT("StuG III") }, { TEXT("howitzer"), TEXT("leFH 18") }, { TEXT("flamer"), TEXT("Flammenwerfer Team") }, { TEXT("bomber"), TEXT("He 111") } };
	static const FName_ Sov[] = { { TEXT("rifle"), TEXT("Riflemen") }, { TEXT("mg"), TEXT("Maxim MG") }, { TEXT("at"), TEXT("45mm AT Gun") }, { TEXT("tank"), TEXT("T-70") }, { TEXT("rocket"), TEXT("Katyusha") }, { TEXT("conscript"), TEXT("Conscripts") }, { TEXT("bunker"), TEXT("Command Bunker") }, { TEXT("mortar"), TEXT("82mm Mortar") }, { TEXT("sniper"), TEXT("Snipers") }, { TEXT("armoredcar"), TEXT("BA-64") }, { TEXT("medium"), TEXT("T-34") }, { TEXT("flak"), TEXT("61-K AA Gun") }, { TEXT("flaktrack"), TEXT("ZSU-37") }, { TEXT("fighter"), TEXT("Yak-9") }, { TEXT("attacker"), TEXT("Il-2 Sturmovik") }, { TEXT("halftrack"), TEXT("M5 Half-track") }, { TEXT("lorry"), TEXT("ZiS-5") }, { TEXT("medic"), TEXT("Sanitary Team") }, { TEXT("lcvp"), TEXT("Assault Boat") }, { TEXT("gunboat"), TEXT("Armored Boat") }, { TEXT("destroyer"), TEXT("Gnevny Destroyer") }, { TEXT("tankdestroyer"), TEXT("SU-85") }, { TEXT("howitzer"), TEXT("122mm M-30 Howitzer") }, { TEXT("flamer"), TEXT("ROKS-2 Flamethrower Team") }, { TEXT("bomber"), TEXT("Pe-2") } };
	static const FName_ Uk[] = { { TEXT("rifle"), TEXT("Rifle Section") }, { TEXT("mg"), TEXT("Vickers MG") }, { TEXT("at"), TEXT("6-pounder") }, { TEXT("tank"), TEXT("Stuart V") }, { TEXT("rocket"), TEXT("Land Mattress") }, { TEXT("churchill"), TEXT("Churchill VII") }, { TEXT("commando"), TEXT("Commandos") }, { TEXT("bunker"), TEXT("Command Bunker") }, { TEXT("mortar"), TEXT("3-inch Mortar") }, { TEXT("sniper"), TEXT("Sniper Pair") }, { TEXT("armoredcar"), TEXT("Daimler Armoured Car") }, { TEXT("medium"), TEXT("Cromwell") }, { TEXT("flak"), TEXT("40mm Bofors") }, { TEXT("flaktrack"), TEXT("Crusader AA") }, { TEXT("fighter"), TEXT("Spitfire") }, { TEXT("attacker"), TEXT("Typhoon") }, { TEXT("halftrack"), TEXT("Universal Carrier") }, { TEXT("lorry"), TEXT("Bedford QL") }, { TEXT("medic"), TEXT("Stretcher Bearers") }, { TEXT("lcvp"), TEXT("LCA") }, { TEXT("gunboat"), TEXT("MTB") }, { TEXT("destroyer"), TEXT("Tribal-class Destroyer") }, { TEXT("tankdestroyer"), TEXT("Achilles") }, { TEXT("howitzer"), TEXT("25-pounder") }, { TEXT("flamer"), TEXT("Lifebuoy Flamethrower Team") }, { TEXT("bomber"), TEXT("Mosquito") } };
	const TArrayView<const FName_> Tables[] = { MakeArrayView(Usa), MakeArrayView(Ger), MakeArrayView(Sov), MakeArrayView(Uk) };
	if (Faction < 0 || Faction >= 4) return nullptr;
	for (const FName_& N : Tables[Faction]) if (Type == N.Type) return N.Name;
	return nullptr;
}

FString FTCAlerts::UnitName(const FTCMatchState& S, const FTCUnitTable* Table, const FString& Type, int32 Owner)
{
	// client/main.js facOf: factions[slot] ?? (slot < 0 ? 0 : slot % 3)
	const int32 Faction = S.Factions.IsValidIndex(Owner) ? S.Factions[Owner] : Owner < 0 ? 0 : Owner % 3;
	if (const TCHAR* N = FactionUnitName(Faction, Type)) return N;
	const FTCUnitDef* Def = Table ? Table->Find(Type) : nullptr;
	return Def ? Def->Name : Type;
}

// ---------------------------------------------------------------------------------------------------------------------
// client/alert-history.js

bool FTCAlertHistory::Start(int32 MatchId)
{
	if (MatchId >= 0 && bMatchKnown && MatchId == Match) return false;
	Match = MatchId; bMatchKnown = MatchId >= 0;
	List.Reset(); Serial = 0; Cursor = 0; LastTick = -1;
	return true;
}

void FTCAlertHistory::Reset()
{
	List.Reset(); Serial = 0; Cursor = 0; LastTick = -1;
}

bool FTCAlertHistory::AcceptTick(int32 Tick)
{
	if (Tick <= LastTick) return false;
	LastTick = Tick;
	return true;
}

const FTCAlertHistoryEntry& FTCAlertHistory::Add(const FTCAlertPush& A)
{
	FTCAlertHistoryEntry E;
	E.Id = ++Serial; E.Kind = A.Kind; E.Text = A.Text; E.bHasAt = A.bHasAt; E.At = A.At; E.Time = A.Time;
	List.Insert(MoveTemp(E), 0);
	if (List.Num() > Limit) List.SetNum(Limit);
	if (Cursor && !List.ContainsByPredicate([this](const FTCAlertHistoryEntry& X) { return X.Id == Cursor; })) Cursor = 0;
	return List[0];
}

const FTCAlertHistoryEntry* FTCAlertHistory::Select(int32 Id)
{
	const FTCAlertHistoryEntry* E = List.FindByPredicate([Id](const FTCAlertHistoryEntry& X) { return X.Id == Id; });
	if (E) Cursor = Id;
	return E;
}

const FTCAlertHistoryEntry* FTCAlertHistory::Navigate(int32 Direction)
{
	TArray<const FTCAlertHistoryEntry*> Eligible;
	for (const FTCAlertHistoryEntry& E : List) if (E.bHasAt) Eligible.Add(&E);
	if (!Eligible.Num()) return nullptr;
	const int32 Current = Eligible.IndexOfByPredicate([this](const FTCAlertHistoryEntry* X) { return X->Id == Cursor; });
	const int32 Index = Current < 0 ? 0 : FMath::Clamp(Current + Direction, 0, Eligible.Num() - 1);
	Cursor = Eligible[Index]->Id;
	return Eligible[Index];
}

// ---------------------------------------------------------------------------------------------------------------------
// client/pings.js

TSharedPtr<FJsonObject> FTCPings::MakeSend(double X, double Z)
{
	if (!FMath::IsFinite(X) || !FMath::IsFinite(Z)) return nullptr;
	TSharedPtr<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("ping"));
	M->SetNumberField(TEXT("x"), X);
	M->SetNumberField(TEXT("z"), Z);
	return M;
}

bool FTCPings::Receive(const FJsonObject& M, double Now, FTCPingRing& Out)
{
	const TSharedPtr<FJsonValue> From = M.TryGetField(TEXT("from")), X = M.TryGetField(TEXT("x")), Z = M.TryGetField(TEXT("z"));
	auto Finite = [](const TSharedPtr<FJsonValue>& V) { return V.IsValid() && V->Type == EJson::Number && FMath::IsFinite(V->AsNumber()); };
	// Number.isInteger(m.from) && m.from >= 0, Number.isFinite(m.x), Number.isFinite(m.z)
	if (!Finite(From) || !Finite(X) || !Finite(Z)) return false;
	const double F = From->AsNumber();
	if (F < 0 || F != FMath::FloorToDouble(F) || F > 1e6) return false;
	Out.From = (int32)F; Out.X = X->AsNumber(); Out.Z = Z->AsNumber(); Out.Born = Now;
	if (RingList.Num() >= 64) RingList.RemoveAt(0);
	RingList.Add(Out);
	return true;
}

void FTCPings::Frame(double Now)
{
	RingList.RemoveAll([Now](const FTCPingRing& R) { return Now - R.Born >= Life; });
}

bool FTCPings::LookOf(const FTCPingRing& R, double Now, FTCPingLook& Out) const
{
	const double Age = Now - R.Born;
	if (Age >= Life || Age < 0) return false;
	const double Phase = FMath::Fmod(Age, Period) / Period, Fade = FMath::Min(1.0, (Life - Age) / FadeTime);
	Out.Radius = (float)(2 + Phase * 6);
	Out.Opacity = (float)(Fade * (1 - Phase) * 0.9);
	Out.MinimapPx = (float)(4 + Phase * 14);
	Out.MinimapAlpha = (float)(Fade * (1 - Phase));
	return true;
}

bool FTCPings::Active(double Now) const
{
	return RingList.ContainsByPredicate([Now](const FTCPingRing& R) { return Now - R.Born < Life; });
}

// ---------------------------------------------------------------------------------------------------------------------
// client/alerts.js

namespace
{
	bool Near(FVector2D A, FVector2D B, double R) { return FVector2D::Distance(A, B) <= R; }

	struct FSpot { FString Type; int32 Owner = -1; FVector2D At; };
	// alerts.js groups: spots within AREA of the first one of each group
	TArray<TArray<FSpot>> Groups(const TArray<FSpot>& List)
	{
		TArray<TArray<FSpot>> Out;
		for (const FSpot& It : List)
		{
			TArray<FSpot>* G = Out.FindByPredicate([&It](const TArray<FSpot>& Q) { return Near(Q[0].At, It.At, TCAlertRules::Area); });
			if (G) G->Add(It); else Out.Add({ It });
		}
		return Out;
	}
	FVector2D Mid(const TArray<FSpot>& G)
	{
		double X = 0, Z = 0;
		for (const FSpot& S : G) { X += S.At.X; Z += S.At.Y; }
		return FVector2D(X / G.Num(), Z / G.Num());
	}
	const TArray<TSharedPtr<FJsonValue>>* ArrayField(const FJsonObject* O, const TCHAR* Name)
	{
		const TArray<TSharedPtr<FJsonValue>>* A = nullptr;
		if (!O) return nullptr;
		const TSharedPtr<FJsonValue> V = O->TryGetField(Name);
		return V.IsValid() && V->Type == EJson::Array && V->TryGetArray(A) ? A : nullptr;
	}
	bool Number(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return FMath::IsFinite(Out);
	}
}

void FTCAlerts::StartMatch(const FTCMatchState& S)
{
	if (Hist.Start(S.MatchId))
	{
		// alerts.js startMatch: the lines go, the history box closes
		LineList.Reset(); OwnQuiet.Reset(); LastSound.Reset(); MatchTime = 0.0; Echoes.Reset(); bHistoryOpen = false;
	}
	PingRings.Reset(); // main.js startGame: pings.reset()
	bHasPrev = false; LastSnapTick = 0;
	PrevOrder.Reset(); PrevRows.Reset(); PrevPoints.Reset(); PrevStrikes.Reset(); PrevQueueHeads.Reset();
	RowOrder.Reset(); RowSet.Reset(); RowXZ.Reset(); RowBuilt.Reset();
	// main.js startGame: home = m.spawn
	bHasHome = S.You >= 0;
	Home = S.Spawn;
	MatchSeen = S.MatchId;
}

void FTCAlerts::Reset()
{
	LineList.Reset(); OwnQuiet.Reset(); LastSound.Reset(); Sounds.Reset(); Hist.Reset(); MatchTime = 0.0;
	PingRings.Reset(); Echoes.Reset(); bHistoryOpen = false;
	bHasPrev = false; LastSnapTick = 0;
	PrevOrder.Reset(); PrevRows.Reset(); PrevPoints.Reset(); PrevStrikes.Reset(); PrevQueueHeads.Reset();
	RowOrder.Reset(); RowSet.Reset(); RowXZ.Reset(); RowBuilt.Reset(); bHasHome = false;
}

double FTCAlerts::HistoryTime() const
{
	return FMath::Max(MatchTime, LastSnapTick / 20.0);
}

void FTCAlerts::Sound(const FString& Kind, double Now)
{
	const double* Last = LastSound.Find(Kind);
	if (Last && Now - *Last < TCAlertRules::SoundGap) return;
	LastSound.Add(Kind, Now);
	if (Sounds.Num() < 64) Sounds.Add(TEXT("alert:") + Kind);
}

FTCAlertPush FTCAlerts::Push(const FString& Kind, const FString& Text, bool bHasAt, FVector2D At, double Now, int32 N)
{
	FTCAlertPush A;
	A.Kind = Kind; A.Text = Text.Left(TCMatchLimits::Text); A.bHasAt = bHasAt; A.At = bHasAt ? At : FVector2D::ZeroVector; A.N = N; A.Time = HistoryTime();
	Hist.Add(A);
	AddLine(A, Now);
	if (Kind != TEXT("ping")) Sound(Kind, Now);
	// ATCGameMode::PushAlert mirrors every alert into this feed; this one is already here
	Echoes.Add({ A.Kind, A.Text, Now });
	if (Echoes.Num() > 64) Echoes.RemoveAt(0);
	return A;
}

void FTCAlerts::AddLine(const FTCAlertPush& A, double Now)
{
	FTCAlertLine L;
	L.Serial = ++LineSerial; L.Alert = A; L.Born = Now;
	LineList.Insert(L, 0);
	// four routine lines, two losses: the oldest beyond that goes
	const bool bLoss = L.IsLoss();
	const int32 Limit = bLoss ? TCAlertRules::MaxLosses : TCAlertRules::MaxLines;
	int32 Seen = 0;
	for (int32 I = 0; I < LineList.Num(); I++)
		if (LineList[I].IsLoss() == bLoss && ++Seen > Limit) { LineList.RemoveAt(I); I--; }
}

void FTCAlerts::Mirror(const FString& Kind, const FString& Text, FVector2D At, int32 Tick, double Now)
{
	const FString T = Text.Left(TCMatchLimits::Text);
	const int32 E = Echoes.IndexOfByPredicate([&](const FEcho& X) { return X.Kind == Kind && X.Text == T && FMath::Abs(Now - X.Now) < 1.0; });
	if (E != INDEX_NONE) { Echoes.RemoveAt(E); return; }
	// alerts.js push for an alert raised outside this feed (region, supply, the game mode's point alerts): the history
	// time is Math.max(matchTime, hooks.matchTime()), the line as any other; the game mode already played its sound
	FTCAlertPush A;
	A.Kind = Kind; A.Text = T; A.bHasAt = true; A.At = At; A.Time = FMath::Max(HistoryTime(), Tick / 20.0);
	Hist.Add(A);
	AddLine(A, Now);
}

const FTCAlertHistoryEntry* FTCAlerts::Navigate(int32 Direction)
{
	// alerts.js navigate: history.navigate(direction), visit, and the box opens
	const FTCAlertHistoryEntry* E = Hist.Navigate(Direction);
	bHistoryOpen = true;
	return E;
}

bool FTCAlerts::Click(const FString& Tag, FVector2D& OutAt, bool& bOutJump)
{
	bOutJump = false;
	FString Rest;
	if (Tag.StartsWith(TEXT("alert:line:"), ESearchCase::CaseSensitive))
	{
		// init's mousedown: the line's spot, if it has one (a.x != null)
		const int32 Serial = FCString::Atoi(*Tag.RightChop(11));
		const FTCAlertLine* L = LineList.FindByPredicate([Serial](const FTCAlertLine& X) { return X.Serial == Serial; });
		if (L && L->Alert.bHasAt) { OutAt = L->Alert.At; bOutJump = true; }
		return true;
	}
	if (Tag == TEXT("alert:history")) { ToggleHistory(); return true; }
	if (Tag.StartsWith(TEXT("alert:nav:"), ESearchCase::CaseSensitive))
	{
		const FTCAlertHistoryEntry* E = Navigate(FCString::Atoi(*Tag.RightChop(10)) >= 0 ? 1 : -1);
		if (E && E->bHasAt) { OutAt = E->At; bOutJump = true; }
		return true;
	}
	if (Tag.StartsWith(TEXT("alert:entry:"), ESearchCase::CaseSensitive))
	{
		// renderHistory's button: history.select(a.id), visit(entry)
		const FTCAlertHistoryEntry* E = Hist.Select(FCString::Atoi(*Tag.RightChop(12)));
		if (E && E->bHasAt) { OutAt = E->At; bOutJump = true; }
		return true;
	}
	return false;
}

float FTCAlerts::ShownAlpha(const FTCAlertLine& L, double Now, float* OutSlide)
{
	// alert-in (0.18 s) or loss-in (0.22 s), ease-out from opacity 0 and 8 px (6 px) to the right; then frame()'s fade
	const double Age = Now - L.Born, Dur = L.IsLoss() ? TCAlertLook::LossSlideIn : TCAlertLook::SlideIn;
	if (OutSlide) *OutSlide = 0.f;
	if (Age >= 0 && Age < Dur)
	{
		const float K = TCCss::EaseOut((float)(Age / Dur));
		if (OutSlide) *OutSlide = (1.f - K) * (L.IsLoss() ? 6.f : 8.f);
		return K;
	}
	return Alpha(L, Now);
}

TArray<FTCAlertRing> FTCAlerts::MinimapRings(double Now) const
{
	// drawPings: every line with a place but a ping's; ring(4 + ph * 14, 1 - ph) then ring(4, 1), at fade * alpha
	TArray<FTCAlertRing> Out;
	for (const FTCAlertLine& L : LineList)
	{
		if (L.Alert.Kind == TEXT("ping") || !L.Alert.bHasAt) continue;
		const double Age = Now - L.Born, Fade = FMath::Clamp((TCAlertRules::LifeOf(L.Alert.Kind) - Age) / TCAlertRules::Fade, 0.0, 1.0);
		const double Ph = FMath::Fmod(FMath::Max(0.0, Age), 1.2) / 1.2;
		const FLinearColor C = TCAlertRules::ColourOf(L.Alert.Kind);
		Out.Add({ L.Alert.At, (float)(4 + Ph * 14), (float)(Fade * (1 - Ph)), C });
		Out.Add({ L.Alert.At, 4.f, (float)Fade, C });
	}
	return Out;
}

namespace
{
	// alerts.css: light signal red for trouble, brass for a point won, the text colour for the rest (the base alert
	// is white on red)
	FLinearColor LineColour(const FString& Kind)
	{
		if (Kind == TEXT("attack") || Kind == TEXT("unitLost") || Kind == TEXT("pointLost") || Kind == TEXT("air") || Kind == TEXT("base")) return TCCss::RedHi;
		if (Kind == TEXT("pointWon")) return TCCss::Brass;
		return TCCss::Text;
	}
	// alert-history.js matchClock: m:ss
	FString MatchClock(double T)
	{
		return FString::Printf(TEXT("%d:%02d"), (int32)FMath::FloorToDouble(T / 60.0), (int32)FMath::FloorToDouble(FMath::Fmod(FMath::Max(0.0, T), 60.0)));
	}
}

FTCPaint FTCAlerts::Layout(const FTCAlertViewInput& In, const FTCMeasure& Measure, const FTCMeasure& HeavyMeasure) const
{
	using namespace TCAlertLook;
	FTCPaint P;
	const double Now = In.Now;
	const float BoxW = FMath::Min(BoxMax, In.ViewW - BoxSide), Right = In.Right;
	const float Scale = FMath::Max(0.01f, In.UiScale), Border = FMath::Max(1.f, FMath::FloorToFloat(Scale)) / Scale;
	auto Line = [Scale](float Px) { return FMath::FloorToFloat(Px * Scale * 64.f) / (64.f * Scale); };
	const float LineBox = Line(FontPx * LineHeight), HeadingH = Line(HeadingPx * LineHeight), DetailH = Line(DetailPx * DetailLine), HintH = Line(HintPx * LineHeight);
	auto Hover = [&In](float X, float Y, float W, float H) { return In.bMouse && In.Mouse.X >= X && In.Mouse.X < X + W && In.Mouse.Y >= Y && In.Mouse.Y < Y + H; };

	// the children, top to bottom: the loss column, the routine lines (newest first), the history box
	TArray<const FTCAlertLine*> Losses, Routine;
	for (const FTCAlertLine& L : LineList) (L.IsLoss() ? Losses : Routine).Add(&L);
	if (Losses.Num() > TCAlertRules::MaxLosses) Losses.SetNum(TCAlertRules::MaxLosses);
	if (Losses.Num() && Routine.Num() > LinesWithLoss) Routine.SetNum(LinesWithLoss);

	struct FBlock { float W = 0, H = 0; TFunction<void(FTCPaint&, float X, float Y)> Paint; };
	TArray<FBlock> Blocks;

	// #lossAlerts: 380 px, 6 px between losses, 8 px under the column
	if (Losses.Num())
	{
		const float W = FMath::Min(LossW, In.ViewW - LossSide), Inner = W - LossEdge - 2 * LossPadX - Border;
		struct FLoss { const FTCAlertLine* L; FString Heading, Hint; TArray<FString> Detail, HintLines; float H; };
		TArray<FLoss> Items;
		float H = 0;
		for (const FTCAlertLine* L : Losses)
		{
			FLoss F;
			F.L = L;
			F.Heading = TC_T(FString(TEXT("Unit lost"))).ToUpper(); // tr('Unit lost'), then text-transform: uppercase
			F.Detail = FTCPaint::Wrap(TC_T(L->Alert.Text), Inner, DetailPx, true, HeavyMeasure ? HeavyMeasure : Measure);
			F.HintLines = FTCPaint::Wrap(TC_T(FString(TEXT("Space or click: look there"))), Inner, HintPx, false, Measure);
			F.H = 2 * Border + 2 * LossPadY + HeadingH + 2 * LossInner + F.Detail.Num() * DetailH + HintTop + F.HintLines.Num() * HintH;
			H += (Items.Num() ? LossGap : 0.f) + F.H;
			Items.Add(MoveTemp(F));
		}
		FBlock B;
		for (const FLoss& Loss : Items)
		{
		B.W = W; B.H = Loss.H+(&Loss==&Items.Last()?LossBottom:LossGap-Gap);
		B.Paint = [this, Items=TArray<FLoss>{Loss}, W, Now, Hover, Border, HeadingH, DetailH, HintH, ViewW=In.ViewW](FTCPaint& Out, float X, float Y)
		{
			for (const FLoss& F : Items)
			{
				float Slide = 0.f;
				const float A = ShownAlpha(*F.L, Now, &Slide);
				const int32 First = Out.Items.Num();
				const bool bHover = Hover(X, Y, W, F.H);
				// box-shadow: 0 3px 14px rgba(0, 0, 0, 0.4), as three soft layers
				for (int32 K = 3; K >= 1; K--) Out.Rect(X - 3.f * K, Y + 3 - 3.f * K, W + 6.f * K, F.H + 6.f * K, TCCss::Rgba(0, 0, 0, 0.4f / (K + 2)), TEXT("alert:loss-shadow"));
				const FLinearColor Edge = bHover ? TCCss::Rgba(214, 178, 94, 0.8f) : TCCss::Hex(0xc66a57);
				Out.Rect(X, Y, W, F.H, bHover ? TCCss::Rgba(46, 51, 55, 0.94f) : TCCss::Rgba(73, 24, 21, 0.97f), TEXT("alert:loss"));
				Out.Frame(X, Y, W, F.H, Edge, Border, TEXT("alert:loss-frame"));
				Out.Rect(X, Y, LossEdge, F.H, bHover ? Edge : TCCss::Hex(0xff9a76), TEXT("alert:loss-edge"));
				const float Tx = X + LossEdge + LossPadX;
				float Ty = Y + Border + LossPadY;
				FTCPaintItem& Hd = Out.Text(F.Heading, Tx, Ty, HeadingPx, TCCss::Hex(0xffb293), true, ETCPaintAlign::Left, TEXT("alert:loss-heading"));
				Hd.Tracking = HeadingTrack * HeadingPx; Hd.Size.Y = HeadingH;
				Ty += Hd.Size.Y + LossInner;
				for (const FString& D : F.Detail)
				{
					FTCPaintItem& Dt = Out.Text(D, Tx, Ty, DetailPx, TCCss::Hex(0xfff1e8), true, ETCPaintAlign::Left, TEXT("alert:loss-detail"));
					Dt.Size.Y = DetailH;
					Ty += Dt.Size.Y;
				}
				Ty += LossInner + HintTop;
				for (const FString& Hn : F.HintLines)
				{
					FTCPaintItem& Hi = Out.Text(Hn, Tx, Ty, HintPx, TCCss::Hex(0xdec4bb), false, ETCPaintAlign::Left, TEXT("alert:loss-hint"));
					Hi.Size.Y = HintH;
					Ty += Hi.Size.Y;
				}
				Out.Hit(FBox2D(FVector2D(X, Y), FVector2D(X + W, Y + F.H)), FString::Printf(TEXT("alert:line:%d"), F.L->Serial), F.L->Alert.Text);
				Out.Panels.Add(FBox2D(FVector2D(X, Y), FVector2D(X + W, Y + F.H)));
				Out.Shift(First, FVector2D(FMath::Min(Slide,FMath::Max(0.f,ViewW-8-X-W)), 0));
				Out.Fade(First, A);
				Y += F.H + LossGap;
			}
		};
		Blocks.Add(MoveTemp(B)); B=FBlock();
		}
	}
	// the routine lines: padding 3px 10px, at most 320 px with an ellipsis; the newest semi-bold with the strong border
	for (int32 I = 0; I < Routine.Num(); I++)
	{
		const FTCAlertLine* L = Routine[I];
		const bool bBase = L->Alert.Kind == TEXT("base"), bNewest = I == 0, bBold = bBase || bNewest;
		const float MaxW = FMath::Min(LineMax, BoxW) - 2 * PadX - 2 * Border;
		const FString Text = FTCPaint::Ellipsis(TC_T(L->Alert.Text), MaxW, FontPx, bBold, Measure);
		FBlock B;
		B.W = FMath::Min(FMath::Min(LineMax, BoxW), Measure(Text, FontPx, bBold, 0.f) + 2 * PadX + 2);
		B.H = LineBox + 2 * PadY + 2 * Border;
		const float W = B.W, H = B.H;
		B.Paint = [L, Text, W, H, Now, bBase, bNewest, bBold, LineBox, Hover, Border, ViewW=In.ViewW](FTCPaint& Out, float X, float Y)
		{
			float Slide = 0.f;
			const float A = ShownAlpha(*L, Now, &Slide);
			const int32 First = Out.Items.Num();
			FLinearColor Back = TCCss::Rgba(25, 28, 30, 0.9f), Edge = bNewest ? TCCss::Rgba(176, 164, 122, 0.46f) : TCCss::Rgba(176, 164, 122, 0.24f);
			if (bBase)
			{
				Back = TCCss::Rgba(140, 36, 28, 0.94f); Edge = TCCss::Hex(0xd4574a);
				// alert-base: 0.9 s ease-in-out, 4 alternate iterations after the 0.18 s slide-in
				const double T = Now - L->Born - SlideIn;
				if (T >= 0 && T < 4 * BasePulse)
				{
					const int32 Iter = (int32)(T / BasePulse);
					const float F = (float)(T / BasePulse - Iter), K = TCCss::EaseInOut(Iter % 2 ? 1.f - F : F);
					Back = FMath::Lerp(Back, TCCss::Rgba(200, 60, 46, 0.96f), K);
				}
			}
			if (Hover(X, Y, W, H) && !(bBase && Now - L->Born < SlideIn + 4 * BasePulse)) { Back = TCCss::Rgba(46, 51, 55, 0.94f); Edge = TCCss::Rgba(214, 178, 94, 0.8f); }
			else if (Hover(X, Y, W, H)) Edge = TCCss::Rgba(214, 178, 94, 0.8f);
			Out.Rect(X, Y, W, H, Back, TEXT("alert:line"));
			Out.Frame(X, Y, W, H, Edge, Border, TEXT("alert:line-frame"));
			FTCPaintItem& T = Out.Text(Text, X + Border + PadX, Y + Border + PadY, FontPx, bBase ? FLinearColor::White : LineColour(L->Alert.Kind), bBold, ETCPaintAlign::Left, bNewest ? TEXT("alert:text-newest") : TEXT("alert:text"));
			T.Size.Y = LineBox;
			Out.Hit(FBox2D(FVector2D(X, Y), FVector2D(X + W, Y + H)), FString::Printf(TEXT("alert:line:%d"), L->Serial), L->Alert.Text);
			Out.Panels.Add(FBox2D(FVector2D(X, Y), FVector2D(X + W, Y + H)));
			Out.Shift(First, FVector2D(FMath::Min(Slide,FMath::Max(0.f,ViewW-8-X-W)), 0));
			Out.Fade(First, A);
		};
		Blocks.Add(MoveTemp(B));
	}
	// the Alert history box: the summary always, the nav buttons and the list when open
	{
		float W = FMath::Min(HistoryW, In.ViewW - BoxSide);
        // Measure free vertical intervals at each possible column edge. Prefer the source width, then narrow it.
        auto FreeHeight=[&](float Width)
        {
            const float Bottom=FMath::Min(In.ViewH-8,In.MinimapTop-MinimapFrame-MinimapGap);
            TArray<float> Xs={Right-Width,8.f};
            for (const FBox2D& B : In.Avoid) if (B.bIsValid) { Xs.Add(B.Min.X-Width-6); Xs.Add(B.Max.X+6); }
            float Largest=0;
            for (float X : Xs)
            {
                if (X<8 || X+Width>In.ViewW-8+.01f) continue;
                TArray<FVector2D> Intervals;
                for (const FBox2D& A : In.Avoid)
                    if (A.bIsValid && X<A.Max.X+.01 && X+Width>A.Min.X-.01 && A.Min.Y<Bottom && A.Max.Y>8)
                        Intervals.Add(FVector2D(A.Min.Y-6,A.Max.Y+6));
                Intervals.Sort([](FVector2D A,FVector2D B) { return A.X<B.X; });
                float Top=8;
                for (FVector2D Interval : Intervals) { Largest=FMath::Max(Largest,float(Interval.X-Top)); Top=FMath::Max(Top,float(Interval.Y)); }
                Largest=FMath::Max(Largest,Bottom-Top);
            }
            return Largest;
        };
        const float Minimum=2*Border+2*HistoryPadY+LineBox;
        const float EntryMinimum=2*ListPad+2*EntryMargin+2*Border+2*EntryPad+LineBox;
        float Available=FreeHeight(W);
        bool Found=false;
        for (float Extra : {bHistoryOpen?EntryMinimum:0.f,0.f})
        {
            for (float Width : {W,280.f,240.f,200.f,176.f}) if (Width<=W && FreeHeight(Width)>=Minimum+Extra)
            { W=Width; Available=FreeHeight(Width); Found=true; break; }
            if (Found) break;
        }
        const bool Expand=bHistoryOpen && Available>=Minimum+EntryMinimum;
		const float SumH = Border + HistoryPadY + LineBox + HistoryPadY;
		const float ButtonH = LineBox + 2 * ButtonPadY + 2 * Border;
		float NavH = 2 * NavPad + ButtonH;
        const float NavW=Measure(TC_T(FString(TEXT("Previous Alert"))),FontPx,false,0)+Measure(TC_T(FString(TEXT("Next Alert"))),FontPx,false,0)+4*ButtonPadX+4*Border+NavGap+2*NavPad;
        if (NavW>W) NavH=0;
		const TArray<FTCAlertHistoryEntry>& Entries = Hist.Entries();
        // A short interval retains a readable entry through Alt+Left/Right; the nav buttons give way first.
        if (Available-SumH-NavH-Border<EntryMinimum) NavH=0;
        const float Room=FMath::Max(0.f,Available-SumH-NavH-2*ListPad-Border);
		const float ListMaxH = FMath::Min3(ListMax,ListVh*In.ViewH,Room);
		struct FEntry { const FTCAlertHistoryEntry* E; TArray<FString> Lines; float H; };
		TArray<FEntry> Rows;
		float ListH = 2 * ListPad, Content = 0.f;
		int32 First = 0;
		if (Expand)
		{
			const float TextW = W - 4 * Border - 2 * ListPad - 2 * EntryPad;
			for (const FTCAlertHistoryEntry& E : Entries)
			{
				FEntry R;
				R.E = &E;
				// alerts.js translates the stored loss text before renderHistory prefixes its clock. Translating the
				// combined string lets es.js's '{0} lost' pattern move the clock into 'Perdido: {0}'.
				const FString Shown = MatchClock(E.Time) + TEXT(" · ") + TC_T(E.Text);
				R.Lines = FTCPaint::Wrap(Shown, TextW, FontPx, false, Measure);
				R.H = 2 * Border + 2 * EntryPad + R.Lines.Num() * LineBox;
				Rows.Add(MoveTemp(R));
			}
			// the list scrolls (max-height min(30vh, 250px)): this shows a window of whole entries, the selected one in it
			for (const FEntry& R : Rows) Content += R.H + EntryMargin;
			Content += Rows.Num() ? EntryMargin : LineBox;
			const int32 Sel = Rows.IndexOfByPredicate([this](const FEntry& R) { return R.E->Id == Hist.Selected(); });
			if (Sel > 0)
			{
				float Run = EntryMargin;
				for (int32 K = 0; K <= Sel; K++) Run += Rows[K].H + EntryMargin;
				while (First < Sel && Run > ListMaxH) { Run -= Rows[First].H + EntryMargin; First++; }
			}
			ListH += FMath::Min(Content, ListMaxH);
		}
		FBlock B;
		B.W = W;
		B.H = SumH + (Expand ? NavH + ListH : 0.f) + Border;
		const float H = B.H;
		const bool bOpen = Expand;
		const int32 Selected = Hist.Selected();
		B.Paint = [W, H, SumH, ButtonH, NavH, ListH, ListMaxH, Content, Rows, First, bOpen, Selected, LineBox, &Measure, this, Border](FTCPaint& Out, float X, float Y)
		{
			Out.Rect(X, Y, W, H, TCCss::Rgba(25, 28, 30, 0.96f), TEXT("alert:history"));
			Out.Frame(X, Y, W, H, TCCss::Rgba(176, 164, 122, 0.46f), Border, TEXT("alert:history-frame"));
			Out.Panels.Add(FBox2D(FVector2D(X, Y), FVector2D(X + W, Y + H)));
			// <summary>Alert history</summary> with the details marker
			const float Sx = X + Border + HistoryPadX, Sy = Y + Border + HistoryPadY;
			// The browser's <summary> marker is geometry, not a glyph in the Barlow font.
			const float MidY = Sy + LineBox / 2;
			for (int32 K = 0; K < 7; K++)
			{
				if (bOpen) Out.Line(FVector2D(Sx + K / 2.f, MidY - 3 + K), FVector2D(Sx + 7 - K / 2.f, MidY - 3 + K), TCCss::Text, 1.f, TEXT("alert:history-marker"));
				else Out.Line(FVector2D(Sx + K, MidY - 4 + K / 2.f), FVector2D(Sx + K, MidY + 4 - K / 2.f), TCCss::Text, 1.f, TEXT("alert:history-marker"));
			}
			FTCPaintItem& Su = Out.Text(TC_T(FString(TEXT("Alert history"))), Sx + FontPx * 0.95f, Sy, FontPx, TCCss::Text, false, ETCPaintAlign::Left, TEXT("alert:history-summary"));
			Su.Size.Y = LineBox;
			Out.Hit(FBox2D(FVector2D(X, Y), FVector2D(X + W, Y + SumH)), TEXT("alert:history"), TEXT("Alert history"));
			if (!bOpen) return;
			// the nav: Previous Alert (navigate(1)), Next Alert (navigate(-1))
			float Bx = X + Border + NavPad;
			const float By = Y + SumH + NavPad;
			for (int32 K = 0; NavH>0 && K < 2; K++)
			{
				const FString Label = TC_T(FString(K == 0 ? TEXT("Previous Alert") : TEXT("Next Alert")));
				const float Bw = Measure(Label, FontPx, false, 0.f) + 2 * ButtonPadX + 2 * Border;
				Out.Rect(Bx, By, Bw, ButtonH, TCCss::Rgba(46, 51, 55, 0.94f), TEXT("alert:nav"));
				Out.Frame(Bx, By, Bw, ButtonH, TCCss::Rgba(176, 164, 122, 0.24f), Border, TEXT("alert:nav-frame"));
				FTCPaintItem& Bt = Out.Text(Label, Bx + Border + ButtonPadX, By + Border + ButtonPadY, FontPx, TCCss::Text, false, ETCPaintAlign::Left, TEXT("alert:nav-label"));
				Bt.Size.Y = LineBox;
				Out.Hit(FBox2D(FVector2D(Bx, By), FVector2D(Bx + Bw, By + ButtonH)), K == 0 ? TEXT("alert:nav:1") : TEXT("alert:nav:-1"), K == 0 ? TEXT("Previous Alert") : TEXT("Next Alert"));
				Bx += Bw + NavGap;
			}
			// the list
			const float Lx = X + Border + ListPad, Lw = W - 2 * Border - 2 * ListPad, Top = Y + SumH + NavH + ListPad, Bottom = Top + ListH - 2 * ListPad;
			if (!Rows.Num())
			{
				FTCPaintItem& E = Out.Text(TC_T(FString(TEXT("No Alerts yet"))), Lx, Top, FontPx, TCCss::Text, false, ETCPaintAlign::Left, TEXT("alert:history-empty"));
				E.Size.Y = LineBox;
				return;
			}
			float Ey = Top + EntryMargin;
			for (int32 K = First; K < Rows.Num(); K++)
			{
				const FEntry& R = Rows[K];
				if (Ey + R.H > Bottom + 0.5f) break;
				Out.Rect(Lx, Ey, Lw, R.H, TCCss::Rgba(46, 51, 55, 0.94f), TEXT("alert:entry"));
				Out.Frame(Lx, Ey, Lw, R.H, TCCss::Rgba(176, 164, 122, 0.24f), Border, TEXT("alert:entry-frame"));
				if (R.E->Id == Selected) Out.Frame(Lx, Ey, Lw, R.H, TCCss::Brass, 2.f, TEXT("alert:entry-current")); // aria-current: the 2 px brass outline inside
				float Ty = Ey + Border + EntryPad;
				for (const FString& Ln : R.Lines)
				{
					FTCPaintItem& T = Out.Text(Ln, Lx + Border + EntryPad, Ty, FontPx, LineColour(R.E->Kind), false, ETCPaintAlign::Left, TEXT("alert:entry-text"));
					T.Size.Y = LineBox;
					Ty += LineBox;
				}
				Out.Hit(FBox2D(FVector2D(Lx, Ey), FVector2D(Lx + Lw, Ey + R.H)), FString::Printf(TEXT("alert:entry:%d"), R.E->Id), R.E->Text);
				Ey += R.H + EntryMargin;
			}
			// the list's scrollbar when it overflows
			if (Content > ListMaxH)
			{
				const float Track = Bottom - Top, Thumb = FMath::Min(Track,FMath::Max(12.f, Track * ListMaxH / Content));
				float Before = 0.f;
				for (int32 K = 0; K < First; K++) Before += Rows[K].H + EntryMargin;
				Out.Rect(X + W - 1 - 4, Top + (Track - Thumb) * FMath::Clamp(Before / FMath::Max(1.f, Content - ListMaxH), 0.f, 1.f), 3, Thumb, TCCss::Rgba(176, 164, 122, 0.46f), TEXT("alert:history-scroll"));
			}
		};
		Blocks.Add(MoveTemp(B));
	}
	// Keep the pinned anchor whenever it fits. Overflow is retained in history, never painted offscreen or over controls.
	const float Bottom=FMath::Min(In.ViewH-8,In.MinimapTop-MinimapFrame-MinimapGap);
	auto Intersects=[](const FBox2D& A,const FBox2D& B) { return A.bIsValid && B.bIsValid && A.Min.X<B.Max.X+.01 && A.Max.X>B.Min.X-.01 && A.Min.Y<B.Max.Y+.01 && A.Max.Y>B.Min.Y-.01; };
	TArray<FBox2D> Used=In.Avoid;
	auto Clear=[&](const FBox2D& B)
	{
	 if (B.Min.X<8 || B.Min.Y<8 || B.Max.X>In.ViewW-8+.01 || B.Max.Y>Bottom+.01) return false;
	 return !Used.ContainsByPredicate([&](const FBox2D& A) { return Intersects(B,A); });
	};
	float Total=0;
	for (int32 I=0; I<Blocks.Num(); I++) Total+=(I?Gap:0)+Blocks[I].H;
	float Y=Bottom-Total;
	bool Original=true;
	for (const FBlock& B : Blocks)
	{
	 if (!Clear(FBox2D(FVector2D(Right-B.W,Y),FVector2D(Right,Y+B.H)))) { Original=false; break; }
	 Y+=B.H+Gap;
	}
	if (Original)
	{
	 Y=Bottom-Total;
	 for (const FBlock& B : Blocks) { B.Paint(P,Right-B.W,Y); Y+=B.H+Gap; }
	}
	else
	{
	 TArray<int32> Order;
	 if (Blocks.Num()) Order.Add(Blocks.Num()-1); // history remains reachable even during a burst
	 for (int32 I=0; I<Blocks.Num()-1; I++) Order.Add(I); // newest losses, then routine lines
	 for (int32 I : Order)
	 {
	  const FBlock& B=Blocks[I];
	  TArray<float> Xs={Right-B.W,8.f}, Ys={Bottom-B.H,8.f};
	  for (const FBox2D& A : Used) if (A.bIsValid) { Xs.Add(A.Min.X-B.W-6); Xs.Add(A.Max.X+6); Ys.Add(A.Min.Y-B.H-6); Ys.Add(A.Max.Y+6); }
	  bool Placed=false;
	  for (float X : Xs)
	  {
	   for (float Top : Ys)
	   {
	    const FBox2D Box(FVector2D(X,Top),FVector2D(X+B.W,Top+B.H));
	    if (!Clear(Box)) continue;
	    B.Paint(P,X,Top); Used.Add(Box); Placed=true; break;
	   }
	   if (Placed) break;
	  }
	 }
	}

    for (FTCPaintItem& Item : P.Items) if (Item.Role==TEXT("alert:loss-shadow"))
    {
        const FVector2D End(FMath::Min(double(In.ViewW),Item.Pos.X+Item.Size.X),FMath::Min(double(In.ViewH),Item.Pos.Y+Item.Size.Y));
        Item.Pos=FVector2D(FMath::Max(0.0,Item.Pos.X),FMath::Max(0.0,Item.Pos.Y)); Item.Size=End-Item.Pos;
    }
	return P;
}

void FTCAlerts::UnderAttack(FVector2D At, const FString& Text, const TCHAR* Kind, const FTCAlertContext& Ctx, TArray<FTCAlertPush>& Out)
{
	// alerts.js underAttack: once per AREA per ATTACK_EVERY, none for a spot on screen (no cooldown then)
	TCWorldRecoveryCore::FAttackQuiet& Quiet = Ctx.Quiet ? *Ctx.Quiet : OwnQuiet;
	const bool bOnScreen = Ctx.OnScreen && Ctx.OnScreen(At.X, At.Y);
	if (!Quiet.Allow(Ctx.Now, At.X, At.Y, bOnScreen)) return;
	Out.Add(Push(Kind, Text, true, At, Ctx.Now));
}

void FTCAlerts::ReadOrder(const FTCMatchState& S, const FJsonObject* Raw)
{
	// client/world-state.js apply: all (or no held) clears, gone deletes, rows set (a new id goes last)
	if (Raw)
	{
		bool bAll = false;
		Raw->TryGetBoolField(TEXT("all"), bAll);
		const TArray<TSharedPtr<FJsonValue>>* Held = ArrayField(Raw, TEXT("held"));
		if (bAll || !Held || Held->Num() < 2) { RowOrder.Reset(); RowSet.Reset(); RowXZ.Reset(); RowBuilt.Reset(); }
		if (const TArray<TSharedPtr<FJsonValue>>* Gone = ArrayField(Raw, TEXT("gone")))
			for (const TSharedPtr<FJsonValue>& V : *Gone) { double D; if (Number(V, D)) { RowXZ.Remove((int32)D); RowBuilt.Remove((int32)D); if (RowSet.Remove((int32)D)) RowOrder.Remove((int32)D); } }
		if (const TArray<TSharedPtr<FJsonValue>>* Rows = ArrayField(Raw, TEXT("units")))
			for (const TSharedPtr<FJsonValue>& V : *Rows)
			{
				const TArray<TSharedPtr<FJsonValue>>* R = nullptr;
				double D, X, Z;
				if (!V.IsValid() || !V->TryGetArray(R) || !R->Num() || !Number((*R)[0], D)) continue;
				const int32 Id = (int32)D;
				if (!RowSet.Contains(Id)) { RowSet.Add(Id); RowOrder.Add(Id); }
				if (R->Num() > 4 && Number((*R)[3], X) && Number((*R)[4], Z)) RowXZ.Add(Id, FVector2D(X, Z));
				if (R->Num() > 14 && Number((*R)[14], D)) RowBuilt.Add(Id, D);
			}
	}
	// held rows the order does not know (no raw message, or an unreadable row's id): by id, at the end
	TArray<int32> Missing;
	for (const auto& Pair : S.Units) if (!RowSet.Contains(Pair.Key)) Missing.Add(Pair.Key);
	Missing.Sort();
	for (int32 Id : Missing) { RowSet.Add(Id); RowOrder.Add(Id); }
	for (int32 I = RowOrder.Num() - 1; I >= 0; I--) if (!S.Units.Contains(RowOrder[I])) { RowSet.Remove(RowOrder[I]); RowXZ.Remove(RowOrder[I]); RowBuilt.Remove(RowOrder[I]); RowOrder.RemoveAt(I); }
	for (auto It = RowXZ.CreateIterator(); It; ++It) if (!S.Units.Contains(It.Key())) It.RemoveCurrent();
	for (auto It = RowBuilt.CreateIterator(); It; ++It) if (!S.Units.Contains(It.Key())) It.RemoveCurrent();
}

TArray<FTCAlertPush> FTCAlerts::Snapshot(const FTCMatchState& S, const FJsonObject* Raw, const FTCAlertContext& Ctx)
{
	TArray<FTCAlertPush> Out;
	if (!S.bHasMatch) return Out;
	if (S.MatchId != MatchSeen) StartMatch(S); // a start this feed was not told about
	ReadOrder(S, Raw);
	const int32 Me = S.You;
	const bool bOut = S.OutSeats.IsValidIndex(Me) && S.OutSeats[Me];
	const int32 MyTeam = S.TeamOf(Me);
	auto Friend = [&S, MyTeam](int32 Slot) { return Slot >= 0 && S.TeamOf(Slot) == MyTeam; };
	auto Def = [&Ctx](const FString& Type) { return Ctx.Table ? Ctx.Table->Find(Type) : nullptr; };
	const double Now = Ctx.Now;
	const TArray<FTCCapturePoint>& Pts = S.Objectives.Points();
	auto PointPos = [&Pts](int32 I, FVector2D& At) { if (!Pts.IsValidIndex(I)) return false; At = Pts[I].Centre; return true; };

	// raw facts the decoder does not keep: strikes, queue heads, the say lines' text, home
	struct FStrike { FString Kind; double X = 0, Z = 0; int32 Owner = -1; };
	TArray<FStrike> Strikes;
	if (const TArray<TSharedPtr<FJsonValue>>* A = ArrayField(Raw, TEXT("strikes")))
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const TArray<TSharedPtr<FJsonValue>>* R = nullptr;
			double X, Z, O;
			if (!V.IsValid() || !V->TryGetArray(R) || R->Num() < 3 || !(*R)[0].IsValid() || (*R)[0]->Type != EJson::String || !Number((*R)[1], X) || !Number((*R)[2], Z)) continue;
			FStrike St; St.Kind = (*R)[0]->AsString().Left(64); St.X = X; St.Z = Z; St.Owner = R->Num() > 5 && Number((*R)[5], O) ? (int32)O : -1;
			if (Strikes.Num() < 256) Strikes.Add(MoveTemp(St));
		}
	TArray<TPair<int32, FString>> QueueHeads;
	if (const TArray<TSharedPtr<FJsonValue>>* A = ArrayField(Raw, TEXT("queues")))
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const TArray<TSharedPtr<FJsonValue>>* R = nullptr;
			double Id;
			if (!V.IsValid() || !V->TryGetArray(R) || R->Num() < 5 || !Number((*R)[0], Id) || !(*R)[4].IsValid() || (*R)[4]->Type != EJson::String || (*R)[4]->AsString().IsEmpty()) continue;
			if (QueueHeads.Num() < 4096) QueueHeads.Add(TPair<int32, FString>((int32)Id, (*R)[4]->AsString().Left(64)));
		}
	const TArray<FTCShotExtra> Extras = TCShotExtras::Read(Raw);

	// main.js: if (!operative && !watching && !s.out?.[me]) alerts.snapshot(s, lastSnap). A seat is this client's only
	// once a lobby confirms it (FTCMatchState::SeatConfirmed); a spectator never is.
	if (S.SeatConfirmed() && !bOut && Hist.AcceptTick(S.Tick))
	{
		MatchTime = S.Tick / 20.0;
		if (S.Winner < -1)
		{
			// the map's scripted events speak to everyone: sh.localized?.[lang] ?? sh.text (lang: the page's, 'es' in
			// Spanish, else 'en'; FTCLocale::Language). The line is drawn through TC_T like every alert.
			const bool bSpanish = FTCLocale::Language() == ETCLanguage::Spanish;
			for (int32 I = 0; I < S.Shots.Num(); I++)
			{
				const FTCShot& Sh = S.Shots[I];
				if (Sh.K != TEXT("say")) continue;
				const FTCShotExtra* E = Extras.IsValidIndex(I) ? &Extras[I] : nullptr;
				const FString* Localized = !E ? nullptr : bSpanish ? (E->bHasLocalizedEs ? &E->LocalizedEs : nullptr) : (E->bHasLocalizedEn ? &E->LocalizedEn : nullptr);
				const FString Text = Localized ? *Localized : E && E->bHasText ? E->Text : FString();
				Out.Add(Push(TEXT("event"), Text, Sh.bHasPos, FVector2D(Sh.X, Sh.Z), Now));
			}
			if (bHasPrev)
			{
				TMultiMap<int32, const FTCShot*> ShotsAt;
				for (const FTCShot& Sh : S.Shots) if (Sh.bHasTarget) ShotsAt.Add(Sh.T, &Sh);
				auto NameOf = [&](const FString& Type, int32 Owner) { return (Owner == Me ? FString() : FString(TEXT("Allied "))) + UnitName(S, Ctx.Table, Type, Owner); };

				// under attack: own units and buildings, an allied HQ or bunker, losing health to something not friendly
				TArray<FSpot> Hits;
				for (const int32 Id : RowOrder)
				{
					const FTCUnit& U = S.Units[Id];
					const FRow* P = PrevRows.Find(Id);
					const FTCUnitDef* D = Def(U.Type);
					if (!P || !D || P->Owner != U.Owner || U.Hp >= P->Hp) continue;
					if (U.Owner != Me && !(Friend(U.Owner) && (U.Type == TEXT("hq") || U.Type == TEXT("bunker")))) continue;
					TArray<const FTCShot*> By;
					ShotsAt.MultiFind(Id, By, true);
					if (By.Num() && !By.ContainsByPredicate([&](const FTCShot* Q) { return !(Q->bHasShooterOwner && Friend(Q->Fo)); })) continue; // own side's fire
					const bool bProduces = U.Type == TEXT("hq") || U.Type == TEXT("barracks") || U.Type == TEXT("motorpool") || U.Type == TEXT("airfield") || U.Type == TEXT("shipyard") || U.Type == TEXT("worldbase");
					if (!By.Num() && S.ModeRow.bSuddenDeath && bProduces) continue; // Sudden Death crumbling, not an enemy
					Hits.Add({ U.Type, U.Owner, PosOf(U) });
				}
				for (const TArray<FSpot>& G : Groups(Hits))
				{
					const FSpot* Big = G.FindByPredicate([&](const FSpot& H) { const FTCUnitDef* D = Def(H.Type); return D && D->bStructure; });
					const FString Text = Big ? NameOf(Big->Type, Big->Owner) + TEXT(" under attack") : G.Num() == 1 ? NameOf(G[0].Type, G[0].Owner) + TEXT(" under attack") : FString(TEXT("Units under attack"));
					const FVector2D At = Big ? Big->At : Mid(G);
					if (bHasHome && (Near(At, Home, TCAlertRules::BaseRadius) || (Big && Big->Owner == Me))) UnderAttack(At, TEXT("Our base is under attack!"), TEXT("base"), Ctx, Out);
					else UnderAttack(At, Text, TEXT("attack"), Ctx, Out);
				}

				// points: captured, lost, under attack, or cut off
				if (bPointAlerts)
					for (int32 I = 0; I < S.Points.Num(); I++)
					{
						FVector2D At;
						if (!PrevPoints.IsValidIndex(I) || !PointPos(I, At)) continue;
						const FTCPointRow& Was = PrevPoints[I];
						const FTCPointRow& R = S.Points[I];
						const bool bHad = Friend(Was.Owner), bHas = Friend(R.Owner);
						if (bHad && !bHas) Out.Add(Push(TEXT("pointLost"), TEXT("Point lost"), true, At, Now));
						else if (!bHad && bHas) Out.Add(Push(TEXT("pointWon"), R.Owner == Me ? FString(TEXT("Point captured")) : FString::Printf(TEXT("%s captured a point"), S.Names.IsValidIndex(R.Owner) ? *S.Names[R.Owner] : TEXT("An ally")), true, At, Now));
						else if (bHas && R.Progress < Was.Progress) UnderAttack(At, TEXT("Point under attack"), TEXT("attack"), Ctx, Out);
						else if (bHas && R.bCut && !Was.bCut) Out.Add(Push(TEXT("pointLost"), TEXT("Point cut off: it pays nothing until the road to it is open"), true, At, Now));
					}

				// unit lost: an own unit left the rows (own units are always in them while they exist)
				TArray<FSpot> Gone;
				for (const int32 Id : PrevOrder)
				{
					if (S.Units.Contains(Id)) continue;
					const FRow& P = PrevRows[Id];
					if (P.Owner != Me || !Def(P.Type) || P.Type == TEXT("truck")) continue;
					TArray<const FTCShot*> By;
					ShotsAt.MultiFind(Id, By, true);
					if (!By.ContainsByPredicate([](const FTCShot* Q) { return Q->bKill; }))
					{
						if (P.Built < 1) continue; // a site cancelled
						if ((P.Flags & 1) && bHasHome && Near(FVector2D(P.X, P.Z), Home, TCAlertRules::HomeRadius)) continue; // retreated off the field
					}
					Gone.Add({ P.Type, P.Owner, FVector2D(P.X, P.Z) });
				}
				for (const TArray<FSpot>& G : Groups(Gone))
				{
					const FVector2D At = Mid(G);
					const int32 Recent = LineList.IndexOfByPredicate([&](const FTCAlertLine& L) { return L.IsLoss() && Now - L.Born < TCAlertRules::Merge && L.Alert.bHasAt && Near(L.Alert.At, At, TCAlertRules::Area); });
					const int32 N = G.Num() + (Recent >= 0 ? LineList[Recent].Alert.N : 0);
					const FTCUnitDef* D = Def(G[0].Type);
					const FString Text = N > 1 ? FString::Printf(TEXT("%d units lost"), N) : UnitName(S, Ctx.Table, G[0].Type, Me) + (D && D->bStructure ? TEXT(" destroyed") : TEXT(" lost"));
					if (Recent >= 0) LineList.RemoveAt(Recent);
					Out.Add(Push(TEXT("unitLost"), Text, true, At, Now, N));
				}

				// enemy Air Support incoming: a new enemy strike mark (strikes are public) near anything of this side's
				for (const FStrike& St : Strikes)
				{
					if (PrevStrikes.ContainsByPredicate([&](const FStrikeKey& K) { return K.Kind == St.Kind && K.X == St.X && K.Z == St.Z; })) continue;
					if (St.Kind == TEXT("artillery") || St.Kind == TEXT("smoke") || St.Kind == TEXT("cover") || Friend(St.Owner)) continue;
					const FTCSupportDef* Sp = FTCSupportPowers::Find(St.Kind);
					// sp.len ? len / 2 : sp.radius ?? 20 (only Fighter Cover has a radius, 40 m, and it is never an air threat)
					const double Reach = (Sp && Sp->Len > 0 ? Sp->Len / 2 : 20.0) + TCAlertRules::AirMargin;
					const FVector2D At(St.X, St.Z);
					bool bMine = false;
					for (const auto& Pair : S.Units) if (Friend(Pair.Value.Owner) && Near(PosOf(Pair.Value), At, Reach)) { bMine = true; break; }
					for (int32 I = 0; !bMine && I < S.Points.Num(); I++) { FVector2D P; if (Friend(S.Points[I].Owner) && PointPos(I, P) && Near(P, At, Reach)) bMine = true; }
					if (!bMine && bHasHome && Near(Home, At, Reach)) bMine = true;
					if (bMine) Out.Add(Push(TEXT("air"), FString::Printf(TEXT("Enemy %s incoming"), Sp ? *Sp->Name : *St.Kind), true, At, Now));
				}

				// a unit out of a production queue, a building finished
				TArray<FString> Heads;
				for (const TPair<int32, FString>& Q : PrevQueueHeads) if (S.Units.Contains(Q.Key)) Heads.Add(Q.Value);
				for (const int32 Id : RowOrder)
				{
					const FTCUnit& U = S.Units[Id];
					const FTCUnitDef* D = Def(U.Type);
					if (U.Owner != Me || !D) continue;
					const FRow* P = PrevRows.Find(Id);
					if (!P)
					{
						const int32 I = Heads.IndexOfByKey(U.Type);
						if (I >= 0 && !D->bStructure) { Heads.RemoveAt(I); Out.Add(Push(TEXT("ready"), UnitName(S, Ctx.Table, U.Type, Me) + TEXT(" ready"), true, PosOf(U), Now)); }
					}
					else if (D->bBuilding && P->Built < 1 && BuiltOf(U) >= 1) Out.Add(Push(TEXT("ready"), UnitName(S, Ctx.Table, U.Type, Me) + TEXT(" finished"), true, PosOf(U), Now));
				}
			}
		}
	}

	// main.js: lastSnap = s; then home from s.home ?? s.world.home
	bHasPrev = true;
	LastSnapTick = S.Tick;
	PrevOrder = RowOrder;
	PrevRows.Reset();
	for (const auto& Pair : S.Units) { const FVector2D P = PosOf(Pair.Value); PrevRows.Add(Pair.Key, { Pair.Value.Type, Pair.Value.Owner, P.X, P.Y, Pair.Value.Hp, Pair.Value.Flags, BuiltOf(Pair.Value) }); }
	PrevPoints = S.Points;
	PrevStrikes.Reset();
	for (const FStrike& St : Strikes) PrevStrikes.Add({ St.Kind, St.X, St.Z });
	PrevQueueHeads = MoveTemp(QueueHeads);
	if (Raw)
	{
		const TArray<TSharedPtr<FJsonValue>>* H = ArrayField(Raw, TEXT("home"));
		if (!H) { const TSharedPtr<FJsonObject>* W; if (Raw->TryGetObjectField(TEXT("world"), W)) H = ArrayField(W->Get(), TEXT("home")); }
		double X, Z;
		if (H && H->Num() >= 2 && Number((*H)[0], X) && Number((*H)[1], Z)) { Home = FVector2D(X, Z); bHasHome = true; }
	}
	return Out;
}

bool FTCAlerts::ReceivePing(const FJsonObject& M, const FTCMatchState& S, double Now, FTCAlertPush& Out)
{
	FTCPingRing R;
	if (!PingRings.Receive(M, Now, R)) return false;
	if (Sounds.Num() < 64) Sounds.Add(TEXT("ui:click"));
	const FString Name = S.Names.IsValidIndex(R.From) ? S.Names[R.From] : FString(TEXT("An ally"));
	Out = Push(TEXT("ping"), FString::Printf(TEXT("Ping from %s"), *Name), true, FVector2D(R.X, R.Z), Now);
	return true;
}

void FTCAlerts::Frame(double Now)
{
	Echoes.RemoveAll([Now](const FEcho& E) { return Now - E.Now > 2.0; });
	LineList.RemoveAll([Now](const FTCAlertLine& L) { return TCAlertRules::LifeOf(L.Alert.Kind) - (Now - L.Born) <= 0; });
	PingRings.Frame(Now);
}

float FTCAlerts::Alpha(const FTCAlertLine& L, double Now)
{
	const double Left = TCAlertRules::LifeOf(L.Alert.Kind) - (Now - L.Born);
	return Left <= 0 ? 0.f : Left < TCAlertRules::Fade ? (float)(Left / TCAlertRules::Fade) : 1.f;
}

bool FTCAlerts::Newest(FVector2D& Out) const
{
	const FTCAlertLine* A = LineList.FindByPredicate([](const FTCAlertLine& L) { return L.IsLoss() && L.Alert.bHasAt; });
	if (!A) A = LineList.FindByPredicate([](const FTCAlertLine& L) { return L.Alert.bHasAt; });
	if (!A) return false;
	Out = A->Alert.At;
	return true;
}

// ---------------------------------------------------------------------------------------------------------------------
// The game mode's glue (declared in Client/TCGameMode.h): client/main.js's calls into alerts.js, pings.js and fx.js

void ATCGameMode::OnCombatStart()
{
	UWorld* W = GetWorld();
	if (!Effects && W)
	{
		FActorSpawnParameters P;
		P.SpawnCollisionHandlingOverride = ESpawnActorCollisionHandlingMethod::AlwaysSpawn;
		Effects = W->SpawnActor<ATCEffects>(ATCEffects::StaticClass(), FTransform::Identity, P);
	}
	if (Effects) Effects->Bind(&State, &UnitTable, Terrain, Units, &AlertFeed);
	// main.js startGame: alerts.startMatch (a new match id empties the history), pings.reset, effects reset
	AlertFeed.bPointAlerts = false; // TakePointAlerts raises the point alerts already
	AlertFeed.StartMatch(State);
	if (Effects) Effects->OnStart(State);
	ShooterLogSeen = 0;
}

void ATCGameMode::OnCombatSnapshot(const FJsonObject& M)
{
	// alerts.js snapshot against the last snapshot, through the throttle the point and region alerts share
	FTCAlertContext Ctx;
	Ctx.Now = FPlatformTime::Seconds();
	Ctx.OnScreen = [this](double X, double Z) { return SpotOnScreen(X, Z); };
	Ctx.Quiet = &AttackQuiet;
	Ctx.Table = &UnitTable;
	AlertFeed.Frame(Ctx.Now);
	for (const FTCAlertPush& A : AlertFeed.Snapshot(State, &M, Ctx)) PushAlert(*A.Kind, A.Text, A.At);
	// the feed's alert sounds are played by PushAlert (alerts.js push: sound(kind)), which every alert above went through
	for (const FString& Sound : AlertFeed.TakeSounds())
		if (Sound.StartsWith(TEXT("ui:")))
			if (UTCAudio* Au = Audio()) Au->Ui(Sound.RightChop(3));
	// fx.js, projectiles.js and debris.js from what this snapshot delivered
	if (Effects) Effects->OnSnapshot(State, M);
	// client/fx.js privacy: an effect at a shooter only when the shooter is in this snapshot's rows. Every shooter-side
	// effect made since the last snapshot is checked against the rows this client holds now (it must be among them).
	if (Effects)
	{
		const TArray<TPair<int32, int32>>& Log = Effects->Fx.ShooterLog();
		if (ShooterLogSeen > Log.Num()) ShooterLogSeen = 0;
		for (; ShooterLogSeen < Log.Num(); ShooterLogSeen++)
		{
			FxPrivacyChecked++;
			const TPair<int32, int32>& E = Log[ShooterLogSeen];
			if (State.Units.Contains(E.Value)) continue;
			FxPrivacyViolations++;
			UE_LOG(LogTC, Warning, TEXT("fx privacy: a shooter-side effect at tick %d names unit %d, which this snapshot (tick %d) does not hold"), E.Key, E.Value, State.Tick);
		}
	}
	// client/main.js applySnapshot's sounds: kill marks, air shots, projectiles, fx.js snapshot (Client/TCAudio.h)
	if (UTCAudio* Au = Audio()) Au->Snapshot(State, UnitTable);
	const FTCCombatEffects::FStats& C = Effects ? Effects->Fx.Stats() : FTCCombatEffects::FStats();
	if (Effects && State.Snapshots % 200 == 1)
		UE_LOG(LogTC, Log, TEXT("combat effects at tick %d: %d launches flashed, %d not (shooter unseen), %d contacts, %d direct fire (%d unseen shooter), %d blasts, %d wrecks, %d air shots, %d particles, %d dropped"),
			State.Tick, C.Launches, C.LaunchesHidden + C.ShooterNotHeld, C.Contacts, C.DirectFire, C.DirectFireHidden, C.Blasts, C.Wrecks, C.AirShots, Effects->Fx.Particles().Num(), C.Dropped);
}

void ATCGameMode::OnPingRelay(const FJsonObject& M)
{
	// client/pings.js receive: a ring at the spot for 4 s, the click and a "Ping from <name>" line
	FTCAlertPush A;
	if (!State.bHasMatch || !AlertFeed.ReceivePing(M, State, FPlatformTime::Seconds(), A))
	{
		UE_LOG(LogTC, Warning, TEXT("ping relay ignored (%s)"), State.bHasMatch ? TEXT("malformed") : TEXT("no match"));
		return;
	}
	// pings.js receive: audio.ui('click'), then alerts.push('ping', ...) (which plays no alert sound for a ping)
	for (const FString& Sound : AlertFeed.TakeSounds())
		if (Sound.StartsWith(TEXT("ui:")))
			if (UTCAudio* Au = Audio()) Au->Ui(Sound.RightChop(3));
	UE_LOG(LogTC, Log, TEXT("ping relay: %s at (%.1f, %.1f), ring for %.0f s"), *A.Text, A.At.X, A.At.Y, FTCPings::Life);
	PushAlert(TEXT("ping"), A.Text, A.At);
}

void ATCGameMode::ClearCombat()
{
	// the room went back to the lobby: nothing of the fight stays drawn (the alert history waits for the next start)
	if (Effects) Effects->Reset();
}

// ---------------------------------------------------------------------------------------------------------------------
// Automation tests

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "WW2ViewReplay.h"

namespace TCAlertTest
{
	// 36 messages, generated by the oracle tools (gen-*.mjs); every value is plain ASCII JSON
	const char* const SyntheticAlerts[] = {
		R"TC({"t":"lobby","code":"synthetic","state":"play","mode":"conquest","you":0,"spectator":false,"amHost":true,"players":[{"name":"Me","team":0,"connected":true},{"name":"Ally","team":0,"connected":true},{"name":"Foe","team":1,"connected":true}]})TC",
		R"TC({"t":"start","matchId":1,"you":0,"spawn":{"x":20,"z":20},"spawns":[{"x":20,"z":20},{"x":180,"z":180},{"x":180,"z":20}],"names":["Me","Ally","Foe"],"teams":[0,0,1],"factions":[0,1,2],"map":{"name":"synthetic","w":100,"h":100,"rows":["","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","",""],"points":[{"x":50,"y":50},{"x":80,"y":20},{"x":20,"y":80}]}})TC",
		R"TC({"t":"s","tick":2,"mp":100,"units":[[100,"hq",0,22,22,0,0,1500,0,0,0,0,0,0,1],[101,"rifle",0,60,60,0,0,100,0,0,0,0,0,0,1],[102,"rifle",0,62,61,0,0,100,0,0,0,0,0,0,1],[103,"tank",0,120,120,0,0,360,0,0,0,0,0,0,1],[104,"truck",0,30,30,0,0,200,0,0,0,0,0,0,1],[105,"mg",0,130,40,0,0,75,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,0.5],[107,"depot",0,45,45,0,0,300,0,0,0,0,0,0,0.3],[108,"rifle",0,64,62,0,0,100,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1500,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,100,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,360,0,0,0,0,0,0,1]],"held":[12,321],"gone":[],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[{"k":"say","text":"Hold the crossroads","localized":{"en":"Hold the crossroads!","es":"Mantened el cruce"},"x":100,"z":100,"pub":true},{"k":"say","text":"No place","pub":true}]})TC",
		R"TC({"t":"s","tick":4,"mp":100,"units":[[100,"hq",0,22,22,0,0,1500,0,0,0,0,0,0,1],[101,"rifle",0,60,60,0,0,80,0,0,0,0,0,0,1],[102,"rifle",0,62,61,0,0,90,0,0,0,0,0,0,1],[103,"tank",0,120,120,0,0,360,0,0,0,0,0,0,1],[104,"truck",0,30,30,0,0,200,0,0,0,0,0,0,1],[105,"mg",0,130,40,0,0,75,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,0.5],[107,"depot",0,45,45,0,0,300,0,0,0,0,0,0,0.3],[108,"rifle",0,64,62,0,0,100,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1500,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1]],"held":[12,321],"gone":[],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[{"k":"tank","f":300,"fo":2,"t":101,"to":0,"x":60,"z":60,"hit":true}]})TC",
		R"TC({"t":"s","tick":6,"mp":100,"units":[[100,"hq",0,22,22,0,0,1500,0,0,0,0,0,0,1],[101,"rifle",0,60,60,0,0,80,0,0,0,0,0,0,1],[102,"rifle",0,62,61,0,0,90,0,0,0,0,0,0,1],[103,"tank",0,120,120,0,0,300,0,0,0,0,0,0,1],[104,"truck",0,30,30,0,0,200,0,0,0,0,0,0,1],[105,"mg",0,130,40,0,0,50,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,0.5],[107,"depot",0,45,45,0,0,300,0,0,0,0,0,0,0.3],[108,"rifle",0,64,62,0,0,100,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1500,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1]],"held":[12,321],"gone":[],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[{"k":"rifle","f":201,"fo":1,"t":103,"to":0,"x":120,"z":120,"hit":true},{"k":"tank","fo":2,"t":105,"to":0,"x":130,"z":40,"hit":true}]})TC",
		R"TC({"t":"s","tick":8,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[101,"rifle",0,60,60,0,0,80,0,0,0,0,0,0,1],[102,"rifle",0,62,61,0,0,90,0,0,0,0,0,0,1],[103,"tank",0,120,120,0,0,300,0,0,0,0,0,0,1],[104,"truck",0,30,30,0,0,200,0,0,0,0,0,0,1],[105,"mg",0,130,40,0,0,50,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,0.5],[107,"depot",0,45,45,0,0,300,0,0,0,0,0,0,0.3],[108,"rifle",0,64,62,0,0,100,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1500,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1]],"held":[12,321],"gone":[],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[{"k":"tank","f":300,"fo":2,"t":100,"to":0,"x":22,"z":22,"hit":true}]})TC",
		R"TC({"t":"s","tick":10,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[101,"rifle",0,60,60,0,0,60,0,0,0,0,0,0,1],[102,"rifle",0,62,61,0,0,90,0,0,0,0,0,0,1],[103,"tank",0,120,120,0,0,300,0,0,0,0,0,0,1],[104,"truck",0,30,30,0,0,200,0,0,0,0,0,0,1],[105,"mg",0,130,40,0,0,50,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,0.5],[107,"depot",0,45,45,0,0,300,0,0,0,0,0,0,0.3],[108,"rifle",0,64,62,0,0,100,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1]],"held":[12,321],"gone":[],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[{"k":"tank","f":300,"fo":2,"t":101,"to":0,"x":60,"z":60,"hit":true},{"k":"tank","f":300,"fo":2,"t":200,"to":1,"x":180,"z":180,"hit":true}]})TC",
		R"TC({"t":"s","tick":12,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[103,"tank",0,120,120,0,0,300,0,0,0,0,0,0,1],[105,"mg",0,130,40,0,0,50,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,0.5],[108,"rifle",0,64,62,0,0,100,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1]],"held":[8,321],"gone":[101,102,104,107],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[{"k":"tank","f":300,"fo":2,"t":101,"to":0,"x":60,"z":60,"hit":true,"kill":true},{"k":"tank","f":300,"fo":2,"t":102,"to":0,"x":62,"z":61,"hit":true,"kill":true}]})TC",
		R"TC({"t":"s","tick":14,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[103,"tank",0,120,120,0,0,300,0,0,0,0,0,0,1],[105,"mg",0,130,40,0,0,50,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,0.5],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1]],"held":[7,301],"gone":[108],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[{"k":"tank","f":300,"fo":2,"t":108,"to":0,"x":64,"z":62,"hit":true,"kill":true}]})TC",
		R"TC({"t":"s","tick":16,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[105,"mg",0,24,24,0,0,50,0,0,0,0,1,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,0.5],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1]],"held":[6,330],"gone":[103],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[{"k":"rifle","fo":2,"t":103,"to":0,"x":120,"z":120,"hit":true,"kill":true}]})TC",
		R"TC({"t":"s","tick":18,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1]],"held":[5,291],"gone":[105],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[],"queues":[[100,0.5,-1,-1,"rifle","mg"],[106,0,-1,-1]]})TC",
		R"TC({"t":"s","tick":20,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,360,0,0,0,0,0,0,1]],"held":[7,288],"gone":[],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[0,-1,1,0,0,0]],"shots":[],"queues":[[100,0,-1,-1,"mg"]]})TC",
		R"TC({"t":"s","tick":22,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,360,0,0,0,0,0,0,1]],"held":[7,288],"gone":[],"points":[[0,-1,1,0,0,0],[1,-1,1,0,0,0],[0,-1,1,0,0,0]],"shots":[]})TC",
		R"TC({"t":"s","tick":24,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,360,0,0,0,0,0,0,1]],"held":[7,288],"gone":[],"points":[[0,-1,1,0,0,0],[1,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[]})TC",
		R"TC({"t":"s","tick":26,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,360,0,0,0,0,0,0,1]],"held":[7,288],"gone":[],"points":[[0,-1,1,0,1,0],[1,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[]})TC",
		R"TC({"t":"s","tick":28,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,360,0,0,0,0,0,0,1]],"held":[7,288],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[]})TC",
		R"TC({"t":"s","tick":30,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,360,0,0,0,0,0,0,1]],"held":[7,288],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[],"strikes":[["strafe",125,125,0,5,2],["artillery",120,120,0,5,2],["bombing",190,10,0,5,2],["dive",60,60,0,5,1]]})TC",
		R"TC({"t":"s","tick":32,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,360,0,0,0,0,0,0,1]],"held":[7,288],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[],"strikes":[["strafe",125,125,0,4.9,2],["para",100,100,1,5,2],["recon",20,20,0,3,2],["cover",120,125,0,2,2]]})TC",
		R"TC({"t":"s","tick":34,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,300,0,0,0,0,0,0,1]],"held":[7,288],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[{"k":"tank","f":300,"fo":2,"t":110,"to":0,"x":120,"z":120,"hit":true}]})TC",
		R"TC({"t":"s","tick":36,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[106,"barracks",0,40,22,0,0,600,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,250,0,0,0,0,0,0,1]],"held":[7,288],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[{"k":"tank","f":300,"fo":2,"t":110,"to":0,"x":120,"z":120,"hit":true}]})TC",
		R"TC({"t":"s","tick":38,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,250,0,0,0,0,0,0,1]],"held":[6,330],"gone":[106],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[{"k":"tank","f":300,"fo":2,"t":106,"to":0,"x":40,"z":22,"hit":true,"kill":true}]})TC",
		R"TC({"t":"s","tick":38,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,250,0,0,0,0,0,0,1]],"held":[6,330],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[{"k":"say","text":"duplicate tick: never shown","x":1,"z":1}]})TC",
		R"TC({"t":"ping","from":1,"x":50.5,"z":60})TC",
		R"TC({"t":"ping","from":-1,"x":50,"z":60})TC",
		R"TC({"t":"ping","from":7,"x":10,"z":12.5})TC",
		R"TC({"t":"ping","from":1,"x":"a","z":60})TC",
		R"TC({"t":"s","tick":1000,"mp":100,"units":[[100,"hq",0,22,22,0,0,1400,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,100,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,250,0,0,0,0,0,0,1]],"held":[6,330],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[]})TC",
		R"TC({"t":"s","tick":1002,"mp":100,"units":[[100,"hq",0,22,22,0,0,1300,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,90,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,250,0,0,0,0,0,0,1]],"held":[6,330],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[],"mode":{"kind":"classic","suddenDeath":true,"timeLeft":0}})TC",
		R"TC({"t":"start","matchId":1,"you":0,"spawn":{"x":20,"z":20},"spawns":[{"x":20,"z":20},{"x":180,"z":180},{"x":180,"z":20}],"names":["Me","Ally","Foe"],"teams":[0,0,1],"factions":[0,1,2],"map":{"name":"synthetic","w":100,"h":100,"rows":["","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","",""],"points":[{"x":50,"y":50},{"x":80,"y":20},{"x":20,"y":80}]}})TC",
		R"TC({"t":"s","tick":1010,"mp":100,"units":[[100,"hq",0,22,22,0,0,1300,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,90,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,250,0,0,0,0,0,0,1]],"held":[6,330],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[]})TC",
		R"TC({"t":"s","tick":1012,"mp":100,"units":[[100,"hq",0,22,22,0,0,1300,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[109,"rifle",0,24,30,0,0,70,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,250,0,0,0,0,0,0,1]],"held":[6,330],"gone":[],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[{"k":"rifle","f":300,"fo":2,"t":109,"to":0,"x":24,"z":30,"hit":true}]})TC",
		R"TC({"t":"s","tick":1014,"mp":100,"units":[[100,"hq",0,22,22,0,0,1300,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1],[110,"tank",0,120,120,0,0,250,0,0,0,0,0,0,1]],"held":[5,295],"gone":[109],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[{"k":"rifle","f":300,"fo":2,"t":109,"to":0,"x":24,"z":30,"hit":true,"kill":true}],"out":[true,false,false]})TC",
		R"TC({"t":"s","tick":1016,"mp":100,"units":[[100,"hq",0,22,22,0,0,1300,0,0,0,0,0,0,1],[200,"hq",1,180,180,0,0,1400,0,0,0,0,0,0,1],[201,"rifle",1,150,150,0,0,90,0,0,0,0,0,0,1],[300,"tank",2,90,90,0,0,300,0,0,0,0,0,0,1]],"held":[4,329],"gone":[110],"points":[[0,-1,1,0,1,0],[2,-1,1,0,0,0],[0,-1,0.8,0,0,0]],"shots":[{"k":"rifle","f":300,"fo":2,"t":110,"to":0,"x":120,"z":120,"hit":true,"kill":true}],"winner":1})TC",
		R"TC({"t":"start","matchId":2,"you":0,"spawn":{"x":20,"z":20},"spawns":[{"x":20,"z":20},{"x":180,"z":180},{"x":180,"z":20}],"names":["Me","Ally","Foe"],"teams":[0,0,1],"factions":[0,1,2],"map":{"name":"synthetic","w":100,"h":100,"rows":["","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","",""],"points":[{"x":50,"y":50},{"x":80,"y":20},{"x":20,"y":80}]}})TC",
		R"TC({"t":"s","tick":2,"mp":100,"units":[[400,"rifle",0,50,50,0,0,100,0,0,0,0,0,0,1]],"held":[1,400],"gone":[],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[-1,-1,0,0,0,0]],"shots":[]})TC",
		R"TC({"t":"s","tick":4,"mp":100,"units":[],"held":[0,0],"gone":[400],"points":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0],[-1,-1,0,0,0,0]],"shots":[{"k":"rifle","fo":2,"t":400,"to":0,"x":50,"z":50,"hit":true,"kill":true}]})TC",
	};
	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> Obj;
		return FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Obj) && Obj.IsValid() ? Obj : nullptr;
	}
	uint64 Fnv(const FString& Text)
	{
		const FTCHARToUTF8 Utf8(*Text);
		uint64 H = 0xcbf29ce484222325ull;
		for (int32 I = 0; I < Utf8.Length(); I++) { H ^= (uint8)Utf8.Get()[I]; H *= 0x100000001b3ull; }
		return H;
	}
	FString N(double V) { return FTCObjectives::JsNumber(FMath::FloorToDouble(V * 1e6 + 0.5) / 1e6); }

	bool LoadStream(const FString& Stem, TArray<FString>& Lines)
	{
		if (Stem == TEXT("synthetic-alerts")) { for (const char* L : SyntheticAlerts) Lines.Add(UTF8_TO_TCHAR(L)); return true; }
		const FString Dir = FWW2ViewReplay::ProtocolDir();
		return !Dir.IsEmpty() && FFileHelper::LoadFileToStringArray(Lines, *(Dir / (Stem + TEXT(".jsonl"))));
	}

	// One stream through FTCMatchState and FTCAlerts as client/main.js drives client/alerts.js and client/pings.js, logged
	// as the oracle logged the browser: every alert added to the history (tick, kind, text, x, z, time) and, after
	// each snapshot (and each ping message), the lines showing and the sounds asked for. The alert clock is the
	// snapshot's tick / 20 s; nothing is on screen.
	void Replay(const TArray<FString>& Lines, const FTCUnitTable& Table, FString& AlertText, FString& LineText, FString& Problem)
	{
		FTCMatchState St;
		FTCAlerts A;
		TArray<FString> AlertLog, LineLog;
		int32 LastId = 0, LastTick = -1, Match = INT32_MIN;
		auto Collect = [&](int32 Tick)
		{
			const TArray<FTCAlertHistoryEntry>& H = A.History().Entries();
			for (int32 I = H.Num() - 1; I >= 0; I--)
				if (H[I].Id > LastId)
				{
					LastId = H[I].Id;
					AlertLog.Add(FString::Printf(TEXT("%d|%s|%s|%s|%s|%s"), Tick, *H[I].Kind, *H[I].Text, H[I].bHasAt ? *N(H[I].At.X) : TEXT("-"), H[I].bHasAt ? *N(H[I].At.Y) : TEXT("-"), *FTCObjectives::JsNumber(H[I].Time)));
				}
			TArray<FString> Texts;
			for (const FTCAlertLine& L : A.Lines()) Texts.Add(L.Alert.Text);
			LineLog.Add(FString::Printf(TEXT("%d|%s|%s"), Tick, *FString::Join(Texts, TEXT(" / ")), *FString::Join(A.TakeSounds(), TEXT(","))));
		};
		for (int32 L = 0; L < Lines.Num(); L++)
		{
			if (Lines[L].IsEmpty()) continue;
			const TSharedPtr<FJsonObject> M = Parse(Lines[L]);
			if (!M) { Problem = FString::Printf(TEXT("line %d does not parse"), L + 1); return; }
			const FString T = M->GetStringField(TEXT("t"));
			if (T == TEXT("lobby")) St.ApplyLobby(*M);
			else if (T == TEXT("start"))
			{
				St.ApplyStart(*M);
				if (St.MatchId != Match) LastId = 0;
				Match = St.MatchId;
				A.StartMatch(St);
				LastTick = -1;
			}
			else if (T == TEXT("ping"))
			{
				A.TakeSounds();
				FTCAlertPush P;
				A.ReceivePing(*M, St, (LastTick < 0 ? 0 : LastTick) / 20.0, P);
				Collect(LastTick);
			}
			else if (T == TEXT("s"))
			{
				St.ApplySnapshot(*M);
				A.TakeSounds();
				FTCAlertContext Ctx;
				Ctx.Now = St.Tick / 20.0;
				Ctx.OnScreen = [](double, double) { return false; };
				Ctx.Table = &Table;
				A.Snapshot(St, M.Get(), Ctx);
				A.Frame(Ctx.Now);
				Collect(St.Tick);
				LastTick = St.Tick;
			}
		}
		AlertText = FString::Join(AlertLog, TEXT("\n"));
		LineText = FString::Join(LineLog, TEXT("\n"));
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAlertsRecordedTest, "ThreeCrossroads.Alerts.Recorded", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCAlertsRecordedTest::RunTest(const FString& Parameters)
{
	// The oracle: reference/ww2-rts at e4e579a, client/alerts.js, client/alert-history.js, client/pings.js and
	// client/world-state.js run by node on the same streams with client/main.js's hooks (axel-gaps-6-7/oracle), the
	// history and the lines logged; a hash of each log per stream. The synthetic stream reaches what the recorded ones
	// do not: losses and their merge, points, air, ready and finished, say lines, pings, duplicate ticks, a reconnect,
	// Sudden Death, a seat out, a winner and a new match.
	using namespace TCAlertTest;
	struct FExpect { const TCHAR* Stem; int32 Alerts; uint64 AlertsFnv, LinesFnv; };
	const FExpect Streams[] = {
		{ TEXT("protocol-conquest-p0"), 1, 0x175897ae3ea1f72eull, 0xa8097c3a69aa3fc8ull },
		{ TEXT("protocol-conquest-p1"), 2, 0xc4e24c5ea8e5ed71ull, 0x0f27f94b0c198838ull },
		{ TEXT("protocol-world-p0"), 0, 0xcbf29ce484222325ull, 0xe34d45037a433001ull },
		{ TEXT("protocol-world-p1"), 0, 0xcbf29ce484222325ull, 0xe34d45037a433001ull },
		{ TEXT("synthetic-alerts"), 25, 0x77a61716927b70bfull, 0x6b497016d5c1802eull },
	};
	FTCUnitTable Table;
	FString Err;
	if (!TestTrue(TEXT("units.json loads"), Table.Load(Err))) { AddError(Err); return false; }
	int32 Ran = 0;
	for (const FExpect& E : Streams)
	{
		TArray<FString> Lines;
		if (!LoadStream(E.Stem, Lines))
		{
			AddWarning(FString::Printf(TEXT("%s: recorded stream not installed; its checks were NOT run. Run unreal/Tools/ww2-ffi.sh build."), E.Stem));
			continue;
		}
		Ran++;
		FString AlertText, LineText, Problem;
		Replay(Lines, Table, AlertText, LineText, Problem);
		if (!TestTrue(FString::Printf(TEXT("%s replays"), E.Stem), Problem.IsEmpty())) { AddError(Problem); continue; }
		TArray<FString> Raised;
		AlertText.ParseIntoArray(Raised, TEXT("\n"));
		TestEqual(FString::Printf(TEXT("%s: alerts raised"), E.Stem), Raised.Num(), E.Alerts);
		const FString Dir = FPaths::ProjectSavedDir() / TEXT("TCAlertsTest");
		if (Fnv(AlertText) != E.AlertsFnv)
		{
			FFileHelper::SaveStringToFile(AlertText, *(Dir / (FString(E.Stem) + TEXT(".alerts.txt"))), FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM);
			AddError(FString::Printf(TEXT("%s: alerts differ from the browser's (hash %016llx, oracle %016llx); this client's list is in %s"), E.Stem, (unsigned long long)Fnv(AlertText), (unsigned long long)E.AlertsFnv, *Dir));
		}
		if (Fnv(LineText) != E.LinesFnv)
		{
			FFileHelper::SaveStringToFile(LineText, *(Dir / (FString(E.Stem) + TEXT(".lines.txt"))), FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM);
			AddError(FString::Printf(TEXT("%s: lines or sounds differ from the browser's (hash %016llx, oracle %016llx); this client's log is in %s"), E.Stem, (unsigned long long)Fnv(LineText), (unsigned long long)E.LinesFnv, *Dir));
		}
		for (const FString& R : Raised) AddInfo(FString::Printf(TEXT("%s: %s"), E.Stem, *R));
	}
	TestTrue(TEXT("at least the synthetic stream ran"), Ran >= 1);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAlertsRulesTest, "ThreeCrossroads.Alerts.Rules", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCAlertsRulesTest::RunTest(const FString& Parameters)
{
	using namespace TCAlertTest;
	FTCUnitTable Table;
	FString Err;
	if (!TestTrue(TEXT("units.json loads"), Table.Load(Err))) { AddError(Err); return false; }

	// pings.js: send and receive
	TestTrue(TEXT("send: a finite spot"), FTCPings::MakeSend(12.5, 40).IsValid());
	TestFalse(TEXT("send: not a number"), FTCPings::MakeSend(NAN, 40).IsValid());
	FTCPings P;
	FTCPingRing R;
	TestFalse(TEXT("receive: a fractional seat"), P.Receive(*Parse(TEXT(R"({"t":"ping","from":1.5,"x":1,"z":2})")), 0, R));
	TestFalse(TEXT("receive: a seat as a string"), P.Receive(*Parse(TEXT(R"({"t":"ping","from":"1","x":1,"z":2})")), 0, R));
	TestFalse(TEXT("receive: no z"), P.Receive(*Parse(TEXT(R"({"t":"ping","from":1,"x":1})")), 0, R));
	TestTrue(TEXT("receive: good"), P.Receive(*Parse(TEXT(R"({"t":"ping","from":2,"x":10,"z":20})")), 100.0, R) && R.From == 2 && R.X == 10 && R.Z == 20);
	FTCPingLook L;
	TestTrue(TEXT("ring look at birth"), P.LookOf(P.Rings()[0], 100.0, L) && L.Radius == 2.f && FMath::IsNearlyEqual(L.Opacity, 0.9f) && L.MinimapPx == 4.f);
	TestTrue(TEXT("ring look half a period on"), P.LookOf(P.Rings()[0], 100.6, L) && FMath::IsNearlyEqual(L.Radius, 5.f) && FMath::IsNearlyEqual(L.MinimapPx, 11.f));
	TestTrue(TEXT("ring fading at the end"), P.LookOf(P.Rings()[0], 103.7, L) && L.Opacity < 0.5f);
	P.Frame(104.0);
	TestEqual(TEXT("ring gone after 4 s"), P.Rings().Num(), 0);

	// alert-history.js: navigation over located entries, clamped; a new match empties it, the same keeps it
	FTCAlertHistory H(3);
	TestTrue(TEXT("history start"), H.Start(5));
	FTCAlertPush A1; A1.Kind = TEXT("attack"); A1.Text = TEXT("one"); A1.bHasAt = true; A1.At = FVector2D(1, 1);
	FTCAlertPush A2 = A1; A2.Text = TEXT("two"); A2.bHasAt = false;
	FTCAlertPush A3 = A1; A3.Text = TEXT("three"); A3.At = FVector2D(3, 3);
	H.Add(A1); H.Add(A2); H.Add(A3);
	const FTCAlertHistoryEntry* E = H.Navigate(1);
	TestTrue(TEXT("navigate from nothing: the newest located"), E && E->Text == TEXT("three"));
	E = H.Navigate(1);
	TestTrue(TEXT("navigate older: skips the one without a place"), E && E->Text == TEXT("one"));
	E = H.Navigate(1);
	TestTrue(TEXT("navigate clamps at the oldest"), E && E->Text == TEXT("one"));
	FTCAlertPush A4 = A1; A4.Text = TEXT("four");
	H.Add(A4);
	TestEqual(TEXT("limit kept"), H.Entries().Num(), 3);
	TestEqual(TEXT("the selected entry fell off: cursor cleared"), H.Selected(), 0);
	TestFalse(TEXT("same match: kept"), H.Start(5));
	TestEqual(TEXT("same match: entries kept"), H.Entries().Num(), 3);
	TestTrue(TEXT("tick accepted"), H.AcceptTick(10));
	TestFalse(TEXT("same tick refused"), H.AcceptTick(10));
	TestTrue(TEXT("new match"), H.Start(6));
	TestEqual(TEXT("new match: emptied"), H.Entries().Num(), 0);
	TestTrue(TEXT("new match: ticks again"), H.AcceptTick(1));

	// alerts.js push: lives, columns, fade, newest
	FTCAlerts F;
	for (int32 I = 0; I < 6; I++) F.Push(TEXT("attack"), FString::Printf(TEXT("a%d"), I), true, FVector2D(I, 0), 10.0);
	F.Push(TEXT("unitLost"), TEXT("l1"), true, FVector2D(50, 50), 10.0);
	F.Push(TEXT("unitLost"), TEXT("l2"), true, FVector2D(60, 60), 10.0);
	F.Push(TEXT("unitLost"), TEXT("l3"), true, FVector2D(70, 70), 10.0);
	F.Push(TEXT("event"), TEXT("no place"), false, FVector2D::ZeroVector, 10.0);
	int32 Losses = 0, Routine = 0;
	for (const FTCAlertLine& Ln : F.Lines()) (Ln.IsLoss() ? Losses : Routine)++;
	TestEqual(TEXT("two losses kept"), Losses, 2);
	TestEqual(TEXT("four routine lines kept"), Routine, 4);
	FVector2D At;
	TestTrue(TEXT("newest: a loss first"), F.Newest(At) && At == FVector2D(70, 70));
	TestEqual(TEXT("a line at full opacity"), FTCAlerts::Alpha(F.Lines()[0], 10.0), 1.f);
	const FTCAlertLine* Routine0 = F.Lines().FindByPredicate([](const FTCAlertLine& X) { return X.Alert.Kind == TEXT("attack"); });
	TestTrue(TEXT("a routine line fading in its last 0.6 s"), Routine0 && FTCAlerts::Alpha(*Routine0, 15.7) < 1.f && FTCAlerts::Alpha(*Routine0, 15.7) > 0.f);
	F.Frame(16.5);
	for (const FTCAlertLine& Ln : F.Lines()) TestTrue(TEXT("after 6.5 s only losses (12 s) and the event (15 s) stay"), Ln.IsLoss() || Ln.Alert.Kind == TEXT("event"));
	F.Frame(22.5);
	TestTrue(TEXT("after 12.5 s only the event"), F.Lines().Num() == 1 && F.Lines()[0].Alert.Kind == TEXT("event"));
	TestFalse(TEXT("newest: none with a place"), F.Newest(At));
	TArray<FString> Snd = F.TakeSounds();
	TestTrue(TEXT("one sound per kind within 1.5 s"), Snd.Num() == 3 && Snd.Contains(TEXT("alert:attack")) && Snd.Contains(TEXT("alert:unitLost")) && Snd.Contains(TEXT("alert:event")));

	// under attack: quiet for a spot on screen (it may fire once the camera looks away), and a shared throttle
	FTCMatchState St;
	St.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","state":"play","mode":"conquest","you":0,"spectator":false,"players":[{"name":"Me"},{"name":"Foe"}]})")));
	St.ApplyStart(*Parse(TEXT(R"({"t":"start","matchId":3,"you":0,"spawn":{"x":10,"z":10},"names":["Me","Foe"],"teams":[0,1],"factions":[0,1],"map":{"name":"t","w":1,"h":1,"rows":[""],"points":[]}})")));
	FTCAlerts G;
	G.StartMatch(St);
	TCWorldRecoveryCore::FAttackQuiet Shared;
	FTCAlertContext Ctx;
	Ctx.Table = &Table; Ctx.Quiet = &Shared;
	bool bOnScreen = true;
	Ctx.OnScreen = [&bOnScreen](double, double) { return bOnScreen; };
	auto SnapAt = [&](int32 Tick, int32 Hp, const TCHAR* Shots)
	{
		const TSharedPtr<FJsonObject> M = Parse(FString::Printf(TEXT(R"({"t":"s","tick":%d,"units":[[1,"rifle",0,100,100,0,0,%d,0,0,0,0,0,0,1]],"points":[],"shots":%s})"), Tick, Hp, Shots));
		St.ApplySnapshot(*M);
		Ctx.Now = Tick / 20.0;
		return G.Snapshot(St, M.Get(), Ctx);
	};
	const TCHAR* Enemy = TEXT(R"([{"k":"rifle","f":9,"fo":1,"t":1,"to":0,"x":100,"z":100}])");
	SnapAt(2, 100, TEXT("[]"));
	TestEqual(TEXT("on screen: no alert"), SnapAt(4, 90, Enemy).Num(), 0);
	bOnScreen = false;
	TestEqual(TEXT("looked away: the next hit alerts"), SnapAt(6, 80, Enemy).Num(), 1);
	TestEqual(TEXT("throttled within 20 s in 30 m"), SnapAt(8, 70, Enemy).Num(), 0);
	TestFalse(TEXT("the shared throttle saw it too"), Shared.Allow(8 / 20.0, 110, 100, false));
	TestEqual(TEXT("an own-side shot does not alert"), SnapAt(2000, 60, TEXT(R"([{"k":"rifle","f":9,"fo":0,"t":1,"to":0,"x":100,"z":100}])")).Num(), 0);
	const TArray<FTCAlertPush> Late = SnapAt(2002, 50, Enemy);
	TestTrue(TEXT("after 20 s it alerts again, by the faction's name"), Late.Num() == 1 && Late[0].Text == TEXT("Rifle Squad under attack"));
	TestEqual(TEXT("unit names by faction"), FTCAlerts::UnitName(St, &Table, TEXT("rifle"), 1), FString(TEXT("Grenadiers")));
	TestEqual(TEXT("unit names: a type the faction does not rename"), FTCAlerts::UnitName(St, &Table, TEXT("hq"), 1), Table.Find(TEXT("hq"))->Name);

	// a spectator raises nothing
	FTCMatchState Watch;
	Watch.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","state":"play","mode":"conquest","you":-1,"spectator":true,"players":[]})")));
	Watch.ApplyStart(*Parse(TEXT(R"({"t":"start","matchId":3,"you":0,"names":["Me","Foe"],"teams":[0,1],"map":{"name":"t","w":1,"h":1,"rows":[""],"points":[]}})")));
	FTCAlerts W;
	W.StartMatch(Watch);
	for (int32 Tick = 2; Tick <= 6; Tick += 2)
	{
		const TSharedPtr<FJsonObject> M = Parse(FString::Printf(TEXT(R"({"t":"s","tick":%d,"units":[%s],"points":[],"shots":[{"k":"say","text":"hello","x":1,"z":1}]})"), Tick, Tick < 6 ? TEXT(R"([1,"rifle",0,100,100,0,0,100,0,0,0,0,0,0,1])") : TEXT("")));
		Watch.ApplySnapshot(*M);
		Ctx.Now = Tick / 20.0;
		TestEqual(TEXT("spectator: nothing"), W.Snapshot(Watch, M.Get(), Ctx).Num(), 0);
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAlertsSayLanguageTest, "ThreeCrossroads.Alerts.SayLanguage", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCAlertsSayLanguageTest::RunTest(const FString& Parameters)
{
	using namespace TCAlertTest;
	// client/alerts.js:182: push('event', sh.localized?.[lang] ?? sh.text, ...), lang 'es' in Spanish, else 'en' (CLI-121).
	// The second line has no 'es': a Spanish reader gets sh.text, not the English localization.
	const TCHAR* Shots = TEXT(R"([{"k":"say","text":"Hold the crossroads","localized":{"en":"Hold the crossroads!","es":"Mantened el cruce"},"x":100,"z":100,"pub":true},{"k":"say","text":"No place","localized":{"en":"Only English"},"pub":true},{"k":"say","text":"Plain","pub":true}])");
	struct FCase { ETCLanguage Language; const TCHAR* First; const TCHAR* Second; const TCHAR* Third; };
	for (const FCase& C : { FCase{ ETCLanguage::English, TEXT("Hold the crossroads!"), TEXT("Only English"), TEXT("Plain") }, FCase{ ETCLanguage::Spanish, TEXT("Mantened el cruce"), TEXT("No place"), TEXT("Plain") } })
	{
		FTCLocale::FScopedLanguage Scope(C.Language);
		FTCMatchState St;
		St.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","state":"play","mode":"conquest","you":0,"spectator":false,"players":[{"name":"Me"},{"name":"Foe"}]})")));
		St.ApplyStart(*Parse(TEXT(R"({"t":"start","matchId":4,"you":0,"spawn":{"x":10,"z":10},"names":["Me","Foe"],"teams":[0,1],"factions":[0,1],"map":{"name":"t","w":1,"h":1,"rows":[""],"points":[]}})")));
		FTCAlerts A;
		A.StartMatch(St);
		FTCAlertContext Ctx;
		Ctx.Now = 1.0;
		const TSharedPtr<FJsonObject> M = Parse(FString::Printf(TEXT(R"({"t":"s","tick":2,"units":[[1,"rifle",0,10,10,0,0,100,0,0,0,0,0,0,1]],"points":[],"shots":%s})"), Shots));
		St.ApplySnapshot(*M);
		const TArray<FTCAlertPush> Out = A.Snapshot(St, M.Get(), Ctx);
		const TCHAR* Lang = FTCLocale::Code(C.Language);
		if (!TestEqual(FString::Printf(TEXT("%s: three say lines"), Lang), Out.Num(), 3)) continue;
		TestEqualSensitive(FString::Printf(TEXT("%s: localized[lang]"), Lang), Out[0].Text, FString(C.First));
		TestEqualSensitive(FString::Printf(TEXT("%s: no entry for lang: sh.text"), Lang), Out[1].Text, FString(C.Second));
		TestEqualSensitive(FString::Printf(TEXT("%s: no localized at all"), Lang), Out[2].Text, FString(C.Third));
		TestTrue(FString::Printf(TEXT("%s: the first has its place, the others none"), Lang), Out[0].bHasAt && !Out[1].bHasAt && !Out[2].bHasAt);
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAlertsViewTest, "ThreeCrossroads.Alerts.View", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCAlertsViewTest::RunTest(const FString&)
{
	// client/alerts.css with alerts.js push/frame/drawPings/place and the history box, on lines pushed at known times
	FTCLocale::FScopedLanguage En(ETCLanguage::English);
	const FTCMeasure M = FTCPaint::ApproxMeasure();
	FTCAlerts A;
	A.Push(TEXT("attack"), TEXT("Rifle Squad under attack"), true, FVector2D(60, 60), 100.0);
	A.Push(TEXT("pointWon"), TEXT("Point captured"), true, FVector2D(50, 50), 100.5);
	A.Push(TEXT("unitLost"), TEXT("M5 Stuart lost"), true, FVector2D(120, 120), 101.0);
	A.Push(TEXT("base"), TEXT("Our base is under attack!"), true, FVector2D(22, 22), 101.2);
	A.Push(TEXT("ready"), TEXT("Rifle Squad ready"), true, FVector2D(24, 30), 101.3);
	A.Push(TEXT("event"), TEXT("No place"), false, FVector2D::ZeroVector, 101.4);
	FTCAlertViewInput In;
	In.Now = 102.0; In.Right = 1272.f; In.MinimapTop = 528.f; In.ViewW = 1280.f; In.ViewH = 720.f;
	A.Frame(In.Now);
	FTCPaint P = A.Layout(In, M);
	TestEqual(TEXT("loss heading (tr('Unit lost'), upper-cased)"), P.TextsOf(TEXT("alert:loss-heading")), TArray<FString>{ TEXT("UNIT LOST") });
	TestEqual(TEXT("loss detail"), P.TextsOf(TEXT("alert:loss-detail")), TArray<FString>{ TEXT("M5 Stuart lost") });
	TestEqual(TEXT("loss hint"), P.TextsOf(TEXT("alert:loss-hint")), TArray<FString>{ TEXT("Space or click: look there") });
	// while a loss shows, only two routine lines (nth-of-type(n+4) hidden); the newest first and semi-bold
	TestEqual(TEXT("newest routine line"), P.TextsOf(TEXT("alert:text-newest")), TArray<FString>{ TEXT("No place") });
	TestEqual(TEXT("the second routine line"), P.TextsOf(TEXT("alert:text")), TArray<FString>{ TEXT("Rifle Squad ready") });
	TestEqual(TEXT("history summary"), P.TextsOf(TEXT("alert:history-summary")), TArray<FString>{ TEXT("Alert history") });
	// order top to bottom: the loss box, the routine lines, the history box whose bottom is 13 px over the minimap
	const FTCPaintItem* Loss = P.Find(TEXT("alert:loss"));
	const FTCPaintItem* Newest = P.Find(TEXT("alert:text-newest"));
	const FTCPaintItem* Hist = P.Find(TEXT("alert:history"));
	TestTrue(TEXT("stacked: loss, routine, history"), Loss && Newest && Hist && Loss->Pos.Y < Newest->Pos.Y && Newest->Pos.Y < Hist->Pos.Y);
	TestTrue(TEXT("history bottom = minimap top - 5 - 8"), Hist && FMath::IsNearlyEqual(Hist->Pos.Y + Hist->Size.Y, 528.f - 13.f, 0.01f));
	TestTrue(TEXT("right edges on the minimap's"), Loss && Hist && FMath::IsNearlyEqual(Loss->Pos.X + Loss->Size.X, 1272.f, 0.01f) && FMath::IsNearlyEqual(Hist->Pos.X + Hist->Size.X, 1272.f, 0.01f));
	TestTrue(TEXT("loss column 380 px, history 320 px"), Loss && Hist && Loss->Size.X == 380.f && Hist->Size.X == 320.f);
	int32 LineHits = 0;
	for (const FTCPaintHit& H : P.Hits) LineHits += H.Tag.StartsWith(TEXT("alert:line:"));
	TestEqual(TEXT("a hit per shown line (1 loss, 2 routine)"), LineHits, 3);
	// the slide-in: a line 0.09 s old is half way in
	{
		FTCAlertLine L; L.Alert.Kind = TEXT("attack"); L.Born = 0.0;
		float Slide = 0;
		const float Al = FTCAlerts::ShownAlpha(L, 0.09, &Slide);
		TestTrue(TEXT("alert-in"), Al > 0.4f && Al < 0.9f && Slide > 0.5f && Slide < 8.f);
		TestEqual(TEXT("in, steady"), FTCAlerts::ShownAlpha(L, 3.0), 1.f);
		TestTrue(TEXT("fading: 0.3 s left of 6 s"), FMath::IsNearlyEqual(FTCAlerts::ShownAlpha(L, 5.7), 0.5f, 1e-3f));
		L.Alert.Kind = TEXT("unitLost");
		TestTrue(TEXT("a loss lives 12 s"), FTCAlerts::ShownAlpha(L, 11.0) == 1.f && FTCAlerts::ShownAlpha(L, 12.0) == 0.f);
		L.Alert.Kind = TEXT("event");
		TestTrue(TEXT("an event lives 15 s"), FTCAlerts::ShownAlpha(L, 14.0) == 1.f);
	}
	// the base alert: white bold on red, pulsing after its slide-in
	{
		FTCAlerts B;
		B.Push(TEXT("base"), TEXT("Our base is under attack!"), true, FVector2D(22, 22), 0.0);
		FTCAlertViewInput I2 = In;
		I2.Now = 0.18 + 0.9; // the top of the first pulse
		const FTCPaint Q = B.Layout(I2, M);
		const FTCPaintItem* Line = Q.Find(TEXT("alert:line"));
		const FTCPaintItem* Text = Q.Find(TEXT("alert:text-newest"));
		TestTrue(TEXT("pulse at its peak: rgba(200, 60, 46, 0.96)"), Line && Line->Colour.Equals(TCCss::Rgba(200, 60, 46, 0.96f), 1e-3f));
		TestTrue(TEXT("white, bold"), Text && Text->Colour.Equals(FLinearColor::White) && Text->bBold);
		I2.Now = 5.0;
		const FTCPaintItem* After = B.Layout(I2, M).Find(TEXT("alert:line"));
		TestTrue(TEXT("after four half-cycles: rgba(140, 36, 28, 0.94)"), After && After->Colour.Equals(TCCss::Rgba(140, 36, 28, 0.94f), 1e-3f));
	}
	// once the loss is gone the routine lines show four at most
	In.Now = 113.5;
	A.Frame(In.Now);
	A.Push(TEXT("air"), TEXT("Enemy Strafing Run incoming"), true, FVector2D(125, 125), 113.5);
	A.Push(TEXT("pointLost"), TEXT("Point lost"), true, FVector2D(80, 20), 113.5);
	A.Push(TEXT("ready"), TEXT("M4 Sherman ready"), true, FVector2D(30, 30), 113.5);
	A.Push(TEXT("attack"), TEXT("Units under attack"), true, FVector2D(70, 70), 113.5);
	A.Push(TEXT("pointWon"), TEXT("Ally captured a point"), true, FVector2D(20, 80), 113.5);
	In.Now = 114.0; // past the 0.18 s slide-in
	P = A.Layout(In, M);
	TestEqual(TEXT("no loss now"), P.TextsOf(TEXT("alert:loss-detail")).Num(), 0);
	TestEqual(TEXT("four routine lines"), P.TextsOf(TEXT("alert:text-newest")).Num() + P.TextsOf(TEXT("alert:text")).Num(), 4);
	// colours: light red for trouble, brass for a point won
	for (const FTCPaintItem& I : P.Items)
	{
		if (I.Role != TEXT("alert:text") && I.Role != TEXT("alert:text-newest")) continue;
		if (I.Text == TEXT("Point lost") || I.Text == TEXT("Units under attack")) TestTrue(I.Text + TEXT(": light red"), I.Colour.Equals(TCCss::RedHi));
		if (I.Text == TEXT("Ally captured a point")) TestTrue(I.Text + TEXT(": brass"), I.Colour.Equals(TCCss::Brass));
		if (I.Text == TEXT("M4 Sherman ready")) TestTrue(I.Text + TEXT(": chalk"), I.Colour.Equals(TCCss::Text));
	}
	// the minimap pings: two marks per line with a place, in the kind's colour
	const TArray<FTCAlertRing> Rings = A.MinimapRings(113.5 + 0.6);
	TestEqual(TEXT("rings"), Rings.Num(), 2 * 4);
	TestTrue(TEXT("spreading ring half way"), Rings.Num() && FMath::IsNearlyEqual(Rings[0].RadiusPx, 11.f, 1e-3f) && FMath::IsNearlyEqual(Rings[0].Alpha, 0.5f, 1e-3f));
	TestTrue(TEXT("steady mark"), Rings.Num() > 1 && Rings[1].RadiusPx == 4.f && Rings[1].Alpha == 1.f && Rings[1].Colour.Equals(TCAlertRules::ColourOf(TEXT("pointWon"))));
	// clicks: a line looks at its spot; the summary opens the history; Previous Alert visits the newest with a place
	FVector2D At; bool bJump = false;
	int32 Serial = -1;
	for (const FTCPaintHit& H : P.Hits) if (H.Label == TEXT("Point lost")) Serial = FCString::Atoi(*H.Tag.RightChop(11));
	TestTrue(TEXT("line click"), A.Click(FString::Printf(TEXT("alert:line:%d"), Serial), At, bJump) && bJump && At == FVector2D(80, 20));
	TestTrue(TEXT("summary click"), A.Click(TEXT("alert:history"), At, bJump) && !bJump && A.bHistoryOpen);
	TestTrue(TEXT("Previous Alert"), A.Click(TEXT("alert:nav:1"), At, bJump) && bJump && At == FVector2D(20, 80));
	TestTrue(TEXT("again: the one before"), A.Click(TEXT("alert:nav:1"), At, bJump) && bJump && At == FVector2D(70, 70));
	TestTrue(TEXT("Next Alert"), A.Click(TEXT("alert:nav:-1"), At, bJump) && bJump && At == FVector2D(20, 80));
	TestFalse(TEXT("not an alert tag"), A.Click(TEXT("report:toggle"), At, bJump));
	P = A.Layout(In, M);
	TestEqual(TEXT("nav labels"), P.TextsOf(TEXT("alert:nav-label")), TArray<FString>{ TEXT("Previous Alert"), TEXT("Next Alert") });
	const TArray<FString> Entries = P.TextsOf(TEXT("alert:entry-text"));
	TestTrue(TEXT("entries: m:ss · text, newest first"), Entries.Num() >= 3 && Entries[0] == TEXT("0:00 · Ally captured a point"));
	TestTrue(TEXT("the selected entry has the brass outline"), P.Find(TEXT("alert:entry-current")) != nullptr);
	int32 EntryId = -1;
	for (const FTCPaintHit& H : P.Hits) if (H.Tag.StartsWith(TEXT("alert:entry:")) && H.Label == TEXT("Point lost")) EntryId = FCString::Atoi(*H.Tag.RightChop(12));
	TestTrue(TEXT("entry click visits it"), EntryId > 0 && A.Click(FString::Printf(TEXT("alert:entry:%d"), EntryId), At, bJump) && bJump && At == FVector2D(80, 20));
	// the list scrolls: max-height min(30vh, 250px) holds a window of entries with the selected one in it
	{
		FTCAlerts C;
		for (int32 K = 0; K < 30; K++) C.Push(TEXT("attack"), FString::Printf(TEXT("Line %d"), K), true, FVector2D(K, K), K);
		C.bHistoryOpen = true;
		FTCAlertViewInput I3 = In; I3.Now = 40.0;
		for (int32 K = 0; K < 20; K++) C.Navigate(1);
		const FTCPaint Q = C.Layout(I3, M);
		const TArray<FString> Shown = Q.TextsOf(TEXT("alert:entry-text"));
		TestTrue(TEXT("a window of the 30"), Shown.Num() > 3 && Shown.Num() < 30);
		TestTrue(TEXT("the selected (the 20th newest, Line 10) is shown"), Shown.ContainsByPredicate([](const FString& S) { return S.EndsWith(TEXT("Line 10")); }));
		TestTrue(TEXT("a scrollbar"), Q.Find(TEXT("alert:history-scroll")) != nullptr);
		const FTCPaintItem* H = Q.Find(TEXT("alert:history"));
		TestTrue(TEXT("history height within 28.9 + 38.9 + 258 + 1"), H && H->Size.Y <= 28.9f + 38.9f + 258.f + 1.01f);
	}
	// Mirror: the game mode's PushAlert gives every alert back; the feed's own is not added twice, another one joins
	{
		FTCAlerts C;
		C.Push(TEXT("unitLost"), TEXT("Rifle Squad lost"), true, FVector2D(5, 5), 10.0);
		C.Mirror(TEXT("unitLost"), TEXT("Rifle Squad lost"), FVector2D(5, 5), 200, 10.01);
		TestEqual(TEXT("echo: one line"), C.Lines().Num(), 1);
		TestEqual(TEXT("echo: one entry"), C.History().Entries().Num(), 1);
		C.Mirror(TEXT("pointWon"), TEXT("Region claimed: Ardennes"), FVector2D(7, 8), 220, 10.02);
		TestEqual(TEXT("a region alert joins"), C.Lines().Num(), 2);
		TestTrue(TEXT("its history time is tick / 20"), C.History().Entries().Num() == 2 && C.History().Entries()[0].Time == 11.0 && C.History().Entries()[0].Text == TEXT("Region claimed: Ardennes"));
		C.Mirror(TEXT("unitLost"), TEXT("Rifle Squad lost"), FVector2D(5, 5), 230, 10.03);
		TestEqual(TEXT("the same text again later is a new alert"), C.Lines().Num(), 3); // two losses and the region line
		TestEqual(TEXT("history keeps both losses"), C.History().Entries().Num(), 3);
		TArray<FString> Sounds = C.TakeSounds();
		TestEqual(TEXT("Mirror plays no sound (PushAlert did)"), Sounds.Num(), 1);
	}
	// Spanish: every element's text through spanish(), the heading upper-cased after it
	{
		FTCLocale::FScopedLanguage Es(ETCLanguage::Spanish);
		FTCAlerts C;
		C.Push(TEXT("unitLost"), TEXT("3 units lost"), true, FVector2D(5, 5), 0.0);
		C.bHistoryOpen = true;
		FTCAlertViewInput I4 = In; I4.Now = 1.0;
		const FTCPaint Q = C.Layout(I4, M);
		TestEqual(TEXT("es heading"), Q.TextsOf(TEXT("alert:loss-heading")), TArray<FString>{ TEXT("SE PIERDE UNA UNIDAD") });
		TestEqual(TEXT("es detail ('{0} units lost')"), Q.TextsOf(TEXT("alert:loss-detail")), TArray<FString>{ TEXT("3 unidades perdidas") });
		TestEqual(TEXT("es hint"), FString::Join(Q.TextsOf(TEXT("alert:loss-hint")), TEXT(" ")), FString(TEXT("Espacio o clic: mirar allí")));
		TestEqual(TEXT("es summary"), Q.TextsOf(TEXT("alert:history-summary")), TArray<FString>{ TEXT("Historial de alertas") });
		TestEqual(TEXT("es nav"), Q.TextsOf(TEXT("alert:nav-label")), TArray<FString>{ TEXT("Alerta anterior"), TEXT("Alerta siguiente") });
		FTCAlerts LossHistory;
		LossHistory.Mirror(TEXT("unitLost"), TEXT("MG 42 Team lost"), FVector2D(5, 5), 1800, 90.0);
		LossHistory.bHistoryOpen = true;
		TestEqual(TEXT("es history clock precedes the translated loss prefix"),
			FString::Join(LossHistory.Layout(I4, M).TextsOf(TEXT("alert:entry-text")), TEXT(" ")),
			FString(TEXT("1:30 · Perdido: Equipo MG 42")));
		{
			FTCLocale::FScopedLanguage English(ETCLanguage::English);
			TestEqual(TEXT("en history keeps the same clock separator"),
				FString::Join(LossHistory.Layout(I4, M).TextsOf(TEXT("alert:entry-text")), TEXT(" ")),
				FString(TEXT("1:30 · MG 42 Team lost")));
		}
		FTCAlerts Empty;
		Empty.bHistoryOpen = true;
		TestEqual(TEXT("es empty list"), Empty.Layout(I4, M).TextsOf(TEXT("alert:history-empty")), TArray<FString>{ TEXT("Todavía no hay alertas") });
	}
	return true;
}

#endif // WITH_DEV_AUTOMATION_TESTS
