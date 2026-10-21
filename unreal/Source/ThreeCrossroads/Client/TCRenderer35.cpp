#include "Client/TCRenderer35.h"
#include "CoreMinimal.h"
#include "DynamicRHI.h"
#include "Engine/Engine.h"
#include "Engine/World.h"
#include "HAL/IConsoleManager.h"
#include "HAL/PlatformTime.h"
#include "Misc/CommandLine.h"
#include "Misc/OutputDeviceRedirector.h"
#include "Misc/Parse.h"
#include "RHIStats.h"

DEFINE_LOG_CATEGORY_STATIC(LogTCRenderer35, Log, All);

void TCRenderer35::Tick(UWorld* World)
{
    static const bool Enabled = FParse::Param(FCommandLine::Get(), TEXT("TCRenderer35Memory"));
    static double Next = 0;
    static int32 Count = 0;
    if (!Enabled || !World) return;
    const double Now = FPlatformTime::Seconds();
    if (Now < Next) return;
    Next = Now + 5.0;
    if (++Count == 1 && GEngine) GEngine->Exec(World, TEXT("stat RHI"), *GLog);
    FRHIMemoryStats Memory;
    if (GDynamicRHI && GIsRHIInitialized) RHIGetMemoryStats(Memory);
    UE_LOG(LogTCRenderer35, Log, TEXT("memory sample=%d monotonic=%.3f world=%.3f valid=%d localUsed=%llu localBudget=%llu systemUsed=%llu systemBudget=%llu"),
        Count, Now, World->GetRealTimeSeconds(), Memory.IsValid(),
        (unsigned long long)Memory.UsedLocal, (unsigned long long)Memory.BudgetLocal,
        (unsigned long long)Memory.UsedSystem, (unsigned long long)Memory.BudgetSystem);
    for (const TCHAR* Name : { TEXT("r.Streaming.PoolSize"), TEXT("r.Streaming.UseFixedPoolSize"),
        TEXT("r.VirtualTextures"), TEXT("r.VT.PoolSizeScale"), TEXT("r.VT.PoolAutoGrow"),
        TEXT("r.ScreenPercentage"), TEXT("r.DynamicRes.OperationMode"), TEXT("r.Nanite"),
        TEXT("r.DynamicGlobalIlluminationMethod"), TEXT("r.ReflectionMethod"),
        TEXT("r.RDG.TransientAllocator"), TEXT("r.RDG.ParallelExecute"),
        TEXT("r.Vulkan.UseMemoryBarrierOpt"), TEXT("r.Vulkan.ForceStagingBufferOnLock"),
        TEXT("r.Vulkan.EnableValidation") })
    {
        IConsoleVariable* Var = IConsoleManager::Get().FindConsoleVariable(Name);
        UE_LOG(LogTCRenderer35, Log, TEXT("cvar %s=%s"), Name, Var ? *Var->GetString() : TEXT("unavailable"));
    }
    // These commands emit counters to the log. stat RHI alone only draws an overlay.
    // Inventories can perturb submission timing. Compare with the DRM-only lane.
    for (const TCHAR* Command : { TEXT("rhi.DumpMemory"), TEXT("r.Vulkan.DumpMemory"), TEXT("r.Vulkan.DumpStagingMemory") })
    {
        UE_LOG(LogTCRenderer35, Log, TEXT("inventory begin %s"), Command);
        const bool Handled = IConsoleManager::Get().ProcessUserConsoleInput(Command, *GLog, World);
        UE_LOG(LogTCRenderer35, Log, TEXT("inventory end handled=%d %s"), Handled, Command);
    }
    GLog->Flush();
}
