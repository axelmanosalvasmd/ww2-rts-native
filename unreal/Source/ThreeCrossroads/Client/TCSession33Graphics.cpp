#include "Client/TCSession33Graphics.h"
#include "Client/TCGraphics.h"
#include "Client/TCPlayerController.h"
#include "Client/TCLocale.h"
#include "Dom/JsonObject.h"
#include "Engine/World.h"
#include "HAL/IConsoleManager.h"
#include "Misc/App.h"
#include "Misc/ConfigCacheIni.h"
#include "Misc/Paths.h"

void TCSession33Graphics::Append(const ATCPlayerController& PC, FJsonObject& Out)
{
	const FTCGraphics& G = FTCGraphics::Get();
	const auto State = MakeShared<FJsonObject>();
	State->SetNumberField(TEXT("frameDt"), FApp::GetDeltaTime());
	State->SetNumberField(TEXT("clampedDt"), FTCGraphics::ClampFrame(FApp::GetDeltaTime()));
	State->SetNumberField(TEXT("slowSeconds"), G.SlowSeconds());
	State->SetNumberField(TEXT("changes"), G.Changes());
	State->SetNumberField(TEXT("scheduledLeft"), G.ScheduledLeft());
	const FConfigFile* Settings = GConfig ? GConfig->FindConfigFile(GGameUserSettingsIni) : nullptr;
	const FString SettingsPath = Settings && Settings->Branch ? Settings->Branch->IniPath : GGameUserSettingsIni;
	State->SetStringField(TEXT("settingsPlatform"), FPaths::GetCleanFilename(FPaths::GetPath(SettingsPath)));
	State->SetStringField(TEXT("settingsFilename"), FPaths::GetCleanFilename(SettingsPath));
	const double Age = PC.GetWorld() && PC.GraphicsNoticeAt >= 0 ? PC.GetWorld()->GetRealTimeSeconds() - PC.GraphicsNoticeAt : -1;
	State->SetNumberField(TEXT("noticeAge"), Age);
	State->SetBoolField(TEXT("noticeActive"), !PC.GraphicsNotice.IsEmpty() && Age >= 0 && Age < FTCGraphics::NoticeSeconds);
	State->SetStringField(TEXT("noticeEnglish"), PC.GraphicsNotice);
	State->SetStringField(TEXT("noticeLocalized"), TC_T(PC.GraphicsNotice));
	for (const TCHAR* Name : {TEXT("r.ScreenPercentage"), TEXT("r.Shadow.MaxResolution"), TEXT("r.Shadow.MaxCSMResolution"), TEXT("r.ShadowQuality")})
		if (auto* V = IConsoleManager::Get().FindConsoleVariable(Name)) State->SetStringField(Name, V->GetString());
	Out.SetObjectField(TEXT("graphics33"), State);
}
