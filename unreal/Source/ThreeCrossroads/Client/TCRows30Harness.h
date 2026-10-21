// Focused row fixtures. Automation friends are supplied in the lead patch.
#pragma once
#if WITH_DEV_AUTOMATION_TESTS
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "Client/TCAudio.h"
#include "Client/TCGraphics.h"
#include "KeyState.h"
#include "World/TCTerrain.h"
#include "Net/TCToken.h"
#include "IWebSocket.h"
#include "Misc/AutomationTest.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Engine/Engine.h"
#include "Engine/GameInstance.h"
#include "Engine/World.h"
#include "Engine/Canvas.h"
#include "GameFramework/PlayerInput.h"
#include "GameFramework/WorldSettings.h"

namespace TCRows30
{
inline TSharedPtr<FJsonObject> Json(const FString& Text)
{
 TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), O); return O;
}
// Transport sink only. Every command still passes the production controller, GameMode and connection gates.
class FSocket final : public IWebSocket
{
public:
 TArray<TSharedPtr<FJsonObject>> Sent;
 void Connect() override {} void Close(int32, const FString&) override {} bool IsConnected() override { return true; }
 void Send(const FString& Data) override { Sent.Add(Json(Data)); }
 void Send(const void*, SIZE_T, bool) override {} void SetTextMessageMemoryLimit(uint64) override {}
 FWebSocketConnectedEvent Connected; FWebSocketConnectionErrorEvent Error; FWebSocketClosedEvent Closed;
 FWebSocketMessageEvent Message; FWebSocketBinaryMessageEvent Binary; FWebSocketRawMessageEvent Raw; FWebSocketMessageSentEvent MessageSent;
 FWebSocketConnectedEvent& OnConnected() override { return Connected; }
 FWebSocketConnectionErrorEvent& OnConnectionError() override { return Error; }
 FWebSocketClosedEvent& OnClosed() override { return Closed; }
 FWebSocketMessageEvent& OnMessage() override { return Message; }
 FWebSocketBinaryMessageEvent& OnBinaryMessage() override { return Binary; }
 FWebSocketRawMessageEvent& OnRawMessage() override { return Raw; }
 FWebSocketMessageSentEvent& OnMessageSent() override { return MessageSent; }
};
inline TSharedPtr<FJsonObject> Lobby(int32 Seats = 2, const FString& Mode = TEXT("conquest"), bool Host = true, bool Playing = false)
{
 auto O = Json(TEXT(R"({"t":"lobby","code":"rows30","state":"lobby","you":0,"host":0,"amHost":true,"spectator":false,"role":"commander","mode":"conquest","mapName":"default","maps":["default"],"spawns":4,"defenderTeam":0,"army":"standard","weather":"map","sandbox":false,"listed":true,"players":[]})"));
 O->SetStringField(TEXT("mode"),Mode); O->SetBoolField(TEXT("amHost"),Host); O->SetStringField(TEXT("state"),Playing?TEXT("play"):TEXT("lobby"));
 TArray<TSharedPtr<FJsonValue>> Players;
 for (int32 I=0;I<Seats;I++) { auto P=MakeShared<FJsonObject>(); P->SetStringField(TEXT("name"),FString::Printf(TEXT("Seat%d"),I)); P->SetNumberField(TEXT("team"),I%2); P->SetNumberField(TEXT("faction"),I%2); P->SetBoolField(TEXT("connected"),true); P->SetBoolField(TEXT("ai"),false); Players.Add(MakeShared<FJsonValueObject>(P)); }
 O->SetArrayField(TEXT("players"),Players); return O;
}
}

struct FTCRows30Access
{
 static void Attach(ATCGameMode& M,const TSharedPtr<TCRows30::FSocket>& Socket)
 { M.Connection=MakeShared<FTCConnection>(FTCConnection::FSettings()); M.Connection->Socket=Socket; M.Connection->bRunning=true; M.Connection->Host=ETCHostKind::Reference; }
 static void Receive(ATCGameMode& M,const FJsonObject& O) { M.OnMessage(O,0); }
 static int32 VoiceAsked(const UTCAudio& A,const FString& Kind) { return A.Asked.FindRef(TEXT("voice:")+Kind); }
 static void MemoryGraphics(FTCGraphics& G,bool On) { if(On) { GraphicsWasMemory=G.bMemoryOnly; GraphicsMemory=G.Memory; } G.bMemoryOnly=On?true:GraphicsWasMemory; if(!On) G.Memory=GraphicsMemory; }
 inline static bool GraphicsWasMemory=false;
 inline static TMap<FString,FString> GraphicsMemory;
 static void Projection(ATCPlayerController& PC) { PC.Rows30Projection=[](const FTCUnit&,FVector2D& P) { P=FVector2D(100,100); return true; }; }
 static void RefreshFrame(ATCGameMode& M,bool Shown,bool Navigating)
 { M.ServerUrl=TEXT("invalid-rows30"); M.bRoomBrowserOpen=Shown; M.bQuickPlayPending=Navigating; M.QuickPlayAnswers=M.Rooms.Answers(); M.Controls.LastViewWrite=TNumericLimits<double>::Max(); M.ControlsFrame(); }
 static void Camera(ATCPlayerController& PC,float Dt) { PC.UpdateCamera(Dt,true); }
 static void Pose(ATCPlayerController& PC,FVector2D Focus,float Yaw,float Dist,int32 Speed=1)
 { PC.Focus=Focus; PC.Yaw=Yaw; PC.Distance=Dist; PC.PanSpeed=Speed; PC.bEdgeScroll=false; PC.bIntroPending=false; PC.Intro=FTCCameraIntro(); }
 static void Wide(ATCPlayerController& PC,const ATCGameMode& M,int32 W,int32 H) { PC.UpdateWideView(M,W,H); }
 static void Clamp(ATCPlayerController& PC,const ATCGameMode& M) { PC.ClampFocus(M); }
 static void Clock(FTCRoomBrowser& B,double Now,bool Loading=false) { B.LastRefreshAt=Now; B.bLoading=Loading; }
 static bool Binding(ATCPlayerController& PC,const TCHAR* Code,bool Shift=false,bool Ctrl=false)
 { const auto* B=TCKeys::Match(Code,Shift,Ctrl,false,{ETCKeyContext::Army}); return B && PC.RunBinding(*B,Shift); }
 static void HudOwner(ATCHUD& Hud,ATCPlayerController* PC) { Hud.PlayerOwner=PC; }
 static bool AddAiShown(ATCHUD& Hud,UCanvas* Canvas)
 { Hud.Canvas=Canvas; Hud.Rows30GatherOnly=true; Hud.Buttons.Reset(); Hud.DrawLobby(GEngine->GetSmallFont(),20,680); for (const auto& B:Hud.Buttons) if (B.Command.Action==ETCUiAction::AddAi) return true; return false; }
 static void HouseFixture(FTCWorldObjects& O,const TArray<TCWorldCore::FObjShape>& Shapes)
 { O.W=4; O.H=2; O.Grid.Init('.',8); O.Grid[1]='B'; O.ChunkPicks={Shapes}; O.ChunkHouses.SetNum(1); O.ChunkPick.SetNum(1); for(const auto& S:Shapes) O.ChunkPick[0].Add(S); }
};

namespace TCRows30
{
struct FRows30Scene
{
 UWorld* World=nullptr; ATCGameMode* M=nullptr; ATCPlayerController* PC=nullptr; ATCCameraPawn* Cam=nullptr;
 TSharedPtr<FSocket> Socket=MakeShared<FSocket>();
 FRows30Scene()
 {
  if (!GEngine) return;
  World=UWorld::CreateWorld(EWorldType::Game,false); if (!World) return;
  auto* Instance=NewObject<UGameInstance>(World); World->SetGameInstance(Instance);
  auto& Context=GEngine->CreateNewWorldContext(EWorldType::Game); Context.SetCurrentWorld(World); Context.OwningGameInstance=Instance;
  World->GetWorldSettings()->DefaultGameMode=ATCGameMode::StaticClass(); if(!World->SetGameMode(FURL())) return;
  M=World->GetAuthGameMode<ATCGameMode>(); PC=World->SpawnActor<ATCPlayerController>(); Cam=World->SpawnActor<ATCCameraPawn>();
  if (!M || !PC || !Cam) return;
  World->AddController(PC); PC->Possess(Cam); PC->PlayerInput=NewObject<UPlayerInput>(PC);
  M->Terrain=World->SpawnActor<ATCTerrain>(); FTCRows30Access::Attach(*M,Socket);
 }
 ~FRows30Scene() { if(World) { World->DestroyWorld(false); GEngine->DestroyWorldContext(World); } }
 bool Ready(FAutomationTestBase& T)
 { FString E; return T.TestNotNull(TEXT("isolated controller"),PC) && T.TestNotNull(TEXT("authority"),M) && T.TestNotNull(TEXT("terrain"),M->Terrain.Get()) && T.TestTrue(TEXT("pinned definitions"),M->UnitTable.Load(E)); }
 void Playing()
 {
  auto L=Lobby(2,TEXT("conquest"),true,true); M->State.ApplyLobby(*L); M->Controls.ApplyLobby(*L); M->Lobby.ApplyLobby(*L);
  auto S=Json(TEXT(R"({"t":"start","matchId":30,"you":0,"map":{"name":"default","w":128,"h":64,"rows":[]},"names":["Seat0","Seat1"],"teams":[0,1],"factions":[0,1]})"));
  TArray<TSharedPtr<FJsonValue>> Rows; for(int32 I=0;I<64;I++) Rows.Add(MakeShared<FJsonValueString>(FString::ChrN(128,'.')));
  S->GetObjectField(TEXT("map"))->SetArrayField(TEXT("rows"),Rows);
  M->State.ApplyStart(*S); M->State.Snapshots=1;
  M->Logistics.ApplyStart(); M->Logistics.ApplySnapshot(*Json(TEXT(R"({"logistics":{"enabled":true}})")),M->State);
  // A rectangular known raised map. Terrain ResetFrom builds the production relief, with no mesh upload/GPU.
  M->State.Map.W=128; M->State.Map.H=64; M->State.Map.Chars.Init('.',128*64); M->State.Map.Known.Init(1,128*64);
  M->State.Map.Levels.Init(2,128*64);
  M->Terrain->ResetFrom(M->State);
  FTCRows30Access::Pose(*PC,FVector2D(100,60),-90,60);
 }
 void Unit(int32 Id,const TCHAR* Type,int32 Owner=0,double X=100,double Z=60)
 { FTCUnit U; U.Id=Id; U.Type=Type; U.Owner=Owner; U.X=X; U.Z=Z; U.Hp=100; M->State.Units.Add(Id,U); }
 void Held(const FKey& K)
 {
  PC->PlayerInput->FlushPressedKeys();
  PC->PlayerInput->InputKey(FInputKeyEventArgs::CreateSimulated(K,IE_Pressed,1));
  PC->PlayerInput->ProcessInputStack({},.01f,false); PC->PlayerInput->ProcessInputStack({},.01f,false);
 }
 void Release() { PC->PlayerInput->FlushPressedKeys(); }
 TSharedPtr<FJsonObject> Last(FAutomationTestBase& T,const TCHAR* Type)
 { if(!T.TestTrue(TEXT("one command reaches transport"),Socket->Sent.Num()==1)) return nullptr; auto O=Socket->Sent[0]; T.TestEqual(TEXT("wire type"),O->GetStringField(TEXT("t")),FString(Type)); return O; }
};
}
#define TC_ROWS30_TEST(Class, Name) IMPLEMENT_SIMPLE_AUTOMATION_TEST(Class, "ThreeCrossroads.Rows30." Name, EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
#endif
