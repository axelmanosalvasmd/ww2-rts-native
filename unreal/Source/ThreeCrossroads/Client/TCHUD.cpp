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
#include "Client/TCMarkers.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCPersistence.h"
#include "Client/TCLocale.h"
#include "Client/TCAlerts.h"
#include "Client/TCEndReport.h"
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
	Minimap.Reset(); // releases the texture with the HUD
	Super::EndPlay(Reason);
}

void ATCHUD::DrawLabel(const FString& Str, const FLinearColor& Colour, float X, float Y, UFont* Font, float Scale)
{
	const FString Shown = TC_T(Str);
	NoteKept(Str, Shown);
	AHUD::DrawText(Shown, Colour, X, Y, Font, Scale);
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

float FTCHudFit::StripMaxWidth(float ClipX, float CornerRight)
{
	return FMath::Max(0.f, ClipX - 2.f * FMath::Max(8.f, CornerRight + 4.f));
}

FBox2D FTCHudFit::SandboxPlace(float ClipX, float ClipY, FVector2D Size, float StripLow, const TArray<FBox2D>& Avoid, bool* bOutBrowserSpot, const TArray<FBox2D>& SlideFrom)
{
	auto Hits = [](const FBox2D& A, const FBox2D& S) { return A.bIsValid && A.Min.X < S.Max.X && A.Max.X > S.Min.X && A.Min.Y < S.Max.Y + 4.f && A.Max.Y > S.Min.Y - 4.f; };
	// round 17: centred, or slid left just clear of a SlideFrom box it would meet (the alert history at the right),
	// staying 8 px in from the left; null when it still meets one
	auto Across = [&](float Top, FBox2D& Out)
	{
		float X = FMath::Max(8.f, (ClipX - Size.X) / 2);
		for (int32 Pass = 0; Pass < 2; Pass++)
		{
			const FBox2D Try(FVector2D(X, Top), FVector2D(X + Size.X, Top + Size.Y));
			const FBox2D* Met = SlideFrom.FindByPredicate([&](const FBox2D& A) { return Hits(A, Try); });
			if (!Met) { Out = Try; return true; }
			X = Met->Min.X - 4.f - Size.X;
			if (X < 8.f) break;
		}
		return false;
	};
	FBox2D Spot;
	bool bClear = ClipY - SandboxBottom - Size.Y >= StripLow + 4.f && Across(ClipY - SandboxBottom - Size.Y, Spot);
	for (const FBox2D& A : Avoid)
		if (bClear && Hits(A, Spot)) bClear = false;
	if (bOutBrowserSpot) *bOutBrowserSpot = bClear;
	if (bClear) return Spot;
	FBox2D Under;
	if (Across(StripLow + 4.f, Under)) return Under;
	const float X = FMath::Max(8.f, (ClipX - Size.X) / 2);
	return FBox2D(FVector2D(X, StripLow + 4.f), FVector2D(X + Size.X, StripLow + 4.f + Size.Y));
}

int32 FTCHudFit::FitLevel(const TArray<float>& Heights, float ClipY, float BottomGap, float TopLimit)
{
	for (int32 L = 0; L < Heights.Num(); L++) if (ClipY - BottomGap - Heights[L] >= TopLimit) return L;
	return FMath::Max(0, Heights.Num() - 1);
}

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

int32 FTCHudFit::TextRowsShown(int32 Lines, float Room, float RowPx)
{
	const int32 Fit = RowPx > 0.f ? FMath::Max(0, FMath::FloorToInt((Room + 0.01f) / RowPx)) : Lines;
	return Lines <= Fit ? Lines : Fit;
}

float FTCHudFit::NarrowerPanel(float Start, float Floor, float Step, TFunctionRef<bool(float)> Fits)
{
	float Best = Start;
	for (float Try = Start - Step; Try > Floor - Step + 0.01f; Try -= Step)
	{
		const float W = FMath::Max(Try, Floor);
		if (!Fits(W)) break;
		Best = W;
		if (W <= Floor) break;
	}
	return Best;
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

FVector2D ATCHUD::StartBand(float ScreenH, float UiScale, bool bClassic)
{
	return FVector2D((CssGap + CssEmptyScores) * UiScale, bClassic ? ScreenH : ScreenH - (CssGap + CssBuyTabs) * UiScale);
}

float ATCHUD::MatchPanelWidth(float RoomRight)
{
	const float Beside = RoomRight - (8.f + Gap) - RecruitMinW;
	return Beside >= MatchPanelW ? MatchPanelW : FMath::Max(LeftColumnMinW, Beside);
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
	if (bLobby) AddButtons({ FItem{ TCTutorialEntry::Label(), FTCUiCommand(ETCUiAction::Tutorial), true, TCTutorialEntry::Title() } });
	if (L.HasLobby() && bLobby)
	{
		FString Info = FTCLobbySettings::ModeInfo(R.Mode);
		// round 17: main.js renderLobby appends the Horde record (server.js hordeBest) to #modeInfo; the page translates
		// the text sentence by sentence, so es.js's 'Record here with ...' and 'No record yet here with ...' keys apply
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
		for (const FTCControlItem& It : Row.Items) { FTCUiCommand C(ETCUiAction::MatchControl); C.Unit = It.Key; C.Group = It.Slot; Items.Add({ It.Label, C, It.bEnabled, It.Hover }); }
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
	// round 16: each text row with the fit level from which it is left out (FTCHudFit::FitLevel; 99: always shown)
	struct FPanelRow { FString Key; FLinearColor Value; int32 DropAt = 99; bool bMore = false; };
	TArray<FPanelRow> Rows;
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
	// round 15: hud.js drawSelection's heading with nothing selected ('Selection') and the Controls sheet's two lines for
	// selecting (client/index.html: 'Left click' and 'Drag'), the browser's words, so es.js has them all
	else if (!S.bSpectator) Rows.Add({ TEXT("Selection  |  Select a unit or building  |  Select units in a box"), Dim });
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
		// round 15: the card's heading (hud.js drawLogistics: <div class="hd">Logistics</div>)
		if (Card.Entries.Num()) Rows.Add({ TEXT("Logistics"), Dim, 3 });
		for (int32 I = 0; I < Card.Entries.Num() && I < 4; I++)
		{
			const FTCLogisticsEntry& E = Card.Entries[I];
			const FTCUnit* EU = S.Units.Find(E.Id);
			// hud.js drawLogistics: tr(name(v.type, v.owner)) + ' #' + id; round 16: rows past the first two (fit level
			// 2) or the first (level 3) go to the count below, as the browser's list scrolls
			Rows.Add({ FString::Printf(TEXT("%s #%d: %s"), *FTCAlerts::UnitName(S, &M->UnitTable, E.Type, EU ? EU->Owner : S.You), E.Id, *FString::Join(E.Lines, TEXT("  |  "))), Good, I >= 2 ? 2 : I >= 1 ? 3 : 99 });
		}
		// the rest are cut where the browser's list scrolls (logistics.css .logistics-reserves max-height and overflow:
		// auto): a count with no words, so it reads the same in both languages (round 15; it was an English sentence).
		// Round 16: the count is made at the fit level (bMore), from the entries left out there.
		if (Card.Entries.Num() > 1) { FPanelRow More{ FString(), Dim }; More.bMore = true; More.DropAt = Card.Entries.Num(); Rows.Add(More); }
		if (Card.bVisible && Card.bTrucks) Rows.Add({ FTCLogisticsCard::Hint, Dim });
	}
	// the control groups that hold units (client/selection.js groups; the browser shows them as badges)
	if (S.SeatConfirmed())
	{
		TArray<FString> Parts;
		for (int32 G = 1; G <= FTCControlGroups::GroupCount; G++)
			if (const int32 N = PC->Groups.Members(G).Num()) Parts.Add(FString::Printf(TEXT("%d: %d"), G, N));
		// the browser's words: hud.js's 'Control group' label, and the Controls sheet's line for Ctrl+1-9 (index.html; the
		// key written as es.js's 'Fullscreen: also lets Ctrl+1-9 set groups' writes it). Round 16: the hint only with a
		// selection (hud.js drawOrders puts its Control group row in #abil, which is empty with nothing selected)
		if (Parts.Num()) Rows.Add({ TEXT("Control group  ") + FString::Join(Parts, TEXT("   ")), Dim });
		else if (PC->Selected.Num()) Rows.Add({ TEXT("Ctrl+1-9: Set a control group; Shift adds to it"), Dim });
	}

	// the Formation menu's settings while the Orders panel shows (client/hud.js drawOrders: the Form button's submenu)
	if (!S.bSpectator && TCArmyCommands::ShowsOrdersPanel(S, M->UnitTable, PC->Selected))
	{
		const FTCFormationSettings& F = PC->Formation.Settings;
		// the Form submenu's own words (hud.js: 'Formation: {0}', 'Formation spacing {0}x', the Together and Snap switches),
		// each its own column so each is translated by itself
		// round 16: this summary is the native panel's own (the browser shows the state on the Form menu's buttons), so it
		// is the first row to go when the panel needs the room (fit level 2)
		Rows.Add({ FString::Printf(TEXT("Formation: %s  |  Formation spacing %.2fx  |  Together: %s  |  Snap: %s"), LexToString(F.Shape), F.Spread,
			F.bTogether ? TEXT("On") : TEXT("Off"), F.bSnap ? TEXT("On") : TEXT("Off")), Dim, 2 });
	}

	// The buttons (client/hud.js drawOrders), in rows that wrap at the panel's width: Stop and Attack-move always; for a
	// selection that is neither all buildings nor all trucks, Retreat, the three stances and (with a builder squad)
	// the Build menu; Resume deliveries for an all-truck selection; the logistics switches; End match for the host.
	// Slice 1f: the support bar (client/hud.js buildSupport and drawEcon) in its own row first, the Shell area button
	// with a salvo unit selected, the Form menu and the cover preview switch.
	// Compact (round 16): the label at fit level 1 and up (the support bar: what the browser's button shows, its key and
	// its cost; the browser draws the power's icon where this has none, and its title names it, as Hover does here)
	struct FItem { FString Label; FTCUiCommand Command; bool bEnabled; FLinearColor Fill; FString Hover; FString Compact; };
	TArray<FItem> Bar, Orders, Menu;
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
	auto Measure = [&](const FString& L) { float TW = 0, TH = 0; LabelSize(L, TW, TH, Font, 1.f); return TW + 2 * Pad; };
	const float RowH = Line + Pad + 4.f;
	// round 16: the layout at a fit level (FTCHudFit::FitLevel): the rows it keeps, the buttons in rows that wrap at the
	// panel's width, and its height
	struct FLayout { TArray<TPair<FString, FLinearColor>> Shown; TArray<TArray<FItem>> ButtonRows; float H = 0.f; bool bTooltip = false; int32 Cut = 0; };
	auto Build = [&](int32 Level)
	{
		FLayout Lo;
		auto Wrap = [&](const TArray<FItem>& Items)
		{
			TArray<FItem> Row; float X = 0;
			for (FItem I : Items)
			{
				if (Level >= 1 && !I.Compact.IsEmpty()) I.Label = I.Compact;
				const float IW = Measure(I.Label);
				if (Row.Num() && X + IW > W - 2 * Pad) { Lo.ButtonRows.Add(Row); Row.Reset(); X = 0; }
				Row.Add(I); X += IW + Pad;
			}
			if (Row.Num()) Lo.ButtonRows.Add(Row);
		};
		Wrap(Bar);
		Wrap(Orders);
		Wrap(Menu);
		// each text row in the language in use, wrapped at the panel's width (a narrow left column)
		int32 Dropped = 0;
		for (const FPanelRow& R : Rows)
		{
			if (R.bMore)
			{
				// the logistics count: the card's entries past the ones shown at this level (at most 4 are kept)
				const int32 Kept = Level >= 3 ? 1 : Level >= 2 ? 2 : 4;
				if (R.DropAt > Kept) Lo.Shown.Add({ FString::Printf(TEXT("... +%d"), R.DropAt - Kept), R.Value });
				continue;
			}
			if (Level >= R.DropAt) { Dropped++; continue; }
			const FString T = TC_T(R.Key);
			if (bDraw) NoteKept(R.Key, T);
			for (const FString& Piece : WrapShown(T, W - 2 * Pad, Font)) Lo.Shown.Add({ Piece, R.Value });
		}
		if (Level >= 4)
		{
			// round 17: the browser's fit (FTCHudFit::MaxLevel): no reserved hover row, the text rows cut from the end to
			// the room left under the limit, a count of the rest in the last row kept
			Lo.bTooltip = true;
			const float Room = Canvas->ClipY - Gap - TopLimit - Lo.ButtonRows.Num() * RowH - 2 * Pad;
			const int32 Keep = FTCHudFit::TextRowsShown(Lo.Shown.Num(), Room, Line);
			if (Keep < Lo.Shown.Num())
			{
				const int32 Cut = Lo.Shown.Num() - FMath::Max(0, Keep - 1);
				Lo.Cut = Cut;
				Lo.Shown.SetNum(FMath::Max(0, Keep - 1));
				if (Keep >= 1) Lo.Shown.Add({ FString::Printf(TEXT("... +%d"), Cut), Dim });
			}
			Lo.H = Lo.Shown.Num() * Line + Lo.ButtonRows.Num() * RowH + 2 * Pad;
			return Lo;
		}
		Lo.H = Lo.Shown.Num() * Line + Lo.ButtonRows.Num() * RowH + (Lo.ButtonRows.Num() ? Line : 0.f) + 2 * Pad;
		return Lo;
	};
	TArray<FLayout> Levels;
	TArray<float> Heights;
	for (int32 L = 0; L <= FTCHudFit::MaxLevel; L++)
	{
		Levels.Add(Build(L));
		Heights.Add(Levels.Last().H);
		if (Canvas->ClipY - Gap - Heights.Last() >= TopLimit) break; // this level fits: no need to build the next
	}
	const int32 Level = FTCHudFit::FitLevel(Heights, Canvas->ClipY, Gap, TopLimit);
	const FLayout& Lo = Levels[Level];
	MatchPanelCut = Lo.Cut;
	const float H = Lo.H, Y0 = Canvas->ClipY - H - Gap;
	if (!bDraw) return Y0;
	MatchPanelLevel = Level;
	if (Level != LoggedPanelLevel)
	{
		LoggedPanelLevel = Level;
		UE_LOG(LogTC, Log, TEXT("hud: match panel fit level %d (heights %s px, top limit %.1f px, window %.0f x %.0f UI px at %.0f%%)%s"), Level,
			*FString::JoinBy(Heights, TEXT("/"), [](float V) { return FString::Printf(TEXT("%.0f"), V); }), TopLimit, Canvas->ClipX, Canvas->ClipY, UiNow * 100.f,
			Y0 < TopLimit ? TEXT(": still above the limit") : TEXT(""));
	}
	FVector2D Mouse(-1, -1);
	float MX, MY;
	const bool bMouse = PC->GetMousePosition(MX, MY);
	if (bMouse) Mouse = FVector2D(MX, MY) / UiNow; // in the scaled panels' own pixels
	AddPanel(8.f, Y0, W, H, Panel);
	MatchPanelBox = FBox2D(FVector2D(8.f, Y0), FVector2D(8.f + W, Y0 + H));
	float Y = Y0 + Pad;
	for (const auto& R : Lo.Shown) { DrawShown(R.Key, R.Value, 14.f, Y, Font, 1.f); Y += Line; }
	FString Hover;
	for (const TArray<FItem>& Row : Lo.ButtonRows)
	{
		float X = 14.f;
		for (const FItem& I : Row)
		{
			const float Left = X;
			X = Button(Font, X, Y + 2.f, I.Label, I.Command, I.bEnabled, I.Fill, 0.f, W - 2 * Pad);
			if (bMouse && FBox2D(FVector2D(Left, Y + 2.f), FVector2D(X - Pad, Y + 2.f + Line + Pad)).IsInsideOrOn(Mouse)) Hover = I.Hover;
		}
		Y += RowH;
	}
	if (!Hover.IsEmpty() && Lo.bTooltip)
	{
		// level 4: the hover text over the panel's top edge, on its own plate (the browser's title tooltip)
		float TW = 0, TH = 0;
		LabelSize(Hover, TW, TH, Font, 1.f);
		const float TipW = FMath::Min(TW, Canvas->ClipX - 30.f);
		DrawRect(FLinearColor(FColor(25, 28, 30, 245)), 8.f, Y0 - Line - 4.f, TipW + 2 * Pad + 4.f, Line + 2.f);
		DrawLabel(Hover, Good, 14.f, Y0 - Line - 3.f, Font, 1.f);
	}
	else if (!Hover.IsEmpty()) DrawLabel(Hover, Good, 14.f, Y, Font, 1.f);
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
	// round 15: the text lines in the language in use, wrapped at the panel's width
	TArray<TPair<FString, FLinearColor>> ShownLines;
	for (const auto& L : Lines)
	{
		const FString T = TC_T(L.Key);
		NoteKept(L.Key, T);
		for (const FString& Piece : WrapShown(T, Width - 2 * Pad, Font)) ShownLines.Add({ Piece, L.Value });
	}
	float H = 2 * Pad + ShownLines.Num() * Line + Rows.Num() * RowH + Line;
	// round 17: index.html #buy.fit, the browser's fit for a narrow bar: the cards share the row's width (each at least
	// .uc's 50 px, names cut), so a list takes as few rows as its count allows instead of one card per row; the hover
	// row floats over the panel as a tooltip. Used when the card as built would rise past RecruitTopLimit (under the
	// strip and its boss bar, the sandbox panel and the alert history box; DrawHUD), round 16's squeezed column
	bool bFit = false;
	if (RecruitTopLimit > 0.f && Bottom - H < RecruitTopLimit)
	{
		bFit = true;
		Rows.Reset();
		const float Room = Width - 2 * Pad;
		for (const TArray<FItem>& L : Lists)
		{
			if (!L.Num()) continue;
			const int32 Cols = FMath::Clamp(FMath::FloorToInt((Room + Pad) / (RecruitFitMinW + Pad)), 1, L.Num());
			const int32 PerRow = FMath::Min(Cols, FMath::DivideAndRoundUp(L.Num(), FMath::DivideAndRoundUp(L.Num(), Cols)));
			const float CardW = (Room - (PerRow - 1) * Pad) / PerRow;
			TArray<FItem> Row;
			for (FItem I : L)
			{
				I.W = CardW;
				Row.Add(I);
				if (Row.Num() == PerRow) { Rows.Add(Row); Row.Reset(); }
			}
			if (Row.Num()) Rows.Add(Row);
		}
		H = 2 * Pad + ShownLines.Num() * Line + Rows.Num() * RowH;
		// logged when the fit changes (its width, rows or whether it stays under the limit), not every frame
		const FString FitKey = FString::Printf(TEXT("%.0f/%d/%d"), Width, Rows.Num(), Bottom - H < RecruitTopLimit ? 1 : 0);
		if (FitKey != RecruitFitLogged)
		{
			RecruitFitLogged = FitKey;
			UE_LOG(LogTC, Log, TEXT("hud: recruit card fit to the browser's #buy.fit at %.0f UI px wide: %d rows, top %.0f, limit %.0f%s"), Width, Rows.Num(), Bottom - H, RecruitTopLimit,
				Bottom - H < RecruitTopLimit ? TEXT(": still above the limit") : TEXT(""));
		}
	}
	const float Top = Bottom - H;
	AddPanel(Left, Top, Width, H, Panel);
	float Y = Top + Pad;
	for (const auto& L : ShownLines) { DrawShown(L.Key, L.Value, Left + Pad, Y, Font, 1.f); Y += Line; }
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
				const FString Shown = TC_T(I.Label);
				NoteKept(I.Label, Shown);
				DrawShown(FitShown(Shown, I.W - 2 * Pad, Font), Dim, X + Pad, Y + 2.f + Pad / 2, Font, 1.f);
				X = Rect.Max.X + Pad;
			}
			else X = Button(Font, X, Y + 2.f, I.Label, I.Command, I.bEnabled, I.Fill, I.W, I.W);
		}
		Y += RowH;
	}
	if (!Hover.IsEmpty())
	{
		float TW = 0, TH = 0;
		LabelSize(Hover, TW, TH, Font, 1.f);
		if (TW <= Width - 2 * Pad && !bFit) DrawLabel(Hover, Good, Left + Pad, Y, Font, 1.f);
		else
		{
			// round 15: a narrow card's hover line floats over the panel's top right, as the browser's tooltip floats
			const float HX = FMath::Max(8.f, Right - TW - 2 * Pad), HY = Top - Line - 2.f;
			DrawRect(Panel, HX, HY, TW + 2 * Pad, Line + 2.f);
			DrawLabel(Hover, Good, HX + Pad, HY + 1.f, Font, 1.f);
		}
	}
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
		auto Layer = [&](const TArray<FTCCoverGlyph>& Glyphs, float Opacity) { TCMarkersHud::DrawCoverLayer(Marks, Glyphs, Opacity); };
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
	// round 15: a match's first frame starts the reference's start band (StartBand)
	if (S.bHasMatch && !bHadMatch) { bStartBand = true; StartBandFrames = 0; }
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
	// round 15: the status lines as shown (each diagnostic as it is, each browser text through TC_T, the order note
	// through FTCLocale::OrderNote), then drawn in one column
	TArray<FStatusLine> Status;
	for (int32 I = 0; I < Lines.Num(); I++)
	{
		const bool bAsIs = I < Diagnostics || I == KeyLine;
		const FString Shown = bAsIs ? Lines[I] : TC_T(Lines[I]);
		if (!bAsIs) NoteKept(Lines[I], Shown);
		Status.Add({ Shown, Text, bAsIs });
	}
	if (!S.DenyReason.IsEmpty() || !S.DenyCmd.IsEmpty())
	{
		const FString Deny = FString::Printf(TEXT("Refused (%s): %s"), *S.DenyCmd, *TCOrders::DenySentence(S.DenyReason, S.DenyCmd));
		const FString Shown = TC_T(Deny);
		NoteKept(Deny, Shown);
		Status.Add({ Shown, Warn });
	}
	// What the last order or button did, for a few seconds: what was sent, or why nothing was (orange)
	const double Now = GetWorld()->GetTimeSeconds();
	if (!PC->LastOrderNote.IsEmpty() && Now - PC->LastOrderNoteTime < OrderNoteSeconds)
	{
		const FString Shown = FTCLocale::OrderNote(PC->LastOrderNote);
		NoteKept(PC->LastOrderNote, Shown);
		Status.Add({ Shown, PC->bLastOrderSent ? Good : Warn });
	}
	if (S.bHasMatch)
	{
		// the world's own overlays first, under every panel; then the score strip at the top centre
		View.YawDegrees = PC->ViewYaw();
		Minimap.ViewFromCanvas(Canvas, M->Terrain, View);
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
		float CornerClosedH = 0.f;
		{
			TArray<FTCControlRow> Closed;
			for (const FTCControlRow& R : CornerRows) { bool bMenu = false; for (const FTCControlItem& I : R.Items) bMenu |= I.Key == TEXT("Menu"); if (bMenu) Closed.Add(R); }
			if (!Closed.Num()) Closed = CornerRows;
			const FVector2D C = MeasureControlRows(Font, Closed);
			CornerRight = C.X > 0.f ? 8.f + C.X : 0.f;
			CornerClosedH = C.Y;
		}
		const int32 StripFrom = Panels.Num();
		const float StripBottom = DrawScoreStrip(Font, Big);
		BandTop = bStrip ? StripBottom * UiNow : 0.f; // round 14: camera.js band, #top (screen pixels: the strip is drawn at the UI scale)
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
		const float Ui = UiNow;
		EndUi();
		// the minimap (bottom right): the recruit card fits between it and the match panel
		Minimap.bDeferViewOutline = true; // round 17: client/main.js drawMinimap strokes the view last
		Minimap.Draw(Canvas, S, M->UnitTable, PC->Selected, View);
		DrawMinimapOverlays();
		Minimap.DrawViewOutline(Canvas, View);
		BeginUi(Ui);
		// round 15: the recruit card always sits beside the match panel at the bottom, between it and the minimap
		// (#buy at every UI scale); the match panel narrows only when the window leaves the card less than RecruitMinW
		const float RoomRight = Minimap.Rect().bIsValid ? ToUi((float)Minimap.Rect().Min.X) - 12.f : Canvas->ClipX - Gap;
		float MatchW = MatchPanelWidth(RoomRight);
		// round 16: the match panel stays under the strip (and its notice lines, the boss bar and the follow chip) where
		// they share columns; under the sandbox panel too when that sits under the strip over it. The sandbox panel takes
		// #sandbox's place (230 px over the bottom, centred) when that is clear of the strip, the match panel and the
		// recruit card (last frame's), else it goes under the strip
		const float StripLowUi = StripLow / UiNow;
		const bool bShareStrip = StripRight > StripLeft && StripLeft / UiNow < 8.f + MatchW;
		float PanelLimit = bShareStrip ? StripLowUi + 4.f : 0.f;
		const FVector2D SbSize = MeasureControlRows(Font, SandboxRows);
		SandboxBox = FBox2D(ForceInit);
		if (SbSize.X > 0.f)
		{
			const float PanelTop = DrawMatchPanel(Font, MatchW, PanelLimit, false);
			TArray<FBox2D> Avoid = { FBox2D(FVector2D(8.f, PanelTop), FVector2D(8.f + MatchW, Canvas->ClipY)) };
			if (RecruitBox.bIsValid) Avoid.Add(FBox2D(RecruitBox.Min / UiNow, RecruitBox.Max / UiNow));
			bool bSpot = false;
			// round 17: and clear of the alert history box (last frame's), sliding left from it rather than under it
			// (round 16: the history's header sat over the sandbox row's right end by about 20 px at 150%)
			TArray<FBox2D> Slide;
			if (AlertHistoryBox.bIsValid) Slide.Add(AlertHistoryBox);
			SandboxBox = FTCHudFit::SandboxPlace(Canvas->ClipX, Canvas->ClipY, SbSize, StripLowUi, Avoid, &bSpot, Slide);
			if (!bSpot && SandboxBox.Min.X < 8.f + MatchW) PanelLimit = FMath::Max(PanelLimit, SandboxBox.Max.Y + 4.f);
		}
		// round 16: with the settings in the menu the match panel is short; where the recruit card beside it would get
		// less than RecruitWantW (a large UI scale in a small window: a one-card column that grew over the strip's boss
		// bar), the panel gives the card room, 40 px at a time down to the browser's 248 px left column, for as long as
		// it still fits under the strip (FTCHudFit::NarrowerPanel)
		{
			const float Floor = FMath::Max(LeftColumnMinW, RoomRight - (8.f + Gap) - RecruitWantW);
			if (Floor < MatchW)
			{
				// round 17: at fit level 4 a narrower panel may wrap its buttons into more rows and cut more text rows:
				// it narrows only while it cuts no more than at its full width
				DrawMatchPanel(Font, MatchW, PanelLimit, false);
				const int32 CutAtStart = MatchPanelCut;
				MatchW = FTCHudFit::NarrowerPanel(MatchW, Floor, 40.f, [&](float Try) { return DrawMatchPanel(Font, Try, PanelLimit, false) >= PanelLimit && MatchPanelCut <= CutAtStart; });
			}
		}
		const float MatchTop = DrawMatchPanel(Font, MatchW, PanelLimit) * UiNow; // screen pixels
		const int32 PanelsBeforeRecruit = Panels.Num();
		// round 17: the recruit card's top stays under the strip and its lines (boss bar, follow chip), under the sandbox
		// panel where it shares columns with it, and leaves the alert history box (which stands on the card when the card
		// reaches under its span) room under the strip; past that it takes the browser's #buy.fit (DrawRecruitPanel)
		{
			const float CardLeft = 8.f + MatchW + Gap;
			RecruitTopLimit = bStrip ? StripLowUi + 4.f : 0.f;
			// the history's closed summary row (an open history's list may cover more, as the browser's does)
			if (AlertHistoryBox.bIsValid) RecruitTopLimit += FMath::Min((float)AlertHistoryBox.GetSize().Y, 32.f) + TCAlertLook::MinimapFrame + TCAlertLook::MinimapGap;
			if (SandboxBox.bIsValid && SandboxBox.Max.X > CardLeft) RecruitTopLimit = FMath::Max(RecruitTopLimit, (float)SandboxBox.Max.Y + 4.f);
		}
		DrawRecruitPanel(Font, 8.f + MatchW + Gap, Canvas->ClipY - Gap);
		// round 14: camera.js band: from the top panels down to the recruit bar (#buy), the whole height when it is empty
		float RecruitTop = Canvas->ClipY * UiNow; // screen pixels, as the panels are kept
		RecruitBox = FBox2D(ForceInit);
		for (int32 K = PanelsBeforeRecruit; K < Panels.Num(); K++) { RecruitTop = FMath::Min(RecruitTop, (float)Panels[K].Min.Y); RecruitBox += Panels[K]; }
		FVector2D Band(BandTop, RecruitTop);
		if (bStartBand)
		{
			// round 15: the band camera.js measured at receiveStart, until the controller has taken it (StartBand)
			Band = StartBand(Canvas->ClipY * UiNow, UiNow, TCArmyCommands::ClassicLobby(S));
			const FVector2D Taken = PC->ClearBand();
			// kept until the strip is up too: the controller measures again when the reported top first turns positive
			// (round 14's glue for a strip that arrives with the first snapshot), which must not happen after this
			if (++StartBandFrames > 600 || (bStrip && FMath::Abs(Taken.X - Band.X) < 0.5f && FMath::Abs(Taken.Y - Band.Y) < 0.5f))
			{
				bStartBand = false;
				UE_LOG(LogTC, Log, TEXT("hud: camera band at the match start %.1f to %.1f px (client/main.js receiveStart: empty #scores, %s), taken after %d frames; this HUD's own band now %.1f to %.1f px"),
					Band.X, Band.Y, TCArmyCommands::ClassicLobby(S) ? TEXT("no #buy in Classic and World") : TEXT("#buy with its group tabs"), StartBandFrames, BandTop, RecruitTop);
			}
		}
		PC->SetClearBand(Band.X, Band.Y);
		// round 16: the sandbox panel (#sandbox), then the corner: the Menu button (#util) and, open, the menu (#menu)
		// under it. The open menu is the browser's dropdown over the HUD (z-index 20): under the strip when it fits there
		// beside the strip's first card, else straight under the Menu button over the strip; drawn last of the panels,
		// so its buttons take the clicks (CommandAt reads the last drawn first)
		if (SandboxBox.bIsValid) DrawControlRows(Font, SandboxBox.Min.X, SandboxBox.Min.Y, 0.f, SandboxRows);
		// the corner's place now (drawn after the status column and the alert feed, over them): the status column keeps
		// to the closed Menu panel (CornerRight), the open menu covering it as the browser's dropdown covers the HUD
		const FVector2D CtlSize = MeasureControlRows(Font, CornerRows);
		float CtlTop = Y + 4.f;
		if (CtlSize.X > 0.f)
		{
			const bool bOverStrip = StripRight > StripLeft && StripLeft / UiNow < 8.f + CtlSize.X + 4.f && CtlTop < StripLowUi;
			if (bOverStrip && StripLowUi + 4.f + CtlSize.Y <= Canvas->ClipY - Gap) CtlTop = StripLowUi + 4.f;
		}
		float CtlBottom = CtlSize.X > 0.f ? (CornerRight > 0.f && M->Controls.MenuOpen() ? Y + 4.f + MeasureControlRows(Font, CornerRows.Num() ? TArray<FTCControlRow>{ CornerRows[0] } : CornerRows).Y : CtlTop + CtlSize.Y) * UiNow : 0.f;
		// round 16 (lead, controls' stats box on hudfit's corner): F2's box under the closed Menu panel (stats.js place:
		// under #util; the open menu is a dropdown over it), the status column under both
		{
			const float ClosedBottom = CtlSize.X > 0.f ? (CtlTop + (CornerClosedH > 0.f ? CornerClosedH : CtlSize.Y)) * UiNow : 0.f;
			const float ClosedRight = CtlSize.X > 0.f ? (CornerRight > 0.f ? CornerRight : 8.f + CtlSize.X) * UiNow : 0.f;
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
			if (LeftW >= ColumnMinW) { Col = Rows(LeftW); bPlaced = LeftTop + Col.Num() * Line <= MatchTop - 4.f; if (bPlaced) Cy = LeftTop; }
			if (!bPlaced && RightW >= ColumnMinW) { Col = Rows(RightW); Cx = StripRight + 12.f; bPlaced = 12.f + Col.Num() * Line <= RecruitTop - 4.f; }
			if (!bPlaced)
			{
				Col = Rows(Canvas->ClipX - 24.f); Cx = 12.f; Cy = UnderStripTop(Col.Num());
				// round 16: where even that runs into the match panel or the recruit card (a large UI scale in a small
				// window), the column keeps the lines the player needs (pause, notices, refusals, the order note) and
				// leaves out this client's own diagnostics, which the log keeps; before, they were drawn over the panel
				if (Cy + Col.Num() * Line > FMath::Min(MatchTop, RecruitTop) - 4.f)
				{
					Col = Rows(Canvas->ClipX - 24.f, false);
					Cy = UnderStripTop(Col.Num());
					if (!bLoggedDiagDrop) { bLoggedDiagDrop = true; UE_LOG(LogTC, Log, TEXT("hud: no room for the status column's diagnostics at %.0f%% in %.0f x %.0f; they stay in the log"), Ui2 * 100.f, Canvas->ClipX, Canvas->ClipY); }
				}
			}
			for (const FStatusLine& L : Col) { DrawShown(L.Shown, L.Colour, Cx, Cy, Font, 1.f); Cy += Line; }
			BeginUi(Ui2);
		}
		// client/alerts.js: the alert feed over the minimap and the Alt+H history
		DrawAlertFeed(Font);
		DrawEpilogue(Font, Big);
		// round 16: the corner last (#util, and #menu at z-index 20 over the HUD), on the browser's solid panel colour
		// (--panel-solid) while the menu is open, so what it covers does not show through
		if (CtlSize.X > 0.f)
		{
			if (M->Controls.MenuOpen()) DrawRect(FLinearColor(FColor(25, 28, 30, 245)), 8.f, CtlTop, CtlSize.X, CtlSize.Y);
			DrawControlRows(Font, 8.f, CtlTop, 0.f, CornerRows);
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

// Round 16 (hudfit): the fit rules at UI 150% in a 1280 x 720 window (853.3 x 480 UI pixels), the case slice 1p left
// crowded: the strip clear of the corner's Menu panel, the sandbox panel at #sandbox's place only where it is clear,
// the match panel's level the first that stays under the strip, and the settings' clicks routed to the HUD's actions.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCHudFitTest, "ThreeCrossroads.HUD.Fit", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCHudFitTest::RunTest(const FString&)
{
	const float ClipX = 1280.f / 1.5f, ClipY = 720.f / 1.5f;
	// the Menu panel: 8 px in, about 62 px wide at the small font ('Menu' plus padding)
	const float CornerRight = 8.f + 62.f;
	const float MaxW = FTCHudFit::StripMaxWidth(ClipX, CornerRight);
	TestEqual(TEXT("strip at most the window less both corners"), MaxW, ClipX - 2.f * (CornerRight + 4.f), 0.01f);
	// Horde with two teams: the lead block (216) and two team blocks (236), 4 px apart (DrawScoreStrip)
	const float Horde = 216.f + 4.f + 2 * 236.f + 4.f, W = FMath::Min(Horde, MaxW);
	TestTrue(TEXT("Horde, two teams: the strip's left edge clears the Menu panel"), (ClipX - W) / 2 >= CornerRight + 4.f - 0.01f);
	// four teams and a lead: narrowed to fit, still clear
	const float Wide = FMath::Min(150.f + 4.f + 4 * 236.f + 3 * 4.f, MaxW);
	TestTrue(TEXT("four teams: narrowed, clear of the corner"), Wide < 150.f + 4.f + 4 * 236.f + 3 * 4.f && (ClipX - Wide) / 2 >= CornerRight + 4.f - 0.01f);
	TestEqual(TEXT("a window narrower than both corners: 0"), FTCHudFit::StripMaxWidth(100.f, 60.f), 0.f);

	// the sandbox panel (Sandbox, the unit, the side, Spawn: about 330 x 54 UI px)
	const FVector2D Sb(330.f, 54.f);
	bool bSpot = true;
	// 150%: the strip with its notice line and the boss bar ends at 205 UI px; #sandbox's place (480 - 230 - 54 = 196)
	// is above that, so the panel goes under the strip
	FBox2D B = FTCHudFit::SandboxPlace(ClipX, ClipY, Sb, 205.f, { FBox2D(FVector2D(8, 380), FVector2D(568, 480)) }, &bSpot);
	TestFalse(TEXT("150%: #sandbox's place is under the strip's bottom: not used"), bSpot);
	TestEqual(TEXT("150%: under the strip"), (float)B.Min.Y, 209.f, 0.01f);
	TestEqual(TEXT("150%: centred"), (float)(B.Min.X + B.Max.X) / 2, ClipX / 2, 0.01f);
	// 100% in 1280 x 720: the strip ends at 120 px, the match panel's top at 600, the recruit card at 560 (x 576 to
	// 1080): #sandbox's place (436 to 490) is clear
	B = FTCHudFit::SandboxPlace(1280.f, 720.f, Sb, 120.f, { FBox2D(FVector2D(8, 600), FVector2D(568, 720)), FBox2D(FVector2D(576, 560), FVector2D(1080, 712)) }, &bSpot);
	TestTrue(TEXT("100%: #sandbox's own place"), bSpot);
	TestEqual(TEXT("100%: 230 px over the bottom"), (float)B.Max.Y, 720.f - FTCHudFit::SandboxBottom, 0.01f);
	// the same with a tall match panel reaching the spot: under the strip
	B = FTCHudFit::SandboxPlace(1280.f, 720.f, Sb, 120.f, { FBox2D(FVector2D(8, 420), FVector2D(568, 720)) }, &bSpot);
	TestFalse(TEXT("100%, a tall match panel under the spot: not used"), bSpot);
	TestEqual(TEXT("100%, a tall match panel: under the strip"), (float)B.Min.Y, 124.f, 0.01f);

	// the match panel's levels: the first whose top stays under the limit; the last when none does
	TestEqual(TEXT("level 0 fits"), FTCHudFit::FitLevel({ 200.f, 180.f }, ClipY, 8.f, 209.f), 0);
	TestEqual(TEXT("level 1 fits"), FTCHudFit::FitLevel({ 300.f, 260.f, 200.f, 180.f }, ClipY, 8.f, 209.f), 1);
	TestEqual(TEXT("level 2 fits"), FTCHudFit::FitLevel({ 300.f, 280.f, 250.f, 240.f }, ClipY, 8.f, 209.f), 2);
	TestEqual(TEXT("none fits: the last"), FTCHudFit::FitLevel({ 400.f, 380.f, 350.f, 340.f }, ClipY, 8.f, 209.f), 3);
	TestEqual(TEXT("no limit: level 0"), FTCHudFit::FitLevel({ 400.f, 380.f }, ClipY, 8.f, 0.f), 0);
	// round 17: level 4, round 16's worst case (UI 150% in 1280 x 720, a large Spanish selection, the sandbox row
	// under the strip and the boss bar: level 3 was 222 px tall under a top limit of 290.2 px, 182 px of room): five
	// rows of buttons (130 px) and the 12 px of padding leave 40 px, two rows of 16: the first text row and the count
	TestEqual(TEXT("level 4 when level 3 is still too tall"), FTCHudFit::FitLevel({ 290.f, 238.f, 222.f, 222.f, 174.f }, ClipY, 8.f, 290.2f), 4);
	TestEqual(TEXT("level 4: two of four text rows show (one and the count)"), FTCHudFit::TextRowsShown(4, 480.f - 8.f - 290.2f - 5 * 26.f - 12.f, 16.f), 2);
	TestEqual(TEXT("level 4: every row when they fit"), FTCHudFit::TextRowsShown(3, 64.f, 16.f), 3);
	TestEqual(TEXT("level 4: no room, no row"), FTCHudFit::TextRowsShown(3, 10.f, 16.f), 0);
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
	// round 17: the sandbox panel slides left of the alert history box (round 16's Spanish guest: the history at x 528
	// to 847 UI px, 231 to 258, stood on the recruit card; #sandbox's place 197 to 250 met it by about 20 px)
	{
		const FBox2D History(FVector2D(528.f, 231.f), FVector2D(847.f, 258.f));
		const FVector2D SbEs(373.f, 53.f);
		FBox2D C = FTCHudFit::SandboxPlace(ClipX, ClipY, SbEs, 180.f, { FBox2D(FVector2D(8, 257), FVector2D(405, 480)) }, &bSpot, { History });
		TestTrue(TEXT("sandbox beside the history: #sandbox's height kept"), bSpot && FMath::IsNearlyEqual((float)C.Max.Y, ClipY - FTCHudFit::SandboxBottom, 0.01f));
		TestTrue(TEXT("sandbox beside the history: 4 px left of it"), FMath::IsNearlyEqual((float)C.Max.X, 524.f, 0.01f) && C.Min.X >= 8.f);
		C = FTCHudFit::SandboxPlace(ClipX, ClipY, SbEs, 180.f, {}, &bSpot, { FBox2D(FVector2D(600.f, 100.f), FVector2D(847.f, 120.f)) });
		TestTrue(TEXT("a history box elsewhere: centred as before"), bSpot && FMath::IsNearlyEqual((float)(C.Min.X + C.Max.X) / 2, ClipX / 2, 0.01f));
	}

	// the panel narrows 40 px at a time while it fits, stopping at the floor or the first width that does not fit
	TestEqual(TEXT("narrows to the floor"), FTCHudFit::NarrowerPanel(560.f, 425.f, 40.f, [](float) { return true; }), 425.f, 0.01f);
	TestEqual(TEXT("stops where it no longer fits"), FTCHudFit::NarrowerPanel(560.f, 425.f, 40.f, [](float W) { return W >= 480.f; }), 480.f, 0.01f);
	TestEqual(TEXT("nothing narrower fits: the start"), FTCHudFit::NarrowerPanel(560.f, 425.f, 40.f, [](float) { return false; }), 560.f, 0.01f);
	TestEqual(TEXT("a floor over the start: the start"), FTCHudFit::NarrowerPanel(560.f, 600.f, 40.f, [](float) { return true; }), 560.f, 0.01f);
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
