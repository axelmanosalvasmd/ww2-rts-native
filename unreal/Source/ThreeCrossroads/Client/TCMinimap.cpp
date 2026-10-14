#include "Client/TCMinimap.h"
#include "World/TCFogFrontier.h"
#include "ThreeCrossroads.h"
#include "Client/TCUnitTable.h"
#include "Client/TCUnitRenderer.h"
#include "Net/TCMatchState.h"
#include "World/TCTerrain.h"
#include "Client/TCCameraPawn.h"
#include "CanvasItem.h"
#include "Engine/Canvas.h"
#include "Engine/Texture2D.h"
#include "Misc/App.h"
#include "TextureResource.h"
#include "SceneView.h"
#include "GlobalRenderResources.h"

namespace
{
	namespace MMCore = TCMinimapCore;

	FLinearColor MinimapSrgb(int32 R, int32 G, int32 B, int32 A = 255) { return FLinearColor(FColor((uint8)R, (uint8)G, (uint8)B, (uint8)A)); }

	// client/index.html #minimap box-shadow: 4 px of --panel-solid, then 1 px of --line, around the square
	const FColor MinimapPanelSolid(25, 28, 30, 245), MinimapFrameLine(176, 164, 122, 117);
	const FColor MinimapViewOutline(255, 255, 255, 0x88); // drawMinimap: the camera's view, '#fff8'
}

FTCMinimap::FTCMinimap() = default;
FTCMinimap::~FTCMinimap() = default;

void FTCMinimap::Reset()
{
	if (bHasImage) UE_LOG(LogTC, Log, TEXT("minimap: cleared (%dx%d image dropped)"), Key.W, Key.H);
	Texture.Reset();
	Pixels.Empty();
	Lit.Empty();
	Key = FImageKey();
	bHasImage = false;
	LastRefreshAt = -1.0e9;
	Basis = TCMinimapCore::FBasis();
	LayoutX = LayoutY = LayoutSide = 0.f;
	bDragging = false;
	LastStats = FTCMinimapStats();
	CornerKey.Reset();
	CornerCache.Reset();
}

FTCMinimap::FImageKey FTCMinimap::KeyOf(const FTCMatchState& State)
{
	FImageKey K;
	K.MatchId = State.MatchId; K.W = State.Map.W; K.H = State.Map.H; K.You = State.You;
	K.bWorld = State.Map.bWorld; K.bSpectator = State.bSpectator;
	K.Name = State.Map.Name; K.Room = State.RoomCode;
	return K;
}

bool FTCMinimap::HasDrawableMap(const FTCMatchState& State)
{
	const int64 N = (int64)State.Map.W * State.Map.H;
	return State.bHasMatch && State.Map.W > 0 && State.Map.H > 0 && N <= MAX_int32
		&& State.Map.Chars.Num() == N && State.Map.Levels.Num() == N && State.Map.Known.Num() == N;
}

FColor FTCMinimap::CellColour(const FTCMatchState& State, int32 Cell, bool bLitByUnit)
{
	if (!HasDrawableMap(State) || !State.Map.IsValidCell(Cell)) return FColor(MMCore::Shade, MMCore::Shade, MMCore::Shade, 255);
	const bool bKnown = State.Map.Known[Cell];
	// client/main.js: the fog lifts once an authored-map match is decided (World maps keep it); no mask, no fog
	const bool bLifted = State.bFogLifted || (!State.Map.bWorld && State.Winner >= -1);
	const int32 Alpha = MMCore::FogAlpha(bLifted, State.CellVisible(Cell) || bLitByUnit, State.CellExplored(Cell), State.Map.bWorld);
	// The character and level are read only for a known cell.
	const MMCore::FRgb P = bKnown ? MMCore::CellPixel(true, State.Map.Chars[Cell], State.Map.Levels[Cell], Alpha) : MMCore::CellPixel(false, '.', 0, Alpha);
	return FColor((uint8)P.R, (uint8)P.G, (uint8)P.B, 255);
}

void FTCMinimap::Refresh(const FTCMatchState& State, const FTCUnitTable& Table, bool bForce)
{
	if (!HasDrawableMap(State)) { if (bHasImage) Reset(); return; }
	const FImageKey NewKey = KeyOf(State);
	const bool bNewImage = !bHasImage || !(NewKey == Key);
	const double Now = FPlatformTime::Seconds();
	if (!bNewImage && !bForce && Now - LastRefreshAt < RefreshSeconds) return;
	LastRefreshAt = Now;

	const int32 W = State.Map.W, H = State.Map.H, N = W * H;
	if (bNewImage)
	{
		// a fresh texture for every new image: its size and filter (World or authored) follow the map
		Texture.Reset();
		if (bHasImage) UE_LOG(LogTC, Log, TEXT("minimap: new map or view, image rebuilt (%dx%d -> %dx%d)"), Key.W, Key.H, W, H);
		Key = NewKey;
		// alpha 0 never equals a cell's colour, so every pixel is written below
		Pixels.Init(FColor(0, 0, 0, 0), N);
		bHasImage = true;
		bDragging = false;
	}

	// client/fog.js snapshot: the ground under every held unit counts as seen until the next one (its centre cell;
	// aircraft and types this client does not know light nothing)
	Lit.Init(false, N);
	// round 14: the game's fog (World/TCFogFrontier, fog.js itself) lights a building's whole footprint
	const FTCFogFrontier* Frontier = TCFogLook::Active();
	if (Frontier && Frontier->HasMap() && Frontier->MapW() == W && Frontier->MapH() == H)
		for (int32 C = 0; C < N; C++) Lit[C] = Frontier->IsLit(C);
	else
	{
		for (const auto& Pair : State.Units)
		{
			const FTCUnit& U = Pair.Value;
			const FTCUnitDef* Def = Table.Find(U.Type);
			if (!Def || Def->bAir) continue;
			const int32 CX = FMath::FloorToInt32(U.X / MMCore::CellMetres), CZ = FMath::FloorToInt32(U.Z / MMCore::CellMetres);
			if (CX >= 0 && CZ >= 0 && CX < W && CZ < H) Lit[CZ * W + CX] = true;
		}
	}

	FTCMinimapStats S = LastStats;
	S.W = W; S.H = H;
	S.KnownCells = S.HiddenCells = S.ClearCells = S.DimCells = S.DarkCells = S.LitCells = 0;
	S.bFogLifted = State.bFogLifted || (!State.Map.bWorld && State.Winner >= -1);
	int32 Y0 = H, Y1 = -1;
	for (int32 Y = 0; Y < H; Y++)
	{
		for (int32 X = 0; X < W; X++)
		{
			const int32 C = Y * W + X;
			const bool bLit = Lit[C];
			const FColor New = CellColour(State, C, bLit);
			if (State.Map.Known[C])
			{
				S.KnownCells++;
				const bool bSeen = S.bFogLifted || State.CellVisible(C);
				if (bSeen || bLit) S.ClearCells++;
				else if (State.CellExplored(C)) S.DimCells++;
				else S.DarkCells++;
				if (bLit && !bSeen) S.LitCells++;
			}
			else S.HiddenCells++;
			if (Pixels[C] != New)
			{
				Pixels[C] = New;
				Y0 = FMath::Min(Y0, Y);
				Y1 = FMath::Max(Y1, Y);
			}
		}
	}
	S.Refreshes++;
	LastStats = S;

	// the GPU copy: built whole for a new image, else only the rows that changed
	if (FApp::CanEverRender())
	{
		if (!Texture.IsValid()) BuildTexture();
		else if (Y1 >= Y0) UploadRows(Y0, Y1);
	}
	if (bNewImage)
		UE_LOG(LogTC, Log, TEXT("minimap: %s %dx%d%s, %d cells known, %d hidden"), *State.Map.Name, W, H, State.Map.bWorld ? TEXT(" (World)") : TEXT(""), LastStats.KnownCells, LastStats.HiddenCells);
}

void FTCMinimap::BuildTexture()
{
	const int32 W = Key.W, H = Key.H;
	UTexture2D* T = UTexture2D::CreateTransient(W, H, PF_B8G8R8A8, NAME_None);
	if (!T || !T->GetPlatformData() || !T->GetPlatformData()->Mips.Num())
	{
		UE_LOG(LogTC, Warning, TEXT("minimap: could not create a %dx%d texture"), W, H);
		return;
	}
	// Rows in map order, FColor in memory is B, G, R, A: the pixel format's byte order.
	FTexture2DMipMap& Mip = T->GetPlatformData()->Mips[0];
	void* Data = Mip.BulkData.Lock(LOCK_READ_WRITE);
	const int64 Bytes = (int64)Pixels.Num() * sizeof(FColor);
	if (Data && Mip.BulkData.GetBulkDataSize() >= Bytes) FMemory::Memcpy(Data, Pixels.GetData(), Bytes);
	Mip.BulkData.Unlock();
	// Updated in place from here on: never streamed. A World map's fog is drawn crisp (client/main.js turns smoothing
	// off for it); an authored map is smoothed.
	T->NeverStream = true;
	T->SRGB = true;
	T->Filter = Key.bWorld ? TF_Nearest : TF_Bilinear;
	T->AddressX = TA_Clamp;
	T->AddressY = TA_Clamp;
	T->UpdateResource();
	Texture.Reset(T);
	LastStats.TextureBuilds++;
}

void FTCMinimap::UploadRows(int32 Y0, int32 Y1)
{
	UTexture2D* T = Texture.Get();
	if (!T || !T->GetResource()) return;
	const int32 W = Key.W, RowCount = Y1 - Y0 + 1;
	// The render thread reads its own copy, so later refreshes never race the upload.
	const int64 Bytes = (int64)W * RowCount * sizeof(FColor);
	uint8* Copy = (uint8*)FMemory::Malloc(Bytes);
	FMemory::Memcpy(Copy, Pixels.GetData() + (int64)Y0 * W, Bytes);
	FUpdateTextureRegion2D* Region = new FUpdateTextureRegion2D(0, (uint32)Y0, 0, 0, (uint32)W, (uint32)RowCount);
	T->UpdateTextureRegions(0, 1, Region, (uint32)(W * sizeof(FColor)), (uint32)sizeof(FColor), Copy,
		[](uint8* SrcData, const FUpdateTextureRegion2D* Regions)
		{
			FMemory::Free(SrcData);
			delete Regions;
		});
	LastStats.RegionUploads++;
	LastStats.RowsUploaded += RowCount;
}

void FTCMinimap::LayoutIn(float ClipX, float ClipY)
{
	// client/index.html: --mm 184 px, 228 px at 1600 x 860 and up; --gap 8 px; bottom right
	float Side = SideOverride > 0.f ? SideOverride : (ClipX >= 1600.f && ClipY >= 860.f ? 228.f : 184.f);
	Side = FMath::Min(Side, FMath::Min(ClipX, ClipY) - 2.f * Gap);
	if (Side < 16.f) { LayoutSide = 0.f; return; }
	LayoutSide = Side;
	LayoutX = ClipX - Gap - Side;
	LayoutY = ClipY - Gap - Side;
}

bool FTCMinimap::Arrange(float ClipX, float ClipY, const FTCMatchState& State, float YawDegrees)
{
	if (!HasDrawableMap(State) || !bHasImage || !(KeyOf(State) == Key)) { Basis = TCMinimapCore::FBasis(); bDragging = false; return false; }
	LayoutIn(ClipX, ClipY);
	if (LayoutSide <= 0.f) { Basis = TCMinimapCore::FBasis(); bDragging = false; return false; }
	Basis = TCMinimapCore::FBasis::Make(LayoutX, LayoutY, LayoutSide, State.Map.W * MMCore::CellMetres, State.Map.H * MMCore::CellMetres, YawDegrees);
	return Basis.IsValid();
}

bool FTCMinimap::Draw(UCanvas* Canvas, const FTCMatchState& State, const FTCUnitTable& Table, const TSet<int32>& Selected, const FTCMinimapView& View)
{
	if (!Canvas || !Canvas->Canvas) return false;
	if (!HasDrawableMap(State)) { if (bHasImage) Reset(); return false; }
	Refresh(State, Table, false);
	if (!Arrange(Canvas->ClipX, Canvas->ClipY, State, View.YawDegrees)) return false;
	if (!Texture.IsValid() || !Texture->GetResource()) return false;
	const double K = Basis.CanvasScale();
	const FVector2D Origin(LayoutX, LayoutY), Size(LayoutSide, LayoutSide);

	// frame and background
	{
		FCanvasTileItem FrameItem(Origin - FVector2D(5, 5), Size + FVector2D(10, 10), FLinearColor(MinimapFrameLine));
		FrameItem.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(FrameItem);
		FCanvasTileItem PanelItem(Origin - FVector2D(4, 4), Size + FVector2D(8, 8), FLinearColor(MinimapPanelSolid));
		PanelItem.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(PanelItem);
		FCanvasTileItem BackItem(Origin, Size, MinimapSrgb(MMCore::Background[0], MMCore::Background[1], MMCore::Background[2]));
		BackItem.BlendMode = SE_BLEND_Opaque;
		Canvas->DrawItem(BackItem);
	}

	// the map: two triangles carrying the image, its corners turned with the camera
	{
		const double MW = Basis.MapW, MH = Basis.MapH;
		FVector2D P[4];
		const double CX[4] = { 0.0, MW, MW, 0.0 }, CZ[4] = { 0.0, 0.0, MH, MH };
		for (int32 I = 0; I < 4; I++) { double PX, PY; Basis.ToPixel(CX[I], CZ[I], PX, PY); P[I] = FVector2D(PX, PY); }
		const FVector2D UV[4] = { FVector2D(0, 0), FVector2D(1, 0), FVector2D(1, 1), FVector2D(0, 1) };
		TArray<FCanvasUVTri> Tris;
		auto Tri = [&Tris, &P, &UV](int32 A, int32 B, int32 C)
		{
			FCanvasUVTri T;
			T.V0_Pos = P[A]; T.V1_Pos = P[B]; T.V2_Pos = P[C];
			T.V0_UV = UV[A]; T.V1_UV = UV[B]; T.V2_UV = UV[C];
			T.V0_Color = T.V1_Color = T.V2_Color = FLinearColor::White;
			Tris.Add(T);
		};
		Tri(0, 1, 2);
		Tri(0, 2, 3);
		FCanvasTriangleItem MapItem(Tris, Texture->GetResource());
		MapItem.BlendMode = SE_BLEND_Opaque;
		Canvas->DrawItem(MapItem);
	}

	// held units: a dot each in the seat's colour (3.5 m for anything not infantry, 2.6 m for infantry), selected ones
	// ringed white. Every held row is drawn, as client/main.js does; a minimum of 1 px keeps them visible on the
	// largest World maps.
	int32 Units = 0, Ringed = 0;
	for (const auto& Pair : State.Units)
	{
		const FTCUnit& U = Pair.Value;
		const FTCUnitDef* Def = Table.Find(U.Type);
		const double R = FMath::Max(1.0, (Def && Def->bInfantry ? MMCore::InfantryDotMetres : MMCore::VehicleDotMetres) * Basis.S);
		double PX, PY;
		Basis.ToPixel(U.X, U.Z, PX, PY);
		if (!Basis.InSquare(PX, PY)) continue;
		FLinearColor Seat = ATCUnitRenderer::SeatColor(U.Owner);
		Seat.A = 1.f;
		FCanvasNGonItem Dot(FVector2D(PX, PY), FVector2D(R, R), 10, Seat);
		Dot.BlendMode = SE_BLEND_Opaque;
		Canvas->DrawItem(Dot);
		Units++;
		if (Selected.Contains(U.Id))
		{
			constexpr int32 Segments = 12;
			FVector2D Prev(PX + R, PY);
			for (int32 I = 1; I <= Segments; I++)
			{
				const double A = 2.0 * UE_DOUBLE_PI * I / Segments;
				const FVector2D Next(PX + R * FMath::Cos(A), PY + R * FMath::Sin(A));
				FCanvasLineItem Seg(Prev, Next);
				Seg.SetColor(FLinearColor::White);
				Seg.LineThickness = (float)(1.5 * K);
				Canvas->DrawItem(Seg);
				Prev = Next;
			}
			Ringed++;
		}
	}

	// what the camera sees (client/camera.js corners), unless the HUD draws it last over its own overlays as
	// drawMinimap does (bDeferViewOutline, then DrawViewOutline)
	LastStats.bViewDrawn = false;
	if (!bDeferViewOutline) DrawViewOutline(Canvas, View);
	LastStats.UnitsDrawn = Units;
	LastStats.SelectedDrawn = Ringed;
	return true;
}

bool FTCMinimap::DrawViewOutline(UCanvas* Canvas, const FTCMinimapView& View)
{
	// client/main.js drawMinimap: the four corners closed into one outline, '#fff8', 1.5 canvas pixels, cut by the
	// minimap's square (the browser's canvas edge)
	if (!Canvas || !Canvas->Canvas || !Basis.IsValid() || LayoutSide <= 0.f || View.Corners.Num() != 4) return false;
	const double K = Basis.CanvasScale();
	const double MinX = LayoutX, MinY = LayoutY, MaxX = LayoutX + LayoutSide, MaxY = LayoutY + LayoutSide;
	bool bView = false;
	TArray<FCanvasUVTri> Strokes;
	for (int32 I = 0; I < 4; I++)
	{
		const FVector2D& A = View.Corners[I];
		const FVector2D& B = View.Corners[(I + 1) % 4];
		double X0, Y0, X1, Y1;
		Basis.ToPixel(A.X, A.Y, X0, Y0);
		Basis.ToPixel(B.X, B.Y, X1, Y1);
		if (!MMCore::ClipSegment(X0, Y0, X1, Y1, MinX, MinY, MaxX, MaxY)) continue;
		// a see-through stroke (a line item would draw it opaque): two triangles 1.5 canvas pixels wide
		const FVector2D A2(X0, Y0), B2(X1, Y1), D = B2 - A2;
		const double L = D.Size();
		if (L < 1e-6) continue;
		const FVector2D N = FVector2D(-D.Y, D.X) / L * (0.75 * K);
		FCanvasUVTri T0, T1;
		T0.V0_Pos = A2 + N; T0.V1_Pos = B2 + N; T0.V2_Pos = B2 - N;
		T1.V0_Pos = A2 + N; T1.V1_Pos = B2 - N; T1.V2_Pos = A2 - N;
		T0.V0_Color = T0.V1_Color = T0.V2_Color = T1.V0_Color = T1.V1_Color = T1.V2_Color = FLinearColor(MinimapViewOutline);
		for (FCanvasUVTri* T : { &T0, &T1 })
		{
			// the winding TCWarMap's flat triangles use
			if ((T->V1_Pos.X - T->V0_Pos.X) * (T->V2_Pos.Y - T->V0_Pos.Y) - (T->V1_Pos.Y - T->V0_Pos.Y) * (T->V2_Pos.X - T->V0_Pos.X) < 0) Swap(T->V1_Pos, T->V2_Pos);
			Strokes.Add(*T);
		}
		bView = true;
	}
	if (Strokes.Num())
	{
		FCanvasTriangleItem Item(Strokes, GWhiteTexture);
		Item.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(Item);
	}
	LastStats.bViewDrawn |= bView;
	return bView;
}

bool FTCMinimap::ViewFromCanvas(const UCanvas* Canvas, const ATCTerrain* Terrain, FTCMinimapView& Out)
{
	if (!Canvas || !Canvas->SceneView || Canvas->ClipX <= 0.f || Canvas->ClipY <= 0.f) return false;
	FVector O, D;
	Canvas->Deproject(FVector2D(Canvas->ClipX * 0.5f, Canvas->ClipY * 0.5f), O, D);
	if (FVector2D(D.X, D.Y).SizeSquared() < 1e-8) return false; // looking straight down: no heading to read
	Out.YawDegrees = (float)FMath::RadiansToDegrees(FMath::Atan2(D.Y, D.X));
	Out.Corners.Reset();
	const FSceneView* SV = Canvas->SceneView;
	const FString NewKey = FString::Printf(TEXT("%s|%s|%g|%g|%d|%d|%d|%d"), *SV->ViewLocation.ToString(), *SV->ViewRotation.ToString(), Canvas->ClipX, Canvas->ClipY,
		Terrain ? Terrain->MapW() : -1, Terrain ? Terrain->MapH() : -1, Terrain ? Terrain->ChunksMeshed() : -1, Terrain ? Terrain->CellsDrawn() : -1);
	if (NewKey != CornerKey)
	{
		CornerKey = NewKey;
		CornerCache.Reset();
		const FVector2D Screen[4] = { FVector2D(0, 0), FVector2D(Canvas->ClipX, 0), FVector2D(Canvas->ClipX, Canvas->ClipY), FVector2D(0, Canvas->ClipY) };
		for (const FVector2D& P : Screen)
		{
			FVector RO, RD;
			Canvas->Deproject(P, RO, RD);
			FVector2D G;
			if (GroundUnder(Terrain, RO, RD, G)) CornerCache.Add(G);
		}
	}
	Out.Corners = CornerCache;
	return true;
}

bool FTCMinimap::GroundUnder(const ATCTerrain* Terrain, const FVector& Origin, const FVector& Dir, FVector2D& OutMetres)
{
	// camera.js groundAt: the relief this client drew under the ray, else the flat y = 0 plane (Unreal Z = 0)
	if (Terrain && Terrain->RayToGround(Origin, Dir, OutMetres)) return true;
	const FVector D = Dir.GetSafeNormal();
	if (D.IsNearlyZero()) return false;
	const double O3[3] = { Origin.X / 100.0, Origin.Z / 100.0, Origin.Y / 100.0 }, D3[3] = { D.X, D.Z, D.Y };
	double P[3];
	if (!MMCore::FlatGround(O3, D3, P)) return false;
	OutMetres = FVector2D(P[0], P[2]);
	return true;
}

bool FTCMinimap::Contains(FVector2D Pixel) const
{
	return bHasImage && Basis.InSquare(Pixel.X, Pixel.Y);
}

FBox2D FTCMinimap::Rect() const
{
	if (!bHasImage || !Basis.IsValid()) return FBox2D(ForceInit);
	return FBox2D(FVector2D(Basis.OriginX, Basis.OriginY), FVector2D(Basis.OriginX + Basis.Side, Basis.OriginY + Basis.Side));
}

bool FTCMinimap::PixelToWorld(FVector2D Pixel, FVector2D& OutMetres) const
{
	if (!bHasImage || !Basis.IsValid()) return false;
	double X, Z;
	Basis.ToWorld(Pixel.X, Pixel.Y, X, Z);
	OutMetres = FVector2D(X, Z);
	return true;
}

bool FTCMinimap::WorldToPixel(FVector2D Metres, FVector2D& OutPixel) const
{
	if (!bHasImage || !Basis.IsValid()) return false;
	double PX, PY;
	Basis.ToPixel(Metres.X, Metres.Y, PX, PY);
	OutPixel = FVector2D(PX, PY);
	return true;
}

FVector2D FTCMinimap::ClampToMap(FVector2D Metres) const
{
	return FVector2D(FMath::Clamp(Metres.X, 0.0, Basis.MapW), FMath::Clamp(Metres.Y, 0.0, Basis.MapH));
}

void FTCMinimap::TargetCamera(FVector2D Metres)
{
	if (CameraTargetHook) CameraTargetHook(Metres);
}

FTCCursor FTCMinimap::CursorAt(FVector2D Metres, const FTCMatchState& State, const FTCUnitTable& Table) const
{
	FTCCursor C;
	C.bHasGround = Basis.IsValid() && Metres.X >= 0.0 && Metres.Y >= 0.0 && Metres.X <= Basis.MapW && Metres.Y <= Basis.MapH;
	C.Ground = Metres;
	if (!Basis.IsValid()) return C;
	const double Reach = MMCore::PickPixels * Basis.CanvasScale() / Basis.S;
	double BestEnemy = Reach, BestFriend = Reach, BestBuilding = Reach;
	for (const auto& Pair : State.Units)
	{
		const FTCUnit& U = Pair.Value;
		const double D = FVector2D::Distance(FVector2D(U.X, U.Z), Metres);
		if (D >= Reach) continue;
		if (State.IsFoe(U.Owner))
		{
			if (D < BestEnemy) { BestEnemy = D; C.Enemy = U.Id; }
			continue;
		}
		const FTCUnitDef* Def = Table.Find(U.Type);
		if (!Def) continue;
		if (!Def->bStructure && !Def->bAir && D < BestFriend) { BestFriend = D; C.Friend = U.Id; }
		if (Def->bBuilding && D < BestBuilding) { BestBuilding = D; C.Building = U.Id; }
	}
	return C;
}

FTCMinimapPress FTCMinimap::MouseDown(FVector2D Pixel, ETCMinimapButton Button, const FTCMatchState& State, const FTCUnitTable& Table, bool bAimArmed)
{
	FTCMinimapPress R;
	if (!Contains(Pixel)) return R;
	R.bConsumed = true; // the square is on screen there: the world never gets this press
	// The image (and the frame from it) belongs to another match or map than the state now (a start arrived since the
	// last draw): nothing on it maps to this match's ground yet, so the press does nothing.
	if (!HasDrawableMap(State) || !(KeyOf(State) == Key)) { bDragging = false; return R; }
	FVector2D At;
	PixelToWorld(Pixel, At);
	R.Ground = ClampToMap(At);
	if (Button == ETCMinimapButton::Right)
	{
		if (bAimArmed) { R.Action = ETCMinimapAction::CancelAim; return R; }
		R.Action = ETCMinimapAction::Order;
		R.Cursor = CursorAt(At, State, Table);
		return R;
	}
	bDragging = true;
	R.Action = ETCMinimapAction::CameraTarget;
	TargetCamera(R.Ground);
	return R;
}

bool FTCMinimap::MouseMove(FVector2D Pixel, bool bLeftHeld, FVector2D* OutGround)
{
	if (!bDragging) return false;
	if (!bLeftHeld || !bHasImage || !Basis.IsValid()) { bDragging = false; return false; }
	FVector2D At;
	if (!PixelToWorld(Pixel, At)) return false;
	const FVector2D Ground = ClampToMap(At);
	if (OutGround) *OutGround = Ground;
	TargetCamera(Ground);
	return true;
}

bool FTCMinimap::PixelOf(int32 Cell, FColor& Out) const
{
	if (!Pixels.IsValidIndex(Cell)) return false;
	Out = Pixels[Cell];
	return true;
}

FString FTCMinimap::Describe() const
{
	const FTCMinimapStats& S = LastStats;
	return FString::Printf(TEXT("minimap %dx%d: %d known (%d clear, %d dim, %d dark, %d lit), %d hidden; %d refreshes, %d texture builds, %d uploads (%d rows); %d units, %d ringed%s%s"),
		S.W, S.H, S.KnownCells, S.ClearCells, S.DimCells, S.DarkCells, S.LitCells, S.HiddenCells, S.Refreshes, S.TextureBuilds, S.RegionUploads, S.RowsUploaded,
		S.UnitsDrawn, S.SelectedDrawn, S.bViewDrawn ? TEXT(", view outlined") : TEXT(""), S.bFogLifted ? TEXT(", fog lifted") : TEXT(""));
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace
{
	TSharedPtr<FJsonObject> MinimapTestJson(const TCHAR* Src)
	{
		TSharedPtr<FJsonObject> Obj;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(FString(Src)), Obj);
		return Obj;
	}

	FTCUnit MinimapTestUnit(int32 Id, int32 Owner, const TCHAR* Type, float X, float Z)
	{
		FTCUnit U;
		U.Id = Id; U.Owner = Owner; U.Type = Type; U.X = X; U.Z = Z;
		return U;
	}

	FColor MinimapTestRgb(const TCMinimapCore::FRgb& C) { return FColor((uint8)C.R, (uint8)C.G, (uint8)C.B, 255); }
}

// Run (after a build): -ExecCmds="Automation RunTests ThreeCrossroads.Minimap; Quit" (works under -nullrhi: the image
// model is checked, the GPU texture is skipped there).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMinimapModelTest, "ThreeCrossroads.Minimap.Model", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCMinimapModelTest::RunTest(const FString& Parameters)
{
	namespace Core = TCMinimapCore;
	const FColor ShadeColour(Core::Shade, Core::Shade, Core::Shade, 255);
	FTCUnitTable Tbl;
	FString TableError;
	const bool bTable = Tbl.Load(TableError);
	TestTrue(TEXT("unit table loads (Content/Data/units.json)"), bTable);

	// World Conquest: every row says 'H' at level 4 (the server's placeholders stand in for undiscovered ground);
	// only cells 0-3 were sent. Seen: 0-2, explored: 0-4.
	FTCMatchState St;
	const TSharedPtr<FJsonObject> WorldStart = MinimapTestJson(TEXT("{\"t\":\"start\",\"matchId\":7,\"you\":0,\"teams\":[0,1],\"names\":[\"a\",\"b\"],")
		TEXT("\"map\":{\"name\":\"World\",\"w\":6,\"h\":4,\"world\":true,\"rows\":[\"HHHHHH\",\"HHHHHH\",\"HHHHHH\",\"HHHHHH\"],\"heights\":[\"444444\",\"444444\",\"444444\",\"444444\"]},")
		TEXT("\"fog\":{\"v\":\"AD\",\"e\":\"AF\"},\"cells\":[[0,\"W\",1,0,null],[1,\".\",0,0,null],[2,\"B\",2,0,null],[3,\"T\",0,0,null]]}"));
	if (!TestTrue(TEXT("World start parses"), WorldStart.IsValid() && St.ApplyStart(*WorldStart))) return false;
	TestTrue(TEXT("World map flagged"), St.Map.bWorld);

	FTCMinimap Mm;
	Mm.Refresh(St, Tbl);
	FColor Px;
	TestTrue(TEXT("cell 0 drawn"), Mm.PixelOf(0, Px));
	TestEqual(TEXT("seen W level 1"), Px, FColor(67, 104, 125, 255));
	Mm.PixelOf(2, Px);
	TestEqual(TEXT("seen B level 2"), Px, FColor(186, 164, 124, 255));
	Mm.PixelOf(3, Px);
	TestEqual(TEXT("explored T: dimmed"), Px, MinimapTestRgb(Core::CellPixel(true, 'T', 0, Core::AlphaExplored)));
	int32 Leaks = 0;
	for (int32 C = 4; C < 24; C++) { Mm.PixelOf(C, Px); Leaks += Px != ShadeColour; }
	TestEqual(TEXT("undiscovered World cells are the shade, never their placeholder"), Leaks, 0);
	TestEqual(TEXT("known cells"), Mm.Stats().KnownCells, 4);
	TestEqual(TEXT("hidden cells"), Mm.Stats().HiddenCells, 20);
	TestEqual(TEXT("static CellColour agrees"), FTCMinimap::CellColour(St, 4), ShadeColour);

	// a held unit lights its cell: a known one shows clear, an unknown one stays the shade
	if (bTable)
	{
		St.Units.Add(1, MinimapTestUnit(1, 0, TEXT("rifle"), 7.f, 1.f));  // cell 3, known, explored, not seen
		St.Units.Add(2, MinimapTestUnit(2, 0, TEXT("rifle"), 11.f, 1.f)); // cell 5, unknown
		Mm.Refresh(St, Tbl);
		Mm.PixelOf(3, Px);
		TestEqual(TEXT("lit known cell is clear"), Px, MinimapTestRgb(Core::TerrainColour('T', 0)));
		Mm.PixelOf(5, Px);
		TestEqual(TEXT("lit unknown cell stays the shade"), Px, ShadeColour);
		TestEqual(TEXT("lit cells counted"), Mm.Stats().LitCells, 1);
		St.Units.Reset();
	}

	// the side discovers cell 6 (not yet seen: dark on a World map), then sees it
	St.Map.Known[6] = true; St.Map.Chars[6] = 'W'; St.Map.Levels[6] = 0;
	Mm.Refresh(St, Tbl);
	Mm.PixelOf(6, Px);
	TestEqual(TEXT("known, never seen World cell is opaque dark"), Px, ShadeColour);
	St.FogVisible[6] = 1; St.FogExplored[6] = 1;
	Mm.Refresh(St, Tbl);
	Mm.PixelOf(6, Px);
	TestEqual(TEXT("newly seen cell drawn"), Px, MinimapTestRgb(Core::TerrainColour('W', 0)));
	// a decided World match keeps its fog (client/main.js lifts it on authored maps only)
	St.Winner = 0;
	Mm.Refresh(St, Tbl);
	TestFalse(TEXT("World fog stays after the end"), Mm.Stats().bFogLifted);
	Mm.PixelOf(10, Px);
	TestEqual(TEXT("still no leak after the end"), Px, ShadeColour);
	St.Winner = -2;

	// input: 1280 x 720 puts the 184 px square at (1088, 528); Unreal yaw -90 is x right, rows down
	TestTrue(TEXT("arranged"), Mm.Arrange(1280.f, 720.f, St, -90.f));
	TestEqual(TEXT("rect"), Mm.Rect().Min, FVector2D(1088, 528));
	const FVector2D Mid(1088 + 92, 528 + 92);
	FVector2D G;
	TestTrue(TEXT("pixel to ground"), Mm.PixelToWorld(Mid, G));
	TestTrue(TEXT("centre is the map centre"), G.Equals(FVector2D(6, 4), 1e-4));
	FVector2D P;
	Mm.WorldToPixel(FVector2D(10, 4), P);
	TestTrue(TEXT("east is right"), P.X > Mid.X && FMath::IsNearlyEqual(P.Y, Mid.Y, 1e-3));
	Mm.WorldToPixel(FVector2D(6, 7), P);
	TestTrue(TEXT("south is down"), P.Y > Mid.Y && FMath::IsNearlyEqual(P.X, Mid.X, 1e-3));

	TArray<FVector2D> Targets;
	Mm.SetCameraTargetHook([&Targets](FVector2D M) { Targets.Add(M); });
	TestFalse(TEXT("press off the minimap is not consumed"), Mm.MouseDown(FVector2D(10, 10), ETCMinimapButton::Left, St, Tbl).bConsumed);
	const FTCMinimapPress Left = Mm.MouseDown(Mid, ETCMinimapButton::Left, St, Tbl);
	TestTrue(TEXT("left press consumed"), Left.bConsumed && Left.Action == ETCMinimapAction::CameraTarget);
	TestTrue(TEXT("hook got the centre"), Targets.Num() == 1 && Targets[0].Equals(FVector2D(6, 4), 1e-4));
	TestTrue(TEXT("dragging"), Mm.IsDragging());
	TestTrue(TEXT("drag moves the camera"), Mm.MouseMove(FVector2D(1088, 528), true));
	TestTrue(TEXT("drag target clamped to the map"), Targets.Num() == 2 && Targets[1].Equals(FVector2D(0, 0), 1e-4));
	TestFalse(TEXT("button up ends the drag"), Mm.MouseMove(Mid, false));
	TestFalse(TEXT("not dragging"), Mm.IsDragging());

	// right press: an order at the ground, with the foe and friend under it (the seat confirmed by its lobby)
	St.LobbySeat = 0; St.bLobbySinceOpen = true;
	St.Units.Add(101, MinimapTestUnit(101, 1, TEXT("rifle"), 6.f, 4.f));
	St.Units.Add(102, MinimapTestUnit(102, 0, TEXT("rifle"), 6.1f, 4.f));
	const FTCMinimapPress Right = Mm.MouseDown(Mid, ETCMinimapButton::Right, St, Tbl);
	TestTrue(TEXT("right press is an order"), Right.bConsumed && Right.Action == ETCMinimapAction::Order);
	TestTrue(TEXT("order ground"), Right.Cursor.bHasGround && Right.Cursor.Ground.Equals(FVector2D(6, 4), 1e-4));
	TestEqual(TEXT("enemy under the press"), Right.Cursor.Enemy, 101);
	if (bTable) TestEqual(TEXT("friend under the press"), Right.Cursor.Friend, 102);
	TestEqual(TEXT("no hook call for an order"), Targets.Num(), 2);
	TestTrue(TEXT("right press with an aim armed cancels it"), Mm.MouseDown(Mid, ETCMinimapButton::Right, St, Tbl, true).Action == ETCMinimapAction::CancelAim);

	// Unreal yaw 0 (the camera looks along +x): +x is up on the minimap, +z to the right
	Mm.Arrange(1280.f, 720.f, St, 0.f);
	Mm.PixelToWorld(Mid + FVector2D(10, 0), G);
	TestTrue(TEXT("turned: right is +z"), FMath::IsNearlyEqual(G.X, 6.0, 1e-4) && G.Y > 4.0);
	Mm.PixelToWorld(Mid - FVector2D(0, 10), G);
	TestTrue(TEXT("turned: up is +x"), G.X > 6.0 && FMath::IsNearlyEqual(G.Y, 4.0, 1e-4));

	// another match (an authored map) replaces the image whole
	const TSharedPtr<FJsonObject> AuthoredStart = MinimapTestJson(TEXT("{\"t\":\"start\",\"matchId\":8,\"you\":0,\"teams\":[0,1],")
		TEXT("\"map\":{\"name\":\"Test\",\"w\":4,\"h\":3,\"rows\":[\"....\",\"HHHH\",\"WWWW\"],\"heights\":[\"0000\",\"1111\",\"a000\"]},")
		TEXT("\"fog\":{\"v\":\"AB\",\"e\":\"AC\"}}"));
	if (!TestTrue(TEXT("authored start parses"), AuthoredStart.IsValid() && St.ApplyStart(*AuthoredStart))) return false;
	const FTCMinimapPress Stale = Mm.MouseDown(Mid, ETCMinimapButton::Left, St, Tbl);
	TestTrue(TEXT("a press on the previous match's image is consumed but does nothing"), Stale.bConsumed && Stale.Action == ETCMinimapAction::None);
	TestEqual(TEXT("no camera target from a stale image"), Targets.Num(), 2);
	Mm.Refresh(St, Tbl);
	TestFalse(TEXT("old image gone"), Mm.PixelOf(12, Px));
	TestEqual(TEXT("authored: every cell known"), Mm.Stats().KnownCells, 12);
	Mm.PixelOf(0, Px);
	TestEqual(TEXT("authored seen"), Px, MinimapTestRgb(Core::TerrainColour('.', 0)));
	Mm.PixelOf(1, Px);
	TestEqual(TEXT("authored explored"), Px, MinimapTestRgb(Core::CellPixel(true, '.', 0, Core::AlphaExplored)));
	Mm.PixelOf(8, Px);
	TestEqual(TEXT("authored never seen, level -1"), Px, MinimapTestRgb(Core::CellPixel(true, 'W', -1, Core::AlphaDark)));
	St.Winner = -1; // a draw decides it: the fog lifts on an authored map
	Mm.Refresh(St, Tbl);
	TestTrue(TEXT("authored fog lifted at the end"), Mm.Stats().bFogLifted);
	Mm.PixelOf(8, Px);
	TestEqual(TEXT("lifted cell clear"), Px, MinimapTestRgb(Core::TerrainColour('W', -1)));

	// the match ends (the room is back in the lobby): nothing is kept
	St.ClearMatch();
	TestFalse(TEXT("no arrange without a match"), Mm.Arrange(1280.f, 720.f, St, -90.f));
	Mm.Refresh(St, Tbl);
	TestFalse(TEXT("image dropped with the match"), Mm.PixelOf(0, Px));
	TestFalse(TEXT("no input without a match"), Mm.MouseDown(Mid, ETCMinimapButton::Left, St, Tbl).bConsumed);
	Mm.Reset();
	TestEqual(TEXT("stats cleared"), Mm.Stats().Refreshes, 0);
	return true;
}

// Round 17: the view outline as the pinned reference computes it. The cases are client/camera.js pose() and corners()
// run under Node on the pinned files (round-17/markers/tools/view-oracle.mjs: THREE.PerspectiveCamera(42, w / h),
// PITCH 0.95, groundAt with no relief mesh over a constant hAt inside the map, the flat y = 0 plane past it).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMinimapViewOutlineTest, "ThreeCrossroads.Minimap.ViewOutline", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCMinimapViewOutlineTest::RunTest(const FString& Parameters)
{
	namespace Core = TCMinimapCore;
	struct FCase { const TCHAR* Name; double X, Y, Z, Yaw, Dist, W, H, MapW, MapH, Ground; double C[4][3]; };
	static const FCase Cases[] = {
		{ TEXT("conquest-pose-north"), 80, 0, 80, 0, 90, 1280, 720, 160, 160, 0, { { -4.657106525625977, 0, 21.457198516279277 }, { 164.65710652562598, 0, 21.457198516279277 }, { 128.1898543935684, 2.842170943040401e-14, 113.32465749273048 }, { 31.810145606431576, 0, 113.32465749273047 } } },
		{ TEXT("turned-35deg"), 60, 0, 100, 0.61, 70, 1280, 720, 160, 160, 0, { { -20.05378543042923, 0, 100.39885253297417 }, { 87.88470499664695, 7.105427357601002e-15, 24.958605607743138 }, { 105.56947930152307, 0, 99.77295886574255 }, { 44.12702809315526, 7.105427357601002e-15, 142.71624695982905 } } },
		{ TEXT("close-wide-window"), 120, 0, 40, -1.2, 30, 1920, 1080, 240, 200, 0, { { 127.96267338790976, 0, 6.627609908936833 }, { 148.4134460686554, 0, 59.23009803345336 }, { 115.46736137230532, 0, 58.996758633215094 }, { 103.82604975456444, 0, 29.05354007196494 } } },
		{ TEXT("raised-ground-edge"), 20, 3, 150, 2.5, 85, 1600, 900, 160, 160, 3, { { 51.024445345947676, 0, 247.86270130359156 }, { -82.64338183373803, 0, 148.0098539769229 }, { 2.373724759679348, 3, 97.54732915274366 }, { 75.29804710259545, 3, 152.02342395392117 } } },
		{ TEXT("far-zoom-square"), 200, 0, 200, 3, 260, 1000, 1000, 400, 400, 0, { { 312.3243572249476, 2.842170943040401e-14, 386.84471201836357 }, { 39.942181431057804, 2.842170943040401e-14, 348.01757446389377 }, { 136.0609564669811, -2.842170943040401e-14, 93.64130389162716 }, { 291.1106378181585, 0, 115.74309997300577 } } },
	};
	for (const FCase& C : Cases)
	{
		// the reference's formula: its camera, its corner order, the relief (here a level ground inside the map) else
		// the flat plane
		const Core::FRefView V = Core::FRefView::Pose(C.X, C.Y, C.Z, C.Yaw, C.Dist, C.W / C.H);
		auto Relief = [&C](const double O[3], const double D[3], double P[3])
		{
			if (D[1] == 0.0) return false;
			const double T = (C.Ground - O[1]) / D[1];
			if (T < 0.0) return false;
			for (int32 I = 0; I < 3; I++) P[I] = O[I] + D[I] * T;
			return P[0] >= 0.0 && P[2] >= 0.0 && P[0] <= C.MapW && P[2] <= C.MapH;
		};
		double Out[4][3];
		const int32 N = Core::ViewCorners(V, C.W, C.H, Relief, Out);
		TestEqual(FString::Printf(TEXT("%s: four corners"), C.Name), N, 4);
		double Worst = 0.0;
		for (int32 I = 0; I < N; I++) for (int32 K = 0; K < 3; K++) Worst = FMath::Max(Worst, FMath::Abs(Out[I][K] - C.C[I][K]));
		TestTrue(FString::Printf(TEXT("%s: corners equal the reference's within 1e-6 m (worst %g)"), C.Name, Worst), Worst < 1e-6);
		// not a rectangle: the far edge (top) is wider than the near one (bottom), a trapezoid for the tilted camera
		const double Top = FMath::Sqrt(FMath::Square(Out[1][0] - Out[0][0]) + FMath::Square(Out[1][2] - Out[0][2]));
		const double Bottom = FMath::Sqrt(FMath::Square(Out[2][0] - Out[3][0]) + FMath::Square(Out[2][2] - Out[3][2]));
		if (C.Ground == 0.0) TestTrue(FString::Printf(TEXT("%s: far edge wider than the near edge (%.2f > %.2f m)"), C.Name, Top, Bottom), Top > Bottom * 1.05);

		// this client's camera at the same pose (ATCCameraPawn::Place's pinhole: Unreal yaw -90 - degrees(yaw)) and
		// FTCMinimap::GroundUnder's fallback with no terrain: on level ground at 0 they give the reference's corners
		if (C.Ground != 0.0) continue;
		const FTCPinhole P(FVector(C.X, C.Z, C.Y), -90.f - (float)FMath::RadiansToDegrees(C.Yaw), (float)FMath::RadiansToDegrees(Core::RefPitch), (float)C.Dist, C.W / C.H);
		const FVector2D Px[4] = { FVector2D(0, 0), FVector2D(C.W, 0), FVector2D(C.W, C.H), FVector2D(0, C.H) };
		double WorstPinhole = 0.0, WorstFallback = 0.0;
		for (int32 I = 0; I < 4; I++)
		{
			FVector2D G;
			if (P.Ground(Px[I], C.W, C.H, 0.0, G)) WorstPinhole = FMath::Max(WorstPinhole, FVector2D::Distance(G, FVector2D(C.C[I][0], C.C[I][2])));
			else WorstPinhole = 1e9;
			const FVector Dir = P.F + P.R * ((2.0 * Px[I].X / C.W - 1.0) * P.TanX) + P.U * ((1.0 - 2.0 * Px[I].Y / C.H) * P.TanY);
			FVector2D F;
			if (FTCMinimap::GroundUnder(nullptr, P.Pos * 100.0, Dir, F)) WorstFallback = FMath::Max(WorstFallback, FVector2D::Distance(F, FVector2D(C.C[I][0], C.C[I][2])));
			else WorstFallback = 1e9;
		}
		// the pinhole is posed in float degrees: centimetres of difference over a 400 m map
		TestTrue(FString::Printf(TEXT("%s: this client's camera sees the same footprint (worst %.4f m)"), C.Name, WorstPinhole), WorstPinhole < 0.05);
		TestTrue(FString::Printf(TEXT("%s: GroundUnder's flat fallback (worst %.4f m)"), C.Name, WorstFallback), WorstFallback < 0.05);
	}

	// a corner ray above the horizon is dropped (THREE.Ray.intersectPlane returns null), so no outline is drawn
	{
		const Core::FRefView V = Core::FRefView::Pose(80, 0, 80, 0, 90, 16.0 / 9.0, 0.1);
		double Out[4][3];
		TestEqual(TEXT("a low pitch leaves the two top corners above the horizon"), Core::ViewCorners(V, 1280, 720, [](const double*, const double*, double*) { return false; }, Out), 2);
		double Hit[3];
		const double O[3] = { 0, 10, 0 }, Level[3] = { 1, 0, 0 }, Upward[3] = { 0, 1, 0 };
		TestFalse(TEXT("a level ray off the plane misses"), Core::FlatGround(O, Level, Hit));
		TestFalse(TEXT("a ray pointing up misses"), Core::FlatGround(O, Upward, Hit));
		FVector2D G;
		TestFalse(TEXT("GroundUnder: up misses"), FTCMinimap::GroundUnder(nullptr, FVector(0, 0, 1000), FVector(0, 0, 1), G));
		TestTrue(TEXT("GroundUnder: down hits the plane"), FTCMinimap::GroundUnder(nullptr, FVector(500, 700, 1000), FVector(0, 0, -1), G) && G.Equals(FVector2D(5, 7), 1e-9));
	}
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
