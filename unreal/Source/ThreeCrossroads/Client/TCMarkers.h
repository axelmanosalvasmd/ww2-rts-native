// Map markers drawn on the ground and over units, as the pinned reference draws them (round 17). Ported from:
//   client/overlay.js     Builder.ring and Builder.line (the ribbon: a width in metres that never drops under a pixel
//                         count, the dark edge outside the core, the fill of a ring's inside, dashes fitted to a ring)
//                         and the fragment rule (body colour, edge colour 0.012 0.016 0.014 at 0.55 of the opacity,
//                         inside at the fill alpha);
//   client/markers.js     capturePoint (ring over a faint wash, progress track and arc, contested and flip rings),
//                         selectionRing's weapon range rings (dashed, RANGE_DASH), hqRing, nodeSquare (four corner
//                         brackets), label (the text tags: gunmetal plate, hairline, fixed screen size, depth fade);
//   client/main.js        makeUnit (range rings [w.range, w.minRange], veterancy stars), rangeRings (only for a small
//                         selection), buildHQ (the tinted reinforce zone and ring cut at the map edge, the "<name> HQ"
//                         tag), nodeMark and the snapshot's nodes (brackets and the "+<rate> MP/s" tag);
//   client/world-regions.js the World region marker (capturePoint with its text tag, the name tag above it).
//
// The first part (TCMarkersCore) is plain C++ with no engine types: the geometry in metres (ring and line vertices,
// dashes, clipping at the map edge) and the reference's constants, so a standalone harness can check it by defining
// TC_MARKERS_CORE_ONLY. The second part draws on the HUD's canvas through a projection the HUD gives (FTCMarkerPainter)
// and keeps the snapshot's resource nodes (FTCMarkers). The canvas has no depth test: markers the browser depth tests
// (capture rings, HQ ring) are drawn over models here; the range rings are drawn over everything on both sides.
#pragma once

#include <cmath>
#include <cstdint>
#include <vector>

namespace TCMarkersCore
{
	inline constexpr double Pi = 3.14159265358979323846;
	inline constexpr double CellMetres = 2.0;         // shared/sim.js CELL
	inline constexpr double PointRadius = 8.0;        // shared/sim.js CFG.pointRadius
	inline constexpr double ReinforceRadius = 15.0;   // shared/sim.js CFG.reinforceRadius
	inline constexpr int ProgressSegments = 64;       // markers.js PROG_SEGS

	// overlay.js FRAG_END: the colour outside the core and its alpha factor
	inline constexpr double EdgeRgb[3] = { 0.012, 0.016, 0.014 };
	inline constexpr double EdgeAlpha = 0.55;

	// markers.js colours (sRGB hex)
	inline constexpr uint32_t White = 0xf4f2ea;       // WHITE
	inline constexpr uint32_t NeutralRing = 0xe6e0d0; // NEUTRAL_RING
	inline constexpr uint32_t NeutralFlag = 0x8f8a7c; // NEUTRAL_FLAG
	inline constexpr uint32_t Brass = 0xd2a849;       // the flip and contested rings, the MP node brackets
	inline constexpr uint32_t FuelNode = 0xe07a30;    // nodeMark: a fuel node's brackets
	inline constexpr uint32_t Chalk = 0xf2ecdc;       // CHALK: buildHQ lerps the zone toward it
	inline constexpr uint32_t Star = 0xffd24a;        // makeUnit: veterancy stars
	inline constexpr double RangeDash[2] = { 2.4, 1.3 }; // RANGE_DASH: period and lit length, metres

	// overlay.js ribbon options (Builder.ring and Builder.line): width in metres, minimum half-width in pixels, where
	// the dark edge starts (0..1 of the half-width), lift over the ground, fill (rings), dashes fitted to a closed ring.
	struct FRibbon
	{
		double W = 0.3, Min = 1.1, Core = 0.74, Y = 0.1;
		bool bFill = false;
		double Arc = 1.0;        // fraction of the circle, from north clockwise
		int Segs = -1;           // -1: max(40, min(360, ceil(2 pi r / 1.1)))
		double DashPeriod = 0.0; // fit a whole number of dashes of this period into the ring
		double Step = 0.0;       // line: longest segment (a vertex every so often, for draping)
		bool bClosed = false;    // line
	};

	// How a ribbon is painted: overlayMat({ color, opacity, fill, dash }).
	struct FLook
	{
		uint32_t Hex = 0xffffff;
		double Opacity = 1.0;
		double Fill = 0.0;                  // alpha of a ring's inside (times Opacity)
		double DashPeriod = 0.0, DashOn = 0.0; // uDash: [period, lit] in metres (0: solid)
	};

	// overlay.js FRAG_BODY and FRAG_END at one point across a ribbon: V the side value (-1 .. 1), Aa = fwidth(v) (the
	// change of v over one pixel, at least 1e-3), Core where the dark edge starts. OutBody: how much of the body colour
	// (else the edge colour), OutAlpha: the alpha factor before the material's opacity (band * dash * mix(0.55, 1, body);
	// dashes apart). The fragment shader runs at each pixel's centre, so sampling this per pixel is what the browser shows.
	inline double GlslSmoothstep(double E0, double E1, double X)
	{
		const double T = E1 == E0 ? (X < E0 ? 0.0 : 1.0) : (X - E0) / (E1 - E0), C = T < 0 ? 0 : T > 1 ? 1 : T;
		return C * C * (3 - 2 * C);
	}
	inline void RibbonProfile(double V, double Aa, double Core, double& OutBody, double& OutAlpha)
	{
		const double Lv = std::fabs(V), A = Aa > 1e-3 ? Aa : 1e-3;
		const double Band = 1.0 - GlslSmoothstep(1.0 - A, 1.0, Lv);
		const double Body = std::fmax(1.0 - GlslSmoothstep(Core - A, Core, Lv), Lv >= 1.0 ? 1.0 : 0.0);
		OutBody = Body;
		OutAlpha = Band * (EdgeAlpha + (1.0 - EdgeAlpha) * Body);
	}
	// Strips drawn across a ribbon to sample that profile (each pixel centre falls in one strip)
	inline constexpr int ProfileStrips = 12;

	// One vertex of a ribbon's centre line: where it is, the offset direction for the half-width (a line's is
	// stretched through its corners), and its distance along (dashes).
	struct FVertex { double X = 0, Z = 0, Nx = 0, Nz = 0, U = 0; };

	inline int RingSegments(double R, int Segs)
	{
		if (Segs > 0) return Segs;
		const int N = (int)std::ceil(Pi * 2.0 * R / 1.1);
		return N < 40 ? 40 : N > 360 ? 360 : N;
	}

	// overlay.js ring: n + 1 vertices around (Ox, Oz), clockwise from north (sin a, -cos a), the arc's fraction of the
	// circle; U is a * r * k with k fitting a whole number of DashPeriod into the circle.
	inline std::vector<FVertex> Ring(double R, const FRibbon& O, double Ox = 0.0, double Oz = 0.0)
	{
		const double Full = Pi * 2.0 * (O.Arc < 1.0 ? O.Arc : 1.0);
		const int N = RingSegments(R, O.Segs);
		double K = 1.0;
		if (O.DashPeriod > 0.0)
		{
			const double C = Pi * 2.0 * R, Whole = std::round(C / O.DashPeriod);
			K = ((Whole < 1.0 ? 1.0 : Whole) * O.DashPeriod) / C;
		}
		std::vector<FVertex> V;
		V.reserve((size_t)N + 1);
		for (int I = 0; I <= N; I++)
		{
			const double A = (double)I / N * Full, Sx = std::sin(A), Sz = -std::cos(A);
			V.push_back({ Ox + Sx * R, Oz + Sz * R, Sx, Sz, A * R * K });
		}
		return V;
	}

	// The first Count segments of a ring built with Segs segments (markers.js capturePoint's progress arc: the geometry
	// is the whole PROG_SEGS ring and setDrawRange shows k segments).
	inline std::vector<FVertex> RingFirstSegments(double R, const FRibbon& O, int Count, double Ox = 0.0, double Oz = 0.0)
	{
		std::vector<FVertex> V = Ring(R, O, Ox, Oz);
		const size_t Keep = Count <= 0 ? 0 : (size_t)Count + 1;
		if (V.size() > Keep) V.resize(Keep);
		return V;
	}

	// overlay.js FRAG_BODY's dash: lit while mod(u, period) < lit (soft ends left out).
	inline bool DashLit(double U, double Period, double Lit)
	{
		if (!(Period > 0.0)) return true;
		const double P = std::fmod(U, Period);
		return (P < 0 ? P + Period : P) < Lit;
	}

	// The lit stretches of a dashed ring as angle ranges (radians from north, clockwise): with U = a * r * k, dash m is
	// lit for U in [m * period, m * period + lit).
	struct FSpan { double A0 = 0, A1 = 0; };
	inline std::vector<FSpan> RingDashes(double R, double Period, double Lit, double FitPeriod)
	{
		std::vector<FSpan> Out;
		if (!(R > 0.0) || !(Period > 0.0)) return Out;
		const double C = Pi * 2.0 * R;
		double K = 1.0;
		if (FitPeriod > 0.0) { const double Whole = std::round(C / FitPeriod); K = ((Whole < 1.0 ? 1.0 : Whole) * FitPeriod) / C; }
		const double UEnd = Pi * 2.0 * R * K;
		for (double U0 = 0.0; U0 < UEnd - 1e-9; U0 += Period)
		{
			const double U1 = U0 + Lit < UEnd ? U0 + Lit : UEnd;
			Out.push_back({ U0 / (R * K), U1 / (R * K) });
		}
		return Out;
	}

	// overlay.js line: the points (x, z pairs) with a vertex every Step metres, the mitered offset direction (the
	// average of the two segment normals, stretched by 1 / max(0.55, cos)), U the distance along.
	inline std::vector<FVertex> Line(const double* Pts, int N, const FRibbon& O)
	{
		std::vector<double> T; // x, z, s
		std::vector<FVertex> Out;
		if (N < 1) return Out;
		double S = 0.0;
		T.push_back(Pts[0]); T.push_back(Pts[1]); T.push_back(0.0);
		auto Seg = [&](double Ax, double Az, double Bx, double Bz)
		{
			const double L = std::hypot(Bx - Ax, Bz - Az);
			if (L < 1e-4) return;
			const int K = O.Step > 0 ? (int)std::fmax(1.0, std::ceil(L / O.Step)) : 1;
			for (int J = 1; J <= K; J++) { T.push_back(Ax + (Bx - Ax) * J / K); T.push_back(Az + (Bz - Az) * J / K); T.push_back(S + L * J / K); }
			S += L;
		};
		for (int I = 1; I < N; I++) Seg(Pts[I * 2 - 2], Pts[I * 2 - 1], Pts[I * 2], Pts[I * 2 + 1]);
		if (O.bClosed) Seg(Pts[N * 2 - 2], Pts[N * 2 - 1], Pts[0], Pts[1]);
		const int M = (int)T.size() / 3;
		if (M < 2) return Out;
		for (int I = 0; I < M; I++)
		{
			const double X = T[I * 3], Z = T[I * 3 + 1];
			double Ax = 0, Az = 0, Bx = 0, Bz = 0;
			const int Pi_ = I > 0 ? I - 1 : O.bClosed ? M - 2 : -1, Ni = I < M - 1 ? I + 1 : O.bClosed ? 1 : -1;
			if (Pi_ >= 0) { Ax = X - T[Pi_ * 3]; Az = Z - T[Pi_ * 3 + 1]; double L = std::hypot(Ax, Az); if (L == 0) L = 1; Ax /= L; Az /= L; }
			if (Ni >= 0) { Bx = T[Ni * 3] - X; Bz = T[Ni * 3 + 1] - Z; double L = std::hypot(Bx, Bz); if (L == 0) L = 1; Bx /= L; Bz /= L; }
			if (Pi_ < 0) { Ax = Bx; Az = Bz; } else if (Ni < 0) { Bx = Ax; Bz = Az; }
			double Nx = -Az - Bz, Nz = Ax + Bx;
			const double Nl = std::hypot(Nx, Nz);
			if (Nl < 1e-4) { Nx = -Bz; Nz = Bx; }
			else
			{
				Nx /= Nl; Nz /= Nl;
				const double Cos = std::fmax(0.55, Nx * -Az + Nz * Ax);
				Nx /= Cos; Nz /= Cos;
			}
			Out.push_back({ X, Z, Nx, Nz, T[I * 3 + 2] });
		}
		return Out;
	}

	// markers.js nodeSquare: four corner brackets, each a 3-point line ([sx * half, sz * (half - len)], [sx * half,
	// sz * half], [sx * (half - len), sz * half]) for (sx, sz) in (-1,-1) (1,-1) (1,1) (-1,1); half 1.85, len 1.1.
	inline constexpr double NodeHalf = 1.85, NodeLen = 1.1;
	inline void NodeBracket(int Corner, double Ox, double Oz, double Out[6])
	{
		static const int Sx[4] = { -1, 1, 1, -1 }, Sz[4] = { -1, -1, 1, 1 };
		const double X = Sx[Corner & 3], Z = Sz[Corner & 3];
		Out[0] = Ox + X * NodeHalf; Out[1] = Oz + Z * (NodeHalf - NodeLen);
		Out[2] = Ox + X * NodeHalf; Out[3] = Oz + Z * NodeHalf;
		Out[4] = Ox + X * (NodeHalf - NodeLen); Out[5] = Oz + Z * NodeHalf;
	}
	inline FRibbon NodeRibbon() { FRibbon R; R.W = 0.24; R.Y = 0.12; R.Step = 0.7; return R; }
	inline FLook NodeLook(bool bFuel) { FLook L; L.Hex = bFuel ? FuelNode : Brass; L.Opacity = 0.9; return L; }

	// ---- capture point (markers.js capturePoint) ----
	struct FRingDraw { double R = 0; FRibbon Ribbon; FLook Look; int Segments = -1; /* -1: the whole ring */ };
	inline FRibbon CaptureRingRibbon() { FRibbon R; R.W = 0.42; R.Y = 0.12; R.bFill = true; return R; }
	// heldLook: the owner's colour at 0.95 with a 0.07 wash, else chalk at 0.8 with a 0.05 wash
	inline FLook CaptureRingLook(bool bOwner, uint32_t OwnerHex)
	{
		FLook L;
		if (bOwner) { L.Hex = OwnerHex; L.Opacity = 0.95; L.Fill = 0.07; }
		else { L.Hex = NeutralRing; L.Opacity = 0.8; L.Fill = 0.05; }
		return L;
	}
	// The rings of one point, back to front: the ring with its wash; while taken, the dim track and the arc of
	// ArcSegments of 64 in ArcHex; contested, the brass ring (DangerOpacity); a flip, the brass ring growing outward.
	struct FCaptureState
	{
		double Radius = PointRadius;
		bool bOwner = false; uint32_t OwnerHex = 0;
		bool bArc = false; uint32_t ArcHex = 0; int ArcSegments = 0;
		bool bDanger = false; double DangerOpacity = 0.0;
		bool bFlip = false; double FlipOpacity = 0.0, FlipScale = 1.0;
	};
	inline std::vector<FRingDraw> CaptureRings(const FCaptureState& S)
	{
		std::vector<FRingDraw> Out;
		const double R = S.Radius - 0.3;
		Out.push_back({ R, CaptureRingRibbon(), CaptureRingLook(S.bOwner, S.OwnerHex), -1 });
		if (S.bArc && S.ArcSegments > 0)
		{
			FRibbon Track; Track.W = 0.5; Track.Y = 0.13;
			FLook TrackLook; TrackLook.Hex = White; TrackLook.Opacity = 0.22;
			Out.push_back({ R - 0.9, Track, TrackLook, -1 });
			FRibbon Arc; Arc.W = 0.7; Arc.Y = 0.14; Arc.Segs = ProgressSegments;
			FLook ArcLook; ArcLook.Hex = S.ArcHex; ArcLook.Opacity = 0.9;
			Out.push_back({ R - 0.9, Arc, ArcLook, S.ArcSegments < ProgressSegments ? S.ArcSegments : ProgressSegments });
		}
		FRibbon Brass_; Brass_.W = 0.5; Brass_.Y = 0.15;
		// flip, then danger (both renderOrder 4, added in that order)
		if (S.bFlip) { FLook L; L.Hex = Brass; L.Opacity = S.FlipOpacity; Out.push_back({ (R + 0.25) * S.FlipScale, Brass_, L, -1 }); }
		if (S.bDanger) { FLook L; L.Hex = Brass; L.Opacity = S.DangerOpacity; Out.push_back({ R - 0.3, Brass_, L, -1 }); }
		return Out;
	}
	// capturePoint frame: the contested ring's slow pulse
	inline double DangerOpacity(double TimeSeconds) { return 0.55 + 0.3 * (0.5 + 0.5 * std::sin(TimeSeconds * Pi * 1.6)); }

	// ---- weapon range rings (makeUnit, selectionRing, rangeRings) ----
	inline FRibbon RangeRibbon() { FRibbon R; R.W = 0.22; R.Y = 0.18; R.Min = 1.2; R.DashPeriod = RangeDash[0]; return R; }
	inline FLook RangeLook() { FLook L; L.Hex = White; L.Opacity = 0.7; L.DashPeriod = RangeDash[0]; L.DashOn = RangeDash[1]; return L; }
	// [def.w.range, def.w.minRange].filter(Boolean)
	inline int RangeRadii(double Range, double MinRange, double Out[2])
	{
		int N = 0;
		if (Range != 0 && !std::isnan(Range)) Out[N++] = Range;
		if (MinRange != 0 && !std::isnan(MinRange)) Out[N++] = MinRange;
		return N;
	}
	// rangeRings: shown for two units or fewer, or up to four of one type. Types: the selected units' types in
	// selection order, as ids that are held (a missing unit has no type: undefined, which differs from a type).
	template <typename FTypeEq>
	bool RangeRingsShown(int Selected, int TypeCount, FTypeEq&& SameType /* (i, j) */)
	{
		if (Selected <= 2) return true;
		if (Selected > 4) return false;
		for (int I = 1; I < TypeCount; I++) if (!SameType(I - 1, I)) return false;
		return true;
	}

	// ---- veterancy (makeUnit stars) ----
	// Up to three gold squares turned 45 degrees, 0.32 m a side, at x -0.5, 0, 0.5 and 0.42 m over the bar's centre (the
	// billboarded bar group's metres); star i shows when i < vet.
	inline constexpr double StarSide = 0.32, StarRise = 0.42;
	inline int StarsShown(int Vet) { return Vet <= 0 ? 0 : Vet >= 3 ? 3 : Vet; }
	inline double StarX(int I) { return -0.5 + 0.5 * I; }

	// ---- HQ (buildHQ, hqRing) ----
	inline FRibbon HqRingRibbon() { FRibbon R; R.W = 0.36; R.Y = 0.1; return R; }
	inline double HqRingRadius() { return ReinforceRadius - 0.35; }
	inline double HqZoneRadius() { return ReinforceRadius - 0.75; }
	inline constexpr int HqZoneSegments = 48;   // CircleGeometry(R - 0.75, 48)
	inline constexpr double HqZoneOpacity = 0.07, HqRingOpacity = 0.85, HqZoneLift = 0.05;
	inline constexpr double HqZoneChalk = 0.6;  // color.lerp(CHALK, 0.6)

	// The map-edge cut (buildHQ's clipping planes x >= 0, x <= MW, z >= 0, z <= MH): Sutherland-Hodgman on a convex
	// polygon in the ground plane. In and Out hold x, z pairs.
	inline std::vector<double> ClipToRect(const std::vector<double>& In, double X0, double Z0, double X1, double Z1)
	{
		std::vector<double> P = In;
		for (int Edge = 0; Edge < 4 && P.size() >= 6; Edge++)
		{
			auto Inside = [&](double X, double Z) { return Edge == 0 ? X >= X0 : Edge == 1 ? X <= X1 : Edge == 2 ? Z >= Z0 : Z <= Z1; };
			auto Cross = [&](double Ax, double Az, double Bx, double Bz, double& Cx, double& Cz)
			{
				double T;
				if (Edge < 2) { const double E = Edge == 0 ? X0 : X1; T = (E - Ax) / (Bx - Ax); }
				else { const double E = Edge == 2 ? Z0 : Z1; T = (E - Az) / (Bz - Az); }
				Cx = Ax + (Bx - Ax) * T; Cz = Az + (Bz - Az) * T;
			};
			std::vector<double> Q;
			const size_t N = P.size() / 2;
			for (size_t I = 0; I < N; I++)
			{
				const double Ax = P[I * 2], Az = P[I * 2 + 1], Bx = P[((I + 1) % N) * 2], Bz = P[((I + 1) % N) * 2 + 1];
				const bool InA = Inside(Ax, Az), InB = Inside(Bx, Bz);
				if (InA) { Q.push_back(Ax); Q.push_back(Az); }
				if (InA != InB) { double Cx, Cz; Cross(Ax, Az, Bx, Bz, Cx, Cz); Q.push_back(Cx); Q.push_back(Cz); }
			}
			P.swap(Q);
		}
		if (P.size() < 6) P.clear();
		return P;
	}

	// ---- text tags (markers.js LABEL) ----
	// tag: 256 x 64 canvas shown 128 px wide (scale 0.5), text 30 px; hq: 512 x 112 shown 156 px wide, text 54 px. The
	// plate is the canvas height less 12, 6 from the top; pad 16; an hq tag leads with a swatch (plate height - 22)
	// and 14 px of gap. fade: the depth band (m) over which the tag fades out as the camera comes closer.
	struct FTagStyle { double W, H, Sx, Px, FadeNear, FadeFar; bool bSwatch; };
	inline FTagStyle TagStyle() { return { 256, 64, 128, 30, 19, 27, false }; }
	inline FTagStyle HqStyle() { return { 512, 112, 156, 54, 10, 17, true }; }
	inline double Smoothstep(double E0, double E1, double X)
	{
		const double T = (X - E0) / (E1 - E0), C = T < 0 ? 0 : T > 1 ? 1 : T;
		return C * C * (3 - 2 * C);
	}
	inline constexpr double NodeTagLift = 3.0, HqTagLift = 17.0, PointTagLift = 10.0, RegionNameLift = 12.0;
}

#ifndef TC_MARKERS_CORE_ONLY

#include "CoreMinimal.h"

class UCanvas;
class UFont;
class FJsonObject;
class ATCGameMode;
struct FCanvasUVTri;
struct FTCCapturePoint;
struct FTCPointRingFrame;
struct FTCCoverGlyph;

// A resource node as the snapshot lists it ([x, z, rate, fuel]).
struct FTCResourceNode
{
	double X = 0, Z = 0, Rate = 0;
	bool bFuel = false;
	FString RateText; // `${rate}` as JavaScript writes the number
};

// The snapshot state the markers keep between snapshots (client/main.js nodeMarks: a node's mark is added the first
// time a snapshot lists it and kept for the match; a start drops them).
class THREECROSSROADS_API FTCMarkers
{
public:
	void ApplyStart();
	void ApplySnapshot(const FJsonObject& M);
	void Clear() { Nodes.Reset(); RegionBusy.Reset(); }
	const TArray<FTCResourceNode>& ResourceNodes() const { return Nodes; }
	// The node tag's English text ('+<rate> MP/s' or '+<rate> Fuel/s') before translation.
	static FString NodeTag(const FTCResourceNode& N);
	// capturePoint set's tag rule for World regions (world-regions.js): hidden for 2.5 s after the progress changes.
	bool RegionTagVisible(double Id, double Progress, double NowSeconds);

private:
	TArray<FTCResourceNode> Nodes;
	struct FBusy { double Last = -1, Until = 0; };
	TMap<int64, FBusy> RegionBusy;
};

// Draws ribbons, fills, diamonds and tags on a UCanvas. Project maps Unreal world centimetres to canvas pixels (Z > 0
// in front of the camera, as UCanvas::Project); Height gives the ground in metres at (x, z). CameraRight and
// CameraForward are the view's unit axes (Unreal), CameraLocation its origin (cm): for the minimum widths and the tags'
// depth fade. Triangles are batched in call order and drawn by Flush (the destructor flushes too).
class THREECROSSROADS_API FTCMarkerPainter
{
public:
	FTCMarkerPainter(UCanvas* InCanvas, TFunction<FVector(const FVector&)> InProject, TFunction<double(double, double)> InHeight,
		const FVector& InCameraLocation, const FVector& InCameraRight, const FVector& InCameraForward);
	~FTCMarkerPainter();
	FTCMarkerPainter(FTCMarkerPainter&&) = default;
	FTCMarkerPainter(const FTCMarkerPainter&) = delete;
	FTCMarkerPainter& operator=(const FTCMarkerPainter&) = delete;

	// A ring (or the first Segments of it) around (Cx, Cz) in metres, as overlay.js draws it. Clip: cut at a rectangle in
	// metres (x0, z0, x1, z1).
	void Ring(double Cx, double Cz, double R, const TCMarkersCore::FRibbon& O, const TCMarkersCore::FLook& L, int32 Segments = -1, const double* Clip = nullptr);
	// A polyline (x, z pairs, metres) as overlay.js line. Hub (x, z): a closed line's inside is filled from there to its
	// inner edge at L.Fill (cover-preview.js's shield template).
	void Line(const double* Pts, int32 N, const TCMarkersCore::FRibbon& O, const TCMarkersCore::FLook& L, const double* Hub = nullptr);
	// A flat disc (CircleGeometry) at Lift metres over the ground, cut at Clip when given.
	void Disc(double Cx, double Cz, double R, int32 Segs, const FLinearColor& Colour, double Lift, const double* Clip = nullptr);
	// A world quad (four points: x, z, height over the ground), e.g. a flag cloth.
	void Quad(const FVector (&Points)[4], const FLinearColor& Colour);
	// A line between two world points (cm), Width pixels wide, as two triangles in the batch (WebGL lines are 1 px).
	void ScreenLine(const FVector& A, const FVector& B, const FLinearColor& Colour, double Width = 1.0);
	FVector ProjectWorld(const FVector& World) const;
	// In front of the camera and within three viewports of the screen (the HUD's cost limit for World regions).
	bool OnOrNearScreen(const FVector& Screen) const;
	// Screen-space shapes: a square turned 45 degrees with half-diagonal H pixels.
	void Diamond(FVector2D Centre, double HalfDiagonal, const FLinearColor& Colour);
	// markers.js label at (x, z) Lift metres over the ground: Text already translated. Swatch: the hq style's owner
	// colour. Fade multiplies the depth fade (map-view.js fadeLabels). Returns false when it was not drawn.
	bool Tag(UFont* Font, const FString& Text, double X, double Z, double Lift, const TCMarkersCore::FTagStyle& Style, float Fade = 1.f, const FLinearColor* Swatch = nullptr);
	void Flush();

	// Pixels per metre at a world point (cm), measured across the view; 0 behind the camera.
	double PixelsPerMetre(const FVector& World) const;
	FVector Ground(double X, double Z, double Lift) const;
	int32 TrianglesDrawn() const { return Drawn; }
	int32 TagsDrawn() const { return Tags; }

	static FLinearColor Colour(uint32 Hex, double Alpha);

private:
	UCanvas* Canvas = nullptr;
	TFunction<FVector(const FVector&)> Project;
	TFunction<double(double, double)> Height;
	FVector CamLoc, CamRight, CamFwd;
	TArray<FCanvasUVTri> Tris;
	int32 Drawn = 0, Tags = 0;
	void Tri(const FVector& A, const FVector& B, const FVector& C, const FLinearColor& Colour);
	void Ribbon(const std::vector<TCMarkersCore::FVertex>& V, const TCMarkersCore::FRibbon& O, const TCMarkersCore::FLook& L, double Fill, double Cx, double Cz, const double* Clip);
};

// The HUD's side (ATCHUD calls these; see the round 17 lead patch). Each draws through a painter made for the HUD's
// canvas from the game mode's terrain and the canvas's scene view.
namespace TCMarkersHud
{
	// Canvas->Project, the terrain's height (0 without terrain), the scene view's camera.
	FTCMarkerPainter Painter(UCanvas* Canvas, const ATCGameMode& M);
	// The weapon range and minimum range of a type (pinned shared/sim.js UNITS, TCMarkers.Ranges.inl); false when
	// the type has no weapon row.
	bool Ranges(const FString& Type, double& OutRange, double& OutMinRange);
	// buildHQ for every spawn of the start: the reinforce zone tinted toward chalk and the HQ ring, cut at the map
	// edge, the tall flag's cloth and the "<name> HQ" tag. The camp's models (sandbags, tent, crates, flagpole) are not
	// drawn here.
	void DrawHqCamps(FTCMarkerPainter& P, const ATCGameMode& M, UFont* Font);
	// nodeMark for every node the snapshots listed: the four brackets and the "+<rate> MP/s" (or Fuel/s) tag. The
	// small wooden box on it is not drawn.
	void DrawNodes(FTCMarkerPainter& P, const ATCGameMode& M, UFont* Font);
	// makeUnit's weapon range rings for the selection while rangeRings() allows them (dashed, white 0.7, over all).
	int32 DrawRangeRings(FTCMarkerPainter& P, const ATCGameMode& M, const TSet<int32>& Selected);
	// markers.js capturePoint's rings for an authored point (the HUD keeps its flag and tags).
	void DrawCapturePoint(FTCMarkerPainter& P, const FTCCapturePoint& Point, const FTCPointRingFrame& Frame);
	// world-regions.js for every known World region within three viewports: the border (1 px lines at 0.55), the
	// capture point (rings, wash, arc, contested ring), its flag cloth and its two tags (the text, the name above).
	void DrawWorldRegions(FTCMarkerPainter& P, ATCGameMode& M, UFont* Font);
	// client/cover-preview.js glyph for each mark of one layer at the layer's opacity: the shield's closed ribbon (0.18 m,
	// at least 1.15 px each side, 0.14 m over the ground) over its 0.3 fill, or the chevron's open ribbon, in the mark's
	// colour and alpha. This canvas has no depth test: the ground layer is not hidden behind trees and units as the
	// browser's is; the through-models layer has none on either side.
	void DrawCoverLayer(FTCMarkerPainter& P, const TArray<FTCCoverGlyph>& Glyphs, float Opacity);
	// makeUnit's veterancy stars over a bar centred at BarCentre (canvas pixels), PxPerMetre the bar group's scale.
	void DrawStars(UCanvas* Canvas, FVector2D BarCentre, float PxPerMetre, int32 Stars);

	// Verification switches (round 17), read once from the command line; call once per HUD frame:
	//   -TCMarkersSelect=<type>  keep the selection on the living unit of that type with the lowest id on seat
	//                            -TCMarkersSeat=<n> (default 0), whoever owns it: a spectator's selection, for
	//                            drawing only (nothing is sent; the tour's unit:<type> picks the same unit)
	//   -TCMarkersLog            every 5 s a "markers:" line: nodes, spawns, selection, range rings drawn, veterans
	void Verify(ATCGameMode& M, TSet<int32>& Selected);
	// What the last frame drew (for the log line).
	struct FFrameStats { int32 RangeRings = 0, Nodes = 0, HqCamps = 0, Regions = 0, CapturePoints = 0, Stars = 0, Tags = 0; };
	FFrameStats& Stats();
}

#endif // TC_MARKERS_CORE_ONLY
