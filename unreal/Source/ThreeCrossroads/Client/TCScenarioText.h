// Scenario and Tutorial guidance on the screen (parity gap 11: CLI-121, CLI-125, CLI-126), ported from the pinned
// reference client (reference/ww2-rts at TCScenarioText::ReferencePin):
//   client/objectives.js   init (the #objective-goal and #objective-banner boxes), snapshot (the goal text: "Objective:
//                          <mode.goal>" in the Tutorial, else the scenario's active and complete objectives, one per
//                          line with a tick or a bullet, each label text[lang] ?? text.en; the collapse banners; the
//                          damaged buildings that smoke), collapse (the banner's sentence, 4 s, never over a showing
//                          one), placeBanner (the goal under the score strip, the banner under the goal) and frame (the
//                          smoke and fire cadence of each damaged building, the 12 nearest on screen);
//   client/objectives.css  the two boxes: dark plate, brass text and border for the goal (16 px, semi-bold), chalk text
//                          and a red border for the banner (14 px); centred, at most max(280, min(620, vw - 660)) px wide
//                          (the banner min(620, vw - 660)); the scenario list left-aligned, a line per objective, at
//                          most 28 % of the view high.
// Everything comes from this recipient's own snapshot (FTCMatchState: mode.goal, scenario.objectives, shots, unit rows);
// nothing is inferred for what the server did not send. The label language is the decoder's (FTCLocale::ObjectiveText,
// CLI-121); the whole goal text is then drawn through TC_T as the page's observer translates the element's text, so the
// Tutorial's "Objective: ..." reads "Objetivo: ..." in Spanish with the reference's own es.js entries.
//
// Native: the boxes are laid out and drawn on the HUD canvas (no DOM): the text is wrapped at word breaks to the box's
// width with the HUD's fonts (the engine's medium font for the goal, the small one for the banner); a scenario list that
// would pass 28 % of the view is cut at the last whole line with an ellipsis line (the browser scrolls it). The smoke is
// the High graphics cadence only (this client has no Graphics Low, CLI-122).
#pragma once

#include "CoreMinimal.h"

class FTCMatchState;
class FTCUnitTable;
struct FTCShot;
class UCanvas;
class UFont;

namespace TCScenarioRules
{
	constexpr double BannerTime = 4.0;   // objectives.js BANNER_TIME
	constexpr int32 MaxPlumes = 12;      // MAX_PLUMES
	constexpr double SmokeEvery = 0.28, FireEvery = 0.16; // frame(): a smoke burst, a flame burst (High)
	constexpr double Damaged = 0.66, Burning = 0.33;      // snapshot(): smokes at or under 66 % health, burns at 33 %
	constexpr double CollapseNear = 4.0; // collapse(): the nearest building within 4 m of a nameless collapse
	// objectives.css
	constexpr float GoalPadX = 14.f, GoalPadY = 6.f, BannerPadX = 12.f, BannerPadY = 5.f, Gap = 6.f, MinTop = 12.f;
	constexpr float GoalMin = 280.f, BoxMax = 620.f, ViewReserve = 660.f, ScenarioMaxShare = 0.28f;
	// ROOF: metres from the ground to a building's roof, where its smoke starts (default 3)
	double Roof(const FString& Type);
}

// What objectives.js puts in #objective-goal.
struct FTCGoalText
{
	FString Text;                      // English for the Tutorial; scenario labels already in the reader's language
	bool bScenario = false;            // .scenario-goals: left-aligned, one line per objective, height capped
	bool IsShown() const { return !Text.IsEmpty(); }
};

// A box laid out for the canvas (objectives.css): its rectangle and the lines inside it, already translated.
struct FTCTextBox
{
	bool bShown = false;
	FVector2D Min = FVector2D::ZeroVector, Size = FVector2D::ZeroVector;
	TArray<FString> Lines;
	TArray<float> LineX;               // each line's left edge
	float FirstLineY = 0.f, LineHeight = 0.f;
	bool bLeft = false, bCut = false;  // bCut: a scenario list taller than 28 % of the view, cut with an ellipsis line
	float Bottom() const { return Min.Y + Size.Y; }
};

// The colours of a box (objectives.css).
struct FTCTextBoxStyle
{
	FLinearColor Plate, Text, Border;
	static FTCTextBoxStyle Goal();     // rgba(25, 28, 30, 0.92), #d6b25e, rgba(214, 178, 94, 0.6)
	static FTCTextBoxStyle Banner();   // rgba(25, 28, 30, 0.92), #e2dfd3, rgba(200, 72, 59, 0.8)
};

// One burst objectives.js frame() asks fx.js plume() for: 'smoke', 'dark' or 'fire' at (x, y, z) reference metres.
struct FTCPlumeBurst
{
	FString Kind;
	double X = 0.0, Y = 0.0, Z = 0.0, Radius = 3.0;
	int32 Id = 0;
};

// What a camera tells the plume ranking (objectives.js snapshot: camera.position, cam.dist, Vector3.project).
struct FTCPlumeCamera
{
	bool bValid = false;
	FVector2D Position = FVector2D::ZeroVector;    // the camera's x and z in reference metres
	double Dist = 85.0;                            // cam.dist
	// reference metres (x, y up, z) to normalised device coordinates (x, y in -1..1 on screen, z in -1..1 in depth)
	TFunction<FVector(double X, double Y, double Z)> Project;
};

class FTCScenarioText
{
public:
	static const TCHAR* const ReferencePin;
	// The client clock in seconds (tests drive it; the game uses FPlatformTime::Seconds).
	TFunction<double()> Clock;
	double Now() const;
	// Ground height at reference metres (x, z) for the roofs (main.js hAt); null: 0.
	TFunction<double(double X, double Z)> GroundHeight;

	// ----- fed by the game mode -----
	void Reset();                                                  // objectives.js reset(): a new match or none
	void OnSnapshot(const FTCMatchState& S, const FTCUnitTable* Table, const FTCPlumeCamera& Camera);
	// objectives.js frame(dt): expires the banner; returns the bursts due this frame.
	TArray<FTCPlumeBurst> Frame(double Dt);

	// ----- the goal (CLI-125, CLI-126) -----
	// objectives.js snapshot: "Objective: <mode.goal>", else the scenario's active and complete objectives.
	static FTCGoalText GoalText(const FTCMatchState& S);
	const FTCGoalText& Goal() const { return CurrentGoal; }

	// ----- the collapse banner -----
	// objectives.js collapse(sh): a 'collapse' shot's sentence, or nothing. Me and Friend as main.js hooks.
	static FString CollapseText(const FTCShot& Sh, const FTCMatchState& S, const FTCUnitTable* Table);
	bool BannerShown() const { return !BannerText.IsEmpty(); }
	const FString& Banner() const { return BannerText; }
	// Shows a banner as collapse() would (no new one while one shows). True when it took.
	bool ShowBanner(const FString& Text);

	// ----- damaged buildings (the emitters objectives.js keeps) -----
	struct FDamage
	{
		int32 Id = 0;
		FString Type;
		double X = 0.0, Z = 0.0, Radius = 1.5, Distance = 0.0;
		bool bBurning = false;
		double Smoke = 0.0, Fire = 0.0;
	};
	const TMap<int32, FDamage>& Damaged() const { return Damage; }
	const TArray<int32>& Active() const { return ActiveIds; }      // the ranked emitters, nearest first, at most 12

	// ----- layout (objectives.css and placeBanner) -----
	// Measure: a line's size in pixels with the box's font. Text is translated (TC_T) before it is measured.
	using FMeasure = TFunctionRef<FVector2D(const FString& Line)>;
	static FTCTextBox LayoutGoal(const FTCGoalText& Goal, float ViewW, float ViewH, float Top, FMeasure Measure);
	static FTCTextBox LayoutBanner(const FString& Text, float ViewW, float Top, FMeasure Measure);
	// Wraps one paragraph at spaces to MaxWidth (a word longer than that stands alone on its line).
	static TArray<FString> Wrap(const FString& Paragraph, float MaxWidth, FMeasure Measure);
	// placeBanner: the goal at max(12, strip bottom + 6), the banner under it (6 px apart). Returns the lowest edge.
	float Place(float StripBottom, float ViewW, float ViewH, FMeasure GoalMeasure, FMeasure BannerMeasure, FTCTextBox& OutGoal, FTCTextBox& OutBanner) const;
	// Draws a laid-out box on the canvas: plate, 1 px border, the lines.
	static void DrawBox(UCanvas* Canvas, UFont* Font, const FTCTextBox& Box, const FTCTextBoxStyle& Style);
	// The HUD's whole job: lay out and draw the goal and the banner under the strip. Returns the lowest edge drawn.
	float Draw(UCanvas* Canvas, UFont* GoalFont, UFont* BannerFont, float StripBottom) const;

private:
	FTCGoalText CurrentGoal;
	FString BannerText;
	double BannerUntil = 0.0;
	TMap<int32, FDamage> Damage;
	TArray<int32> ActiveIds;
	double Ground(double X, double Z) const { return GroundHeight ? GroundHeight(X, Z) : 0.0; }
};
