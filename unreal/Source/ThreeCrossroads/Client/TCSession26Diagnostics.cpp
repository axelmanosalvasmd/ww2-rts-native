#include "Client/TCSession26Diagnostics.h"
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCAudio.h"
#include "Client/TCGraphics.h"
#include "Client/TCLocale.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"

TSharedRef<FJsonObject> TCSession26::ConsumerState(const ATCGameMode& Match, const ATCPlayerController& Controller)
{
	const auto Out = MakeShared<FJsonObject>();
	// The existing preference dump covers storage. Read actual controller/model state here, without raw ini text,
	// names, tokens, server addresses or arbitrary stored JSON. UI clicks and effects need separate lane evidence.
	Out->SetBoolField(TEXT("edge"), Controller.EdgeScrollOn());
	Out->SetNumberField(TEXT("pan"), Controller.PanSpeedIndex());
	Out->SetStringField(TEXT("capture"), Match.Controls.MouseCapture.Mode());
	Out->SetNumberField(TEXT("uiScale"), Controller.UiScalePercent);
	Out->SetBoolField(TEXT("cover"), Match.Cover.IsEnabled());
	Out->SetBoolField(TEXT("controlsSeen"), Match.Onboarding.Seen());
	Out->SetBoolField(TEXT("onboarding"), Match.Onboarding.IsOnboarding());
	Out->SetBoolField(TEXT("controlsOpen"), Controller.bControlsSheetOpen);
	Out->SetStringField(TEXT("language"), FTCLocale::Code());
	Out->SetStringField(TEXT("graphics"), LexToString(FTCGraphics::Get().Level()));
	Out->SetBoolField(TEXT("graphicsAutoLatched"), FTCGraphics::Get().AutoDropDone());
	Out->SetBoolField(TEXT("gore"), FTCGraphics::Get().Gore());
	Out->SetBoolField(TEXT("statsLoaded"), Match.bStatsLoaded);
	if (Match.bStatsLoaded)
	{
		const FTCStatsSettings& Settings = Match.Stats.Settings();
		const auto Stats = MakeShared<FJsonObject>();
		Stats->SetBoolField(TEXT("show"), Settings.bShow); Stats->SetBoolField(TEXT("warn"), Settings.bWarn);
		Stats->SetStringField(TEXT("pos"), Settings.Pos); Stats->SetStringField(TEXT("layout"), Settings.Layout);
		Stats->SetStringField(TEXT("size"), Settings.Size); Stats->SetStringField(TEXT("bg"), Settings.Bg);
		TArray<TSharedPtr<FJsonValue>> Metrics;
		for (const FString& Metric : Settings.Stats) Metrics.Add(MakeShared<FJsonValueString>(Metric));
		Stats->SetArrayField(TEXT("stats"), Metrics); Out->SetObjectField(TEXT("stats"), Stats);
	}
	const auto Autocast = MakeShared<FJsonObject>();
	for (bool bClassic : {false, true})
	{
		const TOptional<bool> Preference = Match.Abilities.Autocast.Preference(TEXT("mg"), bClassic);
		if (Preference.IsSet()) Autocast->SetBoolField(bClassic ? TEXT("classicMg") : TEXT("freeMg"), *Preference);
	}
	Out->SetObjectField(TEXT("autocast"), Autocast);
	if (const UTCAudio* Audio = Match.Audio())
	{
		Out->SetNumberField(TEXT("volume"), Audio->Volume());
		const auto Bank = Audio->BankState();
		const auto Loaded = MakeShared<FJsonObject>();
		Loaded->SetStringField(TEXT("folder"), Bank.Folder); Loaded->SetStringField(TEXT("speaker"), Bank.Speaker);
		Loaded->SetNumberField(TEXT("effects"), Bank.Effects); Loaded->SetNumberField(TEXT("voiceLines"), Bank.VoiceLines);
		Loaded->SetBoolField(TEXT("callbackCompleted"), Bank.bCallbackCompleted); Loaded->SetBoolField(TEXT("fullyLoaded"), Bank.FullyLoaded());
		Out->SetObjectField(TEXT("audioBank"), Loaded);
	}
	Out->SetNumberField(TEXT("matchId"), Match.State.MatchId); Out->SetNumberField(TEXT("seat"), Match.State.You);
	Out->SetNumberField(TEXT("tick"), Match.State.Tick); Out->SetNumberField(TEXT("snapshots"), Match.State.Snapshots);
	Out->SetBoolField(TEXT("confirmed"), Match.State.SeatConfirmed()); Out->SetBoolField(TEXT("canOrder"), Match.State.CanOrder());
	Out->SetBoolField(TEXT("viewSeatActive"), Match.Controls.bViewSeatActive); Out->SetBoolField(TEXT("resumePending"), Match.Controls.bResumePending);
	const auto Camera = MakeShared<FJsonObject>();
	Camera->SetNumberField(TEXT("x"), Controller.ViewFocus().X); Camera->SetNumberField(TEXT("z"), Controller.ViewFocus().Y);
	Camera->SetNumberField(TEXT("yaw"), Controller.ViewYaw()); Camera->SetNumberField(TEXT("distance"), Controller.ViewDistance());
	Out->SetObjectField(TEXT("camera"), Camera);
	auto Ids = [](const TArray<int32>& Members)
	{
		TArray<TSharedPtr<FJsonValue>> Values;
		for (int32 Id : Members) Values.Add(MakeShared<FJsonValueNumber>(Id));
		return Values;
	};
	TArray<int32> Selected = Controller.Selected.Array(); Selected.Sort();
	Out->SetArrayField(TEXT("selected"), Ids(Selected));
	const auto Groups = MakeShared<FJsonObject>();
	for (int32 N = 1; N <= 9; ++N) Groups->SetArrayField(FString::FromInt(N), Ids(Controller.Groups.Members(N)));
	Out->SetObjectField(TEXT("groups"), Groups);
	return Out;
}
