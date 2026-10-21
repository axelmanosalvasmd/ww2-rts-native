#include "WW2FormationFixture.h"
#include "Interfaces/IPluginManager.h"
#include "Misc/FileHelper.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"

// The generator below must give the harness's bits: one statement per draw, in its order, and no fused multiply-add
// (Unreal builds with -ffp-contract=off already; this keeps it so if that changes).
#if defined(__clang__)
#pragma clang fp contract(off)
#endif

namespace
{
	constexpr double Pi = 3.141592653589793; // Math.PI, 0x400921fb54442d18
	constexpr double Spreads[] = { 1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5 };

	double FromBits(uint64 B)
	{
		double X;
		FMemory::Memcpy(&X, &B, sizeof X);
		return X;
	}

	bool Hex(const FString& S, uint64& Out)
	{
		if (S.IsEmpty() || S.Len() > 16) return false;
		Out = 0;
		for (TCHAR C : S)
		{
			if (!FChar::IsHexDigit(C)) return false;
			Out = Out << 4 | (uint64)FParse::HexDigit(C);
		}
		return true;
	}

	bool Dec(const FString& S, uint64 Max, uint64& Out)
	{
		if (S.IsEmpty() || S.Len() > 20) return false;
		Out = 0;
		for (TCHAR C : S)
		{
			if (!FChar::IsDigit(C)) return false;
			const uint64 D = (uint64)(C - TEXT('0'));
			if (Out > (Max - D) / 10) return false;
			Out = Out * 10 + D;
		}
		return true;
	}

	int32 ShapeCode(const FString& Name)
	{
		if (Name == TEXT("line")) return FWW2Core::ShapeLine;
		if (Name == TEXT("block")) return FWW2Core::ShapeBlock;
		if (Name == TEXT("column")) return FWW2Core::ShapeColumn;
		if (Name == TEXT("wedge")) return FWW2Core::ShapeWedge;
		return -1;
	}

	double MakeFace(FWW2Mulberry32& Rand)
	{
		const double R = Rand.Next();
		if (R < 0.25) return (Rand.Below(17.0) - 8.0) * (Pi / 2.0);
		if (R < 0.3) return Rand.Next() < 0.5 ? Pi : -Pi;
		const double T = Rand.Next() * 4.0 - 2.0;
		return T * Pi;
	}

	double MakeReach(FWW2Mulberry32& Rand)
	{
		const double R = Rand.Next();
		if (R < 0.25) return 0.0;
		if (R < 0.32) return std::numeric_limits<double>::infinity();
		if (R < 0.36) return std::numeric_limits<double>::quiet_NaN();
		if (R < 0.39) return -std::numeric_limits<double>::infinity();
		if (R < 0.44) return -Rand.Next() * 40.0;
		return Rand.Next() * 160.0;
	}

	double MakeSpread(FWW2Mulberry32& Rand)
	{
		const double R = Rand.Next();
		if (R < 0.75) return Spreads[Rand.Index(UE_ARRAY_COUNT(Spreads))];
		if (R < 0.95) { const double T = Rand.Next() * 2.5; return 0.5 + T; }
		if (R < 0.97) return 0.0;
		if (R < 0.98) return std::numeric_limits<double>::quiet_NaN();
		const double T = Rand.Next();
		return -1.0 - T;
	}

	double MakeSize(FWW2Mulberry32& Rand)
	{
		const double R = Rand.Next();
		if (R < 0.55) return 3.0;
		if (R < 0.95) { const double T = Rand.Next() * 6.0; return 5.0 + T; }
		return Rand.Next() * 3.0;
	}
}

double FWW2Mulberry32::Next()
{
	State += 0x6D2B79F5u;
	uint32 T = State;
	T = (T ^ (T >> 15)) * (T | 1u);
	T ^= T + (T ^ (T >> 7)) * (T | 61u);
	return (double)(T ^ (T >> 14)) / 4294967296.0;
}

double FWW2Mulberry32::Below(double N) { return FMath::FloorToDouble(Next() * N); }

int32 FWW2Mulberry32::Index(int32 N) { return (int32)Below((double)N); }

FString FWW2FormationFixture::InstalledPath()
{
#if PLATFORM_LINUX && PLATFORM_CPU_X86_FAMILY
	const TSharedPtr<IPlugin> Plugin = IPluginManager::Get().FindPlugin(TEXT("WW2Core"));
	if (!Plugin) return FString();
	return FPaths::ConvertRelativePathToFull(Plugin->GetBaseDir() / TEXT("ThirdParty/ww2_ffi/Linux/x86_64-unknown-linux-gnu/fixtures/formation.txt"));
#else
	return FString();
#endif
}

bool FWW2FormationFixture::Load(const FString& Path, FString& Error)
{
	*this = FWW2FormationFixture();
	TArray<FString> Lines;
	if (!FFileHelper::LoadFileToStringArray(Lines, *Path)) { Error = FString::Printf(TEXT("cannot read %s"), *Path); return false; }
	int32 LineNo = 0;
	auto Bad = [&Error, &LineNo, &Path](const TCHAR* What) { Error = FString::Printf(TEXT("%s:%d: %s"), *Path, LineNo, What); return false; };
	for (const FString& Line : Lines)
	{
		LineNo++;
		if (Line.IsEmpty() || Line.StartsWith(TEXT("#"))) continue;
		TArray<FString> F;
		Line.ParseIntoArray(F, TEXT(" "), true);
		const FString& Kind = F[0];
		uint64 A, B, C, D, E, G;
		if (Kind == TEXT("node") || Kind == TEXT("v8") || Kind == TEXT("arch"))
		{
			Runtime += (Runtime.IsEmpty() ? TEXT("") : TEXT(" ")) + Line;
		}
		else if (Kind == TEXT("range"))
		{
			FWW2FixtureRange R;
			uint64 Seed, Count, Fewest, Most;
			if (F.Num() != 7 || !Dec(F[2], MAX_uint32, Seed) || !Dec(F[3], MAX_int32, Count) || !Dec(F[4], MAX_int32, Fewest) || !Dec(F[5], MAX_int32, Most)) return Bad(TEXT("bad range line"));
			R.Label = F[1]; R.Seed = (uint32)Seed; R.Count = (int32)Count; R.Fewest = (int32)Fewest; R.Most = (int32)Most;
			F[6].ParseIntoArray(R.Styles, TEXT(","), true);
			Ranges.Add(MoveTemp(R));
		}
		else if (Kind == TEXT("hashes"))
		{
			if (!Ranges.Num()) return Bad(TEXT("hashes before any range"));
			for (int32 I = 1; I < F.Num(); I++) { if (!Hex(F[I], A)) return Bad(TEXT("bad hash")); Ranges.Last().Hashes.Add(A); }
		}
		else if (Kind == TEXT("case"))
		{
			FWW2FixtureCase W;
			uint64 Items, Spots;
			if (F.Num() != 10 || !Hex(F[3], A) || !Hex(F[4], B) || !Hex(F[5], C) || !Hex(F[6], D) || !Hex(F[7], E) || !Dec(F[8], MAX_int32, Items) || !Dec(F[9], MAX_int32, Spots)) return Bad(TEXT("bad case line"));
			W.Label = F[1]; W.Shape = ShapeCode(F[2]);
			if (W.Shape < 0) return Bad(TEXT("unknown shape"));
			W.Spread = FromBits(A); W.Reach = FromBits(B); W.Face = FromBits(C); W.AtX = FromBits(D); W.AtZ = FromBits(E);
			W.DeclaredItems = (int32)Items; W.DeclaredSpots = (int32)Spots;
			Written.Add(MoveTemp(W));
		}
		else if (Kind == TEXT("item"))
		{
			if (!Written.Num()) return Bad(TEXT("item before any case"));
			if (F.Num() != 6 || !Dec(F[1], MAX_uint32, G) || !Hex(F[2], A) || !Hex(F[3], B) || !Hex(F[4], C) || (F[5] != TEXT("0") && F[5] != TEXT("1"))) return Bad(TEXT("bad item line"));
			Written.Last().Items.Add({ (uint32)G, F[5] == TEXT("1"), FromBits(A), FromBits(B), FromBits(C) });
		}
		else if (Kind == TEXT("spot"))
		{
			if (!Written.Num()) return Bad(TEXT("spot before any case"));
			if (F.Num() != 4 || !Dec(F[1], MAX_uint32, G) || !Hex(F[2], A) || !Hex(F[3], B)) return Bad(TEXT("bad spot line"));
			Written.Last().Spots.Add({ (uint32)G, FromBits(A), FromBits(B) });
		}
		else if (Kind == TEXT("face"))
		{
			if (F.Num() != 3) return Bad(TEXT("bad face line"));
			Faces++;
		}
		else if (Kind == TEXT("slot"))
		{
			if (F.Num() != 4 || (F[1] != TEXT("0") && F[1] != TEXT("1")) || !Hex(F[2], A) || !Hex(F[3], B)) return Bad(TEXT("bad slot line"));
			Slots.Add({ F[1] == TEXT("1"), FromBits(A), FromBits(B) });
		}
		else
		{
			return Bad(TEXT("unknown line kind"));
		}
	}
	for (const FWW2FixtureRange& R : Ranges)
		if (R.Hashes.Num() != R.Count) { Error = FString::Printf(TEXT("%s: range %s has %d hashes for %d cases"), *Path, *R.Label, R.Hashes.Num(), R.Count); return false; }
	for (const FWW2FixtureCase& W : Written)
		if (W.Items.Num() != W.DeclaredItems || W.Spots.Num() != W.DeclaredSpots) { Error = FString::Printf(TEXT("%s: case %s lists %d items and %d spots, declares %d and %d"), *Path, *W.Label, W.Items.Num(), W.Spots.Num(), W.DeclaredItems, W.DeclaredSpots); return false; }
	if (!Ranges.Num() || !Written.Num() || !Slots.Num()) { Error = FString::Printf(TEXT("%s: no ranges, written cases or slot sizes"), *Path); return false; }
	return true;
}

bool FWW2FormationFixture::CanRebuild(const FWW2FixtureRange& Range)
{
	return Range.Styles.Num() == 1 && Range.Styles[0] == TEXT("scatter") && Range.Fewest >= 1 && Range.Most >= Range.Fewest;
}

FWW2FixtureCase FWW2FormationFixture::MakeScatterCase(FWW2Mulberry32& Rand, int32 Fewest, int32 Most)
{
	// harness makeCase, draw by draw (shapes in SHAPES order, which is WW2_SHAPE_* order)
	FWW2FixtureCase Case;
	Case.Shape = Rand.Index(4);
	const int32 N = Fewest + Rand.Index(Most - Fewest + 1);
	Rand.Index(1); // the style, from a list of one
	Case.Face = MakeFace(Rand);
	Case.Reach = MakeReach(Rand);
	Case.Spread = MakeSpread(Rand);
	Case.AtX = Rand.Next() * 1200.0;
	Case.AtZ = Rand.Next() * 1200.0;
	const double Kinds = 1.0 + Rand.Below(3.0);
	double Palette[3];
	Palette[0] = MakeSize(Rand);
	Palette[1] = MakeSize(Rand);
	Palette[2] = MakeSize(Rand);
	static constexpr double BackRates[] = { 0.0, 0.0, 0.1, 0.3 };
	const double BackRate = BackRates[Rand.Index(4)];
	const double CX = Rand.Next() * 1200.0;
	const double CZ = Rand.Next() * 1200.0;
	const double Scatter = 5.0 + Rand.Next() * 60.0;
	Case.Items.Reserve(N);
	for (int32 I = 0; I < N; I++)
	{
		const double SX = Rand.Next() * 2.0 - 1.0;
		const double X = CX + SX * Scatter;
		const double SZ = Rand.Next() * 2.0 - 1.0;
		const double Z = CZ + SZ * Scatter;
		const uint32 Id = (uint32)Rand.Below(4294967296.0);
		const double Size = Palette[(int32)Rand.Below(Kinds)];
		const bool bBack = Rand.Next() < BackRate;
		Case.Items.Add({ Id, bBack, X, Z, Size });
	}
	Case.DeclaredItems = N;
	return Case;
}

uint64 FWW2FormationFixture::Bits(double X)
{
	if (X != X) return 0x7FF8000000000000ull;
	uint64 B;
	FMemory::Memcpy(&B, &X, sizeof B);
	return B;
}

uint64 FWW2FormationFixture::HashSpots(TConstArrayView<FWW2FormationSpot> Spots)
{
	uint64 Hash = 0xCBF29CE484222325ull;
	for (const FWW2FormationSpot& S : Spots)
	{
		const double Values[3] = { (double)S.Id, S.X, S.Z };
		for (double V : Values)
		{
			const uint64 B = Bits(V);
			for (int32 Byte = 0; Byte < 8; Byte++) Hash = (Hash ^ ((B >> (8 * Byte)) & 0xFF)) * 0x00000100000001B3ull;
		}
	}
	return Hash;
}

bool FWW2FormationFixture::SameInputs(const FWW2FixtureCase& A, const FWW2FixtureCase& B)
{
	if (A.Shape != B.Shape || Bits(A.Spread) != Bits(B.Spread) || Bits(A.Reach) != Bits(B.Reach) || Bits(A.Face) != Bits(B.Face) || Bits(A.AtX) != Bits(B.AtX)
		|| Bits(A.AtZ) != Bits(B.AtZ) || A.Items.Num() != B.Items.Num()) return false;
	for (int32 I = 0; I < A.Items.Num(); I++)
	{
		const FWW2FormationItem &U = A.Items[I], &V = B.Items[I];
		if (U.Id != V.Id || U.bBack != V.bBack || Bits(U.X) != Bits(V.X) || Bits(U.Z) != Bits(V.Z) || Bits(U.Size) != Bits(V.Size)) return false;
	}
	return true;
}

bool FWW2FormationFixture::SameSpots(TConstArrayView<FWW2FormationSpot> A, TConstArrayView<FWW2FormationSpot> B)
{
	if (A.Num() != B.Num()) return false;
	for (int32 I = 0; I < A.Num(); I++)
		if (A[I].Id != B[I].Id || Bits(A[I].X) != Bits(B[I].X) || Bits(A[I].Z) != Bits(B[I].Z)) return false;
	return true;
}
