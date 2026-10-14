#include "Client/TCKeys.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "InputCoreTypes.h"
#include "Containers/StringConv.h"

const TCHAR* LexToString(ETCKeyContext Context)
{
	switch (Context)
	{
	case ETCKeyContext::Army: return TEXT("army");
	case ETCKeyContext::Classic: return TEXT("classic");
	case ETCKeyContext::Targeting: return TEXT("targeting");
	case ETCKeyContext::Recruit: return TEXT("recruit");
	case ETCKeyContext::Building: return TEXT("building");
	default: return TEXT("global");
	}
}

namespace
{
	// shared/sim.js unitFlags bits the idle keys read beyond Net's TCFlags: digging (u.dig) and building (u.build)
	constexpr int32 DiggingFlag = 16, BuildingFlag = 128;
	const TCHAR* const TruckType = TEXT("truck");
	const TCHAR* const EngineerType = TEXT("engineer");

	TArray<FTCKeyBinding> BuildTable()
	{
		// client/keys.js bindings, built in the same order with the same loops
		TArray<FTCKeyBinding> B;
		auto Add = [&B](const FString& Id, const FString& Code, const FString& Label, const FString& Help, ETCKeyContext Context = ETCKeyContext::Global, bool bShift = false, bool bCtrl = false, bool bAlt = false)
		{
			FTCKeyBinding K;
			K.Id = Id; K.Code = Code; K.Label = Label; K.Help = Help; K.Context = Context;
			K.bShift = bShift; K.bCtrl = bCtrl; K.bAlt = bAlt;
			B.Add(MoveTemp(K));
		};
		const ETCKeyContext G = ETCKeyContext::Global;
		Add(TEXT("stop"), TEXT("KeyX"), TEXT("X"), TEXT("Stop selected units"));
		Add(TEXT("retreat"), TEXT("KeyR"), TEXT("R"), TEXT("Retreat selected units"));
		Add(TEXT("destroy"), TEXT("Delete"), TEXT("Del"), TEXT("Destroy the selected units and buildings (no refund)"));
		Add(TEXT("ability"), TEXT("KeyF"), TEXT("F"), TEXT("Use the first ready selected ability"));
		Add(TEXT("amove"), TEXT("KeyG"), TEXT("G"), TEXT("Aim an attack-move order"));
		Add(TEXT("area"), TEXT("KeyB"), TEXT("Shift+B"), TEXT("Shell an area: mortars, howitzers, rocket trucks, ships and bombers keep firing on a spot"), G, true);
		Add(TEXT("cover"), TEXT("KeyC"), TEXT("Shift+C"), TEXT("Selected infantry take cover"), G, true);
		Add(TEXT("stance:holdFire"), TEXT("KeyF"), TEXT("Shift+F"), TEXT("Toggle hold fire"), G, true);
		Add(TEXT("stance:holdPos"), TEXT("KeyG"), TEXT("Shift+G"), TEXT("Toggle hold position"), G, true);
		Add(TEXT("stance:autoRetreat"), TEXT("KeyX"), TEXT("Shift+X"), TEXT("Toggle auto-retreat"), G, true);
		Add(TEXT("formation"), TEXT("KeyV"), TEXT("Shift+V"), TEXT("Cycle the formation: line, block, column, wedge"), G, true);
		Add(TEXT("tighten"), TEXT("BracketLeft"), TEXT("["), TEXT("Tighten the formation"));
		Add(TEXT("spread"), TEXT("BracketRight"), TEXT("]"), TEXT("Spread the formation"));
		Add(TEXT("mute"), TEXT("KeyM"), TEXT("M"), TEXT("Toggle audio"));
		Add(TEXT("unload"), TEXT("KeyE"), TEXT("Shift+E"), TEXT("Unload the selected carriers: halftracks, trucks, boats and tanks with riders"), G, true);
		Add(TEXT("rail"), TEXT("KeyN"), TEXT("Shift+N"), TEXT("Send by rail (World Conquest): ground units near your station ride to your station nearest a click"), G, true);
		Add(TEXT("mountUp"), TEXT("KeyQ"), TEXT("Shift+Q"), TEXT("Mount up: selected infantry board the nearest selected truck, halftrack or tank"), G, true);
		Add(TEXT("alert"), TEXT("Space"), TEXT("Space"), TEXT("Jump to the newest alert, or center the selection"));
		Add(TEXT("alertHistory"), TEXT("KeyH"), TEXT("Alt+H"), TEXT("Open or close Alert history"), G, false, false, true);
		Add(TEXT("alertPrevious"), TEXT("ArrowLeft"), TEXT("Alt+Left"), TEXT("Visit the previous Alert"), G, false, false, true);
		Add(TEXT("alertNext"), TEXT("ArrowRight"), TEXT("Alt+Right"), TEXT("Visit the next Alert"), G, false, false, true);
		Add(TEXT("follow"), TEXT("Space"), TEXT("Shift+Space"), TEXT("Follow the selection"), G, true);
		Add(TEXT("rally"), TEXT("KeyH"), TEXT("Shift+H"), TEXT("Set the army rally point"), G, true);
		Add(TEXT("home"), TEXT("KeyH"), TEXT("H"), TEXT("Go home and select the Classic HQ"));
		Add(TEXT("clear"), TEXT("Escape"), TEXT("Esc"), TEXT("Clear the selection"), ETCKeyContext::Army);
		Add(TEXT("clear"), TEXT("Escape"), TEXT("Esc"), TEXT("Clear the selection"), ETCKeyContext::Classic);
		Add(TEXT("cancelAim"), TEXT("Escape"), TEXT("Esc"), TEXT("Cancel targeting"), ETCKeyContext::Targeting);
		Add(TEXT("army"), TEXT("KeyA"), TEXT("Ctrl+A"), TEXT("Select the whole army"), G, false, true);
		Add(TEXT("idle"), TEXT("Period"), TEXT("."), TEXT("Select and center the next idle ground unit"));
		Add(TEXT("idleAll"), TEXT("Period"), TEXT("Shift+."), TEXT("Select all idle ground units"), G, true);
		Add(TEXT("idleEngineer"), TEXT("Comma"), TEXT(","), TEXT("Select and center the next idle Engineer"));
		static const TCHAR* const Support[][2] = { { TEXT("recon"), TEXT("Z") }, { TEXT("artillery"), TEXT("C") }, { TEXT("strafe"), TEXT("V") }, { TEXT("smoke"), TEXT("B") },
			{ TEXT("bombing"), TEXT("N") }, { TEXT("dive"), TEXT("U") }, { TEXT("para"), TEXT("P") }, { TEXT("cover"), TEXT("I") } };
		for (const auto& S : Support) Add(FString(TEXT("support:")) + S[0], FString(TEXT("Key")) + S[1], S[1], FString::Printf(TEXT("Aim %s support"), S[0]));
		static const TCHAR* const Forts[][2] = { { TEXT("trench"), TEXT("T") }, { TEXT("sandbags"), TEXT("Y") }, { TEXT("wire"), TEXT("U") }, { TEXT("traps"), TEXT("I") },
			{ TEXT("nest"), TEXT("O") }, { TEXT("mines"), TEXT("J") }, { TEXT("bridge"), TEXT("K") }, { TEXT("fill"), TEXT("L") }, { TEXT("demine"), TEXT("M") } };
		for (const auto& F : Forts)
		{
			const bool bTrench = FCString::Strcmp(F[0], TEXT("trench")) == 0;
			Add(FString(TEXT("fort:")) + F[0], FString(TEXT("Key")) + F[1], bTrench ? FString(F[1]) : FString(TEXT("Shift+")) + F[1], FString::Printf(TEXT("Place %s"), F[0]), G, !bTrench);
		}
		Add(TEXT("entrench:line"), TEXT("KeyT"), TEXT("Shift+T"), TEXT("All selected builders dig a trench line"), G, true);
		static const TCHAR* const Builds[][2] = { { TEXT("depot"), TEXT("J") }, { TEXT("barracks"), TEXT("K") }, { TEXT("motorpool"), TEXT("L") }, { TEXT("airfield"), TEXT("O") }, { TEXT("flakpos"), TEXT("Y") } };
		for (const auto& K : Builds) Add(FString(TEXT("build:")) + K[0], FString(TEXT("Key")) + K[1], K[1], FString::Printf(TEXT("Place a %s"), K[0]), ETCKeyContext::Classic);
		static const TCHAR* const Camera[][2] = { { TEXT("KeyW"), TEXT("W") }, { TEXT("ArrowUp"), TEXT("Up") }, { TEXT("KeyS"), TEXT("S") }, { TEXT("ArrowDown"), TEXT("Down") },
			{ TEXT("KeyA"), TEXT("A") }, { TEXT("ArrowLeft"), TEXT("Left") }, { TEXT("KeyD"), TEXT("D") }, { TEXT("ArrowRight"), TEXT("Right") }, { TEXT("KeyQ"), TEXT("Q") }, { TEXT("KeyE"), TEXT("E") } };
		for (const auto& C : Camera)
		{
			const FString Code = C[0];
			const bool bTurn = Code == TEXT("KeyQ") || Code == TEXT("KeyE");
			const TCHAR* Id = Code == TEXT("KeyQ") ? TEXT("rotateLeft") : Code == TEXT("KeyE") ? TEXT("rotateRight")
				: (Code == TEXT("KeyW") || Code == TEXT("ArrowUp")) ? TEXT("panForward") : (Code == TEXT("KeyS") || Code == TEXT("ArrowDown")) ? TEXT("panBack")
				: (Code == TEXT("KeyA") || Code == TEXT("ArrowLeft")) ? TEXT("panLeft") : TEXT("panRight");
			Add(Id, Code, C[1], bTurn ? TEXT("Rotate the camera") : TEXT("Pan the camera"));
		}
		for (int32 N = 1; N <= 9; N++)
		{
			const FString Code = FString::Printf(TEXT("Digit%d"), N);
			Add(FString::Printf(TEXT("group:recall:%d"), N), Code, FString::Printf(TEXT("%d"), N), FString::Printf(TEXT("Recall group %d; tap twice to center"), N));
			Add(FString::Printf(TEXT("group:transfer:%d"), N), Code, FString::Printf(TEXT("Alt+%d"), N), FString::Printf(TEXT("Move selected units to group %d and remove them from other groups"), N), G, false, false, true);
			Add(FString::Printf(TEXT("group:set:%d"), N), Code, FString::Printf(TEXT("Ctrl+%d"), N), FString::Printf(TEXT("Set group %d"), N), G, false, true);
			Add(FString::Printf(TEXT("group:append:%d"), N), Code, FString::Printf(TEXT("Shift+%d"), N), FString::Printf(TEXT("Append selection to group %d"), N), G, true);
			Add(FString::Printf(TEXT("group:append:%d"), N), Code, FString::Printf(TEXT("Ctrl+Shift+%d"), N), FString::Printf(TEXT("Append selection to group %d"), N), G, true, true);
		}
		Add(TEXT("recruitMode"), TEXT("Tab"), TEXT("Tab"), TEXT("Recruit by letter: turn the Command Card letters on or off"), ETCKeyContext::Army);
		Add(TEXT("recruitMode"), TEXT("Backquote"), TEXT("`"), TEXT("Recruit by letter: turn the Command Card letters on or off"), ETCKeyContext::Army);
		Add(TEXT("recruitMode"), TEXT("Tab"), TEXT("Tab"), TEXT("Explain how Classic trains by letter"), ETCKeyContext::Classic);
		Add(TEXT("recruitOff"), TEXT("Escape"), TEXT("Esc"), TEXT("Leave recruit mode"), ETCKeyContext::Recruit);
		static const FString Letters = TEXT("QWERTASDFGZXCVB");
		for (const ETCKeyContext Context : { ETCKeyContext::Recruit, ETCKeyContext::Building })
			for (int32 I = 0; I < Letters.Len(); I++)
			{
				const FString Key = Letters.Mid(I, 1);
				Add(FString::Printf(TEXT("card:%d"), I + 1), TEXT("Key") + Key, Key, FString::Printf(TEXT("Buy the unit on card %d"), I + 1), Context);
				Add(FString::Printf(TEXT("cardMany:%d"), I + 1), TEXT("Key") + Key, TEXT("Shift+") + Key, FString::Printf(TEXT("Buy five of the unit on card %d"), I + 1), Context, true);
			}
		return B;
	}

	struct FCodeKey { FKey Key; FString Code; };

	const TArray<FCodeKey>& CodeKeys()
	{
		// the DOM codes keys.js names (and F1, F2, which client/main.js reads before the table), as the engine names them
		static const TArray<FCodeKey> Pairs = []()
		{
			TArray<FCodeKey> P;
			for (TCHAR C = TEXT('A'); C <= TEXT('Z'); C++) P.Add({ FKey(FName(*FString::Chr(C))), FString(TEXT("Key")) + FString::Chr(C) });
			const FKey* const Digits[] = { &EKeys::Zero, &EKeys::One, &EKeys::Two, &EKeys::Three, &EKeys::Four, &EKeys::Five, &EKeys::Six, &EKeys::Seven, &EKeys::Eight, &EKeys::Nine };
			for (int32 D = 0; D < UE_ARRAY_COUNT(Digits); D++) P.Add({ *Digits[D], FString::Printf(TEXT("Digit%d"), D) });
			P.Add({ EKeys::SpaceBar, TEXT("Space") });
			P.Add({ EKeys::Escape, TEXT("Escape") });
			P.Add({ EKeys::Delete, TEXT("Delete") });
			P.Add({ EKeys::Tab, TEXT("Tab") });
			P.Add({ EKeys::Tilde, TEXT("Backquote") });
			P.Add({ EKeys::Period, TEXT("Period") });
			P.Add({ EKeys::Comma, TEXT("Comma") });
			P.Add({ EKeys::Semicolon, TEXT("Semicolon") });
			P.Add({ EKeys::LeftBracket, TEXT("BracketLeft") });
			P.Add({ EKeys::RightBracket, TEXT("BracketRight") });
			P.Add({ EKeys::Up, TEXT("ArrowUp") });
			P.Add({ EKeys::Down, TEXT("ArrowDown") });
			P.Add({ EKeys::Left, TEXT("ArrowLeft") });
			P.Add({ EKeys::Right, TEXT("ArrowRight") });
			P.Add({ EKeys::F1, TEXT("F1") });
			P.Add({ EKeys::F2, TEXT("F2") });
			return P;
		}();
		return Pairs;
	}

	TArray<FTCSheetSection> BuildSheet()
	{
		// client/index.html #controlsSheet, section by section and line by line (kbd runs written as text)
		TArray<FTCSheetSection> S;
		auto Section = [&S](const TCHAR* Title) -> TArray<FTCSheetRow>& { FTCSheetSection& X = S.AddDefaulted_GetRef(); X.Title = Title; return X.Rows; };
		{
			TArray<FTCSheetRow>& R = Section(TEXT("Mouse"));
			R.Add({ TEXT("Left click"), TEXT("Select a unit or building") });
			R.Add({ TEXT("Drag"), TEXT("Select units in a box") });
			R.Add({ TEXT("Shift + click / drag"), TEXT("Add to or remove from your selection") });
			R.Add({ TEXT("Double-click"), TEXT("Select nearby units of the same type; Ctrl/Command includes the whole map") });
			R.Add({ TEXT("Right-click"), TEXT("Move, attack, garrison, or interact with the target") });
			R.Add({ TEXT("Shift + right-click"), TEXT("Queue the order") });
			R.Add({ TEXT("Ctrl + right-click"), TEXT("Attack-move, fighting anything met along the way") });
			R.Add({ TEXT("Right-click-drag"), TEXT("Set a destination and facing for the formation; release to confirm") });
			R.Add({ TEXT("Wheel"), TEXT("Zoom at the cursor") });
			R.Add({ TEXT("Middle-drag"), TEXT("Rotate the camera") });
			R.Add({ TEXT("Map edge"), TEXT("Move the cursor to an edge to scroll") });
			R.Add({ TEXT("Minimap"), TEXT("Left-drag to move the camera; right-click to order") });
			R.Add({ TEXT("Alt + left-click"), TEXT("Ping a spot for your team") });
			R.Add({ TEXT("While aiming"), TEXT("Click to place or call in; move to aim direction. Right-click cancels") });
		}
		{
			TArray<FTCSheetRow>& R = Section(TEXT("Camera"));
			R.Add({ TEXT("W A S D / arrows"), TEXT("Pan") });
			R.Add({ TEXT("Q / E"), TEXT("Rotate left / right") });
			R.Add({ TEXT("F1"), TEXT("Open or close this sheet") });
			R.Add({ TEXT("F2"), TEXT("Show or hide the stats overlay (pick the numbers in the menu)") });
			R.Add({ TEXT("M"), TEXT("Mute or unmute audio") });
		}
		{
			TArray<FTCSheetRow>& R = Section(TEXT("Units and orders"));
			R.Add({ TEXT("X / R"), TEXT("Stop / retreat selected units") });
			R.Add({ TEXT("F / G"), TEXT("Use a ready ability / aim attack-move") });
			R.Add({ TEXT("Shift+E"), TEXT("Unload the selected halftrack") });
			R.Add({ TEXT("Shift+C"), TEXT("Take cover") });
			R.Add({ TEXT("Shift+F, Shift+G, Shift+X"), TEXT("Toggle hold fire, hold position, auto-retreat") });
			R.Add({ TEXT("Ctrl+A"), TEXT("Select your whole army") });
			R.Add({ TEXT(". / Shift+."), TEXT("Select next idle unit / all idle units") });
			R.Add({ TEXT(","), TEXT("Select the next idle Engineer") });
			R.Add({ TEXT("Space / Shift+Space"), TEXT("Jump to newest alert or selection / follow selection") });
			R.Add({ TEXT("1-9"), TEXT("Recall a control group; tap twice to center") });
			R.Add({ TEXT("Ctrl+1-Ctrl+9"), TEXT("Set a control group; Shift adds to it") });
			R.Add({ TEXT("Alt+1-Alt+9"), TEXT("Move selected units to a control group and remove them from other groups") });
			R.Add({ TEXT("Alt+H"), TEXT("Open or close Alert history") });
			R.Add({ TEXT("Alt+Left / Alt+Right"), TEXT("Visit the previous or next Alert at its recorded location") });
			R.Add({ TEXT("H / Shift+H"), TEXT("Go home in Classic / set army rally point") });
			R.Add({ TEXT("Esc"), TEXT("Clear selection, cancel aiming, or release captured mouse") });
		}
		{
			TArray<FTCSheetRow>& R = Section(TEXT("Recruiting and building"));
			R.Add({ TEXT("Tab"), TEXT("Turn recruit letters on or off") });
			R.Add({ TEXT("QWERTASDFGZXCVB"), TEXT("Choose a card while recruiting; Shift + letter buys up to five") });
			R.Add({ TEXT("J, K, L, O, Y"), TEXT("Place Depot, Barracks, Motorpool, Airfield, Flak position in Classic") });
			R.Add({ TEXT("T / Shift+T"), TEXT("Place a trench / dig a trench line") });
			R.Add({ TEXT("Shift+Y/U/I/O/J/K/L/M"), TEXT("Place sandbags, wire, traps, nest, mines, bridge, fill ground, or demine") });
			R.Add({ TEXT("Field Hospital"), TEXT("Build from a builder squad's orders panel") });
			R.Add({ TEXT("Support keys"), TEXT("Z Recon, C Artillery, V Strafe, B Smoke, N Bombing, U Dive, P Paratroops, I Air cover") });
		}
		return S;
	}
}

const TArray<FTCKeyBinding>& TCKeys::Bindings()
{
	static const TArray<FTCKeyBinding> Table = BuildTable();
	return Table;
}

int32 TCKeys::Rank(ETCKeyContext Context)
{
	return Context == ETCKeyContext::Targeting ? 2 : Context == ETCKeyContext::Recruit || Context == ETCKeyContext::Building ? 1 : 0;
}

const FTCKeyBinding* TCKeys::Match(const FString& Code, bool bShift, bool bCtrl, bool bAlt, TConstArrayView<ETCKeyContext> Active)
{
	const FTCKeyBinding* Best = nullptr;
	for (const FTCKeyBinding& B : Bindings())
	{
		if (B.Code != Code || B.bShift != bShift || B.bCtrl != bCtrl || B.bAlt != bAlt) continue;
		if (B.Context != ETCKeyContext::Global && !Active.Contains(B.Context)) continue;
		if (!Best || Rank(B.Context) > Rank(Best->Context)) Best = &B;
	}
	return Best;
}

FString TCKeys::Label(const FString& Id)
{
	for (const FTCKeyBinding& B : Bindings()) if (B.Id == Id) return B.Label;
	return FString();
}

FString TCKeys::Badge(const FString& Id)
{
	const FString L = Label(Id);
	return L.StartsWith(TEXT("Shift+"), ESearchCase::CaseSensitive) ? FString(TEXT("⇧")) + L.Mid(6) : L;
}

FString TCKeys::CodeOf(const FKey& Key)
{
	for (const FCodeKey& P : CodeKeys()) if (P.Key == Key) return P.Code;
	return FString();
}

FKey TCKeys::KeyOf(const FString& Code)
{
	for (const FCodeKey& P : CodeKeys()) if (P.Code == Code) return P.Key;
	return EKeys::Invalid;
}

const TArray<FKey>& TCKeys::PolledKeys()
{
	static const TArray<FKey> Keys = []()
	{
		TArray<FKey> K;
		for (const FTCKeyBinding& B : Bindings())
		{
			const FKey Key = KeyOf(B.Code);
			if (Key.IsValid()) K.AddUnique(Key);
		}
		K.AddUnique(EKeys::F1);
		K.AddUnique(EKeys::F2);
		return K;
	}();
	return Keys;
}

const TArray<FTCSheetSection>& TCKeys::ControlsSheet()
{
	static const TArray<FTCSheetSection> Sheet = BuildSheet();
	return Sheet;
}

FString TCKeys::TableText()
{
	FString Out;
	for (const FTCKeyBinding& B : Bindings())
		Out += FString::Printf(TEXT("%s|%s|%s|%s|%s|%d%d%d\n"), *B.Id, *B.Code, *B.Label, *B.Help, LexToString(B.Context), B.bShift ? 1 : 0, B.bCtrl ? 1 : 0, B.bAlt ? 1 : 0);
	return Out;
}

uint32 TCKeys::Fnv1a(const FString& Text)
{
	const FTCHARToUTF8 Utf8(*Text);
	uint32 H = 0x811c9dc5u;
	for (int32 I = 0; I < Utf8.Length(); I++)
	{
		H ^= (uint8)Utf8.Get()[I];
		H *= 0x01000193u;
	}
	return H;
}

bool TCSelectionKeys::Own(const FTCKeyUnit& U) { return U.bOwn && U.Hp > 0; }

bool TCSelectionKeys::MobileOwn(const FTCKeyUnit& U)
{
	return Own(U) && U.bDef && !U.bStructure && !(U.Flags & (TCClientFlags::PlaneAtBase | TCFlags::Riding));
}

bool TCSelectionKeys::ArmyUnit(const FTCKeyUnit& U)
{
	return Own(U) && U.bDef && !U.bStructure && !U.bRail && U.Type != TruckType;
}

bool TCSelectionKeys::Selectable(const FTCKeyUnit& U) { return ArmyUnit(U) && MobileOwn(U); }

bool TCSelectionKeys::SelectableOwn(const FTCKeyUnit& U) { return Own(U) && U.bDef && (MobileOwn(U) || U.bBuilding); }

TArray<int32> TCSelectionKeys::Army(TConstArrayView<FTCKeyUnit> Units)
{
	TArray<int32> Out;
	for (const FTCKeyUnit& U : Units) if (Selectable(U)) Out.Add(U.Id);
	return Out;
}

TArray<int32> TCSelectionKeys::Idle(TConstArrayView<FTCKeyUnit> Units, bool bEngineers)
{
	constexpr int32 Busy = TCFlags::Retreating | DiggingFlag | TCFlags::Garrisoned | BuildingFlag | TCFlags::Riding;
	TArray<int32> Out;
	for (const FTCKeyUnit& U : Units)
		if (ArmyUnit(U) && !U.bAir && (!bEngineers || U.Type == EngineerType) && !(U.Flags & Busy) && U.PlanKind <= 0) Out.Add(U.Id);
	Out.Sort();
	return Out;
}

bool TCSelectionKeys::DoubleClick(TConstArrayView<FTCKeyUnit> Units, int32 Picked, bool bMapWide, TArray<int32>& Out)
{
	const FTCKeyUnit* V = Units.FindByPredicate([Picked](const FTCKeyUnit& U) { return U.Id == Picked; });
	if (!V || !SelectableOwn(*V)) return false;
	Out.Reset();
	for (const FTCKeyUnit& U : Units) if (SelectableOwn(U) && U.Type == V->Type && (bMapWide || U.bOnScreen)) Out.Add(U.Id);
	return true;
}

bool TCSelectionKeys::Centre(TConstArrayView<FTCKeyUnit> Units, const TArray<int32>& Ids, FVector2D& Out)
{
	FVector2D Sum = FVector2D::ZeroVector;
	int32 N = 0;
	for (const FTCKeyUnit& U : Units) if (Ids.Contains(U.Id)) { Sum += U.At; N++; }
	if (!N) return false;
	Out = Sum / N;
	return true;
}

FTCIdleResult FTCIdleCursor::Find(TConstArrayView<FTCKeyUnit> Units, bool bAll, bool bEngineers)
{
	FTCIdleResult R;
	const TArray<int32> List = TCSelectionKeys::Idle(Units, bEngineers);
	if (bAll) { R.bReplace = true; R.Ids = List; return R; }
	if (!List.Num()) return R;
	int32& Cursor = bEngineers ? Engineers : Army;
	Cursor = (Cursor + 1) % List.Num();
	R.bReplace = R.bCentre = true;
	R.Ids.Add(List[Cursor]);
	return R;
}

bool FTCDoubleClick::Click(double PressTime, FVector2D At)
{
	if (bHas && PressTime - Time <= Seconds && PressTime >= Time && FMath::Abs(At.X - Where.X) <= Pixels && FMath::Abs(At.Y - Where.Y) <= Pixels)
	{
		bHas = false;
		return true;
	}
	bHas = true;
	Time = PressTime;
	Where = At;
	return false;
}

#if WITH_DEV_AUTOMATION_TESTS

#include "Client/TCArmyCommands.h"
#include "Client/TCAbilities.h"
#include "Client/TCSupportOrders.h"
#include "Client/TCSupportPowers.h"
#include "Client/TCFieldworks.h"
#include "Client/TCConstruction.h"
#include "Client/TCControlGroups.h"
#include "Misc/AutomationTest.h"

// ThreeCrossroads.Keys.Table: the table equals client/keys.js at the pin, and match() answers as keys.js does for every
// code it names (and one it does not) with every modifier set in seven context sets. The expected hashes come from
// running keys.js itself under Node at e4e579a: FNV-1a 32 of the same lines, built the same way. Run:
//   UnrealEditor-Cmd ThreeCrossroads.uproject -nullrhi -ExecCmds="Automation RunTests ThreeCrossroads.Keys; Quit"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCKeysTableTest, "ThreeCrossroads.Keys.Table", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCKeysTableTest::RunTest(const FString& Parameters)
{
	TestEqual(TEXT("keys.js has 173 bindings at the pin"), TCKeys::Bindings().Num(), 173);
	const FString Text = TCKeys::TableText();
	TestEqual(TEXT("the table text has the length keys.js gives"), Text.Len(), 10868);
	const uint32 TableHash = TCKeys::Fnv1a(Text);
	TestTrue(FString::Printf(TEXT("the table equals keys.js (FNV-1a 0x%08x, expected 0x6f3e0f72)"), TableHash), TableHash == 0x6f3e0f72u);

	static const TCHAR* const Codes[] = { TEXT("ArrowDown"), TEXT("ArrowLeft"), TEXT("ArrowRight"), TEXT("ArrowUp"), TEXT("Backquote"), TEXT("BracketLeft"), TEXT("BracketRight"), TEXT("Comma"),
		TEXT("Delete"), TEXT("Digit1"), TEXT("Digit2"), TEXT("Digit3"), TEXT("Digit4"), TEXT("Digit5"), TEXT("Digit6"), TEXT("Digit7"), TEXT("Digit8"), TEXT("Digit9"), TEXT("Escape"),
		TEXT("KeyA"), TEXT("KeyB"), TEXT("KeyC"), TEXT("KeyD"), TEXT("KeyE"), TEXT("KeyF"), TEXT("KeyG"), TEXT("KeyH"), TEXT("KeyI"), TEXT("KeyJ"), TEXT("KeyK"), TEXT("KeyL"), TEXT("KeyM"),
		TEXT("KeyN"), TEXT("KeyO"), TEXT("KeyP"), TEXT("KeyQ"), TEXT("KeyR"), TEXT("KeyS"), TEXT("KeyT"), TEXT("KeyU"), TEXT("KeyV"), TEXT("KeyW"), TEXT("KeyX"), TEXT("KeyY"), TEXT("KeyZ"),
		TEXT("Period"), TEXT("Semicolon"), TEXT("Space"), TEXT("Tab") };
	using C = ETCKeyContext;
	const TArray<TArray<ETCKeyContext>> Sets = { { C::Army }, { C::Classic }, { C::Army, C::Targeting }, { C::Classic, C::Building }, { C::Army, C::Recruit },
		{ C::Classic, C::Building, C::Targeting }, { C::Army, C::Recruit, C::Targeting } };
	FString Lines;
	int32 Cases = 0, Hits = 0;
	for (const TCHAR* Code : Codes)
		for (int32 M = 0; M < 8; M++)
			for (const TArray<ETCKeyContext>& Set : Sets)
			{
				TArray<FString> Names;
				for (const ETCKeyContext X : Set) Names.Add(LexToString(X));
				const FTCKeyBinding* B = TCKeys::Match(Code, (M & 1) != 0, (M & 2) != 0, (M & 4) != 0, Set);
				Lines += FString::Printf(TEXT("%s|%d%d%d|%s=%s\n"), Code, M & 1 ? 1 : 0, M & 2 ? 1 : 0, M & 4 ? 1 : 0, *FString::Join(Names, TEXT(",")), B ? *B->Id : TEXT(""));
				Cases++;
				Hits += B ? 1 : 0;
			}
	TestEqual(TEXT("match cases"), Cases, 2744);
	TestEqual(TEXT("cases keys.js binds"), Hits, 764);
	const uint32 MatchHash = TCKeys::Fnv1a(Lines);
	TestTrue(FString::Printf(TEXT("match() answers as keys.js does (FNV-1a 0x%08x, expected 0xbd1cbe49)"), MatchHash), MatchHash == 0xbd1cbe49u);

	// the same, by hand, for the chords this gap adds and the ranks
	auto Id = [](const TCHAR* Code, bool bS, bool bC, bool bA, TArray<ETCKeyContext> Set) { const FTCKeyBinding* B = TCKeys::Match(Code, bS, bC, bA, Set); return B ? B->Id : FString(); };
	TestEqual(TEXT("Esc cancels the aim first"), Id(TEXT("Escape"), false, false, false, { C::Army, C::Targeting }), FString(TEXT("cancelAim")));
	TestEqual(TEXT("Esc clears"), Id(TEXT("Escape"), false, false, false, { C::Classic }), FString(TEXT("clear")));
	TestEqual(TEXT("Shift+E buys card 3's five in a building's card"), Id(TEXT("KeyE"), true, false, false, { C::Classic, C::Building }), FString(TEXT("cardMany:3")));
	TestEqual(TEXT("Shift+E unloads otherwise"), Id(TEXT("KeyE"), true, false, false, { C::Classic }), FString(TEXT("unload")));
	TestEqual(TEXT("Ctrl+A"), Id(TEXT("KeyA"), false, true, false, { C::Army }), FString(TEXT("army")));
	TestEqual(TEXT("J outside Classic is nothing"), Id(TEXT("KeyJ"), false, false, false, { C::Army }), FString());
	TestEqual(TEXT("Alt+H"), Id(TEXT("KeyH"), false, false, true, { C::Army }), FString(TEXT("alertHistory")));
	TestEqual(TEXT("H"), Id(TEXT("KeyH"), false, false, false, { C::Army }), FString(TEXT("home")));
	TestEqual(TEXT("Space"), Id(TEXT("Space"), false, false, false, { C::Army }), FString(TEXT("alert")));
	TestEqual(TEXT("Shift+Space"), Id(TEXT("Space"), true, false, false, { C::Army }), FString(TEXT("follow")));
	TestEqual(TEXT("."), Id(TEXT("Period"), false, false, false, { C::Army }), FString(TEXT("idle")));
	TestEqual(TEXT("Shift+."), Id(TEXT("Period"), true, false, false, { C::Army }), FString(TEXT("idleAll")));
	TestEqual(TEXT(","), Id(TEXT("Comma"), false, false, false, { C::Army }), FString(TEXT("idleEngineer")));
	TestEqual(TEXT("Delete"), Id(TEXT("Delete"), false, false, false, { C::Army }), FString(TEXT("destroy")));
	TestEqual(TEXT("Shift+C"), Id(TEXT("KeyC"), true, false, false, { C::Army }), FString(TEXT("cover")));
	TestEqual(TEXT("Ctrl+Shift+C is nothing"), Id(TEXT("KeyC"), true, true, false, { C::Army }), FString());
	TestEqual(TEXT("Shift+N"), Id(TEXT("KeyN"), true, false, false, { C::Army }), FString(TEXT("rail")));
	TestEqual(TEXT("labels"), TCKeys::Label(TEXT("area")), FString(TEXT("Shift+B")));
	TestEqual(TEXT("badges draw Shift as the arrow"), TCKeys::Badge(TEXT("area")), FString(TEXT("⇧B")));
	TestEqual(TEXT("a badge without Shift"), TCKeys::Badge(TEXT("destroy")), FString(TEXT("Del")));

	// every code keys.js names maps to one engine key and back; the polled set holds them and F1, F2
	for (const FTCKeyBinding& B : TCKeys::Bindings())
	{
		const FKey K = TCKeys::KeyOf(B.Code);
		TestTrue(FString::Printf(TEXT("%s has an engine key"), *B.Code), K.IsValid());
		TestEqual(FString::Printf(TEXT("%s round trip"), *B.Code), TCKeys::CodeOf(K), B.Code);
		TestTrue(FString::Printf(TEXT("%s is polled"), *B.Code), TCKeys::PolledKeys().Contains(K));
	}
	TestEqual(TEXT("the backquote is the engine's Tilde key"), TCKeys::CodeOf(EKeys::Tilde), FString(TEXT("Backquote")));
	TestTrue(TEXT("F1 is polled"), TCKeys::PolledKeys().Contains(EKeys::F1));

	// client/index.html #controlsSheet: four sections of 14, 5, 16 and 7 lines
	const TArray<FTCSheetSection>& Sheet = TCKeys::ControlsSheet();
	TestEqual(TEXT("sheet sections"), Sheet.Num(), 4);
	if (Sheet.Num() == 4)
	{
		TestEqual(TEXT("Mouse"), Sheet[0].Rows.Num(), 14);
		TestEqual(TEXT("Camera"), Sheet[1].Rows.Num(), 5);
		TestEqual(TEXT("Units and orders"), Sheet[2].Rows.Num(), 16);
		TestEqual(TEXT("Recruiting and building"), Sheet[3].Rows.Num(), 7);
		TestEqual(TEXT("the wheel line"), Sheet[0].Rows[8].Text, FString(TEXT("Zoom at the cursor")));
	}
	return true;
}

// ThreeCrossroads.Keys.Modules: the chords this controller used to match through each module's own key function
// (stances, retreat, rally, ability, carriers, support calls, area fire, field works, Classic builds, control groups)
// give the same action as the table, for every polled key and every modifier set, so dispatching by the table changes
// none of them.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCKeysModulesTest, "ThreeCrossroads.Keys.Modules", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCKeysModulesTest::RunTest(const FString& Parameters)
{
	const TArray<ETCKeyContext> Army = { ETCKeyContext::Army }, Classic = { ETCKeyContext::Classic };
	auto Family = [](const FString& Id)
	{
		return Id == TEXT("retreat") || Id == TEXT("rally") || Id == TEXT("ability") || Id == TEXT("unload") || Id == TEXT("mountUp") || Id == TEXT("area")
			|| Id.StartsWith(TEXT("stance:")) || Id.StartsWith(TEXT("support:")) || Id.StartsWith(TEXT("fort:")) || Id.StartsWith(TEXT("entrench:"));
	};
	int32 Checked = 0;
	for (const FKey& K : TCKeys::PolledKeys())
		for (int32 M = 0; M < 8; M++)
		{
			const bool bS = (M & 1) != 0, bC = (M & 2) != 0, bA = (M & 4) != 0;
			const FString Code = TCKeys::CodeOf(K), Name = K.GetFName().ToString();
			const FString Chord = FString::Printf(TEXT("%s%s%s%s"), bC ? TEXT("Ctrl+") : TEXT(""), bS ? TEXT("Shift+") : TEXT(""), bA ? TEXT("Alt+") : TEXT(""), *Code);
			FString Module;
			switch (TCArmyCommands::KeyAction(K, bS, bC, bA))
			{
			case ETCArmyKey::Retreat: Module = TEXT("retreat"); break;
			case ETCArmyKey::HoldFire: Module = TEXT("stance:holdFire"); break;
			case ETCArmyKey::HoldPos: Module = TEXT("stance:holdPos"); break;
			case ETCArmyKey::AutoRetreat: Module = TEXT("stance:autoRetreat"); break;
			case ETCArmyKey::Rally: Module = TEXT("rally"); break;
			default: break;
			}
			if (FTCAbilities::KeyAction(K, bS, bC, bA) == ETCAbilityKey::Use) Module = TEXT("ability");
			const ETCSupportKey Carrier = FTCSupportOrders::KeyAction(K, bS, bC, bA);
			if (Carrier == ETCSupportKey::Unload) Module = TEXT("unload"); else if (Carrier == ETCSupportKey::MountUp) Module = TEXT("mountUp");
			const FTCSupportKeyMatch Power = FTCSupportPowers::KeyAction(K, bS, bC, bA);
			if (Power.Action == ETCSupportPowerKey::Support) Module = TEXT("support:") + Power.Kind; else if (Power.Action == ETCSupportPowerKey::Area) Module = TEXT("area");
			const FString Field = FTCFieldworks::ActionForChord(Name, bS, bC, bA);
			if (!Field.IsEmpty()) Module = Field;
			const FTCKeyBinding* B = TCKeys::Match(Code, bS, bC, bA, Army);
			const FString Table = B ? B->Id : FString();
			if (!Module.IsEmpty() || Family(Table)) { TestEqual(FString::Printf(TEXT("%s: the module and the table agree"), *Chord), Table, Module); Checked++; }
			// the Classic build letters
			const FString Kind = !bS && !bC && !bA ? FTCConstruction::KindForKey(Name) : FString();
			const FTCKeyBinding* BC = TCKeys::Match(Code, bS, bC, bA, Classic);
			const FString ClassicId = BC ? BC->Id : FString();
			if (!Kind.IsEmpty() || ClassicId.StartsWith(TEXT("build:"))) { TestEqual(FString::Printf(TEXT("%s in Classic"), *Chord), ClassicId, Kind.IsEmpty() ? FString() : TEXT("build:") + Kind); Checked++; }
			// the digits: FTCControlGroups reads the same chords
			const int32 Digit = FTCControlGroups::DigitOf(K);
			if (Digit)
			{
				const ETCGroupOp Op = FTCControlGroups::OpForChord(bC, bS, bA);
				const FString Want = Op == ETCGroupOp::None ? FString() : FString::Printf(TEXT("group:%s:%d"), LexToString(Op), Digit);
				TestEqual(FString::Printf(TEXT("%s group chord"), *Chord), Table, Want);
				Checked++;
			}
		}
	TestTrue(FString::Printf(TEXT("chords compared: %d"), Checked), Checked > 100);
	return true;
}

// ThreeCrossroads.Keys.Selection: client/selection.js army, idle, findIdle and doubleClick on a recorded set of rows
// covering each eligibility rule, and the double-click timing.
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCKeysSelectionTest, "ThreeCrossroads.Keys.Selection", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCKeysSelectionTest::RunTest(const FString& Parameters)
{
	auto Row = [](int32 Id, const TCHAR* Type, bool bOwn, int32 Hp, int32 Flags, int32 Plan, bool bOnScreen, bool bStructure = false, bool bBuilding = false, bool bRail = false, bool bAir = false)
	{
		FTCKeyUnit U;
		U.Id = Id; U.Type = Type; U.bOwn = bOwn; U.Hp = Hp; U.Flags = Flags; U.PlanKind = Plan; U.bOnScreen = bOnScreen;
		U.bDef = true; U.bStructure = bStructure; U.bBuilding = bBuilding; U.bRail = bRail; U.bAir = bAir;
		U.At = FVector2D(Id * 10.0, Id * 2.0);
		return U;
	};
	const TArray<FTCKeyUnit> Units = {
		Row(14, TEXT("rifle"), true, 100, 0, -1, false),            // idle, off screen
		Row(1, TEXT("rifle"), true, 100, 0, -1, true),              // idle (no plan row)
		Row(2, TEXT("rifle"), true, 100, 1, -1, true),              // retreating
		Row(3, TEXT("engineer"), true, 100, 0, 0, true),            // plan kind 0: idle
		Row(4, TEXT("engineer"), true, 100, 0, 1, true),            // moving
		Row(5, TEXT("truck"), true, 100, 0, -1, true),              // a supply truck: never the army
		Row(6, TEXT("fighter"), true, 100, 0, -1, true, false, false, false, true), // flying: army, never idle
		Row(7, TEXT("fighter"), true, 100, 512, -1, false, false, false, false, true), // parked at its airfield
		Row(8, TEXT("rifle"), true, 100, 262144, -1, false),        // riding
		Row(9, TEXT("rifle"), true, 100, 32, -1, false),            // garrisoned: not drawn here
		Row(10, TEXT("train"), true, 100, 0, -1, true, false, false, true), // trains run themselves
		Row(11, TEXT("hq"), true, 100, 0, -1, true, true, true),    // an own building
		Row(12, TEXT("rifle"), false, 100, 0, -1, true),            // someone else's
		Row(13, TEXT("rifle"), true, 0, 0, -1, true),               // dead
		Row(15, TEXT("bunker"), true, 100, 0, -1, true, true),      // a structure that is not a building
		Row(16, TEXT("rifle"), true, 100, 16, -1, true),            // digging
		Row(17, TEXT("engineer"), true, 100, 128, -1, true),        // building
	};
	TestTrue(TEXT("Ctrl+A: the army, in row order"), TCSelectionKeys::Army(Units) == TArray<int32>({ 14, 1, 2, 3, 4, 6, 9, 16, 17 }));
	TestTrue(TEXT("idle ground units, by id"), TCSelectionKeys::Idle(Units, false) == TArray<int32>({ 1, 3, 14 }));
	TestTrue(TEXT("idle Engineers"), TCSelectionKeys::Idle(Units, true) == TArray<int32>({ 3 }));

	FTCIdleCursor Cursor;
	const int32 Walk[] = { 1, 3, 14, 1 };
	for (const int32 Want : Walk)
	{
		const FTCIdleResult R = Cursor.Find(Units, false, false);
		TestTrue(FString::Printf(TEXT("'.' walks to %d and centres"), Want), R.bReplace && R.bCentre && R.Ids == TArray<int32>({ Want }));
	}
	const FTCIdleResult Eng = Cursor.Find(Units, false, true);
	TestTrue(TEXT("',' has its own cursor"), Eng.Ids == TArray<int32>({ 3 }));
	const FTCIdleResult All = Cursor.Find(Units, true, false);
	TestTrue(TEXT("Shift+'.' takes all, without centring"), All.bReplace && !All.bCentre && All.Ids == TArray<int32>({ 1, 3, 14 }));
	const TArray<FTCKeyUnit> Busy = { Row(2, TEXT("rifle"), true, 100, 1, -1, true) };
	TestFalse(TEXT("no idle unit: '.' leaves the selection"), Cursor.Find(Busy, false, false).bReplace);
	const FTCIdleResult None = Cursor.Find(Busy, true, false);
	TestTrue(TEXT("no idle unit: Shift+'.' clears it"), None.bReplace && !None.Ids.Num());
	Cursor.Reset();
	TestTrue(TEXT("a new match starts the walk again"), Cursor.Find(Units, false, false).Ids == TArray<int32>({ 1 }));

	TArray<int32> Out;
	TestTrue(TEXT("a double-click on a rifle squad"), TCSelectionKeys::DoubleClick(Units, 1, false, Out));
	TestTrue(TEXT("the drawn rifle squads on the screen"), Out == TArray<int32>({ 1, 2, 16 }));
	TestTrue(TEXT("with Ctrl, on the whole map"), TCSelectionKeys::DoubleClick(Units, 1, true, Out) && Out == TArray<int32>({ 14, 1, 2, 9, 16 }));
	TestTrue(TEXT("on the HQ: own buildings of that type"), TCSelectionKeys::DoubleClick(Units, 11, false, Out) && Out == TArray<int32>({ 11 }));
	TestTrue(TEXT("on a truck: the trucks"), TCSelectionKeys::DoubleClick(Units, 5, false, Out) && Out == TArray<int32>({ 5 }));
	Out = { 99 };
	TestFalse(TEXT("someone else's unit selects nothing new"), TCSelectionKeys::DoubleClick(Units, 12, true, Out));
	TestFalse(TEXT("nor a bunker"), TCSelectionKeys::DoubleClick(Units, 15, true, Out));
	TestTrue(TEXT("and leaves the selection as it was"), Out == TArray<int32>({ 99 }));
	FVector2D Centre;
	TestTrue(TEXT("centre of units 1 and 3"), TCSelectionKeys::Centre(Units, { 1, 3 }, Centre) && Centre.Equals(FVector2D(20, 4)));
	TestFalse(TEXT("no centre for nothing"), TCSelectionKeys::Centre(Units, {}, Centre));

	FTCDoubleClick D;
	TestFalse(TEXT("a first click"), D.Click(10.0, FVector2D(100, 100)));
	TestTrue(TEXT("a second, 0.3 s later and 3 px off"), D.Click(10.3, FVector2D(103, 98)));
	TestFalse(TEXT("a third starts a new pair"), D.Click(10.5, FVector2D(103, 98)));
	TestFalse(TEXT("too late"), D.Click(11.2, FVector2D(103, 98)));
	TestFalse(TEXT("too far"), D.Click(11.4, FVector2D(120, 98)));
	TestTrue(TEXT("near the last one and in time"), D.Click(11.6, FVector2D(121, 99)));
	return true;
}

#endif
