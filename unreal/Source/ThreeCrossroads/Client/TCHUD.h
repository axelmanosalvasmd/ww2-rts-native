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
#include "Client/TCRecruitment.h"
#include "Client/TCHUD26Feedback.h"
#include "TCHUD.generated.h"

class UFont;
class ATCTerrain;
class FTCBossBar;
class FTCHudPortraits;

// Round 16 (hudfit): the HUD's fit rules at a large UI scale in a small window, pure so tests read them.
namespace FTCHudFit
{
	inline bool Contains(const FBox2D& Box,FVector2D Point)
	{ return Box.bIsValid && TCHud26::Contains(Box.Min.X,Box.Min.Y,Box.Max.X,Box.Max.Y,Point.X,Point.Y); }
	// index.html's lower-left column and #buy. Drawing and tests consume the same measured boxes.
	constexpr float LeftW = 248.f, ScreenGap = 8.f, GridGap = 3.f, OrderH = 50.f;
	// index.html's media query measures the physical viewport, independently of #hud zoom.
	inline float LeftWidth(FVector2D View, float Scale) { return View.X * Scale >= 1600 && View.Y * Scale >= 860 ? 268.f : LeftW; }
	struct FControl
	{
		FString Label, Icon, Key, Value, Hover;
		FTCUiCommand Command;
		bool bEnabled = true, bOn = false;
		bool bSubmenu = false;
		FString Sub;
		bool bBroke = false;
		bool bPending = false;
		bool bLetterHit = false;
	};
	struct FSelection
	{
		FString Type, Name;
		int32 Count = 1, Hp = 0, MaxHp = 0;
		TArray<FString> Tags;
		FString Hover;
		int32 Owner = -1;
	};
	struct FIcon { FString Key; FBox2D Box; FLinearColor Colour; bool bPortrait = false; int32 Owner = -1; FBox2D Clip = FBox2D(ForceInit); };
	struct FHit { FBox2D Box; FControl Control; };
	struct FTextFit { FString Text; FBox2D Box, Clip; };
	struct FProductionInfo
	{
		FString Name, Icon, Status;
		TArray<FControl> Slots, Research;
		FString Tier;
		float Progress = 0;
		bool bTraining = false;
	};
	struct FTooltipInteraction
	{
		FBox2D Dismissed = FBox2D(ForceInit);
		bool bPressed = false;
		void Frame(FVector2D Pointer, bool bPointerDown, const FBox2D& PreviousAnchor)
		{
			bPressed = bPointerDown;
			if (Dismissed.bIsValid && !Contains(Dismissed,Pointer)) Dismissed=FBox2D(ForceInit);
			if (bPointerDown && Contains(PreviousAnchor,Pointer)) Dismissed=PreviousAnchor;
		}
		bool Allows(FVector2D Pointer) const { return !Contains(Dismissed,Pointer); }
		bool Allows(FVector2D Pointer, const FBox2D& Anchor)
		{
			// A click can enter a new button before its first tooltip frame.
			if (bPressed && Contains(Anchor,Pointer)) Dismissed=Anchor;
			return Allows(Pointer);
		}
	};
	struct FBottomLayout
	{
		FTCPaint Paint;
		TArray<FIcon> Icons;
		TArray<FHit> Hits;
		TArray<FTextFit> TextBounds;
		int32 SelectionMax = 0;
		FBox2D SelectionList = FBox2D(ForceInit);
		int32 MenuMax = 0, AirMax = 0, RecruitMax = 0, LogisticsMax = 0;
		FBox2D MenuList = FBox2D(ForceInit), AirList = FBox2D(ForceInit), RecruitList = FBox2D(ForceInit);
		FBox2D Selection = FBox2D(ForceInit), Orders = FBox2D(ForceInit), Recruit = FBox2D(ForceInit), Economy = FBox2D(ForceInit);
		FBox2D Logistics = FBox2D(ForceInit), LogisticsList = FBox2D(ForceInit);
	};
	// Original index.html button/uc/ob hover rules, applied to the same visible hit boxes as input.
	THREECROSSROADS_API void ApplyPointer(FBottomLayout& Layout, FVector2D Pointer, bool bHeld = false);
	THREECROSSROADS_API FBottomLayout LeftPanel(float ViewH, const FString& Title, const TArray<FSelection>& Selection,
		const TArray<FString>& Details, const TArray<FControl>& Orders, const TArray<FControl>& Menu, const FTCMeasure& Measure, float Scale = 1.f,
		int32 Idle = 0, int32 Destination = 1, bool bTransfer = false, bool bDropdown = false, int32 SelectionFirst = 0, float TopLimit = 80.f,
		int32 MenuFirst = 0, const TArray<FControl>& MenuTabs = {}, float Width = LeftW);
	THREECROSSROADS_API FBottomLayout RecruitPanel(float ViewW, float ViewH, float Right, const TArray<FString>& Headings,
		const TArray<FControl>& Tabs, const TArray<TArray<FControl>>& Lists, const TArray<FControl>& Above, const FTCMeasure& Measure, float Scale = 1.f,
		const TArray<FString>& Logistics = {}, int32 TotalCards = 0, int32 First = 0, float TopLimit = 80.f, const FBox2D& Economy = FBox2D(ForceInit), int32 LogisticsFirst = 0);
	THREECROSSROADS_API FBottomLayout EconomyPanel(float ViewW, const FString& Money, const FString& Income, const FString& Population,
		const TArray<FControl>& Support, const FTCMeasure& Measure, float Scale = 1.f, const TArray<FControl>& Resources = {}, const TArray<FControl>& Air = {},
		float MaxH = 0.f, int32 AirFirst = 0);
	THREECROSSROADS_API FBottomLayout ProductionPanel(FVector2D View, float Right, const FProductionInfo& Info,
		const TArray<FString>& GroupNames, const TArray<TArray<FControl>>& Groups, const TArray<FString>& Logistics,
		const FTCMeasure& Measure, float Scale, FVector2D Pointer = FVector2D(-1,-1), int32 LogisticsFirst = 0);
	struct THREECROSSROADS_API FPlaneInteraction
	{
		int32 LastId = 0;
		double LastAt = -1;
		TSet<int32> Press(const class FTCMatchState& State, const TSet<int32>& Current, int32 Id, bool bToggle, double Now);
	};
	THREECROSSROADS_API TSet<int32> PlaneSelection(const class FTCMatchState& State, const TSet<int32>& Current, int32 Id, bool bToggle, bool bDouble);
	// HUD controls keep their complete tooltip even when the visible caption is elided.
	THREECROSSROADS_API FString ControlTip(const FControl& Control);
	// A match HUD right press is swallowed, except the ability's separate context action.
	THREECROSSROADS_API bool RightStepsBack(const FTCUiCommand& Command, bool bHasMatch);
	THREECROSSROADS_API FBottomLayout Tooltip(const FString& Shown, const FBox2D& Anchor, FVector2D View, const FTCMeasure& Measure);
	struct FControlsLayout
	{
		FTCPaint Paint;
		FBox2D Bounds = FBox2D(ForceInit), Viewport = FBox2D(ForceInit), Close = FBox2D(ForceInit);
		float ScrollMax = 0;
	};
	THREECROSSROADS_API FControlsLayout ControlsDialog(FVector2D View,const FTCMeasure& Measure,float Scroll = 0);
	THREECROSSROADS_API FBottomLayout IconBar(bool bMenu, bool bCaptured, const FTCMeasure& Measure, float Scale = 1.f,
		bool bLogistics = false, bool bSupplyOverlay = false, bool bTruckSelect = false);
	// A source DOM panel painted later also intercepts clicks on the controls underneath it.
	THREECROSSROADS_API void BehindPanels(FBottomLayout& Layout, const TArray<FBox2D>& Foreground);
	THREECROSSROADS_API FBottomLayout Toolbar(float MaxW, const TArray<FControl>& Controls, const FTCMeasure& Measure, float Scale = 1.f);
	THREECROSSROADS_API FBottomLayout SandboxToolbar(FVector2D View, float CornerRight, float ScoreLeft,
		const TArray<FControl>& Controls, const TArray<FBox2D>& Avoid, const FTCMeasure& Measure, float Scale = 1.f, bool bShown = true);
	// The browser's central boss frame and padding, including its overlaps in a crowded viewport.
	THREECROSSROADS_API FTCPaint BossCard(const FTCBossBar& Boss, FVector2D View, float Top, const FBox2D& Economy,
		const TArray<FBox2D>& Avoid, double Now, const FTCMeasure& Measure, float Scale = 1.f);
	// Empty on a sound layout. Text is checked against its actual container, controls against each other and panels.
	THREECROSSROADS_API TArray<FString> LayoutErrors(const FBottomLayout& Layout, FVector2D View);
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
	// Round 17: main.js renderLobby's Horde record sentence after #modeInfo (leading space included), from the lobby's
	// hordeBest (FTCMatchState::LobbyHordeBest with the lead patch; empty without it)
	THREECROSSROADS_API FString HordeRecordText(int32 Defenders, bool bBest, int32 Wave, int32 Time, const TArray<FString>& Names);
	THREECROSSROADS_API FString HordeRecordText(const class FTCMatchState& S);
	// Round 17: main.js's #connectionBanner text for connection.js's 'retry' (English; drawn through TC_T: es.js has both)
	THREECROSSROADS_API FString ConnectionBannerText(bool bFullRoom, int32 SecondsLeft);

}

UCLASS()
class THREECROSSROADS_API ATCHUD : public AHUD
{
	GENERATED_BODY()

public:
	virtual void PostRender() override;
	virtual void DrawHUD() override;
	virtual void EndPlay(const EEndPlayReason::Type Reason) override;
	// Controller integration is a small lead patch. These local UI commands never cross the wire.
	bool RunPanelCommand(const FTCUiCommand& Command, bool bShift, bool bCtrl = false);
	bool RecruitKey(const FString& Code, bool bShift, bool bCtrl, bool bAlt);
	void NoteCardLetter(const FString& Type);
	bool ScrollSelection(FVector2D Pixel, int32 Notch);
	bool ScrollControls(FVector2D Pixel, int32 Notch);
	bool Recruiting() const { return RecruitNavigation.bLetters; }
	void StopRecruiting();
	void ToggleRecruitGroup(int32 Group);
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
	// Verification only: require an observed disabled active-training rectangle and the current producer/type.
	bool ObservedTrainingSlot(const FBox2D& Observed, int32 Producer, const FString& Type, FVector2D& Out, FString* Refusal = nullptr) const;
	// Opt-in input receipts describe the same topmost hit that CommandAt consumes.
	FString DrawnTargetDescription(FVector2D Pixel) const;
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
	friend struct FTCHud28Access;
	// Physical bounds of the retained hit table. Canvas exists only during PostRender.
	FVector2D DrawnViewport = FVector2D::ZeroVector;
	struct FButton { FBox2D Rect; FTCUiCommand Command; FString Label; bool bEnabled = true; FString Hover; };
	// Opt-in integration evidence from the actual drawn controls. Does not synthesize clicks or purchases.
	void WriteAcceptanceAudit();
	FString AcceptanceSignature;
	// client/hud.js UI scale (#hud { zoom: var(--ui) }): the panels, buttons and text are drawn at UiNow through a canvas
	// transform, laid out in a window of ClipX / UiNow by ClipY / UiNow; the world's own overlays, the minimap (drawn
	// at its own size here) and the Controls sheet (a dialog outside #hud) are not. Buttons and panels are recorded in
	// screen pixels, so the controller's hit tests need no change.
	void DrawMenuRows(UFont* Font, float X, float Y, const TArray<FTCControlRow>& Rows);
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
	void DrawRecruitPanel(UFont* Font, float Left, float Bottom);
	void DrawBottomLayout(const FTCHudFit::FBottomLayout& Layout, UFont* Font);
	void DrawHudIcon(const FString& Key, const FBox2D& Box, FLinearColor Colour, const FBox2D& Clip = FBox2D(ForceInit));
	void OfferTooltip(const FString& Text, const FBox2D& ScreenBox);
	void DrawHint(UFont* Font);
	TSharedPtr<FTCHudPortraits> Portraits;
	bool bLossFixtureDrawn = false;
	FString PendingTooltip;
	FBox2D TooltipAnchor = FBox2D(ForceInit);
	FTCHudFit::FTooltipInteraction TooltipInteraction;
	FBox2D SelectionScrollBox = FBox2D(ForceInit), CornerBox = FBox2D(ForceInit);
	int32 SelectionFirst = 0, SelectionMax = 0;
	int32 MenuFirst = 0, MenuMax = 0, AirFirst = 0, AirMax = 0, RecruitFirst = 0, RecruitMax = 0, BuildTab = 0;
	FString MenuContext, RecruitContext;
	FBox2D EconomyBox = FBox2D(ForceInit);
	FBox2D MenuScrollBox = FBox2D(ForceInit), AirScrollBox = FBox2D(ForceInit), RecruitScrollBox = FBox2D(ForceInit);
	FBox2D LogisticsScrollBox = FBox2D(ForceInit);
	int32 LogisticsFirst = 0, LogisticsMax = 0;
	FBox2D ControlsScrollBox = FBox2D(ForceInit);
	float ControlsScroll = 0, ControlsScrollMax = 0;
	FTCHudFit::FPlaneInteraction PlaneInteraction;
	int32 GroupDestination = 1;
	bool bGroupDropdown = false;
	FTCRecruitNavigation RecruitNavigation;
	FString LetterCardIdentity;
	TMap<FString,TCHud26::FLetterHit> LetterHits;
	void UpdateRecruitNavigation();
	// A line over the ground through metre points (each leg split every 2 m so it follows the relief), Lift metres up.
	void GroundLine(const ATCTerrain& T, const TArray<FVector2D>& Points, bool bClosed, const FLinearColor& Colour, float Thickness, float Lift = 0.3f);
	void GroundRing(const ATCTerrain& T, FVector2D At, float Radius, const FLinearColor& Colour, float Thickness);
	// The placement preview, rally markers and logistics overlay in the view; then the logistics overlay on the minimap.
	void DrawWorldOverlays(UFont* Font);
	void DrawMinimapOverlays();
	// Slice 9 (see the file comment). Each returns or draws only what this recipient's messages delivered.
	float DrawScoreStrip(UFont* Font, UFont* Big, FTCHudFit::FBottomLayout Sandbox);
	// Round 17: the strip's one card in screen pixels as last drawn (invalid without a strip)
	FBox2D StripCard = FBox2D(ForceInit);
	// Round 17: the alert history box as last drawn (UI pixels; DrawAlertFeed), which the sandbox panel keeps clear of
	FBox2D AlertHistoryBox = FBox2D(ForceInit);
	void DrawCapturePoints(UFont* Font);
	void DrawPlans();
	void DrawHealthBars();
	FTCHudFit::FBottomLayout DeferredEconomy;
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
	void DrawMinimapAlertRings();
	// ORDER_DASH: 2.2 m period, 1.3 m lit, anchored at the goal (overlay.js uEnd).
	void GroundDashes(const ATCTerrain& T, FVector2D A, FVector2D B, const FLinearColor& Colour, float Thickness, float Period = 2.2f, float Lit = 1.3f);
	// A filled screen-space quad (two canvas triangles).
	void FillQuad(const FVector2D (&P)[4], const FLinearColor& Colour);
	// client/main.js frame: the preview keeps its last colour when the browser's placement check throws
	bool bLastPreviewOk = false;
};
