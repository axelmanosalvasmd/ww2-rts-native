// The pinned client/structures.js piece builders, independent of placement and engine rendering.
// Models remain the source-exported piece/* meshes. Inputs are recipient-known cells only.
#pragma once

#include <vector>
#include <cstdint>

namespace TCWorldCore
{
	class FCellSource;
	struct FPiece;
	struct FHouseRect;
	struct FHouseTint;
}

namespace TCStructures
{
	// structures.js C.foot and C.tint, shared by house and cell builders during one rebuild.
	struct FContext
	{
		const std::vector<unsigned char>* Foot = nullptr;
		const std::vector<int>* TintIndex = nullptr;
		const std::vector<TCWorldCore::FHouseTint>* Tints = nullptr;
	};

	// Appends put() instances in the source's call order. A completely fallen house emits nothing.
	void House(const TCWorldCore::FCellSource& Source, const TCWorldCore::FHouseRect& Rect,
		std::vector<TCWorldCore::FPiece>& Out, const FContext& Context = {});
	// structures.js rebuild: hedges, walls, trenches, wire, traps, mines, hospitals, rubble, bridges.
	void Chunk(const TCWorldCore::FCellSource& Source, int X0, int Y0, int X1, int Y1,
		std::vector<TCWorldCore::FPiece>& Out, const FContext& Context = {});

	// unit-models.js updateBuildingBreach: sorted failed section cells, at most nine world-space columns.
	struct FColumn { double X0, X1, Z0, Z1; };
	std::vector<FColumn> FailedColumns(std::vector<int> Cells, int Width, double Cell = 2.0);
	// unit-models.js breachGeometry: position is the first three floats of each vertex. Remaining attributes
	// (normals, UVs, colours and material selectors) interpolate at each cut. Matrix is Three column-major.
	// Empty Indices means non-indexed triangles. The result is non-indexed, like the source geometry.
	std::vector<float> BreachTriangles(const std::vector<float>& Attributes, int Stride,
		const std::vector<uint32_t>& Indices, const double Matrix[16], const std::vector<FColumn>& Columns);
}

#if !defined(TC_WORLD_CORE_ONLY)
#include "CoreMinimal.h"
#include "UObject/StrongObjectPtrTemplates.h"
class UStaticMesh;
class FTCMatchState;
struct FTCModelPart;
namespace TCStructures
{
	// Reuses the imported source model, its material sections and every shader attribute. Source meshes must
	// retain CPU buffers when cooked. The renderer owns/cache-clears the returned transient mesh.
	UStaticMesh* BreachMesh(UStaticMesh* Source, const std::vector<FColumn>& Columns,
		const FTransform& WorldTransform, UObject* Outer, bool& bEmpty);
	// One cache per renderer. The lead's unit and ghost placement paths call PartFor before drawing a part.
	class FBuildingBreaches
	{
	public:
		bool PartFor(const FTCModelPart& Source, int32 BuildingId, const FTCMatchState& State,
			const FTransform& WorldTransform, UObject* Outer, FTCModelPart& Out);
		void Reset() { Cache.Empty(); }
	private:
		struct FEntry { TStrongObjectPtr<UStaticMesh> Mesh; bool bEmpty = false; };
		TMap<FString, FEntry> Cache;
	};
}
#endif
