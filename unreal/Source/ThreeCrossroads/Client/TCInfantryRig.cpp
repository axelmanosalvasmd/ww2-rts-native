// The infantry pose solver, ported from the pinned reference (reference/ww2-rts @ e4e579a):
//   client/models/infantry.js        soldierKit, STANCES, spec, fallenSpec, digSpec, solve, makeBody, leg, arm, bend,
//                                    headMatrix, basis, torsoAt, clothAt, onTorso (kit placement)
//   client/models/infantry-authored.js  frame, transforms (the 15 fitted skin matrices)
//   client/squad-motion.js           gaitWeights
//   client/unit-models.js            POSES, AIM_SHIFT/HAND offsets, poseMorphs (the posture undo) and the per-man
//                                    keyframe weights of animate()
// Every step follows the JavaScript operation by operation in double precision (three.js Matrix4 and Vector3 use
// JavaScript numbers), so the Node oracle (unreal/Tools/models/infantry/oracle) can compare joint positions directly.
//
// The core lives in namespace TCInfantryRigCore below and uses no engine types. With TC_INFANTRY_RIG_STANDALONE
// defined this file compiles on its own (g++ or clang) for the oracle; otherwise the Unreal adapters at the end wrap it.
// Axes in the core are the reference's: +x forward, +y up, +z the soldier's right, units the soldier's own metres.
#ifndef TC_INFANTRY_RIG_STANDALONE
#include "Client/TCInfantryRig.h"
#include "Animation/AnimInstanceProxy.h"
#include "BonePose.h"
#include "Engine/Texture2D.h"
#include "Components/SkeletalMeshComponent.h"
#endif

#include <cmath>
#include <cstdint>
#include <cstring>

namespace TCInfantryRigCore
{
	constexpr double Tau = 6.283185307179586;
	constexpr double HalfPi = 1.5707963267948966;
	constexpr double OnePi = 3.141592653589793;

	// ------------------------------------------------------------------ three.js math, operation for operation

	struct FV
	{
		double X = 0, Y = 0, Z = 0;
	};
	inline FV V3(double X = 0, double Y = 0, double Z = 0) { FV R; R.X = X; R.Y = Y; R.Z = Z; return R; }
	inline FV Add(const FV& A, const FV& B) { return V3(A.X + B.X, A.Y + B.Y, A.Z + B.Z); }
	inline FV Sub(const FV& A, const FV& B) { return V3(A.X - B.X, A.Y - B.Y, A.Z - B.Z); }
	inline FV Mul(const FV& A, double S) { return V3(A.X * S, A.Y * S, A.Z * S); }
	// a.addScaledVector(b, s)
	inline FV AddScaled(const FV& A, const FV& B, double S) { return V3(A.X + B.X * S, A.Y + B.Y * S, A.Z + B.Z * S); }
	inline double Dot(const FV& A, const FV& B) { return A.X * B.X + A.Y * B.Y + A.Z * B.Z; }
	inline FV Cross(const FV& A, const FV& B) { return V3(A.Y * B.Z - A.Z * B.Y, A.Z * B.X - A.X * B.Z, A.X * B.Y - A.Y * B.X); }
	inline double LengthSq(const FV& A) { return A.X * A.X + A.Y * A.Y + A.Z * A.Z; }
	inline double Length(const FV& A) { return std::sqrt(A.X * A.X + A.Y * A.Y + A.Z * A.Z); }
	inline double Distance(const FV& A, const FV& B) { const double DX = A.X - B.X, DY = A.Y - B.Y, DZ = A.Z - B.Z; return std::sqrt(DX * DX + DY * DY + DZ * DZ); }
	// Vector3.normalize(): divideScalar(length || 1), which multiplies by the reciprocal
	inline FV Normalize(const FV& A) { const double L = Length(A); const double K = 1.0 / (L != 0.0 ? L : 1.0); return V3(A.X * K, A.Y * K, A.Z * K); }
	inline FV SetLength(const FV& A, double L) { return Mul(Normalize(A), L); }

	// column-major, as three.js Matrix4.elements
	struct FM
	{
		double E[16];
	};
	inline FM Set(double N11, double N12, double N13, double N14, double N21, double N22, double N23, double N24,
		double N31, double N32, double N33, double N34, double N41, double N42, double N43, double N44)
	{
		FM M;
		M.E[0] = N11; M.E[4] = N12; M.E[8] = N13; M.E[12] = N14;
		M.E[1] = N21; M.E[5] = N22; M.E[9] = N23; M.E[13] = N24;
		M.E[2] = N31; M.E[6] = N32; M.E[10] = N33; M.E[14] = N34;
		M.E[3] = N41; M.E[7] = N42; M.E[11] = N43; M.E[15] = N44;
		return M;
	}
	inline FM Identity() { return Set(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1); }
	inline FM MatMul(const FM& A, const FM& B)
	{
		const double* AE = A.E; const double* BE = B.E;
		const double A11 = AE[0], A12 = AE[4], A13 = AE[8], A14 = AE[12];
		const double A21 = AE[1], A22 = AE[5], A23 = AE[9], A24 = AE[13];
		const double A31 = AE[2], A32 = AE[6], A33 = AE[10], A34 = AE[14];
		const double A41 = AE[3], A42 = AE[7], A43 = AE[11], A44 = AE[15];
		const double B11 = BE[0], B12 = BE[4], B13 = BE[8], B14 = BE[12];
		const double B21 = BE[1], B22 = BE[5], B23 = BE[9], B24 = BE[13];
		const double B31 = BE[2], B32 = BE[6], B33 = BE[10], B34 = BE[14];
		const double B41 = BE[3], B42 = BE[7], B43 = BE[11], B44 = BE[15];
		FM R;
		double* TE = R.E;
		TE[0] = A11 * B11 + A12 * B21 + A13 * B31 + A14 * B41;
		TE[4] = A11 * B12 + A12 * B22 + A13 * B32 + A14 * B42;
		TE[8] = A11 * B13 + A12 * B23 + A13 * B33 + A14 * B43;
		TE[12] = A11 * B14 + A12 * B24 + A13 * B34 + A14 * B44;
		TE[1] = A21 * B11 + A22 * B21 + A23 * B31 + A24 * B41;
		TE[5] = A21 * B12 + A22 * B22 + A23 * B32 + A24 * B42;
		TE[9] = A21 * B13 + A22 * B23 + A23 * B33 + A24 * B43;
		TE[13] = A21 * B14 + A22 * B24 + A23 * B34 + A24 * B44;
		TE[2] = A31 * B11 + A32 * B21 + A33 * B31 + A34 * B41;
		TE[6] = A31 * B12 + A32 * B22 + A33 * B32 + A34 * B42;
		TE[10] = A31 * B13 + A32 * B23 + A33 * B33 + A34 * B43;
		TE[14] = A31 * B14 + A32 * B24 + A33 * B34 + A34 * B44;
		TE[3] = A41 * B11 + A42 * B21 + A43 * B31 + A44 * B41;
		TE[7] = A41 * B12 + A42 * B22 + A43 * B32 + A44 * B42;
		TE[11] = A41 * B13 + A42 * B23 + A43 * B33 + A44 * B43;
		TE[15] = A41 * B14 + A42 * B24 + A43 * B34 + A44 * B44;
		return R;
	}
	// chain(...ms): identity times each matrix in turn
	inline FM Chain(const FM& A) { return MatMul(Identity(), A); }
	template <typename... TRest>
	inline FM Chain(const FM& A, const TRest&... Rest)
	{
		FM Acc = MatMul(Identity(), A);
		const FM Items[] = { Rest... };
		for (const FM& Item : Items) Acc = MatMul(Acc, Item);
		return Acc;
	}
	inline FM Tr(double X, double Y, double Z) { return Set(1, 0, 0, X, 0, 1, 0, Y, 0, 0, 1, Z, 0, 0, 0, 1); }
	inline FM Tr(const FV& P) { return Tr(P.X, P.Y, P.Z); }
	inline FM RotX(double A) { const double C = std::cos(A), S = std::sin(A); return Set(1, 0, 0, 0, 0, C, -S, 0, 0, S, C, 0, 0, 0, 0, 1); }
	inline FM RotY(double A) { const double C = std::cos(A), S = std::sin(A); return Set(C, 0, S, 0, 0, 1, 0, 0, -S, 0, C, 0, 0, 0, 0, 1); }
	inline FM RotZ(double A) { const double C = std::cos(A), S = std::sin(A); return Set(C, -S, 0, 0, S, C, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1); }
	inline FM Scale(double X, double Y, double Z) { return Set(X, 0, 0, 0, 0, Y, 0, 0, 0, 0, Z, 0, 0, 0, 0, 1); }
	inline FM MakeBasis(const FV& X, const FV& Y, const FV& Z) { return Set(X.X, Y.X, Z.X, 0, X.Y, Y.Y, Z.Y, 0, X.Z, Y.Z, Z.Z, 0, 0, 0, 0, 1); }
	inline FM SetPosition(FM M, const FV& P) { M.E[12] = P.X; M.E[13] = P.Y; M.E[14] = P.Z; return M; }
	inline FV Position(const FM& M) { return V3(M.E[12], M.E[13], M.E[14]); }
	// Vector3.applyMatrix4 (with the projective divide three.js performs)
	inline FV Apply(const FM& M, const FV& P)
	{
		const double* E = M.E;
		const double W = 1.0 / (E[3] * P.X + E[7] * P.Y + E[11] * P.Z + E[15]);
		return V3((E[0] * P.X + E[4] * P.Y + E[8] * P.Z + E[12]) * W, (E[1] * P.X + E[5] * P.Y + E[9] * P.Z + E[13]) * W,
			(E[2] * P.X + E[6] * P.Y + E[10] * P.Z + E[14]) * W);
	}
	// Vector3.transformDirection: the upper 3x3, then normalized
	inline FV Dir(const FM& M, const FV& D)
	{
		const double* E = M.E;
		return Normalize(V3(E[0] * D.X + E[4] * D.Y + E[8] * D.Z, E[1] * D.X + E[5] * D.Y + E[9] * D.Z, E[2] * D.X + E[6] * D.Y + E[10] * D.Z));
	}
	// Matrix4.scale(v): the first three columns times x, y, z
	inline FM ScaleColumns(FM M, double X, double Y, double Z)
	{
		for (int R = 0; R < 4; R++) { M.E[R] *= X; M.E[4 + R] *= Y; M.E[8 + R] *= Z; }
		return M;
	}
	inline FM Invert(const FM& M)
	{
		const double* TE = M.E;
		const double N11 = TE[0], N21 = TE[1], N31 = TE[2], N41 = TE[3];
		const double N12 = TE[4], N22 = TE[5], N32 = TE[6], N42 = TE[7];
		const double N13 = TE[8], N23 = TE[9], N33 = TE[10], N43 = TE[11];
		const double N14 = TE[12], N24 = TE[13], N34 = TE[14], N44 = TE[15];
		const double T1 = N11 * N22 - N21 * N12, T2 = N11 * N32 - N31 * N12, T3 = N11 * N42 - N41 * N12;
		const double T4 = N21 * N32 - N31 * N22, T5 = N21 * N42 - N41 * N22, T6 = N31 * N42 - N41 * N32;
		const double T7 = N13 * N24 - N23 * N14, T8 = N13 * N34 - N33 * N14, T9 = N13 * N44 - N43 * N14;
		const double T10 = N23 * N34 - N33 * N24, T11 = N23 * N44 - N43 * N24, T12 = N33 * N44 - N43 * N34;
		const double Det = T1 * T12 - T2 * T11 + T3 * T10 + T4 * T9 - T5 * T8 + T6 * T7;
		FM R;
		if (Det == 0.0) { for (double& Value : R.E) Value = 0.0; return R; }
		const double DetInv = 1.0 / Det;
		double* O = R.E;
		O[0] = (N22 * T12 - N32 * T11 + N42 * T10) * DetInv;
		O[1] = (N31 * T11 - N21 * T12 - N41 * T10) * DetInv;
		O[2] = (N24 * T6 - N34 * T5 + N44 * T4) * DetInv;
		O[3] = (N33 * T5 - N23 * T6 - N43 * T4) * DetInv;
		O[4] = (N32 * T9 - N12 * T12 - N42 * T8) * DetInv;
		O[5] = (N11 * T12 - N31 * T9 + N41 * T8) * DetInv;
		O[6] = (N34 * T3 - N14 * T6 - N44 * T2) * DetInv;
		O[7] = (N13 * T6 - N33 * T3 + N43 * T2) * DetInv;
		O[8] = (N12 * T11 - N22 * T9 + N42 * T7) * DetInv;
		O[9] = (N21 * T9 - N11 * T11 - N41 * T7) * DetInv;
		O[10] = (N14 * T5 - N24 * T3 + N44 * T1) * DetInv;
		O[11] = (N23 * T3 - N13 * T5 - N43 * T1) * DetInv;
		O[12] = (N22 * T8 - N12 * T10 - N32 * T7) * DetInv;
		O[13] = (N11 * T10 - N21 * T8 + N31 * T7) * DetInv;
		O[14] = (N24 * T2 - N14 * T4 - N34 * T1) * DetInv;
		O[15] = (N13 * T4 - N23 * T2 + N33 * T1) * DetInv;
		return R;
	}
	// Matrix4.extractRotation (the reference only uses it on rigid matrices)
	inline FM ExtractRotation(const FM& M)
	{
		const double SX = 1.0 / Length(V3(M.E[0], M.E[1], M.E[2]));
		const double SY = 1.0 / Length(V3(M.E[4], M.E[5], M.E[6]));
		const double SZ = 1.0 / Length(V3(M.E[8], M.E[9], M.E[10]));
		FM R = Identity();
		for (int I = 0; I < 3; I++) { R.E[I] = M.E[I] * SX; R.E[4 + I] = M.E[4 + I] * SY; R.E[8 + I] = M.E[8 + I] * SZ; }
		return R;
	}

	inline double Clamp(double X, double A, double B) { return X < A ? A : (X > B ? B : X); }
	inline double Smooth(double T) { const double K = Clamp(T, 0, 1); return K * K * (3 - 2 * K); }
	inline double Sign(double X) { return X > 0 ? 1.0 : (X < 0 ? -1.0 : X); }
	inline double Sp(double C, double E = 0.72) { return Sign(C) * std::pow(std::fabs(C), E); }
	// Math.round: halves round up
	inline double JsRound(double X) { return std::floor(X + 0.5); }

	// infantry.js basis(at, x, up): a frame at `at` with +x along x and +y as close to `up` as it can be
	inline FM Basis(const FV& At, const FV& XIn, const FV& Up)
	{
		const FV X = Normalize(XIn);
		FV Z = Cross(X, Up);
		if (LengthSq(Z) < 1e-8) Z = Cross(X, std::fabs(X.Y) < 0.9 ? V3(0, 1, 0) : V3(1, 0, 0));
		Z = Normalize(Z);
		return SetPosition(MakeBasis(X, Cross(Z, X), Z), At);
	}
	// infantry-authored.js frame(a, b, up)
	inline FM Frame(const FV& A, const FV& B, const FV& Up)
	{
		const FV X = Normalize(Sub(B, A));
		FV Y = AddScaled(Up, X, -Dot(Up, X));
		if (LengthSq(Y) < 1e-10) Y = AddScaled(V3(0, 0, 1), X, -X.Z);
		Y = Normalize(Y);
		return SetPosition(MakeBasis(X, Y, Normalize(Cross(X, Y))), A);
	}

	// ------------------------------------------------------------------ the reference's tables

	constexpr double HIP = 0.7, HIPZ = 0.085, THIGH = 0.32, SHIN = 0.318, ANKLE = 0.062, UPPER = 0.235, FORE = 0.25, FIST = 0.035;
	constexpr double SHO[3] = { -0.005, 1.1, 0.152 };
	const FV PIVOT = { 0, 1.195, 0 };
	const FV HEADP = { 0.012, 1.27, 0 };
	constexpr double TORSO[8][4] = { { 0.6, 0.146, 0.086, 0.096 }, { 0.68, 0.15, 0.09, 0.1 }, { 0.8, 0.134, 0.09, 0.086 }, { 0.92, 0.145, 0.1, 0.09 },
		{ 1.02, 0.156, 0.102, 0.094 }, { 1.09, 0.162, 0.084, 0.09 }, { 1.14, 0.128, 0.064, 0.068 }, { 1.19, 0.05, 0.044, 0.046 } };

	inline void Section(double Y, double Out[3])
	{
		int I = 0;
		while (I < 8 - 2 && TORSO[I + 1][0] < Y) I++;
		const double* A = TORSO[I]; const double* B = TORSO[I + 1];
		const double T = Clamp((Y - A[0]) / (B[0] - A[0]), 0, 1);
		for (int K = 1; K <= 3; K++) Out[K - 1] = A[K] + (B[K] - A[K]) * T;
	}
	inline FV TorsoAt(double Y, double A, double Out = 0)
	{
		double S[3]; Section(Y, S);
		const double C = std::cos(A), Sn = std::sin(A);
		return V3(Sp(C) * ((C >= 0 ? S[1] : S[2]) + Out), Y, -Sp(Sn) * (S[0] + Out));
	}
	inline FV ClothAt(double Y, double A, double Out = 0)
	{
		double S[3]; Section(Y, S);
		const double C = std::cos(A), Sn = std::sin(A);
		const double CollarFit = Smooth((Y - 1.14) / 0.05);
		const double W = S[0] - 0.014 * CollarFit, F = S[1] - 0.013 * CollarFit, Back = S[2] - 0.015 * CollarFit;
		const double ChestQ = (Y - 1.025) / 0.095, WaistQ = (Y - 0.805) / 0.07;
		const double Chest = std::exp(-(ChestQ * ChestQ)), WaistFit = std::exp(-(WaistQ * WaistQ));
		const double Front = F + 0.006 * Chest - 0.004 * WaistFit;
		const double Fold = 0.0025 * std::sin(A * 6 + (Y - 0.6) * 19) * (1 - Smooth((Y - 1.08) / 0.1));
		return V3(Sp(C, 0.92) * ((C >= 0 ? Front : Back) + Out + Fold), Y, -Sp(Sn, 0.92) * (W + Out + Fold));
	}
	inline double Waist(double Y) { return Smooth((Y - 0.68) / 0.36); }

	enum class EStance : uint8_t { Stand, Stride, Brace, Kneel, Sit, Run, Prone };
	struct FLegSpec
	{
		FV Ankle, Toe, Pole, Knee;
		bool bKnee = false, bPole = false;
	};
	struct FStanceDef
	{
		FV Root;
		double Tilt;
		FLegSpec Legs[2];
	};
	inline FLegSpec LegS(FV Ankle, FV Toe, FV Pole) { FLegSpec L; L.Ankle = Ankle; L.Toe = Toe; L.Pole = Pole; L.bPole = true; return L; }
	inline FLegSpec KneeS(FV Knee, FV Ankle, FV Toe) { FLegSpec L; L.Knee = Knee; L.bKnee = true; L.Ankle = Ankle; L.Toe = Toe; return L; }
	inline const FStanceDef& Stance(EStance S)
	{
		constexpr double A = ANKLE;
		static const FStanceDef Defs[7] = {
			{ V3(0, 0.69, 0), 0.02, { LegS(V3(0.01, A, -0.1), V3(0.97, 0, -0.24), V3(1, 0, -0.1)), LegS(V3(-0.02, A, 0.1), V3(0.97, 0, 0.24), V3(1, 0, 0.1)) } },
			{ V3(0, 0.665, 0), 0.04, { LegS(V3(0.19, A, -0.11), V3(0.98, 0, -0.2), V3(1, 0, -0.1)), LegS(V3(-0.17, A, 0.12), V3(0.55, 0, 0.83), V3(0.6, 0, 0.8)) } },
			{ V3(0, 0.645, 0), 0.04, { LegS(V3(0.21, A, -0.15), V3(0.95, 0, -0.3), V3(1, 0, -0.2)), LegS(V3(-0.17, A, 0.17), V3(0.4, 0, 0.9), V3(0.5, 0, 0.9)) } },
			{ V3(-0.02, 0.37, 0), 0.05, { LegS(V3(0.27, A, -0.12), V3(0.98, 0, -0.15), V3(1, 1.2, -0.1)), KneeS(V3(-0.03, 0.05, 0.1), V3(-0.33, 0.15, 0.11), V3(0.5, -0.87, 0)) } },
			{ V3(-0.06, 0.13, 0), -0.12, { LegS(V3(0.42, A, -0.17), V3(0.9, 0.3, -0.2), V3(0.3, 1, -0.3)), LegS(V3(0.36, A, 0.2), V3(0.9, 0.3, 0.3), V3(0.3, 1, 0.4)) } },
			{ V3(0, 0.63, 0), 0.15, { LegS(V3(0.25, 0.1, -0.08), V3(0.95, -0.3, -0.05), V3(1, 0.3, 0)), LegS(V3(-0.3, 0.21, 0.09), V3(0.2, -0.98, 0), V3(1, -0.4, 0)) } },
			{ V3(-0.33, 0.11, 0), 1.44, { LegS(V3(-0.95, 0.075, -0.22), V3(-0.3, -0.9, -0.3), V3(0, -1, 0)), LegS(V3(-0.87, 0.08, 0.33), V3(-0.2, -0.9, 0.4), V3(0.1, -0.5, 1)) } },
		};
		return Defs[static_cast<int>(S)];
	}

	enum class EType : uint8_t { Rifle, Conscript, Commando, Ranger, Sniper, Engineer, Flamer, Mg, Mortar, At, Howitzer, Flak, Other };
	enum class EBase : uint8_t { Leader, Rifle, Bren, Tommy, Smg, Bazooka, Sniper, Spotter, Engineer, Flamer, Gunner, Feeder, Ammo, Loader, Layer, Shell, Carbine };
	struct FJob
	{
		EBase Base = EBase::Rifle;
		char Variant = 0; // 0, 'B' or 'C'
	};
	enum class EGun : uint8_t { None, Rifle, Smg, Scoped, Carbine, Bren, Thompson, Bazooka, Flamer };
	enum class EHold : uint8_t { None, Aim, Under, Shoulder, Hip, Port, Carry, Dropped };
	enum class EThing : uint8_t { None, Can, Round, Clip, Bomb, Binos, Belt, Shovel };
	enum class EBelt : uint8_t { None, Can, Feed };
	// posture k of the reference: 0 stand, 1 crouch, 2 prone, 3 retreat; then the corpse and the four dig keys
	enum class EPose : uint8_t { Stand, Crouch, Prone, Retreat, Fallen, Dig };

	inline const char* BaseName(EBase B)
	{
		static const char* Names[] = { "leader", "rifle", "bren", "tommy", "smg", "bazooka", "sniper", "spotter", "engineer", "flamer", "gunner", "feeder", "ammo", "loader", "layer", "shell", "carbine" };
		return Names[static_cast<int>(B)];
	}
	inline const char* TypeName(EType T)
	{
		static const char* Names[] = { "rifle", "conscript", "commando", "ranger", "sniper", "engineer", "flamer", "mg", "mortar", "at", "howitzer", "flak", "other" };
		return Names[static_cast<int>(T)];
	}
	inline EType ParseType(const char* Name)
	{
		for (int I = 0; I < static_cast<int>(EType::Other); I++) if (std::strcmp(Name, TypeName(static_cast<EType>(I))) == 0) return static_cast<EType>(I);
		// unit-models.js: medics borrow the engineer figure
		if (std::strcmp(Name, "medic") == 0) return EType::Engineer;
		return EType::Other;
	}
	inline FJob J(EBase B, char V = 0) { FJob R; R.Base = B; R.Variant = V; return R; }

	// infantry.js soldierKit(type, i, fac)
	inline FJob SoldierKit(EType Type, int I, int Fac)
	{
		switch (Type)
		{
		case EType::Flamer: return I == 0 ? J(EBase::Flamer) : I == 1 ? J(EBase::Rifle, 'B') : J(EBase::Rifle);
		case EType::Rifle: case EType::Conscript:
			if (I == 0) return J(EBase::Leader);
			if (Fac == 3 && Type == EType::Rifle && I == 4) return J(EBase::Bren);
			{
				const int M = (I - 1) % 3; // JavaScript remainder keeps the sign; negative men do not exist
				return M == 0 ? J(EBase::Rifle) : M == 1 ? J(EBase::Rifle, 'B') : J(EBase::Rifle, 'C');
			}
		case EType::Commando:
			switch (I) { case 0: return J(EBase::Leader); case 1: return J(EBase::Tommy); case 2: return J(EBase::Bren); case 3: return J(EBase::Tommy, 'B'); case 4: return J(EBase::Rifle); default: return J(EBase::Tommy); }
		case EType::Ranger: return I % 3 == 1 ? J(EBase::Bazooka) : (I % 2 ? J(EBase::Smg) : J(EBase::Smg, 'B'));
		case EType::Sniper: return I == 0 ? J(EBase::Sniper) : J(EBase::Spotter);
		case EType::Engineer: return I % 2 ? J(EBase::Engineer, 'B') : J(EBase::Engineer);
		case EType::Mg: return I == 0 ? J(EBase::Gunner) : I == 1 ? J(EBase::Feeder) : J(EBase::Ammo);
		case EType::Mortar: return I == 0 ? J(EBase::Loader) : I == 1 ? J(EBase::Ammo) : I == 2 ? J(EBase::Spotter) : J(EBase::Ammo);
		case EType::At: case EType::Howitzer: return I == 0 ? J(EBase::Layer) : I == 1 ? J(EBase::Shell) : I == 2 ? J(EBase::Ammo) : I == 3 ? J(EBase::Carbine) : J(EBase::Ammo);
		case EType::Flak: return I == 0 ? J(EBase::Shell) : I == 1 ? J(EBase::Ammo) : I == 2 ? J(EBase::Spotter) : J(EBase::Ammo);
		default: return J(EBase::Rifle);
		}
	}

	// weapon(kind, F) without its geometry: the length and where the fists and the shoulder go
	struct FGunInfo
	{
		double L = 0, Grip[2] = { 0, 0 }, Fore[2] = { 0, 0 }, Butt[2] = { 0, 0 };
	};
	enum class EModel : uint8_t { Garand, Kar98, Mosin, M1903, M1Carbine, Lee, No4t, Thompson, Mp40, Ppsh, Sten, Bren, Bazooka, M2, Fmw41, Roks2, FlameGeneric };
	inline EModel ModelOf(EGun Kind, int Fac)
	{
		// FACTIONS[fac] ?? FACTIONS[0]
		const int F = (Fac >= 0 && Fac <= 3) ? Fac : 0;
		switch (Kind)
		{
		case EGun::Rifle: { const EModel M[4] = { EModel::Garand, EModel::Kar98, EModel::Mosin, EModel::Lee }; return M[F]; }
		case EGun::Smg: { const EModel M[4] = { EModel::Thompson, EModel::Mp40, EModel::Ppsh, EModel::Sten }; return M[F]; }
		case EGun::Carbine: { const EModel M[4] = { EModel::M1Carbine, EModel::Kar98, EModel::Mosin, EModel::Lee }; return M[F]; }
		case EGun::Scoped: { const EModel M[4] = { EModel::M1903, EModel::Kar98, EModel::Mosin, EModel::No4t }; return M[F]; }
		case EGun::Flamer: { const EModel M[4] = { EModel::M2, EModel::Fmw41, EModel::Roks2, EModel::FlameGeneric }; return M[F]; }
		case EGun::Bren: return EModel::Bren;
		case EGun::Thompson: return EModel::Thompson;
		case EGun::Bazooka: return EModel::Bazooka;
		default: return EModel::Garand;
		}
	}
	inline FGunInfo GunInfo(EGun Kind, int Fac)
	{
		FGunInfo I;
		auto Put = [&I](double L, double G0, double G1, double F0, double F1, double B0, double B1)
		{ I.L = L; I.Grip[0] = G0; I.Grip[1] = G1; I.Fore[0] = F0; I.Fore[1] = F1; I.Butt[0] = B0; I.Butt[1] = B1; };
		const EModel Model = ModelOf(Kind, Fac);
		switch (Model)
		{
		case EModel::Garand: Put(0.864, 0.28, -0.036, 0.5, -0.024, 0, -0.01); break;
		case EModel::Kar98: Put(0.866, 0.28, -0.036, 0.5, -0.024, 0, -0.01); break;
		case EModel::Mosin: Put(0.96, 0.28, -0.036, 0.52, -0.024, 0, -0.01); break;
		case EModel::M1903: Put(0.86, 0.28, -0.036, 0.5, -0.024, 0, -0.01); break;
		case EModel::M1Carbine: Put(0.705, 0.28, -0.036, 0.4, -0.024, 0, -0.01); break;
		case EModel::Lee: case EModel::No4t: Put(0.885, 0.28, -0.036, 0.5, -0.024, 0, -0.01); break;
		case EModel::Bren: Put(0.9, 0.3, -0.06, 0.55, -0.03, 0, -0.02); break;
		case EModel::Sten: Put(0.6, 0.24, -0.05, 0.47, -0.014, 0, -0.03); break;
		case EModel::Bazooka: { const double L = 1.07; Put(L, 0.58 * L, -0.085, 0.76 * L, -0.075, 0.45 * L, -0.035); break; }
		case EModel::M2: case EModel::Fmw41: case EModel::Roks2: case EModel::FlameGeneric:
		{
			const double L = Model == EModel::Fmw41 ? 0.95 : Model == EModel::Roks2 ? 0.92 : 0.8;
			Put(L, Model == EModel::Roks2 ? 0.27 : 0.12, -0.05, L * 0.55, -0.04, 0, -0.01);
			break;
		}
		case EModel::Thompson: Put(0.63, 0.255, -0.065, 0.45, -0.03, 0, -0.01); break;
		case EModel::Mp40: Put(0.65, 0.24, -0.065, 0.33, -0.1, 0, -0.02); break;
		case EModel::Ppsh: Put(0.658, 0.22, -0.04, 0.42, -0.034, 0, -0.01); break;
		}
		return I;
	}

	constexpr double AIM_SHIFT[4][2] = { { 0, 0 }, { 0.05, -0.33 }, { 0.28, -0.84 }, { -0.25, 0.15 } };
	inline FV HandOf(EType T)
	{
		switch (T)
		{
		case EType::Rifle: case EType::Conscript: return V3(0.72, 1.1, 0.2);
		case EType::Engineer: return V3(0.6, 1.0, 0.2);
		case EType::Ranger: return V3(0.62, 0.9, 0.2);
		case EType::Sniper: return V3(1.13, 1.08, 0.2);
		default: return V3(0.72, 1.1, 0.2);
		}
	}
	const FV BAZ = { 0.63, 0.93, 0.25 };
	const FV FLAME = { 0.9, 0.78, 0.2 };
	inline FV Flash(const FV& H, int K) { return V3(H.X + AIM_SHIFT[K][0], H.Y + AIM_SHIFT[K][1], H.Z); }
	inline FV AnchorOf(EHold H)
	{
		switch (H)
		{
		case EHold::Aim: return V3(0.05, 1.075, 0.105);
		case EHold::Under: return V3(0.0, 0.97, 0.14);
		case EHold::Shoulder: return V3(0.0, 1.14, 0.13);
		case EHold::Hip: return V3(0.06, 0.8, 0.17);
		default: return V3(0, 0, 0);
		}
	}
	struct FMg { FV R, L, Feed, Butt; bool bButt; };
	inline const FMg& MgOf(int Fac)
	{
		static const FMg Table[3] = {
			{ V3(0.767, 0.222, 0), V3(0.82, 0.33, -0.06), V3(0.79, 0.25, -0.8), V3(0, 0, 0), false },
			{ V3(0.782, 0.244, 0), V3(0.52, 0.355, -0.03), V3(0.8, 0.28, -0.8), V3(0.43, 0.31, 0), true },
			{ V3(0.56, 0.39, 0.074), V3(0.56, 0.39, -0.074), V3(0.66, 0.44, -0.76), V3(0, 0, 0), false },
		};
		return Table[(Fac >= 0 && Fac <= 2) ? Fac : 0];
	}
	const FV MOUTH = { 0.633, 0.644, -0.819 };
	struct FDig { FV Grip, Tip; double Lean, Turn, Y; };
	inline const FDig& DigKey(int I)
	{
		static const FDig Keys[4] = {
			{ V3(0.22, 1.02, 0.2), V3(0.86, 0.3, 0.08), 0.32, -0.1, 0.66 },
			{ V3(0.24, 0.72, 0.16), V3(0.66, -0.06, 0.06), 0.6, -0.15, 0.6 },
			{ V3(0.02, 0.66, 0.2), V3(0.62, 0.2, 0.02), 0.48, -0.05, 0.62 },
			{ V3(0.12, 0.98, 0.12), V3(0.42, 0.66, -0.66), 0.18, -0.5, 0.67 },
		};
		return Keys[I < 0 ? 0 : (I > 3 ? 3 : I)];
	}
	inline bool IsDigger(EType T) { return T == EType::Rifle || T == EType::Conscript || T == EType::Engineer; }

	// ------------------------------------------------------------------ pose specs

	struct FSpec
	{
		EStance StanceKind = EStance::Stand;
		bool bTilt = false, bRoot = false, bLegs = false;
		double Tilt = 0;
		FV Root;
		FLegSpec Legs[2];
		double Lean = 0, Turn = 0, Yaw = 0;
		EGun Gun = EGun::None;
		EHold Hold = EHold::None;
		bool bTarget = false;
		FV Target;
		double Psi = 0;
		bool bSling = false, bBinos = false, bDown = false, bFallen = false, bPoint = false, bBombOver = false;
		EThing Carry = EThing::None;
		EBelt Belt = EBelt::None;
		bool bHands = false, bPoleL = false;
		FV HandR, HandL, PoleL;
		bool bPlace = false;
		FV PlaceAnchor, PlaceAt;
		double Approach = 0;
		bool bDig = false;
		FDig Dig;
	};
	inline FSpec Sp0(EStance S, double Turn, double Lean) { FSpec R; R.StanceKind = S; R.Turn = Turn; R.Lean = Lean; return R; }
	inline FSpec RunSpec() { return Sp0(EStance::Run, 0.1, 0.12); }
	inline FSpec ProneSpec() { return Sp0(EStance::Prone, -0.1, -0.42); }
	inline FSpec ProneAim(bool C) { FSpec R = Sp0(EStance::Prone, -0.05, -0.3); R.Yaw = C ? -0.16 : -0.28; return R; }
	inline void SetGun(FSpec& S, EGun Kind, EHold Hold) { S.Gun = Kind; S.Hold = Hold; S.bTarget = false; S.Psi = 0; }
	inline void SetGun(FSpec& S, EGun Kind, EHold Hold, const FV& Target, double Psi = 0) { S.Gun = Kind; S.Hold = Hold; S.bTarget = true; S.Target = Target; S.Psi = Psi; }

	struct FCtx
	{
		EType Type = EType::Rifle;
		int Fac = 0;
		FJob Job;
	};

	// infantry.js spec(ctx, k)
	inline FSpec Spec(const FCtx& Ctx, int K)
	{
		const bool B = Ctx.Job.Variant == 'B', C = Ctx.Job.Variant == 'C';
		switch (Ctx.Job.Base)
		{
		case EBase::Rifle: case EBase::Leader: case EBase::Sniper: case EBase::Engineer: case EBase::Bren: case EBase::Tommy:
		{
			const EBase Base = Ctx.Job.Base;
			const EGun Kind = Base == EBase::Leader ? EGun::Smg : Base == EBase::Sniper ? EGun::Scoped : Base == EBase::Engineer ? EGun::Carbine
				: Base == EBase::Bren ? EGun::Bren : Base == EBase::Tommy ? EGun::Thompson : EGun::Rifle;
			const FV Target = Flash(HandOf(Ctx.Type), K);
			if (K == 0)
			{
				if (C) { FSpec S = Sp0(EStance::Stand, -0.6, -0.03); SetGun(S, Kind, EHold::Aim, Target, -0.03); return S; }
				FSpec S = Sp0(Base == EBase::Leader || Base == EBase::Sniper || Base == EBase::Bren || B ? EStance::Brace : EStance::Stride, B ? -0.82 : -0.68, B ? 0.1 : 0.05);
				SetGun(S, Kind, EHold::Aim, Target, B ? 0.04 : 0);
				return S;
			}
			if (K == 1) { FSpec S = Sp0(EStance::Kneel, C ? -0.45 : -0.55, C ? 0.22 : 0.14); SetGun(S, Kind, EHold::Aim, Target); return S; }
			if (K == 2) { FSpec S = ProneAim(C); SetGun(S, Kind, EHold::Aim, Target); return S; }
			FSpec S = RunSpec(); SetGun(S, Kind, EHold::Port); return S;
		}
		case EBase::Flamer:
		{
			const FV Target = Flash(FLAME, K);
			if (K == 0) { FSpec S = Sp0(EStance::Brace, -0.35, 0.12); SetGun(S, EGun::Flamer, EHold::Hip, Target); return S; }
			if (K == 1) { FSpec S = Sp0(EStance::Kneel, -0.35, 0.14); SetGun(S, EGun::Flamer, EHold::Hip, Target); return S; }
			if (K == 2) { FSpec S = ProneAim(C); SetGun(S, EGun::Flamer, EHold::Aim, Add(Target, V3(0, 0.1, 0))); return S; }
			FSpec S = RunSpec(); SetGun(S, EGun::Flamer, EHold::Port); return S;
		}
		case EBase::Smg:
		{
			const FV Target = Flash(HandOf(EType::Ranger), K);
			if (K == 0) { FSpec S = Sp0(B ? EStance::Brace : EStance::Stride, -0.3, 0.1); SetGun(S, EGun::Smg, EHold::Under, Target, B ? 0.06 : 0); return S; }
			if (K == 1) { FSpec S = Sp0(EStance::Kneel, -0.35, 0.12); SetGun(S, EGun::Smg, EHold::Under, Target); return S; }
			if (K == 2) { FSpec S = ProneAim(C); SetGun(S, EGun::Smg, EHold::Aim, Add(Target, V3(0, 0.1, 0))); return S; }
			FSpec S = RunSpec(); SetGun(S, EGun::Smg, EHold::Port); return S;
		}
		case EBase::Bazooka:
		{
			if (K == 0) { FSpec S = Sp0(EStance::Kneel, -0.2, 0.06); SetGun(S, EGun::Bazooka, EHold::Shoulder, BAZ); return S; }
			if (K == 1) { FSpec S = Sp0(EStance::Sit, -0.15, 0.3); SetGun(S, EGun::Bazooka, EHold::Shoulder, Add(BAZ, V3(AIM_SHIFT[1][0], AIM_SHIFT[1][1], 0))); return S; }
			if (K == 2) { FSpec S = ProneSpec(); S.Lean = -0.5; SetGun(S, EGun::Bazooka, EHold::Shoulder, Add(BAZ, V3(AIM_SHIFT[2][0], AIM_SHIFT[2][1] + 0.12, 0))); return S; }
			FSpec S = RunSpec(); SetGun(S, EGun::Bazooka, EHold::Carry); return S;
		}
		case EBase::Carbine:
		{
			if (K == 0) { FSpec S = Sp0(EStance::Stand, -0.5, 0.06); SetGun(S, EGun::Carbine, EHold::Aim, V3(0.45, 0.78, 0.12), 0.35); return S; }
			if (K == 1) { FSpec S = Sp0(EStance::Kneel, -0.55, 0.14); SetGun(S, EGun::Carbine, EHold::Aim, V3(0.75, 0.76, 0.2)); return S; }
			if (K == 2) { FSpec S = ProneAim(C); SetGun(S, EGun::Carbine, EHold::Aim, V3(1.0, 0.26, 0.2)); return S; }
			FSpec S = RunSpec(); SetGun(S, EGun::Carbine, EHold::Port); return S;
		}
		case EBase::Spotter:
		{
			FSpec S;
			if (K == 0) S = Sp0(Ctx.Type == EType::Sniper ? EStance::Kneel : EStance::Stand, 0.1, 0.05);
			else if (K == 1) S = Sp0(EStance::Kneel, 0.1, 0.1);
			else if (K == 2) { S = ProneSpec(); S.Lean = -0.5; S.Turn = 0; }
			else { S = RunSpec(); S.bSling = true; S.Carry = EThing::Binos; return S; }
			S.bBinos = true; S.bSling = true;
			return S;
		}
		case EBase::Ammo:
		{
			FSpec S;
			if (K == 0) { S = Sp0(EStance::Stand, 0.05, 0); }
			else if (K == 1) { S = Sp0(EStance::Kneel, 0.15, 0.55); S.bDown = true; }
			else if (K == 2) { S = ProneSpec(); S.Turn = 0; S.bDown = true; }
			else S = RunSpec();
			S.bSling = true; S.Carry = EThing::Can;
			return S;
		}
		case EBase::Shell:
		{
			const EThing What = Ctx.Type == EType::Flak ? EThing::Clip : EThing::Round;
			FSpec S;
			if (K == 0) S = Sp0(EStance::Stand, -0.1, 0.04);
			else if (K == 1) S = Sp0(EStance::Kneel, -0.1, 0.12);
			else if (K == 2) { S = ProneSpec(); S.Turn = 0; S.bDown = true; }
			else S = RunSpec();
			S.bSling = true; S.Carry = What;
			return S;
		}
		case EBase::Layer:
		{
			FSpec S;
			if (K < 2) { S = Sp0(EStance::Kneel, -0.15, 0.12); S.bPoint = true; }
			else if (K == 2) { S = ProneSpec(); S.Turn = 0; S.Lean = -0.5; }
			else S = RunSpec();
			S.bSling = true;
			return S;
		}
		case EBase::Gunner:
		{
			if (K == 3) { FSpec S = RunSpec(); S.bSling = true; return S; }
			const FMg& G = MgOf(Ctx.Fac);
			FSpec S = ProneSpec();
			S.bSling = true; S.bHands = true; S.HandR = G.R; S.HandL = G.L;
			if (G.bButt)
			{
				S.Lean = -0.5; S.Turn = -0.1; S.bPoleL = true; S.PoleL = V3(0.2, -1, -0.6);
				S.bPlace = true; S.PlaceAnchor = AnchorOf(EHold::Aim); S.PlaceAt = G.Butt;
				return S;
			}
			S.Lean = Ctx.Fac == 2 ? -0.75 : -0.5; S.Turn = 0; S.Approach = 0.36;
			return S;
		}
		case EBase::Feeder:
		{
			if (K == 3) { FSpec S = RunSpec(); S.bSling = true; S.Carry = EThing::Can; S.Belt = EBelt::Can; return S; }
			const FMg& G = MgOf(Ctx.Fac);
			FSpec S;
			if (K == 2) { S = ProneSpec(); S.Lean = -0.6; }
			else S = Sp0(EStance::Kneel, -0.2, 0.7);
			S.Yaw = 0.8; S.bSling = true; S.Carry = EThing::Can; S.bDown = true; S.Belt = EBelt::Feed;
			S.bHands = true; S.HandR = Add(G.Feed, V3(-0.1, -0.03, 0.12)); S.HandL = Add(G.Feed, V3(-0.02, 0, 0.05));
			S.Approach = K == 2 ? 0.34 : 0.24;
			return S;
		}
		case EBase::Loader:
		{
			if (K == 3) { FSpec S = RunSpec(); S.bSling = true; S.Carry = EThing::Bomb; return S; }
			if (K == 2) { FSpec S = ProneSpec(); S.Turn = 0; S.bSling = true; S.Carry = EThing::Bomb; S.bDown = true; return S; }
			FSpec S = Sp0(EStance::Kneel, -0.1, 0.12);
			S.Yaw = 0.91; S.bSling = true; S.Carry = EThing::Bomb; S.bBombOver = true; S.Approach = 0.34;
			return S;
		}
		default: { FSpec S = Sp0(EStance::Stand, 0, 0); S.bSling = true; return S; }
		}
	}

	// infantry.js fallenSpec(ctx): the corpse, chest in the dirt, weapon dropped beside him
	inline FSpec FallenSpec(const FCtx& Ctx)
	{
		FSpec S = Sp0(EStance::Prone, 0.06, 0.5);
		S.Yaw = 0.35; S.bFallen = true;
		EGun Gun = EGun::None;
		switch (Ctx.Job.Base)
		{
		case EBase::Rifle: Gun = EGun::Rifle; break;
		case EBase::Leader: Gun = EGun::Smg; break;
		case EBase::Sniper: Gun = EGun::Scoped; break;
		case EBase::Engineer: Gun = EGun::Carbine; break;
		case EBase::Smg: Gun = EGun::Smg; break;
		case EBase::Bazooka: Gun = EGun::Bazooka; break;
		case EBase::Carbine: Gun = EGun::Carbine; break;
		case EBase::Flamer: Gun = EGun::Flamer; break;
		case EBase::Bren: Gun = EGun::Bren; break;
		case EBase::Tommy: Gun = EGun::Thompson; break;
		default: break;
		}
		if (Gun != EGun::None) { SetGun(S, Gun, EHold::Dropped); return S; }
		const EBase Base = Ctx.Job.Base;
		const EThing Carry = Base == EBase::Spotter ? EThing::Binos : (Base == EBase::Ammo || Base == EBase::Feeder) ? EThing::Can
			: Base == EBase::Shell ? (Ctx.Type == EType::Flak ? EThing::Clip : EThing::Round) : Base == EBase::Loader ? EThing::Bomb : EThing::None;
		S.bSling = true;
		if (Carry != EThing::None) { S.Carry = Carry; S.bDown = true; }
		if (Base == EBase::Feeder) S.Belt = EBelt::Can;
		return S;
	}
	// infantry.js digSpec(ctx, i)
	inline FSpec DigSpec(const FCtx& Ctx, int I)
	{
		FSpec S = FallenSpec(Ctx);
		const FDig& D = DigKey(I);
		S.StanceKind = EStance::Brace; S.bFallen = false; S.Yaw = 0; S.Lean = D.Lean; S.Turn = D.Turn;
		S.bRoot = true; S.Root = V3(0, D.Y, 0); S.bDig = true; S.Dig = D;
		return S;
	}

	// ------------------------------------------------------------------ the body and its limbs

	struct FBody
	{
		FM F, B, U;
		double Lean = 0, Turn = 0;
		FLegSpec Legs[2];
		// M(t): t of the spine's lean and turn, quantized to percent as the reference's cache key
		FM Mt(double T) const
		{
			const double K = JsRound(T * 100);
			return Chain(B, Tr(0, HIP, 0), RotZ(-Lean * (K / 100)), RotY(Turn * (K / 100)), Tr(0, -HIP, 0));
		}
		FM On(double Y) const { return Mt(Waist(Y)); }
		FV At(const FV& P) const { return Apply(Mt(Waist(P.Y)), P); }
	};
	inline FBody MakeBody(const FSpec& S, const FM& F)
	{
		const FStanceDef& St = Stance(S.StanceKind);
		FBody Body;
		Body.F = F; Body.Lean = S.Lean; Body.Turn = S.Turn;
		for (int I = 0; I < 2; I++) Body.Legs[I] = S.bLegs ? S.Legs[I] : St.Legs[I];
		Body.B = Chain(F, Tr(S.bRoot ? S.Root : St.Root), RotZ(-(S.bTilt ? S.Tilt : St.Tilt)), Tr(0, -HIP, 0));
		Body.U = Body.Mt(1);
		return Body;
	}
	// two-bone IK: the knee (or elbow) for a hip and ankle, bending toward pole
	inline FV Bend(const FV& A, const FV& C, double L1, double L2, const FV& Pole)
	{
		const FV D = Sub(C, A);
		const double DL = Length(D), Max = (L1 + L2) * 0.999, L = DL < Max ? DL : Max;
		const FV U = Normalize(D);
		const double X = (L1 * L1 - L2 * L2 + L * L) / (2 * L);
		const double HSq = L1 * L1 - X * X, H = std::sqrt(HSq > 0 ? HSq : 0);
		FV P = AddScaled(Pole, U, -Dot(Pole, U));
		if (LengthSq(P) < 1e-8) P = Cross(U, V3(0, 0, 1));
		return AddScaled(AddScaled(A, U, X), Normalize(P), H);
	}
	struct FLeg { FV Hip, Knee, Ankle, Toe, Pole; };
	struct FArm { FV Sho, Elbow, Hand; double Err = 0; };
	inline FLeg SolveLeg(const FBody& Body, double Side, const FLegSpec& L)
	{
		FLeg R;
		R.Hip = Apply(Body.B, V3(0, HIP, Side * HIPZ));
		FV Ankle = Apply(Body.F, L.Ankle);
		const FV PoleSpec = L.bPole ? L.Pole : V3(1, 0, 0);
		if (L.bKnee)
		{
			R.Knee = Add(R.Hip, SetLength(Sub(Apply(Body.F, L.Knee), R.Hip), THIGH));
			Ankle = Add(R.Knee, SetLength(Sub(Ankle, R.Knee), SHIN));
		}
		else
		{
			const double Max = (THIGH + SHIN) * 0.995;
			if (Distance(R.Hip, Ankle) > Max) Ankle = Add(R.Hip, SetLength(Sub(Ankle, R.Hip), Max));
			R.Knee = Bend(R.Hip, Ankle, THIGH, SHIN, Dir(Body.F, PoleSpec));
		}
		R.Ankle = Ankle;
		R.Toe = Dir(Body.F, L.Toe);
		R.Pole = Dir(Body.F, PoleSpec);
		return R;
	}
	inline FArm SolveArm(const FBody& Body, double Side, const FV& Hand, const FV& Pole)
	{
		FArm R;
		R.Sho = Apply(Body.U, V3(SHO[0], SHO[1], Side * SHO[2]));
		const double L2 = FORE + FIST, D = Distance(R.Sho, Hand), Max = (UPPER + L2) * 0.999;
		R.Hand = Hand;
		R.Elbow = Bend(R.Sho, Hand, UPPER, D > Max ? D / 0.999 - UPPER : L2, Pole);
		R.Err = D - Max > 0 ? D - Max : 0;
		return R;
	}
	inline FM HeadMatrix(const FBody& Body, const FV& DirIn, const FV& Jut, double Roll)
	{
		const FV D = Normalize(Apply(Invert(ExtractRotation(Body.U)), DirIn));
		const double Look = Clamp(std::atan2(-D.Z, D.X), -1.1, 1.1);
		const double Nod = Clamp(-std::atan2(D.Y, std::hypot(D.X, D.Z)), -1.3, 0.8);
		return Chain(Body.U, Tr(Add(PIVOT, Jut)), RotY(Look), RotZ(-Nod), RotX(Roll), Tr(Sub(HEADP, PIVOT)));
	}

	struct FThing { EThing Kind = EThing::None; FM M; };
	struct FRig
	{
		FCtx Ctx;
		FSpec S;
		FBody Body;
		FLeg Legs[2];
		FArm Arms[2];
		FM H, M0;
		bool bW = false;
		FM W;
		FGunInfo Info;
		bool bSling = false;
		FM Sling;
		int NumThings = 0;
		FThing Things[4];
	};

	// infantry.js solve(ctx, k, phase)
	inline FRig Solve(const FCtx& Ctx, EPose Pose, int DigKeyIndex, bool bPhase, double Phase)
	{
		FSpec S = Pose == EPose::Fallen ? FallenSpec(Ctx) : Pose == EPose::Dig ? DigSpec(Ctx, DigKeyIndex) : Spec(Ctx, static_cast<int>(Pose));
		if (Pose == EPose::Dig) bPhase = false;
		const int K = static_cast<int>(Pose);
		if (bPhase && Pose != EPose::Fallen)
		{
			// Each boot spends half a cycle planted, travelling back as the body advances, then swings forward.
			const bool bLow = K == 1, bCrawl = K == 2;
			S.bRoot = true;
			S.Root = bCrawl ? V3(-0.33 + 0.025 * std::sin(Phase * OnePi * 2), 0.11, 0) : V3(0, (bLow ? 0.47 : 0.61) + 0.018 * std::cos(Phase * OnePi * 4), 0);
			if (bLow || K == 0) { S.StanceKind = EStance::Stride; S.bTilt = true; S.Tilt = bLow ? 0.18 : 0.15; }
			S.Turn = (K == 0 ? S.Turn : bCrawl ? -0.1 : 0.1) + (bCrawl || K == 0 ? 0.04 : 0.08) * std::sin(Phase * OnePi * 2);
			S.bLegs = true;
			for (int I = 0; I < 2; I++)
			{
				const double Side = I == 0 ? -1 : 1;
				const double T = std::fmod(Phase + I * 0.5, 1.0);
				const double SwingRaw = std::sin((T - 0.5) * OnePi * 2), Swing = SwingRaw > 0 ? SwingRaw : 0;
				if (bCrawl)
				{
					S.Legs[I] = LegS(V3(-0.92 + 0.12 * std::cos(T * OnePi * 2), 0.075 + 0.025 * Swing, Side * (0.22 + 0.06 * Swing)), V3(-0.3, -0.9, Side * 0.3), V3(0.2, -1, Side * 0.2));
					continue;
				}
				const double Reach = bLow ? 0.19 : 0.31;
				const double X = T < 0.5 ? Reach - 4 * Reach * T : -Reach + 2 * Reach * Smooth((T - 0.5) * 2);
				S.Legs[I] = LegS(V3(X, ANKLE + (bLow ? 0.08 : 0.13) * Swing, Side * 0.1), V3(1, 0.18 * Swing, 0), V3(1, 0.15, Side * 0.08));
			}
		}
		FRig R;
		R.Ctx = Ctx;
		const bool bGun = S.Gun != EGun::None;
		if (bGun) R.Info = GunInfo(S.Gun, Ctx.Fac);
		const EHold Hold = S.Hold;
		FM M0 = RotY(S.Yaw);
		FBody Body = MakeBody(S, M0);
		auto Shift = [&](const FV& D) { M0 = MatMul(Tr(D.X, 0, D.Z), M0); Body = MakeBody(S, M0); };
		bool bW = false;
		FM W = Identity();
		if (Hold == EHold::Dropped) { W = Chain(M0, Tr(0.05, 0.02, 0.46), RotY(-HalfPi), RotZ(0.12)); bW = true; }
		else if (bGun && S.bTarget)
		{
			// the muzzle on the target, the butt on the body's anchor for the hold, the body moved to fit
			const FGunInfo& Info = R.Info;
			const FV Aw = Apply(Body.U, AnchorOf(Hold));
			const double L = Info.L, A = L - Info.Butt[0], YA = Info.Butt[1];
			const double P = std::atan2(YA, A) + std::asin(Clamp((S.Target.Y - Aw.Y) / std::hypot(A, YA), -1, 1));
			const FM Rot = Chain(RotY(S.Psi), RotZ(P));
			const FV ButtAt = Sub(S.Target, Apply(Rot, V3(L, 0, 0)));
			W = Chain(Tr(ButtAt), Rot);
			bW = true;
			Shift(Sub(Apply(W, V3(Info.Butt[0], Info.Butt[1], 0)), Aw));
		}
		if (S.bPlace) Shift(Sub(S.PlaceAt, Apply(Body.U, S.PlaceAnchor)));
		// hands that must reach a point: the shoulders `approach` back from it, facing the man's own way
		const FV Facing = Dir(M0, V3(1, 0, 0));
		bool bHandsAt = false, bBomb = false;
		FV HandsAt, BombAt;
		if (S.bHands) { HandsAt = Mul(Add(S.HandR, S.HandL), 0.5); bHandsAt = true; }
		if (S.bBombOver) { BombAt = Add(MOUTH, V3(0, 0.05, 0)); bBomb = true; HandsAt = Add(BombAt, V3(0, 0.15, 0)); bHandsAt = true; }
		if (S.Approach != 0 && bHandsAt) Shift(Sub(AddScaled(HandsAt, Facing, -S.Approach), Apply(Body.U, V3(SHO[0], SHO[1], 0))));
		const FM U = Body.U;
		auto Rot = [&U](const FV& D) { return Dir(U, D); };
		FLeg Legs[2];
		for (int I = 0; I < 2; I++) Legs[I] = SolveLeg(Body, I ? 1 : -1, Body.Legs[I]);
		const bool bProne = S.StanceKind == EStance::Prone;
		auto Neck = [&U]() { return Apply(U, V3(SHO[0], SHO[1] + 0.12, 0)); };
		if (Hold == EHold::Port) { W = Chain(U, Tr(0.15, 0.86, 0.1), RotY(1.1), RotZ(0.85), Tr(-R.Info.Grip[0], -R.Info.Grip[1], 0)); bW = true; }
		if (Hold == EHold::Carry) { W = Chain(U, Tr(0.0, 1.16, 0.13), RotZ(0.18), Tr(-R.Info.Butt[0], -R.Info.Butt[1], 0)); bW = true; }
		int NumThings = 0;
		FThing Things[4];
		auto Push = [&](EThing Kind, const FM& M) { if (NumThings < 4) { Things[NumThings].Kind = Kind; Things[NumThings].M = M; NumThings++; } };
		bool bSling = false;
		FM Sling = Identity();
		if (S.bSling)
		{
			const FV ButtAt = Body.At(V3(-0.13, 0.62, 0.13)), Top = Body.At(V3(-0.14, 1.4, -0.15));
			Sling = Basis(ButtAt, Sub(Top, ButtAt), Rot(V3(-1, 0, 0)));
			bSling = true;
		}
		bool bR = false, bL = false;
		FV HR, HL;
		FV PR = Rot(V3(-0.3, -0.5, 1)), PL = Rot(V3(0, -1, -0.4)), Gaze = Facing;
		if (bW && Hold != EHold::Carry)
		{
			// lying down the left hand slides back along the fore-end so the elbow can rest on the ground
			HR = Apply(W, V3(R.Info.Grip[0], R.Info.Grip[1], 0)); bR = true;
			HL = Apply(W, V3(R.Info.Fore[0] - (bProne && Hold == EHold::Aim ? 0.09 : 0), R.Info.Fore[1], 0)); bL = true;
			Gaze = Dir(W, V3(1, 0, 0));
			if (Hold == EHold::Aim) { PR = bProne ? V3(0, -1, 0.5) : Rot(V3(-0.1, -0.35, 1)); PL = bProne ? V3(0.3, -1, -0.3) : Rot(V3(0.1, -1, -0.3)); }
			if (Hold == EHold::Port) { Gaze = Add(Facing, V3(0, -0.1, 0)); PR = Rot(V3(-0.4, -0.6, 1)); PL = Rot(V3(-0.2, -1, -0.6)); }
			if (Hold == EHold::Under || Hold == EHold::Hip) { PR = Rot(V3(-1, -0.3, 0.6)); PL = Rot(V3(0, -1, -0.6)); }
			if (Hold == EHold::Shoulder) { PR = Rot(V3(0, -1, 0.6)); PL = Rot(V3(0, -1, -0.5)); }
		}
		else if (Hold == EHold::Carry)
		{
			HR = Apply(W, V3(R.Info.Grip[0], R.Info.Grip[1], 0)); bR = true;
			HL = Apply(U, V3(0.08, 0.84, -0.2)); bL = true;
			PL = Rot(V3(-1, 0, -0.3)); PR = Rot(V3(0, -1, 0.5));
		}
		else if (S.bHands)
		{
			HR = S.HandR; HL = S.HandL; bR = bL = true;
			PR = V3(0.2, -1, 0.6); PL = S.bPoleL ? S.PoleL : V3(0.2, -1, -0.6);
			Gaze = bProne ? Facing : Sub(Mul(Add(HR, HL), 0.5), Neck());
		}
		// the head first (binoculars sit at the eyes), then the free hands
		const FV Look = S.bFallen ? Dir(M0, V3(0.2, -0.35, 0.9)) : S.bBinos ? Add(Facing, V3(0, bProne ? 0.1 : 0.05, 0)) : bBomb ? Sub(BombAt, Neck()) : Gaze;
		FM H;
		if (S.bFallen) H = HeadMatrix(Body, Look, V3(0.01, 0.01, 0.04), 0.35);
		else if (Hold == EHold::Aim) H = HeadMatrix(Body, Look, V3(0.035, -0.02, 0.05), 0.22);
		else H = HeadMatrix(Body, Look, V3(0, 0, 0), 0);
		if (S.bBinos)
		{
			Push(EThing::Binos, Chain(H, Tr(0.095, 0, 0), RotZ(0.05)));
			HR = Apply(H, V3(0.11, -0.012, 0.042)); HL = Apply(H, V3(0.11, -0.012, -0.042)); bR = bL = true;
			PR = Rot(V3(0, -1, 0.8)); PL = Rot(V3(0, -1, -0.8));
		}
		if (S.Carry != EThing::None && !S.bDown && !S.bBinos)
		{
			if (S.Carry == EThing::Can || S.Carry == EThing::Binos)
			{
				HR = Apply(U, V3(S.StanceKind == EStance::Run ? -0.12 : 0.02, 0.66, 0.2)); bR = true; PR = Rot(V3(-1, 0, 0.3));
				Push(S.Carry, S.Carry == EThing::Can ? Tr(Add(HR, V3(0, -0.02, 0))) : Chain(Tr(Add(HR, V3(0.01, -0.04, 0))), RotZ(-1.3)));
			}
			else if (S.Carry == EThing::Round)
			{
				Push(EThing::Round, Chain(U, Tr(0.2, 0.92, 0), RotY(HalfPi - 0.25), RotZ(0.12)));
				HR = Apply(U, V3(0.18, 0.88, 0.11)); HL = Apply(U, V3(0.21, 0.89, -0.11)); bR = bL = true;
				PR = Rot(V3(-0.5, -1, 0.6)); PL = Rot(V3(-0.5, -1, -0.6));
			}
			else if (S.Carry == EThing::Clip)
			{
				Push(EThing::Clip, Chain(U, Tr(0.21, 0.98, 0), RotZ(-0.1)));
				HR = Apply(U, V3(0.19, 0.93, 0.085)); HL = Apply(U, V3(0.2, 0.96, -0.085)); bR = bL = true;
				PR = Rot(V3(-0.5, -1, 0.6)); PL = Rot(V3(-0.5, -1, -0.6));
			}
			else if (S.Carry == EThing::Bomb)
			{
				if (bBomb)
				{
					// fins down over the mortar's mouth, about to let go
					Push(EThing::Bomb, Tr(BombAt));
					const FV Side = Normalize(Cross(V3(0, 1, 0), Facing));
					HR = AddScaled(AddScaled(Add(BombAt, V3(0, 0.17, 0)), Side, 0.04), Facing, -0.02);
					HL = AddScaled(AddScaled(Add(BombAt, V3(0, 0.13, 0)), Side, -0.04), Facing, -0.02);
					bR = bL = true;
					PR = Rot(V3(0, -1, 1)); PL = Rot(V3(0, -1, -1));
				}
				else
				{
					Push(EThing::Bomb, Chain(U, Tr(0.19, 0.78, 0.02), RotZ(-0.5)));
					HR = Apply(U, V3(0.16, 0.86, 0.1)); HL = Apply(U, V3(0.17, 0.92, -0.06)); bR = bL = true;
					PR = Rot(V3(-0.5, -1, 0.6)); PL = Rot(V3(-0.5, -1, -0.6));
				}
			}
		}
		else if (S.Carry != EThing::None && S.bDown)
		{
			// set down on the ground beside him
			const FV G = Apply(M0, bProne ? V3(0.45, 0, 0.32) : V3(0.25, 0, 0.23));
			const FM Mg = Chain(SetPosition(M0, G), RotY(bProne ? 0.3 : 0.6));
			FM Lift = Identity();
			switch (S.Carry)
			{
			case EThing::Can: Lift = Tr(0, 0.155, 0); break;
			case EThing::Round: Lift = Chain(Tr(0, 0.03, 0), RotY(0.4)); break;
			case EThing::Clip: Lift = Chain(Tr(0, 0.075, 0), RotZ(HalfPi)); break;
			case EThing::Bomb: Lift = Chain(Tr(0, 0.032, 0), RotZ(HalfPi)); break;
			case EThing::Binos: Lift = Tr(0, 0.02, 0); break;
			default: break;
			}
			Push(S.Carry, Chain(Mg, Lift));
			if (!S.bHands)
			{
				if (bProne) { HR = Apply(M0, V3(0.36, 0.06, 0.24)); HL = Apply(M0, V3(0.38, 0.06, -0.16)); bR = bL = true; PR = V3(0, -1, 0.5); PL = V3(0, -1, -0.5); }
				else { HR = Add(G, V3(0, 0.17, 0)); bR = true; PR = Rot(V3(-0.5, -0.4, 1)); }
			}
		}
		if (S.bPoint) { HR = Apply(U, V3(0.5, 1.12, 0.22)); PR = Rot(V3(0, -1, 0.5)); HL = Add(Legs[0].Knee, V3(0, 0.06, 0)); PL = Rot(V3(-0.3, -0.3, -1)); bR = bL = true; }
		if (!bR)
		{
			if (bProne) { HR = Apply(M0, V3(0.3, 0.05, 0.14)); PR = V3(0, -1, 0.5); }
			else if (S.StanceKind == EStance::Run) { HR = Apply(U, V3(!bPhase ? -0.14 : -0.05 - 0.16 * std::sin(Phase * OnePi * 2), 0.78, 0.2)); PR = Rot(V3(-1, 0, 0.3)); }
			else { HR = Apply(Body.On(0.64), V3(0.02, 0.64, 0.21)); PR = Rot(V3(-1, 0, 0.3)); }
			bR = true;
		}
		if (!bL)
		{
			if (bProne) { HL = Apply(M0, V3(0.32, 0.05, -0.16)); PL = V3(0, -1, -0.5); }
			else if (S.StanceKind == EStance::Run) { HL = Apply(U, V3(!bPhase ? 0.2 : 0.05 + 0.16 * std::sin(Phase * OnePi * 2), 0.84, -0.17)); PL = Rot(V3(-1, -0.3, -0.3)); }
			else { HL = Apply(Body.On(0.64), V3(0.02, 0.64, -0.21)); PL = Rot(V3(-1, 0, -0.3)); }
			bL = true;
		}
		// the MG belt from the can to the feed (or folded on the can)
		if (S.Belt != EBelt::None)
		{
			const FThing* Can = nullptr;
			for (int I = 0; I < NumThings; I++) if (Things[I].Kind == EThing::Can) { Can = &Things[I]; break; }
			if (Can)
			{
				const FV From = Apply(Can->M, V3(0, -0.02, 0));
				const FV To = S.Belt == EBelt::Feed ? HL : Add(From, V3(0.01, 0.005, 0));
				const double Len = Distance(From, To);
				Push(EThing::Belt, MatMul(Basis(From, Sub(To, From), V3(0, 1, 0)), Scale(Len > 0.01 ? Len : 0.01, 1, 1)));
			}
		}
		if (S.bFallen)
		{
			// one hand by the helmet, the other flung out along the ground
			const double Reach = UPPER + FORE + FIST - 0.05;
			auto ShoAt = [&U](double Side) { return Apply(U, V3(SHO[0], SHO[1], Side * SHO[2])); };
			auto Put = [Reach](const FV& At, FV D) { if (Length(D) > Reach) D = SetLength(D, Reach); return Add(At, D); };
			HL = Put(ShoAt(-1), V3(0.16, 0.06, -0.28));
			HR = Put(ShoAt(1), V3(0.1, 0.05, 0.38));
			PL = V3(0.2, -0.2, -1); PR = V3(0.15, -0.2, 1);
		}
		if (IsDigger(Ctx.Type))
		{
			if (S.bDig)
			{
				// right fist on the T-grip, left hand down the shaft
				const FV Grip = Apply(M0, S.Dig.Grip), Tip = Apply(M0, S.Dig.Tip), Along = Sub(Tip, Grip);
				Push(EThing::Shovel, MatMul(Basis(Grip, Along, V3(0, 1, 0)), Scale(Length(Along) / 0.89, 1, 1)));
				HR = Grip; HL = AddScaled(Grip, Along, 0.32);
				PR = Rot(V3(-0.4, -0.6, 1)); PL = Rot(V3(0, -1, -0.6));
			}
			else Push(EThing::Shovel, Chain(Body.On(0.8), Tr(TorsoAt(0.8, -HalfPi, 0.03)), Scale(0.01, 0.01, 0.01)));
		}
		R.S = S;
		R.Body = Body;
		R.Legs[0] = Legs[0]; R.Legs[1] = Legs[1];
		R.Arms[0] = SolveArm(Body, -1, HL, PL);
		R.Arms[1] = SolveArm(Body, 1, HR, PR);
		R.H = H; R.M0 = M0;
		R.bW = bW; R.W = W;
		R.bSling = bSling; R.Sling = Sling;
		R.NumThings = NumThings;
		for (int I = 0; I < NumThings; I++) R.Things[I] = Things[I];
		return R;
	}

	// ------------------------------------------------------------------ the fitted skin (infantry-authored.js)

	constexpr int NumDeform = 15;
	// infantry-authored-data.js (Blender 5.2.2 LTS, source infantry-retopo-v3.glb sha256 aaa7eb91...cff5): bone
	// order, rest lengths and rest frames (column-major), written by tools/model export and copied here verbatim
	constexpr double RestLengths[NumDeform] = { 0.18000000233451524, 0.26999998108104356, 0.10000002216547482, 0.35397455064964506, 0.30019494691815946, 0.12999999704269258, 0.22588938732506367, 0.19984990777993497, 0.11589651224052316, 0.34486226460005664, 0.30137021574840167, 0.12999999704269258, 0.21455073613820494, 0.2000599477644009, 0.11739677800942688 };
	constexpr double RestColumnMajor[NumDeform][16] = {
		{ 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.699999988079071, 0.0, 1.0 },
		{ 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.8799999952316284, 0.0, 1.0 },
		{ 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.08500000089406967, 1.2699999809265137, 0.00800000037997961, 1.0 },
		{ -0.27403101325035095, -0.9605209231376648, -0.04802604392170906, 0.0, 0.9617208242416382, -0.2736890912055969, -0.013684453442692757, 0.0, -9.31322685637781e-10, -0.049937620759010315, 0.9987523555755615, 0.0, -0.02500000037252903, 0.699999988079071, -0.10000000149011612, 1.0 },
		{ -0.07661686092615128, -0.9926882982254028, -0.09327271580696106, 0.0, 0.9970605969429016, -0.076280876994133, -0.007167330011725426, 0.0, 0.0, -0.09354768693447113, 0.9956148266792297, 0.0, -0.12200000137090683, 0.36000001430511475, -0.11699999868869781, 1.0 },
		{ 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -0.14499999582767487, 0.06199999898672104, -0.14499999582767487, 1.0 },
		{ -0.004426954779773951, -0.8853893280029297, -0.4648292362689972, 0.0, 0.9999902844429016, -0.003919617272913456, -0.002057798206806183, 0.0, -2.3283061589829401e-10, -0.4648337662220001, 0.8853979706764221, 0.0, -0.035999998450279236, 1.100000023841858, -0.1550000011920929, 1.0 },
		{ -0.11008261144161224, -0.9006756544113159, -0.42031553387641907, 0.0, 0.9939224720001221, -0.09975499659776688, -0.04655235633254051, 0.0, 0.0, -0.4228856563568115, 0.9061830639839172, 0.0, -0.03700000047683716, 0.8999999761581421, -0.25999999046325684, 1.0 },
		{ 0.05177033320069313, -0.9491227269172668, -0.31062179803848267, 0.0, 0.9986590147018433, 0.04920237883925438, 0.016102587804198265, 0.0, -9.313225746154785e-10, -0.31103888154029846, 0.9503971934318542, 0.0, -0.05900000035762787, 0.7200000286102295, -0.3440000116825104, 1.0 },
		{ -0.0782921314239502, -0.9859008193016052, 0.14788511395454407, 0.0, 0.9969304800033569, -0.07742594182491302, 0.011613890528678894, 0.0, 0.0, 0.14834043383598328, 0.9889363050460815, 0.0, -0.02500000037252903, 0.699999988079071, 0.10000000149011612, 1.0 },
		{ -0.12609077990055084, -0.9888170957565308, 0.07963629066944122, 0.0, 0.9920187592506409, -0.12568384408950806, 0.010122190229594707, 0.0, 9.313225746154785e-10, 0.08027701079845428, 0.9967726469039917, 0.0, -0.052000001072883606, 0.36000001430511475, 0.1509999930858612, 1.0 },
		{ 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -0.09000000357627869, 0.06199999898672104, 0.17499999701976776, 1.0 },
		{ 0.07457443326711655, -0.9321807026863098, 0.3542286157608032, 0.0, 0.9972154498100281, 0.06971096247434616, -0.026490163058042526, 0.0, 1.862645149230957e-9, 0.35521772503852844, 0.9347836375236511, 0.0, 0.03500000014901161, 1.100000023841858, 0.17299999296665192, 1.0 },
		{ 0.14995506405830383, -0.8997300863265991, 0.40987715125083923, 0.0, 0.9886927604675293, 0.13646207749843597, -0.06216607615351677, 0.0, 3.725290742551124e-9, 0.41456472873687744, 0.9100199341773987, 0.0, 0.050999999046325684, 0.8999999761581421, 0.24899999797344208, 1.0 },
		{ 0.2470254898071289, -0.936993420124054, 0.24702562391757965, 0.0, 0.9690090417861938, 0.23886390030384064, -0.06297323107719421, 0.0, 0.0, 0.25492605566978455, 0.966960608959198, 0.0, 0.08100000023841858, 0.7200000286102295, 0.3310000002384186, 1.0 },
	};
	inline FM Rest(int I) { FM M; for (int K = 0; K < 16; K++) M.E[K] = RestColumnMajor[I][K]; return M; }

	// infantry-authored.js transforms(r): the skinning matrices (pose times inverse rest) of the 15 bones
	inline void SkinMatrices(const FRig& R, FM Out[NumDeform])
	{
		Out[0] = R.Body.B;
		Out[1] = R.Body.U;
		Out[2] = MatMul(R.H, Tr(-0.085, -1.27, -0.008));
		int Index = 3;
		auto Fitted = [&](const FV& A, const FV& B, const FV& Up)
		{
			const double S = Distance(A, B) / RestLengths[Index];
			Out[Index] = MatMul(ScaleColumns(Frame(A, B, Up), S, 1, 1), Invert(Rest(Index)));
			Index++;
		};
		for (int Side = 0; Side < 2; Side++)
		{
			const FLeg& L = R.Legs[Side];
			const FArm& A = R.Arms[Side];
			const FV Forward = Dir(R.Body.U, V3(1, 0, 0));
			const FV ForeDir = Normalize(Sub(A.Hand, A.Elbow));
			const FV Wrist = AddScaled(A.Hand, ForeDir, -0.040);
			const FV HandUp = R.bW ? Dir(R.W, Side == 0 ? V3(0, -1, 0) : V3(0, 0, 1)) : Forward;
			Fitted(L.Hip, L.Knee, L.Pole);
			Fitted(L.Knee, L.Ankle, L.Pole);
			Fitted(L.Ankle, AddScaled(L.Ankle, L.Toe, 0.13), Normalize(Sub(L.Knee, L.Ankle)));
			Fitted(A.Sho, A.Elbow, Forward);
			Fitted(A.Elbow, Wrist, HandUp);
			Fitted(Wrist, AddScaled(Wrist, ForeDir, 0.075), HandUp);
		}
	}

	// ------------------------------------------------------------------ the skeleton and its kit anchors

	// Bone order of the exported skeleton (unreal/Tools/models/infantry/export-infantry.mjs BONES): the root, the 15
	// Blender deform bones, then the rigid kit anchors the attachment meshes hang from.
	enum EBone : int
	{
		BRoot = 0,
		BDeform0 = 1, // pelvis .. hand.R, in the payload's order
		BHeadGear = 16, BGripL, BGripR, BWeapon, BSling, BItem0, BItem1, BItem2, BItem3, BSpine0, BSpine1, BSpine2, BSpine3, BHose0, BHose1,
		NumBones
	};
	inline const char* BoneName(int I)
	{
		// Blender's thigh.L is thigh_L here: Unreal bone names cannot hold '.'
		static const char* Names[NumBones] = { "root", "pelvis", "chest", "head", "thigh_L", "shin_L", "foot_L", "upper_arm_L", "forearm_L", "hand_L",
			"thigh_R", "shin_R", "foot_R", "upper_arm_R", "forearm_R", "hand_R", "head_gear", "grip_L", "grip_R", "weapon", "sling",
			"item_0", "item_1", "item_2", "item_3", "spine_0", "spine_1", "spine_2", "spine_3", "hose_0", "hose_1" };
		return Names[I];
	}
	// Heights on the torso where the job's carried kit is fastened (infantry.js authoredParts: the flame tanks and
	// the engineer's pack, read through onTorso(b, y, ...)). Each becomes body.on(y) on spine_N.
	inline int KitHeights(const FCtx& Ctx, double Out[4])
	{
		if (Ctx.Job.Base == EBase::Flamer)
		{
			if (Ctx.Fac == 2) { Out[0] = 0.9; Out[1] = 0.8; Out[2] = 1.0; Out[3] = 0.7; return 4; }
			if (Ctx.Fac == 1) { Out[0] = 0.9; Out[1] = 0.93; Out[2] = 0.74; Out[3] = 1.06; return 4; }
			Out[0] = 0.9; Out[1] = 0.92; Out[2] = 1.14; return 3;
		}
		if (Ctx.Job.Base == EBase::Engineer)
		{
			if (Ctx.Fac == 1) { Out[0] = 0.76; Out[1] = 0.84; return 2; }
			Out[0] = 0.96; Out[1] = 0.86; Out[2] = 1.1; return 3;
		}
		return 0;
	}

	// Component-space transforms of every bone, reference axes and metres. Deform bones: skin matrix times rest (so
	// skinning with the rest pose as bind pose reproduces infantry-authored.js). Anchors: the reference's own frames.
	// bValid[i] is false for anchors this job does not use in this rig (they collapse to a zero scale).
	inline void BoneTransforms(const FRig& R, FM Out[NumBones], bool bValid[NumBones])
	{
		for (int I = 0; I < NumBones; I++) { Out[I] = Identity(); bValid[I] = false; }
		bValid[BRoot] = true;
		FM Skin[NumDeform];
		SkinMatrices(R, Skin);
		for (int I = 0; I < NumDeform; I++) { Out[BDeform0 + I] = MatMul(Skin[I], Rest(I)); bValid[BDeform0 + I] = true; }
		Out[BHeadGear] = R.H; bValid[BHeadGear] = true;
		for (int Side = 0; Side < 2; Side++)
		{
			const FArm& A = R.Arms[Side];
			const FV ForeDir = Normalize(Sub(A.Hand, A.Elbow));
			const FV Up = R.bW ? Dir(R.W, Side == 0 ? V3(0, -1, 0) : V3(0, 0, 1)) : Dir(R.Body.U, V3(1, 0, 0));
			Out[BGripL + Side] = Basis(A.Hand, ForeDir, Up);
			bValid[BGripL + Side] = true;
		}
		if (R.bW) { Out[BWeapon] = R.W; bValid[BWeapon] = true; }
		if (R.bSling) { Out[BSling] = R.Sling; bValid[BSling] = true; }
		for (int I = 0; I < R.NumThings && I < 4; I++) { Out[BItem0 + I] = R.Things[I].M; bValid[BItem0 + I] = true; }
		double Heights[4];
		const int NumHeights = KitHeights(R.Ctx, Heights);
		for (int I = 0; I < NumHeights; I++) { Out[BSpine0 + I] = R.Body.On(Heights[I]); bValid[BSpine0 + I] = true; }
		if (R.Ctx.Job.Base == EBase::Flamer && R.bW)
		{
			// the hose: from under the tanks, round the right hip, to the gun (infantry.js authoredParts tube)
			const FM Back = Chain(R.Body.On(0.9), Tr(ClothAt(0.9, OnePi, 0.08)), RotY(OnePi), Tr(0, 0, 0.05));
			const FV From = Apply(Back, V3(0, -0.22, 0)), HipAt = R.Body.At(TorsoAt(0.68, -1.7, 0.08)), To = Apply(R.W, V3(0, 0, 0));
			Out[BHose0] = MatMul(Basis(From, Sub(HipAt, From), V3(0, 1, 0)), Scale(Distance(From, HipAt), 1, 1));
			Out[BHose1] = MatMul(Basis(HipAt, Sub(To, HipAt), V3(0, 1, 0)), Scale(Distance(HipAt, To), 1, 1));
			bValid[BHose0] = bValid[BHose1] = true;
		}
		for (int I = 0; I < NumBones; I++) if (!bValid[I]) Out[I] = Scale(0, 0, 0);
	}
	inline FV Muzzle(const FRig& R) { return R.bW ? Apply(R.W, V3(R.Info.L, 0, 0)) : V3(0, 0, 0); }

	// ------------------------------------------------------------------ keyframes and their weights (unit-models.js)

	// The reference bakes per figure: the standing build, the crouch, prone and retreat builds, 20 gait frames
	// (8 running, 4 walking with the weapon up, 4 crouched, 4 crawling), 4 dig strokes for the squads that dig, and the
	// fallen build. The same order is kept here, with the dig keys always present (non-diggers repeat the stand).
	constexpr int NumKeys = 29;
	constexpr int KeyGait = 4, KeyDig = 24, KeyFallen = 28;
	inline FRig SolveKey(const FCtx& Ctx, int Key, int& OutPosture)
	{
		OutPosture = 0;
		if (Key == 0) return Solve(Ctx, EPose::Stand, 0, false, 0);
		if (Key < KeyGait) { OutPosture = Key; return Solve(Ctx, static_cast<EPose>(Key), 0, false, 0); }
		if (Key < KeyDig)
		{
			// [[3, 8], [0, 4], [1, 4], [2, 4]] in infantry.js figure()
			int G = Key - KeyGait;
			if (G < 8) { OutPosture = 3; return Solve(Ctx, EPose::Retreat, 0, true, G / 8.0); }
			G -= 8;
			OutPosture = G / 4;
			return Solve(Ctx, static_cast<EPose>(G / 4), 0, true, (G % 4) / 4.0);
		}
		if (Key < KeyFallen) return IsDigger(Ctx.Type) ? Solve(Ctx, EPose::Dig, Key - KeyDig, false, 0) : Solve(Ctx, EPose::Stand, 0, false, 0);
		OutPosture = -1;
		return Solve(Ctx, EPose::Fallen, 0, false, 0);
	}
	// unit-models.js POSES: [lean, height scale, shift x, shift y, weapon x, weapon y] per posture
	constexpr double HANDXY[2] = { 0.7, 1.05 };
	inline void PoseValues(int K, double Out[6])
	{
		const double PRONE_LEAN = -1.49, PRONE_X = -0.95, PRONE_Y = 0.12;
		const double Base[4][4] = { { 0, 1, 0, 0 }, { 0, 1, 0, 0 }, { PRONE_LEAN, 1, PRONE_X, PRONE_Y }, { 0, 1, 0, 0 } };
		for (int J2 = 0; J2 < 4; J2++) Out[J2] = Base[K][J2];
		Out[4] = HANDXY[0] + AIM_SHIFT[K][0];
		Out[5] = HANDXY[1] + AIM_SHIFT[K][1];
	}
	// poseMorphs(): each posture frame is stored undone by its pose's lean, squash and shift, which animate() then
	// applies to the whole body, so the two cancel at full weight. As a matrix applied before the frame's own.
	inline FM PostureUndo(int K)
	{
		double P[6]; PoseValues(K, P);
		const double C = std::cos(P[0]), S = std::sin(P[0]);
		// x' = (x - tx) c + (y - ty) s; y' = ((y - ty) c - (x - tx) s) / sy
		return MatMul(Set(C, S, 0, 0, -S / P[1], C / P[1], 0, 0, 0, 0, 1, 0, 0, 0, 0, 1), Tr(-P[2], -P[3], 0));
	}
	// squad-motion.js gaitWeights(phase, blend, count, output, offset)
	inline void GaitWeights(double Phase, double Blend, int Count, double* Weights)
	{
		if (!Count || !Blend) return;
		const double Frame = std::fmod(std::fmod(Phase, 1.0) + 1.0, 1.0) * Count;
		const int First = static_cast<int>(std::floor(Frame));
		const double Fraction = Frame - First, T = Fraction * Fraction * (3 - 2 * Fraction);
		Weights[First % Count] = (1 - T) * Blend;
		Weights[(First + 1) % Count] += T * Blend;
	}
	// One man's keyframe weights and whole-body pose for a frame of unit-models.js animate(). PostureW is the squad's
	// eased posture weights w[0..3]; Moving the man's walk blend (motion.blend); Aim the squad's aimBlend; Work the
	// dig blend (sq.work) and WorkPhase sq.workT + seed. Weights are relative to keyframe 0 (morphTargetBaseInfluence).
	struct FBlend
	{
		double Key[NumKeys] = {};
		double Lean = 0, HeightScale = 1, ShiftX = 0, ShiftY = 0, OffsetX = 0, OffsetY = 0;
		double PW0 = 0; // pw[0], the standing build's share (animate()'s muzzle blend weighs tips[0] by it)
	};
	inline FBlend BlendFor(const double PostureW[4], double Moving, double Aim, double Phase, double Work, double WorkPhase, bool bDigger)
	{
		FBlend R;
		double Sum = PostureW[0] + PostureW[1] + PostureW[2] + PostureW[3];
		if (Sum <= 0) Sum = 1;
		const double AimW = Aim * Moving * PostureW[0] / Sum;
		const double Upright = Moving * (PostureW[0] + PostureW[3]) / Sum - AimW, Crouch = Moving * PostureW[1] / Sum, Crawl = Moving * PostureW[2] / Sum;
		const double Dig = bDigger ? Work * (1 - Moving) : 0;
		double Gait[24] = {};
		GaitWeights(Phase, Upright, 8, Gait + 0);
		GaitWeights(Phase, AimW, 4, Gait + 8);
		GaitWeights(Phase, Crouch, 4, Gait + 12);
		GaitWeights(Phase, Crawl, 4, Gait + 16);
		if (Dig) GaitWeights(WorkPhase, Dig, 4, Gait + 20);
		double PW[4], MP[6] = {};
		for (int I = 0; I < 4; I++) PW[I] = PostureW[I] / Sum * (1 - Moving) * (1 - Dig);
		double PV[4][6];
		for (int I = 0; I < 4; I++) PoseValues(I, PV[I]);
		for (int I = 0; I < 4; I++) for (int K = 0; K < 6; K++) MP[K] += PW[I] * PV[I][K];
		for (int K = 0; K < 6; K++) MP[K] += Upright * PV[3][K] + (AimW + Dig) * PV[0][K] + Crouch * PV[1][K] + Crawl * PV[2][K];
		for (int I = 1; I < 4; I++) R.Key[I] = PW[I];
		R.PW0 = PW[0];
		for (int I = 0; I < 24; I++) R.Key[KeyGait + I] = Gait[I];
		R.OffsetX = MP[4] - HANDXY[0]; R.OffsetY = MP[5] - HANDXY[1];
		R.Lean = MP[0]; R.HeightScale = MP[1]; R.ShiftX = MP[2] - R.OffsetX; R.ShiftY = MP[3] - R.OffsetY;
		return R;
	}
	// ------------------------------------------------------------------ squad followers (squad-motion.js moveSquad)

	// seedOf(id, i): Math.imul wraps to 32 bits, the sum is taken modulo 2^32 by >>> 0
	inline double SeedOf(int32_t Id, int32_t I)
	{
		const uint32_t A = static_cast<uint32_t>(Id + 1) * 1664525u, B = static_cast<uint32_t>(I + 1) * 1013904223u;
		return static_cast<double>(static_cast<uint32_t>(A + B)) / 4294967296.0;
	}
	inline double WrapAngle(double A) { return std::atan2(std::sin(A), std::cos(A)); }
	struct FSquadIn
	{
		double X = 0, Z = 0, Yaw = 0;
		bool bVisible = true, bGarrisoned = false, bTrench = false;
		int32_t Id = 0, Flags = 0, Cover = 0;
		double Supp = 0;
		bool bMoveFire = false, bTarget = false, bAim = false;
		double Aim = 0;
	};
	struct FManIn
	{
		double SlotX = 0, SlotZ = 0, Scale = 1.1;
		int32_t Index = 0;
		bool bVisible = true, bRefill = false;
		// motion (u.motion), created on the first call
		bool bInit = false, bStride = false;
		double X = 0, Z = 0, MYaw = 0, Phase = 0, Seed = 0, Blend = 0, Speed = 0, StartX = 0, StartZ = 0, Stride = 0;
		double LocalX = 0, LocalZ = 0, LocalYaw = 0;
	};
	struct FSquadState
	{
		bool bHasLast = false, bHidden = false, bTrenched = false;
		double LastX = 0, LastZ = 0, Speed = 0;
	};
	inline void MoveSquad(FSquadState& Sq, const FSquadIn& V, FManIn* Men, int32_t Count, double Elapsed)
	{
		const double Dt = std::fmin(0.1, std::fmax(0.0, Elapsed));
		const double X = V.X, Z = V.Z, Yaw = V.Yaw, C = std::cos(Yaw), S = std::sin(Yaw);
		const bool bHiddenNow = !V.bVisible || V.bGarrisoned;
		const bool bJump = !Sq.bHasLast || std::hypot(X - Sq.LastX, Z - Sq.LastZ) > 8 || bHiddenNow || Sq.bHidden || (Sq.bTrenched && !V.bTrench);
		const double Speed = bJump || !Dt ? 0 : std::hypot(X - Sq.LastX, Z - Sq.LastZ) / Dt;
		Sq.bHasLast = true; Sq.LastX = X; Sq.LastZ = Z; Sq.bHidden = bHiddenNow; Sq.bTrenched = V.bTrench; Sq.Speed = Speed;
		for (int32_t N = 0; N < Count; N++)
		{
			FManIn& U = Men[N];
			const double SX = U.SlotX, SZ = U.SlotZ, Seed = U.bInit ? U.Seed : SeedOf(V.Id, U.Index);
			const double TX = X + SX * C + SZ * S, TZ = Z + SZ * C - SX * S;
			if (!U.bInit) { U.bInit = true; U.X = TX; U.Z = TZ; U.MYaw = Yaw; U.Phase = Seed; U.Seed = Seed; U.Blend = 0; U.Speed = 0; }
			U.StartX = U.X; U.StartZ = U.Z;
			if (bJump || !U.bVisible || V.bTrench) { U.X = TX; U.Z = TZ; U.MYaw = Yaw; U.Speed = U.Blend = 0; }
			else if (Dt)
			{
				// unequal response times loosen the ranks on a turn; a refill walks to a fallen man's place
				if (U.bRefill && std::hypot(TX - U.X, TZ - U.Z) <= 0.3) U.bRefill = false;
				if (Speed > 0.025 || U.bRefill)
				{
					const double Response = 1 - std::exp(-Dt * (7 + Seed * 4));
					const double DX = (TX - U.X) * Response, DZ = (TZ - U.Z) * Response;
					const double Travel = std::hypot(DX, DZ), Limit = Dt * (Speed + 1.2), Step = Travel > Limit ? Limit / Travel : 1;
					U.X += DX * Step; U.Z += DZ * Step;
					const double Radius = std::hypot(U.X - X, U.Z - Z), Extent = std::hypot(SX, SZ) + 0.65;
					if (Radius > Extent) { U.X = X + (U.X - X) * Extent / Radius; U.Z = Z + (U.Z - Z) * Extent / Radius; }
				}
			}
		}
		// keep shoulders apart when followers take different arcs through a corner
		if (Speed > 0.025 && !bJump && !V.bTrench)
		{
			for (int32_t Pass = 0; Pass < 2; Pass++)
			{
				for (int32_t I = 0; I < Count; I++)
				{
					if (!Men[I].bVisible) continue;
					FManIn& A = Men[I];
					for (int32_t K = 0; K < I; K++)
					{
						if (!Men[K].bVisible) continue;
						FManIn& B = Men[K];
						const double DX = A.X - B.X, DZ = A.Z - B.Z, Dist = std::hypot(DX, DZ);
						if (Dist >= 0.5 || Dist < 0.0001) continue;
						const double Push = (0.5 - Dist) / (2 * Dist);
						A.X += DX * Push; A.Z += DZ * Push; B.X -= DX * Push; B.Z -= DZ * Push;
					}
				}
			}
		}
		for (int32_t N = 0; N < Count; N++)
		{
			FManIn& U = Men[N];
			const double Seed = U.Seed;
			if (!bJump && U.bVisible && !V.bTrench && Dt)
			{
				U.Speed = std::hypot(U.X - U.StartX, U.Z - U.StartZ) / Dt;
				const bool bWalk = U.Speed > 0.045, bFiring = V.bMoveFire && V.bTarget && !(V.Flags & 1) && V.bAim;
				const double Facing = bFiring ? -V.Aim : bWalk ? std::atan2(-(U.Z - U.StartZ), U.X - U.StartX) : Yaw;
				const double Turn = Dt * (3 + Seed);
				U.MYaw += std::fmax(-Turn, std::fmin(Turn, WrapAngle(Facing - U.MYaw)));
				const double BlendGoal = bWalk ? std::fmin(1.0, U.Speed / 0.7) : 0;
				U.Blend += (BlendGoal - U.Blend) * (1 - std::exp(-Dt * 14));
				if (U.Blend < 0.002) U.Blend = 0;
				// a full cycle travels two steps; phase follows actual travel
				const double Stride = (V.Flags & 1) ? 1.24 : (V.Supp >= 90 && V.Cover != 2) ? 0.48 : V.Supp >= 50 ? 0.76 : 1.24;
				if (!U.bStride) { U.bStride = true; U.Stride = Stride; }
				U.Stride += (Stride - U.Stride) * (1 - std::exp(-Dt * 8));
				if (bWalk) U.Phase = std::fmod(U.Phase + U.Speed * Dt / (U.Stride * U.Scale) * (0.96 + Seed * 0.08), 1.0);
			}
			const double DX = U.X - X, DZ = U.Z - Z;
			U.LocalX = DX * C - DZ * S; U.LocalZ = DX * S + DZ * C; U.LocalYaw = WrapAngle(U.MYaw - Yaw);
		}
	}

	// ------------------------------------------------------------------ the muzzle points (unit-models.js poseMorphs)

	// infantry.js figure() bakes a muzzle with every frame (muzzle(r): r.W's (info.L, 0, 0), else none) and poseMorphs
	// lists them as userData.muzzles: the standing build's first, then the three posture builds, the 20 gait frames and
	// the 4 dig strokes of a digger, each undone by its posture's pose node like its vertices. A frame without a weapon
	// takes the standing build's. No list (returns 0) when the standing build holds no weapon.
	constexpr int MaxTips = 28;
	inline int FigureMuzzles(const FCtx& Ctx, FV Out[MaxTips])
	{
		const FRig Base = Solve(Ctx, EPose::Stand, 0, false, 0);
		if (!Base.bW) return 0;
		const FV Muzzle0 = Muzzle(Base);
		Out[0] = Muzzle0;
		const int Frames = IsDigger(Ctx.Type) ? KeyFallen - 1 : KeyDig - 1; // 27 or 23 frames after the standing build
		for (int F = 0; F < Frames; F++)
		{
			int Posture = 0;
			const FRig R = SolveKey(Ctx, F + 1, Posture);
			const FV Tip = R.bW ? Muzzle(R) : Muzzle0;
			double P[6]; PoseValues(Posture, P);
			const double C = std::cos(P[0]), S = std::sin(P[0]), X = Tip.X - P[2], Y = Tip.Y - P[3];
			Out[F + 1] = V3(X * C + Y * S, (Y * C - X * S) / P[1], Tip.Z);
		}
		return Frames + 1;
	}
	// animate()'s u.muzzle: the tips blended by the frame weights, then through the man's pose node (soldier metres,
	// in the man node's frame, before his figure scale)
	inline FV BlendMuzzle(const FBlend& B, const FV* Tips, int NumTips)
	{
		FV Tip = Mul(Tips[0], B.PW0);
		for (int J = 0; J < NumTips - 1; J++) Tip = AddScaled(Tip, Tips[J + 1], B.Key[J + 1]);
		const double C = std::cos(B.Lean), Sn = std::sin(B.Lean), X = Tip.X, Y = Tip.Y * B.HeightScale;
		return V3(X * C - Y * Sn + B.ShiftX, X * Sn + Y * C + B.ShiftY, Tip.Z);
	}

	// ------------------------------------------------------------------ a squad's frame (unit-models.js animate)

	// v.squad as animate() keeps it between frames
	struct FAnimSquad
	{
		double W[4] = { 1, 0, 0, 0 };
		bool bFar = false, bSuspended = false, bPose = false;
		double PoseX = 0, PoseZ = 0, PoseTravel = 0, PoseSpeed = 0, Work = 0, WorkT = 0, AimBlend = 0;
		FSquadState Move;
	};
	// What animate() reads from the unit for one frame (reference metres and radians, three.js axes)
	struct FAnimIn
	{
		double RootX = 0, RootY = 0, RootZ = 0, RootYaw = 0; // v.root.position and rotation.y
		double VX = 0, VZ = 0;                               // v.x, v.z (the eased position; the root stands on it)
		bool bVisible = true, bDetailed = true;              // v.root.visible; main.js animationInterest
		double EyeX = 0, EyeY = 0, EyeZ = 0;                 // camera position
		double Lod = 110;                                    // LOD.high (LOD.low on Low graphics)
		int32_t Id = 0, Flags = 0, Cover = 0;
		double Supp = 0;
		bool bTarget = false, bMoveFire = false, bAim = true; // v.tgt, v.squad.moveFire, Number.isFinite(v.aim)
		double Aim = 0;
		bool bGarrisoned = false, bTrench = false;           // v.garr, v.trench (seated in trench cells)
		bool bDigger = false;                                // the figure has dig frames (frames > 20)
		bool bCarved = true;                                 // the ground is carved under trench lines (the 0.6 m sink applies)
		double (*Ground)(void* Ctx, double X, double Z) = nullptr; // groundAt, or none (floor 0)
		void* GroundCtx = nullptr;
	};
	// One man after the frame: man.position, man.rotation.y, the pose node, the morph influences and u.muzzle
	struct FAnimMan
	{
		double X = 0, Y = 0, Z = 0, Yaw = 0;
		FBlend Blend;
		bool bMuzzle = false;
		FV Muzzle;
	};
	// unit-models.js postureOf
	inline int PostureOf(double Supp, int32_t Flags, int32_t Cover) { return (Flags & 1) ? 3 : Supp >= 90 && Cover != 2 ? 2 : Supp >= 50 ? 1 : 0; }
	// One call of animate(v, dt, eye, groundAt, detailed) for a squad. Men[i].bVisible is man.visible (alive and not
	// garrisoned), Tips[i] his figure's muzzle list (FigureMuzzles) or null. False when the squad is suspended (not
	// detailed or hidden): nothing moves and Out keeps the last frame.
	inline bool AnimateSquad(FAnimSquad& Sq, const FAnimIn& V, FManIn* Men, FAnimMan* Out, const FV* const* Tips, const int* NumTips, int32_t Count, double Dt)
	{
		const double Px = V.RootX, Pz = V.RootZ;
		const double Distance = !Sq.bPose ? 0 : std::hypot(Px - Sq.PoseX, Pz - Sq.PoseZ);
		const double Travel = Distance > 8 ? 0 : Distance;
		Sq.PoseX = Px; Sq.PoseZ = Pz; Sq.bPose = true;
		Sq.PoseTravel += Travel;
		Sq.PoseSpeed = Dt > 0 ? Travel / Dt : 0;
		if (!V.bDetailed || !V.bVisible) { Sq.bSuspended = true; return false; }
		const bool bReturned = Sq.bSuspended;
		Sq.bSuspended = false;
		const double Lim = V.Lod, DX = V.EyeX - V.VX, DY = V.EyeY - V.RootY, DZ = V.EyeZ - V.VZ, D2 = DX * DX + DY * DY + DZ * DZ;
		Sq.bFar = Sq.bFar ? D2 > (Lim - 4) * (Lim - 4) : D2 > (Lim + 4) * (Lim + 4);
		const int Goal = PostureOf(V.Supp, V.Flags, V.Cover);
		const double Step = Dt / 0.3; // POSTURE.blend
		const bool bWorking = (V.Flags & 144) && Goal < 2 && Sq.PoseSpeed < 0.3;
		Sq.Work = std::fmax(0.0, std::fmin(1.0, Sq.Work + (bWorking ? Step : -Step)));
		if (Sq.Work) Sq.WorkT += Dt * 0.62;
		if (bReturned) for (int I = 0; I < 4; I++) Sq.W[I] = I == Goal ? 1 : 0;
		for (int I = 0; I < 4; I++)
		{
			const double D = (I == Goal ? 1 : 0) - Sq.W[I];
			if (D) Sq.W[I] += std::fmax(-Step, std::fmin(Step, D));
		}
		const double AimGoal = V.bMoveFire && V.bTarget && !(V.Flags & 1) ? 1 : 0;
		Sq.AimBlend = Sq.AimBlend + std::fmax(-Step, std::fmin(Step, AimGoal - Sq.AimBlend));
		FSquadIn In;
		In.X = V.RootX; In.Z = V.RootZ; In.Yaw = V.RootYaw;
		In.bVisible = V.bVisible; In.bGarrisoned = V.bGarrisoned; In.bTrench = V.bTrench;
		In.Id = V.Id; In.Flags = V.Flags; In.Cover = V.Cover; In.Supp = V.Supp;
		In.bMoveFire = V.bMoveFire; In.bTarget = V.bTarget; In.bAim = V.bAim; In.Aim = V.Aim;
		if (bReturned)
		{
			Sq.AimBlend = AimGoal;
			Sq.Move.bHasLast = false;
			MoveSquad(Sq.Move, In, Men, Count, 0);
			const double Stride = (V.Flags & 1) ? 1.24 : Goal == 2 ? 0.48 : Goal == 1 ? 0.76 : 1.24;
			for (int32_t N = 0; N < Count; N++)
			{
				FManIn& M = Men[N];
				M.Speed = Sq.PoseSpeed;
				M.Blend = V.bTrench || !M.bVisible ? 0 : std::fmin(1.0, M.Speed / 0.7);
				M.Phase = std::fmod(M.Seed + Sq.PoseTravel / (Stride * M.Scale), 1.0);
			}
		}
		else MoveSquad(Sq.Move, In, Men, Count, Dt);
		const double W[4] = { Sq.W[0], Sq.W[1], Sq.W[2], Sq.W[3] };
		const double Sink = V.bCarved && V.Cover == 2 && !V.bTrench ? -0.6 : 0; // seated in a trench they stand on its carved floor
		for (int32_t N = 0; N < Count; N++)
		{
			const FManIn& M = Men[N];
			FAnimMan& O = Out[N];
			// BlendFor divides by the weights' sum itself, as animate() does
			O.Blend = BlendFor(W, M.Blend, Sq.AimBlend, M.Phase, Sq.Work, Sq.WorkT + M.Seed, V.bDigger);
			O.bMuzzle = Tips && Tips[N] && NumTips[N] > 0;
			if (O.bMuzzle) O.Muzzle = BlendMuzzle(O.Blend, Tips[N], NumTips[N]);
			const double S = M.Scale;
			const double Floor = V.Ground ? V.Ground(V.GroundCtx, M.X, M.Z) - V.RootY : 0;
			O.X = M.LocalX + O.Blend.OffsetX * S;
			O.Y = Floor + Sink + O.Blend.OffsetY * S;
			O.Z = M.LocalZ;
			O.Yaw = M.LocalYaw;
		}
		return true;
	}

	// The figure scale unit-models.js gives a man: line infantry 1.1, everyone else 1.35
	inline double FigureScale(EType T) { return (T == EType::Rifle || T == EType::Engineer || T == EType::Ranger || T == EType::Conscript || T == EType::Commando) ? 1.1 : 1.35; }
} // namespace TCInfantryRigCore

#ifndef TC_INFANTRY_RIG_STANDALONE
// ---------------------------------------------------------------------------------------------- Unreal adapters

namespace TCInfantryRigCore
{
	// Reference space (+x forward, +y up, +z right, metres, column vectors) to Unreal component space (+X forward,
	// +Y right, +Z up, centimetres, row vectors): conjugation by the axis swap y <-> z and a scale of 100 on positions.
	// The glTF importer applies the same map to the exported skeleton and meshes.
	static constexpr int32 Perm[3] = { 0, 2, 1 };
	static FMatrix44f ToUnrealMatrix(const FM& M)
	{
		FMatrix44f U;
		for (int32 R = 0; R < 3; R++)
		{
			for (int32 C = 0; C < 3; C++) U.M[R][C] = static_cast<float>(M.E[Perm[R] * 4 + Perm[C]]);
			U.M[R][3] = 0.f;
		}
		for (int32 C = 0; C < 3; C++) U.M[3][C] = static_cast<float>(M.E[12 + Perm[C]] * 100.0);
		U.M[3][3] = 1.f;
		return U;
	}
	static FTransform ToUnreal(const FM& M)
	{
		FMatrix U;
		for (int32 R = 0; R < 3; R++)
		{
			for (int32 C = 0; C < 3; C++) U.M[R][C] = M.E[Perm[R] * 4 + Perm[C]];
			U.M[R][3] = 0.0;
		}
		for (int32 C = 0; C < 3; C++) U.M[3][C] = M.E[12 + Perm[C]] * 100.0;
		U.M[3][3] = 1.0;
		const FVector Scale = U.GetScaleVector(0.0);
		if (Scale.IsNearlyZero(1e-9)) return FTransform(FQuat::Identity, U.GetOrigin(), FVector::ZeroVector);
		return FTransform(U);
	}
	static bool MakeCtx(const FString& UnitType, int32 Faction, int32 Slot, FCtx& Out)
	{
		const FTCHARToUTF8 Name(*UnitType.ToLower());
		Out.Type = ParseType(Name.Get());
		Out.Fac = Faction;
		Out.Job = SoldierKit(Out.Type, Slot, Faction);
		return Out.Type != EType::Other;
	}
	static FString JobName(const FJob& Job)
	{
		FString Name = UTF8_TO_TCHAR(BaseName(Job.Base));
		if (Job.Variant) Name.AppendChar(static_cast<TCHAR>(Job.Variant));
		return Name;
	}
	static bool BakeRef(const FString& UnitType, int32 Faction, int32 Slot, FM OutPose[NumKeys][NumBones], FM OutSkin[NumKeys][NumBones])
	{
		FCtx Ctx;
		if (!MakeCtx(UnitType, Faction, Slot, Ctx)) return false;
		for (int32 Key = 0; Key < NumKeys; Key++)
		{
			int32 Posture = 0;
			const FRig R = SolveKey(Ctx, Key, Posture);
			FM Bones[NumBones];
			bool bValid[NumBones];
			BoneTransforms(R, Bones, bValid);
			FM Skin[NumDeform];
			SkinMatrices(R, Skin);
			// posture frames are stored undone by their pose node (unit-models.js poseMorphs); stand, dig and fallen are not
			const bool bUndo = Key >= 1 && Key < KeyDig && Posture > 0;
			const FM Undo = bUndo ? PostureUndo(Posture) : Identity();
			for (int32 B = 0; B < NumBones; B++)
			{
				const FM SkinRef = (B >= BDeform0 && B < BDeform0 + NumDeform) ? Skin[B - BDeform0] : Bones[B];
				OutPose[Key][B] = bUndo ? MatMul(Undo, Bones[B]) : Bones[B];
				OutSkin[Key][B] = bUndo ? MatMul(Undo, SkinRef) : SkinRef;
			}
		}
		return true;
	}
	static FM PoseNode(double Lean, double HeightScale, double ShiftX, double ShiftY)
	{
		// three.js Object3D: position, rotation z, scale y composed as T * R * S
		return Chain(Tr(ShiftX, ShiftY, 0), RotZ(Lean), Scale(1, HeightScale, 1));
	}
}

FString UTCInfantryRigLibrary::SoldierKit(const FString& UnitType, int32 Slot, int32 Faction)
{
	TCInfantryRigCore::FCtx Ctx;
	if (!TCInfantryRigCore::MakeCtx(UnitType, Faction, Slot, Ctx)) return FString();
	return TCInfantryRigCore::JobName(Ctx.Job);
}

TArray<FName> UTCInfantryRigLibrary::BoneNames()
{
	TArray<FName> Names;
	for (int32 I = 0; I < NumBones; I++) Names.Add(FName(UTF8_TO_TCHAR(TCInfantryRigCore::BoneName(I))));
	return Names;
}

float UTCInfantryRigLibrary::FigureScale(const FString& UnitType)
{
	// unit-models.js BLOCKS: the line infantry stand smaller so the ranks close up; medics are not in it
	static const TCHAR* Blocks[] = { TEXT("rifle"), TEXT("engineer"), TEXT("ranger"), TEXT("conscript"), TEXT("commando") };
	for (const TCHAR* Name : Blocks) if (UnitType.Equals(Name, ESearchCase::IgnoreCase)) return 1.1f;
	return 1.35f;
}

bool UTCInfantryRigLibrary::SolvePose(const FString& UnitType, int32 Faction, int32 Slot, ETCInfantryState Pose, float Phase, int32 DigKey, FTCInfantryPose& Out)
{
	using namespace TCInfantryRigCore;
	FCtx Ctx;
	if (!MakeCtx(UnitType, Faction, Slot, Ctx)) return false;
	const bool bPhase = Phase >= 0.f && Pose != ETCInfantryState::Fallen && Pose != ETCInfantryState::Dig;
	const FRig R = Solve(Ctx, static_cast<EPose>(Pose), DigKey, bPhase, bPhase ? FMath::Fmod(static_cast<double>(Phase), 1.0) : 0.0);
	FM Bones[NumBones];
	bool bValid[NumBones];
	BoneTransforms(R, Bones, bValid);
	Out.Job = JobName(Ctx.Job);
	Out.Bones.SetNum(NumBones);
	Out.BoneUsed.SetNum(NumBones);
	for (int32 B = 0; B < NumBones; B++) { Out.Bones[B] = ToUnreal(Bones[B]); Out.BoneUsed[B] = bValid[B]; }
	Out.bHasMuzzle = R.bW;
	const FV M = Muzzle(R);
	Out.Muzzle = R.bW ? FVector(M.X * 100.0, M.Z * 100.0, M.Y * 100.0) : FVector::ZeroVector;
	return true;
}

FTCInfantryBlend UTCInfantryRigLibrary::BlendFor(const FString& UnitType, const FTCInfantryMotion& Motion)
{
	using namespace TCInfantryRigCore;
	FTCInfantryBlend Out;
	Out.KeyWeights.Init(0.f, NumKeys);
	if (Motion.bFallen)
	{
		Out.KeyWeights[KeyFallen] = 1.f;
		return Out;
	}
	const double W[4] = { Motion.PostureWeights.X, Motion.PostureWeights.Y, Motion.PostureWeights.Z, Motion.PostureWeights.W };
	const FTCHARToUTF8 Name(*UnitType.ToLower());
	const FBlend B = TCInfantryRigCore::BlendFor(W, Motion.Moving, Motion.Aim, Motion.Phase, Motion.Work, Motion.WorkPhase, IsDigger(ParseType(Name.Get())));
	for (int32 K = 1; K < NumKeys; K++) Out.KeyWeights[K] = static_cast<float>(B.Key[K]);
	Out.Lean = static_cast<float>(B.Lean);
	Out.HeightScale = static_cast<float>(B.HeightScale);
	Out.Shift = FVector2D(B.ShiftX * 100.0, B.ShiftY * 100.0);
	Out.ManOffset = FVector2D(B.OffsetX * 100.0, B.OffsetY * 100.0);
	return Out;
}

bool UTCInfantryRigLibrary::BakeKeyframes(const FString& UnitType, int32 Faction, int32 Slot, TArray<FMatrix44f>& OutSkin)
{
	using namespace TCInfantryRigCore;
	static FM Pose[NumKeys][NumBones], Skin[NumKeys][NumBones];
	// The static scratch arrays make this game-thread only, like the texture it feeds.
	check(IsInGameThread());
	if (!BakeRef(UnitType, Faction, Slot, Pose, Skin)) return false;
	OutSkin.SetNum(NumKeys * NumBones);
	for (int32 K = 0; K < NumKeys; K++) for (int32 B = 0; B < NumBones; B++) OutSkin[K * NumBones + B] = ToUnrealMatrix(Skin[K][B]);
	return true;
}

bool UTCInfantryRigLibrary::BakeKeyframeTransforms(const FString& UnitType, int32 Faction, int32 Slot, TArray<FTransform>& OutBones)
{
	using namespace TCInfantryRigCore;
	static FM Pose[NumKeys][NumBones], Skin[NumKeys][NumBones];
	check(IsInGameThread());
	if (!BakeRef(UnitType, Faction, Slot, Pose, Skin)) return false;
	OutBones.SetNum(NumKeys * NumBones);
	for (int32 K = 0; K < NumKeys; K++) for (int32 B = 0; B < NumBones; B++) OutBones[K * NumBones + B] = ToUnreal(Pose[K][B]);
	return true;
}

UTexture2D* UTCInfantryRigLibrary::BakeKeyframeTexture(const FString& UnitType, int32 Faction, int32 Slot)
{
	TArray<FMatrix44f> Skin;
	if (!BakeKeyframes(UnitType, Faction, Slot, Skin)) return nullptr;
	const int32 Width = NumBones * TexelsPerBone, Height = NumKeys;
	UTexture2D* Texture = UTexture2D::CreateTransient(Width, Height, PF_A32B32G32R32F);
	if (!Texture) return nullptr;
	Texture->Filter = TF_Nearest;
	Texture->SRGB = false;
	Texture->AddressX = TA_Clamp;
	Texture->AddressY = TA_Clamp;
	Texture->NeverStream = true;
	FTexture2DMipMap& Mip = Texture->GetPlatformData()->Mips[0];
	float* Texels = static_cast<float*>(Mip.BulkData.Lock(LOCK_READ_WRITE));
	for (int32 K = 0; K < NumKeys; K++)
	{
		for (int32 B = 0; B < NumBones; B++)
		{
			const FMatrix44f& M = Skin[K * NumBones + B];
			// texel c holds column c of the row-vector matrix: p' = (p, 1) . (M[0][c], M[1][c], M[2][c], M[3][c])
			for (int32 C = 0; C < TexelsPerBone; C++)
			{
				float* T = Texels + ((K * Width) + B * TexelsPerBone + C) * 4;
				T[0] = M.M[0][C]; T[1] = M.M[1][C]; T[2] = M.M[2][C]; T[3] = M.M[3][C];
			}
		}
	}
	Mip.BulkData.Unlock();
	Texture->UpdateResource();
	return Texture;
}

TArray<float> UTCInfantryRigLibrary::CrowdCustomData(const FTCInfantryBlend& Blend)
{
	// key 0 carries what the others leave (three.js morphTargetBaseInfluence for absolute targets)
	TArray<TPair<float, int32>> Keys;
	float Others = 0.f;
	for (int32 K = 1; K < Blend.KeyWeights.Num(); K++) { Others += Blend.KeyWeights[K]; if (Blend.KeyWeights[K] > 0.f) Keys.Add({ Blend.KeyWeights[K], K }); }
	if (1.f - Others > 0.f) Keys.Add({ 1.f - Others, 0 });
	Keys.Sort([](const TPair<float, int32>& A, const TPair<float, int32>& B) { return A.Key > B.Key || (A.Key == B.Key && A.Value < B.Value); });
	TArray<float> Data;
	Data.Init(0.f, 12);
	float Sum = 0.f;
	for (int32 I = 0; I < 4 && I < Keys.Num(); I++) Sum += Keys[I].Key;
	for (int32 I = 0; I < 4; I++)
	{
		const bool bUsed = I < Keys.Num() && Sum > 0.f;
		Data[I] = bUsed ? static_cast<float>(Keys[I].Value) : 0.f;
		Data[4 + I] = bUsed ? Keys[I].Key / Sum : (I == 0 && Sum <= 0.f ? 1.f : 0.f);
	}
	Data[8] = Blend.Lean;
	Data[9] = Blend.HeightScale;
	Data[10] = static_cast<float>(Blend.Shift.X);
	Data[11] = static_cast<float>(Blend.Shift.Y);
	return Data;
}

void FTCInfantrySquadMotion::Move(const FTCInfantrySquadInput& In, float Elapsed)
{
	using namespace TCInfantryRigCore;
	FSquadIn V;
	V.X = In.Position.X / 100.0; V.Z = In.Position.Y / 100.0; V.Yaw = In.Yaw;
	V.bVisible = In.bVisible; V.bGarrisoned = In.bGarrisoned; V.bTrench = In.bTrench;
	V.Id = In.Id; V.Flags = In.Flags; V.Cover = In.Cover; V.Supp = In.Suppression;
	V.bMoveFire = In.bMoveFire; V.bTarget = In.bHasTarget; V.bAim = In.bHasAim; V.Aim = In.Aim;
	TArray<FManIn, TInlineAllocator<32>> Core;
	Core.SetNum(Men.Num());
	for (int32 I = 0; I < Men.Num(); I++)
	{
		const FTCInfantrySquadMan& M = Men[I];
		FManIn& C = Core[I];
		C.SlotX = M.Slot.X / 100.0; C.SlotZ = M.Slot.Y / 100.0; C.Scale = M.Scale; C.Index = M.Index; C.bVisible = M.bVisible; C.bRefill = M.bRefill;
		C.bInit = M.bInit; C.bStride = M.bStride; C.X = M.X; C.Z = M.Z; C.MYaw = M.Yaw; C.Phase = M.Phase; C.Seed = M.Seed;
		C.Blend = M.Moving; C.Speed = M.Speed; C.Stride = M.Stride;
	}
	FSquadState Sq;
	Sq.bHasLast = bHasLast; Sq.bHidden = bHidden; Sq.bTrenched = bTrenched; Sq.LastX = LastX; Sq.LastZ = LastZ; Sq.Speed = Speed;
	MoveSquad(Sq, V, Core.GetData(), Core.Num(), Elapsed);
	bHasLast = Sq.bHasLast; bHidden = Sq.bHidden; bTrenched = Sq.bTrenched; LastX = Sq.LastX; LastZ = Sq.LastZ; Speed = Sq.Speed;
	for (int32 I = 0; I < Men.Num(); I++)
	{
		FTCInfantrySquadMan& M = Men[I];
		const FManIn& C = Core[I];
		M.bRefill = C.bRefill; M.bInit = C.bInit; M.bStride = C.bStride; M.X = C.X; M.Z = C.Z; M.Yaw = C.MYaw; M.Phase = C.Phase; M.Seed = C.Seed;
		M.Moving = C.Blend; M.Speed = C.Speed; M.Stride = C.Stride;
		M.Local = FVector2D(C.LocalX * 100.0, C.LocalZ * 100.0);
		M.LocalYaw = C.LocalYaw;
	}
}

// ---------------------------------------------------------------------------------------------- a squad's animate()

struct FTCInfantrySquadAnimation::FState
{
	TCInfantryRigCore::FAnimSquad Squad;
	TArray<TCInfantryRigCore::FManIn> Men;
	TArray<TCInfantryRigCore::FAnimMan> Posed;
	// each man's figure muzzles (TCInfantryRigCore::FigureMuzzles), shared per figure
	TArray<TSharedPtr<TArray<TCInfantryRigCore::FV>>> Tips;
	bool bDigger = false;
};

FTCInfantrySquadAnimation::FTCInfantrySquadAnimation() : State(new FState) {}
FTCInfantrySquadAnimation::~FTCInfantrySquadAnimation() { delete State; }
FTCInfantrySquadAnimation::FTCInfantrySquadAnimation(const FTCInfantrySquadAnimation& Other) : State(new FState(*Other.State)), Out(Other.Out), Type(Other.Type), Fac(Other.Fac) {}
FTCInfantrySquadAnimation& FTCInfantrySquadAnimation::operator=(const FTCInfantrySquadAnimation& Other)
{
	if (this != &Other) { *State = *Other.State; Out = Other.Out; Type = Other.Type; Fac = Other.Fac; }
	return *this;
}

void FTCInfantrySquadAnimation::Setup(const FString& UnitType, int32 Faction, const TArray<FVector2D>& SlotsCm, float Scale)
{
	using namespace TCInfantryRigCore;
	// figure muzzles per (figure, faction, job); game thread only, like the keyframe bakes
	static TMap<FString, TSharedPtr<TArray<FV>>> Cache;
	*State = FState();
	Type = UnitType; Fac = Faction;
	const FString Figure = UnitType.Equals(TEXT("medic"), ESearchCase::IgnoreCase) ? FString(TEXT("engineer")) : UnitType.ToLower();
	const FTCHARToUTF8 Name(*Figure);
	State->bDigger = IsDigger(ParseType(Name.Get()));
	const int32 Count = SlotsCm.Num();
	State->Men.SetNum(Count);
	State->Posed.SetNum(Count);
	State->Tips.SetNum(Count);
	Out.SetNum(Count);
	for (int32 I = 0; I < Count; I++)
	{
		FManIn& M = State->Men[I];
		M.SlotX = SlotsCm[I].X / 100.0; M.SlotZ = SlotsCm[I].Y / 100.0; M.Scale = Scale; M.Index = I;
		FCtx Ctx;
		const bool bKnown = MakeCtx(Figure, Faction, I, Ctx);
		Out[I] = FTCInfantryManPose();
		Out[I].Scale = Scale;
		Out[I].Job = bKnown ? JobName(Ctx.Job) : FString();
		if (!bKnown) continue;
		const FString Key = FString::Printf(TEXT("%s|%d|%s"), *Figure, Faction, *Out[I].Job);
		TSharedPtr<TArray<FV>>& Tips = Cache.FindOrAdd(Key);
		if (!Tips.IsValid())
		{
			FV Buffer[MaxTips];
			const int N = FigureMuzzles(Ctx, Buffer);
			Tips = MakeShared<TArray<FV>>(Buffer, N);
		}
		State->Tips[I] = Tips;
	}
}

bool FTCInfantrySquadAnimation::IsSetUp(const FString& UnitType, int32 Faction, int32 Men) const
{
	return Type == UnitType && Fac == Faction && State->Men.Num() == Men && Men > 0;
}

void FTCInfantrySquadAnimation::SetAlive(int32 Alive)
{
	for (int32 I = 0; I < State->Men.Num(); I++) State->Men[I].bVisible = I < Alive;
}

bool FTCInfantrySquadAnimation::IsFar() const { return State->Squad.bFar; }

bool FTCInfantrySquadAnimation::Animate(const FTCInfantrySquadFrame& In, float DeltaSeconds)
{
	using namespace TCInfantryRigCore;
	const int32 Count = State->Men.Num();
	if (!Count) return false;
	FAnimIn V;
	V.RootX = In.Root.X / 100.0; V.RootY = In.Root.Z / 100.0; V.RootZ = In.Root.Y / 100.0; V.RootYaw = In.Yaw;
	V.VX = V.RootX; V.VZ = V.RootZ;
	V.bVisible = In.bVisible; V.bDetailed = In.bDetailed;
	V.EyeX = In.Eye.X / 100.0; V.EyeY = In.Eye.Z / 100.0; V.EyeZ = In.Eye.Y / 100.0;
	V.Lod = In.LodMetres;
	V.Id = In.Id; V.Flags = In.Flags; V.Cover = In.Cover; V.Supp = In.Suppression;
	V.bTarget = In.bHasTarget; V.bMoveFire = In.bMoveFire; V.bAim = In.bHasAim; V.Aim = In.Aim;
	V.bGarrisoned = In.bGarrisoned; V.bTrench = In.bTrench; V.bDigger = State->bDigger; V.bCarved = In.bCarvedTrenches;
	struct FGround { const TFunction<double(double, double)>* Fn; };
	FGround Ground{ &In.GroundCm };
	if (In.GroundCm)
	{
		V.Ground = [](void* Ctx, double X, double Z) { return (*static_cast<FGround*>(Ctx)->Fn)(X * 100.0, Z * 100.0) / 100.0; };
		V.GroundCtx = &Ground;
	}
	// main.js hides a garrisoned squad's men (man.visible = i < alive && !garr)
	TArray<bool, TInlineAllocator<32>> Alive;
	for (FManIn& M : State->Men) { Alive.Add(M.bVisible); if (In.bGarrisoned) M.bVisible = false; }
	TArray<const FV*, TInlineAllocator<32>> Tips;
	TArray<int, TInlineAllocator<32>> NumTips;
	for (const TSharedPtr<TArray<FV>>& T : State->Tips) { Tips.Add(T.IsValid() && T->Num() ? T->GetData() : nullptr); NumTips.Add(T.IsValid() ? T->Num() : 0); }
	const bool bPosed = AnimateSquad(State->Squad, V, State->Men.GetData(), State->Posed.GetData(), Tips.GetData(), NumTips.GetData(), Count, DeltaSeconds);
	for (int32 I = 0; I < Count; I++) { Out[I].bVisible = State->Men[I].bVisible; State->Men[I].bVisible = Alive[I]; }
	if (!bPosed) return false;
	for (int32 I = 0; I < Count; I++)
	{
		const FAnimMan& P = State->Posed[I];
		FTCInfantryManPose& O = Out[I];
		O.Local = FVector(P.X * 100.0, P.Z * 100.0, P.Y * 100.0);
		O.LocalYaw = P.Yaw;
		O.Blend.KeyWeights.Init(0.f, NumKeys);
		for (int32 K = 1; K < NumKeys; K++) O.Blend.KeyWeights[K] = static_cast<float>(P.Blend.Key[K]);
		O.Blend.Lean = static_cast<float>(P.Blend.Lean);
		O.Blend.HeightScale = static_cast<float>(P.Blend.HeightScale);
		O.Blend.Shift = FVector2D(P.Blend.ShiftX * 100.0, P.Blend.ShiftY * 100.0);
		O.Blend.ManOffset = FVector2D(P.Blend.OffsetX * 100.0, P.Blend.OffsetY * 100.0);
		O.bHasMuzzle = P.bMuzzle;
		O.Muzzle = P.bMuzzle ? FVector(P.Muzzle.X * 100.0, P.Muzzle.Z * 100.0, P.Muzzle.Y * 100.0) : FVector::ZeroVector;
	}
	return true;
}

FTransform FTCInfantrySquadAnimation::ManTransform(int32 Index, const FTransform& Root) const
{
	if (!Out.IsValidIndex(Index)) return Root;
	const FTCInfantryManPose& M = Out[Index];
	// man.position, man.rotation.y (a three.js turn a about +y is an Unreal yaw of -a), man.scale
	const FTransform Local(FQuat(FVector::UpVector, -M.LocalYaw), M.Local, FVector(M.Scale));
	return Local * FTransform(Root.GetRotation(), Root.GetLocation());
}

bool FTCInfantrySquadAnimation::MuzzleWorld(int32 Index, const FTransform& Root, FVector& OutWorld) const
{
	if (!Out.IsValidIndex(Index) || !Out[Index].bHasMuzzle) return false;
	OutWorld = ManTransform(Index, Root).TransformPosition(Out[Index].Muzzle);
	return true;
}

// ---------------------------------------------------------------------------------------------- the anim instance

struct FTCInfantryAnimInstanceProxy : public FAnimInstanceProxy
{
	FTCInfantryAnimInstanceProxy() = default;
	explicit FTCInfantryAnimInstanceProxy(UAnimInstance* InAnimInstance) : FAnimInstanceProxy(InAnimInstance) {}

	virtual void PreUpdate(UAnimInstance* InAnimInstance, float DeltaSeconds) override
	{
		FAnimInstanceProxy::PreUpdate(InAnimInstance, DeltaSeconds);
		UTCInfantryAnimInstance* Owner = CastChecked<UTCInfantryAnimInstance>(InAnimInstance);
		if (Owner->BakedFor != BakedFor) { Keys = Owner->Keys; BakedFor = Owner->BakedFor; }
		Blend = UTCInfantryRigLibrary::BlendFor(Owner->UnitType, Owner->Motion);
		// mesh bone index -> solver bone index, by name, so any bone order or extra bones in the asset still work
		const USkeletalMeshComponent* Component = Owner->GetSkelMeshComponent();
		const USkeletalMesh* Mesh = Component ? Component->GetSkeletalMeshAsset() : nullptr;
		if (Mesh != MappedMesh)
		{
			MappedMesh = Mesh;
			MeshToRig.Reset();
			if (Mesh)
			{
				const FReferenceSkeleton& Ref = Mesh->GetRefSkeleton();
				const TArray<FName> Names = UTCInfantryRigLibrary::BoneNames();
				for (int32 I = 0; I < Ref.GetNum(); I++) MeshToRig.Add(Names.IndexOfByKey(Ref.GetBoneName(I)));
			}
		}
	}

	virtual bool Evaluate(FPoseContext& Output) override
	{
		constexpr int32 NumBones = UTCInfantryRigLibrary::NumBones;
		if (Keys.Num() != UTCInfantryRigLibrary::NumKeys * NumBones || MeshToRig.IsEmpty()) return false;
		// the keyframes blended as unit-models.js weights its morph targets (key 0 takes what the others leave)
		float Weights[UTCInfantryRigLibrary::NumKeys];
		float Others = 0.f;
		for (int32 K = 1; K < UTCInfantryRigLibrary::NumKeys; K++) { Weights[K] = Blend.KeyWeights.IsValidIndex(K) ? Blend.KeyWeights[K] : 0.f; Others += Weights[K]; }
		Weights[0] = 1.f - Others;
		TCInfantryRigCore::FM NodeRef = TCInfantryRigCore::PoseNode(Blend.Lean, Blend.HeightScale, Blend.Shift.X / 100.0, Blend.Shift.Y / 100.0);
		const FTransform Node = TCInfantryRigCore::ToUnreal(NodeRef);
		FTransform Rig[NumBones];
		for (int32 B = 0; B < NumBones; B++)
		{
			FVector T = FVector::ZeroVector, S = FVector::ZeroVector;
			FQuat Q(0, 0, 0, 0);
			const FQuat Lead = Keys[B].GetRotation();
			for (int32 K = 0; K < UTCInfantryRigLibrary::NumKeys; K++)
			{
				if (Weights[K] == 0.f) continue;
				const FTransform& X = Keys[K * NumBones + B];
				FQuat R = X.GetRotation();
				if ((R | Lead) < 0) R = -R;
				T += X.GetTranslation() * Weights[K];
				S += X.GetScale3D() * Weights[K];
				Q = Q + R * Weights[K];
			}
			Q.Normalize();
			Rig[B] = FTransform(Q, T, S) * Node;
		}
		const FBoneContainer& Required = Output.Pose.GetBoneContainer();
		TArray<FTransform, TInlineAllocator<64>> Component;
		Component.SetNum(Output.Pose.GetNumBones());
		for (FCompactPoseBoneIndex Index : Output.Pose.ForEachBoneIndex())
		{
			const int32 MeshIndex = Required.MakeMeshPoseIndex(Index).GetInt();
			const int32 RigIndex = MeshToRig.IsValidIndex(MeshIndex) ? MeshToRig[MeshIndex] : INDEX_NONE;
			const FCompactPoseBoneIndex Parent = Required.GetParentBoneIndex(Index);
			const FTransform ParentComponent = Parent.IsValid() ? Component[Parent.GetInt()] : FTransform::Identity;
			Component[Index.GetInt()] = RigIndex != INDEX_NONE ? Rig[RigIndex] : Output.Pose.GetRefPose(Index) * ParentComponent;
			Output.Pose[Index] = Parent.IsValid() ? Component[Index.GetInt()].GetRelativeTransform(ParentComponent) : Component[Index.GetInt()];
		}
		return true;
	}

	TArray<FTransform> Keys;
	FString BakedFor;
	FTCInfantryBlend Blend;
	const USkeletalMesh* MappedMesh = nullptr;
	TArray<int32> MeshToRig;
};

void UTCInfantryAnimInstance::NativeInitializeAnimation()
{
	Super::NativeInitializeAnimation();
	RebuildFigure();
}

void UTCInfantryAnimInstance::RebuildFigure()
{
	const FString Key = FString::Printf(TEXT("%s|%d|%d"), *UnitType, Faction, Slot);
	if (Key == BakedFor && !Keys.IsEmpty()) return;
	if (!UTCInfantryRigLibrary::BakeKeyframeTransforms(UnitType, Faction, Slot, Keys)) Keys.Reset();
	BakedFor = Key;
}

FAnimInstanceProxy* UTCInfantryAnimInstance::CreateAnimInstanceProxy()
{
	return new FTCInfantryAnimInstanceProxy(this);
}
#endif // TC_INFANTRY_RIG_STANDALONE
