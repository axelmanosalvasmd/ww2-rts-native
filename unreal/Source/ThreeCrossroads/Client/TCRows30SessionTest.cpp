#include "Client/TCRows30Harness.h"
#if WITH_DEV_AUTOMATION_TESTS
#include "HAL/FileManager.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
using namespace TCRows30;

TC_ROWS30_TEST(FTCRows30RefreshTest,"CLI01.RefreshClock");
bool FTCRows30RefreshTest::RunTest(const FString&)
{
 FTCRoomBrowser B;
 TestTrue(TEXT("first page load refreshes immediately"),B.RefreshDue(0));
 // Invalid URL exercises the real failed request path without network or elapsed wall time.
 TestFalse(TEXT("controlled request fails"),B.Request(TEXT("invalid-rows30"),10));
 TestFalse(TEXT("not before the original five seconds"),B.RefreshDue(14.999));
 TestTrue(TEXT("failed request retries at exactly five seconds"),B.RefreshDue(15));
 FTCRows30Access::Clock(B,10,true);
 TestFalse(TEXT("in-flight request suppresses duplicate refresh"),B.RefreshDue(18)); B.ApplyFailure();
 TestEqual(TEXT("timeout remains eight seconds"),FTCRoomBrowser::TimeoutSeconds,8.0);
 FRows30Scene S; if(!S.Ready(*this)) return false;
 FTCRows30Access::RefreshFrame(*S.M,false,false);
 TestEqual(TEXT("hidden directory does not request"),S.M->Rooms.Answers(),0);
 FTCRows30Access::RefreshFrame(*S.M,true,true);
 TestEqual(TEXT("navigation suppresses automatic request"),S.M->Rooms.Answers(),0);
 FTCRows30Access::RefreshFrame(*S.M,true,false);
 TestEqual(TEXT("visible directory attempts the due request"),S.M->Rooms.Answers(),1);
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30TokenTest,"CLI03.TokenRoundTrip");
bool FTCRows30TokenTest::RunTest(const FString&)
{
 const FString Root=FPaths::Combine(FPaths::ProjectSavedDir(),TEXT("Automation/Rows30Tokens"));
 IFileManager::Get().MakeDirectory(*Root,true);
 FTCTokenRequest R; R.Server=TEXT("ws://rows30.invalid/ws"); R.Room=TEXT("rows30"); R.Seat=TEXT("first");
 // Isolate the default room/seat filename in the configured test Saved directory, retaining the real identity hash.
 auto Path=[&]() { return FPaths::Combine(Root,FPaths::GetCleanFilename(FTCTokenStore::DefaultPath(R.Server,R.Room,R.Seat))); };
 R.TokenFile=Path(); IFileManager::Get().Delete(*R.TokenFile,false,true);
 FString First,Second,Other,Error;
 { FTCTokenStore Store; if(!TestTrue(TEXT("creates persistent identity"),Store.Resolve(R,First,Error))) return false;
   TestTrue(TEXT("generated token is accepted by reference"),FTCTokenStore::ValidToken(First));
   TestFalse(TEXT("source label contains no secret"),Store.Source().Contains(First)); TestFalse(TEXT("error contains no secret"),Error.Contains(First)); }
 { FTCTokenStore Store; TestTrue(TEXT("reopens same room and seat"),Store.Resolve(R,Second,Error)); TestTrue(TEXT("same identity reuses its secret"),Second==First); }
 R.Seat=TEXT("second"); R.TokenFile=Path(); IFileManager::Get().Delete(*R.TokenFile,false,true);
 { FTCTokenStore Store; TestTrue(TEXT("different seat creates its own identity"),Store.Resolve(R,Other,Error)); TestTrue(TEXT("different seat does not reuse secret"),Other!=First); TestFalse(TEXT("second label contains no secret"),Store.Source().Contains(Other)); }
 IFileManager::Get().DeleteDirectory(*Root,false,true);
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30StartTest,"CLI13.StartRefusals");
bool FTCRows30StartTest::RunTest(const FString&)
{
 struct FCase { int32 N; const TCHAR* Mode; bool OneSide; bool Allowed; const TCHAR* Line; };
 const FCase Cases[]={
 {0,TEXT("conquest"),false,false,TEXT("")},
 {5,TEXT("conquest"),false,false,TEXT("This map has 4 spawns: pick a bigger map or remove players.")},
 {5,TEXT("horde"),false,false,TEXT("Horde seats 4 defenders: remove a player.")},
 {2,TEXT("assault"),true,false,TEXT("Assault needs players on the defending team and on another team.")},
 {1,TEXT("conquest"),false,true,TEXT("Send the invite link, or add an AI opponent.")},
 {1,TEXT("horde"),false,true,TEXT("Send the invite link, or add an AI teammate.")},
 {2,TEXT("conquest"),true,true,TEXT("")},{4,TEXT("conquest"),false,true,TEXT("")},{2,TEXT("assault"),false,true,TEXT("")}};
 for(const auto& C:Cases) { auto O=Lobby(C.N,C.Mode); if(C.OneSide) for(auto P:O->GetArrayField(TEXT("players"))) P->AsObject()->SetNumberField(TEXT("team"),0);
  FTCLobbySettings L; if(!TestTrue(TEXT("reference lobby parses"),L.ApplyLobby(*O))) return false;
  TestEqual(TEXT("source start eligibility"),L.StartAllowed(),C.Allowed); TestEqual(TEXT("source refusal or invite line"),L.StartLine(),FString(C.Line)); }
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30AddAiTest,"CLI14.AddAiControls");
bool FTCRows30AddAiTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false;
 auto* Hud=S.World->SpawnActor<ATCHUD>(); FTCRows30Access::HudOwner(*Hud,S.PC);
 auto* Canvas=NewObject<UCanvas>(); Canvas->Init(0,0,nullptr,nullptr);
 struct FCase { bool Host,Play; int32 N; bool Shown; };
 for(const FCase C: {FCase{true,false,2,true},{false,false,2,false},{true,false,4,false},{true,true,2,false}})
 {
  auto L=Lobby(C.N,TEXT("conquest"),C.Host,C.Play); S.M->State.ApplyLobby(*L); S.M->Lobby.ApplyLobby(*L); S.M->Controls.ApplyLobby(*L);
  TestEqual(TEXT("Add AI visibility follows host, lobby and seat limit"),FTCRows30Access::AddAiShown(*Hud,Canvas),C.Shown);
  S.Socket->Sent.Reset(); const bool Sent=S.PC->RunUi(ETCUiAction::AddAi);
  TestEqual(TEXT("enabled press alone sends"),Sent,C.Shown); TestEqual(TEXT("one enabled press gives exactly one request"),S.Socket->Sent.Num(),C.Shown?1:0);
  if(C.Shown) TestEqual(TEXT("reference addAi payload"),TCOrders::ToJson(S.Socket->Sent[0]),FString(TEXT("{\"t\":\"addAi\"}")));
 }
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30SaveDateTest,"CLI19.StringSaveDate");
bool FTCRows30SaveDateTest::RunTest(const FString&)
{
 auto O=Json(TEXT(R"({"id":"rows30-save","kind":"manual","mode":"conquest","minutes":7,"names":["Seat0"],"savedAt":1767225600123})"));
 FTCSaveRow Numeric; auto Offset=[](int64) { return FTimespan::FromMinutes(-300); };
 if(!TestTrue(TEXT("numeric baseline"),FTCPersistence::ReadRow(MakeShared<FJsonValueObject>(O),Offset,Numeric))) return false;
 for(const TCHAR* ISO:{TEXT("2026-01-01T00:00:00.123Z"),TEXT("2025-12-31T19:00:00.123-05:00")})
 { O->SetStringField(TEXT("savedAt"),ISO); FTCSaveRow String;
  TestTrue(TEXT("ISO row accepted"),FTCPersistence::ReadRow(MakeShared<FJsonValueObject>(O),Offset,String));
  TestTrue(TEXT("new Date(string) retains instant"),String.bHasSavedAt && String.SavedAtMs==Numeric.SavedAtMs);
  TestEqual(TEXT("ISO and numeric display are identical in viewer zone"),String.Detail,Numeric.Detail); }
 O->SetStringField(TEXT("savedAt"),TEXT("not a date")); FTCSaveRow Invalid; FTCPersistence::ReadRow(MakeShared<FJsonValueObject>(O),Offset,Invalid);
 TestFalse(TEXT("invalid source string stays invalid"),Invalid.bHasSavedAt);
 return !HasAnyErrors();
}

TC_ROWS30_TEST(FTCRows30ReturnTest,"CLI30.ReturnToLobby");
bool FTCRows30ReturnTest::RunTest(const FString&)
{
 FRows30Scene S; if(!S.Ready(*this)) return false; S.Playing(); S.Unit(1,TEXT("rifle"));
 S.PC->Selected={1}; S.PC->Groups.Assign(1,S.M->State,S.M->UnitTable,S.PC->Selected);
 S.PC->RunUi(ETCUiAction::AttackMove);
 auto* Audio=S.M->Audio(); if(!TestNotNull(TEXT("production audio subsystem"),Audio)) return false;
 auto& Mixer=Audio->GetMixer(); Mixer.SetRunning(true); int32 Token=Mixer.BeginMatch(0,0); Mixer.FinishLoading(Token);
 TestTrue(TEXT("audio is match active"),Mixer.IsStarted());
 S.M->State.Winner=0; S.M->State.End.bPresent=true; S.M->State.End.Reason=TEXT("victory");
 S.M->State.Objectives.OnSnapshot(S.M->State);
 TestTrue(TEXT("epilogue was populated"),S.M->State.Objectives.Epilogue().bActive);
 FTCRows30Access::Receive(*S.M,*Lobby());
 TestFalse(TEXT("lobby return drops live match"),S.M->State.bHasMatch);
 TestTrue(TEXT("lobby is shown"),S.M->State.RoomState==TEXT("lobby") && S.M->Lobby.InLobby());
 TestTrue(TEXT("selection, groups and aim are empty together"),S.PC->Selected.IsEmpty() && S.PC->Groups.Members(1).IsEmpty() && S.PC->Aim==ETCAim::None);
 TestFalse(TEXT("same teardown stops match audio"),Mixer.IsStarted());
 TestFalse(TEXT("same teardown clears epilogue"),S.M->State.Objectives.Epilogue().bActive);
 return !HasAnyErrors();
}
#endif
