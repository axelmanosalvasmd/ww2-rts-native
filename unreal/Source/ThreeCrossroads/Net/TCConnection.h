// One client connection to a reference-protocol room (reference/ww2-rts client/connection.js): hello on open, retry
// with backoff when lost, a ten second wait when the room is full, and no retry once another client took the seat or
// this seat was handed to an AI (left).
#pragma once

#include "CoreMinimal.h"
#include "Templates/SharedPointer.h"

class IWebSocket;
class FJsonObject;

// Why a message was not sent.
enum class ETCSendResult : uint8
{
	Sent,
	NotOpen,          // no open socket (connecting, retrying or stopped)
	Unserializable,   // the JSON writer refused the message
	TooLarge,         // over the server's 16 KB maxPayload in UTF-8 bytes; ws would close the socket
	ProfileRefused,   // the required Rust R1 launch profile has not been verified
};
const TCHAR* LexToString(ETCSendResult Result);

// Which server the lobby says this client reached (docs/contracts/protocol.md v0, "Additions in v0" 1): a Rust server
// adds proto to every lobby; the pinned reference server (e4e579a, server.js lobby()) sends no proto at all.
enum class ETCHostKind : uint8
{
	Unknown,    // no lobby yet on this connection
	Reference,  // a lobby without proto: the Node reference server (or the browser-era protocol)
	Rust,       // a lobby with proto: the Rust server (ww2-server); HostProto() says which version it speaks
};
const TCHAR* LexToString(ETCHostKind Kind);

// connection.js and main.js presentation state, independent of a socket and a wall clock.
// A retry banner survives the next connection attempt until the first valid server message.
struct FTCConnectionNotice
{
	bool bReplaced = false, bLeft = false, bFull = false, bRetry = false;
	double Until = 0;
	FString FullReason;
	void Retry(double Now, double Seconds, bool bRoomFull) { Until = Now + Seconds; bRetry = true; bFull = bRoomFull; }
	void Receive(const FJsonObject& Message);
	int32 Left(double Now) const { return bRetry ? FMath::Max(1, (int32)FMath::CeilToDouble(Until - Now)) : -1; }
	FString Text(double Now) const;
	FString RoomLine() const;
};

class FTCConnection : public TSharedFromThis<FTCConnection>
{
public:
	struct FSettings
	{
		FString Url;          // ws://host:port/ws
		FString Room;         // 3 to 12 letters or digits (server.js)
		FString Name;
		FString Token;        // the seat's key across reconnects; never logged (see Net/TCToken.h for where it lives)
		bool bSpectate = false;
		// client/main.js hello listing: { public, title, tutorial }. Set only for a room this client creates (round 14:
		// the lobby's Tutorial button, -TCTutorial); server.js reads it when the hello makes a new room.
		TSharedPtr<FJsonObject> Listing;
	};

	static constexpr int32 MaxSendBytes = 16 * 1024; // server.js WebSocketServer maxPayload, in bytes
	// hello's proto and client (docs/contracts/protocol.md v0, "Additions in v0" 1): a server that knows v0 can tell this
	// client from the browser; the reference server ignores both.
	static constexpr int32 ProtocolVersion = 0;
	static constexpr const TCHAR* ClientId = TEXT("unreal/0.1");

	explicit FTCConnection(const FSettings& InSettings);
	~FTCConnection();

	// Called on the game thread for every JSON message with a string "t". Bytes: the message's length in UTF-8.
	TFunction<void(const FJsonObject& Message, int32 Bytes)> OnMessage;
	// Called on the game thread with admitted text from the current socket, before OnMessage for the same text,
	// with no intervening callback (the Rust view, TC_DECODER=shadow or rust, feeds it to
	// ww2_view_apply unchanged; under rust FTCMatchState then reads the view's rows for that very message, so this
	// order is part of the contract, Net/TCRustViewShadow.h). Profile refusal precedes both decoder callbacks.
	TFunction<void(const FString& Text)> OnText;
	// Called on the game thread when a new socket opens, just before its hello goes out.
	TFunction<void(int32 OpenNumber)> OnOpened;

	void Start();
	void Stop();
	void Tick(double Now);
	bool Send(const TSharedRef<FJsonObject>& Message) { return TrySend(Message) == ETCSendResult::Sent; }
	// As Send, with the reason when it is not sent. OutBytes (optional) gets the UTF-8 size when it was measured.
	ETCSendResult TrySend(const TSharedRef<FJsonObject>& Message, int32* OutBytes = nullptr);
	bool IsOpen() const;
	// False once stopped for good (Stop, replaced, left); a retry wait still counts as running.
	bool IsRunning() const { return bRunning; }
	// The last lobby said this client watches: a reconnect asks to keep watching (client/main.js hello spectate).
	void SetWatching(bool bInWatching) { bWatching = bInWatching; }
	// Round 15: the Name field changed (client/main.js: the next hello sends $('name').value)
	void SetName(const FString& Name) { Settings.Name = Name; }
	// Closes the socket as a network drop would, so the retry path runs (a -TCDropAfter verification switch).
	void SimulateDrop();

	FString Status() const { return StatusText; }
	const FTCConnectionNotice& Notice() const { return Banner; }
	// Round 17 (hudui): connection.js's 'retry' event for main.js's #connectionBanner: the whole seconds left of the
	// wait before the next try (at least 1, as the countdown shows until it opens), and whether it waits for a full room
	// ('full': "Trying this room again in {n} s") or after a loss ("Connection lost. Retrying in {n} s"); -1 when no
	// wait is running (connected, stopped, or a try is under way)
	int32 RetryLeft(double Now, bool* bOutFull = nullptr) const
	{
		if (bOutFull) *bOutFull = Banner.bFull;
		return Banner.Left(Now);
	}
	int32 Opens() const { return OpenCount; }
	// Round 18 (rusthost): which server the last lobby came from, the proto it carried (-1 when it carried none, or a
	// value that is not an integer) and its rules hash (protocol.md addition 2; empty when absent). Unknown until the
	// first lobby; kept across a reconnect until the next lobby says otherwise, forgotten by Start.
	ETCHostKind HostKind() const { return Host; }
	bool IsRustHost() const { return Host == ETCHostKind::Rust; }
	int32 HostProto() const { return HostProtoValue; }
	const FString& HostRules() const { return HostRulesValue; }
	// What a lobby message says about its server. OutProto: the integer proto, -1 when absent or not an integer.
	// Anything but a lobby is Unknown. A proto that is present but not an integer still marks a non-reference host
	// (the reference never sends the field), with OutProto -1.
	static ETCHostKind HostKindOf(const FJsonObject& Message, int32* OutProto = nullptr, FString* OutRules = nullptr);
	// deny {cmd:'join', reason:'version'} (protocol.md addition 1; only a Rust server sends it): the connection stops
	// for good instead of retrying with the same hello.
	static bool IsVersionDeny(const FJsonObject& Message);
	// Proto, rules and version are not profile proof. Approval belongs to the active connection URL.
	static const TCHAR* RequiredRustProfileRefusal();
	// Local development harness only. Requires both an environment opt-in and a loopback command line.
	static bool OwnerR1Qualification(const FString& ActiveUrl = FString());
	// Missing endpoint context fails closed. Never infer it from the original command line.
	static bool OwnerR1AdmissionEnabled(const FString& ActiveUrl = FString());
	static bool RustProfileCommandBlocked(ETCHostKind Kind, const FString& Type, const FString& ActiveUrl = FString());
	static FString MatchAdmissionRefusal(ETCHostKind Kind, const FString& Type, const FString& ActiveUrl = FString());
	static bool ValidRoom(const FString& Room);
	static int32 Utf8Length(const FString& Text);
	// The initial spectator request is replaced by the server-confirmed role after sit/spectate.
	static TSharedRef<FJsonObject> Hello(const FSettings& Settings, bool bWatching);

private:
#if WITH_DEV_AUTOMATION_TESTS
	friend struct FTCRows30Access;
#endif
	// Same receive path for the socket and the deterministic admission test. Never feeds a refused match to a view.
	void ReceiveText(const FString& Text, uint64 ExpectedGeneration);
#if WITH_DEV_AUTOMATION_TESTS
	friend class FTCSessionRustProfileAdmissionTest;
	friend class FTCSession26TransportOrderingTest;
#endif
	void Open();
	void Lost();
	void Retry(double Seconds, const TCHAR* Why);
	// Drops the current socket. bClose: ask it to close (a socket still connecting closes as soon as it connects);
	// false when it already reported closed or failed.
	void Retire(bool bClose = true);
	// Round 18: reads a lobby's host kind (HostKindOf) into Host, HostProtoValue and HostRulesValue.
	void NoteLobby(const FJsonObject& Message);

	FSettings Settings;
	TSharedPtr<IWebSocket> Socket;
	TArray<TSharedPtr<IWebSocket>> Graveyard; // retired sockets, released on the next Tick
	uint64 Generation = 0;        // events from a retired socket are ignored (connection.js current !== socket)
	bool bRunning = false, bWaitingForRoom = false, bAnswered = false, bWatching = false;
	int32 Attempts = 0, OpenCount = 0;
	double RetryAt = -1.0, LastNow = 0.0;
	FString StatusText = TEXT("idle");
	ETCHostKind Host = ETCHostKind::Unknown;
	int32 HostProtoValue = -1;
	FString HostRulesValue;
	// server.js lobby() awaits its map list, so a fresh reference reconnect may receive start first.
	TArray<FString> PendingHostMessages;
	int64 PendingHostBytes = 0;
	FTCConnectionNotice Banner;
	void TickOwnerR1Qualification(double Now);
	double QualificationPollAt = 0;
	int32 QualificationSequence = 0;
};
