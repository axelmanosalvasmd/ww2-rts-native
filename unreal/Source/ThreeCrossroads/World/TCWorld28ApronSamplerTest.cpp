// Engine-only regression. Lead must record unchanged-production RED before the
// separately supplied fix. Resource binding is not a sampled GPU texel proof.
#if WITH_DEV_AUTOMATION_TESTS
#include "World/TCApron.h"
#include "World/TCGroundPaint.h"
#include "Client/TCGraphics.h"
#include "Net/TCMatchState.h"
#include "Dom/JsonObject.h"
#include "Engine/Engine.h"
#include "Engine/Texture2D.h"
#include "Engine/World.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/AutomationTest.h"
#include "Misc/ScopeExit.h"
#include "RenderingThread.h"
#include "RHI.h"
#include "TextureResource.h"

namespace TCWorld28ApronTest
{
	struct FWorld
	{
		UWorld* World = nullptr;
		FWorld()
		{
			World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("World28ApronSampler"));
			if (World) { GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(World); World->InitializeActorsForPlay(FURL()); }
		}
		~FWorld() { if (World) { GEngine->DestroyWorldContext(World); World->DestroyWorld(false); } }
	};
	// Access the engine's cached state, rather than assuming FRHISamplerState has
	// a GetInitializer API. No instance or replacement texture is constructed.
	struct FSamplerProbe : FTexture
	{
		static FRHISamplerState* OriginalTiles()
		{
			return GetOrCreateSamplerState(FSamplerStateInitializerRHI(SF_AnisotropicLinear, AM_Wrap, AM_Wrap, AM_Wrap, 0.f, 8));
		}
	};
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCWorld28ApronSamplerTest, "ThreeCrossroads.World.World28.ApronOriginalSampler", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCWorld28ApronSamplerTest::RunTest(const FString& Parameters)
{
	using namespace TCWorld28ApronTest;
	if (!TestFalse(TEXT("actual resource sampler requires a real RHI"), GUsingNullRHI)) return false;
	FWorld Scratch;
	if (!TestNotNull(TEXT("scratch world"), Scratch.World)) return false;
	// A tiny synthetic authored layout only loads the existing six source tiles.
	// It makes no map, unknown-ground, visibility or screenshot acceptance claim.
	FTCMatchState State;
	State.bHasMatch = true; State.Map.W = State.Map.H = 8;
	State.Map.Chars.Init('.', 64); State.Map.Levels.Init(0, 64); State.Map.Known.Init(true, 64);
	State.FogVisible.Init(true, 64); State.FogExplored.Init(true, 64);
	const TSharedRef<FJsonObject> Map = MakeShared<FJsonObject>();
	Map->SetNumberField(TEXT("w"), 8); Map->SetNumberField(TEXT("h"), 8);
	TArray<TSharedPtr<FJsonValue>> Rows;
	for (int32 I = 0; I < 8; I++) Rows.Add(MakeShared<FJsonValueString>(TEXT("........")));
	Map->SetArrayField(TEXT("rows"), Rows);
	FJsonObject Start; Start.SetObjectField(TEXT("map"), Map);
	const TCHAR* Names[] = { TEXT("Grass"), TEXT("Dirt"), TEXT("Mud"), TEXT("Field"), TEXT("Road"), TEXT("Water") };
	for (ETCGraphicsLevel Level : { ETCGraphicsLevel::Low, ETCGraphicsLevel::High })
	{
		FTCGraphicsLevelScope Tier(Level);
		FTCGroundPaint Paint;
		FTCApron Apron;
		ON_SCOPE_EXIT { Apron.Detach(); Paint.Detach(); FlushRenderingCommands(); };
		if (!TestTrue(TEXT("load unchanged committed ground tiles"), Paint.ApplyStart(Start, State))) return false;
		TArray<TArray<uint8>> Bytes; TArray<int32> Sizes;
		if (!TestTrue(TEXT("six original tinted tile payloads"), Paint.ApronTiles(Bytes, Sizes))) return false;
		Apron.Attach(Scratch.World); Apron.ResetFrom(State);
		// Replacement calls must request the policy too, not only the first load.
		for (int32 Replacement = 0; Replacement < 2; Replacement++)
		{
			Apron.SetTileBytes(Bytes, Sizes); FlushRenderingCommands();
			UMaterialInstanceDynamic* MID = Apron.MaterialInstance();
			if (!TestNotNull(TEXT("actual apron material"), MID)) return false;
			TArray<FTextureResource*> Resources;
			for (int32 M = 0; M < 6; M++)
			{
				UTexture* Bound = nullptr;
				if (!TestTrue(TEXT("actual MID texture parameter"), MID->GetTextureParameterValue(FMaterialParameterInfo(Names[M]), Bound))) return false;
				UTexture2D* T = Cast<UTexture2D>(Bound);
				if (!TestNotNull(Names[M], T) || !TestNotNull(TEXT("initialized tile resource"), T->GetResource())) return false;
				TestFalse(TEXT("raw source bytes stay linear"), T->SRGB);
				TestTrue(TEXT("immutable tile stays nonstreaming and wrapped"), T->NeverStream && T->AddressX == TA_Wrap && T->AddressY == TA_Wrap);
				const FTexturePlatformData* Data = T->GetPlatformData();
				if (!TestNotNull(TEXT("source mip data"), Data)) return false;
				TestEqual(TEXT("complete source mip chain"), Data->Mips.Num(), (int32)FMath::FloorLog2(Sizes[M]) + 1);
				TestEqual(TEXT("original tile extent"), T->GetSizeX(), Sizes[M]);
				Resources.Add(T->GetResource());
			}
			TArray<uint8> MainMatches, DeferredMatches;
			ENQUEUE_RENDER_COMMAND(TCWorld28ReadApronSamplers)([Resources, &MainMatches, &DeferredMatches](FRHICommandListImmediate&)
			{
				FRHISamplerState* Expected = FSamplerProbe::OriginalTiles();
				for (FTextureResource* R : Resources)
				{
					MainMatches.Add(Expected && R->SamplerStateRHI.GetReference() == Expected);
					DeferredMatches.Add(Expected && R->DeferredPassSamplerStateRHI.GetReference() == Expected);
				}
			});
			FlushRenderingCommands();
			for (int32 M = 0; M < 6; M++)
			{
				TestTrue(FString::Printf(TEXT("%s uses original linear-mip anisotropy-8 request"), Names[M]), MainMatches[M] != 0);
				TestTrue(FString::Printf(TEXT("%s deferred sampler preserves the same request"), Names[M]), DeferredMatches[M] != 0);
			}
		}
	}
	AddInfo(TEXT("Sampler request and original bytes only. Platform limits, derivative-selected mips, actual filtering and Low visual fidelity need raw rendering controls."));
	return true;
}
#endif
