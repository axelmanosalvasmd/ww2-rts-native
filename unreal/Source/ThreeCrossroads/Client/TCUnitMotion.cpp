// Unit motion, ported from the pinned reference (reference/ww2-rts @ e4e579a). See TCUnitMotion.h for the sources.
// The core (namespace TCUnitMotionCore) follows the JavaScript operation for operation in doubles and three.js axes,
// so the Node oracle (unreal/Tools/motion/oracle) compares it directly; with TC_UNIT_MOTION_STANDALONE defined it
// compiles without the engine. The Unreal adapters and the soldier crowd follow it.
#ifndef TC_UNIT_MOTION_STANDALONE
#include "Client/TCUnitMotion.h"
#include "Client/TCModelLibrary.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Engine/Texture2D.h"
#include "Materials/MaterialInstanceConstant.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/MaterialInterface.h"
#include "MeshDescription.h"
#include "StaticMeshAttributes.h"
#include "Misc/Compression.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Client/TCEffects.h"
#include "Client/TCLighting.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "EngineUtils.h"
#include "Engine/World.h"
#endif

#include <cmath>
#include <cstdint>
#include <vector>

namespace TCUnitMotionCore
{
	constexpr double OnePi = 3.141592653589793;
	inline double Angle(double A) { return std::atan2(std::sin(A), std::cos(A)); }
	inline double ClampAbs(double X, double Limit) { return std::fmax(-Limit, std::fmin(Limit, X)); }

	// ------------------------------------------------------------------ model-motion.js moveModel

	enum class EHull : uint8_t { None, Land, Sea };
	inline EHull HullOf(const char* T)
	{
		static const char* Land[] = { "tank", "medium", "tiger", "churchill", "tankdestroyer", "armoredcar", "halftrack", "flaktrack", "rocket", "truck", "lorry" };
		static const char* Sea[] = { "lcvp", "gunboat", "destroyer" };
		auto Eq = [](const char* A, const char* B) { while (*A && *A == *B) { A++; B++; } return *A == *B; };
		for (const char* L : Land) if (Eq(T, L)) return EHull::Land;
		for (const char* S : Sea) if (Eq(T, S)) return EHull::Sea;
		return EHull::None;
	}
	inline bool Wheeled(const char* T)
	{
		auto Eq = [](const char* A, const char* B) { while (*A && *A == *B) { A++; B++; } return *A == *B; };
		return Eq(T, "armoredcar") || Eq(T, "halftrack") || Eq(T, "rocket") || Eq(T, "lorry");
	}
	inline bool Large(const char* T) { const char* D = "destroyer"; while (*T && *T == *D) { T++; D++; } return *T == *D; }

	// v.modelMotion and the visual body it moves (body.rotation.x roll, .z pitch, position.y heave)
	struct FHull
	{
		bool bInit = false, bHidden = false;
		double X = 0, Z = 0, Yaw = 0, Speed = 0, Travel = 0, Time = 0;
		double RotX = 0, RotZ = 0, PosY = 0;
	};
	// moveModel(v, elapsed) for a LAND or SEA type: Root* is v.root.position and rotation.y, Id is v.id
	inline void MoveModel(FHull& M, EHull Kind, bool bWheel, bool bLarge, int32_t Id, double RootX, double RootZ, double RootYaw, bool bVisible, double Elapsed)
	{
		if (Kind == EHull::None) return;
		const bool bSea = Kind == EHull::Sea;
		const double Dt = std::fmin(0.1, std::fmax(0.0, std::isfinite(Elapsed) ? Elapsed : 0.0));
		if (!M.bInit)
		{
			M.bInit = true; M.X = RootX; M.Z = RootZ; M.Yaw = RootYaw; M.Speed = 0; M.Travel = 0; M.Time = 0; M.bHidden = !bVisible;
			return;
		}
		const double DX = RootX - M.X, DZ = RootZ - M.Z;
		const double Distance = std::hypot(DX, DZ), Yaw = RootYaw;
		const bool bHiddenNow = !bVisible, bJump = bHiddenNow || M.bHidden || Distance > 8;
		const double YawStep = Angle(Yaw - M.Yaw);
		M.X = RootX; M.Z = RootZ; M.Yaw = Yaw; M.bHidden = bHiddenNow;
		if (bJump)
		{
			M.Speed = 0; M.Travel = 0;
			M.PosY = M.RotX = M.RotZ = 0;
			return;
		}
		if (!Dt) return;
		// signed speed: the same settling when reversing; large snapshot corrections cannot kick the hull
		const double Speed = ClampAbs((DX * std::cos(Yaw) - DZ * std::sin(Yaw)) / Dt, 12);
		const double Acceleration = ClampAbs((Speed - M.Speed) / Dt, 10), Turn = ClampAbs(YawStep / Dt, 2);
		M.Speed = Speed; M.Travel += std::fmin(Distance, Dt * 12); M.Time += Dt;
		const double Response = 1 - std::exp(-Dt * (bSea ? 3.5 : 9));
		double Pitch, Roll, Heave = 0;
		if (bSea)
		{
			const double Phase = std::fmod(static_cast<double>(Id) * 0.61803398875, 1.0) * OnePi * 2;
			const double Wave = M.Time * (bLarge ? 0.65 : 1.45) + Phase;
			Pitch = std::sin(Wave) * (bLarge ? 0.0015 : 0.009) + Acceleration * 0.0008;
			Roll = std::sin(Wave * 0.73 + 1.1) * (bLarge ? 0.0025 : 0.014) - Turn * std::fabs(Speed) * 0.001;
			Heave = std::sin(Wave * 1.07 + 0.4) * (bLarge ? 0.025 : 0.045);
		}
		else
		{
			const double Moving = std::fmin(1.0, std::fabs(Speed) / 1.5);
			Pitch = Acceleration * (bWheel ? 0.002 : 0.0012) + std::sin(M.Travel * 5) * Moving * 0.002;
			Roll = -Turn * std::fabs(Speed) * (bWheel ? 0.0025 : 0.0012);
		}
		M.RotZ += (ClampAbs(Pitch, bSea ? 0.014 : 0.022) - M.RotZ) * Response;
		M.RotX += (ClampAbs(Roll, bSea ? 0.02 : 0.024) - M.RotX) * Response;
		M.PosY += (Heave - M.PosY) * Response;
		if (!bSea && Speed == 0)
		{
			if (std::fabs(M.RotX) < 0.00001) M.RotX = 0;
			if (std::fabs(M.RotZ) < 0.00001) M.RotZ = 0;
		}
	}

	// ------------------------------------------------------------------ wheel-motion.js moveWheels, wheelAngle

	struct FWheels
	{
		bool bInit = false, bHidden = false;
		double X = 0, Y = 0, Z = 0, Yaw = 0, Travel = 0, Turn = 0;
	};
	// moveWheels(v, dt): P* is v.root.position (height included: travel on hills), Yaw v.root.rotation.y
	inline void MoveWheels(FWheels& M, double PX, double PY, double PZ, double Yaw, bool bVisible, double Dt)
	{
		const double DX = PX - (M.bInit ? M.X : PX), DY = PY - (M.bInit ? M.Y : PY), DZ = PZ - (M.bInit ? M.Z : PZ);
		const double Turn = Angle(Yaw - (M.bInit ? M.Yaw : Yaw)), Horizontal = std::hypot(DX, DZ);
		const bool bSkip = !(Dt > 0) || !bVisible || M.bHidden || std::hypot(DX, DY, DZ) > 8;
		const double Middle = Yaw - Turn / 2;
		M.bInit = true; M.X = PX; M.Y = PY; M.Z = PZ; M.Yaw = Yaw; M.bHidden = !bVisible;
		if (bSkip) return;
		// height adds travel on hills; suspension heave and settling do not move the gameplay root
		const double Forward = DX * std::cos(Middle) - DZ * std::sin(Middle);
		M.Travel += Horizontal > 1e-9 ? Forward * std::hypot(Horizontal, DY) / Horizontal : 0;
		M.Turn += Turn;
	}
	inline double WheelAngle(double Radius, double Side, double Travel, double Turn) { return Radius > 0 ? -(Travel + Turn * Side) / Radius : 0; }

	// ------------------------------------------------------------------ track-motion.js

	constexpr int Samples = 1024, Rows = 128;
	// register(loop) into row Row of Pixels (Samples * Rows * 4 floats): Points holds N (x, y) pairs. Returns the length.
	inline double TrackRegister(float* Pixels, int Row, const double* Points, int N)
	{
		std::vector<double> Arc(N + 1, 0.0);
		for (int I = 0; I < N; I++)
		{
			const double* A = Points + I * 2; const double* B = Points + ((I + 1) % N) * 2;
			Arc[I + 1] = Arc[I] + std::hypot(B[0] - A[0], B[1] - A[1]);
		}
		const double Length = Arc[N];
		int Segment = 0;
		for (int I = 0; I < Samples; I++)
		{
			const double Distance = I * Length / Samples;
			while (Segment < N - 1 && Arc[Segment + 1] < Distance) Segment++;
			const double* A = Points + Segment * 2; const double* B = Points + ((Segment + 1) % N) * 2;
			const double Fraction = (Distance - Arc[Segment]) / (Arc[Segment + 1] - Arc[Segment]);
			float* P = Pixels + (static_cast<size_t>(Row) * Samples + I) * 4;
			// a Float32Array, as the reference's table
			P[0] = static_cast<float>(A[0] + (B[0] - A[0]) * Fraction);
			P[1] = static_cast<float>(A[1] + (B[1] - A[1]) * Fraction);
			P[2] = static_cast<float>(Length);
			P[3] = 0.f;
		}
		return Length;
	}
	// frame(id, phase): (x, y, dx, dy)
	inline void TrackFrame(const float* Pixels, int Id, double Phase, double Out[4])
	{
		const double At = std::fmod(std::fmod(Phase, 1.0) + 1.0, 1.0) * Samples;
		const int Index = static_cast<int>(std::floor(At));
		const double Fraction = At - Index;
		const size_t A = (static_cast<size_t>(Id) * Samples + Index) * 4, B = (static_cast<size_t>(Id) * Samples + (Index + 1) % Samples) * 4;
		const double DX = static_cast<double>(Pixels[B]) - Pixels[A], DY = static_cast<double>(Pixels[B + 1]) - Pixels[A + 1], Length = std::hypot(DX, DY);
		Out[0] = Pixels[A] + DX * Fraction; Out[1] = Pixels[A + 1] + DY * Fraction; Out[2] = DX / Length; Out[3] = DY / Length;
	}
	// trackVertex(geometry, index, travel, turn): Position (x, y, z), Data the vertex's trackData (x, y, z, w)
	inline void TrackVertex(const float* Pixels, const double Position[3], const double Data[4], double Travel, double Turn, double Out[3])
	{
		Out[0] = Position[0]; Out[1] = Position[1]; Out[2] = Position[2];
		if (Data[1] < 0) return;
		const int Id = static_cast<int>(Data[1]);
		double A[4], B[4];
		TrackFrame(Pixels, Id, Data[0], A);
		TrackFrame(Pixels, Id, Data[0] - (Travel + Turn * Data[2]) / Pixels[static_cast<size_t>(Id) * Samples * 4 + 2], B);
		const double C = A[2] * B[2] + A[3] * B[3], S = A[2] * B[3] - A[3] * B[2];
		const double X = Position[0] - A[0], Y = Position[1] - A[1];
		Out[0] = B[0] + C * X - S * Y;
		Out[1] = std::fmax(Data[3], B[1] + S * X + C * Y);
	}

	// ------------------------------------------------------------------ unit-models.js stride (the kaiju)

	struct FKaiju
	{
		bool bHasX = false;
		double X = 0, Z = 0, Phase = 0, T = 0;
		double LegRotZ[2] = { 0, 0 }, LegY[2] = { 7, 7 };
		double HullY = 0, HullRotX = 0, TurretY = 15.4, TailY = 7, TailRotY = 0;
	};
	// stride(v, dt): VX, VZ are v.x, v.z
	inline void Stride(FKaiju& W, double VX, double VZ, double Dt)
	{
		const double Moved = !W.bHasX ? 0 : std::hypot(VX - W.X, VZ - W.Z);
		W.bHasX = true; W.X = VX; W.Z = VZ; W.Phase += Moved / 3.2; W.T += Dt;
		const double S = std::sin(W.Phase), Bob = std::fabs(std::cos(W.Phase)) * 0.35 * std::fmin(1.0, Moved * 20);
		W.LegRotZ[0] = S * 0.5; W.LegRotZ[1] = -S * 0.5;
		W.LegY[0] = 7 + std::fmax(0.0, -S) * 0.4; W.LegY[1] = 7 + std::fmax(0.0, S) * 0.4;
		W.HullY = Bob; W.HullRotX = S * 0.04; W.TurretY = 15.4 + Bob;
		W.TailY = 7 + Bob; W.TailRotY = std::sin(W.T * 0.9 + W.Phase * 0.5) * 0.18;
	}

	// ------------------------------------------------------------------ aircraft.js tick (bank, wobble, propellers)

	constexpr int MaxProps = 4;
	struct FPlane
	{
		bool bPrev = false;
		double PrevRot = 0, Bank = 0, RotX = 0, RotZ = 0;
		int NumProps = 0;
		double PropAngle[MaxProps] = {}, PropSpin[MaxProps] = {};
	};
	// instance(): each propeller starts at a random angle and spins at 22 to 26 rad/s (Math.random there; here two
	// fixed draws per unit and propeller, U in [0, 1))
	inline void PlaneProps(FPlane& P, int Count, const double* AngleDraw, const double* SpinDraw)
	{
		P.NumProps = Count < MaxProps ? Count : MaxProps;
		for (int I = 0; I < P.NumProps; I++) { P.PropAngle[I] = AngleDraw[I] * OnePi * 2; P.PropSpin[I] = 22 + SpinDraw[I] * 4; }
	}
	// tick(v, dt): Rot is v.rot (snapshot sign), Now performance.now() / 1000
	inline void PlaneTick(FPlane& P, int32_t Id, double Rot, bool bVisible, double Dt, double Now)
	{
		if (!bVisible) { P.PrevRot = Rot; P.bPrev = true; return; }
		if (!P.bPrev) { P.PrevRot = Rot; P.bPrev = true; }
		const double Yaw = std::atan2(std::sin(Rot - P.PrevRot), std::cos(Rot - P.PrevRot)) / std::fmax(Dt, 1e-3);
		P.PrevRot = Rot;
		P.Bank += (std::fmax(-0.65, std::fmin(0.65, Yaw * 0.55)) - P.Bank) * (1 - std::exp(-5 * Dt));
		const double Wob = Now + Id;
		P.RotX = P.Bank + std::sin(Wob * 1.7) * 0.02;
		P.RotZ = std::sin(Wob * 1.1) * 0.015;
		for (int I = 0; I < P.NumProps; I++) P.PropAngle[I] += Dt * P.PropSpin[I];
	}

	// ------------------------------------------------------------------ unit-models.js createBodies (the update)

	struct FCorpseBody
	{
		double X = 0, Y = 0, Z = 0, Yaw = 0, Age = 0, Out = -1; // Out < 0: undefined (not fading)
		bool bTossed = false, bFly = false, bFar = false;
		double VX = 0, VY = 0, VZ = 0, RX = 0, RZ = 0, WX = 0, WZ = 0;
	};
	struct FCorpseRulesCore { int Cap = 200, Soft = 184; double Life = 25, Fade = 1.5, Sink = 0.45; };
	// bodies.add: which body to fade and which to drop before a new one joins. Returns the index to drop or -1;
	// sets Out on the oldest living body when the living count reaches Soft.
	inline int BeforeAdd(FCorpseBody* Bodies, int Count, const FCorpseRulesCore& R)
	{
		int Live = 0, Oldest = -1, Most = -1;
		for (int I = 0; I < Count; I++)
		{
			const FCorpseBody& B = Bodies[I];
			if (B.Out < 0) { Live++; if (Oldest < 0 || B.Age > Bodies[Oldest].Age) Oldest = I; }
			else if (Most < 0 || B.Out < Bodies[Most].Out) Most = I;
		}
		if (Live >= R.Soft && Oldest >= 0) Bodies[Oldest].Out = R.Fade;
		if (Count >= R.Cap) return Most >= 0 ? Most : Oldest;
		return -1;
	}
	// one body's step in update(dt, fx, ground): false when it has faded out (drop it). Blast(X, Z, &bx, &bz, &s)
	// stands for fx.blastNear; Rand() for Math.random; Ground for the height under a point.
	template <typename FBlast, typename FRand, typename FGround>
	inline bool StepBody(FCorpseBody& B, const FCorpseRulesCore& R, double Dt, FBlast&& Blast, FRand&& Rand, FGround&& Ground, bool bHasFx)
	{
		B.Age += Dt;
		if (bHasFx && !B.bTossed && B.Age < 0.6)
		{
			double HX = 0, HZ = 0, HS = 0;
			if (Blast(B.X, B.Z, HX, HZ, HS))
			{
				const double DX = B.X - HX, DZ = B.Z - HZ, D0 = std::hypot(DX, DZ), D = D0 != 0 ? D0 : 0.01;
				const double K = std::fmax(0.15, 1 - D / (HS + 0.5));
				const double P = std::sqrt(HS / 3), H = (2 + 6 * K) * P * (0.8 + 0.4 * Rand());
				B.bTossed = true; B.bFly = true;
				B.VX = DX / D * H; B.VY = (3 + 7 * K) * P; B.VZ = DZ / D * H; B.RX = 0; B.RZ = 0;
				B.WX = (Rand() - 0.5) * 16 * K; B.WZ = (Rand() - 0.5) * 16 * K;
			}
		}
		if (B.bFly)
		{
			B.VY -= 18 * Dt;
			B.X += B.VX * Dt; B.Y += B.VY * Dt; B.Z += B.VZ * Dt; B.RX += B.WX * Dt; B.RZ += B.WZ * Dt;
			const double G = Ground(B.X, B.Z);
			if (B.VY < 0 && B.Y <= G) { B.Y = G; B.bFly = false; }
			return true;
		}
		if (B.Out < 0 && B.Age >= R.Life) B.Out = R.Fade;
		if (B.Out < 0) return true;
		B.Out -= Dt;
		return B.Out > 0;
	}
	// place(b): the sink while fading (none in flight)
	inline double BodySink(const FCorpseBody& B, const FCorpseRulesCore& R) { return B.bFly || B.Out < 0 ? 0 : -(1 - B.Out / R.Fade) * R.Sink; }
	inline double BodyFade(const FCorpseBody& B, const FCorpseRulesCore& R) { return B.Out < 0 ? 1 : B.Out / R.Fade; }
} // namespace TCUnitMotionCore

#ifndef TC_UNIT_MOTION_STANDALONE
// ---------------------------------------------------------------------------------------------- Unreal adapters

DEFINE_LOG_CATEGORY_STATIC(LogTCUnitMotion, Log, All);

namespace TCUnitMotionImpl
{
	// a three.js turn of A about (ax, ay, az) -> the Unreal quaternion (see TCUnitMotion.h)
	inline FQuat ThreeTurn(const FVector& ThreeAxis, double A) { return FQuat(FVector(ThreeAxis.X, ThreeAxis.Z, ThreeAxis.Y), -A); }
	inline FQuat TX(double A) { return ThreeTurn(FVector(1, 0, 0), A); }
	inline FQuat TY(double A) { return ThreeTurn(FVector(0, 1, 0), A); }
	inline FQuat TZ(double A) { return ThreeTurn(FVector(0, 0, 1), A); }
	// a three.js position (metres) -> Unreal cm
	inline FVector ThreePoint(double X, double Y, double Z) { return FVector(X * 100.0, Z * 100.0, Y * 100.0); }
	// the unit id and a propeller's index as two draws in [0, 1) (squad-motion.js seedOf's hash)
	inline double Draw(int32 Id, int32 I, uint32 Salt)
	{
		const uint32 A = static_cast<uint32>(Id + 1) * 1664525u, B = static_cast<uint32>(I + 1) * 1013904223u;
		uint32 H = A + B + Salt * 2654435761u;
		H ^= H >> 16; H *= 0x7feb352du; H ^= H >> 15; H *= 0x846ca68bu; H ^= H >> 16;
		return static_cast<double>(H) / 4294967296.0;
	}
	bool IsKaiju(const FString& Type) { return Type == TEXT("kaiju"); }
}

struct FTCUnitMotion::FState
{
	FString Type;
	TCUnitMotionCore::FHull Hull;
	TCUnitMotionCore::FWheels Wheels;
	TCUnitMotionCore::FKaiju Kaiju;
	TCUnitMotionCore::FPlane Plane;
	bool bPlaneSet = false;
	FTCPartMotion Out;
};

FTCUnitMotion::FTCUnitMotion() = default;
FTCUnitMotion::~FTCUnitMotion() { Reset(); }

void FTCUnitMotion::Reset()
{
	for (auto& P : States) delete P.Value;
	States.Reset();
}

bool FTCUnitMotion::HasHullMotion(const FString& Type)
{
	const FTCHARToUTF8 Name(*Type);
	return TCUnitMotionCore::HullOf(Name.Get()) != TCUnitMotionCore::EHull::None;
}

bool FTCUnitMotion::IsSea(const FString& Type)
{
	const FTCHARToUTF8 Name(*Type);
	return TCUnitMotionCore::HullOf(Name.Get()) == TCUnitMotionCore::EHull::Sea;
}

bool FTCUnitMotion::IsPlane(const FString& Type)
{
	// shared/sim.js UNITS[type].air at the pin (aircraft.js ROLE_OF_UNIT)
	return Type == TEXT("fighter") || Type == TEXT("attacker") || Type == TEXT("bomber");
}

double FTCUnitMotion::WheelAngle(double Radius, double Side, double Travel, double Turn)
{
	return TCUnitMotionCore::WheelAngle(Radius, Side, Travel, Turn);
}

const FTCPartMotion* FTCUnitMotion::Find(int32 Id) const
{
	FState* const* S = States.Find(Id);
	return S ? &(*S)->Out : nullptr;
}

void FTCUnitMotion::Forget(int32 Id)
{
	if (FState** S = States.Find(Id)) { delete *S; States.Remove(Id); }
}

void FTCUnitMotion::Retain(const TSet<int32>& Present)
{
	for (auto It = States.CreateIterator(); It; ++It)
		if (!Present.Contains(It.Key())) { delete It.Value(); It.RemoveCurrent(); }
}

const FTCPartMotion& FTCUnitMotion::Update(int32 Id, const FString& Type, const FVector& Root, double Rot, bool bVisible, float Dt, double Now)
{
	using namespace TCUnitMotionCore;
	using namespace TCUnitMotionImpl;
	FState*& S = States.FindOrAdd(Id);
	if (S && S->Type != Type) { delete S; S = nullptr; }
	if (!S) { S = new FState; S->Type = Type; }
	FTCPartMotion& Out = S->Out;
	const FTCHARToUTF8 Name(*Type);
	// three's root: position (x, y, z) metres, rotation.y = -rot
	const double RX = Root.X / 100.0, RY = Root.Z / 100.0, RZ = Root.Y / 100.0, Yaw = -Rot;
	if (IsKaiju(Type))
	{
		// animate(): the kaiju only strides (no hull settle, no wheels)
		Stride(S->Kaiju, RX, RZ, Dt);
		const FKaiju& K = S->Kaiju;
		Out.Body = FTransform::Identity;
		Out.Parts.Reset();
		// rest places (unit-models.js kaiju): legs (0, 7, -/+2.4), hull 0, head (3, 15.4, 0), tail (-3, 7, 0)
		Out.Parts.Add(TEXT("leg_0"), FTransform(TZ(K.LegRotZ[0]), FVector(0, 0, (K.LegY[0] - 7) * 100.0)));
		Out.Parts.Add(TEXT("leg_1"), FTransform(TZ(K.LegRotZ[1]), FVector(0, 0, (K.LegY[1] - 7) * 100.0)));
		Out.Parts.Add(TEXT("hull"), FTransform(TX(K.HullRotX), FVector(0, 0, K.HullY * 100.0)));
		Out.Parts.Add(TEXT("turret"), FTransform(FQuat::Identity, FVector(0, 0, (K.TurretY - 15.4) * 100.0)));
		Out.Parts.Add(TEXT("tail"), FTransform(TY(K.TailRotY), FVector(0, 0, (K.TailY - 7) * 100.0)));
		return Out;
	}
	if (IsPlane(Type))
	{
		if (!S->bPlaneSet)
		{
			S->bPlaneSet = true;
			double A[MaxProps], B[MaxProps];
			for (int32 I = 0; I < MaxProps; I++) { A[I] = Draw(Id, I, 1); B[I] = Draw(Id, I, 2); }
			PlaneProps(S->Plane, MaxProps, A, B);
		}
		PlaneTick(S->Plane, Id, Rot, bVisible, Dt, Now);
		// body.rotation.order = 'ZXY': Rz(z) Rx(x) Ry(0)
		Out.Body = FTransform(TZ(S->Plane.RotZ) * TX(S->Plane.RotX));
		Out.Parts.Reset();
		for (int32 I = 0; I < S->Plane.NumProps; I++) Out.Parts.Add(FName(*FString::Printf(TEXT("prop_%d"), I)), FTransform(TX(S->Plane.PropAngle[I])));
		return Out;
	}
	const EHull Kind = HullOf(Name.Get());
	if (Kind == EHull::None) { Out = FTCPartMotion(); return Out; }
	// animate(): moveModel then moveWheels
	MoveModel(S->Hull, Kind, Wheeled(Name.Get()), Large(Name.Get()), Id, RX, RZ, Yaw, bVisible, Dt);
	MoveWheels(S->Wheels, RX, RY, RZ, Yaw, bVisible, Dt);
	// visualBody: position (0, heave, 0), rotation (roll, 0, pitch) in three's XYZ order
	Out.Body = FTransform(TX(S->Hull.RotX) * TZ(S->Hull.RotZ), FVector(0, 0, S->Hull.PosY * 100.0));
	Out.Parts.Reset();
	Out.WheelTravel = S->Wheels.Travel;
	Out.WheelTurn = S->Wheels.Turn;
	return Out;
}

void FTCUnitMotion::SetDamageAnchor(int32 Id, const FVector& WorldCm)
{
	if (FState** S = States.Find(Id))
		if (*S) { (*S)->Out.bDamageAnchor = true; (*S)->Out.DamageAnchor = WorldCm; }
}

FTransform FTCUnitMotion::PartTransform(const FTCModelEntry& Entry, int32 PartIndex, const FTransform& Unit, const TMap<FName, FQuat>& Turns, const FTCPartMotion& Motion)
{
	if (!Entry.Parts.IsValidIndex(PartIndex)) return Unit;
	const FTCModelPart& Part = Entry.Parts[PartIndex];
	const FQuat* Turn = Turns.Find(Part.Name);
	const FTransform* Move = Motion.Parts.Find(Part.Name);
	const int32 ParentIndex = Part.Parent.IsNone() ? INDEX_NONE : Entry.FindPart(Part.Parent);
	const FTransform Local = ParentIndex == INDEX_NONE ? Part.Rest : Part.Rest.GetRelativeTransform(Entry.Parts[ParentIndex].Rest);
	// three.js: the part's matrix in its parent is T(rest + shift) * R(move) * R(turn); in the part's own frame that is
	// the turn, then the move, then the shift carried back through the rest rotation and scale
	FQuat Q = Turn ? *Turn : FQuat::Identity;
	FVector Shift = FVector::ZeroVector;
	if (Move)
	{
		Q = Move->GetRotation() * Q;
		Shift = Local.GetRotation().UnrotateVector(Move->GetTranslation()) / Local.GetScale3D().ComponentMax(FVector(UE_SMALL_NUMBER));
	}
	const FTransform Own(Q, Shift);
	if (ParentIndex == INDEX_NONE) return Own * Local * Motion.Body * Unit;
	return Own * Local * PartTransform(Entry, ParentIndex, Unit, Turns, Motion);
}

// ---------------------------------------------------------------------------------------------- track paths

FTCTrackPaths::FTCTrackPaths()
{
	Table.Init(0.f, Samples * Rows * 4);
}

int32 FTCTrackPaths::Register(const TArray<FVector2D>& Points)
{
	if (Points.Num() < 2) return INDEX_NONE;
	for (int32 R = 0; R < Keys.Num(); R++) if (Keys[R] == Points) return R;
	if (Keys.Num() >= Rows) return INDEX_NONE;
	TArray<double> Flat;
	for (const FVector2D& P : Points) { Flat.Add(P.X); Flat.Add(P.Y); }
	const int32 Row = Keys.Num();
	Lengths.Add(TCUnitMotionCore::TrackRegister(Table.GetData(), Row, Flat.GetData(), Points.Num()));
	Keys.Add(Points);
	return Row;
}

FVector4 FTCTrackPaths::Frame(int32 Row, double Phase) const
{
	if (!Lengths.IsValidIndex(Row)) return FVector4(0, 0, 1, 0);
	double Out[4];
	TCUnitMotionCore::TrackFrame(Table.GetData(), Row, Phase, Out);
	return FVector4(Out[0], Out[1], Out[2], Out[3]);
}

FVector FTCTrackPaths::TrackVertex(const FVector& Position, const FVector4& TrackData, double Travel, double Turn) const
{
	if (TrackData.Y < 0 || !Lengths.IsValidIndex(static_cast<int32>(TrackData.Y))) return Position;
	const double P[3] = { Position.X, Position.Y, Position.Z }, D[4] = { TrackData.X, TrackData.Y, TrackData.Z, TrackData.W };
	double Out[3];
	TCUnitMotionCore::TrackVertex(Table.GetData(), P, D, Travel, Turn, Out);
	return FVector(Out[0], Out[1], Out[2]);
}

UTexture2D* FTCTrackPaths::CreateTexture() const
{
	UTexture2D* Texture = UTexture2D::CreateTransient(Samples, Rows, PF_A32B32G32R32F);
	if (!Texture) return nullptr;
	Texture->Filter = TF_Nearest;
	Texture->SRGB = false;
	Texture->NeverStream = true;
	FTexture2DMipMap& Mip = Texture->GetPlatformData()->Mips[0];
	void* Data = Mip.BulkData.Lock(LOCK_READ_WRITE);
	FMemory::Memcpy(Data, Table.GetData(), Table.Num() * sizeof(float));
	Mip.BulkData.Unlock();
	Texture->UpdateResource();
	return Texture;
}

// ---------------------------------------------------------------------------------------------- wheels and tracks

const TCHAR* const FTCWheelTracks::TextureParameter = TEXT("TrackPaths");

bool FTCWheelTracks::Load(FString& OutError, const FString& InFile)
{
	bLoaded = false;
	Hulls.Reset();
	Table = FTCTrackPaths();
	const FString File = InFile.IsEmpty() ? FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Models/Motion/WheelTracks.json")) : InFile;
	FString Text;
	if (!FFileHelper::LoadFileToString(Text, *File)) { OutError = FString::Printf(TEXT("no %s (unreal/Tools/motion/make-wheel-tracks.py): wheels and tracks stand still"), *File); return false; }
	TSharedPtr<FJsonObject> Root;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root) || !Root.IsValid() || Root->GetStringField(TEXT("format")) != TEXT("tc-wheel-tracks-v1"))
	{
		OutError = FString::Printf(TEXT("%s is not a tc-wheel-tracks-v1 file"), *File);
		return false;
	}
	const TSharedPtr<FJsonObject>* Data = nullptr;
	if (!Root->TryGetObjectField(TEXT("custom_data"), Data) || (int32)(*Data)->GetNumberField(TEXT("travel")) != TravelFloat || (int32)(*Data)->GetNumberField(TEXT("turn")) != TurnFloat)
	{
		OutError = TEXT("WheelTracks.json: custom data is not travel 8, turn 9");
		return false;
	}
	for (const TSharedPtr<FJsonValue>& V : Root->GetArrayField(TEXT("loops")))
	{
		const TSharedPtr<FJsonObject> L = V->AsObject();
		TArray<FVector2D> Points;
		for (const TSharedPtr<FJsonValue>& P : L->GetArrayField(TEXT("points")))
		{
			const TArray<TSharedPtr<FJsonValue>>& XY = P->AsArray();
			if (XY.Num() == 2) Points.Add(FVector2D(XY[0]->AsNumber(), XY[1]->AsNumber()));
		}
		const int32 Want = (int32)L->GetNumberField(TEXT("row"));
		const int32 Row = Table.Register(Points);
		if (Row != Want) { OutError = FString::Printf(TEXT("WheelTracks.json: loop %d registered as row %d"), Want, Row); return false; }
	}
	const TSharedPtr<FJsonObject>* Meshes = nullptr;
	if (Root->TryGetObjectField(TEXT("meshes"), Meshes))
		for (const auto& P : (*Meshes)->Values)
		{
			const TSharedPtr<FJsonObject> M = P.Value->AsObject();
			FHull H;
			H.Mesh = FSoftObjectPath(M->GetStringField(TEXT("mesh")));
			for (const TSharedPtr<FJsonValue>& Mat : M->GetArrayField(TEXT("materials"))) H.Materials.Add(FSoftObjectPath(Mat->AsString()));
			for (const TSharedPtr<FJsonValue>& L : M->GetArrayField(TEXT("loops"))) H.Loops.Add((int32)L->AsNumber());
			H.WheelVertices = (int32)M->GetNumberField(TEXT("wheel_vertices"));
			H.LinkVertices = (int32)M->GetNumberField(TEXT("link_vertices"));
			bool bRows = true;
			for (const int32 R : H.Loops) bRows &= R >= 0 && R < Table.Num();
			if (!bRows) { OutError = FString::Printf(TEXT("WheelTracks.json: %s names a loop row that is not there"), *FString(P.Key)); return false; }
			Hulls.Add(FString(P.Key), MoveTemp(H));
		}
	bLoaded = true;
	return true;
}

UTexture2D* FTCWheelTracks::Texture()
{
	if (UTexture2D* T = Made.Get()) return T;
	UTexture2D* T = Table.CreateTexture();
	Made = T;
	return T;
}

// ---------------------------------------------------------------------------------------------- the soldier crowd

namespace TCUnitMotionImpl
{
	const TCHAR* FactionTag(int32 Faction)
	{
		static const TCHAR* Tags[] = { TEXT("US"), TEXT("DE"), TEXT("SU"), TEXT("UK") };
		return Tags[FMath::Clamp(Faction, 0, 3)];
	}
	FString FigureKey(const FString& FigureType, int32 Faction, const FString& Job) { return FString::Printf(TEXT("%s|%d|%s"), *FigureType, Faction, *Job); }
	// FarFigures.bin (unreal/Tools/motion/export-far-figures.mjs): "TCFARFG1", u32 version, u32 figures, u32 keys,
	// u32 compressed and uncompressed sizes, then the zlib payload: per figure a u16-length key, u32 vertices,
	// u32 indices, f32 near and far fallen lowest y, f32 positions and normals [keys][vertices][3], f32 base and tint
	// colors [vertices][3], f32 material tags [vertices] (v2), u16 indices.
	constexpr uint32 FarVersion = 2;
	struct FReader
	{
		const uint8* P; const uint8* End; bool bOk = true;
		template <typename T> T Get() { T V{}; if (P + sizeof(T) > End) { bOk = false; return V; } FMemory::Memcpy(&V, P, sizeof(T)); P += sizeof(T); return V; }
		void Floats(TArray<float>& Out, int64 N) { if (P + N * 4 > End) { bOk = false; return; } Out.SetNumUninitialized(N); FMemory::Memcpy(Out.GetData(), P, N * 4); P += N * 4; }
	};
	// one far-figure vertex's keyframe as a skinning matrix: the turn that carries its rest normal onto the frame's,
	// and the shift that then puts its rest point on the frame's point. Blending these is blending the reference's
	// morph targets (positions and normals) exactly, so the crowd material's skinning plays the far figure's morphs.
	FMatrix44f VertexKey(const FVector& P0, const FVector& N0, const FVector& Pk, const FVector& Nk)
	{
		const FVector A = N0.GetSafeNormal(), B = Nk.GetSafeNormal();
		const FQuat Q = A.IsNearlyZero() || B.IsNearlyZero() ? FQuat::Identity : FQuat::FindBetweenNormals(A, B);
		const FVector T = Pk - Q.RotateVector(P0);
		return FMatrix44f(FQuatRotationTranslationMatrix(Q, T));
	}
	FVector ThreeToUnreal(const float* V, double Scale) { return FVector(V[0] * Scale, V[2] * Scale, V[1] * Scale); }
}

UTCSoldierCrowd::UTCSoldierCrowd()
{
	PrimaryComponentTick.bCanEverTick = false;
	SetMobility(EComponentMobility::Movable);
}

bool UTCSoldierCrowd::Load(FString& OutError)
{
	bLoaded = true;
	TArray<FString> Problems;
	const FString KitsPath = FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Models/Infantry/Source/kits.json"));
	FString Text;
	TSharedPtr<FJsonObject> Root;
	if (FFileHelper::LoadFileToString(Text, *KitsPath) && FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root) && Root.IsValid())
	{
		const TArray<TSharedPtr<FJsonValue>>* List;
		if (Root->TryGetArrayField(TEXT("meshes"), List))
			for (const TSharedPtr<FJsonValue>& V : *List)
			{
				const TSharedPtr<FJsonObject>& O = V->AsObject();
				TArray<FString>& Mats = KitMaterials.Add(O->GetStringField(TEXT("name")));
				for (const TSharedPtr<FJsonValue>& M : O->GetArrayField(TEXT("materials"))) Mats.Add(M->AsString());
			}
		if (Root->TryGetArrayField(TEXT("kits"), List))
			for (const TSharedPtr<FJsonValue>& V : *List)
			{
				const TSharedPtr<FJsonObject>& O = V->AsObject();
				FKit Kit;
				for (const TSharedPtr<FJsonValue>& S : O->GetArrayField(TEXT("sections"))) Kit.Sections.Add(static_cast<int32>(S->AsNumber()));
				for (const TSharedPtr<FJsonValue>& A : O->GetArrayField(TEXT("attachments")))
					Kit.Attachments.Add({ FName(*A->AsObject()->GetStringField(TEXT("bone"))), A->AsObject()->GetStringField(TEXT("mesh")) });
				Kits.Add(TCUnitMotionImpl::FigureKey(O->GetStringField(TEXT("type")), static_cast<int32>(O->GetNumberField(TEXT("fac"))), O->GetStringField(TEXT("job"))), MoveTemp(Kit));
			}
	}
	else Problems.Add(FString::Printf(TEXT("no soldier kits (%s)"), *KitsPath));
	CrowdFaded = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Models/Motion/M_InfantryCrowdFaded.M_InfantryCrowdFaded"));
	CrowdKitFaded = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Models/Motion/M_InfantryCrowdKitFaded.M_InfantryCrowdKitFaded"));
	// the near crowd in the game's model look (unreal/Tools/motion/make-crowd-look.py: unit-models.js INFANTRY_PAINT, the
	// model textures and the reference's light, as the model masters draw it); without them the atlas-only materials
	// of import_infantry.py, lit by the engine's sky light (measured 1.86 Delta E warmer and lighter than the reference)
	CrowdLookFaded = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Models/Motion/M_InfantryCrowdLookFaded.M_InfantryCrowdLookFaded"));
	CrowdKitLookFaded = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Models/Motion/M_InfantryCrowdKitLookFaded.M_InfantryCrowdKitLookFaded"));
	// -TCCrowdAtlasLook: the atlas-only materials anyway (a verification switch, for side by sides of the two looks)
	bModelLook = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Models/Motion/MI_InfantryCrowdLook_DE.MI_InfantryCrowdLook_DE")) != nullptr && CrowdLookFaded && CrowdKitLookFaded
		&& !FParse::Param(FCommandLine::Get(), TEXT("TCCrowdAtlasLook"));
	if (!bModelLook) Problems.Add(TEXT("no crowd model look (unreal/Tools/motion/make-crowd-look.py): near men take the atlas-only materials"));
	FarSolid = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Models/Motion/MI_InfantryFarLook.MI_InfantryFarLook"));
	FarFaded = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Models/Motion/MI_InfantryFarLookFaded.MI_InfantryFarLookFaded"));
	if (!FarSolid || !FarFaded)
	{
		Problems.Add(TEXT("no far model look (unreal/Tools/motion/make-crowd-look.py): far men keep the vertex-only fallback"));
		FarSolid = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Models/Motion/M_InfantryFar.M_InfantryFar"));
		FarFaded = LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Models/Motion/M_InfantryFarFaded.M_InfantryFarFaded"));
	}
	if (!CrowdFaded || !CrowdKitFaded) Problems.Add(TEXT("no faded crowd materials (camouflaged near men draw solid, near bodies do not fade)"));
	if (!FarSolid || !FarFaded) Problems.Add(TEXT("no far figure materials"));
	FString FarError;
	if (!LoadFarFigures(FarError)) Problems.Add(FarError);
	if (!FarSolid || !FarFaded) FarData.Reset();
	OutError = FString::Join(Problems, TEXT("; "));
	UE_LOG(LogTCUnitMotion, Log, TEXT("soldier crowd: %d kits, %d far figures%s%s"), Kits.Num(), FarData.Num(), Problems.IsEmpty() ? TEXT("") : TEXT("; "), *OutError);
	return Problems.IsEmpty();
}

bool UTCSoldierCrowd::LoadFarFigures(FString& OutError)
{
	using namespace TCUnitMotionImpl;
	const FString Path = FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Models/Motion/FarFigures.bin"));
	TArray<uint8> File;
	if (!FFileHelper::LoadFileToArray(File, *Path, FILEREAD_Silent)) { OutError = FString::Printf(TEXT("no far figures (%s): far men draw their catalog rest figure"), *Path); return false; }
	FReader Head{ File.GetData(), File.GetData() + File.Num() };
	char Magic[8];
	for (char& C : Magic) C = Head.Get<char>();
	const uint32 Version = Head.Get<uint32>(), Count = Head.Get<uint32>(), Keys = Head.Get<uint32>(), Packed = Head.Get<uint32>(), Unpacked = Head.Get<uint32>();
	if (!Head.bOk || FMemory::Memcmp(Magic, "TCFARFG1", 8) != 0 || Version != FarVersion || Keys != static_cast<uint32>(UTCInfantryRigLibrary::NumKeys) || Head.P + Packed > Head.End)
	{
		OutError = FString::Printf(TEXT("far figures: %s is not a version %u file with %d keys"), *Path, FarVersion, UTCInfantryRigLibrary::NumKeys);
		return false;
	}
	TArray<uint8> Data;
	Data.SetNumUninitialized(Unpacked);
	if (!FCompression::UncompressMemory(NAME_Zlib, Data.GetData(), Unpacked, Head.P, Packed)) { OutError = TEXT("far figures: the payload does not inflate"); return false; }
	FReader R{ Data.GetData(), Data.GetData() + Data.Num() };
	for (uint32 F = 0; F < Count && R.bOk; F++)
	{
		const uint16 KeyLen = R.Get<uint16>();
		if (R.P + KeyLen > R.End) { R.bOk = false; break; }
		const FUTF8ToTCHAR Name(reinterpret_cast<const ANSICHAR*>(R.P), KeyLen);
		const FString Key(Name.Length(), Name.Get());
		R.P += KeyLen;
		FFarShape Shape;
		Shape.Vertices = static_cast<int32>(R.Get<uint32>());
		const int32 NumIndices = static_cast<int32>(R.Get<uint32>());
		Shape.NearMinY = R.Get<float>(); Shape.FarMinY = R.Get<float>();
		R.Floats(Shape.Positions, static_cast<int64>(Keys) * Shape.Vertices * 3);
		R.Floats(Shape.Normals, static_cast<int64>(Keys) * Shape.Vertices * 3);
		R.Floats(Shape.Base, Shape.Vertices * 3);
		R.Floats(Shape.Tint, Shape.Vertices * 3);
		R.Floats(Shape.Materials, Shape.Vertices);
		if (R.P + NumIndices * 2 > R.End) { R.bOk = false; break; }
		Shape.Indices.SetNumUninitialized(NumIndices);
		FMemory::Memcpy(Shape.Indices.GetData(), R.P, NumIndices * 2);
		R.P += NumIndices * 2;
		FarData.Add(Key, MoveTemp(Shape));
	}
	if (!R.bOk || FarData.Num() != static_cast<int32>(Count)) { FarData.Reset(); OutError = TEXT("far figures: the payload is cut short"); return false; }
	return true;
}

UInstancedStaticMeshComponent* UTCSoldierCrowd::MakeBatch(UStaticMesh* Mesh, int32 Floats, const TArray<UMaterialInterface*>& Materials)
{
	UObject* Outer = GetOwner() ? static_cast<UObject*>(GetOwner()) : static_cast<UObject*>(this);
	UInstancedStaticMeshComponent* Ism = NewObject<UInstancedStaticMeshComponent>(Outer);
	Ism->SetStaticMesh(Mesh);
	Ism->SetNumCustomDataFloats(Floats);
	Ism->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	Ism->SetMobility(EComponentMobility::Movable);
	// unit-models.js: soldiers cast no shadow (bakeMeshes(..., shadow false), drawSoldiers' pools castShadow false) and,
	// like every unit model, receive none (receiveShadow is never set): drawn on the units' lighting channel, lit by
	// their unshadowed sun, when the world has it (TCLook::UnitChannel)
	Ism->SetCastShadow(false);
	if (TCLook::HasUnitSun(GetWorld())) Ism->SetLightingChannels(false, true, false);
	// the vertex shader moves the men out of their rest box (prone, the stride, the fallen): not culled early
	Ism->SetBoundsScale(2.f);
	Ism->SetupAttachment(this);
	if (GetWorld()) Ism->RegisterComponent();
	for (int32 S = 0; S < Materials.Num(); S++) if (Materials[S]) Ism->SetMaterial(S, Materials[S]);
	Owned.Add(Ism);
	AllBatches.Add(Ism);
	return Ism;
}

UTCSoldierCrowd::FNearFigure* UTCSoldierCrowd::NearFigure(const FString& FigureType, int32 Faction, int32 Slot)
{
	using namespace TCUnitMotionImpl;
	const FString Job = UTCInfantryRigLibrary::SoldierKit(FigureType, Slot, Faction);
	const FString Key = FigureKey(FigureType, Faction, Job);
	if (FNearFigure* Found = NearFigures.Find(Key)) return Found->bValid ? Found : nullptr;
	FNearFigure& Made = NearFigures.Add(Key);
	const FKit* Kit = Kits.Find(Key);
	const TCHAR* Tag = FactionTag(Faction);
	UStaticMesh* BodyMesh = LoadObject<UStaticMesh>(nullptr, TEXT("/Game/Models/Infantry/SM_InfantryCrowdBody.SM_InfantryCrowdBody"));
	UMaterialInterface* BodyMaterial = bModelLook ? LoadObject<UMaterialInterface>(nullptr, *FString::Printf(TEXT("/Game/Models/Motion/MI_InfantryCrowdLook_%s.MI_InfantryCrowdLook_%s"), Tag, Tag)) : nullptr;
	UMaterialInterface* BodyFaded = CrowdLookFaded;
	if (!BodyMaterial)
	{
		BodyMaterial = LoadObject<UMaterialInterface>(nullptr, *FString::Printf(TEXT("/Game/Models/Infantry/Materials/MI_InfantryCrowd_%s.MI_InfantryCrowd_%s"), Tag, Tag));
		BodyFaded = CrowdFaded;
	}
	if (!Kit || !BodyMesh || !BodyMaterial) return nullptr;
	Made.Keys = UTCInfantryRigLibrary::BakeKeyframeTexture(FigureType, Faction, Slot);
	if (!Made.Keys) return nullptr;
	Owned.Add(Made.Keys);
	UStaticMesh* CrownMesh = Kit->Sections.Contains(1) ? LoadObject<UStaticMesh>(nullptr, TEXT("/Game/Models/Infantry/SM_InfantryCrowdCrown.SM_InfantryCrowdCrown")) : nullptr;
	const TArray<FName> Bones = UTCInfantryRigLibrary::BoneNames();
	// Solid and faded draws: the faded ones (camouflage and the corpses) are made the first time they are needed
	for (int32 Faded = 0; Faded < 2; Faded++)
	{
		if (Faded && (!BodyFaded || !(bModelLook ? CrowdKitLookFaded : CrowdKitFaded))) break;
		auto Look = [&](UMaterialInterface* Solid, UMaterialInterface* FadedParent) -> UMaterialInterface*
		{
			UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(Faded ? FadedParent : Solid, this);
			if (Faded) if (UMaterialInstance* From = Cast<UMaterialInstance>(Solid)) Mid->CopyParameterOverrides(From);
			Mid->SetTextureParameterValue(TEXT("Keyframes"), Made.Keys);
			Owned.Add(Mid);
			return Mid;
		};
		const int32 BodyFloats = Faded ? TCCrowdData::Faded : TCCrowdData::NearSolidBody;
		UMaterialInterface* BodyLook = Look(BodyMaterial, BodyFaded);
		TArray<UMaterialInterface*> BodyMats;
		BodyMats.Init(BodyLook, BodyMesh->GetStaticMaterials().Num());
		Made.Body[Faded] = MakeBatch(BodyMesh, BodyFloats, BodyMats);
		if (CrownMesh)
		{
			TArray<UMaterialInterface*> CrownMats;
			CrownMats.Init(BodyLook, CrownMesh->GetStaticMaterials().Num());
			Made.Crown[Faded] = MakeBatch(CrownMesh, BodyFloats, CrownMats);
		}
		for (const FKitAttachment& A : Kit->Attachments)
		{
			UStaticMesh* Mesh = LoadObject<UStaticMesh>(nullptr, *FString::Printf(TEXT("/Game/Models/Infantry/Kit/SM_InfantryKit_%s.SM_InfantryKit_%s"), *A.Mesh, *A.Mesh));
			const int32 Bone = Bones.IndexOfByKey(A.Bone);
			if (!Mesh || Bone == INDEX_NONE) continue;
			TArray<UMaterialInterface*> Mats;
			if (const TArray<FString>* Names = KitMaterials.Find(A.Mesh))
				for (const FString& N : *Names)
				{
					const FString Safe = N.Replace(TEXT("-"), TEXT("_"));
					UMaterialInterface* Base = bModelLook ? LoadObject<UMaterialInterface>(nullptr, *FString::Printf(TEXT("/Game/Models/Motion/MI_InfantryCrowdKitLook_%s.MI_InfantryCrowdKitLook_%s"), *Safe, *Safe)) : nullptr;
					UMaterialInterface* KitFaded = CrowdKitLookFaded;
					if (!Base)
					{
						Base = LoadObject<UMaterialInterface>(nullptr, *FString::Printf(TEXT("/Game/Models/Infantry/Materials/MI_InfantryCrowdKit_%s.MI_InfantryCrowdKit_%s"), *Safe, *Safe));
						KitFaded = CrowdKitFaded;
					}
					Mats.Add(Base ? Look(Base, KitFaded) : nullptr);
				}
			Made.Kit[Faded].Add({ MakeBatch(Mesh, Faded ? TCCrowdData::Faded : TCCrowdData::NearSolidKit, Mats), static_cast<float>(Bone) });
		}
	}
	Made.bValid = true;
	return &Made;
}

UTCSoldierCrowd::FFarFigure* UTCSoldierCrowd::FarFigure(const FString& FigureType, int32 Faction, int32 Slot)
{
	using namespace TCUnitMotionImpl;
	const FString Job = UTCInfantryRigLibrary::SoldierKit(FigureType, Slot, Faction);
	const FString Key = FigureKey(FigureType, Faction, Job);
	if (FFarFigure* Found = FarFigures.Find(Key)) return Found->bValid ? Found : nullptr;
	FFarFigure& Made = FarFigures.Add(Key);
	const FFarShape* Shape = FarData.Find(Key);
	if (!Shape || !FarSolid || !FarFaded || Shape->Vertices <= 0) return nullptr;
	const int32 V = Shape->Vertices, NumKeys = UTCInfantryRigLibrary::NumKeys;
	// the rest mesh: the standing build, its vertex index and colors in UV channels for M_InfantryFar
	FMeshDescription Desc;
	FStaticMeshAttributes Attributes(Desc);
	Attributes.Register();
	TVertexAttributesRef<FVector3f> Positions = Attributes.GetVertexPositions();
	TVertexInstanceAttributesRef<FVector3f> Normals = Attributes.GetVertexInstanceNormals();
	TVertexInstanceAttributesRef<FVector3f> Tangents = Attributes.GetVertexInstanceTangents();
	TVertexInstanceAttributesRef<float> Signs = Attributes.GetVertexInstanceBinormalSigns();
	TVertexInstanceAttributesRef<FVector4f> Colors = Attributes.GetVertexInstanceColors();
	TVertexInstanceAttributesRef<FVector2f> UVs = Attributes.GetVertexInstanceUVs();
	UVs.SetNumChannels(7);
	const FPolygonGroupID Group = Desc.CreatePolygonGroup();
	Attributes.GetPolygonGroupMaterialSlotNames()[Group] = TEXT("Far");
	TArray<FVertexInstanceID> Instances;
	for (int32 I = 0; I < V; I++)
	{
		const FVertexID Vert = Desc.CreateVertex();
		Positions[Vert] = FVector3f(ThreeToUnreal(&Shape->Positions[I * 3], 100.0));
		const FVertexInstanceID Inst = Desc.CreateVertexInstance(Vert);
		const FVector3f N = FVector3f(ThreeToUnreal(&Shape->Normals[I * 3], 1.0)).GetSafeNormal();
		Normals[Inst] = N;
		const FVector3f Side = FMath::Abs(N.Z) < 0.9f ? FVector3f::UpVector : FVector3f::ForwardVector;
		Tangents[Inst] = FVector3f::CrossProduct(Side, N).GetSafeNormal();
		Signs[Inst] = 1.f;
		Colors[Inst] = FVector4f(1, 1, 1, 1);
		const float* B = &Shape->Base[I * 3]; const float* T = &Shape->Tint[I * 3];
		UVs.Set(Inst, 0, FVector2f(B[0], B[1]));
		UVs.Set(Inst, 1, FVector2f(static_cast<float>(I), Shape->Materials[I])); // keyframe column; unused joint carries the material tag (its weight is zero)
		UVs.Set(Inst, 2, FVector2f(0.f, 0.f));
		UVs.Set(Inst, 3, FVector2f(1.f, 0.f));                    // weights: all on it
		UVs.Set(Inst, 4, FVector2f(0.f, 0.f));
		UVs.Set(Inst, 5, FVector2f(B[2], T[0]));
		UVs.Set(Inst, 6, FVector2f(T[1], T[2]));
		Instances.Add(Inst);
	}
	// three.js counter-clockwise faces stay front faces through the axis swap with the same index order
	for (int32 I = 0; I + 2 < Shape->Indices.Num(); I += 3)
	{
		const uint16 A = Shape->Indices[I], B = Shape->Indices[I + 1], C = Shape->Indices[I + 2];
		if (A >= V || B >= V || C >= V) continue;
		Desc.CreateTriangle(Group, { Instances[A], Instances[B], Instances[C] });
	}
	// named for its figure (SM_Far_rifle_0_leader), unique within this crowd
	UStaticMesh* Mesh = NewObject<UStaticMesh>(this, MakeUniqueObjectName(this, UStaticMesh::StaticClass(), FName(*(TEXT("SM_Far_") + Key.Replace(TEXT("|"), TEXT("_"))))));
	Mesh->GetStaticMaterials().Add(FStaticMaterial(FarSolid, TEXT("Far")));
	UStaticMesh::FBuildMeshDescriptionsParams Params;
	Params.bBuildSimpleCollision = false;
	Params.bFastBuild = true;
	Params.bMarkPackageDirty = false;
	UStaticMesh::FBuildMeshDescriptionsLODParams Lod;
	Lod.bUseFullPrecisionUVs = true; // colors ride in the UVs
	Params.PerLODOverrides.Add(Lod);
	if (!Mesh->BuildFromMeshDescriptions({ &Desc }, Params)) return nullptr;
	// slice 11 lead: run-time meshes carry no texture-streaming UV data (the UVs hold colors here); a default density
	// keeps a cooked game from tripping the engine's 'UVChannelData.bInitialized' ensure
	for (FStaticMaterial& M : Mesh->GetStaticMaterials()) M.UVChannelData = FMeshUVChannelInfo(1.f);
	Owned.Add(Mesh);
	Made.Mesh = Mesh;
	// every vertex's keyframes as skinning matrices, one row per keyframe (BakeKeyframeTexture's layout)
	const int32 Width = V * UTCInfantryRigLibrary::TexelsPerBone;
	UTexture2D* Keys = UTexture2D::CreateTransient(Width, NumKeys, PF_A32B32G32R32F);
	if (!Keys) return nullptr;
	Keys->Filter = TF_Nearest;
	Keys->SRGB = false;
	Keys->AddressX = TA_Clamp;
	Keys->AddressY = TA_Clamp;
	Keys->NeverStream = true;
	FTexture2DMipMap& Mip = Keys->GetPlatformData()->Mips[0];
	float* Texels = static_cast<float*>(Mip.BulkData.Lock(LOCK_READ_WRITE));
	for (int32 K = 0; K < NumKeys; K++)
		for (int32 I = 0; I < V; I++)
		{
			const FMatrix44f M = VertexKey(ThreeToUnreal(&Shape->Positions[I * 3], 100.0), ThreeToUnreal(&Shape->Normals[I * 3], 1.0),
				ThreeToUnreal(&Shape->Positions[(K * V + I) * 3], 100.0), ThreeToUnreal(&Shape->Normals[(K * V + I) * 3], 1.0));
			for (int32 C = 0; C < UTCInfantryRigLibrary::TexelsPerBone; C++)
			{
				float* T = Texels + ((K * Width) + I * UTCInfantryRigLibrary::TexelsPerBone + C) * 4;
				T[0] = M.M[0][C]; T[1] = M.M[1][C]; T[2] = M.M[2][C]; T[3] = M.M[3][C];
			}
		}
	Mip.BulkData.Unlock();
	Keys->UpdateResource();
	Owned.Add(Keys);
	Made.Keys = Keys;
	for (int32 Faded = 0; Faded < 2; Faded++)
	{
		UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(Faded ? FarFaded.Get() : FarSolid.Get(), this);
		Mid->SetTextureParameterValue(TEXT("Keyframes"), Keys);
		Owned.Add(Mid);
		Made.Draw[Faded] = MakeBatch(Mesh, TCCrowdData::Far, { Mid });
	}
	Made.bValid = true;
	return &Made;
}

void UTCSoldierCrowd::Add(UInstancedStaticMeshComponent* Ism, const FTransform& Xf, const float* Data, int32 Floats, uint64 Key)
{
	if (!Ism) return;
	FBatch& B = Frame.FindOrAdd(Ism);
	B.Floats = Floats;
	B.Transforms.Add(Xf);
	B.Keys.Add(Key);
	B.Data.Append(Data, Floats);
}

bool UTCSoldierCrowd::DrawNear(const FString& FigureType, int32 Faction, int32 Slot, const FTransform& Xf, const TArray<float>& Blend12, float Fade, bool bForceFaded, uint64 Key)
{
	FNearFigure* F = NearFigure(FigureType, Faction, Slot);
	if (!F) return false;
	const bool bFaded = (bForceFaded || Fade < 1.f) && F->Body[1];
	const int32 L = bFaded ? 1 : 0;
	float D[TCCrowdData::Faded];
	FMemory::Memcpy(D, Blend12.GetData(), 12 * sizeof(float));
	D[TCCrowdData::Bone] = 0.f;
	D[TCCrowdData::Fade] = FMath::Clamp(Fade, 0.f, 1.f);
	const int32 BodyFloats = bFaded ? TCCrowdData::Faded : TCCrowdData::NearSolidBody;
	Add(F->Body[L], Xf, D, BodyFloats, Key);
	if (F->Crown[L]) Add(F->Crown[L], Xf, D, BodyFloats, Key);
	for (const auto& K : F->Kit[L])
	{
		D[TCCrowdData::Bone] = K.Value;
		Add(K.Key, Xf, D, bFaded ? TCCrowdData::Faded : TCCrowdData::NearSolidKit, Key);
	}
	if (bFaded) Now.FadedMen++;
	return true;
}

bool UTCSoldierCrowd::DrawFar(const FString& FigureType, int32 Faction, int32 Slot, const FTransform& Xf, const TArray<float>& Blend12, float Fade, const FLinearColor& Owner, bool bForceFaded, uint64 Key)
{
	FFarFigure* F = FarFigure(FigureType, Faction, Slot);
	if (!F) return false;
	const bool bFaded = bForceFaded || Fade < 1.f;
	float D[TCCrowdData::Far];
	FMemory::Memcpy(D, Blend12.GetData(), 12 * sizeof(float));
	D[TCCrowdData::Bone] = 0.f;
	D[TCCrowdData::Fade] = FMath::Clamp(Fade, 0.f, 1.f);
	D[TCCrowdData::OwnerR] = Owner.R; D[TCCrowdData::OwnerG] = Owner.G; D[TCCrowdData::OwnerB] = Owner.B;
	Add(F->Draw[bFaded ? 1 : 0], Xf, D, TCCrowdData::Far, Key);
	if (bFaded) Now.FadedMen++;
	return true;
}

void UTCSoldierCrowd::BeginFrame()
{
	Frame.Reset();
	Now = FTCSoldierCrowdStats();
}

bool UTCSoldierCrowd::AddMan(const FTCSoldierDraw& Man)
{
	// -TCCrowdOff: every man takes the catalog's rest figure (the renderer's fallback; a verification switch)
	static const bool bOff = FParse::Param(FCommandLine::Get(), TEXT("TCCrowdOff"));
	if (bOff) { Now.Refused++; return false; }
	const TArray<float> Blend12 = UTCInfantryRigLibrary::CrowdCustomData(Man.Blend);
	const uint64 Key = Man.ManIndex >= 0 ? FTCInstanceHistory::ManKey(Man.UnitId, Man.ManIndex) : FTCInstanceHistory::NoKey;
	const bool bDrawn = Man.bFar ? DrawFar(Man.FigureType, Man.Faction, Man.Slot, Man.Man, Blend12, Man.Fade, Man.Owner, false, Key)
		: DrawNear(Man.FigureType, Man.Faction, Man.Slot, Man.Man, Blend12, Man.Fade, false, Key);
	if (bDrawn) (Man.bFar ? Now.FarMen : Now.NearMen)++;
	else Now.Refused++;
	if (Man.ManIndex >= 0)
	{
		FSquadRecord& R = Squads.FindOrAdd(Man.UnitId);
		if (R.Men.Num() <= Man.ManIndex) R.Men.SetNum(Man.ManIndex + 1);
		FStood& S = R.Men[Man.ManIndex];
		S.bSet = true; S.FigureType = Man.FigureType; S.Faction = Man.Faction; S.Slot = Man.Slot; S.Owner = Man.Owner; S.Man = Man.Man;
	}
	return bDrawn;
}

void UTCSoldierCrowd::Fall(const FStood& Man)
{
	// main.js corpse(): where he stood, the ground under him, facing his world heading (YXZ's y) give or take 0.2 rad
	const FVector At = Man.Man.GetLocation();
	const double Heading = -FMath::DegreesToRadians(Man.Man.Rotator().Yaw);
	AddCorpse(Man.FigureType, Man.Faction, Man.Slot, Man.Owner, At, Heading + (Random.FRand() - 0.5) * 0.4, static_cast<float>(Man.Man.GetScale3D().X));
}

void UTCSoldierCrowd::SquadAlive(int32 UnitId, int32 Alive)
{
	FSquadRecord& R = Squads.FindOrAdd(UnitId);
	if (R.Alive >= 0 && Alive < R.Alive)
		for (int32 I = R.Alive - 1; I >= Alive; I--) // while (v.alive > alive) corpse(v, v.models[--v.alive])
			if (R.Men.IsValidIndex(I) && R.Men[I].bSet) { Fall(R.Men[I]); R.Men[I].bSet = false; }
	R.Alive = Alive;
}

void UTCSoldierCrowd::SquadGone(int32 UnitId, bool bKilled)
{
	if (FSquadRecord* R = Squads.Find(UnitId))
	{
		if (bKilled)
			for (int32 I = 0; I < R->Men.Num() && I < (R->Alive >= 0 ? R->Alive : R->Men.Num()); I++)
				if (R->Men[I].bSet) Fall(R->Men[I]);
		Squads.Remove(UnitId);
	}
}

namespace TCUnitMotionImpl
{
	TCUnitMotionCore::FCorpseRulesCore CoreRules(const FTCCorpseRules& R)
	{
		TCUnitMotionCore::FCorpseRulesCore C;
		C.Cap = R.Cap; C.Soft = R.Soft; C.Life = R.Life; C.Fade = R.Fade; C.Sink = R.Sink;
		return C;
	}
}

void UTCSoldierCrowd::AddCorpse(const FString& FigureType, int32 Faction, int32 Slot, const FLinearColor& Owner, const FVector& Where, double Yaw, float Scale)
{
	using namespace TCUnitMotionCore;
	// bodies.add: past Soft living bodies the oldest starts to fade; at Cap one goes at once
	TArray<FCorpseBody> Core;
	for (const FCorpse& C : Corpses)
	{
		FCorpseBody B; B.Age = C.Age; B.Out = C.Out;
		Core.Add(B);
	}
	const int32 Drop = BeforeAdd(Core.GetData(), Core.Num(), TCUnitMotionImpl::CoreRules(Rules));
	for (int32 I = 0; I < Corpses.Num(); I++) Corpses[I].Out = Core[I].Out;
	if (Drop >= 0) DropCorpse(Drop);
	FCorpse C;
	C.FigureType = FigureType; C.Faction = Faction; C.Slot = Slot; C.Owner = Owner; C.Scale = Scale;
	C.X = Where.X / 100.0; C.Z = Where.Y / 100.0; C.Y = TNumericLimits<double>::Lowest(); // the ground, at the next EndFrame
	C.Yaw = Yaw;
	C.Serial = NextSerial++;
	Corpses.Add(C);
}

void UTCSoldierCrowd::DropCorpse(int32 Index)
{
	// swap with the last, as bodies.drop does
	Corpses.RemoveAtSwap(Index, EAllowShrinking::No);
}

int32 UTCSoldierCrowd::NumCorpses() const { return Corpses.Num(); }

bool UTCSoldierCrowd::CheckFarKeys(double& OutWorstPositionCm, double& OutWorstNormalRad, int32& OutFigures) const
{
	using namespace TCUnitMotionImpl;
	OutWorstPositionCm = OutWorstNormalRad = 0;
	OutFigures = FarData.Num();
	for (const auto& P : FarData)
	{
		const FFarShape& Shape = P.Value;
		const int32 V = Shape.Vertices;
		for (int32 K = 0; K < UTCInfantryRigLibrary::NumKeys; K++)
			for (int32 I = 0; I < V; I++)
			{
				const FVector P0 = ThreeToUnreal(&Shape.Positions[I * 3], 100.0), N0 = ThreeToUnreal(&Shape.Normals[I * 3], 1.0);
				const FVector Pk = ThreeToUnreal(&Shape.Positions[(K * V + I) * 3], 100.0), Nk = ThreeToUnreal(&Shape.Normals[(K * V + I) * 3], 1.0);
				const FMatrix44f M = VertexKey(P0, N0, Pk, Nk);
				// as the crowd material reads it: p' = (p, 1) . columns, in float
				const FVector3f Got(M.TransformPosition(FVector3f(P0)));
				const FVector3f GotN = FVector3f(M.TransformVector(FVector3f(N0))).GetSafeNormal();
				OutWorstPositionCm = FMath::Max(OutWorstPositionCm, FVector::Dist(FVector(Got), Pk));
				if (!N0.IsNearlyZero() && !Nk.IsNearlyZero())
					OutWorstNormalRad = FMath::Max(OutWorstNormalRad, FMath::Acos(FMath::Clamp(FVector(GotN) | Nk.GetSafeNormal(), -1.0, 1.0)));
			}
	}
	return OutFigures > 0;
}

void UTCSoldierCrowd::ClearCorpses() { Corpses.Reset(); Squads.Reset(); History.Reset(); }

void UTCSoldierCrowd::EndFrame(float Dt, const FVector& Eye, TFunctionRef<double(double X, double Y)> GroundCm, float LodMetres,
	TFunction<bool(double X, double Z, double& OutX, double& OutZ, double& OutSize)> BlastNear)
{
	using namespace TCUnitMotionCore;
	using namespace TCUnitMotionImpl;
	const FCorpseRulesCore R = CoreRules(Rules);
	auto Ground = [&GroundCm](double X, double Z) { return GroundCm(X * 100.0, Z * 100.0) / 100.0; };
	for (FCorpse& C : Corpses) if (C.Y == TNumericLimits<double>::Lowest()) C.Y = Ground(C.X, C.Z);
	// main.js bodies.update(sdt, effects, hAt, camera): without a caller's BlastNear the game's effects answer it
	// (fx.js blastNear), and a body thrown by a blast gets fx.gore's blood
	ATCEffects* Fx = nullptr;
	if (!BlastNear && Corpses.Num())
	{
		if (!EffectsSeen.IsValid() && GetWorld())
		{
			TActorIterator<ATCEffects> It(GetWorld());
			if (It) EffectsSeen = *It;
		}
		Fx = EffectsSeen.Get();
	}
	const bool bHasFx = static_cast<bool>(BlastNear) || Fx != nullptr;
	// bodies.update: age, toss, fall, fade out
	for (int32 I = Corpses.Num() - 1; I >= 0; I--)
	{
		FCorpse& C = Corpses[I];
		FCorpseBody B;
		B.X = C.X; B.Y = C.Y; B.Z = C.Z; B.Yaw = C.Yaw; B.Age = C.Age; B.Out = C.Out; B.bTossed = C.bTossed; B.bFly = C.bFly; B.bFar = C.bFar;
		B.VX = C.VX; B.VY = C.VY; B.VZ = C.VZ; B.RX = C.RX; B.RZ = C.RZ; B.WX = C.WX; B.WZ = C.WZ;
		double HitX = 0, HitZ = 0, HitS = 0;
		auto Blast = [&BlastNear, Fx, &HitX, &HitZ, &HitS](double X, double Z, double& HX, double& HZ, double& HS)
		{
			const bool bHit = BlastNear ? BlastNear(X, Z, HX, HZ, HS) : Fx ? Fx->Fx.BlastNear(X, Z, HX, HZ, HS) : false;
			if (bHit) { HitX = HX; HitZ = HZ; HitS = HS; }
			return bHit;
		};
		auto Rand = [this]() { return static_cast<double>(Random.FRand()); };
		const bool bWasTossed = B.bTossed;
		const bool bKeep = StepBody(B, R, Dt, Blast, Rand, Ground, bHasFx);
		if (B.bTossed && !bWasTossed)
		{
			Tosses++;
			if (Fx)
			{
				// fx.gore(b.x, b.z, k), k as the toss computed it (StepBody), at the place the body lay
				const double D0 = FMath::Sqrt(FMath::Square(C.X - HitX) + FMath::Square(C.Z - HitZ)), D = D0 != 0 ? D0 : 0.01;
				Fx->Fx.Gore(C.X, C.Z, FMath::Max(0.15, 1 - D / (HitS + 0.5)));
				Fx->Fx.NoteToss();
			}
		}
		C.X = B.X; C.Y = B.Y; C.Z = B.Z; C.Age = B.Age; C.Out = B.Out; C.bTossed = B.bTossed; C.bFly = B.bFly;
		C.VX = B.VX; C.VY = B.VY; C.VZ = B.VZ; C.RX = B.RX; C.RZ = B.RZ; C.WX = B.WX; C.WZ = B.WZ;
		if (!bKeep) DropCorpse(I);
	}
	// draw: the fallen build, near or far by each body's own distance (4 m of hysteresis), fading as it sinks
	FTCInfantryMotion Fallen;
	Fallen.bFallen = true;
	const TArray<float> Blend12 = UTCInfantryRigLibrary::CrowdCustomData(UTCInfantryRigLibrary::BlendFor(TEXT("rifle"), Fallen));
	for (FCorpse& C : Corpses)
	{
		FCorpseBody B; B.Out = C.Out; B.bFly = C.bFly;
		const double Y = C.Y + BodySink(B, R);
		const FVector At = ThreePoint(C.X, Y, C.Z);
		const double D2 = FVector::DistSquared(At, Eye) / 10000.0;
		C.bFar = C.bFar ? D2 > FMath::Square(LodMetres - 4.0) : D2 > FMath::Square(LodMetres + 4.0);
		// place(): Euler (rx, yaw, rz) in YXZ order; fallenGeometry: scaled and resting on y = 0 (its lowest point)
		const FQuat Q = TY(C.Yaw) * TX(C.RX) * TZ(C.RZ);
		float MinY = 0.f;
		const FString Job = UTCInfantryRigLibrary::SoldierKit(C.FigureType, C.Slot, C.Faction);
		if (const FFarShape* Shape = FarData.Find(FigureKey(C.FigureType, C.Faction, Job))) MinY = C.bFar ? Shape->FarMinY : Shape->NearMinY;
		const FTransform Xf(Q, At + Q.RotateVector(FVector(0, 0, -MinY * C.Scale * 100.0)), FVector(C.Scale));
		const float Fade = static_cast<float>(BodyFade(B, R));
		const uint64 Key = FTCInstanceHistory::BodyKey(C.Serial);
		const bool bDrawn = C.bFar ? DrawFar(C.FigureType, C.Faction, C.Slot, Xf, Blend12, Fade, C.Owner, true, Key)
			: DrawNear(C.FigureType, C.Faction, C.Slot, Xf, Blend12, Fade, true, Key);
		if (bDrawn) { Now.Corpses++; (C.bFar ? Now.FarCorpses : Now.NearCorpses)++; }
	}
	// write every component's instances, with last frame's transforms (round 17: motion vectors)
	History.ResetCounts();
	for (UInstancedStaticMeshComponent* Ism : AllBatches)
	{
		if (!Ism) continue;
		const FBatch* B = Frame.Find(Ism);
		const int32 N = B ? B->Transforms.Num() : 0;
		if (Ism->GetInstanceCount() == 0 && N == 0) continue;
		History.Write(Ism, B ? B->Transforms : TArray<FTransform>(), B ? B->Keys : TArray<uint64>());
		if (N && B->Floats > 0 && B->Data.Num() == N * B->Floats) Ism->SetCustomData(0, N - 1, B->Data, false);
		Ism->MarkRenderStateDirty();
	}
	Now.Components = AllBatches.Num();
	for (const auto& P : NearFigures) if (P.Value.bValid) Now.NearFigures++;
	for (const auto& P : FarFigures) if (P.Value.bValid) Now.FarFigures++;
	Last = Now;
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "StaticMeshResources.h"

// The arithmetic is checked against the reference under Node (unreal/Tools/motion/oracle); these check the Unreal
// adapters: axes, signs, composition, the asset paths and the crowd's bookkeeping.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMotionVehiclesTest, "ThreeCrossroads.Motion.Vehicles", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMotionVehiclesTest::RunTest(const FString& Parameters)
{
	FTCUnitMotion Motion;
	// a Sherman facing +X pulls away: the hull squats (nose up), then settles; the wheels roll the ground covered
	FVector At(1000, 2000, 0);
	Motion.Update(1, TEXT("medium"), At, 0.0, true, 0.016f, 0.0);
	double Speed = 0;
	for (int32 I = 0; I < 30; I++) { Speed = FMath::Min(Speed + 0.4, 6.0); At.X += Speed * 100.0 * 0.016; Motion.Update(1, TEXT("medium"), At, 0.0, true, 0.016f, 0.0); }
	const FTCPartMotion* M = Motion.Find(1);
	TestNotNull(TEXT("state kept"), M);
	if (!M) return false;
	const FVector Nose = M->Body.GetRotation().RotateVector(FVector::ForwardVector);
	TestTrue(TEXT("accelerating: the nose lifts"), Nose.Z > 1e-4);
	TestTrue(TEXT("rolled ground equals the distance driven forward"), FMath::IsNearlyEqual(M->WheelTravel, (At.X - 1000.0) / 100.0, 1e-6));
	// turning left (rot decreasing is a turn to the left in Unreal's yaw) while moving rolls the hull outward
	for (int32 I = 0; I < 30; I++) { const double Rot = -0.03 * I; At += FVector(FMath::Cos(Rot), FMath::Sin(Rot), 0) * 600.0 * 0.016; Motion.Update(1, TEXT("medium"), At, Rot, true, 0.016f, 0.0); }
	const FVector Up = Motion.Find(1)->Body.GetRotation().RotateVector(FVector::UpVector);
	TestTrue(TEXT("turning: the hull rolls"), FMath::Abs(Up.Y) > 1e-5);
	// a jump past 8 m resets the hull
	Motion.Update(1, TEXT("medium"), At + FVector(5000, 0, 0), -0.87, true, 0.016f, 0.0);
	TestTrue(TEXT("a snapshot jump resets the settle"), Motion.Find(1)->Body.Equals(FTransform::Identity, 1e-9));
	// boats swell even at rest
	FVector Boat(0, 0, 0);
	for (int32 I = 0; I < 60; I++) Motion.Update(2, TEXT("gunboat"), Boat, 0.0, true, 0.05f, 0.0);
	TestTrue(TEXT("a boat heaves on the swell"), FMath::Abs(Motion.Find(2)->Body.GetLocation().Z) > 0.01);
	// the kaiju walks: legs swing opposite ways and the head bobs with the body
	FVector K(0, 0, 0);
	for (int32 I = 0; I < 20; I++) { K.X += 50.0; Motion.Update(3, TEXT("kaiju"), K, 0.0, true, 0.05f, 0.0); }
	const FTCPartMotion* KM = Motion.Find(3);
	const FQuat L0 = KM->Parts.FindChecked(TEXT("leg_0")).GetRotation(), L1 = KM->Parts.FindChecked(TEXT("leg_1")).GetRotation();
	TestTrue(TEXT("kaiju legs swing opposite"), FMath::IsNearlyEqual(L0.GetAngle(), L1.GetAngle(), 1e-6) && (L0.GetRotationAxis() | L1.GetRotationAxis()) < 0);
	TestTrue(TEXT("kaiju legs swing about the lateral axis"), FMath::Abs(L0.GetRotationAxis().Y) > 0.99);
	// a plane's propellers spin about the thrust axis and it banks into a turn
	double Rot = 0;
	for (int32 I = 0; I < 40; I++) { Rot += 0.02; Motion.Update(4, TEXT("fighter"), FVector(0, 0, 2000), Rot, true, 0.016f, I * 0.016); }
	const FTCPartMotion* PM = Motion.Find(4);
	TestTrue(TEXT("propeller turns about X"), FMath::Abs(PM->Parts.FindChecked(TEXT("prop_0")).GetRotation().GetRotationAxis().X) > 0.99);
	TestTrue(TEXT("the plane banks"), FMath::Abs(PM->Body.GetRotation().RotateVector(FVector::UpVector).Y) > 0.05);
	// PartTransform: a kaiju head keeps its aim turn and rises with the bob; parts without motion are untouched
	FTCModelEntry Entry;
	Entry.Type = TEXT("kaiju");
	FTCModelPart Head; Head.Name = TEXT("turret"); Head.Rest = FTransform(FVector(300, 0, 1540));
	Entry.Parts.Add(Head);
	FTCPartMotion Bob; Bob.Parts.Add(TEXT("turret"), FTransform(FQuat::Identity, FVector(0, 0, 30)));
	TMap<FName, FQuat> Turns; Turns.Add(TEXT("turret"), FQuat(FVector::UpVector, HALF_PI));
	const FTransform Drawn = FTCUnitMotion::PartTransform(Entry, 0, FTransform::Identity, Turns, Bob);
	TestTrue(TEXT("the head rises by the bob"), Drawn.GetLocation().Equals(FVector(300, 0, 1570), 1e-6));
	TestTrue(TEXT("the head keeps its aim"), Drawn.GetRotation().RotateVector(FVector::ForwardVector).Equals(FVector(0, 1, 0), 1e-6));
	Motion.Retain({ 1 });
	TestNull(TEXT("gone units forgotten"), Motion.Find(3));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMotionTracksTest, "ThreeCrossroads.Motion.Tracks", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMotionTracksTest::RunTest(const FString& Parameters)
{
	// a 4 m by 1 m loop; a link on its lower run, phase 0.1
	FTCTrackPaths Paths;
	const TArray<FVector2D> Loop = { { -2, 0 }, { 2, 0 }, { 2, 1 }, { -2, 1 } };
	const int32 Row = Paths.Register(Loop);
	TestEqual(TEXT("first row"), Row, 0);
	TestEqual(TEXT("the same loop shares its row"), Paths.Register(Loop), 0);
	TestTrue(TEXT("loop length"), FMath::IsNearlyEqual(Paths.LengthOf(0), 10.0, 1e-6));
	const FVector4 Start = Paths.Frame(0, 0.1);
	TestTrue(TEXT("phase 0.1 is 1 m along the lower run"), FMath::IsNearlyEqual(Start.X, -1.0, 1e-5) && FMath::IsNearlyEqual(Start.Y, 0.0, 1e-5));
	const FVector Link(-1, 0, 0.5);
	const FVector4 Data(0.1, 0, 0, -10);
	TestTrue(TEXT("no travel, no move"), Paths.TrackVertex(Link, Data, 0, 0).Equals(Link, 1e-5));
	TestTrue(TEXT("a whole loop of travel brings it back"), Paths.TrackVertex(Link, Data, 10.0, 0).Equals(Link, 1e-4));
	TestTrue(TEXT("rolling forward runs the lower run back"), Paths.TrackVertex(Link, Data, 0.5, 0).X < Link.X - 0.4);
	TestTrue(TEXT("a static vertex stays"), Paths.TrackVertex(Link, FVector4(0.1, -1, 0, 0), 3, 1).Equals(Link, 0));
	TestTrue(TEXT("wheel angle"), FMath::IsNearlyEqual(FTCUnitMotion::WheelAngle(0.5, 1.0, 1.0, 0.5), -3.0, 1e-12));
	UTexture2D* Texture = Paths.CreateTexture();
	TestTrue(TEXT("table texture"), Texture && Texture->GetSizeX() == FTCTrackPaths::Samples && Texture->GetSizeY() == FTCTrackPaths::Rows);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMotionSquadTest, "ThreeCrossroads.Motion.SquadAnimation", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMotionSquadTest::RunTest(const FString& Parameters)
{
	// an MG crew (shared/squad-men.js GUN_SLOTS.mg times SLOT_SCALE 1.3)
	const TArray<FVector2D> Slots = { FVector2D(-15.6, 0), FVector2D(-6.5, 110.5), FVector2D(-6.5, -110.5) };
	FTCInfantrySquadAnimation Squad;
	Squad.Setup(TEXT("mg"), 1, Slots, 1.35f);
	TestTrue(TEXT("set up"), Squad.IsSetUp(TEXT("mg"), 1, 3));
	FTCInfantrySquadFrame F;
	F.Root = FVector(1000, 0, 0); F.Eye = FVector(1000, -3000, 2000); F.Id = 9;
	TestTrue(TEXT("first frame posed"), Squad.Animate(F, 0.016f));
	TestFalse(TEXT("near at 36 m"), Squad.IsFar());
	TestEqual(TEXT("gunner kit"), Squad.Men()[0].Job, FString(TEXT("gunner")));
	TestTrue(TEXT("a fresh squad stands on its slots"), FVector2D(Squad.Men()[1].Local.X, Squad.Men()[1].Local.Y).Equals(Slots[1] + FVector2D(0, 0), 25.0));
	// suppressed to the ground: after the 0.3 s blend the prone keyframe carries the man
	F.Suppression = 95;
	for (int32 I = 0; I < 30; I++) Squad.Animate(F, 0.016f);
	TestTrue(TEXT("prone weight"), Squad.Men()[0].Blend.KeyWeights[2] > 0.99f);
	TestTrue(TEXT("prone lean"), FMath::IsNearlyEqual(Squad.Men()[0].Blend.Lean, -1.49f, 1e-3f));
	// beyond LOD + 4 m it goes far, and stays far until inside LOD - 4 m
	F.Eye = FVector(1000 + 11500, 0, 0);
	Squad.Animate(F, 0.016f);
	TestTrue(TEXT("far at 115 m"), Squad.IsFar());
	F.Eye = FVector(1000 + 10800, 0, 0);
	Squad.Animate(F, 0.016f);
	TestTrue(TEXT("still far at 108 m"), Squad.IsFar());
	F.Eye = FVector(1000 + 10500, 0, 0);
	Squad.Animate(F, 0.016f);
	TestFalse(TEXT("near again at 105 m"), Squad.IsFar());
	// off screen it is suspended; on return it snaps to its posture
	F.bDetailed = false;
	TestFalse(TEXT("suspended"), Squad.Animate(F, 0.016f));
	F.bDetailed = true; F.Suppression = 0; F.Flags = 1;
	Squad.Animate(F, 0.016f);
	TestTrue(TEXT("returned straight into the retreat posture"), Squad.Men()[0].Blend.KeyWeights[3] > 0.99f);
	// a man lost is hidden
	Squad.SetAlive(2);
	Squad.Animate(F, 0.016f);
	TestFalse(TEXT("the third man is down"), Squad.Men()[2].bVisible);
	// the MG crew carries no weapon of its own (the gun model has the muzzle): no u.muzzle, as in the reference
	FVector Muzzle;
	TestFalse(TEXT("the MG gunner has no muzzle of his own"), Squad.MuzzleWorld(0, FTransform(F.Root), Muzzle));
	// a rifleman's muzzle is ahead of him at about shoulder height (infantry.js HAND rifle [0.72, 1.1] times 1.1)
	FTCInfantrySquadAnimation Rifle;
	Rifle.Setup(TEXT("rifle"), 0, { FVector2D(0, 0) }, 1.1f);
	FTCInfantrySquadFrame R;
	R.Root = FVector(0, 0, 0); R.Eye = FVector(0, -2000, 1000);
	Rifle.Animate(R, 0.016f);
	TestTrue(TEXT("the rifleman has a muzzle"), Rifle.MuzzleWorld(0, FTransform::Identity, Muzzle));
	TestTrue(FString::Printf(TEXT("muzzle ahead at shoulder height (%s)"), *Muzzle.ToString()), Muzzle.X > 50.0 && Muzzle.Z > 100.0 && Muzzle.Z < 140.0);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMotionCrowdTest, "ThreeCrossroads.Motion.Crowd", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMotionCrowdTest::RunTest(const FString& Parameters)
{
	UTCSoldierCrowd* Crowd = NewObject<UTCSoldierCrowd>(GetTransientPackage());
	FString Error;
	const bool bLoaded = Crowd->Load(Error);
	TestTrue(FString::Printf(TEXT("crowd assets load (%s)"), *Error), bLoaded);
	TestTrue(TEXT("near kits"), Crowd->HasNear());
	TestTrue(TEXT("far figures"), Crowd->HasFar());
	if (!Crowd->HasNear() || !Crowd->HasFar()) return false;
	// the far figure's keyframe matrices reproduce the reference's morph targets (float32 in the shader)
	double WorstCm = 0, WorstRad = 0;
	int32 Figures = 0;
	TestTrue(TEXT("far keys checked"), Crowd->CheckFarKeys(WorstCm, WorstRad, Figures));
	AddInfo(FString::Printf(TEXT("far figures %d: worst position %.3g cm, worst normal %.3g rad"), Figures, WorstCm, WorstRad));
	TestTrue(FString::Printf(TEXT("far key positions within 0.001 cm (%.3g)"), WorstCm), WorstCm < 1e-3);
	TestTrue(FString::Printf(TEXT("far key normals within 0.001 rad (%.3g)"), WorstRad), WorstRad < 1e-3);
	auto Ground = [](double, double) { return 0.0; };
	FTCInfantryMotion Stand;
	const FTCInfantryBlend Blend = UTCInfantryRigLibrary::BlendFor(TEXT("rifle"), Stand);
	Crowd->BeginFrame();
	for (int32 K = 0; K < 6; K++)
	{
		FTCSoldierDraw D;
		D.FigureType = TEXT("rifle"); D.Faction = K % 4; D.Slot = K; D.Blend = Blend;
		D.Man = FTransform(FQuat::Identity, FVector(100.0 * K, 0, 0), FVector(1.1));
		D.bFar = K >= 3; D.Fade = K == 2 || K == 5 ? 0.45f : 1.f;
		D.UnitId = 77; D.ManIndex = K;
		TestTrue(FString::Printf(TEXT("man %d drawn"), K), Crowd->AddMan(D));
	}
	Crowd->SquadAlive(77, 6);
	Crowd->EndFrame(0.016f, FVector::ZeroVector, Ground);
	const FTCSoldierCrowdStats& S = Crowd->Stats();
	TestEqual(TEXT("near men"), S.NearMen, 3);
	TestEqual(TEXT("far men"), S.FarMen, 3);
	TestEqual(TEXT("camouflaged"), S.FadedMen, 2);
	TestEqual(TEXT("refused"), S.Refused, 0);
	TestEqual(TEXT("no bodies yet"), Crowd->NumCorpses(), 0);
	// two men fall: two bodies, near, solid at first
	Crowd->BeginFrame();
	Crowd->SquadAlive(77, 4);
	Crowd->EndFrame(0.016f, FVector::ZeroVector, Ground);
	TestEqual(TEXT("two bodies"), Crowd->NumCorpses(), 2);
	TestEqual(TEXT("bodies drawn near"), Crowd->Stats().NearCorpses, 2);
	// reinforced: no bodies; killed outright: the four left fall
	Crowd->SquadAlive(77, 6);
	Crowd->SquadGone(77, true);
	Crowd->BeginFrame();
	Crowd->EndFrame(0.016f, FVector(1e6, 0, 0), Ground);
	TestEqual(TEXT("six bodies"), Crowd->NumCorpses(), 6);
	TestEqual(TEXT("seen from 10 km they draw far"), Crowd->Stats().FarCorpses, 6);
	// a body lasts Life seconds, then fades for Fade seconds and is gone
	for (int32 I = 0; I < 26 * 10; I++) { Crowd->BeginFrame(); Crowd->EndFrame(0.1f, FVector::ZeroVector, Ground); }
	TestEqual(TEXT("fading, still there"), Crowd->NumCorpses(), 6);
	for (int32 I = 0; I < 20; I++) { Crowd->BeginFrame(); Crowd->EndFrame(0.1f, FVector::ZeroVector, Ground); }
	TestEqual(TEXT("all gone"), Crowd->NumCorpses(), 0);
	return true;
}
// The tracked hull copies (unreal/Tools/motion/make-wheel-tracks.py): every one loads, keeps the catalog hull's shape,
// carries 8 texture channels in its mesh and its render data, and its wheel and link vertices hold what the tool wrote
// (radius > 0 for wheels, a row of the shared table for links, the travelled link position finite and on its loop).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMotionWheelTracksTest, "ThreeCrossroads.Motion.WheelTracks", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMotionWheelTracksTest::RunTest(const FString& Parameters)
{
	FTCWheelTracks W;
	FString Why;
	if (!TestTrue(FString::Printf(TEXT("WheelTracks.json loads (%s)"), *Why), W.Load(Why))) return false;
	TestEqual(TEXT("tracked hulls"), W.NumHulls(), 21);
	TestTrue(TEXT("loops in the table"), W.Paths().Num() > 0 && W.Paths().Num() <= FTCTrackPaths::Rows);
	const FString Catalog = FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Models/ModelCatalog.json"));
	FString Text;
	FFileHelper::LoadFileToString(Text, *Catalog);
	TSharedPtr<FJsonObject> Root;
	FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root);
	int32 Checked = 0, Wheels = 0, Links = 0;
	if (Root.IsValid())
		for (const TSharedPtr<FJsonValue>& M : Root->GetArrayField(TEXT("models")))
			for (const TSharedPtr<FJsonValue>& P : M->AsObject()->GetArrayField(TEXT("parts")))
			{
				const FSoftObjectPath CatalogMesh(P->AsObject()->GetStringField(TEXT("mesh")));
				const FTCWheelTracks::FHull* H = W.Find(CatalogMesh);
				if (!H) continue;
				UStaticMesh* Copy = Cast<UStaticMesh>(H->Mesh.TryLoad());
				UStaticMesh* Original = Cast<UStaticMesh>(CatalogMesh.TryLoad());
				if (!TestTrue(FString::Printf(TEXT("%s loads"), *H->Mesh.ToString()), Copy && Original)) continue;
				TestEqual(FString::Printf(TEXT("%s: triangles as the catalog hull"), *H->Mesh.ToString()), Copy->GetNumTriangles(0), Original->GetNumTriangles(0));
				TestTrue(FString::Printf(TEXT("%s: same bounds"), *H->Mesh.ToString()), Copy->GetBoundingBox().Min.Equals(Original->GetBoundingBox().Min, 0.01) && Copy->GetBoundingBox().Max.Equals(Original->GetBoundingBox().Max, 0.01));
				if (Copy->GetRenderData() && Copy->GetRenderData()->LODResources.Num())
					TestEqual(FString::Printf(TEXT("%s: render data texture channels"), *H->Mesh.ToString()), (int32)Copy->GetRenderData()->LODResources[0].GetNumTexCoords(), 8);
#if WITH_EDITOR
				if (const FMeshDescription* D = Copy->GetMeshDescription(0))
				{
					FStaticMeshConstAttributes A(*D);
					const TVertexInstanceAttributesConstRef<FVector2f> UV = A.GetVertexInstanceUVs();
					if (!TestEqual(FString::Printf(TEXT("%s: mesh texture channels"), *H->Mesh.ToString()), UV.GetNumChannels(), 8)) continue;
					const TVertexAttributesConstRef<FVector3f> Pos = A.GetVertexPositions();
					int32 HW = 0, HL = 0;
					bool bRows = true, bOnLoop = true;
					for (const FVertexInstanceID I : D->VertexInstances().GetElementIDs())
					{
						if (UV.Get(I, 5).Y > 0) HW++;
						const FVector2f T0 = UV.Get(I, 6), T1 = UV.Get(I, 7);
						if (T0.Y < -0.5f) continue;
						HL++;
						const int32 Row = FMath::RoundToInt(T0.Y);
						bRows &= H->Loops.Contains(Row);
						const FVector3f P3 = Pos[D->GetVertexInstanceVertex(I)];
						const FVector Three(P3.X / 100.0, P3.Z / 100.0, P3.Y / 100.0);
						const FVector Moved = W.Paths().TrackVertex(Three, FVector4(T0.X, T0.Y, T1.X, T1.Y), 0.37, 0.25);
						bOnLoop &= !Moved.ContainsNaN() && FVector::Dist(Moved, Three) < 5.0 && FMath::IsNearlyEqual(Moved.Z, Three.Z, 1e-9);
					}
					TestTrue(FString::Printf(TEXT("%s: wheel corners"), *H->Mesh.ToString()), HW > 0);
					TestTrue(FString::Printf(TEXT("%s: link corners"), *H->Mesh.ToString()), HL > 0);
					TestTrue(FString::Printf(TEXT("%s: links name their hull's rows"), *H->Mesh.ToString()), bRows);
					TestTrue(FString::Printf(TEXT("%s: links stay on their side, near their loop"), *H->Mesh.ToString()), bOnLoop);
					Wheels += HW; Links += HL;
				}
#endif
				Checked++;
			}
	TestTrue(TEXT("every catalog hull with wheels checked"), Checked >= 21);
	AddInfo(FString::Printf(TEXT("%d catalog parts on %d hull copies, %d loops; %d wheel and %d link vertex instances"), Checked, W.NumHulls(), W.Paths().Num(), Wheels, Links));
	return true;
}

#endif // WITH_DEV_AUTOMATION_TESTS
#endif // TC_UNIT_MOTION_STANDALONE
