#include "Client/TCBossBar.h"
#include "Client/TCLocale.h"
#include "Net/TCMatchState.h"

void FTCBossBar::Reset()
{
	bShown = false; bEverSet = false; Name.Reset(); Target = From = 100.0; Started = -1e9;
}

double FTCBossBar::WidthAt(double Now) const
{
	const double K = FMath::Clamp((Now - Started) / TCBossRules::Transition, 0.0, 1.0);
	return From + (Target - From) * TCCss::Ease((float)K);
}

void FTCBossBar::Snapshot(const FTCMatchState& S, double Now)
{
	// hud.js drawBoss: const b = s.mode?.boss; show(el, !!b); if (!b) return
	const bool bWas = bShown;
	bShown = S.bHasMatch && S.ModeRow.bPresent && S.ModeRow.bBoss;
	if (!bShown) return;
	const double Hp = S.ModeRow.BossHp, Max = S.ModeRow.BossMax;
	const double Pct = Max != 0 ? Hp / Max * 100.0 : 0.0;
	// setText(el.firstChild, `KAIJU  ${Math.ceil(b[0] / b[1] * 100)}%`)
	Name = FString::Printf(TEXT("KAIJU  %s%%"), *FTCObjectives::JsNumber(FMath::CeilToDouble(Pct)));
	// el.lastChild.firstChild.style.width = `${b[0] / b[1] * 100}%`: a change while shown eases over 0.3 s; the first
	// value (from the CSS auto width) and one set as the bar reappears (from display: none) apply at once
	if (Pct == Target && bEverSet && bWas) return;
	if (!bWas || !bEverSet) { From = Target = Pct; Started = -1e9; }
	else { From = WidthAt(Now); Target = Pct; Started = Now; }
	bEverSet = true;
}

FTCPaint FTCBossBar::Layout(float CentreX, float Top, float ViewW, double Now, const FTCMeasure& Measure, float* OutBottom) const
{
	using namespace TCBossRules;
	FTCPaint P;
	if (!bShown) { if (OutBottom) *OutBottom = Top; return P; }
	const float W = FMath::Min(MaxWidth, ViewW - SideRoom), X = CentreX - W / 2;
	const float NameH = NamePx * NameLine, H = 1 + PadTop + NameH + BarGap + BarH + 2 + PadBottom + 1;
	P.Rect(X, Top, W, H, TCCss::PanelSolid, TEXT("boss:panel"));
	P.Frame(X, Top, W, H, TCCss::Line, 1.f, TEXT("boss:frame"));
	// the name line: translated as a text node, its whitespace collapsed as HTML draws it, letter-spaced 0.2 em
	FString Shown = TC_T(Name);
	while (Shown.ReplaceInline(TEXT("  "), TEXT(" ")) > 0) {}
	const float Track = Tracking * NamePx;
	FTCPaintItem& T = P.Text(Shown, CentreX, Top + 1 + PadTop, NamePx, TCCss::Hex(0xe8c060), true, ETCPaintAlign::Centre, TEXT("boss:name"));
	T.Tracking = Track;
	T.Size.Y = NameH;
	// text-align: center places the line box, letter-spacing included after the last letter: half a spacing to the left
	T.Pos.X -= Track / 2;
	const float BarX = X + 1 + PadSide, BarW = W - 2 - 2 * PadSide, BarY = Top + 1 + PadTop + NameH + BarGap;
	P.Rect(BarX, BarY, BarW, BarH + 2, TCCss::Hex(0x000000), TEXT("boss:bar-border"));
	P.Rect(BarX + 1, BarY + 1, BarW - 2, BarH, TCCss::Hex(0x2a1010), TEXT("boss:bar"));
	const float Fill = (float)FMath::Clamp(WidthAt(Now), 0.0, 100.0) / 100.f * (BarW - 2);
	if (Fill > 0) P.Gradient(BarX + 1, BarY + 1, Fill, BarH, TCCss::Hex(0xe04a3a), TCCss::Hex(0x8e1c14), TEXT("boss:fill"));
	P.Panels.Add(FBox2D(FVector2D(X, Top), FVector2D(X + W, Top + H)));
	if (OutBottom) *OutBottom = Top + H;
	return P;
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Dom/JsonObject.h"

namespace TCBossTest
{
	TSharedPtr<FJsonObject> Parse(const FString& S)
	{
		TSharedPtr<FJsonObject> O;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(S), O);
		return O;
	}
	const TCHAR* Start = TEXT(R"({"t":"start","matchId":7,"you":0,"spawn":{"x":20,"z":20},"spawns":[{"x":20,"z":20},{"x":180,"z":180}],"names":["Ann","Horde"],"teams":[0,1],"factions":[0,1],"map":{"name":"synthetic","w":4,"h":4,"rows":["....","....","....","...."],"points":[]}})");
	// shared/sim.js modeRow in Horde: kind, profile, wave, left, active, boss, slot
	FString Snap(int32 Tick, const FString& Boss, const TCHAR* Kind = TEXT("horde"))
	{
		return FString::Printf(TEXT("{\"t\":\"s\",\"tick\":%d,\"mp\":100,\"units\":[],\"held\":[0,0],\"gone\":[],\"points\":[],\"shots\":[],\"mode\":{\"kind\":\"%s\",\"wave\":10,\"left\":14,\"active\":true,\"boss\":%s,\"slot\":1}}"), Tick, Kind, *Boss);
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCBossBarTest, "ThreeCrossroads.BossBar.Snapshots", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCBossBarTest::RunTest(const FString&)
{
	using namespace TCBossTest;
	FTCLocale::FScopedLanguage En(ETCLanguage::English);
	FTCMatchState St;
	TestTrue(TEXT("start"), St.ApplyStart(*Parse(Start)));
	FTCBossBar B;
	const FTCMeasure M = FTCPaint::ApproxMeasure();
	St.ApplySnapshot(*Parse(Snap(20, TEXT("null"))));
	B.Snapshot(St, 1.0);
	TestFalse(TEXT("no boss: hidden"), B.Shown());
	TestEqual(TEXT("hidden: nothing drawn"), B.Layout(640, 60, 1280, 1.0, M).Items.Num(), 0);
	// wave 10: one Kaiju, 6000 hp
	St.ApplySnapshot(*Parse(Snap(40, TEXT("[6000,6000]"))));
	B.Snapshot(St, 2.0);
	TestTrue(TEXT("boss: shown"), B.Shown());
	TestEqual(TEXT("full"), B.Text(), FString(TEXT("KAIJU  100%")));
	TestEqual(TEXT("the first width applies at once"), B.WidthAt(2.0), 100.0);
	// Math.ceil(4321 / 6000 * 100) = 73; the width eases to 72.0166... over 0.3 s
	St.ApplySnapshot(*Parse(Snap(60, TEXT("[4321,6000]"))));
	B.Snapshot(St, 3.0);
	TestEqual(TEXT("ceil of the percentage"), B.Text(), FString(TEXT("KAIJU  73%")));
	TestTrue(TEXT("target"), FMath::IsNearlyEqual(B.Percent(), 4321.0 / 6000.0 * 100.0, 1e-9));
	TestEqual(TEXT("eases from where it was"), B.WidthAt(3.0), 100.0);
	TestTrue(TEXT("mid transition"), B.WidthAt(3.15) < 100.0 && B.WidthAt(3.15) > B.Percent());
	TestTrue(TEXT("done after 0.3 s"), FMath::IsNearlyEqual(B.WidthAt(3.3), B.Percent(), 1e-6));
	// a later boss wave: two Kaiju, the sum of their hp over the sum of their hpPer
	St.ApplySnapshot(*Parse(Snap(80, TEXT("[11999,12000]"))));
	B.Snapshot(St, 4.0);
	TestEqual(TEXT("two bosses"), B.Text(), FString(TEXT("KAIJU  100%")));
	St.ApplySnapshot(*Parse(Snap(100, TEXT("[1,12000]"))));
	B.Snapshot(St, 5.0);
	TestEqual(TEXT("ceil keeps 1%"), B.Text(), FString(TEXT("KAIJU  1%")));
	// the layout: centred, min(640, vw - 32) wide, the name with its spaces collapsed, the fill on the bar
	float Bottom = 0;
	const FTCPaint P = B.Layout(640, 60, 1280, 5.5, M, &Bottom);
	const FTCPaintItem* Panel = P.Find(TEXT("boss:panel"));
	TestTrue(TEXT("640 wide, centred"), Panel && Panel->Pos.X == 320.f && Panel->Size.X == 640.f && Panel->Pos.Y == 60.f);
	TestEqual(TEXT("name as drawn"), P.TextsOf(TEXT("boss:name")), TArray<FString>{ TEXT("KAIJU 1%") });
	const FTCPaintItem* Fill = P.Find(TEXT("boss:fill"));
	TestTrue(TEXT("fill width"), Fill && FMath::IsNearlyEqual(Fill->Size.X, (640.f - 2 - 24 - 2) * (float)(1.0 / 12000.0), 0.01f));
	TestTrue(TEXT("height: 1 + 6 + 20.25 + 4 + 20 + 8 + 1"), FMath::IsNearlyEqual(Bottom - 60.f, 60.25f, 0.01f));
	const FTCPaint Narrow = B.Layout(300, 60, 600, 5.5, M);
	TestTrue(TEXT("narrow window: vw - 32"), Narrow.Find(TEXT("boss:panel")) && Narrow.Find(TEXT("boss:panel"))->Size.X == 568.f);
	// the last Kaiju dies: mode.boss null, hidden; it comes back with its width set at once
	St.ApplySnapshot(*Parse(Snap(120, TEXT("null"))));
	B.Snapshot(St, 6.0);
	TestFalse(TEXT("dead: hidden"), B.Shown());
	St.ApplySnapshot(*Parse(Snap(140, TEXT("[3000,6000]"))));
	B.Snapshot(St, 7.0);
	TestTrue(TEXT("back: no transition from display none"), B.Shown() && B.WidthAt(7.0) == 50.0);
	// another mode never shows it (the server sets boss in Horde only, and the client reads only the row it got)
	FTCMatchState C;
	C.ApplyStart(*Parse(Start));
	C.ApplySnapshot(*Parse(FString::Printf(TEXT("{\"t\":\"s\",\"tick\":20,\"mp\":100,\"units\":[],\"held\":[0,0],\"gone\":[],\"points\":[],\"shots\":[]}"))));
	FTCBossBar B2;
	B2.Snapshot(C, 1.0);
	TestFalse(TEXT("Conquest (no mode row): hidden"), B2.Shown());
	{
		FTCLocale::FScopedLanguage Es(ETCLanguage::Spanish);
		TestEqual(TEXT("Spanish: es.js has no key, the line stays"), B.Layout(640, 60, 1280, 7.0, M).TextsOf(TEXT("boss:name")), TArray<FString>{ TEXT("KAIJU 50%") });
	}
	return true;
}
#endif
