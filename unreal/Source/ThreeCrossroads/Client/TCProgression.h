// HQ tiers and the Armory: a port of the reference's research controls, for the facts this client actually receives.
// Ported from the pinned reference (FTCProgression::ReferencePin):
//   shared/tech.js         TECH (tier and line prices, line names, the 10% step), TIER_NAMES, techCost;
//   client/availability.js availability({ t: 'tech', kind }) and resources;
//   client/hud.js          drawTech (the research panel over the Command Card when one finished own HQ or Armory is
//                          selected, its labels and tooltips, the click through attempt, and the "ready" and
//                          "researched" announcements);
//   shared/sim.js          productionAccess and allied (whose HQs and Armories count as labs), isConstructionMode.
// The server still validates and decides every research order (shared/sim.js command 'tech'); a refusal comes back as
// { t: 'deny', cmd: 'tech', reason }, which TCOrders::DenySentence words. This class only says what the browser would
// show, announce and send. It runs no rules of its own and reads only what the server sends this seat: the snapshot's
// tech ({ tier, armory: { inf, guns, armor }, lab: [[kind, progress 0-1], ...] }, present only in matches created with
// tech on), mp, mun, mode and out, and the unit rows FTCMatchState holds.
//
// The wire command is { t: 'tech', kind } with kind 'tier' (the next HQ tier, at a finished HQ) or an Armory line
// ('inf', 'guns', 'armor'), as server.js passes any seated in-play message to shared/sim.js command.
//
// Not ported: the operative role (client/main.js skips hud.update for an operative, so its baseline for the
// announcements stands still); translations (client/i18n.js); the panel's position and styling.
#pragma once

#include "CoreMinimal.h"
#include "Client/TCRecruitment.h"

class FJsonObject;
class FTCMatchState;

// What one research order names: the next HQ tier, or the next level of one Armory line (shared/tech.js TECH.lines,
// in its key order).
enum class ETCTechKind : uint8
{
	Tier,
	Inf,
	Guns,
	Armor,
};
// The wire names: 'tier', 'inf', 'guns', 'armor'.
const TCHAR* LexToString(ETCTechKind Kind);

// What the next step of a kind costs (shared/tech.js techCost). Mun is 0 outside Classic and World.
struct FTCTechCost
{
	int32 Mp = 0, Mun = 0, Time = 0;
};

// One running research job (snapshot tech.lab): its kind as sent and its progress 0-1 (rounded to 0.1 by the server).
struct FTCResearchJob
{
	FString Kind;
	double Progress = 0;
};

// One research button of the panel (client/hud.js drawTech).
struct FTCResearchButton
{
	ETCTechKind Kind = ETCTechKind::Tier;
	FString Text;          // "Upgrade to Company HQ: 250 MP, 60s", "Vehicle Armor 1: 70%", "Top tier", "Vehicle Guns 3/3"
	FString Tooltip;       // the button's own title (what it does)
	FTCRecruitCheck Check; // availability({ t: 'tech', kind })
	bool bMaxed = false;   // techCost is null: nothing more to research on this button
	bool bResearching = false;
	double Progress = 0;   // the job's progress as sent, when bResearching
	// What the browser's tooltip shows (client/feedback.js setAvailability): the refusal, else the title.
	const FString& ShownTooltip() const { return Check.bOk ? Tooltip : Check.Reason; }
};

// The research panel. Shown when the match has tech and exactly one own finished HQ or Armory is selected; the browser
// then also unhides the Command Card it sits on.
struct FTCResearchPanel
{
	bool bShown = false;
	int32 Building = 0;      // the selected HQ or Armory
	FString BuildingType;    // 'hq' or 'armory'
	FString TierLabel;       // TIER_NAMES[tech.tier]: the side's current HQ tier
	TArray<FTCResearchButton> Buttons; // HQ: the tier; Armory: inf, guns, armor
};

// What client/hud.js attempt does with a press: the refusal (Check.Reason, shown as feedback), or one message to send
// with the 'recruit' blip.
struct FTCResearchPress
{
	FTCRecruitCheck Check;
	TSharedPtr<FJsonObject> Message; // { t: 'tech', kind } when Check.bOk
	bool bBlip = false;              // client/hud.js ctx.act('blip', 'recruit') after the send
};

class FTCProgression
{
public:
	// The reference commit the built-in tech table and sentences come from.
	static const TCHAR* const ReferencePin;
	// shared/tech.js TECH.step as a whole percentage (0.1 * 100 is exactly 10 in JavaScript).
	static constexpr int32 StepPercent = 10;
	static constexpr int32 TopTier = 3;
	static constexpr int32 MaxLevel = 3;

	static bool ParseKind(const FString& Name, ETCTechKind& Out);
	static bool IsLine(ETCTechKind Kind) { return Kind != ETCTechKind::Tier; }
	// shared/tech.js TECH.lines: 'Infantry Weapons', 'Vehicle Guns', 'Vehicle Armor' (empty for the tier).
	static const TCHAR* LineName(ETCTechKind Kind);
	// The button's title as client/hud.js writes it.
	static FString Tooltip(ETCTechKind Kind);
	// { t: 'tech', kind }
	static TSharedRef<FJsonObject> Message(ETCTechKind Kind);

	// FTCUnitTable::Reference after it loads: research stays refused until it equals ReferencePin.
	void SetUnitTableReference(const FString& Reference);
	bool ReferenceMatches() const { return bReferenceMatches; }

	// Message hooks, called next to FTCMatchState's. A start forgets the last snapshot; a snapshot before any start is
	// ignored, as FTCMatchState ignores it. Clear when the match ends. Neither touches the announcement baseline: the
	// browser's HUD keeps it for as long as the page lives (see ResetAnnouncements).
	void ApplyStart(const FJsonObject& M);
	void ApplySnapshot(const FJsonObject& M);
	void Clear();
	bool HasSnapshot() const { return bHasSnapshot; }

	// "Company HQ ready", "Vehicle Guns 2 researched": what each snapshot announced (client/hud.js drawTech compares
	// tech with the last snapshot's), oldest first. Taking them empties the list.
	TArray<FString> TakeAnnouncements();
	// Forget the announcement baseline, as a page load would: the next tech snapshot announces nothing.
	void ResetAnnouncements();

	// The last snapshot's research state. HasTech: the match has tech and the server's tech object could be read.
	bool HasTech() const { return bHasSnapshot && bTech && !bTechUnreadable; }
	int32 Tier() const { return (int32)TechTier; }
	int32 Level(ETCTechKind LineKind) const;
	const TArray<FTCResearchJob>& Lab() const { return LabJobs; }
	const FTCResearchJob* Job(ETCTechKind Kind) const;
	// shared/tech.js techCost for this side: unset when maxed or when there is no readable tech. bClassic says whether
	// Munitions are charged (availability passes the mode; the panel's label passes whether the snapshot has mun).
	TOptional<FTCTechCost> NextCost(ETCTechKind Kind, bool bClassic) const;
	// Finished HQs ('tier') or Armories (a line) this seat may research at: its own, and in Horde an ally's.
	int32 Labs(const FTCMatchState& State, ETCTechKind Kind) const;

	// client/availability.js availability({ t: 'tech', kind }).
	FTCRecruitCheck CheckResearch(const FTCMatchState& State, ETCTechKind Kind) const;
	// client/hud.js drawTech's click: refused with the reason, or the message to send.
	FTCResearchPress Research(const FTCMatchState& State, ETCTechKind Kind) const;
	// client/hud.js drawTech's panel for this selection.
	FTCResearchPanel Panel(const FTCMatchState& State, const TSet<int32>& Selected) const;

private:
	bool bReferenceMatches = false;
	FString UnitTableReference;
	bool bStarted = false, bHasSnapshot = false;
	// the last snapshot's scalars (the server sends them whole every snapshot)
	TOptional<double> SnapMp, SnapMun;
	bool bMunField = false;           // the snapshot has mun at all (client/hud.js drawTech: s.mun !== undefined)
	FString ModeKind;
	bool bSuddenDeath = false;
	TOptional<double> ModeSlot;       // Horde: the attacking seat
	TArray<bool> OutSeats;
	bool bTech = false, bTechUnreadable = false;
	double TechTier = 0;
	double Armory[3] = { 0, 0, 0 };   // inf, guns, armor
	TArray<FTCResearchJob> LabJobs;
	// client/hud.js techSeen: the last snapshot's tier and lines, unset after a snapshot without tech
	bool bSeen = false;
	double SeenTier = 0;
	double SeenArmory[3] = { 0, 0, 0 };
	TArray<FString> Announcements;

	bool Classic() const;
	bool Allied(const FTCMatchState& State, int32 A, int32 B) const;
	bool ProductionAccess(const FTCMatchState& State, int32 Owner) const;
	double LevelJs(ETCTechKind LineKind) const;
	TOptional<FTCTechCost> CostJs(ETCTechKind Kind, bool bClassic) const;
	FTCRecruitCheck Gate(const FTCMatchState& State) const;
};
