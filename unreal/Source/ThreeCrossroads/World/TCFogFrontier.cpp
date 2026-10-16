#include "World/TCFogFrontier.h"
#include "ThreeCrossroads.h"
#include "Net/TCMatchState.h"
#include "Client/TCUnitTable.h"
#include "Engine/World.h"
#include "Engine/Texture2D.h"
#include "GameFramework/Actor.h"
#include "Materials/MaterialInterface.h"
#include "Materials/Material.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/MaterialParameterCollection.h"
#include "Materials/MaterialParameterCollectionInstance.h"
#include "ProceduralMeshComponent.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"

const TCHAR* const FTCFogFrontier::ReferencePin = TEXT("e4e579a838aabae3b1d9dadbc17a0a87147bc52d");
// unlit, Color (unreal/Tools/look/make-look-assets.py): the overlay's colour where the ground is opaque
const TCHAR* const FTCFogFrontier::UnknownMaterialPath = TEXT("/Game/Models/Materials/Look/M_TCLookEmissive.M_TCLookEmissive");

namespace TCWaterView
{
	AActor* SpawnHolder(UWorld* World, const TCHAR* Name, TStrongObjectPtr<UProceduralMeshComponent>& OutMesh);
}

FTCFogFrontier::FTCFogFrontier()
{
	FParse::Value(FCommandLine::Get(), TEXT("TCFogTime="), FixedTime);
}
FTCFogFrontier::~FTCFogFrontier()
{
	if (TCFogLook::Active() == this) TCFogLook::SetActive(nullptr);
}

void FTCFogFrontier::Attach(UWorld* World)
{
	if (Actor.IsValid() || !World) return;
	AActor* A = TCWaterView::SpawnHolder(World, TEXT("FogUnknownGround"), CoverMesh);
	if (!A) return;
	Actor = A;
	CoverOverlay.Attach(World);
	ClockCollection.Reset(LoadObject<UMaterialParameterCollection>(nullptr, TEXT("/Game/Fog/MPC_TCFog.MPC_TCFog")));
#if WITH_EDITOR
	// These three masters sample the new raw alpha texture. Complete their shaders before any
	// scenery instances draw, rather than displaying the editor's default grid during compilation.
	for (const TCHAR* Path : { TEXT("/Game/World/Materials/M_TCWorldSurface.M_TCWorldSurface"), TEXT("/Game/World/Materials/M_TCWorldLambert.M_TCWorldLambert"), TEXT("/Game/World/Materials/M_TCWorldLeaf.M_TCWorldLeaf") })
		TCFogLook::PrepareMaterial(LoadObject<UMaterialInterface>(nullptr, Path));
#endif
	CoverMesh->SetVisibility(false);
	if (UMaterialInterface* M = LoadObject<UMaterialInterface>(nullptr, UnknownMaterialPath))
	{
		TCFogLook::PrepareMaterial(M);
		CoverMid.Reset(UMaterialInstanceDynamic::Create(M, GetTransientPackage()));
		// fog.js SHADE: new THREE.Color(10 / 255, ...) sets the working (linear) colour directly
		CoverMid->SetVectorParameterValue(TEXT("Color"), FLinearColor(TCFogFrontierCore::Shade / 255.f, TCFogFrontierCore::Shade / 255.f, TCFogFrontierCore::Shade / 255.f, 1.f));
	}
	else UE_LOG(LogTC, Warning, TEXT("fog frontier: %s does not load; World Conquest's unknown ground is not drawn"), UnknownMaterialPath);
}

void FTCFogFrontier::Detach()
{
	CoverOverlay.Detach();
	if (AActor* A = Actor.Get()) A->Destroy();
	Actor.Reset(); CoverMesh.Reset(); CoverMid.Reset();
	Clear();
}

void FTCFogFrontier::Clear()
{
	Core = TCFogFrontierCore::FFog();
	Tex.Reset(); AlphaTex.Reset(); TexBytes.Reset(); Clock = 0;
	CoverKnown.Reset(); CoverCells = 0; bCoverDirty = false; Snapshots = 0;
	if (CoverMesh.IsValid()) { CoverMesh->ClearAllMeshSections(); CoverMesh->SetVisibility(false); }
	CoverOverlay.Clear();
}

void FTCFogFrontier::Start(const FTCMatchState& State, bool bOff)
{
	const int32 W = State.Map.W, H = State.Map.H, N = W * H;
	if (W <= 0 || H <= 0) { Clear(); return; }
	// the start's mask: none for a spectator of an authored map (server.js sendStart: fog undefined)
	const bool bKey = !State.bFogLifted && State.FogVisible.Num() == N;
	TArray<uint8> Seen, Ever;
	if (bKey)
	{
		Seen.Init(0, N); Ever.Init(0, N);
		for (int32 C = 0; C < N; C++) { Seen[C] = State.FogVisible[C] ? 1 : 0; Ever[C] = State.FogExplored.IsValidIndex(C) && State.FogExplored[C] ? 1 : 0; }
	}
	Core.Start(W, H, bKey ? Seen.GetData() : nullptr, bKey ? Ever.GetData() : nullptr, bOff, State.Map.bWorld);
	Clock = 0;
	Core.TakeChanged();
	Snapshots = 0;
	MakeTexture();
	CoverKnown.Reset(); bCoverDirty = true;
	RebuildCover(State);
	UE_LOG(LogTC, Log, TEXT("fog frontier: start %dx%d, %s, %s"), W, H, bKey ? TEXT("with a mask") : TEXT("no mask (lifted)"), Core.bOpaque ? TEXT("World: opaque unknown ground, frontier darkening") : TEXT("authored map"));
}

void FTCFogFrontier::Snapshot(const FTCMatchState& State, const TSet<int32>& FogDirty, const FTCUnitTable& Table)
{
	using namespace TCFogFrontierCore;
	if (!Core.N || State.Map.W != Core.W || State.Map.H != Core.H) return;
	Snapshots++;
	// s.fog: the cells whose seen bit flipped (what this fog holds against the decoded mask)
	std::vector<int> Flips;
	if (State.FogVisible.Num() == Core.N)
		for (const int32 C : FogDirty)
			if (C >= 0 && C < Core.N && (State.FogVisible[C] ? 1 : 0) != Core.Seen[C]) Flips.push_back(C);
	// s.units: every ground unit the snapshot holds (fog.js: UNITS[type], skipped when unknown or in the air)
	std::vector<FFootprint> Units;
	Units.reserve(State.Units.Num());
	for (const auto& Pair : State.Units)
	{
		const FTCUnit& U = Pair.Value;
		const FTCUnitDef* Def = Table.Find(U.Type);
		if (!Def || Def->bAir) continue;
		const int32 Size = Def->bBuilding ? FMath::Max(1, BuildingSize(TCHAR_TO_ANSI(*U.Type))) : 1;
		Units.push_back(Footprint(U.X, U.Z, Size));
	}
	Core.Snapshot(Flips, Units);
	// World: newly discovered cells leave the unknown ground
	if (Core.bOpaque && State.Map.Known.Num() == Core.N)
	{
		if (CoverKnown.Num() != Core.N) bCoverDirty = true;
		else for (int32 C = 0; C < Core.N && !bCoverDirty; C++) if (CoverKnown[C] != State.Map.Known[C]) bCoverDirty = true;
		if (bCoverDirty) RebuildCover(State);
	}
}

void FTCFogFrontier::Frame(float DeltaSeconds, bool bLift, const FTCMatchState& State)
{
	if (!Core.N || State.Map.W != Core.W || State.Map.H != Core.H) return;
	Core.Frame(FMath::Max(0.f, DeltaSeconds), bLift, [this](int C) { Moved(C); });
	Clock += FMath::Max(0.f, DeltaSeconds);
	if (Actor.IsValid() && ClockCollection.IsValid())
		if (UMaterialParameterCollectionInstance* C = Actor->GetWorld()->GetParameterCollectionInstance(ClockCollection.Get()))
			C->SetScalarParameterValue(TEXT("FogTime"), PresentationTime());
	Upload(false);
	CoverOverlay.Flush(State);
}

TSet<int32> FTCFogFrontier::TakeChanged()
{
	TSet<int32> Out;
	for (const int C : Core.TakeChanged()) Out.Add(C);
	return Out;
}

int32 FTCFogFrontier::FrontierCells() const
{
	int32 N = 0;
	for (const uint8 E : Core.Edge) N += E > 0;
	return N;
}

void FTCFogFrontier::MakeTexture()
{
	const int32 W = Core.W, H = Core.H;
	TexBytes.SetNumUninitialized(W * H);
	for (int32 C = 0; C < W * H; C++) TexBytes[C] = TCFogFrontierCore::MultiplierByte(Core.Bytes[C]);
	if (!Tex.IsValid() || Tex->GetSizeX() != W || Tex->GetSizeY() != H)
	{
		UTexture2D* T = UTexture2D::CreateTransient(W, H, PF_G8);
		if (!T) { Tex.Reset(); return; }
		T->SRGB = false;
		T->Filter = TF_Bilinear; // fog.js: LinearFilter, so the look ramps between cell centres
		T->AddressX = T->AddressY = TA_Clamp;
		T->NeverStream = true;
		T->UpdateResource();
		Tex.Reset(T);
	}
	if (!AlphaTex.IsValid() || AlphaTex->GetSizeX() != W || AlphaTex->GetSizeY() != H)
	{
		UTexture2D* T = UTexture2D::CreateTransient(W, H, PF_G8);
		if (T)
		{
			T->SRGB = false;
			T->Filter = TF_Bilinear;
			T->AddressX = T->AddressY = TA_Clamp;
			T->NeverStream = true;
			T->LODGroup = TEXTUREGROUP_Pixels2D;
			T->UpdateResource(); AlphaTex.Reset(T);
		}
	}
	Upload(true);
}

void FTCFogFrontier::Moved(int32 C)
{
	const uint8 B = TCFogFrontierCore::MultiplierByte(Core.Bytes[C]);
	if (!TexBytes.IsValidIndex(C)) return;
	TexBytes[C] = B;
	const int32 X = C % Core.W, Y = C / Core.W;
	Rx0 = FMath::Min(Rx0, X); Rx1 = FMath::Max(Rx1, X); Ry0 = FMath::Min(Ry0, Y); Ry1 = FMath::Max(Ry1, Y);
}

void FTCFogFrontier::Upload(bool bAll)
{
	UTexture2D* T = Tex.Get();
	if (!T || TexBytes.Num() != Core.W * Core.H) return;
	if (bAll) { Rx0 = 0; Ry0 = 0; Rx1 = Core.W - 1; Ry1 = Core.H - 1; }
	if (Rx1 < Rx0 || Ry1 < Ry0) return;
	const int32 RW = Rx1 - Rx0 + 1, RH = Ry1 - Ry0 + 1;
	uint8* Copy = new uint8[(size_t)RW * RH];
	for (int32 Y = 0; Y < RH; Y++) FMemory::Memcpy(Copy + (size_t)Y * RW, TexBytes.GetData() + (int64)(Ry0 + Y) * Core.W + Rx0, RW);
	FUpdateTextureRegion2D* Region = new FUpdateTextureRegion2D(Rx0, Ry0, 0, 0, RW, RH);
	T->UpdateTextureRegions(0, 1, Region, RW, 1, Copy, [](uint8* Data, const FUpdateTextureRegion2D* Regions) { delete[] Data; delete Regions; });
	if (UTexture2D* A = AlphaTex.Get())
	{
		uint8* Alphas = new uint8[(size_t)RW * RH];
		for (int32 Y = 0; Y < RH; Y++) FMemory::Memcpy(Alphas + (size_t)Y * RW, Core.Bytes.data() + (int64)(Ry0 + Y) * Core.W + Rx0, RW);
		FUpdateTextureRegion2D* R = new FUpdateTextureRegion2D(Rx0, Ry0, 0, 0, RW, RH);
		A->UpdateTextureRegions(0, 1, R, RW, 1, Alphas, [](uint8* D, const FUpdateTextureRegion2D* Regions) { delete[] D; delete Regions; });
	}
	Rx0 = Core.W; Ry0 = Core.H; Rx1 = Ry1 = -1;
}

// ---------------------------------------------------------------------------------------------------- overlay

const TCHAR* const FTCFogOverlay::MaterialPath = TEXT("/Game/Fog/M_TCFogOverlay.M_TCFogOverlay");
FTCFogOverlay::FTCFogOverlay() = default;
FTCFogOverlay::~FTCFogOverlay() { Detach(); }

void FTCFogOverlay::Attach(UWorld* World)
{
	if (!World || Actor.IsValid()) return;
	AActor* A = TCWaterView::SpawnHolder(World, TEXT("ReferenceFogOverlay"), Mesh);
	if (!A) return;
	Actor = A;
	Mesh->SetTranslucentSortPriority(1); // main.js / apron.js renderOrder 1, water -1
	Mesh->SetVisibility(false);
	if (UMaterialInterface* M = LoadObject<UMaterialInterface>(nullptr, MaterialPath))
	{
		TCFogLook::PrepareMaterial(M);
		Mid.Reset(UMaterialInstanceDynamic::Create(M, GetTransientPackage()));
		Mesh->SetMaterial(0, Mid.Get());
	}
	else UE_LOG(LogTC, Error, TEXT("fog: %s missing, regenerate with Tools/world/make-fog-materials.py"), MaterialPath);
}

void FTCFogOverlay::Detach()
{
	if (AActor* A = Actor.Get()) A->Destroy();
	Actor.Reset(); Mesh.Reset(); Mid.Reset();
}
void FTCFogOverlay::Clear() { if (Mesh.IsValid()) { Mesh->ClearAllMeshSections(); Mesh->SetVisibility(false); } }
void FTCFogOverlay::ClearSection(int32 Section) { if (Mesh.IsValid()) Mesh->ClearMeshSection(Section); }

void FTCFogOverlay::SetSection(int32 Section, const TArray<FVector>& Positions, const TArray<int32>& Indices, const TArray<FVector>& Normals)
{
	if (!Mesh.IsValid() || !Mid.IsValid()) return;
	if (Indices.IsEmpty()) { ClearSection(Section); return; }
	// The material nudges depth toward the view, like the source polygon offset. Moving only along
	// normals leaves uncovered slivers where a vertical frontier join meets the water plane.
	Mesh->CreateMeshSection_LinearColor(Section, Positions, Indices, Normals, TArray<FVector2D>(), TArray<FLinearColor>(), TArray<FProcMeshTangent>(), false);
	Mesh->SetMaterial(Section, Mid.Get());
}

void FTCFogOverlay::Flush(const FTCMatchState& State, bool bOuter)
{
	const FTCFogFrontier* F = TCFogLook::Active();
	const bool bDraw = F && F->HasMap() && F->MapW() == State.Map.W && F->MapH() == State.Map.H && F->AlphaTexture();
	if (!Mesh.IsValid() || !Mid.IsValid()) return;
	Mesh->SetVisibility(bDraw);
	if (!bDraw) return;
	Mid->SetTextureParameterValue(TEXT("FogAlpha"), F->AlphaTexture());
	Mid->SetVectorParameterValue(TEXT("FogSize"), FLinearColor(State.Map.W * 2.f, State.Map.H * 2.f, 0, 0));
	Mid->SetScalarParameterValue(TEXT("FogTime"), F->PresentationTime());
	Mid->SetScalarParameterValue(TEXT("Outer"), bOuter ? 1.f : 0.f);
	if (FParse::Param(FCommandLine::Get(), TEXT("TCFogAudit")))
	{
		static TSet<const UProceduralMeshComponent*> Audited;
		if (Mesh->GetNumSections() && !Audited.Contains(Mesh.Get()))
		{
			Audited.Add(Mesh.Get());
			int32 Lo = 255, Hi = 0, Verts = 0;
			for (const uint8 A : F->CoreState().Bytes) { Lo = FMath::Min(Lo,(int32)A); Hi = FMath::Max(Hi,(int32)A); }
			for (int32 I=0;I<Mesh->GetNumSections();I++) if (const FProcMeshSection* S=Mesh->GetProcMeshSection(I)) Verts+=S->ProcVertexBuffer.Num();
			UE_LOG(LogTC, Log, TEXT("fog overlay audit: draw %d actor hidden %d visible %d sections %d vertices %d alpha %d..%d outer %d material %s complete %d"), bDraw, Actor->IsHidden(), Mesh->IsVisible(), Mesh->GetNumSections(), Verts, Lo, Hi, bOuter, *GetNameSafe(Mid->GetMaterial()), Mid->GetMaterial()->IsComplete());
		}
	}
}

void FTCFogFrontier::RebuildCover(const FTCMatchState& State)
{
	bCoverDirty = false;
	CoverKnown = State.Map.Known;
	CoverCells = 0;
	if (!CoverMesh.IsValid()) return;
	if (!Core.bOpaque || !bDrawUnknown || !CoverMid.IsValid() || State.Map.Known.Num() != Core.N)
	{
		CoverMesh->ClearAllMeshSections(); CoverMesh->SetVisibility(false);
		CoverOverlay.Clear();
		return;
	}
	// One quad per run of unknown cells in a row, flat at level 0 (the start map's '.' at level 0), facing up: the
	// browser's placeholder ground under its opaque overlay (fog.js dark 255 on World, colour SHADE).
	TArray<FVector> Verts, Normals;
	TArray<int32> Tris;
	const int32 W = Core.W, H = Core.H;
	const double M = TCFogFrontierCore::Cell * 100.0;
	for (int32 Y = 0; Y < H; Y++)
	{
		for (int32 X = 0; X < W;)
		{
			if (State.Map.Known[Y * W + X]) { X++; continue; }
			int32 E = X;
			while (E < W && !State.Map.Known[Y * W + E]) E++;
			CoverCells += E - X;
			const int32 B = Verts.Num();
			// three (x, 0, z) -> Unreal (x, z, 0): p00, p10, p01, p11; (p00, p01, p11) and (p00, p11, p10) face up in three
			Verts.Add(FVector(X * M, Y * M, 0)); Verts.Add(FVector(E * M, Y * M, 0)); Verts.Add(FVector(X * M, (Y + 1) * M, 0)); Verts.Add(FVector(E * M, (Y + 1) * M, 0));
			for (int32 K = 0; K < 4; K++) Normals.Add(FVector::UpVector);
			Tris.Append({ B, B + 2, B + 3, B, B + 3, B + 1 });
			X = E;
		}
	}
	CoverMesh->ClearAllMeshSections();
	if (!Tris.Num()) { CoverMesh->SetVisibility(false); CoverOverlay.Clear(); return; }
	CoverMesh->CreateMeshSection_LinearColor(0, Verts, Tris, Normals, TArray<FVector2D>(), TArray<FLinearColor>(), TArray<FProcMeshTangent>(), false);
	CoverMesh->SetMaterial(0, CoverMid.Get());
	CoverMesh->SetVisibility(true);
	// Water is translucent and sits above this opaque placeholder. As in main.js, the fog must draw
	// after that water, including its foam over undiscovered cells at the edge of a known river.
	CoverOverlay.SetSection(0, Verts, Tris, Normals);
}

FString FTCFogFrontier::StatusLine() const
{
	if (!Core.N) return TEXT("fog frontier: no map");
	return FString::Printf(TEXT("fog frontier: %dx%d %s%s, snapshots %d, lit cells %d, frontier cells %d, fading %d, unknown ground %d cells"),
		Core.W, Core.H, Core.bOpaque ? TEXT("World") : TEXT("authored"), Core.bLifted ? TEXT(" lifted") : TEXT(""), Snapshots, LitCells(), FrontierCells(), (int32)Core.ActiveList.size(), CoverCells);
}

// ---------------------------------------------------------------------------------------------------- shared look

namespace TCFogLook
{
	void PrepareMaterial(UMaterialInterface* Material)
	{
#if WITH_EDITOR
		if (Material) Material->GetMaterial()->EnsureIsComplete();
#endif
	}
	static const FTCFogFrontier* GActive = nullptr;
	void SetActive(const FTCFogFrontier* Frontier) { GActive = Frontier; }
	const FTCFogFrontier* Active() { return GActive; }
	static bool Holds(const FTCMatchState& State) { return GActive && GActive->HasMap() && GActive->MapW() == State.Map.W && GActive->MapH() == State.Map.H; }
	uint8 Byte(const FTCMatchState& State, bool bReveal, int32 C)
	{
		if (bReveal) return 255;
		if (Holds(State)) return GActive->MultiplierByte(C);
		return State.CellVisible(C) ? 255 : State.CellExplored(C) ? 140 : 82;
	}
	uint8 AlphaByte(const FTCMatchState& State, bool bReveal, int32 C)
	{
		if (bReveal) return 0;
		if (Holds(State)) return (uint8)GActive->AlphaNow(C);
		return State.CellVisible(C) ? 0 : State.CellExplored(C) ? 120 : State.Map.bWorld ? 255 : 185;
	}
	float Multiplier(const FTCMatchState& State, bool bReveal, int32 C)
	{
		const uint8 B = Byte(State, bReveal, C);
		return B == 255 ? 1.f : B == 140 ? 0.55f : B == 82 ? 0.32f : B / 255.f;
	}
}

// ---------------------------------------------------------------------------------------------------- tests

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "World/TCFogFrontier.Oracle.inl"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFogFrontierTest, "ThreeCrossroads.World.FogFrontier", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCFogFrontierTest::RunTest(const FString& Parameters)
{
	using namespace TCFogFrontierCore;
	// the native multiplier bytes for the three looks stay FTCCellFog's
	TestEqual(TEXT("multiplier: seen"), (int32)MultiplierByte(0), 255);
	TestEqual(TEXT("multiplier: explored"), (int32)MultiplierByte(Dim), 140);
	TestEqual(TEXT("multiplier: never seen"), (int32)MultiplierByte(Dark), 82);
	TestEqual(TEXT("multiplier: opaque"), (int32)MultiplierByte(Opaque), 17);
	auto Ints = [](const FString& S, int32 From) { TArray<int32> Out; TArray<FString> P; S.Mid(From).ParseIntoArray(P, TEXT(" ")); for (const FString& T : P) Out.Add(FCString::Atoi(*T)); return Out; };
	auto Rle = [](const FString& S, int32 From) {
		TArray<int32> Out; FString Body = S.Mid(From); FString Count, Rest; Body.Split(TEXT(":"), &Count, &Rest);
		TArray<FString> P; Rest.ParseIntoArray(P, TEXT(" "));
		for (const FString& T : P) { FString V, Nn; if (T.Split(TEXT("*"), &V, &Nn)) { for (int32 I = 0, K = FCString::Atoi(*Nn); I < K; I++) Out.Add(FCString::Atoi(*V)); } else Out.Add(FCString::Atoi(*T)); }
		return Out.Num() == FCString::Atoi(*Count) ? Out : TArray<int32>(); };
	FFog F;
	TArray<int32> KeyV;
	int32 Scenarios = 0, Steps = 0, Off = 0, Checked = 0, W = 0, H = 0;
	bool bOpaque = false, bOffMap = false, bKey = false;
	std::vector<int> PendingFlips;
	TArray<double> Dts;
	bool bLift = false;
	FString Name;
	auto Compare = [&](const TArray<int32>& Want, TFunctionRef<int32(int32)> Got, const TCHAR* What)
	{
		if (Want.Num() != W * H) { AddError(FString::Printf(TEXT("%s %s: %d values for %d cells"), *Name, What, Want.Num(), W * H)); Off++; return; }
		for (int32 C = 0; C < W * H; C++) { Checked++; if (Got(C) != Want[C]) { if (Off < 8) AddError(FString::Printf(TEXT("%s step %d %s cell %d: %d, reference %d"), *Name, Steps, What, C, Got(C), Want[C])); Off++; } }
	};
	for (const TCHAR* Line : TCFogFrontierOracle)
	{
		const FString L(Line);
		const TCHAR Kind = L[0];
		if (Kind == 'B')
		{
			TArray<FString> P; L.Mid(2).ParseIntoArray(P, TEXT(" "));
			for (const FString& T : P) { FString K, V; T.Split(TEXT("="), &K, &V); TestEqual(*FString::Printf(TEXT("footprint of %s"), *K), BuildingSize(TCHAR_TO_ANSI(*K)), FCString::Atoi(*V)); }
			TestEqual(TEXT("building footprints: as many as the reference has"), (int32)UE_ARRAY_COUNT(BuildingSizes), P.Num());
		}
		else if (Kind == 'S')
		{
			TArray<FString> P; L.Mid(2).ParseIntoArray(P, TEXT(" "));
			Name = P[0]; W = FCString::Atoi(*P[1]); H = FCString::Atoi(*P[2]); bOpaque = P[3] == TEXT("1"); bOffMap = P[4] == TEXT("1"); bKey = P[5] == TEXT("1");
			Scenarios++; Steps = -1;
			if (!bKey) F.Start(W, H, nullptr, nullptr, bOffMap, bOpaque);
		}
		else if (Kind == 'V') KeyV = Ints(L, 2);
		else if (Kind == 'E')
		{
			std::vector<uint8_t> Seen(W * H, 0), Ever(W * H, 0);
			for (const int32 C : KeyV) Seen[C] = 1;
			for (const int32 C : Ints(L, 2)) Ever[C] = 1;
			F.Start(W, H, Seen.data(), Ever.data(), bOffMap, bOpaque);
		}
		else if (Kind == 'I') Compare(Rle(L, 2), [&](int32 C) { return (int32)F.Bytes[C]; }, TEXT("start alpha"));
		else if (Kind == 'F')
		{
			Steps++;
			const TArray<int32> Flips = L.Len() > 2 ? Ints(L, 2) : TArray<int32>();
			PendingFlips.assign(Flips.GetData(), Flips.GetData() + Flips.Num());
		}
		else if (Kind == 'U')
		{
			// one snapshot: the flips of the line before and the ground units of this one
			std::vector<FFootprint> Units;
			TArray<FString> P; L.Mid(2).ParseIntoArray(P, TEXT(" "));
			for (int32 I = 0; I + 2 < P.Num(); I += 3)
			{
				const int32 B = BuildingSize(TCHAR_TO_ANSI(*P[I]));
				Units.push_back(Footprint(FCString::Atod(*P[I + 1]), FCString::Atod(*P[I + 2]), B > 0 ? B : 1));
			}
			F.Snapshot(PendingFlips, Units);
			PendingFlips.clear();
		}
		else if (Kind == 'L')
		{
			TArray<FString> P; L.Mid(2).ParseIntoArray(P, TEXT(" "));
			bLift = P[0] == TEXT("1");
			Dts.Reset();
			for (int32 I = 1; I < P.Num(); I++) Dts.Add(FCString::Atod(*P[I]));
		}
		else if (Kind == 'A')
		{
			// the frames up to and including the 0.05 s one (the oracle's 0.016, 0.05, 0.1 sequence)
			for (const double Dt : Dts) { F.Frame(Dt, bLift); if (Dt == 0.05) break; }
			Compare(Rle(L, 2), [&](int32 C) { return (int32)F.Bytes[C]; }, TEXT("alpha after 0.05 s"));
			bool bPast = false;
			for (const double Dt : Dts) { if (bPast) F.Frame(Dt, bLift); if (Dt == 0.05) bPast = true; }
			F.Frame(1.0, bLift);
		}
		else if (Kind == 'T') Compare(Rle(L, 2), [&](int32 C) { return (int32)F.Bytes[C]; }, TEXT("settled alpha"));
		else if (Kind == 'M') Compare(Rle(L, 2), [&](int32 C) { return F.MinimapAlpha(C); }, TEXT("minimap alpha"));
	}
	AddInfo(FString::Printf(TEXT("fog.js oracle: %d scenarios, %d values, %d off"), Scenarios, Checked, Off));
	TestEqual(TEXT("values off the reference fog.js"), Off, 0);
	TestTrue(TEXT("the oracle was read"), Scenarios == 3 && Checked > 100000);
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
