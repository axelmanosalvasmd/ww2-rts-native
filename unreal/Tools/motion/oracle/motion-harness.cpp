// Oracle side B: the C++ ports (Client/TCInfantryRig.cpp AnimateSquad and FigureMuzzles, Client/TCUnitMotion.cpp
// MoveModel, MoveWheels, WheelAngle, the track table, Stride, PlaneTick and the corpse rules), compiled without the
// engine, run on motion-reference.mjs's scenario.txt (stdin). Prints the same records as reference.jsonl.
#define TC_INFANTRY_RIG_STANDALONE 1
#define TC_UNIT_MOTION_STANDALONE 1
#include "../../../Source/ThreeCrossroads/Client/TCInfantryRig.cpp"
#include "../../../Source/ThreeCrossroads/Client/TCUnitMotion.cpp"
#include <cstdio>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace TCInfantryRigCore;
using namespace TCUnitMotionCore;

static double Ground(void*, double X, double Z) { return 0.4 * std::sin(0.13 * X) + 0.3 * std::cos(0.11 * Z) + 2; }
static double GroundXZ(double X, double Z) { return Ground(nullptr, X, Z); }
static void P(double V) { std::printf("%.17g", V); }
static void PA(const double* V, int N) { std::printf("["); for (int I = 0; I < N; I++) { if (I) std::printf(","); P(V[I]); } std::printf("]"); }
static bool Next(std::istringstream& In, std::string& Cmd, std::string& Line)
{
	if (!std::getline(std::cin, Line)) return false;
	In.clear(); In.str(Line); In >> Cmd;
	return true;
}

int main()
{
	std::string Line, Cmd;
	std::istringstream In;
	std::vector<float> Table(static_cast<size_t>(Samples) * Rows * 4, 0.f);
	int Loops = 0;
	while (Next(In, Cmd, Line))
	{
		if (Cmd == "tracks")
		{
			// register(loop): one row per distinct point list (FTCTrackPaths::Register keeps the same rule)
			int N = 0; In >> N;
			std::vector<std::vector<double>> Seen;
			for (int L = 0; L < N; L++)
			{
				std::string C2; Next(In, C2, Line);
				int Count = 0; In >> Count;
				std::vector<double> Pts(Count * 2);
				for (double& V : Pts) In >> V;
				bool bKnown = false;
				for (const auto& K : Seen) bKnown = bKnown || K == Pts;
				if (bKnown) continue;
				Seen.push_back(Pts);
				TrackRegister(Table.data(), Loops++, Pts.data(), Count);
			}
			std::printf("{\"kind\":\"trackrows\",\"rows\":[");
			for (int R = 0; R < Loops; R++) std::printf(R ? ",%d" : "%d", R);
			std::printf("]}\n");
		}
		else if (Cmd == "tv")
		{
			double Pos[3], D[4], Travel, Turn, Out[3];
			In >> Pos[0] >> Pos[1] >> Pos[2] >> D[0] >> D[1] >> D[2] >> D[3] >> Travel >> Turn;
			TrackVertex(Table.data(), Pos, D, Travel, Turn, Out);
			std::printf("{\"kind\":\"tv\",\"p\":"); PA(Out, 3); std::printf("}\n");
		}
		else if (Cmd == "wheel")
		{
			double R, Side, Travel, Turn; In >> R >> Side >> Travel >> Turn;
			std::printf("{\"kind\":\"wheel\",\"a\":"); P(WheelAngle(R, Side, Travel, Turn)); std::printf("}\n");
		}
		else if (Cmd == "squad")
		{
			std::string Type, Figure; int Fac, Id, Men, Digger, MoveFire;
			In >> Type >> Figure >> Fac >> Id >> Men >> Digger >> MoveFire;
			std::vector<FManIn> M(Men);
			std::vector<FAnimMan> O(Men);
			std::vector<std::vector<FV>> Tips(Men);
			std::vector<const FV*> TipPtr(Men);
			std::vector<int> NumTips(Men);
			FAnimSquad Sq;
			for (int I = 0; I < Men; I++)
			{
				std::string C2, Kit; Next(In, C2, Line);
				int Index; double SX, SZ, Sc; In >> Index >> SX >> SZ >> Sc >> Kit;
				M[I].SlotX = SX; M[I].SlotZ = SZ; M[I].Scale = Sc; M[I].Index = I;
				FCtx Ctx; Ctx.Type = ParseType(Figure.c_str()); Ctx.Fac = Fac; Ctx.Job = SoldierKit(Ctx.Type, I, Fac);
				std::string Job = std::string(BaseName(Ctx.Job.Base)) + (Ctx.Job.Variant ? std::string(1, Ctx.Job.Variant) : std::string());
				if (Job != Kit) std::fprintf(stderr, "kit differs: %s %d: %s vs %s\n", Figure.c_str(), I, Job.c_str(), Kit.c_str());
				FV Buf[MaxTips];
				const int N = FigureMuzzles(Ctx, Buf);
				Tips[I].assign(Buf, Buf + N);
				TipPtr[I] = N ? Tips[I].data() : nullptr; NumTips[I] = N;
			}
			int Step = 0;
			while (Next(In, Cmd, Line) && Cmd == "step")
			{
				double Dt; FAnimIn V; int Vis, Det, Tgt, Garr, Alive;
				In >> Dt >> V.RootX >> V.RootY >> V.RootZ >> V.RootYaw >> Vis >> Det >> V.EyeX >> V.EyeY >> V.EyeZ >> V.Flags >> V.Supp >> V.Cover >> Tgt >> V.Aim >> Garr >> Alive;
				V.VX = V.RootX; V.VZ = V.RootZ; V.bVisible = Vis; V.bDetailed = Det; V.bTarget = Tgt; V.bMoveFire = MoveFire; V.bGarrisoned = Garr;
				V.Id = Id; V.Lod = 110; V.bDigger = Digger; V.Ground = Ground;
				for (int I = 0; I < Men; I++) M[I].bVisible = I < Alive && !Garr;
				AnimateSquad(Sq, V, M.data(), O.data(), TipPtr.data(), NumTips.data(), Men, Dt);
				std::printf("{\"kind\":\"squad\",\"type\":\"%s\",\"step\":%d,\"far\":%s,\"men\":[", Type.c_str(), Step++, Sq.bFar ? "true" : "false");
				const int W = Digger ? 27 : 23;
				for (int I = 0; I < Men; I++)
				{
					const FAnimMan& A = O[I];
					std::printf(I ? ",{\"p\":" : "{\"p\":"); const double Pp[3] = { A.X, A.Y, A.Z }; PA(Pp, 3);
					std::printf(",\"ry\":"); P(A.Yaw); std::printf(",\"lean\":"); P(A.Blend.Lean); std::printf(",\"sy\":"); P(A.Blend.HeightScale);
					const double Node[2] = { A.Blend.ShiftX, A.Blend.ShiftY }; std::printf(",\"node\":"); PA(Node, 2);
					std::printf(",\"w\":"); PA(A.Blend.Key + 1, W);
					std::printf(",\"muzzle\":"); if (A.bMuzzle) { const double Mz[3] = { A.Muzzle.X, A.Muzzle.Y, A.Muzzle.Z }; PA(Mz, 3); } else std::printf("null");
					const double Mm[5] = { M[I].X, M[I].Z, M[I].MYaw, M[I].Phase, M[I].Blend }; std::printf(",\"m\":"); PA(Mm, 5);
					std::printf("}");
				}
				std::printf("]}\n");
			}
		}
		else if (Cmd == "vehicle")
		{
			std::string Type; int Id; In >> Type >> Id;
			FHull H; FWheels Wh; FKaiju K;
			const bool bKaiju = Type == "kaiju";
			int Step = 0;
			while (Next(In, Cmd, Line) && Cmd == "vstep")
			{
				double Dt, X, Y, Z, Yaw; int Vis; In >> Dt >> X >> Y >> Z >> Yaw >> Vis;
				std::printf("{\"kind\":\"vehicle\",\"type\":\"%s\",\"step\":%d", Type.c_str(), Step++);
				if (bKaiju)
				{
					Stride(K, X, Z, Dt);
					const double A[9] = { K.LegRotZ[0], K.LegRotZ[1], K.LegY[0], K.LegY[1], K.HullY, K.HullRotX, K.TurretY, K.TailY, K.TailRotY };
					std::printf(",\"kaiju\":"); PA(A, 9);
				}
				else
				{
					MoveModel(H, HullOf(Type.c_str()), Wheeled(Type.c_str()), Large(Type.c_str()), Id, X, Z, Yaw, Vis, Dt);
					MoveWheels(Wh, X, Y, Z, Yaw, Vis, Dt);
					const double B[3] = { H.RotX, H.RotZ, H.PosY }, Wv[2] = { Wh.Travel, Wh.Turn };
					std::printf(",\"body\":"); PA(B, 3); std::printf(",\"wheels\":"); PA(Wv, 2);
				}
				std::printf("}\n");
			}
		}
		else if (Cmd == "plane")
		{
			int Id, N; In >> Id >> N;
			FPlane Pl; Pl.NumProps = N;
			for (int I = 0; I < N; I++) In >> Pl.PropAngle[I] >> Pl.PropSpin[I];
			int Step = 0;
			while (Next(In, Cmd, Line) && Cmd == "pstep")
			{
				double Dt, Rot, Now; int Vis; In >> Dt >> Rot >> Vis >> Now;
				PlaneTick(Pl, Id, Rot, Vis, Dt, Now);
				const double B[3] = { Pl.RotX, Pl.RotZ, Pl.Bank };
				std::printf("{\"kind\":\"plane\",\"step\":%d,\"body\":", Step++); PA(B, 3); std::printf(",\"props\":"); PA(Pl.PropAngle, Pl.NumProps); std::printf("}\n");
			}
		}
		else if (Cmd == "bodies")
		{
			std::vector<FCorpseBody> Bodies;
			FCorpseRulesCore R;
			int Step = 0;
			double Time = 0;
			std::vector<double> Rands;
			size_t RandAt = 0;
			while (Next(In, Cmd, Line) && Cmd != "end")
			{
				if (Cmd == "add")
				{
					FCorpseBody B; In >> B.X >> B.Y >> B.Z >> B.Yaw;
					const int Drop = BeforeAdd(Bodies.data(), static_cast<int>(Bodies.size()), R);
					if (Drop >= 0) { Bodies[Drop] = Bodies.back(); Bodies.pop_back(); }
					Bodies.push_back(B);
				}
				else if (Cmd == "update")
				{
					double Dt; In >> Dt >> Time;
					std::string Tag; Rands.clear(); RandAt = 0;
					if (In >> Tag) { double V; while (In >> V) Rands.push_back(V); }
					auto Blast = [&](double X, double Z, double& HX, double& HZ, double& HS) {
						if (Time > 3 && Time < 3.5 && std::hypot(X - 20, Z - 20) < 6) { HX = 20; HZ = 20; HS = 3; return true; }
						return false;
					};
					auto Rand = [&]() { return RandAt < Rands.size() ? Rands[RandAt++] : 0.5; };
					for (int I = static_cast<int>(Bodies.size()) - 1; I >= 0; I--)
						if (!StepBody(Bodies[I], R, Dt, Blast, Rand, GroundXZ, true)) { Bodies[I] = Bodies.back(); Bodies.pop_back(); }
					if (RandAt != Rands.size()) std::fprintf(stderr, "bodies: %zu draws recorded, %zu used\n", Rands.size(), RandAt);
					Step++;
				}
				else if (Cmd == "snap")
				{
					std::printf("{\"kind\":\"bodies\",\"step\":%d,\"count\":%zu,\"list\":[", Step - 1, Bodies.size());
					for (size_t I = 0; I < Bodies.size(); I++)
					{
						const FCorpseBody& B = Bodies[I];
						// place(): Euler (rx, yaw, rz) 'YXZ' -> quaternion -> compose(at, q, 1), as three.js does it
						const double X = B.bFly ? B.RX : 0, Y = B.Yaw, Z = B.bFly ? B.RZ : 0;
						const double C1 = std::cos(X / 2), C2 = std::cos(Y / 2), C3 = std::cos(Z / 2), S1 = std::sin(X / 2), S2 = std::sin(Y / 2), S3 = std::sin(Z / 2);
						const double QX = S1 * C2 * C3 + C1 * S2 * S3, QY = C1 * S2 * C3 - S1 * C2 * S3, QZ = C1 * C2 * S3 - S1 * S2 * C3, QW = C1 * C2 * C3 + S1 * S2 * S3;
						const double X2 = QX + QX, Y2 = QY + QY, Z2 = QZ + QZ, XX = QX * X2, XY = QX * Y2, XZ = QX * Z2, YY = QY * Y2, YZ = QY * Z2, ZZ = QZ * Z2, WX = QW * X2, WY = QW * Y2, WZ = QW * Z2;
						const double M[16] = { 1 - (YY + ZZ), XY + WZ, XZ - WY, 0, XY - WZ, 1 - (XX + ZZ), YZ + WX, 0, XZ + WY, YZ - WX, 1 - (XX + YY), 0, B.X, B.Y + BodySink(B, R), B.Z, 1 };
						std::printf(I ? ",{\"m\":" : "{\"m\":"); PA(M, 16); std::printf(",\"fade\":"); P(BodyFade(B, R)); std::printf("}");
					}
					std::printf("]}\n");
				}
			}
		}
	}
	return 0;
}
