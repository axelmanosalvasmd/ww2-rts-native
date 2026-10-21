// One game-audio bus after summing effects, UI, voices and ambience.
#pragma once
#include "CoreMinimal.h"
#include "Sound/SoundSubmix.h"
#include "SubmixEffects/AudioMixerSubmixEffectDynamicsProcessor.h"
namespace TCLook30
{
 inline FSubmixEffectDynamicsProcessorSettings MasterDynamicsSettings()
 {
  FSubmixEffectDynamicsProcessorSettings S;
  // client/audio.js init(): DynamicsCompressor after the master gain, times converted to milliseconds.
  S.ThresholdDb=-10; S.KneeBandwidthDb=6; S.Ratio=8;
  S.AttackTimeMsec=3; S.ReleaseTimeMsec=250;
  S.LookAheadMsec=6; S.InputGainDb=S.OutputGainDb=0;
  S.bBypass=false; S.bAnalogMode=false;
  S.PeakMode=ESubmixEffectDynamicsPeakMode::Peak;
  S.LinkMode=ESubmixEffectDynamicsChannelLinkMode::Peak;
  return S;
 }
 inline USoundSubmix* CreateMasterDynamics(UObject* Owner)
 {
  USoundSubmix* Bus=NewObject<USoundSubmix>(Owner);
  USubmixEffectDynamicsProcessorPreset* Compressor=NewObject<USubmixEffectDynamicsProcessorPreset>(Bus);
  // Init copies the stored properties into the audio-thread settings.
  Compressor->Settings=MasterDynamicsSettings();
  Compressor->SetSettings(Compressor->Settings);
  Bus->SubmixEffectChain.Add(Compressor);
  return Bus;
 }
}
