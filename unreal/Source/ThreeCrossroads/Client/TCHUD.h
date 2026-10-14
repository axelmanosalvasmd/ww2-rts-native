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
#include "TCHUD.generated.h"

class UFont;
class ATCTerrain;

UCLASS()
class THREECROSSROADS_API ATCHUD : public AHUD
{
	GENERATED_BODY()

public:
	virtual void DrawHUD() override;
	virtual void EndPlay(const EEndPlayReason::Type Reason) override;
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
	TArray<FButton> Buttons;
	TArray<FBox2D> Panels;
	int32 Frames = 0;
	// Every text the HUD shows is measured and drawn through these two: client/i18n.js t() (TC_T, Client/TCLocale.h) is
	// applied here, so a Spanish label is measured as it is drawn, while what the HUD stores (button labels for hit tests
	// and logs), compares and logs stays English.
	void DrawLabel(const FString& Text, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale = 1.f);
	void LabelSize(const FString& Text, float& OutWidth, float& OutHeight, UFont* Font, float Scale = 1.f) const;
	// A text already in the language in use (put together from parts that were each passed through TC_T): drawn and
	// measured as it is
	void DrawShown(const FString& Text, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale = 1.f) { AHUD::DrawText(Text, Colour, X, Y, Font, Scale); }
	void ShownSize(const FString& Text, float& OutWidth, float& OutHeight, UFont* Font, float Scale = 1.f) const { AHUD::GetTextSize(Text, OutWidth, OutHeight, Font, Scale); }
	// In Spanish, the distinct texts drawn that came back unchanged (no entry or pattern in client/es.js, or a name kept
	// as it is), logged once each as "locale: kept in English" (CLI-120's capture review reads them)
	TSet<FString> KeptEnglish;
	// client/pings.js drawMinimap: each relayed ping's ring on the minimap (dark under chalk, growing and fading)
	void DrawMinimapPings();
	float Button(UFont* Font, float X, float Y, const FString& Label, ETCUiAction Action);
	float Button(UFont* Font, float X, float Y, const FString& Label, const FTCUiCommand& Command, bool bEnabled, const FLinearColor& Fill, float MinWidth = 0.f);
	void AddPanel(float X, float Y, float W, float H, const FLinearColor& Colour);
	float DrawLobby(UFont* Font, float Y);
	float DrawMatchPanel(UFont* Font);
	void DrawRecruitPanel(UFont* Font, float Left, float Bottom);
	// A line over the ground through metre points (each leg split every 2 m so it follows the relief), Lift metres up.
	void GroundLine(const ATCTerrain& T, const TArray<FVector2D>& Points, bool bClosed, const FLinearColor& Colour, float Thickness, float Lift = 0.3f);
	void GroundRing(const ATCTerrain& T, FVector2D At, float Radius, const FLinearColor& Colour, float Thickness);
	// The placement preview, rally markers and logistics overlay in the view; then the logistics overlay on the minimap.
	void DrawWorldOverlays(UFont* Font);
	void DrawMinimapOverlays();
	// Slice 9 (see the file comment). Each returns or draws only what this recipient's messages delivered.
	float DrawScoreStrip(UFont* Font, UFont* Big);
	void DrawCapturePoints(UFont* Font);
	void DrawPlans();
	void DrawHealthBars();
	void DrawAlertFeed(UFont* Font);
	void DrawEpilogue(UFont* Font, UFont* Big);
	void DrawControlsSheet(UFont* Font, UFont* Big);
	void DrawMatchReport(UFont* Font, float Top);
	// client/main.js drawMinimap: the capture points (owner discs, the cut cross, the locked link) and the start's spawns
	void DrawMinimapPoints();
	// A line over the ground drawn in dashes of Dash metres with gaps of Gap metres (client/markers.js ORDER_DASH).
	void GroundDashes(const ATCTerrain& T, FVector2D A, FVector2D B, const FLinearColor& Colour, float Thickness, float Dash = 1.2f, float GapM = 0.8f);
	// A filled screen-space quad (two canvas triangles).
	void FillQuad(const FVector2D (&P)[4], const FLinearColor& Colour);
	// client/main.js frame: the preview keeps its last colour when the browser's placement check throws
	bool bLastPreviewOk = false;
};
