// The paper war map (client/map-view.js, CLI-39): zoomed all the way out, the battlefield turns into a staff map lying on
// a desk. Three layers fade in together over the last stretch of zoom (0.7 to 0.9 of the widest view, smoothstepped and
// eased at 8 per second) and, fully in, cover the 3D scene:
// - the paper, painted on the CPU from this client's grid exactly as map-view.js paint() draws its canvas (the same
//   seeded specks and stains, water and wood washes, ripples, tree circles, contour ink where the drawn ground steps a
//   level, roads, houses, rubble, trenches, wire, the blue grid every 10 cells and the inked border), draped over the
//   drawn ground; and the desk under it (paintDesk: dark planks with grain and the paper's shadow), flat at height 0,
//   seven map sizes across;
// - the sepia wash of client/fog.js's look (0 seen, 120/255 explored, 185/255 never seen, World's unknown ground opaque
//   with the frontier darkening), at 0.55 x the fade: (0.55, 0.40, 0.22), near black on World Conquest;
// - the screen overlay: World region borders and names, capture points (inked dashed rings filled in the holder's
//   colour, lettered), strike boxes hatched in red, this seat's orders in grease pencil, every unit held as its map
//   symbol (client/symbols.js, rasterised from the reference by unreal/Tools/warmap into /Game/WarMap) in its owner's
//   colour with a paper halo, a pencil ring round the selected ones, the map's name and a compass.
// Unreal draws all three on the HUD's canvas (after the tone mapper, so the colours are the reference's canvas colours,
// as map-view.js's toneMapped: false and its screen overlay). The paper and the wash are textured triangles projected
// from the drawn ground; the desk is a flat grid clipped at the camera.
//
// Recipient knowledge: the paper is painted from this client's own grid (State.Map), and a World Conquest cell this side
// has not discovered is plain paper: no wash colour, no ink, no contour on or next to it, and the relief under it is not
// read; its wash is the opaque near-black. Units are the rows this client holds (the server sends no unseen enemy).
#pragma once

#include "CoreMinimal.h"
#include "UObject/GCObject.h"
#include "Commandlets/Commandlet.h"
#include "TCWarMap.generated.h"

class UCanvas;
class UTexture2D;
class UFont;
class UFontFace;
class ATCTerrain;
class ATCGameMode;
class ATCPlayerController;
class FTCMatchState;
class FTCUnitTable;

// ---------------------------------------------------------------------------------------------------------------------
// The CPU canvas the paper and the desk are painted on: a port of the few Canvas2D calls map-view.js uses (fillRect,
// stroke of lines, quadratic curves and arcs with butt caps and miter joins, a radial gradient, a rectangle's blurred
// shadow), with exact-area anti-aliasing, composited source-over on 8-bit sRGB values as a browser canvas does.
namespace TCWarMapPaint
{
	// map-view.js mulberry: the small seeded random the paper and the desk are painted with
	struct FMulberry
	{
		uint32 A;
		explicit FMulberry(uint32 Seed) : A(Seed) {}
		double Next();
	};

	struct FRgba { double R = 0, G = 0, B = 0, A = 1; };
	FRgba Css(uint8 R, uint8 G, uint8 B, double A = 1.0);

	class FPaintCanvas
	{
	public:
		void Init(int32 InW, int32 InH, FRgba Clear = FRgba{ 0, 0, 0, 0 });
		int32 W = 0, H = 0;
		TArray<uint8> Rgba; // W * H * 4, row 0 at the top (map z = 0, north)

		// Path building, as Canvas2D: subpaths of points (curves and arcs are flattened finely)
		void BeginPath() { Subpaths.Reset(); }
		void MoveTo(double X, double Y);
		void LineTo(double X, double Y);
		void QuadTo(double Cx, double Cy, double X, double Y);
		void Arc(double Cx, double Cy, double R, double A0, double A1);
		void Stroke(const FRgba& Colour, double Width);
		void FillRect(double X, double Y, double RW, double RH, const FRgba& Colour);
		// fillRect with a concentric radial gradient (createRadialGradient(x, y, r0, x, y, r1)); Stops: (offset, colour)
		void FillRectRadial(double X, double Y, double RW, double RH, double Cx, double Cy, double R0, double R1, const TArray<TPair<double, FRgba>>& Stops);
		void StrokeRect(double X, double Y, double RW, double RH, const FRgba& Colour, double Width);
		// fillRect under shadowColor / shadowBlur (offsets 0): the blurred shadow first, then the rectangle itself
		void FillRectShadowed(double X, double Y, double RW, double RH, const FRgba& Fill, const FRgba& Shadow, double Blur);

	private:
		struct FPoly { TArray<FVector2D> P; };
		TArray<TArray<FVector2D>> Subpaths;
		// exact-area coverage accumulation (signed area per cell, then a running sum per row) over a bounding box
		int32 BX0 = 0, BY0 = 0, BW = 0, BH = 0;
		TArray<float> Acc;
		void Begin(double X0, double Y0, double X1, double Y1);
		void Edge(FVector2D A, FVector2D B);
		void Polygon(const TArray<FVector2D>& P);
		template <typename F> void Resolve(F&& ColourAt);
		void Blend(int32 X, int32 Y, const FRgba& C, double Coverage);
		void StrokeSubpath(const TArray<FVector2D>& Pts, double Width);
	};

	// map-view.js paint(): the paper for a w x h cell map at PX pixels a cell. Chars and Known are this client's grid
	// (an unknown cell is painted as the '.' the server would send for it, and gets no contour); Level is the drawn
	// ground's level at each cell centre (round(hAt / levelHeight)), read only for known cells.
	struct FPaperInput
	{
		int32 W = 0, H = 0;
		TArray<uint8> Chars;
		TArray<int8> Level;
		TBitArray<> Known;
	};
	int32 PixelsPerCell(int32 W, int32 H);
	void PaintPaper(const FPaperInput& In, FPaintCanvas& Out);
	// map-view.js paintDesk(c, 2048, 7, fx, fz)
	void PaintDesk(FPaintCanvas& Out, int32 Size, double Span, double Fx, double Fz);
}

// client/fog.js's look per cell, for the wash: target alpha (0 seen, 120 explored, 185 never seen; World's unknown 255
// and the frontier darkening of explored cells within 4 cells of unknown ground), the ground under every unit counted
// seen until the next snapshot, each change faded within a quarter second.
class THREECROSSROADS_API FTCWarMapFog
{
public:
	void Reset(int32 InW, int32 InH, bool bInOpaque);
	// the whole look from the state (a start or a reconnect): no fade
	void Rebuild(const FTCMatchState& S, const TMap<FString, int32>& BuildingSize, bool bLift);
	void Touch(const FTCMatchState& S, const TSet<int32>& Cells, bool bLift);
	// a new snapshot: the cells under the units it holds are lit until the next one
	void Snapshot(const FTCMatchState& S, const TMap<FString, int32>& BuildingSize, bool bLift);
	// every frame: lift (the authored match is decided) clears it all; returns true when any alpha byte changed
	bool Frame(const FTCMatchState& S, float Dt, bool bLift);
	uint8 AlphaByte(int32 C) const { return Bytes.IsValidIndex(C) ? Bytes[C] : 0; }
	bool IsOpaque() const { return bOpaque; }
	int32 Active() const { return ActiveList.Num(); }

private:
	int32 W = 0, H = 0;
	bool bOpaque = false, bLifted = false;
	TArray<uint8> Want, Lit, Edge, Busy, Bytes;
	TArray<float> Alpha;
	TArray<int32> ActiveList, LitList;
	uint8 Look(const FTCMatchState& S, int32 C, bool bLift) const;
	uint8 Frontier(const FTCMatchState& S, int32 C) const;
	void Retarget(const FTCMatchState& S, int32 C, bool bLift);
	bool Known(const FTCMatchState& S, int32 C) const;
};

class THREECROSSROADS_API FTCWarMap : public FGCObject
{
public:
	// client/map-view.js FROM, TO (shares of the widest zoom), REPAINT (seconds), the wash's share of the fade
	static constexpr float FadeFrom = 0.7f, FadeTo = 0.9f, RepaintSeconds = 3.f, WashShare = 0.55f;
	// where the fade goes for a zoom (camera distance / widest distance): smoothstep over [FROM, TO]
	static float WantFade(float Zoom);

	FTCWarMap();
	// The atlas, the font face and WarMapData.json (Content/WarMap, unreal/Tools/warmap). False with the reason.
	bool LoadAssets(FString& OutProblem);
	// A start (a new match or a reconnect): the map size, the title; repainted the next time it shows.
	void Start(const FTCMatchState& S);
	void Clear();
	// map-view.js refresh(): the terrain changed (craters, wrecked houses): repainted the next time the map shows, at most
	// every RepaintSeconds while it is up
	void MarkTerrain() { bDirty = true; }
	void MarkFog(const FTCMatchState& S, const TSet<int32>& Cells);
	// map-view.js frame(dt, zoom); Lift: the authored match is decided and the fog is lifted
	void Frame(float Dt, float Zoom, const FTCMatchState& S, const ATCTerrain* Terrain, bool bLift);
	// 0 (3D) to 1 (the map): the HUD fades its world labels with it (markers.js fadeLabels(1 - min(1, fade * 2)))
	float Fade() const { return FadeNow; }
	float LabelFade() const { return 1.f - FMath::Min(1.f, FadeNow * 2.f); }
	// Draws the desk, the paper, the wash and the overlay on the HUD's canvas, when the fade is up. Call after the HUD's
	// own world overlays and before its panels.
	void Draw(UCanvas* Canvas, const ATCGameMode& M, const ATCPlayerController& PC);
	FString StatusLine() const;
	int32 Paints() const { return PaintCount; }
	const TCWarMapPaint::FPaintCanvas& PaperCanvas() const { return Paper; }

	virtual void AddReferencedObjects(FReferenceCollector& Collector) override;
	virtual FString GetReferencerName() const override { return TEXT("FTCWarMap"); }

	// the map-view.js colours
	static FColor PaperColour() { return FColor(0xe8, 0xde, 0xc3); }
	static FColor InkColour() { return FColor(0x3b, 0x32, 0x26); }

private:
	struct FAtlas { int32 Cell = 128, Columns = 16, Rows = 0, Box = 120, HaloRowsOffset = 0; TMap<FString, int32> Index; };
	FAtlas Atlas;
	TMap<FString, int32> BuildingSize;
	double PointRadius = 8.0, LevelHeight = 2.5, CellMetres = 2.0;
	bool bAssets = false, bAssetsTried = false;
	FString AssetProblem;
	TObjectPtr<UTexture2D> SymbolTexture = nullptr;
	TObjectPtr<UFontFace> FontFace = nullptr;
	TObjectPtr<UFont> MapFont = nullptr;
	TObjectPtr<UTexture2D> PaperTexture = nullptr, DeskTexture = nullptr, WashTexture = nullptr;

	int32 W = 0, H = 0;
	FString Title;
	bool bWorld = false;
	float FadeNow = 0.f;
	bool bDirty = true;
	double SincePaint = 1e30;
	int32 PaintCount = 0, LastSnapshots = -1, WashUploads = 0;
	double LastPaintMs = 0;
	TCWarMapPaint::FPaintCanvas Paper, Desk;
	FTCWarMapFog Fog;
	bool bWashDirty = true, bFogBuilt = false, bLastLift = false;
	FVector2D DeskSize = FVector2D::ZeroVector;

	// -TCWarMapTour=<file> -TCWarMapShotDir=<dir>: this scope's evidence tour (the -TCTourFile format: name, match
	// seconds, x, z, metres or wF (a share of the widest view at the pose's own heading), native yaw). At each pose the
	// camera is set, and 2.5 s later a screenshot WITH the HUD (the paper is drawn on the HUD's canvas) is saved as
	// <dir>/warmap-<name>.png, with the zoom, fade, widest view and overlay counts in the log.
	struct FTourPose { FString Name; double At = 0, X = 0, Z = 0, Dist = 0, Yaw = -90; bool bShare = false; };
	TArray<FTourPose> Tour;
	FString TourFile, ShotDir;
	bool bTourRead = false;
	int32 TourStep = 0, TourPhase = 0;
	double TourClock = 0, TourShotAt = -1;
	int32 LastDrawnUnits = 0, LastDrawnPoints = 0, LastDrawnPencil = 0;
	void TourTick(float Dt, float Zoom, const FTCMatchState& S, const ATCTerrain* Terrain);
	// the paper is painted on a worker thread; the texture is made from it on the game thread when it is done
	TSharedPtr<TCWarMapPaint::FPaintCanvas, ESPMode::ThreadSafe> PaintJob;
	TSharedPtr<TAtomic<bool>, ESPMode::ThreadSafe> PaintDone;
	double PaintStartedAt = 0;
	int32 UnknownAtPaint = 0;
	void Paint(const FTCMatchState& S, const ATCTerrain* Terrain);
	void FinishPaint();
	void PaintDeskOnce();
	void UploadWash();
	struct FLevels { TArray<TArray<uint8>> Bytes; TArray<FIntPoint> Sizes; };
	static void BuildLevels(const TCWarMapPaint::FPaintCanvas& C, bool bMips, bool bSrgb, FLevels& Out);
	static UTexture2D* TextureFromLevels(const FLevels& L, bool bSrgb);
	TSharedPtr<FLevels, ESPMode::ThreadSafe> PaintLevels;
	static UTexture2D* MakeTexture(const TCWarMapPaint::FPaintCanvas& C, bool bMips, bool bSrgb, UTexture2D* Reuse);
	UFont* Font();
};

// -run=TCWarMapPaint -Map=<reference maps/*.json> -Out=<dir>: paints the map's paper and desk with the port above from
// the map file's rows and levels (hAt = the cell's level x 2.5 m, as unreal/Tools/warmap/make-warmap.mjs oracle feeds
// map-view.js), saved as <dir>/<name>-paper.png and <name>-desk.png, for the comparison with the reference's canvases.
UCLASS()
class THREECROSSROADS_API UTCWarMapPaintCommandlet : public UCommandlet
{
	GENERATED_BODY()
public:
	UTCWarMapPaintCommandlet();
	virtual int32 Main(const FString& Params) override;
};
