// Slice 11 and 12 verification switches of ATCGameMode (see TCGameMode.h): screenshots of the reference's models, map pieces
// and scenery in a live match, picked from what this client holds and draws. The camera is moved directly (FocusOn,
// ZoomTo), as the earlier slices' shot switches do; nothing here is a player or a physical mouse or keyboard.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "Client/TCEffects.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCCameraPawn.h"
#include "Client/TCLocale.h"
#include "World/TCWorldObjects.h"
#include "World/TCScatter.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "Dom/JsonObject.h"
#include "Misc/Paths.h"
#include "UnrealClient.h"

namespace TCSlice11Local
{
	// effects that would hide or cloud a close look at the models
	bool Clouds(ETCFxLook L)
	{
		return L == ETCFxLook::Smoke || L == ETCFxLook::DarkSmoke || L == ETCFxLook::Screen || L == ETCFxLook::Dust || L == ETCFxLook::Fireball || L == ETCFxLook::Flame;
	}
	bool IsVehicle(const FTCUnitDef* D) { return D && !D->bInfantry && !D->bStructure && !D->bAir; }
}

void ATCGameMode::RunSlice11Switches(double Now)
{
	using namespace TCSlice11Local;
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	ATCHUD* Hud = PC ? Cast<ATCHUD>(PC->GetHUD()) : nullptr;
	const bool bDrawn = Hud && Hud->HasDrawn();
	const double InMatch = MatchSeenAt >= 0 && State.bHasMatch ? Now - MatchSeenAt : -1;

	// -TCSpawnOwn: sandbox units for this seat beside its army (shared/sim.js sandboxSpawn: no 'enemy', the sender owns it)
	if (!SpawnOwn.IsEmpty() && !bSpawnedOwn && State.CanOrder() && State.bSandbox && State.Snapshots >= 3)
	{
		double CX, CZ;
		if (ArmyIds(CX, CZ).Num())
		{
			bSpawnedOwn = true;
			const FVector2D Centre(State.Map.W * FTCMatchState::CellMetres / 2, State.Map.H * FTCMatchState::CellMetres / 2), From(CX, CZ);
			const FVector2D Dir = (Centre - From).GetSafeNormal(), Across(-Dir.Y, Dir.X);
			TArray<FString> Types;
			SpawnOwn.ParseIntoArray(Types, TEXT(","));
			// slice 12: no sandbox unit in a building. Each point moves to the nearest spot (rings of 4 m out to 40 m, 16
			// directions) at least 9 m from every house, wall, hedge or sandbag piece, 4 m from every tree or bush and 7 m
			// from every unit held and every point already chosen.
			TArray<FVector2D> Blocks, Trees, Taken;
			{
				using namespace TCWorldCore;
				using namespace TCScatterCore;
				for (int32 K = 0; K < WorldObjects.ChunkCount(); K++)
				{
					WorldObjects.ForEachPiece(K, [&](const FPiece& P) { Blocks.Add(FVector2D(P.X, P.Z)); });
					WorldObjects.ForEachProp(K, [&](const FCandidate& C, const FPlaced& P)
					{
						if (P.bShown && (P.Kind == EProp::Deciduous || P.Kind == EProp::Poplar || P.Kind == EProp::Pine || P.Kind == EProp::Bush)) Trees.Add(FVector2D(P.X, P.Z));
					});
				}
				// a structure (a barracks, the HQ, a Nissen hut) is wider than a unit: eight more points on a 6 m ring round it
				for (const auto& U : State.Units)
				{
					Taken.Add(FVector2D(U.Value.X, U.Value.Z));
					const FTCUnitDef* D = UnitTable.Find(U.Value.Type);
					if (D && D->bStructure) for (int32 A = 0; A < 8; A++) Taken.Add(FVector2D(U.Value.X, U.Value.Z) + FVector2D(FMath::Cos(A * UE_DOUBLE_PI / 4), FMath::Sin(A * UE_DOUBLE_PI / 4)) * 6.0);
				}
			}
			auto Clear = [&](const FVector2D& P)
			{
				for (const FVector2D& B : Blocks) if (FVector2D::DistSquared(B, P) < 9.0 * 9.0) return false;
				for (const FVector2D& T : Trees) if (FVector2D::DistSquared(T, P) < 4.0 * 4.0) return false;
				for (const FVector2D& T : Taken) if (FVector2D::DistSquared(T, P) < 7.0 * 7.0) return false;
				return true;
			};
			for (int32 I = 0; I < Types.Num(); I++)
			{
				const FVector2D Nominal = From + Dir * 12.0 + Across * ((I - (Types.Num() - 1) / 2.0) * 7.0);
				FVector2D At = Nominal;
				bool bClear = Clear(At);
				for (int32 Ring = 1; Ring <= 10 && !bClear; Ring++)
					for (int32 A = 0; A < 16 && !bClear; A++)
					{
						const double Ang = A * UE_DOUBLE_PI / 8.0;
						const FVector2D Try = Nominal + FVector2D(FMath::Cos(Ang), FMath::Sin(Ang)) * (4.0 * Ring);
						if (Clear(Try)) { At = Try; bClear = true; }
					}
				Taken.Add(At);
				UE_LOG(LogTC, Log, TEXT("sandbox spawn: own %s moved %.1f m from (%.1f, %.1f) to stand clear of %d pieces and %d trees: %s"), *Types[I], FVector2D::Distance(At, Nominal), Nominal.X, Nominal.Y, Blocks.Num(), Trees.Num(), bClear ? TEXT("clear") : TEXT("no clear spot within 40 m, kept"));
				TSharedRef<FJsonObject> Sp = MakeShared<FJsonObject>();
				Sp->SetStringField(TEXT("t"), TEXT("spawn")); Sp->SetStringField(TEXT("unit"), Types[I]);
				Sp->SetNumberField(TEXT("x"), At.X); Sp->SetNumberField(TEXT("z"), At.Y);
				UE_LOG(LogTC, Log, TEXT("sandbox spawn: own %s at (%.1f, %.1f): %s"), *Types[I], At.X, At.Y, SendCommand(Sp) ? TEXT("sent") : *LastSendProblemText);
			}
		}
	}

	// -TCMoveOwnAfter (slice 12): the whole own army, spawned units too, ordered toward the centre once
	if (MoveOwnAfter >= 0 && !bMovedOwn && PC && InMatch >= MoveOwnAfter && State.CanOrder())
	{
		double CX, CZ;
		const TArray<int32> Ids = ArmyIds(CX, CZ, bAttackWithTrucks); // round 14: -TCAttackWithTrucks sends the trucks too
		if (Ids.Num())
		{
			bMovedOwn = true;
			const FVector2D Centre(State.Map.W * FTCMatchState::CellMetres / 2, State.Map.H * FTCMatchState::CellMetres / 2), From(CX, CZ);
			const FVector2D To = From + (Centre - From).GetSafeNormal() * MoveOwnMetres;
			PC->SelectOwn(Ids);
			const int32 Sent = PC->IssueMoveAt(To);
			UE_LOG(LogTC, Log, TEXT("move own: %d own units at (%.1f, %.1f) -> (%.1f, %.1f), %d ordered"), Ids.Num(), CX, CZ, To.X, To.Y, Sent);
		}
	}

	// -TCCloseShotAfter: a squad and a vehicle side by side, no smoke among them, the camera as close as the game allows
	if (CloseShotAfter >= 0 && !bCloseShot && InMatch >= CloseShotAfter && PC && bDrawn && Units && State.Snapshots > 0 && Now - CloseShotTried >= 0.5)
	{
		CloseShotTried = Now;
		const TArray<FTCFxParticle>* Live = Effects ? &Effects->Fx.Particles() : nullptr;
		double Best = TNumericLimits<double>::Max();
		FVector2D BestAt;
		int32 BestSquad = 0, BestVehicle = 0;
		TMap<int32, FVector2D> NowAt;
		for (const auto& B : State.Units) if (IsVehicle(UnitTable.Find(B.Value.Type))) NowAt.Add(B.Key, FVector2D(B.Value.X, B.Value.Z));
		auto Moving = [&](int32 Id) { const FVector2D* Was = CloseLastAt.Find(Id); return Was && FVector2D::Distance(*Was, NowAt[Id]) >= 0.4; };
		for (const auto& A : State.Units)
		{
			const FTCUnitDef* DA = UnitTable.Find(A.Value.Type);
			if (!DA || !DA->bInfantry || !ATCUnitRenderer::IsDrawn(A.Value) || !Units->UsesModel(A.Value.Type)) continue;
			for (const auto& B : State.Units)
			{
				const FTCUnitDef* DB = UnitTable.Find(B.Value.Type);
				if (!IsVehicle(DB) || !ATCUnitRenderer::IsDrawn(B.Value) || (bCloseShotMoving && !Moving(B.Key))) continue;
				const double D2 = FMath::Square(A.Value.X - B.Value.X) + FMath::Square(A.Value.Z - B.Value.Z);
				if (D2 > 30.0 * 30.0 || D2 >= Best) continue;
				const FVector2D Mid((A.Value.X + B.Value.X) / 2, (A.Value.Z + B.Value.Z) / 2);
				int32 Cloud = 0;
				// -TCCloseShotMoving: the dust a moving vehicle kicks up belongs in the picture (fx.js vehicle dust)
				if (Live) for (const FTCFxParticle& P : *Live) if (Clouds(P.Look) && !(bCloseShotMoving && P.Look == ETCFxLook::Dust) && FVector2D::DistSquared(FVector2D(P.P.X, P.P.Y), Mid) < 30.0 * 30.0) Cloud++;
				if (Cloud) continue;
				Best = D2; BestAt = Mid; BestSquad = A.Key; BestVehicle = B.Key;
			}
		}
		const double MovedBy = BestVehicle && CloseLastAt.Contains(BestVehicle) ? FVector2D::Distance(CloseLastAt[BestVehicle], NowAt[BestVehicle]) : 0.0;
		CloseLastAt = MoveTemp(NowAt);
		if (BestSquad)
		{
			bCloseShot = true;
			if (bCloseShotMoving) UE_LOG(LogTC, Log, TEXT("close shot: vehicle %d moved %.2f m since the last try 0.5 s before"), BestVehicle, MovedBy);
			PC->FocusOn(BestAt);
			PC->ZoomTo(TCCameraFeel::MinDistance);
			if (Hud) Hud->bShowHUD = false; // the first frame is the models alone (AHUD's canvas, not Slate, draws this HUD)
			CloseShotAt = Now + 0.3;
			const FTCUnit& Sq = State.Units[BestSquad];
			const FTCUnit& Ve = State.Units[BestVehicle];
			UE_LOG(LogTC, Log, TEXT("close shot: camera on (%.1f, %.1f) at %.0f m, tick %d: squad %d (%s, seat %d) and vehicle %d (%s, seat %d), %.1f m apart, no smoke, dust or fire within 30 m"),
				BestAt.X, BestAt.Y, TCCameraFeel::MinDistance, State.Tick, BestSquad, *Sq.Type, Sq.Owner, BestVehicle, *Ve.Type, Ve.Owner, FMath::Sqrt(Best));
		}
		else if (InMatch >= CloseShotAfter + 60.0)
		{
			bCloseShot = true;
			UE_LOG(LogTC, Warning, TEXT("close shot: no squad and vehicle within 30 m without smoke in 60 s: no screenshot"));
		}
	}
	if (CloseShotAt > 0 && Now >= CloseShotAt)
	{
		CloseShotAt = -1;
		// the frame without the HUD first (the models alone), then the same view with it 0.3 s later
		const FString File = CloseShotFile.IsEmpty() ? FString(TEXT("tc-close.png")) : CloseShotFile;
		FScreenshotRequest::RequestScreenshot(FPaths::ChangeExtension(File, TEXT("")) + TEXT("-nohud.png"), false, false);
		CloseShotUiAt = Now + 0.3;
		UE_LOG(LogTC, Log, TEXT("close shot: screenshot requested without the HUD: %s; %s"), *(FPaths::ChangeExtension(File, TEXT("")) + TEXT("-nohud.png")), *Units->ModelStatusLine());
	}
	if (CloseShotUiAt > 0 && Now >= CloseShotUiAt && Hud && !Hud->bShowHUD)
	{
		Hud->bShowHUD = true; // drawn again from the next frame; the shot with it a moment later
		CloseShotUiAt = Now + 0.2;
	}
	else if (CloseShotUiAt > 0 && Now >= CloseShotUiAt)
	{
		CloseShotUiAt = -1;
		const FString File = CloseShotFile.IsEmpty() ? FString(TEXT("tc-close.png")) : CloseShotFile;
		FScreenshotRequest::RequestScreenshot(File, true, false);
		UE_LOG(LogTC, Log, TEXT("close shot: screenshot requested: %s"), *File);
	}

	// -TCCombatShotAfter: smoke, a tracer or shell in flight and a blast together
	if (CombatShotAfter >= 0 && !bCombatShot && InMatch >= CombatShotAfter && PC && bDrawn && Effects && Now - CombatShotTried >= 0.2)
	{
		CombatShotTried = Now;
		enum EClass { Cloud, Tracer, Blast, None };
		auto ClassOf = [](ETCFxLook L)
		{
			if (L == ETCFxLook::Smoke || L == ETCFxLook::DarkSmoke || L == ETCFxLook::Screen) return Cloud;
			if (L == ETCFxLook::Tracer || L == ETCFxLook::Rocket || L == ETCFxLook::Grenade) return Tracer;
			if (L == ETCFxLook::Fireball || L == ETCFxLook::Flash) return Blast; // an explosion or a gun's flash
			return None;
		};
		TArray<TPair<FVector2D, int32>> Pts;
		for (const FTCFxParticle& P : Effects->Fx.Particles()) if (P.Age >= 0.f) { const int32 C = ClassOf(P.Look); if (C != None) Pts.Add({ FVector2D(P.P.X, P.P.Y), C }); }
		for (const FTCFlightDraw& F : Effects->Fx.FlightsNow()) Pts.Add({ FVector2D(F.P.X, F.P.Z), Tracer }); // reference axes: (x, z) on the map
		// -TCCombatShotDamage: the held ground vehicles smoking from damage (their smoke level over 0.3)
		TArray<TPair<FVector2D, int32>> Smoking;
		if (bCombatShotDamage)
			for (const auto& U : State.Units)
				if (IsVehicle(UnitTable.Find(U.Value.Type)) && Effects->Fx.DamageLevel(U.Key) > 0.3) Smoking.Add({ FVector2D(U.Value.X, U.Value.Z), U.Key });
		auto SmokerNear = [&](const FVector2D& P) { for (const auto& V : Smoking) if (FVector2D::DistSquared(V.Key, P) < 25.0 * 25.0) return V.Value; return 0; };
		int32 BestScore = 0; bool bBestAll = false; FVector2D BestAt; int32 BestN[3] = {}, BestSmoker = 0;
		const int32 Step = FMath::Max(1, Pts.Num() / 300);
		for (int32 I = 0; I < Pts.Num(); I += Step)
		{
			if (Pts[I].Value == Cloud) continue; // centre on the action, not the middle of a smoke screen
			int32 N[3] = {};
			for (const auto& Q : Pts) if (FVector2D::DistSquared(Q.Key, Pts[I].Key) < 25.0 * 25.0) N[Q.Value]++;
			const int32 Smoker = bCombatShotDamage ? SmokerNear(Pts[I].Key) : 0;
			const bool bAll = N[Cloud] && N[Tracer] && N[Blast] && (!bCombatShotDamage || Smoker);
			const int32 Score = FMath::Min(N[Cloud], 60) + 8 * N[Tracer] + 2 * N[Blast] + (Smoker ? 40 : 0);
			if ((bAll && !bBestAll) || (bAll == bBestAll && Score > BestScore)) { BestScore = Score; bBestAll = bAll; BestAt = Pts[I].Key; BestN[0] = N[0]; BestN[1] = N[1]; BestN[2] = N[2]; BestSmoker = Smoker; }
		}
		const bool bGiveUp = InMatch >= CombatShotAfter + 90.0;
		if ((bBestAll && BestN[Tracer] >= 2) || (bGiveUp && BestScore > 0))
		{
			bCombatShot = true;
			PC->FocusOn(BestAt);
			PC->ZoomTo(32.f);
			if (Hud) Hud->bShowHUD = false; // a burst of frames of the fight alone, then one with the HUD
			CombatShotAt = Now + 0.05;
			CombatFrames = 0;
			FString Damage;
			if (BestSmoker) Damage = FString::Printf(TEXT("; vehicle %d (%s, seat %d, hp %.0f) smoking from damage, level %.2f, %d damage puffs so far"), BestSmoker, *State.Units[BestSmoker].Type, State.Units[BestSmoker].Owner, (double)State.Units[BestSmoker].Hp, Effects->Fx.DamageLevel(BestSmoker), Effects->Fx.DamagePuffs());
			else if (bCombatShotDamage) Damage = TEXT("; no damaged vehicle smoking within 25 m");
			UE_LOG(LogTC, Log, TEXT("combat shot: camera on (%.1f, %.1f) at 32 m, tick %d: within 25 m smoke %d, tracers and shells in flight %d, explosions and gun flashes %d%s%s"), BestAt.X, BestAt.Y, State.Tick, BestN[Cloud], BestN[Tracer], BestN[Blast],
				bBestAll ? TEXT("") : TEXT(" (gave up waiting for all of them: the best spot seen)"), *Damage);
		}
		else if (bGiveUp) { bCombatShot = true; UE_LOG(LogTC, Warning, TEXT("combat shot: no effects alive: no screenshot")); }
	}
	if (CombatShotAt > 0 && Now >= CombatShotAt && !FScreenshotRequest::IsScreenshotRequested())
	{
		// frames 1-8 without the HUD, 0.12 s apart; then the HUD back and the last one with it
		const FString File = CombatShotFile.IsEmpty() ? FString(TEXT("tc-combat.png")) : CombatShotFile;
		CombatFrames++;
		FString Name = File;
		if (CombatFrames <= 8) Name = FPaths::ChangeExtension(File, TEXT("")) + FString::Printf(TEXT("-%d-nohud.png"), CombatFrames);
		if (CombatFrames == 9 && Hud && !Hud->bShowHUD) { Hud->bShowHUD = true; CombatFrames--; CombatShotAt = Now + 0.1; return; }
		FScreenshotRequest::RequestScreenshot(Name, CombatFrames > 8, false);
		int32 Tracers = 0, Blasts = 0;
		for (const FTCFxParticle& P : Effects->Fx.Particles()) { Tracers += P.Look == ETCFxLook::Tracer ? 1 : 0; Blasts += P.Look == ETCFxLook::Fireball || P.Look == ETCFxLook::Flash ? 1 : 0; }
		UE_LOG(LogTC, Log, TEXT("combat shot: screenshot requested: %s (tracer sprites alive %d, shells in flight %d, explosions and flashes %d, particles %d, damage puffs so far %d)"), *Name, Tracers, Effects->Fx.FlightsNow().Num(), Blasts, Effects->Fx.Particles().Num(), Effects->Fx.DamagePuffs());
		CombatShotAt = CombatFrames >= 9 ? -1 : Now + 0.12;
	}

	// -TCWorldShotAfter: the spot with the most map pieces (houses, walls, hedges) and trees, near the own army
	if (WorldShotAfter >= 0 && !bWorldShot && InMatch >= WorldShotAfter && PC && bDrawn && State.Snapshots > 0 && Now - WorldShotTried >= 1.0)
	{
		WorldShotTried = Now;
		bWorldShot = !bWorldShotVisible || InMatch >= WorldShotAfter + 90.0;
		// -TCWorldShotVisible: only what stands in a cell this seat sees now
		auto InSight = [&](double X, double Z)
		{
			if (!bWorldShotVisible) return true;
			const int32 CXi = FMath::FloorToInt(X / FTCMatchState::CellMetres), CZi = FMath::FloorToInt(Z / FTCMatchState::CellMetres);
			return CXi >= 0 && CZi >= 0 && CXi < State.Map.W && CZi < State.Map.H && State.CellVisible(CZi * State.Map.W + CXi);
		};
		int32 Houses = 0;
		using namespace TCWorldCore;
		using namespace TCScatterCore;
		// 8 m bins: a roof or gable counts 3 (a house), a wall, hedge or sandbag piece 1, a tree or bush 2
		TMap<FIntPoint, double> Score;
		auto Bin = [](double X, double Z) { return FIntPoint(FMath::FloorToInt(X / 8.0), FMath::FloorToInt(Z / 8.0)); };
		int32 Pieces = 0, Trees = 0;
		for (int32 K = 0; K < WorldObjects.ChunkCount(); K++)
		{
			WorldObjects.ForEachPiece(K, [&](const FPiece& P)
			{
				Pieces++;
				const bool bHouse = P.Kind == EPiece::Roof || P.Kind == EPiece::Gable || P.Kind == EPiece::GableWood || P.Kind == EPiece::GableStone || P.Kind == EPiece::GableBrick;
				const bool bWall = P.Kind == EPiece::Stone || P.Kind == EPiece::Wall || P.Kind == EPiece::Shrub || P.Kind == EPiece::Leaves || P.Kind == EPiece::Bag;
				// -TCWorldShotVisible (a village in sight) weighs houses up: a roof or gable 8, a wall 1, a tree 1
				if ((bHouse || bWall) && InSight(P.X, P.Z)) { Score.FindOrAdd(Bin(P.X, P.Z)) += bHouse ? (bWorldShotVisible ? 8.0 : 3.0) : 1.0; Houses += bHouse ? 1 : 0; }
			});
			WorldObjects.ForEachProp(K, [&](const FCandidate& C, const FPlaced& P)
			{
				if (!P.bShown) return;
				if ((P.Kind == EProp::Deciduous || P.Kind == EProp::Poplar || P.Kind == EProp::Pine || P.Kind == EProp::Bush) && InSight(P.X, P.Z)) { Trees++; Score.FindOrAdd(Bin(P.X, P.Z)) += bWorldShotVisible ? 1.0 : 2.0; }
			});
		}
		double CX = 0, CZ = 0;
		const bool bArmy = ArmyIds(CX, CZ).Num() > 0;
		double BestScore = 0;
		FIntPoint BestBin(0, 0);
		for (int32 Pass = 0; Pass < 2 && BestScore <= 0; Pass++)
			for (const auto& P : Score)
			{
				// a 32 m window (4 x 4 bins) from this corner
				const FVector2D Centre((P.Key.X + 2) * 8.0, (P.Key.Y + 2) * 8.0);
				if (Pass == 0 && bArmy && FVector2D::Distance(Centre, FVector2D(CX, CZ)) > 160.0) continue;
				double Sum = 0;
				for (int32 DX = 0; DX < 4; DX++) for (int32 DY = 0; DY < 4; DY++) if (const double* V = Score.Find(P.Key + FIntPoint(DX, DY))) Sum += *V;
				if (Sum > BestScore) { BestScore = Sum; BestBin = P.Key; }
			}
		if (bWorldShotVisible && !Houses && !bWorldShot) { /* no house in sight yet: try again in a second */ }
		else if (BestScore > 0)
		{
			bWorldShot = true;
			const FVector2D At((BestBin.X + 2) * 8.0, (BestBin.Y + 2) * 8.0);
			PC->FocusOn(At);
			PC->ZoomTo(45.f);
			// slice 12: nothing selected (no selection rings over the houses), and the first frame without the HUD
			PC->SelectOwn(TArray<int32>());
			if (Hud) Hud->bShowHUD = false;
			WorldShotAt = Now + 0.6;
			const FString Counted = bWorldShotVisible ? FString::Printf(TEXT(" (counted: those in sight now, %d house roof and gable pieces)"), Houses) : FString();
			UE_LOG(LogTC, Log, TEXT("world shot: camera on (%.1f, %.1f) at 45 m, score %.0f; this side holds %d pieces and %d trees and bushes%s; %s"), At.X, At.Y, BestScore, Pieces, Trees, *Counted, *WorldView.StatusLine());
		}
		else if (bWorldShot) UE_LOG(LogTC, Warning, TEXT("world shot: no house, wall or tree known: no screenshot (%d pieces)"), Pieces);
	}
	if (WorldShotAt > 0 && Now >= WorldShotAt && !FScreenshotRequest::IsScreenshotRequested())
	{
		WorldShotAt = -1;
		const FString File = WorldShotFile.IsEmpty() ? FString(TEXT("tc-world.png")) : WorldShotFile;
		if (Hud && !Hud->bShowHUD && !bWorldBareTaken)
		{
			// slice 12: the view alone first (<file>-nohud.png); the HUD comes back after that frame is saved
			const FString Bare = FPaths::ChangeExtension(File, TEXT("")) + TEXT("-nohud.png");
			FScreenshotRequest::RequestScreenshot(Bare, false, false);
			bWorldBareTaken = true;
			WorldShotAt = Now + 0.2;
			UE_LOG(LogTC, Log, TEXT("world shot: screenshot requested without the HUD: %s"), *Bare);
		}
		else if (Hud && !Hud->bShowHUD)
		{
			Hud->bShowHUD = true; // drawn again from the next frame; the shot with it a moment later
			WorldShotAt = Now + 0.3;
		}
		else
		{
			FScreenshotRequest::RequestScreenshot(File, true, false);
			UE_LOG(LogTC, Log, TEXT("world shot: screenshot requested: %s"), *File);
		}
	}

	// -TCHudShotAfter: the own army selected so the unit panel shows its rows and tags
	if (HudShotAfter >= 0 && !bHudShot && InMatch >= HudShotAfter && PC && bDrawn && State.Snapshots > 0)
	{
		bHudShot = true;
		double CX, CZ;
		const TArray<int32> Ids = ArmyIds(CX, CZ);
		PC->SelectOwn(Ids);
		HudShotAt = Now + 1.0;
		UE_LOG(LogTC, Log, TEXT("hud shot: %d own units selected"), Ids.Num());
	}
	if (HudShotAt > 0 && Now >= HudShotAt)
	{
		HudShotAt = -1;
		const FString File = HudShotFile.IsEmpty() ? FString(TEXT("tc-hud.png")) : HudShotFile;
		FScreenshotRequest::RequestScreenshot(File, true, false);
		TArray<FString> Shown;
		if (Hud) for (const FString& L : Hud->ButtonLabelList()) Shown.Add(TC_T(L));
		UE_LOG(LogTC, Log, TEXT("hud shot: screenshot requested: %s; buttons as drawn: %s"), *File, *FString::Join(Shown, TEXT(" | ")));
	}
}
