// The paper war map's runtime: the fade, the wash, the textures and the drawing on the HUD's canvas (client/map-view.js
// createMapView frame() and draw()). See Client/TCWarMap.h. The paper itself is painted in Client/TCWarMapPaint.cpp.
#include "Client/TCWarMap.h"
#include "ThreeCrossroads.h"
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "Client/TCLocale.h"
#include "World/TCTerrain.h"
#include "Net/TCMatchState.h"
#include "Engine/Canvas.h"
#include "Engine/Texture2D.h"
#include "Engine/Font.h"
#include "Engine/FontFace.h"
#include "CanvasItem.h"
#include "CanvasTypes.h"
#include "SceneView.h"
#include "TextureResource.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "HAL/PlatformTime.h"
#include "ImageUtils.h"
#include "UnrealClient.h"
#include "Misc/AutomationTest.h"
#include "GameFramework/HUD.h"
#include "Async/Async.h"
#include "Misc/PackageName.h"
#include "UObject/Package.h"
#include "UObject/SavePackage.h"

namespace
{
	// client/map-view.js
	const FColor PAPER(0xe8, 0xde, 0xc3), INK(0x3b, 0x32, 0x26), TITLE(0xef, 0xe6, 0xcc), RED(0xa3, 0x24, 0x1c);
	const FColor PENCIL_MOVE(0x2f, 0x3d, 0x6b), PENCIL_ATTACK(0xa3, 0x24, 0x1c), PENCIL_RETREAT(0x6b, 0x5a, 0x2f), PENCIL_OTHER(0x3b, 0x32, 0x26);
	// the wash: fog.js's overlay alpha, coloured (0.55, 0.40, 0.22) (World: 0.04) as the raw shader writes it
	const FColor WASH(140, 102, 56), WASH_OPAQUE(10, 10, 10);
	constexpr int32 DeskSpan = 7, DeskPixels = 2048;
	// client/fog.js
	constexpr uint8 FogClear = 0, FogDim = 120, FogDark = 185;
	constexpr float FogSpeed = FogDark / 0.25f;
	constexpr int32 FogFrontier = 4;
	// map-view.js KIND: plan kinds 1 move, 2 attack-move, 3 retreat, 4 attack, 5 fire at, 6 ability, 7 dig, 8 build,
	// 9 board; 0 none
	const TCHAR* const PlanKind[] = { nullptr, TEXT("move"), TEXT("attack"), TEXT("retreat"), TEXT("attack"), TEXT("attack"), TEXT("attack"), TEXT("other"), TEXT("other"), TEXT("other") };
	FColor PencilColour(const FString& Kind)
	{
		return Kind == TEXT("move") ? PENCIL_MOVE : Kind == TEXT("attack") ? PENCIL_ATTACK : Kind == TEXT("retreat") ? PENCIL_RETREAT : PENCIL_OTHER;
	}
	FLinearColor Lin(FColor C, float A = 1.f) { FLinearColor L(C); L.A = A; return L; }
	int32 JsRound(double V) { return (int32)FMath::FloorToDouble(V + 0.5); }

	// sRGB byte <-> linear, for mip averaging (WebGL generates the paper's mipmaps of an sRGB texture in linear light)
	struct FSrgbTable
	{
		float ToLinear[256];
		FSrgbTable()
		{
			for (int32 I = 0; I < 256; I++) { const float C = I / 255.f; ToLinear[I] = C <= 0.04045f ? C / 12.92f : FMath::Pow((C + 0.055f) / 1.055f, 2.4f); }
			for (int32 I = 0; I <= 4096; I++) Encode[I] = Exact(I / 4096.f);
		}
		uint8 Encode[4097];
		static uint8 Exact(float L) { L = FMath::Clamp(L, 0.f, 1.f); const float C = L <= 0.0031308f ? L * 12.92f : 1.055f * FMath::Pow(L, 1.f / 2.4f) - 0.055f; return (uint8)FMath::Clamp(FMath::RoundToInt(C * 255.f), 0, 255); }
		static uint8 ToByte(float L);
	};
	const FSrgbTable& Srgb() { static FSrgbTable T; return T; }
	uint8 FSrgbTable::ToByte(float L) { return Srgb().Encode[FMath::Clamp(FMath::RoundToInt(L * 4096.f), 0, 4096)]; }
}

// ---------------------------------------------------------------------------------------------------------------------
// The wash (client/fog.js)

void FTCWarMapFog::Reset(int32 InW, int32 InH, bool bInOpaque)
{
	W = InW; H = InH; bOpaque = bInOpaque; bLifted = false;
	const int32 N = FMath::Max(0, W * H);
	Want.SetNumZeroed(N); Lit.SetNumZeroed(N); Edge.SetNumZeroed(N); Busy.SetNumZeroed(N); Bytes.SetNumZeroed(N); Alpha.SetNumZeroed(N);
	ActiveList.Reset(); LitList.Reset();
}

bool FTCWarMapFog::Known(const FTCMatchState& S, int32 C) const
{
	// fog.js known: seen or explored (World: what this side discovered)
	return (S.FogVisible.IsValidIndex(C) && S.FogVisible[C]) || (S.FogExplored.IsValidIndex(C) && S.FogExplored[C]);
}

uint8 FTCWarMapFog::Frontier(const FTCMatchState& S, int32 C) const
{
	// full darkness next to the nearest never-seen cell, none FRONTIER cells in, smoothstepped between
	if (!Known(S, C)) return 0;
	const int32 X = C % W, Y = C / W;
	int32 Best = INT32_MAX;
	for (int32 J = FMath::Max(0, Y - FogFrontier); J <= FMath::Min(H - 1, Y + FogFrontier); J++)
		for (int32 I = FMath::Max(0, X - FogFrontier); I <= FMath::Min(W - 1, X + FogFrontier); I++)
			if (!Known(S, J * W + I)) Best = FMath::Min(Best, (I - X) * (I - X) + (J - Y) * (J - Y));
	if (Best == INT32_MAX) return 0;
	const double T = FMath::Min(1.0, (FMath::Sqrt((double)Best) - 0.5) / FogFrontier);
	return (uint8)JsRound(255 * (1 - T * T * (3 - 2 * T)));
}

uint8 FTCWarMapFog::Look(const FTCMatchState& S, int32 C, bool bLift) const
{
	// fog.js: lifted with no mask (a spectator of an authored map) or once the authored match is decided
	const bool bNoMask = S.bFogLifted && !bOpaque;
	const bool bLiftNow = bNoMask || (bLift && !bOpaque);
	const bool bSeen = S.FogVisible.IsValidIndex(C) && S.FogVisible[C];
	const bool bExplored = S.FogExplored.IsValidIndex(C) && S.FogExplored[C];
	const uint8 Base = bLiftNow || bSeen || Lit[C] ? FogClear : bExplored ? FogDim : bOpaque ? 255 : FogDark;
	return bOpaque && !bLiftNow ? FMath::Max(Base, Edge[C]) : Base;
}

void FTCWarMapFog::Retarget(const FTCMatchState& S, int32 C, bool bLift)
{
	const uint8 T = Look(S, C, bLift);
	if (T == Want[C]) return;
	Want[C] = T;
	if (!Busy[C]) { Busy[C] = 1; ActiveList.Add(C); }
}

static void LitCells(const FTCMatchState& S, const TMap<FString, int32>& BuildingSize, TFunctionRef<void(int32)> Each)
{
	// fog.js snapshot: a building's whole footprint (its position is the footprint's centre), one cell for anything else;
	// aircraft and unknown types light nothing
	const FTCUnitTable* Table = nullptr;
	(void)Table;
	for (const TPair<int32, FTCUnit>& P : S.Units)
	{
		const FTCUnit& U = P.Value;
		const int32* Size = BuildingSize.Find(U.Type);
		const bool bAir = U.Type == TEXT("fighter") || U.Type == TEXT("attacker") || U.Type == TEXT("bomber");
		if (bAir) continue;
		const int32 Sz = Size ? FMath::Max(1, *Size) : 1;
		const int32 X0 = Sz > 1 ? JsRound(U.X / 2.0 - Sz / 2.0) : (int32)FMath::FloorToDouble(U.X / 2.0);
		const int32 Y0 = Sz > 1 ? JsRound(U.Z / 2.0 - Sz / 2.0) : (int32)FMath::FloorToDouble(U.Z / 2.0);
		for (int32 Y = FMath::Max(0, Y0); Y < FMath::Min(S.Map.H, Y0 + Sz); Y++)
			for (int32 X = FMath::Max(0, X0); X < FMath::Min(S.Map.W, X0 + Sz); X++) Each(Y * S.Map.W + X);
	}
}

void FTCWarMapFog::Rebuild(const FTCMatchState& S, const TMap<FString, int32>& BuildingSize, bool bLift)
{
	for (const int32 C : LitList) Lit[C] = 0;
	LitList.Reset();
	LitCells(S, BuildingSize, [&](int32 C) { if (Lit.IsValidIndex(C) && !Lit[C]) { Lit[C] = 1; LitList.Add(C); } });
	const int32 N = W * H;
	if (bOpaque) for (int32 C = 0; C < N; C++) Edge[C] = Frontier(S, C);
	bLifted = bLift;
	ActiveList.Reset();
	for (int32 C = 0; C < N; C++) { Want[C] = Look(S, C, bLift); Alpha[C] = Want[C]; Bytes[C] = Want[C]; Busy[C] = 0; }
}

void FTCWarMapFog::Touch(const FTCMatchState& S, const TSet<int32>& Cells, bool bLift)
{
	// newly explored ground moves the frontier: redo the darkness of every cell within reach of it
	if (bOpaque)
	{
		TSet<int32> Redo;
		for (const int32 C : Cells)
		{
			if (C < 0 || C >= W * H) continue;
			const int32 X = C % W, Y = C / W;
			for (int32 J = FMath::Max(0, Y - FogFrontier); J <= FMath::Min(H - 1, Y + FogFrontier); J++)
				for (int32 I = FMath::Max(0, X - FogFrontier); I <= FMath::Min(W - 1, X + FogFrontier); I++) Redo.Add(J * W + I);
		}
		for (const int32 C : Redo) { Edge[C] = Frontier(S, C); Retarget(S, C, bLift); }
	}
	for (const int32 C : Cells) if (C >= 0 && C < W * H) Retarget(S, C, bLift);
}

void FTCWarMapFog::Snapshot(const FTCMatchState& S, const TMap<FString, int32>& BuildingSize, bool bLift)
{
	TArray<int32> Old = MoveTemp(LitList);
	LitList.Reset();
	for (const int32 C : Old) Lit[C] = 0;
	LitCells(S, BuildingSize, [&](int32 C) { if (Lit.IsValidIndex(C) && !Lit[C]) { Lit[C] = 1; LitList.Add(C); } });
	for (const int32 C : Old) Retarget(S, C, bLift);
	for (const int32 C : LitList) Retarget(S, C, bLift);
}

bool FTCWarMapFog::Frame(const FTCMatchState& S, float Dt, bool bLift)
{
	if (!S.bFogLifted && bLift != bLifted) { bLifted = bLift; for (int32 C = 0; C < W * H; C++) Retarget(S, C, bLift); }
	if (!ActiveList.Num()) return false;
	const float Step = FogSpeed * Dt;
	bool bChanged = false;
	for (int32 I = ActiveList.Num() - 1; I >= 0; I--)
	{
		const int32 C = ActiveList[I];
		const float T = Want[C], A = Alpha[C];
		const float Next = A < T ? FMath::Min(T, A + Step) : FMath::Max(T, A - Step);
		Alpha[C] = Next;
		const uint8 V = (uint8)JsRound(Next);
		if (Bytes[C] != V) { Bytes[C] = V; bChanged = true; }
		if (Next == T) { Busy[C] = 0; ActiveList.RemoveAtSwap(I, EAllowShrinking::No); }
	}
	return bChanged;
}

// ---------------------------------------------------------------------------------------------------------------------

float FTCWarMap::WantFade(float Zoom)
{
	const float T = FMath::Clamp((Zoom - FadeFrom) / (FadeTo - FadeFrom), 0.f, 1.f);
	return T * T * (3.f - 2.f * T);
}

FTCWarMap::FTCWarMap() {}

void FTCWarMap::AddReferencedObjects(FReferenceCollector& Collector)
{
	Collector.AddReferencedObject(SymbolTexture);
	Collector.AddReferencedObject(FontFace);
	Collector.AddReferencedObject(MapFont);
	Collector.AddReferencedObject(PaperTexture);
	Collector.AddReferencedObject(DeskTexture);
	Collector.AddReferencedObject(WashTexture);
}

bool FTCWarMap::LoadAssets(FString& OutProblem)
{
	bAssetsTried = true;
	// WarMapData.json (unreal/Tools/warmap/make-warmap.mjs): the atlas layout and the shared/sim.js numbers
	const FString Path = FPaths::Combine(FPaths::ProjectContentDir(), TEXT("WarMap/WarMapData.json"));
	FString Text;
	TSharedPtr<FJsonObject> Root;
	if (!FFileHelper::LoadFileToString(Text, *Path) || !FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root) || !Root.IsValid())
	{
		OutProblem = AssetProblem = FString::Printf(TEXT("cannot read %s"), *Path);
		return false;
	}
	const TSharedPtr<FJsonObject>* A = nullptr, * Sim = nullptr;
	if (Root->TryGetObjectField(TEXT("atlas"), A))
	{
		Atlas.Cell = (*A)->GetIntegerField(TEXT("cell")); Atlas.Columns = (*A)->GetIntegerField(TEXT("columns"));
		Atlas.Rows = (*A)->GetIntegerField(TEXT("rows")); Atlas.Box = (*A)->GetIntegerField(TEXT("box"));
		Atlas.HaloRowsOffset = (*A)->GetIntegerField(TEXT("halo_rows_offset"));
		const TArray<TSharedPtr<FJsonValue>>* Keys = nullptr;
		if ((*A)->TryGetArrayField(TEXT("keys"), Keys)) for (int32 I = 0; I < Keys->Num(); I++) Atlas.Index.Add((*Keys)[I]->AsString(), I);
	}
	if (Root->TryGetObjectField(TEXT("sim"), Sim))
	{
		PointRadius = (*Sim)->GetNumberField(TEXT("pointRadius"));
		LevelHeight = (*Sim)->GetNumberField(TEXT("levelHeight"));
		CellMetres = (*Sim)->GetNumberField(TEXT("CELL"));
		const TSharedPtr<FJsonObject>* Sizes = nullptr;
		if ((*Sim)->TryGetObjectField(TEXT("buildingSize"), Sizes)) for (const auto& P : (*Sizes)->Values) BuildingSize.Add(FString(*P.Key), (int32)P.Value->AsNumber());
	}
	SymbolTexture = LoadObject<UTexture2D>(nullptr, TEXT("/Game/WarMap/T_TCWarMapSymbols.T_TCWarMapSymbols"));
	FontFace = LoadObject<UFontFace>(nullptr, TEXT("/Game/WarMap/Fonts/F_TCWarMapMono.F_TCWarMapMono"));
	TArray<FString> Missing;
	if (!Atlas.Index.Num()) Missing.Add(TEXT("the atlas layout"));
	if (!SymbolTexture) Missing.Add(TEXT("/Game/WarMap/T_TCWarMapSymbols"));
	if (!FontFace) Missing.Add(TEXT("/Game/WarMap/Fonts/F_TCWarMapMono (texts fall back to the engine's mono font)"));
	OutProblem = AssetProblem = FString::Join(Missing, TEXT(", "));
	bAssets = Atlas.Index.Num() && SymbolTexture;
	return bAssets && FontFace;
}

UFont* FTCWarMap::Font()
{
	if (MapFont) return MapFont;
	// map-view.js writes in bold "Courier New": the face the reference's Chromium draws it with here (Tools/warmap), as a
	// runtime font made on first use
	if (FontFace)
	{
		UFont* F = NewObject<UFont>(GetTransientPackage(), NAME_None, RF_Transient);
		F->FontCacheType = EFontCacheType::Runtime;
		F->LegacyFontSize = 12;
		FTypefaceEntry& E = F->GetMutableInternalCompositeFont().DefaultTypeface.Fonts.AddDefaulted_GetRef();
		E.Name = TEXT("Bold");
		E.Font = FFontData(FontFace);
		MapFont = F;
	}
	else MapFont = LoadObject<UFont>(nullptr, TEXT("/Engine/EngineFonts/DroidSansMono.DroidSansMono"));
	return MapFont;
}

void FTCWarMap::Start(const FTCMatchState& S)
{
	if (!bAssetsTried) { FString Why; if (!LoadAssets(Why)) UE_LOG(LogTC, Warning, TEXT("war map: %s"), *Why); }
	const bool bNewSize = S.Map.W != W || S.Map.H != H;
	W = S.Map.W; H = S.Map.H; bWorld = S.Map.bWorld;
	// map.name || lobbyState.mapName
	Title = !S.Map.Name.IsEmpty() ? S.Map.Name : S.LobbyMap;
	bDirty = true; SincePaint = 1e30;
	bFogBuilt = false; LastSnapshots = -1;
	PaintJob.Reset(); PaintDone.Reset(); PaintLevels.Reset();
	Fog.Reset(W, H, bWorld);
	if (bNewSize || !DeskTexture) { DeskTexture = nullptr; DeskSize = FVector2D::ZeroVector; }
	FadeNow = 0.f;
}

void FTCWarMap::Clear()
{
	W = H = 0; Title.Reset(); FadeNow = 0.f; bDirty = true; SincePaint = 1e30; bFogBuilt = false;
	PaperTexture = nullptr; WashTexture = nullptr; DeskTexture = nullptr; Paper.Init(0, 0); Desk.Init(0, 0);
	PaintJob.Reset(); PaintDone.Reset(); PaintLevels.Reset(); // a paint still running on a worker finishes into its own canvas, unused
}

void FTCWarMap::MarkFog(const FTCMatchState& S, const TSet<int32>& Cells)
{
	if (!bFogBuilt || !Cells.Num()) return;
	Fog.Touch(S, Cells, bLastLift);
}

void FTCWarMap::BuildLevels(const TCWarMapPaint::FPaintCanvas& C, bool bMips, bool bSrgb, FLevels& Out)
{
	Out.Bytes.Reset(); Out.Sizes.Reset();
	if (C.W <= 0 || C.H <= 0) return;
	// level 0 from the canvas (RGBA to BGRA), then each level the linear-light average of 2 x 2 (alpha averaged as is)
	TArray<uint8> Level;
	Level.SetNumUninitialized(C.W * C.H * 4);
	for (int32 I = 0; I < C.W * C.H; I++) { Level[I * 4] = C.Rgba[I * 4 + 2]; Level[I * 4 + 1] = C.Rgba[I * 4 + 1]; Level[I * 4 + 2] = C.Rgba[I * 4]; Level[I * 4 + 3] = C.Rgba[I * 4 + 3]; }
	int32 Lw = C.W, Lh = C.H;
	const FSrgbTable& Tb = Srgb();
	for (;;)
	{
		Out.Sizes.Add(FIntPoint(Lw, Lh));
		Out.Bytes.Add(Level);
		if (!bMips || (Lw == 1 && Lh == 1)) break;
		const int32 Nw = FMath::Max(1, Lw / 2), Nh = FMath::Max(1, Lh / 2);
		TArray<uint8> Next;
		Next.SetNumUninitialized(Nw * Nh * 4);
		for (int32 Y = 0; Y < Nh; Y++) for (int32 X = 0; X < Nw; X++)
		{
			float Sum[4] = { 0, 0, 0, 0 };
			for (int32 Dy = 0; Dy < 2; Dy++) for (int32 Dx = 0; Dx < 2; Dx++)
			{
				const int32 Sx = FMath::Min(Lw - 1, X * 2 + Dx), Sy = FMath::Min(Lh - 1, Y * 2 + Dy);
				const uint8* P = Level.GetData() + (Sy * Lw + Sx) * 4;
				for (int32 K = 0; K < 3; K++) Sum[K] += bSrgb ? Tb.ToLinear[P[K]] : P[K] / 255.f;
				Sum[3] += P[3] / 255.f;
			}
			uint8* O = Next.GetData() + (Y * Nw + X) * 4;
			for (int32 K = 0; K < 3; K++) O[K] = bSrgb ? FSrgbTable::ToByte(Sum[K] / 4) : (uint8)FMath::RoundToInt(Sum[K] / 4 * 255.f);
			O[3] = (uint8)FMath::RoundToInt(Sum[3] / 4 * 255.f);
		}
		Level = MoveTemp(Next); Lw = Nw; Lh = Nh;
	}
}

UTexture2D* FTCWarMap::TextureFromLevels(const FLevels& L, bool bSrgb)
{
	if (!L.Sizes.Num()) return nullptr;
	UTexture2D* T = NewObject<UTexture2D>(GetTransientPackage(), NAME_None, RF_Transient);
	FTexturePlatformData* PD = new FTexturePlatformData();
	PD->SizeX = L.Sizes[0].X; PD->SizeY = L.Sizes[0].Y; PD->SetNumSlices(1); PD->PixelFormat = PF_B8G8R8A8;
	for (int32 I = 0; I < L.Sizes.Num(); I++)
	{
		FTexture2DMipMap* Mip = new FTexture2DMipMap(L.Sizes[I].X, L.Sizes[I].Y, 1);
		Mip->BulkData.Lock(LOCK_READ_WRITE);
		FMemory::Memcpy(Mip->BulkData.Realloc(L.Bytes[I].Num()), L.Bytes[I].GetData(), L.Bytes[I].Num());
		Mip->BulkData.Unlock();
		PD->Mips.Add(Mip);
	}
	T->SetPlatformData(PD);
	T->SRGB = bSrgb;
	T->Filter = L.Sizes.Num() > 1 ? TF_Trilinear : TF_Bilinear;
	T->AddressX = T->AddressY = TA_Clamp;
	T->NeverStream = true;
	T->LODGroup = TEXTUREGROUP_UI;
#if WITH_EDITORONLY_DATA
	T->MipGenSettings = TMGS_NoMipmaps; // editor-only data (round 14 lead fix: the game target has no such member)
#endif
	T->UpdateResource();
	return T;
}

UTexture2D* FTCWarMap::MakeTexture(const TCWarMapPaint::FPaintCanvas& C, bool bMips, bool bSrgb, UTexture2D* Reuse)
{
	(void)Reuse;
	FLevels L;
	BuildLevels(C, bMips, bSrgb, L);
	return TextureFromLevels(L, bSrgb);
}

void FTCWarMap::Paint(const FTCMatchState& S, const ATCTerrain* Terrain)
{
	// the inputs are read here, on the game thread (the grid and the drawn ground's heights); the painting and the
	// mipmaps run on a worker, and the texture is made when it is done (a paint costs tens of milliseconds: a fight's
	// craters would otherwise stall a frame every RepaintSeconds)
	if (PaintJob.IsValid()) { bDirty = true; return; } // one at a time: the newer terrain is painted next
	PaintStartedAt = FPlatformTime::Seconds();
	TSharedPtr<TCWarMapPaint::FPaperInput, ESPMode::ThreadSafe> In = MakeShared<TCWarMapPaint::FPaperInput, ESPMode::ThreadSafe>();
	In->W = W; In->H = H;
	In->Chars = S.Map.Chars;
	In->Known = S.Map.Known;
	In->Level.SetNumZeroed(W * H);
	// contours: Math.round(hAt(centre) / levelHeight) of the drawn ground, known cells only
	for (int32 Y = 0; Y < H; Y++) for (int32 X = 0; X < W; X++)
	{
		const int32 C = Y * W + X;
		if (In->Known.IsValidIndex(C) && !In->Known[C]) continue;
		const float Hm = Terrain ? Terrain->HeightAt((X + 0.5f) * CellMetres, (Y + 0.5f) * CellMetres) : (S.Map.Levels.IsValidIndex(C) ? S.Map.Levels[C] * LevelHeight : 0.f);
		In->Level[C] = (int8)JsRound(Hm / LevelHeight);
	}
	if (In->Chars.Num() != W * H) In->Chars.Init('.', W * H);
	// recipient knowledge, for the log: cells this side does not know are painted as blank paper whatever they hold
	UnknownAtPaint = 0;
	for (int32 C = 0; C < W * H; C++) if (In->Known.IsValidIndex(C) && !In->Known[C]) UnknownAtPaint++;
	PaintJob = MakeShared<TCWarMapPaint::FPaintCanvas, ESPMode::ThreadSafe>();
	PaintDone = MakeShared<TAtomic<bool>, ESPMode::ThreadSafe>(false);
	PaintLevels = MakeShared<FLevels, ESPMode::ThreadSafe>();
	TSharedPtr<TCWarMapPaint::FPaintCanvas, ESPMode::ThreadSafe> Job = PaintJob;
	TSharedPtr<TAtomic<bool>, ESPMode::ThreadSafe> Done = PaintDone;
	TSharedPtr<FLevels, ESPMode::ThreadSafe> Levels = PaintLevels;
	// a thread of its own: the task pool is busy compiling shaders and building the world in a match's first minute
	Async(EAsyncExecution::Thread, [In, Job, Done, Levels]()
	{
		TCWarMapPaint::PaintPaper(*In, *Job);
		BuildLevels(*Job, true, true, *Levels);
		*Done = true;
	});
}

void FTCWarMap::FinishPaint()
{
	if (!PaintJob.IsValid() || !PaintDone.IsValid() || !*PaintDone) return;
	Paper = MoveTemp(*PaintJob);
	const double T1 = FPlatformTime::Seconds();
	PaperTexture = TextureFromLevels(*PaintLevels, true);
	PaintJob.Reset(); PaintDone.Reset(); PaintLevels.Reset();
	PaintCount++;
	LastPaintMs = (FPlatformTime::Seconds() - PaintStartedAt) * 1000.0;
	UE_LOG(LogTC, Log, TEXT("war map: paper painted (%d x %d px, %d px a cell, paint #%d, %d unknown cells left blank): %.1f ms from the request, %.1f ms of it on the game thread for the texture"),
		Paper.W, Paper.H, TCWarMapPaint::PixelsPerCell(W, H), PaintCount, UnknownAtPaint, LastPaintMs, (FPlatformTime::Seconds() - T1) * 1000.0);
}

void FTCWarMap::PaintDeskOnce()
{
	if (DeskTexture) return;
	const double MW = W * CellMetres, MH = H * CellMetres, M = FMath::Max(MW, MH);
	TCWarMapPaint::PaintDesk(Desk, DeskPixels, DeskSpan, MW / M, MH / M);
	DeskTexture = MakeTexture(Desk, true, true, nullptr);
	DeskSize = FVector2D(M * DeskSpan, M * DeskSpan);
}

void FTCWarMap::UploadWash()
{
	TCWarMapPaint::FPaintCanvas C;
	C.Init(W, H);
	const FColor Wc = Fog.IsOpaque() ? WASH_OPAQUE : WASH;
	for (int32 I = 0; I < W * H; I++) { C.Rgba[I * 4] = Wc.R; C.Rgba[I * 4 + 1] = Wc.G; C.Rgba[I * 4 + 2] = Wc.B; C.Rgba[I * 4 + 3] = Fog.AlphaByte(I); }
	WashTexture = MakeTexture(C, false, true, WashTexture);
	WashUploads++;
}

void FTCWarMap::Frame(float Dt, float Zoom, const FTCMatchState& S, const ATCTerrain* Terrain, bool bLift)
{
	if (W <= 0 || H <= 0) return;
	// map-view.js frame
	const float Want = WantFade(Zoom);
	FadeNow += (Want - FadeNow) * FMath::Min(1.f, Dt * 8.f);
	if ((Want == 0.f || Want == 1.f) && FMath::Abs(Want - FadeNow) < 0.001f) FadeNow = Want;
	const bool bOn = FadeNow > 0.01f;
	SincePaint += Dt;
	TourTick(Dt, Zoom, S, Terrain);
	bLastLift = bLift;
	if (bFogBuilt)
	{
		if (S.Snapshots != LastSnapshots) { LastSnapshots = S.Snapshots; Fog.Snapshot(S, BuildingSize, bLift); }
		if (Fog.Frame(S, Dt, bLift)) bWashDirty = true;
	}
	if (!bOn) return;
	if (!bFogBuilt)
	{
		Fog.Rebuild(S, BuildingSize, bLift);
		bFogBuilt = true; LastSnapshots = S.Snapshots; bWashDirty = true;
	}
	const bool bChanged = bDirty && SincePaint >= RepaintSeconds;
	if (bChanged) { bDirty = false; SincePaint = 0; Paint(S, Terrain); }
	FinishPaint();
	PaintDeskOnce();
	if (bWashDirty) { UploadWash(); bWashDirty = false; }
}

void FTCWarMap::TourTick(float Dt, float Zoom, const FTCMatchState& S, const ATCTerrain* Terrain)
{
	if (!bTourRead)
	{
		bTourRead = true;
		FParse::Value(FCommandLine::Get(), TEXT("TCWarMapTour="), TourFile);
		FParse::Value(FCommandLine::Get(), TEXT("TCWarMapShotDir="), ShotDir);
		TArray<FString> Lines;
		if (!TourFile.IsEmpty() && FFileHelper::LoadFileToStringArray(Lines, *TourFile))
			for (FString L : Lines)
			{
				int32 Hash;
				if (L.FindChar('#', Hash)) L.LeftInline(Hash);
				TArray<FString> F;
				L.ParseIntoArrayWS(F);
				if (F.Num() < 6) continue;
				FTourPose P;
				P.Name = F[0]; P.At = FCString::Atod(*F[1]); P.X = FCString::Atod(*F[2]); P.Z = FCString::Atod(*F[3]);
				P.bShare = F[4].StartsWith(TEXT("w")); P.Dist = FCString::Atod(P.bShare ? *F[4].Mid(1) : *F[4]); P.Yaw = FCString::Atod(*F[5]);
				Tour.Add(P);
			}
		if (!TourFile.IsEmpty()) UE_LOG(LogTC, Log, TEXT("war map tour: %d poses from %s into %s"), Tour.Num(), *TourFile, *ShotDir);
	}
	if (TourStep >= Tour.Num() || !Terrain || !Terrain->GetWorld()) return;
	ATCPlayerController* PC = Cast<ATCPlayerController>(Terrain->GetWorld()->GetFirstPlayerController());
	if (!PC) return;
	TourClock += Dt;
	FTourPose& P = Tour[TourStep];
	const double InMatch = S.bHasMatch && S.Snapshots > 0 ? S.Tick / 20.0 : -1;
	if (TourPhase == 0)
	{
		if (InMatch < P.At || !PC->HasWideView()) return;
		PC->FocusOn(FVector2D(P.X, P.Z));
		PC->SetViewYawForChecks((float)P.Yaw);
		TourPhase = 1; // the widest view is measured again for this heading in the controller's next tick
		return;
	}
	if (TourPhase == 1)
	{
		PC->ZoomTo(P.bShare ? (float)(PC->WideDistance() * P.Dist) : (float)P.Dist);
		if (AHUD* Hud = PC->GetHUD()) Hud->bShowHUD = true;
		TourPhase = 2; TourShotAt = TourClock + 2.5;
		return;
	}
	if (TourClock < TourShotAt || FScreenshotRequest::IsScreenshotRequested()) return;
	const FString File = FPaths::Combine(ShotDir, FString::Printf(TEXT("warmap-%s.png"), *P.Name));
	FScreenshotRequest::RequestScreenshot(File, true, false);
	UE_LOG(LogTC, Log, TEXT("war map tour framing: %s"), *PC->FramingCheck());
	UE_LOG(LogTC, Log, TEXT("war map tour: %s: target (%.1f, %.1f) yaw %.0f distance %.1f m, widest %.1f m at (%.1f, %.1f), zoom %.3f | drawn: %d unit symbols, %d capture points, %d pencil routes | match time %.1f s | %s"),
		*File, PC->ViewFocus().X, PC->ViewFocus().Y, PC->ViewYaw(), PC->ViewDistance(), PC->WideDistance(), PC->WideFocus().X, PC->WideFocus().Y, Zoom,
		LastDrawnUnits, LastDrawnPoints, LastDrawnPencil, S.Tick / 20.0, *StatusLine());
	TourPhase = 0; TourStep++;
}

FString FTCWarMap::StatusLine() const
{
	return FString::Printf(TEXT("war map: fade %.3f, %d x %d cells, paper %d x %d px (%d paints, last %.1f ms), wash %s uploads %d active %d, assets %s%s"),
		FadeNow, W, H, Paper.W, Paper.H, PaintCount, LastPaintMs, Fog.IsOpaque() ? TEXT("opaque") : TEXT("sepia"), WashUploads, Fog.Active(),
		bAssets ? TEXT("loaded") : TEXT("missing"), AssetProblem.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *AssetProblem));
}

// ---------------------------------------------------------------------------------------------------------------------
// Drawing on the canvas

namespace
{
	// The engine's own projection of the view being drawn (the same matrices the 3D scene used), clip space kept so
	// triangles can be cut at the camera.
	struct FProj
	{
		FMatrix VP;
		double SX = 0, SY = 0;
		bool bOk = false;
		FVector4 Clip(const FVector& P) const { return VP.TransformFVector4(FVector4(P, 1.0)); }
		FVector2D Screen(const FVector4& C) const { return FVector2D(SX / 2 + C.X / C.W * SX / 2, SY / 2 - C.Y / C.W * SY / 2); }
		bool At(const FVector& P, FVector2D& Out) const { const FVector4 C = Clip(P); if (C.W <= 1.0) return false; Out = Screen(C); return true; }
	};

	struct FCVert { FVector4 C; FVector2D UV; };

	// Cuts a triangle at the camera (clip w > Near) and adds what is left, wound the way the canvas keeps
	void AddClipped(const FProj& P, const FCVert (&In)[3], const FLinearColor& Colour, TArray<FCanvasUVTri>& Out)
	{
		constexpr double Near = 5.0;
		TArray<FCVert, TInlineAllocator<6>> Poly;
		for (int32 I = 0; I < 3; I++)
		{
			const FCVert& A = In[I]; const FCVert& B = In[(I + 1) % 3];
			const bool Ain = A.C.W > Near, Bin = B.C.W > Near;
			if (Ain) Poly.Add(A);
			if (Ain != Bin)
			{
				const double T = (Near - A.C.W) / (B.C.W - A.C.W);
				FCVert M; M.C = A.C + (B.C - A.C) * T; M.UV = A.UV + (B.UV - A.UV) * T;
				Poly.Add(M);
			}
		}
		for (int32 I = 1; I + 1 < Poly.Num(); I++)
		{
			FCanvasUVTri T;
			FVector2D S0 = P.Screen(Poly[0].C), S1 = P.Screen(Poly[I].C), S2 = P.Screen(Poly[I + 1].C);
			FVector2D U0 = Poly[0].UV, U1 = Poly[I].UV, U2 = Poly[I + 1].UV;
			if ((S1.X - S0.X) * (S2.Y - S0.Y) - (S1.Y - S0.Y) * (S2.X - S0.X) < 0) { Swap(S1, S2); Swap(U1, U2); }
			T.V0_Pos = S0; T.V1_Pos = S1; T.V2_Pos = S2; T.V0_UV = U0; T.V1_UV = U1; T.V2_UV = U2;
			T.V0_Color = T.V1_Color = T.V2_Color = Colour;
			Out.Add(T);
		}
	}

	void DrawTris(UCanvas* Canvas, TArray<FCanvasUVTri>& Tris, UTexture2D* Tex)
	{
		if (!Tris.Num() || !Tex || !Tex->GetResource()) return;
		FCanvasTriangleItem Item(Tris, Tex->GetResource());
		Item.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(Item);
		Tris.Reset();
	}

	// flat-coloured triangles (GWhiteTexture)
	struct FFlat
	{
		TArray<FCanvasUVTri> Tris;
		void Tri(FVector2D A, FVector2D B, FVector2D C, const FLinearColor& Col)
		{
			if ((B.X - A.X) * (C.Y - A.Y) - (B.Y - A.Y) * (C.X - A.X) < 0) Swap(B, C);
			FCanvasUVTri T; T.V0_Pos = A; T.V1_Pos = B; T.V2_Pos = C; T.V0_Color = T.V1_Color = T.V2_Color = Col;
			Tris.Add(T);
		}
		void Flush(UCanvas* Canvas)
		{
			if (!Tris.Num()) return;
			FCanvasTriangleItem Item(Tris, GWhiteTexture);
			Item.BlendMode = SE_BLEND_Translucent;
			Canvas->DrawItem(Item);
			Tris.Reset();
		}
		// a stroke along a polyline as one strip that does not overlap itself (offsets joined at each vertex, miters
		// capped at 4 half widths), so a see-through stroke blends once; round caps as half discs when asked
		void Strip(const TArray<FVector2D>& In, double Width, const FLinearColor& Col, bool bRoundCaps, bool bClosed = false)
		{
			TArray<FVector2D> P;
			for (const FVector2D& Q : In) if (!P.Num() || !Q.Equals(P.Last(), 0.01)) P.Add(Q);
			if (bClosed && P.Num() > 2 && P[0].Equals(P.Last(), 0.01)) P.Pop();
			if (P.Num() < 2) return;
			const double Hw = Width / 2;
			const int32 N = P.Num();
			TArray<FVector2D> L, R;
			for (int32 I = 0; I < N; I++)
			{
				const bool bHasPrev = bClosed || I > 0, bHasNext = bClosed || I + 1 < N;
				const FVector2D Dp = bHasPrev ? (P[I] - P[(I - 1 + N) % N]).GetSafeNormal() : FVector2D::ZeroVector;
				const FVector2D Dn = bHasNext ? (P[(I + 1) % N] - P[I]).GetSafeNormal() : FVector2D::ZeroVector;
				FVector2D D = (Dp + Dn).GetSafeNormal();
				if (D.IsNearlyZero()) D = Dn.IsNearlyZero() ? Dp : Dn;
				const FVector2D Nrm(-D.Y, D.X);
				const FVector2D Ref = !Dn.IsNearlyZero() ? Dn : Dp;
				const double Cos = FMath::Max(0.25, FMath::Abs(Nrm | FVector2D(-Ref.Y, Ref.X)));
				L.Add(P[I] + Nrm * (Hw / Cos)); R.Add(P[I] - Nrm * (Hw / Cos));
			}
			const int32 Segs = bClosed ? N : N - 1;
			for (int32 I = 0; I < Segs; I++)
			{
				const int32 J = (I + 1) % N;
				Tri(L[I], L[J], R[J], Col); Tri(L[I], R[J], R[I], Col);
			}
			if (bRoundCaps && !bClosed)
			{
				auto Cap = [&](FVector2D C, FVector2D Dir)
				{
					const FVector2D Nrm(-Dir.Y, Dir.X);
					constexpr int32 K = 8;
					FVector2D Prev = C + Nrm * Hw;
					for (int32 S = 1; S <= K; S++)
					{
						const double A = PI * S / K;
						const FVector2D Q = C + Nrm * (Hw * FMath::Cos(A)) + Dir * (Hw * FMath::Sin(A));
						Tri(C, Prev, Q, Col); Prev = Q;
					}
				};
				Cap(P.Last(), (P.Last() - P[N - 2]).GetSafeNormal());
				Cap(P[0], (P[0] - P[1]).GetSafeNormal());
			}
		}
		void Disc(FVector2D C, double R, const FLinearColor& Col)
		{
			const int32 K = FMath::Clamp((int32)(R * 0.8), 16, 96);
			for (int32 S = 0; S < K; S++)
			{
				const double A = 2 * PI * S / K, B = 2 * PI * (S + 1) / K;
				Tri(C, C + FVector2D(FMath::Cos(A), FMath::Sin(A)) * R, C + FVector2D(FMath::Cos(B), FMath::Sin(B)) * R, Col);
			}
		}
	};

	// Splits a polyline into dashes (on, off, ... in pixels along it), as setLineDash
	TArray<TArray<FVector2D>> Dashes(const TArray<FVector2D>& P, const TArray<double>& Pattern)
	{
		TArray<TArray<FVector2D>> Out;
		if (!Pattern.Num()) { Out.Add(P); return Out; }
		int32 K = 0; double Left = Pattern[0]; bool bOn = true;
		TArray<FVector2D> Cur;
		if (P.Num()) Cur.Add(P[0]);
		for (int32 I = 0; I + 1 < P.Num(); I++)
		{
			FVector2D A = P[I]; const FVector2D B = P[I + 1];
			double Len = FVector2D::Distance(A, B);
			while (Len > 0)
			{
				const double Step = FMath::Min(Len, Left);
				const FVector2D M = A + (B - A).GetSafeNormal() * Step;
				if (bOn) Cur.Add(M);
				Len -= Step; Left -= Step; A = M;
				if (Left <= 1e-9)
				{
					if (bOn && Cur.Num() > 1) Out.Add(Cur);
					Cur.Reset();
					bOn = !bOn; K = (K + 1) % Pattern.Num(); Left = Pattern[K];
					if (bOn) Cur.Add(A);
				}
			}
		}
		if (bOn && Cur.Num() > 1) Out.Add(Cur);
		return Out;
	}

	void Text(UCanvas* Canvas, UFont* Font, const FString& S, FVector2D At, float Px, const FLinearColor& Col)
	{
		if (!Font || S.IsEmpty()) return;
		// CSS px to Slate's font size (points at 96 dpi); textAlign center, textBaseline middle
		FSlateFontInfo Info(Font, FMath::Max(1, FMath::RoundToInt(Px * 0.75f)), TEXT("Bold"));
		FCanvasTextItem Item(At, FText::FromString(S), Info, Col);
		Item.bCentreX = true; Item.bCentreY = true;
		Item.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(Item);
	}
}

void FTCWarMap::Draw(UCanvas* Canvas, const ATCGameMode& M, const ATCPlayerController& PC)
{
	if (!Canvas || FadeNow <= 0.01f || W <= 0 || H <= 0 || !M.Terrain) return;
	const ATCTerrain& T = *M.Terrain;
	const FTCMatchState& S = M.State;
	FProj P;
	P.SX = Canvas->ClipX; P.SY = Canvas->ClipY;
	if (Canvas->SceneView) { P.VP = Canvas->SceneView->ViewMatrices.GetWorldToClip(); P.bOk = true; }
	if (!P.bOk) return;
	const double MW = W * CellMetres, MH = H * CellMetres;
	const float F = FadeNow;
	auto Known = [&](int32 X, int32 Y) { const int32 C = Y * W + X; return !S.Map.Known.IsValidIndex(C) || S.Map.Known[C]; };
	auto Ground = [&](double X, double Z) -> float
	{
		// the ground the paper lies on; a World cell this side does not know is read as level 0 (its relief is not drawn)
		const int32 Cx = FMath::Clamp((int32)(X / CellMetres), 0, W - 1), Cy = FMath::Clamp((int32)(Z / CellMetres), 0, H - 1);
		if (!Known(Cx, Cy)) return 0.f;
		return T.HeightAt(FMath::Min(X, MW - 0.01), FMath::Min(Z, MH - 0.01));
	};
	TArray<FCanvasUVTri> Tris;
	// the desk: flat at height 0, seven map sizes across, centred on the map
	if (DeskTexture)
	{
		constexpr int32 N = 28;
		const FVector2D C(MW / 2, MH / 2), Half = DeskSize / 2;
		const FLinearColor Col(1, 1, 1, F);
		for (int32 J = 0; J < N; J++) for (int32 I = 0; I < N; I++)
		{
			auto V = [&](int32 A, int32 B) { FCVert O; const double X = C.X - Half.X + DeskSize.X * A / N, Z = C.Y - Half.Y + DeskSize.Y * B / N; O.C = P.Clip(ATCTerrain::ToWorld(X, Z, 0)); O.UV = FVector2D(double(A) / N, double(B) / N); return O; };
			const FCVert A = V(I, J), B = V(I + 1, J), Cc = V(I + 1, J + 1), D = V(I, J + 1);
			AddClipped(P, { A, B, Cc }, Col, Tris); AddClipped(P, { A, Cc, D }, Col, Tris);
		}
		DrawTris(Canvas, Tris, DeskTexture);
	}
	// the paper and the wash, draped over the drawn ground (a vertex every Step cells, at most 160 a side)
	const int32 Step = FMath::Max(1, FMath::CeilToInt(FMath::Max(W, H) / 160.f));
	const int32 Nx = FMath::DivideAndRoundUp(W, Step), Ny = FMath::DivideAndRoundUp(H, Step);
	TArray<FCVert> Grid;
	Grid.SetNum((Nx + 1) * (Ny + 1));
	for (int32 J = 0; J <= Ny; J++) for (int32 I = 0; I <= Nx; I++)
	{
		const double X = FMath::Min(W, I * Step) * CellMetres, Z = FMath::Min(H, J * Step) * CellMetres;
		FCVert& O = Grid[J * (Nx + 1) + I];
		O.C = P.Clip(ATCTerrain::ToWorld(X, Z, Ground(X, Z)));
		O.UV = FVector2D(X / MW, Z / MH);
	}
	auto Sheet = [&](UTexture2D* Tex, const FLinearColor& Col)
	{
		if (!Tex) return;
		for (int32 J = 0; J < Ny; J++) for (int32 I = 0; I < Nx; I++)
		{
			const FCVert& A = Grid[J * (Nx + 1) + I]; const FCVert& B = Grid[J * (Nx + 1) + I + 1];
			const FCVert& C = Grid[(J + 1) * (Nx + 1) + I + 1]; const FCVert& D = Grid[(J + 1) * (Nx + 1) + I];
			AddClipped(P, { A, B, C }, Col, Tris); AddClipped(P, { A, C, D }, Col, Tris);
		}
		DrawTris(Canvas, Tris, Tex);
	};
	Sheet(PaperTexture, FLinearColor(1, 1, 1, F));
	Sheet(WashTexture, FLinearColor(1, 1, 1, F * WashShare));

	// ----- the overlay (map-view.js draw), every element at the fade's opacity -----
	auto Screen = [&](double X, double Z, FVector2D& Out) { return P.At(ATCTerrain::ToWorld(X, Z, Ground(FMath::Clamp(X, 0.0, MW - 0.01), FMath::Clamp(Z, 0.0, MH - 0.01))), Out); };
	FVector2D Mid, East, North;
	if (!Screen(MW / 2, MH / 2, Mid) || !Screen(MW / 2 + 10, MH / 2, East) || !Screen(MW / 2, MH / 2 - 10, North)) return;
	const double Mpx = FVector2D::Distance(East, Mid) / 10; // a metre on the map in screen pixels, at its middle
	UFont* Fnt = Font();
	FFlat Ink;
	LastDrawnUnits = LastDrawnPoints = LastDrawnPencil = 0;
	auto A = [&](FColor C, float Alpha = 1.f) { return Lin(C, Alpha * F); };

	// World regions: the border in the team's colour (1.5 px), the name (and [base]) at the claim point
	if (M.WorldRec.IsWorld())
	{
		const FTCWorldSnapshot& Snap = M.WorldRec.Snapshot();
		for (const FTCWorldRegionView& V : M.WorldRec.RegionViews())
		{
			const FLinearColor C = A(V.TeamColour);
			for (const TPair<FVector2D, FVector2D>& Sg : V.Segments)
			{
				FVector2D Pa, Pb;
				if (Screen(Sg.Key.X, Sg.Key.Y, Pa) && Screen(Sg.Value.X, Sg.Value.Y, Pb)) Ink.Strip({ Pa, Pb }, 1.5, C, false);
			}
			if (!V.Segments.Num() && Snap.Regions.IsValidIndex(V.Index) && Snap.Regions[V.Index].bBounds)
			{
				const double* B = Snap.Regions[V.Index].Bounds;
				TArray<FVector2D> Loop; bool bAll = true;
				for (const FVector2D& Q : { FVector2D(B[0], B[1]), FVector2D(B[2], B[1]), FVector2D(B[2], B[3]), FVector2D(B[0], B[3]) }) { FVector2D S2; bAll &= Screen(Q.X, Q.Y, S2); Loop.Add(S2); }
				if (bAll) Ink.Strip(Loop, 1.5, C, false, true);
			}
		}
		Ink.Flush(Canvas);
		for (const FTCWorldRegionView& V : M.WorldRec.RegionViews())
		{
			FVector2D Sp;
			if (!Screen(V.At.X, V.At.Y, Sp)) continue;
			const bool bLocked = Snap.Regions.IsValidIndex(V.Index) && Snap.Regions[V.Index].bLocked;
			Text(Canvas, Fnt, V.Name + (bLocked ? TEXT(" [base]") : TEXT("")), Sp, 12.f, A(V.TeamColour));
		}
	}
	// capture points: an inked ring, filled in the holder's colour (0x55 alpha), dashed 5 on 3, lettered
	const TArray<FTCCapturePoint>& Points = S.Objectives.Points();
	for (int32 I = 0; I < Points.Num(); I++)
	{
		FVector2D Sp;
		if (!Screen(Points[I].Centre.X, Points[I].Centre.Y, Sp)) continue;
		LastDrawnPoints++;
		const double R = PointRadius * Mpx;
		const int32 Owner = Points[I].bHasRow ? Points[I].Owner : -1;
		Ink.Disc(Sp, R, Owner >= 0 ? Lin(ATCUnitRenderer::SeatColor(Owner).ToFColor(true), (0x55 / 255.f) * F) : A(INK, 0.08f));
		TArray<FVector2D> Ring;
		const int32 K = FMath::Clamp((int32)(R * 0.8), 24, 160);
		for (int32 S2 = 0; S2 <= K; S2++) Ring.Add(Sp + FVector2D(FMath::Cos(2 * PI * S2 / K), FMath::Sin(2 * PI * S2 / K)) * R);
		for (const TArray<FVector2D>& D : Dashes(Ring, { 5.0, 3.0 })) Ink.Strip(D, 2, A(INK), false);
	}
	Ink.Flush(Canvas);
	for (int32 I = 0; I < Points.Num(); I++)
	{
		FVector2D Sp;
		if (Screen(Points[I].Centre.X, Points[I].Centre.Y, Sp)) Text(Canvas, Fnt, FString::Chr(TEXT('A') + I), Sp, 15.f, A(INK));
	}
	// strikes on the way: their box, hatched in red (the strip kinds; a point kind's box has no length, as in the browser)
	for (const FTCStrikeMark& K : M.Powers.Strikes(S))
	{
		if (!K.bKnownKind || !K.bStrip) continue;
		const double C = FMath::Cos(K.Dir), Sn = FMath::Sin(K.Dir), L = K.Len / 2, Wd = K.Width / 2;
		FVector2D Q[4]; bool bAll = true;
		const double Co[4][2] = { { -L, -Wd }, { L, -Wd }, { L, Wd }, { -L, Wd } };
		for (int32 I = 0; I < 4; I++) bAll &= Screen(K.At.X + Co[I][0] * C - Co[I][1] * Sn, K.At.Y + Co[I][0] * Sn + Co[I][1] * C, Q[I]);
		if (!bAll) continue;
		Ink.Strip({ Q[0], Q[1], Q[2], Q[3] }, 2, A(RED), false, true);
		// hatching every 7 px at 45 degrees, clipped to the box (convex: Cyrus-Beck), at half opacity
		double Bx = 1e30, By = 1e30, Ex = -1e30, Ey = -1e30;
		for (const FVector2D& V : Q) { Bx = FMath::Min(Bx, V.X); By = FMath::Min(By, V.Y); Ex = FMath::Max(Ex, V.X); Ey = FMath::Max(Ey, V.Y); }
		double Area = 0;
		for (int32 I = 0; I < 4; I++) Area += Q[I].X * Q[(I + 1) % 4].Y - Q[(I + 1) % 4].X * Q[I].Y;
		for (double Tt = Bx - (Ey - By); Tt < Ex; Tt += 7)
		{
			const FVector2D P0(Tt, Ey), P1(Tt + (Ey - By), By), D = P1 - P0;
			double T0 = 0, T1 = 1; bool bIn = true;
			for (int32 I = 0; I < 4 && bIn; I++)
			{
				const FVector2D E = Q[(I + 1) % 4] - Q[I];
				FVector2D Nin(-E.Y, E.X); if (Area < 0) Nin = -Nin;
				const double Num = Nin | (P0 - Q[I]), Den = Nin | D;
				if (FMath::Abs(Den) < 1e-12) { if (Num < 0) bIn = false; continue; }
				const double Th = -Num / Den;
				if (Den > 0) T0 = FMath::Max(T0, Th); else T1 = FMath::Min(T1, Th);
				if (T0 > T1) bIn = false;
			}
			if (bIn) Ink.Strip({ P0 + D * T0, P0 + D * T1 }, 2, A(RED, 0.5f), false);
		}
	}
	Ink.Flush(Canvas);
	// this seat's orders in grease pencil: the route to where each unit is going, then what is queued after it
	const ATCUnitRenderer* UnitsR = M.Units;
	auto UnitAt = [&](const FTCUnit& U) { FVector2D D(U.X, U.Z); if (UnitsR) UnitsR->DrawnAt(U.Id, D); return D; };
	auto Pencil = [&](TArray<FVector2D> Pts, FColor Col, const TArray<double>& Dash, double Width, int32 Seed)
	{
		if (Pts.Num() < 2) return;
		LastDrawnPencil++;
		// a slightly wobbly line, the same wobble every frame (seeded by the unit's id): quadratic pieces through the
		// points, bent at each middle by up to 3 px; an arrow head at the end; round caps and joins, 85% opaque
		TCWarMapPaint::FMulberry Rnd((uint32)Seed);
		TArray<FVector2D> Line; Line.Add(Pts[0]);
		for (int32 I = 1; I < Pts.Num(); I++)
		{
			const FVector2D Pa = Pts[I - 1], Pb = Pts[I];
			const FVector2D C((Pa.X + Pb.X) / 2 + (Rnd.Next() - 0.5) * 6, (Pa.Y + Pb.Y) / 2 + (Rnd.Next() - 0.5) * 6);
			for (int32 K = 1; K <= 12; K++) { const double Tq = K / 12.0, Sq = 1 - Tq; Line.Add(Pa * (Sq * Sq) + C * (2 * Sq * Tq) + Pb * (Tq * Tq)); }
		}
		const FLinearColor Lc = A(Col, 0.85f);
		for (const TArray<FVector2D>& D : Dashes(Line, Dash)) Ink.Strip(D, Width, Lc, true);
		const FVector2D B = Pts.Last(), Aa = Pts[Pts.Num() - 2];
		const double Ang = FMath::Atan2(B.Y - Aa.Y, B.X - Aa.X), Lh = 11;
		Ink.Strip({ FVector2D(B.X - FMath::Cos(Ang - 0.45) * Lh, B.Y - FMath::Sin(Ang - 0.45) * Lh), B, FVector2D(B.X - FMath::Cos(Ang + 0.45) * Lh, B.Y - FMath::Sin(Ang + 0.45) * Lh) }, Width, Lc, true);
	};
	TMap<int32, const FTCPlan*> PlanOf;
	for (const FTCPlan& Pl : S.Plans) PlanOf.Add(Pl.Id, &Pl);
	TMap<int32, const FTCWaitingOrders*> OrdersOf;
	for (const FTCWaitingOrders& Wo : S.WaitingOrders) OrdersOf.Add(Wo.Id, &Wo);
	TArray<const FTCUnit*> Rows;
	for (const TPair<int32, FTCUnit>& Pr : S.Units) Rows.Add(&Pr.Value);
	Rows.Sort([](const FTCUnit& X, const FTCUnit& Y) { return X.Id < Y.Id; }); // the browser's units map keeps arrival order
	for (const FTCUnit* U : Rows)
	{
		if (U->Owner != S.You) continue;
		const FVector2D At = UnitAt(*U);
		FVector2D From;
		if (!Screen(At.X, At.Y, From)) continue;
		const FTCPlan* const* Pl = PlanOf.Find(U->Id);
		if (Pl && (*Pl)->Kind > 0 && (*Pl)->Kind < 10 && PlanKind[(*Pl)->Kind])
		{
			TArray<FVector2D> Pts; Pts.Add(From);
			for (const FVector2D& Q : (*Pl)->Path) { FVector2D Sq; if (Screen(Q.X, Q.Y, Sq)) Pts.Add(Sq); }
			FVector2D End; if (Screen((*Pl)->Tx, (*Pl)->Tz, End)) Pts.Add(End);
			const FString Kind = PlanKind[(*Pl)->Kind];
			if (Pts.Num() > 1 && FVector2D::Distance(Pts.Last(), From) > 14)
				Pencil(Pts, PencilColour(Kind), Kind == TEXT("retreat") ? TArray<double>{ 8.0, 6.0 } : TArray<double>{}, 3, U->Id);
		}
		FVector2D Last; bool bLast = Pl ? Screen((*Pl)->Tx, (*Pl)->Tz, Last) : (Last = From, true);
		if (const FTCWaitingOrders* const* Wo = OrdersOf.Find(U->Id))
			for (const FTCQueuedOrder& O : (*Wo)->Orders)
			{
				FVector2D Q; const bool bQ = Screen(O.X, O.Z, Q);
				if (bLast && bQ) Pencil({ Last, Q }, PENCIL_MOVE, { 6.0, 6.0 }, 2, U->Id + 7);
				Last = Q; bLast = bQ;
			}
	}
	Ink.Flush(Canvas);
	// every unit as its symbol, in its owner's colour, upright on screen; the selected ones get a pencil ring
	const double Size = 26;
	auto Tile = [&](int32 Index, int32 RowOffset, FVector2D C, double Sz, const FLinearColor& Col)
	{
		if (!SymbolTexture || !SymbolTexture->GetResource() || Index < 0) return;
		// a cell holds the 0..100 box in its middle Box units; drawSymbol puts the box Sz px across centred on C
		const double CellPx = Sz * Atlas.Box / 100.0;
		const double Tw = SymbolTexture->GetSizeX(), Th = SymbolTexture->GetSizeY();
		const int32 Cx = Index % Atlas.Columns, Cy = Index / Atlas.Columns + RowOffset;
		FCanvasTileItem Item(C - FVector2D(CellPx / 2, CellPx / 2), SymbolTexture->GetResource(), FVector2D(CellPx, CellPx),
			FVector2D(Cx * Atlas.Cell / Tw, Cy * Atlas.Cell / Th), FVector2D((Cx + 1) * Atlas.Cell / Tw, (Cy + 1) * Atlas.Cell / Th), Col);
		Item.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(Item);
	};
	const int32* Unknown = Atlas.Index.Find(TEXT("unknown"));
	for (const FTCUnit* U : Rows)
	{
		const FVector2D At = UnitAt(*U);
		FVector2D Sp;
		if (!Screen(At.X, At.Y, Sp)) continue;
		const FTCUnitDef* Def = M.UnitTable.Find(U->Type);
		const double K = Def && Def->bBuilding ? 1.45 : 1.0;
		if (PC.Selected.Contains(U->Id))
		{
			TArray<FVector2D> Ring;
			for (int32 S2 = 0; S2 <= 48; S2++) Ring.Add(Sp + FVector2D(FMath::Cos(2 * PI * S2 / 48), FMath::Sin(2 * PI * S2 / 48)) * (Size * K * 0.62));
			Ink.Strip(Ring, 2, A(INK), false, true);
			Ink.Flush(Canvas);
		}
		const int32* Idx = Atlas.Index.Find(U->Type);
		const int32 I = Idx ? *Idx : Unknown ? *Unknown : -1;
		Tile(I, Atlas.HaloRowsOffset, Sp, Size * K, A(PAPER));
		LastDrawnUnits++;
		Tile(I, 0, Sp, Size * K, Lin(ATCUnitRenderer::SeatColor(U->Owner).ToFColor(true), F));
	}
	// the map's name above its far edge, and a compass in the corner
	FVector2D Edges[4]; bool bE[4];
	bE[0] = Screen(MW / 2, 0, Edges[0]); bE[1] = Screen(MW / 2, MH, Edges[1]); bE[2] = Screen(0, MH / 2, Edges[2]); bE[3] = Screen(MW, MH / 2, Edges[3]);
	int32 Top = -1;
	for (int32 I = 0; I < 4; I++) if (bE[I] && (Top < 0 || Edges[I].Y < Edges[Top].Y)) Top = I;
	if (!Title.IsEmpty() && Top >= 0) Text(Canvas, Fnt, TC_T(Title).ToUpper(), Edges[Top] - FVector2D(0, 22), 20.f, A(TITLE));
	const double Ang = FMath::Atan2(North.Y - Mid.Y, North.X - Mid.X) + PI / 2, Cx = 70, Cy = P.SY - 230, R = 26;
	Ink.Disc(FVector2D(Cx, Cy), R, Lin(PAPER, 0.9f * F));
	{
		TArray<FVector2D> Ring;
		for (int32 S2 = 0; S2 <= 64; S2++) Ring.Add(FVector2D(Cx, Cy) + FVector2D(FMath::Cos(2 * PI * S2 / 64), FMath::Sin(2 * PI * S2 / 64)) * R);
		Ink.Strip(Ring, 2, A(INK), false, true);
	}
	auto Rot = [&](double X, double Y) { return FVector2D(Cx + X * FMath::Cos(Ang) - Y * FMath::Sin(Ang), Cy + X * FMath::Sin(Ang) + Y * FMath::Cos(Ang)); };
	Ink.Tri(Rot(0, -R * 0.8), Rot(R * 0.22, 0), Rot(0, R * 0.8), A(INK)); Ink.Tri(Rot(0, -R * 0.8), Rot(0, R * 0.8), Rot(-R * 0.22, 0), A(INK));
	Ink.Tri(Rot(0, -R * 0.8), Rot(R * 0.22, 0), Rot(-R * 0.22, 0), A(RED));
	Ink.Flush(Canvas);
	Text(Canvas, Fnt, TEXT("N"), Rot(0, -R - 9), 12.f, A(INK));
}

// ---------------------------------------------------------------------------------------------------------------------

UTCWarMapPaintCommandlet::UTCWarMapPaintCommandlet()
{
	IsClient = false; IsEditor = true; IsServer = false; LogToConsole = true;
}

static bool SavePng(const TCWarMapPaint::FPaintCanvas& C, const FString& File)
{
	TArray<FColor> Px;
	Px.SetNumUninitialized(C.W * C.H);
	for (int32 I = 0; I < C.W * C.H; I++) Px[I] = FColor(C.Rgba[I * 4], C.Rgba[I * 4 + 1], C.Rgba[I * 4 + 2], C.Rgba[I * 4 + 3]);
	TArray64<uint8> Png;
	FImageUtils::PNGCompressImageArray(C.W, C.H, TArrayView64<const FColor>(Px.GetData(), Px.Num()), Png);
	return FFileHelper::SaveArrayToFile(Png, *File);
}

#if WITH_EDITOR
// -ImportFont=<ttf> [-FontPackage=/Game/WarMap/Fonts/F_TCWarMapMono]: the font face as an asset (the editor's font import
// factory asks questions through Slate, which a commandlet does not have), loaded inline so a cooked game carries it
static int32 ImportFontFace(const FString& Ttf, const FString& PackageName)
{
	TArray<uint8> Bytes;
	if (!FFileHelper::LoadFileToArray(Bytes, *Ttf)) { UE_LOG(LogTC, Error, TEXT("cannot read %s"), *Ttf); return 1; }
	UPackage* Pkg = CreatePackage(*PackageName);
	UFontFace* Face = NewObject<UFontFace>(Pkg, FName(*FPackageName::GetShortName(PackageName)), RF_Public | RF_Standalone);
	Face->InitializeFromBulkData(Ttf, EFontHinting::Default, Bytes.GetData(), Bytes.Num());
	Face->LoadingPolicy = EFontLoadingPolicy::Inline;
	Face->MarkPackageDirty();
	const FString File = FPackageName::LongPackageNameToFilename(PackageName, FPackageName::GetAssetPackageExtension());
	FSavePackageArgs Args;
	Args.TopLevelFlags = RF_Public | RF_Standalone;
	const bool bOk = UPackage::SavePackage(Pkg, Face, *File, Args);
	UE_LOG(LogTC, Display, TEXT("TCWarMapPaint: font face %s (%d bytes) -> %s: %s"), *Ttf, Bytes.Num(), *File, bOk ? TEXT("saved") : TEXT("FAILED"));
	return bOk ? 0 : 1;
}
#endif

int32 UTCWarMapPaintCommandlet::Main(const FString& Params)
{
	FString MapFile, Out;
#if WITH_EDITOR
	FString Ttf, FontPackage = TEXT("/Game/WarMap/Fonts/F_TCWarMapMono");
	if (FParse::Value(*Params, TEXT("-ImportFont="), Ttf)) { FParse::Value(*Params, TEXT("-FontPackage="), FontPackage); return ImportFontFace(Ttf, FontPackage); }
#endif
	if (!FParse::Value(*Params, TEXT("-Map="), MapFile) || !FParse::Value(*Params, TEXT("-Out="), Out)) { UE_LOG(LogTC, Error, TEXT("usage: -run=TCWarMapPaint -Map=<maps/*.json> -Out=<dir>")); return 2; }
	FString Text;
	TSharedPtr<FJsonObject> Root;
	if (!FFileHelper::LoadFileToString(Text, *MapFile) || !FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root) || !Root.IsValid()) { UE_LOG(LogTC, Error, TEXT("cannot read %s"), *MapFile); return 1; }
	TCWarMapPaint::FPaperInput In;
	In.W = Root->GetIntegerField(TEXT("w")); In.H = Root->GetIntegerField(TEXT("h"));
	const TArray<TSharedPtr<FJsonValue>>& Rows = Root->GetArrayField(TEXT("rows"));
	const TArray<TSharedPtr<FJsonValue>>& Heights = Root->GetArrayField(TEXT("heights"));
	In.Chars.Init('.', In.W * In.H); In.Level.Init(0, In.W * In.H);
	for (int32 Y = 0; Y < In.H && Y < Rows.Num(); Y++)
	{
		const FString R = Rows[Y]->AsString(), Hh = Heights.IsValidIndex(Y) ? Heights[Y]->AsString() : FString();
		for (int32 X = 0; X < In.W; X++)
		{
			if (X < R.Len()) In.Chars[Y * In.W + X] = (uint8)R[X];
			// shared/sim.js levelOf; the oracle's hAt is the cell's level x 2.5, so round(hAt / 2.5) is the level
			if (X < Hh.Len()) In.Level[Y * In.W + X] = FTCMatchState::LevelOf(Hh[X]);
		}
	}
	const FString Name = FPaths::GetBaseFilename(MapFile);
	TCWarMapPaint::FPaintCanvas Paper, Desk;
	double T0 = FPlatformTime::Seconds();
	TCWarMapPaint::PaintPaper(In, Paper);
	const double PaperMs = (FPlatformTime::Seconds() - T0) * 1000;
	T0 = FPlatformTime::Seconds();
	const double M = FMath::Max(In.W, In.H);
	TCWarMapPaint::PaintDesk(Desk, 2048, 7, In.W / M, In.H / M);
	const double DeskMs = (FPlatformTime::Seconds() - T0) * 1000;
	const bool bOk = SavePng(Paper, FPaths::Combine(Out, Name + TEXT("-paper.png"))) && SavePng(Desk, FPaths::Combine(Out, Name + TEXT("-desk.png")));
	UE_LOG(LogTC, Display, TEXT("TCWarMapPaint %s: paper %d x %d in %.1f ms, desk 2048 in %.1f ms -> %s"), *Name, Paper.W, Paper.H, PaperMs, DeskMs, bOk ? *Out : TEXT("(save failed)"));
	return bOk ? 0 : 1;
}

#if WITH_DEV_AUTOMATION_TESTS

// ThreeCrossroads.WarMap.Paint: the port's pieces against client/map-view.js's numbers.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWarMapPaintTest, "ThreeCrossroads.WarMap.Paint", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCWarMapPaintTest::RunTest(const FString& Parameters)
{
	using namespace TCWarMapPaint;
	// mulberry(7) and mulberry(3), the first values as node prints them for map-view.js's function
	FMulberry R7(7), R3(3);
	TestEqual(TEXT("mulberry(7) first"), R7.Next(), 0.011704753153026104, 1e-15);
	TestEqual(TEXT("mulberry(7) second"), R7.Next(), 0.061958257574588060, 1e-15);
	TestEqual(TEXT("mulberry(3) first"), R3.Next(), 0.72022678377106786, 1e-15);
	// PX: max(2, min(12, floor(2048 / max(w, h))))
	TestEqual(TEXT("PX small map"), PixelsPerCell(110, 120), 12);
	TestEqual(TEXT("PX World huge"), PixelsPerCell(512, 512), 4);
	TestEqual(TEXT("PX World massive"), PixelsPerCell(1024, 512), 2);
	TestEqual(TEXT("PX floor"), PixelsPerCell(4096, 10), 2);
	// the fade: 0 below 0.7 of the widest view, 1 from 0.9, smoothstepped
	TestEqual(TEXT("fade at 0.7"), FTCWarMap::WantFade(0.7f), 0.f);
	TestEqual(TEXT("fade at 0.8"), FTCWarMap::WantFade(0.8f), 0.5f, 1e-5f);
	TestEqual(TEXT("fade at 0.9"), FTCWarMap::WantFade(0.9f), 1.f);
	// exact-area coverage: a rectangle at half-pixel edges covers its edge pixels half
	FPaintCanvas C;
	C.Init(8, 8, FRgba{ 255, 255, 255, 1 });
	C.FillRect(2.5, 2, 3, 3, Css(0, 0, 0));
	TestTrue(TEXT("half-covered edge pixel"), FMath::Abs((int32)C.Rgba[(3 * 8 + 2) * 4] - (128)) <= 1);
	TestEqual(TEXT("fully covered pixel"), (int32)C.Rgba[(3 * 8 + 3) * 4], 0);
	TestEqual(TEXT("outside"), (int32)C.Rgba[(3 * 8 + 6) * 4], 255);
	// a 1 px line on a pixel boundary covers the two columns beside it half each, as a browser canvas draws it
	C.Init(8, 8, FRgba{ 255, 255, 255, 1 });
	C.BeginPath(); C.MoveTo(4, 0); C.LineTo(4, 8); C.Stroke(Css(0, 0, 0), 1);
	TestTrue(TEXT("line left half"), FMath::Abs((int32)C.Rgba[(4 * 8 + 3) * 4] - (128)) <= 1);
	TestTrue(TEXT("line right half"), FMath::Abs((int32)C.Rgba[(4 * 8 + 4) * 4] - (128)) <= 1);
	// crossing subpaths of one stroke blend once where they cross
	C.Init(9, 9, FRgba{ 255, 255, 255, 1 });
	C.BeginPath(); C.MoveTo(0, 4.5); C.LineTo(9, 4.5); C.MoveTo(4.5, 0); C.LineTo(4.5, 9); C.Stroke(Css(0, 0, 0, 0.5), 1);
	TestTrue(TEXT("crossing blended once"), FMath::Abs((int32)C.Rgba[(4 * 9 + 4) * 4] - ((int32)C.Rgba[(4 * 9 + 1) * 4])) <= 1);
	// a tiny paper: water, wood, a house and the frame land where map-view.js puts them; an unknown cell stays paper
	FPaperInput In;
	In.W = 20; In.H = 20;
	In.Chars.Init('.', 400); In.Level.Init(0, 400);
	In.Chars[5 * 20 + 5] = 'W'; In.Chars[5 * 20 + 7] = 'B'; In.Chars[8 * 20 + 8] = 'O'; In.Chars[12 * 20 + 12] = 'B';
	In.Known.Init(true, 400); In.Known[12 * 20 + 12] = false;
	FPaintCanvas Pp;
	PaintPaper(In, Pp);
	TestEqual(TEXT("paper size"), Pp.W, 240);
	auto Px = [&](int32 X, int32 Y) { const uint8* P = Pp.Rgba.GetData() + (Y * Pp.W + X) * 4; return FColor(P[0], P[1], P[2]); };
	const FColor Water = Px(5 * 12 + 6, 5 * 12 + 2), House = Px(7 * 12 + 6, 5 * 12 + 6), Unknown = Px(12 * 12 + 6, 12 * 12 + 6);
	TestTrue(TEXT("water is blue-grey"), FMath::Abs(Water.B - 0xc4) < 20 && Water.B > Water.R);
	TestTrue(TEXT("a house is ink"), FMath::Abs(House.R - 0x3b) < 6 && FMath::Abs(House.G - 0x32) < 6);
	TestTrue(TEXT("an unknown house cell stays paper"), Unknown.R > 180 && Unknown.G > 170);
	return true;
}

#endif
