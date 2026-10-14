// The ground, meshed from what this client was sent: the start's map rows and heights, then the cells its side learns.
// World Conquest cells nobody on this side discovered are never drawn and never shape the ground next to them. Fog dims
// what is out of sight, and lifts once an authored-map match is decided (client/main.js). Axes: Unreal X = reference x,
// Unreal Y = reference z (rows go down the screen seen from above), Z up; 1 m = 100 cm. Surface: World/TCGroundSurface.h.
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "TCTerrain.generated.h"

class UProceduralMeshComponent;
class UMaterialInterface;
class FTCMatchState;

UCLASS()
class THREECROSSROADS_API ATCTerrain : public AActor
{
	GENERATED_BODY()

public:
	ATCTerrain();

	// A start arrived: every chunk is queued for meshing. A start for a different match, room, seat, map or size clears
	// the ground first; a reconnect to the same match keeps the drawn chunks up while they are rebuilt.
	void ResetFrom(const FTCMatchState& State);
	// Cells whose terrain or knowledge changed reshape their chunks; fog flips only recolor theirs.
	void MarkCells(const TSet<int32>& Shaped, const TSet<int32>& Fogged);
	// Meshes at most MaxChunksPerFrame queued chunks, so a big reveal spreads over frames. Call every frame of a match:
	// it also notices the match being decided and lifts the fog on an authored map.
	void Flush(const FTCMatchState& State);

	// Ground height in metres at a reference position: exactly the drawn ground. At cell centres, edge midpoints and
	// corners of an authored map it equals the reference heightAt (shared/sim.js); between them it is the drawn triangle.
	float HeightAt(float X, float Z) const;
	// The first ground point a ray (Unreal cm) hits inside the map, as reference metres.
	bool RayToGround(const FVector& Origin, const FVector& Dir, FVector2D& OutMetres) const;
	static FVector ToWorld(float X, float Z, float HeightMetres) { return FVector(X * 100.f, Z * 100.f, HeightMetres * 100.f); }

	int32 MapW() const { return W; }
	int32 MapH() const { return H; }
	int32 ChunksMeshed() const { return MeshedTotal; }
	int32 ChunksQueued() const { return Queue.Num(); }
	// Cells the ground mesh holds now (each known cell is one fan of TCGround::CellNodeCount vertices).
	int32 CellsDrawn() const;
	// The authored-map match is decided and the ground is drawn without fog.
	bool IsEndRevealed() const { return bEndRevealed; }
	// The last start kept the drawn ground (a reconnect to the same match) instead of clearing it.
	bool LastResetKeptGround() const { return bLastResetKept; }

	int32 MaxChunksPerFrame = 24;

private:
	static constexpr int32 ChunkCells = 32;
	void MeshChunk(const FTCMatchState& State, int32 Chunk, bool bShape);
	FLinearColor CellColor(const FTCMatchState& State, int32 C) const;
	void Queue_(int32 Chunk, bool bShape);
	void QueueAll(bool bShape);
	void CopyShape(const FTCMatchState& State);

	UPROPERTY() TObjectPtr<UProceduralMeshComponent> Mesh;
	UPROPERTY() TObjectPtr<UMaterialInterface> Material;

	int32 W = 0, H = 0, ChunksX = 0, ChunksY = 0, MeshedTotal = 0;
	TArray<int8> Levels;              // copy of the map levels for HeightAt between rebuilds
	TBitArray<> Known;                // copy of the cells this client may draw (and sample heights from)
	int32 MinLevel = 0, MaxLevel = 0; // bounds of Levels and 0, so a pick ray only marches where ground can be
	TArray<int32> SectionVerts;       // vertices in each chunk's mesh section (0: none)
	TMap<int32, bool> Queue;          // chunk -> needs reshaping (else only recoloring)
	bool bShapePending = false;       // a queued chunk needs the latest levels and knowledge copied first
	bool bEndRevealed = false;        // latched until the next start, like client/epilogue.js active()
	bool bLastResetKept = false;

	// what the ground on screen was built from, to tell a reconnect from a new match
	FString BuiltRoom, BuiltMapName;
	int32 BuiltMatch = -1, BuiltYou = -1;
	bool bBuiltWorld = false, bBuiltSpectator = false;
};
