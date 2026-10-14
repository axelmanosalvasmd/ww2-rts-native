#include "Client/TCKeys.h"
#include "Client/TCUnitTable.h"
#include "Net/TCMatchState.h"
#include "InputCoreTypes.h"
#include "Containers/StringConv.h"
#include "Client/TCLocale.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

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

// ---------------------------------------------------------------------------------------------------------------------
// Round 15: client/main.js keydown's order for F1, F2 and the table (M), client/perf.js and client/stats.js

ETCKeyRoute TCKeys::Route(const FString& Code, bool bHudShown, bool bRepeat, bool bControlsOpen, bool bStatsSheetOpen)
{
	// if (e.code === 'F1' && !$('hud').classList.contains('hidden')) { ...; toggle; return }
	if (Code == TEXT("F1") && bHudShown) return ETCKeyRoute::ControlsSheet;
	// if (e.code === 'F2' && !hidden) { e.preventDefault(); if (!e.repeat) stats.toggle(); return }
	if (Code == TEXT("F2") && bHudShown) return bRepeat ? ETCKeyRoute::Swallowed : ETCKeyRoute::StatsToggle;
	// if (controlsSheet.open || stats.isOpen()) return
	if (bControlsOpen || bStatsSheetOpen) return ETCKeyRoute::Swallowed;
	return ETCKeyRoute::Table;
}

bool TCKeys::RunsInLobby(const FString& Id)
{
	// the browser runs any table action without a match; in the lobby only M changes anything a player can see or hear
	return Id == TEXT("mute");
}

namespace
{
	double JsRoundN(double V) { return FMath::FloorToDouble(V + 0.5); } // Math.round
	FString Num(double V) { return FTCObjectives::JsNumber(V); }
}

void FTCPerf::Net(int32 InBytes, double NowMs)
{
	if (LastSnap) GapMs = FMath::Max(GapMs, NowMs - LastSnap);
	LastSnap = NowMs; Snaps++; Bytes += InBytes;
}

void FTCPerf::Pinged(double C) { Out.Add(C, C); }

TOptional<double> FTCPerf::Pong(const FJsonObject& M, double NowMs)
{
	double C = 0;
	const TSharedPtr<FJsonValue> V = M.TryGetField(TEXT("c"));
	if (!V.IsValid() || V->Type != EJson::Number || !FMath::IsFinite(C = V->AsNumber())) return TOptional<double>();
	const double Rtt = JsRoundN(NowMs - C);
	// out.delete(m.c): the stamp as the JSON brought it back (a double's text can lose its last digits here)
	double Key = 0; bool bFound = false;
	for (const TPair<double, double>& P : Out) if (FMath::Abs(P.Key - C) < 0.01) { Key = P.Key; bFound = true; break; }
	if (bFound) { Out.Remove(Key); Done.Add(false); if (Done.Num() > Pings) Done.RemoveAt(0); }
	Rtts.Add(Rtt); if (Rtts.Num() > Pings) Rtts.RemoveAt(0);
	Stat.Ping = Rtt;
	const TArray<TSharedPtr<FJsonValue>>* Srv = nullptr;
	if (M.TryGetArrayField(TEXT("srv"), Srv) && Srv)
	{
		// [stat.srvTick, stat.srvEvery] = m.srv (a missing or non-number entry is undefined, shown as n/a)
		auto At = [Srv](int32 I) { return Srv->IsValidIndex(I) && (*Srv)[I].IsValid() && (*Srv)[I]->Type == EJson::Number ? TOptional<double>((*Srv)[I]->AsNumber()) : TOptional<double>(); };
		Stat.SrvTick = At(0); Stat.SrvEvery = At(1);
	}
	return Rtt;
}

void FTCPerf::RollPings(double NowMs)
{
	for (auto It = Out.CreateIterator(); It; ++It)
		if (NowMs - It.Value() > LostAfter) { It.RemoveCurrent(); Done.Add(true); if (Done.Num() > Pings) Done.RemoveAt(0); }
	int32 Lost = 0;
	for (bool B : Done) Lost += B;
	Stat.Loss = Done.Num() ? TOptional<double>(JsRoundN((double)Lost / Done.Num() * 1000) / 10) : TOptional<double>();
	double D = 0;
	for (int32 I = 1; I < Rtts.Num(); I++) D += FMath::Abs(Rtts[I] - Rtts[I - 1]);
	Stat.Jitter = Rtts.Num() > 1 ? TOptional<double>(JsRoundN(D / (Rtts.Num() - 1))) : TOptional<double>();
}

bool FTCPerf::Frame(double StartMs, double NowMs, double BusyMs, int64 Calls, int64 Tris, int32 Units, int32 Fx, int32 Corpses)
{
	if (Since < 0) Since = StartMs; // perf.js: since = performance.now() when the module loads
	Frames++; Busy += BusyMs;
	if (LastStart) Worst = FMath::Max(Worst, StartMs - LastStart);
	LastStart = StartMs;
	Stat.Calls = Calls; Stat.Tris = Tris; Stat.Units = Units; Stat.Fx = Fx; Stat.Corpses = Corpses;
	const double Span = NowMs - Since;
	if (Span < 1000) return false;
	Stat.Fps = JsRoundN(Frames * 10000 / Span) / 10;
	Stat.Ms = JsRoundN(Span / Frames * 10) / 10;
	Stat.Worst = JsRoundN(Worst * 10) / 10;
	Stat.Cpu = JsRoundN(Busy / Frames * 10) / 10;
	Stat.Snaps = JsRoundN(Snaps * 10000 / Span) / 10;
	Stat.Bytes = Snaps ? (int32)JsRoundN(Bytes / Snaps) : 0;
	Stat.Kbs = JsRoundN(Bytes / Span * 10) / 10;
	Stat.Gap = JsRoundN(LastSnap ? FMath::Max(GapMs, NowMs - LastSnap) : 0);
	RollPings(NowMs);
	Frames = Snaps = 0; Busy = Bytes = GapMs = Worst = 0; Since = NowMs;
	return true;
}

const TArray<FTCStatDef>& FTCStatsOverlay::Defs()
{
	static const TArray<FTCStatDef> D = {
		{ TEXT("Performance"), TEXT("fps"), TEXT("FPS"), TEXT("Frame rate"), nullptr },
		{ TEXT("Performance"), TEXT("frame"), TEXT("Frame"), TEXT("Frame time"), TEXT("Average over the last second") },
		{ TEXT("Performance"), TEXT("worst"), TEXT("Worst"), TEXT("Worst frame"), TEXT("Longest in the last second: hitches") },
		{ TEXT("Performance"), TEXT("cpu"), TEXT("JS"), TEXT("Script time"), TEXT("Game code per frame") },
		{ TEXT("Performance"), TEXT("calls"), TEXT("Draws"), TEXT("Draw calls"), nullptr },
		{ TEXT("Performance"), TEXT("tris"), TEXT("Tris"), TEXT("Triangles"), nullptr },
		{ TEXT("Performance"), TEXT("scale"), TEXT("Res"), TEXT("Render scale and graphics"), nullptr },
		{ TEXT("Performance"), TEXT("mem"), TEXT("Mem"), TEXT("Memory"), TEXT("Chrome and Edge only") },
		{ TEXT("Network"), TEXT("ping"), TEXT("Ping"), TEXT("Ping"), TEXT("Round trip to the server") },
		{ TEXT("Network"), TEXT("jitter"), TEXT("Jitter"), TEXT("Jitter"), TEXT("How much the ping swings") },
		{ TEXT("Network"), TEXT("loss"), TEXT("Loss"), TEXT("Loss"), TEXT("Pings unanswered after 3 s, last minute") },
		{ TEXT("Network"), TEXT("snaps"), TEXT("Upd"), TEXT("Updates per second"), TEXT("Game state messages received") },
		{ TEXT("Network"), TEXT("gap"), TEXT("Gap"), TEXT("Longest update gap"), TEXT("Longest wait in the last second: stalls") },
		{ TEXT("Network"), TEXT("size"), TEXT("Size"), TEXT("Update size"), nullptr },
		{ TEXT("Network"), TEXT("kbs"), TEXT("Down"), TEXT("Download rate"), nullptr },
		{ TEXT("Network"), TEXT("server"), TEXT("Server"), TEXT("Server tick"), TEXT("Slowest 5% of ticks that send an update; over 40 ms it sends fewer") },
		{ TEXT("Game"), TEXT("units"), TEXT("Units"), TEXT("Units"), nullptr },
		{ TEXT("Game"), TEXT("fx"), TEXT("FX"), TEXT("Effect particles"), nullptr },
		{ TEXT("Game"), TEXT("corpses"), TEXT("Bodies"), TEXT("Bodies"), nullptr },
	};
	return D;
}

TArray<FString> FTCStatsOverlay::Preset(const FString& Name)
{
	if (Name == TEXT("Basic")) return { TEXT("fps"), TEXT("ping"), TEXT("loss") };
	if (Name == TEXT("Performance")) return { TEXT("fps"), TEXT("frame"), TEXT("worst"), TEXT("cpu"), TEXT("calls"), TEXT("tris"), TEXT("mem") };
	if (Name == TEXT("Network")) return { TEXT("ping"), TEXT("jitter"), TEXT("loss"), TEXT("snaps"), TEXT("gap"), TEXT("kbs"), TEXT("server") };
	TArray<FString> All;
	if (Name == TEXT("Everything")) for (const FTCStatDef& D : Defs()) All.Add(D.Id);
	return All;
}

void FTCStatsOverlay::Load(const TOptional<FString>& Saved, bool bForced)
{
	S = FTCStatsSettings();
	bForcedAll = bForced;
	TSharedPtr<FJsonObject> O;
	if (Saved.IsSet())
	{
		// try { saved = JSON.parse(...) } catch {}; a value that is not an object keeps every default
		const TSharedRef<TJsonReader<>> R = TJsonReaderFactory<>::Create(*Saved);
		if (!FJsonSerializer::Deserialize(R, O)) O.Reset();
	}
	if (O.IsValid())
	{
		bool B = false;
		if (O->HasTypedField<EJson::Boolean>(TEXT("show")) && O->TryGetBoolField(TEXT("show"), B)) S.bShow = B;
		if (O->HasTypedField<EJson::Boolean>(TEXT("warn")) && O->TryGetBoolField(TEXT("warn"), B)) S.bWarn = B;
		const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
		if (O->HasTypedField<EJson::Array>(TEXT("stats")) && O->TryGetArrayField(TEXT("stats"), Arr))
		{
			// IDS.filter((id) => saved.stats.includes(id)): the table's order, strings only
			S.Stats.Reset();
			for (const FTCStatDef& D : Defs())
				if (Arr->ContainsByPredicate([&D](const TSharedPtr<FJsonValue>& V) { return V.IsValid() && V->Type == EJson::String && V->AsString() == D.Id; })) S.Stats.Add(D.Id);
		}
		// if (saved[key] in CHOICES[key]) s[key] = saved[key]: a string that names a choice (a number never does)
		auto Choice = [&O](const TCHAR* Key, std::initializer_list<const TCHAR*> Opts, FString& Out)
		{
			FString V;
			if (!O->HasTypedField<EJson::String>(Key) || !O->TryGetStringField(Key, V)) return;
			for (const TCHAR* Opt : Opts) if (V == Opt) { Out = V; return; }
		};
		Choice(TEXT("pos"), { TEXT("tl"), TEXT("tc"), TEXT("tr") }, S.Pos);
		Choice(TEXT("layout"), { TEXT("list"), TEXT("line") }, S.Layout);
		Choice(TEXT("size"), { TEXT("s"), TEXT("m"), TEXT("l") }, S.Size);
		Choice(TEXT("bg"), { TEXT("panel"), TEXT("clear"), TEXT("none") }, S.Bg);
	}
	if (bForced) { S.bShow = true; S.Stats = Preset(TEXT("Everything")); } // '?perf' (-TCPerf here)
}

TOptional<FString> FTCStatsOverlay::SaveText() const
{
	if (bForcedAll) return TOptional<FString>();
	// JSON.stringify(settings): show, stats, pos, layout, size, bg, warn (DEFAULTS' order)
	TArray<FString> Q;
	for (const FString& Id : S.Stats) Q.Add(FString::Printf(TEXT("\"%s\""), *Id));
	return FString::Printf(TEXT("{\"show\":%s,\"stats\":[%s],\"pos\":\"%s\",\"layout\":\"%s\",\"size\":\"%s\",\"bg\":\"%s\",\"warn\":%s}"),
		S.bShow ? TEXT("true") : TEXT("false"), *FString::Join(Q, TEXT(",")), *S.Pos, *S.Layout, *S.Size, *S.Bg, S.bWarn ? TEXT("true") : TEXT("false"));
}

FString FTCStatsOverlay::Show(const FString& Id, const FTCPerfStats& St)
{
	auto K = [](double N) { return N >= 1000 ? FTCObjectives::JsToFixed(N / 1000, 1) + TEXT("k") : Num(N); };
	auto Ms = [](const TOptional<double>& N) { return N.IsSet() ? Num(*N) + TEXT(" ms") : FString(TEXT("n/a")); };
	if (Id == TEXT("fps")) return Num(St.Fps);
	if (Id == TEXT("frame")) return Ms(St.Ms);
	if (Id == TEXT("worst")) return Ms(St.Worst);
	if (Id == TEXT("cpu")) return Ms(St.Cpu);
	if (Id == TEXT("calls")) return Num((double)St.Calls);
	if (Id == TEXT("tris")) return K((double)St.Tris);
	if (Id == TEXT("scale")) return FString::Printf(TEXT("%sx %s"), *Num(St.Scale), *St.Gfx);
	if (Id == TEXT("mem")) return St.Mem.IsSet() ? Num(*St.Mem) + TEXT(" MB") : FString(TEXT("n/a"));
	if (Id == TEXT("ping")) return Ms(St.Ping);
	if (Id == TEXT("jitter")) return Ms(St.Jitter);
	if (Id == TEXT("loss")) return St.Loss.IsSet() ? Num(*St.Loss) + TEXT("%") : FString(TEXT("n/a"));
	if (Id == TEXT("snaps")) return Num(St.Snaps) + TEXT("/s");
	if (Id == TEXT("gap")) return Ms(St.Gap);
	if (Id == TEXT("size")) return FTCObjectives::JsToFixed(St.Bytes / 1000.0, 1) + TEXT(" kB");
	if (Id == TEXT("kbs")) return Num(St.Kbs) + TEXT(" kB/s");
	if (Id == TEXT("server")) return St.SrvTick.IsSet() ? Num(*St.SrvTick) + TEXT(" ms") : FString(TEXT("n/a"));
	if (Id == TEXT("units")) return Num(St.Units);
	if (Id == TEXT("fx")) return K(St.Fx);
	if (Id == TEXT("corpses")) return Num(St.Corpses);
	return FString();
}

int32 FTCStatsOverlay::Warn(const FString& Id, const FTCPerfStats& St)
{
	// band(v, amber, red, low): 0 fine, 1 amber, 2 red; null is fine; low marks numbers where less is worse
	auto Band = [](const TOptional<double>& V, double Amber, double Red, bool bLow = false) { if (!V.IsSet()) return 0; const double X = *V; return bLow ? (X < Red ? 2 : X < Amber ? 1 : 0) : (X > Red ? 2 : X > Amber ? 1 : 0); };
	if (Id == TEXT("fps")) return Band(St.Fps, 50, 30, true);
	if (Id == TEXT("frame")) return Band(St.Ms, 20, 33);
	if (Id == TEXT("worst")) return Band(St.Worst, 33, 66);
	if (Id == TEXT("cpu")) return Band(St.Cpu, 12, 20);
	if (Id == TEXT("ping")) return Band(St.Ping, 80, 150);
	if (Id == TEXT("jitter")) return Band(St.Jitter, 15, 40);
	if (Id == TEXT("loss")) return Band(St.Loss, 0, 5);
	if (Id == TEXT("snaps")) return Band(St.Snaps, 7, 4, true);
	if (Id == TEXT("gap")) return Band(St.Gap, 200, 400);
	if (Id == TEXT("server")) return Band(St.SrvTick, 25, 40);
	return 0;
}

TArray<FTCStatsOverlay::FRow> FTCStatsOverlay::Rows(const FTCPerfStats& St) const
{
	TArray<FRow> Out;
	for (const FTCStatDef& D : Defs())
		if (S.Stats.Contains(D.Id)) Out.Add({ D.Tag, TC_T(Show(D.Id, St)), S.bWarn ? Warn(D.Id, St) : 0 });
	return Out;
}

FTCPaint FTCStatsOverlay::Layout(const FTCPerfStats& St, float TopTl, float TopTc, float TopTr, const FBox2D& Strip, float LeftOfTr, float RightOfTl, float ViewW, const FTCMeasure& Measure) const
{
	FTCPaint P;
	if (!S.bShow) return P;
	// stats.css: 14 px (12 small, 17 large), padding 5px 9px (4px 7px, 6px 11px); no background: no side padding
	const float Px = S.Size == TEXT("s") ? 12.f : S.Size == TEXT("l") ? 17.f : 14.f;
	const float PadY = S.Size == TEXT("s") ? 4.f : S.Size == TEXT("l") ? 6.f : 5.f, PadX = S.Bg == TEXT("none") ? 0.f : S.Size == TEXT("s") ? 7.f : S.Size == TEXT("l") ? 11.f : 9.f;
	const float LineH = Px * 1.35f, Gap = 8.f, ColGap = 14.f;
	const TArray<FRow> R = Rows(St);
	// the content: a grid of tag and value columns (list), or the stats wrapped in one line (line)
	struct FCell { FString Text; float X = 0, Y = 0; FLinearColor C; ETCPaintAlign A = ETCPaintAlign::Left; };
	TArray<FCell> Cells;
	float W = 0, H = 0;
	const FLinearColor Amber = TCCss::Brass, RedC = TCCss::RedHi;
	auto ValC = [&](int32 Warn) { return Warn == 2 ? RedC : Warn == 1 ? Amber : TCCss::Text; };
	if (!R.Num())
	{
		const FString T = TC_T(FString(TEXT("No stats picked. Menu > Stats overlay")));
		Cells.Add({ T, 0, 0, TCCss::Text });
		W = Measure(T, Px, false, 0.f); H = LineH;
	}
	else if (S.Layout == TEXT("list"))
	{
		float TagW = 0, ValW = 0;
		for (const FRow& Row : R) { TagW = FMath::Max(TagW, Measure(Row.Tag, Px, false, 0.f)); ValW = FMath::Max(ValW, Measure(Row.Value, Px, false, 0.f)); }
		for (int32 I = 0; I < R.Num(); I++)
		{
			Cells.Add({ R[I].Tag, 0, I * LineH, TCCss::Dim });
			Cells.Add({ R[I].Value, TagW + ColGap + ValW, I * LineH, ValC(R[I].Warn), ETCPaintAlign::Right });
		}
		W = TagW + ColGap + ValW; H = R.Num() * LineH;
	}
	else
	{
		// layout-line: max-width from the room beside the panels at its sides (at least 160), wrapping between stats
		const float Room = S.Pos == TEXT("tl") ? LeftOfTr : S.Pos == TEXT("tr") ? ViewW - RightOfTl : 2 * FMath::Min(ViewW / 2 - RightOfTl, LeftOfTr - ViewW / 2);
		const float MaxW = FMath::Min(FMath::Max(160.f, Room - 16.f), ViewW - 2 * Gap) - 2 * PadX - 2;
		float X = 0, Y = 0;
		for (const FRow& Row : R)
		{
			const float Tw = Measure(Row.Tag, Px, false, 0.f), Vw = Measure(Row.Value, Px, false, 0.f), Iw = Tw + 5 + Vw;
			if (X > 0 && X + ColGap + Iw > MaxW) { X = 0; Y += LineH; }
			const float At = X > 0 ? X + ColGap : 0.f;
			Cells.Add({ Row.Tag, At, Y, TCCss::Dim });
			Cells.Add({ Row.Value, At + Tw + 5, Y, ValC(Row.Warn) });
			X = At + Iw; W = FMath::Max(W, X);
		}
		H = Y + LineH;
	}
	const float Border = S.Bg == TEXT("none") ? 0.f : 1.f, BoxW = W + 2 * PadX + 2 * Border, BoxH = H + 2 * PadY + 2 * Border;
	// left var(--gap) (+2 px without a background), right likewise, or centred
	const float X0 = S.Pos == TEXT("tl") ? Gap + (S.Bg == TEXT("none") ? 2.f : 0.f) : S.Pos == TEXT("tr") ? ViewW - Gap - (S.Bg == TEXT("none") ? 2.f : 0.f) - BoxW : (ViewW - BoxW) / 2;
	// place(): under its panel (6 px), else 52 px; a box that would run into the score strip goes under it
	const float Under = S.Pos == TEXT("tl") ? TopTl : S.Pos == TEXT("tr") ? TopTr : TopTc;
	float Y0 = Under >= 0 ? Under + 6.f : 52.f;
	if (Strip.bIsValid && S.Pos != TEXT("tc") && X0 < Strip.Max.X && X0 + BoxW > Strip.Min.X && Y0 < Strip.Max.Y) Y0 = Strip.Max.Y + 6.f;
	if (S.Bg == TEXT("panel")) { P.Rect(X0, Y0, BoxW, BoxH, TCCss::PanelSolid, TEXT("stats:box")); P.Frame(X0, Y0, BoxW, BoxH, TCCss::Line, 1.f, TEXT("stats:frame")); }
	else if (S.Bg == TEXT("clear")) { P.Rect(X0, Y0, BoxW, BoxH, TCCss::Rgba(25, 28, 30, 0.5f), TEXT("stats:box")); P.Frame(X0, Y0, BoxW, BoxH, TCCss::LineSoft, 1.f, TEXT("stats:frame")); }
	for (const FCell& C : Cells)
	{
		const float Tx = X0 + Border + PadX + C.X, Ty = Y0 + Border + PadY + C.Y;
		if (S.Bg == TEXT("none")) { FTCPaintItem& Sh = P.Text(C.Text, Tx, Ty + 1.f, Px, TCCss::Rgba(0, 0, 0, 0.95f), false, C.A, TEXT("stats:shadow")); Sh.Size.Y = LineH; }
		FTCPaintItem& T = P.Text(C.Text, Tx, Ty, Px, C.C, false, C.A, C.C.Equals(TCCss::Dim) ? TEXT("stats:tag") : TEXT("stats:value"));
		T.Size.Y = LineH;
	}
	return P;
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


IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCKeysF2MTest, "ThreeCrossroads.Keys.F2AndM", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCKeysF2MTest::RunTest(const FString&)
{
	// client/main.js keydown: F1 and F2 first while the HUD shows (F2 not on a repeat), then the open sheets swallow keys
	TestTrue(TEXT("F1 in a match"), TCKeys::Route(TEXT("F1"), true, false, false, false) == ETCKeyRoute::ControlsSheet);
	TestTrue(TEXT("F1 repeated still toggles"), TCKeys::Route(TEXT("F1"), true, true, false, false) == ETCKeyRoute::ControlsSheet);
	TestTrue(TEXT("F1 with the Controls sheet open closes it"), TCKeys::Route(TEXT("F1"), true, false, true, false) == ETCKeyRoute::ControlsSheet);
	TestTrue(TEXT("F2 in a match"), TCKeys::Route(TEXT("F2"), true, false, false, false) == ETCKeyRoute::StatsToggle);
	TestTrue(TEXT("F2 over the Controls sheet still toggles"), TCKeys::Route(TEXT("F2"), true, false, true, false) == ETCKeyRoute::StatsToggle);
	TestTrue(TEXT("F2 repeated: nothing"), TCKeys::Route(TEXT("F2"), true, true, false, false) == ETCKeyRoute::Swallowed);
	TestTrue(TEXT("F2 in the lobby: the table (no binding)"), TCKeys::Route(TEXT("F2"), false, false, false, false) == ETCKeyRoute::Table);
	TestTrue(TEXT("M with the Controls sheet open: stops there"), TCKeys::Route(TEXT("KeyM"), true, false, true, false) == ETCKeyRoute::Swallowed);
	TestTrue(TEXT("M with the stats sheet open: stops there"), TCKeys::Route(TEXT("KeyM"), true, false, false, true) == ETCKeyRoute::Swallowed);
	TestTrue(TEXT("M in the lobby: the table"), TCKeys::Route(TEXT("KeyM"), false, false, false, false) == ETCKeyRoute::Table);
	// keys.js: M is 'mute' with no modifier in every context; Shift+M is 'fort:demine'; Ctrl+M and Alt+M nothing
	const TArray<ETCKeyContext> Army = { ETCKeyContext::Army }, Classic = { ETCKeyContext::Classic, ETCKeyContext::Building }, Aim = { ETCKeyContext::Army, ETCKeyContext::Targeting };
	for (const TArray<ETCKeyContext>& C : { Army, Classic, Aim })
	{
		const FTCKeyBinding* B = TCKeys::Match(TEXT("KeyM"), false, false, false, C);
		TestTrue(TEXT("M is mute"), B && B->Id == TEXT("mute") && B->Label == TEXT("M") && B->Help == TEXT("Toggle audio"));
		const FTCKeyBinding* Sh = TCKeys::Match(TEXT("KeyM"), true, false, false, C);
		TestTrue(TEXT("Shift+M is demine"), Sh && Sh->Id == TEXT("fort:demine"));
		TestNull(TEXT("Ctrl+M"), TCKeys::Match(TEXT("KeyM"), false, true, false, C));
		TestNull(TEXT("Alt+M"), TCKeys::Match(TEXT("KeyM"), false, false, true, C));
	}
	TestNull(TEXT("F2 is not in the table"), TCKeys::Match(TEXT("F2"), false, false, false, Army));
	TestTrue(TEXT("only mute runs in the lobby"), TCKeys::RunsInLobby(TEXT("mute")) && !TCKeys::RunsInLobby(TEXT("alert")) && !TCKeys::RunsInLobby(TEXT("stop")));
	// stats.js: DEFAULTS, toggle (F2), save and load
	FTCStatsOverlay O;
	O.Load(TOptional<FString>());
	TestFalse(TEXT("hidden by default"), O.Settings().bShow);
	TestEqual(TEXT("defaults saved"), O.SaveText().Get(TEXT("")), FString(TEXT("{\"show\":false,\"stats\":[\"fps\",\"ping\",\"loss\"],\"pos\":\"tl\",\"layout\":\"list\",\"size\":\"m\",\"bg\":\"panel\",\"warn\":true}")));
	O.Toggle();
	TestTrue(TEXT("F2 shows it"), O.Settings().bShow);
	TestEqual(TEXT("menu label"), O.MenuLabel(), FString(TEXT("Stats overlay: On")));
	FTCStatsOverlay L;
	L.Load(O.SaveText());
	TestTrue(TEXT("survives a restart"), L.Settings().bShow);
	L.Load(FString(TEXT("{\"show\":true,\"stats\":[\"loss\",\"fps\",\"bogus\",3],\"pos\":\"tr\",\"layout\":\"line\",\"size\":7,\"bg\":\"none\",\"warn\":\"yes\"}")));
	TestEqual(TEXT("stats in table order, unknown dropped"), FString::Join(L.Settings().Stats, TEXT(",")), FString(TEXT("fps,loss")));
	TestTrue(TEXT("choices: named ones taken, others default"), L.Settings().Pos == TEXT("tr") && L.Settings().Layout == TEXT("line") && L.Settings().Size == TEXT("m") && L.Settings().Bg == TEXT("none") && L.Settings().bWarn);
	L.Load(FString(TEXT("[1,2]")));
	TestFalse(TEXT("not an object: defaults"), L.Settings().bShow);
	L.Load(FString(TEXT("{broken")));
	TestTrue(TEXT("unparsable: defaults"), !L.Settings().bShow && L.Settings().Stats.Num() == 3);
	L.Load(TOptional<FString>(), true);
	TestTrue(TEXT("?perf: all shown, nothing saved"), L.Settings().bShow && L.Settings().Stats.Num() == 19 && !L.SaveText().IsSet());
	// perf.js: the second's roll
	FTCPerf Pf;
	double T = 0;
	Pf.Pinged(100.0);
	for (int32 F = 0; F < 60; F++) { Pf.Frame(T, T + 4, 4, 900, 250000, 40, 1200, 6); T += F == 30 ? 50 : 16; if (F % 6 == 0) Pf.Net(5200, T); }
	TSharedPtr<FJsonObject> Pong = MakeShared<FJsonObject>();
	Pong->SetNumberField(TEXT("c"), 100.0);
	TArray<TSharedPtr<FJsonValue>> Srv = { MakeShared<FJsonValueNumber>(31), MakeShared<FJsonValueNumber>(2) };
	Pong->SetArrayField(TEXT("srv"), Srv);
	TestTrue(TEXT("pong rtt"), Pf.Pong(*Pong, 148.4).Get(-1) == 48.0);
	const bool bRolled = Pf.Frame(T, 1000.0 + 1, 4, 900, 250000, 40, 1200, 6);
	TestTrue(TEXT("rolled after a second"), bRolled);
	const FTCPerfStats& St = Pf.Stats();
	TestTrue(TEXT("fps = round(frames * 10000 / span) / 10"), St.Fps == FMath::FloorToDouble(61 * 10000.0 / 1001.0 + 0.5) / 10);
	TestTrue(TEXT("worst: the 50 ms frame"), St.Worst == 50.0);
	TestTrue(TEXT("loss 0%, ping 48, server 31"), St.Loss.Get(-1) == 0.0 && St.Ping.Get(-1) == 48.0 && St.SrvTick.Get(-1) == 31.0 && !St.Jitter.IsSet());
	TestEqual(TEXT("size"), FTCStatsOverlay::Show(TEXT("size"), St), FString(TEXT("5.2 kB")));
	TestEqual(TEXT("tris"), FTCStatsOverlay::Show(TEXT("tris"), St), FString(TEXT("250.0k")));
	TestEqual(TEXT("fx"), FTCStatsOverlay::Show(TEXT("fx"), St), FString(TEXT("1.2k")));
	TestEqual(TEXT("mem n/a"), FTCStatsOverlay::Show(TEXT("mem"), St), FString(TEXT("n/a")));
	TestEqual(TEXT("worst is red over 66? amber over 33"), FTCStatsOverlay::Warn(TEXT("worst"), St), 1);
	TestEqual(TEXT("server 31 ms amber"), FTCStatsOverlay::Warn(TEXT("server"), St), 1);
	// a ping never answered counts as lost after 3 s
	Pf.Pinged(1500.0);
	Pf.Frame(4600, 4601, 4, 900, 1, 1, 1, 1);
	TestTrue(TEXT("loss 50%"), Pf.Stats().Loss.Get(-1) == 50.0);
	TestEqual(TEXT("loss red over 5"), FTCStatsOverlay::Warn(TEXT("loss"), Pf.Stats()), 2);
	// the box: list layout under the top-left panel, 52 px without one, below the strip when it would run into it
	{
		FTCLocale::FScopedLanguage En(ETCLanguage::English);
		const FTCMeasure M = FTCPaint::ApproxMeasure();
		const FTCPaint Box = O.Layout(St, -1, -1, -1, FBox2D(ForceInit), 1280, 0, 1280, M);
		TestEqual(TEXT("tags"), Box.TextsOf(TEXT("stats:tag")), TArray<FString>{ TEXT("FPS"), TEXT("Ping"), TEXT("Loss") });
		const FTCPaintItem* B = Box.Find(TEXT("stats:box"));
		TestTrue(TEXT("at 8, 52"), B && B->Pos.X == 8.f && B->Pos.Y == 52.f);
		const FTCPaint Under = O.Layout(St, -1, -1, -1, FBox2D(FVector2D(0, 6), FVector2D(1280, 90)), 1280, 0, 1280, M);
		TestTrue(TEXT("under the strip"), Under.Find(TEXT("stats:box")) && Under.Find(TEXT("stats:box"))->Pos.Y == 96.f);
		FTCStatsOverlay E;
		E.Load(FString(TEXT("{\"show\":true,\"stats\":[]}")));
		TestEqual(TEXT("none picked"), E.Layout(St, -1, -1, -1, FBox2D(ForceInit), 1280, 0, 1280, M).TextsOf(TEXT("stats:value")), TArray<FString>{ TEXT("No stats picked. Menu > Stats overlay") });
		FTCStatsOverlay Hidden;
		Hidden.Load(TOptional<FString>());
		TestEqual(TEXT("hidden: nothing"), Hidden.Layout(St, -1, -1, -1, FBox2D(ForceInit), 1280, 0, 1280, M).Items.Num(), 0);
	}
	return true;
}

#endif
