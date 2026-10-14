#include "Client/TCLogistics.h"
#include "Client/TCOrders.h"
#include "Client/TCRecruitment.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "ThreeCrossroads.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

const TCHAR* const FTCLogistics::ReferencePin = TEXT("e4e579a");
const TCHAR* const FTCLogisticsCard::Hint = TEXT("Right-click an allied unit or Supply Cache to deliver carried cargo");
const TCHAR* const FTCLogisticsCard::ResumeLabel = TEXT("Resume deliveries");

const TCHAR* LexToString(ETCLogisticsOrderKind Kind)
{
	switch (Kind)
	{
	case ETCLogisticsOrderKind::Supply: return TEXT("supply");
	case ETCLogisticsOrderKind::Resume: return TEXT("logisticsResume");
	case ETCLogisticsOrderKind::Unsupported: return TEXT("unsupported");
	default: return TEXT("none");
	}
}

namespace
{
// BEGIN TC_LOGISTICS_PURE
// The JavaScript number semantics the reference's label code relies on, in standard C++ only (no engine types), so
// they can be checked against node outside the engine. Assumes the "C" numeric locale for snprintf and strtod.
namespace TCJs
{
	// Math.round: the closest integer, ties toward +infinity.
	inline double Round(double X)
	{
		if (!std::isfinite(X)) return X;
		const double F = std::floor(X);
		const double R = X - F >= 0.5 ? F + 1.0 : F;
		return R == 0.0 && std::signbit(X) ? -0.0 : R; // -0.5 up to -0 round to -0
	}

	// Math.hypot for two arguments as V8 computes it (src/builtins/math.tq MathHypot): both normalised to the larger
	// magnitude, squared and added with Kahan compensation, then the square root scaled back.
	inline double Hypot(double A, double B)
	{
		// a NaN argument keeps its zeroed slot; Infinity wins over NaN, then NaN, then zero
		const bool bNaN = std::isnan(A) || std::isnan(B);
		const double Values[2] = { std::isnan(A) ? 0.0 : std::fabs(A), std::isnan(B) ? 0.0 : std::fabs(B) };
		const double Max = Values[0] > Values[1] ? Values[0] : Values[1];
		if (Max == HUGE_VAL) return HUGE_VAL;
		if (bNaN) return NAN;
		if (Max == 0.0) return 0.0;
		double Sum = 0.0, Compensation = 0.0;
		for (double V : Values)
		{
			const double N = V / Max;
			const double Summand = N * N - Compensation;
			const double Preliminary = Sum + Summand;
			Compensation = (Preliminary - Sum) - Summand;
			Sum = Preliminary;
		}
		return std::sqrt(Sum) * Max;
	}

	inline int Put(char* Out, int Cap, int At, char C)
	{
		if (At < Cap - 1) Out[At] = C;
		return At + 1;
	}

	// Number.prototype.toString() (ECMAScript Number::toString, radix 10): the shortest digits that read back as the
	// same double, laid out plainly from 1e-6 up to 1e21 and in exponent form outside. Writes a terminated string.
	// The shortest digits come from the first precision that round-trips; at an exact power of two, where the gap
	// below is half the gap above, a shortest string on the far side can exist that this search does not find.
	inline int ToChars(double V, char* Out, int Cap)
	{
		if (Cap <= 0) return 0;
		if (std::isnan(V)) return std::snprintf(Out, Cap, "NaN");
		if (std::isinf(V)) return std::snprintf(Out, Cap, V > 0 ? "Infinity" : "-Infinity");
		if (V == 0.0) return std::snprintf(Out, Cap, "0");
		int At = 0;
		if (V < 0) { At = Put(Out, Cap, At, '-'); V = -V; }
		if (V < 9007199254740992.0 && std::floor(V) == V)
		{
			char Digits[32];
			std::snprintf(Digits, sizeof(Digits), "%llu", (unsigned long long)V);
			for (const char* P = Digits; *P; P++) At = Put(Out, Cap, At, *P);
			Out[At < Cap ? At : Cap - 1] = 0;
			return At;
		}
		char Buf[40];
		for (int Precision = 1; Precision <= 17; Precision++)
		{
			std::snprintf(Buf, sizeof(Buf), "%.*e", Precision - 1, V);
			if (std::strtod(Buf, nullptr) == V || Precision == 17) break;
		}
		// Buf is d[.ddd]e(+|-)xx
		char Digits[24];
		int K = 0;
		const char* P = Buf;
		for (; *P && *P != 'e'; P++) if (*P >= '0' && *P <= '9' && K < 23) Digits[K++] = *P;
		while (K > 1 && Digits[K - 1] == '0') K--;
		const int Exp = *P == 'e' ? std::atoi(P + 1) : 0;
		const int N = Exp + 1;
		if (K <= N && N <= 21)
		{
			for (int I = 0; I < K; I++) At = Put(Out, Cap, At, Digits[I]);
			for (int I = K; I < N; I++) At = Put(Out, Cap, At, '0');
		}
		else if (0 < N && N <= 21)
		{
			for (int I = 0; I < K; I++) { if (I == N) At = Put(Out, Cap, At, '.'); At = Put(Out, Cap, At, Digits[I]); }
		}
		else if (-6 < N && N <= 0)
		{
			At = Put(Out, Cap, At, '0'); At = Put(Out, Cap, At, '.');
			for (int I = 0; I < -N; I++) At = Put(Out, Cap, At, '0');
			for (int I = 0; I < K; I++) At = Put(Out, Cap, At, Digits[I]);
		}
		else
		{
			At = Put(Out, Cap, At, Digits[0]);
			if (K > 1) { At = Put(Out, Cap, At, '.'); for (int I = 1; I < K; I++) At = Put(Out, Cap, At, Digits[I]); }
			char E[16];
			std::snprintf(E, sizeof(E), "e%c%d", N - 1 < 0 ? '-' : '+', N - 1 < 0 ? 1 - N : N - 1);
			for (const char* Q = E; *Q; Q++) At = Put(Out, Cap, At, *Q);
		}
		Out[At < Cap ? At : Cap - 1] = 0;
		return At;
	}

	// Number.prototype.toFixed(1), exactly: the integer n nearest to 10x (the larger on a tie), as n / 10.
	inline int ToFixed1(double X, char* Out, int Cap)
	{
		if (Cap <= 0) return 0;
		if (!std::isfinite(X) || X >= 1e21 || X <= -1e21) return ToChars(X, Out, Cap);
		int At = 0;
		if (X < 0) { At = Put(Out, Cap, At, '-'); X = -X; }
		char Digits[40];
		int Len = 0;
		if (X == 0.0) { Digits[0] = '0'; Digits[1] = '0'; Len = 2; }
		else
		{
			int E2 = 0;
			const double F = std::frexp(X, &E2);
			const uint64_t M = (uint64_t)std::ldexp(F, 53); // X = M / 2^S, M < 2^53
			const int S = 53 - E2;
			if (S <= 0)
			{
				// X is an integer below 1e21: its exact digits, then one zero decimal. Base 1e9 limbs, doubled -S times.
				uint32_t Limb[4] = { (uint32_t)(M % 1000000000u), (uint32_t)((M / 1000000000u) % 1000000000u), (uint32_t)(M / 1000000000000000000ull), 0 };
				for (int I = 0; I < -S; I++)
				{
					uint32_t Carry = 0;
					for (uint32_t& L : Limb) { const uint64_t T = (uint64_t)L * 2 + Carry; L = (uint32_t)(T % 1000000000u); Carry = (uint32_t)(T / 1000000000u); }
				}
				int Top = 3;
				while (Top > 0 && !Limb[Top]) Top--;
				char Part[16];
				std::snprintf(Part, sizeof(Part), "%u", Limb[Top]);
				for (const char* P = Part; *P; P++) Digits[Len++] = *P;
				for (int I = Top - 1; I >= 0; I--) { std::snprintf(Part, sizeof(Part), "%09u", Limb[I]); for (const char* P = Part; *P; P++) Digits[Len++] = *P; }
				Digits[Len++] = '0';
			}
			else
			{
				// n = floor(10x + 1/2) = floor((20M + 2^S) / 2^(S+1)); below 2^-6 (S >= 59) it is 0
				const uint64_t Nn = S >= 59 ? 0 : (20 * M + ((uint64_t)1 << S)) >> (S + 1);
				char Part[32];
				std::snprintf(Part, sizeof(Part), "%llu", (unsigned long long)Nn);
				const int PL = (int)std::strlen(Part);
				if (PL < 2) { Digits[Len++] = '0'; }
				for (int I = 0; I < PL; I++) Digits[Len++] = Part[I];
			}
		}
		for (int I = 0; I < Len - 1; I++) At = Put(Out, Cap, At, Digits[I]);
		At = Put(Out, Cap, At, '.');
		At = Put(Out, Cap, At, Digits[Len - 1]);
		Out[At < Cap ? At : Cap - 1] = 0;
		return At;
	}

	// ECMAScript StringToNumber: surrounding white space and line terminators ignored, empty is 0, Infinity with an
	// optional sign, decimal literals, and unsigned 0x/0o/0b integers; anything else NaN. Decimal values go through
	// strtod (correctly rounded); 0x/0o/0b digits are accumulated in a double (exact up to 2^53).
	template <typename CharT>
	double StringToNumber(const CharT* S, int N)
	{
		auto White = [](uint32_t C)
		{
			return C == 9 || C == 10 || C == 11 || C == 12 || C == 13 || C == 32 || C == 0xA0 || C == 0x1680 || (C >= 0x2000 && C <= 0x200A)
				|| C == 0x2028 || C == 0x2029 || C == 0x202F || C == 0x205F || C == 0x3000 || C == 0xFEFF;
		};
		auto At = [S](int I) { return (uint32_t)S[I]; };
		int B = 0, E = N;
		while (B < E && White(At(B))) B++;
		while (E > B && White(At(E - 1))) E--;
		if (B == E) return 0.0;
		if (E - B > 2 && At(B) == '0')
		{
			const uint32_t P = At(B + 1);
			const int Radix = P == 'x' || P == 'X' ? 16 : P == 'o' || P == 'O' ? 8 : P == 'b' || P == 'B' ? 2 : 0;
			if (Radix)
			{
				double V = 0;
				for (int I = B + 2; I < E; I++)
				{
					const uint32_t C = At(I);
					const int D = C >= '0' && C <= '9' ? (int)(C - '0') : C >= 'a' && C <= 'z' ? (int)(C - 'a' + 10) : C >= 'A' && C <= 'Z' ? (int)(C - 'A' + 10) : 99;
					if (D >= Radix) return NAN;
					V = V * Radix + D;
				}
				return V;
			}
		}
		int I = B;
		bool bNeg = false;
		if (At(I) == '+' || At(I) == '-') { bNeg = At(I) == '-'; I++; }
		static const char Inf[] = "Infinity";
		if (E - I == 8)
		{
			bool bInf = true;
			for (int K = 0; K < 8; K++) bInf &= At(I + K) == (uint32_t)Inf[K];
			if (bInf) return bNeg ? -HUGE_VAL : HUGE_VAL;
		}
		auto Digit = [&At](int K) { return At(K) >= '0' && At(K) <= '9'; };
		const int Start = I;
		int Whole = 0, Frac = 0;
		while (I < E && Digit(I)) { I++; Whole++; }
		if (I < E && At(I) == '.') { I++; while (I < E && Digit(I)) { I++; Frac++; } }
		if (!Whole && !Frac) return NAN;
		if (I < E && (At(I) == 'e' || At(I) == 'E'))
		{
			I++;
			if (I < E && (At(I) == '+' || At(I) == '-')) I++;
			int ExpDigits = 0;
			while (I < E && Digit(I)) { I++; ExpDigits++; }
			if (!ExpDigits) return NAN;
		}
		if (I != E) return NAN;
		const int Len = E - Start;
		char Small[128];
		char* Buf = Len < (int)sizeof(Small) ? Small : (char*)std::malloc(Len + 1);
		if (!Buf) return NAN;
		for (int K = 0; K < Len; K++) Buf[K] = (char)At(Start + K);
		Buf[Len] = 0;
		const double V = std::strtod(Buf, nullptr);
		if (Buf != Small) std::free(Buf);
		return bNeg ? -V : V;
	}
}
// END TC_LOGISTICS_PURE

	FString JsStr(double V)
	{
		char Buf[64];
		TCJs::ToChars(V, Buf, sizeof(Buf));
		return FString(ANSI_TO_TCHAR(Buf));
	}

	FString Fixed1(double V)
	{
		char Buf[64];
		TCJs::ToFixed1(V, Buf, sizeof(Buf));
		return FString(ANSI_TO_TCHAR(Buf));
	}

	// client/logistics.js seconds: `${Math.ceil(amount(n))} s`
	FString Seconds(const FTCJsValue& V) { return JsStr(std::ceil(V.Amount())) + TEXT(" s"); }

	// A unit id as the browser's Map of unit rows holds it: an integral number.
	bool IdOf(const FTCJsValue& V, int32& Out)
	{
		if (!V.IsFiniteNumber() || std::floor(V.Number) != V.Number || V.Number < -2147483648.0 || V.Number > 2147483647.0) return false;
		Out = (int32)V.Number;
		return true;
	}

	// JOBS[row.state] for the string states. client/logistics.js JOBS.
	const TCHAR* JobText(const FTCJsValue& State)
	{
		if (State.Kind != FTCJsValue::EKind::String) return nullptr;
		static const TPair<const TCHAR*, const TCHAR*> Jobs[] = {
			{ TEXT("idle"), TEXT("Ready for deliveries") }, { TEXT("loading"), TEXT("Loading cargo") }, { TEXT("unloading"), TEXT("Unloading cargo") },
			{ TEXT("delivering"), TEXT("Delivering supplies") }, { TEXT("collecting"), TEXT("Returning to source") },
			{ TEXT("waiting"), TEXT("Delivery route blocked") }, { TEXT("waitingCollect"), TEXT("Delivery route blocked") },
			{ TEXT("waitingReturn"), TEXT("Delivery route blocked") }, { TEXT("manual"), TEXT("Manual route") },
			{ TEXT("travelling"), TEXT("Delivering supplies") }, { TEXT("transit"), TEXT("Delivering supplies") },
			{ TEXT("returning"), TEXT("Returning to source") }, { TEXT("blocked"), TEXT("Delivery route blocked") },
			{ TEXT("hold"), TEXT("Deliveries held") }, { TEXT("held"), TEXT("Deliveries held") },
		};
		// the reference's keys are case-sensitive
		for (const TPair<const TCHAR*, const TCHAR*>& J : Jobs) if (State.String.Equals(J.Key, ESearchCase::CaseSensitive)) return J.Value;
		return nullptr;
	}

	// EVENT_TEXT[event.type] for the string types. client/logistics.js EVENT_TEXT.
	const TCHAR* EventText(const FTCJsValue& Type)
	{
		if (Type.Kind != FTCJsValue::EKind::String) return nullptr;
		static const TPair<const TCHAR*, const TCHAR*> Texts[] = {
			{ TEXT("low"), TEXT("Low supplies") }, { TEXT("lowReserve"), TEXT("Low supplies") }, { TEXT("lowReserves"), TEXT("Low supplies") },
			{ TEXT("blocked"), TEXT("Delivery route blocked") }, { TEXT("unreachable"), TEXT("Delivery route unreachable") },
			{ TEXT("dangerous"), TEXT("Known danger blocks deliveries") }, { TEXT("danger"), TEXT("Known danger blocks deliveries") },
			{ TEXT("stock"), TEXT("Supply stockpile empty") }, { TEXT("insufficientStock"), TEXT("Supply stockpile empty") },
			{ TEXT("withdrawal"), TEXT("Mandatory withdrawal") }, { TEXT("forced"), TEXT("Mandatory withdrawal") },
			{ TEXT("transit"), TEXT("Supplies in transit") }, { TEXT("stranded"), TEXT("Stranded") }, { TEXT("cut"), TEXT("Cut off from supply") },
			{ TEXT("restored"), TEXT("Supply line restored") },
		};
		for (const TPair<const TCHAR*, const TCHAR*>& T : Texts) if (Type.String.Equals(T.Key, ESearchCase::CaseSensitive)) return T.Value;
		return nullptr;
	}

	const TArray<TSharedPtr<FJsonValue>>* ArrayField(const FJsonObject& O, const TCHAR* Name)
	{
		const TSharedPtr<FJsonValue> V = O.TryGetField(Name);
		return V.IsValid() && V->Type == EJson::Array ? &V->AsArray() : nullptr;
	}

	uint64 GObjectKeys = 0; // every object or array read gets its own Map key (game thread only)
}

// ---------- FTCJsValue ----------

FTCJsValue FTCJsValue::From(const TSharedPtr<FJsonValue>& V)
{
	FTCJsValue R;
	if (!V.IsValid()) return R;
	switch (V->Type)
	{
	case EJson::Null: R.Kind = EKind::Null; break;
	case EJson::Boolean: R.Kind = EKind::Bool; R.bBool = V->AsBool(); break;
	case EJson::Number: R.Kind = EKind::Number; R.Number = V->AsNumber(); break;
	case EJson::String: R.Kind = EKind::String; R.String = V->AsString(); break;
	case EJson::Array: R.Kind = EKind::Array; break;
	case EJson::Object: R.Kind = EKind::Object; R.Obj = V->AsObject(); break;
	default: break; // EJson::None: no value
	}
	return R;
}

FTCJsValue FTCJsValue::Field(const FJsonObject& O, const TCHAR* Name) { return From(O.TryGetField(Name)); }

FTCJsValue FTCJsValue::Get(const TCHAR* Name) const
{
	return Kind == EKind::Object && Obj.IsValid() ? Field(*Obj, Name) : FTCJsValue();
}

bool FTCJsValue::IsFiniteNumber() const { return Kind == EKind::Number && std::isfinite(Number); }

bool FTCJsValue::Truthy() const
{
	switch (Kind)
	{
	case EKind::Bool: return bBool;
	case EKind::Number: return Number != 0 && !std::isnan(Number);
	case EKind::String: return !String.IsEmpty();
	case EKind::Array: case EKind::Object: return true;
	default: return false;
	}
}

double FTCJsValue::ToNumber() const
{
	switch (Kind)
	{
	case EKind::Null: return 0;
	case EKind::Bool: return bBool ? 1 : 0;
	case EKind::Number: return Number;
	case EKind::String: return TCJs::StringToNumber(*String, String.Len());
	default: return NAN;
	}
}

double FTCJsValue::Amount() const { return IsFiniteNumber() && Number > 0 ? Number : 0.0; }

bool FTCJsValue::IsPosition() const { return Kind == EKind::Object && Get(TEXT("x")).IsFiniteNumber() && Get(TEXT("z")).IsFiniteNumber(); }

FString FTCJsValue::MapKey() const
{
	switch (Kind)
	{
	case EKind::Null: return TEXT("null");
	case EKind::Bool: return bBool ? TEXT("true") : TEXT("false");
	case EKind::Number: return TEXT("n") + JsStr(Number); // shortest digits: one string per double, 0 and -0 alike
	case EKind::String: return TEXT("s") + String;
	case EKind::Array: case EKind::Object: return FString::Printf(TEXT("o%llu"), (unsigned long long)++GObjectKeys);
	default: return TEXT("undefined");
	}
}

FTCSupplies FTCSupplies::From(const FTCJsValue& V)
{
	FTCSupplies S;
	S.Ammo = V.Get(TEXT("ammo")); S.Provisions = V.Get(TEXT("provisions")); S.Fuel = V.Get(TEXT("fuel"));
	return S;
}

// ---------- lifecycle and decoding ----------

void FTCLogistics::ResetData()
{
	bEnabled = false;
	ReserveRows.Reset(); ReserveIndex.Reset();
	TruckRows.Reset(); TruckIndex.Reset();
	StoreRows.Reset(); StoreIndex.Reset();
	RegionRows.Reset(); RegionIndex.Reset();
}

void FTCLogistics::ApplyStart()
{
	// client/main.js startGame: logisticsAlerts.reset(); logisticsData = decodeLogistics({}, ...); logisticsSelection = false
	ResetData();
	bHasSnapshot = false;
	bSelectionMode = false;
	Seen.Reset(); SeenOrder.Reset(); Recent.Reset(); PendingAlerts.Reset();
	bWarnedMalformed = false;
}

void FTCLogistics::Clear()
{
	ApplyStart();
}

void FTCLogistics::ApplySnapshot(const FJsonObject& M, const FTCMatchState& State)
{
	if (!State.bHasMatch) return;
	bHasSnapshot = true;
	ResetData();

	// client/main.js: decodeLogistics(s, units, watching || s.out?.[me] ? -1 : me, owner => !foe(owner)). This client
	// also waits for its seat to be confirmed (FTCMatchState::SeatConfirmed) before it holds any row as its own.
	const bool bSeat = State.SeatConfirmed();
	bool bOut = false;
	if (const TArray<TSharedPtr<FJsonValue>>* Out = ArrayField(M, TEXT("out")))
		bOut = State.You >= 0 && Out->IsValidIndex(State.You) && FTCJsValue::From((*Out)[State.You]).Truthy();
	const int32 Owner = bSeat && !bOut ? State.You : -1;

	const FTCJsValue Wire = FTCJsValue::Field(M, TEXT("logistics"));
	const FTCJsValue Enabled = Wire.Get(TEXT("enabled"));
	bEnabled = Enabled.Kind == FTCJsValue::EKind::Bool && Enabled.bBool && Wire.Obj.IsValid();
	if (!bEnabled) return;
	const FJsonObject& W = *Wire.Obj;
	auto Rows = [&W](const TCHAR* Name, TFunctionRef<void(const FTCJsValue&)> Each)
	{
		// A missing, null or non-array list reads as empty (the browser's `?? []`; a non-array would throw there).
		if (const TArray<TSharedPtr<FJsonValue>>* Arr = ArrayField(W, Name))
			for (const TSharedPtr<FJsonValue>& V : *Arr)
			{
				const FTCJsValue Row = FTCJsValue::From(V);
				if (Row.Kind == FTCJsValue::EKind::Object && Row.Obj.IsValid()) Each(Row);
			}
	};

	// units and trucks: own = visibleUnits.get(row.id)?.owner === owner. new Map(rows): a repeated id keeps its first
	// place and takes the last row.
	auto OwnId = [&State, Owner](const FTCJsValue& Row, int32& Id)
	{
		if (Owner < 0 || !IdOf(Row.Get(TEXT("id")), Id)) return false;
		const FTCUnit* U = State.Units.Find(Id);
		return U && U->Owner == Owner;
	};
	Rows(TEXT("units"), [&](const FTCJsValue& Row)
	{
		int32 Id;
		if (!OwnId(Row, Id)) return;
		FTCLogisticsReserve R;
		R.Id = Id;
		R.Ammo = Row.Get(TEXT("ammo")); R.Provisions = Row.Get(TEXT("provisions")); R.Fuel = Row.Get(TEXT("fuel"));
		R.Emergency = Row.Get(TEXT("emergency")); R.Warning = Row.Get(TEXT("warning")); R.Forced = Row.Get(TEXT("forced"));
		R.Stranded = Row.Get(TEXT("stranded")); R.Supply = Row.Get(TEXT("supply")); R.Cut = Row.Get(TEXT("cut")); R.Grace = Row.Get(TEXT("grace"));
		if (const int32* At = ReserveIndex.Find(Id)) ReserveRows[*At] = MoveTemp(R);
		else ReserveIndex.Add(Id, ReserveRows.Add(MoveTemp(R)));
	});
	Rows(TEXT("trucks"), [&](const FTCJsValue& Row)
	{
		int32 Id;
		if (!OwnId(Row, Id)) return;
		FTCLogisticsTruck T;
		T.Id = Id;
		T.State = Row.Get(TEXT("state")); T.Manual = Row.Get(TEXT("manual")); T.Hold = Row.Get(TEXT("hold"));
		T.Cargo = FTCSupplies::From(Row.Get(TEXT("cargo")));
		const FTCJsValue Dest = Row.Get(TEXT("destination"));
		if (Dest.IsPosition()) { T.bDestination = true; T.DestX = Dest.Get(TEXT("x")).Number; T.DestZ = Dest.Get(TEXT("z")).Number; }
		if (const TArray<TSharedPtr<FJsonValue>>* Route = ArrayField(*Row.Obj, TEXT("route")))
			for (const TSharedPtr<FJsonValue>& P : *Route)
			{
				const FTCJsValue Point = FTCJsValue::From(P);
				FTCRoutePoint RP;
				if (Point.IsPosition()) { RP.bValid = true; RP.X = Point.Get(TEXT("x")).Number; RP.Z = Point.Get(TEXT("z")).Number; }
				T.Route.Add(RP);
			}
		if (const int32* At = TruckIndex.Find(Id)) TruckRows[*At] = MoveTemp(T);
		else TruckIndex.Add(Id, TruckRows.Add(MoveTemp(T)));
	});
	// stores: friendly(row.owner) && position(row). friendly is !foe: an integral seat on this client's team.
	Rows(TEXT("stores"), [&](const FTCJsValue& Row)
	{
		int32 StoreOwner;
		if (!bSeat || !IdOf(Row.Get(TEXT("owner")), StoreOwner) || State.IsFoe(StoreOwner) || !Row.IsPosition()) return;
		FTCLogisticsStore S;
		S.Id = Row.Get(TEXT("id"));
		S.Owner = StoreOwner;
		S.X = Row.Get(TEXT("x")).Number; S.Z = Row.Get(TEXT("z")).Number;
		S.bSource = Row.Get(TEXT("source")).Truthy();
		S.Stock = FTCSupplies::From(Row.Get(TEXT("stock")));
		S.Capacity = FTCSupplies::From(Row.Get(TEXT("capacity")));
		const FString Key = S.Id.MapKey();
		if (const int32* At = StoreIndex.Find(Key)) StoreRows[*At] = MoveTemp(S);
		else StoreIndex.Add(Key, StoreRows.Add(MoveTemp(S)));
	});
	// regions: new Map([[regionId, rate], ...]); the reference's region ids are integers
	if (bSeat)
		if (const TArray<TSharedPtr<FJsonValue>>* Regions = ArrayField(W, TEXT("regions")))
			for (const TSharedPtr<FJsonValue>& V : *Regions)
			{
				if (!V.IsValid() || V->Type != EJson::Array) continue;
				const TArray<TSharedPtr<FJsonValue>>& Entry = V->AsArray();
				int32 Region;
				if (!Entry.Num() || !IdOf(FTCJsValue::From(Entry[0]), Region)) continue;
				const FTCJsValue Rate = Entry.Num() > 1 ? FTCJsValue::From(Entry[1]) : FTCJsValue();
				if (const int32* At = RegionIndex.Find(Region)) RegionRows[*At].Value = Rate;
				else RegionIndex.Add(Region, RegionRows.Add(TPair<int32, FTCJsValue>(Region, Rate)));
			}

	// client/main.js: if (!watching && !s.out?.[me]) logisticsAlerts.update(logisticsData.events, s.tick / 20)
	if (Owner >= 0)
	{
		static const TArray<TSharedPtr<FJsonValue>> NoEvents;
		const TArray<TSharedPtr<FJsonValue>>* Events = ArrayField(W, TEXT("events"));
		UpdateAlerts(Events ? *Events : NoEvents, FTCJsValue::Field(M, TEXT("tick")).ToNumber() / 20.0);
	}
}

void FTCLogistics::UpdateAlerts(const TArray<TSharedPtr<FJsonValue>>& Events, double Time)
{
	// client/logistics.js createLogisticsAlerts update
	TArray<FTCLogisticsAlert> Groups;
	TMap<FString, int32> GroupIndex;
	for (const TSharedPtr<FJsonValue>& V : Events)
	{
		const FTCJsValue Event = FTCJsValue::From(V);
		if (Event.IsNullish()) continue; // the browser would throw reading event.id; the reference never sends one
		const FString Id = Event.Get(TEXT("id")).MapKey();
		if (Seen.Contains(Id)) continue;
		Seen.Add(Id); SeenOrder.Add(Id);
		const FTCJsValue Type = Event.Get(TEXT("type"));
		// Convoy losses are cargo losses, not one combat loss Alert per truck.
		if (Type.Kind == FTCJsValue::EKind::String && (Type.String.Equals(TEXT("truckLost"), ESearchCase::CaseSensitive) || Type.String.Equals(TEXT("truckLoss"), ESearchCase::CaseSensitive))) continue;
		// group.count += event.count ?? 1: numbers and booleans add; a string, array or object would turn the browser's
		// count into text, which the reference never sends, so such an event is left out (once logged).
		const FTCJsValue Count = Event.Get(TEXT("count"));
		double Add = 1;
		if (Count.Kind == FTCJsValue::EKind::Number || Count.Kind == FTCJsValue::EKind::Bool) Add = Count.ToNumber();
		else if (!Count.IsNullish())
		{
			if (!bWarnedMalformed) { bWarnedMalformed = true; UE_LOG(LogTC, Warning, TEXT("logistics: an event's count is not a number; left out of the alerts")); }
			continue;
		}
		const TCHAR* Known = EventText(Type);
		const FString Text = Known ? Known : TEXT("Supply delivery delayed");
		const bool bAt = Event.IsPosition();
		const double X = bAt ? Event.Get(TEXT("x")).Number : 0, Z = bAt ? Event.Get(TEXT("z")).Number : 0;
		const FString Key = Text + TEXT("|") + JsStr(std::floor(X / 30)) + TEXT("|") + JsStr(std::floor(Z / 30));
		const int32* At = GroupIndex.Find(Key);
		if (!At)
		{
			FTCLogisticsAlert G;
			G.Key = Key; G.Text = Text; G.X = X; G.Z = Z; G.Count = 0;
			At = &GroupIndex.Add(Key, Groups.Add(MoveTemp(G)));
		}
		Groups[*At].Count += Add;
	}
	for (FTCLogisticsAlert& G : Groups)
	{
		TPair<FString, double>* Last = Recent.FindByPredicate([&G](const TPair<FString, double>& R) { return R.Key == G.Key; });
		if (Time - (Last ? Last->Value : -HUGE_VAL) < 20) continue;
		if (Last) Last->Value = Time; else Recent.Add(TPair<FString, double>(G.Key, Time));
		G.Text = G.Text + TEXT(": ") + JsStr(G.Count);
		PendingAlerts.Add(G);
	}
	Recent.RemoveAll([Time](const TPair<FString, double>& R) { return Time - R.Value > 120; });
	if (Seen.Num() > 2000)
	{
		TArray<FString> Retained(SeenOrder.GetData() + SeenOrder.Num() - 1000, 1000);
		Seen.Reset();
		for (const FString& S : Retained) Seen.Add(S);
		SeenOrder = MoveTemp(Retained);
	}
}

TArray<FTCLogisticsAlert> FTCLogistics::TakeAlerts()
{
	TArray<FTCLogisticsAlert> Out = MoveTemp(PendingAlerts);
	PendingAlerts.Reset();
	return Out;
}

// ---------- reads ----------

bool FTCLogistics::ControlsVisible(const FTCMatchState& State) const { return bEnabled && !State.bSpectator; }

const FTCLogisticsReserve* FTCLogistics::FindReserve(int32 UnitId) const
{
	const int32* At = ReserveIndex.Find(UnitId);
	return At ? &ReserveRows[*At] : nullptr;
}

const FTCLogisticsTruck* FTCLogistics::FindTruck(int32 UnitId) const
{
	const int32* At = TruckIndex.Find(UnitId);
	return At ? &TruckRows[*At] : nullptr;
}

const FTCLogisticsStore* FTCLogistics::StoreForUnit(const FTCUnit& Unit) const
{
	if (!bEnabled) return nullptr;
	for (const FTCLogisticsStore& S : StoreRows)
		if (S.Owner == Unit.Owner && TCJs::Hypot(S.X - (double)Unit.X, S.Z - (double)Unit.Z) < 0.5) return &S;
	return nullptr;
}

bool FTCLogistics::RegionRate(int32 RegionId, FTCJsValue& Out) const
{
	const int32* At = RegionIndex.Find(RegionId);
	if (!At) return false;
	Out = RegionRows[*At].Value;
	return true;
}

bool FTCLogistics::RegionTint(int32 RegionId, FColor& Out) const
{
	// client/main.js minimap: rate !== undefined; rate <= 0 red, rate < 1 amber, else green
	FTCJsValue Rate;
	if (!OverlayActive() || !RegionRate(RegionId, Rate) || Rate.Kind == FTCJsValue::EKind::Missing) return false;
	const double N = Rate.ToNumber();
	Out = N <= 0 ? FColor(0xd4, 0x57, 0x4a, 0x55) : N < 1 ? FColor(0xe8, 0xc8, 0x60, 0x44) : FColor(0x95, 0xc8, 0x88, 0x33);
	return true;
}

TArray<FString> FTCLogistics::ReserveLabels(const FTCLogisticsReserve* Row)
{
	TArray<FString> L;
	if (!Row) return L;
	if (!Row->Ammo.IsNullish()) L.Add(TEXT("Ammunition: ") + JsStr(TCJs::Round(Row->Ammo.Amount() * 100)) + TEXT("%"));
	L.Add(TEXT("Provisions: ") + Seconds(Row->Provisions));
	if (!Row->Fuel.IsNullish()) L.Add(TEXT("Fuel: ") + Seconds(Row->Fuel));
	if (!Row->Emergency.IsNullish()) L.Add(TEXT("Emergency fuel: ") + Seconds(Row->Emergency));
	if (!Row->Warning.IsNullish() && !Row->Forced.Truthy()) L.Add(TEXT("Withdrawal in ") + JsStr(std::ceil(Row->Warning.Amount())) + TEXT(" s"));
	if (Row->Forced.Truthy()) L.Add(Row->Stranded.Truthy() ? TEXT("Stranded") : TEXT("Mandatory withdrawal"));
	// territory supply: the line's strength, the grace countdown once out of it, or cut off
	if (Row->Cut.Truthy()) L.Add(TEXT("Cut off from supply"));
	else if (!Row->Grace.IsNullish()) L.Add(TEXT("Out of supply: refill stops in ") + JsStr(std::ceil(Row->Grace.Amount())) + TEXT(" s"));
	else if (!Row->Supply.IsNullish()) L.Add(TEXT("Supply line: ") + JsStr(TCJs::Round(Row->Supply.Amount() * 100)) + TEXT("%"));
	return L;
}

TArray<FString> FTCLogistics::TruckLabels(const FTCLogisticsTruck* Row)
{
	TArray<FString> L;
	if (!Row) return L;
	const TCHAR* Job = JobText(Row->State);
	L.Add(Row->Hold.Truthy() ? TEXT("Deliveries held") : Row->Manual.Truthy() ? TEXT("Manual route") : Job ? Job : TEXT("Automatic deliveries"));
	L.Add(TEXT("Ammunition cargo: ") + Fixed1(Row->Cargo.Ammo.Amount()));
	L.Add(TEXT("Provisions cargo: ") + Seconds(Row->Cargo.Provisions));
	L.Add(TEXT("Fuel cargo: ") + Seconds(Row->Cargo.Fuel));
	if (Row->bDestination) L.Add(TEXT("Destination: ") + JsStr(TCJs::Round(Row->DestX)) + TEXT(", ") + JsStr(TCJs::Round(Row->DestZ)));
	return L;
}

TArray<FString> FTCLogistics::StoreLabels(const FTCLogisticsStore* Row)
{
	TArray<FString> L;
	if (!Row) return L;
	L.Add(Row->bSource ? TEXT("Supply source") : TEXT("Finite supply store"));
	auto Line = [&L](const TCHAR* Name, const FTCJsValue& Stock, const FTCJsValue& Capacity)
	{
		L.Add(FString(Name) + TEXT(": ") + JsStr(TCJs::Round(Stock.Amount())) + TEXT("/") + JsStr(TCJs::Round(Capacity.Amount())));
	};
	Line(TEXT("Ammunition stock"), Row->Stock.Ammo, Row->Capacity.Ammo);
	Line(TEXT("Provisions stock"), Row->Stock.Provisions, Row->Capacity.Provisions);
	Line(TEXT("Fuel stock"), Row->Stock.Fuel, Row->Capacity.Fuel);
	return L;
}

FString FTCLogistics::Indicator(const FTCLogisticsReserve* Row)
{
	if (!Row) return FString();
	if (Row->Stranded.Truthy()) return TEXT("Stranded");
	if (Row->Forced.Truthy()) return TEXT("Mandatory withdrawal");
	if (Row->Cut.Truthy()) return TEXT("Cut off from supply");
	const bool bLow = !Row->Warning.IsNullish() || (!Row->Ammo.IsNullish() && Row->Ammo.ToNumber() < 0.25) || Row->Provisions.ToNumber() < 30
		|| (!Row->Fuel.IsNullish() && Row->Fuel.ToNumber() < 45);
	return bLow ? TEXT("Low supplies") : FString();
}

FColor FTCLogistics::IndicatorColor(const FString& Indication)
{
	return Indication.Equals(TEXT("Low supplies"), ESearchCase::CaseSensitive) ? FColor(0xd6, 0xb2, 0x5e) : FColor(0xd4, 0x57, 0x4a);
}

TArray<int32> FTCLogistics::CutOffUnits(const FTCMatchState& State) const
{
	TArray<int32> Out;
	for (const TPair<int32, FTCUnit>& P : State.Units)
		if (const FTCLogisticsReserve* R = FindReserve(P.Key)) if (R->Cut.Truthy()) Out.Add(P.Key);
	Out.Sort();
	return Out;
}

FTCLogisticsCard FTCLogistics::Card(const FTCMatchState& State, const TArray<int32>& SelectedInOrder) const
{
	// hud.js drawLogistics: sel = the selected units held; trucks = every one a truck
	FTCLogisticsCard C;
	TArray<const FTCUnit*> Sel;
	for (int32 Id : SelectedInOrder) if (const FTCUnit* U = State.Units.Find(Id)) Sel.Add(U);
	C.bTrucks = Sel.Num() > 0 && !Sel.ContainsByPredicate([](const FTCUnit* U) { return U->Type != TEXT("truck"); });
	for (const FTCUnit* U : Sel)
	{
		FTCLogisticsEntry E;
		E.Id = U->Id; E.Type = U->Type;
		const FTCLogisticsReserve* R = FindReserve(U->Id);
		E.Lines = U->Type == TEXT("truck") ? TruckLabels(FindTruck(U->Id)) : R ? ReserveLabels(R) : StoreLabels(StoreForUnit(*U));
		if (E.Lines.Num()) C.Entries.Add(MoveTemp(E));
	}
	C.bVisible = C.Entries.Num() > 0;
	return C;
}

bool FTCLogistics::KnownByFog(const FTCMatchState& State, double X, double Z)
{
	// client/main.js knownSupplyPosition: fogOfWar.at(x, z) is 'seen' or 'explored' (null off the map)
	const double CX = std::floor(X / FTCMatchState::CellMetres), CY = std::floor(Z / FTCMatchState::CellMetres);
	if (!(CX >= 0 && CY >= 0 && CX < State.Map.W && CY < State.Map.H)) return false;
	const int32 Cell = (int32)CY * State.Map.W + (int32)CX;
	return State.CellVisible(Cell) || State.CellExplored(Cell);
}

FTCLogisticsOverlay FTCLogistics::Overlay(const FTCMatchState& State, const TSet<int32>& Selected, TFunctionRef<bool(double X, double Z)> Known) const
{
	FTCLogisticsOverlay O;
	if (!bEnabled) return O;
	// A route is drawn only if every segment is known. Never bridge an unknown section.
	constexpr double MaxSteps = 1 << 20; // a guard only: no reference map is a thousand kilometres across
	for (const FTCLogisticsTruck& T : TruckRows)
	{
		if (!Selected.Contains(T.Id)) continue;
		TArray<FTCRoutePoint> Points;
		const FTCUnit* U = State.Units.Find(T.Id);
		if (U && std::isfinite(U->X) && std::isfinite(U->Z)) { FTCRoutePoint P; P.bValid = true; P.X = U->X; P.Z = U->Z; Points.Add(P); }
		Points.Append(T.Route);
		bool bKnown = true;
		for (int32 I = 0; I < Points.Num() && bKnown; I++)
		{
			const FTCRoutePoint& P = Points[I];
			if (!P.bValid || !Known(P.X, P.Z)) { bKnown = false; break; }
			if (!I) continue;
			const FTCRoutePoint& A = Points[I - 1];
			const double Steps = std::ceil(TCJs::Hypot(P.X - A.X, P.Z - A.Z));
			if (Steps > MaxSteps) { bKnown = false; break; }
			for (double N = 1; N < Steps; N++)
				if (!Known(A.X + (P.X - A.X) * N / Steps, A.Z + (P.Z - A.Z) * N / Steps)) { bKnown = false; break; }
		}
		if (Points.Num() > 1 && bKnown)
		{
			FTCSupplyRoute R;
			R.Id = T.Id; R.bManual = T.Manual.Truthy(); R.bHold = T.Hold.Truthy();
			for (const FTCRoutePoint& P : Points) R.Points.Add(FVector2D(P.X, P.Z));
			O.Routes.Add(MoveTemp(R));
		}
	}
	for (const FTCLogisticsStore& S : StoreRows) if (Known(S.X, S.Z)) O.Stores.Add(S);
	return O;
}

// ---------- controls ----------

bool FTCLogistics::BoxSelectable(const FTCMatchState& State, const FTCUnitTable& Table, const FTCUnit& Unit, bool bLogisticsMode)
{
	// client/selection.js: own (and alive), mobileOwn (not a structure, not a plane at base, not riding), armyUnit
	const FTCUnitDef* D = Table.Find(Unit.Type);
	if (!D || !State.IsOwn(Unit) || Unit.Hp <= 0 || D->bStructure || (Unit.Flags & (TCClientFlags::PlaneAtBase | TCFlags::Riding))) return false;
	const bool bTruck = Unit.Type == TEXT("truck");
	return bLogisticsMode ? bTruck : !bTruck && !D->bRail;
}

FTCLogisticsOrder FTCLogistics::PlanSupply(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bQueue)
{
	FTCLogisticsOrder O;
	// client/orders.js selection: troops are the own selected units that are not structures; a selected own
	// Production Building (any building whose type makes units, finished or not) gets a rally from the same click.
	// canUseBuilding (shared/sim.js productionAccess) adds allies' buildings only in Horde, which has no logistics.
	TArray<int32> Troops, Trucks;
	bool bProductionBuilding = false;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		const FTCUnitDef* D = U ? Table.Find(U->Type) : nullptr;
		if (!U || !D) continue;
		if (D->bStructure)
		{
			const FTCRecruitDef* R = D->bBuilding ? FTCRecruitment::Find(U->Type) : nullptr;
			if (R && R->Makes.Num() && State.IsOwn(*U)) bProductionBuilding = true;
			continue;
		}
		if (!State.IsOwn(*U)) continue;
		Troops.Add(Id);
		if (U->Type == TEXT("truck")) Trucks.Add(Id);
	}
	// kind: trucks.length === troops.length && (cursor.friend || b) gives 'supply'
	if (!Troops.Num() || Trucks.Num() != Troops.Num() || (!Cursor.Friend && !Cursor.Building)) return O;
	const int32 TargetId = Cursor.Friend ? Cursor.Friend : Cursor.Building; // at = cursor.friend ?? b
	const FTCUnit* T = State.Units.Find(TargetId);
	if (!T) { O.Why = FString::Printf(TEXT("unit %d under the cursor is no longer held"), TargetId); return O; }
	if (bProductionBuilding && Cursor.bHasGround)
	{
		// dispatch adds { t: 'rally' } for the selected buildings and sends { t: 'orders', commands: [supply, rally] };
		// shared/sim.js dispatchOrderGroups does not carry 'supply', so the browser's click only sets the rally.
		O.Kind = ETCLogisticsOrderKind::Unsupported;
		O.Why = TEXT("a rally point for the selected building (the reference drops the trucks' supply part of this click)");
		return O;
	}
	Trucks.Sort(); // the browser lists them in selection order; the server treats each truck alike
	TArray<TSharedPtr<FJsonValue>> Ids;
	for (int32 Id : Trucks) Ids.Add(MakeShared<FJsonValueNumber>(Id));
	TSharedRef<FJsonObject> Msg = MakeShared<FJsonObject>();
	Msg->SetStringField(TEXT("t"), TEXT("supply"));
	Msg->SetArrayField(TEXT("ids"), Ids);
	Msg->SetNumberField(TEXT("target"), TargetId);
	Msg->SetBoolField(TEXT("queue"), bQueue);
	O.Kind = ETCLogisticsOrderKind::Supply;
	O.Message = Msg;
	O.Trucks = MoveTemp(Trucks);
	O.Target = TargetId;
	// shared/supply-convoys.js commandConvoy: relief = targetUnit.type === 'supplycache' && targetUnit.owner !== slot
	O.bRelief = T->Type == TEXT("supplycache") && T->Owner != State.You;
	O.At = FVector2D(T->X, T->Z);
	return O;
}

bool FTCLogistics::ResumeShown(const FTCMatchState& State, const TSet<int32>& Selected)
{
	int32 Held = 0;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (!U) continue;
		if (U->Type != TEXT("truck")) return false;
		Held++;
	}
	return Held > 0;
}

FTCLogisticsOrder FTCLogistics::PlanResume(const FTCMatchState& State, const TSet<int32>& Selected)
{
	FTCLogisticsOrder O;
	if (!ResumeShown(State, Selected)) return O;
	for (int32 Id : Selected)
		if (const FTCUnit* U = State.Units.Find(Id)) if (State.IsOwn(*U)) O.Trucks.Add(Id);
	if (!O.Trucks.Num()) { O.Why = TEXT("none of the selected trucks is this client's own"); return O; }
	O.Trucks.Sort();
	TArray<TSharedPtr<FJsonValue>> Ids;
	for (int32 Id : O.Trucks) Ids.Add(MakeShared<FJsonValueNumber>(Id));
	TSharedRef<FJsonObject> Msg = MakeShared<FJsonObject>();
	Msg->SetStringField(TEXT("t"), TEXT("logisticsResume"));
	Msg->SetArrayField(TEXT("ids"), Ids);
	O.Kind = ETCLogisticsOrderKind::Resume;
	O.Message = Msg;
	return O;
}
