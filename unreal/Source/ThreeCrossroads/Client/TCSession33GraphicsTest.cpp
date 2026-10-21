// Integration coverage, not a performance measurement. PlayerTick reads injected FApp time.
#include "Client/TCRows30Harness.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCPersistence.h"
#include "Client/TCLocale.h"
#include "Client/TCSession33Graphics.h"
#include "HAL/IConsoleManager.h"
#include "Misc/App.h"
#include "Engine/LocalPlayer.h"
#include "Engine/Engine.h"

// Keep the singleton identity and its real subscribers, but isolate writes and restore every changed field.
struct FTCGraphics33Scope
{
	FTCGraphics& G = FTCGraphics::Get();
	FString Ini = G.Ini;
	bool MemoryOnly = G.bMemoryOnly;
	TMap<FString, FString> Memory = G.Memory;
	ETCGraphicsLevel Level = G.Current;
	double Slow = G.SlowFor, Delta = FApp::GetDeltaTime();
	int32 Changes = G.ChangeCount;
	TArray<TPair<double, ETCGraphicsLevel>> Schedule = G.Schedule;
	struct FCVar { IConsoleVariable* Var; FString Value; uint32 Priority; };
	TArray<FCVar> CVars;
	FTCGraphics33Scope()
	{
		for (const TCHAR* Name : { TEXT("r.ScreenPercentage"), TEXT("r.Shadow.MaxResolution"), TEXT("r.Shadow.MaxCSMResolution"), TEXT("r.ShadowQuality"),
			TEXT("r.Forward.LightGridPixelSize"), TEXT("r.Forward.LightGridSizeZ"), TEXT("r.Forward.MaxCulledLightsPerCell") })
			if (auto* V = IConsoleManager::Get().FindConsoleVariable(Name)) CVars.Add({V, V->GetString(), V->GetFlags() & ECVF_SetByMask});
		G.bMemoryOnly = true; G.Memory.Empty(); G.Schedule.Empty(); G.SlowFor = 0;
		G.Set(ETCGraphicsLevel::High, false);
	}
	~FTCGraphics33Scope()
	{
		G.Set(Level, false);
		G.Ini = Ini; G.bMemoryOnly = MemoryOnly; G.Memory = Memory;
		G.SlowFor = Slow; G.ChangeCount = Changes; G.Schedule = Schedule;
		FApp::SetDeltaTime(Delta);
		for (auto& C : CVars)
		{
			C.Var->Set(*C.Value, ECVF_SetByCode);
			C.Var->SetFlags(static_cast<EConsoleVariableFlags>((C.Var->GetFlags() & ~ECVF_SetByMask) | C.Priority));
		}
	}
	TOptional<FString> Saved(const TCHAR* Key) const { return G.Read(Key); }
};

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSession33GraphicsFrameTest, "ThreeCrossroads.Session33.GraphicsControllerFrames",
	EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::EngineFilter)
bool FTCSession33GraphicsFrameTest::RunTest(const FString&)
{
	FTCGraphics33Scope Restore;
	TCRows30::FRows30Scene Scene;
	if (!Scene.Ready(*this)) return false;
	FTCGraphics& G = FTCGraphics::Get();
	ATCPlayerController& PC = *Scene.PC;
	// UE 5.8 requires a real ULocalPlayer to classify a controller as local without a NetDriver.
	PC.Player = NewObject<ULocalPlayer>(GEngine);
	// Exercise the production BeginPlay subscription, not a replacement ApplyEngine callback.
	PC.DispatchBeginPlay();
	if (!TestTrue(TEXT("production local-game graphics subscription"), PC.GraphicsChangedHandle.IsValid())) return false;
	FTCGraphicsWatch Watch(&G); Watch.TakeChanged();
	const int32 Before = G.Changes();
	auto Frame = [&](double Dt) { FApp::SetDeltaTime(Dt); PC.PlayerTick(0.001f); };
	for (int32 I = 0; I < 200; ++I) Frame(0.1);
	TestEqual(TEXT("lobby frames do not accumulate slow time"), G.SlowSeconds(), 0.0);
	Scene.Playing();
	auto* Terrain = Scene.M->Terrain.Get(); Scene.M->Terrain = nullptr;
	for (int32 I = 0; I < 200; ++I) Frame(0.1);
	TestEqual(TEXT("match without ground does not accumulate slow time"), G.SlowSeconds(), 0.0);
	Scene.M->Terrain = Terrain;
	for (int32 I = 0; I < 10; ++I) Frame(0.5);
	TestTrue(TEXT("controller clamps FApp time to 0.1 despite 0.001 PlayerTick argument"), FMath::IsNearlyEqual(G.SlowSeconds(), 1.0, 1e-9));
	for (int32 I = 0; I < 100; ++I) Frame(0.01);
	TestEqual(TEXT("fast frames drain the clock to zero"), G.SlowSeconds(), 0.0);
	for (int32 I = 0; I < 100; ++I) Frame(0.03);
	for (int32 I = 0; I < 50; ++I) Frame(0.01);
	TestTrue(TEXT("fast frames subtract twice dt through controller"), FMath::IsNearlyEqual(G.SlowSeconds(), 2.0, 1e-9));
	for (int32 I = 0; I < 200; ++I) Frame(0.01);
	Frame(1.0 / 45.0);
	TestEqual(TEXT("exactly 45 fps is not slow through controller"), G.SlowSeconds(), 0.0);
	for (int32 I = 0; I < 166; ++I) Frame(0.03);
	TestFalse(TEXT("166 slow frames do not reach five seconds"), G.IsLow());
	TestFalse(TEXT("no early automatic key"), G.AutoDropDone());
	TestTrue(TEXT("no early notice"), PC.GraphicsNotice.IsEmpty());
	Frame(0.03);
	TestTrue(TEXT("167th actual controller call drops singleton to Low"), G.IsLow());
	TestEqual(TEXT("exactly one level change"), G.Changes(), Before + 1);
	TestTrue(TEXT("world subscription receives exactly one change"), Watch.TakeChanged() && Watch.Events() == 1 && !Watch.TakeChanged());
	TestTrue(TEXT("singleton writes Low and automatic marker in isolated memory"), Restore.Saved(FTCGraphics::LevelKey).Get(FString()) == TEXT("low")
		&& Restore.Saved(FTCGraphics::AutoKey).Get(FString()) == TEXT("1"));
	TestEqual(TEXT("original notice reaches controller"), PC.GraphicsNotice, FString(FTCGraphics::DropNotice()));
	TestEqual(TEXT("notice uses actual world real-time clock"), PC.GraphicsNoticeAt, double(Scene.World->GetRealTimeSeconds()));
	TestEqual(TEXT("original six-second HUD duration"), FTCGraphics::NoticeSeconds, 6.0);
	TestFalse(TEXT("original Spanish translation exists"), FTCLocale::Translate(PC.GraphicsNotice, ETCLanguage::Spanish) == PC.GraphicsNotice);
	FJsonObject Sensor; TCSession33Graphics::Append(PC, Sensor);
	const auto& State = Sensor.GetObjectField(TEXT("graphics33"));
	TestEqual(TEXT("sensor identifies actual persisted settings file"), State->GetStringField(TEXT("settingsFilename")), FString(TEXT("GameUserSettings.ini")));
	TestTrue(TEXT("sensor identifies actual platform directory"), !State->GetStringField(TEXT("settingsPlatform")).IsEmpty());
	TestTrue(TEXT("observational sensor reports the active original notice"), State->GetBoolField(TEXT("noticeActive"))
		&& State->GetStringField(TEXT("noticeEnglish")) == PC.GraphicsNotice);
	auto* Percentage = IConsoleManager::Get().FindConsoleVariable(TEXT("r.ScreenPercentage"));
	TestTrue(TEXT("BeginPlay subscriber applies actual engine Low screen percentage"), Percentage && Percentage->GetFloat() == 75.f);
	for (int32 I = 0; I < 200; ++I) Frame(0.1);
	TestEqual(TEXT("remaining Low does not drop again"), G.Changes(), Before + 1);
	const double NoticeAt = PC.GraphicsNoticeAt;
	G.Set(ETCGraphicsLevel::High); // Explicitly separate a manual return from the automatic transition.
	for (int32 I = 0; I < 200; ++I) Frame(0.1);
	TestFalse(TEXT("saved automatic marker prevents a second drop after manual High"), G.IsLow());
	TestEqual(TEXT("second drop never renews notice"), PC.GraphicsNoticeAt, NoticeAt);
	G.OnChanged.Remove(PC.GraphicsChangedHandle);
	AddInfo(TEXT("Injected FApp dt; real PlayerTick/BeginPlay/singleton/ApplyEngine, memory-isolated store. Not rendered-frame, disk persistence, fresh-process or performance evidence."));
	return !HasAnyErrors();
}
#endif
