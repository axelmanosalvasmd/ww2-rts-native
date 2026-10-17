#include "Client/TCMarkers.h"
#include "Client/TCCampModels.h"
#include "Client/TCWorldRecovery.h"
#include "ThreeCrossroads.h"
#include "CanvasItem.h"
#include "CanvasTypes.h"
#include "Engine/Canvas.h"
#include "Engine/Font.h"
#include "GlobalRenderResources.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "SceneView.h"
#include "SceneInterface.h"
#include "Client/TCGameMode.h"
#include "Client/TCLocale.h"
#include "Client/TCStats.h"
#include "Client/TCLobbySettings.h"
#include "Client/TCObjectives.h"
#include "Client/TCCoverPreview.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCLighting.h"
#include "Camera/PlayerCameraManager.h"
#include "Engine/World.h"
#include "Net/TCMatchState.h"
#include "World/TCTerrain.h"
#include "World/TCFogFrontier.h"
#include "Client/TCMarkers.Ranges.inl"
#include "Client/TCMarkers.Badges.inl"
#include "Engine/Texture2D.h"
#include "ImageUtils.h"
#include "Misc/Base64.h"
#include "UObject/StrongObjectPtr.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Fonts/SlateFontInfo.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Materials/Material.h"

namespace TCMarkerText
{
	// Barlow has no VP star. Keep the reference text and use the engine's Unicode font for missing glyphs,
	// as the browser's sans-serif fallback does. This font and its metrics are confined to marker labels.
	FString FallbackPath() { return FPaths::EngineContentDir() / TEXT("Slate/Fonts/DroidSansFallback.ttf"); }
	const FTCTypeFace* FallbackFace()
	{
		static FTCTypeFace Face;
		static bool bTried = false;
		if (!bTried)
		{
			bTried = true;
			TArray<uint8> Bytes;
			if (FFileHelper::LoadFileToArray(Bytes, *FallbackPath(), FILEREAD_Silent)) Face.Parse(Bytes);
		}
		return Face.IsValid() ? &Face : nullptr;
	}
	void Pens(const FString& Text, float Px, float Tracking, TArray<float>& Out)
	{
		const FTCTypeFace* Face = FTCType::Face(true);
		Face->Pens(Text, Px, Tracking, Out, false);
		float Extra = 0.f;
		for (int32 I = 0; I < Text.Len(); I++)
		{
			Out[I] += Extra;
			if (Face->Glyph(Text[I]) != 0) continue;
			const FTCTypeFace* Fallback = FallbackFace();
			if (Fallback && Fallback->Glyph(Text[I]))
				Extra += Fallback->Width(FString::Chr(Text[I]), Px, 0.f, false) - float(Face->Advance(0)) * Px / Face->UnitsPerEm();
		}
		Out.Last() += Extra;
	}
	double Width(const FString& Text, float Px, float Tracking)
	{
		TArray<float> P; Pens(Text, Px, Tracking, P); return P.Last();
	}
	UFont* Font()
	{
		static TStrongObjectPtr<UFont> MarkerFont;
		if (!MarkerFont)
		{
			MarkerFont.Reset(NewObject<UFont>());
			MarkerFont->FontCacheType = EFontCacheType::Runtime;
			MarkerFont->LegacyFontSize = 11;
			FCompositeFont& Composite = MarkerFont->GetMutableInternalCompositeFont();
			Composite = *FTCType::Font()->GetCompositeFont();
			FTypefaceEntry& E = Composite.FallbackTypeface.Typeface.Fonts.AddDefaulted_GetRef();
			E.Name = TEXT("Regular");
			E.Font = FFontData(FallbackPath(), EFontHinting::AutoLight, EFontLoadingPolicy::LazyLoad);
		}
		return MarkerFont.Get();
	}
	void Draw(UCanvas* Canvas, const FString& Text, FVector2D At, float Px, float Tracking, FLinearColor Colour)
	{
		TArray<float> P; Pens(Text, Px, Tracking, P);
		const FSlateFontInfo Info(Font(), Px * 0.75f, TEXT("SemiBold"));
		for (int32 I = 0; I < Text.Len(); I++)
		{
			if (FChar::IsWhitespace(Text[I])) continue;
			FCanvasTextItem Item(At + FVector2D(P[I], 0), FText::FromString(FString::Chr(Text[I])), Info, Colour);
			Canvas->DrawItem(Item);
		}
	}
}

namespace TCMarkerIcons
{
#include "Client/TCHUD.Icons.inl"
	void Hq(UCanvas* Canvas, FVector2D Origin, double Size, const FLinearColor& Colour)
	{
		static TStrongObjectPtr<UTexture2D> Atlas;
		if (!Atlas)
		{
			TArray<uint8> Bytes;
			if (FBase64::Decode(HudIconPng, Bytes)) Atlas.Reset(FImageUtils::ImportBufferAsTexture2D(Bytes));
			if (Atlas) { Atlas->SRGB = true; Atlas->Filter = TF_Bilinear; Atlas->UpdateResource(); }
		}
		if (!Atlas || !Atlas->GetResource()) return;
		int32 Index = INDEX_NONE;
		for (int32 I = 0; I < UE_ARRAY_COUNT(HudIconKeys); I++) if (FString(HudIconKeys[I]) == TEXT("hq")) { Index = I; break; }
		if (Index == INDEX_NONE) return;
		const FVector2D Uv0(double(Index % HudIconColumns) * HudIconSide / HudIconWidth, double(Index / HudIconColumns) * HudIconSide / HudIconHeight);
		const FVector2D Uv1 = Uv0 + FVector2D(double(HudIconSide) / HudIconWidth, double(HudIconSide) / HudIconHeight);
		FCanvasTileItem Tile(Origin, Atlas->GetResource(), FVector2D(Size, Size), Uv0, Uv1, Colour);
		Tile.BlendMode = SE_BLEND_Translucent; Canvas->DrawItem(Tile);
	}
}

namespace
{
	namespace MC = TCMarkersCore;

	// markers.js label: PLATE, HAIRLINE, LABEL_TEXT
	FLinearColor SrgbA(uint8 R, uint8 G, uint8 B, float A) { FLinearColor C(FColor(R, G, B)); C.A = A; return C; }
	const FLinearColor TagPlate = SrgbA(22, 25, 27, 0.88f);       // rgba(22, 25, 27, 0.88)
	const FLinearColor TagHairline = SrgbA(176, 164, 122, 0.55f); // rgba(176, 164, 122, 0.55)
	const FLinearColor TagText = FLinearColor(FColor(0xe2, 0xdf, 0xd3));
	FBox2D MarkerHudExclusion(ForceInit);

	const UWorld* MarkerWorld(const UCanvas* Canvas)
	{
		// HUD canvases have a transient outer, so UObject::GetWorld() returns null. The rendered view owns the scene.
		const FSceneView* View = Canvas ? Canvas->SceneView : nullptr;
		return View && View->Family && View->Family->Scene ? View->Family->Scene->GetWorld() : nullptr;
	}

	const ATCLighting* MarkerLighting(const UCanvas* Canvas)
	{
		const UWorld* W = MarkerWorld(Canvas);
		const ATCGameMode* M = W ? W->GetAuthGameMode<ATCGameMode>() : nullptr;
		return M ? M->Lighting.Get() : nullptr;
	}

	float BillboardDepth(const UCanvas* Canvas, float PxPerMetre)
	{
		// A billboard metre projects to P[1][1] * viewHeight / (2 * viewDepth).
		// Using the measured, unclamped scale also works with off-axis cameras and render scaling.
		return Canvas && Canvas->SceneView && PxPerMetre > 0 ?
			Canvas->ClipY * Canvas->SceneView->ViewMatrices.GetViewToClip().M[1][1] / (2.f * PxPerMetre) : 0.f;
	}

	UMaterialInstanceDynamic* BadgeMaterial(UCanvas* Canvas, UTexture2D* Atlas, float Depth)
	{
		const ATCLighting* L = MarkerLighting(Canvas);
		if (!L) return nullptr;
		static TStrongObjectPtr<UMaterialInterface> Base;
		if (!Base)
		{
			Base.Reset(LoadObject<UMaterialInterface>(nullptr, TEXT("/Game/Markers/M_TCMarkerBillboard.M_TCMarkerBillboard")));
			if (!Base) return nullptr;
			TCFogLook::PrepareMaterial(Base.Get());
		}
		// Each queued tile needs its own uniform values until Canvas flushes. Reuse a bounded frame pool.
		static TArray<TStrongObjectPtr<UMaterialInstanceDynamic>> Pool;
		static uint64 Frame = MAX_uint64;
		static int32 Next = 0;
		if (Frame != GFrameCounter) { Frame = GFrameCounter; Next = 0; }
		if (Next == Pool.Num()) Pool.Emplace(UMaterialInstanceDynamic::Create(Base.Get(), GetTransientPackage()));
		UMaterialInstanceDynamic* Mid = Pool[Next++].Get();
		Mid->SetTextureParameterValue(TEXT("Badge"), Atlas);
		Mid->SetVectorParameterValue(TEXT("Haze"), L->Shown().Haze);
		Mid->SetScalarParameterValue(TEXT("Exposure"), L->Shown().Exposure);
		Mid->SetScalarParameterValue(TEXT("BeforeTone"), L->HazeBeforeTone() ? 1.f : 0.f);
		Mid->SetScalarParameterValue(TEXT("Fog"), TCLook::BillboardFog(Depth, L->FogNear(), L->FogFar()));
		return Mid;
	}

	bool JsonNumber(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return true;
	}
	// JavaScript truthiness of a row entry (nodes' fuel is 1 or 0)
	bool JsonTruthy(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid()) return false;
		switch (V->Type)
		{
		case EJson::Boolean: return V->AsBool();
		case EJson::Number: { const double D = V->AsNumber(); return D != 0 && !FMath::IsNaN(D); }
		case EJson::String: return !V->AsString().IsEmpty();
		case EJson::Array: case EJson::Object: return true;
		default: return false;
		}
	}
	FString JsNum(double V) { return UTF8_TO_TCHAR(TCWorldRecoveryCore::JsNumber(V).c_str()); }
}

// ------------------------------------------------------------------------------------------------ FTCMarkers

void FTCMarkers::ApplyStart()
{
	// client/main.js start: nodeMarks = null
	Clear();
}

void FTCMarkers::ApplySnapshot(const FJsonObject& M)
{
	// client/main.js applySnapshot: if (s.nodes) a mark for every node not marked yet (keyed `${x}:${z}`), kept after
	const TArray<TSharedPtr<FJsonValue>>* Arr = nullptr;
	const TSharedPtr<FJsonValue> Field = M.TryGetField(TEXT("nodes"));
	if (!Field.IsValid() || Field->Type != EJson::Array || !Field->TryGetArray(Arr)) return;
	for (const TSharedPtr<FJsonValue>& V : *Arr)
	{
		const TArray<TSharedPtr<FJsonValue>>* Row = nullptr;
		if (!V.IsValid() || !V->TryGetArray(Row) || Row->Num() < 2) continue;
		FTCResourceNode N;
		if (!JsonNumber((*Row)[0], N.X) || !JsonNumber((*Row)[1], N.Z)) continue;
		bool bKnown = false;
		for (const FTCResourceNode& K : Nodes) if (K.X == N.X && K.Z == N.Z) { bKnown = true; break; }
		if (bKnown) continue;
		double Rate = NAN;
		if (Row->Num() > 2 && JsonNumber((*Row)[2], Rate)) { N.Rate = Rate; N.RateText = JsNum(Rate); }
		else N.RateText = Row->Num() > 2 && (*Row)[2].IsValid() && (*Row)[2]->Type == EJson::Null ? TEXT("null") : TEXT("undefined");
		N.bFuel = Row->Num() > 3 && JsonTruthy((*Row)[3]);
		Nodes.Add(N);
	}
}

FString FTCMarkers::NodeTag(const FTCResourceNode& N)
{
	// nodeMark: label(fuel ? `+${rate} Fuel/s` : `+${rate} MP/s`)
	return N.bFuel ? FString::Printf(TEXT("+%s Fuel/s"), *N.RateText) : FString::Printf(TEXT("+%s MP/s"), *N.RateText);
}

bool FTCMarkers::RegionTagVisible(double Id, double Progress, double NowSeconds)
{
	// capturePoint set: if (last >= 0 && progress !== last) busyUntil = now + 2500; last = progress
	const int64 Key = FMath::IsFinite(Id) ? (int64)Id : MIN_int64;
	FBusy& B = RegionBusy.FindOrAdd(Key);
	if (B.Last >= 0 && Progress != B.Last) B.Until = NowSeconds + 2.5;
	B.Last = Progress;
	return NowSeconds >= B.Until;
}

void FTCMarkers::Clear()
{
	Nodes.Reset(); RegionBusy.Reset(); Scene.Reset();
}

TSharedPtr<FTCMarkerScene> FTCMarkers::WorldScene(const ATCGameMode& Match) const
{
	if (!Scene) Scene = MakeShared<FTCMarkerScene>();
	Scene->Begin(Match);
	return Scene;
}

// ------------------------------------------------------------------------------------------------ FTCMarkerPainter

FTCMarkerPainter::FTCMarkerPainter(UCanvas* InCanvas, TFunction<FVector(const FVector&)> InProject, TFunction<double(double, double)> InHeight,
	const FVector& InCameraLocation, const FVector& InCameraRight, const FVector& InCameraForward)
	: Canvas(InCanvas), Project(MoveTemp(InProject)), Height(MoveTemp(InHeight)), CamLoc(InCameraLocation), CamRight(InCameraRight.GetSafeNormal()), CamFwd(InCameraForward.GetSafeNormal())
{
}

FTCMarkerPainter::~FTCMarkerPainter()
{
	Flush();
}

FLinearColor FTCMarkerPainter::Colour(uint32 Hex, double Alpha)
{
	FLinearColor C(FColor((Hex >> 16) & 255, (Hex >> 8) & 255, Hex & 255));
	C.A = (float)FMath::Clamp(Alpha, 0.0, 1.0);
	return C;
}

FVector FTCMarkerPainter::Ground(double X, double Z, double Lift) const
{
	return FVector(X * 100.0, Z * 100.0, ((Height ? Height(X, Z) : 0.0) + Lift) * 100.0);
}

double FTCMarkerPainter::PixelsPerMetre(const FVector& World) const
{
	if (!Project) return 0.0;
	const FVector A = Project(World), B = Project(World + CamRight * 100.0);
	if (A.Z <= 0 || B.Z <= 0) return 0.0;
	return FVector2D::Distance(FVector2D(A.X, A.Y), FVector2D(B.X, B.Y));
}

void FTCMarkerPainter::Tri(const FVector& A, const FVector& B, const FVector& C, const FLinearColor& Col)
{
	if (A.Z <= 0 || B.Z <= 0 || C.Z <= 0 || Col.A <= 0.f) return;
	FVector2D P0(A.X, A.Y), P1(B.X, B.Y), P2(C.X, C.Y);
	if ((P1.X - P0.X) * (P2.Y - P0.Y) - (P1.Y - P0.Y) * (P2.X - P0.X) < 0) Swap(P1, P2);
	FCanvasUVTri T;
	T.V0_Pos = P0; T.V1_Pos = P1; T.V2_Pos = P2;
	T.V0_Color = T.V1_Color = T.V2_Color = Col;
	Tris.Add(T);
}

void FTCMarkerPainter::WorldTri(const FVector& A, const FVector& B, const FVector& C, const FLinearColor& Col)
{
	if (Scene) Scene->Triangle(A, B, C, Col, bDepthTest, bBeforeFog);
	else Tri(ProjectWorld(A), ProjectWorld(B), ProjectWorld(C), Col);
}

void FTCMarkerPainter::Flush()
{
	if (Scene) Scene->Flush();
	if (!Tris.Num()) return;
	if (Canvas && Canvas->Canvas)
	{
		FCanvasTriangleItem Item(Tris, GWhiteTexture);
		Item.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(Item);
		Drawn += Tris.Num();
	}
	Tris.Reset();
}

void FTCMarkerPainter::Ribbon(const std::vector<MC::FVertex>& V, const MC::FRibbon& O, const MC::FLook& L, double Fill, double Cx, double Cz, const double* Clip)
{
	const int32 N = (int32)V.size();
	if (N < 2) return;
	// overlay.js VERT_BODY: hw = max(w / 2, min pixels at this depth), applied along the offset direction; the fragment
	// shader's anti-aliasing width is fwidth(v): 2 over the ribbon's width in pixels, times (|nx| + |ny|) on screen
	TArray<double> Hw, Aa;
	Hw.SetNumUninitialized(N);
	Aa.SetNumUninitialized(N);
	for (int32 I = 0; I < N; I++)
	{
		const FVector C = Ground(V[I].X, V[I].Z, O.Y);
		const double Ppm = PixelsPerMetre(C);
		Hw[I] = Ppm > 0 ? FMath::Max(O.W / 2.0, O.Min / Ppm) : O.W / 2.0;
		const FVector A = ProjectWorld(Ground(V[I].X + V[I].Nx * Hw[I], V[I].Z + V[I].Nz * Hw[I], O.Y));
		const FVector B = ProjectWorld(Ground(V[I].X - V[I].Nx * Hw[I], V[I].Z - V[I].Nz * Hw[I], O.Y));
		const FVector2D D(A.X - B.X, A.Y - B.Y);
		const double Wpx = D.Size();
		Aa[I] = A.Z > 0 && B.Z > 0 && Wpx > 1e-6 ? 2.0 / Wpx * (FMath::Abs(D.X) + FMath::Abs(D.Y)) / Wpx : 1.0;
	}

	const FLinearColor BodyC = Colour(L.Hex, 1.0);
	if (Scene)
	{
		// overlay.js computes its profile once per fragment, with real pixel derivatives.
		const FLinearColor Ink = Colour(L.Hex, L.Opacity);
		struct FClipVertex { FVector P; double Side; };
		for (int32 I = 1; I < N; I++)
		{
			const MC::FVertex& A = V[I - 1]; const MC::FVertex& B = V[I];
			if (L.DashPeriod > 0 && !MC::DashLit((A.U + B.U) / 2, L.DashPeriod, L.DashOn)) continue;
			const FVector Q[] = {
				Ground(A.X + A.Nx * Hw[I-1], A.Z + A.Nz * Hw[I-1], O.Y),
				Ground(B.X + B.Nx * Hw[I], B.Z + B.Nz * Hw[I], O.Y),
				Ground(B.X - B.Nx * Hw[I], B.Z - B.Nz * Hw[I], O.Y),
				Ground(A.X - A.Nx * Hw[I-1], A.Z - A.Nz * Hw[I-1], O.Y) };
			TArray<FClipVertex> Poly = {{Q[0],1},{Q[1],1},{Q[2],-1},{Q[3],-1}};
			if (Clip) for (int32 E = 0; E < 4; E++)
			{
				TArray<FClipVertex> Out;
				auto Distance = [&](const FVector& P) { return E==0 ? P.X-Clip[0]*100 : E==1 ? Clip[2]*100-P.X : E==2 ? P.Y-Clip[1]*100 : Clip[3]*100-P.Y; };
				for (int32 J = 0; J < Poly.Num(); J++)
				{
					const FClipVertex& X=Poly[J]; const FClipVertex& Y=Poly[(J+1)%Poly.Num()];
					const double Dx=Distance(X.P), Dy=Distance(Y.P);
					if (Dx>=0) Out.Add(X);
					if ((Dx>=0)!=(Dy>=0)) { const double T=Dx/(Dx-Dy); Out.Add({FMath::Lerp(X.P,Y.P,T),FMath::Lerp(X.Side,Y.Side,T)}); }
				}
				Poly=MoveTemp(Out);
			}
			for (int32 J=1; J+1<Poly.Num(); J++)
				Scene->RibbonTriangle(Poly[0].P,Poly[J].P,Poly[J+1].P,Ink,bDepthTest,
					FVector2D(Poly[0].Side,O.Core),FVector2D(Poly[J].Side,O.Core),FVector2D(Poly[J+1].Side,O.Core));
			if (Fill > 0)
			{
				std::vector<double> P = {Cx,Cz,A.X-A.Nx*Hw[I-1],A.Z-A.Nz*Hw[I-1],B.X-B.Nx*Hw[I],B.Z-B.Nz*Hw[I]};
				if (Clip) P=MC::ClipToRect(P,Clip[0],Clip[1],Clip[2],Clip[3]);
				for (int32 J=1; J+1<(int32)P.size()/2; J++)
					WorldTri(Ground(P[0],P[1],O.Y),Ground(P[J*2],P[J*2+1],O.Y),Ground(P[J*2+2],P[J*2+3],O.Y),Colour(L.Hex,L.Opacity*Fill));
			}
		}
		return;
	}
	// the shader's edge colour (0.012, 0.016, 0.014) is a linear working-space colour mixed in linear light
	const FLinearColor EdgeC((float)MC::EdgeRgb[0], (float)MC::EdgeRgb[1], (float)MC::EdgeRgb[2], 1.f);
	// a polygon in metres (x, z pairs), cut at Clip, draped and fanned
	auto Poly = [&](std::vector<double> P, const FLinearColor& Col, double Lift)
	{
		if (Clip) P = MC::ClipToRect(P, Clip[0], Clip[1], Clip[2], Clip[3]);
		const int32 K = (int32)P.size() / 2;
		if (K < 3) return;
		TArray<FVector, TInlineAllocator<12>> S;
		for (int32 I = 0; I < K; I++) S.Add(Ground(P[I * 2], P[I * 2 + 1], Lift));
		for (int32 I = 1; I + 1 < K; I++) WorldTri(S[0], S[I], S[I + 1], Col);
	};
	constexpr int32 Strips = MC::ProfileStrips;
	for (int32 I = 1; I < N; I++)
	{
		const MC::FVertex& A = V[I - 1];
		const MC::FVertex& B = V[I];
		if (L.DashPeriod > 0 && !MC::DashLit((A.U + B.U) / 2, L.DashPeriod, L.DashOn)) continue;
		const double SegAa = (Aa[I - 1] + Aa[I]) / 2;
		for (int32 K = 0; K < Strips; K++)
		{
			const double V0 = 1.0 - 2.0 * K / Strips, V1 = 1.0 - 2.0 * (K + 1) / Strips;
			double Body, Alpha;
			MC::RibbonProfile((V0 + V1) / 2, SegAa, O.Core, Body, Alpha);
			FLinearColor Col = FMath::Lerp(EdgeC, BodyC, (float)Body);
			Col.A = (float)FMath::Clamp(L.Opacity * Alpha, 0.0, 1.0);
			if (Col.A <= 0.002f) continue;
			std::vector<double> P = {
				A.X + A.Nx * Hw[I - 1] * V0, A.Z + A.Nz * Hw[I - 1] * V0,
				B.X + B.Nx * Hw[I] * V0, B.Z + B.Nz * Hw[I] * V0,
				B.X + B.Nx * Hw[I] * V1, B.Z + B.Nz * Hw[I] * V1,
				A.X + A.Nx * Hw[I - 1] * V1, A.Z + A.Nz * Hw[I - 1] * V1 };
			Poly(MoveTemp(P), Col, O.Y);
		}
		// the inside of a filled ring: the hub to the inner edge, at the fill alpha
		if (Fill > 0)
		{
			std::vector<double> P = { Cx, Cz, A.X - A.Nx * Hw[I - 1], A.Z - A.Nz * Hw[I - 1], B.X - B.Nx * Hw[I], B.Z - B.Nz * Hw[I] };
			Poly(MoveTemp(P), Colour(L.Hex, L.Opacity * Fill), O.Y);
		}
	}
}

void FTCMarkerPainter::Ring(double Cx, double Cz, double R, const MC::FRibbon& O, const MC::FLook& L, int32 Segments, const double* Clip)
{
	if (!(R > 0)) return;
	if (L.DashPeriod > 0)
	{
		// one solid arc per lit dash (the fragment shader's cut, without its soft ends)
		MC::FLook Solid = L;
		Solid.DashPeriod = 0;
		for (const MC::FSpan& S : MC::RingDashes(R, L.DashPeriod, L.DashOn, O.DashPeriod))
		{
			const int32 Segs = FMath::Max(1, FMath::CeilToInt((S.A1 - S.A0) * R / 0.6));
			std::vector<MC::FVertex> V;
			for (int32 I = 0; I <= Segs; I++)
			{
				const double A = S.A0 + (S.A1 - S.A0) * I / Segs, Sx = FMath::Sin(A), Sz = -FMath::Cos(A);
				V.push_back({ Cx + Sx * R, Cz + Sz * R, Sx, Sz, 0 });
			}
			Ribbon(V, O, Solid, 0, Cx, Cz, Clip);
		}
		return;
	}
	const std::vector<MC::FVertex> V = Segments >= 0 ? MC::RingFirstSegments(R, O, Segments, Cx, Cz) : MC::Ring(R, O, Cx, Cz);
	Ribbon(V, O, L, O.bFill ? L.Fill : 0.0, Cx, Cz, Clip);
}

void FTCMarkerPainter::Line(const double* Pts, int32 N, const MC::FRibbon& O, const MC::FLook& L, const double* Hub)
{
	Ribbon(MC::Line(Pts, N, O), O, L, Hub ? L.Fill : 0.0, Hub ? Hub[0] : 0.0, Hub ? Hub[1] : 0.0, nullptr);
}

void FTCMarkerPainter::Disc(double Cx, double Cz, double R, int32 Segs, const FLinearColor& Col, double Lift, const double* Clip)
{
	// buildHQ's unpatched CircleGeometry stays flat at the spawn height. Its ribbon alone drapes over terrain.
	const double WorldZ = Ground(Cx, Cz, Lift).Z;
	for (int32 I = 0; I < Segs; I++)
	{
		const double A0 = 2 * MC::Pi * I / Segs, A1 = 2 * MC::Pi * (I + 1) / Segs;
		std::vector<double> P = { Cx, Cz, Cx + R * FMath::Cos(A0), Cz + R * FMath::Sin(A0), Cx + R * FMath::Cos(A1), Cz + R * FMath::Sin(A1) };
		if (Clip) P = MC::ClipToRect(P, Clip[0], Clip[1], Clip[2], Clip[3]);
		const int32 K = (int32)P.size() / 2;
		if (K < 3) continue;
		TArray<FVector, TInlineAllocator<8>> S;
		for (int32 J = 0; J < K; J++) S.Add(FVector(P[J * 2] * 100, P[J * 2 + 1] * 100, WorldZ));
		for (int32 J = 1; J + 1 < K; J++) WorldTri(S[0], S[J], S[J + 1], Col);
	}
}

void FTCMarkerPainter::Quad(const FVector (&Points)[4], const FLinearColor& Col, const FVector2D& GroundAnchor)
{
	FVector S[4];
	const double Base = Ground(GroundAnchor.X, GroundAnchor.Y, 0).Z;
	for (int32 I = 0; I < 4; I++) S[I] = FVector(Points[I].X * 100, Points[I].Y * 100, Base + Points[I].Z * 100);
	WorldTri(S[0], S[1], S[2], Col);
	WorldTri(S[0], S[2], S[3], Col);
}

FVector FTCMarkerPainter::ProjectWorld(const FVector& World) const
{
	return Project ? Project(World) : FVector(0, 0, -1);
}

bool FTCMarkerPainter::OnOrNearScreen(const FVector& Sc) const
{
	if (!Canvas || Sc.Z <= 0) return false;
	const double VW = Canvas->ClipX, VH = Canvas->ClipY;
	return Sc.X >= -VW && Sc.X <= 2 * VW && Sc.Y >= -VH && Sc.Y <= 2 * VH;
}

void FTCMarkerPainter::ScreenLine(const FVector& WA, const FVector& WB, const FLinearColor& Col, double Width)
{
	const FVector A = ProjectWorld(WA), B = ProjectWorld(WB);
	if (A.Z <= 0 || B.Z <= 0) return;
	FVector2D D(B.X - A.X, B.Y - A.Y);
	const double L = D.Size();
	if (L < 1e-6) return;
	const FVector2D N = FVector2D(-D.Y, D.X) / L * (Width / 2);
	const FVector A0(A.X + N.X, A.Y + N.Y, 1), A1(A.X - N.X, A.Y - N.Y, 1), B0(B.X + N.X, B.Y + N.Y, 1), B1(B.X - N.X, B.Y - N.Y, 1);
	if (Scene)
	{
		const FVector Up = FVector::CrossProduct(CamFwd, CamRight).GetSafeNormal();
		const FVector OffsetDir = CamRight * N.X - Up * N.Y;
		const double Pa = PixelsPerMetre(WA), Pb = PixelsPerMetre(WB);
		if (Pa <= 0 || Pb <= 0) return;
		const FVector Oa = OffsetDir * (100.0 / Pa), Ob = OffsetDir * (100.0 / Pb);
		WorldTri(WA + Oa, WB + Ob, WB - Ob, Col);
		WorldTri(WA + Oa, WB - Ob, WA - Oa, Col);
	}
	else
	{
		Tri(A0, B0, B1, Col); Tri(A0, B1, A1, Col);
	}
}

void FTCMarkerPainter::Diamond(FVector2D C, double H, const FLinearColor& Col)
{
	const FVector A(C.X, C.Y - H, 1), B(C.X + H, C.Y, 1), D(C.X, C.Y + H, 1), E(C.X - H, C.Y, 1);
	Tri(A, B, D, Col);
	Tri(A, D, E, Col);
}

bool FTCMarkerPainter::Tag(UFont* Font, const FString& Text, double X, double Z, double Lift, const MC::FTagStyle& St, float Fade, const FLinearColor* Swatch)
{
	if (!Canvas || !Canvas->Canvas || !Font || Text.IsEmpty() || !Project) return false;
	const FVector W = Ground(X, Z, Lift);
	const FVector At = Project(W);
	if (At.Z <= 0) return false;
	// the sprite's fade by view depth (screenSized: smoothstep(fade.x, fade.y, -mvPosition.z)) times fadeLabels
	const double Depth = FVector::DotProduct(W - CamLoc, CamFwd) / 100.0;
	const float Alpha = (float)MC::Smoothstep(St.FadeNear, St.FadeFar, Depth) * Fade;
	if (Alpha <= 0.01f) return false;
	Flush(); // the tag goes over every ribbon drawn so far
	// the canvas is laid out in its own pixels (W x H, widened for a long text) and shown at Sx / W of that
	const double K = St.Sx / St.W;
	const bool bBarlow = FTCType::IsLoaded();
	const float Tracking = St.bSwatch ? 1.f : 0.f;
	float Px = St.Px;
	float TW1 = 0, TH1 = 0;
	Canvas->TextSize(Font, Text, TW1, TH1, 1.f, 1.f);
	if (TH1 <= 0) return false;
	const float TextScale = (float)(St.Px * K * 1.2 / TH1); // the font's line height to the tag's text size
	double TextW = bBarlow ? TCMarkerText::Width(Text, Px, Tracking) : TW1 * TextScale / K; // in canvas pixels
	const double Lead = St.bSwatch && Swatch ? St.H - 20 : 0;
	const double Cw = FMath::Max(St.W, FMath::CeilToDouble((TextW * 1.1 + 40 + Lead) / 32) * 32);
	if (bBarlow) while (TextW > Cw - 32 - (St.bSwatch && Swatch ? St.H - 20 : 0) - 8 && Px > 12)
	{ Px -= 2; TextW = TCMarkerText::Width(Text, Px, Tracking); }
	const double Bh = St.H - 12, Y0 = 6, Pad = 16, Sw = St.bSwatch && Swatch ? Bh - 22 : 0, LeadIn = Sw > 0 ? Sw + 14 : 0;
	const double Bw = FMath::Min(Cw - 4, TextW + 2 * Pad + LeadIn), X0 = (Cw - Bw) / 2;
	const FVector2D Origin = TCMarkersHud::ClearHud(FVector2D(At.X - Cw * K / 2, At.Y - St.H * K / 2), FVector2D(Cw * K, St.H * K), MarkerHudExclusion);
	auto Rect = [&](double Cx0, double Cy0, double Cw0, double Ch0, FLinearColor C)
	{
		C.A *= Alpha;
		C = TCMarkersHud::WorldColour(Canvas, C, W);
		FCanvasTileItem T(Origin + FVector2D(Cx0, Cy0) * K, GWhiteTexture, FVector2D(Cw0, Ch0) * K, C);
		T.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(T);
	};
	// the plate (inset 1.5 canvas px) and its 3 px hairline centred on that edge
	Rect(X0 + 1.5, Y0 + 1.5, Bw - 3, Bh - 3, TagPlate);
	Rect(X0, Y0, Bw, 3, TagHairline);
	Rect(X0, Y0 + Bh - 3, Bw, 3, TagHairline);
	Rect(X0, Y0 + 3, 3, Bh - 6, TagHairline);
	Rect(X0 + Bw - 3, Y0 + 3, 3, Bh - 6, TagHairline);
	if (Sw > 0)
	{
		Rect(X0 + 11, (St.H - Sw) / 2, Sw, Sw, *Swatch);
		const FColor C = Swatch->ToFColor(true);
		const double Lum = (C.R * 0.2126 + C.G * 0.7152 + C.B * 0.0722) / 255.0;
		FLinearColor Ink(FColor(Lum > 0.6 ? 0x1a1d1f : 0xf2efe6)); Ink.A = Alpha;
		Ink = TCMarkersHud::WorldColour(Canvas, Ink, W);
		TCMarkerIcons::Hq(Canvas, Origin + FVector2D(X0 + 11 + Sw * 0.07, (St.H - Sw * 0.86) / 2) * K, Sw * 0.86 * K, Ink);
	}
	FLinearColor TC = TagText;
	TC.A *= Alpha;
	TC = TCMarkersHud::WorldColour(Canvas, TC, W);
	const double TextCx = X0 + LeadIn + (Bw - LeadIn) / 2, TextCy = St.H / 2 + 2;
	if (bBarlow)
	{
		TCMarkerText::Draw(Canvas, Text, Origin + FVector2D(TextCx - TextW / 2, TextCy - Px * 0.6) * K, Px * K, Tracking * K, TC);
	}
	else
	{
		FCanvasTextItem Item(Origin + FVector2D(TextCx * K - TW1 * TextScale / 2, TextCy * K - TH1 * TextScale / 2), FText::FromString(Text), Font, TC);
		Item.Scale = FVector2D(TextScale, TextScale);
		Item.BlendMode = SE_BLEND_Translucent;
		Canvas->DrawItem(Item);
	}
	Tags++;
	return true;
}

// ------------------------------------------------------------------------------------------------ TCMarkersHud

void TCMarkersHud::SetHudExclusion(const FBox2D& Bounds) { MarkerHudExclusion = Bounds; }

FVector2D TCMarkersHud::ClearHud(FVector2D Origin, FVector2D Size, const FBox2D& Bounds)
{
	if (Bounds.bIsValid && Bounds.Intersect(FBox2D(Origin, Origin + Size))) Origin.Y = Bounds.Max.Y + 8;
	return Origin;
}

namespace
{
	uint32 HexOf(const FLinearColor& C) { const FColor F = C.ToFColor(true); return ((uint32)F.R << 16) | ((uint32)F.G << 8) | F.B; }
	uint32 HexOf(const FColor& F) { return ((uint32)F.R << 16) | ((uint32)F.G << 8) | F.B; }
}

FTCMarkerPainter TCMarkersHud::Painter(UCanvas* Canvas, const ATCGameMode& M)
{
	const ATCTerrain* T = M.Terrain;
	FVector Loc = FVector::ZeroVector, Right = FVector::RightVector, Fwd = FVector::ForwardVector;
	if (Canvas && Canvas->SceneView)
	{
		Loc = Canvas->SceneView->ViewLocation;
		const FRotationMatrix RM(Canvas->SceneView->ViewRotation);
		Fwd = RM.GetUnitAxis(EAxis::X); Right = RM.GetUnitAxis(EAxis::Y);
	}
	FTCMarkerPainter P(Canvas,
		[Canvas](const FVector& W) { return Canvas ? Canvas->Project(W) : FVector(0, 0, -1); },
		[T](double X, double Z) { return T ? (double)T->HeightAt((float)X, (float)Z) : 0.0; },
		Loc, Right, Fwd);
	P.SetWorldScene(M.Markers.WorldScene(M));
	return P;
}

bool TCMarkersHud::Ranges(const FString& Type, double& OutRange, double& OutMinRange)
{
	for (const FTCMarkerRange& R : TCMarkerRanges)
		if (Type == R.Type) { OutRange = R.Range; OutMinRange = R.MinRange; return true; }
	return false;
}

void TCMarkersHud::DrawHqCamps(FTCMarkerPainter& P, const ATCGameMode& M, UFont* Font)
{
	const FTCMatchState& S = M.State;
	if (!S.bHasMatch || S.Map.W <= 0) return;
	const double Clip[4] = { 0, 0, S.Map.W * MC::CellMetres, S.Map.H * MC::CellMetres };
	const float Fade = M.WarMap.LabelFade();
	for (int32 Slot = 0; Slot < S.Spawns.Num(); Slot++)
	{
		if (!S.Spawns[Slot].IsSet()) continue;
		const FVector2D Sp = S.Spawns[Slot].GetValue();
		const FLinearColor Seat = ATCUnitRenderer::SeatColor(Slot);
		// the zone: f.color lerped 0.6 toward chalk (THREE.Color.lerp works on linear components), 0.07 opaque
		FLinearColor Zone = FMath::Lerp(Seat, FLinearColor(FColor(0xf2, 0xec, 0xdc)), (float)MC::HqZoneChalk);
		Zone.A = (float)MC::HqZoneOpacity;
		P.Disc(Sp.X, Sp.Y, MC::HqZoneRadius(), MC::HqZoneSegments, Zone, MC::HqZoneLift, Clip);
		MC::FLook Ring; Ring.Hex = HexOf(Seat); Ring.Opacity = MC::HqRingOpacity;
		P.Ring(Sp.X, Sp.Y, MC::HqRingRadius(), MC::HqRingRibbon(), Ring, -1, Clip);
		// TCCampModels draws main.js buildHQ's textured, lit cloth.
		Stats().HqCamps++;
	}
	// the tags last, over the rings
	for (int32 Slot = 0; Slot < S.Spawns.Num(); Slot++)
	{
		if (!S.Spawns[Slot].IsSet()) continue;
		const FVector2D Sp = S.Spawns[Slot].GetValue();
		// `${names[slot] ?? f.name} HQ`
		const FString Name = S.Names.IsValidIndex(Slot) ? S.Names[Slot] : FTCLobbySettings::FactionName(S.Factions.IsValidIndex(Slot) ? S.Factions[Slot] : 0);
		const FLinearColor Seat = ATCUnitRenderer::SeatColor(Slot);
		Stats().Tags += P.Tag(Font, FTCLocale::T(Name + TEXT(" HQ")), Sp.X, Sp.Y, MC::HqTagLift, MC::HqStyle(), Fade, &Seat);
	}
}

void TCMarkersHud::DrawNodes(FTCMarkerPainter& P, const ATCGameMode& M, UFont* Font)
{
	const TArray<FTCResourceNode>& Nodes = M.Markers.ResourceNodes();
	if (!M.State.bHasMatch || !Nodes.Num()) return;
	const MC::FRibbon O = MC::NodeRibbon();
	for (const FTCResourceNode& N : Nodes)
	{
		for (int32 C = 0; C < 4; C++)
		{
			double B[6];
			MC::NodeBracket(C, N.X, N.Z, B);
			P.Line(B, 3, O, MC::NodeLook(N.bFuel));
		}
	}
	const float Fade = M.WarMap.LabelFade();
	for (const FTCResourceNode& N : Nodes) Stats().Tags += P.Tag(Font, FTCLocale::T(FTCMarkers::NodeTag(N)), N.X, N.Z, MC::NodeTagLift, MC::TagStyle(), Fade);
	Stats().Nodes += Nodes.Num();
}

int32 TCMarkersHud::DrawRangeRings(FTCMarkerPainter& P, const ATCGameMode& M, const TSet<int32>& Selected)
{
	P.SetDepthTest(false); // markers.js selectionRing: range rings draw through models
	const FTCMatchState& S = M.State;
	if (!S.bHasMatch || !Selected.Num()) return 0;
	// rangeRings(): the types of the selection in its order (a missing unit has none)
	TArray<FString> Types;
	TArray<bool> Held;
	for (int32 Id : Selected) { const FTCUnit* U = S.Units.Find(Id); Types.Add(U ? U->Type : FString()); Held.Add(U != nullptr); }
	const bool bShown = MC::RangeRingsShown(Selected.Num(), Types.Num(), [&](int32 A, int32 B) { return Held[A] == Held[B] && Types[A] == Types[B]; });
	if (!bShown) return 0;
	int32 Rings = 0;
	for (int32 Id : Selected)
	{
		const FTCUnit* U = S.Units.Find(Id);
		double Range = 0, Min = 0;
		if (!U || !Ranges(U->Type, Range, Min)) continue;
		double Radii[2];
		const int32 N = MC::RangeRadii(Range, Min, Radii);
		FVector2D At(U->X, U->Z);
		if (M.Units) M.Units->DrawnAt(Id, At);
		for (int32 I = 0; I < N; I++) { P.Ring(At.X, At.Y, Radii[I], MC::RangeRibbon(), MC::RangeLook()); Rings++; }
	}
	Stats().RangeRings += Rings;
	return Rings;
}

void TCMarkersHud::DrawCapturePoint(FTCMarkerPainter& P, const FTCCapturePoint& Point, const FTCPointRingFrame& Frame)
{
	MC::FCaptureState St;
	St.bOwner = Point.RingSlot >= 0;
	St.OwnerHex = St.bOwner ? HexOf(ATCUnitRenderer::SeatColor(Point.RingSlot)) : 0;
	St.bArc = Point.bArcVisible && Point.ArcSlot >= 0;
	St.ArcHex = St.bArc ? HexOf(ATCUnitRenderer::SeatColor(Point.ArcSlot)) : 0;
	St.ArcSegments = Point.ArcSegments;
	St.bDanger = Frame.bDanger; St.DangerOpacity = Frame.DangerOpacity;
	St.bFlip = Frame.bFlip; St.FlipOpacity = Frame.FlipOpacity; St.FlipScale = Frame.FlipScale;
	for (const MC::FRingDraw& D : MC::CaptureRings(St)) P.Ring(Point.Centre.X, Point.Centre.Y, D.R, D.Ribbon, D.Look, D.Segments);
	Stats().CapturePoints++;
}

void TCMarkersHud::DrawCaptureLabels(FTCMarkerPainter& P, const FTCCapturePoint& Point, const ATCGameMode& M, UFont* Font)
{
	const float Fade = M.WarMap.LabelFade();
	if (Point.bTagVisible) Stats().Tags += P.Tag(Font, FTCLocale::T(Point.Label), Point.Centre.X, Point.Centre.Y, MC::PointTagLift, MC::TagStyle(), Fade);
	if (Point.CutTag()) Stats().Tags += P.Tag(Font, FTCLocale::T(TEXT("Cut off: no supply")), Point.Centre.X, Point.Centre.Y, 13, MC::TagStyle(), Fade);
	if (Point.LockTag()) Stats().Tags += P.Tag(Font, FTCLocale::T(TEXT("Locked: take the linked point first")), Point.Centre.X, Point.Centre.Y, 15, MC::TagStyle(), Fade);
}

void TCMarkersHud::DrawWorldRegions(FTCMarkerPainter& P, ATCGameMode& M, UFont* Font)
{
	if (!M.WorldRec.IsWorld() || !M.Terrain) return;
	const double Now = FPlatformTime::Seconds();
	struct FTagged { FVector2D At; FString Text, Name; bool bText; };
	TArray<FTagged> Tags;
	const TArray<FTCWorldRegionView> Views = M.WorldRec.RegionViews();
	for (const FTCWorldRegionView& V : Views)
	{
		const FVector C = P.ProjectWorld(P.Ground(V.At.X, V.At.Y, 0));
		if (!P.OnOrNearScreen(C)) continue;
		P.SetBeforeFog(true);
		// the border: LineSegments (runs) or a LineLoop (bounds), 0.55 opaque, 0.2 m over the ground (1 px lines)
		FLinearColor Border(V.TeamColour);
		Border.A = (float)FTCWorldRecovery::BorderOpacity;
		for (const TPair<FVector2D, FVector2D>& Sg : V.Segments) P.ScreenLine(P.Ground(Sg.Key.X, Sg.Key.Y, FTCWorldRecovery::BorderLift), P.Ground(Sg.Value.X, Sg.Value.Y, FTCWorldRecovery::BorderLift), Border);
		for (int32 I = 0; I < V.Loop.Num(); I++)
		{
			const FVector2D A = V.Loop[I], B = V.Loop[(I + 1) % V.Loop.Num()];
			P.ScreenLine(P.Ground(A.X, A.Y, FTCWorldRecovery::BorderLift), P.Ground(B.X, B.Y, FTCWorldRecovery::BorderLift), Border);
		}
		P.SetBeforeFog(false);
		// capturePoint(CFG.pointRadius, text).set(team >= 0 ? colour : null, capper colour, progress, contested)
		MC::FCaptureState St;
		St.bOwner = V.bOwner; St.OwnerHex = HexOf(V.TeamColour);
		St.bArc = V.Arc.bVisible; St.ArcHex = HexOf(V.ArcColour); St.ArcSegments = V.Arc.Segments;
		St.bDanger = V.bContested; St.DangerOpacity = MC::DangerOpacity(Now);
		for (const MC::FRingDraw& D : MC::CaptureRings(St)) P.Ring(V.At.X, V.At.Y, D.R, D.Ribbon, D.Look, D.Segments);
		// the flag: a 2.2 x 1.4 m cloth at 7.2 m in the owner's colour (else the neutral flag)
		const FLinearColor Flag = V.bOwner ? FLinearColor(V.TeamColour) : FLinearColor(FColor(0x8f, 0x8a, 0x7c));
		const FVector Cloth[4] = { FVector(V.At.X, V.At.Y, 6.5), FVector(V.At.X + 2.2, V.At.Y, 6.5), FVector(V.At.X + 2.2, V.At.Y, 7.9), FVector(V.At.X, V.At.Y, 7.9) };
		P.Quad(Cloth, Flag, V.At);
		const FTCWorldRegion& R = M.WorldRec.Snapshot().Regions[V.Index];
		Tags.Add({ V.At, FTCLocale::T(V.Text), FTCLocale::T(V.Name), M.Markers.RegionTagVisible(R.Id, V.Progress, Now) });
		Stats().Regions++;
	}
	const float Fade = M.WarMap.LabelFade();
	for (const FTagged& G : Tags)
	{
		if (G.bText) Stats().Tags += P.Tag(Font, G.Text, G.At.X, G.At.Y, MC::PointTagLift, MC::TagStyle(), Fade);
		Stats().Tags += P.Tag(Font, G.Name, G.At.X, G.At.Y, MC::RegionNameLift, MC::TagStyle(), Fade);
	}
}

void TCMarkersHud::DrawCoverLayer(FTCMarkerPainter& P, const TArray<FTCCoverGlyph>& Glyphs, float Opacity, bool bDepthTest)
{
	P.SetDepthTest(bDepthTest);
	// cover-preview.js templates: b.line(pts, n, { w: 0.18, min: 1.15, y: LIFT, closed: !cue }), the shield filled from
	// its centre (makeOverlay fill 0.3)
	MC::FRibbon Shield; Shield.W = 0.18; Shield.Min = 1.15; Shield.Y = FTCCoverPreview::LiftMetres; Shield.bClosed = true;
	MC::FRibbon Cue = Shield; Cue.bClosed = false;
	const TArrayView<const FVector2D> ShieldPts = FTCCoverPreview::ShieldOutline(), CuePts = FTCCoverPreview::ChevronLine();
	TArray<double> Pts;
	for (const FTCCoverGlyph& G : Glyphs)
	{
		const TArrayView<const FVector2D> Shape = G.bChevron ? CuePts : ShieldPts;
		Pts.Reset();
		for (const FVector2D& Q : Shape) { const FVector2D W = G.Place(Q.X, Q.Y); Pts.Add(W.X); Pts.Add(W.Y); }
		MC::FLook L;
		L.Hex = HexOf(G.Color);
		L.Opacity = (double)G.Alpha * Opacity;
		L.Fill = G.bChevron ? 0.0 : FTCCoverPreview::FillAlpha;
		const double Hub[2] = { G.X, G.Z };
		P.Line(Pts.GetData(), Shape.Num(), G.bChevron ? Cue : Shield, L, G.bChevron ? nullptr : Hub);
	}
}

TCMarkersHud::FFrameStats& TCMarkersHud::Stats()
{
	static FFrameStats S;
	return S;
}

void TCMarkersHud::Verify(ATCGameMode& M, TSet<int32>& Selected)
{
	static bool bParsed = false, bLog = false;
	static FString SelectType;
	static int32 Seat = 0;
	static double NextLog = 0;
	if (!bParsed)
	{
		bParsed = true;
		FParse::Value(FCommandLine::Get(), TEXT("TCMarkersSelect="), SelectType);
		FParse::Value(FCommandLine::Get(), TEXT("TCMarkersSeat="), Seat);
		bLog = FParse::Param(FCommandLine::Get(), TEXT("TCMarkersLog"));
		if (!SelectType.IsEmpty() || bLog) UE_LOG(LogTC, Log, TEXT("markers: verification switches: select '%s' on seat %d, log %d"), *SelectType, Seat, bLog ? 1 : 0);
	}
	const FTCMatchState& S = M.State;
	if (!S.bHasMatch) return;
	if (!SelectType.IsEmpty())
	{
		int32 Best = -1;
		for (const auto& P : S.Units)
			if (P.Value.Type == SelectType && P.Value.Owner == Seat && P.Value.Hp > 0 && (Best < 0 || P.Key < Best)) Best = P.Key;
		// set every frame: the controller prunes a spectator's selection to its own units, which it has none of
		static int32 Logged = -1;
		if (Best >= 0)
		{
			Selected.Reset();
			Selected.Add(Best);
			if (Best != Logged) UE_LOG(LogTC, Log, TEXT("markers: selected %s %d of seat %d (verification switch, nothing sent)"), *SelectType, Best, Seat);
			Logged = Best;
		}
	}
	const double Now = FPlatformTime::Seconds();
	if (bLog && Now >= NextLog)
	{
		NextLog = Now + 5.0;
		FString Nodes, Vets;
		for (const FTCResourceNode& N : M.Markers.ResourceNodes()) Nodes += FString::Printf(TEXT(" %s@%s,%s"), *FTCMarkers::NodeTag(N), *JsNum(N.X), *JsNum(N.Z));
		int32 VetCount = 0;
		for (const auto& P : S.Units)
			if (P.Value.Stars > 0)
			{
				if (VetCount++ < 6) Vets += FString::Printf(TEXT(" %s %d (seat %d) %d star(s) at %.0f,%.0f;"), *P.Value.Type, P.Key, P.Value.Owner, P.Value.Stars, P.Value.X, P.Value.Z);
			}
		FString Spawns;
		for (int32 I = 0; I < S.Spawns.Num(); I++) if (S.Spawns[I].IsSet()) Spawns += FString::Printf(TEXT(" %d@%.0f,%.0f"), I, S.Spawns[I]->X, S.Spawns[I]->Y);
		const FFrameStats& F = Stats();
		UE_LOG(LogTC, Log, TEXT("markers: tick %d; spawns%s; nodes %d%s; selected %d; drawn last frame: %d HQ camps, %d nodes, %d capture points, %d World regions, %d range rings, %d stars, %d tags; veterans %d%s"),
			S.Tick, *Spawns, M.Markers.ResourceNodes().Num(), *Nodes, Selected.Num(), F.HqCamps, F.Nodes, F.CapturePoints, F.Regions, F.RangeRings, F.Stars, F.Tags, VetCount, *Vets);
	}
	Stats() = FFrameStats();
}

FLinearColor TCMarkersHud::WorldColour(UCanvas* Canvas, const FLinearColor& Colour, const FVector& World)
{
	const ATCLighting* Lighting = MarkerLighting(Canvas);
	if (!Lighting) return Colour;
	const ATCLighting& L = *Lighting;
	const FSceneView& View = *Canvas->SceneView;
	const float Depth = FVector::DotProduct(World - View.ViewLocation, View.ViewRotation.Vector()) / 100.0;
	return TCLook::BillboardColour(Colour, Depth, L.FogNear(), L.FogFar(), L.Shown().Haze, L.Shown().Exposure, L.HazeBeforeTone());
}

void TCMarkersHud::DrawCoverShield(UCanvas* Canvas, FVector2D Centre, float Scale, int32 Cover, bool bGarrisoned, const FVector& World)
{
	const MC::FCoverShield Look = MC::CoverShield(Cover, bGarrisoned);
	if (!Canvas || Look.Alpha <= 0) return;
	const FLinearColor Colour = WorldColour(Canvas, FTCMarkerPainter::Colour(Look.Hex, Look.Alpha), World);
	FVector2D Vertices[5];
	for (int32 I = 0; I < 5; I++)
		Vertices[I] = FVector2D(Centre.X + (1.55 + MC::ShieldPoints[I][0] * 0.5) * Scale,
			Centre.Y - MC::ShieldPoints[I][1] * 0.5 * Scale);
	TArray<FCanvasUVTri> Triangles;
	for (int32 I = 1; I < 4; I++)
	{
		FCanvasUVTri& T = Triangles.AddDefaulted_GetRef();
		T.V0_Pos = Vertices[0]; T.V1_Pos = Vertices[I]; T.V2_Pos = Vertices[I + 1];
		T.V0_Color = T.V1_Color = T.V2_Color = Colour;
	}
	FCanvasTriangleItem Item(Triangles, GWhiteTexture);
	Item.BlendMode = SE_BLEND_Translucent;
	Canvas->DrawItem(Item);
}

void TCMarkersHud::DrawBadge(UCanvas* Canvas, const FString& Type, int32 Owner, FVector2D Centre, float Scale)
{
	if (!Canvas) return;
	int32 TypeIndex = INDEX_NONE;
	for (int32 I = 0; I < UE_ARRAY_COUNT(BadgeTypes); I++) if (Type == BadgeTypes[I]) { TypeIndex = I; break; }
	if (TypeIndex == INDEX_NONE) return;
	static TWeakObjectPtr<UTexture2D> Atlas;
	if (!Atlas.IsValid()) Atlas = LoadObject<UTexture2D>(nullptr, TEXT("/Game/Markers/T_TCUnitBadges.T_TCUnitBadges"));
	if (!Atlas.IsValid()) return;
	const int32 Index = TypeIndex * BadgeColours + (Owner >= 0 && Owner < BadgeColours - 1 ? Owner : BadgeColours - 1);
	const FVector2D Uv0(double(Index % BadgeColumns) * 128 / BadgeWidth, double(Index / BadgeColumns) * 128 / BadgeHeight);
	const FVector2D Uv1 = Uv0 + FVector2D(128.0 / BadgeWidth, 128.0 / BadgeHeight);
	const double Size = 1.45 * Scale;
	// markers.js symbolBadge: local centre (-1.98, +0.08), 1.45 m square, billboarded without depth testing.
	const FVector2D At = Centre + FVector2D(-1.98 * Scale - Size / 2, -0.08 * Scale - Size / 2);
	if (UMaterialInstanceDynamic* Mat = BadgeMaterial(Canvas, Atlas.Get(), BillboardDepth(Canvas, Scale)))
	{
		FCanvasTileItem Tile(At, Mat->GetRenderProxy(), FVector2D(Size, Size), Uv0, Uv1);
		Canvas->DrawItem(Tile);
	}
	else
	{
		FCanvasTileItem Tile(At, Atlas->GetResource(), FVector2D(Size, Size), Uv0, Uv1, FLinearColor::White);
		Tile.BlendMode = SE_BLEND_Translucent; Canvas->DrawItem(Tile);
	}
}

void TCMarkersHud::DrawHealthBar(UCanvas* Canvas, FVector2D Centre, float Scale, int32 Owner, float Fraction, int32 Suppression, const FVector& World)
{
	if (!Canvas || Scale <= 0) return;
	auto Rect = [&](double X, double Y, double W, double H, FLinearColor Colour)
	{
		FCanvasTileItem Tile(Centre + FVector2D(X, Y) * Scale, FVector2D(W, H) * Scale, WorldColour(Canvas, Colour, World));
		Tile.BlendMode = SE_BLEND_Translucent; Canvas->DrawItem(Tile);
	};
	// main.js makeUnit and applySnapshot. These are world metres, with no minimum pixel size.
	Rect(-1.2, -.21, 2.4, .42, FLinearColor(FColor(0x11,0x11,0x11)));
	const FLinearColor Hp = Fraction > .5f ? ATCUnitRenderer::SeatColor(Owner) : Fraction > .25f ?
		FLinearColor(FColor(0xe0,0x8a,0x2a)) : FLinearColor(FColor(0xd0,0x2a,0x1a));
	Rect(-1.15, -.17, 2.3 * FMath::Clamp(Fraction,0.f,1.f), .2, Hp);
	if (Suppression > 0) Rect(-1.15, .06, 2.3 * FMath::Min(1.f,Suppression / 100.f), .1,
		Suppression >= 90 ? FLinearColor(FColor(0xff,0x3b,0x2a)) : FLinearColor(FColor(0xff,0xd2,0x3a)));
}

void TCMarkersHud::DrawStars(UCanvas* Canvas, FVector2D BarCentre, float PxPerMetre, int32 Stars)
{
	const int32 N = MC::StarsShown(Stars);
	if (!Canvas || N <= 0) return;
	Stats().Stars += N;
	FTCMarkerPainter P(Canvas, nullptr, nullptr, FVector::ZeroVector, FVector::RightVector, FVector::ForwardVector);
	FLinearColor Gold = FTCMarkerPainter::Colour(MC::Star, 1.0);
	if (const ATCLighting* L = MarkerLighting(Canvas)) Gold = TCLook::BillboardColour(Gold, BillboardDepth(Canvas, PxPerMetre),
		L->FogNear(), L->FogFar(), L->Shown().Haze, L->Shown().Exposure, L->HazeBeforeTone());
	const double Half = MC::StarSide * FMath::Sqrt(2.0) / 2.0 * PxPerMetre;
	for (int32 I = 0; I < N; I++) P.Diamond(BarCentre + FVector2D(MC::StarX(I) * PxPerMetre, -MC::StarRise * PxPerMetre), Half, Gold);
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Client/TCMarkers.Oracle.inl"
#include "Engine/TextureRenderTarget2D.h"
#include "RenderingThread.h"

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMarkerBillboardMaterialTest, "ThreeCrossroads.Markers.BillboardMaterial", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMarkerBillboardMaterialTest::RunTest(const FString&)
{
	UMaterial* Base = LoadObject<UMaterial>(nullptr, TEXT("/Game/Markers/M_TCMarkerBillboard.M_TCMarkerBillboard"));
	if (!TestNotNull(TEXT("generated source badge fog material"), Base)) return false;
	TestTrue(TEXT("badge draws after world tone mapping in the UI domain"), Base->MaterialDomain == MD_UI);
	TestTrue(TEXT("badge preserves the source texture's alpha"), Base->BlendMode == BLEND_Translucent);
	TCFogLook::PrepareMaterial(Base);
	TStrongObjectPtr<UTexture2D> Tex(UTexture2D::CreateTransient(1, 1, PF_B8G8R8A8));
	const FColor Ink(22,25,27,224);
	void* Bytes = Tex->GetPlatformData()->Mips[0].BulkData.Lock(LOCK_READ_WRITE);
	FMemory::Memcpy(Bytes, &Ink, sizeof(Ink)); Tex->GetPlatformData()->Mips[0].BulkData.Unlock();
	Tex->SRGB = true; Tex->Filter = TF_Nearest; Tex->UpdateResource();
	TStrongObjectPtr<UTextureRenderTarget2D> Target(NewObject<UTextureRenderTarget2D>());
	Target->ClearColor = FLinearColor::Transparent;
	Target->InitCustomFormat(16,16,PF_FloatRGBA,true);
	TStrongObjectPtr<UMaterialInstanceDynamic> Mid(UMaterialInstanceDynamic::Create(Base, GetTransientPackage()));
	Mid->SetTextureParameterValue(TEXT("Badge"), Tex.Get());
	const FLinearColor Haze = TCLook::HexLinear(0xc9cbc6);
	Mid->SetVectorParameterValue(TEXT("Haze"), Haze); Mid->SetScalarParameterValue(TEXT("Exposure"), .9f);
	for (const bool High : {false,true}) for (const float Fog : {0.f,.15625f,.5f,.84375f,1.f})
	{
		Mid->SetScalarParameterValue(TEXT("BeforeTone"), High ? 1.f : 0.f); Mid->SetScalarParameterValue(TEXT("Fog"), Fog);
		Target->UpdateResourceImmediate(true);
		FTextureRenderTargetResource* Resource = Target->GameThread_GetRenderTargetResource();
		FCanvas Canvas(Resource, nullptr, static_cast<UWorld*>(nullptr), ERHIFeatureLevel::SM5);
		FCanvasTileItem Tile(FVector2D::ZeroVector, Mid->GetRenderProxy(), FVector2D(16,16));
		Canvas.DrawItem(Tile); Canvas.Flush_GameThread(); FlushRenderingCommands();
		TArray<FLinearColor> Pixels;
		if (!TestTrue(TEXT("badge material GPU readback"), Resource->ReadLinearColorPixels(Pixels, FReadSurfaceDataFlags(RCM_MinMax)))) return false;
		// BillboardColour receives depth, so invert this finite source smoothstep fixture for the CPU comparison.
		const float T = Fog == .15625f ? .25f : Fog == .84375f ? .75f : Fog;
		// Canvas material tiles write encoded RGB directly. Ordinary texture/colour tiles apply their own transfer.
		FLinearColor Expected = TCLook::ToDisplay(TCLook::BillboardColour(FLinearColor(Ink), T, 0, 1, Haze, .9f, High));
		Expected.R *= Expected.A; Expected.G *= Expected.A; Expected.B *= Expected.A;
		const FLinearColor Pixel = Pixels[8 * 16 + 8];
		TestTrue(*FString::Printf(TEXT("%s fog %.5f GPU RGB matches source CPU curve: %s expected %s"), High ? TEXT("High") : TEXT("Low"), Fog, *Pixel.ToString(), *Expected.ToString()),
			FVector(Pixel.R,Pixel.G,Pixel.B).Equals(FVector(Expected.R,Expected.G,Expected.B), .003));
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMarkersWorldTagContextTest, "ThreeCrossroads.Markers.WorldTagContext", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMarkersWorldTagContextTest::RunTest(const FString&)
{
	// Exercise the actual transient HUD canvas. Numeric fog tests alone cannot catch a skipped world lookup.
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCWorldTagContext"));
	if (!TestNotNull(TEXT("transient scene world"), World)) return false;
	{
		FSceneViewFamilyContext Family(FSceneViewFamily::ConstructionValues(nullptr, World->Scene, FEngineShowFlags(ESFIM_Game)));
		FSceneViewInitOptions Options;
		Options.ViewFamily = &Family;
		Options.SetViewRectangle(FIntRect(0, 0, 1280, 720));
		Options.ViewRotationMatrix = FMatrix::Identity;
		Options.ProjectionMatrix = FMatrix::Identity;
		FSceneView View(Options);
		TStrongObjectPtr<UCanvas> Canvas(NewObject<UCanvas>());
		Canvas->SceneView = &View;
		Canvas->ClipY = 720;
		TestEqual(TEXT("billboard scale recovers view depth without screen-size clamping"), BillboardDepth(Canvas.Get(),18.f), 20.f);
		TestNull(TEXT("a HUD canvas is not a world object"), Canvas->GetWorld());
		TestTrue(TEXT("world tags resolve the rendered scene for weather fog"), MarkerWorld(Canvas.Get()) == World);
		Canvas->SceneView = nullptr;
		TestNull(TEXT("screen-only canvases have no world tag fog"), MarkerWorld(Canvas.Get()));
	}
	World->DestroyWorld(false);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMarkersCoverShieldTest, "ThreeCrossroads.Markers.CoverShield", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMarkersCoverShieldTest::RunTest(const FString&)
{
	// main.js COVER_LOOK and GEO.shield at e4e579a. A shield shows even without a health bar.
	for (int32 Cover = 0; Cover <= 4; Cover++)
	{
		const auto S = MC::CoverShield(Cover, false);
		TestEqual(TEXT("only cover states 1, 2 and 3 show a shield"), S.Alpha > 0, Cover >= 1 && Cover <= 3);
		TestEqual(TEXT("garrison hides every cover shield"), MC::CoverShield(Cover, true).Alpha, 0.0);
	}
	TestEqual(TEXT("cover is source gold"), MC::CoverShield(1, false).Hex, uint32_t(0xe2b850));
	TestEqual(TEXT("trench is source green"), MC::CoverShield(2, false).Hex, uint32_t(0xb2de92));
	TestEqual(TEXT("one-sided cover is 45 percent"), MC::CoverShield(3, false).Alpha, 0.45);
	TestEqual(TEXT("shield point after 0.5 scale, relative to bar"), 1.55 + MC::ShieldPoints[3][0] * 0.5, 1.55);
	TestEqual(TEXT("shield point is 0.3 m below bar"), -MC::ShieldPoints[3][1] * 0.5, 0.3);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMarkersUnicodeTest, "ThreeCrossroads.Markers.UnicodeLabels", EAutomationTestFlags::EditorContext | EAutomationTestFlags::EngineFilter)
bool FTCMarkersUnicodeTest::RunTest(const FString&)
{
	if (!TestTrue(TEXT("reference Barlow loads"), FTCType::IsLoaded())) return false;
	const FTCTypeFace* Fallback = TCMarkerText::FallbackFace();
	if (!TestNotNull(TEXT("packaged engine fallback metrics load"), Fallback)) return false;
	TestTrue(TEXT("the source VP star has a real fallback glyph"), Fallback->Glyph(0x2605) != 0);
	TestEqual(TEXT("star width uses that glyph instead of Barlow's missing box"), float(TCMarkerText::Width(TEXT("★"), 30, 0)), Fallback->Width(TEXT("★"), 30, 0, false));
	TestEqual(TEXT("income label keeps reference Barlow metrics"), float(TCMarkerText::Width(TEXT("+1 MP/s"), 30, 0)), FTCType::Face(true)->Width(TEXT("+1 MP/s"), 30, 0, false));
	TestNotNull(TEXT("mixed marker typeface is available"), TCMarkerText::Font());
	return true;
}

// Run: -ExecCmds="Automation RunTests ThreeCrossroads.Markers; Quit" (no rendering needed).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMarkersGeometryTest, "ThreeCrossroads.Markers.Geometry", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCMarkersGeometryTest::RunTest(const FString& Parameters)
{
	const FBox2D Scores(FVector2D(400, 8), FVector2D(880, 130));
	TestTrue(TEXT("HQ tag moves below scores and its rate row"), TCMarkersHud::ClearHud(FVector2D(570, 115), FVector2D(156, 34), Scores).Equals(FVector2D(570, 138)));
	TestTrue(TEXT("a tag beside scores retains its world anchor"), TCMarkersHud::ClearHud(FVector2D(80, 115), FVector2D(128, 32), Scores).Equals(FVector2D(80, 115)));
	// every ring and line against the pinned overlay.js Builder (TCMarkers.Oracle.inl, ribbon-oracle.mjs)
	for (const TCMarkersOracle::FCase& C : TCMarkersOracle::Cases)
	{
		const FString Name = UTF8_TO_TCHAR(C.Name);
		MC::FRibbon O;
		double R = 0;
		std::vector<MC::FVertex> V;
		if (Name == TEXT("node-bracket")) { double P[6]; MC::NodeBracket(0, 0, 0, P); O = MC::NodeRibbon(); V = MC::Line(P, 3, O); }
		else
		{
			const std::vector<MC::FRingDraw> Point = MC::CaptureRings({ MC::PointRadius, true, 0x3b73d6, true, 0xcc3a2e, 20, true, 0.7, true, 0.5, 1.0 });
			if (Name == TEXT("capture-ring")) { O = Point[0].Ribbon; R = Point[0].R; }
			else if (Name == TEXT("capture-track")) { O = Point[1].Ribbon; R = Point[1].R; }
			else if (Name == TEXT("capture-arc")) { O = Point[2].Ribbon; R = Point[2].R; }
			else if (Name == TEXT("capture-flip")) { O = Point[3].Ribbon; R = Point[3].R; }
			else if (Name.StartsWith(TEXT("range-"))) { O = MC::RangeRibbon(); R = FCString::Atod(*Name.Mid(6)); }
			else if (Name == TEXT("hq-ring")) { O = MC::HqRingRibbon(); R = MC::HqRingRadius(); }
			V = MC::Ring(R, O);
		}
		TestEqual(FString::Printf(TEXT("%s: vertices along the ribbon"), *Name), (int32)V.size(), C.Side);
		TestEqual(FString::Printf(TEXT("%s: filled like the reference"), *Name), O.bFill, C.bHub);
		double Worst = 0;
		for (int32 J = 0; J < C.RowCount && (int32)V.size() == C.Side; J++)
		{
			const int32 I = J == C.RowCount - 1 ? C.Side - 1 : J * C.Every;
			const TCMarkersOracle::FRow& W = C.Rows[J];
			const double Got[9] = { V[I].X, V[I].Z, V[I].Nx, V[I].Nz, O.W / 2, O.Min, V[I].U, O.Core, O.Y };
			const double Ref[9] = { W.X, W.Z, W.Ex, W.Ez, W.Hw, W.Min, W.U, W.Core, W.Y };
			for (int32 K = 0; K < 9; K++) Worst = FMath::Max(Worst, FMath::Abs(Got[K] - Ref[K]) / (1.0 + FMath::Abs(Ref[K])));
		}
		TestTrue(FString::Printf(TEXT("%s: radius, half-width, normals and distances equal the reference (worst relative %g)"), *Name, Worst), Worst < 1e-6);
	}

	// the capture ring: R = 8 - 0.3, 0.42 m wide (0.21 half), a wash inside (0.07 held, 0.05 neutral); the ring's radius
	// at every vertex
	{
		const std::vector<MC::FRingDraw> Neutral = MC::CaptureRings(MC::FCaptureState());
		TestEqual(TEXT("neutral point: the ring only"), (int32)Neutral.size(), 1);
		TestTrue(TEXT("ring radius 8 - 0.3"), FMath::IsNearlyEqual(Neutral[0].R, 7.7, 1e-12));
		TestEqual(TEXT("ring width"), Neutral[0].Ribbon.W, 0.42);
		TestTrue(TEXT("ring filled"), Neutral[0].Ribbon.bFill);
		TestEqual(TEXT("neutral chalk"), (int64)Neutral[0].Look.Hex, (int64)0xe6e0d0);
		TestEqual(TEXT("neutral opacity"), Neutral[0].Look.Opacity, 0.8);
		TestEqual(TEXT("neutral wash"), Neutral[0].Look.Fill, 0.05);
		const MC::FLook Held = MC::CaptureRingLook(true, 0xcc3a2e);
		TestTrue(TEXT("held look"), Held.Hex == 0xcc3a2e && Held.Opacity == 0.95 && Held.Fill == 0.07);
		double Off = 0;
		for (const MC::FVertex& P : MC::Ring(7.7, Neutral[0].Ribbon, 50, 60)) Off = FMath::Max(Off, FMath::Abs(FMath::Sqrt(FMath::Square(P.X - 50) + FMath::Square(P.Z - 60)) - 7.7));
		TestTrue(TEXT("every vertex 7.7 m from the centre"), Off < 1e-9);
		MC::FCaptureState Taken; Taken.bArc = true; Taken.ArcHex = 0x3b73d6; Taken.ArcSegments = 16;
		const std::vector<MC::FRingDraw> T = MC::CaptureRings(Taken);
		TestTrue(TEXT("taken: ring, track, arc"), T.size() == 3 && FMath::IsNearlyEqual(T[1].R, 6.8, 1e-12) && T[1].Look.Opacity == 0.22 && FMath::IsNearlyEqual(T[2].R, 6.8, 1e-12) && T[2].Segments == 16 && T[2].Ribbon.W == 0.7);
		const std::vector<MC::FVertex> Quarter = MC::RingFirstSegments(6.8, T[2].Ribbon, 16);
		TestTrue(TEXT("16 of 64 segments: a quarter, clockwise from north to east"), Quarter.size() == 17 && FMath::IsNearlyEqual(Quarter.back().X, 6.8, 1e-9) && FMath::Abs(Quarter.back().Z) < 1e-9);
		MC::FCaptureState Hot; Hot.bDanger = true; Hot.DangerOpacity = MC::DangerOpacity(0); Hot.bFlip = true; Hot.FlipOpacity = 0.45; Hot.FlipScale = 1.03;
		const std::vector<MC::FRingDraw> H = MC::CaptureRings(Hot);
		TestTrue(TEXT("flip then danger, brass 0.5 m"), H.size() == 3 && H[1].Look.Hex == MC::Brass && FMath::IsNearlyEqual(H[1].R, 7.95 * 1.03) && FMath::IsNearlyEqual(H[2].R, 7.4, 1e-12) && H[2].Ribbon.W == 0.5);
		TestTrue(TEXT("danger pulse between 0.55 and 0.85"), FMath::IsNearlyEqual(MC::DangerOpacity(0), 0.7) && FMath::IsNearlyEqual(MC::DangerOpacity(1.0 / 3.2), 0.85));
	}

	// range rings: [range, minRange].filter(Boolean); the dashes fit the circle; shown for a small selection only
	{
		double Rr[2];
		TestEqual(TEXT("rifle: one ring"), MC::RangeRadii(28, 0, Rr), 1);
		TestEqual(TEXT("rocket: range and minimum"), MC::RangeRadii(70, 18, Rr), 2);
		TestEqual(TEXT("truck (range 0): none"), MC::RangeRadii(0, 0, Rr), 0);
		const std::vector<MC::FSpan> D = MC::RingDashes(28, MC::RangeDash[0], MC::RangeDash[1], MC::RangeDash[0]);
		TestEqual(TEXT("28 m: 73 dashes (round(2 pi 28 / 2.4))"), (int32)D.size(), 73);
		const double K = 73 * 2.4 / (2 * MC::Pi * 28);
		TestTrue(TEXT("each dash lit for 1.3 of 2.4 u"), FMath::IsNearlyEqual((D[0].A1 - D[0].A0) * 28 * K, 1.3, 1e-9) && FMath::IsNearlyEqual((D[1].A0 - D[0].A0) * 28 * K, 2.4, 1e-9));
		auto Shown = [](TArray<FString> Types) { return MC::RangeRingsShown(Types.Num(), Types.Num(), [&Types](int32 A, int32 B) { return Types[A] == Types[B]; }); };
		TestTrue(TEXT("two of any type"), Shown({ TEXT("rifle"), TEXT("tank") }));
		TestTrue(TEXT("four of one type"), Shown({ TEXT("mg"), TEXT("mg"), TEXT("mg"), TEXT("mg") }));
		TestFalse(TEXT("three of two types"), Shown({ TEXT("mg"), TEXT("mg"), TEXT("rifle") }));
		TestFalse(TEXT("five"), Shown({ TEXT("mg"), TEXT("mg"), TEXT("mg"), TEXT("mg"), TEXT("mg") }));
	}

	// overlay.js's fragment profile across a ribbon (FRAG_BODY, FRAG_END)
	{
		double Body, Alpha;
		MC::RibbonProfile(0.0, 0.01, 0.74, Body, Alpha);
		TestTrue(TEXT("wide ribbon, centre: body colour, full alpha"), Body == 1.0 && Alpha == 1.0);
		MC::RibbonProfile(0.9, 0.01, 0.74, Body, Alpha);
		TestTrue(TEXT("wide ribbon, edge band: edge colour at 0.55"), Body == 0.0 && FMath::IsNearlyEqual(Alpha, 0.55));
		MC::RibbonProfile(1.0, 0.01, 0.74, Body, Alpha);
		TestTrue(TEXT("outer border: faded out"), Alpha == 0.0);
		// a 0.42 m ring about 3.9 px wide: aa = 2 / 3.9; at v = 0.5 the body has started to fade into the edge
		MC::RibbonProfile(0.5, 2.0 / 3.9, 0.74, Body, Alpha);
		const double T = (0.5 - (0.74 - 2.0 / 3.9)) / (2.0 / 3.9), Want = 1.0 - T * T * (3 - 2 * T);
		TestTrue(TEXT("narrow ring: smoothstep body at v 0.5"), FMath::IsNearlyEqual(Body, Want, 1e-12));
		MC::RibbonProfile(-1.5, 0.3, 0.74, Body, Alpha);
		TestTrue(TEXT("inside a ring (|v| > 1): no ribbon alpha (the fill is separate)"), Alpha == 0.0 && Body == 1.0);
	}

	// veterancy, HQ and nodes
	{
		TestTrue(TEXT("stars: 0 to 3"), MC::StarsShown(0) == 0 && MC::StarsShown(2) == 2 && MC::StarsShown(7) == 3);
		TestTrue(TEXT("star places"), MC::StarX(0) == -0.5 && MC::StarX(1) == 0 && MC::StarX(2) == 0.5);
		TestTrue(TEXT("HQ ring 14.65 m, 0.36 m wide; zone 14.25 m"), FMath::IsNearlyEqual(MC::HqRingRadius(), 14.65, 1e-12) && MC::HqRingRibbon().W == 0.36 && FMath::IsNearlyEqual(MC::HqZoneRadius(), 14.25, 1e-12));
		const std::vector<double> Corner = MC::ClipToRect({ -10, -10, 10, -10, 10, 10, -10, 10 }, 0, 0, 160, 160);
		TestEqual(TEXT("a square over the map corner keeps its quarter"), (int32)Corner.size(), 8);
		TestTrue(TEXT("nothing left outside"), MC::ClipToRect({ -10, -10, -5, -10, -5, -5 }, 0, 0, 160, 160).empty());
		double B[6];
		MC::NodeBracket(2, 100, 50, B);
		const double Want[6] = { 101.85, 50.75, 101.85, 51.85, 100.75, 51.85 };
		bool bBracket = true;
		for (int32 I = 0; I < 6; I++) bBracket &= FMath::IsNearlyEqual(B[I], Want[I], 1e-9);
		TestTrue(TEXT("bracket (+, +)"), bBracket);
		TestTrue(TEXT("MP node brass, fuel orange"), MC::NodeLook(false).Hex == 0xd2a849 && MC::NodeLook(true).Hex == 0xe07a30 && MC::NodeLook(true).Opacity == 0.9);
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMarkersNodesTest, "ThreeCrossroads.Markers.Nodes", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCMarkersNodesTest::RunTest(const FString& Parameters)
{
	auto Parse = [](const TCHAR* Src) { TSharedPtr<FJsonObject> O; FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(FString(Src)), O); return O; };
	FTCMarkers M;
	M.ApplySnapshot(*Parse(TEXT("{\"t\":\"snap\",\"nodes\":[[41,77,1.5,0],[120,33,2,1]]}")));
	TestEqual(TEXT("two nodes"), M.ResourceNodes().Num(), 2);
	TestEqual(TEXT("MP tag"), FTCMarkers::NodeTag(M.ResourceNodes()[0]), FString(TEXT("+1.5 MP/s")));
	TestEqual(TEXT("fuel tag"), FTCMarkers::NodeTag(M.ResourceNodes()[1]), FString(TEXT("+2 Fuel/s")));
	M.ApplySnapshot(*Parse(TEXT("{\"t\":\"snap\"}")));
	TestEqual(TEXT("a snapshot without nodes keeps them"), M.ResourceNodes().Num(), 2);
	M.ApplySnapshot(*Parse(TEXT("{\"t\":\"snap\",\"nodes\":[[41,77,3,0],[60,61,0.5,0]]}")));
	TestEqual(TEXT("a new node is added, a known one kept as first marked"), M.ResourceNodes().Num(), 3);
	TestEqual(TEXT("the first mark keeps its rate"), FTCMarkers::NodeTag(M.ResourceNodes()[0]), FString(TEXT("+1.5 MP/s")));
	TestEqual(TEXT("0.5"), FTCMarkers::NodeTag(M.ResourceNodes()[2]), FString(TEXT("+0.5 MP/s")));
	M.ApplyStart();
	TestEqual(TEXT("a start drops them"), M.ResourceNodes().Num(), 0);
	// the World region tag steps aside for 2.5 s after the progress changes
	TestTrue(TEXT("first sight: shown"), M.RegionTagVisible(4, 0, 10.0));
	TestFalse(TEXT("progress moved: hidden"), M.RegionTagVisible(4, 0.1, 10.5));
	TestFalse(TEXT("still within 2.5 s"), M.RegionTagVisible(4, 0.1, 12.9));
	TestTrue(TEXT("back after 2.5 s"), M.RegionTagVisible(4, 0.1, 13.0));
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
