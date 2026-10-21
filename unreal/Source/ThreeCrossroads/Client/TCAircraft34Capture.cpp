#include "Client/TCAircraft34Capture.h"
#include "Client/TCEffects.h"
#include "Client/TCPlayerController.h"
#include "Client/TCGraphics.h"
#include "Engine/World.h"
#include "UnrealClient.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"

void TCAircraft34::FCapture::Frame(UWorld* World, const TArray<FTCFxAircraftDraw>& Aircraft, float Dt)
{
   if (!Initialized) { Initialized=true; FParse::Value(FCommandLine::Get(),TEXT("TCAircraft34Capture="),Prefix); }
   if (Prefix.IsEmpty() || Shot>=3 || !World) return;
   auto* PC=Cast<ATCPlayerController>(World->GetFirstPlayerController());
   if (!PC) return;
   if (!Serial && Aircraft.Num()) Serial=Aircraft[0].Serial;
   for (const auto& A:Aircraft) if (A.Serial==Serial)
   {
    PC->FocusOn(FVector2D(A.P.X,A.P.Y)); PC->ZoomTo(CaptureDistance());
    Age+=Dt;
    if (Age >= .35f + Shot*.4f)
    {
     const FString File=FString::Printf(TEXT("%s-%s-%d.png"),*Prefix,LexToString(FTCGraphics::Get().Level()),++Shot);
     FScreenshotRequest::RequestScreenshot(File,true,false);
     UE_LOG(LogTemp,Display,TEXT("aircraft34 capture: %s serial %d role %s owner %d at %.2f %.2f altitude %.2f"),*File,A.Serial,*A.Type,A.Owner,A.P.X,A.P.Y,A.P.Z);
    }
    break;
   }
}
