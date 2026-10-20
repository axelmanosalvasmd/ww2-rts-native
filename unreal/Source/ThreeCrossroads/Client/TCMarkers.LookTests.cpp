#include "Client/TCMarkers.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Engine/Canvas.h"
#include "Engine/TextureRenderTarget2D.h"
#include "CanvasTypes.h"
#include "RenderingThread.h"
#include "RHI.h"
#include "UObject/StrongObjectPtr.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHealthBarLayersTest, "ThreeCrossroads.Markers.HealthBarLayers",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCHealthBarLayersTest::RunTest(const FString&)
{
	if (GUsingNullRHI)
	{
		AddWarning(TEXT("HealthBarLayers requires a rendering RHI; no rendered assertion ran"));
		return true;
	}
	TStrongObjectPtr<UTextureRenderTarget2D> Target(NewObject<UTextureRenderTarget2D>());
	Target->ClearColor = FLinearColor(.3f, .3f, .3f, 1);
	Target->InitCustomFormat(192, 128, PF_FloatRGBA, true);
	Target->UpdateResourceImmediate(true);
	FTextureRenderTargetResource* Resource = Target->GameThread_GetRenderTargetResource();
	FCanvas Draw(Resource, nullptr, static_cast<UWorld*>(nullptr), ERHIFeatureLevel::SM5);
	TStrongObjectPtr<UCanvas> Canvas(NewObject<UCanvas>());
	Canvas->Init(192, 128, nullptr, &Draw);
	TArray<TCMarkersHud::FUnitBillboard> Units;
	auto& Full = Units.AddDefaulted_GetRef();
	Full.Centre = FVector2D(96, 64); Full.PxPerMetre = 32; Full.Owner = 0; Full.bHealthBar = true;
	auto& Hurt = Units.AddDefaulted_GetRef();
	Hurt.Centre = FVector2D(96, 68); Hurt.PxPerMetre = 32; Hurt.Health = .25f; Hurt.bHealthBar = true;
	// Pinned main.js renderOrder 3/4: the second backdrop intersects the first fill. Rendering complete
	// units in sequence makes the three blue probes black. All fills must survive all later backdrops.
	TCMarkersHud::DrawUnitBillboards(Canvas.Get(), Units);
	Draw.Flush_GameThread(); FlushRenderingCommands();
	TArray<FLinearColor> Pixels;
	if (!TestTrue(TEXT("actual bar GPU readback"), Resource->ReadLinearColorPixels(Pixels, FReadSurfaceDataFlags(RCM_MinMax)))) return false;
	if (!TestEqual(TEXT("bar target size"), Pixels.Num(), 192 * 128)) return false;
	for (int32 X : {88, 96, 124})
	{
		const FLinearColor& P = Pixels[63 * 192 + X];
		TestTrue(*FString::Printf(TEXT("full-health blue fill survives the overlapping backdrop at x=%d: %s"), X, *P.ToString()),
			P.B > .1f && P.B > 2 * P.R && P.B > P.G);
	}
	const FLinearColor& Damage = Pixels[66 * 192 + 66];
	TestTrue(TEXT("quarter-health fill is red"), Damage.R > .1f && Damage.R > 2 * Damage.G && Damage.R > 2 * Damage.B);
	const FLinearColor& Backdrop = Pixels[73 * 192 + 96];
	TestTrue(TEXT("unfilled suppression space keeps the source dark backdrop"), FMath::Max3(Backdrop.R, Backdrop.G, Backdrop.B) < .09f);
	return true;
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSupplyFlagsTest, "ThreeCrossroads.Markers.SupplyFlags",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSupplyFlagsTest::RunTest(const FString&)
{
	if (GUsingNullRHI) { AddError(TEXT("SupplyFlags needs a rendering RHI")); return false; }
	TStrongObjectPtr<UTextureRenderTarget2D> Target(NewObject<UTextureRenderTarget2D>());
	Target->ClearColor = FLinearColor::Black;
	Target->InitCustomFormat(192, 128, PF_FloatRGBA, true); Target->UpdateResourceImmediate(true);
	FTextureRenderTargetResource* Resource = Target->GameThread_GetRenderTargetResource();
	FCanvas Draw(Resource, nullptr, static_cast<UWorld*>(nullptr), ERHIFeatureLevel::SM5);
	TStrongObjectPtr<UCanvas> Canvas(NewObject<UCanvas>());
	Canvas->Init(192, 128, nullptr, &Draw);
	TArray<TCMarkersHud::FUnitBillboard> Units;
	auto& Low = Units.AddDefaulted_GetRef(); Low.Centre = FVector2D(70, 40); Low.PxPerMetre = 32;
	Low.SupplyColour = FColor(0xd6, 0xb2, 0x5e);
	auto& Cut = Units.AddDefaulted_GetRef(); Cut.Centre = FVector2D(70, 90); Cut.PxPerMetre = 32;
	Cut.SupplyColour = FColor(0xd4, 0x57, 0x4a);
	TCMarkersHud::DrawUnitBillboards(Canvas.Get(), Units);
	Draw.Flush_GameThread(); FlushRenderingCommands();
	TArray<FLinearColor> Pixels;
	if (!TestTrue(TEXT("actual warning GPU readback"), Resource->ReadLinearColorPixels(Pixels, FReadSurfaceDataFlags(RCM_MinMax)))) return false;
	if (!TestEqual(TEXT("warning target size"), Pixels.Num(), 192 * 128)) return false;
	const auto& Amber = Pixels[40 * 192 + 137]; const auto& Red = Pixels[90 * 192 + 137];
	TestTrue(TEXT("healthy unit without a bar still shows low-supply amber"), Amber.R > .1f && Amber.G > .1f && Amber.B < Amber.G * .5f);
	TestTrue(TEXT("urgent recipient warning is red"), Red.R > .1f && Red.R > Red.G * 2 && Red.R > Red.B * 2);
	TestTrue(TEXT("the hidden health bar does not gain a backdrop"), Pixels[40 * 192 + 70].GetLuminance() < .01f);
	return true;
}
#endif
