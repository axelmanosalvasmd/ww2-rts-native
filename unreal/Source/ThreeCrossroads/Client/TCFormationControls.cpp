#include "Client/TCFormationControls.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "ThreeCrossroads.h"
#include "WW2Core.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include <cmath>
#include <limits>

// The port below reproduces JavaScript's NaN, infinity and signed-zero behaviour bit for bit, and the build must not
// fuse or reorder its arithmetic. UBT's default (precise) floating point gives -ffp-contract=off on Clang; fast math
// would break both.
#if defined(__FAST_MATH__)
#error "TCFormationControls.cpp needs precise floating point (no -ffast-math): it reproduces JavaScript's formation arithmetic bit for bit"
#endif

// The shapes cross the C interface as their enum values (WW2_SHAPE_*, checked against ww2_core.h in WW2Core.cpp).
static_assert((int32)ETCFormationShape::Line == FWW2Core::ShapeLine && (int32)ETCFormationShape::Block == FWW2Core::ShapeBlock
	&& (int32)ETCFormationShape::Column == FWW2Core::ShapeColumn && (int32)ETCFormationShape::Wedge == FWW2Core::ShapeWedge, "ETCFormationShape must be WW2_SHAPE_*");

// ---------------------------------------------------------------------------------------------------------------------
// The C++ port of shared/formation.js facingSpots and the client helpers it leans on. Plain C++ over TArray and <cmath>
// only, so a scratch harness can extract the text between the markers verbatim and compare it with the pinned
// JavaScript under Node. The client asks the Rust core first (FacingSpots below); this answers when the core cannot.
// TC-PORT-BEGIN
namespace TCFormationPort
{
	constexpr int32 ShapeLine = 0, ShapeBlock = 1, ShapeColumn = 2, ShapeWedge = 3;

	inline double NaN() { return std::numeric_limits<double>::quiet_NaN(); }

	// Math.max(A, B) and Math.min(A, B): NaN wins, and +0 is above -0.
	inline double JsMax(double A, double B)
	{
		if (std::isnan(A) || std::isnan(B)) return NaN();
		if (A > B) return A;
		if (B > A) return B;
		return std::signbit(A) ? B : A;
	}
	inline double JsMin(double A, double B)
	{
		if (std::isnan(A) || std::isnan(B)) return NaN();
		if (A < B) return A;
		if (B < A) return B;
		return std::signbit(A) ? A : B;
	}

	// Math.round: the nearest integer, halves towards +infinity, -0 for [-0.5, -0]. Exact (no V + 0.5 rounding).
	inline double JsRound(double V)
	{
		if (!std::isfinite(V)) return V;
		const double F = std::floor(V);
		const double R = V - F >= 0.5 ? F + 1.0 : F;
		return R == 0.0 && std::signbit(V) ? -0.0 : R;
	}

	// Math.hypot(A, B) as V8 computes it (builtins/math.tq MathHypot): scaled by the larger magnitude, Kahan summed.
	inline double JsHypot(double A, double B)
	{
		const double Abs[2] = { std::fabs(A), std::fabs(B) };
		bool bNaN = false;
		double Max = 0;
		for (int32 I = 0; I < 2; I++)
		{
			if (std::isnan(Abs[I])) bNaN = true;
			else if (Abs[I] > Max) Max = Abs[I];
		}
		if (Max == std::numeric_limits<double>::infinity()) return Max;
		if (bNaN) return NaN();
		if (Max == 0) return 0;
		double Sum = 0, Compensation = 0;
		for (int32 I = 0; I < 2; I++)
		{
			const double N = Abs[I] / Max;
			const double Summand = N * N - Compensation;
			const double Preliminary = Sum + Summand;
			Compensation = (Preliminary - Sum) - Summand;
			Sum = Preliminary;
		}
		return std::sqrt(Sum) * Max;
	}

	// V8's Array.prototype.sort (third_party/v8/builtins/array-sort.tq: TimSort with binary insertion for short runs and
	// galloping merges) over an array of element handles. Compare(A, B) is the JavaScript comparator's number; NaN counts
	// as 0. The formation comparators round, so they are not consistent orders, and only the same algorithm gives the
	// same arrangement as the browser.
	template <typename CompareT>
	class TJsSort
	{
	public:
		TJsSort(TArray<int32>& InWork, const CompareT& InCompare) : Work(InWork), Comparator(InCompare) {}

		void Run()
		{
			const int32 Length = Work.Num();
			if (Length < 2) return;
			int32 Low = 0, Remaining = Length;
			const int32 MinRun = MinRunLength(Remaining);
			while (Remaining != 0)
			{
				int32 RunLength = CountAndMakeRun(Low, Low + Remaining);
				if (RunLength < MinRun)
				{
					const int32 Forced = Remaining < MinRun ? Remaining : MinRun;
					BinaryInsertionSort(Low, Low + RunLength, Low + Forced);
					RunLength = Forced;
				}
				PushRun(Low, RunLength);
				MergeCollapse();
				Low += RunLength;
				Remaining -= RunLength;
			}
			MergeForceCollapse();
		}

	private:
		static constexpr int32 MinGallopWins = 7;
		static constexpr int32 MaxPending = 85;
		enum class EEnd { Succeed, CopyRest };

		TArray<int32>& Work;
		const CompareT& Comparator;
		TArray<int32> Temp;
		int32 RunBase[MaxPending] = {};
		int32 RunLen[MaxPending] = {};
		int32 Pending = 0;
		int32 MinGallop = MinGallopWins;

		double Order(int32 A, int32 B) const
		{
			const double R = Comparator(A, B);
			return std::isnan(R) ? 0.0 : R;
		}

		static int32 MinRunLength(int32 N)
		{
			int32 R = 0;
			while (N >= 64) { R |= N & 1; N >>= 1; }
			return N + R;
		}

		// memmove semantics: the merges copy within Work in both directions
		static void CopyRange(const TArray<int32>& Src, int32 SrcPos, TArray<int32>& Dst, int32 DstPos, int32 N)
		{
			if (&Src == &Dst && SrcPos < DstPos) for (int32 I = N - 1; I >= 0; I--) Dst[DstPos + I] = Src[SrcPos + I];
			else for (int32 I = 0; I < N; I++) Dst[DstPos + I] = Src[SrcPos + I];
		}

		int32 CountAndMakeRun(int32 LowArg, int32 High)
		{
			const int32 Low = LowArg + 1;
			if (Low == High) return 1;
			int32 RunLength = 2;
			const int32 ElementLow = Work[Low];
			double O = Order(ElementLow, Work[Low - 1]);
			const bool bDescending = O < 0;
			int32 Previous = ElementLow;
			for (int32 Idx = Low + 1; Idx < High; ++Idx)
			{
				const int32 Current = Work[Idx];
				O = Order(Current, Previous);
				if (bDescending) { if (O >= 0) break; }
				else if (O < 0) break;
				Previous = Current;
				++RunLength;
			}
			if (bDescending)
				for (int32 A = LowArg, B = LowArg + RunLength - 1; A < B; ++A, --B) { const int32 T = Work[A]; Work[A] = Work[B]; Work[B] = T; }
			return RunLength;
		}

		void BinaryInsertionSort(int32 Low, int32 StartArg, int32 High)
		{
			for (int32 Start = Low == StartArg ? StartArg + 1 : StartArg; Start < High; ++Start)
			{
				int32 Left = Low, Right = Start;
				const int32 Pivot = Work[Start];
				while (Left < Right)
				{
					const int32 Mid = Left + ((Right - Left) >> 1);
					if (Order(Pivot, Work[Mid]) < 0) Right = Mid;
					else Left = Mid + 1;
				}
				for (int32 P = Start; P > Left; --P) Work[P] = Work[P - 1];
				Work[Left] = Pivot;
			}
		}

		void PushRun(int32 Base, int32 Len)
		{
			RunBase[Pending] = Base;
			RunLen[Pending] = Len;
			++Pending;
		}

		bool Established(int32 N) const { return N < 2 || RunLen[N - 2] > RunLen[N - 1] + RunLen[N]; }

		void MergeCollapse()
		{
			while (Pending > 1)
			{
				int32 N = Pending - 2;
				if (!Established(N + 1) || !Established(N))
				{
					if (RunLen[N - 1] < RunLen[N + 1]) --N;
					MergeAt(N);
				}
				else if (RunLen[N] <= RunLen[N + 1]) MergeAt(N);
				else break;
			}
		}

		void MergeForceCollapse()
		{
			while (Pending > 1)
			{
				int32 N = Pending - 2;
				if (N > 0 && RunLen[N - 1] < RunLen[N + 1]) --N;
				MergeAt(N);
			}
		}

		static int32 Grow(int32 Offset, int32 MaxOfs)
		{
			const int64 Next = (int64)Offset * 2 + 1; // V8 clamps an overflow to MaxOfs, which ends the gallop the same way
			return Next > MaxOfs ? MaxOfs : (int32)Next;
		}

		int32 GallopLeft(const TArray<int32>& Arr, int32 Key, int32 Base, int32 Length, int32 Hint) const
		{
			int32 LastOfs = 0, Offset = 1;
			if (Order(Arr[Base + Hint], Key) < 0)
			{
				const int32 MaxOfs = Length - Hint;
				while (Offset < MaxOfs)
				{
					if (Order(Arr[Base + Hint + Offset], Key) >= 0) break;
					LastOfs = Offset;
					Offset = Grow(Offset, MaxOfs);
				}
				if (Offset > MaxOfs) Offset = MaxOfs;
				LastOfs += Hint;
				Offset += Hint;
			}
			else
			{
				const int32 MaxOfs = Hint + 1;
				while (Offset < MaxOfs)
				{
					if (Order(Arr[Base + Hint - Offset], Key) < 0) break;
					LastOfs = Offset;
					Offset = Grow(Offset, MaxOfs);
				}
				if (Offset > MaxOfs) Offset = MaxOfs;
				const int32 Tmp = LastOfs;
				LastOfs = Hint - Offset;
				Offset = Hint - Tmp;
			}
			LastOfs++;
			while (LastOfs < Offset)
			{
				const int32 M = LastOfs + ((Offset - LastOfs) >> 1);
				if (Order(Arr[Base + M], Key) < 0) LastOfs = M + 1;
				else Offset = M;
			}
			return Offset;
		}

		int32 GallopRight(const TArray<int32>& Arr, int32 Key, int32 Base, int32 Length, int32 Hint) const
		{
			int32 LastOfs = 0, Offset = 1;
			if (Order(Key, Arr[Base + Hint]) < 0)
			{
				const int32 MaxOfs = Hint + 1;
				while (Offset < MaxOfs)
				{
					if (Order(Key, Arr[Base + Hint - Offset]) >= 0) break;
					LastOfs = Offset;
					Offset = Grow(Offset, MaxOfs);
				}
				if (Offset > MaxOfs) Offset = MaxOfs;
				const int32 Tmp = LastOfs;
				LastOfs = Hint - Offset;
				Offset = Hint - Tmp;
			}
			else
			{
				const int32 MaxOfs = Length - Hint;
				while (Offset < MaxOfs)
				{
					if (Order(Key, Arr[Base + Hint + Offset]) < 0) break;
					LastOfs = Offset;
					Offset = Grow(Offset, MaxOfs);
				}
				if (Offset > MaxOfs) Offset = MaxOfs;
				LastOfs += Hint;
				Offset += Hint;
			}
			LastOfs++;
			while (LastOfs < Offset)
			{
				const int32 M = LastOfs + ((Offset - LastOfs) >> 1);
				if (Order(Key, Arr[Base + M]) < 0) Offset = M;
				else LastOfs = M + 1;
			}
			return Offset;
		}

		void MergeAt(int32 I)
		{
			int32 BaseA = RunBase[I], LengthA = RunLen[I];
			const int32 BaseB = RunBase[I + 1];
			int32 LengthB = RunLen[I + 1];
			RunLen[I] = LengthA + LengthB;
			if (I == Pending - 3)
			{
				RunBase[I + 1] = RunBase[I + 2];
				RunLen[I + 1] = RunLen[I + 2];
			}
			--Pending;
			const int32 K = GallopRight(Work, Work[BaseB], BaseA, LengthA, 0);
			BaseA += K;
			LengthA -= K;
			if (LengthA == 0) return;
			LengthB = GallopLeft(Work, Work[BaseA + LengthA - 1], BaseB, LengthB, LengthB - 1);
			if (LengthB == 0) return;
			if (LengthA <= LengthB) MergeLow(BaseA, LengthA, BaseB, LengthB);
			else MergeHigh(BaseA, LengthA, BaseB, LengthB);
		}

		void MergeLow(int32 BaseA, int32 LengthAArg, int32 BaseB, int32 LengthBArg)
		{
			int32 LengthA = LengthAArg, LengthB = LengthBArg;
			Temp.Reset();
			for (int32 I = 0; I < LengthA; I++) Temp.Add(Work[BaseA + I]);
			int32 Dest = BaseA, CursorTemp = 0, CursorB = BaseB;
			Work[Dest++] = Work[CursorB++];
			const EEnd End = [&]() -> EEnd
			{
				if (--LengthB == 0) return EEnd::Succeed;
				if (LengthA == 1) return EEnd::CopyRest;
				int32 Gallop = MinGallop;
				for (;;)
				{
					int32 WinsA = 0, WinsB = 0;
					for (;;)
					{
						if (Order(Work[CursorB], Temp[CursorTemp]) < 0)
						{
							Work[Dest++] = Work[CursorB++];
							++WinsB; --LengthB; WinsA = 0;
							if (LengthB == 0) return EEnd::Succeed;
							if (WinsB >= Gallop) break;
						}
						else
						{
							Work[Dest++] = Temp[CursorTemp++];
							++WinsA; --LengthA; WinsB = 0;
							if (LengthA == 1) return EEnd::CopyRest;
							if (WinsA >= Gallop) break;
						}
					}
					++Gallop;
					bool bFirst = true;
					while (WinsA >= MinGallopWins || WinsB >= MinGallopWins || bFirst)
					{
						bFirst = false;
						Gallop = Gallop - 1 > 1 ? Gallop - 1 : 1;
						MinGallop = Gallop;
						WinsA = GallopRight(Temp, Work[CursorB], CursorTemp, LengthA, 0);
						if (WinsA > 0)
						{
							CopyRange(Temp, CursorTemp, Work, Dest, WinsA);
							Dest += WinsA; CursorTemp += WinsA; LengthA -= WinsA;
							if (LengthA == 1) return EEnd::CopyRest;
							if (LengthA == 0) return EEnd::Succeed;
						}
						Work[Dest++] = Work[CursorB++];
						if (--LengthB == 0) return EEnd::Succeed;
						WinsB = GallopLeft(Work, Temp[CursorTemp], CursorB, LengthB, 0);
						if (WinsB > 0)
						{
							CopyRange(Work, CursorB, Work, Dest, WinsB);
							Dest += WinsB; CursorB += WinsB; LengthB -= WinsB;
							if (LengthB == 0) return EEnd::Succeed;
						}
						Work[Dest++] = Temp[CursorTemp++];
						if (--LengthA == 1) return EEnd::CopyRest;
					}
					++Gallop;
					MinGallop = Gallop;
				}
			}();
			if (End == EEnd::Succeed)
			{
				if (LengthA > 0) CopyRange(Temp, CursorTemp, Work, Dest, LengthA);
			}
			else
			{
				// the last element of run A belongs at the end of the merge
				CopyRange(Work, CursorB, Work, Dest, LengthB);
				Work[Dest + LengthB] = Temp[CursorTemp];
			}
		}

		void MergeHigh(int32 BaseA, int32 LengthAArg, int32 BaseB, int32 LengthBArg)
		{
			int32 LengthA = LengthAArg, LengthB = LengthBArg;
			Temp.Reset();
			for (int32 I = 0; I < LengthB; I++) Temp.Add(Work[BaseB + I]);
			int32 Dest = BaseB + LengthB - 1, CursorTemp = LengthB - 1, CursorA = BaseA + LengthA - 1;
			Work[Dest--] = Work[CursorA--];
			const EEnd End = [&]() -> EEnd
			{
				if (--LengthA == 0) return EEnd::Succeed;
				if (LengthB == 1) return EEnd::CopyRest;
				int32 Gallop = MinGallop;
				for (;;)
				{
					int32 WinsA = 0, WinsB = 0;
					for (;;)
					{
						if (Order(Temp[CursorTemp], Work[CursorA]) < 0)
						{
							Work[Dest--] = Work[CursorA--];
							++WinsA; --LengthA; WinsB = 0;
							if (LengthA == 0) return EEnd::Succeed;
							if (WinsA >= Gallop) break;
						}
						else
						{
							Work[Dest--] = Temp[CursorTemp--];
							++WinsB; --LengthB; WinsA = 0;
							if (LengthB == 1) return EEnd::CopyRest;
							if (WinsB >= Gallop) break;
						}
					}
					++Gallop;
					bool bFirst = true;
					while (WinsA >= MinGallopWins || WinsB >= MinGallopWins || bFirst)
					{
						bFirst = false;
						Gallop = Gallop - 1 > 1 ? Gallop - 1 : 1;
						MinGallop = Gallop;
						int32 K = GallopRight(Work, Temp[CursorTemp], BaseA, LengthA, LengthA - 1);
						WinsA = LengthA - K;
						if (WinsA > 0)
						{
							Dest -= WinsA; CursorA -= WinsA;
							CopyRange(Work, CursorA + 1, Work, Dest + 1, WinsA);
							LengthA -= WinsA;
							if (LengthA == 0) return EEnd::Succeed;
						}
						Work[Dest--] = Temp[CursorTemp--];
						if (--LengthB == 1) return EEnd::CopyRest;
						K = GallopLeft(Temp, Work[CursorA], 0, LengthB, LengthB - 1);
						WinsB = LengthB - K;
						if (WinsB > 0)
						{
							Dest -= WinsB; CursorTemp -= WinsB;
							CopyRange(Temp, CursorTemp + 1, Work, Dest + 1, WinsB);
							LengthB -= WinsB;
							if (LengthB == 1) return EEnd::CopyRest;
							if (LengthB == 0) return EEnd::Succeed;
						}
						Work[Dest--] = Work[CursorA--];
						if (--LengthA == 0) return EEnd::Succeed;
					}
					++Gallop;
					MinGallop = Gallop;
				}
			}();
			if (End == EEnd::Succeed)
			{
				if (LengthB > 0) CopyRange(Temp, 0, Work, Dest - (LengthB - 1), LengthB);
			}
			else
			{
				// the first element of run B belongs at the front of the merge
				Dest -= LengthA; CursorA -= LengthA;
				CopyRange(Work, CursorA + 1, Work, Dest + 1, LengthA);
				Work[Dest] = Temp[CursorTemp];
			}
		}
	};

	template <typename CompareT>
	void JsSort(TArray<int32>& Work, const CompareT& Compare)
	{
		TJsSort<CompareT>(Work, Compare).Run();
	}

	// shared/formation.js facingSpots(items, at, face, reach, { shape, spread }) with Dx = cos(face), Dz = sin(face)
	// computed by the caller. Spots come rank by rank from the front, as the JavaScript returns them.
	inline TArray<FTCSpot> FacingSpots(const TArray<FTCFormationItem>& Items, double AtX, double AtZ, double Dx, double Dz, double Reach, int32 Shape, double Spread)
	{
		TArray<FTCSpot> Out;
		const int32 N = Items.Num();
		if (N == 0) return Out;
		const double Ax = -Dz, Az = Dx;
		double SizeSum = 0;
		for (int32 I = 0; I < N; I++) SizeSum += Items[I].Size;
		const double Avg = SizeSum / N * Spread;
		const bool bFixed = Shape == ShapeColumn || Shape == ShapeWedge;
		const double Far = std::isfinite(Reach) && !bFixed ? Reach : 0.0;

		// rankSizes(shape, n, Math.floor(far / avg)), in doubles so NaN and Infinity behave as in JavaScript
		TArray<int32> Sizes;
		if (Shape == ShapeWedge)
		{
			for (int32 K = 1, Left = N; Left > 0; Left -= K++) Sizes.Add(K < Left ? K : Left);
		}
		else
		{
			const double Nd = N;
			const double Fit = std::floor(Far / Avg);
			const double Base = Shape == ShapeColumn ? JsMin(Nd, 2.0) : Shape == ShapeBlock ? std::ceil(std::sqrt(Nd)) : std::ceil(Nd / std::ceil(Nd / 10.0));
			const double Cols = Shape == ShapeColumn ? Base : JsMax(Base, JsMin(Nd, Fit));
			const double Ranks = std::ceil(Nd / Cols); // Array.from({ length }): NaN gives no ranks (and no spots)
			if (Ranks >= 1 && Ranks <= Nd)
				for (int32 I = 0; I < (int32)Ranks; I++) Sizes.Add((int32)JsMin(Cols, Nd - I * Cols));
		}

		// back ranks last, then those furthest forward (to the centimetre), then the selection's order
		TArray<int32> Order;
		for (int32 I = 0; I < N; I++) Order.Add(I);
		JsSort(Order, [&Items, Dx, Dz](int32 A, int32 B) -> double
		{
			const double Back = (double)((Items[A].bBack ? 1 : 0) - (Items[B].bBack ? 1 : 0));
			if (Back != 0) return Back;
			const double Ahead = JsRound((Items[B].X - Items[A].X) * Dx * 100 + (Items[B].Z - Items[A].Z) * Dz * 100);
			if (Ahead != 0 && !std::isnan(Ahead)) return Ahead;
			return (double)(A - B);
		});

		double BackDist = 0;
		int32 Start = 0;
		for (int32 R = 0; R < Sizes.Num(); R++)
		{
			const int32 K = Sizes[R];
			TArray<int32> Rank;
			for (int32 J = 0; J < K && Start + J < N; J++) Rank.Add(Order[Start + J]);
			Start += K;
			// each rank keeps its units' left to right order
			JsSort(Rank, [&Items, Ax, Az](int32 A, int32 B) -> double
			{
				const double D = (Items[A].X * Ax + Items[A].Z * Az) - (Items[B].X * Ax + Items[B].Z * Az);
				if (D != 0 && !std::isnan(D)) return D;
				return (double)(A - B);
			});
			double Natural = 0;
			double Deepest = -std::numeric_limits<double>::infinity();
			for (int32 J = 0; J < Rank.Num(); J++)
			{
				Natural += Items[Rank[J]].Size;
				Deepest = JsMax(Deepest, Items[Rank[J]].Size);
			}
			const double Tight = Natural * Spread;
			const double Width = Rank.Num() > 1 ? JsMax(Tight, JsMin(3 * Tight, Far)) : Natural;
			const double Gap = Rank.Num() > 1 ? (Width - Natural) / (Rank.Num() - 1) : 0.0;
			double Along = -Width / 2;
			for (int32 J = 0; J < Rank.Num(); J++)
			{
				const FTCFormationItem& U = Items[Rank[J]];
				Along += U.Size / 2;
				FTCSpot S;
				S.Id = U.Id;
				S.X = AtX + Ax * Along - Dx * BackDist;
				S.Z = AtZ + Az * Along - Dz * BackDist;
				Out.Add(S);
				Along += U.Size / 2 + Gap;
			}
			// a later rank leaves the deepest slot above it clear, with another 1.5 m between ranks
			BackDist += (Deepest + 1.5) * Spread;
		}
		return Out;
	}

	// client/main.js snapToTrench: an infantry spot within 3 m of a trench cell (2 m cells) nobody else was given steps
	// into its centre. IsInfantry(Id) and IsTrench(CellX, CellY) read the client's units and terrain.
	template <typename IsInfantryT, typename IsTrenchT>
	void SnapToTrench(TArray<FTCSpot>& Spots, const IsInfantryT& IsInfantry, const IsTrenchT& IsTrench)
	{
		const double Cell = 2.0;
		TArray<int64> Taken;
		for (int32 I = 0; I < Spots.Num(); I++)
		{
			FTCSpot& S = Spots[I];
			if (!IsInfantry(S.Id)) continue;
			const double FX = std::floor(S.X / Cell), FY = std::floor(S.Z / Cell);
			// NaN or far off any map: the browser's loops find no trench cell there either
			if (!(std::fabs(FX) < 1e9) || !(std::fabs(FY) < 1e9)) continue;
			const int32 CX = (int32)FX, CY = (int32)FY;
			double Best = 3;
			bool bFound = false;
			int64 BestKey = 0;
			double BX = 0, BZ = 0;
			for (int32 Y = CY - 2; Y <= CY + 2; Y++)
				for (int32 X = CX - 2; X <= CX + 2; X++)
				{
					const int64 Key = (int64)Y * 4096 + X;
					bool bTaken = false;
					for (int32 T = 0; T < Taken.Num() && !bTaken; T++) bTaken = Taken[T] == Key;
					if (!IsTrench(X, Y) || bTaken) continue;
					const double PX = (X + 0.5) * Cell, PZ = (Y + 0.5) * Cell, D = JsHypot(PX - S.X, PZ - S.Z);
					if (D < Best) { Best = D; bFound = true; BestKey = Key; BX = PX; BZ = PZ; }
				}
			if (bFound) { Taken.Add(BestKey); S.X = BX; S.Z = BZ; }
		}
	}

	// client/main.js updateFacing for one pointer position: the press pixel and ground, the pointer now and the ground
	// under it. bClear: drop the facing; bSet: take Face and Reach; neither: keep what the gesture had.
	struct FFacingStep
	{
		bool bClear = false, bSet = false;
		double Face = 0, Reach = 0;
	};
	inline FFacingStep UpdateFacing(double PressX, double PressY, double FromX, double FromZ, double MouseX, double MouseY, bool bHasGround, double GroundX, double GroundZ,
		double Pixels, double Metres)
	{
		FFacingStep Step;
		const bool bNear = JsHypot(MouseX - PressX, MouseY - PressY) < Pixels;
		const bool bG = !bNear && bHasGround;
		const double Dx = bG ? GroundX - FromX : 0.0, Dz = bG ? GroundZ - FromZ : 0.0, Reach = JsHypot(Dx, Dz);
		if (!bG || Reach < Metres)
		{
			Step.bClear = bG || bNear;
			return Step;
		}
		Step.bSet = true;
		Step.Face = std::atan2(Dz, Dx);
		Step.Reach = Reach;
		return Step;
	}

	// client/formation-preview.js show: the order the slots are joined in (front rank first, then left to right), and
	// which neighbours share a rank (along the facing within 0.1 m). Returns pairs of spot indices to join.
	inline TArray<int32> RankNeighbours(const TArray<FTCSpot>& Spots, double Fx, double Fz)
	{
		const double Ax = -Fz, Az = Fx;
		TArray<int32> Sorted;
		for (int32 I = 0; I < Spots.Num(); I++) Sorted.Add(I);
		JsSort(Sorted, [&Spots, Fx, Fz, Ax, Az](int32 A, int32 B) -> double
		{
			const FTCSpot &P = Spots[A], &Q = Spots[B];
			const double Rank = JsRound((Q.X * Fx + Q.Z * Fz) * 100) - JsRound((P.X * Fx + P.Z * Fz) * 100);
			if (Rank != 0 && !std::isnan(Rank)) return Rank;
			return (P.X - Q.X) * Ax + (P.Z - Q.Z) * Az;
		});
		TArray<int32> Pairs;
		for (int32 I = 1; I < Sorted.Num(); I++)
		{
			const FTCSpot &P = Spots[Sorted[I - 1]], &Q = Spots[Sorted[I]];
			if (std::fabs((P.X - Q.X) * Fx + (P.Z - Q.Z) * Fz) < 0.1) { Pairs.Add(Sorted[I - 1]); Pairs.Add(Sorted[I]); }
		}
		return Pairs;
	}
}
// TC-PORT-END
// ---------------------------------------------------------------------------------------------------------------------

namespace TCFormationControlsLocal
{
	// file-local helpers, named so a unity build cannot merge them with another file's
	const FColor MoveColor(0x5f, 0xa4, 0xf2);       // client/markers.js MOVE_COLOR (PLAN_COLORS[1])
	const FColor AttackMoveColor(0xff, 0x9a, 0x40); // client/orders.js: an attack-move's marker
	const FColor PlanAttackColor(0xf0, 0x93, 0x2f); // PLAN_COLORS[2]: the preview of an attack-move drag

	const TCHAR* const ShapeTips[] = {
		TEXT("ranks of up to ten across the facing; a longer right-drag fits more side by side"),
		TEXT("a square, widened by a longer right-drag"),
		TEXT("two files deep, for roads and gaps"),
		TEXT("an arrowhead, one unit at the tip"),
	};
	const TCHAR* const ShapeNames[] = { TEXT("Line"), TEXT("Block"), TEXT("Column"), TEXT("Wedge") };

	// A quarter-step spread as JavaScript prints it (1, 1.25, 1.5, ...). Spread only ever holds quarter steps.
	FString SpreadText(double V)
	{
		const int64 Hundredths = (int64)TCFormationPort::JsRound(V * 100);
		const int64 Whole = Hundredths / 100, Frac = Hundredths % 100;
		if (Frac == 0) return FString::Printf(TEXT("%lld"), (long long)Whole);
		if (Frac % 10 == 0) return FString::Printf(TEXT("%lld.%lld"), (long long)Whole, (long long)(Frac / 10));
		return FString::Printf(TEXT("%lld.%02lld"), (long long)Whole, (long long)Frac);
	}

	// A wire number: JSON.stringify writes NaN and the infinities as null.
	TSharedPtr<FJsonValue> WireNumber(double V)
	{
		if (std::isfinite(V)) return MakeShared<FJsonValueNumber>(V);
		return MakeShared<FJsonValueNull>();
	}

	TSharedPtr<FJsonValue> WireRow(int32 Id, double X, double Z)
	{
		TArray<TSharedPtr<FJsonValue>> Row;
		Row.Add(MakeShared<FJsonValueNumber>(Id));
		Row.Add(WireNumber(X));
		Row.Add(WireNumber(Z));
		return MakeShared<FJsonValueArray>(Row);
	}

	FVector2D Centroid(const TArray<FTCFormationTroop>& Troops)
	{
		double X = 0, Z = 0;
		for (const FTCFormationTroop& T : Troops) X += T.X;
		for (const FTCFormationTroop& T : Troops) Z += T.Z;
		return FVector2D(X / Troops.Num(), Z / Troops.Num());
	}

	TArray<FTCFormationItem> ItemsOf(const TArray<FTCFormationTroop>& Troops)
	{
		TArray<FTCFormationItem> Items;
		Items.Reserve(Troops.Num());
		for (const FTCFormationTroop& T : Troops)
		{
			FTCFormationItem I;
			I.Id = T.Id;
			I.X = T.X;
			I.Z = T.Z;
			I.Size = T.Size;
			I.bBack = T.bBack;
			I.bInfantry = T.bInfantry;
			Items.Add(I);
		}
		return Items;
	}

	// One warning per distinct reason, so a failing bridge does not flood the log from the per-frame preview.
	void WarnOnce(const FString& Why)
	{
		static FString Last;
		if (Why == Last) return;
		Last = Why;
		UE_LOG(LogTC, Warning, TEXT("formation controls: %s; the C++ port answers"), *Why);
	}
}

const TCHAR* LexToString(ETCFormationShape Shape)
{
	switch (Shape)
	{
	case ETCFormationShape::Block: return TEXT("block");
	case ETCFormationShape::Column: return TEXT("column");
	case ETCFormationShape::Wedge: return TEXT("wedge");
	default: return TEXT("line");
	}
}

const TCHAR* FTCFormationControls::Key(ETCFormationControl Control)
{
	switch (Control)
	{
	case ETCFormationControl::ShapeLine: return TEXT("shape:line");
	case ETCFormationControl::ShapeBlock: return TEXT("shape:block");
	case ETCFormationControl::ShapeColumn: return TEXT("shape:column");
	case ETCFormationControl::ShapeWedge: return TEXT("shape:wedge");
	case ETCFormationControl::Tighten: return TEXT("tighten");
	case ETCFormationControl::Spread: return TEXT("spread");
	case ETCFormationControl::Together: return TEXT("together");
	case ETCFormationControl::Snap: return TEXT("snap");
	case ETCFormationControl::Menu: return TEXT("menu");
	case ETCFormationControl::Cycle: return TEXT("cycle");
	default: return TEXT("");
	}
}

ETCFormationControl FTCFormationControls::ParseKey(const FString& InKey)
{
	static const ETCFormationControl All[] = { ETCFormationControl::ShapeLine, ETCFormationControl::ShapeBlock, ETCFormationControl::ShapeColumn, ETCFormationControl::ShapeWedge,
		ETCFormationControl::Tighten, ETCFormationControl::Spread, ETCFormationControl::Together, ETCFormationControl::Snap, ETCFormationControl::Menu, ETCFormationControl::Cycle };
	for (ETCFormationControl C : All)
		if (InKey.Equals(Key(C), ESearchCase::CaseSensitive)) return C;
	return ETCFormationControl::None;
}

ETCFormationControl FTCFormationControls::ShapeControl(ETCFormationShape Shape)
{
	switch (Shape)
	{
	case ETCFormationShape::Block: return ETCFormationControl::ShapeBlock;
	case ETCFormationShape::Column: return ETCFormationControl::ShapeColumn;
	case ETCFormationShape::Wedge: return ETCFormationControl::ShapeWedge;
	default: return ETCFormationControl::ShapeLine;
	}
}

TArray<FTCFormationTroop> FTCFormationControls::Troops(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, ETCTroopFilter Filter)
{
	TArray<FTCFormationTroop> Out;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		const FTCUnitDef* D = U ? Table.Find(U->Type) : nullptr;
		if (!U || !D || !State.IsOwn(*U) || D->bStructure) continue;
		if (Filter == ETCTroopFilter::Mine && D->bAir) continue;
		FTCFormationTroop T;
		T.Id = Id;
		T.X = U->X;
		T.Z = U->Z;
		T.Size = TCFormation::Slot(D->bInfantry, D->Radius);
		T.Radius = D->Radius;
		T.bBack = D->bBack;
		T.bInfantry = D->bInfantry;
		T.bAir = D->bAir;
		T.bTruck = U->Type == TEXT("truck");
		Out.Add(T);
	}
	return Out;
}

TArray<FTCSpot> FTCFormationControls::PortSpots(const TArray<FTCFormationItem>& Items, double AtX, double AtZ, double Dx, double Dz, double Reach,
	ETCFormationShape Shape, double Spread)
{
	return TCFormationPort::FacingSpots(Items, AtX, AtZ, Dx, Dz, Reach, (int32)Shape, Spread);
}

TArray<FTCSpot> FTCFormationControls::FacingSpots(const TArray<FTCFormationItem>& Items, double AtX, double AtZ, double Face, double Reach,
	ETCFormationShape Shape, double Spread, ETCFormationSource* Source)
{
	static bool bSaidCore = false, bSaidNotReady = false;
	if (!FWW2Core::IsReady())
	{
		if (!bSaidNotReady)
		{
			bSaidNotReady = true;
			UE_LOG(LogTC, Warning, TEXT("formation controls: the C++ port answers, the Rust core is %s (%s)"), FWW2Core::StateName(FWW2Core::State()), *FWW2Core::Describe());
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
			FWW2FormationItem W;
			W.Id = (uint32)U.Id;
			W.bBack = U.bBack;
			W.X = U.X;
			W.Z = U.Z;
			W.Size = U.Size;
			In.Add(W);
		}
		TArray<FTCSpot> Result;
		if (Why.IsEmpty())
		{
			TArray<FWW2FormationSpot> Out;
			const FWW2Result R = FWW2Core::FormationSpots(In, AtX, AtZ, Face, Reach, (int32)Shape, Spread, Out);
			if (!R.IsOk()) Why = FString::Printf(TEXT("ww2_formation_spots failed (%d: %s)"), R.Code, *R.Error);
			// the contract: one spot per item, or none at all (an empty or degenerate formation)
			else if (Out.Num() != 0 && Out.Num() != Items.Num()) Why = FString::Printf(TEXT("ww2_formation_spots returned %d spots for %d items"), Out.Num(), Items.Num());
			else
			{
				TMap<int32, int32> Left;
				for (const FTCFormationItem& U : Items) Left.FindOrAdd(U.Id)++;
				Result.Reserve(Out.Num());
				for (const FWW2FormationSpot& S : Out)
				{
					int32* Count = S.Id <= (uint32)MAX_int32 ? Left.Find((int32)S.Id) : nullptr;
					if (!Count || *Count == 0) { Why = FString::Printf(TEXT("ww2_formation_spots returned id %u, which no item had"), S.Id); break; }
					--*Count;
					FTCSpot Spot;
					Spot.Id = (int32)S.Id;
					Spot.X = S.X;
					Spot.Z = S.Z;
					Result.Add(Spot);
				}
			}
		}
		if (Why.IsEmpty())
		{
			if (!bSaidCore)
			{
				bSaidCore = true;
				UE_LOG(LogTC, Log, TEXT("formation controls: spots from the Rust core, every shape and spread (%s)"), *FWW2Core::Describe());
			}
			if (Source) *Source = ETCFormationSource::RustCore;
			return Result;
		}
		TCFormationControlsLocal::WarnOnce(Why);
	}
	if (Source) *Source = ETCFormationSource::InterimPort;
	return PortSpots(Items, AtX, AtZ, std::cos(Face), std::sin(Face), Reach, Shape, Spread);
}

TArray<FTCSpot> FTCFormationControls::Formation(const FTCMatchState& State, const TArray<FTCFormationTroop>& InTroops, FVector2D G, const FTCMoveFacing& Facing,
	ETCFormationSource* Source) const
{
	TArray<FTCSpot> Spots;
	if (!InTroops.Num()) return Spots;
	double Face = Facing.Face, Reach = Facing.Reach;
	if (!Facing.bSet || !std::isfinite(Face))
	{
		// a plain click faces the way the units travel: from their centre to the click
		const FVector2D C = TCFormationControlsLocal::Centroid(InTroops);
		Face = std::atan2(G.Y - C.Y, G.X - C.X);
		Reach = 0;
	}
	const TArray<FTCFormationItem> Items = TCFormationControlsLocal::ItemsOf(InTroops);
	Spots = FacingSpots(Items, G.X, G.Y, Face, Reach, Settings.Shape, Settings.Spread, Source);
	const FTCMap& Map = State.Map;
	if (Settings.bSnap)
	{
		TMap<int32, bool> Infantry;
		for (const FTCFormationTroop& T : InTroops) Infantry.Add(T.Id, T.bInfantry);
		// only 'T' cells carry TRENCH (shared/sim.js TERRAIN), and only cells this client knows have a character
		TCFormationPort::SnapToTrench(Spots, [&Infantry](int32 Id) { const bool* B = Infantry.Find(Id); return B && *B; },
			[&Map](int32 X, int32 Y) { return X >= 0 && Y >= 0 && X < Map.W && Y < Map.H && Map.Known[Y * Map.W + X] && Map.Chars[Y * Map.W + X] == 'T'; });
	}
	const double MW = Map.W * (double)FTCMatchState::CellMetres, MH = Map.H * (double)FTCMatchState::CellMetres;
	for (FTCSpot& S : Spots)
	{
		S.X = TCFormationPort::JsMin(MW - 1, TCFormationPort::JsMax(1.0, S.X));
		S.Z = TCFormationPort::JsMin(MH - 1, TCFormationPort::JsMax(1.0, S.Z));
	}
	return Spots;
}

TArray<TSharedPtr<FJsonValue>> FTCFormationControls::Rows(const FTCMatchState& State, const TArray<FTCFormationTroop>& InTroops, FVector2D G, const FTCMoveFacing& Facing) const
{
	TArray<TSharedPtr<FJsonValue>> Out;
	for (const FTCSpot& S : Formation(State, InTroops, G, Facing)) Out.Add(TCFormationControlsLocal::WireRow(S.Id, S.X, S.Z));
	return Out;
}

bool FTCFormationControls::WouldMove(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCCursor& Cursor, bool bOtherKind)
{
	const TArray<FTCFormationTroop> All = Troops(State, Table, Selected, ETCTroopFilter::Orders);
	if (!All.Num()) return false;
	int32 Trucks = 0, Planes = 0;
	bool bInfantry = false;
	for (const FTCFormationTroop& T : All) { Trucks += T.bTruck; Planes += T.bAir; bInfantry |= T.bInfantry; }
	if (Trucks == All.Num()) return !Cursor.Friend && !Cursor.Building && Cursor.bHasGround;
	if (Cursor.Friend && Planes == All.Num()) return false;
	if (Cursor.Friend && bInfantry)
	{
		const FTCUnit* F = State.Units.Find(Cursor.Friend);
		const FTCUnitDef* FD = F ? Table.Find(F->Type) : nullptr;
		if (F && FD && F->Owner == State.You && FD->Carries > 0) return false;
	}
	if (bOtherKind) return false;
	if (Cursor.Enemy) return false;
	return Cursor.bHasGround;
}

FTCOrderPlan FTCFormationControls::PlanMove(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, FVector2D Ground,
	bool bAttack, bool bQueue, const FTCMoveFacing& Facing) const
{
	FTCOrderPlan Plan;
	const TArray<FTCFormationTroop> All = Troops(State, Table, Selected, ETCTroopFilter::Orders);
	if (!All.Num()) return Plan;
	TArray<FTCFormationTroop> Trucks, Marching;
	for (const FTCFormationTroop& T : All) if (T.bTruck) Trucks.Add(T);
	const bool bAmove = Trucks.Num() != All.Num() && bAttack;
	for (const FTCFormationTroop& T : All) if (!(bAmove && T.bTruck)) Marching.Add(T);
	const bool bFaced = Facing.bSet && std::isfinite(Facing.Face);

	TArray<TSharedRef<FJsonObject>> Commands;
	TSharedRef<FJsonObject> Main = MakeShared<FJsonObject>();
	Main->SetStringField(TEXT("t"), bAmove ? TEXT("amove") : TEXT("move"));
	Main->SetArrayField(TEXT("orders"), Rows(State, Marching, Ground, bFaced ? Facing : FTCMoveFacing()));
	if (bFaced) Main->SetNumberField(TEXT("face"), Facing.Face);
	if (Settings.bTogether) Main->SetBoolField(TEXT("together"), true);
	Commands.Add(Main);
	Plan.Units = Marching.Num();
	if (bAmove && Trucks.Num())
	{
		// the trucks of an attack-move get a plain move of their own, facing the way they travel
		TSharedRef<FJsonObject> T = MakeShared<FJsonObject>();
		T->SetStringField(TEXT("t"), TEXT("move"));
		T->SetArrayField(TEXT("orders"), Rows(State, Trucks, Ground, FTCMoveFacing()));
		Commands.Add(T);
		Plan.Units += Trucks.Num();
	}
	if (Commands.Num() == 1)
	{
		Commands[0]->SetBoolField(TEXT("queue"), bQueue);
		Plan.Message = Commands[0];
	}
	else
	{
		TArray<TSharedPtr<FJsonValue>> Parts;
		for (const TSharedRef<FJsonObject>& C : Commands) Parts.Add(MakeShared<FJsonValueObject>(C));
		TSharedRef<FJsonObject> Group = MakeShared<FJsonObject>();
		Group->SetStringField(TEXT("t"), TEXT("orders"));
		Group->SetBoolField(TEXT("queue"), bQueue);
		Group->SetArrayField(TEXT("commands"), Parts);
		Plan.Message = Group;
	}
	Plan.Kind = bAmove ? ETCOrderKind::AttackMove : ETCOrderKind::Move;
	Plan.At = Ground;
	Plan.Color = bAmove ? TCFormationControlsLocal::AttackMoveColor : TCFormationControlsLocal::MoveColor;
	return Plan;
}

FTCFormationResult FTCFormationControls::Run(ETCFormationControl Control, const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected,
	TOptional<double> UpScreenFace)
{
	FTCFormationResult R;
	auto SetShape = [this, &R](ETCFormationShape Shape)
	{
		// setFormation({ shape }): feedback and a blip
		Settings.Shape = Shape;
		R.bDone = true;
		R.Feedback = FString::Printf(TEXT("Formation: %s"), LexToString(Shape));
		R.bBlip = true;
	};
	switch (Control)
	{
	case ETCFormationControl::ShapeLine: SetShape(ETCFormationShape::Line); return R;
	case ETCFormationControl::ShapeBlock: SetShape(ETCFormationShape::Block); return R;
	case ETCFormationControl::ShapeColumn: SetShape(ETCFormationShape::Column); return R;
	case ETCFormationControl::ShapeWedge: SetShape(ETCFormationShape::Wedge); return R;
	case ETCFormationControl::Cycle: SetShape((ETCFormationShape)(((int32)Settings.Shape + 1) % 4)); return R;
	case ETCFormationControl::Together: Settings.bTogether = !Settings.bTogether; R.bDone = true; return R;
	case ETCFormationControl::Snap: Settings.bSnap = !Settings.bSnap; R.bDone = true; return R;
	case ETCFormationControl::Menu: bMenuOpen = !bMenuOpen; R.bDone = true; return R;
	case ETCFormationControl::Tighten:
	case ETCFormationControl::Spread:
		break;
	default:
		return R;
	}

	// reform(step)
	R.bDone = true;
	const double Step = Control == ETCFormationControl::Spread ? SpreadStep : -SpreadStep;
	Settings.Spread = TCFormationPort::JsRound(TCFormationPort::JsMin(SpreadMax, TCFormationPort::JsMax(SpreadMin, Settings.Spread + Step)) * 4) / 4;
	R.Feedback = FString::Printf(TEXT("Formation spacing %sx"), *TCFormationControlsLocal::SpreadText(Settings.Spread));
	R.bBlip = true;
	const TArray<FTCFormationTroop> Mine = Troops(State, Table, Selected, ETCTroopFilter::Mine);
	if (Mine.Num() < 2)
	{
		R.Why = FString::Printf(TEXT("re-forming needs two of your own ground or naval units selected (%d)"), Mine.Num());
		return R;
	}
	const FVector2D C = TCFormationControlsLocal::Centroid(Mine);
	const double* Last = Faced.Find(Mine[0].Id);
	const double Face = Last ? *Last : UpScreenFace.IsSet() ? UpScreenFace.GetValue() : 0.0;
	// faceOrder: every unit remembers the facing, then the formation stays centred where they stand
	Remember(Mine, Face);
	FTCMoveFacing Facing;
	Facing.bSet = true;
	Facing.Face = Face;
	Facing.Reach = 0;
	const TArray<FTCSpot> Spots = Formation(State, Mine, C, Facing);
	double MX = 0, MZ = 0;
	for (const FTCSpot& S : Spots) MX += S.X;
	for (const FTCSpot& S : Spots) MZ += S.Z;
	MX /= Spots.Num();
	MZ /= Spots.Num();
	TArray<TSharedPtr<FJsonValue>> Orders;
	for (const FTCSpot& S : Spots) Orders.Add(TCFormationControlsLocal::WireRow(S.Id, S.X + C.X - MX, S.Z + C.Y - MZ));
	TSharedRef<FJsonObject> Cmd = MakeShared<FJsonObject>();
	Cmd->SetStringField(TEXT("t"), TEXT("move"));
	Cmd->SetArrayField(TEXT("orders"), Orders);
	Cmd->SetField(TEXT("face"), TCFormationControlsLocal::WireNumber(Face));
	R.Message = Cmd;
	R.Units = Orders.Num();
	return R;
}

FTCFormationButton FTCFormationControls::MenuButton() const
{
	FTCFormationButton B;
	B.Control = ETCFormationControl::Menu;
	B.Key = Key(B.Control);
	B.Icon = TEXT("f_wedge");
	B.Name = TEXT("Formation");
	B.Value = TEXT("Form"); // the browser adds a right or down triangle for closed or open
	B.bOn = bMenuOpen;
	B.Tip = TEXT("Formation: shape, spacing, marching together and snapping to trenches. Right-drag sets the facing and the width");
	return B;
}

TArray<FTCFormationButton> FTCFormationControls::SubmenuButtons() const
{
	TArray<FTCFormationButton> Out;
	if (!bMenuOpen) return Out;
	for (int32 I = 0; I < 4; I++)
	{
		const ETCFormationShape Shape = (ETCFormationShape)I;
		FTCFormationButton B;
		B.Control = ShapeControl(Shape);
		B.Key = Key(B.Control);
		B.Icon = FString(TEXT("f_")) + LexToString(Shape);
		B.Name = TCFormationControlsLocal::ShapeNames[I];
		B.bOn = Settings.Shape == Shape;
		B.Tip = FString::Printf(TEXT("%s (Shift+V cycles): %s"), TCFormationControlsLocal::ShapeNames[I], TCFormationControlsLocal::ShapeTips[I]);
		Out.Add(B);
	}
	{
		FTCFormationButton B;
		B.Control = ETCFormationControl::Tighten;
		B.Key = Key(B.Control);
		B.Icon = TEXT("f_tight");
		B.Name = TEXT("Tighten");
		B.Kbd = TEXT("[");
		B.Tip = TEXT("Tighten ([): closer spacing; the selection re-forms where it stands");
		Out.Add(B);
	}
	{
		FTCFormationButton B;
		B.Control = ETCFormationControl::Spread;
		B.Key = Key(B.Control);
		B.Icon = TEXT("f_spread");
		B.Name = TEXT("Spread");
		B.Kbd = TEXT("]");
		B.Value = TCFormationControlsLocal::SpreadText(Settings.Spread) + TEXT("x");
		B.Tip = TEXT("Spread (]): wider spacing; the selection re-forms where it stands");
		Out.Add(B);
	}
	{
		FTCFormationButton B;
		B.Control = ETCFormationControl::Together;
		B.Key = Key(B.Control);
		B.Icon = TEXT("f_together");
		B.Name = TEXT("March together");
		B.bOn = Settings.bTogether;
		B.Value = B.bOn ? TEXT("On") : TEXT("Off");
		B.Tip = TEXT("March together: the group moves at the pace of its slowest unit and arrives in one piece");
		Out.Add(B);
	}
	{
		FTCFormationButton B;
		B.Control = ETCFormationControl::Snap;
		B.Key = Key(B.Control);
		B.Icon = TEXT("f_snap");
		B.Name = TEXT("Snap to trenches");
		B.bOn = Settings.bSnap;
		B.Value = B.bOn ? TEXT("On") : TEXT("Off");
		B.Tip = TEXT("Snap to trenches: infantry placed within 3 m of a trench step into it");
		Out.Add(B);
	}
	return Out;
}

double FTCFormationControls::UpScreenFace(FVector2D G0, FVector2D G1)
{
	return std::atan2(G1.Y - G0.Y, G1.X - G0.X);
}

void FTCFormationControls::OnGroupChord(ETCGroupOp Op, int32 Group)
{
	if (Op == ETCGroupOp::Set) GroupForm.Add(Group, Settings);
	else if (Op == ETCGroupOp::Recall)
		if (const FTCFormationSettings* Saved = GroupForm.Find(Group)) Settings = *Saved;
}

bool FTCFormationControls::BeginGesture(ETCGestureButton Button, FVector2D PressPixel, const FTCCursor& Cursor, bool bAttack, bool bShift, bool bCtrl)
{
	CancelGesture();
	if (!Cursor.bHasGround) return false;
	Gesture.bActive = true;
	Gesture.Button = Button;
	Gesture.Cursor = Cursor;
	Gesture.PressPixel = PressPixel;
	Gesture.bAttack = bAttack;
	Gesture.bPressShift = bShift;
	Gesture.bPressCtrl = bCtrl;
	Gesture.bCtrl = bCtrl;
	return true;
}

void FTCFormationControls::UpdateGesture(FVector2D Pixel, bool bHasGround, FVector2D Ground, bool bMouseMoved, bool bCtrl, bool bButtonHeld)
{
	if (!Gesture.bActive) return;
	if (bMouseMoved)
	{
		if (!bButtonHeld) { CancelGesture(); return; }
		Gesture.bCtrl = bCtrl;
	}
	const TCFormationPort::FFacingStep Step = TCFormationPort::UpdateFacing(Gesture.PressPixel.X, Gesture.PressPixel.Y, Gesture.Cursor.Ground.X, Gesture.Cursor.Ground.Y,
		Pixel.X, Pixel.Y, bHasGround, Ground.X, Ground.Y, FacingPixels, FacingMetres);
	if (Step.bSet) { Gesture.bFacing = true; Gesture.Face = Step.Face; Gesture.Reach = Step.Reach; }
	else if (Step.bClear) { Gesture.bFacing = false; Gesture.Face = Gesture.Reach = 0; }
}

bool FTCFormationControls::ReleaseGesture(ETCGestureButton Button, FVector2D Pixel, bool bHasGround, FVector2D Ground, bool bShift, bool bCtrl,
	const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, FTCFacingRelease& Out)
{
	if (!Gesture.bActive || Gesture.Button != Button) return false;
	// the release's own updateFacing (no mouse move: Ctrl is not taken from it, the button is the one released)
	UpdateGesture(Pixel, bHasGround, Ground, false, false, true);
	const FTCFacingGesture F = Gesture;
	CancelGesture();
	const bool bFacing = F.bFacing && std::isfinite(F.Face);
	Out = FTCFacingRelease();
	Out.Cursor = F.Cursor;
	Out.bCancelAim = F.bAttack;
	if (bFacing)
	{
		Remember(Troops(State, Table, Selected, ETCTroopFilter::Mine), F.Face);
		Out.Facing.bSet = true;
		Out.Facing.Face = F.Face;
		Out.Facing.Reach = F.Reach;
	}
	// dispatch(cursor, event, options): with a facing the release's Shift and Ctrl join the press's
	const bool bEventShift = bFacing ? F.bPressShift || bShift : F.bPressShift;
	const bool bEventCtrl = bFacing ? F.bPressCtrl || bCtrl : F.bPressCtrl;
	const bool bOptionAttack = bFacing ? F.bAttack || F.bPressCtrl || bCtrl : F.bAttack;
	Out.bQueue = bEventShift;
	Out.bAttack = bOptionAttack || bEventCtrl;
	return true;
}

FTCFormationPreview FTCFormationControls::Preview(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected)
{
	FTCFormationPreview P;
	if (!Gesture.bActive || !Gesture.bFacing || !std::isfinite(Gesture.Face) || !State.bHasMatch) return P;
	const TArray<FTCFormationTroop> All = Troops(State, Table, Selected, ETCTroopFilter::Orders);
	if (!All.Num()) { CancelGesture(); return P; }
	FTCMoveFacing Facing;
	Facing.bSet = true;
	Facing.Face = Gesture.Face;
	Facing.Reach = Gesture.Reach;
	P.Spots = Formation(State, All, Gesture.Cursor.Ground, Facing);
	P.Face = Gesture.Face;
	P.Reach = Gesture.Reach;
	P.At = Gesture.Cursor.Ground;
	P.Color = Gesture.bAttack || Gesture.bPressCtrl || Gesture.bCtrl ? TCFormationControlsLocal::PlanAttackColor : TCFormationControlsLocal::MoveColor;
	if (!P.Spots.Num()) return P;

	const double Fx = std::cos(P.Face), Fz = std::sin(P.Face), Ax = -Fz, Az = Fx;
	auto Line = [&P](double X0, double Z0, double X1, double Z1) { P.Lines.Add(TPair<FVector2D, FVector2D>(FVector2D(X0, Z0), FVector2D(X1, Z1))); };
	// overlay.js arrow, placed as formation-preview.js chevron: centred on (X, Z), pointing along the facing
	auto Chevron = [&P, Fx, Fz](double X, double Z, double Length, double Half)
	{
		const double TX = X + Fx * Length / 2, TZ = Z + Fz * Length / 2, BX = TX - Fx * Length, BZ = TZ - Fz * Length, NX = -Fz, NZ = Fx;
		FTCFormationPreview::FArrow A;
		A.Tip = FVector2D(TX, TZ);
		A.Left = FVector2D(BX + NX * Half, BZ + NZ * Half);
		A.Right = FVector2D(BX - NX * Half, BZ - NZ * Half);
		P.Arrows.Add(A);
	};
	const TArray<int32> Pairs = TCFormationPort::RankNeighbours(P.Spots, Fx, Fz);
	for (int32 I = 0; I + 1 < Pairs.Num(); I += 2) Line(P.Spots[Pairs[I]].X, P.Spots[Pairs[I]].Z, P.Spots[Pairs[I + 1]].X, P.Spots[Pairs[I + 1]].Z);
	TMap<int32, double> Radius;
	for (const FTCFormationTroop& T : All) Radius.FindOrAdd(T.Id) = T.Radius;
	for (const FTCSpot& S : P.Spots)
	{
		const double* Rp = Radius.Find(S.Id);
		const double R = Rp ? *Rp : 1.0;
		const double HalfSide = TCFormationPort::JsMax(0.85, R), HalfDepth = TCFormationPort::JsMax(0.85, R * 0.8);
		const double X0 = S.X - Ax * HalfSide - Fx * HalfDepth, Z0 = S.Z - Az * HalfSide - Fz * HalfDepth;
		const double X1 = S.X + Ax * HalfSide - Fx * HalfDepth, Z1 = S.Z + Az * HalfSide - Fz * HalfDepth;
		const double X2 = S.X + Ax * HalfSide + Fx * HalfDepth, Z2 = S.Z + Az * HalfSide + Fz * HalfDepth;
		const double X3 = S.X - Ax * HalfSide + Fx * HalfDepth, Z3 = S.Z - Az * HalfSide + Fz * HalfDepth;
		Line(X0, Z0, X1, Z1);
		Line(X1, Z1, X2, Z2);
		Line(X2, Z2, X3, Z3);
		Line(X3, Z3, X0, Z0);
		Chevron(S.X + Fx * (HalfDepth + 0.65), S.Z + Fz * (HalfDepth + 0.65), 0.9, 0.65);
	}
	const double Reach = std::isfinite(P.Reach) ? TCFormationPort::JsMax(2.0, P.Reach) : 4.0;
	const double EX = P.At.X + Fx * Reach, EZ = P.At.Y + Fz * Reach;
	Line(P.At.X, P.At.Y, EX, EZ);
	Chevron(EX, EZ, 1.8, 1.15);
	P.bVisible = true;
	return P;
}

TOptional<double> FTCFormationControls::FacedOf(int32 Id) const
{
	const double* F = Faced.Find(Id);
	return F ? TOptional<double>(*F) : TOptional<double>();
}

void FTCFormationControls::Remember(const TArray<FTCFormationTroop>& InTroops, double Face)
{
	for (const FTCFormationTroop& T : InTroops) Faced.Add(T.Id, Face);
}

// ThreeCrossroads.Formation.Controls: the menu's state rules, the facing gesture's thresholds and dispatch flags, and
// the spots of every shape and spread through FacingSpots (the Rust core when ready) against the reference fixture,
// with the C++ port measured against the same fixture. Run:
//   UnrealEditor-Cmd ThreeCrossroads.uproject -nullrhi -ExecCmds="Automation RunTests ThreeCrossroads.Formation; Quit"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "WW2FormationFixture.h"

namespace TCFormationControlsTestLocal
{
	// The fixture's ids are any uint32; the client's are int32. Ids only travel through facingSpots, so each case runs
	// with its items' indices as ids and the spots get the fixture's ids back.
	TArray<FTCFormationItem> IndexedItems(const FWW2FixtureCase& C)
	{
		TArray<FTCFormationItem> Out;
		for (int32 I = 0; I < C.Items.Num(); I++)
		{
			FTCFormationItem It;
			It.Id = I;
			It.X = C.Items[I].X;
			It.Z = C.Items[I].Z;
			It.Size = C.Items[I].Size;
			It.bBack = C.Items[I].bBack;
			Out.Add(It);
		}
		return Out;
	}

	TArray<FWW2FormationSpot> FixtureIds(const FWW2FixtureCase& C, const TArray<FTCSpot>& Spots)
	{
		TArray<FWW2FormationSpot> Out;
		for (const FTCSpot& S : Spots) Out.Add({ C.Items.IsValidIndex(S.Id) ? C.Items[S.Id].Id : 0xFFFFFFFFu, S.X, S.Z });
		return Out;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFormationControlsTest, "ThreeCrossroads.Formation.Controls", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCFormationControlsTest::RunTest(const FString& Parameters)
{
	const FTCMatchState NoMatch;
	const FTCUnitTable NoTable;
	const TSet<int32> NoneSelected;

	// The menu (client/main.js setFormation, cycleFormation, reform; client/hud.js button states).
	FTCFormationControls F;
	TestTrue(TEXT("the browser's defaults: line, spread 1, together, snap"), F.Settings == FTCFormationSettings());
	const TCHAR* Cycle[] = { TEXT("Formation: block"), TEXT("Formation: column"), TEXT("Formation: wedge"), TEXT("Formation: line") };
	for (const TCHAR* Expected : Cycle) TestEqual(TEXT("Shift+V cycles in SHAPES order"), F.Run(ETCFormationControl::Cycle, NoMatch, NoTable, NoneSelected, {}).Feedback, FString(Expected));
	FTCFormationResult R = F.Run(ETCFormationControl::Tighten, NoMatch, NoTable, NoneSelected, {});
	TestEqual(TEXT("tighten stops at 1"), F.Settings.Spread, 1.0);
	TestEqual(TEXT("tighten's feedback"), R.Feedback, FString(TEXT("Formation spacing 1x")));
	TestFalse(TEXT("no re-form without two own units"), R.Message.IsValid());
	F.Run(ETCFormationControl::Spread, NoMatch, NoTable, NoneSelected, {});
	TestEqual(TEXT("spread steps a quarter"), F.Settings.Spread, 1.25);
	for (int32 I = 0; I < 8; I++) R = F.Run(ETCFormationControl::Spread, NoMatch, NoTable, NoneSelected, {});
	TestEqual(TEXT("spread stops at 2.5"), F.Settings.Spread, 2.5);
	TestEqual(TEXT("spread's feedback"), R.Feedback, FString(TEXT("Formation spacing 2.5x")));
	F.Run(ETCFormationControl::Menu, NoMatch, NoTable, NoneSelected, {});
	const TArray<FTCFormationButton> Buttons = F.SubmenuButtons();
	TestEqual(TEXT("the open submenu has eight buttons"), Buttons.Num(), 8);
	if (Buttons.Num() == 8)
	{
		TestEqual(TEXT("the Spread button shows the spacing"), Buttons[5].Value, FString(TEXT("2.5x")));
		TestEqual(TEXT("the line is on"), Buttons[0].bOn, true);
		TestEqual(TEXT("March together shows On"), Buttons[6].Value, FString(TEXT("On")));
	}
	for (int32 C = (int32)ETCFormationControl::ShapeLine; C <= (int32)ETCFormationControl::Cycle; C++)
		TestEqual(TEXT("keys parse back"), (int32)FTCFormationControls::ParseKey(FTCFormationControls::Key((ETCFormationControl)C)), C);
	// groupForm: Ctrl+N saves a copy, N restores it; a group never saved changes nothing
	F.Run(ETCFormationControl::ShapeWedge, NoMatch, NoTable, NoneSelected, {});
	F.OnGroupChord(ETCGroupOp::Set, 3);
	const FTCFormationSettings Saved = F.Settings;
	F.Run(ETCFormationControl::ShapeColumn, NoMatch, NoTable, NoneSelected, {});
	F.Run(ETCFormationControl::Snap, NoMatch, NoTable, NoneSelected, {});
	F.OnGroupChord(ETCGroupOp::Recall, 4);
	TestTrue(TEXT("recalling an unsaved group keeps the settings"), F.Settings != Saved);
	F.OnGroupChord(ETCGroupOp::Recall, 3);
	TestTrue(TEXT("recalling group 3 restores its copy"), F.Settings == Saved);

	// The facing gesture (client/main.js updateFacing: 12 px on the screen, then 2 m on the ground).
	FTCCursor Press;
	Press.bHasGround = true;
	Press.Ground = FVector2D(50, 50);
	FTCFacingRelease Out;
	TestTrue(TEXT("a right press with ground begins"), F.BeginGesture(ETCGestureButton::Right, FVector2D(100, 100), Press, false, true, false));
	F.UpdateGesture(FVector2D(105, 100), true, FVector2D(80, 80), true, false, true);
	TestFalse(TEXT("within 12 px: no facing"), F.CurrentGesture().bFacing);
	F.UpdateGesture(FVector2D(200, 100), true, FVector2D(51, 50), true, false, true);
	TestFalse(TEXT("under 2 m on the ground: no facing"), F.CurrentGesture().bFacing);
	F.UpdateGesture(FVector2D(200, 100), true, FVector2D(60, 60), true, false, true);
	TestTrue(TEXT("a 2 m drag sets the facing"), F.CurrentGesture().bFacing);
	TestEqual(TEXT("the facing is the drag's angle"), F.CurrentGesture().Face, std::atan2(10.0, 10.0), 0.0);
	F.UpdateGesture(FVector2D(300, 100), false, FVector2D::ZeroVector, true, false, true);
	TestTrue(TEXT("no ground past 12 px keeps the facing"), F.CurrentGesture().bFacing);
	F.UpdateGesture(FVector2D(104, 100), false, FVector2D::ZeroVector, true, false, true);
	TestFalse(TEXT("back at the press point: the facing is dropped"), F.CurrentGesture().bFacing);
	F.UpdateGesture(FVector2D(200, 100), true, FVector2D(60, 60), true, false, true);
	TestFalse(TEXT("another button's release is not the gesture's"), F.ReleaseGesture(ETCGestureButton::Left, FVector2D(200, 100), true, FVector2D(60, 60), false, true, NoMatch, NoTable, NoneSelected, Out));
	TestTrue(TEXT("the right release ends it"), F.ReleaseGesture(ETCGestureButton::Right, FVector2D(200, 100), true, FVector2D(60, 60), false, true, NoMatch, NoTable, NoneSelected, Out));
	TestFalse(TEXT("ended"), F.IsGesturing());
	TestTrue(TEXT("a facing release carries the facing"), Out.Facing.bSet);
	TestTrue(TEXT("Shift at the press queues"), Out.bQueue);
	TestTrue(TEXT("Ctrl at a facing release attack-moves"), Out.bAttack);
	F.BeginGesture(ETCGestureButton::Right, FVector2D(100, 100), Press, false, false, false);
	F.ReleaseGesture(ETCGestureButton::Right, FVector2D(100, 100), true, FVector2D(50, 50), true, true, NoMatch, NoTable, NoneSelected, Out);
	TestFalse(TEXT("a plain click has no facing"), Out.Facing.bSet);
	TestFalse(TEXT("a plain click takes the press's modifiers only (Ctrl)"), Out.bAttack);
	TestFalse(TEXT("a plain click takes the press's modifiers only (Shift)"), Out.bQueue);
	F.BeginGesture(ETCGestureButton::Left, FVector2D(100, 100), Press, true, false, false);
	F.UpdateGesture(FVector2D(120, 100), true, FVector2D(60, 50), true, false, false);
	TestFalse(TEXT("a move with the button up cancels"), F.IsGesturing());

	// Spots of every shape and spread through FacingSpots, against the fixture.
	AddInfo(FWW2Core::Describe());
	if (!FWW2Core::IsReady())
	{
		ETCFormationSource Source = ETCFormationSource::RustCore;
		FTCFormationControls::FacingSpots({}, 0, 0, 0, 0, ETCFormationShape::Wedge, 1, &Source);
		TestEqual(TEXT("without the core the port answers"), (int32)Source, (int32)ETCFormationSource::InterimPort);
		AddInfo(TEXT("no fixture comparison: the fixture is installed with the library"));
		return true;
	}
	FWW2FormationFixture Fx;
	FString Error;
	if (!TestTrue(FString::Printf(TEXT("reference fixture loads: %s"), *Error), Fx.Load(FWW2FormationFixture::InstalledPath(), Error))) return false;
	int32 Cases = 0, ClientDiffer = 0, PortDiffer = 0;
	int32 PerShape[4] = { 0, 0, 0, 0 };
	TArray<FString> PortExamples;
	auto Compare = [&](const FWW2FixtureCase& C, TFunctionRef<bool(TConstArrayView<FWW2FormationSpot>)> MatchesJs)
	{
		if (C.Shape < 0 || C.Shape > 3) return;
		const TArray<FTCFormationItem> Items = TCFormationControlsTestLocal::IndexedItems(C);
		const ETCFormationShape Shape = (ETCFormationShape)C.Shape;
		ETCFormationSource Source = ETCFormationSource::InterimPort;
		const TArray<FTCSpot> Client = FTCFormationControls::FacingSpots(Items, C.AtX, C.AtZ, C.Face, C.Reach, Shape, C.Spread, &Source);
		Cases++;
		PerShape[C.Shape]++;
		if (Source != ETCFormationSource::RustCore || !MatchesJs(TCFormationControlsTestLocal::FixtureIds(C, Client)))
		{
			ClientDiffer++;
			AddError(FString::Printf(TEXT("%s (%s, spread %g): the client's spots differ from the JavaScript"), *C.Label, LexToString(Shape), C.Spread));
		}
		if (!MatchesJs(TCFormationControlsTestLocal::FixtureIds(C, FTCFormationControls::PortSpots(Items, C.AtX, C.AtZ, std::cos(C.Face), std::sin(C.Face), C.Reach, Shape, C.Spread))))
		{
			PortDiffer++;
			if (PortExamples.Num() < 8) PortExamples.Add(C.Label);
		}
	};
	for (const FWW2FixtureCase& W : Fx.Written) Compare(W, [&W](TConstArrayView<FWW2FormationSpot> S) { return FWW2FormationFixture::SameSpots(S, W.Spots); });
	for (const FWW2FixtureRange& Range : Fx.Ranges)
	{
		if (!FWW2FormationFixture::CanRebuild(Range)) continue;
		FWW2Mulberry32 Rand(Range.Seed);
		for (int32 I = 0; I < Range.Count; I++)
		{
			FWW2FixtureCase C = FWW2FormationFixture::MakeScatterCase(Rand, Range.Fewest, Range.Most);
			C.Label = FString::Printf(TEXT("%s#%d"), *Range.Label, I);
			const uint64 Hash = Range.Hashes.IsValidIndex(I) ? Range.Hashes[I] : 0;
			Compare(C, [Hash](TConstArrayView<FWW2FormationSpot> S) { return FWW2FormationFixture::HashSpots(S) == Hash; });
		}
	}
	AddInfo(FString::Printf(TEXT("facingSpots, all shapes and spreads: %d cases (line %d, block %d, column %d, wedge %d); the client (Rust core) differs from the JavaScript in %d, the C++ port with this platform's cos and sin in %d%s%s"),
		Cases, PerShape[0], PerShape[1], PerShape[2], PerShape[3], ClientDiffer, PortDiffer, PortExamples.Num() ? TEXT(", e.g. ") : TEXT(""), *FString::Join(PortExamples, TEXT(", "))));
	TestEqual(TEXT("client formations differing from the JavaScript"), ClientDiffer, 0);
	return true;
}

#endif
