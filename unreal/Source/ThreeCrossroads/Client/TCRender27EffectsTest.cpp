#include "Client/TCEffects.h"
#include "Client/TCGraphics.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCRender27LowFlipbookTest, "ThreeCrossroads.Render27.LowFlipbook",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCRender27LowFlipbookTest::RunTest(const FString&)
{
	const FTCGraphicsLevelScope Restore(ETCGraphicsLevel::High);
	FTCMatchState State;
	State.bHasMatch = true;
	FTCCombatEffects Fx;
	Fx.Start(State);
	FTCFxContext Context;
	Context.HeightAt = [](float, float) { return 0.f; };
	FTCShot& Shot = State.Shots.AddDefaulted_GetRef();
	Shot.K = TEXT("shell"); Shot.bHasPos = true; Shot.X = 20; Shot.Z = 20;
	Fx.Snapshot(State, nullptr, Context, 0);
	Fx.Plume(TEXT("fire"), 25, 0, 25);
	Fx.Frame(Context, .137, .137f);
	TArray<FTCFxSpriteDraw> High, Low, Again;
	Fx.SpriteDraws(High);
	bool bOnce = false, bLoop = false;
	for (const FTCFxSpriteDraw& Draw : High)
	{
		bOnce |= Draw.CellA >= 24 && Draw.CellA < 39 && Draw.Blend > 0;
		bLoop |= Draw.CellA >= 12 && Draw.CellA < 24 && Draw.Blend > 0;
	}
	TestTrue(TEXT("control includes a fractional one-shot fireball and a looping flame"), bOnce && bLoop);
	FTCGraphics::Get().Set(ETCGraphicsLevel::Low, false);
	Fx.SpriteDraws(Low);
	if (!TestEqual(TEXT("switching quality retains the same live particles"), Low.Num(), High.Num())) return false;
	for (int32 I = 0; I < High.Num(); I++)
	{
		// Pinned fx.js:353 retains both frame indices, but Low writes a zero frame blend.
		TestEqual(TEXT("Low never interpolates adjacent flipbook frames"), Low[I].Blend, 0.f);
		TestTrue(TEXT("quality changes only frame interpolation, not particle age, geometry or colour"),
			Low[I].CellA == High[I].CellA && Low[I].CellB == High[I].CellB && Low[I].Center == High[I].Center
			&& Low[I].Size == High[I].Size && Low[I].Colour == High[I].Colour && Low[I].Alpha == High[I].Alpha
			&& Low[I].Rot == High[I].Rot && Low[I].Mode == High[I].Mode);
	}
	FTCGraphics::Get().Set(ETCGraphicsLevel::High, false);
	Fx.SpriteDraws(Again);
	if (!TestEqual(TEXT("High restores the same particle draws"), Again.Num(), High.Num())) return false;
	for (int32 I = 0; I < High.Num(); I++)
		TestEqual(TEXT("High resumes the original fractional blend without advancing particles"), Again[I].Blend, High[I].Blend);
	return true;
}
#endif
