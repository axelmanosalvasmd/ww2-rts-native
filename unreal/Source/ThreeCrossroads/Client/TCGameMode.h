// Glue for the first slice: one connection to a reference-protocol room, the decoded state, and the presentation.
// Command line (all optional): -TCServer=ws://127.0.0.1:3000/ws -TCRoom=native -TCName=Commander -TCSpectate
//   -TCSeat=<label> (one identity per server, room and label) -TCTokenFile=<absolute path>; or TC_TOKEN in the
//   environment. -TCToken=<token> is refused: the engine logs the command line. See Net/TCToken.h.
// Verification switches, for runs where nobody is at the controls (they use the same paths as the mouse):
//   -TCHostStart=N  as host in the lobby: add N AI seats, then start  | -TCAutoMove  move own units toward the centre
//   -TCHostMode=M   with -TCHostStart: pick mode M first (conquest, world, ...: server.js 'mode')
//   -TCDropAfter=S  drop the socket once, S s into the match (reconnect) | -TCExitAfter=S  quit S s after the game began
// Environment: TC_DECODER=rust (the default since slice 12) takes the unit rows and the resync verdict from the Rust
//   recipient view (ww2_view_*) after its startup self-check, still comparing it with the C++ decoder (LogTC "rust view
//   shadow"); TC_DECODER=shadow (a fallback) runs the view beside the C++ decoder, which drives; TC_DECODER=cpp runs no
//   view. TC_RUST_VIEW=1/0 are old names for shadow/cpp. See Net/TCRustViewShadow.h.
//   -TCEndAfter=S   as host, end the match S s into it (server.js 'end': the room goes back to the lobby)
//   -TCShotAfter=S  S s into the match, save a screenshot with the HUD to -TCShotFile (needs a renderer)
//   -TCLobbyShotAfter=S  S s into a lobby with no match (the first, or the one after an end), a screenshot to
//                   -TCLobbyShotFile (needs a renderer)
//   -TCStartZoom=D  at each new match, zoom to D metres (clamped to the widest view, the whole map, like the wheel)
//   -TCAutoAttackMove  as -TCAutoMove, but an attack-move (the G order)
//   -TCAutoStopAfter=S  S s after the auto move or attack-move, stop those units (the Stop button / X key path)
//   -TCHostSandbox  with -TCHostStart: switch the room's Sandbox on first (server.js 'sandbox')
//   -TCSpawnEnemy=T in a sandbox match, once: spawn one enemy unit of type T 30 m from the army toward the map's centre
//                   (client/main.js sandbox spawn: { t: 'spawn', unit, enemy: true, x, z })
//   -TCAutoAttack   once an enemy unit is held, select the army and right-click it: through the screen pick when it
//                   is drawn on screen, else with the enemy named directly
//   -TCAttackWithTrucks  with -TCAutoAttack: the selection also takes the supply trucks (client/orders.js then sends
//                   { t: 'orders', commands: [attack, move for the trucks] }); trucks switch to manual runs. Only
//                   automatic supply trucks exist to take: a sandbox spawn of an own truck crashes the reference server
//                   at e4e579a (shared/supply-convoys.js fleet reads u.convoy.state of a truck that has none)
//   -TCProbeDeny    once in a match, send an attack on one of this seat's own units, which the server must refuse
//                   ('unseen'): checks the refusal path and its sentence
//   -TCRecruit=a,b  once in a match, press the recruit card of each unit type in turn (2.5 s apart), opening its group
//                   tab first: through the HUD's hit test when it is drawn. A card the browser's check refuses shows
//                   the reason and sends nothing. Logs MP and the own count of that type before and after.
//   -TCRecruitShift with -TCRecruit: the presses hold Shift (buy up to five)
//   -TCSelect=T     once in a match, before any -TCRecruit press, select this seat's first unit of type T (set directly,
//                   as a click would): a Production Building gets its Classic card, or opens its recruit group
//   -TCCancelQueued after the -TCRecruit presses, press the first waiting queue place that can be cancelled (Classic and
//                   World: { t: 'cancelProduction', id, job })
//   -TCHostArmy=A   with -TCHostStart: pick the army size first (server.js 'army': standard, large, massive, endless)
//   -TCProbeBuyDeny=a,b  once in a match, after any -TCRecruit presses, send { t: 'buy', unit } for each type in turn
//                   (2 s apart) without the client's check, to see the server's refusal ('mp', 'tier', ...)
//   -TCGroupCheck   once in a match, run a control-group script through simulated key events fed to the controller's
//                   engine input (ATCPlayerController::InjectKey, the engine's bIsSimulatedInput events; the selection
//                   before each chord is set directly): Ctrl+1, Alt+2, 1, 2 twice (centres), Shift+2
//   -TCMinimapCheck once in a match, press the minimap: left at the map centre (the camera goes there), then, with the
//                   army selected, right 20 m from the army toward the centre (a move); with no renderer the minimap is
//                   laid out for a 1280 x 720 window instead of drawn. Its fog and knowledge counts are logged with
//                   the status line, with a check that no cell this side does not know is drawn as ground
//   -TCAutoMoveMetres=D  -TCAutoMove and -TCAutoAttackMove go D metres toward the centre (default 20)
//   -TCHostSeats=N  with -TCHostStart: wait until N seats are taken (other clients joined) before adding AI and starting
//   -TCHostTeams=a,b,c  with -TCHostStart: after the AI seats are added, set seat i's team to the i-th value
//                   (server.js 'team', host only) before the start
//   -TCBuildCheck=K once in a match, the construction script: select this seat's builder squads (set directly), arm
//                   building K (Classic and World: its key through a simulated key event; otherwise the Orders panel's
//                   Build menu and K's button, pressed through the HUD's hit test), click a spot the client's placement
//                   check refuses (nothing is sent), then a spot it accepts near the builders (through the screen pixel's
//                   ground pick when drawn), and report the site's progress. -TCBuildShotFile=path: a screenshot of the
//                   armed preview over the accepted spot before that click (the pointer's ground pinned there)
//   -TCBuildProbe   after -TCBuildCheck's click, send { t: 'build' } for K at a spot this side has never seen, past the
//                   client's check, to see the server's refusal ('notVisible')
//   -TCArmyCheck    once in a match, the army command script through simulated key events (selections set directly):
//                   Shift+F twice, Shift+G, Shift+X, Shift+F with nothing selected (refused by the client), Shift+H and
//                   a left click 15 m toward the centre (the army rally), a right click with the HQ selected (its
//                   rally), R with only the HQ selected (the server refuses: 'needs'), then R with the army
//   -TCSupplyCheck  once in a match with logistics, the supply script: select an own truck (directly), right-click an
//                   own squad through the screen pick (a supply run), press Resume deliveries, send a supply run to a
//                   unit that does not exist past the client's check ('needs'), switch Truck select on and drag over
//                   the whole view, then off and drag again, then the supply overlay on with the trucks selected.
//                   -TCSupplyShotFile=path: a screenshot then
//   -TCSupplyTo=ally once an allied (not own) finished Supply Cache is held, select every own truck and right-click
//                   the cache (the reference's relief), then report its stock as this seat sees it
//   -TCSupportCheck once in a match, the support order script: Mount up (Shift+Q) and Unload (Shift+E) through
//                   simulated key events, a carrier's Orders buttons through the HUD's hit test, the right click on an own
//                   site with builders (assist, through the screen pick when drawn) and the site card's Cancel through
//                   the HUD's hit test. -TCSupportBuild=K first places a K site (the -TCBuildCheck path) to work on
//   -TCAbilityCheck once in a match, the ability script: select the army (directly), F through a simulated key event
//                   (the instant or aimed ability F picks), an aimed ability's click through the ground pick, the Orders
//                   panel's ability buttons through the HUD's hit test, an autocast right click on one, then F again at
//                   once (the server's cooldown refusal) and F with nothing selected (the client's refusal)
//   -TCFieldCheck   once in a match, the fieldworks script: select the builder squads (directly), T through a simulated
//                   key event (the trench aim), a refused click and an accepted one through the ground pick, Shift+T and a
//                   two-click trench line, the Trench menu and one of its patterns through the HUD's hit test, then a
//                   right click with another squad on the planned works (join). -TCFieldShotFile=path: a screenshot of
//                   the armed line preview
//   -TCFormationCheck once in a match, the formation script (TCSlice6Checks.cpp): Shift+V, [ and ] through simulated
//                   key events, the Form menu and its buttons through the HUD's hit test, a right-drag facing gesture and a
//                   plain right click through the controller's gesture path at computed pixels, an attack-move drag, the
//                   control group copy (Ctrl+1, recall 1), a re-form refused by the client (one unit) and a raw move of the
//                   HQ past the client's check (the server: 'blocked'). -TCFormationShotFile=path: a screenshot of the drag
//   -TCPowersCheck  once in a match (a sandbox: an own mortar is spawned first), the support and area fire script:
//                   support keys and the support bar through the HUD's hit test, a strip call's two clicks and a point
//                   call through the ground pick, the client's cooldown refusal, the server's (a second call before the
//                   next snapshot), Shift+B with and without a mortar, area fire clicks (Shift keeps the aim), and a raw
//                   fireat by riflemen past the client's check (the server: 'blocked'). -TCPowersShotFile=path: a screenshot
//                   of the strip preview and the public strike warning
//   -TCCoverCheck   once in a match, the cover preview script: own infantry selected, the pointer's ground pinned next
//                   to them (no mouse offscreen), the marks reported, a move's flash, the menu switch off and on through
//                   the HUD's hit test. -TCCoverShotFile=path: a screenshot of the marks
//   -TCLobbyCheck   in the lobby (TCSlice7Checks.cpp), the lobby settings script: each step presses a setting's HUD
//                   button through the hit test (FTCLobbySettings::PlanStep), waits for the lobby that shows the value
//                   (or reports it still pending after 4 s), and logs the room. As host: Map, Army size, Mode (to Assault),
//                   Defender, an AI seat's team, faction and difficulty, own faction, Mode (to World Conquest), World size,
//                   Mode back (to -TCLobbyCheckMode, default conquest); as a guest: own faction, then Map (refused here)
//   -TCSaveCheck    as host, once in a match (TCSlice7Checks.cpp): the save and load script through the HUD's hit test:
//                   Pause, Save match ('saved' and the lobby's row), Resume, a second Save, End match, the lobby's saves
//                   panel, Load on the older row (the paused start), a socket drop while paused (the reconnect keeps the
//                   pause and the rows), Resume, and the tick moving on from the saved one
//   -TCGarrisonCheck once in a match: the world objects' houses and the house right click: the army selected (directly),
//                   the camera on the nearest standing house, the right press through the controller's world path at the
//                   pixel of that house's roof (FTCWorldObjects::PickHouse), then the garrison flags in later snapshots.
//                   With -TCHostSandbox an own mortar is spawned first, so the order also carries fireat (and a move for
//                   the rest). -TCGarrisonShotFile=path: a screenshot of the houses before the press
//   -TCLetterCheck  in Classic, once the HQ is finished: select it (directly), then Q, W and Shift+E through simulated
//                   key events (the card letters of client/keys.js 'building' context: buy cards 1 and 2; Shift+E with no
//                   card 3 falls back to Unload)
//   -TCResearchCheck once in a match (TCSlice8Checks.cpp): the own HQ selected (directly), its research panel logged,
//                   the tier button pressed through the hit test while MP is short (refused by this client's check,
//                   nothing sent), a raw { t: 'tech', kind: 'inf' } probe with no Armory (the server's deny), then the tier
//                   pressed again once the wallet covers it (sent), the lab job in a snapshot, its ready announcement
//   -TCRecoverCheck in World Conquest with -TCHostSandbox (TCSlice8Checks.cpp): the score, regions and card logged, a
//                   Restore press with no card (refused here), a raw { t: 'recover' } probe while HQ and Engineers live (the
//                   server's 'needs'), enemy tanks spawned on the own Engineers, then once the snapshot offers recovery the
//                   Restore card pressed through the hit test and the server's answer (an Engineer queued or an HQ placed)
//   -TCOptionsCheck in the lobby (TCSlice8Checks.cpp): Weather, Sandbox and Remove through the hit test. As host: Weather
//                   forward and back, Sandbox on and off, Add AI, Remove on that AI seat, Remove on the own (connected)
//                   seat (refused here), Weather to fog, Start: the start's weather. As guest: Weather, Sandbox and Remove,
//                   each refused here with nothing sent
//   Slice 9 (TCSlice9Checks.cpp):
//   -TCHostMap=M    with -TCHostStart: pick map M first (the lobby's Map select, server.js 'map')
//   -TCHostWeather=W with -TCHostStart: pick weather W first (the lobby's Weather select, server.js 'weather')
//   -TCCapturePlay  scripted play for a Conquest check: every 4 s the own ground army (selected directly) is split into
//                   groups sent through the right click's move path to the capture points this team does not own,
//                   nearest first. Not a player: the log says what it sent
//   -TCObjectivesLog  once per game second, log the decoded vp and point rows ("objectives: tick ..."), and every
//                   owner change, for comparison with what another recipient was sent
//   -TCPointShotAfter=S  S s into the match, point the camera at a capture point (one being taken, else one held,
//                   else the nearest) and 1.5 s later save a screenshot to -TCPointShotFile (needs a renderer)
//   -TCEpilogueShotFile=path  1.2 s after the epilogue's stamp appears, a screenshot (needs a renderer)
//   -TCResultShotFile=path  1.5 s into the lobby that carries a decided match's result, a screenshot of the result card
//   -TCKeyCheck     once in a match (-TCKeyCheckAfter=S seconds in, default 0; with -TCKeyCheckOnAlert also right
//                   after an alert): Space, Alt+H, Alt+Left, Alt+H, Ctrl+A, '.', Shift+'.', Ctrl+A, Shift+Space, H and F1
//                   twice through simulated key events (ATCPlayerController::InjectKey), each logged with what it did
//   -TCPlanShotAfter=S  S s into the match, select the own army (directly) with the camera on it, and 1.5 s later save a
//                   screenshot of its plan lines and bars to -TCPlanShotFile; the selection is cleared after
//   -TCExitAfterResult=S  quit S s after the lobby that carries a decided match's result
//   Slice 10 (TCSlice10Checks.cpp):
//   -TCPingAfter=S  S s into the match, once: the map ping (client/main.js Alt+left press, pings.send) at the screen's
//                   centre through the controller's world press path when the view is drawn, else at the army's middle
//                   directly (no viewport under -nullrhi)
//   -TCSpawnEnemy=a,b  (sandbox) a list spawns each type in turn, 6 m apart across the line to the centre
//   -TCFxShotAfter=S  S s into the match, once a delivered blast or tracer is alive in this client's effects: point the
//                   camera at it and 0.25 s later save a screenshot to -TCFxShotFile (needs a renderer)
//   -TCLangToggleAfter=S  S s into the match, press the HUD's Language button (the settings' picker) through the hit
//                   test, and 1.5 s later save a screenshot to -TCLangShotFile; the stored setting changes as a player's
//                   would (GameUserSettings.ini [ThreeCrossroads] Language)
//   -TCSettingsShotAfter=S  S s into the match, select the own army (directly) so the Orders and settings buttons show,
//                   and 1.5 s later save a screenshot to -TCSettingsShotFile
//   Slice 11 (TCSlice11Checks.cpp):
//   -TCCloseShotAfter=S  from S s into the match, once a squad drawn with its soldiers and a tank or other vehicle stand
//                   within 30 m of each other with no smoke, dust or fire among them: the camera on the pair at the
//                   closest zoom the game allows, and 0.3 s later a screenshot without the HUD (<file>-nohud.png) and
//                   0.3 s after that one with it to -TCCloseShotFile (tries for 60 s)
//   -TCWorldShotAfter=S  S s into the match: the camera on the spot of the map with the most houses, walls and trees
//                   (within 160 m of the own army when there is one), and 0.6 s later a screenshot to -TCWorldShotFile
//   -TCCombatShotAfter=S  from S s into the match, once smoke, two tracers or shells in flight and an explosion or gun
//                   flash are all alive within 25 m of one spot: the camera on it at 32 m, eight screenshots without the
//                   HUD 0.12 s apart (<file>-1-nohud.png ...) and one with it to -TCCombatShotFile (tries for 90 s, then
//                   takes the best spot seen)
//   -TCSpawnOwn=a,b  (sandbox) once, like -TCSpawnEnemy but for this seat (the 'spawn' command without 'enemy'), 12 m
//                   from the army toward the map's centre, 7 m apart
//   -TCHudShotAfter=S  S s into the match: the own army selected (directly) so the unit panel and its tags show, and
//                   1 s later a screenshot to -TCHudShotFile (the language is -TCLang's)
//   Slice 12 (also TCSlice11Checks.cpp):
//   -TCSpawnOwn now moves each spawn point to the nearest spot at least 9 m from every house, wall or hedge piece,
//                   4 m from every tree or bush and 7 m from every unit (searched out to 40 m), so no sandbox unit
//                   stands in a building
//   -TCMoveOwnAfter=S  S s into the match, once: the own army (spawned units too) ordered to move -TCMoveOwnMetres=D
//                   (default 30) toward the map's centre, as a box selection and a right click would
//   -TCCloseShotMoving  with -TCCloseShotAfter: the vehicle must have moved at least 0.4 m since the last try (0.5 s)
//   -TCCombatShotDamage  with -TCCombatShotAfter: a held ground vehicle with damage smoke (engine-presentation.js
//                   vehicleDamageFrame level over 0.3) must be within 25 m of the spot as well (tries for 90 s, then
//                   takes the best spot seen)
//   -TCWorldShotVisible  with -TCWorldShotAfter: only pieces and trees in cells this seat sees now count (a house
//                   roof or gable 8, a wall 1, a tree 1), and the shot waits (up to 90 s, a try a second) for a spot
//                   with at least one house in sight. Every world shot now deselects, saves <file>-nohud.png first,
//                   then the same view with the HUD
//   Slice 13 (TCSlice13Checks.cpp):
//   -TCTourFile=F -TCTourDir=D  a camera tour for the terrain side by sides: one pose per line of F ("name seconds x z
//                   distance yaw", seconds of match time = tick / 20; x "frontier": where the known world ends toward the map's centre; distance wF: a share
//                   of the widest view), each saved without the HUD to D/name.png 2.5 s after it is set; the log has the
//                   camera, the frame times and the terrain status (unreal/Tools/compare/tour-reference.mjs takes the
//                   browser's half from the same file)
//   -TCWaterShots=D  the water child's fixed poses over the nearest water, the edges and the widest view, saved to D
// Language: -TCLang=en|es for one run (Client/TCLocale.h). Sound: Client/TCAudio.h (no sound under -nosound or without
//   an audio device, as the browser's suspended context; LogTC "audio: play" names each sound that started and its asset)
// The host's -TCHostStart presses the lobby's Mode, Add AI and Start buttons (ATCPlayerController::PressButton).
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/GameModeBase.h"
#include "Net/TCMatchState.h"
#include "Net/TCRustViewShadow.h"
#include "Client/TCUnitTable.h"
#include "Client/TCRecruitment.h"
#include "Client/TCConstruction.h"
#include "Client/TCLogistics.h"
#include "Client/TCSupportOrders.h"
#include "Client/TCAbilities.h"
#include "Client/TCFieldworks.h"
#include "Client/TCSupportPowers.h"
#include "Client/TCCoverPreview.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCPersistence.h"
#include "Client/TCProgression.h"
#include "Client/TCWorldRecovery.h"
#include "Client/TCMarkers.h"
#include "Client/TCOrders.h"
#include "Client/TCMatchOptions.h"
#include "World/TCWorldObjects.h"
#include "World/TCGroundPaint.h"
#include "World/TCRailways.h"
#include "World/TCAtmosphere.h"
#include "World/TCWater.h"
#include "World/TCApron.h"
#include "World/TCFogFrontier.h"
#include "Client/TCAlerts.h"
#include "Client/TCEndReport.h"
#include "Client/TCBossBar.h"
#include "Client/TCKeys.h"
#include "Client/TCEffects.h"
#include "Client/TCWarMap.h"
#include "Client/TCLobbyBackdrop.h"
#include "Client/TCTutorial.h"
#include "Client/TCModelCompare.h"
#include "Client/TCMatchControls.h"
#include "Client/TCRoomBrowser.h"
// round 15 (lobby): the lobby and match controls glue below is declared (Client/TCMatchControls.cpp defines it)
#define TC_HAS_LOBBY_GLUE 1
// round 16 (controls): the stats sheet and its frame probe (Client/TCStats.h); the glue is defined in Client/TCStats.cpp
#include "Client/TCStats.h"
#define TC_HAS_STATS_GLUE 1
#include "Client/TCScenarioText.h"
#include "Net/TCConnection.h"
#include "Net/TCToken.h"
#include "TCGameMode.generated.h"

// round 14: the tutorial and scenario guidance glue below is declared (Client/TCTutorial.cpp defines it)
#define TC_HAS_TUTORIAL_GLUE 1

class FTCConnection;
class FTCTokenStore;
struct FTCUiCommand; // round 15 (lobby): Client/TCPlayerController.h
class FJsonObject;
class ATCTerrain;
class ATCUnitRenderer;
class ATCPlayerController;
class UTCAudio;

// One alert this client raised from its own messages (client/alerts.js push; client/alert-history.js entries): World
// region alerts, supply alerts and the capture point alerts (captured, lost, under attack, cut off). Only what this
// recipient's snapshots delivered: the points are public rows, the regions and supply rows are this side's own.
struct FTCAlertEntry
{
	int32 Id = 0;            // match-local serial (alert-history.js)
	FString Kind;            // 'pointWon', 'pointLost', 'attack', 'region', 'supply'
	FString Text;
	FVector2D At = FVector2D::ZeroVector;
	double Born = 0.0;       // world seconds
	int32 Tick = 0;
};
#if WITH_DEV_AUTOMATION_TESTS
namespace TCSession27Lifecycle { struct FAccess; }
namespace TCSession28Failure { struct FAccess; }
#endif

UCLASS()
class THREECROSSROADS_API ATCGameMode : public AGameModeBase
{
	GENERATED_BODY()

public:
	ATCGameMode();
	virtual void BeginPlay() override;
	virtual void EndPlay(const EEndPlayReason::Type Reason) override;
	virtual void Tick(float DeltaSeconds) override;

	bool SendCommand(const TSharedRef<FJsonObject>& Cmd);
	// Why the last SendCommand returned false (empty after a send that went out), for a log or the HUD.
	const FString& LastSendProblem() const { return LastSendProblemText; }
	FString ConnectionStatus() const;
	// Round 17 (hudui): the connection's retry countdown for #connectionBanner (FTCConnection::RetryLeft; -1: none)
	FTCConnectionNotice ConnectionNotice() const { return Connection ? Connection->Notice() : FTCConnectionNotice(); }
	void SaveMatchView();
	int32 ConnectionRetryLeft(bool* bOutFull = nullptr) const { return Connection ? Connection->RetryLeft(FPlatformTime::Seconds(), bOutFull) : -1; }

	FTCMatchState State;
	// TC_DECODER (default rust): the Rust recipient view decodes every message beside State and is compared after
	// each start and snapshot; under rust it supplies State's unit rows and resync verdict (Net/TCRustViewShadow.h).
	// Declared after State, so it is destroyed first and detaches from it.
	FTCRustViewShadow RustShadow;
	FTCUnitTable UnitTable;
	// What the Command Card needs beyond FTCMatchState (wallet, mode, tech, army limit, queues), fed every start and
	// snapshot next to State
	FTCRecruitment Recruit;
	// The builder card, build orders and placement (fed every start and snapshot; FTCConstruction), and the logistics
	// rows this seat may see with the supply controls (FTCLogistics)
	FTCConstruction Build;
	FTCLogistics Logistics;
	// Slice 1e: assist, repair, site cancel and carriers (FTCSupportOrders), unit abilities with autocast memory
	// (FTCAbilities) and field fortifications with the snapshot's works (FTCFieldworks); fed every start and snapshot
	FTCSupportOrders Support;
	FTCAbilities Abilities;
	FTCFieldworks Field;
	// Slice 1f: off-map support calls and area fire (FTCSupportPowers, fed every start and snapshot) and the cover
	// preview (FTCCoverPreview, started at every start, run by the controller each frame, drawn by the HUD)
	FTCSupportPowers Powers;
	FTCCoverPreview Cover;
	// Slice 1g: the lobby's settings (FTCLobbySettings, fed every lobby), saved matches (FTCPersistence: the lobby's
	// saves panel, the save and load requests and their answers) and the map's objects as this side knows them
	// (FTCWorldObjects: houses, walls, wrecks and the rest as the reference's own structures.js pieces and its props.js
	// and foliage.js scatter (World/TCScatter.h), the house pick, the cover preview's cell states and wrecks; fed every
	// start and snapshot, drawn by FTCWorldObjectsView as instanced pieces). All interim: the reference protocol decoded
	// here until a core-owned client view replaces it.
	FTCLobbySettings Lobby;
	FTCPersistence Persistence;
	FTCWorldObjects WorldObjects;
	FTCWorldObjectsView WorldView;
	// Slice 13 (paint): the painted ground (client/ground.js, the fog of surfaces.js and fog.js, the rubble and crater
	// decals), fed every start and snapshot; the relief material reads it through MF_TCGroundPaint (Bind).
	FTCGroundPaint GroundPaint;
	// Slice 13 water and apron (World/TCWater.h, World/TCApron.h): client/water.js and client/apron.js, fed next to the
	// terrain (ResetFrom after its ResetFrom, MarkCells with its cells, Flush after its Flush).
	FTCWater Water;
	FTCApron Apron;
	// Round 14 (edge): client/fog.js as a whole (World's frontier darkening, the cells under delivered units lit until
	// the next snapshot), the look every fog consumer reads (TCFogLook), the apron's fog texture and World's unknown
	// ground. Started with each start, fed after each snapshot, faded every frame.
	FTCFogFrontier FogFrontier;
	int32 FogFrontierSnapshots = -1;
	// round 14: the paper war map at the widest zoom (client/map-view.js) and the lobby's battlefield (client/lobby-view.js)
	FTCWarMap WarMap;
	FTCLobbyBackdrop Backdrop;
	FString ServerUrl;
	// -TCWaterShots=<dir>: verification captures of the water and the land past the edge (no HUD), then nothing more
	FString WaterShotsDir;
	int32 WaterShotStep = -1;
	double WaterShotAt = -1;
	void WaterShotsTick();
	// Slice 13 lead: frame times while a match is drawn (FApp's delta per tick), the whole match and the last status
	// window; frames in the first 10 s after a start (the whole-map paint and relief build) are kept apart as "settling".
	struct FFrameTimes { int64 N = 0; double Sum = 0, Worst = 0; void Add(double Ms) { N++; Sum += Ms; Worst = FMath::Max(Worst, Ms); } double Mean() const { return N ? Sum / N : 0; } };
	FFrameTimes FramesMatch, FramesWindow, FramesSettling;
	double FramesStartAt = -1;
	int32 FramesStart = -1;
	FString FrameTimesLine() const;
	// Slice 13 verification switch (TCSlice13Checks.cpp): -TCTourFile, -TCTourDir
	void RunSlice13Switches(double Now);
	// round 17 (poses): the line itself is Client/TCModelCompare.h FTCTourPose (TCTour::ParseLine), shared with the tests
	struct FTourPose : FTCTourPose { double Asked = 0; int32 UnitId = -1; bool bTurned = false; double StepSeenAt = -1; };
	TArray<FTourPose> Tour;
	FString TourFile, TourDir;
	int32 TourStep = 0, TourShots = 0;
	double TourShotAt = -1;
	bool bTourRead = false, TourDone = false;
	FFrameTimes TourFrames;
	// Slice 1h: HQ tiers and Armory research (FTCProgression: the panel, availability and the ready announcements, fed
	// every start and snapshot), World Conquest's regions, score, alerts and the Restore card (FTCWorldRecovery, fed every
	// start and snapshot) and the lobby's Weather, Sandbox and seat removal (FTCMatchOptions, fed every lobby). The
	// region alerts' under-attack throttle is one shared FAttackQuiet (client/alerts.js). All interim, as above.
	FTCProgression Progress;
	FTCWorldRecovery WorldRec;
	// round 17 (markers): what the map markers keep between snapshots (the resource nodes, the region tags' busy time)
	FTCMarkers Markers;
	// round 16 (orders): the -TCOrdersCheck= live script
	FTCOrdersCheck OrdersCheck;
	FTCMatchOptions Options;
	TCWorldRecoveryCore::FAttackQuiet AttackQuiet;
	// World region alerts as they arrived, with the world time each was taken, newest last
	TArray<TPair<double, FTCWorldAlert>> RegionAlerts;
	// The army's rally point from the last snapshot (shared/sim.js snapshotFor rally; absent for a spectator)
	bool bArmyRally = false;
	FVector2D ArmyRally = FVector2D::ZeroVector;
	// Logistics alerts as they arrived (createLogisticsAlerts), with the world time each was taken, newest last
	TArray<TPair<double, FTCLogisticsAlert>> SupplyAlerts;
	UPROPERTY() TObjectPtr<ATCTerrain> Terrain;
	UPROPERTY() TObjectPtr<class ATCLighting> Lighting;
	int32 LightingStart = -1; // the StartsReceived the light was set up for
	// slice 13 (railways and atmosphere): client/railways.js on World maps, client/atmosphere.js over every match; each
	// spawns an actor of its own for its meshes (Attach in BeginPlay, Detach in EndPlay)
	FTCRailways Railways;
	FTCAtmosphere Atmosphere;
	int32 AtmosphereStart = -1; // the StartsReceived the atmosphere was started for
	FString HostWeather;       // -TCHostWeather: the lobby's Weather select before the start
	UPROPERTY() TObjectPtr<ATCUnitRenderer> Units;

	// Slice 9 glue (parity gaps 4, 5, 10 and 13). The alert feed and history (client/alerts.js lines: four for 6 s;
	// client/alert-history.js: newest first, 100 kept, reset at a new match), Alt+H's panel and its cursor.
	static constexpr int32 AlertHistoryLimit = 100, AlertLines = 4;
	static constexpr double AlertLineSeconds = 6.0;
	const TArray<FTCAlertEntry>& AlertHistory() const { return Alerts; }
	bool bAlertHistoryOpen = false;
	int32 AlertCursor = 0;
	// client/alerts.js newest: the newest alert still on screen (6 s) with a place. False: none.
	bool NewestAlertAt(FVector2D& Out) const;
	// client/alert-history.js navigate (Step +1 the older, -1 the newer): the visited alert's place.
	bool VisitAlert(int32 Step, FVector2D& Out);
	// The snapshot's own plans by unit id (FTCMatchState::Plans; shared/sim.js planOf kinds), rebuilt every snapshot.
	TMap<int32, int32> PlanKinds;

	// Parity gaps 6 and 7 (Client/TCEffects.h, Client/TCAlerts.h): combat effects from the delivered shots, flights,
	// fires, smokes and falling sections, drawn by Effects; the alerts.js snapshot alerts and the teammates' map pings
	// (AlertFeed), which go to the feed above through PushAlert. Glue in Client/TCAlerts.cpp.
	FTCAlerts AlertFeed;
	FTCStatsOverlay Stats;
	bool bStatsLoaded = false;
	// Round 15 (alerts scope): the lobby's report card (its details open state, Client/TCEndReport.h) and the Horde
	// boss bar (Client/TCBossBar.h), both drawn by ATCHUD from State; the feed above also draws every PushAlert.
	FTCEndReport EndReport;
	FTCBossBar BossBar;
	// Round 15: F2's stats overlay (client/stats.js, saved as 'ww2-stats'; -TCPerf shows every number, as '?perf') and
	// its numbers (client/perf.js: snapshots, latency pings, frames), Client/TCKeys.h
	FTCPerf Perf;
	FTCStatsOverlay& StatsOverlay()
	{
		if (!bStatsLoaded) { bStatsLoaded = true; Stats.Load(FTCSettingsStore::Game().Get(TEXT("ww2-stats")), FParse::Param(FCommandLine::Get(), TEXT("TCPerf"))); }
		return Stats;
	}
	// stats.toggle(): show or hide, then save()
	void ToggleStats()
	{
		StatsOverlay().Toggle();
		if (const TOptional<FString> Saved = Stats.SaveText()) FTCSettingsStore::Game().Set(TEXT("ww2-stats"), *Saved);
	}
	// Round 16 (controls): the stats sheet (Menu > Stats overlay, Client/TCStats.h), modal while open: a press goes to
	// StatsSheetPress (a control, the sheet, or the backdrop that closes it); StatsSheetTag presses a control by its tag
	// (verification steps). The probe logs the overlay's frame numbers beside 'stat unit' with -TCStatUnitProbe.
	FTCStatsSheet StatsSheet;
	FTCStatUnitProbe StatUnitProbe;
	bool StatsSheetPress(FVector2D Pixel, bool bBack);
	bool StatsSheetTag(const FString& Tag, bool bBack);
	// Round 14, tutorial and scenario guidance (Client/TCTutorial.h, Client/TCScenarioText.h; the glue is defined in
	// Client/TCTutorial.cpp): the goal box, the collapse banner and the damaged buildings' smoke (objectives.js), the
	// Tutorial's progress as the snapshots tell it, the Controls sheet at the first match (CLI-127), and the lobby's
	// Tutorial button (a new private Tutorial room).
	FTCScenarioText ScenarioText;
	FTCTutorialProgress Tutorial;
	FTCControlsOnboarding Onboarding;
	void EnterTutorialRoom();
	// Round 15, lobby and match controls (Client/TCMatchControls.h, Client/TCRoomBrowser.h; the glue is defined in
	// Client/TCMatchControls.cpp): Watch as a spectator / Take a seat, the Name field, the invite link and Copy, the Room
	// box, the match menu (Pause, Save, Restart, End, Hand <name> to AI, Leave), Send next wave, the sandbox panel, and
	// the room directory (public-lobby.js) inside the client.
	FTCMatchControls Controls;
	FTCRoomBrowser Rooms;
	FTCTextFields Form;
	bool bRoomBrowserOpen = false;
	TArray<FTCControlRow> LobbyControlRows() const;
	TArray<FTCControlRow> BrowserRows() const;
	TArray<FTCControlRow> MatchControlRows() const;
	// A click on a MatchControl button (FTCUiCommand::Unit names it, Group the seat; bBack: a right click)
	bool RunControl(const FTCUiCommand& Command, bool bBack);
	// A key while a text field is typed in: true when taken
	bool ControlsKey(const FKey& Key, bool bPressed, bool bShift);
	// A map click with the sandbox spawn aim armed
	bool ControlsSpawnAt(FVector2D Ground);
	UPROPERTY() TObjectPtr<ATCEffects> Effects;
	// Parity gap 9 (Client/TCAudio.h): the world's sound subsystem, null when there is no game world.
	UTCAudio* Audio() const;
	// client/pings.js send: { t: 'ping', x, z } (the server relays it to this team's other players). False when not
	// sent (no match, a spot that is not a number, the socket refused it).
	bool SendPing(FVector2D At, const TCHAR* How);
	// Shooter-side effects whose shooter was not in that snapshot's held rows (client/fx.js privacy rule): counted
	// after each snapshot, logged with the status line; must stay 0.
	int32 FxPrivacyChecked = 0, FxPrivacyViolations = 0;

private:
#if WITH_DEV_AUTOMATION_TESTS
	friend struct FTCRows30Access;
#endif
#if WITH_DEV_AUTOMATION_TESTS
	friend struct TCSession27Lifecycle::FAccess;
	friend struct TCSession28Failure::FAccess;
#endif
	void PushAlert(const TCHAR* Kind, const FString& Text, FVector2D At);
	// client/alerts.js snapshot, the points block: captured, lost, under attack (the shared throttle) and cut off,
	// from this snapshot's point rows against the last ones
	void TakePointAlerts();
	TArray<FTCAlertEntry> Alerts;
	int32 AlertSerial = 0;
	TArray<FTCPointRow> PrevPoints;
	bool bHasPrevPoints = false;
	// Hooks the controller's keys call (Space, Alt+H, Alt+arrows, the idle keys), wired once a controller exists
	void WireController(ATCPlayerController* PC);
	TWeakObjectPtr<ATCPlayerController> WiredController;
	// Parity gaps 6 and 7 glue (defined in Client/TCAlerts.cpp): each start, snapshot and ping relay, and the clear
	void OnCombatStart();
	void OnCombatSnapshot(const FJsonObject& M);
	void OnPingRelay(const FJsonObject& M);
	void ClearCombat();
	// Parity gap 9 glue (TCSlice10Checks.cpp): the listener, beds and delayed effect sounds every frame
	void AudioFrame(float DeltaSeconds, ATCPlayerController* PC);
	// Slice 10 verification switches (TCSlice10Checks.cpp)
	void RunSlice10Switches(double Now);
	void RunSlice11Switches(double Now);
	double CombatShotAfter = -1, CombatShotAt = -1, CombatShotTried = -1, CloseShotUiAt = -1;
	bool bCombatShot = false, bSpawnedOwn = false;
	int32 CombatFrames = 0;
	FString CombatShotFile, SpawnOwn;
	double CloseShotAfter = -1, CloseShotAt = -1, WorldShotAfter = -1, WorldShotAt = -1, HudShotAfter = -1, HudShotAt = -1, CloseShotTried = -1;
	bool bCloseShot = false, bWorldShot = false, bHudShot = false;
	FString CloseShotFile, WorldShotFile, HudShotFile;
	// slice 12 (TCSlice11Checks.cpp): -TCCloseShotMoving, -TCCombatShotDamage, -TCWorldShotVisible, -TCMoveOwnAfter
	bool bCloseShotMoving = false, bCombatShotDamage = false, bWorldShotVisible = false, bMovedOwn = false, bWorldBareTaken = false;
	double MoveOwnAfter = -1, MoveOwnMetres = 30, WorldShotTried = -1;
	TMap<int32, FVector2D> CloseLastAt; // each vehicle's place at the previous close shot try (-TCCloseShotMoving)
	double PingAfter = -1, FxShotAfter = -1, FxShotAt = -1, LangToggleAfter = -1, LangShotAt = -1, SettingsShotAfter = -1, SettingsShotAt = -1;
	bool bPingSent = false, bFxShot = false, bLangToggled = false, bLangShot = false, bSettingsShot = false;
	FString FxShotFile, LangShotFile, SettingsShotFile;
	int32 ShooterLogSeen = 0;
	// The epilogue's camera glide and slowed screen time (FTCObjectives::EpilogueFrame) and its sounds (UTCAudio::Cues)
	float EpilogueFrame(float DeltaSeconds, ATCPlayerController* PC);
	// Slice 9 verification switches (TCSlice9Checks.cpp)
	void RunSlice9Switches(double Now);
	void LogObjectives(bool bForce);
	FString HostMap;
	bool bCapturePlay = false, bObjectivesLog = false, bKeyCheck = false, bKeyScriptMade = false, bHostMapSent = false;
	double CapturePlayNext = -1, EpilogueSeenAt = -1, ResultLobbyAt = -1, PointShotAfter = -1, PointShotAt = -1, PlanShotAt = 0, PlanClearAt = -1;
	double LastObjectivesLogSecond = -1;
	FString EpilogueShotFile, ResultShotFile, PointShotFile;
	bool bEpilogueShot = false, bResultShot = false, bPointShot = false, bEpilogueLogged = false;
	int32 LastLoggedOwners = INT32_MIN;

	void OnMessage(const FJsonObject& M, int32 Bytes);
	void SpawnScene();
	void RunVerificationSwitches(double Now);
	// The room went back to the lobby: nothing of the match stays drawn or selected.
	void ClearMatchPresentation();
	// Round 14: BeginPlay's connection (token, socket, decoder hooks) as a call, so the Tutorial button can open a new
	// room; BaseSettings and BaseTokenRequest keep the command line's (no token).
	bool Connect(FTCConnection::FSettings Settings, const FTCTokenRequest& Request);
	FTCConnection::FSettings BaseSettings;
	FTCTokenRequest BaseTokenRequest;
	void TutorialSwitches(const TCHAR* Cmd);
	void TutorialStart();
	void TutorialSnapshot(const FJsonObject& M);
	void TutorialFrame(float Dt);
	void TutorialClear();
	FTCTutorialDriver TutorialDriver;
	// Round 15 (lobby): see Controls above
	void ControlsSwitches(const TCHAR* Cmd, FTCConnection::FSettings& Settings);
	void ControlsMessage(const FString& T, const FJsonObject& M);
	void ControlsOpened();
	void ControlsFrame();
	bool ControlsSend(const FTCControlRequest& Request);
	void ControlsCommitField(ETCTextField Which, bool bCancelled);
	void ControlsGoRoom(const FTCControlRequest& Request);
	bool ControlsEnter(const FTCRoomEntry& Entry, bool bKeepSeat);
	FString ControlsRoom() const;
	bool bControlsDeferConnect = false, bQuickPlayPending = false, bControlsShift = false;
	int32 QuickPlayAnswers = 0, ControlsStep = 0;
	double ControlsStepAt = -1.0;
	FString NoticeText, ControlsShotDir;
	TArray<FTCControlsStep> ControlsScript;
	bool bTutorialDrive = false, bTutorialEndShot = false, bTutorialButtonPressed = false, bTutorialLobbyShot = false, bTutorialOnboardShot = false, bTutorialJumped = false;
	FString TutorialShotDir, TutorialShotTag;
	int32 TutorialShotStep = -2, TutorialPlumeBursts = 0;
	double TutorialShotAt = -1.0, TutorialStartAt = -1.0, TutorialEndAt = -1.0, TutorialLobbyAt = -1.0, TutorialButtonAfter = 0.0;

	TSharedPtr<FTCConnection> Connection;
	TSharedPtr<FTCTokenStore> TokenStore;   // holds the token file's lock while this game mode lives
	FString SetupError, LastSendProblemText;
	double BeganAt = 0, LastPing = 0, LastResync = -10, LastReport = 0, MatchSeenAt = -1, Rtt = -1;
	int32 HostStartAi = -1, StartsReceived = 0;
	bool bHostStarted = false, bAutoMove = false, bAutoMoved = false, bDropped = false, bEndSent = false;
	double DropAfter = -1, ExitAfter = -1, ShotAfter = -1, EndAfter = -1;
	FString ShotFile, HostMode;
	TArray<int32> AutoIds;
	FVector2D AutoTarget;
	double AutoMovedAt = -1;
	int32 AutoReports = 0;
	// attack, attack-move, stop, sandbox spawn, refusal probe and lobby screenshot switches
	bool bAutoAttackMove = false, bHostSandbox = false, bAutoAttack = false, bAutoAttacked = false, bProbeDeny = false, bProbeSent = false;
	bool bSpawned = false, bStopSent = false;
	double AutoStopAfter = -1, StopSentAt = -1, AttackAt = -1, LobbySeenAt = -1, LobbyShotAfter = -1;
	int32 AttackTarget = 0, AttackReports = 0, StopReports = 0, DeniesAtProbe = 0;
	TArray<int32> AttackIds;
	TMap<int32, FVector2D> StopFrom;
	FString SpawnEnemy, LobbyShotFile;
	float StartZoom = -1;
	void RunHostLobby(double Now);
	bool bHostStartPending = false;
	double HostStartAskedAt = -1;
	bool bAttackWithTrucks = false;
	TArray<int32> ArmyIds(double& OutX, double& OutZ, bool bTrucks = false) const;
	// recruit, buy refusal, control group and minimap switches
	void RunSlice3Switches(double Now);
	FString MinimapReport() const;
	int32 OwnCount(const FString& Type) const;
	TArray<FString> RecruitTypes;
	bool bRecruitShift = false;
	int32 RecruitIndex = 0, RecruitPhase = 0, RecruitReports = -1, RecruitCountBefore = 0, RecruitDeniesBefore = 0;
	double RecruitNextAt = -1, RecruitPressedAt = -1, RecruitMpBefore = 0;
	FString SelectType, HostArmy;
	bool bSelectedType = false, bCancelQueued = false, bCancelSent = false;
	double CancelAt = -1, CancelMpBefore = 0;
	int32 CancelBuilding = 0, CancelQueueBefore = 0;
	TArray<FString> ProbeBuyDeny;
	int32 ProbeBuyIndex = 0;
	int32 ProbeBuyDenies = 0;
	double ProbeBuyAt = -1;
	// construction, army command and logistics switches (slice 1d)
	void RunSlice4Switches(double Now);
	FString SupplyReport() const;
	TArray<int32> OwnOfTypes(const TArray<FString>& Types) const;
	// A spot near Around where the client's placement check accepts Kind (rings outward 4 m apart), or false
	bool FindPlacement(const FString& Kind, FVector2D Around, FVector2D& Out) const;
	// The right click on a held unit as the mouse would make it: the screen pick at its drawn point when it is drawn
	// (else the unit named directly, with its ground), then ATCPlayerController::IssueOrder
	int32 RightClickUnit(int32 Id, const TCHAR* What);
	int32 HostSeats = 0;
	double AutoMoveMetres = 20.0;
	TArray<int32> HostTeams;
	FString BuildCheckKind, BuildShotFile, SupplyShotFile, SupplyTo;
	bool bBuildProbe = false, bArmyCheck = false, bSupplyCheck = false;
	bool bBuildScriptMade = false, bArmyScriptMade = false, bSupplyScriptMade = false, bSupplyToSent = false;
	FVector2D BuildAt = FVector2D(-1, -1), BuildFar = FVector2D(-1, -1);
	double BuildClickAt = -1, BuildMpBefore = 0, SupplyToAt = -1;
	int32 BuildReports = 0, BuildCountBefore = 0, BuildDeniesBefore = 0, SupplyToTarget = 0, SupplyToReports = 0;
	bool bBuildProbeSent = false;
	bool bLastSnapLogistics = false; // the last snapshot carried a 'logistics' field (it is absent for a spectator)
	// support order, ability and fieldworks switches (slice 1e), defined in TCSlice5Checks.cpp
	void RunSlice5Switches(double Now);
	// formation, support power and cover preview switches (slice 1f), defined in TCSlice6Checks.cpp
	void RunSlice6Switches(double Now);
	bool bFormationCheck = false, bPowersCheck = false, bCoverCheck = false;
	// lobby settings, save and load, house garrison switches (slice 1g), defined in TCSlice7Checks.cpp
	void RunSlice7Switches(double Now);
	struct FWaitStep { FString What; TFunction<bool()> Ready; TFunction<void()> Run; double Timeout = 10.0; };
	TArray<FWaitStep> WaitSteps;
	double WaitStepSince = -1;
	uint64 WaitStepFrame = 0;
	bool bGarrisonSpawnSent = false;
	bool bLobbyCheck = false, bSaveCheck = false, bGarrisonCheck = false, bLobbyScriptMade = false, bSaveScriptMade = false, bGarrisonScriptMade = false;
	FString LobbyCheckMode, GarrisonShotFile;
	// research, World recovery and match options switches (slice 1h), defined in TCSlice8Checks.cpp
	void RunSlice8Switches(double Now);
	// After each snapshot: research announcements and World region alerts to the feedback line (alerts kept in RegionAlerts)
	void TakeSlice8Notices();
	bool SpotOnScreen(double X, double Z) const;
	bool bResearchCheck = false, bRecoverCheck = false, bOptionsCheck = false, bResearchScriptMade = false, bRecoverScriptMade = false, bOptionsScriptMade = false;
	bool bFormationScriptMade = false, bPowersScriptMade = false, bCoverScriptMade = false, bPowersSpawnSent = false;
	FString FormationShotFile, PowersShotFile, CoverShotFile;
	// Sends what FTCAbilities::AdoptAutocast returns after a snapshot (client/autocast.js adopt)
	void AdoptAutocast();
	bool bSupportCheck = false, bAbilityCheck = false, bFieldCheck = false, bLetterCheck = false, bLetterScriptMade = false;
	bool bSupportScriptMade = false, bAbilityScriptMade = false, bFieldScriptMade = false, bSupportSiteSent = false;
	FString SupportBuild, FieldShotFile;
	double SupportSiteAt = -1;
	FVector2D SupportSite = FVector2D(-1, -1);
	struct FScriptStep { double At; FString What; TFunction<void()> Run; };
	bool bGroupCheck = false, bMinimapCheck = false, bGroupScriptMade = false, bMinimapScriptMade = false;
	TArray<FScriptStep> Script;
	double ScriptT0 = -1;
	uint64 ScriptLastFrame = 0;
};
