// Graphics High or Low (client/gfx.js and its consumers at the pin). See TCGraphics.h.
#include "Client/TCGraphics.h"
#include "Client/TCPersistence.h"
#include "HAL/IConsoleManager.h"
#include "Misc/ConfigCacheIni.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"

DEFINE_LOG_CATEGORY_STATIC(LogTCGraphics, Log, All);

const TCHAR* const FTCGraphics::LevelKey = TEXT("ww2-gfx");
const TCHAR* const FTCGraphics::AutoKey = TEXT("ww2-gfx-auto");
const TCHAR* const FTCGraphics::GoreKey = TEXT("ww2-gore");
const TCHAR* const FTCGraphics::ReferencePin = TEXT("e4e579a838aabae3b1d9dadbc17a0a87147bc52d");

const TCHAR* LexToString(ETCGraphicsLevel Level) { return Level == ETCGraphicsLevel::Low ? TEXT("low") : TEXT("high"); }

FTCGraphicsBudget FTCGraphicsBudget::For(ETCGraphicsLevel Level)
{
	FTCGraphicsBudget B;
	B.Level = Level;
	if (Level == ETCGraphicsLevel::High) return B;
	// light.js applyGfx and renderFrame
	B.bEnvironment = false; B.bSoftShadows = false; B.ShadowMapSize = 1024; B.ShadowRadiusPerSoft = 0.f; B.ShadowRadius = 1.f;
	B.bAmbientOcclusion = false;
	// fx.js
	B.ParticleCap = 1600; B.CraterCap = 24; B.CraterLife = 60.f; B.bScreenShake = false; B.bFlipbookBlend = false; B.LitSmokeScale = 0.85f;
	// atmosphere.js
	B.bCloudShadows = false; B.bBirds = false; B.bDustFall = false; B.MistCap = 18; B.BankCap = 26;
	// unit-models.js LOD.low
	B.LodMetres = 80.f;
	// model-textures.js, infantry-material.js
	B.bModelTextures = false; B.bInfantryDetail = false;
	// water.js, props.js, ground.js, relief.js, structures.js, lobby-view.js
	B.bWaterMotion = false; B.bGrass = false; B.bPropShadows = false; B.GroundTexels = 1280; B.bReliefLow = true; B.bStructuresLow = true; B.bLobbyBackdrop = false;
	// aircraft.js, objectives.js
	B.PlaneSmokeDrip = 0.07f; B.bPlaneChunks = false; B.FlakBurstShare = 0.5f; B.DamageSmokeIntervalScale = 2.f; B.ObjectiveFireSources = 1;
	return B;
}

double FTCGraphicsBudget::PixelRatio(double Dpr) const
{
	const double R = Level == ETCGraphicsLevel::Low ? FMath::Min(1.0, 0.75 * Dpr) : FMath::Min(Dpr, 1.5);
	return FMath::RoundHalfFromZero(R * 100.0) / 100.0; // Math.round(... * 100) / 100 (positive values)
}

double FTCGraphicsBudget::ScreenPercentage(double Dpr) const
{
	return Dpr > 0 ? 100.0 * PixelRatio(Dpr) / Dpr : 100.0;
}

FTCGraphics& FTCGraphics::Get()
{
	static FTCGraphics Game;
	static bool bScheduleRead = false;
	if (!bScheduleRead)
	{
		bScheduleRead = true;
		FString Spec;
		if (FParse::Value(FCommandLine::Get(), TEXT("TCGraphicsAt="), Spec, false))
		{
			Game.Schedule = ParseSchedule(Spec);
			UE_LOG(LogTCGraphics, Log, TEXT("graphics: %d switches scheduled (-TCGraphicsAt=%s)"), Game.Schedule.Num(), *Spec);
		}
	}
	return Game;
}

TArray<TPair<double, ETCGraphicsLevel>> FTCGraphics::ParseSchedule(const FString& Spec)
{
	TArray<TPair<double, ETCGraphicsLevel>> Out;
	TArray<FString> Items;
	Spec.ParseIntoArray(Items, TEXT(","));
	for (const FString& Item : Items)
	{
		FString L, T;
		if (!Item.Split(TEXT("@"), &L, &T)) continue;
		L.TrimStartAndEndInline(); T.TrimStartAndEndInline();
		if ((L != TEXT("low") && L != TEXT("high")) || !T.IsNumeric()) continue;
		Out.Add({ FCString::Atod(*T), L == TEXT("low") ? ETCGraphicsLevel::Low : ETCGraphicsLevel::High });
	}
	Out.StableSort([](const TPair<double, ETCGraphicsLevel>& A, const TPair<double, ETCGraphicsLevel>& B) { return A.Key < B.Key; });
	return Out;
}

void FTCGraphics::RunSchedule(double MatchSeconds)
{
	while (Schedule.Num() && MatchSeconds >= Schedule[0].Key)
	{
		const TPair<double, ETCGraphicsLevel> Next = Schedule[0];
		Schedule.RemoveAt(0);
		UE_LOG(LogTCGraphics, Log, TEXT("graphics: -TCGraphicsAt switch to %s at match %.2f s (scheduled %.2f s)"), LexToString(Next.Value), MatchSeconds, Next.Key);
		Set(Next.Value, false);
	}
}

FTCGraphics::FTCGraphics() : HighBudget(FTCGraphicsBudget::For(ETCGraphicsLevel::High)), LowBudget(FTCGraphicsBudget::For(ETCGraphicsLevel::Low))
{
	Load();
}

FTCGraphics::FTCGraphics(const FString& InIni)
	: Ini(InIni), bMemoryOnly(InIni.IsEmpty()), HighBudget(FTCGraphicsBudget::For(ETCGraphicsLevel::High)), LowBudget(FTCGraphicsBudget::For(ETCGraphicsLevel::Low))
{
	Load();
}

TOptional<FString> FTCGraphics::Read(const TCHAR* Key) const
{
	if (bMemoryOnly) { const FString* V = Memory.Find(Key); return V ? TOptional<FString>(*V) : TOptional<FString>(); }
	if (Ini.IsEmpty()) return FTCSettingsStore::Game().Get(Key);
	return FTCSettingsStore(Ini).Get(Key);
}

void FTCGraphics::Write(const TCHAR* Key, const FString& Value)
{
	// gfx.js store(): a failed write keeps the choice for this run only
	if (bMemoryOnly) { Memory.Add(Key, Value); return; }
	const bool bOk = Ini.IsEmpty() ? FTCSettingsStore::Game().Set(Key, Value) : FTCSettingsStore(Ini).Set(Key, Value);
	if (!bOk) UE_LOG(LogTCGraphics, Warning, TEXT("graphics: %s could not be saved; kept for this run"), Key);
}

void FTCGraphics::Load()
{
	Current = FTCSettingsStore::ReadIs(Read(LevelKey), TEXT("low")) ? ETCGraphicsLevel::Low : ETCGraphicsLevel::High;
	bGore = FTCSettingsStore::ReadOnUnless(Read(GoreKey), TEXT("off"));
	SlowFor = 0;
}

FTCGraphicsLevelScope::FTCGraphicsLevelScope(ETCGraphicsLevel Level) : Was(FTCGraphics::Get().Level()) { FTCGraphics::Get().Set(Level, false); }
FTCGraphicsLevelScope::~FTCGraphicsLevelScope() { FTCGraphics::Get().Set(Was, false); }

FTCGraphicsWatch::~FTCGraphicsWatch()
{
	if (Handle.IsValid() && Graphics) Graphics->OnChanged.Remove(Handle);
}

bool FTCGraphicsWatch::TakeChanged()
{
	if (!Handle.IsValid())
	{
		if (!Graphics) Graphics = &FTCGraphics::Get();
		Count = MakeShared<int32>(0);
		// the lambda holds the counter, not this object, so a moved or copied owner never leaves a dangling subscriber
		Handle = Graphics->OnChanged.AddLambda([C = Count](ETCGraphicsLevel) { ++*C; });
		Taken = 0;
		return false;
	}
	if (*Count == Taken) return false;
	Taken = *Count;
	return true;
}

void FTCGraphics::Set(ETCGraphicsLevel Level, bool bSave)
{
	if (Level == Current) return;
	Current = Level;
	if (bSave) Write(LevelKey, LexToString(Level));
	ChangeCount++;
	UE_LOG(LogTCGraphics, Log, TEXT("graphics: level %s (change %d)%s"), LexToString(Current), ChangeCount, bSave ? TEXT("") : TEXT(", not saved"));
	OnChanged.Broadcast(Current);
}

void FTCGraphics::SetGore(bool bOn)
{
	if (bOn == bGore) return;
	bGore = bOn;
	Write(GoreKey, bGore ? TEXT("on") : TEXT("off"));
	OnGoreChanged.Broadcast(bGore);
}

bool FTCGraphics::AutoDropDone() const
{
	// store(() => localStorage.getItem(AUTO_KEY)): any non-empty string counts
	const TOptional<FString> V = Read(AutoKey);
	return V.IsSet() && !V->IsEmpty();
}

bool FTCGraphics::WatchFps(double Dt, FString* OutNotice)
{
	if (IsLow() || AutoDropDone()) return false;
	SlowFor = Dt > SlowFrame ? SlowFor + Dt : FMath::Max(0.0, SlowFor - Dt * 2);
	if (SlowFor < SlowLimit) return false;
	Write(AutoKey, TEXT("1"));
	Set(ETCGraphicsLevel::Low);
	if (OutNotice) *OutNotice = DropNotice();
	UE_LOG(LogTCGraphics, Log, TEXT("graphics: %.2f s of frames slower than 1/45 s: set to Low (once; %s saved)"), SlowFor, AutoKey);
	return true;
}

FString FTCGraphics::MenuLabel(ETCGraphicsLevel Level) { return Level == ETCGraphicsLevel::Low ? TEXT("Graphics: Low") : TEXT("Graphics: High"); }
const TCHAR* FTCGraphics::MenuTip() { return TEXT("Low uses cheaper shadows, fewer particles and no cloud shadows or birds"); }
FString FTCGraphics::GoreLabel(bool bOn) { return FString::Printf(TEXT("Gore: %s"), bOn ? TEXT("On") : TEXT("Off")); }
const TCHAR* FTCGraphics::GoreTip() { return TEXT("Blood and torn bodies when men die in a blast"); }
const TCHAR* FTCGraphics::DropNotice() { return TEXT("Graphics set to Low to keep the game smooth. Change it in the menu."); }

TArray<FString> FTCGraphics::ApplyEngine(double DevicePixelRatio) const
{
	const FTCGraphicsBudget& B = Budget();
	TArray<FString> Lines;
	auto SetVar = [&Lines](const TCHAR* Name, const FString& Value)
	{
		if (IConsoleVariable* V = IConsoleManager::Get().FindConsoleVariable(Name))
		{
			// round 15: by code, as the lobby backdrop sets r.ScreenPercentage (TCLobbyBackdrop, by code): a game setting
			// is lower priority and was ignored after any backdrop had shown ("Value remains '100'" in the round 15 log),
			// so Low's 75% never applied mid-match. The line reports the value read back.
			V->Set(*Value, ECVF_SetByCode);
			const FString Now = V->GetString();
			Lines.Add(Now == Value || FCString::Atod(*Now) == FCString::Atod(*Value) ? FString::Printf(TEXT("%s %s"), Name, *Value) : FString::Printf(TEXT("%s %s (asked %s)"), Name, *Now, *Value));
		}
	};
	SetVar(TEXT("r.ScreenPercentage"), FString::SanitizeFloat(B.ScreenPercentage(DevicePixelRatio)));
	SetVar(TEXT("r.Shadow.MaxResolution"), FString::FromInt(B.ShadowMapSize));
	SetVar(TEXT("r.ShadowQuality"), B.bSoftShadows ? TEXT("3") : TEXT("1"));
	UE_LOG(LogTCGraphics, Log, TEXT("graphics %s: %s"), LexToString(Current), *FString::Join(Lines, TEXT(", ")));
	return Lines;
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/Paths.h"
#include "HAL/FileManager.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCGraphicsTest, "ThreeCrossroads.Graphics.Levels", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCGraphicsTest::RunTest(const FString&)
{
	// gfx.js: High by default, gore on, nothing saved until a change
	FTCGraphics G{ FString() };
	TestFalse(TEXT("High by default"), G.IsLow());
	TestTrue(TEXT("gore on by default"), G.Gore());
	int32 Changes = 0;
	ETCGraphicsLevel Seen = ETCGraphicsLevel::High;
	G.OnChanged.AddLambda([&](ETCGraphicsLevel L) { Changes++; Seen = L; });
	G.Set(ETCGraphicsLevel::High);
	TestEqual(TEXT("no change, no notice"), Changes, 0);
	G.Set(ETCGraphicsLevel::Low);
	TestEqual(TEXT("one notice"), Changes, 1);
	TestTrue(TEXT("notified Low"), Seen == ETCGraphicsLevel::Low);
	G.Toggle();
	TestFalse(TEXT("toggled back"), G.IsLow());

	// the budgets (the reference's numbers)
	const FTCGraphicsBudget H = FTCGraphicsBudget::For(ETCGraphicsLevel::High), L = FTCGraphicsBudget::For(ETCGraphicsLevel::Low);
	TestEqual(TEXT("particles"), H.ParticleCap * 10000 + L.ParticleCap, 4096 * 10000 + 1600);
	TestEqual(TEXT("craters"), H.CraterCap * 100 + L.CraterCap, 64 * 100 + 24);
	TestEqual(TEXT("shadow map"), H.ShadowMapSize * 10000 + L.ShadowMapSize, 2048 * 10000 + 1024);
	TestTrue(TEXT("Low: no shake, clouds, birds, AO"), !L.bScreenShake && !L.bCloudShadows && !L.bBirds && !L.bAmbientOcclusion && H.bScreenShake && H.bCloudShadows && H.bBirds);
	TestEqual(TEXT("LOD"), H.LodMetres * 1000 + L.LodMetres, 110.f * 1000 + 80.f);
	TestEqual(TEXT("rain at a third, rounded up"), L.FallParticles(1000), 334);
	TestEqual(TEXT("rain on High"), H.FallParticles(1000), 1000);
	// perf.js renderScale
	TestEqual(TEXT("High at dpr 1"), H.PixelRatio(1.0), 1.0);
	TestEqual(TEXT("High at dpr 2"), H.PixelRatio(2.0), 1.5);
	TestEqual(TEXT("Low at dpr 1"), L.PixelRatio(1.0), 0.75);
	TestEqual(TEXT("Low at dpr 2"), L.PixelRatio(2.0), 1.0);
	TestEqual(TEXT("Low at dpr 1.25"), L.PixelRatio(1.25), 0.94);
	TestEqual(TEXT("screen percentage Low dpr 1"), L.ScreenPercentage(1.0), 75.0);
	TestEqual(TEXT("screen percentage High dpr 2"), H.ScreenPercentage(2.0), 75.0);

	// watchFps: 5 s of slow frames, a fast frame takes twice its time off
	FTCGraphics W{ FString() };
	FString Notice;
	int32 Frames = 0;
	bool bDropped = false;
	for (; Frames < 1000 && !bDropped; Frames++) bDropped = W.WatchFps(0.03, &Notice);
	TestTrue(TEXT("dropped"), bDropped);
	TestEqual(TEXT("after 5 s of 30 ms frames (167 frames)"), Frames, 167);
	TestTrue(TEXT("Low now"), W.IsLow());
	TestEqual(TEXT("notice"), Notice, FString(FTCGraphics::DropNotice()));
	TestTrue(TEXT("auto key saved"), W.AutoDropDone());
	W.Set(ETCGraphicsLevel::High);
	bool bAgain = false;
	for (int32 I = 0; I < 1000; I++) bAgain |= W.WatchFps(0.05);
	TestFalse(TEXT("never again once the key is saved"), bAgain);
	FTCGraphics F{ FString() };
	for (int32 I = 0; I < 100; I++) F.WatchFps(0.03);   // 3 s slow
	for (int32 I = 0; I < 50; I++) F.WatchFps(0.01);    // 0.5 s fast takes 1 s off
	TestTrue(TEXT("slow clock 2 s"), FMath::IsNearlyEqual(F.SlowSeconds(), 2.0, 1e-9));
	for (int32 I = 0; I < 1000; I++) F.WatchFps(1.0 / 60.0);
	TestEqual(TEXT("not below zero"), F.SlowSeconds(), 0.0);
	TestFalse(TEXT("a frame of exactly 1/45 s is not slow"), F.WatchFps(1.0 / 45.0) || F.SlowSeconds() > 0);
	TestEqual(TEXT("light.js clamps dt"), FTCGraphics::ClampFrame(0.5), 0.1);

	// labels
	TestEqual(TEXT("menu label"), FTCGraphics::MenuLabel(ETCGraphicsLevel::Low), FString(TEXT("Graphics: Low")));
	TestEqual(TEXT("gore label"), FTCGraphics::GoreLabel(false), FString(TEXT("Gore: Off")));

	// round 15: the change event the world builders watch (gfx.onChange), the unsaved switch and the schedule
	{
		FTCGraphics S{ FString() };
		FTCGraphicsWatch A(&S), B(&S);
		TestFalse(TEXT("a watch subscribes at its first look"), A.TakeChanged());
		TestTrue(TEXT("subscribed"), A.Subscribed() && !B.Subscribed());
		S.Set(ETCGraphicsLevel::Low, false);
		TestTrue(TEXT("the watch sees the change"), A.TakeChanged());
		TestFalse(TEXT("once"), A.TakeChanged());
		TestFalse(TEXT("a watch that subscribed after the change saw nothing"), B.TakeChanged());
		S.Set(ETCGraphicsLevel::High, false); S.Set(ETCGraphicsLevel::Low, false);
		TestTrue(TEXT("two changes, one look"), A.TakeChanged() && !A.TakeChanged());
		TestEqual(TEXT("events counted"), A.Events(), 3);
		TestTrue(TEXT("B saw the two after it subscribed"), B.TakeChanged() && B.Events() == 2);
		{
			FTCGraphicsWatch C(A); // a copy subscribes on its own and ends with itself
			C.TakeChanged();
			S.Set(ETCGraphicsLevel::High, false);
			TestTrue(TEXT("the copy sees it"), C.TakeChanged());
		}
		S.Set(ETCGraphicsLevel::Low, false); // the copy is gone: no dangling subscriber
		TestTrue(TEXT("the original still sees it"), A.TakeChanged());
		TestEqual(TEXT("changes counted by the source"), S.Changes(), 5);
		const auto Plan = FTCGraphics::ParseSchedule(TEXT("high@120, low@60,bad@3,low@x,medium@4"));
		TestEqual(TEXT("schedule: two valid items"), Plan.Num(), 2);
		TestTrue(TEXT("schedule sorted by time"), Plan.Num() == 2 && Plan[0].Key == 60.0 && Plan[0].Value == ETCGraphicsLevel::Low && Plan[1].Key == 120.0);
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSettingsRestartTest, "ThreeCrossroads.Settings.SurviveRestart", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSettingsRestartTest::RunTest(const FString&)
{
	// the reference's read rules
	TestEqual(TEXT("ui scale unset"), FTCSettingsStore::ReadUIScale({}), 100);
	// measured in Chromium 152 (<input type=range min=75 max=150 step=5>, value = stored ?? 100)
	const TPair<const TCHAR*, int32> Cases[] = { { TEXT("100"), 100 }, { TEXT("75"), 75 }, { TEXT("150"), 150 }, { TEXT("62"), 75 }, { TEXT("151"), 150 }, { TEXT("97"), 95 },
		{ TEXT("97.5"), 100 }, { TEXT("112.5"), 115 }, { TEXT("abc"), 115 }, { TEXT(""), 115 }, { TEXT(" 90"), 115 }, { TEXT("90 "), 115 }, { TEXT("+90"), 115 },
		{ TEXT(".5e2"), 75 }, { TEXT("1e2"), 100 }, { TEXT("80.0"), 80 }, { TEXT("-5"), 75 }, { TEXT("200"), 150 }, { TEXT("102.4999"), 100 }, { TEXT("77.5"), 80 } };
	for (const auto& C : Cases) TestEqual(FString::Printf(TEXT("ui scale '%s'"), C.Key), FTCSettingsStore::ReadUIScale(FString(C.Key)), C.Value);
	TestEqual(TEXT("volume 0.35"), FTCSettingsStore::ReadVolume(FString(TEXT("0.35")), {}), 0.35);
	TestEqual(TEXT("volume above 1"), FTCSettingsStore::ReadVolume(FString(TEXT("3")), {}), 1.0);
	TestEqual(TEXT("volume prefix"), FTCSettingsStore::ReadVolume(FString(TEXT(" .5x")), {}), 0.5);
	TestEqual(TEXT("volume muted"), FTCSettingsStore::ReadVolume({}, FString(TEXT("1"))), 0.0);
	TestEqual(TEXT("volume default"), FTCSettingsStore::ReadVolume(FString(TEXT("abc")), {}), 0.8);
	TestEqual(TEXT("language stored"), FTCSettingsStore::ReadLanguage(FString(TEXT("es")), TEXT("en-US")), FString(TEXT("es")));
	TestEqual(TEXT("language system"), FTCSettingsStore::ReadLanguage(FString(TEXT("fr")), TEXT("es-MX")), FString(TEXT("es")));
	TestTrue(TEXT("edge on unless 0"), FTCSettingsStore::ReadEdge({}) && !FTCSettingsStore::ReadEdge(FString(TEXT("0"))) && !FTCSettingsStore::ReadEdge(FString(TEXT("False"))));
	TestEqual(TEXT("pan"), FTCSettingsStore::ReadPan(FString(TEXT("2"))) * 10 + FTCSettingsStore::ReadPan(FString(TEXT("3"))), 21);
	TestTrue(TEXT("every key in the table"), FTCSettingsStore::Entries().Num() == 15 && FTCSettingsStore::Find(TEXT("ww2-gfx")) && FTCSettingsStore::Find(TEXT("ww2rts-lang")));

	// a restart: write through one store, drop the file from the config cache, read it back from disk
	const FString Path = FPaths::ConvertRelativePathToFull(FPaths::Combine(FPaths::ProjectSavedDir(), TEXT("Automation"), TEXT("TCSettingsRestart.ini")));
	IFileManager::Get().Delete(*Path, false, true, true);
	if (GConfig) GConfig->UnloadFile(Path);
	{
		FTCSettingsStore Store(Path);
		FTCGraphics G(Path);
		G.Set(ETCGraphicsLevel::Low);
		G.SetGore(false);
		TestTrue(TEXT("ui scale saved"), Store.SetUIScale(137));
		TestTrue(TEXT("language saved"), Store.Set(TEXT("ww2rts-lang"), TEXT("es")));
		TestTrue(TEXT("volume saved"), Store.Set(TEXT("ww2-volume"), TEXT("0.35")));
	}
	if (GConfig) GConfig->UnloadFile(Path);
	TestTrue(TEXT("the file exists"), IFileManager::Get().FileExists(*Path));
	FConfigFile Disk;
	Disk.Read(Path);
	FString Raw;
	TestTrue(TEXT("graphics on disk"), Disk.GetString(FTCSettingsStore::Section, TEXT("Graphics"), Raw) && Raw == TEXT("low"));
	{
		FTCSettingsStore Store(Path);
		FTCGraphics G(Path);
		TestTrue(TEXT("graphics survives"), G.IsLow());
		TestFalse(TEXT("gore survives"), G.Gore());
		TestEqual(TEXT("ui scale survives (snapped to 135)"), Store.UIScale(), 135);
		TestEqual(TEXT("language survives"), FTCSettingsStore::ReadLanguage(Store.Get(TEXT("ww2rts-lang")), TEXT("en-US")), FString(TEXT("es")));
		TestEqual(TEXT("volume survives"), FTCSettingsStore::ReadVolume(Store.Get(TEXT("ww2-volume")), Store.Get(TEXT("ww2-muted"))), 0.35);
	}
	if (GConfig) GConfig->UnloadFile(Path);
	IFileManager::Get().Delete(*Path, false, true, true);
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
