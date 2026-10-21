// Recipient memory and presentation acceptance fixtures. The integration lead runs these with a real RHI.
#if WITH_DEV_AUTOMATION_TESTS
#include "World/TCWorld30Memory.inl"
#include "World/TCWorld30VehicleOracle.inl"
#include "World/TCWorldObjects.h"
#include "World/TCTerrain.h"
#include "World/TCRailways.h"
#include "Client/TCGraphics.h"
#include "Client/TCLighting.h"
#include "Engine/TextureCube.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCUnitTable.h"
#include "Client/TCEffects.h"
#include "Net/TCMatchState.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "Engine/TextureRenderTarget2D.h"
#include "GameFramework/WorldSettings.h"
#include "Components/DirectionalLightComponent.h"
#include "Components/SkyLightComponent.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Misc/AutomationTest.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "Misc/FileHelper.h"
#include "HAL/FileManager.h"
#include "ImageUtils.h"
#include "RHI.h"
#include "RenderingThread.h"
#include "TextureResource.h"
#if WITH_EDITOR
#include "AssetCompilingManager.h"
#include "ShaderCompiler.h"
#endif

namespace TCWorld30
{
	struct FWorld
	{
		UWorld* World = UWorld::CreateWorld(EWorldType::Game, false);
		FWorld()
		{
			if (!World) return;
			GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World);
			World->InitializeActorsForPlay(FURL());
			FString Shots; if(FParse::Value(FCommandLine::Get(),TEXT("TCWorld30Shots="),Shots))
			{ auto* Light=World->SpawnActor<ATCLighting>(); Light->StartMatch(TEXT("default"),TEXT("default"),TEXT("clear"),0); Light->Tick(.016f); }
		}
		~FWorld() { if (World) { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); } }
	};
	TSharedPtr<FJsonObject> Json(const TCHAR* Text)
	{
		TSharedPtr<FJsonObject> J;
		FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), J);
		return J;
	}
	FTCMatchState State(int32 W = 24)
	{
		FTCMatchState S; S.bHasMatch = true; S.Map.bWorld = true; S.Map.W = S.Map.H = W;
		S.Map.Chars.Init('.', W*W); S.Map.Levels.Init(0, W*W); S.Map.Known.Init(false, W*W);
		S.FogVisible.Init(false, W*W); S.FogExplored.Init(false, W*W);
		S.You = 0; S.Teams = {0,1,2,3}; S.Factions = {0,1,2,3};
		return S;
	}
	// Optional review artifacts. A successful write is not a visual acceptance verdict.
	bool Shot(UWorld* W, const TCHAR* Name, FVector At, FVector Target, float Width = 5200)
	{
		FString Dir;
		if (!FParse::Value(FCommandLine::Get(), TEXT("TCWorld30Shots="), Dir)) return true;
		if (GUsingNullRHI) return false;
		IFileManager::Get().MakeDirectory(*Dir, true);
		AActor* Holder = W->SpawnActor<AActor>();
		UTextureRenderTarget2D* RT = NewObject<UTextureRenderTarget2D>(Holder);
		RT->RenderTargetFormat = RTF_RGBA8_SRGB; RT->InitAutoFormat(1280,960); RT->UpdateResourceImmediate(true);
		USceneCaptureComponent2D* C = NewObject<USceneCaptureComponent2D>(Holder);
		Holder->AddInstanceComponent(C); C->TextureTarget = RT; C->CaptureSource = SCS_FinalColorLDR;
		C->bCaptureEveryFrame = C->bCaptureOnMovement = false;
		C->ProjectionType = ECameraProjectionMode::Perspective; C->FOVAngle = 65;
		C->PostProcessSettings.bOverride_AutoExposureMethod = true;
		C->PostProcessSettings.AutoExposureMethod = AEM_Manual;
		C->PostProcessSettings.bOverride_AutoExposureApplyPhysicalCameraExposure = true;
		C->PostProcessSettings.AutoExposureApplyPhysicalCameraExposure = false;
		C->PostProcessSettings.bOverride_AutoExposureBias=true; C->PostProcessSettings.AutoExposureBias=1;
		C->PostProcessSettings.bOverride_BloomIntensity=true; C->PostProcessSettings.BloomIntensity=0;
		C->SetWorldLocationAndRotation(At, (Target-At).Rotation()); C->RegisterComponent();
#if WITH_EDITOR
		FAssetCompilingManager::Get().FinishAllCompilation();
		if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();
#endif
		for (int32 I=0; I<4; I++) { W->SendAllEndOfFrameUpdates(); C->CaptureScene(); FlushRenderingCommands(); }
		TArray<FColor> Pixels; bool Ok = RT->GameThread_GetRenderTargetResource()->ReadPixels(Pixels) && Pixels.Num()==1280*960;
		if (Ok)
		{
			for (FColor& P : Pixels) P.A = 255;
			TArray64<uint8> Bytes;
			FImageUtils::PNGCompressImageArray(1280,960,TArrayView64<const FColor>(Pixels.GetData(),Pixels.Num()),Bytes);
			Ok = FFileHelper::SaveArrayToFile(Bytes, *FPaths::Combine(Dir,FString(Name)+TEXT(".png")));
		}
		Holder->Destroy(); return Ok;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld30MinesTest, "ThreeCrossroads.World30.MineRecipientMemory",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld30MinesTest::RunTest(const FString&)
{
	using namespace TCWorld30;
	FWorld W; if (!TestNotNull(TEXT("world"),W.World)) return false;
	FTCMatchState S = State(8);
	// terrainFor: own mine 18 and found enemy mine 20 are N. Undiscovered enemy 22 is public ground.
	// Ownership is deliberately not reconstructed by the client. The delivered character is authoritative.
	for (int32 C : {18,20,22,35}) S.Map.Known[C] = S.FogExplored[C] = true;
	S.Map.Chars[18] = S.Map.Chars[20] = 'N'; S.Map.Chars[35] = 'B';
	auto Start = Json(TEXT(R"({"map":{"w":8,"h":8,"world":true,"rows":["........","........","..N.N...","........","...B....","........","........","........"]},"cells":[[18,"N",0,0,{"ground":".","object":"."}],[20,"N",0,0,{"ground":".","object":"."}],[22,".",0,0],[35,"B",0,0]]})"));
	ATCTerrain* T = W.World->SpawnActor<ATCTerrain>(); FTCWorldObjects O; T->SetCellSource(&O);
	if (!TestTrue(TEXT("recipient start"), O.ApplyStart(*Start,S))) return false;
	T->ResetFrom(S); while(T->ChunksQueued()) T->Flush(S); O.Update(*T,nullptr);
	auto Check = [&]
	{
		TSet<int32> Mines;
		O.ForEachPiece(0,[&](const TCWorldCore::FPiece& P)
		{
			if (P.Kind==TCWorldCore::EPiece::Dark && FMath::IsNearlyEqual(P.Sy,.14)) Mines.Add(P.Cell);
		});
		TestEqual(TEXT("exactly two delivered mines submitted"),Mines.Num(),2);
		TestTrue(TEXT("own mine submitted"),Mines.Contains(18));
		TestTrue(TEXT("found enemy mine submitted"),Mines.Contains(20));
		TestFalse(TEXT("undiscovered enemy mine absent even on explored ground"),Mines.Contains(22));
	};
	Check();
	auto Snap = Json(TEXT(R"({"cells":[[35,"R",1,16],[18,"N",0,0],[20,"N",0,0],[22,".",0,0]]})"));
	S.Map.Chars[35]='R'; S.Map.Levels[35]=1;
	T->MarkCells({35},{}); while(T->ChunksQueued()) T->Flush(S);
	O.ApplySnapshot(*Snap); O.Update(*T,nullptr); Check();
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld30DecalsTest, "ThreeCrossroads.World30.DecalLandmarkMemory",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld30DecalsTest::RunTest(const FString&)
{
	using namespace TCWorld30;
	TestEqual(TEXT("production decals discover, remember and exclude adjacent unknown cells"),TCWorld30Memory::Check(),0);
	FWorld W; if (!TestNotNull(TEXT("world"),W.World)) return false;
	FTCMatchState S = State(8);
	for (int32 C : {18,27,35}) { S.Map.Known[C]=S.FogExplored[C]=true; S.Map.Chars[C]= C==35 ? '+' : 'R'; }
	auto Start = Json(TEXT(R"({"map":{"w":8,"h":8,"world":true,"rows":["........","........","........","........","........","........","........","........"]},"cells":[[18,"R",0,0,{"initial":["R"],"ground":".","object":"R"}],[27,"R",0,0,{"ground":".","object":"R"}],[35,"+",0,0,{"ground":"+","object":"."}]]})"));
	FTCWorldObjects O; ATCTerrain* T=W.World->SpawnActor<ATCTerrain>(); T->SetCellSource(&O);
	if (!TestTrue(TEXT("landmark start"),O.ApplyStart(*Start,S))) return false;
	T->ResetFrom(S); while(T->ChunksQueued()) T->Flush(S); O.Update(*T,nullptr);
	auto Pieces = [&]
	{
		TArray<TCWorldCore::FPiece> Result;
		O.ForEachPiece(0,[&](const TCWorldCore::FPiece& P)
		{
			TestTrue(TEXT("no landmark originates in an unknown cell"),S.Map.Known[P.Cell]);
			if(P.Cell==18) Result.Add(P);
		}); return Result;
	};
	const auto Before=Pieces();
	bool SpoilEarth=false; for(const auto& P : Before) SpoilEarth|=P.Kind==TCWorldCore::EPiece::Earth;
	TestTrue(TEXT("discovered initial rubble builds the spoil earth heap"),SpoilEarth);
	O.ApplySnapshot(*Json(TEXT(R"({"cells":[[18,"R",0,0],[27,"R",0,0],[35,"+",0,0]]})"))); O.Update(*T,nullptr);
	const auto After=Pieces(); TestEqual(TEXT("spoil geometry survives omitted metadata"),After.Num(),Before.Num());
	for(int32 I=0; I<FMath::Min(Before.Num(),After.Num()); I++)
	{
		TestEqual(TEXT("remembered spoil kind"),(int32)After[I].Kind,(int32)Before[I].Kind);
		TestTrue(TEXT("remembered spoil pose"),After[I].X==Before[I].X && After[I].Y==Before[I].Y && After[I].Z==Before[I].Z);
	}
	FTCWorldObjectsView V; TestTrue(TEXT("structure view"),V.Attach(W.World)); V.Draw(O,S,T);
	FTCGroundPaint Paint; TestTrue(TEXT("decal view"),Paint.Attach(W.World)); Paint.ApplyStart(*Start,S);
	T->SetPaintSource(&Paint); Paint.Bind(T->GroundMaterialInstance());
	Paint.Tick(S,0,false,[&](float X,float Z){return T->HeightAt(X,Z);});
	Paint.ApplySnapshot(*Json(TEXT(R"({"cells":[[18,"R",0,0],[27,"R",0,0],[35,"+",0,0]]})")));
	Paint.Tick(S,0,false,[&](float X,float Z){return T->HeightAt(X,Z);});
	TestEqual(TEXT("two known rubble decals"),Paint.RubbleDecals(),2);
	TestEqual(TEXT("one known crater decal"),Paint.CraterDecals(),1);
	TestTrue(TEXT("optional decal review capture"),Shot(W.World,TEXT("decals-spoil-memory"),FVector(800,-1200,2400),FVector(800,800,0),2100));
	T->SetPaintSource(nullptr); Paint.Detach(); V.Detach(); return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld30RailTest, "ThreeCrossroads.World30.RailJunctionStation",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld30RailTest::RunTest(const FString&)
{
	using namespace TCWorld30;
	FWorld W; if(!TestNotNull(TEXT("world"),W.World)) return false;
	FTCMatchState S=State();
	for(int32 C : {299,300,301,324}) S.Map.Known[C]=S.FogVisible[C]=S.FogExplored[C]=true;
	ATCTerrain* T=W.World->SpawnActor<ATCTerrain>(); T->ResetFrom(S); while(T->ChunksQueued()) T->Flush(S);
	FTCRailways Rail; TestTrue(TEXT("rail attaches"),Rail.Attach(W.World)); Rail.Reset(S);
	Rail.ApplyCells(*Json(TEXT(R"({"cells":[[299,".",0,0,{"rail":1}],[300,".",0,0,{"rail":21,"station":7}],[301,".",0,0,{"rail":16}],[324,".",0,0,{"rail":64}]]})")));
	Rail.ApplySnapshot(*Json(TEXT(R"({"world":{"regions":[{"id":7,"x":24,"z":20}]}})")));
	Rail.Frame(1,S,T);
	TestEqual(TEXT("junction has six delivered half segments"),Rail.HalfCount(),6);
	TestEqual(TEXT("complete discovered station"),Rail.StationsBuilt(),1);
	TestFalse(TEXT("adjacent unknown railway absent"),Rail.Has(302));
	TestTrue(TEXT("optional station review capture"),Shot(W.World,TEXT("rail-junction-station"),FVector(2400,-1200,3400),FVector(2400,2400,0),5400));
	Rail.Detach(); return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld30CorpsesTest, "ThreeCrossroads.World30.CorpseCapBlastGore",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld30CorpsesTest::RunTest(const FString&)
{
	using namespace TCWorld30;
	FWorld W; if(!TestNotNull(TEXT("world"),W.World)) return false;
	const FTCGraphicsLevelScope High(ETCGraphicsLevel::High);
	const bool PreviousGore = FTCGraphics::Get().Gore();
	ATCEffects* Effects=W.World->SpawnActor<ATCEffects>();
	AActor* Holder=W.World->SpawnActor<AActor>();
	UTCSoldierCrowd* Crowd=NewObject<UTCSoldierCrowd>(Holder);
	Holder->AddInstanceComponent(Crowd); Crowd->RegisterComponent();
	FString Error;
	if(!TestTrue(TEXT("original corpse figures load"),Crowd->Load(Error))) return false;
	for(bool Gore : {false,true})
	{
		FTCGraphics::Get().SetGore(Gore);
		Crowd->ClearCorpses(); Crowd->Random.Initialize(0x5eed);
		FTCMatchState S=State();
		Effects->Bind(&S,nullptr,nullptr,nullptr,nullptr);
		Effects->Fx.Start(S);
		for(int32 I=0; I<201; I++)
		{
			Crowd->AddCorpse(TEXT("rifle"),I%4,I%6,ATCUnitRenderer::SeatColor(I%4),FVector(3020+(I%10)*10,3000+(I/10)*10,0),0,1);
			TestEqual(TEXT("each casualty respects the original hard cap"),Crowd->NumCorpses(),FMath::Min(I+1,200));
		}
		const int32 TossesBefore=Crowd->BlastTosses();
		FTCShot Sh; Sh.K=TEXT("shell"); Sh.bHasPos=true; Sh.X=Sh.Z=30;
		S.Tick=10; S.Shots.Add(Sh);
		FTCFxContext Context; Context.HeightAt=[](float,float){return 0.f;};
		Effects->Fx.Snapshot(S,nullptr,Context,.5);
		double HX,HZ,HS;
		TestTrue(TEXT("delivered shell creates a real nearby blast"),Effects->Fx.BlastNear(30.2,30,HX,HZ,HS));
		const int32 Before=Effects->Fx.Particles().Num();
		Crowd->BeginFrame(); Crowd->EndFrame(.016f,FVector(3000,3000,2500),[](double,double){return 0.;});
		TestTrue(TEXT("real effects blast throws bodies with either Gore setting"),Crowd->BlastTosses()>TossesBefore);
		if(Gore) TestTrue(TEXT("Gore on adds blood/scrap particles from thrown bodies"),Effects->Fx.Particles().Num()>Before);
		else TestEqual(TEXT("Gore off adds no blood/scrap particles"),Effects->Fx.Particles().Num(),Before);
		// Capture after the initial flash, keeping the blast/gore assertions above at their original clock.
		Effects->Fx.Frame(Context,1.1,.584f); Effects->Present(1.1);
		TestTrue(TEXT("optional corpse review capture"),Shot(W.World,Gore?TEXT("corpses-gore-on"):TEXT("corpses-gore-off"),FVector(3100,2400,1100),FVector(3100,3100,0),1900));
		for(int32 I=0; I<280; I++) { Crowd->BeginFrame(); Crowd->EndFrame(.1f,FVector(3000,3000,2500),[](double,double){return 0.;}); }
		TestEqual(TEXT("original life and fade expire every body"),Crowd->NumCorpses(),0);
		Effects->Bind(nullptr,nullptr,nullptr,nullptr,nullptr);
	}
	FTCGraphics::Get().SetGore(PreviousGore);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld30ModelsTest, "ThreeCrossroads.World30.ModelFamilies",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld30ModelsTest::RunTest(const FString&)
{
	using namespace TCWorld30;
	FWorld W; if(!TestNotNull(TEXT("world"),W.World)) return false;
	FTCUnitTable Table; FString Error;
	if(!TestTrue(TEXT("original unit definitions"),Table.Load(Error))) return false;
	ATCUnitRenderer* View=W.World->SpawnActor<ATCUnitRenderer>();
	View->SetEnabledBatches((1u<<ATCUnitRenderer::BatchCount)-1);
	const TArray<TArray<FString>> Families = {
		{TEXT("rifle"),TEXT("tank"),TEXT("tankdestroyer"),TEXT("at"),TEXT("flak")},
		{TEXT("fighter"),TEXT("attacker"),TEXT("bomber")},
		{TEXT("lcvp"),TEXT("gunboat"),TEXT("destroyer")}
	};
	for(int32 Family=0; Family<Families.Num(); Family++)
	{
		FTCMatchState S=State(160); S.Map.bWorld=false;
		S.Map.Known.Init(true,160*160); S.FogVisible.Init(true,160*160); S.FogExplored.Init(true,160*160);
		View->Reset();
		const float Gap=Family==2?60.f:Family==1?35.f:13.f;
		for(int32 F=0; F<4; F++) for(int32 K=0; K<Families[Family].Num(); K++)
		{
			FTCUnit U; U.Id=F*20+K+1; U.Type=Families[Family][K]; U.Owner=F;
			U.X=20+K*Gap; U.Z=20+F*Gap; U.Built=1;
			const FTCUnitDef* Def=Table.Find(U.Type);
			if(!TestNotNull(TEXT("family unit definition"),Def)) continue;
			U.Hp=Def->MaxHp(); S.Units.Add(U.Id,U); S.UnitOrder.Add(U.Id);
		}
		const float Span=Gap*4+40;
		const FVector Centre(Span*50,Span*50,0), Eye=Centre+FVector(0,-Span*55,Span*65);
		View->SetViewPoint(Eye); View->Draw(S,Table,nullptr,{},0,&S.UnitOrder);
		TestEqual(TEXT("all requested family representatives are drawn"),View->ModelStats().ModelUnits,S.Units.Num());
		TestEqual(TEXT("every representative uses an original model"),View->ModelStats().PlaceholderUnits,0);
		TestEqual(TEXT("no missing model in the four faction rows"),View->ModelStats().MissingModel,0);
		TestTrue(TEXT("original meshes submitted"),View->ModelStats().PartInstances>0);
		TestTrue(TEXT("optional faction/model family review capture"),Shot(W.World,*FString::Printf(TEXT("models-family-%d"),Family),Eye,Centre,Span*100));
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld30VehiclesTest, "ThreeCrossroads.World30.VehicleSnapshotPresentation",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld30VehiclesTest::RunTest(const FString&)
{
	using namespace TCWorld30;
	FWorld W; if(!TestNotNull(TEXT("world"),W.World)) return false;
	FTCUnitTable Table; FString Error;
	if(!TestTrue(TEXT("original unit definitions"),Table.Load(Error))) return false;
	FTCMatchState S=State(40); S.Snapshots=1;
	ATCUnitRenderer* V=W.World->SpawnActor<ATCUnitRenderer>();
	V->SetEnabledBatches((1u<<ATCUnitRenderer::BatchCount)-1); V->SetViewPoint(FVector(3300,2000,4000));
	for(int32 Id : {1,2})
	{
		FTCUnit U; U.Id=Id; U.Type=Id==1?TEXT("tankdestroyer"):TEXT("gunboat"); U.Owner=Id==1?1:0;
		U.X=30; U.Z=Id==1?10:30; U.Aim=1.2f; U.Built=1;
		const FTCUnitDef* D=Table.Find(U.Type); if(!TestNotNull(TEXT("vehicle definition"),D)) return false;
		U.Hp=D->MaxHp(); S.Units.Add(Id,U); S.UnitOrder.Add(Id);
	}
	V->Draw(S,Table,nullptr,{},0,&S.UnitOrder);
	TestTrue(TEXT("optional initial turn frame"),Shot(W.World,TEXT("vehicles-turn-0"),FVector(3300,-1000,3300),FVector(3300,2300,0),5100));
	S.Snapshots=2; S.Tick+=2;
	for(auto& P : S.Units) { P.Value.X+=4; P.Value.Z+=2; P.Value.Rot=.4f; P.Value.Aim=1.4f; }
	V->ApplySnapshot(S,Table); V->Draw(S,Table,nullptr,{},.1f,&S.UnitOrder);
	for(int32 Id : {1,2})
	{
		FVector2D At; TestTrue(TEXT("delivered vehicle still drawn"),V->DrawnAt(Id,At));
		TestTrue(TEXT("snapshot gap uses source exponential x easing"),FMath::IsNearlyEqual(At.X,TCWorld30Oracle::x,1e-5));
		TestTrue(TEXT("snapshot gap uses source exponential z easing"),FMath::IsNearlyEqual(At.Y,TCWorld30Oracle::z-(Id==1?20:0),1e-5));
	}
	const FTCPartMotion* Boat=V->MotionOf(2);
	if(TestNotNull(TEXT("boat hull motion"),Boat))
	{
		TestTrue(TEXT("boat heave stays on the source waterline"),FMath::IsNearlyEqual(Boat->Body.GetLocation().Z,TCWorld30Oracle::heave*100,1e-4));
		const FQuat SourceRotation=FQuat(FVector::ForwardVector,-TCWorld30Oracle::roll)*FQuat(FVector::RightVector,-TCWorld30Oracle::pitch);
		TestTrue(TEXT("boat pitch and roll match source outputs"),Boat->Body.GetRotation().Equals(SourceRotation,1e-6));
		const FTCModelEntry* Ship=V->Models().Resolve({TEXT("gunboat"),0,FString()});
		if(TestNotNull(TEXT("original gunboat model"),Ship))
		{
			const int32 Hull=Ship->FindPart(TEXT("hull"));
			if(TestTrue(TEXT("gunboat hull part exists"),Hull!=INDEX_NONE))
			{
				FTCPartMotion SourceMotion=*Boat;
				SourceMotion.Body=FTransform(SourceRotation,FVector(0,0,TCWorld30Oracle::heave*100));
				const FTransform Root(FQuat(FVector::UpVector,TCWorld30Oracle::rot),FVector(TCWorld30Oracle::x*100,TCWorld30Oracle::z*100,0));
				const FTransform Expected=FTCUnitMotion::PartTransform(*Ship,Hull,Root,{},SourceMotion);
				bool Found=false; TArray<UInstancedStaticMeshComponent*> Components; V->GetComponents(Components);
				for(auto* C : Components) if(C->GetStaticMesh() && C->GetStaticMesh()->GetPathName()==Ship->Parts[Hull].Mesh.ToString())
					for(int32 I=0; I<C->GetInstanceCount(); I++) { FTransform T; C->GetInstanceTransform(I,T,true); Found|=T.Equals(Expected,.02); }
				TestTrue(TEXT("submitted boat hull follows the source waterline pose"),Found);
			}
		}
	}
	// Check a submitted turret instance, not only the helper's result.
	const FTCModelEntry* E=V->Models().Resolve({TEXT("tankdestroyer"),1,FString()});
	if(!TestNotNull(TEXT("German casemate model"),E)) return false;
	const int32 Turret=E->FindPart(TEXT("turret"));
	if(!TestTrue(TEXT("casemate has the source turret part"),Turret!=INDEX_NONE)) return false;
	const FTCPartMotion* Motion=V->MotionOf(1); if(!TestNotNull(TEXT("tank hull motion"),Motion)) return false;
	TMap<FName,FQuat> Turns; Turns.Add(TEXT("turret"),FQuat(FVector::UpVector,.21));
	const FTransform Root(FQuat(FVector::UpVector,TCWorld30Oracle::rot),FVector(TCWorld30Oracle::x*100,(TCWorld30Oracle::z-20)*100,0));
	const FTransform Expected=FTCUnitMotion::PartTransform(*E,Turret,Root,Turns,*Motion);
	bool Found=false;
	TArray<UInstancedStaticMeshComponent*> Components; V->GetComponents(Components);
	for(auto* C : Components) if(C->GetStaticMesh() && C->GetStaticMesh()->GetPathName()==E->Parts[Turret].Mesh.ToString())
		for(int32 I=0; I<C->GetInstanceCount(); I++) { FTransform T; C->GetInstanceTransform(I,T,true); Found|=T.Equals(Expected,.02); }
	TestTrue(TEXT("submitted casemate traverse is limited to source 0.21 radians"),Found);
	TestTrue(TEXT("optional delivered turn frame"),Shot(W.World,TEXT("vehicles-turn-1"),FVector(3300,-1000,3300),FVector(3300,2300,0),5100));
	return true;
}
#endif
