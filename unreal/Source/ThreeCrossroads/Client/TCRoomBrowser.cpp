#include "Client/TCRoomBrowser.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "HttpModule.h"
#include "Interfaces/IHttpRequest.h"
#include "Interfaces/IHttpResponse.h"
#include "Misc/Guid.h"
#include "Misc/AutomationTest.h"

namespace TCRoomL
{
	// public-lobby.js modes, and the mode filter's options in client/public-lobby.html's order
	struct FMode { const TCHAR* Key; const TCHAR* Name; };
	const FMode Modes[] = {
		{ TEXT("conquest"), TEXT("Conquest") }, { TEXT("assault"), TEXT("Assault") }, { TEXT("annihilation"), TEXT("Annihilation") },
		{ TEXT("classic"), TEXT("Classic") }, { TEXT("world"), TEXT("World Conquest") }, { TEXT("horde"), TEXT("Horde") },
	};
	const TCHAR* const FilterOrder[] = { TEXT(""), TEXT("conquest"), TEXT("assault"), TEXT("annihilation"), TEXT("classic"), TEXT("horde"), TEXT("world") };

	bool Lower(TCHAR C) { return C >= 'a' && C <= 'z'; }
	bool Digit(TCHAR C) { return C >= '0' && C <= '9'; }
	bool Alnum(TCHAR C) { return Lower(C) || Digit(C) || (C >= 'A' && C <= 'Z'); }
	// JavaScript's String.prototype.trim: whitespace and line terminators at both ends (the ones a name can hold)
	bool JsSpace(TCHAR C)
	{
		return C == ' ' || C == '\t' || C == '\n' || C == '\r' || C == 0x0B || C == 0x0C || C == 0xA0 || C == 0xFEFF || C == 0x1680
			|| (C >= 0x2000 && C <= 0x200A) || C == 0x2028 || C == 0x2029 || C == 0x202F || C == 0x205F || C == 0x3000;
	}
	FString JsTrim(const FString& S)
	{
		int32 A = 0, B = S.Len();
		while (A < B && JsSpace(S[A])) A++;
		while (B > A && JsSpace(S[B - 1])) B--;
		return S.Mid(A, B - A);
	}
	// String.prototype.toLowerCase for the ASCII letters a code may hold (a code with anything else is refused anyway)
	FString AsciiLower(const FString& S)
	{
		FString O = S;
		for (TCHAR& C : O) if (C >= 'A' && C <= 'Z') C = C - 'A' + 'a';
		return O;
	}
	// slice(0, n) counts UTF-16 code units, as FString does on every platform this client builds for
	FString Slice(const FString& S, int32 N) { return S.Left(N); }

	bool Str(const FJsonObject& O, const TCHAR* K, FString& Out)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(K);
		if (!V.IsValid() || V->Type != EJson::String) return false;
		Out = V->AsString();
		return true;
	}
	bool Num(const FJsonObject& O, const TCHAR* K, int32& Out)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(K);
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = (int32)V->AsNumber();
		return true;
	}
	bool Flag(const FJsonObject& O, const TCHAR* K)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(K);
		return V.IsValid() && V->Type == EJson::Boolean && V->AsBool();
	}
	// String.prototype.localeCompare for room codes (letters and digits): digits before letters, then letters without
	// regard to case, then a lower-case letter before its upper case (ICU's root collation on these characters)
	int32 CodeCompare(const FString& A, const FString& B)
	{
		const int32 Primary = AsciiLower(A).Compare(AsciiLower(B), ESearchCase::CaseSensitive);
		if (Primary) return Primary;
		for (int32 I = 0; I < FMath::Min(A.Len(), B.Len()); I++)
			if (A[I] != B[I]) return Lower(A[I]) ? -1 : 1;
		return A.Len() - B.Len();
	}
	// the server origin of a socket URL: ws://host:port/ws -> http://host:port (wss: https); empty otherwise
	FString Origin(const FString& SocketUrl)
	{
		FString Rest;
		FString Scheme;
		if (SocketUrl.StartsWith(TEXT("ws://"), ESearchCase::IgnoreCase)) { Scheme = TEXT("http://"); Rest = SocketUrl.Mid(5); }
		else if (SocketUrl.StartsWith(TEXT("wss://"), ESearchCase::IgnoreCase)) { Scheme = TEXT("https://"); Rest = SocketUrl.Mid(6); }
		else return FString();
		int32 Cut = INDEX_NONE;
		for (int32 I = 0; I < Rest.Len(); I++) if (Rest[I] == '/' || Rest[I] == '?' || Rest[I] == '#') { Cut = I; break; }
		const FString Host = Cut == INDEX_NONE ? Rest : Rest.Left(Cut);
		if (Host.IsEmpty()) return FString();
		return Scheme + Host;
	}
}

// ---------------------------------------------------------------------------------------------------------------------

TSharedRef<FJsonObject> FTCRoomListing::ToJson() const
{
	TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
	O->SetBoolField(TEXT("public"), bPublic);
	O->SetStringField(TEXT("title"), Title);
	O->SetBoolField(TEXT("tutorial"), bTutorial);
	return O;
}

FString FTCRoomListing::ServerTitle() const
{
	// server.js: title.replace(/[<>&"'\x00-\x1f]/g, '').trim().slice(0, 48) || 'Open skirmish'
	FString Clean;
	for (const TCHAR C : Title) if (!(C == '<' || C == '>' || C == '&' || C == '"' || C == '\'' || C < 0x20)) Clean.AppendChar(C);
	Clean = TCRoomL::Slice(TCRoomL::JsTrim(Clean), 48);
	return Clean.IsEmpty() ? FString(TEXT("Open skirmish")) : Clean;
}

FTCRoomBrowser::FTCRoomBrowser() : Alive(MakeShared<bool>(true)) {}
FTCRoomBrowser::~FTCRoomBrowser() { *Alive = false; }

FString FTCRoomBrowser::RoomsUrl(const FString& SocketUrl)
{
	const FString O = TCRoomL::Origin(SocketUrl);
	return O.IsEmpty() ? FString() : O + TEXT("/api/rooms");
}

bool FTCRoomBrowser::ApplyRooms(const FString& Body)
{
	TSharedPtr<FJsonObject> Data;
	const TArray<TSharedPtr<FJsonValue>>* List = nullptr;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Body), Data) || !Data.IsValid() || !Data->TryGetArrayField(TEXT("rooms"), List) || !List)
	{
		ApplyFailure();
		return false;
	}
	TArray<FTCRoomInfo> Got;
	for (const TSharedPtr<FJsonValue>& V : *List)
	{
		// render() reads these fields as they come; a room without them throws inside refresh's try, which then shows
		// the page as Offline. The same here: a malformed room fails the whole answer.
		if (!V.IsValid() || V->Type != EJson::Object) { ApplyFailure(); return false; }
		const FJsonObject& R = *V->AsObject();
		FTCRoomInfo I;
		if (!TCRoomL::Str(R, TEXT("code"), I.Code) || !TCRoomL::Str(R, TEXT("title"), I.Title) || !TCRoomL::Str(R, TEXT("mode"), I.Mode)
			|| !TCRoomL::Str(R, TEXT("map"), I.Map) || !TCRoomL::Str(R, TEXT("state"), I.State)
			|| !TCRoomL::Num(R, TEXT("humans"), I.Humans) || !TCRoomL::Num(R, TEXT("ai"), I.Ai) || !TCRoomL::Num(R, TEXT("occupied"), I.Occupied)
			|| !TCRoomL::Num(R, TEXT("capacity"), I.Capacity)) { ApplyFailure(); return false; }
		TCRoomL::Str(R, TEXT("worldSize"), I.WorldSize);
		I.bJoinable = TCRoomL::Flag(R, TEXT("joinable"));
		Got.Add(MoveTemp(I));
	}
	RoomList = MoveTemp(Got);
	bAnswered = true; bOnline = true; AnswerCount++;
	return true;
}

void FTCRoomBrowser::ApplyFailure()
{
	RoomList.Reset();
	bAnswered = true; bOnline = false; AnswerCount++;
}

const TArray<FString>& FTCRoomBrowser::ModeFilters()
{
	static const TArray<FString> L = [] { TArray<FString> A; for (const TCHAR* M : TCRoomL::FilterOrder) A.Add(M); return A; }();
	return L;
}

FString FTCRoomBrowser::ModeFilterLabel(const FString& Mode)
{
	return Mode.IsEmpty() ? FString(TEXT("All modes")) : ModeName(Mode);
}

void FTCRoomBrowser::SetModeFilter(const FString& Mode)
{
	if (ModeFilters().Contains(Mode)) Filter = Mode;
}

void FTCRoomBrowser::StepModeFilter(int32 Direction)
{
	const TArray<FString>& L = ModeFilters();
	const int32 At = FMath::Max(0, L.IndexOfByKey(Filter));
	Filter = L[((At + (Direction < 0 ? -1 : 1)) % L.Num() + L.Num()) % L.Num()];
}

FString FTCRoomBrowser::ModeName(const FString& Mode)
{
	for (const TCRoomL::FMode& M : TCRoomL::Modes) if (Mode == M.Key) return M.Name;
	return Mode;
}

TArray<FTCRoomRow> FTCRoomBrowser::Rows() const
{
	TArray<FTCRoomRow> Out;
	for (const FTCRoomInfo& R : RoomList)
	{
		if (!Filter.IsEmpty() && R.Mode != Filter) continue;
		if (bOpenOnly && !R.bJoinable) continue;
		FTCRoomRow W;
		W.Code = R.Code;
		W.Title = R.Title;
		W.MapLine = R.Map.Replace(TEXT("-"), TEXT(" ")) + TEXT(" \u00B7 ") + R.Code;
		W.Mode = ModeName(R.Mode);
		W.Seats = FString::Printf(TEXT("%d/%d"), R.Occupied, R.Capacity);
		W.Counts = FString::Printf(TEXT("%d human%s"), R.Humans, R.Humans == 1 ? TEXT("") : TEXT("s"));
		if (R.Ai) W.Counts += FString::Printf(TEXT(" \u00B7 %d AI"), R.Ai);
		W.Status = R.State == TEXT("play") ? TEXT("In progress") : R.bJoinable ? TEXT("Open") : TEXT("Full");
		W.bOpen = R.bJoinable;
		W.bJoinEnabled = R.bJoinable;
		W.JoinLabel = TEXT("Join");
		W.JoinAria = TEXT("Join ") + R.Title;
		Out.Add(MoveTemp(W));
	}
	return Out;
}

FString FTCRoomBrowser::StatusLine() const
{
	if (!bAnswered) return TEXT("Connecting\u2026");
	if (!bOnline) return TEXT("Offline");
	return FString::Printf(TEXT("%d public match%s \u00B7 Live"), RoomList.Num(), RoomList.Num() == 1 ? TEXT("") : TEXT("es"));
}

bool FTCRoomBrowser::EmptyShown() const
{
	return bAnswered && Rows().Num() == 0;
}

FString FTCRoomBrowser::EmptyLine() const
{
	if (!bAnswered) return FString();
	if (!bOnline) return TEXT("Cannot reach the match server. Retry with Refresh.");
	return RoomList.Num() ? TEXT("No matches fit these filters.") : TEXT("No public matches yet. Create one and invite the first players.");
}

FString FTCRoomBrowser::Nickname(const FString& Typed)
{
	// enter(): $('nickname').value.trim().slice(0, 16) || 'Soldier' (the input already stops at 16)
	const FString N = TCRoomL::Slice(TCRoomL::JsTrim(TCRoomL::Slice(Typed, NameMax)), NameMax);
	return N.IsEmpty() ? FString(TEXT("Soldier")) : N;
}

FString FTCRoomBrowser::NewCode(TFunctionRef<uint8()> RandomByte)
{
	FString S;
	for (int32 I = 0; I < 6; I++) S += FString::Printf(TEXT("%02x"), RandomByte());
	return S;
}

FString FTCRoomBrowser::NewCode()
{
	// crypto.getRandomValues: the platform GUID's random bits (getrandom on Linux), as the Tutorial entry takes them
	const FGuid G = FGuid::NewGuid();
	const uint32 Parts[2] = { G.C, G.D };
	int32 K = 0;
	return NewCode([&]() { const uint8 B = (uint8)(Parts[K / 4] >> (8 * (K % 4))); K++; return B; });
}

bool FTCRoomBrowser::ValidCode(const FString& Code)
{
	if (Code.Len() < 3 || Code.Len() > 12) return false;
	for (const TCHAR C : Code) if (!TCRoomL::Alnum(C)) return false;
	return true;
}

FTCRoomEntry FTCRoomBrowser::Join(const FString& Code, const FString& Typed) const
{
	FTCRoomEntry E;
	E.Name = Nickname(Typed);
	const FTCRoomInfo* R = RoomList.FindByPredicate([&](const FTCRoomInfo& I) { return I.Code == Code; });
	if (!R) { E.Note = TEXT("that room is not in the directory"); return E; }
	if (!R->bJoinable) { E.Note = TEXT("the Join button is disabled: the room is not joinable"); return E; }
	// enter(room.code): the page goes to /play#code, where roomAddress lowercases it
	E.Room = RoomAddress(TEXT("#") + R->Code).Room;
	E.bGo = true;
	E.Note = FString::Printf(TEXT("join %s"), *E.Room);
	return E;
}

FTCRoomEntry FTCRoomBrowser::JoinCode(const FString& TypedCode, const FString& Typed)
{
	FTCRoomEntry E;
	E.Name = Nickname(Typed);
	const FString Code = TCRoomL::Slice(TypedCode, CodeMax);
	// <input pattern="[a-zA-Z0-9]{3,12}" required>: the form is not sent otherwise
	if (!ValidCode(Code)) { E.Note = TEXT("a room code is 3 to 12 letters or digits"); return E; }
	E.Room = TCRoomL::AsciiLower(TCRoomL::JsTrim(Code));
	E.bGo = true;
	E.Note = FString::Printf(TEXT("join %s"), *E.Room);
	return E;
}

FTCRoomEntry FTCRoomBrowser::Create(const FString& Code, const FString& TypedTitle, bool bPublic, const FString& Typed)
{
	FTCRoomEntry E;
	E.Name = Nickname(Typed);
	E.Room = Code;
	E.Listing.bPublic = bPublic;
	const FString Title = TCRoomL::JsTrim(TCRoomL::Slice(TypedTitle, TitleMax));
	E.Listing.Title = Title.IsEmpty() ? FString(TEXT("Open skirmish")) : Title;
	E.bListingGiven = true;
	E.bGo = ValidCode(Code);
	E.Note = E.bGo ? FString::Printf(TEXT("create %s room %s"), bPublic ? TEXT("a public") : TEXT("an unlisted"), *Code) : FString(TEXT("not a room code"));
	return E;
}

FTCRoomEntry FTCRoomBrowser::Tutorial(const FString& Code, const FString& Typed)
{
	FTCRoomEntry E;
	E.Name = Nickname(Typed);
	E.Room = Code;
	E.Listing.bPublic = false;
	E.Listing.Title = TEXT("Tutorial");
	E.Listing.bTutorial = true;
	E.bListingGiven = true;
	E.bGo = ValidCode(Code);
	E.Note = E.bGo ? FString::Printf(TEXT("a new Tutorial room %s"), *Code) : FString(TEXT("not a room code"));
	return E;
}

FTCRoomEntry FTCRoomBrowser::QuickPlay(bool bRefreshOk, const FString& NewRoomCode, const FString& Typed) const
{
	if (!bRefreshOk) { FTCRoomEntry E; E.Name = Nickname(Typed); E.Note = QuickPlayNotice(); return E; }
	const FTCRoomInfo* Best = nullptr;
	for (const FTCRoomInfo& R : RoomList)
	{
		if (!R.bJoinable || R.Mode != TEXT("conquest")) continue;
		if (!Best || R.Humans > Best->Humans || (R.Humans == Best->Humans && TCRoomL::CodeCompare(R.Code, Best->Code) < 0)) Best = &R;
	}
	if (Best)
	{
		FTCRoomEntry E;
		E.Name = Nickname(Typed);
		E.Room = RoomAddress(TEXT("#") + Best->Code).Room;
		E.bGo = true;
		E.Note = FString::Printf(TEXT("quick play: join %s (%d human%s)"), *E.Room, Best->Humans, Best->Humans == 1 ? TEXT("") : TEXT("s"));
		return E;
	}
	FTCRoomEntry E = Create(NewRoomCode, FString(), true, Typed);
	E.Note = TEXT("quick play: no joinable Conquest room, ") + E.Note;
	return E;
}

bool FTCRoomBrowser::Request(const FString& SocketUrl, double Now)
{
	if (bLoading) return false;
	const FString Url = RoomsUrl(SocketUrl);
	if (Url.IsEmpty()) { LastRefreshAt = Now; ApplyFailure(); return false; }
	bLoading = true;
	LastRefreshAt = Now;
	const int32 MyTicket = ++Ticket;
	TSharedRef<IHttpRequest, ESPMode::ThreadSafe> Req = FHttpModule::Get().CreateRequest();
	Req->SetURL(Url);
	Req->SetVerb(TEXT("GET"));
	Req->SetHeader(TEXT("Cache-Control"), TEXT("no-store")); // fetch(..., { cache: 'no-store' })
	Req->SetTimeout(TimeoutSeconds);
	TWeakPtr<bool> WeakAlive = Alive;
	Req->OnProcessRequestComplete().BindLambda([this, WeakAlive, MyTicket](FHttpRequestPtr, FHttpResponsePtr Resp, bool bOk)
	{
		const TSharedPtr<bool> A = WeakAlive.Pin();
		if (!A || !*A || MyTicket != Ticket) return;
		bLoading = false;
		// if (!response.ok) throw: a status outside 200 to 299
		if (!bOk || !Resp.IsValid() || Resp->GetResponseCode() < 200 || Resp->GetResponseCode() > 299) { ApplyFailure(); return; }
		ApplyRooms(Resp->GetContentAsString());
	});
	Req->ProcessRequest();
	return true;
}

void FTCRoomBrowser::Cancel()
{
	Ticket++;
	bLoading = false;
}

FTCRoomAddress FTCRoomBrowser::RoomAddress(const FString& Hash, const FString& Main)
{
	// roomAddress(hash, main): the code before the first '&', the seat from the rest as URL parameters
	FString Body = Hash.StartsWith(TEXT("#")) ? Hash.Mid(1) : Hash;
	FString Code = Body, Suffix;
	int32 Amp = INDEX_NONE;
	if (Body.FindChar('&', Amp)) { Code = Body.Left(Amp); Suffix = Body.Mid(Amp + 1); }
	FTCRoomAddress A;
	const FString Low = TCRoomL::AsciiLower(Code);
	bool bOk = Low.Len() >= 3 && Low.Len() <= 12;
	for (const TCHAR C : Low) bOk = bOk && (TCRoomL::Lower(C) || TCRoomL::Digit(C));
	A.Room = bOk ? Low : Main;
	// new URLSearchParams(suffix).get('seat'): the first seat parameter; '+' is a space and %xx decoded
	TArray<FString> Params;
	Suffix.ParseIntoArray(Params, TEXT("&"), false);
	for (const FString& P : Params)
	{
		FString K = P, V;
		int32 Eq = INDEX_NONE;
		if (P.FindChar('=', Eq)) { K = P.Left(Eq); V = P.Mid(Eq + 1); }
		if (K != TEXT("seat")) continue;
		V = V.Replace(TEXT("+"), TEXT(" "));
		// percent-decoding: a seat that survives the check below holds only [A-Za-z0-9_-], so decode those
		FString D;
		for (int32 I = 0; I < V.Len(); I++)
		{
			if (V[I] == '%' && I + 2 < V.Len() + 0 && FChar::IsHexDigit(V[I + 1]) && FChar::IsHexDigit(V[I + 2]))
			{
				D.AppendChar((TCHAR)FParse::HexNumber(*V.Mid(I + 1, 2)));
				I += 2;
			}
			else D.AppendChar(V[I]);
		}
		bool bSeat = D.Len() >= 1 && D.Len() <= 32;
		for (const TCHAR C : D) bSeat = bSeat && (TCRoomL::Alnum(C) || C == '_' || C == '-');
		A.Seat = bSeat ? D : FString();
		break;
	}
	const FString Part = (A.Room == Main ? FString() : A.Room) + (A.Seat.IsEmpty() ? FString() : TEXT("&seat=") + A.Seat);
	A.Hash = Part.IsEmpty() ? FString() : TEXT("#") + Part;
	return A;
}

bool FTCRoomBrowser::ParseInviteLink(const FString& Link, FString& OutSocketUrl, FTCRoomAddress& OutAddress)
{
	const FString L = TCRoomL::JsTrim(Link);
	FString Scheme, Rest;
	if (L.StartsWith(TEXT("http://"), ESearchCase::IgnoreCase)) { Scheme = TEXT("ws://"); Rest = L.Mid(7); }
	else if (L.StartsWith(TEXT("https://"), ESearchCase::IgnoreCase)) { Scheme = TEXT("wss://"); Rest = L.Mid(8); }
	else return false;
	int32 Cut = INDEX_NONE;
	for (int32 I = 0; I < Rest.Len(); I++) if (Rest[I] == '/' || Rest[I] == '?' || Rest[I] == '#') { Cut = I; break; }
	const FString Host = Cut == INDEX_NONE ? Rest : Rest.Left(Cut);
	if (Host.IsEmpty()) return false;
	FString Tail = Cut == INDEX_NONE ? FString() : Rest.Mid(Cut);
	FString Hash;
	int32 HashAt = INDEX_NONE;
	if (Tail.FindChar('#', HashAt)) { Hash = Tail.Mid(HashAt); Tail = Tail.Left(HashAt); }
	int32 Q = INDEX_NONE;
	if (Tail.FindChar('?', Q)) Tail = Tail.Left(Q);
	// roomLink writes <origin>/play#room; the bare site (the directory) forwards an old /#code link to /play
	if (!(Tail == TEXT("/play") || Tail == TEXT("/") || Tail.IsEmpty())) return false;
	OutSocketUrl = Scheme + Host + TEXT("/ws");
	OutAddress = RoomAddress(Hash);
	return true;
}

// ---------------------------------------------------------------------------------------------------------------------
#if WITH_DEV_AUTOMATION_TESTS

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRoomBrowserReferenceTest, "ThreeCrossroads.RoomBrowser.Reference", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

// BEGIN GENERATED FIXTURES (round-15 lobby tools/write-fixtures.mjs from directory-oracle.json: the reference's own
// client/public-lobby.js and client/room-session.js under node, fed by GET /api/rooms recorded from an isolated
// reference server at e4e579a, run tag abyq)
namespace TCRoomFx
{
	struct FAddress { const TCHAR* Hash; const TCHAR* Room; const TCHAR* Seat; const TCHAR* Out; };
	const FAddress Addresses[] = {
		{ TEXT(""), TEXT("main"), TEXT(""), TEXT("") },
		{ TEXT("#"), TEXT("main"), TEXT(""), TEXT("") },
		{ TEXT("#main"), TEXT("main"), TEXT(""), TEXT("") },
		{ TEXT("#abc"), TEXT("abc"), TEXT(""), TEXT("#abc") },
		{ TEXT("#ABC"), TEXT("abc"), TEXT(""), TEXT("#abc") },
		{ TEXT("#Ab3"), TEXT("ab3"), TEXT(""), TEXT("#ab3") },
		{ TEXT("#ab"), TEXT("main"), TEXT(""), TEXT("") },
		{ TEXT("#abcdefghijkl"), TEXT("abcdefghijkl"), TEXT(""), TEXT("#abcdefghijkl") },
		{ TEXT("#abcdefghijklm"), TEXT("main"), TEXT(""), TEXT("") },
		{ TEXT("#ab-c"), TEXT("main"), TEXT(""), TEXT("") },
		{ TEXT("#abc&seat=guest"), TEXT("abc"), TEXT("guest"), TEXT("#abc&seat=guest") },
		{ TEXT("#abc&seat=Guest_2-x"), TEXT("abc"), TEXT("Guest_2-x"), TEXT("#abc&seat=Guest_2-x") },
		{ TEXT("#abc&seat=bad seat"), TEXT("abc"), TEXT(""), TEXT("#abc") },
		{ TEXT("#abc&seat=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"), TEXT("abc"), TEXT(""), TEXT("#abc") },
		{ TEXT("#abc&seat="), TEXT("abc"), TEXT(""), TEXT("#abc") },
		{ TEXT("#&seat=two"), TEXT("main"), TEXT("two"), TEXT("#&seat=two") },
		{ TEXT("#main&seat=two"), TEXT("main"), TEXT("two"), TEXT("#&seat=two") },
		{ TEXT("#abc&x=1&seat=s1"), TEXT("abc"), TEXT("s1"), TEXT("#abc&seat=s1") },
		{ TEXT("#abc&seat=a&seat=b"), TEXT("abc"), TEXT("a"), TEXT("#abc&seat=a") },
		{ TEXT("#abc&seat=a%2Db"), TEXT("abc"), TEXT("a-b"), TEXT("#abc&seat=a-b") },
		{ TEXT("abc"), TEXT("abc"), TEXT(""), TEXT("#abc") },
		{ TEXT("#ABC&seat=a+b"), TEXT("abc"), TEXT(""), TEXT("#abc") },
	};
	const TCHAR* const Directory = TEXT("{\"rooms\":[{\"code\":\"qaabyq\",\"title\":\"Friday night\",\"mode\":\"conquest\",\"map\":\"default\",\"state\":\"lobby\",\"humans\":2,\"ai\":0,\"occupied\":2,\"capacity\":3,\"joinable\":true},{\"code\":\"qbabyq\",\"title\":\"Open skirmish\",\"mode\":\"conquest\",\"map\":\"default\",\"state\":\"lobby\",\"humans\":1,\"ai\":0,\"occupied\":1,\"capacity\":3,\"joinable\":true},{\"code\":\"qhabyq\",\"title\":\"Hold the bunker\",\"mode\":\"horde\",\"map\":\"bastogne\",\"state\":\"lobby\",\"humans\":1,\"ai\":0,\"occupied\":1,\"capacity\":5,\"joinable\":true},{\"code\":\"qpabyq\",\"title\":\"Running now\",\"mode\":\"conquest\",\"map\":\"default\",\"state\":\"play\",\"humans\":1,\"ai\":1,\"occupied\":2,\"capacity\":3,\"joinable\":false},{\"code\":\"qfabyq\",\"title\":\"Full house\",\"mode\":\"conquest\",\"map\":\"default\",\"state\":\"lobby\",\"humans\":1,\"ai\":2,\"occupied\":3,\"capacity\":3,\"joinable\":false}]}");
	const TCHAR* const DirectoryStatus = TEXT("5 public matches \u00B7 Live");
	struct FRow { const TCHAR* Cells; };
	const FRow DirectoryRows[] = {
		{ TEXT("Friday night|default \u00B7 qaabyq|Conquest|2/3|2 humans|Open|open|enabled|Join Friday night") },
		{ TEXT("Open skirmish|default \u00B7 qbabyq|Conquest|1/3|1 human|Open|open|enabled|Join Open skirmish") },
		{ TEXT("Hold the bunker|bastogne \u00B7 qhabyq|Horde|1/5|1 human|Open|open|enabled|Join Hold the bunker") },
		{ TEXT("Running now|default \u00B7 qpabyq|Conquest|2/3|1 human \u00B7 1 AI|In progress||disabled|Join Running now") },
		{ TEXT("Full house|default \u00B7 qfabyq|Conquest|3/3|1 human \u00B7 2 AI|Full||disabled|Join Full house") },
	};
	const int32 HordeRows = 1;
	const int32 OpenRows = 3;
	const TCHAR* const QuickPlayRoom = TEXT("qaabyq");
	const TCHAR* const ServerTitleIn = TEXT(" <b>Tom's & co</b>\u0009");
	const TCHAR* const ServerTitleOut = TEXT("bToms  co/b");
	constexpr double RefreshMs = 5000;
}
// END GENERATED FIXTURES

bool FTCRoomBrowserReferenceTest::RunTest(const FString& Parameters)
{
	// roomAddress: every case answered by client/room-session.js itself under node (TCRoomBrowserFixtures.inl)
	for (const TCRoomFx::FAddress& C : TCRoomFx::Addresses)
	{
		const FTCRoomAddress A = FTCRoomBrowser::RoomAddress(C.Hash);
		TestEqual(FString::Printf(TEXT("roomAddress('%s').room"), C.Hash), A.Room, FString(C.Room));
		TestEqual(FString::Printf(TEXT("roomAddress('%s').seat"), C.Hash), A.Seat, FString(C.Seat));
		TestEqual(FString::Printf(TEXT("roomAddress('%s').hash"), C.Hash), A.Hash, FString(C.Out));
	}

	// the URL of the directory from the socket's
	TestEqual(TEXT("rooms url"), FTCRoomBrowser::RoomsUrl(TEXT("ws://127.0.0.1:39405/ws")), FString(TEXT("http://127.0.0.1:39405/api/rooms")));
	TestEqual(TEXT("rooms url tls"), FTCRoomBrowser::RoomsUrl(TEXT("wss://host.example/ws?room=abc")), FString(TEXT("https://host.example/api/rooms")));
	TestEqual(TEXT("rooms url refused"), FTCRoomBrowser::RoomsUrl(TEXT("http://x/ws")), FString());

	// the page before an answer, then a live answer recorded from the isolated reference server
	FTCRoomBrowser B;
	TestEqual(TEXT("connecting"), B.StatusLine(), FString(TEXT("Connecting\u2026")));
	TestFalse(TEXT("no empty line before an answer"), B.EmptyShown());
	TestTrue(TEXT("recorded directory parses"), B.ApplyRooms(TCRoomFx::Directory));
	TestEqual(TEXT("status line"), B.StatusLine(), FString(TCRoomFx::DirectoryStatus));
	{
		const TArray<FTCRoomRow> Rows = B.Rows();
		TestEqual(TEXT("row count"), Rows.Num(), (int32)UE_ARRAY_COUNT(TCRoomFx::DirectoryRows));
		for (int32 I = 0; I < FMath::Min(Rows.Num(), (int32)UE_ARRAY_COUNT(TCRoomFx::DirectoryRows)); I++)
		{
			const TCRoomFx::FRow& E = TCRoomFx::DirectoryRows[I];
			const FTCRoomRow& R = Rows[I];
			const FString Got = FString::Join(TArray<FString>{ R.Title, R.MapLine, R.Mode, R.Seats, R.Counts, R.Status, R.bOpen ? TEXT("open") : TEXT(""), R.bJoinEnabled ? TEXT("enabled") : TEXT("disabled"), R.JoinAria }, TEXT("|"));
			TestEqual(FString::Printf(TEXT("row %d as the page renders it"), I), Got, FString(E.Cells));
		}
	}
	// filters: mode, then open seats only
	B.SetModeFilter(TEXT("horde"));
	TestEqual(TEXT("horde rows"), B.Rows().Num(), TCRoomFx::HordeRows);
	if (TCRoomFx::HordeRows == 0) TestEqual(TEXT("filtered away"), B.EmptyLine(), FString(TEXT("No matches fit these filters.")));
	B.SetModeFilter(FString());
	B.SetOpenOnly(true);
	TestEqual(TEXT("open rows"), B.Rows().Num(), TCRoomFx::OpenRows);
	B.SetOpenOnly(false);
	// the filter cycles through the select's options in order
	B.StepModeFilter(1);
	TestEqual(TEXT("filter after All modes"), B.ModeFilter(), FString(TEXT("conquest")));
	B.StepModeFilter(-1); B.StepModeFilter(-1);
	TestEqual(TEXT("filter before All modes"), B.ModeFilter(), FString(TEXT("world")));
	TestEqual(TEXT("filter label"), FTCRoomBrowser::ModeFilterLabel(B.ModeFilter()), FString(TEXT("World Conquest")));
	B.SetModeFilter(FString());

	// Quick Play over the recorded directory: the joinable Conquest room with the most humans, ties by code
	{
		const FTCRoomEntry Q = B.QuickPlay(true, TEXT("aaaaaaaaaaaa"), TEXT("  Rook  "));
		TestEqual(TEXT("quick play room"), Q.Room, FString(TCRoomFx::QuickPlayRoom));
		TestFalse(TEXT("quick play into an existing room passes no listing"), Q.bListingGiven);
		TestEqual(TEXT("nickname trimmed"), Q.Name, FString(TEXT("Rook")));
		const FTCRoomEntry N = B.QuickPlay(false, TEXT("aaaaaaaaaaaa"), TEXT(""));
		TestFalse(TEXT("quick play without a directory"), N.bGo);
		TestEqual(TEXT("quick play notice"), N.Note, FTCRoomBrowser::QuickPlayNotice());
	}
	// joining a full or running room is disabled
	for (const FTCRoomInfo& R : B.Rows().Num() ? B.Rooms() : TArray<FTCRoomInfo>())
		TestEqual(FString::Printf(TEXT("join %s"), *R.Code), B.Join(R.Code, TEXT("x")).bGo, R.bJoinable);

	// an empty directory, a failure
	{
		FTCRoomBrowser E;
		TestTrue(TEXT("empty directory"), E.ApplyRooms(TEXT("{\"rooms\":[]}")));
		TestEqual(TEXT("no rooms status"), E.StatusLine(), FString(TEXT("0 public matches \u00B7 Live")));
		TestEqual(TEXT("no rooms line"), E.EmptyLine(), FString(TEXT("No public matches yet. Create one and invite the first players.")));
		const FTCRoomEntry Q = E.QuickPlay(true, TEXT("0123456789ab"), TEXT(""));
		TestTrue(TEXT("quick play creates"), Q.bGo && Q.Room == TEXT("0123456789ab") && Q.bListingGiven && Q.Listing.bPublic && Q.Listing.Title == TEXT("Open skirmish") && !Q.Listing.bTutorial);
		TestEqual(TEXT("default nickname"), Q.Name, FString(TEXT("Soldier")));
		TestFalse(TEXT("not json"), E.ApplyRooms(TEXT("<html>")));
		TestEqual(TEXT("offline"), E.StatusLine(), FString(TEXT("Offline")));
		TestEqual(TEXT("offline line"), E.EmptyLine(), FString(TEXT("Cannot reach the match server. Retry with Refresh.")));
		TestTrue(TEXT("offline shows the line"), E.EmptyShown());
		TestFalse(TEXT("rooms not an array"), E.ApplyRooms(TEXT("{\"rooms\":{}}")));
		TestTrue(TEXT("one room"), E.ApplyRooms(TEXT("{\"rooms\":[{\"code\":\"abc\",\"title\":\"T\",\"mode\":\"horde\",\"map\":\"the-bridge\",\"state\":\"lobby\",\"humans\":1,\"ai\":0,\"occupied\":1,\"capacity\":3,\"joinable\":true}]}")));
		TestEqual(TEXT("one room status"), E.StatusLine(), FString(TEXT("1 public match \u00B7 Live")));
		TestEqual(TEXT("one human, no AI"), E.Rows()[0].Counts, FString(TEXT("1 human")));
		TestEqual(TEXT("map line"), E.Rows()[0].MapLine, FString(TEXT("the bridge \u00B7 abc")));
		TestFalse(TEXT("a room without its fields fails the answer"), E.ApplyRooms(TEXT("{\"rooms\":[{\"code\":\"abc\"}]}")));
	}

	// the page's actions and the listing they pass on
	{
		const FTCRoomEntry C = FTCRoomBrowser::Create(TEXT("00ff00ff00ff"), TEXT("   "), false, TEXT("Abcdefghijklmnopqrstu"));
		TestTrue(TEXT("create unlisted"), C.bGo && !C.Listing.bPublic && C.Listing.Title == TEXT("Open skirmish"));
		TestEqual(TEXT("nickname cut to 16"), C.Name, FString(TEXT("Abcdefghijklmnop")));
		const FTCRoomEntry P = FTCRoomBrowser::Create(TEXT("00ff00ff00ff"), TEXT(" Friday night "), true, TEXT(""));
		TestEqual(TEXT("create public title"), P.Listing.Title, FString(TEXT("Friday night")));
		const FTCRoomEntry T = FTCRoomBrowser::Tutorial(TEXT("00ff00ff00ff"), TEXT(""));
		TestTrue(TEXT("tutorial listing"), T.Listing.bTutorial && !T.Listing.bPublic && T.Listing.Title == TEXT("Tutorial") && !T.Listing.ServerListed());
		TestTrue(TEXT("join code lowercased"), FTCRoomBrowser::JoinCode(TEXT("CrossRoads"), TEXT("")).Room == TEXT("crossroads"));
		TestFalse(TEXT("join code too short"), FTCRoomBrowser::JoinCode(TEXT("ab"), TEXT("")).bGo);
		TestFalse(TEXT("join code with a space"), FTCRoomBrowser::JoinCode(TEXT("ab c"), TEXT("")).bGo);
		TestFalse(TEXT("join code with a dash"), FTCRoomBrowser::JoinCode(TEXT("ab-cd"), TEXT("")).bGo);
		int32 K = 0;
		TestEqual(TEXT("new code"), FTCRoomBrowser::NewCode([&]() { return (uint8)(K++ * 51); }), FString(TEXT("00336699ccff")));
		const FString Fresh = FTCRoomBrowser::NewCode();
		TestTrue(TEXT("a fresh code is a room code"), Fresh.Len() == 12 && FTCRoomBrowser::ValidCode(Fresh));
	}
	// listing JSON, the browser's default, and the server's reading of a title
	{
		FString Wire;
		const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> W = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Wire);
		FJsonSerializer::Serialize(FTCRoomListing().ToJson(), W);
		TestEqual(TEXT("default listing"), Wire, FString(TEXT("{\"public\":false,\"title\":\"Open skirmish\",\"tutorial\":false}")));
		FTCRoomListing L; L.bPublic = true;
		L.Title = TCRoomFx::ServerTitleIn;
		TestEqual(TEXT("server title (server.js run in node)"), L.ServerTitle(), FString(TCRoomFx::ServerTitleOut));
		TestTrue(TEXT("public listed"), L.ServerListed());
	}
	// invite links: <origin>/play#room
	{
		FString Url; FTCRoomAddress A;
		TestTrue(TEXT("invite link"), FTCRoomBrowser::ParseInviteLink(TEXT("http://127.0.0.1:39405/play#Abc12&seat=guest"), Url, A));
		TestEqual(TEXT("invite socket"), Url, FString(TEXT("ws://127.0.0.1:39405/ws")));
		TestTrue(TEXT("invite room and seat"), A.Room == TEXT("abc12") && A.Seat == TEXT("guest"));
		TestTrue(TEXT("public url"), FTCRoomBrowser::ParseInviteLink(TEXT("https://box.tail1234.ts.net/play#main"), Url, A) && Url == TEXT("wss://box.tail1234.ts.net/ws") && A.Room == TEXT("main"));
		TestFalse(TEXT("not a link"), FTCRoomBrowser::ParseInviteLink(TEXT("abc12"), Url, A));
		TestFalse(TEXT("another page"), FTCRoomBrowser::ParseInviteLink(TEXT("http://h/maps/x#abc"), Url, A));
	}
	return true;
}

#endif
