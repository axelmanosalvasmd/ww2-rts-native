#include "Client/TCRows30Harness.h"
#if WITH_DEV_AUTOMATION_TESTS
using namespace TCRows30;

TC_ROWS30_TEST(FTCRows30PanTest,"CLI31.CameraPanInput");
bool FTCRows30PanTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing();
 const FKey Letter[]={EKeys::W,EKeys::A,EKeys::S,EKeys::D}; const FKey Arrow[]={EKeys::Up,EKeys::Left,EKeys::Down,EKeys::Right};
 const FVector2D Local[]={FVector2D(1,0),FVector2D(0,-1),FVector2D(-1,0),FVector2D(0,1)};
 const double Factors[]={.6,1,1.6}; const FVector2D Origin(100,60); const float Dt=.125f, Dist=30;
 for(float Yaw:{-90.f,-32.f}) for(int32 Speed=0;Speed<3;Speed++) for(int32 I=0;I<4;I++)
 {
  const double R=FMath::DegreesToRadians(Yaw);
  const FVector2D Fwd(FMath::Cos(R),FMath::Sin(R)), Right(-FMath::Sin(R),FMath::Cos(R));
  const FVector2D Expected=Origin+(Fwd*Local[I].X+Right*Local[I].Y)*Dist*1.1*Factors[Speed]*Dt;
  FVector2D Result;
  for(bool bArrow:{false,true})
  { FTCRows30Access::Pose(*S.PC,Origin,Yaw,Dist,Speed); S.Held(bArrow?Arrow[I]:Letter[I]); FTCRows30Access::Camera(*S.PC,Dt); S.Release();
   TestTrue(TEXT("actual key moves by reference yaw, distance, dt and speed factor"),S.PC->ViewFocus().Equals(Expected,.0001));
   if(!bArrow) Result=S.PC->ViewFocus(); else TestTrue(TEXT("arrows and WASD displace identically"),Result.Equals(S.PC->ViewFocus(),.00001)); }
 }
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30YawTest,"CLI33.CameraYawInput");
bool FTCRows30YawTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing(); const float Dt=.125f;
 for(const FKey& K:{EKeys::Q,EKeys::E})
 { FTCRows30Access::Pose(*S.PC,FVector2D(100,60),-90,30); S.Held(K); FTCRows30Access::Camera(*S.PC,Dt); S.Release();
  const float Expected=-90+(K==EKeys::Q?1:-1)*FMath::RadiansToDegrees(1.6f*Dt);
  TestTrue(TEXT("held Q/E rotate oppositely at original 1.6 rad/s"),FMath::IsNearlyEqual(S.PC->ViewYaw(),Expected,.0001f)); }
 TestTrue(TEXT("middle drag keeps original 0.006 rad/pixel"),FMath::IsNearlyEqual(TCCameraFeel::MiddleDragYawDegrees(25),FMath::RadiansToDegrees(.15f),.00001f));
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30WideTest,"CLI34.WideMapFit");
bool FTCRows30WideTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing();
 for(FIntPoint Size:{FIntPoint(1280,720),FIntPoint(900,1200)}) for(float Yaw:{-90.f,-32.f})
 {
  FTCRows30Access::Pose(*S.PC,FVector2D(-999,999),Yaw,30); FTCRows30Access::Wide(*S.PC,*S.M,Size.X,Size.Y);
  TestTrue(TEXT("wide() computes a valid fit for rectangular raised ground"),S.PC->HasWideView());
  FTCRows30Access::Pose(*S.PC,FVector2D(-999,999),Yaw,S.PC->WideDistance()); FTCRows30Access::Clamp(*S.PC,*S.M);
  TestTrue(TEXT("widest focus clamps to computed centre"),S.PC->ViewFocus().Equals(S.PC->WideFocus(),.00001));
  const auto Focus=S.PC->ViewFocus(); const float H=S.M->Terrain->HeightAt(Focus.X,Focus.Y);
  FTCPinhole View(FVector(Focus.X,Focus.Y,H),Yaw,54.43099f,S.PC->WideDistance(),double(Size.X)/Size.Y);
  const double W=S.M->Terrain->MapW()*2., Depth=S.M->Terrain->MapH()*2.;
  for(FVector2D C:{FVector2D(0,0),FVector2D(W,0),FVector2D(W,Depth),FVector2D(0,Depth)})
  { FVector2D P; const double Z=S.M->Terrain->HeightAt(FMath::Min(C.X,W-.01),FMath::Min(C.Y,Depth-.01));
   TestTrue(TEXT("every map corner projects in front"),View.Project(FVector(C.X,C.Y,Z),Size.X,Size.Y,P));
   TestTrue(TEXT("all corners fit original four percent margin"),P.X>=Size.X*.04-.1 && P.X<=Size.X*.96+.1 && P.Y>=Size.Y*.04-.1 && P.Y<=Size.Y*.96+.1); }
 }
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30FollowTest,"CLI37.FollowAlertHome");
bool FTCRows30FollowTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing(); S.Unit(1,TEXT("rifle")); S.Unit(2,TEXT("hq"),0,30,40); S.PC->Selected={1};
 TestTrue(TEXT("Shift+Space invokes production binding"),FTCRows30Access::Binding(*S.PC,TEXT("Space"),true));
 TestEqual(TEXT("selection is followed"),S.PC->FollowedId(),1);
 S.M->State.Units[1].X=120; S.M->State.Units[1].Z=80; FTCRows30Access::Camera(*S.PC,.125f);
 TestTrue(TEXT("follow tracks delivered movement"),S.PC->ViewFocus().Equals(FVector2D(120,80),.001));
 S.Held(EKeys::W); FTCRows30Access::Camera(*S.PC,.125f); S.Release(); TestEqual(TEXT("camera input cancels follow"),S.PC->FollowedId(),0);
 S.PC->NewestAlertAt=[](FVector2D& P) { P=FVector2D(70,50); return true; };
 FTCRows30Access::Binding(*S.PC,TEXT("Space")); TestTrue(TEXT("Space visits newest alert"),S.PC->ViewFocus().Equals(FVector2D(70,50),.001));
 S.PC->NewestAlertAt=[](FVector2D&) { return false; }; FTCRows30Access::Binding(*S.PC,TEXT("Space"));
 TestTrue(TEXT("Space without alert centres selection"),S.PC->ViewFocus().Equals(FVector2D(120,80),.001));
 S.M->State.Mode=TEXT("classic"); S.M->State.ModeRow.Kind=TEXT("classic");
 S.PC->StartView(FVector2D(30,40),FVector2D(256,128)); S.PC->FocusOn(FVector2D(100,60));
 FTCRows30Access::Binding(*S.PC,TEXT("KeyH")); TestTrue(TEXT("Classic H selects only the HQ"),S.PC->Selected.Num()==1 && S.PC->Selected.Contains(2));
 return !HasAnyErrors();
}
#endif
