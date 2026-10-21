// The atmosphere over the battlefield, ported from the pinned reference (TCAtmosCore::ReferencePin):
//   client/atmosphere.js  noiseTexture (tileable value noise), cloudMaterial (the cloud shade drifting on the wind),
//                         wispTexture, mistMaterial, mistSites, flowAngle (river mist on dawn maps), weatherPoints and
//                         FALLS (rain as slanted streaks, snow and dust as soft dots, in a box that follows the camera
//                         target, slanted by the wind they started in), groundMask, coverMaterial (wet ground, mud by the
//                         roads, puddles catching the sky, lying snow), bankSites (ground fog), birdMesh and the flight
//                         of two pairs, createAtmosphere (start, setWeather, setRain, update: easing toward the weather,
//                         fading one fall out and the next in);
//   client/wind.js        the wind (setWind: direction and strength from snapshot wx);
//   client/moods.js       MOODS and WEATHER_LOOK (through TCLook, Client/TCLighting.h, which applies the light);
//   shared/weather.js     roadMask (where mud and puddles gather).
// The light itself (sun, sky fill, haze, exposure) eases in ATCLighting with the same rules; this file draws what
// atmosphere.js adds to the scene, including its High/Low budgets and blend modes.
//
// The first part (TCAtmosCore) is plain C++ with no engine types, so a standalone harness can compare its numbers with
// the reference's own functions (unreal/Tools/atmosphere). Define TC_ATMOS_CORE_ONLY before including this header for that.
#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <functional>
#include <string>
#include <vector>

namespace TCAtmosCore
{
	inline constexpr const char* ReferencePin = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";
	inline constexpr double Pi = 3.14159265358979323846;
	inline constexpr double Tau = Pi * 2;
	// the wind before the server reports one (client/wind.js), normalised
	inline const double WL = std::hypot(0.55, 0.25), WX = 0.55 / WL, WZ = 0.25 / WL;
	inline constexpr double CloudScale = 260, CloudSpeed = 2.2; // metres per noise tile, metres per second
	inline constexpr int NoiseN = 128;
	inline constexpr int WispW = 128, WispH = 64;
	inline constexpr int MistCap = 36, BankCap = 64, Birds = 4;
	inline constexpr double Cell = 2.0;

	inline float F32(double V) { return (float)V; }

	// atmosphere.js seeded: (s = (s * 16807) % 2147483647) / 2147483647
	struct FSeeded
	{
		double S;
		explicit FSeeded(double Seed) : S(Seed) {}
		double operator()() { S = std::fmod(S * 16807.0, 2147483647.0); return S / 2147483647.0; }
	};

	inline double Smooth(double E0, double E1, double X) { double T = (X - E0) / (E1 - E0); T = T < 0 ? 0 : T > 1 ? 1 : T; return T * T * (3 - 2 * T); }

	// atmosphere.js noiseTexture: 128 px, four octaves (4, 8, 16, 32 cells; weights 0.5, 0.27, 0.15, 0.08), each octave's
	// lattice a Float32Array of the seeded(77) sequence. One byte per texel (the reference fills r, g, b and a with it).
	inline std::vector<uint8_t> NoiseTexture()
	{
		const int N = NoiseN;
		FSeeded Rnd(77);
		struct FOct { int G; double W; std::vector<float> V; };
		std::vector<FOct> Oct = { { 4, 0.5, {} }, { 8, 0.27, {} }, { 16, 0.15, {} }, { 32, 0.08, {} } };
		for (FOct& O : Oct) { O.V.resize(O.G * O.G); for (float& F : O.V) F = F32(Rnd()); }
		std::vector<uint8_t> Out(N * N);
		for (int Y = 0; Y < N; Y++)
			for (int X = 0; X < N; X++)
			{
				double Sum = 0;
				for (const FOct& O : Oct)
				{
					const int G = O.G;
					auto At = [&](int I, int J) { return (double)O.V[(J % G) * G + (I % G)]; };
					auto S = [](double T) { return T * T * (3 - 2 * T); };
					const double FX = ((double)X / N) * G, FY = ((double)Y / N) * G;
					const int X0 = (int)std::floor(FX), Y0 = (int)std::floor(FY);
					const double TX = S(FX - X0), TY = S(FY - Y0);
					Sum += ((At(X0, Y0) * (1 - TX) + At(X0 + 1, Y0) * TX) * (1 - TY) + (At(X0, Y0 + 1) * (1 - TX) + At(X0 + 1, Y0 + 1) * TX) * TY) * O.W;
				}
				const double R = std::floor(Sum * 255 + 0.5); // Math.round (Sum is never negative)
				Out[Y * N + X] = (uint8_t)(R < 0 ? 0 : R > 255 ? 255 : R);
			}
		return Out;
	}

	// atmosphere.js wispTexture: a 128 x 64 canvas with 26 soft white blobs (radial gradients from alpha 0.22 at the
	// centre to 0 at r, stretched 2.2 times across), drawn source-over. Alpha 0..1 per pixel, row 0 the canvas top,
	// sampled at pixel centres; colour white. (The canvas keeps 8 bits per draw; this keeps the exact sum.)
	inline std::vector<double> WispAlpha()
	{
		const int W = WispW, H = WispH;
		std::vector<double> A(W * H, 0.0);
		FSeeded Rnd(31);
		for (int I = 0; I < 26; I++)
		{
			const double X = W * (0.2 + Rnd() * 0.6), Y = H * (0.3 + Rnd() * 0.4), R = H * (0.16 + Rnd() * 0.18);
			for (int PY = 0; PY < H; PY++)
				for (int PX = 0; PX < W; PX++)
				{
					const double LX = (PX + 0.5 - X) / 2.2, LY = PY + 0.5 - Y;
					if (std::fabs(LX) > R || std::fabs(LY) > R) continue; // fillRect(-r, -r, 2r, 2r)
					const double T = std::sqrt(LX * LX + LY * LY) / R;
					if (T >= 1) continue;
					const double Src = 0.22 * (1 - T);
					double& D = A[PY * W + PX];
					D = Src + D * (1 - Src);
				}
		}
		return A;
	}

	// shared/weather.js roadMask: the map's roads (D) and bridges (=), and open ground (.) next to a house (B) unless it
	// also lies by water (W, F, =). Chars(c) is the cell's terrain character; cells outside the map read as none.
	inline std::vector<uint8_t> RoadMask(int W, int H, const std::function<char(int, int)>& Ch)
	{
		std::vector<uint8_t> Out(W * H, 0);
		for (int Y = 0; Y < H; Y++)
			for (int X = 0; X < W; X++)
			{
				const char C = Ch(X, Y);
				if (C == '=' || C == 'D') { Out[Y * W + X] = 1; continue; }
				if (C != '.') continue;
				bool Town = false, Wet = false;
				for (int DY = -1; DY <= 1; DY++)
					for (int DX = -1; DX <= 1; DX++)
					{
						const int XX = X + DX, YY = Y + DY;
						if (XX < 0 || YY < 0 || XX >= W || YY >= H) continue;
						const char N = Ch(XX, YY);
						if (N == 'B') Town = true; else if (N == 'W' || N == 'F' || N == '=') Wet = true;
					}
				if (Town && !Wet) Out[Y * W + X] = 1;
			}
		return Out;
	}

	// atmosphere.js groundMask: per cell r how close a road is (1 on it, fading over 3 cells), g water (nothing settles
	// there), b the road itself, a 255. RGBA bytes, row 0 the map's first row.
	inline std::vector<uint8_t> GroundMask(int W, int H, const std::function<char(int, int)>& Ch)
	{
		const std::vector<uint8_t> Road = RoadMask(W, H, Ch);
		std::vector<uint8_t> Out(W * H * 4);
		const int R = 3;
		for (int Y = 0; Y < H; Y++)
			for (int X = 0; X < W; X++)
			{
				double Near = 0;
				for (int DY = -R; DY <= R; DY++)
					for (int DX = -R; DX <= R; DX++)
					{
						const int XX = X + DX, YY = Y + DY;
						if (XX >= 0 && YY >= 0 && XX < W && YY < H && Road[YY * W + XX]) { const double V = 1 - std::hypot((double)DX, (double)DY) / (R + 0.5); if (V > Near) Near = V; }
					}
				const char C = Ch(X, Y);
				const int I = (Y * W + X) * 4;
				Out[I] = (uint8_t)std::floor(Near * 255 + 0.5);
				Out[I + 1] = C == 'W' || C == 'F' ? 255 : 0;
				Out[I + 2] = Road[Y * W + X] ? 255 : 0;
				Out[I + 3] = 255;
			}
		return Out;
	}

	// One instanced sheet of mist or ground fog: centre, height, yaw about the vertical (three's rotation.y), size, phase.
	struct FSheet { double X = 0, Z = 0, Y = 0, Yaw = 0, SX = 0, SZ = 0, Ph = 0; };

	// atmosphere.js flowAngle: the yaw that lays a sheet's long side along the river (the main axis of the water cells
	// within R of P)
	inline double FlowAngle(const std::vector<std::pair<double, double>>& Water, double PX, double PZ, double R)
	{
		int N = 0;
		double XX = 0, ZZ = 0, XZ = 0;
		for (const auto& Wt : Water)
		{
			const double DX = Wt.first - PX, DZ = Wt.second - PZ;
			if (DX * DX + DZ * DZ > R * R) continue;
			N++; XX += DX * DX; ZZ += DZ * DZ; XZ += DX * DZ;
		}
		return N < 3 ? 0 : -0.5 * std::atan2(2 * XZ, XX - ZZ);
	}

	// atmosphere.js mistSites: sheets over water cells, 14 m apart, kept 12 m off fords and bridges (where units cross);
	// low ground (below -0.5 m) if the map has no water
	inline std::vector<FSheet> MistSites(int W, int H, const std::function<char(int, int)>& Ch, double CellM, const std::function<double(double, double)>& Height, int Cap)
	{
		std::vector<std::pair<double, double>> Water, Cross, Low;
		FSeeded Rnd(4021);
		for (int Y = 0; Y < H; Y++)
			for (int X = 0; X < W; X++)
			{
				const double PX = (X + 0.5) * CellM, PZ = (Y + 0.5) * CellM;
				const char C = Ch(X, Y);
				if (C == 'W') Water.push_back({ PX, PZ });
				else if (C == 'F' || C == '=') Cross.push_back({ PX, PZ });
				else if (Height(PX, PZ) < -0.5) Low.push_back({ PX, PZ });
			}
		std::vector<std::pair<double, double>> Cand = Water.size() ? Water : Low;
		for (int I = (int)Cand.size() - 1; I > 0; I--) { const int J = (int)std::floor(Rnd() * (I + 1)); std::swap(Cand[I], Cand[J]); }
		std::vector<FSheet> Out;
		auto Near = [](double AX, double AZ, double BX, double BZ, double R) { return (AX - BX) * (AX - BX) + (AZ - BZ) * (AZ - BZ) < R * R; };
		for (const auto& P : Cand)
		{
			if ((int)Out.size() >= Cap) break;
			bool Skip = false;
			for (const FSheet& S : Out) if (Near(S.X, S.Z, P.first, P.second, 14)) { Skip = true; break; }
			if (!Skip) for (const auto& C : Cross) if (Near(C.first, C.second, P.first, P.second, 12)) { Skip = true; break; }
			if (Skip) continue;
			const double Base = Water.size() ? FlowAngle(Water, P.first, P.second, CellM * 4) : Rnd() * Pi;
			FSheet S;
			S.X = P.first; S.Z = P.second;
			S.Yaw = Base + (Rnd() - 0.5) * 0.3;
			S.Y = Height(P.first, P.second) + 0.35;
			S.SX = 15 + Rnd() * 8; S.SZ = 7 + Rnd() * 4; S.Ph = Rnd() * Tau;
			Out.push_back(S);
		}
		return Out;
	}

	// atmosphere.js bankSites: wide low sheets of ground fog scattered over the whole map
	inline std::vector<FSheet> BankSites(double MW, double MH, const std::function<double(double, double)>& Height, int Cap)
	{
		FSeeded Rnd(std::floor(MW * 3 + MH * 5 + 0.5) + 17);
		std::vector<FSheet> Out;
		for (int I = 0; I < Cap; I++)
		{
			FSheet S;
			S.X = Rnd() * MW; S.Z = Rnd() * MH;
			S.Y = Height(S.X, S.Z) + 0.8 + Rnd() * 1.8;
			S.Yaw = std::atan2(WZ, WX) + (Rnd() - 0.5) * 0.6;
			S.SX = 30 + Rnd() * 22; S.SZ = 13 + Rnd() * 8; S.Ph = Rnd() * Tau;
			Out.push_back(S);
		}
		return Out;
	}

	// atmosphere.js update: a mist sheet (Bank false) or a fog bank at time T (seconds): centre, yaw and the scale of its
	// unit plane (x, z), as the instance matrix composes them. Lift: the banks rise as they thin ((1 - banks) * 3).
	inline void SheetPose(const FSheet& S, double T, bool bBank, double Lift, double& X, double& Y, double& Z, double& Yaw, double& SX, double& SZ)
	{
		const double Wd = std::sin(T * (bBank ? 0.03 : 0.05) + S.Ph) * (bBank ? 6 : 3), Br = 1 + (bBank ? 0.1 : 0.08) * std::sin(T * (bBank ? 0.07 : 0.11) + S.Ph * 2);
		X = S.X + WX * Wd; Y = S.Y + (bBank ? Lift : 0); Z = S.Z + WZ * Wd;
		Yaw = bBank ? S.Yaw : S.Yaw + 0.05 * std::sin(T * 0.03 + S.Ph);
		SX = S.SX * Br; SZ = S.SZ * Br;
	}

	// atmosphere.js FALLS: n points, seed, alpha, sway (m), box (m), vel (m/s), size (m), pxMax, fade (m), colour, streak
	struct FFall
	{
		const char* Kind; int N; int Seed; double Alpha, Sway, Box[3], Vel[3], Size[2], PxMax, Fade[2]; uint32_t Color; int Streak;
	};
	inline const FFall& Fall(int I)
	{
		static const FFall Table[3] = {
			{ "rain", 3600, 3, 0.42, 0.1, { 150, 36, 150 }, { WX * 3, -21, WZ * 3 }, { 0.55, 1.0 }, 16, { 50, 90 }, 0xc4ccd4, 1 },
			{ "snow", 2600, 5, 0.9, 0.7, { 200, 60, 200 }, { WX * 1.4, -2.0, WZ * 1.4 }, { 0.16, 0.3 }, 5, { 55, 98 }, 0xf7f8fb, 0 },
			{ "dust", 800, 9, 0.13, 1.2, { 220, 9, 220 }, { WX * 7, 0.25, WZ * 7 }, { 1.4, 3.2 }, 30, { 50, 100 }, 0xcaa879, 0 },
		};
		return Table[I];
	}
	inline int FallIndex(const std::string& Kind) { return Kind == "rain" ? 0 : Kind == "snow" ? 1 : Kind == "dust" ? 2 : -1; }
	// atmosphere.js weatherPoints: position (n x 3) then aRand (n x 4), Float32Arrays from the one seeded(F.seed) stream
	inline void FallData(const FFall& F, std::vector<float>& Position, std::vector<float>& Rand)
	{
		FSeeded Rnd(F.Seed);
		Position.resize(F.N * 3); Rand.resize(F.N * 4);
		for (float& V : Position) V = F32(Rnd());
		for (float& V : Rand) V = F32(Rnd());
	}
	// atmosphere.js fallKind: the weather's rain or snow, a shower's rain, else the mood's dust on a clear day; "" none
	inline std::string FallKind(const std::string& WeatherNow, double ShowerRain, const std::string& MoodWeather)
	{
		if (WeatherNow == "rain" || WeatherNow == "snow") return WeatherNow;
		if (ShowerRain > 0.02) return "rain";
		if (MoodWeather == "dust" && WeatherNow == "clear") return "dust";
		return "";
	}
	// atmosphere.js update: fade the old fall out over three seconds before swapFall.
	// Keep the accumulator and division in JS number precision. A float accumulator
	// leaves a positive remainder after twelve 0.25-second steps and swaps a frame late.
	// True asks the caller to swap to the wanted kind (which resets Alpha to zero).
	inline bool AdvanceFall(double& Alpha, bool HasFall, bool Matches, bool WantsFall, double Dt)
	{
		if (HasFall && !Matches)
		{
			Alpha = std::max(0.0, Alpha - Dt / 3.0);
			return Alpha == 0.0;
		}
		if (!HasFall && WantsFall) { Alpha = 0.0; return true; }
		Alpha = std::min(1.0, Alpha + Dt / 3.0);
		return false;
	}
	// swapFall: the velocity a fall keeps (the wind it started in, at the table's horizontal speed)
	inline void FallVelocity(const FFall& F, double WindX, double WindZ, double Out[3])
	{
		const double H = std::hypot(F.Vel[0], F.Vel[2]) / WL;
		Out[0] = WindX * H; Out[1] = F.Vel[1]; Out[2] = WindZ * H;
	}
	// weatherPoints' vertex shader for one point (GLSL mod; float precision aside): its world position (reference
	// metres) and its height in the box (0..1)
	inline void FallPoint(const float* Pos, const float* Rand, double Time, const double Box[3], const double BoxMin[3], const double Vel[3], double Sway, double Out[3], double& HInBox)
	{
		double P[3];
		for (int K = 0; K < 3; K++) P[K] = Pos[K] * Box[K] + Vel[K] * Time * (0.75 + 0.5 * Rand[0]);
		P[0] += std::sin(Time * (0.5 + Rand[1]) + Rand[2] * 6.283) * Sway;
		P[2] += std::cos(Time * (0.4 + Rand[0]) + Rand[3] * 6.283) * Sway;
		for (int K = 0; K < 3; K++) { const double L = P[K] - BoxMin[K]; const double M = L - Box[K] * std::floor(L / Box[K]); Out[K] = BoxMin[K] + M; if (K == 1) HInBox = M / Box[1]; }
	}

	// birdMesh: a thin body and two wings in two panels each (21 vertices, 7 triangles, three's axes)
	inline const double* BirdVertices()
	{
		static const double V[63] = {
			0, 0, 0.55, -0.09, 0, 0.05, 0, 0, -0.42, 0, 0, 0.55, 0, 0, -0.42, 0.09, 0, 0.05,
			0, 0, -0.38, -0.16, 0, -0.62, 0.16, 0, -0.62,
			-0.07, 0, 0.16, -0.07, 0, -0.14, -0.42, 0, 0.1, -0.42, 0, 0.1, -0.07, 0, -0.14, -0.8, 0, -0.16,
			0.07, 0, 0.16, 0.42, 0, 0.1, 0.07, 0, -0.14, 0.42, 0, 0.1, 0.8, 0, -0.16, 0.07, 0, -0.14,
		};
		return V;
	}
	// birdMesh's aPhase: Float32Array.from({ length: n }, seeded(123))
	inline std::vector<float> BirdPhases(int N = Birds) { FSeeded Rnd(123); std::vector<float> P(N); for (float& V : P) V = F32(Rnd()); return P; }
	// the vertex shader's flap: a vertex lifted by |x|^1.2, gliding now and then
	inline double FlapLift(double VX, double Time, double Phase)
	{
		const double Glide = Smooth(0.3, 0.7, 0.5 + 0.5 * std::sin(Time * 0.31 + Phase * 17.0));
		return std::pow(std::fabs(VX), 1.2) * std::sin(Time * 6.5 + Phase * 6.283) * (0.95 + (0.1 - 0.95) * Glide);
	}

	// start(): two pairs circling over different parts of the map, 10 to 16 m above the local ground
	struct FFlight { double CX = 0, CZ = 0, R = 0, W = 0, A0 = 0, Y = 0, Bob = 0; };
	inline std::vector<FFlight> Flights(double MW, double MH)
	{
		FSeeded Rnd(std::floor(MW * 7 + MH * 13 + 0.5));
		std::vector<FFlight> Out;
		for (int I = 0; I < Birds; I++)
		{
			const double C0 = I < 2 ? 0.36 : 0.68, C1 = I < 2 ? 0.6 : 0.32;
			FFlight F;
			F.R = 20 + Rnd() * 18;
			F.CX = MW * C0; F.CZ = MH * C1; F.W = (I < 2 ? 1 : -1) * 5.5 / F.R;
			F.A0 = Rnd() * Tau; F.Y = 10 + Rnd() * 6; F.Bob = Rnd() * Tau;
			Out.push_back(F);
		}
		return Out;
	}
	// update(): a bird's place (x, z; the height above the ground at it in Up) and its Euler (0, yaw, roll, 'YXZ')
	inline void BirdPose(const FFlight& B, double T, double& X, double& Z, double& Up, double& Yaw, double& Roll)
	{
		const double A = B.A0 + B.W * T, Dir = B.W > 0 ? 1 : B.W < 0 ? -1 : 0;
		X = B.CX + std::cos(A) * B.R; Z = B.CZ + std::sin(A) * B.R;
		Up = B.Y + std::sin(T * 0.4 + B.Bob) * 0.8;
		Yaw = std::atan2(-std::sin(A) * Dir, std::cos(A) * Dir); Roll = 0.3 * Dir;
	}

	// client/wind.js: { x, z } until the first snapshot, then setWind(a, v)
	struct FWind
	{
		double X = 0.55, Z = 0.25;
		void Set(double A, double V) { const double K = 0.25 + 0.6 * V; X = std::cos(A) * K; Z = std::sin(A) * K; }
	};

	// update(): the cloud shade's drift, summed frame by frame (JavaScript's % keeps the sign)
	inline void Drift(double& DX, double& DY, double WindX, double WindZ, double Dt)
	{
		const double K = (CloudSpeed * Dt) / CloudScale / WL;
		DX = std::fmod(DX - WindX * K, 1.0); DY = std::fmod(DY - WindZ * K, 1.0);
	}

	// Observation only. Snapshot ticks and the optional held particle clock do not
	// advance the weather's easing. A comparison needs the dt actually consumed by
	// every update since start, including the frames after its last goal change.
	struct FRenderHistory
	{
		uint64_t Starts = 0, Frames = 0;
		double Seconds = 0, GoalSeconds = 0;
		void Start() { Starts++; Frames = 0; Seconds = GoalSeconds = 0; }
		void GoalChanged() { GoalSeconds = 0; }
		void Frame(double Dt) { Frames++; Seconds += Dt; GoalSeconds += Dt; }
	};
}

#ifndef TC_ATMOS_CORE_ONLY

#include "CoreMinimal.h"
#include "UObject/StrongObjectPtr.h"
#include "Client/TCLighting.h"
#include "Client/TCGraphics.h"
#include "World/TCRailways.h"

class FTCMatchState;
class ATCTerrain;
class APlayerController;
class AActor;
class UWorld;
class UProceduralMeshComponent;
class UMaterialInstanceDynamic;
class UMaterialInterface;
class UTexture2D;

namespace TCAtmos
{
	// the materials unreal/Tools/atmosphere/make-atmosphere-assets.py makes
	constexpr const TCHAR* GroundPath = TEXT("/Game/Atmosphere/Materials/M_TCAtmosGround.M_TCAtmosGround");
	constexpr const TCHAR* MistPath = TEXT("/Game/Atmosphere/Materials/M_TCAtmosMist.M_TCAtmosMist");
	constexpr const TCHAR* FallPath = TEXT("/Game/Atmosphere/Materials/M_TCAtmosFall.M_TCAtmosFall");
	constexpr const TCHAR* BirdPath = TEXT("/Game/Atmosphere/Materials/M_TCAtmosBird.M_TCAtmosBird");
	// the reference's transparent render orders (cover 0.58, clouds 0.6, mist 0.7, banks 0.72, falls 0.8, birds 0.9)
	// Ground cover shares the fog overlay's before-DOF pass and must sort below
	// its priority 1. Equal priorities let component bounds reorder the layers.
	constexpr int32 SortGround = 0, SortMist = 2, SortBanks = 3, SortFall = 4, SortBirds = 5;
}

// What atmosphere.js draws. The game mode holds one: Attach once (it spawns an actor of its own for its meshes),
// StartMatch at every start (atmosphere.js start), Update every frame of a match (update, with setWeather, setRain and
// setWind from the match state), StopMatch when the match is cleared, Detach at the end.
class THREECROSSROADS_API FTCAtmosphere
{
public:
	bool Attach(UWorld* World);
	void Detach();

	// atmosphere.js start: the mood (TCLook::MoodFor for the map, its key and the weather), the weather with no easing
	// in, the ground mask, the mist and fog bank sites, the birds' flights and the fall in the air. MapKey: the lobby's
	// map name (main.js passes lobbyState.mapName). Mood: '?mood=' in the browser (NAME_None: none).
	void StartMatch(const FTCMatchState& State, const FString& MapKey, const ATCTerrain* Terrain, FName MoodOverride = NAME_None, const FString& AuthoredMood = FString());
	// More ground for the shade and the wet ground to lie on beyond the terrain's own meshes (the apron past the map edge,
	// cloudApron and coverApron): every procedural mesh section of the actor is mirrored.
	void AddGroundActor(AActor* Ground);
	// Round 14: light.js followView's fog distances (ATCLighting FogNear, FogFar, metres) for the ground layer's FOG_FADE
	// (the cover and cloud shade fade with the haze, as atmosphere.js coverMaterial and cloudMaterial do).
	void SetHazeDistances(float Near, float Far);
	// main.js renderFrame: update(dt) after the weather row (setWeather), snapshot wx (setWind, setRain) and the camera
	// target the falls follow (main.js cam: Focus in reference metres).
	void Update(float Dt, const FTCMatchState& State, const ATCTerrain* Terrain, APlayerController* PC, FVector2D Focus);
	// the match ended or the lobby came back: nothing is drawn until the next start
	void StopMatch();
	// the railways the shot switch can look at (-TCAtmosShotFocus=station|track)
	void SetRailways(const FTCRailways* InRailways) { Railways = InRailways; }

	// diagnostics and checks
	bool IsOn() const { return bOn; }
	FName Mood() const { return MoodRow.Name; }
	const FString& WeatherNow() const { return WxNow; }
	const FTCWeatherLook& Look() const { return Cur; }
	FString FallKind() const { return FallNow; }
	float FallFade() const { return FallA; }
	// round 15: the sheets made for the level now (atmosphere.js applyGfx mist.count, banks.count)
	int32 MistSheets() const { return MistOn ? MistBuilt : 0; }
	int32 BankSheets() const { return BanksOn ? BanksBuilt : 0; }
	int32 FallPointsDrawn() const { return FallNow.IsEmpty() ? 0 : FallDrawn; }
	int32 GraphicsSwitches() const { return LevelSwitches; }
	bool CloudsOn() const { return bCloudsShown; }
	bool CoverOn() const { return bCoverShown; }
	int32 GroundSectionsMirrored() const { return MirroredSections; }
	FVector2D Wind() const { return FVector2D(WindNow.X, WindNow.Z); }
	// every material of make-atmosphere-assets.py loaded (the fall's once a fall was drawn)
	bool MaterialsLoaded() const { return GroundMat.IsValid() && MistMat.IsValid() && BankMat.IsValid() && BirdMat.IsValid() && (FallNow.IsEmpty() || FallMat.IsValid()); }
	int32 FallPoints() const;
	FString Describe() const;
	const TCAtmosCore::FRenderHistory& RenderHistory() const { return History; }

private:
#if WITH_DEV_AUTOMATION_TESTS
	friend class FTCAtmosphereOracleTest;
	friend class FTCWorld26WeatherHistoryTest;
	friend class FTCWorld27GroundLayerTest;
	friend class FTCWorld27GroundPixelsTest;
#endif
	void Blend();
	void ApplyMood();
	void ApplyBlendMode();
	void SwapFall(const FString& Kind);
	void BuildFall();
	void BuildSheets();
	void SyncGround(const ATCTerrain* Terrain, bool bForce);
	void UpdateSheets(double T);
	void UpdateBirds(double T, const ATCTerrain* Terrain);
	void UpdateFall(const ATCTerrain* Terrain, APlayerController* PC, FVector2D Focus);
	void RunChecks(const FTCMatchState& State, APlayerController* PC);
	void TraceHistory(const TCHAR* Event, double Dt, const FTCMatchState& State, bool bFixedClock) const;
	void TraceView(const APlayerController* PC, FVector2D Focus) const;
	UMaterialInstanceDynamic* Mid(const TCHAR* Path, const TCHAR* Name);
	UProceduralMeshComponent* NewMesh(const TCHAR* Name, int32 Sort, bool bVisible);

	bool bOn = false;
	FTCMood MoodRow;
	FString WxNow = TEXT("clear");
	FTCWeatherLook Cur, Goal;
	FLinearColor HazeNow = FLinearColor::White, HazeGoal = FLinearColor::White;
	float ShowerRain = 0.f, ShowerWet = 0.f;
	TCAtmosCore::FWind WindNow;
	double T = 0.0, DriftX = 0.0, DriftY = 0.0;
	double MW = 0.0, MH = 0.0;
	std::vector<TCAtmosCore::FSheet> Sites, BankAt;
	std::vector<TCAtmosCore::FFlight> Flight;
	std::vector<float> Phases;
	bool MistOn = false, BanksOn = false, bCloudsShown = false, bCoverShown = false;
	// Graphics (round 15): the level the sheets and the fall were made for, their counts, the change event
	FTCGraphicsWatch GfxWatch;
	bool GfxLow = false;
	int32 MistBuilt = 0, BanksBuilt = 0, FallDrawn = 0, LevelSwitches = 0;
	FString FallNow;      // the fall drawn now ('' none)
	double FallA = 0.0;   // how far it has faded in (atmosphere.js number precision)
	double FallVel[3] = { 0, 0, 0 };
	FTCCellFogMap Fog;
	bool bFogParamsStale = true; // a material made after the fog texture still needs it
	int32 MirroredSections = 0, LastTerrainChunks = -1;
	double GroundSyncAt = -1.0;
	TArray<TWeakObjectPtr<AActor>> ExtraGround;
	FString LoggedState;
	TCAtmosCore::FRenderHistory History;
	bool bTraceHistory = false; // -TCAtmosHistory: one JSON log row per consumed frame, disabled by default

	TWeakObjectPtr<AActor> Actor;
	const FTCRailways* Railways = nullptr;
	TStrongObjectPtr<UTexture2D> NoiseTex, WispTex, MaskTex;
	TStrongObjectPtr<UMaterialInstanceDynamic> GroundMat, MistMat, BankMat, FallMat, BirdMat;
	// one overlay per mirrored ground mesh (the terrain's own, an apron's), keyed by the source component
	TMap<TWeakObjectPtr<UProceduralMeshComponent>, TWeakObjectPtr<UProceduralMeshComponent>> Overlays;
	TWeakObjectPtr<UProceduralMeshComponent> Sheets;   // section 0 mist, 1 fog banks
	TWeakObjectPtr<UProceduralMeshComponent> FallMesh;
	TWeakObjectPtr<UProceduralMeshComponent> BirdMesh;

	// verification switches (-TCAtmosShotAfter=S[,S2] -TCAtmosShotFile=P [-TCAtmosShotFocus=x,z|spawn|station|track]
	// [-TCAtmosShotDist=m]), read once
	bool bChecksRead = false;
	double ShotDist = -1.0, MatchSeen = -1.0;
	TArray<double> ShotTimes;
	FString ShotFile, ShotFocusKind;
	FVector2D ShotFocus = FVector2D(-1, -1);
	int32 ShotStep = 0;
	double ShotStepAt = 0.0;
};

#endif
