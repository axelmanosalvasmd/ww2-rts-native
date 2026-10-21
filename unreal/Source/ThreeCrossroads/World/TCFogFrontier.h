// The fog of war's state as the pinned reference keeps it (ReferencePin), client/fog.js line for line: each cell is
// seen now (clear), explored (dim, alpha 120) or never seen (dark, alpha 185; 255 on World Conquest, whose unknown
// ground is opaque), and fades to a new look within a quarter second. Two parts the earlier slices did not port:
//   - the frontier (fog.js FRONTIER, frontier(), edge[]): on World Conquest the ground this side knows darkens over 4
//     cells (8 m) toward the nearest cell nobody has seen, smoothstepped, so the frontier reads as a soft curve and not
//     a staircase of 2 m cells; it is redone around every newly explored cell;
//   - the lit cells (fog.js snapshot(), lit[]): the ground under every unit a snapshot delivers counts as seen until
//     the next snapshot (a building's whole footprint, round(x / CELL - size / 2); aircraft and unknown types light
//     nothing), because the server's mask follows its 5 Hz vision pass and a unit can step onto a cell first.
// Fog lifting (fog.js frame(dt, lift)): a match decided on an authored map clears every cell; a client without a fog
// mask (a spectator of an authored map, CLI-90) is lifted from the start; World stays fogged.
//
// The engine-free part (namespace TCFogFrontierCore) compiles alone with TC_FOG_FRONTIER_CORE_ONLY defined; the
// standalone harness and ThreeCrossroads.World.FogFrontier hold it to a Node oracle that runs the unmodified fog.js
// (TCFogFrontier.Oracle.inl). The engine part (FTCFogFrontier) feeds it from FTCMatchState and draws what the
// reference's overlay draws, as a raw G8 alpha texture with LinearFilter and the source mist shader. The legacy
// multiplier texture remains for native consumers outside this renderer. On World Conquest it also draws unknown ground
// itself: an opaque plane at public level 0 using the terrain's placeholder paint beneath the filtered overlay.
// The fog shade is used only when the terrain material is unavailable.
//
// Recipient knowledge: everything here comes from what the server sent this client (its fog mask and the units its
// snapshot holds). The unknown ground is a flat plane at level 0 over every cell this client does not know: it shows
// nothing the start map does not already say.
#pragma once

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <vector>

namespace TCFogFrontierCore
{
	inline constexpr int Clear = 0, Dim = 120, Dark = 185, Opaque = 255; // overlay alpha for seen, explored and never seen
	inline constexpr double Speed = Dark / 0.25; // alpha per second: any change of look finishes within a quarter second
	inline constexpr int Shade = 10;             // the overlay's colour (near black)
	inline constexpr int Frontier = 4;           // World: cells over which explored ground darkens toward unseen ground
	inline constexpr double Cell = 2.0;          // shared/sim.js CELL
	// relief.js planRow merges at most 32 flat cells. Keep the public placeholder
	// on that same lattice: a map-wide, two-metre-thin triangle gives the separately
	// drawn fog an unnecessarily ill-conditioned coplanar depth comparison.
	inline constexpr int PlaceholderRunCells = 32;
	template <typename KnownFn, typename EmitFn>
	void UnknownRuns(int W, int H, KnownFn Known, EmitFn Emit)
	{
		for (int Y = 0; Y < H; Y++) for (int X = 0; X < W;)
		{
			if (Known(Y * W + X)) { X++; continue; }
			int End = X + 1;
			while (End < W && End - X < PlaceholderRunCells && !Known(Y * W + End)) End++;
			Emit(X, End, Y);
			X = End;
		}
	}
	inline double JsRound(double V) { return std::floor(V + 0.5); }
	// surfaces.js MIST and fogOverlayShader, in reference metres. Texture coordinates in the native
	// upload run north to south, so x / (w * CELL), z / (h * CELL) samples the same texel as three.
	inline double Saturate(double V) { return std::min(1.0, std::max(0.0, V)); }
	inline double Fract(double V) { return V - std::floor(V); }
	inline double Mix(double A, double B, double T) { return A + (B - A) * T; }
	// fog.js frontier(), not a polygon contour. LinearFilter interpolates these cell-centre alphas.
	inline int FrontierAlpha(int DistanceSquared)
	{
		if (DistanceSquared < 0) return 0;
		const double T = std::min(1.0, (std::sqrt((double)DistanceSquared) - 0.5) / Frontier);
		return (int)JsRound(255 * (1 - T * T * (3 - 2 * T)));
	}
	inline double MistHash(double X, double Z)
	{
		X = Fract(X * 0.1031); Z = Fract(Z * 0.1030);
		const double D = X * (Z + 33.33) + Z * (X + 33.33);
		X += D; Z += D;
		return Fract((X + Z) * X);
	}
	inline double MistNoise(double X, double Z)
	{
		const double I = std::floor(X), J = std::floor(Z), U = Fract(X), V = Fract(Z);
		const double S = U * U * (3 - 2 * U), T = V * V * (3 - 2 * V);
		return Mix(Mix(MistHash(I, J), MistHash(I + 1, J), S), Mix(MistHash(I, J + 1), MistHash(I + 1, J + 1), S), T);
	}
	inline double Mist(double Alpha, double X, double Z, double Time)
	{
		const double M = MistNoise(X * .05 + Time * .04, Z * .05 + Time * .015) * .65
			+ MistNoise(X * .13 - Time * .02, Z * .13 - Time * .05) * .35;
		return Saturate(Alpha + (M - .5) * 1.4 * Alpha * (1 - Alpha));
	}
	inline double OuterAlpha(double Alpha, double Distance)
	{
		const double T = Saturate((Distance - 6) / (32 - 6));
		return Mix(Alpha, Dark / 255.0, T * T * (3 - 2 * T));
	}
	// The overlay blends after lighting in linear space. Objects use surfaces.js's literal 0.0392.
	inline double Composite(double Lit, double Alpha, bool bPiece = false)
	{
		return Mix(Lit, bPiece ? .0392 : Shade / 255.0, Alpha);
	}
	inline double SampleAlpha(const uint8_t* Bytes, int W, int H, double X, double Z)
	{
		if (!Bytes || W <= 0 || H <= 0) return 0;
		// LinearFilter + ClampToEdge: integer texel centres at half a cell, no nearest-cell lookup.
		const double U = X / Cell - .5, V = Z / Cell - .5;
		const int I = (int)std::floor(U), J = (int)std::floor(V);
		auto At = [&](int A, int B) { return Bytes[std::clamp(B, 0, H - 1) * W + std::clamp(A, 0, W - 1)] / 255.0; };
		return Mix(Mix(At(I, J), At(I + 1, J), Fract(U)), Mix(At(I, J + 1), At(I + 1, J + 1), Fract(U)), Fract(V));
	}

	// fog.js snapshot: the cells a unit lights, x0 / y0 its footprint's first cell and Size its side (1 for anything
	// but a building). Air units and types the client does not know are not passed in.
	struct FFootprint { int X0 = 0, Y0 = 0, Size = 1; };
	inline FFootprint Footprint(double X, double Z, int Size)
	{
		FFootprint F;
		F.Size = Size;
		F.X0 = Size > 1 ? (int)JsRound(X / Cell - Size / 2.0) : (int)std::floor(X / Cell);
		F.Y0 = Size > 1 ? (int)JsRound(Z / Cell - Size / 2.0) : (int)std::floor(Z / Cell);
		return F;
	}

	// The building footprints of the pinned shared/sim.js UNITS (def.building ? def.size : 1); checked against the
	// reference by the oracle's meta (TCFogFrontier.Oracle.inl) in ThreeCrossroads.World.FogFrontier.
	struct FBuildingSize { const char* Type; int Size; };
	inline constexpr FBuildingSize BuildingSizes[] = {
		{ "hq", 3 }, { "depot", 2 }, { "barracks", 3 }, { "motorpool", 3 }, { "airfield", 3 }, { "shipyard", 3 }, { "flakpos", 2 },
		{ "worldbase", 3 }, { "armory", 3 }, { "pillbox", 2 }, { "tower", 1 }, { "wall", 1 }, { "gate", 1 }, { "supplycache", 2 } };
	// a building's side in cells, 0 when the type is not a building of the table
	inline int BuildingSize(const char* Type)
	{
		for (const FBuildingSize& B : BuildingSizes) { const char *A = B.Type, *T = Type; while (*A && *A == *T) { A++; T++; } if (!*A && !*T) return B.Size; }
		return 0;
	}

	// The reference's overlay alpha (0..255, near black over the lit ground) as the multiplier the native consumers dim
	// the lit colour by, as a byte: the three looks keep FTCCellFog's bytes exactly (0 -> 255, 120 -> 140, 185 -> 82),
	// and the World frontier's values between them are linear between those. Past 185 (World's opaque unknown and the
	// frontier's darkest cells) it runs to 17 at 255: the overlay over the ground those bytes were set for (1 - a +
	// a * 0.0392 / g = 0.32 at a = 185 / 255 gives g = 0.0392 * 255 / 12, so a = 255 leaves 12 / 185 of it).
	inline uint8_t MultiplierByte(double Alpha)
	{
		const double A = std::min(255.0, std::max(0.0, Alpha));
		double M;
		if (A <= Dim) M = 255.0 + (140.0 - 255.0) * (A / Dim);
		else if (A <= Dark) M = 140.0 + (82.0 - 140.0) * ((A - Dim) / (Dark - Dim));
		else M = 82.0 + (17.0 - 82.0) * ((A - Dark) / (Opaque - Dark));
		return (uint8_t)JsRound(M);
	}

	// One fog of war (fog.js createFog's closure without the texture upload).
	class FFog
	{
	public:
		int W = 0, H = 0, N = 0, DarkAlpha = Dark;
		bool bOpaque = false, bOff = false, bHasKey = false, bLifted = false;
		std::vector<uint8_t> Seen, Explored, Lit, Want, Busy, Edge, Bytes; // Bytes: the texture's alpha, Math.round(alpha)
		std::vector<float> Alpha;                                          // fog.js's Float32Array
		std::vector<uint32_t> Mark;
		std::vector<int> LitList, ActiveList, Grown;
		uint32_t Stamp = 0, Version = 0;
		// cells whose target look changed since the last TakeChanged (what other fog consumers must refresh)
		std::vector<int> Changed;
		std::vector<uint8_t> ChangedMark;

		// fog.js createFog(w, h, key, off, opaque). SeenNow / Explored: the start's mask (null: no mask, key absent).
		void Start(int InW, int InH, const uint8_t* SeenNow, const uint8_t* EverSeen, bool bInOff, bool bInOpaque)
		{
			W = InW; H = InH; N = std::max(0, W * H);
			bOpaque = bInOpaque; bOff = bInOff; bHasKey = SeenNow != nullptr;
			DarkAlpha = bOpaque ? Opaque : Dark;
			Seen.assign(N, 0); Explored.assign(N, 0); Lit.assign(N, 0); Want.assign(N, 0); Busy.assign(N, 0); Edge.assign(N, 0); Bytes.assign(N, 0);
			Alpha.assign(N, 0.f); Mark.assign(N, 0); LitList.clear(); ActiveList.clear(); Grown.clear(); Stamp = 0;
			Changed.clear(); ChangedMark.assign(N, 0);
			// without a mask (an older server) nothing is hidden on the ground
			bLifted = bOff || (!bHasKey && !bOpaque);
			if (bHasKey) for (int C = 0; C < N; C++) { Seen[C] = SeenNow[C] ? 1 : 0; Explored[C] = EverSeen && EverSeen[C] ? 1 : 0; }
			if (bOpaque) for (int C = 0; C < N; C++) Edge[C] = (uint8_t)FrontierOf(C);
			for (int C = 0; C < N; C++) { const int L = Look(C); Bytes[C] = (uint8_t)L; Alpha[C] = (float)L; Want[C] = (uint8_t)L; }
			Version++;
		}

		// what the cell is: seen now, explored or never seen (the minimap and gameplay checks read this)
		int Base(int C) const { return bLifted || Seen[C] || Lit[C] ? Clear : Explored[C] ? Dim : DarkAlpha; }
		int Look(int C) const { return bOpaque && !bLifted ? std::max(Base(C), (int)Edge[C]) : Base(C); }
		bool Known(int C) const { return Seen[C] || Explored[C]; }
		// full darkness next to the nearest never-seen cell, none FRONTIER cells in, smoothstepped between
		int FrontierOf(int C) const
		{
			if (!Known(C)) return 0;
			const int X = C % W, Y = C / W;
			int Best = -1;
			for (int J = std::max(0, Y - Frontier); J <= std::min(H - 1, Y + Frontier); J++)
				for (int I = std::max(0, X - Frontier); I <= std::min(W - 1, X + Frontier); I++)
					if (!Known(J * W + I)) { const int D = (I - X) * (I - X) + (J - Y) * (J - Y); if (Best < 0 || D < Best) Best = D; }
			return FrontierAlpha(Best);
		}
		void Touch(int C)
		{
			const int T = Look(C);
			if (T == Want[C]) return;
			Want[C] = (uint8_t)T; Version++;
			if (!ChangedMark[C]) { ChangedMark[C] = 1; Changed.push_back(C); }
			if (!Busy[C]) { Busy[C] = 1; ActiveList.push_back(C); }
		}

		// fog.js state.snapshot: Flips, the cells whose seen bit flipped (s.fog's runs); Units, the footprints of the
		// snapshot's ground units (s.units).
		void Snapshot(const std::vector<int>& Flips, const std::vector<FFootprint>& Units)
		{
			for (const int C : Flips)
			{
				if (C < 0 || C >= N) continue;
				Seen[C] ^= 1;
				if (Seen[C] && !Explored[C]) Grown.push_back(C);
				Explored[C] |= Seen[C];
				Touch(C);
			}
			// newly explored ground moves the frontier: redo the darkness of every cell within reach of it
			if (bOpaque && !Grown.empty())
			{
				Stamp++;
				for (const int C : Grown)
				{
					const int X = C % W, Y = C / W;
					for (int J = std::max(0, Y - Frontier); J <= std::min(H - 1, Y + Frontier); J++)
						for (int I = std::max(0, X - Frontier); I <= std::min(W - 1, X + Frontier); I++)
						{
							const int O = J * W + I;
							if (Mark[O] == Stamp) continue;
							Mark[O] = Stamp; Edge[O] = (uint8_t)FrontierOf(O); Touch(O);
						}
				}
				Grown.clear();
			}
			std::vector<int> Old;
			Old.swap(LitList);
			for (const int C : Old) Lit[C] = 0;
			for (const FFootprint& U : Units)
			{
				for (int Y = std::max(0, U.Y0); Y < std::min(H, U.Y0 + U.Size); Y++)
					for (int X = std::max(0, U.X0); X < std::min(W, U.X0 + U.Size); X++)
					{
						const int C = Y * W + X;
						if (!Lit[C]) { Lit[C] = 1; LitList.push_back(C); }
					}
			}
			for (const int C : Old) Touch(C);
			for (const int C : LitList) Touch(C);
		}

		// fog.js state.frame: Lift (the match is decided) clears the whole map; cells still changing fade one step.
		// Calls Moved(c) for every cell whose texture byte changed. True when any did.
		template <typename MovedFn>
		bool Frame(double Dt, bool bLift, MovedFn Moved)
		{
			if (!bOff && bHasKey && bLift != bLifted) { bLifted = bLift; for (int C = 0; C < N; C++) Touch(C); }
			if (ActiveList.empty()) return false;
			const double StepBy = Speed * Dt;
			bool bAny = false;
			for (int I = (int)ActiveList.size() - 1; I >= 0; I--)
			{
				const int C = ActiveList[I];
				const double T = Want[C], A = Alpha[C], Next = A < T ? std::min(T, A + StepBy) : std::max(T, A - StepBy);
				Alpha[C] = (float)Next;
				const uint8_t V = (uint8_t)JsRound(Next);
				if (Bytes[C] != V) { Bytes[C] = V; Moved(C); bAny = true; }
				if (Next == T) { Busy[C] = 0; ActiveList[I] = ActiveList.back(); ActiveList.pop_back(); }
			}
			return bAny;
		}
		bool Frame(double Dt, bool bLift) { return Frame(Dt, bLift, [](int) {}); }

		// fog.js state.at, by cell: 0 seen, 1 explored, 2 dark
		int At(int C) const { const int T = Base(C); return T == Clear ? 0 : T == Dim ? 1 : 2; }
		// fog.js state.minimap: the minimap fog's alpha for a cell
		int MinimapAlpha(int C) const { const int T = Base(C); return T == Clear ? 0 : T == Dim ? 110 : bOpaque ? 255 : 175; }
		// the cells whose look changed since the last call
		std::vector<int> TakeChanged()
		{
			std::vector<int> Out;
			Out.swap(Changed);
			for (const int C : Out) ChangedMark[C] = 0;
			return Out;
		}
	};
}

#ifndef TC_FOG_FRONTIER_CORE_ONLY

#include "CoreMinimal.h"
#include "UObject/StrongObjectPtr.h"

class AActor;
class UWorld;
class UTexture2D;
class UProceduralMeshComponent;
class UMaterialInstanceDynamic;
class UMaterialInterface;
class UMaterialParameterCollection;
class FTCMatchState;
class FTCUnitTable;

// main.js fogMesh / apron.js fogMesh: the same ground geometry, translucent, drawn after water.
// A tiny normal offset substitutes for WebGL's polygonOffset without changing picking or relief heights.
class THREECROSSROADS_API FTCFogOverlay
{
public:
	FTCFogOverlay();
	~FTCFogOverlay();
	void Attach(UWorld* World);
	void Detach();
	void Clear();
	void ClearSection(int32 Section);
	void SetSection(int32 Section, const TArray<FVector>& Positions, const TArray<int32>& Indices, const TArray<FVector>& Normals);
	void Flush(const FTCMatchState& State, bool bOuter = false);
	bool Ready() const { return Mid.IsValid(); }
	UProceduralMeshComponent* MeshComponent() const { return Mesh.Get(); }
	static const TCHAR* const MaterialPath;
private:
	TWeakObjectPtr<AActor> Actor;
	TStrongObjectPtr<UProceduralMeshComponent> Mesh;
	TStrongObjectPtr<UMaterialInstanceDynamic> Mid;
};

// The fog of war for this client, fed from FTCMatchState: Start at each start, Snapshot after each snapshot was taken
// (before FogDirtyCells is reset), Frame every frame. Other fog consumers read Look / MultiplierByte (the target look)
// and add TakeChanged() to the cells they refresh; materials can read Texture() (the faded multiplier, one texel per
// cell, row 0 north, bilinear, clamped: the FTCCellFog layout).
class THREECROSSROADS_API FTCFogFrontier
{
public:
	static const TCHAR* const ReferencePin;
	static const TCHAR* const UnknownMaterialPath;

	FTCFogFrontier();
	~FTCFogFrontier();
	FTCFogFrontier(const FTCFogFrontier&) = delete;
	FTCFogFrontier& operator=(const FTCFogFrontier&) = delete;

	// Spawns the holder of the unknown ground (World Conquest only draws it). Without it the state still runs.
	void Attach(UWorld* World);
	void Detach();
	// A start: the map's size, the start's mask (none when State.bFogLifted: a spectator of an authored map), World's
	// opaque unknown ground. bOff: no fog at all.
	void Start(const FTCMatchState& State, bool bOff = false);
	// A snapshot was taken: the cells whose seen bit flipped (FogDirtyCells, compared with what this fog holds) and the
	// units the snapshot delivered (State.Units; aircraft and unknown types light nothing).
	void Snapshot(const FTCMatchState& State, const TSet<int32>& FogDirty, const FTCUnitTable& Table);
	// Every frame: lift once an authored-map match is decided (fog.js frame(dt, lift)); the texture follows.
	void Frame(float DeltaSeconds, bool bLift, const FTCMatchState& State);
	// No match.
	void Clear();

	bool HasMap() const { return Core.N > 0; }
	int32 MapW() const { return Core.W; }
	int32 MapH() const { return Core.H; }
	// the target look (overlay alpha 0..255) and the same as the native multiplier byte
	int32 Look(int32 C) const { return C >= 0 && C < Core.N ? (int32)Core.Want[C] : 0; }
	uint8 MultiplierByte(int32 C) const { return TCFogFrontierCore::MultiplierByte(Look(C)); }
	// the faded alpha now
	int32 AlphaNow(int32 C) const { return C >= 0 && C < Core.N ? (int32)Core.Bytes[C] : 0; }
	bool IsLit(int32 C) const { return C >= 0 && C < Core.N && Core.Lit[C]; }
	int32 FrontierDarkness(int32 C) const { return C >= 0 && C < Core.N ? (int32)Core.Edge[C] : 0; }
	int32 MinimapAlpha(int32 C) const { return C >= 0 && C < Core.N ? Core.MinimapAlpha(C) : 0; }
	bool Lifted() const { return Core.bLifted; }
	bool Opaque() const { return Core.bOpaque; }
	TSet<int32> TakeChanged();
	UTexture2D* Texture() const { return Tex.Get(); }
	// The exact fog.js alpha bytes. Keep Texture() for legacy multiplier consumers outside this scope.
	UTexture2D* AlphaTexture() const { return AlphaTex.Get(); }
	double PresentationTime() const { return FixedTime >= 0 ? FixedTime : Clock; }
	int32 LitCells() const { return Core.LitList.size(); }
	int32 FrontierCells() const;
	int32 UnknownCovered() const { return CoverCells; }
	FString StatusLine() const;
	// for tests
	const TCFogFrontierCore::FFog& CoreState() const { return Core; }
#if WITH_DEV_AUTOMATION_TESTS
	UProceduralMeshComponent* UnknownOverlayForTests() const { return CoverOverlay.MeshComponent(); }
	UProceduralMeshComponent* UnknownGroundForTests() const { return CoverMesh.Get(); }
#endif

	// The unknown ground (World Conquest): drawn when true and a holder is attached.
	bool bDrawUnknown = true;

private:
	TCFogFrontierCore::FFog Core;
	TStrongObjectPtr<UTexture2D> Tex;
	TStrongObjectPtr<UTexture2D> AlphaTex;
	TStrongObjectPtr<UMaterialParameterCollection> ClockCollection;
	double Clock = 0, FixedTime = -1;
	TArray<uint8> TexBytes;
	int32 Rx0 = 0, Ry0 = 0, Rx1 = -1, Ry1 = -1;
	TWeakObjectPtr<AActor> Actor;
	TStrongObjectPtr<UProceduralMeshComponent> CoverMesh;
	TStrongObjectPtr<UMaterialInstanceDynamic> CoverMid;
	FTCFogOverlay CoverOverlay;
	TBitArray<> CoverKnown;
	bool bCoverDirty = false;
	int32 CoverCells = 0, Snapshots = 0;

	void MakeTexture();
	void Moved(int32 C);
	void Upload(bool bAll);
	void RebuildCover(const FTCMatchState& State);
};

// The look every native fog consumer dims by (the ground paint, the map pieces, water, apron, railways, atmosphere,
// the placeholder ground): the active FTCFogFrontier's when one holds this state's map (World's frontier darkening and
// the lit cells included), else the decoded mask's three looks exactly as before. bReveal: the consumer's own "the
// match is decided" flag (every cell clear).
namespace TCFogLook
{
	// Uncooked editor games must finish generated surfaces before drawing the first match frame.
	THREECROSSROADS_API void PrepareMaterial(UMaterialInterface* Material);
	// the game's frontier (one fog per client); null: the three looks from FTCMatchState
	THREECROSSROADS_API void SetActive(const FTCFogFrontier* Frontier);
	THREECROSSROADS_API const FTCFogFrontier* Active();
	// 255 seen (or lit), 140 explored, 82 never seen, and between them on World's frontier (FTCCellFog's bytes)
	THREECROSSROADS_API uint8 Byte(const FTCMatchState& State, bool bReveal, int32 C);
	// Raw fog.js alpha, including the in-progress fade. New materials sample this, never Byte's legacy multiplier.
	THREECROSSROADS_API uint8 AlphaByte(const FTCMatchState& State, bool bReveal, int32 C);
	// the same as a multiplier: exactly 1, 0.55 and 0.32 for the three looks (ATCTerrain, FTCGroundPaint), the byte
	// over 255 between them
	THREECROSSROADS_API float Multiplier(const FTCMatchState& State, bool bReveal, int32 C);
}

#endif // TC_FOG_FRONTIER_CORE_ONLY
