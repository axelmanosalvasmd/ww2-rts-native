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
	void Triangle(const FVector& A, const FVector& B, const FVector& C, const FLinearColor& Colour, bool bDepthTest, bool bBeforeFog = false);
	void RibbonTriangle(const FVector& A, const FVector& B, const FVector& C, const FLinearColor& Colour, bool bDepthTest, const FVector2D& U, const FVector2D& V, const FVector2D& W);
	void Flush();
	static bool TentForMode(const FString& Mode, bool bWorldMap);
	static FString CampMode(const FString& StartMode, const FString& LobbyMode);
	static bool BagInside(double X, double Z, double W, double H);
	static bool CampSlotDrawn(int32 Slot, const TArray<TOptional<FVector2D>>& Spawns);
private:
#if WITH_DEV_AUTOMATION_TESTS
	friend class FTCCampPlacementTest;
#endif
	TWeakObjectPtr<AActor> Actor;
	TWeakObjectPtr<UProceduralMeshComponent> Ground, Through, Borders;
	uint64 Frame = MAX_uint64;
	int32 Section = 0;
	struct FBatch { TArray<FVector> Vertices; TArray<int32> Indices; TArray<FLinearColor> Colours; TArray<FVector2D> UV; void Reset() { Vertices.Reset(); Indices.Reset(); Colours.Reset(); UV.Reset(); } };
	FBatch DepthBatch, ThroughBatch, BorderBatch;
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
