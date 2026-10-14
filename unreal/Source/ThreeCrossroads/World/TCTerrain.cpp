#include "World/TCTerrain.h"
#include "ThreeCrossroads.h"
#include "Net/TCMatchState.h"
#include "ProceduralMeshComponent.h"
#include "Materials/MaterialInterface.h"
#include "World/TCGroundSurface.h"

static_assert(TCGround::CellMetres == FTCMatchState::CellMetres && TCGround::LevelMetres == FTCMatchState::LevelMetres, "one cell and level size");

namespace
{
	// The browser minimap's ground colours per terrain character (client/main.js MM_COLORS). K (a building section)
	// and Q (a wreck) are not in that table; they borrow B and R here.
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
}

ATCTerrain::ATCTerrain()
{
	PrimaryActorTick.bCanEverTick = false;
	Mesh = CreateDefaultSubobject<UProceduralMeshComponent>(TEXT("Ground"));
	Mesh->bUseAsyncCooking = false;
	Mesh->SetCollisionEnabled(ECollisionEnabled::NoCollision); // picking is analytic (RayToGround)
	Mesh->SetCastShadow(false);
	RootComponent = Mesh;
	// client/ground.js draws the ground with a MeshLambertMaterial: the cell colors lit by the sun and the sky light, no
	// specular (Client/TCLighting.h, unreal/Tools/look/make-look-assets.py M_TCGroundLambert)
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> VertexColor(TEXT("/Game/Models/Materials/Look/M_TCGroundLambert.M_TCGroundLambert"));
	Material = VertexColor.Object;
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
	QueueAll(true);
	CopyShape(State); // after the queueing, so the first Flush does not copy again
	MeshedTotal = 0;
	if (W <= 0 || H <= 0) { UE_LOG(LogTC, Log, TEXT("terrain: cleared (no match held)")); return; }
	UE_LOG(LogTC, Log, TEXT("terrain: %s %dx%d cells, %d chunks queued%s"), *State.Map.Name, W, H, Queue.Num(),
		bKeep ? TEXT(" (same match: drawn ground kept while it rebuilds)") : TEXT(""));
}

void ATCTerrain::CopyShape(const FTCMatchState& State)
{
	Levels = State.Map.Levels;
	Known = State.Map.Known;
	if (Levels.Num() != W * H || Known.Num() != W * H) { Levels.Init(0, W * H); Known.Init(false, W * H); }
	MinLevel = 0; MaxLevel = 0; // unknown cells and their guesses never leave the range of known levels and 0
	for (const int8 L : Levels) { MinLevel = FMath::Min<int32>(MinLevel, L); MaxLevel = FMath::Max<int32>(MaxLevel, L); }
	bShapePending = false;
}

void ATCTerrain::Queue_(int32 Chunk, bool bShape)
{
	if (Chunk < 0 || Chunk >= ChunksX * ChunksY) return;
	bool& Shape = Queue.FindOrAdd(Chunk, false);
	Shape |= bShape;
	bShapePending |= bShape;
}

void ATCTerrain::QueueAll(bool bShape)
{
	for (int32 C = 0; C < ChunksX * ChunksY; C++) Queue_(C, bShape);
}

void ATCTerrain::MarkCells(const TSet<int32>& Shaped, const TSet<int32>& Fogged)
{
	if (W <= 0) return;
	for (int32 C : Shaped)
	{
		if (C < 0 || C >= W * H) continue;
		const int32 X = C % W, Y = C / W;
		// A cell's level and knowledge reach the 9 nodes it touches, which neighbouring cells share, and the normals of
		// the nodes next to those (still inside the neighbours): the chunks within one cell reshape.
		for (int32 DY = -1; DY <= 1; DY++) for (int32 DX = -1; DX <= 1; DX++)
		{
			const int32 NX = X + DX, NY = Y + DY;
			if (NX >= 0 && NY >= 0 && NX < W && NY < H) Queue_((NY / ChunkCells) * ChunksX + NX / ChunkCells, true);
		}
	}
	for (int32 C : Fogged) if (C >= 0 && C < W * H) Queue_(((C / W) / ChunkCells) * ChunksX + (C % W) / ChunkCells, false);
}

void ATCTerrain::Flush(const FTCMatchState& State)
{
	if (W <= 0 || State.Map.W != W || State.Map.H != H) return;
	// client/main.js: the fog lifts once the match is decided, on authored maps only (the server sends every unit then)
	if (!bEndRevealed && !State.Map.bWorld && State.Winner >= -1)
	{
		bEndRevealed = true;
		QueueAll(false);
		UE_LOG(LogTC, Log, TEXT("terrain: match decided (winner %d), fog lifted on %d chunks"), State.Winner, ChunksX * ChunksY);
	}
	if (!Queue.Num()) return;
	if (bShapePending) CopyShape(State);
	int32 Done = 0;
	for (auto It = Queue.CreateIterator(); It && Done < MaxChunksPerFrame; ++It, ++Done)
	{
		MeshChunk(State, It.Key(), It.Value());
		It.RemoveCurrent();
	}
}

FLinearColor ATCTerrain::CellColor(const FTCMatchState& State, int32 C) const
{
	const FColor Base = CharColor(State.Map.Chars[C]);
	const float K = (1.f + Levels[C] * 0.12f) * (bEndRevealed || State.CellVisible(C) ? 1.f : State.CellExplored(C) ? 0.55f : 0.32f);
	const FColor Lit(FMath::Clamp(int32(Base.R * K), 0, 255), FMath::Clamp(int32(Base.G * K), 0, 255), FMath::Clamp(int32(Base.B * K), 0, 255));
	return FLinearColor(Lit);
}

int32 ATCTerrain::CellsDrawn() const
{
	int64 Verts = 0;
	for (const int32 N : SectionVerts) Verts += N;
	return (int32)(Verts / TCGround::CellNodeCount);
}

float ATCTerrain::HeightAt(float X, float Z) const
{
	if (W <= 0 || H <= 0 || Levels.Num() != W * H || Known.Num() != W * H) return 0.f;
	const auto Surface = TCGround::MakeSurface(W, H, [this](int32 C) { return Levels[C]; }, [this](int32 C) { return (bool)Known[C]; });
	return Surface.HeightAt(X, Z);
}

void ATCTerrain::MeshChunk(const FTCMatchState& State, int32 Chunk, bool bShape)
{
	const int32 CX0 = (Chunk % ChunksX) * ChunkCells, CY0 = (Chunk / ChunksX) * ChunkCells;
	const int32 CX1 = FMath::Min(W, CX0 + ChunkCells), CY1 = FMath::Min(H, CY0 + ChunkCells);
	const float Half = TCGround::CellMetres * 0.5f;
	const auto Surface = TCGround::MakeSurface(W, H, [this](int32 C) { return Levels[C]; }, [this](int32 C) { return (bool)Known[C]; });

	// Node levels of this chunk plus a margin of one node for the normals, from the same surface HeightAt reads. Every
	// node of a known cell touches that cell, so it always has a level; margin nodes may not (unknown or off the map).
	const int32 I0 = 2 * CX0 - 1, J0 = 2 * CY0 - 1, NI = 2 * (CX1 - CX0) + 3, NJ = 2 * (CY1 - CY0) + 3;
	TArray<float> NodeLv;
	TBitArray<> NodeOk;
	NodeLv.SetNumUninitialized(NI * NJ);
	NodeOk.Init(false, NI * NJ);
	for (int32 B = 0; B < NJ; B++) for (int32 A = 0; A < NI; A++)
	{
		const int32 I = I0 + A, J = J0 + B;
		float L = 0.f;
		const bool bOk = I >= 0 && J >= 0 && I <= 2 * W && J <= 2 * H && Surface.NodeLevel(I, J, L);
		NodeLv[B * NI + A] = L;
		NodeOk[B * NI + A] = bOk;
	}
	auto NodeNormal = [&](int32 A, int32 B)
	{
		const float Here = NodeLv[B * NI + A];
		// central differences over the neighbouring nodes this client has heights for, one-sided where it has one
		auto Slope = [&](int32 DA, int32 DB)
		{
			const int32 Lo = B * NI + A - DA - DB * NI, Hi = B * NI + A + DA + DB * NI;
			const bool bLo = NodeOk[Lo], bHi = NodeOk[Hi];
			const float Span = float((bLo ? 1 : 0) + (bHi ? 1 : 0)) * Half;
			return Span > 0.f ? ((bHi ? NodeLv[Hi] : Here) - (bLo ? NodeLv[Lo] : Here)) * TCGround::LevelMetres / Span : 0.f;
		};
		return FVector(-Slope(1, 0), -Slope(0, 1), 1.f).GetSafeNormal();
	};

	TArray<FLinearColor> Colors;
	TArray<FVector> Verts;
	TArray<int32> Tris;
	TArray<FVector> Normals;
	TArray<FVector2D> UV;
	// Each known cell is a fan of 8 triangles around its centre (TCGroundSurface.h), with its own flat colour. The nodes
	// come from the shared surface, so neighbouring cells meet and units and clicks sit on what is drawn.
	for (int32 Y = CY0; Y < CY1; Y++) for (int32 X = CX0; X < CX1; X++)
	{
		const int32 C = Y * W + X;
		if (!Known[C]) continue;
		const FLinearColor Color = CellColor(State, C);
		const int32 Base = Verts.Num();
		for (int32 K = 0; K < TCGround::CellNodeCount; K++)
		{
			const int32 I = 2 * X + TCGround::NodeDI[K], J = 2 * Y + TCGround::NodeDJ[K];
			const int32 A = I - I0, B = J - J0;
			Verts.Add(ToWorld(I * Half, J * Half, NodeLv[B * NI + A] * TCGround::LevelMetres));
			Normals.Add(NodeNormal(A, B));
			UV.Add(FVector2D(TCGround::NodeDI[K] * 0.5f, TCGround::NodeDJ[K] * 0.5f));
			Colors.Add(Color);
		}
		// Unreal is left-handed with Z up: centre, next ring node, ring node is the winding that faces up (the order the
		// one-quad mesh used before).
		for (int32 K = 0; K < TCGround::CellTriangleCount; K++)
			Tris.Append({ Base, Base + 1 + (K + 1) % 8, Base + 1 + K });
	}
	// A fog flip only recolours: the section keeps its buffers (the engine wants the positions passed again). If the
	// known cells changed underneath, the vertex count differs and the section is recreated instead.
	if (!bShape && Verts.Num() && SectionVerts[Chunk] == Verts.Num())
	{
		Mesh->UpdateMeshSection_LinearColor(Chunk, Verts, Normals, UV, Colors, TArray<FProcMeshTangent>());
		return;
	}
	MeshedTotal++;
	SectionVerts[Chunk] = Verts.Num();
	if (!Verts.Num()) { Mesh->ClearMeshSection(Chunk); return; }
	Mesh->CreateMeshSection_LinearColor(Chunk, Verts, Tris, Normals, UV, Colors, TArray<FProcMeshTangent>(), false);
	Mesh->SetMaterial(Chunk, Material);
}

bool ATCTerrain::RayToGround(const FVector& Origin, const FVector& Dir, FVector2D& OutMetres) const
{
	if (W <= 0 || Dir.IsNearlyZero()) return false;
	const FVector D = Dir.GetSafeNormal();
	const double MaxX = W * FTCMatchState::CellMetres, MaxZ = H * FTCMatchState::CellMetres;
	auto Above = [this](const FVector& P) { return P.Z / 100.0 - HeightAt(float(P.X / 100.0), float(P.Y / 100.0)); };
	// Only march where ground can be: between the lowest and the highest level (cm), slightly widened.
	const double Lo = MinLevel * TCGround::LevelMetres * 100.0 - 1.0, Hi = MaxLevel * TCGround::LevelMetres * 100.0 + 1.0;
	double T0 = 0.0, T1 = 2000000.0; // 20 km
	if (FMath::Abs(D.Z) > 1e-6)
	{
		const double TA = (Hi - Origin.Z) / D.Z, TB = (Lo - Origin.Z) / D.Z;
		T0 = FMath::Max(T0, FMath::Min(TA, TB));
		T1 = FMath::Min(T1, FMath::Max(TA, TB));
	}
	else if (Origin.Z < Lo || Origin.Z > Hi) return false;
	if (T1 < T0) return false;
	const double Step = 25.0; // cm: a quarter of the shortest side of a drawn triangle
	FVector Prev = Origin + D * T0;
	if (Above(Prev) <= 0.0) return false; // starts under the ground
	for (int32 I = 1; I < 20000; I++)
	{
		const double T = FMath::Min(T1, T0 + Step * I);
		const FVector P = Origin + D * T;
		if (Above(P) <= 0.0)
		{
			FVector Lower = Prev, Upper = P;
			for (int32 K = 0; K < 20; K++) { const FVector Mid = (Lower + Upper) * 0.5; (Above(Mid) > 0.0 ? Lower : Upper) = Mid; }
			OutMetres = FVector2D(Upper.X / 100.0, Upper.Y / 100.0);
			return OutMetres.X >= 0 && OutMetres.Y >= 0 && OutMetres.X <= MaxX && OutMetres.Y <= MaxZ;
		}
		if (T >= T1) break;
		Prev = P;
	}
	return false;
}
