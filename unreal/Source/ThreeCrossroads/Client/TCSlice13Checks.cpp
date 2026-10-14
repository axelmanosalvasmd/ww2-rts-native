// Slice 13 verification switch of ATCGameMode (see TCGameMode.h): a camera tour for the terrain side by sides. The
// camera is moved directly (FocusOn, ZoomTo, the heading), as the earlier slices' shot switches do; nothing here is a
// player or a physical mouse or keyboard. The reference half of each pair is taken by
// unreal/Tools/compare/tour-reference.mjs from the same tour file (the pinned browser client, its camera state set).
//
// -TCTourFile=<file> -TCTourDir=<dir>: one pose per line, "name seconds x z distance yaw" ('#' starts a comment):
//   seconds   match time: the server's tick / 20 (shared/sim.js TICK), so a spectator that joined late and the browser
//             tour (which reads the same tick) shoot at the same moment of the same match
//   x z       the camera's target in reference metres, or "frontier" for x (z ignored): the first cell this side does
//             not know on the line from the own army toward the map's centre, stepped back 6 m onto known ground
//             or "unit:<type>" for x (round 14, z ignored): the living unit of that type with the lowest id owned by this
//             seat (seat 0 for a spectator), so both halves frame the same moving vehicle or squad
//   distance  metres, or wF: the fraction F of the widest view (client/camera.js wide, measured for this map and window)
//   yaw       the native heading in degrees (-90 looks north; the browser's yaw is -(yaw + 90) degrees)
//   [hud]     optional 7th field (round 14): keep the HUD in the shot (the goal line, the panels, the paper war map)
// At each pose the HUD is hidden (unless "hud"), and 2.5 s later (the view settles, the paint and textures stream) <dir>/<name>.png is
// saved; the log line has the camera as set, the frame times over those 2.5 s and the terrain status.
#include "Client/TCGameMode.h"
#include "Client/TCPlayerController.h"
#include "Client/TCHUD.h"
#include "World/TCTerrain.h"
#include "ThreeCrossroads.h"
#include "Engine/World.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Misc/App.h"
#include "UnrealClient.h"

void ATCGameMode::RunSlice13Switches(double Now)
{
	if (TourFile.IsEmpty() || TourDone) return;
	ATCPlayerController* PC = Cast<ATCPlayerController>(GetWorld()->GetFirstPlayerController());
	if (!PC || !Terrain) return;
	if (!bTourRead)
	{
		bTourRead = true;
		TArray<FString> Lines;
		if (!FFileHelper::LoadFileToStringArray(Lines, *TourFile)) { UE_LOG(LogTC, Error, TEXT("tour: cannot read %s"), *TourFile); TourDone = true; return; }
		for (FString L : Lines)
		{
			int32 Hash;
			if (L.FindChar('#', Hash)) L.LeftInline(Hash);
			TArray<FString> F;
			L.ParseIntoArrayWS(F);
			if (F.Num() < 6) continue;
			FTourPose P;
			P.Name = F[0]; P.At = FCString::Atod(*F[1]);
			P.bFrontier = F[2] == TEXT("frontier");
			P.X = FCString::Atod(*F[2]); P.Z = FCString::Atod(*F[3]);
			P.bWideShare = F[4].StartsWith(TEXT("w"));
			P.Dist = FCString::Atod(P.bWideShare ? *F[4].Mid(1) : *F[4]);
			P.Yaw = FCString::Atod(*F[5]);
			if (F[2].StartsWith(TEXT("unit:"))) P.UnitType = F[2].Mid(5);
			P.bHud = F.Num() > 6 && F[6] == TEXT("hud");
			Tour.Add(P);
		}
		UE_LOG(LogTC, Log, TEXT("tour: %d poses from %s into %s"), Tour.Num(), *TourFile, *TourDir);
	}
	if (TourStep >= Tour.Num()) { TourDone = true; UE_LOG(LogTC, Log, TEXT("tour: done (%d shots)"), TourShots); if (AHUD* Hud = PC->GetHUD()) Hud->bShowHUD = true; return; }
	const double InMatch = State.bHasMatch && State.Snapshots > 0 ? State.Tick / 20.0 : -1; // shared/sim.js: 20 ticks a second
	FTourPose& P = Tour[TourStep];
	if (TourShotAt < 0)
	{
		if (InMatch < P.At || FScreenshotRequest::IsScreenshotRequested()) return;
		if (P.bWideShare && !PC->HasWideView()) return; // the widest view is measured once the map is drawn
		FVector2D Focus(P.X, P.Z);
		if (P.bFrontier)
		{
			double CX, CZ;
			if (!ArmyIds(CX, CZ).Num())
			{
				// round 14: a spectator owns nothing; it watches seat 0 (server.js), whose ground army the browser tour uses
				int32 N = 0; CX = CZ = 0;
				for (const auto& U : State.Units)
				{
					const FTCUnitDef* D = UnitTable.Find(U.Value.Type);
					if (U.Value.Owner == 0 && D && U.Value.Hp > 0 && !D->bStructure && !D->bAir && !D->bRail && U.Value.Type != TEXT("truck")) { CX += U.Value.X; CZ += U.Value.Z; N++; }
				}
				if (N) { CX /= N; CZ /= N; }
			}
			const FVector2D From(CX, CZ), To(State.Map.W * FTCMatchState::CellMetres / 2, State.Map.H * FTCMatchState::CellMetres / 2);
			const double Len = FVector2D::Distance(From, To);
			const FVector2D Dir = Len > 1 ? (To - From) / Len : FVector2D(1, 0);
			Focus = From;
			for (double S = 0; S <= FMath::Max(Len, 1.0) + 400; S += 1.0)
			{
				const FVector2D Q = From + Dir * S;
				const int32 Cx = FMath::FloorToInt(Q.X / FTCMatchState::CellMetres), Cz = FMath::FloorToInt(Q.Y / FTCMatchState::CellMetres);
				if (Cx < 0 || Cz < 0 || Cx >= State.Map.W || Cz >= State.Map.H) break;
				if (!State.Map.Known[Cz * State.Map.W + Cx]) { Focus = From + Dir * FMath::Max(0.0, S - 6.0); break; }
			}
		}
		if (!P.UnitType.IsEmpty())
		{
			// round 14: the lowest id of that type on this seat (seat 0 for a spectator, as the browser tour takes it)
			const int32 Seat = State.bSpectator || State.You < 0 ? 0 : State.You;
			int32 Best = MAX_int32;
			for (const auto& U : State.Units)
				if (U.Value.Type == P.UnitType && U.Value.Owner == Seat && U.Value.Hp > 0 && U.Key < Best) { Best = U.Key; Focus = FVector2D(U.Value.X, U.Value.Z); }
			if (Best == MAX_int32) { UE_LOG(LogTC, Warning, TEXT("tour: %s: no living %s of seat %d; the pose's own target"), *P.Name, *P.UnitType, Seat); }
			else P.UnitId = Best;
		}
		const float Dist = P.bWideShare ? (float)(PC->WideDistance() * P.Dist) : (float)P.Dist;
		PC->FocusOn(Focus);
		PC->SetViewYawForChecks((float)P.Yaw);
		PC->ZoomTo(Dist);
		if (AHUD* Hud = PC->GetHUD()) Hud->bShowHUD = P.bHud;
		P.X = Focus.X; P.Z = Focus.Y; P.Asked = Dist;
		TourShotAt = Now + 2.5;
		TourFrames = FFrameTimes();
		return;
	}
	// round 14: a unit pose follows its unit while the view settles, so a moving vehicle stays in the middle of the shot
	if (const FTCUnit* U = P.UnitId >= 0 ? State.Units.Find(P.UnitId) : nullptr) PC->FocusOn(FVector2D(U->X, U->Z));
	if (Now < TourShotAt) { TourFrames.Add(FApp::GetDeltaTime() * 1000.0); return; }
	if (FScreenshotRequest::IsScreenshotRequested()) return;
	const FString File = FPaths::Combine(TourDir, P.Name + TEXT(".png"));
	FScreenshotRequest::RequestScreenshot(File, false, false);
	TourShots++;
	int32 Verts = 0, Unowned = 0;
	const int32 OnUnknown = Terrain->DrawnOnUnknown(State, &Verts, &Unowned);
	if (const FTCUnit* U = P.UnitId >= 0 ? State.Units.Find(P.UnitId) : nullptr)
	{
		UE_LOG(LogTC, Log, TEXT("tour: %s: unit %d (%s) at (%.1f, %.1f), row tail %s"), *P.Name, P.UnitId, *U->Type, U->X, U->Z,
				*FString::JoinBy(U->Tail, TEXT(" "), [](float V) { return FString::Printf(TEXT("%.2f"), V); }));
	}
	UE_LOG(LogTC, Log, TEXT("tour: %s: target (%.1f, %.1f) yaw %.0f distance %.1f m (asked %.1f, widest %.1f) at match time %.1f s | frames %lld mean %.2f ms worst %.1f ms | cells known %d of %d, drawn %d, relief vertices %d, on unknown cells %d | water %d triangles, apron %d columns | weather %s"),
		*File, PC->ViewFocus().X, PC->ViewFocus().Y, PC->ViewYaw(), PC->ViewDistance(), P.Asked, PC->HasWideView() ? PC->WideDistance() : -1.f, State.Tick / 20.0,
		TourFrames.N, TourFrames.Mean(), TourFrames.Worst, State.Map.Known.CountSetBits(), State.Map.W * State.Map.H, Terrain->CellsDrawn(), Verts, OnUnknown,
		Water.TrianglesDrawn(), Apron.Columns(), *State.Weather.Now);
	TourShotAt = -1;
	TourStep++;
}
