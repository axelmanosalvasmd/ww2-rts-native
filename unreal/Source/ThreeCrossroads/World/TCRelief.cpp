// client/relief.js and client/water-levels.js (pin e4e579a), ported line for line; see TCRelief.h. The reference's
// Float32Array stores (heights, normals, controls, tangents, road corners, water lines, the geometry) are float here
// and every intermediate is double, as in JavaScript. Comments name the reference function each block ports.
#include "World/TCRelief.h"

#include <algorithm>
#include <cmath>
#include <limits>

namespace TCRelief
{
namespace ReliefMath
{
	inline double JsRound(double V) { return std::floor(V + 0.5); } // Math.round
	inline double Sign(double V) { return V > 0 ? 1.0 : V < 0 ? -1.0 : 0.0; }
	inline double ClampD(double V, double A, double B) { return std::max(A, std::min(B, V)); }
	inline int ClampI(int V, int A, int B) { return std::max(A, std::min(B, V)); }
	inline double Ease(double T) { return T * T * (3 - 2 * T); }
	inline double Fract(double V) { return V - std::floor(V); }
	inline double Hash(double A, double B, double K) { return Fract(std::sin(A * 127.1 + B * 311.7 + K * 74.7) * 43758.5453); }
	// relief.js fieldNoise: cell-scale value noise
	inline double FieldNoise(double U, double V, double K)
	{
		const double X0 = std::floor(U), Y0 = std::floor(V), TX = U - X0, TY = V - Y0;
		const double SX = TX * TX * (3 - 2 * TX), SY = TY * TY * (3 - 2 * TY);
		const double A = Hash(X0, Y0, K), B = Hash(X0 + 1, Y0, K), C = Hash(X0, Y0 + 1, K), D = Hash(X0 + 1, Y0 + 1, K);
		return (A + (B - A) * SX) * (1 - SY) + (C + (D - C) * SX) * SY;
	}
	inline double Hypot2(double A, double B) { return std::hypot(A, B); }
	inline double Hypot3(double A, double B, double C) { return std::sqrt(A * A + B * B + C * C); }
	constexpr double PiV = 3.141592653589793;
	constexpr double Sqrt1_2 = 0.7071067811865476;
	constexpr double Inf = std::numeric_limits<double>::infinity();

	// relief.js hermite
	inline void Hermite(double T, double Basis[4], double Der[4])
	{
		const double T2 = T * T, T3 = T2 * T;
		Basis[0] = 2 * T3 - 3 * T2 + 1; Basis[1] = -2 * T3 + 3 * T2;
		Basis[2] = T3 - 2 * T2 + T; Basis[3] = T3 - T2;
		Der[0] = 6 * T2 - 6 * T; Der[1] = -Der[0];
		Der[2] = 3 * T2 - 4 * T + 1; Der[3] = 3 * T2 - 2 * T;
	}

	// three.js r186 IcosahedronGeometry(1, detail): PolyhedronGeometry subdivide, applyRadius, Float32 positions
	std::vector<float> Icosahedron(int Detail)
	{
		const double T = (1 + std::sqrt(5.0)) / 2;
		const double Vt[36] = { -1, T, 0, 1, T, 0, -1, -T, 0, 1, -T, 0, 0, -1, T, 0, 1, T, 0, -1, -T, 0, 1, -T, T, 0, -1, T, 0, 1, -T, 0, -1, -T, 0, 1 };
		const int Ix[60] = { 0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7, 1, 8,
			3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9, 8, 1 };
		struct FV { double X, Y, Z; };
		auto Lerp = [](FV A, const FV& B, double Alpha) { A.X += (B.X - A.X) * Alpha; A.Y += (B.Y - A.Y) * Alpha; A.Z += (B.Z - A.Z) * Alpha; return A; };
		std::vector<double> Buf;
		for (int F = 0; F < 60; F += 3)
		{
			const FV A{ Vt[Ix[F] * 3], Vt[Ix[F] * 3 + 1], Vt[Ix[F] * 3 + 2] };
			const FV B{ Vt[Ix[F + 1] * 3], Vt[Ix[F + 1] * 3 + 1], Vt[Ix[F + 1] * 3 + 2] };
			const FV C{ Vt[Ix[F + 2] * 3], Vt[Ix[F + 2] * 3 + 1], Vt[Ix[F + 2] * 3 + 2] };
			const int Cols = Detail + 1;
			std::vector<std::vector<FV>> Grid((size_t)Cols + 1);
			for (int I = 0; I <= Cols; I++)
			{
				const FV Aj = Lerp(A, C, (double)I / Cols), Bj = Lerp(B, C, (double)I / Cols);
				const int RowsN = Cols - I;
				for (int J = 0; J <= RowsN; J++) Grid[(size_t)I].push_back(J == 0 && I == Cols ? Aj : Lerp(Aj, Bj, (double)J / RowsN));
			}
			auto Push = [&Buf](const FV& P) { Buf.push_back(P.X); Buf.push_back(P.Y); Buf.push_back(P.Z); };
			for (int I = 0; I < Cols; I++)
				for (int J = 0; J < 2 * (Cols - I) - 1; J++)
				{
					const int K = J / 2;
					if (J % 2 == 0) { Push(Grid[(size_t)I][(size_t)K + 1]); Push(Grid[(size_t)I + 1][(size_t)K]); Push(Grid[(size_t)I][(size_t)K]); }
					else { Push(Grid[(size_t)I][(size_t)K + 1]); Push(Grid[(size_t)I + 1][(size_t)K + 1]); Push(Grid[(size_t)I + 1][(size_t)K]); }
				}
		}
		std::vector<float> Out(Buf.size());
		for (size_t I = 0; I < Buf.size(); I += 3)
		{
			const double L = std::sqrt(Buf[I] * Buf[I] + Buf[I + 1] * Buf[I + 1] + Buf[I + 2] * Buf[I + 2]);
			const double Inv = 1 / (L != 0 ? L : 1);
			Out[I] = (float)(Buf[I] * Inv); Out[I + 1] = (float)(Buf[I + 1] * Inv); Out[I + 2] = (float)(Buf[I + 2] * Inv);
		}
		return Out;
	}

	// relief.js rockModels: a lumpy sphere cut by cleavage planes
	std::vector<std::vector<double>> MakeRocks(int Detail)
	{
		const std::vector<float> P = Icosahedron(Detail);
		std::vector<std::vector<double>> Models;
		for (int M = 0; M < 6; M++)
		{
			std::vector<double> Cuts = { 0, 0, 1, 0.42 + Hash(M, 1, 3) * 0.15, 0, 1, 0, 0.45 + Hash(M, 2, 3) * 0.2 };
			for (int J = 0; J < 7; J++)
			{
				const double A = (J + Hash(M, J, 5) * 0.8) / 7 * PiV * 2, B = (Hash(M, J, 7) - 0.4) * 1.3;
				Cuts.insert(Cuts.end(), { std::cos(A) * std::cos(B), std::sin(B), std::sin(A) * std::cos(B), 0.45 + Hash(M, J, 9) * 0.2 });
			}
			std::vector<double> Model;
			for (size_t I = 0; I < P.size(); I += 3)
			{
				double X = P[I], Y = P[I + 1], Z = P[I + 2];
				const double R = 0.72 + 0.56 * FieldNoise(X * 2.3 + Z * 1.3 + M * 7.3, Y * 2.3 - Z * 1.7, 31 + M);
				X *= R; Y *= R; Z *= R;
				for (size_t K = 0; K < Cuts.size(); K += 4)
				{
					const double Over = X * Cuts[K] + Y * Cuts[K + 1] + Z * Cuts[K + 2] - Cuts[K + 3];
					if (Over > 0) { X -= Cuts[K] * Over; Y -= Cuts[K + 1] * Over; Z -= Cuts[K + 2] * Over; }
				}
				Model.insert(Model.end(), { X, Y, Z });
			}
			Models.push_back(std::move(Model));
		}
		return Models;
	}

	// relief.js rockPlan / filler rocks
	struct FRock
	{
		const std::vector<double>* Model = nullptr;
		double Along = 0, T = 0, Size[3] = { 0, 0, 0 }, Out = 0, Yaw = 0, Crest = 0, Lift = 0;
		bool bLoose = false;
	};
}
using namespace ReliefMath;

const std::vector<std::vector<double>>& RockModels(int Detail)
{
	static const std::vector<std::vector<double>> Coarse = MakeRocks(0), Fine = MakeRocks(1);
	return Detail ? Fine : Coarse;
}

void FPart::Clear()
{
	Pos.clear(); Nrm.clear(); Uv.clear(); Paint.clear(); Scar.clear(); Idx.clear(); Owner.clear(); Detail.clear(); Picks.clear();
}

// ------------------------------------------------------------------------------------------------- cells and quads

int FRelief::IdAt(int X, int Y) const { return ClampI(Y, 0, H - 1) * W + ClampI(X, 0, W - 1); }

bool FRelief::Joined(int A, int B) const
{
	return A == B || (Kn(A) && Kn(B) && std::abs(Levels[(size_t)A] - Levels[(size_t)B]) < 2);
}

bool FRelief::Cliff(int A, int B) const
{
	return A != B && Kn(A) && Kn(B) && std::abs(Levels[(size_t)A] - Levels[(size_t)B]) >= 2;
}

int FRelief::PartOfCell(int C) const
{
	const int X = C % W, Z = C / W;
	return ChunkCells > 0 ? (Z / ChunkCells) * PartsX + X / ChunkCells : Z;
}

void FRelief::Reset(int InW, int InH, bool bInLow, int InChunkCells, bool bInDrawnPicks)
{
	W = std::max(0, InW); H = std::max(0, InH); N = W * H; QW = W + 1;
	bLow = bInLow; ChunkCells = std::max(0, InChunkCells); bDrawnPicks = bInDrawnPicks;
	bMergeFlatRuns = ChunkCells <= 0;
	const size_t Nn = (size_t)N;
	Levels.assign(Nn, 0); Roads.assign(Nn, 0); NX.assign(Nn, 1); NZ.assign(Nn, 1); Flat.assign(Nn, 1); Ragged.assign(Nn, 0);
	Torn.assign(Nn, 0); Shoved.assign(Nn, 0); Fan.assign(Nn, 0); Wet.assign(Nn, 0); Bridges.assign(Nn, 0); Depth.assign(Nn, 0);
	Masks.assign((size_t)(W + 1) * (size_t)(H + 1), 0);
	Heights.assign(Nn * Stride, 0.f); Normals.assign(Nn * Stride * 3, 0.f); Controls.assign(Nn * 9, 0.f);
	RoadCorners.assign(Nn * 4, 0.f); Tangents.assign(Nn * 18, 0.f); WL.assign(Nn, std::numeric_limits<float>::quiet_NaN());
	Rows.assign((size_t)H, FRowPlan());
	PartsX = ChunkCells > 0 ? (W + ChunkCells - 1) / ChunkCells : 1;
	PartsY = ChunkCells > 0 ? (H + ChunkCells - 1) / ChunkCells : H;
	Parts.assign((size_t)(PartsX * PartsY), FPart());
	Rebuilt.clear();
	Stats = FStats();
	if (InLevel.size() != Nn) InLevel.assign(Nn, 0);
	if (InFrozen.size() != Nn) InFrozen.assign(Nn, 0);
	if (InGrid.size() != Nn) InGrid.assign(Nn, '.');
	if (InRoad.size() != Nn) InRoad.assign(Nn, 0);
	if (InKnown.size() != Nn) InKnown.assign(Nn, 1);
}

// relief.js readCell
void FRelief::ReadCell(int C)
{
	Levels[(size_t)C] = InLevel[(size_t)C];
	Roads[(size_t)C] = Kn(C) && InRoad[(size_t)C] ? 1 : 0;
}

// relief.js readQuad: which of the four cells around a corner each one joins
void FRelief::ReadQuad(int X, int Y)
{
	const int A = IdAt(X - 1, Y - 1), B = IdAt(X, Y - 1), C = IdAt(X - 1, Y), D = IdAt(X, Y);
	int Edges[4] = { 1, 2, 4, 8 };
	if (Joined(A, B)) { Edges[0] |= 2; Edges[1] |= 1; }
	if (Joined(A, C)) { Edges[0] |= 4; Edges[2] |= 1; }
	if (Joined(B, D)) { Edges[1] |= 8; Edges[3] |= 2; }
	if (Joined(C, D)) { Edges[2] |= 8; Edges[3] |= 4; }
	int Packed = 0;
	for (int Seed = 0; Seed < 4; Seed++)
	{
		int Bits = Edges[Seed];
		for (int Pass = 0; Pass < 3; Pass++) for (int K = 0; K < 4; K++) if (Bits & (1 << K)) Bits |= Edges[K];
		Packed |= Bits << (Seed * 4);
	}
	Masks[(size_t)(Y * QW + X)] = (uint16_t)Packed;
}

// relief.js readWater with water-levels.js read: bodies by four-neighbour flood, each at the lowest frozen level of its
// non-bridge cells; banks take the lowest line among their eight neighbours; depth 1 at the shore, 2 one row in, 3 beyond
void FRelief::ReadWater()
{
	std::vector<int> CompOf((size_t)N, -1), Queue((size_t)N);
	std::vector<double> CompLevel;
	for (int C = 0; C < N; C++)
	{
		const uint8_t Chr = Ch(C);
		Wet[(size_t)C] = (Chr == 'W' || Chr == 'F' || Chr == '=') ? 1 : 0;
		Bridges[(size_t)C] = Chr == '=' ? 1 : 0;
	}
	for (int C = 0; C < N; C++)
	{
		if (!Wet[(size_t)C] || CompOf[(size_t)C] >= 0) continue;
		const int Id = (int)CompLevel.size();
		int Head = 0, Tail = 1;
		double Water = Inf, Any = Inf;
		Queue[0] = C; CompOf[(size_t)C] = Id;
		while (Head < Tail)
		{
			const int K = Queue[(size_t)Head++], X = K % W, Y = K / W;
			Any = std::min(Any, (double)InFrozen[(size_t)K]);
			if (!Bridges[(size_t)K]) Water = std::min(Water, (double)InFrozen[(size_t)K]);
			for (int Side = 0; Side < 4; Side++)
			{
				if ((Side == 0 && X == 0) || (Side == 1 && X == W - 1) || (Side == 2 && Y == 0) || (Side == 3 && Y == H - 1)) continue;
				const int Next = K + (Side == 0 ? -1 : Side == 1 ? 1 : Side == 2 ? -W : W);
				if (!Wet[(size_t)Next] || CompOf[(size_t)Next] >= 0) continue;
				CompOf[(size_t)Next] = Id; Queue[(size_t)Tail++] = Next;
			}
		}
		CompLevel.push_back((Water == Inf ? Any : Water) * LevelHeight);
	}
	const float NaN = std::numeric_limits<float>::quiet_NaN();
	for (int C = 0; C < N; C++)
	{
		WL[(size_t)C] = NaN;
		if (Wet[(size_t)C]) { WL[(size_t)C] = (float)CompLevel[(size_t)CompOf[(size_t)C]]; continue; }
		const int X = C % W, Y = C / W;
		for (int DY = -1; DY <= 1; DY++) for (int DX = -1; DX <= 1; DX++)
		{
			const int NXc = X + DX, NYc = Y + DY, K = NYc * W + NXc;
			if (NXc >= 0 && NYc >= 0 && NXc < W && NYc < H && Wet[(size_t)K] && !((double)WL[(size_t)C] <= CompLevel[(size_t)CompOf[(size_t)K]]))
				WL[(size_t)C] = (float)CompLevel[(size_t)CompOf[(size_t)K]];
		}
	}
	for (int C = 0; C < N; C++) Depth[(size_t)C] = 3;
	for (int C = 0; C < N; C++)
	{
		if (!Wet[(size_t)C] || Bridges[(size_t)C]) { Depth[(size_t)C] = 0; continue; }
		const int X = C % W, Y = C / W;
		if ((X > 0 && !Wet[(size_t)C - 1]) || (X + 1 < W && !Wet[(size_t)C + 1]) || (Y > 0 && !Wet[(size_t)(C - W)]) || (Y + 1 < H && !Wet[(size_t)(C + W)])) Depth[(size_t)C] = 1;
	}
	for (int C = 0; C < N; C++) if (Depth[(size_t)C] > 1)
	{
		const int X = C % W, Y = C / W;
		if ((X > 0 && Depth[(size_t)C - 1] <= 1) || (X + 1 < W && Depth[(size_t)C + 1] <= 1) || (Y > 0 && Depth[(size_t)(C - W)] <= 1) || (Y + 1 < H && Depth[(size_t)(C + W)] <= 1)) Depth[(size_t)C] = 2;
	}
}

// ------------------------------------------------------------------------------------------------- the smooth surface

// relief.js sample: the base patch and the road fan, then the water carve on the owner's side of the node
void FRelief::Sample(double U, double V, int Owner, bool bNearWet)
{
	BaseSample(U, V, Owner); RoadSample(U, V, Owner, Out); Out[3] = Out[0]; Out[4] = Out[1]; Out[5] = Out[2];
	if (!bNearWet) return;
	const double InitialHeight = Out[0], InitialX = Out[1], InitialZ = Out[2];
	const int Qx = ClampI((int)std::floor(U + 0.5), 0, W), Qz = ClampI((int)std::floor(V + 0.5), 0, H);
	const double Tx = ClampD(U - Qx + 0.5, 0, 1), Tz = ClampD(V - Qz + 0.5, 0, 1);
	const double Ex = Ease(Tx), Ez = Ease(Tz), Dex = 6 * Tx * (1 - Tx) / Cell, Dez = 6 * Tz * (1 - Tz) / Cell;
	const int Ids[4] = { IdAt(Qx - 1, Qz - 1), IdAt(Qx, Qz - 1), IdAt(Qx - 1, Qz), IdAt(Qx, Qz) };
	const int Ox = Owner % W, Oz = Owner / W, Seed = (Ox >= Qx ? 1 : 0) + (Oz >= Qz ? 2 : 0);
	const int Bits = (Masks[(size_t)(Qz * QW + Qx)] >> (Seed * 4)) & 15;
	const double Weights[4] = { (1 - Ex) * (1 - Ez), Ex * (1 - Ez), (1 - Ex) * Ez, Ex * Ez };
	const double Dxs[4] = { -Dex * (1 - Ez), Dex * (1 - Ez), -Dex * Ez, Dex * Ez };
	const double Dzs[4] = { -(1 - Ex) * Dez, -Ex * Dez, (1 - Ex) * Dez, Ex * Dez };
	double Sum = 0, Sx = 0, Sz = 0, Height = InitialHeight, Hx = InitialX, Hz = InitialZ;
	double WetW = 0, Wx = 0, Wz = 0, Bed = 0, Bx = 0, Bz = 0, Bridge = 0, Gx = 0, Gz = 0;
	for (int K = 0; K < 4; K++) if (Bits & (1 << K))
	{
		const int C = Ids[K];
		const double A = Weights[K], Ax = Dxs[K], Az = Dzs[K];
		Sum += A; Sx += Ax; Sz += Az;
		if (Bridges[(size_t)C]) { Bridge += A; Gx += Ax; Gz += Az; }
		else if (Wet[(size_t)C])
		{
			const double Target = (double)WL[(size_t)C] + WaterLift - (Ch(C) == 'F' ? 0.15 : 0.42 + 0.38 * (Depth[(size_t)C] - 1));
			WetW += A; Wx += Ax; Wz += Az; Bed += A * Target; Bx += Ax * Target; Bz += Az * Target;
		}
	}
	if (WetW > 0.000001)
	{
		const double Target = Bed / WetW, TargetX = (Bx * WetW - Bed * Wx) / (WetW * WetW), TargetZ = (Bz * WetW - Bed * Wz) / (WetW * WetW);
		const double WetX = (Wx * Sum - WetW * Sx) / (Sum * Sum), WetZ = (Wz * Sum - WetW * Sz) / (Sum * Sum), Amount = WetW / Sum;
		const double BridgeX = (Gx * Sum - Bridge * Sx) / (Sum * Sum), BridgeZ = (Gz * Sum - Bridge * Sz) / (Sum * Sum);
		const double T = ClampD(Amount / 0.08, 0, 1), Bt = ClampD(Bridge / Sum / 0.45, 0, 1);
		const double Fade = 1 - Ease(Bt), Carving = Ease(T) * Fade;
		const double Kx = 6 * T * (1 - T) / 0.08 * WetX * Fade - Ease(T) * 6 * Bt * (1 - Bt) / 0.45 * BridgeX;
		const double Kz = 6 * T * (1 - T) / 0.08 * WetZ * Fade - Ease(T) * 6 * Bt * (1 - Bt) / 0.45 * BridgeZ;
		if (Target < Height) { Hx += (TargetX - Hx) * Carving + (Target - Height) * Kx; Hz += (TargetZ - Hz) * Carving + (Target - Height) * Kz; Height += (Target - Height) * Carving; }
	}
	Out[0] = Height; Out[1] = Hx; Out[2] = Hz;
}

// relief.js controlCell: the 3 x 3 control heights (centre, edge midpoints, corners) and the road corners
void FRelief::ControlCell(int C)
{
	const int X = C % W, Z = C / W, Off = C * 9;
	const int L = Levels[(size_t)C];
	Controls[(size_t)Off + 4] = (float)(L * LevelHeight);
	static const int Slot[4] = { 3, 5, 1, 7 };
	for (int Side = 0; Side < 4; Side++)
	{
		const int Other = IdAt(X + (Side == 0 ? -1 : Side == 1 ? 1 : 0), Z + (Side == 2 ? -1 : Side == 3 ? 1 : 0));
		Controls[(size_t)(Off + Slot[Side])] = (float)((Joined(C, Other) ? (L + Levels[(size_t)Other]) * 0.5 : (double)L) * LevelHeight);
	}
	for (int J = 0; J < 2; J++) for (int I = 0; I < 2; I++)
	{
		const int Qx = X + I, Qz = Z + J, Seed = (1 - I) + (1 - J) * 2;
		const int Bits = (Masks[(size_t)(Qz * QW + Qx)] >> (Seed * 4)) & 15;
		double Sum = 0, Road = 0;
		int Count = 0;
		bool bBuilding = false;
		for (int K = 0; K < 4; K++)
		{
			const int Id = IdAt(Qx - 1 + (K & 1), Qz - 1 + (K >> 1));
			Road += Roads[(size_t)Id] * 0.25;
			bBuilding = bBuilding || Ch(Id) == 'B';
			if (Bits & (1 << K)) { Sum += Levels[(size_t)Id]; Count++; }
		}
		Controls[(size_t)(Off + J * 6 + I * 2)] = (float)(Sum / Count * LevelHeight);
		RoadCorners[(size_t)(C * 4 + J * 2 + I)] = (float)(bBuilding ? 0 : Road);
	}
}

// relief.js tangentCell: each node's tangents from its connected owners, limited by both neighbouring secants
void FRelief::TangentCell(int C)
{
	const int X = C % W, Z = C / W;
	auto Limited = [](double A, double B) { return A * B > 0 ? Sign(A) * std::min(std::abs(A), std::abs(B)) : 0.0; };
	for (int J = 0; J < 3; J++) for (int I = 0; I < 3; I++)
	{
		const double Qx = X + I / 2.0, Qz = Z + J / 2.0, Value = Controls[(size_t)(C * 9 + J * 3 + I)];
		const bool bCorner = I != 1 && J != 1;
		const int Seed = (I == 0 ? 1 : 0) + (J == 0 ? 2 : 0);
		const int Bits = bCorner ? (Masks[(size_t)((int)Qz * QW + (int)Qx)] >> (Seed * 4)) & 15 : 0;
		double Left = 0, Right = 0, Top = 0, Bottom = 0;
		int Nl = 0, Nr = 0, Nt = 0, Nb = 0;
		for (int Oz = (int)std::ceil(Qz - 1); Oz <= (int)std::floor(Qz); Oz++) for (int Ox = (int)std::ceil(Qx - 1); Ox <= (int)std::floor(Qx); Ox++)
		{
			if (Ox < 0 || Oz < 0 || Ox >= W || Oz >= H) continue;
			const int Donor = Oz * W + Ox;
			if (bCorner ? !(Bits & (1 << ((Ox >= Qx ? 1 : 0) + (Oz >= Qz ? 2 : 0)))) : !Joined(C, Donor)) continue;
			const int Di = (int)JsRound((Qx - Ox) * 2), Dj = (int)JsRound((Qz - Oz) * 2), K = Donor * 9 + Dj * 3 + Di;
			if (Di > 0) { Left += Controls[(size_t)K - 1]; Nl++; }
			if (Di < 2) { Right += Controls[(size_t)K + 1]; Nr++; }
			if (Dj > 0) { Top += Controls[(size_t)K - 3]; Nt++; }
			if (Dj < 2) { Bottom += Controls[(size_t)K + 3]; Nb++; }
		}
		const size_t K2 = (size_t)(C * 9 + J * 3 + I) * 2;
		Tangents[K2] = (float)Limited(Nl ? Value - Left / Nl : 0, Nr ? Right / Nr - Value : 0);
		Tangents[K2 + 1] = (float)Limited(Nt ? Value - Top / Nt : 0, Nb ? Bottom / Nb - Value : 0);
	}
}

// relief.js baseSample: bicubic Hermite over the owner's 2 x 2 half-cell patches, with its derivatives
void FRelief::BaseSample(double U, double V, int Owner)
{
	const int X = Owner % W, Z = Owner / W;
	const double Fx = ClampD((U - X) * 2, 0, 2), Fz = ClampD((V - Z) * 2, 0, 2);
	const int I = std::min(1, (int)std::floor(Fx)), J = std::min(1, (int)std::floor(Fz));
	const double Tx = Fx - I, Tz = Fz - J;
	double Hx[4], Hz[4], Dhx[4], Dhz[4];
	Hermite(Tx, Hx, Dhx); Hermite(Tz, Hz, Dhz);
	Out[0] = Out[1] = Out[2] = 0;
	for (int B = 0; B < 2; B++) for (int A = 0; A < 2; A++)
	{
		const size_t K = (size_t)(Owner * 9 + (J + B) * 3 + I + A);
		const double Value = Controls[K], Dx = Tangents[K * 2], Dz = Tangents[K * 2 + 1];
		Out[0] += Value * Hx[A] * Hz[B] + Dx * Hx[A + 2] * Hz[B] + Dz * Hx[A] * Hz[B + 2];
		Out[1] += (Value * Dhx[A] * Hz[B] + Dx * Dhx[A + 2] * Hz[B] + Dz * Dhx[A] * Hz[B + 2]) * 2 / Cell;
		Out[2] += (Value * Hx[A] * Dhz[B] + Dx * Hx[A + 2] * Dhz[B] + Dz * Hx[A] * Dhz[B + 2]) * 2 / Cell;
	}
}

// relief.js roadSample: a shallow centre fan sinks roads 0.1 m and keeps house centres level
void FRelief::RoadSample(double U, double V, int Owner, double* Dst) const
{
	const double X = U - Owner % W - 0.5, Z = V - Owner / W - 0.5;
	const size_t K = (size_t)Owner * 4;
	const double Centre = Roads[(size_t)Owner];
	double A, B, Amount, Rx, Rz;
	if (std::abs(Z) >= std::abs(X))
	{
		A = RoadCorners[K + (Z < 0 ? 0 : 2)]; B = RoadCorners[K + (Z < 0 ? 1 : 3)];
		Amount = Centre + std::abs(Z) * (A + B - 2 * Centre) + X * (B - A);
		Rx = (B - A) / Cell; Rz = Sign(Z) * (A + B - 2 * Centre) / Cell;
	}
	else
	{
		A = RoadCorners[K + (X < 0 ? 0 : 1)]; B = RoadCorners[K + (X < 0 ? 2 : 3)];
		Amount = Centre + std::abs(X) * (A + B - 2 * Centre) + Z * (B - A);
		Rx = Sign(X) * (A + B - 2 * Centre) / Cell; Rz = (B - A) / Cell;
	}
	Dst[0] -= 0.1 * Amount; Dst[1] -= 0.1 * Rx; Dst[2] -= 0.1 * Rz;
}

// ------------------------------------------------------------------------------------------------- crater and rubble lips

// relief.js touch: the four cells around the nearest corner (unknown cells: no scar, and they read as the mean of the
// known ones, so they neither widen the span nor tilt the gradient)
FRelief::FTouch FRelief::Touch(double WU, double WV) const
{
	const int Ix = (int)JsRound(WU), Iz = (int)JsRound(WV);
	int Ls[4];
	bool bKnown[4];
	double KnownSum = 0;
	int KnownN = 0;
	for (int K = 0; K < 4; K++)
	{
		const int Id = IdAt(Ix - 1 + (K & 1), Iz - 1 + (K >> 1));
		Ls[K] = Levels[(size_t)Id]; bKnown[K] = Kn(Id);
		if (bKnown[K]) { KnownSum += Ls[K]; KnownN++; }
	}
	FTouch T;
	int MinL = 99, MaxL = -99;
	double Gx = 0, Gz = 0;
	bool bScar = false, bInside = true;
	for (int Dz = 0; Dz <= 1; Dz++) for (int Dx = 0; Dx <= 1; Dx++)
	{
		const int K = Dz * 2 + Dx, X = Ix - 1 + Dx, Z = Iz - 1 + Dz;
		const double L = bKnown[K] ? (double)Ls[K] : KnownN ? KnownSum / KnownN : 0.0;
		if (bKnown[K]) { MinL = std::min(MinL, Ls[K]); MaxL = std::max(MaxL, Ls[K]); }
		Gx += L * (Dx - 0.5); Gz += L * (Dz - 0.5);
		const bool bOnMap = X >= 0 && Z >= 0 && X < W && Z < H;
		const uint8_t Chr = bOnMap ? Ch(Z * W + X) : 0;
		const bool bDug = bOnMap && (Chr == '+' || Chr == 'R');
		if (bDug) bScar = true;
		if (!bDug) bInside = false;
	}
	if (!KnownN) { MinL = MaxL = 0; }
	T.Span = MaxL - MinL; T.bScar = bScar; T.Gx = Gx; T.Gz = Gz; T.MinL = MinL; T.bInside = bInside;
	return T;
}

// relief.js scarRun: the axis of a long scar (a bombing run) within 5 cells, false for a lone hit or a block
bool FRelief::ScarRun(double WU, double WV, double& Ax, double& Az) const
{
	const int Cx = (int)JsRound(WU - 0.5), Cz = (int)JsRound(WV - 0.5);
	int Num = 0;
	double Sx = 0, Sz = 0, Sxx = 0, Szz = 0, Sxz = 0;
	for (int Dz = -5; Dz <= 5; Dz++) for (int Dx = -5; Dx <= 5; Dx++)
	{
		const int X = Cx + Dx, Z = Cz + Dz;
		if (X < 0 || Z < 0 || X >= W || Z >= H) continue;
		const uint8_t Chr = Ch(Z * W + X);
		if (Chr != '+' && Chr != 'R') continue;
		const double Px = X + 0.5, Pz = Z + 0.5;
		Num++; Sx += Px; Sz += Pz; Sxx += Px * Px; Szz += Pz * Pz; Sxz += Px * Pz;
	}
	if (Num < 6) return false;
	const double Mx = Sx / Num, Mz = Sz / Num;
	const double Cxx = Sxx / Num - Mx * Mx, Czz = Szz / Num - Mz * Mz, Cxz = Sxz / Num - Mx * Mz;
	const double Tr = Cxx + Czz, Det = Cxx * Czz - Cxz * Cxz;
	const double Major = Tr * 0.5 + std::sqrt(std::max(0.0, Tr * Tr * 0.25 - Det));
	const double Minor = Tr - Major;
	if (!(Major > 1.6 && Minor < Major * 0.42)) return false;
	if (std::abs(Cxz) < 1e-6) { if (Cxx >= Czz) { Ax = 1; Az = 0; } else { Ax = 0; Az = 1; } }
	else { Ax = Cxz; Az = Major - Cxx; }
	if (Ax < 0 || (Ax == 0 && Az < 0)) { Ax = -Ax; Az = -Az; }
	double Len = Hypot2(Ax, Az);
	if (!(Len != 0)) Len = 1;
	Ax /= Len; Az /= Len;
	return true;
}

// relief.js lipShift: the sideways shove of a lip vertex (world position only, so shared vertices move together)
void FRelief::LipShift(double WU, double WV, double Shift[2]) const
{
	Shift[0] = Shift[1] = 0;
	const FTouch T = Touch(WU, WV);
	if (!T.bScar || (T.Span < 1 && !T.bInside)) return;
	double Ax = 0, Az = 0, Dx, Dz;
	if (ScarRun(WU, WV, Ax, Az))
	{
		const double Along = WU * Ax + WV * Az, Px = -Az, Pz = Ax;
		const double Side = std::max(-1.0, std::min(1.0, T.Gx * Px + T.Gz * Pz));
		const double Width = (FieldNoise(Along * 0.08 + 1.7, 4.2, 4) * 2 - 1) * 0.8;
		const double Sway = (FieldNoise(Along * 0.045 + 9.1, 1.3, 6) * 2 - 1) * 1.35;
		const double AlongJ = (FieldNoise(Along * 0.07 + 3.1, 8.8, 8) * 2 - 1) * 0.35;
		const double Nick = (FieldNoise(WU * 0.2 + 2.4, WV * 0.17 + 6.1, 9) * 2 - 1) * 0.7;
		double Glen = Hypot2(T.Gx, T.Gz);
		if (!(Glen != 0)) Glen = 1;
		Dx = Px * (Side * Width + Sway) + Ax * AlongJ + (T.Gx / Glen) * Nick;
		Dz = Pz * (Side * Width + Sway) + Az * AlongJ + (T.Gz / Glen) * Nick;
	}
	else
	{
		const double Radial = (FieldNoise(WU * 0.18 + 1.7, WV * 0.16 + 4.2, 2) * 2 - 1) * 0.65;
		const double Tangent = (FieldNoise(WU * 0.11 + 3.3, WV * 0.09 + 9.1, 8) * 2 - 1) * 0.22;
		double Glen = Hypot2(T.Gx, T.Gz);
		if (!(Glen != 0)) Glen = 1;
		const double Rx = T.Gx / Glen, Rz = T.Gz / Glen;
		Dx = Rx * Radial - Rz * Tangent;
		Dz = Rz * Radial + Rx * Tangent;
	}
	const double Mag = Hypot2(Dx, Dz), Cap = 2.1;
	if (Mag > Cap) { Dx *= Cap / Mag; Dz *= Cap / Mag; }
	Shift[0] = Dx; Shift[1] = Dz;
}

// relief.js chewScar: bite the high lip of a dug step, then redo the normals from the bitten nodes
void FRelief::ChewScar(int X, int Z, int Off)
{
	bool bMoved = false;
	for (int J = 0; J <= S; J++) for (int I = 0; I <= S; I++)
	{
		if (I == 2 || J == 2) continue;
		const double WU = X + (double)I / S, WV = Z + (double)J / S;
		const FTouch T = Touch(WU, WV);
		if (!T.bScar || T.Span < 1) continue;
		const double Here = Heights[(size_t)(Off + J * 5 + I)];
		const double Rise = (Here - T.MinL * LevelHeight) / (LevelHeight * std::max(1, T.Span));
		const double Bite = FieldNoise(WU * 0.23 + 4.2, WV * 0.19 + 1.1, 5);
		if (Rise > 0.45) Heights[(size_t)(Off + J * 5 + I)] = (float)(Here - std::min(1.15, (Rise - 0.45) * (0.7 + 1.5 * Bite)));
		bMoved = true;
	}
	if (bMoved) Torn[(size_t)(X + Z * W)] = 1;
	if (!bMoved) return;
	for (int J = 0; J <= S; J++) for (int I = 0; I <= S; I++)
	{
		const int I0 = std::max(0, I - 1), I1 = std::min(S, I + 1), J0 = std::max(0, J - 1), J1 = std::min(S, J + 1);
		const double Dx = ((double)Heights[(size_t)(Off + J * 5 + I1)] - Heights[(size_t)(Off + J * 5 + I0)]) / ((I1 - I0) * Cell / S);
		const double Dz = ((double)Heights[(size_t)(Off + J1 * 5 + I)] - Heights[(size_t)(Off + J0 * 5 + I)]) / ((J1 - J0) * Cell / S);
		const double Inv = 1 / Hypot3(Dx, 1, Dz);
		const size_t K = (size_t)(Off + J * 5 + I);
		Normals[K * 3] = (float)(-Dx * Inv); Normals[K * 3 + 1] = (float)Inv; Normals[K * 3 + 2] = (float)(-Dz * Inv);
	}
}

// the level an unknown cell reads as: the mean of its known neighbours, 0 when none
double FRelief::GuessLevel(int C) const
{
	const int X = C % W, Z = C / W;
	double Sum = 0;
	int Num = 0;
	for (int Dz = -1; Dz <= 1; Dz++) for (int Dx = -1; Dx <= 1; Dx++)
	{
		const int Xx = X + Dx, Zz = Z + Dz;
		if ((Dx || Dz) && Xx >= 0 && Zz >= 0 && Xx < W && Zz < H && Kn(Zz * W + Xx)) { Sum += Levels[(size_t)(Zz * W + Xx)]; Num++; }
	}
	return Num ? Sum / Num : 0.0;
}

// relief.js buildCell: the cell's 5 x 5 node heights and normals, its density and flags
void FRelief::BuildCell(int C)
{
	Ragged[(size_t)C] = 0;
	Shoved[(size_t)C] = 0;
	const int X = C % W, Z = C / W, Off = C * Stride;
	Torn[(size_t)C] = 0;
	auto FillFlat = [this, C, Off](double Value)
	{
		for (int K = Off; K < Off + Stride; K++) { Heights[(size_t)K] = (float)Value; Normals[(size_t)K * 3] = Normals[(size_t)K * 3 + 2] = 0.f; Normals[(size_t)K * 3 + 1] = 1.f; }
		NX[(size_t)C] = NZ[(size_t)C] = Flat[(size_t)C] = 1;
	};
	if (!Kn(C)) { FillFlat(GuessLevel(C) * LevelHeight); return; } // never drawn; a query reads it flat
	const bool bDug = Ch(C) == 'T';
	bool bConstant = !bDug, bNearWet = false, bNearRoad = false;
	const int Reference = Levels[(size_t)C];
	for (int J = -1; J <= 1; J++) for (int I = -1; I <= 1; I++)
	{
		const int K = IdAt(X + I, Z + J);
		if (Kn(K) && Levels[(size_t)K] != Reference) bConstant = false;
		if (Wet[(size_t)K]) bNearWet = true;
		if (Roads[(size_t)K]) bNearRoad = true;
	}
	if (bConstant && !bNearWet)
	{
		if (!bNearRoad) { FillFlat(Reference * LevelHeight); return; }
		for (int J = 0; J <= S; J++) for (int I = 0; I <= S; I++)
		{
			const size_t K = (size_t)(Off + J * 5 + I);
			Out[0] = Reference * LevelHeight; Out[1] = Out[2] = 0;
			RoadSample(X + (double)I / S, Z + (double)J / S, C, Out);
			Heights[K] = (float)Out[0];
			const double Inv = 1 / Hypot3(Out[1], 1, Out[2]);
			Normals[K * 3] = (float)(-Out[1] * Inv); Normals[K * 3 + 1] = (float)Inv; Normals[K * 3 + 2] = (float)(-Out[2] * Inv);
		}
		NX[(size_t)C] = NZ[(size_t)C] = 1;
		const float* Hh = &Heights[(size_t)Off];
		Flat[(size_t)C] = Hh[0] == Hh[12] && Hh[0] == Hh[4] && Hh[0] == Hh[20] && Hh[0] == Hh[24] ? 1 : 2;
		return;
	}
	bool bWetFlat = bConstant && !bNearRoad && Ch(C) == 'W';
	if (bWetFlat) for (int J = -1; J <= 1; J++) for (int I = -1; I <= 1; I++)
	{
		const int K = IdAt(X + I, Z + J);
		if (Ch(K) != 'W' || Depth[(size_t)K] != Depth[(size_t)C] || !(WL[(size_t)K] == WL[(size_t)C])) bWetFlat = false;
	}
	if (bWetFlat)
	{
		const double Target = (double)WL[(size_t)C] + WaterLift - 0.42 - 0.38 * (Depth[(size_t)C] - 1);
		FillFlat(std::min(Reference * LevelHeight, Target));
		return;
	}
	float BaseValues[25], BaseDx[25], BaseDz[25], WaterDx[9], WaterDz[9];
	for (int J = 0; J <= S; J++) for (int I = 0; I <= S; I++)
	{
		Sample(X + (double)I / S, Z + (double)J / S, C, bNearWet);
		const size_t K = (size_t)(Off + J * 5 + I);
		const double Inv = 1 / Hypot3(Out[1], 1, Out[2]);
		Heights[K] = (float)Out[0]; BaseValues[J * 5 + I] = (float)Out[3]; BaseDx[J * 5 + I] = (float)Out[4]; BaseDz[J * 5 + I] = (float)Out[5];
		Normals[K * 3] = (float)(-Out[1] * Inv); Normals[K * 3 + 1] = (float)Inv; Normals[K * 3 + 2] = (float)(-Out[2] * Inv);
	}
	if (bNearWet)
	{
		// water displacement uses a shared half-cell lattice
		auto Delta = [&](int I, int J) { return (double)Heights[(size_t)(Off + J * 10 + I * 2)] - BaseValues[J * 10 + I * 2]; };
		for (int J = 0; J < 3; J++) for (int I = 0; I < 3; I++)
		{
			const int Left = std::max(0, I - 1), Right = std::min(2, I + 1), Top = std::max(0, J - 1), Bottom = std::min(2, J + 1);
			WaterDx[J * 3 + I] = (float)((Delta(Right, J) - Delta(Left, J)) / ((Right - Left) * Cell / 2));
			WaterDz[J * 3 + I] = (float)((Delta(I, Bottom) - Delta(I, Top)) / ((Bottom - Top) * Cell / 2));
		}
		for (int J = 0; J <= S; J++) for (int I = 0; I <= S; I++)
		{
			const int Ix = std::min(1, I / 2), Iz = std::min(1, J / 2);
			const double Tx = I / 2.0 - Ix, Tz = J / 2.0 - Iz;
			const int A = Iz * 10 + Ix * 2;
			if ((I & 1) || (J & 1))
			{
				const double Av = (double)Heights[(size_t)(Off + A)] - BaseValues[A], Bv = (double)Heights[(size_t)(Off + A + 2)] - BaseValues[A + 2];
				const double Cv = (double)Heights[(size_t)(Off + A + 10)] - BaseValues[A + 10], Dv = (double)Heights[(size_t)(Off + A + 12)] - BaseValues[A + 12];
				Heights[(size_t)(Off + J * 5 + I)] = (float)(BaseValues[J * 5 + I] + (Tx + Tz <= 1 ? Av + (Bv - Av) * Tx + (Cv - Av) * Tz : Dv + (Cv - Dv) * (1 - Tx) + (Bv - Dv) * (1 - Tz)));
			}
			const int K = Iz * 3 + Ix;
			const size_t Node = (size_t)(Off + J * 5 + I);
			const double Dx = BaseDx[J * 5 + I] + ((double)WaterDx[K] * (1 - Tx) + WaterDx[K + 1] * Tx) * (1 - Tz) + ((double)WaterDx[K + 3] * (1 - Tx) + WaterDx[K + 4] * Tx) * Tz;
			const double Dz = BaseDz[J * 5 + I] + ((double)WaterDz[K] * (1 - Tx) + WaterDz[K + 1] * Tx) * (1 - Tz) + ((double)WaterDz[K + 3] * (1 - Tx) + WaterDz[K + 4] * Tx) * Tz;
			const double Inv = 1 / Hypot3(Dx, 1, Dz);
			Normals[Node * 3] = (float)(-Dx * Inv); Normals[Node * 3 + 1] = (float)Inv; Normals[Node * 3 + 2] = (float)(-Dz * Inv);
		}
	}
	else ChewScar(X, Z, Off);
	if (bDug)
	{
		auto Tr = [&](int Dx, int Dz) { const int Xx = X + Dx, Zz = Z + Dz; return Xx >= 0 && Zz >= 0 && Xx < W && Zz < H && Ch(Zz * W + Xx) == 'T'; };
		const bool W0 = Tr(-1, 0), E = Tr(1, 0), N0 = Tr(0, -1), So = Tr(0, 1);
		for (int J = 0; J <= S; J++) for (int I = 0; I <= S; I++)
		{
			const bool bInI = I > 0 && I < S, bInJ = J > 0 && J < S;
			if ((bInI && bInJ) || (bInJ && ((I == 0 && W0) || (I == S && E))) || (bInI && ((J == 0 && N0) || (J == S && So))))
				Heights[(size_t)(Off + J * 5 + I)] = (float)((double)Heights[(size_t)(Off + J * 5 + I)] - TrenchDepth);
		}
		auto Hn = [&](int I, int J) { return (double)Heights[(size_t)(Off + ClampI(J, 0, S) * 5 + ClampI(I, 0, S))]; };
		const double Step = Cell / S;
		for (int J = 0; J <= S; J++) for (int I = 0; I <= S; I++)
		{
			const size_t K = (size_t)(Off + J * 5 + I);
			const double Dx = (Hn(I + 1, J) - Hn(I - 1, J)) / (Step * (std::min(S, I + 1) - std::max(0, I - 1)));
			const double Dz = (Hn(I, J + 1) - Hn(I, J - 1)) / (Step * (std::min(S, J + 1) - std::max(0, J - 1)));
			const double Inv = 1 / Hypot3(Dx, 1, Dz);
			Normals[K * 3] = (float)(-Dx * Inv); Normals[K * 3 + 1] = (float)Inv; Normals[K * 3 + 2] = (float)(-Dz * Inv);
		}
	}
	int Cx = 1, Cz = 1;
	bool bIsFlat = true;
	const float* Hh = &Heights[(size_t)Off];
	for (int J = 0; J <= S; J++) for (int I = 0; I <= S; I++)
	{
		const double Value = Hh[J * 5 + I];
		if (std::abs(Value - Hh[0]) > 0.00001) bIsFlat = false;
		const double Ax = Hh[J * 5], Bx = Hh[J * 5 + 2], Ex = Hh[J * 5 + 4];
		const double Az = Hh[I], Bz = Hh[10 + I], Ez = Hh[20 + I];
		if (std::abs(Bx - (Ax + Ex) * 0.5) > 0.004) Cx = std::max(Cx, 2);
		if (std::abs(Bz - (Az + Ez) * 0.5) > 0.004) Cz = std::max(Cz, 2);
		if (std::abs(Value - (I < 2 ? Ax + (Bx - Ax) * I / 2 : Bx + (Ex - Bx) * (I - 2) / 2)) > 0.000001) Cx = 4;
		if (std::abs(Value - (J < 2 ? Az + (Bz - Az) * J / 2 : Bz + (Ez - Bz) * (J - 2) / 2)) > 0.000001) Cz = 4;
	}
	if (!bIsFlat)
	{
		if (Cx == 1 && std::abs((double)Hh[0] - Hh[4]) > 0.00001) Cx = 2;
		if (Cz == 1 && std::abs((double)Hh[0] - Hh[20]) > 0.00001) Cz = 2;
	}
	if (bConstant && bNearWet) { Cx = std::min(Cx, 2); Cz = std::min(Cz, 2); }
	const bool bCliffX = (X > 0 && Cliff(C, C - 1)) || (X + 1 < W && Cliff(C, C + 1));
	const bool bCliffZ = (Z > 0 && Cliff(C, C - W)) || (Z + 1 < H && Cliff(C, C + W));
	if (bCliffX) Cx = 4;
	if (bCliffZ) Cz = 4;
	bool bRim = false;
	if (N <= 65536)
		for (int K = 0; K < 4 && !bRim; K++)
		{
			const int Qx = X + (K & 1), Qz = Z + (K >> 1);
			int Lo = 99, Hi = -99;
			for (int Q = 0; Q < 4; Q++)
			{
				const int Id = IdAt(Qx - 1 + (Q & 1), Qz - 1 + (Q >> 1));
				if (!Kn(Id)) continue;
				Lo = std::min(Lo, (int)Levels[(size_t)Id]); Hi = std::max(Hi, (int)Levels[(size_t)Id]);
			}
			bRim = Hi - Lo >= 2;
		}
	if (bRim) Cx = Cz = 4;
	Ragged[(size_t)C] = bRim ? 1 : 0;
	NX[(size_t)C] = (uint8_t)Cx; NZ[(size_t)C] = (uint8_t)Cz;
	Flat[(size_t)C] = bIsFlat && !bCliffX && !bCliffZ && !bRim ? 1 : 0;
	if (Cx < S || Cz < S)
		for (int J = 0; J <= S; J++) for (int I = 0; I <= S; I++)
		{
			const int StepX = S / Cx, StepZ = S / Cz;
			if (I % StepX == 0 && J % StepZ == 0) continue;
			const int Ix = std::min(Cx - 1, (int)std::floor((double)I / StepX)), Iz = std::min(Cz - 1, (int)std::floor((double)J / StepZ));
			const double Tx = (double)I / StepX - Ix, Tz = (double)J / StepZ - Iz;
			const int A = Off + Iz * StepZ * 5 + Ix * StepX;
			const double Av = Heights[(size_t)A], Bv = Heights[(size_t)(A + StepX)], Cv = Heights[(size_t)(A + StepZ * 5)], Dv = Heights[(size_t)(A + StepZ * 5 + StepX)];
			Heights[(size_t)(Off + J * 5 + I)] = (float)(Tx + Tz <= 1 ? Av + (Bv - Av) * Tx + (Cv - Av) * Tz : Dv + (Cv - Dv) * (1 - Tx) + (Bv - Dv) * (1 - Tz));
		}
}

// ------------------------------------------------------------------------------------------------- cliff edges

// relief.js cornerShift: the bevel of a corner on a cliff and the outward direction of the face there
void FRelief::CornerShift(int X, int Z, double V[4]) const
{
	X = ClampI(X, 0, W); Z = ClampI(Z, 0, H);
	double Dx = 0, Dz = 0;
	if (X > 0 && X < W) for (int Y = Z - 1; Y <= Z; Y++) if (Y >= 0 && Y < H)
	{
		const int A = Y * W + X - 1, B = Y * W + X;
		if (Cliff(A, B)) Dx += Sign(Levels[(size_t)A] - Levels[(size_t)B]);
	}
	if (Z > 0 && Z < H) for (int I = X - 1; I <= X; I++) if (I >= 0 && I < W)
	{
		const int A = (Z - 1) * W + I, B = Z * W + I;
		if (Cliff(A, B)) Dz += Sign(Levels[(size_t)A] - Levels[(size_t)B]);
	}
	const double Len = Hypot2(Dx, Dz), Bevel = Dx != 0 && Dz != 0 ? 0.2 : 0.08 + FieldNoise(X * 0.6, Z * 0.6, 107) * 0.12;
	int Bottom = 99, Count = 0, Ls[4];
	bool bK[4];
	for (int Q = 0; Q < 4; Q++)
	{
		const int Id = IdAt(X - 1 + (Q & 1), Z - 1 + (Q >> 1));
		Ls[Q] = Levels[(size_t)Id]; bK[Q] = Kn(Id);
		if (bK[Q]) Bottom = std::min(Bottom, Ls[Q]);
	}
	for (int Q = 0; Q < 4; Q++) if (bK[Q] && Ls[Q] >= Bottom + 2) Count++;
	const bool bConcave = Dx != 0 && Dz != 0 && Count == 3;
	const double Direction = bConcave ? 1 : -1;
	if (Len != 0) { V[0] = Direction * Dx / Len * Bevel; V[1] = Direction * Dz / Len * Bevel; V[2] = Dx / Len; V[3] = Dz / Len; }
	else V[0] = V[1] = V[2] = V[3] = 0;
}

// relief.js edgeShift: the lip shove plus the corner bevels carried through the cell
void FRelief::EdgeShift(double U, double V, double Shift[2]) const
{
	LipShift(U, V, Shift);
	if (N > 65536) return;
	const int X = (int)std::floor(U), Z = (int)std::floor(V);
	const double Tx = U - X, Tz = V - Z;
	double Corners[4][4];
	CornerShift(X, Z, Corners[0]); CornerShift(X + 1, Z, Corners[1]); CornerShift(X, Z + 1, Corners[2]); CornerShift(X + 1, Z + 1, Corners[3]);
	for (int I = 0; I < 4; I++)
	{
		const double Weight = ((I & 1) ? Tx : 1 - Tx) * ((I & 2) ? Tz : 1 - Tz);
		Shift[0] += Corners[I][0] * Weight; Shift[1] += Corners[I][1] * Weight;
	}
}

// relief.js facePoint's mixed: more than two heights meet at the corner
bool FRelief::Mixed(int CX, int CZ) const
{
	int Seen[4], Num = 0;
	for (int Q = 0; Q < 4; Q++)
	{
		const int Id = IdAt(CX - 1 + (Q & 1), CZ - 1 + (Q >> 1));
		if (!Kn(Id)) continue;
		const int L = Levels[(size_t)Id];
		bool bNew = true;
		for (int K = 0; K < Num; K++) if (Seen[K] == L) bNew = false;
		if (bNew) Seen[Num++] = L;
	}
	return Num > 2;
}

namespace ReliefPlan
{
	struct FWall { int C, Axis, K, A0, A1, B0, B1, Bands; std::vector<FRock> Rocks; };
	struct FFiller { double Cx, Cz, Ox, Oz, FootY, TopY; std::vector<FRock> Rocks; };
}
using namespace ReliefPlan;

// relief.js rockPlan: strata of fractured slabs on each half of a cliff edge, crest boulders, scree at the foot
static std::vector<FRock> TCReliefRockPlan(int X, int Z, int Axis, int K, double Rise, bool bLow)
{
	std::vector<FRock> Rocks;
	if (Rise < 1.5) return Rocks;
	const std::vector<std::vector<double>>& Models = RockModels(bLow ? 0 : 1);
	const std::vector<std::vector<double>>& Coarse = RockModels(0);
	auto R = [&](double Salt) { return Hash(X * 4 + K + Axis * 0.5, Z * 4 + Salt * 0.37, 157 + Salt); };
	if (K % 2 == 0)
	{
		const int Layers = R(40) > 0.72 ? 1 : std::max(2, (int)JsRound(Rise / 3));
		for (int Layer = 0; Layer < Layers; Layer++)
		{
			FRock Rk;
			const double T = (Layer + 0.3 + R(Layer + 13) * 0.4) / Layers;
			Rk.Model = &Models[(size_t)std::floor(R(Layer) * 6)]; Rk.Along = 1 + (R(Layer + 9) - 0.5) * 0.8; Rk.T = T;
			Rk.Size[0] = 0.75 + R(Layer + 3) * 0.45; Rk.Size[1] = Rise / Layers * (0.55 + R(Layer + 5) * 0.25); Rk.Size[2] = 0.45 + R(Layer + 7) * 0.25;
			Rk.Out = 0.4 - 0.95 * T; Rk.Yaw = (R(Layer + 11) - 0.5) * 0.5;
			Rocks.push_back(Rk);
		}
		if (R(30) > 0.2)
		{
			FRock Rk;
			const double Size = 0.35 + R(31) * 0.3;
			Rk.Model = &Coarse[(size_t)std::floor(R(32) * 6)]; Rk.Along = R(33) * 2; Rk.T = 1;
			Rk.Size[0] = Size * 1.4; Rk.Size[1] = Size; Rk.Size[2] = Size;
			Rk.Out = -0.15 - R(34) * 0.25; Rk.Yaw = R(35) * 3; Rk.Crest = 0.1 + R(36) * 0.25;
			Rocks.push_back(Rk);
		}
	}
	if (R(20) > 0.4)
	{
		FRock Rk;
		const double Size = 0.14 + R(21) * 0.18;
		Rk.Model = &Coarse[(size_t)std::floor(R(22) * 6)]; Rk.Along = R(23); Rk.T = 0;
		Rk.Size[0] = Size * 1.3; Rk.Size[1] = Size; Rk.Size[2] = Size;
		Rk.Out = 0.45 + R(24) * 0.25; Rk.Yaw = R(25) * 3; Rk.bLoose = true;
		Rocks.push_back(Rk);
	}
	return Rocks;
}

	// relief.js planRow: the walls of a row's cliff edges (cells X0..X1-1)
	void FRelief::WallsOf(int Z, int X0, int X1, int& CliffEdges, std::vector<ReliefPlan::FWall>& OutWalls) const
	{
		const int Nn = N;
		for (int X = X0; X < X1; X++)
		{
			const int C = Z * W + X;
			for (int Axis = 0; Axis < 2; Axis++)
			{
				if ((Axis == 0 && X + 1 >= W) || (Axis == 1 && Z + 1 >= H)) continue;
				const int D = C + (Axis == 0 ? 1 : W);
				if (!Cliff(C, D)) continue;
				CliffEdges++;
				for (int K = 0; K < S; K++)
				{
					const int A0 = C * Stride + (Axis == 0 ? K * 5 + 4 : 20 + K), A1 = A0 + (Axis == 0 ? 5 : 1);
					const int B0 = D * Stride + (Axis == 0 ? K * 5 : K), B1 = B0 + (Axis == 0 ? 5 : 1);
					const double DA = std::abs((double)Heights[(size_t)A0] - Heights[(size_t)B0]), DB = std::abs((double)Heights[(size_t)A1] - Heights[(size_t)B1]);
					if (DA < 0.00001 && DB < 0.00001) continue;
					const int Bands = Nn > 65536 ? 1 : bLow ? 2 : 4;
					const double Rise = std::min(DA, DB);
					FWall Wl{ C, Axis, K, A0, A1, B0, B1, Bands, {} };
					if (Nn <= 65536) Wl.Rocks = TCReliefRockPlan(X, Z, Axis, K, Rise, bLow);
					OutWalls.push_back(std::move(Wl));
				}
			}
		}
	}

// ------------------------------------------------------------------------------------------------- mesh

namespace ReliefEmit
{
	struct FWriter
	{
		FPart& P;
		int W, H;
		int Owner = 0;
		uint32_t Vertex(double X, double Z, double Height, double Nx, double Ny, double Nz, double Rock = 0, double Lip = 0, double Foot = 0, double Damp = 0, double Dug = 0, uint8_t Detail = 0)
		{
			const uint32_t K = (uint32_t)(P.Pos.size() / 3);
			P.Pos.insert(P.Pos.end(), { (float)X, (float)Height, (float)Z });
			P.Nrm.insert(P.Nrm.end(), { (float)Nx, (float)Ny, (float)Nz });
			P.Uv.insert(P.Uv.end(), { (float)(X / (W * Cell)), (float)(1 - Z / (H * Cell)) });
			P.Paint.insert(P.Paint.end(), { (float)Rock, (float)Lip, (float)Foot, (float)Damp });
			P.Scar.push_back((float)Dug);
			P.Owner.push_back(Owner);
			P.Detail.push_back(Detail);
			return K;
		}
		void Tri(uint32_t A, uint32_t B, uint32_t C) { P.Idx.insert(P.Idx.end(), { A, B, C }); }
		// relief.js rockTri: one facet with its own normal
		void RockTri(const double* A, const double* B, const double* C, double Top, double Foot, double Damp, bool bReverse = false, double Stone = 1)
		{
			if (bReverse) std::swap(B, C);
			const double Ux = B[0] - A[0], Uy = B[1] - A[1], Uz = B[2] - A[2];
			const double Vx = C[0] - A[0], Vy = C[1] - A[1], Vz = C[2] - A[2];
			const double Nx = Uy * Vz - Uz * Vy, Ny = Uz * Vx - Ux * Vz, Nz = Ux * Vy - Uy * Vx;
			const double Length = Hypot3(Nx, Ny, Nz);
			if (Length < 1e-9) return;
			const uint32_t Start = (uint32_t)(P.Pos.size() / 3);
			for (const double* Pt : { A, B, C }) Vertex(Pt[0], Pt[2], Pt[1], Nx / Length, Ny / Length, Nz / Length, Stone, Top, Foot, Damp);
			Tri(Start, Start + 1, Start + 2);
		}
		// relief.js placeRock: x along the wall, y up, z out toward the low ground
		void PlaceRock(const FRock& Rk, double Wx, double Wz, double FootY, double TopY, double Ox, double Oz, double Damp)
		{
			const double Sa = Rk.Size[0], Su = Rk.Size[1], So = Rk.Size[2], Cs = std::cos(Rk.Yaw), Sn = std::sin(Rk.Yaw), Tx = Oz, Tz = -Ox;
			const double Cx = Wx + Ox * Rk.Out, Cz = Wz + Oz * Rk.Out;
			const double Cy = Rk.bLoose ? FootY + Su * 0.3 : Rk.Crest != 0 ? TopY + Rk.Crest - Su : FootY + (TopY - FootY) * Rk.T;
			const double Limit = TopY + (Rk.Crest != 0 ? Rk.Crest : Rk.Lift != 0 ? Rk.Lift : -0.03);
			const std::vector<double>& M = *Rk.Model;
			std::vector<double> Pts(M.size());
			for (size_t I = 0; I < M.size(); I += 3)
			{
				const double Lx = M[I], Ly = M[I + 1], Lz = M[I + 2];
				const double Ax = (Lx * Cs - Lz * Sn) * Sa, Az = (Lx * Sn + Lz * Cs) * So;
				Pts[I] = Cx + Tx * Ax + Ox * Az; Pts[I + 1] = std::min(Limit, Cy + Ly * Su); Pts[I + 2] = Cz + Tz * Ax + Oz * Az;
			}
			for (size_t I = 0; I < Pts.size(); I += 9) RockTri(&Pts[I], &Pts[I + 3], &Pts[I + 6], TopY + 2, FootY, Damp, false, 0.98);
		}
	};
}
using namespace ReliefEmit;

void FRelief::PlanRow(int Z)
{
	FRowPlan& Row = Rows[(size_t)Z];
	Row.Rough.clear();
	int Triangles = 0;
	for (int X = 0; X < W; X++)
	{
		const int C = Z * W + X;
		Fan[(size_t)C] = 0;
		if (!Kn(C)) continue;
		if (Flat[(size_t)C] == 1)
		{
			int End = X + 1;
			while (End < W && End - X < 32 && Kn(Z * W + End) && Flat[(size_t)(Z * W + End)] == 1 && Heights[(size_t)(Z * W + End) * Stride] == Heights[(size_t)C * Stride]) End++;
			Triangles += 2; X = End - 1;
		}
		else if (Flat[(size_t)C] == 2) Triangles += 4;
		else { Triangles += NX[(size_t)C] * NZ[(size_t)C] * 2; Row.Rough.push_back(C); }
	}
	int CliffEdges = 0;
	std::vector<FWall> WallList;
	WallsOf(Z, 0, W, CliffEdges, WallList);
	for (const FWall& Wl : WallList)
	{
		Triangles += Wl.Bands * 2;
		for (const FRock& Rk : Wl.Rocks) Triangles += (int)(Rk.Model->size() / 9);
	}
	if (N <= 65536 && Z >= 1)
		for (int X = 1; X < W; X++)
		{
			// counted as relief.js does: the filler rocks' corners / 3
			int Quad[4] = { Levels[(size_t)((Z - 1) * W + X - 1)], Levels[(size_t)((Z - 1) * W + X)], Levels[(size_t)(Z * W + X - 1)], Levels[(size_t)(Z * W + X)] };
			if (!Kn((Z - 1) * W + X - 1) || !Kn((Z - 1) * W + X) || !Kn(Z * W + X - 1) || !Kn(Z * W + X)) continue;
			const int Bottom = std::min(std::min(Quad[0], Quad[1]), std::min(Quad[2], Quad[3]));
			int HighN = 0, Top = 1 << 20;
			for (int Q = 0; Q < 4; Q++) if (Quad[Q] >= Bottom + 2) { HighN++; Top = std::min(Top, Quad[Q]); }
			const bool bNotch = HighN == 3;
			bool bSkip = !bNotch && HighN != 1;
			for (int Q = 0; Q < 4; Q++) if (Quad[Q] < Bottom + 2 && Quad[Q] > Top - 2) bSkip = true;
			if (bSkip) continue;
			const double Rise = (Top - Bottom) * LevelHeight;
			const int Layers = std::max(1, (int)JsRound(Rise / 3.2));
			for (int Layer = 0; Layer < Layers; Layer++)
			{
				const double R0 = Hash(X * 2 + Layer, Z * 2 + 0 * 0.37, 211 + 0);
				Triangles += (int)(RockModels(bLow ? 0 : 1)[(size_t)std::floor(R0 * 6)].size() / 9);
			}
		}
	Row.Triangles = Triangles;
	Row.CliffEdges = CliffEdges;
}

// relief.js rebuildGeometry's fan budget: centre fans go to the first rough runs in map order while the budget lasts.
// Parts whose fans change are marked in Dirty.
void FRelief::Fans()
{
	long long Total = 0;
	for (const FRowPlan& Row : Rows) Total += Row.Triangles;
	long long Extra = std::max(0LL, (long long)std::min(150000, (int)std::floor(N * 5.0)) - Total);
	for (const FRowPlan& Row : Rows)
		for (const int C : Row.Rough)
		{
			const int Xs = NX[(size_t)C], Zs = NZ[(size_t)C], Cost = Xs * Zs * 2;
			const bool bFan = !bLow && !(Flat[(size_t)C] || Torn[(size_t)C] || (Xs < 4 && Zs < 4) || std::isfinite(WL[(size_t)C]) || Cost > Extra);
			if (bFan) Extra -= Cost;
			if ((Fan[(size_t)C] != 0) != bFan) { Fan[(size_t)C] = bFan ? 1 : 0; Rebuilt.push_back(-1 - PartOfCell(C)); }
		}
}

void FRelief::EmitPart(int PartIndex)
{
	FPart& P = Parts[(size_t)PartIndex];
	P.Clear();
	int Z0, Z1, X0, X1;
	if (ChunkCells > 0) { Z0 = (PartIndex / PartsX) * ChunkCells; Z1 = std::min(H, Z0 + ChunkCells); X0 = (PartIndex % PartsX) * ChunkCells; X1 = std::min(W, X0 + ChunkCells); }
	else { Z0 = PartIndex; Z1 = PartIndex + 1; X0 = 0; X1 = W; }
	FWriter Wr{ P, W, H };
	double Sh[2];
	// relief.js surfaceVertex
	auto SurfaceVertex = [&](int C, double U, double V, const double* HeightOverride) -> uint32_t
	{
		const int X = C % W, Z = C / W;
		const size_t Node = (size_t)(C * Stride) + (size_t)(JsRound(V * S) * 5 + JsRound(U * S));
		EdgeShift(X + U, Z + V, Sh);
		if (Ragged[(size_t)C] && U == 0.5 && V == 0.5) Sh[0] = Sh[1] = 0;
		const double Sx = Sh[0], Sz = Sh[1];
		const FTouch Dug = Touch(X + U, Z + V);
		const double ScarHere = Dug.bScar && (Dug.Span >= 1 || Dug.bInside) ? 1 : 0;
		double Lip = 0, Foot = 0;
		for (int Side = 0; Side < 4; Side++)
		{
			const int Other = Side == 0 ? (X > 0 ? C - 1 : C) : Side == 1 ? (X + 1 < W ? C + 1 : C) : Side == 2 ? (Z > 0 ? C - W : C) : (Z + 1 < H ? C + W : C);
			if (!Cliff(C, Other)) continue;
			const double Distance = Side == 0 ? U : Side == 1 ? 1 - U : Side == 2 ? V : 1 - V;
			const double Edge = std::max(0.0, 1 - Distance * Cell / 0.32);
			const double FootEdge = std::max(0.0, 1 - Distance * Cell / 0.75);
			if (Levels[(size_t)C] > Levels[(size_t)Other]) Lip = std::max(Lip, Edge * 0.65); else Foot = std::max(Foot, FootEdge * 0.35);
		}
		const uint8_t Detail = Ragged[(size_t)C];
		if (Sx != 0 || Sz != 0) Shoved[(size_t)C] = 1;
		Wr.Owner = C;
		if (HeightOverride)
		{
			const double Fx = U * S, Fz = V * S;
			const int I = std::min(3, (int)std::floor(Fx)), J = std::min(3, (int)std::floor(Fz));
			const double Tx = Fx - I, Tz = Fz - J;
			const int A = C * Stride + J * 5 + I;
			double Vx = 0, Vy = 0, Vz = 0;
			for (int K = 0; K < 4; K++)
			{
				const size_t Nk = (size_t)(A + (K & 1) + (K >> 1) * 5) * 3;
				const double Weight = ((K & 1) ? Tx : 1 - Tx) * ((K >> 1) ? Tz : 1 - Tz);
				Vx += Normals[Nk] * Weight; Vy += Normals[Nk + 1] * Weight; Vz += Normals[Nk + 2] * Weight;
			}
			const double Inv = 1 / Hypot3(Vx, Vy, Vz);
			return Wr.Vertex((X + U) * Cell + Sx, (Z + V) * Cell + Sz, *HeightOverride, Vx * Inv, Vy * Inv, Vz * Inv, 0, Lip, Foot, Wet[(size_t)C] ? 1 : 0, ScarHere, Detail);
		}
		return Wr.Vertex((X + U) * Cell + Sx, (Z + V) * Cell + Sz, Heights[Node], Normals[Node * 3], Normals[Node * 3 + 1], Normals[Node * 3 + 2], 0, Lip, Foot, Wet[(size_t)C] ? 1 : 0, ScarHere, Detail);
	};
	for (int Z = Z0; Z < Z1; Z++)
	{
		// runs (relief.js planRow's runs; in chunk layout a flat run stops at the chunk edge)
		for (int X = X0; X < X1; X++)
		{
			const int C = Z * W + X;
			if (!Kn(C)) continue;
			const uint32_t Start = (uint32_t)(P.Pos.size() / 3);
			if (Shoved[(size_t)C]) Shoved[(size_t)C] = 0;
			if (Flat[(size_t)C] == 1)
			{
				int End = X + 1;
				while (bMergeFlatRuns && End < X1 && End - X < 32 && Kn(Z * W + End) && Flat[(size_t)(Z * W + End)] == 1 && Heights[(size_t)(Z * W + End) * Stride] == Heights[(size_t)C * Stride]) End++;
				const double Xm = X * Cell, Zm = Z * Cell, Height = Heights[(size_t)C * Stride], Width = End - X;
				Wr.Owner = C;
				Wr.Vertex(Xm, Zm, Height, 0, 1, 0); Wr.Vertex(Xm + Width * Cell, Zm, Height, 0, 1, 0); Wr.Vertex(Xm, Zm + Cell, Height, 0, 1, 0); Wr.Vertex(Xm + Width * Cell, Zm + Cell, Height, 0, 1, 0);
				Wr.Tri(Start, Start + 2, Start + 1); Wr.Tri(Start + 1, Start + 2, Start + 3);
				X = End - 1;
				continue;
			}
			if (Flat[(size_t)C] == 2)
			{
				SurfaceVertex(C, 0, 0, nullptr); SurfaceVertex(C, 1, 0, nullptr); SurfaceVertex(C, 0, 1, nullptr); SurfaceVertex(C, 1, 1, nullptr); SurfaceVertex(C, 0.5, 0.5, nullptr);
				Wr.Tri(Start, Start + 4, Start + 1); Wr.Tri(Start + 1, Start + 4, Start + 3); Wr.Tri(Start + 3, Start + 4, Start + 2); Wr.Tri(Start + 2, Start + 4, Start);
			}
			else
			{
				const int Xs = NX[(size_t)C], Zs = NZ[(size_t)C];
				const bool bFan = Fan[(size_t)C] != 0;
				for (int J = 0; J <= Zs; J++) for (int I = 0; I <= Xs; I++) SurfaceVertex(C, (double)I / Xs, (double)J / Zs, nullptr);
				for (int J = 0; J < Zs; J++) for (int I = 0; I < Xs; I++)
				{
					const uint32_t A = Start + (uint32_t)(J * (Xs + 1) + I), B = A + 1, Cc = A + (uint32_t)Xs + 1, D = Cc + 1;
					if (bFan)
					{
						const double Mid = ((double)P.Pos[(size_t)B * 3 + 1] + P.Pos[(size_t)Cc * 3 + 1]) * 0.5;
						const uint32_t M = SurfaceVertex(C, (I + 0.5) / Xs, (J + 0.5) / Zs, &Mid);
						Wr.Tri(A, Cc, M); Wr.Tri(Cc, D, M); Wr.Tri(D, B, M); Wr.Tri(B, A, M);
					}
					else { Wr.Tri(A, Cc, B); Wr.Tri(B, Cc, D); }
				}
			}
			// Drawn picks: a cell a lip shoved answers ground queries from its own triangles
			if (bDrawnPicks && Shoved[(size_t)C]) for (uint32_t V = Start; V < (uint32_t)P.Detail.size(); V++) P.Detail[V] = 1;
		}
		// walls
		int CliffEdges = 0;
		std::vector<FWall> WallList;
		WallsOf(Z, X0, X1, CliffEdges, WallList);
		for (const FWall& Wl : WallList)
		{
			const int C = Wl.C, Axis = Wl.Axis, K = Wl.K, X = C % W, D = C + (Axis == 0 ? 1 : W);
			const double SignV = Levels[(size_t)C] > Levels[(size_t)D] ? 1 : -1;
			const double P0x = (X + (Axis == 0 ? 1 : (double)K / S)) * Cell, P0z = (Z + (Axis == 0 ? (double)K / S : 1)) * Cell;
			const double P1x = P0x + (Axis == 0 ? 0 : Cell / S), P1z = P0z + (Axis == 0 ? Cell / S : 0);
			EdgeShift(P0x / Cell, P0z / Cell, Sh);
			const double S0x = Sh[0], S0z = Sh[1];
			EdgeShift(P1x / Cell, P1z / Cell, Sh);
			const double S1x = Sh[0], S1z = Sh[1];
			const double Ah = Heights[(size_t)Wl.A0], Bh = Heights[(size_t)Wl.B0], Ah1 = Heights[(size_t)Wl.A1], Bh1 = Heights[(size_t)Wl.B1];
			const double Top = std::max(std::max(Ah, Bh), std::max(Ah1, Bh1)), FootV = std::min(std::min(Ah, Bh), std::min(Ah1, Bh1));
			const double Damp = Wet[(size_t)C] || Wet[(size_t)D] ? 1 : 0;
			const bool bReverse = Axis == 1;
			auto FacePoint = [&](double Px, double Pz, double Sx, double Sz, double A, double B, double T, double OutP[3])
			{
				const double U = Px / Cell, V = Pz / Cell;
				const int Ix = (int)std::floor(U), Iz = (int)std::floor(V);
				const double Q = Axis == 0 ? V - Iz : U - Ix;
				const double EndA = Mixed(Ix, Iz) ? 0 : 1, EndB = Mixed(Axis == 0 ? Ix : Ix + 1, Axis == 0 ? Iz + 1 : Iz) ? 0 : 1;
				const double Fade = T > 0 && T < 1 ? std::sin(PiV * T) * (EndA + (EndB - EndA) * Q) : 0;
				const double Warp = (FieldNoise(Px * 0.24, Pz * 0.24, 113) - 0.5) * 0.18 * Fade;
				const double Height = A + (B - A) * T + std::abs(B - A) * Warp;
				double Ca[4], Cb[4];
				CornerShift(Ix, Iz, Ca); CornerShift(Axis == 0 ? Ix : Ix + 1, Axis == 0 ? Iz + 1 : Iz, Cb);
				const double Dx = Ca[2] + (Cb[2] - Ca[2]) * Q, Dz = Ca[3] + (Cb[3] - Ca[3]) * Q;
				const double Length = Hypot2(Dx, Dz);
				const double RockN = FieldNoise(Px * 0.20 + Height * 0.17, Pz * 0.20 - Height * 0.14, 127);
				const double DepthV = N > 65536 || !(Length != 0) ? 0 : -Fade * (0.25 + RockN * 0.85);
				const double L1 = Length != 0 ? Length : 1;
				OutP[0] = Px + Sx + Dx / L1 * DepthV; OutP[1] = Height; OutP[2] = Pz + Sz + Dz / L1 * DepthV;
			};
			Wr.Owner = C;
			for (int Band = 0; Band < Wl.Bands; Band++)
			{
				double A[3], B[3], Dd[3], E[3];
				FacePoint(P0x, P0z, S0x, S0z, Ah, Bh, (double)Band / Wl.Bands, A);
				FacePoint(P1x, P1z, S1x, S1z, Ah1, Bh1, (double)Band / Wl.Bands, B);
				FacePoint(P1x, P1z, S1x, S1z, Ah1, Bh1, (double)(Band + 1) / Wl.Bands, Dd);
				FacePoint(P0x, P0z, S0x, S0z, Ah, Bh, (double)(Band + 1) / Wl.Bands, E);
				Wr.RockTri(A, B, E, Top, FootV, Damp, bReverse); Wr.RockTri(B, Dd, E, Top, FootV, Damp, bReverse);
			}
			const double Ox = Axis == 0 ? SignV : 0, Oz = Axis == 1 ? SignV : 0;
			for (const FRock& Rk : Wl.Rocks)
			{
				const double F = Rk.Along, FootY = std::min(Ah, Bh) + (std::min(Ah1, Bh1) - std::min(Ah, Bh)) * F;
				const double TopY = std::max(Ah, Bh) + (std::max(Ah1, Bh1) - std::max(Ah, Bh)) * F;
				Wr.PlaceRock(Rk, P0x + S0x + (P1x + S1x - P0x - S0x) * F, P0z + S0z + (P1z + S1z - P0z - S0z) * F, FootY, TopY, Ox, Oz, Damp);
			}
		}
		// fillers: a notch (one low cell, high on three sides) gets a rock pile, a tooth (one high cell) a rock column
		if (N <= 65536 && Z >= 1)
			for (int X = std::max(1, X0); X < X1; X++)
			{
				const int Ids[4] = { (Z - 1) * W + X - 1, (Z - 1) * W + X, Z * W + X - 1, Z * W + X };
				if (!Kn(Ids[0]) || !Kn(Ids[1]) || !Kn(Ids[2]) || !Kn(Ids[3])) continue;
				const int Quad[4] = { Levels[(size_t)Ids[0]], Levels[(size_t)Ids[1]], Levels[(size_t)Ids[2]], Levels[(size_t)Ids[3]] };
				const int Bottom = std::min(std::min(Quad[0], Quad[1]), std::min(Quad[2], Quad[3]));
				int HighN = 0, Top = 1 << 20, FirstHigh = -1;
				for (int Q = 0; Q < 4; Q++) if (Quad[Q] >= Bottom + 2) { HighN++; Top = std::min(Top, Quad[Q]); if (FirstHigh < 0) FirstHigh = Q; }
				const bool bNotch = HighN == 3;
				bool bSkip = !bNotch && HighN != 1;
				for (int Q = 0; Q < 4; Q++) if (Quad[Q] < Bottom + 2 && Quad[Q] > Top - 2) bSkip = true;
				if (bSkip) continue;
				int IndexOfBottom = 0;
				while (Quad[IndexOfBottom] != Bottom) IndexOfBottom++;
				// quad.indexOf(high[0]): the first cell holding the first high value
				int IndexOfHigh = 0;
				while (Quad[IndexOfHigh] != Quad[FirstHigh]) IndexOfHigh++;
				const int I = bNotch ? IndexOfBottom : 3 - IndexOfHigh;
				const double Rise = (Top - Bottom) * LevelHeight;
				const int Layers = std::max(1, (int)JsRound(Rise / 3.2));
				std::vector<FRock> Rocks;
				for (int Layer = 0; Layer < Layers; Layer++)
				{
					auto R = [&](double Salt) { return Hash(X * 2 + Layer, Z * 2 + Salt * 0.37, 211 + Salt); };
					const double T = (Layer + 0.5) / Layers;
					FRock Rk;
					Rk.Model = &RockModels(bLow ? 0 : 1)[(size_t)std::floor(R(0) * 6)]; Rk.T = T; Rk.Yaw = (R(1) - 0.5) * 0.6;
					if (bNotch) { Rk.Size[0] = 1.0 + R(2) * 0.3; Rk.Size[1] = Rise / Layers * (0.6 + R(3) * 0.2); Rk.Size[2] = 0.5 + R(4) * 0.15; }
					else { Rk.Size[0] = 0.55 + R(2) * 0.15; Rk.Size[1] = Rise / Layers * (0.6 + R(3) * 0.2); Rk.Size[2] = 0.55 + R(4) * 0.15; }
					Rk.Out = bNotch ? 0.6 - 0.6 * T : 0.1 - 0.4 * T;
					Rk.Lift = bNotch && Layer == Layers - 1 ? 0.25 : 0;
					Rocks.push_back(Rk);
				}
				Wr.Owner = Z * W + X;
				for (const FRock& Rk : Rocks)
					Wr.PlaceRock(Rk, X * Cell, Z * Cell, Bottom * LevelHeight, Top * LevelHeight, ((I & 1) ? 1 : -1) * Sqrt1_2, ((I & 2) ? 1 : -1) * Sqrt1_2, 0);
			}
	}
	// relief.js detailPicks: reshaped triangles (detail vertices, wall bands) indexed by the cells they overlap
	if (N <= 65536 || bDrawnPicks)
		for (uint32_t T = 0; T < (uint32_t)P.Idx.size(); T += 3)
		{
			const uint32_t A = P.Idx[T], B = P.Idx[T + 1], C = P.Idx[T + 2];
			if (!P.Detail[A] && !P.Detail[B] && !P.Detail[C] && P.Paint[(size_t)A * 4] != 1.f) continue;
			if (N > 65536 && !(P.Detail[A] || P.Detail[B] || P.Detail[C])) continue; // relief.js indexes nothing on huge maps
			const float* Ps = P.Pos.data();
			const int MinX = ClampI((int)std::floor(std::min(std::min(Ps[A * 3], Ps[B * 3]), Ps[C * 3]) / Cell), 0, W - 1);
			const int MaxX = ClampI((int)std::floor(std::max(std::max(Ps[A * 3], Ps[B * 3]), Ps[C * 3]) / Cell), 0, W - 1);
			const int MinZ = ClampI((int)std::floor(std::min(std::min(Ps[A * 3 + 2], Ps[B * 3 + 2]), Ps[C * 3 + 2]) / Cell), 0, H - 1);
			const int MaxZ = ClampI((int)std::floor(std::max(std::max(Ps[A * 3 + 2], Ps[B * 3 + 2]), Ps[C * 3 + 2]) / Cell), 0, H - 1);
			for (int Z = MinZ; Z <= MaxZ; Z++) for (int X = MinX; X <= MaxX; X++) P.Picks[Z * W + X].push_back(T);
		}
}

// ------------------------------------------------------------------------------------------------- build and update

void FRelief::Build()
{
	for (int C = 0; C < N; C++) ReadCell(C);
	for (int Z = 0; Z <= H; Z++) for (int X = 0; X <= W; X++) ReadQuad(X, Z);
	ReadWater();
	for (int C = 0; C < N; C++) ControlCell(C);
	for (int C = 0; C < N; C++) TangentCell(C);
	for (int C = 0; C < N; C++) BuildCell(C);
	for (int Z = 0; Z < H; Z++) PlanRow(Z);
	Rebuilt.clear();
	Fans();
	Rebuilt.clear();
	Stats.PartsRebuilt = 0;
	for (int P = 0; P < (int)Parts.size(); P++) { EmitPart(P); Rebuilt.push_back(P); Stats.PartsRebuilt++; }
	Stats.CellsUpdated = N;
	Stats.CliffEdges = CliffEdgesNow();
	Stats.Vertices = Stats.Triangles = 0;
	for (const FPart& P : Parts) { Stats.Vertices += P.Vertices(); Stats.Triangles += (int)(P.Idx.size() / 3); }
}

bool FRelief::SetLow(bool bNewLow)
{
	if (bNewLow == bLow) return false;
	bLow = bNewLow;
	if (N <= 0) return false;
	// relief.js rebuildGeometry() with no touched set: the rows' walls and rocks, the centre fans, every part
	for (int Z = 0; Z < H; Z++) PlanRow(Z);
	const size_t Before = Rebuilt.size();
	Fans();
	Rebuilt.resize(Before);
	Stats.PartsRebuilt = 0;
	for (int P = 0; P < (int)Parts.size(); P++) { EmitPart(P); Rebuilt.push_back(P); Stats.PartsRebuilt++; }
	Stats.CliffEdges = CliffEdgesNow();
	Stats.Vertices = Stats.Triangles = 0;
	for (const FPart& P : Parts) { Stats.Vertices += P.Vertices(); Stats.Triangles += (int)(P.Idx.size() / 3); }
	return true;
}

int FRelief::CliffEdgesNow() const
{
	int Sum = 0;
	for (const FRowPlan& Row : Rows) Sum += Row.CliffEdges;
	return Sum;
}

void FRelief::Update(const std::vector<int>& Cells)
{
	if (N <= 0) return;
	std::vector<uint8_t> DirtyMark((size_t)N, 0);
	std::vector<int> Dirty;
	auto Mark = [&](int X0, int Z0, int R)
	{
		for (int J = std::max(0, Z0 - R); J <= std::min(H - 1, Z0 + R); J++)
			for (int I = std::max(0, X0 - R); I <= std::min(W - 1, X0 + R); I++)
				if (!DirtyMark[(size_t)(J * W + I)]) { DirtyMark[(size_t)(J * W + I)] = 1; Dirty.push_back(J * W + I); }
	};
	for (const int C : Cells) if (C >= 0 && C < N) Mark(C % W, C / W, 3);
	// a topology change can alter the line of a whole connected water body
	bool bWetChanged = false;
	for (const int C : Cells)
	{
		if (C < 0 || C >= N) continue;
		const uint8_t Chr = Ch(C);
		if ((Wet[(size_t)C] != 0) != (Chr == 'W' || Chr == 'F' || Chr == '=') || (Bridges[(size_t)C] != 0) != (Chr == '=')) bWetChanged = true;
	}
	if (bWetChanged)
	{
		const std::vector<float> OldWL = WL;
		ReadWater();
		for (int C = 0; C < N; C++)
			if (OldWL[(size_t)C] != WL[(size_t)C] && !(std::isnan(OldWL[(size_t)C]) && std::isnan(WL[(size_t)C]))) Mark(C % W, C / W, 1);
	}
	for (const int C : Dirty) ReadCell(C);
	for (const int C : Dirty) { const int X = C % W, Z = C / W; ReadQuad(X, Z); ReadQuad(X + 1, Z); ReadQuad(X, Z + 1); ReadQuad(X + 1, Z + 1); }
	for (const int C : Dirty) ControlCell(C);
	for (const int C : Dirty) TangentCell(C);
	for (const int C : Dirty) BuildCell(C);
	// crater lips read the scar shape up to six cells away (scarRun, touch), so a change moves vertices that far
	std::vector<uint8_t> RowTouched((size_t)H, 0), PartDirty(Parts.size(), 0);
	for (const int C : Dirty)
	{
		const int Z = C / W, X = C % W;
		for (int J = std::max(0, Z - 7); J <= std::min(H - 1, Z + 7); J++) RowTouched[(size_t)J] = 1;
		if (ChunkCells > 0)
			for (int J = std::max(0, Z - 7); J <= std::min(H - 1, Z + 7); J += 1)
				for (int I = std::max(0, X - 7); I <= std::min(W - 1, X + 7); I++) PartDirty[(size_t)PartOfCell(J * W + I)] = 1;
	}
	for (int Z = 0; Z < H; Z++) if (RowTouched[(size_t)Z]) { PlanRow(Z); if (ChunkCells <= 0) PartDirty[(size_t)Z] = 1; }
	const size_t Before = Rebuilt.size();
	Fans();
	for (size_t I = Before; I < Rebuilt.size(); I++) PartDirty[(size_t)(-1 - Rebuilt[I])] = 1;
	Rebuilt.resize(Before);
	Stats.PartsRebuilt = 0;
	for (int P = 0; P < (int)Parts.size(); P++) if (PartDirty[(size_t)P])
	{
		EmitPart(P);
		if (std::find(Rebuilt.begin(), Rebuilt.end(), P) == Rebuilt.end()) Rebuilt.push_back(P);
		Stats.PartsRebuilt++;
	}
	Stats.CellsUpdated = (int)Dirty.size();
	Stats.CliffEdges = CliffEdgesNow();
	Stats.Vertices = Stats.Triangles = 0;
	for (const FPart& P : Parts) { Stats.Vertices += P.Vertices(); Stats.Triangles += (int)(P.Idx.size() / 3); }
}

std::vector<int> FRelief::TakeRebuilt()
{
	std::vector<int> Out2;
	Out2.swap(Rebuilt);
	return Out2;
}

// ------------------------------------------------------------------------------------------------- queries

double FRelief::Lattice(int C, double U, double V) const
{
	const int Ix = C % W, Iz = C / W;
	if (Flat[(size_t)C] == 2)
	{
		double R[3] = { Levels[(size_t)C] * LevelHeight, 0, 0 };
		RoadSample(U, V, C, R);
		return R[0];
	}
	const int Xs = NX[(size_t)C], Zs = NZ[(size_t)C];
	const double Fx = (U - Ix) * Xs, Fz = (V - Iz) * Zs;
	const int I = std::min(Xs - 1, (int)std::floor(Fx)), J = std::min(Zs - 1, (int)std::floor(Fz));
	const double Tx = Fx - I, Tz = Fz - J;
	const int StepX = S / Xs, StepZ = S / Zs, A = C * Stride + J * StepZ * 5 + I * StepX;
	const double Av = Heights[(size_t)A], Bv = Heights[(size_t)(A + StepX)], Cv = Heights[(size_t)(A + StepZ * 5)], Dv = Heights[(size_t)(A + StepZ * 5 + StepX)];
	return Tx + Tz <= 1 ? Av + (Bv - Av) * Tx + (Cv - Av) * Tz : Dv + (Cv - Dv) * (1 - Tx) + (Bv - Dv) * (1 - Tz);
}

double FRelief::HAt(double X, double Z) const
{
	if (N <= 0) return 0;
	const double U = ClampD(X / Cell, 0, W), V = ClampD(Z / Cell, 0, H);
	const int Ix = std::min(W - 1, (int)std::floor(U)), Iz = std::min(H - 1, (int)std::floor(V));
	const int C = Iz * W + Ix;
	double Top = -Inf;
	bool bAny = false;
	// the parts that can hold triangles over this cell: rock faces and shoved lips reach at most two cells out
	int Seen[9], NumSeen = 0;
	for (int Dz = -2; Dz <= 2; Dz++) for (int Dx = -2; Dx <= 2; Dx++)
	{
		const int Xx = Ix + Dx, Zz = Iz + Dz;
		if (Xx < 0 || Zz < 0 || Xx >= W || Zz >= H) continue;
		const int Pp = PartOfCell(Zz * W + Xx);
		bool bNew = true;
		for (int K = 0; K < NumSeen; K++) if (Seen[K] == Pp) bNew = false;
		if (!bNew || NumSeen >= 9) continue;
		Seen[NumSeen++] = Pp;
		const FPart& P = Parts[(size_t)Pp];
		const auto Found = P.Picks.find(C);
		if (Found == P.Picks.end()) continue;
		bAny = true;
		const float* Ps = P.Pos.data();
		for (const uint32_t T : Found->second)
		{
			const size_t A = (size_t)P.Idx[T] * 3, B = (size_t)P.Idx[T + 1] * 3, D = (size_t)P.Idx[T + 2] * 3;
			const double Bx = (double)Ps[B] - Ps[A], Bz = (double)Ps[B + 2] - Ps[A + 2], Dxx = (double)Ps[D] - Ps[A], Dzz = (double)Ps[D + 2] - Ps[A + 2];
			const double Determinant = Bx * Dzz - Bz * Dxx;
			if (Determinant >= -1e-10) continue; // downward-facing or vertical triangles do not support the ground ray
			const double Px = X - Ps[A], Pz = Z - Ps[A + 2];
			const double Sv = (Px * Dzz - Pz * Dxx) / Determinant, T0 = (Bx * Pz - Bz * Px) / Determinant;
			if (Sv >= -1e-7 && T0 >= -1e-7 && Sv + T0 <= 1 + 1e-7)
				Top = std::max(Top, (double)Ps[A + 1] + ((double)Ps[B + 1] - Ps[A + 1]) * Sv + ((double)Ps[D + 1] - Ps[A + 1]) * T0);
		}
	}
	if (bAny && Top > -Inf)
	{
		if (bDrawnPicks && !Ragged[(size_t)C] && !Shoved[(size_t)C]) return std::max(Top, Lattice(C, U, V));
		return Top;
	}
	return Lattice(C, U, V);
}

double FRelief::WaterAt(double X, double Z) const
{
	if (N <= 0) return std::numeric_limits<double>::quiet_NaN();
	const int C = std::min(H - 1, std::max(0, (int)std::floor(Z / Cell))) * W + std::min(W - 1, std::max(0, (int)std::floor(X / Cell)));
	return (double)WL[(size_t)C] + WaterLift;
}

namespace ReliefRay
{
	// three.js r186 Ray.intersectTriangle (watertight), backface culling on; the distance along the ray or -1
	inline double Intersect(const double O[3], const double Dir[3], const double* A, const double* B, const double* C)
	{
		const double Dx = Dir[0], Dy = Dir[1], Dz = Dir[2];
		const double Aox = A[0] - O[0], Aoy = A[1] - O[1], Aoz = A[2] - O[2];
		const double Box = B[0] - O[0], Boy = B[1] - O[1], Boz = B[2] - O[2];
		const double Cox = C[0] - O[0], Coy = C[1] - O[1], Coz = C[2] - O[2];
		const double Adx = std::abs(Dx), Ady = std::abs(Dy), Adz = std::abs(Dz);
		double Dkx, Dky, Dkz, Akx, Aky, Akz, Bkx, Bky, Bkz, Ckx, Cky, Ckz;
		if (Adx >= Ady && Adx >= Adz)
		{
			Dkz = Dx; Akz = Aox; Bkz = Box; Ckz = Cox;
			if (Dx >= 0) { Dkx = Dy; Dky = Dz; Akx = Aoy; Aky = Aoz; Bkx = Boy; Bky = Boz; Ckx = Coy; Cky = Coz; }
			else { Dkx = Dz; Dky = Dy; Akx = Aoz; Aky = Aoy; Bkx = Boz; Bky = Boy; Ckx = Coz; Cky = Coy; }
		}
		else if (Ady >= Adz)
		{
			Dkz = Dy; Akz = Aoy; Bkz = Boy; Ckz = Coy;
			if (Dy >= 0) { Dkx = Dz; Dky = Dx; Akx = Aoz; Aky = Aox; Bkx = Boz; Bky = Box; Ckx = Coz; Cky = Cox; }
			else { Dkx = Dx; Dky = Dz; Akx = Aox; Aky = Aoz; Bkx = Box; Bky = Boz; Ckx = Cox; Cky = Coz; }
		}
		else
		{
			Dkz = Dz; Akz = Aoz; Bkz = Boz; Ckz = Coz;
			if (Dz >= 0) { Dkx = Dx; Dky = Dy; Akx = Aox; Aky = Aoy; Bkx = Box; Bky = Boy; Ckx = Cox; Cky = Coy; }
			else { Dkx = Dy; Dky = Dx; Akx = Aoy; Aky = Aox; Bkx = Boy; Bky = Box; Ckx = Coy; Cky = Cox; }
		}
		if (Dkz == 0) return -1;
		const double Sx = Dkx / Dkz, Sy = Dky / Dkz, Sz = 1 / Dkz;
		const double Ax = Akx - Sx * Akz, Ay = Aky - Sy * Akz;
		const double Bx = Bkx - Sx * Bkz, By = Bky - Sy * Bkz;
		const double Cx = Ckx - Sx * Ckz, Cy = Cky - Sy * Ckz;
		const double U = Cx * By - Cy * Bx, V = Ax * Cy - Ay * Cx, Wt = Bx * Ay - By * Ax;
		if (U < 0 || V < 0 || Wt < 0) return -1;
		const double Det = U + V + Wt;
		if (Det == 0) return -1;
		const double TScaled = Sz * (U * Akz + V * Bkz + Wt * Ckz);
		if (Det > 0 ? TScaled < 0 : TScaled > 0) return -1;
		const double T = TScaled / Det;
		const double Px = Dx * T, Py = Dy * T, Pz = Dz * T;
		return std::sqrt(Px * Px + Py * Py + Pz * Pz); // pickPoint.distanceTo(origin)
	}
}

// relief.js mesh.raycast: visit cells in ray order and intersect their contact triangles
bool FRelief::Ray(const double O[3], const double D[3], double MaxT, double OutP[3]) const
{
	if (N <= 0) return false;
	double Begin = 0, End = MaxT;
	const double Origins[3] = { O[0], O[1], O[2] }, Directions[3] = { D[0], D[1], D[2] };
	const double Mins[3] = { 0, -12, 0 }, Maxs[3] = { W * Cell, 12, H * Cell }; // relief.js fixed bounds
	for (int Axis = 0; Axis < 3; Axis++)
	{
		if (std::abs(Directions[Axis]) < 1e-12) { if (Origins[Axis] < Mins[Axis] || Origins[Axis] > Maxs[Axis]) return false; }
		else
		{
			const double A = (Mins[Axis] - Origins[Axis]) / Directions[Axis], B = (Maxs[Axis] - Origins[Axis]) / Directions[Axis];
			Begin = std::max(Begin, std::min(A, B)); End = std::min(End, std::max(A, B));
		}
	}
	if (End < Begin) return false;
	const int Sxn = std::abs(D[0]) < 1e-12 ? 0 : (int)Sign(D[0]), Szn = std::abs(D[2]) < 1e-12 ? 0 : (int)Sign(D[2]);
	const double Epsilon = 1e-7;
	const double StartX = (O[0] + D[0] * Begin) / Cell, StartZ = (O[2] + D[2] * Begin) / Cell;
	const bool bBoundaryX = !Sxn && std::abs(StartX - JsRound(StartX)) < 1e-9;
	const bool bBoundaryZ = !Szn && std::abs(StartZ - JsRound(StartZ)) < 1e-9;
	int X = ClampI((int)(bBoundaryX ? JsRound(StartX) : std::floor(StartX)) - (Sxn < 0 && StartX == std::floor(StartX) ? 1 : 0), 0, W - 1);
	int Z = ClampI((int)(bBoundaryZ ? JsRound(StartZ) : std::floor(StartZ)) - (Szn < 0 && StartZ == std::floor(StartZ) ? 1 : 0), 0, H - 1);
	const bool bBothX = bBoundaryX && StartX > 0 && StartX < W, bBothZ = bBoundaryZ && StartZ > 0 && StartZ < H;
	double NextX = Sxn ? ((X + (Sxn > 0 ? 1 : 0)) * Cell - O[0]) / D[0] : Inf;
	double NextZ = Szn ? ((Z + (Szn > 0 ? 1 : 0)) * Cell - O[2]) / D[2] : Inf;
	const double DeltaX = Sxn ? Cell / std::abs(D[0]) : Inf, DeltaZ = Szn ? Cell / std::abs(D[2]) : Inf;
	double IntervalEnd = 0, Best = Inf;
	auto Triangle = [&](const double* A, const double* B, const double* C)
	{
		const double Distance = ReliefRay::Intersect(O, D, A, B, C);
		if (Distance < 0) return;
		if (Distance >= Begin - Epsilon && Distance <= IntervalEnd + Epsilon && Distance < Best && Distance <= MaxT) Best = Distance;
	};
	auto Node = [&](double* Tg, int C, int I, int J) { Tg[0] = (C % W + (double)I / S) * Cell; Tg[1] = Heights[(size_t)(C * Stride + J * 5 + I)]; Tg[2] = (C / W + (double)J / S) * Cell; };
	double Pa[3], Pb[3], Pc[3], Pd[3], Pm[3];
	auto WallFn = [&](int C, int Axis)
	{
		if (N <= 65536) return; // detailed walls are queried from their drawn triangles
		const int Neighbor = C + (Axis == 0 ? 1 : W);
		if (!Cliff(C, Neighbor)) return;
		for (int K = 0; K < S; K++)
		{
			if (Axis == 0)
			{
				Node(Pa, C, S, K); Node(Pb, C, S, K + 1); Node(Pc, Neighbor, 0, K); Node(Pd, Neighbor, 0, K + 1);
				Triangle(Pa, Pb, Pc); Triangle(Pb, Pd, Pc);
			}
			else
			{
				Node(Pa, C, K, S); Node(Pb, C, K + 1, S); Node(Pc, Neighbor, K, 0); Node(Pd, Neighbor, K + 1, 0);
				Triangle(Pa, Pc, Pb); Triangle(Pb, Pc, Pd);
			}
		}
	};
	auto Surface = [&](int C)
	{
		const int Cx = C % W, Cz = C / W;
		int Seen[9], NumSeen = 0;
		for (int Dz = -2; Dz <= 2; Dz++) for (int Dx = -2; Dx <= 2; Dx++)
		{
			const int Xx = Cx + Dx, Zz = Cz + Dz;
			if (Xx < 0 || Zz < 0 || Xx >= W || Zz >= H) continue;
			const int Pp = PartOfCell(Zz * W + Xx);
			bool bNew = true;
			for (int K = 0; K < NumSeen; K++) if (Seen[K] == Pp) bNew = false;
			if (!bNew || NumSeen >= 9) continue;
			Seen[NumSeen++] = Pp;
			const FPart& P = Parts[(size_t)Pp];
			const auto Found = P.Picks.find(C);
			if (Found == P.Picks.end()) continue;
			for (const uint32_t T : Found->second)
			{
				const float* Ps = P.Pos.data();
				const size_t A = (size_t)P.Idx[T] * 3, B = (size_t)P.Idx[T + 1] * 3, Cc = (size_t)P.Idx[T + 2] * 3;
				const double Ta[3] = { Ps[A], Ps[A + 1], Ps[A + 2] }, Tb[3] = { Ps[B], Ps[B + 1], Ps[B + 2] }, Tc[3] = { Ps[Cc], Ps[Cc + 1], Ps[Cc + 2] };
				Triangle(Ta, Tb, Tc);
			}
		}
		if (Ragged[(size_t)C] || (bDrawnPicks && Shoved[(size_t)C]) || !Kn(C)) return;
		const int Xs = NX[(size_t)C], Zs = NZ[(size_t)C], StepX = S / Xs, StepZ = S / Zs;
		if (Flat[(size_t)C] == 2)
		{
			Node(Pa, C, 0, 0); Node(Pb, C, S, 0); Node(Pc, C, 0, S); Node(Pd, C, S, S); Node(Pm, C, S / 2, S / 2);
			Triangle(Pa, Pm, Pb); Triangle(Pb, Pm, Pd); Triangle(Pd, Pm, Pc); Triangle(Pc, Pm, Pa);
		}
		else for (int J = 0; J < Zs; J++) for (int I = 0; I < Xs; I++)
		{
			Node(Pa, C, I * StepX, J * StepZ); Node(Pb, C, (I + 1) * StepX, J * StepZ);
			Node(Pc, C, I * StepX, (J + 1) * StepZ); Node(Pd, C, (I + 1) * StepX, (J + 1) * StepZ);
			Triangle(Pa, Pc, Pb); Triangle(Pb, Pc, Pd);
		}
	};
	while (X >= 0 && X < W && Z >= 0 && Z < H)
	{
		IntervalEnd = std::min(End, std::min(NextX, NextZ)); Best = Inf;
		const int C = Z * W + X;
		Surface(C);
		if (bBothX) Surface(C - 1);
		if (bBothZ) Surface(C - W);
		if (bBothX && bBothZ) Surface(C - W - 1);
		if (X > 0) WallFn(C - 1, 0);
		if (X + 1 < W) WallFn(C, 0);
		if (Z > 0) WallFn(C - W, 1);
		if (Z + 1 < H) WallFn(C, 1);
		if (Best < Inf) { OutP[0] = O[0] + D[0] * Best; OutP[1] = O[1] + D[1] * Best; OutP[2] = O[2] + D[2] * Best; return true; }
		if (IntervalEnd >= End) return false;
		const bool bCrossX = NextX <= NextZ, bCrossZ = NextZ <= NextX;
		Begin = IntervalEnd;
		if (bCrossX) { X += Sxn; NextX += DeltaX; }
		if (bCrossZ) { Z += Szn; NextZ += DeltaZ; }
	}
	return false;
}

// ------------------------------------------------------------------------------------------------- roads

void RoadMask(int W, int H, const std::vector<uint8_t>& Grid, const std::vector<uint8_t>& RowsIn, const std::vector<uint8_t>& Fields,
	const std::vector<uint8_t>& Known, std::vector<uint8_t>& OutMask)
{
	const int Nn = W * H;
	OutMask.assign((size_t)std::max(0, Nn), 0);
	auto KnownAt = [&](int C) { return Known.empty() || Known[(size_t)C] != 0; };
	for (int I = 0; I < Nn; I++)
	{
		if (!KnownAt(I)) continue;
		const uint8_t Chr = Grid[(size_t)I];
		if (Chr == 'D') { OutMask[(size_t)I] = 1; continue; }
		if (Chr != '.') continue;
		const int X = I % W, Y = I / W;
		bool bWet = false, bTown = false;
		for (int Dy = -1; Dy <= 1; Dy++) for (int Dx = -1; Dx <= 1; Dx++)
		{
			const int Xx = X + Dx, Yy = Y + Dy;
			if (Xx < 0 || Yy < 0 || Xx >= W || Yy >= H) continue;
			const int K = Yy * W + Xx;
			const uint8_t Nc = KnownAt(K) ? Grid[(size_t)K] : (uint8_t)'.';
			if (Nc == 'W' || Nc == 'F' || Nc == '=') bWet = true;
			if (KnownAt(K) && RowsIn[(size_t)K] == 'B') bTown = true;
		}
		if (!Fields.empty() && Fields[(size_t)I]) continue;
		if (bWet) continue;
		if (bTown) OutMask[(size_t)I] = 1;
	}
}
}
