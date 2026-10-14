#include "World/TCApron.h"
#include "World/TCWater.h"
#include "World/TCWorldObjects.h"
#include "ThreeCrossroads.h"
#include "Net/TCMatchState.h"
#include "Engine/World.h"
#include "Engine/Texture2D.h"
#include "GameFramework/Actor.h"
#include "Materials/MaterialInterface.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "ProceduralMeshComponent.h"

const TCHAR* const FTCApron::ReferencePin = TEXT("e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
const TCHAR* const FTCApron::MaterialPath = TEXT("/Game/Water/M_TCApron.M_TCApron");

namespace TCWaterView
{
	UTexture2D* MakeTexture(int32 W, int32 H, const uint8* Rgba, bool bMips, bool bWrap);
	void UpdateRect(UTexture2D* T, const uint8* Rgba, int32 W, int32 X0, int32 Y0, int32 X1, int32 Y1);
	AActor* SpawnHolder(UWorld* World, const TCHAR* Name, TStrongObjectPtr<UProceduralMeshComponent>& OutMesh);
}

namespace TCApronView
{
	const TCHAR* const TileNames[6] = { TEXT("Grass"), TEXT("Dirt"), TEXT("Mud"), TEXT("Field"), TEXT("Road"), TEXT("Water") };
	inline FVector ToUnreal(const float* P) { return FVector(P[0] * 100.0, P[2] * 100.0, P[1] * 100.0); }
	inline FVector NormalToUnreal(const float* N) { return FVector(N[0], N[2], N[1]); }
}

FTCApron::FTCApron() = default;
FTCApron::~FTCApron() = default;

void FTCApron::Attach(UWorld* World)
{
	if (Actor.IsValid() || !World) return;
	AActor* A = TCWaterView::SpawnHolder(World, TEXT("Apron"), Mesh);
	if (!A) return;
	Actor = A;
	Mesh->SetVisibility(false);
	if (UMaterialInterface* M = LoadObject<UMaterialInterface>(nullptr, MaterialPath)) Material.Reset(M);
	else { UE_LOG(LogTC, Error, TEXT("apron: %s does not load (run unreal/Tools/water/make-water-materials.py); the land past the map edge is not drawn"), MaterialPath); }
	if (!NoiseTex.IsValid())
	{
		// ground.js noise tables as apron.js noiseTex packs them (512 x 512, linear, repeating, no mipmaps)
		const std::vector<uint8_t> Noise = TCApronCore::NoiseTexture(TCApronCore::BuildNoise());
		NoiseTex.Reset(TCWaterView::MakeTexture(TCApronCore::NoiseSize, TCApronCore::NoiseSize, Noise.data(), false, true));
	}
	if (!TileTex.Num()) MakeFlatTiles();
}

void FTCApron::Detach()
{
	if (AActor* A = Actor.Get()) A->Destroy();
	Actor.Reset(); Mesh.Reset(); MID.Reset(); CellTex.Reset();
	Core = TCApronCore::FApron();
	W = H = 0;
}

void FTCApron::MakeFlatTiles()
{
	// apron.js tileTextures before the textures load: a 1 x 1 tile of each flat colour, Math.round, alpha 255
	TileTex.Reset(); TileMetres.Reset();
	bTileSRGB = bTileTint = false;
	for (int32 M = 0; M < 6; M++)
	{
		double C[3];
		TCApronCore::FlatColor(M, C);
		const uint8 Px[4] = { (uint8)TCApronCore::JsRound(C[0]), (uint8)TCApronCore::JsRound(C[1]), (uint8)TCApronCore::JsRound(C[2]), 255 };
		TileTex.Emplace(TCWaterView::MakeTexture(1, 1, Px, false, true));
		TileMetres.Add((float)TCApronCore::LookMetres[M]);
	}
}

void FTCApron::SetTiles(const TArray<UTexture*>& Tiles, const TArray<float>& Metres, bool bSRGB, bool bRawPhotos)
{
	if (Tiles.Num() != 6 || Metres.Num() != 6) { UE_LOG(LogTC, Warning, TEXT("apron: SetTiles wants six tiles and six lengths; kept the current ones")); return; }
	TileTex.Reset();
	for (UTexture* T : Tiles) TileTex.Emplace(T);
	TileMetres = Metres;
	bTileSRGB = bSRGB; bTileTint = bRawPhotos;
	UE_LOG(LogTC, Log, TEXT("apron: tiles set (%s, %s)"), bSRGB ? TEXT("sRGB") : TEXT("raw bytes"), bRawPhotos ? TEXT("photos tinted in the shader") : TEXT("already tinted"));
	BindMaterial();
}

void FTCApron::SetTileBytes(const TArray<TArray<uint8>>& Rgba, const TArray<int32>& Sizes)
{
	if (Rgba.Num() != 6 || Sizes.Num() != 6) { UE_LOG(LogTC, Warning, TEXT("apron: SetTileBytes wants six tiles; kept the current ones")); return; }
	TArray<UTexture*> Tiles;
	TArray<float> Metres;
	for (int32 M = 0; M < 6; M++)
	{
		if (Sizes[M] <= 0 || Rgba[M].Num() != Sizes[M] * Sizes[M] * 4) { UE_LOG(LogTC, Warning, TEXT("apron: tile %d has %d bytes for %d px; kept the current ones"), M, Rgba[M].Num(), Sizes[M]); return; }
		UTexture2D* T = TCWaterView::MakeTexture(Sizes[M], Sizes[M], Rgba[M].GetData(), true, true);
		if (!T) { UE_LOG(LogTC, Warning, TEXT("apron: tile %d could not be made; kept the current ones"), M); return; }
		Tiles.Add(T);
		Metres.Add((float)TCApronCore::LookMetres[M]);
	}
	SetTiles(Tiles, Metres, false, false);
}

float FTCApron::Height(float X, float Z) const
{
	if (GroundHeight) return GroundHeight(X, Z);
	const int32 CX = FMath::Clamp((int32)FMath::FloorToInt(X / 2.f), 0, W - 1), CZ = FMath::Clamp((int32)FMath::FloorToInt(Z / 2.f), 0, H - 1);
	return Levels.IsValidIndex(CZ * W + CX) ? Levels[CZ * W + CX] * 2.5f : 0.f;
}

bool FTCApron::WetCell(int32 C) const
{
	// apron.js wetRow: the physical grid's W, F or = (an undiscovered World cell reads as dry open ground)
	return Chars.IsValidIndex(C) && TCWaterCore::IsWet(Chars[C]);
}

void FTCApron::CopyCells(const FTCMatchState& State, const TSet<int32>* Cells)
{
	const FTCMap& Map = State.Map;
	auto One = [&](int32 C)
	{
		const bool bKnown = Map.Known.IsValidIndex(C) && Map.Known[C];
		const bool bNew = bKnown && !Known[C];
		Known[C] = bKnown;
		Chars[C] = bKnown && Map.Chars.IsValidIndex(C) ? Map.Chars[C] : '.';
		Levels[C] = bKnown && Map.Levels.IsValidIndex(C) ? Map.Levels[C] : 0;
		// map.rows: the start map's rows on an authored map; World rows follow each discovered cell
		if (bWorld && bNew) Orig[C] = Chars[C];
	};
	if (Cells) { for (const int32 C : *Cells) if (C >= 0 && C < W * H) One(C); }
	else for (int32 C = 0; C < W * H; C++) One(C);
}

void FTCApron::ReadCells(const TSet<int32>* Cells)
{
	using namespace TCApronCore;
	static const FGroundNoise Noise = BuildNoise();
	if (CellData.Num() != W * H * 4) CellData.Init(0, W * H * 4);
	auto One = [&](int32 C)
	{
		uint8_t Px[4];
		CellTexel(CellMaterial(Source, FieldCells, Noise, C), Px);
		FMemory::Memcpy(&CellData[C * 4], Px, 4);
	};
	if (Cells)
	{
		// a cell's paint reads its eight neighbours (wet, town), so they are redone too
		TSet<int32> Near;
		for (const int32 C : *Cells)
		{
			const int32 X = C % W, Y = C / W;
			for (int32 DY = -1; DY <= 1; DY++) for (int32 DX = -1; DX <= 1; DX++)
				if (X + DX >= 0 && Y + DY >= 0 && X + DX < W && Y + DY < H) Near.Add((Y + DY) * W + X + DX);
		}
		for (const int32 C : Near) One(C);
	}
	else for (int32 C = 0; C < W * H; C++) One(C);
	if (!CellTex.IsValid() || CellTex->GetSizeX() != W || CellTex->GetSizeY() != H)
	{
		UTexture2D* T = TCWaterView::MakeTexture(W, H, CellData.GetData(), false, false);
		if (T) { T->Filter = TF_Nearest; T->UpdateResource(); } // apron.js: NearestFilter, read with texelFetch
		CellTex.Reset(T);
		if (MID.IsValid()) MID->SetTextureParameterValue(TEXT("Cells"), CellTex.Get());
	}
	else TCWaterView::UpdateRect(CellTex.Get(), CellData.GetData(), W, 0, 0, W, H);
}

void FTCApron::ResetFrom(const FTCMatchState& State)
{
	W = State.Map.W; H = State.Map.H; bWorld = State.Map.bWorld;
	ShapeCount = 0;
	if (Mesh.IsValid()) { Mesh->ClearAllMeshSections(); Mesh->SetVisibility(false); }
	if (W <= 0 || H <= 0) { Core = TCApronCore::FApron(); Fog.Reset(0, 0); return; }
	Chars.Init('.', W * H); Orig.Init('.', W * H); Levels.Init(0, W * H); Known.Init(false, W * H);
	CopyCells(State, nullptr);
	for (int32 C = 0; C < W * H; C++) Orig[C] = Chars[C];

	// what ground.js cellAttrs reads, from what this client was told
	Source = TCApronCore::FCellSource();
	Source.W = W; Source.H = H;
	// the map objects' copy of the cells (terrain.grid, physicalGrid, terrain.state) when it holds this map
	auto Held = [this](int C) { return Objects && Objects->MapW() == W && Objects->MapH() == H && Known[C]; };
	Source.Grid = [this, Held](int C) -> uint8_t { return Held(C) ? Objects->Char(C) : Chars[C]; };
	Source.Physical = [this, Held](int C) -> uint8_t { return Held(C) ? Objects->Physical(C) : Chars[C]; };
	Source.Orig = [this](int C) -> uint8_t { return Orig[C]; };
	Source.Level = [this](int C) { return (int)Levels[C]; };
	Source.State = [this, Held](int C) { return Held(C) ? (int)Objects->CellState(C) : (int)TCApronCore::StartState(Chars[C], C); };
	// The start's map.spawns are not kept by FTCMatchState; the seats' spawn points (metres) stand in. Fields never
	// reach the map's edge cells (they start one cell in), and only those paint the apron, so the apron is unchanged.
	for (const TOptional<FVector2D>& Sp : State.Spawns) if (Sp.IsSet()) Source.Spawns.push_back({ (int)FMath::FloorToInt(Sp->X / 2.0), (int)FMath::FloorToInt(Sp->Y / 2.0) });
	for (const FTCMapPoint& P : State.MapPoints) Source.Points.push_back({ P.X, P.Y });
	FieldCells = TCApronCore::Fields(Source);

	Fog.Reset(W, H);
	bFogAll = true; FogCells.Reset(); CellsChanged.Reset(); bEdgeDirty = bCellsDirty = false;

	Core.Build(W, H);
	Core.Shape([this](double X, double Z) { return (double)Height((float)X, (float)Z); }, [this](int C) { return WetCell(C); });
	ShapeCount = 1;
	ReadCells(nullptr);
	SendMesh(true);
	BindMaterial();
	UE_LOG(LogTC, Log, TEXT("apron: %s %dx%d: %d columns, %d vertices, %d triangles"), *State.Map.Name, W, H, Core.N, Core.N * TCApronCore::FApron::R, (int32)Core.Index.size() / 3);
}

void FTCApron::SendMesh(bool bCreate)
{
	if (!Mesh.IsValid() || !Core.N) return;
	const int32 NV = (int32)(Core.Position.size() / 3);
	TArray<FVector> Verts, Normals;
	Verts.SetNumUninitialized(NV); Normals.SetNumUninitialized(NV);
	for (int32 V = 0; V < NV; V++) { Verts[V] = TCApronView::ToUnreal(&Core.Position[(size_t)V * 3]); Normals[V] = TCApronView::NormalToUnreal(&Core.Normal[(size_t)V * 3]); }
	if (!bCreate && Mesh->GetNumSections() > 0)
	{
		Mesh->UpdateMeshSection_LinearColor(0, Verts, Normals, TArray<FVector2D>(), TArray<FLinearColor>(), TArray<FProcMeshTangent>());
		return;
	}
	TArray<int32> Tris;
	Tris.SetNumUninitialized((int32)Core.Index.size());
	for (int32 I = 0; I < Tris.Num(); I++) Tris[I] = (int32)Core.Index[I];
	Mesh->CreateMeshSection_LinearColor(0, Verts, Tris, Normals, TArray<FVector2D>(), TArray<FLinearColor>(), TArray<FProcMeshTangent>(), false);
	if (MID.IsValid()) Mesh->SetMaterial(0, MID.Get());
	Mesh->SetVisibility(true);
}

void FTCApron::BindMaterial()
{
	if (!Mesh.IsValid() || !Material.IsValid() || W <= 0) return;
	if (!MID.IsValid()) MID.Reset(UMaterialInstanceDynamic::Create(Material.Get(), GetTransientPackage()));
	UMaterialInstanceDynamic* D = MID.Get();
	for (int32 M = 0; M < 6 && M < TileTex.Num(); M++) D->SetTextureParameterValue(TCApronView::TileNames[M], TileTex[M].Get());
	D->SetTextureParameterValue(TEXT("Cells"), CellTex.Get());
	D->SetTextureParameterValue(TEXT("Noise"), NoiseTex.Get());
	D->SetTextureParameterValue(TEXT("Fog"), Fog.Texture());
	if (TileMetres.Num() == 6)
	{
		D->SetVectorParameterValue(TEXT("MetersA"), FLinearColor(TileMetres[0], TileMetres[1], TileMetres[2], 0));
		D->SetVectorParameterValue(TEXT("MetersB"), FLinearColor(TileMetres[3], TileMetres[4], TileMetres[5], 0));
	}
	D->SetVectorParameterValue(TEXT("MapSize"), FLinearColor(W * 2.f, H * 2.f, 0, 0));
	D->SetVectorParameterValue(TEXT("CellDim"), FLinearColor((float)W, (float)H, 0, 0));
	D->SetScalarParameterValue(TEXT("TileSRGB"), bTileSRGB ? 1.f : 0.f);
	D->SetScalarParameterValue(TEXT("TileTint"), bTileTint ? 1.f : 0.f);
	D->SetScalarParameterValue(TEXT("FogGamma"), FogGamma);
	D->SetScalarParameterValue(TEXT("Unseen"), 82.f / 255.f); // never seen: as dark as ATCTerrain's unexplored ground
	if (Mesh->GetNumSections() > 0) Mesh->SetMaterial(0, D);
}

void FTCApron::MarkCells(const FTCMatchState& State, const TSet<int32>& Shaped, const TSet<int32>& Fogged)
{
	if (W <= 0 || State.Map.W != W || State.Map.H != H) return;
	FogCells.Append(Fogged);
	if (!Shaped.Num()) return;
	CopyCells(State, &Shaped);
	CellsChanged.Append(Shaped);
	bCellsDirty = true;
	// the edge profile reads the drawn ground along the edge, which a change a few cells in can move
	for (const int32 C : Shaped)
	{
		const int32 X = C % W, Y = C / W;
		if (X < 4 || Y < 4 || X >= W - 4 || Y >= H - 4) { bEdgeDirty = true; break; }
	}
}

void FTCApron::Flush(const FTCMatchState& State, bool bRevealAll)
{
	if (W <= 0 || State.Map.W != W || State.Map.H != H) return;
	if (bRevealAll != bLastReveal) { bLastReveal = bRevealAll; bFogAll = true; }
	if (bFogAll) { Fog.Update(State, bRevealAll, nullptr); bFogAll = false; FogCells.Reset(); }
	else if (FogCells.Num()) { Fog.Update(State, bRevealAll, &FogCells); FogCells.Reset(); }
	if (bCellsDirty)
	{
		if (bWorld) FieldCells = TCApronCore::Fields(Source); // World fields follow the ground discovered so far
		ReadCells(&CellsChanged);
		CellsChanged.Reset();
		bCellsDirty = false;
	}
	if (bEdgeDirty)
	{
		bEdgeDirty = false;
		auto HAt = [this](double X, double Z) { return (double)Height((float)X, (float)Z); };
		auto Wet = [this](int C) { return WetCell(C); };
		if (Core.Differs(HAt, Wet)) { Core.Shape(HAt, Wet); ShapeCount++; SendMesh(false); }
	}
	if (!MID.IsValid() && Material.IsValid()) BindMaterial();
}

// ---------------------------------------------------------------------------------------------------- tests

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCApronShapeTest, "ThreeCrossroads.World.ApronShape", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCApronShapeTest::RunTest(const FString& Parameters)
{
	using namespace TCApronCore;
	// a 12 x 8 map: a hill of 2.5 m in its north-west corner (two cells wide, clear of the river), a river leaving through the south edge in column 5
	const int W = 12, H = 8;
	auto Level = [](int X, int Z) { return X < 2 && Z < 3 ? 1 : 0; };
	auto HAt = [&](double X, double Z) { return Level(FMath::Min(W - 1, (int)(X / 2)), FMath::Min(H - 1, (int)(Z / 2))) * 2.5; };
	auto Wet = [&](int C) { return C % W == 5; };
	FApron A;
	A.Build(W, H);
	A.Shape(HAt, Wet);
	TestEqual(TEXT("columns: five per edge cell and one per corner"), A.N, 2 * 5 * (W + H) + 4);
	TestEqual(TEXT("indices: the ground rows, the strip and the skirt"), (int)A.Index.size(), (A.N * (FApron::K - 1) + 2 * A.N) * 6);
	int Seam = 0, Short = 0, Down = 0, Outside = 0;
	for (int C = 0; C < A.N; C++)
	{
		const float* P0 = &A.Position[(size_t)C * FApron::R * 3];
		// the first row sits exactly on the map's edge height: no step where apron and map meet
		if (FMath::Abs(P0[1] - HAt(A.ClampX(A.Cols[C].X), A.ClampZ(A.Cols[C].Z))) > 1e-5) Seam++;
		// the last row lies 1250 m out (along the diagonal at the corners): far past what the camera sees
		const float* PL = &A.Position[((size_t)C * FApron::R + FApron::K - 1) * 3];
		const double Out = FMath::Sqrt(FMath::Square(PL[0] - P0[0]) + FMath::Square(PL[2] - P0[2]));
		if (Out < 1250.0 - 1e-3) Short++;
		for (int K = 0; K < FApron::K; K++) if (A.Normal[((size_t)C * FApron::R + K) * 3 + 1] < 0) Down++;
		// the strip tucked under the map's edge: 0.3 m in and 0.3 m below
		const float* PS = &A.Position[((size_t)C * FApron::R + FApron::K + 1) * 3];
		if (PS[0] < 0 || PS[2] < 0 || PS[0] > W * 2 || PS[2] > H * 2) Outside++;
	}
	TestEqual(TEXT("first-row heights off the edge"), Seam, 0);
	TestEqual(TEXT("columns shorter than 1250 m"), Short, 0);
	TestEqual(TEXT("normals facing down"), Down, 0);
	TestEqual(TEXT("strip vertices outside the map"), Outside, 0);
	// where the river leaves, the first rows (2 m out) stay at the edge's water height: the rolling land starts farther
	int RiverHigh = 0;
	for (int C = 0; C < A.N; C++) if (A.Wet[C] > 0) for (int K = 0; K < 4; K++) if (FMath::Abs(A.Position[((size_t)C * FApron::R + K) * 3 + 1]) > 0.05f) RiverHigh++;
	TestEqual(TEXT("apron vertices off the water height within 2 m of where the river leaves"), RiverHigh, 0);
	TestFalse(TEXT("same heights: no reshape"), A.Differs(HAt, Wet));
	TestTrue(TEXT("a changed edge height asks for a reshape"), A.Differs([&](double X, double Z) { return HAt(X, Z) + (X < 1 ? 1.0 : 0.0); }, Wet));
	// the cell paint of an undiscovered World cell: open ground at level 0, as the start map writes it
	FCellSource S;
	S.W = W; S.H = H;
	S.Grid = S.Physical = S.Orig = [](int) -> uint8_t { return '.'; };
	S.Level = [](int) { return 0; };
	S.State = [](int) { return 0; };
	const FGroundNoise Noise = BuildNoise();
	const FCellMat M = CellMaterial(S, Fields(S), Noise, 0);
	TestTrue(TEXT("open ground: grass with dirt"), M.Prim == Grass && M.Sec == Dirt && M.Amt > 0.f);
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
