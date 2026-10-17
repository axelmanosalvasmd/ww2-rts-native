#include "Client/TCCoverPreview.h"
#include "Net/TCMatchState.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "ThreeCrossroads.h"

const TCHAR* const FTCCoverPreview::ReferencePin = TEXT("e4e579a");

namespace TCCoverImpl
{
	// shared/sim.js TERRAIN bits the preview reads.
	constexpr int32 MoveBit = 1, CoverBit = 4, TrenchBit = 8;
	// The 8 sides of a cell: E, SE, S, SW, W, NW, N, NE in world x and z (+z is south on the map). The diagonal is
	// Math.SQRT1_2.
	constexpr double Diag = 0.7071067811865476;
	constexpr double SideC[8] = { 1, Diag, 0, -Diag, -1, -Diag, 0, Diag };
	constexpr double SideS[8] = { 0, Diag, 1, Diag, 0, -Diag, -1, -Diag };
	// AROUND: the 8 neighbours, in the reference's order.
	constexpr int32 Around[8][2] = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 }, { 1, 1 }, { 1, -1 }, { -1, 1 }, { -1, -1 } };
	// shared/sim.js SOLID: a house, wall or sandbags, rubble, hedge, a Classic building or a wreck; WALLS: a house or a
	// Classic building.
	bool IsSolid(uint8 Ch) { return Ch == 'B' || Ch == '#' || Ch == 'R' || Ch == 'H' || Ch == 'K' || Ch == 'Q'; }
	bool IsWall(uint8 Ch) { return Ch == 'B' || Ch == 'K'; }
	// walkable: !((TERRAIN[ch] ?? MOVE) & MOVE)
	bool Walkable(uint8 Ch)
	{
		int32 F;
		return FTCCoverPreview::TerrainFlags(Ch, F) && !(F & MoveBit);
	}
	// SIZE and ALPHA per kind (index: ETCCoverKind) and for the chevron.
	constexpr double KindSize[4] = { 0, 1.05, 1.15, 1.3 };
	constexpr double KindAlpha[4] = { 0, 0.85, 0.95, 0.95 };
	constexpr double ChevronSize = 0.85;
	constexpr double ChevronAlpha = 1;
	// the shield's tilt: tip toward +z
	constexpr double Tilt = -UE_DOUBLE_HALF_PI;
	// The template points (client/cover-preview.js templates): x, z.
	const FVector2D ShieldPts[] = { { -0.34, 0.36 }, { 0.34, 0.36 }, { 0.34, -0.04 }, { 0.2, -0.3 }, { 0, -0.45 }, { -0.2, -0.3 }, { -0.34, -0.04 } };
	const FVector2D ChevronPts[] = { { -0.26, -0.22 }, { 0, 0.12 }, { 0.26, -0.22 } };

	// One layer of a region being written: glyph() stops at the layer's capacity.
	struct FWriter
	{
		TArray<FTCCoverGlyph>& Out;
		int32 Cap;
		void Add(bool bChevron, ETCCoverKind Kind, double X, double Z, double Size, double Angle, float Alpha)
		{
			if (Out.Num() >= Cap) return;
			FTCCoverGlyph& G = Out.AddDefaulted_GetRef();
			G.bChevron = bChevron; G.Kind = Kind; G.X = X; G.Z = Z; G.Size = Size; G.Angle = Angle;
			G.Color = FTCCoverPreview::KindColor(bChevron ? ETCCoverKind::Light : Kind);
			G.Alpha = Alpha;
		}
		// pip(kind, x, z, tilt, fade)
		void Pip(ETCCoverKind Kind, double X, double Z, double Fade)
		{
			Add(false, Kind, X, Z, KindSize[(int32)Kind], Tilt, (float)(KindAlpha[(int32)Kind] * Fade));
		}
		// chevron(x, z, ca, sa, fade)
		void Chevron(double X, double Z, double Ca, double Sa, double Fade)
		{
			Add(true, ETCCoverKind::Light, X + Ca * FTCCoverPreview::ChevronOffset, Z + Sa * FTCCoverPreview::ChevronOffset, ChevronSize, FMath::Atan2(Sa, Ca), (float)(ChevronAlpha * Fade));
		}
	};

	// Replaces Into's marks with Next's, moving Revision only when they differ.
	void Commit(FTCCoverMarks& Into, FTCCoverMarks& Next)
	{
		if (Into.Ground == Next.Ground && Into.Through == Next.Through) return;
		Swap(Into.Ground, Next.Ground);
		Swap(Into.Through, Next.Through);
		Into.Revision++;
	}
	void Clear(FTCCoverMarks& Marks)
	{
		if (Marks.Ground.IsEmpty() && Marks.Through.IsEmpty()) return;
		Marks.Reset();
		Marks.Revision++;
	}
	// Math.hypot(X, Y) as V8 computes it: both scaled by the larger, a compensated sum, then the square root scaled back.
	// sqrt(X * X + Y * Y) can differ from it in the last bit, which moves a mark that sits exactly on a threshold.
	double Hypot(double X, double Y)
	{
		const double Ax = FMath::Abs(X), Ay = FMath::Abs(Y), Big = TNumericLimits<double>::Max();
		if (Ax > Big || Ay > Big) return Ax > Big ? Ax : Ay; // an infinite side wins over NaN, as in V8
		if (FMath::IsNaN(X) || FMath::IsNaN(Y)) return X + Y;
		const double Max = FMath::Max(Ax, Ay);
		if (Max == 0) return 0;
		double Sum = 0, Compensation = 0;
		for (const double V : { Ax, Ay })
		{
			const double N = V / Max, Summand = N * N - Compensation, Preliminary = Sum + Summand;
			Compensation = (Preliminary - Sum) - Summand;
			Sum = Preliminary;
		}
		return FMath::Sqrt(Sum) * Max;
	}
	double Smooth(double T)
	{
		const double K = FMath::Clamp(T, 0.0, 1.0);
		return K * K * (3 - 2 * K);
	}
}

// ---------- rules ----------

bool FTCCoverPreview::TerrainFlags(uint8 Ch, int32& OutFlags)
{
	// shared/sim.js TERRAIN at ReferencePin (the LAND bit left out: the preview does not read it)
	switch (Ch)
	{
	case '.': case 'N': case 'A': case 'M': case 'F': case 'X': OutFlags = 0; return true;
	case 'B': case 'K': OutFlags = TCCoverImpl::MoveBit | 2; return true;       // MOVE | SIGHT
	case 'H': OutFlags = 2 | TCCoverImpl::CoverBit; return true;                // SIGHT | COVER
	case '#': case '+': OutFlags = TCCoverImpl::CoverBit; return true;
	case 'T': OutFlags = TCCoverImpl::CoverBit | TCCoverImpl::TrenchBit; return true;
	case 'W': OutFlags = TCCoverImpl::MoveBit; return true;
	case '=': case 'D': OutFlags = 128; return true;               // ROAD
	case 'R': case 'Y': case 'Q': OutFlags = 64 | TCCoverImpl::CoverBit; return true; // VBLOCK | COVER
	case 'O': OutFlags = TCCoverImpl::CoverBit | 512; return true;              // COVER | WOOD
	default: OutFlags = 0; return false;
	}
}

bool FTCCoverPreview::HouseEdge(const FTCCoverGrid& G, int32 X, int32 Y)
{
	const auto Step = [&G](int32 Nx, int32 Ny) { const uint8 Ch = G.At(Nx, Ny); return Ch != 'B' && TCCoverImpl::Walkable(Ch); };
	return Step(X + 1, Y) || Step(X - 1, Y) || Step(X, Y + 1) || Step(X, Y - 1);
}

ETCCoverKind FTCCoverPreview::CellCover(const FTCCoverGrid& G, int32 X, int32 Y, bool bGarrisons)
{
	const uint8 Ch = G.At(X, Y);
	int32 F;
	if (!TerrainFlags(Ch, F)) return ETCCoverKind::None;
	if (Ch == 'B') return bGarrisons && HouseEdge(G, X, Y) ? ETCCoverKind::Heavy : ETCCoverKind::None;
	if (F & TCCoverImpl::MoveBit) return ETCCoverKind::None;
	return F & TCCoverImpl::TrenchBit ? ETCCoverKind::Heavy : F & TCCoverImpl::CoverBit ? ETCCoverKind::Light : ETCCoverKind::Open;
}

bool FTCCoverPreview::SolidToward(const FTCCoverGrid& G, double Px, double Pz, double Ca, double Sa)
{
	// Stepping out as the sim does, the first solid cell decides: within 2.2 m it covers, at 2.9 m only while not
	// nearly gone, never at 3.6 m.
	for (double D = 0.8; D < 3.7; D += 0.7)
	{
		const int32 X = FMath::FloorToInt32((Px + Ca * D) / CellMetres), Y = FMath::FloorToInt32((Pz + Sa * D) / CellMetres);
		if (!TCCoverImpl::IsSolid(G.At(X, Y))) continue;
		if (D < 2.5 || (D < 3.3 && G.Stage(X, Y) < 2)) return true;
		break;
	}
	// a house wall in a cell next to the squad, within 60 degrees of the shooter (leaning out from the corner)
	const int32 X = FMath::FloorToInt32(Px / CellMetres), Y = FMath::FloorToInt32(Pz / CellMetres);
	for (const auto& A : TCCoverImpl::Around)
		if (TCCoverImpl::IsWall(G.At(X + A[0], Y + A[1])) && (A[0] * Ca + A[1] * Sa) / TCCoverImpl::Hypot(A[0], A[1]) > 0.5) return true;
	return false;
}

bool FTCCoverPreview::VehicleToward(TArrayView<const FVector2D> Hulls, double Px, double Pz, double Ca, double Sa)
{
	for (const FVector2D& V : Hulls)
	{
		const double Dx = V.X - Px, Dz = V.Y - Pz, D = TCCoverImpl::Hypot(Dx, Dz);
		if (VehicleCovers(D) && (Dx * Ca + Dz * Sa) / D > 0.7) return true;
	}
	return false;
}

bool FTCCoverPreview::Modelled(uint8 Ch)
{
	return Ch == 'B' || Ch == 'H' || Ch == '#' || Ch == 'Y' || Ch == 'R' || Ch == 'O' || Ch == 'Q';
}

bool FTCCoverPreview::ReferenceUnitFacts(const FString& Type, FTCCoverUnitFacts& Out)
{
	// shared/sim.js UNITS at ReferencePin: infantry, air, garrisons, w && !w.salvo. Generated, not typed.
	struct FRow { const TCHAR* Type; FTCCoverUnitFacts Facts; };
	static const FRow Rows[] = {
		{ TEXT("rifle"), { true, false, true, true } },
		{ TEXT("mg"), { true, false, true, true } },
		{ TEXT("at"), { true, false, false, true } },
		{ TEXT("tank"), { false, false, false, true } },
		{ TEXT("rocket"), { false, false, false, false } },
		{ TEXT("flak"), { true, false, false, true } },
		{ TEXT("fighter"), { false, true, false, true } },
		{ TEXT("attacker"), { false, true, false, true } },
		{ TEXT("flaktrack"), { false, false, false, true } },
		{ TEXT("mortar"), { true, false, false, false } },
		{ TEXT("sniper"), { true, false, true, true } },
		{ TEXT("armoredcar"), { false, false, false, true } },
		{ TEXT("medium"), { false, false, false, true } },
		{ TEXT("halftrack"), { false, false, false, true } },
		{ TEXT("lorry"), { false, false, false, true } },
		{ TEXT("train"), { false, false, false, true } },
		{ TEXT("lcvp"), { false, false, false, true } },
		{ TEXT("gunboat"), { false, false, false, true } },
		{ TEXT("destroyer"), { false, false, false, false } },
		{ TEXT("medic"), { true, false, true, true } },
		{ TEXT("tankdestroyer"), { false, false, false, true } },
		{ TEXT("howitzer"), { true, false, false, false } },
		{ TEXT("flamer"), { true, false, true, true } },
		{ TEXT("bomber"), { false, true, false, false } },
		{ TEXT("ranger"), { true, false, true, true } },
		{ TEXT("tiger"), { false, false, false, true } },
		{ TEXT("conscript"), { true, false, true, true } },
		{ TEXT("churchill"), { false, false, false, true } },
		{ TEXT("commando"), { true, false, true, true } },
		{ TEXT("bunker"), { false, false, false, true } },
		{ TEXT("kaiju"), { false, false, false, true } },
		{ TEXT("engineer"), { true, false, true, true } },
		{ TEXT("hq"), { false, false, false, false } },
		{ TEXT("depot"), { false, false, false, false } },
		{ TEXT("barracks"), { false, false, false, false } },
		{ TEXT("motorpool"), { false, false, false, false } },
		{ TEXT("airfield"), { false, false, false, false } },
		{ TEXT("shipyard"), { false, false, false, false } },
		{ TEXT("flakpos"), { false, false, false, false } },
		{ TEXT("worldbase"), { false, false, false, false } },
		{ TEXT("truck"), { false, false, false, true } },
		{ TEXT("armory"), { false, false, false, false } },
		{ TEXT("pillbox"), { false, false, false, true } },
		{ TEXT("tower"), { false, false, false, false } },
		{ TEXT("wall"), { false, false, false, false } },
		{ TEXT("gate"), { false, false, false, false } },
		{ TEXT("supplycache"), { false, false, false, false } },
	};
	static const TMap<FString, FTCCoverUnitFacts> Table = []
	{
		TMap<FString, FTCCoverUnitFacts> M;
		for (const FRow& R : Rows) M.Add(FString(R.Type), R.Facts);
		return M;
	}();
	const FTCCoverUnitFacts* Found = Table.Find(Type);
	if (!Found) return false;
	Out = *Found;
	return true;
}

FColor FTCCoverPreview::KindColor(ETCCoverKind Kind)
{
	switch (Kind)
	{
	case ETCCoverKind::Heavy: return FColor(0xb2, 0xde, 0x92);
	case ETCCoverKind::Light: return FColor(0xe2, 0xb8, 0x50);
	default: return FColor(0xe5, 0x48, 0x3b);
	}
}

TArrayView<const FVector2D> FTCCoverPreview::ShieldOutline() { return MakeArrayView(TCCoverImpl::ShieldPts); }
TArrayView<const FVector2D> FTCCoverPreview::ChevronLine() { return MakeArrayView(TCCoverImpl::ChevronPts); }

FString FTCCoverPreview::MenuLabel(bool bOn) { return FString(bOn ? TEXT("Cover preview: On") : TEXT("Cover preview: Off")); }
const TCHAR* FTCCoverPreview::MenuTip() { return TEXT("With infantry selected, mark the cover around the cursor: green heavy, yellow light, red open"); }

// ---------- what the client knows ----------

void FTCCoverPreview::Gather(const FTCMatchState& State, const TSet<int32>& Selected, const FTCCoverSources& Sources, FTCCoverKnown& Out)
{
	Out.Grid.W = State.Map.W;
	Out.Grid.H = State.Map.H;
	Out.Grid.Chars = State.Map.Chars;
	Out.Grid.State = Sources.CellState;
	Out.Explored = State.FogExplored;
	Out.Units.Reset();
	Out.Wrecks.Reset();
	Out.Wrecks.Append(Sources.Wrecks.GetData(), Sources.Wrecks.Num());
	Out.bInfantrySelected = Out.bGarrisonSelected = false;

	const auto Facts = [&Sources](const FString& Type, FTCCoverUnitFacts& F) { return Sources.FactsOf ? Sources.FactsOf(Type, F) : ReferenceUnitFacts(Type, F); };
	// a unit the caller draws, at the place it draws it (the browser's units map)
	const auto Drawn = [&Sources](const FTCUnit& U, FVector2D& At)
	{
		if (Sources.PositionOf) return Sources.PositionOf(U.Id, At);
		At = FVector2D(U.X, U.Z);
		return true;
	};

	TArray<int32> Ids;
	State.Units.GetKeys(Ids);
	Ids.Sort();
	Out.Units.Reserve(Ids.Num());
	for (const int32 Id : Ids)
	{
		const FTCUnit& U = State.Units[Id];
		FTCCoverUnitFacts F;
		FVector2D At;
		if (!Facts(U.Type, F) || !Drawn(U, At)) continue;
		// scanUnit: a known, live, ground unit
		if (F.bAir || U.Hp <= 0) continue;
		FTCCoverUnit& C = Out.Units.AddDefaulted_GetRef();
		C.Id = U.Id;
		C.X = At.X;
		C.Z = At.Y;
		C.bHull = !F.bInfantry;
		C.bShooter = F.bDirectFire && State.IsFoe(U.Owner);
	}
	// scanSel: own infantry in the selection, and whether one of them garrisons
	for (const int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		FTCCoverUnitFacts F;
		FVector2D At;
		if (!U || !State.IsOwn(*U) || !Facts(U->Type, F) || !F.bInfantry || !Drawn(*U, At)) continue;
		Out.bInfantrySelected = true;
		if (F.bGarrisons) Out.bGarrisonSelected = true;
	}
}

// ---------- the preview ----------

bool FTCCoverPreview::Usable(const FTCCoverKnown& Known) const
{
	// a grid to read, the size the match started with (the browser keeps the start's width and height)
	return bStarted && Known.Grid.IsValid() && Known.Grid.W == W && Known.Grid.H == H;
}

void FTCCoverPreview::ScanAt(const FTCCoverKnown& Known, double X, double Z, FScan& Out) const
{
	const double Reach = Radius() + 4 + CellMetres, Reach2 = Reach * Reach;
	double BestD2 = EnemyRange * EnemyRange;
	Out.Enemy = nullptr;
	Out.Hulls.Reset();
	for (const FTCCoverUnit& V : Known.Units)
	{
		const double Dx = V.X - X, Dz = V.Z - Z, D2 = Dx * Dx + Dz * Dz;
		if (V.bHull && D2 < Reach2 && Out.Hulls.Num() < HullCap) Out.Hulls.Add(FVector2D(V.X, V.Z));
		if (D2 < BestD2 && V.bShooter) { BestD2 = D2; Out.Enemy = &V; }
	}
	for (const FTCCoverWreck& Wr : Known.Wrecks)
	{
		if (Out.Hulls.Num() >= HullCap || (Wr.X - X) * (Wr.X - X) + (Wr.Z - Z) * (Wr.Z - Z) >= Reach2) continue;
		Out.Hulls.Add(FVector2D(Wr.X, Wr.Z));
	}
}

void FTCCoverPreview::Fill(const FTCCoverKnown& Known, const FScan& Scan, int32 Cx, int32 Cy, FTCCoverMarks& Into)
{
	const FTCCoverGrid& G = Known.Grid;
	const double R = Radius();
	const int32 Rc = FMath::CeilToInt32(R / CellMetres);
	const bool bMerge = Known.Explored.Num() == W * H && Seen.Num() == W * H;
	FTCCoverMarks Next;
	TCCoverImpl::FWriter Ground{ Next.Ground, GroundCap }, Through{ Next.Through, ThroughCap };
	for (int32 Dy = -Rc; Dy <= Rc; Dy++) for (int32 Dx = -Rc; Dx <= Rc; Dx++)
	{
		const int32 X = Cx + Dx, Y = Cy + Dy;
		const double D = TCCoverImpl::Hypot(Dx, Dy) * CellMetres;
		if (D > R || X < 0 || Y < 0 || X >= W || Y >= H) continue;
		const int32 C = Y * W + X;
		if (bMerge && !Seen[C] && Known.Explored[C]) Seen[C] = 1; // scouting this side has done (mergeFog)
		if (!Seen.IsValidIndex(C) || !Seen[C]) continue;
		const ETCCoverKind Kind = CellCover(G, X, Y, Known.bGarrisonSelected);
		if (Kind == ETCCoverKind::None) continue;
		const double Fade = FMath::Min(1.0, (R - D) / (R * 0.45)), Px = (X + 0.5) * CellMetres, Pz = (Y + 0.5) * CellMetres;
		// houses, hedges, walls, traps, woods, wrecks and rubble: drawn through their models at the cell's ground
		if (Modelled(G.At(X, Y))) { Through.Pip(Kind, Px, Pz, Fade); continue; }
		if (Kind != ETCCoverKind::Open) Ground.Pip(Kind, Px, Pz, Fade);
		else if (Scan.Enemy)
		{
			// the sim aims from the squad to the shooter
			const double A = FMath::Atan2(Scan.Enemy->Z - Pz, Scan.Enemy->X - Px), Ca = FMath::Cos(A), Sa = FMath::Sin(A);
			if (SolidToward(G, Px, Pz, Ca, Sa) || VehicleToward(Scan.Hulls, Px, Pz, Ca, Sa))
			{
				Ground.Pip(ETCCoverKind::Light, Px, Pz, Fade);
				Ground.Chevron(Px, Pz, Ca, Sa, Fade);
			}
			else Ground.Pip(ETCCoverKind::Open, Px, Pz, Fade);
		}
		else
		{
			Ground.Pip(ETCCoverKind::Open, Px, Pz, Fade);
			// the sides with something solid next to them; a corner only when neither of its edges is covered
			for (int32 K = 0; K < 8; K += 2) if (SolidToward(G, Px, Pz, TCCoverImpl::SideC[K], TCCoverImpl::SideS[K])) Ground.Chevron(Px, Pz, TCCoverImpl::SideC[K], TCCoverImpl::SideS[K], Fade);
			for (int32 K = 1; K < 8; K += 2)
			{
				if (SolidToward(G, Px, Pz, TCCoverImpl::SideC[K - 1], TCCoverImpl::SideS[K - 1]) || SolidToward(G, Px, Pz, TCCoverImpl::SideC[(K + 1) & 7], TCCoverImpl::SideS[(K + 1) & 7])) continue;
				if (SolidToward(G, Px, Pz, TCCoverImpl::SideC[K], TCCoverImpl::SideS[K])) Ground.Chevron(Px, Pz, TCCoverImpl::SideC[K], TCCoverImpl::SideS[K], Fade);
			}
			for (int32 I = 0, N = 0; I < Scan.Hulls.Num() && N < 2; I++)
			{
				const double Vx = Scan.Hulls[I].X - Px, Vz = Scan.Hulls[I].Y - Pz, Vd = TCCoverImpl::Hypot(Vx, Vz);
				if (VehicleCovers(Vd)) { Ground.Chevron(Px, Pz, Vx / Vd, Vz / Vd, Fade); N++; }
			}
		}
	}
	TCCoverImpl::Commit(Into, Next);
}

void FTCCoverPreview::Start(const FTCCoverKnown& Known)
{
	bStarted = true;
	W = Known.Grid.IsValid() ? Known.Grid.W : 0;
	H = Known.Grid.IsValid() ? Known.Grid.H : 0;
	Seen.Reset();
	Seen.SetNumZeroed(W * H);
	FlT = FlashSeconds;
	bCurOk = false;
	TCCoverImpl::Clear(Cursor);
	TCCoverImpl::Clear(Flashed);
	// the view stays as the last frame left it (the browser's start leaves its mesh and materials alone); the next
	// Frame sets it
}

const FTCCoverView& FTCCoverPreview::Frame(double Dt, const FTCCoverKnown& Known, const FTCCoverPointer& Pointer)
{
	if (!bStarted || !bEnabled || !Usable(Known))
	{
		LastView = FTCCoverView();
		return LastView;
	}
	if (FlT < FlashSeconds) FlT += Dt;
	const bool bShow = Known.bInfantrySelected && Pointer.bInside && Pointer.bOverView && !Pointer.bLeftDown && Pointer.bAimAllows;
	if (bShow)
	{
		// the cell under the cursor
		bCurOk = Pointer.bGroundHit;
		if (bCurOk) { CurX = FMath::FloorToInt32(Pointer.Ground.X / CellMetres); CurY = FMath::FloorToInt32(Pointer.Ground.Y / CellMetres); }
		if (bCurOk)
		{
			FScan Scan;
			ScanAt(Known, (CurX + 0.5) * CellMetres, (CurY + 0.5) * CellMetres, Scan);
			Fill(Known, Scan, CurX, CurY, Cursor);
		}
	}
	// a flash on the cell under the cursor stands in for the cursor marks, then hands back to them as it fades
	const bool bSame = FlT < FlashSeconds && bShow && bCurOk && CurX == FlX && CurY == FlY;
	double Fo = FlT < FlashSeconds ? FMath::Min(1.0, FlT / 0.12) * (1 - TCCoverImpl::Smooth((FlT - 0.45) / (FlashSeconds - 0.45))) : 0;
	if (bSame && FlT > 0.45 && Fo <= CursorOpacity) { FlT = FlashSeconds; Fo = 0; }
	const bool bCursorOn = bShow && bCurOk && !(bSame && FlT < FlashSeconds);
	FTCCoverView& V = LastView;
	V.CursorGroundOpacity = (float)CursorOpacity;
	V.CursorThroughOpacity = (float)(CursorOpacity * XrayOpacity);
	V.FlashGroundOpacity = (float)Fo;
	V.FlashThroughOpacity = (float)(Fo * XrayOpacity);
	// a hidden or empty layer is left out entirely
	V.bCursorGround = bCursorOn && Cursor.Ground.Num() > 0;
	V.bCursorThrough = bCursorOn && Cursor.Through.Num() > 0;
	V.bFlashGround = Fo > 0 && Flashed.Ground.Num() > 0;
	V.bFlashThrough = Fo > 0 && Flashed.Through.Num() > 0;
	V.bVisible = V.bCursorGround || V.bCursorThrough || V.bFlashGround || V.bFlashThrough;
	if (FParse::Param(FCommandLine::Get(), TEXT("TCCoverLog")))
	{
		static double NextLog = 0;
		const double Now = FPlatformTime::Seconds();
		if (Now >= NextLog)
		{
			NextLog = Now + 2;
			int32 Heavy = 0, Light = 0, Open = 0, Chevrons = 0;
			for (const auto& G : Cursor.Ground) { if (G.bChevron) Chevrons++; else if (G.Kind == ETCCoverKind::Heavy) Heavy++; else if (G.Kind == ETCCoverKind::Light) Light++; else Open++; }
			UE_LOG(LogTC, Log, TEXT("cover markers: visible %d; cursor cell %d,%d; ground %d (heavy %d light %d open %d chevrons %d) depth tested, through %d opacity %.3f; flash ground %d through %d"), V.bVisible, CurX, CurY, Cursor.Ground.Num(), Heavy, Light, Open, Chevrons, Cursor.Through.Num(), V.CursorThroughOpacity, Flashed.Ground.Num(), Flashed.Through.Num());
		}
	}
	return V;
}

void FTCCoverPreview::Flash(double X, double Z, const FTCCoverKnown& Known)
{
	if (!bStarted || !bEnabled || !Usable(Known) || !Known.bInfantrySelected) return;
	FlX = FMath::FloorToInt32(X / CellMetres);
	FlY = FMath::FloorToInt32(Z / CellMetres);
	FScan Scan;
	ScanAt(Known, (FlX + 0.5) * CellMetres, (FlY + 0.5) * CellMetres, Scan);
	Fill(Known, Scan, FlX, FlY, Flashed);
	FlT = 0;
}

void FTCCoverPreview::OnGraphicsChanged(bool bLow)
{
	bLowGraphics = bLow;
	if (!bStarted) return; // the browser rebuilds only once its mesh exists
	TCCoverImpl::Clear(Cursor);
	TCCoverImpl::Clear(Flashed);
	FlT = FlashSeconds;
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCCoverLayerTest, "ThreeCrossroads.Markers.CoverLayers", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCCoverLayerTest::RunTest(const FString&)
{
	TArray<uint8> Cells, Explored;
	Cells.Init('.', 49); Explored.Init(1, 49);
	Cells[3 * 7 + 3] = 'T';
	Cells[3 * 7 + 4] = '+';
	Cells[2 * 7 + 3] = 'B';
	Cells[4 * 7 + 3] = 'H';
	FTCCoverKnown Known;
	Known.Grid.W = Known.Grid.H = 7; Known.Grid.Chars = Cells; Known.Explored = Explored;
	Known.bInfantrySelected = Known.bGarrisonSelected = true;
	FTCCoverPreview Preview; Preview.Start(Known);
	FTCCoverPointer Pointer;
	Pointer.bInside = Pointer.bOverView = Pointer.bGroundHit = true; Pointer.Ground = FVector2D(7, 7);
	const FTCCoverView& View = Preview.Frame(0.05, Known, Pointer);
	bool Heavy = false, Light = false, Chevron = false, HouseThrough = false;
	for (const FTCCoverGlyph& G : Preview.CursorMarks().Ground)
	{
		Heavy |= !G.bChevron && G.Kind == ETCCoverKind::Heavy;
		Light |= !G.bChevron && G.Kind == ETCCoverKind::Light;
		Chevron |= G.bChevron;
	}
	for (const FTCCoverGlyph& G : Preview.CursorMarks().Through) HouseThrough |= G.X == 7 && G.Z == 5 && G.Kind == ETCCoverKind::Heavy;
	TestTrue(TEXT("explored cover draws heavy and light ground shields and directional chevrons"), View.bCursorGround && Heavy && Light && Chevron);
	TestTrue(TEXT("a house shield uses the faint through-model layer"), View.bCursorThrough && HouseThrough && FMath::IsNearlyEqual(View.CursorThroughOpacity, View.CursorGroundOpacity * float(FTCCoverPreview::XrayOpacity)));
	Pointer.bOverView = false;
	TestFalse(TEXT("a pointer over the HUD hides both cursor layers"), Preview.Frame(0.05, Known, Pointer).bVisible);
	return true;
}
#endif
