// Reference HQ dressing and world-space marker batches. Owned by the match's FTCMarkers, never the HUD.
#pragma once
#include "CoreMinimal.h"
#include "UObject/WeakObjectPtrTemplates.h"
#include "Client/TCModelLibrary.h"
class ATCGameMode;
class AActor;
class UProceduralMeshComponent;
class UInstancedStaticMeshComponent;
class UTexture2D;
class UMaterialInterface;
class UMaterialInstanceDynamic;

class THREECROSSROADS_API FTCMarkerScene
{
public:
	~FTCMarkerScene();
	void Begin(const ATCGameMode& Match);
	void Triangle(const FVector& A, const FVector& B, const FVector& C, const FLinearColor& Colour, bool bDepthTest);
	void Flush();
	static bool TentForMode(const FString& Mode, bool bWorldMap);
	static bool BagInside(double X, double Z, double W, double H);
	static bool CampSlotDrawn(int32 Slot, const TArray<TOptional<FVector2D>>& Spawns);
private:
	TWeakObjectPtr<AActor> Actor;
	TWeakObjectPtr<UProceduralMeshComponent> Ground, Through;
	uint64 Frame = MAX_uint64;
	int32 Section = 0;
	struct FBatch { TArray<FVector> Vertices; TArray<int32> Indices; TArray<FLinearColor> Colours; void Reset() { Vertices.Reset(); Indices.Reset(); Colours.Reset(); } };
	FBatch DepthBatch, ThroughBatch;
	FTCModelLibrary Library;
	TArray<TWeakObjectPtr<UInstancedStaticMeshComponent>> Models;
	FString ModelKey;
	TWeakObjectPtr<UTexture2D> FogTexture;
	TMap<UMaterialInterface*, TWeakObjectPtr<UMaterialInstanceDynamic>> FogMaterials;
	TArray<uint8> FogBytes;
	int32 FogW = 0, FogH = 0;
	void UpdateFog(const ATCGameMode& Match);
	UMaterialInterface* WithFog(UMaterialInterface* Material);
	void SyncModels(const ATCGameMode& Match);
};
