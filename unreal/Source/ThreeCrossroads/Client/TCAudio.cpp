#include "Client/TCAudio.h"
#include "Client/TCEffects.h"
#include "Client/TCLook30Dynamics.h"
#include "AudioDeviceManager.h"
#include "Engine/Engine.h"

#include "ThreeCrossroads.h"
#include "Client/TCObjectives.h"
#include "Client/TCUnitTable.h"
#include "Client/TCPersistence.h"
#include "Net/TCMatchState.h"
#include "Components/AudioComponent.h"
#include "Engine/World.h"
#include "GameFramework/PlayerController.h"
#include "Kismet/GameplayStatics.h"
#include "Misc/ConfigCacheIni.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Misc/AutomationTest.h"
#include "Sound/SoundAttenuation.h"
#include "Sound/SoundWave.h"
#include "Sound/SoundWaveProcedural.h"
#include "AudioDevice.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include <limits>

// ---------------------------------------------------------------------------------------------------------------------
// Tables (client/audio.js and client/audio/index.json at the pin)

const TCHAR* const TCAudioRules::ReferencePin = TEXT("e4e579a");

namespace TCAudioDetail
{
	struct FNamedMix
	{
		const TCHAR* Name;
		TCAudioRules::FMix Mix;
	};
	// audio.js MIX: gain, range, max, gap, pitch (vol is never set there: 0.15 for every effect)
	static const FNamedMix MixTable[] = {
		{ TEXT("rifle"), { 0.42, 120.0, 6, 0.04, 0.08 } },
		{ TEXT("smg"), { 0.42, 120.0, 4, 0.06, 0.06 } },
		{ TEXT("mg"), { 0.42, 130.0, 3, 0.15, 0.05 } },
		{ TEXT("sniper"), { 0.65, 180.0, 2, 0.0, 0.04 } },
		{ TEXT("atgun"), { 0.75, 180.0, 3, 0.0, 0.06 } },
		{ TEXT("tankgun"), { 0.8, 200.0, 4, 0.0, 0.07 } },
		{ TEXT("mortar"), { 0.6, 160.0, 3, 0.0, 0.08 } },
		{ TEXT("whistle"), { 0.4, 140.0, 3, 0.0, 0.06 } },
		{ TEXT("blast_small"), { 0.6, 170.0, 5, 0.05, 0.12 } },
		{ TEXT("blast_large"), { 0.85, 220.0, 4, 0.08, 0.1 } },
		{ TEXT("bomb"), { 1.0, 260.0, 4, 0.1, 0.08 } },
		{ TEXT("rockets"), { 0.8, 200.0, 2, 0.0, 0.05 } },
		{ TEXT("plane_flyby"), { 0.75, 300.0, 2, 0.0, 0.05 } },
		{ TEXT("strafe"), { 0.75, 220.0, 2, 0.0, 0.04 } },
		{ TEXT("bomber"), { 0.8, 320.0, 1, 0.0, 0.03 } },
		{ TEXT("flak"), { 0.55, 200.0, 2, 0.0, 0.08 } },
		{ TEXT("collapse"), { 0.8, 200.0, 2, 0.3, 0.08 } },
		{ TEXT("smoke"), { 0.45, 140.0, 3, 0.1, 0.1 } },
		{ TEXT("dig"), { 0.5, 110.0, 2, 0.5, 0.1 } },
		{ TEXT("build"), { 0.5, 110.0, 2, 0.5, 0.08 } },
		{ TEXT("vehicle_destroyed"), { 1.0, 220.0, 2, 0.2, 0.06 } },
		{ TEXT("ricochet"), { 0.3, 90.0, 2, 0.2, 0.15 } },
		{ TEXT("fire_loop"), { 0.45, 90.0, 3, 0.0, 0.0 } },
		{ TEXT("tank_engine"), { 0.5, 110.0, 0, 0.0, 0.0 } },
		{ TEXT("ambient"), { 0.3, 0.0, 0, 0.0, 0.0 } },
		{ TEXT("match_start"), { 0.8, 0.0, 0, 0.0, 0.0 } },
	};
	// index.json sfx, in its order
	static const TCAudioRules::FClip ClipTable[] = {
		{ TEXT("rifle"), 0.66, 1.0, false }, { TEXT("smg"), 1.2, 0.0, false }, { TEXT("mg"), 1.6, 3.0, false },
		{ TEXT("sniper"), 2.0, 0.0, false }, { TEXT("atgun"), 2.0, 0.0, false }, { TEXT("tankgun"), 2.48, 0.0, false },
		{ TEXT("mortar"), 0.51, 5.0, false }, { TEXT("whistle"), 1.76, 0.0, false }, { TEXT("blast_small"), 1.56, 10.0, false },
		{ TEXT("blast_large"), 3.0, 0.0, false }, { TEXT("bomb"), 3.48, 0.0, false }, { TEXT("rockets"), 3.0, 0.0, false },
		{ TEXT("plane_flyby"), 4.0, 0.0, false }, { TEXT("strafe"), 1.42, 0.0, false }, { TEXT("bomber"), 6.0, 0.0, false },
		{ TEXT("flak"), 2.48, 0.0, false }, { TEXT("collapse"), 2.071, 4.0, false }, { TEXT("tank_engine"), 3.7, 0.0, true },
		{ TEXT("smoke"), 1.89, 0.0, false }, { TEXT("dig"), 1.55, 0.0, false }, { TEXT("build"), 2.343, 0.0, false },
		{ TEXT("vehicle_destroyed"), 2.95, 0.0, false }, { TEXT("fire_loop"), 5.5, 4.0, true }, { TEXT("ambient"), 18.0, 0.0, true },
		{ TEXT("ricochet"), 0.386, 8.0, false }, { TEXT("ui_click"), 0.25, 0.0, false }, { TEXT("ui_recruit"), 0.24, 0.0, false },
		{ TEXT("ui_error"), 0.32, 3.0, false }, { TEXT("alert_attack"), 1.48, 0.0, false }, { TEXT("alert_point_won"), 1.2, 0.0, false },
		{ TEXT("alert_point_lost"), 1.2, 0.0, false }, { TEXT("alert_unit_lost"), 0.99, 2.0, false }, { TEXT("alert_air"), 2.0, 0.0, false },
		{ TEXT("alert_ready"), 0.95, 0.0, false }, { TEXT("match_start"), 1.41, 0.0, false },
	};
	static const TCAudioRules::FAlert AlertTable[] = {
		{ TEXT("attack"), TEXT("alert_attack"), 6.0, TEXT("fire") },
		{ TEXT("base"), TEXT("alert_attack"), 6.0, TEXT("fire") },
		{ TEXT("pointWon"), TEXT("alert_point_won"), 1.0, nullptr },
		{ TEXT("pointLost"), TEXT("alert_point_lost"), 1.0, nullptr },
		{ TEXT("unitLost"), TEXT("alert_unit_lost"), 4.0, TEXT("lost") },
		{ TEXT("air"), TEXT("alert_air"), 4.0, nullptr },
		{ TEXT("ready"), TEXT("alert_ready"), 1.5, nullptr },
		{ TEXT("event"), TEXT("alert_air"), 1.0, nullptr }, // a map's scripted event (trigger)
	};
	static const TCAudioRules::FUi UiTable[] = {
		{ TEXT("click"), 0.45, 0.05 }, { TEXT("recruit"), 0.5, 0.1 }, { TEXT("error"), 0.5, 0.3 },
	};
	// FACS: voice folders by faction index; index.json has voices for the first three
	static const TCHAR* const Facs[] = { TEXT("us"), TEXT("de"), TEXT("ru"), TEXT("uk") };
	struct FFolder
	{
		const TCHAR* Folder;
		const TCHAR* Speakers[2];
	};
	static const FFolder Folders[] = {
		{ TEXT("de"), { TEXT("callum"), TEXT("daniel") } },
		{ TEXT("ru"), { TEXT("brian"), TEXT("charlie") } },
		{ TEXT("us"), { TEXT("adam"), TEXT("harry") } },
	};
	struct FKindLines
	{
		const TCHAR* Kind;
		int32 Lines;
	};
	static const FKindLines VoiceKinds[] = { { TEXT("attack"), 3 }, { TEXT("fire"), 2 }, { TEXT("lost"), 1 }, { TEXT("move"), 4 }, { TEXT("retreat"), 2 } };

	static const FFolder* FindFolder(const FString& Folder)
	{
		for (const FFolder& F : Folders) if (Folder == F.Folder) return &F;
		return nullptr;
	}
	static double Clamp01(double V, double Lo = 0.0, double Hi = 1.0) { return FMath::Max(Lo, FMath::Min(Hi, V)); }
}

const TCAudioRules::FMix* TCAudioRules::FindMix(const FString& Name)
{
	for (const TCAudioDetail::FNamedMix& M : TCAudioDetail::MixTable) if (Name == M.Name) return &M.Mix;
	return nullptr;
}
TConstArrayView<TCAudioRules::FClip> TCAudioRules::Clips() { return TCAudioDetail::ClipTable; }
const TCAudioRules::FClip* TCAudioRules::FindClip(const FString& Id)
{
	for (const FClip& C : TCAudioDetail::ClipTable) if (Id == C.Id) return &C;
	return nullptr;
}
const TCAudioRules::FAlert* TCAudioRules::FindAlert(const FString& Kind)
{
	for (const FAlert& A : TCAudioDetail::AlertTable) if (Kind.Equals(A.Kind, ESearchCase::CaseSensitive)) return &A;
	return nullptr;
}
const TCAudioRules::FUi* TCAudioRules::FindUi(const FString& Kind)
{
	for (const FUi& U : TCAudioDetail::UiTable) if (Kind.Equals(U.Kind, ESearchCase::CaseSensitive)) return &U;
	return nullptr;
}
const TCHAR* TCAudioRules::VoiceFolder(int32 Faction)
{
	// voiceFac: index.voice[FACS[f]] ? FACS[f] : 'us'
	if (Faction >= 0 && Faction < UE_ARRAY_COUNT(TCAudioDetail::Facs) && TCAudioDetail::FindFolder(TCAudioDetail::Facs[Faction])) return TCAudioDetail::Facs[Faction];
	return TEXT("us");
}
TArray<FString> TCAudioRules::Speakers(const FString& Folder)
{
	TArray<FString> Out;
	if (const TCAudioDetail::FFolder* F = TCAudioDetail::FindFolder(Folder)) for (const TCHAR* S : F->Speakers) Out.Add(S);
	Out.Sort();
	return Out;
}
int32 TCAudioRules::LineCount(const FString& Kind)
{
	for (const TCAudioDetail::FKindLines& K : TCAudioDetail::VoiceKinds) if (Kind.Equals(K.Kind, ESearchCase::CaseSensitive)) return K.Lines;
	return 0;
}
FString TCAudioRules::VoiceStem(const FString& Speaker, const FString& Kind, int32 Line, int32 Take)
{
	return FString::Printf(TEXT("%s_%s%d_t%d"), *Speaker, *Kind, Line + 1, Take + 1);
}
TArray<FString> TCAudioRules::VoiceStems(const FString& Speaker)
{
	TArray<FString> Out;
	for (const TCAudioDetail::FKindLines& K : TCAudioDetail::VoiceKinds)
		for (int32 L = 0; L < K.Lines; L++)
			for (int32 T = 0; T < Takes; T++) Out.Add(VoiceStem(Speaker, K.Kind, L, T));
	return Out;
}
FString TCAudioRules::SfxAssetPath(const FString& Id) { return FString::Printf(TEXT("/Game/Audio/SFX/%s.%s"), *Id, *Id); }
FString TCAudioRules::VoiceAssetPath(const FString& Folder, const FString& Stem) { return FString::Printf(TEXT("/Game/Audio/Voice/%s/%s.%s"), *Folder, *Stem, *Stem); }

double TCAudioRules::BrowserYaw(double UnrealYawDegrees) { return FMath::DegreesToRadians(-90.0 - UnrealYawDegrees); }

TCAudioRules::FPlace TCAudioRules::Place(const FEar& Ear, const FVector2D* Pos, double Range)
{
	if (!Pos) return FPlace();
	const double Dx = Pos->X - Ear.X, Dz = Pos->Y - Ear.Z, D = FMath::Sqrt(Dx * Dx + Dz * Dz);
	const double Zoom = 1.0 - 0.35 * FMath::Min(1.0, FMath::Max(0.0, (Ear.Dist - 25.0) / 125.0));
	FPlace P;
	P.Gain = FMath::Pow(FMath::Max(0.0, 1.0 - D / Range), 1.6) * Zoom;
	const double Side = Dx * FMath::Cos(Ear.Yaw) - Dz * FMath::Sin(Ear.Yaw); // along the camera's right vector
	P.Pan = FMath::Max(-1.0, FMath::Min(1.0, Side / (D + 25.0))) * 0.8;
	return P;
}

TCAudioRules::FEnvironment TCAudioRules::EnvironmentMix(int32 W, int32 H, TFunctionRef<ECell(int32, int32)> Classify, const FEar* Camera, const FString& Weather,
	const double* Wx, int32 WxNum)
{
	using TCAudioDetail::Clamp01;
	FEnvironment R;
	R.Wind = { 0.07, 0.0 };
	const double Rain = Weather == TEXT("snow") ? 0.0 : Clamp01(WxNum > 0 ? Wx[0] : (Weather == TEXT("rain") ? 1.0 : 0.0));
	R.Rain.Gain = Rain * 0.2;
	const double W2 = WxNum > 2 ? Wx[2] : 0.0, W3 = WxNum > 3 ? Wx[3] : 0.0;
	R.Wind.Gain = 0.055 + FMath::Min(0.04, FMath::Sqrt(W2 * W2 + W3 * W3) * 0.004);
	if (W <= 0 || H <= 0 || !Camera) return R;
	const double Range = 56.0, Cell = CellMetres;
	const int32 Radius = FMath::CeilToInt(Range / Cell), Cx = FMath::FloorToInt(Camera->X / Cell), Cz = FMath::FloorToInt(Camera->Z / Cell);
	struct FSrc { double Weight, Pan; };
	TArray<FSrc> Sources[2];
	for (int32 Z = FMath::Max(0, Cz - Radius); Z <= FMath::Min(H - 1, Cz + Radius); Z++)
	{
		for (int32 X = FMath::Max(0, Cx - Radius); X <= FMath::Min(W - 1, Cx + Radius); X++)
		{
			const ECell Kind = Classify(X, Z);
			if (Kind == ECell::None) continue;
			const double Px = (X + 0.5) * Cell, Pz = (Z + 0.5) * Cell, Dx = Px - Camera->X, Dz = Pz - Camera->Z, D = FMath::Sqrt(Dx * Dx + Dz * Dz);
			if (D >= Range) continue;
			const double Weight = FMath::Square(1.0 - D / Range);
			const double Side = Dx * FMath::Cos(Camera->Yaw) - Dz * FMath::Sin(Camera->Yaw);
			TArray<FSrc>& List = Sources[Kind == ECell::Water ? 0 : 1];
			List.Add({ Weight, Clamp01(Side / (D + 20.0), -1.0, 1.0) * 0.8 });
			List.StableSort([](const FSrc& A, const FSrc& B) { return A.Weight > B.Weight; });
			if (List.Num() > 8) List.Pop();
		}
	}
	const double Zoom = 1.0 - 0.35 * Clamp01((Camera->Dist - 25.0) / 125.0);
	for (int32 K = 0; K < 2; K++)
	{
		double Sum = 0.0, PanSum = 0.0;
		for (const FSrc& S : Sources[K]) { Sum += S.Weight; PanSum += S.Pan * S.Weight; }
		FBedTarget& T = K == 0 ? R.Water : R.Woodland;
		T.Gain = FMath::Min(1.0, Sum / 3.0) * Zoom * (K == 0 ? 0.14 : 0.095);
		T.Pan = Sum ? PanSum / Sum : 0.0;
	}
	return R;
}

// ---------------------------------------------------------------------------------------------------------------------
// FTCAudioMixer (client/audio.js)

void FTCAudioParam::Step(double Dt)
{
	if (Tau <= 0.0 || Dt <= 0.0) { if (Tau <= 0.0) Value = Target; return; }
	Value += (Target - Value) * (1.0 - FMath::Exp(-Dt / Tau));
}

double FTCAudioSound::Envelope(double Now) const
{
	double E = 1.0;
	if (LoopFor > 0.0)
	{
		// a loop that plays for a while: 0 -> 1 over 0.6 s, held, then down to 0 over its last 1.5 s
		const double T = Now - StartAt, End = FMath::Max(0.7, LoopFor), Hold = FMath::Max(0.6, End - 1.5);
		if (T <= 0.0) E = 0.0;
		else if (T < 0.6) E = T / 0.6;
		else if (T < Hold) E = 1.0;
		else E = T >= End ? 0.0 : 1.0 - (T - Hold) / (End - Hold);
	}
	if (bDone && FadeLen > 0.0) E *= FMath::Max(0.0, 1.0 - (Now - FadeFrom) / FadeLen);
	return E;
}

double FTCAudioBed::StopFactor(double Now) const
{
	return bDone && FadeLen > 0.0 ? FMath::Max(0.0, 1.0 - (Now - FadeFrom) / FadeLen) : 1.0;
}

FTCAudioMixer::FTCAudioMixer(int32 Seed) : Random(Seed)
{
	// ensureContext: the master at the volume's curve, the ambience bus at 1
	Master.Snap(TCAudioRules::Curve(VolumeLevel));
	AmbGain.Snap(1.0);
}

double FTCAudioMixer::BusGain(ETCAudioBus Bus) const
{
	switch (Bus)
	{
	case ETCAudioBus::Amb: return AmbGain.Value;
	case ETCAudioBus::Voice: return TCAudioRules::VoiceBusGain;
	default: return 1.0;
	}
}

int32 FTCAudioMixer::LivePlaying() const
{
	int32 N = 0;
	for (const FTCAudioSound& P : Playing) N += P.bDone ? 0 : 1;
	return N;
}

double FTCAudioMixer::Length(const FString& Name) const
{
	const double* D = Loaded.Find(Name);
	return D ? *D : 0.0;
}

void FTCAudioMixer::Advance(double Dt) { AdvanceTo(Clock + FMath::Max(0.0, Dt)); }

void FTCAudioMixer::AdvanceTo(double T)
{
	const double Dt = FMath::Max(0.0, T - Clock);
	Clock = FMath::Max(Clock, T);
	// the ambience bus: down while a voice, an alert or a loud effect plays, back over 0.4 s after
	if (Clock < AmbientDuckUntil) AmbGain.To(TCAudioRules::DuckLevel, 0.04);
	else AmbGain.To(1.0, 0.4);
	Master.Step(Dt);
	AmbGain.Step(Dt);
	for (int32 I = Playing.Num() - 1; I >= 0; I--)
	{
		FTCAudioSound& P = Playing[I];
		if (Clock >= P.StartAt)
		{
			P.Gain.Step(Dt);
			P.Pan.Step(Dt);
		}
		// onended: the file ran out (or the loop's stop time came), or the stop fade is over
		const bool bEnded = Clock >= P.EndAt || (P.bDone && P.FadeLen > 0.0 && Clock >= P.FadeFrom + P.FadeLen + 0.01);
		if (bEnded) Playing.RemoveAt(I);
	}
	for (int32 I = BedList.Num() - 1; I >= 0; I--)
	{
		FTCAudioBed& B = BedList[I];
		B.Gain.Step(Dt);
		B.Pan.Step(Dt);
		B.Rate.Step(Dt);
		if (B.bDone && Clock >= B.FadeFrom + B.FadeLen + 0.01) BedList.RemoveAt(I);
	}
	for (int32 I = 0; I < PendingVoices.Num();)
	{
		if (PendingVoices[I].At > Clock) { I++; continue; }
		const FString Kind = PendingVoices[I].Kind;
		PendingVoices.RemoveAt(I);
		Voice(Kind);
	}
}

void FTCAudioMixer::DuckAmbience(double Seconds)
{
	AmbientDuckUntil = FMath::Max(AmbientDuckUntil, Clock + Seconds);
	AmbGain.To(TCAudioRules::DuckLevel, 0.04);
}

void FTCAudioMixer::Stop(FTCAudioSound& P, double Fade)
{
	if (P.bDone) return;
	P.bDone = true;
	P.FadeFrom = Clock;
	P.FadeLen = Fade;
	if (Trace) Trace->Add({ false, P.Key, Clock + Fade + 0.01 });
}

int32 FTCAudioMixer::Launch(const FString& Key, ETCAudioBus Bus, const FVector2D* Pos, const TCAudioRules::FMix& Mix, double Gain, double Rate, double Delay,
	double Offset, double Dur, const FString& Name)
{
	const double* Duration = Loaded.Find(Key);
	// a suspended context (no device) would save every sound up and play them all at once later
	if (!Duration || !bStarted || !bRunning) return 0;
	const double T0 = Clock + Delay;
	if (Mix.Gap > 0.0)
	{
		const double* Last = LastStart.Find(Name);
		if (FMath::Abs(T0 - (Last ? *Last : -9.0)) < Mix.Gap) return 0;
	}
	// what the build's limiter could not reach comes back here, at most 6 dB
	const TCAudioRules::FClip* Clip = TCAudioRules::FindClip(Name);
	const double Boost = FMath::Min(6.0, Clip ? Clip->Boost : 0.0);
	const TCAudioRules::FPlace At = TCAudioRules::Place(EarAt, Pos, Mix.Range > 0.0 ? Mix.Range : 150.0);
	const double Level = Gain * Mix.Gain * At.Gain * FMath::Pow(10.0, Boost / 20.0);
	if (Level < 0.005) return 0;
	// too many of this sound: replace the quietest one if the new one is louder, else skip it
	TArray<int32> Live, Same;
	for (int32 I = 0; I < Playing.Num(); I++)
	{
		if (Playing[I].bDone) continue;
		Live.Add(I);
		if (Playing[I].Name == Name) Same.Add(I);
	}
	auto Quietest = [this](const TArray<int32>& List)
	{
		int32 Q = List[0];
		for (int32 I : List) if (Playing[I].Level < Playing[Q].Level) Q = I;
		return Q;
	};
	if (Mix.Max > 0 && Same.Num() >= Mix.Max)
	{
		const int32 Q = Quietest(Same);
		if (Playing[Q].Level >= Level) return 0;
		Stop(Playing[Q], 0.05);
	}
	if (Live.Num() >= TCAudioRules::MaxPlaying)
	{
		const int32 Q = Quietest(Live);
		if (Playing[Q].Level >= Level) return 0;
		Stop(Playing[Q], 0.02);
	}
	FTCAudioSound P;
	P.Handle = NextHandle++;
	P.Name = Name;
	P.Key = Key;
	P.Bus = Bus;
	P.bHasPos = Pos != nullptr;
	if (Pos) P.Pos = *Pos;
	P.Mix = Mix;
	P.Rate = Rate * Rnd(Mix.Pitch);
	const double V = Level * Rnd(Mix.Vol);
	P.Gain.Snap(V);
	P.Pan.Snap(At.Pan);
	P.Level = V;
	P.Base = V / FMath::Max(At.Gain, 1e-3);
	P.StartAt = T0;
	if (Dur > 0.0)
	{
		P.LoopFor = FMath::Max(0.7, Dur);
		P.EndAt = T0 + P.LoopFor + 0.05;
	}
	else
	{
		P.Offset = FMath::Min(Offset, *Duration - 0.05);
		P.EndAt = T0 + FMath::Max(0.0, *Duration - P.Offset) / FMath::Max(1e-3, P.Rate);
	}
	if (Bus == ETCAudioBus::Voice || Name.StartsWith(TEXT("alert_"))) DuckAmbience(FMath::Min(4.0, *Duration + 0.2));
	else if (Bus == ETCAudioBus::Sfx && Level > 0.2) DuckAmbience(0.45);
	LastStart.Add(Name, T0);
	if (Trace) Trace->Add({ true, Key, T0, Dur > 0.0 ? 0.0 : P.Offset, P.Rate, V, At.Pan, Dur > 0.0 });
	if (Trace && Dur > 0.0) Trace->Add({ false, Key, P.EndAt }); // src.stop(t0 + end + 0.05)
	Playing.Add(P);
	return P.Handle;
}

void FTCAudioMixer::Refresh()
{
	// sounds that last (fires, planes, the bomber) follow the camera as it moves
	for (FTCAudioSound& P : Playing)
	{
		if (P.bDone || !P.bHasPos) continue;
		const TCAudioRules::FPlace At = TCAudioRules::Place(EarAt, &P.Pos, P.Mix.Range > 0.0 ? P.Mix.Range : 150.0);
		P.Gain.To(P.Base * At.Gain, 0.08);
		P.Pan.To(At.Pan, 0.08);
	}
}

void FTCAudioMixer::Listener(double X, double Z, double Dist, double Yaw)
{
	EarAt.X = X;
	EarAt.Z = Z;
	EarAt.Dist = Dist;
	EarAt.Yaw = Yaw;
	if (Clock - EarRefreshedAt > TCAudioRules::RefreshEvery)
	{
		EarRefreshedAt = Clock;
		Refresh();
	}
}

int32 FTCAudioMixer::PlaySound(const FString& Name, const FVector2D* Pos, const FTCAudioPlayOptions& Opts)
{
	if (!bStarted) return 0;
	const TCAudioRules::FMix* M = TCAudioRules::FindMix(Name);
	const TCAudioRules::FMix Mix = Opts.Mix.IsSet() ? Opts.Mix.GetValue() : (M ? *M : TCAudioRules::FMix());
	const ETCAudioBus Bus = Opts.Bus.IsSet() ? Opts.Bus.GetValue() : (Pos ? ETCAudioBus::Sfx : ETCAudioBus::Ui);
	return Launch(Name, Bus, Pos, Mix, Opts.Gain, Opts.Rate, Opts.Delay, Opts.Offset, Opts.Dur, Opts.Name.IsEmpty() ? Name : Opts.Name);
}

FTCAudioBed* FTCAudioMixer::FindBed(const FString& Name)
{
	for (FTCAudioBed& B : BedList) if (!B.bDone && B.Name == Name) return &B;
	return nullptr;
}

void FTCAudioMixer::Bed(const FString& Name, TConstArrayView<FVector2D> Points)
{
	// a loop that swells with how many of its sources are near: tank engines
	if (!bStarted || !bRunning) return;
	const double* Duration = Loaded.Find(Name);
	if (!Duration) return;
	const TCAudioRules::FMix* M = TCAudioRules::FindMix(Name);
	const TCAudioRules::FMix Mix = M ? *M : TCAudioRules::FMix();
	double Sum = 0.0, Side = 0.0;
	for (const FVector2D& P : Points)
	{
		const TCAudioRules::FPlace At = TCAudioRules::Place(EarAt, &P, Mix.Range > 0.0 ? Mix.Range : 120.0);
		Sum += At.Gain;
		Side += At.Gain * At.Pan;
	}
	const double Level = FMath::Min(1.0, FMath::Sqrt(Sum)) * Mix.Gain;
	FTCAudioBed* B = FindBed(Name);
	if (!B && Level > 0.01)
	{
		FTCAudioBed N;
		N.Handle = NextHandle++;
		N.Name = Name;
		N.Bus = ETCAudioBus::Sfx;
		N.Gain.Snap(0.0);
		N.Pan.Snap(0.0);
		N.Rate.Snap(1.0);
		N.StartOffset = Rand() * *Duration;
		if (Trace) Trace->Add({ true, Name, Clock, N.StartOffset, 1.0, 0.0, 0.0, true });
		BedList.Add(N);
		B = &BedList.Last();
	}
	if (!B) return;
	B->Gain.To(Level, 0.25);
	B->Pan.To(Sum ? Side / Sum : 0.0, 0.25);
	B->Rate.To(0.92 + 0.12 * FMath::Min(1.0, Sum), 0.5);
}

void FTCAudioMixer::Environment(const TCAudioRules::FEnvironment& Targets)
{
	// targets come from known nearby terrain and public weather; four beds are the fixed upper bound
	EnvState = Targets;
	if (!bStarted || !bRunning) return;
	const TPair<const TCHAR*, const TCAudioRules::FBedTarget*> Kinds[] = {
		{ TEXT("amb:wind"), &Targets.Wind }, { TEXT("amb:water"), &Targets.Water }, { TEXT("amb:woodland"), &Targets.Woodland }, { TEXT("amb:rain"), &Targets.Rain } };
	for (const auto& K : Kinds)
	{
		FTCAudioBed* B = FindBed(K.Key);
		if (!B && K.Value->Gain > 0.001)
		{
			FTCAudioBed N;
			N.Handle = NextHandle++;
			N.Name = K.Key;
			N.Bus = ETCAudioBus::Amb;
			N.bSynth = true;
			N.Gain.Snap(0.0);
			N.Pan.Snap(0.0);
			N.Rate.Snap(1.0);
			if (Trace) Trace->Add({ true, N.Name, Clock, 0.0, 1.0, 0.0, 0.0, true });
			BedList.Add(N);
			B = &BedList.Last();
		}
		if (!B) continue;
		B->Gain.To(FMath::Max(0.0, FMath::Min(0.2, K.Value->Gain)), 1.3);
		B->Pan.To(FMath::Max(-0.8, FMath::Min(0.8, K.Value->Pan)), 0.8);
	}
}

int32 FTCAudioMixer::BeginMatch(int32 InFaction, int32 InSlot)
{
	Faction = InFaction;
	Slot = InSlot;
	End();
	bStarted = true;
	MatchStartedAt = Clock;
	return ++Match; // a restart before loading finishes must not start a second ambience
}

FString FTCAudioMixer::ChooseVoice(int32 Token, FString& OutFolder)
{
	OutFolder.Reset();
	if (Token != Match) return FString();
	VoiceFolderName = TCAudioRules::VoiceFolder(Faction);
	const TArray<FString> Names = TCAudioRules::Speakers(VoiceFolderName);
	VoiceSpeaker = Names.Num() ? Names[((Slot % Names.Num()) + Names.Num()) % Names.Num()] : FString();
	OutFolder = VoiceFolderName;
	return VoiceSpeaker;
}

void FTCAudioMixer::FinishLoading(int32 Token)
{
	if (Token != Match) return;
	Ambience();
	if (Clock - MatchStartedAt < TCAudioRules::MatchStartWithin)
	{
		FTCAudioPlayOptions O;
		O.Bus = ETCAudioBus::Ui;
		PlaySound(TEXT("match_start"), nullptr, O);
	}
}

void FTCAudioMixer::End()
{
	bStarted = false;
	Match++;
	for (FTCAudioSound& P : Playing) Stop(P, 0.4);
	for (FTCAudioBed& B : BedList)
	{
		if (B.bDone) continue;
		B.bDone = true;
		B.FadeFrom = Clock;
		B.FadeLen = 0.4;
		if (Trace) Trace->Add({ false, B.Name, Clock + 0.4 + 0.01 });
	}
	EnvState = TCAudioRules::FEnvironment();
	AmbientDuckUntil = 0.0;
	AmbGain.Snap(1.0);
}

void FTCAudioMixer::InitVolume(TOptional<double> Saved)
{
	VolumeLevel = Saved.IsSet() && FMath::IsFinite(Saved.GetValue()) ? FMath::Clamp(Saved.GetValue(), 0.0, 1.0) : TCAudioRules::DefaultVolume;
	Master.Snap(TCAudioRules::Curve(VolumeLevel));
	SetVolume(VolumeLevel); // bind(): remember the starting volume as the one M unmutes to
}

void FTCAudioMixer::InitPreferences(const FTCSettingsStore& Preferences)
{
	InitVolume(TOptional<double>(FTCSettingsStore::ReadVolume(Preferences.Get(TEXT("ww2-volume")), Preferences.Get(TEXT("ww2-muted")))));
}

void FTCAudioMixer::SetVolume(double V)
{
	VolumeLevel = FMath::IsFinite(V) ? FMath::Clamp(V, 0.0, 1.0) : 0.0;
	if (VolumeLevel > 0.0) MutedFrom = VolumeLevel;
	bVolumeChanged = true;
	Master.To(TCAudioRules::Curve(VolumeLevel), 0.03);
}

void FTCAudioMixer::ToggleMute() { SetVolume(VolumeLevel > 0.0 ? 0.0 : (MutedFrom > 0.0 ? MutedFrom : TCAudioRules::DefaultVolume)); }

void FTCAudioMixer::Voice(const FString& Kind)
{
	if (!bStarted || VoiceSpeaker.IsEmpty()) return;
	const double T = Clock;
	if (T < VoiceUntil || T - LastLineAt < TCAudioRules::VoiceGap) return;
	const int32 Lines = TCAudioRules::LineCount(Kind);
	if (Lines <= 0) return;
	// a different line than last time when there is a choice, and either take of it
	int32 N = FMath::Min(Lines - 1, (int32)FMath::FloorToDouble(Rand() * Lines));
	const int32* Last = LastLineIndex.Find(Kind);
	if (Lines > 1 && Last && N == *Last) N = (N + 1) % Lines;
	const int32 Take = FMath::Min(TCAudioRules::Takes - 1, (int32)FMath::FloorToDouble(Rand() * TCAudioRules::Takes));
	const FString Key = TEXT("voice:") + TCAudioRules::VoiceStem(VoiceSpeaker, Kind, N, Take);
	TCAudioRules::FMix Mix;
	Mix.Gain = 1.0;
	Mix.Vol = 0.05;
	const int32 H = Launch(Key, ETCAudioBus::Voice, nullptr, Mix, 1.0, 1.0, 0.0, 0.0, 0.0, TEXT("voice"));
	if (!H) return;
	LastLineIndex.Add(Kind, N);
	LastLineAt = T;
	VoiceUntil = T + Loaded.FindRef(Key);
}

void FTCAudioMixer::Alert(const FString& Kind)
{
	const TCAudioRules::FAlert* A = TCAudioRules::FindAlert(Kind);
	if (!A || !bStarted || !bRunning) return;
	const double T = Clock;
	const double* Last = LastAlert.Find(Kind);
	if (T - (Last ? *Last : -99.0) < A->Gap) return;
	LastAlert.Add(Kind, T);
	TCAudioRules::FMix Mix;
	Mix.Gain = 0.7;
	Launch(A->Sound, ETCAudioBus::Ui, nullptr, Mix, 1.0, 1.0, 0.0, 0.0, 0.0, A->Sound);
	if (A->Voice) PendingVoices.Add({ T + TCAudioRules::AlertVoiceDelay, A->Voice });
}

void FTCAudioMixer::Ui(const FString& Kind)
{
	const TCAudioRules::FUi* U = TCAudioRules::FindUi(Kind);
	if (!U) U = TCAudioRules::FindUi(TEXT("click"));
	TCAudioRules::FMix Mix;
	Mix.Gain = U->Gain;
	Mix.Gap = U->Gap;
	const FString Key = FString(TEXT("ui_")) + U->Kind;
	Launch(Key, ETCAudioBus::Ui, nullptr, Mix, 1.0, 1.0, 0.0, 0.0, 0.0, Key);
}

FString FTCAudioMixer::Stats() const
{
	TArray<FString> BedNames;
	for (const FTCAudioBed& B : BedList) if (!B.bDone) BedNames.Add(B.Name);
	return FString::Printf(TEXT("state %s, loaded %d, playing %d, beds [%s], voice %s/%s, volume %.2f, clock %.2f"), bRunning ? TEXT("running") : TEXT("suspended"),
		Loaded.Num(), LivePlaying(), *FString::Join(BedNames, TEXT(", ")), VoiceFolderName.IsEmpty() ? TEXT("-") : *VoiceFolderName,
		VoiceSpeaker.IsEmpty() ? TEXT("-") : *VoiceSpeaker, VolumeLevel, Clock);
}

// ---------------------------------------------------------------------------------------------------------------------
// FTCAudioCues (client/fx.js, client/main.js, client/projectiles.js, client/battle-sound.js)

// fx.js GUNS: the sound per shooter type, soldiers or guns firing per shot (n), rounds per burst and their gap, tracer
// speed (m/s, for the impact's delay) and heavy (an explosive shell lands: blast_small at the far end).
struct FTCAudioCues::FGun
{
	const TCHAR* Type;
	const TCHAR* Snd;
	int32 N, Burst;
	double Gap, Spd, Heavy;
	int32 Guns;
};

namespace TCAudioDetail
{
	using FGun = FTCAudioCues::FGun;
	static const FGun Small = { TEXT("small"), TEXT("rifle"), 3, 1, 0.0, 230.0, 0.0, 0 };
	static const FGun Bazooka = { TEXT("bazooka"), TEXT("atgun"), 1, 1, 0.0, 70.0, 2.2, 0 };
	static const FGun GunTable[] = {
		{ TEXT("rifle"), TEXT("rifle"), 3, 1, 0.0, 230.0, 0.0, 0 },
		{ TEXT("conscript"), TEXT("rifle"), 3, 1, 0.0, 230.0, 0.0, 0 },
		{ TEXT("engineer"), TEXT("rifle"), 2, 1, 0.0, 230.0, 0.0, 0 },
		{ TEXT("ranger"), TEXT("smg"), 3, 2, 0.07, 230.0, 0.0, 0 },
		{ TEXT("commando"), TEXT("smg"), 3, 3, 0.06, 230.0, 0.0, 0 },
		{ TEXT("mg"), TEXT("mg"), 1, 4, 0.065, 210.0, 0.0, 0 },
		{ TEXT("bunker"), TEXT("mg"), 1, 4, 0.08, 210.0, 0.0, 0 },
		{ TEXT("pillbox"), TEXT("mg"), 1, 4, 0.08, 210.0, 0.0, 0 },
		{ TEXT("sniper"), TEXT("sniper"), 1, 1, 0.0, 420.0, 0.0, 0 },
		{ TEXT("at"), TEXT("atgun"), 1, 1, 0.0, 150.0, 2.4, 0 },
		{ TEXT("tank"), TEXT("tankgun"), 1, 1, 0.0, 150.0, 2.6, 0 },
		{ TEXT("medium"), TEXT("tankgun"), 1, 1, 0.0, 150.0, 3.0, 0 },
		{ TEXT("tiger"), TEXT("tankgun"), 1, 1, 0.0, 160.0, 3.4, 0 },
		{ TEXT("churchill"), TEXT("tankgun"), 1, 1, 0.0, 150.0, 3.0, 0 },
		{ TEXT("tankdestroyer"), TEXT("tankgun"), 1, 1, 0.0, 165.0, 3.0, 0 },
		{ TEXT("flamer"), TEXT("flak"), 1, 3, 0.06, 40.0, 0.0, 0 },
		{ TEXT("kaiju"), TEXT("tankgun"), 1, 8, 0.05, 60.0, 4.0, 0 },
		{ TEXT("halftrack"), TEXT("mg"), 1, 3, 0.08, 210.0, 0.0, 0 },
		{ TEXT("gunboat"), TEXT("tankgun"), 1, 2, 0.12, 170.0, 1.5, 0 },
		{ TEXT("lcvp"), TEXT("mg"), 1, 3, 0.08, 210.0, 0.0, 0 },
		{ TEXT("armoredcar"), TEXT("tankgun"), 1, 2, 0.12, 170.0, 1.5, 0 },
		{ TEXT("flak"), TEXT("flak"), 1, 2, 0.1, 240.0, 0.0, 0 },
		{ TEXT("flaktrack"), TEXT("flak"), 1, 4, 0.06, 240.0, 0.0, 0 },
		{ TEXT("fighter"), TEXT("mg"), 2, 4, 0.05, 280.0, 0.0, 2 },
		{ TEXT("attacker"), TEXT("rockets"), 1, 1, 0.0, 110.0, 2.6, 0 },
	};
	static const FGun* FindGun(const FString& Kind)
	{
		for (const FGun& G : GunTable) if (Kind.Equals(G.Type, ESearchCase::CaseSensitive)) return &G;
		return &Small;
	}
	// shared/sim.js UNITS[type].w for the salvo weapons (flight and every; the destroyer's model has four gun mounts)
	struct FSalvoWeapon
	{
		const TCHAR* Type;
		double Flight, Every;
	};
	static const FSalvoWeapon SalvoTable[] = {
		{ TEXT("rocket"), 1.2, 0.15 }, { TEXT("mortar"), 1.5, 0.1 }, { TEXT("destroyer"), 1.8, 0.1 }, { TEXT("howitzer"), 2.2, 0.1 }, { TEXT("bomber"), 1.0, 0.25 },
	};
	constexpr int32 DestroyerMounts = 4;
	static const FSalvoWeapon* FindSalvo(const FString& Type)
	{
		for (const FSalvoWeapon& W : SalvoTable) if (Type.Equals(W.Type, ESearchCase::CaseSensitive)) return &W;
		return nullptr;
	}
	// fx.js BURST_SND: a recording that is a whole burst plays once per burst per shooter
	static const double* BurstGap(const TCHAR* Snd)
	{
		static const double Mg = 1.25, Smg = 0.7;
		if (FCString::Strcmp(Snd, TEXT("mg")) == 0) return &Mg;
		if (FCString::Strcmp(Snd, TEXT("smg")) == 0) return &Smg;
		return nullptr;
	}
	// main.js SUPPORT_PLANES
	static const TCHAR* SupportPlane(const FString& K)
	{
		if (K == TEXT("strafe") || K == TEXT("recon") || K == TEXT("dive")) return TEXT("plane_flyby");
		if (K == TEXT("bombing") || K == TEXT("para")) return TEXT("bomber");
		return nullptr;
	}
	// main.js airShot: drawn and heard by client/aircraft.js and main.js, not fx.js ('flak' with a target or a hit flag
	// is a flak gun firing at the ground, which stays with fx.js)
	static bool AirShot(const FTCAudioShot& Sh)
	{
		const FString& K = Sh.K;
		const bool bAir = SupportPlane(K) || K == TEXT("chutes") || K == TEXT("shotdown") || K == TEXT("planedown") || K == TEXT("flak") || K == TEXT("aa");
		return bAir && !(K == TEXT("flak") && (Sh.T.IsSet() || Sh.bHitPresent));
	}
	// battle-sound.js isVeh: a UNITS type that is not infantry, a structure or an aircraft
	static bool BattleVehicle(const FTCAudioUnit& U) { return U.bKnown && !U.bInfantry && !U.bStructure && !U.bAir; }
	// a hidden shooter has no id: fx.js keys its burst gap by undefined, one key for all of them
	constexpr int64 HiddenShooter = TNumericLimits<int64>::Min();
	// main.js topple: a = min(PI / 2, 1.6 t^2) reaches PI / 2 after sqrt(PI / 3.2) s
	static double ToppleSeconds() { return FMath::Sqrt(UE_DOUBLE_PI / 3.2); }
	// a JS number as String(n) writes it, for the strike keys
	static FString JsNum(double V)
	{
		if (V == FMath::FloorToDouble(V) && FMath::Abs(V) < 1e15) return FString::Printf(TEXT("%lld"), (long long)V);
		FString S = FString::Printf(TEXT("%.17g"), V);
		for (int32 P = 15; P <= 17; P++)
		{
			const FString T = FString::Printf(TEXT("%.*g"), P, V);
			if (FCString::Atod(*T) == V) { S = T; break; }
		}
		return S;
	}
}

FTCAudioCues::FTCAudioCues(int32 Seed) : Random(Seed) {}

void FTCAudioCues::Reset()
{
	FxClock = 0.0;
	LastPlay.Reset();
	LastBurst.Reset();
	Events.Reset();
	StrikeSounds.Reset();
	Clouds.Reset();
	FlightEvents.Reset();
	FlightOrder.Reset();
	ProjTick = -1;
	Held.Reset();
	Killed.Reset();
	BattleTick = -1e9;
	BattleSeen.Reset();
	Foley.Reset();
	for (FThrow& Th : Throws) Th = FThrow();
	ThrowNext = 0;
}

void FTCAudioCues::Fx(const FString& Name, double X, double Z, double Gap, ITCAudioSink& Sink, const FTCAudioPlayOptions* Opts)
{
	// fx.js play(): an optional per-name gap on the effects clock, then audio.play at (x, z)
	if (Gap > 0.0)
	{
		const double* Last = LastPlay.Find(Name);
		if (FxClock - (Last ? *Last : -1e9) < Gap) return;
		LastPlay.Add(Name, FxClock);
	}
	const FVector2D P(X, Z);
	Sink.Play(Name, &P, Opts ? *Opts : FTCAudioPlayOptions());
}

void FTCAudioCues::Later(double Delay, const FString& Name, double X, double Z, double Offset, double Gain)
{
	if (Events.Num() >= TCAudioRules::MaxDelayed) return;
	FEvent E;
	E.Due = FxClock + Delay;
	E.Name = Name;
	E.Pos = FVector2D(X, Z);
	E.Offset = Offset;
	E.Gain = Gain;
	Events.Add(E);
}

void FTCAudioCues::LaterImpact(double Delay, double X, double Z, const FGun* Style, int32 Flags)
{
	if (Events.Num() >= TCAudioRules::MaxDelayed) return;
	FEvent E;
	E.Due = FxClock + Delay;
	E.bImpact = true;
	E.Pos = FVector2D(X, Z);
	E.Style = Style;
	E.Flags = Flags;
	Events.Add(E);
}

void FTCAudioCues::Whistle(double Delay, double Flight, double X, double Z, double Gain, const ITCAudioSink& Sink)
{
	// a falling round's whistle starts early or part way in, so that it ends as the round lands
	const double L = Sink.Length(TEXT("whistle"));
	const double Len = L > 0.0 ? L : 1.76;
	Later(Delay + FMath::Max(0.0, Flight - Len), TEXT("whistle"), X, Z, FMath::Max(0.0, Len - Flight), Gain);
}

void FTCAudioCues::Frame(double Dt, ITCAudioSink& Sink)
{
	FxClock += FMath::Max(0.0, Dt);
	TArray<FEvent> Due;
	for (int32 I = 0; I < Events.Num();)
	{
		if (Events[I].Due > FxClock) { I++; continue; }
		Due.Add(Events[I]);
		Events.RemoveAt(I);
	}
	for (const FEvent& E : Due)
	{
		if (E.bImpact) { Impact(E.Pos.X, E.Pos.Y, E.Style, E.Flags, Sink); continue; }
		// E_SOUND: options only when an offset or a gain was given (a gain of 0 means 1)
		if (E.Offset != 0.0 || E.Gain != 0.0)
		{
			FTCAudioPlayOptions O;
			O.Offset = E.Offset;
			O.Gain = E.Gain != 0.0 ? E.Gain : 1.0;
			Fx(E.Name, E.Pos.X, E.Pos.Y, 0.0, Sink, &O);
		}
		else Fx(E.Name, E.Pos.X, E.Pos.Y, 0.0, Sink);
	}
}

void FTCAudioCues::Impact(double X, double Z, const FGun* Style, int32 Flags, ITCAudioSink& Sink)
{
	// fx.js impact(): an explosive shell bursts (on armor or the ground alike); a bullet off armor ricochets; dirt and
	// masonry hits are silent
	const bool bHit = Flags & 1, bVeh = Flags & 2, bWall = Flags & 4;
	if (Style->Heavy > 0.0) { Fx(TEXT("blast_small"), X, Z, 0.0, Sink); return; }
	if (bVeh && bHit && !bWall) Fx(TEXT("ricochet"), X, Z, 0.3, Sink);
}

void FTCAudioCues::DirectFire(const FTCAudioShot& Sh, const FTCAudioUnit* From, const FTCAudioUnit* To, const FTCAudioSnapshot& S, ITCAudioSink& Sink)
{
	using namespace TCAudioDetail;
	FTCAudioType TDef;
	const bool bTDef = To && S.TypeOf && S.TypeOf(To->Type, TDef) && TDef.bKnown;
	const bool bVeh = bTDef && !TDef.bInfantry;
	const bool bBazooka = Sh.K == TEXT("ranger") && bVeh;
	const FGun* St = bBazooka ? &TCAudioDetail::Bazooka : FindGun(Sh.K);
	const int32 Flags = (Sh.bHit ? 1 : 0) | (bVeh ? 2 : 0) | (bTDef && TDef.bStructure ? 4 : 0);
	const double Ty = bVeh ? (TDef.bStructure ? 1.6 : 1.2) : 0.8;
	// the gun at the shooter if we can see it, else where the rounds land. An MG's recording is a whole burst, so one
	// plays per burst; rifles crack once per soldier, with his flash (below)
	const double Sx = From ? From->X : Sh.X, Sz = From ? From->Z : Sh.Z;
	if (const double* Gap = BurstGap(St->Snd))
	{
		const int64 Key = Sh.F.IsSet() ? (int64)Sh.F.GetValue() : HiddenShooter;
		const double* Last = LastBurst.Find(Key);
		if (FxClock - (Last ? *Last : -1e9) >= *Gap)
		{
			LastBurst.Add(Key, FxClock);
			Fx(St->Snd, Sx, Sz, 0.0, Sink);
		}
	}
	else if (FCString::Strcmp(St->Snd, TEXT("rifle")) != 0 || !From)
	{
		FTCAudioPlayOptions Tiger;
		Tiger.Rate = 0.85;
		Fx(St->Snd, Sx, Sz, 0.0, Sink, Sh.K == TEXT("tiger") ? &Tiger : nullptr);
	}
	if (LastBurst.Num() > 400) LastBurst.Reset();
	if (!From)
	{
		// shooter hidden: only the arrival
		const double Ex = Sh.X + Rr(-1, 1) * (Sh.bHit ? 0.6 : 2.5), Ez = Sh.Z + Rr(-1, 1) * (Sh.bHit ? 0.6 : 2.5);
		Impact(Ex, Ez, St, Flags, Sink);
		return;
	}
	const double Spread = Sh.bHit ? (bVeh ? 0.6 : 1.4) : 3.0;
	const int32 Men = St->N > 1 ? FMath::Min(St->N, St->Guns > 0 ? St->Guns : From->Alive) : 1;
	// muzzleAt: guns, bunkers and the MG team always have a muzzle; a squad's k-th weapon is its k-th visible man (a
	// garrisoned squad shows none), the bazooka every third from the second, a ranger's rifles the others
	auto Muzzle = [&](int32 K)
	{
		if (!From->bInfantry || From->Type == TEXT("mg")) return true;
		if (From->Flags & TCFlags::Garrisoned) return false;
		int32 Count = 0;
		for (int32 M = 0; M < From->Alive; M++)
		{
			const bool bPass = bBazooka ? M % 3 == 1 : (From->Type == TEXT("ranger") ? M % 3 != 1 : true);
			if (bPass && Count++ == K) return true;
		}
		return false;
	};
	for (int32 I = 0; I < Men; I++)
	{
		if (!Muzzle(I)) break;
		FVector M(From->X, From->Z, 1);
		if (S.MuzzleAt && !S.MuzzleAt(From->Id, I, M)) break;
		const int32 Shots = bBazooka ? 1 : St->Burst;
		const double T0 = St->N > 1 ? Rand() * 0.18 : 0.0;
		if (FCString::Strcmp(St->Snd, TEXT("rifle")) == 0) Later(T0, TEXT("rifle"), Sx, Sz, 0.0, I ? 0.8 : 0.0);
		for (int32 B = 0; B < Shots; B++)
		{
			double Ex = Sh.X + Rr(-0.5, 0.5) * Spread, Ez = Sh.Z + Rr(-0.5, 0.5) * Spread;
			if (Sh.bHit && bVeh)
			{
				const double Dx = Ex - M.X, Dz = Ez - M.Y, Dd = FMath::Max(1e-9, FMath::Sqrt(Dx * Dx + Dz * Dz));
				const double R = FMath::Min(Dd * 0.5, (double)TDef.Radius * 0.9);
				Ex -= Dx / Dd * R;
				Ez -= Dz / Dd * R;
			}
			const double Ground = S.HeightAt ? S.HeightAt(Ex, Ez) : 0;
			const double Ey = Ground + (Sh.bHit ? Ty : 0.1), Dx = Ex - M.X, Dz = Ez - M.Y;
			const double T = FMath::Sqrt(Dx * Dx + Dz * Dz + FMath::Square(Ey - M.Z)) / St->Spd;
			LaterImpact(T0 + B * St->Gap + T, Ex, Ez, St, Flags & (B == 0 ? 7 : 6));
		}
	}
}

void FTCAudioCues::Salvo(const FTCAudioShot& Sh, const FTCAudioUnit* From, ITCAudioSink& Sink)
{
	using namespace TCAudioDetail;
	// a destroyer's guns arc over like the mortar's; an unseen launcher's count tells mortar bombs (4 or fewer)
	const bool bShip = From && From->Type == TEXT("destroyer");
	const bool bMortar = From ? (From->Type == TEXT("mortar") || From->Type == TEXT("howitzer") || bShip) : (Sh.N.IsSet() && Sh.N.GetValue() <= 4);
	const int32 N = Sh.N.IsSet() ? Sh.N.GetValue() : 8;
	const FSalvoWeapon* W = From && FindSalvo(From->Type) ? FindSalvo(From->Type) : FindSalvo(bMortar ? TEXT("mortar") : TEXT("rocket"));
	const double Flight = W->Flight, Every = W->Every;
	if (From && From->bAir) From = nullptr; // a bomber's stick falls from the sky like an unseen launcher's rockets
	if (bMortar)
	{
		for (int32 I = 0; I < N; I++)
		{
			const double D = bShip ? FMath::FloorToDouble((double)I / DestroyerMounts) * 1.2 : I * Every; // a ship's guns fire together
			if (From) Later(D, bShip ? TEXT("tankgun") : TEXT("mortar"), From->X, From->Z);
			Whistle(D, Flight, Sh.X, Sh.Z, 0.7, Sink);
		}
		return;
	}
	if (From) Fx(TEXT("rockets"), From->X, From->Z, 0.0, Sink);
	else Whistle(0.0, Flight, Sh.X, Sh.Z, 0.0, Sink); // rockets drop in from the sky, from no telltale direction
}

void FTCAudioCues::AntiAir(const FTCAudioShot& Sh, const FTCAudioUnit* From, const FTCAudioUnit* To, ITCAudioSink& Sink)
{
	// fx.js antiAir: flak guns sound as flak; an MG, a fighter or a shooter out of sight as an MG
	const double Ex = To ? To->X : Sh.X, Ez = To ? To->Z : Sh.Z;
	const FString T = From ? From->Type : FString();
	const bool bFlak = T == TEXT("flak") || T == TEXT("flaktrack") || T == TEXT("flakpos");
	Fx(bFlak ? TEXT("flak") : TEXT("mg"), From ? From->X : Ex, From ? From->Z : Ez, 0.1, Sink);
}

void FTCAudioCues::ProjectileLaunch(const FTCAudioShot& Sh, ITCAudioSink& Sink)
{
	using namespace TCAudioDetail;
	// a filtered launch has no source id when its muzzle is outside current vision
	if (!Sh.F.IsSet() || Sh.Effect == TEXT("boom")) return;
	const TCHAR* Snd = FindSalvo(Sh.Kind) ? (Sh.Kind == TEXT("destroyer") ? TEXT("tankgun") : Sh.Kind == TEXT("rocket") ? TEXT("rockets") : TEXT("mortar")) : FindGun(Sh.Kind)->Snd;
	const double* Gap = BurstGap(Snd);
	const int64 Key = Sh.F.GetValue();
	const double* Last = LastBurst.Find(Key);
	if (!Gap || FxClock - (Last ? *Last : -1e9) >= *Gap)
	{
		Fx(Snd, Sh.X, Sh.Z, 0.0, Sink);
		LastBurst.Add(Key, FxClock);
	}
	if (LastBurst.Num() > 400) LastBurst.Reset();
}

void FTCAudioCues::ProjectileContact(const FTCAudioShot& Sh, ITCAudioSink& Sink)
{
	if (Sh.Effect == TEXT("boom") || Sh.Effect == TEXT("rocket")) { Fx(TEXT("blast_small"), Sh.X, Sh.Z, 0.0, Sink); return; }
	// a terminal contact is an impact with only the hit flag (no armor or wall bits): a shell bursts, a bullet is silent
	if (Sh.bTerminal && TCAudioDetail::FindGun(Sh.Kind)->Heavy > 0.0) Fx(TEXT("blast_small"), Sh.X, Sh.Z, 0.0, Sink);
}

bool FTCAudioCues::Fresh(int32 Id, int32 Sequence)
{
	// projectiles.js fresh(): each flight event once, newest sequence only; at most 4096 flights remembered
	const int32* Old = FlightEvents.Find(Id);
	if (Sequence <= (Old ? *Old : -1)) return false;
	if (!Old) FlightOrder.Add(Id);
	FlightEvents.Add(Id, Sequence);
	if (FlightEvents.Num() > 4096 && FlightOrder.Num())
	{
		FlightEvents.Remove(FlightOrder[0]);
		FlightOrder.RemoveAt(0);
	}
	return true;
}

void FTCAudioCues::Snapshot(const FTCAudioSnapshot& S, ITCAudioSink& Sink)
{
	using namespace TCAudioDetail;
	static const TMap<int32, FTCAudioUnit> NoUnits;
	const TMap<int32, FTCAudioUnit>& Units = S.Units ? *S.Units : NoUnits;
	auto Seen = [&Units](const TOptional<int32>& Id) -> const FTCAudioUnit* { return Id.IsSet() ? Units.Find(Id.GetValue()) : nullptr; };

	// main.js: a killed unit (or a tower a collapse brought down) is marked; its removal below makes the wreck
	for (const FTCAudioShot& Sh : S.Shots)
	{
		if (!Sh.T.IsSet()) continue;
		const FTCAudioUnit* U = Units.Find(Sh.T.GetValue());
		if (!U) U = Held.Find(Sh.T.GetValue());
		if (U && (Sh.bKill || (Sh.K == TEXT("collapse") && U->Type == TEXT("tower")))) Killed.Add(Sh.T.GetValue());
	}

	// main.js: planes, parachutes, flak and shoot-downs (client/aircraft.js draws them; their sounds play here)
	for (const FTCAudioShot& Sh : S.Shots)
	{
		if (!AirShot(Sh) || !Sh.bHasPos) continue;
		const FVector2D At(Sh.X, Sh.Z);
		if (const TCHAR* Plane = SupportPlane(Sh.K)) Sink.Play(Plane, &At, FTCAudioPlayOptions());
		else if (Sh.K == TEXT("shotdown") || Sh.K == TEXT("planedown") || Sh.K == TEXT("flak")) Sink.Play(TEXT("flak"), &At, FTCAudioPlayOptions());
		else if (Sh.K == TEXT("aa")) AntiAir(Sh, Seen(Sh.F), Seen(Sh.T), Sink);
	}

	// projectiles.js snapshot: launches within 0.2 s, contacts within 0.25 s of the server time, each once
	if (S.Tick >= ProjTick)
	{
		ProjTick = S.Tick;
		const double ServerTime = S.Tick / 20.0;
		for (const FTCAudioShot& Sh : S.Shots)
		{
			if (Sh.K == TEXT("flight"))
			{
				if (Fresh(Sh.Flight, 0) && ServerTime - Sh.Time < 0.2) ProjectileLaunch(Sh, Sink);
			}
			else if (Sh.K == TEXT("contact") || Sh.K == TEXT("expiry"))
			{
				if (!Fresh(Sh.Flight, Sh.Seq)) continue;
				if (Sh.K == TEXT("contact") && ServerTime - Sh.Time < 0.25) ProjectileContact(Sh, Sink);
			}
		}
	}

	// fx.js syncStrikes: an artillery strike's whistle once it is 1.4 s or less from landing
	TSet<FString> KeepStrikes;
	for (const FTCAudioStrike& St : S.Strikes)
	{
		if (SupportPlane(St.Kind) || St.Kind != TEXT("artillery")) continue;
		const FString Key = FString::Printf(TEXT("%s,%s,%s"), *St.Kind, *JsNum(St.X), *JsNum(St.Z));
		KeepStrikes.Add(Key);
		if (St.T <= 1.4 && !StrikeSounds.Contains(Key))
		{
			StrikeSounds.Add(Key);
			Whistle(0.0, St.T, St.X, St.Z, 0.0, Sink);
		}
	}
	for (auto It = StrikeSounds.CreateIterator(); It; ++It) if (!KeepStrikes.Contains(*It)) It.RemoveCurrent();

	// fx.js snapshot: the ground war
	for (const FTCAudioShot& Sh : S.Shots)
	{
		if (AirShot(Sh) || (S.OwnShot && S.OwnShot(Sh))) continue;
		const FString& K = Sh.K;
		if (Sh.bLocal && (K == TEXT("flight") || K == TEXT("contact") || K == TEXT("expiry") || Sh.bPhysical)) continue;
		if (!Sh.bHasPos || K == TEXT("hurt")) continue;
		const FTCAudioUnit* From = Seen(Sh.F);
		if (K == TEXT("throw"))
		{
			// a grenade or satchel in flight is silent; it is remembered so its blast knows a satchel
			if (!From) continue;
			FThrow& Th = Throws[ThrowNext++ % UE_ARRAY_COUNT(Throws)];
			Th = { Sh.X, Sh.Z, From->Type == TEXT("ranger") || From->Type == TEXT("churchill"), FxClock };
			continue;
		}
		if (K == TEXT("boom"))
		{
			const FThrow* Th = nullptr;
			for (const FThrow& Q : Throws) if (FxClock - Q.T < 6.0 && FMath::Abs(Q.X - Sh.X) < 0.5 && FMath::Abs(Q.Z - Sh.Z) < 0.5) { Th = &Q; break; }
			Fx(Th && Th->bSatchel ? TEXT("blast_large") : TEXT("blast_small"), Sh.X, Sh.Z, 0.0, Sink);
		}
		else if (K == TEXT("shell")) Fx(TEXT("blast_large"), Sh.X, Sh.Z, 0.0, Sink);
		else if (K == TEXT("rocket"))
		{
			FTCAudioPlayOptions Rocket;
			Rocket.Gain = 0.8;
			Fx(TEXT("blast_small"), Sh.X, Sh.Z, 0.0, Sink, &Rocket);
		}
		else if (K == TEXT("bomb")) Fx(TEXT("bomb"), Sh.X, Sh.Z, 0.0, Sink);
		else if (K == TEXT("salvo")) Salvo(Sh, From, Sink);
		else if (K == TEXT("aa")) AntiAir(Sh, From, Seen(Sh.T), Sink);
		else if (K == TEXT("collapse"))
		{
			// every wrecked map cell is a collapse; hedges, wire and trees (they turn to open ground) only crunch
			if (!Sh.Id.IsEmpty()) continue;
			FTCAudioPlayOptions Light;
			Light.Gain = 0.3;
			Light.Rate = 1.3;
			const bool bOpen = S.OpenedCell && S.OpenedCell(Sh.X, Sh.Z);
			Fx(TEXT("collapse"), Sh.X, Sh.Z, 0.0, Sink, bOpen ? &Light : nullptr);
		}
		else if (K == TEXT("smokeshells"))
		{
			for (int32 I = 0; I < 3; I++)
			{
				const double X = Sh.X + Rr(-6, 6), Z = Sh.Z + Rr(-6, 6);
				Later(I * 0.25, TEXT("smoke"), X, Z);
			}
		}
		else
		{
			FTCAudioType Def;
			if (S.TypeOf && S.TypeOf(K, Def) && Def.bKnown) DirectFire(Sh, From, Seen(Sh.T), S, Sink);
		}
	}

	// fx.js syncClouds: a new smoke screen pops (a cloud over a fire is that fire's smoke and is silent)
	TSet<int64> KeepClouds;
	for (const FTCAudioSmoke& C : S.Smokes)
	{
		const double Raw = C.X * 100003.0 + C.Z;
		uint64 Bits = 0;
		FMemory::Memcpy(&Bits, &Raw, sizeof(Bits));
		const int64 Key = C.Id.IsSet() ? (int64)C.Id.GetValue() : (int64)(Bits | (1ull << 63));
		if (!Clouds.Contains(Key))
		{
			bool bFire = false;
			for (const FTCAudioFire& F : S.Fires)
				if (F.Kind != '.' && FMath::Sqrt(FMath::Square(F.X - C.X) + FMath::Square(F.Z - C.Z)) < C.R * 1.5) { bFire = true; break; }
			if (!bFire) Fx(TEXT("smoke"), C.X, C.Z, 0.15, Sink);
		}
		KeepClouds.Add(Key);
	}
	Clouds = MoveTemp(KeepClouds);

	// main.js removeUnit: a killed vehicle or building wrecks (vehicle_destroyed and a fire, or a collapse); a killed
	// tower topples first; planes fall as their own copy (client/aircraft.js) and are silent here
	for (const TPair<int32, FTCAudioUnit>& H : Held)
	{
		if (Units.Contains(H.Key)) continue;
		const FTCAudioUnit& V = H.Value;
		const bool bKilled = Killed.Contains(H.Key);
		if (V.bAir) {}
		else if (bKilled && V.Type == TEXT("tower"))
		{
			const double Dir = Rand() * UE_DOUBLE_TWO_PI;
			Later(ToppleSeconds(), TEXT("collapse"), V.X + FMath::Cos(Dir) * 6.0, V.Z + FMath::Sin(Dir) * 6.0);
		}
		else if (bKilled && !V.bInfantry)
		{
			if (V.bStructure) Fx(TEXT("collapse"), V.X, V.Z, 0.0, Sink);
			else
			{
				Fx(TEXT("vehicle_destroyed"), V.X, V.Z, 0.0, Sink);
				FTCAudioPlayOptions Fire;
				Fire.Delay = 0.8;
				Fire.Dur = 14.0;
				Fx(TEXT("fire_loop"), V.X, V.Z, 0.0, Sink, &Fire);
			}
		}
	}
	for (auto It = Killed.CreateIterator(); It; ++It) if (!Units.Contains(*It)) It.RemoveCurrent();
	Held = Units;
}

void FTCAudioCues::Battle(double Now, const TMap<int32, FTCAudioUnit>& Units, int32 Me, ITCAudioSink& Sink)
{
	using namespace TCAudioDetail;
	if (Now - BattleTick < TCAudioRules::BattleEvery) return;
	BattleTick = Now;
	TArray<FVector2D> Engines;
	for (const TPair<int32, FTCAudioUnit>& P : Units)
	{
		const FTCAudioUnit& V = P.Value;
		const FVector2D* Prev = BattleSeen.Find(P.Key);
		const bool bMoving = Prev && FVector2D::Distance(*Prev, FVector2D(V.X, V.Z)) > 0.25;
		BattleSeen.Add(P.Key, FVector2D(V.X, V.Z));
		if (BattleVehicle(V) && bMoving) Engines.Add(FVector2D(V.X, V.Z));
		const TCHAR* Work = (V.Flags & TCAudioRules::DigFlag) ? TEXT("dig") : (V.Flags & TCAudioRules::BuildFlag) ? TEXT("build") : nullptr;
		const double* Next = Foley.Find(P.Key);
		if (Work && !bMoving && Now > (Next ? *Next : 0.0))
		{
			Foley.Add(P.Key, Now + 1.2 + Rand() * 0.8);
			FTCAudioPlayOptions O;
			O.Gain = V.Owner == Me ? 1.0 : 0.7;
			const FVector2D At(V.X, V.Z);
			Sink.Play(Work, &At, O);
		}
	}
	Sink.Bed(TEXT("tank_engine"), Engines);
	if (BattleSeen.Num() > Units.Num() + 50)
	{
		for (auto It = BattleSeen.CreateIterator(); It; ++It)
			if (!Units.Contains(It.Key())) { Foley.Remove(It.Key()); It.RemoveCurrent(); }
	}
}

// ---------------------------------------------------------------------------------------------------------------------
// Adapters from Net/TCMatchState (read only)

FTCAudioShot FTCAudioShot::From(const FTCShot& S)
{
	FTCAudioShot O;
	O.K = S.K;
	O.Kind = S.Kind;
	O.Effect = S.Effect;
	O.Id = S.Id;
	O.bHasPos = S.bHasPos;
	O.X = S.X;
	O.Z = S.Z;
	if (S.bHasShooter) O.F = S.F;
	if (S.bHasTarget) O.T = S.T;
	O.Flight = S.Flight;
	O.Seq = S.Seq;
	O.Time = S.Time;
	O.bHit = S.bHit;
	// The decoder keeps hit as a truthy flag, so a delivered hit: false reads as absent (main.js airShot asks for the
	// field's presence on 'flak'); it does not carry n (a salvo's count) or physical either. At the pin every salvo and
	// throw is pushed with physical: true and local: true (shared/sim.js 2104, 4153, 5427), so local stands in for it.
	O.bHitPresent = S.bHit;
	O.bKill = S.bKill;
	O.bTerminal = S.bTerminal;
	O.bLocal = S.bLocal;
	O.bPhysical = S.bLocal && (S.K == TEXT("salvo") || S.K == TEXT("throw"));
	return O;
}

TMap<int32, FTCAudioUnit> TCAudioView::Units(const FTCMatchState& S, const FTCUnitTable& Table)
{
	TMap<int32, FTCAudioUnit> Out;
	Out.Reserve(S.Units.Num());
	for (const TPair<int32, FTCUnit>& P : S.Units)
	{
		const FTCUnit& U = P.Value;
		FTCAudioUnit A;
		A.Id = U.Id;
		A.Owner = U.Owner;
		A.Flags = U.Flags;
		A.Type = U.Type;
		A.X = U.X;
		A.Z = U.Z;
		if (const FTCUnitDef* D = Table.Find(U.Type))
		{
			A.bKnown = true;
			A.bInfantry = D->bInfantry;
			A.bStructure = D->bStructure;
			A.bAir = D->bAir;
			A.Radius = D->Radius;
			// main.js: alive = operative ? 1 : ceil(hp * models / (def.models * def.hpPer))
			A.Alive = (U.Flags & TCFlags::Operative) ? 1 : (D->HpPer > 0 ? FMath::CeilToInt((double)U.Hp / D->HpPer) : 1);
		}
		Out.Add(P.Key, A);
	}
	return Out;
}

TArray<FTCAudioShot> TCAudioView::Shots(const FTCMatchState& S)
{
	TArray<FTCAudioShot> Out;
	Out.Reserve(S.Shots.Num());
	for (const FTCShot& Sh : S.Shots) Out.Add(FTCAudioShot::From(Sh));
	return Out;
}

TArray<FTCAudioFire> TCAudioView::Fires(const FTCMatchState& S)
{
	// main.js: each burning cell at its centre with the terrain character under it
	TArray<FTCAudioFire> Out;
	const int32 W = S.Map.W;
	if (W <= 0) return Out;
	for (const int32 C : S.Fires)
	{
		if (!S.Map.IsValidCell(C)) continue;
		FTCAudioFire F;
		F.X = (C % W + 0.5) * TCAudioRules::CellMetres;
		F.Z = (C / W + 0.5) * TCAudioRules::CellMetres;
		F.Kind = S.Map.Chars.IsValidIndex(C) ? (TCHAR)S.Map.Chars[C] : '.';
		Out.Add(F);
	}
	return Out;
}

TCAudioRules::FEnvironment TCAudioView::Environment(const FTCMatchState& S, const TCAudioRules::FEar& Ear)
{
	const FTCMap& M = S.Map;
	double Wx[4] = { S.Wx.Rain, S.Wx.Wet, S.Wx.WindAngle, S.Wx.WindStrength };
	const bool bGrid = S.bHasMatch && M.W > 0 && M.H > 0 && M.Chars.Num() == M.W * M.H;
	auto Classify = [&S, &M](int32 X, int32 Z)
	{
		const int32 C = Z * M.W + X;
		// only cells this side has seen or explored (and, in World Conquest, discovered)
		if (M.bWorld && !(M.Known.IsValidIndex(C) && M.Known[C])) return TCAudioRules::ECell::None;
		if (!S.CellVisible(C) && !S.CellExplored(C)) return TCAudioRules::ECell::None;
		const TCHAR Ch = (TCHAR)M.Chars[C];
		if (Ch == 'W' || Ch == 'F' || Ch == '=') return TCAudioRules::ECell::Water;
		return Ch == 'O' ? TCAudioRules::ECell::Woodland : TCAudioRules::ECell::None;
	};
	return TCAudioRules::EnvironmentMix(bGrid ? M.W : 0, bGrid ? M.H : 0, Classify, &Ear, S.Weather.Now, Wx, S.Wx.bKnown ? 4 : 0);
}

// ---------------------------------------------------------------------------------------------------------------------
// The environment loops (audio.js environmentalBuffer and the bed's BiquadFilterNode)

TArray<float> TCAudioRules::RenderNoise(const FString& Kind, int32 Rate)
{
	const int32 Seconds = 8, N = Seconds * Rate;
	const bool bRain = Kind == TEXT("rain"), bWater = Kind == TEXT("water");
	uint32 Seed = Kind == TEXT("wind") ? 41u : bWater ? 73u : Kind == TEXT("woodland") ? 107u : 149u;
	TArray<float> Data;
	Data.SetNumUninitialized(N);
	double Low = 0.0;
	for (int32 I = 0; I < N; I++)
	{
		Seed = Seed * 1664525u + 1013904223u; // (Math.imul(seed, 1664525) + 1013904223) >>> 0
		const double Noise = Seed / 2147483648.0 - 1.0;
		Low += (Noise - Low) * 0.08;
		const double T = (double)I / Rate, Flutter = 0.7 + 0.3 * FMath::Sin(T / Seconds * UE_DOUBLE_PI * 6.0);
		float V = (float)((bRain ? Noise * 0.35 : bWater ? Noise * 0.2 + Low * 0.8 : Low * 2.0) * Flutter);
		// join the endpoints quietly: two short tapers avoid a click when the loop wraps
		V = (float)(V * FMath::Min3(1.0, I / (Rate * 0.04), (N - 1 - I) / (Rate * 0.04)));
		Data[I] = V;
	}
	// the bed's filter (Web Audio BiquadFilterNode, Q 0.45: in dB for low and high pass, linear for band pass). Run
	// twice over the loop and keep the second pass, so the filter's state at the wrap is that of a playing loop.
	const double F0 = Kind == TEXT("wind") ? 280.0 : bWater ? 900.0 : Kind == TEXT("woodland") ? 650.0 : 1700.0, Q = 0.45;
	const double W0 = 2.0 * UE_DOUBLE_PI * F0 / Rate, Cw = FMath::Cos(W0), Sw = FMath::Sin(W0);
	double B0, B1, B2, A0, A1, A2;
	if (bWater)
	{
		const double Alpha = Sw / (2.0 * Q);
		B0 = Alpha; B1 = 0.0; B2 = -Alpha; A0 = 1.0 + Alpha; A1 = -2.0 * Cw; A2 = 1.0 - Alpha;
	}
	else
	{
		const double Alpha = Sw / (2.0 * FMath::Pow(10.0, Q / 20.0));
		if (bRain) { B0 = (1.0 + Cw) / 2.0; B1 = -(1.0 + Cw); B2 = (1.0 + Cw) / 2.0; }
		else { B0 = (1.0 - Cw) / 2.0; B1 = 1.0 - Cw; B2 = (1.0 - Cw) / 2.0; }
		A0 = 1.0 + Alpha; A1 = -2.0 * Cw; A2 = 1.0 - Alpha;
	}
	TArray<float> Out;
	Out.SetNumUninitialized(N);
	double X1 = 0, X2 = 0, Y1 = 0, Y2 = 0;
	for (int32 Pass = 0; Pass < 2; Pass++)
	{
		for (int32 I = 0; I < N; I++)
		{
			const double X = Data[I], Y = (B0 * X + B1 * X1 + B2 * X2 - A1 * Y1 - A2 * Y2) / A0;
			X2 = X1; X1 = X; Y2 = Y1; Y1 = Y;
			Out[I] = (float)Y;
		}
	}
	return Out;
}

// ---------------------------------------------------------------------------------------------------------------------
// UTCAudio: the mixer's sounds through Unreal audio components

namespace TCAudioDetail
{
	constexpr double PlaceCm = 5000.0;                   // how far from the listener a sound is put to pan it
	constexpr double FrontPairDegrees = 30.0;             // Unreal's stereo pair (FMixerDevice::Get3DChannelMap)
	constexpr int32 NoiseRate = 48000;
}

namespace TCAudioDetail
{
	// Forwards the cue selector's sounds to the mixer and counts them by name (the log's "asked")
	struct FCountingSink : ITCAudioSink
	{
		ITCAudioSink& Inner;
		TMap<FString, int32>& Counts;
		FCountingSink(ITCAudioSink& In, TMap<FString, int32>& C) : Inner(In), Counts(C) {}
		virtual void Play(const FString& Name, const FVector2D* Pos, const FTCAudioPlayOptions& Opts) override { Counts.FindOrAdd(Name)++; Inner.Play(Name, Pos, Opts); }
		virtual void Bed(const FString& Name, TConstArrayView<FVector2D> Points) override { Inner.Bed(Name, Points); }
		virtual double Length(const FString& Name) const override { return Inner.Length(Name); }
	};
}

FString UTCAudio::TakeCueReport()
{
	if (!Asked.Num() && Starts == StartsReported) return FString();
	Asked.ValueSort([](int32 A, int32 B) { return A > B; });
	TArray<FString> Parts;
	for (const TPair<FString, int32>& P : Asked) Parts.Add(FString::Printf(TEXT("%s %d"), *P.Key, P.Value));
	const FString Out = FString::Printf(TEXT("asked [%s], components started %d (%d in all), mixer %s"), *FString::Join(Parts, TEXT(", ")), Starts - StartsReported, Starts, Mixer.IsRunning() ? TEXT("running") : TEXT("suspended (no audio device)"));
	Asked.Reset();
	StartsReported = Starts;
	return Out;
}

bool UTCAudio::ShouldCreateSubsystem(UObject* Outer) const
{
	const UWorld* W = Cast<UWorld>(Outer);
	return W && (W->WorldType == EWorldType::Game || W->WorldType == EWorldType::PIE);
}

void UTCAudio::Initialize(FSubsystemCollectionBase& Collection)
{
	Super::Initialize(Collection);
	MasterDynamics = TCLook30::CreateMasterDynamics(this);
	if (GEngine && GEngine->GetAudioDeviceManager()) GEngine->GetAudioDeviceManager()->RegisterSoundSubmix(MasterDynamics);
	const int32 Seed = (int32)(FPlatformTime::Cycles() & 0x7fffffff);
	Mixer.Reseed(Seed);
	Cue.Reseed(Seed ^ 0x2545F491);
	const FTCSettingsStore& Preferences = FTCSettingsStore::Game();
	const bool bSaved = Preferences.Get(TEXT("ww2-volume")).IsSet() || Preferences.Get(TEXT("ww2-muted")).IsSet();
	Mixer.InitPreferences(Preferences);
	SetVolume(Mixer.Volume()); // bind() saves the starting volume
	UE_LOG(LogTC, Log, TEXT("audio: reference %s, volume %.0f%%%s"), TCAudioRules::ReferencePin, Mixer.Volume() * 100.0, bSaved ? TEXT(" (saved preferences)") : TEXT(" (default)"));
}

void UTCAudio::Deinitialize()
{
	for (TPair<int32, TObjectPtr<UAudioComponent>>& C : Components) if (C.Value) { C.Value->Stop(); C.Value->DestroyComponent(); }
	Components.Reset();
	Started.Reset();
	if (MasterDynamics && GEngine && GEngine->GetAudioDeviceManager()) GEngine->GetAudioDeviceManager()->UnregisterSoundSubmix(MasterDynamics);
	MasterDynamics = nullptr;
	if (SfxHandle.IsValid()) SfxHandle->CancelHandle();
	if (VoiceHandle.IsValid()) VoiceHandle->CancelHandle();
	Super::Deinitialize();
}

TStatId UTCAudio::GetStatId() const { RETURN_QUICK_DECLARE_CYCLE_STAT(UTCAudio, STATGROUP_Tickables); }

void UTCAudio::SetVolume(double V)
{
	Mixer.SetVolume(V);
	if (Mixer.TakeVolumeChanged()) FTCSettingsStore::Game().Set(TEXT("ww2-volume"), FString::Printf(TEXT("%.17g"), Mixer.Volume()));
}

void UTCAudio::ToggleMute()
{
	Mixer.ToggleMute();
	SetVolume(Mixer.Volume());
	UE_LOG(LogTC, Log, TEXT("audio: %s (volume %.0f%%)"), Mixer.Volume() > 0.0 ? TEXT("unmuted") : TEXT("muted"), Mixer.Volume() * 100.0);
}

void UTCAudio::MatchStarted(int32 Faction, int32 Slot)
{
	SnapshotRaw.Reset(); SnapshotMuzzle = {}; SnapshotHeight = {};
	const int32 Token = Mixer.BeginMatch(Faction, Slot);
	Cue.Reset();
	AmbienceTimer = 0.0;
	BattleViewAt = -1e9;
	LoadAssets(Token);
}

void UTCAudio::ReturnedToLobby()
{
	SnapshotRaw.Reset(); SnapshotMuzzle = {}; SnapshotHeight = {};
	Mixer.End();
	Cue.Reset();
}

UTCAudio::FBankState UTCAudio::BankState() const
{
	FBankState State;
	State.Generation = Mixer.MatchToken();
	State.bCallbackCompleted = CompletedLoadToken == State.Generation;
	State.Folder = Mixer.VoiceFolder(); State.Speaker = Mixer.VoiceName();
	for (const TCAudioRules::FClip& Clip : TCAudioRules::Clips())
		if (Waves.Contains(Clip.Id) && Mixer.IsLoaded(Clip.Id)) ++State.Effects;
	if (!State.Speaker.IsEmpty())
	{
		const TArray<FString> Stems = TCAudioRules::VoiceStems(State.Speaker);
		State.ExpectedVoiceLines = Stems.Num();
		for (const FString& Stem : Stems)
		{
			const FString Key = TEXT("voice:") + Stem;
			if (Waves.Contains(Key) && Mixer.IsLoaded(Key)) ++State.VoiceLines;
		}
	}
	return State;
}

void UTCAudio::LoadAssets(int32 Token)
{
	// the effects load once, at the first match start (never in the lobby), and stay
	TArray<FSoftObjectPath> Paths;
	for (const TCAudioRules::FClip& C : TCAudioRules::Clips()) if (!Waves.Contains(C.Id)) Paths.Add(FSoftObjectPath(TCAudioRules::SfxAssetPath(C.Id)));
	if (Paths.Num() == 0) { OnSfxLoaded(Token); return; }
	TWeakObjectPtr<UTCAudio> Self(this);
	SfxHandle = Streamable.RequestAsyncLoad(Paths, FStreamableDelegate::CreateLambda([Self, Token]() { if (Self.IsValid()) Self->OnSfxLoaded(Token); }));
	if (!SfxHandle.IsValid()) OnSfxLoaded(Token);
}

void UTCAudio::OnSfxLoaded(int32 Token)
{
	int32 Missing = 0;
	for (const TCAudioRules::FClip& C : TCAudioRules::Clips())
	{
		if (Waves.Contains(C.Id)) continue;
		USoundWave* W = Cast<USoundWave>(FSoftObjectPath(TCAudioRules::SfxAssetPath(C.Id)).ResolveObject());
		if (!W) { Missing++; continue; }
		Waves.Add(C.Id, W);
		Mixer.SetLoaded(C.Id, W->Duration);
	}
	if (Missing && !bWarnedMissing)
	{
		bWarnedMissing = true;
		UE_LOG(LogTC, Warning, TEXT("audio: %d of %d effects did not load from /Game/Audio/SFX (run unreal/Tools/import-reference-audio.sh); those stay quiet"), Missing, TCAudioRules::Clips().Num());
	}
	if (Token != Mixer.MatchToken()) return;
	FString Folder;
	const FString Speaker = Mixer.ChooseVoice(Token, Folder);
	TArray<FSoftObjectPath> Paths;
	if (!Speaker.IsEmpty())
		for (const FString& Stem : TCAudioRules::VoiceStems(Speaker))
			if (!Waves.Contains(TEXT("voice:") + Stem)) Paths.Add(FSoftObjectPath(TCAudioRules::VoiceAssetPath(Folder, Stem)));
	UE_LOG(LogTC, Log, TEXT("audio: effects ready (%d), voice %s/%s"), Waves.Num(), *Folder, Speaker.IsEmpty() ? TEXT("-") : *Speaker);
	if (Paths.Num() == 0) { OnVoicesLoaded(Token); return; }
	TWeakObjectPtr<UTCAudio> Self(this);
	VoiceHandle = Streamable.RequestAsyncLoad(Paths, FStreamableDelegate::CreateLambda([Self, Token]() { if (Self.IsValid()) Self->OnVoicesLoaded(Token); }));
	if (!VoiceHandle.IsValid()) OnVoicesLoaded(Token);
}

void UTCAudio::OnVoicesLoaded(int32 Token)
{
	if (Token != Mixer.MatchToken()) return;
	const FString& Folder = Mixer.VoiceFolder();
	const FString& Speaker = Mixer.VoiceName();
	int32 Got = 0;
	for (const FString& Stem : TCAudioRules::VoiceStems(Speaker))
	{
		const FString Key = TEXT("voice:") + Stem;
		if (!Waves.Contains(Key))
		{
			USoundWave* W = Cast<USoundWave>(FSoftObjectPath(TCAudioRules::VoiceAssetPath(Folder, Stem)).ResolveObject());
			if (!W) continue;
			Waves.Add(Key, W);
		}
		Mixer.SetLoaded(Key, Waves[Key]->Duration);
		Got++;
	}
	if (!Speaker.IsEmpty() && Got == 0) UE_LOG(LogTC, Warning, TEXT("audio: no voice lines loaded for %s/%s"), *Folder, *Speaker);
	CompletedLoadToken = Token;
	const FBankState Bank = BankState();
	UE_LOG(LogTC, Log, TEXT("audio: selected bank generation %d, %s/%s, effects %d/%d, voice lines %d/%d, complete %d (resource loading only)"),
		Bank.Generation, *Bank.Folder, *Bank.Speaker, Bank.Effects, TCAudioRules::Clips().Num(), Bank.VoiceLines, Bank.ExpectedVoiceLines, Bank.FullyLoaded());
	Mixer.FinishLoading(Token);
}

void TCAudioView::Extras(const FJsonObject& Raw, TArray<FTCAudioShot>& Shots, TArray<FTCAudioStrike>& Strikes, TArray<FTCAudioSmoke>& Smokes)
{
	Strikes.Reset(); Smokes.Reset();
	const TArray<FTCShotExtra> Fields = TCShotExtras::Read(&Raw);
	for (int32 I = 0; I < FMath::Min(Fields.Num(), Shots.Num()); ++I)
	{
		Shots[I].bPhysical = Fields[I].bPhysical;
		Shots[I].bHitPresent = Fields[I].bHasHitField;
		if (Fields[I].bHasN) Shots[I].N = Fields[I].N;
	}
	const TArray<TSharedPtr<FJsonValue>>* Rows = nullptr;
	if (Raw.TryGetArrayField(TEXT("strikes"), Rows)) for (const auto& Value : *Rows)
	{
		const TArray<TSharedPtr<FJsonValue>>* Row = nullptr;
		if (!Value || !Value->TryGetArray(Row) || Row->Num() < 5 || Strikes.Num() >= TCMatchLimits::Shots) continue;
		FTCAudioStrike Strike; double Direction;
		if (!(*Row)[0]->TryGetString(Strike.Kind) || !(*Row)[1]->TryGetNumber(Strike.X) || !(*Row)[2]->TryGetNumber(Strike.Z) || !(*Row)[3]->TryGetNumber(Direction) || !(*Row)[4]->TryGetNumber(Strike.T)) continue;
		Strikes.Add(MoveTemp(Strike));
	}
	if (Raw.TryGetArrayField(TEXT("smokes"), Rows)) for (const auto& Value : *Rows)
	{
		const TArray<TSharedPtr<FJsonValue>>* Row = nullptr;
		if (!Value || !Value->TryGetArray(Row) || Row->Num() < 3 || Smokes.Num() >= 256) continue;
		FTCAudioSmoke Smoke;
		if (!(*Row)[0]->TryGetNumber(Smoke.X) || !(*Row)[1]->TryGetNumber(Smoke.Z) || !(*Row)[2]->TryGetNumber(Smoke.R)) continue;
		double Id;
		if (Row->Num() > 3 && (*Row)[3]->TryGetNumber(Id) && Id >= MIN_int32 && Id <= MAX_int32) Smoke.Id = int32(Id);
		Smokes.Add(MoveTemp(Smoke));
	}
}

void UTCAudio::PrepareSnapshot(const FJsonObject& Raw, TFunction<bool(int32, int32, FVector&)> MuzzleAt, TFunction<float(float, float)> HeightAt)
{
	SnapshotRaw = MakeShared<FJsonObject>(Raw);
	SnapshotMuzzle = MoveTemp(MuzzleAt); SnapshotHeight = MoveTemp(HeightAt);
}

void UTCAudio::Snapshot(const FTCMatchState& S, const FTCUnitTable& Table)
{
	const TMap<int32, FTCAudioUnit> Units = TCAudioView::Units(S, Table);
	TArray<FTCAudioShot> Shots = TCAudioView::Shots(S);
	TArray<FTCAudioStrike> Strikes;
	TArray<FTCAudioSmoke> Smokes;
	if (SnapshotRaw) TCAudioView::Extras(*SnapshotRaw, Shots, Strikes, Smokes);
	SnapshotRaw.Reset();
	const TArray<FTCAudioFire> Fires = TCAudioView::Fires(S);
	FTCAudioSnapshot Snap;
	Snap.Tick = S.Tick;
	Snap.Shots = Shots;
	Snap.Units = &Units;
	Snap.Fires = Fires;
	Snap.Strikes = Strikes;
	Snap.Smokes = Smokes;
	Snap.MuzzleAt = MoveTemp(SnapshotMuzzle); Snap.HeightAt = MoveTemp(SnapshotHeight);
	const FTCUnitTable* T = &Table;
	Snap.TypeOf = [T](const FString& Type, FTCAudioType& Out)
	{
		const FTCUnitDef* D = T->Find(Type);
		if (!D) return false;
		Out.bKnown = true;
		Out.bInfantry = D->bInfantry;
		Out.bStructure = D->bStructure;
		Out.bAir = D->bAir;
		Out.Radius = D->Radius;
		return true;
	};
	// fx.js openCell: the collapse's cell is open ground ('.') now that this snapshot's cells are applied
	const FTCMap* M = &S.Map;
	Snap.OpenedCell = [M](double X, double Z)
	{
		if (M->W <= 0) return false;
		const int32 C = FMath::FloorToInt(Z / TCAudioRules::CellMetres) * M->W + FMath::FloorToInt(X / TCAudioRules::CellMetres);
		return M->IsValidCell(C) && M->Chars.IsValidIndex(C) && M->Chars[C] == '.';
	};
	TCAudioDetail::FCountingSink Sink(Mixer, Asked);
	Cue.Snapshot(Snap, Sink);
}

void UTCAudio::Frame(float Dt, const FTCMatchState& S, const FTCUnitTable& Table, FVector2D Focus, float Distance, float YawDegrees)
{
	Mixer.Listener(Focus.X, Focus.Y, Distance, TCAudioRules::BrowserYaw(YawDegrees));
	if (S.bHasMatch && Mixer.IsStarted())
	{
		// battle-sound.js looks at the units only every 0.2 s: build their view only then
		if (Mixer.Now() - BattleViewAt >= TCAudioRules::BattleEvery)
		{
			BattleViewAt = Mixer.Now();
			const TMap<int32, FTCAudioUnit> Units = TCAudioView::Units(S, Table);
			TCAudioDetail::FCountingSink Sink(Mixer, Asked);
			Cue.Battle(Mixer.Now(), Units, S.SeatConfirmed() ? S.You : -1, Sink);
		}
		if ((AmbienceTimer -= Dt) <= 0.0)
		{
			AmbienceTimer = TCAudioRules::AmbienceEvery;
			Mixer.Environment(TCAudioView::Environment(S, Mixer.Ear()));
		}
	}
	TCAudioDetail::FCountingSink Sink(Mixer, Asked);
	Cue.Frame(Dt, Sink);
}

void UTCAudio::FeedAlert(const FString& Kind)
{
	// alerts.js sound(): its own 1.5 s per kind, on the page clock, before audio.alert's gap
	const double T = FPlatformTime::Seconds();
	const double* Last = FeedLast.Find(Kind);
	if (T - (Last ? *Last : -1e9) < TCAudioRules::FeedSoundGap) return;
	FeedLast.Add(Kind, T);
	Asked.FindOrAdd(TEXT("alert:") + Kind)++;
	Mixer.Alert(Kind);
}

void UTCAudio::Blip(const FString& KindOrTone)
{
	const FString Kind = KindOrTone.IsNumeric() ? FString(TEXT("click")) : KindOrTone;
	Asked.FindOrAdd(TEXT("ui:") + Kind)++;
	Mixer.Ui(Kind);
}

void UTCAudio::Cues(TConstArrayView<FTCSoundCue> Sounds)
{
	// FTCObjectives::TakeSounds (epilogue.js and endgame.js call audio.play(name, null, { gain, rate, delay }))
	for (const FTCSoundCue& C : Sounds)
	{
		Asked.FindOrAdd(C.Name)++;
		FTCAudioPlayOptions O;
		O.Gain = C.Gain;
		O.Rate = C.Rate;
		O.Delay = C.Delay;
		Mixer.PlaySound(C.Name, nullptr, O);
	}
}

int32 UTCAudio::Play(const FString& Name, TOptional<FVector2D> Pos, const FTCAudioPlayOptions& Opts)
{
	return Mixer.PlaySound(Name, Pos.IsSet() ? &Pos.GetValue() : nullptr, Opts);
}

void UTCAudio::Tick(float DeltaTime)
{
	UWorld* W = GetWorld();
	FAudioDeviceHandle Device = W ? W->GetAudioDevice() : FAudioDeviceHandle();
	Mixer.SetRunning(Device.IsValid());
	if (!Device.IsValid() && !bWarnedNoDevice && W)
	{
		bWarnedNoDevice = true;
		UE_LOG(LogTC, Log, TEXT("audio: no audio device in this run (-nosound, or none found): the mixer stays suspended and plays nothing, as the browser's suspended context does; the cues asked are still counted"));
	}
	Mixer.Advance(DeltaTime);
	Sync();
}

void UTCAudio::RouteToMaster(USoundBase* Sound)
{
	// Every source reaches the same compressor once, after its mixer gain. No parallel dry send.
	Sound->bEnableBaseSubmix = true;
	Sound->SoundSubmixObject = MasterDynamics;
}

UAudioComponent* UTCAudio::MakeComponent(USoundBase* Sound)
{
	if (!Sound || !MasterDynamics) return nullptr;
	RouteToMaster(Sound);
	UAudioComponent* C = UGameplayStatics::CreateSound2D(this, Sound, 1.f, 1.f, 0.f, nullptr, false, false);
	if (!C) return nullptr;
	// the reference computes every gain and pan itself: no attenuation, spatialized only to pan
	FSoundAttenuationSettings A;
	A.bAttenuate = false;
	A.bSpatialize = true;
	A.SpatializationAlgorithm = ESoundSpatializationAlgorithm::SPATIALIZATION_Default;
	A.NonSpatializedRadiusStart = 0.f;
	A.NonSpatializedRadiusEnd = 0.f;
	A.bAttenuateWithLPF = false;
	A.bEnableListenerFocus = false;
	A.bEnableOcclusion = false;
	A.bEnableReverbSend = false;
	A.bEnablePriorityAttenuation = false;
	A.bEnableLogFrequencyScaling = false;
	A.bEnableSubmixSends = false;
	A.bEnableSourceDataOverride = false;
	C->bAllowSpatialization = true;
	C->bOverrideAttenuation = true;
	C->AttenuationOverrides = A;
	C->bIsUISound = true; // heard while the game is paused, as the browser's are
	C->bAutoDestroy = false;
	return C;
}

void UTCAudio::Place(UAudioComponent* C, double Pan) const
{
	// equal-power pan between the front pair at -30 and +30 degrees: the azimuth Pan * 30 gives the stereo panner's law
	const double Az = FMath::DegreesToRadians(FMath::Clamp(Pan, -1.0, 1.0) * TCAudioDetail::FrontPairDegrees);
	C->SetWorldLocation(ListenerAt + (ListenerFront * FMath::Cos(Az) + ListenerRight * FMath::Sin(Az)) * TCAudioDetail::PlaceCm);
}

USoundWaveProcedural* UTCAudio::MakeNoise(const FString& Kind)
{
	// the loop is rendered once per kind and shared; each bed gets its own wave, fed on the audio render thread when it
	// runs short (QueueAudio is thread safe and the samples never change once rendered)
	TSharedPtr<TArray<int16>, ESPMode::ThreadSafe>& Loop = NoiseLoops.FindOrAdd(Kind);
	if (!Loop.IsValid())
	{
		const TArray<float> F = TCAudioRules::RenderNoise(Kind, TCAudioDetail::NoiseRate);
		Loop = MakeShared<TArray<int16>, ESPMode::ThreadSafe>();
		Loop->SetNumUninitialized(F.Num());
		for (int32 I = 0; I < F.Num(); I++) (*Loop)[I] = (int16)FMath::Clamp(FMath::RoundToInt(F[I] * 32767.f), -32768, 32767);
	}
	USoundWaveProcedural* W = NewObject<USoundWaveProcedural>(this);
	W->SetSampleRate(TCAudioDetail::NoiseRate);
	W->NumChannels = 1;
	W->Duration = INDEFINITELY_LOOPING_DURATION;
	W->SoundGroup = SOUNDGROUP_Default;
	W->bLooping = true;
	TSharedPtr<TArray<int16>, ESPMode::ThreadSafe> Samples = Loop;
	TSharedPtr<int32, ESPMode::ThreadSafe> Cursor = MakeShared<int32, ESPMode::ThreadSafe>(0);
	W->OnSoundWaveProceduralUnderflow = FOnSoundWaveProceduralUnderflow::CreateLambda([Samples, Cursor](USoundWaveProcedural* Wave, int32 Needed)
	{
		const int32 N = Samples->Num();
		if (!Wave || N == 0) return;
		int32 Left = FMath::Max(Needed, 4096);
		while (Left > 0)
		{
			const int32 Run = FMath::Min(Left, N - *Cursor);
			Wave->QueueAudio(reinterpret_cast<const uint8*>(Samples->GetData() + *Cursor), Run * sizeof(int16));
			*Cursor = (*Cursor + Run) % N;
			Left -= Run;
		}
	});
	return W;
}

void UTCAudio::Sync()
{
	UWorld* W = GetWorld();
	if (!W) return;
	if (APlayerController* PC = W->GetFirstPlayerController()) PC->GetAudioListenerPosition(ListenerAt, ListenerFront, ListenerRight);
	const double Now = Mixer.Now(), Master = Mixer.MasterGain();
	TSet<int32> Live;
	auto Drive = [&](int32 Handle, USoundBase* Sound, double StartOffset, double Volume, double Pitch, double Pan, const FString& What)
	{
		Live.Add(Handle);
		TObjectPtr<UAudioComponent>* Found = Components.Find(Handle);
		UAudioComponent* C = Found ? Found->Get() : nullptr;
		if (!Found && !Started.Contains(Handle))
		{
			Started.Add(Handle);
			C = Sound ? MakeComponent(Sound) : nullptr;
			if (!C) return;
			Components.Add(Handle, C);
			C->SetVolumeMultiplier(FMath::Max(0.0, Volume));
			C->SetPitchMultiplier(Pitch);
			Place(C, Pan);
			C->Play(StartOffset);
			Starts++;
			UE_LOG(LogTC, Log, TEXT("audio: play %s (%s) volume %.3f pitch %.3f pan %.2f offset %.2f at %.2f s"), *What, *Sound->GetPathName(), Volume, Pitch, Pan, StartOffset, Now);
			return;
		}
		if (!C) return;
		C->SetVolumeMultiplier(FMath::Max(0.0, Volume));
		C->SetPitchMultiplier(Pitch);
		Place(C, Pan);
	};
	for (const FTCAudioSound& P : Mixer.Sounds())
	{
		if (Now < P.StartAt && !Components.Contains(P.Handle)) { Live.Add(P.Handle); continue; }
		TObjectPtr<USoundWave>* Wave = Waves.Find(P.Key);
		Drive(P.Handle, Wave ? Wave->Get() : nullptr, P.Offset, P.Gain.Value * P.Envelope(Now) * Mixer.BusGain(P.Bus) * Master, P.Rate, P.Pan.Value, P.Name == P.Key ? P.Name : P.Name + TEXT(" / ") + P.Key);
	}
	for (const FTCAudioBed& B : Mixer.Beds())
	{
		USoundBase* Sound = nullptr;
		if (B.bSynth)
		{
			TObjectPtr<USoundWaveProcedural>& N = Noise.FindOrAdd(B.Handle);
			if (!N) N = MakeNoise(B.Name.RightChop(4));
			Sound = N;
		}
		else if (TObjectPtr<USoundWave>* Wave = Waves.Find(B.Name)) Sound = *Wave;
		Drive(B.Handle, Sound, B.bSynth ? 0.0 : B.StartOffset, B.Gain.Value * B.StopFactor(Now) * Mixer.BusGain(B.Bus) * Master, B.Rate.Value, B.Pan.Value, TEXT("bed ") + B.Name);
	}
	for (auto It = Components.CreateIterator(); It; ++It)
	{
		if (Live.Contains(It.Key())) continue;
		if (UAudioComponent* C = It.Value()) { C->Stop(); C->DestroyComponent(); }
		It.RemoveCurrent();
	}
	for (auto It = Started.CreateIterator(); It; ++It) if (!Live.Contains(*It)) It.RemoveCurrent();
	for (auto It = Noise.CreateIterator(); It; ++It) if (!Live.Contains(It.Key())) It.RemoveCurrent();
}


// ---------------------------------------------------------------------------------------------------------------------
// Automation tests

#if WITH_DEV_AUTOMATION_TESTS

namespace TCAudioTest
{
	struct FTCAudioOracleOp
	{
		double T;
		const TCHAR* Op;
		const TCHAR* Name;
		double X, Z;
		int32 HasPos;
		double A, B, C;
	};
	struct FTCAudioOracleRow
	{
		int32 Start;
		const TCHAR* Key;
		double At, Offset, Rate, Gain, Pan;
		int32 Loop;
	};
	// Generated by unreal/Tools/audio/mixer-oracle.mjs (sha256 ea55fcb6...) from the
	// pinned client/audio.js itself, run under Node 26 with a fake Web Audio context, Math.random = mulberry32(20261013)
	// and every voice line decoded as 1.5 s. Not hand-written: regenerate with
	//   node mixer-oracle.mjs reference/ww2-rts
	constexpr uint32 OracleSeed = 20261013u;
	constexpr double OracleVoiceSeconds = 1.5;
	static const FTCAudioOracleOp OracleOps[] = {
		{ 0.0, TEXT("start"), TEXT(""), 0.0, 0.0, 0, 3.0, 1.0, 0.0 },
		{ 0.0, TEXT("listener"), TEXT(""), 10.0, 10.0, 0, 60.0, 0.3, 0.0 },
		{ 0.5, TEXT("play"), TEXT("rifle"), 12.0, 10.0, 1, 0.0, 0.0, 0.0 },
		{ 0.515625, TEXT("play"), TEXT("rifle"), 12.0, 10.0, 1, 0.0, 0.0, 0.0 },
		{ 0.625, TEXT("play"), TEXT("rifle"), 40.0, 30.0, 1, 0.0, 0.0, 0.0 },
		{ 0.75, TEXT("play"), TEXT("rifle"), 10.0, 10.0, 1, 0.0, 0.0, 0.0 },
		{ 0.796875, TEXT("play"), TEXT("rifle"), 35.0, 14.0, 1, 0.0, 0.0, 0.0 },
		{ 0.84375, TEXT("play"), TEXT("rifle"), 60.0, 18.0, 1, 0.0, 0.0, 0.0 },
		{ 0.890625, TEXT("play"), TEXT("rifle"), 10.0, 22.0, 1, 0.0, 0.0, 0.0 },
		{ 0.9375, TEXT("play"), TEXT("rifle"), 35.0, 26.0, 1, 0.0, 0.0, 0.0 },
		{ 0.984375, TEXT("play"), TEXT("rifle"), 60.0, 30.0, 1, 0.0, 0.0, 0.0 },
		{ 1.0, TEXT("voice"), TEXT("move"), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 1.5, TEXT("voice"), TEXT("move"), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 4.0, TEXT("voice"), TEXT("move"), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 7.0, TEXT("alert"), TEXT("attack"), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 7.6, TEXT("tick"), TEXT(""), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 8.0, TEXT("alert"), TEXT("attack"), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 8.5, TEXT("alert"), TEXT("unitLost"), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 9.1, TEXT("tick"), TEXT(""), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 9.25, TEXT("ui"), TEXT("error"), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 9.375, TEXT("ui"), TEXT("error"), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 9.5, TEXT("ui"), TEXT("bogus"), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
		{ 10.0, TEXT("play"), TEXT("fire_loop"), 15.0, 12.0, 1, 0.8, 14.0, 1.0 },
		{ 10.0, TEXT("play"), TEXT("blast_small"), 11.0, 10.0, 1, 0.0, 0.0, 0.0 },
		{ 10.25, TEXT("play"), TEXT("match_start"), 0.0, 0.0, 0, 0.0, 0.0, 0.7 },
		{ 11.0, TEXT("bed"), TEXT("tank_engine"), 20.0, 10.0, 0, 30.0, 15.0, 0.0 },
		{ 11.5, TEXT("listener"), TEXT(""), 30.0, 10.0, 0, 120.0, -0.5, 0.0 },
		{ 12.0, TEXT("play"), TEXT("bomb"), 35.0, 12.0, 1, 0.0, 0.0, 0.0 },
		{ 12.5, TEXT("play"), TEXT("whistle"), 30.0, 40.0, 1, 0.0, 0.0, 0.7 },
		{ 20.0, TEXT("play"), TEXT("bomber"), 10.0, -5.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("plane_flyby"), 17.0, -2.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("rockets"), 24.0, 1.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("bomb"), 31.0, 4.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("blast_large"), 38.0, 7.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("tankgun"), 45.0, 10.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("flak"), 12.0, 13.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("vehicle_destroyed"), 19.0, 16.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("smoke"), 26.0, 19.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("sniper"), 33.0, 22.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("atgun"), 40.0, -5.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("whistle"), 47.0, -2.0, 1, 0.0, 0.0, 0.0 },
		{ 20.0, TEXT("play"), TEXT("mg"), 14.0, 1.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("bomber"), 15.0, 6.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("plane_flyby"), 22.0, 9.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("rockets"), 29.0, 12.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("bomb"), 36.0, 15.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("blast_large"), 43.0, 18.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("tankgun"), 10.0, 21.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("flak"), 17.0, 24.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("vehicle_destroyed"), 24.0, -3.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("smoke"), 31.0, 0.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("sniper"), 38.0, 3.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("atgun"), 45.0, 6.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("whistle"), 12.0, 9.0, 1, 0.0, 0.0, 0.0 },
		{ 20.25, TEXT("play"), TEXT("mg"), 19.0, 12.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("bomber"), 20.0, 17.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("plane_flyby"), 27.0, 20.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("rockets"), 34.0, 23.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("bomb"), 41.0, -4.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("blast_large"), 48.0, -1.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("tankgun"), 15.0, 2.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("flak"), 22.0, 5.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("vehicle_destroyed"), 29.0, 8.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("smoke"), 36.0, 11.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("sniper"), 43.0, 14.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("atgun"), 10.0, 17.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("whistle"), 17.0, 20.0, 1, 0.0, 0.0, 0.0 },
		{ 20.5, TEXT("play"), TEXT("mg"), 24.0, 23.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("bomber"), 25.0, -2.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("plane_flyby"), 32.0, 1.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("rockets"), 39.0, 4.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("bomb"), 46.0, 7.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("blast_large"), 13.0, 10.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("tankgun"), 20.0, 13.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("flak"), 27.0, 16.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("vehicle_destroyed"), 34.0, 19.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("smoke"), 41.0, 22.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("sniper"), 48.0, -5.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("atgun"), 15.0, -2.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("whistle"), 22.0, 1.0, 1, 0.0, 0.0, 0.0 },
		{ 20.75, TEXT("play"), TEXT("mg"), 29.0, 4.0, 1, 0.0, 0.0, 0.0 },
		{ 22.0, TEXT("end"), TEXT(""), 0.0, 0.0, 0, 0.0, 0.0, 0.0 },
	};
	static const FTCAudioOracleRow OracleRows[] = {
		{ 1, TEXT("amb:wind"), 0.0, 0.0, 1.0, 0.0, 0.0, 1 },
		{ 1, TEXT("match_start"), 0.0, 0.0, 1.0, 0.7766928521730007, 0.0, 0 },
		{ 1, TEXT("rifle"), 0.5, 0.0, 1.0703801567852498, 0.4454090407952524, 0.0566125326889248, 0 },
		{ 1, TEXT("rifle"), 0.625, 0.0, 0.9265972435474396, 0.26631555800380274, 0.29808532614526323, 0 },
		{ 1, TEXT("rifle"), 0.75, 0.0, 0.9598264649510384, 0.3635168102791041, 0.0, 0 },
		{ 1, TEXT("rifle"), 0.796875, 0.0, 0.9205467230081559, 0.286978055820725, 0.360925973466877, 0 },
		{ 1, TEXT("rifle"), 0.84375, 0.0, 1.0348041201010347, 0.19676443444877315, 0.4802230625898255, 0 },
		{ 1, TEXT("rifle"), 0.890625, 0.0, 1.041457542590797, 0.33353537007912043, -0.07667551307969891, 0 },
		{ 0, TEXT("rifle"), 0.9975, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("rifle"), 0.9375, 0.0, 1.0796970680356026, 0.2407710662082268, 0.28024159428347145, 0 },
		{ 1, TEXT("voice:harry_move4_t1"), 1.0, 0.0, 1.0, 1.0395141227869318, 0.0, 0 },
		{ 1, TEXT("voice:harry_move2_t2"), 4.0, 0.0, 1.0, 0.9726547175087035, 0.0, 0 },
		{ 1, TEXT("alert_attack"), 7.0, 0.0, 1.0, 0.7112539164582267, 0.0, 0 },
		{ 1, TEXT("voice:harry_fire1_t1"), 7.6, 0.0, 1.0, 1.0340698144165799, 0.0, 0 },
		{ 1, TEXT("alert_unit_lost"), 8.5, 0.0, 1.0, 0.8587936969150738, 0.0, 0 },
		{ 1, TEXT("ui_error"), 9.25, 0.0, 1.0, 0.6026546150898336, 0.0, 0 },
		{ 1, TEXT("ui_click"), 9.5, 0.0, 1.0, 0.41591008579940536, 0.0, 0 },
		{ 1, TEXT("fire_loop"), 10.8, 0.0, 1.0, 0.6356075273247226, 0.11020225320805377, 1 },
		{ 0, TEXT("fire_loop"), 24.85, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("blast_small"), 10.0, 0.0, 1.0101987848430871, 1.0362876979008633, 0.02939496889617249, 0 },
		{ 1, TEXT("match_start"), 10.25, 0.0, 1.0, 0.4974865530226379, 0.0, 0 },
		{ 1, TEXT("tank_engine"), 11.0, 2.636916286428459, 1.0, 0.0, 0.0, 1 },
		{ 1, TEXT("bomb"), 12.0, 0.0, 0.9275826091691851, 0.6860311679226023, 0.14077301000269282, 0 },
		{ 1, TEXT("whistle"), 12.5, 0.0, 0.9915490394458175, 0.12774556768751513, 0.20920387139092497, 0 },
		{ 1, TEXT("bomber"), 20.0, 0.0, 0.9883927929168567, 0.4715561299648163, -0.395888549069928, 0 },
		{ 1, TEXT("plane_flyby"), 20.0, 0.0, 0.9523263898212463, 0.5046283281334176, -0.3215920125303269, 0 },
		{ 1, TEXT("rockets"), 20.0, 0.0, 0.9598294100025668, 0.5011505675826288, -0.21398593548614922, 0 },
		{ 1, TEXT("bomb"), 20.0, 0.0, 0.9235247182846069, 0.6330778202774814, -0.051448983475296445, 0 },
		{ 1, TEXT("blast_large"), 20.0, 0.0, 1.070492746308446, 0.6231659445201916, 0.13313578001468304, 0 },
		{ 1, TEXT("tankgun"), 20.0, 0.0, 1.0170493392087518, 0.4952439442002654, 0.26327476856711185, 0 },
		{ 1, TEXT("flak"), 20.0, 0.0, 1.0526582558080555, 0.3882263581492897, -0.2655958938126749, 0 },
		{ 1, TEXT("vehicle_destroyed"), 20.0, 0.0, 1.0189575584139674, 0.590231709135677, -0.14445747794725564, 0 },
		{ 1, TEXT("smoke"), 20.0, 0.0, 1.0119525838643313, 0.2725721288283665, 0.018468314903218947, 0 },
		{ 1, TEXT("sniper"), 20.0, 0.0, 0.9891110078431666, 0.4865158073861293, 0.17952384147789135, 0 },
		{ 1, TEXT("atgun"), 20.0, 0.0, 1.0315886163245886, 0.4896399982550764, 0.029458985050419162, 0 },
		{ 1, TEXT("whistle"), 20.0, 0.0, 1.0567488770186901, 0.2070075140381575, 0.16007102028751763, 0 },
		{ 1, TEXT("mg"), 20.0, 0.0, 0.9920164200710133, 0.3821843125584996, -0.3386934309630918, 0 },
		{ 0, TEXT("bomber"), 20.310000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("bomber"), 20.25, 0.0, 1.0095808187918738, 0.5962406824225591, -0.2977272839397619, 0 },
		{ 1, TEXT("plane_flyby"), 20.25, 0.0, 0.9508043025620282, 0.5147618750340458, -0.18147789157836702, 0 },
		{ 1, TEXT("rockets"), 20.25, 0.0, 0.9564333497779444, 0.6200286361935015, 0.0023870851074441627, 0 },
		{ 1, TEXT("bomb"), 20.25, 0.0, 1.0119980549067258, 0.6860342485659658, 0.18683486142417496, 0 },
		{ 1, TEXT("blast_large"), 20.25, 0.0, 0.9908570243045688, 0.4744332597020692, 0.3028780017532878, 0 },
		{ 1, TEXT("tankgun"), 20.25, 0.0, 1.030116240689531, 0.5098698282223824, -0.2053798030950778, 0 },
		{ 1, TEXT("flak"), 20.25, 0.0, 1.0536978396773338, 0.32169647594838197, -0.08518977205639695, 0 },
		{ 1, TEXT("vehicle_destroyed"), 20.25, 0.0, 0.9791057258844376, 0.5996609114849258, -0.23395044918063923, 0 },
		{ 1, TEXT("smoke"), 20.25, 0.0, 0.9136097692418843, 0.30876969199476817, -0.08939655858388243, 0 },
		{ 1, TEXT("sniper"), 20.25, 0.0, 1.00826668927446, 0.4289973395669946, 0.08228272192102586, 0 },
		{ 1, TEXT("atgun"), 20.25, 0.0, 0.9508053528610617, 0.5106215169395594, 0.22201140644034734, 0 },
		{ 1, TEXT("whistle"), 20.25, 0.0, 0.9485234108753502, 0.20465137406003983, -0.302612323262293, 0 },
		{ 1, TEXT("mg"), 20.25, 0.0, 1.0495096870698035, 0.4253522825181438, -0.19224931840045745, 0 },
		{ 0, TEXT("plane_flyby"), 20.560000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("plane_flyby"), 20.5, 0.0, 0.9909068502951414, 0.4943160902113463, 0.048792076893069906, 0 },
		{ 0, TEXT("rockets"), 20.560000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("rockets"), 20.5, 0.0, 1.0003456871723757, 0.48445049054830785, 0.20191691396235983, 0 },
		{ 1, TEXT("bomb"), 20.5, 0.0, 0.9633715397492051, 0.5748140361833658, 0.05497461370414605, 0 },
		{ 1, TEXT("blast_large"), 20.5, 0.0, 1.0290303040761501, 0.5160646465602035, 0.18262804926824294, 0 },
		{ 1, TEXT("tankgun"), 20.5, 0.0, 0.9765171093679964, 0.4996448740127466, -0.3237931949940804, 0 },
		{ 0, TEXT("flak"), 20.560000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("flak"), 20.5, 0.0, 1.0571511106938123, 0.34580158340733186, -0.2188021919864835, 0 },
		{ 0, TEXT("vehicle_destroyed"), 20.560000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("vehicle_destroyed"), 20.5, 0.0, 1.0355705464910716, 0.7473665204844345, -0.053941226482938434, 0 },
		{ 1, TEXT("smoke"), 20.5, 0.0, 1.032568017579615, 0.30084499452300206, 0.14786126952124087, 0 },
		{ 1, TEXT("atgun"), 20.5, 0.0, 0.9730770146753639, 0.3898879232361019, -0.24586775014328122, 0 },
		{ 1, TEXT("whistle"), 20.5, 0.0, 0.9910243422258646, 0.21667646559704437, -0.1278091419278668, 0 },
		{ 0, TEXT("whistle"), 20.53, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("mg"), 20.5, 0.0, 0.9825351933948696, 0.369437657827233, 0.01967630157238056, 0 },
		{ 0, TEXT("plane_flyby"), 20.810000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("whistle"), 20.78, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("plane_flyby"), 20.75, 0.0, 0.9961352228419855, 0.4694356391804032, -0.059841000556898165, 0 },
		{ 0, TEXT("rockets"), 20.810000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("rockets"), 20.75, 0.0, 0.9802727144910023, 0.4879754043966264, 0.11216435459837043, 0 },
		{ 1, TEXT("bomb"), 20.75, 0.0, 1.0207083969563246, 0.57562222778794, 0.2442520244994437, 0 },
		{ 0, TEXT("whistle"), 20.78, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("blast_large"), 20.75, 0.0, 0.905143003212288, 0.5969449975096295, -0.28416959146926357, 0 },
		{ 0, TEXT("smoke"), 20.78, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("tankgun"), 20.75, 0.0, 1.0192316475976257, 0.5569236861375186, -0.16563172784628782, 0 },
		{ 0, TEXT("flak"), 20.810000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("smoke"), 20.78, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("flak"), 20.75, 0.0, 0.9952924272418022, 0.3625245392556019, 0.006151229416162823, 0 },
		{ 0, TEXT("vehicle_destroyed"), 20.810000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("vehicle_destroyed"), 20.75, 0.0, 0.9407624295074493, 0.7536414466372863, 0.17963653533794754, 0 },
		{ 1, TEXT("smoke"), 20.75, 0.0, 1.0555050019640475, 0.2446416173409429, 0.2985843959989544, 0 },
		{ 0, TEXT("atgun"), 20.810000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("smoke"), 20.78, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("atgun"), 20.75, 0.0, 1.0354987965337932, 0.5209398570288557, -0.34231374445796786, 0 },
		{ 1, TEXT("whistle"), 20.75, 0.0, 0.9508007577620446, 0.28414604807209437, -0.24481646584514594, 0 },
		{ 0, TEXT("mg"), 20.810000000000002, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("whistle"), 20.78, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 1, TEXT("mg"), 20.75, 0.0, 0.977953863190487, 0.3440755869261897, -0.0966229636727099, 0 },
		{ 0, TEXT("fire_loop"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("bomb"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("blast_large"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("tankgun"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("flak"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("sniper"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("bomber"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("plane_flyby"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("rockets"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("bomb"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("blast_large"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("tankgun"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("smoke"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("sniper"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("atgun"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("bomb"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("blast_large"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("tankgun"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("vehicle_destroyed"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("plane_flyby"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("rockets"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("bomb"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("blast_large"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("tankgun"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("flak"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("vehicle_destroyed"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("atgun"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("mg"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("amb:wind"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
		{ 0, TEXT("tank_engine"), 22.41, 0.0, 0.0, 0.0, 0.0, 0 },
	};

	// A sink that writes down what the cue selector asked for.
	struct FRecorder : public ITCAudioSink
	{
		struct FCall
		{
			FString Name;
			bool bPos = false;
			FVector2D Pos = FVector2D::ZeroVector;
			FTCAudioPlayOptions Opts;
		};
		TArray<FCall> Calls;
		TArray<int32> BedPoints;
		virtual void Play(const FString& Name, const FVector2D* Pos, const FTCAudioPlayOptions& Opts) override
		{
			FCall C;
			C.Name = Name;
			C.bPos = Pos != nullptr;
			if (Pos) C.Pos = *Pos;
			C.Opts = Opts;
			Calls.Add(C);
		}
		virtual void Bed(const FString& Name, TConstArrayView<FVector2D> Points) override { BedPoints.Add(Points.Num()); }
		virtual double Length(const FString& Name) const override { return Name == TEXT("whistle") ? 1.76 : 0.0; }
		int32 Count(const TCHAR* Name) const { int32 N = 0; for (const FCall& C : Calls) N += C.Name == Name; return N; }
		const FCall* First(const TCHAR* Name) const { for (const FCall& C : Calls) if (C.Name == Name) return &C; return nullptr; }
		void Reset() { Calls.Reset(); BedPoints.Reset(); }
	};

	// a few UNITS entries as the unit table gives them
	static bool TypeOf(const FString& Type, FTCAudioType& Out)
	{
		struct FDef { const TCHAR* Type; bool bInf, bStruct, bAir; float Radius; };
		static const FDef Defs[] = {
			{ TEXT("rifle"), true, false, false, 1.4f }, { TEXT("ranger"), true, false, false, 1.4f }, { TEXT("mg"), true, false, false, 1.2f },
			{ TEXT("mortar"), true, false, false, 1.4f }, { TEXT("tank"), false, false, false, 2.f }, { TEXT("tiger"), false, false, false, 2.6f },
			{ TEXT("flak"), false, false, false, 1.8f }, { TEXT("bunker"), false, true, false, 2.5f }, { TEXT("tower"), false, true, false, 1.5f },
			{ TEXT("fighter"), false, false, true, 3.f }, { TEXT("truck"), false, false, false, 1.6f },
		};
		for (const FDef& D : Defs)
			if (Type == D.Type) { Out.bKnown = true; Out.bInfantry = D.bInf; Out.bStructure = D.bStruct; Out.bAir = D.bAir; Out.Radius = D.Radius; return true; }
		return false;
	}
	static FTCAudioUnit Unit(int32 Id, const TCHAR* Type, double X, double Z, int32 Owner = 1, int32 Alive = 1, int32 Flags = 0)
	{
		FTCAudioUnit U;
		U.Id = Id;
		U.Type = Type;
		U.X = X;
		U.Z = Z;
		U.Owner = Owner;
		U.Alive = Alive;
		U.Flags = Flags;
		FTCAudioType T;
		if (TypeOf(Type, T)) { U.bKnown = true; U.bInfantry = T.bInfantry; U.bStructure = T.bStructure; U.bAir = T.bAir; U.Radius = T.Radius; }
		return U;
	}
	static FTCAudioShot Shot(const TCHAR* K, double X, double Z, TOptional<int32> F = {}, TOptional<int32> T = {}, bool bHit = false)
	{
		FTCAudioShot S;
		S.K = K;
		S.bHasPos = true;
		S.X = X;
		S.Z = Z;
		S.F = F;
		S.T = T;
		S.bHit = bHit;
		S.bHitPresent = bHit;
		return S;
	}
	static void Run(FTCAudioCues& Cues, FRecorder& Rec, const TMap<int32, FTCAudioUnit>& Units, const TArray<FTCAudioShot>& Shots, int32 Tick = 0,
		TConstArrayView<FTCAudioStrike> Strikes = {}, TConstArrayView<FTCAudioSmoke> Smokes = {}, TConstArrayView<FTCAudioFire> Fires = {},
		TFunction<bool(double, double)> Opened = nullptr)
	{
		FTCAudioSnapshot S;
		S.Tick = Tick;
		S.Shots = Shots;
		S.Units = &Units;
		S.TypeOf = &TypeOf;
		S.Strikes = Strikes;
		S.Smokes = Smokes;
		S.Fires = Fires;
		S.OpenedCell = MoveTemp(Opened);
		Cues.Snapshot(S, Rec);
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAudioMixerOracleTest, "ThreeCrossroads.Audio.MixerOracle", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCAudioMixerOracleTest::RunTest(const FString& Parameters)
{
	using namespace TCAudioTest;
	// client/audio.js's own decisions (gaps, voices per effect, the 32-voice cap, quietest-first stealing, pitch and
	// level spread, boost, place(), voice lines and their gap, alert gaps and the voice 600 ms after, ui(), loops,
	// match_start, beds, end()) against the mixer, from the same scenario and the same random numbers
	FTCAudioMixer M;
	FTCAudioMulberry R{ OracleSeed };
	M.SetRandom([&R]() { return R.Next(); });
	TArray<FTCAudioTraceRow> Trace;
	M.Trace = &Trace;
	M.SetRunning(true);
	for (const TCAudioRules::FClip& C : TCAudioRules::Clips()) M.SetLoaded(C.Id, C.Dur);
	M.InitVolume(TOptional<double>());
	for (const FTCAudioOracleOp& O : OracleOps)
	{
		M.AdvanceTo(O.T);
		const FString Op = O.Op;
		const FVector2D P(O.X, O.Z);
		if (Op == TEXT("start"))
		{
			const int32 Token = M.BeginMatch((int32)O.A, (int32)O.B);
			FString Folder;
			const FString Speaker = M.ChooseVoice(Token, Folder);
			for (const FString& Stem : TCAudioRules::VoiceStems(Speaker)) M.SetLoaded(TEXT("voice:") + Stem, OracleVoiceSeconds);
			M.FinishLoading(Token);
		}
		else if (Op == TEXT("listener")) M.Listener(O.X, O.Z, O.A, O.B);
		else if (Op == TEXT("play"))
		{
			FTCAudioPlayOptions Opts;
			if (O.A) Opts.Delay = O.A;
			if (O.B) Opts.Dur = O.B;
			if (O.C) Opts.Gain = O.C;
			M.PlaySound(O.Name, O.HasPos ? &P : nullptr, Opts);
		}
		else if (Op == TEXT("voice")) M.Voice(O.Name);
		else if (Op == TEXT("alert")) M.Alert(O.Name);
		else if (Op == TEXT("ui")) M.Ui(O.Name);
		else if (Op == TEXT("bed"))
		{
			const FVector2D Pts[] = { P, FVector2D(O.A, O.B) };
			M.Bed(O.Name, Pts);
		}
		else if (Op == TEXT("end")) M.End();
	}
	TestEqual(TEXT("as many starts and stops as client/audio.js"), Trace.Num(), (int32)UE_ARRAY_COUNT(OracleRows));
	for (int32 I = 0; I < FMath::Min(Trace.Num(), (int32)UE_ARRAY_COUNT(OracleRows)); I++)
	{
		const FTCAudioTraceRow& A = Trace[I];
		const FTCAudioOracleRow& E = OracleRows[I];
		const FString L = FString::Printf(TEXT("row %d (%s %s at %.4f)"), I, E.Start ? TEXT("start") : TEXT("stop"), E.Key, E.At);
		if (!TestEqual(L + TEXT(": kind and file"), FString::Printf(TEXT("%d %s"), A.bStart ? 1 : 0, *A.Key), FString::Printf(TEXT("%d %s"), E.Start, E.Key))) break;
		TestEqual(L + TEXT(": time"), A.At, E.At, 1e-9);
		if (!E.Start) continue;
		TestEqual(L + TEXT(": offset"), A.Offset, E.Offset, 1e-9);
		TestEqual(L + TEXT(": rate"), A.Rate, E.Rate, 1e-9);
		TestEqual(L + TEXT(": gain"), A.Gain, E.Gain, 1e-9);
		TestEqual(L + TEXT(": pan"), A.Pan, E.Pan, 1e-9);
		TestEqual(L + TEXT(": loop"), A.bLoop ? 1 : 0, E.Loop);
	}
	TestEqual(TEXT("the speaker: seat 1 of the UK speaks as the second American"), M.VoiceName(), FString(TEXT("harry")));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAudioMixerRulesTest, "ThreeCrossroads.Audio.MixerRules", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCAudioMixerRulesTest::RunTest(const FString& Parameters)
{
	using namespace TCAudioRules;
	// place(): full level at the focus, nothing at the range, quieter as the camera pulls back, right is positive
	{
		FEar E;
		E.Dist = 25.0;
		const FVector2D At(0, 0), Far(150, 0), Right(10, 0);
		TestEqual(TEXT("place: at the focus"), Place(E, &At, 150).Gain, 1.0, 1e-12);
		TestEqual(TEXT("place: at the range"), Place(E, &Far, 150).Gain, 0.0, 1e-12);
		TestEqual(TEXT("place: pan to the right"), Place(E, &Right, 150).Pan, 10.0 / 35.0 * 0.8, 1e-12);
		E.Dist = 150.0;
		TestEqual(TEXT("place: pulled back 35% quieter"), Place(E, &At, 150).Gain, 0.65, 1e-12);
		TestEqual(TEXT("place: no position"), Place(E, nullptr, 150).Gain, 1.0, 1e-12);
		TestEqual(TEXT("browser yaw of this camera's -90"), BrowserYaw(-90.0), 0.0, 1e-12);
	}
	// voices: folders, speakers by seat, the UK speaks American, file names as index.json lists them
	TestEqual(TEXT("UK folder"), FString(VoiceFolder(3)), FString(TEXT("us")));
	TestEqual(TEXT("unknown faction folder"), FString(VoiceFolder(7)), FString(TEXT("us")));
	TestEqual(TEXT("Germany folder"), FString(VoiceFolder(1)), FString(TEXT("de")));
	TestEqual(TEXT("files per speaker (48 per folder)"), VoiceStems(TEXT("adam")).Num(), 24);
	TestEqual(TEXT("a file stem"), VoiceStem(TEXT("callum"), TEXT("retreat"), 1, 0), FString(TEXT("callum_retreat2_t1")));
	FTCAudioMixer M;
	M.SetRandom([]() { return 0.5; }); // rnd(a) = 1: no spread
	M.SetRunning(true);
	for (const FClip& C : Clips()) M.SetLoaded(C.Id, C.Dur);
	TestEqual(TEXT("quiet before a match"), M.PlaySound(TEXT("rifle"), nullptr), 0);
	{
		const int32 Token = M.BeginMatch(1, 3);
		FString Folder;
		TestEqual(TEXT("German seat 3"), M.ChooseVoice(Token, Folder), FString(TEXT("daniel")));
		M.AdvanceTo(5.0);
		M.FinishLoading(Token);
		TestEqual(TEXT("no match_start after a 5 s load"), M.LivePlaying(), 0);
		const int32 Stale = M.BeginMatch(0, 0);
		M.BeginMatch(0, 0);
		M.FinishLoading(Stale);
		TestEqual(TEXT("a stale load starts nothing"), M.LivePlaying(), 0);
	}
	{
		// boost: blast_small's 10 dB comes back at most 6 dB
		const FVector2D At(0, 0);
		M.Listener(0, 0, 25, 0);
		const int32 H = M.PlaySound(TEXT("blast_small"), &At);
		const FTCAudioSound* P = M.Sounds().FindByPredicate([H](const FTCAudioSound& S) { return S.Handle == H; });
		TestTrue(TEXT("blast plays"), P != nullptr);
		if (P) TestEqual(TEXT("blast level: gain 0.6, boost capped at 6 dB"), P->Level, 0.6 * FMath::Pow(10.0, 6.0 / 20.0), 1e-12);
		M.SetRunning(false);
		TestEqual(TEXT("no device: quiet"), M.PlaySound(TEXT("rifle"), &At), 0);
		M.SetRunning(true);
	}
	{
		// a loop with its own fade (a wreck's fire): up over 0.6 s, down over its last 1.5 s
		const FVector2D At(0, 0);
		FTCAudioPlayOptions O;
		O.Delay = 0.8;
		O.Dur = 14.0;
		const int32 H = M.PlaySound(TEXT("fire_loop"), &At, O);
		const FTCAudioSound* P = M.Sounds().FindByPredicate([H](const FTCAudioSound& S) { return S.Handle == H; });
		TestTrue(TEXT("fire loop plays"), P != nullptr);
		if (P)
		{
			TestEqual(TEXT("fire: silent before its delay"), P->Envelope(P->StartAt - 0.1), 0.0, 1e-12);
			TestEqual(TEXT("fire: half way up"), P->Envelope(P->StartAt + 0.3), 0.5, 1e-12);
			TestEqual(TEXT("fire: fading"), P->Envelope(P->StartAt + 13.3), 1.0 - 0.8 / 1.5, 1e-12);
			TestEqual(TEXT("fire: stops 0.05 s after its 14 s"), P->EndAt - P->StartAt, 14.05, 1e-12);
		}
	}
	{
		// ducking: an alert pulls the ambience bus toward 0.32, which comes back after it
		M.Alert(TEXT("pointWon"));
		M.Advance(0.3);
		TestTrue(TEXT("ambience ducked"), M.BusGain(ETCAudioBus::Amb) < 0.4);
		M.Advance(4.0);
		TestTrue(TEXT("ambience back"), M.BusGain(ETCAudioBus::Amb) > 0.95);
		TestEqual(TEXT("voice bus"), M.BusGain(ETCAudioBus::Voice), 1.2, 1e-12);
	}
	{
		// volume: 80% by default, M mutes and comes back to the volume before muting, the gain follows the square
		FTCAudioMixer V;
		V.InitVolume(TOptional<double>());
		TestEqual(TEXT("default volume"), V.Volume(), 0.8, 1e-12);
		TestTrue(TEXT("bind saves the starting volume"), V.TakeVolumeChanged());
		V.ToggleMute();
		TestEqual(TEXT("muted"), V.Volume(), 0.0, 1e-12);
		V.ToggleMute();
		TestEqual(TEXT("unmuted to 80%"), V.Volume(), 0.8, 1e-12);
		V.SetVolume(0.5);
		V.ToggleMute();
		V.ToggleMute();
		TestEqual(TEXT("unmuted to 50%"), V.Volume(), 0.5, 1e-12);
		V.Advance(1.0);
		TestEqual(TEXT("master gain is the square"), V.MasterGain(), 0.25, 1e-9);
		V.SetVolume(2.0);
		TestEqual(TEXT("clamped"), V.Volume(), 1.0, 1e-12);
		V.SetVolume(std::numeric_limits<double>::quiet_NaN());
		TestEqual(TEXT("not a number: 0"), V.Volume(), 0.0, 1e-12);
		V.InitVolume(TOptional<double>(0.35));
		TestEqual(TEXT("saved volume"), V.Volume(), 0.35, 1e-12);
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAudioSavedVolumeTest, "ThreeCrossroads.Audio.SavedVolume", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCAudioSavedVolumeTest::RunTest(const FString&)
{
	// audio.js firstVolume: a finite parseFloat volume wins; otherwise the read-only old mute key wins over 80%.
	// Use the same preference-to-mixer adapter as UTCAudio startup, without changing real user preferences.
	FTCSettingsStore Preferences{ FString() };
	FTCAudioMixer M;
	M.InitPreferences(Preferences);
	TestEqual(TEXT("no saved preference defaults to 80 percent"), M.Volume(), 0.8, 1e-12);
	Preferences.Set(TEXT("ww2-muted"), TEXT("1"));
	M.InitPreferences(Preferences);
	TestEqual(TEXT("legacy muted preference is consumed at startup"), M.Volume(), 0.0, 1e-12);
	Preferences.Set(TEXT("ww2-volume"), TEXT(" .35suffix"));
	M.InitPreferences(Preferences);
	TestEqual(TEXT("numeric prefix overrides the older mute preference"), M.Volume(), 0.35, 1e-12);
	M.ToggleMute(); M.ToggleMute();
	TestEqual(TEXT("M restores the consumed volume"), M.Volume(), 0.35, 1e-12);
	Preferences.Set(TEXT("ww2-volume"), TEXT("invalid")); M.InitPreferences(Preferences);
	TestEqual(TEXT("invalid volume retains the legacy mute fallback"), M.Volume(), 0.0, 1e-12);
	Preferences.Remove(TEXT("ww2-muted")); M.InitPreferences(Preferences);
	TestEqual(TEXT("invalid volume without legacy mute defaults to 80 percent"), M.Volume(), 0.8, 1e-12);
	Preferences.Set(TEXT("ww2-volume"), TEXT("3")); M.InitPreferences(Preferences);
	TestEqual(TEXT("stored volume is clamped"), M.Volume(), 1.0, 1e-12);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAudioCuesTest, "ThreeCrossroads.Audio.Cues", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCAudioCuesTest::RunTest(const FString& Parameters)
{
	using namespace TCAudioTest;
	// Expected cues read off client/fx.js, client/main.js, client/projectiles.js and client/battle-sound.js at the pin
	// (the reference code paths named per case); these modules need three.js and a WebGL scene, so they are not run.
	FRecorder Rec;
	auto Near = [](const FRecorder::FCall* C, double X, double Z, double Tol) { return C && C->bPos && FMath::Abs(C->Pos.X - X) <= Tol && FMath::Abs(C->Pos.Y - Z) <= Tol; };
	{
		// directFire, a seen rifle squad of three: one crack per soldier, each with his flash (delayed up to 0.18 s), at
		// the shooter; the second and third at 0.8
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 1, Unit(1, TEXT("rifle"), 10, 10, 1, 3) }, { 2, Unit(2, TEXT("rifle"), 50, 50, 2) } };
		Run(Q, Rec, U, { Shot(TEXT("rifle"), 50, 50, 1, 2) });
		TestEqual(TEXT("rifle: nothing before the flashes"), Rec.Calls.Num(), 0);
		Q.Frame(0.2, Rec);
		TestEqual(TEXT("rifle: three cracks"), Rec.Count(TEXT("rifle")), 3);
		TestEqual(TEXT("rifle: nothing else (dirt hits are silent)"), Rec.Calls.Num(), 3);
		int32 Quieter = 0;
		for (const FRecorder::FCall& C : Rec.Calls) { TestTrue(TEXT("rifle: at the shooter"), Near(&C, 10, 10, 1e-9)); Quieter += C.Opts.Gain == 0.8; }
		TestEqual(TEXT("rifle: two of them at 0.8"), Quieter, 2);
		Q.Frame(1.0, Rec);
		TestEqual(TEXT("rifle: the impacts are silent"), Rec.Calls.Num(), 3);
	}
	{
		// privacy: the shooter is not in this snapshot (no f): one crack where the rounds land, nothing at the shooter
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 2, Unit(2, TEXT("rifle"), 50, 50, 2) } };
		Run(Q, Rec, U, { Shot(TEXT("rifle"), 50, 50, {}, 2) });
		TestEqual(TEXT("hidden rifle: one crack"), Rec.Count(TEXT("rifle")), 1);
		TestTrue(TEXT("hidden rifle: at the delivered shot"), Near(Rec.First(TEXT("rifle")), 50, 50, 1e-9));
		Q.Frame(1.0, Rec);
		TestEqual(TEXT("hidden rifle: nothing more"), Rec.Calls.Num(), 1);
		// a shooter id the snapshot does not hold counts as hidden too
		Rec.Reset();
		Run(Q, Rec, U, { Shot(TEXT("rifle"), 50, 50, 77, 2) });
		TestTrue(TEXT("unknown shooter: at the shot"), Near(Rec.First(TEXT("rifle")), 50, 50, 1e-9));
	}
	{
		// a garrisoned squad shows no men: no muzzle, so no rifle crack
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 1, Unit(1, TEXT("rifle"), 10, 10, 1, 3, TCFlags::Garrisoned) } };
		Run(Q, Rec, U, { Shot(TEXT("rifle"), 50, 50, 1) });
		Q.Frame(1.0, Rec);
		TestEqual(TEXT("garrisoned rifles: silent"), Rec.Calls.Num(), 0);
	}
	{
		// an MG recording is a whole burst: once per 1.25 s per shooter
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 3, Unit(3, TEXT("mg"), 0, 0) } };
		Run(Q, Rec, U, { Shot(TEXT("mg"), 30, 0, 3) });
		Q.Frame(0.5, Rec);
		Run(Q, Rec, U, { Shot(TEXT("mg"), 30, 0, 3) });
		TestEqual(TEXT("mg: one burst within 1.25 s"), Rec.Count(TEXT("mg")), 1);
		Q.Frame(0.8, Rec);
		Run(Q, Rec, U, { Shot(TEXT("mg"), 30, 0, 3) });
		TestEqual(TEXT("mg: the next burst after 1.25 s"), Rec.Count(TEXT("mg")), 2);
	}
	{
		// a tank gun at the seen shooter, its shell bursting at the far end after the tracer's flight; the Tiger lower
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 5, Unit(5, TEXT("tank"), 0, 0) }, { 6, Unit(6, TEXT("tank"), 100, 0, 2) }, { 7, Unit(7, TEXT("tiger"), 0, 20) } };
		Run(Q, Rec, U, { Shot(TEXT("tank"), 100, 0, 5, 6, true), Shot(TEXT("tiger"), 100, 0, 7, 6, true) });
		TestTrue(TEXT("tank gun at the shooter"), Near(Rec.First(TEXT("tankgun")), 0, 0, 1e-9));
		TestEqual(TEXT("two gun reports"), Rec.Count(TEXT("tankgun")), 2);
		TestEqual(TEXT("the Tiger at rate 0.85"), Rec.Calls.Num() == 2 ? Rec.Calls[1].Opts.Rate : 0.0, 0.85, 1e-12);
		Q.Frame(0.5, Rec);
		TestEqual(TEXT("no burst before the shell arrives (about 0.65 s)"), Rec.Count(TEXT("blast_small")), 0);
		Q.Frame(0.5, Rec);
		TestEqual(TEXT("both shells burst"), Rec.Count(TEXT("blast_small")), 2);
		TestTrue(TEXT("burst at the target, short of its centre"), Near(Rec.First(TEXT("blast_small")), 98.5, 0, 1.5));
	}
	{
		// rifles hitting armor ricochet (one per 0.3 s); a ranger's bazooka on a vehicle is an AT gun with a blast
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 1, Unit(1, TEXT("rifle"), 0, 0, 1, 3) }, { 4, Unit(4, TEXT("ranger"), 0, 5, 1, 3) }, { 6, Unit(6, TEXT("tank"), 40, 0, 2) } };
		Run(Q, Rec, U, { Shot(TEXT("rifle"), 40, 0, 1, 6, true) });
		Q.Frame(1.0, Rec);
		TestEqual(TEXT("ricochet: one within its gap"), Rec.Count(TEXT("ricochet")), 1);
		Rec.Reset();
		Run(Q, Rec, U, { Shot(TEXT("ranger"), 40, 0, 4, 6, true) });
		TestEqual(TEXT("bazooka: an AT gun report"), Rec.Count(TEXT("atgun")), 1);
		TestEqual(TEXT("bazooka: not the SMG"), Rec.Count(TEXT("smg")), 0);
		Q.Frame(1.5, Rec);
		TestEqual(TEXT("bazooka: one rocket, one blast"), Rec.Count(TEXT("blast_small")), 1);
	}
	{
		// main.js air shots: support planes, shoot-downs and flak; flak at a target is a ground gun (fx.js)
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 8, Unit(8, TEXT("flak"), 0, 0) }, { 6, Unit(6, TEXT("tank"), 40, 0, 2) } };
		Run(Q, Rec, U, { Shot(TEXT("strafe"), 10, 10), Shot(TEXT("bombing"), 20, 20), Shot(TEXT("planedown"), 30, 30), Shot(TEXT("chutes"), 5, 5), Shot(TEXT("flak"), 40, 0, 8, 6) });
		TestEqual(TEXT("strafe: plane_flyby"), Rec.Count(TEXT("plane_flyby")), 1);
		TestEqual(TEXT("bombing: bomber"), Rec.Count(TEXT("bomber")), 1);
		TestEqual(TEXT("plane down and the flak gun: flak"), Rec.Count(TEXT("flak")), 2);
		TestEqual(TEXT("chutes: silent"), Rec.Calls.Num(), 4);
	}
	{
		// projectiles.js: a launch within 0.2 s, once; a terminal shell contact bursts; a bullet's is silent; a launch
		// whose muzzle is out of view (no f) is silent
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U;
		FTCAudioShot Launch = Shot(TEXT("flight"), 5, 5, 9);
		Launch.Kind = TEXT("mortar");
		Launch.Flight = 11;
		Launch.Time = 4.9;
		Launch.bLocal = true;
		FTCAudioShot Old = Launch;
		Old.Flight = 12;
		Old.Time = 4.7;
		FTCAudioShot Hidden = Launch;
		Hidden.Flight = 13;
		Hidden.F.Reset();
		Run(Q, Rec, U, { Launch, Old, Hidden }, 100);
		TestEqual(TEXT("launch: the mortar once"), Rec.Count(TEXT("mortar")), 1);
		TestEqual(TEXT("launch: nothing else"), Rec.Calls.Num(), 1);
		Run(Q, Rec, U, { Launch }, 101);
		TestEqual(TEXT("launch: not twice"), Rec.Count(TEXT("mortar")), 1);
		FTCAudioShot Contact = Shot(TEXT("contact"), 60, 0);
		Contact.Kind = TEXT("tank");
		Contact.Flight = 11;
		Contact.Seq = 1;
		Contact.Time = 5.0;
		Contact.bTerminal = true;
		Contact.bLocal = true;
		FTCAudioShot Bullet = Contact;
		Bullet.Kind = TEXT("rifle");
		Bullet.Flight = 14;
		Run(Q, Rec, U, { Contact, Bullet }, 101);
		TestEqual(TEXT("contact: one burst"), Rec.Count(TEXT("blast_small")), 1);
		Run(Q, Rec, U, { Contact }, 102);
		TestEqual(TEXT("contact: once"), Rec.Count(TEXT("blast_small")), 1);
	}
	{
		// salvos: a physical one (every salvo the pinned server sends) is the projectiles' business; a seen mortar's
		// bombs leave 0.1 s apart and each whistle ends as its bomb lands; an unseen launcher's rockets whistle once
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 9, Unit(9, TEXT("mortar"), 0, 0, 1, 3) } };
		FTCAudioShot Physical = Shot(TEXT("salvo"), 60, 0, 9);
		Physical.bLocal = Physical.bPhysical = true;
		Physical.N = 3;
		FTCAudioShot Seen = Physical;
		Seen.bLocal = Seen.bPhysical = false;
		Run(Q, Rec, U, { Physical });
		Q.Frame(3.0, Rec);
		TestEqual(TEXT("physical salvo: silent here"), Rec.Calls.Num(), 0);
		Run(Q, Rec, U, { Seen });
		Q.Frame(0.05, Rec);
		TestEqual(TEXT("mortar: the first bomb and its whistle at once"), Rec.Calls.Num(), 2);
		Q.Frame(0.3, Rec);
		TestEqual(TEXT("mortar: three bombs"), Rec.Count(TEXT("mortar")), 3);
		TestEqual(TEXT("mortar: three whistles"), Rec.Count(TEXT("whistle")), 3);
		const FRecorder::FCall* W = Rec.First(TEXT("whistle"));
		TestTrue(TEXT("whistle: 0.26 s in (1.76 s file, 1.5 s flight), at 0.7, where the bombs land"), W && FMath::IsNearlyEqual(W->Opts.Offset, 0.26, 1e-9) && FMath::IsNearlyEqual(W->Opts.Gain, 0.7, 1e-9) && Near(W, 60, 0, 1e-9));
		Rec.Reset();
		FTCAudioShot Rockets = Shot(TEXT("salvo"), 80, 0);
		Run(Q, Rec, U, { Rockets });
		Q.Frame(0.05, Rec);
		TestEqual(TEXT("unseen rockets: one whistle"), Rec.Count(TEXT("whistle")), 1);
		TestTrue(TEXT("unseen rockets: 0.56 s in"), Rec.Calls.Num() == 1 && FMath::IsNearlyEqual(Rec.Calls[0].Opts.Offset, 0.56, 1e-9) && Rec.Calls[0].Opts.Gain == 1.0);
	}
	{
		// main.js removeUnit: a killed vehicle wrecks (vehicle_destroyed, a 14 s fire 0.8 s later), a killed bunker
		// collapses, a killed tower topples and crashes 6 m away about 1 s later; a unit that only left the view, or
		// killed infantry, is silent
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 1, Unit(1, TEXT("tank"), 10, 0) }, { 2, Unit(2, TEXT("tank"), 20, 0) }, { 3, Unit(3, TEXT("bunker"), 30, 0) },
			{ 4, Unit(4, TEXT("tower"), 40, 0) }, { 5, Unit(5, TEXT("rifle"), 50, 0) } };
		Run(Q, Rec, U, {});
		FTCAudioShot K1 = Shot(TEXT("hurt"), 10, 0, {}, 1), K3 = Shot(TEXT("hurt"), 30, 0, {}, 3), K4 = Shot(TEXT("collapse"), 40, 0, {}, 4), K5 = Shot(TEXT("hurt"), 50, 0, {}, 5);
		K1.bKill = K3.bKill = K5.bKill = true;
		K4.Id = TEXT("section-4"); // a building section's collapse, not a map cell
		Run(Q, Rec, U, { K1, K3, K4, K5 });
		TestEqual(TEXT("kills: nothing until the units go"), Rec.Calls.Num(), 0);
		Run(Q, Rec, TMap<int32, FTCAudioUnit>(), {});
		TestTrue(TEXT("wreck: vehicle_destroyed at the tank"), Near(Rec.First(TEXT("vehicle_destroyed")), 10, 0, 1e-9));
		const FRecorder::FCall* Fire = Rec.First(TEXT("fire_loop"));
		TestTrue(TEXT("wreck: a 14 s fire 0.8 s later"), Fire && Fire->Opts.Delay == 0.8 && Fire->Opts.Dur == 14.0);
		TestEqual(TEXT("one wreck: the tank that left the view is silent"), Rec.Count(TEXT("vehicle_destroyed")), 1);
		TestTrue(TEXT("bunker: collapses"), Near(Rec.First(TEXT("collapse")), 30, 0, 1e-9));
		TestEqual(TEXT("bunker and nothing for the rifles"), Rec.Calls.Num(), 3);
		Q.Frame(0.9, Rec);
		TestEqual(TEXT("tower: still toppling"), Rec.Count(TEXT("collapse")), 1);
		Q.Frame(0.2, Rec);
		TestEqual(TEXT("tower: crashed"), Rec.Count(TEXT("collapse")), 2);
		const FRecorder::FCall& T = Rec.Calls.Last();
		TestEqual(TEXT("tower: 6 m from its base"), FVector2D::Distance(T.Pos, FVector2D(40, 0)), 6.0, 1e-9);
	}
	{
		// a map cell's collapse: a hedge, wire or tree that became open ground only crunches (0.3, rate 1.3)
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U;
		Run(Q, Rec, U, { Shot(TEXT("collapse"), 10, 10), Shot(TEXT("collapse"), 30, 30) }, 0, {}, {}, {}, [](double X, double Z) { return X < 20; });
		TestEqual(TEXT("collapse: two"), Rec.Count(TEXT("collapse")), 2);
		TestTrue(TEXT("collapse: the opened cell is light"), Rec.Calls.Num() == 2 && Rec.Calls[0].Opts.Gain == 0.3 && Rec.Calls[0].Opts.Rate == 1.3);
		TestTrue(TEXT("collapse: a house is full"), Rec.Calls.Num() == 2 && Rec.Calls[1].Opts.Gain == 1.0 && Rec.Calls[1].Opts.Rate == 1.0);
	}
	{
		// fx.js syncStrikes: an artillery strike whistles once, timed to land; a strike far off waits
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U;
		const FTCAudioStrike Soon[] = { { TEXT("artillery"), 50.5, 20, 1.0 }, { TEXT("artillery"), 10, 10, 3.0 }, { TEXT("strafe"), 0, 0, 0.5 } };
		Run(Q, Rec, U, {}, 0, Soon);
		Q.Frame(0.01, Rec);
		TestEqual(TEXT("strike: one whistle"), Rec.Count(TEXT("whistle")), 1);
		TestTrue(TEXT("strike: 0.76 s into the file"), Rec.Calls.Num() == 1 && FMath::IsNearlyEqual(Rec.Calls[0].Opts.Offset, 0.76, 1e-9));
		Run(Q, Rec, U, {}, 1, Soon);
		Q.Frame(0.01, Rec);
		TestEqual(TEXT("strike: not again"), Rec.Count(TEXT("whistle")), 1);
	}
	{
		// fx.js syncClouds: a new smoke screen pops (one per 0.15 s); a cloud over a fire is that fire's smoke
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U;
		FTCAudioSmoke A{ 10, 10, 7, 1 }, B{ 60, 60, 7, 2 }, C{ 100, 100, 7, 3 };
		const FTCAudioFire Fires[] = { { 102, 101, 'O' }, { 11, 12, '.' } };
		const FTCAudioSmoke First[] = { A };
		Run(Q, Rec, U, {}, 0, {}, First, Fires);
		TestEqual(TEXT("smoke: a new screen"), Rec.Count(TEXT("smoke")), 1);
		const FTCAudioSmoke Next[] = { A, B, C };
		Run(Q, Rec, U, {}, 1, {}, Next, Fires);
		TestEqual(TEXT("smoke: inside 0.15 s, and over a fire: silent"), Rec.Count(TEXT("smoke")), 1);
		Q.Frame(0.2, Rec);
		const FTCAudioSmoke Later[] = { A, B, C, { 140, 10, 5, 4 } };
		Run(Q, Rec, U, {}, 2, {}, Later, Fires);
		TestEqual(TEXT("smoke: the next new one"), Rec.Count(TEXT("smoke")), 2);
	}
	{
		// battle-sound.js: moving vehicles feed the engine bed; a digging squad is heard every 1.2 to 2 s, quieter
		// when it is not ours; checks run every 0.2 s
		Rec.Reset();
		FTCAudioCues Q;
		TMap<int32, FTCAudioUnit> U = { { 1, Unit(1, TEXT("tank"), 0, 0) }, { 2, Unit(2, TEXT("rifle"), 5, 5, 0, 3, TCAudioRules::DigFlag) },
			{ 3, Unit(3, TEXT("rifle"), 9, 5, 1, 3, TCAudioRules::BuildFlag) } };
		Q.Battle(1.0, U, 0, Rec);
		TestEqual(TEXT("battle: first sight is not moving"), Rec.BedPoints.Num() == 1 ? Rec.BedPoints[0] : -1, 0);
		TestTrue(TEXT("battle: our diggers at full level"), Rec.First(TEXT("dig")) && Rec.First(TEXT("dig"))->Opts.Gain == 1.0);
		TestTrue(TEXT("battle: their builders at 0.7"), Rec.First(TEXT("build")) && Rec.First(TEXT("build"))->Opts.Gain == 0.7);
		U[1].X = 1.0;
		Q.Battle(1.1, U, 0, Rec);
		TestEqual(TEXT("battle: inside 0.2 s nothing"), Rec.BedPoints.Num(), 1);
		Q.Battle(1.25, U, 0, Rec);
		TestEqual(TEXT("battle: the moving tank in the bed"), Rec.BedPoints.Num() == 2 ? Rec.BedPoints[1] : -1, 1);
		TestEqual(TEXT("battle: no second dig before 1.2 s"), Rec.Count(TEXT("dig")), 1);
		Q.Battle(3.5, U, 0, Rec);
		TestEqual(TEXT("battle: the next dig"), Rec.Count(TEXT("dig")), 2);
	}
	{
		// the adapter from the decoder: f and t only when delivered; salvos and throws marked local are physical
		FTCShot S;
		S.K = TEXT("salvo");
		S.bHasPos = true;
		S.bLocal = true;
		S.bHasShooter = false;
		S.F = 12;
		const FTCAudioShot A = FTCAudioShot::From(S);
		TestFalse(TEXT("adapter: no shooter when not delivered"), A.F.IsSet());
		TestTrue(TEXT("adapter: a local salvo is physical"), A.bPhysical);
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAudioLibraryTest, "ThreeCrossroads.Audio.Library", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCAudioLibraryTest::RunTest(const FString& Parameters)
{
	using namespace TCAudioRules;
	TestEqual(TEXT("35 effects"), Clips().Num(), 35);
	for (const TCHAR* Name : { TEXT("ui_click"), TEXT("ui_recruit"), TEXT("ui_error"), TEXT("alert_attack"), TEXT("alert_point_won"), TEXT("alert_point_lost"),
		TEXT("alert_unit_lost"), TEXT("alert_air"), TEXT("alert_ready"), TEXT("match_start"), TEXT("tank_engine"), TEXT("fire_loop") })
		TestNotNull(*FString::Printf(TEXT("effect %s"), Name), FindClip(Name));
	// Resolve every original sound at its production path, even when a package has no loose reference/manifest JSON.
	// Asset resolution is distinct from the async selected-bank path and from audible output.
	for (const FClip& C : Clips())
	{
		USoundWave* W = LoadObject<USoundWave>(nullptr, *SfxAssetPath(C.Id));
		if (!TestNotNull(FString::Printf(TEXT("effect asset %s loads"), C.Id), W)) continue;
		TestEqual(FString::Printf(TEXT("%s asset length"), C.Id), (double)W->Duration, C.Dur, 0.002);
		TestEqual(FString::Printf(TEXT("%s asset looping"), C.Id), (bool)W->bLooping, C.bLoop);
	}
	for (const TCHAR* Folder : { TEXT("us"), TEXT("de"), TEXT("ru") })
		for (const FString& Speaker : TCAudioRules::Speakers(Folder))
			for (const FString& Stem : VoiceStems(Speaker))
			{
				const FString Path = VoiceAssetPath(Folder, Stem);
				USoundWave* W = LoadObject<USoundWave>(nullptr, *Path);
				if (!TestNotNull(Path + TEXT(" loads"), W)) continue;
				TestTrue(Path + TEXT(" has a finite positive duration"), FMath::IsFinite(W->Duration) && W->Duration > 0);
				TestFalse(Path + TEXT(" does not loop"), W->bLooping);
			}
	// the embedded tables against the reference's own client/audio/index.json, when the checkout is next to the project
	const FString IndexFile = FPaths::Combine(FPaths::ProjectDir(), TEXT("../reference/ww2-rts/client/audio/index.json"));
	FString Body;
	if (!FFileHelper::LoadFileToString(Body, *IndexFile))
	{
		AddWarning(FString::Printf(TEXT("no reference checkout at %s: index.json not compared"), *IndexFile));
	}
	else
	{
		TSharedPtr<FJsonObject> Root;
		const TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Body);
		if (TestTrue(TEXT("index.json parses"), FJsonSerializer::Deserialize(Reader, Root) && Root.IsValid()))
		{
			const TSharedPtr<FJsonObject> Sfx = Root->GetObjectField(TEXT("sfx"));
			TestEqual(TEXT("index.json effects"), Sfx->Values.Num(), Clips().Num());
			for (const FClip& C : Clips())
			{
				const TSharedPtr<FJsonObject>* E = nullptr;
				if (!TestTrue(FString::Printf(TEXT("index.json has %s"), C.Id), Sfx->TryGetObjectField(C.Id, E))) continue;
				double Dur = 0, Boost = 0;
				bool bLoop = false;
				(*E)->TryGetNumberField(TEXT("dur"), Dur);
				(*E)->TryGetNumberField(TEXT("boost"), Boost);
				(*E)->TryGetBoolField(TEXT("loop"), bLoop);
				TestEqual(FString::Printf(TEXT("%s dur"), C.Id), C.Dur, Dur, 1e-12);
				TestEqual(FString::Printf(TEXT("%s boost"), C.Id), C.Boost, Boost, 1e-12);
				TestEqual(FString::Printf(TEXT("%s loop"), C.Id), C.bLoop, bLoop);
			}
			const TSharedPtr<FJsonObject> Voice = Root->GetObjectField(TEXT("voice"));
			TArray<FString> Folders;
			for (const auto& KV : Voice->Values) Folders.Add(FString(*KV.Key));
			Folders.Sort();
			TestEqual(TEXT("voice folders (no uk)"), FString::Join(Folders, TEXT(",")), FString(TEXT("de,ru,us")));
			for (const FString& F : Folders)
			{
				const TSharedPtr<FJsonObject> Speakers = Voice->GetObjectField(F);
				TArray<FString> Names;
				for (const auto& KV : Speakers->Values) Names.Add(FString(*KV.Key));
				Names.Sort();
				TestEqual(F + TEXT(" speakers"), FString::Join(Names, TEXT(",")), FString::Join(TCAudioRules::Speakers(F), TEXT(",")));
				for (const FString& S : Names)
				{
					TArray<FString> Listed;
					for (const auto& Kind : Speakers->GetObjectField(S)->Values)
					{
						const FString KindName(*Kind.Key);
						TestEqual(S + TEXT(" ") + KindName + TEXT(" lines"), Kind.Value->AsArray().Num(), LineCount(KindName));
						for (const TSharedPtr<FJsonValue>& Line : Kind.Value->AsArray())
							for (const TSharedPtr<FJsonValue>& Take : Line->AsArray()) Listed.Add(Take->AsString().LeftChop(4));
					}
					TArray<FString> Ours = VoiceStems(S);
					Listed.Sort();
					Ours.Sort();
					TestEqual(S + TEXT(" files"), FString::Join(Ours, TEXT(",")), FString::Join(Listed, TEXT(",")));
				}
			}
		}
	}
	// the import tool's manifest: the pin, every file, the loops
	const FString ManifestFile = FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Audio/import-manifest.json"));
	if (!FFileHelper::LoadFileToString(Body, *ManifestFile))
	{
		AddWarning(TEXT("no Content/Audio/import-manifest.json: run unreal/Tools/import-reference-audio.sh"));
		return true;
	}
	TSharedPtr<FJsonObject> Manifest;
	const TSharedRef<TJsonReader<>> MReader = TJsonReaderFactory<>::Create(Body);
	if (!TestTrue(TEXT("manifest parses"), FJsonSerializer::Deserialize(MReader, Manifest) && Manifest.IsValid())) return true;
	TestTrue(TEXT("manifest is for the pin"), Manifest->GetStringField(TEXT("pin")).StartsWith(ReferencePin));
	TMap<FString, TSharedPtr<FJsonObject>> ByAsset;
	for (const TSharedPtr<FJsonValue>& V : Manifest->GetArrayField(TEXT("files"))) ByAsset.Add(V->AsObject()->GetStringField(TEXT("asset")), V->AsObject());
	TestEqual(TEXT("manifest files"), ByAsset.Num(), 179);
	for (const FClip& C : Clips())
	{
		const FString Package = FString::Printf(TEXT("/Game/Audio/SFX/%s"), C.Id);
		const TSharedPtr<FJsonObject>* E = ByAsset.Find(Package);
		if (!TestNotNull(*FString::Printf(TEXT("manifest has %s"), *Package), E)) continue;
		TestEqual(FString::Printf(TEXT("%s looping"), C.Id), (*E)->GetBoolField(TEXT("looping")), C.bLoop);
		TestEqual(FString::Printf(TEXT("%s imported length"), C.Id), (*E)->GetNumberField(TEXT("duration")), C.Dur, 0.002);
	}
	for (const TCHAR* Folder : { TEXT("us"), TEXT("de"), TEXT("ru") })
		for (const FString& S : TCAudioRules::Speakers(Folder))
			for (const FString& Stem : VoiceStems(S))
				TestTrue(FString::Printf(TEXT("manifest has %s/%s"), Folder, *Stem), ByAsset.Contains(FString::Printf(TEXT("/Game/Audio/Voice/%s/%s"), Folder, *Stem)));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCAudioEnvironmentTest, "ThreeCrossroads.Audio.Environment", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCAudioEnvironmentTest::RunTest(const FString& Parameters)
{
	using namespace TCAudioRules;
	// client/engine-presentation.js environmentalMix run under Node on this map (a river at column 20, woods at 5..9):
	// unreal/Tools/audio/env-oracle.mjs; the third case knows only x < 36 m
	struct FCase { double X, Z, Dist, Yaw; const TCHAR* Weather; int32 WxNum; double Wx[4]; bool bKnowHalf; double Water, WaterPan, Wood, WoodPan, Wind, Rain; };
	static const FCase Cases[] = {
		{ 30, 30, 60, 0, TEXT("clear"), 0, { 0, 0, 0, 0 }, false, 0.12628, 0.276308279596703, 0.08569, -0.26644882615326754, 0.055, 0 },
		{ 30, 30, 140, 1.1, TEXT("rain"), 4, { 0.6, 0.4, 2.0, 0.8 }, false, 0.09492, 0.12533236394283848, 0.06441, 0.11660100084600183, 0.06361626369141521, 0.12 },
		{ 30, 30, 60, 0, TEXT("snow"), 4, { 0.9, 0, 0, 0 }, true, 0, 0, 0.08569, -0.26644882615326754, 0.055, 0 },
		{ 16, 14, 85, -0.7, TEXT("clear"), 0, { 0, 0, 0, 0 }, false, 0.09267224346029411, 0.33691433236600504, 0.07904000000000001, -0.021526621918415288, 0.055, 0 },
	};
	for (const FCase& C : Cases)
	{
		FEar E;
		E.X = C.X; E.Z = C.Z; E.Dist = C.Dist; E.Yaw = C.Yaw;
		auto Classify = [&C](int32 X, int32 Z)
		{
			if (C.bKnowHalf && (X + 0.5) * CellMetres >= 36) return ECell::None;
			return X == 20 ? ECell::Water : (X >= 5 && X <= 9 && Z >= 5 && Z <= 9 ? ECell::Woodland : ECell::None);
		};
		const FEnvironment R = EnvironmentMix(40, 30, Classify, &E, C.Weather, C.Wx, C.WxNum);
		const FString L = FString::Printf(TEXT("environment at (%.0f, %.0f) %s"), C.X, C.Z, C.Weather);
		TestEqual(L + TEXT(" water"), R.Water.Gain, C.Water, 1e-9);
		TestEqual(L + TEXT(" water pan"), R.Water.Pan, C.WaterPan, 1e-9);
		TestEqual(L + TEXT(" woodland"), R.Woodland.Gain, C.Wood, 1e-9);
		TestEqual(L + TEXT(" woodland pan"), R.Woodland.Pan, C.WoodPan, 1e-9);
		TestEqual(L + TEXT(" wind"), R.Wind.Gain, C.Wind, 1e-9);
		TestEqual(L + TEXT(" rain"), R.Rain.Gain, C.Rain, 1e-9);
	}
	// the synthesized loops: 8 s, deterministic, quiet at the wrap, never clipping
	for (const TCHAR* Kind : { TEXT("wind"), TEXT("water"), TEXT("woodland"), TEXT("rain") })
	{
		const TArray<float> A = RenderNoise(Kind, 48000), B = RenderNoise(Kind, 48000);
		TestEqual(FString::Printf(TEXT("%s: 8 s"), Kind), A.Num(), 8 * 48000);
		TestTrue(FString::Printf(TEXT("%s: the same every time"), Kind), A == B);
		double Peak = 0, Sum = 0;
		for (const float V : A) { Peak = FMath::Max(Peak, (double)FMath::Abs(V)); Sum += (double)V * V; }
		TestTrue(FString::Printf(TEXT("%s: below full scale (peak %.3f)"), Kind, Peak), Peak < 1.0);
		TestTrue(FString::Printf(TEXT("%s: not silent"), Kind), Sum / A.Num() > 1e-6);
		TestTrue(FString::Printf(TEXT("%s: quiet at the wrap"), Kind), FMath::Abs(A.Last()) < 0.05 && FMath::Abs(A[0]) < 0.05);
	}
	return true;
}

#endif // WITH_DEV_AUTOMATION_TESTS
