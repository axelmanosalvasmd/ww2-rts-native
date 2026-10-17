// World Conquest: the region objective and paid recovery, as the browser presents them. Ported from the pinned
// reference (FTCWorldRecovery::ReferencePin):
//   shared/sim.js             worldSnapshot (what a seat is sent: home, size, total, goal, owned, cap, regions,
//                             stations, recovery), recoverWorld (the server's answers to { t: 'recover' }), stepWorld
//                             (what progress, capper, locked and contested mean), snapshotFor (top-level home);
//   server.js                 watcherSnapshot (a spectator's world has no home, owned, cap or recovery);
//   shared/world-territories.js territoryEdges, insideKnownRegion;
//   client/hud.js             drawScores (the World lead line and team blocks), drawClassicCard (the Restore HQ or
//                             Engineer card, its key, cost and disabled state), update (an all-truck selection skips the
//                             card);
//   client/world-regions.js   the region marker's text, capture ring and border;
//   client/markers.js         capturePoint set (ring colour, progress arc, contested), PROG_SEGS;
//   client/main.js            regionTeamColor, look (seat colours), foe, teams, applySnapshot (home, alert gate),
//                             drawMinimap (region borders, discs, locked ring, stations), sendCmd (observers send nothing);
//   client/alerts.js          snapshot (the World region alerts), underAttack (20 s per 30 m, quiet on screen);
//   client/alert-history.js   acceptTick, start (same match keeps its last tick).
//
// Only what the server sent this seat is read: the snapshot's world object (regions this side discovered or owns, with
// their last remembered state), its top-level home, mp, tick, winner and out, and the start's you, names, teams and
// matchId. Nothing here runs a rule: the server decides every recovery and refuses with { t: 'deny', cmd: 'recover',
// reason } (needs, territory, mp, pop, queueFull, blocked), which TCOrders::DenySentence words as the browser does.
//
// The first part (TCWorldRecoveryCore) is plain C++ with no engine types, so a standalone harness can check it against
// the reference's own functions by defining TC_WORLD_RECOVERY_CORE_ONLY before including this header.
#pragma once

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <map>
#include <string>
#include <utility>
#include <vector>

namespace TCWorldRecoveryCore
{
	inline constexpr double CellMetres = 2.0;          // shared/sim.js CELL (territoryEdges callers scale by 2)
	inline constexpr double PointRadius = 8.0;         // shared/sim.js CFG.pointRadius: claim radius, marker, minimap disc
	inline constexpr int ProgressSegments = 64;        // client/markers.js PROG_SEGS
	inline constexpr double FullHold = 0.999;          // capturePoint set: an owned point this full draws no arc
	inline constexpr double BoundsStep = 8.0;          // world-regions.js: a bounds border is sampled every 8 m or less
	inline constexpr double AttackEvery = 20.0;        // client/alerts.js ATTACK_EVERY (seconds)
	inline constexpr double AttackArea = 30.0;         // client/alerts.js AREA (metres)

	// One run of a region's discovered cells: row Row, columns [X0, X1) (shared/world-territories.js knownTerritoryRuns).
	struct FRun { double Row = 0, X0 = 0, X1 = 0; };
	// One border segment in cells, (X0, Y0) to (X1, Y1) (territoryEdges' [x, y, xx, yy]).
	struct FEdge { double X0 = 0, Y0 = 0, X1 = 0, Y1 = 0; };

	// Math.round: the nearest integer, halves toward +Infinity, and -0 for -0.5 up to -0.
	inline double JsRound(double V)
	{
		if (!std::isfinite(V)) return V;
		const double F = std::floor(V);
		const double R = V - F >= 0.5 ? F + 1.0 : F;
		return R == 0 ? std::copysign(0.0, V) : R;
	}

	// Number.prototype.toString(10) for a double: the shortest digits that read back the same, fixed notation for
	// 1e-7 <= |V| < 1e21 and exponent notation outside it, as ECMAScript Number::toString writes them.
	inline std::string JsNumber(double V)
	{
		if (std::isnan(V)) return "NaN";
		if (std::isinf(V)) return V > 0 ? "Infinity" : "-Infinity";
		if (V == 0) return "0";
		std::string Sign = V < 0 ? "-" : "";
		const double A = std::fabs(V);
		char Buf[64];
		int P = 1;
		for (; P <= 17; P++)
		{
			std::snprintf(Buf, sizeof Buf, "%.*e", P - 1, A);
			if (std::strtod(Buf, nullptr) == A) break;
		}
		if (P > 17) { P = 17; std::snprintf(Buf, sizeof Buf, "%.*e", P - 1, A); }
		// Buf is d[.ddd]e[+-]xx: the digits and the decimal exponent
		std::string Digits;
		const char* C = Buf;
		for (; *C && *C != 'e'; C++) if (*C >= '0' && *C <= '9') Digits.push_back(*C);
		const int Exp = *C == 'e' ? std::atoi(C + 1) : 0;
		while (Digits.size() > 1 && Digits.back() == '0') Digits.pop_back();
		const int K = (int)Digits.size(), N = Exp + 1;
		if (K <= N && N <= 21) return Sign + Digits + std::string((size_t)(N - K), '0');
		if (0 < N && N <= 21) return Sign + Digits.substr(0, (size_t)N) + "." + Digits.substr((size_t)N);
		if (-6 < N && N <= 0) return Sign + "0." + std::string((size_t)(-N), '0') + Digits;
		const int E = N - 1;
		std::string Out = Sign + Digits.substr(0, 1);
		if (K > 1) Out += "." + Digits.substr(1);
		return Out + "e" + (E < 0 ? "-" : "+") + std::to_string(E < 0 ? -E : E);
	}

	// shared/world-territories.js territoryEdges: the outline of a region's known cells, rows in the order given.
	inline std::vector<FEdge> TerritoryEdges(const FRun* Runs, int Count)
	{
		std::map<double, std::vector<std::pair<double, double>>> Rows; // runs by row (the reference's Map; -0 and 0 meet)
		for (int I = 0; I < Count; I++) Rows[Runs[I].Row].push_back({ Runs[I].X0, Runs[I].X1 });
		std::vector<FEdge> Edges;
		for (int I = 0; I < Count; I++)
		{
			const double Y = Runs[I].Row, A = Runs[I].X0, B = Runs[I].X1;
			Edges.push_back({ A, Y, A, Y + 1 });
			Edges.push_back({ B, Y, B, Y + 1 });
			for (const int Side : { -1, 1 })
			{
				double X = A;
				const double Line = Side < 0 ? Y : Y + 1;
				const auto Found = Rows.find(Y + Side);
				if (Found != Rows.end())
				{
					for (const std::pair<double, double>& Other : Found->second)
					{
						const double Lo = Other.first, Hi = Other.second;
						if (Hi <= X || Lo >= B) continue;
						if (Lo > X) Edges.push_back({ X, Line, Lo < B ? Lo : B, Line });
						X = X > Hi ? X : Hi;
						if (X >= B) break;
					}
				}
				if (X < B) Edges.push_back({ X, Line, B, Line });
			}
		}
		return Edges;
	}

	// insideKnownRegion for a region sent with runs: the cell under (X, Z) is in one of them.
	inline bool InsideRuns(const FRun* Runs, int Count, double X, double Z)
	{
		const double CX = std::floor(X / CellMetres), CY = std::floor(Z / CellMetres);
		for (int I = 0; I < Count; I++) if (Runs[I].Row == CY && CX >= Runs[I].X0 && CX < Runs[I].X1) return true;
		return false;
	}
	// insideKnownRegion for a region sent with bounds [x0, z0, x1, z1] in metres.
	inline bool InsideBounds(const double Bounds[4], double X, double Z)
	{
		return X >= Bounds[0] && Z >= Bounds[1] && X < Bounds[2] && Z < Bounds[3];
	}

	// client/markers.js capturePoint set: the progress arc. Segments of ProgressSegments are drawn; bOwnerColour says the
	// arc takes the owner's colour (else the capper's).
	struct FRingArc { int Segments = 0; bool bVisible = false; bool bOwnerColour = false; };
	inline FRingArc RingArc(bool bOwner, bool bCapper, double Progress)
	{
		FRingArc R;
		const double K = JsRound(Progress * ProgressSegments);
		R.Segments = std::isfinite(K) ? (int)K : 0;
		R.bOwnerColour = bOwner;
		R.bVisible = (bOwner || bCapper) && K > 0 && !(bOwner && Progress >= FullHold);
		return R;
	}

	// client/alerts.js snapshot, World part: what one region present in both snapshots raises.
	enum class EAlert { None, Lost, Claimed, Contested, UnderAttack };
	struct FRegionFacts
	{
		double Team = NAN, Progress = NAN, Capper = NAN; // NaN: the field was missing or not a number
		bool bContested = false;                         // truthy
	};
	// MyTeam: hooks.team() (teams[me] ?? me). bCapperFriend: friend(r.capper), a seat >= 0 that is not a foe.
	inline EAlert ClassifyRegion(const FRegionFacts& Was, const FRegionFacts& Now, double MyTeam, bool bCapperFriend)
	{
		const bool bHad = Was.Team == MyTeam, bHas = Now.Team == MyTeam;
		if (bHad && !bHas) return EAlert::Lost;
		if (!bHad && bHas) return EAlert::Claimed;
		if (Now.bContested && !Was.bContested && (bHas || bCapperFriend)) return EAlert::Contested;
		if (bHas && (Now.Progress < Was.Progress || (Now.Capper >= 0 && !bCapperFriend))) return EAlert::UnderAttack;
		return EAlert::None;
	}

	// One region of a snapshot as the alerts read it.
	struct FRegionState { double Id = NAN, X = NAN, Z = NAN; FRegionFacts Facts; };
	struct FRaised { int Index = -1; EAlert Alert = EAlert::None; }; // Index: into the current snapshot's regions

	// client/alerts.js snapshot, the World loop: each current region that was in the previous snapshot (by id, a repeated
	// id keeping its last entry, as new Map does; SameValueZero) and has a finite position raises at most one alert.
	// Friend(slot) is alerts.js friend: a seat >= 0 that is not a foe.
	template <typename FFriend>
	std::vector<FRaised> RegionAlerts(const FRegionState* Was, int WasCount, const FRegionState* Now, int NowCount, double MyTeam, FFriend&& Friend)
	{
		std::vector<FRaised> Out;
		for (int I = 0; I < NowCount; I++)
		{
			const FRegionState& R = Now[I];
			const FRegionState* Old = nullptr;
			for (int J = WasCount - 1; J >= 0 && !Old; J--)
				if (Was[J].Id == R.Id || (std::isnan(Was[J].Id) && std::isnan(R.Id))) Old = &Was[J];
			if (!Old || !std::isfinite(R.X) || !std::isfinite(R.Z)) continue;
			const EAlert A = ClassifyRegion(Old->Facts, R.Facts, MyTeam, Friend(R.Facts.Capper));
			if (A != EAlert::None) Out.push_back({ I, A });
		}
		return Out;
	}

	// client/hud.js update and drawClassicCard: whether the Restore card is drawn, or what takes its place.
	enum class ECard
	{
		Shown,            // the Restore HQ or Restore Engineer card
		NoSnapshot,       // nothing to draw from yet
		NotConstruction,  // the mode is neither Classic nor World: the recruit card is drawn instead
		TrucksOnly,       // every selected unit is a truck: the card is skipped (the logistics card)
		Building,         // one own building is selected: its card
		Engineers,        // own Engineers are selected: their build card
		NotOffered,       // world.recovery.available is not truthy (both alive, out, spectating, not World)
	};
	inline ECard CardDecision(bool bSnapshot, bool bClassicOrWorld, int Selected, bool bAllTrucks, bool bOwnBuilding, bool bOwnEngineer, bool bAvailable)
	{
		if (!bSnapshot) return ECard::NoSnapshot;
		if (Selected > 0 && bAllTrucks) return ECard::TrucksOnly;
		if (!bClassicOrWorld) return ECard::NotConstruction;
		if (bOwnBuilding) return ECard::Building;
		if (bOwnEngineer) return ECard::Engineers;
		return bAvailable ? ECard::Shown : ECard::NotOffered;
	}

	// client/alerts.js underAttack's quiet list: one "under attack" per AttackArea metres per AttackEvery seconds, and
	// none for a spot on screen (QUIET_ON_SCREEN; it can fire once the player looks away). The browser shares one list
	// among every under-attack alert (units, points, regions), so one instance should serve them all.
	class FAttackQuiet
	{
	public:
		bool Allow(double NowSeconds, double X, double Z, bool bOnScreen)
		{
			std::vector<FSpot> Kept;
			for (const FSpot& S : Spots) if (S.Until > NowSeconds) Kept.push_back(S);
			Spots.swap(Kept);
			for (const FSpot& S : Spots) if (std::hypot(S.X - X, S.Z - Z) <= AttackArea) return false;
			if (bOnScreen) return false;
			Spots.push_back({ X, Z, NowSeconds + AttackEvery });
			return true;
		}
		void Reset() { Spots.clear(); }
	private:
		struct FSpot { double X = 0, Z = 0, Until = 0; };
		std::vector<FSpot> Spots;
	};
}

#ifndef TC_WORLD_RECOVERY_CORE_ONLY

#include "CoreMinimal.h"

class FJsonObject;
class FJsonValue;
class FTCMatchState;

// One region this side knows (worldSnapshot regions): current where seen now, else as last seen; an own region always.
// Numbers the snapshot left out (or sent as something else) are NaN, so comparisons fail as the browser's do with
// undefined.
struct FTCWorldRegion
{
	double Id = NAN;
	FString Name, Kind;        // as a template literal writes them ('undefined' when missing)
	double Team = NAN;         // the owning team, -1 unowned
	double X = NAN, Z = NAN;   // the claim point, metres
	double Progress = NAN;     // 0-1 (rounded to 0.1 by the server)
	double Capper = NAN;       // the seat claiming an unowned region, -1 none
	bool bLocked = false;      // a Production Building of its owner (or a Regional Military Base) stands in it
	bool bContested = false;   // infantry of more than one team on the point
	// shared/sim.js: runs (a region map: the cells this side discovered) or, without a region map, bounds in metres.
	bool bRuns = false;
	TArray<TCWorldRecoveryCore::FRun> Runs;
	bool bBounds = false;
	double Bounds[4] = { 0, 0, 0, 0 };

	bool FinitePosition() const { return FMath::IsFinite(X) && FMath::IsFinite(Z); }
	TCWorldRecoveryCore::FRegionFacts Facts() const;
	// shared/world-territories.js insideKnownRegion, at (X, Z) in metres.
	bool Contains(double AtX, double AtZ) const;
};

// A station this side has seen (worldSnapshot stations): which region it serves and where it stands.
struct FTCWorldStation { double Region = NAN, X = NAN, Z = NAN; };

// The snapshot's world object and the few top-level fields read with it.
struct FTCWorldSnapshot
{
	FString ModeKind;              // s.mode.kind ('' when missing)
	bool bWorld = false;           // the snapshot has a world object
	bool bHome = false;            // s.home ?? s.world.home: the nearest own region's claim point (seats only). The
	                               // persisted home (the start's spawn, then the last one sent) is FTCSupportPowers::Home.
	FVector2D Home = FVector2D::ZeroVector;
	FString Size;                  // 'huge' or 'massive'
	double Total = NAN, Goal = NAN, Owned = NAN, Cap = NAN; // Owned and Cap: seats only
	TArray<FTCWorldRegion> Regions;
	bool bStations = false;
	TArray<FTCWorldStation> Stations;
	bool bRecovery = false;        // seats only
	bool bRecoveryAvailable = false;
	double RecoverHqCost = NAN, RecoverEngineerCost = NAN;
	FString RecoverHqText = TEXT("undefined"), RecoverEngineerText = TEXT("undefined"); // as `${cost}` writes them
	double Mp = NAN;               // s.mp (a spectator gets none)
	double Tick = NAN;
	bool bWinner = false;          // s.winner != null
	TArray<bool> Out;              // s.out, truthy per seat
};

// The lead block of the score panel in World Conquest (hud.js drawScores, kind 'world').
struct FTCWorldScoreLead
{
	bool bShown = false;           // the snapshot's mode is World
	FString Mode;                  // 'World Conquest'
	FString Clock;                 // '<owned>/<goal> regions'
	FString Title;                 // the tooltip
};

// One team block of the score panel (hud.js drawScores, kind 'world').
struct FTCWorldTeamScore
{
	double Team = NAN;
	TArray<int32> Members;         // seats on this team, in seat order
	TArray<FString> Held;          // per member: 'Out' or ''
	TArray<bool> HeldDanger;       // per member: out
	double Count = 0, Total = 0;
	double Fraction = 0;           // the bar
	FString Label;                 // u0: 'Owned' (this client's team) or 'Discovered'
	FString Number;                // '<count> / <total>'
	FString Unit;                  // 'regions'
	FColor Colour = FColor::White; // the bar's colour: the first member's seat colour
};

// One region's 3D marker (world-regions.js) and minimap entry (main.js drawMinimap).
struct FTCWorldRegionView
{
	int32 Index = -1;              // into Snapshot().Regions
	FString Text;                  // '<name> (<kind>): owned | destroy military base | infantry can claim'
	FString Name;                  // the label above the point
	FVector2D At = FVector2D::ZeroVector;
	FColor TeamColour = FColor::White; // regionTeamColor(team): border, minimap disc
	bool bOwner = false;           // team >= 0: the ring and flag take TeamColour, else the neutral look
	bool bCapper = false;          // capper >= 0: CapperColour is that seat's colour
	FColor CapperColour = FColor::White;
	double Progress = 0;           // r.progress ?? 0
	bool bContested = false;       // the pulsing inner ring
	TCWorldRecoveryCore::FRingArc Arc;
	FColor ArcColour = FColor::White; // owner's colour when bOwner, else the capper's
	bool bLocked = false;          // minimap: a brass ring around the disc
	// The border in metres: line segments (start, end) for runs, or a closed loop of points for bounds (3D: every
	// BoundsStep metres or less along each side; the minimap strokes the plain rectangle Bounds).
	TArray<TPair<FVector2D, FVector2D>> Segments;
	TArray<FVector2D> Loop;
};

// A station block on the minimap: a dark 14 m square with a 9 m square of its region's team colour.
struct FTCWorldStationView
{
	FVector2D At = FVector2D::ZeroVector;
	FColor Fill = FColor::White;
};

enum class ETCWorldAlertKind : uint8
{
	PointLost,    // 'Region lost: <name>' (alerts.js kind 'pointLost')
	PointWon,     // 'Region claimed: <name>' ('pointWon')
	Attack,       // 'Claim contested: <name>' ('attack', never throttled)
	UnderAttack,  // 'Region under attack: <name>' ('attack' through underAttack: pass it through FAttackQuiet first)
};
const TCHAR* LexToString(ETCWorldAlertKind Kind);

struct FTCWorldAlert
{
	ETCWorldAlertKind Kind = ETCWorldAlertKind::PointLost;
	FString Text;
	double X = 0, Z = 0;
	double Tick = 0;               // the snapshot that raised it (alerts.js history time is tick / 20)
};

// The Restore card (hud.js drawClassicCard, the recovery branch).
struct FTCWorldRecoveryCard
{
	TCWorldRecoveryCore::ECard Decision = TCWorldRecoveryCore::ECard::NoSnapshot;
	bool bShown = false;
	bool bMissingHq = false;       // no own HQ row: the card restores the HQ, else an Engineer
	FString Name;                  // 'Restore HQ' or 'Restore Engineer'
	FString Cost;                  // '<cost> MP'
	FString Sub;                   // 'Deploys in friendly territory'
	double CostValue = NAN;
	bool bDisabled = false;        // the button's disabled: s.mp < cost
	FString Key;                   // the browser's card key: 'recover:<missingHQ>:<cost>' when shown
};

// What a press of the Restore button does.
struct FTCWorldRecoverResult
{
	bool bSend = false;
	bool bNative = false;          // the reason is this client's own, not a sentence the browser shows
	FString Reason;
	TSharedPtr<FJsonObject> Message; // { t: 'recover' } when bSend
};

class FTCWorldRecovery
{
public:
	// The reference commit this port reads.
	static const TCHAR* const ReferencePin;
	// client/main.js COLORS (seat colours), regionTeamColor's fallback, look's fallback, and the minimap's brass and
	// station frame colours.
	static FColor SlotColour(double Slot);
	static const FColor NeutralTeamColour;   // 0xaaaaaa
	static const FColor LockedColour;        // 0xd6b25e
	static const FColor StationFrameColour;  // 0x1b1b18
	static constexpr double BorderOpacity = 0.55;   // world-regions.js border lines
	static constexpr double BorderLift = 0.2;       // metres above the ground
	static constexpr double LabelHeight = 12.0;     // the name label above the point

	// Message hooks. ApplyStart after FTCMatchState::ApplyStart (client/main.js startGame: lastSnap = null; alerts
	// startMatch keeps its last tick only for the same match id). ApplySnapshot after FTCMatchState::ApplySnapshot, so
	// the unit rows are this snapshot's; ignored before a start. Clear when the match ends (ClearMatch).
	void ApplyStart(const FJsonObject& M);
	void ApplySnapshot(const FJsonObject& M, const FTCMatchState& State);
	void Clear();

	bool HasSnapshot() const { return bHasSnapshot; }
	bool IsWorld() const { return bHasSnapshot && Current.bWorld; }
	const FTCWorldSnapshot& Snapshot() const { return Current; }
	// The first region with this id (main.js regions.find), or null.
	const FTCWorldRegion* FindRegion(double Id) const;
	// The region whose known cells (or bounds) hold the spot, first in snapshot order, or null.
	const FTCWorldRegion* RegionAt(double X, double Z) const;

	// client/main.js teams[slot] ?? slot, this client's team (teams[me] ?? me), foe and alerts.js friend.
	double TeamOf(double Slot) const;
	double MyTeam() const { return TeamOf(Me); }
	bool Foe(double Slot) const { return TeamOf(Slot) != MyTeam(); }
	bool Friend(double Slot) const { return Slot >= 0 && !Foe(Slot); }
	// client/main.js regionTeamColor: grey for no team, else the colour of the first seat on that team.
	FColor RegionTeamColour(double Team) const;

	// The score panel (hud.js drawScores, kind 'world').
	FTCWorldScoreLead ScoreLead() const;
	TArray<FTCWorldTeamScore> TeamScores() const;

	// world-regions.js snapshot and main.js drawMinimap for every known region, in snapshot order.
	TArray<FTCWorldRegionView> RegionViews() const;
	FTCWorldRegionView RegionView(int32 Index) const;
	TArray<FTCWorldStationView> StationViews() const;

	// The region alerts raised since the last call (alerts.js snapshot), oldest first. Raised only for a confirmed seat
	// that is not spectating and not out, from the second snapshot after a start, while the match is undecided.
	TArray<FTCWorldAlert> TakeAlerts();

	// The Restore card for this selection (hud.js update and drawClassicCard).
	FTCWorldRecoveryCard Card(const FTCMatchState& State, const TSet<int32>& Selected) const;
	// A press of the Restore button: { t: 'recover' } when the card is shown and enabled and this client commands its
	// seat (sendCmd sends nothing for an observer); else why not.
	FTCWorldRecoverResult Recover(const FTCMatchState& State, const TSet<int32>& Selected) const;
	static TSharedRef<FJsonObject> RecoverMessage();

private:
	bool bStarted = false, bHasSnapshot = false, bHasPrevious = false;
	int32 Me = -1;
	TArray<double> Teams;          // the start's teams (NaN for a non-number entry), or every seat its own team
	int32 NameCount = 0;           // the start's names: hud.js members() walks these seats
	FTCWorldSnapshot Current, Previous;
	// client/alert-history.js: the match the alerts belong to and the last tick they accepted
	bool bAlertMatch = false;
	double AlertMatch = NAN;
	double LastAlertTick = -1;
	TArray<FTCWorldAlert> PendingAlerts;

	void RaiseAlerts(const FTCMatchState& State);
	static void ParseWorld(const FJsonObject& World, FTCWorldSnapshot& Out);
};

#endif // TC_WORLD_RECOVERY_CORE_ONLY
