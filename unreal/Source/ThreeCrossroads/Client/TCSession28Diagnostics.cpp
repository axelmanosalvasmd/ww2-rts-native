#include "Client/TCSession28Diagnostics.h"
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Policies/CondensedJsonPrintPolicy.h"
#include "Serialization/JsonSerializer.h"

DEFINE_LOG_CATEGORY_STATIC(LogTCSession28, Log, All);

TSharedRef<FJsonObject> TCSession28::ViewSeamState(const ATCGameMode& Match, const ATCPlayerController& Controller,
	const FString& Phase, int32 IncomingMatchId, TOptional<bool> WriteSucceeded)
{
	// Read the held state directly. Do not initialize settings singletons, advance the script or run a player tick.
	// Full JSON numbers avoid the old view log's rounding and leave progression visible to the seam analyzer.
	const auto Out = MakeShared<FJsonObject>();
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
	Out->SetNumberField(TEXT("matchId"), Match.State.MatchId); Out->SetNumberField(TEXT("seat"), Match.State.You);
	Out->SetNumberField(TEXT("tick"), Match.State.Tick); Out->SetNumberField(TEXT("snapshots"), Match.State.Snapshots);
	Out->SetBoolField(TEXT("confirmed"), Match.State.SeatConfirmed()); Out->SetBoolField(TEXT("canOrder"), Match.State.CanOrder());
	Out->SetBoolField(TEXT("amHost"), Match.State.bAmHost);
	Out->SetBoolField(TEXT("roomInLobby"), Match.State.RoomState == TEXT("lobby"));
	Out->SetBoolField(TEXT("roomInPlay"), Match.State.RoomState == TEXT("play"));
	Out->SetBoolField(TEXT("viewSeatActive"), Match.Controls.bViewSeatActive); Out->SetBoolField(TEXT("resumePending"), Match.Controls.bResumePending);
	Out->SetStringField(TEXT("phase"), Phase);
	Out->SetNumberField(TEXT("incomingMatchId"), IncomingMatchId);
	Out->SetBoolField(TEXT("paused"), Match.State.bPaused);
	Out->SetBoolField(TEXT("hasMatch"), Match.State.bHasMatch);
	Out->SetBoolField(TEXT("viewStoreOpen"), Match.Controls.ViewMemory.IsOpen());
	Out->SetBoolField(TEXT("controlsHaveLobby"), Match.Controls.HasLobby());
	Out->SetBoolField(TEXT("lobbySettingsHaveLobby"), Match.Lobby.HasLobby());
	Out->SetBoolField(TEXT("matchOptionsHaveLobby"), Match.Options.HasLobby());
	if (Match.Options.HasLobby())
	{
		const FTCOptionControl Weather = Match.Options.Control(ETCMatchOption::Weather);
		Out->SetStringField(TEXT("weatherCurrent"), Weather.CurrentValue);
		Out->SetBoolField(TEXT("weatherEditable"), Weather.bEditable);
	}
	if (WriteSucceeded.IsSet()) Out->SetBoolField(TEXT("writeSucceeded"), *WriteSucceeded);
	if (Match.Controls.bResumePending)
	{
		if (const auto Pending = Match.Controls.ResumeView.Json()) Out->SetObjectField(TEXT("pendingView"), Pending);
	}
	return Out;
}

void TCSession28::LogViewSeam(const ATCGameMode& Match, const ATCPlayerController& Controller,
	const FString& Phase, int32 IncomingMatchId, TOptional<bool> WriteSucceeded)
{
	if (!FParse::Param(FCommandLine::Get(), TEXT("TCViewSeam28"))) return;
	FString Json;
	FJsonSerializer::Serialize(ViewSeamState(Match, Controller, Phase, IncomingMatchId, WriteSucceeded),
		TJsonWriterFactory<TCHAR, TCondensedJsonPrintPolicy<TCHAR>>::Create(&Json));
	UE_LOG(LogTCSession28, Log, TEXT("session28 view seam: %s"), *Json);
}
