#include "Client/TCRustQueries.h"
#include "Net/TCMatchState.h"
#include "Net/TCRustViewShadow.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "Policies/CondensedJsonPrintPolicy.h"

namespace
{
	FWW2Result Contract(const TCHAR* Text) { return { FWW2Core::ErrContract, Text }; }
	bool Index(const TSharedPtr<FJsonValue>& V, int32& Out)
	{
		double N;
		if (!V.IsValid() || !V->TryGetNumber(N) || !FMath::IsFinite(N) || N < 0 || N > MAX_int32 || N != FMath::FloorToDouble(N)) return false;
		Out = (int32)N;
		return true;
	}
}

const TCHAR* FTCRustQueries::ReasonCode(ETCRustPlacementReason Reason)
{
	switch (Reason)
	{
	case ETCRustPlacementReason::None: return TEXT("");
	case ETCRustPlacementReason::Blocked: return TEXT("blocked");
	case ETCRustPlacementReason::NotVisible: return TEXT("notVisible");
	case ETCRustPlacementReason::Territory: return TEXT("territory");
	case ETCRustPlacementReason::Rail: return TEXT("rail");
	case ETCRustPlacementReason::Coast: return TEXT("coast");
	}
	return TEXT("");
}

FWW2Result FTCRustQueries::DecodePlacement(const FString& Json, FTCRustPlacement& Out)
{
	TSharedPtr<FJsonObject> Object;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Json), Object) || !Object.IsValid()) return Contract(TEXT("placement result is not a JSON object"));
	FTCRustPlacement P;
	double X, Z;
	if (!Object->TryGetBoolField(TEXT("ok"), P.bOk) || !Object->TryGetNumberField(TEXT("x"), X) || !Object->TryGetNumberField(TEXT("z"), Z)
		|| !FMath::IsFinite(X) || !FMath::IsFinite(Z)) return Contract(TEXT("placement result lacks finite x/z or Boolean ok"));
	P.At = { X, Z };
	if (P.bOk)
	{
		if (Object->HasField(TEXT("reason"))) return Contract(TEXT("successful placement must omit reason"));
	}
	else
	{
		FString Reason;
		if (!Object->TryGetStringField(TEXT("reason"), Reason)) return Contract(TEXT("refused placement lacks a reason"));
		bool bKnown = false;
		for (ETCRustPlacementReason R : { ETCRustPlacementReason::Blocked, ETCRustPlacementReason::NotVisible, ETCRustPlacementReason::Territory,
			ETCRustPlacementReason::Rail, ETCRustPlacementReason::Coast })
			if (Reason == ReasonCode(R)) { P.Reason = R; bKnown = true; break; }
		if (!bKnown) return Contract(TEXT("unreviewed placement reason"));
	}
	if (const auto V = Object->TryGetField(TEXT("c")); V.IsValid())
	{
		int32 C;
		if (!Index(V, C)) return Contract(TEXT("placement c is not a cell index"));
		P.Cell = C;
	}
	if (const auto V = Object->TryGetField(TEXT("cells")); V.IsValid() && V->Type != EJson::Null)
	{
		const TArray<TSharedPtr<FJsonValue>>* Cells;
		if (!V->TryGetArray(Cells)) return Contract(TEXT("placement cells is not an array or null"));
		P.bHasCells = true;
		for (const auto& Entry : *Cells)
		{
			FTCRustPlacementCell C;
			if (Entry.IsValid() && Entry->Type == EJson::Array)
			{
				const auto& Pair = Entry->AsArray();
				if (Pair.Num() != 2 || !Index(Pair[0], C.Cell) || !Pair[1].IsValid()) return Contract(TEXT("invalid fort placement pair"));
				C.bFort = true;
				if (Pair[1]->Type != EJson::Null)
				{
					FString Ch;
					if (!Pair[1]->TryGetString(Ch) || Ch.Len() != 1) return Contract(TEXT("invalid fort placement character"));
					C.Character = Ch;
				}
			}
			else if (!Index(Entry, C.Cell)) return Contract(TEXT("invalid building placement cell"));
			P.Cells.Add(MoveTemp(C));
		}
	}
	if (const auto V = Object->TryGetField(TEXT("node")); V.IsValid())
	{
		if (V->Type != EJson::Object) return Contract(TEXT("placement node is not an object"));
		P.Node = V->AsObject();
	}
	Out = MoveTemp(P);
	return {};
}

FWW2Result FTCRustQueries::Placement(const FTCMatchState& State, const FString& Kind, const FVector2D& At,
	TOptional<double> Direction, TOptional<int32> Team, FTCRustPlacement& Out)
{
	if (State.Mode == TEXT("world") || State.Map.bWorld) return { FWW2Core::ErrBadArgument, TEXT("World recipient placement retains the C++ terrain/history fallback") };
	FTCRustViewShadow* View = State.GetUnitAuthority();
	if (!View) return { FWW2Core::ErrWrongState, TEXT("no adopted Rust recipient; use the C++ placement fallback") };
	if (!FMath::IsFinite(At.X) || !FMath::IsFinite(At.Y) || (Direction.IsSet() && !FMath::IsFinite(Direction.GetValue())))
		return { FWW2Core::ErrBadArgument, TEXT("placement coordinates and direction must be finite") };
	const auto Request = MakeShared<FJsonObject>();
	Request->SetStringField(TEXT("kind"), Kind);
	Request->SetNumberField(TEXT("x"), At.X); Request->SetNumberField(TEXT("z"), At.Y);
	if (Direction.IsSet()) Request->SetNumberField(TEXT("dir"), Direction.GetValue());
	if (Team.IsSet()) Request->SetNumberField(TEXT("team"), Team.GetValue());
	FString Json;
	if (!FJsonSerializer::Serialize(Request, TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Json))) return Contract(TEXT("could not serialize placement request"));
	const FTCHARToUTF8 Utf8(*Json, Json.Len());
	TArray<UTF8CHAR> Bytes;
	const FWW2Result R = View->PlacementCheck(State, { reinterpret_cast<const UTF8CHAR*>(Utf8.Get()), Utf8.Length() }, Bytes);
	if (!R.IsOk()) return R;
	return DecodePlacement(FString::ConstructFromPtrSize(Bytes.GetData(), Bytes.Num()), Out);
}

FWW2Result FTCRustQueries::AuthoringJson(EWW2AuthoringQuery Query, const FString& Json, FString& Out)
{
	const FTCHARToUTF8 Utf8(*Json, Json.Len());
	TArray<UTF8CHAR> Bytes;
	const FWW2Result R = FWW2Core::AuthoringQuery(Query, { reinterpret_cast<const UTF8CHAR*>(Utf8.Get()), Utf8.Length() }, Bytes);
	if (R.IsOk()) Out = FString::ConstructFromPtrSize(Bytes.GetData(), Bytes.Num());
	return R;
}

FWW2Result FTCRustQueries::Authoring(EWW2AuthoringQuery Query, const FString& Json, TSharedPtr<FJsonValue>& Out)
{
	FString Answer;
	const FWW2Result R = AuthoringJson(Query, Json, Answer);
	if (!R.IsOk()) return R;
	// Unreal's serializer requires an object/array root. The ABI also returns scalar strings and null, so parse a
	// local envelope. AuthoringJson still retains the provider's exact bytes, including full-width seed literals.
	TSharedPtr<FJsonObject> Envelope;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TEXT("{\"value\":") + Answer + TEXT("}")), Envelope) || !Envelope.IsValid())
		return Contract(TEXT("authoring result is not JSON"));
	TSharedPtr<FJsonValue> Value = Envelope->TryGetField(TEXT("value"));
	if (!Value.IsValid()) return Contract(TEXT("authoring result is missing"));
	Out = MoveTemp(Value);
	return R;
}

FWW2Result FTCRustQueries::PlayMatch(const FString& MapJson, const FString& OptionsJson, uint64 Seed, int32 MaxTicks, FString& Out)
{
	if (MaxTicks < 0 || MaxTicks > 36000) return { FWW2Core::ErrBadArgument, TEXT("max_ticks must be an integer from 0 through 36000") };
	// Keep the seed as a decimal u64 token, never a JSON double. Options are explicit; no mode is silently replaced.
	const FString Request = FString::Printf(TEXT("{\"map\":%s,\"options\":%s,\"seed\":%llu,\"max_ticks\":%d}"),
		*MapJson, OptionsJson.IsEmpty() ? TEXT("{}") : *OptionsJson, (unsigned long long)Seed, MaxTicks);
	return AuthoringJson(EWW2AuthoringQuery::PlayMatch, Request, Out);
}
