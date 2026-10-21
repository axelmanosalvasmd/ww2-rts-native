// The atmosphere (client/atmosphere.js, client/wind.js). The port's numbers are in TCAtmosCore (TCAtmosphere.h); this
// file draws them with the materials of unreal/Tools/atmosphere/make-atmosphere-assets.py.
#include "World/TCAtmosphere.h"
#include "Client/TCRenderer34.h"
#include "World/TCTerrain.h"
#include "World/TCRailways.h"
#include "World/TCFogFrontier.h"
#include "Net/TCMatchState.h"
#include "Client/TCCameraPawn.h"
#include "Client/TCPlayerController.h"
#include "Client/TCGraphics.h"
#include "ThreeCrossroads.h"
#include "ProceduralMeshComponent.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/Material.h"
#include "Materials/MaterialInterface.h"
#include "Engine/Texture2D.h"
#include "Engine/GameViewportClient.h"
#include "Engine/Engine.h"
#include "GameFramework/HUD.h"
#include "Camera/PlayerCameraManager.h"
#include "Engine/World.h"
#include "EngineUtils.h"
#include "GameFramework/Actor.h"
#include "UObject/Package.h"
#include "UnrealClient.h"
#include "HAL/PlatformTime.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "CoreGlobals.h"

namespace TCAtmosImpl
{
	using namespace TCAtmosCore;

	// A transient RGBA8 texture with a full box-filtered mip chain (three generates mipmaps for these) or one level.
	UTexture2D* MakeTexture(UObject* Outer, const TCHAR* Name, int32 W, int32 H, TArray<FColor> Level, bool bMips, bool bWrap)
	{
		// A second view or a restarted match can still hold the previous transient texture.
		UTexture2D* T = NewObject<UTexture2D>(Outer, MakeUniqueObjectName(Outer, UTexture2D::StaticClass(), FName(Name)), RF_Transient);
		FTexturePlatformData* PD = new FTexturePlatformData();
		PD->SizeX = W; PD->SizeY = H; PD->PixelFormat = PF_B8G8R8A8;
		int32 LW = W, LH = H;
		for (;;)
		{
			FTexture2DMipMap* Mip = new FTexture2DMipMap(LW, LH, 1);
			PD->Mips.Add(Mip);
			Mip->BulkData.Lock(LOCK_READ_WRITE);
			void* Data = Mip->BulkData.Realloc((int64)LW * LH * 4);
			FMemory::Memcpy(Data, Level.GetData(), (int64)LW * LH * 4);
			Mip->BulkData.Unlock();
			if (!bMips || (LW == 1 && LH == 1)) break;
			const int32 NW = FMath::Max(1, LW / 2), NH = FMath::Max(1, LH / 2);
			TArray<FColor> Next;
			Next.SetNumUninitialized(NW * NH);
			for (int32 Y = 0; Y < NH; Y++)
				for (int32 X = 0; X < NW; X++)
				{
					int32 S[4] = { 0, 0, 0, 0 }, N = 0;
					for (int32 DY = 0; DY < 2; DY++) for (int32 DX = 0; DX < 2; DX++)
					{
						const int32 SX = FMath::Min(X * 2 + DX, LW - 1), SY = FMath::Min(Y * 2 + DY, LH - 1);
						const FColor& C = Level[SY * LW + SX];
						S[0] += C.R; S[1] += C.G; S[2] += C.B; S[3] += C.A; N++;
					}
					Next[Y * NW + X] = FColor((uint8)((S[0] + N / 2) / N), (uint8)((S[1] + N / 2) / N), (uint8)((S[2] + N / 2) / N), (uint8)((S[3] + N / 2) / N));
				}
			Level = MoveTemp(Next); LW = NW; LH = NH;
		}
		T->SetPlatformData(PD);
		T->SRGB = false; // linear data (noise, masks, alpha)
		T->Filter = bMips ? TF_Trilinear : TF_Bilinear;
		T->AddressX = T->AddressY = bWrap ? TA_Wrap : TA_Clamp;
		T->NeverStream = true;
		T->LODGroup = TEXTUREGROUP_Pixels2D; // no streaming bias: every level is resident
		T->UpdateResource();
		return T;
	}

	FVector ToUe(double X, double Y, double Z) { return FVector(X * 100.0, Z * 100.0, Y * 100.0); }

	// three.js Object3D: position, Euler (x, y, z, 'YXZ'), uniform scale, applied to a point in three's axes
	FVector3d Pose(const FVector3d& P, double Yaw, double Roll, double Scale, const FVector3d& Pos)
	{
		// R = Ry(yaw) * Rx(0) * Rz(roll)
		const double CZ = FMath::Cos(Roll), SZ = FMath::Sin(Roll), CY = FMath::Cos(Yaw), SY = FMath::Sin(Yaw);
		const double X1 = P.X * CZ - P.Y * SZ, Y1 = P.X * SZ + P.Y * CZ, Z1 = P.Z;
		return FVector3d((X1 * CY + Z1 * SY) * Scale + Pos.X, Y1 * Scale + Pos.Y, (-X1 * SY + Z1 * CY) * Scale + Pos.Z);
	}

	// atmosphere.js fallKind for the native strings
	FString FallFor(const FString& Wx, float Rain, FName MoodWeather)
	{
		const std::string Kind = FallKind(std::string(TCHAR_TO_UTF8(*Wx)), Rain, MoodWeather.IsNone() ? std::string() : std::string(TCHAR_TO_UTF8(*MoodWeather.ToString())));
		return FString(UTF8_TO_TCHAR(Kind.c_str()));
	}

	bool KnownLook(const FString& Kind) { return Kind == TEXT("clear") || Kind == TEXT("fog") || Kind == TEXT("rain") || Kind == TEXT("mud") || Kind == TEXT("snow"); }
}

bool FTCAtmosphere::Attach(UWorld* World)
{
	if (Actor.IsValid()) return true;
	if (!World) return false;
	FActorSpawnParameters Params;
	Params.ObjectFlags |= RF_Transient;
	AActor* A = World->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity, Params);
	if (!A) return false;
	USceneComponent* Root = NewObject<USceneComponent>(A, TEXT("Atmosphere"));
	Root->SetMobility(EComponentMobility::Movable);
	A->SetRootComponent(Root);
	A->AddInstanceComponent(Root);
	Root->RegisterComponent();
	Actor = A;
	bTraceHistory = FParse::Param(FCommandLine::Get(), TEXT("TCAtmosHistory"));
	return true;
}

void FTCAtmosphere::Detach()
{
	if (AActor* A = Actor.Get()) A->Destroy();
	Actor.Reset();
	Overlays.Reset(); Sheets.Reset(); FallMesh.Reset(); BirdMesh.Reset();
	NoiseTex.Reset(); WispTex.Reset(); MaskTex.Reset();
	GroundMat.Reset(); MistMat.Reset(); BankMat.Reset(); FallMat.Reset(); BirdMat.Reset();
	Fog.Release();
	bOn = false; FallNow.Reset();
}

UMaterialInstanceDynamic* FTCAtmosphere::Mid(const TCHAR* Path, const TCHAR* Name)
{
	UMaterialInterface* M = LoadObject<UMaterialInterface>(nullptr, Path);
	if (!M) { UE_LOG(LogTC, Error, TEXT("atmosphere: %s is missing (unreal/Tools/atmosphere/make-atmosphere-assets.py); that layer is not drawn"), Path); return nullptr; }
	return UMaterialInstanceDynamic::Create(M, Actor.Get(), Name);
}

UProceduralMeshComponent* FTCAtmosphere::NewMesh(const TCHAR* Name, int32 Sort, bool bVisible)
{
	AActor* A = Actor.Get();
	if (!A) return nullptr;
	UProceduralMeshComponent* C = NewObject<UProceduralMeshComponent>(A, Name);
	C->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	C->SetCanEverAffectNavigation(false);
	C->SetCastShadow(false);
	C->bUseAsyncCooking = false;
	C->TranslucencySortPriority = Sort;
	C->SetupAttachment(A->GetRootComponent());
	A->AddInstanceComponent(C);
	C->RegisterComponent();
	C->SetVisibility(bVisible);
	return C;
}

void FTCAtmosphere::StartMatch(const FTCMatchState& State, const FString& MapKey, const ATCTerrain* Terrain, FName MoodOverride, const FString& AuthoredMood)
{
	using namespace TCAtmosImpl;
	const int32 W = State.Map.W, H = State.Map.H;
	bOn = W > 0 && H > 0 && Terrain && Actor.IsValid();
	if (!bOn) { StopMatch(); return; }
	MW = W * Cell; MH = H * Cell;
	auto Height = [Terrain](double X, double Z) { return (double)Terrain->HeightAt((float)X, (float)Z); };
	auto Ch = [&State, W](int X, int Y) { return (char)State.Map.Chars[Y * W + X]; };

	// the mood and the weather, no easing in (a match starts in its weather)
	const FString Kind = State.Weather.Now.IsEmpty() ? FString(TEXT("clear")) : State.Weather.Now;
	const FName MoodName = TCLook::MoodFor(State.Map.Name, MapKey, Kind, MoodOverride, AuthoredMood);
	const FTCMood* Found = TCLook::FindMood(MoodName);
	MoodRow = Found ? *Found : TCLook::DefaultMood();
	ShowerRain = ShowerWet = 0.f;
	WxNow = KnownLook(Kind) ? Kind : FString(TEXT("clear"));
	Blend();
	Cur = Goal; HazeNow = HazeGoal;
	History.Start();

	if (!NoiseTex)
	{
		const std::vector<uint8_t> N = NoiseTexture();
		TArray<FColor> Px; Px.SetNumUninitialized(NoiseN * NoiseN);
		for (int32 I = 0; I < Px.Num(); I++) Px[I] = FColor(N[I], N[I], N[I], N[I]);
		NoiseTex.Reset(MakeTexture(GetTransientPackage(), TEXT("T_TCAtmosNoise"), NoiseN, NoiseN, Px, true, true));
	}
	if (!WispTex)
	{
		const std::vector<double> A = WispAlpha();
		TArray<FColor> Px; Px.SetNumUninitialized(WispW * WispH);
		for (int32 I = 0; I < Px.Num(); I++) Px[I] = FColor(255, 255, 255, (uint8)FMath::Clamp(FMath::RoundToInt(A[I] * 255.0), 0, 255));
		WispTex.Reset(MakeTexture(GetTransientPackage(), TEXT("T_TCAtmosWisp"), WispW, WispH, Px, true, false));
	}
	{
		const std::vector<uint8_t> M = GroundMask(W, H, Ch);
		TArray<FColor> Px; Px.SetNumUninitialized(W * H);
		for (int32 I = 0; I < Px.Num(); I++) Px[I] = FColor(M[I * 4], M[I * 4 + 1], M[I * 4 + 2], M[I * 4 + 3]);
		MaskTex.Reset(MakeTexture(GetTransientPackage(), *MakeUniqueObjectName(GetTransientPackage(), UTexture2D::StaticClass(), TEXT("T_TCAtmosMask")).ToString(), W, H, Px, false, false));
	}
	if (!GroundMat) GroundMat.Reset(Mid(TCAtmos::GroundPath, TEXT("MID_TCAtmosGround")));
	if (!MistMat) MistMat.Reset(Mid(TCAtmos::MistPath, TEXT("MID_TCAtmosMist")));
	if (!BankMat) BankMat.Reset(Mid(TCAtmos::MistPath, TEXT("MID_TCAtmosBanks")));
	if (!BirdMat) BirdMat.Reset(Mid(TCAtmos::BirdPath, TEXT("MID_TCAtmosBird")));
	if (GroundMat)
	{
		GroundMat->SetTextureParameterValue(TEXT("Noise"), NoiseTex.Get());
		GroundMat->SetTextureParameterValue(TEXT("Mask"), MaskTex.Get());
		GroundMat->SetVectorParameterValue(TEXT("MapSize"), FLinearColor(MW, MH, 0, 0));
		// cloudMaterial: tint 0x1c2230; the shade fades out from the map centre over the surrounding land
		GroundMat->SetVectorParameterValue(TEXT("Tint"), TCLook::HexLinear(0x1c2230));
		GroundMat->SetVectorParameterValue(TEXT("MudColor"), TCLook::HexLinear(0x4b3a26));
		GroundMat->SetVectorParameterValue(TEXT("SnowColor"), TCLook::HexLinear(0xeef1f4));
		const double Hyp = FMath::Sqrt(MW * MW + MH * MH) / 2;
		GroundMat->SetVectorParameterValue(TEXT("FadeCentre"), FLinearColor(MW / 2, MH / 2, 0, 0));
		GroundMat->SetVectorParameterValue(TEXT("FadeRadii"), FLinearColor(Hyp + 180, Hyp + 480, 0, 0));
	}
	for (UMaterialInstanceDynamic* M : { MistMat.Get(), BankMat.Get() })
		if (M) { M->SetTextureParameterValue(TEXT("Wisp"), WispTex.Get()); }
	if (MistMat) MistMat->SetScalarParameterValue(TEXT("Opacity"), 0.28f);
	if (BirdMat) BirdMat->SetVectorParameterValue(TEXT("Color"), TCLook::HexLinear(0x2b2622));
	// the mood's tone mapping exposure: each layer blends in display space as three's canvas does (make-atmosphere-assets.py)
	for (UMaterialInstanceDynamic* M : { GroundMat.Get(), MistMat.Get(), BankMat.Get(), BirdMat.Get(), FallMat.Get() }) if (M) M->SetScalarParameterValue(TEXT("Exposure"), MoodRow.Exposure);
	// round 14: Graphics High's composer blends these layers in linear light (the engine's blend); -TCHazeAfterTone
	// (ATCLighting: the direct path, Low) keeps the display-space emulation
	ApplyBlendMode();
	ApplyMood();

	Sites = MoodRow.bMist ? MistSites(W, H, Ch, Cell, Height, MistCap) : std::vector<FSheet>();
	BankAt = BankSites(MW, MH, Height, BankCap);
	Flight = Flights(MW, MH);
	Phases = BirdPhases();

	MistOn = MoodRow.bMist && Sites.size() > 0;
	GfxWatch.TakeChanged(); // subscribed before the level is read (gfx.onChange(applyGfx))
	BuildSheets();

	if (!BirdMesh.IsValid()) BirdMesh = NewMesh(TEXT("AtmosBirds"), TCAtmos::SortBirds, true);
	if (UProceduralMeshComponent* B = BirdMesh.Get())
	{
		TArray<FVector> V; TArray<int32> I;
		for (int32 K = 0; K < Birds * 21; K++) { V.Add(FVector::ZeroVector); I.Add(K); }
		B->CreateMeshSection_LinearColor(0, V, I, {}, {}, {}, {}, false);
		if (BirdMat) B->SetMaterial(0, BirdMat.Get());
	}

	const FString Want = TCAtmosImpl::FallFor(WxNow, ShowerRain, MoodRow.Weather);
	if (FallNow != Want || !FallMesh.IsValid()) SwapFall(Want);
	FallA = 1.f;

	GroundSyncAt = -1.0; LastTerrainChunks = -1; bFogParamsStale = true;
	SyncGround(Terrain, true);
	LoggedState.Reset();
	UE_LOG(LogTC, Log, TEXT("atmosphere: start on '%s' (key '%s'): mood %s, weather %s; mist %d sheets, fog banks %d, birds %d, fall '%s'; clouds %.3f; reference %hs"),
		*State.Map.Name, *MapKey, *MoodRow.Name.ToString(), *WxNow, MistSheets(), (int32)BankAt.size(), (int32)Flight.size(), *FallNow, FMath::Min(0.32f, MoodRow.Clouds * Cur.Clouds), ReferencePin);
	TraceHistory(TEXT("start"), 0, State, false);
}

void FTCAtmosphere::BuildSheets()
{
	using namespace TCAtmosImpl;
	if (!Sheets.IsValid()) Sheets = NewMesh(TEXT("AtmosSheets"), TCAtmos::SortMist, true);
	UProceduralMeshComponent* SheetMesh = Sheets.Get();
	// the mist and the banks: unit planes (three's PlaneGeometry(1, 1) laid flat), placed every frame
	auto Planes = [SheetMesh](int32 Section, int32 N, UMaterialInterface* Mat)
	{
		TArray<FVector> V; TArray<int32> I; TArray<FVector> Nrm; TArray<FVector2D> UV;
		for (int32 K = 0; K < N; K++)
		{
			const int32 B = V.Num();
			// PlaneGeometry: (-0.5, 0.5) uv (0, 1), (0.5, 0.5) uv (1, 1), (-0.5, -0.5) uv (0, 0), (0.5, -0.5) uv (1, 0)
			for (int32 Q = 0; Q < 4; Q++) { V.Add(FVector::ZeroVector); Nrm.Add(FVector::UpVector); UV.Add(FVector2D(Q % 2, Q < 2 ? 1 : 0)); }
			I.Append({ B + 0, B + 2, B + 1, B + 2, B + 3, B + 1 });
		}
		if (!SheetMesh) return;
		SheetMesh->CreateMeshSection_LinearColor(Section, V, I, Nrm, UV, {}, {}, false);
		if (Mat) SheetMesh->SetMaterial(Section, Mat);
	};
	// atmosphere.js applyGfx: mist.count min(sites, low ? 18 : MIST_CAP), banks min(bankAt, low ? 26 : BANK_CAP)
	// (Client/TCGraphics.h); round 15: at the start and again on every change of level (Update)
	const FTCGraphicsBudget& Gfx = FTCGraphics::Get().Budget();
	GfxLow = FTCGraphics::Get().IsLow();
	MistBuilt = MistOn ? FMath::Min((int32)Sites.size(), FMath::Min(MistCap, Gfx.MistCap)) : 0;
	BanksBuilt = FMath::Min((int32)BankAt.size(), FMath::Min(BankCap, Gfx.BankCap));
	Planes(0, MistBuilt, MistMat.Get());
	Planes(1, BanksBuilt, BankMat.Get());
}

void FTCAtmosphere::SetHazeDistances(float Near, float Far)
{
	if (UMaterialInstanceDynamic* M = GroundMat.Get())
	{
		M->SetScalarParameterValue(TEXT("FogNear"), Near);
		M->SetScalarParameterValue(TEXT("FogFar"), Far);
	}
}

void FTCAtmosphere::AddGroundActor(AActor* Ground)
{
	if (Ground && !ExtraGround.Contains(Ground)) { ExtraGround.Add(Ground); GroundSyncAt = -1.0; }
}

void FTCAtmosphere::StopMatch()
{
	bOn = false;
	for (TPair<TWeakObjectPtr<UProceduralMeshComponent>, TWeakObjectPtr<UProceduralMeshComponent>>& O : Overlays) if (UProceduralMeshComponent* C = O.Value.Get()) C->SetVisibility(false);
	for (UProceduralMeshComponent* C : { Sheets.Get(), FallMesh.Get(), BirdMesh.Get() }) if (C) C->SetVisibility(false);
}

// atmosphere.js blend(): the look to ease toward
void FTCAtmosphere::Blend()
{
	TCLook::WeatherGoal(MoodRow, WxNow, ShowerRain, ShowerWet, Goal, HazeGoal);
}

// atmosphere.js applyMood (the light is ATCLighting's): cloud strength, the wet ground's numbers, the mist's tint
void FTCAtmosphere::ApplyBlendMode()
{
	// light.js uses its linear composer on High and draws straight to sRGB on Low. All atmosphere layers,
	// including a fall created after a weather change, must follow the same path as the haze post process.
	const bool bLinear = FTCGraphics::Get().Budget().bAmbientOcclusion && !FParse::Param(FCommandLine::Get(), TEXT("TCHazeAfterTone"));
	for (UMaterialInstanceDynamic* M : { GroundMat.Get(), MistMat.Get(), BankMat.Get(), BirdMat.Get(), FallMat.Get() })
		if (M) M->SetScalarParameterValue(TEXT("LinearBlend"), bLinear ? 1.f : 0.f);
}

void FTCAtmosphere::ApplyMood()
{
	if (GroundMat)
	{
		GroundMat->SetScalarParameterValue(TEXT("Strength"), FMath::Min(0.32f, MoodRow.Clouds * Cur.Clouds));
		GroundMat->SetScalarParameterValue(TEXT("Wet"), Cur.Wet);
		GroundMat->SetScalarParameterValue(TEXT("Sheen"), Cur.Sheen);
		GroundMat->SetScalarParameterValue(TEXT("Mud"), Cur.Mud);
		GroundMat->SetScalarParameterValue(TEXT("Cover"), Cur.Cover);
		GroundMat->SetVectorParameterValue(TEXT("Sky"), HazeNow);
	}
	const FLinearColor Tint = FMath::Lerp(MoodRow.Top, HazeNow, 0.65f);
	if (MistMat) MistMat->SetVectorParameterValue(TEXT("Tint"), Tint);
	if (BankMat) { BankMat->SetVectorParameterValue(TEXT("Tint"), Tint); BankMat->SetScalarParameterValue(TEXT("Opacity"), 0.34f * Cur.Banks); }
}

void FTCAtmosphere::SwapFall(const FString& Kind)
{
	using namespace TCAtmosImpl;
	FallNow = Kind;
	FallA = 0.f;
	if (UProceduralMeshComponent* C = FallMesh.Get()) { C->ClearAllMeshSections(); C->SetVisibility(false); }
	const int32 Index = FallIndex(TCHAR_TO_UTF8(*Kind));
	if (Index < 0) return;
	const FFall& F = Fall(Index);
	// it falls on the wind it started in (client/wind.js): the slant never changes under a fall already on screen
	FallVelocity(F, WindNow.X, WindNow.Z, FallVel);
	BuildFall();
	UE_LOG(LogTC, Log, TEXT("atmosphere: %s falls (%d points, %d drawn, velocity %.2f, %.2f, %.2f m/s on the wind %.2f, %.2f)"), *Kind, F.N, FallDrawn, FallVel[0], FallVel[1], FallVel[2], WindNow.X, WindNow.Z);
}

void FTCAtmosphere::BuildFall()
{
	using namespace TCAtmosImpl;
	const int32 Index = FallIndex(TCHAR_TO_UTF8(*FallNow));
	if (Index < 0) return;
	const FFall& F = Fall(Index);
	std::vector<float> Pos, Rnd;
	FallData(F, Pos, Rnd);
	if (!FallMesh.IsValid()) FallMesh = NewMesh(TEXT("AtmosFall"), TCAtmos::SortFall, false);
	if (!FallMat) { FallMat.Reset(Mid(TCAtmos::FallPath, TEXT("MID_TCAtmosFall"))); bFogParamsStale = true; }
	ApplyBlendMode();
	if (FallMat) FallMat->SetScalarParameterValue(TEXT("Exposure"), MoodRow.Exposure);
	UProceduralMeshComponent* FM = FallMesh.Get();
	if (!FM) return;
	// each point a camera-facing quad of four vertices at the point's own place in the box (pos x box, Unreal axes, cm):
	// UV0 = aRand.xy, UV1 = aRand.zw, vertex colour = the corner (gl_PointCoord)
	TArray<FVector> V; TArray<int32> I; TArray<FVector2D> U0, U1; TArray<FLinearColor> Col;
	V.Reserve(F.N * 4); I.Reserve(F.N * 6);
	// atmosphere.js applyGfx: fall.geometry.setDrawRange(0, low ? ceil(n / 3) : n) (round 15: again on a change of level)
	const int32 Drawn = FTCGraphics::Get().Budget().FallParticles(F.N);
	FallDrawn = Drawn;
	for (int32 K = 0; K < Drawn; K++)
	{
		const FVector P = ToUe(Pos[K * 3] * F.Box[0], Pos[K * 3 + 1] * F.Box[1], Pos[K * 3 + 2] * F.Box[2]);
		const int32 B = V.Num();
		for (int32 Q = 0; Q < 4; Q++)
		{
			V.Add(P);
			U0.Add(FVector2D(Rnd[K * 4], Rnd[K * 4 + 1]));
			U1.Add(FVector2D(Rnd[K * 4 + 2], Rnd[K * 4 + 3]));
			Col.Add(FLinearColor((float)(Q % 2), (float)(Q / 2), 0.f, 1.f));
		}
		I.Append({ B, B + 2, B + 1, B + 1, B + 2, B + 3 });
	}
	FM->CreateMeshSection_LinearColor(0, V, I, {}, U0, U1, {}, {}, Col, {}, false, false);
	FM->SetMaterial(0, FallMat.Get());
	FM->SetBoundsScale(1.2f);
	if (FallMat)
	{
		FallMat->SetVectorParameterValue(TEXT("Box"), FLinearColor(F.Box[0], F.Box[1], F.Box[2], 0));
		FallMat->SetVectorParameterValue(TEXT("Vel"), FLinearColor(FallVel[0], FallVel[1], FallVel[2], 0));
		FallMat->SetScalarParameterValue(TEXT("Sway"), F.Sway);
		FallMat->SetVectorParameterValue(TEXT("SizeM"), FLinearColor(F.Size[0], F.Size[1], 0, 0));
		FallMat->SetVectorParameterValue(TEXT("Px"), FLinearColor(1.f, F.PxMax, 0, 0)); // uPx: 1 px to pxMax (pixel ratio 1)
		FallMat->SetVectorParameterValue(TEXT("FadeRadii"), FLinearColor(F.Fade[0], F.Fade[1], 0, 0));
		FallMat->SetVectorParameterValue(TEXT("Color"), TCLook::HexLinear(F.Color));
		FallMat->SetScalarParameterValue(TEXT("Streak"), F.Streak);
	}
}

void FTCAtmosphere::SyncGround(const ATCTerrain* Terrain, bool bForce)
{
	if (TCRenderer34::NoWeatherMirror()) return;
	// cloudBoard.geometry = cover.geometry = ground.geometry (and the apron's): the ground's own meshes, 5 cm above them.
	// Checked at most twice a second and whenever the terrain meshed chunks; a section is copied again only when its
	// shape changed (fog recolours leave it alone).
	const double Now = FPlatformTime::Seconds();
	const int32 Chunks = Terrain ? Terrain->ChunksMeshed() : -1;
	if (!bForce && Chunks == LastTerrainChunks && GroundSyncAt >= 0 && Now - GroundSyncAt < 2.0) return;
	if (!bForce && GroundSyncAt >= 0 && Now - GroundSyncAt < 0.5) return;
	GroundSyncAt = Now; LastTerrainChunks = Chunks;
	TArray<AActor*> Sources;
	if (Terrain) Sources.Add(const_cast<ATCTerrain*>(Terrain));
	ExtraGround.RemoveAll([](const TWeakObjectPtr<AActor>& A) { return !A.IsValid(); });
	for (const TWeakObjectPtr<AActor>& A : ExtraGround) if (A.IsValid()) Sources.Add(A.Get());
	// The holder actor owns its components. Removing only a weak map entry leaves the old mesh registered,
	// rendering weather over retired ground and retaining its vertex/index buffers until the whole view detaches.
	for (auto It = Overlays.CreateIterator(); It; ++It)
		if (!It.Key().IsValid() || !It.Key()->IsRegistered() || !Sources.Contains(It.Key()->GetOwner()))
		{
			if (UProceduralMeshComponent* Old = It.Value().Get()) Old->DestroyComponent();
			It.RemoveCurrent();
		}
	int32 Mirrored = 0;
	for (AActor* A : Sources)
	{
		TArray<UProceduralMeshComponent*> Comps;
		A->GetComponents<UProceduralMeshComponent>(Comps);
		for (UProceduralMeshComponent* Src : Comps)
		{
			if (!Src || Src->GetOwner() == Actor.Get()) continue;
			TWeakObjectPtr<UProceduralMeshComponent>& Slot = Overlays.FindOrAdd(Src);
			if (!Slot.IsValid())
			{
				Slot = NewMesh(*FString::Printf(TEXT("AtmosGround_%s_%s"), *A->GetName(), *Src->GetName()), TCAtmos::SortGround, true);
				if (UProceduralMeshComponent* D = Slot.Get()) { D->SetUsingAbsoluteLocation(true); D->SetUsingAbsoluteRotation(true); D->SetUsingAbsoluteScale(true); }
			}
			UProceduralMeshComponent* Dst = Slot.Get();
			if (!Dst) continue;
			FTransform Tm = Src->GetComponentTransform();
			Tm.AddToTranslation(FVector(0, 0, 5.0)); // cover +0.05 m, the cloud board +0.06 m: one layer here
			Dst->SetWorldTransform(Tm);
			const int32 N = Src->GetNumSections();
			for (int32 S = 0; S < N; S++)
			{
				FProcMeshSection* From = Src->GetProcMeshSection(S);
				FProcMeshSection* To = S < Dst->GetNumSections() ? Dst->GetProcMeshSection(S) : nullptr;
				const int32 NV = From ? From->ProcVertexBuffer.Num() : 0;
				// Digging and discovery can change normals or triangle connections while
				// retaining all vertex positions. Snow and puddles read those normals.
				bool bSame = From && To && To->ProcVertexBuffer.Num() == NV && To->ProcIndexBuffer == From->ProcIndexBuffer;
				for (int32 K = 0; bSame && K < NV; K++)
					bSame = To->ProcVertexBuffer[K].Position == From->ProcVertexBuffer[K].Position
						&& To->ProcVertexBuffer[K].Normal == From->ProcVertexBuffer[K].Normal;
				if (!bSame)
				{
					if (!From || NV == 0) { if (To) Dst->ClearMeshSection(S); continue; }
					TArray<FVector> V; TArray<FVector> Nrm; TArray<int32> I;
					V.Reserve(NV); Nrm.Reserve(NV);
					for (const FProcMeshVertex& PV : From->ProcVertexBuffer) { V.Add(PV.Position); Nrm.Add(PV.Normal); }
					I.Reserve(From->ProcIndexBuffer.Num());
					for (const uint32 X : From->ProcIndexBuffer) I.Add((int32)X);
					Dst->CreateMeshSection_LinearColor(S, V, I, Nrm, {}, {}, {}, false);
					Dst->SetMaterial(S, GroundMat.Get());
				}
				if (From) Dst->SetMeshSectionVisible(S, From->bSectionVisible);
				if (From && NV) Mirrored++;
			}
			for (int32 S = N; S < Dst->GetNumSections(); S++) Dst->ClearMeshSection(S);
		}
	}
	if (Mirrored != MirroredSections) UE_LOG(LogTC, Log, TEXT("atmosphere: the cloud shade and wet ground lie on %d ground sections of %d mesh(es)"), Mirrored, Overlays.Num());
	MirroredSections = Mirrored;
}

void FTCAtmosphere::Update(float Dt, const FTCMatchState& State, const ATCTerrain* Terrain, APlayerController* PC, FVector2D Focus)
{
	using namespace TCAtmosImpl;
	if (!bOn || !Terrain) return;
	// weather-view.js snapshot: setWeather when the row's weather turned
	if (!State.Weather.Now.IsEmpty() && State.Weather.Now != WxNow)
	{
		const FString Next = KnownLook(State.Weather.Now) ? State.Weather.Now : FString(TEXT("clear"));
		if (Next != WxNow) { WxNow = Next; Blend(); History.GoalChanged(); UE_LOG(LogTC, Log, TEXT("atmosphere: the weather turns to %s; easing (about 8 s)"), *WxNow); }
	}
	// main.js applySnapshot: if (s.wx) { setWind(wx[2], wx[3]); atmos.setRain(wx[0], wx[1]) }
	if (State.Wx.bKnown)
	{
		WindNow.Set(State.Wx.WindAngle, State.Wx.WindStrength);
		if ((float)State.Wx.Rain != ShowerRain || (float)State.Wx.Wet != ShowerWet) { ShowerRain = (float)State.Wx.Rain; ShowerWet = (float)State.Wx.Wet; Blend(); History.GoalChanged(); }
	}
	History.Frame(Dt);
	// Comparison-only clock: hold animated weather at a recorded age without changing simulation or easing.
	// The browser comparison copy holds its atmosphere.js time and accumulated drift at the same values.
	double LookTime = 0;
	const bool bFixedLookTime = FParse::Value(FCommandLine::Get(), TEXT("TCLookTime="), LookTime) && FMath::IsFinite(LookTime) && LookTime >= 0;
	T = bFixedLookTime ? FMath::Fmod(LookTime, 10000.0) : FMath::Fmod(T + Dt, 10000.0);
	if (bFixedLookTime) { DriftX = 0; DriftY = 0; }
	SyncGround(Terrain, false);
	// round 15: gfx.onChange(applyGfx): the mist and bank counts and the fall's draw range follow a change of level in
	// the middle of a match (clouds, birds and the dust fall read the level every frame below)
	if (GfxWatch.TakeChanged() && FTCGraphics::Get().IsLow() != GfxLow)
	{
		ApplyBlendMode();
		BuildSheets();
		if (!FallNow.IsEmpty()) BuildFall();
		LevelSwitches++;
		UE_LOG(LogTC, Log, TEXT("atmosphere: graphics %s mid-match: mist %d sheets, fog banks %d, fall '%s' %d points drawn, clouds %s, birds %s"), GfxLow ? TEXT("low") : TEXT("high"),
			MistBuilt, BanksBuilt, *FallNow, FallDrawn, FTCGraphics::Get().Budget().bCloudShadows ? TEXT("on") : TEXT("off"), FTCGraphics::Get().Budget().bBirds ? TEXT("on") : TEXT("off"));
	}

	// ease toward the weather (about 8 s to settle), as ATCLighting eases the light
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
		ApplyMood();
	}
	// fade the fall out and the next one in
	const FString Want = FallFor(WxNow, ShowerRain, MoodRow.Weather);
	if (AdvanceFall(FallA, !FallNow.IsEmpty(), FallNow == Want, !Want.IsEmpty(), Dt)) SwapFall(Want);

	bCoverShown = Cur.Wet + Cur.Sheen + Cur.Mud + Cur.Cover > 0.004f;
	bCloudsShown = MoodRow.Clouds > 0.f && FTCGraphics::Get().Budget().bCloudShadows; // Low hides the cloud shadows
	BanksOn = Cur.Banks > 0.004f && BankAt.size() > 0;
	if (GroundMat)
	{
		GroundMat->SetScalarParameterValue(TEXT("CoverOn"), bCoverShown ? 1.f : 0.f);
		GroundMat->SetScalarParameterValue(TEXT("CloudOn"), bCloudsShown ? 1.f : 0.f);
		if (bCloudsShown)
		{
			// the cloud shade moves with the wind, summed frame by frame so a change of wind does not make it jump
			if (!bFixedLookTime) Drift(DriftX, DriftY, WindNow.X, WindNow.Z, Dt);
			GroundMat->SetVectorParameterValue(TEXT("DriftA"), FLinearColor(DriftX, DriftY, 0, 0));
			GroundMat->SetVectorParameterValue(TEXT("DriftB"), FLinearColor(FMath::Fmod(DriftX * 1.6 + 0.37, 1.0), FMath::Fmod(DriftY * 1.9 + 0.61, 1.0), 0, 0));
		}
		GroundMat->SetScalarParameterValue(TEXT("Time"), T);
		if (PC && PC->PlayerCameraManager) GroundMat->SetVectorParameterValue(TEXT("CamPos"), FLinearColor(PC->PlayerCameraManager->GetCameraLocation()));
	}
	for (TPair<TWeakObjectPtr<UProceduralMeshComponent>, TWeakObjectPtr<UProceduralMeshComponent>>& O : Overlays)
		if (UProceduralMeshComponent* C = O.Value.Get()) C->SetVisibility(O.Key.IsValid() && O.Key->IsVisible() && (bCloudsShown || bCoverShown));

	// the fog of war over these layers (the ground's own dimming: 1, 0.55, 0.32)
	if (Fog.Update(State, Terrain) || bFogParamsStale)
		for (UMaterialInstanceDynamic* M : { GroundMat.Get(), MistMat.Get(), BankMat.Get(), FallMat.Get(), BirdMat.Get() })
			if (M && Fog.Texture())
			{
				M->SetTextureParameterValue(TEXT("FogMap"), Fog.Texture());
				M->SetVectorParameterValue(TEXT("FogSize"), Fog.Size());
				M->SetScalarParameterValue(TEXT("FogOn"), 1.f);
				bFogParamsStale = false;
			}
	if (GroundMat)
	{
		const FTCFogFrontier* Shared = TCFogLook::Active();
		const bool bSharedOverlay = Shared && Shared->HasMap() && Shared->MapW() == State.Map.W
			&& Shared->MapH() == State.Map.H && Shared->AlphaTexture();
		// atmosphere.js cover/cloud render before main.js and apron.js's fog.
		// The common overlay now shades this ground too. Keep the legacy
		// multiplier only without it, or with an old after-DOF carrier asset.
		// Recheck every frame: attachment can change without any texel change.
		const bool bBeforeFog = GroundMat->GetMaterial()->TranslucencyPass == MTP_BeforeDOF;
		GroundMat->SetScalarParameterValue(TEXT("FogOn"), bSharedOverlay && bBeforeFog ? 0.f : 1.f);
	}

	UpdateSheets(T);
	UpdateFall(Terrain, PC, Focus);
	UpdateBirds(T, Terrain);
	TraceHistory(TEXT("frame"), Dt, State, bFixedLookTime);
	TraceView(PC, Focus);

	const FString Line = FString::Printf(TEXT("mood %s, weather %s, shower %.2f wet %.2f, fall '%s', mist %d, banks %s, cover %s"), *MoodRow.Name.ToString(), *WxNow, ShowerRain, ShowerWet,
		*FallNow, MistSheets(), BanksOn ? TEXT("on") : TEXT("off"), bCoverShown ? TEXT("on") : TEXT("off"));
	if (Line != LoggedState) { LoggedState = Line; UE_LOG(LogTC, Log, TEXT("atmosphere: %s"), *Describe()); }
	RunChecks(State, PC);
}

void FTCAtmosphere::UpdateSheets(double Time)
{
	using namespace TCAtmosImpl;
	UProceduralMeshComponent* SheetMesh = Sheets.Get();
	if (!SheetMesh) return;
	SheetMesh->SetVisibility(bOn);
	auto Place = [SheetMesh](int32 Section, const std::vector<FSheet>& At, int32 Count, bool bBank, double Tm, double Lift) mutable
	{
		if (Section >= SheetMesh->GetNumSections()) return;
		const FProcMeshSection* S = SheetMesh->GetProcMeshSection(Section);
		if (!S || S->ProcVertexBuffer.Num() % 4) return;
		Count = FMath::Min(Count, S->ProcVertexBuffer.Num() / 4); // the sheets made at the match's start (the graphics level's caps)
		TArray<FVector> V; V.Reserve(Count * 4);
		for (int32 K = 0; K < Count; K++)
		{
			double X, Y, Z, Yaw, SX, SZ;
			SheetPose(At[K], Tm, bBank, Lift, X, Y, Z, Yaw, SX, SZ);
			// the plane's corners (x, 0, -y) for PlaneGeometry's (x, y), scaled, turned by yaw about three's y, moved
			const double CY = FMath::Cos(Yaw), SY = FMath::Sin(Yaw);
			for (int32 Q = 0; Q < 4; Q++)
			{
				const double PX = (Q % 2 ? 0.5 : -0.5) * SX, PZ = -(Q < 2 ? 0.5 : -0.5) * SZ;
				V.Add(ToUe(X + PX * CY + PZ * SY, Y, Z - PX * SY + PZ * CY));
			}
		}
		SheetMesh->UpdateMeshSection_LinearColor(Section, V, {}, {}, {}, {});
	};
	const double MistT = Time; // reduceMotion: never in the native client
	if (MistOn) Place(0, Sites, FMath::Min((int32)Sites.size(), MistCap), false, MistT, 0.0);
	if (SheetMesh->GetNumSections() > 0) SheetMesh->SetMeshSectionVisible(0, MistOn);
	if (MistMat) MistMat->SetScalarParameterValue(TEXT("Time"), MistT);
	if (BanksOn) Place(1, BankAt, FMath::Min((int32)BankAt.size(), BankCap), true, MistT, (1.0 - Cur.Banks) * 3.0);
	if (SheetMesh->GetNumSections() > 1) SheetMesh->SetMeshSectionVisible(1, BanksOn);
	if (BankMat) BankMat->SetScalarParameterValue(TEXT("Time"), MistT);
}

void FTCAtmosphere::UpdateFall(const ATCTerrain* Terrain, APlayerController* PC, FVector2D Focus)
{
	using namespace TCAtmosImpl;
	UProceduralMeshComponent* FM = FallMesh.Get();
	if (!FM) return;
	const int32 Index = FallIndex(TCHAR_TO_UTF8(*FallNow));
	// Graphics Low keeps rain and snow, drops the dust fall (atmosphere.js: !(low && kind === 'dust'))
	const bool bShow = bOn && Index >= 0 && FallMat && PC && PC->PlayerCameraManager && FM->GetNumSections() > 0 && !(Index == 2 && !FTCGraphics::Get().Budget().bDustFall);
	FM->SetVisibility(bShow);
	if (!bShow) return;
	const FFall& F = Fall(Index);
	// the box follows the camera target; it starts 4 m below the ground there (dust 1.5 m)
	const double GY = Terrain->HeightAt((float)Focus.X, (float)Focus.Y);
	const double Min[3] = { Focus.X - F.Box[0] / 2, GY - (Index == 2 ? 1.5 : 4.0), Focus.Y - F.Box[2] / 2 };
	FM->SetWorldLocation(ToUe(Min[0], Min[1], Min[2]));
	const bool bFallsWithWeather = WxNow == TEXT("rain") || WxNow == TEXT("snow");
	FallMat->SetScalarParameterValue(TEXT("Alpha"), F.Alpha * FallA * (bFallsWithWeather || Index != 0 ? 1.f : ShowerRain));
	FallMat->SetScalarParameterValue(TEXT("Time"), T);
	FallMat->SetVectorParameterValue(TEXT("BoxMin"), FLinearColor(Min[0], Min[1], Min[2], 0));
	FallMat->SetVectorParameterValue(TEXT("FadeCentre"), FLinearColor(Focus.X, Focus.Y, 0, 0));
	const FVector Eye = PC->PlayerCameraManager->GetCameraLocation();
	const FRotationMatrix R(PC->PlayerCameraManager->GetCameraRotation());
	FallMat->SetVectorParameterValue(TEXT("CamPos"), FLinearColor(Eye));
	FallMat->SetVectorParameterValue(TEXT("CamFwd"), FLinearColor(R.GetUnitAxis(EAxis::X)));
	FallMat->SetVectorParameterValue(TEXT("CamRight"), FLinearColor(R.GetUnitAxis(EAxis::Y)));
	FallMat->SetVectorParameterValue(TEXT("CamUp"), FLinearColor(R.GetUnitAxis(EAxis::Z)));
	// uScale: the drawing buffer's height over 2 tan(fov / 2), the browser camera's 42 degrees top to bottom
	FIntPoint Size(1280, 720);
	if (GEngine && GEngine->GameViewport && GEngine->GameViewport->Viewport) Size = GEngine->GameViewport->Viewport->GetSizeXY();
	FallMat->SetScalarParameterValue(TEXT("Scale"), Size.Y / (2.0 * FMath::Tan(FMath::DegreesToRadians(ATCCameraPawn::VerticalFovDegrees) / 2.0)));
}

void FTCAtmosphere::UpdateBirds(double Time, const ATCTerrain* Terrain)
{
	using namespace TCAtmosImpl;
	UProceduralMeshComponent* BM = BirdMesh.Get();
	if (!BM) return;
	const bool bShow = bOn && Flight.size() > 0 && FTCGraphics::Get().Budget().bBirds; // Low: no birds
	BM->SetVisibility(bShow);
	if (!bShow || BM->GetNumSections() < 1) return;
	const double* BV = BirdVertices();
	TArray<FVector> V; V.Reserve((int32)Flight.size() * 21);
	for (size_t I = 0; I < Flight.size(); I++)
	{
		double X, Z, Up, Yaw, Roll;
		BirdPose(Flight[I], Time, X, Z, Up, Yaw, Roll);
		const FVector3d Pos(X, Terrain->HeightAt((float)X, (float)Z) + Up, Z);
		for (int32 K = 0; K < 21; K++)
		{
			const FVector3d P(BV[K * 3], BV[K * 3 + 1] + FlapLift(BV[K * 3], Time, Phases[I]), BV[K * 3 + 2]);
			const FVector3d W = Pose(P, Yaw, Roll, 1.3, Pos);
			V.Add(ToUe(W.X, W.Y, W.Z));
		}
	}
	if (V.Num() == BM->GetProcMeshSection(0)->ProcVertexBuffer.Num()) BM->UpdateMeshSection_LinearColor(0, V, {}, {}, {}, {});
}

FString FTCAtmosphere::Describe() const
{
	return FString::Printf(TEXT("mood %s, weather %s (shower rain %.2f, wet ground %.2f), wind (%.2f, %.2f); clouds %s strength %.3f, cover %s (wet %.2f, puddles %.2f, mud %.2f, snow %.2f), mist %d, fog banks %s (%.2f), fall '%s' at %.2f, birds %d, ground sections mirrored %d; rendered %llu frames %.9f s, current goal %.9f s"),
		*MoodRow.Name.ToString(), *WxNow, ShowerRain, ShowerWet, WindNow.X, WindNow.Z, bCloudsShown ? TEXT("on") : TEXT("off"), FMath::Min(0.32f, MoodRow.Clouds * Cur.Clouds),
		bCoverShown ? TEXT("on") : TEXT("off"), Cur.Wet, Cur.Sheen, Cur.Mud, Cur.Cover, MistSheets(), BanksOn ? TEXT("on") : TEXT("off"), Cur.Banks, *FallNow, FallA, (int32)Flight.size(), MirroredSections,
		(unsigned long long)History.Frames, History.Seconds, History.GoalSeconds);
}

void FTCAtmosphere::TraceHistory(const TCHAR* Event, double Dt, const FTCMatchState& State, bool bFixedClock) const
{
	if (!bTraceHistory) return;
	// Known weather/fall/mood enums only. No room, identity, receipt, token or seed.
	// Precision retains the exact input float dt as a double for source replay.
	UE_LOG(LogTC, Log, TEXT("atmos-history {\"event\":\"%s\",\"start\":%llu,\"frame\":%llu,\"engineFrame\":%llu,\"tick\":%d,\"dt\":%.17g,\"renderSeconds\":%.17g,\"goalSeconds\":%.17g,\"fixedClock\":%s,\"time\":%.17g,\"graphics\":\"%s\",\"mood\":\"%s\",\"weather\":\"%s\",\"rain\":%.17g,\"wet\":%.17g,\"wind\":[%.17g,%.17g],\"drift\":[%.17g,%.17g],\"look\":[%.17g,%.17g,%.17g,%.17g,%.17g,%.17g,%.17g,%.17g,%.17g],\"goal\":[%.17g,%.17g,%.17g,%.17g,%.17g,%.17g,%.17g,%.17g,%.17g],\"haze\":[%.17g,%.17g,%.17g],\"hazeGoal\":[%.17g,%.17g,%.17g],\"fall\":\"%s\",\"fallAlpha\":%.17g,\"banksVisible\":%s}"),
		Event, (unsigned long long)History.Starts, (unsigned long long)History.Frames, (unsigned long long)GFrameCounter, State.Tick, Dt, History.Seconds, History.GoalSeconds, bFixedClock ? TEXT("true") : TEXT("false"), T,
		FTCGraphics::Get().IsLow() ? TEXT("low") : TEXT("high"), *MoodRow.Name.ToString(), *WxNow, (double)ShowerRain, (double)ShowerWet, WindNow.X, WindNow.Z, DriftX, DriftY,
		(double)Cur.Haze, (double)Cur.Sun, (double)Cur.Shadow, (double)Cur.Clouds, (double)Cur.Wet, (double)Cur.Sheen, (double)Cur.Mud, (double)Cur.Cover, (double)Cur.Banks,
		(double)Goal.Haze, (double)Goal.Sun, (double)Goal.Shadow, (double)Goal.Clouds, (double)Goal.Wet, (double)Goal.Sheen, (double)Goal.Mud, (double)Goal.Cover, (double)Goal.Banks,
		(double)HazeNow.R, (double)HazeNow.G, (double)HazeNow.B, (double)HazeGoal.R, (double)HazeGoal.G, (double)HazeGoal.B, *FallNow, FallA, BanksOn ? TEXT("true") : TEXT("false"));
}

void FTCAtmosphere::TraceView(const APlayerController* PC, FVector2D Focus) const
{
	if (!bTraceHistory || !PC || !PC->PlayerCameraManager) return;
	const APlayerCameraManager& Camera = *PC->PlayerCameraManager;
	const FVector Eye = Camera.GetCameraLocation();
	const FRotationMatrix Axes(Camera.GetCameraRotation());
	const FVector Forward = Axes.GetUnitAxis(EAxis::X), Right = Axes.GetUnitAxis(EAxis::Y), Up = Axes.GetUnitAxis(EAxis::Z);
	const ATCPlayerController* View = Cast<ATCPlayerController>(PC);
	int32 Width = 0, Height = 0; PC->GetViewportSize(Width, Height);
	// Public presentation only. Join to atmos-history by start/frame/engineFrame.
	// This observes the CPU view, not the eventual native GPU readback frame.
	UE_LOG(LogTC, Log, TEXT("atmos-view {\"start\":%llu,\"frame\":%llu,\"engineFrame\":%llu,\"eyeCm\":[%.17g,%.17g,%.17g],\"forward\":[%.17g,%.17g,%.17g],\"right\":[%.17g,%.17g,%.17g],\"up\":[%.17g,%.17g,%.17g],\"hFov\":%.17g,\"focus\":[%.17g,%.17g],\"distance\":%.17g,\"yaw\":%.17g,\"viewport\":[%d,%d]}"),
		(unsigned long long)History.Starts, (unsigned long long)History.Frames, (unsigned long long)GFrameCounter,
		Eye.X, Eye.Y, Eye.Z, Forward.X, Forward.Y, Forward.Z, Right.X, Right.Y, Right.Z, Up.X, Up.Y, Up.Z,
		(double)Camera.GetFOVAngle(), Focus.X, Focus.Y, View ? (double)View->ViewDistance() : 0.0, View ? (double)View->ViewYaw() : 0.0, Width, Height);
}

void FTCAtmosphere::RunChecks(const FTCMatchState& State, APlayerController* PC)
{
	// -TCAtmosShotAfter=S[,S2...] -TCAtmosShotFile=<path.png> [-TCAtmosShotFocus=x,z|spawn|station|track] [-TCAtmosShotDist=m]:
	// S seconds after the first match frame, look there and save the frame without the HUD (<file>-<S>s-nohud.png),
	// then with it (<file>-<S>s-hud.png). The atmosphere's state is logged with each.
	const double Now = FPlatformTime::Seconds();
	if (!bChecksRead)
	{
		bChecksRead = true;
		const TCHAR* Cmd = FCommandLine::Get();
		FString After, Focus;
		if (FParse::Value(Cmd, TEXT("TCAtmosShotAfter="), After, false) && FParse::Value(Cmd, TEXT("TCAtmosShotFile="), ShotFile))
		{
			TArray<FString> Parts; After.ParseIntoArray(Parts, TEXT(","));
			for (const FString& P : Parts) ShotTimes.Add(FCString::Atod(*P));
			ShotTimes.Sort();
		}
		FParse::Value(Cmd, TEXT("TCAtmosShotDist="), ShotDist);
		if (FParse::Value(Cmd, TEXT("TCAtmosShotFocus="), Focus, false))
		{
			FString A, B;
			if (Focus == TEXT("station") || Focus == TEXT("track") || Focus == TEXT("spawn")) ShotFocusKind = Focus;
			else if (Focus.Split(TEXT(","), &A, &B)) ShotFocus = FVector2D(FCString::Atod(*A), FCString::Atod(*B));
		}
	}
	if (!ShotTimes.Num() || !PC || !State.bHasMatch) return;
	if (MatchSeen < 0) MatchSeen = Now;
	AHUD* Hud = PC->GetHUD();
	ATCPlayerController* TPC = Cast<ATCPlayerController>(PC);
	const FString Base = FPaths::ChangeExtension(ShotFile, TEXT(""));
	const int32 Sec = FMath::RoundToInt(ShotTimes[0]);
	switch (ShotStep)
	{
	case 0:
		if (Now - MatchSeen < ShotTimes[0]) return;
		{
			FVector2D At = ShotFocus;
			if (ShotFocusKind == TEXT("spawn")) At = State.Spawn; // the opening view's target (ATCPlayerController::StartView)
			else if (!ShotFocusKind.IsEmpty())
			{
				if (!Railways || !(ShotFocusKind == TEXT("station") ? Railways->FirstStation(At) : Railways->TrackCentre(At))) At = FVector2D(-1, -1);
			}
			if (TPC && At.X >= 0) TPC->EpilogueCamera(At, ShotDist > 0 ? (float)ShotDist : TPC->ViewDistance());
			else if (TPC && ShotDist > 0) TPC->ZoomTo((float)ShotDist);
			UE_LOG(LogTC, Log, TEXT("atmosphere check: shot at %d s, looking at (%.1f, %.1f)%s, distance %.0f m, view yaw %.2f degrees (browser cam.yaw %.4f rad)"), Sec, At.X, At.Y,
				ShotFocusKind.IsEmpty() ? TEXT("") : *(TEXT(" (") + ShotFocusKind + TEXT(")")), TPC ? TPC->ViewDistance() : -1.f, TPC ? TPC->ViewYaw() : 0.f, TPC ? FMath::DegreesToRadians(-90.f - TPC->ViewYaw()) : 0.f);
		}
		ShotStep = 1; ShotStepAt = Now + 2.0; // the camera glides there
		return;
	case 1:
		if (Now < ShotStepAt) return;
		if (Hud) Hud->bShowHUD = false;
		ShotStep = 2; ShotStepAt = Now + 0.3;
		return;
	case 2:
		if (Now < ShotStepAt) return;
		FScreenshotRequest::RequestScreenshot(FString::Printf(TEXT("%s-%ds-nohud.png"), *Base, Sec), false, false);
		{
			// the light's haze distances at this moment (ATCLighting, light.js followView), for side by sides
			float Near = -1.f, Far = -1.f;
			TActorIterator<ATCLighting> It(PC->GetWorld());
			if (It) { Near = It->FogNear(); Far = It->FogFar(); }
			UE_LOG(LogTC, Log, TEXT("atmosphere check: %s-%ds-nohud.png requested; haze %.1f to %.1f m; %s"), *Base, Sec, Near, Far, *Describe());
		}
		ShotStep = 3; ShotStepAt = Now + 0.5;
		return;
	case 3:
		if (Now < ShotStepAt) return;
		if (Hud) Hud->bShowHUD = true;
		ShotStep = 4; ShotStepAt = Now + 0.4;
		return;
	default:
		if (Now < ShotStepAt) return;
		FScreenshotRequest::RequestScreenshot(FString::Printf(TEXT("%s-%ds-hud.png"), *Base, Sec), true, false);
		UE_LOG(LogTC, Log, TEXT("atmosphere check: %s-%ds-hud.png requested"), *Base, Sec);
		ShotTimes.RemoveAt(0);
		ShotStep = 0;
		return;
	}
}


int32 FTCAtmosphere::FallPoints() const
{
	const UProceduralMeshComponent* FM = FallMesh.Get();
	return FM && FM->GetNumSections() > 0 ? const_cast<UProceduralMeshComponent*>(FM)->GetProcMeshSection(0)->ProcVertexBuffer.Num() / 4 : 0;
}

// ---------------------------------------------------------------------------------------------------------------------
// Tests: the cores against the reference oracle (unreal/Tools/atmosphere/fixtures/atmos-oracle.json.gz, oracle.mjs),
// and both views in a scratch world.

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/Compression.h"
#include "Misc/FileHelper.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Engine/StaticMesh.h"

namespace TCAtmosTests
{
	TSharedPtr<FJsonObject> LoadFixture(FString& OutError)
	{
		const FString Path = FPaths::Combine(FPaths::ProjectDir(), TEXT("Tools/atmosphere/fixtures/atmos-oracle.json.gz"));
		TArray<uint8> Gz;
		if (!FFileHelper::LoadFileToArray(Gz, *Path) || Gz.Num() < 18) { OutError = TEXT("cannot read ") + Path; return nullptr; }
		const uint32 Size = (uint32)Gz[Gz.Num() - 4] | (uint32)Gz[Gz.Num() - 3] << 8 | (uint32)Gz[Gz.Num() - 2] << 16 | (uint32)Gz[Gz.Num() - 1] << 24; // gzip ISIZE
		TArray<uint8> Raw;
		Raw.SetNumUninitialized((int64)Size + 1);
		if (!FCompression::UncompressMemory(NAME_Gzip, Raw.GetData(), Size, Gz.GetData(), Gz.Num())) { OutError = TEXT("cannot gunzip ") + Path; return nullptr; }
		Raw[Size] = 0;
		TSharedPtr<FJsonObject> Root;
		if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(UTF8_TO_TCHAR((const ANSICHAR*)Raw.GetData())), Root) || !Root.IsValid()) { OutError = TEXT("cannot parse ") + Path; return nullptr; }
		return Root;
	}
	double Wave(double X, double Z) { return 0.6 * FMath::Sin(X * 0.13) + 0.4 * FMath::Cos(Z * 0.09) + 0.002 * X - 0.35; }
	bool Near(double A, double B) { return FMath::Abs(A - B) <= 2e-4 + 1e-5 * FMath::Abs(B); }
	// how many of Want's numbers differ from Got's
	int32 Differ(const std::vector<double>& Got, const TArray<TSharedPtr<FJsonValue>>& Want, bool bExact = false)
	{
		if ((int32)Got.size() != Want.Num()) return FMath::Max(1, FMath::Abs((int32)Got.size() - Want.Num()));
		int32 Bad = 0;
		for (int32 I = 0; I < Want.Num(); I++) Bad += bExact ? Got[I] != Want[I]->AsNumber() : !Near(Got[I], Want[I]->AsNumber());
		return Bad;
	}
	std::vector<double> Flat(const std::vector<TCRailCore::FMat4>& Ms) { std::vector<double> V; for (const auto& M : Ms) V.insert(V.end(), M.E, M.E + 16); return V; }
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
		~FScratchWorld() { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); }
	};
	// a start message: a fixture map's rows, flat, World Conquest or not, with cell rows
	TSharedRef<FJsonObject> StartOf(const FJsonObject& MapCase, bool bWorld, const TArray<TSharedPtr<FJsonValue>>& Cells, const TCHAR* Weather)
	{
		TSharedRef<FJsonObject> Map = MakeShared<FJsonObject>();
		Map->SetNumberField(TEXT("w"), MapCase.GetNumberField(TEXT("w")));
		Map->SetNumberField(TEXT("h"), MapCase.GetNumberField(TEXT("h")));
		Map->SetStringField(TEXT("name"), MapCase.GetStringField(TEXT("key")));
		Map->SetArrayField(TEXT("rows"), MapCase.GetArrayField(TEXT("rows")));
		if (bWorld) Map->SetBoolField(TEXT("world"), true);
		TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
		M->SetStringField(TEXT("t"), TEXT("start"));
		M->SetObjectField(TEXT("map"), Map);
		M->SetNumberField(TEXT("you"), 0);
		M->SetNumberField(TEXT("matchId"), 3);
		M->SetArrayField(TEXT("cells"), Cells);
		M->SetArrayField(TEXT("weather"), { MakeShared<FJsonValueString>(Weather) });
		return M;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRailwaysOracleTest, "ThreeCrossroads.World.RailwaysOracle", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCRailwaysOracleTest::RunTest(const FString& Parameters)
{
	using namespace TCAtmosTests;
	FString Error;
	const TSharedPtr<FJsonObject> Root = LoadFixture(Error);
	if (!Root.IsValid()) { AddError(Error); return false; }
	TestEqual(TEXT("fixture reference"), Root->GetStringField(TEXT("reference")), FString(UTF8_TO_TCHAR(TCRailCore::ReferencePin)));
	const FJsonObject& Rail = *Root->GetObjectField(TEXT("rail"));
	const TArray<TSharedPtr<FJsonValue>>& Steps = Rail.GetArrayField(TEXT("steps"));
	const TArray<TSharedPtr<FJsonValue>>& Outs = Rail.GetArrayField(TEXT("out"));
	TCRailCore::FModel Model;
	Model.Reset((int)Rail.GetNumberField(TEXT("w")));
	int32 Bad = 0, Values = 0;
	for (int32 I = 0; I < Steps.Num(); I++)
	{
		const FJsonObject& S = *Steps[I]->AsObject();
		TMap<int32, FVector2D> Centres;
		for (const TSharedPtr<FJsonValue>& C : S.GetArrayField(TEXT("centres"))) Centres.Add((int32)C->AsArray()[0]->AsNumber(), FVector2D(C->AsArray()[1]->AsNumber(), C->AsArray()[2]->AsNumber()));
		for (const TSharedPtr<FJsonValue>& R : S.GetArrayField(TEXT("cells")))
		{
			const TArray<TSharedPtr<FJsonValue>>& Row = R->AsArray();
			if (Row.Num() < 5 || Row[4]->Type != EJson::Object) continue;
			const TSharedPtr<FJsonObject> D = Row[4]->AsObject();
			double RailBits = 0, St = 0;
			D->TryGetNumberField(TEXT("rail"), RailBits);
			const bool bSt = D->TryGetNumberField(TEXT("station"), St);
			if (RailBits || bSt) Model.Cell_((int)Row[0]->AsNumber(), (int)RailBits, bSt, (int)St);
		}
		Model.Frame(S.GetNumberField(TEXT("dt")), Wave, [&](int R, double& X, double& Z) { const FVector2D* C = Centres.Find(R); if (!C) return false; X = C->X; Z = C->Y; return true; });
		const FJsonObject& O = *Outs[I]->AsObject();
		Bad += Differ(Flat(Model.BallastM), O.GetArrayField(TEXT("ballast"))) + Differ(Flat(Model.TiesM), O.GetArrayField(TEXT("ties"))) + Differ(Flat(Model.RailsM), O.GetArrayField(TEXT("rails")));
		Values += O.GetArrayField(TEXT("ballast")).Num() + O.GetArrayField(TEXT("ties")).Num() + O.GetArrayField(TEXT("rails")).Num();
		const TArray<TSharedPtr<FJsonValue>>& St = O.GetArrayField(TEXT("stations"));
		if (!TestEqual(FString::Printf(TEXT("step %d stations built"), I), (int32)Model.BuiltStations.size(), St.Num())) { Bad++; continue; }
		for (int32 K = 0; K < St.Num(); K++)
		{
			std::vector<double> Parts;
			for (const TCRailCore::FPart& P : Model.BuiltStations[K].Parts) Parts.insert(Parts.end(), P.M.E, P.M.E + 16);
			TArray<TSharedPtr<FJsonValue>> Want;
			for (const TSharedPtr<FJsonValue>& P : St[K]->AsObject()->GetArrayField(TEXT("parts"))) Want.Append(P->AsArray());
			Bad += Differ(Parts, Want);
			Values += Want.Num();
		}
	}
	AddInfo(FString::Printf(TEXT("railways: %d steps, %d matrix values compared with client/railways.js, %d differ; %d rebuilds"), Steps.Num(), Values, Bad, Model.Rebuilds));
	TestEqual(TEXT("railway values that differ from the reference"), Bad, 0);
	// at most two rebuilds a second: the fixture's 7 frames (2.016 s) rebuilt 3 times
	TestEqual(TEXT("rebuilds"), Model.Rebuilds, 3);

	// the view: the same track through a World start and snapshots, in a scratch world
	FScratchWorld Scratch(TEXT("TCRailwaysTest"));
	ATCTerrain* Terrain = Scratch.World->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity);
	if (!TestNotNull(TEXT("terrain"), Terrain)) return false;
	TSharedRef<FJsonObject> MapCase = MakeShared<FJsonObject>();
	const int32 W = (int32)Rail.GetNumberField(TEXT("w")), H = (int32)Rail.GetNumberField(TEXT("h"));
	TArray<TSharedPtr<FJsonValue>> Rows;
	for (int32 Y = 0; Y < H; Y++) Rows.Add(MakeShared<FJsonValueString>(FString::ChrN(W, TEXT('.'))));
	MapCase->SetNumberField(TEXT("w"), W); MapCase->SetNumberField(TEXT("h"), H); MapCase->SetStringField(TEXT("key"), TEXT("world")); MapCase->SetArrayField(TEXT("rows"), Rows);
	TArray<TSharedPtr<FJsonValue>> AllCells;
	for (const TSharedPtr<FJsonValue>& S : Steps) AllCells.Append(S->AsObject()->GetArrayField(TEXT("cells")));
	FTCMatchState State;
	const TSharedRef<FJsonObject> Start = StartOf(*MapCase, true, AllCells, TEXT("clear"));
	TestTrue(TEXT("match state takes the start"), State.ApplyStart(*Start));
	TestTrue(TEXT("a World map"), State.Map.bWorld);
	Terrain->ResetFrom(State);
	for (int32 I = 0; I < 64 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
	FTCRailways View;
	TestTrue(TEXT("railways attach"), View.Attach(Scratch.World));
	View.Reset(State);
	View.ApplyCells(*Start);
	TSharedRef<FJsonObject> Snap = MakeShared<FJsonObject>(), World = MakeShared<FJsonObject>();
	TArray<TSharedPtr<FJsonValue>> Regions;
	for (const TSharedPtr<FJsonValue>& C : Steps.Last()->AsObject()->GetArrayField(TEXT("centres")))
	{
		TSharedRef<FJsonObject> R = MakeShared<FJsonObject>();
		R->SetNumberField(TEXT("id"), C->AsArray()[0]->AsNumber()); R->SetNumberField(TEXT("x"), C->AsArray()[1]->AsNumber()); R->SetNumberField(TEXT("z"), C->AsArray()[2]->AsNumber());
		Regions.Add(MakeShared<FJsonValueObject>(R));
	}
	World->SetArrayField(TEXT("regions"), Regions);
	Snap->SetObjectField(TEXT("world"), World);
	View.ApplySnapshot(*Snap);
	View.Frame(0.016f, State, Terrain);
	const FJsonObject& Last = *Outs.Last()->AsObject();
	TestEqual(TEXT("halves drawn"), View.HalfCount(), Last.GetArrayField(TEXT("ballast")).Num() / 16);
	TestEqual(TEXT("sleepers drawn"), View.SleeperCount(), Last.GetArrayField(TEXT("ties")).Num() / 16);
	// region 2 is the only centre the last snapshot knows (as in the fixture's last step); 0 and 1 wait
	TestEqual(TEXT("stations built"), View.StationsBuilt(), 1);
	TestTrue(TEXT("stations still waiting"), View.StationsWaiting());
	const int32 Parts = (int32)TCRailCore::FModel::Station(0).Parts.size();
	TestEqual(TEXT("instances drawn (ballast, two rails a half, sleepers, one station)"), View.InstancesDrawn(), View.HalfCount() * 3 + View.SleeperCount() + Parts);
	TestTrue(TEXT("a track cell is known (props skip it)"), View.Has(View.TrackCells()[0]));
	// a three.js matrix and its Unreal transform put a box corner in the same place
	const TCRailCore::FMat4 M = TCRailCore::Compose(3, 1, 7, TCRailCore::QuatYXZ(0.2, 0.9, 0.1), 2.3, 0.14, 1.5);
	const double P[3] = { 0.5, 0.5, -0.5 };
	const double WX = M.E[0] * P[0] + M.E[4] * P[1] + M.E[8] * P[2] + M.E[12], WY = M.E[1] * P[0] + M.E[5] * P[1] + M.E[9] * P[2] + M.E[13], WZ = M.E[2] * P[0] + M.E[6] * P[1] + M.E[10] * P[2] + M.E[14];
	const FVector Got = FTCRailways::ToUnreal(M).TransformPosition(FVector(P[0] * 100, P[2] * 100, P[1] * 100));
	TestTrue(TEXT("matrix to Unreal"), Got.Equals(FVector(WX * 100, WZ * 100, WY * 100), 0.01));
	UStaticMesh* Box = FTCRailways::BuildMesh(GetTransientPackage(), false, nullptr);
	UStaticMesh* Tower = FTCRailways::BuildMesh(GetTransientPackage(), true, nullptr);
	TestTrue(TEXT("box mesh"), Box && Box->GetNumVertices(0) == 24 && Box->GetNumTriangles(0) == 12);
	TestTrue(TEXT("water tower mesh (10 sides, closed)"), Tower && Tower->GetNumTriangles(0) == 40);
	View.Detach();
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAtmosphereOracleTest, "ThreeCrossroads.World.AtmosphereOracle", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCAtmosphereOracleTest::RunTest(const FString& Parameters)
{
	using namespace TCAtmosTests;
	const FTCGraphicsLevelScope HighLevel(ETCGraphicsLevel::High); // round 15: the oracle is atmosphere.js on High
	FString Error;
	const TSharedPtr<FJsonObject> Root = LoadFixture(Error);
	if (!Root.IsValid()) { AddError(Error); return false; }
	int32 Bad = 0, Values = 0;
	{
		const std::vector<uint8_t> N = TCAtmosCore::NoiseTexture();
		Bad += Differ(std::vector<double>(N.begin(), N.end()), Root->GetArrayField(TEXT("noise")), true);
		Values += (int32)N.size();
	}
	const TCHAR* Kinds[3] = { TEXT("rain"), TEXT("snow"), TEXT("dust") };
	for (int32 K = 0; K < 3; K++)
	{
		std::vector<float> Pos, Rnd;
		TCAtmosCore::FallData(TCAtmosCore::Fall(K), Pos, Rnd);
		const FJsonObject& F = *Root->GetObjectField(TEXT("falls"))->GetObjectField(Kinds[K]);
		Bad += Differ(std::vector<double>(Pos.begin(), Pos.end()), F.GetArrayField(TEXT("position"))) + Differ(std::vector<double>(Rnd.begin(), Rnd.end()), F.GetArrayField(TEXT("rand")));
		Values += (int32)(Pos.size() + Rnd.size());
	}
	for (const TSharedPtr<FJsonValue>& V : Root->GetArrayField(TEXT("maps")))
	{
		const FJsonObject& M = *V->AsObject();
		const int32 W = (int32)M.GetNumberField(TEXT("w")), H = (int32)M.GetNumberField(TEXT("h"));
		std::vector<std::string> Rows;
		for (const TSharedPtr<FJsonValue>& R : M.GetArrayField(TEXT("rows"))) Rows.push_back(TCHAR_TO_UTF8(*R->AsString()));
		auto Ch = [&Rows](int X, int Y) { return Rows[Y][X]; };
		const std::vector<uint8_t> Mask = TCAtmosCore::GroundMask(W, H, Ch);
		Bad += Differ(std::vector<double>(Mask.begin(), Mask.end()), M.GetArrayField(TEXT("mask")), true);
		auto Sheets = [](const std::vector<TCAtmosCore::FSheet>& L) { std::vector<double> V; for (const auto& S : L) V.insert(V.end(), { S.X, S.Z, S.Y, S.Yaw, S.SX, S.SZ, S.Ph }); return V; };
		TArray<TSharedPtr<FJsonValue>> Mist, Banks;
		for (const TSharedPtr<FJsonValue>& S : M.GetArrayField(TEXT("mist"))) Mist.Append(S->AsArray());
		for (const TSharedPtr<FJsonValue>& S : M.GetArrayField(TEXT("banks"))) Banks.Append(S->AsArray());
		Bad += Differ(Sheets(TCAtmosCore::MistSites(W, H, Ch, 2.0, Wave, 36)), Mist) + Differ(Sheets(TCAtmosCore::BankSites(W * 2.0, H * 2.0, Wave, 64)), Banks);
		Values += (int32)Mask.size() + Mist.Num() + Banks.Num();
	}
	AddInfo(FString::Printf(TEXT("atmosphere: %d values (noise, falls, ground masks, mist and fog bank sites) compared with client/atmosphere.js, %d differ"), Values, Bad));
	TestEqual(TEXT("atmosphere values that differ from the reference"), Bad, 0);

	// the view in a scratch world: a dawn map in clear weather (river mist), then rain
	FScratchWorld Scratch(TEXT("TCAtmosphereTest"));
	ATCTerrain* Terrain = Scratch.World->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity);
	if (!TestNotNull(TEXT("terrain"), Terrain)) return false;
	const TSharedPtr<FJsonObject>* PegasusP = nullptr;
	for (const TSharedPtr<FJsonValue>& V : Root->GetArrayField(TEXT("maps"))) if (V->AsObject()->GetStringField(TEXT("key")) == TEXT("pegasus-bridge")) PegasusP = &V->AsObject();
	if (!TestNotNull(TEXT("pegasus-bridge in the fixture"), PegasusP)) return false;
	const FJsonObject& Pegasus = **PegasusP;
	FTCMatchState State;
	const TSharedRef<FJsonObject> Start = StartOf(Pegasus, false, {}, TEXT("clear"));
	TestTrue(TEXT("match state takes the start"), State.ApplyStart(*Start));
	Terrain->ResetFrom(State);
	for (int32 I = 0; I < 256 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
	FTCAtmosphere View;
	TestTrue(TEXT("atmosphere attaches"), View.Attach(Scratch.World));
	View.StartMatch(State, TEXT("pegasus-bridge"), Terrain);
	TestTrue(TEXT("materials loaded"), View.MaterialsLoaded());
	TestEqual(TEXT("mood"), View.Mood(), FName(TEXT("dawn")));
	TestEqual(TEXT("mist sheets as the reference places them"), View.MistSheets(), Pegasus.GetArrayField(TEXT("mist")).Num());
	View.Update(0.25f, State, Terrain, nullptr, FVector2D(90, 100));
	TestTrue(TEXT("the ground is mirrored for the cloud shade"), View.GroundSectionsMirrored() > 0);
	TestTrue(TEXT("cloud shade on"), View.CloudsOn());
	TestEqual(TEXT("nothing falls on a clear dawn"), View.FallKind(), FString());
	// a shower: rain falls and the ground gets wet as the snapshot's wx says
	State.Wx.bKnown = true; State.Wx.Rain = 0.8; State.Wx.Wet = 0.6; State.Wx.WindAngle = 1.0; State.Wx.WindStrength = 0.5;
	View.Update(0.25f, State, Terrain, nullptr, FVector2D(90, 100));
	TestEqual(TEXT("a shower's rain"), View.FallKind(), FString(TEXT("rain")));
	TestEqual(TEXT("rain points"), View.FallPoints(), TCAtmosCore::Fall(0).N);
	// High's composer blends in linear light; Low's direct path needs the atmosphere's output-space emulation.
	// Inspect the actual MIDs on both transitions, including rain created after a Low match has started.
	auto CheckBlend = [this](const FTCAtmosphere& V, bool bLinear)
	{
		for (const auto* M : { V.GroundMat.Get(), V.MistMat.Get(), V.BankMat.Get(), V.BirdMat.Get(), V.FallMat.Get() })
			if (M)
			{
				float Value = -1.f;
				TestTrue(TEXT("atmosphere blend parameter exists"), M->GetScalarParameterValue(FMaterialParameterInfo(TEXT("LinearBlend")), Value));
				TestEqual(TEXT("atmosphere blend follows the graphics path"), Value, bLinear ? 1.f : 0.f);
			}
	};
	CheckBlend(View, true);
	{
		FTCGraphicsLevelScope LowLevel(ETCGraphicsLevel::Low);
		View.Update(0.f, State, Terrain, nullptr, FVector2D(90, 100));
		CheckBlend(View, false);
		FTCAtmosphere LowView;
		LowView.Attach(Scratch.World);
		FTCMatchState LowState = State;
		LowState.Wx.Rain = LowState.Wx.Wet = 0;
		LowView.StartMatch(LowState, TEXT("pegasus-bridge"), Terrain);
		CheckBlend(LowView, false);
		LowState.Wx.Rain = 0.8;
		LowView.Update(0.25f, LowState, Terrain, nullptr, FVector2D(90, 100));
		TestEqual(TEXT("Low starts a new rain material"), LowView.FallKind(), FString(TEXT("rain")));
		CheckBlend(LowView, false);
		LowView.Detach();
	}
	View.Update(0.f, State, Terrain, nullptr, FVector2D(90, 100));
	CheckBlend(View, true);
	TestTrue(TEXT("wind from wx"), View.Wind().Equals(FVector2D(FMath::Cos(1.0) * 0.55, FMath::Sin(1.0) * 0.55), 1e-4));
	for (int32 I = 0; I < 200; I++) View.Update(0.1f, State, Terrain, nullptr, FVector2D(90, 100));
	TestTrue(TEXT("the ground is wet after the shower"), View.CoverOn() && View.Look().Wet > 0.55f && View.Look().Sheen > 0.55f);
	// the weather turns to snow: the rain fades out and snow falls
	State.Weather.Now = TEXT("snow"); State.Wx.Rain = 0; State.Wx.Wet = 0;
	for (int32 I = 0; I < 80; I++) View.Update(0.1f, State, Terrain, nullptr, FVector2D(90, 100));
	TestEqual(TEXT("snow after the turn"), View.FallKind(), FString(TEXT("snow")));
	TestTrue(TEXT("lying snow eases in"), View.Look().Cover > 0.9f);
	// Production adapter at the source's exact 3-second boundary, on both tiers.
	for (const ETCGraphicsLevel Level : { ETCGraphicsLevel::High, ETCGraphicsLevel::Low })
	{
		const FTCGraphicsLevelScope LevelScope(Level);
		State.Weather.Now = TEXT("rain");
		View.StartMatch(State, TEXT("pegasus-bridge"), Terrain);
		State.Weather.Now = TEXT("snow");
		for (int32 I = 0; I < 11; I++) View.Update(.25f, State, Terrain, nullptr, FVector2D(90, 100));
		TestEqual(TEXT("old rain remains at 2.75 seconds"), View.FallKind(), FString(TEXT("rain")));
		View.Update(.25f, State, Terrain, nullptr, FVector2D(90, 100));
		TestEqual(TEXT("snow replaces rain at 3 seconds"), View.FallKind(), FString(TEXT("snow")));
		TestEqual(TEXT("the replacement begins transparent"), View.FallA, 0.0);
	}
	// Source update swaps the whole ground geometry. A position-only native cache
	// misses a changed normal or index buffer with the same number of vertices.
	AActor* ProbeActor = Scratch.World->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity);
	if (!TestNotNull(TEXT("weather ground probe actor"), ProbeActor)) return false;
	UProceduralMeshComponent* Probe = NewObject<UProceduralMeshComponent>(ProbeActor);
	ProbeActor->SetRootComponent(Probe);
	ProbeActor->AddInstanceComponent(Probe);
	Probe->RegisterComponent();
	const TArray<FVector> ProbeVertices = { FVector(0,0,0), FVector(100,0,0), FVector(0,100,0), FVector(100,100,0) };
	const TArray<int32> FirstIndices = { 0,2,1, 1,2,3 }, NextIndices = { 0,2,3, 0,3,1 };
	TArray<FVector> ProbeNormals; ProbeNormals.Init(FVector::UpVector, 4);
	Probe->CreateMeshSection_LinearColor(0, ProbeVertices, FirstIndices, ProbeNormals, {}, {}, {}, false);
	View.AddGroundActor(ProbeActor);
	View.SyncGround(Terrain, true);
	const TWeakObjectPtr<UProceduralMeshComponent>* MirrorSlot = View.Overlays.Find(TWeakObjectPtr<UProceduralMeshComponent>(Probe));
	UProceduralMeshComponent* Mirror = MirrorSlot ? MirrorSlot->Get() : nullptr;
	if (!TestNotNull(TEXT("weather ground probe mirror"), Mirror)) return false;
	ProbeNormals.Init(FVector(0,.6,.8), 4);
	Probe->UpdateMeshSection_LinearColor(0, ProbeVertices, ProbeNormals, {}, {}, {});
	View.SyncGround(Terrain, true);
	const FProcMeshSection* Mirrored = Mirror->GetProcMeshSection(0);
	if (!TestNotNull(TEXT("mirrored weather section"), Mirrored)) return false;
	TestTrue(TEXT("weather snow/puddles consume changed ground normals"), Mirrored->ProcVertexBuffer[0].Normal.Equals(ProbeNormals[0]));
	Probe->CreateMeshSection_LinearColor(0, ProbeVertices, NextIndices, ProbeNormals, {}, {}, {}, false);
	View.SyncGround(Terrain, true);
	Mirrored = Mirror->GetProcMeshSection(0);
	TestTrue(TEXT("weather overlay follows changed triangle connections"), Mirrored && Mirrored->ProcIndexBuffer == Probe->GetProcMeshSection(0)->ProcIndexBuffer);
	Probe->SetVisibility(false);
	View.Update(0.f, State, Terrain, nullptr, FVector2D(90, 100));
	TestFalse(TEXT("weather does not expose a hidden source mesh"), Mirror->IsVisible());
	Probe->SetVisibility(true);
	View.Update(0.f, State, Terrain, nullptr, FVector2D(90, 100));
	TestTrue(TEXT("weather returns when its source is shown"), Mirror->IsVisible());
	const TWeakObjectPtr<UProceduralMeshComponent> RetiredSource(Probe);
	Probe->DestroyComponent();
	View.SyncGround(Terrain, true);
	TestFalse(TEXT("retired ground releases the registered weather mirror"), Mirror->IsRegistered());
	TestFalse(TEXT("retired ground leaves no stale mirror entry"), View.Overlays.Contains(RetiredSource));
	View.StopMatch();
	View.Detach();
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
