#include "World/TCWorldObjects.h"
#include "Client/TCGraphics.h"
#include "Client/TCLighting.h"
#include "Client/TCEffects.h"
#include "EngineUtils.h"
#include "ThreeCrossroads.h"
#include "Net/TCMatchState.h"
#include "World/TCTerrain.h"
#include "Client/TCUnitTable.h"
#include "Client/TCCoverPreview.h"
#include "World/TCFogFrontier.h"
#include "Components/HierarchicalInstancedStaticMeshComponent.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Components/SceneComponent.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Engine/StaticMesh.h"
#include "Engine/Texture2D.h"
#include "Engine/World.h"
#include "GameFramework/Actor.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/MaterialInterface.h"
#include "Misc/Crc.h"
#include "StaticMeshResources.h"
#include "TextureResource.h"
#include "RHITypes.h"

static_assert(TCWorldCore::CellMetres == FTCMatchState::CellMetres, "one cell size");

const TCHAR* const FTCWorldObjects::ReferencePin = TEXT("e4e579a");

namespace TCWorldObjectsImpl
{
	constexpr int32 MaxSide = 4096; // as FTCMatchState: a start past this is refused

	bool Num(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return FMath::IsFinite(Out);
	}
	// FTCMatchState's IntOr: a number clamped to int32, else the fallback.
	int32 IntOr(const TSharedPtr<FJsonValue>& V, int32 Fallback)
	{
		double D;
		return Num(V, D) ? (int32)FMath::Clamp(D, -2147483648.0, 2147483647.0) : Fallback;
	}
	// A Uint8Array element store (ToUint8) of a JSON number; anything else stores 0 (null, a missing field).
	uint8 ToUint8(const TSharedPtr<FJsonValue>& V)
	{
		double D;
		if (!Num(V, D)) return 0;
		const double T = FMath::Fmod(D < 0 ? FMath::CeilToDouble(D) : FMath::FloorToDouble(D), 256.0);
		return (uint8)(int32)(T < 0 ? T + 256.0 : T);
	}
	// The first character of a terrain string, as FTCMatchState stores it.
	uint8 CharOf(const FString& S) { return S.IsEmpty() ? 0 : (uint8)FMath::Clamp((int32)S[0], 0, 127); }
	uint8 CharAt(const FString& S, int32 I) { return I < S.Len() ? (uint8)FMath::Clamp((int32)S[I], 0, 127) : 0; }
	// shared/world-layers.js migrateWorldMap for one cell of a row without layers.
	uint8 MigratedGround(uint8 Ch) { return TCWorldCore::IsBaseSurface(Ch) ? Ch : Ch == '=' ? 'W' : '.'; }
	uint8 MigratedObject(uint8 Ch) { return TCWorldCore::IsBaseSurface(Ch) || Ch == 'N' ? '.' : Ch; }
	// The CFG.houses index of a building kind name (case sensitive, as JavaScript compares), -1 if none.
	int32 HouseTypeNamed(const FString& Kind)
	{
		for (int32 I = 0; I < TCWorldCore::HouseTypeCount; I++) if (Kind.Equals(ANSI_TO_TCHAR(TCWorldCore::HouseTypes[I].Name), ESearchCase::CaseSensitive)) return I;
		return -1;
	}
	uint64 HouseKey(const TCWorldCore::FHouseRect& R)
	{
		return (uint64)R.R.X0 | (uint64)R.R.Y0 << 13 | (uint64)R.R.X1 << 26 | (uint64)R.R.Y1 << 39 | (uint64)R.Look << 52 | (uint64)(R.Type & 7) << 56;
	}
	// A cell list [x, y] of the start map (spawns, points), null entries skipped.
	void ReadCells(const FJsonObject& JMap, const TCHAR* Field, TArray<TCScatterCore::FCellXY>& Out)
	{
		Out.Reset();
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (!JMap.TryGetArrayField(Field, A)) return;
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const TSharedPtr<FJsonObject>* O;
			double X, Y;
			if (!V.IsValid() || !V->TryGetObject(O) || !O->IsValid() || !(*O)->TryGetNumberField(TEXT("x"), X) || !(*O)->TryGetNumberField(TEXT("y"), Y)) continue;
			TCScatterCore::FCellXY P;
			P.X = (int32)X; P.Y = (int32)Y;
			Out.Add(P);
		}
	}
	// Reference metres and a reference rotation (x, y, z, w) to an Unreal transform: (x, y, z) -> (X, Y, Z) = (x, z, y)
	// * 100 is a reflection, so a rotation about axis a by t becomes one about the swapped axis by -t.
	FTransform ToUnreal(double X, double Y, double Z, double Sx, double Sy, double Sz, const double Q[4])
	{
		return FTransform(FQuat(-Q[0], -Q[2], -Q[1], Q[3]), FVector(X * 100.0, Z * 100.0, Y * 100.0), FVector(Sx, Sz, Sy));
	}
	// The custom data floats every world material reads (make-world-materials.py): tint (linear), the bark grey, and a
	// roof's tile scale (structures.js roofShader: uv times the ridge and slope lengths, in 1.6 m tiles).
	constexpr int32 WorldFloats = 6;
	constexpr float RoofTile = 1.6f;
}

// What the piece builders read: this client's grids and the ground as ATCTerrain draws it.
class FTCWorldObjects::FSource final : public TCWorldCore::FCellSource
{
public:
	FSource(const FTCWorldObjects& InObjects, const ATCTerrain& InGround) : Objects(InObjects), Ground_(InGround) {}
	int Width() const override { return Objects.W; }
	int Height() const override { return Objects.H; }
	unsigned char At(int X, int Y) const override { return In(X, Y) ? Objects.Physical(Y * Objects.W + X) : 0; }
	unsigned char Orig(int X, int Y) const override { return In(X, Y) ? Objects.Orig(Y * Objects.W + X) : 0; }
	int Stage(int X, int Y) const override { return In(X, Y) ? TCWorldCore::StageOf(Objects.States[Y * Objects.W + X]) : 0; }
	bool Mine(int X, int Y) const override { return In(X, Y) && Objects.Grid[Y * Objects.W + X] == 'N'; }
	bool StoneDeck(int X, int Y) const override { return In(X, Y) && Objects.StoneDeckMask[Y * Objects.W + X] != 0; }
	bool Spoil(int X, int Y) const override { return In(X, Y) && Objects.SpoilCells.Contains(Y * Objects.W + X); }
	double Ground(double X, double Z) const override { return Ground_.HeightAt((float)X, (float)Z); }
	// round 15: structures.js C.low (gfx.low when the pieces are built)
	bool Low() const override { return Objects.bLow; }
	// slice 13 lead glue: the relief (World/TCRelief.h) carves trench cells TRENCH_DEPTH down as client/relief.js does, so
	// the ground at a trench centre is the carved floor structures.js reads (before slice 13 the flat placeholder ground was
	// not carved and this answered Ground - TRENCH_DEPTH; with the relief that sank the parapets into the ground)
	double TrenchFloor(double X, double Z) const override { return Ground(X, Z); }

private:
	bool In(int X, int Y) const { return X >= 0 && Y >= 0 && X < Objects.W && Y < Objects.H; }
	const FTCWorldObjects& Objects;
	const ATCTerrain& Ground_;
};

// What the scenery's live filter reads: the physical grid, the levels, the drawn ground and the railway cells.
class FTCWorldObjects::FLive final : public TCScatterCore::FLiveSource
{
public:
	FLive(const FTCWorldObjects& InObjects, const ATCTerrain& InGround) : Objects(InObjects), Ground_(InGround) {}
	int Width() const override { return Objects.W; }
	int Height() const override { return Objects.H; }
	unsigned char At(int X, int Y) const override { return X >= 0 && Y >= 0 && X < Objects.W && Y < Objects.H ? Objects.Physical(Y * Objects.W + X) : 0; }
	int Level(int X, int Y) const override { return Objects.LevelAt(X, Y); }
	double Ground(double X, double Z) const override { return Ground_.HeightAt((float)X, (float)Z); }
	bool Skip(int Cell) const override { return Objects.RailCells.Contains(Cell); }
	// round 15: props.js place() and visible() read gfx.low
	bool Low() const override { return Objects.bLow; }

private:
	const FTCWorldObjects& Objects;
	const ATCTerrain& Ground_;
};

uint8 FTCWorldObjects::Physical(int32 C) const
{
	if (!Grid.IsValidIndex(C)) return 0;
	const uint8 Ch = Grid[C];
	// client/main.js physicalGrid: a known mine is drawn as the ground and object under it
	return Ch != 'N' ? Ch : TCWorldCore::Compose(GroundL[C], ObjectL[C]);
}

uint8 FTCWorldObjects::Orig(int32 C) const
{
	// client/main.js lastStart.map.rows: the map file's rows on an authored map; on a World map the rows follow every
	// cell this client is sent (applyCells), so they are the grid. A mine in them reads as what is under it.
	if (!Grid.IsValidIndex(C)) return 0;
	const uint8 Raw = bWorld ? Grid[C] : StartChars[C];
	return Raw == 'N' ? TCWorldCore::Compose(GroundL[C], ObjectL[C]) : Raw;
}

bool FTCWorldObjects::ApplyStart(const FJsonObject& M, const FTCMatchState& State)
{
	using namespace TCWorldObjectsImpl;
	const TSharedPtr<FJsonObject>* MapObj;
	if (!M.TryGetObjectField(TEXT("map"), MapObj) || !MapObj->IsValid()) return false;
	const FJsonObject& JMap = **MapObj;
	double Wd = 0, Hd = 0;
	const TArray<TSharedPtr<FJsonValue>>* Rows;
	if (!JMap.TryGetNumberField(TEXT("w"), Wd) || !JMap.TryGetNumberField(TEXT("h"), Hd) || !JMap.TryGetArrayField(TEXT("rows"), Rows)) return false;
	if (Wd < 1 || Hd < 1 || Wd > MaxSide || Hd > MaxSide || Rows->Num() != (int32)Hd) return false;
	const int32 NewW = (int32)Wd, NewH = (int32)Hd;
	const bool bNewWorld = JMap.HasField(TEXT("world"));
	FString MapName;
	JMap.TryGetStringField(TEXT("name"), MapName);

	// A reconnect to the match already held keeps what is drawn while it is rebuilt (as ATCTerrain::ResetFrom).
	bLastKept = HasMap() && State.MatchId >= 0 && State.MatchId == BuiltMatch && !State.RoomCode.IsEmpty() && State.RoomCode == BuiltRoom
		&& State.You == BuiltYou && State.bSpectator == bBuiltSpectator && bNewWorld == bWorld && MapName == BuiltMap && NewW == W && NewH == H;
	BuiltMatch = State.MatchId; BuiltRoom = State.RoomCode; BuiltYou = State.You; bBuiltSpectator = State.bSpectator; BuiltMap = MapName;

	W = NewW; H = NewH; bWorld = bNewWorld;
	const int32 N = W * H;
	ChunksX = FMath::DivideAndRoundUp(W, ChunkCells); ChunksY = FMath::DivideAndRoundUp(H, ChunkCells);
	Grid.Init('.', N);
	for (int32 Y = 0; Y < H; Y++)
	{
		FString Row;
		if ((*Rows)[Y].IsValid()) (*Rows)[Y]->TryGetString(Row);
		for (int32 X = 0; X < W && X < Row.Len(); X++) Grid[Y * W + X] = CharAt(Row, X);
	}
	StartChars = Grid;
	// client/main.js startGame: worldLayers(map) gives the ground and object grids (mines kept apart)
	GroundL.SetNumUninitialized(N); ObjectL.SetNumUninitialized(N);
	for (int32 C = 0; C < N; C++) { GroundL[C] = MigratedGround(Grid[C]); ObjectL[C] = MigratedObject(Grid[C]); }
	const TSharedPtr<FJsonObject>* Layers;
	if (JMap.TryGetObjectField(TEXT("layers"), Layers) && Layers->IsValid())
	{
		const TArray<TSharedPtr<FJsonValue>>* GroundRows = nullptr;
		const TArray<TSharedPtr<FJsonValue>>* ObjectRows = nullptr;
		const bool bGround = (*Layers)->TryGetArrayField(TEXT("ground"), GroundRows) && GroundRows->Num() == H;
		const bool bObjects = (*Layers)->TryGetArrayField(TEXT("objects"), ObjectRows) && ObjectRows->Num() == H;
		for (int32 Y = 0; Y < H; Y++)
		{
			FString G, O;
			if (bGround && (*GroundRows)[Y].IsValid() && (*GroundRows)[Y]->TryGetString(G))
				for (int32 X = 0; X < W && X < G.Len(); X++) GroundL[Y * W + X] = CharAt(G, X);
			if (bObjects && (*ObjectRows)[Y].IsValid() && (*ObjectRows)[Y]->TryGetString(O))
				for (int32 X = 0; X < W && X < O.Len(); X++) { const uint8 Ch = CharAt(O, X); ObjectL[Y * W + X] = Ch == 'N' ? '.' : Ch; }
		}
	}
	// terrain.state = Uint8Array.from(map.rows.join(''), startState)
	States.SetNumUninitialized(N);
	for (int32 C = 0; C < N; C++) States[C] = TCWorldCore::StartState(Grid[C], C);
	// map.heights: one level character per cell (shared/sim.js levelOf); absent: all level 0
	Levels.Init(0, N);
	const TArray<TSharedPtr<FJsonValue>>* Heights;
	if (JMap.TryGetArrayField(TEXT("heights"), Heights) && Heights->Num() == H)
		for (int32 Y = 0; Y < H; Y++)
		{
			FString Row;
			if ((*Heights)[Y].IsValid() && (*Heights)[Y]->TryGetString(Row))
				for (int32 X = 0; X < W && X < Row.Len(); X++) Levels[Y * W + X] = (int8)FMath::Clamp(TCScatterCore::LevelOf((unsigned char)FMath::Clamp((int32)Row[X], 0, 255)), -127, 127);
		}
	// the scenery's inputs: spawns and points (props.js zones and paths), a World map's region looks and discovery
	ReadCells(JMap, TEXT("spawns"), Spawns);
	ReadCells(JMap, TEXT("points"), Points);
	Biome.Reset();
	const TArray<TSharedPtr<FJsonValue>>* BiomeArr;
	if (JMap.TryGetArrayField(TEXT("biome"), BiomeArr) && BiomeArr->Num() == N)
	{
		Biome.SetNumUninitialized(N);
		for (int32 C = 0; C < N; C++) Biome[C] = (uint8)FMath::Clamp(IntOr((*BiomeArr)[C], 255), 0, 255);
	}
	Discovered.Reset();
	if (bWorld) Discovered.Init(0, N);
	RailCells.Reset(); Nodes.Reset(); NodesChanged.Reset(); bNodesChanged = false; bDiscoveryDue = false;

	// The map's buildings (a house type by name, a stone bridge span, a spoil tip); World maps add theirs as cells
	// are discovered (client/main.js rememberLook).
	Named.Reset(); Landmarks.Reset(); SpoilCells.Reset(); StoneBridgeSeeds.Reset();
	NamedCount = 0;
	const TArray<TSharedPtr<FJsonValue>>* Buildings;
	if (JMap.TryGetArrayField(TEXT("buildings"), Buildings))
	{
		NamedCount = Buildings->Num();
		for (const TSharedPtr<FJsonValue>& V : *Buildings)
		{
			const TSharedPtr<FJsonObject>* B;
			double Bx, By;
			FString Kind;
			if (!V.IsValid() || !V->TryGetObject(B) || !B->IsValid() || !(*B)->TryGetNumberField(TEXT("x"), Bx) || !(*B)->TryGetNumberField(TEXT("y"), By) || !(*B)->TryGetStringField(TEXT("kind"), Kind)) continue;
			if (Bx < 0 || By < 0 || Bx >= W || By >= H || Bx != FMath::FloorToDouble(Bx) || By != FMath::FloorToDouble(By)) continue;
			const int32 Cell = (int32)By * W + (int32)Bx;
			const int32 Type = HouseTypeNamed(Kind);
			if (Type >= 0) Named.Add(Cell, Type);
			if (Kind.Equals(TEXT("stone bridge"), ESearchCase::CaseSensitive)) StoneBridgeSeeds.Add(Cell);
			if (Kind.Equals(TEXT("spoil"), ESearchCase::CaseSensitive)) SpoilCells.Add(Cell);
		}
	}
	StoneDeckMask.Init(0, N);

	// the start's cells: this side's memory of what changed (client/main.js startGame applies each row)
	const TArray<TSharedPtr<FJsonValue>>* Cells;
	if (M.TryGetArrayField(TEXT("cells"), Cells))
		for (const TSharedPtr<FJsonValue>& Entry : *Cells)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			if (Entry.IsValid() && Entry->TryGetArray(Row)) ApplyCellRow(*Row);
		}

	// a start empties the wrecks (hulks.clear; the next snapshot sends the list in full)
	WreckRows.Reset(); CoverRows.Reset();
	bWrecksDirty = true;

	Pending.Reset(); Shaped.Reset(); bWhole = true;
	// round 15: a start lays out at the level of the moment (structures.js C.low, props.js gfx.low); later changes
	// arrive through the watch
	GfxWatch.TakeChanged();
	bLow = FTCGraphics::Get().IsLow();
	Foot.clear(); Blocks.clear(); Rects.clear(); TintIndex.clear(); Tints.clear();
	Houses.Reset();
	HouseType.Init(-1, N);
	ChunkHouses.Reset(); ChunkHouses.SetNum(ChunksX * ChunksY);
	ChunkPieces.Reset(); ChunkPieces.SetNum(ChunksX * ChunksY);
	ChunkPicks.Reset(); ChunkPicks.SetNum(ChunksX * ChunksY);
	ChunkPick.Reset(); ChunkPick.SetNum(ChunksX * ChunksY);
	DirtyChunks.Reset(); DirtyPropChunks.Reset();
	Candidates.clear(); Placed.clear();
	CandidateAt.Init(-1, N);
	ChunkCandidates.Reset(); ChunkCandidates.SetNum(ChunksX * ChunksY);
	NearMask.Init(0, N);
	LaidNamedCount = -1;
	Gen++;
	return true;
}

int32 FTCWorldObjects::ApplyCellRow(const TArray<TSharedPtr<FJsonValue>>& Row)
{
	using namespace TCWorldObjectsImpl;
	// [cell, char, level, state, data] (shared/sim.js logCell, seenWorldCell), read as FTCMatchState reads it
	if (Row.Num() < 2) return -1;
	const int32 C = IntOr(Row[0], -1);
	FString Ch;
	if (C < 0 || C >= W * H || !Row[1].IsValid() || !Row[1]->TryGetString(Ch) || Ch.IsEmpty()) return -1;
	const uint8 OldChar = Grid[C], OldPhysical = Physical(C);
	const int8 OldLevel = Levels[C];
	Grid[C] = CharOf(Ch);
	States[C] = Row.Num() > 3 ? ToUint8(Row[3]) : 0; // client/main.js applyCells: st = 0 when absent
	double Lv;
	if (Row.Num() > 2 && Num(Row[2], Lv)) Levels[C] = (int8)FMath::Clamp((int32)Lv, -127, 127); // setLevel
	const TSharedPtr<FJsonObject>* Data = nullptr;
	if (Row.Num() > 4 && Row[4].IsValid() && Row[4]->TryGetObject(Data) && Data && Data->IsValid())
	{
		FString G, O;
		if ((*Data)->TryGetStringField(TEXT("ground"), G) && !G.IsEmpty()) GroundL[C] = CharOf(G);
		if ((*Data)->TryGetStringField(TEXT("object"), O) && !O.IsEmpty()) ObjectL[C] = CharOf(O);
		// client/railways.js cells: a track or station cell; the scenery skips it
		double Rail;
		if (((*Data)->TryGetNumberField(TEXT("rail"), Rail) && Rail != 0) || (*Data)->HasField(TEXT("station"))) RailCells.Add(C);
		if (bWorld) RememberLook(C, **Data);
	}
	if (bWorld && Discovered.IsValidIndex(C) && !Discovered[C]) { Discovered[C] = 1; bDiscoveryDue = true; }
	// props.js refresh: only cells whose type, physical type or height moved
	if (Grid[C] != OldChar || Physical(C) != OldPhysical || Levels[C] != OldLevel) Shaped.Add(C);
	return C;
}

void FTCWorldObjects::RememberLook(int32 Cell, const FJsonObject& Data)
{
	using namespace TCWorldObjectsImpl;
	// client/main.js rememberLook: a discovered cell's region look (client/props.js dresses it) ...
	double B;
	if (Data.TryGetNumberField(TEXT("biome"), B))
	{
		if (!Biome.Num()) Biome.Init(255, W * H);
		const uint8 Now = (uint8)FMath::Clamp((int32)B, 0, 255);
		if (Biome[Cell] != Now) { Biome[Cell] = Now; bDiscoveryDue = true; }
	}
	// ... and a landmark's name, or a mine's spoil tip (a cell that started as rubble), remembered once
	FString Kind;
	if (!Data.TryGetStringField(TEXT("landmark"), Kind))
	{
		const TArray<TSharedPtr<FJsonValue>>* Initial;
		FString First;
		if (Data.TryGetArrayField(TEXT("initial"), Initial) && Initial->Num() > 0 && (*Initial)[0].IsValid() && (*Initial)[0]->TryGetString(First) && First.Equals(TEXT("R"), ESearchCase::CaseSensitive))
			Kind = TEXT("spoil");
	}
	if (Kind.IsEmpty() || Landmarks.Contains(Cell)) return;
	Landmarks.Add(Cell);
	NamedCount++;
	const int32 Type = HouseTypeNamed(Kind);
	if (Type >= 0) Named.Add(Cell, Type);
	if (Kind.Equals(TEXT("spoil"), ESearchCase::CaseSensitive)) SpoilCells.Add(Cell);
}

void FTCWorldObjects::ApplySnapshot(const FJsonObject& M)
{
	using namespace TCWorldObjectsImpl;
	if (!HasMap()) return;
	const TArray<TSharedPtr<FJsonValue>>* Arr;
	if (M.TryGetArrayField(TEXT("cells"), Arr))
		for (const TSharedPtr<FJsonValue>& Entry : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			if (!Entry.IsValid() || !Entry->TryGetArray(Row)) continue;
			const int32 C = ApplyCellRow(*Row);
			if (C >= 0) Pending.Add(C);
		}
	// wrecks: [id, type, owner, x, z, rot, motion?]; a snapshot without them keeps the last list (client/world-state.js)
	if (M.TryGetArrayField(TEXT("wrecks"), Arr))
	{
		WreckRows.Reset(); CoverRows.Reset();
		for (const TSharedPtr<FJsonValue>& Entry : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			if (!Entry.IsValid() || !Entry->TryGetArray(Row) || Row->Num() < 6) continue;
			FTCWreckRow Wr;
			double Id, Owner;
			if (!Num((*Row)[0], Id) || !(*Row)[1].IsValid() || !(*Row)[1]->TryGetString(Wr.Type) || !Num((*Row)[2], Owner)
				|| !Num((*Row)[3], Wr.X) || !Num((*Row)[4], Wr.Z) || !Num((*Row)[5], Wr.Rot)) continue;
			Wr.Id = (int32)Id; Wr.Owner = (int32)Owner;
			const TSharedPtr<FJsonObject>* Motion;
			Wr.bMoving = Row->Num() > 6 && (*Row)[6].IsValid() && (*Row)[6]->TryGetObject(Motion);
			FTCCoverWreck Cw;
			Cw.Id = Wr.Id; Cw.X = Wr.X; Cw.Z = Wr.Z;
			CoverRows.Add(Cw);
			WreckRows.Add(MoveTemp(Wr));
		}
		bWrecksDirty = true;
	}
	// nodes: [x, z, rate, fuel]; props.js setNodes keeps scenery 4 m off each (only when the snapshot carries them)
	if (M.TryGetArrayField(TEXT("nodes"), Arr))
	{
		TArray<TPair<double, double>> Next;
		for (const TSharedPtr<FJsonValue>& Entry : *Arr)
		{
			const TArray<TSharedPtr<FJsonValue>>* Row;
			double X, Z;
			if (Entry.IsValid() && Entry->TryGetArray(Row) && Row->Num() >= 2 && Num((*Row)[0], X) && Num((*Row)[1], Z)) Next.Add(TPair<double, double>(X, Z));
		}
		for (const TPair<double, double>& P : Nodes) if (!Next.Contains(P)) NodesChanged.Add(P);
		for (const TPair<double, double>& P : Next) if (!Nodes.Contains(P)) NodesChanged.Add(P);
		if (NodesChanged.Num()) bNodesChanged = true;
		Nodes = MoveTemp(Next);
	}
}

void FTCWorldObjects::Clear()
{
	W = H = ChunksX = ChunksY = 0;
	bWorld = false;
	Grid.Reset(); GroundL.Reset(); ObjectL.Reset(); StartChars.Reset(); States.Reset(); Levels.Reset();
	Named.Reset(); Landmarks.Reset(); SpoilCells.Reset(); StoneBridgeSeeds.Reset(); StoneDeckMask.Reset();
	NamedCount = 0; LaidNamedCount = -1;
	WreckRows.Reset(); CoverRows.Reset();
	bWrecksDirty = true;
	Pending.Reset(); Shaped.Reset(); bWhole = true;
	Foot.clear(); Blocks.clear(); Rects.clear(); TintIndex.clear(); Tints.clear();
	Houses.Reset(); HouseType.Reset(); ChunkHouses.Reset(); ChunkPieces.Reset(); ChunkPicks.Reset(); ChunkPick.Reset();
	DirtyChunks.Reset(); DirtyPropChunks.Reset(); NearMask.Reset();
	Spawns.Reset(); Points.Reset(); Biome.Reset(); Discovered.Reset(); RailCells.Reset(); Nodes.Reset(); NodesChanged.Reset();
	bNodesChanged = false; bDiscoveryDue = false;
	Candidates.clear(); Placed.clear(); CandidateAt.Reset(); ChunkCandidates.Reset();
	bLastKept = false;
	Gen++;
}

void FTCWorldObjects::MarkChunksNear(int32 C, TSet<int32>& Into) const
{
	// client/structures.js rebuild: a builder reads its cell's neighbours up to two cells away
	const int32 X = C % W, Y = C / W;
	for (int32 Ky = FMath::Max(0, Y - 2) / ChunkCells; Ky <= FMath::Min(H - 1, Y + 2) / ChunkCells; Ky++)
		for (int32 Kx = FMath::Max(0, X - 2) / ChunkCells; Kx <= FMath::Min(W - 1, X + 2) / ChunkCells; Kx++) Into.Add(Ky * ChunksX + Kx);
}

void FTCWorldObjects::RecomputeStoneDecks(TSet<int32>& Changed)
{
	// client/structures.js stoneDecks: the whole span a 'stone bridge' entry sits on gets a stone deck
	if (!StoneBridgeSeeds.Num()) return;
	TSet<int32> Stone;
	TArray<int32> Q;
	for (const int32 C : StoneBridgeSeeds) if (Physical(C) == '=' && !Stone.Contains(C)) { Stone.Add(C); Q.Add(C); }
	for (int32 I = 0; I < Q.Num(); I++)
	{
		const int32 X = Q[I] % W, Y = Q[I] / W;
		const int32 D[4][2] = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } };
		for (const auto& Dd : D)
		{
			const int32 Nx = X + Dd[0], Ny = Y + Dd[1];
			if (Nx < 0 || Ny < 0 || Nx >= W || Ny >= H) continue;
			const int32 K = Ny * W + Nx;
			if (!Stone.Contains(K) && Physical(K) == '=') { Stone.Add(K); Q.Add(K); }
		}
	}
	for (int32 C = 0; C < W * H; C++)
	{
		const uint8 Now = Stone.Contains(C) ? 1 : 0;
		if (StoneDeckMask[C] != Now) { StoneDeckMask[C] = Now; Changed.Add(C); }
	}
}

int32 FTCWorldObjects::StoneCells() const
{
	int32 N = 0;
	for (const uint8 B : StoneDeckMask) N += B ? 1 : 0;
	return N;
}

void FTCWorldObjects::RebuildHouseTypes()
{
	for (int32& T : HouseType) T = -1;
	for (const TCWorldCore::FHouseBlock& B : Blocks)
		for (int32 Y = B.R.Y0; Y < B.R.Y1; Y++) for (int32 X = B.R.X0; X < B.R.X1; X++) HouseType[Y * W + X] = B.Type;
}

void FTCWorldObjects::RebuildChunkBounds(int32 Chunk)
{
	TCWorldCore::FBounds B;
	for (const TCWorldCore::FObjShape& S : ChunkPicks[Chunk]) B.Add(S);
	for (const int32 I : ChunkHouses[Chunk]) for (const TCWorldCore::FObjShape& S : Houses[I].Picks) B.Add(S);
	ChunkPick[Chunk] = B;
}

void FTCWorldObjects::BuildCandidates()
{
	// props.js candidates(map): lastStart.map (a World map's rows follow the discovered cells)
	TCScatterCore::FScatterMap Map;
	Map.W = W; Map.H = H;
	Map.Rows.resize((size_t)H);
	for (int32 Y = 0; Y < H; Y++)
	{
		std::string& Row = Map.Rows[(size_t)Y];
		Row.resize((size_t)W);
		for (int32 X = 0; X < W; X++) Row[(size_t)X] = (char)(bWorld ? Grid[Y * W + X] : StartChars[Y * W + X]);
	}
	Map.Spawns.assign(Spawns.GetData(), Spawns.GetData() + Spawns.Num());
	Map.Points.assign(Points.GetData(), Points.GetData() + Points.Num());
	if (Biome.Num()) Map.Biome.assign(Biome.GetData(), Biome.GetData() + Biome.Num());
	Map.bWorld = bWorld;
	if (bWorld) Map.Discovered.assign(Discovered.GetData(), Discovered.GetData() + Discovered.Num());
	Candidates = TCScatterCore::Candidates(Map);
	Placed.assign(Candidates.size(), TCScatterCore::FPlaced());
	for (int32& I : CandidateAt) I = -1;
	for (TArray<int32>& L : ChunkCandidates) L.Reset();
	for (int32 I = 0; I < (int32)Candidates.size(); I++)
	{
		const TCScatterCore::FCandidate& C = Candidates[(size_t)I];
		if (C.Cx < 0 || C.Cy < 0 || C.Cx >= W || C.Cy >= H) continue;
		CandidateAt[C.Cy * W + C.Cx] = I;
		ChunkCandidates[ChunkOf(C.Cx, C.Cy)].Add(I);
	}
	CandidatesBuiltAt = FPlatformTime::Seconds();
	for (int32 K = 0; K < ChunksX * ChunksY; K++) DirtyPropChunks.Add(K);
}

void FTCWorldObjects::PlaceProps(const FLive& Live, const TArray<int32>* Cells, int32 RadiusCells)
{
	std::vector<std::pair<double, double>> NodeList;
	for (const TPair<double, double>& P : Nodes) NodeList.push_back({ P.Key, P.Value });
	auto Redo = [&](int32 I)
	{
		const TCScatterCore::FPlaced Now = TCScatterCore::Place(Candidates[(size_t)I], Live, NodeList);
		TCScatterCore::FPlaced& Was = Placed[(size_t)I];
		const bool bSame = Now.bShown == Was.bShown && (!Now.bShown || (Now.Kind == Was.Kind && FMemory::Memcmp(Now.M, Was.M, sizeof(Now.M)) == 0 && FMemory::Memcmp(Now.Col, Was.Col, sizeof(Now.Col)) == 0));
		if (!bSame) { Was = Now; DirtyPropChunks.Add(ChunkOf(Candidates[(size_t)I].Cx, Candidates[(size_t)I].Cy)); }
	};
	if (!Cells) { for (int32 I = 0; I < (int32)Candidates.size(); I++) Redo(I); return; }
	// props.js refresh(box): the props within three cells of a change look again (a prop reads its neighbours and its
	// anchor, two cells away); the rest keep where they stood
	TSet<int32> Done;
	for (const int32 C : *Cells)
	{
		const int32 X = C % W, Y = C / W;
		for (int32 Ny = FMath::Max(0, Y - RadiusCells); Ny <= FMath::Min(H - 1, Y + RadiusCells); Ny++)
			for (int32 Nx = FMath::Max(0, X - RadiusCells); Nx <= FMath::Min(W - 1, X + RadiusCells); Nx++)
			{
				const int32 I = CandidateAt[Ny * W + Nx];
				if (I >= 0 && !Done.Contains(I)) { Done.Add(I); Redo(I); }
			}
	}
}

void FTCWorldObjects::Update(const ATCTerrain& Ground, const FTCUnitTable* Table)
{
	using namespace TCWorldObjectsImpl;
	if (!HasMap() || Ground.MapW() != W || Ground.MapH() != H) return;
	const double T0 = FPlatformTime::Seconds();
	const FSource Src(*this, Ground);
	const bool bStart = bWhole;
	// round 15: a change of Graphics level (gfx.onChange): structures.js rebuild() lays every piece again (whole, as
	// old.low !== C.low), props.js refresh() places every prop again; the candidates stay
	bool bLevelChanged = false;
	if (GfxWatch.TakeChanged() && FTCGraphics::Get().IsLow() != bLow)
	{
		bLow = FTCGraphics::Get().IsLow();
		bLevelChanged = true;
		LevelSwitchCount++;
		bWhole = true;
	}
	const bool bAny = bWhole || Pending.Num() > 0;
	if (bAny)
	{
		const int32 N = W * H;
		std::vector<int> Changed(Pending.GetData(), Pending.GetData() + Pending.Num());
		std::sort(Changed.begin(), Changed.end());
		Changed.erase(std::unique(Changed.begin(), Changed.end()), Changed.end());
		TSet<int32> Extra;
		RecomputeStoneDecks(Extra);
		auto NamedFn = [this](int C) { return NamedType(C); };

		// the footprint and its blocks: whole again on a start or when a landmark arrived, else only the blocks a
		// flipped cell touched (client/structures.js relaid, relay)
		const bool bRelaid = bWhole || NamedCount != LaidNamedCount;
		bool bRelay = false;
		if (bRelaid)
		{
			Foot.assign((size_t)N, 0);
			for (int32 C = 0; C < N; C++) if (IsFoot(C)) Foot[(size_t)C] = 1;
			Blocks = TCWorldCore::BlocksIn(Foot, W, H, NamedFn, nullptr);
		}
		else bRelay = TCWorldCore::RelayBlocks(Foot, W, H, Changed, [this](int C) { return IsFoot(C); }, NamedFn, Blocks);
		if (bRelaid || bRelay)
		{
			Rects = TCWorldCore::Layout(Blocks);
			RebuildHouseTypes();
			// the houses' tints for the rubble on their cells (C.tint); a cell whose tint changed redoes its part
			std::vector<int> NewIndex;
			std::vector<TCWorldCore::FHouseTint> NewTints;
			TCWorldCore::HouseTints(Rects, W, H, NewIndex, NewTints);
			if (!bWhole && TintIndex.size() == NewIndex.size())
				for (int32 C = 0; C < N; C++)
				{
					const int A = TintIndex[(size_t)C], B = NewIndex[(size_t)C];
					if ((A < 0) != (B < 0) || (A >= 0 && !(Tints[(size_t)A] == NewTints[(size_t)B]))) Extra.Add(C);
				}
			TintIndex = MoveTemp(NewIndex); Tints = MoveTemp(NewTints);
		}
		LaidNamedCount = NamedCount;
		bool bNearHouse = false;
		if (!bRelaid && !bRelay)
			for (const int C : Changed)
			{
				const int32 X = C % W, Y = C / W;
				for (int32 Dy = -2; Dy <= 2 && !bNearHouse; Dy++) for (int32 Dx = -2; Dx <= 2; Dx++)
				{
					const int32 Nx = X + Dx, Ny = Y + Dy;
					if (Nx >= 0 && Ny >= 0 && Nx < W && Ny < H && Foot[(size_t)(Ny * W + Nx)] == 1) { bNearHouse = true; break; }
				}
				if (bNearHouse) break;
			}

		TSet<int32> Touched; // chunks whose pieces change
		auto PicksOf = [](const std::vector<TCWorldCore::FPiece>& Pieces, TArray<TCWorldCore::FObjShape>& Out)
		{
			Out.Reset();
			for (const TCWorldCore::FPiece& P : Pieces) if (TCWorldCore::PiecePick(P.Kind)) Out.Add(TCWorldCore::ShapeOf(P));
		};
		if (bRelaid || bRelay || bNearHouse)
		{
			// each house is kept while no changed cell comes within two cells of it (client/structures.js piece)
			for (const int C : Changed)
			{
				const int32 X = C % W, Y = C / W;
				for (int32 Ny = FMath::Max(0, Y - 2); Ny <= FMath::Min(H - 1, Y + 2); Ny++) for (int32 Nx = FMath::Max(0, X - 2); Nx <= FMath::Min(W - 1, X + 2); Nx++) NearMask[Ny * W + Nx] = 1;
			}
			auto Near = [this](const TCWorldCore::FCellRect& R)
			{
				for (int32 Y = R.Y0; Y < R.Y1; Y++) for (int32 X = R.X0; X < R.X1; X++) if (NearMask[Y * W + X]) return true;
				return false;
			};
			TMap<uint64, int32> Old;
			if (!bWhole) for (int32 I = 0; I < Houses.Num(); I++) Old.Add(Houses[I].Key, I);
			TBitArray<> Reused(false, Houses.Num());
			TArray<FHouseEntry> Next;
			Next.Reserve((int32)Rects.size());
			for (const TCWorldCore::FHouseRect& R : Rects)
			{
				const uint64 Key = HouseKey(R);
				const int32* Was = Old.Find(Key);
				if (Was && !Near(R.R)) { Reused[*Was] = true; Next.Add(MoveTemp(Houses[*Was])); continue; }
				FHouseEntry E;
				E.Rect = R; E.Key = Key; E.Chunk = ChunkOf(R.R.X0, R.R.Y0);
				TCWorldCore::FPieceBuilder B(Src, E.Pieces);
				B.Foot = &Foot; B.TintIndex = &TintIndex; B.Tints = &Tints;
				B.House(R);
				PicksOf(E.Pieces, E.Picks);
				Touched.Add(E.Chunk);
				Next.Add(MoveTemp(E));
			}
			for (int32 I = 0; I < Houses.Num(); I++) if (!Reused[I]) Touched.Add(Houses[I].Chunk);
			Houses = MoveTemp(Next);
			for (TArray<int32>& L : ChunkHouses) L.Reset();
			for (int32 I = 0; I < Houses.Num(); I++) ChunkHouses[Houses[I].Chunk].Add(I);
			for (const int C : Changed)
			{
				const int32 X = C % W, Y = C / W;
				for (int32 Ny = FMath::Max(0, Y - 2); Ny <= FMath::Min(H - 1, Y + 2); Ny++) for (int32 Nx = FMath::Max(0, X - 2); Nx <= FMath::Min(W - 1, X + 2); Nx++) NearMask[Ny * W + Nx] = 0;
			}
		}

		// every other piece: per 32 x 32 part, redone when a change (a stone span or a house tint) comes within two cells
		TSet<int32> Redo;
		if (bWhole) { for (int32 K = 0; K < ChunksX * ChunksY; K++) Redo.Add(K); }
		else
		{
			for (const int C : Changed) MarkChunksNear(C, Redo);
			for (const int32 C : Extra) MarkChunksNear(C, Redo);
		}
		for (const int32 K : Redo)
		{
			std::vector<TCWorldCore::FPiece>& Out = ChunkPieces[K];
			Out.clear();
			const int32 X0 = (K % ChunksX) * ChunkCells, Y0 = (K / ChunksX) * ChunkCells;
			TCWorldCore::FPieceBuilder B(Src, Out);
			B.Foot = &Foot; B.TintIndex = &TintIndex; B.Tints = &Tints;
			B.Chunk(X0, Y0, FMath::Min(W, X0 + ChunkCells), FMath::Min(H, Y0 + ChunkCells));
			PicksOf(Out, ChunkPicks[K]);
			Touched.Add(K);
		}
		for (const int32 K : Touched) { RebuildChunkBounds(K); DirtyChunks.Add(K); }
		Pending.Reset();
		bWhole = false;
	}

	// the scenery (client/props.js): candidates once per start (a World map: again after discoveries, at most twice a
	// second), placements again around every reshaped cell and changed supply node
	const FLive Live(*this, Ground);
	const double Now = FPlatformTime::Seconds();
	if (bStart || (bWorld && bDiscoveryDue && Now - CandidatesBuiltAt >= 0.5))
	{
		BuildCandidates();
		PlaceProps(Live, nullptr, 0);
		bDiscoveryDue = false;
		Shaped.Reset(); bNodesChanged = false; NodesChanged.Reset();
	}
	else
	{
		if (bLevelChanged) { PlaceProps(Live, nullptr, 0); Shaped.Reset(); }
		if (Shaped.Num()) { PlaceProps(Live, &Shaped, 3); Shaped.Reset(); }
		if (bNodesChanged)
		{
			TArray<int32> NodeCells;
			for (const TPair<double, double>& P : NodesChanged)
			{
				const int32 X = FMath::FloorToInt32(P.Key / TCWorldCore::CellMetres), Y = FMath::FloorToInt32(P.Value / TCWorldCore::CellMetres);
				if (X >= 0 && Y >= 0 && X < W && Y < H) NodeCells.Add(Y * W + X);
			}
			PlaceProps(Live, &NodeCells, 3);
			NodesChanged.Reset(); bNodesChanged = false;
		}
	}
	UpdateMs = (FPlatformTime::Seconds() - T0) * 1000.0;
	if (bLevelChanged)
		UE_LOG(LogTC, Log, TEXT("world objects: graphics %s mid-match: %d pieces relaid, %d of %d props shown, in %.1f ms"),
			bLow ? TEXT("low") : TEXT("high"), ShapeCount(), PropsShown(), (int32)Candidates.size(), UpdateMs);
	if (bStart)
		UE_LOG(LogTC, Log, TEXT("world objects: %d houses on %d blocks, %d pieces (%d pickable), %d stone deck cells, %d named buildings; scenery %d candidates, %d shown; laid out in %.1f ms"),
			Houses.Num(), (int32)Blocks.size(), ShapeCount(), PickableCount(), StoneCells(), Named.Num(), (int32)Candidates.size(), PropsShown(), UpdateMs);
}

void FTCWorldObjects::TakeDirtyChunks(TSet<int32>& Into)
{
	Into.Append(DirtyChunks);
	DirtyChunks.Reset();
}

void FTCWorldObjects::TakeDirtyPropChunks(TSet<int32>& Into)
{
	Into.Append(DirtyPropChunks);
	DirtyPropChunks.Reset();
}

void FTCWorldObjects::ForEachPiece(int32 Chunk, TFunctionRef<void(const TCWorldCore::FPiece&)> Fn) const
{
	if (!ChunkPieces.IsValidIndex(Chunk)) return;
	for (const int32 I : ChunkHouses[Chunk]) for (const TCWorldCore::FPiece& P : Houses[I].Pieces) Fn(P);
	for (const TCWorldCore::FPiece& P : ChunkPieces[Chunk]) Fn(P);
}

void FTCWorldObjects::ForEachProp(int32 Chunk, TFunctionRef<void(const TCScatterCore::FCandidate&, const TCScatterCore::FPlaced&)> Fn) const
{
	if (!ChunkCandidates.IsValidIndex(Chunk)) return;
	for (const int32 I : ChunkCandidates[Chunk]) if (Placed[(size_t)I].bShown) Fn(Candidates[(size_t)I], Placed[(size_t)I]);
}

void FTCWorldObjects::ForEachShape(int32 Chunk, TFunctionRef<void(const TCWorldCore::FObjShape&)> Fn) const
{
	if (!ChunkPieces.IsValidIndex(Chunk)) return;
	auto Each = [&Fn](const std::vector<TCWorldCore::FPiece>& Pieces, const TArray<TCWorldCore::FObjShape>& Picks)
	{
		for (const TCWorldCore::FObjShape& S : Picks) Fn(S);
		for (const TCWorldCore::FPiece& P : Pieces)
		{
			if (TCWorldCore::PiecePick(P.Kind)) continue;
			TCWorldCore::FObjShape S;
			S.Kind = P.Kind; S.Cell = P.Cell;
			Fn(S);
		}
	};
	for (const int32 I : ChunkHouses[Chunk]) Each(Houses[I].Pieces, Houses[I].Picks);
	Each(ChunkPieces[Chunk], ChunkPicks[Chunk]);
}

int32 FTCWorldObjects::ShapeCount() const
{
	int32 N = 0;
	for (const std::vector<TCWorldCore::FPiece>& L : ChunkPieces) N += (int32)L.size();
	for (const FHouseEntry& E : Houses) N += (int32)E.Pieces.size();
	return N;
}

int32 FTCWorldObjects::PickableCount() const
{
	int32 N = 0;
	for (const TArray<TCWorldCore::FObjShape>& L : ChunkPicks) N += L.Num();
	for (const FHouseEntry& E : Houses) N += E.Picks.Num();
	return N;
}

int32 FTCWorldObjects::PropsShown() const
{
	int32 N = 0;
	for (const TCScatterCore::FPlaced& P : Placed) N += P.bShown ? 1 : 0;
	return N;
}

int32 FTCWorldObjects::GridMismatches(const FTCMatchState& State) const
{
	if (State.Map.Chars.Num() != Grid.Num()) return FMath::Max(Grid.Num(), State.Map.Chars.Num());
	int32 N = 0;
	for (int32 C = 0; C < Grid.Num(); C++) N += Grid[C] != State.Map.Chars[C] ? 1 : 0;
	return N;
}

TArrayView<const FTCCoverWreck> FTCWorldObjects::CoverWrecks() const
{
	return TArrayView<const FTCCoverWreck>(CoverRows);
}

void FTCWorldObjects::FillCoverSources(FTCCoverSources& Out) const
{
	Out.CellState = TArrayView<const uint8>(States);
	Out.Wrecks = CoverWrecks();
}

bool FTCWorldObjects::PickHouse(const FVector& Origin, const FVector& Dir, const ATCTerrain* Ground, FTCHousePick& Out) const
{
	if (!HasMap() || Dir.IsNearlyZero()) return false;
	const FVector D = Dir.GetSafeNormal();
	// Unreal cm (X, Y, Z up) -> reference metres (x, y up, z)
	const TCWorldCore::FP3 O = TCWorldCore::P3(Origin.X / 100.0, Origin.Z / 100.0, Origin.Y / 100.0);
	const TCWorldCore::FP3 Dr = TCWorldCore::P3(D.X, D.Z, D.Y);
	double Best = 1e300;
	auto Try = [&](const TCWorldCore::FObjShape& S)
	{
		double T;
		if (S.bPick && TCWorldCore::RayHit(S, O, Dr, T) && T < Best) Best = T;
	};
	for (int32 K = 0; K < ChunkPick.Num(); K++)
	{
		if (!ChunkPick[K].RayMeets(O, Dr, Best)) continue;
		for (const TCWorldCore::FObjShape& S : ChunkPicks[K]) Try(S);
		for (const int32 I : ChunkHouses[K]) for (const TCWorldCore::FObjShape& S : Houses[I].Picks) Try(S);
	}
	if (Best >= 1e299) return false;
	// the drawn ground in front of the piece hides it (houseAt intersects the pieces and the ground together)
	FVector2D G;
	if (Ground && Ground->RayToGround(Origin, Dir, G))
	{
		const double Gy = Ground->HeightAt((float)G.X, (float)G.Y);
		const double Tg = (G.X - O.X) * Dr.X + (Gy - O.Y) * Dr.Y + (G.Y - O.Z) * Dr.Z;
		if (Tg < Best) return false;
	}
	const double Px = O.X + Dr.X * (Best + 0.4), Pz = O.Z + Dr.Z * (Best + 0.4);
	const int32 X = FMath::FloorToInt32(Px / TCWorldCore::CellMetres), Y = FMath::FloorToInt32(Pz / TCWorldCore::CellMetres);
	if (X < 0 || Y < 0 || X >= W || Y >= H || !IsHouseCell(Y * W + X)) return false;
	Out.Cell = Y * W + X;
	Out.Center = FVector2D((X + 0.5) * TCWorldCore::CellMetres, (Y + 0.5) * TCWorldCore::CellMetres);
	Out.DistanceMetres = Best;
	return true;
}

bool FTCWorldObjects::HouseUnder(FVector2D Metres, FTCHousePick& Out) const
{
	const int32 X = FMath::FloorToInt32(Metres.X / TCWorldCore::CellMetres), Y = FMath::FloorToInt32(Metres.Y / TCWorldCore::CellMetres);
	if (X < 0 || Y < 0 || X >= W || Y >= H || !IsHouseCell(Y * W + X)) return false;
	Out.Cell = Y * W + X;
	Out.Center = FVector2D((X + 0.5) * TCWorldCore::CellMetres, (Y + 0.5) * TCWorldCore::CellMetres);
	Out.DistanceMetres = 0;
	return true;
}

TSharedPtr<FJsonObject> FTCWorldObjects::GarrisonCommand(const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, FVector2D HouseCenter, int32* OutCount)
{
	// client/orders.js: troops are the selected own units that are not structures; inf = troops.filter(def.garrisons);
	// add({ t: 'garrison', ids: inf.map(v => v.id), x: at.x, z: at.z })
	if (OutCount) *OutCount = 0;
	TArray<TSharedPtr<FJsonValue>> Ids;
	for (const int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		const FTCUnitDef* Def = U ? Table.Find(U->Type) : nullptr;
		FTCCoverUnitFacts Facts;
		if (!U || !Def || !State.IsOwn(*U) || Def->bStructure || !FTCCoverPreview::ReferenceUnitFacts(U->Type, Facts) || !Facts.bGarrisons) continue;
		Ids.Add(MakeShared<FJsonValueNumber>(Id));
	}
	if (!Ids.Num()) return nullptr;
	if (OutCount) *OutCount = Ids.Num();
	TSharedRef<FJsonObject> Msg = MakeShared<FJsonObject>();
	Msg->SetStringField(TEXT("t"), TEXT("garrison"));
	Msg->SetArrayField(TEXT("ids"), Ids);
	Msg->SetNumberField(TEXT("x"), HouseCenter.X);
	Msg->SetNumberField(TEXT("z"), HouseCenter.Y);
	return Msg;
}

const TCWorldCore::FHouseType* FTCWorldObjects::GarrisonHouseType(int32 Flags)
{
	if (!(Flags & 32)) return nullptr;
	const int32 Type = (Flags >> 20) & 7;
	return Type < TCWorldCore::HouseTypeCount ? &TCWorldCore::HouseTypes[Type] : nullptr;
}

// ---------- the view ----------

namespace TCWorldObjectsImpl
{
	constexpr int32 RegionChunks = 2; // a component spans 2 x 2 parts: 64 x 64 cells
	enum : int32 { LayerPieces = 0, LayerProps = 1, LayerPropsDetail = 2 };
	uint64 CompKey(int32 Region, int32 Layer, int32 Kind) { return (uint64)(uint32)Region << 32 | (uint64)Layer << 16 | (uint64)(uint32)Kind; }
	int32 RegionOfKey(uint64 Key) { return (int32)(Key >> 32); }
	// The world material made from a catalog material (make-world-materials.py): MI_TC<slot> -> MI_TCWorld_<slot>.
	FSoftObjectPath WorldMaterialFor(const FSoftObjectPath& Source)
	{
		const FString Name = Source.GetAssetName();
		if (!Name.StartsWith(TEXT("MI_TC"))) return FSoftObjectPath();
		const FString World = TEXT("MI_TCWorld_") + Name.Mid(5);
		return FSoftObjectPath(FString::Printf(TEXT("/Game/World/Materials/Instances/%s.%s"), *World, *World));
	}
	// Small trim the far level of detail drops (windows, painted boards, wire, duckboards, sandbags, rubble chunks).
	bool IsTrim(TCWorldCore::EPiece K)
	{
		using TCWorldCore::EPiece;
		return K == EPiece::Window || K == EPiece::Pane || K == EPiece::Paint || K == EPiece::Wire || K == EPiece::Duck || K == EPiece::Bag || K == EPiece::Chunk;
	}
	bool IsManPart(const FString& Name) { return Name.StartsWith(TEXT("man_")); }
	struct FBatch
	{
		TArray<FTransform> Transforms;
		TArray<float> Data;
	};
}

bool FTCWorldObjectsView::Attach(UWorld* World)
{
	if (Actor.IsValid()) return true;
	if (!World) return false;
	FActorSpawnParameters Params;
	Params.ObjectFlags |= RF_Transient;
	AActor* A = World->SpawnActor<AActor>(AActor::StaticClass(), FTransform::Identity, Params);
	if (!A) return false;
	USceneComponent* Root = NewObject<USceneComponent>(A, TEXT("WorldObjects"));
	Root->SetMobility(EComponentMobility::Movable);
	A->SetRootComponent(Root);
	A->AddInstanceComponent(Root);
	Root->RegisterComponent();
	Actor = A;
	if (!Library.IsLoaded() && !Library.Load(LibraryError))
		UE_LOG(LogTC, Error, TEXT("world objects: the model catalog does not load (%s); map pieces, props, wrecks and ghosts are not drawn"), *LibraryError);
	Queue.Reset(); Comps.Reset(); ModelComps.Reset(); ModelShadowTwins.Reset(); DrawnGen = 0xffffffffu; bFogAll = true; bModelsDirty = true;
	return true;
}

void FTCWorldObjectsView::Detach()
{
	BuildingBreaches.Reset();
	if (AActor* A = Actor.Get()) A->Destroy();
	Actor.Reset();
	if (UTexture2D* Fog = FogTexture.Get()) Fog->RemoveFromRoot();
	Comps.Reset(); ModelComps.Reset(); ModelShadowTwins.Reset(); Mids.Reset(); Queue.Reset();
	PieceAssets.Reset(); PropAssets.Reset(); bAssetsLoaded = false; KeepLoaded.Reset();
	FogTexture.Reset(); FogBytes.Reset(); FogW = FogH = 0;
	DrawnGen = 0xffffffffu;
}

void FTCWorldObjectsView::ClearAll()
{
	for (TPair<uint64, FComp>& P : Comps)
	{
		if (UHierarchicalInstancedStaticMeshComponent* C = P.Value.Comp.Get()) C->ClearInstances();
		P.Value.Count = 0;
	}
	for (TPair<FString, TWeakObjectPtr<UInstancedStaticMeshComponent>>& P : ModelComps) if (UInstancedStaticMeshComponent* C = P.Value.Get()) C->ClearInstances();
	for (const auto& P : ModelShadowTwins) if (UInstancedStaticMeshComponent* C = P.Value.Get()) C->ClearInstances();
	GhostsDrawn = WrecksDrawn = ModelInstances = 0;
}

void FTCWorldObjectsView::Reset()
{
	BuildingBreaches.Reset();
	ClearAll();
	Queue.Reset(); FogPending.Reset();
	DrawnGen = 0xffffffffu; bFogAll = true; bModelsDirty = true; GhostHash = 0;
}

bool FTCWorldObjectsView::MaterialsFor(const FTCModelPart& Part, FKindAsset& Out)
{
	using namespace TCWorldObjectsImpl;
	Out.Materials.Reset();
	bool bAllWorld = true;
	for (const FSoftObjectPath& Source : Part.Materials)
	{
		const FSoftObjectPath WorldPath = WorldMaterialFor(Source);
		UMaterialInterface* Made = nullptr;
		if (const TWeakObjectPtr<UMaterialInstanceDynamic>* Found = Mids.Find(WorldPath)) Made = Found->Get();
		if (!Made && WorldPath.IsValid())
			if (UMaterialInterface* Base = Cast<UMaterialInterface>(WorldPath.TryLoad()))
			{
				UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(Base, Actor.Get());
				if (UTexture2D* Fog = FogTexture.Get())
				{
					Mid->SetTextureParameterValue(TEXT("FogMap"), Fog);
					Mid->SetVectorParameterValue(TEXT("FogSize"), FLinearColor(FogW * TCWorldCore::CellMetres, FogH * TCWorldCore::CellMetres, 0, 1));
					Mid->SetScalarParameterValue(TEXT("FogOn"), 1.f);
				}
				Mids.Add(WorldPath, Mid);
				Made = Mid;
			}
		if (!Made)
		{
			// without the world materials the catalog's own draw the piece, untinted and without fog
			bAllWorld = false;
			MissingWorldMaterials++;
			Made = Cast<UMaterialInterface>(Source.TryLoad());
		}
		Out.Materials.Add(Made);
	}
	Out.bWorldMaterials = bAllWorld;
	return bAllWorld;
}

void FTCWorldObjectsView::LoadAssets()
{
	using namespace TCWorldObjectsImpl;
	if (bAssetsLoaded || !Library.IsLoaded()) return;
	bAssetsLoaded = true;
	auto Load = [this](const FString& Key, FKindAsset& Out)
	{
		const FTCModelEntry* E = Library.Find(Key);
		if (!E || !E->Parts.Num()) { UE_LOG(LogTC, Warning, TEXT("world objects: the catalog has no %s"), *Key); return; }
		const FTCModelPart& Part = E->Parts[0];
		Out.Mesh = Cast<UStaticMesh>(Part.Mesh.TryLoad());
		if (!Out.Mesh) { UE_LOG(LogTC, Warning, TEXT("world objects: mesh %s does not load"), *Part.Mesh.ToString()); return; }
		MaterialsFor(Part, Out);
		KeepLoaded.Emplace(Out.Mesh);
		for (UMaterialInterface* M : Out.Materials) if (M) KeepLoaded.Emplace(M);
		if (const FStaticMeshRenderData* R = Out.Mesh->GetRenderData()) if (R->LODResources.Num()) Out.Triangles = R->LODResources[0].GetNumTriangles();
		Out.bLoaded = true;
	};
	// round 14 lead glue: Graphics Low (Client/TCGraphics.h) as structures.js and props.js read gfx.low when they build:
	// no sandbag, chunk or rail shadows, no tree or bush shadows. Round 15: read here for the first components and again
	// on every change of level (ApplyShadowLevel, from Draw)
	GfxWatch.TakeChanged();
	const bool bLowShadows = !FTCGraphics::Get().Budget().bPropShadows;
	bShadowsLow = bLowShadows;
	PieceAssets.SetNum(TCWorldCore::PieceCount);
	for (int32 K = 0; K < TCWorldCore::PieceCount; K++)
	{
		const TCWorldCore::EPiece Kind = (TCWorldCore::EPiece)K;
		Load(FString::Printf(TEXT("piece/%s|-1"), ANSI_TO_TCHAR(TCWorldCore::PieceName(Kind))), PieceAssets[K]);
		PieceAssets[K].bShadow = bShadows && PieceCastsShadow(Kind, bLowShadows);
		PieceAssets[K].bTrim = IsTrim(Kind);
	}
	PropAssets.SetNum(TCScatterCore::PropCount);
	for (int32 K = 0; K < TCScatterCore::PropCount; K++)
	{
		const TCScatterCore::EProp Kind = (TCScatterCore::EProp)K;
		Load(FTCScatter::CatalogKey(Kind), PropAssets[K]);
		// props.js refresh: grass and crops are too low to shade anything; Low drops tree and bush shadows too
		PropAssets[K].bShadow = bShadows && PropCastsShadow(Kind, bLowShadows);
	}
	int32 Loaded = 0, World = 0;
	for (const FKindAsset& A : PieceAssets) { Loaded += A.bLoaded ? 1 : 0; World += A.bWorldMaterials ? 1 : 0; }
	for (const FKindAsset& A : PropAssets) { Loaded += A.bLoaded ? 1 : 0; World += A.bWorldMaterials ? 1 : 0; }
	UE_LOG(LogTC, Log, TEXT("world objects: %d of %d piece and prop models loaded, %d with the world materials (%d material slots fell back to the catalog's: untinted, no fog)"),
		Loaded, PieceAssets.Num() + PropAssets.Num(), World, MissingWorldMaterials);
	if (MissingWorldMaterials) UE_LOG(LogTC, Error, TEXT("world objects: /Game/World materials missing; run unreal/Tools/world/make-world-materials.py"));
}

void FTCWorldObjectsView::ApplyShadowLevel(bool bLow)
{
	using namespace TCWorldObjectsImpl;
	// light.js applyGfx and the builders' flush: the shadow casters of each kind at the new level, on the components
	// already drawn (props.js refresh sets castShadow; structures.js rebuild flushes its meshes again)
	bShadowsLow = bLow;
	ShadowTurned = 0;
	for (int32 K = 0; K < PieceAssets.Num(); K++) PieceAssets[K].bShadow = bShadows && PieceCastsShadow((TCWorldCore::EPiece)K, bLow);
	for (int32 K = 0; K < PropAssets.Num(); K++) PropAssets[K].bShadow = bShadows && PropCastsShadow((TCScatterCore::EProp)K, bLow);
	for (TPair<uint64, FComp>& P : Comps)
	{
		UHierarchicalInstancedStaticMeshComponent* C = P.Value.Comp.Get();
		if (!C) continue;
		const int32 Layer = (int32)((P.Key >> 16) & 0xffff), Kind = (int32)(P.Key & 0xffff);
		const TArray<FKindAsset>& Assets = Layer == LayerPieces ? PieceAssets : PropAssets;
		if (!Assets.IsValidIndex(Kind)) continue;
		const bool bWant = Assets[Kind].bShadow;
		if (C->CastShadow != bWant) { C->SetCastShadow(bWant); ShadowTurned++; }
	}
	UE_LOG(LogTC, Log, TEXT("world view: graphics %s: shadows turned on %d components (tree and bush shadows %s)"), bLow ? TEXT("low") : TEXT("high"), ShadowTurned, bLow ? TEXT("off") : TEXT("on"));
}

const FTCWorldObjectsView::FKindAsset& FTCWorldObjectsView::PieceAsset(TCWorldCore::EPiece Kind) { return PieceAssets[(int32)Kind]; }
const FTCWorldObjectsView::FKindAsset& FTCWorldObjectsView::PropAsset(TCScatterCore::EProp Kind) { return PropAssets[(int32)Kind]; }

UHierarchicalInstancedStaticMeshComponent* FTCWorldObjectsView::Component(uint64 Key, const FKindAsset& Asset, float CullMetres)
{
	using namespace TCWorldObjectsImpl;
	if (FComp* Found = Comps.Find(Key)) if (UHierarchicalInstancedStaticMeshComponent* C = Found->Comp.Get()) return C;
	AActor* A = Actor.Get();
	if (!A || !Asset.bLoaded) return nullptr;
	UHierarchicalInstancedStaticMeshComponent* C = NewObject<UHierarchicalInstancedStaticMeshComponent>(A);
	C->SetMobility(EComponentMobility::Movable);
	C->SetStaticMesh(Asset.Mesh);
	for (int32 I = 0; I < Asset.Materials.Num(); I++) if (Asset.Materials[I]) C->SetMaterial(I, Asset.Materials[I]);
	C->SetNumCustomDataFloats(WorldFloats);
	C->SetCollisionEnabled(ECollisionEnabled::NoCollision); // the house pick is analytic (FTCWorldObjects::PickHouse)
	C->SetCanEverAffectNavigation(false);
	C->SetCastShadow(Asset.bShadow);
	if (CullMetres > 0) C->SetCullDistances(FMath::RoundToInt32(CullMetres * 85.f), FMath::RoundToInt32(CullMetres * 100.f));
	C->SetupAttachment(A->GetRootComponent());
	C->RegisterComponent();
	A->AddInstanceComponent(C);
	FComp& Entry = Comps.FindOrAdd(Key);
	Entry.Comp = C;
	Entry.Triangles = Asset.Triangles;
	return C;
}

int32 FTCWorldObjectsView::RegionOfChunk(const FTCWorldObjects& Model, int32 Chunk) const
{
	using namespace TCWorldObjectsImpl;
	const int32 Cw = FMath::Max(1, Model.ChunksWide());
	return (Chunk / Cw / RegionChunks) * RegionsX + (Chunk % Cw) / RegionChunks;
}

void FTCWorldObjectsView::BuildRegion(const FTCWorldObjects& Model, int32 Region)
{
	using namespace TCWorldObjectsImpl;
	using namespace TCScatterCore;
	if (PieceAssets.Num() != TCWorldCore::PieceCount || PropAssets.Num() != PropCount) return; // no catalog
	TMap<uint64, FBatch> Batches;
	const int32 Rx = Region % FMath::Max(1, RegionsX), Ry = Region / FMath::Max(1, RegionsX);
	for (int32 Ky = Ry * RegionChunks; Ky < FMath::Min(Model.ChunksHigh(), (Ry + 1) * RegionChunks); Ky++)
		for (int32 Kx = Rx * RegionChunks; Kx < FMath::Min(Model.ChunksWide(), (Rx + 1) * RegionChunks); Kx++)
		{
			const int32 Chunk = Ky * Model.ChunksWide() + Kx;
			Model.ForEachPiece(Chunk, [&](const TCWorldCore::FPiece& P)
			{
				FBatch& B = Batches.FindOrAdd(CompKey(Region, LayerPieces, (int32)P.Kind));
				double Q[4];
				TCWorldCore::PieceQuat(P, Q);
				B.Transforms.Add(ToUnreal(P.X, P.Y, P.Z, P.Sx, P.Sy, P.Sz, Q));
				const float U = FMath::Abs(P.Sz) / RoofTile, V = FMath::Sqrt(FMath::Square(0.5f * P.Sx) + FMath::Square(0.4f * P.Sy)) / RoofTile;
				B.Data.Append({ P.R, P.G, P.B, 1.f, U, V });
			});
			Model.ForEachProp(Chunk, [&](const FCandidate& C, const FPlaced& P)
			{
				// the far level of detail is the reference's Low set: no grass, odd seeds thinned (crops stay whole)
				const bool bDetail = P.Kind == EProp::Grass || ((C.Seed & 1) && C.Kind != EProp::Crop);
				FBatch& B = Batches.FindOrAdd(CompKey(Region, bDetail ? LayerPropsDetail : LayerProps, (int32)P.Kind));
				const double Q[4] = { 0, FMath::Sin(P.Yaw / 2), 0, FMath::Cos(P.Yaw / 2) };
				B.Transforms.Add(ToUnreal(P.X, P.Y, P.Z, P.Sx, P.Sy, P.Sz, Q));
				B.Data.Append({ (float)P.Col[0], (float)P.Col[1], (float)P.Col[2], (float)P.Bark, 1.f, 1.f });
			});
		}
	for (TPair<uint64, FComp>& P : Comps)
	{
		if (RegionOfKey(P.Key) != Region || Batches.Contains(P.Key)) continue;
		if (UHierarchicalInstancedStaticMeshComponent* C = P.Value.Comp.Get()) if (P.Value.Count) C->ClearInstances();
		P.Value.Count = 0;
	}
	for (TPair<uint64, FBatch>& P : Batches)
	{
		const int32 Layer = (int32)((P.Key >> 16) & 0xffff), Kind = (int32)(P.Key & 0xffff);
		const FKindAsset& Asset = Layer == LayerPieces ? PieceAsset((TCWorldCore::EPiece)Kind) : PropAsset((EProp)Kind);
		const float Cull = Layer == LayerPropsDetail ? ScenicDetailMetres : Layer == LayerPieces && Asset.bTrim ? TrimDetailMetres : 0.f;
		UHierarchicalInstancedStaticMeshComponent* C = Component(P.Key, Asset, Cull);
		if (!C) continue;
		C->ClearInstances();
		C->AddInstances(P.Value.Transforms, false, false, false);
		for (int32 I = 0; I < P.Value.Transforms.Num(); I++) C->SetCustomData(I, TArrayView<const float>(P.Value.Data.GetData() + I * WorldFloats, WorldFloats), false);
		// the cluster tree now, not on a worker: while an asynchronous build runs the component draws none of the
		// region's instances, so a region refilled in a fight (a crater changing a cell) blinked out for a frame
		// (slice 11 lead, seen live: trees and a hedge missing from one frame of a burst)
		C->BuildTreeIfOutdated(false, true);
		C->MarkRenderStateDirty();
		Comps.FindChecked(P.Key).Count = P.Value.Transforms.Num();
	}
}

void FTCWorldObjectsView::EnsureFog(const FTCWorldObjects& Model)
{
	const int32 W = Model.MapW(), H = Model.MapH();
	const FTCFogFrontier* Shared = TCFogLook::Active();
	UTexture2D* Alpha = Shared && Shared->HasMap() && Shared->MapW() == W && Shared->MapH() == H ? Shared->AlphaTexture() : nullptr;
	if (Alpha)
	{
		// Bind fog.js's one live texture. Copying target multiplier bytes here loses fades and inverts clear fog.
		if (FogTexture.Get() != Alpha)
		{
			if (UTexture2D* Old = FogTexture.Get()) Old->RemoveFromRoot();
			Alpha->AddToRoot(); FogTexture = Alpha; FogW = W; FogH = H; FogBytes.Reset();
			for (TPair<FSoftObjectPath, TWeakObjectPtr<UMaterialInstanceDynamic>>& P : Mids)
				if (UMaterialInstanceDynamic* Mid = P.Value.Get())
				{
					Mid->SetTextureParameterValue(TEXT("FogMap"), Alpha);
					Mid->SetVectorParameterValue(TEXT("FogSize"), FLinearColor(W * TCWorldCore::CellMetres, H * TCWorldCore::CellMetres, 0, 1));
					Mid->SetScalarParameterValue(TEXT("FogOn"), 1.f);
				}
		}
		return;
	}
	if (FogTexture.IsValid() && FogW == W && FogH == H && FogBytes.Num() == W * H) return;
	UTexture2D* T = UTexture2D::CreateTransient(W, H, PF_G8);
	if (!T) return;
	if (UTexture2D* Old = FogTexture.Get()) Old->RemoveFromRoot();
	T->AddToRoot(); // held by the dynamic materials too, but drawn into before they exist
	T->SRGB = false;
	T->Filter = TF_Bilinear; // fog.js LinearFilter
	T->AddressX = TA_Clamp;
	T->AddressY = TA_Clamp;
	T->NeverStream = true;
	T->UpdateResource();
	FogTexture = T;
	FogW = W; FogH = H;
	FogBytes.Init(0, W * H);
	bFogAll = true;
	for (TPair<FSoftObjectPath, TWeakObjectPtr<UMaterialInstanceDynamic>>& P : Mids)
		if (UMaterialInstanceDynamic* Mid = P.Value.Get())
		{
			Mid->SetTextureParameterValue(TEXT("FogMap"), T);
			Mid->SetVectorParameterValue(TEXT("FogSize"), FLinearColor(W * TCWorldCore::CellMetres, H * TCWorldCore::CellMetres, 0, 1));
			Mid->SetScalarParameterValue(TEXT("FogOn"), 1.f);
		}
}

void FTCWorldObjectsView::WriteFog(const FTCWorldObjects& Model, const FTCMatchState& State, const ATCTerrain* Ground, const TSet<int32>* Cells)
{
	UTexture2D* T = FogTexture.Get();
	if (!T || FogBytes.Num() != FogW * FogH) return;
	// ATCTerrain::CellColor's dimming: in sight 1, explored 0.55, never seen 0.32; none once an authored match is decided
	const bool bReveal = Ground && Ground->IsEndRevealed();
	// round 14: the shared look (fog.js: World's frontier darkening and the lit cells under units included)
	auto Byte = [&](int32 C) -> uint8 { return TCFogLook::AlphaByte(State, bReveal, C); };
	bool bChanged = false;
	if (Cells) { for (const int32 C : *Cells) if (FogBytes.IsValidIndex(C)) { const uint8 B = Byte(C); bChanged |= FogBytes[C] != B; FogBytes[C] = B; } }
	else { for (int32 C = 0; C < FogBytes.Num(); C++) FogBytes[C] = Byte(C); bChanged = true; }
	if (!bChanged) return;
	uint8* Copy = new uint8[FogBytes.Num()];
	FMemory::Memcpy(Copy, FogBytes.GetData(), FogBytes.Num());
	FUpdateTextureRegion2D* Region = new FUpdateTextureRegion2D(0, 0, 0, 0, FogW, FogH);
	T->UpdateTextureRegions(0, 1, Region, FogW, 1, Copy, [](uint8* Data, const FUpdateTextureRegion2D* Regions) { delete[] Data; delete Regions; });
}

void FTCWorldObjectsView::MarkFog(const FTCWorldObjects& Model, const TSet<int32>& FogCells)
{
	FogPending.Append(FogCells);
}

void FTCWorldObjectsView::DrawModels(const FTCWorldObjects& Model, const FTCMatchState& State, const ATCTerrain* Ground)
{
	using namespace TCWorldObjectsImpl;
	AActor* A = Actor.Get();
	if (!A || !Library.IsLoaded()) return;
	TMap<UInstancedStaticMeshComponent*, FBatch> Batches;
	GhostsDrawn = WrecksDrawn = ModelInstances = ModelsMissing = 0;
	auto Comp = [&](const FTCModelPart& Part, bool bFaded, bool bWreck) -> UInstancedStaticMeshComponent*
	{
		FString Key = Part.Mesh.ToString() + (bFaded ? TEXT("|faded") : bWreck ? TEXT("|wreck") : TEXT("|solid"));
		if (const TWeakObjectPtr<UInstancedStaticMeshComponent>* Found = ModelComps.Find(Key)) return Found->Get();
		UInstancedStaticMeshComponent* C = NewObject<UInstancedStaticMeshComponent>(A);
		if (!FTCModelLibrary::SetupComponent(C, Part, bFaded)) { ModelComps.Add(Key, nullptr); return nullptr; }
		if (bWreck)
		{
			// main.js: replace every non-MeshBasicMaterial with the same opaque Lambert wreckMaterial.
			// Retaining the original blend mode leaves glass/paint transparent and changes the hulk's colour.
			UMaterialInterface* Base = LoadObject<UMaterialInterface>(nullptr, TCLook::LambertPath);
			UMaterialInstanceDynamic* Wreck = Base ? UMaterialInstanceDynamic::Create(Base, C) : nullptr;
			if (Wreck) Wreck->SetVectorParameterValue(TEXT("Color"), TCLook::HexLinear(0x1d1b18));
			for (int32 I = 0; I < C->GetNumMaterials(); I++)
			{
				float Unlit = 0.f;
				UMaterialInterface* Source = C->GetMaterial(I);
				if (Source) Source->GetScalarParameterValue(FMaterialParameterInfo(TEXT("Unlit")), Unlit);
				if (Unlit <= 0.5f && Wreck) C->SetMaterial(I, Wreck); // opaque, flat colour, no texture/paint/rim or metal reflection
			}
			if (TCLook::HasUnitSun(A->GetWorld()))
			{
				// makeUnit's meshes cast but never receive the sun's shadow, including when they become hulks.
				C->SetLightingChannels(false, true, false); C->SetCastShadow(false);
				auto* Twin = NewObject<UInstancedStaticMeshComponent>(A);
				Twin->SetStaticMesh(C->GetStaticMesh());
				for (int32 I = 0; I < C->GetNumMaterials(); I++) Twin->SetMaterial(I, C->GetMaterial(I));
				Twin->SetNumCustomDataFloats(C->NumCustomDataFloats);
				Twin->SetMobility(EComponentMobility::Movable); Twin->SetCollisionEnabled(ECollisionEnabled::NoCollision);
				Twin->SetCastShadow(true); Twin->bCastHiddenShadow = true; Twin->SetHiddenInGame(true); Twin->SetVisibility(false);
				Twin->SetupAttachment(A->GetRootComponent()); Twin->RegisterComponent(); A->AddInstanceComponent(Twin);
				ModelShadowTwins.Add(C, Twin);
			}
		}
		C->SetCanEverAffectNavigation(false);
		C->SetupAttachment(A->GetRootComponent());
		C->RegisterComponent();
		A->AddInstanceComponent(C);
		ModelComps.Add(Key, C);
		return C;
	};
	// one model as main.js makeUnit builds it at rest: no squad men, the near level of detail, its rings left out
	auto AddModel = [&](const FString& Type, int32 Owner, const FTransform& Root, bool bFaded, float Fade, float Damage, int32 BuildingId = -1) -> bool
	{
		const int32 Faction = State.Factions.IsValidIndex(Owner) ? State.Factions[Owner] : Owner < 0 ? 0 : Owner % 3;
		const FTCModelEntry* Entry = Library.Resolve({ Type, Faction, FString() });
		if (!Entry) { ModelsMissing++; return false; }
		// The wreck has an actual Lambert replacement. Basic materials retain their original colour/emission.
		const TArray<float> Custom = Library.CustomData(Owner, Faction, Fade, 0.f);
		TArray<bool> Visible;
		Visible.SetNum(Entry->Parts.Num());
		for (int32 I = 0; I < Entry->Parts.Num(); I++)
		{
			const FTCModelPart& Part = Entry->Parts[I];
			const FString Name = Part.Name.ToString();
			bool bShow = !IsManPart(Name) && (Name.EndsWith(TEXT("_near")) || (!Name.EndsWith(TEXT("_far")) && !Part.bHidden));
			const int32 Parent = Part.Parent.IsNone() ? INDEX_NONE : Entry->FindPart(Part.Parent);
			if (Parent != INDEX_NONE && !Visible[Parent]) bShow = false;
			Visible[I] = bShow;
			if (!bShow) continue;
			const FTransform PartTransform = FTCModelLibrary::PartTransform(*Entry, I, Root);
			FTCModelPart DrawPart = Part;
			if (BuildingId >= 0 && !BuildingBreaches.PartFor(Part, BuildingId, State, PartTransform, A, DrawPart)) continue;
			UInstancedStaticMeshComponent* C = Comp(DrawPart, bFaded, Damage > 0.f);
			if (!C) continue;
			FBatch& B = Batches.FindOrAdd(C);
			B.Transforms.Add(PartTransform);
			B.Data.Append(Custom);
			ModelInstances++;
		}
		return true;
	};
	// client/main.js hulks: the wreck's own model in the wreck material, turned by -rot, on the ground
	TActorIterator<ATCEffects> EffectsIt(A->GetWorld());
	const ATCEffects* Effects = EffectsIt ? *EffectsIt : nullptr;
	for (const FTCWreckRow& Wr : Model.Wrecks())
	{
		const float G = Ground ? Ground->HeightAt((float)Wr.X, (float)Wr.Z) : 0.f;
		FTransform Pose(FQuat(FVector::UpVector, Wr.Rot), ATCTerrain::ToWorld((float)Wr.X, (float)Wr.Z, G));
		// main.js debris.pose: Euler (0, -hullDir, tilt), XYZ. Reflecting Three Y/Z gives yaw then pitch -tilt.
		if (Wr.bMoving && Effects)
			if (const FTCDebrisRow* R = Effects->Fx.WreckPose(Wr.Id))
				Pose = FTransform(FQuat(FVector::UpVector, R->HullDir) * FQuat(FVector::RightVector, -R->Tilt),
					ATCTerrain::ToWorld(R->X, R->Z, R->Y));
		if (AddModel(Wr.Type, Wr.Owner, Pose, false, 1.f, 1.f)) WrecksDrawn++;
	}
	// client/main.js applyGhosts: enemy buildings remembered under fog at 0.35 opacity, risen to their built share
	for (const FTCGhost& Gh : State.Ghosts)
	{
		const float G = Ground ? Ground->HeightAt((float)Gh.X, (float)Gh.Z) : 0.f;
		FTransform Root(FQuat::Identity, ATCTerrain::ToWorld((float)Gh.X, (float)Gh.Z, G));
		Root.SetScale3D(FVector(1, 1, 0.15f + 0.85f * FMath::Clamp((float)Gh.Built, 0.f, 1.f)));
		if (AddModel(Gh.Type, Gh.Owner, Root, true, Library.GhostFade, 0.f, Gh.Id)) GhostsDrawn++;
	}
	for (TPair<FString, TWeakObjectPtr<UInstancedStaticMeshComponent>>& P : ModelComps)
	{
		UInstancedStaticMeshComponent* C = P.Value.Get();
		if (!C) continue;
		C->ClearInstances();
		UInstancedStaticMeshComponent* Twin = ModelShadowTwins.FindRef(C).Get();
		if (Twin) Twin->ClearInstances();
		if (FBatch* B = Batches.Find(C))
		{
			C->AddInstances(B->Transforms, false, false, false);
			if (Twin)
			{
				Twin->AddInstances(B->Transforms, false, false, false);
				if (!B->Transforms.IsEmpty()) Twin->SetCustomData(0, B->Transforms.Num() - 1, B->Data, false);
				Twin->MarkRenderStateDirty();
			}
			const int32 Floats = TCModelData::Count;
			for (int32 I = 0; I < B->Transforms.Num(); I++) C->SetCustomData(I, TArrayView<const float>(B->Data.GetData() + I * Floats, Floats), false);
		}
		C->MarkRenderStateDirty();
	}
}

void FTCWorldObjectsView::Draw(FTCWorldObjects& Model, const FTCMatchState& State, const ATCTerrain* Ground)
{
	using namespace TCWorldObjectsImpl;
	if (!Actor.IsValid() || !Model.HasMap()) return;
	LoadAssets();
	// round 15: a change of Graphics level turns the shadows of what is drawn now (the model relays and replaces at its
	// own Update; the chunks it marks rebuild below)
	if (GfxWatch.TakeChanged() && bAssetsLoaded && FTCGraphics::Get().IsLow() != bShadowsLow) ApplyShadowLevel(FTCGraphics::Get().IsLow());
	const int32 Chunks = Model.ChunkCount();
	if (Model.Generation() != DrawnGen)
	{
		// a start: a reconnect to the same match keeps its instances up while they are rebuilt, anything else clears
		const int32 NewRx = FMath::DivideAndRoundUp(FMath::Max(1, Model.ChunksWide()), RegionChunks), NewRy = FMath::DivideAndRoundUp(FMath::Max(1, Model.ChunksHigh()), RegionChunks);
		if (!(Model.LastStartKept() && NewRx == RegionsX && NewRy == RegionsY)) ClearAll();
		RegionsX = NewRx; RegionsY = NewRy;
		Queue.Reset();
		for (int32 R = 0; R < RegionsX * RegionsY; R++) Queue.Add(R);
		DrawnGen = Model.Generation();
		bFogAll = true; bModelsDirty = true;
		bLastReveal = Ground && Ground->IsEndRevealed();
	}
	TSet<int32> Dirty;
	Model.TakeDirtyChunks(Dirty);
	Model.TakeDirtyPropChunks(Dirty);
	for (const int32 K : Dirty) if (K >= 0 && K < Chunks) Queue.Add(RegionOfChunk(Model, K));
	// fog: the whole texture after a start or the end-of-match reveal, else the cells that flipped
	EnsureFog(Model);
	const bool bReveal = Ground && Ground->IsEndRevealed();
	if (bReveal != bLastReveal) { bLastReveal = bReveal; bFogAll = true; }
	const double F0 = FPlatformTime::Seconds();
	if (bFogAll) { WriteFog(Model, State, Ground, nullptr); bFogAll = false; FogPending.Reset(); }
	else if (FogPending.Num()) { WriteFog(Model, State, Ground, &FogPending); FogPending.Reset(); }
	WindowFogMs = FMath::Max(WindowFogMs, (FPlatformTime::Seconds() - F0) * 1000.0);
	WindowLayoutMs = FMath::Max(WindowLayoutMs, Model.LastUpdateMs());
	// at most MaxChunksPerFrame regions a frame, and no more once 4 ms are spent (the first always goes)
	const double T0 = FPlatformTime::Seconds();
	int32 Done = 0;
	for (auto It = Queue.CreateIterator(); It && Done < MaxChunksPerFrame; ++It)
	{
		if (Done && (FPlatformTime::Seconds() - T0) * 1000.0 > 4.0) break;
		BuildRegion(Model, *It);
		It.RemoveCurrent();
		Done++;
	}
	if (Done) { LastBuildMs = (FPlatformTime::Seconds() - T0) * 1000.0; RegionsBuilt += Done; WindowBuildMs = FMath::Max(WindowBuildMs, LastBuildMs); }
	// wrecks and ghosts: again when either list changed
	uint32 Hash = 0;
	for (const FTCGhost& G : State.Ghosts) Hash = FCrc::StrCrc32(*FString::Printf(TEXT("%d|%s|%d|%.3f|%.3f|%.3f"), G.Id, *G.Type, G.Owner, G.X, G.Z, G.Built), Hash);
	bool bMovingWreck = false;
	for (const FTCWreckRow& Wr : Model.Wrecks()) bMovingWreck |= Wr.bMoving;
	// Delivered failed sections invalidate remembered building meshes in stable cell order.
	TArray<int32> FailedCells;
	for (const auto& Section : State.Sections) if (Section.Value.State == TEXT("failed") && Section.Value.StructureId.StartsWith(TEXT("building:"))) FailedCells.Add(Section.Key);
	FailedCells.Sort();
	for (const int32 Cell : FailedCells) Hash = FCrc::StrCrc32(*FString::Printf(TEXT("%d|%s"), Cell, *State.Sections[Cell].StructureId), Hash);
	if (Model.TakeWrecksDirty() || Hash != GhostHash || bModelsDirty || bMovingWreck) { GhostHash = Hash; bModelsDirty = false; DrawModels(Model, State, Ground); }
	static double LastLog = 0;
	const double Now = FPlatformTime::Seconds();
	if (Now - LastLog >= 5.0)
	{
		// the most this frame's share of the work took in any frame of the window: layout (FTCWorldObjects::Update),
		// component rebuilds and the fog texture
		LastLog = Now;
		UE_LOG(LogTC, Log, TEXT("world view: %s | worst in the last window: layout %.2f ms, rebuilds %.2f ms, fog %.2f ms"), *StatusLine(), WindowLayoutMs, WindowBuildMs, WindowFogMs);
		WindowLayoutMs = WindowBuildMs = WindowFogMs = 0;
	}
}

int32 FTCWorldObjectsView::SectionsDrawn() const
{
	int32 N = 0;
	for (const TPair<uint64, FComp>& P : Comps) N += P.Value.Count > 0 ? 1 : 0;
	for (const TPair<FString, TWeakObjectPtr<UInstancedStaticMeshComponent>>& P : ModelComps) if (const UInstancedStaticMeshComponent* C = P.Value.Get()) N += C->GetInstanceCount() > 0 ? 1 : 0;
	return N;
}

int32 FTCWorldObjectsView::TrianglesDrawn() const
{
	int64 N = 0;
	for (const TPair<uint64, FComp>& P : Comps) N += (int64)P.Value.Count * P.Value.Triangles;
	return (int32)FMath::Min<int64>(N, MAX_int32);
}

int32 FTCWorldObjectsView::InstancesDrawn() const
{
	int32 N = ModelInstances;
	for (const TPair<uint64, FComp>& P : Comps) N += P.Value.Count;
	return N;
}

FString FTCWorldObjectsView::StatusLine() const
{
	using namespace TCWorldObjectsImpl;
	int32 Pieces = 0, Props = 0, Detail = 0, Live = 0;
	for (const TPair<uint64, FComp>& P : Comps)
	{
		if (!P.Value.Count) continue;
		Live++;
		const int32 Layer = (int32)((P.Key >> 16) & 0xffff);
		(Layer == LayerPieces ? Pieces : Layer == LayerProps ? Props : Detail) += P.Value.Count;
	}
	return FString::Printf(TEXT("%d instanced components (%d made), instances: pieces %d, props %d + %d near detail, wreck and ghost parts %d (%d wrecks, %d ghosts, %d without a model); %d triangles at full detail; %d regions queued, %d built (last batch %.2f ms); fog %dx%d; world materials %s"),
		Live, Comps.Num() + ModelComps.Num(), Pieces, Props, Detail, ModelInstances, WrecksDrawn, GhostsDrawn, ModelsMissing, TrianglesDrawn(), Queue.Num(), RegionsBuilt, LastBuildMs, FogW, FogH,
		MissingWorldMaterials ? *FString::Printf(TEXT("missing for %d slots"), MissingWorldMaterials) : TEXT("ok"));
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Engine/Engine.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWreckPresentationTest, "ThreeCrossroads.World.WreckPresentation", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCWreckPresentationTest::RunTest(const FString&)
{
	// Exercise the real world adapter, including the callbacks that used to discard wreck poses.
	auto Parse = [](const FString& S) { TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(S), O); return O; };
	const auto Start = Parse(TEXT(R"({"map":{"w":1,"h":1,"rows":["."]}})"));
	const auto Snap = Parse(TEXT(R"({"wrecks":[[7,"tank",0,10,20,0.7,{}]]})"));
	FTCMatchState State; State.Tick = 2; State.Factions.Add(0);
	FTCWorldObjects Model;
	if (!TestTrue(TEXT("wreck model starts"), Model.ApplyStart(*Start, State))) return false;
	Model.ApplySnapshot(*Snap);
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCWreckPresentation"));
	if (!TestNotNull(TEXT("transient world"), World)) return false;
	FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game); Context.SetCurrentWorld(World);
	World->SpawnActor<ATCLighting>();
	ATCEffects* Effects = World->SpawnActor<ATCEffects>();
	FTCWorldObjectsView View;
	if (TestNotNull(TEXT("effects actor"), Effects) && TestTrue(TEXT("world view attaches"), View.Attach(World)))
	{
		FTCDebrisRow Row; Row.Id = TEXT("wreck-7"); Row.bWreck = true; Row.WreckId = 7;
		Row.X = 10; Row.Y = 2; Row.Z = 20; Row.HullDir = .7; Row.Tilt = .1;
		Effects->Fx.Debris.Snapshot(State, {Row}, {}, 0);
		State.Tick = 4; Row.X = 14; Row.Y = 1; Row.Z = 22; Row.Tilt = .5;
		Effects->Fx.Debris.Snapshot(State, {Row}, {}, .1); Effects->Fx.Debris.Frame(.15);
		View.DrawModels(Model, State, nullptr);
		TestEqual(TEXT("one real hulk is drawn"), View.WrecksDrawn, 1);
		const FTCModelEntry* Entry = View.Library.Resolve({TEXT("tank"), 0, FString()});
		if (TestNotNull(TEXT("source tank catalog"), Entry))
		{
			const FTCModelPart& Part = Entry->Parts[0];
			const auto* Found = View.ModelComps.Find(Part.Mesh.ToString() + TEXT("|wreck"));
			UInstancedStaticMeshComponent* Comp = Found ? Found->Get() : nullptr;
			if (TestNotNull(TEXT("hulk instance component"), Comp))
			{
				TestTrue(TEXT("wreck receives the source unshadowed unit sun"), Comp->LightingChannels.bChannel1 && !Comp->CastShadow);
				for (int32 I = 0; I < Comp->GetNumMaterials(); I++)
					if (Comp->GetMaterial(I)->GetName().StartsWith(TEXT("MaterialInstanceDynamic")))
					{
						FLinearColor Colour;
						TestTrue(TEXT("hulk uses opaque Lambert replacement"), Comp->GetMaterial(I)->GetBlendMode() == BLEND_Opaque);
						TestTrue(TEXT("wreck colour is the original flat 0x1d1b18"), Comp->GetMaterial(I)->GetVectorParameterValue(FMaterialParameterInfo(TEXT("Color")), Colour) && Colour.Equals(TCLook::HexLinear(0x1d1b18), 1e-6f));
					}
				FTransform Drawn; TestTrue(TEXT("hulk has an instance"), Comp->GetInstanceTransform(0, Drawn, true));
				// Independent reflection of Three Ry(-hullDir)*Rz(tilt), at the delivered halfway pose.
				const FVector P = FTCModelLibrary::PartTransform(*Entry, 0, FTransform::Identity).TransformPosition(FVector(100,200,300));
				const double X = P.X * FMath::Cos(.3) - P.Z * FMath::Sin(.3), Z = P.X * FMath::Sin(.3) + P.Z * FMath::Cos(.3);
				const FVector Expected(1200 + X * FMath::Cos(.7) - P.Y * FMath::Sin(.7), 2100 + X * FMath::Sin(.7) + P.Y * FMath::Cos(.7), 150 + Z);
				TestTrue(TEXT("world adapter draws interpolated slide, height and tilt"), Drawn.TransformPosition(FVector(100,200,300)).Equals(Expected, .01));
			}
		}
		Effects->Fx.Reset(); TestNull(TEXT("match reset clears held wreck pose"), Effects->Fx.WreckPose(7));
		View.Reset(); TestEqual(TEXT("reset clears world model instances"), View.InstancesDrawn(), 0);
	}
	View.Detach(); GEngine->DestroyWorldContext(World); World->DestroyWorld(false);
	return true;
}
#endif
