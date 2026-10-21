#include "Client/TCScenarioText.h"

#include "Client/TCLocale.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "CanvasItem.h"
#include "Engine/Canvas.h"
#include "Engine/Font.h"
#include "HAL/PlatformTime.h"

const TCHAR* const FTCScenarioText::ReferencePin = TEXT("e4e579a");

double TCScenarioRules::Roof(const FString& Type)
{
	// objectives.js ROOF
	static const TPair<const TCHAR*, double> Roofs[] = { { TEXT("hq"), 3.6 }, { TEXT("barracks"), 4.7 }, { TEXT("motorpool"), 3.7 }, { TEXT("bunker"), 3.1 },
		{ TEXT("depot"), 2.5 }, { TEXT("airfield"), 3.7 }, { TEXT("flakpos"), 1.6 }, { TEXT("pillbox"), 2.0 }, { TEXT("tower"), 7.4 } };
	for (const TPair<const TCHAR*, double>& R : Roofs) if (Type.Equals(R.Key, ESearchCase::CaseSensitive)) return R.Value;
	return 3.0;
}

FTCTextBoxStyle FTCTextBoxStyle::Goal()
{
	// objectives.css #objective-goal
	return { FLinearColor(FColor(25, 28, 30, 235)), FLinearColor(FColor(0xd6, 0xb2, 0x5e)), FLinearColor(FColor(214, 178, 94, 153)) };
}

FTCTextBoxStyle FTCTextBoxStyle::Banner()
{
	// objectives.css #objective-banner
	return { FLinearColor(FColor(25, 28, 30, 235)), FLinearColor(FColor(0xe2, 0xdf, 0xd3)), FLinearColor(FColor(200, 72, 59, 204)) };
}

double FTCScenarioText::Now() const { return Clock ? Clock() : FPlatformTime::Seconds(); }

void FTCScenarioText::Reset()
{
	// objectives.js reset(): pointState (FTCObjectives keeps it), damage, active, bannerUntil; both boxes hidden
	CurrentGoal = FTCGoalText();
	BannerText.Reset();
	BannerUntil = 0.0;
	Damage.Reset();
	ActiveIds.Reset();
}

FTCGoalText FTCScenarioText::GoalText(const FTCMatchState& S)
{
	// objectives.js snapshot: scenarioGoals = objectives with state 'active' or 'complete'; the text is the Tutorial's
	// goal when mode.goal is set, else those objectives one per line (label: FTCLocale::ObjectiveText in the decoder);
	// the box takes the scenario class whenever there are scenario goals, even under a Tutorial goal
	FTCGoalText Out;
	TArray<FString> Lines;
	for (const FTCScenarioObjective& O : S.Scenario)
		if (O.State == TEXT("active") || O.State == TEXT("complete"))
			Lines.Add(FString::Printf(TEXT("%s %s"), O.State == TEXT("complete") ? TEXT("✓") : TEXT("•"), *O.Text));
	Out.bScenario = Lines.Num() > 0;
	Out.Text = !S.ModeRow.Goal.IsEmpty() ? FString::Printf(TEXT("Objective: %s"), *S.ModeRow.Goal) : FString::Join(Lines, TEXT("\n"));
	return Out;
}

FString FTCScenarioText::CollapseText(const FTCShot& Sh, const FTCMatchState& S, const FTCUnitTable* Table)
{
	// objectives.js collapse(sh): a named piece ('The church has collapsed'), else the building the shot names, else the
	// nearest building within 4 m of the shot (by its newest position, v.tx ?? v.x: the snapshot's), told as yours, an
	// ally's or the enemy's with its UNITS name
	if (!Sh.Name.IsEmpty()) return FString::Printf(TEXT("The %s has collapsed"), *Sh.Name);
	auto Building = [Table](const FTCUnit* U) -> const FTCUnitDef* { const FTCUnitDef* D = U && Table ? Table->Find(U->Type) : nullptr; return D && D->bBuilding ? D : nullptr; };
	const FTCUnit* B = Sh.bHasTarget ? S.Units.Find(Sh.T) : nullptr;
	if (!Building(B)) B = nullptr;
	if (!B && Sh.bHasPos)
	{
		double Distance = TCScenarioRules::CollapseNear;
		for (const TPair<int32, FTCUnit>& P : S.Units)
		{
			if (!Building(&P.Value)) continue;
			const double D = FMath::Sqrt(FMath::Square(P.Value.X - Sh.X) + FMath::Square(P.Value.Z - Sh.Z));
			if (D <= Distance) { B = &P.Value; Distance = D; }
		}
	}
	const FTCUnitDef* D = Building(B);
	if (!D) return FString();
	// main.js hooks: me() and friend(slot) = !foe(slot), foe comparing teams[slot] ?? slot with teams[me] ?? me
	const TCHAR* Who = B->Owner == S.You ? TEXT("Your") : S.TeamOf(B->Owner) == S.TeamOf(S.You) ? TEXT("The allied") : TEXT("The enemy");
	return FString::Printf(TEXT("%s %s has collapsed"), Who, *D->Name);
}

bool FTCScenarioText::ShowBanner(const FString& Text)
{
	const double T = Now();
	if (T < BannerUntil || Text.IsEmpty()) return false;
	BannerText = Text;
	BannerUntil = T + TCScenarioRules::BannerTime;
	return true;
}

void FTCScenarioText::OnSnapshot(const FTCMatchState& S, const FTCUnitTable* Table, const FTCPlumeCamera& Camera)
{
	using namespace TCScenarioRules;
	CurrentGoal = GoalText(S);
	// damaged buildings: received rows only (never ghosts); a building that heals past 66 %, dies or leaves the rows
	// stops smoking on this snapshot
	TSet<int32> Keep;
	if (Table)
		for (const TPair<int32, FTCUnit>& P : S.Units)
		{
			const FTCUnit& U = P.Value;
			const FTCUnitDef* D = Table->Find(U.Type);
			if (!D || !D->bStructure || U.Built < 1.f || U.Hp <= 0) continue;
			const double Fraction = D->HpPer > 0 ? (double)U.Hp / ((double)FMath::Max(1, D->Models) * D->HpPer) : 0.0;
			if (Fraction > TCScenarioRules::Damaged) continue;
			Keep.Add(U.Id);
			FDamage& E = Damage.FindOrAdd(U.Id);
			E.Id = U.Id; E.Type = U.Type; E.X = U.X; E.Z = U.Z;
			E.Radius = FMath::Max(1.5, (double)D->Radius);
			E.bBurning = Fraction <= TCScenarioRules::Burning;
		}
	for (auto It = Damage.CreateIterator(); It; ++It) if (!Keep.Contains(It.Key())) It.RemoveCurrent();
	// rank once per snapshot: within min(260, max(130, 3 x the camera distance)) of the camera and on screen, nearest
	// first, at most 12
	ActiveIds.Reset();
	const double Limit = FMath::Min(260.0, FMath::Max(130.0, Camera.Dist * 3.0));
	for (TPair<int32, FDamage>& P : Damage)
	{
		FDamage& E = P.Value;
		if (Camera.bValid)
		{
			const double D2 = FMath::Square(E.X - Camera.Position.X) + FMath::Square(E.Z - Camera.Position.Y);
			if (D2 > Limit * Limit) continue;
			if (Camera.Project)
			{
				const FVector Ndc = Camera.Project(E.X, Ground(E.X, E.Z) + Roof(E.Type) + 3.0, E.Z);
				if (Ndc.Z < -1.0 || Ndc.Z > 1.0 || FMath::Abs(Ndc.X) > 1.25 || FMath::Abs(Ndc.Y) > 1.3) continue;
			}
			E.Distance = D2;
		}
		ActiveIds.Add(E.Id);
	}
	if (Camera.bValid) ActiveIds.StableSort([this](int32 A, int32 B) { return Damage[A].Distance < Damage[B].Distance; });
	if (ActiveIds.Num() > MaxPlumes) ActiveIds.SetNum(MaxPlumes);
	// collapse banners
	for (const FTCShot& Sh : S.Shots)
		if (Sh.K == TEXT("collapse") && Now() >= BannerUntil) ShowBanner(CollapseText(Sh, S, Table));
}

TArray<FTCPlumeBurst> FTCScenarioText::Frame(double Dt)
{
	using namespace TCScenarioRules;
	TArray<FTCPlumeBurst> Out;
	for (const int32 Id : ActiveIds)
	{
		FDamage* E = Damage.Find(Id);
		if (!E) continue;
		const double Roof = Ground(E->X, E->Z) + TCScenarioRules::Roof(E->Type);
		// High graphics: smoke every 0.28 s (dark and a metre up once burning), flames every 0.16 s while burning
		E->Smoke -= Dt; E->Fire -= Dt;
		if (E->Smoke <= 0.0) { E->Smoke = SmokeEvery; Out.Add({ E->bBurning ? TEXT("dark") : TEXT("smoke"), E->X, Roof + (E->bBurning ? 1.0 : 0.0), E->Z, E->Radius, E->Id }); }
		if (E->bBurning && E->Fire <= 0.0) { E->Fire = FireEvery; Out.Add({ TEXT("fire"), E->X, Roof, E->Z, E->Radius, E->Id }); }
	}
	if (!BannerText.IsEmpty() && Now() >= BannerUntil) BannerText.Reset();
	return Out;
}

TArray<FString> FTCScenarioText::Wrap(const FString& Paragraph, float MaxWidth, FMeasure Measure)
{
	TArray<FString> Words, Out;
	Paragraph.ParseIntoArray(Words, TEXT(" "), true);
	FString Line;
	for (const FString& W : Words)
	{
		const FString Try = Line.IsEmpty() ? W : Line + TEXT(" ") + W;
		if (!Line.IsEmpty() && Measure(Try).X > MaxWidth) { Out.Add(Line); Line = W; }
		else Line = Try;
	}
	if (!Line.IsEmpty() || Out.Num() == 0) Out.Add(Line);
	return Out;
}

namespace TCScenarioTextImpl
{
	// objectives.css: a fixed box centred on the view, shrink-to-fit up to its max-width (border-box: padding and the
	// 1 px border count), its text wrapped inside
	FTCTextBox Layout(const TArray<FString>& Paragraphs, float MaxBox, float PadX, float PadY, bool bLeft, float MaxHeight, float ViewW, float Top, FTCScenarioText::FMeasure Measure)
	{
		FTCTextBox B;
		B.bShown = true;
		B.bLeft = bLeft;
		const float ContentMax = FMath::Max(1.f, MaxBox - 2.f * PadX - 2.f);
		float Natural = 0.f, LineH = 0.f;
		for (const FString& P : Paragraphs) { const FVector2D Sz = Measure(P); Natural = FMath::Max(Natural, Sz.X); LineH = FMath::Max(LineH, Sz.Y); }
		if (LineH <= 0.f) LineH = Measure(TEXT("Ag")).Y;
		B.LineHeight = FMath::RoundToFloat(LineH * 1.15f);
		for (const FString& P : Paragraphs) B.Lines.Append(Natural > ContentMax ? FTCScenarioText::Wrap(P, ContentMax, Measure) : TArray<FString>{ P });
		float Content = FMath::Min(Natural, ContentMax);
		for (const FString& L : B.Lines) Content = FMath::Max(Content, Measure(L).X); // a lone word wider than the box
		// .scenario-goals: max-height 28vh (the browser scrolls; here the list is cut at a whole line with an ellipsis)
		if (MaxHeight > 0.f)
		{
			const int32 Fit = FMath::Max(1, FMath::FloorToInt((MaxHeight - 2.f * PadY - 2.f) / B.LineHeight));
			if (B.Lines.Num() > Fit) { B.Lines.SetNum(Fit); B.Lines.Last() = TEXT("…"); B.bCut = true; }
		}
		B.Size = FVector2D(Content + 2.f * PadX + 2.f, B.Lines.Num() * B.LineHeight + 2.f * PadY + 2.f);
		B.Min = FVector2D(FMath::RoundToFloat((ViewW - B.Size.X) / 2.f), Top);
		B.FirstLineY = Top + 1.f + PadY;
		for (const FString& L : B.Lines) B.LineX.Add(bLeft ? B.Min.X + 1.f + PadX : B.Min.X + (B.Size.X - Measure(L).X) / 2.f);
		return B;
	}
}

FTCTextBox FTCScenarioText::LayoutGoal(const FTCGoalText& Goal, float ViewW, float ViewH, float Top, FMeasure Measure)
{
	using namespace TCScenarioRules;
	if (!Goal.IsShown()) return FTCTextBox();
	// the page's observer translates the element's whole text (i18n.js node()), then the box lays it out; pre-line
	// keeps the scenario list's line breaks
	const FString Shown = FTCLocale::T(Goal.Text);
	TArray<FString> Paragraphs;
	if (Goal.bScenario) Shown.ParseIntoArray(Paragraphs, TEXT("\n"), false);
	else Paragraphs.Add(Shown.Replace(TEXT("\n"), TEXT(" ")));
	const float MaxBox = FMath::Max(GoalMin, FMath::Min(BoxMax, ViewW - ViewReserve));
	return TCScenarioTextImpl::Layout(Paragraphs, MaxBox, GoalPadX, GoalPadY, Goal.bScenario, Goal.bScenario ? ScenarioMaxShare * ViewH : 0.f, ViewW, Top, Measure);
}

FTCTextBox FTCScenarioText::LayoutBanner(const FString& Text, float ViewW, float Top, FMeasure Measure)
{
	using namespace TCScenarioRules;
	if (Text.IsEmpty()) return FTCTextBox();
	const float MaxBox = FMath::Max(0.f, FMath::Min(BoxMax, ViewW - ViewReserve));
	return TCScenarioTextImpl::Layout({ FTCLocale::T(Text) }, MaxBox, BannerPadX, BannerPadY, false, 0.f, ViewW, Top, Measure);
}

float FTCScenarioText::Place(float StripBottom, float ViewW, float ViewH, FMeasure GoalMeasure, FMeasure BannerMeasure, FTCTextBox& OutGoal, FTCTextBox& OutBanner) const
{
	using namespace TCScenarioRules;
	// placeBanner: top = max(12, #top's bottom + 6); the goal there; the banner under the goal (6 px) or at top
	const float Top = FMath::Max(MinTop, StripBottom + Gap);
	OutGoal = LayoutGoal(CurrentGoal, ViewW, ViewH, Top, GoalMeasure);
	OutBanner = LayoutBanner(BannerText, ViewW, Top + (OutGoal.bShown ? OutGoal.Size.Y + Gap : 0.f), BannerMeasure);
	return OutBanner.bShown ? OutBanner.Bottom() : OutGoal.bShown ? OutGoal.Bottom() : StripBottom;
}

void FTCScenarioText::DrawBox(UCanvas* Canvas, UFont* Font, const FTCTextBox& Box, const FTCTextBoxStyle& Style)
{
	if (!Canvas || !Box.bShown) return;
	FCanvasTileItem Plate(Box.Min, Box.Size, Style.Plate);
	Plate.BlendMode = SE_BLEND_Translucent;
	Canvas->DrawItem(Plate);
	const FVector2D Edges[4][2] = { { Box.Min, FVector2D(Box.Size.X, 1.f) }, { Box.Min + FVector2D(0.f, Box.Size.Y - 1.f), FVector2D(Box.Size.X, 1.f) },
		{ Box.Min, FVector2D(1.f, Box.Size.Y) }, { Box.Min + FVector2D(Box.Size.X - 1.f, 0.f), FVector2D(1.f, Box.Size.Y) } };
	for (const auto& E : Edges)
	{
		FCanvasTileItem Edge(E[0], E[1], Style.Border);
		Edge.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(Edge);
	}
	for (int32 I = 0; I < Box.Lines.Num(); I++)
	{
		FCanvasTextItem T(FVector2D(Box.LineX[I], Box.FirstLineY + I * Box.LineHeight), FText::FromString(Box.Lines[I]), Font, Style.Text);
		T.EnableShadow(FLinearColor(0.f, 0.f, 0.f, 0.6f));
		Canvas->DrawItem(T);
	}
}

float FTCScenarioText::Draw(UCanvas* Canvas, UFont* GoalFont, UFont* BannerFont, float StripBottom) const
{
	if (!Canvas) return StripBottom;
	auto MeasureWith = [Canvas](UFont* F) { return [Canvas, F](const FString& L) { float W = 0, H = 0; Canvas->StrLen(F, L, W, H); return FVector2D(W, H); }; };
	const auto GoalM = MeasureWith(GoalFont);
	const auto BannerM = MeasureWith(BannerFont);
	FTCTextBox G, B;
	const float Bottom = Place(StripBottom, Canvas->ClipX, Canvas->ClipY, GoalM, BannerM, G, B);
	DrawBox(Canvas, GoalFont, G, FTCTextBoxStyle::Goal());
	DrawBox(Canvas, BannerFont, B, FTCTextBoxStyle::Banner());
	return Bottom;
}

// ---------------------------------------------------------------------------------------------------------------------
// Tests. The oracle: the pinned reference's own client/objectives.js (goal box text and class, collapse(), the banner's
// 4 s, the damaged-building emitters and their burst cadence) and client/i18n.js spanish(), run under node with stub
// hooks by unreal/Tools/tutorial/objectives-oracle.mjs; the values below are its output.

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace TCScenarioTextTest
{
	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> O;
		const TSharedRef<TJsonReader<>> R = TJsonReaderFactory<>::Create(Text);
		FJsonSerializer::Deserialize(R, O);
		return O;
	}
	// a two-team start on a 128 x 128 flat map, seat 0 on team 0 with seat 2, seat 1 the enemy
	const TCHAR* Start = TEXT(R"({"t":"start","matchId":7,"you":0,"names":["Me","Foe","Ally"],"teams":[0,1,0],"factions":[0,1,2],"map":{"name":"Test","w":64,"h":64,"rows":[],"points":[]},"spawn":{"x":10,"z":10}})");
	bool StartState(FTCMatchState& S)
	{
		FString Rows;
		for (int32 I = 0; I < 64; I++) Rows += FString::Printf(TEXT("%s\"%s\""), I ? TEXT(",") : TEXT(""), *FString::ChrN(64, '.'));
		const TSharedPtr<FJsonObject> M = Parse(FString(Start).Replace(TEXT("\"rows\":[]"), *FString::Printf(TEXT("\"rows\":[%s]"), *Rows)));
		return M.IsValid() && S.ApplyStart(*M);
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCScenarioTextGoalTest, "ThreeCrossroads.ScenarioText.Goal", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCScenarioTextGoalTest::RunTest(const FString& Parameters)
{
	using namespace TCScenarioTextTest;
	FTCLocale::FScopedLanguage English(ETCLanguage::English);
	FTCMatchState S;
	if (!TestTrue(TEXT("start"), StartState(S))) return false;
	struct FCase { const TCHAR* Snapshot; const TCHAR* Text; bool bScenario; };
	// objectives-oracle.json goalModule (the module's own #objective-goal text and its scenario-goals class)
	const FCase Cases[] = {
		{ TEXT(R"({"t":"s","tick":1,"units":[],"mode":{"kind":"tutorial","step":2,"steps":9,"goal":"Take the crossroads: keep a squad by the flag"}})"), TEXT("Objective: Take the crossroads: keep a squad by the flag"), false },
		{ TEXT(R"({"t":"s","tick":2,"units":[],"scenario":{"objectives":[{"id":"a","state":"active","text":"Hold the ridge"},{"id":"b","state":"complete","text":{"en":"Take the village","es":"Toma el pueblo"}},{"id":"c","state":"hidden","text":"Secret"},{"id":"d","state":"failed","text":"Lost cause"},{"id":"e","state":"active","text":{"es":"Solo espanol"}}]}})"), TEXT("• Hold the ridge\n✓ Take the village\n• "), true },
		{ TEXT(R"({"t":"s","tick":3,"units":[],"mode":{"kind":"tutorial","goal":"Cross the bridge and take the bridgehead"},"scenario":{"objectives":[{"id":"a","state":"active","text":"Both"}]}})"), TEXT("Objective: Cross the bridge and take the bridgehead"), true },
		{ TEXT(R"({"t":"s","tick":4,"units":[],"scenario":{"objectives":[]}})"), TEXT(""), false },
		{ TEXT(R"({"t":"s","tick":5,"units":[]})"), TEXT(""), false },
	};
	for (const FCase& C : Cases)
	{
		const TSharedPtr<FJsonObject> M = Parse(C.Snapshot);
		if (!TestTrue(TEXT("snapshot parses"), M.IsValid())) continue;
		S.ApplySnapshot(*M);
		const FTCGoalText G = FTCScenarioText::GoalText(S);
		TestEqualSensitive(FString::Printf(TEXT("tick %d text"), S.Tick), G.Text, FString(C.Text));
		TestEqual(FString::Printf(TEXT("tick %d scenario class"), S.Tick), G.bScenario, C.bScenario);
		TestEqualSensitive(FString::Printf(TEXT("tick %d FTCObjectives::GoalText agrees"), S.Tick), FTCObjectives::GoalText(S), G.Text);
	}
	// i18n.js spanish() of the Tutorial's goal lines (objectives-oracle.json spanish), as the page's observer shows them;
	// the sixth step's goal keeps the reference's own partial translation
	FTCLocale::FScopedLanguage Spanish(ETCLanguage::Spanish);
	if (!FTCLocale::IsLoaded()) { AddWarning(FString::Printf(TEXT("locale-es.json not loaded (%s): the Spanish checks were NOT run"), *FTCLocale::LoadError())); return true; }
	const TCHAR* const Es[][2] = {
		{ TEXT("Objective: Move your squads up to the hedgerow"), TEXT("Objetivo: Lleva tus pelotones hasta el seto") },
		{ TEXT("Objective: Clear the enemy sentries out of the orchard"), TEXT("Objetivo: Echa a los centinelas enemigos del huerto") },
		{ TEXT("Objective: Take the crossroads: keep a squad by the flag"), TEXT("Objetivo: Toma el cruce: deja un pelotón junto a la bandera") },
		{ TEXT("Objective: Recruit an AT gun: Tab, W, E, or click its card"), TEXT("Objetivo: Recluta un cañón antitanque: Tab, W, E, o haz clic en su carta") },
		{ TEXT("Objective: Knock out the tank with your AT gun"), TEXT("Objetivo: Destruye el tanque con tu cañón antitanque") },
		{ TEXT("Objective: Clear the farm trench. Rifles: F throws a grenade"), TEXT("Objetivo: Clear the farm trench. Fusileros: F throws a grenade") },
		{ TEXT("Objective: Send a squad home to refill: select it and press R"), TEXT("Objetivo: Manda un pelotón a la base a reponerse: selecciónalo y pulsa R") },
		{ TEXT("Objective: Shell the stone house: press C, click it, aim, click"), TEXT("Objetivo: Bombardea la casa de piedra: pulsa C, haz clic en ella, apunta, clic") },
		{ TEXT("Objective: Cross the bridge and take the bridgehead"), TEXT("Objetivo: Cruza el puente y toma la cabeza de puente") },
		{ TEXT("The church has collapsed"), TEXT("Se ha derrumbado: iglesia") },
		{ TEXT("Your Barracks has collapsed"), TEXT("Se ha derrumbado: Cuartel (tuyo)") },
		{ TEXT("The enemy Supply Depot has collapsed"), TEXT("Se ha derrumbado: enemy Supply Depot") },
		{ TEXT("The allied HQ has collapsed"), TEXT("Se ha derrumbado: CG de allied") },
	};
	for (const auto& P : Es) TestEqualSensitive(FString::Printf(TEXT("Spanish: %s"), P[0]), FTCLocale::T(P[0]), FString(P[1]));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCScenarioTextBannerTest, "ThreeCrossroads.ScenarioText.Banner", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCScenarioTextBannerTest::RunTest(const FString& Parameters)
{
	using namespace TCScenarioTextTest;
	FTCLocale::FScopedLanguage English(ETCLanguage::English);
	FTCUnitTable Table;
	FString Error;
	if (!TestTrue(FString::Printf(TEXT("unit table loads (%s)"), *Error), Table.Load(Error))) return false;
	FTCMatchState S;
	if (!TestTrue(TEXT("start"), StartState(S))) return false;
	// the units the oracle's main.js map held: [id, type, owner, x, z, rot, aim, hp, supp, target, cover, cd, flags, stars, built]
	const TSharedPtr<FJsonObject> M = Parse(TEXT(R"({"t":"s","tick":10,"units":[[10,"barracks",0,50,50,0,0,1500,0,0,0,0,0,0,1],[11,"depot",1,80,80,0,0,600,0,0,0,0,0,0,1],[12,"hq",2,120,40,0,0,3000,0,0,0,0,0,0,1],[13,"rifle",1,81,81,0,0,100,0,0,0,0,0,0,1],[14,"bunker",1,200,200,0,0,3000,0,0,0,0,0,0,1]]})"));
	if (!TestTrue(TEXT("snapshot"), M.IsValid())) return false;
	S.ApplySnapshot(*M);
	auto Shot = [](const TCHAR* Name, bool bT, int32 T, double X, double Z) { FTCShot Sh; Sh.K = TEXT("collapse"); Sh.Name = Name; Sh.bHasTarget = bT; Sh.T = T; Sh.bHasPos = true; Sh.X = X; Sh.Z = Z; return Sh; };
	// objectives-oracle.json collapse
	struct FCase { FTCShot Sh; const TCHAR* Text; };
	const FCase Cases[] = {
		{ Shot(TEXT("church"), false, 0, 1, 1), TEXT("The church has collapsed") },
		{ Shot(TEXT(""), true, 10, 0, 0), TEXT("Your Barracks has collapsed") },
		{ Shot(TEXT(""), true, 13, 82, 81), TEXT("The enemy Supply Depot has collapsed") },  // a rifle squad is no building: the depot 2.2 m off
		{ Shot(TEXT(""), false, 0, 122, 41), TEXT("The allied HQ has collapsed") },
		{ Shot(TEXT(""), false, 0, 300, 300), TEXT("") },
		{ Shot(TEXT(""), true, 14, 200, 200), TEXT("") },                                    // a bunker is a structure, not a building
		{ Shot(TEXT(""), false, 0, 83.9, 80), TEXT("The enemy Supply Depot has collapsed") }, // 3.9 m: within 4
	};
	for (const FCase& C : Cases) TestEqualSensitive(FString::Printf(TEXT("collapse at (%.1f, %.1f)"), C.Sh.X, C.Sh.Z), FTCScenarioText::CollapseText(C.Sh, S, &Table), FString(C.Text));
	// bannerTiming: a second collapse 2 s later keeps the first banner; still up at 3.9 s, gone at 4.1 s
	double T = 100.0;
	FTCScenarioText X;
	X.Clock = [&T]() { return T; };
	TestTrue(TEXT("first banner"), X.ShowBanner(TEXT("The mill has collapsed")));
	T += 2.0;
	TestFalse(TEXT("a second banner waits"), X.ShowBanner(TEXT("The barn has collapsed")));
	TestEqualSensitive(TEXT("the first stays"), X.Banner(), FString(TEXT("The mill has collapsed")));
	T += 1.9; X.Frame(0.0);
	TestTrue(TEXT("shown at 3.9 s"), X.BannerShown());
	T += 0.2; X.Frame(0.0);
	TestFalse(TEXT("hidden at 4.1 s"), X.BannerShown());
	// the shots path: a collapse shot in a snapshot raises it
	FTCMatchState S2;
	StartState(S2);
	const TSharedPtr<FJsonObject> M2 = Parse(TEXT(R"({"t":"s","tick":11,"units":[[10,"barracks",0,50,50,0,0,1500,0,0,0,0,0,0,1]],"shots":[{"k":"collapse","t":10,"x":50,"z":50}]})"));
	S2.ApplySnapshot(*M2);
	X.OnSnapshot(S2, &Table, FTCPlumeCamera());
	TestEqualSensitive(TEXT("banner from a delivered shot"), X.Banner(), FString(TEXT("Your Barracks has collapsed")));
	// layout: under the strip (max(12, bottom + 6)), the banner 6 px under the goal; widths per objectives.css
	auto Measure = [](const FString& L) { return FVector2D(8.f * L.Len(), 16.f); };
	FTCGoalText G; G.Text = TEXT("Objective: Move your squads up to the hedgerow");
	const FTCTextBox Box = FTCScenarioText::LayoutGoal(G, 1280.f, 720.f, 100.f, Measure);
	TestEqual(TEXT("goal fits on one line"), Box.Lines.Num(), 1);
	TestEqual(TEXT("goal width: text + 2 x 14 px padding + 2 px border"), (float)Box.Size.X, 8.f * 46 + 30.f);
	TestEqual(TEXT("goal centred"), (float)Box.Min.X, FMath::RoundToFloat((1280.f - (float)Box.Size.X) / 2.f));
	const FTCTextBox Narrow = FTCScenarioText::LayoutGoal(G, 800.f, 720.f, 100.f, Measure);
	TestEqual(TEXT("at 800 px the goal box is 280 px wide (max(280, min(620, 800 - 660)))"), (float)Narrow.Size.X, 280.f);
	TestTrue(TEXT("and wraps"), Narrow.Lines.Num() > 1);
	FTCGoalText L; L.bScenario = true;
	for (int32 I = 0; I < 30; I++) L.Text += FString::Printf(TEXT("%s• Objective %d"), I ? TEXT("\n") : TEXT(""), I);
	const FTCTextBox List = FTCScenarioText::LayoutGoal(L, 1280.f, 720.f, 100.f, Measure);
	TestTrue(TEXT("a long scenario list is cut within 28% of the view"), List.bCut && List.Size.Y <= 0.28f * 720.f);
	TestTrue(TEXT("scenario list is left-aligned"), List.bLeft && List.LineX[0] == List.Min.X + 15.f);
	FTCScenarioText P;
	P.Clock = [&T]() { return T; };
	FTCMatchState S3; StartState(S3);
	S3.ApplySnapshot(*Parse(TEXT(R"({"t":"s","tick":12,"units":[],"mode":{"kind":"tutorial","step":0,"steps":9,"goal":"Move your squads up to the hedgerow"}})")));
	P.OnSnapshot(S3, &Table, FTCPlumeCamera());
	T += 10.0; P.ShowBanner(TEXT("The mill has collapsed"));
	FTCTextBox Gb, Bb;
	P.Place(0.f, 1280.f, 720.f, Measure, Measure, Gb, Bb);
	TestEqual(TEXT("goal at max(12, 0 + 6) = 12"), (float)Gb.Min.Y, 12.f);
	TestEqual(TEXT("banner 6 px under the goal"), (float)Bb.Min.Y, Gb.Bottom() + 6.f);
	P.Place(94.f, 1280.f, 720.f, Measure, Measure, Gb, Bb);
	TestEqual(TEXT("goal 6 px under the strip"), (float)Gb.Min.Y, 100.f);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCScenarioTextPlumesTest, "ThreeCrossroads.ScenarioText.Plumes", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCScenarioTextPlumesTest::RunTest(const FString& Parameters)
{
	using namespace TCScenarioTextTest;
	FTCUnitTable Table;
	FString Error;
	if (!TestTrue(FString::Printf(TEXT("unit table loads (%s)"), *Error), Table.Load(Error))) return false;
	FTCMatchState S;
	if (!TestTrue(TEXT("start"), StartState(S))) return false;
	FTCScenarioText X;
	double T = 100.0;
	X.Clock = [&T]() { return T; };
	// objectives-oracle.json: hq at 66 % smokes, at 66.03 % does not; a depot and a tower at 33 % burn; a half-built
	// barracks, a rifle squad and a dead pillbox do not
	S.ApplySnapshot(*Parse(TEXT(R"({"t":"s","tick":20,"units":[[1,"hq",0,10,10,0,0,3000,0,0,0,0,0,0,1],[2,"hq",0,20,10,0,0,1980,0,0,0,0,0,0,1],[3,"hq",0,30,10,0,0,1981,0,0,0,0,0,0,1],[4,"depot",1,40,10,0,0,198,0,0,0,0,0,0,1],[5,"barracks",0,50,10,0,0,400,0,0,0,0,0,0,0.5],[6,"rifle",0,60,10,0,0,10,0,0,0,0,0,0,1],[7,"pillbox",1,70,10,0,0,0,0,0,0,0,0,0,1],[8,"tower",0,80,10,0,0,99,0,0,0,0,0,0,1]]})")));
	X.OnSnapshot(S, &Table, FTCPlumeCamera());
	TArray<int32> Ids = X.Active();
	Ids.Sort();
	TestTrue(TEXT("emitters 2, 4, 8"), Ids == TArray<int32>({ 2, 4, 8 }));
	TestTrue(TEXT("depot and tower burn, the hq smokes"), X.Damaged()[4].bBurning && X.Damaged()[8].bBurning && !X.Damaged()[2].bBurning);
	// frames 0.1, 0.1, 0.1, 0.05, 0.2, 0.3 (the oracle's bursts: kind, x, y, z, radius)
	auto Key = [](const TArray<FTCPlumeBurst>& B) { TArray<FString> K; for (const FTCPlumeBurst& P : B) K.Add(FString::Printf(TEXT("%s %g %g %g %g"), *P.Kind, P.X, P.Y, P.Z, P.Radius)); K.Sort(); return FString::Join(K, TEXT("; ")); };
	const FString All = TEXT("dark 40 3.5 10 2; dark 80 8.4 10 1.5; fire 40 2.5 10 2; fire 80 7.4 10 1.5; smoke 20 3.6 10 3");
	const FString Fires = TEXT("fire 40 2.5 10 2; fire 80 7.4 10 1.5");
	const double Dts[] = { 0.1, 0.1, 0.1, 0.05, 0.2, 0.3 };
	const FString Want[] = { All, TEXT(""), Fires, TEXT(""), All, All };
	for (int32 I = 0; I < 6; I++) TestEqualSensitive(FString::Printf(TEXT("frame %d"), I + 1), Key(X.Frame(Dts[I])), Want[I]);
	// the depot heals past 66 %: it stops; the tower burns on with its timers
	S.ApplySnapshot(*Parse(TEXT(R"({"t":"s","tick":24,"all":true,"units":[[4,"depot",1,40,10,0,0,500,0,0,0,0,0,0,1],[8,"tower",0,80,10,0,0,99,0,0,0,0,0,0,1]]})")));
	X.OnSnapshot(S, &Table, FTCPlumeCamera());
	TestTrue(TEXT("only the tower"), X.Active() == TArray<int32>({ 8 }));
	TestEqualSensitive(TEXT("frame after"), Key(X.Frame(0.3)), FString(TEXT("dark 80 8.4 10 1.5; fire 80 7.4 10 1.5")));
	// ranking with a camera: far (over max(130, 3 x dist)) and off-screen buildings are left out, nearest first, 12 at most
	FString Rows;
	for (int32 I = 0; I < 20; I++) Rows += FString::Printf(TEXT("%s[%d,\"depot\",1,%d,10,0,0,100,0,0,0,0,0,0,1]"), I ? TEXT(",") : TEXT(""), 100 + I, 10 + I * 5);
	Rows += TEXT(",[200,\"depot\",1,10,500,0,0,100,0,0,0,0,0,0,1],[201,\"depot\",1,-40,10,0,0,100,0,0,0,0,0,0,1]");
	S.ApplySnapshot(*Parse(FString::Printf(TEXT(R"({"t":"s","tick":28,"all":true,"units":[%s]})"), *Rows)));
	FTCPlumeCamera Cam;
	Cam.bValid = true; Cam.Position = FVector2D(0.0, 10.0); Cam.Dist = 40.0;
	Cam.Project = [](double PX, double PY, double PZ) { return FVector(PX < -20.0 ? -2.0 : PX / 200.0, 0.0, 0.5); }; // x < -20 m is off screen
	X.OnSnapshot(S, &Table, Cam);
	TestEqual(TEXT("12 at most"), X.Active().Num(), 12);
	TestEqual(TEXT("nearest first"), X.Active()[0], 100);
	TestFalse(TEXT("490 m away: out"), X.Active().Contains(200));
	TestFalse(TEXT("off screen: out"), X.Active().Contains(201));
	return true;
}
#endif
