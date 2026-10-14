// World Conquest railways: the track and stations this player has explored, ported from the pinned reference
// (TCRailCore::ReferencePin):
//   client/railways.js   createRailways: cells (a track cell's direction bits, a station's region), frame (at most two
//                        rebuilds a second), rebuild (half segments from each cell centre toward each direction bit:
//                        ballast, sleepers and two rails as instanced unit boxes), buildStations (once the region's
//                        centre is known, the house turned away from the town), station (platform, trim, brick house,
//                        two roof planes, canopy and posts, a water tower on four legs), has, track, stations;
//   shared/sim.js        RAIL_DIRS, setupRails (each track cell's bits), terrainFor (data.rail, data.station);
//   client/main.js       createRailways on a World map at every start, railways.cells(m.cells) and applyCells(s.cells),
//                        center(id) = the last snapshot's world.regions entry, frame(dt) every frame.
// Only cells this player's side was sent are drawn: the track runs on as more is explored and ends at the edge of the
// known world. Each cell draws its own half of every joint.
//
// The first part (TCRailCore) is plain C++ with no engine types: it builds three.js's own instance matrices (Float64 here,
// Float32 in three) so a standalone harness can compare them with the reference's (unreal/Tools/atmosphere/harness).
// Define TC_RAIL_CORE_ONLY before including this header for that.
#pragma once

#include <cmath>
#include <cstdint>
#include <functional>
#include <unordered_map>
#include <vector>

namespace TCRailCore
{
	inline constexpr const char* ReferencePin = "e4e579a838aabae3b1d9dadbc17a0a87147bc52d";
	inline constexpr double Cell = 2.0;          // shared/sim.js CELL
	inline constexpr double Gauge = 0.72;        // railways.js GAUGE: each rail's offset from the track's centre line
	inline constexpr double SleeperGap = 0.66;   // railways.js SLEEPER_GAP
	inline constexpr double RebuildEvery = 0.5;  // railways.js frame: wait = 0.5 (at most two rebuilds a second)
	inline constexpr double Pi = 3.14159265358979323846;
	// shared/sim.js RAIL_DIRS: bit d of a track cell leaves it toward (dx, dz)
	inline constexpr int Dirs[8][2] = { { 1, 0 }, { 1, 1 }, { 0, 1 }, { -1, 1 }, { -1, 0 }, { -1, -1 }, { 0, -1 }, { 1, -1 } };

	// railways.js MATERIAL (MeshLambertMaterial colours, sRGB hex as three's Color.setHex takes them)
	enum EMat : int { Ballast, Sleeper, Rail, Platform, Brick, Roof, Wood, Tank, Trim, MatCount };
	inline constexpr uint32_t MatHex[MatCount] = { 0x5f5a52, 0x4b3b2b, 0x8f9297, 0x8d877b, 0x8a4a36, 0x4d4f55, 0x6e5638, 0x3e4a44, 0xd8cfb8 };
	inline constexpr const char* MatName[MatCount] = { "ballast", "sleeper", "rail", "platform", "brick", "roof", "wood", "tank", "trim" };

	// three.js Matrix4 elements, column-major (te[12..14] the position)
	struct FMat4 { double E[16] = { 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1 }; };
	struct FQuat { double X = 0, Y = 0, Z = 0, W = 1; };

	// three.js Quaternion.setFromEuler for order 'YXZ'
	inline FQuat QuatYXZ(double X, double Y, double Z)
	{
		const double C1 = std::cos(X / 2), C2 = std::cos(Y / 2), C3 = std::cos(Z / 2), S1 = std::sin(X / 2), S2 = std::sin(Y / 2), S3 = std::sin(Z / 2);
		return { S1 * C2 * C3 + C1 * S2 * S3, C1 * S2 * C3 - S1 * C2 * S3, C1 * C2 * S3 - S1 * S2 * C3, C1 * C2 * C3 + S1 * S2 * S3 };
	}
	// three.js Matrix4.compose(position, quaternion, scale)
	inline FMat4 Compose(double PX, double PY, double PZ, const FQuat& Q, double SX, double SY, double SZ)
	{
		const double X2 = Q.X + Q.X, Y2 = Q.Y + Q.Y, Z2 = Q.Z + Q.Z;
		const double XX = Q.X * X2, XY = Q.X * Y2, XZ = Q.X * Z2, YY = Q.Y * Y2, YZ = Q.Y * Z2, ZZ = Q.Z * Z2;
		const double WX = Q.W * X2, WY = Q.W * Y2, WZ = Q.W * Z2;
		FMat4 M;
		double* T = M.E;
		T[0] = (1 - (YY + ZZ)) * SX; T[1] = (XY + WZ) * SX; T[2] = (XZ - WY) * SX; T[3] = 0;
		T[4] = (XY - WZ) * SY; T[5] = (1 - (XX + ZZ)) * SY; T[6] = (YZ + WX) * SY; T[7] = 0;
		T[8] = (XZ + WY) * SZ; T[9] = (YZ - WX) * SZ; T[10] = (1 - (XX + YY)) * SZ; T[11] = 0;
		T[12] = PX; T[13] = PY; T[14] = PZ; T[15] = 1;
		return M;
	}
	// A * B (three.js Matrix4.multiplyMatrices)
	inline FMat4 Multiply(const FMat4& A, const FMat4& B)
	{
		FMat4 M;
		for (int R = 0; R < 4; R++)
			for (int C = 0; C < 4; C++)
			{
				double S = 0;
				for (int K = 0; K < 4; K++) S += A.E[K * 4 + R] * B.E[C * 4 + K];
				M.E[C * 4 + R] = S;
			}
		return M;
	}

	// One half segment: from a cell's centre (X0, Z0) toward one of its directions, half a cell (or half a diagonal) long.
	struct FHalf { double X0 = 0, Z0 = 0, X1 = 0, Z1 = 0; };

	// One part of a station: a unit box (or the water tower's cylinder) of a material, its world matrix in three's axes.
	struct FPart { int Mat = 0; bool bCylinder = false; FMat4 M; };
	struct FStation { int Cell = -1; int Region = -1; double X = 0, Z = 0, Y = 0, Yaw = 0; std::vector<FPart> Parts; };

	// What railways.js keeps and builds, in the reference's order. Height(x, z) is the drawn ground (main.js hAt);
	// Centre(region, x, z) gives a region's objective position when this side knows it.
	struct FModel
	{
		int W = 0;
		std::vector<int> TrackOrder;                 // the track Map's insertion order
		std::unordered_map<int, int> Track;          // cell -> direction bits
		std::vector<int> StationOrder;
		std::unordered_map<int, int> Stations;       // cell -> region id
		std::unordered_map<int, size_t> Built;       // cell -> index in BuiltStations
		std::vector<FStation> BuiltStations;
		bool bDirty = false;
		int Waiting = 0;                             // stations whose town is not known yet
		double Wait = 0;
		// the last rebuild: the instance matrices of ballast, sleepers ('ties') and rails, in three's order
		std::vector<FMat4> BallastM, TiesM, RailsM;
		int Rebuilds = 0;

		void Reset(int InW) { *this = FModel(); W = InW; }

		// railways.js cells: one start or snapshot terrain row [cell, ch, level, state, data]. Rail is data.rail (0 or
		// absent: none), Station data.station (bHasStation false when absent).
		void Cell_(int C, int Rail, bool bHasStation, int StationRegion)
		{
			if (Rail)
			{
				auto It = Track.find(C);
				if (It == Track.end()) { Track.emplace(C, Rail); TrackOrder.push_back(C); bDirty = true; }
				else if (It->second != Rail) { It->second = Rail; bDirty = true; }
			}
			if (bHasStation && !Stations.count(C)) { Stations.emplace(C, StationRegion); StationOrder.push_back(C); bDirty = true; }
		}

		// railways.js frame: true when it rebuilt the track or looked for stations to build this call
		bool Frame(double Dt, const std::function<double(double, double)>& Height, const std::function<bool(int, double&, double&)>& Centre, bool* bRebuilt = nullptr)
		{
			if (bRebuilt) *bRebuilt = false;
			if ((Wait -= Dt) > 0 || !(bDirty || Waiting)) return false;
			if (bDirty) { Rebuild(Height); if (bRebuilt) *bRebuilt = true; }
			bDirty = false; Wait = RebuildEvery;
			BuildStations(Height, Centre);
			return true;
		}

		static double CellX(int C, int W) { return (C % W + 0.5) * Cell; }
		static double CellZ(int C, int W) { return (C / W + 0.5) * Cell; }

		std::vector<FHalf> Halves() const
		{
			std::vector<FHalf> Out;
			for (const int C : TrackOrder)
			{
				const int Mask = Track.at(C);
				const double X = CellX(C, W), Z = CellZ(C, W);
				for (int D = 0; D < 8; D++)
					if (Mask & (1 << D)) Out.push_back({ X, Z, X + Dirs[D][0] * Cell / 2, Z + Dirs[D][1] * Cell / 2 });
			}
			return Out;
		}

		static int SleepersOn(double Len) { const long N = std::lround(Len / SleeperGap); return N > 1 ? (int)N : 1; } // Math.max(1, Math.round(...))

		// railways.js rebuild
		void Rebuild(const std::function<double(double, double)>& Height)
		{
			Rebuilds++;
			const std::vector<FHalf> H = Halves();
			BallastM.assign(H.size(), FMat4());
			RailsM.assign(H.size() * 2, FMat4());
			size_t NumTies = 0;
			for (const FHalf& S : H) NumTies += SleepersOn(std::hypot(S.X1 - S.X0, S.Z1 - S.Z0));
			TiesM.assign(NumTies, FMat4());
			size_t Ti = 0;
			for (size_t I = 0; I < H.size(); I++)
			{
				const FHalf& S = H[I];
				const double Len = std::hypot(S.X1 - S.X0, S.Z1 - S.Z0), Y0 = Height(S.X0, S.Z0), Y1 = Height(S.X1, S.Z1);
				const double Yaw = std::atan2(S.X1 - S.X0, S.Z1 - S.Z0), Pitch = -std::atan2(Y1 - Y0, Len);
				const FQuat Q = QuatYXZ(Pitch, Yaw, 0);
				const double RX = std::cos(Yaw), RZ = -std::sin(Yaw); // right = (cos yaw, 0, -sin yaw)
				auto Mid = [&](double K, double Up, double& PX, double& PY, double& PZ) { PX = S.X0 + (S.X1 - S.X0) * K; PY = Y0 + (Y1 - Y0) * K + Up; PZ = S.Z0 + (S.Z1 - S.Z0) * K; };
				double PX, PY, PZ;
				Mid(0.5, 0.05, PX, PY, PZ);
				BallastM[I] = Compose(PX, PY, PZ, Q, 2.3, 0.14, Len + 0.05);
				for (const int Side : { -1, 1 })
				{
					Mid(0.5, 0.25, PX, PY, PZ);
					RailsM[I * 2 + (Side > 0 ? 1 : 0)] = Compose(PX + RX * Side * Gauge, PY, PZ + RZ * Side * Gauge, Q, 0.09, 0.13, Len + 0.02);
				}
				const int N = SleepersOn(Len);
				for (int K = 0; K < N; K++) { Mid((K + 0.5) / N, 0.15, PX, PY, PZ); TiesM[Ti++] = Compose(PX, PY, PZ, Q, 2.0, 0.09, 0.24); }
			}
		}

		// railways.js buildStations
		void BuildStations(const std::function<double(double, double)>& Height, const std::function<bool(int, double&, double&)>& Centre)
		{
			Waiting = 0;
			for (const int C : StationOrder)
			{
				if (Built.count(C)) continue;
				const int Region = Stations.at(C);
				double TX = 0, TZ = 0;
				if (!Centre(Region, TX, TZ)) { Waiting = 1; continue; } // built once its region's centre is known
				auto It = Track.find(C);
				const int Mask = It != Track.end() ? It->second : 1;
				int D = 0;
				for (int K = 0; K < 8; K++) if (Mask & (1 << K)) { D = K; break; }
				const double X = CellX(C, W), Z = CellZ(C, W), Yaw = std::atan2((double)Dirs[D][0], (double)Dirs[D][1]);
				// the house is on the station's +x side: turn it round when the town lies that way
				const double Turn = (TX - X) * std::cos(Yaw) - (TZ - Z) * std::sin(Yaw) > 0 ? Pi : 0;
				FStation St = Station(Yaw + Turn);
				St.Cell = C; St.Region = Region; St.X = X; St.Z = Z; St.Y = Height(X, Z);
				// the group's matrix (position, rotation.y), then each part's own
				const FMat4 G = Compose(St.X, St.Y, St.Z, QuatYXZ(0, St.Yaw, 0), 1, 1, 1);
				for (FPart& P : St.Parts) P.M = Multiply(G, P.M);
				Built.emplace(C, BuiltStations.size());
				BuiltStations.push_back(std::move(St));
			}
		}

		// railways.js station(yaw): parts in the station's own frame (along the track is +z)
		static FStation Station(double Yaw)
		{
			FStation S;
			S.Yaw = Yaw;
			// three.js Euler(x, y, z) default order 'XYZ'; only z (the roof planes) is ever set here, so XYZ = YXZ
			auto Part = [&](int Mat, double SX, double SY, double SZ, double X, double Y, double Z, double RotZ = 0)
			{
				S.Parts.push_back({ Mat, false, Compose(X, Y, Z, QuatYXZ(0, 0, RotZ), SX, SY, SZ) });
			};
			Part(Platform, 3.2, 0.7, 16, 2.9, 0.35, 0);
			Part(Trim, 0.25, 0.08, 16, 1.35, 0.72, 0);
			Part(Brick, 4.4, 3.4, 9, 6.6, 1.7, 0);
			Part(Roof, 3.6, 0.25, 10, 5.4, 3.9, 0, 0.5);
			Part(Roof, 3.6, 0.25, 10, 7.8, 3.9, 0, -0.5);
			Part(Wood, 2.6, 0.12, 6, 3.4, 3.1, 0); // the platform canopy
			for (const double Z : { -2.6, 2.6 }) Part(Wood, 0.14, 2.4, 0.14, 2.3, 1.9, Z);
			S.Parts.push_back({ Tank, true, Compose(-3.4, 4.2, 6, FQuat(), 1, 1, 1) }); // CylinderGeometry(1.1, 1.1, 1.8, 10)
			const double Legs[4][2] = { { -4, 5.4 }, { -2.8, 5.4 }, { -4, 6.6 }, { -2.8, 6.6 } };
			for (const auto& XZ : Legs) Part(Wood, 0.16, 3.3, 0.16, XZ[0], 1.65, XZ[1]);
			return S;
		}

		bool Has(int C) const { return Track.count(C) || Stations.count(C); }
	};

	// the water tower: three.js CylinderGeometry(1.1, 1.1, 1.8, 10) (radiusTop, radiusBottom, height, radialSegments;
	// heightSegments 1, closed), vertices, normals and indices in three's order
	inline void Cylinder(std::vector<double>& Pos, std::vector<double>& Nrm, std::vector<int>& Idx, double RTop = 1.1, double RBottom = 1.1, double Height = 1.8, int Radial = 10)
	{
		const double Half = Height / 2;
		const int HeightSegs = 1;
		int Index = 0;
		std::vector<std::vector<int>> Rows;
		const double Slope = (RBottom - RTop) / Height;
		for (int Y = 0; Y <= HeightSegs; Y++)
		{
			std::vector<int> Row;
			const double V = (double)Y / HeightSegs, Radius = V * (RBottom - RTop) + RTop;
			for (int X = 0; X <= Radial; X++)
			{
				const double U = (double)X / Radial, Theta = U * 2 * Pi, S = std::sin(Theta), C = std::cos(Theta);
				Pos.insert(Pos.end(), { Radius * S, -V * Height + Half, Radius * C });
				double NX = S, NY = Slope, NZ = C;
				const double L = std::sqrt(NX * NX + NY * NY + NZ * NZ);
				Nrm.insert(Nrm.end(), { NX / L, NY / L, NZ / L });
				Row.push_back(Index++);
			}
			Rows.push_back(Row);
		}
		for (int X = 0; X < Radial; X++)
			for (int Y = 0; Y < HeightSegs; Y++)
			{
				const int A = Rows[Y][X], B = Rows[Y + 1][X], C = Rows[Y + 1][X + 1], D = Rows[Y][X + 1];
				if (RTop > 0 || Y != 0) Idx.insert(Idx.end(), { A, B, D });
				if (RBottom > 0 || Y != HeightSegs - 1) Idx.insert(Idx.end(), { B, C, D });
			}
		for (const bool Top : { true, false })
		{
			const double R = Top ? RTop : RBottom, Sign = Top ? 1 : -1;
			const int CentreStart = Index;
			for (int X = 1; X <= Radial; X++) { Pos.insert(Pos.end(), { 0, Half * Sign, 0 }); Nrm.insert(Nrm.end(), { 0, Sign, 0 }); Index++; }
			const int CentreEnd = Index;
			for (int X = 0; X <= Radial; X++)
			{
				const double U = (double)X / Radial, Theta = U * 2 * Pi;
				Pos.insert(Pos.end(), { R * std::sin(Theta), Half * Sign, R * std::cos(Theta) });
				Nrm.insert(Nrm.end(), { 0, Sign, 0 });
				Index++;
			}
			for (int X = 0; X < Radial; X++)
			{
				const int C = CentreStart + X, I = CentreEnd + X;
				if (Top) Idx.insert(Idx.end(), { I, I + 1, C });
				else Idx.insert(Idx.end(), { I + 1, I, C });
			}
		}
	}

	// three.js BoxGeometry(1, 1, 1): 6 faces of 4 vertices (px, nx, py, ny, pz, nz), two triangles each, in three's order
	inline void Box(std::vector<double>& Pos, std::vector<double>& Nrm, std::vector<int>& Idx)
	{
		// buildPlane(u, v, w, udir, vdir, width, height, depth) with axes x=0, y=1, z=2
		auto Plane = [&](int U, int V, int W, double UDir, double VDir, double Width, double Height, double Depth)
		{
			const int Start = (int)Pos.size() / 3;
			for (int IY = 0; IY < 2; IY++)
			{
				const double Y = IY * Height - Height / 2;
				for (int IX = 0; IX < 2; IX++)
				{
					const double X = IX * Width - Width / 2;
					double P[3], N[3];
					P[U] = X * UDir; P[V] = Y * VDir; P[W] = Depth / 2;
					N[U] = 0; N[V] = 0; N[W] = Depth > 0 ? 1 : -1;
					Pos.insert(Pos.end(), { P[0], P[1], P[2] });
					Nrm.insert(Nrm.end(), { N[0], N[1], N[2] });
				}
			}
			const int A = Start, B = Start + 2, C = Start + 3, D = Start + 1; // gridX1 = 2: a = ix + 2 iy, b = ix + 2 (iy + 1), ...
			Idx.insert(Idx.end(), { A, B, D, B, C, D });
		};
		Plane(2, 1, 0, -1, -1, 1, 1, 1);  // px
		Plane(2, 1, 0, 1, -1, 1, 1, -1);  // nx
		Plane(0, 2, 1, 1, 1, 1, 1, 1);    // py
		Plane(0, 2, 1, 1, -1, 1, 1, -1);  // ny
		Plane(0, 1, 2, 1, -1, 1, 1, 1);   // pz
		Plane(0, 1, 2, -1, -1, 1, 1, -1); // nz
	}
}

#ifndef TC_RAIL_CORE_ONLY

#include "CoreMinimal.h"
#include "UObject/StrongObjectPtr.h"

class FJsonObject;
class FTCMatchState;
class ATCTerrain;
class AActor;
class UWorld;
class UInstancedStaticMeshComponent;
class UMaterialInstanceDynamic;
class UMaterialInterface;
class UStaticMesh;
class UTexture2D;

// The fog of war on a map's cells as one texel each (G8, nearest): 1 in sight, 0.55 explored, 0.32 never seen, nothing
// once an authored match is decided: ATCTerrain::CellColor's dimming, the texture the world materials' FogMap reads
// (FTCWorldObjectsView::WriteFog keeps its own of the same bytes). Railways and the atmosphere each hold one.
struct THREECROSSROADS_API FTCCellFogMap
{
	// Rewrites the texels that changed; makes the texture when the map size changes. True when the texture is new.
	bool Update(const FTCMatchState& State, const ATCTerrain* Terrain);
	UTexture2D* Texture() const { return Tex.Get(); }
	FLinearColor Size() const { return FLinearColor(W * 2.f, H * 2.f, 0.f, 1.f); } // map metres (FogSize)
	void Release();
	~FTCCellFogMap() { Release(); }
private:
	TWeakObjectPtr<UTexture2D> Tex;
	TArray<uint8> Bytes;
	int32 W = 0, H = 0;
};

// The explored railway of a World Conquest match (TCRailCore for the port). The game mode holds one: Attach once (it
// spawns an actor of its own for the instanced meshes), Reset at every start, fed the start's and each snapshot's cell
// rows and world regions, Frame every frame, Detach at the end.
class THREECROSSROADS_API FTCRailways
{
public:
	bool Attach(UWorld* World);
	void Detach();

	// client/main.js startGame: the railways are dropped, then made again on a World map (State.Map.bWorld).
	void Reset(const FTCMatchState& State);
	// railways.cells(m.cells) and applyCells(s.cells): the message's 'cells' rows (data.rail, data.station).
	void ApplyCells(const FJsonObject& Message);
	// center(id): the last snapshot's world.regions (id, x, z). A snapshot without them leaves no centre known.
	void ApplySnapshot(const FJsonObject& Snapshot);
	// railways.frame(dt): rebuilds and stations at most twice a second; the fog texels every frame.
	void Frame(float Dt, const FTCMatchState& State, const ATCTerrain* Terrain);

	// props.js skip: a railway cell has no scenery prop. TrackCells: the minimap's dark railway (main.js drawMinimap).
	bool Has(int32 Cell) const { return bOn && Model.Has(Cell); }
	const std::vector<int>& TrackCells() const { return Model.TrackOrder; }
	bool IsOn() const { return bOn; }
	int32 HalfCount() const { return (int32)Model.BallastM.size(); }
	int32 SleeperCount() const { return (int32)Model.TiesM.size(); }
	int32 StationCount() const { return (int32)Model.StationOrder.size(); }
	int32 StationsBuilt() const { return (int32)Model.BuiltStations.size(); }
	int32 Rebuilds() const { return Model.Rebuilds; }
	bool StationsWaiting() const { return Model.Waiting != 0; }
	// instances drawn now (ballast + sleepers + rails + station parts), from the components
	int32 InstancesDrawn() const;
	// a built station (reference metres) for checks and shots; false when none is built
	bool FirstStation(FVector2D& OutMetres) const;
	bool TrackCentre(FVector2D& OutMetres) const;
	// The Unreal transform of a three.js instance matrix of a mesh built through TCRailCore (metres, three's axes).
	static FTransform ToUnreal(const TCRailCore::FMat4& M);
	// The unit box and the water tower as run-time static meshes (three's geometry, the axes swapped, cm).
	static UStaticMesh* BuildMesh(UObject* Outer, bool bCylinder, UMaterialInterface* Material);
	// "track 22 cells, 44 halves, ..."
	FString Describe() const;
	const TCRailCore::FModel& Core() const { return Model; }

private:
	UInstancedStaticMeshComponent* Batch(int32 Mat, bool bCylinder);
	void DrawTrack();
	void DrawStation(const TCRailCore::FStation& St);
	void ClearDrawn();

	TCRailCore::FModel Model;
	bool bOn = false;
	int32 MapCells = 0;
	TMap<int32, FVector2D> Centres;   // region id -> centre (metres), from the last snapshot
	FTCCellFogMap Fog;

	TWeakObjectPtr<AActor> Actor;
	TStrongObjectPtr<UStaticMesh> BoxMesh, CylinderMesh;
	TArray<TStrongObjectPtr<UMaterialInstanceDynamic>> Materials;            // one per TCRailCore::EMat
	TMap<int32, TWeakObjectPtr<UInstancedStaticMeshComponent>> Batches;      // (mat * 2 + cylinder) -> instances
	int32 LoggedRebuilds = 0, LoggedStations = 0;
};

#endif
