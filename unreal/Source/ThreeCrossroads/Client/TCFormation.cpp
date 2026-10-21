#include "Client/TCFormation.h"
#include "ThreeCrossroads.h"
#include "WW2Core.h"

namespace
{
	// JavaScript Math.round: halves go towards +infinity.
	double JsRound(double V) { return FMath::FloorToDouble(V + 0.5); }
}

const TCHAR* LexToString(ETCFormationSource Source)
{
	return Source == ETCFormationSource::RustCore ? TEXT("Rust core (ww2-ffi)") : TEXT("interim C++ port");
}

double TCFormation::Slot(bool bInfantry, double Radius, ETCFormationSource* Source)
{
	if (FWW2Core::IsReady())
	{
		double Size = 0;
		const FWW2Result R = FWW2Core::SlotSize(bInfantry, Radius, Size);
		if (R.IsOk())
		{
			if (Source) *Source = ETCFormationSource::RustCore;
			return Size;
		}
		UE_LOG(LogTC, Warning, TEXT("formation: ww2_formation_slot_size failed (%d: %s); the interim C++ port answers"), R.Code, *R.Error);
	}
	if (Source) *Source = ETCFormationSource::InterimPort;
	return SlotSize(bInfantry, Radius);
}

TArray<FTCSpot> TCFormation::Spots(const TArray<FTCFormationItem>& Items, double AtX, double AtZ, double Face, double Reach, ETCFormationSource* Source)
{
	// Said once per process (game thread): where formations come from.
	static bool bSaidCore = false, bSaidNotReady = false;
	if (!FWW2Core::IsReady())
	{
		if (!bSaidNotReady)
		{
			bSaidNotReady = true;
			UE_LOG(LogTC, Warning, TEXT("formation: the interim C++ port answers, the Rust core is %s (%s)"), FWW2Core::StateName(FWW2Core::State()), *FWW2Core::Describe());
		}
	}
	else
	{
		FString Why;
		TArray<FWW2FormationItem> In;
		In.Reserve(Items.Num());
		for (const FTCFormationItem& U : Items)
		{
			if (U.Id < 0) { Why = FString::Printf(TEXT("unit id %d does not fit the interface's uint32"), U.Id); break; }
			In.Add({ (uint32)U.Id, U.bBack, U.X, U.Z, U.Size });
		}
		if (Why.IsEmpty())
		{
			TArray<FWW2FormationSpot> Out;
			const FWW2Result R = FWW2Core::FormationSpots(In, AtX, AtZ, Face, Reach, FWW2Core::ShapeLine, 1.0, Out);
			if (!R.IsOk()) Why = FString::Printf(TEXT("ww2_formation_spots failed (%d: %s)"), R.Code, *R.Error);
			TArray<FTCSpot> Result;
			Result.Reserve(Out.Num());
			for (const FWW2FormationSpot& S : Out)
			{
				// ids come back as they went in, so they fit an int32 again; anything else is not the documented contract
				if (S.Id > (uint32)MAX_int32) { Why = FString::Printf(TEXT("ww2_formation_spots returned id %u, which no item had"), S.Id); break; }
				Result.Add({ (int32)S.Id, S.X, S.Z });
			}
			if (Why.IsEmpty())
			{
				if (!bSaidCore)
				{
					bSaidCore = true;
					UE_LOG(LogTC, Log, TEXT("formation: spots from the Rust core (%s)"), *FWW2Core::Describe());
				}
				if (Source) *Source = ETCFormationSource::RustCore;
				return Result;
			}
		}
		UE_LOG(LogTC, Warning, TEXT("formation: %s; the interim C++ port answers"), *Why);
	}
	if (Source) *Source = ETCFormationSource::InterimPort;
	return FacingSpots(Items, AtX, AtZ, Face, Reach);
}

double TCFormation::SlotSize(bool bInfantry, double Radius)
{
	return FMath::Max(bInfantry ? 3.0 : 5.0, 2.0 * Radius + 1.5);
}

TArray<FTCSpot> TCFormation::FacingSpots(const TArray<FTCFormationItem>& Items, double AtX, double AtZ, double Face, double Reach)
{
	TArray<FTCSpot> Out;
	const int32 N = Items.Num();
	if (!N) return Out;
	const double Spread = 1.0;
	const double Dx = FMath::Cos(Face), Dz = FMath::Sin(Face), Ax = -Dz, Az = Dx;
	auto Side = [Ax, Az](const FTCFormationItem& U) { return U.X * Ax + U.Z * Az; };
	double SizeSum = 0;
	for (const auto& U : Items) SizeSum += U.Size;
	const double Avg = SizeSum / N * Spread;
	const double Far = FMath::IsFinite(Reach) ? Reach : 0; // line is not a fixed shape
	// rankSizes('line', n, fit)
	const int32 Fit = (int32)FMath::FloorToDouble(Far / Avg);
	const int32 Base = FMath::DivideAndRoundUp(N, FMath::DivideAndRoundUp(N, 10));
	const int32 Cols = FMath::Max(Base, FMath::Min(N, Fit));
	TArray<int32> Sizes;
	for (int32 I = 0; I < FMath::DivideAndRoundUp(N, Cols); I++) Sizes.Add(FMath::Min(Cols, N - I * Cols));

	TArray<int32> Order;
	for (int32 I = 0; I < N; I++) Order.Add(I);
	Order.Sort([&](int32 A, int32 B)
	{
		const FTCFormationItem &UA = Items[A], &UB = Items[B];
		const int32 Back = (UA.bBack ? 1 : 0) - (UB.bBack ? 1 : 0);
		if (Back) return Back < 0;
		const double Ahead = JsRound((UB.X - UA.X) * Dx * 100 + (UB.Z - UA.Z) * Dz * 100);
		if (Ahead != 0) return Ahead < 0;
		return A < B;
	});
	double BackDist = 0;
	int32 Start = 0;
	for (int32 K : Sizes)
	{
		TArray<int32> Rank(Order.GetData() + Start, K);
		Start += K;
		Rank.Sort([&](int32 A, int32 B) { const double D = Side(Items[A]) - Side(Items[B]); return D != 0 ? D < 0 : A < B; });
		double Natural = 0, Deepest = 0;
		for (int32 I : Rank) { Natural += Items[I].Size; Deepest = FMath::Max(Deepest, Items[I].Size); }
		const double Tight = Natural * Spread;
		const double Width = Rank.Num() > 1 ? FMath::Max(Tight, FMath::Min(3 * Tight, Far)) : Natural;
		const double Gap = Rank.Num() > 1 ? (Width - Natural) / (Rank.Num() - 1) : 0;
		double Along = -Width / 2;
		for (int32 I : Rank)
		{
			Along += Items[I].Size / 2;
			Out.Add({ Items[I].Id, AtX + Ax * Along - Dx * BackDist, AtZ + Az * Along - Dz * BackDist });
			Along += Items[I].Size / 2 + Gap;
		}
		BackDist += (Deepest + 1.5) * Spread;
	}
	return Out;
}

void TCFormation::SnapToTrench(TArray<FTCSpot>& Spots, const TArray<FTCFormationItem>& Items, TFunctionRef<bool(int32, int32)> IsTrench)
{
	const double Cell = 2.0;
	TSet<int64> Taken;
	for (FTCSpot& S : Spots)
	{
		const FTCFormationItem* Item = Items.FindByPredicate([&S](const FTCFormationItem& I) { return I.Id == S.Id; });
		if (!Item || !Item->bInfantry) continue;
		const int32 CX = (int32)FMath::FloorToDouble(S.X / Cell), CY = (int32)FMath::FloorToDouble(S.Z / Cell);
		double Best = 3;
		int64 BestKey = -1;
		double BX = 0, BZ = 0;
		for (int32 Y = CY - 2; Y <= CY + 2; Y++) for (int32 X = CX - 2; X <= CX + 2; X++)
		{
			const int64 Key = (int64)Y * 4096 + X;
			if (!IsTrench(X, Y) || Taken.Contains(Key)) continue;
			const double PX = (X + 0.5) * Cell, PZ = (Y + 0.5) * Cell, D = FMath::Sqrt(FMath::Square(PX - S.X) + FMath::Square(PZ - S.Z));
			if (D < Best) { Best = D; BestKey = Key; BX = PX; BZ = PZ; }
		}
		if (BestKey >= 0) { Taken.Add(BestKey); S.X = BX; S.Z = BZ; }
	}
}
