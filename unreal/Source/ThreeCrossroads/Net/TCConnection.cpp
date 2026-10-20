#include "Net/TCConnection.h"
#include "ThreeCrossroads.h"
#include "WebSocketsModule.h"
#include "IWebSocket.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "Policies/CondensedJsonPrintPolicy.h"
#include "HAL/PlatformTime.h"
#include "GenericPlatform/GenericPlatformString.h"

const TCHAR* LexToString(ETCSendResult Result)
{
	switch (Result)
	{
	case ETCSendResult::Sent: return TEXT("sent");
	case ETCSendResult::NotOpen: return TEXT("not sent: socket not open");
	case ETCSendResult::Unserializable: return TEXT("not sent: could not write the JSON");
	case ETCSendResult::TooLarge: return TEXT("not sent: over the server's 16 KB message limit");
	case ETCSendResult::ProfileRefused: return FTCConnection::RequiredRustProfileRefusal();
	}
	return TEXT("not sent");
}

const TCHAR* LexToString(ETCHostKind Kind)
{
	switch (Kind)
	{
	case ETCHostKind::Unknown: return TEXT("unknown");
	case ETCHostKind::Reference: return TEXT("reference");
	case ETCHostKind::Rust: return TEXT("rust");
	}
	return TEXT("unknown");
}

ETCHostKind FTCConnection::HostKindOf(const FJsonObject& Message, int32* OutProto, FString* OutRules)
{
	if (OutProto) *OutProto = -1;
	if (OutRules) OutRules->Reset();
	FString Type;
	if (!Message.TryGetStringField(TEXT("t"), Type) || Type != TEXT("lobby")) return ETCHostKind::Unknown;
	if (OutRules) Message.TryGetStringField(TEXT("rules"), *OutRules);
	const TSharedPtr<FJsonValue> Proto = Message.TryGetField(TEXT("proto"));
	// Only an absent field means reference. A malformed or future proto must keep the Rust restrictions.
	if (!Proto.IsValid()) return ETCHostKind::Reference;
	double Number = 0.0;
	if (OutProto && Proto->Type == EJson::Number && Proto->TryGetNumber(Number) && FMath::IsFinite(Number) && Number == FMath::FloorToDouble(Number) && FMath::Abs(Number) < 1e9) *OutProto = (int32)Number;
	return ETCHostKind::Rust;
}

const TCHAR* FTCConnection::RequiredRustProfileRefusal()
{
	return TEXT("Rust host: required R1 profile unverified. Start, Restart and Load are disabled.");
}

bool FTCConnection::RustProfileCommandBlocked(ETCHostKind Kind, const FString& Type)
{
	return Kind == ETCHostKind::Rust && (Type == TEXT("start") || Type == TEXT("restart") || Type == TEXT("loadSave"));
}

FString FTCConnection::MatchAdmissionRefusal(ETCHostKind Kind, const FString& Type)
{
	if (Type != TEXT("start") && Type != TEXT("s")) return FString();
	if (Kind == ETCHostKind::Rust) return RequiredRustProfileRefusal();
	if (Kind == ETCHostKind::Unknown) return TEXT("Match refused: waiting for the host's lobby.");
	return FString();
}

FTCConnection::FTCConnection(const FSettings& InSettings) : Settings(InSettings), bWatching(InSettings.bSpectate) {}

TSharedRef<FJsonObject> FTCConnection::Hello(const FSettings& Settings, bool bWatching)
{
	auto M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("hello"));
	M->SetStringField(TEXT("name"), Settings.Name);
	M->SetStringField(TEXT("token"), Settings.Token);
	if (bWatching) M->SetBoolField(TEXT("spectate"), true);
	M->SetNumberField(TEXT("proto"), ProtocolVersion);
	M->SetStringField(TEXT("client"), ClientId);
	if (Settings.Listing) M->SetObjectField(TEXT("listing"), Settings.Listing);
	return M;
}

void FTCConnectionNotice::Receive(const FJsonObject& M)
{
	FString T;
	if (!M.TryGetStringField(TEXT("t"), T)) return;
	bRetry = false;
	bFull = T == TEXT("full");
	bReplaced = T == TEXT("replaced"); bLeft = T == TEXT("left");
	if (T == TEXT("full")) M.TryGetStringField(TEXT("reason"), FullReason);
	else FullReason.Reset();
}

FString FTCConnectionNotice::Text(double Now) const
{
	if (bReplaced) return TEXT("Open in another tab");
	if (bLeft) return TEXT("You left the match; an AI took over your army. Reload to rejoin the lobby when the match is over.");
	const int32 N = Left(Now);
	if (N < 0) return FString();
	return bFull ? FString::Printf(TEXT("Trying this room again in %d s"), N) : FString::Printf(TEXT("Connection lost. Retrying in %d s"), N);
}

FString FTCConnectionNotice::RoomLine() const
{
	if (!bFull) return FString();
	if (FullReason == TEXT("capacity")) return TEXT("The server cannot create more rooms right now. Browse matches to join an existing room, or wait for a retry.");
	if (FullReason == TEXT("started")) return TEXT("A match is running in this room. You will join when it ends.");
	return TEXT("This room is full. You will join when a seat opens.");
}

FTCConnection::~FTCConnection() { bRunning = false; Retire(); Graveyard.Reset(); }

int32 FTCConnection::Utf8Length(const FString& Text)
{
	// What the socket really carries: FLwsWebSocket::Send converts with FTCHARToUTF8, and the server counts bytes.
	return Text.IsEmpty() ? 0 : FPlatformString::ConvertedLength<UTF8CHAR>(*Text, Text.Len());
}

bool FTCConnection::ValidRoom(const FString& Room)
{
	if (Room.Len() < 3 || Room.Len() > 12) return false;
	for (TCHAR C : Room) if (!FChar::IsAlnum(C) || C > 127) return false;
	return true;
}

void FTCConnection::Start()
{
	FModuleManager::LoadModuleChecked<FWebSocketsModule>(TEXT("WebSockets"));
	RetryAt = -1.0; Retire();
	LastNow = FPlatformTime::Seconds(); // the clock Tick is given: a failure before the first Tick still waits
	Attempts = 0; bWaitingForRoom = false; bRunning = true;
	Banner = FTCConnectionNotice();
	Host = ETCHostKind::Unknown; HostProtoValue = -1; HostRulesValue.Reset();
	Open();
}

void FTCConnection::Stop()
{
	bRunning = false; RetryAt = -1.0;
	Banner = FTCConnectionNotice();
	Retire();
	StatusText = TEXT("stopped");
}

void FTCConnection::Retire(bool bClose)
{
	Generation++;
	PendingHostMessages.Reset(); PendingHostBytes = 0;
	if (!Socket.IsValid()) return;
	// This may run inside the socket's own callback: keep it alive until the next Tick, and leave its delegates bound
	// (the generation check above already ignores them). Unbinding a delegate mid-broadcast frees the running lambda.
	// Close even while still connecting: the engine's socket manager keeps its own reference and would finish the
	// connect, leaving the server an idle socket that never says hello. FLwsWebSocket::Close queues the request and
	// its service thread acts on it once the socket is connected (engine LwsWebSocket.cpp LwsThreadTick).
	if (bClose) Socket->Close();
	Graveyard.Add(Socket);
	Socket.Reset();
}

bool FTCConnection::IsOpen() const { return Socket.IsValid() && Socket->IsConnected(); }

void FTCConnection::Open()
{
	if (!bRunning) return;
	bAnswered = false;
	const FString Url = FString::Printf(TEXT("%s?room=%s"), *Settings.Url, *Settings.Room);
	TSharedRef<IWebSocket> Current = FWebSocketsModule::Get().CreateWebSocket(Url, FString());
	Current->SetTextMessageMemoryLimit(64 * 1024 * 1024); // a World start carries the discovered map
	Socket = Current;
	const uint64 Gen = ++Generation;
	TWeakPtr<FTCConnection> Weak = AsShared();
	StatusText = FString::Printf(TEXT("connecting to %s (room %s)"), *Settings.Url, *Settings.Room);
	UE_LOG(LogTC, Log, TEXT("%s"), *StatusText);

	Current->OnConnected().AddLambda([Weak, Gen]()
	{
		TSharedPtr<FTCConnection> Self = Weak.Pin();
		if (!Self || Gen != Self->Generation || !Self->bRunning) return;
		Self->OpenCount++;
		Self->StatusText = TEXT("connected, waiting for the room");
		UE_LOG(LogTC, Log, TEXT("socket open (open #%d)"), Self->OpenCount);
		if (Self->OnOpened) Self->OnOpened(Self->OpenCount);
		if (Gen != Self->Generation || !Self->bRunning) return; // the callback stopped or restarted us
		const bool bSpectate = Self->bWatching;
		const ETCSendResult Sent = Self->TrySend(Hello(Self->Settings, bSpectate));
		if (Sent != ETCSendResult::Sent) { UE_LOG(LogTC, Warning, TEXT("hello %s"), LexToString(Sent)); } // never the token
		else { UE_LOG(LogTC, Log, TEXT("hello sent: proto %d, client %s%s%s"), FTCConnection::ProtocolVersion, FTCConnection::ClientId, bSpectate ? TEXT(", spectate") : TEXT(""), Self->Settings.Listing.IsValid() ? TEXT(", listing") : TEXT("")); }
	});
	Current->OnConnectionError().AddLambda([Weak, Gen](const FString& Error)
	{
		TSharedPtr<FTCConnection> Self = Weak.Pin();
		if (!Self || Gen != Self->Generation || !Self->bRunning) return;
		UE_LOG(LogTC, Warning, TEXT("connection error: %s"), *Error);
		Self->Retire(false);
		Self->Lost();
	});
	Current->OnClosed().AddLambda([Weak, Gen](int32 Code, const FString& Reason, bool bClean)
	{
		TSharedPtr<FTCConnection> Self = Weak.Pin();
		if (!Self || Gen != Self->Generation || !Self->bRunning) return;
		UE_LOG(LogTC, Warning, TEXT("socket closed: %d %s"), Code, *Reason);
		Self->Retire(false);
		Self->Lost();
	});
	Current->OnMessage().AddLambda([Weak, Gen](const FString& Text)
	{
		TSharedPtr<FTCConnection> Self = Weak.Pin();
		if (Self) Self->ReceiveText(Text, Gen);
	});
	Current->Connect();
}

void FTCConnection::ReceiveText(const FString& Text, uint64 ExpectedGeneration)
{
	if (ExpectedGeneration != Generation || !bRunning) return;
	TSharedPtr<FJsonObject> Message;
	const TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);
	FString Type;
	const bool bParsed = FJsonSerializer::Deserialize(Reader, Message) && Message.IsValid() && Message->TryGetStringField(TEXT("t"), Type);
	// These retire the socket without needing a host profile. A start buffered while lobby() awaits the map list
	// must not hide a seat replacement, hand-off, full-room retry or protocol refusal behind a lobby that never comes.
	const bool bConnectionControl = bParsed && (Type == TEXT("replaced") || Type == TEXT("left") || Type == TEXT("full") || IsVersionDeny(*Message));
	FString RefusedPendingMatch;
	if (Host == ETCHostKind::Unknown)
	{
		if (bParsed && Type == TEXT("lobby"))
		{
			NoteLobby(*Message);
			TArray<FString> Pending = MoveTemp(PendingHostMessages); PendingHostMessages.Reset(); PendingHostBytes = 0;
			if (Host == ETCHostKind::Reference)
			{
				// Preserve the reference's exact bytes and receive order after classification, before this lobby.
				for (const FString& Earlier : Pending)
				{
					ReceiveText(Earlier, ExpectedGeneration);
					if (ExpectedGeneration != Generation || !bRunning) return;
				}
			}
			else if (Pending.Num()) RefusedPendingMatch = MoveTemp(Pending[0]);
		}
		else if (!bConnectionControl && (PendingHostMessages.Num() || (bParsed && (Type == TEXT("start") || Type == TEXT("s")))))
		{
			// Do not infer a host profile from a start packet. Bound the wait by the existing socket text limit.
			const int64 Bytes = Utf8Length(Text);
			if (PendingHostMessages.Num() >= 64 || PendingHostBytes + Bytes > 64 * 1024 * 1024)
			{
				bRunning = false; RetryAt = -1; Banner = FTCConnectionNotice(); Retire();
				StatusText = TEXT("Match refused: host lobby did not arrive within the receive limit.");
				UE_LOG(LogTC, Log, TEXT("%s"), *StatusText);
				return;
			}
			PendingHostBytes += Bytes; PendingHostMessages.Add(Text);
			StatusText = TEXT("Waiting for the host's lobby before admitting the match.");
			return;
		}
	}
	if (bParsed)
	{
		const FString Refusal = MatchAdmissionRefusal(Host, Type);
		if (!Refusal.IsEmpty())
		{
			// A remote start or reconnect must not bypass the local Start/Restart gate. Stop before either decoder
			// observes the match. Retry cannot establish the host-private profile and must not loop on refusal.
			bRunning = false; RetryAt = -1.0; Banner = FTCConnectionNotice(); Retire();
			StatusText = Refusal;
			UE_LOG(LogTC, Log, TEXT("%s Match not admitted; connection stopped."), *Refusal);
			return;
		}
	}
	// Keep the original text and raw-before-parsed callback order, including malformed reference messages.
	if (OnText) OnText(Text);
	if (ExpectedGeneration != Generation || !bRunning || !bParsed) return;
	Banner.Receive(*Message);
	if (Type == TEXT("replaced"))
	{
		bRunning = false; RetryAt = -1.0; Retire();
		StatusText = TEXT("replaced by another client with this token");
	}
	else if (Type == TEXT("left"))
	{
		bRunning = false; RetryAt = -1.0; Retire();
		StatusText = TEXT("left the match: an AI took over this army");
	}
	else if (Type == TEXT("deny") && IsVersionDeny(*Message))
	{
		// protocol.md addition 1: retrying the same unsupported hello cannot help.
		bRunning = false; RetryAt = -1.0; Retire();
		StatusText = TEXT("the server does not speak this client's protocol version");
		UE_LOG(LogTC, Warning, TEXT("%s (hello proto %d): stopped"), *StatusText, ProtocolVersion);
	}
	else if (Type == TEXT("full"))
	{
		bWaitingForRoom = true; Attempts = 0;
		Retire(); Retry(10.0, TEXT("room full"));
	}
	else
	{
		Attempts = 0; bWaitingForRoom = false;
		if (!bAnswered) { bAnswered = true; StatusText = TEXT("in room"); }
		if (Type == TEXT("lobby")) NoteLobby(*Message);
	}
	if (OnMessage) OnMessage(*Message, Utf8Length(Text));
	// Keep a Rust lobby inspectable, then refuse its queued match before either decoder sees that match.
	if (!RefusedPendingMatch.IsEmpty() && ExpectedGeneration == Generation && bRunning)
		ReceiveText(RefusedPendingMatch, ExpectedGeneration);
}

bool FTCConnection::IsVersionDeny(const FJsonObject& Message)
{
	FString Type, Cmd, Reason;
	return Message.TryGetStringField(TEXT("t"), Type) && Type == TEXT("deny") && Message.TryGetStringField(TEXT("cmd"), Cmd) && Cmd == TEXT("join") && Message.TryGetStringField(TEXT("reason"), Reason) && Reason == TEXT("version");
}

void FTCConnection::NoteLobby(const FJsonObject& Message)
{
	int32 Proto = -1;
	FString Rules;
	const ETCHostKind Kind = HostKindOf(Message, &Proto, &Rules);
	if (Kind == ETCHostKind::Unknown) return;
	const bool bChanged = Kind != Host || Proto != HostProtoValue || Rules != HostRulesValue;
	const bool bWasRust = Host == ETCHostKind::Rust;
	Host = Kind; HostProtoValue = Proto; HostRulesValue = MoveTemp(Rules);
	Message.TryGetBoolField(TEXT("spectator"), bWatching);
	// Logged only for a Rust host (or when a Rust host turns into the reference): a reference session logs as before.
	if (bChanged && (Kind == ETCHostKind::Rust || bWasRust)) UE_LOG(LogTC, Log, TEXT("host: %s server (lobby proto %s%s)"), LexToString(Kind), Proto >= 0 ? *FString::FromInt(Proto) : Kind == ETCHostKind::Rust ? TEXT("present, not an integer") : TEXT("absent"), HostRulesValue.IsEmpty() ? TEXT(", no rules hash") : *FString::Printf(TEXT(", rules %s"), *HostRulesValue.Left(24)));
}

void FTCConnection::Lost()
{
	if (!bRunning) return;
	if (bWaitingForRoom) Retry(10.0, TEXT("room full"));
	else Retry(FMath::Min(8.0, FMath::Pow(2.0, (double)Attempts++)), TEXT("lost"));
}

void FTCConnection::Retry(double Seconds, const TCHAR* Why)
{
	RetryAt = LastNow + Seconds;
	Banner.Retry(LastNow, Seconds, bWaitingForRoom);
	StatusText = FString::Printf(TEXT("%s: retrying in %.0f s"), Why, Seconds);
	UE_LOG(LogTC, Log, TEXT("%s"), *StatusText);
}

void FTCConnection::Tick(double Now)
{
	LastNow = Now;
	Graveyard.Reset(); // retired sockets, now outside their callbacks
	if (bRunning && RetryAt >= 0.0 && Now >= RetryAt) { RetryAt = -1.0; Open(); }
}

ETCSendResult FTCConnection::TrySend(const TSharedRef<FJsonObject>& Message, int32* OutBytes)
{
	if (OutBytes) *OutBytes = 0;
	FString Type;
	Message->TryGetStringField(TEXT("t"), Type);
	if (RustProfileCommandBlocked(Host, Type)) return ETCSendResult::ProfileRefused;
	if (!IsOpen()) return ETCSendResult::NotOpen;
	FString Out;
	TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
	if (!FJsonSerializer::Serialize(Message, Writer)) return ETCSendResult::Unserializable;
	// The limit is in bytes: a name or text outside ASCII takes two to four UTF-8 bytes per character
	const int32 Bytes = Utf8Length(Out);
	if (OutBytes) *OutBytes = Bytes;
	if (Bytes > MaxSendBytes)
	{
		UE_LOG(LogTC, Warning, TEXT("'%s' message of %d UTF-8 bytes (%d characters) is over the server's %d byte limit: not sent"), *Type.Left(16), Bytes, Out.Len(), MaxSendBytes);
		return ETCSendResult::TooLarge;
	}
	Socket->Send(Out);
	return ETCSendResult::Sent;
}

void FTCConnection::SimulateDrop()
{
	if (!Socket.IsValid()) return;
	UE_LOG(LogTC, Log, TEXT("simulating a network drop"));
	Retire();
	Lost();
}
