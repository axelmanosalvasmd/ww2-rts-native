#include "Client/TCFieldworks.h"
#include "Client/TCRustQueries.h"
#include "Client/TCOrders.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include <cmath>
#include <cstring>
#include <limits>

// The generated sight and terrain tables (shared/sim.js UNITS vision and flags, TERRAIN, CELL, CFG.eye and the like).
#include "Client/TCConstructionData.inl"

// ---------- the reference rules that need no engine (a scratch harness compiles this block alone) ----------
// TCFIELDWORKS-PURE-BEGIN
namespace TCFieldPure
{
	constexpr double Cell = 2.0;                   // shared/sim.js CELL
	constexpr double Pi = 3.141592653589793;       // Math.PI
	constexpr double NaN = std::numeric_limits<double>::quiet_NaN();
	constexpr double Inf = std::numeric_limits<double>::infinity();

	// shared/sim.js FORTS at the pin, in its order, with client/hud.js FORT_TIP (CFG.mine.detect 6, CFG.aid.radius 12
	// written in) and FORT_TAB, and client/keys.js FORT_KEYS and FORT_BADGES. Ch 0: no ch; On 0: no on; Reach < 0: none.
	struct FFort
	{
		const char* Kind; const char* Name; int Cost; char Ch; int N;
		bool bNest, bSingle, bPoint, bFill, bClear, bAlong;
		char On; double Reach, Range; int Time; const char* Unit; int Tier;
		const char* Tab; const char* Key; const char* Badge; const char* Tip;
	};
	constexpr FFort Forts[] = {
		{ "trench", "Trench", 30, 'T', 4, false, false, false, false, false, false, 0, -1, 0, 0, "", 0, "field", "T", "T", "Heavy cover for infantry" },
		{ "sandbags", "Sandbags", 20, '#', 4, false, false, false, false, false, false, 0, -1, 0, 0, "", 0, "field", "Shift+Y", "\xE2\x87\xA7Y", "Cover for infantry" },
		{ "wire", "Barbed Wire", 25, 'X', 5, false, false, false, false, false, false, 0, -1, 0, 0, "", 0, "field", "Shift+U", "\xE2\x87\xA7U", "Slows infantry; tanks flatten it" },
		{ "traps", "Tank Traps", 40, 'Y', 4, false, false, false, false, false, false, 0, -1, 0, 0, "", 0, "field", "Shift+I", "\xE2\x87\xA7I", "Stops vehicles; cover for infantry" },
		{ "nest", "Gun Pit", 60, 0, 0, true, false, false, false, false, false, 0, -1, 1.25, 0, "", 0, "defense", "Shift+O", "\xE2\x87\xA7O",
			"A trench pit behind a horseshoe of sandbags. An MG, AT gun, mortar, flak gun or howitzer standing in the pit reaches 25% farther" },
		{ "mines", "Minefield", 40, 'N', 4, false, false, false, false, false, false, 0, -1, 0, 0, "", 0, "field", "Shift+J", "\xE2\x87\xA7J", "Hidden from the enemy; goes off under the first enemy squad or vehicle" },
		{ "bridge", "Bridge", 80, '=', 5, false, false, false, false, false, true, 'W', 9, 0, 0, "", 0, "eng", "Shift+K", "\xE2\x87\xA7K", "Across a river, up to 5 cells; aim it along the crossing" },
		{ "fill", "Fill in", 10, '.', 5, false, false, false, true, false, false, 0, -1, 0, 0, "", 0, "eng", "Shift+L", "\xE2\x87\xA7L",
			"Shovels craters, flooded craters and sunken ground back to open ground, and clears rubble (a road gets its road back)" },
		{ "demine", "Clear mines", 5, '.', 4, false, false, false, false, true, false, 0, 8, 0, 0, "", 0, "eng", "Shift+M", "\xE2\x87\xA7M",
			"Lifts the mines your side knows about: your own, and enemy ones a builder squad found by standing within 6 m" },
		{ "aid", "Field Hospital", 100, 'A', 1, false, true, false, false, false, false, 0, -1, 0, 12, "", 0, "eng", "", "",
			"Infantry within 12 m reinforce for manpower, at half the HQ's pace. One per player" },
		{ "wall", "Concrete Wall", 120, 0, 4, false, false, false, false, false, false, 0, 6, 0, 6, "wall", 2, "defense", "", "",
			"Concrete, Engineers only: stops infantry and vehicles and blocks sight; explosives breach it a block at a time" },
		{ "gate", "Gate", 150, 0, 3, false, false, true, false, false, false, 0, 6, 0, 10, "gate", 2, "defense", "", "",
			"Three cells of wall that stand open for your side and allies, and shut while an enemy is within 16 m" },
	};
	constexpr int FortCount = (int)(sizeof(Forts) / sizeof(Forts[0]));
	constexpr int TrenchFort = 0, WireFort = 2;
	constexpr const char* BuildableGround = ".+RDM";     // shared/sim.js BUILDABLE_GROUND
	constexpr const char* FortBuilders[] = { "rifle", "conscript", "engineer" }; // CFG.fortBuilders

	// shared/sim.js ENTRENCH and ENTRENCH_TYPES, client/hud.js ENTRENCH_TIP
	struct FPattern { const char* Kind; const char* Name; const char* Tip; };
	constexpr FPattern Patterns[] = {
		{ "line", "Trench line", "One straight trench from the first click to the second" },
		{ "zigzag", "Zigzag trench", "A sawtooth trench: more room on the same frontage" },
		{ "double", "Double line", "Two rows, the second 6 m behind the first" },
		{ "arc", "Arc", "A crescent around the first click, bowed toward the second" },
		{ "ring", "Ring", "A circle around the first click, out to the second" },
		{ "strongpoint", "Strongpoint", "A trench square with barbed wire on the side of the second click" },
	};
	constexpr int PatternCount = (int)(sizeof(Patterns) / sizeof(Patterns[0]));
	enum EPattern { Line = 0, Zigzag = 1, Double = 2, Arc = 3, Ring = 4, Strongpoint = 5 };

	inline int FortIndex(const char* Kind) { for (int I = 0; I < FortCount; I++) if (!std::strcmp(Forts[I].Kind, Kind)) return I; return -1; }
	inline int PatternIndex(const char* Kind) { for (int I = 0; I < PatternCount; I++) if (!std::strcmp(Patterns[I].Kind, Kind)) return I; return -1; }
	// shared/sim.js lineFort
	inline bool LineFort(int K) { return K >= 0 && Forts[K].N > 0 && !Forts[K].On && !Forts[K].bSingle && !Forts[K].bPoint; }

	// Math.max and Math.min of two numbers: NaN wins
	inline double JsMax(double A, double B) { return A != A || B != B ? NaN : A > B ? A : B; }
	inline double JsMin(double A, double B) { return A != A || B != B ? NaN : A < B ? A : B; }
	// Math.round: halves go up
	// Math.round: halves go up, and a result of zero keeps the argument's sign (Math.round(-0.3) is -0)
	inline double JsRound(double V) { const double F = std::floor(V); const double R = V - F >= 0.5 ? F + 1 : F; return R == 0 ? std::copysign(0.0, V) : R; }
	// V8's Math.hypot for two numbers (src/builtins/math.tq MathHypot): Infinity wins over NaN, then the values scaled by
	// the larger one are summed with Kahan compensation.
	inline double JsHypot(double A, double B)
	{
		const bool bNaN = A != A || B != B;
		A = std::fabs(A); B = std::fabs(B);
		double Max = 0;
		if (A == A && A > Max) Max = A;
		if (B == B && B > Max) Max = B;
		if (Max == Inf) return Inf;
		if (bNaN) return NaN;
		if (Max == 0) return 0;
		double Sum = 0, Compensation = 0;
		const double Values[2] = { A, B };
		for (const double V : Values)
		{
			const double N = V / Max;
			const double Summand = N * N - Compensation;
			const double Preliminary = Sum + Summand;
			Compensation = (Preliminary - Sum) - Summand;
			Sum = Preliminary;
		}
		return std::sqrt(Sum) * Max;
	}
	// shared/sim.js squareDir: a gate runs straight along the grid
	inline double SquareDir(double A) { return JsRound(A / (Pi / 2)) * (Pi / 2); }
	// shared/sim.js segmentCost (the Gun Pit has no n: JavaScript's NaN, here -1)
	inline int SegmentCost(int K, int Cells)
	{
		if (K < 0 || Forts[K].N <= 0) return -1;
		return (int)JsMax(1, JsRound((double)Forts[K].Cost * Cells / Forts[K].N));
	}
	// shared/sim.js cellOf: -1 off the map (and for NaN, which JavaScript leaves as NaN: every caller asks c >= 0)
	inline long long CellOf(double X, double Z, int W, int H)
	{
		const double CX = std::floor(X / Cell), CY = std::floor(Z / Cell);
		if (!(CX >= 0) || !(CY >= 0) || CX >= W || CY >= H) return -1;
		return (long long)CY * W + (long long)CX;
	}

	struct FSeg { int Kind; double X, Z, Dir, Front; };
	struct FSegs { FSeg S[48]; int N = 0; };
	inline void Push(FSegs& O, int K, double X, double Z, double D, double Front) { if (O.N < 48) O.S[O.N++] = FSeg{ K, X, Z, D, Front }; }

	// shared/sim.js entrenchPlan(pattern, a, b, back, fort). Fort must be a line fort (FORTS[fort].n > 0).
	inline void EntrenchPlan(int Pattern, double AX, double AZ, double BX, double BZ, bool bBack, double BackX, double BackZ, int Fort, FSegs& Out)
	{
		Out.N = 0;
		if (Fort < 0 || Forts[Fort].N <= 0 || Pattern < 0 || Pattern >= PatternCount) return;
		const double Seg = Forts[Fort].N * Cell, Len = JsHypot(BX - AX, BZ - AZ);
		const bool bDrawn = Len >= 2, bLinear = Pattern == Line || Pattern == Zigzag || Pattern == Double;
		const double Away = bBack && JsHypot(AX - BackX, AZ - BackZ) > 0.5 ? std::atan2(AZ - BackZ, AX - BackX) : 0;
		const double Dir = bDrawn ? std::atan2(BZ - AZ, BX - AX) : bLinear ? Away + Pi / 2 : Away;
		const double DX = std::cos(Dir), DZ = std::sin(Dir);
		auto Clamp = [](double V, double Lo, double Hi) { return JsMax(Lo, JsMin(Hi, V)); };
		// seg(kind, x, z, d, front = Math.atan2(z - a.z, x - a.x))
		auto SegAt = [&](int K, double X, double Z, double D) { Push(Out, K, X, Z, D, std::atan2(Z - AZ, X - AX)); };
		if (bLinear)
		{
			const double MX = (AX + BX) / 2, MZ = (AZ + BZ) / 2, L = bDrawn ? Len : Seg;
			// the front is the side away from the diggers
			const double Side = bBack && (MX - BackX) * DZ + (MZ - BackZ) * -DX < 0 ? -1 : 1;
			const double NX = DZ * Side, NZ = -DX * Side, Front = std::atan2(NZ, NX);
			auto Row = [&](double Off)
			{
				const double N = Clamp(JsRound(L / Seg), 1, 12);
				for (int I = 0; I < N; I++)
				{
					const double T = (I - (N - 1) / 2) * Seg;
					Push(Out, Fort, MX + DX * T + NX * Off, MZ + DZ * T + NZ * Off, Dir, Front);
				}
			};
			if (Pattern == Zigzag)
			{
				// a sawtooth: each segment turns 0.6 rad off the line, alternately forward and back
				const double Wd = Seg * std::cos(0.6), Hd = Seg * std::sin(0.6), N = Clamp(JsRound(L / Wd), 1, 16);
				auto V = [&](int I, double& X, double& Z)
				{
					X = MX + DX * (I - N / 2) * Wd + NX * (I % 2 ? Hd / 2 : -Hd / 2);
					Z = MZ + DZ * (I - N / 2) * Wd + NZ * (I % 2 ? Hd / 2 : -Hd / 2);
				};
				for (int I = 0; I < N; I++)
				{
					double PX, PZ, QX, QZ;
					V(I, PX, PZ); V(I + 1, QX, QZ);
					Push(Out, Fort, (PX + QX) / 2, (PZ + QZ) / 2, std::atan2(QZ - PZ, QX - PX), Front);
				}
			}
			else { Row(0); if (Pattern == Double) Row(-3 * Cell); }
		}
		else if (Pattern == Arc || Pattern == Ring)
		{
			const bool bRing = Pattern == Ring;
			const double R = bDrawn ? Clamp(Len, bRing ? 6 : 8, bRing ? 24 : 30) : bRing ? 10 : 12, Span = bRing ? Pi * 2 : Pi * 2 / 3;
			const double N = Clamp(JsRound(R * Span / Seg), bRing ? 4 : 2, 20), Step = bRing ? Span / N : Seg / R;
			for (int I = 0; I < N; I++)
			{
				const double T = Dir + (I - (N - 1) / 2) * Step;
				SegAt(TrenchFort, AX + std::cos(T) * R, AZ + std::sin(T) * R, T + Pi / 2);
			}
		}
		else if (Pattern == Strongpoint)
		{
			// a trench square around a, with barbed wire across the side it faces
			const double Hh = Seg / 2 - Cell / 2, PX = -DZ, PZ = DX;
			SegAt(TrenchFort, AX + DX * Hh, AZ + DZ * Hh, Dir + Pi / 2); SegAt(TrenchFort, AX - DX * Hh, AZ - DZ * Hh, Dir + Pi / 2);
			SegAt(TrenchFort, AX + PX * Hh, AZ + PZ * Hh, Dir); SegAt(TrenchFort, AX - PX * Hh, AZ - PZ * Hh, Dir);
			const double F = Hh + 3 * Cell, Wl = (double)Forts[WireFort].N * Cell / 2;
			SegAt(WireFort, AX + DX * F + PX * Wl, AZ + DZ * F + PZ * Wl, Dir + Pi / 2); SegAt(WireFort, AX + DX * F - PX * Wl, AZ + DZ * F - PZ * Wl, Dir + Pi / 2);
			return; // trenches before wire
		}
		if (!Out.N) return;
		// middle first: by the distance to the segments' centre, then the plan's order (a stable sort)
		double SX = 0, SZ = 0;
		for (int I = 0; I < Out.N; I++) SX = SX + Out.S[I].X;
		for (int I = 0; I < Out.N; I++) SZ = SZ + Out.S[I].Z;
		const double CX = SX / Out.N, CZ = SZ / Out.N;
		double D[48];
		int Order[48];
		for (int I = 0; I < Out.N; I++) { D[I] = JsHypot(Out.S[I].X - CX, Out.S[I].Z - CZ); Order[I] = I; }
		for (int I = 1; I < Out.N; I++)
		{
			const int K = Order[I];
			int J = I - 1;
			while (J >= 0 && (D[Order[J]] > D[K] || (D[Order[J]] == D[K] && Order[J] > K))) { Order[J + 1] = Order[J]; J--; }
			Order[J + 1] = K;
		}
		FSegs Sorted;
		for (int I = 0; I < Out.N; I++) Sorted.S[I] = Out.S[Order[I]];
		Sorted.N = Out.N;
		Out = Sorted;
	}

	// The cells a fortification's plan covers when centred on (X, Z) running along Dir (shared/sim.js placementCheck's
	// plan and fortCells' plan, which are the same cells), with the character each becomes; -1 off the map.
	struct FPlanCell { long long C; char Ch; };
	inline int FortPlan(int K, double X, double Z, double Dir, int W, int H, FPlanCell Out[8])
	{
		const FFort& F = Forts[K];
		if (F.bPoint) Dir = SquareDir(Dir);
		const double SX = std::cos(Dir), SZ = std::sin(Dir), FX = SZ, FZ = -SX; // forward: away from the builders
		auto At = [&](double Along, double Fwd) { return CellOf(X + (SX * Along + FX * Fwd) * Cell, Z + (SZ * Along + FZ * Fwd) * Cell, W, H); };
		int N = 0;
		if (F.bNest)
		{
			const double Pit[6][2] = { { 0, 0 }, { -1, 1 }, { 0, 1 }, { 1, 1 }, { -1, 0 }, { 1, 0 } };
			for (int I = 0; I < 6; I++) Out[N++] = FPlanCell{ At(Pit[I][0], Pit[I][1]), I == 0 ? 'T' : '#' };
		}
		else for (int I = 0; I < F.N && I < 8; I++) Out[N++] = FPlanCell{ At(I - (F.N - 1) / 2.0, 0), F.Ch };
		return N;
	}
}
// TCFIELDWORKS-PURE-END

const TCHAR* const FTCFieldworks::ReferencePin = TEXT("e4e579a");

static_assert(TCFieldPure::Cell == FTCMatchState::CellMetres, "CELL differs from FTCMatchState::CellMetres");
static_assert(TCConstructionData::Cell == TCFieldPure::Cell, "the generated CELL differs from FORTS' CELL");

// Helpers in a named namespace (a unity build puts this file in one translation unit with others that have their own).
namespace TCField
{
	namespace Data = ::TCConstructionData;
	using namespace TCFieldPure;
	constexpr int32 RetreatingFlag = 1;   // a unit row's flag bit 1 (client/main.js diggers, availability)
	constexpr int32 DigFlag = 16;         // shared/sim.js unitFlags: the squad has a dig
	constexpr int32 EntrenchFlag = 1024;  // shared/sim.js unitFlags: the squad is on a mass entrenchment

	struct FTables
	{
		TArray<FTCFortDef> Defs;
		TArray<FTCEntrenchDef> Patterns;
		TArray<TPair<FString, FString>> Tabs;
		TMap<FString, const Data::FSightRow*> Sight;
		int32 Flags[128] = {};
		FTables()
		{
			for (int I = 0; I < FortCount; I++)
			{
				const FFort& R = Forts[I];
				FTCFortDef D;
				D.Kind = UTF8_TO_TCHAR(R.Kind); D.Name = UTF8_TO_TCHAR(R.Name); D.Cost = R.Cost; D.Ch = (TCHAR)R.Ch; D.N = R.N;
				D.bNest = R.bNest; D.bSingle = R.bSingle; D.bPoint = R.bPoint; D.bFill = R.bFill; D.bClear = R.bClear; D.bAlong = R.bAlong;
				D.On = (TCHAR)R.On; D.Reach = R.Reach; D.Range = R.Range; D.Time = R.Time; D.Unit = UTF8_TO_TCHAR(R.Unit); D.Tier = R.Tier;
				D.bLine = LineFort(I);
				D.Tab = UTF8_TO_TCHAR(R.Tab); D.KeyLabel = UTF8_TO_TCHAR(R.Key); D.Badge = UTF8_TO_TCHAR(R.Badge); D.Tip = UTF8_TO_TCHAR(R.Tip);
				Defs.Add(MoveTemp(D));
			}
			for (int I = 0; I < PatternCount; I++) Patterns.Add(FTCEntrenchDef{ UTF8_TO_TCHAR(TCFieldPure::Patterns[I].Kind), UTF8_TO_TCHAR(TCFieldPure::Patterns[I].Name), UTF8_TO_TCHAR(TCFieldPure::Patterns[I].Tip) });
			// client/hud.js BUILD_TABS
			Tabs.Add({ TEXT("base"), TEXT("Base") }); Tabs.Add({ TEXT("defense"), TEXT("Defenses") });
			Tabs.Add({ TEXT("field"), TEXT("Field works") }); Tabs.Add({ TEXT("eng"), TEXT("Engineering") });
			for (const Data::FSightRow& R : Data::Sight) Sight.Add(R.Type, &R);
			for (const Data::FTerrainRow& R : Data::Terrain) if (R.Ch < 128) Flags[R.Ch] = R.Flags;
		}
	};
	const FTables& Tables() { static const FTables T; return T; }
	// The reference compares its keys and types case-sensitively; FString's == and TMap<FString> do not.
	bool Eq(const FString& A, const TCHAR* B) { return A.Equals(B, ESearchCase::CaseSensitive); }
	bool Eq(const FString& A, const FString& B) { return A.Equals(B, ESearchCase::CaseSensitive); }
	const Data::FSightRow* SightOf(const FString& Type) { const Data::FSightRow* const* R = Tables().Sight.Find(Type); return R && Eq(Type, (*R)->Type) ? *R : nullptr; }
	int KindIndex(const FString& Kind) { const TArray<FTCFortDef>& D = Tables().Defs; for (int32 I = 0; I < D.Num(); I++) if (Eq(Kind, D[I].Kind)) return I; return -1; }
	bool IsFortBuilder(const FString& Type) { for (const char* T : TCFieldPure::FortBuilders) if (Eq(Type, UTF8_TO_TCHAR(T))) return true; return false; }

	bool Num(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return true;
	}
	double NumOr(const TSharedPtr<FJsonValue>& V, double Or) { double D; return Num(V, D) ? D : Or; }
	bool Nullish(const TSharedPtr<FJsonValue>& V) { return !V.IsValid() || V->Type == EJson::None || V->Type == EJson::Null; }
	// JavaScript truthiness of a JSON value (a missing one is undefined: false)
	bool Truthy(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid()) return false;
		switch (V->Type)
		{
		case EJson::Boolean: return V->AsBool();
		case EJson::Number: { const double D = V->AsNumber(); return D != 0 && !FMath::IsNaN(D); }
		case EJson::String: return !V->AsString().IsEmpty();
		case EJson::Array: case EJson::Object: return true;
		default: return false;
		}
	}
	// A layer value as FTCFieldworks keeps it: 0 undefined or null, 1 the empty string, the character, 255 anything else
	constexpr uint8 LayerNullish = 0, LayerEmpty = 1, LayerOther = 255;
	uint8 LayerCode(const TSharedPtr<FJsonValue>& V)
	{
		if (Nullish(V)) return LayerNullish;
		if (V->Type != EJson::String) return LayerOther;
		const FString S = V->AsString();
		return S.IsEmpty() ? LayerEmpty : S.Len() == 1 && S[0] > 1 && S[0] < 128 ? (uint8)S[0] : LayerOther;
	}
	uint8 CharCode(TCHAR Ch) { return Ch > 1 && Ch < 128 ? (uint8)Ch : LayerOther; }
	// a single-character string's membership in a reference character set ('WF'.includes(ch)); never the terminator
	bool InSet(const char* Set, uint8 Ch) { if (!Ch) return false; for (const char* P = Set; *P; P++) if ((uint8)*P == Ch) return true; return false; }
	// shared/sim.js levelOf(ch): a digit is a level, from 'a' on 96 - code; anything else is +ch (whitespace 0, else NaN)
	double JsLevelOf(TCHAR Ch)
	{
		if (Ch >= TEXT('a')) return 96.0 - (double)Ch;
		if (Ch >= TEXT('0') && Ch <= TEXT('9')) return (double)(Ch - TEXT('0'));
		if (Ch == TEXT(' ') || Ch == TEXT('\t') || Ch == TEXT('\n') || Ch == 0x0B || Ch == 0x0C || Ch == TEXT('\r') || Ch == 0xA0) return 0;
		return NaN;
	}
	FColor Rgb(uint32 C) { return FColor((uint8)((C >> 16) & 255), (uint8)((C >> 8) & 255), (uint8)(C & 255)); }
	// client/main.js TILE_COLOR[kind] ?? 0x60e070
	FColor TileColor(const FString& Kind)
	{
		if (Eq(Kind, TEXT("wire"))) return Rgb(0xd2a849);
		if (Eq(Kind, TEXT("sandbags"))) return Rgb(0xd8c79a);
		if (Eq(Kind, TEXT("traps"))) return Rgb(0xa9b0b8);
		if (Eq(Kind, TEXT("mines"))) return Rgb(0xd0604a);
		if (Eq(Kind, TEXT("fill"))) return Rgb(0x9a7b55);
		if (Eq(Kind, TEXT("demine"))) return Rgb(0x8fc7e8);
		if (Eq(Kind, TEXT("aid"))) return Rgb(0xf0ece0);
		return Rgb(0x60e070);
	}
	FTCFieldCheck Yes() { FTCFieldCheck C; C.bOk = true; return C; }
	FTCFieldCheck No(const FString& Reason, bool bNative = false) { FTCFieldCheck C; C.Reason = Reason; C.bNative = bNative; return C; }
	FString Deny(const TCHAR* Code) { return TCOrders::DenySentence(Code, FString()); }
	TArray<TSharedPtr<FJsonValue>> IdArray(const TArray<int32>& Ids)
	{
		TArray<TSharedPtr<FJsonValue>> Out;
		for (int32 Id : Ids) Out.Add(MakeShared<FJsonValueNumber>(Id));
		return Out;
	}

// ---------- the placement view (client/availability.js placementState, shared/sim.js placementCheck's FORTS branch) ----------

struct FView
{
	using FRow = FTCFieldworks::FRow;
	using FTeam = FTCFieldworks::FTeam;
	const FTCFieldworks& F;
	const FTCMatchState& S;
	int32 W, H;
	int64 N;
	FTeam Team;                     // teams[me] ?? me
	TArray<const FRow*> Units;      // g.units, in the browser's row order
	bool bThrew = false;            // the browser would have thrown

	FView(const FTCFieldworks& InF, const FTCMatchState& InS) : F(InF), S(InS), W(InF.W), H(InF.H), N((int64)InF.W * InF.H)
	{
		Team = F.MyTeam(S);
		for (int32 Id : F.RowOrder) if (const FRow* R = F.Rows.Find(Id)) Units.Add(R);
	}

	uint8 Char(int64 C) const { return C >= 0 && C < N ? S.Map.Chars[C] : 0; }
	uint8 GroundAt(int64 C) const { return F.Ground.IsValidIndex(C) ? F.Ground[C] : LayerNullish; }
	// g.flags[c]: TERRAIN of the cell, of its composed layers when it is 'N'
	int32 Flags(int64 C) const
	{
		if (C < 0 || C >= N) return 0;
		uint8 Ch = S.Map.Chars[C];
		if (Ch == 'N')
		{
			// shared/world-layers.js composeWorldCell(ground, object): the object when it is truthy and not '.', else the ground
			const uint8 Object = F.Objects.IsValidIndex(C) ? F.Objects[C] : LayerNullish;
			Ch = Object != LayerNullish && Object != LayerEmpty && Object != '.' ? Object : GroundAt(C);
		}
		return Ch > 1 && Ch < 128 ? Tables().Flags[Ch] : 0;
	}
	// level(g, c): 0 before the map, undefined (NaN) past it
	double Level(int64 C) const { return C < 0 ? 0 : C >= N ? NaN : (double)S.Map.Levels[C]; }
	int64 CellOf(double X, double Z) const { return TCFieldPure::CellOf(X, Z, W, H); }
	FVector2D CellCenter(int64 C) const { return FVector2D((C % W + 0.5) * Cell, (std::floor((double)C / W) + 0.5) * Cell); }

	// shared/sim.js blocker (the grid ray walk), true when the line is clear
	bool Clear(double X0, double Z0, double X1, double Z1, int32 Mask, bool bWithEnd) const
	{
		int64 CX = (int64)std::floor(X0 / Cell), CY = (int64)std::floor(Z0 / Cell);
		const int64 EX = (int64)std::floor(X1 / Cell), EY = (int64)std::floor(Z1 / Cell);
		const double DX = X1 - X0, DZ = Z1 - Z0;
		const double SX = DX > 0 ? 1 : DX < 0 ? -1 : DX, SY = DZ > 0 ? 1 : DZ < 0 ? -1 : DZ;
		const double TDX = DX ? Cell / std::fabs(DX) : Inf, TDY = DZ ? Cell / std::fabs(DZ) : Inf;
		double TX = DX ? (SX > 0 ? (CX + 1) * Cell - X0 : X0 - CX * Cell) / std::fabs(DX) : Inf;
		double TY = DZ ? (SY > 0 ? (CY + 1) * Cell - Z0 : Z0 - CY * Cell) / std::fabs(DZ) : Inf;
		int32 Wood = 0;
		for (int64 Steps = (int64)W + H; Steps > 0 && !(CX == EX && CY == EY); Steps--)
		{
			if (CY == EY || (CX != EX && TX < TY)) { TX += TDX; CX += (int64)SX; } else { TY += TDY; CY += (int64)SY; }
			const bool bEnd = CX == EX && CY == EY;
			if (bEnd && !bWithEnd) return true;
			if (CX < 0 || CY < 0 || CX >= W || CY >= H) return false;
			const int32 Fl = Flags(CY * W + CX);
			if (Fl & Mask) return false;
			if ((Mask & Data::SightFlag) && (Fl & Data::WoodFlag) && ++Wood > Data::WoodSight) return false;
		}
		return true;
	}
	// shared/sim.js segHits, smoked, hillsClear, los
	static bool SegHits(double AX, double AZ, double BX, double BZ, double CX, double CZ, double R)
	{
		const double DX = BX - AX, DZ = BZ - AZ, L2Raw = DX * DX + DZ * DZ, L2 = L2Raw != 0 && !FMath::IsNaN(L2Raw) ? L2Raw : 1;
		const double T = JsMax(0.0, JsMin(1.0, ((CX - AX) * DX + (CZ - AZ) * DZ) / L2));
		return JsHypot(AX + DX * T - CX, AZ + DZ * T - CZ) < R;
	}
	bool Smoked(double AX, double AZ, double BX, double BZ) const
	{
		if (!F.Smokes.Num() || !(JsHypot(BX - AX, BZ - AZ) >= Data::SmokeSight)) return false;
		for (const FTCFieldworks::FSmoke& Sm : F.Smokes) if (SegHits(AX, AZ, BX, BZ, Sm.X, Sm.Z, Sm.R)) return true;
		return false;
	}
	bool HillsClear(double AX, double AZ, double BX, double BZ) const
	{
		const double L = Data::LevelHeight, YA = Level(CellOf(AX, AZ)) * L + Data::Eye, YB = Level(CellOf(BX, BZ)) * L + Data::Eye;
		const double D = JsHypot(BX - AX, BZ - AZ), Steps = std::ceil(D / (Cell / 2));
		for (double I = 1; I < Steps; I++)
		{
			const double T = I / Steps;
			if (Level(CellOf(AX + (BX - AX) * T, AZ + (BZ - AZ) * T)) * L > YA + (YB - YA) * T) return false;
		}
		return true;
	}
	bool Los(const FRow& U, double X, double Z) const { return Clear(U.X, U.Z, X, Z, Data::SightFlag, false) && !Smoked(U.X, U.Z, X, Z) && HillsClear(U.X, U.Z, X, Z); }
	// view.sees(me, at): teamSees(g, teams[me] ?? me, at) over the received rows. A row of a seat outside the view's players
	// (World's -1) throws, as does an unknown type's sight, when the loop reaches it.
	bool Sees(const FVector2D& At)
	{
		if (bThrew) return false;
		for (const FRow* U : Units)
		{
			bool bSeat = false;
			const FTeam Of = F.TeamOfSeat(U->Owner, bSeat);
			if (!bSeat) { bThrew = true; return false; }
			if (!FTCFieldworks::TeamsEqual(Of, Team)) continue;
			const double D = JsHypot(U->X - At.X, U->Z - At.Y);
			const Data::FSightRow* Def = SightOf(U->Type);
			if (!Def) { bThrew = true; return false; }
			// visionOf: a lookout sees 12 m until it is built; the view has no weather and no air state
			const double Vision = Def->bLookout && U->Built < 1 ? 12 : Def->Vision;
			if (D <= Vision && (Def->bBuilding || Los(*U, At.X, At.Y))) return true;
		}
		return false;
	}

	// shared/sim.js fillable on the view: rubble, a crater, battle-made rubble, a flooded crater, sunken open ground
	bool Fillable(int64 C) const
	{
		const uint8 Ch = Char(C);
		const uint8 Initial = F.InitialChars.IsValidIndex(C) ? F.InitialChars[C] : 0;
		const double Level0 = F.InitialLevels.IsValidIndex(C) ? F.InitialLevels[C] : 0; // initialTerrain.height?.[c] ?? 0
		return Ch == '+' || (Ch == 'R' && !(F.bSnapWorld && Initial == 'R')) || (Ch == 'F' && Initial != 'F') || (Ch == '.' && Level(C) < Level0);
	}
	// shared/sim.js bridgeSupportedAt on the view: a neighbour that is dry ground, or a deck (the view has no structures)
	bool BridgeSupportedAt(int64 C) const
	{
		const int64 X = C % W;
		const int64 Near[4] = { C - W, C + W, X > 0 ? C - 1 : -1, X < W - 1 ? C + 1 : -1 };
		for (int64 K : Near)
		{
			if (K < 0 || K >= N) continue;
			if (Char(K) != '=')
			{
				// !'WF'.includes(g.ground?.[n] ?? g.chars[n]): the empty string is in every string; another value is not
				const uint8 G = GroundAt(K);
				const bool bWet = G == LayerNullish ? InSet("WF", Char(K)) : G == LayerEmpty ? true : G == LayerOther ? false : InSet("WF", G);
				if (!bWet) return true;
				continue;
			}
			return true;
		}
		return false;
	}

	// shared/sim.js placementCheck for FORTS kind K. bTeam: a team is given (footAt), so Clear mines asks mineKnown, which
	// throws on the view (it has no mine list) at the first mine cell. bSight: sees() is this side's sight, else always.
	FTCFortPlacement Check(int K, double X, double Z, double Dir, bool bTeam, bool bSight)
	{
		FTCFortPlacement P;
		P.At = FVector2D(X, Z);
		P.Dir = Dir;
		auto Fail = [&](const TCHAR* Reason) { P.bOk = false; P.Reason = Reason; P.Cells.Reset(); P.CellChars.Reset(); return P; };
		auto Threw = [&]() { FTCFortPlacement T; T.bThrows = true; T.At = P.At; T.Dir = Dir; return T; };
		if (!FMath::IsFinite(X) || !FMath::IsFinite(Z)) return Fail(TEXT("blocked"));
		const FFort& Fo = Forts[K];
		FPlanCell Plan[8];
		const int Count = FortPlan(K, X, Z, Dir, W, H, Plan);
		if (bSight)
		{
			for (int I = 0; I < Count; I++)
			{
				if (Plan[I].C < 0) continue;
				if (!Sees(CellCenter(Plan[I].C))) return bThrew ? Threw() : Fail(TEXT("notVisible"));
			}
		}
		// fortCells: the plan's cells that can be built, each once
		TArray<int64> Cells;
		TArray<TCHAR> Chars;
		for (int I = 0; I < Count; I++)
		{
			const int64 C = Plan[I].C;
			if (C < 0) continue;
			bool bOk;
			if (Fo.bFill) bOk = Fillable(C);
			else if (Fo.bClear)
			{
				bOk = Char(C) == 'N';
				if (bOk && bTeam) return Threw(); // mineKnown(g, c, team) reads g.mines, which the view lacks
			}
			else
			{
				const char OnSet[2] = { Fo.On, 0 }; // (f.on ?? BUILDABLE_GROUND).includes(g.chars[c])
				bOk = InSet(Fo.On ? OnSet : BuildableGround, Char(C));
			}
			if (bOk && !Cells.Contains(C)) { Cells.Add(C); Chars.Add((TCHAR)(uint8)Plan[I].Ch); }
		}
		if (!std::strcmp(Fo.Kind, "bridge"))
		{
			// supportedBridgePlan: build from a surviving bank or deck outwards, even when the line is centred on a gap
			TArray<int64> Pending = Cells;
			TArray<TCHAR> PendingChars = Chars;
			TSet<int64> Accepted;
			TArray<int64> Result;
			TArray<TCHAR> ResultChars;
			for (int32 Pass = 0; Pending.Num() && Pass < Cells.Num(); Pass++)
			{
				const TArray<int64> Round = Pending;
				const TArray<TCHAR> RoundChars = PendingChars;
				for (int32 I = 0; I < Round.Num(); I++)
				{
					const int64 C = Round[I], Col = C % W;
					const int64 Near[4] = { C - W, C + W, Col > 0 ? C - 1 : -1, Col < W - 1 ? C + 1 : -1 };
					bool bNear = false;
					for (int64 K2 : Near) if (Accepted.Contains(K2)) { bNear = true; break; }
					if (!BridgeSupportedAt(C) && !bNear) continue;
					Accepted.Add(C);
					const int32 At = Pending.IndexOfByKey(C);
					if (At != INDEX_NONE) { Pending.RemoveAt(At); PendingChars.RemoveAt(At); }
					Result.Add(C); ResultChars.Add(RoundChars[I]);
				}
			}
			Cells = MoveTemp(Result);
			Chars = MoveTemp(ResultChars);
		}
		for (int64 C : Cells) P.Cells.Add((int32)C);
		P.CellChars = Chars;
		if (!P.Cells.Num()) { P.bOk = false; P.Reason = TEXT("blocked"); return P; }
		P.bOk = true;
		return P;
	}
};
} // namespace TCField

// ---------- the tables ----------

const TArray<FTCFortDef>& FTCFieldworks::Forts() { return TCField::Tables().Defs; }

const FTCFortDef* FTCFieldworks::Find(const FString& Kind)
{
	const int32 I = TCField::KindIndex(Kind);
	return I >= 0 ? &TCField::Tables().Defs[I] : nullptr;
}

bool FTCFieldworks::LineFort(const FString& Kind) { return TCFieldPure::LineFort(TCField::KindIndex(Kind)); }

TArray<FString> FTCFieldworks::FortBuilders()
{
	TArray<FString> Out;
	for (const char* T : TCFieldPure::FortBuilders) Out.Add(UTF8_TO_TCHAR(T));
	return Out;
}

const TArray<FTCEntrenchDef>& FTCFieldworks::Entrenchments() { return TCField::Tables().Patterns; }

const FTCEntrenchDef* FTCFieldworks::FindEntrenchment(const FString& Kind)
{
	for (const FTCEntrenchDef& D : TCField::Tables().Patterns) if (TCField::Eq(D.Kind, Kind)) return &D;
	return nullptr;
}

const TArray<TPair<FString, FString>>& FTCFieldworks::BuildTabs() { return TCField::Tables().Tabs; }

int32 FTCFieldworks::SegmentCost(const FString& Kind, int32 Cells)
{
	const int K = TCField::KindIndex(Kind);
	return K < 0 ? 0 : FMath::Max(0, TCFieldPure::SegmentCost(K, Cells));
}

TArray<FTCFortSegment> FTCFieldworks::EntrenchPlan(const FString& Pattern, const FVector2D& A, const FVector2D& B, const FVector2D* Back, const FString& Fort)
{
	TArray<FTCFortSegment> Out;
	int Pat = -1;
	for (int I = 0; I < TCFieldPure::PatternCount; I++) if (TCField::Eq(Pattern, UTF8_TO_TCHAR(TCFieldPure::Patterns[I].Kind))) Pat = I;
	const int K = TCField::KindIndex(Fort);
	if (Pat < 0 || K < 0 || TCFieldPure::Forts[K].N <= 0) return Out;
	TCFieldPure::FSegs Segs;
	TCFieldPure::EntrenchPlan(Pat, A.X, A.Y, B.X, B.Y, Back != nullptr, Back ? Back->X : 0, Back ? Back->Y : 0, K, Segs);
	for (int I = 0; I < Segs.N; I++)
	{
		FTCFortSegment S;
		S.Kind = UTF8_TO_TCHAR(TCFieldPure::Forts[Segs.S[I].Kind].Kind);
		S.X = Segs.S[I].X; S.Z = Segs.S[I].Z; S.Dir = Segs.S[I].Dir; S.Front = Segs.S[I].Front;
		Out.Add(MoveTemp(S));
	}
	return Out;
}

FString FTCFieldworks::ActionForChord(const FString& KeyName, bool bShift, bool bCtrl, bool bAlt)
{
	// client/keys.js: fort:trench is T, every other fort key and entrench:line take Shift; none takes Ctrl or Alt
	if (bCtrl || bAlt) return FString();
	if (TCField::Eq(KeyName, TEXT("T"))) return bShift ? FString(TEXT("entrench:line")) : FString(TEXT("fort:trench"));
	if (!bShift) return FString();
	for (const FTCFortDef& D : Forts()) if (TCField::Eq(D.KeyLabel, TEXT("Shift+") + KeyName)) return TEXT("fort:") + D.Kind;
	return FString();
}

FTCDigStatus FTCFieldworks::DigStatus(const FTCUnit& U)
{
	FTCDigStatus S;
	S.bDigging = (U.Flags & TCField::DigFlag) != 0;
	S.bWaitingForMp = !S.bDigging && (U.Flags & TCField::EntrenchFlag) != 0;
	// client/main.js: dig = flags & 16 ? the tail's first value ?? 0 : 0
	S.Progress = S.bDigging && U.Tail.Num() ? U.Tail[0] : 0.f;
	S.Tag = S.bDigging ? TEXT("Digging") : S.bWaitingForMp ? TEXT("Waiting for MP to dig") : TEXT("");
	return S;
}

void FTCFieldAim::Cancel()
{
	Kind = ETCFieldAim::None;
	bHasCenter = false;
	Center = FVector2D::ZeroVector;
	Hint.Reset();
	MeshKind = ETCFieldAim::None;
	MeshFort.Reset();
	bMeshPlaced = false;
	MeshAt = FVector2D::ZeroVector;
	MeshRotation = 0;
	MeshColor = FColor(0xf4, 0xdc, 0x90);
	MeshPieces.Reset();
}

void FTCFieldworks::SetUnitTableReference(const FString& Reference)
{
	UnitTableReference = Reference;
	// FORTS here and the generated sight table must both come from the unit table's reference
	bReferenceMatches = Reference == ReferencePin && FCString::Strcmp(TCConstructionData::ReferencePin, ReferencePin) == 0;
}

// ---------- the messages ----------

void FTCFieldworks::Clear()
{
	bStarted = bHasSnapshot = false;
	W = H = 0; bMapWorld = false;
	Teams.Reset(); Ground.Reset(); Objects.Reset(); InitialChars.Reset(); InitialLevels.Reset();
	SnapMp = 0; ModeKind.Reset(); bTech = false; TechTier = 0; bSnapWorld = false;
	OutSeats.Reset(); Smokes.Reset(); Rows.Reset(); RowOrder.Reset(); WorkRows.Reset();
}

void FTCFieldworks::ApplyStart(const FJsonObject& M)
{
	// client/main.js start: lastSnap = null and the held rows reset
	Clear();
	const TSharedPtr<FJsonObject>* MapObj;
	double DW = 0, DH = 0;
	if (!M.TryGetObjectField(TEXT("map"), MapObj) || !MapObj->IsValid() || !(*MapObj)->TryGetNumberField(TEXT("w"), DW) || !(*MapObj)->TryGetNumberField(TEXT("h"), DH)) return;
	if (DW < 1 || DH < 1 || DW > 16384 || DH > 16384) return;
	const FJsonObject& Map = **MapObj;
	bStarted = true;
	W = (int32)DW; H = (int32)DH;
	bMapWorld = TCField::Truthy(Map.TryGetField(TEXT("world")));

	// teams = m.teams ?? names.map((_, i) => i)
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	const TSharedPtr<FJsonValue> TeamsField = M.TryGetField(TEXT("teams"));
	if (TeamsField.IsValid() && TeamsField->Type != EJson::Null && TeamsField->TryGetArray(Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			FTeam T;
			if (!V.IsValid() || V->Type == EJson::Null) T.bNull = true; else T.V = TCField::NumOr(V, TCField::NaN);
			Teams.Add(T);
		}
	}
	else if (M.TryGetArrayField(TEXT("names"), Arr)) for (int32 I = 0; I < Arr->Num(); I++) Teams.Add(FTeam{ false, (double)I });

	const int32 Count = W * H;
	auto Flat = [](const FJsonObject& O, const TCHAR* Field, FString& Out)
	{
		const TArray<TSharedPtr<FJsonValue>>* Lines;
		if (!O.TryGetArrayField(Field, Lines)) return false;
		for (const TSharedPtr<FJsonValue>& L : *Lines) if (L.IsValid() && L->Type == EJson::String) Out += L->AsString();
		return true;
	};
	FString RowsText;
	Flat(Map, TEXT("rows"), RowsText);

	// client/main.js terrain.groundGrid and objectGrid from shared/world-layers.js worldLayers(map): authored layers
	// (worldVersion 2), else split from the rows; a mine in the objects layer is not an object
	Ground.Init(TCField::LayerNullish, Count); Objects.Init(TCField::LayerNullish, Count);
	double Version = 0;
	const TSharedPtr<FJsonObject>* Layers;
	FString GroundText, ObjectText;
	if (Map.TryGetNumberField(TEXT("worldVersion"), Version) && Version == 2 && Map.TryGetObjectField(TEXT("layers"), Layers) && Layers->IsValid())
	{
		Flat(**Layers, TEXT("ground"), GroundText);
		Flat(**Layers, TEXT("objects"), ObjectText);
		ObjectText.ReplaceCharInline(TEXT('N'), TEXT('.'));
	}
	else
	{
		for (const TCHAR Ch : RowsText)
		{
			const bool bBase = Ch && FCString::Strchr(TEXT(".D+MWF"), Ch) != nullptr;
			GroundText.AppendChar(bBase ? Ch : Ch == TEXT('=') ? TEXT('W') : TEXT('.'));
			ObjectText.AppendChar(bBase || Ch == TEXT('N') ? TEXT('.') : Ch);
		}
	}
	for (int32 I = 0; I < Count; I++)
	{
		if (I < GroundText.Len()) Ground[I] = TCField::CharCode(GroundText[I]);
		if (I < ObjectText.Len()) Objects[I] = TCField::CharCode(ObjectText[I]);
	}

	// client/availability.js rememberPlacementTerrain(map, m.cells): an authored map's rows and heights as they start; a
	// World map's cells from what arrives ('?' and level 0 until then)
	InitialChars.Init(0, Count);
	InitialLevels.Init(0, Count);
	if (bMapWorld) InitialChars.Init('?', Count);
	else
	{
		for (int32 I = 0; I < Count && I < RowsText.Len(); I++) InitialChars[I] = TCField::CharCode(RowsText[I]);
		const TArray<TSharedPtr<FJsonValue>>* Heights = nullptr;
		const bool bHeights = Map.TryGetArrayField(TEXT("heights"), Heights);
		for (int32 Y = 0; Y < H; Y++)
		{
			FString Row;
			const bool bRow = bHeights && Y < Heights->Num() && (*Heights)[Y].IsValid() && (*Heights)[Y]->TryGetString(Row);
			for (int32 X = 0; X < W; X++) InitialLevels[Y * W + X] = TCField::JsLevelOf(bRow && X < Row.Len() ? Row[X] : TEXT('0')); // ?? '0'
		}
	}
	if (M.TryGetArrayField(TEXT("cells"), Arr)) ApplyCells(*Arr);
}

void FTCFieldworks::ApplyCells(const TArray<TSharedPtr<FJsonValue>>& Cells)
{
	// [cell, char, level, state, data]: client/main.js keeps data.ground and data.object in its layers (when data is
	// there); rememberPlacementTerrain fills a World cell's starting character and level the first time it arrives
	for (const TSharedPtr<FJsonValue>& Entry : Cells)
	{
		const TArray<TSharedPtr<FJsonValue>>* Row;
		double CD;
		if (!Entry.IsValid() || !Entry->TryGetArray(Row) || Row->Num() < 1 || !TCField::Num((*Row)[0], CD) || CD != std::floor(CD)) continue;
		const int64 Cell = (int64)CD;
		if (Cell < 0 || Cell >= (int64)W * H) continue;
		const TArray<TSharedPtr<FJsonValue>>& R = *Row;
		auto At = [&R](int32 I) { return I < R.Num() ? R[I] : TSharedPtr<FJsonValue>(); };
		const TSharedPtr<FJsonObject>* Data = nullptr;
		const bool bData = At(4).IsValid() && At(4)->TryGetObject(Data) && Data && Data->IsValid();
		if (bMapWorld && InitialChars[Cell] == '?')
		{
			// data?.initial?.[0] ?? (ch === 'N' ? composeWorldCell(data?.ground ?? '.', data?.object ?? '.') : ch)
			const TArray<TSharedPtr<FJsonValue>>* Initial = nullptr;
			const bool bInitial = bData && (*Data)->TryGetArrayField(TEXT("initial"), Initial);
			const TSharedPtr<FJsonValue> I0 = bInitial && Initial->Num() > 0 ? (*Initial)[0] : TSharedPtr<FJsonValue>();
			const TSharedPtr<FJsonValue> I1 = bInitial && Initial->Num() > 1 ? (*Initial)[1] : TSharedPtr<FJsonValue>();
			uint8 Ch;
			if (!TCField::Nullish(I0)) Ch = TCField::LayerCode(I0);
			else
			{
				const uint8 CharCode = TCField::LayerCode(At(1));
				if (CharCode == 'N')
				{
					const TSharedPtr<FJsonValue> G = bData ? (*Data)->TryGetField(TEXT("ground")) : TSharedPtr<FJsonValue>(), O = bData ? (*Data)->TryGetField(TEXT("object")) : TSharedPtr<FJsonValue>();
					const uint8 GC = TCField::Nullish(G) ? '.' : TCField::LayerCode(G), OC = TCField::Nullish(O) ? '.' : TCField::LayerCode(O);
					Ch = OC != TCField::LayerEmpty && OC != '.' ? OC : GC;
				}
				else Ch = CharCode;
			}
			InitialChars[Cell] = Ch;
			// data?.initial?.[1] ?? lv ?? 0 (a value that is not a number counts as NaN here)
			const TSharedPtr<FJsonValue> Lv = !TCField::Nullish(I1) ? I1 : At(2);
			InitialLevels[Cell] = TCField::Nullish(Lv) ? 0 : TCField::NumOr(Lv, TCField::NaN);
		}
		if (!bData) continue;
		Ground[Cell] = TCField::LayerCode((*Data)->TryGetField(TEXT("ground")));
		Objects[Cell] = TCField::LayerCode((*Data)->TryGetField(TEXT("object")));
	}
}

void FTCFieldworks::ApplySnapshot(const FJsonObject& M)
{
	if (!bStarted) return;
	bHasSnapshot = true;
	SnapMp = TCField::NumOr(M.TryGetField(TEXT("mp")), TCField::NaN);
	ModeKind.Reset();
	const TSharedPtr<FJsonObject>* Obj;
	if (M.TryGetObjectField(TEXT("mode"), Obj) && Obj->IsValid()) (*Obj)->TryGetStringField(TEXT("kind"), ModeKind);
	bTech = M.TryGetObjectField(TEXT("tech"), Obj) && Obj->IsValid();
	TechTier = bTech ? TCField::NumOr((*Obj)->TryGetField(TEXT("tier")), TCField::NaN) : 0;
	bSnapWorld = M.TryGetObjectField(TEXT("world"), Obj) && Obj->IsValid();
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	OutSeats.Reset();
	if (M.TryGetArrayField(TEXT("out"), Arr)) for (const TSharedPtr<FJsonValue>& V : *Arr) OutSeats.Add(TCField::Truthy(V));
	Smokes.Reset();
	if (M.TryGetArrayField(TEXT("smokes"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			if (V.IsValid() && V->TryGetArray(Row) && Row->Num() >= 3) Smokes.Add(FSmoke{ TCField::NumOr((*Row)[0], TCField::NaN), TCField::NumOr((*Row)[1], TCField::NaN), TCField::NumOr((*Row)[2], TCField::NaN) });
		}
	}
	if (M.TryGetArrayField(TEXT("cells"), Arr)) ApplyCells(*Arr);

	// client/world-state.js apply: a full resend (or no held) replaces the rows, gone ids leave, rows upsert in place
	if (TCField::Truthy(M.TryGetField(TEXT("all"))) || !TCField::Truthy(M.TryGetField(TEXT("held")))) { Rows.Reset(); RowOrder.Reset(); }
	if (M.TryGetArrayField(TEXT("gone"), Arr))
	{
		TSet<int32> Gone;
		double D;
		for (const TSharedPtr<FJsonValue>& V : *Arr) if (TCField::Num(V, D) && Rows.Remove((int32)D)) Gone.Add((int32)D);
		if (Gone.Num()) RowOrder.RemoveAll([&Gone](int32 Id) { return Gone.Contains(Id); });
	}
	if (M.TryGetArrayField(TEXT("units"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			double D;
			if (!V.IsValid() || !V->TryGetArray(Row) || Row->Num() < 1 || !TCField::Num((*Row)[0], D)) continue;
			const int32 Id = (int32)D;
			const TArray<TSharedPtr<FJsonValue>>& R = *Row;
			auto At = [&R](int32 I) { return I < R.Num() ? R[I] : TSharedPtr<FJsonValue>(); };
			FRow U;
			if (At(1).IsValid() && At(1)->Type == EJson::String) U.Type = At(1)->AsString();
			U.Owner = TCField::NumOr(At(2), TCField::NaN); U.X = TCField::NumOr(At(3), TCField::NaN); U.Z = TCField::NumOr(At(4), TCField::NaN); U.Hp = TCField::NumOr(At(7), TCField::NaN);
			U.Flags = (int32)(int64)TCField::NumOr(At(12), 0);
			const TSharedPtr<FJsonValue> Built = At(14);
			U.Built = !TCField::Nullish(Built) ? TCField::NumOr(Built, TCField::NaN) : 1; // built ?? 1
			if (!Rows.Contains(Id)) RowOrder.Add(Id);
			Rows.Add(Id, MoveTemp(U));
		}
	}

	// works: [project id or -1, 1 for wire, x, z, dir, kind]; a snapshot without works has none (drawWorks(s.works ?? []))
	WorkRows.Reset();
	if (M.TryGetArrayField(TEXT("works"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			if (!V.IsValid() || !V->TryGetArray(Row)) continue;
			const TArray<TSharedPtr<FJsonValue>>& R = *Row;
			auto At = [&R](int32 I) { return I < R.Num() ? R[I] : TSharedPtr<FJsonValue>(); };
			FTCWork Wk;
			const double Id = TCField::NumOr(At(0), TCField::NaN);
			Wk.Project = Id >= 0 && Id == std::floor(Id) && Id <= MAX_int32 ? (int32)Id : -1;
			Wk.bWire = TCField::Truthy(At(1));
			Wk.X = TCField::NumOr(At(2), TCField::NaN);
			Wk.Z = TCField::NumOr(At(3), TCField::NaN);
			// placementCheck's dir = 0 default: undefined is 0, null is Math.cos(null) = 1's 0; a non-number is NaN cells
			const TSharedPtr<FJsonValue> Dir = At(4);
			Wk.bDirNumber = TCField::Nullish(Dir) || Dir->Type == EJson::Number;
			Wk.Dir = TCField::NumOr(Dir, 0);
			const TSharedPtr<FJsonValue> Kind = At(5);
			if (!TCField::Nullish(Kind) && Kind->Type == EJson::String) Wk.Kind = Kind->AsString();
			else if (TCField::Nullish(Kind)) Wk.Kind = Wk.bWire ? TEXT("wire") : TEXT("trench"); // kind ?? (wire ? 'wire' : 'trench')
			WorkRows.Add(MoveTemp(Wk));
		}
	}
}

// ---------- shared rules over the snapshot ----------

FTCFieldworks::FTeam FTCFieldworks::TeamOfSeat(double Seat, bool& bOutExists) const
{
	bOutExists = Seat >= 0 && Seat < Teams.Num() && Seat == std::floor(Seat);
	return bOutExists ? Teams[(int32)Seat] : FTeam();
}

bool FTCFieldworks::TeamsEqual(const FTeam& A, const FTeam& B)
{
	if (A.bNull || B.bNull) return A.bNull && B.bNull;
	return A.V == B.V;
}

FTCFieldworks::FTeam FTCFieldworks::MyTeam(const FTCMatchState& State) const
{
	bool bSeat = false;
	const FTeam T = TeamOfSeat(State.You, bSeat);
	return bSeat && !T.bNull ? T : FTeam{ false, (double)State.You };
}

bool FTCFieldworks::Observing(const FTCMatchState& State) const
{
	// client/main.js observing: watching, or out; this client also arms nothing on an unconfirmed seat (its SetAim)
	return State.bSpectator || (OutSeats.IsValidIndex(State.You) && OutSeats[State.You]) || !State.SeatConfirmed();
}

FTCFieldCheck FTCFieldworks::Gate(const FTCMatchState& State) const
{
	if (!bReferenceMatches)
	{
		return TCField::No(FString::Printf(TEXT("Fortification data is for reference %s, the unit table is for %s"), ReferencePin,
			UnitTableReference.IsEmpty() ? TEXT("an unknown reference") : *UnitTableReference), true);
	}
	if (!bHasSnapshot) return TCField::No(TEXT("Waiting for the army"));
	if (State.bSpectator || (OutSeats.IsValidIndex(State.You) && OutSeats[State.You])) return TCField::No(TEXT("You are spectating"));
	if (!State.SeatConfirmed()) return TCField::No(TEXT("Waiting for the lobby to confirm this seat"), true);
	return TCField::Yes();
}

bool FTCFieldworks::ViewReady(const FTCMatchState& State, FString& OutWhy) const
{
	OutWhy.Reset();
	if (!bReferenceMatches) { OutWhy = Gate(State).Reason; return false; }
	// client/main.js placementView: none before the first snapshot of this match
	if (!bHasSnapshot || !State.bHasMatch) return false;
	if (State.Map.W != W || State.Map.H != H || State.Map.Chars.Num() != W * H || State.Map.Levels.Num() != W * H)
	{
		OutWhy = TEXT("The map held does not match the start the placement view was built from");
		return false;
	}
	return true;
}

FTCFortPlacement FTCFieldworks::Place(const FTCMatchState& State, const FString& Kind, const FVector2D& At, double Dir, bool bTeam, bool bSight) const
{
	FTCFortPlacement P;
	P.At = At; P.Dir = Dir;
	P.Reason = TEXT("notVisible"); // footAt without a placement view
	FString Why;
	if (!ViewReady(State, Why))
	{
		if (!Why.IsEmpty()) { P.bNative = true; P.NativeReason = Why; }
		return P;
	}
	const int K = TCField::KindIndex(Kind);
	if (K < 0)
	{
		P.bNative = true;
		P.NativeReason = FString::Printf(TEXT("'%s' is not a fortification (FTCConstruction places buildings)"), *Kind);
		return P;
	}
	if (bSight)
	{
		FTCRustPlacement Rust;
		// Direct footAt passes team; entrenchment/work callers omit it, as in the reference.
		const TOptional<int32> Team = bTeam ? TOptional<int32>(State.TeamOf(State.You)) : TOptional<int32>();
		if (FTCRustQueries::Placement(State, Kind, At, Dir, Team, Rust).IsOk())
		{
			P.bOk = Rust.bOk; P.Reason = FTCRustQueries::ReasonCode(Rust.Reason); P.At = Rust.At;
			for (const FTCRustPlacementCell& Cell : Rust.Cells)
			{
				P.Cells.Add(Cell.Cell);
				P.CellChars.Add(Cell.Character.IsSet() ? Cell.Character.GetValue()[0] : 0);
			}
			return P;
		}
	}
	// Sight-free cost/planning calls, World and unavailable/unsupported recipient queries retain the existing mirror.
	TCField::FView View(*this, State);
	return View.Check(K, At.X, At.Y, Dir, bTeam, bSight);
}

FTCFortPlacement FTCFieldworks::FortPlacement(const FTCMatchState& State, const FString& Kind, const FVector2D& At, double Dir) const
{
	return Place(State, Kind, At, Dir, true, true);
}

bool FTCFieldworks::PositionFor(int32 Id, const FTCPositionOf& PositionOf, FVector2D& Out) const
{
	if (PositionOf) return PositionOf(Id, Out);
	const FRow* R = Rows.Find(Id);
	if (!R) return false;
	Out = FVector2D(R->X, R->Z);
	return true;
}

TArray<int32> FTCFieldworks::Diggers(const TSet<int32>& Selected, const FTCPositionOf& PositionOf) const
{
	// client/main.js diggers: the selected units (any owner) of a fort builder type, not retreating
	TArray<int32> Out;
	for (int32 Id : Selected)
	{
		const FRow* R = Rows.Find(Id);
		FVector2D P;
		if (R && TCField::IsFortBuilder(R->Type) && !(R->Flags & TCField::RetreatingFlag) && PositionFor(Id, PositionOf, P)) Out.Add(Id);
	}
	return Out;
}

bool FTCFieldworks::NearestDigger(const FVector2D& At, const TSet<int32>& Selected, const FTCPositionOf& PositionOf, FVector2D& OutPos, int32& OutId) const
{
	// client/main.js nearestDigger: diggers sorted by distance (a stable sort), the first
	bool bAny = false;
	double Best = 0;
	for (int32 Id : Diggers(Selected, PositionOf))
	{
		FVector2D P;
		PositionFor(Id, PositionOf, P);
		const double D = TCFieldPure::JsHypot(P.X - At.X, P.Y - At.Y);
		if (!bAny || D < Best) { bAny = true; Best = D; OutPos = P; OutId = Id; }
	}
	return bAny;
}

double FTCFieldworks::DefaultDigDir(const FString& Kind, const FVector2D& At, const TSet<int32>& Selected, const FTCPositionOf& PositionOf) const
{
	// client/main.js defaultDir('dig', at): across the nearest digger's approach (a bridge: along it)
	FVector2D V;
	int32 Id = 0;
	if (!NearestDigger(At, Selected, PositionOf, V, Id)) return 0;
	const FTCFortDef* D = Find(Kind);
	return std::atan2(At.Y - V.Y, At.X - V.X) + (D && D->bAlong ? 0 : TCFieldPure::Pi / 2);
}

FString FTCFieldworks::EntrenchName(const FString& Pattern, const FString& Fort) const
{
	// client/main.js entrenchName
	if (TCField::Eq(Fort, TEXT("trench")))
	{
		const FTCEntrenchDef* E = FindEntrenchment(Pattern);
		return E ? E->Name : FString(TEXT("undefined"));
	}
	const FTCFortDef* D = Find(Fort);
	return (D ? D->Name : Fort) + TEXT(" line");
}

// ---------- availability and menus ----------

FTCFieldCheck FTCFieldworks::CheckDig(const FTCMatchState& State, const FString& Kind, bool bQueue, const TSet<int32>& Selected) const
{
	const FTCFieldCheck G = Gate(State);
	if (!G.bOk) return G;
	// concrete (wall, gate) takes an HQ tier, and Engineers where the mode has them
	const FTCFortDef* F = Find(Kind);
	if (F && F->Tier && bTech && F->Tier > TechTier) return TCField::No(TEXT("Needs ") + FTCRecruitment::TierName(F->Tier));
	const bool bEngineersOnly = F && !F->Unit.IsEmpty() && SnapshotClassic(); // builderTypes(classic)
	int32 Crew = 0, Retreating = 0;
	for (int32 Id : RowOrder)
	{
		const FRow& U = Rows[Id];
		if (U.Owner != State.You || !Selected.Contains(Id)) continue;
		if (bEngineersOnly ? !TCField::Eq(U.Type, TEXT("engineer")) : !TCField::IsFortBuilder(U.Type)) continue;
		Crew++;
		if (U.Flags & TCField::RetreatingFlag) Retreating++;
	}
	if (!Crew) return TCField::No(TCField::Deny(TEXT("noBuilders")));
	if (Crew == Retreating) return TCField::No(TCField::Deny(TEXT("retreating")));
	if (bQueue) return TCField::Yes(); // a queued dig is paid when it starts
	if (!F) return TCField::No(FString::Printf(TEXT("No fortification '%s' (the browser throws here)"), *Kind), true);
	if (SnapMp < F->Cost) return TCField::No(FString::Printf(TEXT("Needs %d MP"), F->Cost));
	return TCField::Yes();
}

FTCFieldCheck FTCFieldworks::CheckEntrench(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	const FTCFieldCheck G = Gate(State);
	if (!G.bOk) return G;
	// nothing is paid up front: each digger pays for its segment as it starts
	int32 Crew = 0, Retreating = 0;
	for (int32 Id : RowOrder)
	{
		const FRow& U = Rows[Id];
		if (U.Owner != State.You || !Selected.Contains(Id) || !TCField::IsFortBuilder(U.Type)) continue;
		Crew++;
		if (U.Flags & TCField::RetreatingFlag) Retreating++;
	}
	if (!Crew) return TCField::No(TCField::Deny(TEXT("noBuilders")));
	return Crew == Retreating ? TCField::No(TCField::Deny(TEXT("retreating"))) : TCField::Yes();
}

bool FTCFieldworks::OffersFieldMenus(const TSet<int32>& Selected) const
{
	// client/hud.js drawOrders: not all buildings, not all trucks, and a builder squad among the selection
	if (!bHasSnapshot) return false;
	TArray<const FRow*> Sel;
	for (int32 Id : Selected) if (const FRow* U = Rows.Find(Id)) Sel.Add(U);
	if (!Sel.Num()) return false;
	if (!Sel.ContainsByPredicate([](const FRow* U) { const TCConstructionData::FSightRow* D = TCField::SightOf(U->Type); return !D || !D->bBuilding; })) return false;
	if (!Sel.ContainsByPredicate([](const FRow* U) { return !TCField::Eq(U->Type, TEXT("truck")); })) return false;
	return Sel.ContainsByPredicate([](const FRow* U) { return TCField::IsFortBuilder(U->Type); });
}

FTCFieldMenu FTCFieldworks::FortMenu(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	FTCFieldMenu Menu;
	Menu.bOffered = OffersFieldMenus(Selected);
	if (!Menu.bOffered) return Menu;
	for (const FTCFortDef& D : Forts())
	{
		FTCFieldButton& B = Menu.Buttons.AddDefaulted_GetRef();
		B.Action = TEXT("fort:") + D.Kind; B.Kind = D.Kind; B.Name = D.Name; B.Icon = D.Kind; B.Badge = D.Badge; B.Tab = D.Tab;
		B.Tip = FString::Printf(TEXT("%s%s: %s. %s"), *D.Name, D.KeyLabel.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *D.KeyLabel), *D.Tip,
			D.bLine && !TCField::Eq(D.Kind, TEXT("trench"))
				? TEXT("Click where it starts, then where it ends: one piece, or a continuous line that every selected builder squad works on. Price per piece")
				: TEXT("Click where; the nearest builder squad puts it across its approach"));
		B.Value = FString::Printf(TEXT("%d MP"), D.Cost);
		B.Check = CheckDig(State, D.Kind, false, Selected);
	}
	return Menu;
}

FTCFieldMenu FTCFieldworks::TrenchMenu(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	FTCFieldMenu Menu;
	Menu.bOffered = OffersFieldMenus(Selected);
	if (!Menu.bOffered) return Menu;
	const FTCFieldCheck Check = CheckEntrench(State, Selected);
	const FTCFortDef* Trench = Find(TEXT("trench"));
	for (const FTCEntrenchDef& E : Entrenchments())
	{
		const bool bLine = TCField::Eq(E.Kind, TEXT("line"));
		FTCFieldButton& B = Menu.Buttons.AddDefaulted_GetRef();
		B.Action = TEXT("ent:") + E.Kind; B.Kind = E.Kind; B.Name = E.Name; B.Icon = TEXT("e_") + E.Kind;
		B.Badge = bLine ? FString(TEXT("⇧T")) : FString(); // badge('entrench:line')
		B.Tip = FString::Printf(TEXT("%s%s: %s. Every selected builder squad digs; each segment is paid as it is started. Shift on the second click queues it. Right-click a planned pattern with other squads to send them to help"),
			*E.Name, bLine ? TEXT(" (Shift+T)") : TEXT(""), *E.Tip);
		B.Value = FString::Printf(TEXT("%d/seg"), Trench ? Trench->Cost : 0);
		B.Check = Check;
	}
	return Menu;
}

// ---------- aims ----------

FTCFieldArm FTCFieldworks::StartDig(const FTCMatchState& State, const FString& Kind, const TSet<int32>& Selected, FTCFieldAim& Aim) const
{
	// client/main.js startDig: sandbags, wire, traps, mines and the other line forts are drawn out like a trench line
	if (LineFort(Kind) && !TCField::Eq(Kind, TEXT("trench"))) return StartEntrench(State, TEXT("line"), Kind, Selected, Aim);
	FTCFieldArm A;
	A.Check = CheckDig(State, Kind, false, Selected);
	if (!A.Check.bOk) { A.Feedback = A.Check.Reason; return A; }
	if (Observing(State)) return A; // setAim arms nothing
	const FTCFortDef* D = Find(Kind);
	Aim.Kind = ETCFieldAim::Dig;
	Aim.FortKind = Kind;
	Aim.bHasCenter = false;
	Aim.Hint = FString::Printf(TEXT("Click where to build the %s. Right-click cancels"), *(D ? D->Name : Kind).ToLower());
	A.bArm = true;
	return A;
}

FTCFieldArm FTCFieldworks::StartEntrench(const FTCMatchState& State, const FString& Pattern, const FString& Fort, const TSet<int32>& Selected, FTCFieldAim& Aim) const
{
	FTCFieldArm A;
	A.Check = CheckEntrench(State, Selected);
	if (!A.Check.bOk) { A.Feedback = A.Check.Reason; return A; }
	if (!FindEntrenchment(Pattern) || !LineFort(Fort))
	{
		// the browser's buttons and keys never ask for these; the server refuses them ('blocked')
		A.Check = TCField::No(FString::Printf(TEXT("No entrenchment '%s' of '%s'"), *Pattern, *Fort), true);
		A.Feedback = A.Check.Reason;
		return A;
	}
	if (Observing(State)) return A;
	Aim.Kind = ETCFieldAim::Entrench;
	Aim.EntrenchKind = Pattern;
	Aim.EntrenchFort = Fort;
	Aim.bHasCenter = false;
	Aim.Hint = EntrenchName(Pattern, Fort) + TEXT(": click where it starts. Right-click cancels");
	A.bArm = true;
	return A;
}

TArray<FTCEntrenchPiece> FTCFieldworks::EntrenchPieces(const FTCMatchState& State, const FString& Pattern, const FString& Fort, const FVector2D& A, const FVector2D& B, const TSet<int32>& Selected, const FTCPositionOf& PositionOf, bool& bOutThrows) const
{
	bOutThrows = false;
	TArray<FTCEntrenchPiece> Out;
	const TArray<int32> Crew = Diggers(Selected, PositionOf);
	FString Why;
	if (!Crew.Num() || !ViewReady(State, Why)) return Out;
	// back: where the diggers are (the sums run in the selection's order)
	double SX = 0, SZ = 0;
	for (int32 Id : Crew) { FVector2D P; PositionFor(Id, PositionOf, P); SX = SX + P.X; }
	for (int32 Id : Crew) { FVector2D P; PositionFor(Id, PositionOf, P); SZ = SZ + P.Y; }
	const FVector2D Back(SX / Crew.Num(), SZ / Crew.Num());
	TCField::FView View(*this, State);
	for (const FTCFortSegment& S : EntrenchPlan(Pattern, A, B, &Back, Fort))
	{
		const int K = TCField::KindIndex(S.Kind);
		const FTCFortPlacement Place_ = View.Check(K, S.X, S.Z, S.Dir, false, true);
		if (Place_.bThrows) { bOutThrows = true; Out.Reset(); return Out; }
		if (!Place_.bOk) continue;
		FTCEntrenchPiece& Piece = Out.AddDefaulted_GetRef();
		Piece.Segment = S;
		Piece.Place = Place_;
		Piece.Cost = SegmentCost(S.Kind, Place_.Cells.Num());
		Piece.Color = TCField::TileColor(S.Kind);
	}
	return Out;
}

FString FTCFieldworks::EntrenchSummary(const FString& Pattern, const FString& Fort, const TArray<FTCEntrenchPiece>& Pieces, int32 NumDiggers) const
{
	// client/main.js entrenchSummary: the price, and how much starts now with the squads and manpower at hand
	const FString Name = EntrenchName(Pattern, Fort);
	if (!Pieces.Num()) return Name + TEXT(": nothing can be built there");
	int64 Total = 0;
	for (const FTCEntrenchPiece& P : Pieces) Total += P.Cost;
	double Mp = FMath::IsNaN(SnapMp) ? 0 : SnapMp; // lastSnap?.mp ?? 0
	int32 Now = 0;
	for (int32 I = 0; I < Pieces.Num() && I < NumDiggers; I++) { if (Mp < Pieces[I].Cost) break; Mp -= Pieces[I].Cost; Now++; }
	const int32 Segs = Pieces.Num();
	FString Text = FString::Printf(TEXT("%s: %d segment%s, %lld MP"), *Name, Segs, Segs > 1 ? TEXT("s") : TEXT(""), (long long)Total);
	if (Now < Segs) Text += FString::Printf(TEXT("; %d of %d start now, the rest as squads and manpower free up"), Now, Segs);
	return Text;
}

FTCFieldPreview FTCFieldworks::Preview(const FTCMatchState& State, FTCFieldAim& Aim, bool bHasGround, const FVector2D& Ground_, const TSet<int32>& Selected, const FTCPositionOf& PositionOf) const
{
	FTCFieldPreview P;
	if (!Aim.IsArmed())
	{
		// the frame without an aim drops the mesh
		Aim.MeshKind = ETCFieldAim::None; Aim.MeshPieces.Reset(); Aim.bMeshPlaced = false;
		return P;
	}
	P.bShown = true;
	if (Aim.MeshKind != Aim.Kind)
	{
		// aimShape(targeting, 0xf4dc90): a new mesh, made for the fort armed now
		Aim.MeshKind = Aim.Kind; Aim.MeshFort = Aim.FortKind; Aim.MeshRotation = 0; Aim.MeshColor = FColor(0xf4, 0xdc, 0x90);
		Aim.MeshPieces.Reset(); Aim.bMeshPlaced = false; Aim.MeshAt = FVector2D::ZeroVector;
	}
	if (Aim.Kind == ETCFieldAim::Entrench)
	{
		if (Aim.bHasCenter || bHasGround)
		{
			const FVector2D A = Aim.bHasCenter ? Aim.Center : Ground_, B = bHasGround ? Ground_ : Aim.Center;
			bool bThrows = false;
			TArray<FTCEntrenchPiece> Pieces = EntrenchPieces(State, Aim.EntrenchKind, Aim.EntrenchFort, A, B, Selected, PositionOf, bThrows);
			if (bThrows) P.bThrows = true;
			else
			{
				Aim.MeshPieces = MoveTemp(Pieces);
				Aim.Hint = FString::Printf(TEXT("%s. Click %s. Right-click cancels"), *EntrenchSummary(Aim.EntrenchKind, Aim.EntrenchFort, Aim.MeshPieces, Diggers(Selected, PositionOf).Num()),
					Aim.bHasCenter ? TEXT("to dig") : TEXT("where it starts"));
			}
		}
		P.Pieces = Aim.MeshPieces;
		P.Hint = Aim.Hint;
		return P;
	}
	// dig: the marker sits on the centre once pinned (turning toward the pointer past 1.5 m), else under the pointer
	// across the nearest digger's approach
	if (Aim.bHasCenter)
	{
		Aim.MeshAt = Aim.Center; Aim.bMeshPlaced = true;
		if (bHasGround && TCFieldPure::JsHypot(Ground_.X - Aim.Center.X, Ground_.Y - Aim.Center.Y) > 1.5) Aim.MeshRotation = -std::atan2(Ground_.Y - Aim.Center.Y, Ground_.X - Aim.Center.X);
	}
	else if (bHasGround)
	{
		Aim.MeshAt = Ground_; Aim.bMeshPlaced = true;
		Aim.MeshRotation = -DefaultDigDir(Aim.FortKind, Ground_, Selected, PositionOf);
	}
	if (Aim.bHasCenter || bHasGround)
	{
		const FVector2D At = Aim.bHasCenter ? Aim.Center : Ground_;
		P.Placement = FortPlacement(State, Aim.FortKind, At, -Aim.MeshRotation);
		if (P.Placement.bThrows) P.bThrows = true;
		else Aim.MeshColor = P.Placement.PreviewColor();
	}
	// aimShape('dig'): the Gun Pit's 3 by 2 cells, else n cells by one, of the fort the mesh was made for
	const FTCFortDef* MeshDef = Find(Aim.MeshFort);
	const double C = FTCMatchState::CellMetres;
	P.bMarker = true;
	P.bMarkerPlaced = Aim.bMeshPlaced;
	P.MarkerAt = Aim.MeshAt;
	P.MarkerDir = -Aim.MeshRotation;
	P.MarkerLen = MeshDef ? (MeshDef->bNest ? 3 * C : MeshDef->N * C) : 0;
	P.MarkerWidth = MeshDef ? (MeshDef->bNest ? 2 * C : C) : 0;
	P.MarkerColor = Aim.MeshColor;
	P.Hint = Aim.Hint;
	return P;
}

FTCFieldClick FTCFieldworks::Click(const FTCMatchState& State, FTCFieldAim& Aim, bool bHasGround, const FVector2D& Ground_, bool bShift, const TSet<int32>& Selected, const FTCPositionOf& PositionOf) const
{
	FTCFieldClick R;
	if (!Aim.IsArmed()) return R;
	if (!bHasGround) { R.Feedback = TCField::Deny(TEXT("blocked")); return R; }
	// first click: pin the centre, then the mouse rotates it
	if (!Aim.bHasCenter)
	{
		Aim.bHasCenter = true;
		Aim.Center = Ground_;
		if (Aim.Kind != ETCFieldAim::Entrench) Aim.Hint = TEXT("Move the mouse to rotate, then click to place");
		R.bPinnedCenter = true;
		return R;
	}
	if (Aim.Kind == ETCFieldAim::Entrench)
	{
		const FTCFieldCheck Check = CheckEntrench(State, Selected);
		if (!Check.bOk) { R.Feedback = Check.Reason; return R; }
		const FVector2D A = Aim.Center;
		bool bThrows = false;
		const TArray<FTCEntrenchPiece> Pieces = EntrenchPieces(State, Aim.EntrenchKind, Aim.EntrenchFort, A, Ground_, Selected, PositionOf, bThrows);
		if (bThrows) { R.bThrows = true; return R; }
		if (!Pieces.Num()) { R.Feedback = TCField::Deny(TEXT("blocked")); return R; }
		const TArray<int32> Ids = Diggers(Selected, PositionOf);
		const FString Pattern = Aim.EntrenchKind, Fort = Aim.EntrenchFort;
		Aim.Cancel();
		TSharedRef<FJsonObject> Msg = MakeShared<FJsonObject>();
		Msg->SetStringField(TEXT("t"), TEXT("entrench"));
		Msg->SetArrayField(TEXT("ids"), TCField::IdArray(Ids));
		Msg->SetStringField(TEXT("pattern"), Pattern);
		Msg->SetStringField(TEXT("fort"), Fort);
		Msg->SetNumberField(TEXT("x"), A.X);
		Msg->SetNumberField(TEXT("z"), A.Y);
		Msg->SetNumberField(TEXT("x2"), Ground_.X);
		Msg->SetNumberField(TEXT("z2"), Ground_.Y);
		Msg->SetBoolField(TEXT("queue"), bShift);
		R.Message = Msg;
		R.Marker = A;
		R.Units = Ids.Num();
		R.What = FString::Printf(TEXT("%s, %d segment%s%s"), *EntrenchName(Pattern, Fort), Pieces.Num(), Pieces.Num() > 1 ? TEXT("s") : TEXT(""), bShift ? TEXT(" (queued)") : TEXT(""));
		return R;
	}
	// dig: the drag sets the direction past 1.5 m, else it runs across the nearest digger's approach
	const FVector2D C = Aim.Center;
	const double Dir = TCFieldPure::JsHypot(Ground_.X - C.X, Ground_.Y - C.Y) > 1.5 ? std::atan2(Ground_.Y - C.Y, Ground_.X - C.X) : DefaultDigDir(Aim.FortKind, C, Selected, PositionOf);
	const FTCFieldCheck Check = CheckDig(State, Aim.FortKind, bShift, Selected);
	if (!Check.bOk) { R.Feedback = Check.Reason; return R; }
	const FTCFortPlacement F = FortPlacement(State, Aim.FortKind, C, Dir);
	if (F.bThrows) { R.bThrows = true; return R; }
	if (F.bNative) { R.Feedback = F.NativeReason; return R; }
	if (!F.bOk) { R.Feedback = TCOrders::DenySentence(F.Reason, FString()); return R; }
	FVector2D V;
	int32 Id = 0;
	const bool bDigger = NearestDigger(C, Selected, PositionOf, V, Id);
	const FString Kind = Aim.FortKind;
	if (bShift)
	{
		// setAim('dig'): armed again for the next centre (the mesh and its rotation stay)
		Aim.bHasCenter = false;
		Aim.Hint = TEXT("Click to set the center. Right-click cancels");
	}
	else Aim.Cancel();
	if (!bDigger) return R;
	TSharedRef<FJsonObject> Msg = MakeShared<FJsonObject>();
	Msg->SetStringField(TEXT("t"), TEXT("dig"));
	Msg->SetArrayField(TEXT("ids"), TCField::IdArray({ Id }));
	Msg->SetStringField(TEXT("kind"), Kind);
	Msg->SetNumberField(TEXT("x"), C.X);
	Msg->SetNumberField(TEXT("z"), C.Y);
	Msg->SetNumberField(TEXT("dir"), Dir);
	Msg->SetBoolField(TEXT("queue"), bShift);
	R.Message = Msg;
	R.Marker = C;
	R.Units = 1;
	const FTCFortDef* D = Find(Kind);
	R.What = FString::Printf(TEXT("%s, %d cell%s%s"), D ? *D->Name : *Kind, F.Cells.Num(), F.Cells.Num() == 1 ? TEXT("") : TEXT("s"), bShift ? TEXT(" (queued)") : TEXT(""));
	return R;
}

// ---------- works ----------

TArray<FTCWorkGhost> FTCFieldworks::WorkGhosts(const FTCMatchState& State) const
{
	TArray<FTCWorkGhost> Out;
	FString Why;
	if (!ViewReady(State, Why)) return Out;
	TCField::FView View(*this, State);
	for (const FTCWork& Wk : WorkRows)
	{
		// placementCheck(view.game, { kind, x, z, dir }): no sight asked, no team; anything not a FORTS kind is refused
		const int K = TCField::KindIndex(Wk.Kind);
		if (K < 0 || !Wk.bDirNumber) continue;
		FTCFortPlacement P = View.Check(K, Wk.X, Wk.Z, Wk.Dir, false, false);
		if (!P.bOk) continue;
		FTCWorkGhost& G = Out.AddDefaulted_GetRef();
		G.Work = Wk;
		G.Place = MoveTemp(P);
		G.Color = TCField::TileColor(Wk.Kind);
	}
	return Out;
}

bool FTCFieldworks::WorksAt(const FVector2D& G, int32& OutProject) const
{
	// client/main.js worksAt: within 5 m, the nearest that still hands out segments (-1: nothing left to hand out)
	bool bAny = false;
	double Best = 5;
	for (const FTCWork& Wk : WorkRows)
	{
		const double D = TCFieldPure::JsHypot(Wk.X - G.X, Wk.Z - G.Y);
		if (Wk.Project >= 0 && D < Best) { Best = D; OutProject = Wk.Project; bAny = true; }
	}
	return bAny;
}

FTCFieldClick FTCFieldworks::PlanJoin(const FTCMatchState& State, const TSet<int32>& Selected, bool bHasGround, const FVector2D& Ground_, bool bShift) const
{
	FTCFieldClick R;
	int32 Project = -1;
	if (!bHasGround || !bHasSnapshot || !State.SeatConfirmed() || !WorksAt(Ground_, Project)) return R;
	// client/orders.js selection: own selected units that are not structures (an unknown type's definition throws)
	TArray<int32> Diggers_;
	int32 Troops = 0, Trucks = 0;
	for (int32 Id : Selected)
	{
		const FRow* U = Rows.Find(Id);
		if (!U) continue;
		const TCConstructionData::FSightRow* Def = TCField::SightOf(U->Type);
		if (U->Owner != State.You)
		{
			if (!Def) { R.bThrows = true; return R; } // ctx.defs[v.type].building on an undefined definition
			continue;
		}
		if (!Def) { R.bThrows = true; return R; }
		if (Def->bStructure) continue;
		Troops++;
		if (TCField::Eq(U->Type, TEXT("truck"))) Trucks++;
		if (TCField::IsFortBuilder(U->Type)) Diggers_.Add(Id);
	}
	// kind(): no troops, or every troop a truck, is not an entrenchment; then a builder squad must be among them
	if (!Troops || Trucks == Troops || !Diggers_.Num()) return R;
	TSharedRef<FJsonObject> Msg = MakeShared<FJsonObject>();
	Msg->SetStringField(TEXT("t"), TEXT("entrench"));
	Msg->SetArrayField(TEXT("ids"), TCField::IdArray(Diggers_));
	Msg->SetNumberField(TEXT("join"), Project);
	Msg->SetBoolField(TEXT("queue"), bShift);
	R.Message = Msg;
	R.Marker = Ground_;
	R.Units = Diggers_.Num();
	R.What = FString::Printf(TEXT("Join entrenchment %d with %d squad%s%s"), Project, Diggers_.Num(), Diggers_.Num() == 1 ? TEXT("") : TEXT("s"), bShift ? TEXT(" (queued)") : TEXT(""));
	return R;
}
