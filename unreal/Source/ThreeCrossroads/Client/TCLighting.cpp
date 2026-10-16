#include "Client/TCLighting.h"
#include "Client/TCGraphics.h"
#include "Misc/App.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "World/TCTerrain.h"
#include "UObject/UObjectIterator.h"
#include "MeshDescription.h"
#include "StaticMeshAttributes.h"
#include "World/TCWorldObjects.h"
#include "Misc/Compression.h"
#include "HAL/FileManager.h"
#include "Net/TCMatchState.h"
#include "ThreeCrossroads.h"
#include "Components/DirectionalLightComponent.h"
#include "Components/ExponentialHeightFogComponent.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Components/PostProcessComponent.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Components/SkyAtmosphereComponent.h"
#include "Components/SkyLightComponent.h"
#include "Components/StaticMeshComponent.h"
#include "Camera/PlayerCameraManager.h"
#include "Engine/Engine.h"
#include "HAL/IConsoleManager.h"
#include "Engine/GameViewportClient.h"
#include "UnrealClient.h"
#include "Misc/CommandLine.h"
#include "Engine/StaticMesh.h"
#include "StaticMeshResources.h"
#include "Engine/TextureCube.h"
#include "Engine/TextureRenderTarget2D.h"
#include "Engine/World.h"
#include "GameFramework/PlayerController.h"
#include "GameFramework/WorldSettings.h"
#include "ImageUtils.h"
#include "Internationalization/Regex.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/MaterialParameterCollection.h"
#include "Materials/MaterialParameterCollectionInstance.h"
#include "Materials/MaterialRenderProxy.h"
#include "MaterialShared.h"
#include "Misc/FileHelper.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "TextureResource.h"
#include "RenderingThread.h"
#include "Math/SHMath.h"
#if WITH_EDITOR
#include "AssetCompilingManager.h"
#include "ShaderCompiler.h"
#endif

DEFINE_LOG_CATEGORY_STATIC(LogTCLook, Log, All);

// Independent controls for matched-frame diagnosis. Defaults preserve the pinned light.js values.
// Change one at a time with -ExecCmds="tc.Look.Haze 0", for example. These never affect match state.
static TAutoConsoleVariable<float> CVarLookHaze(TEXT("tc.Look.Haze"), 1.f, TEXT("Reference haze density multiplier; 0 disables distance haze."));
static TAutoConsoleVariable<float> CVarLookExposure(TEXT("tc.Look.Exposure"), 1.f, TEXT("Reference tone-mapping exposure multiplier."));
static TAutoConsoleVariable<int32> CVarLookNeutral(TEXT("tc.Look.Neutral"), 0, TEXT("Diagnostic: use the reference lobby's Neutral curve instead of the match's ACES curve."));
static TAutoConsoleVariable<float> CVarLookIndirect(TEXT("tc.Look.Indirect"), 1.f, TEXT("Hemisphere and environment intensity multiplier."));
static TAutoConsoleVariable<float> CVarLookShadow(TEXT("tc.Look.Shadow"), 1.f, TEXT("Reference sun shadow strength multiplier."));
static TAutoConsoleVariable<int32> CVarLookShadowView(TEXT("tc.Look.ShadowView"), 1, TEXT("Follow the reference shadow footprint; 0 retains the old fixed-distance cascades for comparison."));
static TAutoConsoleVariable<int32> CVarLookShadowFilter(TEXT("tc.Look.ShadowFilter"), 1, TEXT("Map the reference PCF radius to the native filter footprint; 0 retains the old 3x3 filter for comparison."));

// ------------------------------------------------------------------------------------------------- color and moods

namespace TCLook
{
	static float SrgbToLinear(float C) { return C < 0.04045f ? C * 0.0773993808f : FMath::Pow(C * 0.9478672986f + 0.0521327014f, 2.4f); }
	static float LinearToSrgb(float C) { return C < 0.0031308f ? C * 12.92f : 1.055f * FMath::Pow(C, 0.41666f) - 0.055f; }

	FLinearColor HexLinear(uint32 Hex)
	{
		return FLinearColor(SrgbToLinear(((Hex >> 16) & 255) / 255.f), SrgbToLinear(((Hex >> 8) & 255) / 255.f), SrgbToLinear((Hex & 255) / 255.f), 1.f);
	}

	uint32 LinearHex(const FLinearColor& C)
	{
		// three Color.getHex: to sRGB, then round(clamp(c * 255))
		auto B = [](float V) { return (uint32)FMath::Clamp(FMath::RoundToInt(LinearToSrgb(V) * 255.f), 0, 255); };
		return (B(C.R) << 16) | (B(C.G) << 8) | B(C.B);
	}

	FLinearColor ToDisplay(const FLinearColor& L)
	{
		// sRGBTransferOETF: lessThanEqual( value, 0.0031308 ) picks the linear segment
		auto O = [](float V) { return V <= 0.0031308f ? V * 12.92f : FMath::Pow(V, 0.41666f) * 1.055f - 0.055f; };
		return FLinearColor(O(L.R), O(L.G), O(L.B), L.A);
	}

	FLinearColor ACESFilmic(const FLinearColor& In, float Exposure)
	{
		const double K = Exposure / 0.6;
		const double R = In.R * K, G = In.G * K, B = In.B * K;
		// ACESInputMat (three's columns, transposed from the source)
		double X = 0.59719 * R + 0.35458 * G + 0.04823 * B;
		double Y = 0.07600 * R + 0.90834 * G + 0.01566 * B;
		double Z = 0.02840 * R + 0.13383 * G + 0.83777 * B;
		auto Fit = [](double V) { const double A = V * (V + 0.0245786) - 0.000090537, D = V * (0.983729 * V + 0.4329510) + 0.238081; return A / D; };
		X = Fit(X); Y = Fit(Y); Z = Fit(Z);
		const double OR = 1.60475 * X - 0.53108 * Y - 0.07367 * Z;
		const double OG = -0.10208 * X + 1.10813 * Y - 0.00605 * Z;
		const double OB = -0.00327 * X - 0.07276 * Y + 1.07602 * Z;
		return FLinearColor((float)FMath::Clamp(OR, 0.0, 1.0), (float)FMath::Clamp(OG, 0.0, 1.0), (float)FMath::Clamp(OB, 0.0, 1.0), In.A);
	}

	FLinearColor ToneMapped(const FLinearColor& Linear, float Exposure) { return ToDisplay(ACESFilmic(Linear, Exposure)); }

	FLinearColor BillboardColour(const FLinearColor& Linear, float Depth, float Near, float Far,
		const FLinearColor& Haze, float Exposure, bool bBeforeTone)
	{
		const float T = Far > Near ? FMath::Clamp((Depth - Near) / (Far - Near), 0.f, 1.f) : 0.f;
		const float Fog = T * T * (3.f - 2.f * T);
		FLinearColor C;
		if (bBeforeTone) C = ACESFilmic(FMath::Lerp(Linear, Haze, Fog), Exposure);
		else
		{
			// Direct rendering applies fog after tone mapping and the sRGB output transfer.
			C = FMath::Lerp(ToneMapped(Linear, Exposure), ToDisplay(Haze), Fog);
			C = FLinearColor(SrgbToLinear(C.R), SrgbToLinear(C.G), SrgbToLinear(C.B), C.A);
		}
		C.A = Linear.A;
		return C;
	}

	FShadowView ShadowView(const FVector& Eye, const FVector& Forward, const FVector& Right, const FVector& Up,
		double TanX, double TanY, double Ground, int32 MapSize, double FilterRadius, bool bSoft)
	{
		FVector Foot[4], Centre = FVector::ZeroVector;
		FShadowView V;
		for (int32 I = 0; I < 4; I++)
		{
			const FVector Dir = (Forward + Right * ((I & 1 ? 1 : -1) * TanX) + Up * ((I & 2 ? 1 : -1) * TanY)).GetSafeNormal();
			const double T = Dir.Z < -0.02 ? FMath::Min(600.0, (Ground - Eye.Z) / Dir.Z) : 600.0;
			Foot[I] = Eye + Dir * T;
			Centre += Foot[I];
			V.FarDepth = FMath::Max(V.FarDepth, FVector::DotProduct(Foot[I] - Eye, Forward));
		}
		Centre *= 0.25;
		for (const FVector& P : Foot) V.Radius = FMath::Max(V.Radius, FVector::Distance(P, Centre));
		V.Radius = FMath::CeilToDouble((V.Radius + 10.0) / 8.0) * 8.0;
		V.Texel = 2.0 * V.Radius / MapSize;
		// Native CSM bounds a view subfrustum with a sphere. Keep all four ground corners inside it and map its
		// world-space texel/filter width to the reference square. The native 5x5 PCF approximates three 0.186.1's
		// five rotated Vogel samples, each using hardware bilinear PCF. It is not the same sampling kernel.
		V.FarDepth += 10.0;
		const double Near = 0.1, Far = V.FarDepth, D = FMath::Square(TanX) + FMath::Square(TanY);
		const double C = FMath::Clamp(Far - ((Near * Near - Far * Far) * D / (2.0 * (Far - Near)) + (Far - Near) * 0.5), Near, Far);
		V.CascadeRadius = FMath::Sqrt(FMath::Max(Near * Near * D + FMath::Square(C - Near), Far * Far * D + FMath::Square(Far - C)));
		const double FilterScale = bSoft ? 2.0 / FilterRadius : 1.0;
		V.Resolution = FMath::Clamp(FMath::RoundToInt(MapSize * V.CascadeRadius / V.Radius * FilterScale), 32, 8192);
		return V;
	}

	static FTCMood Row(const TCHAR* Name, float SunUp, uint32 Sun, float SunI, uint32 Top, uint32 Bottom, float HemiI, uint32 Haze, float HazeK, float Shadow, float Soft, float Exposure, float Clouds, bool bMist = false, const TCHAR* Weather = nullptr)
	{
		FTCMood M;
		M.Name = Name; M.SunUp = SunUp;
		M.SunHex = Sun; M.TopHex = Top; M.BottomHex = Bottom; M.HazeHex = Haze;
		M.Sun = HexLinear(Sun); M.Top = HexLinear(Top); M.Bottom = HexLinear(Bottom); M.Haze = HexLinear(Haze);
		M.SunI = SunI; M.HemiI = HemiI; M.HazeK = HazeK; M.Shadow = Shadow; M.Soft = Soft; M.Exposure = Exposure; M.Clouds = Clouds;
		M.bMist = bMist; M.Weather = Weather ? FName(Weather) : NAME_None;
		return M;
	}

	const TArray<FTCMood>& Moods()
	{
		// client/moods.js MOODS at the pin
		static const TArray<FTCMood> Table = {
			Row(TEXT("day"), 42, 0xffe7c9, 3.2f, 0xc3cddd, 0x8a806b, 1.25f, 0xb9bdbc, 1.f, 0.95f, 1.6f, 0.9f, 0.085f),
			Row(TEXT("golden"), 22, 0xffb06a, 3.9f, 0xb4b6c4, 0x7c6a54, 1.1f, 0xcdb08f, 1.15f, 0.95f, 1.4f, 0.95f, 0.08f),
			Row(TEXT("dawn"), 21, 0xffd2ae, 3.3f, 0xc4cddb, 0x8d8170, 1.2f, 0xcdc9c1, 1.35f, 0.9f, 1.9f, 1.0f, 0.06f, true),
			Row(TEXT("overcast"), 52, 0xe9e6de, 1.9f, 0xc3cad1, 0x89857b, 1.5f, 0xa8aba6, 1.25f, 0.6f, 2.8f, 1.0f, 0.08f),
			Row(TEXT("snow"), 30, 0xeef0f5, 2.2f, 0xd2dae4, 0x9e9a8e, 1.45f, 0xb9bfc5, 1.4f, 0.65f, 2.4f, 0.95f, 0.06f, false, TEXT("snow")),
			Row(TEXT("dust"), 48, 0xffe6bd, 3.5f, 0xd5cdbd, 0x9a886c, 1.25f, 0xcdb895, 1.3f, 0.95f, 1.4f, 0.95f, 0.085f, false, TEXT("dust")),
		};
		return Table;
	}

	const FTCMood* FindMood(FName Name)
	{
		for (const FTCMood& M : Moods()) if (M.Name == Name) return &M;
		return nullptr;
	}

	const FTCMood& DefaultMood() { return Moods()[0]; }

	FName MapMood(const FString& MapName, const FString& MapKey)
	{
		// shared/weather.js MOOD_RULES: name/key inference, after MoodFor checks the authored map mood.
		static const TCHAR* Rules[][2] = {
			{ TEXT("snow"), TEXT("ardennes|bastogne|bulge|winter|snow") },
			{ TEXT("dust"), TEXT("kasserine|desert|tobruk|alamein|africa|tunis") },
			{ TEXT("dawn"), TEXT("pegasus|polder|river|canal|marsh") },
			{ TEXT("overcast"), TEXT("bocage|cassino|stalingrad|seawall|rain|storm") },
		};
		const FString Text = (MapName + TEXT(" ") + MapKey).ToLower();
		for (const auto& R : Rules)
		{
			FRegexMatcher M(FRegexPattern(R[1]), Text);
			if (M.FindNext()) return FName(R[0]);
		}
		return FName(TEXT("warm"));
	}

	FName MoodFor(const FString& MapName, const FString& MapKey, const FString& WeatherKind, FName Override, const FString& AuthoredMood)
	{
		if (!Override.IsNone() && FindMood(Override)) return Override;
		if (WeatherKind == TEXT("snow")) return FName(TEXT("snow"));
		if (WeatherKind == TEXT("rain") || WeatherKind == TEXT("mud")) return FName(TEXT("overcast"));
		if (AuthoredMood.Equals(AuthoredMood.ToLower(), ESearchCase::CaseSensitive))
			if (const FTCMood* Authored = FindMood(FName(*AuthoredMood))) return Authored->Name;
		const FName Name = MapMood(MapName, MapKey);
		return FindMood(Name) ? Name : DefaultMood().Name;
	}

	FTCWeatherLook WeatherLook(const FString& Kind)
	{
		auto L = [](float Haze, uint32 HazeTo, float HazeMix, float Sun, float Shadow, float Clouds, float Wet, float Sheen, float Mud, float Cover, float Banks)
		{
			FTCWeatherLook W;
			W.Haze = Haze; W.HazeTo = HexLinear(HazeTo); W.HazeMix = HazeMix; W.Sun = Sun; W.Shadow = Shadow; W.Clouds = Clouds;
			W.Wet = Wet; W.Sheen = Sheen; W.Mud = Mud; W.Cover = Cover; W.Banks = Banks;
			return W;
		};
		// client/moods.js WEATHER_LOOK at the pin
		if (Kind == TEXT("fog")) return L(2.4f, 0xc9cbc6, 0.75f, 0.62f, 0.45f, 0.2f, 0.25f, 0.f, 0.f, 0.f, 1.f);
		if (Kind == TEXT("rain")) return L(1.6f, 0x8e959a, 0.55f, 0.6f, 0.5f, 2.6f, 1.f, 1.f, 0.35f, 0.f, 0.f);
		if (Kind == TEXT("mud")) return L(1.15f, 0xa29c90, 0.3f, 0.85f, 0.8f, 1.6f, 0.55f, 0.55f, 1.f, 0.f, 0.f);
		if (Kind == TEXT("snow")) return L(1.25f, 0xdfe3e8, 0.35f, 0.95f, 0.9f, 1.f, 0.f, 0.f, 0.f, 1.f, 0.f);
		return L(1.f, 0xffffff, 0.f, 1.f, 1.f, 1.f, 0.f, 0.f, 0.f, 0.f, 0.f);
	}

	void WeatherGoal(const FTCMood& M, const FString& Kind, float RainK, float WetK, FTCWeatherLook& Goal, FLinearColor& Haze)
	{
		const FTCWeatherLook Base = WeatherLook(Kind), R = WeatherLook(TEXT("rain"));
		const float K = RainK, W = Kind == TEXT("mud") ? 0.f : WetK; // Mud's look is its soaked ground
		Goal = Base;
		auto Mix = [K](float A, float B) { return A + (B - A) * K; };
		Goal.Haze = Mix(Base.Haze, R.Haze); Goal.Sun = Mix(Base.Sun, R.Sun); Goal.Shadow = Mix(Base.Shadow, R.Shadow);
		Goal.Clouds = Mix(Base.Clouds, R.Clouds); Goal.Banks = Mix(Base.Banks, R.Banks);
		Goal.Wet = FMath::Max(Base.Wet, R.Wet * W); Goal.Sheen = FMath::Max(Base.Sheen, R.Sheen * W); Goal.Mud = FMath::Max(Base.Mud, R.Mud * W);
		const FLinearColor HazeBase = FMath::Lerp(M.Haze, Base.HazeTo, Base.HazeMix);
		Haze = FMath::Lerp(FMath::Lerp(M.Haze, R.HazeTo, R.HazeMix), HazeBase, 1.f - K);
	}

	FTCMood Applied(const FTCMood& M, const FTCWeatherLook& Cur, const FLinearColor& HazeNow)
	{
		FTCMood A = M;
		A.SunI = M.SunI * Cur.Sun;
		A.Shadow = M.Shadow * Cur.Shadow;
		A.HazeK = M.HazeK * Cur.Haze;
		A.HazeHex = LinearHex(HazeNow); // { haze: hazeNow.getHex() }, then setHex
		A.Haze = HexLinear(A.HazeHex);
		return A;
	}

	float EnvironmentIntensity(float SunI) { return 0.38f * (0.7f + 0.3f * FMath::Min(SunI / 3.2f, 1.f)); }

	FVector SunDirectionThree(float SunUpDegrees, float Yaw)
	{
		const double SunSide = 60.0 * PI / 180.0;
		const double Az = Yaw + PI - SunSide, Up = SunUpDegrees * PI / 180.0;
		return FVector(FMath::Sin(Az) * FMath::Cos(Up), FMath::Sin(Up), FMath::Cos(Az) * FMath::Cos(Up));
	}

	FLinearColor OutdoorEnvironment(const FVector& D)
	{
		// light.js outdoorEnvironment, per direction: (dx, up, dz) is the equirect texel's direction
		static const FVector Light = FVector(0.55, 0.72, -0.42).GetSafeNormal();
		const FVector N = D.GetSafeNormal();
		const double Upv = N.Y;
		const double T = FMath::Clamp((Upv - -0.08) / (0.12 - -0.08), 0.0, 1.0), SkyMix = T * T * (3.0 - 2.0 * T);
		const double High = FMath::Max(Upv, 0.0), Horizon = FMath::Exp(-FMath::Abs(Upv) * 6.0);
		const double Bright = FMath::Pow(FMath::Max(0.0, N.X * Light.X + Upv * Light.Y + N.Z * Light.Z), 24.0) * 0.75;
		return FLinearColor(
			(float)(0.11 * (1 - SkyMix) + (0.26 + High * 0.28 + Horizon * 0.16 + Bright) * SkyMix),
			(float)(0.092 * (1 - SkyMix) + (0.34 + High * 0.29 + Horizon * 0.15 + Bright * 0.93) * SkyMix),
			(float)(0.064 * (1 - SkyMix) + (0.48 + High * 0.3 + Horizon * 0.08 + Bright * 0.78) * SkyMix), 1.f);
	}

	FLinearColor SkyRadiance(const FVector& DirThree, const FTCMood& M, float EnvIntensity, float EnvYaw)
	{
		const FVector D = DirThree.GetSafeNormal();
		// three's envMapRotation: makeRotationFromEuler(-x, -y, -z) applied to the lookup direction
		const double C = FMath::Cos(-EnvYaw), S = FMath::Sin(-EnvYaw);
		const FVector E(C * D.X + S * D.Z, D.Y, -S * D.X + C * D.Z);
		const FLinearColor Env = OutdoorEnvironment(E) * EnvIntensity;
		const float K = M.HemiI / PI;
		const FLinearColor Hemi = ((M.Top + M.Bottom) * 0.5f + (M.Top - M.Bottom) * (0.75f * (float)D.Y)) * K;
		FLinearColor L = Env + Hemi;
		L.R = FMath::Max(L.R, 0.f); L.G = FMath::Max(L.G, 0.f); L.B = FMath::Max(L.B, 0.f); L.A = 1.f;
		return L;
	}

	FVector CubeTexelDirection(int32 Face, int32 X, int32 Y, int32 Size)
	{
		// the D3D face table of TextureCompressorModule.cpp ComputeWSCubeDirectionAtTexelCenter, read as Unreal world axes:
		// the sky light samples its cube by world direction (+X, -X, +Y, -Y, +Z, -Z). The compressor's extra y/z swap is
		// for long-lat sources and put the sky's top on a side face here (the capture read the same light up and down).
		const double Inv = 1.0 / Size;
		const FVector S = FVector((X + 0.5) * Inv * 2 - 1, (Y + 0.5) * Inv * 2 - 1, 1).GetSafeNormal();
		const double x = S.X, y = S.Y, z = S.Z;
		FVector R;
		switch (Face)
		{
		default:
		case 0: R = FVector(+z, -y, -x); break;
		case 1: R = FVector(-z, -y, +x); break;
		case 2: R = FVector(+x, +z, +y); break;
		case 3: R = FVector(+x, -z, -y); break;
		case 4: R = FVector(+x, -y, +z); break;
		case 5: R = FVector(-x, -y, -z); break;
		}
		return R;
	}

	FLinearColor ExpectedDiffuse(const FVector& NormalUnreal, const FTCMood& M, float EnvIntensity, float EnvYaw)
	{
		// the cosine-weighted mean of SkyRadiance over the hemisphere around the normal: what a white Lambert surface shows
		const int32 NT = 96, NP = 192;
		FLinearColor Sum(0, 0, 0, 0);
		for (int32 I = 0; I < NT; I++)
		{
			const double T = (I + 0.5) / NT * PI, Sin = FMath::Sin(T), Cos = FMath::Cos(T);
			for (int32 J = 0; J < NP; J++)
			{
				const double P = (J + 0.5) / NP * 2 * PI;
				const FVector D(Sin * FMath::Cos(P), Sin * FMath::Sin(P), Cos);
				const double W = FMath::Max(0.0, D | NormalUnreal) * Sin * (PI / NT) * (2 * PI / NP) / PI;
				if (W > 0) Sum += SkyRadiance(ThreeToUnreal(D), M, EnvIntensity, EnvYaw) * (float)W; // (x, y, z) <-> (x, z, y) is its own inverse
			}
		}
		Sum.A = 1.f;
		return Sum;
	}

	UTextureCube* MakeSkyCube(const FTCMood& M, float EnvIntensity, float EnvYaw, int32 Size)
	{
		UTextureCube* Cube = UTextureCube::CreateTransient(Size, Size, PF_FloatRGBA, MakeUniqueObjectName(GetTransientPackage(), UTextureCube::StaticClass(), TEXT("TCLookSky")));
		if (!Cube) return nullptr;
		Cube->SRGB = false;
		FTexture2DMipMap& Mip = Cube->GetPlatformData()->Mips[0];
		FFloat16Color* Texels = static_cast<FFloat16Color*>(Mip.BulkData.Lock(LOCK_READ_WRITE));
		for (int32 F = 0; F < 6; F++)
			for (int32 Y = 0; Y < Size; Y++)
				for (int32 X = 0; X < Size; X++)
					Texels[(F * Size + Y) * Size + X] = FFloat16Color(SkyRadiance(ThreeToUnreal(CubeTexelDirection(F, X, Y, Size)), M, EnvIntensity, EnvYaw));
		Mip.BulkData.Unlock();
		Cube->UpdateResource();
		return Cube;
	}

	void LockPostProcess(FPostProcessSettings& S, UMaterialInterface* PostMaterial, float AmbientOcclusion)
	{
		S.bOverride_AutoExposureMethod = true; S.AutoExposureMethod = AEM_Manual;
		S.bOverride_AutoExposureBias = true; S.AutoExposureBias = 0.f;
		S.bOverride_AutoExposureApplyPhysicalCameraExposure = true; S.AutoExposureApplyPhysicalCameraExposure = false;
		S.bOverride_BloomIntensity = true; S.BloomIntensity = 0.f;
		S.bOverride_VignetteIntensity = true; S.VignetteIntensity = 0.f;
		S.bOverride_FilmGrainIntensity = true; S.FilmGrainIntensity = 0.f;
		S.bOverride_SceneFringeIntensity = true; S.SceneFringeIntensity = 0.f;
		S.bOverride_LensFlareIntensity = true; S.LensFlareIntensity = 0.f;
		S.bOverride_MotionBlurAmount = true; S.MotionBlurAmount = 0.f;
		S.bOverride_AmbientOcclusionIntensity = true; S.AmbientOcclusionIntensity = AmbientOcclusion;
		S.bOverride_AmbientOcclusionRadius = true; S.AmbientOcclusionRadius = 160.f;
		int32 WorldRadius = 0;
		FParse::Value(FCommandLine::Get(), TEXT("TCLookAOWorld="), WorldRadius);
		S.bOverride_AmbientOcclusionRadiusInWS = true; S.AmbientOcclusionRadiusInWS = WorldRadius != 0;
		// DiffuseIndirectComposite multiplies the existing scene color by material AO in its static-lighting pass.
		// Model AO is zero to suppress the engine sky fill; that multiplication also erases their own hemisphere and
		// environment light in Emissive. Keep screen AO for the sky pass, without applying material AO to scene color.
		S.bOverride_AmbientOcclusionStaticFraction = true; S.AmbientOcclusionStaticFraction = 0.f;
		S.bOverride_DynamicGlobalIlluminationMethod = true; S.DynamicGlobalIlluminationMethod = EDynamicGlobalIlluminationMethod::None;
		S.bOverride_ReflectionMethod = true; S.ReflectionMethod = EReflectionMethod::None;
		if (PostMaterial) S.WeightedBlendables.Array.Add(FWeightedBlendable(1.f, PostMaterial));
	}

	UMaterialInstanceDynamic* MakePost(UObject* Outer)
	{
		UMaterialInterface* Base = LoadObject<UMaterialInterface>(nullptr, PostPath);
		return Base ? UMaterialInstanceDynamic::Create(Base, Outer) : nullptr;
	}

	void SetPost(UMaterialInstanceDynamic* Post, float Exposure, float FogNear, float FogFar, const FLinearColor& HazeLinear, bool bHazeBeforeTone, bool bNeutral)
	{
		if (!Post) return;
		Post->SetScalarParameterValue(TEXT("Neutral"), bNeutral ? 1.f : 0.f);
		Post->SetScalarParameterValue(TEXT("Exposure"), Exposure);
		Post->SetScalarParameterValue(TEXT("FogNear"), FogNear);
		Post->SetScalarParameterValue(TEXT("FogFar"), FogFar);
		// WebGLMaterials refreshFogUniforms and WebGLBackground setClear: drawn to the canvas, the haze is in the output
		// color space (getUnlitUniformColorSpace: renderer.outputColorSpace); drawn into the composer's target it is the
		// working (linear) color, mixed before OutputPass tone-maps (round 14; M_TCLookPost's HazeBeforeTone)
		Post->SetVectorParameterValue(TEXT("Haze"), ToDisplay(HazeLinear));
		Post->SetVectorParameterValue(TEXT("HazeLinear"), FLinearColor(HazeLinear.R, HazeLinear.G, HazeLinear.B, 1.f));
		Post->SetScalarParameterValue(TEXT("HazeBeforeTone"), bHazeBeforeTone ? 1.f : 0.f);
	}

	FLinearColor HazeOnScreen(const FLinearColor& HazeLinear, float Exposure, bool bHazeBeforeTone)
	{
		const FLinearColor D = bHazeBeforeTone ? ToneMapped(HazeLinear, Exposure) : ToDisplay(HazeLinear);
		return FLinearColor(D.R * 255.f, D.G * 255.f, D.B * 255.f, 1.f);
	}

	void SetCollection(UWorld* World, const FTCMood& M, float EnvIntensity, float EnvYaw, bool bOn)
	{
		UMaterialParameterCollection* Mpc = LoadObject<UMaterialParameterCollection>(nullptr, CollectionPath);
		if (!World || !Mpc) return;
		UMaterialParameterCollectionInstance* I = World->GetParameterCollectionInstance(Mpc);
		if (!I) { World->AddParameterCollectionInstance(Mpc, true); I = World->GetParameterCollectionInstance(Mpc); }
		if (!I) return;
		I->SetVectorParameterValue(TEXT("HemiTop"), M.Top * M.HemiI);
		I->SetVectorParameterValue(TEXT("HemiBottom"), M.Bottom * M.HemiI);
		I->SetVectorParameterValue(TEXT("Env"), FLinearColor(EnvIntensity, EnvYaw, bOn ? 1.f : 0.f, 0.f));
	}

	bool IndirectInMaterials()
	{
		// the model masters' AO of 0 only keeps the sky light off their pixels when the GBuffer stores AO, which it does
		// without static lighting (DeferredShadingCommon.ush); otherwise their own indirect would come on top of the sky
		const IConsoleVariable* V = IConsoleManager::Get().FindConsoleVariable(TEXT("r.AllowStaticLighting"));
		return V && V->GetInt() == 0;
	}

	bool HasUnitSun(const UWorld* World)
	{
		if (!World) return false;
		for (TObjectIterator<UDirectionalLightComponent> It; It; ++It)
			if (It->GetWorld() == World && It->IsRegistered() && It->LightingChannels.bChannel1 && !It->LightingChannels.bChannel0) return true;
		return false;
	}

	void MatchUnitSun(UDirectionalLightComponent* UnitSun, const UDirectionalLightComponent* Sun)
	{
		if (!UnitSun || !Sun) return;
		UnitSun->SetWorldRotation(Sun->GetComponentRotation());
		UnitSun->SetIntensity(Sun->Intensity);
		UnitSun->SetLightColor(Sun->GetLightColor());
	}

	UDirectionalLightComponent* MakeUnitSun(UObject* Outer, UWorld* World, const UDirectionalLightComponent* Sun)
	{
		UDirectionalLightComponent* L = NewObject<UDirectionalLightComponent>(Outer);
		L->SetMobility(EComponentMobility::Movable);
		L->SetAtmosphereSunLight(false);
		L->SetCastShadows(false);
		L->SetLightingChannels(false, true, false);
		L->ForwardShadingPriority = 0;
		// the shadowed sun outranks it for forward shading (see ATCLighting's constructor)
		if (Sun && Sun->ForwardShadingPriority <= 0) const_cast<UDirectionalLightComponent*>(Sun)->SetForwardShadingPriority(1);
		MatchUnitSun(L, Sun);
		if (World) L->RegisterComponentWithWorld(World);
		return L;
	}

	bool AssetsPresent(FString& Missing)
	{
		Missing.Reset();
		for (const TCHAR* P : { PostPath, LambertPath, GroundLambertPath, EmissivePath, LinePath })
			if (!LoadObject<UMaterialInterface>(nullptr, P)) Missing += FString(Missing.IsEmpty() ? TEXT("") : TEXT(", ")) + P;
		if (!LoadObject<UMaterialParameterCollection>(nullptr, CollectionPath)) Missing += FString(Missing.IsEmpty() ? TEXT("") : TEXT(", ")) + CollectionPath;
		return Missing.IsEmpty();
	}
}

// ------------------------------------------------------------------------------------------------- the game's light

ATCLighting::ATCLighting()
{
	PrimaryActorTick.bCanEverTick = true;
	RootComponent = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));
	Sun = CreateDefaultSubobject<UDirectionalLightComponent>(TEXT("Sun"));
	Sun->SetupAttachment(RootComponent);
	Sun->SetMobility(EComponentMobility::Movable);
	Sun->SetAtmosphereSunLight(false);
	Sun->SetDynamicShadowDistanceMovableLight(40000.f); // light.js: the shadow box covers what the view shows, up to 600 m
	// the units' sun (TCLook::UnitChannel): no shadow, lighting channel 1 only; the shadowed sun keeps channel 0
	Sun->SetLightingChannels(true, false, false);
	// lead (slice 12): the shadowed sun stays the one forward-shaded translucency and volumetric fog use, as before the
	// units' sun came in; at equal priority the engine picks either and warns on screen about the two
	Sun->ForwardShadingPriority = 1;
	UnitSun = CreateDefaultSubobject<UDirectionalLightComponent>(TEXT("UnitSun"));
	UnitSun->SetupAttachment(RootComponent);
	UnitSun->SetMobility(EComponentMobility::Movable);
	UnitSun->SetAtmosphereSunLight(false);
	UnitSun->SetCastShadows(false);
	UnitSun->SetLightingChannels(false, true, false);
	Sky = CreateDefaultSubobject<USkyLightComponent>(TEXT("Sky"));
	Sky->SetupAttachment(RootComponent);
	Sky->SetMobility(EComponentMobility::Movable);
	Sky->SourceType = SLS_SpecifiedCubemap;
	Sky->bLowerHemisphereIsBlack = false;
	Sky->SetCastShadows(false);
	Post = CreateDefaultSubobject<UPostProcessComponent>(TEXT("Post"));
	Post->SetupAttachment(RootComponent);
	Post->bUnbound = true;
	Post->Priority = 10.f;
	Base = Look = TCLook::DefaultMood();
	HazeNow = HazeGoal = Base.Haze;
}

void ATCLighting::BeginPlay()
{
	Super::BeginPlay();
	FString Missing;
	if (!TCLook::AssetsPresent(Missing)) UE_LOG(LogTCLook, Error, TEXT("look: missing %s (unreal/Tools/look/make-look-assets.py): the engine tonemapper stays"), *Missing);
	PostMaterial = TCLook::MakePost(this);
	// -TCLookOutputLinear: the post process writes linear color, for a viewport whose target sRGB-encodes in hardware
	bOutputLinear = FParse::Param(FCommandLine::Get(), TEXT("TCLookOutputLinear"));
	// round 14: Graphics High's composer mixes the haze in linear light before the tone curve; -TCHazeAfterTone
	// draws the direct path's order (Graphics Low, ?ao=0), as the earlier slices did
	bHazeAfterToneForced = FParse::Param(FCommandLine::Get(), TEXT("TCHazeAfterTone"));
	bHazeBeforeTone = !bHazeAfterToneForced;
	UE_LOG(LogTCLook, Log, TEXT("look: the haze mixes %s"), bHazeBeforeTone ? TEXT("in linear light before the tone curve (Graphics High: light.js's composer)") : TEXT("after the tone curve, in output space (-TCHazeAfterTone: the direct path)"));
	if (PostMaterial) PostMaterial->SetScalarParameterValue(TEXT("OutputLinear"), bOutputLinear ? 1.f : 0.f);
	// gfx.js High: the AO pass (light.js AO.mix 0.85); the engine's applies to the sky light only
	TCLook::LockPostProcess(Post->Settings, PostMaterial, 0.85f);
	AimSun(0.f);
	Apply(true);
	UE_LOG(LogTCLook, Log, TEXT("look: mood %s (reference %s), sun %.2f lux, exposure %.2f, ACES filmic"), *Base.Name.ToString(), TCLook::ReferencePin, Look.SunI, Look.Exposure);
	if (!TCLook::IndirectInMaterials())
		UE_LOG(LogTCLook, Warning, TEXT("look: r.AllowStaticLighting is on, so the model materials take the engine sky light (their reflections then include the sky fill; unreal/Config/TCPostProcess.md)"));
}

void ATCLighting::StartMatch(const FString& MapName, const FString& MapKey, const FString& WeatherNow, float BrowserYaw, FName Override, const FString& AuthoredMood)
{
	const FName Name = TCLook::MoodFor(MapName, MapKey, WeatherNow, Override, AuthoredMood);
	Base = *TCLook::FindMood(Name);
	// moods.js LOOK[kind] ? kind : 'clear'
	static const TSet<FString> Known = { TEXT("clear"), TEXT("fog"), TEXT("rain"), TEXT("mud"), TEXT("snow") };
	WeatherKind = Known.Contains(WeatherNow) ? WeatherNow : TEXT("clear");
	Rain = Wet = 0.f;
	TCLook::WeatherGoal(Base, WeatherKind, Rain, Wet, Goal, HazeGoal);
	Cur = Goal; HazeNow = HazeGoal; // a match starts in its weather, no easing in
	AimSun(BrowserYaw);
	Apply(true);
	UE_LOG(LogTCLook, Log, TEXT("look: match on '%s' in %s: mood %s, sun %.2f lux, haze x%.2f, exposure %.2f, yaw %.3f"), *MapName, *WeatherKind, *Base.Name.ToString(), Look.SunI, Look.HazeK, Look.Exposure, BrowserYaw);
}

void ATCLighting::SetWeather(const FString& Kind)
{
	static const TSet<FString> Known = { TEXT("clear"), TEXT("fog"), TEXT("rain"), TEXT("mud"), TEXT("snow") };
	WeatherKind = Known.Contains(Kind) ? Kind : TEXT("clear");
	TCLook::WeatherGoal(Base, WeatherKind, Rain, Wet, Goal, HazeGoal);
}

void ATCLighting::SetShower(float InRain, float InWet)
{
	Rain = FMath::Clamp(InRain, 0.f, 1.f); Wet = FMath::Clamp(InWet, 0.f, 1.f);
	TCLook::WeatherGoal(Base, WeatherKind, Rain, Wet, Goal, HazeGoal);
}

void ATCLighting::AimSun(float BrowserYaw)
{
	Yaw = BrowserYaw;
	const FVector Dir = TCLook::ThreeToUnreal(TCLook::SunDirectionThree(Base.SunUp, Yaw));
	Sun->SetWorldRotation((-Dir).Rotation());
	TCLook::MatchUnitSun(UnitSun, Sun);
}

void ATCLighting::Apply(bool bForceSky)
{
	Look = TCLook::Applied(Base, Cur, HazeNow);
	Sun->SetIntensity(Look.SunI);
	Sun->SetLightColor(Look.Sun);
	Sun->SetShadowAmount(FMath::Clamp(Look.Shadow * FMath::Max(0.f, CVarLookShadow.GetValueOnGameThread()), 0.f, 1.f));
	TCLook::MatchUnitSun(UnitSun, Sun);
	// light.js applyGfx: scene.environment = low ? null : the PMREM sky (Client/TCGraphics.h)
	EnvLevel = (int32)FTCGraphics::Get().Level();
	// round 14 lead glue: light.js renderFrame draws High through the composer (the GTAO pass, the haze mixed in linear
	// light before the tone curve) and Low straight to the canvas (no AO pass, the haze after the curve)
	{
		const FTCGraphicsBudget& Gfx = FTCGraphics::Get().Budget();
		SetHazeBeforeTone(Gfx.bAmbientOcclusion);
		if (Post && (bAoApplied < 0 || bAoApplied != (int32)Gfx.bAmbientOcclusion))
		{
			bAoApplied = (int32)Gfx.bAmbientOcclusion;
			Post->Settings.bOverride_AmbientOcclusionIntensity = true;
			Post->Settings.AmbientOcclusionIntensity = Gfx.bAmbientOcclusion ? 0.85f : 0.f;
			UE_LOG(LogTCLook, Log, TEXT("look: graphics %s: the AO pass %s, the haze mixes %s"), LexToString(FTCGraphics::Get().Level()), Gfx.bAmbientOcclusion ? TEXT("on (0.85)") : TEXT("off"),
				bHazeBeforeTone ? TEXT("before the tone curve") : TEXT("after the tone curve"));
		}
	}
	const float Env = FTCGraphics::Get().Budget().bEnvironment ? TCLook::EnvironmentIntensity(Look.SunI) : 0.f;
	const float IndirectScale = FMath::Max(0.f, CVarLookIndirect.GetValueOnGameThread());
	FTCMood IndirectLook = Look;
	IndirectLook.HemiI *= IndirectScale;
	TCLook::SetCollection(GetWorld(), IndirectLook, Env * IndirectScale, Yaw, TCLook::IndirectInMaterials());
	// the sky light's cube is rebuilt only when what it holds moved by more than half a percent
	const bool bSkyMoved = SkyEnv < 0.f || FMath::Abs(Env - SkyEnv) > 0.005f * SkyEnv || !SkyTop.Equals(Look.Top * Look.HemiI, 0.002f) || !SkyBottom.Equals(Look.Bottom * Look.HemiI, 0.002f);
	if (bForceSky || bSkyMoved)
	{
		SkyCube = TCLook::MakeSkyCube(IndirectLook, Env * IndirectScale, Yaw);
		Sky->SetCubemap(SkyCube);
		Sky->SetIntensity(1.f);
		Sky->SetLightColor(FLinearColor::White);
		Sky->RecaptureSky();
		SkyEnv = Env; SkyTop = Look.Top * Look.HemiI; SkyBottom = Look.Bottom * Look.HemiI;
		// what the capture must hold: the cube's own cosine integral for a white surface facing up
		SkyWantUp = TCLook::ExpectedDiffuse(FVector(0, 0, 1), IndirectLook, Env * IndirectScale, Yaw);
		bSkyCheck = true; bSkyCaptured = false; SkyTries = 1;
		SkyCheckAt = GetWorld() ? GetWorld()->GetRealTimeSeconds() + 0.25 : 0.0;
	}
}

void ATCLighting::SetLobbyLook(const FTCLobbyLook* L)
{
	if (!L)
	{
		if (!bLobbyLook) return;
		bLobbyLook = false;
		Sun->SetCastShadows(true);
		AimSun(Yaw);
		Apply(true);
		UE_LOG(LogTCLook, Log, TEXT("look: the lobby backdrop's light ends: mood %s again"), *Base.Name.ToString());
		return;
	}
	if (bLobbyLook && FMath::IsNearlyEqual(LobbyLook.FogNear, L->FogNear) && FMath::IsNearlyEqual(LobbyLook.FogFar, L->FogFar)) return;
	bLobbyLook = true;
	LobbyLook = *L;
	// lobby-view.js: HemisphereLight(sky, ground, 0.9) and DirectionalLight(sun, 3.0), no environment, no shadows
	FTCMood M = Base;
	M.Sun = L->Sun; M.SunI = L->SunI; M.Top = L->Sky; M.Bottom = L->Ground; M.HemiI = L->HemiI;
	M.Haze = L->Fog; M.HazeK = 1.f; M.Exposure = FTCLobbyLook::Exposure; M.Shadow = 0.f;
	Look = M;
	Sun->SetCastShadows(L->bShadows);
	Sun->SetIntensity(M.SunI);
	Sun->SetLightColor(M.Sun);
	Sun->SetWorldRotation((-TCLook::ThreeToUnreal(L->SunDirThree)).Rotation());
	TCLook::MatchUnitSun(UnitSun, Sun);
	TCLook::SetCollection(GetWorld(), M, L->Env, 0.f, TCLook::IndirectInMaterials());
	SkyCube = TCLook::MakeSkyCube(M, L->Env, 0.f);
	Sky->SetCubemap(SkyCube);
	Sky->SetIntensity(1.f);
	Sky->SetLightColor(FLinearColor::White);
	Sky->RecaptureSky();
	SkyEnv = -1.f; // the mood's cube is rebuilt when the lobby look ends
	SkyWantUp = TCLook::ExpectedDiffuse(FVector(0, 0, 1), M, L->Env, 0.f);
	bSkyCheck = true; bSkyCaptured = false; SkyTries = 1;
	SkyCheckAt = GetWorld() ? GetWorld()->GetRealTimeSeconds() + 0.25 : 0.0;
	TCLook::SetPost(PostMaterial, M.Exposure, L->FogNear, L->FogFar, L->Fog, L->bHazeBeforeTone, L->bNeutralToneMapping);
	LastFogNear = L->FogNear; LastFogFar = L->FogFar;
	UE_LOG(LogTCLook, Log, TEXT("look: the lobby backdrop's light (client/lobby-view.js): sun %.1f without shadows, hemisphere %.1f, no environment, fog #%06x from %.1f to %.1f m after the tone curve, %s"),
		M.SunI, M.HemiI, FTCLobbyLook::FogHex, L->FogNear, L->FogFar, L->bNeutralToneMapping ? TEXT("NeutralToneMapping") : TEXT("ACES filmic"));
}

void ATCLighting::CheckSkyCapture()
{
	// The sky light's capture of a just-made transient cube can run before the cube's render resource exists, and then
	// holds no light at all: everything the engine sky light fills (the map pieces, trees and bushes, the soldiers) is
	// left with the sun alone and its shaded faces go nearly black. Read the capture back and redo it until it holds
	// the cube's light (the comparison commandlet does the same with its retries).
	const UWorld* World = GetWorld();
	if (!bSkyCheck || !World || World->GetRealTimeSeconds() < SkyCheckAt) return;
	// lead (slice 12): nothing is captured without a renderer (-nullrhi): no check, no false error
	if (!FApp::CanEverRender())
	{
		bSkyCheck = false;
		UE_LOG(LogTCLook, Log, TEXT("look: no renderer (-nullrhi): the sky light capture check is skipped"));
		return;
	}
	const FSHVectorRGB3 Sh = Sky->GetIrradianceEnvironmentMap();
	SkyGotUp = Dot(Sh, FSHVector3::CalcDiffuseTransfer(FVector(0, 0, 1))) / PI;
	SkyGotUp.A = 1.f;
	if (SkyGotUp.G >= 0.5f * SkyWantUp.G && SkyWantUp.G > 0.f)
	{
		bSkyCheck = false; bSkyCaptured = true;
		UE_LOG(LogTCLook, Log, TEXT("look: sky light captured on try %d: diffuse up (%.4f, %.4f, %.4f), the cube's integral (%.4f, %.4f, %.4f)"),
			SkyTries, SkyGotUp.R, SkyGotUp.G, SkyGotUp.B, SkyWantUp.R, SkyWantUp.G, SkyWantUp.B);
		return;
	}
	if (SkyTries >= 40)
	{
		bSkyCheck = false;
		UE_LOG(LogTCLook, Error, TEXT("look: the sky light capture still holds diffuse up (%.4f, %.4f, %.4f) after %d tries, want (%.4f, %.4f, %.4f): shaded faces of the map pieces and scenery will be too dark"),
			SkyGotUp.R, SkyGotUp.G, SkyGotUp.B, SkyTries, SkyWantUp.R, SkyWantUp.G, SkyWantUp.B);
		return;
	}
	if (SkyTries == 1)
		UE_LOG(LogTCLook, Warning, TEXT("look: the sky light's first capture holds diffuse up (%.4f, %.4f, %.4f), want (%.4f, %.4f, %.4f): capturing again"),
			SkyGotUp.R, SkyGotUp.G, SkyGotUp.B, SkyWantUp.R, SkyWantUp.G, SkyWantUp.B);
	Sky->RecaptureSky();
	SkyTries++;
	SkyCheckAt = World->GetRealTimeSeconds() + 0.25;
}

void ATCLighting::Tick(float Dt)
{
	Super::Tick(Dt);
	CheckSkyCapture();
	if (bLobbyLook)
	{
		// round 15: the lobby backdrop's light holds; no weather easing or camera-following haze in the lobby
		TCLook::SetPost(PostMaterial, FTCLobbyLook::Exposure, LobbyLook.FogNear, LobbyLook.FogFar, LobbyLook.Fog, LobbyLook.bHazeBeforeTone, LobbyLook.bNeutralToneMapping);
		return;
	}
	const FVector2D Controls(CVarLookIndirect.GetValueOnGameThread(), CVarLookShadow.GetValueOnGameThread());
	if (!Controls.Equals(LastDiagnosticControls))
	{
		LastDiagnosticControls = Controls;
		Apply(true);
		UE_LOG(LogTCLook, Log, TEXT("look controls: indirect %.4f shadow %.4f"), Controls.X, Controls.Y);
	}
	// atmosphere.js update: ease toward the weather, about 8 s to settle
	const float K = 1.f - FMath::Exp(-Dt / 2.5f);
	auto Near = [](float A, float B) { return FMath::Abs(A - B) <= 5e-4f; };
	const bool bSettled = Near(Cur.Haze, Goal.Haze) && Near(Cur.Sun, Goal.Sun) && Near(Cur.Shadow, Goal.Shadow) && Near(Cur.Clouds, Goal.Clouds) && Near(Cur.Wet, Goal.Wet)
		&& Near(Cur.Sheen, Goal.Sheen) && Near(Cur.Mud, Goal.Mud) && Near(Cur.Cover, Goal.Cover) && Near(Cur.Banks, Goal.Banks) && TCLook::LinearHex(HazeNow) == TCLook::LinearHex(HazeGoal);
	if (K > 0.f && !bSettled)
	{
		auto Ease = [K](float& C, float G) { C += (G - C) * K; };
		Ease(Cur.Haze, Goal.Haze); Ease(Cur.Sun, Goal.Sun); Ease(Cur.Shadow, Goal.Shadow); Ease(Cur.Clouds, Goal.Clouds); Ease(Cur.Wet, Goal.Wet);
		Ease(Cur.Sheen, Goal.Sheen); Ease(Cur.Mud, Goal.Mud); Ease(Cur.Cover, Goal.Cover); Ease(Cur.Banks, Goal.Banks);
		HazeNow = FMath::Lerp(HazeNow, HazeGoal, K);
		Apply(false);
	}
	// gfx.onChange(applyGfx): a new graphics level relights at once (the environment on or off, the sky cube with it)
	else if (EnvLevel != (int32)FTCGraphics::Get().Level()) Apply(true);
	if (!bViewportLogged && GEngine && GEngine->GameViewport && GEngine->GameViewport->Viewport)
	{
		bViewportLogged = true;
		UE_LOG(LogTCLook, Log, TEXT("look: viewport display gamma %.2f, the post process writes %s"), GEngine->GameViewport->Viewport->GetDisplayGamma(), bOutputLinear ? TEXT("linear (-TCLookOutputLinear)") : TEXT("display-encoded"));
	}
	// light.js followView: the haze from 0.6 to 3.8 camera distances, pulled in by the mood and weather
	float Dist = CameraDistance;
	if (Dist <= 0.f)
	{
		Dist = 85.f; // light.js: cam?.dist ?? 85
		if (APlayerController* PC = GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr)
			if (PC->PlayerCameraManager)
			{
				const FVector Eye = PC->PlayerCameraManager->GetCameraLocation(), Fwd = PC->PlayerCameraManager->GetCameraRotation().Vector();
				if (Fwd.Z < -0.02) Dist = (float)(((GroundZ - Eye.Z) / Fwd.Z) / 100.0);
			}
	}
	LastFogNear = Dist * 0.6f / Look.HazeK;
	LastFogFar = Dist * 3.8f / Look.HazeK;
	const float HazeScale = FMath::Max(0.f, CVarLookHaze.GetValueOnGameThread());
	if (HazeScale > 0.f) { LastFogNear /= HazeScale; LastFogFar /= HazeScale; }
	else { LastFogNear = LastFogFar = 0.f; }
	TCLook::SetPost(PostMaterial, Look.Exposure * FMath::Max(0.f, CVarLookExposure.GetValueOnGameThread()), LastFogNear, LastFogFar, Look.Haze, bHazeBeforeTone, CVarLookNeutral.GetValueOnGameThread() != 0);
	FollowShadowView();
	if (FParse::Param(FCommandLine::Get(), TEXT("TCLookAudit"))) Audit();
}

void ATCLighting::FollowShadowView()
{
	const APlayerController* PC = GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr;
	if (!PC || !PC->PlayerCameraManager || !Sun) return;
	const FTCGraphicsBudget& G = FTCGraphics::Get().Budget();
	const bool Follow = CVarLookShadowView.GetValueOnGameThread() != 0;
	const bool Filter = CVarLookShadowFilter.GetValueOnGameThread() != 0 && G.bSoftShadows;
	int32 Resolution = G.ShadowMapSize;
	float Distance = 40000.f;
	if (Follow)
	{
		const APlayerCameraManager& Camera = *PC->PlayerCameraManager;
		const FRotationMatrix Axes(Camera.GetCameraRotation());
		double Aspect = 16.0 / 9.0;
		int32 W, H; PC->GetViewportSize(W, H);
		if (W > 0 && H > 0) Aspect = double(W) / H;
		const double TanX = FMath::Tan(FMath::DegreesToRadians(Camera.GetFOVAngle()) * 0.5);
		const double Radius = G.bSoftShadows ? G.ShadowRadiusPerSoft * Look.Soft : G.ShadowRadius;
		const TCLook::FShadowView V = TCLook::ShadowView(Camera.GetCameraLocation() / 100.0, Axes.GetUnitAxis(EAxis::X),
			Axes.GetUnitAxis(EAxis::Y), Axes.GetUnitAxis(EAxis::Z), TanX, TanX / Aspect, GroundZ / 100.0, G.ShadowMapSize, Radius, Filter);
		Resolution = V.Resolution;
		Distance = float(V.FarDepth * 100.0);
	}
	Sun->SetDynamicShadowCascades(Follow ? 1 : 3);
	Sun->SetDynamicShadowDistanceMovableLight(Distance);
	Sun->SetCascadeTransitionFraction(Follow ? 0.f : 0.1f);
	Sun->ShadowDistanceFadeoutFraction = Follow ? 0.f : 0.1f;
	auto Set = [](IConsoleVariable* C, int32 Value)
	{
		if (C && C->GetInt() != Value) C->Set(Value, ECVF_SetByCode);
	};
	// MaxResolution alone does not size directional cascades. Low must resize the CSM map as well.
	static IConsoleVariable* Csm = IConsoleManager::Get().FindConsoleVariable(TEXT("r.Shadow.MaxCSMResolution"));
	static IConsoleVariable* Quality = IConsoleManager::Get().FindConsoleVariable(TEXT("r.ShadowQuality"));
	Set(Csm, Resolution);
	Set(Quality, G.bSoftShadows ? (Filter ? 4 : 3) : 1);
}

void ATCLighting::Audit()
{
	UWorld* World = GetWorld();
	if (!World || World->GetRealTimeSeconds() < NextAuditAt) return;
	NextAuditAt = World->GetRealTimeSeconds() + 2.0;
	int32 HeightFogs = 0;
	for (TObjectIterator<UExponentialHeightFogComponent> It; It; ++It)
		if (It->GetWorld() == World && It->IsRegistered() && It->IsVisible()) ++HeightFogs;
	UE_LOG(LogTCLook, Log, TEXT("look audit passes: AO %.4f static fraction %.4f sky intensity %.4f engine height fogs %d neutral curve %d"),
		Post->Settings.AmbientOcclusionIntensity, Post->Settings.AmbientOcclusionStaticFraction, Sky->Intensity, HeightFogs, CVarLookNeutral.GetValueOnGameThread());
	static const IConsoleVariable* Csm = IConsoleManager::Get().FindConsoleVariable(TEXT("r.Shadow.MaxCSMResolution"));
	static const IConsoleVariable* Quality = IConsoleManager::Get().FindConsoleVariable(TEXT("r.ShadowQuality"));
	UE_LOG(LogTCLook, Log, TEXT("look shadow audit: follow %d filter %d cascades %d distance %.4f m CSM resolution %d quality %d strength %.4f"),
		CVarLookShadowView.GetValueOnGameThread(), CVarLookShadowFilter.GetValueOnGameThread(), Sun->DynamicShadowCascades,
		Sun->DynamicShadowDistanceMovableLight / 100.f, Csm ? Csm->GetInt() : -1, Quality ? Quality->GetInt() : -1,
		Look.Shadow * FMath::Max(0.f, CVarLookShadow.GetValueOnGameThread()));
	if (auto* Collection = LoadObject<UMaterialParameterCollection>(nullptr, TCLook::CollectionPath))
		if (auto* Instance = World->GetParameterCollectionInstance(Collection))
		{
			FLinearColor Env, Top, Bottom;
			Instance->GetVectorParameterValue(TEXT("Env"), Env);
			Instance->GetVectorParameterValue(TEXT("HemiTop"), Top);
			Instance->GetVectorParameterValue(TEXT("HemiBottom"), Bottom);
			UE_LOG(LogTCLook, Log, TEXT("look audit collection: env %s top %s bottom %s sun %.6f unitSun %.6f"),
				*Env.ToString(), *Top.ToString(), *Bottom.ToString(), Sun->Intensity, UnitSun->Intensity);
		}
	if (const auto* PC = World->GetFirstPlayerController(); PC && PC->PlayerCameraManager)
	{
		const auto& V = PC->PlayerCameraManager->GetCameraCacheView();
		UE_LOG(LogTCLook, Log, TEXT("look audit: camera %s rotation %s fov %.6f exposure %.6f fog %.6f %.6f beforeTone %d indirect %.6f shadow %.6f"),
			*V.Location.ToString(), *V.Rotation.ToString(), V.FOV, Look.Exposure * CVarLookExposure.GetValueOnGameThread(),
			LastFogNear, LastFogFar, bHazeBeforeTone, CVarLookIndirect.GetValueOnGameThread(), CVarLookShadow.GetValueOnGameThread());
	}
	TMap<FString, UMaterialInterface*> Surfaces;
	for (TObjectIterator<UInstancedStaticMeshComponent> It; It; ++It)
		if (It->GetWorld() == World && It->IsRegistered())
			for (int32 I = 0; I < It->GetNumMaterials(); I++) if (auto* M = It->GetMaterial(I))
				Surfaces.FindOrAdd(M->GetMaterial()->GetName(), M);
	TArray<FString> Names; Surfaces.GetKeys(Names); Names.Sort();
	for (const FString& Name : Names)
	{
		const FMaterialRenderProxy* Proxy = Surfaces[Name]->GetRenderProxy();
		const auto FeatureLevel = World->GetFeatureLevel();
		ENQUEUE_RENDER_COMMAND(TCLookAudit)([Proxy, FeatureLevel, Name](FRHICommandListImmediate&)
		{
			const FMaterial* Requested = Proxy->GetMaterialNoFallback(FeatureLevel);
			const FMaterialRenderProxy* Fallback = nullptr;
			const FMaterial& Actual = Proxy->GetMaterialWithFallback(FeatureLevel, Fallback);
			UE_LOG(LogTCLook, Log, TEXT("look audit material: %s requested %s actual %s ready %d"), *Name,
				Requested ? *Requested->GetFriendlyName() : TEXT("missing"), *Actual.GetFriendlyName(), Requested == &Actual);
		});
	}
}

// ------------------------------------------------------------------------------------------------- the comparison

namespace TCLookCompareImpl
{
	FVector FromReference(const TArray<TSharedPtr<FJsonValue>>& A) { return A.Num() == 3 ? FVector(A[0]->AsNumber(), A[2]->AsNumber(), A[1]->AsNumber()) * 100.0 : FVector::ZeroVector; }
	double Num(const TSharedPtr<FJsonObject>& O, const TCHAR* Field, double Default) { double V = Default; if (O) O->TryGetNumberField(Field, V); return V; }
	uint32 HexField(const TSharedPtr<FJsonObject>& O, const TCHAR* Field, uint32 Default) { return (uint32)Num(O, Field, Default); }
	// TCModelCompare's color reading (FColor to linear through the engine's sRGB table)
	FLinearColor Hex(const TSharedPtr<FJsonObject>& O, const TCHAR* Field, uint32 Default) { const uint32 C = HexField(O, Field, Default); return FLinearColor(FColor((C >> 16) & 255, (C >> 8) & 255, C & 255)); }

	struct FScratchWorld
	{
		UWorld* World = nullptr;
		explicit FScratchWorld(const TCHAR* Name)
		{
			World = UWorld::CreateWorld(EWorldType::Game, false, Name);
			FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
			Context.SetCurrentWorld(World);
			World->InitializeActorsForPlay(FURL());
		}
		~FScratchWorld()
		{
			GEngine->DestroyWorldContext(World);
			World->DestroyWorld(false);
		}
	};

	FTCUnit MakeUnit(int32 Id, const FString& Type, int32 Owner, float X, float Z, const FTCUnitTable& Table)
	{
		FTCUnit U;
		U.Id = Id; U.Type = Type; U.Owner = Owner; U.X = X; U.Z = Z;
		const FTCUnitDef* Def = Table.Find(Type);
		U.Hp = Def ? FMath::Max(1, Def->MaxHp()) : 100;
		U.Built = 1.f;
		return U;
	}

	void FinishCompiling()
	{
#if WITH_EDITOR
		FAssetCompilingManager::Get().FinishAllCompilation();
		if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();
#endif
	}

	FTCMood MoodFromJson(const TSharedPtr<FJsonObject>& R)
	{
		FTCMood M = TCLook::DefaultMood();
		if (!R) return M;
		M.SunUp = (float)Num(R, TEXT("sunUp"), M.SunUp);
		M.SunHex = HexField(R, TEXT("sun"), M.SunHex); M.TopHex = HexField(R, TEXT("top"), M.TopHex);
		M.BottomHex = HexField(R, TEXT("bottom"), M.BottomHex); M.HazeHex = HexField(R, TEXT("haze"), M.HazeHex);
		M.Sun = TCLook::HexLinear(M.SunHex); M.Top = TCLook::HexLinear(M.TopHex); M.Bottom = TCLook::HexLinear(M.BottomHex); M.Haze = TCLook::HexLinear(M.HazeHex);
		M.SunI = (float)Num(R, TEXT("sunI"), M.SunI); M.HemiI = (float)Num(R, TEXT("hemiI"), M.HemiI); M.HazeK = (float)Num(R, TEXT("hazeK"), M.HazeK);
		M.Shadow = (float)Num(R, TEXT("shadow"), M.Shadow); M.Soft = (float)Num(R, TEXT("soft"), M.Soft); M.Exposure = (float)Num(R, TEXT("exposure"), M.Exposure);
		return M;
	}

	bool SavePng(const FString& Path, int32 W, int32 H, TArray<FColor>& Pixels)
	{
		for (FColor& C : Pixels) C.A = 255;
		TArray64<uint8> Bytes;
		FImageUtils::PNGCompressImageArray(W, H, TArrayView64<const FColor>(Pixels.GetData(), Pixels.Num()), Bytes);
		return FFileHelper::SaveArrayToFile(Bytes, *Path);
	}

	enum class ELight : uint8 { Look, Before, GameBefore };
}

UTCLookCompareCommandlet::UTCLookCompareCommandlet()
{
	LogToConsole = true;
}

int32 UTCLookCompareCommandlet::Main(const FString& Params)
{
	using namespace TCLookCompareImpl;
	FString CamerasPath, OutDir, LightName = TEXT("look"), Only;
	FParse::Value(*Params, TEXT("Cameras="), CamerasPath);
	FParse::Value(*Params, TEXT("Out="), OutDir);
	FParse::Value(*Params, TEXT("Light="), LightName);
	FParse::Value(*Params, TEXT("Assets="), Only, false);
	const bool bNoPlates = FParse::Param(*Params, TEXT("NoPlates")), bNoMasks = FParse::Param(*Params, TEXT("NoMasks"));
	// calibration switches for the look: the sun's lux times SunScale; SkyUniform >= 0 fills the sky cube with that radiance
	float SunScale = 1.f, SkyUniform = -1.f;
	FParse::Value(*Params, TEXT("SunScale="), SunScale);
	FParse::Value(*Params, TEXT("SkyUniform="), SkyUniform);
	const bool bNoUnitShadows = FParse::Param(*Params, TEXT("NoUnitShadows")); // calibration: the units cast no shadow
	const ELight Mode = LightName == TEXT("before") ? ELight::Before : LightName == TEXT("gamebefore") ? ELight::GameBefore : ELight::Look;
	FString Text;
	TSharedPtr<FJsonObject> Doc;
	if (CamerasPath.IsEmpty() || OutDir.IsEmpty() || !FFileHelper::LoadFileToString(Text, *CamerasPath) || !FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Doc) || !Doc.IsValid())
	{
		UE_LOG(LogTCLook, Error, TEXT("usage: -run=TCLookCompare -Cameras=<cameras.json> -Out=<dir> [-Light=look|before|gamebefore] [-Assets=a,b] (could not read '%s')"), *CamerasPath);
		return 1;
	}
	TSet<FString> Filter;
	{
		TArray<FString> Parts;
		Only.ParseIntoArray(Parts, TEXT(","));
		Filter.Append(Parts);
	}
	IFileManager::Get().MakeDirectory(*OutDir, true);
	FTCUnitTable Table;
	FString Error;
	if (!Table.Load(Error)) { UE_LOG(LogTCLook, Error, TEXT("unit table: %s"), *Error); return 1; }
	if (Mode == ELight::Look && !TCLook::AssetsPresent(Error)) { UE_LOG(LogTCLook, Error, TEXT("look assets missing: %s"), *Error); return 1; }
	const int32 W = (int32)Num(Doc, TEXT("width"), 1280), H = (int32)Num(Doc, TEXT("height"), 720);
	const TSharedPtr<FJsonObject>* LightPtr;
	const TSharedPtr<FJsonObject> Light = Doc->TryGetObjectField(TEXT("light"), LightPtr) ? *LightPtr : MakeShared<FJsonObject>();
	const TSharedPtr<FJsonObject>* RowPtr;
	const FTCMood Mood = MoodFromJson(Light->TryGetObjectField(TEXT("row"), RowPtr) ? *RowPtr : nullptr);
	const FString Suffix = FString(Mode == ELight::Look ? TEXT("look") : Mode == ELight::Before ? TEXT("before") : TEXT("gamebefore")) + (bNoUnitShadows ? TEXT("-noshadow") : TEXT(""));

	FScratchWorld Scratch(TEXT("TCLookCompare"));
	UWorld* World = Scratch.World;
	UObject* Outer = World->GetWorldSettings();
	ATCUnitRenderer* Renderer = World->SpawnActor<ATCUnitRenderer>(ATCUnitRenderer::StaticClass(), FTransform::Identity);
	Renderer->SetEnabledBatches((1u << ATCUnitRenderer::BatchCount) - 1);

	const TArray<TSharedPtr<FJsonValue>>* SunDirArr;
	FVector SunDir(0.5, -0.5, 0.7);
	if (Light->TryGetArrayField(TEXT("sunDir"), SunDirArr)) SunDir = FromReference(*SunDirArr).GetSafeNormal();
	UDirectionalLightComponent* SunLight = NewObject<UDirectionalLightComponent>(Outer);
	USkyLightComponent* Sky = NewObject<USkyLightComponent>(Outer);
	UMaterialInterface* Basic = LoadObject<UMaterialInterface>(nullptr, TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	auto Flat = [&](const FLinearColor& C) { UMaterialInstanceDynamic* M = UMaterialInstanceDynamic::Create(Basic, Outer); M->SetVectorParameterValue(TEXT("Color"), C); return M; };
	auto Mid = [&](const TCHAR* Path) { UMaterialInterface* B = LoadObject<UMaterialInterface>(nullptr, Path); return B ? UMaterialInstanceDynamic::Create(B, Outer) : nullptr; };
	UMaterialInstanceDynamic* PostMid = nullptr;
	const float EnvIntensity = (float)Num(Light, TEXT("environmentIntensity"), TCLook::EnvironmentIntensity(Mood.SunI));
	const float EnvYaw = (float)Num(Light, TEXT("environmentRotationY"), 0.0);
	if (Mode == ELight::Look)
	{
		SunLight->SetMobility(EComponentMobility::Movable);
		SunLight->SetWorldRotation((-SunDir).Rotation());
		SunLight->SetLightColor(Mood.Sun);
		SunLight->SetIntensity((float)Num(Light, TEXT("sunI"), Mood.SunI) * SunScale);
		SunLight->SetShadowAmount((float)Num(Light, TEXT("shadowIntensity"), Mood.Shadow));
		SunLight->SetAtmosphereSunLight(false);
		SunLight->SetLightingChannels(true, false, false);
		SunLight->RegisterComponentWithWorld(World);
		// the units' unshadowed sun (TCLook::UnitChannel), as ATCLighting has it; -UnitShadows leaves it out (slice 11's light)
		if (!FParse::Param(*Params, TEXT("UnitShadows"))) TCLook::MakeUnitSun(Outer, World, SunLight);
		Sky->SetMobility(EComponentMobility::Movable);
		Sky->SourceType = SLS_SpecifiedCubemap;
		Sky->bLowerHemisphereIsBlack = false;
		if (SkyUniform >= 0.f)
		{
			FTCMood Even = Mood;
			Even.Top = Even.Bottom = FLinearColor(SkyUniform, SkyUniform, SkyUniform) * (PI / Even.HemiI); // equal halves: L = top I / pi
			Sky->Cubemap = TCLook::MakeSkyCube(Even, 0.f, 0.f);
		}
		else Sky->Cubemap = TCLook::MakeSkyCube(Mood, EnvIntensity, EnvYaw);
		Sky->SetIntensity(1.f);
		Sky->SetLightColor(FLinearColor::White);
		TCLook::SetCollection(World, Mood, EnvIntensity, EnvYaw, TCLook::IndirectInMaterials());
		UE_LOG(LogTCLook, Display, TEXT("LOOK model materials: %s"), TCLook::IndirectInMaterials() ? TEXT("their own indirect light (r.AllowStaticLighting=0)") : TEXT("the engine sky light (r.AllowStaticLighting is on)"));
		PostMid = TCLook::MakePost(Outer);
	}
	else if (Mode == ELight::GameBefore)
	{
		// TCModelCompare -GameLight: ATCGameMode::SpawnScene's light before this slice
		SunLight->SetWorldRotation(FRotator(-50.f, 35.f, 0.f));
		SunLight->SetAtmosphereSunLight(true);
		SunLight->RegisterComponentWithWorld(World);
		USkyAtmosphereComponent* Atmosphere = NewObject<USkyAtmosphereComponent>(Outer);
		Atmosphere->RegisterComponentWithWorld(World);
		Sky->SetRealTimeCapture(true);
	}
	else
	{
		// TCModelCompare's stand-in light
		SunLight->SetWorldRotation((-SunDir).Rotation());
		SunLight->SetLightColor(Hex(Light, TEXT("sun"), 0xffffff));
		SunLight->SetIntensity(Num(Light, TEXT("sunLux"), 6.0));
		SunLight->RegisterComponentWithWorld(World);
		Sky->SourceType = SLS_SpecifiedCubemap;
		Sky->Cubemap = LoadObject<UTextureCube>(nullptr, TEXT("/Engine/MapTemplates/Sky/DaylightAmbientCubemap.DaylightAmbientCubemap"));
		Sky->SetLightColor(Hex(Light, TEXT("hemiTop"), 0xffffff));
		Sky->SetIntensity(Num(Light, TEXT("skyIntensity"), 1.0));
	}
	Sky->RegisterComponentWithWorld(World);
	// the transient cube's resource is made on the render thread: let it exist before the capture reads it, then wait for
	// the capture's irradiance (nothing ticks in a commandlet)
	FlushRenderingCommands();
	for (int32 Try = 0; Try < 5; Try++)
	{
		Sky->RecaptureSky();
		USkyLightComponent::UpdateSkyCaptureContents(World);
		FlushRenderingCommands();
		const FSHVectorRGB3 Sh = Sky->GetIrradianceEnvironmentMap();
		// the capture's diffuse for six normals (Dot with the cosine transfer is the irradiance; / pi the white Lambert
		// surface's radiance) against the integral of the radiance the cube was filled with
		const FLinearColor Up = Dot(Sh, FSHVector3::CalcDiffuseTransfer(FVector(0, 0, 1))) / PI;
		if (Mode == ELight::Look && SkyUniform < 0.f)
		{
			for (const FVector& N : { FVector(0, 0, 1), FVector(0, 0, -1), FVector(1, 0, 0), FVector(-1, 0, 0), FVector(0, 1, 0), FVector(0, -1, 0) })
			{
				const FLinearColor Got = Dot(Sh, FSHVector3::CalcDiffuseTransfer(N)) / PI, Want = TCLook::ExpectedDiffuse(N, Mood, EnvIntensity, EnvYaw);
				UE_LOG(LogTCLook, Display, TEXT("LOOK sky light capture %d, normal (%g, %g, %g): diffuse (%.4f, %.4f, %.4f), the cube's integral (%.4f, %.4f, %.4f)"), Try, N.X, N.Y, N.Z, Got.R, Got.G, Got.B, Want.R, Want.G, Want.B);
			}
		}
		else UE_LOG(LogTCLook, Display, TEXT("LOOK sky light capture %d: diffuse up (%.4f, %.4f, %.4f)"), Try, Up.R, Up.G, Up.B);
		if (Mode != ELight::Look || SkyUniform == 0.f || Up.G > 1e-4f) break;
	}

	UStaticMeshComponent* Ground = NewObject<UStaticMeshComponent>(Outer);
	Ground->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Plane.Plane")));
	Ground->SetWorldTransform(FTransform(FQuat::Identity, FVector::ZeroVector, FVector(600, 600, 1)));
	const FLinearColor GroundC = Hex(Light, TEXT("ground"), 0x6c6c45), LineC = Hex(Light, TEXT("grid"), 0x56623a), CentreC = Hex(Light, TEXT("gridCentre"), 0x4a5530);
	if (Mode == ELight::Look)
	{
		UMaterialInstanceDynamic* G = Mid(TCLook::LambertPath);
		G->SetVectorParameterValue(TEXT("Color"), TCLook::HexLinear(HexField(Light, TEXT("ground"), 0x6c6c45)));
		Ground->SetMaterial(0, G);
	}
	else Ground->SetMaterial(0, Flat(GroundC));
	Ground->RegisterComponentWithWorld(World);
	// GridHelper(48, 24): lines every 2 m over 48 m, 2 cm over the ground, 0.45 opaque; the middle lines in the centre color
	TArray<UInstancedStaticMeshComponent*> Grids;
	for (int32 Pass = 0; Pass < (Mode == ELight::Look ? 2 : 1); Pass++)
	{
		UInstancedStaticMeshComponent* Grid = NewObject<UInstancedStaticMeshComponent>(Outer);
		Grid->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Cube.Cube")));
		Grid->SetCastShadow(false);
		if (Mode == ELight::Look)
		{
			UMaterialInstanceDynamic* L = Mid(TCLook::LinePath);
			L->SetVectorParameterValue(TEXT("Color"), TCLook::HexLinear(HexField(Light, Pass ? TEXT("gridCentre") : TEXT("grid"), Pass ? 0x4a5530 : 0x56623a)));
			L->SetScalarParameterValue(TEXT("Opacity"), 0.45f);
			Grid->SetMaterial(0, L);
		}
		else Grid->SetMaterial(0, Flat(FMath::Lerp(GroundC, LineC, 0.45f)));
		for (int32 I = 0; I <= 24; I++)
		{
			const bool bCentre = I == 12;
			if (Mode == ELight::Look && bCentre != (Pass == 1)) continue;
			const double O = (I - 12) * 200.0;
			Grid->AddInstance(FTransform(FQuat::Identity, FVector(O, 0, 2), FVector(0.03, 48.0, 0.01)));
			Grid->AddInstance(FTransform(FQuat::Identity, FVector(0, O, 2), FVector(48.0, 0.03, 0.01)));
		}
		Grid->RegisterComponentWithWorld(World);
		Grids.Add(Grid);
	}
	auto ShowGround = [&](bool bShow) { Ground->SetVisibility(bShow); for (UInstancedStaticMeshComponent* G : Grids) G->SetVisibility(bShow); };

	// one capture: the view, the light's post process, optionally the scene depth with the ground hidden or the renderer hidden
	TArray<UPrimitiveComponent*> HideAlso; // 'rings-' assets: everything the renderer drew but its rings
	auto Capture = [&](const TSharedPtr<FJsonObject>& V, bool bDepth, bool bHideUnits, TArray<FColor>& OutColor, TArray<FLinearColor>& OutDepth) -> bool
	{
		const FVector Pos = FromReference(V->GetArrayField(TEXT("pos"))), Target = FromReference(V->GetArrayField(TEXT("target")));
		const double VFov = Num(V, TEXT("fov"), 30.0);
		const double HFov = FMath::RadiansToDegrees(2.0 * FMath::Atan(FMath::Tan(FMath::DegreesToRadians(VFov) / 2.0) * W / H));
		UTextureRenderTarget2D* Target2D = NewObject<UTextureRenderTarget2D>(Outer);
		// the look's post process writes display-encoded color (as the engine tonemapper does for the viewport): a plain
		// 8-bit target keeps those bytes; an sRGB one would encode them again. The before lights keep TCModelCompare's.
		// depth: RGBA32f (the Vulkan RHI reads linear color back from float RGBA, not from R32f)
		Target2D->RenderTargetFormat = bDepth ? RTF_RGBA32f : Mode == ELight::Look ? RTF_RGBA8 : RTF_RGBA8_SRGB;
		Target2D->ClearColor = bDepth ? FLinearColor(1e9f, 0, 0, 0) : Hex(Light, TEXT("haze"), 0xb8c4cc);
		Target2D->InitAutoFormat(W, H);
		Target2D->UpdateResourceImmediate(true);
		USceneCaptureComponent2D* Cap = NewObject<USceneCaptureComponent2D>(Outer);
		Cap->TextureTarget = Target2D;
		Cap->CaptureSource = bDepth ? SCS_SceneDepth : SCS_FinalColorLDR;
		Cap->bCaptureEveryFrame = false;
		Cap->bCaptureOnMovement = false;
		// a view state: UMaterialInterface::OverrideBlendableSettings drops post process materials from views without one
		Cap->bAlwaysPersistRenderingState = Mode == ELight::Look;
		Cap->FOVAngle = HFov;
		if (bDepth)
		{
			Cap->HideComponent(Ground);
			for (UInstancedStaticMeshComponent* G : Grids) Cap->HideComponent(G);
			// the owner and selection rings (their one material slot is 'Ring'): look.mjs's silhouette leaves them out too
			for (UActorComponent* C : Renderer->GetComponents())
				if (UStaticMeshComponent* Sm = Cast<UStaticMeshComponent>(C))
					if (Sm->GetStaticMesh() && Sm->GetStaticMesh()->GetStaticMaterials().Num() == 1 && Sm->GetStaticMesh()->GetStaticMaterials()[0].MaterialSlotName == TEXT("Ring"))
						Cap->HideComponent(Sm);
		}
		if (bHideUnits) Cap->HideActorComponents(Renderer);
		for (UPrimitiveComponent* C : HideAlso) Cap->HideComponent(C);
		if (Mode == ELight::Look)
		{
			TCLook::LockPostProcess(Cap->PostProcessSettings, PostMid, 0.f); // the reference views draw no AO pass
			const TSharedPtr<FJsonObject>* Fog;
			const bool bFog = V->TryGetObjectField(TEXT("fog"), Fog);
			TCLook::SetPost(PostMid, Mood.Exposure, bFog ? (float)Num(*Fog, TEXT("near"), 1e4) : 1e4f, bFog ? (float)Num(*Fog, TEXT("far"), 2e4) : 2e4f, Mood.Haze);
		}
		else
		{
			Cap->PostProcessSettings.bOverride_AutoExposureMethod = Mode == ELight::Before;
			Cap->PostProcessSettings.AutoExposureMethod = AEM_Manual;
			Cap->PostProcessSettings.bOverride_AutoExposureBias = Mode == ELight::Before;
			Cap->PostProcessSettings.AutoExposureBias = Num(Light, TEXT("exposureBias"), 1.0);
			Cap->PostProcessSettings.bOverride_AutoExposureApplyPhysicalCameraExposure = Mode == ELight::Before;
			Cap->PostProcessSettings.AutoExposureApplyPhysicalCameraExposure = false;
			Cap->PostProcessSettings.bOverride_ReflectionMethod = true;
			Cap->PostProcessSettings.ReflectionMethod = EReflectionMethod::ScreenSpace;
			Cap->PostProcessSettings.bOverride_DynamicGlobalIlluminationMethod = true;
			Cap->PostProcessSettings.DynamicGlobalIlluminationMethod = EDynamicGlobalIlluminationMethod::None;
		}
		Cap->PostProcessBlendWeight = 1.f;
		Cap->SetWorldLocationAndRotation(Pos, (Target - Pos).Rotation());
		Cap->RegisterComponentWithWorld(World);
		for (int32 F = 0; F < (Mode == ELight::GameBefore ? 16 : 4); F++) { World->SendAllEndOfFrameUpdates(); Cap->CaptureScene(); FlushRenderingCommands(); }
		FTextureRenderTargetResource* Resource = Target2D->GameThread_GetRenderTargetResource();
		const bool bOk = Resource && (bDepth ? Resource->ReadLinearColorPixels(OutDepth) && OutDepth.Num() == W * H : Resource->ReadPixels(OutColor) && OutColor.Num() == W * H);
		Cap->DestroyComponent();
		return bOk;
	};

	TArray<TSharedPtr<FJsonValue>> Reports;
	int32 Failures = 0;
	for (const TSharedPtr<FJsonValue>& AV : Doc->GetArrayField(TEXT("assets")))
	{
		const TSharedPtr<FJsonObject> A = AV->AsObject();
		const FString Name = A->GetStringField(TEXT("name")), Type = A->GetStringField(TEXT("type"));
		if (Filter.Num() && !Filter.Contains(Name)) continue;
		const int32 Fac = (int32)Num(A, TEXT("fac"), 0), Owner = (int32)Num(A, TEXT("owner"), 0), Alive = (int32)Num(A, TEXT("alive"), 0);
		const bool bCalib = Type == TEXT("calib"), bHorizon = Type == TEXT("horizon");
		// look.mjs 'rings-<type>': the owner ring and the selection ring of that unit alone
		const bool bRings = Type.StartsWith(TEXT("rings-")), bUnit = !bCalib && !bHorizon && !bRings;
		const FString UnitType = bRings ? Type.Mid(6) : Type;
		TSharedRef<FJsonObject> R = MakeShared<FJsonObject>();
		R->SetStringField(TEXT("name"), Name);
		TArray<UStaticMeshComponent*> Patches;
		FTCMatchState State;
		State.bHasMatch = true;
		if (bUnit || bRings)
		{
			State.Factions.Init(Fac, Owner + 1);
			FTCUnit U = MakeUnit(1, UnitType, Owner, 0.f, 0.f, Table);
			if (Alive == 1) U.Hp = 1; // main.js: alive = ceil(hp * men / (models * hpPer)) = 1
			State.Units.Add(1, U);
		}
		const TSet<int32> Selected = bRings ? TSet<int32>{ 1 } : TSet<int32>();
		Renderer->Reset();
		Renderer->SetViewPoint(FVector(0, 3000, 3000));
		Renderer->Draw(State, Table, nullptr, Selected, 0.f);
		if (bCalib)
		{
			ShowGround(false);
			UStaticMesh* Plane = LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Plane.Plane"));
			for (const TSharedPtr<FJsonValue>& PV : A->GetArrayField(TEXT("patches")))
			{
				const TSharedPtr<FJsonObject> P = PV->AsObject();
				const TArray<TSharedPtr<FJsonValue>>& Lin = P->GetArrayField(TEXT("linear"));
				const FLinearColor C((float)Lin[0]->AsNumber(), (float)Lin[1]->AsNumber(), (float)Lin[2]->AsNumber(), 1.f);
				UStaticMeshComponent* Patch = NewObject<UStaticMeshComponent>(Outer);
				Patch->SetStaticMesh(Plane);
				const double S = Num(P, TEXT("size"), 1.6) / 1.0; // the engine plane is 1 m square
				Patch->SetWorldTransform(FTransform(FRotationMatrix::MakeFromZ(FVector(1, 0, 0)).ToQuat(), FromReference(P->GetArrayField(TEXT("centre"))), FVector(S, S, 1)));
				Patch->SetCastShadow(false);
				// unlit, so the patch shows the tone curve alone (the engine's in the before lights, the look's here)
				if (UMaterialInstanceDynamic* M = Mid(TCLook::EmissivePath)) { M->SetVectorParameterValue(TEXT("Color"), C); Patch->SetMaterial(0, M); }
				Patch->RegisterComponentWithWorld(World);
				Patches.Add(Patch);
			}
		}
		FinishCompiling();
		Renderer->Draw(State, Table, nullptr, Selected, 0.f);
		HideAlso.Reset();
		if (bRings)
			for (UActorComponent* C : Renderer->GetComponents())
				if (UPrimitiveComponent* Pc = Cast<UPrimitiveComponent>(C))
				{
					const UStaticMeshComponent* Sm = Cast<UStaticMeshComponent>(Pc);
					const bool bRing = Sm && Sm->GetStaticMesh() && Sm->GetStaticMesh()->GetStaticMaterials().Num() == 1 && Sm->GetStaticMesh()->GetStaticMaterials()[0].MaterialSlotName == TEXT("Ring");
					if (!bRing) HideAlso.Add(Pc);
				}
		if (bNoUnitShadows) for (UActorComponent* C : Renderer->GetComponents()) if (UPrimitiveComponent* Pc = Cast<UPrimitiveComponent>(C)) Pc->SetCastShadow(false);
		if (bUnit)
		{
			R->SetStringField(TEXT("renderer"), Renderer->ModelStatusLine());
			R->SetNumberField(TEXT("modelUnits"), Renderer->ModelStats().ModelUnits);
			R->SetNumberField(TEXT("crowdMen"), Renderer->ModelStats().CrowdMen);
			if (Renderer->ModelStats().ModelUnits != 1) { Failures++; UE_LOG(LogTCLook, Error, TEXT("LOOK %s: not drawn with its model"), *Name); }
		}
		UE_LOG(LogTCLook, Display, TEXT("LOOK %s (%s fac %d owner %d%s) %s"), *Name, *Type, Fac, Owner, Alive == 1 ? TEXT(", one man") : TEXT(""), bUnit ? *Renderer->ModelStatusLine() : TEXT(""));
		TArray<TSharedPtr<FJsonValue>> Shots;
		for (const TSharedPtr<FJsonValue>& VV : A->GetArrayField(TEXT("views")))
		{
			const TSharedPtr<FJsonObject> V = VV->AsObject();
			const FString Label = V->GetStringField(TEXT("label"));
			TArray<FColor> Color;
			TArray<FLinearColor> Depth;
			const FString Png = FPaths::Combine(OutDir, FString::Printf(TEXT("%s-%s-unreal-%s.png"), *Name, *Label, *Suffix));
			if (Capture(V, false, false, Color, Depth) && SavePng(Png, W, H, Color)) Shots.Add(MakeShared<FJsonValueString>(Png));
			else { Failures++; UE_LOG(LogTCLook, Error, TEXT("LOOK %s %s: capture failed"), *Name, *Label); }
			if (bUnit && !bNoMasks)
			{
				if (Capture(V, true, false, Color, Depth))
				{
					TArray<FColor> Mask;
					Mask.SetNumUninitialized(W * H);
					int32 Count = 0;
					for (int32 I = 0; I < W * H; I++) { const bool bOn = Depth[I].R < 1e6f; Mask[I] = bOn ? FColor::White : FColor::Black; Count += bOn; }
					SavePng(FPaths::Combine(OutDir, FString::Printf(TEXT("%s-%s-mask-unreal.png"), *Name, *Label)), W, H, Mask);
					R->SetNumberField(Label + TEXT("MaskPixels"), Count);
				}
				else { Failures++; UE_LOG(LogTCLook, Error, TEXT("LOOK %s %s: depth capture failed"), *Name, *Label); }
			}
			if ((bUnit || bRings) && !bNoPlates)
			{
				if (Capture(V, false, true, Color, Depth)) SavePng(FPaths::Combine(OutDir, FString::Printf(TEXT("%s-%s-plate-unreal-%s.png"), *Name, *Label, *Suffix)), W, H, Color);
				else { Failures++; UE_LOG(LogTCLook, Error, TEXT("LOOK %s %s: plate capture failed"), *Name, *Label); }
			}
		}
		for (UStaticMeshComponent* P : Patches) P->DestroyComponent();
		ShowGround(true);
		R->SetArrayField(TEXT("shots"), Shots);
		Reports.Add(MakeShared<FJsonValueObject>(R));
	}
	TSharedRef<FJsonObject> Report = MakeShared<FJsonObject>();
	Report->SetStringField(TEXT("light"), Suffix);
	Report->SetStringField(TEXT("reference"), TCLook::ReferencePin);
	Report->SetArrayField(TEXT("assets"), Reports);
	Report->SetNumberField(TEXT("failures"), Failures);
	FString Json;
	FJsonSerializer::Serialize(Report, TJsonWriterFactory<>::Create(&Json));
	FFileHelper::SaveStringToFile(Json, *FPaths::Combine(OutDir, FString::Printf(TEXT("unreal-%s-report.json"), *Suffix)));
	UE_LOG(LogTCLook, Display, TEXT("LOOK %s %s: %d failures"), *Suffix, Failures ? TEXT("FAIL") : TEXT("PASS"), Failures);
	return Failures ? 1 : 0;
}

// ------------------------------------------------------------------------------------------------- the world's look

UTCWorldLookCommandlet::UTCWorldLookCommandlet()
{
	LogToConsole = true;
}

int32 UTCWorldLookCommandlet::Main(const FString& Params)
{
	using namespace TCLookCompareImpl;
	FString OutDir, ViewsArg = TEXT("67,119,45,0;81,121,45,0"), Suffix = TEXT("look");
	FParse::Value(*Params, TEXT("Out="), OutDir);
	FParse::Value(*Params, TEXT("Views="), ViewsArg, false);
	FParse::Value(*Params, TEXT("Suffix="), Suffix);
	float AoIntensity = 0.f;
	FParse::Value(*Params, TEXT("Ao="), AoIntensity);
	if (OutDir.IsEmpty()) { UE_LOG(LogTCLook, Error, TEXT("usage: -run=TCWorldLook -Out=<dir> [-Views=x,z,dist,yaw;...]")); return 1; }
	IFileManager::Get().MakeDirectory(*OutDir, true);
	FString Missing;
	if (!TCLook::AssetsPresent(Missing)) { UE_LOG(LogTCLook, Error, TEXT("look assets missing: %s"), *Missing); return 1; }

	// the scenery fixture's default.json case (World/TCScatter.cpp reads the same file)
	const FString FixturePath = FPaths::Combine(FPaths::ProjectDir(), TEXT("Tools/world/fixtures/scenery-oracle.json.gz"));
	TArray<uint8> Gz;
	if (!FFileHelper::LoadFileToArray(Gz, *FixturePath) || Gz.Num() < 18) { UE_LOG(LogTCLook, Error, TEXT("cannot read %s"), *FixturePath); return 1; }
	const uint32 Size = (uint32)Gz[Gz.Num() - 4] | (uint32)Gz[Gz.Num() - 3] << 8 | (uint32)Gz[Gz.Num() - 2] << 16 | (uint32)Gz[Gz.Num() - 1] << 24;
	TArray<uint8> Raw;
	Raw.SetNumUninitialized((int64)Size + 1);
	if (!FCompression::UncompressMemory(NAME_Gzip, Raw.GetData(), Size, Gz.GetData(), Gz.Num())) { UE_LOG(LogTCLook, Error, TEXT("cannot gunzip %s"), *FixturePath); return 1; }
	Raw[Size] = 0;
	TSharedPtr<FJsonObject> Fixture;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(FString(UTF8_TO_TCHAR((const ANSICHAR*)Raw.GetData()))), Fixture) || !Fixture.IsValid()) { UE_LOG(LogTCLook, Error, TEXT("cannot parse %s"), *FixturePath); return 1; }
	const TSharedPtr<FJsonObject> Input = Fixture->GetArrayField(TEXT("cases"))[0]->AsObject()->GetObjectField(TEXT("input"));
	TSharedRef<FJsonObject> Map = MakeShared<FJsonObject>();
	for (const TCHAR* F : { TEXT("w"), TEXT("h"), TEXT("rows"), TEXT("heights"), TEXT("spawns"), TEXT("points"), TEXT("buildings") })
		if (Input->HasField(F)) Map->SetField(F, Input->TryGetField(F));
	Map->SetStringField(TEXT("name"), TEXT("Three Crossroads"));
	TSharedRef<FJsonObject> Start = MakeShared<FJsonObject>();
	Start->SetStringField(TEXT("t"), TEXT("start"));
	Start->SetObjectField(TEXT("map"), Map);
	Start->SetNumberField(TEXT("you"), 0);
	Start->SetNumberField(TEXT("matchId"), 1);
	Start->SetArrayField(TEXT("cells"), {});

	FScratchWorld Scratch(TEXT("TCWorldLook"));
	UWorld* World = Scratch.World;
	UObject* Outer = World->GetWorldSettings();
	ATCTerrain* Terrain = World->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity);
	FTCMatchState State;
	if (!Terrain || !State.ApplyStart(*Start)) { UE_LOG(LogTCLook, Error, TEXT("the start does not apply")); return 1; }
	State.bFogLifted = true; // village.mjs draws no fog of war
	Terrain->ResetFrom(State);
	for (int32 I = 0; I < 64 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
	FTCWorldObjects Objects;
	if (!Objects.ApplyStart(*Start, State)) { UE_LOG(LogTCLook, Error, TEXT("world objects do not take the start")); return 1; }
	Objects.Update(*Terrain, nullptr);
	FTCWorldObjectsView View;
	if (!View.Attach(World)) { UE_LOG(LogTCLook, Error, TEXT("world view does not attach")); return 1; }
	for (int32 I = 0; I < 64 && (I == 0 || View.Queued()); I++) View.Draw(Objects, State, Terrain);
	UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK %d pieces, %d props shown; view: %s"), Objects.ShapeCount(), Objects.PropsShown(), *View.StatusLine());
#if WITH_EDITOR
	if (FParse::Param(*Params, TEXT("DumpFoliage")))
		if (UTexture2D* T = LoadObject<UTexture2D>(nullptr, TEXT("/Game/Models/Textures/Maps/T_Map_foliage.T_Map_foliage")))
		{
			TArray64<uint8> Mip;
			T->Source.GetMipData(Mip, 0);
			const int32 SW = T->Source.GetSizeX(), SH = T->Source.GetSizeY();
			UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK T_Map_foliage source %dx%d format %d, srgb %d, compression %d, platform %dx%d, bytes %lld"), SW, SH, (int32)T->Source.GetFormat(), T->SRGB ? 1 : 0, (int32)T->CompressionSettings, T->GetSizeX(), T->GetSizeY(), Mip.Num());
			const int32 Bpp = SW * SH > 0 ? (int32)(Mip.Num() / ((int64)SW * SH)) : 0;
			for (const FIntPoint Px : { FIntPoint(250, 120), FIntPoint(760, 250), FIntPoint(250, 770), FIntPoint(160, 760), FIntPoint(760, 800), FIntPoint(700, 900) })
				if (Bpp == 4 && Px.X < SW && Px.Y < SH)
				{
					const uint8* B = &Mip[((int64)Px.Y * SW + Px.X) * 4];
					UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK   source texel (%d, %d): B %d G %d R %d A %d"), Px.X, Px.Y, B[0], B[1], B[2], B[3]);
				}
		}
#endif
	if (FParse::Param(*Params, TEXT("DumpFoliage")))
		for (TObjectIterator<UInstancedStaticMeshComponent> It; It; ++It)
		{
			UInstancedStaticMeshComponent* C = *It;
			if (C->GetWorld() != World || !C->GetStaticMesh() || !C->GetInstanceCount()) continue;
			const FString Mesh = C->GetStaticMesh()->GetName();
			if (!Mesh.Contains(TEXT("foliage"))) continue;
			FString Data;
			for (int32 K = 0; K < FMath::Min(C->NumCustomDataFloats, 8) && K < C->PerInstanceSMCustomData.Num(); K++) Data += FString::Printf(TEXT(" %.3f"), C->PerInstanceSMCustomData[K]);
			FString Mats;
			for (int32 M = 0; M < C->GetNumMaterials(); M++) Mats += FString::Printf(TEXT(" [%d] %s"), M, C->GetMaterial(M) ? *C->GetMaterial(M)->GetPathName() : TEXT("none"));
			const FBox B = C->GetStaticMesh()->GetBoundingBox();
			FTransform First;
			C->GetInstanceTransform(0, First, true);
			UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK   first instance at reference (%.1f, %.1f)"), First.GetLocation().X / 100.0, First.GetLocation().Y / 100.0);
			UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK foliage %s: %d instances, %d floats, first:%s; bounds %s; uv channels %d; materials%s"), *Mesh, C->GetInstanceCount(), C->NumCustomDataFloats, *Data, *B.ToString(),
				// lead (slice 12): the render data's count; UStaticMesh::GetNumUVChannels is editor-only and broke the game link
				C->GetStaticMesh()->GetRenderData() && C->GetStaticMesh()->GetRenderData()->LODResources.Num() ? (int32)C->GetStaticMesh()->GetRenderData()->LODResources[0].GetNumTexCoords() : 0, *Mats);
#if WITH_EDITOR
			if (const FMeshDescription* MD = C->GetStaticMesh()->GetMeshDescription(0))
			{
				FStaticMeshConstAttributes A(*MD);
				const auto Cols = A.GetVertexInstanceColors();
				const auto Nors = A.GetVertexInstanceNormals();
				const auto Uvs = A.GetVertexInstanceUVs();
				for (const FPolygonGroupID G : MD->PolygonGroups().GetElementIDs())
				{
					FVector4f CMin(1e9f), CMax(-1e9f), CSum(0.f); double NMin = 1e9, NMax = 0, U3Min = 1e9, U3Max = -1e9; int32 N = 0;
					for (const FTriangleID T : MD->GetPolygonGroupTriangles(G))
						for (const FVertexInstanceID V : MD->GetTriangleVertexInstances(T))
						{
							const FVector4f Cv = Cols[V];
							for (int32 K = 0; K < 4; K++) { CMin[K] = FMath::Min(CMin[K], Cv[K]); CMax[K] = FMath::Max(CMax[K], Cv[K]); }
							CSum += Cv; N++;
							const double L = Nors[V].Size(); NMin = FMath::Min(NMin, L); NMax = FMath::Max(NMax, L);
							if (Uvs.GetNumChannels() > 3) { const double U = Uvs.Get(V, 3).X; U3Min = FMath::Min(U3Min, U); U3Max = FMath::Max(U3Max, U); }
						}
					if (N) UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK   group %d: %d corners, colour min (%.3f %.3f %.3f) max (%.3f %.3f %.3f) mean (%.3f %.3f %.3f); normal length %.3f..%.3f; uv3.x %.3f..%.3f"),
						G.GetValue(), N, CMin.X, CMin.Y, CMin.Z, CMax.X, CMax.Y, CMax.Z, CSum.X / N, CSum.Y / N, CSum.Z / N, NMin, NMax, U3Min, U3Max);
				}
			}
#endif
		}

	// the game's light, default mood, the sun aimed for yaw 0 (village.mjs SUN_DIR)
	const FTCMood& Mood = TCLook::DefaultMood();
	const float Env = TCLook::EnvironmentIntensity(Mood.SunI);
	UDirectionalLightComponent* SunLight = NewObject<UDirectionalLightComponent>(Outer);
	SunLight->SetMobility(EComponentMobility::Movable);
	SunLight->SetWorldRotation((-TCLook::ThreeToUnreal(TCLook::SunDirectionThree(Mood.SunUp, 0.f))).Rotation());
	SunLight->SetLightColor(Mood.Sun);
	SunLight->SetIntensity(Mood.SunI);
	SunLight->SetShadowAmount(Mood.Shadow);
	SunLight->SetAtmosphereSunLight(false);
	SunLight->SetDynamicShadowDistanceMovableLight(40000.f);
	SunLight->SetLightingChannels(true, false, false);
	SunLight->RegisterComponentWithWorld(World);
	TCLook::MakeUnitSun(Outer, World, SunLight);
	USkyLightComponent* Sky = NewObject<USkyLightComponent>(Outer);
	Sky->SetMobility(EComponentMobility::Movable);
	Sky->SourceType = SLS_SpecifiedCubemap;
	Sky->bLowerHemisphereIsBlack = false;
	Sky->Cubemap = TCLook::MakeSkyCube(Mood, Env, 0.f);
	Sky->SetIntensity(1.f);
	Sky->SetLightColor(FLinearColor::White);
	Sky->SetCastShadows(false);
	Sky->RegisterComponentWithWorld(World);
	TCLook::SetCollection(World, Mood, Env, 0.f, TCLook::IndirectInMaterials());
	FlushRenderingCommands();
	for (int32 Try = 0; Try < 5; Try++)
	{
		Sky->RecaptureSky();
		USkyLightComponent::UpdateSkyCaptureContents(World);
		FlushRenderingCommands();
		const FLinearColor Up = Dot(Sky->GetIrradianceEnvironmentMap(), FSHVector3::CalcDiffuseTransfer(FVector(0, 0, 1))) / PI;
		UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK sky light capture %d: diffuse up (%.4f, %.4f, %.4f)"), Try, Up.R, Up.G, Up.B);
		if (Up.G > 1e-4f) break;
	}
	UMaterialInterface* PostBase = LoadObject<UMaterialInterface>(nullptr, TCLook::PostPath);
	UMaterialInstanceDynamic* PostMid = PostBase ? UMaterialInstanceDynamic::Create(PostBase, Outer) : nullptr;
	// the viewer's grass: MeshLambertMaterial 0x6c6c45 over the whole board and 200 m round it
	UStaticMeshComponent* Ground = NewObject<UStaticMeshComponent>(Outer);
	Ground->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Plane.Plane")));
	const double MapW = Input->GetNumberField(TEXT("w")) * 2.0, MapH = Input->GetNumberField(TEXT("h")) * 2.0;
	Ground->SetWorldTransform(FTransform(FQuat::Identity, FVector(MapW * 50.0, MapH * 50.0, 0.0), FVector(MapW + 400.0, MapH + 400.0, 1.0)));
	if (UMaterialInterface* LB = LoadObject<UMaterialInterface>(nullptr, TCLook::LambertPath))
	{
		UMaterialInstanceDynamic* G = UMaterialInstanceDynamic::Create(LB, Outer);
		G->SetVectorParameterValue(TEXT("Color"), TCLook::HexLinear(0x6c6c45));
		Ground->SetMaterial(0, G);
	}
	Ground->RegisterComponentWithWorld(World);
	// diagnosis switches: -LeafScalar=Name:Value sets a scalar on the world leaf material instances
	FString LeafScalar;
	if (FParse::Value(*Params, TEXT("LeafScalar="), LeafScalar))
	{
		FString Name, Value;
		if (LeafScalar.Split(TEXT(":"), &Name, &Value))
			for (TObjectIterator<UMaterialInstanceDynamic> It; It; ++It)
				if (It->GetName().Contains(TEXT("leafMaterial"))) { It->SetScalarParameterValue(FName(*Name), FCString::Atof(*Value)); UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK %s: %s = %s"), *It->GetName(), *Name, *Value); }
	}
	FinishCompiling();
	for (int32 I = 0; I < 4; I++) View.Draw(Objects, State, Terrain);

	const int32 W = 1280, H = 720;
	const double Pitch = 0.95, VFov = 42.0;
	const double HFov = FMath::RadiansToDegrees(2.0 * FMath::Atan(FMath::Tan(FMath::DegreesToRadians(VFov) / 2.0) * W / H));
	TArray<FString> Views;
	ViewsArg.ParseIntoArray(Views, TEXT(";"));
	int32 Failures = 0;
	for (const FString& VS : Views)
	{
		TArray<FString> P;
		VS.ParseIntoArray(P, TEXT(","));
		if (P.Num() != 4) { Failures++; continue; }
		const double X = FCString::Atod(*P[0]), Z = FCString::Atod(*P[1]), Dist = FCString::Atod(*P[2]), Yaw = FCString::Atod(*P[3]);
		// client/camera.js: the camera at focus + (sin yaw, ., cos yaw) times the horizontal distance, up dist x sin pitch
		const double Hd = Dist * FMath::Cos(Pitch);
		const FVector Eye = TCLook::ThreeToUnreal(FVector(X + FMath::Sin(Yaw) * Hd, Dist * FMath::Sin(Pitch), Z + FMath::Cos(Yaw) * Hd)) * 100.0;
		const FVector Target = TCLook::ThreeToUnreal(FVector(X, 0, Z)) * 100.0;
		UTextureRenderTarget2D* Target2D = NewObject<UTextureRenderTarget2D>(Outer);
		Target2D->RenderTargetFormat = RTF_RGBA8;
		Target2D->ClearColor = TCLook::ToDisplay(Mood.Haze);
		Target2D->InitAutoFormat(W, H);
		Target2D->UpdateResourceImmediate(true);
		USceneCaptureComponent2D* Cap = NewObject<USceneCaptureComponent2D>(Outer);
		Cap->TextureTarget = Target2D;
		Cap->CaptureSource = SCS_FinalColorLDR;
		Cap->bCaptureEveryFrame = false;
		Cap->bCaptureOnMovement = false;
		Cap->bAlwaysPersistRenderingState = true;
		Cap->FOVAngle = HFov;
		Cap->HideActorComponents(Terrain);
		TCLook::LockPostProcess(Cap->PostProcessSettings, PostMid, AoIntensity);
		TCLook::SetPost(PostMid, Mood.Exposure, 50.f, 300.f, Mood.Haze); // setupLight's Fog(.., 50, 300): no followView there
		Cap->PostProcessBlendWeight = 1.f;
		Cap->SetWorldLocationAndRotation(Eye, (Target - Eye).Rotation());
		Cap->RegisterComponentWithWorld(World);
		for (int32 F = 0; F < 6; F++) { World->SendAllEndOfFrameUpdates(); Cap->CaptureScene(); FlushRenderingCommands(); }
		TArray<FColor> Color;
		FTextureRenderTargetResource* Res = Target2D->GameThread_GetRenderTargetResource();
		const FString Png = FPaths::Combine(OutDir, FString::Printf(TEXT("default-%s-%s-%s-unreal-%s.png"), *P[0], *P[1], *P[2], *Suffix));
		if (Res && Res->ReadPixels(Color) && Color.Num() == W * H && SavePng(Png, W, H, Color)) { UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK wrote %s"), *Png); }
		else { Failures++; UE_LOG(LogTCLook, Error, TEXT("WORLDLOOK %s: capture failed"), *VS); }
		Cap->DestroyComponent();
	}
	View.Detach();
	UE_LOG(LogTCLook, Display, TEXT("WORLDLOOK %s: %d failures"), Failures ? TEXT("FAIL") : TEXT("PASS"), Failures);
	return Failures ? 1 : 0;
}

// ------------------------------------------------------------------------------------------------- tests

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

// The mood table, the weather looks, the sun's aim, the tone curve and the sky light's cube against the pinned
// reference (numbers from client/moods.js, client/light.js and three r186 as rendered by unreal/Tools/look).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLookReferenceTest, "ThreeCrossroads.Look.Reference", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCLookReferenceTest::RunTest(const FString& Parameters)
{
	using namespace TCLook;
	FPostProcessSettings Settings;
	LockPostProcess(Settings, nullptr, 0.85f);
	TestTrue(TEXT("custom indirect light is protected from static material AO"),
		Settings.bOverride_AmbientOcclusionStaticFraction && Settings.AmbientOcclusionStaticFraction == 0.f);
	TestEqual(TEXT("High screen ambient occlusion remains enabled"), Settings.AmbientOcclusionIntensity, 0.85f);
	auto Rgb = [](const FLinearColor& A, const FLinearColor& B, float Tol) { return FMath::Abs(A.R - B.R) <= Tol && FMath::Abs(A.G - B.G) <= Tol && FMath::Abs(A.B - B.B) <= Tol; };
	TestEqual(TEXT("six moods"), Moods().Num(), 6);
	TestEqual(TEXT("day is the default"), DefaultMood().Name, FName(TEXT("day")));
	const FTCMood& Day = DefaultMood();
	TestEqual(TEXT("day sun"), Day.SunHex, 0xffe7c9u);
	TestTrue(TEXT("day sun 3.2, hemisphere 1.25, exposure 0.9"), FMath::IsNearlyEqual(Day.SunI, 3.2f) && FMath::IsNearlyEqual(Day.HemiI, 1.25f) && FMath::IsNearlyEqual(Day.Exposure, 0.9f));
	// three Color.setHex(0xc3cddd): (0.545724, 0.610496, 0.723055)
	TestTrue(TEXT("hemisphere top in linear"), Day.Top.Equals(FLinearColor(0.5457244f, 0.6104956f, 0.7230551f, 1.f), 1e-5f));
	TestEqual(TEXT("getHex round trip"), LinearHex(HexLinear(0x8a806b)), 0x8a806bu);
	// look.mjs cameras.json light.sunDir: light.js aimSun(0) with the day's 42 degrees
	TestTrue(TEXT("sun direction"), SunDirectionThree(42.f, 0.f).Equals(FVector(0.6435822975543765, 0.6691306063588582, -0.3715724127386973), 1e-6));
	TestTrue(TEXT("environment intensity 0.38 at full sun"), FMath::IsNearlyEqual(EnvironmentIntensity(3.2f), 0.38f, 1e-6f));
	TestTrue(TEXT("environment intensity with an overcast sun"), FMath::IsNearlyEqual(EnvironmentIntensity(1.9f), 0.38f * (0.7f + 0.3f * 1.9f / 3.2f), 1e-6f));
	// shared/weather.js mapMood and client/moods.js moodFor
	TestEqual(TEXT("Ardennes snows"), MapMood(TEXT("Ardennes Forest"), TEXT("")), FName(TEXT("snow")));
	TestEqual(TEXT("Three Crossroads is warm"), MapMood(TEXT("Three Crossroads"), TEXT("crossroads")), FName(TEXT("warm")));
	TestEqual(TEXT("warm has no row: day"), MoodFor(TEXT("Three Crossroads"), TEXT(""), TEXT("clear")), FName(TEXT("day")));
	TestEqual(TEXT("rain is overcast"), MoodFor(TEXT("Three Crossroads"), TEXT(""), TEXT("rain")), FName(TEXT("overcast")));
	TestEqual(TEXT("a desert map"), MoodFor(TEXT("Tobruk"), TEXT(""), TEXT("clear")), FName(TEXT("dust")));
	TestEqual(TEXT("snow weather"), MoodFor(TEXT("Tobruk"), TEXT(""), TEXT("snow")), FName(TEXT("snow")));
	TestEqual(TEXT("authored mood precedes map-name inference"), MoodFor(TEXT("Seelow Heights"), TEXT("seelow-heights"), TEXT("fog"), NAME_None, TEXT("dawn")), FName(TEXT("dawn")));
	TestEqual(TEXT("snow precedes authored mood"), MoodFor(TEXT("Seelow Heights"), TEXT(""), TEXT("snow"), NAME_None, TEXT("dawn")), FName(TEXT("snow")));
	TestEqual(TEXT("rain precedes authored mood"), MoodFor(TEXT("Seelow Heights"), TEXT(""), TEXT("rain"), NAME_None, TEXT("dawn")), FName(TEXT("overcast")));
	TestEqual(TEXT("explicit URL override precedes weather"), MoodFor(TEXT("Seelow Heights"), TEXT(""), TEXT("snow"), TEXT("golden"), TEXT("dawn")), FName(TEXT("golden")));
	TestEqual(TEXT("unknown authored mood falls back to map name"), MoodFor(TEXT("Tobruk"), TEXT(""), TEXT("clear"), NAME_None, TEXT("unknown")), FName(TEXT("dust")));
	TestEqual(TEXT("source mood keys are case sensitive"), MoodFor(TEXT("Seelow Heights"), TEXT(""), TEXT("clear"), NAME_None, TEXT("DAWN")), FName(TEXT("day")));
	// atmosphere.js blend and applyMood in fog
	FTCWeatherLook Goal;
	FLinearColor Haze;
	WeatherGoal(Day, TEXT("fog"), 0.f, 0.f, Goal, Haze);
	const FTCMood Fogged = Applied(Day, Goal, Haze);
	TestTrue(TEXT("fog: sun x0.62, shadow x0.45, haze x2.4"), FMath::IsNearlyEqual(Fogged.SunI, 3.2f * 0.62f, 1e-5f) && FMath::IsNearlyEqual(Fogged.Shadow, 0.95f * 0.45f, 1e-5f) && FMath::IsNearlyEqual(Fogged.HazeK, 2.4f, 1e-5f));
	TestEqual(TEXT("fog haze: day haze 3/4 toward 0xc9cbc6"), Fogged.HazeHex, LinearHex(FMath::Lerp(HexLinear(0xb9bdbc), HexLinear(0xc9cbc6), 0.75f)));
	// the tone curve: ACES filmic at the day's exposure 0.9, then sRGB, against three's own output (calib patches)
	TestTrue(TEXT("black stays black"), Rgb(ToneMapped(FLinearColor::Black, 0.9f), FLinearColor::Black, 1e-4f));
	const float Mid = ToneMapped(FLinearColor(0.18f, 0.18f, 0.18f), 0.9f).R;
	TestTrue(FString::Printf(TEXT("0.18 grey maps to %.4f, in (0.45, 0.6)"), Mid), Mid > 0.45f && Mid < 0.6f);
	// the hemisphere radiance: straight up its irradiance is the sky color (times intensity), straight down the ground's
	FTCMood NoEnv = Day;
	const FLinearColor Up = SkyRadiance(FVector(0, 1, 0), NoEnv, 0.f, 0.f), Down = SkyRadiance(FVector(0, -1, 0), NoEnv, 0.f, 0.f);
	// E(up) = pi * a + 2pi/3 * c with L = a + c y: (Up + Down)/2 = a, (Up - Down)/2 = c
	const FLinearColor A = (Up + Down) * 0.5f, C = (Up - Down) * 0.5f;
	const FLinearColor EUp = A * PI + C * (2.f * PI / 3.f), EDown = A * PI - C * (2.f * PI / 3.f);
	TestTrue(TEXT("hemisphere irradiance up = top x 1.25"), Rgb(EUp, Day.Top * 1.25f, 1e-4f));
	TestTrue(TEXT("hemisphere irradiance down = bottom x 1.25"), Rgb(EDown, Day.Bottom * 1.25f, 1e-4f));
	// the calibration patches three r186 rendered (unreal/Tools/look calib, sRGB bytes at exposure 0.9)
	const float Linear[] = { 0.01f, 0.02f, 0.05f, 0.1f, 0.18f, 0.3f, 0.5f, 0.75f, 1.f, 1.5f, 2.5f, 4.f, 8.f };
	const int32 Bytes[] = { 7, 18, 45, 80, 120, 157, 191, 212, 223, 234, 244, 249, 253 };
	for (int32 I = 0; I < 13; I++)
	{
		const int32 Got = FMath::RoundToInt(ToneMapped(FLinearColor(Linear[I], Linear[I], Linear[I]), 0.9f).R * 255.f);
		TestTrue(FString::Printf(TEXT("tone curve at %g: %d, three %d"), Linear[I], Got, Bytes[I]), FMath::Abs(Got - Bytes[I]) <= 1);
	}
	// the cube face table: face centres point along +X, -X, +Y, -Y, +Z, -Z (Unreal axes, as the sky light samples it)
	const FVector Faces[6] = { FVector(1, 0, 0), FVector(-1, 0, 0), FVector(0, 1, 0), FVector(0, -1, 0), FVector(0, 0, 1), FVector(0, 0, -1) };
	for (int32 F = 0; F < 6; F++)
	{
		const FVector D = (CubeTexelDirection(F, 31, 31, 64) + CubeTexelDirection(F, 32, 32, 64)).GetSafeNormal();
		TestTrue(FString::Printf(TEXT("cube face %d centre"), F), D.Equals(Faces[F], 1e-6));
	}
	// light.js outdoorEnvironment: below the horizon the ground color (0.11, 0.092, 0.064)
	TestTrue(TEXT("environment below the horizon"), Rgb(OutdoorEnvironment(FVector(0, -1, 0)), FLinearColor(0.11f, 0.092f, 0.064f), 1e-5f));
	return true;
}
// Round 14: the haze per mood and weather against a Node run of the pinned client/moods.js (MOODS, WEATHER_LOOK) with
// atmosphere.js blend() and applyMood() and light.js followView() restated (haze-oracle.mjs in the round 14 edge
// evidence, with three r186's own Color). Per row: hazeK, the haze hex light.js receives, the fog at a 140 m camera
// distance and the haze as the canvas shows it on Graphics High, where the composer tone-maps it (0-255). The live
// browser read snow mood, snow weather at 140 m as near 48, far 304, #c7cdd2.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLookHazeTest, "ThreeCrossroads.Look.Haze", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCLookHazeTest::RunTest(const FString& Parameters)
{
	using namespace TCLook;
	struct FRow { const TCHAR* Mood; const TCHAR* Weather; float HazeK; uint32 Hex; float Exposure; float Near140, Far140; float High[3]; };
	static const FRow Rows[] = {
		{ TEXT("day"), TEXT("clear"), 1.00000000f, 0xb9bdbc, 0.900f, 84.0000000f, 532.000000f, { 189.3812f, 191.8648f, 191.2456f } },
		{ TEXT("day"), TEXT("fog"), 2.40000000f, 0xc5c8c4, 0.900f, 35.0000000f, 221.666667f, { 197.3063f, 198.9314f, 196.7426f } },
		{ TEXT("day"), TEXT("rain"), 1.60000000f, 0xa3a9aa, 0.900f, 52.5000000f, 332.500000f, { 171.6596f, 176.3753f, 177.1396f } },
		{ TEXT("day"), TEXT("mud"), 1.15000000f, 0xb3b4b0, 0.900f, 73.0434783f, 462.608696f, { 184.6988f, 185.3802f, 182.6050f } },
		{ TEXT("day"), TEXT("snow"), 1.25000000f, 0xc7cbcd, 0.900f, 67.2000000f, 425.600000f, { 198.6231f, 200.7123f, 201.7335f } },
		{ TEXT("golden"), TEXT("clear"), 1.15000000f, 0xcdb08f, 0.950f, 73.0434783f, 462.608696f, { 204.8254f, 186.2353f, 160.1474f } },
		{ TEXT("golden"), TEXT("fog"), 2.76000000f, 0xcac5ba, 0.950f, 30.4347826f, 192.753623f, { 202.7634f, 200.0889f, 193.8834f } },
		{ TEXT("golden"), TEXT("rain"), 1.84000000f, 0xaea295, 0.950f, 45.6521739f, 289.130435f, { 183.5612f, 174.0632f, 162.9266f } },
		{ TEXT("golden"), TEXT("mud"), 1.32250000f, 0xc1aa8f, 0.950f, 63.5160681f, 402.268431f, { 197.4069f, 181.2726f, 158.9534f } },
		{ TEXT("golden"), TEXT("snow"), 1.43750000f, 0xd4c4b5, 0.950f, 58.4347826f, 370.086957f, { 207.9689f, 199.6668f, 190.9589f } },
		{ TEXT("dawn"), TEXT("clear"), 1.35000000f, 0xcdc9c1, 1.000f, 62.2222222f, 394.074074f, { 206.9534f, 204.9960f, 200.9147f } },
		{ TEXT("dawn"), TEXT("fog"), 3.24000000f, 0xcacbc5, 1.000f, 25.9259259f, 164.197531f, { 205.5480f, 206.0234f, 203.0632f } },
		{ TEXT("dawn"), TEXT("rain"), 2.16000000f, 0xaeafad, 1.000f, 38.8888889f, 246.296296f, { 187.3077f, 187.9930f, 186.6125f } },
		{ TEXT("dawn"), TEXT("mud"), 1.55250000f, 0xc1bdb4, 1.000f, 54.1062802f, 342.673108f, { 200.1537f, 197.8649f, 192.4846f } },
		{ TEXT("dawn"), TEXT("snow"), 1.68750000f, 0xd4d3d0, 1.000f, 49.7777778f, 315.259259f, { 210.5599f, 210.1279f, 208.8146f } },
		{ TEXT("overcast"), TEXT("clear"), 1.25000000f, 0xa8aba6, 1.000f, 67.2000000f, 425.600000f, { 182.6284f, 184.8046f, 181.1252f } },
		{ TEXT("overcast"), TEXT("fog"), 3.00000000f, 0xc1c4bf, 1.000f, 28.0000000f, 177.333333f, { 200.4872f, 202.0760f, 199.3857f } },
		{ TEXT("overcast"), TEXT("rain"), 2.00000000f, 0x9a9fa0, 1.000f, 42.0000000f, 266.000000f, { 170.2351f, 174.4318f, 175.2515f } },
		{ TEXT("overcast"), TEXT("mud"), 1.43750000f, 0xa6a7a0, 1.000f, 58.4347826f, 370.086957f, { 180.7878f, 181.5346f, 176.0911f } },
		{ TEXT("overcast"), TEXT("snow"), 1.56250000f, 0xbec1c1, 1.000f, 53.7600000f, 340.480000f, { 198.6525f, 200.3023f, 200.3002f } },
		{ TEXT("snow"), TEXT("clear"), 1.40000000f, 0xb9bfc5, 0.950f, 60.0000000f, 380.000000f, { 192.7557f, 196.2693f, 199.6372f } },
		{ TEXT("snow"), TEXT("fog"), 3.36000000f, 0xc5c8c6, 0.950f, 25.0000000f, 158.333333f, { 200.2139f, 201.7796f, 200.7342f } },
		{ TEXT("snow"), TEXT("rain"), 2.24000000f, 0xa3aaaf, 0.950f, 37.5000000f, 237.500000f, { 175.4385f, 180.7475f, 184.3898f } },
		{ TEXT("snow"), TEXT("mud"), 1.61000000f, 0xb3b5b7, 0.950f, 52.1739130f, 330.434783f, { 188.1054f, 189.4177f, 190.7111f } },
		{ TEXT("snow"), TEXT("snow"), 1.75000000f, 0xc7cdd2, 0.950f, 48.0000000f, 304.000000f, { 201.6250f, 204.5760f, 206.9391f } },
		{ TEXT("dust"), TEXT("clear"), 1.30000000f, 0xcdb895, 0.950f, 64.6153846f, 409.230769f, { 204.6037f, 191.8860f, 166.3150f } },
		{ TEXT("dust"), TEXT("fog"), 3.12000000f, 0xcac6bb, 0.950f, 26.9230769f, 170.512821f, { 202.7950f, 200.6694f, 194.5386f } },
		{ TEXT("dust"), TEXT("rain"), 2.08000000f, 0xaea698, 0.950f, 40.3846154f, 255.769231f, { 183.6831f, 177.5210f, 165.9752f } },
		{ TEXT("dust"), TEXT("mud"), 1.49500000f, 0xc1b094, 0.950f, 56.1872910f, 355.852843f, { 197.3361f, 185.9076f, 164.1046f } },
		{ TEXT("dust"), TEXT("snow"), 1.62500000f, 0xd4c8b8, 0.950f, 51.6923077f, 327.384615f, { 207.9842f, 201.9497f, 193.0642f } },
	};
	int32 Off = 0;
	for (const FRow& R : Rows)
	{
		const FTCMood* M = FindMood(FName(R.Mood));
		if (!M) { AddError(FString::Printf(TEXT("no mood %s"), R.Mood)); Off++; continue; }
		FTCWeatherLook Goal;
		FLinearColor Haze;
		WeatherGoal(*M, R.Weather, 0.f, 0.f, Goal, Haze);
		const FTCMood A = Applied(*M, Goal, Haze);
		const float Near = 140.f * 0.6f / A.HazeK, Far = 140.f * 3.8f / A.HazeK;
		const FLinearColor S = HazeOnScreen(A.Haze, A.Exposure, true);
		const bool bOk = FMath::IsNearlyEqual(A.HazeK, R.HazeK, 1e-5f) && A.HazeHex == R.Hex && FMath::IsNearlyEqual(A.Exposure, R.Exposure, 1e-6f)
			&& FMath::IsNearlyEqual(Near, R.Near140, 1e-3f) && FMath::IsNearlyEqual(Far, R.Far140, 1e-3f)
			&& FMath::Abs(S.R - R.High[0]) < 0.05f && FMath::Abs(S.G - R.High[1]) < 0.05f && FMath::Abs(S.B - R.High[2]) < 0.05f;
		if (!bOk)
		{
			Off++;
			AddError(FString::Printf(TEXT("%s/%s: hazeK %.5f (%.5f), hex %06x (%06x), fog %.2f-%.2f (%.2f-%.2f), on screen %.2f,%.2f,%.2f (%.2f,%.2f,%.2f)"),
				R.Mood, R.Weather, A.HazeK, R.HazeK, A.HazeHex, R.Hex, Near, Far, R.Near140, R.Far140, S.R, S.G, S.B, R.High[0], R.High[1], R.High[2]));
		}
		// Low (the direct path): the haze's own bytes
		const FLinearColor L = HazeOnScreen(A.Haze, A.Exposure, false);
		if (FMath::Abs(L.R - (float)((R.Hex >> 16) & 255)) > 0.51f || FMath::Abs(L.G - (float)((R.Hex >> 8) & 255)) > 0.51f || FMath::Abs(L.B - (float)(R.Hex & 255)) > 0.51f)
		{
			Off++;
			AddError(FString::Printf(TEXT("%s/%s: Low haze %.2f,%.2f,%.2f"), R.Mood, R.Weather, L.R, L.G, L.B));
		}
	}
	TestEqual(TEXT("mood and weather rows off the reference"), Off, 0);
	TestEqual(TEXT("rows"), (int32)UE_ARRAY_COUNT(Rows), 30);
	return true;
}


IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLookBillboardFogTest, "ThreeCrossroads.Look.BillboardFog", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCLookBillboardFogTest::RunTest(const FString&)
{
    using namespace TCLook;
    const FLinearColor Haze = HexLinear(0xc9cbc6);
    FLinearColor Ink = HexLinear(0x16191b); Ink.A = .88f;
    for (const bool High : {false,true})
    {
        const auto Near = BillboardColour(Ink, 20, 20, 100, Haze, .9f, High);
        const auto Far = BillboardColour(Ink, 100, 20, 100, Haze, .9f, High);
        TestTrue(TEXT("near sprite keeps source tone mapping"), Near.Equals(ACESFilmic(Ink,.9f), 2e-5f));
        FLinearColor Expected = High ? ACESFilmic(Haze,.9f) : Haze; Expected.A = Ink.A;
        TestTrue(TEXT("distant sprite reaches the same haze as the world"), Far.Equals(Expected,2e-5f));
        TestEqual(TEXT("source fog changes colour, not opacity"), Far.A,.88f);
    }
    const auto High = ToDisplay(BillboardColour(Ink,60,20,100,Haze,.9f,true));
    const auto Low = ToDisplay(BillboardColour(Ink,60,20,100,Haze,.9f,false));
    const auto WantHigh = ToneMapped(FMath::Lerp(Ink,Haze,.5f),.9f);
    const auto WantLow = FMath::Lerp(ToneMapped(Ink,.9f),ToDisplay(Haze),.5f);
    TestTrue(TEXT("High fog mixes in linear light before OutputPass"), FVector(High.R,High.G,High.B).Equals(FVector(WantHigh.R,WantHigh.G,WantHigh.B),2e-5));
    TestTrue(TEXT("Low fog mixes after tone mapping and output transfer"), FVector(Low.R,Low.G,Low.B).Equals(FVector(WantLow.R,WantLow.G,WantLow.B),2e-5));
    TestTrue(TEXT("High and Low do not collapse to an alpha fade"), FMath::Abs(High.R-Low.R)>.1f);
    return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLookShadowViewTest, "ThreeCrossroads.Look.ShadowView", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCLookShadowViewTest::RunTest(const FString&)
{
    const double S = FMath::Sin(.95), C = FMath::Cos(.95), TanY = FMath::Tan(21.0*PI/180), TanX = TanY*16/9;
    for (const auto D : {25.0,40.0,140.0})
    {
        const auto V = TCLook::ShadowView({0,D*C,D*S}, {0,-C,-S}, {1,0,0}, {0,-S,C}, TanX,TanY,0,2048,4,true);
        const double Expected = D == 25 ? 40 : D == 40 ? 56 : 160;
        TestEqual(TEXT("source followView pad and radius quantization"), V.Radius, Expected);
        TestEqual(TEXT("source texel is square width over map size"), V.Texel, 2*Expected/2048);
        TestTrue(TEXT("native cascade includes the ground footprint"), V.FarDepth>D && V.CascadeRadius>0 && V.Resolution>=32);
        const auto Hard = TCLook::ShadowView({0,D*C,D*S}, {0,-C,-S}, {1,0,0}, {0,-S,C}, TanX,TanY,0,1024,1,false);
        TestEqual(TEXT("Low and day High use the same world footprint with Basic versus PCF filter"), Hard.Resolution,V.Resolution);
        const auto Soft = TCLook::ShadowView({0,D*C,D*S}, {0,-C,-S}, {1,0,0}, {0,-S,C}, TanX,TanY,0,2048,7,true);
        TestTrue(TEXT("overcast soft 2.8 widens the filter without changing source strength"), Soft.Resolution<V.Resolution);
    }
    return true;
}

#endif // WITH_DEV_AUTOMATION_TESTS
