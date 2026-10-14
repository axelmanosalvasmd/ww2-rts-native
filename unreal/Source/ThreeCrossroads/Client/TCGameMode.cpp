#include "Client/TCGameMode.h"
#include "RHIStats.h"
#include "Client/TCGraphics.h"
#include "Client/TCCameraPawn.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "Client/TCOrders.h"
#include "Client/TCArmyCommands.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCAudio.h"
#include "Client/TCLighting.h"
#include "World/TCTerrain.h"
#include "Net/TCConnection.h"
#include "Net/TCToken.h"
#include "Dom/JsonValue.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "Misc/ConfigCacheIni.h"
#include "HAL/PlatformTime.h"
#include "Misc/App.h"
#include "CoreGlobals.h"
#include "UnrealClient.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"

ATCGameMode::ATCGameMode()
{
	PrimaryActorTick.bCanEverTick = true;
	DefaultPawnClass = ATCCameraPawn::StaticClass();
	PlayerControllerClass = ATCPlayerController::StaticClass();
	HUDClass = ATCHUD::StaticClass();
}

void ATCGameMode::SpawnScene()
{
	UWorld* W = GetWorld();
	FActorSpawnParameters P;
	P.SpawnCollisionHandlingOverride = ESpawnActorCollisionHandlingMethod::AlwaysSpawn;
	// the reference's light, haze, sky and tone mapping (Client/TCLighting.h) in place of the engine-default sun, sky
	// atmosphere, real-time sky light and auto exposure
	Lighting = W->SpawnActor<ATCLighting>(ATCLighting::StaticClass(), FTransform::Identity, P);
	Terrain = W->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity, P);
	Units = W->SpawnActor<ATCUnitRenderer>(ATCUnitRenderer::StaticClass(), FTransform::Identity, P);
}

void ATCGameMode::BeginPlay()
{
	Super::BeginPlay();
	BeganAt = FPlatformTime::Seconds();
	FString Error;
	if (!UnitTable.Load(Error)) { UE_LOG(LogTC, Error, TEXT("unit table: %s"), *Error); }
	else UE_LOG(LogTC, Log, TEXT("unit table: %d types from reference %s"), UnitTable.Num(), *UnitTable.Reference);
	// the recruit table is compiled in from one reference commit: purchases stay refused unless units.json is from it
	Recruit.SetUnitTableReference(UnitTable.Reference);
	if (!Recruit.ReferenceMatches()) UE_LOG(LogTC, Error, TEXT("recruit data is for reference %s, the unit table for %s: recruiting is refused"), FTCRecruitment::ReferencePin, *UnitTable.Reference);
	// the construction table is generated from one reference commit too: building stays refused unless they agree
	Build.SetUnitTableReference(UnitTable.Reference);
	if (!Build.ReferenceMatches()) UE_LOG(LogTC, Error, TEXT("construction data is for reference %s, the unit table for %s: building is refused"), FTCConstruction::ReferencePin, *UnitTable.Reference);
	// the fieldworks tables are written out from one reference commit: digging stays refused unless they agree (the
	// ability and support order modules compare the unit table's reference themselves on every check)
	Field.SetUnitTableReference(UnitTable.Reference);
	if (!Field.ReferenceMatches()) UE_LOG(LogTC, Error, TEXT("fieldworks data is for reference %s, the unit table for %s: digging is refused"), FTCFieldworks::ReferencePin, *UnitTable.Reference);
	// the tech costs are written out from one reference commit too: research stays refused unless they agree
	Progress.SetUnitTableReference(UnitTable.Reference);
	if (!Progress.ReferenceMatches()) UE_LOG(LogTC, Error, TEXT("research data is for reference %s, the unit table for %s: research is refused"), FTCProgression::ReferencePin, *UnitTable.Reference);
	if (UnitTable.Reference != FTCAbilities::ReferencePin) UE_LOG(LogTC, Error, TEXT("ability data is for reference %s, the unit table for %s: abilities are refused"), FTCAbilities::ReferencePin, *UnitTable.Reference);
	if (UnitTable.Reference != FTCSupportPowers::ReferencePin) UE_LOG(LogTC, Error, TEXT("support data is for reference %s, the unit table for %s: support calls and area fire are refused"), FTCSupportPowers::ReferencePin, *UnitTable.Reference);
	// client/cover-preview.js: on unless the stored setting says '0' (localStorage 'ww2-cover'; here the user settings)
	bool bCoverOn = true;
	if (GConfig) GConfig->GetBool(TEXT("ThreeCrossroads"), TEXT("CoverPreview"), bCoverOn, GGameUserSettingsIni);
	Cover.SetEnabled(bCoverOn);
	UE_LOG(LogTC, Log, TEXT("cover preview %s (reference %s)"), bCoverOn ? TEXT("on") : TEXT("off"), FTCCoverPreview::ReferencePin);
	SpawnScene();
	// the map's objects: one procedural mesh actor of its own (FTCWorldObjectsView), cleared with each match
	if (!WorldView.Attach(GetWorld())) UE_LOG(LogTC, Error, TEXT("world objects: the mesh actor could not be spawned; houses and wrecks are not drawn"));
	if (!GroundPaint.Attach(GetWorld())) UE_LOG(LogTC, Error, TEXT("ground paint: the decal actor could not be spawned; rubble and crater marks are not drawn"));
	UE_LOG(LogTC, Log, TEXT("ground paint: reference %s"), FTCGroundPaint::ReferencePin);
	if (!Railways.Attach(GetWorld())) UE_LOG(LogTC, Error, TEXT("railways: the mesh actor could not be spawned; no railway is drawn"));
	if (!Atmosphere.Attach(GetWorld())) UE_LOG(LogTC, Error, TEXT("atmosphere: the mesh actor could not be spawned; no clouds, mist, rain or snow are drawn"));
	Atmosphere.SetRailways(&Railways);
	// slice 13: the water and the apron drape on the drawn ground (main.js hAt)
	Water.Attach(GetWorld());
	Apron.Attach(GetWorld());
	Water.GroundHeight = [this](float X, float Z) { return Terrain ? Terrain->HeightAt(X, Z) : 0.f; };
	Apron.GroundHeight = Water.GroundHeight;
	Apron.Objects = &WorldObjects;
	FParse::Value(FCommandLine::Get(), TEXT("TCWaterShots="), WaterShotsDir);
	// slice 13 lead glue: the cloud shade and the wet ground cover the apron too (atmosphere.js cloudApron, coverApron)
	if (AActor* ApronActor = Apron.HolderActor()) Atmosphere.AddGroundActor(ApronActor);
	// round 14 (edge): fog.js's state for every fog consumer, the apron's fog texture, World's unknown ground
	// -TCFogFrontierOff: the slice 13 fog (three looks from the mask, no unknown ground) for comparisons
	if (!FParse::Param(FCommandLine::Get(), TEXT("TCFogFrontierOff")))
	{
		FogFrontier.Attach(GetWorld());
		TCFogLook::SetActive(&FogFrontier);
		Apron.SetFogSource(&FogFrontier);
		UE_LOG(LogTC, Log, TEXT("fog frontier: reference %s"), FTCFogFrontier::ReferencePin);
	}
	else UE_LOG(LogTC, Log, TEXT("fog frontier: off (-TCFogFrontierOff): the three looks from the mask"));
	UE_LOG(LogTC, Log, TEXT("world objects, lobby settings and saves: reference %s / %s / %s"), FTCWorldObjects::ReferencePin, FTCLobbySettings::ReferencePin, FTCPersistence::ReferencePin);

	const TCHAR* Cmd = FCommandLine::Get();
	FTCConnection::FSettings Settings;
	Settings.Url = TEXT("ws://127.0.0.1:3000/ws");
	Settings.Room = TEXT("native");
	Settings.Name = TEXT("Commander");
	FParse::Value(Cmd, TEXT("TCServer="), Settings.Url);
	ServerUrl = Settings.Url; // round 14: the lobby backdrop fetches /maps/<name> from this server
	FParse::Value(Cmd, TEXT("TCRoom="), Settings.Room);
	// round 15 (lead): -TCInvite=<link> opens an invite link as the browser opens <origin>/play#<room>[&seat=..]: the
	// link's origin gives the socket (room-session.js connects to location.host/ws), its hash the room and the seat
	{
		FString Invite;
		if (FParse::Value(Cmd, TEXT("TCInvite="), Invite, false) && !Invite.IsEmpty())
		{
			FString InviteUrl;
			FTCRoomAddress InviteAt;
			if (FTCRoomBrowser::ParseInviteLink(Invite, InviteUrl, InviteAt))
			{
				Settings.Url = ServerUrl = InviteUrl;
				Settings.Room = InviteAt.Room;
				UE_LOG(LogTC, Log, TEXT("invite link: %s -> socket %s, room %s%s"), *Invite, *InviteUrl, *InviteAt.Room, InviteAt.Seat.IsEmpty() ? TEXT("") : *(TEXT(", seat ") + InviteAt.Seat));
			}
			else UE_LOG(LogTC, Warning, TEXT("invite link: '%s' is not an invite link (<origin>/play#<room>); ignored"), *Invite);
		}
	}
	FParse::Value(Cmd, TEXT("TCName="), Settings.Name);
	Settings.bSpectate = FParse::Param(Cmd, TEXT("TCSpectate"));
	FTCTokenRequest TokenRequest;
	FParse::Value(Cmd, TEXT("TCSeat="), TokenRequest.Seat);
	FParse::Value(Cmd, TEXT("TCTokenFile="), TokenRequest.TokenFile);
	FString Refused;
	const bool bTokenOnCommandLine = FParse::Value(Cmd, TEXT("TCToken="), Refused);
	Refused.Reset();
	FParse::Value(Cmd, TEXT("TCHostStart="), HostStartAi);
	FParse::Value(Cmd, TEXT("TCHostMode="), HostMode);
	FParse::Value(Cmd, TEXT("TCEndAfter="), EndAfter);
	bAutoMove = FParse::Param(Cmd, TEXT("TCAutoMove"));
	bAutoAttackMove = FParse::Param(Cmd, TEXT("TCAutoAttackMove"));
	bAutoAttack = FParse::Param(Cmd, TEXT("TCAutoAttack"));
	bHostSandbox = FParse::Param(Cmd, TEXT("TCHostSandbox"));
	bProbeDeny = FParse::Param(Cmd, TEXT("TCProbeDeny"));
	bAttackWithTrucks = FParse::Param(Cmd, TEXT("TCAttackWithTrucks"));
	FParse::Value(Cmd, TEXT("TCAutoStopAfter="), AutoStopAfter);
	FParse::Value(Cmd, TEXT("TCSpawnEnemy="), SpawnEnemy, false); // a list: do not stop at the comma
	FParse::Value(Cmd, TEXT("TCStartZoom="), StartZoom);
	FParse::Value(Cmd, TEXT("TCLobbyShotAfter="), LobbyShotAfter);
	FString RecruitList;
	if (FParse::Value(Cmd, TEXT("TCRecruit="), RecruitList, false)) RecruitList.ParseIntoArray(RecruitTypes, TEXT(",")); // a list: do not stop at the comma
	bRecruitShift = FParse::Param(Cmd, TEXT("TCRecruitShift"));
	FString ProbeList;
	if (FParse::Value(Cmd, TEXT("TCProbeBuyDeny="), ProbeList, false)) ProbeList.ParseIntoArray(ProbeBuyDeny, TEXT(","));
	bGroupCheck = FParse::Param(Cmd, TEXT("TCGroupCheck"));
	FParse::Value(Cmd, TEXT("TCSelect="), SelectType);
	FParse::Value(Cmd, TEXT("TCHostArmy="), HostArmy);
	bCancelQueued = FParse::Param(Cmd, TEXT("TCCancelQueued"));
	bMinimapCheck = FParse::Param(Cmd, TEXT("TCMinimapCheck"));
	FParse::Value(Cmd, TEXT("TCHostSeats="), HostSeats);
	FParse::Value(Cmd, TEXT("TCAutoMoveMetres="), AutoMoveMetres);
	FString TeamList;
	if (FParse::Value(Cmd, TEXT("TCHostTeams="), TeamList, false))
	{
		TArray<FString> Parts;
		TeamList.ParseIntoArray(Parts, TEXT(","));
		for (const FString& T : Parts) HostTeams.Add(FCString::Atoi(*T));
	}
	FParse::Value(Cmd, TEXT("TCBuildCheck="), BuildCheckKind);
	FParse::Value(Cmd, TEXT("TCBuildShotFile="), BuildShotFile);
	bBuildProbe = FParse::Param(Cmd, TEXT("TCBuildProbe"));
	bArmyCheck = FParse::Param(Cmd, TEXT("TCArmyCheck"));
	bSupplyCheck = FParse::Param(Cmd, TEXT("TCSupplyCheck"));
	FParse::Value(Cmd, TEXT("TCSupplyShotFile="), SupplyShotFile);
	FParse::Value(Cmd, TEXT("TCSupplyTo="), SupplyTo);
	bSupportCheck = FParse::Param(Cmd, TEXT("TCSupportCheck"));
	FParse::Value(Cmd, TEXT("TCSupportBuild="), SupportBuild);
	bAbilityCheck = FParse::Param(Cmd, TEXT("TCAbilityCheck"));
	bFieldCheck = FParse::Param(Cmd, TEXT("TCFieldCheck"));
	bLetterCheck = FParse::Param(Cmd, TEXT("TCLetterCheck"));
	FParse::Value(Cmd, TEXT("TCFieldShotFile="), FieldShotFile);
	bFormationCheck = FParse::Param(Cmd, TEXT("TCFormationCheck"));
	bPowersCheck = FParse::Param(Cmd, TEXT("TCPowersCheck"));
	bCoverCheck = FParse::Param(Cmd, TEXT("TCCoverCheck"));
	FParse::Value(Cmd, TEXT("TCFormationShotFile="), FormationShotFile);
	FParse::Value(Cmd, TEXT("TCPowersShotFile="), PowersShotFile);
	FParse::Value(Cmd, TEXT("TCCoverShotFile="), CoverShotFile);
	bLobbyCheck = FParse::Param(Cmd, TEXT("TCLobbyCheck"));
	bSaveCheck = FParse::Param(Cmd, TEXT("TCSaveCheck"));
	bGarrisonCheck = FParse::Param(Cmd, TEXT("TCGarrisonCheck"));
	bResearchCheck = FParse::Param(Cmd, TEXT("TCResearchCheck"));
	bRecoverCheck = FParse::Param(Cmd, TEXT("TCRecoverCheck"));
	bOptionsCheck = FParse::Param(Cmd, TEXT("TCOptionsCheck"));
	if (!FParse::Value(Cmd, TEXT("TCLobbyCheckMode="), LobbyCheckMode)) LobbyCheckMode = TEXT("conquest");
	// slice 9 switches (TCSlice9Checks.cpp)
	FParse::Value(Cmd, TEXT("TCHostMap="), HostMap);
	FParse::Value(Cmd, TEXT("TCHostWeather="), HostWeather);
	bCapturePlay = FParse::Param(Cmd, TEXT("TCCapturePlay"));
	bObjectivesLog = FParse::Param(Cmd, TEXT("TCObjectivesLog"));
	bKeyCheck = FParse::Param(Cmd, TEXT("TCKeyCheck"));
	FParse::Value(Cmd, TEXT("TCPointShotAfter="), PointShotAfter);
	FParse::Value(Cmd, TEXT("TCPointShotFile="), PointShotFile);
	FParse::Value(Cmd, TEXT("TCEpilogueShotFile="), EpilogueShotFile);
	FParse::Value(Cmd, TEXT("TCResultShotFile="), ResultShotFile);
	FParse::Value(Cmd, TEXT("TCGarrisonShotFile="), GarrisonShotFile);
	// slice 10 switches (TCSlice10Checks.cpp)
	FParse::Value(Cmd, TEXT("TCPingAfter="), PingAfter);
	FParse::Value(Cmd, TEXT("TCFxShotAfter="), FxShotAfter);
	FParse::Value(Cmd, TEXT("TCFxShotFile="), FxShotFile);
	FParse::Value(Cmd, TEXT("TCLangToggleAfter="), LangToggleAfter);
	FParse::Value(Cmd, TEXT("TCLangShotFile="), LangShotFile);
	FParse::Value(Cmd, TEXT("TCSettingsShotAfter="), SettingsShotAfter);
	FParse::Value(Cmd, TEXT("TCSettingsShotFile="), SettingsShotFile);
	// slice 11 switches (TCSlice11Checks.cpp)
	FParse::Value(Cmd, TEXT("TCCloseShotAfter="), CloseShotAfter);
	FParse::Value(Cmd, TEXT("TCCloseShotFile="), CloseShotFile);
	FParse::Value(Cmd, TEXT("TCWorldShotAfter="), WorldShotAfter);
	FParse::Value(Cmd, TEXT("TCTourFile="), TourFile);
	FParse::Value(Cmd, TEXT("TCTourDir="), TourDir);
	FParse::Value(Cmd, TEXT("TCWorldShotFile="), WorldShotFile);
	FParse::Value(Cmd, TEXT("TCSpawnOwn="), SpawnOwn, false); // a list: do not stop at the comma
	FParse::Value(Cmd, TEXT("TCCombatShotAfter="), CombatShotAfter);
	FParse::Value(Cmd, TEXT("TCCombatShotFile="), CombatShotFile);
	FParse::Value(Cmd, TEXT("TCHudShotAfter="), HudShotAfter);
	FParse::Value(Cmd, TEXT("TCHudShotFile="), HudShotFile);
	// slice 12 switches (TCSlice11Checks.cpp)
	bCloseShotMoving = FParse::Param(Cmd, TEXT("TCCloseShotMoving"));
	bCombatShotDamage = FParse::Param(Cmd, TEXT("TCCombatShotDamage"));
	bWorldShotVisible = FParse::Param(Cmd, TEXT("TCWorldShotVisible"));
	FParse::Value(Cmd, TEXT("TCMoveOwnAfter="), MoveOwnAfter);
	FParse::Value(Cmd, TEXT("TCMoveOwnMetres="), MoveOwnMetres);
	if (!FParse::Value(Cmd, TEXT("TCLobbyShotFile="), LobbyShotFile)) LobbyShotFile = FPaths::Combine(FPaths::ProjectSavedDir(), TEXT("Screenshots/tc-lobby.png"));
	FParse::Value(Cmd, TEXT("TCDropAfter="), DropAfter);
	FParse::Value(Cmd, TEXT("TCExitAfter="), ExitAfter);
	FParse::Value(Cmd, TEXT("TCShotAfter="), ShotAfter);
	if (!FParse::Value(Cmd, TEXT("TCShotFile="), ShotFile)) ShotFile = FPaths::Combine(FPaths::ProjectSavedDir(), TEXT("Screenshots/tc-slice.png"));
	// round 14: -TCTutorial is the lobby's Tutorial button from the command line (a new private Tutorial room)
	if (TCTutorialEntry::FromCommandLine(Cmd, Settings.Room, Settings.Listing)) UE_LOG(LogTC, Log, TEXT("tutorial: -TCTutorial, a new private Tutorial room %s (hello carries the listing)"), *Settings.Room);
	TutorialSwitches(Cmd);
	ControlsSwitches(Cmd, Settings); // round 15: the name, the hello's listing, the room's lowercase, -TCBrowse
	{
		// round 16 (orders): the -TCOrdersCheck= live script (Client/TCOrders.h FTCOrdersCheck)
		FString OrdersScript, OrdersShots;
		FParse::Value(Cmd, TEXT("TCOrdersShotDir="), OrdersShots);
		if (FParse::Value(Cmd, TEXT("TCOrdersCheck="), OrdersScript, false) && OrdersCheck.Parse(OrdersScript, OrdersShots))
			UE_LOG(LogTC, Log, TEXT("orders check: script of %s"), *OrdersScript);
	}
	TokenRequest.Server = Settings.Url;
	TokenRequest.Room = Settings.Room;
	TokenRequest.bSpectate = Settings.bSpectate;
	if (bTokenOnCommandLine) { SetupError = TEXT("-TCToken= is refused because the engine logs the command line and other local users can read it with ps. Treat that token as exposed. Use -TCSeat=<label>, -TCTokenFile=<absolute path> or the TC_TOKEN environment variable."); UE_LOG(LogTC, Error, TEXT("not connecting: %s"), *SetupError); return; }
	BaseSettings = Settings;
	BaseSettings.Listing.Reset();
	BaseSettings.Token.Reset();
	BaseTokenRequest = TokenRequest;
	if (!bControlsDeferConnect) Connect(Settings, TokenRequest); // round 15: -TCBrowse connects once a room is picked
}

bool ATCGameMode::Connect(FTCConnection::FSettings Settings, const FTCTokenRequest& InRequest)
{
	auto Refuse = [this](const FString& Why) { SetupError = Why; UE_LOG(LogTC, Error, TEXT("not connecting: %s"), *Why); return false; };
	if (!Settings.Url.StartsWith(TEXT("ws://")) && !Settings.Url.StartsWith(TEXT("wss://"))) return Refuse(TEXT("-TCServer must be a ws:// or wss:// URL"));
	if (!FTCConnection::ValidRoom(Settings.Room)) return Refuse(TEXT("-TCRoom must be 3 to 12 letters or digits"));

	FTCTokenRequest TokenRequest = InRequest;
	TokenRequest.Server = Settings.Url;
	TokenRequest.Room = Settings.Room;
	TokenRequest.bSpectate = Settings.bSpectate;
	TokenStore = MakeShared<FTCTokenStore>();
	FString TokenError;
	if (!TokenStore->Resolve(TokenRequest, Settings.Token, TokenError)) { TokenStore.Reset(); return Refuse(TokenError); }
	UE_LOG(LogTC, Log, TEXT("seat token from %s%s"), *TokenStore->Source(), TokenRequest.Seat.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (seat label %s)"), *TokenRequest.Seat));
	if (FPaths::FileExists(FPaths::Combine(FPaths::ProjectSavedDir(), TEXT("TCToken.txt"))))
		UE_LOG(LogTC, Warning, TEXT("Saved/TCToken.txt (one token shared by every room and client) is no longer read: delete it"));

	// Until a lobby says otherwise, -TCSpectate means watching: the start a spectator gets names seat 0.
	State.bSpectator = Settings.bSpectate;
	Connection = MakeShared<FTCConnection>(Settings);
	Settings.Token.Reset();
	Connection->OnOpened = [this](int32)
	{
		State.OnSocketOpened();
		// a request sent on the old socket can no longer be answered (the server answers the socket that asked)
		Lobby.OnSocketOpened();
		Options.OnSocketOpened();
		Persistence.OnSocketOpened(FPlatformTime::Seconds());
		ControlsOpened();
	};
	Connection->OnMessage = [this](const FJsonObject& M, int32 Bytes) { OnMessage(M, Bytes); };
	// TC_DECODER (Net/TCRustViewShadow.h): rust (the default) makes the Rust view's unit rows and held verdict the
	// state's (Begin(State) attaches it to State); shadow runs it beside the C++ decoder; cpp runs no view.
	if (FTCRustViewShadow::Requested() && RustShadow.Begin(State)) Connection->OnText = [this](const FString& Text) { RustShadow.Feed(Text); };
	Connection->Start();
	SetupError.Reset();
	return true;
}

void ATCGameMode::EndPlay(const EEndPlayReason::Type Reason)
{
	if (Connection) { Connection->OnMessage = nullptr; Connection->OnOpened = nullptr; Connection->OnText = nullptr; Connection->Stop(); Connection.Reset(); }
	RustShadow.End();
	TokenStore.Reset(); // releases the token file's lock
	Backdrop.Hide(); // round 14
	WorldView.Detach();
	GroundPaint.Detach();
	Railways.Detach();
	Atmosphere.Detach();
	Water.Detach();
	Apron.Detach();
	TCFogLook::SetActive(nullptr);
	FogFrontier.Detach();
	Super::EndPlay(Reason);
}

FString ATCGameMode::ConnectionStatus() const
{
	if (!Connection) return SetupError.IsEmpty() ? FString(TEXT("not connected")) : TEXT("not connected: ") + SetupError;
	return FString::Printf(TEXT("%s | rtt %s"), *Connection->Status(), Rtt >= 0 ? *FString::Printf(TEXT("%.0f ms"), Rtt) : TEXT("-"));
}

bool ATCGameMode::SendCommand(const TSharedRef<FJsonObject>& Cmd)
{
	if (!Connection) { LastSendProblemText = TEXT("not sent: not connected"); return false; }
	int32 Bytes = 0;
	const ETCSendResult Result = Connection->TrySend(Cmd, &Bytes);
	if (Result == ETCSendResult::Sent) { LastSendProblemText.Reset(); OrdersCheck.NoteSent(Cmd->GetStringField(TEXT("t"))); return true; }
	LastSendProblemText = Result == ETCSendResult::TooLarge ? FString::Printf(TEXT("%s (%d bytes)"), LexToString(Result), Bytes) : FString(LexToString(Result));
	return false;
}

void ATCGameMode::ClearMatchPresentation()
{
	// State.ClearMatch emptied the units and the map: one draw clears every instance batch, and a reset from the empty
	// map clears the ground's mesh sections (ATCTerrain::ResetFrom with W = H = 0).
	if (Units) { Units->Draw(State, UnitTable, Terrain, TSet<int32>(), 0.f); Units->Reset(); }
	if (Terrain) Terrain->ResetFrom(State);
	WarMap.Clear(); // round 14
	Water.ResetFrom(State);
	Apron.ResetFrom(State);
	FogFrontier.Clear(); FogFrontierSnapshots = -1;
	State.bMapReset = false;
	Railways.Reset(State);
	Atmosphere.StopMatch();
	AtmosphereStart = -1;
	Recruit.Clear();
	Build.Clear();
	Logistics.Clear();
	Support.Clear();
	Abilities.Clear();
	Field.Clear();
	Powers.Clear();
	Progress.Clear();
	WorldRec.Clear();
	RegionAlerts.Reset();
	WorldObjects.Clear();
	WorldView.Reset();
	GroundPaint.Clear();
	bArmyRally = false;
	SupplyAlerts.Reset();
	PlanKinds.Reset();
	bHasPrevPoints = false; PrevPoints.Reset();
	bAlertHistoryOpen = false;
	ClearCombat();
	TutorialClear();
	// client/main.js receiveLobby: audio.end() (everything fades out over 0.4 s)
	if (UTCAudio* A = Audio()) A->ReturnedToLobby();
	State.DirtyCells.Reset(); State.FogDirtyCells.Reset();
	if (ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController()))
	{
		PC->Selected.Reset(); PC->bDragging = PC->bDragMoved = false;
		PC->LastOrderAt = FVector2D(-1, -1); PC->LastOrderNote.Reset();
		PC->CancelAim(); PC->bBuildMenuOpen = PC->bTrenchMenuOpen = false;
		PC->Groups.Sync(State); // no match: every group empties
		if (ATCHUD* Hud = Cast<ATCHUD>(PC->GetHUD())) Hud->Minimap.Reset();
	}
}

bool ATCGameMode::SpotOnScreen(double X, double Z) const
{
	// client/alerts.js underAttack: no alert for a spot the camera already shows
	const ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!PC || !Terrain) return false;
	FVector2D Px;
	if (!PC->ProjectWorldLocationToScreen(ATCTerrain::ToWorld(X, Z, Terrain->HeightAt(X, Z)), Px)) return false;
	int32 W = 0, H = 0;
	PC->GetViewportSize(W, H);
	return W > 0 && H > 0 && Px.X >= 0 && Px.Y >= 0 && Px.X < W && Px.Y < H;
}

void ATCGameMode::TakeSlice8Notices()
{
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	// client/hud.js drawTech's announcements: a finished tier ('<name> ready') or Armory level ('<line> <n> researched')
	for (const FString& A : Progress.TakeAnnouncements())
	{
		UE_LOG(LogTC, Log, TEXT("research announcement at tick %d: %s"), State.Tick, *A);
		if (PC) PC->ShowFeedback(A, 3.0);
	}
	// client/alerts.js: the region alerts; 'under attack' passes the shared throttle first (one per 30 m per 20 s, none
	// for a spot on screen)
	const double Now = FPlatformTime::Seconds();
	for (FTCWorldAlert& A : WorldRec.TakeAlerts())
	{
		const bool bOnScreen = A.Kind == ETCWorldAlertKind::UnderAttack && SpotOnScreen(A.X, A.Z);
		const bool bShow = A.Kind != ETCWorldAlertKind::UnderAttack || AttackQuiet.Allow(Now, A.X, A.Z, bOnScreen);
		UE_LOG(LogTC, Log, TEXT("region alert at tick %.0f: %s (%s) at (%.1f, %.1f)%s"), A.Tick, *A.Text, LexToString(A.Kind), A.X, A.Z, bShow ? TEXT("") : bOnScreen ? TEXT(", not shown: on screen") : TEXT(", not shown: throttled"));
		if (!bShow) continue;
		if (PC) PC->ShowFeedback(A.Text, 3.0);
		PushAlert(A.Kind == ETCWorldAlertKind::UnderAttack ? TEXT("attack") : TEXT("region"), A.Text, FVector2D(A.X, A.Z));
		RegionAlerts.Add(TPair<double, FTCWorldAlert>(GetWorld()->GetTimeSeconds(), MoveTemp(A)));
		if (RegionAlerts.Num() > 20) RegionAlerts.RemoveAt(0);
	}
}

void ATCGameMode::OnMessage(const FJsonObject& M, int32 Bytes)
{
	const FString T = M.GetStringField(TEXT("t"));
	ControlsMessage(T, M); // round 15: lobby and match controls (Client/TCMatchControls.cpp)
	if (T == TEXT("s"))
	{
		Perf.Net(Bytes, FPlatformTime::Seconds() * 1000.0); // perf.js net(size), round 15
		const bool bDrift = State.ApplySnapshot(M);
		RustShadow.AfterSnapshot(State, bDrift);
		Recruit.ApplySnapshot(M); // wallet, mode, tech, army limit, queues and the rows' order (ignored before a start)
		bLastSnapLogistics = M.HasField(TEXT("logistics"));
		Build.ApplySnapshot(M);   // the placement view's rows, mode, tech, nodes, smokes and World regions
		Logistics.ApplySnapshot(M, State); // own reserves and trucks, friendly stores, alerts (nothing before the seat is confirmed)
		Support.ApplySnapshot(M);   // each row's built as the exact double the server sent
		Abilities.ApplySnapshot(M); // wallet (mun), mode, factions, seats out
		Field.ApplySnapshot(M);     // the works, rows, smokes and cells for the fortification placement view
		Powers.ApplySnapshot(M);    // support cooldowns, MP and Munitions, mode, seats out, home, airfields, strikes
		WorldObjects.ApplySnapshot(M); // this side's cell memory (houses, walls, damage stages) and the wrecks it was sent
		GroundPaint.ApplySnapshot(M);  // the same cell rows repaint the ground round them (queued, a few tiles a frame)
		Progress.ApplySnapshot(M); // tech, lab, wallet, mode, seats out (nothing before a start)
		WorldRec.ApplySnapshot(M, State); // World: regions, stations, score, recovery offer; the region alerts against the last
		Railways.ApplySnapshot(M); Railways.ApplyCells(M); // center(id) from this snapshot; the explored track (applyCells)
		TakeSlice8Notices();
		// client/main.js applySnapshot: the seat just went out, so every aim ends
		if (Powers.WentOut(State.You))
			if (ATCPlayerController* OPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController())) { OPC->CancelAim(); UE_LOG(LogTC, Log, TEXT("seat %d is out: aims cancelled"), State.You); }
		AdoptAutocast();
		bArmyRally = State.bHasMatch && TCArmyCommands::ReadArmyRally(M, ArmyRally);
		// the idle keys' plan lookup (client/selection.js idle reads v.plan) and the point alerts (client/alerts.js)
		PlanKinds.Reset();
		for (const FTCPlan& P : State.Plans) PlanKinds.Add(P.Id, P.Kind);
		TakePointAlerts();
		OnCombatSnapshot(M); // alerts.js (units, losses, air, ready, say), the combat effects and their sounds (Client/TCAlerts.cpp)
		TutorialSnapshot(M); // round 14: the goal box, banners, damaged buildings, the Tutorial's progress (Client/TCTutorial.cpp)
		LogObjectives(false);
		for (FTCLogisticsAlert& A : Logistics.TakeAlerts())
		{
			UE_LOG(LogTC, Log, TEXT("supply alert at tick %d: %s at (%.1f, %.1f)"), State.Tick, *A.Text, A.X, A.Z);
			PushAlert(TEXT("supply"), A.Text, FVector2D(A.X, A.Z));
			SupplyAlerts.Add(TPair<double, FTCLogisticsAlert>(GetWorld()->GetTimeSeconds(), MoveTemp(A)));
			if (SupplyAlerts.Num() > 20) SupplyAlerts.RemoveAt(0);
		}
		if (bDrift && FPlatformTime::Seconds() - LastResync > 1.0)
		{
			// client/main.js applySnapshot: the rows drifted from what the server says we hold; ask for a full resend
			LastResync = FPlatformTime::Seconds(); State.Resyncs++;
			UE_LOG(LogTC, Warning, TEXT("unit rows out of sync at tick %d (holding %d): asking for a full resend"), State.Tick, State.Units.Num());
			TSharedRef<FJsonObject> R = MakeShared<FJsonObject>(); R->SetStringField(TEXT("t"), TEXT("resync")); SendCommand(R);
		}
	}
	else if (T == TEXT("start"))
	{
		// The match id restarts at 1 with the server (rooms live in memory), so "same match" also needs the same map and
		// seat; anything else is a new match and the selection made for the old units must go.
		const int32 PrevMatch = State.MatchId, PrevYou = State.You, PrevW = State.Map.W, PrevH = State.Map.H;
		const bool bHadMatch = State.bHasMatch;
		const FString PrevMap = State.Map.Name;
		Backdrop.Hide(); // round 14: lobby-view.js hide() frees the backdrop before the match builds its world
		if (!State.ApplyStart(M)) { UE_LOG(LogTC, Error, TEXT("malformed start (%d bytes) ignored"), Bytes); RustShadow.AfterStart(State, false, false, StartsReceived + 1); return; }
		Recruit.ApplyStart(M);
		Build.ApplyStart(M);
		Logistics.ApplyStart();
		Support.ApplyStart(M);
		Abilities.ApplyStart(M); // also client/main.js startGame autocast.reset()
		Field.ApplyStart(M);
		Powers.ApplyStart(M); // home from the start's spawn, the last snapshot forgotten
		Progress.ApplyStart(M);
		WorldRec.ApplyStart(M); // seat, names, teams and match id: a reconnect to the same match replays no alert
		{
			// server.js start: weather: weatherRow(room.game), shared/weather.js: [now] or [now, next, seconds left] (what the
			// lobby's Weather choice brought; logged only)
			const TArray<TSharedPtr<FJsonValue>>* W = nullptr;
			FString Kind;
			if (M.TryGetArrayField(TEXT("weather"), W) && W && W->Num() && (*W)[0].IsValid() && (*W)[0]->TryGetString(Kind)) { UE_LOG(LogTC, Log, TEXT("start weather: %s (%s), %d field(s)"), *Kind, *FTCMatchOptions::WeatherName(Kind), W->Num()); }
			else { UE_LOG(LogTC, Log, TEXT("start weather: no weather row in this start")); }
		}
		if (!WorldObjects.ApplyStart(M, State)) UE_LOG(LogTC, Error, TEXT("world objects: the start's map could not be read; no houses or wrecks this match"));
		Terrain->SetCellSource(&WorldObjects); // relief: the physical grid (a known mine drawn as what lies under it)
		if (!GroundPaint.ApplyStart(M, State)) UE_LOG(LogTC, Error, TEXT("ground paint: the start's map could not be read; the ground keeps its plain colours this match"));
		const double StartAt = FPlatformTime::Seconds();
		Persistence.ApplyStart(M, StartAt); // a pending load is answered by this start
		if (Persistence.Status().Outcome == ETCPersistenceOutcome::Loaded && Persistence.Status().AnsweredAt == StartAt)
			UE_LOG(LogTC, Log, TEXT("saves: %s (the server pauses a loaded match for its host; the tick comes with the next snapshot)"), *Persistence.Status().Describe());
		bArmyRally = false;
		SupplyAlerts.Reset();
		PlanKinds.Reset();
		bHasPrevPoints = false; PrevPoints.Reset();
		StartsReceived++;
		if (MatchSeenAt < 0) MatchSeenAt = FPlatformTime::Seconds();
		Terrain->ResetFrom(State); State.bMapReset = false;
		GroundPaint.Bind(Terrain->GroundMaterialInstance()); // the relief draws the paint and its fog (MF_TCGroundPaint)
		Water.ResetFrom(State);
		WarMap.Start(State); // round 14: the paper is painted again the next time it shows
		Apron.ResetFrom(State);
		// round 14 (edge): fog.js createFog at main.js startGame (the start's mask; none for a spectator of an authored map)
		FogFrontier.Start(State); FogFrontierSnapshots = State.Snapshots;
		Railways.Reset(State); Railways.ApplyCells(M); // main.js startGame: createRailways on a World map, railways.cells(m.cells)
		// slice 13 lead glue: the apron continues the painted ground with the paint's own tinted tiles (apron.js reads
		// ground.js groundLook().tiles); without them it keeps the reference's flat pre-load colours
		{
			TArray<TArray<uint8>> TileRgba; TArray<int32> TileSizes;
			if (GroundPaint.ApronTiles(TileRgba, TileSizes)) Apron.SetTileBytes(TileRgba, TileSizes);
			else UE_LOG(LogTC, Warning, TEXT("apron: no paint tiles this match; flat colours"));
		}
		Units->Reset();
		OnCombatStart(); // the alert feed's match and the effects reset (Client/TCAlerts.cpp)
		// client/main.js startGame: audio.start({ faction: facOf(me), slot: me }) at every start (a reconnect included);
		// the effects and this seat's voice load now, never in the lobby
		if (UTCAudio* A = Audio()) A->MatchStarted(State.Factions.IsValidIndex(State.You) ? State.Factions[State.You] : 0, FMath::Max(0, State.You));
		ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
		const bool bSameMatch = bHadMatch && PrevMatch == State.MatchId && PrevYou == State.You && PrevW == State.Map.W && PrevH == State.Map.H && PrevMap == State.Map.Name;
		RustShadow.AfterStart(State, true, bSameMatch, StartsReceived);
		if (!bSameMatch)
		{
			// client/alert-history.js start: a new match id empties the history (a reconnect to the same match keeps it)
			Alerts.Reset(); AlertCursor = 0; bAlertHistoryOpen = false;
			EpilogueSeenAt = -1; bEpilogueShot = bEpilogueLogged = false; LastLoggedOwners = INT32_MIN;
		}
		if (PC && !bSameMatch)
		{
			PC->Selected.Reset();
			PC->CancelAim(); PC->bBuildMenuOpen = PC->bTrenchMenuOpen = false;
			PC->StartView(State.Spawn, FVector2D(State.Map.W * FTCMatchState::CellMetres, State.Map.H * FTCMatchState::CellMetres));
			if (StartZoom > 0) PC->ZoomTo(StartZoom);
		}
		// Ownership at this moment: a fresh client joining a running match gets its start before its lobby, so its seat
		// is unconfirmed (and nothing is its own) until that lobby comes
		// client/main.js receiveStart: coverPreview.start() (scouting from what the server says this side explored)
		if (PC)
		{
			FTCCoverKnown Known;
			PC->GatherCover(Known);
			Cover.Start(Known);
		}
		const TCHAR* SeatNote = State.bSpectator ? TEXT("watching") : State.SeatConfirmed() ? TEXT("confirmed") : TEXT("unconfirmed until the lobby");
		UE_LOG(LogTC, Log, TEXT("start #%d: match %d%s, map '%s' %dx%d%s, seat %d (%s), spawn (%.0f, %.0f), %d bytes, fog %s, cells known %d, selected kept %d"), StartsReceived, State.MatchId,
			bSameMatch ? TEXT(" (same match: reconnect)") : TEXT(""), *State.Map.Name, State.Map.W, State.Map.H, State.Map.bWorld ? TEXT(" (World: discovered cells only)") : TEXT(""),
			State.You, SeatNote, State.Spawn.X, State.Spawn.Y, Bytes, State.bFogLifted ? TEXT("lifted") : TEXT("on"), State.Map.Known.CountSetBits(), PC ? PC->Selected.Num() : 0);
		TutorialStart(); // round 14: objectives.js reset, the Tutorial's progress, the Controls sheet at the first match
	}
	else if (T == TEXT("lobby"))
	{
		const bool bHadMatch = State.bHasMatch;
		const int32 EndedMatch = State.MatchId;
		int32 GroupedBefore = 0;
		if (const ATCPlayerController* GPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController()))
			for (int32 G = 1; G <= FTCControlGroups::GroupCount; G++) GroupedBefore += GPC->Groups.Members(G).Num();
		State.ApplyLobby(M);
		if (Connection) Connection->SetWatching(State.bSpectator);
		const int32 ConfirmedBefore = Lobby.Confirmed(), RowsBefore = Persistence.Rows().Num();
		if (!Lobby.ApplyLobby(M)) { UE_LOG(LogTC, Warning, TEXT("lobby settings: this lobby message could not be read; the settings shown are the last good ones")); }
		else if (Lobby.Confirmed() != ConfirmedBefore) { UE_LOG(LogTC, Log, TEXT("lobby settings: %d request(s) shown by this lobby, %d still pending"), Lobby.Confirmed() - ConfirmedBefore, Lobby.PendingCount()); }
		Persistence.ApplyLobby(M);
		const int32 OptionsBefore = Options.Confirmed();
		if (!Options.ApplyLobby(M)) { UE_LOG(LogTC, Warning, TEXT("match options: this lobby message could not be read; the options shown are the last good ones")); }
		else if (Options.Confirmed() != OptionsBefore) { UE_LOG(LogTC, Log, TEXT("match options: %d request(s) shown by this lobby, %d still pending (weather %s, sandbox %s, %d seats)"), Options.Confirmed() - OptionsBefore, Options.PendingCount(), *Options.WeatherChoice(), Options.SandboxOn() ? TEXT("on") : TEXT("off"), Options.Seats().Num()); }
		if (!Persistence.LastMalformed().IsEmpty()) { UE_LOG(LogTC, Warning, TEXT("saves: the lobby's list could not be read (%s); the rows shown are the last good ones"), *Persistence.LastMalformed()); }
		else if (Persistence.Rows().Num() != RowsBefore || Persistence.Rows().Num())
		{
			TArray<FString> Ids;
			for (const FTCSaveRow& R : Persistence.Rows()) Ids.Add(R.Id);
			UE_LOG(LogTC, Log, TEXT("saves: %d row(s) in this lobby [%s]"), Persistence.Rows().Num(), *FString::Join(Ids, TEXT(", ")));
		}
		if (bHadMatch && !State.bHasMatch)
		{
			ClearMatchPresentation();
			int32 GroupedAfter = 0;
			if (const ATCPlayerController* GPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController()))
				for (int32 G = 1; G <= FTCControlGroups::GroupCount; G++) GroupedAfter += GPC->Groups.Members(G).Num();
			UE_LOG(LogTC, Log, TEXT("match %d over (%s): room %s is back in the lobby (state %s); units, ground and selection cleared; group members %d -> %d; recruit data %s"), EndedMatch,
				*State.LastResult, *State.RoomCode, *State.RoomState, GroupedBefore, GroupedAfter, Recruit.HasSnapshot() ? TEXT("still held") : TEXT("cleared"));
		}
		TArray<FString> Seats;
		for (const FTCLobbyPlayer& P : State.Roster) Seats.Add(FString::Printf(TEXT("%s%s"), *P.Name, P.bAi ? TEXT(" (AI)") : P.bConnected ? TEXT("") : TEXT(" (offline)")));
		UE_LOG(LogTC, Log, TEXT("lobby: room %s %s, mode %s, map %s%s, %s, seat %d of %d [%s]%s"), *State.RoomCode, *State.RoomState, *State.Mode, *State.LobbyMap, State.bLobbySandbox ? TEXT(", sandbox") : TEXT(""),
			State.bSpectator ? TEXT("watching") : State.bAmHost ? TEXT("host") : TEXT("guest"), State.LobbySeat, State.Roster.Num(), *FString::Join(Seats, TEXT(", ")),
			State.LastResult.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(", last match: %s"), *State.LastResult));
	}
	else if (T == TEXT("ping")) OnPingRelay(M); // a teammate's map ping (server/map-pings.js relay; Client/TCAlerts.cpp)
	else if (T == TEXT("pause")) { State.ApplyPause(M); UE_LOG(LogTC, Log, TEXT("pause: %s"), State.PauseText.IsEmpty() ? TEXT("running") : *State.PauseText); }
	else if (T == TEXT("deny"))
	{
		State.ApplyDeny(M);
		Persistence.ApplyDeny(M, FPlatformTime::Seconds());
		UE_LOG(LogTC, Warning, TEXT("server refused %s (shown as: %s)"), *State.LastDeny, *TCOrders::DenySentence(State.DenyReason, State.DenyCmd));
		// client/main.js: connection.on('deny', m => feedback.show(denySentence(...))): the error blip with the sentence
		if (ATCPlayerController* DPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController())) DPC->Feedback(TEXT("error"));
	}
	else if (T == TEXT("saved"))
	{
		Persistence.ApplySaved(M, State, FPlatformTime::Seconds());
		UE_LOG(LogTC, Log, TEXT("saves: the server saved this match as %s (%s)"), *Persistence.LastSavedId(), *Persistence.Status().Describe());
	}
	else if (T == TEXT("pong"))
	{
		double C;
		if (M.TryGetNumberField(TEXT("c"), C)) Rtt = FPlatformTime::Seconds() * 1000.0 - C;
		Perf.Pong(M, FPlatformTime::Seconds() * 1000.0); // perf.js pong (round 15: the stats overlay's ping, loss, jitter, server)
	}
	else if (T == TEXT("replaced")) { UE_LOG(LogTC, Warning, TEXT("server: replaced. Another client presented this seat's token; this one has stopped. Give each client its own -TCSeat or -TCTokenFile.")); }
	else if (T == TEXT("left")) { UE_LOG(LogTC, Warning, TEXT("server: left. An AI took over this army; the connection has stopped.")); }
	else if (T == TEXT("full")) { UE_LOG(LogTC, Warning, TEXT("server: full")); }
}

TArray<int32> ATCGameMode::ArmyIds(double& OutX, double& OutZ, bool bTrucks) const
{
	// What a box drag over the whole army would take (ATCPlayerController::Boxable): no supply trucks, which a move
	// switches off their runs (shared/supply-convoys.js), no trains, planes, structures or hidden units.
	TArray<int32> Ids;
	OutX = OutZ = 0;
	for (const auto& P : State.Units)
	{
		const FTCUnit& U = P.Value;
		const FTCUnitDef* D = UnitTable.Find(U.Type);
		if (State.IsOwn(U) && D && U.Hp > 0 && ATCUnitRenderer::IsDrawn(U) && !D->bStructure && !D->bAir && !D->bRail && (bTrucks || U.Type != TEXT("truck"))) { Ids.Add(P.Key); OutX += U.X; OutZ += U.Z; }
	}
	if (Ids.Num()) { OutX /= Ids.Num(); OutZ /= Ids.Num(); }
	return Ids;
}

void ATCGameMode::RunHostLobby(double Now)
{
	// -TCHostStart: once the room has shown its lobby for a second (so the HUD has drawn the host's buttons), press
	// Mode (if another mode was asked for), Add AI up to the seats asked for, and Start match
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	// the Start press waits for the lobby that shows the AI seats and settings asked (the browser's Start button is
	// drawn, and enabled or not, from that lobby), at most 5 s
	if (bHostStartPending && PC && State.RoomState == TEXT("lobby") && !State.bHasMatch
		&& ((State.LobbyPlayers >= 1 + HostStartAi && Lobby.PendingCount() == 0) || Now - HostStartAskedAt >= 5.0))
	{
		bHostStartPending = false;
		PC->PressButton(ETCUiAction::Start);
		UE_LOG(LogTC, Log, TEXT("host: %d seats in the lobby, asked to start: %s"), State.LobbyPlayers, *PC->LastOrderNote);
	}
	if (HostStartAi < 0 || bHostStarted || !PC || !State.bAmHost || State.bSpectator || State.RoomState != TEXT("lobby") || State.bHasMatch || LobbySeenAt < 0 || Now - LobbySeenAt < 1.0) return;
	if (State.LobbyPlayers < HostSeats) return; // -TCHostSeats: the other clients have not all joined yet
	bHostStarted = true;
	if (bHostSandbox && !State.bLobbySandbox)
	{
		TSharedRef<FJsonObject> Sb = MakeShared<FJsonObject>(); Sb->SetStringField(TEXT("t"), TEXT("sandbox")); Sb->SetBoolField(TEXT("v"), true); SendCommand(Sb);
		UE_LOG(LogTC, Log, TEXT("host: asked for Sandbox"));
	}
	if (!HostArmy.IsEmpty())
	{
		TSharedRef<FJsonObject> Ar = MakeShared<FJsonObject>(); Ar->SetStringField(TEXT("t"), TEXT("army")); Ar->SetStringField(TEXT("v"), HostArmy); SendCommand(Ar);
		UE_LOG(LogTC, Log, TEXT("host: asked for army size %s"), *HostArmy);
	}
	if (!HostMode.IsEmpty() && HostMode != State.Mode)
	{
		// the lobby's Mode select (FTCLobbySettings::Plan): server.js 'mode' is handled before the addAi and start after it
		const FTCLobbyRequest Md = Lobby.Plan(ETCLobbySetting::Mode, -1, HostMode);
		if (Md.Message.IsValid() && SendCommand(Md.Message.ToSharedRef())) Lobby.NoteSent(Md, Now);
		else UE_LOG(LogTC, Warning, TEXT("host: mode %s not sent: %s"), *HostMode, Md.Message.IsValid() ? *LastSendProblemText : *Md.Note);
		UE_LOG(LogTC, Log, TEXT("host: asked for mode %s (room had %s)"), *HostMode, *State.Mode);
	}
	if (!HostMap.IsEmpty() && HostMap != State.LobbyMap)
	{
		// -TCHostMap: the lobby's Map select (FTCLobbySettings::Plan, server.js 'map'), after the mode it belongs to
		const FTCLobbyRequest Mp = Lobby.Plan(ETCLobbySetting::Map, -1, HostMap);
		if (Mp.Message.IsValid() && SendCommand(Mp.Message.ToSharedRef())) Lobby.NoteSent(Mp, Now);
		UE_LOG(LogTC, Log, TEXT("host: asked for map %s (room had %s): %s"), *HostMap, *State.LobbyMap, Mp.Message.IsValid() ? TEXT("sent") : *Mp.Note);
	}
	if (!HostWeather.IsEmpty())
	{
		// -TCHostWeather: the lobby's Weather select (server.js 'weather': the host, outside play)
		TSharedRef<FJsonObject> Wx = MakeShared<FJsonObject>(); Wx->SetStringField(TEXT("t"), TEXT("weather")); Wx->SetStringField(TEXT("v"), HostWeather);
		UE_LOG(LogTC, Log, TEXT("host: asked for weather %s: %s"), *HostWeather, SendCommand(Wx) ? TEXT("sent") : *LastSendProblemText);
	}
	for (int32 I = State.LobbyPlayers; I < 1 + HostStartAi; I++) PC->PressButton(ETCUiAction::AddAi);
	for (int32 I = 0; I < HostTeams.Num(); I++)
	{
		// server.js 'team' (host, outside play): handled in order, after the addAi messages above and before the start
		TSharedRef<FJsonObject> Tm = MakeShared<FJsonObject>();
		Tm->SetStringField(TEXT("t"), TEXT("team")); Tm->SetNumberField(TEXT("slot"), I); Tm->SetNumberField(TEXT("v"), HostTeams[I]);
		UE_LOG(LogTC, Log, TEXT("host: seat %d to team %d: %s"), I, HostTeams[I], SendCommand(Tm) ? TEXT("sent") : *LastSendProblemText);
	}
	bHostStartPending = true;
	HostStartAskedAt = Now;
	UE_LOG(LogTC, Log, TEXT("host: added AI seats up to %d; Start follows the lobby that shows them"), HostStartAi);
}

void ATCGameMode::RunVerificationSwitches(double Now)
{
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	const bool bLobby = !State.bHasMatch && State.RoomState == TEXT("lobby");
	if (!bLobby) LobbySeenAt = -1; else if (LobbySeenAt < 0) LobbySeenAt = Now;
	RunHostLobby(Now);
	if (LobbyShotAfter >= 0 && bLobby && Now - LobbySeenAt >= LobbyShotAfter)
	{
		LobbyShotAfter = -1;
		FScreenshotRequest::RequestScreenshot(LobbyShotFile, true, false);
		const ATCHUD* Hud = PC ? Cast<ATCHUD>(PC->GetHUD()) : nullptr;
		UE_LOG(LogTC, Log, TEXT("lobby screenshot requested: %s (buttons on screen: %s)"), *LobbyShotFile, Hud ? *Hud->ButtonLabels() : TEXT("no HUD"));
	}
	if ((bAutoMove || bAutoAttackMove) && !bAutoMoved && PC && State.CanOrder() && State.Snapshots >= 3)
	{
		double CX, CZ;
		const TArray<int32> Ids = ArmyIds(CX, CZ);
		if (Ids.Num())
		{
			const FVector2D Centre(State.Map.W * FTCMatchState::CellMetres / 2, State.Map.H * FTCMatchState::CellMetres / 2), From(CX, CZ);
			AutoTarget = From + (Centre - From).GetSafeNormal() * AutoMoveMetres;
			PC->SelectOwn(Ids);
			const int32 Sent = PC->IssueMoveAt(AutoTarget, bAutoAttackMove);
			bAutoMoved = Sent > 0; AutoMovedAt = Now; AutoIds = Ids;
			UE_LOG(LogTC, Log, TEXT("auto %s: %d own units at (%.1f, %.1f) -> (%.1f, %.1f), %d ordered"), bAutoAttackMove ? TEXT("attack-move") : TEXT("move"), Ids.Num(), CX, CZ, AutoTarget.X, AutoTarget.Y, Sent);
		}
	}
	if (AutoStopAfter >= 0 && bAutoMoved && !bStopSent && PC && Now - AutoMovedAt >= AutoStopAfter)
	{
		bStopSent = true; StopSentAt = Now;
		PC->SelectOwn(AutoIds);
		StopFrom.Reset();
		for (int32 Id : AutoIds) if (const FTCUnit* U = State.Units.Find(Id)) StopFrom.Add(Id, FVector2D(U->X, U->Z));
		const bool bSent = PC->PressButton(ETCUiAction::Stop);
		UE_LOG(LogTC, Log, TEXT("auto stop: %d units at tick %d: %s"), PC->Selected.Num(), State.Tick, bSent ? TEXT("sent") : *PC->LastOrderNote);
	}
	if (bStopSent && StopReports < 8 && Now - StopSentAt >= StopReports * 1.0)
	{
		// how far each stopped unit is from where it was when the stop went out: it should level off within a second
		double Sum = 0, Max = 0; int32 N = 0;
		for (const auto& P : StopFrom) if (const FTCUnit* U = State.Units.Find(P.Key)) { const double D = FVector2D::Distance(FVector2D(U->X, U->Z), P.Value); Sum += D; Max = FMath::Max(Max, D); N++; }
		UE_LOG(LogTC, Log, TEXT("after stop +%ds: server tick %d, %d units held, moved since the stop mean %.2f m max %.2f m, denies %d"), StopReports, State.Tick, N, N ? Sum / N : -1.0, Max, State.Denies);
		StopReports++;
	}
	if (!SpawnEnemy.IsEmpty() && !bSpawned && State.CanOrder() && State.bSandbox && State.Snapshots >= 3)
	{
		double CX, CZ;
		if (ArmyIds(CX, CZ).Num())
		{
			bSpawned = true;
			const FVector2D Centre(State.Map.W * FTCMatchState::CellMetres / 2, State.Map.H * FTCMatchState::CellMetres / 2), From(CX, CZ);
			const FVector2D Dir = (Centre - From).GetSafeNormal(), Across(-Dir.Y, Dir.X);
			// a list (a,b,c) spawns each in turn, 6 m apart across the line to the centre
			TArray<FString> Types;
			SpawnEnemy.ParseIntoArray(Types, TEXT(","));
			for (int32 I = 0; I < Types.Num(); I++)
			{
				const FVector2D At = From + Dir * 30.0 + Across * ((I - (Types.Num() - 1) / 2.0) * 6.0);
				TSharedRef<FJsonObject> Sp = MakeShared<FJsonObject>();
				Sp->SetStringField(TEXT("t"), TEXT("spawn")); Sp->SetStringField(TEXT("unit"), Types[I]); Sp->SetBoolField(TEXT("enemy"), true);
				Sp->SetNumberField(TEXT("x"), At.X); Sp->SetNumberField(TEXT("z"), At.Y);
				UE_LOG(LogTC, Log, TEXT("sandbox spawn: enemy %s at (%.1f, %.1f): %s"), *Types[I], At.X, At.Y, SendCommand(Sp) ? TEXT("sent") : *LastSendProblemText);
			}
		}
	}
	if (bAutoAttack && !bAutoAttacked && PC && State.CanOrder() && State.Snapshots >= 3)
	{
		double CX, CZ;
		const TArray<int32> Ids = ArmyIds(CX, CZ, bAttackWithTrucks);
		const FTCUnit* Best = nullptr;
		for (const auto& P : State.Units)
			if (State.IsFoe(P.Value.Owner) && P.Value.Hp > 0 && ATCUnitRenderer::IsDrawn(P.Value) && (!Best || FVector2D::Distance(FVector2D(P.Value.X, P.Value.Z), FVector2D(CX, CZ)) < FVector2D::Distance(FVector2D(Best->X, Best->Z), FVector2D(CX, CZ)))) Best = &P.Value;
		if (Best && Ids.Num())
		{
			bAutoAttacked = true;
			PC->SelectOwn(Ids);
			FTCCursor C;
			FVector2D Screen;
			const bool bOnScreen = PC->ScreenOf(Best->Id, Screen);
			if (bOnScreen) C = PC->CursorAt(Screen); // the right-click's own pick, at the enemy's drawn point
			if (!bOnScreen || C.Enemy != Best->Id) { C.Enemy = Best->Id; C.bHasGround = true; C.Ground = FVector2D(Best->X, Best->Z); }
			UE_LOG(LogTC, Log, TEXT("auto attack: enemy %s unit %d of seat %d at (%.1f, %.1f), %.1f m from the army; %s"), *Best->Type, Best->Id, Best->Owner, Best->X, Best->Z,
				FVector2D::Distance(FVector2D(Best->X, Best->Z), FVector2D(CX, CZ)),
				bOnScreen ? *FString::Printf(TEXT("drawn at pixel (%.0f, %.0f), the screen pick found unit %d"), Screen.X, Screen.Y, PC->CursorAt(Screen).Enemy) : TEXT("not on screen (no renderer): enemy named directly"));
			AttackTarget = Best->Id; AttackAt = Now; AttackIds = Ids;
			const int32 Sent = PC->IssueOrder(C, false, false);
			UE_LOG(LogTC, Log, TEXT("auto attack: %d of %d units ordered"), Sent, Ids.Num());
		}
	}
	if (AttackAt >= 0 && AttackReports < 20 && Now - AttackAt >= AttackReports * 1.0)
	{
		const FTCUnit* T = State.Units.Find(AttackTarget);
		int32 Aiming = 0, N = 0; double Sum = 0;
		for (int32 Id : AttackIds) if (const FTCUnit* U = State.Units.Find(Id)) { N++; Aiming += U->Target == AttackTarget; if (T) Sum += FVector2D::Distance(FVector2D(U->X, U->Z), FVector2D(T->X, T->Z)); }
		UE_LOG(LogTC, Log, TEXT("attack +%ds: server tick %d, target %d %s, %d of %d ordered units firing at it, mean distance %.1f m, denies %d"), AttackReports, State.Tick, AttackTarget,
			T ? *FString::Printf(TEXT("held, hp %d"), T->Hp) : TEXT("no longer held (destroyed or out of sight)"), Aiming, N, T && N ? Sum / N : -1.0, State.Denies);
		AttackReports++;
	}
	if (bProbeDeny && !bProbeSent && State.CanOrder() && State.Snapshots >= 3)
	{
		double CX, CZ;
		const TArray<int32> Ids = ArmyIds(CX, CZ);
		if (Ids.Num())
		{
			bProbeSent = true; DeniesAtProbe = State.Denies;
			// not something the right-click builds (it attacks only foes): a raw message, to see the server's validation
			TSharedRef<FJsonObject> A = MakeShared<FJsonObject>();
			A->SetStringField(TEXT("t"), TEXT("attack"));
			A->SetArrayField(TEXT("ids"), { MakeShared<FJsonValueNumber>(Ids[0]) });
			A->SetNumberField(TEXT("target"), Ids[0]);
			UE_LOG(LogTC, Log, TEXT("deny probe: attack on own unit %d: %s; the server should refuse it ('unseen')"), Ids[0], SendCommand(A) ? TEXT("sent") : *LastSendProblemText);
		}
	}
	if (bAutoMoved && AutoReports < 12 && Now - AutoMovedAt >= AutoReports * 1.0)
	{
		double Sum = 0; int32 N = 0;
		for (int32 Id : AutoIds) if (const FTCUnit* U = State.Units.Find(Id)) { Sum += FVector2D::Distance(FVector2D(U->X, U->Z), AutoTarget); N++; }
		UE_LOG(LogTC, Log, TEXT("auto move +%ds: server tick %d, %d of %d ordered units held, mean distance to the click %.1f m"), AutoReports, State.Tick, N, AutoIds.Num(), N ? Sum / N : -1.0);
		AutoReports++;
	}
	RunSlice3Switches(Now);
	RunSlice4Switches(Now);
	RunSlice5Switches(Now);
	RunSlice6Switches(Now);
	RunSlice7Switches(Now);
	RunSlice8Switches(Now);
	if (ShotAfter >= 0 && MatchSeenAt >= 0 && Now - MatchSeenAt >= ShotAfter)
	{
		ShotAfter = -1;
		FScreenshotRequest::RequestScreenshot(ShotFile, true, false);
		UE_LOG(LogTC, Log, TEXT("screenshot requested at tick %d: %s"), State.Tick, *ShotFile);
		if (PC) UE_LOG(LogTC, Log, TEXT("framing: focus (%.1f, %.1f) at %.1f m; %s"), PC->ViewFocus().X, PC->ViewFocus().Y, PC->ViewDistance(), *PC->FramingCheck());
	}
	if (EndAfter >= 0 && !bEndSent && MatchSeenAt >= 0 && Now - MatchSeenAt >= EndAfter && State.bHasMatch && State.bAmHost)
	{
		bEndSent = true;
		TSharedRef<FJsonObject> E = MakeShared<FJsonObject>(); E->SetStringField(TEXT("t"), TEXT("end"));
		const bool bSent = SendCommand(E);
		UE_LOG(LogTC, Log, TEXT("end switch: host asks to end match %d at tick %d: %s"), State.MatchId, State.Tick, bSent ? TEXT("sent") : *LastSendProblemText);
	}
	if (DropAfter >= 0 && !bDropped && MatchSeenAt >= 0 && Now - MatchSeenAt >= DropAfter && Connection)
	{
		bDropped = true;
		UE_LOG(LogTC, Log, TEXT("drop switch: dropping the socket at tick %d holding %d units"), State.Tick, State.Units.Num());
		Connection->SimulateDrop();
	}
}

void ATCGameMode::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);
	const double Now = FPlatformTime::Seconds();
	// round 15: perf.js frame once per frame: the frame start to start, the game thread's time of the last frame, the
	// renderer's draw calls and triangles, and the units, effect particles and bodies (main.js's counts)
	Perf.Frame(Now * 1000.0, Now * 1000.0, FPlatformTime::ToMilliseconds(GGameThreadTime), GNumDrawCallsRHI[0], GNumPrimitivesDrawnRHI[0], State.Units.Num(),
		Effects ? Effects->Fx.Particles().Num() : 0, Units ? Units->ModelStats().Corpses : 0);
	if (Connection)
	{
		Connection->Tick(Now);
		if (Now - LastPing >= 2.0 && Connection->IsOpen())
		{
			LastPing = Now;
			TSharedRef<FJsonObject> Ping = MakeShared<FJsonObject>();
			Ping->SetStringField(TEXT("t"), TEXT("ping"));
			Ping->SetNumberField(TEXT("c"), Now * 1000.0);
			if (Rtt >= 0) Ping->SetNumberField(TEXT("rtt"), Rtt);
			if (SendCommand(Ping)) Perf.Pinged(Now * 1000.0); // main.js: if (sendCmd(...)) perf.pinged(c)
		}
	}
	if (Lighting && State.bHasMatch)
	{
		// atmosphere.js start at each new match (the mood for the map and weather, the sun aimed for this player's opening
		// view), then the weather, the showers and the camera distance the haze follows (light.js followView)
		const ATCPlayerController* LightPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
		if (LightingStart != StartsReceived)
		{
			LightingStart = StartsReceived;
			// main.js atmos.start({ key: lobbyState?.mapName }): the mood also reads the lobby's map key
			Lighting->StartMatch(State.Map.Name, State.LobbyMap, State.Weather.Now, FMath::DegreesToRadians(-90.f - (LightPC ? LightPC->ViewYaw() : -90.f)));
		}
		Lighting->SetWeather(State.Weather.Now);
		if (State.Wx.bKnown) Lighting->SetShower((float)State.Wx.Rain, (float)State.Wx.Wet);
		if (LightPC) Lighting->SetCameraDistance(LightPC->ViewDistance());
		// round 14: the ground layer's FOG_FADE follows the haze's distances (the frame before's; the lighting ticks after)
		Atmosphere.SetHazeDistances(Lighting->FogNear(), Lighting->FogFar());
	}
	// round 14: the lobby's backdrop (client/main.js previewMap -> lobby-view.js show), never during a match; gfx.js Low
	// hides it (lobby-view.js: "Nothing runs on Graphics Low"), read every frame so the menu's switch applies at once
	Backdrop.bGraphicsLow = !FTCGraphics::Get().Budget().bLobbyBackdrop;
	if (!State.bHasMatch && !State.LobbyMap.IsEmpty()) Backdrop.Show(GetWorld(), ServerUrl, State.LobbyMap, State.Mode);
	else if (State.bHasMatch) Backdrop.Hide();
	Backdrop.Tick(DeltaSeconds);
	// round 15 (hudfit): the backdrop's own light (client/lobby-view.js) while its battlefield shows, the mood otherwise
	if (Lighting)
	{
		FTCLobbyLook LobbyLook;
		Lighting->SetLobbyLook(Backdrop.Look(LobbyLook) ? &LobbyLook : nullptr);
	}
	if (Terrain && State.bHasMatch)
	{
		// round 14 (edge): fog.js snapshot() after each snapshot (the flips, the frontier around newly explored ground,
		// the cells under the delivered units); the cells whose look moved join the fog cells every consumer refreshes
		if (FogFrontier.HasMap() && State.Snapshots != FogFrontierSnapshots)
		{
			FogFrontierSnapshots = State.Snapshots;
			FogFrontier.Snapshot(State, State.FogDirtyCells, UnitTable);
			State.FogDirtyCells.Append(FogFrontier.TakeChanged());
		}
		// fog.js frame(dt, lift): the match decided on an authored map lifts the fog (ATCTerrain's end reveal)
		FogFrontier.Frame(DeltaSeconds, Terrain->IsEndRevealed(), State);
		if (State.DirtyCells.Num() || State.FogDirtyCells.Num())
		{
			WorldView.MarkFog(WorldObjects, State.FogDirtyCells); // the pieces on those cells are dimmed like the ground
			// round 14: map-view.js refresh() on terrain changes; the wash follows fog.js
			if (State.DirtyCells.Num()) WarMap.MarkTerrain();
			WarMap.MarkFog(State, State.FogDirtyCells);
			GroundPaint.MarkFog(State.FogDirtyCells);             // the ground's fog texels fade to their new look
			Water.MarkCells(State, State.DirtyCells, State.FogDirtyCells);
			Apron.MarkCells(State, State.DirtyCells, State.FogDirtyCells);
			Terrain->MarkCells(State.DirtyCells, State.FogDirtyCells); State.DirtyCells.Reset(); State.FogDirtyCells.Reset();
		}
		// round 15 (gfxlow glue): -TCGraphicsAt switches the level at match seconds (verification); every builder below
		// sees the change event this frame
		if (State.bHasMatch && State.Snapshots > 0) FTCGraphics::Get().RunSchedule(State.Tick / 20.0);
		Terrain->Flush(State);
		// the painted ground: queued tiles, the fog fade, and the decals on the heights as drawn
		GroundPaint.Tick(State, DeltaSeconds, Terrain->IsEndRevealed(), [this](float X, float Z) { return Terrain->HeightAt(X, Z); });
		// after the ground: the water drapes raised spans on it and the apron follows its edge heights
		Water.Flush(State, FPlatformTime::Seconds() - BeganAt, Terrain->IsEndRevealed());
		Apron.Flush(State, Terrain->IsEndRevealed());
		if (!WaterShotsDir.IsEmpty()) WaterShotsTick();
		// after the ground: the pieces stand on the heights as drawn
		WorldObjects.Update(*Terrain, &UnitTable);
		WorldView.Draw(WorldObjects, State, Terrain);
		// round 14: the paper war map's fade (map-view.js frame(dt, cam.dist / rig.wide)), after the ground is current
		if (const ATCPlayerController* MapPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController()))
			WarMap.Frame(DeltaSeconds, MapPC->HasWideView() && MapPC->WideDistance() > 0 ? MapPC->ViewDistance() / MapPC->WideDistance() : 0.f, State, Terrain, Terrain->IsEndRevealed());
		Railways.Frame(DeltaSeconds, State, Terrain); // railways.frame(dt): at most two rebuilds a second
		// atmosphere.js start at each new match, then update(dt) with the weather row, snapshot wx and the camera target
		ATCPlayerController* AtmosPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
		if (AtmosphereStart != StartsReceived) { AtmosphereStart = StartsReceived; Atmosphere.StartMatch(State, State.LobbyMap, Terrain); }
		Atmosphere.Update(DeltaSeconds, State, Terrain, AtmosPC, AtmosPC ? AtmosPC->ViewFocus() : FVector2D(State.Map.W, State.Map.H));
	}
	// client/feedback.js show: the save's sentence over the hint for 2 s (the hint is inside the HUD the lobby hides)
	for (const FTCPersistenceFeedback& F : Persistence.TakeFeedback())
	{
		UE_LOG(LogTC, Log, TEXT("saves feedback: '%s'%s (the browser plays the error blip with it)"), *F.Sentence, F.bVisible ? TEXT("") : TEXT(", not visible: the lobby hides the hint"));
		if (F.bVisible)
			if (ATCPlayerController* FPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController())) FPC->ShowFeedback(F.Sentence, F.Seconds);
	}
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (PC && WiredController.Get() != PC) WireController(PC);
	// client/endgame.js frame (an expired notice clears) and client/epilogue.js frame: once the match is decided the
	// screen's time slows to 0.35 and the camera glides to the decisive spot
	State.Objectives.Tick();
	const float ScreenDt = EpilogueFrame(DeltaSeconds, PC);
	if (Units && Effects) Units->NoteKilled(Effects->Fx.TakeKilledGone()); // main.js v.killed: a squad killed outright leaves bodies
	if (Units && State.bHasMatch) Units->Draw(State, UnitTable, Terrain, PC ? PC->Selected : TSet<int32>(), ScreenDt);
	AudioFrame(ScreenDt, PC);
	TutorialFrame(ScreenDt); // round 14: objectives.js frame (banner, smoke), Tutorial captures and switches
	RunVerificationSwitches(Now);
	RunSlice9Switches(Now);
	RunSlice10Switches(Now);
	RunSlice11Switches(Now);
	RunSlice13Switches(Now);
	// slice 13 lead: frame times while a match is on screen (the first 10 s after each start counted apart)
	if (State.bHasMatch)
	{
		if (FramesStart != StartsReceived) { FramesStart = StartsReceived; FramesStartAt = Now; }
		const double Ms = FApp::GetDeltaTime() * 1000.0;
		if (Now - FramesStartAt < 10.0) FramesSettling.Add(Ms);
		else { FramesMatch.Add(Ms); FramesWindow.Add(Ms); }
	}
	bool bStatusNow = false;
	if (Now - LastReport >= 5.0)
	{
		LastReport = Now;
		bStatusNow = true;
		int32 Own = 0;
		for (const auto& P : State.Units) if (State.IsOwn(P.Value)) Own++;
		// cells known: what the client was sent it may draw; cells drawn: what the ground mesh holds (equal once nothing is
		// queued; on a World map both stay below the map's size until the side has discovered it all)
		const int32 Known = State.Map.Known.CountSetBits();
		UE_LOG(LogTC, Log, TEXT("status: %s | room %s %s%s | match %s tick %d | units held %d (own %d, selected %d) | snapshots %d resyncs %d | opens %d | cells known %d of %d, drawn %d | chunks meshed %d queued %d | instances %d | view %s"),
			*ConnectionStatus(), *State.RoomCode, *State.RoomState, State.bSpectator ? TEXT(" spectator") : TEXT(""), State.bHasMatch ? TEXT("on") : TEXT("off"), State.Tick, State.Units.Num(), Own, PC ? PC->Selected.Num() : 0,
			State.Snapshots, State.Resyncs, Connection ? Connection->Opens() : 0, Known, State.Map.W * State.Map.H, Terrain ? Terrain->CellsDrawn() : 0,
			Terrain ? Terrain->ChunksMeshed() : 0, Terrain ? Terrain->ChunksQueued() : 0, Units ? Units->InstancesDrawn() : 0,
			!PC ? TEXT("-") : *FString::Printf(TEXT("focus (%.1f, %.1f) at %.1f m, widest %s"), PC->ViewFocus().X, PC->ViewFocus().Y, PC->ViewDistance(),
				PC->HasWideView() ? *FString::Printf(TEXT("%.1f m at (%.1f, %.1f)"), PC->WideDistance(), PC->WideFocus().X, PC->WideFocus().Y) : TEXT("not measured")));
		if (RustShadow.IsActive()) RustShadow.LogTotals(TEXT("status"));
		if (State.bHasMatch && Effects)
		{
			const FTCCombatEffects::FStats& C = Effects->Fx.Stats();
			UE_LOG(LogTC, Log, TEXT("status fx: launches %d (shooter unseen %d), contacts %d, direct fire %d (shooter unseen %d), blasts %d, wrecks %d, air %d, particles %d | shooter effects checked %d, outside the held rows %d | alerts %d, ping rings %d | audio %s"),
				C.Launches, C.LaunchesHidden + C.ShooterNotHeld, C.Contacts, C.DirectFire, C.DirectFireHidden, C.Blasts, C.Wrecks, C.AirShots, Effects->Fx.Particles().Num(),
				FxPrivacyChecked, FxPrivacyViolations, AlertSerial, AlertFeed.Pings().Rings().Num(), Audio() ? *Audio()->Stats() : TEXT("no subsystem"));
		}
	}
	if (bStatusNow && State.bHasMatch)
	{
		// the same tick as the status line: the minimap's fog and knowledge, the groups and the wallet
		ATCHUD* Hud = PC ? Cast<ATCHUD>(PC->GetHUD()) : nullptr;
		if (bMinimapCheck && Hud && !Hud->HasDrawn())
		{
			// no renderer draws the HUD: build the minimap's image and lay it out for a 1280 x 720 window so its numbers
			// and presses can still be checked
			Hud->Minimap.Refresh(State, UnitTable, true);
			Hud->Minimap.Arrange(1280.f, 720.f, State, PC->ViewYaw());
		}
		TArray<FString> Sizes;
		if (PC) for (int32 G = 1; G <= FTCControlGroups::GroupCount; G++) if (PC->Groups.Members(G).Num()) Sizes.Add(FString::Printf(TEXT("%d:%d"), G, PC->Groups.Members(G).Num()));
		double Pop = 0, Cap = 0;
		const bool bArmy = Recruit.ArmyUse(State, Pop, Cap);
		UE_LOG(LogTC, Log, TEXT("status+: %s | groups [%s] | recruit: %s, MP %.0f, army %s"), *MinimapReport(), *FString::Join(Sizes, TEXT(" ")),
			Recruit.HasSnapshot() ? (Recruit.Classic() ? TEXT("classic") : Recruit.SkirmishBase() ? TEXT("skirmish base") : TEXT("spawn")) : TEXT("no snapshot"), Recruit.Mp(),
			bArmy ? *FString::Printf(TEXT("%g/%g"), Pop, Cap) : TEXT("?"));
		UE_LOG(LogTC, Log, TEXT("status++: %s"), *SupplyReport());
		int32 Garrisoned = 0;
		for (const auto& P : State.Units) if (State.IsOwn(P.Value) && (P.Value.Flags & TCFlags::Garrisoned)) Garrisoned++;
		// no piece may stand on a cell this side does not know (a World map's undiscovered cells carry no object)
		int32 OnUnknown = 0;
		for (int32 K = 0; K < WorldObjects.ChunkCount(); K++)
			WorldObjects.ForEachShape(K, [&](const TCWorldCore::FObjShape& Sh) { if (Sh.Cell >= 0 && State.Map.Known.IsValidIndex(Sh.Cell) && !State.Map.Known[Sh.Cell]) OnUnknown++; });
		UE_LOG(LogTC, Log, TEXT("status+++: world objects: %d houses, %d pieces (%d pickable, %d on cells this side does not know), %d wrecks, instanced components %d (%d triangles, %d regions queued), grid mismatches %d | own garrisoned %d | pause %s"),
			WorldObjects.HouseCount(), WorldObjects.ShapeCount(), WorldObjects.PickableCount(), OnUnknown, WorldObjects.Wrecks().Num(), WorldView.SectionsDrawn(), WorldView.TrianglesDrawn(), WorldView.Queued(),
			WorldObjects.GridMismatches(State), Garrisoned, State.bPaused ? *State.PauseText : TEXT("running"));
		UE_LOG(LogTC, Log, TEXT("status+++: %s"), *GroundPaint.StatusLine());
		UE_LOG(LogTC, Log, TEXT("status+++: %s"), *WarMap.StatusLine()); // round 14
		UE_LOG(LogTC, Log, TEXT("status terrain: %s"), *FogFrontier.StatusLine());
		// slice 13 lead: the terrain round's views and the recipient-knowledge check (no drawn vertex on an unknown cell)
		int32 ReliefVerts = 0, ReliefUnowned = 0;
		const int32 ReliefOnUnknown = Terrain ? Terrain->DrawnOnUnknown(State, &ReliefVerts, &ReliefUnowned) : 0;
		UE_LOG(LogTC, Log, TEXT("status terrain: relief cells drawn %d of %d known (%d on the map), vertices %d (no cell %d), vertices on unknown cells %d | water %d triangles, %d builds | apron %d columns, %d shapes | railways %d halves, %d sleepers, stations %d of %d | atmosphere mist %d, banks %d, falling %d, ground mirrored %d | %s"),
			Terrain ? Terrain->CellsDrawn() : 0, State.Map.Known.CountSetBits(), State.Map.W * State.Map.H, ReliefVerts, ReliefUnowned, ReliefOnUnknown,
			Water.TrianglesDrawn(), Water.GeometryBuilds(), Apron.Columns(), Apron.Shapes(), Railways.HalfCount(), Railways.SleeperCount(), Railways.StationsBuilt(), Railways.StationCount(),
			Atmosphere.MistSheets(), Atmosphere.BankSheets(), Atmosphere.FallPoints(), Atmosphere.GroundSectionsMirrored(), *FrameTimesLine());
		FramesWindow = FFrameTimes();
	}
	ControlsFrame(); // round 15: the room directory's refresh and the -TCControlsCheck script
	if (ATCPlayerController* OrdersPC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController())) OrdersCheck.Frame(*this, *OrdersPC); // round 16
	if (ExitAfter >= 0 && Now - BeganAt >= ExitAfter)
	{
		ExitAfter = -1;
		UE_LOG(LogTC, Log, TEXT("exit switch: quitting; %s"), *FrameTimesLine());
		FPlatformMisc::RequestExit(false, TEXT("TCExitAfter"));
	}
}

int32 ATCGameMode::OwnCount(const FString& Type) const
{
	int32 N = 0;
	for (const auto& P : State.Units) if (State.IsOwn(P.Value) && P.Value.Type == Type) N++;
	return N;
}

FString ATCGameMode::MinimapReport() const
{
	const ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	const ATCHUD* Hud = PC ? Cast<ATCHUD>(PC->GetHUD()) : nullptr;
	if (!Hud || Hud->Minimap.Stats().W <= 0) return TEXT("minimap not built");
	// what the image holds against what the state says this side knows and sees: no cell this side does not know may be
	// drawn as anything but the fog's shade (a World map's undiscovered placeholders must not leak)
	const FTCMinimap& Mm = Hud->Minimap;
	const FColor Shade(TCMinimapCore::Shade, TCMinimapCore::Shade, TCMinimapCore::Shade, 255);
	int32 Unknown = 0, Leaks = 0, Visible = 0, Explored = 0;
	const int32 N = State.Map.W * State.Map.H;
	for (int32 C = 0; C < N; C++)
	{
		if (State.CellVisible(C)) Visible++;
		if (State.CellExplored(C)) Explored++;
		FColor Px;
		if (!State.Map.Known.IsValidIndex(C) || State.Map.Known[C] || !Mm.PixelOf(C, Px)) continue;
		Unknown++;
		if (Px != Shade) Leaks++;
	}
	return FString::Printf(TEXT("%s | state: %d known, %d visible, %d explored of %d | unknown cells drawn as ground: %d of %d"), *Mm.Describe(),
		State.Map.Known.CountSetBits(), Visible, Explored, N, Leaks, Unknown);
}

void ATCGameMode::RunSlice3Switches(double Now)
{
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!PC) return;
	ATCHUD* Hud = Cast<ATCHUD>(PC->GetHUD());
	const bool bReady = State.CanOrder() && State.Snapshots >= 3 && Recruit.HasSnapshot();

	// -TCSelect: one own unit of a type selected (as a click would), before the recruit presses
	if (!SelectType.IsEmpty() && !bSelectedType && bReady)
	{
		bSelectedType = true;
		int32 Id = 0;
		for (const auto& P : State.Units) if (State.IsOwn(P.Value) && P.Value.Type == SelectType && (!Id || P.Key < Id)) Id = P.Key;
		if (Id) PC->SelectOwn({ Id });
		UE_LOG(LogTC, Log, TEXT("select switch: own %s %s"), *SelectType, Id ? *FString::Printf(TEXT("unit %d selected (set directly)"), Id) : TEXT("not found"));
		RecruitNextAt = Now + 1.0; // the HUD redraws the card for the selection before any press
	}
	// -TCRecruit: open the card's group tab, then press the card, for each type in turn
	if (RecruitTypes.IsValidIndex(RecruitIndex) && bReady && (SelectType.IsEmpty() || bSelectedType))
	{
		if (RecruitNextAt < 0) RecruitNextAt = Now + 1.0;
		if (Now >= RecruitNextAt)
		{
			const FString& T = RecruitTypes[RecruitIndex];
			if (RecruitPhase == 0)
			{
				const FTCCommandCard Card = Recruit.Card(State, PC->Selected);
				int32 G = -1;
				for (int32 I = 0; I < Card.Groups.Num() && G < 0; I++) if (Card.Groups[I].Cards.ContainsByPredicate([&T](const FTCRecruitCard& C) { return C.Type == T; })) G = I;
				UE_LOG(LogTC, Log, TEXT("recruit switch: %s: card kind %s, in card group %d (%s), open group %d"), *T, LexToString(Card.Kind), G, Card.Groups.IsValidIndex(G) ? *Card.Groups[G].Name : TEXT("not on the card"), PC->RecruitGroupOpen);
				if (Card.Kind == ETCCardKind::Recruit && G >= 0 && PC->RecruitGroupOpen != G)
				{
					FTCUiCommand Tab(ETCUiAction::RecruitGroup);
					Tab.Group = G;
					PC->PressCommand(Tab);
				}
				RecruitPhase = 1;
				RecruitNextAt = Now + 0.5; // the next frames draw the opened group's cards, so the press can hit one
			}
			else
			{
				RecruitMpBefore = Recruit.Mp(); RecruitCountBefore = OwnCount(T); RecruitDeniesBefore = State.Denies;
				const FTCRecruitCheck Check = Recruit.CheckBuy(State, Recruit.RecruitAction(State, T, PC->Selected));
				UE_LOG(LogTC, Log, TEXT("recruit switch: pressing the %s card%s at tick %d: MP %.0f, own %d, the browser's check says %s"), *T, bRecruitShift ? TEXT(" with Shift") : TEXT(""), State.Tick,
					RecruitMpBefore, RecruitCountBefore, Check.bOk ? TEXT("ok") : *FString::Printf(TEXT("'%s'"), *Check.Reason));
				FTCUiCommand Card(ETCUiAction::Recruit);
				Card.Unit = T;
				const bool bSent = PC->PressCommand(Card, bRecruitShift);
				UE_LOG(LogTC, Log, TEXT("recruit switch: %s: %s (HUD note: %s)"), *T, bSent ? TEXT("sent") : TEXT("nothing sent"), *PC->LastOrderNote);
				RecruitPressedAt = Now; RecruitReports = 0; RecruitPhase = 0; RecruitIndex++;
				RecruitNextAt = Now + 2.5;
			}
		}
	}
	static const double ReportAt[] = { 0.4, 1.2, 2.3 };
	if (RecruitReports >= 0 && RecruitReports < (int32)UE_ARRAY_COUNT(ReportAt) && Now - RecruitPressedAt >= ReportAt[RecruitReports])
	{
		const FString& T = RecruitTypes[RecruitIndex - 1];
		UE_LOG(LogTC, Log, TEXT("recruit +%.1fs: %s: tick %d, MP %.0f (was %.0f), own %d (was %d), denies %d (was %d)%s"), ReportAt[RecruitReports], *T, State.Tick, Recruit.Mp(), RecruitMpBefore,
			OwnCount(T), RecruitCountBefore, State.Denies, RecruitDeniesBefore,
			State.Denies > RecruitDeniesBefore ? *FString::Printf(TEXT(", last refusal %s/%s shown as '%s'"), *State.DenyCmd, *State.DenyReason, *TCOrders::DenySentence(State.DenyReason, State.DenyCmd)) : TEXT(""));
		RecruitReports++;
	}

	// -TCCancelQueued: the first waiting queue place of the selected building's card, after the recruit presses
	if (bCancelQueued && !bCancelSent && bReady && !RecruitTypes.IsValidIndex(RecruitIndex) && RecruitNextAt >= 0 && Now >= RecruitNextAt - 1.0)
	{
		bCancelSent = true;
		const FTCCommandCard Card = Recruit.Card(State, PC->Selected);
		TArray<FString> Slots;
		const FTCQueueSlot* Pick = nullptr;
		for (const FTCQueueSlot& Q : Card.Slots)
		{
			Slots.Add(Q.Type.IsEmpty() ? FString(TEXT("-")) : FString::Printf(TEXT("%s%s"), *Q.Type, Q.bTraining ? TEXT(" (training)") : Q.Job ? *FString::Printf(TEXT(" (job %d, %s)"), Q.Job, Q.Check.bOk ? TEXT("cancellable") : *Q.Check.Reason) : TEXT("")));
			if (!Pick && Q.Job) Pick = &Q;
		}
		UE_LOG(LogTC, Log, TEXT("cancel switch: card %s, '%s', places [%s]"), LexToString(Card.Kind), *Card.QueueLine, *FString::Join(Slots, TEXT(", ")));
		if (Pick)
		{
			CancelBuilding = Card.Building; CancelMpBefore = Recruit.Mp(); CancelAt = Now;
			const FTCProductionQueue* Q = Recruit.QueueOf(Card.Building);
			CancelQueueBefore = Q ? Q->Types.Num() : 0;
			FTCUiCommand C(ETCUiAction::CancelJob);
			C.Building = Card.Building; C.Job = Pick->Job;
			PC->PressCommand(C);
		}
	}
	if (CancelAt >= 0 && Now - CancelAt >= 1.5)
	{
		CancelAt = -1;
		const FTCProductionQueue* Q = Recruit.QueueOf(CancelBuilding);
		UE_LOG(LogTC, Log, TEXT("cancel +1.5s: building %d queue %d (was %d), MP %.0f (was %.0f), denies %d%s"), CancelBuilding, Q ? Q->Types.Num() : 0, CancelQueueBefore, Recruit.Mp(), CancelMpBefore, State.Denies,
			State.Denies ? *FString::Printf(TEXT(", last refusal %s/%s"), *State.DenyCmd, *State.DenyReason) : TEXT(""));
	}

	// -TCProbeBuyDeny: a raw buy past the client's check, after any -TCRecruit presses
	if (ProbeBuyDeny.IsValidIndex(ProbeBuyIndex) && ProbeBuyAt < 0 && bReady && !RecruitTypes.IsValidIndex(RecruitIndex) && (RecruitNextAt < 0 || Now >= RecruitNextAt))
	{
		const FString& T = ProbeBuyDeny[ProbeBuyIndex++];
		ProbeBuyDenies = State.Denies; ProbeBuyAt = Now;
		const FTCRecruitCheck Check = Recruit.CheckBuy(State, Recruit.RecruitAction(State, T, PC->Selected));
		TSharedRef<FJsonObject> B = MakeShared<FJsonObject>();
		B->SetStringField(TEXT("t"), TEXT("buy")); B->SetStringField(TEXT("unit"), T);
		UE_LOG(LogTC, Log, TEXT("buy probe: { t: 'buy', unit: '%s' } sent past the client's check (which says %s), MP %.0f: %s"), *T, Check.bOk ? TEXT("ok") : *FString::Printf(TEXT("'%s'"), *Check.Reason),
			Recruit.Mp(), SendCommand(B) ? TEXT("sent") : *LastSendProblemText);
	}
	if (ProbeBuyAt >= 0 && Now - ProbeBuyAt >= 2.0)
	{
		ProbeBuyAt = -1;
		UE_LOG(LogTC, Log, TEXT("buy probe +2s: denies %d (was %d), last refusal cmd '%s' reason '%s', shown as '%s'"), State.Denies, ProbeBuyDenies, *State.DenyCmd, *State.DenyReason, *TCOrders::DenySentence(State.DenyReason, State.DenyCmd));
	}

	// -TCGroupCheck and -TCMinimapCheck: timed scripts, one step a frame and never two in consecutive frames, so a
	// simulated key press and its release are seen by different PlayerTicks
	double CX = 0, CZ = 0;
	if (bGroupCheck && !bGroupScriptMade && State.CanOrder() && State.Snapshots >= 3)
	{
		const TArray<int32> Army = ArmyIds(CX, CZ);
		if (Army.Num() >= 2)
		{
			bGroupScriptMade = true;
			const double T0 = Script.Num() ? Script.Last().At + 0.5 : 0.0;
			const int32 First = Army[0], N = Army.Num();
			auto Key = [PC](const FKey& K, bool bDown) { PC->InjectKey(K, bDown); };
			auto Groups = [PC]() { return FString::Printf(TEXT("group 1 has %d, group 2 has %d, selected %d, camera (%.1f, %.1f)"), PC->Groups.Members(1).Num(), PC->Groups.Members(2).Num(), PC->Selected.Num(), PC->ViewFocus().X, PC->ViewFocus().Y); };
			Script.Add({ T0 + 0.0, FString::Printf(TEXT("select the army of %d (set directly), simulated Ctrl+1 down"), N), [=]() { PC->SelectOwn(Army); Key(EKeys::LeftControl, true); Key(EKeys::One, true); } });
			Script.Add({ T0 + 0.15, TEXT("simulated 1 and Ctrl up"), [=]() { Key(EKeys::One, false); Key(EKeys::LeftControl, false); } });
			Script.Add({ T0 + 0.5, FString::Printf(TEXT("select unit %d alone (set directly), simulated Alt+2 down"), First), [=]() { PC->SelectOwn({ First }); Key(EKeys::LeftAlt, true); Key(EKeys::Two, true); } });
			Script.Add({ T0 + 0.65, TEXT("simulated 2 and Alt up"), [=]() { Key(EKeys::Two, false); Key(EKeys::LeftAlt, false); } });
			Script.Add({ T0 + 1.0, TEXT("clear the selection (directly), simulated 1 down"), [=]() { PC->Selected.Reset(); Key(EKeys::One, true); } });
			Script.Add({ T0 + 1.1, TEXT("simulated 1 up"), [=]() { Key(EKeys::One, false); } });
			Script.Add({ T0 + 1.5, TEXT("simulated 2 down (first tap)"), [=]() { Key(EKeys::Two, true); } });
			Script.Add({ T0 + 1.55, TEXT("simulated 2 up"), [=]() { Key(EKeys::Two, false); } });
			Script.Add({ T0 + 1.65, TEXT("simulated 2 down (second tap)"), [=]() { Key(EKeys::Two, true); } });
			Script.Add({ T0 + 1.7, TEXT("simulated 2 up"), [=]() { Key(EKeys::Two, false); } });
			Script.Add({ T0 + 2.1, TEXT("simulated 1 down (recall group 1)"), [=]() { Key(EKeys::One, true); } });
			Script.Add({ T0 + 2.2, TEXT("simulated 1 up"), [=]() { Key(EKeys::One, false); } });
			Script.Add({ T0 + 2.6, TEXT("simulated Shift+2 down (append the selection to group 2)"), [=]() { Key(EKeys::LeftShift, true); Key(EKeys::Two, true); } });
			Script.Add({ T0 + 2.7, TEXT("simulated 2 and Shift up"), [=]() { Key(EKeys::Two, false); Key(EKeys::LeftShift, false); } });
			Script.Add({ T0 + 3.1, TEXT("group summary"), [=]()
			{
				const int32 G1 = PC->Groups.Members(1).Num(), G2 = PC->Groups.Members(2).Num();
				UE_LOG(LogTC, Log, TEXT("group check: %s; expected group 1 = %d (army less the moved unit), group 2 = %d (that unit, then group 1 appended): %s"), *Groups(), N - 1, N,
					G1 == N - 1 && G2 == N && PC->Groups.Members(2).Contains(First) && !PC->Groups.Members(1).Contains(First) ? TEXT("as expected") : TEXT("NOT as expected"));
			} });
			if (ScriptT0 < 0) ScriptT0 = Now;
			UE_LOG(LogTC, Log, TEXT("group check: script of %d steps for an army of %d (unit %d moves to group 2)"), 15, N, First);
		}
	}
	if (bMinimapCheck && !bMinimapScriptMade && State.CanOrder() && State.Snapshots >= 3 && Hud && ArmyIds(CX, CZ).Num())
	{
		bMinimapScriptMade = true;
		const double T0 = Script.Num() ? Script.Last().At + 0.5 : 0.0;
		const FVector2D Centre(State.Map.W * FTCMatchState::CellMetres / 2, State.Map.H * FTCMatchState::CellMetres / 2);
		Script.Add({ T0 + 0.0, TEXT("minimap layout"), [this, PC, Hud]()
		{
			if (!Hud->HasDrawn())
			{
				Hud->Minimap.Refresh(State, UnitTable, true);
				Hud->Minimap.Arrange(1280.f, 720.f, State, PC->ViewYaw());
				UE_LOG(LogTC, Log, TEXT("minimap check: no HUD frame drawn (no renderer): laid out for 1280 x 720 instead"));
			}
			const FBox2D R = Hud->Minimap.Rect();
			UE_LOG(LogTC, Log, TEXT("minimap check: square (%.0f, %.0f) to (%.0f, %.0f) at camera yaw %.1f; %s"), R.Min.X, R.Min.Y, R.Max.X, R.Max.Y, PC->ViewYaw(), *MinimapReport());
		} });
		Script.Add({ T0 + 0.3, TEXT("minimap left press at the map centre's pixel"), [PC, Hud, Centre]()
		{
			FVector2D Px;
			if (Hud->Minimap.WorldToPixel(Centre, Px)) PC->MinimapPress(Px, false, false, false);
			else UE_LOG(LogTC, Warning, TEXT("minimap check: no layout to press"));
		} });
		Script.Add({ T0 + 1.3, TEXT("minimap right press 20 m from the army toward the centre, army selected"), [this, PC, Hud, Centre]()
		{
			double X, Z;
			const TArray<int32> Ids = ArmyIds(X, Z);
			const FVector2D From(X, Z), Target = From + (Centre - From).GetSafeNormal() * 20.0;
			FVector2D Px, Back;
			if (!Ids.Num() || !Hud->Minimap.WorldToPixel(Target, Px)) { UE_LOG(LogTC, Warning, TEXT("minimap check: no army or no layout")); return; }
			Px = FVector2D(FMath::RoundToDouble(Px.X), FMath::RoundToDouble(Px.Y)); // a mouse lands on a whole pixel
			Hud->Minimap.PixelToWorld(Px, Back);
			PC->SelectOwn(Ids);
			UE_LOG(LogTC, Log, TEXT("minimap check: %d units at (%.1f, %.1f), target (%.1f, %.1f) is pixel (%.0f, %.0f), which maps back to (%.1f, %.1f)"), Ids.Num(), X, Z, Target.X, Target.Y, Px.X, Px.Y, Back.X, Back.Y);
			const FTCMinimapPress P = PC->MinimapPress(Px, true, false, false);
			if (P.Action == ETCMinimapAction::Order) { AutoIds = Ids; AutoTarget = P.Ground; AutoMovedAt = FPlatformTime::Seconds(); bAutoMoved = true; AutoReports = 0; }
		} });
		Script.Add({ T0 + 2.3, TEXT("minimap left press at the army's pixel (the camera back to the army)"), [this, PC, Hud]()
		{
			double X, Z;
			FVector2D Px;
			if (ArmyIds(X, Z).Num() && Hud->Minimap.WorldToPixel(FVector2D(X, Z), Px)) PC->MinimapPress(FVector2D(FMath::RoundToDouble(Px.X), FMath::RoundToDouble(Px.Y)), false, false, false);
		} });
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("minimap check: script of 4 steps"));
	}
	if (ScriptT0 >= 0 && Script.Num() && Now - ScriptT0 >= Script[0].At && GFrameCounter > ScriptLastFrame + 1)
	{
		const FScriptStep Step = Script[0];
		Script.RemoveAt(0);
		ScriptLastFrame = GFrameCounter;
		UE_LOG(LogTC, Log, TEXT("script +%.2fs (frame %llu): %s"), Now - ScriptT0, (unsigned long long)GFrameCounter, *Step.What);
		Step.Run();
	}
}

// ---------- slice 1d: construction, army commands and logistics ----------

TArray<int32> ATCGameMode::OwnOfTypes(const TArray<FString>& Types) const
{
	TArray<int32> Ids;
	for (const auto& P : State.Units)
		if (State.IsOwn(P.Value) && P.Value.Hp > 0 && Types.Contains(P.Value.Type) && ATCUnitRenderer::IsDrawn(P.Value)) Ids.Add(P.Key);
	Ids.Sort();
	return Ids;
}

bool ATCGameMode::FindPlacement(const FString& Kind, FVector2D Around, FVector2D& Out) const
{
	for (double R = 4; R <= 48; R += 4)
		for (int32 K = 0; K < 16; K++)
		{
			const double A = 2 * PI * K / 16;
			const FVector2D At = Around + FVector2D(R * FMath::Cos(A), R * FMath::Sin(A));
			const FTCPlacement P = Build.Placement(State, Kind, At);
			if (P.bOk && !P.bThrows) { Out = At; return true; }
		}
	return false;
}

int32 ATCGameMode::RightClickUnit(int32 Id, const TCHAR* What)
{
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	const FTCUnit* U = State.Units.Find(Id);
	if (!PC || !U) return 0;
	const FTCUnitDef* D = UnitTable.Find(U->Type);
	const bool bFoe = State.IsFoe(U->Owner), bBuilding = D && D->bBuilding;
	FTCCursor C;
	FVector2D Screen;
	const bool bOnScreen = PC->ScreenOf(Id, Screen);
	if (bOnScreen) C = PC->CursorAt(Screen); // the right click's own pick, at the unit's drawn point
	const bool bPicked = bOnScreen && (bFoe ? C.Enemy == Id : bBuilding ? C.Building == Id && !C.Friend : C.Friend == Id);
	if (!bPicked)
	{
		C = FTCCursor();
		C.bHasGround = true; C.Ground = FVector2D(U->X, U->Z);
		if (bFoe) C.Enemy = Id; else if (bBuilding) C.Building = Id; else C.Friend = Id;
	}
	UE_LOG(LogTC, Log, TEXT("%s: right click on %s %d of seat %d at (%.1f, %.1f): %s"), What, *U->Type, Id, U->Owner, U->X, U->Z,
		bPicked ? *FString::Printf(TEXT("drawn at pixel (%.0f, %.0f), the screen pick found it (enemy %d, friend %d, building %d)"), Screen.X, Screen.Y, C.Enemy, C.Friend, C.Building)
		: bOnScreen ? *FString::Printf(TEXT("drawn at pixel (%.0f, %.0f) but the pick found enemy %d, friend %d, building %d: named directly"), Screen.X, Screen.Y, C.Enemy, C.Friend, C.Building)
		: TEXT("not drawn on screen (no renderer, or off view): named directly"));
	return PC->IssueOrder(C, false, false);
}

FString ATCGameMode::SupplyReport() const
{
	const ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	// what this seat holds of the logistics data: every reserve and truck row must be one of its own units, and no store
	// may be a foe's (the server sends none; the client keeps none)
	int32 NotOwnReserves = 0, NotOwnTrucks = 0, FoeStores = 0;
	for (const FTCLogisticsReserve& R : Logistics.Reserves()) { const FTCUnit* U = State.Units.Find(R.Id); if (!U || !State.IsOwn(*U)) NotOwnReserves++; }
	TArray<FString> TruckText;
	for (const FTCLogisticsTruck& T : Logistics.Trucks())
	{
		const FTCUnit* U = State.Units.Find(T.Id);
		if (!U || !State.IsOwn(*U)) NotOwnTrucks++;
		if (TruckText.Num() < 3) TruckText.Add(FString::Printf(TEXT("truck %d: %s"), T.Id, *FString::Join(FTCLogistics::TruckLabels(&T), TEXT("; "))));
	}
	TArray<FString> StoreText;
	for (const FTCLogisticsStore& St : Logistics.Stores())
	{
		if (State.IsFoe(St.Owner)) FoeStores++;
		if (StoreText.Num() < 6)
			StoreText.Add(FString::Printf(TEXT("%s owner %d%s: %s"), St.Id.Kind == FTCJsValue::EKind::String ? *St.Id.String : *FString::Printf(TEXT("%g"), St.Id.Number), St.Owner,
				St.bSource ? TEXT(" source") : TEXT(""), *FString::Join(FTCLogistics::StoreLabels(&St), TEXT("; "))));
	}
	return FString::Printf(TEXT("logistics %s (snapshot field %s): reserves %d (not own %d), trucks %d (not own %d) [%s], stores %d (foe %d) [%s], alerts %d | build data %s, %s | aim %s%s | army rally %s"),
		Logistics.Enabled() ? TEXT("on") : TEXT("off"), bLastSnapLogistics ? TEXT("present") : TEXT("absent"),
		Logistics.Reserves().Num(), NotOwnReserves, Logistics.Trucks().Num(), NotOwnTrucks, *FString::Join(TruckText, TEXT(" | ")),
		Logistics.Stores().Num(), FoeStores, *FString::Join(StoreText, TEXT(" | ")), SupplyAlerts.Num(),
		Build.ReferenceMatches() ? TEXT("on the pin") : TEXT("NOT on the pin"), Build.HasSnapshot() ? TEXT("placement view held") : TEXT("no placement view"),
		PC ? LexToString(PC->Aim) : TEXT("-"), PC && PC->Aim == ETCAim::Build ? *FString::Printf(TEXT(" (%s)"), *PC->AimBuild) : TEXT(""),
		bArmyRally ? *FString::Printf(TEXT("(%.1f, %.1f)"), ArmyRally.X, ArmyRally.Y) : TEXT("none"));
}

void ATCGameMode::RunSlice4Switches(double Now)
{
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!PC) return;
	ATCHUD* Hud = Cast<ATCHUD>(PC->GetHUD());
	const bool bReady = State.CanOrder() && State.Snapshots >= 3 && Recruit.HasSnapshot() && Build.HasSnapshot();
	const FVector2D Centre(State.Map.W * FTCMatchState::CellMetres / 2, State.Map.H * FTCMatchState::CellMetres / 2);
	auto Key = [PC](const FKey& K, bool bDown) { PC->InjectKey(K, bDown); };
	auto Note = [PC]() { return FString::Printf(TEXT("%s note: '%s'"), PC->bLastOrderSent ? TEXT("sent") : TEXT("not sent"), *PC->LastOrderNote); };

	// -TCBuildCheck: arm a building through the player's own path, a refused click, an accepted click, the site's progress
	if (!BuildCheckKind.IsEmpty() && !bBuildScriptMade && bReady)
	{
		const bool bClassic = TCArmyCommands::ClassicLobby(State);
		const TArray<int32> Builders = OwnOfTypes(FTCConstruction::BuilderTypes(bClassic));
		if (Builders.Num())
		{
			bBuildScriptMade = true;
			const FString K = BuildCheckKind;
			const FTCBuildDef* Def = FTCConstruction::Find(K);
			const double T0 = Script.Num() ? Script.Last().At + 0.5 : 0.0;
			Script.Add({ T0, FString::Printf(TEXT("build check: select %d builder squads (set directly)"), Builders.Num()), [this, PC, Builders, K]()
			{
				PC->SelectOwn(Builders);
				const FTCBuildCheck C = Build.CheckBuild(State, K, PC->Selected);
				const FTCBuildMenu Menu = Build.BuildMenu(State, PC->Selected);
				const FTCBuildCard Card = Build.BuildCard(State, PC->Selected);
				TArray<FString> Names;
				for (const FTCBuildButton& B : (Card.bShown ? Card.Buttons : Menu.Buttons)) Names.Add(FString::Printf(TEXT("%s%s"), *B.Kind, B.Check.bOk ? TEXT("") : *FString::Printf(TEXT(" ('%s')"), *B.Check.Reason)));
				UE_LOG(LogTC, Log, TEXT("build check: %d selected, MP %.0f; Build menu %s, Build card %s: [%s]; the browser's availability for %s says %s"), PC->Selected.Num(), Recruit.Mp(),
					Menu.bOffered ? TEXT("offered") : TEXT("not offered"), Card.bShown ? TEXT("shown") : TEXT("not shown"), *FString::Join(Names, TEXT(", ")), *K, C.bOk ? TEXT("ok") : *FString::Printf(TEXT("'%s'"), *C.Reason));
			} });
			if (bClassic && Def && !Def->Key.IsEmpty())
			{
				const FKey BuildKey(*Def->Key);
				Script.Add({ T0 + 0.6, FString::Printf(TEXT("build check: simulated %s down (the Classic build key)"), *Def->Key), [Key, BuildKey]() { Key(BuildKey, true); } });
				Script.Add({ T0 + 0.75, FString::Printf(TEXT("build check: simulated %s up"), *Def->Key), [Key, BuildKey]() { Key(BuildKey, false); } });
			}
			else
			{
				// outside Classic the button is in the Orders panel's Build menu; in Classic and World on the Build card
				if (!bClassic) Script.Add({ T0 + 0.6, TEXT("build check: press the Orders panel's Build menu button"), [PC]() { if (!PC->bBuildMenuOpen) PC->PressCommand(FTCUiCommand(ETCUiAction::BuildMenu)); } });
				Script.Add({ T0 + 1.2, FString::Printf(TEXT("build check: press the %s's %s button"), bClassic ? TEXT("Build card") : TEXT("Build menu"), *K), [PC, K]() { FTCUiCommand C(ETCUiAction::Build); C.Unit = K; PC->PressCommand(C); } });
			}
			Script.Add({ T0 + 1.8, TEXT("build check: a click where the client's placement check refuses (a spot this side has never seen)"), [this, PC, K, Note]()
			{
				UE_LOG(LogTC, Log, TEXT("build check: aim %s (%s), hint '%s'"), LexToString(PC->Aim), *PC->AimBuild, *PC->AimHint);
				// the first point of a 32 m grid the client's placement check refuses as not visible (an ally's view counts
				// as this side's, so a fixed far point is not enough with teammates)
				const double MW = State.Map.W * FTCMatchState::CellMetres, MH = State.Map.H * FTCMatchState::CellMetres;
				BuildFar = FVector2D(-1, -1);
				for (double Z = 16; Z < MH && BuildFar.X < 0; Z += 32)
					for (double X = 16; X < MW && BuildFar.X < 0; X += 32)
					{
						const FTCPlacement Q = Build.Placement(State, K, FVector2D(X, Z));
						if (!Q.bOk && !Q.bThrows && !Q.bNative && Q.Reason == TEXT("notVisible")) BuildFar = FVector2D(X, Z);
					}
				if (BuildFar.X < 0) { UE_LOG(LogTC, Warning, TEXT("build check: no spot the client refuses as not visible: refused click skipped")); return; }
				const FTCPlacement P = Build.Placement(State, K, BuildFar);
				const int32 DeniesBefore = State.Denies;
				const bool bSent = PC->AimClick(true, BuildFar, false);
				UE_LOG(LogTC, Log, TEXT("build check: refused spot (%.1f, %.1f): the client's placement check says %s; %s; %s; aim still %s; denies %d -> %d"), BuildFar.X, BuildFar.Y,
					P.bOk ? TEXT("ok") : *FString::Printf(TEXT("'%s'"), *P.Reason), bSent ? TEXT("SENT") : TEXT("nothing sent"), *Note(), LexToString(PC->Aim), DeniesBefore, State.Denies);
			} });
			Script.Add({ T0 + 2.4, TEXT("build check: find a spot the placement check accepts near the builders, pin the preview there"), [this, PC, K, Builders]()
			{
				FVector2D Around(0, 0);
				int32 N = 0;
				for (int32 Id : Builders) if (const FTCUnit* U = State.Units.Find(Id)) { Around += FVector2D(U->X, U->Z); N++; }
				if (N) Around /= N;
				if (!FindPlacement(K, Around, BuildAt)) { BuildAt = FVector2D(-1, -1); UE_LOG(LogTC, Warning, TEXT("build check: no accepted spot for %s within 48 m of (%.1f, %.1f)"), *K, Around.X, Around.Y); return; }
				const FTCPlacement P = Build.Placement(State, K, BuildAt);
				PC->PinPointerGround(BuildAt);
				UE_LOG(LogTC, Log, TEXT("build check: accepted spot (%.1f, %.1f) %.1f m from the builders: footprint cell %d size %d, builds at (%.1f, %.1f)"), BuildAt.X, BuildAt.Y, FVector2D::Distance(BuildAt, Around), P.Cell, P.Size, P.At.X, P.At.Y);
			} });
			if (!BuildShotFile.IsEmpty())
				Script.Add({ T0 + 3.0, TEXT("build check: screenshot of the armed preview"), [this]() { FScreenshotRequest::RequestScreenshot(BuildShotFile, true, false); UE_LOG(LogTC, Log, TEXT("build check: screenshot requested: %s"), *BuildShotFile); } });
			Script.Add({ T0 + 3.6, TEXT("build check: left click on the accepted spot"), [this, PC, K, Note]()
			{
				PC->ReleasePointerGround();
				if (BuildAt.X < 0) return;
				BuildMpBefore = Recruit.Mp(); BuildCountBefore = OwnOfTypes({ K }).Num(); BuildDeniesBefore = State.Denies;
				FVector2D Screen, Picked(-1, -1);
				bool bSent = false;
				const float Up = Terrain ? Terrain->HeightAt(BuildAt.X, BuildAt.Y) : 0.f;
				int32 VX = 0, VY = 0;
				PC->GetViewportSize(VX, VY);
				if (PC->ProjectWorldLocationToScreen(ATCTerrain::ToWorld(BuildAt.X, BuildAt.Y, Up), Screen) && Screen.X >= 0 && Screen.Y >= 0 && Screen.X < VX && Screen.Y < VY)
				{
					bSent = PC->AimClickAt(Screen, false, &Picked);
					UE_LOG(LogTC, Log, TEXT("build check: clicked pixel (%.0f, %.0f); the ground pick found (%.1f, %.1f), %.2f m from the spot"), Screen.X, Screen.Y, Picked.X, Picked.Y, FVector2D::Distance(Picked, BuildAt));
				}
				else
				{
					bSent = PC->AimClick(true, BuildAt, false);
					UE_LOG(LogTC, Log, TEXT("build check: the spot is not on screen (no renderer): clicked with its ground directly"));
				}
				BuildClickAt = FPlatformTime::Seconds(); BuildReports = 0;
				UE_LOG(LogTC, Log, TEXT("build check: %s at tick %d, MP %.0f, own %s %d; %s; aim now %s"), bSent ? TEXT("build sent") : TEXT("NOTHING SENT"), State.Tick, BuildMpBefore, *K, BuildCountBefore, *Note(), LexToString(PC->Aim));
			} });
			if (ScriptT0 < 0) ScriptT0 = Now;
			UE_LOG(LogTC, Log, TEXT("build check: script for %s (%s) with %d builders"), *K, bClassic && Def && !Def->Key.IsEmpty() ? TEXT("Classic key") : bClassic ? TEXT("the Engineers' Build card") : TEXT("Orders panel Build menu"), Builders.Num());
		}
	}
	static const double BuildReportAt[] = { 1, 3, 6, 12, 20, 30 };
	if (BuildClickAt >= 0 && BuildReports < (int32)UE_ARRAY_COUNT(BuildReportAt) && Now - BuildClickAt >= BuildReportAt[BuildReports])
	{
		const FTCUnit* Site = nullptr;
		for (const auto& P : State.Units)
			if (State.IsOwn(P.Value) && P.Value.Type == BuildCheckKind && (!Site || FVector2D::Distance(FVector2D(P.Value.X, P.Value.Z), BuildAt) < FVector2D::Distance(FVector2D(Site->X, Site->Z), BuildAt))) Site = &P.Value;
		UE_LOG(LogTC, Log, TEXT("build +%.0fs: tick %d, own %s %d (was %d), MP %.0f (was %.0f), denies %d (was %d)%s%s"), BuildReportAt[BuildReports], State.Tick, *BuildCheckKind, OwnOfTypes({ BuildCheckKind }).Num(), BuildCountBefore,
			Recruit.Mp(), BuildMpBefore, State.Denies, BuildDeniesBefore,
			Site ? *FString::Printf(TEXT(", nearest site unit %d at (%.1f, %.1f), %.1f m from the click, built %.2f, hp %d"), Site->Id, Site->X, Site->Z, FVector2D::Distance(FVector2D(Site->X, Site->Z), BuildAt), Site->Built, Site->Hp) : TEXT(", no site held"),
			State.Denies > BuildDeniesBefore ? *FString::Printf(TEXT(", last refusal %s/%s shown as '%s'"), *State.DenyCmd, *State.DenyReason, *TCOrders::DenySentence(State.DenyReason, State.DenyCmd)) : TEXT(""));
		BuildReports++;
	}
	// -TCBuildProbe: a build where this side has never seen, past the client's check
	if (bBuildProbe && !bBuildProbeSent && BuildClickAt >= 0 && Now - BuildClickAt >= 2.0 && BuildFar.X >= 0)
	{
		bBuildProbeSent = true;
		const TArray<int32> Builders = OwnOfTypes(FTCConstruction::BuilderTypes(TCArmyCommands::ClassicLobby(State)));
		TSharedRef<FJsonObject> B = MakeShared<FJsonObject>();
		TArray<TSharedPtr<FJsonValue>> Ids;
		for (int32 Id : Builders) Ids.Add(MakeShared<FJsonValueNumber>(Id));
		B->SetStringField(TEXT("t"), TEXT("build")); B->SetArrayField(TEXT("ids"), Ids); B->SetStringField(TEXT("kind"), BuildCheckKind);
		B->SetNumberField(TEXT("x"), BuildFar.X); B->SetNumberField(TEXT("z"), BuildFar.Y); B->SetBoolField(TEXT("queue"), false);
		const int32 Before = State.Denies;
		UE_LOG(LogTC, Log, TEXT("build probe: { t: 'build', kind: '%s', x: %.1f, z: %.1f } for %d builders past the client's check (it says '%s'): %s; denies %d"), *BuildCheckKind, BuildFar.X, BuildFar.Y, Ids.Num(),
			*Build.Placement(State, BuildCheckKind, BuildFar).Reason, SendCommand(B) ? TEXT("sent") : *LastSendProblemText, Before);
	}

	// -TCArmyCheck: stances, the rally aim and click, a building's rally, retreat refused and retreat
	double CX = 0, CZ = 0;
	if (bArmyCheck && !bArmyScriptMade && bReady && ArmyIds(CX, CZ).Num() >= 2)
	{
		bArmyScriptMade = true;
		const TArray<int32> Army = ArmyIds(CX, CZ);
		const double T0 = Script.Num() ? Script.Last().At + 0.5 : 0.0;
		auto Bits = [this, Army]()
		{
			int32 F = 0, P = 0, A = 0, Rt = 0, N = 0;
			for (int32 Id : Army) if (const FTCUnit* U = State.Units.Find(Id)) { N++; F += (U->Flags & 2048) != 0; P += (U->Flags & 4096) != 0; A += (U->Flags & 8192) != 0; Rt += (U->Flags & TCFlags::Retreating) != 0; }
			return FString::Printf(TEXT("of %d held: hold fire %d, hold position %d, auto-retreat %d, retreating %d (tick %d)"), N, F, P, A, Rt, State.Tick);
		};
		auto Chord = [this, Key](double At, const FKey& K, bool bShift, const FString& What)
		{
			Script.Add({ At, FString::Printf(TEXT("army check: simulated %s%s down (%s)"), bShift ? TEXT("Shift+") : TEXT(""), *K.ToString(), *What), [Key, K, bShift]() { if (bShift) Key(EKeys::LeftShift, true); Key(K, true); } });
			Script.Add({ At + 0.1, FString::Printf(TEXT("army check: simulated %s%s up"), bShift ? TEXT("Shift+") : TEXT(""), *K.ToString()), [Key, K, bShift]() { Key(K, false); if (bShift) Key(EKeys::LeftShift, false); } });
		};
		Script.Add({ T0, FString::Printf(TEXT("army check: select the army of %d (set directly)"), Army.Num()), [PC, Army, Bits]() { PC->SelectOwn(Army); UE_LOG(LogTC, Log, TEXT("army check: before: %s"), *Bits()); } });
		Chord(T0 + 0.2, EKeys::F, true, TEXT("hold fire"));
		Script.Add({ T0 + 1.8, TEXT("army check: report"), [Bits, Note]() { UE_LOG(LogTC, Log, TEXT("army check: after Shift+F: %s; %s"), *Bits(), *Note()); } });
		Chord(T0 + 2.0, EKeys::F, true, TEXT("hold fire again"));
		Script.Add({ T0 + 3.6, TEXT("army check: report"), [Bits, Note]() { UE_LOG(LogTC, Log, TEXT("army check: after the second Shift+F: %s; %s"), *Bits(), *Note()); } });
		Chord(T0 + 3.8, EKeys::G, true, TEXT("hold position"));
		Script.Add({ T0 + 5.4, TEXT("army check: report"), [Bits, Note]() { UE_LOG(LogTC, Log, TEXT("army check: after Shift+G: %s; %s"), *Bits(), *Note()); } });
		Chord(T0 + 5.6, EKeys::X, true, TEXT("auto-retreat"));
		Script.Add({ T0 + 7.2, TEXT("army check: report, then clear the selection (directly)"), [PC, Bits, Note]() { UE_LOG(LogTC, Log, TEXT("army check: after Shift+X: %s; %s"), *Bits(), *Note()); PC->Selected.Reset(); } });
		Chord(T0 + 7.4, EKeys::F, true, TEXT("hold fire with nothing selected: refused by the client"));
		Script.Add({ T0 + 7.8, TEXT("army check: report, select the army (directly)"), [PC, Army, Note]() { UE_LOG(LogTC, Log, TEXT("army check: Shift+F with nothing selected: %s"), *Note()); PC->SelectOwn(Army); } });
		Chord(T0 + 8.0, EKeys::H, true, TEXT("arm the rally"));
		Script.Add({ T0 + 8.5, TEXT("army check: rally click 15 m from the army toward the centre"), [this, PC, Centre, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("army check: aim %s, hint '%s'"), LexToString(PC->Aim), *PC->AimHint);
			double X, Z;
			ArmyIds(X, Z);
			const FVector2D Target = FVector2D(X, Z) + (Centre - FVector2D(X, Z)).GetSafeNormal() * 15.0;
			FVector2D Screen, Picked(-1, -1);
			int32 VX = 0, VY = 0;
			PC->GetViewportSize(VX, VY);
			bool bSent;
			if (Terrain && PC->ProjectWorldLocationToScreen(ATCTerrain::ToWorld(Target.X, Target.Y, Terrain->HeightAt(Target.X, Target.Y)), Screen) && Screen.X >= 0 && Screen.Y >= 0 && Screen.X < VX && Screen.Y < VY)
				bSent = PC->AimClickAt(Screen, false, &Picked);
			else { bSent = PC->AimClick(true, Target, false); Picked = Target; }
			UE_LOG(LogTC, Log, TEXT("army check: rally click at (%.1f, %.1f) (ground picked (%.1f, %.1f)): %s; %s; aim now %s; army rally held %s"), Target.X, Target.Y, Picked.X, Picked.Y, bSent ? TEXT("sent") : TEXT("NOTHING SENT"), *Note(), LexToString(PC->Aim),
				bArmyRally ? *FString::Printf(TEXT("(%.1f, %.1f)"), ArmyRally.X, ArmyRally.Y) : TEXT("none"));
		} });
		Script.Add({ T0 + 10.0, TEXT("army check: report the army rally, select the HQ (directly) and right-click the ground 12 m from it"), [this, PC, Centre, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("army check: army rally from the snapshot: %s; denies %d"), bArmyRally ? *FString::Printf(TEXT("(%.1f, %.1f)"), ArmyRally.X, ArmyRally.Y) : TEXT("none"), State.Denies);
			const TArray<int32> Hq = OwnOfTypes({ TEXT("hq") });
			if (!Hq.Num()) { UE_LOG(LogTC, Warning, TEXT("army check: no own HQ held")); return; }
			const FTCUnit* U = State.Units.Find(Hq[0]);
			PC->SelectOwn({ Hq[0] });
			FTCCursor C;
			C.bHasGround = true; C.Ground = FVector2D(U->X, U->Z) + (Centre - FVector2D(U->X, U->Z)).GetSafeNormal() * 12.0;
			const int32 N = PC->IssueOrder(C, false, false);
			UE_LOG(LogTC, Log, TEXT("army check: HQ %d right click at (%.1f, %.1f): %d named; %s"), Hq[0], C.Ground.X, C.Ground.Y, N, *Note());
		} });
		Chord(T0 + 11.5, EKeys::R, false, TEXT("retreat with only the HQ selected: the server refuses"));
		Script.Add({ T0 + 13.0, TEXT("army check: report, select the army (directly)"), [this, PC, Army, Note]()
		{
			const int32 Hq = PC->Selected.Num() ? *PC->Selected.CreateConstIterator() : 0;
			const FTCProductionQueue* Q = Hq ? Recruit.QueueOf(Hq) : nullptr;
			UE_LOG(LogTC, Log, TEXT("army check: HQ rally held %s; R with only the HQ: %s; denies %d, last refusal %s/%s shown as '%s'"), Q && Q->Rally.X >= 0 ? *FString::Printf(TEXT("(%.1f, %.1f)"), Q->Rally.X, Q->Rally.Y) : TEXT("none"),
				*Note(), State.Denies, *State.DenyCmd, *State.DenyReason, *TCOrders::DenySentence(State.DenyReason, State.DenyCmd));
			PC->SelectOwn(Army);
		} });
		Chord(T0 + 13.2, EKeys::R, false, TEXT("retreat the army"));
		for (int32 I = 1; I <= 4; I++)
			Script.Add({ T0 + 13.2 + 1.5 * I, TEXT("army check: report"), [this, Bits, Note, I, Army]()
			{
				double Sum = 0; int32 N = 0;
				const TArray<int32> Hq = OwnOfTypes({ TEXT("hq") });
				const FTCUnit* H = Hq.Num() ? State.Units.Find(Hq[0]) : nullptr;
				for (int32 Id : Army) if (const FTCUnit* U = State.Units.Find(Id)) if (H) { Sum += FVector2D::Distance(FVector2D(U->X, U->Z), FVector2D(H->X, H->Z)); N++; }
				UE_LOG(LogTC, Log, TEXT("army check: %.1f s after R: %s, mean distance to the HQ %.1f m; %s; denies %d"), 1.5 * I, *Bits(), N ? Sum / N : -1.0, I == 1 ? *Note() : TEXT("-"), State.Denies);
			} });
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("army check: script for an army of %d"), Army.Num());
	}

	// -TCSupplyCheck: a supply run through the right click, Resume deliveries, a refused run, the truck selection
	const TArray<int32> Trucks = OwnOfTypes({ TEXT("truck") });
	if (bSupplyCheck && !bSupplyScriptMade && bReady && Logistics.Enabled() && Trucks.Num() && ArmyIds(CX, CZ).Num())
	{
		bSupplyScriptMade = true;
		const int32 Truck = Trucks[0];
		const double T0 = Script.Num() ? Script.Last().At + 0.5 : 0.0;
		auto TruckRow = [this, Truck]()
		{
			const FTCLogisticsTruck* T = Logistics.FindTruck(Truck);
			return T ? FString::Printf(TEXT("truck %d: state %s, manual %s, hold %s, destination %s; %s"), Truck, *T->State.String, T->Manual.Truthy() ? TEXT("yes") : TEXT("no"), T->Hold.Truthy() ? TEXT("yes") : TEXT("no"),
				T->bDestination ? *FString::Printf(TEXT("(%.1f, %.1f)"), T->DestX, T->DestZ) : TEXT("none"), *FString::Join(FTCLogistics::TruckLabels(T), TEXT("; "))) : FString::Printf(TEXT("truck %d: no row"), Truck);
		};
		Script.Add({ T0, FString::Printf(TEXT("supply check: select own truck %d (set directly)"), Truck), [this, PC, Truck, TruckRow]()
		{
			PC->SelectOwn({ Truck });
			const FTCLogisticsCard Card = Logistics.Card(State, PC->Selected.Array());
			UE_LOG(LogTC, Log, TEXT("supply check: %s; logistics card %s (%d entries, trucks %s); Resume shown %s"), *TruckRow(), Card.bVisible ? TEXT("visible") : TEXT("hidden"), Card.Entries.Num(), Card.bTrucks ? TEXT("yes") : TEXT("no"),
				FTCLogistics::ResumeShown(State, PC->Selected) ? TEXT("yes") : TEXT("no"));
		} });
		// the own army unit lowest on provisions (its reserve row), the one a player would send supplies to
		auto Neediest = [this]()
		{
			double X, Z, Best = TNumericLimits<double>::Max();
			int32 Id = 0;
			for (int32 A : ArmyIds(X, Z))
				if (const FTCLogisticsReserve* R = Logistics.FindReserve(A)) if (R->Provisions.Amount() < Best) { Best = R->Provisions.Amount(); Id = A; }
			UE_LOG(LogTC, Log, TEXT("supply check: own unit lowest on provisions: %d (%s)"), Id, Id ? *FString::Join(FTCLogistics::ReserveLabels(Logistics.FindReserve(Id)), TEXT("; ")) : TEXT("no reserve rows"));
			return Id;
		};
		Script.Add({ T0 + 0.3, TEXT("supply check: point the camera at the own squad lowest on provisions (as a minimap press there would)"), [this, PC, Neediest]()
		{
			if (const FTCUnit* U = State.Units.Find(Neediest())) PC->FocusOn(FVector2D(U->X, U->Z));
		} });
		Script.Add({ T0 + 0.6, TEXT("supply check: right-click that squad"), [this, Note, Neediest]()
		{
			const int32 Target = Neediest();
			if (!Target) return;
			const int32 N = RightClickUnit(Target, TEXT("supply check"));
			UE_LOG(LogTC, Log, TEXT("supply check: %d trucks named; %s"), N, *Note());
		} });
		for (double At : { 1.0, 3.0, 6.0 })
			Script.Add({ T0 + 0.6 + At, TEXT("supply check: report"), [this, TruckRow, At]() { UE_LOG(LogTC, Log, TEXT("supply check: +%.0fs: %s; denies %d%s"), At, *TruckRow(), State.Denies, State.Denies ? *FString::Printf(TEXT(" (last %s/%s)"), *State.DenyCmd, *State.DenyReason) : TEXT("")); } });
		Script.Add({ T0 + 7.0, TEXT("supply check: press Resume deliveries"), [PC, Note]() { const bool bSent = PC->PressCommand(FTCUiCommand(ETCUiAction::Resume)); UE_LOG(LogTC, Log, TEXT("supply check: Resume deliveries %s; %s"), bSent ? TEXT("sent") : TEXT("NOT SENT"), *Note()); } });
		Script.Add({ T0 + 9.0, TEXT("supply check: report, then a supply run to a unit that does not exist, past the client's check"), [this, Truck, TruckRow]()
		{
			UE_LOG(LogTC, Log, TEXT("supply check: 2 s after Resume: %s; denies %d"), *TruckRow(), State.Denies);
			TSharedRef<FJsonObject> Sp = MakeShared<FJsonObject>();
			Sp->SetStringField(TEXT("t"), TEXT("supply")); Sp->SetArrayField(TEXT("ids"), { MakeShared<FJsonValueNumber>(Truck) }); Sp->SetNumberField(TEXT("target"), 999999); Sp->SetBoolField(TEXT("queue"), false);
			UE_LOG(LogTC, Log, TEXT("supply probe: { t: 'supply', ids: [%d], target: 999999 }: %s; the server should refuse it ('needs')"), Truck, SendCommand(Sp) ? TEXT("sent") : *LastSendProblemText);
		} });
		Script.Add({ T0 + 10.6, TEXT("supply check: point the camera at the truck (as a minimap press there would)"), [this, PC, Truck]()
		{
			if (const FTCUnit* U = State.Units.Find(Truck)) PC->FocusOn(FVector2D(U->X, U->Z));
		} });
		Script.Add({ T0 + 11.0, TEXT("supply check: report the refusal, Truck select on, drag over the whole view"), [this, PC, Note]()
		{
			UE_LOG(LogTC, Log, TEXT("supply probe +2s: denies %d, last refusal %s/%s shown as '%s'"), State.Denies, *State.DenyCmd, *State.DenyReason, *TCOrders::DenySentence(State.DenyReason, State.DenyCmd));
			PC->PressCommand(FTCUiCommand(ETCUiAction::TruckSelect));
			int32 VX = 0, VY = 0;
			PC->GetViewportSize(VX, VY);
			if (VX <= 0) { VX = 1280; VY = 720; }
			PC->BoxSelect(FVector2D(0, 0), FVector2D(VX, VY), false);
			int32 T = 0;
			for (int32 Id : PC->Selected) if (const FTCUnit* U = State.Units.Find(Id)) T += U->Type == TEXT("truck");
			UE_LOG(LogTC, Log, TEXT("supply check: Truck select %s; a drag over the view selected %d units, %d of them trucks; %s"), Logistics.SelectionModeActive() ? TEXT("on") : TEXT("off"), PC->Selected.Num(), T, *Note());
		} });
		Script.Add({ T0 + 11.6, TEXT("supply check: Truck select off, drag over the whole view again"), [this, PC]()
		{
			PC->PressCommand(FTCUiCommand(ETCUiAction::TruckSelect));
			int32 VX = 0, VY = 0;
			PC->GetViewportSize(VX, VY);
			if (VX <= 0) { VX = 1280; VY = 720; }
			PC->BoxSelect(FVector2D(0, 0), FVector2D(VX, VY), false);
			int32 T = 0;
			for (int32 Id : PC->Selected) if (const FTCUnit* U = State.Units.Find(Id)) T += U->Type == TEXT("truck");
			UE_LOG(LogTC, Log, TEXT("supply check: Truck select %s; a drag over the view selected %d units, %d of them trucks"), Logistics.SelectionModeActive() ? TEXT("on") : TEXT("off"), PC->Selected.Num(), T);
		} });
		Script.Add({ T0 + 12.2, TEXT("supply check: supply overlay on, select the trucks (directly)"), [this, PC, Trucks]()
		{
			if (!Logistics.OverlaySwitch()) PC->PressCommand(FTCUiCommand(ETCUiAction::SupplyOverlay));
			PC->SelectOwn(Trucks);
			const FTCLogisticsOverlay O = Logistics.Overlay(State, PC->Selected, [this](double X, double Z) { return FTCLogistics::KnownByFog(State, X, Z); });
			UE_LOG(LogTC, Log, TEXT("supply check: overlay %s: %d known stores, %d fully known routes for %d selected trucks"), Logistics.OverlayActive() ? TEXT("on") : TEXT("off"), O.Stores.Num(), O.Routes.Num(), PC->Selected.Num());
		} });
		if (!SupplyShotFile.IsEmpty())
			Script.Add({ T0 + 13.0, TEXT("supply check: screenshot"), [this]() { FScreenshotRequest::RequestScreenshot(SupplyShotFile, true, false); UE_LOG(LogTC, Log, TEXT("supply check: screenshot requested: %s"), *SupplyShotFile); } });
		if (ScriptT0 < 0) ScriptT0 = Now;
		UE_LOG(LogTC, Log, TEXT("supply check: script with truck %d (%d own trucks)"), Truck, Trucks.Num());
	}

	// -TCSupplyTo=ally: relief to an allied player's finished Supply Cache
	if (!SupplyTo.IsEmpty() && !bSupplyToSent && bReady && Logistics.Enabled() && Trucks.Num())
	{
		const FTCUnit* Cache = nullptr;
		for (const auto& P : State.Units)
			if (P.Value.Type == TEXT("supplycache") && P.Value.Owner != State.You && !State.IsFoe(P.Value.Owner) && P.Value.Built >= 1.f && P.Value.Hp > 0) { Cache = &P.Value; break; }
		if (Cache)
		{
			bSupplyToSent = true;
			SupplyToTarget = Cache->Id; SupplyToAt = Now; SupplyToReports = 0;
			PC->SelectOwn(Trucks);
			UE_LOG(LogTC, Log, TEXT("relief: allied seat %d's Supply Cache %d is finished; %d own trucks selected (set directly); before: %s"), Cache->Owner, Cache->Id, PC->Selected.Num(), *SupplyReport());
			const int32 N = RightClickUnit(Cache->Id, TEXT("relief"));
			UE_LOG(LogTC, Log, TEXT("relief: %d trucks named; note '%s'"), N, *PC->LastOrderNote);
		}
	}
	if (SupplyToAt >= 0 && SupplyToReports < 45 && Now - SupplyToAt >= 10.0 * (SupplyToReports + 1))
	{
		SupplyToReports++;
		FString Store = TEXT("not held");
		for (const FTCLogisticsStore& St : Logistics.Stores())
			if (St.Id.Kind == FTCJsValue::EKind::String && St.Id.String == FString::Printf(TEXT("cache:%d"), SupplyToTarget)) Store = FString::Join(FTCLogistics::StoreLabels(&St), TEXT("; "));
		TArray<FString> Rows;
		for (const FTCLogisticsTruck& T : Logistics.Trucks()) Rows.Add(FString::Printf(TEXT("%d %s%s"), T.Id, *T.State.String, T.Manual.Truthy() ? TEXT(" manual") : TEXT("")));
		UE_LOG(LogTC, Log, TEXT("relief +%ds: cache %d as this seat sees it: %s; trucks [%s]; denies %d%s"), SupplyToReports * 10, SupplyToTarget, *Store, *FString::Join(Rows, TEXT(", ")), State.Denies,
			State.Denies ? *FString::Printf(TEXT(" (last %s/%s)"), *State.DenyCmd, *State.DenyReason) : TEXT(""));
	}
}

// ---------------------------------------------------------------------------------------------------------------------
// Slice 9 glue: the alert feed and history, the controller's key hooks and the epilogue's frame.

void ATCGameMode::PushAlert(const TCHAR* Kind, const FString& Text, FVector2D At)
{
	// client/alerts.js push and client/alert-history.js add: newest first, at most 100 kept
	FTCAlertEntry E;
	E.Id = ++AlertSerial;
	E.Kind = Kind;
	E.Text = Text.Left(TCMatchLimits::Text);
	E.At = At;
	E.Born = GetWorld()->GetTimeSeconds();
	E.Tick = State.Tick;
	UE_LOG(LogTC, Log, TEXT("alert #%d at tick %d: %s (%s) at (%.1f, %.1f)"), E.Id, E.Tick, *E.Text, Kind, At.X, At.Y);
	// round 15: every alert joins the drawn feed and its history (FTCAlerts::Mirror knows the feed's own echoes)
	AlertFeed.Mirror(E.Kind, E.Text, At, E.Tick, FPlatformTime::Seconds());
	Alerts.Insert(MoveTemp(E), 0);
	if (Alerts.Num() > AlertHistoryLimit) Alerts.SetNum(AlertHistoryLimit);
	if (AlertCursor && !Alerts.ContainsByPredicate([this](const FTCAlertEntry& A) { return A.Id == AlertCursor; })) AlertCursor = 0;
	// client/alerts.js push: if (kind !== 'ping') sound(kind): at most every 1.5 s per kind, then audio.alert(kind), which
	// plays nothing for a kind outside its ALERTS table (region, supply)
	if (FCString::Strcmp(Kind, TEXT("ping")) != 0)
		if (UTCAudio* A = Audio()) A->FeedAlert(Kind);
}

void ATCGameMode::TakePointAlerts()
{
	// client/alerts.js snapshot, the points block, against the last snapshot received. No alerts once decided, for a
	// seat that is out, or without a side of its own (a spectator, an unconfirmed seat): friend() needs one.
	const TArray<FTCPointRow>& Rows = State.Points;
	const bool bOut = State.OutSeats.IsValidIndex(State.You) && State.OutSeats[State.You];
	if (bHasPrevPoints && State.SeatConfirmed() && State.Winner < -1 && !bOut)
	{
		const int32 Mine = State.TeamOf(State.You);
		auto Friend = [this, Mine](int32 Slot) { return Slot >= 0 && State.TeamOf(Slot) == Mine; };
		const TArray<FTCCapturePoint>& Pts = State.Objectives.Points();
		for (int32 I = 0; I < Rows.Num(); I++)
		{
			if (!PrevPoints.IsValidIndex(I) || !Pts.IsValidIndex(I)) continue;
			const FTCPointRow& Was = PrevPoints[I];
			const FTCPointRow& R = Rows[I];
			const FVector2D At = Pts[I].Centre;
			const bool bHad = Friend(Was.Owner), bHas = Friend(R.Owner);
			if (bHad && !bHas) PushAlert(TEXT("pointLost"), TEXT("Point lost"), At);
			else if (!bHad && bHas)
				PushAlert(TEXT("pointWon"), R.Owner == State.You ? FString(TEXT("Point captured"))
					: FString::Printf(TEXT("%s captured a point"), State.Names.IsValidIndex(R.Owner) ? *State.Names[R.Owner] : TEXT("An ally")), At);
			else if (bHas && R.Progress < Was.Progress)
			{
				// underAttack: one per 30 m per 20 s, none for a spot on screen (the shared throttle the regions use)
				const bool bOnScreen = SpotOnScreen(At.X, At.Y);
				if (AttackQuiet.Allow(FPlatformTime::Seconds(), At.X, At.Y, bOnScreen)) PushAlert(TEXT("attack"), TEXT("Point under attack"), At);
			}
			else if (bHas && R.bCut && !Was.bCut) PushAlert(TEXT("pointLost"), TEXT("Point cut off: it pays nothing until the road to it is open"), At);
		}
	}
	PrevPoints = Rows;
	bHasPrevPoints = State.bHasMatch;
}

bool ATCGameMode::NewestAlertAt(FVector2D& Out) const
{
	// client/alerts.js newest: among the lines still showing (each kind's life), a unit loss first, then the newest with
	// a place (round 15: the feed's own lines, every alert mirrored into them)
	return AlertFeed.Newest(Out);
}

bool ATCGameMode::VisitAlert(int32 Step, FVector2D& Out)
{
	// client/alerts.js navigate: alert-history.js navigate over the entries with a place (from the cursor, none: the
	// newest; +1 older, -1 newer, clamped), the box opens, and the camera visits it (round 15: the feed's history)
	const FTCAlertHistoryEntry* E = AlertFeed.Navigate(Step);
	bAlertHistoryOpen = AlertFeed.bHistoryOpen;
	if (!E) return false;
	AlertCursor = E->Id;
	Out = E->At;
	UE_LOG(LogTC, Log, TEXT("alert history: visiting #%d '%s' at (%.1f, %.1f)"), E->Id, *E->Text, Out.X, Out.Y);
	return true;
}

void ATCGameMode::WireController(ATCPlayerController* PC)
{
	// the hooks the gap 13 keys call (ATCPlayerController): Space, Alt+H, Alt+Left/Right and the idle keys
	WiredController = PC;
	TWeakObjectPtr<ATCGameMode> Self(this);
	PC->NewestAlertAt = [Self](FVector2D& OutAt) { return Self.IsValid() && Self->NewestAlertAt(OutAt); };
	PC->ToggleAlertHistory = [Self]()
	{
		if (!Self.IsValid()) return;
		Self->AlertFeed.ToggleHistory(); // alerts.js toggleHistory (round 15: the feed's box)
		Self->bAlertHistoryOpen = Self->AlertFeed.bHistoryOpen;
		UE_LOG(LogTC, Log, TEXT("alert history %s (%d entries)"), Self->bAlertHistoryOpen ? TEXT("open") : TEXT("closed"), Self->AlertFeed.History().Entries().Num());
	};
	PC->VisitAlert = [Self](int32 Step, FVector2D& OutAt) { return Self.IsValid() && Self->VisitAlert(Step, OutAt); };
	PC->PlanKindOf = [Self](int32 Id, int32& OutKind)
	{
		const int32* K = Self.IsValid() ? Self->PlanKinds.Find(Id) : nullptr;
		if (!K) return false;
		OutKind = *K;
		return true;
	};
	UE_LOG(LogTC, Log, TEXT("controller hooks wired: newest alert, alert history, plan kinds"));
}

float ATCGameMode::EpilogueFrame(float DeltaSeconds, ATCPlayerController* PC)
{
	FTCObjectives& O = State.Objectives;
	// the sounds the ending (or a Horde wave's horn) asks for: epilogue.js and endgame.js audio.play(name, null, opts)
	const TArray<FTCSoundCue> Cues = O.TakeSounds();
	for (const FTCSoundCue& C : Cues)
		UE_LOG(LogTC, Log, TEXT("sound cue: %s delay %.2f gain %.2f rate %.2f (to the mixer)"), *C.Name, C.Delay, C.Gain, C.Rate);
	if (Cues.Num())
		if (UTCAudio* A = Audio()) A->Cues(Cues);
	if (!O.Epilogue().bActive || !State.bHasMatch) return DeltaSeconds;
	if (!bEpilogueLogged)
	{
		bEpilogueLogged = true;
		EpilogueSeenAt = FPlatformTime::Seconds();
		const FTCEpilogue& E = O.Epilogue();
		UE_LOG(LogTC, Log, TEXT("epilogue at tick %d: '%s' / '%s' (winner %d, reason %s, my team %d), glide to (%.1f, %.1f)"), State.Tick, *E.Word, *E.Why, State.Winner, *State.End.Reason,
			State.TeamOf(State.You), E.EndAt.X, E.EndAt.Y);
	}
	FTCEpilogueCamera Cam;
	if (PC) { Cam.X = PC->ViewFocus().X; Cam.Z = PC->ViewFocus().Y; Cam.Dist = PC->ViewDistance(); }
	const bool bGliding = O.Epilogue().Elapsed < TCObjectiveRules::Glide;
	const double Screen = O.EpilogueFrame(DeltaSeconds, Cam);
	if (PC && bGliding) PC->EpilogueCamera(FVector2D(Cam.X, Cam.Z), (float)Cam.Dist);
	return (float)Screen;
}

// -TCWaterShots=<dir>: poses the camera over the nearest known water, at each edge looking out, at a corner and at the
// widest zoom on three headings, and saves a frame without the HUD at each (2.5 s apart, so the view settles).
void ATCGameMode::WaterShotsTick()
{
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr);
	if (!PC || !State.bHasMatch || State.Map.W <= 0) return;
	const double Now = FPlatformTime::Seconds();
	const float Wm = State.Map.W * 2.f, Hm = State.Map.H * 2.f;
	if (WaterShotStep < 0) { WaterShotStep = 0; WaterShotAt = Now + 6.0; return; } // the ground and textures settle first
	if (Now < WaterShotAt || FScreenshotRequest::IsScreenshotRequested()) return;
	// nearest known water cell to the map's centre
	FVector2D WaterAt(Wm / 2, Hm / 2);
	double Best = 1e30;
	for (int32 C = 0; C < State.Map.W * State.Map.H; C++)
		if (State.Map.Known[C] && TCWaterCore::IsWet(State.Map.Chars[C]))
		{
			const FVector2D P((C % State.Map.W + 0.5) * 2.0, (C / State.Map.W + 0.5) * 2.0);
			const double D = FVector2D::DistSquared(P, FVector2D(Wm / 2, Hm / 2));
			if (D < Best) { Best = D; WaterAt = P; }
		}
	struct FPose { const TCHAR* Name; FVector2D Focus; float Yaw, Dist; };
	const FPose Poses[] = {
		{ TEXT("water-close"), WaterAt, -90.f, 40.f }, { TEXT("water-gameplay"), WaterAt, -90.f, 85.f }, { TEXT("water-wide"), WaterAt, -60.f, 200.f },
		{ TEXT("edge-north-looking-out"), FVector2D(Wm / 2, 6), -90.f, 85.f }, { TEXT("edge-east-looking-out"), FVector2D(Wm - 6, Hm / 2), 0.f, 200.f },
		{ TEXT("edge-south-looking-out"), FVector2D(Wm / 2, Hm - 6), 90.f, 140.f }, { TEXT("corner-southwest-diagonal"), FVector2D(6, Hm - 6), 135.f, 160.f },
		{ TEXT("widest-heading-north"), FVector2D(Wm / 2, Hm / 2), -90.f, 100000.f }, { TEXT("widest-heading-northeast"), FVector2D(Wm / 2, Hm / 2), -45.f, 100000.f },
		{ TEXT("widest-heading-west"), FVector2D(Wm / 2, Hm / 2), 180.f, 100000.f } };
	const int32 Count = UE_ARRAY_COUNT(Poses);
	// even steps pose the camera, odd steps take the frame of the pose before
	const int32 I = WaterShotStep / 2;
	if (I >= Count) { if (WaterShotStep == 2 * Count) { UE_LOG(LogTC, Log, TEXT("water shots: done (%d)"), Count); WaterShotStep++; } return; }
	if (AHUD* Hud = PC->GetHUD()) Hud->bShowHUD = false;
	if (WaterShotStep % 2 == 0)
	{
		PC->FocusOn(Poses[I].Focus);
		PC->SetViewYawForChecks(Poses[I].Yaw);
		PC->ZoomTo(Poses[I].Dist);
		WaterShotAt = Now + 2.5;
	}
	else
	{
		const FString File = FPaths::Combine(WaterShotsDir, FString::Printf(TEXT("%s-%s.png"), *State.Map.Name.Replace(TEXT(" "), TEXT("-")).ToLower(), Poses[I].Name));
		FScreenshotRequest::RequestScreenshot(File, false, false);
		UE_LOG(LogTC, Log, TEXT("water shots: %s: focus (%.1f, %.1f) yaw %.0f distance %.0f m (asked %.0f); water %d triangles, %d geometry builds; apron %d columns, %d shapes"),
			*File, Poses[I].Focus.X, Poses[I].Focus.Y, PC->ViewYaw(), PC->ViewDistance(), Poses[I].Dist, Water.TrianglesDrawn(), Water.GeometryBuilds(), Apron.Columns(), Apron.Shapes());
		WaterShotAt = Now + 0.5;
	}
	WaterShotStep++;
}

// Slice 13 lead: frame times as the status line and the exit switch print them (milliseconds)
FString ATCGameMode::FrameTimesLine() const
{
	return FString::Printf(TEXT("frame ms: match %lld frames mean %.2f worst %.1f | last window %lld mean %.2f worst %.1f | settling %lld mean %.2f worst %.1f"),
		FramesMatch.N, FramesMatch.Mean(), FramesMatch.Worst, FramesWindow.N, FramesWindow.Mean(), FramesWindow.Worst, FramesSettling.N, FramesSettling.Mean(), FramesSettling.Worst);
}
