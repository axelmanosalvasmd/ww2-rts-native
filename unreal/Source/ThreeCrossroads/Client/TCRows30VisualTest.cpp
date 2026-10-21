#include "Client/TCRows30Harness.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCEffects.h"
#include "Client/TCGraphics.h"
using namespace TCRows30;

TC_ROWS30_TEST(FTCRows30HouseTest,"CLI40.NearestHousePiece");
bool FTCRows30HouseTest::RunTest(const FString&)
{
 // Reference houseAt raycasts every pickable solid, then advances 0.4 m into its grid cell.
 // A thin wall ends at x=1.9, just outside the house cell x=[2,4). Without the step this is rejected.
 auto Shape=[](double X,double Width) { TCWorldCore::FPiece P; P.Kind=TCWorldCore::EPiece::Stone; P.X=X; P.Y=1; P.Z=1; P.Sx=Width; P.Sy=2; P.Sz=2; return TCWorldCore::ShapeOf(P); };
 FTCWorldObjects O; FTCHousePick Hit;
 const auto Roof=Shape(2.,.2); FTCRows30Access::HouseFixture(O,{Roof});
 TestTrue(TEXT("0.4m ray step enters the B cell"),O.PickHouse(FVector(0,100,100),FVector(1,0,0),nullptr,Hit));
 TestEqual(TEXT("chosen B cell"),Hit.Cell,1);
 TestTrue(TEXT("reports nearest intersection distance, before stepping"),FMath::IsNearlyEqual(Hit.DistanceMetres,1.9,.00001));
 const auto Occluder=Shape(.8,.2); FTCRows30Access::HouseFixture(O,{Roof,Occluder});
 TestFalse(TEXT("nearest non-house solid blocks the farther roof"),O.PickHouse(FVector(0,100,100),FVector(1,0,0),nullptr,Hit));
 FTCRows30Access::HouseFixture(O,{});
 TestFalse(TEXT("ground alone never selects a house"),O.PickHouse(FVector(300,100,1000),FVector(0,0,-1),nullptr,Hit));
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30GoreTest,"CLI123.GoreEmitterGate");
bool FTCRows30GoreTest::RunTest(const FString&)
{
 FTCGraphics& Graphics=FTCGraphics::Get();
 // Save and restore preferences in memory, so this test never persists a switch in the player's settings.
 const bool Was=Graphics.Gore(); FTCRows30Access::MemoryGraphics(Graphics,true);
 FTCCombatEffects Fx; FTCMatchState State; Fx.Start(State);
 Graphics.SetGore(false); Fx.Gore(10,10,1);
 TestTrue(TEXT("Gore off submits no blood, gibs or stain"),Fx.Particles().IsEmpty() && Fx.DecalsOn(2)==0);
 Graphics.SetGore(true); Fx.Gore(10,10,1);
 bool Gib=false,Mist=false; for(const auto& P:Fx.Particles()) { Gib|=P.Preset==ETCFxPreset::Gib; Mist|=P.Preset==ETCFxPreset::Mist; }
 TestTrue(TEXT("same blast gore entry emits scraps and blood mist when on"),Gib && Mist);
 TestTrue(TEXT("gore on submits the blood stain"),Fx.DecalsOn(2)>0);
 const int32 Particles=Fx.Particles().Num(), Stains=Fx.DecalsOn(2);
 Graphics.SetGore(false); Fx.Gore(20,20,1);
 TestEqual(TEXT("toggling off suppresses the next blast's particles"),Fx.Particles().Num(),Particles);
 TestEqual(TEXT("toggling off suppresses the next stain"),Fx.DecalsOn(2),Stains);
 Graphics.SetGore(Was); FTCRows30Access::MemoryGraphics(Graphics,false);
 return !HasAnyErrors();
}
#endif
