// The lobby's match options that FTCLobbySettings leaves out: the Weather select, the Sandbox switch, the seat
// remove (kick) buttons, and a read-only model of the lobby's map preview. Ported from the pinned reference (e4e579a):
//   server.js              lobby() (the 'weather', 'sandbox', 'players', 'amHost', 'state', 'role' fields), the
//                          'weather', 'sandbox' and 'kick' handlers (who may send them, when, and which values),
//                          hostOf/isHost (a spectator hosts a room whose seats are all AI), connected(), the
//                          /maps/<name> route and MAP_NAME; an operative's messages stop before these handlers.
//   client/main.js         renderLobby (the sandbox checkbox, the kick button and its title), previewMap (the map
//                          preview: pixels, capture points, spawns, the info lines, the World Conquest placeholder),
//                          MM_COLORS, COLORS (six seats).
//   client/weather-view.js LABEL, the Weather select's options, its tooltip (explain), the Map default option's text.
//   client/index.html      the Weather and Sandbox rows (always shown), the Sandbox tooltip, the 280 x 280 canvas.
//   shared/weather.js      WEATHER, WEATHER_CHOICES, MOOD_NAMES, MOOD_RULES, mapMood, mapWeather, weatherEffects.
//   shared/sim.js          CELL 2, CFG.pointRadius 8, levelOf, validateMap (the checks on the map fields read here).
//
// Facts come only from the lobby message and from a map file the caller hands over (the body of GET /maps/<name>):
// nothing is kept from an earlier room or guessed. Every message built is one the browser sends from its lobby, field
// for field. As with FTCLobbySettings, the server answers none of these with a deny: one it takes comes back as a
// new lobby for everyone (even when the value did not change), one it drops gets no answer at all. A sent request so
// stays pending until a lobby shows it, it is replaced, the room starts a match, or the socket reopens.
//
// Deliberate differences from the browser, all on the side of showing less rather than something stale:
//   - The Map default option names the plan of the lobby's current map only once that map's file is here. The
//     browser keeps naming the last previewed map's plan until the new file loads (and in World Conquest, forever).
//   - Preview() says the file is still needed instead of showing the previous map, as the browser does meanwhile.
//   - No kick is planned while another kick is pending: removing a seat moves every later seat down one, so a second
//     slot number picked from the old lobby could name a different seat by the time the server reads it.
//   - A map file whose fields fail validateMap's checks (on the fields read here) is refused with the reason; the
//     browser draws whatever the file holds.
// This module does no HTTP: it names the path and URL to fetch (MapPath, MapUrl) and takes the parsed body.
// ApplyLobby logs nothing. Interim until the Rust server and its contract carry the lobby.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCLobbySettings.h" // FTCLobbyChoice
#include "UObject/StrongObjectPtr.h"

class FJsonObject;
class UCanvas;
class UFont;
class UTexture2D;

enum class ETCMatchOption : uint8
{
	Weather,  // { t: 'weather', v }: 'map', 'clear', 'fog', 'rain', 'mud', 'snow' or 'random' (the host, in the lobby)
	Sandbox,  // { t: 'sandbox', v: true | false } (the host, in the lobby)
	Kick,     // { t: 'kick', slot }: an AI seat or an offline human's seat (the host, in the lobby)
};
const TCHAR* LexToString(ETCMatchOption Option);

// One seat as the lobby lists it: only what the kick rules read.
struct FTCOptionSeat
{
	FString Name;
	bool bAi = false;
	bool bConnected = false;  // an AI counts as connected (server.js: connected(q) || !!q.ai)
};

// A map's own weather (shared/weather.js mapWeather): Now all match, or Now until AtSeconds and then Next.
struct FTCWeatherPlan
{
	FString Now;              // a WEATHER key; empty when there is no plan
	FString Next;             // empty when nothing changes
	int32 AtSeconds = 0;      // seconds into the match (mapWeather's at, before planWeather turns it into ticks)
	bool IsSet() const { return !Now.IsEmpty(); }
};

// One option's control: whether it shows, whether this client may change it now, its value, and its options.
struct FTCOptionControl
{
	ETCMatchOption Kind = ETCMatchOption::Weather;
	int32 Slot = -1;              // Kick: the seat; -1 otherwise
	FString Title;                // 'Weather', 'Sandbox', or the kick button's text ('Remove')
	FString Tooltip;              // the browser's title on the control (the weather explained, the Sandbox line, ...)
	bool bShown = false;          // the browser shows it (Weather and Sandbox always; a kick button only for the host
	                              // in the lobby, on an AI or offline seat)
	bool bEditable = false;       // this client may change it now: the server would act on it, and the browser enables it
	FString Why;                  // not shown or not editable: why, in this client's words
	FString CurrentValue;         // wire form: the weather choice, 'true'/'false' for Sandbox, the slot for Kick
	FString CurrentLabel;
	TArray<FTCLobbyChoice> Choices; // Weather: the seven choices in the browser's order; empty otherwise
	bool bChecked = false;        // Sandbox: the lobby's switch
	FString SeatName;             // Kick: the seat's name, and whether it is an AI
	bool bSeatAi = false;
	// A request sent and not yet shown by a lobby (FTCMatchOptions::NoteSent).
	bool bPending = false;
	FString PendingValue;
	double PendingSentAt = 0.0;

	const FTCLobbyChoice* Find(const FString& Value) const;
};

// What a change would send.
struct FTCOptionRequest
{
	ETCMatchOption Kind = ETCMatchOption::Weather;
	int32 Slot = -1;                 // Kick: the seat asked for
	FString Value;                   // wire form ('fog', 'true', the slot)
	TSharedPtr<FJsonObject> Message; // the one message to send; null when nothing goes out
	FString Note;                    // what was asked, or why nothing goes out
	FString SeatName;                // Kick: the seat removed, to know it is gone when a lobby no longer lists it
	bool bSeatAi = false;
};

// One map file, as GET /maps/<name> serves it (shared/world-layers.js mapForClient), reduced to what the preview reads.
struct FTCMapPreviewSource
{
	struct FPoint { int32 X = 0, Y = 0; double Mp = 1.0; }; // cell; mp: what holding it gives (the default is 1)
	struct FSpawn { int32 X = 0, Y = 0; bool bAssault = false; }; // cell; bAssault: used only in Assault
	FString Key;                  // the file's name as the lobby lists it ('hill-112-xl')
	FString Name;                 // the map's own name ('Hill 112 XL')
	int32 W = 0, H = 0;           // cells
	TArray<uint8> Cells;          // W * H terrain characters, row by row from the top
	TArray<int8> Levels;          // W * H height levels (levelOf: '0'..'4', 'a' -1, 'b' -2); 0 where the map has none
	int32 TopLevel = 0;           // the highest level (0 without heights), for 'rolling hills' / 'hills and cliffs'
	bool bWater = false;          // some row holds a 'W' cell, for 'rivers'
	TArray<FPoint> Points;
	TArray<FSpawn> Spawns;
	// ABI IF-12 results cached once per public map, before rendering. Empty means the explicit C++ fallback.
	TMap<FString, TArray<int32>> RustSpawnIndices;
	bool bHasDefend = false;      // the map lists defender spawns (Horde-capable, 'built for Assault')
	TArray<int32> Defend;         // spawn numbers
	int32 AssaultTime = 0;        // seconds; 0 when the map has no Assault clock of its own
	FTCWeatherPlan Weather;       // mapWeather(map, key): what Map default brings here
};

// What the lobby's map preview shows (client/main.js previewMap), for one map and mode.
enum class ETCPreviewStatus : uint8
{
	NoLobby,    // no lobby yet, or the lobby names no map
	World,      // World Conquest: the "Uncharted continent" placeholder, no file needed
	NeedsFile,  // the map's file is not here yet: fetch MapPath(Key) and hand it to AddMapFile
	Ready,
};
enum class ETCPreviewSpawnRole : uint8
{
	Neutral,    // '#f2ecd8'
	Defend,     // '#d94a3d': Assault's defenders, Horde's HQ spots
	Attack,     // '#3d7bd9': Assault's attackers, where Horde's waves come from
};
struct FTCMapPreviewSpawn
{
	int32 Index = 0;              // the spawn's number in the map file
	int32 X = 0, Y = 0;           // cell
	ETCPreviewSpawnRole Role = ETCPreviewSpawnRole::Neutral;
	FColor Color = FColor(0, 0, 0, 0);
};
struct FTCMapPreviewPoint
{
	int32 Index = 0;              // the point's number in the map file
	int32 X = 0, Y = 0;           // cell
};
// One line under the preview. Text is plain (the browser's HTML escaped away); each U+25CF dot in it takes the next
// colour of Dots.
struct FTCMapPreviewLine
{
	FString Text;
	bool bBold = false;           // the map's name
	TArray<FColor> Dots;
};
struct FTCMapPreview
{
	// Where the browser draws on its canvas (client/index.html mapCanvas, 280 x 280, shown at 150 x 150 CSS pixels).
	struct FLayout
	{
		double Scale = 0.0;       // canvas pixels per cell: width / max(W, H)
		double OffsetX = 0.0, OffsetY = 0.0;
		double PointRadius = 0.0; // capture point ring: max(4, CFG.pointRadius / CELL * Scale), stroked 2 px
		double SpawnRadius = 6.0; // spawn dot, filled, stroked 2 px
	};

	ETCPreviewStatus Status = ETCPreviewStatus::NoLobby;
	FString Key;                  // the map asked for ('world' in World Conquest)
	FString Mode;                 // the mode it is drawn for ('conquest' when the lobby named none)
	FColor Background = FColor(0, 0, 0, 0); // the canvas behind the map: '#11110d' (World: '#101412')
	// World: the placeholder's two lines, centred, in '#d6b25e' (18 px, then 13 px 26 px lower)
	FString Title, Subtitle;
	FColor TitleColor = FColor(0, 0, 0, 0);
	// Ready: the map's cells, row by row from the top, W * H, opaque (the browser's ImageData, drawn unsmoothed)
	int32 W = 0, H = 0;
	TArray<FColor> Pixels;
	TArray<FTCMapPreviewPoint> Points; // drawn first, as rings in PointColor
	FColor PointColor = FColor(0, 0, 0, 0);
	TArray<FTCMapPreviewSpawn> Spawns; // then the spawns, outlined in SpawnOutline
	FColor SpawnOutline = FColor(0, 0, 0, 0);
	TArray<FTCMapPreviewLine> Info;    // the lines beside the preview (World: one line)

	FLayout Layout(double CanvasWidth = 280.0, double CanvasHeight = 280.0) const;
	// The canvas position of a cell's centre.
	static FVector2D CellCentre(int32 X, int32 Y, const FLayout& At);
	// The original 280 x 280 canvas, including its point rings and outlined spawn dots.
	TArray<FColor> Raster(int32 Side = 280) const;
};

// A map's retry delay must not postpone a different map selected in the lobby.
struct FTCMapPreviewRetry
{
	bool Due(const FString& Key, double Now) const
	{
		const double* At = Next.Find(Key);
		return !Key.IsEmpty() && (!At || Now >= *At);
	}
	void Tried(const FString& Key, double Now) { Next.Add(Key, Now + 5.0); }
	void Reset() { Next.Reset(); }
private:
	TMap<FString, double> Next;
};

class FTCMatchOptions
{
public:
	void TickPreview(const FString& SocketUrl, double Now);
	void DrawPreview(UCanvas* Canvas, UFont* Font, FVector2D TopLeft, float Side = 150.f);
	static constexpr const TCHAR* ReferencePin = TEXT("e4e579a");
	static constexpr int32 MaxPlayers = 6;   // shared/sim.js MAX_PLAYERS (client/main.js COLORS.length)
	static constexpr int32 CellMetres = 2;   // shared/sim.js CELL
	static constexpr int32 PointRadiusMetres = 8; // shared/sim.js CFG.pointRadius

	// Reads a lobby message. False when it is not one or is malformed (missing state or players, a seat that is not
	// an object): nothing changes then. A lobby in play forgets every pending request; any other lobby resolves the
	// pending requests whose value it shows.
	bool ApplyLobby(const FJsonObject& Message);
	// Forgets the room (a new server or room). Map files stay: they belong to the server, not the room.
	void Reset();
	// A new socket opened: pending requests are forgotten, since the server may never have read them.
	void OnSocketOpened() { Pending.Reset(); }

	bool HasLobby() const { return bHasLobby; }
	bool InLobby() const { return bHasLobby && State == TEXT("lobby"); }
	bool AmHost() const { return bHasLobby && bAmHost; }
	// The lobby's weather choice as the browser shows it ('map' when the lobby's value is not one of the choices).
	FString WeatherChoice() const;
	bool IsRustHost() const { return bRustHost; }
	// Owner decision 097 requires logistics off. The lobby does not prove the actual host profile.
	// Requirement and uncertainty only, never sent as a room option or localized with invented text.
	FString LogisticsLine() const { return bRustHost ? FString(TEXT("R1 requires logistics off; host unverified.")) : FString(); }
	bool SandboxOn() const { return bHasLobby && bSandbox; }
	const TArray<FTCOptionSeat>& Seats() const { return SeatList; }
	// Requests the lobby has shown since this object was made, for logs.
	int32 Confirmed() const { return ConfirmedCount; }
	int32 PendingCount() const { return Pending.Num(); }

	// Weather and Sandbox ignore Slot; Kick needs a seat that exists.
	FTCOptionControl Control(ETCMatchOption Kind, int32 Slot = -1) const;
	// One kick control per seat, in seat order (bShown says which seats have the button).
	TArray<FTCOptionControl> KickControls() const;

	FTCOptionRequest PlanWeather(const FString& Choice) const;
	// For a cycling button: the next (Direction > 0) or previous choice after the one displayed, wrapping around.
	FTCOptionRequest PlanWeatherStep(int32 Direction = 1) const;
	FTCOptionRequest PlanSandbox(bool bOn) const;
	// The checkbox's click: the opposite of what it displays (the pending value if there is one, else the lobby's).
	FTCOptionRequest PlanSandboxToggle() const;
	FTCOptionRequest PlanKick(int32 Slot) const;
	// Remember a request that was sent (only when its Message actually went out), until a lobby shows it.
	void NoteSent(const FTCOptionRequest& Request, double Now);

	// The map preview. Map files are kept by name (as the browser's mapCache keeps them) until ForgetMapFiles.
	// AddMapFile: false (with the reason) when the key is not a map name the server serves or the file fails the checks.
	bool AddMapFile(const FString& Key, const FJsonObject& Map, FString* OutError = nullptr);
	bool AddMapFileText(const FString& Key, const FString& Body, FString* OutError = nullptr);
	bool HasMapFile(const FString& Key) const { return MapFiles.Contains(Key); }
	const FTCMapPreviewSource* MapFile(const FString& Key) const;
	void ForgetMapFiles() { MapFiles.Reset(); }
	// The lobby's map when its preview needs a file that is not here yet; empty otherwise (no lobby, World Conquest,
	// already here, or a name the server would refuse).
	FString WantedMap() const;
	// The preview for the lobby's map and mode.
	FTCMapPreview Preview() const;

	// server.js MAP_NAME (/^[a-z0-9-]{1,32}$/): the only names /maps/<name> serves.
	static bool IsMapName(const FString& Key);
	// '/maps/<name>' (client/main.js fetches `/maps/${encodeURIComponent(name)}`, which leaves a valid name as it is);
	// empty for a name the server would refuse.
	static FString MapPath(const FString& Key);
	// The same path on the server a socket URL names: ws://host:port/ws becomes http://host:port/maps/<name> (wss: https).
	// Empty when the URL is not ws:// or wss:// or the name is refused.
	static FString MapUrl(const FString& SocketUrl, const FString& Key);
	// Reads a map file. False with the reason when a field the preview reads fails validateMap's check of it.
	static bool ParseMapFile(const FString& Key, const FJsonObject& Map, FTCMapPreviewSource& Out, FString& OutError);
	// The preview of Source in Mode ('' reads as 'conquest'). WorldSize is read only in World Conquest, where Source
	// is not needed. Source null outside World: NeedsFile.
	static FTCMapPreview BuildPreview(const FTCMapPreviewSource* Source, const FString& Key, const FString& Mode, const FString& WorldSize);

	// Weather text, as the browser writes it.
	static const TArray<FString>& WeatherChoices();        // shared/weather.js WEATHER_CHOICES, in order
	static bool IsWeatherKind(const FString& Kind);         // a key of WEATHER ('clear' ... 'snow')
	static FString WeatherChoiceLabel(const FString& Choice); // weather-view.js LABEL ('Map default', 'Ground fog', ...)
	static FString WeatherName(const FString& Kind);        // WEATHER[kind].name; the key itself when unknown
	static TArray<FString> WeatherEffects(const FString& Kind); // weatherEffects: 'sight -30%', ... (Clear's for unknown)
	static FString WeatherSaid(const FString& Kind);        // weather-view.js said: 'Ground fog: sight -30%'
	// weather-view.js explain: the Weather select's tooltip for Choice. MapPlan: the map's own plan, if known.
	static FString WeatherExplain(const FString& Choice, const FTCWeatherPlan* MapPlan);
	// weather-view.js planShort ('Fog until 4:00', 'Snow') and the Map default option's text.
	static FString PlanShort(const FTCWeatherPlan& Plan);
	static FString MapDefaultLabel(const FTCWeatherPlan* MapPlan);
	// weather-view.js clock: m:ss (seconds rounded as Math.round does)
	static FString Clock(double Seconds);
	// shared/weather.js mapMood and mapWeather. Mood and WeatherField: the map file's 'mood' and 'weather' strings
	// (empty when absent or not strings); Name: its 'name'.
	static FString MapMood(const FString& Name, const FString& Key, const FString& Mood);
	static FTCWeatherPlan MapWeather(const FString& Name, const FString& Key, const FString& Mood, const FString& WeatherField);

	static const TCHAR* MessageType(ETCMatchOption Kind);

private:
	struct FDownload { FString Key, Body; bool bDone = false, bOk = false; };
	TSharedPtr<FDownload> Download;
	FTCMapPreviewRetry MapRetry;
	FString PreviewServer, PreviewImageKey;
	TStrongObjectPtr<UTexture2D> PreviewImage;
	struct FPending
	{
		ETCMatchOption Kind = ETCMatchOption::Weather;
		FString Value;
		// Kick: the slot asked for, the seat removed, and how many seats had that name and kind when it went out
		int32 Slot = -1;
		FString SeatName;
		bool bSeatAi = false;
		int32 SameSeats = 0;
		double SentAt = 0.0;
	};

	bool bHasLobby = false;
	FString State, Role, Mode, MapName, WorldSize;
	FString Weather;              // the lobby's 'weather' as sent
	bool bSandbox = false;
	bool bRustHost = false;
	bool bAmHost = false, bSpectator = false;
	int32 You = -1;
	TArray<FTCOptionSeat> SeatList;
	TArray<FPending> Pending;
	int32 ConfirmedCount = 0;
	TMap<FString, TSharedRef<const FTCMapPreviewSource>> MapFiles;

	const FPending* FindPending(ETCMatchOption Kind) const;
	int32 CountSeats(const FString& Name, bool bAi) const;
	bool AnyKickable(const FString& Name, bool bAi) const;
	// Why nothing may be changed in this room now (Kick: also no lobby), or empty.
	FString RoomLock() const;
	// The current map's plan, when its file is here and the mode is not World Conquest.
	const FTCWeatherPlan* CurrentMapPlan() const;
};
