#include "World/TCTerrain.h"
#include "ThreeCrossroads.h"
#include "Net/TCMatchState.h"
#include "World/TCFogFrontier.h"
#include "ProceduralMeshComponent.h"
#include "Materials/MaterialInterface.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "World/TCGroundSurface.h"
#include "World/TCScatter.h"
#include "World/TCWorldObjects.h"
#include "World/TCGroundPaint.h"
#include <algorithm>
#include <limits>

static_assert(TCGround::CellMetres == FTCMatchState::CellMetres && TCGround::LevelMetres == FTCMatchState::LevelMetres, "one cell and level size");
static_assert(TCRelief::Cell == FTCMatchState::CellMetres && TCRelief::LevelHeight == FTCMatchState::LevelMetres, "the relief's cell and level");

namespace TCTerrainImpl
{
	// The browser minimap's ground colours per terrain character (client/main.js MM_COLORS), the base colour under the
	// relief paint until the painted ground (client/ground.js) lands. K (a building section) and Q (a wreck) are not in
	// that table; they borrow B and R here.
	FColor CharColor(uint8 Ch)
	{
		switch (Ch)
		{
		case 'B': case 'K': return FColor(150, 132, 100);
		case 'H': return FColor(47, 74, 34);
		case '#': return FColor(154, 149, 138);
		case '+': return FColor(90, 79, 54);
		case 'T': return FColor(62, 50, 34);
		case 'W': return FColor(60, 93, 112);
		case '=': return FColor(122, 90, 58);
		case 'F': return FColor(106, 127, 122);
		case 'R': case 'Q': return FColor(122, 114, 102);
		case 'X': return FColor(96, 90, 70);
		case 'Y': return FColor(84, 84, 78);
		case 'D': return FColor(150, 128, 100);
		case 'M': return FColor(92, 80, 58);
		case 'N': return FColor(120, 96, 60);
		case 'O': return FColor(38, 62, 30);
		case 'A': return FColor(226, 220, 204);
		default: return FColor(108, 118, 69);
		}
	}
	constexpr const TCHAR* ReliefMaterialPath = TEXT("/Game/Relief/M_TCRelief.M_TCRelief");
	constexpr const TCHAR* FallbackMaterialPath = TEXT("/Game/Models/Materials/Look/M_TCGroundLambert.M_TCGroundLambert");
}

ATCTerrain::ATCTerrain()
{
	PrimaryActorTick.bCanEverTick = false;
	Mesh = CreateDefaultSubobject<UProceduralMeshComponent>(TEXT("Ground"));
	Mesh->bUseAsyncCooking = false;
	Mesh->SetCollisionEnabled(ECollisionEnabled::NoCollision); // picking is the relief's own (RayToGround)
	Mesh->SetCastShadow(false); // relief.js: castShadow false, the ground takes the shadows of all else
	RootComponent = Mesh;
	// the material loads with the first start (EnsureMaterial), so a project without /Game/Relief still starts
}

void ATCTerrain::ResetFrom(const FTCMatchState& State)
{
	const int32 NewW = State.Map.W, NewH = State.Map.H;
	const int32 NewChunksX = FMath::DivideAndRoundUp(NewW, ChunkCells), NewChunksY = FMath::DivideAndRoundUp(NewH, ChunkCells);
	// A reconnect to the match already on screen keeps its chunks drawn while every one is rebuilt from the new start,
	// so a big World map does not blink out for many frames. Anything else (another match, room, seat or map) clears
	// first, so no ground of an earlier match or view lingers; a reset from an empty map (the match was dropped) clears.
	const bool bKeep = State.bHasMatch && NewW > 0 && NewH > 0 && State.MatchId >= 0 && State.MatchId == BuiltMatch && !State.RoomCode.IsEmpty() && State.RoomCode == BuiltRoom
		&& State.You == BuiltYou && State.bSpectator == bBuiltSpectator && State.Map.bWorld == bBuiltWorld && State.Map.Name == BuiltMapName
		&& NewW == W && NewH == H && SectionVerts.Num() == NewChunksX * NewChunksY;
	if (!bKeep)
	{
		Mesh->ClearAllMeshSections();
		SectionVerts.Init(0, NewChunksX * NewChunksY);
	}
	W = NewW; H = NewH; ChunksX = NewChunksX; ChunksY = NewChunksY;
	BuiltMatch = State.MatchId; BuiltRoom = State.RoomCode; BuiltYou = State.You; bBuiltSpectator = State.bSpectator;
	bBuiltWorld = State.Map.bWorld; BuiltMapName = State.Map.Name;
	bLastResetKept = bKeep;
	bEndRevealed = false; // a start resets the ending (client/main.js startGame calls epilogue.reset)
	Queue.Reset();
	PendingCells.Reset();
	MeshedTotal = 0;
	DrawnCells = 0;
	if (W <= 0 || H <= 0 || State.Map.Levels.Num() != W * H || State.Map.Known.Num() != W * H || State.Map.Chars.Num() != W * H)
	{
		W = H = 0;
		ReliefCore.Reset(0, 0, false, ChunkCells, true);
		UE_LOG(LogTC, Log, TEXT("terrain: cleared (no match held)"));
		return;
	}
	// client/main.js startGame: createRelief(map, physicalGrid, { low: gfx.low || !!map.world }) (round 15: the Graphics
	// level, Client/TCGraphics.h; the watch subscribes before the level is read)
	GfxWatch.TakeChanged();
	const bool bLowRelief = State.Map.bWorld || FTCGraphics::Get().Budget().bReliefLow;
	EnsureMaterial(bLowRelief);
	const double T0 = FPlatformTime::Seconds();
	StartRows = State.Map.Chars;
	ReliefCore.Reset(W, H, bLowRelief, ChunkCells, true);
	CopyInputs(State, true); // the water lines freeze here, as createWaterLevels reads map.heights once
	ReliefCore.Build();
	ReliefCore.TakeRebuilt();
	for (int32 C = 0; C < ChunksX * ChunksY; C++) Queue_(C, true);
	UE_LOG(LogTC, Log, TEXT("terrain: %s %dx%d cells, relief %d vertices %d triangles %d cliff edges in %.0f ms, %d chunks queued%s"), *State.Map.Name, W, H,
		ReliefCore.Stats.Vertices, ReliefCore.Stats.Triangles, ReliefCore.Stats.CliffEdges, (FPlatformTime::Seconds() - T0) * 1000.0, Queue.Num(),
		bKeep ? TEXT(" (same match: drawn ground kept while it rebuilds)") : TEXT(""));
}

void ATCTerrain::EnsureMaterial(bool bLow)
{
	// client/relief-material.js over the painted ground (unreal/Tools/relief/make-relief-material.py); without it the
	// plain vertex-colour Lambert. Low (RELIEF_LOW) on World maps, as client/main.js sets it.
	if (!Material)
	{
		Material = LoadObject<UMaterialInterface>(nullptr, TCTerrainImpl::ReliefMaterialPath, nullptr, LOAD_NoWarn | LOAD_Quiet);
		if (!Material) { Material = LoadObject<UMaterialInterface>(nullptr, TCTerrainImpl::FallbackMaterialPath, nullptr, LOAD_NoWarn | LOAD_Quiet); UE_LOG(LogTC, Warning, TEXT("terrain: %s missing, the ground is drawn with the plain Lambert"), TCTerrainImpl::ReliefMaterialPath); }
	}
	if (Material && !MaterialMid) MaterialMid = UMaterialInstanceDynamic::Create(Material, this);
	if (MaterialMid) MaterialMid->SetScalarParameterValue(TEXT("Low"), bLow ? 1.f : 0.f);
}

void ATCTerrain::CopyInputs(const FTCMatchState& State, bool bFrozen, std::vector<int>* RoadChanges)
{
	const int32 N = W * H;
	TCRelief::FRelief& R = ReliefCore;
	const bool bSource = CellSource && CellSource->HasMap() && CellSource->MapW() == W && CellSource->MapH() == H;
	DrawnCells = 0;
	for (int32 C = 0; C < N; C++)
	{
		const bool bKnown = (bool)State.Map.Known[C];
		R.InKnown[(size_t)C] = bKnown ? 1 : 0;
		DrawnCells += bKnown ? 1 : 0;
		R.InLevel[(size_t)C] = State.Map.Levels[C];
		// unknown cells are never read by the relief (TCRelief.h); their characters are left as the start wrote them
		const uint8 Ch = State.Map.Chars[C];
		R.InGrid[(size_t)C] = bSource ? CellSource->Physical(C) : Ch == 'N' ? (uint8)'.' : Ch;
		if (bFrozen) R.InFrozen[(size_t)C] = State.Map.Levels[C];
	}
	const std::vector<uint8_t> Before = RoadChanges ? R.InRoad : std::vector<uint8_t>();
	if (PaintSource && PaintSource->HasMap() && PaintSource->Painter().MapW() == W && PaintSource->Painter().MapH() == H)
		PaintSource->Painter().CopyRoadMask(R.InRoad);
	else
	{
		// client/main.js hands relief.js ground.js's road mask (prim === ROAD): 'D' cells, and open ground by a house of the
		// map rows that is no field and has no water beside it. Fields: ground.js fieldsOf over the rows (TCScatter.h
		// FieldCells), clear of the capture points and the seats' spawns. World maps: no region looks are held here, so field
		// blocks use the default chance (ground.js look?.fields ?? 0.24).
		TCScatterCore::FScatterMap FM;
		FM.W = W; FM.H = H;
		FM.Rows.resize((size_t)H);
		std::vector<uint8_t> Rows((size_t)N);
		for (int32 Y = 0; Y < H; Y++)
		{
			std::string& Row = FM.Rows[(size_t)Y];
			Row.resize((size_t)W);
			for (int32 X = 0; X < W; X++)
			{
				const int32 C = Y * W + X;
				const uint8 Ch = State.Map.bWorld ? State.Map.Chars[C] : StartRows.IsValidIndex(C) ? StartRows[C] : (uint8)'.';
				Row[(size_t)X] = (char)Ch;
				Rows[(size_t)C] = Ch;
			}
		}
		for (const FTCMapPoint& P : State.MapPoints) FM.Points.push_back(TCScatterCore::FCellXY{ P.X, P.Y });
		for (const TOptional<FVector2D>& S : State.Spawns)
			if (S.IsSet()) FM.Spawns.push_back(TCScatterCore::FCellXY{ (int)FMath::FloorToInt(S->X / TCRelief::Cell), (int)FMath::FloorToInt(S->Y / TCRelief::Cell) });
		const std::vector<unsigned char> Fields = TCScatterCore::FieldCells(FM);
		TCRelief::RoadMask(W, H, R.InGrid, Rows, std::vector<uint8_t>(Fields.begin(), Fields.end()), R.InKnown, R.InRoad);
	}
	// A discovery can remove a whole 10 x 8 field block and expose roads away from the changed cell.
	// Feed every changed road to relief.update, so those shoulders are rebuilt as well.
	if (RoadChanges) for (int32 C = 0; C < N; C++) if (Before[(size_t)C] != R.InRoad[(size_t)C]) RoadChanges->push_back(C);
}

void ATCTerrain::Queue_(int32 Chunk, bool bShape)
{
	if (Chunk < 0 || Chunk >= ChunksX * ChunksY) return;
	bool& Shape = Queue.FindOrAdd(Chunk, false);
	Shape |= bShape;
}

void ATCTerrain::MarkCells(const TSet<int32>& Shaped, const TSet<int32>& Fogged)
{
	if (W <= 0) return;
	for (const int32 C : Shaped) if (C >= 0 && C < W * H) PendingCells.Add(C);
	// fog lives in the vertex colour of each vertex's own cell: a flip recolours that cell's chunk only
	for (const int32 C : Fogged) if (C >= 0 && C < W * H) Queue_(ReliefCore.PartOfCell(C), false);
}

void ATCTerrain::Flush(const FTCMatchState& State)
{
	if (W <= 0 || State.Map.W != W || State.Map.H != H) return;
	// client/main.js: the fog lifts once the match is decided, on authored maps only (the server sends every unit then)
	if (!bEndRevealed && !State.Map.bWorld && State.Winner >= -1)
	{
		bEndRevealed = true;
		for (int32 C = 0; C < ChunksX * ChunksY; C++) Queue_(C, false);
		UE_LOG(LogTC, Log, TEXT("terrain: match decided (winner %d), fog lifted on %d chunks"), State.Winner, ChunksX * ChunksY);
	}
	// round 15: relief.js gfx.onChange: low = gfx.low || map.world; material.userData.setLow(low); rebuildGeometry()
	if (GfxWatch.TakeChanged())
	{
		const bool bLowRelief = bBuiltWorld || FTCGraphics::Get().Budget().bReliefLow;
		const double T0 = FPlatformTime::Seconds();
		if (ReliefCore.SetLow(bLowRelief))
		{
			EnsureMaterial(bLowRelief);
			for (const int P : ReliefCore.TakeRebuilt()) Queue_(P, true);
			ReliefSwitches++;
			UE_LOG(LogTC, Log, TEXT("terrain: graphics %s mid-match: relief %s variant, %d vertices %d triangles, rebuilt in %.0f ms, %d chunks queued"), LexToString(FTCGraphics::Get().Level()),
				bLowRelief ? TEXT("low") : TEXT("high"), ReliefCore.Stats.Vertices, ReliefCore.Stats.Triangles, (FPlatformTime::Seconds() - T0) * 1000.0, Queue.Num());
		}
	}
	if (PendingCells.Num())
	{
		// relief.js update(cells): the cells within 3 rebuild, the chunks within 7 of them re-emit (crater lips read that far)
		std::vector<int> Cells;
		CopyInputs(State, false, &Cells);
		Cells.reserve(Cells.size() + (size_t)PendingCells.Num());
		for (const int32 C : PendingCells) Cells.push_back(C);
		std::sort(Cells.begin(), Cells.end());
		Cells.erase(std::unique(Cells.begin(), Cells.end()), Cells.end());
		ReliefCore.Update(Cells);
		for (const int P : ReliefCore.TakeRebuilt()) Queue_(P, true);
		PendingCells.Reset();
	}
	if (!Queue.Num()) return;
	int32 Done = 0;
	for (auto It = Queue.CreateIterator(); It && Done < MaxChunksPerFrame; ++It, ++Done)
	{
		UploadChunk(State, It.Key(), It.Value());
		It.RemoveCurrent();
	}
}

// Slice 13 lead check: see the header
int32 ATCTerrain::DrawnOnUnknown(const FTCMatchState& State, int32* OutVertices, int32* OutUnowned) const
{
	int32 Bad = 0, Verts = 0, Unowned = 0;
	for (int32 P = 0; P < ReliefCore.NumParts(); P++)
	{
		const TCRelief::FPart& Part = ReliefCore.Part(P);
		Verts += Part.Vertices();
		for (const int32_t Owner : Part.Owner)
		{
			if (Owner < 0 || Owner >= W * H) { Unowned++; continue; }
			if (!State.Map.Known.IsValidIndex(Owner) || !State.Map.Known[Owner]) Bad++;
		}
	}
	if (OutVertices) *OutVertices = Verts;
	if (OutUnowned) *OutUnowned = Unowned;
	return Bad;
}

FLinearColor ATCTerrain::CellColor(const FTCMatchState& State, int32 C) const
{
	const FColor Base = TCTerrainImpl::CharColor(State.Map.Chars.IsValidIndex(C) ? State.Map.Chars[C] : '.');
	const float Fog = TCFogLook::Multiplier(State, bEndRevealed, C); // round 14: the shared look (fog.js)
	const FColor Lit(FMath::Clamp(int32(Base.R * Fog), 0, 255), FMath::Clamp(int32(Base.G * Fog), 0, 255), FMath::Clamp(int32(Base.B * Fog), 0, 255));
	return FLinearColor(Lit);
}

float ATCTerrain::HeightAt(float X, float Z) const
{
	return W > 0 ? (float)ReliefCore.HAt(X, Z) : 0.f;
}

float ATCTerrain::WaterAt(float X, float Z) const
{
	return W > 0 ? (float)ReliefCore.WaterAt(X, Z) : std::numeric_limits<float>::quiet_NaN();
}

void ATCTerrain::UploadChunk(const FTCMatchState& State, int32 Chunk, bool bShape)
{
	const TCRelief::FPart& P = ReliefCore.Part(Chunk);
	const int32 NV = P.Vertices();
	TArray<FVector> Verts, Normals;
	TArray<FVector2D> UV0, UV1, UV2, UV3;
	TArray<FLinearColor> Colors;
	Verts.SetNumUninitialized(NV); Normals.SetNumUninitialized(NV); Colors.SetNumUninitialized(NV);
	UV0.SetNumUninitialized(NV); UV1.SetNumUninitialized(NV); UV2.SetNumUninitialized(NV); UV3.SetNumUninitialized(NV);
	int32 LastOwner = -1;
	FLinearColor LastColor;
	for (int32 I = 0; I < NV; I++)
	{
		// reference (x, y up, z) metres to Unreal (X, Y, Z up) centimetres; the winding carries over (TCRelief.h)
		Verts[I] = FVector(P.Pos[I * 3] * 100.0, P.Pos[I * 3 + 2] * 100.0, P.Pos[I * 3 + 1] * 100.0);
		Normals[I] = FVector(P.Nrm[I * 3], P.Nrm[I * 3 + 2], P.Nrm[I * 3 + 1]);
		UV0[I] = FVector2D(P.Uv[I * 2], P.Uv[I * 2 + 1]);
		UV1[I] = FVector2D(P.Paint[I * 4], P.Paint[I * 4 + 1]);     // rock, lip
		UV2[I] = FVector2D(P.Paint[I * 4 + 2], P.Paint[I * 4 + 3]); // foot, damp
		UV3[I] = FVector2D(P.Scar[I], 0.0);
		const int32 Owner = P.Owner[I];
		if (Owner != LastOwner) { LastOwner = Owner; LastColor = CellColor(State, Owner); }
		Colors[I] = LastColor;
	}
	// A fog flip only recolours: the section keeps its buffers (the engine wants the positions passed again). If the
	// relief changed underneath, the vertex count differs and the section is recreated instead.
	if (!bShape && NV && SectionVerts[Chunk] == NV)
	{
		Mesh->UpdateMeshSection_LinearColor(Chunk, Verts, Normals, UV0, UV1, UV2, UV3, Colors, TArray<FProcMeshTangent>(), false);
		return;
	}
	MeshedTotal++;
	SectionVerts[Chunk] = NV;
	if (!NV) { Mesh->ClearMeshSection(Chunk); return; }
	TArray<int32> Tris;
	Tris.SetNumUninitialized(P.Idx.size());
	for (int32 I = 0; I < Tris.Num(); I++) Tris[I] = (int32)P.Idx[(size_t)I];
	Mesh->CreateMeshSection_LinearColor(Chunk, Verts, Tris, Normals, UV0, UV1, UV2, UV3, Colors, TArray<FProcMeshTangent>(), false);
	Mesh->SetMaterial(Chunk, MaterialMid ? (UMaterialInterface*)MaterialMid : Material.Get());
}

bool ATCTerrain::RayToGround(const FVector& Origin, const FVector& Dir, FVector2D& OutMetres) const
{
	if (W <= 0 || Dir.IsNearlyZero()) return false;
	const FVector D = Dir.GetSafeNormal();
	// Unreal centimetres to reference metres (x, y up, z)
	const double O3[3] = { Origin.X / 100.0, Origin.Z / 100.0, Origin.Y / 100.0 }, D3[3] = { D.X, D.Z, D.Y };
	double Hit[3];
	if (!ReliefCore.Ray(O3, D3, 1e7, Hit)) return false;
	OutMetres = FVector2D(Hit[0], Hit[2]);
	return true;
}

// ------------------------------------------------------------------------------------------------- tests and captures

#include "Client/TCLighting.h"
#include "Components/DirectionalLightComponent.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Components/SkyLightComponent.h"
#include "Dom/JsonObject.h"
#include "Engine/Engine.h"
#include "Engine/TextureCube.h"
#include "Engine/TextureRenderTarget2D.h"
#include "Engine/World.h"
#include "GameFramework/WorldSettings.h"
#include "HAL/FileManager.h"
#include "ImageUtils.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/Compression.h"
#include "Misc/FileHelper.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "ShaderCompiler.h"
#include "TextureResource.h"
#if WITH_EDITOR
#include "AssetCompilingManager.h"
#endif

namespace TCTerrainImpl
{
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
	int32 LevelOfChar(TCHAR Ch) { return Ch >= 'a' ? 96 - (int32)Ch : (int32)Ch - '0'; } // shared/sim.js levelOf
	TArray<TSharedPtr<FJsonValue>> Strings(const TArray<FString>& Rows)
	{
		TArray<TSharedPtr<FJsonValue>> Out;
		for (const FString& R : Rows) Out.Add(MakeShared<FJsonValueString>(R));
		return Out;
	}
	TSharedRef<FJsonObject> StartMessage(const FString& Name, int32 W, int32 H, const TArray<FString>& Rows, const TArray<FString>& Heights, bool bWorld, const TArray<TSharedPtr<FJsonValue>>& Cells)
	{
		TSharedRef<FJsonObject> Map = MakeShared<FJsonObject>();
		Map->SetNumberField(TEXT("w"), W); Map->SetNumberField(TEXT("h"), H);
		Map->SetStringField(TEXT("name"), Name);
		Map->SetArrayField(TEXT("rows"), Strings(Rows));
		Map->SetArrayField(TEXT("heights"), Strings(Heights));
		if (bWorld) Map->SetBoolField(TEXT("world"), true);
		TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
		M->SetStringField(TEXT("t"), TEXT("start"));
		M->SetObjectField(TEXT("map"), Map);
		M->SetNumberField(TEXT("you"), 0);
		M->SetNumberField(TEXT("matchId"), 1);
		M->SetArrayField(TEXT("cells"), Cells);
		return M;
	}
	void FinishCompiling()
	{
#if WITH_EDITOR
		FAssetCompilingManager::Get().FinishAllCompilation();
		if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();
#endif
	}
	bool SavePng(const FString& Path, int32 W, int32 H, TArray<FColor>& Pixels)
	{
		for (FColor& C : Pixels) C.A = 255;
		TArray64<uint8> Bytes;
		FImageUtils::PNGCompressImageArray(W, H, TArrayView64<const FColor>(Pixels.GetData(), Pixels.Num()), Bytes);
		return FFileHelper::SaveArrayToFile(Bytes, *Path);
	}
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

// The pinned client/relief.js against the port: unreal/Tools/relief/relief-oracle.mjs ran the reference unmodified on all
// 33 authored maps, two incremental cases, Low graphics and a World Conquest sample and kept its inputs, relief.hAt at
// about 1,500 points per case and mesh.raycast for 200 rays per case (fixtures/relief-oracle.json.gz). The port runs here
// in the reference's layout and queries, so every number must match; the standalone harness compares the full meshes.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainReliefOracleTest, "ThreeCrossroads.Terrain.ReliefOracle", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainReliefOracleTest::RunTest(const FString& Parameters)
{
	const FString Path = FPaths::Combine(FPaths::ProjectDir(), TEXT("Tools/relief/fixtures/relief-oracle.json.gz"));
	TArray<uint8> Gz;
	if (!TestTrue(TEXT("fixture reads"), FFileHelper::LoadFileToArray(Gz, *Path) && Gz.Num() > 18)) return false;
	const uint32 Size = (uint32)Gz[Gz.Num() - 4] | (uint32)Gz[Gz.Num() - 3] << 8 | (uint32)Gz[Gz.Num() - 2] << 16 | (uint32)Gz[Gz.Num() - 1] << 24;
	TArray<uint8> Raw;
	Raw.SetNumUninitialized((int64)Size + 1);
	if (!TestTrue(TEXT("fixture gunzips"), FCompression::UncompressMemory(NAME_Gzip, Raw.GetData(), Size, Gz.GetData(), Gz.Num()))) return false;
	Raw[Size] = 0;
	TSharedPtr<FJsonObject> Root;
	if (!TestTrue(TEXT("fixture parses"), FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(FString(UTF8_TO_TCHAR((const ANSICHAR*)Raw.GetData()))), Root) && Root.IsValid())) return false;
	const TArray<TSharedPtr<FJsonValue>>& Cases = Root->GetArrayField(TEXT("cases"));
	TMap<FString, TSharedPtr<FJsonObject>> ByName;
	for (const TSharedPtr<FJsonValue>& C : Cases) ByName.Add(C->AsObject()->GetStringField(TEXT("name")), C->AsObject());
	auto Inputs = [](const FJsonObject& C, TCRelief::FRelief& R, bool bFrozen)
	{
		const FString Lv = C.GetStringField(TEXT("levels")), Gr = C.GetStringField(TEXT("grid")), Rd = C.GetStringField(TEXT("roads")), Fz = C.GetStringField(TEXT("frozen"));
		FString Kn;
		const bool bKnownMask = C.TryGetStringField(TEXT("known"), Kn);
		for (int32 I = 0; I < Lv.Len(); I++)
		{
			R.InLevel[(size_t)I] = (int8_t)TCTerrainImpl::LevelOfChar(Lv[I]);
			R.InGrid[(size_t)I] = (uint8_t)Gr[I];
			R.InRoad[(size_t)I] = Rd[I] == '1';
			R.InKnown[(size_t)I] = bKnownMask ? Kn[I] == '1' : 1;
			if (bFrozen) R.InFrozen[(size_t)I] = (int8_t)TCTerrainImpl::LevelOfChar(Fz[I]);
		}
	};
	int32 Samples = 0, Rays = 0, Bad = 0;
	double WorstH = 0, WorstRay = 0;
	for (const TSharedPtr<FJsonValue>& Cv : Cases)
	{
		const FJsonObject& C = *Cv->AsObject();
		const FString Name = C.GetStringField(TEXT("name"));
		const int32 W = (int32)C.GetNumberField(TEXT("w")), H = (int32)C.GetNumberField(TEXT("h"));
		TCRelief::FRelief R;
		R.Reset(W, H, C.GetBoolField(TEXT("low")), 0, false);
		FString BaseName;
		if (C.TryGetStringField(TEXT("base"), BaseName) && ByName.Contains(BaseName))
		{
			Inputs(*ByName[BaseName], R, true);
			R.Build();
			Inputs(C, R, false);
			std::vector<int> Edits;
			for (const TSharedPtr<FJsonValue>& E : C.GetArrayField(TEXT("edits"))) Edits.push_back((int)E->AsArray()[0]->AsNumber());
			R.Update(Edits);
		}
		else { Inputs(C, R, true); R.Build(); }
		const TSharedPtr<FJsonObject> St = C.GetObjectField(TEXT("stats"));
		if (!C.HasField(TEXT("known")))
		{
			TestEqual(Name + TEXT(": vertices"), R.Stats.Vertices, (int32)St->GetNumberField(TEXT("vertices")));
			TestEqual(Name + TEXT(": triangles"), R.Stats.Triangles, (int32)St->GetNumberField(TEXT("triangles")));
			TestEqual(Name + TEXT(": cliff edges"), R.Stats.CliffEdges, (int32)St->GetNumberField(TEXT("cliffEdges")));
		}
		// a World case: the reference also draws the unknown cells ('.' at level 0); compare where no unknown cell is
		// within 3 cells
		FString Kn;
		const bool bWorld = C.TryGetStringField(TEXT("known"), Kn);
		auto FarFromUnknown = [&](double X, double Z)
		{
			if (!bWorld) return true;
			const int32 Cx = FMath::Clamp((int32)FMath::FloorToDouble(X / 2.0), 0, W - 1), Cz = FMath::Clamp((int32)FMath::FloorToDouble(Z / 2.0), 0, H - 1);
			for (int32 Dz = -3; Dz <= 3; Dz++) for (int32 Dx = -3; Dx <= 3; Dx++)
			{
				const int32 Xx = Cx + Dx, Zz = Cz + Dz;
				if (Xx >= 0 && Zz >= 0 && Xx < W && Zz < H && Kn[Zz * W + Xx] != '1') return false;
			}
			return true;
		};
		for (const TSharedPtr<FJsonValue>& Sv : C.GetArrayField(TEXT("samples")))
		{
			const TArray<TSharedPtr<FJsonValue>>& S = Sv->AsArray();
			const double X = S[0]->AsNumber(), Z = S[1]->AsNumber(), Want = S[2]->AsNumber();
			if (!FarFromUnknown(X, Z)) continue;
			const double D = FMath::Abs(R.HAt(X, Z) - Want);
			WorstH = FMath::Max(WorstH, D);
			Samples++;
			if (D > 1e-4) { if (Bad++ < 10) AddError(FString::Printf(TEXT("%s: hAt(%.4f, %.4f) %.5f, reference %.5f"), *Name, X, Z, R.HAt(X, Z), Want)); }
		}
		for (const TSharedPtr<FJsonValue>& Pv : C.GetArrayField(TEXT("picks")))
		{
			const TArray<TSharedPtr<FJsonValue>>& Pk = Pv->AsArray();
			const double O[3] = { Pk[0]->AsNumber(), Pk[1]->AsNumber(), Pk[2]->AsNumber() }, D[3] = { Pk[3]->AsNumber(), Pk[4]->AsNumber(), Pk[5]->AsNumber() };
			const bool bWant = Pk[6]->Type == EJson::Number;
			double Hit[3];
			const bool bGot = R.Ray(O, D, 1e9, Hit);
			if (bWant && !FarFromUnknown(Pk[6]->AsNumber(), Pk[8]->AsNumber())) continue;
			if (!bWant && bWorld) continue;
			Rays++;
			if (bGot != bWant) { if (Bad++ < 10) AddError(FString::Printf(TEXT("%s: ray %d hit %d, reference %d"), *Name, Rays, bGot, bWant)); continue; }
			if (!bWant) continue;
			const double Dd = FMath::Sqrt(FMath::Square(Hit[0] - Pk[6]->AsNumber()) + FMath::Square(Hit[1] - Pk[7]->AsNumber()) + FMath::Square(Hit[2] - Pk[8]->AsNumber()));
			WorstRay = FMath::Max(WorstRay, Dd);
			if (Dd > 1e-3) { if (Bad++ < 10) AddError(FString::Printf(TEXT("%s: ray hit %.3f m from the reference's"), *Name, Dd)); }
		}
	}
	AddInfo(FString::Printf(TEXT("relief oracle: %d cases, %d hAt samples (worst %.2e m), %d rays (worst %.2e m), %d mismatches"), Cases.Num(), Samples, WorstH, Rays, WorstRay, Bad));
	TestEqual(TEXT("cases"), Cases.Num(), 40);
	return Bad == 0;
}

// The actor on a small map with every relief feature (a ramp, a 3-level cliff, a trench, a road, a river with a ford and
// a bridge), then as a World map known only in part: centres per docs/contracts/terrain.md, HeightAt and RayToGround on
// the drawn triangles, the cliff's wall hit by a level ray, unknown cells never drawn nor shaping known ground, a reveal
// rebuilding only its chunks and matching a fresh build, and a fog flip that only recolours.
// Relief must use the same road cells as the paint, including map spawn exclusions, and
// refresh a road changed outside the caller's shape list (a discovered field can change as a block).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainPaintRoadSourceTest, "ThreeCrossroads.Terrain.PaintRoadSource", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainPaintRoadSourceTest::RunTest(const FString& Parameters)
{
	using namespace TCTerrainImpl;
	const int32 W = 40, H = 32;
	TArray<FString> Rows, Heights;
	Rows.Init(FString::ChrN(W, '.'), H); Heights.Init(FString::ChrN(W, '2'), H);
	TArray<TSharedPtr<FJsonValue>> Spawns;
	for (int32 Y = 4; Y < H; Y += 8) for (int32 X = 5; X < W; X += 10)
	{
		TSharedRef<FJsonObject> Spawn = MakeShared<FJsonObject>();
		Spawn->SetNumberField(TEXT("x"), X); Spawn->SetNumberField(TEXT("y"), Y);
		Spawns.Add(MakeShared<FJsonValueObject>(Spawn));
		if (X + 4 < W) Rows[Y][X + 4] = 'B';
	}
	const auto Start = StartMessage(TEXT("paint-road-source"), W, H, Rows, Heights, false, {});
	Start->GetObjectField(TEXT("map"))->SetArrayField(TEXT("spawns"), Spawns);
	FTCMatchState State;
	if (!TestTrue(TEXT("fixture start"), State.ApplyStart(*Start))) return false;
	FTCGroundPaint Paint;
	if (!TestTrue(TEXT("paint start"), Paint.ApplyStart(*Start, State))) return false;
	FScratchWorld Scratch(TEXT("PaintRoadSource"));
	ATCTerrain* Terrain = Scratch.World->SpawnActor<ATCTerrain>();
	Terrain->SetPaintSource(&Paint); Terrain->ResetFrom(State);
	std::vector<uint8_t> Roads; Paint.Painter().CopyRoadMask(Roads);
	TestTrue(TEXT("relief reads the painter's complete road mask"), Terrain->Relief().InRoad == Roads);
	ATCTerrain* Fallback = Scratch.World->SpawnActor<ATCTerrain>(); Fallback->ResetFrom(State);
	TestTrue(TEXT("fixture exposes missing map-spawn field exclusions in the old reconstruction"), Fallback->Relief().InRoad != Roads);
	const int32 Remote = W * H - 1;
	TSharedRef<FJsonObject> Snapshot = MakeShared<FJsonObject>();
	Snapshot->SetArrayField(TEXT("cells"), { MakeShared<FJsonValueArray>(TArray<TSharedPtr<FJsonValue>>{
		MakeShared<FJsonValueNumber>(Remote), MakeShared<FJsonValueString>(TEXT("D")) }) });
	Paint.ApplySnapshot(*Snapshot); State.Map.Chars[Remote] = 'D';
	Terrain->MarkCells(TSet<int32>{ 0 }, {}); Terrain->Flush(State);
	TestTrue(TEXT("a changed road outside the shape list reaches the relief"), Terrain->Relief().InRoad[(size_t)Remote] != 0);
	TestTrue(TEXT("the newly exposed road shoulder was rebuilt"), FMath::Abs(Terrain->HeightAt((W - .5f) * 2.f, (H - .5f) * 2.f) - 4.9f) < 1e-5f);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCTerrainPickingTest, "ThreeCrossroads.Terrain.Picking", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCTerrainPickingTest::RunTest(const FString& Parameters)
{
	using namespace TCTerrainImpl;
	const int32 W = 72, H = 40;
	auto Level = [](int32 X, int32 Y) { return X >= 40 && X < 44 ? 1 : X >= 44 && X < 46 ? 2 : X >= 46 && X < 56 ? 3 : (Y >= 26 && Y <= 30 && X < 36) ? -1 : 0; };
	auto Char = [](int32 X, int32 Y) -> TCHAR
	{
		if (Y >= 26 && Y <= 30 && X < 36) return X == 12 ? TEXT('=') : X == 22 ? TEXT('F') : TEXT('W');
		if (Y == 8 && X >= 4 && X < 12) return TEXT('T');
		if (X == 30 && Y < 24) return TEXT('D');
		return TEXT('.');
	};
	TArray<FString> Rows, Heights;
	for (int32 Y = 0; Y < H; Y++)
	{
		FString R, Hh;
		for (int32 X = 0; X < W; X++) { R.AppendChar(Char(X, Y)); const int32 L = Level(X, Y); Hh.AppendChar(L < 0 ? (TCHAR)(96 - L) : (TCHAR)('0' + L)); }
		Rows.Add(R); Heights.Add(Hh);
	}
	FScratchWorld Scratch(TEXT("TCTerrainPicking"));
	ATCTerrain* Terrain = Scratch.World->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity);
	if (!TestNotNull(TEXT("terrain"), Terrain)) return false;
	FTCMatchState State;
	TestTrue(TEXT("authored start"), State.ApplyStart(*StartMessage(TEXT("relief test"), W, H, Rows, Heights, false, {})));
	State.bFogLifted = true;
	Terrain->ResetFrom(State);
	for (int32 I = 0; I < 64 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
	TestEqual(TEXT("every cell drawn"), Terrain->CellsDrawn(), W * H);
	// centres: plain cells at level x 2.5 within 0.1 m, trench 0.9 m down, road 0.1 m down, ford 0.15 m under its line,
	// bridge at its deck
	int32 Plain = 0;
	for (int32 Y = 0; Y < H; Y++) for (int32 X = 0; X < W; X++)
	{
		const float Cx = (X + 0.5f) * 2.f, Cz = (Y + 0.5f) * 2.f, Nominal = Level(X, Y) * 2.5f, Got = Terrain->HeightAt(Cx, Cz);
		bool bBank = false;
		for (int32 Dy = -1; Dy <= 1; Dy++) for (int32 Dx = -1; Dx <= 1; Dx++)
		{
			const int32 Xx = X + Dx, Yy = Y + Dy;
			if (Xx >= 0 && Yy >= 0 && Xx < W && Yy < H && FString(TEXT("WF=")).Contains(FString::Chr(Char(Xx, Yy)))) bBank = true;
		}
		const TCHAR Ch = Char(X, Y);
		if (Ch == 'F') TestNearlyEqual(*FString::Printf(TEXT("ford %d,%d 0.15 m under the line"), X, Y), Got, Terrain->WaterAt(Cx, Cz) - 0.15f, 1e-3f);
		else if (Ch == '=') TestNearlyEqual(*FString::Printf(TEXT("bridge %d,%d at its deck"), X, Y), Got, Nominal, 0.1f);
		else if (Ch == 'T') TestNearlyEqual(*FString::Printf(TEXT("trench %d,%d"), X, Y), Got, Nominal - 0.9f, 0.1f);
		else if (Ch == 'D') TestNearlyEqual(*FString::Printf(TEXT("road %d,%d"), X, Y), Got, Nominal - 0.1f, 0.1f);
		else if (!bBank && Ch != 'W') { Plain++; if (FMath::Abs(Got - Nominal) > 0.1f) AddError(FString::Printf(TEXT("centre %d,%d at %.3f, level %.2f"), X, Y, Got, Nominal)); }
	}
	TestTrue(TEXT("plain centres checked"), Plain > 2000);
	TestNearlyEqual(TEXT("the water line: the river's frozen level + 0.06 m"), Terrain->WaterAt(41.f, 57.f), -2.5f + 0.06f, 1e-4f);
	// the 3-level cliff between columns 55 and 56: two distinct heights on either side, a wall between
	TestNearlyEqual(TEXT("cliff top"), Terrain->HeightAt(111.f, 20.f), 7.5f, 0.1f);
	TestNearlyEqual(TEXT("cliff foot"), Terrain->HeightAt(113.f, 20.f), 0.f, 0.1f);
	FVector2D Hit;
	TestTrue(TEXT("a level ray finds the cliff's rock face"), Terrain->RayToGround(FVector(13000.0, 2000.0, 300.0), FVector(-1, 0, 0), Hit) && Hit.X > 109.0 && Hit.X < 114.0);
	// vertical rays land under themselves, on the drawn ground
	FRandomStream Rnd(1013);
	int32 Down = 0, DownBad = 0, Slant = 0, SlantBad = 0;
	for (int32 I = 0; I < 400; I++)
	{
		const float X = Rnd.FRandRange(0.5f, W * 2.f - 0.5f), Z = Rnd.FRandRange(0.5f, H * 2.f - 0.5f);
		if (Terrain->RayToGround(FVector(X * 100.0, Z * 100.0, 5000.0), FVector(0, 0, -1), Hit))
		{
			Down++;
			if (FVector2D::Distance(Hit, FVector2D(X, Z)) > 1e-3) DownBad++;
		}
		else DownBad++;
		// camera-like rays: the hit is where the ray meets HeightAt (away from the vertical rock face)
		const float Yaw = Rnd.FRandRange(0.f, 2.f * PI), Pitch = Rnd.FRandRange(0.5f, 1.4f), Dist = Rnd.FRandRange(30.f, 160.f);
		const FVector Eye = ATCTerrain::ToWorld(X + FMath::Sin(Yaw) * FMath::Cos(Pitch) * Dist, Z + FMath::Cos(Yaw) * FMath::Cos(Pitch) * Dist, FMath::Sin(Pitch) * Dist);
		const FVector Dir = (ATCTerrain::ToWorld(X, Z, 0.f) - Eye).GetSafeNormal();
		if (Terrain->RayToGround(Eye, Dir, Hit) && FMath::Abs(Hit.X - 112.0) > 1.5)
		{
			const double T = (FVector2D(Hit.X * 100.0, Hit.Y * 100.0) - FVector2D(Eye.X, Eye.Y)).Size() / FVector2D(Dir.X, Dir.Y).Size();
			const double Z3 = (Eye.Z + Dir.Z * T) / 100.0;
			Slant++;
			if (FMath::Abs(Z3 - Terrain->HeightAt(Hit.X, Hit.Y)) > 0.02) { if (SlantBad++ < 5) AddError(FString::Printf(TEXT("ray hit (%.2f, %.2f) at %.3f m, ground %.3f m"), Hit.X, Hit.Y, Z3, Terrain->HeightAt(Hit.X, Hit.Y))); }
		}
	}
	TestEqual(TEXT("vertical rays land under themselves"), DownBad, 0);
	TestTrue(TEXT("camera rays checked"), Slant > 300);
	TestEqual(TEXT("camera rays meet HeightAt"), SlantBad, 0);
	// a fog flip only recolours
	const int32 Meshed = Terrain->ChunksMeshed(), Verts0 = Terrain->SectionVertices(0);
	Terrain->MarkCells({}, { 5, 6 });
	Terrain->Flush(State);
	TestEqual(TEXT("a fog flip rebuilds nothing"), Terrain->ChunksMeshed(), Meshed);
	TestEqual(TEXT("a fog flip keeps the vertices"), Terrain->SectionVertices(0), Verts0);

	// World: known only west of column 50 (the plateau's west half), the rest '.' at level 0 in start.map
	TArray<FString> WRows, WHeights;
	for (int32 Y = 0; Y < H; Y++) { WRows.Add(FString::ChrN(W, '.')); WHeights.Add(FString::ChrN(W, '0')); }
	auto CellRow = [&](int32 C) { TArray<TSharedPtr<FJsonValue>> R; R.Add(MakeShared<FJsonValueNumber>(C)); R.Add(MakeShared<FJsonValueString>(FString::Chr(Char(C % W, C / W)))); R.Add(MakeShared<FJsonValueNumber>(Level(C % W, C / W))); R.Add(MakeShared<FJsonValueNumber>(0)); return MakeShared<FJsonValueArray>(R); };
	TArray<TSharedPtr<FJsonValue>> Cells;
	int32 Known = 0;
	for (int32 C = 0; C < W * H; C++) if (C % W < 50) { Cells.Add(CellRow(C)); Known++; }
	FTCMatchState WS;
	TestTrue(TEXT("World start"), WS.ApplyStart(*StartMessage(TEXT("relief world test"), W, H, WRows, WHeights, true, Cells)));
	WS.bFogLifted = true;
	Terrain->ResetFrom(WS);
	for (int32 I = 0; I < 64 && Terrain->ChunksQueued(); I++) Terrain->Flush(WS);
	TestEqual(TEXT("only known cells drawn"), Terrain->CellsDrawn(), Known);
	auto OwnersKnown = [&]()
	{
		const TCRelief::FRelief& R = Terrain->Relief();
		for (int32 P = 0; P < R.NumParts(); P++) for (const int32 O : R.Part(P).Owner) if (!WS.Map.Known[O]) return false;
		return true;
	};
	TestTrue(TEXT("no vertex belongs to an unknown cell"), OwnersKnown());
	TestNearlyEqual(TEXT("the known plateau edge keeps its height (the unknown neighbour is not level 0)"), Terrain->HeightAt(99.5f, 20.f), 7.5f, 1e-4f);
	TestNearlyEqual(TEXT("its centre too"), Terrain->HeightAt(99.f, 21.f), 7.5f, 1e-4f);
	// scouting columns 50 to 59 (the plateau's east half, the cliff and its foot)
	TSet<int32> Revealed;
	for (int32 C = 0; C < W * H; C++)
		if (C % W >= 50 && C % W < 60) { WS.Map.Known[C] = true; WS.Map.Chars[C] = (uint8)Char(C % W, C / W); WS.Map.Levels[C] = (int8)Level(C % W, C / W); Revealed.Add(C); }
	const int32 Before = Terrain->ChunksMeshed();
	Terrain->MarkCells(Revealed, {});
	for (int32 I = 0; I < 64 && (I == 0 || Terrain->ChunksQueued()); I++) Terrain->Flush(WS);
	TestTrue(TEXT("the reveal rebuilt some chunks, not all"), Terrain->ChunksMeshed() - Before > 0 && Terrain->ChunksMeshed() - Before < 6);
	TestEqual(TEXT("revealed cells drawn"), Terrain->CellsDrawn(), Known + Revealed.Num());
	TestTrue(TEXT("still no vertex of an unknown cell"), OwnersKnown());
	TestNearlyEqual(TEXT("the revealed cliff foot"), Terrain->HeightAt(113.f, 20.f), 0.f, 0.1f);
	// the incremental relief equals one built at once from the same inputs
	TCRelief::FRelief Fresh;
	const TCRelief::FRelief& Live = Terrain->Relief();
	Fresh.Reset(W, H, true, ATCTerrain::ChunkCells, true);
	Fresh.InLevel = Live.InLevel; Fresh.InGrid = Live.InGrid; Fresh.InRoad = Live.InRoad; Fresh.InKnown = Live.InKnown; Fresh.InFrozen = Live.InFrozen;
	Fresh.Build();
	bool bSame = Fresh.NumParts() == Live.NumParts();
	for (int32 P = 0; bSame && P < Live.NumParts(); P++) bSame = Fresh.Part(P).Pos == Live.Part(P).Pos && Fresh.Part(P).Idx == Live.Part(P).Idx && Fresh.Part(P).Paint == Live.Part(P).Paint;
	TestTrue(TEXT("incremental relief equals a fresh build"), bSame);
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS

UTCReliefShotCommandlet::UTCReliefShotCommandlet()
{
	LogToConsole = true;
}

int32 UTCReliefShotCommandlet::Main(const FString& Params)
{
	using namespace TCTerrainImpl;
	FString MapPath, OutDir, ViewsArg, Label;
	FParse::Value(*Params, TEXT("Map="), MapPath);
	FParse::Value(*Params, TEXT("Out="), OutDir);
	FParse::Value(*Params, TEXT("Views="), ViewsArg, false);
	FParse::Value(*Params, TEXT("Name="), Label);
	if (MapPath.IsEmpty() || OutDir.IsEmpty()) { UE_LOG(LogTC, Error, TEXT("usage: -run=TCReliefShot -Map=<maps/x.json> -Out=<dir> [-Views=x,z,dist,yaw;...] [-Name=label]")); return 1; }
	if (Label.IsEmpty()) Label = FPaths::GetBaseFilename(MapPath);
	IFileManager::Get().MakeDirectory(*OutDir, true);
	FString Text;
	TSharedPtr<FJsonObject> File;
	if (!FFileHelper::LoadFileToString(Text, *MapPath) || !FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), File) || !File.IsValid()) { UE_LOG(LogTC, Error, TEXT("cannot read %s"), *MapPath); return 1; }
	TSharedRef<FJsonObject> Map = MakeShared<FJsonObject>();
	for (const TCHAR* F : { TEXT("w"), TEXT("h"), TEXT("rows"), TEXT("heights"), TEXT("spawns"), TEXT("points") }) if (File->HasField(F)) Map->SetField(F, File->TryGetField(F));
	Map->SetStringField(TEXT("name"), Label);
	TSharedRef<FJsonObject> Start = MakeShared<FJsonObject>();
	Start->SetStringField(TEXT("t"), TEXT("start")); Start->SetObjectField(TEXT("map"), Map);
	Start->SetNumberField(TEXT("you"), 0); Start->SetNumberField(TEXT("matchId"), 1); Start->SetArrayField(TEXT("cells"), {});
	FScratchWorld Scratch(TEXT("TCReliefShot"));
	UWorld* World = Scratch.World;
	UObject* Outer = World->GetWorldSettings();
	FTCMatchState State;
	if (!State.ApplyStart(*Start)) { UE_LOG(LogTC, Error, TEXT("the start does not apply")); return 1; }
	State.bFogLifted = true;
	ATCTerrain* Terrain = World->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity);
	Terrain->ResetFrom(State);
	for (int32 I = 0; I < 256 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
	UE_LOG(LogTC, Display, TEXT("RELIEFSHOT %s: %d vertices, %d triangles, %d cliff edges, material %s"), *Label, Terrain->Relief().Stats.Vertices, Terrain->Relief().Stats.Triangles,
		Terrain->Relief().Stats.CliffEdges, *GetNameSafe(LoadObject<UMaterialInterface>(nullptr, ReliefMaterialPath)));
	// the game's light, default mood (as UTCWorldLookCommandlet sets it up)
	const FTCMood& Mood = TCLook::DefaultMood();
	const float Env = TCLook::EnvironmentIntensity(Mood.SunI);
	UDirectionalLightComponent* Sun = NewObject<UDirectionalLightComponent>(Outer);
	Sun->SetMobility(EComponentMobility::Movable);
	Sun->SetWorldRotation((-TCLook::ThreeToUnreal(TCLook::SunDirectionThree(Mood.SunUp, 0.f))).Rotation());
	Sun->SetLightColor(Mood.Sun); Sun->SetIntensity(Mood.SunI); Sun->SetShadowAmount(Mood.Shadow);
	Sun->SetAtmosphereSunLight(false); Sun->SetDynamicShadowDistanceMovableLight(40000.f);
	Sun->RegisterComponentWithWorld(World);
	USkyLightComponent* Sky = NewObject<USkyLightComponent>(Outer);
	Sky->SetMobility(EComponentMobility::Movable);
	Sky->SourceType = SLS_SpecifiedCubemap; Sky->bLowerHemisphereIsBlack = false;
	Sky->Cubemap = TCLook::MakeSkyCube(Mood, Env, 0.f);
	Sky->SetIntensity(1.f); Sky->SetCastShadows(false);
	Sky->RegisterComponentWithWorld(World);
	TCLook::SetCollection(World, Mood, Env, 0.f, TCLook::IndirectInMaterials());
	FlushRenderingCommands();
	for (int32 Try = 0; Try < 3; Try++) { Sky->RecaptureSky(); USkyLightComponent::UpdateSkyCaptureContents(World); FlushRenderingCommands(); }
	UMaterialInterface* PostBase = LoadObject<UMaterialInterface>(nullptr, TCLook::PostPath);
	UMaterialInstanceDynamic* PostMid = PostBase ? UMaterialInstanceDynamic::Create(PostBase, Outer) : nullptr;
	FinishCompiling();
	const int32 PW = 1280, PH = 720;
	const double Pitch = 0.95, VFov = 42.0;
	const double HFov = FMath::RadiansToDegrees(2.0 * FMath::Atan(FMath::Tan(FMath::DegreesToRadians(VFov) / 2.0) * PW / PH));
	if (ViewsArg.IsEmpty()) ViewsArg = FString::Printf(TEXT("%d,%d,%d,0"), State.Map.W, State.Map.H, FMath::Max(State.Map.W, State.Map.H) * 2);
	TArray<FString> Views;
	ViewsArg.ParseIntoArray(Views, TEXT(";"));
	int32 Failures = 0;
	for (const FString& VS : Views)
	{
		TArray<FString> P;
		VS.ParseIntoArray(P, TEXT(","));
		if (P.Num() != 4) { Failures++; continue; }
		const double X = FCString::Atod(*P[0]), Z = FCString::Atod(*P[1]), Dist = FCString::Atod(*P[2]), Yaw = FCString::Atod(*P[3]);
		const double Ground = Terrain->HeightAt((float)X, (float)Z);
		// client/camera.js: the camera at focus + (sin yaw, ., cos yaw) times the horizontal distance, up dist x sin pitch
		const double Hd = Dist * FMath::Cos(Pitch);
		const FVector Eye = TCLook::ThreeToUnreal(FVector(X + FMath::Sin(Yaw) * Hd, Ground + Dist * FMath::Sin(Pitch), Z + FMath::Cos(Yaw) * Hd)) * 100.0;
		const FVector Target = TCLook::ThreeToUnreal(FVector(X, Ground, Z)) * 100.0;
		UTextureRenderTarget2D* Target2D = NewObject<UTextureRenderTarget2D>(Outer);
		Target2D->RenderTargetFormat = RTF_RGBA8;
		Target2D->ClearColor = TCLook::ToDisplay(Mood.Haze);
		Target2D->InitAutoFormat(PW, PH);
		Target2D->UpdateResourceImmediate(true);
		USceneCaptureComponent2D* Cap = NewObject<USceneCaptureComponent2D>(Outer);
		Cap->TextureTarget = Target2D; Cap->CaptureSource = SCS_FinalColorLDR;
		Cap->bCaptureEveryFrame = false; Cap->bCaptureOnMovement = false; Cap->bAlwaysPersistRenderingState = true;
		Cap->FOVAngle = HFov;
		TCLook::LockPostProcess(Cap->PostProcessSettings, PostMid, 0.f);
		TCLook::SetPost(PostMid, Mood.Exposure, 50.f, 300.f, Mood.Haze);
		Cap->PostProcessBlendWeight = 1.f;
		Cap->SetWorldLocationAndRotation(Eye, (Target - Eye).Rotation());
		Cap->RegisterComponentWithWorld(World);
		for (int32 F = 0; F < 6; F++) { World->SendAllEndOfFrameUpdates(); Cap->CaptureScene(); FlushRenderingCommands(); }
		TArray<FColor> Color;
		FTextureRenderTargetResource* Res = Target2D->GameThread_GetRenderTargetResource();
		const FString Png = FPaths::Combine(OutDir, FString::Printf(TEXT("%s-%s-%s-%s-%s.png"), *Label, *P[0], *P[1], *P[2], *P[3]));
		if (Res && Res->ReadPixels(Color) && Color.Num() == PW * PH && SavePng(Png, PW, PH, Color)) { UE_LOG(LogTC, Display, TEXT("RELIEFSHOT wrote %s"), *Png); }
		else { Failures++; UE_LOG(LogTC, Error, TEXT("RELIEFSHOT capture failed for %s"), *VS); }
		Cap->DestroyComponent();
	}
	UE_LOG(LogTC, Display, TEXT("RELIEFSHOT %s: %d views, %d failures"), *Label, Views.Num(), Failures);
	return Failures ? 1 : 0;
}
