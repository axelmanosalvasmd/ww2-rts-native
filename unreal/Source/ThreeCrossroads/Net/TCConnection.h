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
};
const TCHAR* LexToString(ETCSendResult Result);

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
	// Called on the game thread with every text message of the current socket as received, before it is parsed and
	// before OnMessage for the same text, with nothing in between (the Rust view, TC_DECODER=shadow or rust, feeds it to
	// ww2_view_apply unchanged; under rust FTCMatchState then reads the view's rows for that very message, so this
	// order is part of the contract, Net/TCRustViewShadow.h).
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
	int32 Opens() const { return OpenCount; }
	static bool ValidRoom(const FString& Room);
	static int32 Utf8Length(const FString& Text);

private:
	void Open();
	void Lost();
	void Retry(double Seconds, const TCHAR* Why);
	// Drops the current socket. bClose: ask it to close (a socket still connecting closes as soon as it connects);
	// false when it already reported closed or failed.
	void Retire(bool bClose = true);

	FSettings Settings;
	TSharedPtr<IWebSocket> Socket;
	TArray<TSharedPtr<IWebSocket>> Graveyard; // retired sockets, released on the next Tick
	uint64 Generation = 0;        // events from a retired socket are ignored (connection.js current !== socket)
	bool bRunning = false, bWaitingForRoom = false, bAnswered = false, bWatching = false;
	int32 Attempts = 0, OpenCount = 0;
	double RetryAt = -1.0, LastNow = 0.0;
	FString StatusText = TEXT("idle");
};
