#include "World/TCScatter.h"

const TCHAR* const FTCScatter::ReferencePin = TEXT("e4e579a");

const TCHAR* FTCScatter::CatalogKey(TCScatterCore::EProp Kind)
{
	using TCScatterCore::EProp;
	switch (Kind)
	{
	case EProp::Deciduous: return TEXT("foliage/broadleaf|-1");
	case EProp::Poplar: return TEXT("foliage/poplar|-1");
	case EProp::Pine: return TEXT("foliage/pine|-1");
	case EProp::Bush: return TEXT("foliage/bush|-1");
	case EProp::Grass: return TEXT("foliage/grass|-1");
	case EProp::Crop: return TEXT("foliage/crop|-1");
	case EProp::Rocks: return TEXT("prop/rocks|-1");
	case EProp::Fence: return TEXT("prop/fence|-1");
	case EProp::Haystack: return TEXT("prop/haystack|-1");
	case EProp::Supplies: return TEXT("prop/supplies|-1");
	default: return TEXT("");
	}
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/Compression.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Components/HierarchicalInstancedStaticMeshComponent.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "EngineUtils.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Net/TCMatchState.h"
#include "World/TCTerrain.h"
#include "World/TCWorldObjects.h"
#include <map>
#include <set>

namespace TCScatterTests
{
	// The fixture written by unreal/Tools/world/scenery-oracle.mjs: the pinned client/structures.js and client/props.js
	// run under Node, with their inputs. Regenerate it with that script (the header says how).
	TSharedPtr<FJsonObject> LoadFixture(FString& OutError)
	{
		const FString Path = FPaths::Combine(FPaths::ProjectDir(), TEXT("Tools/world/fixtures/scenery-oracle.json.gz"));
		TArray<uint8> Gz;
		if (!FFileHelper::LoadFileToArray(Gz, *Path) || Gz.Num() < 18) { OutError = TEXT("cannot read ") + Path; return nullptr; }
		const uint32 Size = (uint32)Gz[Gz.Num() - 4] | (uint32)Gz[Gz.Num() - 3] << 8 | (uint32)Gz[Gz.Num() - 2] << 16 | (uint32)Gz[Gz.Num() - 1] << 24; // gzip ISIZE
		TArray<uint8> Raw;
		Raw.SetNumUninitialized((int64)Size + 1);
		if (!FCompression::UncompressMemory(NAME_Gzip, Raw.GetData(), Size, Gz.GetData(), Gz.Num())) { OutError = TEXT("cannot gunzip ") + Path; return nullptr; }
		Raw[Size] = 0;
		const FString Text = UTF8_TO_TCHAR((const ANSICHAR*)Raw.GetData());
		TSharedPtr<FJsonObject> Root;
		if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root) || !Root.IsValid()) { OutError = TEXT("cannot parse ") + Path; return nullptr; }
		return Root;
	}
	double Wave(double X, double Z) { return 0.6 * FMath::Sin(X * 0.13) + 0.4 * FMath::Cos(Z * 0.09) + 0.002 * X; }
	std::vector<std::string> Rows(const FJsonObject& O, const TCHAR* Field)
	{
		std::vector<std::string> Out;
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (O.TryGetArrayField(Field, A)) for (const TSharedPtr<FJsonValue>& V : *A) Out.push_back(TCHAR_TO_UTF8(*V->AsString()));
		return Out;
	}
	std::vector<int> Ints(const FJsonObject& O, const TCHAR* Field)
	{
		std::vector<int> Out;
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (O.TryGetArrayField(Field, A)) for (const TSharedPtr<FJsonValue>& V : *A) Out.push_back((int)V->AsNumber());
		return Out;
	}
	struct FInput
	{
		int W = 0, H = 0;
		bool bLow = false, bWorld = false;
		std::vector<std::string> Live, Orig, Map, Heights;
		std::vector<int> Cells, Mines, Skip, Biome;
		std::vector<std::pair<int, std::pair<int, std::string>>> Bld;
		std::vector<std::pair<double, double>> Nodes;
		std::vector<TCScatterCore::FCellXY> Spawns, Points;
		std::string Disc;
	};
	FInput ReadInput(const FJsonObject& Case)
	{
		FInput In;
		const FJsonObject& O = *Case.GetObjectField(TEXT("input"));
		In.W = (int)O.GetNumberField(TEXT("w")); In.H = (int)O.GetNumberField(TEXT("h"));
		In.bLow = Case.GetBoolField(TEXT("low"));
		In.bWorld = O.GetBoolField(TEXT("world"));
		In.Live = Rows(O, TEXT("live")); In.Orig = Rows(O, TEXT("orig")); In.Map = Rows(O, TEXT("rows")); In.Heights = Rows(O, TEXT("heights"));
		In.Cells = Ints(O, TEXT("cells")); In.Mines = Ints(O, TEXT("mines")); In.Skip = Ints(O, TEXT("skip")); In.Biome = Ints(O, TEXT("biome"));
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (O.TryGetArrayField(TEXT("buildingsUsed"), A))
			for (const TSharedPtr<FJsonValue>& V : *A)
			{
				const FJsonObject& B = *V->AsObject();
				In.Bld.push_back({ (int)B.GetNumberField(TEXT("x")), { (int)B.GetNumberField(TEXT("y")), TCHAR_TO_UTF8(*B.GetStringField(TEXT("kind"))) } });
			}
		if (O.TryGetArrayField(TEXT("nodes"), A)) for (const TSharedPtr<FJsonValue>& V : *A) In.Nodes.push_back({ V->AsArray()[0]->AsNumber(), V->AsArray()[1]->AsNumber() });
		auto Xy = [&O](const TCHAR* Field, std::vector<TCScatterCore::FCellXY>& Out)
		{
			const TArray<TSharedPtr<FJsonValue>>* L;
			if (O.TryGetArrayField(Field, L))
				for (const TSharedPtr<FJsonValue>& V : *L)
				{
					const TSharedPtr<FJsonObject>* P;
					if (!V.IsValid() || !V->TryGetObject(P)) continue;
					TCScatterCore::FCellXY C;
					C.X = (int)(*P)->GetNumberField(TEXT("x")); C.Y = (int)(*P)->GetNumberField(TEXT("y"));
					Out.push_back(C);
				}
		};
		Xy(TEXT("spawns"), In.Spawns); Xy(TEXT("points"), In.Points);
		FString Disc;
		if (O.TryGetStringField(TEXT("discovered"), Disc)) In.Disc = TCHAR_TO_UTF8(*Disc);
		return In;
	}

	class FSrc final : public TCWorldCore::FCellSource
	{
	public:
		const FInput& C;
		std::set<int> Stone, SpoilSet, MineSet;
		explicit FSrc(const FInput& In) : C(In)
		{
			for (const int M : C.Mines) MineSet.insert(M);
			std::vector<std::pair<int, int>> Q;
			for (const auto& B : C.Bld)
			{
				const int X = B.first, Y = B.second.first;
				if (B.second.second == "spoil") SpoilSet.insert(Y * C.W + X);
				if (B.second.second == "stone bridge" && At(X, Y) == '=') { Q.push_back({ X, Y }); Stone.insert(Y * C.W + X); }
			}
			const int D[4][2] = { { 1, 0 }, { -1, 0 }, { 0, 1 }, { 0, -1 } };
			for (size_t I = 0; I < Q.size(); I++) for (const auto& Dd : D)
			{
				const int Nx = Q[I].first + Dd[0], Ny = Q[I].second + Dd[1], K = Ny * C.W + Nx;
				if (!Stone.count(K) && At(Nx, Ny) == '=') { Stone.insert(K); Q.push_back({ Nx, Ny }); }
			}
		}
		bool In(int X, int Y) const { return X >= 0 && Y >= 0 && X < C.W && Y < C.H; }
		int Width() const override { return C.W; }
		int Height() const override { return C.H; }
		unsigned char At(int X, int Y) const override { return In(X, Y) ? (unsigned char)C.Live[(size_t)Y][(size_t)X] : 0; }
		unsigned char Orig(int X, int Y) const override { return In(X, Y) ? (unsigned char)C.Orig[(size_t)Y][(size_t)X] : 0; }
		int Stage(int X, int Y) const override { return In(X, Y) && !C.Cells.empty() ? (C.Cells[(size_t)(Y * C.W + X)] >> 3) & 3 : 0; }
		bool Mine(int X, int Y) const override { return In(X, Y) && (C.Mines.empty() ? At(X, Y) == 'N' : MineSet.count(Y * C.W + X) > 0); }
		bool StoneDeck(int X, int Y) const override { return In(X, Y) && Stone.count(Y * C.W + X) > 0; }
		bool Spoil(int X, int Y) const override { return In(X, Y) && SpoilSet.count(Y * C.W + X) > 0; }
		double Ground(double X, double Z) const override { return Wave(X, Z); }
		bool Low() const override { return C.bLow; }
	};
	class FLive final : public TCScatterCore::FLiveSource
	{
	public:
		const FInput& C;
		std::set<int> SkipSet;
		explicit FLive(const FInput& In) : C(In) { for (const int S : C.Skip) SkipSet.insert(S); }
		int Width() const override { return C.W; }
		int Height() const override { return C.H; }
		unsigned char At(int X, int Y) const override { return X >= 0 && Y >= 0 && X < C.W && Y < C.H ? (unsigned char)C.Live[(size_t)Y][(size_t)X] : 0; }
		int Level(int X, int Y) const override { return X >= 0 && Y >= 0 && Y < (int)C.Heights.size() && X < (int)C.Heights[(size_t)Y].size() ? TCScatterCore::LevelOf((unsigned char)C.Heights[(size_t)Y][(size_t)X]) : 0; }
		double Ground(double X, double Z) const override { return Wave(X, Z); }
		bool Skip(int Cell) const override { return SkipSet.count(Cell) > 0; }
		bool Low() const override { return C.bLow; }
	};
	bool Near(double A, double B) { return FMath::Abs(A - B) <= 2e-4 + 1e-5 * FMath::Abs(B); }
	const TArray<TSharedPtr<FJsonValue>>& Cases(const FJsonObject& Root) { return Root.GetArrayField(TEXT("cases")); }
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorldPiecesOracleTest, "ThreeCrossroads.World.PiecesOracle", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCWorldPiecesOracleTest::RunTest(const FString& Parameters)
{
	using namespace TCScatterTests;
	FString Error;
	const TSharedPtr<FJsonObject> Root = LoadFixture(Error);
	if (!Root.IsValid()) { AddError(Error); return false; }
	TestEqual(TEXT("fixture reference"), Root->GetStringField(TEXT("reference")), FString(FTCWorldObjects::ReferencePin));
	int32 CasesChecked = 0, Instances = 0, Bad = 0;
	for (const TSharedPtr<FJsonValue>& V : Cases(*Root))
	{
		const FJsonObject& Case = *V->AsObject();
		const TArray<TSharedPtr<FJsonValue>>* Want;
		if (!Case.TryGetArrayField(TEXT("pieces"), Want)) continue;
		const FString Name = Case.GetStringField(TEXT("name"));
		const FInput In = ReadInput(Case);
		const FSrc Src(In);
		std::map<int, int> Named;
		for (const auto& B : In.Bld)
		{
			const int T = TCWorldCore::HouseTypeOf(B.second.second.c_str());
			if (T >= 0 && B.first >= 0 && B.second.first >= 0 && B.first < In.W && B.second.first < In.H) Named[B.second.first * In.W + B.first] = T;
		}
		std::vector<TCWorldCore::FPiece> Pieces;
		TCWorldCore::BuildAllPieces(Src, [&Named](int Cell) { const auto It = Named.find(Cell); return It == Named.end() ? -1 : It->second; }, Pieces);
		std::map<int, std::vector<const TCWorldCore::FPiece*>> ByKind;
		for (const TCWorldCore::FPiece& P : Pieces) ByKind[(int)P.Kind].push_back(&P);
		int32 Kinds = 0;
		for (const TSharedPtr<FJsonValue>& K : *Want)
		{
			const FJsonObject& Kind = *K->AsObject();
			const int KindIndex = TCWorldCore::PieceOfName(TCHAR_TO_UTF8(*Kind.GetStringField(TEXT("kind"))));
			const TArray<TSharedPtr<FJsonValue>>& Rows = Kind.GetArrayField(TEXT("instances"));
			const std::vector<const TCWorldCore::FPiece*>& Got = ByKind[KindIndex];
			Kinds++;
			if (!TestEqual(FString::Printf(TEXT("%s: %s count"), *Name, *Kind.GetStringField(TEXT("kind"))), (int32)Got.size(), Rows.Num())) { Bad++; continue; }
			for (int32 I = 0; I < Rows.Num(); I++)
			{
				const TArray<TSharedPtr<FJsonValue>>& R = Rows[I]->AsArray();
				double M[16];
				TCWorldCore::PieceMatrix(*Got[(size_t)I], M);
				const double Col[3] = { Got[(size_t)I]->R, Got[(size_t)I]->G, Got[(size_t)I]->B };
				bool bOk = R.Num() == 19;
				for (int32 J = 0; bOk && J < 16; J++) bOk = Near((float)M[J], R[J]->AsNumber());
				for (int32 J = 0; bOk && J < 3; J++) bOk = Near(Col[J], R[16 + J]->AsNumber());
				Instances++;
				if (!bOk && Bad++ < 20) AddError(FString::Printf(TEXT("%s: %s[%d] differs from the reference"), *Name, *Kind.GetStringField(TEXT("kind")), I));
			}
		}
		// no kind drawn that the reference does not draw
		int32 OurKinds = 0;
		for (const auto& P : ByKind) OurKinds += P.second.empty() ? 0 : 1;
		TestEqual(FString::Printf(TEXT("%s: kinds drawn"), *Name), OurKinds, Kinds);
		CasesChecked++;
	}
	AddInfo(FString::Printf(TEXT("pieces: %d cases, %d instances compared with client/structures.js, %d differ"), CasesChecked, Instances, Bad));
	TestTrue(TEXT("cases checked"), CasesChecked >= 10);
	TestEqual(TEXT("piece instances that differ from the reference"), Bad, 0);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorldScatterOracleTest, "ThreeCrossroads.World.ScatterOracle", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCWorldScatterOracleTest::RunTest(const FString& Parameters)
{
	using namespace TCScatterTests;
	using namespace TCScatterCore;
	FString Error;
	const TSharedPtr<FJsonObject> Root = LoadFixture(Error);
	if (!Root.IsValid()) { AddError(Error); return false; }
	// the integer seeds are the reference's (props.js hash) on a few known values
	// props.js hash on known values (computed by node from the pinned function's text)
	TestEqual(TEXT("hash(0, 0, 71)"), (int64)Hash(0, 0, 71), (int64)1513834590);
	TestEqual(TEXT("hash(12, 34, 94)"), (int64)Hash(12, 34, 94), (int64)4194469992);
	TestEqual(TEXT("hash(-3, 7, 101)"), (int64)Hash(-3, 7, 101), (int64)2840804956);
	TestEqual(TEXT("hash(79, 0, 144)"), (int64)Hash(79, 0, 144), (int64)1935011290);
	int32 CasesChecked = 0, Rows_ = 0, Bad = 0, Shown = 0;
	for (const TSharedPtr<FJsonValue>& V : Cases(*Root))
	{
		const FJsonObject& Case = *V->AsObject();
		const TSharedPtr<FJsonObject>* Props;
		if (!Case.TryGetObjectField(TEXT("props"), Props)) continue;
		const FString Name = Case.GetStringField(TEXT("name"));
		const FInput In = ReadInput(Case);
		FScatterMap M;
		M.W = In.W; M.H = In.H; M.Rows = In.Map; M.Spawns = In.Spawns; M.Points = In.Points; M.bWorld = In.bWorld;
		for (const int B : In.Biome) M.Biome.push_back((unsigned char)B);
		for (const char Ch : In.Disc) M.Discovered.push_back(Ch == '1' ? 1 : 0);
		const std::vector<FCandidate> List = Candidates(M);
		const TArray<TSharedPtr<FJsonValue>>& Want = (*Props)->GetArrayField(TEXT("candidates"));
		const TArray<TSharedPtr<FJsonValue>>& WantPlaced = (*Props)->GetArrayField(TEXT("placed"));
		if (!TestEqual(FString::Printf(TEXT("%s: candidates"), *Name), (int32)List.size(), Want.Num())) { Bad++; continue; }
		const FLive Live(In);
		for (int32 I = 0; I < Want.Num(); I++)
		{
			const TArray<TSharedPtr<FJsonValue>>& R = Want[I]->AsArray();
			const FCandidate& C = List[(size_t)I];
			const double Got[12] = { (double)C.Cx, (double)C.Cy, C.X, C.Z, (double)C.Seed, C.Angle, C.Scale, (double)C.AnchorX, (double)C.AnchorY, (double)C.Biome, C.bOrchard ? 1.0 : 0.0, C.bWood ? 1.0 : 0.0 };
			bool bOk = R.Num() == 13 && R[0]->AsString() == ANSI_TO_TCHAR(PropName(C.Kind));
			for (int32 J = 0; bOk && J < 12; J++) bOk = Near(Got[J], R[J + 1]->AsNumber());
			Rows_++;
			if (!bOk && Bad++ < 20) AddError(FString::Printf(TEXT("%s: candidate %d differs from the reference"), *Name, I));
			const FPlaced P = Place(C, Live, In.Nodes);
			const TSharedPtr<FJsonValue>& WP = WantPlaced[I];
			const bool bWantShown = WP.IsValid() && WP->Type == EJson::Array;
			Rows_++;
			if (bWantShown != P.bShown) { if (Bad++ < 20) AddError(FString::Printf(TEXT("%s: placement %d shown %d, reference %d"), *Name, I, P.bShown, bWantShown)); continue; }
			if (!P.bShown) continue;
			Shown++;
			const TArray<TSharedPtr<FJsonValue>>& Q = WP->AsArray();
			bOk = Q.Num() == 21 && Q[0]->AsString() == ANSI_TO_TCHAR(PropName(P.Kind));
			for (int32 J = 0; bOk && J < 16; J++) bOk = Near(P.M[J], Q[J + 1]->AsNumber());
			for (int32 J = 0; bOk && J < 3; J++) bOk = Near(P.Col[J], Q[17 + J]->AsNumber());
			if (bOk) bOk = Near(P.Bark, Q[20]->AsNumber());
			if (!bOk && Bad++ < 20) AddError(FString::Printf(TEXT("%s: placement %d differs from the reference"), *Name, I));
		}
		CasesChecked++;
	}
	AddInfo(FString::Printf(TEXT("scatter: %d cases, %d candidate and placement rows compared with client/props.js (%d shown), %d differ"), CasesChecked, Rows_, Shown, Bad));
	TestTrue(TEXT("cases checked"), CasesChecked >= 8);
	TestEqual(TEXT("scatter rows that differ from the reference"), Bad, 0);
	return true;
}

namespace TCScatterTests
{
	struct FScratchWorld
	{
		UWorld* World = nullptr;
		explicit FScratchWorld(const TCHAR* Name)
		{
			World = UWorld::CreateWorld(EWorldType::Game, false, Name);
			FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
			Context.SetCurrentWorld(World);
			World->InitializeActorsForPlay(FURL());
		}
		~FScratchWorld()
		{
			GEngine->DestroyWorldContext(World);
			World->DestroyWorld(false);
		}
	};
	// A start message for the fixture's first case (Three Crossroads), with extra cells.
	TSharedRef<FJsonObject> StartOf(const FJsonObject& Case, const TArray<TSharedPtr<FJsonValue>>& Cells)
	{
		const FJsonObject& O = *Case.GetObjectField(TEXT("input"));
		TSharedRef<FJsonObject> Map = MakeShared<FJsonObject>();
		Map->SetNumberField(TEXT("w"), O.GetNumberField(TEXT("w")));
		Map->SetNumberField(TEXT("h"), O.GetNumberField(TEXT("h")));
		Map->SetStringField(TEXT("name"), TEXT("Three Crossroads"));
		Map->SetArrayField(TEXT("rows"), O.GetArrayField(TEXT("rows")));
		Map->SetArrayField(TEXT("heights"), O.GetArrayField(TEXT("heights")));
		Map->SetArrayField(TEXT("spawns"), O.GetArrayField(TEXT("spawns")));
		Map->SetArrayField(TEXT("points"), O.GetArrayField(TEXT("points")));
		TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
		M->SetStringField(TEXT("t"), TEXT("start"));
		M->SetObjectField(TEXT("map"), Map);
		M->SetNumberField(TEXT("you"), 0);
		M->SetNumberField(TEXT("matchId"), 7);
		M->SetArrayField(TEXT("cells"), Cells);
		return M;
	}
	TArray<TSharedPtr<FJsonValue>> CellRow(int32 Cell, const TCHAR* Ch, int32 Level)
	{
		return { MakeShared<FJsonValueNumber>(Cell), MakeShared<FJsonValueString>(Ch), MakeShared<FJsonValueNumber>(Level), MakeShared<FJsonValueNumber>(0) };
	}
	struct FAll { TArray<TCWorldCore::FPiece> Pieces; int32 Props = 0; };
	FAll Everything(const FTCWorldObjects& O)
	{
		FAll A;
		for (int32 K = 0; K < O.ChunkCount(); K++)
		{
			O.ForEachPiece(K, [&A](const TCWorldCore::FPiece& P) { A.Pieces.Add(P); });
			O.ForEachProp(K, [&A](const TCScatterCore::FCandidate&, const TCScatterCore::FPlaced&) { A.Props++; });
		}
		return A;
	}
	// field by field (FPiece has padding after its kind)
	bool SamePiece(const TCWorldCore::FPiece& P, const TCWorldCore::FPiece& Q)
	{
		return P.Kind == Q.Kind && P.Cell == Q.Cell && P.X == Q.X && P.Y == Q.Y && P.Z == Q.Z && P.Sx == Q.Sx && P.Sy == Q.Sy && P.Sz == Q.Sz
			&& P.Ry == Q.Ry && P.Rx == Q.Rx && P.Rz == Q.Rz && P.R == Q.R && P.G == Q.G && P.B == Q.B;
	}
	bool SamePieces(const TArray<TCWorldCore::FPiece>& A, const TArray<TCWorldCore::FPiece>& B)
	{
		if (A.Num() != B.Num()) return false;
		for (int32 I = 0; I < A.Num(); I++) if (!SamePiece(A[I], B[I])) return false;
		return true;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorldObjectsViewTest, "ThreeCrossroads.World.ObjectsView", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCWorldObjectsViewTest::RunTest(const FString& Parameters)
{
	using namespace TCScatterTests;
	const FTCGraphicsLevelScope HighLevel(ETCGraphicsLevel::High); // round 15: the fixture's case is High (the Low cases: GraphicsLowOracle)
	FString Error;
	const TSharedPtr<FJsonObject> Root = LoadFixture(Error);
	if (!Root.IsValid()) { AddError(Error); return false; }
	const FJsonObject& Case = *Cases(*Root)[0]->AsObject(); // default.json as drawn
	int32 WantPieces = 0, WantShown = 0, WantPicks = 0;
	for (const TSharedPtr<FJsonValue>& K : Case.GetArrayField(TEXT("pieces")))
	{
		const int32 N = K->AsObject()->GetArrayField(TEXT("instances")).Num();
		WantPieces += N;
		if (TCWorldCore::PiecePick((TCWorldCore::EPiece)TCWorldCore::PieceOfName(TCHAR_TO_UTF8(*K->AsObject()->GetStringField(TEXT("kind")))))) WantPicks += N;
	}
	for (const TSharedPtr<FJsonValue>& P : Case.GetObjectField(TEXT("props"))->GetArrayField(TEXT("placed"))) WantShown += P.IsValid() && P->Type == EJson::Array ? 1 : 0;

	FScratchWorld Scratch(TEXT("TCWorldObjectsTest"));
	ATCTerrain* Terrain = Scratch.World->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity);
	if (!TestNotNull(TEXT("terrain"), Terrain)) return false;
	auto Start = [&](FTCMatchState& State, FTCWorldObjects& Objects, const TArray<TSharedPtr<FJsonValue>>& Cells)
	{
		const TSharedRef<FJsonObject> M = StartOf(Case, Cells);
		TestTrue(TEXT("match state takes the start"), State.ApplyStart(*M));
		Terrain->ResetFrom(State);
		for (int32 I = 0; I < 64 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
		TestTrue(TEXT("world objects take the start"), Objects.ApplyStart(*M, State));
		Objects.Update(*Terrain, nullptr);
	};
	FTCMatchState State;
	FTCWorldObjects Objects;
	Start(State, Objects, {});
	// the counts do not depend on the ground: the same as the reference's on this map (heights only move pieces up)
	TestEqual(TEXT("pieces as the reference builds them"), Objects.ShapeCount(), WantPieces);
	TestEqual(TEXT("pickable pieces"), Objects.PickableCount(), WantPicks);
	TestEqual(TEXT("scenery candidates"), Objects.CandidateCount(), Case.GetObjectField(TEXT("props"))->GetArrayField(TEXT("candidates")).Num());
	// the oracle's supply nodes, as a snapshot sends them: the props on them go
	{
		TArray<TSharedPtr<FJsonValue>> Nodes;
		for (const TSharedPtr<FJsonValue>& N : Case.GetObjectField(TEXT("input"))->GetArrayField(TEXT("nodes")))
			Nodes.Add(MakeShared<FJsonValueArray>(TArray<TSharedPtr<FJsonValue>>{ N->AsArray()[0], N->AsArray()[1], MakeShared<FJsonValueNumber>(2), MakeShared<FJsonValueNumber>(0) }));
		TSharedRef<FJsonObject> Snap = MakeShared<FJsonObject>();
		Snap->SetArrayField(TEXT("nodes"), Nodes);
		Objects.ApplySnapshot(*Snap);
		Objects.Update(*Terrain, nullptr);
	}
	TestEqual(TEXT("props shown as the reference shows them"), Objects.PropsShown(), WantShown);

	// the house pick: a ray straight down onto a standing house's middle finds a house cell (before any is shot: a house with a fallen cell is a ruin, which houseAt cannot pick)
	int32 HouseCell = -1;
	for (int32 C = 0; C < Objects.MapW() * Objects.MapH() && HouseCell < 0; C++) if (Objects.Char(C) == 'B') HouseCell = C;
	if (HouseCell >= 0)
	{
		const FVector At = ATCTerrain::ToWorld((HouseCell % Objects.MapW() + 0.5f) * 2.f, (HouseCell / Objects.MapW() + 0.5f) * 2.f, 60.f);
		FTCHousePick Pick;
		FVector2D Gnd;
		const bool bG = Terrain->RayToGround(At, FVector(0, 0, -1), Gnd);
		AddInfo(FString::Printf(TEXT("pick: house cell %d at (%.1f, %.1f, %.1f) cm, ground ray %d at (%.2f, %.2f) height %.2f, pickable %d"), HouseCell, At.X, At.Y, At.Z, bG, Gnd.X, Gnd.Y, Terrain->HeightAt(Gnd.X, Gnd.Y), Objects.PickableCount()));
		TestTrue(TEXT("the pick finds the house from above"), Objects.PickHouse(At, FVector(0, 0, -1), Terrain, Pick) && Objects.IsHouseCell(Pick.Cell));
	}

	// incremental: every other house cell shot to rubble and a hedge cut, in a snapshot, equals a start that had them
	TArray<TSharedPtr<FJsonValue>> Changes;
	const int32 W = Objects.MapW();
	int32 Houses = 0;
	for (int32 C = 0; C < W * Objects.MapH(); C++)
	{
		if (Objects.Char(C) == 'B' && (C % W + C / W) % 2 == 0) { Changes.Add(MakeShared<FJsonValueArray>(CellRow(C, TEXT("R"), Objects.LevelAt(C % W, C / W)))); Houses++; }
		else if (Objects.Char(C) == 'H' && C % 7 == 0) Changes.Add(MakeShared<FJsonValueArray>(CellRow(C, TEXT("."), Objects.LevelAt(C % W, C / W))));
	}
	TestTrue(TEXT("some house cells to shoot"), Houses > 4);
	{
		TSharedRef<FJsonObject> Snap = MakeShared<FJsonObject>();
		Snap->SetArrayField(TEXT("cells"), Changes);
		State.ApplySnapshot(*Snap);
		Objects.ApplySnapshot(*Snap);
		Terrain->MarkCells(State.DirtyCells, State.FogDirtyCells); State.DirtyCells.Reset(); State.FogDirtyCells.Reset();
		for (int32 I = 0; I < 64 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
		Objects.Update(*Terrain, nullptr);
	}
	FTCMatchState State2;
	FTCWorldObjects Fresh;
	Start(State2, Fresh, Changes);
	const FAll A = Everything(Objects), B = Everything(Fresh);
	if (!TestTrue(TEXT("a change redoes exactly what a fresh start builds"), SamePieces(A.Pieces, B.Pieces)))
		for (int32 I = 0, Shown = 0; I < FMath::Min(A.Pieces.Num(), B.Pieces.Num()) && Shown < 6; I++)
			if (!SamePiece(A.Pieces[I], B.Pieces[I]))
			{
				const TCWorldCore::FPiece &P = A.Pieces[I], &Q = B.Pieces[I];
				AddInfo(FString::Printf(TEXT("piece %d: changed %s cell %d (%.3f %.3f %.3f) s(%.3f %.3f %.3f) r(%.3f %.3f %.3f) c(%.3f %.3f %.3f) | fresh %s cell %d (%.3f %.3f %.3f) s(%.3f %.3f %.3f) r(%.3f %.3f %.3f) c(%.3f %.3f %.3f)"), I,
					ANSI_TO_TCHAR(TCWorldCore::PieceName(P.Kind)), P.Cell, P.X, P.Y, P.Z, P.Sx, P.Sy, P.Sz, P.Ry, P.Rx, P.Rz, P.R, P.G, P.B,
					ANSI_TO_TCHAR(TCWorldCore::PieceName(Q.Kind)), Q.Cell, Q.X, Q.Y, Q.Z, Q.Sx, Q.Sy, Q.Sz, Q.Ry, Q.Rx, Q.Rz, Q.R, Q.G, Q.B));
				Shown++;
			}
	TestEqual(TEXT("pieces after the change"), A.Pieces.Num(), B.Pieces.Num());
	TestEqual(TEXT("pickable pieces after the change"), Objects.PickableCount(), Fresh.PickableCount());
	TestEqual(TEXT("houses after the change"), Objects.HouseCount(), Fresh.HouseCount());
	int32 Ruins = 0;
	for (const TCWorldCore::FPiece& P : A.Pieces) Ruins += P.Kind == TCWorldCore::EPiece::BrokenBrick || P.Kind == TCWorldCore::EPiece::Broken || P.Kind == TCWorldCore::EPiece::BrokenWood || P.Kind == TCWorldCore::EPiece::BrokenStone ? 1 : 0;
	TestTrue(TEXT("ruins and rubble stand where houses were shot"), Ruins > 0);
	// no piece stands on a cell the map does not have, and every piece has its cell
	int32 Homeless = 0;
	for (const TCWorldCore::FPiece& P : A.Pieces) Homeless += P.Cell < 0 || P.Cell >= W * Objects.MapH() ? 1 : 0;
	TestEqual(TEXT("pieces without a cell"), Homeless, 0);

	// the view: the reference's models, instanced, with the world materials and the fog texture
	FTCWorldObjectsView View;
	TestTrue(TEXT("view attaches"), View.Attach(Scratch.World));
	State.Factions = { 0, 1 };
	FTCGhost Ghost;
	Ghost.Id = 90; Ghost.Type = TEXT("hq"); Ghost.Owner = 1; Ghost.X = 40; Ghost.Z = 40; Ghost.Built = 0.5;
	State.Ghosts.Add(Ghost);
	{
		TSharedRef<FJsonObject> Snap = MakeShared<FJsonObject>();
		Snap->SetArrayField(TEXT("wrecks"), { MakeShared<FJsonValueArray>(TArray<TSharedPtr<FJsonValue>>{ MakeShared<FJsonValueNumber>(91), MakeShared<FJsonValueString>(TEXT("tank")),
			MakeShared<FJsonValueNumber>(1), MakeShared<FJsonValueNumber>(60), MakeShared<FJsonValueNumber>(60), MakeShared<FJsonValueNumber>(0.5) }) });
		Objects.ApplySnapshot(*Snap);
		Objects.Update(*Terrain, nullptr);
	}
	for (int32 I = 0; I < 64 && (I == 0 || View.Queued()); I++) View.Draw(Objects, State, Terrain);
	TestEqual(TEXT("every region built"), View.Queued(), 0);
	TestTrue(TEXT("instanced components drawn"), View.SectionsDrawn() > 10);
	const FString Line = View.StatusLine();
	AddInfo(TEXT("world view: ") + Line);
	TestTrue(TEXT("the ghost and the wreck are drawn with their models"), Line.Contains(TEXT("(1 wrecks, 1 ghosts, 0 without a model)")));
	TestTrue(TEXT("the world materials are there"), Line.EndsWith(TEXT("world materials ok")));
	int32 Hism = 0, Instances = 0, WorldMat = 0;
	for (TActorIterator<AActor> It(Scratch.World); It; ++It)
		for (UActorComponent* C : It->GetComponents())
			if (UHierarchicalInstancedStaticMeshComponent* H = Cast<UHierarchicalInstancedStaticMeshComponent>(C))
			{
				if (!H->GetInstanceCount()) continue;
				Hism++;
				Instances += H->GetInstanceCount();
				if (const UMaterialInstanceDynamic* Mid = Cast<UMaterialInstanceDynamic>(H->GetMaterial(0))) WorldMat += Mid->Parent && Mid->Parent->GetPathName().StartsWith(TEXT("/Game/World/")) ? 1 : 0;
			}
	TestEqual(TEXT("instances drawn: every piece and every prop shown"), Instances, A.Pieces.Num() + A.Props);
	TestEqual(TEXT("every instanced component uses a world material"), WorldMat, Hism);
	View.Detach();
	return true;
}

// ---------------------------------------------------------------------------------------------------- round 15: Graphics Low
#include "Client/TCGraphics.h"
#include "World/TCGroundPaint.h"
#include "World/TCRelief.h"
#include "World/TCWater.h"

namespace TCScatterTests
{
	// unreal/Tools/graphics/fixtures/gfx-low-oracle.json.gz (unreal/Tools/graphics/gfx-low-oracle.mjs): the pinned
	// props.js, structures.js and gfx.js under Node on six authored maps, High, switched to Low mid-match, built on Low
	TSharedPtr<FJsonObject> LoadLowFixture(FString& OutError)
	{
		const FString Path = FPaths::Combine(FPaths::ProjectDir(), TEXT("Tools/graphics/fixtures/gfx-low-oracle.json.gz"));
		TArray<uint8> Gz;
		if (!FFileHelper::LoadFileToArray(Gz, *Path) || Gz.Num() < 18) { OutError = TEXT("cannot read ") + Path; return nullptr; }
		const uint32 Size = (uint32)Gz[Gz.Num() - 4] | (uint32)Gz[Gz.Num() - 3] << 8 | (uint32)Gz[Gz.Num() - 2] << 16 | (uint32)Gz[Gz.Num() - 1] << 24;
		TArray<uint8> Raw;
		Raw.SetNumUninitialized((int64)Size + 1);
		if (!FCompression::UncompressMemory(NAME_Gzip, Raw.GetData(), Size, Gz.GetData(), Gz.Num())) { OutError = TEXT("cannot gunzip ") + Path; return nullptr; }
		Raw[Size] = 0;
		TSharedPtr<FJsonObject> Root;
		if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(UTF8_TO_TCHAR((const ANSICHAR*)Raw.GetData())), Root) || !Root.IsValid()) { OutError = TEXT("cannot parse ") + Path; return nullptr; }
		return Root;
	}
	// a map as drawn at a start: the live grid and the map file's rows are its rows
	FInput LowInput(const FJsonObject& Case, bool bLow)
	{
		FInput In;
		const FJsonObject& O = *Case.GetObjectField(TEXT("input"));
		In.W = (int)Case.GetNumberField(TEXT("w")); In.H = (int)Case.GetNumberField(TEXT("h"));
		In.bLow = bLow; In.bWorld = false;
		In.Map = Rows(O, TEXT("rows")); In.Live = In.Map; In.Orig = In.Map; In.Heights = Rows(O, TEXT("heights"));
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (O.TryGetArrayField(TEXT("buildings"), A))
			for (const TSharedPtr<FJsonValue>& V : *A)
			{
				const TSharedPtr<FJsonObject>* B;
				if (V.IsValid() && V->TryGetObject(B)) In.Bld.push_back({ (int)(*B)->GetNumberField(TEXT("x")), { (int)(*B)->GetNumberField(TEXT("y")), TCHAR_TO_UTF8(*(*B)->GetStringField(TEXT("kind"))) } });
			}
		if (O.TryGetArrayField(TEXT("nodes"), A)) for (const TSharedPtr<FJsonValue>& V : *A) In.Nodes.push_back({ V->AsArray()[0]->AsNumber(), V->AsArray()[1]->AsNumber() });
		auto Xy = [&O](const TCHAR* Field, std::vector<TCScatterCore::FCellXY>& Out)
		{
			const TArray<TSharedPtr<FJsonValue>>* L;
			if (O.TryGetArrayField(Field, L))
				for (const TSharedPtr<FJsonValue>& V : *L)
				{
					const TSharedPtr<FJsonObject>* P;
					if (!V.IsValid() || !V->TryGetObject(P)) continue;
					Out.push_back(TCScatterCore::FCellXY{ (int)(*P)->GetNumberField(TEXT("x")), (int)(*P)->GetNumberField(TEXT("y")) });
				}
		};
		Xy(TEXT("spawns"), In.Spawns); Xy(TEXT("points"), In.Points);
		return In;
	}
	FString AtName(const TSharedPtr<FJsonValue>& V) { return V.IsValid() && V->Type == EJson::String ? V->AsString() : FString(); }
	// a reference mesh name: a piece kind (structures.js KINDS) or a prop kind, with '-bark' for a tree's bark mesh
	int32 PropOfMesh(const FString& Name)
	{
		FString K = Name;
		K.RemoveFromEnd(TEXT("-bark"));
		for (int32 I = 0; I < TCScatterCore::PropCount; I++) if (K == ANSI_TO_TCHAR(TCScatterCore::PropName((TCScatterCore::EProp)I))) return I;
		return -1;
	}
	// piece instances per kind name for a map as drawn, at a level
	TMap<FString, int32> PieceCounts(const FInput& In)
	{
		const FSrc Src(In);
		std::map<int, int> Named;
		for (const auto& B : In.Bld)
		{
			const int T = TCWorldCore::HouseTypeOf(B.second.second.c_str());
			if (T >= 0 && B.first >= 0 && B.second.first >= 0 && B.first < In.W && B.second.first < In.H) Named[B.second.first * In.W + B.first] = T;
		}
		std::vector<TCWorldCore::FPiece> Pieces;
		TCWorldCore::BuildAllPieces(Src, [&Named](int Cell) { const auto It = Named.find(Cell); return It == Named.end() ? -1 : It->second; }, Pieces);
		TMap<FString, int32> Out;
		for (const TCWorldCore::FPiece& P : Pieces) Out.FindOrAdd(ANSI_TO_TCHAR(TCWorldCore::PieceName(P.Kind)))++;
		return Out;
	}
}

// Which props and grass survive on Graphics Low, against the unmodified reference under Node on six authored maps
// (props.js visible: odd seeds left out except crop fields, a wood's tree too; place: no grass on Low), the same
// after a switch in the middle of a match (gfx.onChange -> refresh) and back; the prop and piece shadow casters per
// level; the pieces structures.js builds on Low (fewer sandbags, no rubble chunks or duckboards); ground.js's texels.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCGraphicsLowOracleTest, "ThreeCrossroads.World.GraphicsLowOracle", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCGraphicsLowOracleTest::RunTest(const FString& Parameters)
{
	using namespace TCScatterTests;
	using namespace TCScatterCore;
	FString Error;
	const TSharedPtr<FJsonObject> Root = LoadLowFixture(Error);
	if (!Root.IsValid()) { AddError(Error); return false; }
	TestEqual(TEXT("fixture reference"), Root->GetStringField(TEXT("reference")), FString(FTCScatter::ReferencePin));
	int32 Maps = 0, Rows_ = 0, Bad = 0, ShownHigh = 0, ShownLow = 0, GrassHigh = 0, OddKept = 0, MeshRows = 0;
	for (const TSharedPtr<FJsonValue>& V : Root->GetArrayField(TEXT("cases")))
	{
		const FJsonObject& Case = *V->AsObject();
		const FString Name = Case.GetStringField(TEXT("map"));
		const FInput High = LowInput(Case, false), Low = LowInput(Case, true);
		FScatterMap M;
		M.W = High.W; M.H = High.H; M.Rows = High.Map; M.Spawns = High.Spawns; M.Points = High.Points;
		const std::vector<FCandidate> List = Candidates(M);
		const TArray<TSharedPtr<FJsonValue>>& Want = Case.GetArrayField(TEXT("candidates"));
		if (!TestEqual(FString::Printf(TEXT("%s: candidates"), *Name), (int32)List.size(), Want.Num())) { Bad++; continue; }
		const FJsonObject& At = *Case.GetObjectField(TEXT("at"));
		const TArray<TSharedPtr<FJsonValue>>& AtHigh = At.GetArrayField(TEXT("high"));
		const TArray<TSharedPtr<FJsonValue>>& AtSwitched = At.GetArrayField(TEXT("switched"));
		const TArray<TSharedPtr<FJsonValue>>& AtFresh = At.GetArrayField(TEXT("fresh"));
		const TArray<TSharedPtr<FJsonValue>>& AtBack = At.GetArrayField(TEXT("back"));
		const FLive LiveHigh(High), LiveLow(Low);
		int32 MapLow = 0, MapHigh = 0;
		for (int32 I = 0; I < Want.Num(); I++)
		{
			const TArray<TSharedPtr<FJsonValue>>& R = Want[I]->AsArray();
			const FCandidate& C = List[(size_t)I];
			const bool bSame = R.Num() == 5 && R[0]->AsString() == ANSI_TO_TCHAR(PropName(C.Kind)) && (int)R[1]->AsNumber() == C.Cx && (int)R[2]->AsNumber() == C.Cy
				&& (uint32)R[3]->AsNumber() == C.Seed && ((int)R[4]->AsNumber() != 0) == C.bWood;
			if (!bSame && Bad++ < 20) AddError(FString::Printf(TEXT("%s: candidate %d differs from props.js"), *Name, I));
			const FPlaced PH = Place(C, LiveHigh, High.Nodes), PL = Place(C, LiveLow, Low.Nodes);
			const FString GotH = PH.bShown ? FString(ANSI_TO_TCHAR(PropName(PH.Kind))) : FString(), GotL = PL.bShown ? FString(ANSI_TO_TCHAR(PropName(PL.Kind))) : FString();
			Rows_ += 2;
			if (GotH != AtName(AtHigh[I]) && Bad++ < 20) AddError(FString::Printf(TEXT("%s: candidate %d on High: '%s', props.js '%s'"), *Name, I, *GotH, *AtName(AtHigh[I])));
			if (GotL != AtName(AtFresh[I]) && Bad++ < 20) AddError(FString::Printf(TEXT("%s: candidate %d on Low: '%s', props.js '%s'"), *Name, I, *GotL, *AtName(AtFresh[I])));
			// the reference itself: a mid-match switch shows what a Low start shows, and switching back what High showed
			if (AtName(AtSwitched[I]) != AtName(AtFresh[I]) && Bad++ < 20) AddError(FString::Printf(TEXT("%s: props.js switched and fresh Low differ at %d"), *Name, I));
			if (AtName(AtBack[I]) != AtName(AtHigh[I]) && Bad++ < 20) AddError(FString::Printf(TEXT("%s: props.js back on High differs at %d"), *Name, I));
			// the rules as stated: no grass on Low; an odd seed is never shown on Low unless it is a crop field
			if (PL.bShown && (C.Kind == EProp::Grass || ((C.Seed & 1) && C.Kind != EProp::Crop))) OddKept++;
			MapHigh += PH.bShown ? 1 : 0; MapLow += PL.bShown ? 1 : 0;
			GrassHigh += PH.bShown && PH.Kind == EProp::Grass ? 1 : 0;
		}
		ShownHigh += MapHigh; ShownLow += MapLow;
		AddInfo(FString::Printf(TEXT("%s: %d candidates, %d shown on High, %d on Low (props.js: the same)"), *Name, Want.Num(), MapHigh, MapLow));

		// shadow casters per mesh and level (props.js refresh castShadow, structures.js flush), and the instance counts
		const FJsonObject& PropMeshes = *Case.GetObjectField(TEXT("propMeshes"));
		const FJsonObject& PieceMeshes = *Case.GetObjectField(TEXT("pieceMeshes"));
		const TPair<const TCHAR*, bool> PropLevels[] = { { TEXT("high"), false }, { TEXT("switched"), true }, { TEXT("back"), false }, { TEXT("fresh"), true } };
		for (const auto& L : PropLevels)
		{
			TMap<int32, int32> Count;
			for (int32 I = 0; I < Want.Num(); I++)
			{
				const FPlaced P = Place(List[(size_t)I], L.Value ? (const FLiveSource&)LiveLow : (const FLiveSource&)LiveHigh, High.Nodes);
				if (P.bShown) Count.FindOrAdd((int32)P.Kind)++;
			}
			for (const TSharedPtr<FJsonValue>& E : PropMeshes.GetArrayField(L.Key))
			{
				const TArray<TSharedPtr<FJsonValue>>& R = E->AsArray();
				const int32 K = PropOfMesh(R[0]->AsString());
				MeshRows++;
				if (K < 0) { if (Bad++ < 20) AddError(FString::Printf(TEXT("%s: unknown prop mesh %s"), *Name, *R[0]->AsString())); continue; }
				const bool bCast = (int)R[2]->AsNumber() != 0;
				if (bCast != FTCWorldObjectsView::PropCastsShadow((EProp)K, L.Value) && Bad++ < 20) AddError(FString::Printf(TEXT("%s %s: %s casts %d in props.js"), *Name, L.Key, *R[0]->AsString(), bCast));
				if ((int32)R[1]->AsNumber() != Count.FindRef(K) && Bad++ < 20) AddError(FString::Printf(TEXT("%s %s: %s %d instances, native %d"), *Name, L.Key, *R[0]->AsString(), (int32)R[1]->AsNumber(), Count.FindRef(K)));
			}
		}
		const TPair<const TCHAR*, bool> PieceLevels[] = { { TEXT("high"), false }, { TEXT("switched"), true }, { TEXT("low"), true } };
		for (const auto& L : PieceLevels)
		{
			const TMap<FString, int32> Got = PieceCounts(L.Value ? Low : High);
			int32 Seen = 0;
			for (const TSharedPtr<FJsonValue>& E : PieceMeshes.GetArrayField(L.Key))
			{
				const TArray<TSharedPtr<FJsonValue>>& R = E->AsArray();
				const FString Kind = R[0]->AsString();
				const int32 PK = TCWorldCore::PieceOfName(TCHAR_TO_UTF8(*Kind));
				MeshRows++;
				if (PK < 0) { if (Bad++ < 20) AddError(FString::Printf(TEXT("%s: unknown piece mesh %s"), *Name, *Kind)); continue; }
				const bool bCast = (int)R[2]->AsNumber() != 0;
				if (bCast != FTCWorldObjectsView::PieceCastsShadow((TCWorldCore::EPiece)PK, L.Value) && Bad++ < 20) AddError(FString::Printf(TEXT("%s %s: piece %s casts %d in structures.js"), *Name, L.Key, *Kind, bCast));
				if ((int32)R[1]->AsNumber() != Got.FindRef(Kind) && Bad++ < 20) AddError(FString::Printf(TEXT("%s %s: %d %s pieces, native %d"), *Name, L.Key, (int32)R[1]->AsNumber(), *Kind, Got.FindRef(Kind)));
				Seen += Got.FindRef(Kind);
			}
			int32 All = 0;
			for (const TPair<FString, int32>& P : Got) All += P.Value;
			if (All != Seen && Bad++ < 20) AddError(FString::Printf(TEXT("%s %s: native pieces of a kind structures.js has no mesh for"), *Name, L.Key));
		}
		// ground.js createGround: the texels per cell on each level (the pinned expression, evaluated by the oracle)
		const FJsonObject& Tx = *Case.GetObjectField(TEXT("texels"));
		TestEqual(FString::Printf(TEXT("%s: ground texels per cell, High"), *Name), TCGroundPaintCore::FGround::PixelsPerCell(High.W, High.H, false, false), (int32)Tx.GetNumberField(TEXT("high")));
		TestEqual(FString::Printf(TEXT("%s: ground texels per cell, Low"), *Name), TCGroundPaintCore::FGround::PixelsPerCell(High.W, High.H, false, true), (int32)Tx.GetNumberField(TEXT("low")));
		Maps++;
	}
	AddInfo(FString::Printf(TEXT("graphics low: %d maps, %d placements and %d mesh rows compared with the reference; shown %d on High (%d grass), %d on Low; %d differ"), Maps, Rows_, MeshRows, ShownHigh, GrassHigh, ShownLow, Bad));
	TestTrue(TEXT("at least three authored maps"), Maps >= 3);
	TestEqual(TEXT("grass or odd-seed props shown on Low"), OddKept, 0);
	TestTrue(TEXT("Low leaves out more than half"), ShownLow * 2 < ShownHigh);
	TestEqual(TEXT("rows that differ from the reference"), Bad, 0);
	return true;
}

// A switch of level in the middle of a match through the game's own classes and FTCGraphics's change event: the
// pieces and props relaid and placed again (equal to the reference's switched counts), their shadows turned, the
// relief's low variant equal to a Low build (and ATCTerrain following it with the lead glue), the water's drift frozen,
// then all of it back on High. The level is switched without saving and restored at the end.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCGraphicsSwitchTest, "ThreeCrossroads.World.GraphicsSwitch", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCGraphicsSwitchTest::RunTest(const FString& Parameters)
{
	using namespace TCScatterTests;
	FString Error;
	const TSharedPtr<FJsonObject> Root = LoadLowFixture(Error);
	if (!Root.IsValid()) { AddError(Error); return false; }
	const FJsonObject* CaseP = nullptr;
	for (const TSharedPtr<FJsonValue>& V : Root->GetArrayField(TEXT("cases"))) if (V->AsObject()->GetStringField(TEXT("map")) == TEXT("seelow-heights")) CaseP = V->AsObject().Get();
	if (!TestNotNull(TEXT("seelow-heights in the fixture"), CaseP)) return false;
	const FJsonObject& Case = *CaseP;
	FTCGraphics& G = FTCGraphics::Get();
	const FTCGraphicsLevelScope Restore(ETCGraphicsLevel::High); // starts on High; the saved level comes back at the end
	auto CountOf = [](const TArray<TSharedPtr<FJsonValue>>& L) { int32 N = 0; for (const TSharedPtr<FJsonValue>& V : L) N += V.IsValid() && V->Type == EJson::String ? 1 : 0; return N; };
	auto PieceSum = [](const TArray<TSharedPtr<FJsonValue>>& L) { int32 N = 0; for (const TSharedPtr<FJsonValue>& V : L) N += (int32)V->AsArray()[1]->AsNumber(); return N; };
	const FJsonObject& At = *Case.GetObjectField(TEXT("at"));
	const FJsonObject& PieceMeshes = *Case.GetObjectField(TEXT("pieceMeshes"));

	const FJsonObject& O = *Case.GetObjectField(TEXT("input"));
	TSharedRef<FJsonObject> Map = MakeShared<FJsonObject>();
	Map->SetNumberField(TEXT("w"), Case.GetNumberField(TEXT("w")));
	Map->SetNumberField(TEXT("h"), Case.GetNumberField(TEXT("h")));
	Map->SetStringField(TEXT("name"), TEXT("Seelow Heights"));
	for (const TCHAR* F : { TEXT("rows"), TEXT("heights"), TEXT("spawns"), TEXT("points"), TEXT("buildings") }) Map->SetArrayField(F, O.GetArrayField(F));
	TSharedRef<FJsonObject> M = MakeShared<FJsonObject>();
	M->SetStringField(TEXT("t"), TEXT("start"));
	M->SetObjectField(TEXT("map"), Map);
	M->SetNumberField(TEXT("you"), 0);
	M->SetNumberField(TEXT("matchId"), 15);
	M->SetArrayField(TEXT("cells"), {});

	FScratchWorld Scratch(TEXT("TCGraphicsSwitchTest"));
	ATCTerrain* Terrain = Scratch.World->SpawnActor<ATCTerrain>(ATCTerrain::StaticClass(), FTransform::Identity);
	if (!TestNotNull(TEXT("terrain"), Terrain)) return false;
	FTCMatchState State;
	TestTrue(TEXT("match state takes the start"), State.ApplyStart(*M));
	Terrain->ResetFrom(State);
	for (int32 I = 0; I < 64 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
	FTCWorldObjects Objects;
	TestTrue(TEXT("world objects take the start"), Objects.ApplyStart(*M, State));
	Objects.Update(*Terrain, nullptr);
	{
		TArray<TSharedPtr<FJsonValue>> Nodes;
		for (const TSharedPtr<FJsonValue>& N : O.GetArrayField(TEXT("nodes")))
			Nodes.Add(MakeShared<FJsonValueArray>(TArray<TSharedPtr<FJsonValue>>{ N->AsArray()[0], N->AsArray()[1], MakeShared<FJsonValueNumber>(2), MakeShared<FJsonValueNumber>(0) }));
		TSharedRef<FJsonObject> Snap = MakeShared<FJsonObject>();
		Snap->SetArrayField(TEXT("nodes"), Nodes);
		Objects.ApplySnapshot(*Snap);
		Objects.Update(*Terrain, nullptr);
	}
	FTCWorldObjectsView View;
	TestTrue(TEXT("view attaches"), View.Attach(Scratch.World));
	auto DrawAll = [&]() { for (int32 I = 0; I < 200 && (I == 0 || View.Queued()); I++) View.Draw(Objects, State, Terrain); };
	DrawAll();
	auto TreeShadows = [&]() -> int32
	{
		int32 N = 0;
		for (TActorIterator<AActor> It(Scratch.World); It; ++It)
			for (UActorComponent* C : It->GetComponents())
				if (UHierarchicalInstancedStaticMeshComponent* H = Cast<UHierarchicalInstancedStaticMeshComponent>(C))
					if (H->GetInstanceCount() && H->CastShadow && H->GetStaticMesh() && H->GetStaticMesh()->GetName().Contains(TEXT("foliage"))) N++;
		return N;
	};
	auto Drawn = [&]() -> int32
	{
		int32 N = 0;
		for (TActorIterator<AActor> It(Scratch.World); It; ++It)
			for (UActorComponent* C : It->GetComponents())
				if (UHierarchicalInstancedStaticMeshComponent* H = Cast<UHierarchicalInstancedStaticMeshComponent>(C)) N += H->GetInstanceCount();
		return N;
	};
	const int32 HighProps = CountOf(At.GetArrayField(TEXT("high"))), LowProps = CountOf(At.GetArrayField(TEXT("switched")));
	TestEqual(TEXT("High: props shown as props.js"), Objects.PropsShown(), HighProps);
	TestEqual(TEXT("High: pieces as structures.js"), Objects.ShapeCount(), PieceSum(PieceMeshes.GetArrayField(TEXT("high"))));
	const int32 TreeShadowsHigh = TreeShadows();
	TestTrue(TEXT("High: trees and bushes cast shadows"), TreeShadowsHigh > 0);
	TestEqual(TEXT("High: every piece and prop drawn"), Drawn(), Objects.ShapeCount() + Objects.PropsShown());

	// the relief: its low variant from a High build equals a Low build, part by part
	auto SameRelief = [](const TCRelief::FRelief& A, const TCRelief::FRelief& B)
	{
		if (A.NumParts() != B.NumParts()) return false;
		for (int32 P = 0; P < A.NumParts(); P++)
			if (A.Part(P).Pos != B.Part(P).Pos || A.Part(P).Idx != B.Part(P).Idx || A.Part(P).Nrm != B.Part(P).Nrm || A.Part(P).Paint != B.Part(P).Paint) return false;
		return true;
	};
	{
		TCRelief::FRelief Switched = Terrain->Relief();
		auto Fresh = [&Switched](bool bLow)
		{
			TCRelief::FRelief R;
			R.Reset(Switched.W, Switched.H, bLow, Switched.ChunkCells, Switched.bDrawnPicks);
			R.InLevel = Switched.InLevel; R.InFrozen = Switched.InFrozen; R.InGrid = Switched.InGrid; R.InRoad = Switched.InRoad; R.InKnown = Switched.InKnown;
			R.Build();
			return R;
		};
		const TCRelief::FRelief Low = Fresh(true), High = Fresh(false);
		TestTrue(TEXT("relief: the start is the High variant"), !Switched.bLow && SameRelief(Switched, High));
		TestTrue(TEXT("relief: SetLow(true) rebuilds"), Switched.SetLow(true));
		TestEqual(TEXT("relief: every part re-emitted"), (int32)Switched.TakeRebuilt().size(), Switched.NumParts());
		TestTrue(TEXT("relief: switched to Low equals a Low build"), SameRelief(Switched, Low));
		TestTrue(TEXT("relief: the Low variant differs (fewer fans, lighter rocks)"), !SameRelief(Low, High) && Low.Stats.Triangles < High.Stats.Triangles);
		TestTrue(TEXT("relief: back to High"), Switched.SetLow(false) && SameRelief(Switched, High));
		TestFalse(TEXT("relief: no change, no rebuild"), Switched.SetLow(false));
		AddInfo(FString::Printf(TEXT("relief Seelow Heights: High %d triangles, Low %d"), High.Stats.Triangles, Low.Stats.Triangles));
	}

	// the water: water.js setLow on the change event, the clock stands still on Low
	FTCWater Water;
	Water.Attach(Scratch.World);
	Water.GroundHeight = [Terrain](float X, float Z) { return Terrain->HeightAt(X, Z); };
	Water.ResetFrom(State);
	Water.Flush(State, 3.0, false);
	const bool bWaterMid = Water.MaterialInstance() != nullptr;
	if (!bWaterMid) AddWarning(TEXT("water: no material instance (M_TCWater missing?); the water checks are skipped"));
	float LowParam = -1.f;
	if (bWaterMid) { Water.MaterialInstance()->GetScalarParameterValue(FName(TEXT("Low")), LowParam); TestEqual(TEXT("water: High draws the drift"), LowParam, 0.f); }

	// ---- the switch: one change event, every builder applies it at its next update
	G.Set(ETCGraphicsLevel::Low, false);
	Terrain->Flush(State);
	for (int32 I = 0; I < 64 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
	Objects.Update(*Terrain, nullptr);
	DrawAll();
	Water.Flush(State, 5.0, false);
	const float TimeLow = Water.TimeSent();
	Water.Flush(State, 9.0, false);
	TestEqual(TEXT("Low: props shown as props.js after the switch"), Objects.PropsShown(), LowProps);
	TestEqual(TEXT("Low: pieces as structures.js after the switch"), Objects.ShapeCount(), PieceSum(PieceMeshes.GetArrayField(TEXT("switched"))));
	TestEqual(TEXT("Low: one level switch applied"), Objects.LevelSwitches(), 1);
	TestTrue(TEXT("Low: laid out low"), Objects.LaidOutLow());
	TestTrue(TEXT("Low: the view's shadows follow"), View.ShadowsLow() && View.ShadowFlagsTurned() > 0);
	TestEqual(TEXT("Low: no tree or bush shadows"), TreeShadows(), 0);
	TestEqual(TEXT("Low: every piece and prop drawn, nothing more"), Drawn(), Objects.ShapeCount() + Objects.PropsShown());
	if (bWaterMid)
	{
		Water.MaterialInstance()->GetScalarParameterValue(FName(TEXT("Low")), LowParam);
		TestEqual(TEXT("water: Low freezes the drift"), LowParam, 1.f);
		TestEqual(TEXT("water: the clock stands still on Low"), Water.TimeSent(), TimeLow);
		TestEqual(TEXT("water: one switch"), Water.LevelSwitches(), 1);
	}
#ifdef TC_GFXLOW_GLUE
	TestTrue(TEXT("terrain: the relief follows to its low variant (lead glue)"), Terrain->ReliefLow() && Terrain->ReliefLevelSwitches() == 1);
#else
	AddInfo(TEXT("terrain: built without the round 15 lead glue; ATCTerrain keeps the relief it started with"));
#endif

	// ---- and back
	G.Set(ETCGraphicsLevel::High, false);
	Terrain->Flush(State);
	for (int32 I = 0; I < 64 && Terrain->ChunksQueued(); I++) Terrain->Flush(State);
	Objects.Update(*Terrain, nullptr);
	DrawAll();
	Water.Flush(State, 11.0, false);
	TestEqual(TEXT("back on High: props as props.js"), Objects.PropsShown(), CountOf(At.GetArrayField(TEXT("back"))));
	TestEqual(TEXT("back on High: pieces"), Objects.ShapeCount(), PieceSum(PieceMeshes.GetArrayField(TEXT("high"))));
	TestEqual(TEXT("back on High: tree and bush shadows again"), TreeShadows(), TreeShadowsHigh);
	TestEqual(TEXT("back on High: every piece and prop drawn"), Drawn(), Objects.ShapeCount() + Objects.PropsShown());
	if (bWaterMid) { Water.MaterialInstance()->GetScalarParameterValue(FName(TEXT("Low")), LowParam); TestEqual(TEXT("water: the drift runs again"), LowParam, 0.f); TestEqual(TEXT("water: the clock runs"), Water.TimeSent(), 11.f); }
#ifdef TC_GFXLOW_GLUE
	TestTrue(TEXT("terrain: back to the High relief"), !Terrain->ReliefLow() && Terrain->ReliefLevelSwitches() == 2);
#endif
	AddInfo(FString::Printf(TEXT("switch on Seelow Heights: props %d -> %d -> %d, pieces %d -> %d, tree shadow components %d -> 0 -> %d"), HighProps, LowProps, Objects.PropsShown(),
		PieceSum(PieceMeshes.GetArrayField(TEXT("high"))), PieceSum(PieceMeshes.GetArrayField(TEXT("switched"))), TreeShadowsHigh, TreeShadows()));
	Water.Detach();
	View.Detach();
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
