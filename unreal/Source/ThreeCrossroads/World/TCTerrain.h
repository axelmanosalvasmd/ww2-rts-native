// The ground, meshed from what this client was sent: the start's map rows and heights, then the cells its side learns.
// The mesh is the reference's relief (client/relief.js, ported in World/TCRelief.h): cell centres at their simulation
// height, cubic ramps only between connected neighbours, vertical cliffs on steps of 2+ levels with sealed rock faces and
// fractured rock models, banks carved below each water line, trenches 0.9 m and roads 0.1 m deep, crater and rubble lips.
// It is built in 32 x 32 cell chunks, one mesh section each; a changed cell rebuilds only the chunks it reaches.
// World Conquest cells nobody on this side discovered are never drawn and never shape the ground next to them. Fog dims
// what is out of sight (1 in sight, 0.55 explored, 0.32 never seen), and lifts once an authored-map match is decided
// (client/main.js). Axes: Unreal X = reference x, Unreal Y = reference z (rows go down the screen seen from above), Z up;
// 1 m = 100 cm. Material: /Game/Relief/M_TCRelief (unreal/Tools/relief/make-relief-material.py, client/relief-material.js).
#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "World/TCRelief.h"
#include "Commandlets/Commandlet.h"
#include "TCTerrain.generated.h"

class UProceduralMeshComponent;
class UMaterialInterface;
class UMaterialInstanceDynamic;
class FTCMatchState;
class FTCWorldObjects;

UCLASS()
class THREECROSSROADS_API ATCTerrain : public AActor
{
	GENERATED_BODY()

public:
	ATCTerrain();

	// A start arrived: the relief is rebuilt from it and every chunk is queued for upload. A start for a different match,
	// room, seat, map or size clears the ground first; a reconnect to the same match keeps the drawn chunks up while they
	// are replaced.
	void ResetFrom(const FTCMatchState& State);
	// Cells whose terrain or knowledge changed reshape the relief at the next Flush; fog flips only recolour their chunks.
	void MarkCells(const TSet<int32>& Shaped, const TSet<int32>& Fogged);
	// Applies the marked cells to the relief (relief.js update) and uploads at most MaxChunksPerFrame queued chunks, so a
	// big reveal spreads over frames. Call every frame of a match: it also lifts the fog when an authored match is decided.
	void Flush(const FTCMatchState& State);
	// The physical grid (client/main.js terrain.physicalGrid: a known mine drawn as the ground and object under it). Without
	// it the start's and cell rows' characters are used, a mine ('N') as open ground.
	void SetCellSource(const FTCWorldObjects* Objects) { CellSource = Objects; }

	// Ground height in metres at a reference position: the drawn ground (rock models aside, which are decoration). At cell
	// centres it is the level x 2.5 m, except banks, trenches, roads and crater lips (docs/contracts/terrain.md).
	float HeightAt(float X, float Z) const;
	// The first ground point a ray (Unreal cm) hits inside the map, as reference metres: relief.js mesh.raycast over the
	// drawn triangles.
	bool RayToGround(const FVector& Origin, const FVector& Dir, FVector2D& OutMetres) const;
	// The water line at a reference position (metres, relief.js waterAt), NaN away from water.
	float WaterAt(float X, float Z) const;
	static FVector ToWorld(float X, float Z, float HeightMetres) { return FVector(X * 100.f, Z * 100.f, HeightMetres * 100.f); }

	int32 MapW() const { return W; }
	int32 MapH() const { return H; }
	int32 ChunksMeshed() const { return MeshedTotal; }
	int32 ChunksQueued() const { return Queue.Num(); }
	// Known cells the ground mesh holds now.
	int32 CellsDrawn() const { return DrawnCells; }
	// Slice 13 lead check (recipient knowledge): drawn vertices whose cell this client does not know (the rule is 0),
	// with the vertices drawn in all and how many of them have no owning cell (rock models, walls between two cells).
	int32 DrawnOnUnknown(const FTCMatchState& State, int32* OutVertices = nullptr, int32* OutUnowned = nullptr) const;
	// The authored-map match is decided and the ground is drawn without fog.
	bool IsEndRevealed() const { return bEndRevealed; }
	// The last start kept the drawn ground (a reconnect to the same match) instead of clearing it.
	bool LastResetKeptGround() const { return bLastResetKept; }
	// The relief, for tests and the water and paint views (read only).
	const TCRelief::FRelief& Relief() const { return ReliefCore; }
	// Vertices uploaded for a chunk (0: none), for tests.
	int32 SectionVertices(int32 Chunk) const { return SectionVerts.IsValidIndex(Chunk) ? SectionVerts[Chunk] : 0; }

	int32 MaxChunksPerFrame = 24;
	static constexpr int32 ChunkCells = 32;

private:
	void UploadChunk(const FTCMatchState& State, int32 Chunk, bool bShape);
	FLinearColor CellColor(const FTCMatchState& State, int32 C) const;
	void Queue_(int32 Chunk, bool bShape);
	void CopyInputs(const FTCMatchState& State, bool bFrozen);
	void EnsureMaterial(bool bLow);

	UPROPERTY() TObjectPtr<UProceduralMeshComponent> Mesh;
	UPROPERTY() TObjectPtr<UMaterialInterface> Material;
	UPROPERTY() TObjectPtr<UMaterialInstanceDynamic> MaterialMid;
public:
	// Slice 13 (paint): the ground's dynamic material, for FTCGroundPaint::Bind (its MF_TCGroundPaint parameters).
	UMaterialInstanceDynamic* GroundMaterialInstance() const { return MaterialMid; }
private:

	TCRelief::FRelief ReliefCore;
	const FTCWorldObjects* CellSource = nullptr;
	int32 W = 0, H = 0, ChunksX = 0, ChunksY = 0, MeshedTotal = 0, DrawnCells = 0;
	TArray<uint8> StartRows;          // the map rows at the start (ground.js town check on an authored map)
	TArray<int32> SectionVerts;       // vertices in each chunk's mesh section (0: none)
	TMap<int32, bool> Queue;          // chunk -> needs reshaping (else only recolouring)
	TSet<int32> PendingCells;         // shaped cells not yet applied to the relief
	bool bEndRevealed = false;        // latched until the next start, like client/epilogue.js active()
	bool bLastResetKept = false;

	// what the ground on screen was built from, to tell a reconnect from a new match
	FString BuiltRoom, BuiltMapName;
	int32 BuiltMatch = -1, BuiltYou = -1;
	bool bBuiltWorld = false, bBuiltSpectator = false;
};

// -run=TCReliefShot -Map=<reference maps/*.json> -Out=<dir> [-Views=x,z,dist,yaw;...] [-Name=<label>]: the relief of an
// authored map under the game's light (default mood), seen from client/camera.js cameras (fov 42, pitch 0.95), saved as
// <dir>/<label>-<x>-<z>-<dist>-<yaw>.png. Only the ground is drawn. Evidence for slice 13 (relief).
UCLASS()
class THREECROSSROADS_API UTCReliefShotCommandlet : public UCommandlet
{
	GENERATED_BODY()
public:
	UTCReliefShotCommandlet();
	virtual int32 Main(const FString& Params) override;
};
