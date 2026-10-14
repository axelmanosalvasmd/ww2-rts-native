#include "Client/TCControlGroups.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "InputCoreTypes.h"

const TCHAR* LexToString(ETCGroupOp Op)
{
	switch (Op)
	{
	case ETCGroupOp::Recall: return TEXT("recall");
	case ETCGroupOp::Set: return TEXT("set");
	case ETCGroupOp::Append: return TEXT("append");
	case ETCGroupOp::Transfer: return TEXT("transfer");
	default: return TEXT("none");
	}
}

namespace
{
	// shared/sim.js: the supply truck's type. Groups never hold one (client/selection.js group and prune).
	const TCHAR* const TruckType = TEXT("truck");

	bool IsTruck(const FTCUnit& U) { return U.Type == TruckType; }
	bool IsGroupNumber(int32 Group) { return Group >= 1 && Group <= FTCControlGroups::GroupCount; }
}

ETCGroupOp FTCControlGroups::OpForChord(bool bCtrl, bool bShift, bool bAlt)
{
	// client/keys.js: each binding names its exact modifiers, and match() needs all three to agree
	if (bAlt) return bCtrl || bShift ? ETCGroupOp::None : ETCGroupOp::Transfer;
	if (bShift) return ETCGroupOp::Append; // Shift+N and Ctrl+Shift+N
	if (bCtrl) return ETCGroupOp::Set;
	return ETCGroupOp::Recall;
}

int32 FTCControlGroups::DigitOf(const FKey& Key)
{
	static const FKey* const Digits[GroupCount] = { &EKeys::One, &EKeys::Two, &EKeys::Three, &EKeys::Four, &EKeys::Five, &EKeys::Six, &EKeys::Seven, &EKeys::Eight, &EKeys::Nine };
	for (int32 I = 0; I < GroupCount; I++) if (Key == *Digits[I]) return I + 1;
	return 0;
}

bool FTCControlGroups::IsOwnAlive(const FTCMatchState& State, const FTCUnit& U)
{
	return State.IsOwn(U) && U.Hp > 0;
}

bool FTCControlGroups::CanBeMember(const FTCMatchState& State, const FTCUnit& U)
{
	return IsOwnAlive(State, U) && !IsTruck(U);
}

bool FTCControlGroups::IsRecallable(const FTCMatchState& State, const FTCUnit& U, const FTCUnitDef* Def)
{
	// selectableOwn = own && def && (mobileOwn || building); mobileOwn = not a structure, not parked, not riding
	if (!Def || !IsOwnAlive(State, U)) return false;
	const bool bMobile = !Def->bStructure && !(U.Flags & (TCClientFlags::PlaneAtBase | TCFlags::Riding));
	return bMobile || Def->bBuilding;
}

bool FTCControlGroups::IsArmy(const FTCMatchState& State, const FTCUnit& U, const FTCUnitDef* Def)
{
	// selectable = armyUnit && mobileOwn: trains run themselves and trucks stay on their supply runs
	return Def && IsOwnAlive(State, U) && !Def->bStructure && !Def->bRail && !IsTruck(U) && !(U.Flags & (TCClientFlags::PlaneAtBase | TCFlags::Riding));
}

void FTCControlGroups::Reset()
{
	for (TArray<int32>& G : Groups) G.Reset();
	LastGroup = 0;
	LastRecall = TNumericLimits<double>::Lowest();
}

void FTCControlGroups::Sync(const FTCMatchState& State)
{
	// client/main.js: a lobby ends the match and forgets its groups (matchMemory.clear); a start empties them
	// (selection.reset) unless it resumes the same match (the same matchId), when they are restored.
	if (!State.bHasMatch)
	{
		if (bHeld) { Reset(); bHeld = false; HeldMatch = HeldSeat = -1; }
		return;
	}
	// The server always sends matchId (server.js sendStart); the seat is part of the key so a different seat in the
	// same match (a fresh token) starts empty.
	if (!bHeld || HeldMatch != State.MatchId || HeldSeat != State.You)
	{
		Reset();
		bHeld = true;
		HeldMatch = State.MatchId;
		HeldSeat = State.You;
	}
}

void FTCControlGroups::Prune(const FTCMatchState& State)
{
	Sync(State);
	if (!State.SeatConfirmed()) return; // nothing counts as ours yet: pruning now would empty every group
	const bool bHeldKnown = State.Snapshots > 0;
	for (TArray<int32>& G : Groups)
	{
		TSet<int32> Seen;
		TArray<int32> Kept;
		Kept.Reserve(G.Num());
		for (const int32 Id : G)
		{
			bool bDuplicate = false;
			Seen.Add(Id, &bDuplicate);
			if (bDuplicate) continue;
			if (const FTCUnit* U = State.Units.Find(Id))
			{
				if (CanBeMember(State, *U)) Kept.Add(Id);
			}
			else if (!bHeldKnown || State.UnreadableIds.Contains(Id))
			{
				Kept.Add(Id); // not known to be gone: no snapshot yet, or a held row this client could not read
			}
		}
		G = MoveTemp(Kept);
	}
}

const TArray<int32>& FTCControlGroups::Members(int32 Group) const
{
	static const TArray<int32> None;
	return IsGroupNumber(Group) ? Groups[Group - 1] : None;
}

FTCGroupResult FTCControlGroups::Apply(int32 Group, ETCGroupOp Op, const FTCMatchState& State, const FTCUnitTable& Table, TSet<int32>& Selected, double NowSeconds)
{
	FTCGroupResult R;
	R.Op = Op;
	R.Group = Group;
	Sync(State);
	if (Op == ETCGroupOp::None) { R.Note = TEXT("Control group: not a group chord"); return R; }
	if (!IsGroupNumber(Group)) { R.Note = FString::Printf(TEXT("Control group %d: only groups 1 to 9 exist"), Group); return R; }
	// client/main.js: a spectator selects nothing and keeps no groups (selection owner -1). An unconfirmed seat is
	// treated the same until its lobby confirms it, so a fresh reconnect cannot empty the groups it is about to keep.
	if (!State.SeatConfirmed()) { R.Note = FString::Printf(TEXT("Control group %d: no army to command"), Group); return R; }
	Prune(State);
	TArray<int32>& G = Groups[Group - 1];
	R.bApplied = true;

	if (Op == ETCGroupOp::Recall)
	{
		// client/selection.js group recall: replace the selection with the members that can be selected now
		Selected.Reset();
		FVector2D Sum = FVector2D::ZeroVector;
		for (const int32 Id : G)
		{
			const FTCUnit* U = State.Units.Find(Id);
			if (!U || !IsRecallable(State, *U, Table.Find(U->Type))) continue;
			bool bAlready = false;
			Selected.Add(Id, &bAlready);
			if (bAlready) continue;
			Sum += FVector2D(U->X, U->Z);
			R.Taken++;
		}
		R.Members = G.Num();
		// The reference compares whole milliseconds (now - lastRecall <= 300, inclusive); seconds in a double can land
		// a hair past 0.3 for an exact 300 ms gap, so allow a microsecond.
		if (R.Taken && LastGroup == Group && NowSeconds >= LastRecall && NowSeconds - LastRecall <= DoubleTapSeconds + 1e-6)
		{
			R.bCentre = true;
			R.Centre = Sum / (double)R.Taken;
		}
		LastGroup = Group;
		LastRecall = NowSeconds;
		R.Note = R.bCentre ? FString::Printf(TEXT("Group %d: %d units selected, centred"), Group, R.Taken)
			: R.Taken ? FString::Printf(TEXT("Group %d: %d units selected"), Group, R.Taken)
			: FString::Printf(TEXT("Group %d is empty"), Group);
		return R;
	}

	// client/selection.js group set, append and transfer: the selection's own living units, never a truck; transfer
	// takes only the army
	TArray<int32> Ids;
	for (const int32 Id : Selected)
	{
		const FTCUnit* U = State.Units.Find(Id);
		if (!U || !CanBeMember(State, *U)) continue;
		if (Op == ETCGroupOp::Transfer && !IsArmy(State, *U, Table.Find(U->Type))) continue;
		Ids.Add(Id);
	}
	R.Taken = Ids.Num();
	if (Op == ETCGroupOp::Transfer)
	{
		TSet<int32> Moving;
		for (const int32 Id : Ids) Moving.Add(Id);
		for (int32 I = 0; I < GroupCount; I++)
		{
			if (I != Group - 1) Groups[I].RemoveAll([&Moving](int32 Id) { return Moving.Contains(Id); });
		}
	}
	if (Op == ETCGroupOp::Set) G = MoveTemp(Ids);
	else for (const int32 Id : Ids) G.AddUnique(Id); // [...new Set([...groups[n], ...ids])]: the order of first joining
	R.Members = G.Num();
	// any change to a group forgets the double tap
	LastGroup = 0;
	LastRecall = TNumericLimits<double>::Lowest();
	switch (Op)
	{
	case ETCGroupOp::Set: R.Note = FString::Printf(TEXT("Group %d set: %d units"), Group, R.Members); break;
	case ETCGroupOp::Append: R.Note = FString::Printf(TEXT("Group %d: %d units added, %d in the group"), Group, R.Taken, R.Members); break;
	default: R.Note = FString::Printf(TEXT("Moved %d units to group %d; removed from other groups"), R.Taken, Group); break; // client/main.js
	}
	return R;
}
