// The minimap: a port of the reference browser's (client/main.js drawMinimap, mmTerrain, mmBasis, mmToWorld,
// minimapCursor and the minimap mouse handlers, client/fog.js state.minimap). The map as this client was sent it,
// turned with the camera so "up" on the minimap is "up" on the screen, scaled so the whole map fits at any heading.
// Ground is coloured per terrain character and brightened 12% per level, then darkened by the fog: clear where this
// side sees now (or a held unit stands), dimmed where it explored, dark where it never looked (opaque on World maps).
// The held units are dots in their seat's colour (selected ones ringed white), and the camera's view is outlined: the
// ground under the window's four corners, the relief where the ray hits it, else the flat y = 0 plane (camera.js
// groundAt), so the tilted camera's footprint shows as the browser's trapezoid even where the window reaches past the
// map edge (round 17).
// A left press or drag on it points the camera there (through the camera-target hook); a right press is an order at
// that ground (FTCCursor, for the controller's IssueOrder).
//
// What it may show is only what the client holds: FTCMatchState's map, fog and unit rows. A World Conquest cell this
// side never discovered (FTCMap::Known false) is drawn as the fog's shade whatever character and level the server's
// placeholder holds for it; those values are never read for drawing. A new match, room, seat or map size rebuilds the
// whole image before it is drawn again; Reset (or a state with no match) drops the image, so nothing of an earlier
// match lingers.
//
// Capture points, spawns, strikes, World regions and stations, pings, alert pings and the logistics overlay are drawn
// over this by ATCHUD (DrawMinimapOverlays) through WorldToPixel; the view outline goes last when the HUD sets
// bDeferViewOutline and calls DrawViewOutline after them (the browser strokes it last).
//
// The first part (TCMinimapCore) is plain C++ with no engine types, so a standalone harness can check its numbers by
// defining TC_MINIMAP_CORE_ONLY before including this header.
#pragma once

#include <cmath>

namespace TCMinimapCore
{
	inline constexpr double CellMetres = 2.0;     // shared/sim.js CELL
	inline constexpr int Shade = 10;              // client/fog.js SHADE: the fog's colour
	// client/fog.js state.minimap: fog alpha for a cell seen now, explored, never seen (authored), never seen (World)
	inline constexpr int AlphaSeen = 0, AlphaExplored = 110, AlphaDark = 175, AlphaDarkWorld = 255;
	inline constexpr int Background[3] = { 0x15, 0x16, 0x0f }; // client/main.js drawMinimap fillStyle '#15160f'
	inline constexpr double CanvasPixels = 240.0; // the browser draws into a 240 px canvas, then CSS scales it
	inline constexpr double PickPixels = 5.0;     // minimapCursor: units within 5 canvas pixels
	inline constexpr double VehicleDotMetres = 3.5, InfantryDotMetres = 2.6; // drawMinimap unit dots (isVeh: not infantry)

	struct FRgb { int R = 0, G = 0, B = 0; };

	inline int ClampByte(double V) { return V <= 0.0 ? 0 : V >= 255.0 ? 255 : (int)std::floor(V + 0.5); }

	// client/main.js MM_COLORS; a character it does not list draws as '.'
	inline FRgb CharColour(unsigned char Ch)
	{
		switch (Ch)
		{
		case 'B': return { 150, 132, 100 };
		case 'H': return { 47, 74, 34 };
		case '#': return { 154, 149, 138 };
		case '+': return { 90, 79, 54 };
		case 'T': return { 62, 50, 34 };
		case 'W': return { 60, 93, 112 };
		case '=': return { 122, 90, 58 };
		case 'F': return { 106, 127, 122 };
		case 'R': return { 122, 114, 102 };
		case 'X': return { 96, 90, 70 };
		case 'Y': return { 84, 84, 78 };
		case 'D': return { 150, 128, 100 };
		case 'M': return { 92, 80, 58 };
		case 'N': return { 120, 96, 60 };
		case 'O': return { 38, 62, 30 };
		case 'A': return { 226, 220, 204 };
		default: return { 108, 118, 69 };
		}
	}

	// mmTerrain: the character's colour times 1 + 0.12 per level of the ground at the cell's centre (the cell's level)
	inline FRgb TerrainColour(unsigned char Ch, int Level)
	{
		const FRgb C = CharColour(Ch);
		const double K = 1.0 + Level * 0.12;
		return { ClampByte(C.R * K), ClampByte(C.G * K), ClampByte(C.B * K) };
	}

	// client/fog.js base and state.minimap: no fog once lifted or where seen (now, or lit by a held unit), dimmed where
	// explored, else dark (opaque on a World map)
	inline int FogAlpha(bool bLifted, bool bSeen, bool bExplored, bool bWorld)
	{
		if (bLifted || bSeen) return AlphaSeen;
		if (bExplored) return AlphaExplored;
		return bWorld ? AlphaDarkWorld : AlphaDark;
	}

	// The minimap pixel for one cell. An unknown cell is the fog's shade and its Ch and Level are ignored: on a World
	// map they are the server's placeholders for ground this side has not discovered.
	inline FRgb CellPixel(bool bKnown, unsigned char Ch, int Level, int Alpha)
	{
		if (!bKnown) return { Shade, Shade, Shade };
		const FRgb T = TerrainColour(Ch, Level);
		const double A = (Alpha < 0 ? 0 : Alpha > 255 ? 255 : Alpha) / 255.0;
		return { ClampByte(T.R * (1.0 - A) + Shade * A), ClampByte(T.G * (1.0 - A) + Shade * A), ClampByte(T.B * (1.0 - A) + Shade * A) };
	}

	// mmBasis and mmToWorld: the minimap square on screen and reference metres. S pixels per metre fits the map's
	// diagonal in the square, so the turned map always fits. The map's centre sits at the square's centre; screen right
	// is R and screen up is F, the camera's right and forward on the ground. The camera's Unreal yaw is -90 degrees minus
	// the browser's yaw (ATCPlayerController::StartView), which makes R = (-sin, cos) and F = (cos, sin) of the Unreal
	// yaw, the browser's (cos yaw, -sin yaw) and (-sin yaw, -cos yaw).
	struct FBasis
	{
		double OriginX = 0.0, OriginY = 0.0, Side = 0.0; // the square's top-left corner and side, in pixels
		double MapW = 0.0, MapH = 0.0;                    // the map in reference metres
		double S = 0.0;                                   // pixels per metre (0: no basis)
		double Rx = 1.0, Rz = 0.0, Fx = 0.0, Fz = -1.0;

		bool IsValid() const { return S > 0.0; }

		static FBasis Make(double InOriginX, double InOriginY, double InSide, double InMapW, double InMapH, double UnrealYawDegrees)
		{
			FBasis B;
			if (!(InSide > 0.0) || !(InMapW > 0.0) || !(InMapH > 0.0)) return B;
			B.OriginX = InOriginX; B.OriginY = InOriginY; B.Side = InSide; B.MapW = InMapW; B.MapH = InMapH;
			B.S = InSide / std::hypot(InMapW, InMapH);
			const double Yaw = UnrealYawDegrees * 3.14159265358979323846 / 180.0;
			B.Rx = -std::sin(Yaw); B.Rz = std::cos(Yaw);
			B.Fx = std::cos(Yaw); B.Fz = std::sin(Yaw);
			return B;
		}

		// Canvas pixels per browser canvas pixel (line widths and pick radii are given in the browser's 240 px canvas).
		double CanvasScale() const { return Side / CanvasPixels; }

		void ToPixel(double X, double Z, double& OutPX, double& OutPY) const
		{
			const double DX = X - MapW / 2.0, DZ = Z - MapH / 2.0;
			OutPX = OriginX + Side / 2.0 + (DX * Rx + DZ * Rz) * S;
			OutPY = OriginY + Side / 2.0 - (DX * Fx + DZ * Fz) * S;
		}

		void ToWorld(double PX, double PY, double& OutX, double& OutZ) const
		{
			const double A = (PX - OriginX - Side / 2.0) / S, B = -(PY - OriginY - Side / 2.0) / S;
			OutX = MapW / 2.0 + Rx * A + Fx * B;
			OutZ = MapH / 2.0 + Rz * A + Fz * B;
		}

		bool InSquare(double PX, double PY) const
		{
			return IsValid() && PX >= OriginX && PY >= OriginY && PX < OriginX + Side && PY < OriginY + Side;
		}
	};

	// Liang-Barsky: clips the segment to the rectangle in place. False when nothing of it is inside.
	inline bool ClipSegment(double& X0, double& Y0, double& X1, double& Y1, double MinX, double MinY, double MaxX, double MaxY)
	{
		const double DX = X1 - X0, DY = Y1 - Y0;
		const double P[4] = { -DX, DX, -DY, DY };
		const double Q[4] = { X0 - MinX, MaxX - X0, Y0 - MinY, MaxY - Y0 };
		double T0 = 0.0, T1 = 1.0;
		for (int I = 0; I < 4; I++)
		{
			if (P[I] == 0.0)
			{
				if (Q[I] < 0.0) return false;
				continue;
			}
			const double R = Q[I] / P[I];
			if (P[I] < 0.0) { if (R > T1) return false; if (R > T0) T0 = R; }
			else { if (R < T0) return false; if (R < T1) T1 = R; }
		}
		const double AX = X0 + T0 * DX, AY = Y0 + T0 * DY, BX = X0 + T1 * DX, BY = Y0 + T1 * DY;
		X0 = AX; Y0 = AY; X1 = BX; Y1 = BY;
		return true;
	}

	// ---- the view outline (round 17): client/camera.js corners() as client/main.js drawMinimap strokes it ----
	// Reference axes here: x, y up, z (metres). rig.corners maps the window's four corners, in the order top-left,
	// top-right, bottom-right, bottom-left, through camera.js groundAt: the relief under the pixel when the ray hits it,
	// else THREE.Ray.intersectPlane with the flat y = 0 plane (null when the ray runs level or points away from it).
	// Misses are dropped (.filter(Boolean)) and the outline is drawn only when all four corners are left, so a camera
	// whose window reaches past the map edge still draws its whole footprint (a trapezoid for the tilted camera), cut
	// by the minimap's square.
	inline constexpr double RefPitch = 0.95;          // client/main.js PITCH
	inline constexpr double RefVerticalFov = 42.0;    // client/main.js PerspectiveCamera(42, ...)

	// THREE.Ray.intersectPlane with the plane y = 0 (normal +y, constant 0): t = -o.y / d.y, null for t < 0 or a
	// level ray off the plane; a level ray on the plane hits at its origin.
	inline bool FlatGround(const double O[3], const double D[3], double Out[3])
	{
		double T;
		if (D[1] == 0.0)
		{
			if (O[1] != 0.0) return false;
			T = 0.0;
		}
		else
		{
			T = -O[1] / D[1];
			if (!(T >= 0.0)) return false;
		}
		Out[0] = O[0] + D[0] * T; Out[1] = O[1] + D[1] * T; Out[2] = O[2] + D[2] * T;
		return true;
	}

	// client/camera.js pose() and THREE.Raycaster.setFromCamera for the browser's camera: the eye at the target plus
	// (sin yaw * cos pitch, sin pitch, cos yaw * cos pitch) * dist, looking at the target with +y up.
	struct FRefView
	{
		double Eye[3] = { 0, 0, 0 }, Fwd[3] = { 0, 0, -1 }, Right[3] = { 1, 0, 0 }, Up[3] = { 0, 1, 0 };
		double TanX = 1.0, TanY = 1.0;

		static FRefView Pose(double X, double Y, double Z, double Yaw, double Dist, double Aspect, double Pitch = RefPitch, double VerticalFovDegrees = RefVerticalFov)
		{
			FRefView V;
			const double Hd = Dist * std::cos(Pitch);
			V.Eye[0] = X + std::sin(Yaw) * Hd; V.Eye[1] = Y + Dist * std::sin(Pitch); V.Eye[2] = Z + std::cos(Yaw) * Hd;
			// Object3D.lookAt for a camera: its +z is (eye - target), x = normalize(up x z), y = z x x
			double Zx = V.Eye[0] - X, Zy = V.Eye[1] - Y, Zz = V.Eye[2] - Z;
			const double Zl = std::sqrt(Zx * Zx + Zy * Zy + Zz * Zz);
			if (Zl > 0) { Zx /= Zl; Zy /= Zl; Zz /= Zl; }
			double Xx = Zz, Xy = 0.0, Xz = -Zx; // (0, 1, 0) x z
			const double Xl = std::sqrt(Xx * Xx + Xz * Xz);
			if (Xl > 0) { Xx /= Xl; Xz /= Xl; }
			V.Right[0] = Xx; V.Right[1] = Xy; V.Right[2] = Xz;
			V.Up[0] = Zy * Xz - Zz * Xy; V.Up[1] = Zz * Xx - Zx * Xz; V.Up[2] = Zx * Xy - Zy * Xx;
			V.Fwd[0] = -Zx; V.Fwd[1] = -Zy; V.Fwd[2] = -Zz;
			V.TanY = std::tan(VerticalFovDegrees * 3.14159265358979323846 / 360.0);
			V.TanX = V.TanY * Aspect;
			return V;
		}

		// The unit ray through window pixel (Px, Py) of a W x H window (mx / w * 2 - 1, 1 - my / h * 2)
		void Ray(double Px, double Py, double W, double H, double Out[3]) const
		{
			const double Nx = Px / W * 2.0 - 1.0, Ny = 1.0 - Py / H * 2.0;
			double L = 0.0;
			for (int I = 0; I < 3; I++) { Out[I] = Fwd[I] + Right[I] * Nx * TanX + Up[I] * Ny * TanY; L += Out[I] * Out[I]; }
			L = std::sqrt(L);
			if (L > 0) for (int I = 0; I < 3; I++) Out[I] /= L;
		}
	};

	// rig.corners: Ground(O, D, Out) is the relief pick (true with Out on a hit); a miss falls back to FlatGround.
	// Returns how many corners were found (in screen order, misses dropped); the outline is drawn only for 4.
	template <typename FGround>
	int ViewCorners(const FRefView& V, double W, double H, FGround&& Ground, double Out[4][3])
	{
		const double Px[4] = { 0.0, W, W, 0.0 }, Py[4] = { 0.0, 0.0, H, H };
		int N = 0;
		for (int I = 0; I < 4; I++)
		{
			double D[3], P[3];
			V.Ray(Px[I], Py[I], W, H, D);
			if (Ground(V.Eye, D, P) || FlatGround(V.Eye, D, P)) { Out[N][0] = P[0]; Out[N][1] = P[1]; Out[N][2] = P[2]; N++; }
		}
		return N;
	}
}

#ifndef TC_MINIMAP_CORE_ONLY

#include "CoreMinimal.h"
#include "UObject/StrongObjectPtr.h"
#include "Client/TCOrders.h"

class UCanvas;
class UTexture2D;
class FTCMatchState;
class FTCUnitTable;
class ATCTerrain;

// The camera as the minimap draws it: its heading and the ground under the window's corners (reference metres, in
// screen order top-left, top-right, bottom-right, bottom-left; the relief, else the flat y = 0 plane; a corner whose
// ray points above the horizon is left out, and the outline is drawn only with all four). Fill it with
// FTCMinimap::ViewFromCanvas in the HUD, or by hand.
struct FTCMinimapView
{
	float YawDegrees = -90.f; // the camera's Unreal yaw (-90: the browser's yaw 0, x right and rows down the screen)
	TArray<FVector2D, TInlineAllocator<4>> Corners;
};

enum class ETCMinimapButton : uint8 { Left, Right };

enum class ETCMinimapAction : uint8
{
	None,         // the press was not on the minimap (or no match is drawn): the caller handles it as usual
	CameraTarget, // left press: the camera-target hook was called with Ground (and dragging continues it)
	Order,        // right press: hand Cursor to the controller's IssueOrder (it sends nothing without own troops)
	CancelAim,    // right press while an aim (attack-move) is armed: disarm it, no order (client/main.js cancelAim)
};

// What a press on the minimap did. bConsumed: it landed on the minimap, so the world must not also see it (the
// browser's stopPropagation), whatever Action says.
struct FTCMinimapPress
{
	bool bConsumed = false;
	ETCMinimapAction Action = ETCMinimapAction::None;
	FVector2D Ground = FVector2D::ZeroVector; // the reference metres under the press, clamped to the map
	FTCCursor Cursor;                         // Order: the ground and the nearest enemy, friend and building there
};

// Counts from the last image refresh and draw, for logs and verification.
struct FTCMinimapStats
{
	int32 W = 0, H = 0;
	int32 KnownCells = 0;    // cells drawn from their terrain (every cell of an authored map, discovered World cells)
	int32 HiddenCells = 0;   // cells drawn as the fog's shade because this side does not know them
	int32 ClearCells = 0, DimCells = 0, DarkCells = 0; // known cells by fog look
	int32 LitCells = 0;      // cells clear only because a held unit stands on them
	int32 Refreshes = 0, TextureBuilds = 0, RegionUploads = 0, RowsUploaded = 0;
	int32 UnitsDrawn = 0, SelectedDrawn = 0;
	bool bFogLifted = false, bViewDrawn = false;
};

class THREECROSSROADS_API FTCMinimap
{
public:
	FTCMinimap();
	~FTCMinimap();
	FTCMinimap(const FTCMinimap&) = delete;
	FTCMinimap& operator=(const FTCMinimap&) = delete;

	// Layout, as client/index.html: a square in the bottom-right corner, Gap pixels in, 184 px across, 228 px once
	// the window is at least 1600 x 860. A positive SideOverride replaces that size (clamped to fit the window).
	float Gap = 8.f;
	float UiScale = 1.f; // CSS zoom; the input basis remains in physical viewport pixels
	float SideOverride = 0.f;
	// The image is rebuilt from the state at most this often (client/main.js redraws the minimap every 0.15 s); a new
	// match or map rebuilds it at once. Units and the view outline are drawn from the state every frame.
	double RefreshSeconds = 0.15;
	// Draw leaves the view outline out; the caller draws it with DrawViewOutline after its own overlays (the browser
	// strokes the outline last, over points, pings and the logistics layer).
	bool bDeferViewOutline = false;

	// Called with reference metres (clamped to the map) when a left press or drag on the minimap points the camera
	// there. Bind it to ATCPlayerController::FocusOn. Without a hook, read FTCMinimapPress::Ground and MouseMove's out
	// value instead.
	void SetCameraTargetHook(TFunction<void(FVector2D)> Hook) { CameraTargetHook = MoveTemp(Hook); }

	// Drops the image, the texture, the drag and the layout, so nothing of the match drawn so far stays. Draw calls it
	// itself when the state holds no match; call it too when the room goes back to the lobby.
	void Reset();

	// Draws the minimap on the HUD's canvas. Returns false (and draws nothing) when there is no match to draw.
	bool Draw(UCanvas* Canvas, const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCMinimapView& View);
	// The camera's view outline over the minimap as last arranged (Draw calls it unless bDeferViewOutline). False when
	// nothing of it was drawn (no layout, fewer than four corners, or all of it outside the square).
	bool DrawViewOutline(UCanvas* Canvas, const FTCMinimapView& View);

	// Rebuilds the image from the state now (Draw does this on its timer). bForce: even when the timer has not run out.
	// The image is always computed; the GPU texture only when this process can render (none under -nullrhi).
	void Refresh(const FTCMatchState& State, const FTCUnitTable& Table, bool bForce = true);

	// Lays the minimap out in a ClipX x ClipY viewport and sets the frame that input reads (pixel <-> ground) for the
	// camera's Unreal yaw, without drawing. Draw does this itself each frame. Needs the state's image (Refresh or Draw);
	// false when there is none or the viewport is too small.
	bool Arrange(float ClipX, float ClipY, const FTCMatchState& State, float YawDegrees);

	// The camera's heading and the ground under the screen's corners, from the canvas the HUD is drawing (its scene
	// view): the terrain's pick (only ground this client knows is hit), else the flat plane at height 0, as camera.js
	// groundAt falls back. Terrain may be null (the plane only). False when the canvas has no view.
	bool ViewFromCanvas(const UCanvas* Canvas, const ATCTerrain* Terrain, FTCMinimapView& Out);
	// camera.js groundAt for one ray (Unreal centimetres and direction): the terrain's pick, else the flat plane at
	// height 0 (THREE.Ray.intersectPlane). False only for a ray running level or pointing away from the plane.
	static bool GroundUnder(const ATCTerrain* Terrain, const FVector& Origin, const FVector& Dir, FVector2D& OutMetres);

	// Input, in viewport pixels (the HUD canvas's coordinates). Call MouseDown for every left or right press before the
	// world handles it; if bConsumed, the world must ignore that press (and its release). bAimArmed: the controller
	// has an aim (attack-move) armed. A press on a square still showing another match or map (a start arrived since
	// the last draw) is consumed with Action None.
	FTCMinimapPress MouseDown(FVector2D Pixel, ETCMinimapButton Button, const FTCMatchState& State, const FTCUnitTable& Table, bool bAimArmed = false);
	// Every frame while dragging: bLeftHeld false ends the drag (client/main.js mousemove without the button). Returns
	// true when the camera target moved (OutGround, and the hook was called).
	bool MouseMove(FVector2D Pixel, bool bLeftHeld, FVector2D* OutGround = nullptr);
	void MouseUp() { bDragging = false; }
	// The window lost the focus, or the pointer left it: the drag ends (client/main.js blur and mouseleave).
	void CancelDrag() { bDragging = false; }
	bool IsDragging() const { return bDragging; }

	// Whether the pixel lies on the minimap as last drawn.
	bool Contains(FVector2D Pixel) const;
	// The minimap square as last drawn (empty before the first draw of a match).
	FBox2D Rect() const;
	// Pixel <-> reference metres through the last drawn basis. PixelToWorld does not clamp; false without a basis.
	bool PixelToWorld(FVector2D Pixel, FVector2D& OutMetres) const;
	bool WorldToPixel(FVector2D Metres, FVector2D& OutPixel) const;
	// client/main.js minimapCursor: the ground, and the nearest held enemy, friend (not a structure or aircraft) and
	// friendly building within 5 browser-canvas pixels of it.
	FTCCursor CursorAt(FVector2D Metres, const FTCMatchState& State, const FTCUnitTable& Table) const;

	const FTCMinimapStats& Stats() const { return LastStats; }
	FString Describe() const;
	// The minimap's colour for a cell now (alpha 255), or false for a cell outside the image. For verification.
	bool PixelOf(int32 Cell, FColor& Out) const;

	// The minimap colour of one cell of the state (no unit lighting). Unknown cells are the shade, whatever they hold.
	static FColor CellColour(const FTCMatchState& State, int32 Cell, bool bLitByUnit = false);

private:
	struct FImageKey
	{
		int32 MatchId = -1, W = 0, H = 0, You = -1;
		bool bWorld = false, bSpectator = false;
		FString Name, Room;
		bool operator==(const FImageKey& O) const
		{
			return MatchId == O.MatchId && W == O.W && H == O.H && You == O.You && bWorld == O.bWorld && bSpectator == O.bSpectator && Name == O.Name && Room == O.Room;
		}
	};
	static FImageKey KeyOf(const FTCMatchState& State);
	static bool HasDrawableMap(const FTCMatchState& State);
	void LayoutIn(float ClipX, float ClipY);
	void BuildTexture();
	void UploadRows(int32 Y0, int32 Y1);
	void TargetCamera(FVector2D Metres);
	FVector2D ClampToMap(FVector2D Metres) const;

	TFunction<void(FVector2D)> CameraTargetHook;
	TStrongObjectPtr<UTexture2D> Texture;
	TArray<FColor> Pixels;           // the image, W * H, row 0 the map's row 0 (z = 0)
	TBitArray<> Lit;                 // cells under a held unit at the last refresh
	FImageKey Key;
	bool bHasImage = false;
	double LastRefreshAt = -1.0e9;
	TCMinimapCore::FBasis Basis;
	float LayoutX = 0.f, LayoutY = 0.f, LayoutSide = 0.f;
	bool bDragging = false;
	FTCMinimapStats LastStats;
	// ViewFromCanvas caches the corner picks while the camera, window and ground stay the same (client/camera.js corners)
	FString CornerKey;
	TArray<FVector2D, TInlineAllocator<4>> CornerCache;
};

#endif // TC_MINIMAP_CORE_ONLY
