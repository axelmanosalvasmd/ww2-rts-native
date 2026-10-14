#include "Client/TCConstruction.h"
#include "Client/TCOrders.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Algo/StableSort.h"
#include <cmath>
#include <limits>

#include "Client/TCConstructionData.inl"

const TCHAR* const FTCConstruction::ReferencePin = TCConstructionData::ReferencePin;

static_assert(TCConstructionData::Cell == FTCMatchState::CellMetres, "the generated CELL differs from FTCMatchState::CellMetres");
static_assert(TCConstructionData::LevelHeight == FTCMatchState::LevelMetres, "the generated CFG.levelHeight differs from FTCMatchState::LevelMetres");
static_assert(TCConstructionData::RidingFlag == TCFlags::Riding, "the generated RIDING_FLAG differs from TCFlags::Riding");

// Helpers in a named namespace (not an anonymous one): a unity build puts this file in one translation unit with others
// that define their own Num, Yes, No, JsRound and the like.
namespace TCBuild
{
	namespace Data = ::TCConstructionData;
	constexpr double NaN = std::numeric_limits<double>::quiet_NaN();
	constexpr double Inf = std::numeric_limits<double>::infinity();
	// client/main.js: a unit row's flag bit 1 (retreating)
	constexpr int32 RetreatingFlag = 1;

	struct FTables
	{
		TArray<FTCBuildDef> Defs;
		TMap<FString, int32> Index;
		TMap<FString, const Data::FSightRow*> Sight;
		int32 Flags[128] = {};
		FTables()
		{
			for (const Data::FBuildRow& R : Data::Builds)
			{
				FTCBuildDef D;
				D.Kind = R.Kind; D.Name = R.Name; D.Cost = R.Cost; D.BuildTime = R.BuildTime; D.Size = R.Size; D.Tier = R.Tier;
				D.Needs = R.Needs; D.bCoast = R.bCoast; D.Key = R.Key; D.Role = R.Role;
				Index.Add(D.Kind, Defs.Num());
				Defs.Add(MoveTemp(D));
			}
			for (const Data::FSightRow& R : Data::Sight) Sight.Add(R.Type, &R);
			for (const Data::FTerrainRow& R : Data::Terrain) if (R.Ch < 128) Flags[R.Ch] = R.Flags;
		}
	};
	const FTables& Tables() { static const FTables T; return T; }
	const Data::FSightRow* SightOf(const FString& Type) { const Data::FSightRow* const* R = Tables().Sight.Find(Type); return R ? *R : nullptr; }

	template <int32 N> TArray<FString> ToArray(const TCHAR* const (&List)[N]) { TArray<FString> Out; for (const TCHAR* S : List) Out.Add(S); return Out; }
	template <int32 N> bool InList(const TCHAR* const (&List)[N], const FString& S) { for (const TCHAR* T : List) if (S == T) return true; return false; }

	bool Num(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return true;
	}
	double NumOr(const TSharedPtr<FJsonValue>& V, double Or) { double D; return Num(V, D) ? D : Or; }
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
		default: return false; // null, none
		}
	}
	// a layer cell as the browser stores it: one character, 0 for undefined or '' (falsy), 255 for anything else
	uint8 LayerChar(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid() || V->Type == EJson::None || V->Type == EJson::Null) return 0;
		if (V->Type != EJson::String) return Truthy(V) ? 255 : 0;
		const FString S = V->AsString();
		return S.IsEmpty() ? 0 : S.Len() == 1 && S[0] > 0 && S[0] < 128 ? (uint8)S[0] : 255;
	}

	// V8's Math.hypot for two numbers (src/builtins/math.tq MathHypot): Infinity wins over NaN, then the values scaled by
	// the larger one are summed with Kahan compensation. dist() and the smoke check use it; std::hypot can differ in the
	// last bit.
	double JsHypot(double A, double B)
	{
		const bool bNaN = FMath::IsNaN(A) || FMath::IsNaN(B);
		A = std::fabs(A); B = std::fabs(B);
		double Max = 0;
		if (!FMath::IsNaN(A) && A > Max) Max = A;
		if (!FMath::IsNaN(B) && B > Max) Max = B;
		if (Max == Inf) return Inf;
		if (bNaN) return NaN;
		if (Max == 0) return 0;
		double Sum = 0, Compensation = 0;
		for (const double V : { A, B })
		{
			const double N = V / Max;
			const double Summand = N * N - Compensation;
			const double Preliminary = Sum + Summand;
			Compensation = (Preliminary - Sum) - Summand;
			Sum = Preliminary;
		}
		return std::sqrt(Sum) * Max;
	}
	double JsSign(double V) { return V > 0 ? 1 : V < 0 ? -1 : V; }
	// Math.round: halves go up
	double JsRound(double V) { const double F = std::floor(V); return V - F >= 0.5 ? F + 1 : F; }
	// JavaScript's % on the integers these are (the sign follows the dividend, as C++'s)
	int64 JsMod(int64 A, int64 B) { return A % B; }
	int64 FloorDiv(int64 A, int64 B) { return (int64)std::floor((double)A / (double)B); }

	FTCBuildCheck Yes() { FTCBuildCheck C; C.bOk = true; return C; }
	FTCBuildCheck No(const FString& Reason, bool bNative = false) { FTCBuildCheck C; C.Reason = Reason; C.bNative = bNative; return C; }
	FString Deny(const TCHAR* Code) { return TCOrders::DenySentence(Code, FString()); }

// ---------- the placement view (client/availability.js placementState, shared/sim.js placementCheck and teamSees) ----------

// The game object the browser's placement view builds from this player's snapshot and map, and the shared rules it runs
// on it. Built per call; it reads FTCConstruction's held rows and the map FTCMatchState keeps.
struct FPlacementView
{
	using FRow = FTCConstruction::FRow;
	using FTeam = FTCConstruction::FTeam;
	const FTCConstruction& C;
	const FTCMatchState& S;
	int32 W, H, N;
	FTeam Team;                         // teams[me] ?? me
	TArray<const FRow*> Units;          // g.units, in the browser's row order
	struct FNode { double X, Z; int64 C; int32 Depot; };
	TArray<FNode> Nodes;
	bool bThrew = false;                // the browser would have thrown (see TCConstruction.h)

	FPlacementView(const FTCConstruction& InC, const FTCMatchState& InS) : C(InC), S(InS), W(InC.W), H(InC.H), N(InC.W * InC.H)
	{
		Team = C.MyTeam(S);
		for (int32 Id : C.RowOrder) if (const FRow* R = C.Rows.Find(Id)) Units.Add(R);
		for (const FVector2D& P : C.NodeRows)
		{
			// placementState: c from the node's centre, and the held depot standing on it (its id, 0: none)
			FNode Node{ P.X, P.Y, (int64)(std::floor(P.Y / Data::Cell) - 1) * W + (int64)std::floor(P.X / Data::Cell) - 1, 0 };
			for (int32 Id : C.RowOrder)
			{
				const FRow* R = C.Rows.Find(Id);
				if (R && R->Type == TEXT("depot") && JsHypot(R->X - P.X, R->Z - P.Y) < 1) { Node.Depot = Id; break; }
			}
			Nodes.Add(Node);
		}
	}

	// g.chars[c] and g.flags[c] (TERRAIN of the cell, or of its composed layers when it is 'N'); 0 off the map
	TCHAR Char(int64 Cell) const { return Cell >= 0 && Cell < N ? (TCHAR)S.Map.Chars[Cell] : 0; }
	int32 Flags(int64 Cell) const
	{
		if (Cell < 0 || Cell >= N) return 0;
		uint8 Ch = S.Map.Chars[Cell];
		if (Ch == 'N')
		{
			// shared/world-layers.js composeWorldCell(ground, object): the object when it is one, else the ground
			const uint8 Object = C.Objects.IsValidIndex(Cell) ? C.Objects[Cell] : 0, GroundCh = C.Ground.IsValidIndex(Cell) ? C.Ground[Cell] : 0;
			Ch = Object && Object != '.' ? Object : GroundCh;
		}
		return Ch < 128 ? Tables().Flags[Ch] : 0;
	}
	// level(g, c): the height level; 0 before the map, undefined (NaN) past it
	double Level(int64 Cell) const { return Cell < 0 ? 0 : Cell >= N ? NaN : (double)S.Map.Levels[Cell]; }
	int64 CellOf(double X, double Z) const
	{
		const double CX = std::floor(X / Data::Cell), CY = std::floor(Z / Data::Cell);
		return CX < 0 || CY < 0 || CX >= W || CY >= H ? -1 : (int64)CY * W + (int64)CX;
	}
	FVector2D CellCenter(int64 Cell) const { return FVector2D((JsMod(Cell, W) + 0.5) * Data::Cell, (FloorDiv(Cell, W) + 0.5) * Data::Cell); }
	FVector2D FootCenter(int64 Cell, int32 Size) const { return FVector2D((JsMod(Cell, W) + Size / 2.0) * Data::Cell, (FloorDiv(Cell, W) + Size / 2.0) * Data::Cell); }
	bool Footprint(int64 Cell, int32 Size, TArray<int64>& Out) const
	{
		const int64 X0 = JsMod(Cell, W), Y0 = FloorDiv(Cell, W);
		Out.Reset();
		for (int32 Y = 0; Y < Size; Y++) for (int32 X = 0; X < Size; X++) Out.Add((Y0 + Y) * W + X0 + X);
		return X0 + Size <= W && Y0 + Size <= H;
	}

	// shared/sim.js blocker: the grid ray walk (Amanatides-Woo), true when the line is clear
	bool Clear(double X0, double Z0, double X1, double Z1, int32 Mask, bool bWithEnd) const
	{
		int64 CX = (int64)std::floor(X0 / Data::Cell), CY = (int64)std::floor(Z0 / Data::Cell);
		const int64 EX = (int64)std::floor(X1 / Data::Cell), EY = (int64)std::floor(Z1 / Data::Cell);
		const double DX = X1 - X0, DZ = Z1 - Z0, SX = JsSign(DX), SY = JsSign(DZ);
		const double TDX = DX ? Data::Cell / std::fabs(DX) : Inf, TDY = DZ ? Data::Cell / std::fabs(DZ) : Inf;
		double TX = DX ? (SX > 0 ? (CX + 1) * Data::Cell - X0 : X0 - CX * Data::Cell) / std::fabs(DX) : Inf;
		double TY = DZ ? (SY > 0 ? (CY + 1) * Data::Cell - Z0 : Z0 - CY * Data::Cell) / std::fabs(DZ) : Inf;
		int32 Wood = 0;
		for (int64 Steps = (int64)W + H; Steps > 0 && !(CX == EX && CY == EY); Steps--)
		{
			if (CY == EY || (CX != EX && TX < TY)) { TX += TDX; CX += (int64)SX; } else { TY += TDY; CY += (int64)SY; }
			const bool bEnd = CX == EX && CY == EY;
			if (bEnd && !bWithEnd) return true;
			if (CX < 0 || CY < 0 || CX >= W || CY >= H) return false;
			const int32 F = Flags(CY * W + CX);
			if (F & Mask) return false;
			if ((Mask & Data::SightFlag) && (F & Data::WoodFlag) && ++Wood > Data::WoodSight) return false;
		}
		return true;
	}
	// shared/sim.js segHits, smoked, hillsClear, los
	static bool SegHits(double AX, double AZ, double BX, double BZ, double CX, double CZ, double R)
	{
		const double DX = BX - AX, DZ = BZ - AZ, L2Raw = DX * DX + DZ * DZ, L2 = L2Raw != 0 && !FMath::IsNaN(L2Raw) ? L2Raw : 1;
		const double T = FMath::Max(0.0, FMath::Min(1.0, ((CX - AX) * DX + (CZ - AZ) * DZ) / L2));
		return JsHypot(AX + DX * T - CX, AZ + DZ * T - CZ) < R;
	}
	bool Smoked(double AX, double AZ, double BX, double BZ) const
	{
		if (!C.Smokes.Num() || !(JsHypot(BX - AX, BZ - AZ) >= Data::SmokeSight)) return false;
		for (const FTCConstruction::FSmoke& Sm : C.Smokes) if (SegHits(AX, AZ, BX, BZ, Sm.X, Sm.Z, Sm.R)) return true;
		return false;
	}
	bool HillsClear(double AX, double AZ, double BX, double BZ) const
	{
		const double L = Data::LevelHeight, YA = Level(CellOf(AX, AZ)) * L + Data::Eye, YB = Level(CellOf(BX, BZ)) * L + Data::Eye;
		const double D = JsHypot(BX - AX, BZ - AZ), Steps = std::ceil(D / (Data::Cell / 2));
		for (double I = 1; I < Steps; I++)
		{
			const double T = I / Steps;
			if (Level(CellOf(AX + (BX - AX) * T, AZ + (BZ - AZ) * T)) * L > YA + (YB - YA) * T) return false;
		}
		return true;
	}
	bool Los(const FRow& U, double X, double Z) const { return Clear(U.X, U.Z, X, Z, Data::SightFlag, false) && !Smoked(U.X, U.Z, X, Z) && HillsClear(U.X, U.Z, X, Z); }

	// view.sees(me, at): shared/sim.js teamSees(g, teams[me] ?? me, at) over the received units. The view's players are
	// teams.map(team => ({ team })): a unit of a seat outside them (World's -1) throws, as does an unknown type's sight.
	bool Sees(const FVector2D& At)
	{
		if (bThrew) return false;
		for (const FRow* U : Units)
		{
			bool bSeat = false;
			const FTeam Of = C.TeamOfSeat(U->Owner, bSeat);
			if (!bSeat) { bThrew = true; return false; }
			if (!C.TeamsEqual(Of, Team)) continue;
			const double D = JsHypot(U->X - At.X, U->Z - At.Y);
			const Data::FSightRow* Def = SightOf(U->Type);
			if (!Def) { bThrew = true; return false; }
			// visionOf: a lookout sees 12 m until it is built; the view has no weather (sightMul 1) and no air state
			const double Vision = (Def->bLookout && U->Built < 1 ? 12 : Def->Vision) * 1;
			if (D <= Vision && (Def->bBuilding || Los(*U, At.X, At.Y))) return true;
		}
		return false;
	}

	// shared/sim.js canStamp (without allowOccupied): open ground, at most one level of fall, nothing standing on it
	bool CanStamp(const TArray<int64>& Cells)
	{
		for (int64 K : Cells)
		{
			const TCHAR Ch = Char(K);
			if ((Flags(K) & Data::MoveFlag) || (Ch && FCString::Strchr(Data::NoStamp, Ch))) return false;
		}
		double Max = -Inf, Min = Inf;
		bool bNaN = false;
		for (int64 K : Cells) { const double L = Level(K); if (FMath::IsNaN(L)) bNaN = true; else { Max = FMath::Max(Max, L); Min = FMath::Min(Min, L); } }
		if (bNaN || !(Max - Min <= 1)) return false;
		// stampOccupied: a live ground unit that is not a structure or riding, standing in one of the cells
		for (const FRow* U : Units)
		{
			if (!(U->Hp > 0)) continue;
			const Data::FSightRow* Def = SightOf(U->Type);
			if (!Def) { bThrew = true; return false; }
			if (Def->bAir || (U->Flags & Data::RidingFlag) || Def->bStructure) continue;
			if (Cells.Contains(CellOf(U->X, U->Z))) return false;
		}
		return true;
	}
	// onCoast: open water or surf within two cells of the footprint
	bool OnCoast(const TArray<int64>& Cells) const
	{
		for (int64 K : Cells) for (int32 DY = -2; DY <= 2; DY++) for (int32 DX = -2; DX <= 2; DX++)
		{
			const int64 X = JsMod(K, W) + DX, Y = FloorDiv(K, W) + DY;
			if (X >= 0 && Y >= 0 && X < W && Y < H && !(Flags(Y * W + X) & Data::LandFlag)) return true;
		}
		return false;
	}
	// worldRegionAt over the client's view (no region map): the first held region whose runs or bounds hold the spot
	const FTCConstruction::FRegion* RegionAt(const FVector2D& At) const
	{
		if (!C.bHasWorld) return nullptr;
		for (const FTCConstruction::FRegion& R : C.Regions)
		{
			if (R.bRuns)
			{
				const double X = std::floor(At.X / Data::Cell), Y = std::floor(At.Y / Data::Cell);
				for (const FVector3d& Run : R.Runs) if (Run.X == Y && X >= Run.Y && X < Run.Z) return &R;
			}
			else if (R.bBounds && At.X >= R.Bounds[0] && At.Y >= R.Bounds[1] && At.X < R.Bounds[2] && At.Y < R.Bounds[3]) return &R;
		}
		return nullptr;
	}

	// shared/sim.js placementCheck for a building, as client/main.js footAt calls it (no dir, this side's team)
	FTCPlacement Check(const FString& Kind, const FVector2D& At)
	{
		FTCPlacement P;
		P.At = At;
		auto Fail = [&](const TCHAR* Reason) { P.bOk = false; P.Reason = Reason; return P; };
		auto Threw = [&]() { FTCPlacement T; T.bThrows = true; T.At = At; return T; };
		if (!FMath::IsFinite(At.X) || !FMath::IsFinite(At.Y)) return Fail(TEXT("blocked"));
		const bool bWorld = C.ModeKind == TEXT("world");
		const FTCBuildDef* Def = FTCConstruction::Find(Kind);
		const bool bBuildable = InList(Data::Buildable, Kind) || (Kind == TEXT("hq") && (C.SnapshotSkirmish() || bWorld));
		if (!bBuildable || !Def || (Kind == TEXT("supplycache") && !C.bLogistics)) return Fail(TEXT("blocked"));
		if (bWorld && !Sees(At)) return bThrew ? Threw() : Fail(TEXT("notVisible"));
		int64 Cell;
		if (Kind == TEXT("depot"))
		{
			// the free node nearest the click within 8 m (a stable sort by distance, as Array.prototype.sort)
			TArray<int32> Near;
			for (int32 I = 0; I < Nodes.Num(); I++) if (!Nodes[I].Depot && JsHypot(Nodes[I].X - At.X, Nodes[I].Z - At.Y) <= 8) Near.Add(I);
			Algo::StableSort(Near, [&](int32 A, int32 B) { return JsHypot(Nodes[A].X - At.X, Nodes[A].Z - At.Y) - JsHypot(Nodes[B].X - At.X, Nodes[B].Z - At.Y) < 0; });
			if (!Near.Num())
			{
				bool bHidden = false;
				for (const FNode& Node : Nodes)
				{
					if (JsHypot(Node.X - At.X, Node.Z - At.Y) <= 8 && !Sees(FootCenter(Node.C, Def->Size))) { bHidden = true; break; }
					if (bThrew) return Threw();
				}
				if (bThrew) return Threw();
				return Fail(bHidden ? TEXT("notVisible") : TEXT("blocked"));
			}
			P.Node = Near[0];
			Cell = Nodes[Near[0]].C;
		}
		else
		{
			const double CX = JsRound(At.X / Data::Cell - Def->Size / 2.0), CY = JsRound(At.Y / Data::Cell - Def->Size / 2.0);
			if (CX < 0 || CY < 0 || CX >= W || CY >= H) return Fail(TEXT("blocked"));
			Cell = (int64)CY * W + (int64)CX;
		}
		// from here the answer carries the footprint (extra): its cell, cells and centre
		TArray<int64> Cells;
		const bool bFits = Footprint(Cell, Def->Size, Cells);
		P.Cell = (int32)Cell; P.Size = Def->Size;
		P.At = FootCenter(Cell, Def->Size);
		if (!bFits) return Fail(TEXT("blocked"));
		P.bHasFootprint = true;
		for (int64 K : Cells) P.Cells.Add((int32)K);
		if (bWorld)
		{
			for (int64 K : Cells) { if (!Sees(CellCenter(K))) return bThrew ? Threw() : Fail(TEXT("notVisible")); }
		}
		else if (!Sees(P.At)) return bThrew ? Threw() : Fail(TEXT("notVisible"));
		if (bWorld)
		{
			for (int64 K : Cells)
			{
				const FTCConstruction::FRegion* R = RegionAt(CellCenter(K));
				if (!R || !R->bTeam || C.TeamsEqual(FTeam{ false, R->Team }, Team) == false) return Fail(TEXT("territory"));
			}
		}
		if (C.bHasWorld && C.bRailYard) for (int64 K : Cells) if (C.RailYard.Contains(K)) return Fail(TEXT("rail"));
		if (Kind != TEXT("depot"))
		{
			TSet<int64> Taken;
			TArray<int64> NodeCells;
			for (const FNode& Node : Nodes) if (Footprint(Node.C, 2, NodeCells)) Taken.Append(NodeCells);
			for (int64 K : Cells) if (Taken.Contains(K)) return Fail(TEXT("blocked"));
		}
		if (Def->bCoast && !(C.bNavalMap && OnCoast(Cells))) return Fail(C.bNavalMap ? TEXT("coast") : TEXT("blocked"));
		const bool bStamp = CanStamp(Cells);
		if (bThrew) return Threw();
		if (!bStamp) return Fail(TEXT("blocked"));
		P.bOk = true;
		return P;
	}
};
} // namespace TCBuild

// ---------- the tables ----------

const FTCBuildDef* FTCConstruction::Find(const FString& Kind)
{
	const int32* At = TCBuild::Tables().Index.Find(Kind);
	return At ? &TCBuild::Tables().Defs[*At] : nullptr;
}

TArray<FString> FTCConstruction::BuildKinds(bool bClassic, bool bSkirmish, bool bWorld)
{
	return bClassic ? bWorld ? TCBuild::ToArray(TCBuild::Data::WorldBuilds) : TCBuild::ToArray(TCBuild::Data::Buildable) : bSkirmish ? TCBuild::ToArray(TCBuild::Data::SkirmishBuilds) : TCBuild::ToArray(TCBuild::Data::FieldBuilds);
}

TArray<FString> FTCConstruction::BuilderTypes(bool bClassic)
{
	return bClassic ? TArray<FString>{ TEXT("engineer") } : TCBuild::ToArray(TCBuild::Data::FortBuilders);
}

FString FTCConstruction::KindForKey(const FString& Key)
{
	for (const FTCBuildDef& D : TCBuild::Tables().Defs) if (!D.Key.IsEmpty() && D.Key == Key) return D.Kind;
	return FString();
}

void FTCConstruction::SetUnitTableReference(const FString& Reference)
{
	UnitTableReference = Reference;
	bReferenceMatches = Reference == ReferencePin;
}

// ---------- the messages ----------

void FTCConstruction::Clear()
{
	bStarted = bHasSnapshot = false;
	W = H = 0; bMapWorld = bNavalMap = false;
	Teams.Reset(); Ground.Reset(); Objects.Reset(); RailYard.Reset(); bRailYard = false;
	SnapMp = 0; ModeKind.Reset(); bMode = bSuddenDeath = bModeSlot = false; ModeSlot = 0; bTech = false; TechTier = 0; bLogistics = false;
	OutSeats.Reset(); NodeRows.Reset(); Smokes.Reset(); bHasWorld = false; Regions.Reset();
	Rows.Reset(); RowOrder.Reset();
}

void FTCConstruction::ApplyStart(const FJsonObject& M)
{
	// client/main.js start: lastSnap = null and the held rows reset (worldState.reset, so nodes are not carried over)
	Clear();
	const TSharedPtr<FJsonObject>* MapObj;
	double DW = 0, DH = 0;
	if (!M.TryGetObjectField(TEXT("map"), MapObj) || !MapObj->IsValid() || !(*MapObj)->TryGetNumberField(TEXT("w"), DW) || !(*MapObj)->TryGetNumberField(TEXT("h"), DH)) return;
	if (DW < 1 || DH < 1 || DW > 16384 || DH > 16384) return;
	const FJsonObject& Map = **MapObj;
	bStarted = true;
	W = (int32)DW; H = (int32)DH;
	bMapWorld = TCBuild::Truthy(Map.TryGetField(TEXT("world")));
	const TSharedPtr<FJsonValue> Naval = Map.TryGetField(TEXT("naval"));
	bNavalMap = Naval.IsValid() && Naval->Type == EJson::Boolean && Naval->AsBool();

	// teams = m.teams ?? names.map((_, i) => i)
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	const TSharedPtr<FJsonValue> TeamsField = M.TryGetField(TEXT("teams"));
	if (TeamsField.IsValid() && TeamsField->Type != EJson::Null && TeamsField->TryGetArray(Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			FTeam T;
			if (!V.IsValid() || V->Type == EJson::Null) T.bNull = true; else T.V = TCBuild::NumOr(V, TCBuild::NaN);
			Teams.Add(T);
		}
	}
	else if (M.TryGetArrayField(TEXT("names"), Arr)) for (int32 I = 0; I < Arr->Num(); I++) Teams.Add(FTeam{ false, (double)I });

	// shared/world-layers.js migrateWorldMap: authored layers (worldVersion 2), else split from the rows
	const int32 Count = W * H;
	Ground.Init(0, Count); Objects.Init(0, Count);
	auto Flat = [](const FJsonObject& O, const TCHAR* Field, FString& Out)
	{
		const TArray<TSharedPtr<FJsonValue>>* Lines;
		if (!O.TryGetArrayField(Field, Lines)) return false;
		for (const TSharedPtr<FJsonValue>& L : *Lines) if (L.IsValid() && L->Type == EJson::String) Out += L->AsString();
		return true;
	};
	double Version = 0;
	const TSharedPtr<FJsonObject>* Layers;
	FString GroundText, ObjectText;
	if (Map.TryGetNumberField(TEXT("worldVersion"), Version) && Version == 2 && Map.TryGetObjectField(TEXT("layers"), Layers) && Layers->IsValid())
	{
		Flat(**Layers, TEXT("ground"), GroundText);
		Flat(**Layers, TEXT("objects"), ObjectText);
		ObjectText.ReplaceCharInline(TEXT('N'), TEXT('.')); // worldLayers: a mine in the objects layer is not an object
	}
	else
	{
		FString Rows_;
		Flat(Map, TEXT("rows"), Rows_);
		for (const TCHAR Ch : Rows_)
		{
			const bool bBase = FCString::Strchr(TCBuild::Data::BaseSurfaces, Ch) != nullptr;
			GroundText.AppendChar(bBase ? Ch : Ch == TEXT('=') ? TEXT('W') : TEXT('.'));
			ObjectText.AppendChar(bBase || Ch == TEXT('N') ? TEXT('.') : Ch);
		}
	}
	for (int32 I = 0; I < Count; I++)
	{
		if (I < GroundText.Len()) Ground[I] = GroundText[I] > 0 && GroundText[I] < 128 ? (uint8)GroundText[I] : 255;
		if (I < ObjectText.Len()) Objects[I] = ObjectText[I] > 0 && ObjectText[I] < 128 ? (uint8)ObjectText[I] : 255;
	}
	if (M.TryGetArrayField(TEXT("cells"), Arr)) ApplyCells(*Arr);
}

void FTCConstruction::ApplyCells(const TArray<TSharedPtr<FJsonValue>>& Cells)
{
	// [cell, char, level, state, data]: client/main.js keeps data.ground and data.object in its layers;
	// client/availability.js rememberPlacementTerrain (World) adds every rail cell and station yard to the railYard
	for (const TSharedPtr<FJsonValue>& Entry : Cells)
	{
		const TArray<TSharedPtr<FJsonValue>>* Row;
		double CD;
		if (!Entry.IsValid() || !Entry->TryGetArray(Row) || Row->Num() < 1 || !TCBuild::Num((*Row)[0], CD) || CD != std::floor(CD)) continue;
		const int64 Cell = (int64)CD;
		const TSharedPtr<FJsonObject>* Data = nullptr;
		if (Row->Num() < 5 || !(*Row)[4].IsValid() || !(*Row)[4]->TryGetObject(Data) || !Data->IsValid()) continue;
		if (bMapWorld)
		{
			const bool bRail = TCBuild::Truthy((*Data)->TryGetField(TEXT("rail")));
			const TSharedPtr<FJsonValue> Station = (*Data)->TryGetField(TEXT("station"));
			const bool bStation = Station.IsValid() && Station->Type != EJson::None;
			if (bRail || bStation) bRailYard = true;
			if (bRail) RailYard.Add(Cell);
			if (bStation) for (int32 DY = -TCBuild::Data::StationYard; DY <= TCBuild::Data::StationYard; DY++) for (int32 DX = -TCBuild::Data::StationYard; DX <= TCBuild::Data::StationYard; DX++) RailYard.Add(Cell + (int64)DY * W + DX);
		}
		if (Cell < 0 || Cell >= (int64)W * H) continue;
		Ground[Cell] = TCBuild::LayerChar((*Data)->TryGetField(TEXT("ground")));
		Objects[Cell] = TCBuild::LayerChar((*Data)->TryGetField(TEXT("object")));
	}
}

void FTCConstruction::ApplySnapshot(const FJsonObject& M)
{
	if (!bStarted) return;
	bHasSnapshot = true;
	SnapMp = TCBuild::NumOr(M.TryGetField(TEXT("mp")), TCBuild::NaN);

	ModeKind.Reset(); bMode = bSuddenDeath = bModeSlot = false; ModeSlot = 0;
	const TSharedPtr<FJsonObject>* Obj;
	if (M.TryGetObjectField(TEXT("mode"), Obj) && Obj->IsValid())
	{
		bMode = true;
		(*Obj)->TryGetStringField(TEXT("kind"), ModeKind);
		bSuddenDeath = TCBuild::Truthy((*Obj)->TryGetField(TEXT("suddenDeath")));
		bModeSlot = TCBuild::Num((*Obj)->TryGetField(TEXT("slot")), ModeSlot);
	}
	bTech = M.TryGetObjectField(TEXT("tech"), Obj) && Obj->IsValid();
	TechTier = bTech ? TCBuild::NumOr((*Obj)->TryGetField(TEXT("tier")), TCBuild::NaN) : 0;
	bLogistics = false;
	if (M.TryGetObjectField(TEXT("logistics"), Obj) && Obj->IsValid())
	{
		const TSharedPtr<FJsonValue> E = (*Obj)->TryGetField(TEXT("enabled"));
		bLogistics = E.IsValid() && E->Type == EJson::Boolean && E->AsBool();
	}
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	OutSeats.Reset();
	if (M.TryGetArrayField(TEXT("out"), Arr)) for (const TSharedPtr<FJsonValue>& V : *Arr) OutSeats.Add(TCBuild::Truthy(V));

	// s.nodes ??= last?.nodes
	const TSharedPtr<FJsonValue> NodesField = M.TryGetField(TEXT("nodes"));
	if (NodesField.IsValid() && NodesField->Type != EJson::Null && NodesField->TryGetArray(Arr))
	{
		NodeRows.Reset();
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			if (V.IsValid() && V->TryGetArray(Row) && Row->Num() >= 2) NodeRows.Add(FVector2D(TCBuild::NumOr((*Row)[0], TCBuild::NaN), TCBuild::NumOr((*Row)[1], TCBuild::NaN)));
		}
	}
	Smokes.Reset();
	if (M.TryGetArrayField(TEXT("smokes"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			if (V.IsValid() && V->TryGetArray(Row) && Row->Num() >= 3) Smokes.Add(FSmoke{ TCBuild::NumOr((*Row)[0], TCBuild::NaN), TCBuild::NumOr((*Row)[1], TCBuild::NaN), TCBuild::NumOr((*Row)[2], TCBuild::NaN) });
		}
	}
	// world: the regions this side knows (runs on a region map, else bounds in metres)
	bHasWorld = false; Regions.Reset();
	if (M.TryGetObjectField(TEXT("world"), Obj) && Obj->IsValid())
	{
		bHasWorld = true;
		if ((*Obj)->TryGetArrayField(TEXT("regions"), Arr))
		{
			for (const TSharedPtr<FJsonValue>& V : *Arr)
			{
				const TSharedPtr<FJsonObject>* RO;
				FRegion R;
				if (V.IsValid() && V->TryGetObject(RO) && RO->IsValid())
				{
					R.bTeam = TCBuild::Num((*RO)->TryGetField(TEXT("team")), R.Team);
					const TArray<TSharedPtr<FJsonValue>>* List;
					const TSharedPtr<FJsonValue> RunsField = (*RO)->TryGetField(TEXT("runs"));
					if (TCBuild::Truthy(RunsField))
					{
						R.bRuns = true;
						if (RunsField->TryGetArray(List))
						{
							for (const TSharedPtr<FJsonValue>& Run : *List)
							{
								const TArray<TSharedPtr<FJsonValue>>* RunRow;
								if (Run.IsValid() && Run->TryGetArray(RunRow) && RunRow->Num() >= 3) R.Runs.Add(FVector3d(TCBuild::NumOr((*RunRow)[0], TCBuild::NaN), TCBuild::NumOr((*RunRow)[1], TCBuild::NaN), TCBuild::NumOr((*RunRow)[2], TCBuild::NaN)));
							}
						}
					}
					else if ((*RO)->TryGetArrayField(TEXT("bounds"), List) && List->Num() >= 4)
					{
						R.bBounds = true;
						for (int32 I = 0; I < 4; I++) R.Bounds[I] = TCBuild::NumOr((*List)[I], TCBuild::NaN);
					}
				}
				Regions.Add(MoveTemp(R));
			}
		}
	}
	if (M.TryGetArrayField(TEXT("cells"), Arr)) ApplyCells(*Arr);

	// client/world-state.js apply: a full resend (or no held) replaces the rows, gone ids leave, rows upsert in place
	if (TCBuild::Truthy(M.TryGetField(TEXT("all"))) || !TCBuild::Truthy(M.TryGetField(TEXT("held")))) { Rows.Reset(); RowOrder.Reset(); }
	if (M.TryGetArrayField(TEXT("gone"), Arr))
	{
		TSet<int32> Gone;
		double D;
		for (const TSharedPtr<FJsonValue>& V : *Arr) if (TCBuild::Num(V, D) && Rows.Remove((int32)D)) Gone.Add((int32)D);
		if (Gone.Num()) RowOrder.RemoveAll([&Gone](int32 Id) { return Gone.Contains(Id); });
	}
	if (M.TryGetArrayField(TEXT("units"), Arr))
	{
		for (const TSharedPtr<FJsonValue>& V : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			double D;
			if (!V.IsValid() || !V->TryGetArray(Row) || Row->Num() < 1 || !TCBuild::Num((*Row)[0], D)) continue;
			const int32 Id = (int32)D;
			const TArray<TSharedPtr<FJsonValue>>& R = *Row;
			auto At = [&R](int32 I) { return I < R.Num() ? R[I] : TSharedPtr<FJsonValue>(); };
			FRow U;
			if (At(1).IsValid() && At(1)->Type == EJson::String) U.Type = At(1)->AsString();
			U.Owner = TCBuild::NumOr(At(2), TCBuild::NaN); U.X = TCBuild::NumOr(At(3), TCBuild::NaN); U.Z = TCBuild::NumOr(At(4), TCBuild::NaN); U.Hp = TCBuild::NumOr(At(7), TCBuild::NaN);
			U.Flags = (int32)(int64)TCBuild::NumOr(At(12), 0);
			const TSharedPtr<FJsonValue> Built = At(14);
			U.Built = Built.IsValid() && Built->Type != EJson::Null && Built->Type != EJson::None ? TCBuild::NumOr(Built, TCBuild::NaN) : 1; // built ?? 1
			if (!Rows.Contains(Id)) RowOrder.Add(Id);
			Rows.Add(Id, MoveTemp(U));
		}
	}
}

// ---------- shared rules over the snapshot ----------

bool FTCConstruction::SnapshotClassic() const { return ModeKind == TEXT("classic") || ModeKind == TEXT("world"); }

bool FTCConstruction::SnapshotSkirmish() const
{
	// isSkirmishBaseMode: no mode kind, or one of the skirmish base modes
	return ModeKind.IsEmpty() || ModeKind == TEXT("conquest") || ModeKind == TEXT("assault") || ModeKind == TEXT("annihilation") || ModeKind == TEXT("horde");
}

bool FTCConstruction::LobbyClassic(const FTCMatchState& State) const
{
	// client/main.js classicMode: the lobby's mode, or a World map
	return State.Mode == TEXT("classic") || State.Mode == TEXT("world") || bMapWorld;
}

FTCConstruction::FTeam FTCConstruction::TeamOfSeat(double Seat, bool& bOutExists) const
{
	bOutExists = Seat >= 0 && Seat < Teams.Num() && Seat == std::floor(Seat);
	return bOutExists ? Teams[(int32)Seat] : FTeam();
}

bool FTCConstruction::TeamsEqual(const FTeam& A, const FTeam& B) const
{
	if (A.bNull || B.bNull) return A.bNull && B.bNull;
	return A.V == B.V;
}

FTCConstruction::FTeam FTCConstruction::MyTeam(const FTCMatchState& State) const
{
	// teams[me] ?? me
	bool bSeat = false;
	const FTeam T = TeamOfSeat(State.You, bSeat);
	return bSeat && !T.bNull ? T : FTeam{ false, (double)State.You };
}

bool FTCConstruction::Allied(double A, double B) const
{
	// shared/sim.js allied over players = teams.map(team => ({ team }))
	if (ModeKind == TEXT("world") && A == -1 && B == -1) return true;
	bool bA = false, bB = false;
	const FTeam TA = TeamOfSeat(A, bA), TB = TeamOfSeat(B, bB);
	return A >= 0 && B >= 0 && bA && bB && TeamsEqual(TA, TB);
}

bool FTCConstruction::ProductionAccess(const FTCMatchState& State, double Owner) const
{
	// shared/sim.js productionAccess(baseView, v, slot)
	const double Slot = State.You;
	if (Owner == Slot) return true;
	if (ModeKind != TEXT("horde")) return false;
	return !(bModeSlot && Slot == ModeSlot) && !(bModeSlot && Owner == ModeSlot) && Allied(Owner, Slot);
}

bool FTCConstruction::Owns(const FTCMatchState& State, const FString& Type) const
{
	// client/main.js owns(type): a held unit of this seat, of that type, finished
	for (const auto& Pair : Rows) if (Pair.Value.Owner == State.You && Pair.Value.Type == Type && Pair.Value.Built >= 1) return true;
	return false;
}

FTCBuildCheck FTCConstruction::Gate(const FTCMatchState& State) const
{
	if (!bReferenceMatches)
	{
		return TCBuild::No(FString::Printf(TEXT("Construction data is for reference %s, the unit table is for %s"), ReferencePin,
			UnitTableReference.IsEmpty() ? TEXT("an unknown reference") : *UnitTableReference), true);
	}
	if (!bHasSnapshot) return TCBuild::No(TEXT("Waiting for the army"));
	if (State.bSpectator || (OutSeats.IsValidIndex(State.You) && OutSeats[State.You])) return TCBuild::No(TEXT("You are spectating"));
	if (!State.SeatConfirmed()) return TCBuild::No(TEXT("Waiting for the lobby to confirm this seat"), true);
	return TCBuild::Yes();
}

// ---------- what the player sees and does ----------

FTCBuildCheck FTCConstruction::CheckBuild(const FTCMatchState& State, const FString& Kind, const TSet<int32>& Selected) const
{
	const FTCBuildCheck G = Gate(State);
	if (!G.bOk) return G;
	const bool bClassic = SnapshotClassic(), bSudden = bClassic && bSuddenDeath, bSkirmish = SnapshotSkirmish();
	if (!BuildKinds(bClassic, bSkirmish, ModeKind == TEXT("world")).Contains(Kind) || (Kind == TEXT("armory") && !bTech)) return TCBuild::No(TEXT("Unavailable in this mode"));
	const FTCBuildDef* Def = Find(Kind);
	if (!Def) return TCBuild::No(FString::Printf(TEXT("No construction data for '%s'"), *Kind), true); // the generated table holds every buildKinds kind
	if (bTech && Def->Tier > TechTier) return TCBuild::No(TEXT("Needs ") + FTCRecruitment::TierName(Def->Tier));
	if (bSudden) return TCBuild::No(TCBuild::Deny(TEXT("suddenDeath")));
	// the crew: own selected units of a builder type (by the snapshot's mode)
	const TArray<FString> Can = BuilderTypes(bClassic);
	int32 Crew = 0, Retreating = 0;
	for (int32 Id : RowOrder)
	{
		const FRow& U = Rows[Id];
		if (U.Owner != State.You || !Selected.Contains(Id) || !Can.Contains(U.Type)) continue;
		Crew++;
		if (U.Flags & TCBuild::RetreatingFlag) Retreating++;
	}
	if (!Crew) return TCBuild::No(TCBuild::Deny(TEXT("noBuilders")));
	if (Crew == Retreating) return TCBuild::No(TCBuild::Deny(TEXT("retreating")));
	if (SnapMp < Def->Cost) return TCBuild::No(FString::Printf(TEXT("Needs %d MP"), Def->Cost));
	if (!Def->Needs.IsEmpty())
	{
		bool bHave = false;
		for (const auto& Pair : Rows)
		{
			const FRow& U = Pair.Value;
			if (ProductionAccess(State, U.Owner) && U.Type == Def->Needs && U.Built >= 1 && U.Hp > 0) { bHave = true; break; }
		}
		const TCBuild::Data::FSightRow* Needed = TCBuild::SightOf(Def->Needs);
		if (!bHave) return TCBuild::No(FString::Printf(TEXT("Needs a %s"), Needed ? Needed->Name : *Def->Needs));
	}
	return TCBuild::Yes();
}

FTCBuildCard FTCConstruction::BuildCard(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	// client/hud.js update: the Classic card is drawn from snapshots, in Classic and World, unless every selected unit is
	// a truck (then it stays as it was, or the logistics card replaces it); drawClassicCard: one own selected building
	// gets its own card, else an own selected Engineer gets the Build card
	FTCBuildCard Card;
	if (!bHasSnapshot || !SnapshotClassic()) return Card;
	TArray<const FRow*> Sel;
	for (int32 Id : Selected) if (const FRow* U = Rows.Find(Id)) Sel.Add(U);
	if (Sel.Num() && !Sel.ContainsByPredicate([](const FRow* U) { return U->Type != TEXT("truck"); })) return Card;
	const TCBuild::Data::FSightRow* One = Sel.Num() == 1 ? TCBuild::SightOf(Sel[0]->Type) : nullptr;
	const bool bBuilding = One && One->bBuilding && Sel[0]->Owner == State.You;
	if (bBuilding || !Sel.ContainsByPredicate([&State](const FRow* U) { return U->Owner == State.You && U->Type == TEXT("engineer"); })) return Card;
	Card.bShown = true;
	for (const FString& K : BuildKinds(true, false, ModeKind == TEXT("world")))
	{
		if ((K == TEXT("supplycache") && !bLogistics) || (K == TEXT("armory") && !bTech)) continue;
		const FTCBuildDef* D = Find(K);
		if (!D) continue;
		FTCBuildButton& B = Card.Buttons.AddDefaulted_GetRef();
		B.Kind = K; B.Name = D->Name; B.Key = D->Key;
		B.Tip = FString::Printf(TEXT("%s%s: %s. %d MP, %ds"), *D->Name, D->Key.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *D->Key), *D->Role, D->Cost, D->BuildTime);
		B.Cost = FString::Printf(TEXT("%d MP, %ds"), D->Cost, D->BuildTime);
		B.Check = CheckBuild(State, K, Selected);
		B.bBroke = !(SnapMp >= D->Cost);
		B.Note = B.Check.bOk ? D->Role : B.Check.Reason;
		B.bDanger = !D->Needs.IsEmpty() && !Owns(State, D->Needs);
	}
	return Card;
}

FTCBuildMenu FTCConstruction::BuildMenu(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	// client/hud.js drawOrders: the Build menu exists for a selection that is not all buildings nor all trucks and holds
	// a builder squad (any owner's, as the browser checks); menuHTML('build') lists these buildings
	FTCBuildMenu Menu;
	if (!bHasSnapshot) return Menu;
	TArray<const FRow*> Sel;
	for (int32 Id : Selected) if (const FRow* U = Rows.Find(Id)) Sel.Add(U);
	if (!Sel.Num()) return Menu;
	if (!Sel.ContainsByPredicate([](const FRow* U) { return U->Type != TEXT("truck"); })) return Menu;
	if (!Sel.ContainsByPredicate([](const FRow* U) { const TCBuild::Data::FSightRow* D = TCBuild::SightOf(U->Type); return !D || !D->bBuilding; })) return Menu;
	if (!Sel.ContainsByPredicate([](const FRow* U) { return TCBuild::InList(TCBuild::Data::FortBuilders, U->Type); })) return Menu;
	Menu.bOffered = true;
	auto Add = [&](const FString& K, const FString& Tab, const FString& Tip)
	{
		const FTCBuildDef* D = Find(K);
		if (!D) return;
		FTCBuildButton& B = Menu.Buttons.AddDefaulted_GetRef();
		B.Kind = K; B.Name = D->Name; B.Tab = Tab; B.Tip = Tip;
		B.Cost = FString::Printf(TEXT("%d MP"), D->Cost);
		B.Check = CheckBuild(State, K, Selected);
	};
	if (bLogistics) Add(TEXT("supplycache"), TEXT("base"), TEXT("Supply Cache: stores delivered supplies. 60 MP, 12 s"));
	if (!LobbyClassic(State))
	{
		for (const FString& K : BuildKinds(false, SnapshotSkirmish(), false))
		{
			if (K == TEXT("supplycache") || (K == TEXT("shipyard") && !bNavalMap) || (K == TEXT("armory") && !bTech)) continue;
			const FTCBuildDef* D = Find(K);
			if (!D) continue;
			Add(K, K == TEXT("flakpos") || K == TEXT("pillbox") || K == TEXT("tower") ? TEXT("defense") : TEXT("base"),
				FString::Printf(TEXT("%s: %d MP, %ds. Click where; selected builder squads construct it. Right-click a damaged building to repair it"), *D->Name, D->Cost, D->BuildTime));
		}
	}
	return Menu;
}

FTCBuildArm FTCConstruction::Arm(const FTCMatchState& State, const FString& Kind, const TSet<int32>& Selected) const
{
	// client/main.js startBuild: explain a refusal, else canPlace(k) (availability again) and setAim(k)
	FTCBuildArm A;
	A.Check = CheckBuild(State, Kind, Selected);
	if (!A.Check.bOk) return A;
	A.bArm = true;
	A.Hint = Kind == TEXT("depot") ? TEXT("Click a resource node") : Kind == TEXT("barracks") || Kind == TEXT("motorpool") ? TEXT("Click where to build") : TEXT("Click to set the center");
	A.Hint += TEXT(". Right-click cancels");
	return A;
}

FTCPlacement FTCConstruction::Placement(const FTCMatchState& State, const FString& Kind, const FVector2D& At) const
{
	FTCPlacement P;
	P.At = At;
	P.Reason = TEXT("notVisible"); // footAt without a placement view
	if (!bReferenceMatches) { P.bNative = true; P.NativeReason = Gate(State).Reason; return P; }
	// client/main.js placementView: none before the first snapshot of this match
	if (!bHasSnapshot || !State.bHasMatch) return P;
	if (State.Map.W != W || State.Map.H != H || State.Map.Chars.Num() != W * H || State.Map.Levels.Num() != W * H)
	{
		P.bNative = true;
		P.NativeReason = TEXT("The map held does not match the start the placement view was built from");
		return P;
	}
	if (!Find(Kind) && TCBuild::InList(TCBuild::Data::Forts, Kind))
	{
		P.bNative = true;
		P.NativeReason = TEXT("Fortification placement is not ported");
		return P;
	}
	TCBuild::FPlacementView View(*this, State);
	return View.Check(Kind, At);
}

TArray<int32> FTCConstruction::Builders(const FTCMatchState& State, const TSet<int32>& Selected) const
{
	const TArray<FString> Can = BuilderTypes(LobbyClassic(State));
	TArray<int32> Ids;
	for (int32 Id : Selected)
	{
		const FRow* U = Rows.Find(Id);
		if (U && Can.Contains(U->Type) && !(U->Flags & TCBuild::RetreatingFlag)) Ids.Add(Id);
	}
	return Ids;
}

FTCBuildClick FTCConstruction::Click(const FTCMatchState& State, const FString& Kind, bool bHasGround, const FVector2D& Ground_, bool bShift, const TSet<int32>& Selected) const
{
	// client/main.js mousedown with a building armed
	FTCBuildClick R;
	if (!bHasGround) { R.Feedback = TCBuild::Deny(TEXT("blocked")); return R; }
	R.Check = CheckBuild(State, Kind, Selected);
	if (!R.Check.bOk) { R.Feedback = R.Check.Reason; return R; }
	R.Placement = Placement(State, Kind, Ground_);
	if (R.Placement.bThrows) { R.bThrows = true; return R; }
	if (R.Placement.bNative) { R.Feedback = R.Placement.NativeReason; return R; }
	if (!R.Placement.bOk) { R.Feedback = TCOrders::DenySentence(R.Placement.Reason, FString()); return R; }
	R.bKeepAim = bShift;
	TSharedRef<FJsonObject> Msg = MakeShared<FJsonObject>();
	Msg->SetStringField(TEXT("t"), TEXT("build"));
	TArray<TSharedPtr<FJsonValue>> Ids;
	for (int32 Id : Builders(State, Selected)) Ids.Add(MakeShared<FJsonValueNumber>(Id));
	Msg->SetArrayField(TEXT("ids"), Ids);
	Msg->SetStringField(TEXT("kind"), Kind);
	Msg->SetNumberField(TEXT("x"), R.Placement.At.X);
	Msg->SetNumberField(TEXT("z"), R.Placement.At.Y);
	Msg->SetBoolField(TEXT("queue"), bShift);
	R.Message = Msg;
	R.Marker = R.Placement.At;
	return R;
}
