// The HUD, drawn on the canvas: connection and room status, the lobby (seats, mode, map and, for the host, Mode, Add AI
// and Start buttons), the selection's unit information, the enemy under the pointer, the control groups, Stop,
// Attack-move and (host) End match buttons, the recruit card (client/hud.js: group tabs and unit cards outside Classic;
// a selected Production Building's cards and five queue places in Classic and World), the minimap (bottom right), the
// last order's note or refusal, the drag box and the order marker. Slice 1d adds the Orders buttons (Retreat, the three
// stances with their Off/On/n of N state, the Build menu, Resume deliveries, the supply overlay and truck selection
// switches), the Engineers' Build card in Classic and World, the logistics card for the selection, the Rally button on
// the recruit card, the armed aim's hint, the building placement preview under the pointer, rally markers, the
// logistics overlay (known stores and the selected trucks' fully known routes, in the view and on the minimap) and
// supply alerts. Slice 1e adds the ability buttons (a right click switches autocast), Mount up and Unload, the field
// works in the Build menu and the Trench menu, a site's Cancel on its card or facility line, a squad's dig status and a
// carrier's riders, the aimed ability's ring, the dig and entrenchment previews and the ghosts of planned works. Slice 1f
// adds the support bar (client/hud.js buildSupport and drawEcon), the Shell area button, the Form menu with its shapes,
// Tighten, Spread, March together and Snap to trenches, the cover preview switch, the support and area aim preview, the
// public strike warnings (in the view and, for strips, on the minimap), the facing gesture's formation preview and the
// cover preview's shields and chevrons (outlines only; the canvas has no depth test). Slice 1g adds the lobby's settings
// as cycling buttons (FTCLobbySettings: left click next, right click previous, greyed with the reason when locked), the
// browser's Start text and the line under it, the saves panel with the host's Load buttons (FTCPersistence), and the
// host's Pause match, Save match and End match with the Orders. Slice 9 adds, from FTCObjectives and the decoded snapshot
// fields: the score strip per mode at the top centre with its rate, catch-up and notice line, the weather line, the goal
// line and the out banner (client/hud.js drawScores, endgame.js, weather-view.js, objectives.js); the capture points in
// the view (ring, flag, progress arc, contested and flip rings, label, cut and locked tags; client/markers.js
// capturePoint) and on the minimap (owner discs, the cut cross, the locked link, the start's spawns); the epilogue's
// stamp (client/epilogue.js); the lobby's result line and match report (client/main.js renderLobby, client/report.js);
// the selected units' plan lines, waiting orders and target lines (client/markers.js planLayer) and the health and
// suppression bars (client/main.js frame); the alert feed and the Alt+H history (client/alerts.js, alert-history.js);
// the F1 Controls sheet, the follow chip, and the Take cover, Edge scroll and Pan speed buttons. Slice 10 draws every text
// through TC_T (client/i18n.js: English or Spanish), adds the Language picker (lobby and settings) and the Volume button,
// and the relayed pings' rings on the minimap (client/pings.js); the combat effects and the world rings are ATCEffects'.
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/HUD.h"
#include "Client/TCPlayerController.h"
#include "Client/TCMinimap.h"
#include "Client/TCEndReport.h"
#include "Client/TCMatchControls.h"
#include "TCHUD.generated.h"

class UFont;
class ATCTerrain;

// Round 16 (hudfit): the HUD's fit rules at a large UI scale in a small window, pure so tests read them.
namespace FTCHudFit
{
	THREECROSSROADS_API FTCPaint NoticeCard(const FString& Shown, float ViewW, float Top, FLinearColor Colour, const FTCMeasure& Measure, float Scale = 1.f, bool bEliminated = false);
	THREECROSSROADS_API FTCPaint MatchNoticeCard(const FString& Rate, const FString& Bonus, const FString& Notice, float ViewW, float Top, const FTCMeasure& Measure, float Scale = 1.f);
	// index.html scores flex layout, in CSS pixels, shared by drawing and geometry tests.
	THREECROSSROADS_API FTCPaint ScoreCard(const FTCScoreStrip& Strip, float ViewW, const FTCMeasure& Measure, bool bNextWave, float Scale = 1.f);
	// receiveStart: the still-empty score card and group tabs, absent for spectators and Classic/World.
	THREECROSSROADS_API FVector2D InitialCameraBand(float ScreenH, float UiScale, bool bBuyShown);
	// overlay.js dash is [period, lit length], measured backwards from a queued leg's goal, in metres.
	THREECROSSROADS_API TArray<FVector2D> QueueDashSpans(double Length, double Period = 2.2, double Lit = 1.3);
	// overlay.js FRAG_BODY: the two smoothstep ends, with fwidth(distance) in metres per screen pixel.
	THREECROSSROADS_API double QueueDashAlpha(double FromGoal, double PixelWidth, double Period = 2.2, double Lit = 1.3);
	// Screen projections of one metre along and across the ribbon, including perspective and terrain slope.
	THREECROSSROADS_API double QueueDashPixelWidth(FVector2D Along, FVector2D Across);
	// client/index.html #sandbox: left 50%, translateX(-50%), bottom 230px (zoomed with #hud)
	constexpr float SandboxBottom = 230.f;
	// The strip's widest: centred, its left edge right of the corner's Menu panel (CornerRight) by 4 px
	THREECROSSROADS_API float StripMaxWidth(float ClipX, float CornerRight);
	// Where the sandbox panel goes (UI pixels): #sandbox's own place when it is under the strip and clear of every box
	// in Avoid (the match panel, the recruit card); else centred under the strip (StripLow). Round 17: at either place
	// a box in SlideFrom (the alert history, which hugs the right edge) does not push it away: the panel slides left
	// until it is 4 px clear of it (never past 8 px from the left edge).
	THREECROSSROADS_API FBox2D SandboxPlace(float ClipX, float ClipY, FVector2D Size, float StripLow, const TArray<FBox2D>& Avoid, bool* bOutBrowserSpot = nullptr, const TArray<FBox2D>& SlideFrom = TArray<FBox2D>());
	// The match panel's levels: 0 as built; 1 the support bar shows what the browser's buttons show (the key and the
	// cost, no name); 2 the formation summary line goes and the logistics rows stop at 2; 3 at 1, without the heading.
	// Round 17: 4, the browser's own fit (index.html #lowerleft { max-height } over #selection .list { min-height: 0;
	// overflow: auto }): the panel is held under the limit, its text rows cut from the end where the list would scroll
	// (a count '... +n' in their place, no words, as the logistics rows'), the buttons kept whole, and the hover line
	// becomes a tooltip over the panel (the browser's title) instead of a reserved row.
	// The first level whose top (ClipY - Gap - height) is at or under TopLimit; the last when none is.
	THREECROSSROADS_API int32 FitLevel(const TArray<float>& Heights, float ClipY, float BottomGap, float TopLimit);
	constexpr int32 MaxLevel = 4;
	// Level 4's text rows: how many of Lines text rows show in Room px of rows, the last of them the count of the rest
	// (RowPx a row); 0 when not even the count fits. Lines when they all fit.
	THREECROSSROADS_API int32 TextRowsShown(int32 Lines, float Room, float RowPx);
	// Round 17: main.js renderLobby's Horde record sentence after #modeInfo (leading space included), from the lobby's
	// hordeBest (FTCMatchState::LobbyHordeBest with the lead patch; empty without it)
	THREECROSSROADS_API FString HordeRecordText(int32 Defenders, bool bBest, int32 Wave, int32 Time, const TArray<FString>& Names);
	THREECROSSROADS_API FString HordeRecordText(const class FTCMatchState& S);
	// Round 17: main.js's #connectionBanner text for connection.js's 'retry' (English; drawn through TC_T: es.js has both)
	THREECROSSROADS_API FString ConnectionBannerText(bool bFullRoom, int32 SecondsLeft);
	// The narrowest width from Start down to Floor (in Step px steps, Floor itself last) at which Fits holds, trying
	// narrower only while the previous width fitted; Start when nothing narrower fits.
	THREECROSSROADS_API float NarrowerPanel(float Start, float Floor, float Step, TFunctionRef<bool(float)> Fits);
}

UCLASS()
class THREECROSSROADS_API ATCHUD : public AHUD
{
	GENERATED_BODY()

public:
	virtual void DrawHUD() override;
	virtual void EndPlay(const EEndPlayReason::Type Reason) override;
	// Round 16 (controls): client/index.html #roomBanners (the pause banner) under the score strip, in screen pixels
	// (Client/TCMatchControls.cpp, FTCRoomBanners)
#define TC_HAS_ROOM_BANNERS 1
	void DrawRoomBanners(UFont* Font);
	// Round 16 (controls): the stats sheet over everything (Client/TCStats.cpp), and the -TCStatUnitProbe log line
	void DrawStatsSheet(UFont* Font);
	// Round 16 (controls): F2's stats box under the panel its position belongs to (Client/TCStats.cpp): top left under
	// the match controls (#util), top centre under the strip's lines (#top), top right 52 px (#econ: none here); clear
	// of the score strip. CtlBottom, CtlRight: the match controls' panel in screen pixels (0: none). Returns the box's
	// bottom in screen pixels when it sits in the top left corner (the status column starts under it), else 0.
	float DrawStatsBox(UFont* Font, float CtlBottom, float CtlRight);
	float StatsTopTc = -1.f;
	FBox2D StatsStrip = FBox2D(ForceInit);
	float TopStackBottomPx = -1.f; // #top's physical bottom, also valid after EndUi resets UiNow
	// The button drawn at this viewport pixel in the last frame, if any (the controller sends its click there).
	bool ButtonAt(FVector2D Pixel, ETCUiAction& OutAction) const;
	bool CommandAt(FVector2D Pixel, FTCUiCommand& OutCommand) const;
	// Where the button for Action (or the one matching Command: same action and unit, building, job or group) was drawn
	// in the last frame (verification switches press it there).
	bool ButtonCentre(ETCUiAction Action, FVector2D& Out) const;
	bool CommandCentre(const FTCUiCommand& Command, FVector2D& Out) const;
	// A HUD panel (not the minimap) covers this pixel: a press there must not reach the world.
	bool PanelAt(FVector2D Pixel) const;
	// The labels of the buttons drawn in the last frame, for logs.
	FString ButtonLabels() const;
	TArray<FString> ButtonLabelList() const { TArray<FString> L; for (const FButton& B : Buttons) L.Add(B.Label); return L; }
	// Whether the HUD drew a frame since the match (or lobby) began: false under -nullrhi, where no frame is drawn.
	bool HasDrawn() const { return Frames > 0; }

	// The minimap (client/main.js drawMinimap), drawn here every frame of a match; the controller sends it presses.
	FTCMinimap Minimap;

private:
	struct FButton { FBox2D Rect; FTCUiCommand Command; FString Label; bool bEnabled = true; };
	// client/hud.js UI scale (#hud { zoom: var(--ui) }): the panels, buttons and text are drawn at UiNow through a canvas
	// transform, laid out in a window of ClipX / UiNow by ClipY / UiNow; the world's own overlays, the minimap (drawn
	// at its own size here) and the Controls sheet (a dialog outside #hud) are not. Buttons and panels are recorded in
	// screen pixels, so the controller's hit tests need no change.
	float UiNow = 1.f;
	bool bUiPushed = false;
	float UiOldClipX = 0.f, UiOldClipY = 0.f;
	void BeginUi(float Scale);
	void EndUi();
	FBox2D ToScreen(const FBox2D& R) const { return FBox2D(R.Min * UiNow, R.Max * UiNow); }
	float ToUi(float ScreenPixels) const { return ScreenPixels / UiNow; }
	TArray<FButton> Buttons;
	TArray<FBox2D> Panels;
	int32 Frames = 0;
	float BandTop = 0.f; // round 14: the score strip's bottom this frame (camera.js band)
	FBox2D RecruitBox = FBox2D(ForceInit); // round 15 (lead): the recruit card this frame, screen pixels (the alert feed clears it)
	// Round 15 (hudfit): client/main.js receiveStart measures camera.js's band (rig.frame: band(true)) right after
	// buildBuyBar, before any snapshot fills #scores, and camera.js keeps that band until a wheel notch, frame() or a
	// resize. So from a match's first frame until the controller has taken it, the HUD reports the band the
	// reference's HUD has at that moment (StartBand), not this HUD's own panels.
	bool bHadMatch = false, bStartBand = false;
	int32 StartBandFrames = 0;
	// The reference's panels at receiveStart, in CSS pixels at UI scale 1 (client/index.html): #top at top: var(--gap)
	// 8 px holding the empty #scores panel (padding 8 px 4 px 9 px and a 1 px border: 19 px), so its bottom is 27 px;
	// #buy at bottom: var(--gap) 8 px holding the group tabs buildCard makes with no group open (padding 7 px, a 1 px
	// border, one .hd row of 5 + 16 + 5 px with its 1 px border and #hud .hd's 6 px margin: 50 px), absent in Classic
	// and World (buildCard hides it). Zoomed by the UI scale like the rest of #hud.
	// Round 15: the reference CSS's left column (--left-w 248 px) is this HUD's narrowest match panel; the match panel
	// keeps its own 560 px while the recruit card beside it gets at least RecruitMin px (#buy sits between the left
	// column and the minimap at every scale, client/index.html), and narrows toward 248 px only past that.
	static constexpr float MatchPanelW = 560.f, LeftColumnMinW = 248.f, RecruitMinW = 120.f;
	// the lobby panel (round 16: it gives way down to LobbyPanelMinW so the match report's table fits beside it)
	static constexpr float LobbyPanelW = 700.f, LobbyPanelMinW = 480.f;
	// round 16: the recruit card's room the match panel gives way to (UI pixels) at a large UI scale in a small window
	static constexpr float RecruitWantW = 300.f;
	static float MatchPanelWidth(float RoomRight);
	// The native status column (the connection, room and tick diagnostics, the key reminder, the pause, graphics and
	// refusal lines and the last order's note), drawn unscaled in screen pixels. Round 15: it stays in the top left
	// corner beside the score strip, its lines wrapped at their ' | ' columns, so it no longer runs over the paper war
	// map's top left; when it would reach the match panel there, in the top right corner above the recruit card; when
	// neither has room, under the strip as before.
	// bDiag (round 16): one of this client's diagnostic lines (connection, room, tick, the key reminder)
	struct FStatusLine { FString Shown; FLinearColor Colour; bool bDiag = false; };
	bool bLoggedDiagDrop = false;
	static constexpr float ColumnShare = 0.26f, ColumnMinW = 240.f;
	TArray<FString> WrapShown(const FString& Shown, float MaxWidth, UFont* Font) const;
	// A text already in the language in use, cut to MaxWidth with '...' (the browser's text-overflow: ellipsis)
	FString FitShown(const FString& Shown, float MaxWidth, UFont* Font) const;
	// The kept-English log for a text drawn in Spanish that came back unchanged
	void NoteKept(const FString& Str, const FString& Shown);
	// Every text the HUD shows is measured and drawn through these two: client/i18n.js t() (TC_T, Client/TCLocale.h) is
	// applied here, so a Spanish label is measured as it is drawn, while what the HUD stores (button labels for hit tests
	// and logs), compares and logs stays English.
	void DrawLabel(const FString& Text, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale = 1.f);
	void LabelSize(const FString& Text, float& OutWidth, float& OutHeight, UFont* Font, float Scale = 1.f) const;
	// A text already in the language in use (put together from parts that were each passed through TC_T): drawn and
	// measured as it is
	void DrawShown(const FString& Text, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale = 1.f);
	void ShownSize(const FString& Text, float& OutWidth, float& OutHeight, UFont* Font, float Scale = 1.f) const;
	// In Spanish, the distinct texts drawn that came back unchanged (no entry or pattern in client/es.js, or a name kept
	// as it is), logged once each as "locale: kept in English" (CLI-120's capture review reads them)
	TSet<FString> KeptEnglish;
	// client/pings.js drawMinimap: each relayed ping's ring on the minimap (dark under chalk, growing and fading)
	void DrawMinimapPings();
	float Button(UFont* Font, float X, float Y, const FString& Label, ETCUiAction Action);
	// MaxWidth (round 15): the button is no wider, its label cut with '...' (the stored label stays whole)
	float Button(UFont* Font, float X, float Y, const FString& Label, const FTCUiCommand& Command, bool bEnabled, const FLinearColor& Fill, float MinWidth = 0.f, float MaxWidth = 0.f);
	void AddPanel(float X, float Y, float W, float H, const FLinearColor& Colour);
	// LobbyW (round 16): the lobby panel's width (700 px, narrower so the match report fits beside it)
	float DrawLobby(UFont* Font, float Y, float LobbyW = LobbyPanelW);
	// Round 16 (hudfit): TopLimit is the highest the panel may reach (UI pixels); when it would reach above it the panel
	// drops what the browser's left column does not hold (FTCHudFit::MatchPanelLevels). bDraw false only measures:
	// the top it would have. Settings stay here only when there is no match menu to hold them (bSettingsInMenu).
	float DrawMatchPanel(UFont* Font, float Width = MatchPanelW, float TopLimit = 0.f, bool bDraw = true);
	bool bSettingsInMenu = false;
	int32 MatchPanelLevel = 0, LoggedPanelLevel = -1;
	void DrawRecruitPanel(UFont* Font, float Left, float Bottom);
	// Round 17: the recruit card's top limit this frame (UI pixels; 0: none) and the browser's #buy.fit card width
	// (.uc: clamp(50px, ..., 100px)); MatchPanelCut: the text rows the match panel's last layout cut (fit level 4)
	float RecruitTopLimit = 0.f;
	static constexpr float RecruitFitMinW = 50.f;
	FString RecruitFitLogged;
	int32 MatchPanelCut = 0;
	// A line over the ground through metre points (each leg split every 2 m so it follows the relief), Lift metres up.
	void GroundLine(const ATCTerrain& T, const TArray<FVector2D>& Points, bool bClosed, const FLinearColor& Colour, float Thickness, float Lift = 0.3f);
	void GroundRing(const ATCTerrain& T, FVector2D At, float Radius, const FLinearColor& Colour, float Thickness);
	// The placement preview, rally markers and logistics overlay in the view; then the logistics overlay on the minimap.
	void DrawWorldOverlays(UFont* Font);
	void DrawMinimapOverlays();
	// Slice 9 (see the file comment). Each returns or draws only what this recipient's messages delivered.
	float DrawScoreStrip(UFont* Font, UFont* Big);
	// Round 17: the strip's one card in screen pixels as last drawn (invalid without a strip)
	FBox2D StripCard = FBox2D(ForceInit);
	// Round 17: the alert history box as last drawn (UI pixels; DrawAlertFeed), which the sandbox panel keeps clear of
	FBox2D AlertHistoryBox = FBox2D(ForceInit);
	void DrawCapturePoints(UFont* Font);
	void DrawPlans();
	void DrawHealthBars();
	void DrawAlertFeed(UFont* Font);
	void DrawEpilogue(UFont* Font, UFont* Big);
	void DrawControlsSheet(UFont* Font, UFont* Big);
	// X0 (round 16): the card's left edge beside the lobby panel (the panel's right edge plus the gap)
	void DrawMatchReport(UFont* Font, float Top, float LobbyBottom = 0.f, float X0 = 716.f);
	// Round 16: the lobby panel's width so the match report beside it gets its table's width (FTCEndReport::NaturalWidth,
	// at least its MinWidth): the panel gives way from LobbyPanelW down to LobbyPanelMinW
	float LobbyWidthBesideReport(UFont* Font) const;
	FVector2D ReportLogged = FVector2D(-1, -1);
	// Round 15 (lobby; defined in Client/TCMatchControls.cpp): rows of FTCControlRow as a panel of texts and
	// ETCUiAction::MatchControl buttons, and the match's Menu, sandbox panel and Send next wave
	float DrawControlRows(UFont* Font, float X, float Y, float W, const TArray<FTCControlRow>& Rows);
	void DrawMatchControls(UFont* Font, float Top);
	// round 15 (lead): the match controls panel's size at the UI scale (0 when there is nothing to draw), so the HUD can
	// place it clear of the score strip and keep the status column clear of it
	FVector2D MeasureMatchControls(UFont* Font);
	// Round 16 (hudfit): the match controls where the browser has them. The Menu button in the top left corner (#util),
	// the score strip kept clear of it; the open menu under it (#menu) with the settings the browser keeps there
	// (MenuSettingRows); the sandbox panel at #sandbox's place (centred, 230 px over the bottom) or, when that is taken,
	// under the strip; Send next wave in the strip's Horde lead block (hud.js .sc-next, DrawScoreStrip).
	TArray<FTCControlRow> MenuSettingRows() const;
	void SplitMatchControls(TArray<FTCControlRow>& Corner, TArray<FTCControlRow>& Sandbox) const;
	FVector2D MeasureControlRows(UFont* Font, const TArray<FTCControlRow>& Rows);
	float CornerRight = 0.f; // UI pixels: the Menu button's panel's right edge (the strip starts right of it)
	FBox2D SandboxBox = FBox2D(ForceInit), MatchPanelBox = FBox2D(ForceInit); // UI pixels, last frame
public:
	// A control row item's command: 'Setting.<what>' keys are this HUD's settings (Volume, Graphics, ...); the rest are
	// ETCUiAction::MatchControl with the key (the room and match controls, Client/TCMatchControls.h)
	static FTCUiCommand ControlCommand(const FString& Key, int32 Slot);
private:
	// client/main.js drawMinimap: the capture points (owner discs, the cut cross, the locked link) and the start's spawns
	void DrawMinimapPoints();
	// ORDER_DASH: 2.2 m period, 1.3 m lit, anchored at the goal (overlay.js uEnd).
	void GroundDashes(const ATCTerrain& T, FVector2D A, FVector2D B, const FLinearColor& Colour, float Thickness, float Period = 2.2f, float Lit = 1.3f);
	// A filled screen-space quad (two canvas triangles).
	void FillQuad(const FVector2D (&P)[4], const FLinearColor& Colour);
	// client/main.js frame: the preview keeps its last colour when the browser's placement check throws
	bool bLastPreviewOk = false;
};
