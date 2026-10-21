// Focused CLI-115 replay of events recorded from client/audio.js, with no physical-device claim.
#include "Client/TCAudio.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Client/TCAudio35Fixture.h"
#ifndef TC_AUDIO35_STANDALONE
#include "Client/TCLook30Dynamics.h"
#include "Engine/World.h"
#include "Sound/SoundWave.h"
#endif

namespace TCAudio35
{
bool Replay(FAutomationTestBase& Test, const FString& Kind)
{
 int32 PreviousRows = 0, Base = 0, Scenario = -1;
 TUniquePtr<FTCAudioMixer> Mixer;
 TArray<FTCAudioTraceRow> Trace;
 for (const FEvent& E : Events)
 {
  const int32 Before = PreviousRows; PreviousRows = E.Rows;
  if (Kind != E.Kind) continue;
  if (Scenario != E.Scenario)
  {
   Scenario = E.Scenario; Base = Before; Trace.Reset();
   Mixer = MakeUnique<FTCAudioMixer>(); Mixer->BeginMatch(0, 0); Mixer->SetRunning(true); Mixer->Trace = &Trace;
   for (const auto& Clip : TCAudioRules::Clips()) Mixer->SetLoaded(Clip.Id, Clip.Dur);
  }
  Mixer->AdvanceTo(E.At); Mixer->SetRandom([&E]() { return E.Random; });
  Mixer->Listener(E.FocusX, E.FocusZ, E.Dist, E.Yaw);
  const FVector2D Position(E.X, E.Z);
  if (FString(E.Op) == TEXT("play"))
  {
   FTCAudioPlayOptions Opt; Opt.Gain = E.Gain; Opt.Rate = E.Rate; Opt.Delay = E.Delay; Opt.Dur = E.Dur; Opt.Name = E.Alias;
   Mixer->PlaySound(E.Name, &Position, Opt);
  }
  else if (FString(E.Op) == TEXT("bed")) Mixer->Bed(E.Name, MakeArrayView(&Position, 1));
  const FString Label = FString::Printf(TEXT("%s scenario %d at %.6f"), *Kind, Scenario, E.At);
  Test.TestEqual(Label + TEXT(" live voices"), Mixer->LivePlaying(), E.Live);
  Test.TestEqual(Label + TEXT(" recorded event count"), Trace.Num(), E.Rows - Base);
  for (int32 I = 0; I < FMath::Min(Trace.Num(), E.Rows - Base); ++I)
  {
   const auto& A = Trace[I]; const auto& B = Rows[Base + I];
   Test.TestEqual(Label + TEXT(" start/stop"), A.bStart, B.Start);
   Test.TestEqual(Label + TEXT(" effect"), A.Key, FString(B.Key));
   Test.TestEqual(Label + TEXT(" time"), A.At, B.At, 1e-10);
   Test.TestEqual(Label + TEXT(" offset"), A.Offset, B.Offset, 1e-10);
   Test.TestEqual(Label + TEXT(" pitch"), A.Rate, B.Rate, 1e-10);
   Test.TestEqual(Label + TEXT(" level"), A.Gain, B.Gain, 1e-10);
   Test.TestEqual(Label + TEXT(" pan"), A.Pan, B.Pan, 1e-10);
   Test.TestEqual(Label + TEXT(" loop"), A.bLoop, B.Loop);
  }
 }
 return true;
}
}
#define TC_AUDIO35_TEST(Name) \
 IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAudio35##Name, "ThreeCrossroads.Audio35." #Name, EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter) \
 bool FTCAudio35##Name::RunTest(const FString&) { return TCAudio35::Replay(*this, TEXT(#Name)); }
TC_AUDIO35_TEST(VoiceLimits)
TC_AUDIO35_TEST(Gaps)
TC_AUDIO35_TEST(Pitch)
TC_AUDIO35_TEST(Placement)
TC_AUDIO35_TEST(Loops)
#undef TC_AUDIO35_TEST

#ifndef TC_AUDIO35_STANDALONE
struct FTCAudio35Access
{
 static void Route(UTCAudio* Audio, USoundBase* Sound, USoundSubmix* Bus)
 {
  Audio->MasterDynamics = Bus;
  Audio->RouteToMaster(Sound);
 }
};
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAudio35MasterDynamics, "ThreeCrossroads.Audio35.MasterDynamics", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCAudio35MasterDynamics::RunTest(const FString&)
{
 // audio.js ensureContext: Web Audio seconds become Unreal milliseconds.
 TCAudio35::Replay(*this, TEXT("MasterDynamics"));
 const auto& Expected = TCAudio35::Dynamics;
 const auto S = TCLook30::MasterDynamicsSettings();
 TestEqual(TEXT("threshold dB"), S.ThresholdDb, (float)Expected.Threshold); TestEqual(TEXT("knee dB"), S.KneeBandwidthDb, (float)Expected.Knee);
 TestEqual(TEXT("ratio"), S.Ratio, (float)Expected.Ratio); TestEqual(TEXT("attack ms"), S.AttackTimeMsec, (float)(Expected.AttackSeconds * 1000));
 TestEqual(TEXT("release ms"), S.ReleaseTimeMsec, (float)(Expected.ReleaseSeconds * 1000)); TestFalse(TEXT("compressor enabled"), S.bBypass);
 TestEqual(TEXT("compressor type"), S.DynamicsProcessorType, ESubmixEffectDynamicsProcessorType::Compressor);
 UWorld* World = UWorld::CreateWorld(EWorldType::Game, false);
 if (!TestNotNull(TEXT("isolated world"), World)) return false;
 UTCAudio* Audio = NewObject<UTCAudio>(World);
 USoundSubmix* Bus = TCLook30::CreateMasterDynamics(Audio);
 TestEqual(TEXT("one master processor"), Bus->SubmixEffectChain.Num(), 1);
 if (Bus->SubmixEffectChain.Num() == 1)
 {
  auto* Preset = Cast<USubmixEffectDynamicsProcessorPreset>(Bus->SubmixEffectChain[0]);
  if (TestNotNull(TEXT("actual dynamics preset"), Preset))
  {
   Preset->Init();
   TestEqual(TEXT("installed threshold"), Preset->Settings.ThresholdDb, S.ThresholdDb);
   TestEqual(TEXT("installed knee"), Preset->Settings.KneeBandwidthDb, S.KneeBandwidthDb);
   TestEqual(TEXT("installed ratio"), Preset->Settings.Ratio, S.Ratio);
   TestFalse(TEXT("installed processor enabled"), Preset->Settings.bBypass);
   TestEqual(TEXT("installed attack"), Preset->Settings.AttackTimeMsec, S.AttackTimeMsec);
   TestEqual(TEXT("installed release"), Preset->Settings.ReleaseTimeMsec, S.ReleaseTimeMsec);
   TestEqual(TEXT("initialized audio-thread threshold"), Preset->GetSettings().ThresholdDb, S.ThresholdDb);
   TestEqual(TEXT("initialized audio-thread ratio"), Preset->GetSettings().Ratio, S.Ratio);
  }
 }
 // Same production routing helper used for effect waves, voice waves and procedural beds.
 for (int32 I = 0; I < 3; ++I)
 {
  USoundWave* Wave = NewObject<USoundWave>(Audio); FTCAudio35Access::Route(Audio, Wave, Bus);
  TestTrue(TEXT("base submix enabled"), Wave->bEnableBaseSubmix);
  TestTrue(TEXT("source routed through shared master"), Wave->SoundSubmixObject == Bus);
 }
 World->DestroyWorld(false);
 return true;
}
#endif
#endif
