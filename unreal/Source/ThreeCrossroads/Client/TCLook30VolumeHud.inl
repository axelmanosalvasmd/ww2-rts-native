// Included once by TCHUD.cpp after TCHUD.h and the audio subsystem definition.
#include "Client/TCAudio.h"

bool ATCHUD::VolumePressAt(FVector2D Pixel)
{
 FTCUiCommand Command;
 if (!CommandAt(Pixel,Command) || Command.Action!=ETCUiAction::Volume) return false;
 for (int32 I=Buttons.Num()-1; I>=0; --I)
 {
  const auto& B=Buttons[I];
  if (!B.bEnabled || B.Command.Action!=ETCUiAction::Volume || !B.Rect.IsInside(Pixel)) continue;
  UTCAudio* A=GetWorld()?GetWorld()->GetSubsystem<UTCAudio>():nullptr;
  if (!A) return true;
  VolumeDragBox=B.Rect;
  if (VolumeDrag.Begin(Pixel.X,B.Rect.Min.X+8,B.Rect.Max.X-8)) A->SetVolume(VolumeDrag.Percent/100.);
  return true;
 }
 return true;
}

void ATCHUD::VolumeDragFrame(FVector2D Pixel,bool Held,bool Focused)
{
 if (!VolumeDrag.Active) return;
 FTCUiCommand Current;
 const bool Visible=CommandAt(VolumeDragBox.GetCenter(),Current) && Current.Action==ETCUiAction::Volume;
 if (!Focused || !Visible) { VolumeDrag.Cancel(); return; }
 if (VolumeDrag.Move(Pixel.X,Held))
  if (UTCAudio* A=GetWorld()?GetWorld()->GetSubsystem<UTCAudio>():nullptr) A->SetVolume(VolumeDrag.Percent/100.);
}

void ATCHUD::DrawVolumeTrack()
{
 UTCAudio* A=GetWorld()?GetWorld()->GetSubsystem<UTCAudio>():nullptr;
 if (!A) return;
 for (const auto& B : Buttons)
 {
  if (B.Command.Action!=ETCUiAction::Volume || !B.bEnabled) continue;
  FTCUiCommand Current;
  if (!CommandAt(B.Rect.GetCenter(),Current) || Current.Action!=ETCUiAction::Volume) continue;
  const float X=B.Rect.Min.X+8,Y=B.Rect.Max.Y-4,Width=FMath::Max(1.f,float(B.Rect.GetSize().X)-16);
  DrawRect(FLinearColor(.12f,.14f,.15f,1),X,Y,Width,2);
  DrawRect(FLinearColor(.83f,.68f,.36f,1),X,Y,Width*A->Volume(),2);
  DrawRect(FLinearColor::White,X+Width*A->Volume()-2,Y-2,4,6);
 }
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/ScopeExit.h"
#include "Client/TCPersistence.h"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCLook30VolumeHandler, "ThreeCrossroads.Look30.VolumeRealHandler",
 EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCLook30VolumeHandler::RunTest(const FString&)
{
 auto& Preferences=FTCSettingsStore::Game();
 const auto Saved=Preferences.Get(TEXT("ww2-volume"));
 ON_SCOPE_EXIT { if (Saved.IsSet()) Preferences.Set(TEXT("ww2-volume"),Saved.GetValue()); else Preferences.Remove(TEXT("ww2-volume")); };
 UWorld* W=UWorld::CreateWorld(EWorldType::Game,false,TEXT("TCLook30Volume"));
 if (!TestNotNull(TEXT("world"),W)) return false;
 GEngine->CreateNewWorldContext(EWorldType::Game).SetCurrentWorld(W);
 ON_SCOPE_EXIT { GEngine->DestroyWorldContext(W); W->DestroyWorld(false); };
 auto* H=W->SpawnActor<ATCHUD>(); auto* A=W->GetSubsystem<UTCAudio>();
 if (!TestNotNull(TEXT("HUD"),H) || !TestNotNull(TEXT("audio"),A)) return false;
 H->Buttons.Add({FBox2D(FVector2D(0,0),FVector2D(116,30)),FTCUiCommand(ETCUiAction::Volume),TEXT("Volume"),true,{}});
 TestTrue(TEXT("real volume hit consumes press"),H->VolumePressAt(FVector2D(45,15)));
 TestEqual(TEXT("real press sets 37 percent"),A->Volume(),.37,1e-12);
 H->VolumeDragFrame(FVector2D(71,15),true,true);
 TestEqual(TEXT("real held path sets 63 percent"),A->Volume(),.63,1e-12);
 FTCAudioMixer Restored; Restored.InitPreferences(Preferences);
 TestEqual(TEXT("real handler persisted 63 percent"),Restored.Volume(),.63,1e-12);
 H->VolumeDragFrame(FVector2D(80,15),false,true);
 TestFalse(TEXT("real release ends drag"),H->VolumeDrag.Active);
 H->VolumePressAt(FVector2D(45,15)); H->VolumeDragFrame(FVector2D(108,15),true,false);
 TestEqual(TEXT("blur cancels without sampling a new value"),A->Volume(),.37,1e-12);
 H->Buttons.Add({FBox2D(FVector2D(0,0),FVector2D(116,30)),FTCUiCommand(ETCUiAction::Stop),TEXT("covered"),true,{}});
 TestFalse(TEXT("covered slider refuses the hit"),H->VolumePressAt(FVector2D(90,15)));
 TestEqual(TEXT("covered slider cannot change saved volume"),A->Volume(),.37,1e-12);
 return true;
}
#endif
