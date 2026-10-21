#include "Client/TCEffects.h"
#include "Client/TCAudio.h"
#include "Client/TCGraphics.h"
#include "Client/TCUnitTable.h"
#include "Client/TCFx31AudioFixture.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

namespace TCFx31Test
{
// Recorded protocol-shaped messages, kept inline so editor and packaged tests use the same inputs.
struct FReplay
{
 FTCMatchState State;
 FTCCombatEffects Fx;
 FTCUnitTable Table;
 FTCFxContext Context;
 double Clock = 0;
 bool Init()
 {
  FString Error;
  if (!Table.Load(Error)) return false;
  Context.Table = &Table;
  Context.HeightAt = [](float, float) { return 0.f; };
  Feed(TEXT(R"({"t":"lobby","code":"fx31","state":"play","mode":"conquest","you":0,"players":[{"name":"Me","team":0},{"name":"Foe","team":1}]})"));
  return Feed(TEXT(R"({"t":"start","matchId":31,"you":0,"names":["Me","Foe"],"teams":[0,1],"factions":[0,1],"map":{"name":"fx31","w":8,"h":8,"rows":["........",".HTB....","........","........","........","........","........","........"],"points":[]}})"));
 }
 bool Feed(const FString& Line)
 {
  TSharedPtr<FJsonObject> Raw;
  if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Line), Raw)) return false;
  const FString Kind = Raw->GetStringField(TEXT("t"));
  if (Kind == TEXT("lobby")) State.ApplyLobby(*Raw);
  else if (Kind == TEXT("start")) { State.ApplyStart(*Raw); Fx.Start(State); }
  else { State.ApplySnapshot(*Raw); Fx.Snapshot(State, Raw.Get(), Context, Clock); }
  return true;
 }
 void Step(float Dt) { while (Dt > 1e-6f) { const float D = FMath::Min(Dt, .05f); Clock += D; Fx.Frame(Context, Clock, D); Dt -= D; } }
 int32 Count(ETCFxPreset Preset) const
 {
  int32 N = 0; for (const auto& P : Fx.Particles()) N += P.Preset == Preset; return N;
 }
};
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFx31CombatTest, "ThreeCrossroads.Fx31.CLI110", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCFx31CombatTest::RunTest(const FString&)
{
 const FTCGraphicsLevelScope Quality(ETCGraphicsLevel::High);
 TCFx31Test::FReplay R; if (!TestTrue(TEXT("load replay"), R.Init())) return false;
 R.Feed(TEXT(R"({"t":"s","tick":1,"units":[[2,"tank",1,10,10,0,0,360,0,0,0,0,0,0,1]],"shots":[{"k":"mg","x":10,"z":10,"t":2,"hit":true}]})"));
 TestEqual(TEXT("delivered bullet hitting armor makes three ricochet sparks"), R.Count(ETCFxPreset::Spark), 3);
 TestEqual(TEXT("hidden source cannot flash"), R.Fx.ShooterLog().Num(), 0);
 R.Fx.Reset();
 R.Feed(TEXT(R"({"t":"s","tick":2,"units":[[3,"bunker",1,10,10,0,0,500,0,0,0,0,0,0,1]],"shots":[{"k":"mg","x":10,"z":10,"t":3,"hit":true}]})"));
 TestEqual(TEXT("wall hit makes three masonry chips"), R.Count(ETCFxPreset::Masonry), 3);
 TestEqual(TEXT("wall hit has no armor ricochet"), R.Count(ETCFxPreset::Spark), 0);
 R.Fx.Reset();
 R.Feed(TEXT(R"({"t":"s","tick":3,"units":[],"shots":[{"k":"shell","x":8,"z":8},{"k":"bomb","x":12,"z":12}]})"));
 TestEqual(TEXT("shell and bomb create two craters"), R.Fx.DecalsOn(0), 2);
 TestTrue(TEXT("shell and bomb create fireballs"), R.Count(ETCFxPreset::Fireball) > 0);
 bool ShellRadius = false, BombRadius = false;
 for (const auto& D : R.Fx.Decals()) if (D.bOn) { ShellRadius |= FMath::IsNearlyEqual(D.R, 3.34f, .00001f); BombRadius |= FMath::IsNearlyEqual(D.R, 4.6f, .00001f); }
 TestTrue(TEXT("weapon sizes produce 3.34m shell and 4.6m bomb craters"), ShellRadius && BombRadius);
 for (const TCHAR* Material : {TEXT("wood"), TEXT("steel"), TEXT("soil"), TEXT("road"), TEXT("stone"), TEXT("concrete")})
 {
  R.Fx.Reset();
  R.Feed(FString::Printf(TEXT(R"({"t":"s","tick":20,"units":[],"shots":[{"k":"contact","flight":41,"seq":1,"time":1,"kind":"mg","effect":"bullet","material":"%s","x":8,"y":1,"z":8,"terminal":false,"impulse":1.5,"dir":0}]})"), Material));
  const FString M(Material);
  const ETCFxPreset Preset = M == TEXT("wood") ? ETCFxPreset::Wood : M == TEXT("steel") ? ETCFxPreset::Metal : M == TEXT("soil") || M == TEXT("road") ? ETCFxPreset::Clod : ETCFxPreset::Masonry;
  TestEqual(FString::Printf(TEXT("delivered %s contact emits three matching chips"), Material), R.Count(Preset), 3);
 }
 // Every direct-fire style must pass through the held-shooter branch, rather than becoming a generic explosion.
 for (const TCHAR* Type : {TEXT("rifle"), TEXT("engineer"), TEXT("ranger"), TEXT("commando"), TEXT("mg"), TEXT("sniper"), TEXT("at"), TEXT("tank"), TEXT("medium"), TEXT("tiger"), TEXT("churchill"), TEXT("tankdestroyer"), TEXT("flamer"), TEXT("halftrack"), TEXT("gunboat"), TEXT("armoredcar"), TEXT("flaktrack"), TEXT("fighter"), TEXT("attacker")})
 {
  R.Fx.Reset();
  const FString Message = FString::Printf(TEXT(R"({"t":"s","tick":4,"units":[[1,"%s",0,4,4,0,0,100,0,0,0,0,0,0,1]],"shots":[{"k":"%s","f":1,"x":12,"z":12,"hit":false}]})"), Type, Type);
  R.Feed(Message);
  bool Muzzle = false;
  for (int I = 0; I < 20; ++I) { R.Step(.01f); Muzzle |= R.Count(ETCFxPreset::Muzzle) + R.Count(ETCFxPreset::Flash) > 0; }
  TestEqual(FString::Printf(TEXT("%s uses a held model muzzle"), Type), R.Fx.ShooterLog().Num(), 1);
  TestTrue(FString::Printf(TEXT("%s spawns muzzle particles"), Type), Muzzle);
 }
 {
  const FTCGraphicsLevelScope Low(ETCGraphicsLevel::Low);
  R.Fx.Reset();
  R.Feed(TEXT(R"({"t":"s","tick":5,"units":[[2,"tank",1,10,10,0,0,360,0,0,0,0,0,0,1]],"shots":[{"k":"mg","x":10,"z":10,"t":2,"hit":true}]})"));
  TestEqual(TEXT("Low bullet hitting armor emits one ricochet spark"), R.Count(ETCFxPreset::Spark), 1);
 }
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFx31SmokeTest, "ThreeCrossroads.Fx31.CLI112", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCFx31SmokeTest::RunTest(const FString&)
{
 const FTCGraphicsLevelScope Quality(ETCGraphicsLevel::Low);
 TCFx31Test::FReplay R; if (!TestTrue(TEXT("load replay"), R.Init())) return false;
 R.Feed(TEXT(R"({"t":"s","tick":1,"units":[],"shots":[],"smokes":[[6,6,7,101],[12,12,7,102]]})"));
 TestEqual(TEXT("two numeric cloud ids each spawn seven Low puffs"), R.Count(ETCFxPreset::Screen), 14);
 TestEqual(TEXT("two flashes per Low cloud"), R.Count(ETCFxPreset::Flash), 4);
 R.Feed(TEXT(R"({"t":"s","tick":2,"units":[],"shots":[],"smokes":[[7,6,7,101],[12,13,7,102]]})"));
 TestEqual(TEXT("same numeric ids drift without a second birth burst"), R.Count(ETCFxPreset::Screen), 14);
 R.State.Wx.bKnown = true; R.State.Wx.WindAngle = 0; R.State.Wx.WindStrength = 1;
 R.Fx.Snapshot(R.State, nullptr, R.Context, R.Clock);
 TestTrue(TEXT("wx wind strength is 0.85 metres/sec"), FMath::IsNearlyEqual(R.Fx.Wind().X, .85, 1e-9));
 R.Fx.Reset();
 R.Feed(TEXT(R"({"t":"s","tick":3,"units":[],"shots":[],"fires":[9,10,11],"smokes":[[3,3,7,103]]})"));
 TestEqual(TEXT("new cloud over burning hedge becomes fire smoke"), R.Count(ETCFxPreset::FireSmoke), 3);
 R.Step(.4f);
 TestTrue(TEXT("burning cells emit flame"), R.Count(ETCFxPreset::Flame) > 0);
 R.Feed(TEXT(R"({"t":"s","tick":4,"units":[],"shots":[],"fires":[],"smokes":[]})"));
 TestEqual(TEXT("hedge and woods scorch, house leaves rubble"), R.Fx.DecalsOn(1), 2);
 R.Fx.Reset();
 R.Feed(TEXT(R"({"t":"s","tick":5,"units":[[1,"tank",0,8,8,0,0,360,0,0,0,0,0,0,1]],"shots":[]})"));
 R.Feed(TEXT(R"({"t":"s","tick":6,"units":[],"shots":[{"k":"hurt","t":1,"x":8,"z":8,"kill":true}]})"));
 TestEqual(TEXT("held vehicle removed after delivered kill makes one wreck fire"), R.Fx.Stats().Wrecks, 1);
 R.Step(.5f);
 TestTrue(TEXT("wreck fire emits flame and long black smoke"), R.Count(ETCFxPreset::Flame) > 0 && R.Count(ETCFxPreset::WreckSmoke) > 0);
 TestEqual(TEXT("Low wreck fire omits embers"), R.Count(ETCFxPreset::Ember), 0);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFx31AirTest, "ThreeCrossroads.Fx31.CLI113", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCFx31AirTest::RunTest(const FString&)
{
 const FTCGraphicsLevelScope Quality(ETCGraphicsLevel::High);
 TCFx31Test::FReplay R; if (!TestTrue(TEXT("load replay"), R.Init())) return false;
 R.Feed(TEXT(R"({"t":"s","tick":1,"units":[],"covers":[[8,8,25,45]],"shots":[{"k":"bombing","x":8,"z":8,"dir":0,"fo":1},{"k":"chutes","x":8,"z":8}]})"));
 TestEqual(TEXT("delivered fighter cover ring"), R.Fx.CoverRings().Num(), 1);
 if (R.Fx.CoverRings().Num()) { TestEqual(TEXT("fighter cover radius"), R.Fx.CoverRings()[0].Radius, 25.); TestEqual(TEXT("fighter cover time"), R.Fx.CoverRings()[0].Seconds, 45.); }
 R.Step(.1f);
 if (!TestEqual(TEXT("one delivered bomber pass"), R.Fx.AircraftNow().Num(), 1)) return false;
 const auto& A = R.Fx.AircraftNow()[0];
 TestEqual(TEXT("bomber retains recipient-delivered owner"), A.Owner, 1);
 TestTrue(TEXT("bomber has one-second lead"), FMath::IsNearlyEqual(A.P.X, 8 + (1.1 / 3 - .5) * 140, 1e-5));
 int32 Chutes = 0; for (const auto& P : R.Fx.Particles()) Chutes += P.Look == ETCFxLook::Canopy;
 TestEqual(TEXT("five chutes with three-second descent"), Chutes, 5);
 R.Step(.2f);
 int32 Bombs = 0; for (const auto& P : R.Fx.Particles()) Bombs += P.Look == ETCFxLook::Grenade && P.Preset == ETCFxPreset::None;
 TestEqual(TEXT("two bombs are falling at 0.3 seconds before their delivered impacts"), Bombs, 2);
 R.Feed(TEXT(R"({"t":"s","tick":2,"units":[],"shots":[{"k":"flak","x":8,"z":8},{"k":"bomb","x":8,"z":8}]})"));
 R.Step(.5f);
 TestTrue(TEXT("flak produces dark sky puffs"), R.Count(ETCFxPreset::Flak) > 0);
 TestTrue(TEXT("delivered bomb impact makes a crater"), R.Fx.DecalsOn(0) > 0);
 R.Fx.Reset();
 TArray<FString> Heard;
 R.Context.TimedSound = [&Heard](const FString& Name, double, double, double, double) { Heard.Add(Name); };
 R.Feed(TEXT(R"({"t":"s","tick":3,"units":[],"shots":[{"k":"strafe","x":8,"z":8,"dir":0}]})"));
 R.Step(1.7f);
 TestEqual(TEXT("one gun burst recording for the whole strafing pass"), Heard.FilterByPredicate([](const FString& Name) { return Name == TEXT("strafe"); }).Num(), 1);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFx31GroundTest, "ThreeCrossroads.Fx31.CLI114", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCFx31GroundTest::RunTest(const FString&)
{
 const FTCGraphicsLevelScope Quality(ETCGraphicsLevel::High);
 TCFx31Test::FReplay R; if (!TestTrue(TEXT("load replay"), R.Init())) return false;
 R.Feed(TEXT(R"({"t":"s","tick":1,"units":[],"shots":[{"k":"collapse","x":8,"z":8}]})"));
 TestEqual(TEXT("house collapse has 28 rolling dust puffs"), R.Count(ETCFxPreset::Collapse), 28);
 TestEqual(TEXT("house collapse has 15 masonry chunks"), R.Count(ETCFxPreset::Masonry), 15);
 TestEqual(TEXT("house collapse has three wood chunks"), R.Count(ETCFxPreset::Wood), 3);
 R.Fx.Reset();
 for (int I = 0; I < 20; ++I)
  R.Feed(FString::Printf(TEXT(R"({"t":"s","tick":%d,"units":[[1,"tank",0,8,8,0,0,360,0,0,0,0,32768,0,1],[2,"rifle",0,4,4,0,0,100,0,0,0,0,16,0,1]],"shots":[]})"), I + 2));
 TestTrue(TEXT("delivered dry-ground flag makes vehicle trails"), R.Count(ETCFxPreset::RoadDust) > 0);
 TestTrue(TEXT("digging makes earth clods"), R.Count(ETCFxPreset::Clod) > 0);
 for (const auto& P : R.Fx.Particles()) if (P.Preset == ETCFxPreset::RoadDust)
 {
  TestTrue(TEXT("trail starts 1.6m behind heading, with 0.7m jitter"), P.P.X >= 5.7 && P.P.X <= 7.1);
  TestTrue(TEXT("trail life is 1.4 to 2.4 seconds"), P.Life >= 1.4f && P.Life <= 2.4f);
 }
 {
  const FTCGraphicsLevelScope Low(ETCGraphicsLevel::Low);
  R.Fx.Reset();
  R.Feed(TEXT(R"({"t":"s","tick":23,"units":[],"shots":[{"k":"collapse","x":8,"z":8}]})"));
  TestEqual(TEXT("Low collapse has 12 dust puffs"), R.Count(ETCFxPreset::Collapse), 12);
  TestEqual(TEXT("Low collapse has seven masonry chunks"), R.Count(ETCFxPreset::Masonry), 7);
  TestEqual(TEXT("Low collapse has one wood chunk"), R.Count(ETCFxPreset::Wood), 1);
 }
 R.Fx.Reset();
 R.Feed(TEXT(R"({"t":"s","tick":24,"weather":"mud","units":[[1,"tank",0,8,8,0,0,360,0,0,0,0,0,0,1]],"shots":[]})"));
 TestEqual(TEXT("mud without dry-ground flag cannot invent a dust trail"), R.Count(ETCFxPreset::RoadDust), 0);
 return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCFx31SoundTest, "ThreeCrossroads.Fx31.CLI115", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCFx31SoundTest::RunTest(const FString&)
{
 TCFx31Test::FReplay R; if (!TestTrue(TEXT("load replay"), R.Init())) return false;
 TestEqual(TEXT("original library has 35 clips"), TCAudioRules::Clips().Num(), 35);
 FTCAudioMixer Mixer; Mixer.BeginMatch(0, 0); Mixer.SetRunning(true); Mixer.SetRandom([]() { return .5; });
 TArray<FTCAudioTraceRow> Trace; Mixer.Trace = &Trace;
 TSharedPtr<FJsonObject> Expected;
 if (!TestTrue(TEXT("original metadata fixture parses"), FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TCFx31Test::AudioIndex), Expected))) return false;
 TSharedPtr<FJsonObject> Mixes;
 if (!TestTrue(TEXT("original mix fixture parses"), FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(TCFx31Test::MixIndex), Mixes))) return false;
 for (const auto& Entry : Mixes->Values)
 {
  const auto& Values = Entry.Value->AsObject(); const auto* Mix = TCAudioRules::FindMix(FString(Entry.Key));
  if (!TestNotNull(TEXT("original per-effect mix exists"), Mix)) return false;
  double Gain = 0, Range = 0, Max = 0, Gap = 0, Pitch = 0;
  Values->TryGetNumberField(TEXT("gain"), Gain); Values->TryGetNumberField(TEXT("range"), Range); Values->TryGetNumberField(TEXT("max"), Max); Values->TryGetNumberField(TEXT("gap"), Gap); Values->TryGetNumberField(TEXT("pitch"), Pitch);
  TestEqual(TEXT("mix gain"), Mix->Gain, Gain); TestEqual(TEXT("mix range"), Mix->Range, Range); TestEqual(TEXT("mix concurrency cap"), Mix->Max, int32(Max)); TestEqual(TEXT("mix gap"), Mix->Gap, Gap); TestEqual(TEXT("mix pitch spread"), Mix->Pitch, Pitch);
 }
 for (const auto& Clip : TCAudioRules::Clips())
 {
  const TSharedPtr<FJsonObject>* Meta = nullptr;
  if (!TestTrue(TEXT("clip exists in original index"), Expected->TryGetObjectField(Clip.Id, Meta))) return false;
  double Boost = 0; bool Loop = false;
  (*Meta)->TryGetNumberField(TEXT("boost"), Boost); (*Meta)->TryGetBoolField(TEXT("loop"), Loop);
  TestEqual(TEXT("original duration"), Clip.Dur, (*Meta)->GetNumberField(TEXT("dur")));
  TestEqual(TEXT("original boost"), Clip.Boost, Boost); TestEqual(TEXT("original loop flag"), Clip.bLoop, Loop);
  Trace.Reset();
  FTCAudioPlayOptions Options; Options.Dur = Clip.bLoop ? 1.7 : 0;
  Mixer.SetLoaded(Clip.Id, Clip.Dur); Mixer.PlaySound(Clip.Id, nullptr, Options);
  if (!TestTrue(FString::Printf(TEXT("%s starts using original clip"), Clip.Id), Trace.Num() > 0 && Trace[0].bStart && Trace[0].Key == Clip.Id)) return false;
  TestEqual(TEXT("loop flag survives timed loop launch"), Trace[0].bLoop, Clip.bLoop);
  Mixer.Advance(Clip.Dur + 1);
 }
 FTCAudioCues Cues;
 const FString Message = TEXT(R"({"t":"s","tick":1,"units":[],"shots":[{"k":"shell","x":8,"z":8},{"k":"bomb","x":12,"z":12},{"k":"smokeshells","x":5,"z":5},{"k":"salvo","x":8,"z":8,"n":3,"physical":true}],"strikes":[["artillery",8,8,0,1.2]],"smokes":[[5,5,7,101]]})");
 R.Feed(Message);
 auto Shots = TCAudioView::Shots(R.State);
 auto Units = TCAudioView::Units(R.State, R.Table);
 TSharedPtr<FJsonObject> Raw; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Message), Raw);
 TArray<FTCAudioStrike> Strikes; TArray<FTCAudioSmoke> Smokes;
 TCAudioView::Extras(*Raw, Shots, Strikes, Smokes);
 TestEqual(TEXT("raw adapter preserves salvo count"), Shots.Last().N.Get(0), 3);
 TestTrue(TEXT("raw adapter preserves physical skip flag"), Shots.Last().bPhysical);
 TestEqual(TEXT("raw adapter preserves artillery countdown"), Strikes.Num(), 1);
 TestEqual(TEXT("raw adapter preserves smoke id"), Smokes.Num() ? Smokes[0].Id.Get(0) : 0, 101);
 FTCAudioSnapshot Snapshot; Snapshot.Tick = 1; Snapshot.Shots = Shots; Snapshot.Units = &Units;
 Snapshot.Strikes = Strikes; Snapshot.Smokes = Smokes;
 Trace.Reset(); Cues.Snapshot(Snapshot, Mixer);
 Cues.Frame(.001, Mixer);
 bool Shell = false, Bomb = false, Smoke = false;
 for (const auto& Cue : Trace) { Shell |= Cue.Key == TEXT("blast_large"); Bomb |= Cue.Key == TEXT("bomb"); Smoke |= Cue.Key == TEXT("smoke"); }
 TestTrue(TEXT("recorded shell, bomb and smoke messages select their clips"), Shell && Bomb && Smoke);
 // A live mixer gets the same resolved model muzzle as the visual tracer, including terrain height.
 FTCAudioCues Arrival; Arrival.SetRandom([]() { return .5; });
 FTCAudioUnit Gun; Gun.Id = 1; Gun.Type = TEXT("tank"); Gun.X = 4; Gun.Z = 4; Gun.bKnown = true;
 TMap<int32, FTCAudioUnit> GunUnits; GunUnits.Add(1, Gun);
 FTCAudioShot Fire; Fire.K = TEXT("tank"); Fire.F = 1; Fire.X = 12; Fire.Z = 12; Fire.bHasPos = true;
 TArray<FTCAudioShot> FireShots = { Fire };
 FTCAudioSnapshot A; A.Tick = 2; A.Units = &GunUnits; A.Shots = FireShots;
 A.TypeOf = [](const FString&, FTCAudioType& Type) { Type.bKnown = true; return true; };
 A.MuzzleAt = [](int32, int32, FVector& Out) { Out = FVector(4, 4, 5); return true; };
 A.HeightAt = [](float, float) { return 2.f; };
 Trace.Reset(); Arrival.Snapshot(A, Mixer); Trace.Reset();
 const double Delay = FMath::Sqrt(8. * 8 + 8. * 8 + 2.9 * 2.9) / 150.;
 Arrival.Frame(Delay - .00001, Mixer);
 TestEqual(TEXT("impact sound waits for model-to-ground flight"), Trace.Num(), 0);
 Arrival.Frame(.00002, Mixer);
 TestTrue(TEXT("impact sounds at model-derived arrival"), Trace.Num() > 0 && Trace[0].Key == TEXT("blast_small"));
 return true;
}
#endif
