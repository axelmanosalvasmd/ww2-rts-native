// Standalone numeric probe of production noise/mask kernels and extracted HLSL.
// This is not an Unreal translation unit and produces no rendered evidence.
#if defined(TC_WORLD28_GROUND_STANDALONE)
#define TC_ATMOS_CORE_ONLY
#include "World/TCAtmosphere.h"
#include <fstream>
#include <iomanip>
#include <iostream>
#include <string>
#include <vector>

struct float3
{
	float x, y, z;
	float3(float X, float Y, float Z) : x(X), y(Y), z(Z) {}
};
static float dot(float3 A, float3 B) { return A.x * B.x + A.y * B.y + A.z * B.z; }
static float saturate(float V) { return std::max(0.f, std::min(1.f, V)); }
static float smoothstep(float A, float B, float X)
{
	const float T = saturate((X - A) / (B - A));
	return T * T * (3.f - 2.f * T);
}
// The driver writes this header from the actual material generator. No second
// handwritten copy of its snow expression or Low luminance conversion is used.
#define max(A, B) ((A) > (B) ? (A) : (B))
#define min(A, B) ((A) < (B) ? (A) : (B))
#define abs(A) std::fabs(A)
#include "TCWorld28GroundExpressions.inl"
#undef abs
#undef min
#undef max

static double Sample(const std::vector<uint8_t>& Bytes, int W, int H,
	int Channels, double U, double V, int Channel, bool Repeat)
{
	const double X = U * W - .5, Y = V * H - .5;
	const int IX = (int)std::floor(X), IY = (int)std::floor(Y);
	const double TX = X - IX, TY = Y - IY;
	auto Index = [Repeat](int I, int Size) { return Repeat ? ((I % Size) + Size) % Size : std::max(0, std::min(Size - 1, I)); };
	auto At = [&](int PX, int PY) { return Bytes[(Index(PY, H) * W + Index(PX, W)) * Channels + Channel] / 255.; };
	return (At(IX, IY) * (1 - TX) + At(IX + 1, IY) * TX) * (1 - TY)
		+ (At(IX, IY + 1) * (1 - TX) + At(IX + 1, IY + 1) * TX) * TY;
}

int main(int Argc, char** Argv)
{
	if (Argc != 6) { std::cerr << "input output noise mask alpha-input required\n"; return 2; }
	std::ifstream In(Argv[1]);
	int W = 0, H = 0, Count = 0;
	In >> W >> H;
	if (W <= 0 || H <= 0 || W > 4096 || H > 4096) return 3;
	std::vector<std::string> Rows(H);
	for (std::string& Row : Rows) { In >> Row; if ((int)Row.size() != W) return 4; }
	const auto Noise = TCAtmosCore::NoiseTexture();
	const auto Mask = TCAtmosCore::GroundMask(W, H, [&](int X, int Y) { return Rows[Y][X]; });
	std::ofstream(Argv[3], std::ios::binary).write(reinterpret_cast<const char*>(Noise.data()), Noise.size());
	std::ofstream(Argv[4], std::ios::binary).write(reinterpret_cast<const char*>(Mask.data()), Mask.size());
	std::ofstream Out(Argv[2]); Out << std::setprecision(17) << "{\"samples\":[";
	In >> Count;
	for (int I = 0; I < Count; I++)
	{
		double X, Z, NY, Cover;
		In >> X >> Z >> NY >> Cover;
		if (!In || !std::isfinite(X + Z + NY + Cover)) return 5;
		const double Small = Sample(Noise, 128, 128, 1, X / 9. + .37, Z / 9. + .37, 0, true);
		const double Water = Sample(Mask, W, H, 4, X / (W * 2.), Z / (H * 2.), 1, false);
		const double Road = Sample(Mask, W, H, 4, X / (W * 2.), Z / (H * 2.), 2, false);
		const float Snow = World28Snow((float)Cover, (float)(1 - Water), (float)NY, (float)Small, (float)Road) * .82f;
		if (I) Out << ',';
		Out << "{\"small\":" << Small << ",\"water\":" << Water << ",\"road\":" << Road << ",\"snowAlpha\":" << Snow << '}';
	}
	std::ifstream Alphas(Argv[5]); Alphas >> Count;
	Out << "],\"lowAlphas\":[";
	for (int I = 0; I < Count; I++)
	{
		float R, G, B, CR, CG, CB, A, Exposure, PreExposure, Linear;
		Alphas >> R >> G >> B >> CR >> CG >> CB >> A >> Exposure >> PreExposure >> Linear;
		if (!Alphas) return 6;
		if (I) Out << ',';
		Out << World28LowAlpha(float3(R, G, B), float3(CR, CG, CB), A, Exposure, PreExposure, Linear);
	}
	Out << "]}\n";
	return Out ? 0 : 7;
}
#endif
