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

FTCConnection::FTCConnection(const FSettings& InSettings) : Settings(InSettings) {}

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
	Host = ETCHostKind::Unknown; HostProtoValue = -1; HostRulesValue.Reset();
	Open();
}

void FTCConnection::Stop()
{
	bRunning = false; RetryAt = -1.0;
	Retire();
	StatusText = TEXT("stopped");
}

void FTCConnection::Retire(bool bClose)
{
	Generation++;
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
		TSharedRef<FJsonObject> Hello = MakeShared<FJsonObject>();
		Hello->SetStringField(TEXT("t"), TEXT("hello"));
		Hello->SetStringField(TEXT("name"), Self->Settings.Name);
		Hello->SetStringField(TEXT("token"), Self->Settings.Token);
		// a spectator who reconnects keeps watching (client/main.js hello), instead of taking a seat a lobby room has free
		const bool bSpectate = Self->Settings.bSpectate || Self->bWatching;
		if (bSpectate) Hello->SetBoolField(TEXT("spectate"), true);
		// docs/contracts/protocol.md v0 additions 1 (decided in #5, axel-interface-response-004): the protocol version
		// and which client this is. The reference server reads only t, name and token (server.js) and ignores them.
		Hello->SetNumberField(TEXT("proto"), FTCConnection::ProtocolVersion);
		Hello->SetStringField(TEXT("client"), FTCConnection::ClientId);
		// round 14: the room's listing (client/main.js hello listing), e.g. the Tutorial button's private Tutorial room
		if (Self->Settings.Listing.IsValid()) Hello->SetObjectField(TEXT("listing"), Self->Settings.Listing);
		const ETCSendResult Sent = Self->TrySend(Hello);
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
		if (!Self || Gen != Self->Generation || !Self->bRunning) return;
		// First the raw text (the Rust view applies it), then, for the same text, OnMessage below (TCConnection.h OnText).
		if (Self->OnText) Self->OnText(Text);
		if (Gen != Self->Generation || !Self->bRunning) return; // OnText stopped or restarted us
		TSharedPtr<FJsonObject> Message;
		TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);
		FString Type;
		if (!FJsonSerializer::Deserialize(Reader, Message) || !Message.IsValid() || !Message->TryGetStringField(TEXT("t"), Type)) return;
		if (Type == TEXT("replaced"))
		{
			// Another client took this seat with the same token: stop for good (connection.js)
			Self->bRunning = false; Self->RetryAt = -1.0; Self->Retire();
			Self->StatusText = TEXT("replaced by another client with this token");
		}
		else if (Type == TEXT("left"))
		{
			// This seat went to an AI (server.js leave, handToAi clears the token): reconnecting would only come back
			// as a spectator, so stop like the browser does (client/main.js on left)
			Self->bRunning = false; Self->RetryAt = -1.0; Self->Retire();
			Self->StatusText = TEXT("left the match: an AI took over this army");
		}
		else if (Type == TEXT("deny") && Self->IsVersionDeny(*Message))
		{
			// protocol.md addition 1: a Rust server that does not speak this proto answers deny {cmd:'join',
			// reason:'version'} and closes with 1008; retrying cannot help. The reference never sends 'version'.
			Self->bRunning = false; Self->RetryAt = -1.0; Self->Retire();
			Self->StatusText = TEXT("the server does not speak this client's protocol version");
			UE_LOG(LogTC, Warning, TEXT("%s (hello proto %d): stopped"), *Self->StatusText, FTCConnection::ProtocolVersion);
		}
		else if (Type == TEXT("full"))
		{
			Self->bWaitingForRoom = true; Self->Attempts = 0;
			Self->Retire(); Self->Retry(10.0, TEXT("room full"));
		}
		else
		{
			Self->Attempts = 0; Self->bWaitingForRoom = false;
			if (!Self->bAnswered) { Self->bAnswered = true; Self->StatusText = TEXT("in room"); }
			if (Type == TEXT("lobby")) Self->NoteLobby(*Message);
		}
		if (Self->OnMessage) Self->OnMessage(*Message, Utf8Length(Text));
	});
	Current->Connect();
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
	if (!IsOpen()) return ETCSendResult::NotOpen;
	FString Out;
	TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
	if (!FJsonSerializer::Serialize(Message, Writer)) return ETCSendResult::Unserializable;
	// The limit is in bytes: a name or text outside ASCII takes two to four UTF-8 bytes per character
	const int32 Bytes = Utf8Length(Out);
	if (OutBytes) *OutBytes = Bytes;
	if (Bytes > MaxSendBytes)
	{
		FString Type;
		Message->TryGetStringField(TEXT("t"), Type);
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
