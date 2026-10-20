#include "Client/TCRender26Diagnostics.h"
#include "Client/TCRender26Policy.h"
#include "Client/TCGraphics.h"
#include "Camera/PlayerCameraManager.h"
#include "Dom/JsonObject.h"
#include "DynamicRHI.h"
#include "Engine/Engine.h"
#include "Engine/GameViewportClient.h"
#include "Engine/World.h"
#include "GameFramework/PlayerController.h"
#include "HAL/IConsoleManager.h"
#include "HAL/FileManager.h"
#include "HAL/PlatformMemory.h"
#include "HAL/PlatformProcess.h"
#include "HAL/PlatformTime.h"
#include "Misc/CommandLine.h"
#include "Misc/OutputDeviceRedirector.h"
#include "Misc/Parse.h"
#include "Misc/Paths.h"
#include "RHIStats.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "UnrealClient.h"

DEFINE_LOG_CATEGORY_STATIC(LogTCRender26, Log, All);

void FTCRender26Diagnostics::Tick(UWorld* World)
{
	static const bool Enabled = FParse::Param(FCommandLine::Get(), TEXT("TCRender26Receipts"));
	if (!Enabled || !World) return;
	// Shared across lighting actors and world replacement. A world clock reset cannot extend the process cap.
	static TCRender26::FReceiptBudget Budget;
	const double ProcessClock = FPlatformTime::Seconds();
	if (!Budget.Take(true, ProcessClock)) return;
	const FPlatformMemoryStats Process = FPlatformMemory::GetStats();
	FTextureMemoryStats Textures;
	FRHIMemoryStats Memory;
	if (GDynamicRHI && GIsRHIInitialized)
	{
		RHIGetTextureMemoryStats(Textures);
		RHIGetMemoryStats(Memory);
	}
	TSharedRef<FJsonObject> Receipt = MakeShared<FJsonObject>();
	Receipt->SetStringField(TEXT("schema"), TEXT("tc-render26-resources-v1"));
	Receipt->SetNumberField(TEXT("pid"), FPlatformProcess::GetCurrentProcessId());
	Receipt->SetNumberField(TEXT("sample"), Budget.Count - 1);
	Receipt->SetNumberField(TEXT("frame"), static_cast<double>(GFrameCounter));
	Receipt->SetNumberField(TEXT("monotonicSeconds"), ProcessClock);
	Receipt->SetNumberField(TEXT("worldRealSeconds"), World->GetRealTimeSeconds());
	Receipt->SetStringField(TEXT("adapter"), GRHIAdapterName);
	Receipt->SetNumberField(TEXT("vendorId"), GRHIVendorId);
	Receipt->SetNumberField(TEXT("deviceId"), GRHIDeviceId);
	Receipt->SetStringField(TEXT("driver"), GRHIAdapterUserDriverVersion);
	Receipt->SetStringField(TEXT("rhi"), GDynamicRHI ? GDynamicRHI->GetName() : TEXT("unavailable"));
	Receipt->SetBoolField(TEXT("rhiInitialized"), GIsRHIInitialized);
	Receipt->SetStringField(TEXT("graphics"), LexToString(FTCGraphics::Get().Level()));
	Receipt->SetNumberField(TEXT("processPhysicalBytes"), static_cast<double>(Process.UsedPhysical));
	Receipt->SetNumberField(TEXT("processVirtualBytes"), static_cast<double>(Process.UsedVirtual));
	Receipt->SetNumberField(TEXT("systemAvailablePhysicalBytes"), static_cast<double>(Process.AvailablePhysical));
	Receipt->SetBoolField(TEXT("rhiBudgetValid"), Memory.IsValid());
	Receipt->SetNumberField(TEXT("localBudgetBytes"), static_cast<double>(Memory.BudgetLocal));
	Receipt->SetNumberField(TEXT("localUsedBytes"), static_cast<double>(Memory.UsedLocal));
	Receipt->SetNumberField(TEXT("systemBudgetBytes"), static_cast<double>(Memory.BudgetSystem));
	Receipt->SetNumberField(TEXT("systemUsedBytes"), static_cast<double>(Memory.UsedSystem));
	Receipt->SetBoolField(TEXT("textureHardwareStatsValid"), Textures.AreHardwareStatsValid());
	Receipt->SetNumberField(TEXT("streamingTextureBytes"), static_cast<double>(Textures.StreamingMemorySize));
	Receipt->SetNumberField(TEXT("nonStreamingTextureBytes"), static_cast<double>(Textures.NonStreamingMemorySize));
	Receipt->SetNumberField(TEXT("texturePoolBytes"), static_cast<double>(Textures.TexturePoolSize));
	Receipt->SetStringField(TEXT("limits"), TEXT("RHI and driver accounting, not all allocations, a leak diagnosis, GPU samples or a visibility result"));
	if (const APlayerController* PC = World->GetFirstPlayerController())
	{
		if (const APlayerCameraManager* Camera = PC->PlayerCameraManager)
		{
			const FVector Eye = Camera->GetCameraLocation();
			const FRotator Rotation = Camera->GetCameraRotation();
			Receipt->SetArrayField(TEXT("eyeCm"), {MakeShared<FJsonValueNumber>(Eye.X), MakeShared<FJsonValueNumber>(Eye.Y), MakeShared<FJsonValueNumber>(Eye.Z)});
			Receipt->SetArrayField(TEXT("rotationDegrees"), {MakeShared<FJsonValueNumber>(Rotation.Pitch), MakeShared<FJsonValueNumber>(Rotation.Yaw), MakeShared<FJsonValueNumber>(Rotation.Roll)});
			Receipt->SetNumberField(TEXT("horizontalFovDegrees"), Camera->GetFOVAngle());
		}
	}
	if (World->GetGameViewport() && World->GetGameViewport()->Viewport)
	{
		const FIntPoint Size = World->GetGameViewport()->Viewport->GetSizeXY();
		Receipt->SetNumberField(TEXT("viewportWidth"), Size.X);
		Receipt->SetNumberField(TEXT("viewportHeight"), Size.Y);
	}
	FString Json;
	const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Json);
	FJsonSerializer::Serialize(Receipt, Writer);
	UE_LOG(LogTCRender26, Log, TEXT("receipt %s"), *Json);

	static const bool Allocations = FParse::Param(FCommandLine::Get(), TEXT("TCRender26Allocations"));
	if (Allocations && TCRender26::FReceiptBudget::Inventory(Budget.Count - 1))
	{
		// Fixed read-only commands. These inventories perturb timing and are a separate instrumented lane.
		for (const TCHAR* Command : {TEXT("rhi.DumpMemory"), TEXT("rhi.DumpResourceCounts"), TEXT("rhi.DumpResourceMemory 24 Transient=all"), TEXT("r.Vulkan.DumpMemory")})
		{
			UE_LOG(LogTCRender26, Log, TEXT("allocation inventory sample %d begin %s"), Budget.Count - 1, Command);
			const bool Handled = IConsoleManager::Get().ProcessUserConsoleInput(Command, *GLog, World);
			UE_LOG(LogTCRender26, Log, TEXT("allocation inventory sample %d handled %d end %s"), Budget.Count - 1, Handled, Command);
		}
	}
}

void FTCRender26Diagnostics::CaptureTour(UWorld* World, const TCHAR* Pose)
{
	FString Directory, Target;
	if (!FParse::Value(FCommandLine::Get(), TEXT("TCRender26GpuDir="), Directory)
		|| !FParse::Value(FCommandLine::Get(), TEXT("TCRender26GpuPose="), Target) || Target != Pose) return;
	static bool Attempted = false;
	if (Attempted) return;
	Attempted = true;
#if WITH_DUMPGPU
	FString Filter;
	const bool Binary = FParse::Param(FCommandLine::Get(), TEXT("TCRender26GpuBinary"));
	FParse::Value(FCommandLine::Get(), TEXT("TCRender26GpuFilter="), Filter, false);
	if (!Binary) Filter = TEXT("*@*");
	if (!GEngine || !World || Directory.IsEmpty() || FPaths::IsRelative(Directory)
		|| Directory.Contains(TEXT("\"")) || Directory.Contains(TEXT("\n")) || Directory.Contains(TEXT("\r"))
		|| IFileManager::Get().DirectoryExists(*Directory) || IFileManager::Get().FileExists(*Directory)
		|| Filter.IsEmpty() || (Binary && Filter == TEXT("*@*"))
		|| Filter.Contains(TEXT("\"")) || Filter.Contains(TEXT("\n")) || Filter.Contains(TEXT("\r")))
	{
		UE_LOG(LogTCRender26, Error, TEXT("GPU request rejected: require a fresh absolute path and a bounded explicit binary filter"));
		return;
	}
	// Dump-only controls. No AA, RHI, viewport, tick rate, model, shadow or water setting is changed.
	const struct { const TCHAR* Name; int32 Value; } Settings[] = {
		{TEXT("r.DumpGPU.Texture"), Binary ? 2 : 1}, {TEXT("r.DumpGPU.Buffer"), 0},
		{TEXT("r.DumpGPU.FrameCount"), 1}, {TEXT("r.DumpGPU.Stream"), 0},
		{TEXT("r.DumpGPU.MaxStagingSize"), 64}, {TEXT("r.DumpGPU.Upload"), 0},
		{TEXT("r.DumpGPU.Explore"), 0}, {TEXT("r.DumpGPU.CameraCut"), 0},
		{TEXT("r.DumpGPU.FixedTickRate"), 0}, {TEXT("r.DumpGPU.Screenshot"), 0},
		{TEXT("r.DumpGPU.Delay"), 0}, {TEXT("r.DumpGPU.FrameDelay"), 0}, {TEXT("r.DumpGPU.Draws"), 0}
	};
	IConsoleManager& Console = IConsoleManager::Get();
	IConsoleVariable* Root = Console.FindConsoleVariable(TEXT("r.DumpGPU.Root"));
	if (!Root) { UE_LOG(LogTCRender26, Error, TEXT("GPU request unavailable: r.DumpGPU.Root")); return; }
	for (const auto& Setting : Settings)
	{
		if (!Console.FindConsoleVariable(Setting.Name))
		{
			UE_LOG(LogTCRender26, Error, TEXT("GPU request unavailable: %s"), Setting.Name);
			return;
		}
	}
	for (const auto& Setting : Settings) Console.FindConsoleVariable(Setting.Name)->Set(Setting.Value, ECVF_SetByConsole);
	Root->Set(TEXT("*"), ECVF_SetByConsole);
	const FString Command = FString::Printf(TEXT("DumpGPU -oneframe -noexplorer -path=\"%s\" -unified_filter=\"%s\""), *Directory, *Filter);
	const bool Handled = GEngine->Exec(World, *Command, *GLog);
	UE_LOG(LogTCRender26, Log, TEXT("GPU request pose %s frame %llu binary %d handled %d path %s filter %s; dispatch is not completed capture"),
		Pose, static_cast<unsigned long long>(GFrameCounter), Binary, Handled, *Directory, *Filter);
#else
	UE_LOG(LogTCRender26, Warning, TEXT("GPU request unavailable: engine built without WITH_DUMPGPU"));
#endif
}
