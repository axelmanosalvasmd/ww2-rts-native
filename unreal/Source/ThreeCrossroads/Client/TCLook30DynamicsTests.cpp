#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCLook30Dynamics.h"
#include "Client/TCLook30Controls.h"
#include "Client/TCAudio.h"
#include "Client/TCPersistence.h"
#include "Misc/AutomationTest.h"
#include "DSP/DynamicsProcessor.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30Dynamics, "ThreeCrossroads.Look30.MasterDynamics",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCLook30Dynamics::RunTest(const FString&)
{
 const auto S=TCLook30::MasterDynamicsSettings();
 TestTrue(TEXT("source master compressor settings"),S.ThresholdDb==-10 && S.KneeBandwidthDb==6 && S.Ratio==8 && S.AttackTimeMsec==3 && S.ReleaseTimeMsec==250 && !S.bBypass);
 USoundSubmix* Bus=TCLook30::CreateMasterDynamics(GetTransientPackage());
 if (!TestNotNull(TEXT("game master bus"),Bus) || !TestEqual(TEXT("one compressor after summing the mix"),Bus->SubmixEffectChain.Num(),1)) return false;
 Audio::FDynamicsProcessor D; D.Init(48000,2);
 D.SetProcessingMode(Audio::EDynamicsProcessingMode::Compressor); D.SetPeakMode(Audio::EPeakMode::Peak);
 D.SetAnalogMode(S.bAnalogMode); D.SetChannelLinkMode(Audio::EDynamicsProcessorChannelLinkMode::Peak);
 D.SetThreshold(S.ThresholdDb); D.SetKneeBandwidth(S.KneeBandwidthDb); D.SetRatio(S.Ratio);
 D.SetAttackTime(S.AttackTimeMsec); D.SetReleaseTime(S.ReleaseTimeMsec); D.SetLookaheadMsec(S.LookAheadMsec);
 double Peak=0; bool Finite=true;
 for (int32 I=0; I<48000; I++)
 {
  // Three overlapping 0.7-amplitude effects exceed full scale before the shared compressor.
  const float V=2.1f*FMath::Sin(float(2*PI*1000*I/48000.)); float In[2]={V,V}, Out[2]={};
  D.ProcessAudioFrame(In,Out,In);
  Finite &= FMath::IsFinite(Out[0]) && FMath::IsFinite(Out[1]);
  Peak=FMath::Max(Peak,double(FMath::Abs(Out[0])));
 }
 TestTrue(TEXT("overlapping burst stays finite"),Finite);
 TestTrue(TEXT("overlapping burst including its onset is compressed below full scale without silencing"),Peak>.1 && Peak<1);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30Volume, "ThreeCrossroads.Look30.VolumeDragPersistence",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCLook30Volume::RunTest(const FString&)
{
 FTCSettingsStore Store{FString()}; FTCAudioMixer Mixer; Mixer.InitPreferences(Store);
 TCLook30::FVolumeDrag Drag;
 auto Save=[&]{Mixer.SetVolume(Drag.Percent/100.); Store.Set(TEXT("ww2-volume"),FString::Printf(TEXT("%.17g"),Mixer.Volume()));};
 TestTrue(TEXT("press captures slider"),Drag.Begin(37,0,100)); Save();
 TestEqual(TEXT("source continuous percentage, no ten-point steps"),Mixer.Volume(),.37,1e-12);
 TestTrue(TEXT("held drag changes value"),Drag.Move(63,true)); Save();
 Drag.Move(80,false); TestFalse(TEXT("release ends capture"),Drag.Active);
 FTCAudioMixer Restored; Restored.InitPreferences(Store);
 TestEqual(TEXT("dragged value persists into a fresh mixer"),Restored.Volume(),.63,1e-12);
 Restored.ToggleMute(); TestEqual(TEXT("M mutes"),Restored.Volume(),0.,1e-12);
 Restored.ToggleMute(); TestEqual(TEXT("M restores continuous value"),Restored.Volume(),.63,1e-12);
 return true;
}
#endif
