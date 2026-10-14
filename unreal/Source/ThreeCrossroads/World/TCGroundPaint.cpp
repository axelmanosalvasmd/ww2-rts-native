#include "World/TCGroundPaint.h"
#include "Client/TCGraphics.h"
#include "ThreeCrossroads.h"
#include "Net/TCMatchState.h"
#include "World/TCFogFrontier.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Engine/Texture2D.h"
#include "Engine/World.h"
#include "GameFramework/Actor.h"
#include "Components/SceneComponent.h"
#include "Materials/MaterialInterface.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "ProceduralMeshComponent.h"
#include "TextureResource.h"
#include "ImageCore.h"
#include "HAL/PlatformTime.h"
#include "World/TCGroundPaint.Tiles.inl"

const TCHAR* const FTCGroundPaint::ReferencePin = TEXT("e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
const TCHAR* const FTCGroundPaint::GroundMaterialPath = TEXT("/Game/Terrain/Materials/M_TCGroundPaint.M_TCGroundPaint");
const TCHAR* const FTCGroundPaint::CraterMaterialPath = TEXT("/Game/Terrain/Materials/M_TCCraterDecal.M_TCCraterDecal");
const TCHAR* const FTCGroundPaint::RubbleMaterialPath = TEXT("/Game/Terrain/Materials/M_TCRubbleDecal.M_TCRubbleDecal");


namespace TCGroundPaintImpl
{
	using namespace TCGroundPaintCore;
	constexpr int32 MaxSide = 4096;
	// client/fog.js: a change of look completes within a quarter second; the native looks are multipliers
	constexpr float FogSeen = 1.f, FogExplored = 0.55f, FogDark = 0.32f;
	constexpr float FogRate = (FogSeen - FogDark) / 0.25f;
	static_assert(FogSeen > FogExplored && FogExplored > FogDark, "TCFogLook::Multiplier returns these for the three looks");
	constexpr int32 RubbleCapacity = 2048, CraterCapacity = 1536; // rubble-decals.js, crater-decals.js CAPACITY

	bool Num(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return FMath::IsFinite(Out);
	}
	uint8 CharOf(const FString& S) { return S.Len() ? (uint8)FMath::Clamp((int32)S[0], 0, 127) : 0; }
	std::string Narrow(const FString& S) { return std::string(TCHAR_TO_UTF8(*S)); }
	std::string RowString(const TSharedPtr<FJsonValue>& V)
	{
		FString S;
		if (!V.IsValid() || !V->TryGetString(S)) return std::string();
		std::string Out((size_t)S.Len(), '.');
		for (int32 I = 0; I < S.Len(); I++) Out[(size_t)I] = (char)FMath::Clamp((int32)S[I], 0, 127);
		return Out;
	}
	void ReadRows(const FJsonObject& O, const TCHAR* Field, int32 H, std::vector<std::string>& Out)
	{
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (!O.TryGetArrayField(Field, A) || A->Num() != H) return;
		for (const TSharedPtr<FJsonValue>& V : *A) Out.push_back(RowString(V));
	}
	void ReadXY(const FJsonObject& JMap, const TCHAR* Field, std::vector<FStartMap::FXY>& Out)
	{
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (!JMap.TryGetArrayField(Field, A)) return;
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			FStartMap::FXY P;
			const TSharedPtr<FJsonObject>* O;
			double X, Y;
			if (V.IsValid() && V->TryGetObject(O) && O->IsValid() && (*O)->TryGetNumberField(TEXT("x"), X) && (*O)->TryGetNumberField(TEXT("y"), Y)) { P.X = (int)X; P.Y = (int)Y; }
			else P.bNull = true; // ground.js fieldKeep: spawns.filter(Boolean)
			Out.push_back(P);
		}
	}
	void ReadMaterials(const FJsonObject& Layers, const TCHAR* Field, std::vector<std::pair<int, std::string>>& Out)
	{
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (!Layers.TryGetArrayField(Field, A)) return;
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const TSharedPtr<FJsonObject>* O;
			double C;
			FString M;
			if (V.IsValid() && V->TryGetObject(O) && O->IsValid() && (*O)->TryGetNumberField(TEXT("c"), C) && (*O)->TryGetStringField(TEXT("material"), M)) Out.emplace_back((int)C, Narrow(M));
		}
	}
	// sRGB <-> linear for the mip chain (filtered in linear light)
	struct FGamma
	{
		float ToLinear[256];
		uint8 ToSrgb[4096];
		FGamma()
		{
			for (int32 I = 0; I < 256; I++) { const double U = I / 255.0; ToLinear[I] = (float)(U <= 0.04045 ? U / 12.92 : FMath::Pow((U + 0.055) / 1.055, 2.4)); }
			for (int32 I = 0; I < 4096; I++) { const double L = I / 4095.0, S = L <= 0.0031308 ? L * 12.92 : 1.055 * FMath::Pow(L, 1.0 / 2.4) - 0.055; ToSrgb[I] = (uint8)FMath::Clamp((int32)FMath::RoundToInt(S * 255.0), 0, 255); }
		}
		uint8 Encode(float L) const { return ToSrgb[FMath::Clamp((int32)(L * 4095.f + 0.5f), 0, 4095)]; }
		static const FGamma& Get() { static const FGamma G; return G; }
	};
}

FTCGroundPaint::FTCGroundPaint()
{
	Core.Mode = TCGroundPaintCore::EKnownMode::Native;
	Core.bDeferred = true;
}

FTCGroundPaint::~FTCGroundPaint() = default;

bool FTCGroundPaint::Attach(UWorld* World)
{
	LoadAssets();
	if (Actor.IsValid()) return true;
	if (!World) return false;
	FActorSpawnParameters Params;
	Params.ObjectFlags |= RF_Transient;
	AActor* A = World->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity, Params);
	if (!A) return false;
	USceneComponent* Root = NewObject<USceneComponent>(A, TEXT("GroundPaint"));
	Root->SetMobility(EComponentMobility::Movable);
	A->SetRootComponent(Root);
	A->AddInstanceComponent(Root);
	Root->RegisterComponent();
	UProceduralMeshComponent* Mesh = NewObject<UProceduralMeshComponent>(A, TEXT("GroundDecals"));
	Mesh->bUseAsyncCooking = false;
	Mesh->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	Mesh->SetCastShadow(false); // ground marks: they take the light and shadows, they cast none
	Mesh->SetupAttachment(Root);
	Mesh->RegisterComponent();
	A->AddInstanceComponent(Mesh);
	Actor = A;
	DecalMesh = Mesh;
	bDecalMeshDirty = true;
	RubbleSectionVerts = CraterSectionVerts = -1;
	return true;
}

void FTCGroundPaint::Detach()
{
	if (AActor* A = Actor.Get()) A->Destroy();
	Actor.Reset(); DecalMesh.Reset();
	PaintTex.Reset(); FogTex.Reset(); CraterMid.Reset(); RubbleMid.Reset();
	GroundMat.Reset(); CraterMat.Reset(); RubbleMat.Reset();
	Bound.Reset();
	bAssetsLoaded = false;
}

void FTCGroundPaint::LoadAssets()
{
	if (bAssetsLoaded) return;
	bAssetsLoaded = true;
	GroundMat.Reset(LoadObject<UMaterialInterface>(nullptr, GroundMaterialPath));
	CraterMat.Reset(LoadObject<UMaterialInterface>(nullptr, CraterMaterialPath));
	RubbleMat.Reset(LoadObject<UMaterialInterface>(nullptr, RubbleMaterialPath));
	if (!GroundMat.IsValid() || !CraterMat.IsValid() || !RubbleMat.IsValid())
		UE_LOG(LogTC, Error, TEXT("ground paint: missing materials under /Game/Terrain (ground %d, crater %d, rubble %d); run unreal/Tools/terrain/import-ground.py"),
			GroundMat.IsValid() ? 1 : 0, CraterMat.IsValid() ? 1 : 0, RubbleMat.IsValid() ? 1 : 0);
	if (CraterMat.IsValid()) CraterMid.Reset(UMaterialInstanceDynamic::Create(CraterMat.Get(), GetTransientPackage()));
	if (RubbleMat.IsValid()) { RubbleMid.Reset(UMaterialInstanceDynamic::Create(RubbleMat.Get(), GetTransientPackage())); Bind(RubbleMid.Get()); }
}

bool FTCGroundPaint::LoadTiles(int32 P, TCGroundPaintCore::FTile Out[TCGroundPaintCore::MatCount - 1])
{
	using namespace TCGroundPaintCore;
	// World/TCGroundPaint.Tiles.inl: buildTiles' output as the browser made it (unreal/Tools/terrain/make-ground-tiles.mjs)
	for (int32 M = 0; M < MatCount - 1; M++)
	{
		const int32 T = TileSize(M, P);
		const TCGroundPaintTiles::FEntry* Found = nullptr;
		for (const TCGroundPaintTiles::FEntry& E : TCGroundPaintTiles::Entries) if (E.Material == M && E.Size == T) Found = &E;
		if (!Found) { TileError = FString::Printf(TEXT("no %s tile of %d pixels"), UTF8_TO_TCHAR(Look(M).Tex), T); return false; }
		Out[M].T = T;
		Out[M].Data.resize((size_t)T * T * 4);
		for (int32 I = 0; I < T * T; I++)
		{
			Out[M].Data[(size_t)I * 4] = Found->Rgb[I * 3];
			Out[M].Data[(size_t)I * 4 + 1] = Found->Rgb[I * 3 + 1];
			Out[M].Data[(size_t)I * 4 + 2] = Found->Rgb[I * 3 + 2];
			Out[M].Data[(size_t)I * 4 + 3] = 255;
		}
	}
	TileError.Reset();
	return true;
}

// Slice 13 lead glue (see the header): the tiles LoadTiles gives the paint at this map's pixels per cell
bool FTCGroundPaint::ApronTiles(TArray<TArray<uint8>>& OutRgba, TArray<int32>& OutSizes)
{
	using namespace TCGroundPaintCore;
	OutRgba.Reset(); OutSizes.Reset();
	if (!HasMap() || Core.PixelsPerCell() <= 0) return false;
	FTile Tiles[MatCount - 1];
	if (!LoadTiles(Core.PixelsPerCell(), Tiles)) return false;
	for (int32 M = 0; M < 6; M++)
	{
		OutSizes.Add(Tiles[M].T);
		OutRgba.Emplace(Tiles[M].Data.data(), (int32)Tiles[M].Data.size());
	}
	return true;
}

void FTCGroundPaint::ReadCellRow(const TArray<TSharedPtr<FJsonValue>>& Row, TCGroundPaintCore::FCellRow& Out, int32 Cells)
{
	using namespace TCGroundPaintImpl;
	// [cell, char, level, state, data] (shared/sim.js logCell, seenWorldCell), read as client/main.js reads it
	Out = TCGroundPaintCore::FCellRow();
	double C;
	FString Ch;
	if (Row.Num() < 2 || !Num(Row[0], C) || C < 0 || C >= Cells || C != FMath::FloorToDouble(C) || !Row[1].IsValid() || !Row[1]->TryGetString(Ch) || Ch.IsEmpty()) return;
	Out.C = (int)C;
	Out.Ch = CharOf(Ch);
	double V;
	if (Row.Num() > 2 && Num(Row[2], V)) { Out.bLevel = true; Out.Level = FMath::Clamp((int32)V, -127, 127); }
	if (Row.Num() > 3 && Num(Row[3], V)) { Out.bState = true; Out.State = (int32)V & 255; }
	const TSharedPtr<FJsonObject>* Data;
	if (Row.Num() > 4 && Row[4].IsValid() && Row[4]->TryGetObject(Data) && Data->IsValid())
	{
		Out.bData = true;
		FString S;
		if ((*Data)->TryGetStringField(TEXT("ground"), S)) Out.Ground = CharOf(S);
		if ((*Data)->TryGetStringField(TEXT("object"), S)) Out.Object = CharOf(S);
		double B;
		if ((*Data)->TryGetNumberField(TEXT("biome"), B)) Out.Biome = FMath::Clamp((int32)B, 0, 255);
		if ((*Data)->TryGetStringField(TEXT("material"), S)) { Out.bMaterial = true; Out.Material = Narrow(S); }
	}
}

bool FTCGroundPaint::ApplyStart(const FJsonObject& M, const FTCMatchState& State)
{
	using namespace TCGroundPaintImpl;
	LoadAssets();
	const TSharedPtr<FJsonObject>* MapObj;
	if (!M.TryGetObjectField(TEXT("map"), MapObj) || !MapObj->IsValid()) return false;
	const FJsonObject& JMap = **MapObj;
	double Wd = 0, Hd = 0;
	const TArray<TSharedPtr<FJsonValue>>* Rows;
	if (!JMap.TryGetNumberField(TEXT("w"), Wd) || !JMap.TryGetNumberField(TEXT("h"), Hd) || !JMap.TryGetArrayField(TEXT("rows"), Rows)) return false;
	if (Wd < 1 || Hd < 1 || Wd > MaxSide || Hd > MaxSide || Rows->Num() != (int32)Hd) return false;
	FStartMap Map;
	Map.W = (int)Wd; Map.H = (int)Hd;
	Map.bWorld = JMap.HasField(TEXT("world"));
	for (const TSharedPtr<FJsonValue>& V : *Rows) Map.Rows.push_back(RowString(V));
	ReadRows(JMap, TEXT("heights"), Map.H, Map.Heights);
	const TSharedPtr<FJsonObject>* Layers;
	if (JMap.TryGetObjectField(TEXT("layers"), Layers) && Layers->IsValid())
	{
		ReadRows(**Layers, TEXT("ground"), Map.H, Map.LayerGround);
		ReadRows(**Layers, TEXT("objects"), Map.H, Map.LayerObjects);
		ReadMaterials(**Layers, TEXT("materials"), Map.LayerMaterials);
		ReadMaterials(**Layers, TEXT("objectMaterials"), Map.LayerObjectMaterials);
	}
	ReadXY(JMap, TEXT("spawns"), Map.Spawns);
	ReadXY(JMap, TEXT("points"), Map.Points);
	const TArray<TSharedPtr<FJsonValue>>* Biome;
	if (JMap.TryGetArrayField(TEXT("biome"), Biome) && Biome->Num() == Map.W * Map.H)
		for (const TSharedPtr<FJsonValue>& V : *Biome) { double B; Map.Biome.push_back(Num(V, B) ? FMath::Clamp((int32)B, 0, 255) : 255); }
	std::vector<FCellRow> Cells;
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	if (M.TryGetArrayField(TEXT("cells"), Arr))
		for (const TSharedPtr<FJsonValue>& Entry : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			FCellRow R;
			if (Entry.IsValid() && Entry->TryGetArray(Row)) ReadCellRow(*Row, R, Map.W * Map.H);
			if (R.C >= 0) Cells.push_back(R);
		}

	const double T0 = FPlatformTime::Seconds();
	// round 15: ground.js createGround reads gfx.low once, at the start: 1280 texels across the map on Low (2048 on
	// High). It does not watch gfx.onChange, so a change of level mid-match paints at the next start, as in the browser.
	bStartedLow = FTCGraphics::Get().IsLow();
	const int32 P = FGround::PixelsPerCell(Map.W, Map.H, Map.bWorld, bStartedLow);
	FTile Tiles[MatCount - 1];
	const bool bTiles = LoadTiles(P, Tiles);
	if (!bTiles) UE_LOG(LogTC, Error, TEXT("ground paint: %s; the ground is painted in the materials' flat colours (as the browser before its pictures arrive)"), *TileError);
	Core.Start(Map, Cells, bTiles ? Tiles : nullptr, bStartedLow);
	LastPaintMs = (FPlatformTime::Seconds() - T0) * 1000.0;
	MakeTextures();
	// fog: every cell takes its look at once (client/fog.js starts at the wanted alpha)
	const int32 N = Map.W * Map.H;
	FogWant.Init(1.f, N); FogNow.Init(1.f, N); FogBytes.Init(255, N); FogBusy.Init(false, N);
	FogActive.Reset(); FogPending.Reset();
	bFogAll = true; bLastReveal = false;
	// decals: every known rubble cell (and World crater) on the next Tick, on the ground as it is drawn then
	Rubble.Init(RubbleCapacity, true); Craters.Init(CraterCapacity, false);
	DecalChanged.Reset(); bDecalsAll = true; bDecalMeshDirty = true;
	Starts++; RowsApplied += (int32)Cells.size();
	UE_LOG(LogTC, Log, TEXT("ground paint: %s %dx%d, %d px a cell (graphics %s, %d texels across; %dx%d canvas), %s, %d start cells, painted in %.0f ms; %d known cells"),
		*State.Map.Name, Map.W, Map.H, P, bStartedLow ? TEXT("low") : TEXT("high"), bStartedLow ? 1280 : 2048, Core.CanvasW(), Core.CanvasH(), Core.Textured() ? TEXT("textured") : TEXT("flat colours"), (int32)Cells.size(), LastPaintMs,
		[&]() { int32 K = 0; for (int32 C = 0; C < N; C++) K += Core.IsKnown(C) ? 1 : 0; return K; }());
	return true;
}

void FTCGroundPaint::ApplySnapshot(const FJsonObject& M)
{
	if (!HasMap()) return;
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	if (!M.TryGetArrayField(TEXT("cells"), Arr) || !Arr->Num()) return;
	const int32 W = Core.MapW(), H = Core.MapH();
	std::vector<TCGroundPaintCore::FCellRow> Rows;
	for (const TSharedPtr<FJsonValue>& Entry : *Arr)
	{
		const TArray<TSharedPtr<FJsonValue>>* Row;
		TCGroundPaintCore::FCellRow R;
		if (Entry.IsValid() && Entry->TryGetArray(Row)) ReadCellRow(*Row, R, W * H);
		if (R.C >= 0) Rows.push_back(R);
	}
	if (Rows.empty()) return;
	const double T0 = FPlatformTime::Seconds();
	Core.Apply(Rows);
	LastPaintMs = (FPlatformTime::Seconds() - T0) * 1000.0;
	Snapshots++; RowsApplied += (int32)Rows.size();
	// client/main.js applyCells: the decals look at the changed cells, and at rubble round any cell whose level moved
	TSet<int32> Seen;
	for (const auto& R : Rows) if (!Seen.Contains(R.C)) { Seen.Add(R.C); DecalChanged.Add(R.C); }
	for (const auto& R : Rows) if (R.bLevel)
	{
		const int32 X = R.C % W, Y = R.C / W;
		for (int32 Dz = -1; Dz <= 1; Dz++) for (int32 Dx = -1; Dx <= 1; Dx++)
		{
			const int32 Nx = X + Dx, Ny = Y + Dz, C = Ny * W + Nx;
			if (Nx >= 0 && Nx < W && Ny >= 0 && Ny < H && Core.ObjectAt(C) == 'R' && !Seen.Contains(C)) { Seen.Add(C); DecalChanged.Add(C); }
		}
	}
}

void FTCGroundPaint::Clear()
{
	Core = TCGroundPaintCore::FGround();
	Core.Mode = TCGroundPaintCore::EKnownMode::Native;
	Core.bDeferred = true;
	Rubble.Init(TCGroundPaintImpl::RubbleCapacity, true); Craters.Init(TCGroundPaintImpl::CraterCapacity, false);
	DecalChanged.Reset(); bDecalsAll = false; bDecalMeshDirty = true;
	Mips.Reset(); MipSizes.Reset();
	FogWant.Reset(); FogNow.Reset(); FogBytes.Reset(); FogActive.Reset(); FogPending.Reset();
	PaintTex.Reset(); FogTex.Reset();
	RebindAll();
	if (UProceduralMeshComponent* Mesh = DecalMesh.Get()) Mesh->ClearAllMeshSections();
	RubbleSectionVerts = CraterSectionVerts = -1;
}

void FTCGroundPaint::MakeTextures()
{
	using namespace TCGroundPaintImpl;
	const int32 CW = Core.CanvasW(), CH = Core.CanvasH(), W = Core.MapW(), H = Core.MapH();
	// the canvas: sRGB (client/ground.js CanvasTexture, SRGBColorSpace), mipmapped, anisotropic like the World group
	MipSizes.Reset();
	for (int32 Sx = CW, Sy = CH;; Sx = FMath::Max(1, Sx / 2), Sy = FMath::Max(1, Sy / 2))
	{
		MipSizes.Add(FIntPoint(Sx, Sy));
		if (Sx == 1 && Sy == 1) break;
	}
	Mips.SetNum(MipSizes.Num());
	for (int32 L = 1; L < MipSizes.Num(); L++) Mips[L].SetNumZeroed((int64)MipSizes[L].X * MipSizes[L].Y * 4);
	BuildMips(0, 0, CW, CH);
	UTexture2D* T = NewObject<UTexture2D>(GetTransientPackage(), NAME_None, RF_Transient);
	FTexturePlatformData* PD = new FTexturePlatformData();
	PD->SizeX = CW; PD->SizeY = CH; PD->SetNumSlices(1); PD->PixelFormat = PF_B8G8R8A8;
	T->SetPlatformData(PD);
	T->bNotOfflineProcessed = true;
	const std::vector<uint8_t>& Canvas = Core.Canvas();
	for (int32 L = 0; L < MipSizes.Num(); L++)
	{
		FTexture2DMipMap* Mip = new FTexture2DMipMap(MipSizes[L].X, MipSizes[L].Y, 1);
		PD->Mips.Add(Mip);
		const int64 Bytes = (int64)MipSizes[L].X * MipSizes[L].Y * 4;
		Mip->BulkData.Lock(LOCK_READ_WRITE);
		uint8* Dst = (uint8*)Mip->BulkData.Realloc(Bytes);
		const uint8* Src = L == 0 ? Canvas.data() : Mips[L].GetData();
		for (int64 I = 0; I < Bytes; I += 4) { Dst[I] = Src[I + 2]; Dst[I + 1] = Src[I + 1]; Dst[I + 2] = Src[I]; Dst[I + 3] = 255; }
		Mip->BulkData.Unlock();
	}
	T->SRGB = true;
	T->LODGroup = TEXTUREGROUP_World;
	T->Filter = TF_Default; // the World group's filter (anisotropic), as the reference's anisotropy 8
	T->AddressX = TA_Clamp; T->AddressY = TA_Clamp;
	T->NeverStream = true;
	T->UpdateResource();
	PaintTex.Reset(T);
	Core.TakeDirtyRects(); // the start's full paint is in the texture now

	// the fog: one texel per cell, linear, bilinear between cell centres (client/fog.js LinearFilter)
	UTexture2D* F = UTexture2D::CreateTransient(W, H, PF_G8);
	if (F)
	{
		F->SRGB = false;
		F->Filter = TF_Bilinear;
		F->AddressX = TA_Clamp; F->AddressY = TA_Clamp;
		F->NeverStream = true;
		F->UpdateResource();
	}
	FogTex.Reset(F);
	RebindAll();
}

// mip L+1 from mip L over the rectangle [X0, X1) x [Y0, Y1) of level 0, each texel the mean of its 2 x 2 in linear light
void FTCGroundPaint::BuildMips(int32 X0, int32 Y0, int32 X1, int32 Y1)
{
	const TCGroundPaintImpl::FGamma& G = TCGroundPaintImpl::FGamma::Get();
	const std::vector<uint8_t>& Canvas = Core.Canvas();
	for (int32 L = 1; L < MipSizes.Num(); L++)
	{
		X0 >>= 1; Y0 >>= 1; X1 = FMath::Min(MipSizes[L].X, (X1 + 1) >> 1); Y1 = FMath::Min(MipSizes[L].Y, (Y1 + 1) >> 1);
		const int32 PW = MipSizes[L - 1].X, PH = MipSizes[L - 1].Y, MW = MipSizes[L].X;
		const uint8* Src = L == 1 ? Canvas.data() : Mips[L - 1].GetData();
		uint8* Dst = Mips[L].GetData();
		for (int32 Y = Y0; Y < Y1; Y++) for (int32 X = X0; X < X1; X++)
		{
			float Sum[3] = { 0, 0, 0 };
			for (int32 K = 0; K < 4; K++)
			{
				const int32 Sx = FMath::Min(PW - 1, 2 * X + (K & 1)), Sy = FMath::Min(PH - 1, 2 * Y + (K >> 1));
				const uint8* Px = Src + ((int64)Sy * PW + Sx) * 4;
				for (int32 C = 0; C < 3; C++) Sum[C] += G.ToLinear[Px[C]];
			}
			uint8* Out = Dst + ((int64)Y * MW + X) * 4;
			for (int32 C = 0; C < 3; C++) Out[C] = G.Encode(Sum[C] * 0.25f);
			Out[3] = 255;
		}
	}
}

void FTCGroundPaint::UploadDirty()
{
	UTexture2D* T = PaintTex.Get();
	const std::vector<std::array<int, 4>> Rects = Core.TakeDirtyRects();
	if (!T || Rects.empty()) return;
	const double T0 = FPlatformTime::Seconds();
	int32 X0 = Core.CanvasW(), Y0 = Core.CanvasH(), X1 = 0, Y1 = 0;
	for (const auto& R : Rects) { X0 = FMath::Min(X0, R[0]); Y0 = FMath::Min(Y0, R[1]); X1 = FMath::Max(X1, R[2]); Y1 = FMath::Max(Y1, R[3]); }
	BuildMips(X0, Y0, X1, Y1);
	const std::vector<uint8_t>& Canvas = Core.Canvas();
	for (int32 L = 0; L < MipSizes.Num(); L++)
	{
		const int32 Lx0 = X0 >> L, Ly0 = Y0 >> L;
		const int32 Lx1 = FMath::Min(MipSizes[L].X, FMath::Max(Lx0 + 1, (X1 + (1 << L) - 1) >> L)), Ly1 = FMath::Min(MipSizes[L].Y, FMath::Max(Ly0 + 1, (Y1 + (1 << L) - 1) >> L));
		const int32 RW = Lx1 - Lx0, RH = Ly1 - Ly0, MW = MipSizes[L].X;
		if (RW <= 0 || RH <= 0) continue;
		uint8* Copy = new uint8[(size_t)RW * RH * 4];
		const uint8* Src = L == 0 ? Canvas.data() : Mips[L].GetData();
		for (int32 Y = 0; Y < RH; Y++) for (int32 X = 0; X < RW; X++)
		{
			const uint8* S = Src + ((int64)(Ly0 + Y) * MW + Lx0 + X) * 4;
			uint8* D = Copy + ((size_t)Y * RW + X) * 4;
			D[0] = S[2]; D[1] = S[1]; D[2] = S[0]; D[3] = 255;
		}
		FUpdateTextureRegion2D* Region = new FUpdateTextureRegion2D(Lx0, Ly0, 0, 0, RW, RH);
		T->UpdateTextureRegions(L, 1, Region, RW * 4, 4, Copy, [](uint8* Data, const FUpdateTextureRegion2D* Regions) { delete[] Data; delete Regions; });
	}
	LastUploadMs = (FPlatformTime::Seconds() - T0) * 1000.0;
}

void FTCGroundPaint::MarkFog(const TSet<int32>& Cells)
{
	FogPending.Append(Cells);
}

void FTCGroundPaint::FogTick(const FTCMatchState& State, float DeltaSeconds, bool bEndRevealed)
{
	using namespace TCGroundPaintImpl;
	const int32 W = Core.MapW(), H = Core.MapH(), N = W * H;
	UTexture2D* F = FogTex.Get();
	if (!F || FogWant.Num() != N) return;
	// round 14: the shared look (fog.js: World's frontier darkening and the cells under units lit until the next snapshot)
	auto Look = [&](int32 C) { return TCFogLook::Multiplier(State, bEndRevealed, C); };
	auto Touch = [&](int32 C)
	{
		const float T = Look(C);
		if (T == FogWant[C]) return;
		FogWant[C] = T;
		if (!FogBusy[C]) { FogBusy[C] = true; FogActive.Add(C); }
	};
	int32 Rx0 = W, Ry0 = H, Rx1 = -1, Ry1 = -1;
	auto Write = [&](int32 C)
	{
		const uint8 B = (uint8)FMath::Clamp(FMath::RoundToInt(FogNow[C] * 255.f), 0, 255);
		if (FogBytes[C] == B) return;
		FogBytes[C] = B;
		const int32 X = C % W, Y = C / W;
		Rx0 = FMath::Min(Rx0, X); Rx1 = FMath::Max(Rx1, X); Ry0 = FMath::Min(Ry0, Y); Ry1 = FMath::Max(Ry1, Y);
	};
	if (bFogAll)
	{
		// a start: every cell at its look at once
		for (int32 C = 0; C < N; C++) { FogWant[C] = FogNow[C] = Look(C); FogBytes[C] = (uint8)FMath::Clamp(FMath::RoundToInt(FogNow[C] * 255.f), 0, 255); }
		FogBusy.Init(false, N); FogActive.Reset(); FogPending.Reset();
		bFogAll = false; bLastReveal = bEndRevealed;
		Rx0 = 0; Ry0 = 0; Rx1 = W - 1; Ry1 = H - 1;
	}
	else
	{
		// the match decided (fog lifts on an authored map): every cell fades to clear
		if (bEndRevealed != bLastReveal) { bLastReveal = bEndRevealed; for (int32 C = 0; C < N; C++) Touch(C); }
		for (const int32 C : FogPending) if (C >= 0 && C < N) Touch(C);
		FogPending.Reset();
		const float Step = FogRate * FMath::Max(0.f, DeltaSeconds);
		for (int32 I = FogActive.Num() - 1; I >= 0; I--)
		{
			const int32 C = FogActive[I];
			const float T = FogWant[C], A = FogNow[C];
			const float Next = A < T ? FMath::Min(T, A + Step) : FMath::Max(T, A - Step);
			FogNow[C] = Next;
			Write(C);
			if (Next == T) { FogBusy[C] = false; FogActive.RemoveAtSwap(I, EAllowShrinking::No); }
		}
	}
	if (Rx1 < Rx0) return;
	const int32 RW = Rx1 - Rx0 + 1, RH = Ry1 - Ry0 + 1;
	uint8* Copy = new uint8[(size_t)RW * RH];
	for (int32 Y = 0; Y < RH; Y++) FMemory::Memcpy(Copy + (size_t)Y * RW, FogBytes.GetData() + (int64)(Ry0 + Y) * W + Rx0, RW);
	FUpdateTextureRegion2D* Region = new FUpdateTextureRegion2D(Rx0, Ry0, 0, 0, RW, RH);
	F->UpdateTextureRegions(0, 1, Region, RW, 1, Copy, [](uint8* Data, const FUpdateTextureRegion2D* Regions) { delete[] Data; delete Regions; });
}

void FTCGroundPaint::DecalTick(TFunctionRef<float(float, float)> GroundHeight)
{
	using namespace TCGroundPaintCore;
	const int32 W = Core.MapW(), H = Core.MapH(), N = W * H;
	if (!N) return;
	const std::function<double(double, double)> HAt = [&GroundHeight](double X, double Z) { return (double)GroundHeight((float)X, (float)Z); };
	auto RubbleWanted = [this](int C) { return Core.IsKnown(C) && Core.ObjectAt(C) == 'R'; };
	auto RubbleWrite = [&](int Slot, int C) { RubbleQuad(C, W, Core.MaterialAt(C), HAt, &Rubble.Pos[(size_t)Slot * 75], &Rubble.Uv[(size_t)Slot * 50]); };
	const std::function<uint8_t(int)> KnownChar = [this](int C) { return Core.IsKnown(C) ? Core.CharAt(C) : (uint8_t)0; };
	auto CraterWant = [&](int C) { return Core.IsKnown(C) && CraterWanted(C, W, H, KnownChar); };
	auto CraterWrite = [&](int Slot, int C) { CraterQuad(C, W, HAt, &Craters.Pos[(size_t)Slot * 75]); };
	if (bDecalsAll)
	{
		// client/main.js: the rubble decals for every rubble cell; World Conquest's lasting craters for every crater cell
		std::vector<int> AllR, AllC;
		for (int C = 0; C < N; C++) { if (Core.ObjectAt(C) == 'R') AllR.push_back(C); if (Core.CharAt(C) == '+') AllC.push_back(C); }
		bDecalMeshDirty |= Rubble.Update(AllR, RubbleWanted, RubbleWrite);
		if (Core.IsWorld()) bDecalMeshDirty |= Craters.Update(AllC, CraterWant, CraterWrite);
		bDecalsAll = false;
		DecalChanged.Reset();
	}
	else if (DecalChanged.Num())
	{
		std::vector<int> Changed(DecalChanged.GetData(), DecalChanged.GetData() + DecalChanged.Num());
		bDecalMeshDirty |= Rubble.Update(RubbleLook(Changed, N), RubbleWanted, RubbleWrite);
		if (Core.IsWorld()) bDecalMeshDirty |= Craters.Update(CraterLook(Changed, W, N), CraterWant, CraterWrite);
		DecalChanged.Reset();
	}
	UProceduralMeshComponent* Mesh = DecalMesh.Get();
	if (!Mesh || !bDecalMeshDirty) return;
	bDecalMeshDirty = false;
	// reference metres (x, height, z) -> Unreal cm (x, z, height); the index order stays (two swapped axes and the
	// change of handedness cancel, so the faces still point up)
	auto Section = [&](int32 Index, const FDecalSet& Set, UMaterialInterface* Mat, int32& SectionVerts)
	{
		const int32 Count = Set.Count();
		if (!Count) { if (SectionVerts != 0) Mesh->ClearMeshSection(Index); SectionVerts = 0; return; }
		TArray<FVector> Verts; TArray<int32> Tris; TArray<FVector> Normals; TArray<FVector2D> UV; TArray<FLinearColor> Colors;
		Verts.Reserve(Count * 25); Normals.Reserve(Count * 25); UV.Reserve(Count * 25); Tris.Reserve(Count * 96);
		for (int32 S = 0; S < Count; S++)
		{
			const int32 Base = Verts.Num();
			for (int32 V = 0; V < 25; V++)
			{
				const float* P = &Set.Pos[(size_t)(S * 25 + V) * 3];
				Verts.Add(FVector(P[0] * 100.f, P[2] * 100.f, P[1] * 100.f));
				Normals.Add(FVector(0, 0, 1));
				// a three.js texture is flipped on load (flipY): the reference's v runs up the picture
				if (Set.bUv) UV.Add(FVector2D(Set.Uv[(size_t)(S * 25 + V) * 2], Set.Uv[(size_t)(S * 25 + V) * 2 + 1]));
				else UV.Add(FVector2D((V % 5) / 4.0, (V / 5) / 4.0));
			}
			for (int32 Z = 0; Z < 4; Z++) for (int32 X = 0; X < 4; X++)
			{
				const int32 A = Base + Z * 5 + X;
				Tris.Append({ A, A + 5, A + 1, A + 1, A + 5, A + 6 });
			}
		}
		Mesh->CreateMeshSection_LinearColor(Index, Verts, Tris, Normals, UV, Colors, TArray<FProcMeshTangent>(), false);
		Mesh->SetMaterial(Index, Mat);
		SectionVerts = Verts.Num();
	};
	Section(0, Rubble, RubbleMid.IsValid() ? (UMaterialInterface*)RubbleMid.Get() : RubbleMat.Get(), RubbleSectionVerts);
	Section(1, Craters, CraterMid.IsValid() ? (UMaterialInterface*)CraterMid.Get() : CraterMat.Get(), CraterSectionVerts);
}

void FTCGroundPaint::Tick(const FTCMatchState& State, float DeltaSeconds, bool bEndRevealed, TFunctionRef<float(float, float)> GroundHeight)
{
	if (!HasMap()) return;
	// client/main.js createGround frames: a few queued tiles a frame, within the budget
	if (Core.PendingTiles())
	{
		const double Until = FPlatformTime::Seconds() + BudgetMs / 1000.0;
		const double T0 = FPlatformTime::Seconds();
		Core.PaintPending(FMath::Max(1, TilesPerFrame), [Until]() { return FPlatformTime::Seconds() >= Until; });
		LastPaintMs = (FPlatformTime::Seconds() - T0) * 1000.0;
	}
	UploadDirty();
	FogTick(State, DeltaSeconds, bEndRevealed);
	DecalTick(GroundHeight);
	Clock += DeltaSeconds;
	for (int32 I = Bound.Num() - 1; I >= 0; I--)
	{
		UMaterialInstanceDynamic* Mid = Bound[I].Get();
		if (!Mid) { Bound.RemoveAtSwap(I); continue; }
		Mid->SetScalarParameterValue(TEXT("GroundFogTime"), (float)Clock);
	}
}

UMaterialInterface* FTCGroundPaint::GroundMaterial() const { return GroundMat.Get(); }

UMaterialInstanceDynamic* FTCGroundPaint::MakeGroundMaterial(UObject* Outer)
{
	LoadAssets();
	if (!GroundMat.IsValid()) return nullptr;
	UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(GroundMat.Get(), Outer ? Outer : GetTransientPackage());
	Bind(Mid);
	return Mid;
}

void FTCGroundPaint::Bind(UMaterialInstanceDynamic* Mid)
{
	if (!Mid) return;
	bool bHave = false;
	for (const TWeakObjectPtr<UMaterialInstanceDynamic>& B : Bound) if (B.Get() == Mid) bHave = true;
	if (!bHave) Bound.Add(Mid);
	const int32 W = Core.MapW(), H = Core.MapH();
	if (UTexture2D* T = PaintTex.Get()) Mid->SetTextureParameterValue(TEXT("GroundPaint"), T);
	if (UTexture2D* F = FogTex.Get()) Mid->SetTextureParameterValue(TEXT("GroundFog"), F);
	Mid->SetVectorParameterValue(TEXT("GroundMapSize"), FLinearColor(FMath::Max(1, W) * TCGroundPaintCore::CellMetres, FMath::Max(1, H) * TCGroundPaintCore::CellMetres, 0, 1));
	Mid->SetScalarParameterValue(TEXT("GroundFogOn"), FogTex.IsValid() ? 1.f : 0.f);
	Mid->SetScalarParameterValue(TEXT("GroundMist"), bMist ? 1.f : 0.f);
	Mid->SetScalarParameterValue(TEXT("GroundFogTime"), (float)Clock);
}

void FTCGroundPaint::RebindAll()
{
	TArray<TWeakObjectPtr<UMaterialInstanceDynamic>> Copy = Bound;
	for (const TWeakObjectPtr<UMaterialInstanceDynamic>& B : Copy) if (UMaterialInstanceDynamic* Mid = B.Get()) Bind(Mid);
}

FString FTCGroundPaint::StatusLine() const
{
	return FString::Printf(TEXT("ground paint: %dx%d cells, %d px a cell, %s%s, %d tiles queued, last paint %.1f ms, upload %.1f ms, fog fading %d, rubble %d, craters %d, starts %d, snapshots %d, rows %d"),
		Core.MapW(), Core.MapH(), Core.PixelsPerCell(), Core.Textured() ? TEXT("textured") : TEXT("flat"), TileError.IsEmpty() ? TEXT("") : *(TEXT(" (") + TileError + TEXT(")")),
		Core.PendingTiles(), LastPaintMs, LastUploadMs, FogActive.Num(), Rubble.Count(), Craters.Count(), Starts, Snapshots, RowsApplied);
}
