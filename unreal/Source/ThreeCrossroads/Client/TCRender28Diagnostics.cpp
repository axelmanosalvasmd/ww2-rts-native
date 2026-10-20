#include "Client/TCRender28Diagnostics.h"
#include "CoreMinimal.h"
#include "Client/TCRender28Policy.h"
#include "Client/TCGraphics.h"
#include "Camera/PlayerCameraManager.h"
#include "Dom/JsonObject.h"
#include "DynamicRHI.h"
#include "Engine/World.h"
#include "GameFramework/PlayerController.h"
#include "HAL/IConsoleManager.h"
#include "HAL/PlatformProcess.h"
#include "HAL/PlatformTime.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"

DEFINE_LOG_CATEGORY_STATIC(LogTCRender28, Log, All);

void FTCRender28Diagnostics::CaptureTour(UWorld* World, const TCHAR* Pose)
{
	FString Target;
	const bool Requested = FParse::Value(FCommandLine::Get(), TEXT("TCRender28TransitionPose="), Target, false);
	static TCRender28::FTransitionBudget Budget;
	if (!World || !Budget.Take(Requested, Target == Pose)) return;
	FString Pass, Resource;
	FParse::Value(FCommandLine::Get(), TEXT("TCRender28TransitionPass="), Pass, false);
	FParse::Value(FCommandLine::Get(), TEXT("TCRender28TransitionResource="), Resource, false);
	if (!TCRender28::LiteralFilter(*Pass) || !TCRender28::LiteralFilter(*Resource))
	{
		UE_LOG(LogTCRender28, Error, TEXT("transition request refused: require bounded literal pass and resource filters"));
		return;
	}
	IConsoleManager& Console = IConsoleManager::Get();
	IConsoleVariable* Frames = Console.FindConsoleVariable(TEXT("r.RDG.TransitionLog"));
	IConsoleVariable* PassFilter = Console.FindConsoleVariable(TEXT("r.RDG.Debug.PassFilter"));
	IConsoleVariable* ResourceFilter = Console.FindConsoleVariable(TEXT("r.RDG.Debug.ResourceFilter"));
	IConsoleVariable* GraphFilter = Console.FindConsoleVariable(TEXT("r.RDG.Debug.GraphFilter"));
	if (!Frames || !PassFilter || !ResourceFilter || !GraphFilter)
	{
		UE_LOG(LogTCRender28, Warning, TEXT("transition request unavailable: engine RDG debug controls are absent"));
		return;
	}
	// Never replace another diagnostic or narrow a clobber/lifetime/transient-resource experiment.
	for (const TCHAR* Name : {TEXT("r.RDG.TransitionLog"), TEXT("r.RDG.ClobberResources"),
		TEXT("r.RDG.Debug.ExtendResourceLifetimes"), TEXT("r.RDG.Debug.DisableTransientResources")})
	{
		const IConsoleVariable* Existing = Console.FindConsoleVariable(Name);
		if (!Existing || Existing->GetInt() != 0)
		{
			UE_LOG(LogTCRender28, Error, TEXT("transition request refused: existing or unavailable diagnostic %s"), Name);
			return;
		}
	}
	for (const IConsoleVariable* Filter : {PassFilter, ResourceFilter, GraphFilter})
	{
		const FString Existing = Filter->GetString();
		if (!Existing.IsEmpty() && !Existing.Equals(TEXT("None"), ESearchCase::IgnoreCase))
		{
			UE_LOG(LogTCRender28, Error, TEXT("transition request refused: an RDG filter is already configured"));
			return;
		}
	}
	PassFilter->Set(*Pass, ECVF_SetByConsole);
	ResourceFilter->Set(*Resource, ECVF_SetByConsole);
	Frames->Set(1, ECVF_SetByConsole);
	Console.CallAllConsoleVariableSinks();
	TSharedRef<FJsonObject> Receipt = MakeShared<FJsonObject>();
	Receipt->SetStringField(TEXT("schema"), TEXT("tc-render28-transition-request-v1"));
	Receipt->SetNumberField(TEXT("pid"), FPlatformProcess::GetCurrentProcessId());
	Receipt->SetNumberField(TEXT("gameFrame"), static_cast<double>(GFrameCounter));
	Receipt->SetNumberField(TEXT("monotonicSeconds"), FPlatformTime::Seconds());
	Receipt->SetStringField(TEXT("pose"), Pose);
	Receipt->SetStringField(TEXT("adapter"), GRHIAdapterName);
	Receipt->SetStringField(TEXT("driver"), GRHIAdapterUserDriverVersion);
	Receipt->SetStringField(TEXT("rhi"), GDynamicRHI ? GDynamicRHI->GetName() : TEXT("unavailable"));
	Receipt->SetStringField(TEXT("graphics"), LexToString(FTCGraphics::Get().Level()));
	Receipt->SetStringField(TEXT("passFilter"), PassFilter->GetString());
	Receipt->SetStringField(TEXT("resourceFilter"), ResourceFilter->GetString());
	Receipt->SetNumberField(TEXT("transitionFrames"), Frames->GetInt());
	Receipt->SetStringField(TEXT("limits"), TEXT("Request only. Require actual LogRDG pass/RDG/RHI pointer lines and final process exit. No Vulkan view range, fault association, texels or acceptance is inferred. Filters are local to this process; no preferences are saved."));
	if (const APlayerController* PC = World->GetFirstPlayerController())
		if (const APlayerCameraManager* Camera = PC->PlayerCameraManager)
		{
			const FVector Eye = Camera->GetCameraLocation();
			Receipt->SetArrayField(TEXT("eyeCm"), {MakeShared<FJsonValueNumber>(Eye.X), MakeShared<FJsonValueNumber>(Eye.Y), MakeShared<FJsonValueNumber>(Eye.Z)});
			Receipt->SetNumberField(TEXT("horizontalFovDegrees"), Camera->GetFOVAngle());
		}
	FString Json;
	const TSharedRef<TJsonWriter<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>> Writer = TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Json);
	FJsonSerializer::Serialize(Receipt, Writer);
	UE_LOG(LogTCRender28, Log, TEXT("request %s"), *Json);
}
