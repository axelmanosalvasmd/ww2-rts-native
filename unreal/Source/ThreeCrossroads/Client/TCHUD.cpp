#include "Client/TCHUD.h"
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
#include "Client/TCUnitRenderer.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCPersistence.h"
#include "Client/TCLocale.h"
#include "Client/TCAlerts.h"
#include "ThreeCrossroads.h"
#include "World/TCTerrain.h"
#include "World/TCWorldObjects.h"
#include "Net/TCConnection.h"
#include "Engine/Canvas.h"
#include "Engine/Engine.h"
#include "Engine/Font.h"

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
		case ETCUiAction::LobbySetting: case ETCUiAction::MatchOption: return A.Unit == B.Unit && A.Group == B.Group;
		case ETCUiAction::LoadSave: case ETCUiAction::Research: return A.Unit == B.Unit;
		default: return true;
		}
	}
}

void ATCHUD::EndPlay(const EEndPlayReason::Type Reason)
{
	Minimap.Reset(); // releases the texture with the HUD
	Super::EndPlay(Reason);
}

void ATCHUD::DrawLabel(const FString& Str, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale)
{
	const FString Shown = TC_T(Str);
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
	AHUD::DrawText(Shown, Colour, X, Y, Font, Scale);
}

void ATCHUD::LabelSize(const FString& Str, float& OutWidth, float& OutHeight, UFont* Font, float Scale) const
{
	AHUD::GetTextSize(TC_T(Str), OutWidth, OutHeight, Font, Scale);
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
		FVector2D Px;
		if (!M->AlertFeed.Pings().LookOf(R, Now, L) || !Minimap.WorldToPixel(FVector2D(R.X, R.Z), Px)) continue;
		constexpr int32 Segs = 24;
		for (int32 Pass = 0; Pass < 2; Pass++)
			for (int32 K = 0; K < Segs; K++)
			{
				const double A0 = 2 * PI * K / Segs, A1 = 2 * PI * (K + 1) / Segs;
				const FVector2D B0 = Px + FVector2D(FMath::Cos(A0), FMath::Sin(A0)) * L.MinimapPx, B1 = Px + FVector2D(FMath::Cos(A1), FMath::Sin(A1)) * L.MinimapPx;
				FLinearColor C = Pass == 0 ? Ink : Chalk;
				C.A = L.MinimapAlpha;
				DrawLine(B0.X, B0.Y, B1.X, B1.Y, C, Pass == 0 ? 4.f : 2.f);
			}
	}
}

float ATCHUD::Button(UFont* Font, float X, float Y, const FString& Label, ETCUiAction Action)
{
	return Button(Font, X, Y, Label, FTCUiCommand(Action), true, ButtonFill);
}

float ATCHUD::Button(UFont* Font, float X, float Y, const FString& Label, const FTCUiCommand& Command, bool bEnabled, const FLinearColor& Fill, float MinWidth)
{
	float TW = 0, TH = 0;
	LabelSize(Label, TW, TH, Font, 1.f);
	const FBox2D Rect(FVector2D(X, Y), FVector2D(X + FMath::Max(TW + 2 * Pad, MinWidth), Y + TH + Pad));
	DrawRect(Fill, Rect.Min.X, Rect.Min.Y, Rect.GetSize().X, Rect.GetSize().Y);
	DrawLabel(Label, bEnabled ? Text : Dim, X + Pad, Y + Pad / 2, Font, 1.f);
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
	for (const FButton& B : Buttons) if (B.Rect.IsInsideOrOn(Pixel)) { OutCommand = B.Command; return true; }
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

float ATCHUD::DrawLobby(UFont* Font, float Y)
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
	struct FLine { FString Text; FLinearColor Colour; TArray<FItem> Items; };
	TArray<FLine> Lines;
	const float W = 700.f;
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
		FString Label = FString::Printf(TEXT("%s: %s"), *C.Title, C.CurrentLabel.IsEmpty() ? TEXT("?") : *C.CurrentLabel);
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
	if (bLobby) AddButtons({ FItem{ TCTutorialEntry::Label(), FTCUiCommand(ETCUiAction::Tutorial), true, TCTutorialEntry::Title() } });
	if (L.HasLobby() && bLobby)
	{
		const FString Info = FTCLobbySettings::ModeInfo(R.Mode);
		if (!Info.IsEmpty()) Lines.Add({ Info, Dim, {} });
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
		else Lines.Add({ FString::Printf(TEXT("%sTeam %d%s"), *Seat, P.Team, P.bAi ? *FString::Printf(TEXT("  %s"), *P.Level) : TEXT("")), bYou ? Good : Text, {} });
	}
	if (S.Spectators.Num()) Lines.Add({ TEXT("Watching: ") + FString::Join(S.Spectators, TEXT(", ")), Dim, {} });
	if (!S.LastResult.IsEmpty()) Lines.Add({ FString::Printf(TEXT("Last match: %s."), *S.LastResult), Dim, {} });
	const bool bHost = S.bAmHost && !S.bSpectator;
	if (S.RoomState == TEXT("play")) Lines.Add({ TEXT("A match is running: joining it..."), Dim, {} });
	else if (S.bSpectator) Lines.Add({ TEXT("You are watching this room."), Dim, {} });
	else if (!bHost) Lines.Add({ TEXT("Waiting for the host to start."), Dim, {} });
	if (S.bAmHost && bLobby)
	{
		// the Start button's text and enabled state are the browser's (renderLobby); Add AI as before
		TArray<FItem> Host;
		if (bHost) Host.Add({ TEXT("+ Add AI player"), FTCUiCommand(ETCUiAction::AddAi), true, TEXT("+ Add AI player") }); // lobby-view.js
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
	for (const FLine& Ln : Lines)
	{
		if (!Ln.Items.Num()) { DrawLabel(Ln.Text, Ln.Colour, 14.f, RowY, Font, 1.f); RowY += Line; continue; }
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

float ATCHUD::DrawMatchPanel(UFont* Font)
{
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	const FTCMatchState& S = M->State;
	TArray<TPair<FString, FLinearColor>> Rows;
	// The selection: one unit in detail, several as counts by type with their summed health
	if (PC->Selected.Num() == 1)
	{
		const int32 Id = *PC->Selected.CreateConstIterator();
		if (const FTCUnit* U = S.Units.Find(Id))
		{
			const FTCUnitDef* D = M->UnitTable.Find(U->Type);
			const int32 Max = D ? D->MaxHp() : 0;
			// hud.js: the name with '#id' (tr(name) + ' #' + id), the health as '{0}/{1} hp', then the status tags
			// hud.js name(type, owner): the owner's faction name (client/main.js FACTIONS), else the unit table's
			TArray<FString> Row = { FString::Printf(TEXT("%s #%d"), *FTCAlerts::UnitName(S, &M->UnitTable, U->Type, U->Owner), U->Id), Max > 0 ? FString::Printf(TEXT("%d/%d hp"), U->Hp, Max) : FString::Printf(TEXT("%d hp"), U->Hp) };
			Row.Append(UnitTags(*U, D));
			Rows.Add({ FString::Join(Row, TEXT("  |  ")), Text });
			// client/hud.js tags and the work bar: a squad digging or waiting for its turn on a pattern
			const FTCDigStatus Dig = FTCFieldworks::DigStatus(*U);
			if (!Dig.Tag.IsEmpty()) Rows.Add({ Dig.bDigging ? FString::Printf(TEXT("%s %.0f%%"), *Dig.Tag, Dig.Progress * 100.f) : Dig.Tag, Good });
		}
	}
	else if (PC->Selected.Num() > 1)
	{
		TMap<FString, int32> Counts;
		int32 Hp = 0, Max = 0;
		for (int32 Id : PC->Selected)
		{
			const FTCUnit* U = S.Units.Find(Id);
			if (!U) continue;
			const FTCUnitDef* D = M->UnitTable.Find(U->Type);
			Counts.FindOrAdd(FTCAlerts::UnitName(S, &M->UnitTable, U->Type, U->Owner))++; // hud.js name(r.type, v.owner)
			Hp += U->Hp; Max += D ? D->MaxHp() : 0;
		}
		Counts.ValueSort([](int32 A, int32 B) { return A > B; });
		// hud.js drawSelection: '{0} units selected' over one row per type, the count and the name apart
		TArray<FString> Parts;
		for (const auto& C : Counts) Parts.Add(FString::Printf(TEXT("%d  %s"), C.Value, *C.Key));
		Rows.Add({ PC->Selected.Num() == 1 ? FString(TEXT("1 unit selected")) : FString::Printf(TEXT("%d units selected"), PC->Selected.Num()), Text });
		Rows.Add({ FString::Join(Parts, TEXT("  |  ")), Text });
		Rows.Add({ FString::Printf(TEXT("%d/%d hp"), Hp, Max), Dim });
	}
	else if (!S.bSpectator) Rows.Add({ TEXT("Nothing selected: left click or drag over your units"), Dim });
	if (PC->HoverEnemy)
		if (const FTCUnit* E = S.Units.Find(PC->HoverEnemy))
		{
			const FTCUnitDef* D = M->UnitTable.Find(E->Type);
			Rows.Add({ FString::Printf(TEXT("Enemy %s of %s  hp %d%s%s"), D ? *D->Name : *E->Type, *SeatName(S, E->Owner), E->Hp, D && D->MaxHp() ? *FString::Printf(TEXT(" / %d"), D->MaxHp()) : TEXT(""),
				PC->Selected.Num() ? TEXT("  |  right click: attack") : TEXT("")), Foe });
		}
	// the armed aim's hint (client/main.js setAim) and, for a building, what the spot under the pointer would get
	if (PC->Aim != ETCAim::None)
	{
		Rows.Add({ PC->AimHint, Warn });
		if (PC->Aim == ETCAim::Field && PC->FieldPreview.bShown && PC->FieldPreview.bMarker && PC->FieldPreview.bMarkerPlaced && !PC->FieldPreview.bThrows)
		{
			const FTCFortPlacement& P = PC->FieldPreview.Placement;
			Rows.Add({ P.bOk ? FString::Printf(TEXT("Here: can dig at (%.1f, %.1f), %d cells"), P.At.X, P.At.Y, P.Cells.Num())
				: FString::Printf(TEXT("Here: %s"), P.bNative ? *P.NativeReason : *TCOrders::DenySentence(P.Reason, FString())), P.bOk ? Good : Warn });
		}
		if (PC->Aim == ETCAim::Build && PC->bPointerGround)
		{
			const FTCPlacement P = M->Build.Placement(S, PC->AimBuild, PC->PointerGround);
			if (!P.bThrows) Rows.Add({ P.bOk ? FString::Printf(TEXT("Here: can build at (%.1f, %.1f)"), P.At.X, P.At.Y)
				: FString::Printf(TEXT("Here: %s"), P.bNative ? *P.NativeReason : *TCOrders::DenySentence(P.Reason, FString())), P.bOk ? Good : Warn });
		}
	}
	// hud.js drawLogistics: each selected unit's supply lines (own reserves, own trucks, a friendly store's stock)
	if (PC->Selected.Num())
	{
		const FTCLogisticsCard Card = M->Logistics.Card(S, PC->Selected.Array());
		for (int32 I = 0; I < Card.Entries.Num() && I < 4; I++)
		{
			const FTCLogisticsEntry& E = Card.Entries[I];
			const FTCUnit* EU = S.Units.Find(E.Id);
			// hud.js drawLogistics: tr(name(v.type, v.owner)) + ' #' + id
			Rows.Add({ FString::Printf(TEXT("%s #%d: %s"), *FTCAlerts::UnitName(S, &M->UnitTable, E.Type, EU ? EU->Owner : S.You), E.Id, *FString::Join(E.Lines, TEXT("  |  "))), Good });
		}
		if (Card.Entries.Num() > 4) Rows.Add({ FString::Printf(TEXT("+%d more with supply lines"), Card.Entries.Num() - 4), Dim });
		if (Card.bVisible && Card.bTrucks) Rows.Add({ FTCLogisticsCard::Hint, Dim });
	}
	// the control groups that hold units (client/selection.js groups; the browser shows them as badges)
	if (S.SeatConfirmed())
	{
		TArray<FString> Parts;
		for (int32 G = 1; G <= FTCControlGroups::GroupCount; G++)
			if (const int32 N = PC->Groups.Members(G).Num()) Parts.Add(FString::Printf(TEXT("%d: %d"), G, N));
		// the browser's words: hud.js's 'Control group' label, and the Controls sheet's line for Ctrl+1-9 (index.html; the
		// key written as es.js's 'Fullscreen: also lets Ctrl+1-9 set groups' writes it)
		Rows.Add({ Parts.Num() ? TEXT("Control group  ") + FString::Join(Parts, TEXT("   ")) : FString(TEXT("Ctrl+1-9: Set a control group; Shift adds to it")), Dim });
	}

	// the Formation menu's settings while the Orders panel shows (client/hud.js drawOrders: the Form button's submenu)
	if (!S.bSpectator && TCArmyCommands::ShowsOrdersPanel(S, M->UnitTable, PC->Selected))
	{
		const FTCFormationSettings& F = PC->Formation.Settings;
		// the Form submenu's own words (hud.js: 'Formation: {0}', 'Formation spacing {0}x', the Together and Snap switches),
		// each its own column so each is translated by itself
		Rows.Add({ FString::Printf(TEXT("Formation: %s  |  Formation spacing %.2fx  |  Together: %s  |  Snap: %s"), LexToString(F.Shape), F.Spread,
			F.bTogether ? TEXT("On") : TEXT("Off"), F.bSnap ? TEXT("On") : TEXT("Off")), Dim });
	}

	// The buttons (client/hud.js drawOrders), in rows that wrap at the panel's width: Stop and Attack-move always; for a
	// selection that is neither all buildings nor all trucks, Retreat, the three stances and (with a builder squad)
	// the Build menu; Resume deliveries for an all-truck selection; the logistics switches; End match for the host.
	// Slice 1f: the support bar (client/hud.js buildSupport and drawEcon) in its own row first, the Shell area button
	// with a salvo unit selected, the Form menu and the cover preview switch.
	struct FItem { FString Label; FTCUiCommand Command; bool bEnabled; FLinearColor Fill; FString Hover; };
	TArray<FItem> Bar, Orders, Menu;
	const float W = 560.f;
	if (!S.bSpectator && S.SeatConfirmed())
		for (const FTCSupportButton& B : M->Powers.Buttons(S, M->UnitTable, M->Recruit))
		{
			const FTCSupportDef* D = FTCSupportPowers::Find(B.Kind);
			FTCUiCommand C(ETCUiAction::Support);
			C.Unit = B.Kind;
			const bool bAimed = PC->Aim == ETCAim::Support && PC->PowerAim.Kind == ETCSupportAimKind::Support && PC->PowerAim.SupportKind == B.Kind;
			// the browser never disables a support button: a click on a refused one explains why (aimSupport)
			Bar.Add({ FString::Printf(TEXT("%s [%s] %s"), D ? *D->Name : *B.Kind, *B.Key, *B.Text), C, B.Check.bOk, bAimed ? TabOpen : B.Check.bOk ? ButtonFill : CardOff,
				B.Check.bOk ? B.Tip : FString::Printf(TEXT("%s: %s%s"), D ? *D->Name : *B.Kind, *B.Check.Reason, B.Check.bNative ? TEXT(" (this client's own check)") : TEXT("")) });
		}
	if (!S.bSpectator)
	{
		Orders.Add({ TEXT("Stop (X)"), FTCUiCommand(ETCUiAction::Stop), true, ButtonFill, TEXT("Stop (X): halt where they are") });
		Orders.Add({ TEXT("Attack-move (G)"), FTCUiCommand(ETCUiAction::AttackMove), true, PC->Aim == ETCAim::AttackMove ? TabOpen : ButtonFill, TEXT("Attack-move (G, or Ctrl+right-click): move and fight anything met on the way") });
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
			Orders.Add({ FString::Printf(TEXT("Logistics overlay: %s"), M->Logistics.OverlaySwitch() ? TEXT("On") : TEXT("Off")), FTCUiCommand(ETCUiAction::SupplyOverlay), true, M->Logistics.OverlaySwitch() ? TabOpen : ButtonFill,
				TEXT("Supply overlay: known stores and the selected trucks' routes") });
			Orders.Add({ FString::Printf(TEXT("Logistics selection: %s"), M->Logistics.SelectionSwitch() ? TEXT("On") : TEXT("Off")), FTCUiCommand(ETCUiAction::TruckSelect), true, M->Logistics.SelectionSwitch() ? TabOpen : ButtonFill,
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
		// client/cover-preview.js: the game menu's switch (this client has no game menu: it sits with the Orders)
		Orders.Add({ FTCCoverPreview::MenuLabel(M->Cover.IsEnabled()), FTCUiCommand(ETCUiAction::CoverToggle), true, M->Cover.IsEnabled() ? TabOpen : ButtonFill, FTCCoverPreview::MenuTip() });
		// client/camera.js preferences, the game menu's Edge scroll and Pan speed (saved)
		Orders.Add({ PC->EdgeScrollLabel(), FTCUiCommand(ETCUiAction::EdgeScroll), true, PC->EdgeScrollOn() ? TabOpen : ButtonFill, TEXT("Edge scroll: the view pans when the pointer is at the window's edge") });
		Orders.Add({ PC->PanSpeedLabel(), FTCUiCommand(ETCUiAction::PanSpeed), true, ButtonFill, TEXT("Pan speed: Slow, Normal or Fast for the keys and the edges") });
	}
	// client/index.html's menu Volume (title "Effects, voices and alerts. M mutes") and the page's language picker: this
	// client has no game menu, so they sit with the Orders for every match, selection or not
	Orders.Add({ PC->VolumeLabel(), FTCUiCommand(ETCUiAction::Volume), true, ButtonFill, TEXT("Effects, voices and alerts. M mutes (right click: quieter)") });
	Orders.Add({ PC->LanguageLabel(), FTCUiCommand(ETCUiAction::Language), true, ButtonFill, TEXT("Language") });
	// client/light.js: the menu's Graphics and Gore buttons; client/index.html: the UI scale slider (Client/TCGraphics.h)
	Orders.Add({ FTCGraphics::MenuLabel(FTCGraphics::Get().Level()), FTCUiCommand(ETCUiAction::Graphics), true, FTCGraphics::Get().IsLow() ? TabOpen : ButtonFill, FTCGraphics::MenuTip() });
	Orders.Add({ FTCGraphics::GoreLabel(FTCGraphics::Get().Gore()), FTCUiCommand(ETCUiAction::Gore), true, ButtonFill, FTCGraphics::GoreTip() });
	Orders.Add({ PC->UiScaleLabel(), FTCUiCommand(ETCUiAction::UiScale), true, ButtonFill, FTCSettingsStore::UIScaleTip() });
	// client/main.js renderMatchMenu: the host's Pause (Resume) match, Save match and End match (this client has no
	// match menu: they sit with the Orders, and a watching host gets them too, as the browser's menu shows them)
	if (S.bAmHost && S.RoomState == TEXT("play"))
	{
		Orders.Add({ S.bPaused ? TEXT("Resume match") : TEXT("Pause match"), FTCUiCommand(ETCUiAction::PauseMatch), true, S.bPaused ? TabOpen : ButtonFill, S.bPaused ? TEXT("Resume the match for everyone") : TEXT("Pause the match for everyone") });
		const FTCSaveButton Save = M->Persistence.SaveButton(S);
		if (Save.bShown) Orders.Add({ Save.Label, FTCUiCommand(ETCUiAction::SaveMatch), true, ButtonFill, Save.Tip });
		Orders.Add({ TEXT("End match (back to lobby)"), FTCUiCommand(ETCUiAction::End), true, ButtonFill, TEXT("End match (back to lobby)") }); // index.html #endBtn
	}
	auto Measure = [&](const FString& L) { float TW = 0, TH = 0; LabelSize(L, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	TArray<TArray<FItem>> ButtonRows;
	auto Wrap = [&](const TArray<FItem>& Items)
	{
		TArray<FItem> Row; float X = 0;
		for (const FItem& I : Items)
		{
			const float IW = Measure(I.Label);
			if (Row.Num() && X + IW > W - 2 * Pad) { ButtonRows.Add(Row); Row.Reset(); X = 0; }
			Row.Add(I); X += IW + Pad;
		}
		if (Row.Num()) ButtonRows.Add(Row);
	};
	Wrap(Bar);
	Wrap(Orders);
	Wrap(Menu);
	const float RowH = Line + Pad + 4.f;
	FVector2D Mouse(-1, -1);
	float MX, MY;
	const bool bMouse = PC->GetMousePosition(MX, MY);
	if (bMouse) Mouse = FVector2D(MX, MY) / UiNow; // in the scaled panels' own pixels
	const float H = Rows.Num() * Line + ButtonRows.Num() * RowH + (ButtonRows.Num() ? Line : 0.f) + 2 * Pad, Y0 = Canvas->ClipY - H - Gap;
	AddPanel(8.f, Y0, W, H, Panel);
	float Y = Y0 + Pad;
	for (const auto& R : Rows) { DrawLabel(R.Key, R.Value, 14.f, Y, Font, 1.f); Y += Line; }
	FString Hover;
	for (const TArray<FItem>& Row : ButtonRows)
	{
		float X = 14.f;
		for (const FItem& I : Row)
		{
			const float Left = X;
			X = Button(Font, X, Y + 2.f, I.Label, I.Command, I.bEnabled, I.Fill);
			if (bMouse && FBox2D(FVector2D(Left, Y + 2.f), FVector2D(X - Pad, Y + 2.f + Line + Pad)).IsInsideOrOn(Mouse)) Hover = I.Hover;
		}
		Y += RowH;
	}
	if (!Hover.IsEmpty()) DrawLabel(Hover, Good, 14.f, Y, Font, 1.f);
	return Y0;
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
	if (Card.Kind == ETCCardKind::Hidden && !Research.bShown && !Restore.bShown) return;
	// client/hud.js drawClassicCard: an own Engineer selected (no single own building) gets the Build card; an all-truck
	// selection gets the logistics card, drawn with the selection in the match panel
	const FTCBuildCard BuildCard = Card.Kind == ETCCardKind::NotPorted ? M->Build.BuildCard(S, PC->Selected) : FTCBuildCard();
	if (Card.Kind == ETCCardKind::NotPorted && !BuildCard.bShown && FTCLogistics::ResumeShown(S, PC->Selected)) return;
	const float Right = Left + FMath::Max(300.f, FMath::Min(720.f, (Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Min.X) - 12.f : Canvas->ClipX - Gap) - Left));
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
	auto Wrap = [&](const TArray<FItem>& Items)
	{
		TArray<FItem> Row; float X = 0;
		for (const FItem& I : Items)
		{
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
		Lines.Add({ TEXT("Recruit  |  ") + Wallet, Text });
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
		if (Card.Kind != ETCCardKind::Hidden) Lines.Add({ FString::Printf(TEXT("Command card not built in this client yet: %s"), *Card.Why), Dim });
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

	// the hover line under the cards: the card's price and the browser's availability sentence
	FVector2D Mouse(-1, -1);
	float MX, MY;
	const bool bMouse = PC->GetMousePosition(MX, MY);
	if (bMouse) Mouse = FVector2D(MX, MY) / UiNow; // in the scaled panels' own pixels
	const float RowH = Line + Pad + 4.f;
	const float H = 2 * Pad + Lines.Num() * Line + Rows.Num() * RowH + Line;
	const float Top = Bottom - H;
	AddPanel(Left, Top, Width, H, Panel);
	float Y = Top + Pad;
	for (const auto& L : Lines) { DrawLabel(L.Key, L.Value, Left + Pad, Y, Font, 1.f); Y += Line; }
	FString Hover;
	for (const TArray<FItem>& Row : Rows)
	{
		float X = Left + Pad;
		for (const FItem& I : Row)
		{
			const FBox2D Rect(FVector2D(X, Y + 2.f), FVector2D(X + I.W, Y + 2.f + Line + Pad));
			if (bMouse && Rect.IsInsideOrOn(Mouse)) Hover = I.Hover;
			if (I.Command.Action == ETCUiAction::None)
			{
				// a queue place that is not a button (training, or waiting without a job to cancel)
				DrawRect(I.Fill, Rect.Min.X, Rect.Min.Y, Rect.GetSize().X, Rect.GetSize().Y);
				DrawLabel(I.Label, Dim, X + Pad, Y + 2.f + Pad / 2, Font, 1.f);
				X = Rect.Max.X + Pad;
			}
			else X = Button(Font, X, Y + 2.f, I.Label, I.Command, I.bEnabled, I.Fill, I.W);
		}
		Y += RowH;
	}
	if (!Hover.IsEmpty()) DrawLabel(Hover, Good, Left + Pad, Y, Font, 1.f);
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
	const FLinearColor RallyColour(FColor(0x9d, 0xd0, 0xff));
	auto Label = [&](FVector2D At, const FString& L, const FLinearColor& C)
	{
		const FVector Sc = Project(ATCTerrain::ToWorld(At.X, At.Y, T.HeightAt(At.X, At.Y) + 1.f));
		FLinearColor Lc = C; Lc.A *= M->WarMap.LabelFade(); // round 14: markers.js fadeLabels while the paper war map comes
		if (Sc.Z > 0) DrawLabel(L, Lc, Sc.X + 6.f, Sc.Y - 6.f, Font, 1.f);
	};
	// client/world-regions.js and markers.js, World: each region's border on the ground in its team's colour, its claim
	// ring with the capture arc, and the label. Only regions whose claim point projects within three viewports of the
	// screen are drawn (a cost limit of this client; the browser draws every border it holds)
	if (M->WorldRec.IsWorld())
	{
		const float VW = Canvas->ClipX, VH = Canvas->ClipY;
		for (const FTCWorldRegionView& V : M->WorldRec.RegionViews())
		{
			const FVector C = Project(ATCTerrain::ToWorld(V.At.X, V.At.Y, T.HeightAt(V.At.X, V.At.Y)));
			if (C.Z <= 0 || C.X < -VW || C.X > 2 * VW || C.Y < -VH || C.Y > 2 * VH) continue;
			FLinearColor Border(V.TeamColour);
			Border.A = FTCWorldRecovery::BorderOpacity;
			for (const TPair<FVector2D, FVector2D>& Sg : V.Segments)
			{
				const FVector A = Project(ATCTerrain::ToWorld(Sg.Key.X, Sg.Key.Y, T.HeightAt(Sg.Key.X, Sg.Key.Y) + FTCWorldRecovery::BorderLift));
				const FVector B = Project(ATCTerrain::ToWorld(Sg.Value.X, Sg.Value.Y, T.HeightAt(Sg.Value.X, Sg.Value.Y) + FTCWorldRecovery::BorderLift));
				if (A.Z > 0 && B.Z > 0) DrawLine(A.X, A.Y, B.X, B.Y, Border, 1.5f);
			}
			if (V.Loop.Num()) GroundLine(T, V.Loop, true, Border, 1.5f, FTCWorldRecovery::BorderLift);
			GroundRing(T, V.At, TCWorldRecoveryCore::PointRadius, FLinearColor(V.bOwner ? V.TeamColour : FTCWorldRecovery::NeutralTeamColour), V.bContested ? 3.f : 1.5f);
			if (V.Arc.bVisible)
			{
				// markers.js: the capture arc, Segments of PROG_SEGS around the ring, in the owner's or the capper's colour
				TArray<FVector2D> Arc;
				for (int32 K = 0; K <= V.Arc.Segments; K++)
				{
					const double A = 2 * PI * K / TCWorldRecoveryCore::ProgressSegments;
					Arc.Add(V.At + FVector2D(FMath::Cos(A), FMath::Sin(A)) * (TCWorldRecoveryCore::PointRadius + 0.6));
				}
				GroundLine(T, Arc, false, FLinearColor(V.ArcColour), 3.f);
			}
			const FVector L = Project(ATCTerrain::ToWorld(V.At.X, V.At.Y, T.HeightAt(V.At.X, V.At.Y) + FTCWorldRecovery::LabelHeight));
			if (L.Z > 0) DrawLabel(V.Text, FLinearColor(V.TeamColour), L.X - 40.f, L.Y, Font, 1.f);
		}
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
	// canvas has no depth test, so the through-models layer is drawn the same way, at its own opacity); outlines only
	{
		const FTCCoverView& V = M->Cover.View();
		auto Layer = [&](const TArray<FTCCoverGlyph>& Glyphs, float Opacity)
		{
			const TArrayView<const FVector2D> Shield = FTCCoverPreview::ShieldOutline(), Chevron = FTCCoverPreview::ChevronLine();
			for (const FTCCoverGlyph& G : Glyphs)
			{
				const TArrayView<const FVector2D> Shape = G.bChevron ? Chevron : Shield;
				TArray<FVector2D> Pts;
				Pts.Reserve(Shape.Num());
				for (const FVector2D& Q : Shape) Pts.Add(G.Place(Q.X, Q.Y));
				FLinearColor C(G.Color);
				C.A = G.Alpha * Opacity;
				GroundLine(T, Pts, !G.bChevron, C, 1.5f, (float)FTCCoverPreview::LiftMetres);
			}
		};
		if (V.bVisible)
		{
			if (V.bCursorGround) Layer(M->Cover.CursorMarks().Ground, V.CursorGroundOpacity);
			if (V.bCursorThrough) Layer(M->Cover.CursorMarks().Through, V.CursorThroughOpacity);
			if (V.bFlashGround) Layer(M->Cover.FlashMarks().Ground, V.FlashGroundOpacity);
			if (V.bFlashThrough) Layer(M->Cover.FlashMarks().Through, V.FlashThroughOpacity);
		}
	}
	// client/main.js aimShape for an aimed ability: a ring of its radius under the pointer
	if (PC->Aim == ETCAim::Ability && PC->bPointerGround && PC->AimAbilityRadius > 0)
		GroundRing(T, PC->PointerGround, (float)PC->AimAbilityRadius, FLinearColor(FColor(0xff, 0xa0, 0x30)), 2.f);
	// rally points: the army's (shared/sim.js snapshotFor rally) and the selected own buildings' (their queue's rally)
	if (M->bArmyRally) { GroundRing(T, M->ArmyRally, 1.5f, RallyColour, 2.f); Label(M->ArmyRally, TEXT("Army rally"), RallyColour); }
	for (int32 Id : PC->Selected)
	{
		const FTCUnit* U = S.Units.Find(Id);
		const FTCProductionQueue* Q = U ? M->Recruit.QueueOf(Id) : nullptr;
		if (!Q || Q->Rally.X < 0) continue;
		GroundLine(T, { FVector2D(U->X, U->Z), Q->Rally }, false, RallyColour * FLinearColor(1, 1, 1, 0.6f), 1.5f);
		GroundRing(T, Q->Rally, 1.2f, RallyColour, 2.f);
	}
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
	DrawMinimapPoints(); // slice 9: the capture points and the start's spawns
	DrawMinimapPings();  // slice 10: the relayed pings' rings (client/pings.js drawMinimap)
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
	if (!M->Logistics.OverlayActive()) return;
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

void ATCHUD::DrawHUD()
{
	Super::DrawHUD();
	Buttons.Reset();
	Panels.Reset();
	EndUi(); // a frame starts unscaled
	Frames++;
	ATCGameMode* M = GetWorld()->GetAuthGameMode<ATCGameMode>();
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetOwningPlayerController());
	if (!M || !PC || !Canvas) return;
	UFont* Font = GEngine->GetSmallFont();
	UFont* Big = GEngine->GetMediumFont() ? GEngine->GetMediumFont() : Font;
	const FTCMatchState& S = M->State;
	const bool bStrip = S.bHasMatch && S.Snapshots > 0;
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
		Lines.Add(FString::Printf(TEXT("%s | %s | tick %d | %s%s | units held %d (own %d) | selected %d"), *S.Map.Name, *Seat, S.Tick, *TC_T(FString::Printf(TEXT("%d MP"), S.Mp)),
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
	if (!S.PauseText.IsEmpty()) Lines.Add(S.PauseText);
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
	if (S.bHasMatch)
	{
		// the world's own overlays first, under every panel; then the score strip at the top centre
		View.YawDegrees = PC->ViewYaw();
		Minimap.ViewFromCanvas(Canvas, M->Terrain, View);
		DrawWorldOverlays(Font);
		// round 14: the paper war map over the world and its markers, under every panel (client/map-view.js)
		M->WarMap.Draw(Canvas, *M, *PC);
		BeginUi(FTCSettingsStore::UIScaleFactor(PC->UiScalePercent));
		const float StripBottom = DrawScoreStrip(Font, Big);
		BandTop = bStrip ? StripBottom * UiNow : 0.f; // round 14: camera.js band, #top (screen pixels: the strip is drawn at the UI scale)
		// client/camera.js showFollow: the follow chip under the strip
		const FString Chip = PC->FollowChip();
		if (!Chip.IsEmpty())
		{
			float TW = 0, TH = 0;
			const FString L = Chip + TEXT("  (Shift+Space or pan to stop)");
			LabelSize(L, TW, TH, Font, 1.f);
			AddPanel((Canvas->ClipX - TW) / 2 - Pad, StripBottom, TW + 2 * Pad, Line + 2.f, Panel);
			DrawLabel(L, Good, (Canvas->ClipX - TW) / 2, StripBottom + 1.f, Font, 1.f);
		}
		// the status lines go below the strip when the strip reaches the left column
		const float StripLeft = Canvas->ClipX / 2 - 360.f;
		if (StripLeft < 620.f) Y = FMath::Max(Y, StripBottom + Line + 6.f);
	}
	else BeginUi(FTCSettingsStore::UIScaleFactor(PC->UiScalePercent));
	for (int32 I = 0; I < Lines.Num(); I++)
	{
		if (I < Diagnostics || I == KeyLine) DrawShown(Lines[I], Text, 12.f, Y, Font, 1.f);
		else DrawLabel(Lines[I], Text, 12.f, Y, Font, 1.f);
		Y += Line;
	}
	if (!S.DenyReason.IsEmpty() || !S.DenyCmd.IsEmpty())
	{
		DrawLabel(FString::Printf(TEXT("Refused (%s): %s"), *S.DenyCmd, *TCOrders::DenySentence(S.DenyReason, S.DenyCmd)), Warn, 12.f, Y, Font, 1.f);
		Y += Line;
	}
	// What the last order or button did, for a few seconds: what was sent, or why nothing was (orange)
	const double Now = GetWorld()->GetTimeSeconds();
	if (!PC->LastOrderNote.IsEmpty() && Now - PC->LastOrderNoteTime < OrderNoteSeconds)
	{
		DrawLabel(PC->LastOrderNote, PC->bLastOrderSent ? Good : Warn, 12.f, Y, Font, 1.f);
		Y += Line;
	}
	if (S.bHasMatch)
	{
		// the minimap (bottom right): the recruit card fits between it and the match panel
		const float Ui = UiNow;
		EndUi();
		Minimap.Draw(Canvas, S, M->UnitTable, PC->Selected, View);
		DrawMinimapOverlays();
		BeginUi(Ui);
		const float PanelTop = DrawMatchPanel(Font);
		// beside the match panel when the window leaves room for the cards, else above it
		const float Beside = 8.f + 560.f + Gap, RoomRight = Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Min.X) - 12.f : Canvas->ClipX - Gap;
		const int32 PanelsBeforeRecruit = Panels.Num();
		if (RoomRight - Beside >= 360.f) DrawRecruitPanel(Font, Beside, Canvas->ClipY - Gap);
		else DrawRecruitPanel(Font, 8.f, PanelTop - 4.f);
		// round 14: camera.js band: from the top panels down to the recruit bar (#buy), the whole height when it is empty
		float RecruitTop = Canvas->ClipY * UiNow; // screen pixels, as the panels are kept
		for (int32 K = PanelsBeforeRecruit; K < Panels.Num(); K++) RecruitTop = FMath::Min(RecruitTop, (float)Panels[K].Min.Y);
		PC->SetClearBand(BandTop, RecruitTop);
		// client/alerts.js: the alert feed over the minimap and the Alt+H history
		DrawAlertFeed(Font);
		DrawEpilogue(Font, Big);
	}
	else
	{
		Minimap.Reset();
		DrawLobby(Font, Y + 4.f);
		// client/report.js: the result card beside the lobby after a decided match
		DrawMatchReport(Font, Y + 4.f);
	}
	EndUi();
	DrawControlsSheet(Font, Big);

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
