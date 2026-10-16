#include "Client/TCMatchOptions.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Client/TCEndReport.h"
#include "Client/TCLocale.h"
#include "Client/TCStats.h"
#include "ThreeCrossroads.h"
#include "HttpModule.h"
#include "Interfaces/IHttpRequest.h"
#include "Interfaces/IHttpResponse.h"
#include "Engine/Canvas.h"
#include "Engine/Texture2D.h"
#include "CanvasItem.h"
#include "GlobalRenderResources.h"
#include "Misc/App.h"

namespace TCMOptL
{
	// shared/weather.js WEATHER: sight, infantry, vehicles, vehicles off roads; rains (Rain), soaks (Mud)
	struct FKind { const TCHAR* Key; const TCHAR* Name; double Sight, Inf, Veh, OffRoad; bool bRains, bSoaks; };
	const FKind Kinds[] = {
		{ TEXT("clear"), TEXT("Clear"), 1.0, 1.0, 1.0, 1.0, false, false },
		{ TEXT("fog"), TEXT("Ground fog"), 0.7, 1.0, 1.0, 1.0, false, false },
		{ TEXT("rain"), TEXT("Rain"), 0.8, 1.0, 1.0, 0.8, true, false },
		{ TEXT("mud"), TEXT("Mud"), 1.0, 0.9, 1.0, 0.8, false, true },
		{ TEXT("snow"), TEXT("Snow"), 0.9, 0.9, 0.85, 1.0, false, false },
	};
	// WEATHER_CHOICES = ['map', ...WEATHER_KINDS, 'random'], labels client/weather-view.js LABEL
	const TCHAR* const Choices[] = { TEXT("map"), TEXT("clear"), TEXT("fog"), TEXT("rain"), TEXT("mud"), TEXT("snow"), TEXT("random") };
	const TCHAR* const ChoiceLabels[] = { TEXT("Map default"), TEXT("Clear"), TEXT("Ground fog"), TEXT("Rain"), TEXT("Mud"), TEXT("Snow"), TEXT("Random") };
	static_assert(UE_ARRAY_COUNT(Choices) == UE_ARRAY_COUNT(Kinds) + 2 && UE_ARRAY_COUNT(ChoiceLabels) == UE_ARRAY_COUNT(Choices), "map, the kinds, random");
	// shared/weather.js MOOD_NAMES and MOOD_RULES (case-insensitive alternatives, tried in this order)
	const TCHAR* const MoodNames[] = { TEXT("warm"), TEXT("dawn"), TEXT("overcast"), TEXT("snow"), TEXT("dust") };
	struct FMoodRule { const TCHAR* Mood; const TCHAR* Words[6]; };
	const FMoodRule MoodRules[] = {
		{ TEXT("snow"), { TEXT("ardennes"), TEXT("bastogne"), TEXT("bulge"), TEXT("winter"), TEXT("snow"), nullptr } },
		{ TEXT("dust"), { TEXT("kasserine"), TEXT("desert"), TEXT("tobruk"), TEXT("alamein"), TEXT("africa"), TEXT("tunis") } },
		{ TEXT("dawn"), { TEXT("pegasus"), TEXT("polder"), TEXT("river"), TEXT("canal"), TEXT("marsh"), nullptr } },
		{ TEXT("overcast"), { TEXT("bocage"), TEXT("cassino"), TEXT("stalingrad"), TEXT("seawall"), TEXT("rain"), TEXT("storm") } },
	};
	constexpr int32 DawnFogSeconds = 240; // mapWeather: misty river dawns lift after 4 minutes

	// client/main.js MM_COLORS, by terrain character ('.' for any other)
	struct FCellColor { TCHAR Ch; uint8 R, G, B; };
	const FCellColor CellColors[] = {
		{ '.', 108, 118, 69 }, { 'B', 150, 132, 100 }, { 'H', 47, 74, 34 }, { '#', 154, 149, 138 }, { '+', 90, 79, 54 },
		{ 'T', 62, 50, 34 }, { 'W', 60, 93, 112 }, { '=', 122, 90, 58 }, { 'F', 106, 127, 122 }, { 'R', 122, 114, 102 },
		{ 'X', 96, 90, 70 }, { 'Y', 84, 84, 78 }, { 'D', 150, 128, 100 }, { 'M', 92, 80, 58 }, { 'N', 120, 96, 60 },
		{ 'O', 38, 62, 30 }, { 'A', 226, 220, 204 },
	};
	// shared/sim.js TERRAIN keys a map row may hold (validateMap: every key but 'K' and 'A')
	const TCHAR RowChars[] = TEXT(".BH#+TWF=RXYDMNOQ");

	const FColor Red(0xd9, 0x4a, 0x3d), Blue(0x3d, 0x7b, 0xd9), Chalk(0xf2, 0xec, 0xd8);
	const TCHAR* const Dot = TEXT("●");

	template <int32 N>
	int32 IndexOf(const TCHAR* const (&List)[N], const FString& Value)
	{
		for (int32 I = 0; I < N; I++) if (Value == List[I]) return I;
		return INDEX_NONE;
	}
	const FKind* FindKind(const FString& Key)
	{
		for (const FKind& K : Kinds) if (Key == K.Key) return &K;
		return nullptr;
	}

	// Math.round: the nearest integer, halves toward +infinity
	double JsRound(double X)
	{
		const double F = FMath::FloorToDouble(X);
		const double Frac = X - F;
		return Frac >= 0.5 ? F + 1.0 : F;
	}
	// ToUint8Clamp (Uint8ClampedArray, and so ImageData): NaN and below 0 to 0, above 255 to 255, halves to even
	uint8 Clamp8(double V)
	{
		if (!(V > 0.0)) return 0;
		if (V >= 255.0) return 255;
		const double F = FMath::FloorToDouble(V);
		const double Half = F + 0.5;
		if (Half < V) return (uint8)(F + 1.0);
		if (V < Half) return (uint8)F;
		return ((int32)F % 2 == 0) ? (uint8)F : (uint8)(F + 1.0);
	}
	// /re/i over ASCII words: JavaScript's case folding without the u flag never maps a non-ASCII character to ASCII
	TCHAR AsciiLower(TCHAR C) { return C >= 'A' && C <= 'Z' ? (TCHAR)(C - 'A' + 'a') : C; }
	bool ContainsAsciiNoCase(const FString& Haystack, const TCHAR* Word)
	{
		const int32 N = FCString::Strlen(Word);
		for (int32 I = 0; I + N <= Haystack.Len(); I++)
		{
			int32 K = 0;
			while (K < N && AsciiLower(Haystack[I + K]) == Word[K]) K++;
			if (K == N) return true;
		}
		return false;
	}

	// Typed reads: FJsonObject's TryGet*Field convert between strings, numbers and booleans, and the server's types
	// are part of what is checked here.
	bool Str(const FJsonObject& O, const TCHAR* Name, FString& Out)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(Name);
		if (!V.IsValid() || V->Type != EJson::String) return false;
		Out = V->AsString();
		return true;
	}
	bool Flag(const FJsonObject& O, const TCHAR* Name)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(Name);
		return V.IsValid() && V->Type == EJson::Boolean && V->AsBool();
	}
	// Number.isInteger(v) && Min <= v <= Max
	bool Int(const TSharedPtr<FJsonValue>& V, int32 Min, int32 Max, int32& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		const double D = V->AsNumber();
		if (!FMath::IsFinite(D) || D != FMath::FloorToDouble(D) || D < Min || D > Max) return false;
		Out = (int32)D;
		return true;
	}
	bool Int(const FJsonObject& O, const TCHAR* Name, int32 Min, int32 Max, int32& Out) { return Int(O.TryGetField(Name), Min, Max, Out); }
	// The field is not there (JSON has no undefined: a missing field is the only way to leave one out)
	bool Absent(const FJsonObject& O, const TCHAR* Name) { return !O.TryGetField(Name).IsValid(); }
	// v ?? Default: missing or null gives the default
	TSharedPtr<FJsonValue> Nullish(const FJsonObject& O, const TCHAR* Name)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(Name);
		return V.IsValid() && V->Type != EJson::Null ? V : nullptr;
	}
	// JavaScript truthiness of a JSON value (m.world)
	bool Truthy(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid()) return false;
		switch (V->Type)
		{
		case EJson::Boolean: return V->AsBool();
		case EJson::Number: { const double D = V->AsNumber(); return D != 0.0 && !FMath::IsNaN(D); }
		case EJson::String: return !V->AsString().IsEmpty();
		case EJson::Array:
		case EJson::Object: return true;
		default: return false;
		}
	}
	// typeof v === 'number' && isFinite(v) && Min <= v <= Max, with v ?? 1
	bool NumOrOne(const FJsonObject& O, const TCHAR* Name, double Min, double Max, double& Out)
	{
		const TSharedPtr<FJsonValue> V = Nullish(O, Name);
		if (!V.IsValid()) { Out = 1.0; return true; }
		if (V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return FMath::IsFinite(Out) && Out >= Min && Out <= Max;
	}
	const TArray<TSharedPtr<FJsonValue>>* Array(const FJsonObject& O, const TCHAR* Name)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(Name);
		return V.IsValid() && V->Type == EJson::Array ? &V->AsArray() : nullptr;
	}
	// shared/sim.js levelOf over validateMap's height characters
	int8 LevelOf(TCHAR C) { return C == 'a' ? -1 : C == 'b' ? -2 : (int8)(C - '0'); }
}

const TCHAR* LexToString(ETCMatchOption Option)
{
	return FTCMatchOptions::MessageType(Option);
}

const FTCLobbyChoice* FTCOptionControl::Find(const FString& Value) const
{
	for (const FTCLobbyChoice& C : Choices) if (C.Value == Value) return &C;
	return nullptr;
}

const TCHAR* FTCMatchOptions::MessageType(ETCMatchOption Kind)
{
	switch (Kind)
	{
	case ETCMatchOption::Weather: return TEXT("weather");
	case ETCMatchOption::Sandbox: return TEXT("sandbox");
	case ETCMatchOption::Kick: return TEXT("kick");
	}
	return TEXT("?");
}

// ---------- weather text ----------

const TArray<FString>& FTCMatchOptions::WeatherChoices()
{
	static const TArray<FString> List = [] { TArray<FString> L; for (const TCHAR* C : TCMOptL::Choices) L.Add(C); return L; }();
	return List;
}

bool FTCMatchOptions::IsWeatherKind(const FString& Kind)
{
	return TCMOptL::FindKind(Kind) != nullptr;
}

FString FTCMatchOptions::WeatherChoiceLabel(const FString& Choice)
{
	const int32 I = TCMOptL::IndexOf(TCMOptL::Choices, Choice);
	return I == INDEX_NONE ? Choice : FString(TCMOptL::ChoiceLabels[I]);
}

FString FTCMatchOptions::WeatherName(const FString& Kind)
{
	const TCMOptL::FKind* K = TCMOptL::FindKind(Kind);
	return K ? FString(K->Name) : Kind;
}

TArray<FString> FTCMatchOptions::WeatherEffects(const FString& Kind)
{
	const TCMOptL::FKind* W = TCMOptL::FindKind(Kind);
	if (!W) W = &TCMOptL::Kinds[0]; // WEATHER[kind] ?? WEATHER.clear
	// pct = (m) => `-${Math.round((1 - m) * 100)}%`
	auto Pct = [](double M) { const double Lost = 1.0 - M; const double Percent = Lost * 100.0; return FString::Printf(TEXT("-%d%%"), (int32)TCMOptL::JsRound(Percent)); };
	TArray<FString> Out;
	if (W->Sight < 1.0) Out.Add(TEXT("sight ") + Pct(W->Sight));
	if (W->Inf < 1.0) Out.Add(TEXT("infantry ") + Pct(W->Inf));
	if (W->Veh < 1.0) Out.Add(TEXT("vehicles ") + Pct(W->Veh));
	if (W->OffRoad < 1.0) Out.Add(TEXT("vehicles ") + Pct(W->OffRoad) + TEXT(" off roads"));
	return Out;
}

FString FTCMatchOptions::WeatherSaid(const FString& Kind)
{
	const TArray<FString> Effects = WeatherEffects(Kind);
	return FString::Printf(TEXT("%s: %s"), *WeatherName(Kind), Effects.Num() ? *FString::Join(Effects, TEXT(", ")) : TEXT("no effect"));
}

FString FTCMatchOptions::Clock(double Seconds)
{
	// `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, '0')}`
	const double Minutes = FMath::FloorToDouble(Seconds / 60.0);
	const double Rest = TCMOptL::JsRound(FMath::Fmod(Seconds, 60.0));
	return FString::Printf(TEXT("%d:%02d"), (int32)Minutes, (int32)Rest);
}

FString FTCMatchOptions::PlanShort(const FTCWeatherPlan& Plan)
{
	if (!Plan.IsSet()) return FString();
	if (Plan.Next.IsEmpty()) return WeatherName(Plan.Now);
	return FString::Printf(TEXT("%s until %s"), Plan.Now == TEXT("fog") ? TEXT("Fog") : *WeatherName(Plan.Now), *Clock(Plan.AtSeconds));
}

FString FTCMatchOptions::MapDefaultLabel(const FTCWeatherPlan* MapPlan)
{
	return MapPlan && MapPlan->IsSet() ? FString::Printf(TEXT("Map default: %s"), *PlanShort(*MapPlan)) : FString(TEXT("Map default"));
}

FString FTCMatchOptions::WeatherExplain(const FString& Choice, const FTCWeatherPlan* MapPlan)
{
	// showers come and go in Clear, Fog and Mud; Rain is one all match and Snow has none
	auto WithShowers = [](const FString& Kind, const FString& Said)
	{
		const TCMOptL::FKind* K = TCMOptL::FindKind(Kind);
		const bool bShowers = Kind != TEXT("snow") && !(K && K->bRains);
		return bShowers ? Said + TEXT(". Showers come and go") : Said;
	};
	if (Choice == TEXT("random")) return TEXT("Picked when the match starts. Fog may lift or rain may turn to mud partway through");
	if (Choice == TEXT("map"))
	{
		if (!MapPlan || !MapPlan->IsSet()) return TEXT("The weather this map brings");
		FString Said = WeatherSaid(MapPlan->Now);
		if (!MapPlan->Next.IsEmpty())
		{
			// plannedChange: "Lifts at 4:00", "Turns to mud at 6:00"
			const FString Change = MapPlan->Next == TEXT("clear") ? FString(TEXT("Lifts")) : FString::Printf(TEXT("Turns to %s"), *WeatherName(MapPlan->Next).ToLower());
			Said += FString::Printf(TEXT(". %s at %s"), *Change, *Clock(MapPlan->AtSeconds));
		}
		return WithShowers(MapPlan->Now, Said);
	}
	if (!TCMOptL::FindKind(Choice)) return FString(); // not a choice the browser lists
	return WithShowers(Choice, WeatherSaid(Choice));
}

FString FTCMatchOptions::MapMood(const FString& Name, const FString& Key, const FString& Mood)
{
	if (TCMOptL::IndexOf(TCMOptL::MoodNames, Mood) != INDEX_NONE) return Mood;
	const FString Haystack = Name + TEXT(" ") + Key; // `${map?.name ?? ''} ${key ?? ''}`
	for (const TCMOptL::FMoodRule& Rule : TCMOptL::MoodRules)
		for (const TCHAR* Word : Rule.Words)
			if (Word && TCMOptL::ContainsAsciiNoCase(Haystack, Word)) return Rule.Mood;
	return TEXT("warm");
}

FTCWeatherPlan FTCMatchOptions::MapWeather(const FString& Name, const FString& Key, const FString& Mood, const FString& WeatherField)
{
	FTCWeatherPlan Plan;
	if (IsWeatherKind(WeatherField)) { Plan.Now = WeatherField; return Plan; } // known(map.weather)
	const FString M = MapMood(Name, Key, Mood);
	if (M == TEXT("snow")) Plan.Now = TEXT("snow");
	else if (M == TEXT("dawn")) { Plan.Now = TEXT("fog"); Plan.Next = TEXT("clear"); Plan.AtSeconds = TCMOptL::DawnFogSeconds; }
	else Plan.Now = TEXT("clear");
	return Plan;
}

// ---------- the lobby ----------

void FTCMatchOptions::Reset()
{
	bHasLobby = false;
	State.Reset(); Role.Reset(); Mode.Reset(); MapName.Reset(); WorldSize.Reset(); Weather.Reset();
	bSandbox = bAmHost = bSpectator = bRustHost = false;
	You = -1;
	SeatList.Reset();
	Pending.Reset();
	Download.Reset(); NextMapTry = -1e9; PreviewImageKey.Reset(); PreviewImage.Reset();
}

void FTCMatchOptions::TickPreview(const FString& SocketUrl, double Now)
{
	if (PreviewServer != SocketUrl)
	{
		PreviewServer = SocketUrl; MapFiles.Reset(); Download.Reset(); NextMapTry = -1e9;
		PreviewImage.Reset(); PreviewImageKey.Reset();
	}
	if (Download && Download->bDone)
	{
		if (Download->bOk) AddMapFileText(Download->Key, Download->Body);
		Download.Reset();
	}
	const FString Key = WantedMap();
	if (!InLobby() || Key.IsEmpty() || Download || Now < NextMapTry) return;
	const FString Url = MapUrl(SocketUrl, Key);
	if (Url.IsEmpty()) return;
	NextMapTry = Now + 5.0;
	Download = MakeShared<FDownload>(); Download->Key = Key;
	const auto Result = Download;
	const auto Request = FHttpModule::Get().CreateRequest();
	Request->SetURL(Url); Request->SetVerb(TEXT("GET")); Request->SetTimeout(10.f);
	// Capturing only the result keeps callbacks safe when a room closes or the owner is destroyed.
	Request->OnProcessRequestComplete().BindLambda([Result](FHttpRequestPtr, FHttpResponsePtr Response, bool bOk)
	{
		Result->bOk = bOk && Response && Response->GetResponseCode() == 200;
		if (Result->bOk) Result->Body = Response->GetContentAsString();
		Result->bDone = true;
	});
	if (!Request->ProcessRequest()) Download->bDone = true;
}

TArray<FColor> FTCMapPreview::Raster(int32 Side) const
{
	TArray<FColor> Image;
	if (Side < 1 || Side > 1024) return Image;
	Image.Init(Background, Side * Side);
	if (Status != ETCPreviewStatus::Ready || W < 1 || H < 1 || Pixels.Num() != W * H) return Image;
	const FLayout L = Layout(Side, Side);
	for (int32 Y = 0; Y < Side; Y++) for (int32 X = 0; X < Side; X++)
	{
		const int32 Cx = FMath::FloorToInt((X + 0.5 - L.OffsetX) / L.Scale), Cy = FMath::FloorToInt((Y + 0.5 - L.OffsetY) / L.Scale);
		if (Cx >= 0 && Cx < W && Cy >= 0 && Cy < H) Image[Y * Side + X] = Pixels[Cy * W + Cx];
	}
	auto Circle = [&](FVector2D At, double R, const FColor& Color, bool bFill)
	{
		const int32 X0 = FMath::Max(0, FMath::FloorToInt(At.X - R - 2)), X1 = FMath::Min(Side - 1, FMath::CeilToInt(At.X + R + 2));
		const int32 Y0 = FMath::Max(0, FMath::FloorToInt(At.Y - R - 2)), Y1 = FMath::Min(Side - 1, FMath::CeilToInt(At.Y + R + 2));
		for (int32 Y = Y0; Y <= Y1; Y++) for (int32 X = X0; X <= X1; X++)
		{
			// One-pixel coverage at each arc edge. The browser uses a 2 px centred stroke.
			const double D = FVector2D::Distance(At, FVector2D(X + 0.5, Y + 0.5));
			const double A = FMath::Clamp(bFill ? R + 0.5 - D : 1.5 - FMath::Abs(D - R), 0.0, 1.0);
			FColor& C = Image[Y * Side + X];
			C = FColor(FMath::RoundToInt(C.R + (Color.R - C.R) * A), FMath::RoundToInt(C.G + (Color.G - C.G) * A), FMath::RoundToInt(C.B + (Color.B - C.B) * A), 255);
		}
	};
	for (const auto& P : Points) Circle(CellCentre(P.X, P.Y, L), L.PointRadius, PointColor, false);
	for (const auto& S : Spawns)
	{
		const FVector2D At = CellCentre(S.X, S.Y, L);
		Circle(At, L.SpawnRadius, S.Color, true); Circle(At, L.SpawnRadius, SpawnOutline, false);
	}
	return Image;
}

namespace TCMOptPreview
{
	FTCPaint Info(const FTCMapPreview& Preview, FVector2D At, float Width, const FTCMeasure& Measure)
	{
		FTCPaint Paint;
		float Y = At.Y;
		for (const auto& Line : Preview.Info)
		{
			if (Line.Dots.IsEmpty())
			{
				for (const FString& Text : FTCPaint::Wrap(TC_T(Line.Text), Width, 13, Line.bBold, Measure))
				{ Paint.Text(Text, At.X, Y, 13, FLinearColor(FColor(0xe2,0xdf,0xd3)), Line.bBold); Y += 17; }
				continue;
			}
			// main.js mapInfo translates each HTML text node, separately from its red/blue span.
			TArray<FString> Parts; Line.Text.ParseIntoArray(Parts, TEXT("●"), true);
			float X = At.X;
			for (int32 I = 0; I < Parts.Num(); I++)
			{
				if (X + 10 > At.X + Width) { X = At.X; Y += 17; }
				const FLinearColor Colour(Line.Dots.IsValidIndex(I) ? Line.Dots[I] : FColor::White);
				// Draw the source coloured dot without depending on a missing Barlow glyph.
				for (int32 Row = -4; Row <= 4; Row++)
				{
					const float Half = FMath::Sqrt(FMath::Max(0.f,16.f - Row * Row));
					Paint.Line({X + 4 - Half,Y + 7 + Row},{X + 4 + Half,Y + 7 + Row},Colour,1,TEXT("preview:dot"));
				}
				X += 12;
				TArray<FString> Words; TC_T(Parts[I].TrimStartAndEnd()).ParseIntoArray(Words,TEXT(" "),true);
				for (const FString& Word : Words)
				{
					const float W = Measure(Word,13,false,0);
					if (X > At.X && X + W > At.X + Width) { X = At.X; Y += 17; }
					Paint.Text(Word,X,Y,13,FLinearColor(FColor(0xe2,0xdf,0xd3)),false,ETCPaintAlign::Left,TEXT("preview:legend"));
					X += W + Measure(TEXT(" "),13,false,0);
				}
			}
			Y += 17;
		}
		return Paint;
	}
}

void FTCMatchOptions::DrawPreview(UCanvas* Canvas, UFont* Font, FVector2D At, float Side)
{
	if (!Canvas || !Canvas->Canvas || !Font || !InLobby() || !FApp::CanEverRender()) return;
	const FTCMapPreview P = Preview();
	const FString Key = P.Key + TEXT(":") + P.Mode + FString::Printf(TEXT(":%d:%d:%d"), P.W, P.H, (int32)P.Status);
	if (!PreviewImage || Key != PreviewImageKey)
	{
		PreviewImage.Reset(); PreviewImageKey = Key;
		const TArray<FColor> Image = P.Raster();
		auto* T = UTexture2D::CreateTransient(280, 280, PF_B8G8R8A8);
		if (!T || Image.Num() != 280 * 280) return;
		FTexture2DMipMap& Mip = T->GetPlatformData()->Mips[0];
		void* Bytes = Mip.BulkData.Lock(LOCK_READ_WRITE);
		FMemory::Memcpy(Bytes, Image.GetData(), Image.Num() * sizeof(FColor)); Mip.BulkData.Unlock();
		T->SRGB = true; T->Filter = TF_Nearest; T->NeverStream = true; T->UpdateResource(); PreviewImage.Reset(T);
		UE_LOG(LogTC, Log, TEXT("lobby preview: %s, %dx%d cells, 280x280 canvas, %d points, %d spawns"), *P.Key, P.W, P.H, P.Points.Num(), P.Spawns.Num());
	}
	FCanvasTileItem Tile(At, PreviewImage->GetResource(), FVector2D(Side, Side), FLinearColor::White);
	Tile.BlendMode = SE_BLEND_Opaque; Canvas->DrawItem(Tile);
	FTCPaint Words;
	if (P.Status == ETCPreviewStatus::World)
	{
		Words.Text(TC_T(P.Title), At.X + Side / 2, At.Y + Side / 2 - 10, 18 * Side / 280, FLinearColor(P.TitleColor), false, ETCPaintAlign::Centre);
		Words.Text(TC_T(P.Subtitle), At.X + Side / 2, At.Y + Side / 2 + 16 * Side / 280, 13 * Side / 280, FLinearColor(P.TitleColor), false, ETCPaintAlign::Centre);
	}
	Words.Append(TCMOptPreview::Info(P,{At.X,At.Y + Side + 8},Side + 100,FTCPaint::CanvasMeasure(Font)));
	FTCType::Draw(Words, Canvas, Font);
}

bool FTCMatchOptions::ApplyLobby(const FJsonObject& M)
{
	FString T, NewState;
	if (!TCMOptL::Str(M, TEXT("t"), T) || T != TEXT("lobby") || !TCMOptL::Str(M, TEXT("state"), NewState)) return false;
	const TArray<TSharedPtr<FJsonValue>>* Players = TCMOptL::Array(M, TEXT("players"));
	if (!Players) return false;
	TArray<FTCOptionSeat> NewSeats;
	for (const TSharedPtr<FJsonValue>& V : *Players)
	{
		if (!V.IsValid() || V->Type != EJson::Object) return false;
		const FJsonObject& P = *V->AsObject();
		FTCOptionSeat Seat;
		TCMOptL::Str(P, TEXT("name"), Seat.Name);
		Seat.bAi = TCMOptL::Flag(P, TEXT("ai"));
		Seat.bConnected = TCMOptL::Flag(P, TEXT("connected"));
		NewSeats.Add(MoveTemp(Seat));
	}

	bHasLobby = true;
	bRustHost = FTCConnection::HostKindOf(M) == ETCHostKind::Rust;
	State = NewState;
	SeatList = MoveTemp(NewSeats);
	if (!TCMOptL::Str(M, TEXT("role"), Role)) Role = TEXT("commander");
	if (!TCMOptL::Str(M, TEXT("mode"), Mode)) Mode.Reset();
	if (!TCMOptL::Str(M, TEXT("mapName"), MapName)) MapName.Reset();
	if (!TCMOptL::Str(M, TEXT("worldSize"), WorldSize)) WorldSize.Reset();
	if (!TCMOptL::Str(M, TEXT("weather"), Weather)) Weather.Reset();
	bSandbox = TCMOptL::Flag(M, TEXT("sandbox"));
	bAmHost = TCMOptL::Flag(M, TEXT("amHost"));
	bSpectator = TCMOptL::Flag(M, TEXT("spectator"));
	if (!TCMOptL::Int(M, TEXT("you"), -1, MaxPlayers - 1, You)) You = -1;

	// A match took the room: whatever was asked is moot. Otherwise a request is done once the lobby shows it.
	if (State != TEXT("lobby")) { Pending.Reset(); return true; }
	for (int32 I = Pending.Num() - 1; I >= 0; I--)
	{
		const FPending& P = Pending[I];
		bool bShown = false;
		switch (P.Kind)
		{
		case ETCMatchOption::Weather: bShown = WeatherChoice() == P.Value; break;
		case ETCMatchOption::Sandbox: bShown = (bSandbox ? TEXT("true") : TEXT("false")) == P.Value; break;
		case ETCMatchOption::Kick:
			bShown = CountSeats(P.SeatName, P.bSeatAi) < P.SameSeats;
			// the seat came back online (or left by itself without a lobby count change): nothing left to remove
			if (!bShown && !AnyKickable(P.SeatName, P.bSeatAi)) { Pending.RemoveAt(I); continue; }
			break;
		}
		if (bShown) { ConfirmedCount++; Pending.RemoveAt(I); }
	}
	return true;
}

FString FTCMatchOptions::WeatherChoice() const
{
	// weather-view.js lobby(): WEATHER_CHOICES.includes(m.weather) ? m.weather : 'map'
	return WeatherChoices().Contains(Weather) ? Weather : FString(TEXT("map"));
}

int32 FTCMatchOptions::CountSeats(const FString& Name, bool bAi) const
{
	int32 N = 0;
	for (const FTCOptionSeat& S : SeatList) N += S.Name == Name && S.bAi == bAi;
	return N;
}

bool FTCMatchOptions::AnyKickable(const FString& Name, bool bAi) const
{
	for (const FTCOptionSeat& S : SeatList) if (S.Name == Name && S.bAi == bAi && (S.bAi || !S.bConnected)) return true;
	return false;
}

const FTCMatchOptions::FPending* FTCMatchOptions::FindPending(ETCMatchOption Kind) const
{
	for (const FPending& P : Pending) if (P.Kind == Kind) return &P;
	return nullptr;
}

FString FTCMatchOptions::RoomLock() const
{
	// server.js: an operative's messages return before these handlers; weather and sandbox need room.state !== 'play',
	// kick needs 'lobby' (the only other state), and the browser enables all three only in the lobby
	if (!bHasLobby) return TEXT("no lobby yet");
	if (Role == TEXT("operative")) return TEXT("an operative changes no lobby settings");
	if (State != TEXT("lobby")) return TEXT("a match is running: options change in the lobby");
	return FString();
}

const FTCWeatherPlan* FTCMatchOptions::CurrentMapPlan() const
{
	if (!bHasLobby || Mode == TEXT("world")) return nullptr;
	const FTCMapPreviewSource* Source = MapFile(MapName);
	return Source ? &Source->Weather : nullptr;
}

FTCOptionControl FTCMatchOptions::Control(ETCMatchOption Kind, int32 Slot) const
{
	FTCOptionControl C;
	C.Kind = Kind;
	C.Slot = Kind == ETCMatchOption::Kick ? Slot : -1;
	const FString Lock = RoomLock();
	switch (Kind)
	{
	case ETCMatchOption::Weather:
	{
		C.Title = TEXT("Weather");
		if (!bHasLobby) { C.Why = Lock; return C; }
		C.bShown = true; // client/index.html: the Weather row is never hidden
		const FTCWeatherPlan* Plan = CurrentMapPlan();
		C.CurrentValue = WeatherChoice();
		for (int32 I = 0; I < UE_ARRAY_COUNT(TCMOptL::Choices); I++)
		{
			FTCLobbyChoice Choice;
			Choice.Value = TCMOptL::Choices[I];
			Choice.Label = I == 0 ? MapDefaultLabel(Plan) : FString(TCMOptL::ChoiceLabels[I]);
			Choice.bCurrent = Choice.Value == C.CurrentValue;
			C.Choices.Add(MoveTemp(Choice));
		}
		if (const FTCLobbyChoice* Shown = C.Find(C.CurrentValue)) C.CurrentLabel = Shown->Label;
		if (const FPending* P = FindPending(Kind)) { C.bPending = true; C.PendingValue = P->Value; C.PendingSentAt = P->SentAt; }
		C.Tooltip = WeatherExplain(C.bPending ? C.PendingValue : C.CurrentValue, Plan);
		C.bEditable = Lock.IsEmpty() && bAmHost;
		C.Why = !Lock.IsEmpty() ? Lock : bAmHost ? FString() : FString(TEXT("only the host sets the weather"));
		return C;
	}
	case ETCMatchOption::Sandbox:
	{
		C.Title = TEXT("Sandbox");
		C.Tooltip = TEXT("A test match: everyone can spawn any unit for either side, and Horde records do not count");
		if (!bHasLobby) { C.Why = Lock; return C; }
		C.bShown = true; // client/index.html: the Sandbox row is never hidden
		C.bChecked = bSandbox;
		C.CurrentValue = bSandbox ? TEXT("true") : TEXT("false");
		C.CurrentLabel = bSandbox ? TEXT("On") : TEXT("Off");
		if (const FPending* P = FindPending(Kind)) { C.bPending = true; C.PendingValue = P->Value; C.PendingSentAt = P->SentAt; }
		C.bEditable = Lock.IsEmpty() && bAmHost;
		C.Why = !Lock.IsEmpty() ? Lock : bAmHost ? FString() : FString(TEXT("only the host switches Sandbox"));
		// Permit correcting a mismatched host lobby to off, but never offer enabling Sandbox under R1.
		if (bRustHost && !bSandbox) { C.bEditable = false; C.Why = TEXT("Rust host: Sandbox must be off."); }
		return C;
	}
	case ETCMatchOption::Kick:
	{
		C.Title = TEXT("Remove");
		if (!bHasLobby) { C.Why = Lock; return C; }
		if (!SeatList.IsValidIndex(Slot)) { C.Why = FString::Printf(TEXT("the room has no seat %d"), Slot + 1); return C; }
		const FTCOptionSeat& Seat = SeatList[Slot];
		C.SeatName = Seat.Name;
		C.bSeatAi = Seat.bAi;
		C.CurrentValue = FString::FromInt(Slot);
		C.CurrentLabel = Seat.Name;
		// client/main.js: title="Remove ${p.ai ? 'AI' : 'offline player'}"
		C.Tooltip = Seat.bAi ? TEXT("Remove AI") : TEXT("Remove offline player");
		for (const FPending& P : Pending)
			if (P.Kind == ETCMatchOption::Kick && P.Slot == Slot && P.SeatName == Seat.Name && P.bSeatAi == Seat.bAi) { C.bPending = true; C.PendingValue = P.Value; C.PendingSentAt = P.SentAt; }
		// the button: host && lobby && (p.ai || !p.connected); the server checks the same (and that the slot exists)
		if (!Lock.IsEmpty()) { C.Why = Lock; return C; }
		if (!bAmHost) { C.Why = TEXT("only the host removes seats"); return C; }
		if (!Seat.bAi && Seat.bConnected) { C.Why = TEXT("a connected player cannot be removed"); return C; }
		C.bShown = true;
		if (C.bPending) { C.Why = TEXT("removal asked, waiting for the lobby"); return C; }
		if (FindPending(Kind)) { C.Why = TEXT("waiting for the lobby after the last removal: seat numbers may move"); return C; }
		C.bEditable = true;
		return C;
	}
	}
	return C;
}

TArray<FTCOptionControl> FTCMatchOptions::KickControls() const
{
	TArray<FTCOptionControl> Out;
	for (int32 I = 0; I < SeatList.Num(); I++) Out.Add(Control(ETCMatchOption::Kick, I));
	return Out;
}

FTCOptionRequest FTCMatchOptions::PlanWeather(const FString& Choice) const
{
	FTCOptionRequest R;
	R.Kind = ETCMatchOption::Weather;
	R.Value = Choice;
	const FTCOptionControl C = Control(ETCMatchOption::Weather);
	if (!C.bShown || !C.bEditable) { R.Note = FString::Printf(TEXT("Weather: %s"), *C.Why); return R; }
	const FTCLobbyChoice* Pick = C.Find(Choice);
	if (!Pick) { R.Note = FString::Printf(TEXT("Weather: '%s' is not one of its choices"), *Choice); return R; }
	if (C.bPending && C.PendingValue == Choice) { R.Note = FString::Printf(TEXT("Weather: %s already asked, waiting for the lobby"), *Pick->Label); return R; }
	if (Pick->bCurrent && !C.bPending) { R.Note = FString::Printf(TEXT("Weather is already %s"), *Pick->Label); return R; }
	TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
	O->SetStringField(TEXT("t"), TEXT("weather")); // weather-view.js: sendCmd({ t: 'weather', v: sel.value })
	O->SetStringField(TEXT("v"), Choice);
	R.Message = O;
	R.Note = FString::Printf(TEXT("Weather %s asked (was %s)"), *Pick->Label, *C.CurrentLabel);
	return R;
}

FTCOptionRequest FTCMatchOptions::PlanWeatherStep(int32 Direction) const
{
	const FTCOptionControl C = Control(ETCMatchOption::Weather);
	if (!C.bShown || !C.bEditable || C.Choices.Num() == 0) return PlanWeather(C.CurrentValue);
	const FString Displayed = C.bPending ? C.PendingValue : C.CurrentValue;
	const int32 N = C.Choices.Num(), Step = Direction < 0 ? -1 : 1;
	int32 At = C.Choices.IndexOfByPredicate([&Displayed](const FTCLobbyChoice& Ch) { return Ch.Value == Displayed; });
	if (At == INDEX_NONE) At = Step > 0 ? -1 : 0;
	return PlanWeather(C.Choices[((At + Step) % N + N) % N].Value);
}

FTCOptionRequest FTCMatchOptions::PlanSandbox(bool bOn) const
{
	FTCOptionRequest R;
	R.Kind = ETCMatchOption::Sandbox;
	R.Value = bOn ? TEXT("true") : TEXT("false");
	if (bRustHost && bOn) { R.Note = TEXT("Rust host: Sandbox must be off."); return R; }
	const FTCOptionControl C = Control(ETCMatchOption::Sandbox);
	const TCHAR* Label = bOn ? TEXT("on") : TEXT("off");
	if (!C.bShown || !C.bEditable) { R.Note = FString::Printf(TEXT("Sandbox: %s"), *C.Why); return R; }
	if (C.bPending && C.PendingValue == R.Value) { R.Note = FString::Printf(TEXT("Sandbox %s already asked, waiting for the lobby"), Label); return R; }
	if (C.CurrentValue == R.Value && !C.bPending) { R.Note = FString::Printf(TEXT("Sandbox is already %s"), Label); return R; }
	TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
	O->SetStringField(TEXT("t"), TEXT("sandbox")); // client/main.js: sendCmd({ t: 'sandbox', v: $('sandboxChk').checked })
	O->SetBoolField(TEXT("v"), bOn);
	R.Message = O;
	R.Note = FString::Printf(TEXT("Sandbox %s asked"), Label);
	return R;
}

FTCOptionRequest FTCMatchOptions::PlanSandboxToggle() const
{
	const FTCOptionControl C = Control(ETCMatchOption::Sandbox);
	const FString Displayed = C.bPending ? C.PendingValue : C.CurrentValue;
	return PlanSandbox(Displayed != TEXT("true"));
}

FTCOptionRequest FTCMatchOptions::PlanKick(int32 Slot) const
{
	FTCOptionRequest R;
	R.Kind = ETCMatchOption::Kick;
	R.Slot = Slot;
	R.Value = FString::FromInt(Slot);
	const FTCOptionControl C = Control(ETCMatchOption::Kick, Slot);
	R.SeatName = C.SeatName;
	R.bSeatAi = C.bSeatAi;
	const FString What = SeatList.IsValidIndex(Slot) ? FString::Printf(TEXT("seat %d (%s)"), Slot + 1, *C.SeatName) : FString::Printf(TEXT("seat %d"), Slot + 1);
	if (!C.bEditable) { R.Note = FString::Printf(TEXT("Remove %s: %s"), *What, *C.Why); return R; }
	TSharedRef<FJsonObject> O = MakeShared<FJsonObject>();
	O->SetStringField(TEXT("t"), TEXT("kick")); // client/main.js: sendCmd({ t: 'kick', slot: +b.dataset.slot })
	O->SetNumberField(TEXT("slot"), Slot);
	R.Message = O;
	R.Note = FString::Printf(TEXT("Remove %s asked"), *What);
	return R;
}

void FTCMatchOptions::NoteSent(const FTCOptionRequest& Request, double Now)
{
	if (!Request.Message.IsValid()) return;
	FPending P;
	P.Kind = Request.Kind;
	P.Value = Request.Value;
	P.SentAt = Now;
	if (Request.Kind == ETCMatchOption::Kick)
	{
		P.Slot = Request.Slot;
		P.SeatName = Request.SeatName;
		P.bSeatAi = Request.bSeatAi;
		P.SameSeats = CountSeats(Request.SeatName, Request.bSeatAi);
		Pending.Add(MoveTemp(P));
		return;
	}
	Pending.RemoveAll([&Request](const FPending& Q) { return Q.Kind == Request.Kind; });
	const FString Shown = Request.Kind == ETCMatchOption::Weather ? WeatherChoice() : FString(bSandbox ? TEXT("true") : TEXT("false"));
	if (bHasLobby && Shown == Request.Value) return; // back to the shown value: nothing to wait for
	Pending.Add(MoveTemp(P));
}

// ---------- map files and the preview ----------

bool FTCMatchOptions::IsMapName(const FString& Key)
{
	if (Key.Len() < 1 || Key.Len() > 32) return false;
	for (const TCHAR C : Key) if (!((C >= 'a' && C <= 'z') || (C >= '0' && C <= '9') || C == '-')) return false;
	return true;
}

FString FTCMatchOptions::MapPath(const FString& Key)
{
	return IsMapName(Key) ? TEXT("/maps/") + Key : FString();
}

FString FTCMatchOptions::MapUrl(const FString& SocketUrl, const FString& Key)
{
	const FString Path = MapPath(Key);
	if (Path.IsEmpty()) return FString();
	FString Scheme, Rest;
	if (SocketUrl.StartsWith(TEXT("ws://"), ESearchCase::IgnoreCase)) { Scheme = TEXT("http://"); Rest = SocketUrl.Mid(5); }
	else if (SocketUrl.StartsWith(TEXT("wss://"), ESearchCase::IgnoreCase)) { Scheme = TEXT("https://"); Rest = SocketUrl.Mid(6); }
	else return FString();
	int32 End = Rest.Len();
	for (int32 I = 0; I < Rest.Len(); I++) if (Rest[I] == '/' || Rest[I] == '?' || Rest[I] == '#') { End = I; break; }
	if (End == 0) return FString();
	return Scheme + Rest.Left(End) + Path;
}

bool FTCMatchOptions::ParseMapFile(const FString& Key, const FJsonObject& M, FTCMapPreviewSource& Out, FString& OutError)
{
	// The checks are validateMap's (shared/sim.js), on the fields read here, in its order and with its words; what
	// lies under a spawn or point, the world layers and the scenario are the server's business, not the preview's.
	auto Fail = [&OutError](const FString& Why) { OutError = Why; return false; };
	if (!IsMapName(Key)) return Fail(FString::Printf(TEXT("'%s' is not a map name the server serves"), *Key));
	FTCMapPreviewSource S;
	S.Key = Key;
	if (!TCMOptL::Str(M, TEXT("name"), S.Name) || S.Name.Len() > 40) return Fail(TEXT("name must be a string up to 40 chars"));
	if (!TCMOptL::Int(M, TEXT("w"), 20, 1024, S.W) || !TCMOptL::Int(M, TEXT("h"), 20, 1024, S.H)) return Fail(TEXT("size must be 20-1024 cells"));
	const TArray<TSharedPtr<FJsonValue>>* Rows = TCMOptL::Array(M, TEXT("rows"));
	if (!Rows || Rows->Num() != S.H) return Fail(TEXT("row count must equal height"));
	S.Cells.SetNumUninitialized(S.W * S.H);
	S.Levels.SetNumZeroed(S.W * S.H);
	for (int32 Y = 0; Y < S.H; Y++)
	{
		const TSharedPtr<FJsonValue>& V = (*Rows)[Y];
		const FString Row = V.IsValid() && V->Type == EJson::String ? V->AsString() : FString();
		bool bOk = V.IsValid() && V->Type == EJson::String && Row.Len() == S.W;
		for (int32 X = 0; bOk && X < S.W; X++)
		{
			const TCHAR C = Row[X];
			bOk = C != 0 && FCString::Strchr(TCMOptL::RowChars, C) != nullptr;
			S.Cells[Y * S.W + X] = (uint8)C;
			S.bWater |= C == 'W';
		}
		if (!bOk) return Fail(FString::Printf(TEXT("rows must be %d valid terrain chars"), S.W));
	}
	if (!TCMOptL::Absent(M, TEXT("heights")))
	{
		const TArray<TSharedPtr<FJsonValue>>* Heights = TCMOptL::Array(M, TEXT("heights"));
		bool bOk = Heights && Heights->Num() == S.H;
		for (int32 Y = 0; bOk && Y < S.H; Y++)
		{
			const TSharedPtr<FJsonValue>& V = (*Heights)[Y];
			bOk = V.IsValid() && V->Type == EJson::String && V->AsString().Len() == S.W;
			const FString Row = bOk ? V->AsString() : FString();
			for (int32 X = 0; bOk && X < S.W; X++)
			{
				const TCHAR C = Row[X];
				bOk = (C >= '0' && C <= '4') || C == 'a' || C == 'b';
				if (!bOk) break;
				S.Levels[Y * S.W + X] = TCMOptL::LevelOf(C);
				S.TopLevel = FMath::Max<int32>(S.TopLevel, TCMOptL::LevelOf(C));
			}
		}
		if (!bOk) return Fail(FString::Printf(TEXT("heights must be %d rows of 0-4 (a/b for depths -1/-2)"), S.H));
	}
	const bool bWorldMap = TCMOptL::Truthy(M.TryGetField(TEXT("world")));
	const int32 MinSpawns = bWorldMap ? 1 : 2;
	const TArray<TSharedPtr<FJsonValue>>* Spawns = TCMOptL::Array(M, TEXT("spawns"));
	if (!Spawns || Spawns->Num() < MinSpawns || Spawns->Num() > MaxPlayers) return Fail(FString::Printf(TEXT("needs 2-%d spawns"), MaxPlayers));
	for (const TSharedPtr<FJsonValue>& V : *Spawns)
	{
		FTCMapPreviewSource::FSpawn Sp;
		if (!V.IsValid() || V->Type != EJson::Object || !TCMOptL::Int(*V->AsObject(), TEXT("x"), 0, S.W - 1, Sp.X) || !TCMOptL::Int(*V->AsObject(), TEXT("y"), 0, S.H - 1, Sp.Y))
			return Fail(TEXT("spawns must be on open ground inside the map"));
		S.Spawns.Add(Sp);
	}
	for (int32 I = 0; I < Spawns->Num(); I++)
	{
		const FJsonObject& O = *(*Spawns)[I]->AsObject();
		if (TCMOptL::Absent(O, TEXT("assault"))) continue;
		const TSharedPtr<FJsonValue> A = O.TryGetField(TEXT("assault"));
		if (A->Type != EJson::Boolean) return Fail(TEXT("spawn assault flag must be true or false"));
		S.Spawns[I].bAssault = A->AsBool();
	}
	int32 EveryMode = 0;
	for (const FTCMapPreviewSource::FSpawn& Sp : S.Spawns) EveryMode += !Sp.bAssault;
	if (EveryMode < MinSpawns) return Fail(TEXT("needs 2+ spawns that every mode can use"));
	if (!TCMOptL::Absent(M, TEXT("assaultTime")) && !TCMOptL::Int(M, TEXT("assaultTime"), 300, 3600, S.AssaultTime)) return Fail(TEXT("assaultTime must be 300-3600 seconds"));
	if (!TCMOptL::Absent(M, TEXT("defend")))
	{
		const TArray<TSharedPtr<FJsonValue>>* Defend = TCMOptL::Array(M, TEXT("defend"));
		bool bOk = Defend && Defend->Num() > 0 && Defend->Num() < S.Spawns.Num();
		for (int32 I = 0; bOk && I < Defend->Num(); I++)
		{
			int32 Spawn = 0;
			bOk = TCMOptL::Int((*Defend)[I], 0, S.Spawns.Num() - 1, Spawn) && !S.Defend.Contains(Spawn);
			if (bOk) S.Defend.Add(Spawn);
		}
		if (!bOk) return Fail(TEXT("defend must list some (not all) spawn numbers"));
		S.bHasDefend = true;
	}
	const TArray<TSharedPtr<FJsonValue>>* Points = TCMOptL::Array(M, TEXT("points"));
	if (!Points || (!bWorldMap && (Points->Num() < 1 || Points->Num() > 9)) || (bWorldMap && Points->Num() != 0)) return Fail(TEXT("needs 1-9 capture points"));
	for (const TSharedPtr<FJsonValue>& V : *Points)
	{
		FTCMapPreviewSource::FPoint P;
		double Vp = 1.0;
		if (!V.IsValid() || V->Type != EJson::Object || !TCMOptL::Int(*V->AsObject(), TEXT("x"), 0, S.W - 1, P.X) || !TCMOptL::Int(*V->AsObject(), TEXT("y"), 0, S.H - 1, P.Y)
			|| !TCMOptL::NumOrOne(*V->AsObject(), TEXT("vp"), 0.0, 5.0, Vp) || !TCMOptL::NumOrOne(*V->AsObject(), TEXT("mp"), 0.0, 5.0, P.Mp))
			return Fail(TEXT("points must be on open ground, vp and mp 0-5"));
		S.Points.Add(P);
	}
	FString Mood, WeatherField;
	TCMOptL::Str(M, TEXT("mood"), Mood);
	TCMOptL::Str(M, TEXT("weather"), WeatherField);
	S.Weather = MapWeather(S.Name, Key, Mood, WeatherField);
	Out = MoveTemp(S);
	OutError.Reset();
	return true;
}

bool FTCMatchOptions::AddMapFile(const FString& Key, const FJsonObject& Map, FString* OutError)
{
	FTCMapPreviewSource Source;
	FString Error;
	if (!ParseMapFile(Key, Map, Source, Error)) { if (OutError) *OutError = Error; return false; }
	MapFiles.Add(Key, TSharedRef<const FTCMapPreviewSource>(MakeShared<FTCMapPreviewSource>(MoveTemp(Source))));
	if (OutError) OutError->Reset();
	return true;
}

bool FTCMatchOptions::AddMapFileText(const FString& Key, const FString& Body, FString* OutError)
{
	TSharedPtr<FJsonObject> Obj;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Body), Obj) || !Obj.IsValid())
	{
		if (OutError) *OutError = TEXT("not a map");
		return false;
	}
	return AddMapFile(Key, *Obj, OutError);
}

const FTCMapPreviewSource* FTCMatchOptions::MapFile(const FString& Key) const
{
	const TSharedRef<const FTCMapPreviewSource>* Found = MapFiles.Find(Key);
	return Found ? &Found->Get() : nullptr;
}

FString FTCMatchOptions::WantedMap() const
{
	if (!bHasLobby || Mode == TEXT("world") || !IsMapName(MapName) || MapFiles.Contains(MapName)) return FString();
	return MapName;
}

FTCMapPreview FTCMatchOptions::Preview() const
{
	if (!bHasLobby) return FTCMapPreview();
	const FString Drawn = Mode.IsEmpty() ? FString(TEXT("conquest")) : Mode; // previewMap(m.mapName, m.mode || 'conquest')
	if (Drawn == TEXT("world")) return BuildPreview(nullptr, MapName, Drawn, WorldSize);
	if (MapName.IsEmpty()) return FTCMapPreview();
	return BuildPreview(MapFile(MapName), MapName, Drawn, WorldSize);
}

FTCMapPreview FTCMatchOptions::BuildPreview(const FTCMapPreviewSource* Source, const FString& Key, const FString& InMode, const FString& InWorldSize)
{
	FTCMapPreview P;
	P.Key = Key;
	P.Mode = InMode.IsEmpty() ? FString(TEXT("conquest")) : InMode;
	if (P.Mode == TEXT("world"))
	{
		// previewMap's World branch: no file, a placeholder and one line
		P.Status = ETCPreviewStatus::World;
		P.Background = FColor(0x10, 0x14, 0x12);
		P.TitleColor = FColor(0xd6, 0xb2, 0x5e);
		P.Title = TEXT("Uncharted continent");
		P.Subtitle = TEXT("Explore to reveal terrain");
		const int32 Total = InWorldSize == TEXT("massive") ? 128 : 64;
		const TCHAR* Metres = Total == 128 ? TEXT("2,048 × 1,024") : TEXT("1,024 × 1,024");
		FTCMapPreviewLine WorldLine;
		WorldLine.Text = FString::Printf(TEXT("%s metres. %d regions. Fresh geography at match start. Shared discoveries within your team."), Metres, Total);
		P.Info.Add(MoveTemp(WorldLine));
		return P;
	}
	if (!Source) { P.Status = ETCPreviewStatus::NeedsFile; return P; }

	const FTCMapPreviewSource& M = *Source;
	P.Status = ETCPreviewStatus::Ready;
	P.Background = FColor(0x11, 0x11, 0x0d);
	P.PointColor = FColor(0xf0, 0xd9, 0x8a);
	P.SpawnOutline = FColor(0x11, 0x11, 0x11);
	P.W = M.W;
	P.H = M.H;
	// [r, g, b] = MM_COLORS[ch] ?? MM_COLORS['.'], k = 1 + levelOf(height) * 0.12, each channel r * k clamped
	P.Pixels.SetNumUninitialized(M.W * M.H);
	for (int32 I = 0; I < M.W * M.H; I++)
	{
		const TCMOptL::FCellColor* Color = &TCMOptL::CellColors[0];
		for (const TCMOptL::FCellColor& Cc : TCMOptL::CellColors) if (Cc.Ch == (TCHAR)M.Cells[I]) { Color = &Cc; break; }
		const double Lift = (double)M.Levels[I] * 0.12;
		const double K = 1.0 + Lift;
		P.Pixels[I] = FColor(TCMOptL::Clamp8((double)Color->R * K), TCMOptL::Clamp8((double)Color->G * K), TCMOptL::Clamp8((double)Color->B * K), 255);
	}

	const FString& Md = P.Mode;
	const bool bHorde = Md == TEXT("horde"), bAssault = Md == TEXT("assault") || bHorde, bNoVp = bAssault || Md == TEXT("annihilation");
	// points: in Assault, Horde and Annihilation only those that give MP
	for (int32 I = 0; I < M.Points.Num(); I++)
		if (!bNoVp || M.Points[I].Mp > 0.0) P.Points.Add({ I, M.Points[I].X, M.Points[I].Y });
	// spawns: Assault-only spawns only in Assault; red defend, blue attack when the map lists defenders
	for (int32 I = 0; I < M.Spawns.Num(); I++)
	{
		if (Md != TEXT("assault") && M.Spawns[I].bAssault) continue;
		FTCMapPreviewSpawn Sp;
		Sp.Index = I;
		Sp.X = M.Spawns[I].X;
		Sp.Y = M.Spawns[I].Y;
		Sp.Role = bAssault && M.bHasDefend ? (M.Defend.Contains(I) ? ETCPreviewSpawnRole::Defend : ETCPreviewSpawnRole::Attack) : ETCPreviewSpawnRole::Neutral;
		Sp.Color = Sp.Role == ETCPreviewSpawnRole::Defend ? TCMOptL::Red : Sp.Role == ETCPreviewSpawnRole::Attack ? TCMOptL::Blue : TCMOptL::Chalk;
		P.Spawns.Add(Sp);
	}

	auto AddLine = [&P](const FString& Words, bool bBold = false, TArray<FColor> Dots = {})
	{
		FTCMapPreviewLine InfoLine;
		InfoLine.Text = Words;
		InfoLine.bBold = bBold;
		InfoLine.Dots = MoveTemp(Dots);
		P.Info.Add(MoveTemp(InfoLine));
	};
	AddLine(M.Name, true);
	AddLine(FString::Printf(TEXT("%d × %d m"), M.W * CellMetres, M.H * CellMetres));
	AddLine(bHorde ? FString::Printf(TEXT("up to %d defenders"), MaxPlayers - 1) : FString::Printf(TEXT("up to %d players"), P.Spawns.Num()));
	AddLine(FString::Printf(TEXT("%d capture point%s"), P.Points.Num(), P.Points.Num() == 1 ? TEXT("") : TEXT("s")));
	if (M.TopLevel >= 3) AddLine(TEXT("hills and cliffs"));
	else if (M.TopLevel > 0) AddLine(TEXT("rolling hills"));
	if (M.bWater) AddLine(TEXT("rivers"));
	if (M.bHasDefend)
	{
		if (bHorde) AddLine(FString::Printf(TEXT("%s your HQ is one of these, %s the horde comes from here"), TCMOptL::Dot, TCMOptL::Dot), false, { TCMOptL::Red, TCMOptL::Blue });
		else if (bAssault) AddLine(FString::Printf(TEXT("%s defend, %s attack"), TCMOptL::Dot, TCMOptL::Dot), false, { TCMOptL::Red, TCMOptL::Blue });
		else AddLine(TEXT("built for Assault"));
	}
	if (Md == TEXT("assault") && M.AssaultTime > 0) AddLine(FString::Printf(TEXT("%d minute clock"), (int32)TCMOptL::JsRound((double)M.AssaultTime / 60.0)));
	return P;
}

FTCMapPreview::FLayout FTCMapPreview::Layout(double CanvasWidth, double CanvasHeight) const
{
	FLayout L;
	if (W <= 0 || H <= 0) return L;
	// s = cv.width / Math.max(m.w, m.h), ox = (cv.width - m.w * s) / 2, oy = (cv.height - m.h * s) / 2
	L.Scale = CanvasWidth / (double)FMath::Max(W, H);
	const double DrawnW = (double)W * L.Scale;
	const double DrawnH = (double)H * L.Scale;
	L.OffsetX = (CanvasWidth - DrawnW) / 2.0;
	L.OffsetY = (CanvasHeight - DrawnH) / 2.0;
	// Math.max(4, CFG.pointRadius / CELL * s)
	const double RingCells = (double)FTCMatchOptions::PointRadiusMetres / (double)FTCMatchOptions::CellMetres;
	const double Ring = RingCells * L.Scale;
	L.PointRadius = FMath::Max(4.0, Ring);
	return L;
}

FVector2D FTCMapPreview::CellCentre(int32 X, int32 Y, const FLayout& At)
{
	// at = (p) => [ox + (p.x + 0.5) * s, oy + (p.y + 0.5) * s]
	const double Cx = ((double)X + 0.5) * At.Scale;
	const double Cy = ((double)Y + 0.5) * At.Scale;
	return FVector2D(At.OffsetX + Cx, At.OffsetY + Cy);
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Serialization/JsonWriter.h"
#include "Policies/CondensedJsonPrintPolicy.h"

namespace TCMOptT
{
	struct FExpectedPreview
	{
		const TCHAR* Map;
		const TCHAR* Mode;
		uint32 Fnv;
		double Scale, OffsetX, OffsetY, Ring;
		const TCHAR* Info[9];
		const TCHAR* Spawns;
		const TCHAR* Points;
	};
	struct FExpectedMapWeather
	{
		const TCHAR* Name;
		const TCHAR* Key;
		const TCHAR* Mood;
		const TCHAR* Weather;
		const TCHAR* Now;
		const TCHAR* Next;
		int32 At;
		const TCHAR* Label;
		const TCHAR* Tooltip;
	};

	// Lobby messages exactly as an isolated copy of the pinned reference server (e4e579a) sent them (only publicUrl
	// removed). The same run (probe.mjs, all 39 cases as expected) showed what the server does with each message this
	// module builds. It took: weather from the host for every choice, sandbox true/false from the host (and answered a
	// repeat with an unchanged lobby), kick of an AI seat and of an offline human's seat from the host, and all three
	// from a spectator hosting a room of AI seats. It dropped without an answer: weather and sandbox from a guest or a
	// spectator who is not host, weather 'storm' and 'Fog', kick of a connected human (the host's own seat too), kick
	// from a guest or spectator, kick of slot 9, '2' and 2.5, and weather, sandbox and kick during play.
	// HostFog, GuestFog, WatcherFog: one room (Host, Guest, AI 1, AI 2) after the host set Ground fog and Sandbox.
	// HostGuestOffline: after AI 1 was removed and the guest's socket closed. WatcherHostsAi, FormerHostWatching: the
	// host took 'spectate', leaving AI 2 seated, so the first spectator (Watcher) hosts. WatcherHostsPlay: a match
	// that spectator started.
	const TCHAR* const HostFog = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":true,"spectators":["Watcher"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"mo3510","state":"lobby","you":0,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"huge","defenderTeam":0,"army":"standard","weather":"fog","sandbox":true,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":1,"faction":1,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":2,"level":"normal"},{"name":"AI 2","connected":true,"ai":true,"team":3,"faction":0,"level":"normal"}]})json");
	const TCHAR* const GuestFog = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":false,"spectators":["Watcher"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"mo3510","state":"lobby","you":1,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"huge","defenderTeam":0,"army":"standard","weather":"fog","sandbox":true,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":1,"faction":1,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":2,"level":"normal"},{"name":"AI 2","connected":true,"ai":true,"team":3,"faction":0,"level":"normal"}]})json");
	const TCHAR* const WatcherFog = TEXT(R"json({"role":"commander","operatives":[],"spectator":true,"amHost":false,"spectators":["Watcher"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"mo3510","state":"lobby","you":-1,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"huge","defenderTeam":0,"army":"standard","weather":"fog","sandbox":true,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":true,"ai":false,"team":1,"faction":1,"level":null},{"name":"AI 1","connected":true,"ai":true,"team":2,"faction":2,"level":"normal"},{"name":"AI 2","connected":true,"ai":true,"team":3,"faction":0,"level":"normal"}]})json");
	const TCHAR* const HostGuestOffline = TEXT(R"json({"role":"commander","operatives":[],"spectator":false,"amHost":true,"spectators":["Watcher"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"mo3510","state":"lobby","you":0,"host":0,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"huge","defenderTeam":0,"army":"standard","weather":"fog","sandbox":true,"result":null,"players":[{"name":"Host","connected":true,"ai":false,"team":0,"faction":0,"level":null},{"name":"Guest","connected":false,"ai":false,"team":1,"faction":1,"level":null},{"name":"AI 2","connected":true,"ai":true,"team":3,"faction":0,"level":"normal"}]})json");
	const TCHAR* const WatcherHostsAi = TEXT(R"json({"role":"commander","operatives":[],"spectator":true,"amHost":true,"spectators":["Watcher","Host"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"mo3510","state":"lobby","you":-1,"host":-1,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"huge","defenderTeam":0,"army":"standard","weather":"fog","sandbox":true,"result":null,"players":[{"name":"AI 2","connected":true,"ai":true,"team":3,"faction":0,"level":"normal"}]})json");
	const TCHAR* const FormerHostWatching = TEXT(R"json({"role":"commander","operatives":[],"spectator":true,"amHost":false,"spectators":["Watcher","Host"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"mo3510","state":"lobby","you":-1,"host":-1,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"huge","defenderTeam":0,"army":"standard","weather":"fog","sandbox":true,"result":null,"players":[{"name":"AI 2","connected":true,"ai":true,"team":3,"faction":0,"level":"normal"}]})json");
	const TCHAR* const WatcherHostsPlay = TEXT(R"json({"role":"commander","operatives":[],"spectator":true,"amHost":true,"spectators":["Watcher","Host"],"listed":false,"saves":[],"hordeBest":null,"t":"lobby","code":"mo3510","state":"play","you":-1,"host":-1,"maps":["ardennes-crossing","bastogne","bocage","crater-field","crossroads-village","default","el-alamein","helms-deep","hill-112","hill-112-xl","hot-gates","hurtgen-forest","island-towns","kasserine-pass","king-of-the-hill","monte-cassino","monte-cassino-xl","no-mans-land","pegasus-bridge","pegasus-bridge-xl","prokhorovka","river-towns","seawall","seawall-xl","seelow-heights","six-fronts","stalingrad-factory","test-arena","the-great-bridge","the-polder","three-islands","tutorial","twin-valleys"],"mapName":"default","spawns":3,"mode":"conquest","worldSize":"huge","defenderTeam":0,"army":"standard","weather":"snow","sandbox":true,"result":null,"players":[{"name":"AI 1","connected":true,"ai":true,"team":0,"faction":0,"level":"normal"}]})json");
	const TCHAR* const SynMap = TEXT(R"json({"name":"Test Ford <&'>","w":24,"h":20,"rows":[".....H..................",".....H..................",".....H..................",".....H..................","........................","........................","........................","........................","........................","........................","...WWWWWWWWWWWWWWWWW....","........................","........................","........................","........................","DDDDDDDDDDDDDDDDDDDDDDDD","........................","........................","........................","........................"],"heights":["000000000000000000044444","000000000000000000044444","000000000000000000044444","000000000000000000044444","000000000000000000044444","000000000000000000022222","000000000000000000022222","000000000000000000022222","000000000000000000022222","000000000000000000022222","bbbbbbbbbbbbbbbbbbbbbbbb","aaaaaaaaaaaaaaaaaaaaaaaa","000000000000000000022222","000000000000000000022222","000000000000000000022222","000000000000000000022222","000000000000000000022222","000000000000000000022222","000000000000000000022222","000000000000000000022222"],"spawns":[{"x":2,"y":2},{"x":21,"y":17},{"x":2,"y":17},{"x":12,"y":3,"assault":true}],"defend":[1],"assaultTime":930,"points":[{"x":12,"y":5},{"x":12,"y":14,"mp":0},{"x":6,"y":6,"vp":2}]})json");
	// validateMap (shared/sim.js) on broken copies of SynMap, run in node: the change, then its answer
	const TCHAR* const SynBroken[][2] = {
		{ TEXT("{\"name\":\"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("name must be a string up to 40 chars") }, // long name
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":19,\"h\":20,\"rows\":[\"....H..................\",\"....H..................\",\"....H..................\",\"....H..................\",\".......................\",\".......................\",\".......................\",\".......................\",\".......................\",\".......................\",\"..WWWWWWWWWWWWWWWWW....\",\".......................\",\".......................\",\".......................\",\".......................\",\"DDDDDDDDDDDDDDDDDDDDDDD\",\".......................\",\".......................\",\".......................\",\".......................\"],\"heights\":[\"00000000000000000044444\",\"00000000000000000044444\",\"00000000000000000044444\",\"00000000000000000044444\",\"00000000000000000044444\",\"00000000000000000022222\",\"00000000000000000022222\",\"00000000000000000022222\",\"00000000000000000022222\",\"00000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaa\",\"00000000000000000022222\",\"00000000000000000022222\",\"00000000000000000022222\",\"00000000000000000022222\",\"00000000000000000022222\",\"00000000000000000022222\",\"00000000000000000022222\",\"00000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("size must be 20-1024 cells") }, // w 19
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\"K....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("rows must be 24 valid terrain chars") }, // K in a row
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"c00000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("heights must be 20 rows of 0-4 (a/b for depths -1/-2)") }, // height c
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":null,\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("heights must be 20 rows of 0-4 (a/b for depths -1/-2)") }, // heights null
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2}],\"defend\":[1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("needs 2-6 spawns") }, // one spawn
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":1}],\"defend\":[1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("spawn assault flag must be true or false") }, // assault flag 1
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1],\"assaultTime\":900.5,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("assaultTime must be 300-3600 seconds") }, // assaultTime 900.5
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[0,1,2,3],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("defend must list some (not all) spawn numbers") }, // defend every spawn
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1,1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("defend must list some (not all) spawn numbers") }, // defend twice
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1],\"assaultTime\":930,\"points\":[]}"), TEXT("needs 1-9 capture points") }, // no points
		{ TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5,\"mp\":6},{\"x\":12,\"y\":14,\"mp\":0},{\"x\":6,\"y\":6,\"vp\":2}]}"), TEXT("points must be on open ground, vp and mp 0-5") }, // mp 6
	};
	const TCHAR* const SynMpNull = TEXT("{\"name\":\"Test Ford <&'>\",\"w\":24,\"h\":20,\"rows\":[\".....H..................\",\".....H..................\",\".....H..................\",\".....H..................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"........................\",\"...WWWWWWWWWWWWWWWWW....\",\"........................\",\"........................\",\"........................\",\"........................\",\"DDDDDDDDDDDDDDDDDDDDDDDD\",\"........................\",\"........................\",\"........................\",\"........................\"],\"heights\":[\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000044444\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"bbbbbbbbbbbbbbbbbbbbbbbb\",\"aaaaaaaaaaaaaaaaaaaaaaaa\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\",\"000000000000000000022222\"],\"spawns\":[{\"x\":2,\"y\":2},{\"x\":21,\"y\":17},{\"x\":2,\"y\":17},{\"x\":12,\"y\":3,\"assault\":true}],\"defend\":[1],\"assaultTime\":930,\"points\":[{\"x\":12,\"y\":5},{\"x\":12,\"y\":14,\"mp\":null},{\"x\":6,\"y\":6,\"vp\":2}]}");
	// client/main.js previewMap run in node: map, mode, FNV-1a of the RGBA bytes, scale, offsets, ring radius, the info
	// lines (HTML taken out), spawns as index@x,y#colour, points as x,y
	const FExpectedPreview SynPreviews[] = {
		{ TEXT("syn"), TEXT("conquest"), 3432846818u, 11.666666666666666, 0, 23.333333333333343, 46.666666666666664,
			{ TEXT("Test Ford <&'>"), TEXT("48 \u00D7 40 m"), TEXT("up to 3 players"), TEXT("3 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@2,2#f2ecd8 1@21,17#f2ecd8 2@2,17#f2ecd8"), TEXT("12,5 12,14 6,6") },
		{ TEXT("syn"), TEXT("assault"), 3432846818u, 11.666666666666666, 0, 23.333333333333343, 46.666666666666664,
			{ TEXT("Test Ford <&'>"), TEXT("48 \u00D7 40 m"), TEXT("up to 4 players"), TEXT("2 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("\u25CF defend, \u25CF attack"), TEXT("16 minute clock") },
			TEXT("0@2,2#3d7bd9 1@21,17#d94a3d 2@2,17#3d7bd9 3@12,3#3d7bd9"), TEXT("12,5 6,6") },
		{ TEXT("syn"), TEXT("horde"), 3432846818u, 11.666666666666666, 0, 23.333333333333343, 46.666666666666664,
			{ TEXT("Test Ford <&'>"), TEXT("48 \u00D7 40 m"), TEXT("up to 5 defenders"), TEXT("2 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("\u25CF your HQ is one of these, \u25CF the horde comes from here") },
			TEXT("0@2,2#3d7bd9 1@21,17#d94a3d 2@2,17#3d7bd9"), TEXT("12,5 6,6") },
		{ TEXT("syn"), TEXT("annihilation"), 3432846818u, 11.666666666666666, 0, 23.333333333333343, 46.666666666666664,
			{ TEXT("Test Ford <&'>"), TEXT("48 \u00D7 40 m"), TEXT("up to 3 players"), TEXT("2 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@2,2#f2ecd8 1@21,17#f2ecd8 2@2,17#f2ecd8"), TEXT("12,5 6,6") },
		{ TEXT("syn"), TEXT("classic"), 3432846818u, 11.666666666666666, 0, 23.333333333333343, 46.666666666666664,
			{ TEXT("Test Ford <&'>"), TEXT("48 \u00D7 40 m"), TEXT("up to 3 players"), TEXT("3 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@2,2#f2ecd8 1@21,17#f2ecd8 2@2,17#f2ecd8"), TEXT("12,5 12,14 6,6") },
	};
	// Generated from previewMap at e4e579a8. Regenerate with TCMatchOptions.Export.mjs.
	const FExpectedPreview DiskPreviews[] = {
		{ TEXT("ardennes-crossing"), TEXT("conquest"), 3299664412u, 2.8, 0, 0, 11.2,
			{ TEXT("Ardennes Crossing"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("9 capture points"), TEXT("rivers") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,93#f2ecd8 3@20,93#f2ecd8"), TEXT("50,49 20,45 79,53 35,43 64,55 9,32 90,67 58,28 41,71") },
		{ TEXT("ardennes-crossing"), TEXT("assault"), 3299664412u, 2.8, 0, 0, 11.2,
			{ TEXT("Ardennes Crossing"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("8 capture points"), TEXT("rivers") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,93#f2ecd8 3@20,93#f2ecd8"), TEXT("20,45 79,53 35,43 64,55 9,32 90,67 58,28 41,71") },
		{ TEXT("ardennes-crossing"), TEXT("horde"), 3299664412u, 2.8, 0, 0, 11.2,
			{ TEXT("Ardennes Crossing"), TEXT("200 × 200 m"), TEXT("up to 5 defenders"), TEXT("8 capture points"), TEXT("rivers") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,93#f2ecd8 3@20,93#f2ecd8"), TEXT("20,45 79,53 35,43 64,55 9,32 90,67 58,28 41,71") },
		{ TEXT("ardennes-crossing"), TEXT("annihilation"), 3299664412u, 2.8, 0, 0, 11.2,
			{ TEXT("Ardennes Crossing"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("8 capture points"), TEXT("rivers") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,93#f2ecd8 3@20,93#f2ecd8"), TEXT("20,45 79,53 35,43 64,55 9,32 90,67 58,28 41,71") },
		{ TEXT("ardennes-crossing"), TEXT("tutorial"), 3299664412u, 2.8, 0, 0, 11.2,
			{ TEXT("Ardennes Crossing"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("9 capture points"), TEXT("rivers") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,93#f2ecd8 3@20,93#f2ecd8"), TEXT("50,49 20,45 79,53 35,43 64,55 9,32 90,67 58,28 41,71") },
		{ TEXT("bastogne"), TEXT("conquest"), 3534317856u, 2.3333333333333335, 0, 0, 9.333333333333334,
			{ TEXT("Bastogne"), TEXT("240 × 240 m"), TEXT("up to 5 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("built for Assault") },
			TEXT("1@60,5#f2ecd8 2@112,43#f2ecd8 3@92,104#f2ecd8 4@28,104#f2ecd8 5@8,43#f2ecd8"), TEXT("60,63 60,25 93,49 81,88 39,88 27,49") },
		{ TEXT("bastogne"), TEXT("assault"), 3534317856u, 2.3333333333333335, 0, 0, 9.333333333333334,
			{ TEXT("Bastogne"), TEXT("240 × 240 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("● defend, ● attack") },
			TEXT("0@60,58#d94a3d 1@60,5#3d7bd9 2@112,43#3d7bd9 3@92,104#3d7bd9 4@28,104#3d7bd9 5@8,43#3d7bd9"), TEXT("60,63 60,25 93,49 81,88 39,88 27,49") },
		{ TEXT("bastogne"), TEXT("horde"), 3534317856u, 2.3333333333333335, 0, 0, 9.333333333333334,
			{ TEXT("Bastogne"), TEXT("240 × 240 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("1@60,5#3d7bd9 2@112,43#3d7bd9 3@92,104#3d7bd9 4@28,104#3d7bd9 5@8,43#3d7bd9"), TEXT("60,63 60,25 93,49 81,88 39,88 27,49") },
		{ TEXT("bastogne"), TEXT("annihilation"), 3534317856u, 2.3333333333333335, 0, 0, 9.333333333333334,
			{ TEXT("Bastogne"), TEXT("240 × 240 m"), TEXT("up to 5 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("built for Assault") },
			TEXT("1@60,5#f2ecd8 2@112,43#f2ecd8 3@92,104#f2ecd8 4@28,104#f2ecd8 5@8,43#f2ecd8"), TEXT("60,63 60,25 93,49 81,88 39,88 27,49") },
		{ TEXT("bastogne"), TEXT("tutorial"), 3534317856u, 2.3333333333333335, 0, 0, 9.333333333333334,
			{ TEXT("Bastogne"), TEXT("240 × 240 m"), TEXT("up to 5 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("built for Assault") },
			TEXT("1@60,5#f2ecd8 2@112,43#f2ecd8 3@92,104#f2ecd8 4@28,104#f2ecd8 5@8,43#f2ecd8"), TEXT("60,63 60,25 93,49 81,88 39,88 27,49") },
		{ TEXT("bocage"), TEXT("conquest"), 2376677160u, 2.9166666666666665, 0, 0, 11.666666666666666,
			{ TEXT("Bocage"), TEXT("192 × 192 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("built for Assault") },
			TEXT("0@36,5#f2ecd8 1@60,5#f2ecd8 2@36,90#f2ecd8 3@60,90#f2ecd8"), TEXT("48,26 24,42 72,42 48,58 48,72 24,72 72,72") },
		{ TEXT("bocage"), TEXT("assault"), 2376677160u, 2.9166666666666665, 0, 0, 11.666666666666666,
			{ TEXT("Bocage"), TEXT("192 × 192 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("● defend, ● attack") },
			TEXT("0@36,5#d94a3d 1@60,5#d94a3d 2@36,90#3d7bd9 3@60,90#3d7bd9"), TEXT("48,26 24,42 72,42 48,58 48,72 24,72 72,72") },
		{ TEXT("bocage"), TEXT("horde"), 2376677160u, 2.9166666666666665, 0, 0, 11.666666666666666,
			{ TEXT("Bocage"), TEXT("192 × 192 m"), TEXT("up to 5 defenders"), TEXT("7 capture points"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@36,5#d94a3d 1@60,5#d94a3d 2@36,90#3d7bd9 3@60,90#3d7bd9"), TEXT("48,26 24,42 72,42 48,58 48,72 24,72 72,72") },
		{ TEXT("bocage"), TEXT("annihilation"), 2376677160u, 2.9166666666666665, 0, 0, 11.666666666666666,
			{ TEXT("Bocage"), TEXT("192 × 192 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("built for Assault") },
			TEXT("0@36,5#f2ecd8 1@60,5#f2ecd8 2@36,90#f2ecd8 3@60,90#f2ecd8"), TEXT("48,26 24,42 72,42 48,58 48,72 24,72 72,72") },
		{ TEXT("bocage"), TEXT("tutorial"), 2376677160u, 2.9166666666666665, 0, 0, 11.666666666666666,
			{ TEXT("Bocage"), TEXT("192 × 192 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("built for Assault") },
			TEXT("0@36,5#f2ecd8 1@60,5#f2ecd8 2@36,90#f2ecd8 3@60,90#f2ecd8"), TEXT("48,26 24,42 72,42 48,58 48,72 24,72 72,72") },
		{ TEXT("crater-field"), TEXT("conquest"), 2648825857u, 3.111111111111111, 0, 0, 12.444444444444445,
			{ TEXT("Crater Field"), TEXT("180 × 180 m"), TEXT("up to 4 players"), TEXT("9 capture points") },
			TEXT("0@25,5#f2ecd8 1@64,5#f2ecd8 2@64,84#f2ecd8 3@25,84#f2ecd8"), TEXT("45,45 18,42 71,47 45,20 44,69 6,25 83,64 83,25 6,64") },
		{ TEXT("crater-field"), TEXT("assault"), 2648825857u, 3.111111111111111, 0, 0, 12.444444444444445,
			{ TEXT("Crater Field"), TEXT("180 × 180 m"), TEXT("up to 4 players"), TEXT("9 capture points") },
			TEXT("0@25,5#f2ecd8 1@64,5#f2ecd8 2@64,84#f2ecd8 3@25,84#f2ecd8"), TEXT("45,45 18,42 71,47 45,20 44,69 6,25 83,64 83,25 6,64") },
		{ TEXT("crater-field"), TEXT("horde"), 2648825857u, 3.111111111111111, 0, 0, 12.444444444444445,
			{ TEXT("Crater Field"), TEXT("180 × 180 m"), TEXT("up to 5 defenders"), TEXT("9 capture points") },
			TEXT("0@25,5#f2ecd8 1@64,5#f2ecd8 2@64,84#f2ecd8 3@25,84#f2ecd8"), TEXT("45,45 18,42 71,47 45,20 44,69 6,25 83,64 83,25 6,64") },
		{ TEXT("crater-field"), TEXT("annihilation"), 2648825857u, 3.111111111111111, 0, 0, 12.444444444444445,
			{ TEXT("Crater Field"), TEXT("180 × 180 m"), TEXT("up to 4 players"), TEXT("9 capture points") },
			TEXT("0@25,5#f2ecd8 1@64,5#f2ecd8 2@64,84#f2ecd8 3@25,84#f2ecd8"), TEXT("45,45 18,42 71,47 45,20 44,69 6,25 83,64 83,25 6,64") },
		{ TEXT("crater-field"), TEXT("tutorial"), 2648825857u, 3.111111111111111, 0, 0, 12.444444444444445,
			{ TEXT("Crater Field"), TEXT("180 × 180 m"), TEXT("up to 4 players"), TEXT("9 capture points") },
			TEXT("0@25,5#f2ecd8 1@64,5#f2ecd8 2@64,84#f2ecd8 3@25,84#f2ecd8"), TEXT("45,45 18,42 71,47 45,20 44,69 6,25 83,64 83,25 6,64") },
		{ TEXT("crossroads-village"), TEXT("conquest"), 651637033u, 4.375, 0, 0, 17.5,
			{ TEXT("Crossroads Village"), TEXT("128 × 128 m"), TEXT("up to 2 players"), TEXT("5 capture points") },
			TEXT("0@6,57#f2ecd8 1@57,6#f2ecd8"), TEXT("32,32 16,34 47,29 30,50 33,13") },
		{ TEXT("crossroads-village"), TEXT("assault"), 651637033u, 4.375, 0, 0, 17.5,
			{ TEXT("Crossroads Village"), TEXT("128 × 128 m"), TEXT("up to 2 players"), TEXT("5 capture points") },
			TEXT("0@6,57#f2ecd8 1@57,6#f2ecd8"), TEXT("32,32 16,34 47,29 30,50 33,13") },
		{ TEXT("crossroads-village"), TEXT("horde"), 651637033u, 4.375, 0, 0, 17.5,
			{ TEXT("Crossroads Village"), TEXT("128 × 128 m"), TEXT("up to 5 defenders"), TEXT("5 capture points") },
			TEXT("0@6,57#f2ecd8 1@57,6#f2ecd8"), TEXT("32,32 16,34 47,29 30,50 33,13") },
		{ TEXT("crossroads-village"), TEXT("annihilation"), 651637033u, 4.375, 0, 0, 17.5,
			{ TEXT("Crossroads Village"), TEXT("128 × 128 m"), TEXT("up to 2 players"), TEXT("5 capture points") },
			TEXT("0@6,57#f2ecd8 1@57,6#f2ecd8"), TEXT("32,32 16,34 47,29 30,50 33,13") },
		{ TEXT("crossroads-village"), TEXT("tutorial"), 651637033u, 4.375, 0, 0, 17.5,
			{ TEXT("Crossroads Village"), TEXT("128 × 128 m"), TEXT("up to 2 players"), TEXT("5 capture points") },
			TEXT("0@6,57#f2ecd8 1@57,6#f2ecd8"), TEXT("32,32 16,34 47,29 30,50 33,13") },
		{ TEXT("default"), TEXT("conquest"), 3491444537u, 3.5, 0, 0, 14,
			{ TEXT("Three Crossroads"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("7 capture points"), TEXT("rolling hills") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,40 40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("default"), TEXT("assault"), 3491444537u, 3.5, 0, 0, 14,
			{ TEXT("Three Crossroads"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("6 capture points"), TEXT("rolling hills") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("default"), TEXT("horde"), 3491444537u, 3.5, 0, 0, 14,
			{ TEXT("Three Crossroads"), TEXT("160 × 160 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("rolling hills") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("default"), TEXT("annihilation"), 3491444537u, 3.5, 0, 0, 14,
			{ TEXT("Three Crossroads"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("6 capture points"), TEXT("rolling hills") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("default"), TEXT("tutorial"), 3491444537u, 3.5, 0, 0, 14,
			{ TEXT("Three Crossroads"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("7 capture points"), TEXT("rolling hills") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,40 40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("el-alamein"), TEXT("conquest"), 4160251348u, 2, 0, 40, 8,
			{ TEXT("El Alamein"), TEXT("280 × 200 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@118,46#f2ecd8 1@118,70#f2ecd8 2@6,40#f2ecd8 3@6,70#f2ecd8"), TEXT("30,54 72,50 88,23 112,40 112,72 116,25 43,25") },
		{ TEXT("el-alamein"), TEXT("assault"), 4160251348u, 2, 0, 40, 8,
			{ TEXT("El Alamein"), TEXT("280 × 200 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("● defend, ● attack"), TEXT("30 minute clock") },
			TEXT("0@118,46#d94a3d 1@118,70#d94a3d 2@6,40#3d7bd9 3@6,70#3d7bd9"), TEXT("30,54 72,50 88,23 112,40 112,72 116,25 43,25") },
		{ TEXT("el-alamein"), TEXT("horde"), 4160251348u, 2, 0, 40, 8,
			{ TEXT("El Alamein"), TEXT("280 × 200 m"), TEXT("up to 5 defenders"), TEXT("7 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@118,46#d94a3d 1@118,70#d94a3d 2@6,40#3d7bd9 3@6,70#3d7bd9"), TEXT("30,54 72,50 88,23 112,40 112,72 116,25 43,25") },
		{ TEXT("el-alamein"), TEXT("annihilation"), 4160251348u, 2, 0, 40, 8,
			{ TEXT("El Alamein"), TEXT("280 × 200 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@118,46#f2ecd8 1@118,70#f2ecd8 2@6,40#f2ecd8 3@6,70#f2ecd8"), TEXT("30,54 72,50 88,23 112,40 112,72 116,25 43,25") },
		{ TEXT("el-alamein"), TEXT("tutorial"), 4160251348u, 2, 0, 40, 8,
			{ TEXT("El Alamein"), TEXT("280 × 200 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@118,46#f2ecd8 1@118,70#f2ecd8 2@6,40#f2ecd8 3@6,70#f2ecd8"), TEXT("30,54 72,50 88,23 112,40 112,72 116,25 43,25") },
		{ TEXT("helms-deep"), TEXT("conquest"), 1341625533u, 2.1538461538461537, 21.538461538461547, 0, 8.615384615384615,
			{ TEXT("Helm's Deep"), TEXT("220 × 260 m"), TEXT("up to 3 players"), TEXT("5 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("1@28,122#f2ecd8 2@55,124#f2ecd8 3@82,122#f2ecd8"), TEXT("30,58 62,57 56,18 55,90 76,42") },
		{ TEXT("helms-deep"), TEXT("assault"), 1341625533u, 2.1538461538461537, 21.538461538461547, 0, 8.615384615384615,
			{ TEXT("Helm's Deep"), TEXT("220 × 260 m"), TEXT("up to 4 players"), TEXT("5 capture points"), TEXT("hills and cliffs"), TEXT("● defend, ● attack") },
			TEXT("0@36,54#d94a3d 1@28,122#3d7bd9 2@55,124#3d7bd9 3@82,122#3d7bd9"), TEXT("30,58 62,57 56,18 55,90 76,42") },
		{ TEXT("helms-deep"), TEXT("horde"), 1341625533u, 2.1538461538461537, 21.538461538461547, 0, 8.615384615384615,
			{ TEXT("Helm's Deep"), TEXT("220 × 260 m"), TEXT("up to 5 defenders"), TEXT("5 capture points"), TEXT("hills and cliffs"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("1@28,122#3d7bd9 2@55,124#3d7bd9 3@82,122#3d7bd9"), TEXT("30,58 62,57 56,18 55,90 76,42") },
		{ TEXT("helms-deep"), TEXT("annihilation"), 1341625533u, 2.1538461538461537, 21.538461538461547, 0, 8.615384615384615,
			{ TEXT("Helm's Deep"), TEXT("220 × 260 m"), TEXT("up to 3 players"), TEXT("5 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("1@28,122#f2ecd8 2@55,124#f2ecd8 3@82,122#f2ecd8"), TEXT("30,58 62,57 56,18 55,90 76,42") },
		{ TEXT("helms-deep"), TEXT("tutorial"), 1341625533u, 2.1538461538461537, 21.538461538461547, 0, 8.615384615384615,
			{ TEXT("Helm's Deep"), TEXT("220 × 260 m"), TEXT("up to 3 players"), TEXT("5 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("1@28,122#f2ecd8 2@55,124#f2ecd8 3@82,122#f2ecd8"), TEXT("30,58 62,57 56,18 55,90 76,42") },
		{ TEXT("hill-112-xl"), TEXT("conquest"), 2824717946u, 1.75, 35, 0, 7,
			{ TEXT("Hill 112 XL"), TEXT("240 × 320 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@34,10#f2ecd8 1@60,14#f2ecd8 2@86,10#f2ecd8 3@94,150#f2ecd8 4@60,152#f2ecd8 5@26,150#f2ecd8"), TEXT("60,22 60,74 26,70 94,70 24,104 96,104") },
		{ TEXT("hill-112-xl"), TEXT("assault"), 2824717946u, 1.75, 35, 0, 7,
			{ TEXT("Hill 112 XL"), TEXT("240 × 320 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("● defend, ● attack"), TEXT("16 minute clock") },
			TEXT("0@34,10#d94a3d 1@60,14#d94a3d 2@86,10#d94a3d 3@94,150#3d7bd9 4@60,152#3d7bd9 5@26,150#3d7bd9"), TEXT("60,22 60,74 26,70 94,70 24,104 96,104") },
		{ TEXT("hill-112-xl"), TEXT("horde"), 2824717946u, 1.75, 35, 0, 7,
			{ TEXT("Hill 112 XL"), TEXT("240 × 320 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@34,10#d94a3d 1@60,14#d94a3d 2@86,10#d94a3d 3@94,150#3d7bd9 4@60,152#3d7bd9 5@26,150#3d7bd9"), TEXT("60,22 60,74 26,70 94,70 24,104 96,104") },
		{ TEXT("hill-112-xl"), TEXT("annihilation"), 2824717946u, 1.75, 35, 0, 7,
			{ TEXT("Hill 112 XL"), TEXT("240 × 320 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@34,10#f2ecd8 1@60,14#f2ecd8 2@86,10#f2ecd8 3@94,150#f2ecd8 4@60,152#f2ecd8 5@26,150#f2ecd8"), TEXT("60,22 60,74 26,70 94,70 24,104 96,104") },
		{ TEXT("hill-112-xl"), TEXT("tutorial"), 2824717946u, 1.75, 35, 0, 7,
			{ TEXT("Hill 112 XL"), TEXT("240 × 320 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@34,10#f2ecd8 1@60,14#f2ecd8 2@86,10#f2ecd8 3@94,150#f2ecd8 4@60,152#f2ecd8 5@26,150#f2ecd8"), TEXT("60,22 60,74 26,70 94,70 24,104 96,104") },
		{ TEXT("hill-112"), TEXT("conquest"), 2007954041u, 2.5454545454545454, 38.18181818181819, 0, 10.181818181818182,
			{ TEXT("Hill 112"), TEXT("160 × 220 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@28,9#f2ecd8 1@52,9#f2ecd8 2@20,102#f2ecd8 3@60,102#f2ecd8"), TEXT("40,14 40,50 16,72 64,72") },
		{ TEXT("hill-112"), TEXT("assault"), 2007954041u, 2.5454545454545454, 38.18181818181819, 0, 10.181818181818182,
			{ TEXT("Hill 112"), TEXT("160 × 220 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("● defend, ● attack") },
			TEXT("0@28,9#d94a3d 1@52,9#d94a3d 2@20,102#3d7bd9 3@60,102#3d7bd9"), TEXT("40,14 40,50 16,72 64,72") },
		{ TEXT("hill-112"), TEXT("horde"), 2007954041u, 2.5454545454545454, 38.18181818181819, 0, 10.181818181818182,
			{ TEXT("Hill 112"), TEXT("160 × 220 m"), TEXT("up to 5 defenders"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@28,9#d94a3d 1@52,9#d94a3d 2@20,102#3d7bd9 3@60,102#3d7bd9"), TEXT("40,14 40,50 16,72 64,72") },
		{ TEXT("hill-112"), TEXT("annihilation"), 2007954041u, 2.5454545454545454, 38.18181818181819, 0, 10.181818181818182,
			{ TEXT("Hill 112"), TEXT("160 × 220 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@28,9#f2ecd8 1@52,9#f2ecd8 2@20,102#f2ecd8 3@60,102#f2ecd8"), TEXT("40,14 40,50 16,72 64,72") },
		{ TEXT("hill-112"), TEXT("tutorial"), 2007954041u, 2.5454545454545454, 38.18181818181819, 0, 10.181818181818182,
			{ TEXT("Hill 112"), TEXT("160 × 220 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@28,9#f2ecd8 1@52,9#f2ecd8 2@20,102#f2ecd8 3@60,102#f2ecd8"), TEXT("40,14 40,50 16,72 64,72") },
		{ TEXT("hot-gates"), TEXT("conquest"), 4072343294u, 2.074074074074074, 0, 57.03703703703704, 8.296296296296296,
			{ TEXT("The Hot Gates"), TEXT("270 × 160 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@114,26#f2ecd8 1@5,22#f2ecd8 2@5,36#f2ecd8 3@50,64#f2ecd8"), TEXT("86,12 118,30 106,45 48,22") },
		{ TEXT("hot-gates"), TEXT("assault"), 4072343294u, 2.074074074074074, 0, 57.03703703703704, 8.296296296296296,
			{ TEXT("The Hot Gates"), TEXT("270 × 160 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("● defend, ● attack") },
			TEXT("0@114,26#d94a3d 1@5,22#3d7bd9 2@5,36#3d7bd9 3@50,64#3d7bd9"), TEXT("86,12 118,30 106,45 48,22") },
		{ TEXT("hot-gates"), TEXT("horde"), 4072343294u, 2.074074074074074, 0, 57.03703703703704, 8.296296296296296,
			{ TEXT("The Hot Gates"), TEXT("270 × 160 m"), TEXT("up to 5 defenders"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@114,26#d94a3d 1@5,22#3d7bd9 2@5,36#3d7bd9 3@50,64#3d7bd9"), TEXT("86,12 118,30 106,45 48,22") },
		{ TEXT("hot-gates"), TEXT("annihilation"), 4072343294u, 2.074074074074074, 0, 57.03703703703704, 8.296296296296296,
			{ TEXT("The Hot Gates"), TEXT("270 × 160 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@114,26#f2ecd8 1@5,22#f2ecd8 2@5,36#f2ecd8 3@50,64#f2ecd8"), TEXT("86,12 118,30 106,45 48,22") },
		{ TEXT("hot-gates"), TEXT("tutorial"), 4072343294u, 2.074074074074074, 0, 57.03703703703704, 8.296296296296296,
			{ TEXT("The Hot Gates"), TEXT("270 × 160 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@114,26#f2ecd8 1@5,22#f2ecd8 2@5,36#f2ecd8 3@50,64#f2ecd8"), TEXT("86,12 118,30 106,45 48,22") },
		{ TEXT("hurtgen-forest"), TEXT("conquest"), 232924321u, 2.5454545454545454, 0, 12.727272727272734, 10.181818181818182,
			{ TEXT("Hurtgen Forest"), TEXT("220 × 200 m"), TEXT("up to 4 players"), TEXT("8 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@4,26#f2ecd8 1@4,74#f2ecd8 2@105,73#f2ecd8 3@105,25#f2ecd8"), TEXT("46,49 63,50 26,40 83,59 16,16 93,83 57,16 52,83") },
		{ TEXT("hurtgen-forest"), TEXT("assault"), 232924321u, 2.5454545454545454, 0, 12.727272727272734, 10.181818181818182,
			{ TEXT("Hurtgen Forest"), TEXT("220 × 200 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@4,26#f2ecd8 1@4,74#f2ecd8 2@105,73#f2ecd8 3@105,25#f2ecd8"), TEXT("26,40 83,59 16,16 93,83 57,16 52,83") },
		{ TEXT("hurtgen-forest"), TEXT("horde"), 232924321u, 2.5454545454545454, 0, 12.727272727272734, 10.181818181818182,
			{ TEXT("Hurtgen Forest"), TEXT("220 × 200 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@4,26#f2ecd8 1@4,74#f2ecd8 2@105,73#f2ecd8 3@105,25#f2ecd8"), TEXT("26,40 83,59 16,16 93,83 57,16 52,83") },
		{ TEXT("hurtgen-forest"), TEXT("annihilation"), 232924321u, 2.5454545454545454, 0, 12.727272727272734, 10.181818181818182,
			{ TEXT("Hurtgen Forest"), TEXT("220 × 200 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@4,26#f2ecd8 1@4,74#f2ecd8 2@105,73#f2ecd8 3@105,25#f2ecd8"), TEXT("26,40 83,59 16,16 93,83 57,16 52,83") },
		{ TEXT("hurtgen-forest"), TEXT("tutorial"), 232924321u, 2.5454545454545454, 0, 12.727272727272734, 10.181818181818182,
			{ TEXT("Hurtgen Forest"), TEXT("220 × 200 m"), TEXT("up to 4 players"), TEXT("8 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@4,26#f2ecd8 1@4,74#f2ecd8 2@105,73#f2ecd8 3@105,25#f2ecd8"), TEXT("46,49 63,50 26,40 83,59 16,16 93,83 57,16 52,83") },
		{ TEXT("island-towns"), TEXT("conquest"), 3900993386u, 2, 0, 0, 8,
			{ TEXT("Island Towns"), TEXT("280 × 280 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("rivers") },
			TEXT("0@70,16#f2ecd8 1@117,43#f2ecd8 2@117,97#f2ecd8 3@70,124#f2ecd8 4@23,97#f2ecd8 5@23,43#f2ecd8"), TEXT("70,70 70,30 105,50 105,90 70,110 35,90 35,50") },
		{ TEXT("island-towns"), TEXT("assault"), 3900993386u, 2, 0, 0, 8,
			{ TEXT("Island Towns"), TEXT("280 × 280 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("rivers") },
			TEXT("0@70,16#f2ecd8 1@117,43#f2ecd8 2@117,97#f2ecd8 3@70,124#f2ecd8 4@23,97#f2ecd8 5@23,43#f2ecd8"), TEXT("70,30 105,50 105,90 70,110 35,90 35,50") },
		{ TEXT("island-towns"), TEXT("horde"), 3900993386u, 2, 0, 0, 8,
			{ TEXT("Island Towns"), TEXT("280 × 280 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("rivers") },
			TEXT("0@70,16#f2ecd8 1@117,43#f2ecd8 2@117,97#f2ecd8 3@70,124#f2ecd8 4@23,97#f2ecd8 5@23,43#f2ecd8"), TEXT("70,30 105,50 105,90 70,110 35,90 35,50") },
		{ TEXT("island-towns"), TEXT("annihilation"), 3900993386u, 2, 0, 0, 8,
			{ TEXT("Island Towns"), TEXT("280 × 280 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("rivers") },
			TEXT("0@70,16#f2ecd8 1@117,43#f2ecd8 2@117,97#f2ecd8 3@70,124#f2ecd8 4@23,97#f2ecd8 5@23,43#f2ecd8"), TEXT("70,30 105,50 105,90 70,110 35,90 35,50") },
		{ TEXT("island-towns"), TEXT("tutorial"), 3900993386u, 2, 0, 0, 8,
			{ TEXT("Island Towns"), TEXT("280 × 280 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("rivers") },
			TEXT("0@70,16#f2ecd8 1@117,43#f2ecd8 2@117,97#f2ecd8 3@70,124#f2ecd8 4@23,97#f2ecd8 5@23,43#f2ecd8"), TEXT("70,70 70,30 105,50 105,90 70,110 35,90 35,50") },
		{ TEXT("kasserine-pass"), TEXT("conquest"), 2624070678u, 1.4, 28, 0, 5.6,
			{ TEXT("Kasserine Pass"), TEXT("320 × 400 m"), TEXT("up to 6 players"), TEXT("8 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@40,10#f2ecd8 1@80,8#f2ecd8 2@120,10#f2ecd8 3@40,190#f2ecd8 4@80,192#f2ecd8 5@120,190#f2ecd8"), TEXT("80,32 40,44 120,44 80,82 80,101 79,120 40,160 120,160") },
		{ TEXT("kasserine-pass"), TEXT("assault"), 2624070678u, 1.4, 28, 0, 5.6,
			{ TEXT("Kasserine Pass"), TEXT("320 × 400 m"), TEXT("up to 6 players"), TEXT("8 capture points"), TEXT("hills and cliffs"), TEXT("● defend, ● attack") },
			TEXT("0@40,10#d94a3d 1@80,8#d94a3d 2@120,10#d94a3d 3@40,190#3d7bd9 4@80,192#3d7bd9 5@120,190#3d7bd9"), TEXT("80,32 40,44 120,44 80,82 80,101 79,120 40,160 120,160") },
		{ TEXT("kasserine-pass"), TEXT("horde"), 2624070678u, 1.4, 28, 0, 5.6,
			{ TEXT("Kasserine Pass"), TEXT("320 × 400 m"), TEXT("up to 5 defenders"), TEXT("8 capture points"), TEXT("hills and cliffs"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@40,10#d94a3d 1@80,8#d94a3d 2@120,10#d94a3d 3@40,190#3d7bd9 4@80,192#3d7bd9 5@120,190#3d7bd9"), TEXT("80,32 40,44 120,44 80,82 80,101 79,120 40,160 120,160") },
		{ TEXT("kasserine-pass"), TEXT("annihilation"), 2624070678u, 1.4, 28, 0, 5.6,
			{ TEXT("Kasserine Pass"), TEXT("320 × 400 m"), TEXT("up to 6 players"), TEXT("8 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@40,10#f2ecd8 1@80,8#f2ecd8 2@120,10#f2ecd8 3@40,190#f2ecd8 4@80,192#f2ecd8 5@120,190#f2ecd8"), TEXT("80,32 40,44 120,44 80,82 80,101 79,120 40,160 120,160") },
		{ TEXT("kasserine-pass"), TEXT("tutorial"), 2624070678u, 1.4, 28, 0, 5.6,
			{ TEXT("Kasserine Pass"), TEXT("320 × 400 m"), TEXT("up to 6 players"), TEXT("8 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@40,10#f2ecd8 1@80,8#f2ecd8 2@120,10#f2ecd8 3@40,190#f2ecd8 4@80,192#f2ecd8 5@120,190#f2ecd8"), TEXT("80,32 40,44 120,44 80,82 80,101 79,120 40,160 120,160") },
		{ TEXT("king-of-the-hill"), TEXT("conquest"), 3679864019u, 2.1538461538461537, 0, 0, 8.615384615384615,
			{ TEXT("King of the Hill"), TEXT("260 × 260 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("hills and cliffs") },
			TEXT("0@65,7#f2ecd8 1@115,36#f2ecd8 2@115,94#f2ecd8 3@65,123#f2ecd8 4@15,94#f2ecd8 5@15,36#f2ecd8"), TEXT("65,65 86,29 107,65 86,101 44,101 23,65 44,29") },
		{ TEXT("king-of-the-hill"), TEXT("assault"), 3679864019u, 2.1538461538461537, 0, 0, 8.615384615384615,
			{ TEXT("King of the Hill"), TEXT("260 × 260 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("hills and cliffs") },
			TEXT("0@65,7#f2ecd8 1@115,36#f2ecd8 2@115,94#f2ecd8 3@65,123#f2ecd8 4@15,94#f2ecd8 5@15,36#f2ecd8"), TEXT("86,29 107,65 86,101 44,101 23,65 44,29") },
		{ TEXT("king-of-the-hill"), TEXT("horde"), 3679864019u, 2.1538461538461537, 0, 0, 8.615384615384615,
			{ TEXT("King of the Hill"), TEXT("260 × 260 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("hills and cliffs") },
			TEXT("0@65,7#f2ecd8 1@115,36#f2ecd8 2@115,94#f2ecd8 3@65,123#f2ecd8 4@15,94#f2ecd8 5@15,36#f2ecd8"), TEXT("86,29 107,65 86,101 44,101 23,65 44,29") },
		{ TEXT("king-of-the-hill"), TEXT("annihilation"), 3679864019u, 2.1538461538461537, 0, 0, 8.615384615384615,
			{ TEXT("King of the Hill"), TEXT("260 × 260 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("hills and cliffs") },
			TEXT("0@65,7#f2ecd8 1@115,36#f2ecd8 2@115,94#f2ecd8 3@65,123#f2ecd8 4@15,94#f2ecd8 5@15,36#f2ecd8"), TEXT("86,29 107,65 86,101 44,101 23,65 44,29") },
		{ TEXT("king-of-the-hill"), TEXT("tutorial"), 3679864019u, 2.1538461538461537, 0, 0, 8.615384615384615,
			{ TEXT("King of the Hill"), TEXT("260 × 260 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("hills and cliffs") },
			TEXT("0@65,7#f2ecd8 1@115,36#f2ecd8 2@115,94#f2ecd8 3@65,123#f2ecd8 4@15,94#f2ecd8 5@15,36#f2ecd8"), TEXT("65,65 86,29 107,65 86,101 44,101 23,65 44,29") },
		{ TEXT("monte-cassino-xl"), TEXT("conquest"), 3797070370u, 1.696969696969697, 25.454545454545453, 0, 6.787878787878788,
			{ TEXT("Monte Cassino XL"), TEXT("270 × 330 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@26,14#f2ecd8 1@67,5#f2ecd8 2@108,14#f2ecd8 3@102,158#f2ecd8 4@67,160#f2ecd8 5@32,158#f2ecd8"), TEXT("67,27 36,66 99,66 67,70 22,100 113,100 67,124") },
		{ TEXT("monte-cassino-xl"), TEXT("assault"), 3797070370u, 1.696969696969697, 25.454545454545453, 0, 6.787878787878788,
			{ TEXT("Monte Cassino XL"), TEXT("270 × 330 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("hills and cliffs"), TEXT("● defend, ● attack"), TEXT("23 minute clock") },
			TEXT("0@26,14#d94a3d 1@67,5#d94a3d 2@108,14#d94a3d 3@102,158#3d7bd9 4@67,160#3d7bd9 5@32,158#3d7bd9"), TEXT("67,27 36,66 99,66 67,70 22,100 113,100 67,124") },
		{ TEXT("monte-cassino-xl"), TEXT("horde"), 3797070370u, 1.696969696969697, 25.454545454545453, 0, 6.787878787878788,
			{ TEXT("Monte Cassino XL"), TEXT("270 × 330 m"), TEXT("up to 5 defenders"), TEXT("7 capture points"), TEXT("hills and cliffs"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@26,14#d94a3d 1@67,5#d94a3d 2@108,14#d94a3d 3@102,158#3d7bd9 4@67,160#3d7bd9 5@32,158#3d7bd9"), TEXT("67,27 36,66 99,66 67,70 22,100 113,100 67,124") },
		{ TEXT("monte-cassino-xl"), TEXT("annihilation"), 3797070370u, 1.696969696969697, 25.454545454545453, 0, 6.787878787878788,
			{ TEXT("Monte Cassino XL"), TEXT("270 × 330 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@26,14#f2ecd8 1@67,5#f2ecd8 2@108,14#f2ecd8 3@102,158#f2ecd8 4@67,160#f2ecd8 5@32,158#f2ecd8"), TEXT("67,27 36,66 99,66 67,70 22,100 113,100 67,124") },
		{ TEXT("monte-cassino-xl"), TEXT("tutorial"), 3797070370u, 1.696969696969697, 25.454545454545453, 0, 6.787878787878788,
			{ TEXT("Monte Cassino XL"), TEXT("270 × 330 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@26,14#f2ecd8 1@67,5#f2ecd8 2@108,14#f2ecd8 3@102,158#f2ecd8 4@67,160#f2ecd8 5@32,158#f2ecd8"), TEXT("67,27 36,66 99,66 67,70 22,100 113,100 67,124") },
		{ TEXT("monte-cassino"), TEXT("conquest"), 39369565u, 2.5454545454545454, 25.454545454545453, 0, 10.181818181818182,
			{ TEXT("Monte Cassino"), TEXT("180 × 220 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@18,10#f2ecd8 1@72,10#f2ecd8 2@30,104#f2ecd8 3@60,104#f2ecd8"), TEXT("45,18 30,46 70,46 45,66") },
		{ TEXT("monte-cassino"), TEXT("assault"), 39369565u, 2.5454545454545454, 25.454545454545453, 0, 10.181818181818182,
			{ TEXT("Monte Cassino"), TEXT("180 × 220 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("● defend, ● attack") },
			TEXT("0@18,10#d94a3d 1@72,10#d94a3d 2@30,104#3d7bd9 3@60,104#3d7bd9"), TEXT("45,18 30,46 70,46 45,66") },
		{ TEXT("monte-cassino"), TEXT("horde"), 39369565u, 2.5454545454545454, 25.454545454545453, 0, 10.181818181818182,
			{ TEXT("Monte Cassino"), TEXT("180 × 220 m"), TEXT("up to 5 defenders"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@18,10#d94a3d 1@72,10#d94a3d 2@30,104#3d7bd9 3@60,104#3d7bd9"), TEXT("45,18 30,46 70,46 45,66") },
		{ TEXT("monte-cassino"), TEXT("annihilation"), 39369565u, 2.5454545454545454, 25.454545454545453, 0, 10.181818181818182,
			{ TEXT("Monte Cassino"), TEXT("180 × 220 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@18,10#f2ecd8 1@72,10#f2ecd8 2@30,104#f2ecd8 3@60,104#f2ecd8"), TEXT("45,18 30,46 70,46 45,66") },
		{ TEXT("monte-cassino"), TEXT("tutorial"), 39369565u, 2.5454545454545454, 25.454545454545453, 0, 10.181818181818182,
			{ TEXT("Monte Cassino"), TEXT("180 × 220 m"), TEXT("up to 4 players"), TEXT("4 capture points"), TEXT("hills and cliffs"), TEXT("built for Assault") },
			TEXT("0@18,10#f2ecd8 1@72,10#f2ecd8 2@30,104#f2ecd8 3@60,104#f2ecd8"), TEXT("45,18 30,46 70,46 45,66") },
		{ TEXT("no-mans-land"), TEXT("conquest"), 3369846593u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("No Man's Land"), TEXT("240 × 180 m"), TEXT("up to 4 players"), TEXT("9 capture points") },
			TEXT("0@32,4#f2ecd8 1@88,4#f2ecd8 2@87,85#f2ecd8 3@31,85#f2ecd8"), TEXT("60,45 14,43 105,46 60,27 59,62 15,25 104,64 105,25 14,64") },
		{ TEXT("no-mans-land"), TEXT("assault"), 3369846593u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("No Man's Land"), TEXT("240 × 180 m"), TEXT("up to 4 players"), TEXT("8 capture points") },
			TEXT("0@32,4#f2ecd8 1@88,4#f2ecd8 2@87,85#f2ecd8 3@31,85#f2ecd8"), TEXT("14,43 105,46 60,27 59,62 15,25 104,64 105,25 14,64") },
		{ TEXT("no-mans-land"), TEXT("horde"), 3369846593u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("No Man's Land"), TEXT("240 × 180 m"), TEXT("up to 5 defenders"), TEXT("8 capture points") },
			TEXT("0@32,4#f2ecd8 1@88,4#f2ecd8 2@87,85#f2ecd8 3@31,85#f2ecd8"), TEXT("14,43 105,46 60,27 59,62 15,25 104,64 105,25 14,64") },
		{ TEXT("no-mans-land"), TEXT("annihilation"), 3369846593u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("No Man's Land"), TEXT("240 × 180 m"), TEXT("up to 4 players"), TEXT("8 capture points") },
			TEXT("0@32,4#f2ecd8 1@88,4#f2ecd8 2@87,85#f2ecd8 3@31,85#f2ecd8"), TEXT("14,43 105,46 60,27 59,62 15,25 104,64 105,25 14,64") },
		{ TEXT("no-mans-land"), TEXT("tutorial"), 3369846593u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("No Man's Land"), TEXT("240 × 180 m"), TEXT("up to 4 players"), TEXT("9 capture points") },
			TEXT("0@32,4#f2ecd8 1@88,4#f2ecd8 2@87,85#f2ecd8 3@31,85#f2ecd8"), TEXT("60,45 14,43 105,46 60,27 59,62 15,25 104,64 105,25 14,64") },
		{ TEXT("pegasus-bridge-xl"), TEXT("conquest"), 4040184917u, 1.8666666666666667, 9.333333333333343, 0, 7.466666666666667,
			{ TEXT("Pegasus Bridge XL"), TEXT("280 × 300 m"), TEXT("up to 6 players"), TEXT("8 capture points"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@34,6#f2ecd8 1@70,6#f2ecd8 2@106,6#f2ecd8 3@110,143#f2ecd8 4@70,143#f2ecd8 5@30,143#f2ecd8"), TEXT("70,28 46,64 94,64 20,42 120,42 70,70 30,92 110,92") },
		{ TEXT("pegasus-bridge-xl"), TEXT("assault"), 4040184917u, 1.8666666666666667, 9.333333333333343, 0, 7.466666666666667,
			{ TEXT("Pegasus Bridge XL"), TEXT("280 × 300 m"), TEXT("up to 6 players"), TEXT("8 capture points"), TEXT("rivers"), TEXT("● defend, ● attack"), TEXT("20 minute clock") },
			TEXT("0@34,6#d94a3d 1@70,6#d94a3d 2@106,6#d94a3d 3@110,143#3d7bd9 4@70,143#3d7bd9 5@30,143#3d7bd9"), TEXT("70,28 46,64 94,64 20,42 120,42 70,70 30,92 110,92") },
		{ TEXT("pegasus-bridge-xl"), TEXT("horde"), 4040184917u, 1.8666666666666667, 9.333333333333343, 0, 7.466666666666667,
			{ TEXT("Pegasus Bridge XL"), TEXT("280 × 300 m"), TEXT("up to 5 defenders"), TEXT("8 capture points"), TEXT("rivers"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@34,6#d94a3d 1@70,6#d94a3d 2@106,6#d94a3d 3@110,143#3d7bd9 4@70,143#3d7bd9 5@30,143#3d7bd9"), TEXT("70,28 46,64 94,64 20,42 120,42 70,70 30,92 110,92") },
		{ TEXT("pegasus-bridge-xl"), TEXT("annihilation"), 4040184917u, 1.8666666666666667, 9.333333333333343, 0, 7.466666666666667,
			{ TEXT("Pegasus Bridge XL"), TEXT("280 × 300 m"), TEXT("up to 6 players"), TEXT("8 capture points"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@34,6#f2ecd8 1@70,6#f2ecd8 2@106,6#f2ecd8 3@110,143#f2ecd8 4@70,143#f2ecd8 5@30,143#f2ecd8"), TEXT("70,28 46,64 94,64 20,42 120,42 70,70 30,92 110,92") },
		{ TEXT("pegasus-bridge-xl"), TEXT("tutorial"), 4040184917u, 1.8666666666666667, 9.333333333333343, 0, 7.466666666666667,
			{ TEXT("Pegasus Bridge XL"), TEXT("280 × 300 m"), TEXT("up to 6 players"), TEXT("8 capture points"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@34,6#f2ecd8 1@70,6#f2ecd8 2@106,6#f2ecd8 3@110,143#f2ecd8 4@70,143#f2ecd8 5@30,143#f2ecd8"), TEXT("70,28 46,64 94,64 20,42 120,42 70,70 30,92 110,92") },
		{ TEXT("pegasus-bridge"), TEXT("conquest"), 1535330139u, 2.8, 14.000000000000014, 0, 11.2,
			{ TEXT("Pegasus Bridge"), TEXT("180 × 200 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@28,5#f2ecd8 1@62,5#f2ecd8 2@28,94#f2ecd8 3@62,94#f2ecd8"), TEXT("45,22 45,41 14,30 76,30 45,52 20,63 70,63") },
		{ TEXT("pegasus-bridge"), TEXT("assault"), 1535330139u, 2.8, 14.000000000000014, 0, 11.2,
			{ TEXT("Pegasus Bridge"), TEXT("180 × 200 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rivers"), TEXT("● defend, ● attack") },
			TEXT("0@28,5#d94a3d 1@62,5#d94a3d 2@28,94#3d7bd9 3@62,94#3d7bd9"), TEXT("45,22 45,41 14,30 76,30 45,52 20,63 70,63") },
		{ TEXT("pegasus-bridge"), TEXT("horde"), 1535330139u, 2.8, 14.000000000000014, 0, 11.2,
			{ TEXT("Pegasus Bridge"), TEXT("180 × 200 m"), TEXT("up to 5 defenders"), TEXT("7 capture points"), TEXT("rivers"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@28,5#d94a3d 1@62,5#d94a3d 2@28,94#3d7bd9 3@62,94#3d7bd9"), TEXT("45,22 45,41 14,30 76,30 45,52 20,63 70,63") },
		{ TEXT("pegasus-bridge"), TEXT("annihilation"), 1535330139u, 2.8, 14.000000000000014, 0, 11.2,
			{ TEXT("Pegasus Bridge"), TEXT("180 × 200 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@28,5#f2ecd8 1@62,5#f2ecd8 2@28,94#f2ecd8 3@62,94#f2ecd8"), TEXT("45,22 45,41 14,30 76,30 45,52 20,63 70,63") },
		{ TEXT("pegasus-bridge"), TEXT("tutorial"), 1535330139u, 2.8, 14.000000000000014, 0, 11.2,
			{ TEXT("Pegasus Bridge"), TEXT("180 × 200 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@28,5#f2ecd8 1@62,5#f2ecd8 2@28,94#f2ecd8 3@62,94#f2ecd8"), TEXT("45,22 45,41 14,30 76,30 45,52 20,63 70,63") },
		{ TEXT("prokhorovka"), TEXT("conquest"), 4035426553u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("Prokhorovka"), TEXT("240 × 180 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rolling hills") },
			TEXT("0@6,24#f2ecd8 1@6,64#f2ecd8 2@113,65#f2ecd8 3@113,25#f2ecd8"), TEXT("59,44 20,43 99,46 50,12 69,77 38,76 81,13") },
		{ TEXT("prokhorovka"), TEXT("assault"), 4035426553u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("Prokhorovka"), TEXT("240 × 180 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("rolling hills") },
			TEXT("0@6,24#f2ecd8 1@6,64#f2ecd8 2@113,65#f2ecd8 3@113,25#f2ecd8"), TEXT("20,43 99,46 50,12 69,77 38,76 81,13") },
		{ TEXT("prokhorovka"), TEXT("horde"), 4035426553u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("Prokhorovka"), TEXT("240 × 180 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("rolling hills") },
			TEXT("0@6,24#f2ecd8 1@6,64#f2ecd8 2@113,65#f2ecd8 3@113,25#f2ecd8"), TEXT("20,43 99,46 50,12 69,77 38,76 81,13") },
		{ TEXT("prokhorovka"), TEXT("annihilation"), 4035426553u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("Prokhorovka"), TEXT("240 × 180 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("rolling hills") },
			TEXT("0@6,24#f2ecd8 1@6,64#f2ecd8 2@113,65#f2ecd8 3@113,25#f2ecd8"), TEXT("20,43 99,46 50,12 69,77 38,76 81,13") },
		{ TEXT("prokhorovka"), TEXT("tutorial"), 4035426553u, 2.3333333333333335, 0, 35, 9.333333333333334,
			{ TEXT("Prokhorovka"), TEXT("240 × 180 m"), TEXT("up to 4 players"), TEXT("7 capture points"), TEXT("rolling hills") },
			TEXT("0@6,24#f2ecd8 1@6,64#f2ecd8 2@113,65#f2ecd8 3@113,25#f2ecd8"), TEXT("59,44 20,43 99,46 50,12 69,77 38,76 81,13") },
		{ TEXT("river-towns"), TEXT("conquest"), 1939096196u, 3.5, 0, 0, 14,
			{ TEXT("River Towns"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("7 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,40 40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("river-towns"), TEXT("assault"), 1939096196u, 3.5, 0, 0, 14,
			{ TEXT("River Towns"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("river-towns"), TEXT("horde"), 1939096196u, 3.5, 0, 0, 14,
			{ TEXT("River Towns"), TEXT("160 × 160 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("river-towns"), TEXT("annihilation"), 1939096196u, 3.5, 0, 0, 14,
			{ TEXT("River Towns"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("river-towns"), TEXT("tutorial"), 1939096196u, 3.5, 0, 0, 14,
			{ TEXT("River Towns"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("7 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@40,6#f2ecd8 1@69,57#f2ecd8 2@11,57#f2ecd8"), TEXT("40,40 40,60 23,30 57,30 20,11 75,37 25,72") },
		{ TEXT("seawall-xl"), TEXT("conquest"), 1266400278u, 2, 10, 0, 8,
			{ TEXT("Seawall XL"), TEXT("260 × 280 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@30,26#f2ecd8 1@65,28#f2ecd8 2@100,26#f2ecd8 3@104,126#f2ecd8 4@65,128#f2ecd8 5@26,126#f2ecd8"), TEXT("65,26 24,56 65,56 106,56 40,100 90,100") },
		{ TEXT("seawall-xl"), TEXT("assault"), 1266400278u, 2, 10, 0, 8,
			{ TEXT("Seawall XL"), TEXT("260 × 280 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("● defend, ● attack") },
			TEXT("0@30,26#d94a3d 1@65,28#d94a3d 2@100,26#d94a3d 3@104,126#3d7bd9 4@65,128#3d7bd9 5@26,126#3d7bd9"), TEXT("65,26 24,56 65,56 106,56 40,100 90,100") },
		{ TEXT("seawall-xl"), TEXT("horde"), 1266400278u, 2, 10, 0, 8,
			{ TEXT("Seawall XL"), TEXT("260 × 280 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@30,26#d94a3d 1@65,28#d94a3d 2@100,26#d94a3d 3@104,126#3d7bd9 4@65,128#3d7bd9 5@26,126#3d7bd9"), TEXT("65,26 24,56 65,56 106,56 40,100 90,100") },
		{ TEXT("seawall-xl"), TEXT("annihilation"), 1266400278u, 2, 10, 0, 8,
			{ TEXT("Seawall XL"), TEXT("260 × 280 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@30,26#f2ecd8 1@65,28#f2ecd8 2@100,26#f2ecd8 3@104,126#f2ecd8 4@65,128#f2ecd8 5@26,126#f2ecd8"), TEXT("65,26 24,56 65,56 106,56 40,100 90,100") },
		{ TEXT("seawall-xl"), TEXT("tutorial"), 1266400278u, 2, 10, 0, 8,
			{ TEXT("Seawall XL"), TEXT("260 × 280 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@30,26#f2ecd8 1@65,28#f2ecd8 2@100,26#f2ecd8 3@104,126#f2ecd8 4@65,128#f2ecd8 5@26,126#f2ecd8"), TEXT("65,26 24,56 65,56 106,56 40,100 90,100") },
		{ TEXT("seawall"), TEXT("conquest"), 4178417154u, 3.111111111111111, 15.555555555555557, 0, 12.444444444444445,
			{ TEXT("Seawall"), TEXT("160 × 180 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@28,4#f2ecd8 1@52,4#f2ecd8 2@25,80#f2ecd8 3@55,80#f2ecd8"), TEXT("40,14 19,36 60,36 40,64 12,70 68,70") },
		{ TEXT("seawall"), TEXT("assault"), 4178417154u, 3.111111111111111, 15.555555555555557, 0, 12.444444444444445,
			{ TEXT("Seawall"), TEXT("160 × 180 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("● defend, ● attack") },
			TEXT("0@28,4#d94a3d 1@52,4#d94a3d 2@25,80#3d7bd9 3@55,80#3d7bd9"), TEXT("40,14 19,36 60,36 40,64 12,70 68,70") },
		{ TEXT("seawall"), TEXT("horde"), 4178417154u, 3.111111111111111, 15.555555555555557, 0, 12.444444444444445,
			{ TEXT("Seawall"), TEXT("160 × 180 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@28,4#d94a3d 1@52,4#d94a3d 2@25,80#3d7bd9 3@55,80#3d7bd9"), TEXT("40,14 19,36 60,36 40,64 12,70 68,70") },
		{ TEXT("seawall"), TEXT("annihilation"), 4178417154u, 3.111111111111111, 15.555555555555557, 0, 12.444444444444445,
			{ TEXT("Seawall"), TEXT("160 × 180 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@28,4#f2ecd8 1@52,4#f2ecd8 2@25,80#f2ecd8 3@55,80#f2ecd8"), TEXT("40,14 19,36 60,36 40,64 12,70 68,70") },
		{ TEXT("seawall"), TEXT("tutorial"), 4178417154u, 3.111111111111111, 15.555555555555557, 0, 12.444444444444445,
			{ TEXT("Seawall"), TEXT("160 × 180 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@28,4#f2ecd8 1@52,4#f2ecd8 2@25,80#f2ecd8 3@55,80#f2ecd8"), TEXT("40,14 19,36 60,36 40,64 12,70 68,70") },
		{ TEXT("seelow-heights"), TEXT("conquest"), 1825215391u, 2.3333333333333335, 11.666666666666657, 0, 9.333333333333334,
			{ TEXT("Seelow Heights"), TEXT("220 × 240 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@6,40#f2ecd8 1@6,80#f2ecd8 2@103,30#f2ecd8 3@103,90#f2ecd8"), TEXT("86,60 52,22 52,98 32,22 32,98 26,61") },
		{ TEXT("seelow-heights"), TEXT("assault"), 1825215391u, 2.3333333333333335, 11.666666666666657, 0, 9.333333333333334,
			{ TEXT("Seelow Heights"), TEXT("220 × 240 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("● defend, ● attack"), TEXT("30 minute clock") },
			TEXT("0@6,40#d94a3d 1@6,80#d94a3d 2@103,30#3d7bd9 3@103,90#3d7bd9"), TEXT("86,60 52,22 52,98 32,22 32,98 26,61") },
		{ TEXT("seelow-heights"), TEXT("horde"), 1825215391u, 2.3333333333333335, 11.666666666666657, 0, 9.333333333333334,
			{ TEXT("Seelow Heights"), TEXT("220 × 240 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@6,40#d94a3d 1@6,80#d94a3d 2@103,30#3d7bd9 3@103,90#3d7bd9"), TEXT("86,60 52,22 52,98 32,22 32,98 26,61") },
		{ TEXT("seelow-heights"), TEXT("annihilation"), 1825215391u, 2.3333333333333335, 11.666666666666657, 0, 9.333333333333334,
			{ TEXT("Seelow Heights"), TEXT("220 × 240 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@6,40#f2ecd8 1@6,80#f2ecd8 2@103,30#f2ecd8 3@103,90#f2ecd8"), TEXT("86,60 52,22 52,98 32,22 32,98 26,61") },
		{ TEXT("seelow-heights"), TEXT("tutorial"), 1825215391u, 2.3333333333333335, 11.666666666666657, 0, 9.333333333333334,
			{ TEXT("Seelow Heights"), TEXT("220 × 240 m"), TEXT("up to 4 players"), TEXT("6 capture points"), TEXT("hills and cliffs"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@6,40#f2ecd8 1@6,80#f2ecd8 2@103,30#f2ecd8 3@103,90#f2ecd8"), TEXT("86,60 52,22 52,98 32,22 32,98 26,61") },
		{ TEXT("six-fronts"), TEXT("conquest"), 3443375505u, 1.8666666666666667, 0, 0, 7.466666666666667,
			{ TEXT("Six Fronts"), TEXT("300 × 300 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("rolling hills") },
			TEXT("0@75,11#f2ecd8 1@130,43#f2ecd8 2@130,107#f2ecd8 3@75,139#f2ecd8 4@20,107#f2ecd8 5@20,43#f2ecd8"), TEXT("75,75 97,37 119,75 97,113 53,113 31,75 53,37") },
		{ TEXT("six-fronts"), TEXT("assault"), 3443375505u, 1.8666666666666667, 0, 0, 7.466666666666667,
			{ TEXT("Six Fronts"), TEXT("300 × 300 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("rolling hills") },
			TEXT("0@75,11#f2ecd8 1@130,43#f2ecd8 2@130,107#f2ecd8 3@75,139#f2ecd8 4@20,107#f2ecd8 5@20,43#f2ecd8"), TEXT("97,37 119,75 97,113 53,113 31,75 53,37") },
		{ TEXT("six-fronts"), TEXT("horde"), 3443375505u, 1.8666666666666667, 0, 0, 7.466666666666667,
			{ TEXT("Six Fronts"), TEXT("300 × 300 m"), TEXT("up to 5 defenders"), TEXT("6 capture points"), TEXT("rolling hills") },
			TEXT("0@75,11#f2ecd8 1@130,43#f2ecd8 2@130,107#f2ecd8 3@75,139#f2ecd8 4@20,107#f2ecd8 5@20,43#f2ecd8"), TEXT("97,37 119,75 97,113 53,113 31,75 53,37") },
		{ TEXT("six-fronts"), TEXT("annihilation"), 3443375505u, 1.8666666666666667, 0, 0, 7.466666666666667,
			{ TEXT("Six Fronts"), TEXT("300 × 300 m"), TEXT("up to 6 players"), TEXT("6 capture points"), TEXT("rolling hills") },
			TEXT("0@75,11#f2ecd8 1@130,43#f2ecd8 2@130,107#f2ecd8 3@75,139#f2ecd8 4@20,107#f2ecd8 5@20,43#f2ecd8"), TEXT("97,37 119,75 97,113 53,113 31,75 53,37") },
		{ TEXT("six-fronts"), TEXT("tutorial"), 3443375505u, 1.8666666666666667, 0, 0, 7.466666666666667,
			{ TEXT("Six Fronts"), TEXT("300 × 300 m"), TEXT("up to 6 players"), TEXT("7 capture points"), TEXT("rolling hills") },
			TEXT("0@75,11#f2ecd8 1@130,43#f2ecd8 2@130,107#f2ecd8 3@75,139#f2ecd8 4@20,107#f2ecd8 5@20,43#f2ecd8"), TEXT("75,75 97,37 119,75 97,113 53,113 31,75 53,37") },
		{ TEXT("stalingrad-factory"), TEXT("conquest"), 824518029u, 2.8, 0, 0, 11.2,
			{ TEXT("Stalingrad Factory"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("5 capture points"), TEXT("built for Assault") },
			TEXT("1@50,1#f2ecd8 2@98,50#f2ecd8 3@50,98#f2ecd8 4@1,50#f2ecd8"), TEXT("50,50 30,30 69,30 30,69 69,69") },
		{ TEXT("stalingrad-factory"), TEXT("assault"), 824518029u, 2.8, 0, 0, 11.2,
			{ TEXT("Stalingrad Factory"), TEXT("200 × 200 m"), TEXT("up to 5 players"), TEXT("5 capture points"), TEXT("● defend, ● attack") },
			TEXT("0@50,58#d94a3d 1@50,1#3d7bd9 2@98,50#3d7bd9 3@50,98#3d7bd9 4@1,50#3d7bd9"), TEXT("50,50 30,30 69,30 30,69 69,69") },
		{ TEXT("stalingrad-factory"), TEXT("horde"), 824518029u, 2.8, 0, 0, 11.2,
			{ TEXT("Stalingrad Factory"), TEXT("200 × 200 m"), TEXT("up to 5 defenders"), TEXT("5 capture points"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("1@50,1#3d7bd9 2@98,50#3d7bd9 3@50,98#3d7bd9 4@1,50#3d7bd9"), TEXT("50,50 30,30 69,30 30,69 69,69") },
		{ TEXT("stalingrad-factory"), TEXT("annihilation"), 824518029u, 2.8, 0, 0, 11.2,
			{ TEXT("Stalingrad Factory"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("5 capture points"), TEXT("built for Assault") },
			TEXT("1@50,1#f2ecd8 2@98,50#f2ecd8 3@50,98#f2ecd8 4@1,50#f2ecd8"), TEXT("50,50 30,30 69,30 30,69 69,69") },
		{ TEXT("stalingrad-factory"), TEXT("tutorial"), 824518029u, 2.8, 0, 0, 11.2,
			{ TEXT("Stalingrad Factory"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("5 capture points"), TEXT("built for Assault") },
			TEXT("1@50,1#f2ecd8 2@98,50#f2ecd8 3@50,98#f2ecd8 4@1,50#f2ecd8"), TEXT("50,50 30,30 69,30 30,69 69,69") },
		{ TEXT("test-arena"), TEXT("conquest"), 2336060174u, 3.5, 0, 0, 14,
			{ TEXT("Test Arena"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("2 capture points") },
			TEXT("0@40,8#f2ecd8 1@68,56#f2ecd8 2@12,56#f2ecd8"), TEXT("40,40 41,32") },
		{ TEXT("test-arena"), TEXT("assault"), 2336060174u, 3.5, 0, 0, 14,
			{ TEXT("Test Arena"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("2 capture points") },
			TEXT("0@40,8#f2ecd8 1@68,56#f2ecd8 2@12,56#f2ecd8"), TEXT("40,40 41,32") },
		{ TEXT("test-arena"), TEXT("horde"), 2336060174u, 3.5, 0, 0, 14,
			{ TEXT("Test Arena"), TEXT("160 × 160 m"), TEXT("up to 5 defenders"), TEXT("2 capture points") },
			TEXT("0@40,8#f2ecd8 1@68,56#f2ecd8 2@12,56#f2ecd8"), TEXT("40,40 41,32") },
		{ TEXT("test-arena"), TEXT("annihilation"), 2336060174u, 3.5, 0, 0, 14,
			{ TEXT("Test Arena"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("2 capture points") },
			TEXT("0@40,8#f2ecd8 1@68,56#f2ecd8 2@12,56#f2ecd8"), TEXT("40,40 41,32") },
		{ TEXT("test-arena"), TEXT("tutorial"), 2336060174u, 3.5, 0, 0, 14,
			{ TEXT("Test Arena"), TEXT("160 × 160 m"), TEXT("up to 3 players"), TEXT("2 capture points") },
			TEXT("0@40,8#f2ecd8 1@68,56#f2ecd8 2@12,56#f2ecd8"), TEXT("40,40 41,32") },
		{ TEXT("the-great-bridge"), TEXT("conquest"), 2511679036u, 2.3333333333333335, 0, 23.33333333333333, 9.333333333333334,
			{ TEXT("The Great Bridge"), TEXT("240 × 200 m"), TEXT("up to 4 players"), TEXT("5 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@5,30#f2ecd8 1@5,70#f2ecd8 2@102,64#f2ecd8 3@102,34#f2ecd8"), TEXT("42,49 72,49 90,49 80,12 80,87") },
		{ TEXT("the-great-bridge"), TEXT("assault"), 2511679036u, 2.3333333333333335, 0, 23.33333333333333, 9.333333333333334,
			{ TEXT("The Great Bridge"), TEXT("240 × 200 m"), TEXT("up to 4 players"), TEXT("5 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("● defend, ● attack"), TEXT("25 minute clock") },
			TEXT("0@5,30#3d7bd9 1@5,70#3d7bd9 2@102,64#d94a3d 3@102,34#d94a3d"), TEXT("42,49 72,49 90,49 80,12 80,87") },
		{ TEXT("the-great-bridge"), TEXT("horde"), 2511679036u, 2.3333333333333335, 0, 23.33333333333333, 9.333333333333334,
			{ TEXT("The Great Bridge"), TEXT("240 × 200 m"), TEXT("up to 5 defenders"), TEXT("5 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("● your HQ is one of these, ● the horde comes from here") },
			TEXT("0@5,30#3d7bd9 1@5,70#3d7bd9 2@102,64#d94a3d 3@102,34#d94a3d"), TEXT("42,49 72,49 90,49 80,12 80,87") },
		{ TEXT("the-great-bridge"), TEXT("annihilation"), 2511679036u, 2.3333333333333335, 0, 23.33333333333333, 9.333333333333334,
			{ TEXT("The Great Bridge"), TEXT("240 × 200 m"), TEXT("up to 4 players"), TEXT("5 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@5,30#f2ecd8 1@5,70#f2ecd8 2@102,64#f2ecd8 3@102,34#f2ecd8"), TEXT("42,49 72,49 90,49 80,12 80,87") },
		{ TEXT("the-great-bridge"), TEXT("tutorial"), 2511679036u, 2.3333333333333335, 0, 23.33333333333333, 9.333333333333334,
			{ TEXT("The Great Bridge"), TEXT("240 × 200 m"), TEXT("up to 4 players"), TEXT("5 capture points"), TEXT("rolling hills"), TEXT("rivers"), TEXT("built for Assault") },
			TEXT("0@5,30#f2ecd8 1@5,70#f2ecd8 2@102,64#f2ecd8 3@102,34#f2ecd8"), TEXT("42,49 72,49 90,49 80,12 80,87") },
		{ TEXT("the-polder"), TEXT("conquest"), 2110704137u, 2.8, 0, 0, 11.2,
			{ TEXT("The Polder"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("9 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@8,8#f2ecd8 1@91,8#f2ecd8 2@91,91#f2ecd8 3@8,91#f2ecd8"), TEXT("50,50 25,25 74,74 74,25 25,74 6,34 93,65 93,34 6,65") },
		{ TEXT("the-polder"), TEXT("assault"), 2110704137u, 2.8, 0, 0, 11.2,
			{ TEXT("The Polder"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("8 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@8,8#f2ecd8 1@91,8#f2ecd8 2@91,91#f2ecd8 3@8,91#f2ecd8"), TEXT("25,25 74,74 74,25 25,74 6,34 93,65 93,34 6,65") },
		{ TEXT("the-polder"), TEXT("horde"), 2110704137u, 2.8, 0, 0, 11.2,
			{ TEXT("The Polder"), TEXT("200 × 200 m"), TEXT("up to 5 defenders"), TEXT("8 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@8,8#f2ecd8 1@91,8#f2ecd8 2@91,91#f2ecd8 3@8,91#f2ecd8"), TEXT("25,25 74,74 74,25 25,74 6,34 93,65 93,34 6,65") },
		{ TEXT("the-polder"), TEXT("annihilation"), 2110704137u, 2.8, 0, 0, 11.2,
			{ TEXT("The Polder"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("8 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@8,8#f2ecd8 1@91,8#f2ecd8 2@91,91#f2ecd8 3@8,91#f2ecd8"), TEXT("25,25 74,74 74,25 25,74 6,34 93,65 93,34 6,65") },
		{ TEXT("the-polder"), TEXT("tutorial"), 2110704137u, 2.8, 0, 0, 11.2,
			{ TEXT("The Polder"), TEXT("200 × 200 m"), TEXT("up to 4 players"), TEXT("9 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@8,8#f2ecd8 1@91,8#f2ecd8 2@91,91#f2ecd8 3@8,91#f2ecd8"), TEXT("50,50 25,25 74,74 74,25 25,74 6,34 93,65 93,34 6,65") },
		{ TEXT("three-islands"), TEXT("conquest"), 199467681u, 0.546875, 0, 0, 4,
			{ TEXT("Three Islands"), TEXT("1024 × 1024 m"), TEXT("up to 6 players"), TEXT("4 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@285,453#f2ecd8 1@227,453#f2ecd8 2@71,183#f2ecd8 3@100,132#f2ecd8 4@412,132#f2ecd8 5@441,183#f2ecd8"), TEXT("256,256 256,410 123,179 389,179") },
		{ TEXT("three-islands"), TEXT("assault"), 199467681u, 0.546875, 0, 0, 4,
			{ TEXT("Three Islands"), TEXT("1024 × 1024 m"), TEXT("up to 6 players"), TEXT("3 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@285,453#f2ecd8 1@227,453#f2ecd8 2@71,183#f2ecd8 3@100,132#f2ecd8 4@412,132#f2ecd8 5@441,183#f2ecd8"), TEXT("256,410 123,179 389,179") },
		{ TEXT("three-islands"), TEXT("horde"), 199467681u, 0.546875, 0, 0, 4,
			{ TEXT("Three Islands"), TEXT("1024 × 1024 m"), TEXT("up to 5 defenders"), TEXT("3 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@285,453#f2ecd8 1@227,453#f2ecd8 2@71,183#f2ecd8 3@100,132#f2ecd8 4@412,132#f2ecd8 5@441,183#f2ecd8"), TEXT("256,410 123,179 389,179") },
		{ TEXT("three-islands"), TEXT("annihilation"), 199467681u, 0.546875, 0, 0, 4,
			{ TEXT("Three Islands"), TEXT("1024 × 1024 m"), TEXT("up to 6 players"), TEXT("3 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@285,453#f2ecd8 1@227,453#f2ecd8 2@71,183#f2ecd8 3@100,132#f2ecd8 4@412,132#f2ecd8 5@441,183#f2ecd8"), TEXT("256,410 123,179 389,179") },
		{ TEXT("three-islands"), TEXT("tutorial"), 199467681u, 0.546875, 0, 0, 4,
			{ TEXT("Three Islands"), TEXT("1024 × 1024 m"), TEXT("up to 6 players"), TEXT("4 capture points"), TEXT("rolling hills"), TEXT("rivers") },
			TEXT("0@285,453#f2ecd8 1@227,453#f2ecd8 2@71,183#f2ecd8 3@100,132#f2ecd8 4@412,132#f2ecd8 5@441,183#f2ecd8"), TEXT("256,256 256,410 123,179 389,179") },
		{ TEXT("tutorial"), TEXT("conquest"), 1388465022u, 3.888888888888889, 0, 54.44444444444444, 15.555555555555555,
			{ TEXT("Tutorial: The Road to the Bridge"), TEXT("144 × 88 m"), TEXT("up to 2 players"), TEXT("2 capture points"), TEXT("rivers") },
			TEXT("0@5,21#f2ecd8 1@68,38#f2ecd8"), TEXT("34,22 62,22") },
		{ TEXT("tutorial"), TEXT("assault"), 1388465022u, 3.888888888888889, 0, 54.44444444444444, 15.555555555555555,
			{ TEXT("Tutorial: The Road to the Bridge"), TEXT("144 × 88 m"), TEXT("up to 2 players"), TEXT("2 capture points"), TEXT("rivers") },
			TEXT("0@5,21#f2ecd8 1@68,38#f2ecd8"), TEXT("34,22 62,22") },
		{ TEXT("tutorial"), TEXT("horde"), 1388465022u, 3.888888888888889, 0, 54.44444444444444, 15.555555555555555,
			{ TEXT("Tutorial: The Road to the Bridge"), TEXT("144 × 88 m"), TEXT("up to 5 defenders"), TEXT("2 capture points"), TEXT("rivers") },
			TEXT("0@5,21#f2ecd8 1@68,38#f2ecd8"), TEXT("34,22 62,22") },
		{ TEXT("tutorial"), TEXT("annihilation"), 1388465022u, 3.888888888888889, 0, 54.44444444444444, 15.555555555555555,
			{ TEXT("Tutorial: The Road to the Bridge"), TEXT("144 × 88 m"), TEXT("up to 2 players"), TEXT("2 capture points"), TEXT("rivers") },
			TEXT("0@5,21#f2ecd8 1@68,38#f2ecd8"), TEXT("34,22 62,22") },
		{ TEXT("tutorial"), TEXT("tutorial"), 1388465022u, 3.888888888888889, 0, 54.44444444444444, 15.555555555555555,
			{ TEXT("Tutorial: The Road to the Bridge"), TEXT("144 × 88 m"), TEXT("up to 2 players"), TEXT("2 capture points"), TEXT("rivers") },
			TEXT("0@5,21#f2ecd8 1@68,38#f2ecd8"), TEXT("34,22 62,22") },
		{ TEXT("twin-valleys"), TEXT("conquest"), 2127492249u, 2.8, 0, 28, 11.2,
			{ TEXT("Twin Valleys"), TEXT("200 × 160 m"), TEXT("up to 4 players"), TEXT("9 capture points"), TEXT("hills and cliffs") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,73#f2ecd8 3@20,73#f2ecd8"), TEXT("50,40 50,20 49,59 20,40 79,39 7,25 92,54 66,25 33,54") },
		{ TEXT("twin-valleys"), TEXT("assault"), 2127492249u, 2.8, 0, 28, 11.2,
			{ TEXT("Twin Valleys"), TEXT("200 × 160 m"), TEXT("up to 4 players"), TEXT("8 capture points"), TEXT("hills and cliffs") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,73#f2ecd8 3@20,73#f2ecd8"), TEXT("50,20 49,59 20,40 79,39 7,25 92,54 66,25 33,54") },
		{ TEXT("twin-valleys"), TEXT("horde"), 2127492249u, 2.8, 0, 28, 11.2,
			{ TEXT("Twin Valleys"), TEXT("200 × 160 m"), TEXT("up to 5 defenders"), TEXT("8 capture points"), TEXT("hills and cliffs") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,73#f2ecd8 3@20,73#f2ecd8"), TEXT("50,20 49,59 20,40 79,39 7,25 92,54 66,25 33,54") },
		{ TEXT("twin-valleys"), TEXT("annihilation"), 2127492249u, 2.8, 0, 28, 11.2,
			{ TEXT("Twin Valleys"), TEXT("200 × 160 m"), TEXT("up to 4 players"), TEXT("8 capture points"), TEXT("hills and cliffs") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,73#f2ecd8 3@20,73#f2ecd8"), TEXT("50,20 49,59 20,40 79,39 7,25 92,54 66,25 33,54") },
		{ TEXT("twin-valleys"), TEXT("tutorial"), 2127492249u, 2.8, 0, 28, 11.2,
			{ TEXT("Twin Valleys"), TEXT("200 × 160 m"), TEXT("up to 4 players"), TEXT("9 capture points"), TEXT("hills and cliffs") },
			TEXT("0@20,6#f2ecd8 1@79,6#f2ecd8 2@79,73#f2ecd8 3@20,73#f2ecd8"), TEXT("50,40 50,20 49,59 20,40 79,39 7,25 92,54 66,25 33,54") },
	};
	// weather-view.js and shared/weather.js run in node
	const TCHAR* const ChoiceLabels[][2] = { { TEXT("map"), TEXT("Map default") }, { TEXT("clear"), TEXT("Clear") }, { TEXT("fog"), TEXT("Ground fog") }, { TEXT("rain"), TEXT("Rain") }, { TEXT("mud"), TEXT("Mud") }, { TEXT("snow"), TEXT("Snow") }, { TEXT("random"), TEXT("Random") } };
	const TCHAR* const Explained[][2] = { { TEXT("map"), TEXT("The weather this map brings") },
		{ TEXT("clear"), TEXT("Clear: no effect. Showers come and go") },
		{ TEXT("fog"), TEXT("Ground fog: sight -30%. Showers come and go") },
		{ TEXT("rain"), TEXT("Rain: sight -20%, vehicles -20% off roads") },
		{ TEXT("mud"), TEXT("Mud: infantry -10%, vehicles -20% off roads. Showers come and go") },
		{ TEXT("snow"), TEXT("Snow: sight -10%, infantry -10%, vehicles -15%") },
		{ TEXT("random"), TEXT("Picked when the match starts. Fog may lift or rain may turn to mud partway through") } };
	const TCHAR* const Said[][2] = { { TEXT("clear"), TEXT("Clear: no effect") },
		{ TEXT("fog"), TEXT("Ground fog: sight -30%") },
		{ TEXT("rain"), TEXT("Rain: sight -20%, vehicles -20% off roads") },
		{ TEXT("mud"), TEXT("Mud: infantry -10%, vehicles -20% off roads") },
		{ TEXT("snow"), TEXT("Snow: sight -10%, infantry -10%, vehicles -15%") } };
	// mapWeather(map, key): name, key, mood, weather field, then now, next, at, the Map default option, its tooltip
	const FExpectedMapWeather MapWeathers[] = {
		{ TEXT("Bastogne"), TEXT("bastogne"), TEXT(""), TEXT(""), TEXT("snow"), TEXT(""), 0, TEXT("Map default: Snow"), TEXT("Snow: sight -10%, infantry -10%, vehicles -15%") },
		{ TEXT("Pegasus Bridge"), TEXT("pegasus-bridge"), TEXT(""), TEXT(""), TEXT("fog"), TEXT("clear"), 240, TEXT("Map default: Fog until 4:00"), TEXT("Ground fog: sight -30%. Lifts at 4:00. Showers come and go") },
		{ TEXT("El Alamein"), TEXT("el-alamein"), TEXT(""), TEXT(""), TEXT("clear"), TEXT(""), 0, TEXT("Map default: Clear"), TEXT("Clear: no effect. Showers come and go") },
		{ TEXT("Monte Cassino XL"), TEXT("monte-cassino-xl"), TEXT(""), TEXT(""), TEXT("clear"), TEXT(""), 0, TEXT("Map default: Clear"), TEXT("Clear: no effect. Showers come and go") },
		{ TEXT("Three Crossroads"), TEXT("default"), TEXT(""), TEXT(""), TEXT("clear"), TEXT(""), 0, TEXT("Map default: Clear"), TEXT("Clear: no effect. Showers come and go") },
		{ TEXT("Tutorial: The Road to the Bridge"), TEXT("tutorial"), TEXT(""), TEXT(""), TEXT("clear"), TEXT(""), 0, TEXT("Map default: Clear"), TEXT("Clear: no effect. Showers come and go") },
		{ TEXT("X"), TEXT("x"), TEXT("dust"), TEXT(""), TEXT("clear"), TEXT(""), 0, TEXT("Map default: Clear"), TEXT("Clear: no effect. Showers come and go") },
		{ TEXT("Snowfield"), TEXT("snowfield"), TEXT("warm"), TEXT(""), TEXT("clear"), TEXT(""), 0, TEXT("Map default: Clear"), TEXT("Clear: no effect. Showers come and go") },
		{ TEXT("Bastogne"), TEXT("bastogne"), TEXT(""), TEXT("rain"), TEXT("rain"), TEXT(""), 0, TEXT("Map default: Rain"), TEXT("Rain: sight -20%, vehicles -20% off roads") },
		{ TEXT("Bastogne"), TEXT("bastogne"), TEXT(""), TEXT("__proto__"), TEXT("snow"), TEXT(""), 0, TEXT("Map default: Snow"), TEXT("Snow: sight -10%, infantry -10%, vehicles -15%") },
		{ TEXT("Bastogne"), TEXT("bastogne"), TEXT("nope"), TEXT(""), TEXT("snow"), TEXT(""), 0, TEXT("Map default: Snow"), TEXT("Snow: sight -10%, infantry -10%, vehicles -15%") },
		{ TEXT("WINTER LINE"), TEXT("wl"), TEXT(""), TEXT(""), TEXT("snow"), TEXT(""), 0, TEXT("Map default: Snow"), TEXT("Snow: sight -10%, infantry -10%, vehicles -15%") },
	};


	TSharedPtr<FJsonObject> Parse(const FString& Src)
	{
		TSharedPtr<FJsonObject> Obj;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Src), Obj);
		return Obj;
	}
	// The message as FTCConnection::TrySend writes it (condensed)
	FString Wire(const FTCOptionRequest& R)
	{
		if (!R.Message.IsValid()) return TEXT("(nothing sent)");
		FString Out;
		TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Out);
		FJsonSerializer::Serialize(R.Message.ToSharedRef(), Writer);
		return Out;
	}
	FString Hex(const FColor& C) { return FString::Printf(TEXT("#%02x%02x%02x"), C.R, C.G, C.B); }
	FString SpawnsText(const FTCMapPreview& P)
	{
		TArray<FString> Parts;
		for (const FTCMapPreviewSpawn& S : P.Spawns) Parts.Add(FString::Printf(TEXT("%d@%d,%d%s"), S.Index, S.X, S.Y, *Hex(S.Color)));
		return FString::Join(Parts, TEXT(" "));
	}
	FString PointsText(const FTCMapPreview& P)
	{
		TArray<FString> Parts;
		for (const FTCMapPreviewPoint& Pt : P.Points) Parts.Add(FString::Printf(TEXT("%d,%d"), Pt.X, Pt.Y));
		return FString::Join(Parts, TEXT(" "));
	}
	// FNV-1a over the RGBA bytes, row by row (as gen.mjs hashes the browser's ImageData)
	uint32 Fnv(const FTCMapPreview& P)
	{
		uint32 H = 0x811c9dc5u;
		for (const FColor& C : P.Pixels)
			for (const uint8 B : { C.R, C.G, C.B, C.A }) { H ^= B; H *= 0x01000193u; }
		return H;
	}
}

// Run (after a build): -ExecCmds="Automation RunTests ThreeCrossroads.MatchOptions; Quit"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMatchOptionsLobbyTest, "ThreeCrossroads.MatchOptions.Lobby", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCMatchOptionsLobbyTest::RunTest(const FString& Parameters)
{
	auto Load = [this](FTCMatchOptions& O, const FString& Src, const TCHAR* What)
	{
		const TSharedPtr<FJsonObject> Obj = TCMOptT::Parse(Src);
		TestTrue(FString::Printf(TEXT("%s parses"), What), Obj.IsValid() && O.ApplyLobby(*Obj));
	};
	auto Sends = [this](const FTCOptionRequest& R, const TCHAR* Expected) { TestEqual(FString::Printf(TEXT("sends %s (%s)"), Expected, *R.Note), TCMOptT::Wire(R), FString(Expected)); };
	auto Holds = [this](const FTCOptionRequest& R, const TCHAR* What)
	{
		TestFalse(FString::Printf(TEXT("%s: nothing sent (%s)"), What, *R.Note), R.Message.IsValid());
		TestFalse(FString::Printf(TEXT("%s: says why"), What), R.Note.IsEmpty());
	};

	// Weather text: weather-view.js and shared/weather.js, run in node
	TestEqual(TEXT("seven weather choices"), FTCMatchOptions::WeatherChoices().Num(), (int32)UE_ARRAY_COUNT(TCMOptT::ChoiceLabels));
	for (int32 I = 0; I < UE_ARRAY_COUNT(TCMOptT::ChoiceLabels); I++)
	{
		TestEqual(TEXT("choice order"), FTCMatchOptions::WeatherChoices().IsValidIndex(I) ? FTCMatchOptions::WeatherChoices()[I] : FString(), FString(TCMOptT::ChoiceLabels[I][0]));
		TestEqual(TEXT("choice label"), FTCMatchOptions::WeatherChoiceLabel(TCMOptT::ChoiceLabels[I][0]), FString(TCMOptT::ChoiceLabels[I][1]));
	}
	for (const auto& E : TCMOptT::Explained) TestEqual(FString::Printf(TEXT("tooltip %s"), E[0]), FTCMatchOptions::WeatherExplain(E[0], nullptr), FString(E[1]));
	for (const auto& E : TCMOptT::Said) TestEqual(FString::Printf(TEXT("said %s"), E[0]), FTCMatchOptions::WeatherSaid(E[0]), FString(E[1]));
	for (const TCMOptT::FExpectedMapWeather& E : TCMOptT::MapWeathers)
	{
		const FTCWeatherPlan Plan = FTCMatchOptions::MapWeather(E.Name, E.Key, E.Mood, E.Weather);
		const FString What = FString::Printf(TEXT("map weather of '%s' (%s)"), E.Name, E.Key);
		TestTrue(What, Plan.Now == E.Now && Plan.Next == E.Next && Plan.AtSeconds == E.At);
		TestEqual(What + TEXT(": Map default option"), FTCMatchOptions::MapDefaultLabel(&Plan), FString(E.Label));
		TestEqual(What + TEXT(": tooltip"), FTCMatchOptions::WeatherExplain(TEXT("map"), &Plan), FString(E.Tooltip));
	}

	// The host: weather fog, sandbox on; seats Host, Guest (both connected), AI 1, AI 2
	{
		FTCMatchOptions O;
		TestFalse(TEXT("nothing shown before a lobby"), O.Control(ETCMatchOption::Weather).bShown);
		Holds(O.PlanWeather(TEXT("fog")), TEXT("weather before a lobby"));
		Load(O, TCMOptT::HostFog, TEXT("host"));
		const FTCOptionControl W = O.Control(ETCMatchOption::Weather);
		TestTrue(TEXT("host: weather shown and editable"), W.bShown && W.bEditable);
		TestEqual(TEXT("weather value"), W.CurrentValue, FString(TEXT("fog")));
		TestEqual(TEXT("weather label"), W.CurrentLabel, FString(TEXT("Ground fog")));
		TestEqual(TEXT("weather tooltip"), W.Tooltip, FString(TEXT("Ground fog: sight -30%. Showers come and go")));
		TestEqual(TEXT("seven options"), W.Choices.Num(), 7);
		TestEqual(TEXT("Map default before the map's file"), W.Choices.Num() ? W.Choices[0].Label : FString(), FString(TEXT("Map default")));
		const FTCOptionControl Sb = O.Control(ETCMatchOption::Sandbox);
		TestTrue(TEXT("host: sandbox shown, editable, checked"), Sb.bShown && Sb.bEditable && Sb.bChecked);
		const TArray<FTCOptionControl> Kicks = O.KickControls();
		TestEqual(TEXT("a kick control per seat"), Kicks.Num(), 4);
		if (Kicks.Num() == 4)
		{
			TestFalse(TEXT("no kick for the host's own seat"), Kicks[0].bShown);
			TestFalse(TEXT("no kick for a connected guest"), Kicks[1].bShown);
			TestTrue(TEXT("kick AI 1"), Kicks[2].bShown && Kicks[2].bEditable && Kicks[2].bSeatAi && Kicks[2].SeatName == TEXT("AI 1"));
			TestEqual(TEXT("kick title"), Kicks[2].Tooltip, FString(TEXT("Remove AI")));
			TestTrue(TEXT("kick AI 2"), Kicks[3].bShown && Kicks[3].bEditable);
		}
		Sends(O.PlanWeather(TEXT("snow")), TEXT("{\"t\":\"weather\",\"v\":\"snow\"}"));
		Sends(O.PlanWeather(TEXT("map")), TEXT("{\"t\":\"weather\",\"v\":\"map\"}"));
		Sends(O.PlanWeatherStep(1), TEXT("{\"t\":\"weather\",\"v\":\"rain\"}"));
		Sends(O.PlanWeatherStep(-1), TEXT("{\"t\":\"weather\",\"v\":\"clear\"}"));
		Holds(O.PlanWeather(TEXT("fog")), TEXT("weather already fog"));
		Holds(O.PlanWeather(TEXT("Fog")), TEXT("weather 'Fog' (the server drops it)"));
		Holds(O.PlanWeather(TEXT("storm")), TEXT("weather 'storm' (the server drops it)"));
		Sends(O.PlanSandbox(false), TEXT("{\"t\":\"sandbox\",\"v\":false}"));
		Sends(O.PlanSandboxToggle(), TEXT("{\"t\":\"sandbox\",\"v\":false}"));
		Holds(O.PlanSandbox(true), TEXT("sandbox already on"));
		Sends(O.PlanKick(2), TEXT("{\"t\":\"kick\",\"slot\":2}"));
		Holds(O.PlanKick(0), TEXT("kick the host's own seat (the server drops it)"));
		Holds(O.PlanKick(1), TEXT("kick a connected guest (the server drops it)"));
		Holds(O.PlanKick(9), TEXT("kick slot 9 (the server drops it)"));

		// pending: a weather sent stays pending through a lobby that does not show it, and resolves on one that does
		const FTCOptionRequest Snow = O.PlanWeather(TEXT("snow"));
		O.NoteSent(Snow, 10.0);
		TestTrue(TEXT("weather pending"), O.Control(ETCMatchOption::Weather).bPending && O.Control(ETCMatchOption::Weather).PendingValue == TEXT("snow"));
		TestEqual(TEXT("tooltip follows the asked value"), O.Control(ETCMatchOption::Weather).Tooltip, FString(TEXT("Snow: sight -10%, infantry -10%, vehicles -15%")));
		Holds(O.PlanWeather(TEXT("snow")), TEXT("snow already asked"));
		Sends(O.PlanWeather(TEXT("fog")), TEXT("{\"t\":\"weather\",\"v\":\"fog\"}")); // back to the shown value is a change
		Load(O, TCMOptT::HostFog, TEXT("host again"));
		TestEqual(TEXT("still pending"), O.PendingCount(), 1);
		// (this one lobby is HostFog with only the weather changed, as the server sent it after { t: 'weather', v: 'snow' })
		Load(O, FString(TCMOptT::HostFog).Replace(TEXT("\"weather\":\"fog\""), TEXT("\"weather\":\"snow\"")), TEXT("host, snow"));
		TestTrue(TEXT("snow confirmed"), O.PendingCount() == 0 && O.Confirmed() == 1);

		// kick: pending until the seat is gone; no second kick meanwhile (seat numbers may move)
		const FTCOptionRequest Kick = O.PlanKick(2);
		O.NoteSent(Kick, 11.0);
		TestTrue(TEXT("kick pending on AI 1"), O.Control(ETCMatchOption::Kick, 2).bPending && !O.Control(ETCMatchOption::Kick, 2).bEditable);
		TestFalse(TEXT("AI 2 waits for the lobby"), O.Control(ETCMatchOption::Kick, 3).bEditable);
		Holds(O.PlanKick(3), TEXT("a second kick while one is pending"));
		Load(O, TCMOptT::HostGuestOffline, TEXT("host, AI 1 gone, guest offline"));
		TestTrue(TEXT("kick confirmed"), O.PendingCount() == 0 && O.Confirmed() == 2);
		const TArray<FTCOptionControl> After = O.KickControls();
		TestEqual(TEXT("three seats left"), After.Num(), 3);
		if (After.Num() == 3)
		{
			TestFalse(TEXT("still no kick for the host"), After[0].bShown);
			TestTrue(TEXT("kick the offline guest"), After[1].bShown && After[1].bEditable && !After[1].bSeatAi);
			TestEqual(TEXT("offline kick title"), After[1].Tooltip, FString(TEXT("Remove offline player")));
			TestTrue(TEXT("kick AI 2, now seat 3"), After[2].bShown && After[2].bEditable && After[2].SeatName == TEXT("AI 2"));
		}
		Sends(O.PlanKick(1), TEXT("{\"t\":\"kick\",\"slot\":1}"));

		// a socket reopening forgets what was asked; a match starting does too
		O.NoteSent(O.PlanSandbox(false), 12.0);
		TestEqual(TEXT("sandbox pending"), O.PendingCount(), 1);
		O.OnSocketOpened();
		TestEqual(TEXT("forgotten on a new socket"), O.PendingCount(), 0);
		O.NoteSent(O.PlanWeather(TEXT("rain")), 13.0);
		Load(O, TCMOptT::WatcherHostsPlay, TEXT("a match in play"));
		TestEqual(TEXT("forgotten when a match starts"), O.PendingCount(), 0);
	}

	// A guest and a spectator who is not host: everything shown, nothing editable, no kick buttons
	for (const TCHAR* Src : { TCMOptT::GuestFog, TCMOptT::WatcherFog, TCMOptT::FormerHostWatching })
	{
		FTCMatchOptions O;
		Load(O, Src, TEXT("not host"));
		const FTCOptionControl W = O.Control(ETCMatchOption::Weather), Sb = O.Control(ETCMatchOption::Sandbox);
		TestTrue(TEXT("not host: weather and sandbox shown, locked"), W.bShown && !W.bEditable && Sb.bShown && !Sb.bEditable && !W.Why.IsEmpty());
		for (const FTCOptionControl& K : O.KickControls()) TestFalse(TEXT("not host: no kick button"), K.bShown);
		Holds(O.PlanWeather(TEXT("snow")), TEXT("not host: weather"));
		Holds(O.PlanSandboxToggle(), TEXT("not host: sandbox"));
		Holds(O.PlanKick(0), TEXT("not host: kick"));
	}

	// A spectator hosting a room of AI seats: the host's controls
	{
		FTCMatchOptions O;
		Load(O, TCMOptT::WatcherHostsAi, TEXT("spectator host"));
		TestTrue(TEXT("spectator host: weather editable"), O.Control(ETCMatchOption::Weather).bEditable);
		Sends(O.PlanWeather(TEXT("snow")), TEXT("{\"t\":\"weather\",\"v\":\"snow\"}"));
		Sends(O.PlanSandbox(false), TEXT("{\"t\":\"sandbox\",\"v\":false}"));
		Sends(O.PlanKick(0), TEXT("{\"t\":\"kick\",\"slot\":0}"));
	}

	// During play: nothing changes, not even for the host
	{
		FTCMatchOptions O;
		Load(O, TCMOptT::WatcherHostsPlay, TEXT("host in play"));
		TestFalse(TEXT("play: weather locked"), O.Control(ETCMatchOption::Weather).bEditable);
		TestTrue(TEXT("play: sandbox shown checked, locked"), O.Control(ETCMatchOption::Sandbox).bChecked && !O.Control(ETCMatchOption::Sandbox).bEditable);
		TestFalse(TEXT("play: no kick button"), O.Control(ETCMatchOption::Kick, 0).bShown);
		Holds(O.PlanWeather(TEXT("clear")), TEXT("play: weather"));
		Holds(O.PlanSandbox(false), TEXT("play: sandbox"));
		Holds(O.PlanKick(0), TEXT("play: kick"));
	}

	// An operative: its messages never reach these handlers
	{
		FTCMatchOptions O;
		Load(O, FString(TCMOptT::HostFog).Replace(TEXT("\"role\":\"commander\""), TEXT("\"role\":\"operative\"")), TEXT("operative (HostFog with the role changed)"));
		TestFalse(TEXT("operative: weather locked"), O.Control(ETCMatchOption::Weather).bEditable);
		Holds(O.PlanKick(2), TEXT("operative: kick"));
	}
	{
		FTCMatchOptions O;
		TestFalse(TEXT("not a lobby"), O.ApplyLobby(*TCMOptT::Parse(TEXT("{\"t\":\"start\"}"))));
		TestFalse(TEXT("a seat that is not an object"), O.ApplyLobby(*TCMOptT::Parse(TEXT("{\"t\":\"lobby\",\"state\":\"lobby\",\"players\":[1]}"))));
		TestFalse(TEXT("nothing kept from a bad lobby"), O.HasLobby());
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMatchOptionsPreviewTest, "ThreeCrossroads.MatchOptions.Preview", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCMatchOptionsPreviewTest::RunTest(const FString& Parameters)
{
	auto Check = [this](const FTCMapPreview& P, const TCMOptT::FExpectedPreview& E)
	{
		const FString What = FString::Printf(TEXT("%s in %s"), E.Map, E.Mode);
		TestTrue(What + TEXT(": ready"), P.Status == ETCPreviewStatus::Ready);
		TestEqual(What + TEXT(": pixels"), (int64)TCMOptT::Fnv(P), (int64)E.Fnv);
		const FTCMapPreview::FLayout L = P.Layout();
		TestTrue(What + TEXT(": layout"), FMath::IsNearlyEqual(L.Scale, E.Scale, 1e-9) && FMath::IsNearlyEqual(L.OffsetX, E.OffsetX, 1e-9) && FMath::IsNearlyEqual(L.OffsetY, E.OffsetY, 1e-9) && FMath::IsNearlyEqual(L.PointRadius, E.Ring, 1e-9));
		TArray<FString> Lines;
		for (const FTCMapPreviewLine& InfoLine : P.Info) Lines.Add(InfoLine.Text);
		TArray<FString> Expected;
		for (const TCHAR* T : E.Info) if (T) Expected.Add(T);
		TestEqual(What + TEXT(": info"), FString::Join(Lines, TEXT(" | ")), FString::Join(Expected, TEXT(" | ")));
		TestEqual(What + TEXT(": spawns"), TCMOptT::SpawnsText(P), FString(E.Spawns));
		TestEqual(What + TEXT(": points"), TCMOptT::PointsText(P), FString(E.Points));
	};

	// The small authored map: every preview feature, in every mode that draws differently
	FTCMapPreviewSource Syn;
	FString Error;
	TestTrue(TEXT("the small map reads"), FTCMatchOptions::ParseMapFile(TEXT("test-ford"), *TCMOptT::Parse(TCMOptT::SynMap), Syn, Error));
	TestTrue(TEXT("no error"), Error.IsEmpty());
	for (const TCMOptT::FExpectedPreview& E : TCMOptT::SynPreviews) Check(FTCMatchOptions::BuildPreview(&Syn, TEXT("test-ford"), E.Mode, TEXT("huge")), E);
	{
		const FTCMapPreview P = FTCMatchOptions::BuildPreview(&Syn, TEXT("test-ford"), TEXT("assault"), TEXT("huge"));
		// sampled cells, from the browser's ImageData in node: water at depth -2, a hedge, a road, depth -1, level 4, a road at 2
		const struct { int32 X, Y; FColor C; } Samples[] = { { 3, 10, FColor(46, 71, 85) }, { 5, 0, FColor(47, 74, 34) }, { 0, 15, FColor(150, 128, 100) },
			{ 0, 11, FColor(95, 104, 61) }, { 20, 0, FColor(160, 175, 102) }, { 19, 15, FColor(186, 159, 124) }, { 21, 17, FColor(134, 146, 86) } };
		for (const auto& S : Samples) TestEqual(FString::Printf(TEXT("pixel %d,%d"), S.X, S.Y), P.Pixels.IsValidIndex(S.Y * P.W + S.X) ? P.Pixels[S.Y * P.W + S.X] : FColor(0, 0, 0, 0), S.C);
		const TArray<FColor> Canvas = P.Raster();
		TestEqual(TEXT("native preview retains the source 280px canvas"), Canvas.Num(), 280 * 280);
		const auto At = P.Layout();
		for (const auto& Spawn : P.Spawns)
		{
			const FVector2D Centre = FTCMapPreview::CellCentre(Spawn.X, Spawn.Y, At);
			const int32 X = FMath::FloorToInt(Centre.X), Y = FMath::FloorToInt(Centre.Y);
			if (X >= 0 && X < 280 && Y >= 0 && Y < 280) TestEqual(TEXT("spawn fills above terrain and capture rings"), Canvas[Y * 280 + X], Spawn.Color);
		}
		TestTrue(TEXT("the name is bold, the dots red then blue"), P.Info.Num() > 6 && P.Info[0].bBold && P.Info[6].Dots.Num() == 2 && P.Info[6].Dots[0] == FColor(0xd9, 0x4a, 0x3d) && P.Info[6].Dots[1] == FColor(0x3d, 0x7b, 0xd9));
		const FTCMeasure Measure = [](const FString& S,float Px,bool,float) { return S.Len() * Px / 2; };
		const FTCPaint Legend = TCMOptPreview::Info(P,{0,0},150,Measure);
		int32 Dots = 0; bool bGlyph = false;
		for (const auto& Item : Legend.Items) { if (Item.Role == TEXT("preview:dot")) Dots++; bGlyph |= Item.Text.Contains(TEXT("●")); }
		TestEqual(TEXT("two coloured legend dots draw without a font glyph"), Dots,18);
		TestFalse(TEXT("no missing dot glyph in legend text"), bGlyph);
		TestEqual(TEXT("an empty mode draws as Conquest"), FTCMatchOptions::BuildPreview(&Syn, TEXT("test-ford"), TEXT(""), TEXT("huge")).Mode, FString(TEXT("conquest")));
	}
	for (const auto& B : TCMOptT::SynBroken)
	{
		FTCMapPreviewSource Out;
		FString Why;
		TestFalse(FString::Printf(TEXT("refused: %s"), B[1]), FTCMatchOptions::ParseMapFile(TEXT("test-ford"), *TCMOptT::Parse(B[0]), Out, Why));
		TestEqual(TEXT("validateMap's words"), Why, FString(B[1]));
	}
	{
		FTCMapPreviewSource Out;
		FString Why;
		TestTrue(TEXT("mp null reads as 1"), FTCMatchOptions::ParseMapFile(TEXT("test-ford"), *TCMOptT::Parse(TCMOptT::SynMpNull), Out, Why) && Out.Points.Num() == 3 && Out.Points[1].Mp == 1.0);
		TestFalse(TEXT("a key the server would not serve"), FTCMatchOptions::ParseMapFile(TEXT("Test_Ford"), *TCMOptT::Parse(TCMOptT::SynMap), Out, Why));
	}

	// World Conquest: the placeholder, no file
	{
		const FTCMapPreview Huge = FTCMatchOptions::BuildPreview(nullptr, TEXT("world"), TEXT("world"), TEXT("huge"));
		const FTCMapPreview Massive = FTCMatchOptions::BuildPreview(nullptr, TEXT("world"), TEXT("world"), TEXT("massive"));
		TestTrue(TEXT("world placeholder"), Huge.Status == ETCPreviewStatus::World && Huge.Title == TEXT("Uncharted continent") && Huge.Subtitle == TEXT("Explore to reveal terrain"));
		TestEqual(TEXT("huge world line"), Huge.Info.Num() ? Huge.Info[0].Text : FString(), FString(TEXT("1,024 × 1,024 metres. 64 regions. Fresh geography at match start. Shared discoveries within your team.")));
		TestEqual(TEXT("massive world line"), Massive.Info.Num() ? Massive.Info[0].Text : FString(), FString(TEXT("2,048 × 1,024 metres. 128 regions. Fresh geography at match start. Shared discoveries within your team.")));
		TestTrue(TEXT("no file and an unknown map: needs the file"), FTCMatchOptions::BuildPreview(nullptr, TEXT("bastogne"), TEXT("conquest"), TEXT("huge")).Status == ETCPreviewStatus::NeedsFile);
	}

	// Through the lobby: which file to fetch, where, and the Map default option once it is here
	{
		FTCMatchOptions O;
		TestTrue(TEXT("no lobby: no preview"), O.Preview().Status == ETCPreviewStatus::NoLobby && O.WantedMap().IsEmpty());
		const TSharedPtr<FJsonObject> Lobby = TCMOptT::Parse(FString(TCMOptT::HostFog).Replace(TEXT("\"mapName\":\"default\""), TEXT("\"mapName\":\"pegasus-bridge\""))); // TCMOptT::HostFog on another map
		TestTrue(TEXT("lobby"), Lobby.IsValid() && O.ApplyLobby(*Lobby));
		TestEqual(TEXT("wants the lobby's map"), O.WantedMap(), FString(TEXT("pegasus-bridge")));
		TestTrue(TEXT("needs the file"), O.Preview().Status == ETCPreviewStatus::NeedsFile);
		TestEqual(TEXT("path"), FTCMatchOptions::MapPath(TEXT("pegasus-bridge")), FString(TEXT("/maps/pegasus-bridge")));
		TestEqual(TEXT("url"), FTCMatchOptions::MapUrl(TEXT("ws://127.0.0.1:3000/ws"), TEXT("pegasus-bridge")), FString(TEXT("http://127.0.0.1:3000/maps/pegasus-bridge")));
		TestEqual(TEXT("secure url"), FTCMatchOptions::MapUrl(TEXT("wss://example.ts.net/ws?room=main"), TEXT("bocage")), FString(TEXT("https://example.ts.net/maps/bocage")));
		TestTrue(TEXT("names the server refuses"), FTCMatchOptions::MapPath(TEXT("a b")).IsEmpty() && FTCMatchOptions::MapPath(TEXT("Bad_Name")).IsEmpty() && FTCMatchOptions::MapUrl(TEXT("http://x/ws"), TEXT("bocage")).IsEmpty());
		FString Why;
		TestTrue(TEXT("file added"), O.AddMapFileText(TEXT("pegasus-bridge"), TCMOptT::SynMap, &Why));
		TestTrue(TEXT("nothing more wanted"), O.WantedMap().IsEmpty() && O.Preview().Status == ETCPreviewStatus::Ready);
		// the small map's name holds no mood word; the key 'pegasus-bridge' makes it a misty dawn
		const FTCOptionControl W = O.Control(ETCMatchOption::Weather);
		TestEqual(TEXT("Map default names the map's plan"), W.Choices.Num() ? W.Choices[0].Label : FString(), FString(TEXT("Map default: Fog until 4:00")));
		TestFalse(TEXT("a body that is not JSON"), O.AddMapFileText(TEXT("bocage"), TEXT("<html>"), &Why));
	}

	// The pinned reference's own map files, read from disk (served the same: mapForClient leaves these fields alone)
	const FString Dir = FPaths::Combine(FPaths::ProjectDir(), TEXT("../reference/ww2-rts/maps"));
	TMap<FString, FTCMapPreviewSource> Loaded;
	for (const TCMOptT::FExpectedPreview& E : TCMOptT::DiskPreviews)
	{
		if (!Loaded.Contains(E.Map))
		{
			FString Body;
			const FString File = FPaths::Combine(Dir, FString(E.Map) + TEXT(".json"));
			if (!FFileHelper::LoadFileToString(Body, *File)) { AddWarning(FString::Printf(TEXT("%s not found: its preview was not checked"), *File)); continue; }
			FTCMapPreviewSource Source;
			FString Why;
			const TSharedPtr<FJsonObject> Obj = TCMOptT::Parse(Body);
			if (!TestTrue(FString::Printf(TEXT("%s reads (%s)"), E.Map, *Why), Obj.IsValid() && FTCMatchOptions::ParseMapFile(E.Map, *Obj, Source, Why))) continue;
			Loaded.Add(E.Map, MoveTemp(Source));
		}
		if (const FTCMapPreviewSource* Source = Loaded.Find(E.Map)) Check(FTCMatchOptions::BuildPreview(Source, E.Map, E.Mode, TEXT("huge")), E);
	}
	if (const FTCMapPreviewSource* Bastogne = Loaded.Find(TEXT("bastogne"))) TestEqual(TEXT("Bastogne snows"), FTCMatchOptions::MapDefaultLabel(&Bastogne->Weather), FString(TEXT("Map default: Snow")));
	return true;
}
#endif
