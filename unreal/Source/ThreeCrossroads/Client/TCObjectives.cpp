#include "Client/TCObjectives.h"
#include "Client/TCScenarioText.h"
#include "ThreeCrossroads.h"
#include "Client/TCUnitTable.h"
#include "Client/TCLocale.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Net/TCMatchState.h"
#include "HAL/PlatformTime.h"
#include <cstdio>
#include <cstdlib>

// Helpers in a named namespace (not an anonymous one): unity builds join this file with others that define their own
// Num, Truthy or JsRound. Member functions open it with a function-scope using-directive.
namespace TCObjectivesImpl
{
	// The non-ASCII characters the reference strings use, spelled out so the source stays ASCII.
	const TCHAR* const MiddleDot = TEXT("\u00B7");   // the middle dot of "Radio post <dot> "
	const TCHAR* const Star = TEXT("\u2605");        // the black star of "<star> 2<times> VP"
	const TCHAR* const Times = TEXT("\u00D7");       // the multiplication sign
	const TCHAR* const Minus = TEXT("\u2212");       // the minus sign of "(upkeep <minus>0.1)"

	// JavaScript Math.round (halves go up).
	double JsRound(double V) { return FMath::FloorToDouble(V + 0.5); }
	// JavaScript a % b for the non-negative values used here.
	double JsMod(double A, double B) { return FMath::Fmod(A, B); }
	FString Pad2(const FString& S) { return S.Len() >= 2 ? S : FString::ChrN(2 - S.Len(), TEXT('0')) + S; }

	// client/hud.js clock: `${Math.floor(t / 60)}:${String(Math.max(0, t) % 60).padStart(2, '0')}`
	FString HudClock(double T) { return FString::Printf(TEXT("%s:%s"), *FTCObjectives::JsNumber(FMath::FloorToDouble(T / 60.0)), *Pad2(FTCObjectives::JsNumber(JsMod(FMath::Max(0.0, T), 60.0)))); }
	// client/endgame.js and client/main.js mins: `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
	FString MinClock(double T) { return FString::Printf(TEXT("%s:%s"), *FTCObjectives::JsNumber(FMath::FloorToDouble(T / 60.0)), *Pad2(FTCObjectives::JsNumber(JsMod(T, 60.0)))); }
	// client/report.js clock: "12 min 3 s", "12 min", "45 s"
	FString ReportClock(double Secs)
	{
		const double M = FMath::FloorToDouble(Secs / 60.0), S = JsRound(JsMod(Secs, 60.0));
		if (M != 0.0) return FString::Printf(TEXT("%s min%s"), *FTCObjectives::JsNumber(M), S != 0.0 ? *FString::Printf(TEXT(" %s s"), *FTCObjectives::JsNumber(S)) : TEXT(""));
		return FString::Printf(TEXT("%s s"), *FTCObjectives::JsNumber(S));
	}

	// client/endgame.js COLORS: the team name the notices use (teamName is not passed by main.js).
	FString TeamName(const TArray<int32>& Teams, int32 Team)
	{
		static const TCHAR* const Names[] = { TEXT("Blue"), TEXT("Red"), TEXT("Chalk"), TEXT("Orange"), TEXT("Violet"), TEXT("Cyan") };
		const int32 First = Teams.IndexOfByKey(Team);
		return First >= 0 && First < (int32)UE_ARRAY_COUNT(Names) ? FString(Names[First]) : FString::Printf(TEXT("Team %d"), Team + 1);
	}

	// shared/weather.js WEATHER
	struct FWeatherDef { const TCHAR* Key; const TCHAR* Name; double Sight, Inf, Veh, OffRoad; bool bRains, bSoaks; };
	const FWeatherDef WeatherDefs[] = {
		{ TEXT("clear"), TEXT("Clear"), 1, 1, 1, 1, false, false },
		{ TEXT("fog"), TEXT("Ground fog"), 0.7, 1, 1, 1, false, false },
		{ TEXT("rain"), TEXT("Rain"), 0.8, 1, 1, 0.8, true, false },
		{ TEXT("mud"), TEXT("Mud"), 1, 0.9, 1, 0.8, false, true },
		{ TEXT("snow"), TEXT("Snow"), 0.9, 0.9, 0.85, 1, false, false },
	};
	const FWeatherDef* FindWeather(const FString& Key)
	{
		for (const FWeatherDef& D : WeatherDefs) if (Key == D.Key) return &D;
		return nullptr;
	}
	FString WeatherName(const FString& Key) { const FWeatherDef* D = FindWeather(Key); return D ? FString(D->Name) : Key; }
	// weather-view.js said: "Ground fog: sight -30%"
	FString Said(const FString& Key)
	{
		const FWeatherDef* D = FindWeather(Key);
		const FWeatherDef& W = D ? *D : WeatherDefs[0]; // weatherEffects: WEATHER[kind] ?? WEATHER.clear
		auto Pct = [](double M) { return FString::Printf(TEXT("-%s%%"), *FTCObjectives::JsNumber(JsRound((1.0 - M) * 100.0))); };
		TArray<FString> Out;
		if (W.Sight < 1) Out.Add(TEXT("sight ") + Pct(W.Sight));
		if (W.Inf < 1) Out.Add(TEXT("infantry ") + Pct(W.Inf));
		if (W.Veh < 1) Out.Add(TEXT("vehicles ") + Pct(W.Veh));
		if (W.OffRoad < 1) Out.Add(TEXT("vehicles ") + Pct(W.OffRoad) + TEXT(" off roads"));
		const FString Effects = FString::Join(Out, TEXT(", "));
		return FString::Printf(TEXT("%s: %s"), *WeatherName(Key), Effects.IsEmpty() ? TEXT("no effect") : *Effects);
	}

	int32 HpPer(const FTCUnitTable* Table, const TCHAR* Type, int32 Fallback)
	{
		const FTCUnitDef* D = Table ? Table->Find(Type) : nullptr;
		return D && D->HpPer > 0 ? D->HpPer : Fallback;
	}

	// The team list a start gives (client/main.js: m.teams ?? names.map((_, i) => i)).
	TArray<int32> TeamList(const FTCMatchState& S)
	{
		if (S.Teams.Num()) return S.Teams;
		TArray<int32> T;
		for (int32 I = 0; I < S.Names.Num(); I++) T.Add(I);
		return T;
	}
}

// ---------------------------------------------------------------------------------------------------------------------
// JavaScript number formatting

FString FTCObjectives::JsNumber(double V)
{
	if (V != V) return TEXT("NaN");
	if (!FMath::IsFinite(V)) return V > 0 ? TEXT("Infinity") : TEXT("-Infinity");
	if (V == 0.0) return TEXT("0");
	// The shortest digits that read back as the same double (ECMAScript Number::toString), then JavaScript's layout.
	char Buf[64];
	int32 P = 1;
	for (; P <= 17; P++)
	{
		std::snprintf(Buf, sizeof(Buf), "%.*e", P - 1, V);
		if (std::strtod(Buf, nullptr) == V) break;
	}
	const char* C = Buf;
	FString Sign;
	if (*C == '-') { Sign = TEXT("-"); C++; }
	FString Digits;
	for (; *C && *C != 'e'; C++) if (*C >= '0' && *C <= '9') Digits.AppendChar((TCHAR)*C);
	const int32 Exp = *C == 'e' ? std::atoi(C + 1) : 0;
	while (Digits.Len() > 1 && Digits.EndsWith(TEXT("0"))) Digits.LeftChopInline(1);
	const int32 K = Digits.Len(), N = Exp + 1;
	FString Out;
	if (K <= N && N <= 21) Out = Digits + FString::ChrN(N - K, TEXT('0'));
	else if (0 < N && N <= 21) Out = Digits.Left(N) + TEXT(".") + Digits.Mid(N);
	else if (-6 < N && N <= 0) Out = TEXT("0.") + FString::ChrN(-N, TEXT('0')) + Digits;
	else Out = Digits.Left(1) + (K > 1 ? TEXT(".") + Digits.Mid(1) : FString()) + FString::Printf(TEXT("e%s%d"), N - 1 >= 0 ? TEXT("+") : TEXT("-"), FMath::Abs(N - 1));
	return Sign + Out;
}

FString FTCObjectives::JsToFixed(double V, int32 Digits)
{
	Digits = FMath::Clamp(Digits, 0, 20);
	if (V != V) return TEXT("NaN");
	if (FMath::Abs(V) >= 1e21 || !FMath::IsFinite(V)) return JsNumber(V);
	// Number.prototype.toFixed: the n nearest x * 10^f, the larger on a tie, from the exact decimal value of the double.
	const bool bNeg = V < 0;
	static char Buf[1200];
	std::snprintf(Buf, sizeof(Buf), "%.1100f", FMath::Abs(V));
	FString Int, Frac;
	const char* C = Buf;
	for (; *C && *C != '.'; C++) Int.AppendChar((TCHAR)*C);
	if (*C == '.') C++;
	for (; *C; C++) Frac.AppendChar((TCHAR)*C);
	FString Kept = Int + Frac.Left(Digits);
	if (Frac.Len() > Digits && Frac[Digits] >= TEXT('5'))
	{
		int32 I = Kept.Len() - 1;
		for (; I >= 0; I--)
		{
			if (Kept[I] == TEXT('9')) { Kept[I] = TEXT('0'); continue; }
			Kept[I] = Kept[I] + 1;
			break;
		}
		if (I < 0) Kept = TEXT("1") + Kept;
	}
	const int32 IntLen = Kept.Len() - Digits;
	FString Out = Kept.Left(IntLen);
	if (Digits > 0) Out += TEXT(".") + Kept.Mid(IntLen);
	return bNeg ? TEXT("-") + Out : Out;
}

// ---------------------------------------------------------------------------------------------------------------------
// Lifecycle

double FTCObjectives::Now() const { return Clock ? Clock() : FPlatformTime::Seconds(); }

void FTCObjectives::Reset()
{
	CapturePoints.Reset();
	ResetNotices();
	Ending = FTCEpilogue();
	Sounds.Reset();
}

void FTCObjectives::ResetNotices()
{
	Samples.Reset();
	bHasLeader = false; Leader = 0; bSuddenDeath = false; HordeOn.Reset(); LastKind.Reset(); LastTick.Reset();
	PendingNotice.Reset(); NoticeUntil = 0.0;
	Notice = FTCMatchNotices();
}

void FTCObjectives::OnStart(const FTCMatchState& S)
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	// client/main.js startGame: the capture points from map.points; Assault, Annihilation and Horde leave out the points
	// that pay no manpower (the sim does the same, so snapshot rows follow this filtered order).
	MatchReport = FTCMatchReport();
	CapturePoints.Reset();
	const bool bAssault = S.StartMode == TEXT("assault") || S.StartMode == TEXT("annihilation") || S.StartMode == TEXT("horde");
	const bool bClassic = S.StartMode == TEXT("classic") || S.StartMode == TEXT("world") || S.Map.bWorld;
	for (int32 I = 0; I < S.MapPoints.Num(); I++)
	{
		const FTCMapPoint& P = S.MapPoints[I];
		if (bAssault && !(P.Mp > 0)) continue;
		FTCCapturePoint C;
		C.MapIndex = I;
		C.RowIndex = CapturePoints.Num();
		C.Centre = FVector2D((P.X + 0.5) * CellMetres, (P.Y + 0.5) * CellMetres);
		const FString Kind = P.Kind == TEXT("radio") ? FString::Printf(TEXT("Radio post %s "), MiddleDot) : P.Kind == TEXT("depot") ? FString::Printf(TEXT("Supply depot %s "), MiddleDot) : FString();
		const FString Pays = bClassic ? FString::Printf(TEXT("+%s Mun/s"), *JsNumber(P.Vp * MunPerVp))
			: P.Vp > 1 && !bAssault ? FString::Printf(TEXT("%s %s%s VP"), Star, *JsNumber(P.Vp), Times)
			: FString::Printf(TEXT("+%s MP/s"), *JsNumber(P.Mp));
		C.Label = Kind + Pays;
		if (S.MapPoints.IsValidIndex(P.Needs))
		{
			const FTCMapPoint& N = S.MapPoints[P.Needs];
			C.bHasLink = true;
			C.LinkTo = FVector2D((N.X + 0.5) * CellMetres, (N.Y + 0.5) * CellMetres);
		}
		CapturePoints.Add(MoveTemp(C));
	}
}

void FTCObjectives::OnSnapshot(const FTCMatchState& S)
{
	UpdatePoints(S);
	UpdateEpilogue(S);
	UpdateNotices(S);
}

void FTCObjectives::OnLobby(const FTCMatchState& S)
{
	// renderLobby: in the lobby the ending is over; the result card follows the lobby's result
	if (S.RoomState == TEXT("lobby")) Ending = FTCEpilogue();
	MatchReport = S.LastMatch.bPresent ? BuildReport(S.LastMatch, S.LobbySeat) : FTCMatchReport();
}

void FTCObjectives::QueueSound(const TCHAR* Name, float Delay, float Gain, float Rate)
{
	if (Sounds.Num() >= 16) Sounds.RemoveAt(0);
	Sounds.Add({ Name, Delay, Gain, Rate });
}

TArray<FTCSoundCue> FTCObjectives::TakeSounds()
{
	TArray<FTCSoundCue> Out = MoveTemp(Sounds);
	Sounds.Reset();
	return Out;
}

// ---------------------------------------------------------------------------------------------------------------------
// Capture points (client/objectives.js snapshot and frame, client/markers.js capturePoint)

void FTCObjectives::UpdatePoints(const FTCMatchState& S)
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	const double T = Now();
	for (int32 I = 0; I < S.Points.Num() && I < CapturePoints.Num(); I++)
	{
		const FTCPointRow& R = S.Points[I];
		FTCCapturePoint& C = CapturePoints[I];
		// pointState: a flip is an owner change between two delivered rows (the first row is no flip)
		if (C.bHasRow && C.Owner != R.Owner) C.FlippedAt = T;
		// markers.js set(): the label steps aside for 2.5 s whenever the progress moves
		if (C.LastProgress >= 0 && R.Progress != C.LastProgress) C.BusyUntil = T + TagBusy;
		C.LastProgress = R.Progress;
		C.bTagVisible = T >= C.BusyUntil;
		C.bHasRow = true;
		C.Owner = R.Owner; C.Capper = R.Capper; C.Progress = R.Progress;
		C.bContested = R.bContested; C.bCut = R.bCut; C.bLocked = R.bLocked;
		C.RingSlot = R.Owner >= 0 ? R.Owner : -1;
		C.FlagSlot = C.RingSlot;
		C.ArcSlot = R.Owner >= 0 ? R.Owner : R.Capper >= 0 ? R.Capper : -1;
		C.ArcSegments = (int32)JsRound(R.Progress * ProgressSegments);
		// a point held in full needs no arc: the ring says it
		C.bArcVisible = C.ArcSlot >= 0 && C.ArcSegments > 0 && !(R.Owner >= 0 && R.Progress >= 0.999);
	}
}

FTCPointRingFrame FTCObjectives::RingFrame(int32 Index) const
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	FTCPointRingFrame F;
	if (!CapturePoints.IsValidIndex(Index) || !CapturePoints[Index].bHasRow) return F;
	const FTCCapturePoint& C = CapturePoints[Index];
	const double T = Now(), Age = T - C.FlippedAt;
	const double Change = Age >= 0 && Age < FlipTime ? FMath::Max(0.001, Age / FlipTime) : 0.0;
	F.bDanger = C.bContested;
	if (F.bDanger) F.DangerOpacity = (float)(0.55 + 0.3 * (0.5 + 0.5 * FMath::Sin(T * PI * 1.6)));
	F.bFlip = Change > 0;
	if (F.bFlip) { F.FlipOpacity = (float)(0.9 * (1.0 - Change)); F.FlipScale = (float)(1.0 + 0.06 * Change); }
	return F;
}

// ---------------------------------------------------------------------------------------------------------------------
// Notices (client/endgame.js snapshot and frame)

void FTCObjectives::Announce(const FString& Text) { PendingNotice = Text; NoticeUntil = Now() + TCObjectiveRules::NoticeLife; }

void FTCObjectives::UpdateNotices(const FTCMatchState& S)
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	const FString Kind = S.ModeRow.Kind;
	const TArray<int32> Teams = TeamList(S);
	const int32 Me = S.You;
	if (Kind != LastKind || (LastTick.IsSet() && S.Tick < LastTick.GetValue())) ResetNotices();
	LastKind = Kind; LastTick = S.Tick;
	Notice.Rate.Reset(); Notice.Bonus.Reset();
	const bool bOpen = S.Winner < -1;
	Notice.bOut = S.OutSeats.IsValidIndex(Me) && S.OutSeats[Me] && bOpen;
	Notice.bWatching = S.bSpectator && bOpen;
	if (!bOpen) { PendingNotice.Reset(); Notice.Notice.Reset(); return; }
	if (Kind == TEXT("conquest"))
	{
		TArray<int32> Ids;
		for (int32 T : Teams) Ids.AddUnique(T);
		TMap<int32, double> Totals;
		for (int32 T : Ids)
		{
			double Sum = 0;
			for (int32 Slot = 0; Slot < Teams.Num(); Slot++) if (Teams[Slot] == T) Sum += S.Vp.IsValidIndex(Slot) ? S.Vp[Slot] : 0;
			Totals.Add(T, Sum);
		}
		double Highest = 0;
		for (const auto& P : Totals) Highest = FMath::Max(Highest, P.Value);
		TArray<int32> Leaders;
		for (int32 T : Ids) if (Totals[T] == Highest) Leaders.Add(T);
		const bool bLeading = Leaders.Num() == 1;
		const int32 Leading = bLeading ? Leaders[0] : 0;
		if (bLeading)
		{
			if (bHasLeader && Leader != Leading) Announce(FString::Printf(TEXT("%s takes the lead"), *TeamName(Teams, Leading)));
			bHasLeader = true; Leader = Leading;
		}
		const double Seconds = S.Tick * TCObjectiveRules::Tick;
		Samples.Add({ Seconds, Totals });
		while (Samples.Num() > 1 && Samples[1].Seconds <= Seconds - RateWindow) Samples.RemoveAt(0);
		if (Samples.Num() > 4096) Samples.RemoveAt(0, Samples.Num() - 4096); // a bound; the window keeps it far below
		const FSample& First = Samples[0];
		const double Elapsed = Seconds - First.Seconds;
		if (bLeading)
		{
			const double* Was = First.Totals.Find(Leading);
			const double Rate = Elapsed >= RateMin ? FMath::Max(0.0, Totals[Leading] - (Was ? *Was : 0.0)) / Elapsed : 0.0;
			const double Left = FMath::Max(0.0, WinVp(Teams) - Highest);
			const FString Name = TeamName(Teams, Leading);
			if (Rate > 0)
			{
				const double Eta = FMath::CeilToDouble(Left / Rate);
				Notice.Rate = Eta > 3599 ? FString::Printf(TEXT("%s leads, victory is over an hour away at this rate"), *Name) : FString::Printf(TEXT("%s wins in %s at this rate"), *Name, *MinClock(Eta));
			}
			else Notice.Rate = FString::Printf(TEXT("%s leads. %s"), *Name, Elapsed >= RateMin ? TEXT("No VP gain at this rate") : TEXT("Measuring victory rate"));
		}
		else Notice.Rate = TEXT("Scores tied");
		const bool bMyTeam = Teams.IsValidIndex(Me);
		const int32 MyTeam = bMyTeam ? Teams[Me] : 0;
		TArray<int32> Trailing;
		for (int32 T : Ids) if (Totals[T] < Highest) Trailing.Add(T);
		int32 BonusTeam = 0;
		bool bBonusTeam = false;
		if (bMyTeam && Trailing.Contains(MyTeam)) { BonusTeam = MyTeam; bBonusTeam = true; }
		else if (Trailing.Num())
		{
			Trailing.StableSort([&Totals](int32 A, int32 B) { return Totals[A] < Totals[B]; });
			BonusTeam = Trailing[0]; bBonusTeam = true;
		}
		if (bBonusTeam)
		{
			const double Bonus = FMath::Min(CatchupMax, (Highest - Totals[BonusTeam]) / CatchupPer) * S.ArmyIncome;
			const bool bMeOffline = S.Online.IsValidIndex(Me) && S.Online[Me].IsSet() && !S.Online[Me].GetValue();
			if (Bonus > 0 && !(bMyTeam && BonusTeam == MyTeam && bMeOffline))
				Notice.Bonus = FString::Printf(TEXT("%s catch-up +%s MP/s each"), *TeamName(Teams, BonusTeam), Bonus < 0.1 ? TEXT("<0.1") : *JsToFixed(Bonus, 1));
		}
	}
	else if (Kind == TEXT("horde"))
	{
		const bool bOn = S.ModeRow.bActive;
		const int32 Wave = S.ModeRow.Wave.Get(0);
		if (HordeOn.IsSet() && bOn != HordeOn.GetValue())
			Announce(bOn ? FString::Printf(TEXT("Wave %d incoming"), Wave) : FString::Printf(TEXT("Wave %d cleared. The bunker is patched up"), Wave));
		if (HordeOn.IsSet() && !HordeOn.GetValue() && bOn) QueueSound(TEXT("match_start"), 0.f, 0.7f, 1.f); // the horn for every wave
		HordeOn = bOn;
		if (bOn && S.ModeRow.Left.IsSet() && S.ModeRow.Left.GetValue() <= HordeReveal) Notice.Rate = TEXT("The last of the wave are marked through the fog");
	}
	else if (Kind == TEXT("assault") && S.ModeRow.TimeLeft.IsSet() && S.ModeRow.TimeLeft.GetValue() <= 60)
	{
		Notice.Rate = FString::Printf(TEXT("Assault ends in %s"), *MinClock(FMath::Max(0, S.ModeRow.TimeLeft.GetValue())));
	}
	if (S.ModeRow.bSuddenDeath)
	{
		if (!bSuddenDeath) Announce(TEXT("Sudden Death begins. Production has stopped"));
		bSuddenDeath = true;
		Notice.Rate = TEXT("Sudden Death: Production Buildings are crumbling");
	}
	Notice.Notice = Now() < NoticeUntil ? PendingNotice : FString();
}

void FTCObjectives::Tick()
{
	if (Notice.Notice.IsEmpty() || Now() < NoticeUntil) return;
	Notice.Notice.Reset(); PendingNotice.Reset();
}

// ---------------------------------------------------------------------------------------------------------------------
// Score strip (client/hud.js drawScores, client/weather-view.js snapshot)

double FTCObjectives::WinVp(const TArray<int32>& Teams)
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	TSet<int32> Distinct(Teams);
	return Distinct.Num() ? VpToWin * Teams.Num() / Distinct.Num() : VpToWin;
}

FTCScoreStrip FTCObjectives::ScoreStrip(const FTCMatchState& S, const FTCUnitTable* Table) const
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	FTCScoreStrip Out;
	Out.bValid = S.bHasMatch && S.Snapshots > 0;
	if (!Out.bValid) return Out;
	const FString Kind = S.ModeRow.Kind;
	const FTCModeRow& Mode = S.ModeRow;
	Out.Kind = Kind;
	const TArray<int32> Teams = TeamList(S);
	const int32 Me = S.You;
	const bool bWatching = S.bSpectator;
	TArray<int32> Tids;
	for (int32 T : Teams) Tids.AddUnique(T);
	auto Members = [&](int32 T) { TArray<int32> M; for (int32 I = 0; I < S.Names.Num(); I++) if (Teams.IsValidIndex(I) && Teams[I] == T) M.Add(I); return M; };
	auto TeamAt = [&](int32 Slot, int32& OutTeam) { if (!Teams.IsValidIndex(Slot)) return false; OutTeam = Teams[Slot]; return true; };
	auto Held = [&](int32 Slot) { int32 N = 0; for (const FTCPointRow& P : S.Points) if (P.Owner == Slot) N++; return N; };
	const int32 Mine = Teams.IsValidIndex(Me) ? Teams[Me] : Me;
	const int32 BunkerHp = HpPer(Table, TEXT("bunker"), BunkerHpPer), HqHp = HpPer(Table, TEXT("hq"), HqHpPer);
	// The live rows held, in id order (the browser's unit map keeps arrival order; id order is the stable stand-in). Only
	// the bunker and HQ modes read them.
	TArray<const FTCUnit*> Rows;
	if (Kind == TEXT("assault") || Kind == TEXT("horde") || Kind == TEXT("annihilation") || Kind == TEXT("classic"))
	{
		for (const auto& P : S.Units) if (P.Value.Type == TEXT("bunker") || P.Value.Type == TEXT("hq")) Rows.Add(&P.Value);
		Rows.Sort([](const FTCUnit& A, const FTCUnit& B) { return A.Id < B.Id; });
	}

	if (Kind != TEXT("conquest"))
	{
		FTCScoreLead& L = Out.Lead;
		L.bShown = true;
		if (Kind == TEXT("horde"))
		{
			const bool bOn = Mode.bActive;
			const int32 Wave = Mode.Wave.Get(0);
			L.Mode = bOn ? FString::Printf(TEXT("Wave %d"), Wave) : FString::Printf(TEXT("Wave %d in"), Wave + 1);
			L.Clock = bOn ? FString::Printf(TEXT("%d left"), Mode.Left.Get(0)) : HudClock(Mode.TimeLeft.Get(0));
			const TCHAR* Profile = Mode.NextProfile == TEXT("mixed") ? TEXT("Mixed forces") : Mode.NextProfile == TEXT("infantry") ? TEXT("Infantry assault")
				: Mode.NextProfile == TEXT("armor") ? TEXT("Armored assault") : Mode.NextProfile == TEXT("siege") ? TEXT("Siege weapons") : nullptr;
			if (!bOn && Profile) L.Threat = FString::Printf(TEXT("Next Wave: %s"), Profile);
			L.Title = TEXT("Hold the bunker. The next wave comes 45 seconds after this one is dead");
			L.bNextWave = !bOn && S.bAmHost;
		}
		else if (Kind == TEXT("assault"))
		{
			const bool bDefending = Mode.DefenderTeam.IsSet() && Mine == Mode.DefenderTeam.GetValue();
			L.Mode = bDefending ? TEXT("Assault: hold out") : TEXT("Assault: take the bunker");
			L.Clock = HudClock(Mode.TimeLeft.Get(0));
			L.Title = bDefending ? TEXT("Hold out until the clock runs out") : TEXT("Destroy the command bunker before the clock runs out");
		}
		else if (Kind == TEXT("tutorial"))
		{
			L.Mode = TEXT("Tutorial");
			L.Clock = FString::Printf(TEXT("%d / %d"), Mode.Step.Get(0) + 1, Mode.Steps.Get(0));
			L.Title = Mode.Goal;
		}
		else if (Kind == TEXT("world"))
		{
			const FTCWorldScore& W = S.WorldScore;
			const int32 Goal = W.Goal.IsSet() ? W.Goal.GetValue() : W.Total.Get(0);
			L.Mode = TEXT("World Conquest");
			L.Clock = FString::Printf(TEXT("%d/%d regions"), W.Owned.Get(0), Goal);
			L.Title = W.Goal.IsSet() && W.Total.IsSet() && W.Goal.GetValue() < W.Total.GetValue()
				? FString::Printf(TEXT("Own %d of the %d regions, or be the last nation standing. Destroy military bases, then claim with infantry"), W.Goal.GetValue(), W.Total.GetValue())
				: FString(TEXT("Your team must own every region. Destroy military bases, then claim with infantry"));
		}
		else if (Kind == TEXT("classic"))
		{
			L.Mode = Mode.bSuddenDeath ? TEXT("Sudden death") : TEXT("Sudden death in");
			L.Clock = Mode.bSuddenDeath ? FString() : HudClock(Mode.TimeLeft.Get(0));
			L.bModeDanger = Mode.bSuddenDeath;
			L.Title = Mode.bSuddenDeath ? TEXT("No building or training. Bases crumble: last one standing wins") : TEXT("Destroy every enemy HQ, Barracks, Motor Pool and Airfield");
		}
		else
		{
			L.Mode = TEXT("Annihilation");
			L.Title = TEXT("Destroy every enemy bunker. Last side standing wins");
		}
	}

	for (int32 T : Tids)
	{
		FTCScoreTeam& Tm = Out.Teams.AddDefaulted_GetRef();
		Tm.Team = T;
		const TArray<int32> Mem = Members(T);
		Tm.ColourSlot = Mem.Num() ? Mem[0] : -1;
		// a lone player's points repeat the team VP below, so narrow blocks (four or more teams) drop them
		const bool bPts = Mem.Num() > 1 || Tids.Num() <= 3;
		for (int32 I : Mem)
		{
			FTCScoreSeat& Seat = Tm.Seats.AddDefaulted_GetRef();
			Seat.Slot = I;
			Seat.Name = S.Names[I];
			Seat.Faction = S.Factions.IsValidIndex(I) ? S.Factions[I] : 0;
			Seat.bYou = I == Me && !bWatching;
			// hud.js net: offline, AI, the round trip, or nothing
			const bool bOffline = S.Online.IsValidIndex(I) && S.Online[I].IsSet() && !S.Online[I].GetValue();
			const bool bPing = S.Ping.IsValidIndex(I) && S.Ping[I].IsSet();
			if (bOffline) { Seat.Net = TEXT("Offline"); Seat.bNetDanger = true; }
			else if (bPing && S.Ping[I].GetValue() == -1) Seat.Net = TEXT("AI");
			else if (bPing) Seat.Net = FString::Printf(TEXT("%d ms"), S.Ping[I].GetValue());
			if (Seat.bNetDanger && Kind == TEXT("conquest")) Seat.NetTitle = TEXT("Offline: the clock is paused until they return");
			const bool bOut = S.OutSeats.IsValidIndex(I) && S.OutSeats[I];
			const int32 H = Held(I);
			if (Kind == TEXT("conquest")) Seat.Held = bPts ? FString::Printf(TEXT("%d pts, %d held"), S.Vp.IsValidIndex(I) ? S.Vp[I] : 0, H) : FString::Printf(TEXT("%d held"), H);
			else if (Kind == TEXT("world")) Seat.Held = bOut ? TEXT("Out") : TEXT("");
			else if (Kind == TEXT("assault") || (Mode.Slot.IsSet() && I == Mode.Slot.GetValue())) Seat.Held.Reset();
			else Seat.Held = bOut ? FString(TEXT("Out")) : FString::Printf(TEXT("%d held"), H);
			Seat.bHeldDanger = bOut;
		}

		auto OwnedBy = [&](const FTCUnit& U) { int32 Team; return TeamAt(U.Owner, Team) && Team == T; };
		if (Kind == TEXT("conquest"))
		{
			int32 Sum = 0;
			for (int32 I : Mem) Sum += S.Vp.IsValidIndex(I) ? S.Vp[I] : 0;
			const double Goal = WinVp(Teams);
			Tm.bBar = true; Tm.Fraction = Goal > 0 ? Sum / Goal : 0.0;
			Tm.Num = FString::Printf(TEXT("%d / %s"), Sum, *JsNumber(Goal)); Tm.U = TEXT("VP");
		}
		else if (Kind == TEXT("assault"))
		{
			const bool bDef = Mode.DefenderTeam.IsSet() && T == Mode.DefenderTeam.GetValue();
			Tm.Role = bDef ? TEXT("Defending") : TEXT("Attacking");
			if (bDef)
			{
				int32 Count = 0; double Hp = 0;
				for (const FTCUnit* U : Rows) if (U->Type == TEXT("bunker") && U->Hp > 0 && OwnedBy(*U)) { Count++; Hp += U->Hp; }
				const int32 Total = Mode.Total.IsSet() ? Mode.Total.GetValue() : Mem.Num();
				const double Max = (double)Total * BunkerHp;
				Tm.bBar = true; Tm.Fraction = Max != 0 ? Hp / Max : 0.0;
				Tm.U0 = TEXT("Structures left"); Tm.Num = FString::Printf(TEXT("%d / %d"), Count, Total);
			}
		}
		else if (Kind == TEXT("horde"))
		{
			const FTCUnit* Bunker = nullptr;
			for (const FTCUnit* U : Rows) if (U->Type == TEXT("bunker") && U->Hp > 0) { Bunker = U; break; }
			if (!Mode.DefenderTeam.IsSet() || T != Mode.DefenderTeam.GetValue()) Tm.Role = TEXT("Horde");
			else if (Bunker) { Tm.bBar = true; Tm.Fraction = (double)Bunker->Hp / BunkerHp; Tm.U0 = TEXT("Bunker"); Tm.Num = FString::Printf(TEXT("%s%%"), *JsNumber(FMath::CeilToDouble(Tm.Fraction * 100.0))); }
			else { Tm.U0 = TEXT("Bunker lost"); Tm.bU0Danger = true; }
		}
		else if (Kind == TEXT("annihilation"))
		{
			int32 Count = 0; double Hp = 0;
			for (const FTCUnit* U : Rows) if (U->Type == TEXT("bunker") && U->Hp > 0 && OwnedBy(*U)) { Count++; Hp += U->Hp; }
			const double Max = (double)(Mode.Bunkers.IsValidIndex(T) ? Mode.Bunkers[T] : Mem.Num()) * BunkerHp;
			if (Count) { Tm.bBar = true; Tm.Fraction = Max != 0 ? Hp / Max : 0.0; Tm.U0 = Count > 1 ? FString::Printf(TEXT("%d bunkers"), Count) : FString(TEXT("Bunker")); Tm.Num = FString::Printf(TEXT("%s / %s"), *JsNumber(FMath::CeilToDouble(Hp)), *JsNumber(Max)); }
			else { Tm.U0 = TEXT("Out"); Tm.bU0Danger = true; }
		}
		else if (Kind == TEXT("world"))
		{
			const FTCWorldScore& W = S.WorldScore;
			int32 Count = 0;
			if (T == Mine) Count = W.Owned.Get(0);
			else for (int32 RT : W.RegionTeams) if (RT == T) Count++;
			const int32 Total = W.Goal.IsSet() ? W.Goal.GetValue() : W.Total.Get(0);
			Tm.bBar = true; Tm.Fraction = Total ? (double)Count / Total : 0.0;
			Tm.Num = FString::Printf(TEXT("%d / %d"), Count, Total); Tm.U = TEXT("regions");
			Tm.U0 = T == Mine ? TEXT("Owned") : TEXT("Discovered");
		}
		else if (Kind == TEXT("classic"))
		{
			int32 Count = 0; double Hp = 0;
			for (const FTCUnit* U : Rows) if (U->Type == TEXT("hq") && OwnedBy(*U)) { Count++; Hp += U->Hp; }
			const double Max = (double)Count * HqHp;
			bool bAnyIn = false;
			for (int32 I : Mem) if (!(S.OutSeats.IsValidIndex(I) && S.OutSeats[I])) bAnyIn = true;
			if (!Count && bAnyIn) Tm.U0 = TEXT("HQ out of sight");
			else if (Count) { Tm.bBar = true; Tm.Fraction = Hp / Max; Tm.U0 = TEXT("HQ"); Tm.Num = FString::Printf(TEXT("%s / %s"), *JsNumber(FMath::CeilToDouble(Hp)), *JsNumber(Max)); }
			else { Tm.U0 = TEXT("Out"); Tm.bU0Danger = true; }
		}
		Tm.Fraction = FMath::Clamp(Tm.Fraction, 0.0, 1.0);
	}

	Out.Rate = Notice.Rate; Out.Bonus = Notice.Bonus; Out.Notice = Notice.Notice;
	Out.bLineShown = !Out.Rate.IsEmpty() || !Out.Bonus.IsEmpty() || !Out.Notice.IsEmpty();
	Out.OutBanner = Notice.OutBanner();
	Out.bWeather = WeatherLine(S, Out.Weather, Out.WeatherTitle);
	Out.bWeatherTurning = Out.bWeather && S.Weather.HasNext();
	return Out;
}

bool FTCObjectives::WeatherLine(const FTCMatchState& S, FString& OutText, FString& OutTitle)
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	OutText.Reset(); OutTitle.Reset();
	if (!S.bHasMatch || !S.bWeatherFromSnapshot) return false;
	const FTCWeatherRow& W = S.Weather;
	const FString& NowKind = W.Now;
	FString Base = Said(NowKind);
	if (W.HasNext())
	{
		const FString Turn = W.Next == TEXT("clear")
			? FString::Printf(TEXT("%s in %d s"), NowKind == TEXT("fog") ? TEXT("Fog lifts") : TEXT("Clearing"), W.SecondsLeft)
			: FString::Printf(TEXT("%s in %d s"), *WeatherName(W.Next), W.SecondsLeft);
		Base = FString::Printf(TEXT("%s. %s"), *Base, *Turn);
	}
	// showers come and go in Clear, Fog and Mud (wx); Rain is one all match and Snow has none
	const FWeatherDef* Def = FindWeather(NowKind);
	const bool bShowers = NowKind != TEXT("snow") && !(Def && Def->bRains);
	FString Shower;
	if (bShowers && S.Wx.bKnown)
	{
		const double Rain = S.Wx.Rain, Wet = Def && Def->bSoaks ? 0.0 : S.Wx.Wet;
		Shower = Rain > 0.3 ? FString(TEXT("Rain: vehicles slow off the road, fords deeper, sight shorter"))
			: Wet > 0.15 ? FString::Printf(TEXT("Wet ground (%s%%): vehicles slow off the road"), *JsNumber(JsRound(Wet * 100.0))) : FString();
	}
	OutText = Shower.IsEmpty() ? Base : NowKind == TEXT("clear") && !W.HasNext() ? Shower : FString::Printf(TEXT("%s. %s"), *Base, *Shower);
	OutTitle = FString::Printf(TEXT("%s. %s"), *OutText, W.HasNext() ? *FString::Printf(TEXT("Then %s"), *Said(W.Next)) : TEXT("Weather for this match. Planes and recon flights fly above it"));
	return true;
}

FString FTCObjectives::EconomyLine(const FTCMatchState& S)
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	if (!S.Inc.IsSet()) return FString();
	const bool bUpkeep = S.Upkeep.IsSet() && S.Upkeep.GetValue() != 0.0;
	return FString::Printf(TEXT("+%s/s%s"), *JsNumber(S.Inc.GetValue()), bUpkeep ? *FString::Printf(TEXT(" (upkeep %s%s)"), Minus, *JsNumber(S.Upkeep.GetValue())) : TEXT(""));
}

FString FTCObjectives::AirStateText(const FTCAirRow& Row)
{
	static const TCHAR* const States[] = { TEXT("Ready"), TEXT("Flying out"), TEXT("On station"), TEXT("Heading home"), TEXT("Rearming") };
	const FString Name = Row.State >= 0 && Row.State < (int32)UE_ARRAY_COUNT(States) ? FString(States[Row.State]) : FString();
	return Row.State == 2 ? FString::Printf(TEXT("%s %ds"), *Name, Row.Fuel) : Row.State == 4 ? FString::Printf(TEXT("%s %ds"), *Name, Row.Timer) : Name;
}

FString FTCObjectives::GoalText(const FTCMatchState& S)
{
	// objectives.js snapshot: one source for the goal box (FTCScenarioText, round 14), which also knows its scenario class
	return FTCScenarioText::GoalText(S).Text;
}

// ---------------------------------------------------------------------------------------------------------------------
// The end of the match (client/epilogue.js, client/main.js renderLobby, client/report.js)

void FTCObjectives::UpdateEpilogue(const FTCMatchState& S)
{
	if (Ending.bActive || S.Winner < -1 || !S.End.bPresent) return;
	const int32 Team = S.TeamOf(S.You); // epilogue.snapshot(s, teams[me] ?? me)
	const ETCOutcome Outcome = S.Winner == -1 ? ETCOutcome::Draw : S.Winner == Team ? ETCOutcome::Victory : ETCOutcome::Defeat;
	const bool bWin = Outcome == ETCOutcome::Victory, bDraw = Outcome == ETCOutcome::Draw;
	const FString& Reason = S.End.Reason;
	FString Why;
	if (S.ModeRow.Kind == TEXT("world") && S.ModeRow.bPresent)
		Why = bDraw ? TEXT("No side controls the continent.") : Reason == TEXT("nations") ? (bWin ? TEXT("Every rival nation has fallen.") : TEXT("Your nation has fallen."))
			: bWin ? TEXT("Your side controls the continent.") : TEXT("Another side controls the continent.");
	else if (S.ModeRow.Kind == TEXT("horde") && S.ModeRow.bPresent)
		Why = FString::Printf(TEXT("The bunker fell on wave %s."), S.ModeRow.Wave.IsSet() ? *FString::FromInt(S.ModeRow.Wave.GetValue()) : TEXT("undefined"));
	else if (bDraw) Why = TEXT("No side is left standing.");
	else if (Reason == TEXT("vp")) Why = bWin ? TEXT("Your side reached the victory points first.") : TEXT("The enemy reached the victory points first.");
	else if (Reason == TEXT("bunkers")) Why = bWin ? TEXT("Your bunkers are the last ones standing.") : TEXT("The enemy's bunkers are the last ones standing.");
	else if (Reason == TEXT("structures")) Why = bWin ? TEXT("Every defending structure has fallen.") : TEXT("Every structure you defended has fallen.");
	else if (Reason == TEXT("timer")) Why = bWin ? TEXT("The defenses held until the clock ran out.") : TEXT("The clock ran out before the defenses fell.");
	else if (Reason == TEXT("hq")) Why = bWin ? TEXT("Your side has the last Production Buildings standing.") : TEXT("The enemy has the last Production Buildings standing.");
	else if (Reason == TEXT("tutorial") && bWin) Why = TEXT("The bridge is ours. Training complete: you are ready for a real match.");
	Ending = FTCEpilogue();
	Ending.bActive = true;
	Ending.Outcome = Outcome;
	Ending.Word = bWin ? TEXT("Victory") : bDraw ? TEXT("Draw") : TEXT("Defeat");
	Ending.Why = Why;
	Ending.EndAt = FVector2D(S.End.X, S.End.Z);
	Ending.StartedAt = Now();
	// existing sounds only: the point-won call and the match horn for a win, the point-lost call (lower) otherwise
	if (bWin) { QueueSound(TEXT("alert_point_won"), 0.f, 0.9f, 1.f); QueueSound(TEXT("match_start"), 0.7f, 0.8f, 1.f); }
	else
	{
		QueueSound(TEXT("alert_point_lost"), 0.f, 0.9f, Outcome == ETCOutcome::Defeat ? 0.85f : 1.f);
		if (Outcome == ETCOutcome::Defeat) QueueSound(TEXT("alert_point_lost"), 0.55f, 0.6f, 0.7f);
	}
}

double FTCObjectives::EpilogueFrame(double Dt, FTCEpilogueCamera& Cam)
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	if (!Ending.bActive) return Dt;
	auto Smooth = [](double T) { return T * T * (3 - 2 * T); };
	const double Was = Ending.Elapsed;
	Ending.Elapsed = Now() - Ending.StartedAt;
	if (!Ending.bHasFrom) { Ending.bHasFrom = true; Ending.From = FVector2D(Cam.X, Cam.Z); Ending.FromDist = Cam.Dist; }
	if (Was < Glide)
	{
		const double K = Smooth(FMath::Min(1.0, Ending.Elapsed / Glide));
		Cam.X = Ending.From.X + (Ending.EndAt.X - Ending.From.X) * K;
		Cam.Z = Ending.From.Y + (Ending.EndAt.Y - Ending.From.Y) * K;
		Cam.Dist = Ending.FromDist + (View - Ending.FromDist) * K;
	}
	Ending.Speed = 1 - (1 - Slow) * Smooth(FMath::Min(1.0, Ending.Elapsed / EaseIn));
	return Dt * Ending.Speed;
}

FString FTCObjectives::ResultLine(const FTCMatchResult& R, int32 Me)
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	if (!R.bPresent) return FString();
	if (R.bEnded) return TEXT("Match ended by the host");
	if (R.bHorde)
		return FString::Printf(TEXT("Overrun on wave %d: %d kills in %s%s"), R.HordeWave, R.HordeKills, *MinClock(R.HordeTime),
			R.bHordeRecord ? TEXT(", a new record") : R.bHordeBest ? *FString::Printf(TEXT(". Record: wave %d"), R.HordeBestWave) : TEXT(""));
	// r.teams[i] === w, with JavaScript's undefined for a missing winner or seat (a null team, -1 here, equals nothing)
	auto Equal = [&R](int32 I)
	{
		const bool bSeat = R.Teams.IsValidIndex(I);
		if (!R.bHasWinner) return !bSeat;
		return bSeat && R.Teams[I] >= 0 && R.Teams[I] == R.Winner;
	};
	if (R.bHasWinner && R.Winner == -1) return TEXT("Draw");
	if (Equal(Me)) return TEXT("Victory");
	TArray<FString> Winners;
	for (int32 I = 0; I < R.Names.Num(); I++) if (Equal(I)) Winners.Add(R.Names[I]);
	int32 Count = 0;
	for (int32 I = 0; I < R.Teams.Num(); I++) if (Equal(I)) Count++;
	const FString Who = FString::Join(Winners, TEXT(" & "));
	return FString::Printf(TEXT("%s win%s"), Who.IsEmpty() ? TEXT("Enemy") : *Who, Count > 1 ? TEXT("") : TEXT("s"));
}

const TCHAR* FTCMatchReport::Column(int32 I)
{
	static const TCHAR* const Labels[] = { TEXT("Kills"), TEXT("Losses"), TEXT("Built"), TEXT("Captures"), TEXT("Manpower spent"), TEXT("Support calls"), TEXT("Planes downed") };
	return I >= 0 && I < 7 ? Labels[I] : TEXT("");
}

FTCMatchReport FTCObjectives::BuildReport(const FTCMatchResult& R, int32 Me)
{
	using namespace TCObjectivesImpl;
	using namespace TCObjectiveRules;
	FTCMatchReport Out;
	// report.js: hidden without a story or a timeline (a match the host ended)
	if (!R.bPresent || !R.bHasStory || R.Timeline.Num() == 0) return Out;
	Out.bShown = true;
	const bool bVp = R.Mode == TEXT("conquest");
	Out.What = bVp ? TEXT("Victory points") : TEXT("Points held");
	auto Val = [bVp](const FTCTimelineSample& S, int32 Team) { const TArray<double>& A = bVp ? S.Vp : S.Points; return A.IsValidIndex(Team) ? A[Team] : 0.0; };
	double Top = bVp ? (R.WinVp.IsSet() ? R.WinVp.GetValue() : 0.0) : 1.0;
	for (const FTCTimelineSample& S : R.Timeline) for (double V : (bVp ? S.Vp : S.Points)) Top = FMath::Max(Top, V);
	Out.Top = Top;
	Out.End = FMath::Max(1.0, R.Timeline.Last().T);
	Out.TopLabel = JsNumber(Top);
	Out.EndLabel = ReportClock(Out.End);
	Out.Sub = R.bHorde ? FString::Printf(TEXT("overrun on wave %d"), R.HordeWave) : FString::Printf(TEXT("decided %s"),
		R.Reason == TEXT("vp") ? TEXT("on victory points") : R.Reason == TEXT("bunkers") ? TEXT("when the last bunker fell") : R.Reason == TEXT("structures") ? TEXT("when the last defending structure fell")
		: R.Reason == TEXT("timer") ? TEXT("when the clock ran out") : R.Reason == TEXT("hq") ? TEXT("when the last Production Building fell") : R.Reason == TEXT("draw") ? TEXT("in a draw")
		: R.Reason == TEXT("tutorial") ? TEXT("when the bridge was taken") : TEXT(""));
	Out.Sub += FString::Printf(TEXT(" after %s"), *ReportClock(R.EndTick / 20.0));

	// sides(): the teams that played, in id order. A seat added after the match has a null team and no side here.
	TArray<int32> Ids;
	for (int32 T : R.Teams) if (T >= 0) Ids.AddUnique(T);
	Ids.Sort();
	const FTCTimelineSample& Last = R.Timeline.Last();
	for (int32 T : Ids)
	{
		FTCReportSide Side;
		Side.Team = T;
		TArray<FString> Names;
		for (int32 I = 0; I < R.Teams.Num(); I++) if (R.Teams[I] == T) { Side.Slots.Add(I); Names.Add(R.Names.IsValidIndex(I) ? R.Names[I] : FString::Printf(TEXT("Player %d"), I + 1)); }
		Side.ColourSlot = Side.Slots.Num() ? Side.Slots[0] : -1;
		Side.Name = FString::Join(Names, TEXT(" & "));
		Side.bWon = R.bHasWinner && T == R.Winner;
		const FString Final = bVp ? FString::Printf(TEXT("%s victory points"), *JsNumber(Val(Last, T))) : FString::Printf(TEXT("%s points held"), *JsNumber(Val(Last, T)));
		Side.Legend = FString::Printf(TEXT("%s: %s%s"), *Side.Name, *Final, Side.bWon ? TEXT(" (won)") : TEXT(""));
		for (const FTCTimelineSample& S : R.Timeline) Side.Series.Add(FVector2D(S.T, Val(S, T)));
		Out.Sides.Add(MoveTemp(Side));
	}
	Out.Sides.StableSort([](const FTCReportSide& A, const FTCReportSide& B) { return !A.bWon && B.bWon; }); // the winners drawn last
	for (int32 I = 0; I < R.Names.Num(); I++)
	{
		FTCReportRow Row;
		Row.Slot = I; Row.Name = R.Names[I];
		Row.bMe = R.bHasYou && I == Me;
		if (R.Story.IsValidIndex(I) && R.Story[I].bPresent)
		{
			const FTCStoryRow& S = R.Story[I];
			const int32 V[7] = { S.Kills, S.Losses, S.Built, S.Captures, S.MpSpent, S.SupportCalls, S.PlanesDowned };
			for (int32 K = 0; K < 7; K++) Row.Values[K] = V[K];
		}
		Out.Rows.Add(MoveTemp(Row));
	}
	return Out;
}

// ---------------------------------------------------------------------------------------------------------------------
// Automation tests.
//
// Expected values come from two sources, named per check:
//  - the recorded protocol streams (native rust/crates/core/tests/fixtures, installed by unreal/Tools/ww2-ffi.sh under
//    the WW2Core plugin), read straight from their JSON for the decode checks;
//  - the reference client's own modules at pin e4e579a (client/endgame.js, client/objectives.js, client/epilogue.js,
//    client/report.js, client/world-state.js, and the result line of client/main.js renderLobby) run under Node over
//    the same streams, or over crafted end-of-match messages shaped as server.js and shared/sim.js send them (no
//    recorded stream reaches the end of a match). That oracle script lives outside the repository; its outputs are the
//    literals below. The per-snapshot sequences are compared through FNV-1a 64 digests of one line per snapshot.

// ---------------------------------------------------------------------------------------------------------------------
// FTCModeWatch: the mode coverage lines (verification aid, see TCObjectives.h)

void FTCModeWatch::Reset()
{
	bStarted = bDecided = bSummarized = false;
	MatchId = -1; Snapshots = Strips = Goals = Says = 0; LastStripTick = -1000000000;
	ModeKind.Reset(); LastShape.Reset(); LastGoal.Reset(); DecidedText.Reset();
	Pending.Reset();
}

void FTCModeWatch::OnStart(const FTCMatchState& S)
{
	// a reconnect to the same match goes on counting; a new match starts over (its summary first)
	if (bStarted && S.MatchId == MatchId) { Pending.Add(FString::Printf(TEXT("modes: start again: match %d (a reconnect), seat %d"), S.MatchId, S.You)); return; }
	if (bStarted) OnClear(TEXT("a new match"));
	TArray<FString> P = MoveTemp(Pending);
	Reset();
	Pending = MoveTemp(P);
	bStarted = true;
	MatchId = S.MatchId;
	TArray<FString> Teams;
	for (int32 T : S.Teams) Teams.Add(FString::FromInt(T));
	// a spectator has no seat (the server's start says seat 0, its view's); one that joins a match in play gets the start
	// before any lobby, so the mode is not known yet
	const FString Seat = S.bSpectator ? FString::Printf(TEXT("watching as a spectator (%d seats)"), S.Names.Num()) : FString::Printf(TEXT("seat %d of %d (team %d)"), S.You, S.Names.Num(), S.TeamOf(S.You));
	const FString Mode = !S.StartMode.IsEmpty() ? S.StartMode : !S.Mode.IsEmpty() ? S.Mode : FString(TEXT("not known yet"));
	Pending.Add(FString::Printf(TEXT("modes: start: match %d on '%s' (%dx%d), %s, teams [%s], lobby mode %s, %d authored point(s)"),
		S.MatchId, *S.Map.Name, S.Map.W, S.Map.H, *Seat, *FString::Join(Teams, TEXT(",")), *Mode, S.MapPoints.Num()));
}

FString FTCModeWatch::StripText(const FTCScoreStrip& Strip, const FTCMatchState& S)
{
	TArray<FString> Parts;
	const FTCScoreLead& L = Strip.Lead;
	if (L.bShown)
		Parts.Add(FString::Printf(TEXT("%s%s%s%s%s"), *L.Mode, L.Clock.IsEmpty() ? TEXT("") : *(TEXT(" ") + L.Clock), L.Title.IsEmpty() ? TEXT("") : *(TEXT(" - ") + L.Title),
			L.Threat.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" [%s]"), *L.Threat), L.bNextWave ? TEXT(" [Send next wave]") : TEXT("")));
	else Parts.Add(Strip.Kind.IsEmpty() ? FString(TEXT("conquest")) : Strip.Kind);
	for (const FTCScoreTeam& T : Strip.Teams)
	{
		TArray<FString> Names;
		for (const FTCScoreSeat& Seat : T.Seats)
			Names.Add(FString::Printf(TEXT("%s%s%s"), *Seat.Name, Seat.bYou ? TEXT(" (you)") : TEXT(""), Seat.Held.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(": %s"), *Seat.Held)));
		TArray<FString> V;
		for (const FString& X : { T.Role, T.U0, T.Num, T.U }) if (!X.IsEmpty()) V.Add(X);
		Parts.Add(FString::Printf(TEXT("team %d%s%s (%s)"), T.Team, V.Num() ? TEXT(" ") : TEXT(""), *FString::Join(V, TEXT(" ")), *FString::Join(Names, TEXT(", "))));
	}
	TArray<FString> Line;
	for (const FString& X : { Strip.Rate, Strip.Bonus, Strip.Notice }) if (!X.IsEmpty()) Line.Add(X);
	if (Line.Num()) Parts.Add(TEXT("line: ") + FString::Join(Line, TEXT(" / ")));
	if (!Strip.OutBanner.IsEmpty()) Parts.Add(TEXT("banner: ") + Strip.OutBanner);
	return FString::Join(Parts, TEXT(" | "));
}

FString FTCModeWatch::StripShape(const FTCScoreStrip& Strip)
{
	// the lead's mode word (it carries the wave and the Sudden Death switch), title and threat; each team's role and
	// labels; an "Out" seat; the notice (an event), not the rate line that counts down
	FString K = Strip.Kind + TEXT("|") + Strip.Lead.Mode + TEXT("|") + Strip.Lead.Title + TEXT("|") + Strip.Lead.Threat + (Strip.Lead.bNextWave ? TEXT("|next") : TEXT(""));
	for (const FTCScoreTeam& T : Strip.Teams)
	{
		K += FString::Printf(TEXT("|%d:%s:%s:%s:%d"), T.Team, *T.Role, *T.U0, *T.U, T.Seats.Num());
		for (const FTCScoreSeat& Seat : T.Seats) if (Seat.Held == TEXT("Out")) K += TEXT(":out") + FString::FromInt(Seat.Slot);
	}
	return K + TEXT("|") + Strip.Notice + TEXT("|") + Strip.OutBanner;
}

void FTCModeWatch::OnSnapshot(const FTCMatchState& S, const FTCUnitTable* Table, const FJsonObject* Raw)
{
	if (!S.bHasMatch) return;
	if (!bStarted) OnStart(S);
	Snapshots++;
	if (S.ModeRow.bPresent) ModeKind = S.ModeRow.Kind; else if (ModeKind.IsEmpty()) ModeKind = TEXT("conquest");
	// the score strip
	const FTCScoreStrip Strip = S.Objectives.ScoreStrip(S, Table);
	if (Strip.bValid)
	{
		const FString Shape = StripShape(Strip);
		if (Shape != LastShape || S.Tick - LastStripTick >= PeriodTicks)
		{
			LastShape = Shape; LastStripTick = S.Tick; Strips++;
			FString Text = StripText(Strip, S);
			if (FTCLocale::Language() != ETCLanguage::English && Strip.Lead.bShown)
			{
				const FString Shown = FTCLocale::T(Strip.Lead.Mode) + (Strip.Lead.Title.IsEmpty() ? FString() : TEXT(" - ") + FTCLocale::T(Strip.Lead.Title));
				Text += TEXT(" | shown: ") + Shown;
			}
			Pending.Add(FString::Printf(TEXT("modes: strip at tick %d: %s"), S.Tick, *Text));
		}
	}
	// the goal box (objectives.js): its text, and the reader's language as the HUD draws it (TC_T)
	const FTCGoalText Goal = FTCScenarioText::GoalText(S);
	if (Goal.Text != LastGoal)
	{
		LastGoal = Goal.Text;
		if (Goal.IsShown()) Goals++;
		const FString Flat = Goal.Text.Replace(TEXT("\n"), TEXT(" / "));
		const FString Shown = FTCLocale::T(Goal.Text).Replace(TEXT("\n"), TEXT(" / "));
		Pending.Add(FString::Printf(TEXT("modes: goal at tick %d: %s%s"), S.Tick, Goal.IsShown() ? *Flat : TEXT("(none)"), Goal.IsShown() && Shown != Flat ? *FString::Printf(TEXT(" | shown: %s"), *Shown) : TEXT("")));
	}
	// the 'say' lines (client/alerts.js: localized[lang] ?? text, through TC_T)
	const TArray<TSharedPtr<FJsonValue>>* Shots = nullptr;
	if (Raw && Raw->TryGetArrayField(TEXT("shots"), Shots))
		for (const TSharedPtr<FJsonValue>& V : *Shots)
		{
			const TSharedPtr<FJsonObject>* O = nullptr;
			FString K, Text, Shown;
			if (!V.IsValid() || !V->TryGetObject(O) || !(*O)->TryGetStringField(TEXT("k"), K) || K != TEXT("say")) continue;
			(*O)->TryGetStringField(TEXT("text"), Text);
			if (!FTCLocale::SayText(**O, Shown)) Shown = Text;
			Shown = FTCLocale::T(Shown);
			Says++;
			Pending.Add(FString::Printf(TEXT("modes: say at tick %d: %s%s"), S.Tick, *Text, Shown != Text ? *FString::Printf(TEXT(" | shown: %s"), *Shown) : TEXT("")));
		}
	// decided: the first snapshot with a winner and an end (epilogue.js begins there)
	if (!bDecided && S.Winner >= -1 && S.End.bPresent)
	{
		bDecided = true;
		const FTCEpilogue& E = S.Objectives.Epilogue();
		const TCHAR* Outcome = S.bSpectator ? TEXT("watching") : S.Winner == -1 ? TEXT("draw") : S.Winner == S.TeamOf(S.You) ? TEXT("won") : TEXT("lost");
		DecidedText = FString::Printf(TEXT("at tick %d (%d min %d s): winner team %d (%s), reason %s"), S.Tick, S.Tick / 1200, (S.Tick / 20) % 60, S.Winner, Outcome, *S.End.Reason);
		Pending.Add(FString::Printf(TEXT("modes: decided %s; stamp '%s' '%s'"), *DecidedText, *E.Word, *E.Why));
	}
}

void FTCModeWatch::OnClear(const TCHAR* Why)
{
	if (!bStarted || bSummarized) return;
	bSummarized = true;
	Pending.Add(FString::Printf(TEXT("modes: summary (%s): mode %s, match %d, %d snapshot(s), strip lines %d, goal lines %d, say lines %d, decided %s"), Why, *ModeKind, MatchId, Snapshots, Strips, Goals, Says,
		bDecided ? *DecidedText : TEXT("no")));
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "WW2ViewReplay.h"

namespace TCObjectivesTest
{
	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> Obj;
		const TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);
		return FJsonSerializer::Deserialize(Reader, Obj) && Obj.IsValid() ? Obj : nullptr;
	}

	uint64 Fnv(const TArray<FString>& Lines)
	{
		const FString Joined = FString::Join(Lines, TEXT("\n"));
		const FTCHARToUTF8 Utf8(*Joined);
		uint64 H = 0xcbf29ce484222325ull;
		for (int32 I = 0; I < Utf8.Length(); I++) { H ^= (uint8)Utf8.Get()[I]; H *= 0x100000001b3ull; }
		return H;
	}

	// Feeds one message to the state the way TCGameMode does; Clock follows the snapshot's tick (tick * 50 ms, the clock
	// the oracle ran with).
	FString Apply(FTCMatchState& St, double& Clock, const FJsonObject& M)
	{
		FString Type;
		M.TryGetStringField(TEXT("t"), Type);
		if (Type == TEXT("lobby")) St.ApplyLobby(M);
		else if (Type == TEXT("start")) St.ApplyStart(M);
		else if (Type == TEXT("pause")) St.ApplyPause(M);
		else if (Type == TEXT("deny")) St.ApplyDeny(M);
		else if (Type == TEXT("s"))
		{
			double T = 0;
			M.TryGetNumberField(TEXT("tick"), T);
			Clock = (T * 50.0) / 1000.0;
			St.ApplySnapshot(M);
		}
		return Type;
	}

	const TArray<TSharedPtr<FJsonValue>>* Arr(const FJsonObject& O, const TCHAR* Name)
	{
		const TArray<TSharedPtr<FJsonValue>>* A = nullptr;
		return O.TryGetArrayField(Name, A) ? A : nullptr;
	}
	bool IsNum(const TSharedPtr<FJsonValue>& V) { return V.IsValid() && V->Type == EJson::Number; }

	// The decoded snapshot fields against the raw message, written separately from the decoder. Empty: they agree.
	FString CompareRaw(const FTCMatchState& St, const FJsonObject& M)
	{
		const TArray<TSharedPtr<FJsonValue>>* A;
		// points
		A = Arr(M, TEXT("points"));
		if ((A ? A->Num() : 0) != St.Points.Num()) return TEXT("points count");
		for (int32 I = 0; A && I < A->Num(); I++)
		{
			const TArray<TSharedPtr<FJsonValue>>& R = (*A)[I]->AsArray();
			const FTCPointRow& P = St.Points[I];
			if (P.Owner != (int32)R[0]->AsNumber() || P.Capper != (int32)R[1]->AsNumber() || P.Progress != R[2]->AsNumber()) return FString::Printf(TEXT("point %d owner, capper or progress"), I);
			if (P.bContested != (R[3]->AsNumber() != 0) || P.bCut != (R[4]->AsNumber() != 0) || P.bLocked != (R[5]->AsNumber() != 0)) return FString::Printf(TEXT("point %d flags"), I);
		}
		// vp, online, ping
		A = Arr(M, TEXT("vp"));
		if ((A ? A->Num() : 0) != St.Vp.Num()) return TEXT("vp count");
		for (int32 I = 0; A && I < A->Num(); I++) if (St.Vp[I] != (int32)(*A)[I]->AsNumber()) return FString::Printf(TEXT("vp %d"), I);
		A = Arr(M, TEXT("online"));
		if ((A ? A->Num() : 0) != St.Online.Num()) return TEXT("online count");
		for (int32 I = 0; A && I < A->Num(); I++) if (!St.Online[I].IsSet() || St.Online[I].GetValue() != (*A)[I]->AsBool()) return FString::Printf(TEXT("online %d"), I);
		A = Arr(M, TEXT("ping"));
		if ((A ? A->Num() : 0) != St.Ping.Num()) return TEXT("ping count");
		for (int32 I = 0; A && I < A->Num(); I++)
		{
			const bool bNull = (*A)[I]->IsNull();
			if (bNull != !St.Ping[I].IsSet() || (!bNull && St.Ping[I].GetValue() != (int32)(*A)[I]->AsNumber())) return FString::Printf(TEXT("ping %d"), I);
		}
		// shots: count, kind, and the shooter and target exactly as delivered
		A = Arr(M, TEXT("shots"));
		if ((A ? A->Num() : 0) != St.Shots.Num()) return TEXT("shots count");
		for (int32 I = 0; A && I < A->Num(); I++)
		{
			const FJsonObject& O = *(*A)[I]->AsObject();
			const FTCShot& S = St.Shots[I];
			double F = 0, T = 0;
			const bool bF = O.TryGetNumberField(TEXT("f"), F) && F != 0, bT = O.TryGetNumberField(TEXT("t"), T) && T != 0;
			if (S.K != O.GetStringField(TEXT("k")) || S.bHasShooter != bF || (bF && S.F != (int32)F) || S.bHasTarget != bT || (bT && S.T != (int32)T)) return FString::Printf(TEXT("shot %d"), I);
		}
		A = Arr(M, TEXT("flights"));
		if ((A ? A->Num() : 0) != St.Flights.Num()) return TEXT("flights count");
		for (int32 I = 0; A && I < A->Num(); I++)
		{
			const FJsonObject& O = *(*A)[I]->AsObject();
			if (St.Flights[I].Flight != (int32)O.GetNumberField(TEXT("flight")) || St.Flights[I].bHasShooter != O.HasField(TEXT("f")) || St.Flights[I].Until != O.GetNumberField(TEXT("until"))) return FString::Printf(TEXT("flight %d"), I);
		}
		// fires, falling, foundMines, ghosts (every recorded cell is on the map)
		A = Arr(M, TEXT("fires"));
		if ((A ? A->Num() : 0) != St.Fires.Num()) return TEXT("fires count");
		A = Arr(M, TEXT("falling"));
		if ((A ? A->Num() : 0) != St.Falling.Num()) return TEXT("falling count");
		A = Arr(M, TEXT("foundMines"));
		if ((A ? A->Num() : 0) != St.FoundMines.Num()) return TEXT("foundMines count");
		int32 Ghosts = 0;
		if ((A = Arr(M, TEXT("ghosts")))) for (const auto& V : *A) if (!St.Units.Contains((int32)V->AsArray()[0]->AsNumber())) Ghosts++;
		if (Ghosts != St.Ghosts.Num()) return TEXT("ghosts");
		// plans and waiting orders: only units held (orders: only this seat's)
		int32 Plans = 0, Orders = 0, Air = 0;
		if ((A = Arr(M, TEXT("plans")))) for (const auto& V : *A) if (St.Units.Contains((int32)V->AsArray()[0]->AsNumber())) Plans++;
		if ((A = Arr(M, TEXT("orders")))) for (const auto& V : *A) { const FTCUnit* U = St.Units.Find((int32)V->AsArray()[0]->AsNumber()); if (U && U->Owner == St.You) Orders++; }
		if ((A = Arr(M, TEXT("air")))) for (const auto& V : *A) if (St.Units.Contains((int32)V->AsArray()[0]->AsNumber())) Air++;
		if (Plans != St.Plans.Num() || Orders != St.WaitingOrders.Num() || Air != St.Air.Num()) return TEXT("plans, orders or air");
		if ((A = Arr(M, TEXT("plans"))))
			for (const auto& V : *A)
			{
				const TArray<TSharedPtr<FJsonValue>>& R = V->AsArray();
				const FTCPlan* P = St.Plans.FindByPredicate([&R](const FTCPlan& X) { return X.Id == (int32)R[0]->AsNumber(); });
				if (P && (P->Kind != (int32)R[1]->AsNumber() || P->Tx != R[2]->AsNumber() || P->Path.Num() != (R.Num() - 4) / 2)) return FString::Printf(TEXT("plan %d"), P->Id);
			}
		// weather and wx, the economy
		if ((A = Arr(M, TEXT("weather"))) && (St.Weather.Now != (*A)[0]->AsString() || St.Weather.HasNext() != (A->Num() > 1))) return TEXT("weather");
		if ((A = Arr(M, TEXT("wx"))) && (!St.Wx.bKnown || St.Wx.Rain != (*A)[0]->AsNumber() || St.Wx.Wet != (*A)[1]->AsNumber() || St.Wx.WindAngle != (*A)[2]->AsNumber() || St.Wx.WindStrength != (*A)[3]->AsNumber())) return TEXT("wx");
		const TCHAR* Money[] = { TEXT("inc"), TEXT("fuelInc"), TEXT("upkeep") };
		const TOptional<double>* Held[] = { &St.Inc, &St.FuelInc, &St.Upkeep };
		for (int32 I = 0; I < 3; I++)
		{
			double V;
			const bool bHas = M.TryGetNumberField(Money[I], V);
			if (bHas != Held[I]->IsSet() || (bHas && Held[I]->GetValue() != V)) return FString::Printf(TEXT("%s"), Money[I]);
		}
		// no recorded stream reaches the end: no winner, no end
		if (St.Winner != -2 || St.End.bPresent) return TEXT("winner or end");
		return FString();
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCObjectivesRecordedTest, "ThreeCrossroads.Objectives.Recorded", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCObjectivesRecordedTest::RunTest(const FString& Parameters)
{
	using namespace TCObjectivesTest;
	const FString Dir = FWW2ViewReplay::ProtocolDir();
	struct FExpect
	{
		const TCHAR* Stem;
		int32 Snapshots;
		uint64 NoticesFnv, PointsFnv;  // the oracle's per-snapshot endgame.js state and objectives.js ring inputs
		int32 Shots, ShotsWithShooter, ShotsWithTarget, Flights, FlightsWithShooter; // summed over the stream (raw JSON)
		int32 FinalHeld;               // client/world-state.js rows held after the last snapshot (oracle)
		const TCHAR* FinalRate;        // endgame.js after the last snapshot (oracle)
		const TCHAR* FinalBonus;
		const TCHAR* Economy;          // hud.js drawEcon's line from the last snapshot's inc and upkeep (raw JSON)
	};
	const FExpect Streams[] = {
		{ TEXT("protocol-conquest-p0"), 500, 0xabf1a9bc7d5ae518ull, 0xd029cf14cf9957b9ull, 384, 373, 47, 198, 193, 9, TEXT("Orange wins in 17:26 at this rate"), TEXT("Blue catch-up +0.4 MP/s each"), TEXT("+4.4/s") },
		{ TEXT("protocol-conquest-p1"), 500, 0xc80a5c61cc0a336cull, 0xd029cf14cf9957b9ull, 379, 373, 47, 199, 199, 10, TEXT("Orange wins in 17:26 at this rate"), TEXT("Red catch-up +0.4 MP/s each"), TEXT("+4.4/s") },
		{ TEXT("protocol-world-p0"), 100, 0xed710f444c71da1full, 0x74c90a2b0d0561cfull, 0, 0, 0, 0, 0, 3, TEXT(""), TEXT(""), TEXT("+4.1/s (upkeep \u22120.1)") },
		{ TEXT("protocol-world-p1"), 100, 0xed710f444c71da1full, 0x74c90a2b0d0561cfull, 0, 0, 0, 0, 0, 3, TEXT(""), TEXT(""), TEXT("+3.5/s (upkeep \u22120.1)") },
	};
	int32 Ran = 0;
	for (const FExpect& E : Streams)
	{
		const FString Path = Dir / (FString(E.Stem) + TEXT(".jsonl"));
		TArray<FString> Lines;
		if (Dir.IsEmpty() || !FFileHelper::LoadFileToStringArray(Lines, *Path))
		{
			AddWarning(FString::Printf(TEXT("%s: recorded stream not installed (%s); its checks were NOT run. Run unreal/Tools/ww2-ffi.sh build."), E.Stem, *Path));
			continue;
		}
		Ran++;
		FTCMatchState St;
		double Clock = 0.0;
		St.Objectives.Clock = [&Clock]() { return Clock; };
		TArray<FString> NoticeLines, PointLines, Problems;
		int32 Snapshots = 0, Shots = 0, WithShooter = 0, WithTarget = 0, Flights = 0, FlightsWithShooter = 0;
		for (int32 L = 0; L < Lines.Num(); L++)
		{
			if (Lines[L].IsEmpty()) continue;
			const TSharedPtr<FJsonObject> M = Parse(Lines[L]);
			if (!TestTrue(FString::Printf(TEXT("%s:%d parses"), E.Stem, L + 1), M.IsValid())) return false;
			if (Apply(St, Clock, *M) != TEXT("s")) continue;
			Snapshots++;
			const FString Diff = CompareRaw(St, *M);
			if (!Diff.IsEmpty() && Problems.Num() < 5) Problems.Add(FString::Printf(TEXT("line %d (tick %d): %s"), L + 1, St.Tick, *Diff));
			for (const FTCShot& S : St.Shots) { Shots++; WithShooter += S.bHasShooter; WithTarget += S.bHasTarget; }
			for (const FTCFlight& F : St.Flights) { Flights++; FlightsWithShooter += F.bHasShooter; }
			const FTCMatchNotices& N = St.Objectives.Notices();
			NoticeLines.Add(FString::Printf(TEXT("%d|%s|%s|%s|%d"), St.Tick, *N.Rate, *N.Bonus, *N.Notice, N.bOut ? 1 : 0));
			TArray<FString> Rows;
			for (int32 I = 0; I < St.Objectives.Points().Num(); I++)
			{
				const FTCCapturePoint& C = St.Objectives.Points()[I];
				Rows.Add(FString::Printf(TEXT("%d,%d,%s,%d,%d,%d,%d"), C.RingSlot, C.Capper >= 0 ? C.Capper : -1, *FTCObjectives::JsNumber(C.Progress), C.bContested ? 1 : 0, C.CutTag() ? 1 : 0, C.LockTag() ? 1 : 0, St.Objectives.RingFrame(I).bFlip ? 1 : 0));
			}
			PointLines.Add(FString::Printf(TEXT("%d|%s"), St.Tick, *FString::Join(Rows, TEXT(";"))));
		}
		for (const FString& P : Problems) AddError(FString::Printf(TEXT("%s: decoded field differs from the raw message: %s"), E.Stem, *P));
		TestEqual(FString::Printf(TEXT("%s snapshots"), E.Stem), Snapshots, E.Snapshots);
		TestEqual(FString::Printf(TEXT("%s shots"), E.Stem), Shots, E.Shots);
		TestEqual(FString::Printf(TEXT("%s shots naming a shooter"), E.Stem), WithShooter, E.ShotsWithShooter);
		TestEqual(FString::Printf(TEXT("%s shots naming a target"), E.Stem), WithTarget, E.ShotsWithTarget);
		TestEqual(FString::Printf(TEXT("%s flights"), E.Stem), Flights, E.Flights);
		TestEqual(FString::Printf(TEXT("%s flights naming a shooter"), E.Stem), FlightsWithShooter, E.FlightsWithShooter);
		TestEqual(FString::Printf(TEXT("%s rows held"), E.Stem), St.Units.Num() + St.UnreadableIds.Num(), E.FinalHeld);
		const uint64 NoticesFnv = Fnv(NoticeLines), PointsFnv = Fnv(PointLines);
		if (!TestEqual(FString::Printf(TEXT("%s endgame.js sequence digest"), E.Stem), NoticesFnv, E.NoticesFnv))
			AddInfo(FString::Printf(TEXT("%s notices: first %s; last %s"), E.Stem, NoticeLines.Num() ? *NoticeLines[0] : TEXT("-"), NoticeLines.Num() ? *NoticeLines.Last() : TEXT("-")));
		if (!TestEqual(FString::Printf(TEXT("%s objectives.js ring sequence digest"), E.Stem), PointsFnv, E.PointsFnv))
			AddInfo(FString::Printf(TEXT("%s points: last %s"), E.Stem, PointLines.Num() ? *PointLines.Last() : TEXT("-")));
		TestEqual(FString::Printf(TEXT("%s final rate"), E.Stem), St.Objectives.Notices().Rate, FString(E.FinalRate));
		TestEqual(FString::Printf(TEXT("%s final bonus"), E.Stem), St.Objectives.Notices().Bonus, FString(E.FinalBonus));
		TestEqual(FString::Printf(TEXT("%s economy line"), E.Stem), FTCObjectives::EconomyLine(St), FString(E.Economy));

		// The score strip and weather line after the last snapshot (hud.js drawScores, weather-view.js), derived by hand
		// from the last snapshot: vp [0, 0, 20, 24], point owners [-1, 3, 3, 2, -1, -1, -1], ping [null, null, -1, -1].
		const FTCScoreStrip Strip = St.Objectives.ScoreStrip(St);
		TestTrue(FString::Printf(TEXT("%s strip valid"), E.Stem), Strip.bValid);
		TestEqual(FString::Printf(TEXT("%s weather line"), E.Stem), Strip.Weather, FString(TEXT("Clear: no effect")));
		TestEqual(FString::Printf(TEXT("%s weather title"), E.Stem), Strip.WeatherTitle, FString(TEXT("Clear: no effect. Weather for this match. Planes and recon flights fly above it")));
		TestEqual(FString::Printf(TEXT("%s teams"), E.Stem), Strip.Teams.Num(), 4);
		if (Strip.Teams.Num() == 4)
		{
			const TCHAR* const Nets[] = { TEXT(""), TEXT(""), TEXT("AI"), TEXT("AI") };
			for (int32 T = 0; T < 4; T++)
			{
				TestEqual(FString::Printf(TEXT("%s team %d seat"), E.Stem, T), Strip.Teams[T].Seats.Num(), 1);
				if (Strip.Teams[T].Seats.Num() != 1) continue;
				TestEqual(FString::Printf(TEXT("%s seat %d net"), E.Stem, T), Strip.Teams[T].Seats[0].Net, FString(Nets[T]));
				TestEqual(FString::Printf(TEXT("%s seat %d is you"), E.Stem, T), Strip.Teams[T].Seats[0].bYou, T == St.You);
			}
		}
		if (St.ModeRow.Kind == TEXT("conquest"))
		{
			TestFalse(TEXT("conquest has no lead block"), Strip.Lead.bShown);
			const TCHAR* const Held[] = { TEXT("0 held"), TEXT("0 held"), TEXT("1 held"), TEXT("2 held") };
			const TCHAR* const Nums[] = { TEXT("0 / 1200"), TEXT("0 / 1200"), TEXT("20 / 1200"), TEXT("24 / 1200") };
			for (int32 T = 0; T < Strip.Teams.Num() && T < 4; T++)
			{
				if (Strip.Teams[T].Seats.Num()) TestEqual(FString::Printf(TEXT("%s seat %d held (four teams: no pts)"), E.Stem, T), Strip.Teams[T].Seats[0].Held, FString(Held[T]));
				TestEqual(FString::Printf(TEXT("%s team %d VP"), E.Stem, T), Strip.Teams[T].Num, FString(Nums[T]));
				TestEqual(FString::Printf(TEXT("%s team %d unit"), E.Stem, T), Strip.Teams[T].U, FString(TEXT("VP")));
			}
			if (Strip.Teams.Num() == 4) TestEqual(TEXT("team 3 bar"), Strip.Teams[3].Fraction, 24.0 / 1200.0);
			// Capture points (client/main.js startGame labels; markers.js set): point 0 pays only VP (vp 2, mp 0).
			const TArray<FTCCapturePoint>& Pts = St.Objectives.Points();
			if (TestEqual(TEXT("seven capture points"), Pts.Num(), 7))
			{
				TestEqual(TEXT("point 0 label"), Pts[0].Label, FString(TEXT("\u2605 2\u00D7 VP")));
				TestEqual(TEXT("point 1 label"), Pts[1].Label, FString(TEXT("+1.5 MP/s")));
				TestEqual(TEXT("point 0 centre"), Pts[0].Centre, FVector2D(151.0, 151.0));
				TestTrue(TEXT("point 0: a seat 0 arc of 13 segments while contested"), Pts[0].bArcVisible && Pts[0].ArcSlot == 0 && Pts[0].ArcSegments == 13 && Pts[0].bContested && Pts[0].RingSlot == -1);
				TestTrue(TEXT("point 2: held in full by seat 3, no arc"), Pts[2].RingSlot == 3 && Pts[2].FlagSlot == 3 && !Pts[2].bArcVisible);
				TestFalse(TEXT("no needs links on this map"), Pts[0].bHasLink);
			}
		}
		else
		{
			// World: the lead block and the regions per team (p0 and p1 each know one region, their own)
			TestTrue(TEXT("world lead"), Strip.Lead.bShown && Strip.Lead.Mode == TEXT("World Conquest") && Strip.Lead.Clock == TEXT("1/39 regions"));
			TestEqual(TEXT("world lead title"), Strip.Lead.Title, FString(TEXT("Own 39 of the 64 regions, or be the last nation standing. Destroy military bases, then claim with infantry")));
			for (const FTCScoreTeam& T : Strip.Teams)
			{
				const bool bMine = T.Team == St.TeamOf(St.You);
				TestEqual(FString::Printf(TEXT("%s team %d regions"), E.Stem, T.Team), T.Num, FString(bMine ? TEXT("1 / 39") : TEXT("0 / 39")));
				TestEqual(FString::Printf(TEXT("%s team %d label"), E.Stem, T.Team), T.U0, FString(bMine ? TEXT("Owned") : TEXT("Discovered")));
			}
			TestEqual(TEXT("world: no capture points"), St.Objectives.Points().Num(), 0);
			TestTrue(TEXT("world: another team's spawn is not delivered"), St.Spawns.Num() == 4 && St.Spawns[St.You].IsSet() && !St.Spawns[(St.You + 1) % 4].IsSet());
			TestTrue(TEXT("world: fuel and munitions decoded"), St.FuelInc.IsSet() && St.Mun.IsSet() && St.Fuel.IsSet());
		}
		TestFalse(FString::Printf(TEXT("%s: no ending"), E.Stem), St.Objectives.Epilogue().bActive);
		TestEqual(FString::Printf(TEXT("%s: nothing dropped by the caps"), E.Stem), St.DroppedOverCap, 0);
		TestEqual(FString::Printf(TEXT("%s: nothing malformed"), E.Stem), St.SkippedMalformed, 0);
		AddInfo(FString::Printf(TEXT("%s: %d snapshots compared field by field; digests notices 0x%016llx points 0x%016llx; %s"), E.Stem, Snapshots, (unsigned long long)NoticesFnv, (unsigned long long)PointsFnv, *St.DescribeExtras()));

		// Lifecycle: the lobby that ends the match drops every field (server.js finishMatch)
		const TSharedPtr<FJsonObject> Back = Parse(TEXT("{\"t\":\"lobby\",\"state\":\"lobby\",\"you\":0,\"players\":[],\"result\":null}"));
		St.ApplyLobby(*Back);
		TestTrue(FString::Printf(TEXT("%s: cleared by the lobby"), E.Stem), !St.bHasMatch && St.Points.Num() == 0 && St.Vp.Num() == 0 && St.Shots.Num() == 0 && St.Plans.Num() == 0 && !St.Inc.IsSet()
			&& St.Objectives.Points().Num() == 0 && !St.Wx.bKnown && St.MapPoints.Num() == 0 && !St.Objectives.ScoreStrip(St).bValid);
	}
	if (Ran == 0) AddWarning(TEXT("no recorded stream was available: only the build of this test was checked"));
	return true;
}

namespace TCObjectivesTest
{
	// A small authored start (two points, four seats on four teams), as server.js sendStart shapes it.
	FString StartJson(int32 You, const TCHAR* Extra = TEXT(""))
	{
		return FString::Printf(TEXT("{\"t\":\"start\",\"matchId\":1,\"you\":%d,\"map\":{\"name\":\"t\",\"w\":4,\"h\":4,\"rows\":[\"....\",\"....\",\"....\",\"....\"],")
			TEXT("\"points\":[{\"x\":1,\"y\":1,\"kind\":\"radio\",\"mp\":2},{\"x\":2,\"y\":2,\"vp\":2,\"mp\":0,\"needs\":0}]},")
			TEXT("\"spawn\":{\"x\":1,\"z\":1},\"names\":[\"Player 1\",\"Player 2\",\"AI 1\",\"AI 2\"],\"teams\":[0,1,2,3],\"factions\":[0,1,2,0],\"weather\":[\"clear\"]%s}"), You, Extra);
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCObjectivesEndOfMatchTest, "ThreeCrossroads.Objectives.EndOfMatch", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCObjectivesEndOfMatchTest::RunTest(const FString& Parameters)
{
	using namespace TCObjectivesTest;
	// client/epilogue.js on crafted decided snapshots (winner and end as shared/sim.js finish sets them). Expected word,
	// reason line and sounds: the reference module's output (oracle).
	struct FCase { const TCHAR* Label; const TCHAR* Snap; int32 You; const TCHAR* Word; const TCHAR* Why; int32 Sounds; };
	const FCase Cases[] = {
		{ TEXT("vp defeat"), TEXT("\"winner\":3,\"end\":{\"reason\":\"vp\",\"x\":97,\"z\":37}"), 0, TEXT("Defeat"), TEXT("The enemy reached the victory points first."), 2 },
		{ TEXT("vp victory"), TEXT("\"winner\":3,\"end\":{\"reason\":\"vp\",\"x\":97,\"z\":37}"), 3, TEXT("Victory"), TEXT("Your side reached the victory points first."), 2 },
		{ TEXT("draw"), TEXT("\"winner\":-1,\"end\":{\"reason\":\"draw\",\"x\":150,\"z\":150}"), 0, TEXT("Draw"), TEXT("No side is left standing."), 1 },
		{ TEXT("hq victory"), TEXT("\"winner\":1,\"end\":{\"reason\":\"hq\",\"x\":10,\"z\":20},\"mode\":{\"kind\":\"classic\"}"), 1, TEXT("Victory"), TEXT("Your side has the last Production Buildings standing."), 2 },
		{ TEXT("timer defeat"), TEXT("\"winner\":0,\"end\":{\"reason\":\"timer\",\"x\":1,\"z\":2},\"mode\":{\"kind\":\"assault\"}"), 1, TEXT("Defeat"), TEXT("The clock ran out before the defenses fell."), 2 },
		{ TEXT("structures victory"), TEXT("\"winner\":1,\"end\":{\"reason\":\"structures\",\"x\":1,\"z\":2},\"mode\":{\"kind\":\"assault\"}"), 1, TEXT("Victory"), TEXT("Every defending structure has fallen."), 2 },
		{ TEXT("bunkers defeat"), TEXT("\"winner\":2,\"end\":{\"reason\":\"bunkers\",\"x\":1,\"z\":2},\"mode\":{\"kind\":\"annihilation\"}"), 0, TEXT("Defeat"), TEXT("The enemy's bunkers are the last ones standing."), 2 },
		{ TEXT("tutorial victory"), TEXT("\"winner\":0,\"end\":{\"reason\":\"tutorial\",\"x\":1,\"z\":2},\"mode\":{\"kind\":\"tutorial\"}"), 0, TEXT("Victory"), TEXT("The bridge is ours. Training complete: you are ready for a real match."), 2 },
		{ TEXT("world nations defeat"), TEXT("\"winner\":1,\"end\":{\"reason\":\"nations\",\"x\":1,\"z\":2},\"mode\":{\"kind\":\"world\"}"), 0, TEXT("Defeat"), TEXT("Your nation has fallen."), 2 },
		{ TEXT("world victory"), TEXT("\"winner\":1,\"end\":{\"reason\":\"world\",\"x\":1,\"z\":2},\"mode\":{\"kind\":\"world\"}"), 1, TEXT("Victory"), TEXT("Your side controls the continent."), 2 },
		{ TEXT("world draw"), TEXT("\"winner\":-1,\"end\":{\"reason\":\"draw\",\"x\":1,\"z\":2},\"mode\":{\"kind\":\"world\"}"), 1, TEXT("Draw"), TEXT("No side controls the continent."), 1 },
		{ TEXT("horde defeat"), TEXT("\"winner\":1,\"end\":{\"reason\":\"structures\",\"x\":1,\"z\":2},\"mode\":{\"kind\":\"horde\",\"wave\":7}"), 0, TEXT("Defeat"), TEXT("The bunker fell on wave 7."), 2 },
	};
	for (const FCase& C : Cases)
	{
		FTCMatchState St;
		double Clock = 5.0;
		St.Objectives.Clock = [&Clock]() { return Clock; };
		St.ApplyStart(*Parse(StartJson(C.You)));
		St.ApplySnapshot(*Parse(TEXT("{\"t\":\"s\",\"tick\":10,\"winner\":null,\"units\":[],\"points\":[]}")));
		TestFalse(FString::Printf(TEXT("%s: no ending before the winner"), C.Label), St.Objectives.Epilogue().bActive);
		St.ApplySnapshot(*Parse(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":12,%s,\"units\":[]}"), C.Snap)));
		const FTCEpilogue& Ep = St.Objectives.Epilogue();
		TestTrue(FString::Printf(TEXT("%s: ending"), C.Label), Ep.bActive && St.IsDecided());
		TestEqual(FString::Printf(TEXT("%s: word"), C.Label), Ep.Word, FString(C.Word));
		TestEqual(FString::Printf(TEXT("%s: reason line"), C.Label), Ep.Why, FString(C.Why));
		const TArray<FTCSoundCue> Sounds = St.Objectives.TakeSounds();
		TestEqual(FString::Printf(TEXT("%s: sounds"), C.Label), Sounds.Num(), C.Sounds);
		if (Ep.Outcome == ETCOutcome::Victory && Sounds.Num() == 2) TestTrue(TEXT("victory sounds"), Sounds[0].Name == TEXT("alert_point_won") && Sounds[0].Gain == 0.9f && Sounds[1].Name == TEXT("match_start") && Sounds[1].Delay == 0.7f && Sounds[1].Gain == 0.8f);
		if (Ep.Outcome == ETCOutcome::Defeat && Sounds.Num() == 2) TestTrue(TEXT("defeat sounds"), Sounds[0].Name == TEXT("alert_point_lost") && Sounds[0].Rate == 0.85f && Sounds[1].Delay == 0.55f && Sounds[1].Gain == 0.6f && Sounds[1].Rate == 0.7f);
		if (Ep.Outcome == ETCOutcome::Draw && Sounds.Num() == 1) TestTrue(TEXT("draw sound"), Sounds[0].Name == TEXT("alert_point_lost") && Sounds[0].Rate == 1.f);
		// A second decided snapshot does not restart it
		St.ApplySnapshot(*Parse(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":14,%s,\"units\":[]}"), C.Snap)));
		TestEqual(FString::Printf(TEXT("%s: one stamp"), C.Label), St.Objectives.TakeSounds().Num(), 0);
	}

	// The glide and slow-down (epilogue.js frame), oracle values: decided at 10 s, camera at (0, 0) 120 m, end at (97, 37).
	{
		FTCMatchState St;
		double Clock = 10.0;
		St.Objectives.Clock = [&Clock]() { return Clock; };
		St.ApplyStart(*Parse(StartJson(0)));
		St.ApplySnapshot(*Parse(TEXT("{\"t\":\"s\",\"tick\":200,\"winner\":3,\"end\":{\"reason\":\"vp\",\"x\":97,\"z\":37},\"units\":[]}")));
		FTCEpilogueCamera Cam{ 0.0, 0.0, 120.0 };
		struct FStep { double T, X, Z, Dist, Sdt; };
		const FStep Steps[] = { { 10, 0, 0, 120, 0.1 }, { 10.3, 4.91923365890308, 1.8764087152516904, 117.46431254695717, 0.0675 }, { 11.1, 48.5, 18.5, 95, 0.035 }, { 12.2, 97, 37, 70, 0.035 }, { 13, 97, 37, 70, 0.035 } };
		for (const FStep& S : Steps)
		{
			Clock = S.T;
			const double Sdt = St.Objectives.EpilogueFrame(0.1, Cam);
			TestTrue(FString::Printf(TEXT("glide at %.1f s"), S.T), FMath::IsNearlyEqual(Cam.X, S.X, 1e-9) && FMath::IsNearlyEqual(Cam.Z, S.Z, 1e-9) && FMath::IsNearlyEqual(Cam.Dist, S.Dist, 1e-9) && FMath::IsNearlyEqual(Sdt, S.Sdt, 1e-12));
		}
	}

	// The lobby result line and the report (client/main.js renderLobby, client/report.js), oracle values.
	struct FResult { const TCHAR* Label; const TCHAR* Result; int32 You; const TCHAR* Line; bool bShown; const TCHAR* Sub; TArray<FString> Legend; int32 MeRow; };
	auto Story = [](int32 Kills) { return FString::Printf(TEXT("{\"kills\":%d,\"losses\":2,\"built\":0,\"captures\":1,\"mpSpent\":310,\"supportCalls\":0,\"planesDowned\":0}"), Kills); };
	auto St4 = [&Story](int32 A, int32 B, int32 C) { return FString::Printf(TEXT("[%s,%s,null,%s]"), *Story(A), *Story(B), *Story(C)); };
	const FString Conquest = FString::Printf(TEXT("\"winner\":3,\"teams\":[0,1,2,3],\"names\":[\"Player 1\",\"Player 2\",\"AI 1\",\"AI 2\"],\"reason\":\"vp\",\"at\":{\"x\":97,\"z\":37},\"endTick\":1205,\"story\":%s,")
		TEXT("\"timeline\":[{\"t\":0,\"vp\":[0,0,0,0]},{\"t\":10,\"vp\":[0,0,3,5]},{\"t\":20,\"vp\":[0,0,20,24]},{\"t\":60,\"vp\":[0,0,400,1200]}],\"winVp\":1200,\"mode\":\"conquest\""), *St4(3, 1, 9));
	const TArray<FString> ConquestLegend = { TEXT("Player 1: 0 victory points"), TEXT("Player 2: 0 victory points"), TEXT("AI 1: 400 victory points"), TEXT("AI 2: 1200 victory points (won)") };
	const FString Seat0 = FString::Printf(TEXT("{%s,\"you\":{\"outcome\":\"defeat\",\"team\":0}}"), *Conquest);
	const FString Seat3 = FString::Printf(TEXT("{%s,\"you\":{\"outcome\":\"victory\",\"team\":3}}"), *Conquest);
	const FString Watcher = FString::Printf(TEXT("{%s,\"you\":null}"), *Conquest);
	const FString TeamWin = FString::Printf(TEXT("{\"winner\":1,\"teams\":[0,1,1,0],\"names\":[\"Player 1\",\"Player 2\",\"AI 1\",\"AI 2\"],\"reason\":\"hq\",\"at\":{\"x\":97,\"z\":37},\"endTick\":3599,\"story\":%s,")
		TEXT("\"timeline\":[{\"t\":0,\"points\":[0,0],\"structures\":[3,3]},{\"t\":10,\"points\":[1,2],\"structures\":[3,2]}],\"winVp\":null,\"mode\":\"classic\",\"you\":{\"outcome\":\"defeat\",\"team\":0}}"), *St4(3, 1, 9));
	const FString Draw = FString::Printf(TEXT("{\"winner\":-1,\"teams\":[0,1,2,3],\"names\":[\"Player 1\",\"Player 2\",\"AI 1\",\"AI 2\"],\"reason\":\"draw\",\"at\":{\"x\":97,\"z\":37},\"endTick\":40,\"story\":%s,")
		TEXT("\"timeline\":[{\"t\":0,\"vp\":[0,0,0,0]},{\"t\":10,\"vp\":[0,0,3,5]},{\"t\":20,\"vp\":[0,0,20,24]},{\"t\":60,\"vp\":[0,0,400,1200]}],\"winVp\":1200,\"mode\":\"conquest\",\"you\":{\"outcome\":\"draw\",\"team\":0}}"), *St4(3, 1, 9));
	const FString HordeRecord = FString::Printf(TEXT("{\"winner\":1,\"teams\":[0,0,1],\"names\":[\"Ann\",\"Bo\",\"Horde\"],\"reason\":\"structures\",\"endTick\":10900,\"mode\":\"horde\",\"story\":[%s,%s,%s],")
		TEXT("\"timeline\":[{\"t\":0,\"points\":[0,0],\"structures\":[1,0]},{\"t\":540,\"points\":[0,0],\"structures\":[0,0]}],\"winVp\":null,\"horde\":{\"wave\":7,\"kills\":120,\"time\":545,\"names\":[\"Ann\",\"Bo\"],\"record\":true,\"best\":null},")
		TEXT("\"you\":{\"outcome\":\"defeat\",\"team\":0}}"), *Story(40), *Story(80), *Story(0));
	const FString HordeBest = FString::Printf(TEXT("{\"winner\":1,\"teams\":[0,1],\"names\":[\"Ann\",\"Horde\"],\"reason\":\"structures\",\"endTick\":2000,\"mode\":\"horde\",\"story\":[%s,%s],\"timeline\":[{\"t\":0,\"points\":[0,0]}],")
		TEXT("\"winVp\":null,\"horde\":{\"wave\":3,\"kills\":14,\"time\":100,\"names\":[\"Ann\"],\"record\":false,\"best\":{\"wave\":9,\"kills\":300,\"time\":900}},\"you\":null}"), *Story(4), *Story(0));
	const FResult Results[] = {
		{ TEXT("conquest, seat 0"), *Seat0, 0, TEXT("AI 2 wins"), true, TEXT("decided on victory points after 1 min"), ConquestLegend, 0 },
		{ TEXT("conquest, seat 3"), *Seat3, 3, TEXT("Victory"), true, TEXT("decided on victory points after 1 min"), ConquestLegend, 3 },
		{ TEXT("conquest, spectator"), *Watcher, -1, TEXT("AI 2 wins"), true, TEXT("decided on victory points after 1 min"), ConquestLegend, -1 },
		{ TEXT("two-seat team"), *TeamWin, 0, TEXT("Player 2 & AI 1 win"), true, TEXT("decided when the last Production Building fell after 2 min 60 s"), { TEXT("Player 1 & AI 2: 1 points held"), TEXT("Player 2 & AI 1: 2 points held (won)") }, 0 },
		{ TEXT("draw"), *Draw, 0, TEXT("Draw"), true, TEXT("decided in a draw after 2 s"), { TEXT("Player 1: 0 victory points"), TEXT("Player 2: 0 victory points"), TEXT("AI 1: 400 victory points"), TEXT("AI 2: 1200 victory points") }, 0 },
		{ TEXT("host ended"), TEXT("{\"ended\":true}"), 0, TEXT("Match ended by the host"), false, TEXT(""), {}, -1 },
		{ TEXT("horde record"), *HordeRecord, 1, TEXT("Overrun on wave 7: 120 kills in 9:05, a new record"), true, TEXT("overrun on wave 7 after 9 min 5 s"), { TEXT("Ann & Bo: 0 points held"), TEXT("Horde: 0 points held (won)") }, 1 },
		{ TEXT("horde best"), *HordeBest, 0, TEXT("Overrun on wave 3: 14 kills in 1:40. Record: wave 9"), true, TEXT("overrun on wave 3 after 1 min 40 s"), { TEXT("Ann: 0 points held"), TEXT("Horde: 0 points held (won)") }, -1 },
	};
	for (const FResult& R : Results)
	{
		FTCMatchState St;
		double Clock = 1.0;
		St.Objectives.Clock = [&Clock]() { return Clock; };
		St.ApplyStart(*Parse(StartJson(FMath::Max(0, R.You))));
		St.ApplySnapshot(*Parse(TEXT("{\"t\":\"s\",\"tick\":100,\"winner\":3,\"end\":{\"reason\":\"vp\",\"x\":97,\"z\":37},\"units\":[]}")));
		TestTrue(FString::Printf(TEXT("%s: ending plays"), R.Label), St.Objectives.Epilogue().bActive);
		St.ApplyLobby(*Parse(FString::Printf(TEXT("{\"t\":\"lobby\",\"state\":\"lobby\",\"you\":%d,\"spectator\":%s,\"players\":[],\"result\":%s}"), R.You, R.You < 0 ? TEXT("true") : TEXT("false"), R.Result)));
		TestFalse(FString::Printf(TEXT("%s: match cleared"), R.Label), St.bHasMatch);
		TestFalse(FString::Printf(TEXT("%s: the lobby ends the ending"), R.Label), St.Objectives.Epilogue().bActive);
		TestEqual(FString::Printf(TEXT("%s: result line"), R.Label), St.LastResult, FString(R.Line));
		const FTCMatchReport& Rep = St.Objectives.Report();
		TestEqual(FString::Printf(TEXT("%s: report shown"), R.Label), Rep.bShown, R.bShown);
		if (!Rep.bShown) continue;
		TestEqual(FString::Printf(TEXT("%s: report summary"), R.Label), Rep.Sub, FString(R.Sub));
		TArray<FString> Legend;
		for (const FTCReportSide& Side : Rep.Sides) Legend.Add(Side.Legend);
		Legend.Sort(); // report.js lists the legend in team order; Sides here are in drawing order (winners last)
		TArray<FString> Want = R.Legend;
		Want.Sort();
		TestEqual(FString::Printf(TEXT("%s: legend"), R.Label), FString::Join(Legend, TEXT(" | ")), FString::Join(Want, TEXT(" | ")));
		bool bWinner = false;
		for (const FTCReportSide& Side : Rep.Sides) bWinner |= Side.bWon;
		if (bWinner) TestTrue(FString::Printf(TEXT("%s: winners drawn last"), R.Label), Rep.Sides.Last().bWon);
		int32 MeRow = -1;
		for (const FTCReportRow& Row : Rep.Rows) if (Row.bMe) MeRow = Row.Slot;
		TestEqual(FString::Printf(TEXT("%s: own row marked"), R.Label), MeRow, R.MeRow);
	}
	// The rows' values (the conquest case): story [3,1,null,9] kills, a null story reads as zeros
	{
		const FTCMatchReport Rep = FTCObjectives::BuildReport([&]() { FTCMatchState St; St.LobbySeat = 0; St.ApplyLobby(*Parse(FString::Printf(TEXT("{\"t\":\"lobby\",\"state\":\"lobby\",\"you\":0,\"players\":[],\"result\":%s}"), *Seat0))); return St.LastMatch; }(), 0);
		TestTrue(TEXT("report rows"), Rep.Rows.Num() == 4 && Rep.Rows[0].Values[0] == 3 && Rep.Rows[0].Values[4] == 310 && Rep.Rows[2].Values[1] == 0 && Rep.Rows[3].Values[0] == 9);
		TestTrue(TEXT("report chart range"), Rep.Top == 1200 && Rep.End == 60 && Rep.TopLabel == TEXT("1200") && Rep.EndLabel == TEXT("1 min") && Rep.What == TEXT("Victory points"));
		TestTrue(TEXT("report series"), Rep.Sides.Num() == 4 && Rep.Sides.Last().Team == 3 && Rep.Sides.Last().Series.Num() == 4 && Rep.Sides.Last().Series[2] == FVector2D(20, 24));
	}

	// endgame.js notices for Horde, Assault and Classic (teams [0, 1], seat 0), oracle values.
	{
		FTCMatchState St;
		double Clock = 0.0;
		St.Objectives.Clock = [&Clock]() { return Clock; };
		St.ApplyStart(*Parse(TEXT("{\"t\":\"start\",\"matchId\":2,\"you\":0,\"map\":{\"w\":2,\"h\":2,\"rows\":[\"..\",\"..\"]},\"names\":[\"A\",\"B\"],\"teams\":[0,1]}")));
		struct FNote { int32 Tick; const TCHAR* Mode; const TCHAR* Extra; const TCHAR* Rate; const TCHAR* Notice; bool bOut; const TCHAR* Sound; };
		const FNote Notes[] = {
			{ 100, TEXT("{\"kind\":\"horde\",\"active\":false,\"wave\":2,\"left\":0,\"timeLeft\":40}"), TEXT(""), TEXT(""), TEXT(""), false, nullptr },
			{ 120, TEXT("{\"kind\":\"horde\",\"active\":true,\"wave\":3,\"left\":12}"), TEXT(""), TEXT(""), TEXT("Wave 3 incoming"), false, TEXT("match_start") },
			{ 140, TEXT("{\"kind\":\"horde\",\"active\":true,\"wave\":3,\"left\":3}"), TEXT(""), TEXT("The last of the wave are marked through the fog"), TEXT("Wave 3 incoming"), false, nullptr },
			{ 300, TEXT("{\"kind\":\"horde\",\"active\":false,\"wave\":3,\"left\":0,\"timeLeft\":45}"), TEXT(""), TEXT(""), TEXT("Wave 3 cleared. The bunker is patched up"), false, nullptr },
			{ 10, TEXT("{\"kind\":\"assault\",\"timeLeft\":75}"), TEXT(""), TEXT(""), TEXT(""), false, nullptr },
			{ 20, TEXT("{\"kind\":\"assault\",\"timeLeft\":60}"), TEXT(""), TEXT("Assault ends in 1:00"), TEXT(""), false, nullptr },
			{ 30, TEXT("{\"kind\":\"assault\",\"timeLeft\":5}"), TEXT(""), TEXT("Assault ends in 0:05"), TEXT(""), false, nullptr },
			{ 40, TEXT("{\"kind\":\"classic\",\"suddenDeath\":false,\"timeLeft\":9}"), TEXT(""), TEXT(""), TEXT(""), false, nullptr },
			{ 60, TEXT("{\"kind\":\"classic\",\"suddenDeath\":true,\"timeLeft\":0}"), TEXT(",\"out\":[true,false]"), TEXT("Sudden Death: Production Buildings are crumbling"), TEXT("Sudden Death begins. Production has stopped"), true, nullptr },
			{ 80, TEXT("{\"kind\":\"classic\",\"suddenDeath\":true,\"timeLeft\":0}"), TEXT(",\"out\":[true,false],\"winner\":1"), TEXT(""), TEXT(""), false, nullptr },
		};
		for (const FNote& N : Notes)
		{
			Clock = (N.Tick * 50.0) / 1000.0;
			St.ApplySnapshot(*Parse(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":%d,\"mode\":%s,\"units\":[]%s}"), N.Tick, N.Mode, N.Extra)));
			const FTCMatchNotices& Got = St.Objectives.Notices();
			TestEqual(FString::Printf(TEXT("notices tick %d rate"), N.Tick), Got.Rate, FString(N.Rate));
			TestEqual(FString::Printf(TEXT("notices tick %d notice"), N.Tick), Got.Notice, FString(N.Notice));
			TestEqual(FString::Printf(TEXT("notices tick %d out"), N.Tick), Got.bOut, N.bOut);
			const TArray<FTCSoundCue> Sounds = St.Objectives.TakeSounds();
			if (N.Sound) TestTrue(FString::Printf(TEXT("notices tick %d horn"), N.Tick), Sounds.Num() >= 1 && Sounds[0].Name == N.Sound && Sounds[0].Gain == 0.7f);
			else if (!St.IsDecided()) TestEqual(FString::Printf(TEXT("notices tick %d no sound"), N.Tick), Sounds.Num(), 0);
		}
		// endgame.js frame: a notice goes 6 s after it was announced
		Clock = 4.0 + 5.9; St.Objectives.Tick();
		Clock = 4.0 + 6.1; St.Objectives.Tick();
		TestEqual(TEXT("a decided match clears the notice"), St.Objectives.Notices().Notice, FString());
	}

	// JavaScript formatting (String(n) and toFixed), oracle values
	{
		struct FNum { double V; const TCHAR* S; const TCHAR* F1; const TCHAR* F2; };
		const FNum Nums[] = {
			{ 0, TEXT("0"), TEXT("0.0"), TEXT("0.00") }, { 1, TEXT("1"), TEXT("1.0"), TEXT("1.00") }, { -1, TEXT("-1"), TEXT("-1.0"), TEXT("-1.00") },
			{ 1.5, TEXT("1.5"), TEXT("1.5"), TEXT("1.50") }, { 0.1, TEXT("0.1"), TEXT("0.1"), TEXT("0.10") }, { 0.25, TEXT("0.25"), TEXT("0.3"), TEXT("0.25") },
			{ 1.0 / 3.0, TEXT("0.3333333333333333"), TEXT("0.3"), TEXT("0.33") }, { 1200, TEXT("1200"), TEXT("1200.0"), TEXT("1200.00") },
			{ 1e21, TEXT("1e+21"), TEXT("1e+21"), TEXT("1e+21") }, { 1.5e-7, TEXT("1.5e-7"), TEXT("0.0"), TEXT("0.00") },
			{ 123456789.125, TEXT("123456789.125"), TEXT("123456789.1"), TEXT("123456789.13") }, { 26.7 - 26.1, TEXT("0.5999999999999979"), TEXT("0.6"), TEXT("0.60") },
			{ 4.1, TEXT("4.1"), TEXT("4.1"), TEXT("4.10") }, { 0.05, TEXT("0.05"), TEXT("0.1"), TEXT("0.05") },
		};
		for (const FNum& N : Nums)
		{
			TestEqual(FString::Printf(TEXT("String(%s)"), N.S), FTCObjectives::JsNumber(N.V), FString(N.S));
			TestEqual(FString::Printf(TEXT("(%s).toFixed(1)"), N.S), FTCObjectives::JsToFixed(N.V, 1), FString(N.F1));
			TestEqual(FString::Printf(TEXT("(%s).toFixed(2)"), N.S), FTCObjectives::JsToFixed(N.V, 2), FString(N.F2));
		}
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCObjectivesModesTest, "ThreeCrossroads.Objectives.Modes", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCObjectivesModesTest::RunTest(const FString& Parameters)
{
	using namespace TCObjectivesTest;
	// Crafted snapshots in the shapes of shared/sim.js modeRow and snapshotFor; expected strings derived by hand from
	// client/hud.js drawScores (no oracle run for hud.js: it builds DOM throughout).
	struct FModeStrip { FTCScoreStrip S; int32 NumPoints = 0; };
	auto Strip = [](const TCHAR* LobbyMode, int32 You, const FString& Snap, bool bHost = false)
	{
		FTCMatchState St;
		St.Objectives.Clock = []() { return 1.0; };
		St.ApplyLobby(*Parse(FString::Printf(TEXT("{\"t\":\"lobby\",\"state\":\"play\",\"mode\":\"%s\",\"you\":%d,\"amHost\":%s,\"players\":[],\"result\":null}"), LobbyMode, You, bHost ? TEXT("true") : TEXT("false"))));
		St.ApplyStart(*Parse(StartJson(You)));
		St.ApplySnapshot(*Parse(Snap));
		return FModeStrip{ St.Objectives.ScoreStrip(St), St.Objectives.Points().Num() };
	};
	// Assault: team 0 defends two bunkers (one at 1500 hp), seat 1 attacks
	{
		const FModeStrip M = Strip(TEXT("assault"), 1, TEXT("{\"t\":\"s\",\"tick\":5,\"mode\":{\"kind\":\"assault\",\"defenderTeam\":0,\"attackerTeam\":1,\"timeLeft\":125,\"total\":2},")
			TEXT("\"units\":[[5,\"bunker\",0,1,1,0,0,1500,0,0,0,0,0,0,1],[6,\"bunker\",0,2,2,0,0,3000,0,0,0,0,0,0,1]],\"points\":[[0,-1,1,0,0,0]],\"vp\":[0,0,0,0],\"out\":[false,false,false,false]}"));
		TestEqual(TEXT("assault keeps only points that pay manpower"), M.NumPoints, 1);
		TestTrue(TEXT("assault lead"), M.S.Lead.Mode == TEXT("Assault: take the bunker") && M.S.Lead.Clock == TEXT("2:05") && M.S.Lead.Title == TEXT("Destroy the command bunker before the clock runs out"));
		TestTrue(TEXT("assault defender block"), M.S.Teams.Num() == 4 && M.S.Teams[0].Role == TEXT("Defending") && M.S.Teams[0].U0 == TEXT("Structures left") && M.S.Teams[0].Num == TEXT("2 / 2") && FMath::IsNearlyEqual(M.S.Teams[0].Fraction, 0.75));
		TestTrue(TEXT("assault attacker block"), M.S.Teams.Num() == 4 && M.S.Teams[1].Role == TEXT("Attacking") && !M.S.Teams[1].bBar && M.S.Teams[1].Seats[0].Held.IsEmpty());
	}
	// Horde break, host: next wave button and the next profile
	{
		const FModeStrip M = Strip(TEXT("horde"), 0, TEXT("{\"t\":\"s\",\"tick\":5,\"mode\":{\"kind\":\"horde\",\"defenderTeam\":0,\"wave\":4,\"active\":false,\"timeLeft\":42,\"nextProfile\":\"armor\",\"slot\":3},")
			TEXT("\"units\":[[5,\"bunker\",0,1,1,0,0,2250,0,0,0,0,0,0,1]],\"points\":[[0,-1,1,0,0,0]],\"vp\":[0,0,0,0],\"out\":[false,false,false,false]}"), true);
		TestTrue(TEXT("horde lead"), M.S.Lead.Mode == TEXT("Wave 5 in") && M.S.Lead.Clock == TEXT("0:42") && M.S.Lead.Threat == TEXT("Next Wave: Armored assault") && M.S.Lead.bNextWave);
		TestTrue(TEXT("horde bunker"), M.S.Teams.Num() == 4 && M.S.Teams[0].U0 == TEXT("Bunker") && M.S.Teams[0].Num == TEXT("75%") && M.S.Teams[1].Role == TEXT("Horde"));
		TestTrue(TEXT("horde seats"), M.S.Teams.Num() == 4 && M.S.Teams[0].Seats[0].Held == TEXT("1 held") && M.S.Teams[3].Seats[0].Held.IsEmpty());
	}
	// Classic sudden death, HQ out of sight for an enemy, out seat
	{
		const FModeStrip M = Strip(TEXT("classic"), 0, TEXT("{\"t\":\"s\",\"tick\":5,\"mode\":{\"kind\":\"classic\",\"suddenDeath\":true,\"timeLeft\":0},")
			TEXT("\"units\":[[5,\"hq\",0,1,1,0,0,1000,0,0,0,0,0,0,1]],\"points\":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0]],\"vp\":[0,0,0,0],\"out\":[false,false,true,false]}"));
		TestEqual(TEXT("classic keeps every point"), M.NumPoints, 2);
		TestTrue(TEXT("classic lead"), M.S.Lead.Mode == TEXT("Sudden death") && M.S.Lead.Clock.IsEmpty() && M.S.Lead.bModeDanger);
		TestTrue(TEXT("classic own HQ"), M.S.Teams.Num() == 4 && M.S.Teams[0].U0 == TEXT("HQ") && M.S.Teams[0].Num == TEXT("1000 / 3000"));
		TestTrue(TEXT("classic enemy HQ unseen"), M.S.Teams.Num() == 4 && M.S.Teams[1].U0 == TEXT("HQ out of sight") && !M.S.Teams[1].bBar);
		TestTrue(TEXT("classic out seat"), M.S.Teams.Num() == 4 && M.S.Teams[2].U0 == TEXT("Out") && M.S.Teams[2].bU0Danger && M.S.Teams[2].Seats[0].Held == TEXT("Out"));
		TestEqual(TEXT("classic sudden death notice"), M.S.Rate, FString(TEXT("Sudden Death: Production Buildings are crumbling")));
	}
	// Tutorial: the step counter and the goal line
	{
		FTCMatchState St;
		St.Objectives.Clock = []() { return 1.0; };
		St.ApplyStart(*Parse(StartJson(0)));
		St.ApplySnapshot(*Parse(TEXT("{\"t\":\"s\",\"tick\":5,\"mode\":{\"kind\":\"tutorial\",\"step\":2,\"steps\":9,\"goal\":\"Take the bridge\"},\"units\":[]}")));
		const FTCScoreStrip S = St.Objectives.ScoreStrip(St);
		TestTrue(TEXT("tutorial lead"), S.Lead.Mode == TEXT("Tutorial") && S.Lead.Clock == TEXT("3 / 9") && S.Lead.Title == TEXT("Take the bridge"));
		TestEqual(TEXT("tutorial goal line"), FTCObjectives::GoalText(St), FString(TEXT("Objective: Take the bridge")));
		St.ApplySnapshot(*Parse(TEXT("{\"t\":\"s\",\"tick\":6,\"units\":[],\"scenario\":{\"version\":1,\"objectives\":[{\"id\":\"a\",\"state\":\"complete\",\"text\":\"Hold the ridge\"},")
			TEXT("{\"id\":\"b\",\"state\":\"active\",\"text\":{\"en\":\"Take the town\",\"es\":\"Toma el pueblo\"}},{\"id\":\"c\",\"state\":\"hidden\",\"text\":\"Secret\"}]}}")));
		TestEqual(TEXT("scenario goal lines"), FTCObjectives::GoalText(St), FString(TEXT("\u2713 Hold the ridge\n\u2022 Take the town")));
	}
	// Conquest, two players a team: pts shown, offline seat, ping, catch-up suppressed for an offline self
	{
		FTCMatchState St;
		St.Objectives.Clock = []() { return 1.0; };
		St.ApplyStart(*Parse(StartJson(0, TEXT(",\"x\":0")).Replace(TEXT("\"teams\":[0,1,2,3]"), TEXT("\"teams\":[0,1,0,1]"))));
		St.ApplySnapshot(*Parse(TEXT("{\"t\":\"s\",\"tick\":40,\"units\":[],\"points\":[[0,0,1,0,1,0],[1,1,1,0,0,1]],\"vp\":[10,90,5,0],\"online\":[false,true,true,true],\"ping\":[null,48,-1,-1],\"army\":{\"income\":3}}")));
		const FTCScoreStrip S = St.Objectives.ScoreStrip(St);
		TestTrue(TEXT("two teams"), S.Teams.Num() == 2 && S.Teams[0].Seats.Num() == 2);
		if (S.Teams.Num() == 2 && S.Teams[0].Seats.Num() == 2)
		{
			TestEqual(TEXT("pts and held"), S.Teams[0].Seats[0].Held, FString(TEXT("10 pts, 1 held")));
			TestTrue(TEXT("offline seat"), S.Teams[0].Seats[0].Net == TEXT("Offline") && S.Teams[0].Seats[0].bNetDanger && S.Teams[0].Seats[0].NetTitle == TEXT("Offline: the clock is paused until they return"));
			TestEqual(TEXT("ping"), S.Teams[1].Seats[0].Net, FString(TEXT("48 ms")));
			TestEqual(TEXT("team VP (winVp: 1200 * 4 seats / 2 teams)"), S.Teams[1].Num, FString(TEXT("90 / 2400")));
		}
		TestEqual(TEXT("catch-up hidden while this seat is offline"), S.Bonus, FString());
		TestEqual(TEXT("rate"), S.Rate, FString(TEXT("Red leads. Measuring victory rate")));
		const TArray<FTCCapturePoint>& P = St.Objectives.Points();
		TestTrue(TEXT("cut and locked tags, needs link"), P.Num() == 2 && P[0].CutTag() && !P[0].LockTag() && P[1].LockTag() && P[1].MinimapLink() && P[1].LinkTo == FVector2D(3, 3));
		TestEqual(TEXT("radio point label"), P.Num() ? P[0].Label : FString(), FString(TEXT("Radio post \u00B7 +2 MP/s")));
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCObjectivesDecodeBoundsTest, "ThreeCrossroads.Objectives.DecodeBounds", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCObjectivesDecodeBoundsTest::RunTest(const FString& Parameters)
{
	using namespace TCObjectivesTest;
	FTCMatchState St;
	St.Objectives.Clock = []() { return 1.0; };
	St.ApplyStart(*Parse(StartJson(0)));
	// Own unit 7 and an enemy 8 held; rows in the shared/sim.js UNIT_ROW order
	FString Shots;
	for (int32 I = 0; I < TCMatchLimits::Shots + 100; I++) Shots += FString::Printf(TEXT("%s{\"k\":\"hurt\",\"t\":8,\"x\":1,\"z\":1}"), I ? TEXT(",") : TEXT(""));
	const FString Snap = FString::Printf(TEXT("{\"t\":\"s\",\"tick\":3,\"units\":[[7,\"rifle\",0,1,1,0,0,80,0,0,0,0,0,0,1],[8,\"rifle\",1,2,2,0,0,80,0,0,0,0,0,0,1]],\"held\":[2,15],")
		TEXT("\"plans\":[[7,1,3,3,4,4,5,5],[99,1,0,0]],\"orders\":[[7,2,1,3,3,4,6],[8,1,1,0,0]],\"ghosts\":[[8,\"hq\",1,2,2,1],[50,\"barracks\",1,6,6,0.5]],")
		TEXT("\"air\":[[7,2,40,3,0],[77,0,0,0,0]],\"fires\":[0,15,16,-1],\"foundMines\":[3,99],\"shots\":[%s],\"flights\":[{\"flight\":4,\"seq\":0,\"x\":1,\"y\":2,\"z\":3,\"v\":[1,2,3],\"gravity\":25,\"time\":1,\"until\":1.1,\"expires\":2}],")
		TEXT("\"weather\":[\"fog\",\"clear\",8],\"wx\":[0.5,0.2,1,0.5],\"inc\":4.1,\"upkeep\":0.1,\"points\":[[0,-1,1,0,0,0],\"bad\"]}"), *Shots);
	St.ApplySnapshot(*Parse(Snap));
	TestEqual(TEXT("shots capped"), St.Shots.Num(), TCMatchLimits::Shots);
	TestEqual(TEXT("cap counted"), St.DroppedOverCap, 100);
	TestTrue(TEXT("a shot's hidden shooter stays hidden"), St.Shots.Num() && !St.Shots[0].bHasShooter && St.Shots[0].bHasTarget && St.Shots[0].T == 8);
	TestTrue(TEXT("flight without a shooter"), St.Flights.Num() == 1 && !St.Flights[0].bHasShooter && St.Flights[0].V == FVector(1, 2, 3));
	TestTrue(TEXT("plans only for held units"), St.Plans.Num() == 1 && St.Plans[0].Id == 7 && St.Plans[0].Kind == 1 && St.Plans[0].Path.Num() == 2);
	TestTrue(TEXT("orders only for own held units, whole entries"), St.WaitingOrders.Num() == 1 && St.WaitingOrders[0].Orders.Num() == 1 && St.WaitingOrders[0].Orders[0].X == 3);
	TestTrue(TEXT("a ghost under a live row is not drawn"), St.Ghosts.Num() == 1 && St.Ghosts[0].Id == 50 && St.Ghosts[0].Built == 0.5);
	TestTrue(TEXT("planes only when held"), St.Air.Num() == 1 && FTCObjectives::AirStateText(St.Air[0]) == TEXT("On station 40s"));
	TestTrue(TEXT("fires on the map only"), St.Fires.Num() == 2);
	TestTrue(TEXT("found mines on the map only"), St.FoundMines.Num() == 1 && St.FoundMines[0] == 3);
	TestTrue(TEXT("an unreadable point row keeps its place"), St.Points.Num() == 2 && St.Points[1].Owner == -1 && St.SkippedMalformed >= 1);
	FString Line, Title;
	TestTrue(TEXT("weather line"), FTCObjectives::WeatherLine(St, Line, Title));
	TestEqual(TEXT("fog lifting with a shower"), Line, FString(TEXT("Ground fog: sight -30%. Fog lifts in 8 s. Rain: vehicles slow off the road, fords deeper, sight shorter")));
	TestEqual(TEXT("fog title"), Title, FString(TEXT("Ground fog: sight -30%. Fog lifts in 8 s. Rain: vehicles slow off the road, fords deeper, sight shorter. Then Clear: no effect")));
	TestEqual(TEXT("economy"), FTCObjectives::EconomyLine(St), FString(TEXT("+4.1/s (upkeep \u22120.1)")));

	// A spectator's snapshot (server.js watcherSnapshot drops the private fields): weather and wx kept, economy gone
	St.ApplySnapshot(*Parse(TEXT("{\"t\":\"s\",\"tick\":4,\"units\":[],\"held\":[0,0]}")));
	TestTrue(TEXT("private fields absent"), !St.Inc.IsSet() && !St.Upkeep.IsSet() && St.Plans.Num() == 0 && St.WaitingOrders.Num() == 0 && St.Air.Num() == 0 && St.Shots.Num() == 0 && St.Points.Num() == 0);
	TestTrue(TEXT("weather and wx carried"), St.Weather.Now == TEXT("fog") && St.Wx.bKnown && St.Wx.Rain == 0.5);
	TestEqual(TEXT("no economy line"), FTCObjectives::EconomyLine(St), FString());
	TestTrue(TEXT("points keep their last ring state"), St.Objectives.Points().Num() == 2 && St.Objectives.Points()[0].RingSlot == 0);

	// A new start drops everything; ClearMatch too
	St.ApplyStart(*Parse(StartJson(1)));
	TestTrue(TEXT("start resets"), St.Shots.Num() == 0 && St.DroppedOverCap == 0 && St.SkippedMalformed == 0 && !St.Wx.bKnown && !St.bWeatherFromSnapshot && St.Objectives.Points().Num() == 2 && !St.Objectives.Points()[0].bHasRow);
	St.ClearMatch();
	TestTrue(TEXT("clear resets"), St.MapPoints.Num() == 0 && St.Objectives.Points().Num() == 0 && St.Factions.Num() == 0 && St.Spawns.Num() == 0);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCObjectivesModeWatchTest, "ThreeCrossroads.Objectives.ModeWatch", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCObjectivesModeWatchTest::RunTest(const FString& Parameters)
{
	using namespace TCObjectivesTest;
	// The round 16 mode coverage lines. The strip texts are FTCObjectives::ScoreStrip's (tested in Objectives.Modes from
	// client/hud.js); here: when a line is written, what it carries, and that nothing else is.
	auto Grep = [](const TArray<FString>& Lines, const TCHAR* Head) { TArray<FString> Out; for (const FString& L : Lines) if (L.StartsWith(Head)) Out.Add(L); return Out; };
	const FTCLocale::FScopedLanguage English(ETCLanguage::English);
	// Horde, the host's seat 0: a break, the same break a second later, the wave, a minute later, a line, the end
	{
		FTCMatchState St;
		St.Objectives.Clock = []() { return 1.0; };
		St.ApplyLobby(*Parse(TEXT("{\"t\":\"lobby\",\"state\":\"play\",\"mode\":\"horde\",\"you\":0,\"amHost\":true,\"players\":[],\"result\":null}")));
		St.ApplyStart(*Parse(StartJson(0).Replace(TEXT("\"teams\":[0,1,2,3]"), TEXT("\"teams\":[0,0,0,1]"))));
		FTCModeWatch W;
		W.OnStart(St);
		TArray<FString> Log = W.TakeLog();
		TestTrue(TEXT("start line"), Log.Num() == 1 && Log[0].StartsWith(TEXT("modes: start: match 1 on 't' (4x4), seat 0 of 4 (team 0), teams [0,0,0,1], lobby mode horde, 2 authored point(s)")));
		auto Snap = [&](const FString& Json) { const TSharedPtr<FJsonObject> M = Parse(Json); St.ApplySnapshot(*M); W.OnSnapshot(St, nullptr, M.Get()); return W.TakeLog(); };
		const TCHAR* Units = TEXT("\"units\":[[5,\"bunker\",0,1,1,0,0,3000,0,0,0,0,0,0,1]],\"vp\":[0,0,0,0],\"out\":[false,false,false,false]");
		Log = Snap(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":10,\"mode\":{\"kind\":\"horde\",\"defenderTeam\":0,\"wave\":0,\"active\":false,\"timeLeft\":45,\"nextProfile\":\"infantry\",\"slot\":3},%s}"), Units));
		TestEqual(TEXT("break: one strip line"), Grep(Log, TEXT("modes: strip")).Num(), 1);
		TestTrue(TEXT("break line text"), Log.Num() && Log[0] == TEXT("modes: strip at tick 10: Wave 1 in 0:45 - Hold the bunker. The next wave comes 45 seconds after this one is dead [Next Wave: Infantry assault] [Send next wave] | team 0 Bunker 100% (Player 1 (you): 0 held, Player 2: 0 held, AI 1: 0 held) | team 1 Horde (AI 2)"));
		Log = Snap(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":30,\"mode\":{\"kind\":\"horde\",\"defenderTeam\":0,\"wave\":0,\"active\":false,\"timeLeft\":44,\"nextProfile\":\"infantry\",\"slot\":3},%s}"), Units));
		TestEqual(TEXT("the clock alone writes nothing"), Log.Num(), 0);
		Log = Snap(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":50,\"mode\":{\"kind\":\"horde\",\"defenderTeam\":0,\"wave\":1,\"active\":true,\"left\":12,\"slot\":3},%s}"), Units));
		TestTrue(TEXT("the wave writes a line"), Log.Num() == 1 && Log[0].StartsWith(TEXT("modes: strip at tick 50: Wave 1 12 left - Hold the bunker.")));
		Log = Snap(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":1250,\"mode\":{\"kind\":\"horde\",\"defenderTeam\":0,\"wave\":1,\"active\":true,\"left\":3,\"slot\":3},%s,\"shots\":[{\"k\":\"say\",\"text\":\"Hold on\",\"localized\":{\"en\":\"Hold on\",\"es\":\"Aguanta\"}}]}"), Units));
		TestTrue(TEXT("a minute later: the periodic line and the say line"), Log.Num() == 2 && Log[0].StartsWith(TEXT("modes: strip at tick 1250: Wave 1 3 left")) && Log[1] == TEXT("modes: say at tick 1250: Hold on"));
		Log = Snap(TEXT("{\"t\":\"s\",\"tick\":2400,\"mode\":{\"kind\":\"horde\",\"defenderTeam\":0,\"wave\":2,\"active\":true,\"left\":9,\"slot\":3},\"units\":[],\"gone\":[5],\"vp\":[0,0,0,0],\"out\":[false,false,false,false],\"winner\":1,\"end\":{\"reason\":\"structures\",\"x\":2,\"z\":2}}"));
		const TArray<FString> Decided = Grep(Log, TEXT("modes: decided"));
		TestTrue(TEXT("decided line"), Decided.Num() == 1 && Decided[0].StartsWith(TEXT("modes: decided at tick 2400 (2 min 0 s): winner team 1 (lost), reason structures; stamp 'Defeat' '")));
		TestTrue(TEXT("the lost bunker is a new strip line"), Grep(Log, TEXT("modes: strip at tick 2400: Wave 2 9 left")).Num() == 1 && Log[0].Contains(TEXT("team 0 Bunker lost")));
		Log = Snap(TEXT("{\"t\":\"s\",\"tick\":2410,\"mode\":{\"kind\":\"horde\",\"defenderTeam\":0,\"wave\":2,\"active\":true,\"left\":9,\"slot\":3},\"units\":[],\"winner\":1,\"end\":{\"reason\":\"structures\",\"x\":2,\"z\":2}}"));
		TestEqual(TEXT("decided once"), Grep(Log, TEXT("modes: decided")).Num(), 0);
		W.OnClear(TEXT("back to the lobby"));
		W.OnClear(TEXT("again"));
		Log = W.TakeLog();
		TestTrue(TEXT("one summary"), Log.Num() == 1 && Log[0] == TEXT("modes: summary (back to the lobby): mode horde, match 1, 6 snapshot(s), strip lines 4, goal lines 0, say lines 1, decided at tick 2400 (2 min 0 s): winner team 1 (lost), reason structures"));
		TestTrue(TEXT("counters"), W.StripLines() == 4 && W.SayLines() == 1 && W.Decided() && W.Kind() == TEXT("horde"));
	}
	// Conquest and a scenario: no lead block; the VP moving writes nothing, a capture owner change neither (the
	// objectives log has those); the goal box when scenario objectives arrive, change, and go
	{
		FTCMatchState St;
		St.Objectives.Clock = []() { return 1.0; };
		St.ApplyStart(*Parse(StartJson(1)));
		FTCModeWatch W;
		auto Snap = [&](const FString& Json) { const TSharedPtr<FJsonObject> M = Parse(Json); St.ApplySnapshot(*M); W.OnSnapshot(St, nullptr, M.Get()); return W.TakeLog(); };
		TArray<FString> Log = Snap(TEXT("{\"t\":\"s\",\"tick\":3,\"units\":[],\"points\":[[-1,-1,0,0,0,0],[-1,-1,0,0,0,0]],\"vp\":[0,0,0,0]}"));
		// four one-seat teams: hud.js drops the pts from a lone player's line
		TestTrue(TEXT("a snapshot before the start line: the start line first"), Log.Num() == 2 && Log[0].StartsWith(TEXT("modes: start: match 1")) && Log[1].StartsWith(TEXT("modes: strip at tick 3: conquest | team 0 0 / 1200 VP (Player 1: 0 held) | team 1 0 / 1200 VP (Player 2 (you): 0 held) | team 2 0 / 1200 VP (AI 1: 0 held) | team 3")));
		Log = Snap(TEXT("{\"t\":\"s\",\"tick\":40,\"units\":[],\"points\":[[1,1,1,0,0,0],[-1,-1,0,0,0,0]],\"vp\":[0,30,0,0]}"));
		TestEqual(TEXT("VP and owners alone write nothing"), Log.Num(), 0);
		Log = Snap(TEXT("{\"t\":\"s\",\"tick\":41,\"units\":[],\"vp\":[0,30,0,0],\"scenario\":{\"version\":1,\"objectives\":[{\"id\":\"a\",\"state\":\"active\",\"text\":{\"en\":\"Take the town\",\"es\":\"Toma el pueblo\"}}]}}"));
		TestTrue(TEXT("goal line"), Log.Num() == 1 && Log[0] == TEXT("modes: goal at tick 41: • Take the town"));
		Log = Snap(TEXT("{\"t\":\"s\",\"tick\":42,\"units\":[],\"vp\":[0,30,0,0],\"scenario\":{\"version\":1,\"objectives\":[{\"id\":\"a\",\"state\":\"complete\",\"text\":\"Take the town\"}]}}"));
		TestTrue(TEXT("goal change"), Log.Num() == 1 && Log[0] == TEXT("modes: goal at tick 42: ✓ Take the town"));
		Log = Snap(TEXT("{\"t\":\"s\",\"tick\":43,\"units\":[],\"vp\":[0,30,0,0]}"));
		TestTrue(TEXT("goal gone"), Log.Num() == 1 && Log[0] == TEXT("modes: goal at tick 43: (none)"));
		TestEqual(TEXT("goal lines count the shown ones"), W.GoalLines(), 2);
		// a reconnect to the same match keeps counting; a new match summarises the old one first
		W.OnStart(St);
		Log = W.TakeLog();
		TestTrue(TEXT("reconnect"), Log.Num() == 1 && Log[0] == TEXT("modes: start again: match 1 (a reconnect), seat 1"));
		St.ApplyStart(*Parse(StartJson(1).Replace(TEXT("\"matchId\":1"), TEXT("\"matchId\":2"))));
		W.OnStart(St);
		Log = W.TakeLog();
		TestTrue(TEXT("new match: summary then start"), Log.Num() == 2 && Log[0].StartsWith(TEXT("modes: summary (a new match): mode conquest, match 1, 5 snapshot(s), strip lines 1, goal lines 2, say lines 0, decided no")) && Log[1].StartsWith(TEXT("modes: start: match 2")));
	}
	// a spectator that joins a match in play: the start before any lobby, no seat of its own
	{
		FTCMatchState St;
		St.Objectives.Clock = []() { return 1.0; };
		St.ApplyStart(*Parse(StartJson(0)));
		St.bSpectator = true;
		FTCModeWatch W;
		W.OnStart(St);
		const TArray<FString> Log = W.TakeLog();
		TestTrue(TEXT("spectator start line"), Log.Num() == 1 && Log[0].StartsWith(TEXT("modes: start: match 1 on 't' (4x4), watching as a spectator (4 seats), teams [0,1,2,3], lobby mode ")));
		TestTrue(TEXT("no seat claimed"), Log.Num() == 1 && !Log[0].Contains(TEXT("seat 0")));
	}
	// Spanish: the say line as the alert feed shows it (localized.es) and the strip and goal as the HUD draws them (TC_T)
	{
		const FTCLocale::FScopedLanguage Spanish(ETCLanguage::Spanish);
		FTCMatchState St;
		St.Objectives.Clock = []() { return 1.0; };
		St.ApplyLobby(*Parse(TEXT("{\"t\":\"lobby\",\"state\":\"play\",\"mode\":\"tutorial\",\"you\":0,\"amHost\":false,\"players\":[],\"result\":null}")));
		St.ApplyStart(*Parse(StartJson(0).Replace(TEXT("\"teams\":[0,1,2,3]"), TEXT("\"teams\":[0,0,0,1]"))));
		FTCModeWatch W;
		W.OnStart(St);
		W.TakeLog();
		const TSharedPtr<FJsonObject> M = Parse(TEXT("{\"t\":\"s\",\"tick\":5,\"mode\":{\"kind\":\"tutorial\",\"step\":0,\"steps\":9,\"goal\":\"Move your squads up to the hedgerow\"},\"units\":[],")
			TEXT("\"shots\":[{\"k\":\"say\",\"text\":\"El Alamein\",\"localized\":{\"en\":\"El Alamein\",\"es\":\"El Alamein (es)\"}}]}"));
		St.ApplySnapshot(*M);
		W.OnSnapshot(St, nullptr, M.Get());
		const TArray<FString> Log = W.TakeLog();
		const TArray<FString> Strip = Grep(Log, TEXT("modes: strip")), Goal = Grep(Log, TEXT("modes: goal")), Say = Grep(Log, TEXT("modes: say"));
		TestTrue(TEXT("strip keeps English, adds what is shown"), Strip.Num() == 1 && Strip[0].StartsWith(TEXT("modes: strip at tick 5: Tutorial 1 / 9 - Move your squads up to the hedgerow")) && Strip[0].Contains(TEXT(" | shown: Tutorial - ")));
		TestTrue(TEXT("goal shown in Spanish"), Goal.Num() == 1 && Goal[0].StartsWith(TEXT("modes: goal at tick 5: Objective: Move your squads up to the hedgerow | shown: Objetivo: ")));
		TestTrue(TEXT("say line in the reader's language"), Say.Num() == 1 && Say[0] == TEXT("modes: say at tick 5: El Alamein | shown: El Alamein (es)"));
	}
	return true;
}

#endif // WITH_DEV_AUTOMATION_TESTS
