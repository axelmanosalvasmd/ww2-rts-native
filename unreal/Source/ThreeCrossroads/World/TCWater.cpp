#include "World/TCWater.h"
#include "World/TCFogFrontier.h"
#include "ThreeCrossroads.h"
#include "Net/TCMatchState.h"
#include "Client/TCGraphics.h"
#include "Engine/World.h"
#include "Engine/Texture2D.h"
#include "GameFramework/Actor.h"
#include "Materials/MaterialInterface.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "ProceduralMeshComponent.h"
#include "RenderUtils.h"
#include "TextureResource.h"

const TCHAR* const FTCWater::ReferencePin = TEXT("e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
const TCHAR* const FTCWater::MaterialPath = TEXT("/Game/Water/M_TCWater.M_TCWater");

namespace TCWaterView
{
	// A transient RGBA8 texture, raw bytes (no sRGB decode, as three's DataTexture), with a full mip chain when asked
	// (three generates mipmaps for the ripple table; the mask has none).
	UTexture2D* MakeTexture(int32 W, int32 H, const uint8* Rgba, bool bMips, bool bWrap)
	{
		UTexture2D* T = UTexture2D::CreateTransient(W, H, PF_R8G8B8A8);
		if (!T || !T->GetPlatformData() || !T->GetPlatformData()->Mips.Num()) return nullptr;
		TIndirectArray<FTexture2DMipMap>& Mips = T->GetPlatformData()->Mips;
		TArray<uint8> Level(Rgba, W * H * 4);
		int32 MW = W, MH = H;
		for (int32 M = 0; ; M++)
		{
			if (M > 0) Mips.Add(new FTexture2DMipMap(MW, MH, 1));
			FTexture2DMipMap& Mip = Mips[M];
			Mip.BulkData.Lock(LOCK_READ_WRITE);
			void* Dest = Mip.BulkData.Realloc(Level.Num());
			FMemory::Memcpy(Dest, Level.GetData(), Level.Num());
			Mip.BulkData.Unlock();
			if (!bMips || (MW == 1 && MH == 1)) break;
			// the next level: a 2 x 2 box, as WebGL's generateMipmap
			const int32 NW = FMath::Max(1, MW / 2), NH = FMath::Max(1, MH / 2);
			TArray<uint8> Next;
			Next.SetNumUninitialized(NW * NH * 4);
			for (int32 Y = 0; Y < NH; Y++) for (int32 X = 0; X < NW; X++) for (int32 K = 0; K < 4; K++)
			{
				const int32 X0 = FMath::Min(MW - 1, X * 2), X1 = FMath::Min(MW - 1, X * 2 + 1), Y0 = FMath::Min(MH - 1, Y * 2), Y1 = FMath::Min(MH - 1, Y * 2 + 1);
				const int32 Sum = Level[(Y0 * MW + X0) * 4 + K] + Level[(Y0 * MW + X1) * 4 + K] + Level[(Y1 * MW + X0) * 4 + K] + Level[(Y1 * MW + X1) * 4 + K];
				Next[(Y * NW + X) * 4 + K] = (uint8)((Sum + 2) / 4);
			}
			Level = MoveTemp(Next); MW = NW; MH = NH;
		}
		T->SRGB = false; // raw bytes, as three's DataTexture
		T->Filter = bMips ? TF_Trilinear : TF_Bilinear;
		T->AddressX = bWrap ? TA_Wrap : TA_Clamp;
		T->AddressY = bWrap ? TA_Wrap : TA_Clamp;
		T->NeverStream = true;
		T->LODGroup = TEXTUREGROUP_Pixels2D; // no LOD bias on data textures
		T->UpdateResource();
		return T;
	}

	// sends rows [Y0, Y1) x [X0, X1) of an RGBA8 image that is W texels wide
	void UpdateRect(UTexture2D* T, const uint8* Rgba, int32 W, int32 X0, int32 Y0, int32 X1, int32 Y1)
	{
		if (!T || X1 <= X0 || Y1 <= Y0) return;
		const int32 RW = X1 - X0, RH = Y1 - Y0;
		uint8* Copy = new uint8[RW * RH * 4];
		for (int32 Y = 0; Y < RH; Y++) FMemory::Memcpy(Copy + Y * RW * 4, Rgba + ((Y0 + Y) * W + X0) * 4, RW * 4);
		FUpdateTextureRegion2D* Region = new FUpdateTextureRegion2D(X0, Y0, 0, 0, RW, RH);
		T->UpdateTextureRegions(0, 1, Region, RW * 4, 4, Copy, [](uint8* Data, const FUpdateTextureRegion2D* Regions) { delete[] Data; delete Regions; });
	}

	AActor* SpawnHolder(UWorld* World, const TCHAR* Name, TStrongObjectPtr<UProceduralMeshComponent>& OutMesh)
	{
		if (!World) return nullptr;
		FActorSpawnParameters Params;
		Params.ObjectFlags |= RF_Transient;
		AActor* A = World->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity, Params);
		if (!A) return nullptr;
		UProceduralMeshComponent* M = NewObject<UProceduralMeshComponent>(A, Name);
		M->SetMobility(EComponentMobility::Movable);
		M->bUseAsyncCooking = false;
		M->SetCollisionEnabled(ECollisionEnabled::NoCollision); // picking stays analytic (ATCTerrain::RayToGround)
		M->SetCastShadow(false);
		A->SetRootComponent(M);
		A->AddInstanceComponent(M);
		M->RegisterComponent();
		OutMesh.Reset(M);
		return A;
	}

	// three (x, y up, z) metres -> Unreal (x, z, y) centimetres. The index order that faces up in three faces up here too
	// (the axis swap and the engine's winding rule cancel; ATCTerrain's fan uses the same rule).
	inline FVector ToUnreal(const float* P) { return FVector(P[0] * 100.0, P[2] * 100.0, P[1] * 100.0); }
	inline FVector NormalToUnreal(const float* N) { return FVector(N[0], N[2], N[1]); }
}

// ---------------------------------------------------------------------------------------------------- fog texture

FTCCellFog::FTCCellFog() = default;
FTCCellFog::~FTCCellFog() = default;

uint8 FTCCellFog::Level(const FTCMatchState& State, bool bRevealAll, int32 C)
{
	// ATCTerrain::CellColor's dimming: in sight 1, explored 0.55, never seen 0.32 (FTCWorldObjectsView uses the same bytes)
	return TCFogLook::Byte(State, bRevealAll, C); // round 14: the shared look (fog.js: frontier, lit cells)
}

void FTCCellFog::Reset(int32 InW, int32 InH)
{
	if (InW <= 0 || InH <= 0) { Tex.Reset(); Data.Reset(); W = H = 0; return; }
	if (Tex.IsValid() && W == InW && H == InH) return;
	W = InW; H = InH;
	Data.Init(82, W * H);
	UTexture2D* T = UTexture2D::CreateTransient(W, H, PF_G8);
	if (!T) return;
	T->SRGB = false;
	T->Filter = TF_Bilinear; // a soft step between cells, like the browser's fog texture
	T->AddressX = T->AddressY = TA_Clamp;
	T->NeverStream = true;
	T->UpdateResource();
	Tex.Reset(T);
	Upload();
}

bool FTCCellFog::Update(const FTCMatchState& State, bool bRevealAll, const TSet<int32>* Cells)
{
	if (!Tex.IsValid() || Data.Num() != W * H) return false;
	bool bChanged = false;
	auto Set = [&](int32 C) { const uint8 B = Level(State, bRevealAll, C); bChanged |= Data[C] != B; Data[C] = B; };
	if (Cells) { for (const int32 C : *Cells) if (Data.IsValidIndex(C)) Set(C); }
	else for (int32 C = 0; C < Data.Num(); C++) Set(C);
	if (bChanged) Upload();
	return bChanged;
}

void FTCCellFog::Upload()
{
	UTexture2D* T = Tex.Get();
	if (!T || !Data.Num()) return;
	uint8* Copy = new uint8[Data.Num()];
	FMemory::Memcpy(Copy, Data.GetData(), Data.Num());
	FUpdateTextureRegion2D* Region = new FUpdateTextureRegion2D(0, 0, 0, 0, W, H);
	T->UpdateTextureRegions(0, 1, Region, W, 1, Copy, [](uint8* D, const FUpdateTextureRegion2D* R) { delete[] D; delete R; });
}

// ---------------------------------------------------------------------------------------------------- water

FTCWater::FTCWater() = default;
FTCWater::~FTCWater() = default;

void FTCWater::Attach(UWorld* World)
{
	if (Actor.IsValid() || !World) return;
	AActor* A = TCWaterView::SpawnHolder(World, TEXT("Water"), Mesh);
	if (!A) return;
	Actor = A;
	// client/water.js: renderOrder -1, the first of the see-through pass (fog, smoke and effects draw over it)
	Mesh->SetTranslucentSortPriority(-1);
	Mesh->SetVisibility(false);
	if (UMaterialInterface* M = LoadObject<UMaterialInterface>(nullptr, MaterialPath)) Material.Reset(M);
	else { UE_LOG(LogTC, Error, TEXT("water: %s does not load (run unreal/Tools/water/make-water-materials.py); the water is not drawn"), MaterialPath); }
	if (!NoiseTex.IsValid())
	{
		const std::vector<uint8_t> Noise = TCWaterCore::NoiseTexture();
		NoiseTex.Reset(TCWaterView::MakeTexture(TCWaterCore::NoiseSize, TCWaterCore::NoiseSize, Noise.data(), true, true));
	}
}

void FTCWater::Detach()
{
	if (AActor* A = Actor.Get()) A->Destroy();
	Actor.Reset(); Mesh.Reset(); MID.Reset(); MaskTex.Reset();
	Core = TCWaterCore::FWater();
	W = H = 0; Triangles = 0;
}

void FTCWater::CopyCells(const FTCMatchState& State, const TSet<int32>* Cells)
{
	const FTCMap& Map = State.Map;
	auto One = [&](int32 C)
	{
		const bool bKnown = Map.Known.IsValidIndex(C) && Map.Known[C];
		Known[C] = bKnown;
		// an undiscovered World cell reads as the start map writes it: open ground, nothing else is read of it
		Chars[C] = bKnown && Map.Chars.IsValidIndex(C) ? Map.Chars[C] : '.';
		Levels[C] = bKnown && Map.Levels.IsValidIndex(C) ? Map.Levels[C] : 0;
		// own mines need dry ground (shared/world-layers.js), so 'N' is never water; anything not W, F or = is dry here
	};
	if (Cells) { for (const int32 C : *Cells) if (C >= 0 && C < W * H) One(C); }
	else for (int32 C = 0; C < W * H; C++) One(C);
}

void FTCWater::ResetFrom(const FTCMatchState& State)
{
	Core = TCWaterCore::FWater();
	Triangles = 0;
	W = State.Map.W; H = State.Map.H; bWorld = State.Map.bWorld;
	if (Mesh.IsValid()) { Mesh->ClearAllMeshSections(); Mesh->SetVisibility(false); }
	MaskTex.Reset(); MID.Reset();
	if (W <= 0 || H <= 0) { Chars.Reset(); Levels.Reset(); Known.Reset(); Fog.Reset(0, 0); return; }
	Chars.Init('.', W * H); Levels.Init(0, W * H); Known.Init(false, W * H);
	CopyCells(State, nullptr);
	Fog.Reset(W, H);
	bFogAll = true; FogCells.Reset();
	TryCreate(State);
	if (Core.Valid()) { UE_LOG(LogTC, Log, TEXT("water: %s %dx%d: %d triangles, mask %dx%d"), *State.Map.Name, W, H, Triangles, Core.MaskW(), Core.MaskH()); }
	else { UE_LOG(LogTC, Log, TEXT("water: %s %dx%d: no water known"), *State.Map.Name, W, H); }
}

void FTCWater::TryCreate(const FTCMatchState& State)
{
	Core = TCWaterCore::FWater();
	Core.Char = [this](int C) { return (uint8_t)Chars[C]; };
	if (bWorld) Core.Unknown = [this](int C) { return !Known[C]; };
	Core.GroundLevel = [this](int C) { return (int)Levels[C]; };
	if (GroundHeight) Core.GroundHeight = [this](double X, double Z) { return (double)GroundHeight((float)X, (float)Z); };
	// main.js: the water is made at a start, or when the first wet cell arrives; its levels are read then
	if (!Core.Create(W, H, [this](int C) { return (int)Levels[C]; }, [this](int C) { return (bool)Known[C]; })) return;
	SendMask(true);
	SendGeometry();
	BindMaterial();
}

void FTCWater::BindMaterial()
{
	if (!Mesh.IsValid() || !Material.IsValid()) return;
	if (!MID.IsValid())
	{
		UMaterialInstanceDynamic* D = UMaterialInstanceDynamic::Create(Material.Get(), GetTransientPackage());
		MID.Reset(D);
	}
	UMaterialInstanceDynamic* D = MID.Get();
	D->SetTextureParameterValue(TEXT("Mask"), MaskTex.Get());
	D->SetTextureParameterValue(TEXT("Noise"), NoiseTex.Get());
	D->SetTextureParameterValue(TEXT("Fog"), Fog.Texture());
	D->SetVectorParameterValue(TEXT("Size"), FLinearColor(W * TCWaterCore::Cell, H * TCWaterCore::Cell, 0, 0));
	D->SetScalarParameterValue(TEXT("FogGamma"), FogGamma);
	// round 15: water.js setLow (WATER_LOW on gfx.low: the drift frozen, the ripple read in place), again on every change
	GfxWatch.TakeChanged(); // subscribed before the level is read, so no change can fall between
	bDrawnLow = FTCGraphics::Get().IsLow();
	D->SetScalarParameterValue(TEXT("Low"), bDrawnLow ? 1.f : 0.f);
	for (int32 S = 0; S < Mesh->GetNumSections(); S++) Mesh->SetMaterial(S, D);
}

void FTCWater::SendMask(bool bAll)
{
	if (!Core.Valid()) return;
	const int32 MW = Core.MaskW(), MH = Core.MaskH();
	if (!MaskTex.IsValid() || MaskTex->GetSizeX() != MW || MaskTex->GetSizeY() != MH)
	{
		MaskTex.Reset(TCWaterView::MakeTexture(MW, MH, Core.Mask.data(), false, false));
		Core.MaskTiles.clear();
		if (MID.IsValid()) MID->SetTextureParameterValue(TEXT("Mask"), MaskTex.Get());
		return;
	}
	if (bAll) { TCWaterView::UpdateRect(MaskTex.Get(), Core.Mask.data(), MW, 0, 0, MW, MH); Core.MaskTiles.clear(); return; }
	// the tiles redone go up on their own (water.js upload); many of them: the whole mask at once
	if ((int32)Core.MaskTiles.size() * TCWaterCore::Tile * TCWaterCore::Sub > 2 * MH) TCWaterView::UpdateRect(MaskTex.Get(), Core.Mask.data(), MW, 0, 0, MW, MH);
	else for (const int T : Core.MaskTiles)
	{
		int X0, Y0, X1, Y1;
		Core.TileRect(T, X0, Y0, X1, Y1);
		TCWaterView::UpdateRect(MaskTex.Get(), Core.Mask.data(), MW, X0, Y0, X1, Y1);
	}
	Core.MaskTiles.clear();
}

void FTCWater::SendGeometry()
{
	Core.GeometryChanged = false;
	if (!Mesh.IsValid()) return;
	// Only the vertices a triangle uses are sent: most of the (W + 1)(H + 1) corner grid of a big map carries no water.
	const int32 NV = (int32)(Core.Pos.size() / 3);
	TArray<int32> Remap;
	Remap.Init(INDEX_NONE, NV);
	TArray<FVector> Verts;
	TArray<FVector> Normals;
	TArray<FVector2D> Flow;
	TArray<int32> Tris;
	Tris.Reserve((int32)Core.Index.size());
	for (const uint32_t I : Core.Index)
	{
		int32& R = Remap[(int32)I];
		if (R == INDEX_NONE)
		{
			R = Verts.Num();
			Verts.Add(TCWaterView::ToUnreal(&Core.Pos[(size_t)I * 3]));
			Normals.Add(TCWaterView::NormalToUnreal(&Core.Normal[(size_t)I * 3]));
			// UV0 carries the flow (water.js attribute 'flow', reference x and z), read by M_TCWater
			Flow.Add(FVector2D(Core.Flow[(size_t)I * 2], Core.Flow[(size_t)I * 2 + 1]));
		}
		Tris.Add(R);
	}
	Triangles = Tris.Num() / 3;
	if (!Tris.Num()) { Mesh->ClearAllMeshSections(); Mesh->SetVisibility(false); return; }
	Mesh->CreateMeshSection_LinearColor(0, Verts, Tris, Normals, Flow, TArray<FLinearColor>(), TArray<FProcMeshTangent>(), false);
	if (MID.IsValid()) Mesh->SetMaterial(0, MID.Get());
	Mesh->SetVisibility(true);
}

void FTCWater::MarkCells(const FTCMatchState& State, const TSet<int32>& Shaped, const TSet<int32>& Fogged)
{
	if (W <= 0 || State.Map.W != W || State.Map.H != H) return;
	FogCells.Append(Fogged);
	if (!Shaped.Num()) return;
	CopyCells(State, &Shaped);
	if (!Core.Valid())
	{
		// main.js applyCells: no water yet and a wet cell arrived: the water is made now
		for (const int32 C : Shaped) if (C >= 0 && C < W * H && TCWaterCore::IsWet(Chars[C])) { TryCreate(State); return; }
		return;
	}
	for (const int32 C : Shaped)
	{
		if (C < 0 || C >= W * H) continue;
		// createWaterLevels froze all lines when this mesh was made. Later discoveries keep their zero placeholder.
		// every cell row carries its level, so a changed cell is always a possible dent (water.js changed)
		Core.Changed(C, Chars[C], true);
	}
}

void FTCWater::Flush(const FTCMatchState& State, double NowSeconds, bool bRevealAll)
{
	if (W <= 0 || State.Map.W != W || State.Map.H != H) return;
	if (bRevealAll != bLastReveal) { bLastReveal = bRevealAll; bFogAll = true; }
	if (bFogAll) { Fog.Update(State, bRevealAll, nullptr); bFogAll = false; FogCells.Reset(); }
	else if (FogCells.Num()) { Fog.Update(State, bRevealAll, &FogCells); FogCells.Reset(); }
	if (!Core.Valid()) return;
	if (Core.Tick())
	{
		SendMask(false);
		if (Core.GeometryChanged) SendGeometry();
	}
	if (!MID.IsValid() && Material.IsValid()) BindMaterial();
	// round 15: gfx.onChange(setLow): a change of level mid-match redefines WATER_LOW at once
	if (GfxWatch.TakeChanged() && MID.IsValid() && FTCGraphics::Get().IsLow() != bDrawnLow)
	{
		bDrawnLow = FTCGraphics::Get().IsLow();
		MID->SetScalarParameterValue(TEXT("Low"), bDrawnLow ? 1.f : 0.f);
		LowSwitches++;
		UE_LOG(LogTC, Log, TEXT("water: graphics %s mid-match: the drift %s"), bDrawnLow ? TEXT("low") : TEXT("high"), bDrawnLow ? TEXT("frozen") : TEXT("runs again"));
	}
	// wraps every 1000 s, where every speed in the shader lands on a whole cycle (water.js tick); water.js tick only
	// moves uTime on High (if (!gfx.low)), so on Low the clock stands where it was
	if (MID.IsValid() && !bDrawnLow) { LastTime = (float)FMath::Fmod(NowSeconds, 1000.0); MID->SetScalarParameterValue(TEXT("Time"), LastTime); }
}

float FTCWater::WaterAt(float X, float Z) const
{
	return (float)Core.WaterAt(X, Z);
}

// ---------------------------------------------------------------------------------------------------- tests

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

namespace TCWaterTest
{
	constexpr EAutomationTestFlags Flags = EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter;

	// a water body over rows of characters (reference test-world.js: createWater(grid, map, () => 0)), every cell known
	struct FCase
	{
		int W = 0, H = 0;
		std::vector<uint8_t> Grid;
		std::vector<uint8_t> Known;
		TCWaterCore::FWater Water;
		// KnownColumns: on a World map, the cells known (x < KnownColumns); -1 every cell
		bool Make(const std::vector<std::string>& Rows, bool bWorld = false, int KnownColumns = -1)
		{
			H = (int)Rows.size(); W = (int)Rows[0].size();
			Grid.resize((size_t)W * H); Known.assign((size_t)W * H, 1);
			if (KnownColumns >= 0) for (int C = 0; C < W * H; C++) Known[C] = C % W < KnownColumns ? 1 : 0;
			for (int Y = 0; Y < H; Y++) for (int X = 0; X < W; X++) Grid[(size_t)Y * W + X] = (uint8_t)Rows[Y][X];
			Water.Char = [this](int C) { return Grid[C]; };
			if (bWorld) Water.Unknown = [this](int C) { return !Known[C]; };
			Water.GroundHeight = [](double, double) { return 0.0; };
			return Water.Create(W, H, [](int) { return 0; }, [](int) { return true; });
		}
		void Bounds(double& MinX, double& MinZ, double& MaxX, double& MaxZ) const
		{
			MinX = MinZ = 1e30; MaxX = MaxZ = -1e30;
			for (size_t I = 0; I < Water.Pos.size(); I += 3)
			{
				MinX = FMath::Min(MinX, (double)Water.Pos[I]); MaxX = FMath::Max(MaxX, (double)Water.Pos[I]);
				MinZ = FMath::Min(MinZ, (double)Water.Pos[I + 2]); MaxZ = FMath::Max(MaxZ, (double)Water.Pos[I + 2]);
			}
		}
		// the R (coverage) byte at texel (x * 4 + 2, y * 4 + 2): test-world.js coverage()
		int Coverage(int X, int Y) const { return Water.Mask[((size_t)(Y * 4 + 2) * W * 4 + X * 4 + 2) * 4]; }
	};
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWaterReferenceTest, "ThreeCrossroads.World.WaterReference", TCWaterTest::Flags)
bool FTCWaterReferenceTest::RunTest(const FString& Parameters)
{
	using namespace TCWaterTest;
	// test-world.js: every wet edge continues outwards, while a lake fully inside the map does not gain off-map water
	struct FExtents { const TCHAR* Name; std::vector<std::string> Rows; double Box[4]; };
	const FExtents Cases[] = {
		{ TEXT("sea"), { "WWWW", "WWWW", "WWWW", "WWWW" }, { -900, -900, 908, 908 } },
		{ TEXT("river"), { "...W....", "...W....", "...W....", "...W...." }, { 0, -900, 16, 908 } },
		{ TEXT("corner"), { "W...", "....", "....", "...." }, { -900, -900, 8, 8 } },
		{ TEXT("lake"), { "........", "........", "........", "...WW...", "...WW...", "........", "........", "........" }, { 0, 0, 16, 16 } } };
	for (const FExtents& E : Cases)
	{
		FCase C;
		TestTrue(FString::Printf(TEXT("%s: water made"), E.Name), C.Make(E.Rows));
		double B[4];
		C.Bounds(B[0], B[1], B[2], B[3]);
		for (int K = 0; K < 4; K++) TestEqual(FString::Printf(TEXT("%s: extent %d"), E.Name, K), B[K], E.Box[K]);
		bool bFinite = true;
		for (const float V : C.Water.Pos) bFinite &= FMath::IsFinite(V);
		for (const float V : C.Water.Flow) bFinite &= FMath::IsFinite(V);
		TestTrue(FString::Printf(TEXT("%s: finite water geometry"), E.Name), bFinite);
		// every non-degenerate triangle is wound upwards (three's axes: (b - a) x (c - a) has y >= 0)
		int Down = 0;
		for (size_t I = 0; I + 2 < C.Water.Index.size(); I += 3)
		{
			const float* A = &C.Water.Pos[(size_t)C.Water.Index[I] * 3]; const float* Bv = &C.Water.Pos[(size_t)C.Water.Index[I + 1] * 3]; const float* Cv = &C.Water.Pos[(size_t)C.Water.Index[I + 2] * 3];
			const double Ux = Bv[0] - A[0], Uz = Bv[2] - A[2], Vx = Cv[0] - A[0], Vz = Cv[2] - A[2];
			if (Uz * Vx - Ux * Vz < -1e-6) Down++;
		}
		TestEqual(FString::Printf(TEXT("%s: triangles facing down"), E.Name), Down, 0);
	}
	{
		// scouting finds more water: the mask is redone only near it and comes out as a fresh build draws it
		const int W = 64;
		std::vector<std::string> Rows(W, std::string(W, '.'));
		for (int Y = 0; Y < 12; Y++) Rows[Y][20] = 'W';
		FCase A;
		A.Make(Rows);
		for (int Y = 12; Y < 40; Y++) { const uint8_t Ch = Y == 30 ? 'F' : Y == 35 ? '=' : 'W'; A.Grid[(size_t)Y * W + 20] = Ch; Rows[Y][20] = (char)Ch; A.Water.Changed(Y * W + 20, Ch, false); }
		for (int X = 40; X < 64; X++) { A.Grid[(size_t)48 * W + X] = 'W'; Rows[48][X] = 'W'; A.Water.Changed(48 * W + X, 'W', false); }
		A.Water.Tick();
		FCase Fresh;
		Fresh.Make(Rows);
		TestTrue(TEXT("found water updates the mask exactly as a fresh build draws it"), A.Water.Mask == Fresh.Water.Mask);
		TestTrue(TEXT("the blur reaches across a tile edge"), A.Water.Mask[((size_t)(48 * 4 - 1) * W * 4 + 50 * 4) * 4] > 50);
	}
	{
		// World Conquest: a river running into undiscovered ground carries on under the fog instead of ending in a beach;
		// once the ground past it is discovered as dry land, the shore is drawn there
		const int W = 32;
		std::vector<std::string> Rows(W, std::string(W, '.'));
		for (int X = 0; X < 12; X++) Rows[16][X] = 'W';
		FCase Fogged, Beach;
		Fogged.Make(Rows, true, 12); // explored: the first 12 columns
		Beach.Make(Rows);
		TestTrue(FString::Printf(TEXT("the river runs on into undiscovered ground: %d against %d with a shore"), Fogged.Coverage(12, 16), Beach.Coverage(12, 16)), Fogged.Coverage(12, 16) > 200 && Beach.Coverage(12, 16) < 100);
		for (int C = 0; C < W * W; C++) Fogged.Known[C] = 1;
		Fogged.Water.Changed(16 * W + 12, '.', false);
		Fogged.Water.Tick();
		TestTrue(TEXT("discovered dry land gets its shore"), Fogged.Coverage(13, 16) < 100);
	}
	{
		// removing a wet edge cell removes its continuation on the next water tick
		FCase C;
		C.Make({ ".W..", ".W..", "....", "...." });
		C.Grid[1] = C.Grid[5] = '.';
		C.Water.Changed(1, '.', false); C.Water.Changed(5, '.', false);
		C.Water.Tick();
		double B[4];
		C.Bounds(B[0], B[1], B[2], B[3]);
		TestEqual(TEXT("no water left past the north edge"), B[1], 0.0);
	}
	{
		// frozen lines: a dent in the bed or a flooded crater never moves the body's line (water-levels.js)
		FCase C;
		C.Make({ ".W.", ".W.", ".W." });
		const double Line = C.Water.WaterAt(3, 3);
		C.Grid[3] = 'F'; // a crater beside the river floods into a ford, its level dug down a step
		C.Water.Changed(3, 'F', true);
		C.Water.Tick();
		TestEqual(TEXT("the line stays where the frozen levels put it"), C.Water.WaterAt(1, 3), Line);
		TestEqual(TEXT("the line is the level plus WATER_LIFT"), Line, TCWaterCore::Lift);
	}
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
