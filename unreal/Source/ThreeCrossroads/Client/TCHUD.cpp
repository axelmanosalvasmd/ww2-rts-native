#include "Client/TCHUD.h"
#include "Client/TCStats.h"
#include "Client/TCGraphics.h"
#include "Client/TCPersistence.h"
#include "Client/TCTutorial.h"
#include "Client/TCGameMode.h"
#include "Client/TCOrders.h"
#include "Client/TCRecruitment.h"
#include "Client/TCArmyCommands.h"
#include "Client/TCConstruction.h"
#include "Client/TCLogistics.h"
#include "Client/TCSupportOrders.h"
#include "Client/TCAbilities.h"
#include "Client/TCFieldworks.h"
#include "Client/TCFormationControls.h"
#include "Client/TCSupportPowers.h"
#include "Client/TCCoverPreview.h"
#include "Client/TCMarkers.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCPersistence.h"
#include "Client/TCLocale.h"
#include "Client/TCAlerts.h"
#include "Client/TCBossBar.h"
#include "Client/TCEndReport.h"
#include "ThreeCrossroads.h"
#include "World/TCTerrain.h"
#include "World/TCWorldObjects.h"
#include "Net/TCConnection.h"
#include "Engine/Canvas.h"
#include "Engine/Engine.h"
#include "Engine/Font.h"
#include "Engine/Texture2D.h"
#include "CanvasItem.h"
#include "ImageUtils.h"
#include "Misc/Base64.h"
#include "UObject/StrongObjectPtr.h"

#include "PreviewScene.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Components/DirectionalLightComponent.h"
#include "Components/SkyLightComponent.h"
#include "Engine/TextureRenderTarget2D.h"
#include "Engine/StaticMesh.h"
#include "TextureResource.h"

namespace
{
#include "Client/TCHUD.Icons.inl"
	UTexture2D* HudIcons()
	{
		static TStrongObjectPtr<UTexture2D> Atlas;
		static bool Attempted=false;
		if (!Attempted)
		{
			Attempted=true; TArray<uint8> Bytes;
			if (FBase64::Decode(HudIconPng,Bytes)) Atlas.Reset(FImageUtils::ImportBufferAsTexture2D(Bytes));
			if (Atlas.IsValid()) { Atlas->Filter=TF_Bilinear; Atlas->SRGB=true; Atlas->UpdateResource(); }
			else UE_LOG(LogTC,Warning,TEXT("hud: generated reference icon atlas could not be loaded"));
		}
		return Atlas.Get();
	}
}

float FTCType::HudSymbolAdvance(uint32 CodePoint)
{
 for (const auto& Glyph : HudGlyphs) if (Glyph.CodePoint==CodePoint) return Glyph.Advance;
 return -1;
}

bool FTCType::DrawHudSymbol(uint32 CodePoint, UCanvas* Canvas, FVector2D Pos, float Px, FLinearColor Colour)
{
 const TCHAR* Key=nullptr;
 for (const auto& Glyph : HudGlyphs) if (Glyph.CodePoint==CodePoint) { Key=Glyph.Key; break; }
 if (!Key) return false;
 UTexture2D* Atlas=HudIcons();
 if (!Canvas || !Atlas || !Atlas->GetResource()) return false;
 for (int32 K=0; K<UE_ARRAY_COUNT(HudIconKeys); K++) if (FCString::Strcmp(Key,HudIconKeys[K])==0)
 {
  const FVector2D UV0(double(K%HudIconColumns)*HudIconSide/HudIconWidth,double(K/HudIconColumns)*HudIconSide/HudIconHeight);
  const FVector2D UV1=UV0+FVector2D(double(HudIconSide)/HudIconWidth,double(HudIconSide)/HudIconHeight);
  // Exported at 88px, baseline 88 in a 100-unit tile. Keep the browser's em and baseline.
  FCanvasTileItem Tile(Pos,Atlas->GetResource(),FVector2D(Px*100/88,Px*100/88),UV0,UV1,Colour);
  Tile.BlendMode=SE_BLEND_Translucent; Canvas->DrawItem(Tile); return true;
 }
 return false;
}

// portraits.js: build from the battlefield catalog, cache by type/faction/owner look, three front soldiers,
// 176x132, 22-degree vertical field of view, VIEW (.62,.42,1), source Neutral curve and 20% desaturation.
// The private preview world never sees the match terrain or changes its fog and lighting.
class FTCHudPortraits
{
 struct FJob { FString Key, Type; int32 Faction=0, Owner=0; };
 TUniquePtr<FPreviewScene> Scene;
 TStrongObjectPtr<UTextureRenderTarget2D> Target;
 USceneCaptureComponent2D* Capture=nullptr;
 TArray<UInstancedStaticMeshComponent*> Parts;
 TMap<FString,TStrongObjectPtr<UTexture2D>> Ready;
 TSet<FString> Asked, Failed;
 TArray<FJob> Queue;
 FJob Active;
 uint64 LastFrame=0;
 double FirstAsk=0;
 int32 Stage=0;
 static FLinearColor Neutral(FLinearColor C)
 {
  const float X=FMath::Min3(C.R,C.G,C.B), Offset=X<.08f?X-6.25f*X*X:.04f;
  C.R-=Offset; C.G-=Offset; C.B-=Offset;
  const float Peak=FMath::Max3(C.R,C.G,C.B), Start=.76f, D=1-Start;
  if (Peak<Start) return C;
  const float New=1-D*D/(Peak+D-Start), Mix=1-1/(.15f*(Peak-New)+1);
  C.R=FMath::Lerp(C.R*New/Peak,New,Mix); C.G=FMath::Lerp(C.G*New/Peak,New,Mix); C.B=FMath::Lerp(C.B*New/Peak,New,Mix);
  return C;
 }
 void Studio()
 {
  if (Scene) return;
  Scene=MakeUnique<FPreviewScene>(FPreviewScene::ConstructionValues().SetEditor(false).SetCreatePhysicsScene(false).SetTransactional(false).SetLightBrightness(1.9f).SetSkyBrightness(.85f));
  Scene->SetLightColor(FColor(0xff,0xee,0xdb));
  Scene->SkyLight->LowerHemisphereColor=FLinearColor(FColor(0x4c,0x42,0x32));
  Target.Reset(NewObject<UTextureRenderTarget2D>(GetTransientPackage()));
  Target->ClearColor=FLinearColor(0,0,0,1); Target->InitCustomFormat(176,132,PF_FloatRGBA,true); Target->UpdateResourceImmediate(true);
  Capture=NewObject<USceneCaptureComponent2D>(GetTransientPackage());
  Capture->TextureTarget=Target.Get(); Capture->bCaptureEveryFrame=false; Capture->bCaptureOnMovement=false;
  Capture->CaptureSource=ESceneCaptureSource::SCS_SceneColorHDR;
  Capture->FOVAngle=FMath::RadiansToDegrees(2*FMath::Atan(FMath::Tan(FMath::DegreesToRadians(11.f))*4.f/3));
  Scene->SkyLight->SetLightColor(FLinearColor(FColor(0xd0,0xd8,0xe0)));
  UDirectionalLightComponent* Rim=NewObject<UDirectionalLightComponent>(GetTransientPackage());
  Rim->SetIntensity(.7f); Rim->SetLightColor(FLinearColor(FColor(0xbf,0xd0,0xe6))); Rim->SetCastShadows(false);
  Scene->AddComponent(Rim,FTransform((-FVector(.4,-1,.8)).Rotation()));
  Capture->ShowFlags.SetFog(false); Capture->ShowFlags.SetAtmosphere(false); Capture->ShowFlags.SetMotionBlur(false);
  Capture->ShowFlags.SetTemporalAA(false); Capture->ShowFlags.SetAntiAliasing(true);
  Scene->AddComponent(Capture,FTransform::Identity);
 }
 bool Build(const FTCModelLibrary& Models)
 {
  Studio();
  for (UInstancedStaticMeshComponent* C : Parts) { Scene->RemoveComponent(C); C->DestroyComponent(); }
  Parts.Reset();
  const FTCModelEntry* E=Models.Resolve({Active.Type,Active.Faction,TEXT("")});
  if (!E) return false;
  const TArray<float> Data=Models.CustomData(Active.Owner,Active.Faction);
  FBox Bounds(ForceInit);
  for (int32 K=0; K<E->Parts.Num(); K++)
  {
   const FTCModelPart& P=E->Parts[K]; const FString Name=P.Name.ToString();
   if (P.bHidden || (Name.StartsWith(TEXT("man_")) && FCString::Atoi(*Name.RightChop(4))>=3)) continue;
   UInstancedStaticMeshComponent* C=NewObject<UInstancedStaticMeshComponent>(GetTransientPackage());
   if (!FTCModelLibrary::SetupComponent(C,P,false)) { C->DestroyComponent(); continue; }
   C->SetCastShadow(true); C->SetMobility(EComponentMobility::Movable);
   Scene->AddComponent(C,FTransform::Identity);
   const FTransform Rest=FTCModelLibrary::PartTransform(*E,K,FTransform::Identity);
   const int32 Instance=C->AddInstance(Rest);
   C->SetCustomData(Instance,Data,true); Parts.Add(C);
   Bounds+=C->GetStaticMesh()->GetBoundingBox().TransformBy(Rest);
  }
  if (!Bounds.IsValid) return false;
  const FVector Centre=Bounds.GetCenter(), Size=Bounds.GetSize(), View=FVector(.62,1,.42).GetSafeNormal();
  const FVector Forward=-View, Right=FVector::CrossProduct(FVector::UpVector,Forward).GetSafeNormal(), Up=FVector::CrossProduct(Forward,Right);
  const double Radius=FMath::Max(50.,Size.Length()/2), TanV=FMath::Tan(FMath::DegreesToRadians(11.)), TanH=TanV*4/3;
  double Distance=0;
  for (int32 K=0; K<8; K++)
  {
   const FVector Corner(K&1?Bounds.Max.X:Bounds.Min.X,K&2?Bounds.Max.Y:Bounds.Min.Y,K&4?Bounds.Max.Z:Bounds.Min.Z), V=Corner-Centre;
   Distance=FMath::Max3(Distance,FMath::Abs(FVector::DotProduct(V,Right))/TanH+FVector::DotProduct(V,View),FMath::Abs(FVector::DotProduct(V,Up))/TanV+FVector::DotProduct(V,View));
  }
  Distance*=1.05;
  Capture->SetWorldLocationAndRotation(Centre+View*Distance,Forward.Rotation());
  Capture->bOverride_CustomNearClippingPlane=true; Capture->CustomNearClippingPlane=FMath::Max(5.,Distance-Radius*2);
  const FVector Key=Centre+FVector(-.55,.75,1.1)*Radius*3;
  Scene->SetLightDirection((Centre-Key).Rotation());
  Scene->GetWorld()->SendAllEndOfFrameUpdates();
  return true;
 }
 void Develop()
 {
  TArray<FLinearColor> Samples; FReadSurfaceDataFlags Flags(RCM_MinMax); Flags.SetLinearToGamma(false);
  if (!Target->GameThread_GetRenderTargetResource()->ReadLinearColorPixels(Samples,Flags) || Samples.Num()!=176*132) { Failed.Add(Active.Key); return; }
  TArray<FColor> Pixels; Pixels.Reserve(Samples.Num()); int32 Visible=0;
  for (FLinearColor C : Samples)
  {
   const float Alpha=FMath::Clamp(1-C.A,0.f,1.f); C=Neutral(C);
   FColor P=C.ToFColorSRGB(); P.A=FMath::RoundToInt(255*Alpha); Visible+=P.A>0;
   const float Y=.299f*P.R+.587f*P.G+.114f*P.B;
   P.R=FMath::RoundToInt(FMath::Lerp(float(P.R),Y,.2f)); P.G=FMath::RoundToInt(FMath::Lerp(float(P.G),Y,.2f)); P.B=FMath::RoundToInt(FMath::Lerp(float(P.B),Y,.2f)); Pixels.Add(P);
  }
  if (!Visible) { Failed.Add(Active.Key); UE_LOG(LogTC,Warning,TEXT("portrait: empty capture %s, retaining the source silhouette"),*Active.Key); return; }
  UTexture2D* T=UTexture2D::CreateTransient(176,132,PF_B8G8R8A8);
  void* Bytes=T->GetPlatformData()->Mips[0].BulkData.Lock(LOCK_READ_WRITE); FMemory::Memcpy(Bytes,Pixels.GetData(),Pixels.Num()*sizeof(FColor)); T->GetPlatformData()->Mips[0].BulkData.Unlock();
  T->SRGB=true; T->Filter=TF_Bilinear; T->UpdateResource(); Ready.Add(Active.Key,TStrongObjectPtr<UTexture2D>(T));
  UE_LOG(LogTC,Log,TEXT("portrait: ready %s, 176x132, %d visible pixels, imported model parts %d"),*Active.Key,Visible,Parts.Num());
 }
public:
 UTexture2D* Get(const FString& Type, int32 Faction, int32 Owner)
 {
  const FString Key=FString::Printf(TEXT("%s|%d|%d"),*Type,Faction,Owner);
  if (const TStrongObjectPtr<UTexture2D>* T=Ready.Find(Key)) return T->Get();
  if (!Asked.Contains(Key) && Asked.Num()<256) { Asked.Add(Key); Queue.Add({Key,Type,Faction,Owner}); if (!FirstAsk) FirstAsk=FPlatformTime::Seconds(); }
  return nullptr;
 }
 void Tick(const FTCModelLibrary& Models)
 {
  if (LastFrame==GFrameCounter || !FirstAsk || FPlatformTime::Seconds()-FirstAsk<1.2) return;
  LastFrame=GFrameCounter;
  if (Stage==0 && Queue.Num()) { Active=Queue[0]; Queue.RemoveAt(0); if (Build(Models)) Stage=1; else Failed.Add(Active.Key); }
  else if (Stage==1) { Capture->CaptureScene(); Stage=2; }
  else if (Stage==2) { Develop(); Stage=0; }
 }
};

void ATCHUD::DrawHudIcon(const FString& Key, const FBox2D& Box, FLinearColor Colour)
{
 UTexture2D* Atlas=HudIcons();
 if (!Atlas || !Atlas->GetResource()) return;
 int32 At=INDEX_NONE;
 for (int32 K=0; K<UE_ARRAY_COUNT(HudIconKeys); K++) if (Key==HudIconKeys[K]) { At=K; break; }
 if (At==INDEX_NONE) return;
 const FVector2D Uv0(double(At%HudIconColumns)*HudIconSide/HudIconWidth,double(At/HudIconColumns)*HudIconSide/HudIconHeight);
 const FVector2D Uv1=Uv0+FVector2D(double(HudIconSide)/HudIconWidth,double(HudIconSide)/HudIconHeight);
 FCanvasTileItem Tile(Box.Min,Atlas->GetResource(),Box.GetSize(),Uv0,Uv1,Colour);
 Tile.BlendMode=SE_BLEND_Translucent; Canvas->DrawItem(Tile);
}

bool ATCHUD::ScrollSelection(FVector2D Pixel, int32 Notch)
{
 struct FScroll { const FBox2D* Box; int32* First; int32 Max, Step; };
 const FScroll Lists[]={{&SelectionScrollBox,&SelectionFirst,SelectionMax,1},{&MenuScrollBox,&MenuFirst,MenuMax,4},
  {&AirScrollBox,&AirFirst,AirMax,1},{&RecruitScrollBox,&RecruitFirst,RecruitMax,1}};
 for (const FScroll& L : Lists) if (L.Box->bIsValid && L.Box->IsInsideOrOn(Pixel))
 { *L.First=FMath::Clamp(*L.First+Notch*L.Step,0,L.Max); return true; }
 return false;
}

TSet<int32> FTCHudFit::PlaneSelection(const FTCMatchState& S, const TSet<int32>& Current, int32 Id, bool bToggle, bool bDouble)
{
 TSet<int32> Next=bToggle && !bDouble?Current:TSet<int32>();
 bool Found=false;
 for (const FTCAirRow& Row : S.Air) if (const FTCUnit* U=S.Units.Find(Row.Id); U && S.IsOwn(*U))
 {
  Found|=Row.Id==Id;
  if (bDouble) Next.Add(Row.Id);
 }
 if (!Found) return Current;
 if (!bDouble) { if (bToggle && Next.Contains(Id)) Next.Remove(Id); else Next.Add(Id); }
 return Next;
}

FString FTCHudFit::ControlTip(const FControl& C)
{
 if (!C.Hover.IsEmpty()) return C.Hover;
 return C.Value.IsEmpty() || C.Value==C.Label?C.Label:C.Label+TEXT(": ")+C.Value;
}

void ATCHUD::OfferTooltip(const FString& Tip, const FBox2D& Box)
{
 float X=0,Y=0;
 if (!Tip.IsEmpty() && GetOwningPlayerController()->GetMousePosition(X,Y) && Box.IsInsideOrOn(FVector2D(X,Y)))
 { PendingTooltip=Tip; TooltipAnchor=Box; }
}

void ATCHUD::DrawHint(UFont* Font)
{
 const ATCPlayerController* PC=Cast<ATCPlayerController>(GetOwningPlayerController());
 if (!PC || !Canvas) return;
 const ATCGameMode* M=GetWorld()->GetAuthGameMode<ATCGameMode>();
 if (M && !M->Controls.TerminalNotice.IsEmpty()) return;
 FString Hint=PC->Aim!=ETCAim::None?PC->AimHint:FString();
 const double NoteAge=GetWorld()->GetTimeSeconds()-PC->LastOrderNoteTime;
 if (!PC->bLastOrderSent && NoteAge>=0 && NoteAge<2) Hint=FTCLocale::OrderNote(PC->LastOrderNote);
 if (Hint.IsEmpty()) return;
 const FTCMeasure Measure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
 const float Left=FTCHudFit::LeftW+16, Right=Minimap.Rect().bIsValid?Minimap.Rect().Min.X/UiNow-8:Canvas->ClipX-8;
 const float W=FMath::Max(32.f,Right-Left);
 const TArray<FString> Lines=FTCPaint::Wrap(TC_T(Hint),W-20,14,false,Measure);
 const float H=Lines.Num()*18.9f+12;
 float Bottom=RecruitBox.bIsValid?RecruitBox.Min.Y/UiNow-6:Canvas->ClipY-8;
 for (int32 Attempt=0; Attempt<Panels.Num()+1; Attempt++)
 {
  const FBox2D Box(FVector2D(Left,Bottom-H),FVector2D(Right,Bottom));
  const FBox2D* Collision=Panels.FindByPredicate([&](const FBox2D& P) { return P.Intersect(ToScreen(Box)); });
  if (!Collision)
  {
   if (Box.Min.Y<8) return;
   FTCPaint P; P.Rect(Left,Box.Min.Y,W,H,TCCss::PanelSolid,TEXT("hud:hint"));
   for (int32 I=0; I<Lines.Num(); I++) P.Text(Lines[I],(Left+Right)/2,Box.Min.Y+6+I*18.9f,14,TCCss::Text,false,ETCPaintAlign::Centre);
   FTCType::Draw(P,Canvas,Font,UiNow); Panels.Add(ToScreen(Box)); return;
  }
  Bottom=Collision->Min.Y/UiNow-6;
 }
}

bool ATCHUD::RunPanelCommand(const FTCUiCommand& Command, bool bShift, bool bCtrl)
{
 if (Command.Action!=ETCUiAction::MatchControl || !Command.Unit.StartsWith(TEXT("hudpanel:"))) return false;
 ATCPlayerController* PC=Cast<ATCPlayerController>(GetOwningPlayerController());
 ATCGameMode* M=GetWorld()->GetAuthGameMode<ATCGameMode>();
 if (!PC || !M || !M->State.SeatConfirmed()) return true;
 const FString Key=Command.Unit.RightChop(9);
 if (Key.StartsWith(TEXT("scroll:")))
 {
  TArray<FString> Parts; Key.ParseIntoArray(Parts,TEXT(":"));
  if (Parts.Num()==3)
  {
   const int32 Step=FCString::Atoi(*Parts[2]);
   if (Parts[1]==TEXT("menu")) MenuFirst=FMath::Clamp(MenuFirst+4*Step,0,MenuMax);
   if (Parts[1]==TEXT("air")) AirFirst=FMath::Clamp(AirFirst+Step,0,AirMax);
   if (Parts[1]==TEXT("recruit")) RecruitFirst=FMath::Clamp(RecruitFirst+Step,0,RecruitMax);
  }
 }
 else if (Key.StartsWith(TEXT("buildtab:"))) { BuildTab=FMath::Clamp(FCString::Atoi(*Key.RightChop(9)),0,3); MenuFirst=0; }
 else if (Key==TEXT("idle")) PC->RunUi(bShift?ETCUiAction::IdleAll:ETCUiAction::Idle);
 else if (Key==TEXT("destination")) { bGroupDropdown=!bGroupDropdown; PC->bBuildMenuOpen=false; PC->bTrenchMenuOpen=false; PC->Formation.bMenuOpen=false; }
 else if (Key.StartsWith(TEXT("destination:"))) { GroupDestination=FMath::Clamp(FCString::Atoi(*Key.RightChop(12)),1,9); bGroupDropdown=false; }
 else if (Key==TEXT("transfer")) { PC->GroupChord(GroupDestination,false,false,true); bGroupDropdown=false; }
 else if (Key==TEXT("recruit"))
 {
  if (!TCArmyCommands::ClassicLobby(M->State)) { bRecruitLetters=!bRecruitLetters; PC->RecruitGroupOpen=-1; }
  else PC->ShowFeedback(TEXT("In Classic, select a Production Building and press the letters on its cards"),4);
 }
 else if (Key.StartsWith(TEXT("type:")))
 {
  const FString Type=Key.RightChop(5); TSet<int32> Next;
  for (int32 Id : PC->Selected) if (const FTCUnit* U=M->State.Units.Find(Id)) if (bShift?U->Type!=Type:U->Type==Type) Next.Add(Id);
  PC->Selected=MoveTemp(Next);
 }
 else if (Key.StartsWith(TEXT("plane:")))
 {
  const int32 Id=FCString::Atoi(*Key.RightChop(6));
  const double Now=FPlatformTime::Seconds();
  const bool Double=LastPlane==Id && Now-LastPlanePress>=0 && Now-LastPlanePress<=.32;
  PC->Selected=FTCHudFit::PlaneSelection(M->State,PC->Selected,Id,bShift||bCtrl,Double);
  LastPlane=Double?0:Id; LastPlanePress=Now;
 }
 return true;
}

bool ATCHUD::RecruitKey(const FString& Code, bool bShift, bool bCtrl, bool bAlt)
{
 ATCPlayerController* PC=Cast<ATCPlayerController>(GetOwningPlayerController());
 ATCGameMode* M=GetWorld()->GetAuthGameMode<ATCGameMode>();
 if (!PC || !M || !M->State.SeatConfirmed() || bCtrl || bAlt || PC->Aim!=ETCAim::None) return false;
 if (!bShift && (Code==TEXT("Tab") || Code==TEXT("Backquote"))) { FTCUiCommand C(ETCUiAction::MatchControl); C.Unit=TEXT("hudpanel:recruit"); RunPanelCommand(C,false); return true; }
 if (!bRecruitLetters) return false;
 if (!bShift && Code==TEXT("Escape")) { if (PC->RecruitGroupOpen>=0) PC->RecruitGroupOpen=-1; else bRecruitLetters=false; return true; }
 if (!Code.StartsWith(TEXT("Key"))) return false;
 const int32 Index=FString(TEXT("QWERTASDFGZXCVB")).Find(Code.RightChop(3));
 if (Index<0) return false;
 const FTCCommandCard Card=M->Recruit.Card(M->State,PC->Selected);
 if (PC->RecruitGroupOpen<0)
 {
  if (!Card.Groups.IsValidIndex(Index)) return false;
  PC->RecruitGroupOpen=Index; return true;
 }
 if (!Card.Groups.IsValidIndex(PC->RecruitGroupOpen) || !Card.Groups[PC->RecruitGroupOpen].Cards.IsValidIndex(Index)) return false;
 FTCUiCommand Buy(ETCUiAction::Recruit); Buy.Unit=Card.Groups[PC->RecruitGroupOpen].Cards[Index].Type;
 PC->RunCommand(Buy,bShift); return true;
}

void ATCHUD::DrawBottomLayout(const FTCHudFit::FBottomLayout& Layout, UFont* Font)
{
 FTCType::Draw(Layout.Paint,Canvas,Font,UiNow);
 for (const FBox2D& P : Layout.Paint.Panels) Panels.Add(ToScreen(P));
 for (const FTCHudFit::FIcon& I : Layout.Icons)
 {
  UTexture2D* Image=nullptr;
  if (I.bPortrait)
  {
   ATCGameMode* M=GetWorld()->GetAuthGameMode<ATCGameMode>();
   const int32 Owner=I.Owner>=0?I.Owner:M->State.You;
   const int32 Faction=M->State.Factions.IsValidIndex(Owner)?M->State.Factions[Owner]:Owner<0?0:Owner%3;
   if (!Portraits) Portraits=MakeShared<FTCHudPortraits>();
   Image=Portraits->Get(I.Key,Faction,Owner);
   if (M->Units && M->Units->ModelsLoaded()) Portraits->Tick(M->Units->Models());
  }
  if (Image && Image->GetResource()) { FCanvasTileItem Tile(I.Box.Min,Image->GetResource(),I.Box.GetSize(),FLinearColor::White); Tile.BlendMode=SE_BLEND_Translucent; Canvas->DrawItem(Tile); }
  else if (I.bPortrait) { const float Side=I.Box.GetSize().Y*.78f; const FVector2D At=I.Box.GetCenter()-FVector2D(Side/2,Side/2); DrawHudIcon(I.Key,FBox2D(At,At+FVector2D(Side,Side)),TCCss::Dim); }
  else DrawHudIcon(I.Key,I.Box,I.Colour);
 }
 for (const FTCHudFit::FHit& H : Layout.Hits)
 {
  Buttons.Add({ToScreen(H.Box),H.Control.Command,H.Control.Label,H.Control.bEnabled});
  OfferTooltip(FTCHudFit::ControlTip(H.Control),ToScreen(H.Box));
 }
}

namespace TCHudBottom
{
	using namespace FTCHudFit;
	float Border(float Scale) { return FMath::Max(1.f, FMath::FloorToFloat(Scale)) / Scale; }
	bool Overlap(const FBox2D& A, const FBox2D& B)
	{
		return A.bIsValid && B.bIsValid && A.Min.X < B.Max.X - .01 && A.Max.X > B.Min.X + .01 && A.Min.Y < B.Max.Y - .01 && A.Max.Y > B.Min.Y + .01;
	}
	FBox2D Box(float X, float Y, float W, float H) { return FBox2D(FVector2D(X,Y), FVector2D(X+W,Y+H)); }
	void Panel(FBottomLayout& L, const FBox2D& B, const TCHAR* Role, float Scale)
	{
		L.Paint.Rect(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,TCCss::Panel,Role);
		L.Paint.Frame(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,TCCss::Line,Border(Scale));
		L.Paint.Panels.Add(B);
	}
	void Text(FBottomLayout& L, const FString& Shown, float X, float Y, float Px, bool Bold, FLinearColor C,
		const FBox2D& Clip, const FTCMeasure& Measure, const TCHAR* Role, ETCPaintAlign Align = ETCPaintAlign::Left)
	{
		if (Shown.IsEmpty()) return;
		const float W = Measure(Shown,Px,Bold,0), H = Px * 1.35f;
		L.Paint.Text(Shown,X,Y,Px,C,Bold,Align,Role).Size = FVector2D(W,H);
		const float Left = X - (Align == ETCPaintAlign::Centre ? W / 2 : Align == ETCPaintAlign::Right ? W : 0);
		L.TextBounds.Add({Shown,Box(Left,Y,W,H),Clip});
	}
	float Wrapped(FBottomLayout& L, const FString& Shown, float X, float Y, float W, float Px, bool Bold,
		FLinearColor C, const FTCMeasure& Measure, const TCHAR* Role)
	{
		const TArray<FString> Lines = FTCPaint::Wrap(Shown,W,Px,Bold,Measure);
		const FBox2D Clip = Box(X,Y,W,Lines.Num()*Px*1.35f);
		for (const FString& Line : Lines) { Text(L,Line,X,Y,Px,Bold,C,Clip,Measure,Role); Y += Px*1.35f; }
		return Clip.GetSize().Y;
	}
	void Icon(FBottomLayout& L, const FString& Key, float X, float Y, float Side, FLinearColor C)
	{
		if (!Key.IsEmpty()) L.Icons.Add({Key,Box(X,Y,Side,Side),C});
	}
	float KeyWidth(const FString& Key, const FTCMeasure& Measure)
	{
	 FString Rest=Key; const bool Shift=Rest.RemoveFromStart(TEXT("⇧"));
	 return Measure(Rest,11,false,0)+(Shift?Measure(TEXT("⇧"),11,false,0):0.f);
	}
	void KeyBadge(FBottomLayout& L, const FString& Key, float Right, float Y, const FBox2D& Clip, const FTCMeasure& Measure, bool Cap=false)
	{
	 if (Key.IsEmpty()) return;
	 const float W=KeyWidth(Key,Measure);
	 if (Cap) { L.Paint.Rect(Right-W-4,Y-1,W+8,17,TCCss::Rgba(0,0,0,.18f),TEXT("bottom:keycap")); L.Paint.Frame(Right-W-4,Y-1,W+8,17,TCCss::LineSoft); }
	 FString Rest=Key; const bool Shift=Rest.RemoveFromStart(TEXT("⇧"));
	 Text(L,Rest,Right,Y,11,false,TCCss::Dim,Clip,Measure,TEXT("bottom:key"),ETCPaintAlign::Right);
	 if (Shift) Text(L,TEXT("⇧"),Right-W,Y,11,false,TCCss::Dim,Clip,Measure,TEXT("bottom:shift"));
	}
	FTCUiCommand Local(const FString& Key)
	{ FTCUiCommand C(ETCUiAction::MatchControl); C.Unit=TEXT("hudpanel:")+Key; return C; }
	void RowButton(FBottomLayout& L, const FControl& C, const FBox2D& B, const FTCMeasure& Measure, float Scale)
	{
	 L.Paint.Rect(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,C.bOn?TCCss::Rgba(214,178,94,.12f):TCCss::Hex(0x24282b));
	 L.Paint.Frame(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,C.bOn?TCCss::Brass:TCCss::LineSoft,Border(Scale));
	 Text(L,TC_T(C.Label),B.Min.X+7,B.Min.Y+4,13,false,TCCss::Text,B,Measure,TEXT("bottom:row-label"));
	 if (C.bSubmenu) Text(L,TEXT("⌄"),B.Max.X-8,B.Min.Y+2,14,false,TCCss::Dim,B,Measure,TEXT("bottom:caret"),ETCPaintAlign::Right);
	 L.Hits.Add({B,C});
	}
	void GridControl(FBottomLayout& L, const FControl& C, const FBox2D& B, const FTCMeasure& Measure, float Scale)
	{
		L.Paint.Rect(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,C.bOn ? TCCss::Rgba(214,178,94,.1f) : TCCss::Hex(0x24282b),TEXT("bottom:order"));
		L.Paint.Frame(B.Min.X,B.Min.Y,B.GetSize().X,B.GetSize().Y,C.bOn ? TCCss::Brass : TCCss::LineSoft,Border(Scale));
		Icon(L,C.Icon,B.GetCenter().X-11.5f,B.Min.Y+6,23,C.bEnabled ? TCCss::Text : TCCss::Dim);
		KeyBadge(L,C.Key,B.Max.X-3,B.Min.Y+2,B,Measure);
		if (!C.Value.IsEmpty())
        {
            if (C.bSubmenu)
            {
                const FString Shown=TC_T(C.Value);
                const float Px=FMath::Min(13.f,(B.GetSize().X-13)/FMath::Max(1.f,Measure(Shown,13,false,0))*13.f);
                Text(L,Shown,B.GetCenter().X-5,B.Max.Y-4-Px*1.35f,Px,false,C.bOn?TCCss::Brass:TCCss::Text,B,Measure,TEXT("bottom:value"),ETCPaintAlign::Centre);
            }
            else
            {
                const TArray<FString> Lines=FTCPaint::Wrap(TC_T(C.Value),B.GetSize().X-4,13,false,Measure);
                float Y=B.Max.Y-4-Lines.Num()*17.55f;
                for (const FString& Line : Lines) { Text(L,Line,B.GetCenter().X,Y,13,false,C.bOn?TCCss::Brass:TCCss::Text,B,Measure,TEXT("bottom:value"),ETCPaintAlign::Centre); Y+=17.55f; }
            }
        }
        if (C.bSubmenu) Text(L,C.bOn?TEXT("▾"):TEXT("▸"),B.Max.X-2,B.Max.Y-21,13,false,C.bOn?TCCss::Brass:TCCss::Text,B,Measure,TEXT("bottom:submenu"),ETCPaintAlign::Right);
		L.Hits.Add({B,C});
	}
	float Grid(FBottomLayout& L, const TArray<FControl>& Controls, float X, float Y, float W, const FTCMeasure& Measure, float Scale)
	{
		const float CellW = (W-3*GridGap)/4;
		for (int32 First=0; First<Controls.Num(); First+=4)
		{
			float H = OrderH;
			for (int32 K=First; K<FMath::Min(First+4,Controls.Num()); K++)
				if (!Controls[K].bSubmenu) H=FMath::Max(H,33.f+FTCPaint::Wrap(TC_T(Controls[K].Value),CellW-4,13,false,Measure).Num()*17.55f);
			for (int32 K=First; K<FMath::Min(First+4,Controls.Num()); K++) GridControl(L,Controls[K],Box(X+(K-First)*(CellW+GridGap),Y,CellW,H),Measure,Scale);
			Y+=H+GridGap;
		}
		return Controls.Num() ? Y-GridGap : Y;
	}
 float RowHeight(const TArray<FControl>& Controls, int32 First, float W, const FTCMeasure& Measure)
 {
  float H=OrderH; const float CW=(W-3*GridGap)/4;
  for (int32 K=First; K<FMath::Min(First+4,Controls.Num()); K++)
   if (!Controls[K].bSubmenu) H=FMath::Max(H,33.f+FTCPaint::Wrap(TC_T(Controls[K].Value),CW-4,13,false,Measure).Num()*17.55f);
  return H;
 }
 void Pager(FBottomLayout& L, const FString& Kind, int32 First, int32 Max, float X, float Y, float W, const FTCMeasure& Measure, float Scale)
 {
  for (int32 I=0; I<2; I++)
  {
   FControl C; C.Label=I?TEXT("Next"):TEXT("Previous"); C.Hover=C.Label;
   C.Command=Local(TEXT("scroll:")+Kind+(I?TEXT(":1"):TEXT(":-1")));
   C.bEnabled=I?First<Max:First>0;
   RowButton(L,C,Box(X+I*(W+4)/2,Y,(W-4)/2,24),Measure,Scale);
  }
 }
 float GridWindow(FBottomLayout& L, const TArray<FControl>& Controls, float X, float Y, float W, float MaxH, int32 First,
  const FTCMeasure& Measure, float Scale)
 {
  if (Controls.IsEmpty()) return Y;
  const float Top=Y;
  float Total=0; for (int32 K=0; K<Controls.Num(); K+=4) Total+=RowHeight(Controls,K,W,Measure)+GridGap;
  const bool Pages=Total-GridGap>MaxH+.01f;
  L.MenuMax=Pages?FMath::Max(0,((Controls.Num()-1)/4)*4):0;
  First=FMath::Clamp(First/4*4,0,L.MenuMax);
  const float Bottom=Top+MaxH-(Pages?28:0);
  for (int32 K=First; K<Controls.Num(); K+=4)
  {
   const float H=RowHeight(Controls,K,W,Measure);
   if (Y+H>Bottom+.01) break;
   TArray<FControl> Row; for (int32 J=K; J<FMath::Min(K+4,Controls.Num()); J++) Row.Add(Controls[J]);
   Y=Grid(L,Row,X,Y,W,Measure,Scale)+GridGap;
  }
  if (Y>Top) Y-=GridGap;
  if (Pages) { Y+=4; Pager(L,TEXT("menu"),First,L.MenuMax,X,Y,W,Measure,Scale); Y+=24; }
  L.MenuList=Box(X,Top,W,Y-Top);
  return Y;
 }

 float LogisticsText(FBottomLayout& L, const TArray<FString>& Logistics, float Y, float MaxW, const FTCMeasure& Measure)
 {
  const float Inner=MaxW-16;
	if (!Logistics.Num()) return Y;
	{
		Y+=4;
		const float Column=(Inner-8)/2;
		float DetailX=8, DetailRowH=0;
		for (int32 I=0; I<Logistics.Num(); I++)
		{
			const FString& Detail=Logistics[I];
			if (I==0 || Detail.Contains(TEXT("#")))
			{
				if (DetailX>8) { Y+=DetailRowH+3; DetailX=8; DetailRowH=0; }
				float H=Wrapped(L,TC_T(Detail),8,Y,Column,13,true,TCCss::Dim,Measure,TEXT("bottom:logistics"));
				if (I==0 && Logistics.IsValidIndex(I+1) && Logistics[I+1].Contains(TEXT("#")))
					H=FMath::Max(H,Wrapped(L,TC_T(Logistics[++I]),16+Column,Y,Column,13,true,TCCss::Dim,Measure,TEXT("bottom:logistics")));
				Y+=H+3;
				continue;
			}
			const float DetailWidth=FMath::Min(Inner,Measure(TC_T(Detail),13,false,0));
			if (DetailX>8 && DetailX+DetailWidth>MaxW-8) { Y+=DetailRowH+3; DetailX=8; DetailRowH=0; }
			const float H=Wrapped(L,TC_T(Detail),DetailX,Y,DetailWidth,13,false,TCCss::Dim,Measure,TEXT("bottom:logistics"));
			DetailRowH=FMath::Max(DetailRowH,H);
			DetailX+=DetailWidth+8;
		}
		if (DetailX>8) Y+=DetailRowH+3;
	}
  return Y;
 }

	void Shift(FBottomLayout& L, FVector2D By)
	{
		L.Paint.Shift(0,By);
		for (FBox2D& P : L.Paint.Panels) P=FBox2D(P.Min+By,P.Max+By);
		for (FIcon& I : L.Icons) I.Box=FBox2D(I.Box.Min+By,I.Box.Max+By);
		for (FHit& H : L.Hits) H.Box=FBox2D(H.Box.Min+By,H.Box.Max+By);
		for (FTextFit& T : L.TextBounds) { T.Box=FBox2D(T.Box.Min+By,T.Box.Max+By); T.Clip=FBox2D(T.Clip.Min+By,T.Clip.Max+By); }
		auto Move=[&](FBox2D& B) { if (B.bIsValid) B=FBox2D(B.Min+By,B.Max+By); };
		Move(L.SelectionList); Move(L.MenuList); Move(L.AirList); Move(L.RecruitList); Move(L.Selection); Move(L.Orders); Move(L.Recruit); Move(L.Economy);
	}
}

FTCHudFit::FBottomLayout FTCHudFit::LeftPanel(float ViewH, const FString& Title, const TArray<FSelection>& Selection,
	const TArray<FString>& Details, const TArray<FControl>& Orders, const TArray<FControl>& Menu, const FTCMeasure& Measure, float Scale, int32 Idle, int32 Destination, bool bTransfer, bool bDropdown, int32 SelectionFirst, float TopLimit, int32 MenuFirst, const TArray<FControl>& MenuTabs)
{
	using namespace TCHudBottom;
	FBottomLayout L;
	const float X=ScreenGap, W=LeftW, Inner=W-14;
	float Y=0;
	if (!Selection.IsEmpty() || Idle>0)
	{
		Y=8;
		float IdleW=0;
		if (Idle>0)
		{
		 FControl C; C.Label=FString::Printf(TEXT("Idle %d"),Idle); C.Command=Local(TEXT("idle"));
		 C.Hover=TEXT("Find the next idle unit. Shift+click selects all idle units");
		 IdleW=Measure(TC_T(C.Label),13,false,0)+14;
		 RowButton(L,C,Box(X+W-7-IdleW,Y-2,IdleW,22),Measure,Scale);
		}
		const float TitleW=Inner-(IdleW>0?IdleW+6:0);
		Text(L,FTCPaint::Ellipsis(TC_T(Title),TitleW,13,true,Measure),X+7,Y,13,true,TCCss::Text,Box(X+7,Y,TitleW,18),Measure,TEXT("bottom:selection-title")); Y+=23;
        const FBottomLayout OrdersOnly=LeftPanel(ViewH-180,TEXT(""),{},Details,Orders,Menu,Measure,Scale,0,Destination,bTransfer,bDropdown,0,TopLimit,MenuFirst,MenuTabs);
        const float OrdersH=OrdersOnly.Orders.bIsValid?OrdersOnly.Orders.GetSize().Y:0.f;
        const float ListH=FMath::Max(0.f,FMath::Min(ViewH*.4f,ViewH-ScreenGap-FMath::Max(80.f,TopLimit)-OrdersH-6-Y-6));
        const float ListTop=Y;
        int32 First=FMath::Clamp(SelectionFirst,0,FMath::Max(0,Selection.Num()-1));
        L.SelectionMax=FMath::Max(0,Selection.Num()-1);
        bool More=false;
        for (int32 Index=First; Index<Selection.Num(); Index++)
        {
            const FSelection& S=Selection[Index];
            FBottomLayout Row;
			Row.Paint.Rect(X+7,Y,Inner,Border(Scale),TCCss::LineSoft);
			const float Top=Y+5, NameX=X+7+48+8, NameW=Inner-48-8;
			Row.Paint.Rect(X+7,Top,48,36,TCCss::Rgba(0,0,0,.22f));
			Row.Icons.Add({S.Type,Box(X+7,Top,48,36),FLinearColor::White,true,S.Owner});
			const FString Count=FString::Printf(TEXT("×%d"),S.Count);
			const float CountW=Measure(Count,14,true,0);
			float NameY=Top;
			Text(Row,FTCPaint::Ellipsis(TC_T(S.Name),NameW-CountW-4,14,true,Measure),NameX,NameY,14,true,TCCss::Text,Box(NameX,NameY,NameW-CountW-4,18.9f),Measure,TEXT("bottom:selection-name")); NameY+=18.9f;
			Text(Row,Count,X+W-7,Top,14,true,TCCss::Dim,Box(X+W-7-CountW,Top,CountW,18.9f),Measure,TEXT("bottom:selection-count"),ETCPaintAlign::Right);
			const FString State=TC_T(FString::Printf(TEXT("%d/%d hp"),S.Hp,S.MaxHp));
            const float HealthW=Measure(State,13,false,0);
            Text(Row,State,NameX,NameY,13,false,TCCss::Dim,Box(NameX,NameY,NameW,18.9f),Measure,TEXT("bottom:selection-health"));
            float ChipX=NameX+HealthW+6, ChipH=17.55f;
			for (const FString& Tag : S.Tags)
			{
             if (Tag==TEXT("Auto-retreat")) { ChipX=NameX; NameY+=ChipH+3; ChipH=0; }
			 const FString Shown=TC_T(Tag);
			 const float CW=FMath::Min(NameW,Measure(Shown,13,false,0)+12);
			 if (ChipX>NameX && ChipX+CW>NameX+NameW) { ChipX=NameX; NameY+=ChipH+3; ChipH=0; }
			 const TArray<FString> Lines=FTCPaint::Wrap(Shown,CW-12,13,false,Measure);
			 const float CH=Lines.Num()*17.55f+2;
			 const FBox2D Chip=Box(ChipX,NameY,CW,CH);
			 const bool Danger=Tag==TEXT("Pinned")||Tag==TEXT("AP round loaded")||Tag==TEXT("Low supplies")||Tag==TEXT("Stranded")||Tag==TEXT("Mandatory withdrawal")||Tag==TEXT("Cut off from supply");
			 const bool Cover=Tag==TEXT("Hidden")||Tag==TEXT("Riding")||Tag==TEXT("Reinforcing")||Tag.Contains(TEXT("cover"))||Tag==TEXT("In trench")||Tag.StartsWith(TEXT("Garrisoned:"));
			 const bool Supp=Tag==TEXT("Suppressed")||Tag==TEXT("Suppressive fire");
			 const FLinearColor C=Danger?TCCss::RedHi:Cover?TCCss::Hex(0xa9b37b):Supp?TCCss::Brass:TCCss::Text;
			 FLinearColor Edge=C; Edge.A=Danger?.6f:Cover?.45f:Supp?.5f:.24f;
			 Row.Paint.Frame(Chip.Min.X,Chip.Min.Y,CW,CH,Edge,Border(Scale),TEXT("bottom:status-chip"));
			 for (int32 I=0; I<Lines.Num(); I++) Text(Row,Lines[I],ChipX+6,NameY+1+I*17.55f,13,false,C,Chip,Measure,TEXT("bottom:status-text"));
			 ChipX+=CW+6; ChipH=FMath::Max(ChipH,CH);
			}
			NameY+=ChipH;
			FControl Pick; Pick.Label=S.Name; Pick.Hover=S.Hover; Pick.Command=Local(TEXT("type:" )+S.Type);
			Row.Hits.Add({Box(X+7,Top,Inner,FMath::Max(36.f,NameY-Top)),Pick});
            const float Bottom=FMath::Max(Top+36,NameY)+5;
            if (Bottom>ListTop+ListH+.01f) { More=true; break; }
            L.Paint.Append(Row.Paint); L.Icons.Append(Row.Icons); L.Hits.Append(Row.Hits); L.TextBounds.Append(Row.TextBounds);
            Y=Bottom;
        }
        if (First>0 || More)
        {
            const float H=FMath::Max(18.f,Y-ListTop), Thumb=FMath::Max(9.f,H/Selection.Num());
            L.Paint.Rect(X+W-4,ListTop+(H-Thumb)*First/FMath::Max(1,Selection.Num()-1),2,Thumb,TCCss::Line,TEXT("bottom:selection-scroll"));
        }
        L.SelectionList=Box(X+6,ListTop,Inner,FMath::Max(0.f,Y-ListTop));
		Y+=6;
		L.Selection=Box(X,0,W,Y);
		FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,L.Selection,TEXT("bottom:selection"),Scale); L.Paint.Append(Foreground);
		Y+=6;
	}
	if (Orders.Num() || Menu.Num() || Details.Num())
	{
		const float Top=Y;
		Y+=8;
		Text(L,TC_T(FString(TEXT("Orders"))),X+7,Y,13,false,TCCss::Dim,Box(X+7,Y,Inner,18),Measure,TEXT("bottom:orders-title")); Y+=20;
		const float Budget=ViewH-ScreenGap-FMath::Max(80.f,TopLimit)-Top-8;
        TArray<FControl> VisibleOrders=Orders;
        if (Menu.Num() && Orders.Num()>4)
        {
         VisibleOrders.Reset(); for (const FControl& C : Orders) if (C.bSubmenu) VisibleOrders.Add(C);
        }
        float FixedTail=7+(bTransfer?(bDropdown?112.f:34.f):0.f)+(Menu.Num()?90.f+(MenuTabs.Num()?29.f:0.f):0.f);
        for (const FString& D : Details) FixedTail+=6+FTCPaint::Wrap(TC_T(D),Inner,13,false,Measure).Num()*17.55f;
        if (!bDropdown) Y=GridWindow(L,VisibleOrders,X+7,Y,Inner,FMath::Max(80.f,ViewH-ScreenGap-FMath::Max(80.f,TopLimit)-Y-FixedTail),MenuFirst,Measure,Scale);
		if (bTransfer)
		{
		 Y+=6;
             const FString Label=TC_T(FString(TEXT("Control group")));
             FControl Transfer; Transfer.Label=TEXT("Move to group"); Transfer.Command=Local(TEXT("transfer"));
             Transfer.Hover=TEXT("Alt+1 to Alt+9 moves selected units and removes them from other groups");
             const float TransferW=Measure(TC_T(Transfer.Label),13,false,0)+14;
             const float LabelW=Inner-TransferW-32-6;
             const float LabelPx=FMath::Min(13.f,LabelW/FMath::Max(1.f,Measure(Label,13,false,0))*13.f);
             Text(L,Label,X+7,Y+4,LabelPx,false,TCCss::Dim,Box(X+7,Y,LabelW,25),Measure,TEXT("bottom:group-title"));
             FControl Choose; Choose.Label=FString::FromInt(Destination); Choose.Command=Local(TEXT("destination")); Choose.bSubmenu=true; Choose.bOn=bDropdown;
             Choose.Hover=TEXT("Destination control group"); RowButton(L,Choose,Box(X+7+LabelW+3,Y,32,25),Measure,Scale);
             RowButton(L,Transfer,Box(X+W-7-TransferW,Y,TransferW,25),Measure,Scale); Y+=28;
		 if (bDropdown)
		 {
		  for (int32 G=1; G<=9; G++)
		  {
		   FControl C; C.Label=FString::FromInt(G); C.Command=Local(FString::Printf(TEXT("destination:%d"),G)); C.bOn=G==Destination;
		   RowButton(L,C,Box(X+7+(G-1)%3*(Inner+3)/3,Y+(G-1)/3*26,(Inner-6)/3,23),Measure,Scale);
		  }
		  Y+=78;
		 }
		}
		for (const FString& Detail : Details) { Y+=6; Y+=Wrapped(L,TC_T(Detail),X+7,Y,Inner,13,false,TCCss::Dim,Measure,TEXT("bottom:detail")); }
		if (Menu.Num() && !bDropdown)
        {
         Y+=8;
         if (MenuTabs.Num())
         {
          const float TW=(Inner-3*(MenuTabs.Num()-1))/MenuTabs.Num();
          for (int32 I=0; I<MenuTabs.Num(); I++)
          {
           FControl C=MenuTabs[I]; C.Label=FTCPaint::Ellipsis(TC_T(C.Label),TW-14,13,false,Measure);
           RowButton(L,C,Box(X+7+I*(TW+3),Y,TW,25),Measure,Scale);
          }
          Y+=29;
         }
         Y=GridWindow(L,Menu,X+7,Y,Inner,FMath::Max(82.f,Top+Budget-Y-7),MenuFirst,Measure,Scale);
        }
		Y+=7;
		L.Orders=Box(X,Top,W,Y-Top);
		FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,L.Orders,TEXT("bottom:orders"),Scale); L.Paint.Append(Foreground);
	}
	Shift(L,FVector2D(0,ViewH-ScreenGap-Y));
	return L;
}

FTCHudFit::FBottomLayout FTCHudFit::RecruitPanel(float ViewW, float ViewH, float Right, const TArray<FString>& Headings,
	const TArray<FControl>& Tabs, const TArray<TArray<FControl>>& Lists, const TArray<FControl>& Above, const FTCMeasure& Measure, float Scale,
	const TArray<FString>& Logistics, int32 TotalCards, int32 FirstCard, float TopLimit, const FBox2D& Economy)
{
	using namespace TCHudBottom;
	FBottomLayout L;
	const float Left=LeftW+2*ScreenGap, MaxW=FMath::Clamp(Right-Left,60.f,720.f), Inner=MaxW-16;
	float Y=8;
	for (const FString& H : Headings) { Y+=Wrapped(L,TC_T(H),8,Y,Inner,13,true,TCCss::Text,Measure,TEXT("bottom:recruit-heading")); Y+=6; }
    float X=8, RowH=0, Widest=0;
    TArray<float> Natural;
    float Total=0, Fixed=0;
    for (const FControl& C : Tabs)
    {
        const float Base=20+(C.Icon.IsEmpty()?0:22)+(C.Key.IsEmpty()?0:KeyWidth(C.Key,Measure)+14);
        const float W=Measure(TC_T(C.Label),13,true,0);
        Fixed+=Base;
        Natural.Add(W); Total+=W;
    }
    const float TabRoom=Inner-FMath::Max(0,Tabs.Num()-1)*4;
    const float Fit=FMath::Clamp((TabRoom-Fixed)/FMath::Max(1.f,Total),0.f,1.f);
    for (int32 Index=0; Index<Tabs.Num(); Index++)
    {
        const FControl& C=Tabs[Index];
        const float KeyRoom=C.Key.IsEmpty()?0:KeyWidth(C.Key,Measure)+14;
        const float CW=20+(C.Icon.IsEmpty()?0:22)+KeyRoom+Natural[Index]*Fit, H=29.55f;
        const FBox2D B=Box(X,Y,CW,H);
        L.Paint.Rect(X,Y,CW,H,C.bOn?TCCss::Rgba(214,178,94,.1f):TCCss::Hex(0x24282b),TEXT("bottom:tab"));
        L.Paint.Frame(X,Y,CW,H,C.bOn?TCCss::Brass:TCCss::LineSoft,Border(Scale));
        Icon(L,C.Icon,X+10,Y+(H-17)/2,17,TCCss::Dim);
        const float LabelX=X+10+(C.Icon.IsEmpty()?0:22), LabelW=FMath::Max(1.f,CW-20-(C.Icon.IsEmpty()?0:22)-KeyRoom);
        const FString Caption=LabelW>=Measure(TEXT("…"),13,true,0)?FTCPaint::Ellipsis(TC_T(C.Label),LabelW,13,true,Measure):FString();
        Text(L,Caption,LabelX,Y+6,13,true,C.bOn?TCCss::Brass:TCCss::Dim,B,Measure,TEXT("bottom:tab-label"));
        KeyBadge(L,C.Key,B.Max.X-9,B.Min.Y+6,B,Measure,true);
        L.Hits.Add({B,C}); RowH=H; Widest=FMath::Max(Widest,X+CW+8); X+=CW+4;
    }
	if (Tabs.Num()) Y+=RowH+4;
	int32 CardRow=0; const float CardsTop=Y;
 FBottomLayout Footer;
 const float FooterH=LogisticsText(Footer,Logistics,0,MaxW,Measure);
 auto CardWidth=[&](const TArray<FControl>& Cards)
 {
  const int32 Count=FMath::Max(Cards.Num(),TotalCards);
  return Tabs.Num()?FMath::Clamp((Inner-(Tabs.Num()-1)*25-(Count-Tabs.Num())*4)/Count,50.f,100.f):FMath::Clamp((Inner-4*(Cards.Num()-1))/Cards.Num(),50.f,100.f);
 };
 auto CardHeight=[&](const TArray<FControl>& Cards,int32 First,int32 N,float CW)
 {
  float H=0;
  for (int32 K=First; K<First+N; K++)
  {
   const int32 Names=(CW>72 || Cards[K].Command.Action!=ETCUiAction::Recruit)?FTCPaint::Wrap(TC_T(Cards[K].Label),CW-6-(Cards[K].Key.IsEmpty()?0:KeyWidth(Cards[K].Key,Measure)+12),13,true,Measure).Num():0;
   const int32 Costs=FTCPaint::Wrap(TC_T(Cards[K].Value),CW-6,14,true,Measure).Num();
   H=FMath::Max(H,4.f+Names*17.55f+CW*.75f+Costs*18.9f+7);
  }
  return H;
 };
 int32 AllRows=0; float AllH=0;
 for (const TArray<FControl>& Cards : Lists) if (Cards.Num())
 {
  const float CW=CardWidth(Cards); const int32 Cols=FMath::Max(1,FMath::FloorToInt((Inner+4)/(CW+4)));
  for (int32 First=0; First<Cards.Num(); First+=Cols) { AllRows++; AllH+=CardHeight(Cards,First,FMath::Min(Cols,Cards.Num()-First),CW)+6; }
 }
 const float Limit=ViewH-ScreenGap-FMath::Max(80.f,TopLimit)-(Above.Num()?34.f:0.f)-FooterH-2;
 const bool Pages=Y+AllH>Limit+.01f;
 L.RecruitMax=Pages?FMath::Max(0,AllRows-1):0; FirstCard=FMath::Clamp(FirstCard,0,L.RecruitMax);
 const float CardsBottom=Limit-(Pages?28.f:0.f);
 bool Full=false;
 for (const TArray<FControl>& Cards : Lists)
	{
		if (!Cards.Num()) continue;
		// #buy.fit uses the count across every group, including the closed ones, for its card container width.
		const int32 Count=FMath::Max(Cards.Num(),TotalCards);
		const float CW=Tabs.Num()?FMath::Clamp((Inner-(Tabs.Num()-1)*25-(Count-Tabs.Num())*4)/Count,50.f,100.f)
			:FMath::Clamp((Inner-4*(Cards.Num()-1))/Cards.Num(),50.f,100.f);
		const int32 Cols=FMath::Max(1,FMath::FloorToInt((Inner+4)/(CW+4)));
		for (int32 First=0; First<Cards.Num(); First+=Cols)
		{
			const int32 N=FMath::Min(Cols,Cards.Num()-First);
            if (CardRow++<FirstCard) continue;
			const float H=CardHeight(Cards,First,N,CW);
            if (Full || Y+H>CardsBottom+.01f) { Full=true; continue; }
            X=8+(Inner-(N*CW+(N-1)*4))/2;
			for (int32 K=First; K<First+N; K++)
			{
				const FControl& C=Cards[K]; const FBox2D B=Box(X,Y,CW,H);
				L.Paint.Rect(X,Y,CW,H,TCCss::Hex(0x24282b),TEXT("bottom:card")); L.Paint.Frame(X,Y,CW,H,TCCss::LineSoft,Border(Scale));
				float Ty=Y+4;
				// The reference's <=72px container rule keeps the full name in the hover text.
				if (CW>72 || C.Command.Action!=ETCUiAction::Recruit) Ty+=Wrapped(L,TC_T(C.Label),X+3,Ty,CW-6-(C.Key.IsEmpty()?0:KeyWidth(C.Key,Measure)+12),13,true,C.bEnabled?TCCss::Text:TCCss::Dim,Measure,TEXT("bottom:card-name"));
				L.Paint.Rect(X+1,Ty,CW-2,CW*.75f,TCCss::Rgba(0,0,0,.22f));
				if (C.Command.Action==ETCUiAction::Recruit || C.Command.Action==ETCUiAction::Build)
                    L.Icons.Add({C.Icon,Box(X+1,Ty,CW-2,CW*.75f),FLinearColor::White,true});
                else Icon(L,C.Icon,X+CW*.185f,Ty+CW*.06f,CW*.63f,C.bEnabled?TCCss::Text:TCCss::Dim);
				Ty+=CW*.75f+3;
				Wrapped(L,TC_T(C.Value),X+3,Ty,CW-6,14,true,TCCss::Text,Measure,TEXT("bottom:card-cost"));
				KeyBadge(L,C.Key,B.Max.X-9,B.Min.Y+4,B,Measure,true);
				L.Hits.Add({B,C}); Widest=FMath::Max(Widest,X+CW+8); X+=CW+4;
			}
			Y+=H+6;
		}
	}
 if (Pages)
 { Pager(L,TEXT("recruit"),FirstCard,L.RecruitMax,8,Y,Inner,Measure,Scale); Y+=28; }
 if (AllRows) L.RecruitList=Box(8,CardsTop,Inner,Y-CardsTop);
	Y=LogisticsText(L,Logistics,Y,MaxW,Measure);
	if (Logistics.Num()) Widest=MaxW;
	Y+=2;
	float AboveW=16;
	for (const FControl& C : Above) AboveW+=Measure(TC_T(C.Label),13,false,0)+26+(C.Key.IsEmpty()?0:KeyWidth(C.Key,Measure)+18);
	const float W=Lists.Num()?MaxW:FMath::Min(MaxW,FMath::Max(Widest,FMath::Max(AboveW,80.f)));
	L.Recruit=Box(0,0,W,Y);
	FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,L.Recruit,TEXT("bottom:recruit"),Scale); L.Paint.Append(Foreground);
	float AX=8;
	for (int32 I=0; I<Above.Num(); I++)
	{
		const FControl& C=Above[I]; const FString Shown=TC_T(C.Label);
		const float AW=Measure(Shown,13,false,0)+18+(C.Key.IsEmpty()?0:KeyWidth(C.Key,Measure)+18), AH=27.55f;
		const float At=I==Above.Num()-1 && I>0?FMath::Max(AX,W-8-AW):AX;
		const FBox2D B=Box(At,-6-AH,AW,AH);
		L.Paint.Rect(B.Min.X,B.Min.Y,AW,AH,TCCss::PanelSolid,TEXT("bottom:above")); L.Paint.Frame(B.Min.X,B.Min.Y,AW,AH,TCCss::LineSoft,Border(Scale));
		Text(L,Shown,At+9,B.Min.Y+5,13,false,TCCss::Dim,B,Measure,TEXT("bottom:above-label"));
		KeyBadge(L,C.Key,B.Max.X-12,B.Min.Y+6,B,Measure,true);
		if (C.Command.Action!=ETCUiAction::None) L.Hits.Add({B,C});
		L.Paint.Panels.Add(B);
		AX=At+AW+8;
	}
	Shift(L,FVector2D(Left+(MaxW-W)/2,ViewH-ScreenGap-Y));
	if (Overlap(L.Recruit,Economy) && Economy.Min.X-8<Right && Economy.Min.X-8>Left+60)
		return RecruitPanel(ViewW,ViewH,Economy.Min.X-8,Headings,Tabs,Lists,Above,Measure,Scale,Logistics,TotalCards,FirstCard,TopLimit);
	return L;
}

FTCHudFit::FBottomLayout FTCHudFit::EconomyPanel(float ViewW, const FString& Money, const FString& Income, const FString& Population,
	const TArray<FControl>& Support, const FTCMeasure& Measure, float Scale, const TArray<FControl>& Resources, const TArray<FControl>& Air, float MaxH, int32 AirFirst)
{
	using namespace TCHudBottom;
	FBottomLayout L;
	const FString Second=TC_T(Income)+TEXT("   ")+TC_T(Population);
    float ResourcesW=0;
    for (const FControl& C : Resources) ResourcesW+=Measure(C.Value,22,false,0)+Measure(TC_T(C.Label),13,false,0)+38;
    const float W=FMath::Max(238.f,FMath::Max(Measure(TC_T(Money),28,false,0)+ResourcesW+24,Measure(Second,13,false,0))+18), X=ViewW-ScreenGap-W;
    const float MoneyRight=W-9-ResourcesW;
	float Y=8;
	FString Number=Money;
	if (Number.RemoveFromEnd(TEXT(" MP")))
	{
	 const FString Label=TC_T(FString(TEXT("MP"))); const float LabelW=Measure(Label,13,false,0), NumberW=Measure(Number,28,false,0);
	 Text(L,Number,MoneyRight-4-LabelW,Y,28,false,TCCss::Brass,Box(8,Y,W-16,37.8f),Measure,TEXT("bottom:money"),ETCPaintAlign::Right);
	 Text(L,Label,MoneyRight,Y+12,13,false,TCCss::Dim,Box(8,Y,W-16,37.8f),Measure,TEXT("bottom:money-label"),ETCPaintAlign::Right);
	 Icon(L,TEXT("resource-mp"),MoneyRight-24-LabelW-NumberW,Y+10,16,TCCss::Dim);
	}
	else Text(L,TC_T(Money),W-9,Y,28,false,TCCss::Brass,Box(8,Y,W-16,37.8f),Measure,TEXT("bottom:money"),ETCPaintAlign::Right);
	if (Resources.Num())
	{
	 float RX=MoneyRight+14;
	 for (const FControl& C : Resources)
	 {
	  const float CW=Measure(C.Value,22,false,0)+Measure(TC_T(C.Label),13,false,0)+30;

	  Icon(L,C.Icon,RX,Y+10,16,TCCss::Dim);
	  Text(L,C.Value,RX+20,Y+6,22,false,TCCss::Text,Box(8,Y,W-16,38),Measure,TEXT("bottom:resource"));
	  Text(L,TC_T(C.Label),RX+24+Measure(C.Value,22,false,0),Y+8,13,false,TCCss::Dim,Box(8,Y,W-16,30),Measure,TEXT("bottom:resource-label")); L.Hits.Add({Box(RX,Y,CW,38),C}); RX+=CW+8;
	 }
	 }
    Y+=38;
	const float PopW=Measure(TC_T(Population),13,false,0);
	Text(L,TC_T(Population),W-9,Y,13,false,TCCss::Dim,Box(8,Y,W-16,17.55f),Measure,TEXT("bottom:population"),ETCPaintAlign::Right);
	const float IncomeW=W-28-PopW;
	const TArray<FString> IncLines=FTCPaint::Wrap(TC_T(Income),IncomeW,13,false,Measure);
	for (int32 I=0; I<IncLines.Num(); I++) Text(L,IncLines[I],W-9-PopW-12,Y+I*17.55f,13,false,TCCss::Hex(0xa9b37b),Box(8,Y,IncomeW,IncLines.Num()*17.55f),Measure,TEXT("bottom:income"),ETCPaintAlign::Right);
	Y+=FMath::Max(1,IncLines.Num())*17.55f+7;
	Y=Grid(L,Support,W-9-225,Y,225,Measure,Scale)+8;
 const float AirTop=Y, RowH=25.55f;
 const int32 Room=MaxH>0?FMath::Max(1,FMath::FloorToInt((MaxH-Y-8)/ (RowH+3))):Air.Num();
 const bool Pages=Air.Num()>Room;
 const bool PagerFits=Pages && Room>=2;
 const int32 Visible=PagerFits?Room-1:Room;
 L.AirMax=FMath::Max(0,Air.Num()-Visible); AirFirst=FMath::Clamp(AirFirst,0,L.AirMax);
 for (int32 I=AirFirst; I<FMath::Min(Air.Num(),AirFirst+Visible); I++)
 {
  const FControl& C=Air[I]; const FString State=TC_T(C.Value);
  const float SW=Measure(State,13,false,0), NW=W-16-24-SW;
  const FBox2D B=Box(8,Y,W-16,RowH);
  L.Paint.Rect(B.Min.X,B.Min.Y,B.GetSize().X,RowH,C.bOn?TCCss::Rgba(214,178,94,.12f):TCCss::Hex(0x24282b)); L.Paint.Frame(B.Min.X,B.Min.Y,B.GetSize().X,RowH,C.bOn?TCCss::Brass:TCCss::LineSoft,Border(Scale));
  Text(L,FTCPaint::Ellipsis(TC_T(C.Label),NW,13,false,Measure),15,Y+4,13,false,TCCss::Text,B,Measure,TEXT("bottom:air-name"));
  Text(L,State,W-15,Y+4,13,false,TCCss::Dim,B,Measure,TEXT("bottom:air-state"),ETCPaintAlign::Right);
  L.Hits.Add({B,C}); Y+=RowH+3;
 }
 if (PagerFits) { Pager(L,TEXT("air"),AirFirst,L.AirMax,8,Y,W-16,Measure,Scale); Y+=28; }
 else if (Pages)
 {
  const float H=Y-AirTop-3, Thumb=FMath::Max(4.f,H*Visible/Air.Num());
  L.Paint.Rect(W-6,AirTop+(H-Thumb)*AirFirst/FMath::Max(1,L.AirMax),2,Thumb,TCCss::Line,TEXT("bottom:air-scroll"));
 }
 if (Air.Num()) L.AirList=Box(8,AirTop,W-16,Y-AirTop);


	L.Economy=Box(0,0,W,Y);
	FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,L.Economy,TEXT("bottom:economy"),Scale); L.Paint.Append(Foreground);
	Shift(L,FVector2D(X,ScreenGap)); return L;
}

FTCHudFit::FBottomLayout FTCHudFit::Tooltip(const FString& Shown, const FBox2D& Anchor, FVector2D View, const FTCMeasure& Measure)
{
 using namespace TCHudBottom;
 FBottomLayout L;
 // feedback.js appends #commandTooltip to body, outside #hud's zoom.
 const float W=FMath::Min3(340.f,(float)View.X-16,FMath::CeilToFloat(Measure(Shown,14,false,0))+22);
 const TArray<FString> Lines=FTCPaint::Wrap(Shown,W-22,14,false,Measure);
 const float H=FMath::Min((float)View.Y-16,Lines.Num()*18.9f+14);
 const float X=FMath::Clamp((float)Anchor.GetCenter().X-W/2,8.f,(float)View.X-W-8);
 const float Above=Anchor.Min.Y-H-6;
 const float Y=Above>=8?Above:FMath::Min((float)Anchor.Max.Y+6,(float)View.Y-H-8);
 const FBox2D B=Box(X,Y,W,H);
 L.Paint.Rect(X,Y,W,H,TCCss::PanelSolid,TEXT("bottom:tooltip")); L.Paint.Frame(X,Y,W,H,TCCss::Line);
 for (int32 I=0; I<Lines.Num() && 7+(I+1)*18.9f<=H; I++) Text(L,Lines[I],X+11,Y+7+I*18.9f,14,false,TCCss::Text,B,Measure,TEXT("bottom:tooltip-text"));
 L.Paint.Panels.Add(B); return L;
}

FTCHudFit::FBottomLayout FTCHudFit::IconBar(bool bMenu, bool bCaptured, const FTCMeasure& Measure, float Scale)
{
 using namespace TCHudBottom;
 FBottomLayout L;
 Panel(L,Box(8,8,152,38),TEXT("toolbar:panel"),Scale);
 float X=12;
 const TCHAR* Keys[]={TEXT("Menu"),TEXT("Fullscreen"),TEXT("CaptureNow"),TEXT("Controls")};
 const TCHAR* Icons[]={TEXT("menuBtn"),TEXT("fullscreen"),TEXT("captureNow")};
 const TCHAR* Labels[]={TEXT("Menu"),TEXT("Fullscreen"),TEXT("Capture mouse"),TEXT("Controls, F1")};
 const TCHAR* Tips[]={TEXT("Menu: restart, end or leave the match"),TEXT("Fullscreen: also lets Ctrl+1-9 set groups"),TEXT("Capture mouse: keeps the cursor inside the game so edge scrolling always works. Click again or press Esc to let go"),TEXT("Controls and mouse help (F1)")};
 for (int32 I=0; I<4; I++)
 {
  FControl C; C.Label=Labels[I]; C.Hover=Tips[I]; C.Command=FTCUiCommand(I==3?ETCUiAction::ControlsSheet:ETCUiAction::MatchControl); if (I<3) C.Command.Unit=Keys[I];
  const float W=I==3?42:32; const FBox2D B=Box(X,12,W,30);
  if ((I==0 && bMenu) || (I==2 && bCaptured)) { L.Paint.Rect(X,12,W,30,TCCss::Rgba(214,178,94,.14f)); L.Paint.Frame(X,12,W,30,TCCss::Brass,Border(Scale)); }
  if (I<3) Icon(L,Icons[I],X+(W-18)/2,18,18,TCCss::Text);
  else { Text(L,TEXT("?"),X+W/2,10,14,true,TCCss::Brass,B,Measure,TEXT("toolbar:help"),ETCPaintAlign::Centre); KeyBadge(L,TEXT("F1"),X+W/2+5,25,B,Measure,true); }
  L.Hits.Add({B,C}); X+=W+3;
 }
 return L;
}

void ATCHUD::DrawMenuRows(UFont* Font, float X, float Y, const TArray<FTCControlRow>& Rows)
{
 const float Pad=6, Line=16;
 const FLinearColor Panel=TCCss::PanelSolid, Dim=TCCss::Dim, CardOff=TCCss::Rgba(36,40,43,.42f), TabOpen=TCCss::Rgba(214,178,94,.14f), ButtonFill=TCCss::Hex(0x24282b);
 const FVector2D Size=MeasureControlRows(Font,Rows);
 if (Size.X<=0) return;
 AddPanel(X,Y,Size.X,Size.Y,Panel);
 float RowY=Y+Pad;
 for (const FTCControlRow& Row : Rows)
 {
  if (!Row.Items.Num()) { DrawLabel(Row.Text,Dim,X+6,RowY,Font,1); RowY+=Line; continue; }
  float BX=X+6;
  if (!Row.Text.IsEmpty()) { float W=0,H=0; LabelSize(Row.Text,W,H,Font,1); DrawLabel(Row.Text,Dim,BX,RowY+2+Pad/2,Font,1); BX+=W+Pad; }
  for (const FTCControlItem& Item : Row.Items)
  {
   const float Left=BX; BX=Button(Font,BX,RowY+2,Item.Label,ControlCommand(Item.Key,Item.Slot),Item.bEnabled,!Item.bEnabled?CardOff:Item.bOn?TabOpen:ButtonFill);
   OfferTooltip(Item.Hover.IsEmpty()?Item.Label:Item.Hover,ToScreen(FBox2D(FVector2D(Left,RowY+2),FVector2D(BX-Pad,RowY+2+Line+Pad))));
  }
  RowY+=Line+Pad+4;
 }
}

FTCHudFit::FBottomLayout FTCHudFit::Toolbar(float MaxW, const TArray<FControl>& Controls, const FTCMeasure& Measure, float Scale)
{
	using namespace TCHudBottom;
	FBottomLayout L;
	float X=6,Y=6,H=0,W=0;
	for (const FControl& C : Controls)
	{
		const FString Shown=TC_T(C.Label);
		const float CW=FMath::Min(MaxW-12,FMath::CeilToFloat(Measure(Shown,13,false,0))+18);
		const TArray<FString> Lines=FTCPaint::Wrap(Shown,CW-16,13,false,Measure);
		const float CH=Lines.Num()*17.55f+10;
		if (X>6 && X+CW>MaxW-6) { X=6; Y+=H+4; H=0; }
		const FBox2D B=Box(X,Y,CW,CH);
		L.Paint.Rect(X,Y,CW,CH,TCCss::Hex(0x24282b)); L.Paint.Frame(X,Y,CW,CH,TCCss::LineSoft,Border(Scale));
		float TY=Y+5;
		for (const FString& LineText : Lines) { Text(L,LineText,X+8,TY,13,false,TCCss::Text,B,Measure,TEXT("bottom:toolbar-label")); TY+=17.55f; }
		if (C.Command.Action!=ETCUiAction::None) L.Hits.Add({B,C});
		W=FMath::Max(W,X+CW+6); X+=CW+4; H=FMath::Max(H,CH);
	}
	if (Controls.Num())
	{
		FTCPaint Foreground=MoveTemp(L.Paint); L.Paint=FTCPaint(); Panel(L,Box(0,0,W,Y+H+6),TEXT("bottom:toolbar"),Scale); L.Paint.Append(Foreground);
	}
	return L;
}

FTCPaint FTCHudFit::BossCard(const FTCBossBar& Boss, FVector2D View, float Top, const FBox2D& Economy,
	const TArray<FBox2D>& Avoid, double Now, const FTCMeasure& Measure, float Scale)
{
    return Boss.Layout(View.X/2,Top,View.X,Now,Measure,nullptr,Scale);
}

FTCHudFit::FBottomLayout FTCHudFit::SandboxToolbar(FVector2D View, float CornerRight, float ScoreLeft,
	const TArray<FControl>& Controls, const TArray<FBox2D>& Avoid, const FTCMeasure& Measure, float Scale)
{
	using namespace TCHudBottom;
	FBottomLayout L=Toolbar(View.X-16,Controls,Measure,Scale);
	if (L.Paint.Panels.IsEmpty()) return L;
	const FVector2D Size=L.Paint.Panels[0].GetSize();
	FBox2D Spot=Box((View.X-Size.X)/2,View.Y-SandboxBottom-Size.Y,Size.X,Size.Y);
	const bool Clear=Spot.Min.Y>=8 && !Avoid.ContainsByPredicate([&](const FBox2D& B) { return Overlap(Spot,B); });
	if (!Clear)
	{
		const float X=CornerRight+8, Right=ScoreLeft-4;
		L=Toolbar(FMath::Max(100.f,Right-X),Controls,Measure,Scale);
		Spot=Box(X,8,L.Paint.Panels[0].GetSize().X,L.Paint.Panels[0].GetSize().Y);
	}
	Shift(L,Spot.Min);
	return L;
}

TArray<FString> FTCHudFit::LayoutErrors(const FBottomLayout& L, FVector2D View)
{
	using namespace TCHudBottom;
	TArray<FString> Errors;
	auto Inside=[](const FBox2D& A,const FBox2D& B) { return A.Min.X>=B.Min.X-.1 && A.Min.Y>=B.Min.Y-.1 && A.Max.X<=B.Max.X+.1 && A.Max.Y<=B.Max.Y+.1; };
	const FBox2D Screen=Box(0,0,View.X,View.Y);
	for (const FIcon& I : L.Icons) if (!Inside(I.Box,Screen)) Errors.Add(TEXT("icon outside screen: ")+I.Key);
	for (const FTextFit& T : L.TextBounds) if (!Inside(T.Box,T.Clip) || !Inside(T.Box,Screen)) Errors.Add(TEXT("text outside its box: ")+T.Text);
	for (int32 I=0; I<L.TextBounds.Num(); I++)
	{
		const FTextFit& T=L.TextBounds[I];
		for (int32 J=I+1; J<L.TextBounds.Num(); J++) if (Overlap(T.Box,L.TextBounds[J].Box)) Errors.Add(TEXT("text overlaps: ")+T.Text+TEXT(" / ")+L.TextBounds[J].Text);
		for (const FHit& H : L.Hits) if (Overlap(T.Box,H.Box) && !Inside(T.Box,H.Box)) Errors.Add(TEXT("text crosses a control: ")+T.Text+TEXT(" / ")+H.Control.Label);
		for (const FBox2D& P : L.Paint.Panels) if (Overlap(T.Box,P) && !Inside(T.Box,P)) Errors.Add(TEXT("text crosses a panel: ")+T.Text);
	}
	for (int32 I=0; I<L.Hits.Num(); I++)
	{
		if (!Inside(L.Hits[I].Box,Screen)) Errors.Add(TEXT("control outside screen: ")+L.Hits[I].Control.Label);
		for (int32 J=I+1; J<L.Hits.Num(); J++) if (Overlap(L.Hits[I].Box,L.Hits[J].Box)) Errors.Add(TEXT("controls overlap: ")+L.Hits[I].Control.Label+TEXT(" / ")+L.Hits[J].Control.Label);
	}
	for (int32 I=0; I<L.Paint.Panels.Num(); I++)
	{
		if (!Inside(L.Paint.Panels[I],Screen)) Errors.Add(TEXT("panel outside screen"));
		for (int32 J=I+1; J<L.Paint.Panels.Num(); J++) if (Overlap(L.Paint.Panels[I],L.Paint.Panels[J])) Errors.Add(FString::Printf(TEXT("panels overlap: [%g,%g,%g,%g] / [%g,%g,%g,%g]"),L.Paint.Panels[I].Min.X,L.Paint.Panels[I].Min.Y,L.Paint.Panels[I].Max.X,L.Paint.Panels[I].Max.Y,L.Paint.Panels[J].Min.X,L.Paint.Panels[J].Min.Y,L.Paint.Panels[J].Max.X,L.Paint.Panels[J].Max.Y));
	}
	return Errors;
}

namespace
{
	constexpr double OrderNoteSeconds = 4.0, MarkerSeconds = 0.6;
	constexpr float Line = 16.f, Pad = 6.f, Gap = 8.f;
	const FLinearColor Text(1.f, 1.f, 1.f), Dim(0.75f, 0.78f, 0.72f), Warn(1.f, 0.6f, 0.2f), Good(0.75f, 0.9f, 1.f), Foe(1.f, 0.45f, 0.35f);
	const FLinearColor Panel(0.f, 0.f, 0.f, 0.55f), ButtonFill(0.18f, 0.24f, 0.16f, 0.9f);
	// recruit cards: buyable, buyable but the browser's 'broke' look cannot happen then, refused, refused for lack of MP
	// or fuel ('broke'), and the open group's tab
	const FLinearColor CardFill(0.20f, 0.27f, 0.17f, 0.92f), CardOff(0.16f, 0.16f, 0.16f, 0.85f), CardBroke(0.30f, 0.14f, 0.10f, 0.85f), TabOpen(0.32f, 0.40f, 0.22f, 0.95f);

	FString SeatName(const FTCMatchState& S, int32 Owner)
	{
		return S.Names.IsValidIndex(Owner) && !S.Names[Owner].IsEmpty() ? S.Names[Owner] : FString::Printf(TEXT("seat %d"), Owner);
	}

	// client/hud.js tagsOf: the status tags of a unit's row, from fields the snapshot carries (shared/sim.js UNIT_ROW,
	// unitFlags), in the browser's order and words (each one its own element there, so each is translated by itself:
	// the row joins them with column breaks). The supply indicator is the logistics rows' business here.
	TArray<FString> UnitTags(const FTCUnit& U, const FTCUnitDef* D)
	{
		TArray<FString> T;
		if (U.Flags & 256) T.Add(TEXT("Hidden"));
		if (U.Flags & TCFlags::Riding) T.Add(TEXT("Riding"));
		if (U.Flags & TCCargo::Flag)
		{
			const int32 N = TCCargo::Count(U.Flags), Of = D ? D->Carries : 0;
			T.Add(Of > 1 && Of <= 3 ? FString::Printf(TEXT("Carrying %d of %d squads"), N, Of) : FString(TEXT("Carrying a squad")));
		}
		if (U.Flags & TCFlags::Retreating) T.Add(TEXT("Retreating"));
		if (U.Flags & 8) T.Add(TEXT("Reinforcing"));
		if (U.Flags & 2) T.Add(TEXT("Suppressive fire"));
		if (U.Flags & 4) T.Add(TEXT("AP round loaded"));
		if (U.Supp >= 90) T.Add(TEXT("Pinned")); else if (U.Supp >= 50) T.Add(TEXT("Suppressed"));
		if (!(U.Flags & TCFlags::Garrisoned)) { if (U.Cover == 2) T.Add(TEXT("In trench")); else if (U.Cover == 3) T.Add(TEXT("By cover")); else if (U.Cover) T.Add(TEXT("In cover")); }
		if (U.Flags & 16) T.Add(TEXT("Digging")); else if (U.Flags & 1024) T.Add(TEXT("Waiting for MP to dig"));
		if (U.Flags & TCFlags::Garrisoned)
		{
			// shared/sim.js unitFlags bits 20-22: the CFG.houses type of the house it holds
			const TCWorldCore::FHouseType* H = FTCWorldObjects::GarrisonHouseType(U.Flags);
			T.Add(FString::Printf(TEXT("Garrisoned: %s"), H ? ANSI_TO_TCHAR(H->Name) : TEXT("?")));
		}
		if (U.Flags & 64) T.Add(TEXT("Attack-move"));
		// hud.js STANCE: hold fire, hold position, auto-retreat
		if (U.Flags & 2048) T.Add(TEXT("Hold fire"));
		if (U.Flags & 4096) T.Add(TEXT("Hold position"));
		if (U.Flags & 8192) T.Add(TEXT("Auto-retreat"));
		if (D && D->bBuilding && U.Built < 1.f) T.Add(FString::Printf(TEXT("Building %d%%"), FMath::RoundToInt(U.Built * 100.f)));
		// the veteran mark: the browser draws stars with the title "Veteran: n star(s)"
		if (U.Stars > 0) T.Add(U.Stars == 1 ? FString(TEXT("Veteran: 1 star")) : FString::Printf(TEXT("Veteran: %d stars"), U.Stars));
		return T;
	}

	// JavaScript's String(n) for the army numbers (0.75 per conscript squad, whole numbers otherwise)
	FString Js(double V) { return V == FMath::FloorToDouble(V) ? FString::Printf(TEXT("%.0f"), V) : FString::SanitizeFloat(V); }

	bool SameTarget(const FTCUiCommand& A, const FTCUiCommand& B)
	{
		if (A.Action != B.Action) return false;
		switch (A.Action)
		{
		case ETCUiAction::Recruit: return A.Unit == B.Unit;
		case ETCUiAction::CancelJob: return A.Building == B.Building && A.Job == B.Job;
		case ETCUiAction::RecruitGroup: return A.Group == B.Group;
		case ETCUiAction::Build: case ETCUiAction::Stance: case ETCUiAction::Ability: case ETCUiAction::Autocast:
		case ETCUiAction::Fort: case ETCUiAction::Entrench: case ETCUiAction::Formation: case ETCUiAction::Support: return A.Unit == B.Unit;
		case ETCUiAction::LobbySetting: case ETCUiAction::MatchOption: case ETCUiAction::MatchControl: return A.Unit == B.Unit && A.Group == B.Group;
		case ETCUiAction::LoadSave: case ETCUiAction::Research: return A.Unit == B.Unit;
		// round 16 (controls): the alert feed's hit boxes carry their tag (a line, the summary, Previous / Next, an entry)
		case ETCUiAction::Alert: return B.Unit.IsEmpty() || A.Unit == B.Unit;
		default: return true;
		}
	}
}

void ATCHUD::EndPlay(const EEndPlayReason::Type Reason)
{
	Portraits.Reset();
	Minimap.Reset(); // releases the texture with the HUD
	Super::EndPlay(Reason);
}

void ATCHUD::DrawLabel(const FString& Str, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale)
{
	const FString Shown = TC_T(Str);
	NoteKept(Str, Shown);
	DrawShown(Shown, Colour, X, Y, Font, Scale);
}

// index.html's body is 500 14px Barlow Semi Condensed; larger native headings keep their relative size.
namespace
{
	float HudFontPx(UFont* Font, float Scale)
	{
		return (GEngine && Font == GEngine->GetMediumFont() ? 18.f : 14.f) * Scale;
	}
}

void ATCHUD::DrawShown(const FString& Str, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale)
{
	if (!FTCType::IsLoaded()) { AHUD::DrawText(Str, Colour, X, Y, Font, Scale); return; }
	FTCPaint P;
	P.Text(Str, X, Y, HudFontPx(Font, Scale), Colour);
	FTCType::Draw(P, Canvas, Font, UiNow);
}

void ATCHUD::ShownSize(const FString& Str, float& OutWidth, float& OutHeight, UFont* Font, float Scale) const
{
	if (!FTCType::IsLoaded()) { AHUD::GetTextSize(Str, OutWidth, OutHeight, Font, Scale); return; }
	const float Px = HudFontPx(Font, Scale);
	OutWidth = FTCType::Width(Str, Px);
	OutHeight = 1.2f * Px;
}

void ATCHUD::NoteKept(const FString& Str, const FString& Shown)
{
	if (Shown == Str && FTCLocale::Language() == ETCLanguage::Spanish && KeptEnglish.Num() < 400)
	{
		// one log line per text shape: runs of digits count as one (a status line's tick, a cost, a count)
		FString Shape;
		bool bLetter = false;
		for (int32 I = 0; I < Str.Len(); I++)
		{
			const TCHAR C = Str[I];
			bLetter |= (C >= 'a' && C <= 'z') || (C >= 'A' && C <= 'Z');
			if (C >= '0' && C <= '9') { if (Shape.IsEmpty() || Shape[Shape.Len() - 1] != '#') Shape.AppendChar('#'); }
			else Shape.AppendChar(C);
		}
		if (bLetter && !KeptEnglish.Contains(Shape)) { KeptEnglish.Add(Shape); UE_LOG(LogTC, Log, TEXT("locale: kept in English (%d so far): '%s'"), KeptEnglish.Num(), *Str); }
	}
}

FString ATCHUD::FitShown(const FString& Shown, float MaxWidth, UFont* Font) const
{
	float TW = 0, TH = 0;
	ShownSize(Shown, TW, TH, Font, 1.f);
	if (TW <= MaxWidth || Shown.Len() < 2) return Shown;
	// the longest head that fits with '...' after it (binary search on the length)
	int32 Lo = 0, Hi = Shown.Len();
	while (Lo < Hi)
	{
		const int32 Mid = (Lo + Hi + 1) / 2;
		ShownSize(Shown.Left(Mid).TrimEnd() + TEXT("..."), TW, TH, Font, 1.f);
		if (TW <= MaxWidth) Lo = Mid; else Hi = Mid - 1;
	}
	return Shown.Left(Lo).TrimEnd() + TEXT("...");
}

TArray<FString> ATCHUD::WrapShown(const FString& Shown, float MaxWidth, UFont* Font) const
{
	// the line's own columns first (' | ', and the HUD's '  |  '), packed greedily; a column still too wide is broken
	// at its spaces, and a single word too wide is cut
	auto Width = [&](const FString& T) { float TW = 0, TH = 0; ShownSize(T, TW, TH, Font, 1.f); return TW; };
	if (Width(Shown) <= MaxWidth) return { Shown };
	TArray<FString> Parts;
	Shown.ParseIntoArray(Parts, TEXT(" | "), true);
	TArray<FString> Out;
	FString Cur;
	auto Push = [&](const FString& Piece, const TCHAR* Sep)
	{
		const FString Try = Cur.IsEmpty() ? Piece : Cur + Sep + Piece;
		if (Cur.IsEmpty() || Width(Try) <= MaxWidth) { Cur = Try; return; }
		Out.Add(Cur);
		Cur = Piece;
	};
	for (FString P : Parts)
	{
		P.TrimStartAndEndInline();
		if (Width(P) <= MaxWidth) { Push(P, TEXT(" | ")); continue; }
		TArray<FString> Words;
		P.ParseIntoArray(Words, TEXT(" "), true);
		bool bFirst = true;
		for (const FString& W : Words) { Push(Width(W) <= MaxWidth ? W : FitShown(W, MaxWidth, Font), bFirst ? TEXT(" | ") : TEXT(" ")); bFirst = false; }
	}
	if (!Cur.IsEmpty()) Out.Add(Cur);
	return Out;
}

// ---------------------------------------------------------------------------------------------------------------------
// Round 16 (hudfit): the fit rules (TCHUD.h FTCHudFit) and the match controls where the browser has them

FString FTCHudFit::HordeRecordText(int32 Defenders, bool bBest, int32 Wave, int32 Time, const TArray<FString>& Names)
{
	// main.js: (best ? ` Record here with ${n} defender${n === 1 ? '' : 's'}: wave ${best.wave} in ${mins(best.time)}
	// (${best.names.join(', ')}).` : ` No record yet here with ${n} defender${n === 1 ? '' : 's'}.`), mins: m:ss
	const TCHAR* Plural = Defenders == 1 ? TEXT("") : TEXT("s");
	if (!bBest) return FString::Printf(TEXT(" No record yet here with %d defender%s."), Defenders, Plural);
	return FString::Printf(TEXT(" Record here with %d defender%s: wave %d in %d:%02d (%s)."), Defenders, Plural, Wave, Time / 60, Time % 60, *FString::Join(Names, TEXT(", ")));
}

FString FTCHudFit::ConnectionBannerText(bool bFullRoom, int32 SecondsLeft)
{
	// main.js: reason === 'full' ? `Trying this room again in ${left} s` : `Connection lost. Retrying in ${left} s`
	return bFullRoom ? FString::Printf(TEXT("Trying this room again in %d s"), SecondsLeft) : FString::Printf(TEXT("Connection lost. Retrying in %d s"), SecondsLeft);
}

FString FTCHudFit::HordeRecordText(const FTCMatchState& S)
{
#if defined(TC_HAS_LOBBY_HORDE_BEST)
	// main.js: n = m.players.length
	return HordeRecordText(S.Roster.Num(), S.LobbyHordeBest.bSet, S.LobbyHordeBest.Wave, S.LobbyHordeBest.Time, S.LobbyHordeBest.Names);
#else
	return FString(); // without the lobby's hordeBest (lead patch round-17/hudui/lead-patch/horde-record.patch): no line
#endif
}

FTCUiCommand ATCHUD::ControlCommand(const FString& Key, int32 Slot)
{
	static const TPair<const TCHAR*, ETCUiAction> Settings[] = {
		{ TEXT("Setting.Graphics"), ETCUiAction::Graphics }, { TEXT("Setting.Gore"), ETCUiAction::Gore }, { TEXT("Setting.EdgeScroll"), ETCUiAction::EdgeScroll },
		{ TEXT("Setting.PanSpeed"), ETCUiAction::PanSpeed }, { TEXT("Setting.Cover"), ETCUiAction::CoverToggle }, { TEXT("Setting.Volume"), ETCUiAction::Volume },
		{ TEXT("Setting.UiScale"), ETCUiAction::UiScale }, { TEXT("Setting.Language"), ETCUiAction::Language } };
	if (Key.StartsWith(TEXT("Setting.")))
		for (const auto& P : Settings) if (Key == P.Key) return FTCUiCommand(P.Value);
	FTCUiCommand C(ETCUiAction::MatchControl);
	C.Unit = Key;
	C.Group = Slot;
	return C;
}

TArray<FTCControlRow> ATCHUD::MenuSettingRows() const
{
	// client/index.html #menu after the session buttons: light.js puts Graphics and Gore before Leave (SplitMatchControls
	// places them there); then Edge scroll, Pan speed, cover-preview.js's switch (before the first .vol), Volume and the
	// UI scale. The browser's Capture mouse and Stats overlay buttons have no native switch here. The language picker
	// is the page header's (#langSel), which a native match has nowhere else: it closes the list.
	TArray<FTCControlRow> Rows;
	const ATCGameMode* M = GetWorld() ? GetWorld()->GetAuthGameMode<ATCGameMode>() : nullptr;
	const ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC) return Rows;
	auto Add = [&](const FString& Label, const TCHAR* Key, const FString& Hover, bool bOn)
	{
		FTCControlRow R;
		FTCControlItem I; I.Label = Label; I.Key = Key; I.Hover = Hover; I.bOn = bOn;
		R.Items.Add(I);
		Rows.Add(R);
	};
	Add(FTCGraphics::MenuLabel(FTCGraphics::Get().Level()), TEXT("Setting.Graphics"), FTCGraphics::MenuTip(), FTCGraphics::Get().IsLow());
	Add(FTCGraphics::GoreLabel(FTCGraphics::Get().Gore()), TEXT("Setting.Gore"), FTCGraphics::GoreTip(), false);
	if (!M->State.bSpectator)
	{
		// client/camera.js preferences, the menu's Edge scroll and Pan speed (saved)
		Add(PC->EdgeScrollLabel(), TEXT("Setting.EdgeScroll"), TEXT("Edge scroll: the view pans when the pointer is at the window's edge"), PC->EdgeScrollOn());
		Add(PC->PanSpeedLabel(), TEXT("Setting.PanSpeed"), TEXT("Pan speed: Slow, Normal or Fast for the keys and the edges"), false);
		Add(FTCCoverPreview::MenuLabel(M->Cover.IsEnabled()), TEXT("Setting.Cover"), FTCCoverPreview::MenuTip(), M->Cover.IsEnabled());
	}
	Add(PC->VolumeLabel(), TEXT("Setting.Volume"), TEXT("Effects, voices and alerts. M mutes (right click: quieter)"), false);
	Add(PC->UiScaleLabel(), TEXT("Setting.UiScale"), FTCSettingsStore::UIScaleTip(), false);
	Add(PC->LanguageLabel(), TEXT("Setting.Language"), TEXT("Language"), false);
	return Rows;
}

void ATCHUD::SplitMatchControls(TArray<FTCControlRow>& Corner, TArray<FTCControlRow>& Sandbox) const
{
	// ATCGameMode::MatchControlRows (Client/TCMatchControls.cpp) in the browser's places: the Menu button and the open
	// menu in the corner, with the settings in it (Graphics and Gore before Leave, the rest after it); the sandbox row
	// apart (#sandbox); Send next wave left out (DrawScoreStrip draws it in the Horde lead block). Rows this does not
	// know stay in the corner.
	Corner.Reset(); Sandbox.Reset();
	const ATCGameMode* M = GetWorld() ? GetWorld()->GetAuthGameMode<ATCGameMode>() : nullptr;
	if (!M) return;
	auto Has = [](const FTCControlRow& R, const TCHAR* Key) { for (const FTCControlItem& I : R.Items) if (I.Key == Key) return true; return false; };
	TArray<FTCControlRow> Settings = bSettingsInMenu && M->Controls.MenuOpen() ? MenuSettingRows() : TArray<FTCControlRow>();
	const int32 BeforeLeave = FMath::Min(2, Settings.Num());
	bool bPlaced = Settings.Num() == 0;
	for (const FTCControlRow& R : M->MatchControlRows())
	{
		if (Has(R, TEXT("NextWave"))) continue;
		if (Has(R, TEXT("SandboxUnit")) || Has(R, TEXT("SandboxSide")) || Has(R, TEXT("SandboxSpawn"))) { Sandbox.Add(R); continue; }
		if (!bPlaced && Has(R, TEXT("Leave")))
		{
			for (int32 K = 0; K < BeforeLeave; K++) Corner.Add(Settings[K]);
			Corner.Add(R);
			for (int32 K = BeforeLeave; K < Settings.Num(); K++) Corner.Add(Settings[K]);
			bPlaced = true;
			continue;
		}
		Corner.Add(R);
	}
	// no Leave (a spectator's menu): the settings after the session buttons
	if (!bPlaced) Corner.Append(Settings);
}

FVector2D ATCHUD::MeasureControlRows(UFont* Font, const TArray<FTCControlRow>& Rows)
{
	// the size DrawControlRows gives its panel at W = 0 (as wide as the widest row), with its hover line
	if (!Rows.Num()) return FVector2D::ZeroVector;
	auto Measure = [&](const FString& Lb) { float TW = 0, TH = 0; LabelSize(Lb, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	float H = 2 * Pad + Line, Wide = 0.f;
	for (const FTCControlRow& R : Rows)
	{
		H += R.Items.Num() ? Line + Pad + 4.f : Line;
		float RW = (R.Text.IsEmpty() ? 0.f : Measure(R.Text)) + 2 * Pad;
		for (const FTCControlItem& It : R.Items) RW += Measure(It.Label) + Pad;
		Wide = FMath::Max(Wide, RW);
	}
	return FVector2D(Wide, H);
}

FVector2D FTCHudFit::InitialCameraBand(float ScreenH, float UiScale, bool bBuyShown)
{
	const float Border = FMath::Max(1.f, FMath::FloorToFloat(UiScale));
	// Empty #scores: 8px top gap, 17px padding, two snapped borders. #buy's tabs: 46px plus four borders.
	return FVector2D(25.f * UiScale + 2 * Border, bBuyShown ? ScreenH - (54.f * UiScale + 4 * Border) : ScreenH);
}

void ATCHUD::LabelSize(const FString& Str, float& OutWidth, float& OutHeight, UFont* Font, float Scale) const
{
	ShownSize(TC_T(Str), OutWidth, OutHeight, Font, Scale);
}

void ATCHUD::DrawMinimapPings()
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	if (!M || !Minimap.Rect().bIsValid || !Canvas) return;
	const double Now = FPlatformTime::Seconds(); // the clock the relay was received on (ATCGameMode::OnPingRelay)
	const FLinearColor Ink(FColor(0x15, 0x16, 0x0f)), Chalk(FColor(0xf2, 0xec, 0xdc)); // pings.js stroke and COLOR (markers.js CHALK)
	for (const FTCPingRing& R : M->AlertFeed.Pings().Rings())
	{
		FTCPingLook L;
        if (!M->AlertFeed.Pings().LookOf(R, Now, L)) continue;
        FLinearColor Under=Ink, Over=Chalk; Under.A=Over.A=L.MinimapAlpha;
        Minimap.DrawRing(Canvas,FVector2D(R.X,R.Z),L.MinimapPx,4,Under);
        Minimap.DrawRing(Canvas,FVector2D(R.X,R.Z),L.MinimapPx,2,Over);
	}
}

float ATCHUD::Button(UFont* Font, float X, float Y, const FString& Label, ETCUiAction Action)
{
	return Button(Font, X, Y, Label, FTCUiCommand(Action), true, ButtonFill);
}

float ATCHUD::Button(UFont* Font, float X, float Y, const FString& Label, const FTCUiCommand& Command, bool bEnabled, const FLinearColor& Fill, float MinWidth, float MaxWidth)
{
	float TW = 0, TH = 0;
	LabelSize(Label, TW, TH, Font, 1.f);
	float BW = FMath::Max(TW + 2 * Pad, MinWidth);
	if (MaxWidth > 0.f) BW = FMath::Min(BW, MaxWidth);
	const FBox2D Rect(FVector2D(X, Y), FVector2D(X + BW, Y + TH + Pad));
	DrawRect(Fill, Rect.Min.X, Rect.Min.Y, Rect.GetSize().X, Rect.GetSize().Y);
	if (TW + 2 * Pad <= BW) DrawLabel(Label, bEnabled ? Text : Dim, X + Pad, Y + Pad / 2, Font, 1.f);
	else
	{
		// round 15: a card narrower than its label (a narrow recruit card) cuts it, as the browser's names ellipsize
		const FString Shown = TC_T(Label);
		NoteKept(Label, Shown);
		DrawShown(FitShown(Shown, BW - 2 * Pad, Font), bEnabled ? Text : Dim, X + Pad, Y + Pad / 2, Font, 1.f);
	}
	Buttons.Add({ ToScreen(Rect), Command, Label, bEnabled });
	return Rect.Max.X + Pad;
}

void ATCHUD::AddPanel(float X, float Y, float W, float H, const FLinearColor& Colour)
{
	DrawRect(Colour, X, Y, W, H);
	Panels.Add(ToScreen(FBox2D(FVector2D(X, Y), FVector2D(X + W, Y + H))));
}

void ATCHUD::BeginUi(float Scale)
{
	if (bUiPushed || !Canvas || !Canvas->Canvas) return;
	UiNow = FMath::Max(0.1f, Scale);
	Canvas->Canvas->PushRelativeTransform(FScaleMatrix(FVector(UiNow, UiNow, 1.f)));
	UiOldClipX = Canvas->ClipX; UiOldClipY = Canvas->ClipY;
	Canvas->ClipX = UiOldClipX / UiNow; Canvas->ClipY = UiOldClipY / UiNow;
	bUiPushed = true;
}

void ATCHUD::EndUi()
{
	if (!bUiPushed) return;
	Canvas->Canvas->PopTransform();
	Canvas->ClipX = UiOldClipX; Canvas->ClipY = UiOldClipY;
	UiNow = 1.f;
	bUiPushed = false;
}

bool ATCHUD::ButtonAt(FVector2D Pixel, ETCUiAction& OutAction) const
{
	FTCUiCommand C;
	if (!CommandAt(Pixel, C)) return false;
	OutAction = C.Action;
	return true;
}

bool ATCHUD::CommandAt(FVector2D Pixel, FTCUiCommand& OutCommand) const
{
	// round 16: the last drawn first, so a panel drawn over another (the open match menu) takes the click
	for (int32 I = Buttons.Num() - 1; I >= 0; I--) if (Buttons[I].Rect.IsInsideOrOn(Pixel)) { OutCommand = Buttons[I].Command; return true; }
	return false;
}

bool ATCHUD::ButtonCentre(ETCUiAction Action, FVector2D& Out) const
{
	for (const FButton& B : Buttons) if (B.Command.Action == Action) { Out = B.Rect.GetCenter(); return true; }
	return false;
}

bool ATCHUD::CommandCentre(const FTCUiCommand& Command, FVector2D& Out) const
{
	for (const FButton& B : Buttons) if (SameTarget(B.Command, Command)) { Out = B.Rect.GetCenter(); return true; }
	return false;
}

bool ATCHUD::PanelAt(FVector2D Pixel) const
{
	for (const FBox2D& P : Panels) if (P.IsInsideOrOn(Pixel)) return true;
	return false;
}

FString ATCHUD::ButtonLabels() const
{
	TArray<FString> L;
	for (const FButton& B : Buttons) L.Add(B.Label);
	return FString::Join(L, TEXT(", "));
}

float ATCHUD::DrawLobby(UFont* Font, float Y, float LobbyW)
{
	// client/index.html #lobby as client/main.js renderLobby fills it: the room line, the settings (FTCLobbySettings:
	// each select is a button that cycles its choices, left click forward, right click back, greyed when this client
	// may not change it), the seats with their team, faction and AI difficulty, the Start button's text and the line
	// under it, and the saves panel (FTCPersistence) with a Load button per row for the host
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	const FTCMatchState& S = M->State;
	if (S.RoomState.IsEmpty()) return Y; // no lobby yet: the status line says what the connection is doing
	const FTCLobbySettings& L = M->Lobby;
	struct FItem { FString Label; FTCUiCommand Command; bool bEnabled; FString Hover; };
	// MaxW (round 16): a text line's own wrap width (#modeInfo's max-width: 60ch), 0 for the panel's
	struct FLine { FString Text; FLinearColor Colour; TArray<FItem> Items; float MaxW = 0.f; };
	TArray<FLine> Lines;
	const float W = LobbyW;
	if (8.f + W + 170.f < Canvas->ClipX) M->Options.DrawPreview(Canvas, Font, FVector2D(W + 20.f, Y + 8.f));
	auto Measure = [&](const FString& Lb) { float TW = 0, TH = 0; LabelSize(Lb, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	// buttons in rows that wrap at the panel's width, the first row after an optional text
	auto AddButtons = [&](const TArray<FItem>& Items, const FString& Lead = FString(), FLinearColor Colour = FLinearColor::White)
	{
		FLine Cur{ Lead, Colour, {} };
		float X = Lead.IsEmpty() ? 0.f : Measure(Lead);
		for (const FItem& It : Items)
		{
			const float IW = Measure(It.Label);
			if (Cur.Items.Num() && X + IW > W - 2 * Pad) { Lines.Add(Cur); Cur = FLine{ FString(), Text, {} }; X = 0; }
			Cur.Items.Add(It); X += IW + Pad;
		}
		if (Cur.Items.Num() || !Cur.Text.IsEmpty()) Lines.Add(Cur);
	};
	auto ControlItem = [](const FTCLobbyControl& C)
	{
		FTCUiCommand Cmd(ETCUiAction::LobbySetting);
		Cmd.Unit = LexToString(C.Kind);
		Cmd.Group = C.Slot;
		// round 16: a seat's selects (team, faction, AI difficulty) show their choice alone, as client/main.js renderLobby's
		// pick() <select>s in the roster row do ('Team 1', the faction's name, 'Normal'); the room's selects keep the
		// 'Title: ' their page labels give them. The title stays in the hover line.
		const bool bSeat = C.Kind == ETCLobbySetting::Team || C.Kind == ETCLobbySetting::Faction || C.Kind == ETCLobbySetting::Level;
		FString Label = bSeat ? (C.CurrentLabel.IsEmpty() ? FString(TEXT("?")) : C.CurrentLabel) : FString::Printf(TEXT("%s: %s"), *C.Title, C.CurrentLabel.IsEmpty() ? TEXT("?") : *C.CurrentLabel);
		if (C.bPending)
		{
			const FTCLobbyChoice* P = C.Find(C.PendingValue);
			Label += FString::Printf(TEXT(" (asked: %s)"), P ? *P->Label : *C.PendingValue);
		}
		return FItem{ Label, Cmd, C.bEditable, C.bEditable ? FString::Printf(TEXT("%s: left click for the next choice, right click for the previous"), *C.Title) : C.Why };
	};
	const bool bLobby = S.RoomState == TEXT("lobby");
	const FTCLobbyRoom& R = L.Room();
	const FString ModeLabel = L.HasLobby() ? FTCLobbySettings::ModeName(R.Mode) : S.Mode;
	const FString MapLabel = !L.HasLobby() ? S.LobbyMap : R.Mode == TEXT("world") ? FTCLobbySettings::WorldSizeLabel(R.WorldSize) + TEXT(" world") : FTCLobbySettings::PrettyMap(R.MapName);
	Lines.Add({ FString::Printf(TEXT("Lobby  room %s  |  %s  |  %s%s"), *S.RoomCode, *ModeLabel, *MapLabel, S.bLobbySandbox ? TEXT("  |  sandbox") : TEXT("")), Text, {} });
	// client/index.html: the page header's language picker (#langSel, title "Language"), there for everyone
	if (const ATCPlayerController* LPC = Cast<ATCPlayerController>(GetOwningPlayerController()))
		AddButtons({ FItem{ LPC->LanguageLabel(), FTCUiCommand(ETCUiAction::Language), true, TEXT("Language") } });
	// round 14: client/public-lobby.html's Tutorial button (this client has no room directory, CLI-01: it sits in the
	// room's lobby), for anyone outside a match
	if (bLobby && !L.IsRustHost()) AddButtons({ FItem{ TCTutorialEntry::Label(), FTCUiCommand(ETCUiAction::Tutorial), true, TCTutorialEntry::Title() } });
	if (L.HasLobby() && bLobby)
	{
		// Keep MODE_INFO whole for its exact es.js key before appending the dynamic record sentence.
		FString Info = TC_T(FTCLobbySettings::ModeInfo(R.Mode));
		// main.js renderLobby appends server.js hordeBest; its separate dynamic key is translated by the line painter.
		if (R.Mode == TEXT("horde") && !Info.IsEmpty()) Info += FTCHudFit::HordeRecordText(S);
		// round 16: index.html #modeInfo { max-width: 60ch } in the card's text, wrapped (white-space: normal): 60 times
		// the width of the font's '0', within the panel
		float ZeroW = 0.f, ZeroH = 0.f;
		ShownSize(TEXT("0"), ZeroW, ZeroH, Font, 1.f);
		if (!Info.IsEmpty()) Lines.Add({ Info, Dim, {}, 60.f * ZeroW });
		TArray<FItem> Room;
		for (const FTCLobbyControl& C : L.RoomControls()) if (C.bShown) Room.Add(ControlItem(C));
		// client/index.html: the Weather select and the Sandbox checkbox (FTCMatchOptions), greyed for a guest
		for (const ETCMatchOption K : { ETCMatchOption::Weather, ETCMatchOption::Sandbox })
		{
			const FTCOptionControl C = M->Options.Control(K);
			if (!C.bShown) continue;
			FTCUiCommand Cmd(ETCUiAction::MatchOption);
			Cmd.Unit = LexToString(K);
			FString Label = FString::Printf(TEXT("%s: %s"), *C.Title, C.CurrentLabel.IsEmpty() ? TEXT("?") : *C.CurrentLabel);
			if (C.bPending)
			{
				const FTCLobbyChoice* P = C.Find(C.PendingValue);
				Label += FString::Printf(TEXT(" (asked: %s)"), P ? *P->Label : K == ETCMatchOption::Sandbox ? (C.PendingValue == TEXT("true") ? TEXT("On") : TEXT("Off")) : *C.PendingValue);
			}
			const FString Hover = !C.bEditable ? C.Why : K == ETCMatchOption::Weather ? FString::Printf(TEXT("%s. Left click for the next choice, right click for the previous"), *C.Tooltip) : FString::Printf(TEXT("%s. Click to switch"), *C.Tooltip);
			Room.Add({ Label, Cmd, C.bEditable, Hover });
		}
		AddButtons(Room);
		if (!M->Options.LogisticsLine().IsEmpty()) Lines.Add({ M->Options.LogisticsLine(), Dim, {} });
	}
	for (int32 I = 0; I < S.Roster.Num(); I++)
	{
		const FTCLobbyPlayer& P = S.Roster[I];
		const bool bYou = I == S.LobbySeat && !S.bSpectator;
		// client/main.js renderLobby marks only an offline player ('offline'); AI seats show their level
		const FString Who = P.bAi ? FString(TEXT("  AI")) : P.bConnected ? FString() : FString(TEXT("  offline"));
		const FString Seat = FString::Printf(TEXT("%d. %s%s%s  "), I + 1, *P.Name, bYou ? TEXT(" (you)") : TEXT(""), *Who);
		TArray<FItem> Items;
		if (L.HasLobby() && bLobby && I < R.Seats.Num())
			for (const FTCLobbyControl& C : L.SeatControls(I)) if (C.bShown) Items.Add(ControlItem(C));
		// client/main.js renderLobby: the host's Remove button on an AI seat or an offline player's seat
		const FTCOptionControl Kick = M->Options.Control(ETCMatchOption::Kick, I);
		if (bLobby && Kick.bShown)
		{
			FTCUiCommand Cmd(ETCUiAction::MatchOption);
			Cmd.Unit = LexToString(ETCMatchOption::Kick);
			Cmd.Group = I;
			Items.Add({ Kick.bPending ? FString(TEXT("Remove (asked)")) : Kick.Title, Cmd, Kick.bEditable, Kick.bEditable ? Kick.Tooltip : Kick.Why });
		}
		if (Items.Num()) AddButtons(Items, Seat, bYou ? Good : Text);
		// client/main.js renderLobby: 'Team ' + (k + 1), the teams numbered from 1 (es.js 'Team {0}')
		else Lines.Add({ FString::Printf(TEXT("%sTeam %d%s"), *Seat, P.Team + 1, P.bAi ? *FString::Printf(TEXT("  %s"), *P.Level) : TEXT("")), bYou ? Good : Text, {} });
	}
	if (S.Spectators.Num()) Lines.Add({ TEXT("Watching: ") + FString::Join(S.Spectators, TEXT(", ")), Dim, {} });
	// round 15 (Client/TCMatchControls.cpp): the Name field, Watch as a spectator / Take a seat, the invite link and
	// Copy, the Room box (Join, New) and Browse matches
	for (const FTCControlRow& Row : M->LobbyControlRows())
	{
		TArray<FItem> Items;
		for (const FTCControlItem& It : Row.Items) { FTCUiCommand C(ETCUiAction::MatchControl); C.Unit = It.Key; C.Group = It.Slot; const bool bRustWatch = L.IsRustHost() && It.Key == TEXT("Watch"); Items.Add({ It.Label, C, It.bEnabled && !bRustWatch, bRustWatch ? FString(TEXT("Rust host: no spectators.")) : It.Hover }); }
		const FLinearColor Colour = Row.Tone == FTCControlRow::Dim ? Dim : Row.Tone == FTCControlRow::Warn ? Warn : Row.Tone == FTCControlRow::Good ? Good : Text;
		if (Items.Num()) AddButtons(Items, Row.Text, Colour); else Lines.Add({ Row.Text, Colour, {} });
	}
	if (!S.LastResult.IsEmpty()) Lines.Add({ FString::Printf(TEXT("Last match: %s."), *S.LastResult), Dim, {} });
	const bool bHost = S.bAmHost && !S.bSpectator;
	// round 16: not after a Leave (main.js on left: #lobbyMsg says only "You left the match; ...", the first row here)
	if (S.RoomState == TEXT("play") && !M->Controls.Left()) Lines.Add({ TEXT("A match is running: joining it..."), Dim, {} });
	else if (S.bSpectator) Lines.Add({ TEXT("You are watching this room."), Dim, {} });
	// (a guest's "Waiting for the host to start..." is FTCLobbySettings::StartLine, the browser's #lobbyMsg text)
	if (S.bAmHost && bLobby)
	{
		// the Start button's text and enabled state are the browser's (renderLobby); Add AI as before
		TArray<FItem> Host;
		if (bHost && !L.IsRustHost()) Host.Add({ TEXT("+ Add AI player"), FTCUiCommand(ETCUiAction::AddAi), true, TEXT("+ Add AI player") }); // lobby-view.js
		const bool bStart = !L.HasLobby() || L.StartAllowed();
		Host.Add({ L.HasLobby() ? L.StartLabel() : FString(TEXT("Start match")), FTCUiCommand(ETCUiAction::Start), bStart, bStart ? FString(TEXT("Start the match")) : L.StartLine() });
		AddButtons(Host);
	}
	if (L.HasLobby() && bLobby && !L.StartLine().IsEmpty()) Lines.Add({ L.StartLine(), Warn, {} });
	if (M->Options.InLobby())
	{
		// weather-view.js: the chosen weather explained under the select
		const FTCOptionControl Wc = M->Options.Control(ETCMatchOption::Weather);
		if (Wc.bShown && !Wc.Tooltip.IsEmpty()) Lines.Add({ FString::Printf(TEXT("Weather: %s"), *Wc.Tooltip), Dim, {} });
	}
	if (M->Options.PendingCount()) Lines.Add({ FString::Printf(TEXT("%d option change(s) asked, not shown by a lobby yet"), M->Options.PendingCount()), Dim, {} });
	if (L.PendingCount()) Lines.Add({ FString::Printf(TEXT("%d change(s) asked, not shown by a lobby yet (the server answers a change it takes with a lobby, and ignores one it refuses)"), L.PendingCount()), Dim, {} });
	// the saves panel: rows newest first, a Load button on each for the host
	const FTCSaveListView Saves = M->Persistence.SaveList(S);
	if (Saves.bShown)
	{
		Lines.Add({ Saves.Heading, Text, {} });
		for (const FTCSaveRow& Row : Saves.Rows)
		{
			FTCUiCommand Load(ETCUiAction::LoadSave);
			Load.Unit = Row.Id;
			TArray<FItem> Items;
			if (Saves.bLoadButtons) Items.Add({ Saves.LoadLabel, Load, true, FString::Printf(TEXT("Load %s"), *Row.Id) });
			FLine Ln{ FString::Printf(TEXT("%s  |  %s"), *Row.Title, *Row.Detail), Dim, Items };
			Lines.Add(Ln);
		}
	}
	const FTCPersistenceStatus& St = M->Persistence.Status();
	if (St.Request != ETCPersistenceCommand::None) Lines.Add({ St.Describe(), Dim, {} });

	// round 16: each text line in the language in use, wrapped at the panel's width (or its own MaxW) as the card's
	// text wraps (white-space: normal); before, a long line (Horde's description with its record) ran past the panel
	TArray<FLine> Wrapped;
	TArray<bool> bShownText;
	for (const FLine& Ln : Lines)
	{
		if (Ln.Items.Num()) { Wrapped.Add(Ln); bShownText.Add(false); continue; }
		const FString T = TC_T(Ln.Text);
		NoteKept(Ln.Text, T);
		const float Wrap = Ln.MaxW > 0.f ? FMath::Min(Ln.MaxW, W - 2 * Pad - 6.f) : W - 2 * Pad - 6.f;
		for (const FString& Piece : FTCPaint::Wrap(T, Wrap, 0.f, false, [&](const FString& Q, float, bool, float) { float TW = 0, TH = 0; ShownSize(Q, TW, TH, Font, 1.f); return TW; }))
		{
			Wrapped.Add({ Piece, Ln.Colour, {} });
			bShownText.Add(true);
		}
	}
	Lines = MoveTemp(Wrapped);
	const float RowH = Line + Pad + 4.f;
	float H = 2 * Pad + Line; // the hover line
	for (const FLine& Ln : Lines) H += Ln.Items.Num() ? RowH : Line;
	AddPanel(8.f, Y, W, H, Panel);
	FVector2D Mouse(-1, -1);
	float MX, MY;
	const bool bMouse = GetOwningPlayerController() && GetOwningPlayerController()->GetMousePosition(MX, MY);
	if (bMouse) Mouse = FVector2D(MX, MY) / UiNow; // in the scaled panels' own pixels
	FString Hover;
	float RowY = Y + Pad;
	for (int32 Li = 0; Li < Lines.Num(); Li++)
	{
		const FLine& Ln = Lines[Li];
		if (!Ln.Items.Num()) { if (bShownText[Li]) DrawShown(Ln.Text, Ln.Colour, 14.f, RowY, Font, 1.f); else DrawLabel(Ln.Text, Ln.Colour, 14.f, RowY, Font, 1.f); RowY += Line; continue; }
		float X = 14.f;
		// a lead text before the buttons (a seat's name), or a trailing text after them (a save's row)
		const bool bLead = !Ln.Text.IsEmpty() && Ln.Items[0].Command.Action != ETCUiAction::LoadSave;
		if (bLead) { DrawLabel(Ln.Text, Ln.Colour, X, RowY + 2.f + Pad / 2, Font, 1.f); X += Measure(Ln.Text) - Pad; }
		for (const FItem& It : Ln.Items)
		{
			const float Left = X;
			X = Button(Font, X, RowY + 2.f, It.Label, It.Command, It.bEnabled, It.bEnabled ? ButtonFill : CardOff);
			if (bMouse && FBox2D(FVector2D(Left, RowY + 2.f), FVector2D(X - Pad, RowY + 2.f + Line + Pad)).IsInsideOrOn(Mouse)) Hover = It.Hover;
		}
		if (!bLead && !Ln.Text.IsEmpty()) DrawLabel(Ln.Text, Ln.Colour, X, RowY + 2.f + Pad / 2, Font, 1.f);
		RowY += RowH;
	}
	if (!Hover.IsEmpty()) DrawLabel(Hover, Good, 14.f, RowY, Font, 1.f);
	return Y + H + 4.f;
}

float ATCHUD::DrawMatchPanel(UFont* Font, float Width, float TopLimit, bool bDraw)
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	const FTCMatchState& S = M->State;
	TArray<FTCHudFit::FSelection> Selection;
	TArray<int32> SelectedIds=PC->Selected.Array(); SelectedIds.Sort();
	for (int32 Id : SelectedIds)
	{
		const FTCUnit* U=S.Units.Find(Id); if (!U) continue;
		int32 At=Selection.IndexOfByPredicate([&](const FTCHudFit::FSelection& R) { return R.Type==U->Type; });
		if (At==INDEX_NONE)
		{
			FTCHudFit::FSelection R; R.Type=U->Type; R.Name=FTCAlerts::UnitName(S,&M->UnitTable,U->Type,U->Owner); R.Count=0; R.Owner=U->Owner;
			At=Selection.Add(MoveTemp(R));
		}
		FTCHudFit::FSelection& R=Selection[At]; const FTCUnitDef* D=M->UnitTable.Find(U->Type);
		R.Count++; R.Hp+=U->Hp; R.MaxHp+=D?D->MaxHp():0;
		for (const FString& Tag : UnitTags(*U,D)) R.Tags.AddUnique(Tag);
		const FString Supply=M->Logistics.IndicatorFor(U->Id); if (!Supply.IsEmpty()) R.Tags.AddUnique(Supply);
		R.Hover=R.Name+TEXT(". Click to keep this type; Shift+click removes it");
	}
	TArray<FString> Details;
	// The source hint has its own bounded line above the Command Card.

	// The Form submenu owns its shape and spacing values, as in hud.js. A second always-visible summary squeezed
	// selection and ability rows together and repeated those values outside the browser's layout.
	// The buttons (client/hud.js drawOrders), in rows that wrap at the panel's width: Stop and Attack-move always; for a
	// selection that is neither all buildings nor all trucks, Retreat, the three stances and (with a builder squad)
	// the Build menu; Resume deliveries for an all-truck selection; the logistics switches; End match for the host.
	// Slice 1f: the support bar (client/hud.js buildSupport and drawEcon) in its own row first, the Shell area button
	// with a salvo unit selected, the Form menu and the cover preview switch.
	// Compact (round 16): the label at fit level 1 and up (the support bar: what the browser's button shows, its key and
	// its cost; the browser draws the power's icon where this has none, and its title names it, as Hover does here)
	struct FItem { FString Label; FTCUiCommand Command; bool bEnabled; FLinearColor Fill; FString Hover; FString Compact; };
	TArray<FItem> Bar, Orders, Menu, LogisticsButtons;
	const float W = Width;
	if (!S.bSpectator && S.SeatConfirmed())
		for (const FTCSupportButton& B : M->Powers.Buttons(S, M->UnitTable, M->Recruit))
		{
			const FTCSupportDef* D = FTCSupportPowers::Find(B.Kind);
			FTCUiCommand C(ETCUiAction::Support);
			C.Unit = B.Kind;
			const bool bAimed = PC->Aim == ETCAim::Support && PC->PowerAim.Kind == ETCSupportAimKind::Support && PC->PowerAim.SupportKind == B.Kind;
			// the browser never disables a support button: a click on a refused one explains why (aimSupport)
			Bar.Add({ FString::Printf(TEXT("%s [%s] %s"), D ? *D->Name : *B.Kind, *B.Key, *B.Text), C, B.Check.bOk, bAimed ? TabOpen : B.Check.bOk ? ButtonFill : CardOff,
				B.Check.bOk ? B.Tip : FString::Printf(TEXT("%s: %s%s"), D ? *D->Name : *B.Kind, *B.Check.Reason, B.Check.bNative ? TEXT(" (this client's own check)") : TEXT("")),
				B.Key.IsEmpty() ? B.Text : FString::Printf(TEXT("[%s] %s"), *B.Key, *B.Text) });
		}
	if (!S.bSpectator)
	{
		// round 16: hud.js drawOrders fills #abil only for a selection (its key is '' with nothing selected, and the panel
		// hides when empty), so Stop and Attack-move show with a selection, as the rest of the Orders; the keys still work
		if (PC->Selected.Num())
		{
			Orders.Add({ TEXT("Stop (X)"), FTCUiCommand(ETCUiAction::Stop), true, ButtonFill, TEXT("Stop (X): halt where they are") });
			Orders.Add({ TEXT("Attack-move (G)"), FTCUiCommand(ETCUiAction::AttackMove), true, PC->Aim == ETCAim::AttackMove ? TabOpen : ButtonFill, TEXT("Attack-move (G, or Ctrl+right-click): move and fight anything met on the way") });
		}
		// client/hud.js drawOrders: Shell area while a salvo unit is selected (the badge's arrow is not in the HUD's font)
		const FTCAreaButton Area = M->Powers.AreaButton(S, M->UnitTable, PC->Selected);
		if (Area.bShown)
			Orders.Add({ FString::Printf(TEXT("Shell area (%s)"), FTCSupportPowers::AreaLabel()), FTCUiCommand(ETCUiAction::Area), true,
				PC->Aim == ETCAim::Support && PC->PowerAim.Kind == ETCSupportAimKind::Area ? TabOpen : ButtonFill, Area.Tip });
		if (TCArmyCommands::ShowsOrdersPanel(S, M->UnitTable, PC->Selected))
		{
			Orders.Add({ TEXT("Retreat (R)"), FTCUiCommand(ETCUiAction::Retreat), true, ButtonFill, TEXT("Retreat (R): run back to base, heal and reinforce there") });
			for (ETCStance St : TCStance::All)
			{
				const FTCStanceButton B = TCArmyCommands::StanceButton(S, M->UnitTable, PC->Selected, St);
				FTCUiCommand C(ETCUiAction::Stance);
				C.Unit = TCStance::Key(St);
				Orders.Add({ FString::Printf(TEXT("%s: %s"), TCStance::Name(St), *B.Text), C, B.bOk, B.bLit ? TabOpen : ButtonFill,
					B.bOk ? FString::Printf(TEXT("%s (%s): %s. Click to switch it on or off for the selection"), TCStance::Name(St), TCStance::Label(St), TCStance::Tip(St)) : B.Reason });
			}
			// client/hud.js drawOrders: the selection's abilities in PRIORITY order (a click uses, a right click switches
			// autocast), then the carrier buttons
			for (const FTCAbilityButton& B : M->Abilities.Buttons(S, M->UnitTable, PC->Selected))
			{
				const FTCAbilityDef* D = FTCAbilities::Find(B.Type);
				FTCUiCommand C(ETCUiAction::Ability);
				C.Unit = B.Type;
				const FString L = FString::Printf(TEXT("%s%s%s%s"), D ? *D->Name : *B.AbilityId, B.Key.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *B.Key),
					B.Text.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" %s"), *B.Text), B.bAuto ? TEXT(" [A]") : TEXT("")); // index.html CSS #abil .ob.auto::after: 'A'
				const bool bAimed = PC->Aim == ETCAim::Ability && PC->AimAbilityType == B.Type;
				Orders.Add({ L, C, B.Check.bOk, bAimed || B.bAuto ? TabOpen : ButtonFill, B.Check.bOk ? B.Tip : FString::Printf(TEXT("%s: %s"), D ? *D->Name : *B.AbilityId, *B.Check.Reason) });
			}
			for (int32 I = 0; I < 2; I++)
			{
				const FTCCarrierButton B = I == 0 ? M->Support.MountUpButton(S, M->UnitTable, PC->Selected) : M->Support.UnloadButton(S, M->UnitTable, PC->Selected);
				if (!B.bShown) continue;
				// the browser's button shows no name, only its title's ('Montar (Shift+Q): ...' in Spanish): its head here
				Orders.Add({ FString::Printf(TEXT("%s (%s) %s"), *FTCLocale::TitleName(B.Tip), *B.Label, *B.Text), FTCUiCommand(I == 0 ? ETCUiAction::MountUp : ETCUiAction::Unload), true, ButtonFill,
					B.bOk ? B.Tip : FString::Printf(TEXT("%s%s"), *B.Reason, B.bNative ? TEXT(" (this client's own check)") : TEXT("")) });
			}
			// client/hud.js menuBtn('form') and menuHTML('form'): the shapes, Tighten, Spread, March together, Snap
			{
				const FTCFormationButton B = PC->Formation.MenuButton();
				FTCUiCommand C(ETCUiAction::Formation);
				C.Unit = B.Key;
				Orders.Add({ FString::Printf(TEXT("%s%s"), *B.Value, B.bOn ? TEXT(" (open)") : TEXT("")), C, true, B.bOn ? TabOpen : ButtonFill, B.Tip });
				for (const FTCFormationButton& F : PC->Formation.SubmenuButtons())
				{
					FTCUiCommand FC(ETCUiAction::Formation);
					FC.Unit = F.Key;
					Menu.Add({ FString::Printf(TEXT("%s%s%s"), *F.Name, F.Kbd.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *F.Kbd), F.Value.IsEmpty() ? TEXT("") : *FString::Printf(TEXT("  %s"), *F.Value)),
						FC, true, F.bOn ? TabOpen : CardFill, F.Tip });
				}
			}
			// the Build menu (base buildings and fortifications, by tab) and the Trench menu (the patterns)
			const FTCBuildMenu BuildMenu = M->Build.BuildMenu(S, PC->Selected);
			const bool bField = M->Field.OffersFieldMenus(PC->Selected);
			if (BuildMenu.bOffered || bField)
			{
				Orders.Add({ TEXT("Build"), FTCUiCommand(ETCUiAction::BuildMenu), true, PC->bBuildMenuOpen ? TabOpen : ButtonFill, TEXT("Build: base buildings and field works the selected builder squads construct") });
				if (bField) Orders.Add({ TEXT("Trench"), FTCUiCommand(ETCUiAction::TrenchMenu), true, PC->bTrenchMenuOpen ? TabOpen : ButtonFill, TEXT("Trench: entrenchment patterns all selected builders dig together") });
				if (PC->bBuildMenuOpen)
				{
					for (const FTCBuildButton& B : BuildMenu.Buttons)
					{
						FTCUiCommand C(ETCUiAction::Build);
						C.Unit = B.Kind;
						Menu.Add({ FString::Printf(TEXT("%s  %s"), *B.Name, *B.Cost), C, B.Check.bOk, B.Check.bOk ? CardFill : CardOff, B.Check.bOk ? B.Tip : B.Check.Reason });
					}
					if (bField)
						for (const FTCFieldButton& B : M->Field.FortMenu(S, PC->Selected).Buttons)
						{
							FTCUiCommand C(ETCUiAction::Fort);
							C.Unit = B.Kind;
							// the key as client/keys.js labels it ("T", "Shift+Y"): the badge's arrow is not in the HUD's font
							const FTCFortDef* FD = FTCFieldworks::Find(B.Kind);
							const FString KeyL = FD ? FD->KeyLabel : FString();
							Menu.Add({ FString::Printf(TEXT("%s%s  %s"), *B.Name, KeyL.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" [%s]"), *KeyL), *B.Value), C, B.Check.bOk, B.Check.bOk ? CardFill : CardOff,
								B.Check.bOk ? B.Tip : B.Check.Reason });
						}
				}
				if (PC->bTrenchMenuOpen && bField)
					for (const FTCFieldButton& B : M->Field.TrenchMenu(S, PC->Selected).Buttons)
					{
						FTCUiCommand C(ETCUiAction::Entrench);
						C.Unit = B.Kind;
						Menu.Add({ FString::Printf(TEXT("%s  %s"), *B.Name, *B.Value), C, B.Check.bOk, B.Check.bOk ? CardFill : CardOff, B.Check.bOk ? B.Tip : B.Check.Reason });
					}
			}
		}
		if (FTCLogistics::ResumeShown(S, PC->Selected))
			Orders.Add({ FTCLogisticsCard::ResumeLabel, FTCUiCommand(ETCUiAction::Resume), true, ButtonFill, TEXT("Resume deliveries: the selected trucks go back to automatic supply runs") });
		if (M->Logistics.ControlsVisible(S))
		{
			// hud.js toolbar aria-labels 'Logistics overlay' and 'Logistics selection' (On/Off as the other switches)
			LogisticsButtons.Add({ FString::Printf(TEXT("Logistics overlay: %s"), M->Logistics.OverlaySwitch() ? TEXT("On") : TEXT("Off")), FTCUiCommand(ETCUiAction::SupplyOverlay), true, M->Logistics.OverlaySwitch() ? TabOpen : ButtonFill,
				TEXT("Supply overlay: known stores and the selected trucks' routes") });
			LogisticsButtons.Add({ FString::Printf(TEXT("Logistics selection: %s"), M->Logistics.SelectionSwitch() ? TEXT("On") : TEXT("Off")), FTCUiCommand(ETCUiAction::TruckSelect), true, M->Logistics.SelectionSwitch() ? TabOpen : ButtonFill,
				TEXT("Truck select: a drag takes only your supply trucks") });
		}
		// client/hud.js drawOrders: Take cover with infantry selected (Shift queues it), and in World Conquest Send by rail
		bool bInfantry = false;
		for (int32 Id : PC->Selected)
			if (const FTCUnit* U = S.Units.Find(Id))
				if (const FTCUnitDef* D = M->UnitTable.Find(U->Type)) bInfantry |= D->bInfantry && S.IsOwn(*U);
		if (bInfantry) Orders.Add({ FString::Printf(TEXT("Take cover (%s)"), *TCKeys::Label(TEXT("cover"))), FTCUiCommand(ETCUiAction::Cover), true, ButtonFill, TEXT("Take cover: the selected squads go to the best cover nearby (Shift queues it)") });
		if (S.Map.bWorld && PC->Selected.Num())
			Orders.Add({ FString::Printf(TEXT("Send by rail (%s)"), *TCKeys::Label(TEXT("rail"))), FTCUiCommand(ETCUiAction::Rail), true, PC->Aim == ETCAim::Rail ? TabOpen : ButtonFill, TEXT("Send by rail: click the destination; they ride to your station nearest it") });
	}
	// The game menu's settings (client/index.html #menu, cover-preview.js, light.js, camera.js; Volume, the page's
	// language picker and the UI scale slider). Round 16: they live in the match menu, as in the browser
	// (MenuSettingRows); only a match with no menu to open (no lobby in play) keeps them here, as before.
	if (!bSettingsInMenu)
		for (const FTCControlRow& R : MenuSettingRows())
			for (const FTCControlItem& It : R.Items)
				Orders.Add({ It.Label, ControlCommand(It.Key, It.Slot), true, It.bOn ? TabOpen : ButtonFill, It.Hover });
	// round 15: the host's Pause (Resume) match, Save match and End match moved to the match menu (client/main.js
	// renderMatchMenu; Client/TCMatchControls.h), with Restart, Hand <name> to AI and Leave
	const FTCMeasure Measure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
	auto Convert=[&](const FItem& I)
	{
		FTCHudFit::FControl C;
		C.Label=I.Label; C.Hover=I.Hover; C.Command=I.Command; C.bEnabled=I.bEnabled; C.bOn=I.Fill.Equals(TabOpen);
		const FString& U=I.Command.Unit;
		switch (I.Command.Action)
		{
		case ETCUiAction::Stop: C.Icon=TEXT("stop"); C.Key=TEXT("X"); break;
		case ETCUiAction::AttackMove: C.Icon=TEXT("amove"); C.Key=TEXT("G"); break;
		case ETCUiAction::Retreat: C.Icon=TEXT("retreat"); C.Key=TEXT("R"); break;
		case ETCUiAction::Stance:
			C.Icon=U; C.Key=U==TEXT("holdFire")?TEXT("⇧F"):U==TEXT("holdPos")?TEXT("⇧G"):TEXT("⇧X");
			C.Value=I.Label.Mid(I.Label.Find(TEXT(": "))+2); break;
		case ETCUiAction::Cover: C.Icon=TEXT("takecover"); C.Key=TEXT("⇧C"); break;
		case ETCUiAction::Rail: C.Icon=TEXT("train"); C.Key=TEXT("⇧N"); break;
		case ETCUiAction::MountUp: C.Icon=TEXT("mountup"); C.Key=TEXT("⇧Q"); break;
		case ETCUiAction::Unload: C.Icon=TEXT("unload"); C.Key=TEXT("⇧E"); break;
		case ETCUiAction::Area: C.Icon=TEXT("barrage"); C.Key=TEXT("⇧B"); break;
		case ETCUiAction::Ability:
		{
			const FTCAbilityDef* D=FTCAbilities::Find(U); C.Icon=D?(D->Id==TEXT("smoke")?TEXT("smokeab"):D->Id):TEXT("unknown");
			for (const FTCAbilityButton& B : M->Abilities.Buttons(S,M->UnitTable,PC->Selected))
				if (B.Type==U) { C.Key=B.Key; C.Value=B.Text; if (B.bAuto) C.Key+=TEXT(" A"); break; }
			break;
		}
		case ETCUiAction::Formation:
			if (U==TEXT("menu")) { C.Icon=TEXT("f_wedge"); C.Value=TEXT("Form"); C.bSubmenu=true; }
			else if (U.StartsWith(TEXT("shape:"))) C.Icon=TEXT("f_")+U.RightChop(6);
			else if (U==TEXT("tighten")) { C.Icon=TEXT("f_tight"); C.Key=TEXT("["); }
			else if (U==TEXT("spread")) { C.Icon=TEXT("f_spread"); C.Key=TEXT("]"); C.Value=FString::Printf(TEXT("%gx"),PC->Formation.Settings.Spread); }
			else { C.Icon=U==TEXT("together")?TEXT("f_together"):TEXT("f_snap"); C.Value=U==TEXT("together")?(PC->Formation.Settings.bTogether?TEXT("On"):TEXT("Off")):(PC->Formation.Settings.bSnap?TEXT("On"):TEXT("Off")); }
			break;
		case ETCUiAction::BuildMenu: C.Icon=TEXT("sandbags"); C.Value=TEXT("Build"); C.bSubmenu=true; break;
		case ETCUiAction::TrenchMenu: C.Icon=TEXT("e_zigzag"); C.Value=TEXT("Trench"); C.bSubmenu=true; break;
		case ETCUiAction::Build: case ETCUiAction::Fort: case ETCUiAction::Entrench:
		{
			C.Icon=I.Command.Action==ETCUiAction::Entrench?TEXT("e_")+U:U;
			const int32 Split=I.Label.Find(TEXT("  ")); if (Split>=0) C.Value=I.Label.Mid(Split+2);
			break;
		}
		case ETCUiAction::Resume: C.Icon=TEXT("truck"); C.Value=TEXT("Resume deliveries"); break;
		case ETCUiAction::SupplyOverlay: C.Icon=TEXT("supplycache"); C.Value=M->Logistics.OverlaySwitch()?TEXT("On"):TEXT("Off"); break;
		case ETCUiAction::TruckSelect: C.Icon=TEXT("truck"); C.Value=M->Logistics.SelectionSwitch()?TEXT("On"):TEXT("Off"); break;
		case ETCUiAction::Support:
			C.Icon=U;
			for (const FTCSupportButton& B : M->Powers.Buttons(S,M->UnitTable,M->Recruit)) if (B.Kind==U) { C.Key=B.Key; C.Value=B.Text; break; }
			break;
		default: C.Value=I.Label; break;
		}
		return C;
	};
	TArray<FTCHudFit::FControl> Grid, Submenu, Support, MenuTabs;
	for (const FItem& I : Orders) Grid.Add(Convert(I));
	// hud.js order: retreat, attack-move, stop, stances, cover, menus, abilities. Stable action ranks keep ability order.
	auto Rank=[](ETCUiAction A) { switch(A) { case ETCUiAction::Retreat:return 0; case ETCUiAction::AttackMove:return 1; case ETCUiAction::Stop:return 2; case ETCUiAction::Area:return 3; case ETCUiAction::Stance:return 4; case ETCUiAction::Cover:return 5; case ETCUiAction::Rail:return 6; case ETCUiAction::MountUp:return 7; case ETCUiAction::Unload:return 8; case ETCUiAction::Formation:return 9; case ETCUiAction::BuildMenu:return 10; case ETCUiAction::TrenchMenu:return 11; case ETCUiAction::Ability:return 12; default:return 13; } };
	Grid.StableSort([&](const FTCHudFit::FControl& A,const FTCHudFit::FControl& B) { return Rank(A.Command.Action)<Rank(B.Command.Action); });
	if (PC->bBuildMenuOpen)
 {
  static const TCHAR* Names[]={TEXT("Base"),TEXT("Defenses"),TEXT("Field works"),TEXT("Engineering")};
  auto Tab=[](const FItem& I)
  {
   const FString& K=I.Command.Unit;
   if (I.Command.Action==ETCUiAction::Build) return K==TEXT("flakpos")||K==TEXT("pillbox")||K==TEXT("tower")?1:0;
   if (K==TEXT("nest")||K==TEXT("wall")||K==TEXT("gate")) return 1;
   return K==TEXT("bridge")||K==TEXT("fill")||K==TEXT("demine")||K==TEXT("aid")?3:2;
  };
  TSet<int32> Available; for (const FItem& I : Menu) Available.Add(Tab(I));
  if (!Available.Contains(BuildTab)) { BuildTab=0; while (BuildTab<3 && !Available.Contains(BuildTab)) BuildTab++; }
  for (int32 K=0; K<4; K++) if (Available.Contains(K))
  { FTCHudFit::FControl C; C.Label=Names[K]; C.Hover=Names[K]; C.bOn=BuildTab==K; C.Command=TCHudBottom::Local(FString::Printf(TEXT("buildtab:%d"),K)); MenuTabs.Add(C); }
  for (const FItem& I : Menu) if (Tab(I)==BuildTab) Submenu.Add(Convert(I));
 }
 else for (const FItem& I : Menu) Submenu.Add(Convert(I));
	for (const FItem& I : Bar) Support.Add(Convert(I));
	for (const FItem& I : LogisticsButtons) Support.Add(Convert(I));
	TArray<FTCKeyUnit> KeyRows;
	for (const auto& Pair : S.Units)
	{
	 const FTCUnit& U=Pair.Value; const FTCUnitDef* D=M->UnitTable.Find(U.Type);
	 FTCKeyUnit K; K.Id=U.Id; K.Type=U.Type; K.bOwn=S.IsOwn(U); K.Hp=U.Hp; K.Flags=U.Flags; K.bDef=D!=nullptr;
	 if (D) { K.bStructure=D->bStructure; K.bBuilding=D->bBuilding; K.bRail=D->bRail; K.bAir=D->bAir; }
	 int32 Kind=0; if (PC->PlanKindOf && PC->PlanKindOf(U.Id,Kind)) K.PlanKind=Kind;
	 KeyRows.Add(K);
	}
	if (bRecruitLetters)
	{
	 const FTCCommandCard Card=M->Recruit.Card(S,PC->Selected);
	 const int32 Count=PC->RecruitGroupOpen<0?Card.Groups.Num():Card.Groups.IsValidIndex(PC->RecruitGroupOpen)?Card.Groups[PC->RecruitGroupOpen].Cards.Num():0;
	 const FString Used=FString(TEXT("QWERTASDFGZXCVB")).Left(Count);
	 for (FTCHudFit::FControl& C : Grid) if (C.Key.Len()==1 && Used.Contains(C.Key)) C.Key.Reset();
	 for (FTCHudFit::FControl& C : Support) if (C.Key.Len()==1 && Used.Contains(C.Key)) C.Key.Reset();
	}
	const int32 Idle=TCSelectionKeys::Idle(KeyRows,false).Num();
	const FString MenuNow=PC->bBuildMenuOpen?FString::Printf(TEXT("build:%d"),BuildTab):PC->bTrenchMenuOpen?TEXT("trench"):PC->Formation.bMenuOpen?TEXT("form"):TEXT("orders");
	if (MenuContext!=MenuNow) { MenuContext=MenuNow; MenuFirst=0; }
	const FString Title=PC->Selected.IsEmpty()?TEXT("Selection"):PC->Selected.Num()==1?TEXT("1 unit selected"):FString::Printf(TEXT("%d units selected"),PC->Selected.Num());
	const FTCHudFit::FBottomLayout Layout=FTCHudFit::LeftPanel(Canvas->ClipY,Title,Selection,Details,Grid,Submenu,Measure,UiNow,Idle,GroupDestination,PC->Selected.Num()>0,bGroupDropdown,SelectionFirst,TopLimit,MenuFirst,MenuTabs);
	float Top=Canvas->ClipY-Gap;
	for (const FBox2D& B : Layout.Paint.Panels) Top=FMath::Min(Top,(float)B.Min.Y);
	if (!bDraw) return Top;
	SelectionMax=Layout.SelectionMax; SelectionFirst=FMath::Clamp(SelectionFirst,0,SelectionMax); SelectionScrollBox=Layout.SelectionList.bIsValid?ToScreen(Layout.SelectionList):FBox2D(ForceInit);
	MatchPanelBox=FBox2D(ForceInit); for (const FBox2D& B : Layout.Paint.Panels) MatchPanelBox+=B;
	MenuMax=Layout.MenuMax; MenuFirst=FMath::Clamp(MenuFirst,0,MenuMax); MenuScrollBox=Layout.MenuList.bIsValid?ToScreen(Layout.MenuList):FBox2D(ForceInit);
 DrawBottomLayout(Layout,Font);
	if (!S.bSpectator && S.SeatConfirmed())
	{
		double Pop=0,Cap=0; const bool Known=M->Recruit.ArmyUse(S,Pop,Cap);
		const FString Money=FString::Printf(TEXT("%d MP"),S.Mp);
		const FString Population=Known?FString::Printf(TEXT("%s/%s units"),*Js(Pop),*Js(Cap)):TEXT("");
		TArray<FTCHudFit::FControl> Resources, Air;
		if (S.Mun.IsSet())
		{
		 FTCHudFit::FControl Mun; Mun.Label=TEXT("Mun"); Mun.Icon=TEXT("resource-mun"); Mun.Value=FString::FromInt(S.Mun.GetValue()); Mun.Hover=TEXT("Munitions: support calls and abilities. Hold points to earn them"); Resources.Add(Mun);
		 FTCHudFit::FControl Fuel; Fuel.Label=TEXT("Fuel"); Fuel.Icon=TEXT("resource-fuel"); Fuel.Value=FString::FromInt(S.Fuel.Get(0)); Fuel.Hover=TEXT("Fuel: vehicles. Depots on the contested nodes and the HQ earn it"); Resources.Add(Fuel);
		}
		for (const FTCAirRow& Row : S.Air)
		 if (const FTCUnit* U=S.Units.Find(Row.Id))
		 {
		  FTCHudFit::FControl C; const FTCUnitDef* D=M->UnitTable.Find(U->Type);
		  C.Label=FTCAlerts::UnitName(S,&M->UnitTable,U->Type,S.You); C.Value=FTCObjectives::AirStateText(Row); C.bOn=PC->Selected.Contains(Row.Id);
		  C.Hover=C.Label+TEXT(": ")+C.Value; C.Command=TCHudBottom::Local(FString::Printf(TEXT("plane:%d"),Row.Id)); if (S.IsOwn(*U)) Air.Add(C);
		 }
		const float MaxH=Minimap.Rect().bIsValid?ToUi(Minimap.Rect().Min.Y)-16:Canvas->ClipY-208;
        const FTCHudFit::FBottomLayout Econ=FTCHudFit::EconomyPanel(Canvas->ClipX,Money,FTCObjectives::EconomyLine(S),Population,Support,Measure,UiNow,Resources,Air,MaxH,AirFirst);
        EconomyBox=Econ.Economy;
        AirMax=Econ.AirMax; AirFirst=FMath::Clamp(AirFirst,0,AirMax); AirScrollBox=Econ.AirList.bIsValid?ToScreen(Econ.AirList):FBox2D(ForceInit);
        DrawBottomLayout(Econ,Font);
	}
	return Top;
}

void ATCHUD::DrawRecruitPanel(UFont* Font, float Left, float Bottom)
{
	// client/hud.js the Command Card: outside Classic every unit this side can recruit, in group tabs (one open at a
	// time); in Classic and World a selected own Production Building's units and its five queue places. Each card shows
	// the browser's availability: a refused card is greyed and a click on it shows the reason instead of buying.
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	const FTCMatchState& S = M->State;
	if (S.bSpectator || !S.SeatConfirmed()) return;
	const FTCRecruitment& R = M->Recruit;
	const FTCCommandCard Card = R.Card(S, PC->Selected);
	// client/hud.js drawTech: one own finished HQ or Armory selected (FTCProgression), and drawClassicCard's World
	// Restore card when the snapshot offers recovery and nothing else takes the card (FTCWorldRecovery)
	const FTCResearchPanel Research = M->Progress.Panel(S, PC->Selected);
	const FTCWorldRecoveryCard Restore = M->WorldRec.Card(S, PC->Selected);
	TArray<int32> SelectedIds=PC->Selected.Array(); SelectedIds.Sort();
	const FTCLogisticsCard Logistics=M->Logistics.Card(S,SelectedIds);
	if (Card.Kind == ETCCardKind::Hidden && !Research.bShown && !Restore.bShown && !Logistics.bVisible) return;
	// client/hud.js drawClassicCard: an own Engineer selected (no single own building) gets the Build card; an all-truck
	// selection gets the logistics card in #buy, after ordinary recruitment when the selection is mixed.
	const FTCBuildCard BuildCard = Card.Kind == ETCCardKind::NotPorted ? M->Build.BuildCard(S, PC->Selected) : FTCBuildCard();
	// round 15: as #buy (client/index.html), between the left column and the minimap at every UI scale, never over the
	// left column: as wide as that room allows (720 px at most), however narrow; cards wider than it are cut
	const float Right = Left + FMath::Clamp((Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Min.X) - 12.f : Canvas->ClipX - Gap) - Left, 60.f, 720.f);
	const float Width = Right - Left;

	// client/hud.js drawRecruit: selecting one finished own Production Building opens the group it makes most of
	const int32 SelBuilding = Card.Kind == ETCCardKind::Recruit && Card.SuggestedGroup >= 0 && PC->Selected.Num() == 1 ? *PC->Selected.CreateConstIterator() : 0;
	if (SelBuilding && SelBuilding != PC->RecruitAutoFor) PC->RecruitGroupOpen = Card.SuggestedGroup;
	PC->RecruitAutoFor = SelBuilding;

	// what goes in the panel, row by row: text lines, then rows of buttons that wrap at the panel's width
	struct FItem { FString Label; FTCUiCommand Command; bool bEnabled; FLinearColor Fill; float W; FString Hover; };
	TArray<TPair<FString, FLinearColor>> Lines;
	TArray<TArray<FItem>> Rows;
	auto Measure = [&](const FString& L) { float TW = 0, TH = 0; LabelSize(L, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	TArray<TArray<FItem>> Lists; // round 17: each wrapped list as given, for the browser's fit below
	auto Wrap = [&](const TArray<FItem>& Items)
	{
		Lists.Add(Items);
		TArray<FItem> Row; float X = 0;
		for (FItem I : Items)
		{
			I.W = FMath::Min(I.W, Width - 2 * Pad); // round 15: a card is never wider than the panel
			if (Row.Num() && X + I.W > Width - 2 * Pad) { Rows.Add(Row); Row.Reset(); X = 0; }
			Row.Add(I); X += I.W + Pad;
		}
		if (Row.Num()) Rows.Add(Row);
	};
	double Pop = 0, Cap = 0;
	const bool bArmy = R.ArmyUse(S, Pop, Cap);
	// hud.js drawEcon: the money, the fuel (Classic: index.html #fuelRes, '{n}' then 'Fuel') and the army count as
	// #popCount writes it, '{pop}/{cap} units' (its title: 'Army size: ...'), each in its own column
	const FString Wallet = FString::Printf(TEXT("%.0f MP%s  |  %s"), R.Mp(), R.Fuel().IsSet() ? *FString::Printf(TEXT("  |  %.0f Fuel"), R.Fuel().GetValue()) : TEXT(""),
		bArmy ? *FString::Printf(TEXT("%s/%s units"), *Js(Pop), *Js(Cap)) : TEXT("?"));
	auto CardItems = [&](const FTCRecruitGroup& G)
	{
		TArray<FItem> Items;
		for (const FTCRecruitCard& C : G.Cards)
		{
			FTCUiCommand Cmd(ETCUiAction::Recruit);
			Cmd.Unit = C.Type;
			Cmd.Building = C.Action.bHasFrom ? C.Action.From : 0;
			const FString Name = FTCAlerts::UnitName(S, &M->UnitTable, C.Type, S.You); // hud.js unitCard: name(t), the player's faction
			const FString L = FString::Printf(TEXT("%s  %s%s"), *Name, *C.Cost, C.Sub.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" (%s)"), *C.Sub));
			const FString Hover = FString::Printf(TEXT("%s: %s%s. %s"), *Name, *C.Cost, C.Sub.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(", %s"), *C.Sub),
				C.Check.bOk ? TEXT("Click to recruit, Shift+click buys up to 5") : *C.Check.Reason);
			Items.Add({ L, Cmd, C.Check.bOk, C.Check.bOk ? CardFill : C.bBroke ? CardBroke : CardOff, Measure(L), Hover });
		}
		return Items;
	};
	switch (Card.Kind)
	{
	case ETCCardKind::Recruit:
	{
		// Recruit and Rally sit above the tab row. Economy is drawn in #econ, not on this card.
		TArray<FItem> Tabs;
		for (int32 I = 0; I < Card.Groups.Num(); I++)
		{
			FTCUiCommand Cmd(ETCUiAction::RecruitGroup);
			Cmd.Group = I;
			const FString L = Card.Groups[I].Name;
			Tabs.Add({ L, Cmd, true, I == PC->RecruitGroupOpen ? TabOpen : ButtonFill, Measure(L), FString::Printf(TEXT("%s: click to %s its cards"), *L, I == PC->RecruitGroupOpen ? TEXT("hide") : TEXT("show")) });
		}
		// client/hud.js drawRecruit: the card's Rally button (startRally)
		if (TCArmyCommands::ShowsRallyButton(S, R, PC->Selected))
		{
			const FString L = FString::Printf(TEXT("Rally (%s)"), TCArmyCommands::RallyLabel);
			Tabs.Add({ L, FTCUiCommand(ETCUiAction::Rally), true, PC->Aim == ETCAim::Rally ? TabOpen : ButtonFill, Measure(L), FString::Printf(TEXT("Rally (%s): choose where new recruits gather"), TCArmyCommands::RallyLabel) }); // hud.js drawRecruit b.title
		}
		Wrap(Tabs);
		if (Card.Groups.IsValidIndex(PC->RecruitGroupOpen)) Wrap(CardItems(Card.Groups[PC->RecruitGroupOpen]));
		// client/hud.js drawRecruit: one selected building's facility line, with "Cancel (75% back)" on an own site
		const FTCSiteCard Site = M->Support.SiteCard(S, M->UnitTable, PC->Selected, R.Classic());
		if (Site.bShown && !Site.bClassicCard)
		{
			if (!Site.Status.IsEmpty()) Lines.Add({ Site.Status, Dim });
			if (Site.bCancel) Wrap({ { Site.CancelText, FTCUiCommand(ETCUiAction::CancelSite), true, ButtonFill, Measure(Site.CancelText), Site.CancelText } });
		}
		break;
	}
	case ETCCardKind::Building:
	{
		const FTCUnit* B = S.Units.Find(Card.Building);
		const FTCRecruitDef* D = B ? FTCRecruitment::Find(B->Type) : nullptr;
		Lines.Add({ FString::Printf(TEXT("%s  |  %s"), D ? *D->Name : TEXT("Building"), *Wallet), Text });
		Lines.Add({ Card.QueueLine, Card.bTraining ? Good : Dim });
		for (const FTCRecruitGroup& G : Card.Groups) Wrap(CardItems(G));
		TArray<FItem> Slots;
		for (const FTCQueueSlot& Q : Card.Slots)
		{
			const FString Name = Q.Type.IsEmpty() ? FString() : FTCAlerts::UnitName(S, &M->UnitTable, Q.Type, S.You); // hud.js name(t)
			FTCUiCommand Cmd(Q.Job ? ETCUiAction::CancelJob : ETCUiAction::None);
			Cmd.Building = Card.Building; Cmd.Job = Q.Job;
			const FString L = Q.Type.IsEmpty() ? FString(TEXT("(free)")) : Q.bTraining ? FString::Printf(TEXT("%s %.0f%%"), *Name, Card.Progress * 100.0) : Q.Job ? FString::Printf(TEXT("x %s"), *Name) : Name;
			const FString Hover = Q.Job ? (Q.Check.bOk ? FString::Printf(TEXT("Cancel %s: refund %d MP, %d Fuel"), *Name, Q.RefundMp, Q.RefundFuel) : Q.Check.Reason)
				: Q.Type.IsEmpty() ? FString(TEXT("A free queue place")) : FString::Printf(TEXT("%s: %s"), Q.bTraining ? TEXT("Training") : TEXT("Waiting"), *Name);
			Slots.Add({ L, Cmd, !Q.Job || Q.Check.bOk, Q.bTraining ? TabOpen : Q.Job ? ButtonFill : CardOff, FMath::Max(Measure(L), 80.f), Hover });
		}
		Wrap(Slots);
		break;
	}
	case ETCCardKind::Site:
	{
		const FTCUnit* B = S.Units.Find(Card.Building);
		const FTCRecruitDef* D = B ? FTCRecruitment::Find(B->Type) : nullptr;
		// client/hud.js drawClassicCard for a site: its progress from the exact built the server sent, the hint and Cancel
		const FTCSiteCard Site = M->Support.SiteCard(S, M->UnitTable, PC->Selected, R.Classic());
		if (Site.bShown && !Site.Status.IsEmpty()) Lines.Add({ FString::Printf(TEXT("%s: %s"), D ? *D->Name : TEXT("Building"), *Site.Status), Text });
		else Lines.Add({ FString::Printf(TEXT("%s: under construction, %.0f%% built"), D ? *D->Name : TEXT("Building"), Card.Built * 100.f), Text });
		if (Site.bShown && !Site.Hint.IsEmpty()) Lines.Add({ Site.Hint, Dim });
		if (Site.bShown && Site.bCancel)
		{
			const FString L = Site.CancelSub.IsEmpty() ? Site.CancelText : FString::Printf(TEXT("%s (%s)"), *Site.CancelText, *Site.CancelSub);
			Wrap({ { L, FTCUiCommand(ETCUiAction::CancelSite), true, ButtonFill, Measure(L), Site.CancelTip.IsEmpty() ? L : Site.CancelTip } });
		}
		break;
	}
	default:
		if (BuildCard.bShown)
		{
			// client/hud.js drawClassicCard: the Engineers' Build card. A click arms the building (startBuild explains a
			// refusal); a key badge is its Classic key
			Lines.Add({ TEXT("Build  |  ") + Wallet, Text });
			TArray<FItem> Items;
			for (const FTCBuildButton& B : BuildCard.Buttons)
			{
				FTCUiCommand Cmd(ETCUiAction::Build);
				Cmd.Unit = B.Kind;
				const FString L = FString::Printf(TEXT("%s%s  %s"), *B.Name, B.Key.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" [%s]"), *B.Key), *B.Cost);
				Items.Add({ L, Cmd, B.Check.bOk, B.Check.bOk ? CardFill : B.bBroke ? CardBroke : CardOff, Measure(L), FString::Printf(TEXT("%s. %s"), *B.Tip, *B.Note) });
			}
			Wrap(Items);
			break;
		}
		if (Restore.bShown)
		{
			// client/hud.js drawClassicCard: 'Restore HQ' or 'Restore Engineer', its MP cost and line, disabled when short
			Lines.Add({ FString::Printf(TEXT("World recovery  |  %s"), *Wallet), Text });
			const FString L = FString::Printf(TEXT("%s  %s (%s)"), *Restore.Name, *Restore.Cost, *Restore.Sub);
			const FString Tip = Restore.bDisabled ? FString::Printf(TEXT("%s: needs %s"), *Restore.Name, *Restore.Cost) : FString::Printf(TEXT("%s: %s, %s"), *Restore.Name, *Restore.Cost, *Restore.Sub);
			Wrap({ { L, FTCUiCommand(ETCUiAction::Recover), !Restore.bDisabled, Restore.bDisabled ? CardBroke : CardFill, Measure(L), Tip } });
			break;
		}
		if (Card.Kind != ETCCardKind::Hidden && !Logistics.bVisible) Lines.Add({ FString::Printf(TEXT("Command card not built in this client yet: %s"), *Card.Why), Dim });
		break;
	}
	if (Research.bShown)
	{
		// client/hud.js drawTech: the side's tier, then the HQ's tier button or the Armory's three lines; a refused button
		// is greyed and a click on it shows the reason, a running job shows its progress
		Lines.Add({ FString::Printf(TEXT("Research  |  %s"), *Research.TierLabel), Text });
		TArray<FItem> Items;
		for (const FTCResearchButton& B : Research.Buttons)
		{
			FTCUiCommand Cmd(ETCUiAction::Research);
			Cmd.Unit = LexToString(B.Kind);
			Items.Add({ B.Text, Cmd, B.Check.bOk, B.Check.bOk ? CardFill : B.bResearching ? TabOpen : CardOff, Measure(B.Text), B.ShownTooltip() });
		}
		Wrap(Items);
	}

	const FTCMeasure TypeMeasure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
	TArray<FTCHudFit::FControl> TabControls, Above;
	TArray<TArray<FTCHudFit::FControl>> CardLists;
	TArray<FString> Headings;
	for (const auto& L : Lines) Headings.Add(L.Key);
	if (Card.Kind==ETCCardKind::Recruit)
	{
		FTCHudFit::FControl RecruitTitle; RecruitTitle.Label=bRecruitLetters?TEXT("Recruiting"):TEXT("Recruit"); RecruitTitle.Key=bRecruitLetters?TEXT("Esc"):TEXT("Tab"); RecruitTitle.Command=TCHudBottom::Local(TEXT("recruit")); RecruitTitle.bOn=bRecruitLetters;
		RecruitTitle.Hover=bRecruitLetters?TEXT("A letter opens a group, a second letter buys from it (Q Q: Rifles); Shift+letter buys five. Esc goes back a step, Tab or a right-click stops"):TEXT("Recruit by letter (Tab): a letter opens a group, a second buys from it, Shift buys five. WASD pans again when you stop"); Above.Add(RecruitTitle);
	}
	for (const TArray<FItem>& List : Lists)
	{
		TArray<FTCHudFit::FControl> Cards;
		for (const FItem& I : List)
		{
			FTCHudFit::FControl C;
			C.Label=I.Label; C.Hover=I.Hover; C.Command=I.Command; C.bEnabled=I.bEnabled; C.bOn=I.Fill.Equals(TabOpen);
			if (I.Command.Action==ETCUiAction::RecruitGroup)
			{
				static const TCHAR* const Icons[]={TEXT("rifle"),TEXT("mg"),TEXT("medium"),TEXT("fighter"),TEXT("destroyer")};
				const int32 G=Card.Groups.IsValidIndex(I.Command.Group)?Card.Groups[I.Command.Group].Group:0;
				C.Icon=Icons[FMath::Clamp(G,0,4)];
				if (bRecruitLetters && PC->RecruitGroupOpen<0 && I.Command.Group<15) C.Key=FString::Chr(TEXT("QWERTASDFGZXCVB")[I.Command.Group]);
				TabControls.Add(C); continue;
			}
			if (I.Command.Action==ETCUiAction::Rally) { C.Label=TEXT("Rally"); C.Key=TEXT("Shift+H"); Above.Add(C); continue; }
			if (I.Command.Action==ETCUiAction::Recruit)
			{
				C.Label=FTCAlerts::UnitName(S,&M->UnitTable,I.Command.Unit,S.You); C.Icon=I.Command.Unit;
				const FTCPrice Price=R.PriceOf(I.Command.Unit);
				C.Value=FString::Printf(TEXT("%d MP%s"),Price.Mp,Price.Fuel?*FString::Printf(TEXT("  %d Fuel"),Price.Fuel):TEXT(""));
			}
			else
			{
				const int32 Sep=C.Label.Find(TEXT("  "));
				if (Sep>0) { C.Value=C.Label.Mid(Sep+2); C.Label=C.Label.Left(Sep); }
				C.Icon=I.Command.Unit;
			}
			if (bRecruitLetters && PC->RecruitGroupOpen>=0 && Cards.Num()<15) C.Key=FString::Chr(TEXT("QWERTASDFGZXCVB")[Cards.Num()]);
			Cards.Add(C);
		}
		if (Cards.Num()) CardLists.Add(Cards);
	}
	const float PanelRight=Minimap.Rect().bIsValid?ToUi((float)Minimap.Rect().Min.X)-8.f:Canvas->ClipX-Gap;
	TArray<FString> LogisticsLines;
	if (Logistics.bVisible)
	{
		LogisticsLines.Add(TEXT("Logistics"));
		for (const FTCLogisticsEntry& Entry : Logistics.Entries)
		{
			LogisticsLines.Add(TC_T(FTCAlerts::UnitName(S,&M->UnitTable,Entry.Type,S.You))+FString::Printf(TEXT(" #%d"),Entry.Id));
			for (const FString& Detail : Entry.Lines) LogisticsLines.Add(Detail);
		}
		if (Logistics.bTrucks)
		{
			LogisticsLines.Add(FTCLogisticsCard::Hint);
			FTCHudFit::FControl Resume; Resume.Label=FTCLogisticsCard::ResumeLabel; Resume.Command=FTCUiCommand(ETCUiAction::Resume);
			Above.Add(Resume);
		}
	}
	int32 TotalCards=0; for (const FTCRecruitGroup& Group : Card.Groups) TotalCards+=Group.Cards.Num();
	const FString RecruitNow=FString::Printf(TEXT("%d:%d:%d"),int32(Card.Kind),SelBuilding,PC->RecruitGroupOpen);
	if (RecruitContext!=RecruitNow) { RecruitContext=RecruitNow; RecruitFirst=0; }
	const FTCHudFit::FBottomLayout Recruitment=FTCHudFit::RecruitPanel(Canvas->ClipX,Canvas->ClipY,PanelRight,Headings,TabControls,CardLists,Above,TypeMeasure,UiNow,LogisticsLines,TotalCards,RecruitFirst,80,EconomyBox);
    RecruitMax=Recruitment.RecruitMax; RecruitFirst=FMath::Clamp(RecruitFirst,0,RecruitMax); RecruitScrollBox=Recruitment.RecruitList.bIsValid?ToScreen(Recruitment.RecruitList):FBox2D(ForceInit);
    DrawBottomLayout(Recruitment,Font);

}

void ATCHUD::GroundLine(const ATCTerrain& T, const TArray<FVector2D>& Points, bool bClosed, const FLinearColor& Colour, float Thickness, float Lift)
{
	const int32 N = Points.Num();
	if (N < 2) return;
	FVector Prev = FVector::ZeroVector;
	bool bPrev = false;
	auto Visit = [&](FVector2D P)
	{
		const FVector Sc = Project(ATCTerrain::ToWorld(P.X, P.Y, T.HeightAt(P.X, P.Y) + Lift));
		if (bPrev && Sc.Z > 0 && Prev.Z > 0) DrawLine(Prev.X, Prev.Y, Sc.X, Sc.Y, Colour, Thickness);
		Prev = Sc; bPrev = true;
	};
	Visit(Points[0]);
	for (int32 I = 1; I < N + (bClosed ? 1 : 0); I++)
	{
		const FVector2D A = Points[I - 1], B = Points[I % N];
		const int32 Steps = FMath::Clamp(FMath::CeilToInt(FVector2D::Distance(A, B) / 2.0), 1, 2000);
		for (int32 K = 1; K <= Steps; K++) Visit(A + (B - A) * (double(K) / Steps));
	}
}

void ATCHUD::GroundRing(const ATCTerrain& T, FVector2D At, float Radius, const FLinearColor& Colour, float Thickness)
{
	TArray<FVector2D> Pts;
	constexpr int32 Segments = 32;
	for (int32 I = 0; I < Segments; I++) { const float A = 2.f * PI * I / Segments; Pts.Add(At + FVector2D(Radius * FMath::Cos(A), Radius * FMath::Sin(A))); }
	GroundLine(T, Pts, true, Colour, Thickness);
}

void ATCHUD::DrawWorldOverlays(UFont* Font)
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !M->Terrain) return;
	const ATCTerrain& T = *M->Terrain;
	const FTCMatchState& S = M->State;
	auto Label = [&](FVector2D At, const FString& L, const FLinearColor& C)
	{
		const FVector Sc = Project(ATCTerrain::ToWorld(At.X, At.Y, T.HeightAt(At.X, At.Y) + 1.f));
		FLinearColor Lc = C; Lc.A *= M->WarMap.LabelFade(); // round 14: markers.js fadeLabels while the paper war map comes
		if (Sc.Z > 0) DrawLabel(L, Lc, Sc.X + 6.f, Sc.Y - 6.f, Font, 1.f);
	};
	// round 17 (markers): buildHQ's reinforce zone, ring, flag and tag and the resource nodes' brackets and tags under
	// everything else (TCMarkersHud), then World Conquest's regions as world-regions.js draws them: the border, the
	// capture point's ring over its wash, track, arc and contested ring, its flag and its two tags. Only regions whose
	// claim point projects within three viewports of the screen are drawn (a cost limit of this client).
	{
		FTCMarkerPainter Marks = TCMarkersHud::Painter(Canvas, *M);
		TCMarkersHud::DrawHqCamps(Marks, *M, Font);
		TCMarkersHud::DrawNodes(Marks, *M, Font);
		TCMarkersHud::DrawWorldRegions(Marks, *M, Font);
	}
	// client/markers.js capturePoint: the authored capture points (slice 9)
	DrawCapturePoints(Font);
	// client/main.js frame: the armed building's footprint under the pointer, green where the browser's placement
	// check passes and red where it refuses (the server decides again)
	if (PC->Aim == ETCAim::Build && PC->bPointerGround)
	{
		const FTCPlacement P = M->Build.Placement(S, PC->AimBuild, PC->PointerGround);
		if (!P.bThrows) bLastPreviewOk = P.bOk;
		const FLinearColor C(bLastPreviewOk ? FColor(0x7a, 0xcb, 0x82) : FColor(0xdc, 0x4a, 0x3c));
		if (P.bHasFootprint && S.Map.W > 0)
		{
			const float X0 = (P.Cell % S.Map.W) * FTCMatchState::CellMetres, Z0 = (P.Cell / S.Map.W) * FTCMatchState::CellMetres, Side = P.Size * FTCMatchState::CellMetres;
			GroundLine(T, { FVector2D(X0, Z0), FVector2D(X0 + Side, Z0), FVector2D(X0 + Side, Z0 + Side), FVector2D(X0, Z0 + Side) }, true, C, 3.f);
		}
		GroundRing(T, P.At, 0.8f, C, 2.f);
	}
	// client/main.js drawWorks: the ghost of every planned or started work still to dig, faint, in its colour
	auto Cells = [&](const TArray<int32>& List, const FLinearColor& C, float Thick)
	{
		if (S.Map.W <= 0) return;
		const float Cm = FTCMatchState::CellMetres;
		for (int32 Cell : List)
		{
			const float X0 = (Cell % S.Map.W) * Cm, Z0 = (Cell / S.Map.W) * Cm;
			GroundLine(T, { FVector2D(X0, Z0), FVector2D(X0 + Cm, Z0), FVector2D(X0 + Cm, Z0 + Cm), FVector2D(X0, Z0 + Cm) }, true, C, Thick, 0.15f);
		}
	};
	for (const FTCWorkGhost& G : M->Field.WorkGhosts(S)) Cells(G.Place.Cells, FLinearColor(G.Color) * FLinearColor(1, 1, 1, 0.45f), 1.f);
	// client/main.js frame with a dig aim: the oblong marker along its direction, green or red, with the cells it
	// would dig; with an entrench aim: each planned segment's cells in its colour
	if (PC->Aim == ETCAim::Field && PC->FieldPreview.bShown && !PC->FieldPreview.bThrows)
	{
		const FTCFieldPreview& P = PC->FieldPreview;
		if (P.bMarker && P.bMarkerPlaced)
		{
			const FVector2D Along(FMath::Cos(P.MarkerDir), FMath::Sin(P.MarkerDir)), Across(-Along.Y, Along.X);
			const FVector2D A = Along * (P.MarkerLen / 2), B = Across * (P.MarkerWidth / 2);
			const FLinearColor C(P.MarkerColor);
			GroundLine(T, { P.MarkerAt - A - B, P.MarkerAt + A - B, P.MarkerAt + A + B, P.MarkerAt - A + B }, true, C, 3.f);
			GroundLine(T, { P.MarkerAt + A, P.MarkerAt + A * 1.6 }, false, C, 2.f); // the arrow past its far end
			Cells(P.Placement.Cells, C, 1.5f);
		}
		for (const FTCEntrenchPiece& Piece : P.Pieces) Cells(Piece.Place.Cells, FLinearColor(Piece.Color), 1.5f);
		if (PC->FieldAim.bHasCenter) GroundRing(T, PC->FieldAim.Center, 0.6f, FLinearColor(FColor(0xf4, 0xdc, 0x90)), 2.f);
	}
	// client/main.js strikeShape and syncStrikes: the public warnings of support calls on their way and live, in the
	// caller's colour (red for a foe's), a strip along its direction or a ring
	auto Strip = [&](FVector2D At, double Dir, double Len, double Width, const FLinearColor& C, float Thick, bool bArrow)
	{
		const FVector2D Along(FMath::Cos(Dir), FMath::Sin(Dir)), Across(-Along.Y, Along.X);
		const FVector2D A = Along * (Len / 2), B = Across * (Width / 2);
		GroundLine(T, { At - A - B, At + A - B, At + A + B, At - A + B }, true, C, Thick);
		if (bArrow) GroundLine(T, { At + A, At + A + Along * FMath::Min(6.0, Len * 0.25) }, false, C, Thick); // client/main.js aimMarker's arrow
	};
	for (const FTCStrikeMark& K : M->Powers.Strikes(S))
	{
		const FLinearColor C = K.bFoe ? FLinearColor(FColor(0xd2, 0x32, 0x1e)) : ATCUnitRenderer::SeatColor(K.Owner);
		if (K.bStrip) Strip(K.At, K.Dir, K.Len, K.Width, C, 2.f, false);
		else GroundRing(T, K.At, (float)K.Radius, C, 2.f);
		const FTCSupportDef* D = FTCSupportPowers::Find(K.Kind);
		Label(K.At, K.T > 0 ? FString::Printf(TEXT("%s in %.1f s"), D ? *D->Name : *K.Kind, K.T) : (D ? D->Name : K.Kind), C);
	}
	// client/main.js frame with a support kind or 'area' armed: its mesh under the pointer (or on the pinned centre,
	// turned toward the pointer), a strip with its arrow or a ring
	if (PC->Aim == ETCAim::Support && PC->PowerPreview.bShown && PC->PowerPreview.bPlaced)
	{
		const FTCSupportPreview& P = PC->PowerPreview;
		const FLinearColor C(P.Color);
		if (P.bStrip) Strip(P.At, P.Dir, P.Len, P.Width, C, 3.f, true);
		else GroundRing(T, P.At, (float)P.Radius, C, 2.5f);
		if (PC->PowerAim.bHasCenter) GroundRing(T, PC->PowerAim.Center, 0.6f, C, 2.f);
	}
	// client/formation-preview.js show: the slot boxes, rank ribbons, a chevron per slot and the drag's arrow
	if (PC->FacingPreview.bVisible)
	{
		const FTCFormationPreview& F = PC->FacingPreview;
		FLinearColor C(F.Color);
		C.A = F.Opacity;
		for (const TPair<FVector2D, FVector2D>& L : F.Lines) GroundLine(T, { L.Key, L.Value }, false, C, 2.f, 0.25f);
		for (const FTCFormationPreview::FArrow& A : F.Arrows) GroundLine(T, { A.Tip, A.Left, A.Right }, true, C, 2.f, 0.25f);
	}
	// client/cover-preview.js: the shields and chevrons of the cursor and the flash, each layer at its opacity (this
	// canvas has no depth test, so the through-models layer is drawn the same way, at its own opacity); round 17
	// (markers): as overlay.js ribbons with the shields' 0.3 fill (TCMarkersHud::DrawCoverLayer)
	{
		const FTCCoverView& V = M->Cover.View();
		FTCMarkerPainter Marks = TCMarkersHud::Painter(Canvas, *M);
		auto Layer = [&](const TArray<FTCCoverGlyph>& Glyphs, float Opacity, bool bDepth) { TCMarkersHud::DrawCoverLayer(Marks, Glyphs, Opacity, bDepth); };
		if (V.bVisible)
		{
			if (V.bCursorGround) Layer(M->Cover.CursorMarks().Ground, V.CursorGroundOpacity, true);
			if (V.bCursorThrough) Layer(M->Cover.CursorMarks().Through, V.CursorThroughOpacity, false);
			if (V.bFlashGround) Layer(M->Cover.FlashMarks().Ground, V.FlashGroundOpacity, true);
			if (V.bFlashThrough) Layer(M->Cover.FlashMarks().Through, V.FlashThroughOpacity, false);
		}
	}
	// client/main.js aimShape for an aimed ability: a ring of its radius under the pointer
	if (PC->Aim == ETCAim::Ability && PC->bPointerGround && PC->AimAbilityRadius > 0)
		GroundRing(T, PC->PointerGround, (float)PC->AimAbilityRadius, FLinearColor(FColor(0xff, 0xa0, 0x30)), 2.f);
	// rally points: the army's (shared/sim.js snapshotFor rally) and the selected own buildings' (their queue's rally)
	// are planLayer's flags since round 16 (DrawPlans, TCOrders::Markers)
	// client/logistics.js overlayItems and main.js drawLogisticsRoutes: known stores, the selected trucks' known routes
	if (M->Logistics.OverlayActive())
	{
		const FTCLogisticsOverlay O = M->Logistics.Overlay(S, PC->Selected, [&S](double X, double Z) { return FTCLogistics::KnownByFog(S, X, Z); });
		for (const FTCLogisticsStore& St : O.Stores)
		{
			const FVector2D At(St.X, St.Z);
			GroundRing(T, At, 4.f, FLinearColor(FTCLogistics::StoreRingColor(St.bSource)), 2.f);
			const TArray<FString> L = FTCLogistics::StoreLabels(&St);
			if (L.Num()) Label(At, L[0], FLinearColor(FTCLogistics::StoreRingColor(St.bSource)));
		}
		for (const FTCSupplyRoute& R : O.Routes) GroundLine(T, R.Points, false, FLinearColor(FTCLogistics::RouteColor(R.bManual)), 2.f, 0.6f);
	}
	// slice 9: the selection's plan lines (client/markers.js planLayer) and the unit bars (client/main.js frame)
	DrawPlans();
	DrawHealthBars();
}

void ATCHUD::DrawMinimapOverlays()
{
	// client/main.js drawMinimap's logistics layer while the overlay is on: stores, the selected trucks' routes and
	// the own units cut off from supply (red rings)
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !Minimap.Rect().bIsValid) return;
	const FTCMatchState& S = M->State;
	FVector2D Px, Prev;
	if (M->bArmyRally && Minimap.WorldToPixel(M->ArmyRally, Px)) DrawRect(FLinearColor(FColor(0x9d, 0xd0, 0xff)), Px.X - 2.f, Px.Y - 2.f, 4.f, 4.f);
	// client/main.js drawMinimap, World: each region this side was sent as a disc of the claim radius in its team's
	// colour (a brass ring when locked), and the stations as dark squares with the region's fill
	if (M->WorldRec.IsWorld())
	{
		for (const FTCWorldRegionView& V : M->WorldRec.RegionViews())
		{
			FVector2D Edge;
			if (!Minimap.WorldToPixel(V.At, Px) || !Minimap.WorldToPixel(V.At + FVector2D(TCWorldRecoveryCore::PointRadius, 0), Edge)) continue;
			const float Rad = FMath::Max(2.f, (float)FVector2D::Distance(Px, Edge));
			const float Side = Rad * 1.4f;
			DrawRect(FLinearColor(V.TeamColour), Px.X - Side / 2, Px.Y - Side / 2, Side, Side);
			if (V.bLocked)
			{
				const FLinearColor C(FTCWorldRecovery::LockedColour);
				const float O = Side / 2 + 1.5f;
				DrawLine(Px.X - O, Px.Y - O, Px.X + O, Px.Y - O, C); DrawLine(Px.X + O, Px.Y - O, Px.X + O, Px.Y + O, C);
				DrawLine(Px.X + O, Px.Y + O, Px.X - O, Px.Y + O, C); DrawLine(Px.X - O, Px.Y + O, Px.X - O, Px.Y - O, C);
			}
		}
		for (const FTCWorldStationView& V : M->WorldRec.StationViews())
			if (Minimap.WorldToPixel(V.At, Px))
			{
				DrawRect(FLinearColor(FTCWorldRecovery::StationFrameColour), Px.X - 2.5f, Px.Y - 2.5f, 5.f, 5.f);
				DrawRect(FLinearColor(V.Fill), Px.X - 1.5f, Px.Y - 1.5f, 3.f, 3.f);
			}
	}
	DrawMinimapPoints();
	// client/main.js drawMinimap: the strip strikes' outlines ('#ff3020' for a foe's, else the caller's colour)
	for (const FTCStrikeMark& K : M->Powers.Strikes(S))
	{
		if (!K.bMinimap || !K.bStrip) continue;
		const FVector2D Along(FMath::Cos(K.Dir), FMath::Sin(K.Dir)), Across(-Along.Y, Along.X);
		const FVector2D A = Along * (K.Len / 2), B = Across * (K.Width / 2);
		const FVector2D Corners[4] = { K.At - A - B, K.At + A - B, K.At + A + B, K.At - A + B };
		FVector2D Pix[4];
		bool bAll = true;
		for (int32 I = 0; I < 4; I++) bAll &= Minimap.WorldToPixel(Corners[I], Pix[I]);
		if (!bAll) continue;
		const FLinearColor C = K.bFoe ? FLinearColor(FColor(0xff, 0x30, 0x20)) : ATCUnitRenderer::SeatColor(K.Owner);
		for (int32 I = 0; I < 4; I++) DrawLine(Pix[I].X, Pix[I].Y, Pix[(I + 1) % 4].X, Pix[(I + 1) % 4].Y, C, 1.f);
	}
	Minimap.DrawUnits(Canvas,S,M->UnitTable,PC->Selected);
	if (M->Logistics.OverlayActive())
	{
	const FTCLogisticsOverlay O = M->Logistics.Overlay(S, PC->Selected, [&S](double X, double Z) { return FTCLogistics::KnownByFog(S, X, Z); });
	for (const FTCLogisticsStore& St : O.Stores)
		if (Minimap.WorldToPixel(FVector2D(St.X, St.Z), Px)) DrawRect(FLinearColor(FTCLogistics::MinimapStoreColor(St.bSource)), Px.X - 2.5f, Px.Y - 2.5f, 5.f, 5.f);
	for (const FTCSupplyRoute& R : O.Routes)
		for (int32 I = 0; I < R.Points.Num(); I++)
		{
			if (!Minimap.WorldToPixel(R.Points[I], Px)) continue;
			if (I > 0) DrawLine(Prev.X, Prev.Y, Px.X, Px.Y, FLinearColor(FTCLogistics::MinimapRouteColor()), 1.f);
			Prev = Px;
		}
	for (int32 Id : M->Logistics.CutOffUnits(S))
		if (const FTCUnit* U = S.Units.Find(Id))
			if (Minimap.WorldToPixel(FVector2D(U->X, U->Z), Px))
			{
				const FLinearColor C(FTCLogistics::MinimapCutColor());
				DrawLine(Px.X - 3, Px.Y - 3, Px.X + 3, Px.Y - 3, C); DrawLine(Px.X + 3, Px.Y - 3, Px.X + 3, Px.Y + 3, C);
				DrawLine(Px.X + 3, Px.Y + 3, Px.X - 3, Px.Y + 3, C); DrawLine(Px.X - 3, Px.Y + 3, Px.X - 3, Px.Y - 3, C);
			}
	}
	DrawMinimapAlertRings();
	DrawMinimapPings();
}

void ATCHUD::DrawHUD()
{
	Super::DrawHUD();
	Buttons.Reset();
	Panels.Reset();
	PendingTooltip.Reset(); TooltipAnchor=FBox2D(ForceInit); CornerBox=FBox2D(ForceInit); SelectionScrollBox=FBox2D(ForceInit);
 EconomyBox=FBox2D(ForceInit);
 MenuScrollBox=AirScrollBox=RecruitScrollBox=FBox2D(ForceInit);
	EndUi(); // a frame starts unscaled
	Frames++;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !Canvas) return;
	UFont* Font = GEngine->GetSmallFont();
	UFont* Big = GEngine->GetMediumFont() ? GEngine->GetMediumFont() : Font;
	const FTCMatchState& S = M->State;
    if (!M->Controls.TerminalNotice.IsEmpty())
    {
        PC->LastOrderNote.Reset(); PC->LastOrderNoteTime=-1e9; PC->bLastOrderSent=false;
    }
	const bool bStrip = S.bHasMatch && S.Snapshots > 0;
	// round 15: a match's first frame starts the reference's start band (StartBand)
	if (S.bHasMatch && !bHadMatch) { bStartBand = true; StartBandFrames = 0; PC->RecruitGroupOpen=-1; bRecruitLetters=false; bGroupDropdown=false; GroupDestination=1; SelectionFirst=MenuFirst=AirFirst=RecruitFirst=0; LastPlane=0; }
	bHadMatch = S.bHasMatch;
	// round 14: the lobby's background over its battlefield (index.html #overlay #16191b, the backdrop at opacity 0.5)
	if (!S.bHasMatch) M->Backdrop.DrawDim(Canvas);
	TArray<FString> Lines;
	// the connection, room, seat and tick lines and the key reminder are this client's own diagnostics, with no browser
	// text: drawn as they are, neither translated nor logged as Spanish gaps (Diagnostics counts them from the top)
	int32 Diagnostics = 0;
	Lines.Add(FString::Printf(TEXT("Three Crossroads native slice | %s"), *M->ConnectionStatus()));
	Lines.Add(FString::Printf(TEXT("room %s | %s | mode %s | %s%s"), *S.RoomCode, *S.RoomState, *S.Mode, S.bAmHost ? TEXT("host") : TEXT("guest"), S.bSpectator ? TEXT(" | spectating") : TEXT("")));
	if (S.bHasMatch)
	{
		int32 Own = 0;
		for (const auto& P : S.Units) if (S.IsOwn(P.Value)) Own++;
		const FString Seat = S.bSpectator ? FString(TEXT("watching")) : S.SeatConfirmed() ? FString::Printf(TEXT("seat %d"), S.You) : FString(TEXT("seat unconfirmed"));
		// hud.js drawEcon's income line beside MP (absent for a spectator)
		const FString Income = FTCObjectives::EconomyLine(S);
		// the money as the browser writes it ('{n} MP', es.js '{0} MO'), the one piece of this diagnostic line in the
		// player's language, so it reads like the score strip and the recruit card beside it
		Lines.Add(FString::Printf(TEXT("%s | %s | tick %d | %s%s | entity rows %d (own %d, includes buildings) | selected %d"), *S.Map.Name, *Seat, S.Tick, *TC_T(FString::Printf(TEXT("%d MP"), S.Mp)),
			Income.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(" %s"), *TC_T(Income)), S.Units.Num(), Own, PC->Selected.Num()));
		Diagnostics = Lines.Num();
		// the epilogue's stamp says who won; this line stays for a decided match without one
		if (S.Winner >= -1 && !S.Objectives.Epilogue().bActive) Lines.Add(S.Winner == -1 ? TEXT("Match over: draw") : FString::Printf(TEXT("Match over: team %d wins"), S.Winner));
		// client/hud.js the World Conquest score, until the score strip (which shows it too) has a snapshot to read
		const FTCWorldScoreLead Lead = M->WorldRec.ScoreLead();
		if (Lead.bShown && !bStrip)
		{
			TArray<FString> Blocks;
			for (const FTCWorldTeamScore& T : M->WorldRec.TeamScores())
			{
				TArray<FString> Out;
				for (int32 K = 0; K < T.Members.Num(); K++) if (T.Held.IsValidIndex(K) && !T.Held[K].IsEmpty()) Out.Add(FString::Printf(TEXT("seat %d %s"), T.Members[K], *T.Held[K]));
				Blocks.Add(FString::Printf(TEXT("team %.0f %s %s %s%s"), T.Team, *T.Label, *T.Number, *T.Unit, Out.Num() ? *FString::Printf(TEXT(" (%s)"), *FString::Join(Out, TEXT(", "))) : TEXT("")));
			}
			Lines.Add(FString::Printf(TEXT("%s: %s  |  %s"), *Lead.Mode, *Lead.Clock, *FString::Join(Blocks, TEXT("  |  "))));
		}
	}
	if (!Diagnostics) Diagnostics = Lines.Num();
	// DrawRoomBanners owns #pauseBanner. Repeating it in this diagnostic column drew a ghost pause sentence.
	// client/light.js notice(): the drop to Low's message in the status line for 6 s
	if (!PC->GraphicsNotice.IsEmpty() && PC->GraphicsNoticeAt >= 0 && GetWorld()->GetRealTimeSeconds() - PC->GraphicsNoticeAt < FTCGraphics::NoticeSeconds) Lines.Add(PC->GraphicsNotice);
	int32 KeyLine = INDEX_NONE;
	if (S.bHasMatch)
	{
		// the full key list is the Controls sheet (F1), as in the browser
		KeyLine = Lines.Num();
		Lines.Add(FString::Printf(TEXT("F1 controls | Space newest alert | Alt+H alert history | Shift+Space follow | H home | Ctrl+A army | . idle%s"),
			TCArmyCommands::ClassicLobby(S) ? TEXT(" | Engineers: J depot, K barracks, L motor pool, O airfield, Y flak") : TEXT("")));
	}
	float Y = 12.f;
	FTCMinimapView View;
	// round 15: the status lines as shown (each diagnostic as it is, each browser text through TC_T, the order note
	// through FTCLocale::OrderNote), then drawn in one column
	TArray<FStatusLine> Status;
	for (int32 I = 0; I < Lines.Num(); I++)
	{
		const bool bAsIs = I < Diagnostics || I == KeyLine;
		// Normal play keeps the browser status notices; native counters are an opt-in diagnostic.
		// A lost connection still needs its status line while the held match remains visible.
		if (S.bHasMatch && bAsIs && !FParse::Param(FCommandLine::Get(), TEXT("TCNativeDiagnostics")) &&
			(I != 0 || M->ConnectionStatus().StartsWith(TEXT("in room")))) continue;
		const FString Shown = bAsIs ? Lines[I] : TC_T(Lines[I]);
		if (!bAsIs) NoteKept(Lines[I], Shown);
		Status.Add({ Shown, Text, bAsIs });
	}
	if (M->Controls.TerminalNotice.IsEmpty() && (!S.DenyReason.IsEmpty() || !S.DenyCmd.IsEmpty()))
	{
		const FString Deny = FString::Printf(TEXT("Refused (%s): %s"), *S.DenyCmd, *(M->Controls.TerminalNotice.IsEmpty() ? TCOrders::DenySentence(S.DenyReason, S.DenyCmd) : M->Controls.TerminalNotice));
		const FString Shown = TC_T(Deny);
		NoteKept(Deny, Shown);
		Status.Add({ Shown, Warn });
	}
	// What the last order or button did, for a few seconds: what was sent, or why nothing was (orange)
	const double Now = GetWorld()->GetTimeSeconds();
	if (M->Controls.TerminalNotice.IsEmpty() && !PC->LastOrderNote.IsEmpty() && Now - PC->LastOrderNoteTime < OrderNoteSeconds)
	{
		const FString Shown = FTCLocale::OrderNote(PC->LastOrderNote);
		NoteKept(PC->LastOrderNote, Shown);
		Status.Add({ Shown, PC->bLastOrderSent ? Good : Warn });
	}
	if (S.bHasMatch)
	{
		// the world's own overlays first, under every panel; then the score strip at the top centre
		Minimap.ViewFromCanvas(Canvas, M->Terrain, View);
        View.YawDegrees = PC->ViewYaw();
		TCMarkersHud::Verify(*M, PC->Selected); // round 17: -TCMarkersSelect= and -TCMarkersLog (verification only)
		DrawWorldOverlays(Font);
		// round 14: the paper war map over the world and its markers, under every panel (client/map-view.js)
		M->WarMap.Draw(Canvas, *M, *PC);
		BeginUi(FTCSettingsStore::UIScaleFactor(PC->UiScalePercent));
		// round 16: the match controls in the browser's places (SplitMatchControls). The settings live in the match menu
		// whenever there is one to open; the corner's closed Menu panel sets how far left the strip may reach
		bSettingsInMenu = M->Controls.MenuButton().bShown;
		TArray<FTCControlRow> CornerRows, SandboxRows;
		SplitMatchControls(CornerRows, SandboxRows);
        const FTCHudFit::FBottomLayout Util=FTCHudFit::IconBar(M->Controls.MenuOpen(),M->Controls.MouseCapture.Captured(),FTCType::Measure(),UiNow);
        CornerRows.RemoveAll([](const FTCControlRow& R) { return R.Items.ContainsByPredicate([](const FTCControlItem& I) { return I.Key==TEXT("Menu"); }); });
        const float CornerClosedH=38;
        CornerRight=8+152;
		const int32 StripFrom = Panels.Num();
		const float StripBottom = DrawScoreStrip(Font, Big);
		FBox2D MarkerExclusion = StripCard;
		if (MarkerExclusion.bIsValid) MarkerExclusion.Max.Y = FMath::Max(MarkerExclusion.Max.Y, double(TopStackBottomPx));
		TCMarkersHud::SetHudExclusion(MarkerExclusion);
		// camera.js measures #top's border box. DrawScoreStrip also returns the gap for the next card.
		BandTop = bStrip ? TopStackBottomPx : 0.f;
		// round 15: the strip's left edge in screen pixels (its panels are recorded in screen pixels)
		float StripLeft = Canvas->ClipX * UiNow, StripRight = 0.f, StripLow = StripBottom * UiNow;
		for (int32 K = StripFrom; K < Panels.Num(); K++) { StripLeft = FMath::Min(StripLeft, (float)Panels[K].Min.X); StripRight = FMath::Max(StripRight, (float)Panels[K].Max.X); }
		if (StripRight <= 0.f) StripRight = Canvas->ClipX * UiNow / 2;
		// client/camera.js showFollow: the follow chip under the strip
		const FString Chip = PC->FollowChip();
		if (!Chip.IsEmpty())
		{
			float TW = 0, TH = 0;
			const FString L = Chip + TEXT("  (Shift+Space or pan to stop)");
			LabelSize(L, TW, TH, Font, 1.f);
			AddPanel((Canvas->ClipX - TW) / 2 - Pad, StripBottom, TW + 2 * Pad, Line + 2.f, Panel);
			DrawLabel(L, Good, (Canvas->ClipX - TW) / 2, StripBottom + 1.f, Font, 1.f);
			StripLow = (StripBottom + Line + 2.f) * UiNow;
		}
		// index.html paints #top (including #boss) before #buy, #lowerleft and #minimap.
		// Keep the source-sized frame behind those panels when a narrow view overlaps them.
		if (M->BossBar.Shown())
		{
			const FTCMeasure BossMeasure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
			const FTCPaint Boss=FTCHudFit::BossCard(M->BossBar,FVector2D(Canvas->ClipX,Canvas->ClipY),StripLow/UiNow,FBox2D(ForceInit),{},FPlatformTime::Seconds(),BossMeasure,UiNow);
			FTCType::Draw(Boss,Canvas,Font,UiNow);
			for (const FBox2D& B : Boss.Panels)
			{
				Panels.Add(ToScreen(B));
				if (B.Min.X<Canvas->ClipX/2 && B.Max.X>Canvas->ClipX/2) TopStackBottomPx=FMath::Max(TopStackBottomPx,(float)B.Max.Y*UiNow);
			}
		}
		const float Ui = UiNow;
		EndUi();
		// the minimap (bottom right): the recruit card fits between it and the match panel
		Minimap.UiScale = Ui;
		Minimap.SetRailways(&M->Railways);
		Minimap.bDeferUnitDots = true;
		Minimap.bDeferViewOutline = true; // round 17: client/main.js drawMinimap strokes the view last
		Minimap.Draw(Canvas, S, M->UnitTable, PC->Selected, View);
		DrawMinimapOverlays();
		Minimap.DrawViewOutline(Canvas, View);
		BeginUi(Ui);
		// round 15: the recruit card always sits beside the match panel at the bottom, between it and the minimap
		// (#buy at every UI scale); the match panel narrows only when the window leaves the card less than RecruitMinW
		const float MatchW = FTCHudFit::LeftW;
		// round 16: the match panel stays under the strip (and its notice lines, the boss bar and the follow chip) where
		// they share columns; under the sandbox panel too when that sits under the strip over it. The sandbox panel takes
		// #sandbox's place (230 px over the bottom, centred) when that is clear of the strip, the match panel and the
		// recruit card (last frame's), else it goes under the strip
		const float StripLowUi = StripLow / UiNow;
		const bool bShareStrip = StripRight > StripLeft && StripLeft / UiNow < 8.f + MatchW;
		float PanelLimit = bShareStrip ? StripLowUi + 4.f : 0.f;
		SandboxBox = FBox2D(ForceInit);
		const float MatchTop = DrawMatchPanel(Font, MatchW, PanelLimit) * UiNow; // screen pixels
		const int32 PanelsBeforeRecruit = Panels.Num();
		DrawRecruitPanel(Font, 8.f + MatchW + Gap, Canvas->ClipY - Gap);
		// round 14: camera.js band: from the top panels down to the recruit bar (#buy), the whole height when it is empty
		float RecruitTop = Canvas->ClipY * UiNow; // screen pixels, as the panels are kept
		RecruitBox = FBox2D(ForceInit);
		for (int32 K = PanelsBeforeRecruit; K < Panels.Num(); K++) { RecruitTop = FMath::Min(RecruitTop, (float)Panels[K].Min.Y); RecruitBox += Panels[K]; }
		TArray<FBox2D> Occupied;
		for (const FBox2D& B : Panels) Occupied.Add(FBox2D(B.Min/UiNow,B.Max/UiNow));
		if (Minimap.Rect().bIsValid) Occupied.Add(FBox2D(Minimap.Rect().Min/UiNow-FVector2D(4,4),Minimap.Rect().Max/UiNow+FVector2D(4,4)));
		const FTCMeasure BottomMeasure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
		FTCHudFit::FBottomLayout SandboxLayout;
		if (SandboxRows.Num())
		{
			TArray<FTCHudFit::FControl> Controls;
			for (const FTCControlRow& Row : SandboxRows)
			{
				if (!Row.Text.IsEmpty()) { FTCHudFit::FControl C; C.Label=Row.Text; Controls.Add(C); }
				for (const FTCControlItem& Item : Row.Items)
				{
					FTCHudFit::FControl C; C.Label=Item.Label; C.Hover=Item.Hover; C.Command=ControlCommand(Item.Key,Item.Slot); C.bEnabled=Item.bEnabled; Controls.Add(C);
				}
			}
			SandboxLayout=FTCHudFit::SandboxToolbar(FVector2D(Canvas->ClipX,Canvas->ClipY),CornerRight,
				StripCard.bIsValid?StripCard.Min.X/UiNow:Canvas->ClipX/2,Controls,Occupied,BottomMeasure,UiNow);
			if (SandboxLayout.Paint.Panels.Num()) SandboxBox=SandboxLayout.Paint.Panels[0];
		}
		FVector2D Band(BandTop, RecruitTop);
		if (bStartBand)
		{
			// round 15: the band camera.js measured at receiveStart, until the controller has taken it (StartBand)
			const bool bBuyShown = !S.bSpectator && !TCArmyCommands::ClassicLobby(S);
			Band = FTCHudFit::InitialCameraBand(Canvas->ClipY * UiNow, UiNow, bBuyShown);
			const FVector2D Taken = PC->ClearBand();
			// kept until the strip is up too: the controller measures again when the reported top first turns positive
			// (round 14's glue for a strip that arrives with the first snapshot), which must not happen after this
			if (++StartBandFrames > 600 || (bStrip && FMath::Abs(Taken.X - Band.X) < 0.5f && FMath::Abs(Taken.Y - Band.Y) < 0.5f))
			{
				bStartBand = false;
				UE_LOG(LogTC, Log, TEXT("hud: camera band at the match start %.1f to %.1f px (client/main.js receiveStart: empty #scores, %s), taken after %d frames; this HUD's own band now %.1f to %.1f px"),
					Band.X, Band.Y, bBuyShown ? TEXT("#buy with its group tabs") : TEXT("no #buy for spectators, Classic or World"), StartBandFrames, BandTop, RecruitTop);
			}
		}
		PC->SetClearBand(Band.X, Band.Y);
		// round 16: the sandbox panel (#sandbox), then the corner: the Menu button (#util) and, open, the menu (#menu)
		// under it. The open menu is the browser's dropdown over the HUD (z-index 20): under the strip when it fits there
		// beside the strip's first card, else straight under the Menu button over the strip; drawn last of the panels,
		// so its buttons take the clicks (CommandAt reads the last drawn first)
		if (SandboxBox.bIsValid) DrawBottomLayout(SandboxLayout,Font);
		// the corner's place now (drawn after the status column and the alert feed, over them): the status column keeps
		// to the closed Menu panel (CornerRight), the open menu covering it as the browser's dropdown covers the HUD
		const FVector2D CtlSize = MeasureControlRows(Font, CornerRows);
		float CtlTop = 50.f;
		if (CtlSize.X > 0.f)
		{
			const bool bOverStrip = StripRight > StripLeft && StripLeft / UiNow < 8.f + CtlSize.X + 4.f && CtlTop < StripLowUi;
			if (bOverStrip && StripLowUi + 4.f + CtlSize.Y <= Canvas->ClipY - Gap) CtlTop = StripLowUi + 4.f;
		}
		CornerBox=CtlSize.X>0?ToScreen(TCHudBottom::Box(8,CtlTop,CtlSize.X,CtlSize.Y)):FBox2D(ForceInit);
		float CtlBottom = (8.f+CornerClosedH)*UiNow;
		// round 16 (lead, controls' stats box on hudfit's corner): F2's box under the closed Menu panel (stats.js place:
		// under #util; the open menu is a dropdown over it), the status column under both
		{
			const float ClosedBottom = (8.f+CornerClosedH)*UiNow;
			const float ClosedRight = CornerRight*UiNow;
			CtlBottom = FMath::Max(CtlBottom, DrawStatsBox(Font, ClosedBottom, ClosedRight));
		}
		// under the strip, the status column's last place also clears the sandbox panel (screen pixels): it starts under
		// the strip, or under the sandbox panel when it would reach that panel (#sandbox's own place 230 px over the
		// bottom, or the panel's place under the strip)
		const FBox2D SandboxScreen = SandboxBox.bIsValid ? FBox2D(SandboxBox.Min * UiNow, SandboxBox.Max * UiNow) : FBox2D(ForceInit);
		auto UnderStripTop = [&](int32 Rows)
		{
			const float Top = StripLow + 6.f;
			if (SandboxScreen.bIsValid && Top < SandboxScreen.Max.Y && Top + Rows * Line > SandboxScreen.Min.Y - 4.f) return (float)SandboxScreen.Max.Y + 6.f;
			return Top;
		};
		// round 15: the status column, unscaled (this client's own overlay, outside the browser's zoomed #hud), its lines
		// wrapped at their columns. In the top left corner beside the strip (no wider than ColumnShare of the window, so
		// the paper war map's top left stays clear) when it ends above the match panel; else in the top right corner
		// beside the strip, above the recruit card (the reference's #econ corner, which this HUD leaves empty); else
		// under the strip across the window, as before
		{
			const float Ui2 = UiNow;
			EndUi();
			auto Rows = [&](float W, bool bDiag = true) { TArray<FStatusLine> Out; for (const FStatusLine& L : Status) if (bDiag || !L.bDiag) for (const FString& P : WrapShown(L.Shown, W, Font)) Out.Add({ P, L.Colour, L.bDiag }); return Out; };
			const float LeftW = FMath::Min(StripLeft - 24.f, ColumnShare * Canvas->ClipX), RightW = FMath::Min(Canvas->ClipX - StripRight - 24.f, ColumnShare * Canvas->ClipX);
			TArray<FStatusLine> Col;
			const float LeftTop = CtlBottom > 0.f ? CtlBottom + 6.f : 12.f; // under the match controls
			float Cx = 12.f, Cy = 12.f;
			bool bPlaced = false;
			auto ClearStatus=[&](float X,float Top,float W,int32 Count)
			{
				const FBox2D Box=TCHudBottom::Box(X,Top,W,Count*Line);
				for (const FBox2D& B : Panels) if (TCHudBottom::Overlap(Box,B)) return false;
				if (SandboxScreen.bIsValid && TCHudBottom::Overlap(Box,SandboxScreen)) return false;
                if (CornerBox.bIsValid && TCHudBottom::Overlap(Box,CornerBox)) return false;
                if (Box.Min.Y<0 || Box.Max.Y>Canvas->ClipY) return false;
				return true;
			};
			if (LeftW >= ColumnMinW) { Col = Rows(LeftW); bPlaced = LeftTop + Col.Num() * Line <= MatchTop - 4.f && ClearStatus(12,LeftTop,LeftW,Col.Num()); if (bPlaced) Cy = LeftTop; }
			if (!bPlaced && RightW >= ColumnMinW) { Col = Rows(RightW); Cx = StripRight + 12.f; bPlaced = 12.f + Col.Num() * Line <= RecruitTop - 4.f && ClearStatus(Cx,12,RightW,Col.Num()); }
			if (!bPlaced)
			{
				Col = Rows(Canvas->ClipX - 24.f); Cx = 12.f; Cy = UnderStripTop(Col.Num());
				// round 16: where even that runs into the match panel or the recruit card (a large UI scale in a small
				// window), the column keeps the lines the player needs (pause, notices, refusals, the order note) and
				// leaves out this client's own diagnostics, which the log keeps; before, they were drawn over the panel
				if (Cy + Col.Num() * Line > FMath::Min(MatchTop, RecruitTop) - 4.f || !ClearStatus(Cx,Cy,Canvas->ClipX-24,Col.Num()))
				{
					Col = Rows(Canvas->ClipX - 24.f, false);
					Cy = UnderStripTop(Col.Num());
					if (!bLoggedDiagDrop) { bLoggedDiagDrop = true; UE_LOG(LogTC, Log, TEXT("hud: no room for the status column's diagnostics at %.0f%% in %.0f x %.0f; they stay in the log"), Ui2 * 100.f, Canvas->ClipX, Canvas->ClipY); }
				}
			}
			for (const FStatusLine& L : Col)
            {
                float W=0,H=0; ShownSize(L.Shown,W,H,Font);
                if (ClearStatus(Cx,Cy,W,1)) DrawShown(L.Shown,L.Colour,Cx,Cy,Font,1.f);
                Cy+=Line;
            }
			BeginUi(Ui2);
		}
		// client/alerts.js: the alert feed over the minimap and the Alt+H history
		DrawHint(Font);
		DrawAlertFeed(Font);
		DrawEpilogue(Font, Big);
		// round 16: the corner last (#util, and #menu at z-index 20 over the HUD), on the browser's solid panel colour
		// (--panel-solid) while the menu is open, so what it covers does not show through
        DrawBottomLayout(Util,Font);
        {
         const float SavedScale=UiNow; EndUi();
         const FTCMeasure Msr=FTCType::Measure();
         const FString Shown=TC_T(FString(TEXT("Open teammate FPS")));
         const float W=Msr(Shown,14,false,0)+22;
         FTCPaint FPS; const float X=Canvas->ClipX-W-10;
         FPS.Rect(X,205,W,30,TCCss::PanelSolid); FPS.Frame(X,205,W,30,TCCss::Line);
         FPS.Text(Shown,X+11,210,14,TCCss::Dim);
         FTCType::Draw(FPS,Canvas,Font);
         const FBox2D B(FVector2D(X,205),FVector2D(X+W,235)); Panels.Add(B);
         OfferTooltip(TEXT("Open a separate infantry operative tab for your army"),B);
         BeginUi(SavedScale);
        }
		if (CtlSize.X > 0.f)
		{
			if (M->Controls.MenuOpen()) DrawRect(FLinearColor(FColor(25, 28, 30, 245)), 8.f, CtlTop, CtlSize.X, CtlSize.Y);
			DrawMenuRows(Font,8.f,CtlTop,CornerRows);
		}
	}
	else if (M->bRoomBrowserOpen)
	{
		Minimap.Reset();
		DrawControlRows(Font, 8.f, Y + 4.f, 700.f, M->BrowserRows()); // round 15: the room directory (public-lobby.html)
	}
	else
	{
		// round 15 (lead): the lobby at scale 1: client/hud.js applyScale sets --ui on #hud only, and the lobby card and
		// its report (#overlay) are never zoomed (before, a 150% UI left the report no room in a 1280 px window)
		BeginUi(1.f);
		for (const FStatusLine& L : Status) { DrawShown(L.Shown, L.Colour, 12.f, Y, Font, 1.f); Y += Line; }
		Minimap.Reset();
		// round 15: no band outside a match, so the next match's first frame starts camera.js's measure afresh
		PC->SetClearBand(0.f, 0.f);
		// round 16: the lobby panel narrows (down to LobbyPanelMinW) so the report's table fits beside it
		const float LobbyW = LobbyWidthBesideReport(Font);
		const float LobbyBottom = DrawLobby(Font, Y + 4.f, LobbyW);
		// client/report.js: the result card beside the lobby after a decided match (under it in a narrow window)
		DrawMatchReport(Font, Y + 4.f, LobbyBottom, 8.f + LobbyW + 8.f);
	}
	EndUi();
	DrawRoomBanners(Font); // round 16: #roomBanners (z-index 25: over the HUD, under the sheets)
 if (PendingTooltip.IsEmpty()) for (int32 I=Buttons.Num()-1; I>=0; I--)
 { OfferTooltip(Buttons[I].Label,Buttons[I].Rect); if (!PendingTooltip.IsEmpty()) break; }
 if (PendingTooltip.IsEmpty() && Minimap.Rect().bIsValid) OfferTooltip(TEXT("Minimap"),Minimap.Rect());
	if (!PendingTooltip.IsEmpty() && TooltipAnchor.bIsValid && !PC->ModalSheetOpen())
	{
	 const FTCMeasure Measure=FTCType::IsLoaded()?FTCType::Measure():FTCPaint::CanvasMeasure(Font);
	 const FTCHudFit::FBottomLayout Tip=FTCHudFit::Tooltip(TC_T(PendingTooltip),TooltipAnchor,FVector2D(Canvas->ClipX,Canvas->ClipY),Measure);
	 FTCType::Draw(Tip.Paint,Canvas,Font,1);
	}
	DrawControlsSheet(Font, Big);
	DrawStatsSheet(Font); // round 16: Menu > Stats overlay

	if (PC->bDragging && PC->bDragMoved)
	{
		const FVector2D Lo(FMath::Min(PC->DragFrom.X, PC->DragTo.X), FMath::Min(PC->DragFrom.Y, PC->DragTo.Y));
		const FVector2D Size = (PC->DragFrom - PC->DragTo).GetAbs();
		DrawRect(FLinearColor(0.6f, 1.f, 0.5f, 0.15f), Lo.X, Lo.Y, Size.X, Size.Y);
	}
	// client/main.js marker: a ring on the ground at the click (or the attacked unit), 0.3 m up, that grows from 0.5 to
	// 2.5 m and fades out over 0.6 s, drawn over the terrain, in the order's colour. Only an order that was sent gets one.
	const double Age = Now - PC->LastOrderTime;
	if (M->Terrain && S.bHasMatch && PC->LastOrderAt.X >= 0 && Age >= 0 && Age < MarkerSeconds)
	{
		const float Life = (float)(1.0 - Age / MarkerSeconds), Radius = 0.5f + (1.f - Life) * 2.f;
		const FVector2D At = PC->LastOrderAt;
		const float Up = M->Terrain->HeightAt(At.X, At.Y) + 0.3f;
		FColor C = PC->LastOrderColor;
		C.A = (uint8)FMath::RoundToInt(255.f * Life);
		const FLinearColor Colour(C);
		constexpr int32 Segments = 32;
		FVector Prev = FVector::ZeroVector;
		for (int32 I = 0; I <= Segments; I++)
		{
			const float A = 2.f * PI * I / Segments;
			const FVector P = Project(ATCTerrain::ToWorld(At.X + Radius * FMath::Cos(A), At.Y + Radius * FMath::Sin(A), Up));
			if (I > 0 && P.Z > 0 && Prev.Z > 0) DrawLine(Prev.X, Prev.Y, P.X, P.Y, Colour, 2.f);
			Prev = P;
		}
	}
}

// ---------------------------------------------------------------------------------------------------------------------
#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHudBottomLayoutTest, "ThreeCrossroads.HUD.BottomLayout", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHudBottomLayoutTest::RunTest(const FString&)
{
	using namespace FTCHudFit;
	if (!FTCType::IsLoaded() || !FTCLocale::IsLoaded()) { AddError(TEXT("The reference fonts and dictionary are required for the HUD fit check")); return false; }
	const FTCMeasure Measure=FTCType::Measure();
	for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)})
	for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
	for (float Scale : {1.f,1.5f})
	for (bool Open : {false,true})
	for (int32 Groups : {1,3})
	for (bool WithLogistics : {false,true})
	for (bool Dropdown : {false,true})
	{
		FTCLocale::FScopedLanguage Lang(Language);
		const FVector2D View=Pixels/Scale;
		TArray<FSelection> Selected={{TEXT("rifle"),TEXT("Grenadiers"),1,100,100,{TEXT("In cover"),TEXT("Auto-retreat")}}};
		if (Groups==3)
		{
			Selected.Add({TEXT("mg"),TEXT("MG 42 Team"),1,100,100,{}});
			Selected.Add({TEXT("sniper"),TEXT("Scharfschützen"),1,100,100,{}});
		}
		TArray<FControl> Orders;
		auto Order=[&](const TCHAR* Label,const TCHAR* Icon,const TCHAR* Key,const TCHAR* Value,ETCUiAction Action)
		{
			FControl C; C.Label=Label; C.Icon=Icon; C.Key=Key; C.Value=Value; C.Command=FTCUiCommand(Action); Orders.Add(C);
		};
		Order(TEXT("Retreat"),TEXT("retreat"),TEXT("R"),TEXT(""),ETCUiAction::Retreat);
		Order(TEXT("Attack-move"),TEXT("amove"),TEXT("G"),TEXT(""),ETCUiAction::AttackMove);
		Order(TEXT("Stop"),TEXT("stop"),TEXT("X"),TEXT(""),ETCUiAction::Stop);
		Order(TEXT("Hold fire"),TEXT("holdFire"),TEXT("⇧F"),TEXT("Off"),ETCUiAction::Stance);
		Order(TEXT("Hold position"),TEXT("holdPos"),TEXT("⇧G"),TEXT("Off"),ETCUiAction::Stance);
		Order(TEXT("Auto-retreat"),TEXT("autoRetreat"),TEXT("⇧X"),TEXT("On"),ETCUiAction::Stance);
		Order(TEXT("Take cover"),TEXT("takecover"),TEXT("⇧C"),TEXT(""),ETCUiAction::Cover);
		Order(TEXT("Formation"),TEXT("f_wedge"),TEXT(""),TEXT("Form"),ETCUiAction::Formation);
		Order(TEXT("Build"),TEXT("sandbags"),TEXT(""),TEXT("Build"),ETCUiAction::BuildMenu);
		Order(TEXT("Trench"),TEXT("e_zigzag"),TEXT(""),TEXT("Trench"),ETCUiAction::TrenchMenu);
		Order(TEXT("Grenade"),TEXT("grenade"),TEXT("F A"),TEXT(""),ETCUiAction::Ability);
		for (FControl& C : Orders) if (C.Command.Action==ETCUiAction::Formation || C.Command.Action==ETCUiAction::BuildMenu || C.Command.Action==ETCUiAction::TrenchMenu) C.bSubmenu=true;
		FBottomLayout L=LeftPanel(View.Y,TEXT("1 unit selected"),Selected,{},Orders,{},Measure,Scale,3,1,true,Dropdown);
		const TCHAR* Names[]={TEXT("Infantry"),TEXT("Support weapons"),TEXT("Vehicles"),TEXT("Aircraft")};
		const TCHAR* Icons[]={TEXT("rifle"),TEXT("at"),TEXT("tank"),TEXT("fighter")};
		TArray<FControl> Tabs;
		for (int32 I=0; I<4; I++) { FControl C; C.Label=Names[I]; C.Icon=Icons[I]; C.Command=FTCUiCommand(ETCUiAction::RecruitGroup); C.Command.Group=I; Tabs.Add(C); }
		FControl Recruit; Recruit.Label=TEXT("Recruit"); Recruit.Key=TEXT("Tab"); FControl Rally; Rally.Label=TEXT("Rally"); Rally.Key=TEXT("Shift+H"); Rally.Command=FTCUiCommand(ETCUiAction::Rally);
		TArray<TArray<FControl>> Lists;
		if (Open)
		{
			TArray<FControl> Cards;
			for (const TCHAR* Name : {TEXT("Rifle Squad"),TEXT("Flamethrower Squad"),TEXT("Sniper"),TEXT("Medic Team")}) { FControl C; C.Label=Name; C.Icon=TEXT("rifle"); C.Value=TEXT("100 MP"); C.Command=FTCUiCommand(ETCUiAction::Recruit); Cards.Add(C); }
			Lists.Add(Cards);
		}
		const TArray<FString> Logistics=WithLogistics?TArray<FString>{TEXT("Logistics"),TEXT("Grenadiers #4"),TEXT("Ammunition: 100%"),TEXT("Provisions: 120 s"),TEXT("Supply line: 100%")}:TArray<FString>();
		const FBottomLayout R=RecruitPanel(View.X,View.Y,View.X-184-16,{},Tabs,Lists,{Recruit,Rally},Measure,Scale,Logistics,19);
		FControl Power; Power.Label=TEXT("Recon Flight"); Power.Icon=TEXT("recon"); Power.Key=TEXT("Z"); Power.Value=TEXT("60 MP"); Power.Command=FTCUiCommand(ETCUiAction::Support);
		TArray<FControl> Powers; for (int32 I=0; I<8; I++) Powers.Add(Power);
		const FBottomLayout E=EconomyPanel(View.X,TEXT("100 MP"),TEXT("+5/s"),TEXT("3/12 units"),Powers,Measure,Scale);
		auto Append=[&](const FBottomLayout& Other) { L.Paint.Append(Other.Paint); L.Icons.Append(Other.Icons); L.Hits.Append(Other.Hits); L.TextBounds.Append(Other.TextBounds); };
		Append(R); Append(E);
		L.Paint.Panels.Add(TCHudBottom::Box(View.X-8-184-4,View.Y-8-184-4,188,188));
		L.Paint.Panels.Add(TCHudBottom::Box(8,12,62,54));
		FTCScoreStrip Strip; Strip.Kind=TEXT("horde"); Strip.Lead.bShown=true; Strip.Lead.Mode=TEXT("Wave 1"); Strip.Lead.Clock=TEXT("8 left"); Strip.bWeather=true; Strip.Weather=TEXT("Clear: no effect");
		FTCScoreTeam Team; Team.bBar=true; Team.U0=TEXT("Bunker"); Team.Num=TEXT("99%");
		FTCScoreSeat Seat; Seat.Name=TEXT("Alder"); Seat.Held=TEXT("0 held"); Team.Seats.Add(Seat); Seat.Name=TEXT("Fern"); Seat.Held=TEXT("1 held"); Team.Seats.Add(Seat); Strip.Teams.Add(Team);
		Team=FTCScoreTeam(); Team.Role=TEXT("Horde"); Seat=FTCScoreSeat(); Seat.Name=TEXT("Horde"); Team.Seats.Add(Seat); Strip.Teams.Add(Team);
		// shared/sim.js only enables logistics in Conquest, Classic, Annihilation and World.
		if (WithLogistics)
		{
			Strip=FTCScoreStrip(); Strip.Kind=TEXT("conquest"); Strip.bWeather=true; Strip.Weather=TEXT("Clear: no effect");
			for (const TCHAR* Name : {TEXT("Alder"),TEXT("Fern")})
			{
				FTCScoreTeam Side; Side.bBar=true; Side.Num=TEXT("0 / 1200"); Side.U=TEXT("VP");
				FTCScoreSeat Player; Player.Name=Name; Player.Held=TEXT("0 pts, 0 held"); Side.Seats.Add(Player); Strip.Teams.Add(Side);
			}
		}
		const FTCPaint Scores=ScoreCard(Strip,View.X,Measure,false,Scale); L.Paint.Append(Scores);
		FTCMatchState State; State.bHasMatch=true; State.ModeRow.bPresent=true; State.ModeRow.bBoss=!WithLogistics; State.ModeRow.BossHp=5700; State.ModeRow.BossMax=6000;
		FTCBossBar Boss; Boss.Snapshot(State,0);
		const FBox2D Score=Scores.Panels[0];
		const FTCPaint BossPaint=BossCard(Boss,View,Score.Max.Y+4,E.Economy,L.Paint.Panels,0,Measure,Scale);
		const int32 BeforeBoss=L.Paint.Panels.Num(); L.Paint.Append(BossPaint); L.Paint.Panels.SetNum(BeforeBoss); // source boss may overlap lower-left controls
		float StackBottom=Score.Max.Y;
		if (WithLogistics)
		{
			const FTCPaint Rate=MatchNoticeCard(TC_T(FString(TEXT("Scores tied"))),TEXT(""),TEXT(""),View.X,StackBottom+4,Measure,Scale);
			L.Paint.Append(Rate); StackBottom=Rate.Panels.Last().Max.Y;
		}
		for (const FBox2D& B : BossPaint.Panels) if (B.Min.X<View.X/2 && B.Max.X>View.X/2) StackBottom=FMath::Max(StackBottom,(float)B.Max.Y);
		const FTCPaint Pause=FTCRoomBanners::Layout({FTCRoomBanner{TEXT("Paused by Alder"),false}},FMath::CeilToFloat(StackBottom*Scale+8),Pixels.X,FTCPaint::CanvasMeasure(GEngine->GetSmallFont()));
        // The source centres the pause below the boss even over an open recruitment list.
        TArray<FBox2D> SandboxAvoid=L.Paint.Panels;
        for (const FTCPaintItem& I : Pause.Items) if (I.Role==TEXT("banner:pause")) SandboxAvoid.Add(FBox2D(I.Pos/Scale,(I.Pos+I.Size)/Scale));
		TArray<FControl> Sandbox;
		for (const TCHAR* Label : {TEXT("Sandbox"),TEXT("Rifle Squad"),TEXT("Enemy"),TEXT("Spawn")}) { FControl C; C.Label=Label; Sandbox.Add(C); }
		Append(SandboxToolbar(View,70,Score.Min.X,Sandbox,SandboxAvoid,Measure,Scale));
		const FString Case=FString::Printf(TEXT("%g x %g %s %g%% %s %d groups logistics %d"),Pixels.X,Pixels.Y,Language==ETCLanguage::Spanish?TEXT("es"):TEXT("en"),Scale*100,Open?TEXT("open"):TEXT("tabs"),Groups,WithLogistics);
		FTCAlerts Alerts;
		for (const TCHAR* Type : {TEXT("rifle"),TEXT("tank"),TEXT("mortar"),TEXT("medic"),TEXT("mg"),TEXT("attacker"),TEXT("flamer"),TEXT("medium")})
			Alerts.Push(TEXT("unitLost"),FTCAlerts::FactionUnitName(1,Type)+FString(TEXT(" lost")),true,FVector2D(20,20),0);
		Alerts.Push(TEXT("base"),TEXT("Our base is under attack!"),true,FVector2D(20,20),0);
		Alerts.Push(TEXT("ready"),TEXT("Rifle Squad ready"),true,FVector2D(20,20),0);
		for (bool History : {false,true})
		{
		 Alerts.bHistoryOpen=History;
		 FTCAlertViewInput Input; Input.Now=1; Input.ViewW=View.X; Input.ViewH=View.Y; Input.UiScale=Scale;
		 Input.Right=View.X-8; Input.MinimapTop=View.Y-8-184; Input.Avoid=L.Paint.Panels;
		 const FTCPaint Feed=Alerts.Layout(Input,Measure,FTCType::HeavyMeasure());
		 if (!History) TestEqual(Case+TEXT(": both recent losses remain visible in the full HUD"),Feed.TextsOf(TEXT("alert:loss-heading")).Num(),2);
		 TestNotNull(Case+FString::Printf(TEXT(" dropdown %d history %d: bounded history remains reachable"),Dropdown,History),Feed.Find(TEXT("alert:history")));
		 for (const FBox2D& Box : Feed.Panels)
		 {
		  // alerts.js bottom-anchors the unscaled column, including when routine lines push its top offscreen.
		  TestTrue(TEXT("alert panel stays in horizontal bounds and above the minimap"),Box.Min.X>=0 && Box.Max.X<=View.X+.1 && Box.Max.Y<=Input.MinimapTop-13+.1);
		  TestTrue(TEXT("source alert column stays anchored right"),FMath::IsNearlyEqual(Box.Max.X,double(Input.Right),.01));
		 }
         TestTrue(Case+TEXT(": history bottom follows browser minimap frame and gap"),FMath::IsNearlyEqual(Feed.Panels.Last().Max.Y,double(Input.MinimapTop-13),.01));
         TestEqual(Case+TEXT(": one history summary line"),Feed.TextsOf(TEXT("alert:history-summary")).Num(),1);
		 for (int32 I=0; I<Feed.Panels.Num(); I++) for (int32 J=I+1; J<Feed.Panels.Num(); J++) TestFalse(TEXT("alert cards do not overlap"),TCHudBottom::Overlap(Feed.Panels[I],Feed.Panels[J]));
		}
		for (const FString& Error : LayoutErrors(L,View)) AddError(Case+TEXT(": ")+Error);
		TestEqual(Case+TEXT(": selection preserved"),L.Paint.TextsOf(TEXT("bottom:selection-title")),TArray<FString>{TC_T(FString(TEXT("1 unit selected")))});
		TestTrue(Case+TEXT(": first selected type stays visible"),L.Paint.TextsOf(TEXT("bottom:selection-name")).Num()>0);
		TestEqual(Case+TEXT(": selected status chips stay visible"),L.Paint.TextsOf(TEXT("bottom:status-text")).Num(),2);
		TestEqual(Case+TEXT(": every category stays available"),R.Paint.TextsOf(TEXT("bottom:tab-label")).Num(),4);
        double TabY=-1;
        for (const FTCPaintItem& I : R.Paint.Items) if (I.Role==TEXT("bottom:tab"))
        { if (TabY<0) TabY=I.Pos.Y; TestEqual(Case+TEXT(": categories share one row"),I.Pos.Y,TabY); }
		if (!Dropdown) TestTrue(Case+TEXT(": Form value is present, with no extra clipped summary"),L.Paint.TextsOf(TEXT("bottom:value")).Contains(TC_T(FString(TEXT("Form")))));
		TestEqual(Case+TEXT(": recruitment heading is above the tabs"),R.Paint.TextsOf(TEXT("bottom:above-label"))[0],TC_T(FString(TEXT("Recruit"))));
	}
	return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHudPanelDetailsTest, "ThreeCrossroads.HUD.PanelDetails", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHudPanelDetailsTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 const FTCMeasure Measure=FTCType::Measure();
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)})
 for (float Scale : {1.f,1.5f})
 {
  FTCLocale::FScopedLanguage Lang(Language); const FVector2D View=Pixels/Scale;
  FControl Mun; Mun.Label=TEXT("Mun"); Mun.Value=TEXT("250"); Mun.Icon=TEXT("resource-mun");
  FControl Fuel; Fuel.Label=TEXT("Fuel"); Fuel.Value=TEXT("75"); Fuel.Icon=TEXT("resource-fuel");
  FControl Plane; Plane.Label=TEXT("Ground-attack Plane"); Plane.Value=TEXT("On station 25s"); Plane.Command=TCHudBottom::Local(TEXT("plane:42"));
  const FBottomLayout E=EconomyPanel(View.X,TEXT("476 MP"),TEXT("+5/s (upkeep −2)"),TEXT("3/24 units"),{},Measure,Scale,{Mun,Fuel},{Plane});
  for (const FString& Error : LayoutErrors(E,View)) AddError(Error);
  TestEqual(TEXT("both construction resources survive"),E.Paint.TextsOf(TEXT("bottom:resource")).Num(),2);
  TestEqual(TEXT("plane state survives"),E.Paint.TextsOf(TEXT("bottom:air-state")),TArray<FString>{TC_T(Plane.Value)});
  FControl Order; Order.Label=TEXT("Formation"); Order.Icon=TEXT("f_wedge"); Order.Value=TEXT("Form"); Order.bSubmenu=true;
  const FBottomLayout L=LeftPanel(View.Y,TEXT("1 unit selected"),{{TEXT("rifle"),TEXT("Grenadiers"),1,100,100,{TEXT("In cover"),TEXT("Auto-retreat")}}},{},{Order},{},Measure,Scale,3,2,true,true);
  for (const FString& Error : LayoutErrors(L,View)) AddError(Error);
  TestEqual(TEXT("each status has its own chip"),L.Paint.TextsOf(TEXT("bottom:status-text")).Num(),2);
  const FTCPaintItem* Health=L.Paint.Find(TEXT("bottom:selection-health"));
  TArray<const FTCPaintItem*> Status;
  for (const FTCPaintItem& I : L.Paint.Items) if (I.Role==TEXT("bottom:status-text")) Status.Add(&I);
  TestTrue(TEXT("cover shares health row and auto-retreat follows on its own row"),Health && Status.Num()==2 && FMath::IsNearlyEqual(Status[0]->Pos.Y,Health->Pos.Y+1,.01) && Status[1]->Pos.Y>Status[0]->Pos.Y+17);
  const FBottomLayout Util=IconBar(false,false,Measure,Scale);
  TestEqual(TEXT("four source toolbar controls"),Util.Hits.Num(),4);
  for (const FHit& H : Util.Hits) TestTrue(TEXT("toolbar hit stays in source panel"),H.Box.Min.X>=8 && H.Box.Min.Y>=8 && H.Box.Max.X<=160 && H.Box.Max.Y<=46);
  TestTrue(TEXT("fullscreen routes to the existing handler"),Util.Hits[1].Control.Command.Unit==TEXT("Fullscreen"));
  TestTrue(TEXT("capture routes to the existing handler"),Util.Hits[2].Control.Command.Unit==TEXT("CaptureNow"));
  TestTrue(TEXT("help routes to F1"),Util.Hits[3].Control.Command.Action==ETCUiAction::ControlsSheet);
  const FString MenuTip=TC_T(Util.Hits[0].Control.Hover);
  const FBottomLayout Popup=Tooltip(MenuTip,FBox2D(Util.Hits[0].Box.Min*Scale,Util.Hits[0].Box.Max*Scale),Pixels,Measure);
  TestEqual(TEXT("Menu has one measured tooltip line"),Popup.Paint.TextsOf(TEXT("bottom:tooltip-text")).Num(),1);
  TestEqual(TEXT("tooltips keep source physical type at either HUD scale"),Popup.Paint.Find(TEXT("bottom:tooltip-text"))->FontPx,14.f);
  for (const FString& Error : LayoutErrors(Popup,Pixels)) AddError(Error);
  const FTCPaintItem* Pop=E.Paint.Find(TEXT("bottom:population")), *Inc=E.Paint.Find(TEXT("bottom:income"));
  TestTrue(TEXT("income and population stay grouped"),Pop && Inc && FMath::IsNearlyEqual(Pop->Pos.Y,Inc->Pos.Y,.01) && Pop->Pos.X-Inc->Pos.X<Measure(TC_T(FString(TEXT("3/24 units"))),13,false,0)+13);
  TestEqual(TEXT("dropdown offers all nine groups"),L.Hits.FilterByPredicate([](const FHit& H) { return H.Control.Command.Unit.StartsWith(TEXT("hudpanel:destination:")); }).Num(),9);
  TArray<FSelection> Types;
  for (const TCHAR* Type : {TEXT("rifle"),TEXT("mg"),TEXT("sniper"),TEXT("tank"),TEXT("mortar"),TEXT("at")}) Types.Add({Type,Type,1,100,100,{TEXT("In cover"),TEXT("Auto-retreat")}});
  const FBottomLayout Scrolled=LeftPanel(View.Y,TEXT("6 units selected"),Types,{},{Order},{},Measure,Scale,3,2,true,false,5);
  for (const FString& Error : LayoutErrors(Scrolled,View)) AddError(Error);
  TestEqual(TEXT("scrolling reaches the last selected type"),Scrolled.Paint.TextsOf(TEXT("bottom:selection-name")),TArray<FString>{TC_T(FString(TEXT("at")))});
  TestEqual(TEXT("all types remain scrollable"),Scrolled.SelectionMax,5);
  for (FVector2D Corner : {FVector2D(0,0),FVector2D(View.X-2,0),FVector2D(0,View.Y-2),View-FVector2D(2,2)})
  {
   const FBottomLayout Tip=Tooltip(TC_T(FString(TEXT("Find the next idle unit. Shift+click selects all idle units"))),TCHudBottom::Box(Corner.X,Corner.Y,2,2),View,Measure);
   for (const FString& Error : LayoutErrors(Tip,View)) AddError(Error);
  }
 }
 for (const TCHAR* Key : {TEXT("faction-0"),TEXT("faction-1"),TEXT("faction-2"),TEXT("faction-3"),TEXT("resource-mun"),TEXT("resource-fuel"),TEXT("menu-open"),TEXT("menu-closed"),TEXT("rifle"),TEXT("key-shift")})
 {
  bool Found=false; for (const TCHAR* Existing : HudIconKeys) Found|=FCString::Strcmp(Key,Existing)==0;
  TestTrue(FString(TEXT("original art in generated atlas: "))+Key,Found);
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHudLargeListsTest, "ThreeCrossroads.HUD.LargeLists", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHudLargeListsTest::RunTest(const FString&)
{
 using namespace FTCHudFit;
 const FTCMeasure Measure=FTCType::Measure();
 for (ETCLanguage Language : {ETCLanguage::English,ETCLanguage::Spanish})
 for (FVector2D Pixels : {FVector2D(1280,720),FVector2D(1920,1080)})
 for (float Scale : {1.f,1.5f})
 {
  FTCLocale::FScopedLanguage Lang(Language); const FVector2D View=Pixels/Scale;
  const FString Case=FString::Printf(TEXT("%s %gx%g UI%g: "),Language==ETCLanguage::Spanish?TEXT("es"):TEXT("en"),Pixels.X,Pixels.Y,100*Scale);
  TArray<FControl> Orders,Menu,MenuTabs,Cards,Tabs,Air,Support;
  for (const TCHAR* Name : {TEXT("Stop"),TEXT("Attack-move"),TEXT("Retreat"),TEXT("Hold fire"),TEXT("Hold position"),TEXT("Auto-retreat"),TEXT("Take cover"),TEXT("Formation"),TEXT("Build"),TEXT("Trench")})
  { FControl C; C.Label=Name; C.Icon=TEXT("stop"); C.bSubmenu=Orders.Num()>=7; C.Value=C.bSubmenu?Name:TEXT(""); C.Hover=Name; Orders.Add(C); }
  for (int32 I=0; I<36; I++) { FControl C; C.Label=TEXT("Supply Cache"); C.Icon=TEXT("supplycache"); C.Value=TEXT("60 MP"); C.Command=TCHudBottom::Local(FString::Printf(TEXT("item:%d"),I)); Menu.Add(C); }
  for (const TCHAR* Name : {TEXT("Base"),TEXT("Defenses"),TEXT("Field works"),TEXT("Engineering")})
  { FControl C; C.Label=Name; C.Hover=Name; C.Command=TCHudBottom::Local(TEXT("buildtab:0")); MenuTabs.Add(C); }
  for (const TCHAR* Name : {TEXT("Infantry"),TEXT("Support weapons"),TEXT("Vehicles"),TEXT("Aircraft"),TEXT("Naval")})
  { FControl C; C.Label=Name; C.Icon=TEXT("rifle"); C.Command=FTCUiCommand(ETCUiAction::RecruitGroup); Tabs.Add(C); }
  for (int32 I=0; I<32; I++)
  { FControl C; C.Label=TEXT("Flamethrower Squad"); C.Icon=TEXT("flamer"); C.Value=TEXT("180 MP"); C.Command=FTCUiCommand(ETCUiAction::Recruit); C.Command.Unit=FString::FromInt(I); Cards.Add(C); }
  for (int32 I=0; I<40; I++)
  { FTCAirRow Row; Row.State=I%5; Row.Fuel=25; Row.Timer=30;
    FControl C; C.Label=TEXT("Ground-attack Plane"); C.Value=FTCObjectives::AirStateText(Row); C.Command=TCHudBottom::Local(FString::Printf(TEXT("plane:%d"),I+1)); Air.Add(C); }
  for (int32 I=0; I<8; I++)
  { FControl C; C.Label=TEXT("Recon Flight"); C.Icon=TEXT("recon"); C.Value=TEXT("60 MP"); C.Key=TEXT("Z"); C.Command=FTCUiCommand(ETCUiAction::Support); Support.Add(C); }
  const TArray<FSelection> Selection={{TEXT("engineer"),TEXT("Engineer Squad"),1,100,100,{TEXT("In cover"),TEXT("Auto-retreat")}}};
  const FBottomLayout First=LeftPanel(View.Y,TEXT("1 unit selected"),Selection,{},Orders,Menu,Measure,Scale,3,1,true,false,0,80,0,MenuTabs);
  const FBottomLayout Last=LeftPanel(View.Y,TEXT("1 unit selected"),Selection,{},Orders,Menu,Measure,Scale,3,1,true,false,0,80,First.MenuMax,MenuTabs);
  TestTrue(Case+TEXT("first selected type stays visible with a large menu"),First.Paint.TextsOf(TEXT("bottom:selection-name")).Num()>0);
  TestTrue(Case+TEXT("last submenu row reachable"),Last.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Unit==TEXT("hudpanel:item:35"); }));
  FControl Mun; Mun.Label=TEXT("Mun"); Mun.Value=TEXT("250"); Mun.Icon=TEXT("resource-mun");
  FControl Fuel; Fuel.Label=TEXT("Fuel"); Fuel.Value=TEXT("75"); Fuel.Icon=TEXT("resource-fuel");
  const FBottomLayout Econ=EconomyPanel(View.X,TEXT("476 MP"),TEXT("+5/s (upkeep −2)"),TEXT("3/24 units"),Support,Measure,Scale,{Mun,Fuel},Air,View.Y-208);
  const FBottomLayout EconLast=EconomyPanel(View.X,TEXT("476 MP"),TEXT("+5/s (upkeep −2)"),TEXT("3/24 units"),Support,Measure,Scale,{Mun,Fuel},Air,View.Y-208,Econ.AirMax);
  const FBottomLayout Recruit=RecruitPanel(View.X,View.Y,View.X-200,{},Tabs,{Cards},{},Measure,Scale,{},32,0,80,Econ.Economy);
  const FBottomLayout RecruitLast=RecruitPanel(View.X,View.Y,View.X-200,{},Tabs,{Cards},{},Measure,Scale,{},32,Recruit.RecruitMax,80,Econ.Economy);
  TestTrue(Case+TEXT("last recruit row reachable"),RecruitLast.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Unit==TEXT("31"); }));
  TestTrue(Case+TEXT("recruit overflow has explicit next/previous controls"),Recruit.RecruitMax==0 || Recruit.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Unit==TEXT("hudpanel:scroll:recruit:1"); }));
  TestTrue(Case+TEXT("last plane row reachable"),EconLast.Hits.ContainsByPredicate([](const FHit& H) { return H.Control.Command.Unit==TEXT("hudpanel:plane:40"); }));
  for (const FBottomLayout* L : {&First,&Last,&Recruit,&RecruitLast,&Econ,&EconLast})
   for (const FString& Error : LayoutErrors(*L,View)) AddError(Case+Error);
  TestTrue(Case+TEXT("economy and planes stay above minimap"),Econ.Economy.Max.Y<=View.Y-192);
  FBottomLayout Combined=First;
  for (const FBottomLayout* L : {&Recruit,&Econ})
  { Combined.Paint.Append(L->Paint); Combined.TextBounds.Append(L->TextBounds); Combined.Hits.Append(L->Hits); Combined.Icons.Append(L->Icons); }
  Combined.Paint.Panels.Add(TCHudBottom::Box(View.X-196,View.Y-196,188,188));
  for (const FString& Error : LayoutErrors(Combined,View)) AddError(Case+Error);
 }
 return !HasAnyErrors();
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHudPlanesTest, "ThreeCrossroads.HUD.Planes", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHudPlanesTest::RunTest(const FString&)
{
 FTCMatchState S; S.You=0; S.bHasMatch=true; S.bSpectator=false; S.LobbySeat=0; S.bLobbySinceOpen=true;
 for (int32 Id=1; Id<=3; Id++) { FTCUnit U; U.Id=Id; U.Type=Id==2?TEXT("bomber"):TEXT("fighter"); U.Owner=Id==3?1:0; S.Units.Add(Id,U); FTCAirRow R; R.Id=Id; S.Air.Add(R); }
 FTCAirRow Missing; Missing.Id=4; S.Air.Add(Missing);
 TestTrue(TEXT("single click replaces selection"),FTCHudFit::PlaneSelection(S,{99},1,false,false).Contains(1) && FTCHudFit::PlaneSelection(S,{99},1,false,false).Num()==1);
 TestTrue(TEXT("Shift/Ctrl click adds"),FTCHudFit::PlaneSelection(S,{99},1,true,false).Num()==2);
 TestTrue(TEXT("Shift/Ctrl click drops"),!FTCHudFit::PlaneSelection(S,{1,2},1,true,false).Contains(1));
 const TSet<int32> All=FTCHudFit::PlaneSelection(S,{99},1,false,true);
 TestTrue(TEXT("double click replaces with every delivered own plane of all types"),All.Num()==2 && All.Contains(1) && All.Contains(2));
 TestEqual(TEXT("double click also replaces when Shift is held"),FTCHudFit::PlaneSelection(S,{99},1,true,true).Num(),2);
 TestTrue(TEXT("missing or foreign row cannot select"),FTCHudFit::PlaneSelection(S,{99},3,false,false).Contains(99) && FTCHudFit::PlaneSelection(S,{99},4,false,false).Contains(99));
 const TCHAR* Expected[]={TEXT("Ready"),TEXT("Flying out"),TEXT("On station 25s"),TEXT("Heading home"),TEXT("Rearming 30s")};
 for (int32 I=0; I<5; I++) { FTCAirRow R; R.State=I; R.Fuel=25; R.Timer=30; TestEqual(TEXT("browser AIR_STATE and fuel/timer field"),FTCObjectives::AirStateText(R),FString(Expected[I])); }
 return !HasAnyErrors();
}

// Round 16 (hudfit): the fit rules at UI 150% in a 1280 x 720 window (853.3 x 480 UI pixels), the case slice 1p left
// text and settings controls retained from the earlier fit test. BottomLayout guards the measured replacement.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHudFitTest, "ThreeCrossroads.HUD.Fit", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHudFitTest::RunTest(const FString&)
{
	// round 17: the Horde record after the mode's description (main.js renderLobby), and its Spanish (es.js keys)
	TestEqual(TEXT("no record, three defenders"), FTCHudFit::HordeRecordText(3, false, 0, 0, {}), FString(TEXT(" No record yet here with 3 defenders.")));
	TestEqual(TEXT("no record, one defender"), FTCHudFit::HordeRecordText(1, false, 0, 0, {}), FString(TEXT(" No record yet here with 1 defender.")));
	TestEqual(TEXT("connection banner after a loss"), FTCHudFit::ConnectionBannerText(false, 2), FString(TEXT("Connection lost. Retrying in 2 s")));
	TestEqual(TEXT("connection banner for a full room"), FTCHudFit::ConnectionBannerText(true, 10), FString(TEXT("Trying this room again in 10 s")));
	TestEqual(TEXT("a record"), FTCHudFit::HordeRecordText(2, true, 7, 545, { TEXT("Ann"), TEXT("Bo") }), FString(TEXT(" Record here with 2 defenders: wave 7 in 9:05 (Ann, Bo).")));
	if (FTCLocale::IsLoaded())
	{
		TestEqual(TEXT("es: no record"), FTCLocale::Translate(TEXT("No record yet here with 3 defenders."), ETCLanguage::Spanish), FString(TEXT("Todavía no hay récord aquí con 3 defensores.")));
		TestEqual(TEXT("es: connection lost"), FTCLocale::Translate(FTCHudFit::ConnectionBannerText(false, 4), ETCLanguage::Spanish), FString(TEXT("Conexión perdida. Reintentando en 4 s")));
		TestEqual(TEXT("es: full room"), FTCLocale::Translate(FTCHudFit::ConnectionBannerText(true, 10), ETCLanguage::Spanish), FString(TEXT("Reintentando esta sala en 10 s")));
		TestEqual(TEXT("es: a record"), FTCLocale::Translate(TEXT("Record here with 2 defenders: wave 7 in 9:05 (Ann, Bo)."), ETCLanguage::Spanish), FString(TEXT("Récord aquí con 2 defensores: oleada 7 en 9:05 (Ann, Bo).")));
	}
	// the settings in the match menu: their keys are the HUD's own actions; the session controls stay match controls
	const TPair<const TCHAR*, ETCUiAction> Keys[] = { { TEXT("Setting.Graphics"), ETCUiAction::Graphics }, { TEXT("Setting.Gore"), ETCUiAction::Gore },
		{ TEXT("Setting.EdgeScroll"), ETCUiAction::EdgeScroll }, { TEXT("Setting.PanSpeed"), ETCUiAction::PanSpeed }, { TEXT("Setting.Cover"), ETCUiAction::CoverToggle },
		{ TEXT("Setting.Volume"), ETCUiAction::Volume }, { TEXT("Setting.UiScale"), ETCUiAction::UiScale }, { TEXT("Setting.Language"), ETCUiAction::Language } };
	for (const auto& K : Keys) TestEqual(FString::Printf(TEXT("%s's action"), K.Key), (int32)ATCHUD::ControlCommand(K.Key, -1).Action, (int32)K.Value);
	const FTCUiCommand Hand = ATCHUD::ControlCommand(TEXT("HandAi"), 2);
	TestTrue(TEXT("HandAi stays a match control with its slot"), Hand.Action == ETCUiAction::MatchControl && Hand.Unit == TEXT("HandAi") && Hand.Group == 2);
	TestTrue(TEXT("an unknown setting key stays a match control (RunControl reports it)"), ATCHUD::ControlCommand(TEXT("Setting.Nothing"), -1).Action == ETCUiAction::MatchControl);
	return true;
}
#endif
