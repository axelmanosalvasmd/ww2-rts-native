#include "Client/TCRenderer34.h"
#include "Client/TCRenderer34Policy.h"
#include "CoreMinimal.h"
#include "DynamicRHI.h"
#include "Engine/World.h"
#include "HAL/IConsoleManager.h"
#include "HAL/PlatformMemory.h"
#include "HAL/PlatformTime.h"
#include "Misc/OutputDeviceRedirector.h"
#include "RHIStats.h"
DEFINE_LOG_CATEGORY_STATIC(LogTCRenderer34, Log, All);
void TCRenderer34::Tick(UWorld* World)
{
    static FSamples Samples;
    static const bool Enabled = FParse::Param(FCommandLine::Get(), TEXT("TCRenderer34Memory"));
    const double Now = FPlatformTime::Seconds();
    if (!World || !Samples.Take(Enabled, Now)) return;
    const FPlatformMemoryStats Process = FPlatformMemory::GetStats();
    FRHIMemoryStats Memory;
    if (GDynamicRHI && GIsRHIInitialized) RHIGetMemoryStats(Memory);
    UE_LOG(LogTCRenderer34, Log, TEXT("memory sample=%d monotonic=%.3f world=%.3f valid=%d localUsed=%llu localBudget=%llu systemUsed=%llu systemBudget=%llu rss=%llu"),
        Samples.Count, Now, World->GetRealTimeSeconds(), Memory.IsValid(),
        (unsigned long long)Memory.UsedLocal, (unsigned long long)Memory.BudgetLocal,
        (unsigned long long)Memory.UsedSystem, (unsigned long long)Memory.BudgetSystem, (unsigned long long)Process.UsedPhysical);
    // This engine has DumpMemory, VulkanMemory and VulkanMemoryRaw, not r.Vulkan.MemoryStats.
    // Record handler results so a missing command cannot masquerade as measured allocations.
    for (const TCHAR* Command : { TEXT("r.Vulkan.DumpMemory"), TEXT("rhi.DumpResourceMemory 24 Transient=all") })
    {
        UE_LOG(LogTCRenderer34, Log, TEXT("inventory begin %s"), Command);
        const bool Handled = IConsoleManager::Get().ProcessUserConsoleInput(Command, *GLog, World);
        UE_LOG(LogTCRenderer34, Log, TEXT("inventory end handled=%d %s"), Handled, Command);
    }
    GLog->Flush();
}
