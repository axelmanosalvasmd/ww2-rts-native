// The room directory inside the client (parity CLI-01, CLI-02 and the listing of CLI-04): the reference's public match
// page, client/public-lobby.html and client/public-lobby.js at FTCRoomBrowser::ReferencePin, fed by what the reference
// server exposes, and the room address rules of client/room-session.js. Ported from:
//   server.js               GET /api/rooms (the listed rooms with a connected human: code, title, mode, map, state,
//                           worldSize in World Conquest, humans, ai, occupied, capacity, joinable), the hello's
//                           listing (read only when the hello makes a new room: public, title cut to 48 and cleaned,
//                           tutorial), cleanName (the name the server keeps).
//   client/public-lobby.js  render (filters, rows, the Join button, the empty line), refresh (8 s timeout, the
//                           status line), Quick play (the joinable Conquest room with the most humans, ties by code,
//                           else a new public room), create (a public or unlisted match with a title), the room code
//                           box, the Tutorial button, a new room code (six random bytes in hex), the 5 s refresh.
//   client/public-lobby.html the labels, the mode filter's options, the visibility options, the input limits.
//   client/main.js          listing (every hello carries it: public, title, tutorial from the page's query, with the
//                           defaults false, 'Open skirmish', false), the name (16 characters, default SoldierNN).
//   client/room-session.js  roomAddress (#code&seat=label: the code lowercased, 3 to 12 letters or digits, else the
//                           main room; the seat label 1 to 32 of [a-z0-9_-], any case).
// Every message is one the browser sends; nothing here invents protocol. The browser goes to a room by loading the
// page /play?name=..&public=..&title=..&tutorial=1#code; this client opens a socket to the same server with the same
// hello fields instead (FTCRoomEntry). This module does the HTTP request itself (FTCRoomBrowser::Request); the rest is
// plain state for the HUD to draw and tests to check. Texts are the reference's English; the HUD draws them through
// TC_T (client/es.js, Client/TCLocale.h).
#pragma once

#include "CoreMinimal.h"

class FJsonObject;

// client/main.js listing: what a hello says about a room it would create. server.js reads it only when the hello
// makes the room (an existing room keeps its own).
struct FTCRoomListing
{
	bool bPublic = false;
	FString Title = TEXT("Open skirmish");
	bool bTutorial = false;

	// { public, title, tutorial }, the browser's fields in its order.
	TSharedRef<FJsonObject> ToJson() const;
	// What server.js makes of it for a new room: listed (public and not a tutorial) and the cleaned title.
	bool ServerListed() const { return bPublic && !bTutorial; }
	FString ServerTitle() const;
	bool operator==(const FTCRoomListing& O) const { return bPublic == O.bPublic && Title == O.Title && bTutorial == O.bTutorial; }
};

// client/room-session.js roomAddress.
struct FTCRoomAddress
{
	FString Room;   // lowercased; the main room when the code is not 3 to 12 letters or digits
	FString Seat;   // empty when absent or not 1 to 32 of [A-Za-z0-9_-]
	FString Hash;   // the canonical hash the browser writes back ('' for the main room without a seat)
};

// Where a directory action goes: a room code and the hello fields the browser's page would send there.
struct FTCRoomEntry
{
	bool bGo = false;            // false: nothing happens (with Note saying why)
	FString Room;                // the room code, lowercased
	FString Name;                // the nickname the page saves and passes on ('Soldier' when empty)
	FTCRoomListing Listing;      // what the page passes (the defaults when it passes none, as main.js reads them)
	bool bListingGiven = false;  // the page put public and title (or tutorial) in the query
	FString Note;
};

// One room of GET /api/rooms.
struct FTCRoomInfo
{
	FString Code, Title, Mode, Map, State, WorldSize;
	int32 Humans = 0, Ai = 0, Occupied = 0, Capacity = 0;
	bool bJoinable = false;
};

// One row of the table, as public-lobby.js render writes it.
struct FTCRoomRow
{
	FString Code;
	FString Title;        // the room's title
	FString MapLine;      // the map with dashes as spaces, ' · ', the code
	FString Mode;         // the mode's name ('World Conquest'), else the mode as sent
	FString Seats;        // 'occupied/capacity'
	FString Counts;       // '2 humans · 1 AI'
	FString Status;       // 'In progress', 'Open' or 'Full'
	bool bOpen = false;   // the status cell's 'open' look (joinable)
	bool bJoinEnabled = false;
	FString JoinLabel;    // 'Join'
	FString JoinAria;     // 'Join <title>'
};

// Round 16 (controls): the directory and the room's socket. In the browser the directory is its own page
// (public-lobby.js: "The directory does not open a game socket or load Three.js until a player joins"): the lobby's
// "Browse matches" link (index.html <a href="/">) leaves /play, which closes the room's socket, and a pick
// (enter(): Join, Quick Play, Create match, Tutorial, Join room) loads /play#code, which opens a new one. This client
// shows the directory inside itself, so it does the same: opening it closes the socket, a pick opens the picked room's
// socket, and its Close button (the browser's Back to /play#code) opens the room it came from again.
class FTCDirectorySession
{
public:
	enum class EStep : uint8 { None, CloseSocket, OpenRoom };
	// "Browse matches": the directory shows. CloseSocket when a room's socket was open (Room: that room, kept for Close).
	EStep Open(bool bSocketOpen, const FString& Room);
	// Close: OpenRoom (ReturnRoom()) when no socket is open, the way back to the room the directory left (or, with
	// -TCBrowse at start-up, the command line's room); None when a socket is still open.
	EStep Close(bool bSocketOpen);
	// A pick: the directory closes and the picked room's socket opens (nothing to return to).
	void Entered() { bShown = false; Return.Reset(); }
	bool IsShown() const { return bShown; }
	const FString& ReturnRoom() const { return Return; }
	// -TCBrowse: the directory first, no socket yet, the command line's room behind Close
	void StartShown(const FString& Room) { bShown = true; Return = Room; }

private:
	bool bShown = false;
	FString Return;
};

class FTCRoomBrowser
{
public:
	static constexpr const TCHAR* ReferencePin = TEXT("e4e579a");
	static constexpr double RefreshSeconds = 5.0;   // setInterval(refresh, 5000)
	static constexpr double TimeoutSeconds = 8.0;   // AbortSignal.timeout(8000)
	static constexpr int32 NameMax = 16;            // <input maxlength=16>, slice(0, 16)
	static constexpr int32 TitleMax = 48;           // <input id=matchTitle maxlength=48>
	static constexpr int32 CodeMax = 12;            // <input id=roomCode maxlength=12>

	// --- the directory's data ---
	// GET /api/rooms on the server a socket URL names: ws://host:port/ws becomes http://host:port/api/rooms (wss: https).
	static FString RoomsUrl(const FString& SocketUrl);
	// Reads the response body. False (and the Offline state, as refresh's catch) when it is not JSON with a rooms array.
	bool ApplyRooms(const FString& Body);
	// The request failed or timed out (refresh's catch).
	void ApplyFailure();
	// No answer yet ('Connecting…').
	bool HasAnswer() const { return bAnswered; }
	bool IsOnline() const { return bOnline; }
	const TArray<FTCRoomInfo>& Rooms() const { return RoomList; }

	// --- the filters ---
	// '' (All modes) or one of the six modes, in the select's order.
	static const TArray<FString>& ModeFilters();
	static FString ModeFilterLabel(const FString& Mode);
	FString ModeFilter() const { return Filter; }
	void SetModeFilter(const FString& Mode);
	void StepModeFilter(int32 Direction = 1);
	bool OpenOnly() const { return bOpenOnly; }
	void SetOpenOnly(bool bOn) { bOpenOnly = bOn; }

	// --- what the page shows ---
	TArray<FTCRoomRow> Rows() const;
	FString StatusLine() const;     // #connection: 'Connecting…', 'n public matches · Live', 'Offline'
	bool EmptyShown() const;        // #empty
	FString EmptyLine() const;
	static FString ModeName(const FString& Mode); // public-lobby.js modes, else the mode as sent

	// --- the page's actions ---
	// The nickname box: trimmed, cut to 16, 'Soldier' when empty (enter()).
	static FString Nickname(const FString& Typed);
	// public-lobby.js code(): six random bytes, two hex digits each.
	static FString NewCode(TFunctionRef<uint8()> RandomByte);
	static FString NewCode();
	// Join on a row: the room as it is (no listing).
	FTCRoomEntry Join(const FString& Code, const FString& Typed) const;
	// The 'Have a room code?' form: the input's pattern ([a-zA-Z0-9]{3,12}, required) must hold; then lowercased.
	static FTCRoomEntry JoinCode(const FString& TypedCode, const FString& Typed);
	// Create match: a new code, public or unlisted, the title trimmed or 'Open skirmish' (cut to 48 as the input does).
	static FTCRoomEntry Create(const FString& Code, const FString& TypedTitle, bool bPublic, const FString& Typed);
	// The Tutorial button: a new private room with listing { public: false, title: 'Tutorial', tutorial: true }.
	static FTCRoomEntry Tutorial(const FString& Code, const FString& Typed);
	// Quick Play, after a refresh: the joinable Conquest room with the most humans (ties: the lower code), else a new
	// public room titled 'Open skirmish'. bRefreshOk false: nothing, with the page's notice.
	FTCRoomEntry QuickPlay(bool bRefreshOk, const FString& NewRoomCode, const FString& Typed) const;
	static FString QuickPlayNotice() { return TEXT("The directory is unavailable or still loading. Please try again."); }

	// --- the refresh clock (the page refreshes every 5 s while visible and not leaving) ---
	bool RefreshDue(double Now) const { return !bLoading && (LastRefreshAt < 0.0 || Now - LastRefreshAt >= RefreshSeconds); }
	bool Loading() const { return bLoading; }
	// Sends GET /api/rooms (HTTP module); the answer lands in ApplyRooms or ApplyFailure on the game thread. False when
	// a request is still running or the URL is not one this client can derive.
	bool Request(const FString& SocketUrl, double Now);
	// Stop listening to a request in flight (the browser closed).
	void Cancel();
	int32 Answers() const { return AnswerCount; }

	// client/room-session.js roomAddress, and the parts of an invite link (<origin>/play#code&seat=label): the socket URL
	// of that origin (http -> ws, https -> wss, path /ws) and the room address. False when the text is not such a link.
	static FTCRoomAddress RoomAddress(const FString& Hash, const FString& Main = TEXT("main"));
	static bool ParseInviteLink(const FString& Link, FString& OutSocketUrl, FTCRoomAddress& OutAddress);
	static bool ValidCode(const FString& Code); // /^[a-z0-9]{3,12}$/ after lowercasing

	FTCRoomBrowser();
	~FTCRoomBrowser();

	// round 16: whether the directory shows and the room behind its Close (the socket rule above)
	FTCDirectorySession Session;

private:
	TArray<FTCRoomInfo> RoomList;
	FString Filter;
	bool bOpenOnly = false, bAnswered = false, bOnline = false, bLoading = false;
	double LastRefreshAt = -1.0;
	int32 AnswerCount = 0, Ticket = 0;
	TSharedPtr<bool> Alive;
};
