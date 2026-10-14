#include "Client/TCEffects.h"
#include "Client/TCAlerts.h"
#include "Client/TCUnitTable.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCLighting.h"
#include "Client/TCGraphics.h"
#include "Net/TCMatchState.h"
#include "World/TCTerrain.h"
#include "ThreeCrossroads.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Engine/StaticMesh.h"
#include "Engine/World.h"
#include "GameFramework/PlayerController.h"
#include "Camera/PlayerCameraManager.h"
#include "UObject/ConstructorHelpers.h"
#include "Camera/CameraTypes.h"
#include "Kismet/GameplayStatics.h"
#include "ConvexVolume.h"
#include "Components/DirectionalLightComponent.h"
#include "UObject/UObjectIterator.h"
#include "Engine/CollisionProfile.h"
#include "PrimitiveSceneProxy.h"
#include "PrimitiveViewRelevance.h"
#include "DynamicMeshBuilder.h"
#include "SceneManagement.h"
#include "SceneInterface.h"
#include "SceneView.h"
#include "Materials/MaterialInterface.h"
#include "Materials/MaterialRelevance.h"
#include "Engine/Texture.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Components/SkyLightComponent.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/Engine.h"
#include "Engine/TextureCube.h"
#include "Engine/TextureRenderTarget2D.h"
#include "GameFramework/WorldSettings.h"
#include "ImageUtils.h"
#include "TextureResource.h"
#include "RenderingThread.h"
#include "HAL/FileManager.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Misc/Parse.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Client/TCUnitTable.h"
#if WITH_EDITOR
#include "AssetCompilingManager.h"
#include "ShaderCompiler.h"
#endif

// ---------------------------------------------------------------------------------------------------------------------
// Raw fields the decoder does not keep

namespace TCFxJson
{
	const TArray<TSharedPtr<FJsonValue>>* ArrayOf(const TSharedPtr<FJsonValue>& V)
	{
		const TArray<TSharedPtr<FJsonValue>>* A = nullptr;
		return V.IsValid() && V->Type == EJson::Array && V->TryGetArray(A) ? A : nullptr;
	}
	const FJsonObject* ObjectOf(const TSharedPtr<FJsonValue>& V)
	{
		return V.IsValid() && V->Type == EJson::Object && V->AsObject().IsValid() ? V->AsObject().Get() : nullptr;
	}
	bool Num(const TSharedPtr<FJsonValue>& V, double& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		Out = V->AsNumber();
		return FMath::IsFinite(Out);
	}
	bool NumField(const FJsonObject& O, const TCHAR* Name, double& Out) { return Num(O.TryGetField(Name), Out); }
	bool Truthy(const TSharedPtr<FJsonValue>& V)
	{
		if (!V.IsValid()) return false;
		switch (V->Type)
		{
		case EJson::Boolean: return V->AsBool();
		case EJson::Number: { const double D = V->AsNumber(); return D != 0.0 && D == D; }
		case EJson::String: return !V->AsString().IsEmpty();
		case EJson::Array: case EJson::Object: return true;
		default: return false;
		}
	}
	FString Str(const TSharedPtr<FJsonValue>& V, int32 Max = 1024)
	{
		if (!V.IsValid() || V->Type != EJson::String) return FString();
		const FString S = V->AsString();
		return S.Len() > Max ? S.Left(Max) : S;
	}
}

TArray<FTCShotExtra> TCShotExtras::Read(const FJsonObject* Raw)
{
	using namespace TCFxJson;
	TArray<FTCShotExtra> Out;
	if (!Raw) return Out;
	const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(Raw->TryGetField(TEXT("shots")));
	if (!A) return Out;
	// FTCMatchState::ApplySnapshotExtras: entries past TCMatchLimits::Shots are dropped, non-objects skipped
	for (int32 I = 0; I < A->Num() && I < TCMatchLimits::Shots; I++)
	{
		const FJsonObject* O = ObjectOf((*A)[I]);
		if (!O) continue;
		FTCShotExtra E;
		E.bPhysical = Truthy(O->TryGetField(TEXT("physical")));
		double D;
		if (NumField(*O, TEXT("n"), D)) { E.bHasN = true; E.N = (int32)FMath::Clamp(D, -1e6, 1e6); }
		if (NumField(*O, TEXT("r"), D)) { E.bHasR = true; E.R = D; }
		if (NumField(*O, TEXT("dir"), D)) { E.bHasDir = true; E.Dir = D; }
		if (const TArray<TSharedPtr<FJsonValue>>* V = ArrayOf(O->TryGetField(TEXT("direction"))))
		{
			double X = 0, Y = 0, Z = 0;
			if (V->Num() >= 3 && Num((*V)[0], X) && Num((*V)[1], Y) && Num((*V)[2], Z)) { E.bHasDirection = true; E.Direction = FVector(X, Y, Z); }
		}
		if (NumField(*O, TEXT("impulse"), D)) { E.bHasImpulse = true; E.Impulse = D; }
		E.bHasHitField = O->HasField(TEXT("hit"));
		if (O->HasTypedField<EJson::String>(TEXT("text"))) { E.bHasText = true; E.Text = Str(O->TryGetField(TEXT("text"))); }
		if (const FJsonObject* L = ObjectOf(O->TryGetField(TEXT("localized"))))
		{
			if (L->HasTypedField<EJson::String>(TEXT("en"))) { E.bHasLocalizedEn = true; E.LocalizedEn = Str(L->TryGetField(TEXT("en"))); }
			if (L->HasTypedField<EJson::String>(TEXT("es"))) { E.bHasLocalizedEs = true; E.LocalizedEs = Str(L->TryGetField(TEXT("es"))); }
		}
		Out.Add(MoveTemp(E));
	}
	return Out;
}

namespace TCFxImpl
{
	const FTCShotExtra& ExtraAt(const TArray<FTCShotExtra>& Extras, int32 I)
	{
		static const FTCShotExtra None;
		return Extras.IsValidIndex(I) ? Extras[I] : None;
	}

	// wrecks: [id, type, owner, x, z, rot, motion?]; the motion object's pose (debris.js rows)
	bool ReadWreckMotion(const FJsonObject* Raw, TArray<FTCDebrisRow>& Out)
	{
		using namespace TCFxJson;
		if (!Raw) return false;
		const TArray<TSharedPtr<FJsonValue>>* A = ArrayOf(Raw->TryGetField(TEXT("wrecks")));
		if (!A) return false;
		Out.Reset();
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const TArray<TSharedPtr<FJsonValue>>* W = ArrayOf(V);
			if (!W || W->Num() < 7) continue;
			const FJsonObject* M = ObjectOf((*W)[6]);
			if (!M || !Truthy((*W)[6])) continue;
			FTCDebrisRow R;
			R.bWreck = true;
			double D;
			R.WreckId = Num((*W)[0], D) ? (int32)D : 0;
			R.HullDir = Num((*W)[5], D) ? D : 0.0;
			const TSharedPtr<FJsonValue> Id = M->TryGetField(TEXT("id"));
			R.Id = Id.IsValid() && Id->Type == EJson::Number ? FString::Printf(TEXT("%lld"), (long long)Id->AsNumber()) : Str(Id);
			R.Kind = Str(M->TryGetField(TEXT("kind")));
			R.Material = Str(M->TryGetField(TEXT("material")));
			NumField(*M, TEXT("x"), R.X); NumField(*M, TEXT("y"), R.Y); NumField(*M, TEXT("z"), R.Z);
			NumField(*M, TEXT("tilt"), R.Tilt); NumField(*M, TEXT("dir"), R.Dir);
			if (Out.Num() < TCMatchLimits::Falling) Out.Add(MoveTemp(R));
		}
		return true;
	}
}

// ---------------------------------------------------------------------------------------------------------------------
// client/projectiles.js

bool FTCProjectileView::Fresh(int32 Id, int32 Sequence)
{
	const int32* Old = Events.Find(Id);
	if (Sequence <= (Old ? *Old : -1)) return false;
	if (!Old) EventOrder.Add(Id);
	Events.Add(Id, Sequence);
	if (Events.Num() > TCFxRules::EventMemory && EventOrder.Num()) { Events.Remove(EventOrder[0]); EventOrder.RemoveAt(0); }
	return true;
}

void FTCProjectileView::Snapshot(const FTCMatchState& S, double Clock)
{
	if (S.Tick < LastTick) return;
	LastTick = S.Tick; ServerTime = S.Tick / 20.0; Arrival = Clock;
	TSet<int32> Keep;
	for (const FTCFlight& Row : S.Flights)
	{
		const int32* Terminal = Events.Find(Row.Flight);
		if ((Terminal ? *Terminal : -1) > Row.Seq) continue;
		if (!Flights.Contains(Row.Flight)) FlightOrder.Add(Row.Flight);
		Flights.Add(Row.Flight, Row);
		Keep.Add(Row.Flight);
	}
	for (const int32 Id : TArray<int32>(FlightOrder))
		if (!Keep.Contains(Id)) { Flights.Remove(Id); FlightOrder.Remove(Id); if (On.Remove) On.Remove(Id); }
	for (const FTCShot& Sh : S.Shots)
	{
		if (Sh.K == TEXT("flight"))
		{
			if (Fresh(Sh.Flight, 0) && ServerTime - Sh.Time < TCFxRules::LaunchFresh && On.Launch) On.Launch(Sh);
		}
		else if (Sh.K == TEXT("contact") || Sh.K == TEXT("expiry"))
		{
			if (!Fresh(Sh.Flight, Sh.Seq)) continue;
			// remove the old path before showing its contact, including a ricochet's correction
			if (On.Remove) On.Remove(Sh.Flight);
			if (Sh.bTerminal || Sh.K == TEXT("expiry")) { Flights.Remove(Sh.Flight); FlightOrder.Remove(Sh.Flight); }
			if (Sh.K == TEXT("contact") && ServerTime - Sh.Time < TCFxRules::ContactFresh && On.Contact) On.Contact(Sh);
		}
	}
}

void FTCProjectileView::Frame(double Clock)
{
	const double Time = ServerTime + FMath::Clamp(Clock - Arrival, 0.0, TCFxRules::ExtrapolateMax);
	for (const int32 Id : TArray<int32>(FlightOrder))
	{
		const FTCFlight* P = Flights.Find(Id);
		if (!P) continue;
		// an absent until or expires reads as 0 from the decoder; the browser compares with undefined (never true)
		if ((P->Until > 0.0 && Time > P->Until) || (P->Expires > 0.0 && Time >= P->Expires))
		{
			if (On.Remove) On.Remove(Id);
			Flights.Remove(Id); FlightOrder.Remove(Id);
			continue;
		}
		const double Dt = FMath::Max(0.0, Time - P->Time);
		FTCFlightDraw D;
		D.Flight = Id; D.Kind = P->Kind; D.Effect = P->Effect;
		D.P = FVector(P->X + P->V.X * Dt, P->Y + P->V.Y * Dt - 0.5 * P->Gravity * Dt * Dt, P->Z + P->V.Z * Dt);
		D.V = FVector(P->V.X, P->V.Y - P->Gravity * Dt, P->V.Z);
		if (On.Trail) On.Trail(D);
	}
}

void FTCProjectileView::Reset()
{
	for (const int32 Id : TArray<int32>(FlightOrder)) if (On.Remove) On.Remove(Id);
	Flights.Reset(); FlightOrder.Reset(); Events.Reset(); EventOrder.Reset();
	ServerTime = 0.0; Arrival = 0.0; LastTick = -1;
}

// ---------------------------------------------------------------------------------------------------------------------
// client/debris.js

void FTCDebrisView::Snapshot(const FTCMatchState& S, const TArray<FTCDebrisRow>& WreckMotion, const TArray<FTCShotExtra>& Extras, double Clock)
{
	if (S.Tick < LastTick) return;
	Duration = LastTick >= 0 ? FMath::Clamp((S.Tick - LastTick) / 20.0, 0.05, 0.2) : 0.1;
	LastTick = S.Tick; Arrival = Clock;
	TArray<FTCDebrisRow> Rows;
	for (const FTCFalling& F : S.Falling)
	{
		FTCDebrisRow R;
		R.Id = F.Id; R.Kind = F.Kind; R.Material = F.Material;
		R.X = F.X; R.Y = F.Y; R.Z = F.Z; R.Tilt = F.Tilt; R.Dir = F.Dir;
		Rows.Add(MoveTemp(R));
	}
	Rows.Append(WreckMotion);
	TSet<FString> Keep;
	for (const FTCDebrisRow& Row : Rows)
	{
		FBody* Old = Bodies.Find(Row.Id);
		const bool bNew = !Old;
		FBody B{ Old ? Old->Current : Row, Row };
		if (bNew) BodyOrder.Add(Row.Id);
		Bodies.Add(Row.Id, MoveTemp(B));
		Keep.Add(Row.Id);
		if (bNew && On.Pose) On.Pose(Row.Id, Row);
	}
	for (const FString& Id : TArray<FString>(BodyOrder))
		if (!Keep.Contains(Id)) { Bodies.Remove(Id); BodyOrder.Remove(Id); if (On.Remove) On.Remove(Id); }
	for (int32 I = 0; I < S.Shots.Num(); I++)
	{
		const FTCShot& Sh = S.Shots[I];
		if (Sh.K != TEXT("collapse") || Sh.Id.IsEmpty() || Events.Contains(Sh.Id)) continue;
		Events.Add(Sh.Id); EventOrder.Add(Sh.Id);
		if (EventOrder.Num() > TCFxRules::EventMemory) { Events.Remove(EventOrder[0]); EventOrder.RemoveAt(0); }
		if (S.Tick / 20.0 - Sh.At < TCFxRules::ContactFresh && On.Contact) On.Contact(Sh, TCFxImpl::ExtraAt(Extras, I));
	}
}

void FTCDebrisView::Frame(double Clock)
{
	const double F = FMath::Clamp((Clock - Arrival) / Duration, 0.0, 1.0);
	for (const FString& Id : BodyOrder)
	{
		const FBody* B = Bodies.Find(Id);
		if (!B || !On.Pose) continue;
		FTCDebrisRow Row = B->Current;
		Row.X = B->Previous.X + (B->Current.X - B->Previous.X) * F;
		Row.Y = B->Previous.Y + (B->Current.Y - B->Previous.Y) * F;
		Row.Z = B->Previous.Z + (B->Current.Z - B->Previous.Z) * F;
		Row.Tilt = B->Previous.Tilt + (B->Current.Tilt - B->Previous.Tilt) * F;
		On.Pose(Id, Row);
	}
}

void FTCDebrisView::Reset()
{
	for (const FString& Id : TArray<FString>(BodyOrder)) if (On.Remove) On.Remove(Id);
	Bodies.Reset(); BodyOrder.Reset(); Events.Reset(); EventOrder.Reset();
	LastTick = -1; Arrival = 0.0;
}

// ---------------------------------------------------------------------------------------------------------------------
// client/fx.js: the particle presets (FX), weapon styles, the FIRE table and the small tables of unit facts the
// dispatch reads

namespace TCFxPresets
{
	// three.js new Color(hex): the hex is sRGB, the colour linear (lin() in fx.js)
	static FVector3f Lin(uint32 Hex)
	{
		auto F = [](float C) { return C < 0.04045f ? C * 0.0773993808f : FMath::Pow(C * 0.9478672986f + 0.0521327014f, 2.4f); };
		return FVector3f(F(((Hex >> 16) & 255) / 255.f), F(((Hex >> 8) & 255) / 255.f), F((Hex & 255) / 255.f));
	}

	struct FTable
	{
		FTCFxPreset P[(int32)ETCFxPreset::Count];
		FTable()
		{
			using E = ETCFxPreset;
			using L = ETCFxLook;
			// pre(o): c1 defaults to c0, add1 to add
			auto Glow = [this](E Id, FVector3f C0, FVector3f C1, float Add, float Add1, int32 Cell, L Look) -> FTCFxPreset&
			{
				FTCFxPreset& X = P[(int32)Id];
				X.C0 = C0; X.C1 = C1; X.Add = Add; X.Add1 = Add1; X.Cell = Cell; X.Look = Look;
				return X;
			};
			auto Lit = [this](E Id, uint32 C0, uint32 C1, float A, int32 Cell, int32 Cells, L Look) -> FTCFxPreset&
			{
				FTCFxPreset& X = P[(int32)Id];
				X.bLit = true; X.C0 = Lin(C0); X.C1 = Lin(C1); X.A = A; X.Cell = Cell; X.Cells = Cells; X.Look = Look;
				return X;
			};
			// light: a brief soft flash, a fireball flipbook, flames, muzzle flashes, tracers, sparks, embers
			{ FTCFxPreset& X = Glow(E::Flash, { 3.2f, 2.7f, 2.1f }, { 2.2f, 1.3f, 0.6f }, 1.f, 1.f, AT::Glow, L::Flash); X.Grow = 1.35f; X.Pull = 0.6f; X.K = 0.8f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Glow(E::Fireball, { 1.9f, 1.5f, 1.15f }, { 1.1f, 0.75f, 0.55f }, 0.55f, 0.1f, AT::Fireball, L::Fireball); X.Flip = 15; X.Grow = 1.7f; X.bCurve = true; X.Rv = 0.3f; X.Drag = 1.4f; X.Wind = 0.3f; X.Grav = -0.9f; X.Pull = 0.4f; X.Fin = 0.f; X.K = 2.4f; }
			{ FTCFxPreset& X = Glow(E::Flame, { 2.0f, 1.55f, 1.2f }, { 1.7f, 1.2f, 0.85f }, 0.55f, 0.55f, AT::Flame, L::Flame); X.Flip = 12; X.Fps = 15; X.Grow = 1.12f; X.Grav = -0.25f; X.Drag = 1.f; X.Wind = 0.25f; X.Pull = 0.3f; X.bAnchor = true; X.bNoRot = true; X.Fin = 0.2f; X.K = 2.f; }
			{ FTCFxPreset& X = Glow(E::Burn, { 2.3f, 1.7f, 1.2f }, { 1.3f, 0.65f, 0.35f }, 0.6f, 0.6f, AT::Fireball + 1, L::Flame); X.Cells = 4; X.Grow = 0.7f; X.Rv = 1.5f; X.K = 1.3f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Glow(E::Muzzle, { 2.4f, 1.95f, 1.45f }, { 2.4f, 1.95f, 1.45f }, 0.85f, 0.85f, AT::Muzzle, L::Flash); X.Cells = 4; X.bFixLen = true; X.K = 1.4f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Glow(E::Tracer, { 2.7f, 1.35f, 0.5f }, { 2.7f, 1.35f, 0.5f }, 1.f, 1.f, AT::Tracer, L::Tracer); X.Len = 3.2f; X.K = 20.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Glow(E::TracerHot, { 3.0f, 2.3f, 1.3f }, { 3.0f, 2.3f, 1.3f }, 1.f, 1.f, AT::Tracer, L::Tracer); X.Len = 4.5f; X.K = 20.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Glow(E::TracerFaint, { 1.7f, 1.4f, 0.9f }, { 1.7f, 1.4f, 0.9f }, 1.f, 1.f, AT::Tracer, L::Tracer); X.A = 0.45f; X.Len = 2.2f; X.K = 20.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Glow(E::Rocket, { 3.2f, 2.2f, 1.2f }, { 3.f, 1.8f, 0.8f }, 1.f, 1.f, AT::Tracer, L::Rocket); X.Len = 1.8f; X.K = 20.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Glow(E::Spark, { 3.f, 2.2f, 1.1f }, { 1.8f, 0.45f, 0.08f }, 1.f, 1.f, AT::Spark, L::Spark); X.Grav = 14.f; X.Drag = 0.8f; X.Len = 0.45f; X.K = 1.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Glow(E::Ember, { 2.4f, 1.1f, 0.3f }, { 0.9f, 0.15f, 0.02f }, 1.f, 1.f, AT::Ember, L::Spark); X.Grav = -1.4f; X.Drag = 0.6f; X.Wind = 1.f; X.K = 1.2f; X.Fin = 0.f; }
			// lit: smoke (dark brown-grey from a blast, black from burning fuel, grey from guns, white from smoke shells)
			{ FTCFxPreset& X = Lit(E::Smoke, 0x5f574d, 0x8a857d, 0.72f, AT::Smoke, 4, L::Smoke); X.Grow = 2.8f; X.bCurve = true; X.Rv = 0.22f; X.Drag = 0.8f; X.Wind = 1.f; X.Grav = -0.8f; X.Pull = 0.45f; X.Fin = 0.12f; X.K = 1.7f; }
			{ FTCFxPreset& X = Lit(E::WreckSmoke, 0x2a2622, 0x5c5750, 0.78f, AT::Smoke, 4, L::DarkSmoke); X.Grow = 4.2f; X.bCurve = true; X.Rv = 0.12f; X.Drag = 0.5f; X.Wind = 1.5f; X.Grav = -0.9f; X.Pull = 0.45f; X.Fin = 0.1f; X.K = 1.4f; }
			{ FTCFxPreset& X = Lit(E::Gunsmoke, 0xa8a49c, 0xbab6ae, 0.42f, AT::Wisp, 4, L::Smoke); X.Grow = 3.f; X.bCurve = true; X.Rv = 0.3f; X.Drag = 2.2f; X.Wind = 1.f; X.Grav = -0.35f; X.Pull = 0.3f; X.Fin = 0.05f; X.K = 1.4f; }
			{ FTCFxPreset& X = Lit(E::Screen, 0xcfcdc5, 0xdad8d2, 0.9f, AT::Smoke, 4, L::Screen); X.bCover = true; X.Grow = 1.3f; X.bCurve = true; X.Rv = 0.07f; X.Drag = 0.4f; X.Wind = 1.f; X.Grav = -0.04f; X.Pull = 0.55f; X.Fin = 0.22f; X.K = 2.2f; }
			{ FTCFxPreset& X = Lit(E::FireSmoke, 0x2e2a26, 0x6a655e, 0.86f, AT::Smoke, 4, L::DarkSmoke); X.bCover = true; X.Grow = 1.9f; X.bCurve = true; X.Rv = 0.1f; X.Drag = 0.5f; X.Wind = 1.2f; X.Grav = -0.35f; X.Pull = 0.5f; X.Fin = 0.15f; X.K = 1.8f; }
			{ FTCFxPreset& X = Lit(E::Trail, 0xb4b0a8, 0xc4c0b8, 0.5f, AT::Wisp, 4, L::Smoke); X.Grow = 3.5f; X.bCurve = true; X.Rv = 0.4f; X.Drag = 1.5f; X.Wind = 1.f; X.Grav = -0.3f; X.Pull = 0.2f; X.Fin = 0.05f; X.K = 1.3f; }
			{ FTCFxPreset& X = Lit(E::Flak, 0x1c1a18, 0x3a3632, 0.92f, AT::Smoke, 4, L::DarkSmoke); X.Grow = 2.2f; X.bCurve = true; X.Rv = 0.2f; X.Drag = 1.2f; X.Wind = 1.f; X.Grav = -0.05f; X.Fin = 0.03f; X.K = 1.6f; }
			{ FTCFxPreset& X = Lit(E::AirSmoke, 0x8d887d, 0xa29e96, 0.7f, AT::Smoke, 4, L::Smoke); X.Grow = 2.6f; X.bCurve = true; X.Rv = 0.3f; X.Drag = 0.8f; X.Wind = 1.f; X.Grav = -0.3f; X.Fin = 0.05f; X.K = 1.5f; }
			{ FTCFxPreset& X = Lit(E::AirSmokeDark, 0x24211e, 0x48443e, 0.88f, AT::Smoke, 4, L::DarkSmoke); X.Grow = 2.8f; X.bCurve = true; X.Rv = 0.3f; X.Drag = 0.8f; X.Wind = 1.f; X.Grav = -0.3f; X.Fin = 0.05f; X.K = 1.5f; }
			// lit: dust and dirt
			{ FTCFxPreset& X = Lit(E::Dust, 0x7e705c, 0x8e8270, 0.5f, AT::Smoke, 4, L::Dust); X.Grow = 1.9f; X.bCurve = true; X.Rv = 0.2f; X.Drag = 1.6f; X.Wind = 0.6f; X.Grav = -0.12f; X.Pull = 0.35f; X.Fin = 0.06f; X.K = 1.5f; }
			{ FTCFxPreset& X = Lit(E::RoadDust, 0x9c8c70, 0xac9f88, 0.3f, AT::Smoke, 4, L::Dust); X.Grow = 3.2f; X.bCurve = true; X.Rv = 0.25f; X.Drag = 2.2f; X.Wind = 1.f; X.Grav = -0.3f; X.Pull = 0.3f; X.Fin = 0.12f; X.K = 1.5f; }
			{ FTCFxPreset& X = Lit(E::Collapse, 0x9a8e7a, 0xb0a692, 0.78f, AT::Smoke, 4, L::Dust); X.Grow = 2.3f; X.bCurve = true; X.Rv = 0.15f; X.Drag = 0.9f; X.Wind = 0.6f; X.Grav = -0.15f; X.Pull = 0.5f; X.Fin = 0.1f; X.K = 1.7f; }
			{ FTCFxPreset& X = Lit(E::Plume, 0x5c4b38, 0x8a7a64, 0.95f, AT::Plume, 4, L::Dust); X.Grow = 1.45f; X.Grav = 1.2f; X.Pull = 0.3f; X.bAnchor = true; X.bNoRot = true; X.Fin = 0.02f; X.K = 1.8f; }
			{ FTCFxPreset& X = Lit(E::Kick, 0x8a7860, 0x9c8c74, 0.85f, AT::Kick, 4, L::Dust); X.Grow = 1.35f; X.Grav = 0.4f; X.Pull = 0.25f; X.bAnchor = true; X.bNoRot = true; X.Fin = 0.03f; X.K = 1.6f; }
			// lit: debris with gravity that bounces on the ground
			{ FTCFxPreset& X = Lit(E::Clod, 0x4a3b2c, 0x4a3b2c, 1.f, AT::Soil, 2, L::Clod); X.Rv = 9.f; X.Grav = 22.f; X.bFloor = true; X.K = 8.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Lit(E::Masonry, 0xa89888, 0xa89888, 1.f, AT::Brick, 4, L::Debris); X.Rv = 8.f; X.Grav = 22.f; X.bFloor = true; X.K = 8.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Lit(E::Wood, 0x8a6a4a, 0x8a6a4a, 1.f, AT::Wood, 1, L::Debris); X.Rv = 9.f; X.Grav = 22.f; X.bFloor = true; X.K = 8.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Lit(E::Metal, 0x34302c, 0x34302c, 1.f, AT::Metal, 1, L::Debris); X.Rv = 11.f; X.Grav = 22.f; X.bFloor = true; X.K = 8.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Lit(E::Nade, 0x34342a, 0x34342a, 1.f, AT::Metal, 1, L::Grenade); X.Rv = 10.f; X.K = 20.f; X.Fin = 0.f; }
			// gore (gfx.gore, off by default in the browser; not emitted here)
			{ FTCFxPreset& X = Lit(E::Gib, 0x4a1410, 0x4a1410, 1.f, AT::Soil, 2, L::Clod); X.Rv = 10.f; X.Grav = 22.f; X.bFloor = true; X.K = 8.f; X.Fin = 0.f; }
			{ FTCFxPreset& X = Lit(E::Mist, 0x7a1a14, 0x4a1c18, 0.7f, AT::Smoke, 4, L::Smoke); X.Grow = 2.2f; X.bCurve = true; X.Rv = 0.3f; X.Drag = 2.f; X.Wind = 0.5f; X.Grav = 1.5f; X.Pull = 0.3f; X.Fin = 0.02f; X.K = 1.5f; }
		}
	};

	const FTCFxPreset& Get(ETCFxPreset Id)
	{
		static const FTable T;
		return T.P[FMath::Clamp((int32)Id, 0, (int32)ETCFxPreset::Count - 1)];
	}

	const TCHAR* Name(ETCFxPreset Id)
	{
		static const TCHAR* const Names[] = { TEXT("none"), TEXT("flash"), TEXT("fireball"), TEXT("flame"), TEXT("burn"), TEXT("muzzle"), TEXT("tracer"), TEXT("tracerHot"),
			TEXT("tracerFaint"), TEXT("rocket"), TEXT("spark"), TEXT("ember"), TEXT("smoke"), TEXT("wreckSmoke"), TEXT("gunsmoke"), TEXT("screen"), TEXT("fireSmoke"),
			TEXT("trail"), TEXT("flak"), TEXT("airSmoke"), TEXT("airSmokeDark"), TEXT("dust"), TEXT("roadDust"), TEXT("collapse"), TEXT("plume"), TEXT("kick"),
			TEXT("clod"), TEXT("masonry"), TEXT("wood"), TEXT("metal"), TEXT("nade"), TEXT("gib"), TEXT("mist") };
		static_assert(UE_ARRAY_COUNT(Names) == (int32)ETCFxPreset::Count, "a name per preset");
		return Names[FMath::Clamp((int32)Id, 0, (int32)ETCFxPreset::Count - 1)];
	}
}

struct FTCCombatEffects::FStyle
{
	const TCHAR* Snd = TEXT("rifle");
	int32 N = 3, Burst = 1, Guns = 0;
	float Gap = 0.f, Spd = 230.f, W = 0.05f;
	bool bTracer = true; ETCFxPreset Tr = ETCFxPreset::TracerFaint;
	float TrOdds = 0.45f, Flash = 0.32f, Heavy = 0.f, Smoke = 0.f, Blast = 0.f;
	bool bBack = false;
};

namespace TCFxImpl
{

	// UNITS[type].w for the salvo weapons (flight, every, spread) at e4e579a
	struct FSalvoW { const TCHAR* Type; double Flight, Every, Spread; };
	const FSalvoW SalvoWeapons[] = { { TEXT("rocket"), 1.2, 0.15, 6 }, { TEXT("mortar"), 1.5, 0.1, 3.5 }, { TEXT("destroyer"), 1.8, 0.1, 6 }, { TEXT("howitzer"), 2.2, 0.1, 3 }, { TEXT("bomber"), 1.0, 0.25, 6 } };
	const FSalvoW* SalvoOf(const FString& Type) { for (const FSalvoW& W : SalvoWeapons) if (Type == W.Type) return &W; return nullptr; }

	// client/fx.js FIRE: burning grass ('.'), hedges (H), woods (T), houses (B)
	struct FFireLook { TCHAR Ch; float H, Size, Rate, Smoke; };
	const FFireLook FireLooks[] = { { '.', 0.f, 0.6f, 2.5f, 0.15f }, { 'H', 0.4f, 1.1f, 3.5f, 0.5f }, { 'T', 1.2f, 1.3f, 3.f, 0.7f }, { 'B', 1.6f, 1.6f, 3.f, 0.7f } };
	const FFireLook& FireOf(TCHAR Ch) { for (const FFireLook& F : FireLooks) if (F.Ch == Ch) return F; return FireLooks[0]; }

	// shared/debris-motion.js SECTION_MASS
	double SectionMass(const FString& M) { return M == TEXT("wood") ? 350 : M == TEXT("stone") ? 1200 : M == TEXT("concrete") ? 1600 : M == TEXT("steel") ? 1800 : 1200; }

	// client/main.js: the shots client/aircraft.js draws (SUPPORT_PLANES and AIR_SHOTS)
	bool IsSupportPlane(const FString& K) { return K == TEXT("strafe") || K == TEXT("recon") || K == TEXT("dive") || K == TEXT("bombing") || K == TEXT("para"); }
	// aircraft.js LOW: how low each support plane flies over the ground
	double LowOf(const FString& K) { return K == TEXT("strafe") ? 9 : K == TEXT("bombing") ? 18 : K == TEXT("dive") ? 12 : K == TEXT("para") ? 24 : 26; }
	bool IsFlakType(const FString& T) { return T == TEXT("flak") || T == TEXT("flaktrack") || T == TEXT("flakpos"); }
}

const FTCCombatEffects::FStyle& FTCCombatEffects::StyleOf(const FString& Kind)
{
	// client/fx.js GUNS (SMALL for anything else); tr: SMALL's faint tracer, MGs' tracer, guns' hot tracer, rockets
	static const TMap<FString, FStyle> Guns = []()
	{
		TMap<FString, FStyle> G;
		const FStyle Small;
		auto Mg = [&](float Gap, float Flash, int32 Burst = 4) { FStyle S = Small; S.Snd = TEXT("mg"); S.N = 1; S.Burst = Burst; S.Gap = Gap; S.Spd = 210; S.W = 0.085f; S.Tr = ETCFxPreset::Tracer; S.TrOdds = 1; S.Flash = Flash; return S; };
		auto Gun = [&](float Spd, float W, float Flash, float Heavy, float Blast, const TCHAR* Snd = TEXT("tankgun")) { FStyle S = Small; S.Snd = Snd; S.N = 1; S.Spd = Spd; S.W = W; S.Tr = ETCFxPreset::TracerHot; S.TrOdds = 1; S.Flash = Flash; S.Heavy = Heavy; S.Smoke = 1; S.Blast = Blast; return S; };
		G.Add(TEXT("rifle"), Small); G.Add(TEXT("conscript"), Small);
		{ FStyle S = Small; S.N = 2; G.Add(TEXT("engineer"), S); }
		{ FStyle S = Small; S.Snd = TEXT("smg"); S.Burst = 2; S.Gap = 0.07f; S.Flash = 0.28f; G.Add(TEXT("ranger"), S); }
		{ FStyle S = Small; S.Snd = TEXT("smg"); S.Burst = 3; S.Gap = 0.06f; S.Flash = 0.26f; G.Add(TEXT("commando"), S); }
		G.Add(TEXT("mg"), Mg(0.065f, 0.45f));
		G.Add(TEXT("bunker"), Mg(0.08f, 0.5f)); G.Add(TEXT("pillbox"), Mg(0.08f, 0.5f));
		{ FStyle S = Small; S.Snd = TEXT("sniper"); S.N = 1; S.Spd = 420; S.W = 0.035f; S.TrOdds = 0.6f; S.Flash = 0.4f; G.Add(TEXT("sniper"), S); }
		G.Add(TEXT("at"), Gun(150, 0.16f, 1.3f, 2.4f, 1.f, TEXT("atgun")));
		G.Add(TEXT("tank"), Gun(150, 0.16f, 1.4f, 2.6f, 1.f));
		G.Add(TEXT("medium"), Gun(150, 0.18f, 1.7f, 3.f, 1.2f));
		G.Add(TEXT("tiger"), Gun(160, 0.2f, 2.1f, 3.4f, 1.4f));
		G.Add(TEXT("churchill"), Gun(150, 0.18f, 1.7f, 3.f, 1.2f));
		G.Add(TEXT("tankdestroyer"), Gun(165, 0.2f, 1.9f, 3.f, 1.2f));
		{ FStyle S = Small; S.Snd = TEXT("flak"); S.N = 1; S.Burst = 3; S.Gap = 0.06f; S.Spd = 40; S.W = 0.5f; S.Tr = ETCFxPreset::TracerHot; S.TrOdds = 1; S.Flash = 1.2f; G.Add(TEXT("flamer"), S); }
		{ FStyle S = Small; S.Snd = TEXT("tankgun"); S.N = 1; S.Burst = 8; S.Gap = 0.05f; S.Spd = 60; S.W = 1.2f; S.Tr = ETCFxPreset::TracerHot; S.TrOdds = 1; S.Flash = 3; S.Heavy = 4; S.Smoke = 1; S.Blast = 2; G.Add(TEXT("kaiju"), S); }
		G.Add(TEXT("halftrack"), Mg(0.08f, 0.45f, 3)); G.Add(TEXT("lcvp"), Mg(0.08f, 0.45f, 3));
		{ FStyle S = Small; S.Snd = TEXT("tankgun"); S.N = 1; S.Burst = 2; S.Gap = 0.12f; S.Spd = 170; S.W = 0.12f; S.Tr = ETCFxPreset::TracerHot; S.TrOdds = 1; S.Flash = 0.8f; S.Heavy = 1.5f; S.Smoke = 0.5f; G.Add(TEXT("gunboat"), S); G.Add(TEXT("armoredcar"), S); }
		{ FStyle S = Small; S.Snd = TEXT("flak"); S.N = 1; S.Burst = 2; S.Gap = 0.1f; S.Spd = 240; S.W = 0.11f; S.Tr = ETCFxPreset::TracerHot; S.TrOdds = 1; S.Flash = 0.6f; G.Add(TEXT("flak"), S); S.Burst = 4; S.Gap = 0.06f; G.Add(TEXT("flaktrack"), S); }
		{ FStyle S = Mg(0.05f, 0.45f); S.N = 2; S.Guns = 2; S.Spd = 280; G.Add(TEXT("fighter"), S); }
		{ FStyle S = Small; S.Snd = TEXT("rockets"); S.N = 1; S.Spd = 110; S.W = 0.2f; S.Tr = ETCFxPreset::Rocket; S.TrOdds = 1; S.Flash = 0.8f; S.Heavy = 2.6f; G.Add(TEXT("attacker"), S); }
		return G;
	}();
	static const FStyle Small;
	const FStyle* S = Guns.Find(Kind);
	return S ? *S : Small;
}

const FTCCombatEffects::FStyle& FTCCombatEffects::Bazooka()
{
	static const FStyle B = []() { FStyle S; S.Snd = TEXT("atgun"); S.N = 1; S.Spd = 70; S.W = 0.18f; S.Tr = ETCFxPreset::Rocket; S.TrOdds = 1; S.Flash = 0.7f; S.Heavy = 2.2f; S.Smoke = 1; S.bBack = true; return S; }();
	return B;
}

float FTCFxDecal::Fade() const
{
	// fx.js updateDecals
	return FMath::Min(1.f, Age / 0.3f) * FMath::Min(1.f, (Life - Age) / (Life * 0.3f));
}

FTCCombatEffects::FTCCombatEffects()
{
	Rng.Initialize(Seed);
	DecalPool.SetNum(TCFxRules::DecalCap);
	Projectiles.On.Remove = [this](int32 Flight) { ProjectileRemove(Flight); };
	Projectiles.On.Trail = [this](const FTCFlightDraw& D)
	{
		if (FlightDraws.Num() < TCMatchLimits::Flights) FlightDraws.Add(D);
		ProjectileTrail(D);
	};
	Projectiles.On.Launch = [this](const FTCShot& Sh)
	{
		// fx.js projectileLaunch: a filtered launch has no source id when its muzzle is outside current vision
		if (!Sh.bHasShooter || Sh.Effect == TEXT("boom")) { Counters.LaunchesHidden++; return; }
		// and this client's rule on top: the shooter must be one of this snapshot's held rows
		if (!State_ || !State_->Units.Contains(Sh.F)) { Counters.ShooterNotHeld++; return; }
		Counters.Launches++;
		NoteShooter(Sh.F);
		FStyle St = StyleOf(Sh.Kind);
		if (TCFxImpl::SalvoOf(Sh.Kind))
		{
			St = FStyle(); St.Snd = Sh.Kind == TEXT("destroyer") ? TEXT("tankgun") : Sh.Kind == TEXT("rocket") ? TEXT("rockets") : TEXT("mortar");
			St.Flash = Sh.Kind == TEXT("rocket") ? 0.5f : 1.3f; St.Smoke = 1;
		}
		const double D = FMath::Max(1e-9, FMath::Sqrt(Sh.V.X * Sh.V.X + Sh.V.Z * Sh.V.Z));
		const FVector P = At(Sh.X, Sh.bHasY ? Sh.Y : Ground(Sh.X, Sh.Z) + 1.0, Sh.Z);
		Muzzle(P, D > 1e-6 ? Sh.V.X / D : 1.0, D > 1e-6 ? Sh.V.Z / D : 0.0, St.Flash, St);
		const bool bBurstSnd = FCString::Strcmp(St.Snd, TEXT("mg")) == 0 || FCString::Strcmp(St.Snd, TEXT("smg")) == 0;
		const double Gap = FCString::Strcmp(St.Snd, TEXT("mg")) == 0 ? 1.25 : 0.7;
		const double* Last = LastBurst.Find(Sh.F);
		if (!bBurstSnd || !Last || ClockNow - *Last >= Gap) { Sound(St.Snd); LastBurst.Add(Sh.F, ClockNow); }
		if (LastBurst.Num() > 400) LastBurst.Reset();
	};
	Projectiles.On.Contact = [this](const FTCShot& Sh)
	{
		Counters.Contacts++;
		const int32 I = State_ ? (int32)(&Sh - State_->Shots.GetData()) : -1;
		static const TArray<FTCShotExtra> NoExtras;
		const FTCShotExtra& Ex = TCFxImpl::ExtraAt(CurrentExtras ? *CurrentExtras : NoExtras, I);
		WorldContact(Sh, Ex);
		const double Y = Sh.bHasY ? Sh.Y : Ground(Sh.X, Sh.Z);
		if (Sh.Effect == TEXT("boom")) { Explode(Sh.X, Sh.Z, Sh.Kind == TEXT("ranger") ? 3.2 : 2.0, true, true, ETCFxPreset::Clod, 1.0, Sh.bHasY ? &Y : nullptr); Sound(TEXT("blast_small")); return; }
		if (Sh.Effect == TEXT("rocket"))
		{
			const double Size = Sh.Kind == TEXT("mortar") ? 2.6 : Sh.Kind == TEXT("howitzer") ? 4.2 : 3.0;
			Explode(Sh.X, Sh.Z, Size, true, true, ETCFxPreset::Clod, 1.0, Sh.bHasY ? &Y : nullptr); Sound(TEXT("blast_small")); return;
		}
		const FStyle& St = StyleOf(Sh.Kind);
		const FVector P = At(Sh.X, Y, Sh.Z);
		if (Sh.bTerminal) Impact(P, St, Sh.bHit ? 1 : 0);
		else
		{
			// a nonterminal hard contact makes sparks; it never schedules a blast
			Emit(ETCFxPreset::Flash, P, FVector::ZeroVector, 0.18f, 0.035f);
			for (int32 K = 0; K < 4; K++) Emit(ETCFxPreset::Spark, P, At(Rr(-2, 2), Rr(1, 3), Rr(-2, 2)), 0.04f, 0.2f);
		}
	};
	Debris.On.Pose = [this](const FString& Id, const FTCDebrisRow& Row)
	{
		// sections are drawn here (fx.js sectionPose, at most DEBRIS_LIMITS.active); a wreck's hull is drawn by the world
		// objects, which hold its row
		if (Row.Kind != TEXT("section") || (!SectionPoses.Contains(Id) && SectionPoses.Num() >= 96)) return;
		SectionPoses.Add(Id, Row);
	};
	Debris.On.Remove = [this](const FString& Id) { SectionPoses.Remove(Id); };
	Debris.On.Contact = [this](const FTCShot& Sh, const FTCShotExtra& Ex) { Counters.Collapses++; WorldContact(Sh, Ex); };
}

void FTCCombatEffects::Start(const FTCMatchState& S)
{
	Reset();
	PrevHeld = S.Units;
}

void FTCCombatEffects::Reset()
{
	// fx.js reset, projectiles.reset, debris.reset (their removals change nothing drawn here)
	Projectiles.Reset(); Debris.Reset();
	Live.Reset(); FlightDraws.Reset(); SectionPoses.Reset(); Events.Reset(); Fires.Reset(); WreckFires.Reset(); Columns.Reset(); Clouds.Reset();
	Rockets.Reset(); Passes.Reset(); Falls.Reset(); Throws.Reset(); SalvoMarks.Reset(); LastBurst.Reset(); PrevHeld.Reset(); Killed.Reset(); KilledGone.Reset();
	HeldWreckMotion.Reset(); Sounds.Reset(); ShooterEffects.Reset(); // Counters run for the object's life (logs, tests)
	for (FTCFxDecal& D : DecalPool) D.bOn = false;
	ShakeAmount = 0.f; WindXZ = FVector2D(0.55, 0.25); TickNow = 0; RefitT = 0.f; SimTick = 0;
	for (FBlastMark& B : Blasts) B = FBlastMark();
	BlastNext = 0; DamageCues.Reset(); FrameDt = 0.f;
	Rng.Initialize(Seed);
}

float FTCCombatEffects::Ground(float X, float Z) const
{
	return Ctx_.HeightAt ? Ctx_.HeightAt(X, Z) : 0.f;
}

int32 FTCCombatEffects::Emit(ETCFxPreset Id, const FVector& P, const FVector& V, float Size, float Life)
{
	// fx.js emit: a particle from its preset (field by field as fx.js writes its record); cap(): Low 1600 (Client/TCGraphics.h)
	if (Live.Num() >= FTCGraphics::Get().Budget().ParticleCap) { Counters.Dropped++; return -1; }
	const FTCFxPreset& Pr = TCFxPresets::Get(Id);
	FTCFxParticle& X = Live.AddDefaulted_GetRef();
	X.Preset = Id; X.Look = Pr.Look;
	X.P = P; X.V = V; X.Age = 0.f; X.Life = FMath::Max(1e-4f, Life);
	X.Size0 = Size; X.Size1 = Size * Pr.Grow;
	X.C0 = Pr.C0; X.C1 = Pr.C1;
	X.Alpha = Pr.A; X.Add0 = Pr.Add; X.Add1 = Pr.Add1;
	X.Rot = Pr.bNoRot ? 0.f : Rng.FRand() * Tau;
	X.Spin = Pr.Rv ? (Rng.FRand() - 0.5f) * 2.f * Pr.Rv : 0.f;
	X.Drag = Pr.Drag; X.Grav = Pr.Grav; X.Pull = Pr.Pull; X.Len = Pr.Len; X.Fin = Pr.Fin; X.K = Pr.K; X.bFloor = Pr.bFloor; X.Wind = Pr.Wind; X.Group = 0;
	X.bLit = Pr.bLit;
	X.Cell = Pr.Flip || Pr.Cells < 2 ? Pr.Cell : Pr.Cell + FMath::Min(Pr.Cells - 1, (int32)(Rng.FRand() * Pr.Cells));
	X.Flip = Pr.Flip; X.Fps = Pr.Fps; X.Phase = Pr.Fps ? Rng.FRand() * Pr.Flip : 0.f;
	X.GroundZ = Ground(P.X, P.Y); X.bAnchor = Pr.bAnchor; X.bCurve = Pr.bCurve; X.bFixLen = Pr.bFixLen;
	X.Size = Size;
	return Live.Num() - 1;
}

int32 FTCCombatEffects::Shape(ETCFxLook Look, const FVector& P, const FVector& V, float Size, float Life)
{
	// a placeholder shape (support plane, canopy, falling section): not one of fx.js's sprites
	if (Live.Num() >= FTCGraphics::Get().Budget().ParticleCap) { Counters.Dropped++; return -1; }
	FTCFxParticle& X = Live.AddDefaulted_GetRef();
	X.Preset = ETCFxPreset::None; X.Look = Look;
	X.P = P; X.V = V; X.Size = Size; X.Size0 = X.Size1 = Size; X.Life = FMath::Max(1e-4f, Life);
	if (Look == ETCFxLook::Canopy) { X.Grav = 0.f; X.Drag = 0.f; }
	return Live.Num() - 1;
}

FTCFxParticle* FTCCombatEffects::Last() { return Live.Num() ? &Live.Last() : nullptr; }

// fx.js tint, hold, stretch: adjust a just-emitted particle
void FTCCombatEffects::Tint(int32 I, float K) { if (Live.IsValidIndex(I)) { Live[I].C0 *= K; Live[I].C1 *= K; } }
void FTCCombatEffects::Hold(int32 I, float D) { if (Live.IsValidIndex(I)) Live[I].Age = -D; }
void FTCCombatEffects::Stretch(int32 I, float L) { if (Live.IsValidIndex(I)) Live[I].Len = L; }

void FTCCombatEffects::Later(double Delay, TFunction<void()> Run)
{
	if (Events.Num() >= 768) return; // fx.js EVN
	Events.Add({ ClockNow + FMath::Max(0.0, Delay), MoveTemp(Run) });
}

void FTCCombatEffects::NoteShooter(int32 Id)
{
	if (ShooterEffects.Num() < 65536) ShooterEffects.Add(TPair<int32, int32>(TickNow, Id));
}

void FTCCombatEffects::Kick(double X, double Z, double Size)
{
	// fx.js kick: bigger blasts near the camera shake it a little, more when zoomed out
	// Graphics Low: no screen shake (fx.js kick: if (low() || reduceMotion || size < 3.2) return)
	if (!FTCGraphics::Get().Budget().bScreenShake || Ctx_.bReduceMotion || Size < TCFxRules::ShakeMin) return;
	const double A = Size * 0.035 * FMath::Max(0.0, 1.0 - FMath::Sqrt(FMath::Square(X - Ctx_.CameraFocus.X) + FMath::Square(Z - Ctx_.CameraFocus.Y)) / 70.0) * (Ctx_.CameraDistance / 60.0);
	if (A > ShakeAmount) ShakeAmount = (float)A;
}

FVector FTCCombatEffects::ShakeOffset(double C) const
{
	if (ShakeAmount <= 0.004f) return FVector::ZeroVector;
	const double S = ShakeAmount;
	// fx.js update: x, y (up) and z jitter; returned on Unreal axes (X = x, Y = z, Z = up), metres
	return FVector((FMath::Sin(C * 71.3) + FMath::Sin(C * 43.7)) * 0.5 * S, (FMath::Sin(C * 63.1) + FMath::Sin(C * 39.3)) * 0.5 * S, FMath::Sin(C * 57.9) * 0.5 * S);
}

// ---------- craters: fx.js scorch, fitDecal, updateDecals ----------

void FTCCombatEffects::FitDecal(FTCFxDecal& D) const
{
	const float C = FMath::Cos(D.Rot), S = FMath::Sin(D.Rot);
	constexpr int32 G = TCFxRules::DecalGrid;
	for (int32 J = 0; J < G; J++)
		for (int32 I = 0; I < G; I++)
		{
			const float U = ((float)I / (G - 1) - 0.5f) * 2.f * D.R, W = ((float)J / (G - 1) - 0.5f) * 2.f * D.R;
			const float X = D.X + U * C - W * S, Z = D.Z + U * S + W * C;
			D.Heights[J * G + I] = Ground(X, Z) + 0.05f;
		}
}

void FTCCombatEffects::Scorch(double X, double Z, double R, uint8 Kind)
{
	// fx.js scorch: lim = gfx.low ? 24 : DCAP, life gfx.low ? 60 : 150 (Client/TCGraphics.h)
	const FTCGraphicsBudget& Gfx = FTCGraphics::Get().Budget();
	int32 Slot = -1, Oldest = -1;
	for (int32 K = 0; K < FMath::Min(DecalPool.Num(), Gfx.CraterCap); K++)
	{
		if (!DecalPool[K].bOn) { Slot = K; break; }
		if (Oldest < 0 || DecalPool[K].Age > DecalPool[Oldest].Age) Oldest = K;
	}
	if (Slot < 0) Slot = Oldest;
	if (Slot < 0) return;
	FTCFxDecal& D = DecalPool[Slot];
	D.bOn = true; D.X = X; D.Z = Z; D.R = R; D.Rot = Rng.FRand() * Tau; D.Age = 0.f; D.Life = Gfx.CraterLife; D.Kind = Kind;
	FitDecal(D);
}

void FTCCombatEffects::RefitDecals()
{
	for (FTCFxDecal& D : DecalPool) if (D.bOn) FitDecal(D);
}

void FTCCombatEffects::UpdateDecals(float Dt)
{
	if ((RefitT -= Dt) <= 0.f) { RefitT = 2.f; RefitDecals(); } // the ground can be dug or bombed
	// fx.js updateDecals: on Low the craters past the first 24 go (gfx.low && s >= 24)
	const int32 Lim = FTCGraphics::Get().Budget().CraterCap;
	for (int32 K = 0; K < DecalPool.Num(); K++)
	{
		FTCFxDecal& D = DecalPool[K];
		if (!D.bOn) continue;
		D.Age += Dt;
		if (D.Age >= D.Life || K >= Lim) D.bOn = false;
	}
}

int32 FTCCombatEffects::DecalsOn(int32 Kind) const
{
	int32 N = 0;
	for (const FTCFxDecal& D : DecalPool) N += D.bOn && (Kind < 0 || D.Kind == Kind);
	return N;
}

// ---------- building blocks ----------

void FTCCombatEffects::Column(const FVector& P, double S, double Dur)
{
	// fx.js column: 24 slots, a busy one replaces the first
	if (Columns.Num() >= 24) Columns.RemoveAt(0);
	Columns.Add({ P, (float)S, 0.f, (float)Dur, 0.f });
}

void FTCCombatEffects::Explode(double X, double Z, double S, bool bDecal, bool bDirt, ETCFxPreset DebrisPreset, double SmokeK, const double* Y, double Fire)
{
	// fx.js explode: a brief flash, a fireball that cools into dark smoke, a plume of dirt and clods raining back, dust
	// rolling out along the ground, a dark smoke column that drifts downwind, and a crater
	Counters.Blasts++;
	const double Gy = Y ? *Y : Ground(X, Z), Rs = FMath::Sqrt(S);
	Emit(ETCFxPreset::Flash, At(X, Gy + 0.35 * S, Z), FVector::ZeroVector, 0.85 * S, 0.05 + 0.012 * S);
	const int32 Nf = S >= 4 ? 3 : S >= 2.5 ? 2 : 1;
	for (int32 I = 0; I < Nf; I++)
	{
		const double A = Rng.FRand() * Tau, R = I ? Rr(0.15, 0.32) * S : 0;
		Emit(ETCFxPreset::Fireball, At(X + FMath::Cos(A) * R, Gy + Rr(0.28, 0.4) * S, Z + FMath::Sin(A) * R), At(FMath::Cos(A) * R * 0.5, Rr(0.8, 1.5) * Rs, FMath::Sin(A) * R * 0.5),
			Rr(0.4, 0.52) * S * Fire * (I ? 0.8 : 1.0), (0.5 + 0.1 * S) * Rr(0.9, 1.15));
	}
	if (bDirt)
	{
		for (int32 I = 0, Np = S >= 3.6 ? 2 : 1; I < Np; I++)
			Emit(ETCFxPreset::Plume, At(X + Rr(-0.12, 0.12) * S, Gy - 0.1, Z + Rr(-0.12, 0.12) * S), FVector::ZeroVector, Rr(0.5, 0.62) * S, Rr(1.1, 1.5) + 0.12 * S);
		const int32 Nd = FMath::RoundToInt(5 + S * 3);
		for (int32 I = 0; I < Nd; I++)
		{
			const double A = Rng.FRand() * Tau, H = Rr(1.5, 4.5) * Rs;
			Emit(ETCFxPreset::Clod, At(X + FMath::Cos(A) * 0.2 * S, Gy + 0.3, Z + FMath::Sin(A) * 0.2 * S), At(FMath::Cos(A) * H, Rr(5, 10) * Rs, FMath::Sin(A) * H), Rr(0.05, 0.13) * FMath::Min(2.0, Rs), Rr(1.2, 2.2));
		}
		const int32 Nr = FMath::RoundToInt(4 + 1.2 * S);
		for (int32 I = 0; I < Nr; I++)
		{
			const double A = (double)I / Nr * Tau + Rng.FRand() * 0.4, Sp = Rr(2.5, 4.5) + 0.8 * S;
			Tint(Emit(ETCFxPreset::Dust, At(X + FMath::Cos(A) * 0.35 * S, Gy + 0.2 * Rs, Z + FMath::Sin(A) * 0.35 * S), At(FMath::Cos(A) * Sp, Rr(0.2, 0.6), FMath::Sin(A) * Sp), Rr(0.22, 0.32) * S, Rr(1.8, 2.8) + 0.15 * S), Rr(0.85, 1.1));
		}
	}
	if (DebrisPreset != ETCFxPreset::None && DebrisPreset != ETCFxPreset::Clod)
	{
		const int32 Nb = FMath::RoundToInt(3 + 2 * S);
		for (int32 I = 0; I < Nb; I++)
		{
			const double A = Rng.FRand() * Tau, H = Rr(1.5, 4) * Rs * 0.7;
			Emit(DebrisPreset, At(X, Gy + 0.4 * S, Z), At(FMath::Cos(A) * H, Rr(5, 9) * Rs * 0.6, FMath::Sin(A) * H), Rr(0.1, 0.22) * FMath::Min(2.0, Rs), Rr(1.3, 2.4));
		}
	}
	const int32 Ns = FMath::RoundToInt((2 + S * 1.3) * SmokeK);
	for (int32 I = 0; I < Ns; I++)
	{
		const double A = Rng.FRand() * Tau, R = Rng.FRand() * 0.35 * S;
		const int32 O = Emit(ETCFxPreset::Smoke, At(X + FMath::Cos(A) * R, Gy + Rr(0.25, 0.6) * S, Z + FMath::Sin(A) * R), At(FMath::Cos(A) * Rr(0.3, 1), Rr(1.2, 2.4) + 0.25 * S, FMath::Sin(A) * Rr(0.3, 1)), Rr(0.3, 0.42) * S, Rr(5, 8) + 1.2 * S);
		Hold(O, Rr(0.12, 0.4)); Tint(O, Rr(0.85, 1.15));
	}
	if (S >= 4 && SmokeK > 0) Column(At(X, Gy, Z), S, 1.2 + 0.25 * S);
	for (int32 I = 0, Nk = FMath::RoundToInt(1 + S * 0.6); I < Nk; I++)
	{
		const double A = Rng.FRand() * Tau, H = Rr(4, 9);
		Emit(ETCFxPreset::Spark, At(X, Gy + 0.4 * S, Z), At(FMath::Cos(A) * H, Rr(5, 12), FMath::Sin(A) * H), 0.05f, Rr(0.3, 0.6));
	}
	if (bDecal) Scorch(X, Z, FMath::Min(6.5, 0.7 * S + 0.4));
	Kick(X, Z, S);
	// Object.assign(blasts[blastI++ % blasts.length], { x, z, s, t: clock })
	FBlastMark& B = Blasts[BlastNext++ % UE_ARRAY_COUNT(Blasts)];
	B.X = X; B.Z = Z; B.S = S; B.T = ClockNow;
}

bool FTCCombatEffects::BlastNear(double X, double Z, double& OutX, double& OutZ, double& OutSize) const
{
	// fx.js blastNear: the death and the blast arrive in either order (a tank shell lands after its tracer flies), hence
	// the time window
	for (const FBlastMark& B : Blasts)
		if (ClockNow - B.T < 0.6 && FMath::Sqrt(FMath::Square(X - B.X) + FMath::Square(Z - B.Z)) < B.S + 0.5)
		{
			OutX = B.X; OutZ = B.Z; OutSize = B.S;
			return true;
		}
	return false;
}

void FTCCombatEffects::Gore(double X, double Z, double K)
{
	if (!FTCGraphics::Get().Gore()) return; // round 14 lead glue: fx.js gore returns at once without gfx.gore (the menu's Gore switch)
	// fx.js gore (gfx.gore, on by default): scraps, a red mist and a stain on the ground. Graphics High: 7 scraps.
	const double Gy = Ground(X, Z) + 0.6;
	const int32 N = FMath::RoundToInt(7 * K + 1);
	for (int32 I = 0; I < N; I++)
	{
		const double A = Rng.FRand() * Tau, H = Rr(1, 4);
		Emit(ETCFxPreset::Gib, At(X, Gy, Z), At(FMath::Cos(A) * H, Rr(3, 7), FMath::Sin(A) * H), Rr(0.06, 0.14), Rr(1.2, 2));
	}
	Emit(ETCFxPreset::Mist, At(X, Gy, Z), At(0, 0.6, 0), Rr(0.6, 0.9) * (0.6 + 0.4 * K), Rr(0.5, 0.8));
	Scorch(X, Z, Rr(0.8, 1.3) * (0.7 + 0.3 * K), 2); // stains share the crater decal pool, as fx.js
}

void FTCCombatEffects::VehicleDamage(const FVector& P, double Level)
{
	// fx.js vehicleDamage: a thin grey puff from the engine deck, darker and fuller as the damage grows
	Level = FMath::Clamp(Level, 0.0, 2.0);
	if (Level <= 0.0) return;
	const int32 O = Emit(ETCFxPreset::Smoke, P, At(Rr(-0.12, 0.12), 0.3 + Level * 0.12, Rr(-0.12, 0.12)), 0.23 + Level * 0.1, 1.5 + Level * 0.4);
	if (O >= 0) { Live[O].Alpha *= (float)FMath::Min(0.55, Level * 0.28); Tint(O, (float)(0.8 - Level * 0.06)); DamagePuffCount++; }
}

void FTCCombatEffects::Plume(const FString& Kind, double X, double Y, double Z, double Radius)
{
	// fx.js plume (High graphics: this client has no Low): smoke or wreck smoke rising from the roof, or three flames
	const double Spread = Radius * 0.65;
	if (Kind != TEXT("fire"))
	{
		const bool bDark = Kind == TEXT("dark");
		for (int32 I = 0; I < 2; I++)
			Emit(bDark ? ETCFxPreset::WreckSmoke : ETCFxPreset::Smoke, At(X + Rr(-0.4, 0.4) * Spread, Y + Rr(0, 0.6), Z + Rr(-0.4, 0.4) * Spread),
				At(Rr(-0.4, 0.4), Rr(2.1, 2.8), Rr(-0.4, 0.4)), Radius * 0.45, bDark ? 4.0 : 4.5);
		return;
	}
	for (int32 I = 0; I < 3; I++)
		Emit(ETCFxPreset::Flame, At(X + Rr(-Spread, Spread), Y - 0.2, Z + Rr(-Spread, Spread)), At(0, Rr(0.1, 0.3), 0), Radius * 0.36, Rr(0.8, 1.1));
}

void FTCCombatEffects::VehicleDamageFrame(int32 Id, double Health, bool bKilled, bool bDetailed, float Dt, TFunctionRef<bool(FVector& OutAt)> Anchor)
{
	// engine-presentation.js vehicleDamageFrame: a damage stage with hysteresis (light smoke under 65 % health, heavy
	// under 32 %, back to light above 38 % and none above 71 %), a level eased toward it, and a puff every 0.55 s
	// (0.28 s past level 1.2) while the vehicle is in view; no emission debt is kept while it is hidden
	FDamageCue& Cue = DamageCues.FindOrAdd(Id);
	if (!FMath::IsFinite(Health) || Health <= 0.0 || bKilled) { Cue = FDamageCue(); return; }
	if (Cue.Stage == 2 && Health > 0.38) Cue.Stage = Health > 0.71 ? 0 : 1;
	if (Cue.Stage == 1 && Health > 0.71) Cue.Stage = 0;
	if (Health <= 0.32) Cue.Stage = 2;
	else if (Health <= 0.65 && Cue.Stage == 0) Cue.Stage = 1;
	const double Step = FMath::Min(0.1, FMath::Max(0.0, (double)Dt));
	Cue.Level += (Cue.Stage - Cue.Level) * (1.0 - FMath::Exp(-Step * (Cue.Stage > Cue.Level ? 3.0 : 1.5)));
	if (Cue.Level < 0.03 && !Cue.Stage) Cue.Level = 0;
	if (!bDetailed || !Cue.Level) { Cue.Wait = 0; return; }
	Cue.Wait -= Step;
	if (Cue.Wait <= 0)
	{
		FVector P;
		if (Anchor(P)) VehicleDamage(P, Cue.Level);
		Cue.Wait = Cue.Level > 1.2 ? 0.28 : 0.55;
	}
}

void FTCCombatEffects::RetainDamage(const TSet<int32>& Present)
{
	for (auto It = DamageCues.CreateIterator(); It; ++It) if (!Present.Contains(It.Key())) It.RemoveCurrent();
}

void FTCCombatEffects::Muzzle(const FVector& P, double Dx, double Dz, double S, const FStyle& St)
{
	// fx.js muzzle: a short flame out of the barrel seen from the side, a little glow at the muzzle; heavy guns blow a
	// ring of dust off the ground and leave a cloud of gun smoke; a rocket launcher's back-blast. P on Unreal axes
	// (metres), Dx, Dz the horizontal heading in map x and z.
	const FVector D(Dx, Dz, 0.0);
	const double L = S * (S > 1 ? 1.6 : 1.2);
	Stretch(Emit(ETCFxPreset::Muzzle, P + D * L, D * 0.5, S * (S > 1 ? 0.75 : 0.5), 0.045 + 0.03 * S), L);
	Emit(ETCFxPreset::Flash, P + D * 0.3 * S, FVector::ZeroVector, S * 0.6, 0.035 + 0.02 * S);
	if (St.Smoke > 0)
		for (int32 I = 0; I < 2 + 3 * St.Smoke; I++)
			Emit(ETCFxPreset::Gunsmoke, P + FVector(Dx * Rr(0.3, 1.4) * S, Dz * Rr(0.3, 1.4) * S, Rr(-0.1, 0.2)), FVector(Dx * Rr(1.5, 4) + Rr(-0.6, 0.6), Dz * Rr(1.5, 4) + Rr(-0.6, 0.6), Rr(0.2, 0.6)), 0.4 * S, Rr(1.6, 2.6));
	else if (Rng.FRand() < 0.35f) Emit(ETCFxPreset::Gunsmoke, P + D * 0.3, FVector(Dx * 0.8, Dz * 0.8, 0.3), 0.22 + 0.2 * S, Rr(0.9, 1.4));
	if (St.Blast > 0)
	{
		// the blast ring: dust pushed out radially from under the muzzle
		const FVector B = P + D * 1.6;
		const double Gy = Ground(B.X, B.Y);
		for (int32 I = 0; I < 9; I++)
		{
			const double A = I / 9.0 * Tau + Rng.FRand() * 0.3, Sp = Rr(4, 6.5) * St.Blast;
			Emit(ETCFxPreset::Dust, FVector(B.X + FMath::Cos(A) * 0.6, B.Y + FMath::Sin(A) * 0.6, Gy + 0.3), FVector(FMath::Cos(A) * Sp + Dx * 2, FMath::Sin(A) * Sp + Dz * 2, Rr(0.2, 0.6)), 0.55 * St.Blast, Rr(1.2, 1.9));
		}
	}
	if (St.bBack)
	{
		// a rocket launcher's back-blast: flame and a cone of smoke and dust out behind the tube
		Stretch(Emit(ETCFxPreset::Muzzle, P - D * 1.6, -D * 0.5, 0.45f, 0.06f), 1.4f);
		for (int32 I = 0; I < 5; I++)
			Emit(ETCFxPreset::Gunsmoke, P + FVector(-Dx * Rr(0.6, 2), -Dz * Rr(0.6, 2), Rr(-0.2, 0.3)), FVector(-Dx * Rr(3, 6) + Rr(-1, 1), -Dz * Rr(3, 6) + Rr(-1, 1), Rr(0.2, 0.8)), 0.55f, Rr(1.4, 2.2));
		const double Gy = Ground(P.X - Dx * 2, P.Y - Dz * 2);
		for (int32 I = 0; I < 4; I++)
			Emit(ETCFxPreset::Dust, FVector(P.X - Dx * 2, P.Y - Dz * 2, Gy + 0.3), FVector(-Dx * Rr(3, 5) + Rr(-1.5, 1.5), -Dz * Rr(3, 5) + Rr(-1.5, 1.5), 0.4), 0.5f, Rr(1, 1.6));
	}
}

void FTCCombatEffects::ShotTracer(const FVector& From, const FVector& To, const FStyle& St, int32 Flags)
{
	// fx.js shotTracer: the flash at the muzzle, a tracer to the end point (MGs every round, rifles now and then,
	// faintly), the impact on arrival
	const FVector D = To - From;
	const double Len = FMath::Max(1e-6, D.Size()), Hd = FMath::Max(1e-6, FVector2D(D.X, D.Y).Size()), T = Len / St.Spd;
	Muzzle(From, D.X / Hd, D.Y / Hd, St.Flash, St);
	if (St.bTracer && Rng.FRand() < St.TrOdds) Emit(St.Tr, From, D / T, St.W, T);
	const FStyle* Sp = &St;
	Later(T, [this, To, Sp, Flags]() { Impact(To, *Sp, Flags); });
}

void FTCCombatEffects::SkyShot(const FVector& From, const FVector& To, const FStyle& St)
{
	// fx.js skyShot: a round fired into the sky, flash and streak only
	const FVector D = To - From;
	const double Len = FMath::Max(1e-6, D.Size()), Hd = FMath::Max(1e-6, FVector2D(D.X, D.Y).Size()), T = Len / St.Spd;
	Muzzle(From, D.X / Hd, D.Y / Hd, St.Flash, FStyle());
	if (St.bTracer) Emit(St.Tr, From, D / T, St.W, T);
}

void FTCCombatEffects::Burst(const FVector& P, double S)
{
	// fx.js burst: a flak shell bursting in the air: a sharp flash, a few sparks and a black puff that swells and drifts
	Emit(ETCFxPreset::Flash, P, FVector::ZeroVector, 0.9 * S, 0.06f);
	for (int32 I = 0; I < 4; I++) Emit(ETCFxPreset::Flak, P + FVector(Rr(-0.4, 0.4) * S, Rr(-0.4, 0.4) * S, Rr(-0.3, 0.3) * S), FVector(Rr(-0.8, 0.8), Rr(-0.8, 0.8), Rr(-0.2, 0.4)), Rr(0.6, 0.9) * S, Rr(2.5, 4));
	for (int32 I = 0; I < 3; I++) { const double A = Rng.FRand() * Tau, H = Rr(3, 7); Emit(ETCFxPreset::Spark, P, FVector(FMath::Cos(A) * H, FMath::Sin(A) * H, Rr(-2, 4)), 0.05f, Rr(0.25, 0.5)); }
}

void FTCCombatEffects::Impact(const FVector& P, const FStyle& St, int32 Flags)
{
	// fx.js impact; flags: 1 hit, 2 vehicle target, 4 structure target
	const bool bHit = Flags & 1, bVeh = Flags & 2, bWall = Flags & 4;
	const double Gy = Ground(P.X, P.Y);
	if (St.Heavy > 0)
	{
		if (bVeh && bHit && !bWall)
		{
			// a shell striking armour: a hard flash, sparks, a puff of black smoke and a few torn bits of metal
			Emit(ETCFxPreset::Flash, P, FVector::ZeroVector, 0.7 * St.Heavy, 0.07f);
			Emit(ETCFxPreset::Fireball, P, FVector(0, 0, 0.8), 0.3 * St.Heavy, 0.45f);
			for (int32 I = 0; I < 8; I++) { const double A = Rng.FRand() * Tau, H = Rr(3, 8); Emit(ETCFxPreset::Spark, P, FVector(FMath::Cos(A) * H, FMath::Sin(A) * H, Rr(2, 8)), 0.06f, Rr(0.3, 0.7)); }
			for (int32 I = 0; I < 3; I++) Emit(ETCFxPreset::WreckSmoke, P + FVector(Rr(-0.4, 0.4), Rr(-0.4, 0.4), Rr(0, 0.5)), FVector(Rr(-0.5, 0.5), Rr(-0.5, 0.5), Rr(1, 2)), 0.35 * St.Heavy, Rr(2.5, 4));
			for (int32 I = 0; I < 3; I++) { const double A = Rng.FRand() * Tau, H = Rr(2, 5); Emit(ETCFxPreset::Metal, P, FVector(FMath::Cos(A) * H, FMath::Sin(A) * H, Rr(3, 6)), Rr(0.06, 0.12), Rr(1, 1.8)); }
			Sound(TEXT("blast_small"));
		}
		else
		{
			Explode(P.X, P.Y, St.Heavy * (bHit ? 1.1 : 1.0), true, true, bWall ? ETCFxPreset::Masonry : ETCFxPreset::Clod, 0.6);
			Sound(TEXT("blast_small"));
		}
		return;
	}
	if (bVeh && bHit && !bWall)
	{
		for (int32 I = 0; I < 3; I++) { const double A = Rng.FRand() * Tau, H = Rr(3, 7); Emit(ETCFxPreset::Spark, P, FVector(FMath::Cos(A) * H, FMath::Sin(A) * H, Rr(1, 5)), 0.04f, Rr(0.15, 0.35)); }
		Sound(TEXT("ricochet"));
	}
	else if (bWall && bHit)
	{
		// chips off a wall: a few bits of brick and stone and a puff of pale dust
		for (int32 I = 0; I < 3; I++) { const double A = Rng.FRand() * Tau, H = Rr(1.5, 3.5); Emit(ETCFxPreset::Masonry, P, FVector(FMath::Cos(A) * H, FMath::Sin(A) * H, Rr(1, 4)), Rr(0.04, 0.08), Rr(0.8, 1.4)); }
		Tint(Emit(ETCFxPreset::Dust, P, FVector(Rr(-0.4, 0.4), Rr(-0.4, 0.4), Rr(0.2, 0.6)), Rr(0.25, 0.4), Rr(0.9, 1.5)), 1.25f);
	}
	else
	{
		// a bullet kicking up the dirt: a little spurt of dust and grit standing on the ground
		Emit(ETCFxPreset::Kick, FVector(P.X + Rr(-0.3, 0.3), P.Y + Rr(-0.3, 0.3), Gy - 0.05), FVector::ZeroVector, Rr(0.32, 0.48), Rr(0.55, 0.85));
		Emit(ETCFxPreset::Clod, FVector(P.X, P.Y, Gy + 0.1), FVector(Rr(-1, 1), Rr(-1, 1), Rr(2.5, 4.5)), Rr(0.03, 0.05), Rr(0.5, 0.8));
	}
}

void FTCCombatEffects::StrafeHit(double X, double Z)
{
	// fx.js strafeHit (fx.air 'strafe'): heavy rounds throwing up spurts of dirt along the ground
	const double Gy = Ground(X, Z);
	for (int32 I = 0; I < 4; I++) Emit(ETCFxPreset::Kick, At(X + Rr(-1.2, 1.2), Gy - 0.05, Z + Rr(-1.2, 1.2)), FVector::ZeroVector, Rr(0.5, 0.8), Rr(0.7, 1.1));
	for (int32 I = 0; I < 3; I++) Emit(ETCFxPreset::Clod, At(X, Gy + 0.1, Z), At(Rr(-2, 2), Rr(3, 6), Rr(-2, 2)), Rr(0.04, 0.08), Rr(0.6, 1));
}

void FTCCombatEffects::ProjectileTrail(const FTCFlightDraw& D)
{
	// fx.js projectileTrail: a short streak at the server flight's current solution; it expires between frames rather
	// than flying onward (life 0.035 s), and its direction uses the velocity without integrating its place (v x 0.001)
	const FStyle& St = StyleOf(D.Kind);
	const double Speed = FMath::Max(1e-9, D.V.Size());
	const bool bBoom = D.Effect == TEXT("boom");
	const ETCFxPreset Pr = bBoom ? ETCFxPreset::Nade : D.Kind == TEXT("mortar") || D.Kind == TEXT("howitzer") || D.Kind == TEXT("rocket") ? ETCFxPreset::Rocket : St.bTracer ? St.Tr : ETCFxPreset::TracerFaint;
	// Its life is 0.035 s, two frames at the browser's 60 fps. A frame slower than that (a long screenshot frame, a
	// hitch) would age it past its life in the same frame's simulation and never draw it, so it lives at least one and
	// a half of this frame's steps: drawn once, gone the next frame, as at 60 fps.
	const int32 O = Emit(Pr, At(D.P.X, D.P.Y, D.P.Z), FVector::ZeroVector, bBoom ? 0.12f : St.W, FMath::Max(0.035f, 1.5f * FrameDt));
	if (O < 0) return;
	FTCFxParticle& X = Live[O];
	X.Group = -D.Flight; X.Grav = 0.f; X.Len = bBoom ? 0.f : FMath::Min(1.8, Speed * 0.006);
	X.V = At(D.V.X * 0.001, D.V.Y * 0.001, D.V.Z * 0.001);
}

void FTCCombatEffects::ProjectileRemove(int32 Flight)
{
	// fx.js projectileRemove: its streaks end now
	for (FTCFxParticle& X : Live) if (X.Group == -Flight && X.Preset != ETCFxPreset::None) X.Age = X.Life;
}

void FTCCombatEffects::Collapse(double X, double Z, double R)
{
	// fx.js collapse: a house or bunker coming down: rolling pale dust, chunks of brick, stone and timber
	Counters.Collapses++;
	const double Gy = Ground(X, Z);
	for (int32 I = 0; I < 16; I++)
	{
		const double A = Rng.FRand() * Tau, D = Rng.FRand() * R;
		Emit(ETCFxPreset::Collapse, At(X + FMath::Cos(A) * D, Gy + Rr(1, 3.5), Z + FMath::Sin(A) * D), At(FMath::Cos(A) * Rr(1, 3), Rr(0.4, 1.2), FMath::Sin(A) * Rr(1, 3)), Rr(1.6, 2.6), Rr(5, 8));
	}
	for (int32 I = 0; I < 12; I++)
	{
		const double A = I / 12.0 * Tau, Sp = Rr(4, 7);
		Emit(ETCFxPreset::Collapse, At(X + FMath::Cos(A) * R * 0.7, Gy + 0.6, Z + FMath::Sin(A) * R * 0.7), At(FMath::Cos(A) * Sp, 0.3, FMath::Sin(A) * Sp), Rr(1, 1.6), Rr(3, 5));
	}
	for (int32 I = 0; I < 18; I++)
	{
		const double A = Rng.FRand() * Tau, H = Rr(1.5, 5);
		Emit(I % 5 == 4 ? ETCFxPreset::Wood : ETCFxPreset::Masonry, At(X + Rr(-R, R) * 0.5, Gy + Rr(2, 5), Z + Rr(-R, R) * 0.5), At(FMath::Cos(A) * H, Rr(3, 8), FMath::Sin(A) * H), Rr(0.15, 0.32), Rr(1.5, 2.6));
	}
	Sound(TEXT("collapse"));
}

void FTCCombatEffects::Burning(const FVector& P, double Big, double Fire, double Dur)
{
	if (WreckFires.Num() >= 64) WreckFires.RemoveAt(0);
	WreckFires.Add({ P, 0.f, (float)Fire, (float)Dur, 0.f, 0.f, 0.f, (float)Big });
}

void FTCCombatEffects::Wreck(const FTCUnit& U)
{
	// fx.js wreck (from main.js removeUnit for a killed vehicle or structure): a structure collapses and burns; a
	// vehicle blows up and burns, then smokes
	Counters.Wrecks++;
	const FTCUnitDef* Def = Ctx_.Table ? Ctx_.Table->Find(U.Type) : nullptr;
	const double Gy = Ground(U.X, U.Z);
	if (Def && Def->bStructure)
	{
		Collapse(U.X, U.Z, 4.0); // UNITS[type].size is not in this client's table: the browser's fallback radius
		Burning(At(U.X, Gy + 1, U.Z), 1.4, 8, 30);
		return;
	}
	const double Big = U.Type == TEXT("tiger") || U.Type == TEXT("churchill") ? 1.3 : U.Type == TEXT("medium") ? 1.15 : 1.0;
	Explode(U.X, U.Z, 3.2 * Big, true, true, ETCFxPreset::Metal, 0.7);
	Burning(At(U.X, Gy + 1.3 * Big, U.Z), Big, 20, 38);
	Sound(TEXT("vehicle_destroyed"));
}

void FTCCombatEffects::WorldContact(const FTCShot& Sh, const FTCShotExtra& Ex)
{
	// fx.js worldContact: chips of the struck material thrown along the hit, a collapse's dust
	const bool bCollapsing = Sh.K == TEXT("collapse");
	const FString Material = Sh.Material.IsEmpty() ? FString(TEXT("soil")) : Sh.Material;
	const double Impulse = FMath::Clamp(Ex.bHasImpulse ? Ex.Impulse : 1.0, 0.25, 4.0);
	const ETCFxPreset Pr = Material == TEXT("wood") ? ETCFxPreset::Wood : Material == TEXT("steel") ? ETCFxPreset::Metal : Material == TEXT("soil") || Material == TEXT("road") ? ETCFxPreset::Clod : ETCFxPreset::Masonry;
	const double Dir = Ex.bHasDir ? Ex.Dir : FMath::Atan2(Ex.bHasDirection ? Ex.Direction.Z : 0.0, Ex.bHasDirection ? Ex.Direction.X : 1.0);
	const double Gy = Sh.bHasY ? Sh.Y : Ground(Sh.X, Sh.Z) + 0.6;
	const double Speed = FMath::Min(3.2, Impulse * 800.0 / TCFxImpl::SectionMass(Material));
	const int32 Count = bCollapsing ? 9 : 3;
	for (int32 I = 0; I < Count; I++)
	{
		const double A = Dir + Rr(-0.65, 0.65), Sp = Speed * Rr(0.65, 1);
		Emit(Pr, At(Sh.X + Rr(-0.35, 0.35), Gy, Sh.Z + Rr(-0.35, 0.35)), At(FMath::Cos(A) * Sp, Rr(0.3, 0.7) * Impulse, FMath::Sin(A) * Sp), bCollapsing ? Rr(0.15, 0.28) : Rr(0.06, 0.11), bCollapsing ? 1.5f : 0.45f);
	}
	if (bCollapsing)
	{
		for (int32 I = 0; I < 4; I++) Emit(ETCFxPreset::Collapse, At(Sh.X + Rr(-0.7, 0.7), Gy, Sh.Z + Rr(-0.7, 0.7)), At(FMath::Cos(Dir) * Speed * 0.3, 0.35, FMath::Sin(Dir) * Speed * 0.3), Rr(0.7, 1.1), Rr(1.5, 2.2));
		Sound(TEXT("collapse"));
	}
}

void FTCCombatEffects::LaunchRocket(const FVector& From, const FVector& To, double T, double H)
{
	// fx.js launchRocket: a rocket or shell on an arc (Unreal axes), its flash and smoke; updateRockets lays its trail
	if (Rockets.Num() >= 48) return;
	const double G = H > 0 ? 8 * H / (T * T) : 0;
	const FVector V((To.X - From.X) / T, (To.Y - From.Y) / T, (To.Z - From.Z + 0.5 * G * T * T) / T);
	Rockets.Add({ From, V, (float)G, 0.f, (float)T, 0.f });
	const int32 O = Emit(ETCFxPreset::Rocket, From, V, 0.22f, T * 0.97);
	if (O >= 0) Live[O].Grav = G;
	Emit(ETCFxPreset::Flash, From, FVector::ZeroVector, 0.8f, 0.07f);
	const double Sp = FMath::Max(1e-6, V.Size());
	for (int32 I = 0; I < 3; I++) Emit(ETCFxPreset::Gunsmoke, From, FVector(-V.X / Sp * Rr(2, 4) + Rr(-0.8, 0.8), -V.Y / Sp * Rr(2, 4) + Rr(-0.8, 0.8), Rr(0.3, 0.9)), 0.5f, Rr(1.4, 2.2));
}

bool FTCCombatEffects::MuzzleOf(const FTCUnit& U, int32 K, double Tx, double Tz, FVector& Out)
{
	// Placeholder muzzles (the browser reads barrel tips and soldiers' hands off its models): where this client draws
	// the unit, toward the target. Only ever called for a held, drawn shooter.
	if (!ATCUnitRenderer::IsDrawn(U)) return false;
	FVector2D P(U.X, U.Z);
	if (Ctx_.DrawnAt) Ctx_.DrawnAt(U.Id, P);
	const FTCUnitDef* Def = Ctx_.Table ? Ctx_.Table->Find(U.Type) : nullptr;
	const double Gy = Ground(P.X, P.Y);
	FVector2D D(Tx - P.X, Tz - P.Y);
	const double L = D.Size();
	D = L > 1e-6 ? D / L : FVector2D(FMath::Cos(U.Rot), FMath::Sin(U.Rot));
	if (U.Type == TEXT("bunker") || U.Type == TEXT("pillbox"))
	{
		const double R = U.Type == TEXT("bunker") ? 2.8 : 1.8;
		Out = FVector(P.X + D.X * R, P.Y + D.Y * R, Gy + (U.Type == TEXT("bunker") ? 1.6 : 1.1));
		return true;
	}
	if (Def && Def->bAir)
	{
		const double Side = (K % 2) ? 1.1 : -1.1;
		Out = FVector(P.X + D.X * 2.4 - D.Y * Side, P.Y + D.Y * 2.4 + D.X * Side, Gy + TCFxRules::AirAltitude);
		return true;
	}
	if (Def && !Def->bInfantry)
	{
		const double R = Def->Radius * 0.9;
		Out = FVector(P.X + D.X * R, P.Y + D.Y * R, Gy + 1.6);
		return true;
	}
	// a squad: man K of the ranks ATCUnitRenderer draws (three across the facing)
	const double Across = ((K % 3) - 1) * 1.3, Behind = (K / 3) * 1.3;
	const double C = FMath::Cos(U.Rot), S = FMath::Sin(U.Rot);
	Out = FVector(P.X - Behind * C - Across * S + D.X * 0.7, P.Y - Behind * S + Across * C + D.Y * 0.7, Gy + 1.1);
	return true;
}

void FTCCombatEffects::DirectFire(const FTCShot& Sh, const FTCUnit* From, const FTCUnit* To)
{
	// fx.js directFire. From: the shooter when held this snapshot, else null (only the arrival shows). To: the target
	// when held.
	Counters.DirectFire++;
	const FTCUnitDef* TDef = To && Ctx_.Table ? Ctx_.Table->Find(To->Type) : nullptr;
	const bool bVeh = TDef ? !TDef->bInfantry : false;
	const bool bBazooka = Sh.K == TEXT("ranger") && bVeh;
	const FStyle& St = bBazooka ? Bazooka() : StyleOf(Sh.K);
	const int32 Flags = (Sh.bHit ? 1 : 0) | (bVeh ? 2 : 0) | (TDef && TDef->bStructure ? 4 : 0);
	const double Ty = bVeh ? (TDef->bStructure ? 1.6 : 1.2) : 0.8;
	Sound(St.Snd);
	if (!From)
	{
		Counters.DirectFireHidden++;
		const double S = Sh.bHit ? 0.6 : 2.5;
		const double Ex = Sh.X + Rr(-1, 1) * S, Ez = Sh.Z + Rr(-1, 1) * S;
		Impact(At(Ex, Ground(Ex, Ez) + Ty, Ez), St, Flags);
		return;
	}
	NoteShooter(From->Id);
	const FTCUnitDef* FDef = Ctx_.Table ? Ctx_.Table->Find(From->Type) : nullptr;
	const int32 Alive = FDef && FDef->bInfantry && FDef->HpPer > 0 ? FMath::Clamp(FMath::DivideAndRoundUp(From->Hp, FDef->HpPer), 1, FDef->Models) : 1;
	const double Spread = Sh.bHit ? (bVeh ? 0.6 : 1.4) : 3.0;
	const int32 Men = St.N > 1 ? FMath::Min(St.N, St.Guns ? St.Guns : Alive) : 1;
	for (int32 I = 0; I < Men; I++)
	{
		FVector M;
		if (!MuzzleOf(*From, I, Sh.X, Sh.Z, M)) break;
		const int32 Shots = bBazooka ? 1 : St.Burst;
		const double T0 = St.N > 1 ? Rng.FRand() * 0.18 : 0.0;
		for (int32 B = 0; B < Shots; B++)
		{
			double Ex = Sh.X + Rr(-0.5, 0.5) * Spread, Ez = Sh.Z + Rr(-0.5, 0.5) * Spread;
			if (Sh.bHit && bVeh)
			{
				const double Ddx = Ex - M.X, Ddz = Ez - M.Y, Dd = FMath::Max(1e-6, FMath::Sqrt(Ddx * Ddx + Ddz * Ddz)), R = FMath::Min(Dd * 0.5, TDef->Radius * 0.9);
				Ex -= Ddx / Dd * R; Ez -= Ddz / Dd * R;
			}
			const FVector End = At(Ex, Ground(Ex, Ez) + (Sh.bHit ? Ty : 0.1), Ez);
			const FStyle* Sp = &St;
			const int32 F = Flags & (B == 0 ? 7 : 6);
			Later(T0 + B * St.Gap, [this, M, End, Sp, F]() { ShotTracer(M, End, *Sp, F); });
		}
	}
}

void FTCCombatEffects::Salvo(const FTCShot& Sh, const FTCShotExtra& Ex, const FTCUnit* From)
{
	// fx.js salvo: mortar bombs arcing over (a launcher seen: they leave it; else they whistle in), or a rocket ripple
	Counters.Salvos++;
	const bool bShip = From && From->Type == TEXT("destroyer");
	const bool bMortar = From ? (From->Type == TEXT("mortar") || From->Type == TEXT("howitzer") || bShip) : (Ex.bHasN && Ex.N <= 4);
	const int32 N = FMath::Clamp(Ex.bHasN ? Ex.N : 8, 0, 64);
	const TCFxImpl::FSalvoW* W = From && TCFxImpl::SalvoOf(From->Type) ? TCFxImpl::SalvoOf(From->Type) : TCFxImpl::SalvoOf(bMortar ? TEXT("mortar") : TEXT("rocket"));
	const FTCUnitDef* FDef = From && Ctx_.Table ? Ctx_.Table->Find(From->Type) : nullptr;
	if (FDef && FDef->bAir) From = nullptr; // a bomber's stick falls from the sky like an unseen launcher's rockets
	if (SalvoMarks.Num() >= 8) SalvoMarks.RemoveAt(0);
	SalvoMarks.Add({ Sh.X, Sh.Z, Ex.bHasR ? Ex.R : W->Spread, bMortar, ClockNow });
	if (From) NoteShooter(From->Id);
	FVector Top;
	if (From) { FVector2D P(From->X, From->Z); if (Ctx_.DrawnAt) Ctx_.DrawnAt(From->Id, P); Top = FVector(P.X, P.Y, Ground(P.X, P.Y) + 1.12); }
	if (bMortar)
	{
		const int32 Mounts = 2; // a destroyer's gun mounts (unit-models.js), as a placeholder count
		for (int32 I = 0; I < N; I++)
		{
			const double D = bShip ? FMath::FloorToDouble(I / (double)Mounts) * 1.2 : I * W->Every;
			if (From)
			{
				const FVector Up = Top + FVector((Sh.X - From->X) * 0.15, (Sh.Z - From->Z) * 0.15, 40.0);
				if (bShip && I < Mounts) { const FVector2D Dd(Sh.X - Top.X, Sh.Z - Top.Y); const double L = FMath::Max(1e-6, Dd.Size()); FStyle Sm; Sm.Smoke = 1; Muzzle(Top, Dd.X / L, Dd.Y / L, 2.4, Sm); }
				Later(D, [this, Top, Up]() { LaunchRocket(Top, Up, 0.6, 0); });
				Sound(bShip ? TEXT("tankgun") : TEXT("mortar"));
			}
			Sound(TEXT("whistle"));
		}
		return;
	}
	if (From)
	{
		const double H = 10 + FMath::Sqrt(FMath::Square(Sh.X - From->X) + FMath::Square(Sh.Z - From->Z)) * 0.12;
		for (int32 I = 0; I < N; I++)
		{
			const double A = Rng.FRand() * Tau, R = FMath::Sqrt(Rng.FRand()) * W->Spread, Ex2 = Sh.X + FMath::Cos(A) * R, Ez2 = Sh.Z + FMath::Sin(A) * R;
			const FVector From3 = Top + FVector(Rr(-0.4, 0.4), Rr(-0.4, 0.4), 1.0), To3 = At(Ex2, Ground(Ex2, Ez2), Ez2);
			const double Fl = W->Flight;
			Later(I * W->Every, [this, From3, To3, Fl, H]() { LaunchRocket(From3, To3, Fl, H); });
		}
		Sound(TEXT("rockets"));
		return;
	}
	// launcher out of sight: rockets drop in from the sky, from no telltale direction
	for (int32 I = 0; I < N; I++)
	{
		const double A = Rng.FRand() * Tau, R = FMath::Sqrt(Rng.FRand()) * W->Spread, Ex2 = Sh.X + FMath::Cos(A) * R, Ez2 = Sh.Z + FMath::Sin(A) * R, G = Ground(Ex2, Ez2);
		const FVector From3 = At(Ex2 + Rr(-3, 3), G + 30, Ez2 + Rr(-3, 3)), To3 = At(Ex2, G, Ez2);
		Later(W->Flight + I * W->Every - 0.4, [this, From3, To3]() { LaunchRocket(From3, To3, 0.4, 0); });
	}
	Sound(TEXT("whistle"));
}

void FTCCombatEffects::AntiAir(const FTCShot& Sh, const FTCUnit* From, const FTCUnit* To, bool bBursts)
{
	// fx.js antiAir: tracers into the sky from a seen gun, flak bursts around the plane
	double Ex = Sh.X, Ez = Sh.Z, Ey = Ground(Sh.X, Sh.Z) + TCFxRules::AirAltitude;
	if (To)
	{
		FVector2D P(To->X, To->Z);
		if (Ctx_.DrawnAt) Ctx_.DrawnAt(To->Id, P);
		const FTCUnitDef* TD = Ctx_.Table ? Ctx_.Table->Find(To->Type) : nullptr;
		Ex = P.X; Ez = P.Y; Ey = Ground(P.X, P.Y) + (TD && TD->bAir ? TCFxRules::AirAltitude : 1.0);
	}
	const FString T = From ? From->Type : FString();
	const bool bFlak = TCFxImpl::IsFlakType(T);
	const FStyle& St = T == TEXT("fighter") ? StyleOf(TEXT("fighter")) : T == TEXT("flaktrack") ? StyleOf(TEXT("flaktrack")) : bFlak ? StyleOf(TEXT("flak")) : StyleOf(TEXT("mg"));
	double Tt = 0.15;
	if (From)
	{
		NoteShooter(From->Id);
		for (int32 I = 0; I < (St.Guns ? St.Guns : 1); I++)
		{
			FVector M;
			if (!MuzzleOf(*From, I, Ex, Ez, M)) break;
			Tt = FVector::Dist(FVector(Ex, Ez, Ey), M) / St.Spd;
			const FStyle* Sp = &St;
			for (int32 B = 0; B < St.Burst; B++)
			{
				const FVector End(Ex + Rr(-2.5, 2.5), Ez + Rr(-2.5, 2.5), Ey + Rr(-1.5, 1.5));
				Later(B * St.Gap, [this, M, End, Sp]() { SkyShot(M, End, *Sp); });
			}
		}
	}
	if (bFlak && bBursts) { const FVector B(Ex + Rr(-4, 4), Ez + Rr(-4, 4), Ey + Rr(-2, 3)); Later(Tt, [this, B]() { Burst(B, 0.7); }); }
	Sound(St.Snd);
}


void FTCCombatEffects::AirShot(const FTCShot& Sh, const FTCShotExtra& Ex, const FTCUnit* From, const FTCUnit* To)
{
	// client/main.js applySnapshot's air shots, drawn by client/aircraft.js in the browser (its planes and canopies are
	// models there: placeholder shapes here; its effects are fx.js's: fx.air flak, strafe, smoke, flame, crash)
	Counters.AirShots++;
	if (TCFxImpl::IsSupportPlane(Sh.K))
	{
		// aircraft.js supportPlane: the plane crosses the map over the spot along its heading; a strafing run walks hits
		// along the ground; a bombing run's bombs come as their own 'bomb' shots
		FPass P;
		P.At = FVector2D(Sh.X, Sh.Z); P.Dir = Ex.bHasDir ? Ex.Dir : 0.0; P.Kind = Sh.K; P.T = 0.f; P.Owner = Sh.bHasShooterOwner ? Sh.Fo : -1;
		if (Sh.K == TEXT("strafe")) for (int32 I = 0; I < 10; I++) P.Strafe.Add(1.3f + I * 0.04f);
		if (Passes.Num() < 16) Passes.Add(MoveTemp(P));
		Sound(Sh.K == TEXT("bombing") || Sh.K == TEXT("para") ? TEXT("bomber") : TEXT("plane_flyby"));
		return;
	}
	if (Sh.K == TEXT("chutes"))
	{
		// client/main.js chutes: five canopies drifting down over 3 s
		for (int32 I = 0; I < 5; I++)
		{
			const double Cx = Sh.X + (Rng.FRand() - 0.5) * 8, Cz = Sh.Z + (Rng.FRand() - 0.5) * 8;
			Shape(ETCFxLook::Canopy, At(Cx, Ground(Cx, Cz) + 24, Cz), At(0, -22.0 / 3.0, 0), 1.4f, 3.f);
		}
		return;
	}
	if (Sh.K == TEXT("shotdown") || Sh.K == TEXT("planedown"))
	{
		// aircraft.js shotDown: a support plane in its run (found by its spot) or a commandable plane falls from there
		if (!Sh.Kind.IsEmpty())
			for (FPass& P : Passes)
				if (!P.bDown && P.Kind == Sh.Kind && FVector2D::Distance(P.At, FVector2D(Sh.X, Sh.Z)) < 1.5) { P.bDown = true; return; }
		const double G = Ground(Sh.X, Sh.Z), Heading = Ex.bHasDir ? Ex.Dir : To ? To->Rot : 0.0;
		Falls.Add({ At(Sh.X, G + TCFxRules::AirAltitude, Sh.Z), FVector2D(FMath::Cos(Heading), FMath::Sin(Heading)), 20.f, 0.f, 0.f, 0.f });
		Burst(At(Sh.X, G + TCFxRules::AirAltitude, Sh.Z), 1.5); // fx.js fall: burst(x, y, z, 1.5)
		Sound(TEXT("flak"));
		return;
	}
	if (Sh.K == TEXT("flak"))
	{
		// aircraft.js flak: bursts around a support plane flying through, or over the spot
		for (FPass& P : Passes)
			if (!P.bDown && FVector2D::Distance(P.At, FVector2D(Sh.X, Sh.Z)) < 1.5) { for (int32 I = 0; I < 4; I++) P.Flak.Add(0.7f + I * 0.14f + Rng.FRand() * 0.06f); P.Flak.Sort(); Sound(TEXT("flak")); return; }
		for (int32 I = 0; I < 4; I++)
		{
			const double A = Rng.FRand() * Tau, D = Rng.FRand() * 7;
			const FVector B = At(Sh.X + FMath::Cos(A) * D, Ground(Sh.X, Sh.Z) + TCFxRules::AirAltitude + (Rng.FRand() - 0.5) * 5, Sh.Z + FMath::Sin(A) * D);
			Later(I * 0.12, [this, B]() { Burst(B); });
		}
		Sound(TEXT("flak"));
		return;
	}
	if (Sh.K == TEXT("aa"))
	{
		// main.js: both ends seen, a burst beside the plane (flak guns only); then the guns' tracers (fx.js aaFire)
		if (From && To && TCFxImpl::IsFlakType(From->Type))
		{
			FVector2D P(To->X, To->Z);
			if (Ctx_.DrawnAt) Ctx_.DrawnAt(To->Id, P);
			Burst(FVector(P.X + (Rng.FRand() - 0.5) * 6, P.Y + (Rng.FRand() - 0.5) * 6, Ground(P.X, P.Y) + TCFxRules::AirAltitude + (Rng.FRand() - 0.5) * 4));
		}
		AntiAir(Sh, From, To, false);
	}
}

void FTCCombatEffects::SyncFires(const FTCMatchState& S)
{
	// fx.js syncFires: snapshot.fires as cell centres with the cell's type; a fire that goes out leaves a burn mark (a
	// house leaves rubble instead)
	for (auto& P : Fires) P.Value.bKeep = false;
	const int32 W = S.Map.W;
	for (const int32 C : S.Fires)
	{
		if (W <= 0 || !S.Map.IsValidCell(C)) continue;
		FFire* F = Fires.Find(C);
		if (!F)
		{
			const float X = (C % W + 0.5f) * TCFxRules::CellMetres, Z = (C / W + 0.5f) * TCFxRules::CellMetres;
			const TCHAR Ch = (TCHAR)S.Map.Chars[C];
			F = &Fires.Add(C, { X, Ground(X, Z), Z, TCFxImpl::FireOf(Ch).Ch == Ch ? Ch : TEXT('.'), Rng.FRand(), Rng.FRand(), true });
		}
		F->bKeep = true;
	}
	for (auto It = Fires.CreateIterator(); It; ++It)
		if (!It.Value().bKeep)
		{
			if (It.Value().Kind != 'B') Scorch(It.Value().X, It.Value().Z, 1.7, 1);
			It.RemoveCurrent();
		}
}

void FTCCombatEffects::PuffCloud(int32 Id, float Cx, float Cz, float R, bool bFire, bool bBurst)
{
	// fx.js puffCloud: thick smoke that rolls slowly in place, big slowly turning puffs layered up to a few metres
	const double A = Rng.FRand() * Tau, D = FMath::Sqrt(Rng.FRand()) * R * 0.8, Size = R * Rr(0.46, 0.62);
	const double X = Cx + FMath::Cos(A) * D, Z = Cz + FMath::Sin(A) * D, Y = Ground(X, Z) + Size * Rr(0.35, 0.75) + Rng.FRand() * R * 0.18;
	const double Out = bBurst ? Rr(1.5, 3.5) : 0.25;
	// the smoke over a burning hedge or house is grey-black and rises; a smoke screen is white and hangs
	const int32 O = Emit(bFire ? ETCFxPreset::FireSmoke : ETCFxPreset::Screen, At(X, Y, Z), At(FMath::Cos(A) * Out, Rr(0.05, 0.25) + (bFire ? Rr(0.6, 1.2) : 0), FMath::Sin(A) * Out),
		bBurst ? Size * 0.55 : Size, bBurst ? Rr(4, 8) : Rr(6, 8.5));
	if (O < 0) return;
	Live[O].Group = Id;
	Tint(O, Rr(0.88, 1.05));
	if (bBurst) { Live[O].Size1 = Size * 1.35; Live[O].Drag = 1.2f; }
}

void FTCCombatEffects::SyncClouds(const FJsonObject* Raw)
{
	// fx.js syncClouds: smoke screens ([x, z, r, id]) as delivered; a new cloud over a fire is that fire's smoke
	using namespace TCFxJson;
	const TArray<TSharedPtr<FJsonValue>>* A = Raw ? ArrayOf(Raw->TryGetField(TEXT("smokes"))) : nullptr;
	for (auto& C : Clouds) C.Value.bKeep = false;
	if (A)
		for (const TSharedPtr<FJsonValue>& V : *A)
		{
			const TArray<TSharedPtr<FJsonValue>>* R = ArrayOf(V);
			double X, Z, Rad;
			if (!R || R->Num() < 3 || !Num((*R)[0], X) || !Num((*R)[1], Z) || !Num((*R)[2], Rad) || Clouds.Num() >= 256) continue;
			Rad = FMath::Clamp(Rad, 0.5, 60.0);
			const FString Key = R->Num() > 3 && (*R)[3].IsValid() && (*R)[3]->Type != EJson::Null ? FString::Printf(TEXT("id:%s"), *(*R)[3]->AsString()) : FString::Printf(TEXT("at:%.3f,%.3f"), X, Z);
			FCloud* C = Clouds.Find(Key);
			if (C) { C->X = X; C->Z = Z; C->bKeep = true; continue; } // the wind carries it
			bool bFire = false;
			for (const auto& F : Fires) if (F.Value.Kind != '.' && FMath::Sqrt(FMath::Square(F.Value.X - X) + FMath::Square(F.Value.Z - Z)) < Rad * 1.5) { bFire = true; break; }
			FCloud& N = Clouds.Add(Key, { ++CloudSerial, (float)X, (float)Z, (float)Rad, 0.f, true, bFire });
			const int32 Puffs = FMath::RoundToInt(16 * FMath::Square(Rad / 7) * (bFire ? 0.4 : 1));
			for (int32 I = 0; I < Puffs; I++) PuffCloud(N.Id, N.X, N.Z, N.R, bFire, true);
			if (!bFire)
			{
				// the smoke shells popping: small flashes low on the ground
				for (int32 I = 0; I < 4; I++) Emit(ETCFxPreset::Flash, At(X + Rr(-2.5, 2.5), Ground(X, Z) + 0.6, Z + Rr(-2.5, 2.5)), FVector::ZeroVector, 0.9f, 0.07f);
				Sound(TEXT("smoke"));
			}
		}
	for (auto It = Clouds.CreateIterator(); It; ++It)
		if (!It.Value().bKeep)
		{
			// the screen is gone on the server: thin it out quickly so it does not look like it still blocks sight
			const int32 Id = It.Value().Id;
			for (FTCFxParticle& P : Live) if (P.Group == Id && P.Preset != ETCFxPreset::None) P.Life = FMath::Min(P.Life, P.Age + Rr(1, 2.2));
			It.RemoveCurrent();
		}
}

void FTCCombatEffects::UnitDust(const FTCMatchState& S)
{
	// fx.js dustTrails (flag 32768: a vehicle moving over dry ground) and workDirt (16 digging, 128 building), held units
	for (const auto& Pair : S.Units)
	{
		const FTCUnit& U = Pair.Value;
		if (!ATCUnitRenderer::IsDrawn(U)) continue;
		if ((U.Flags & 32768) && Rng.FRand() <= 0.6f)
		{
			const double X = U.X - FMath::Cos(U.Rot) * 1.6 + Rr(-0.7, 0.7), Z = U.Z - FMath::Sin(U.Rot) * 1.6 + Rr(-0.7, 0.7);
			Emit(ETCFxPreset::RoadDust, At(X, Ground(X, Z) + 0.3, Z), At(Rr(-0.3, 0.3), Rr(0.3, 0.8), Rr(-0.3, 0.3)), Rr(0.7, 1.2), Rr(1.4, 2.4));
		}
		const bool bDig = U.Flags & 16;
		if ((bDig || (U.Flags & 128)) && Rng.FRand() <= 0.45f)
		{
			const double X = U.X + Rr(-1.2, 1.2), Z = U.Z + Rr(-1.2, 1.2), Gy = Ground(X, Z);
			if (bDig) for (int32 I = 0; I < 3; I++) Emit(ETCFxPreset::Clod, At(X, Gy + 0.3, Z), At(Rr(-1.2, 1.2), Rr(2.5, 4), Rr(-1.2, 1.2)), Rr(0.03, 0.06), Rr(0.5, 0.8));
			Emit(bDig ? ETCFxPreset::Kick : ETCFxPreset::Dust, At(X, Gy - 0.05, Z), At(Rr(-0.2, 0.2), Rr(0.1, 0.4), Rr(-0.2, 0.2)), Rr(0.3, 0.5), Rr(0.6, 1));
		}
	}
}

void FTCCombatEffects::Snapshot(const FTCMatchState& S, const FJsonObject* Raw, const FTCFxContext& Ctx, double Clock)
{
	if (!S.bHasMatch) return;
	Ctx_ = Ctx; State_ = &S; ClockNow = Clock; TickNow = S.Tick;
	const TArray<FTCShotExtra> Extras = TCShotExtras::Read(Raw);
	CurrentExtras = &Extras;
	// client/wind.js setWind from wx (kept when absent)
	if (S.Wx.bKnown) { const double K = 0.25 + 0.6 * S.Wx.WindStrength; WindXZ = FVector2D(FMath::Cos(S.Wx.WindAngle) * K, FMath::Sin(S.Wx.WindAngle) * K); }
	auto Held = [&S](int32 Id) -> const FTCUnit* { return S.Units.Find(Id); };

	// main.js: kill marks on units drawn now or last snapshot (removeUnit's wreck), then the air shots aircraft.js draws
	for (const FTCShot& Sh : S.Shots)
	{
		if (!Sh.bHasTarget) continue;
		const FTCUnit* T = Held(Sh.T);
		if (!T) T = PrevHeld.Find(Sh.T);
		if (T && (Sh.bKill || (Sh.K == TEXT("collapse") && T->Type == TEXT("tower")))) Killed.Add(Sh.T);
	}
	auto IsAirShot = [&Extras](const FTCShot& Sh, int32 I)
	{
		if (TCFxImpl::IsSupportPlane(Sh.K) || Sh.K == TEXT("chutes") || Sh.K == TEXT("shotdown") || Sh.K == TEXT("planedown") || Sh.K == TEXT("aa")) return true;
		// 'flak' with a target or a hit flag is a flak gun firing at the ground, which stays with fx.js
		return Sh.K == TEXT("flak") && !Sh.bHasTarget && !TCFxImpl::ExtraAt(Extras, I).bHasHitField;
	};
	for (int32 I = 0; I < S.Shots.Num(); I++)
	{
		const FTCShot& Sh = S.Shots[I];
		if (!IsAirShot(Sh, I) || !Sh.bHasPos) continue;
		const FTCUnit* From = Sh.bHasShooter ? Held(Sh.F) : nullptr;
		const FTCUnit* To = Sh.bHasTarget ? Held(Sh.T) : nullptr;
		AirShot(Sh, TCFxImpl::ExtraAt(Extras, I), From, To);
	}

	// projectiles.js, then fx.js snapshot for the ground war
	Projectiles.Snapshot(S, Clock);
	for (int32 I = 0; I < S.Shots.Num(); I++)
	{
		const FTCShot& Sh = S.Shots[I];
		const FTCShotExtra& Ex = TCFxImpl::ExtraAt(Extras, I);
		if (IsAirShot(Sh, I)) continue;
		if (Sh.bLocal && (Sh.K == TEXT("flight") || Sh.K == TEXT("contact") || Sh.K == TEXT("expiry") || Ex.bPhysical)) continue;
		if (Sh.K == TEXT("hurt") || Sh.K == TEXT("say") || !Sh.bHasPos) continue;
		const FTCUnit* From = Sh.bHasShooter ? Held(Sh.F) : nullptr; // seen.has(sh.f)
		if (Sh.bHasShooter && !From) Counters.ShooterNotHeld++;
		const FString& K = Sh.K;
		if (K == TEXT("throw"))
		{
			if (!From) continue;
			// a tank's HE shell or petard flies flat and fast; a squad's grenade or satchel is lobbed
			Counters.Throws++;
			NoteShooter(From->Id);
			const FTCUnitDef* D = Ctx_.Table ? Ctx_.Table->Find(From->Type) : nullptr;
			const bool bGun = D && !D->bInfantry, bSatchel = From->Type == TEXT("ranger") || From->Type == TEXT("churchill");
			FVector2D P(From->X, From->Z);
			if (Ctx_.DrawnAt) Ctx_.DrawnAt(From->Id, P);
			const double Gy = Ground(P.X, P.Y) + (bGun ? 1 : 0), T = bGun ? 0.3 : bSatchel ? 0.8 : 1.1, G = bGun ? 0 : 25, Ty = Ground(Sh.X, Sh.Z) + 0.2;
			if (Throws.Num() >= 12) Throws.RemoveAt(0);
			Throws.Add({ Sh.X, Sh.Z, bSatchel, ClockNow });
			const int32 O = Emit(ETCFxPreset::Nade, At(P.X, Gy + 1.5, P.Y), At((Sh.X - P.X) / T, (Ty - Gy - 1.5 + 0.5 * G * T * T) / T, (Sh.Z - P.Y) / T), bSatchel ? 0.2f : 0.12f, T);
			if (O >= 0) Live[O].Grav = G;
			continue;
		}
		if (K == TEXT("boom"))
		{
			const FThrow* Th = Throws.FindByPredicate([&](const FThrow& Q) { return ClockNow - Q.T < 6 && FMath::Abs(Q.X - Sh.X) < 0.5 && FMath::Abs(Q.Z - Sh.Z) < 0.5; });
			const bool bSatchel = Th && Th->bSatchel;
			Explode(Sh.X, Sh.Z, bSatchel ? 3.2 : 2.0, true, true, bSatchel ? ETCFxPreset::Masonry : ETCFxPreset::Clod, 1.0, nullptr, bSatchel ? 1.0 : 0.8);
			Sound(bSatchel ? TEXT("blast_large") : TEXT("blast_small"));
			continue;
		}
		if (K == TEXT("shell")) { Explode(Sh.X, Sh.Z, 4.2); Sound(TEXT("blast_large")); continue; }
		if (K == TEXT("rocket"))
		{
			const FSalvoMark* Sv = SalvoMarks.FindByPredicate([&](const FSalvoMark& Q) { return ClockNow - Q.T < 12 && FMath::Sqrt(FMath::Square(Q.X - Sh.X) + FMath::Square(Q.Z - Sh.Z)) <= Q.R + 2; });
			Explode(Sh.X, Sh.Z, Sv && Sv->bMortar ? 2.6 : 3.0); Sound(TEXT("blast_small"));
			continue;
		}
		if (K == TEXT("bomb")) { Explode(Sh.X, Sh.Z, 6, true, true, ETCFxPreset::Clod, 1.3); Sound(TEXT("bomb")); continue; }
		if (K == TEXT("salvo")) { Salvo(Sh, Ex, From); continue; }
		if (K == TEXT("collapse")) { if (Sh.Id.IsEmpty()) Collapse(Sh.X, Sh.Z, 4.0); continue; } // with an id: debris.js shows it
		if (K == TEXT("smokeshells")) { for (int32 J = 0; J < 3; J++) Sound(TEXT("smoke")); continue; }
		if (Ctx_.Table && Ctx_.Table->Find(K)) DirectFire(Sh, From, Sh.bHasTarget ? Held(Sh.T) : nullptr);
	}
	SyncFires(S); // before the clouds: a new cloud over a new fire is its smoke
	SyncClouds(Raw);
	UnitDust(S);

	// main.js: a held unit that is gone after a kill shot leaves a wreck (removeUnit); planes go by their own shot
	for (const auto& Pair : PrevHeld)
	{
		if (S.Units.Contains(Pair.Key)) continue;
		if (Killed.Contains(Pair.Key))
		{
			KilledGone.Add(Pair.Key);
			const FTCUnitDef* D = Ctx_.Table ? Ctx_.Table->Find(Pair.Value.Type) : nullptr;
			if (D && !D->bAir && Pair.Value.Type == TEXT("tower"))
			{
				// a Scout Tower keels over and crashes down where its top lands (placeholder: no toppling, the crash)
				FTCUnit Top = Pair.Value;
				const double A = Rng.FRand() * Tau;
				Top.X += FMath::Cos(A) * 6; Top.Z += FMath::Sin(A) * 6;
				Wreck(Top);
			}
			else if (D && !D->bAir && !D->bInfantry) Wreck(Pair.Value);
		}
		Killed.Remove(Pair.Key);
	}
	for (auto It = Killed.CreateIterator(); It; ++It) if (!S.Units.Contains(*It) && !PrevHeld.Contains(*It)) It.RemoveCurrent();

	// debris.js: falling sections and wreck motion (the wrecks list kept when absent, world-state.js)
	TArray<FTCDebrisRow> Motion;
	if (TCFxImpl::ReadWreckMotion(Raw, Motion)) HeldWreckMotion = MoveTemp(Motion);
	Debris.Snapshot(S, HeldWreckMotion, Extras, Clock);
	PrevHeld = S.Units;
	CurrentExtras = nullptr;
}


void FTCCombatEffects::Frame(const FTCFxContext& Ctx, double Clock, float Dt)
{
	// fx.js update (with projectiles.frame and debris.frame, which main.js runs every frame too)
	Ctx_ = Ctx; ClockNow = Clock;
	Dt = FMath::Clamp(Dt, 0.f, 0.25f);
	FrameDt = Dt;
	FlightDraws.Reset();
	Projectiles.Frame(Clock);
	Debris.Frame(Clock);
	RunEvents(Dt);
	UpdateEmitters(Dt);
	Simulate(Dt);
	UpdateDecals(Dt);
	if (ShakeAmount > 0.004f) ShakeAmount *= FMath::Exp(-Dt * 7.f); else ShakeAmount = 0.f;
}

void FTCCombatEffects::RunEvents(float Dt)
{
	// fx.js runEvents: delayed tracers, impacts, rockets and bursts, in the order they fall due
	for (int32 Guard = 0; Guard < 4 && Events.Num(); Guard++)
	{
		TArray<FEvent> Due;
		for (int32 I = Events.Num() - 1; I >= 0; I--) if (Events[I].At <= ClockNow) { Due.Add(MoveTemp(Events[I])); Events.RemoveAtSwap(I); }
		if (!Due.Num()) break;
		Due.Sort([](const FEvent& A, const FEvent& B) { return A.At < B.At; });
		for (FEvent& E : Due) E.Run();
	}
}

void FTCCombatEffects::UpdateEmitters(float Dt)
{
	// fx.js updateFalls (and the support planes' passes), updateSections, updateRockets, updateWrecks, updateColumns,
	// updateClouds, updateFires, in fx.js update's order
	// planes going down in flames, a blast where they land (fx.js updateFalls)
	for (int32 I = Falls.Num() - 1; I >= 0; I--)
	{
		FFall& F = Falls[I];
		F.T += Dt; F.Vy -= 10 * Dt; F.Speed *= FMath::Exp(-0.3f * Dt);
		F.P += FVector(F.D.X * F.Speed * Dt, F.D.Y * F.Speed * Dt, F.Vy * Dt);
		const float Gy = Ground(F.P.X, F.P.Y);
		if (F.P.Z <= Gy + 0.6f || F.T > 8)
		{
			Explode(F.P.X, F.P.Y, 3.6, true, true, ETCFxPreset::Metal, 1.2);
			Burning(FVector(F.P.X, F.P.Y, Gy + 0.8f), 0.9, 10, 24);
			Sound(TEXT("vehicle_destroyed"));
			Falls.RemoveAt(I);
			continue;
		}
		const int32 O = Shape(ETCFxLook::Plane, F.P, FVector(F.D.X, F.D.Y, F.Vy / FMath::Max(1.f, F.Speed)) * 0.001, 3.f, Dt * 1.5f);
		if (O >= 0) Live[O].Stretch = 5.f;
		for (F.Acc += Dt; F.Acc >= 0.035f; F.Acc -= 0.035f)
		{
			Tint(Emit(ETCFxPreset::WreckSmoke, F.P - FVector(F.D.X, F.D.Y, 0) * 1.5f, FVector(Rr(-0.3, 0.3), Rr(-0.3, 0.3), Rr(0.3, 0.9)), Rr(0.5, 0.8), Rr(2.2, 3.4)), 0.8f);
			Emit(ETCFxPreset::Burn, F.P + FVector(Rr(-0.4, 0.4), Rr(-0.4, 0.4), Rr(-0.2, 0.3)), FVector(0, 0, Rr(0.5, 1.5)), Rr(0.6, 1.0), Rr(0.25, 0.4));
		}
	}
	// support planes crossing (aircraft.js supportPlane tick): the plane, its strafing hits (fx.air 'strafe'), flak
	// around it (fx.air 'flak'), a fall
	for (int32 I = Passes.Num() - 1; I >= 0; I--)
	{
		FPass& P = Passes[I];
		P.T += Dt;
		const double U = P.T / TCFxRules::PlaneRun, A = (U - 0.5) * TCFxRules::PlaneSpan, Dx = FMath::Cos(P.Dir), Dz = FMath::Sin(P.Dir);
		const double X = P.At.X + Dx * A, Z = P.At.Y + Dz * A, Y = FMath::Max(Ground(P.At.X, P.At.Y), Ground(X, Z)) + TCFxImpl::LowOf(P.Kind) + FMath::Abs(U - 0.5) * 30;
		while (P.Strafe.Num() && P.Strafe[0] <= P.T)
		{
			const double Along = (P.Strafe[0] / 0.04 - 1.3 / 0.04) / 9.0 - 0.5; // (i / 9 - 0.5) of the run's ten hits
			P.Strafe.RemoveAt(0);
			StrafeHit(P.At.X + Dx * Along * 36 + (Rng.FRand() - 0.5) * 3, P.At.Y + Dz * Along * 36 + (Rng.FRand() - 0.5) * 3);
		}
		while (P.Flak.Num() && P.Flak[0] <= P.T) { P.Flak.RemoveAt(0); Burst(FVector(X + Dx * 3 + (Rng.FRand() - 0.5) * 5, Z + Dz * 3 + (Rng.FRand() - 0.5) * 5, Y + (Rng.FRand() - 0.5) * 4)); }
		if (P.bDown && P.T >= 1.3f)
		{
			Falls.Add({ FVector(X, Z, Y), FVector2D(Dx, Dz), (float)(TCFxRules::PlaneSpan / TCFxRules::PlaneRun * 0.75), U < 0.5 ? -10.f : 10.f, 0.f, 0.f });
			Burst(FVector(X + Dx * 2, Z + Dz * 2, Y + 0.5), 1.5);
			Passes.RemoveAt(I);
			continue;
		}
		if (U >= 1) { Passes.RemoveAt(I); continue; }
		const int32 O = Shape(ETCFxLook::Plane, FVector(X, Z, Y), FVector(Dx, Dz, 0) * 0.001, 3.f, Dt * 1.5f);
		if (O >= 0) Live[O].Stretch = 5.f;
	}
	// falling sections (debris.js poses, interpolated in Frame; fx.js updateSections draws them as boxes)
	for (const auto& Pair : SectionPoses)
	{
		const FTCDebrisRow& R = Pair.Value;
		const int32 O = Shape(ETCFxLook::Section, At(R.X, R.Y, R.Z), FVector::ZeroVector, 1.f, Dt * 1.5f);
		if (O >= 0) Live[O].ShapeRot = FRotator(0.f, FMath::RadiansToDegrees(R.Dir), FMath::RadiansToDegrees(R.Tilt));
	}
	// rockets' smoke trails (fx.js updateRockets)
	for (int32 I = Rockets.Num() - 1; I >= 0; I--)
	{
		FRocket& R = Rockets[I];
		R.T += Dt; R.Acc += Dt;
		if (R.T >= R.Dur) { Rockets.RemoveAt(I); continue; }
		while (R.Acc >= 0.03f)
		{
			R.Acc -= 0.03f;
			const float T = R.T - R.Acc;
			Emit(ETCFxPreset::Trail, R.P0 + R.V * T - FVector(0, 0, 0.5f * R.G * T * T), FVector(0, 0, 0.3f), 0.2f, Rr(1.2, 1.8));
		}
	}
	// wrecks: flames that flicker for a while, then a long trail of black smoke leaning downwind (fx.js updateWrecks)
	for (int32 I = WreckFires.Num() - 1; I >= 0; I--)
	{
		FWreckFire& W = WreckFires[I];
		W.T += Dt;
		if (W.T >= W.Dur) { WreckFires.RemoveAt(I); continue; }
		const float FireK = W.T < W.Fire * 0.5f ? 1.f : FMath::Max(0.f, 1.f - (W.T - W.Fire * 0.5f) / (W.Fire * 0.5f));
		const float SmokeK = 1.f - 0.65f * (W.T / W.Dur);
		W.Fa += Dt * FireK * 6; W.Sa += Dt * SmokeK * 4.5f; W.Ea += Dt * FireK * 3;
		while (W.Fa >= 1)
		{
			W.Fa -= 1;
			Tint(Emit(ETCFxPreset::Flame, W.P + FVector(Rr(-0.8, 0.8) * W.Big, Rr(-0.8, 0.8) * W.Big, -0.4f * W.Big), FVector(0, 0, Rr(0.1, 0.4)), Rr(0.95, 1.4) * W.Big * (0.5f + 0.5f * FireK), Rr(1, 1.6)), Rr(0.85, 1.1));
		}
		while (W.Ea >= 1) { W.Ea -= 1; Emit(ETCFxPreset::Ember, W.P + FVector(Rr(-0.5, 0.5), Rr(-0.5, 0.5), 0.6f), FVector(Rr(-1, 1), Rr(-1, 1), Rr(2, 4)), 0.05f, Rr(1, 2)); }
		while (W.Sa >= 1)
		{
			W.Sa -= 1;
			// black while the fuel burns, browner and thinner as it smoulders
			Tint(Emit(ETCFxPreset::WreckSmoke, W.P + FVector(Rr(-0.6, 0.6), Rr(-0.6, 0.6), (1 + Rng.FRand()) * W.Big), FVector(Rr(-0.3, 0.3), Rr(-0.3, 0.3), Rr(1.6, 2.6)), Rr(0.8, 1.1) * W.Big, Rr(9, 13)), 0.85f + 0.9f * (1.f - FireK));
		}
	}
	// smoke columns over big craters (fx.js updateColumns)
	for (int32 I = Columns.Num() - 1; I >= 0; I--)
	{
		FColumn& C = Columns[I];
		if ((C.T += Dt) >= C.Dur) { Columns.RemoveAt(I); continue; }
		const float K = 1.f - C.T / C.Dur;
		for (C.Acc += Dt * 5 * K; C.Acc >= 1; C.Acc -= 1)
		{
			const double A = Rng.FRand() * Tau, R = Rng.FRand() * 0.25 * C.S;
			Tint(Emit(ETCFxPreset::Smoke, C.P + FVector(FMath::Cos(A) * R, FMath::Sin(A) * R, Rr(0.3, 0.8) * C.S), FVector(FMath::Cos(A) * 0.4, FMath::Sin(A) * 0.4, Rr(2, 3.2) + 0.2 * C.S), Rr(0.32, 0.42) * C.S, Rr(7, 10) + C.S), Rr(0.8, 1.1));
		}
	}
	// smoke screens renewed as they fade (fx.js updateClouds)
	for (auto& Pair : Clouds)
	{
		FCloud& C = Pair.Value;
		const float Target = 26.f * FMath::Square(C.R / 7.f);
		for (C.Acc += Dt * Target / 7.2f; C.Acc >= 1; C.Acc -= 1) PuffCloud(C.Id, C.X, C.Z, C.R, C.bFire, false);
	}
	// fires: burning grass, hedges, woods and houses; many share the particle budget (fx.js updateFires)
	if (Fires.Num())
	{
		const float Share = FMath::Min(1.f, 50.f / Fires.Num());
		for (auto& Pair : Fires)
		{
			FFire& F = Pair.Value;
			const TCFxImpl::FFireLook& L = TCFxImpl::FireOf(F.Kind);
			for (F.Acc += Dt * L.Rate * Share; F.Acc >= 1; F.Acc--)
			{
				const int32 O = Emit(ETCFxPreset::Flame, FVector(F.X + Rr(-0.8, 0.8), F.Z + Rr(-0.8, 0.8), F.Y + L.H * Rr(0.6, 1.8)), FVector(0, 0, Rr(0.1, 0.3)), L.Size * Rr(0.8, 1.2), Rr(0.9, 1.4));
				if (O >= 0) Live[O].Pull = F.Kind == 'B' ? 0.8f : 0.4f; // in front of the walls and the hedge it burns in
				if (Rng.FRand() < 0.15f) Emit(ETCFxPreset::Ember, FVector(F.X + Rr(-0.6, 0.6), F.Z + Rr(-0.6, 0.6), F.Y + L.H + 0.8f), FVector(Rr(-1, 1), Rr(-1, 1), Rr(2, 4)), 0.05f, Rr(1, 2));
			}
			for (F.Sa += Dt * L.Smoke * Share; F.Sa >= 1; F.Sa--)
				Tint(Emit(F.Kind == '.' ? ETCFxPreset::Smoke : ETCFxPreset::WreckSmoke, FVector(F.X + Rr(-0.6, 0.6), F.Z + Rr(-0.6, 0.6), F.Y + L.H + 1), FVector(Rr(-0.3, 0.3), Rr(-0.3, 0.3), Rr(1.2, 2)), L.Size * Rr(0.8, 1.1), Rr(5, 8)), F.Kind == '.' ? 1.2f : 1.f);
		}
	}
}

void FTCCombatEffects::Simulate(float Dt)
{
	// fx.js simulate's first pass: age, the wind's drag, gravity, ground bounces, the ground under drifting smoke, spin
	SimTick++;
	for (int32 I = 0; I < Live.Num();)
	{
		FTCFxParticle& P = Live[I];
		const float Age = P.Age + Dt;
		if (Age >= P.Life) { Live.RemoveAtSwap(I, EAllowShrinking::No); continue; } // the last one moves here, still to do
		P.Age = Age;
		if (Age < 0.f) { I++; continue; } // held back: not out yet
		if (P.Preset == ETCFxPreset::None)
		{
			P.P += P.V * Dt; // placeholder shapes keep their own motion
			I++;
			continue;
		}
		if (P.Drag > 0.f)
		{
			const float Kk = FMath::Min(1.f, P.Drag * Dt), Wf = P.Wind;
			P.V.X += (WindXZ.X * Wf - P.V.X) * Kk; P.V.Y += (WindXZ.Y * Wf - P.V.Y) * Kk; P.V.Z -= P.V.Z * Kk; // smoke drifts with the game's wind
		}
		const float G = P.Grav;
		P.P.X += P.V.X * Dt; P.P.Z += (P.V.Z - 0.5f * G * Dt) * Dt; P.P.Y += P.V.Y * Dt; P.V.Z -= G * Dt;
		if (P.bFloor)
		{
			const float Gy = Ground(P.P.X, P.P.Y) + 0.06f;
			if (P.P.Z < Gy) { P.P.Z = Gy; P.V.Z = P.V.Z < -3.f ? -P.V.Z * 0.3f : 0.f; P.V.X *= 0.5f; P.V.Y *= 0.5f; P.Spin *= 0.5f; }
		}
		if (((I + SimTick) & 7) == 0) P.GroundZ = Ground(P.P.X, P.P.Y); // drifting smoke keeps its soft edge on the ground below
		P.Rot += P.Spin * Dt;
		I++;
	}
}

void FTCCombatEffects::SpriteDraws(TArray<FTCFxSpriteDraw>& Out) const
{
	// fx.js simulate's second pass: what each sprite out now looks like (sorting is the view's)
	Out.Reset(Live.Num());
	for (const FTCFxParticle& P : Live)
	{
		if (P.Preset == ETCFxPreset::None || P.Age < 0.f) continue;
		FTCFxSpriteDraw& D = Out.AddDefaulted_GetRef();
		const float T = FMath::Clamp(P.Age / P.Life, 0.f, 1.f);
		const float E = P.bCurve ? FMath::Sqrt(T) : 1.f - (1.f - T) * (1.f - T);
		const float Size = P.Size0 + (P.Size1 - P.Size0) * E;
		D.Center = FVector3f(P.P * 100.0);
		D.Size = Size * 100.f;
		D.Colour = P.C0 + (P.C1 - P.C0) * T;
		D.Alpha = P.Alpha * (P.Fin > 0.f ? FMath::Min(1.f, T / P.Fin) : 1.f) * (1.f - FMath::Pow(T, P.K));
		// flipbook: played once over its life, or looped at its own pace from a random frame
		if (P.Flip > 0)
		{
			const bool bLoop = P.Fps > 0.f;
			const float F = bLoop ? P.Phase + P.Age * P.Fps : T * (P.Flip - 1);
			const int32 Fl = FMath::FloorToInt(F);
			D.CellA = (uint8)(P.Cell + (bLoop ? Fl % P.Flip : FMath::Min(P.Flip - 1, Fl)));
			D.CellB = (uint8)(P.Cell + (bLoop ? (Fl + 1) % P.Flip : FMath::Min(P.Flip - 1, Fl + 1)));
			D.Blend = F - Fl;
		}
		else { D.CellA = D.CellB = (uint8)P.Cell; D.Blend = 0.f; }
		D.Rot = P.Rot;
		if (P.Len > 0.f)
		{
			const double Sp = P.V.Size();
			const double S = Sp > 0.0 ? Sp : 1.0;
			D.Dir = FVector3f(P.V / S);
			D.Len = (P.bFixLen ? P.Len : FMath::Min((double)P.Len, P.Age * Sp + 0.01)) * 100.f;
		}
		D.GroundZ = P.GroundZ * 100.f;
		D.Pull = P.Pull * Size * 100.f;
		D.Anchor = P.bAnchor ? 1.f : 0.f;
		D.Mode = P.bLit ? -1.f : P.Add0 + (P.Add1 - P.Add0) * T;
	}
}


// ---------------------------------------------------------------------------------------------------------------------
// The sprite and ground batches

namespace TCFxView
{
	const FVector2f Corners[4] = { FVector2f(-1.f, -1.f), FVector2f(1.f, -1.f), FVector2f(1.f, 1.f), FVector2f(-1.f, 1.f) };

	void Quad(const FTCFxSpriteDraw& S, const FVector& Origin, const FVector& Right, const FVector& Up, FVector (&Pos)[4], FVector2D& Rot)
	{
		// fx.js VERT. Three's view space is x right, y up; Right and Up are this view's axes in the world.
		const FVector C(S.Center);
		const double Size = S.Size;
		if (S.Len > 0.f)
		{
			// stretched along its motion: tail to head in view space, width across
			const FVector Tail = C - FVector(S.Dir) * S.Len;
			FVector2D Ax(FVector::DotProduct(C - Tail, Right), FVector::DotProduct(C - Tail, Up));
			const double L = Ax.Size();
			Ax = L > 1e-2 ? Ax / L : FVector2D(1.0, 0.0); // fx.js: l > 1e-4 m
			for (int32 K = 0; K < 4; K++)
			{
				const FVector2f Q = Corners[K];
				Pos[K] = FMath::Lerp(Tail, C, Q.X * 0.5 + 0.5) + Right * (Ax.X * Q.X * Size - Ax.Y * Q.Y * Size) + Up * (Ax.Y * Q.X * Size + Ax.X * Q.Y * Size);
			}
			Rot = Ax;
			return;
		}
		Rot = FVector2D(FMath::Cos(S.Rot), FMath::Sin(S.Rot));
		const FVector Pull = (Origin - C).GetSafeNormal() * S.Pull; // toward the camera, so it does not cut into the ground
		for (int32 K = 0; K < 4; K++)
		{
			const FVector2D Cc(Corners[K].X, Corners[K].Y + S.Anchor); // anchored sprites stand on their point
			const FVector2D Off(Cc.X * Rot.X - Cc.Y * Rot.Y, Cc.X * Rot.Y + Cc.Y * Rot.X);
			Pos[K] = C + Right * (Off.X * Size) + Up * (Off.Y * Size) + Pull;
		}
	}

	float SoftRatio(const FTCFxSpriteDraw& S, const FVector& Corner)
	{
		// vInfo.z / vInfo.w: the height above the ground over max(0.12 m, size x 0.22 standing, else x 0.5)
		const float Soft = FMath::Max(0.12f, S.Size / 100.f * (S.Anchor > 0.f ? 0.22f : 0.5f));
		return (float)((Corner.Z - S.GroundZ) / 100.0) / Soft;
	}
}

namespace TCFxImpl
{
	// a component's bounds: the whole battlefield (the batch is one primitive; per-particle culling is the GPU's)
	FBoxSphereBounds WorldBounds() { return FBoxSphereBounds(FBox(FVector(-2.0e6, -2.0e6, -2.0e5), FVector(2.0e6, 2.0e6, 2.0e5))); }
}

class FTCFxSpriteProxy final : public FPrimitiveSceneProxy
{
public:
	FTCFxSpriteProxy(const UTCFxSpriteComponent* C, UMaterialInterface* InMaterial, TSharedPtr<UTCFxSpriteComponent::FMailbox, ESPMode::ThreadSafe> InMail)
		: FPrimitiveSceneProxy(C), Material(InMaterial), Mail(MoveTemp(InMail))
	{
		MaterialRelevance = Material->GetRelevance_Concurrent(GetScene().GetShaderPlatform());
	}
	virtual SIZE_T GetTypeHash() const override { static size_t Unique; return reinterpret_cast<size_t>(&Unique); }
	virtual uint32 GetMemoryFootprint() const override { return sizeof(*this) + GetAllocatedSize(); }
	virtual bool CanBeOccluded() const override { return false; }

	virtual FPrimitiveViewRelevance GetViewRelevance(const FSceneView* View) const override
	{
		FPrimitiveViewRelevance R;
		R.bDrawRelevance = IsShown(View);
		R.bDynamicRelevance = true;
		R.bShadowRelevance = false;
		R.bRenderInMainPass = ShouldRenderInMainPass();
		MaterialRelevance.SetPrimitiveViewRelevance(R);
		R.bVelocityRelevance = false;
		return R;
	}

	// fx.js VERT per view, and simulate's back-to-front sort: every sprite becomes one quad turned to this view
	virtual void GetDynamicMeshElements(const TArray<const FSceneView*>& Views, const FSceneViewFamily& ViewFamily, uint32 VisibilityMap, FMeshElementCollector& Collector) const override
	{
		TSharedPtr<const FTCFxSpriteFrame, ESPMode::ThreadSafe> Frame;
		{
			FScopeLock Guard(&Mail->Lock);
			Frame = Mail->Latest;
		}
		if (!Frame.IsValid() || Frame->Sprites.Num() == 0 || !Material) return;
		const FMaterialRenderProxy* MaterialProxy = Material->GetRenderProxy();
		if (!MaterialProxy) return;
		const TArray<FTCFxSpriteDraw>& Sprites = Frame->Sprites;
		const FVector2f* Corners = TCFxView::Corners;
		for (int32 ViewIndex = 0; ViewIndex < Views.Num(); ViewIndex++)
		{
			if (!(VisibilityMap & (1 << ViewIndex))) continue;
			const FSceneView* View = Views[ViewIndex];
			const FVector Origin = View->ViewMatrices.GetViewOrigin();
			const FVector Right = View->GetViewRight(), Up = View->GetViewUp(), Fwd = View->GetViewDirection();
			// back to front; glowing sprites sort 1.5 m nearer, so flames show through the foot of their own smoke
			TArray<TPair<float, int32>> Order;
			Order.Reserve(Sprites.Num());
			for (int32 I = 0; I < Sprites.Num(); I++)
				Order.Emplace((float)FVector::DotProduct(FVector(Sprites[I].Center) - Origin, Fwd) - (Sprites[I].Mode < 0.f ? 0.f : 150.f), I);
			Order.Sort([](const TPair<float, int32>& A, const TPair<float, int32>& B) { return A.Key > B.Key; });

			TArray<FDynamicMeshVertex> Vertices;
			TArray<uint32> Indices;
			Vertices.Reserve(Order.Num() * 4);
			Indices.Reserve(Order.Num() * 6);
			for (const TPair<float, int32>& Key : Order)
			{
				const FTCFxSpriteDraw& S = Sprites[Key.Value];
				FVector Pos[4];
				FVector2D Rot;
				TCFxView::Quad(S, Origin, Right, Up, Pos, Rot);
				const uint32 Base = Vertices.Num();
				for (int32 K = 0; K < 4; K++)
				{
					const FVector2f Q = Corners[K] * 0.49f + FVector2f(0.5f, 0.5f);
					FDynamicMeshVertex V = FDynamicMeshVertex(FVector3f(Pos[K]));
					// fx.js cellUv, with Unreal's v running down the picture
					V.TextureCoordinate[0] = FVector2f(((S.CellA % 8) + Q.X) / 8.f, ((S.CellA / 8) + 1.f - Q.Y) / 8.f);
					V.TextureCoordinate[1] = FVector2f(((S.CellB % 8) + Q.X) / 8.f, ((S.CellB / 8) + 1.f - Q.Y) / 8.f);
					V.TextureCoordinate[2] = Corners[K];
					V.TextureCoordinate[3] = FVector2f((float)Rot.X, (float)Rot.Y);
					V.TextureCoordinate[4] = FVector2f(S.Blend, S.Mode);
					V.TextureCoordinate[5] = FVector2f(TCFxView::SoftRatio(S, Pos[K]), 0.f); // the soft edge on the ground
					V.TextureCoordinate[6] = FVector2f(S.Colour.X, S.Colour.Y);
					V.TextureCoordinate[7] = FVector2f(S.Colour.Z, S.Alpha);
					Vertices.Add(V);
				}
				Indices.Append({ Base, Base + 1, Base + 2, Base, Base + 2, Base + 3 });
			}
			FDynamicMeshBuilder Builder(View->GetFeatureLevel(), 8);
			Builder.AddVertices(Vertices);
			Builder.AddTriangles(Indices);
			Builder.GetMesh(FMatrix::Identity, MaterialProxy, SDPG_World, true, false, ViewIndex, Collector);
		}
	}

private:
	UMaterialInterface* Material = nullptr;
	FMaterialRelevance MaterialRelevance;
	TSharedPtr<UTCFxSpriteComponent::FMailbox, ESPMode::ThreadSafe> Mail;
};

class FTCFxGroundProxy final : public FPrimitiveSceneProxy
{
public:
	FTCFxGroundProxy(const UTCFxGroundComponent* C, UMaterialInterface* InMaterial, TSharedPtr<UTCFxGroundComponent::FMailbox, ESPMode::ThreadSafe> InMail)
		: FPrimitiveSceneProxy(C), Material(InMaterial), Mail(MoveTemp(InMail))
	{
		MaterialRelevance = Material->GetRelevance_Concurrent(GetScene().GetShaderPlatform());
	}
	virtual SIZE_T GetTypeHash() const override { static size_t Unique; return reinterpret_cast<size_t>(&Unique); }
	virtual uint32 GetMemoryFootprint() const override { return sizeof(*this) + GetAllocatedSize(); }
	virtual bool CanBeOccluded() const override { return false; }

	virtual FPrimitiveViewRelevance GetViewRelevance(const FSceneView* View) const override
	{
		FPrimitiveViewRelevance R;
		R.bDrawRelevance = IsShown(View);
		R.bDynamicRelevance = true;
		R.bShadowRelevance = false;
		R.bRenderInMainPass = ShouldRenderInMainPass();
		MaterialRelevance.SetPrimitiveViewRelevance(R);
		R.bVelocityRelevance = false;
		return R;
	}

	virtual void GetDynamicMeshElements(const TArray<const FSceneView*>& Views, const FSceneViewFamily& ViewFamily, uint32 VisibilityMap, FMeshElementCollector& Collector) const override
	{
		TSharedPtr<const FTCFxGroundFrame, ESPMode::ThreadSafe> Frame;
		{
			FScopeLock Guard(&Mail->Lock);
			Frame = Mail->Latest;
		}
		if (!Frame.IsValid() || Frame->Indices.Num() == 0 || !Material) return;
		const FMaterialRenderProxy* MaterialProxy = Material->GetRenderProxy();
		if (!MaterialProxy) return;
		for (int32 ViewIndex = 0; ViewIndex < Views.Num(); ViewIndex++)
		{
			if (!(VisibilityMap & (1 << ViewIndex))) continue;
			TArray<FDynamicMeshVertex> Vertices;
			Vertices.Reserve(Frame->Vertices.Num());
			for (const FTCFxGroundVertex& G : Frame->Vertices)
			{
				FDynamicMeshVertex V = FDynamicMeshVertex(G.P); // normal up (the rubble is lit like the ground under it)
				V.TextureCoordinate[0] = G.Uv;
				V.TextureCoordinate[1] = G.Extra;
				Vertices.Add(V);
			}
			FDynamicMeshBuilder Builder(Views[ViewIndex]->GetFeatureLevel(), 2);
			Builder.AddVertices(Vertices);
			Builder.AddTriangles(Frame->Indices);
			Builder.GetMesh(FMatrix::Identity, MaterialProxy, SDPG_World, true, false, ViewIndex, Collector);
		}
	}

private:
	UMaterialInterface* Material = nullptr;
	FMaterialRelevance MaterialRelevance;
	TSharedPtr<UTCFxGroundComponent::FMailbox, ESPMode::ThreadSafe> Mail;
};

UTCFxSpriteComponent::UTCFxSpriteComponent()
{
	PrimaryComponentTick.bCanEverTick = false;
	SetCollisionProfileName(UCollisionProfile::NoCollision_ProfileName);
	SetGenerateOverlapEvents(false);
	CastShadow = false;
	bReceivesDecals = false;
	bUseAsOccluder = false;
	bNeverDistanceCull = true;
	Mail = MakeShared<FMailbox, ESPMode::ThreadSafe>();
}

void UTCFxSpriteComponent::Publish(TArray<FTCFxSpriteDraw>&& Sprites)
{
	LastCount = Sprites.Num();
	TSharedPtr<FTCFxSpriteFrame, ESPMode::ThreadSafe> Frame = MakeShared<FTCFxSpriteFrame, ESPMode::ThreadSafe>();
	Frame->Sprites = MoveTemp(Sprites);
	FScopeLock Guard(&Mail->Lock);
	Mail->Latest = Frame;
}

void UTCFxSpriteComponent::SetFxMaterial(UMaterialInterface* M)
{
	FxMaterial = M;
	MarkRenderStateDirty();
}

FPrimitiveSceneProxy* UTCFxSpriteComponent::CreateSceneProxy()
{
	return FxMaterial ? new FTCFxSpriteProxy(this, FxMaterial, Mail) : nullptr;
}

FBoxSphereBounds UTCFxSpriteComponent::CalcBounds(const FTransform& LocalToWorld) const { return TCFxImpl::WorldBounds(); }

void UTCFxSpriteComponent::GetUsedMaterials(TArray<UMaterialInterface*>& OutMaterials, bool bGetDebugMaterials) const
{
	if (FxMaterial) OutMaterials.Add(FxMaterial);
}

UTCFxGroundComponent::UTCFxGroundComponent()
{
	PrimaryComponentTick.bCanEverTick = false;
	SetCollisionProfileName(UCollisionProfile::NoCollision_ProfileName);
	SetGenerateOverlapEvents(false);
	CastShadow = false;
	bReceivesDecals = false;
	bUseAsOccluder = false;
	bNeverDistanceCull = true;
	Mail = MakeShared<FMailbox, ESPMode::ThreadSafe>();
}

void UTCFxGroundComponent::Publish(FTCFxGroundFrame&& InFrame)
{
	LastCount = InFrame.Indices.Num() / 3;
	TSharedPtr<FTCFxGroundFrame, ESPMode::ThreadSafe> Frame = MakeShared<FTCFxGroundFrame, ESPMode::ThreadSafe>(MoveTemp(InFrame));
	FScopeLock Guard(&Mail->Lock);
	Mail->Latest = Frame;
}

void UTCFxGroundComponent::SetFxMaterial(UMaterialInterface* M)
{
	FxMaterial = M;
	MarkRenderStateDirty();
}

FPrimitiveSceneProxy* UTCFxGroundComponent::CreateSceneProxy()
{
	return FxMaterial ? new FTCFxGroundProxy(this, FxMaterial, Mail) : nullptr;
}

FBoxSphereBounds UTCFxGroundComponent::CalcBounds(const FTransform& LocalToWorld) const { return TCFxImpl::WorldBounds(); }

void UTCFxGroundComponent::GetUsedMaterials(TArray<UMaterialInterface*>& OutMaterials, bool bGetDebugMaterials) const
{
	if (FxMaterial) OutMaterials.Add(FxMaterial);
}

// ---------------------------------------------------------------------------------------------------------------------
// ATCEffects

namespace TCFxImpl
{
	enum EFxMesh { MSphere, MCube, MCylinder, MCone };
	struct FLookStyle { EFxMesh Mesh; FLinearColor Colour; };
	// the placeholder shapes (aircraft.js and fx.js draw models for these)
	FLookStyle ShapeStyle(ETCFxLook L)
	{
		switch (L)
		{
		case ETCFxLook::Plane: return { MCube, FLinearColor(0.24f, 0.26f, 0.21f) };
		case ETCFxLook::Canopy: return { MSphere, FLinearColor(0.85f, 0.82f, 0.72f) };
		case ETCFxLook::Section: return { MCube, FLinearColor(0.54f, 0.49f, 0.42f) };
		default: return { MSphere, FLinearColor::White };
		}
	}
	constexpr int32 PingKey = 101;
	// client/moods.js 'day' (the reference's default light): what fx.js light() reads, linear and divided by pi
	const FVector3f RefSky(0.2172f, 0.2431f, 0.2878f), RefGround(0.1009f, 0.0861f, 0.0583f);
	constexpr float RefSunIntensity = 3.2f;
}

ATCEffects::ATCEffects()
{
	PrimaryActorTick.bCanEverTick = true;
	PrimaryActorTick.TickGroup = TG_PostUpdateWork;
	RootComponent = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Sphere(TEXT("/Engine/BasicShapes/Sphere.Sphere"));
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cube(TEXT("/Engine/BasicShapes/Cube.Cube"));
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cyl(TEXT("/Engine/BasicShapes/Cylinder.Cylinder"));
	static ConstructorHelpers::FObjectFinder<UStaticMesh> Cone(TEXT("/Engine/BasicShapes/Cone.Cone"));
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Mat(TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	// Content/Effects (unreal/Tools/effects): the reference's effect pictures and the ports of its shaders
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Sprite(TEXT("/Game/Effects/Materials/M_TCFxSprite.M_TCFxSprite"));
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Crater(TEXT("/Game/Effects/Materials/M_TCFxCrater.M_TCFxCrater"));
	static ConstructorHelpers::FObjectFinder<UMaterialInterface> Rubble(TEXT("/Game/Effects/Materials/M_TCFxRubble.M_TCFxRubble"));
	SphereMesh = Sphere.Object; CubeMesh = Cube.Object; CylinderMesh = Cyl.Object; ConeMesh = Cone.Object; BaseMaterial = Mat.Object;
	SpriteMaterial = Sprite.Object; CraterMaterial = Crater.Object; RubbleMaterial = Rubble.Object;
}

void ATCEffects::Bind(const FTCMatchState* InState, const FTCUnitTable* InTable, ATCTerrain* InTerrain, ATCUnitRenderer* InUnits, FTCAlerts* InAlerts)
{
	State = InState; Table = InTable; Terrain = InTerrain; UnitView = InUnits; Alerts = InAlerts;
}

FTCFxContext ATCEffects::MakeContext() const
{
	FTCFxContext C;
	TWeakObjectPtr<ATCTerrain> T = Terrain;
	C.HeightAt = [T](float X, float Z) { return T.IsValid() ? T->HeightAt(X, Z) : 0.f; };
	TWeakObjectPtr<ATCUnitRenderer> U = UnitView;
	C.DrawnAt = [U](int32 Id, FVector2D& Out) { return U.IsValid() && U->DrawnAt(Id, Out); };
	C.Table = Table;
	if (const UWorld* W = GetWorld())
		if (const APlayerController* PC = W->GetFirstPlayerController())
			if (PC->PlayerCameraManager)
			{
				// the shake's falloff: the ground point the camera looks at and how far away it is
				const FVector Loc = PC->PlayerCameraManager->GetCameraLocation(), Dir = PC->PlayerCameraManager->GetCameraRotation().Vector();
				FVector2D Focus;
				if (T.IsValid() && T->RayToGround(Loc, Dir, Focus))
				{
					C.CameraFocus = Focus;
					C.CameraDistance = (float)(FVector::Dist(Loc, ATCTerrain::ToWorld(Focus.X, Focus.Y, T->HeightAt(Focus.X, Focus.Y))) / 100.0);
				}
			}
	return C;
}

void ATCEffects::OnStart(const FTCMatchState& S)
{
	Fx.Start(S);
	RubbleKey = 0; RubbleList.Reset();
	if (RubbleBatch) RubbleBatch->Publish(FTCFxGroundFrame());
	SyncRubble();
}

void ATCEffects::OnSnapshot(const FTCMatchState& S, const FJsonObject& Raw)
{
	const double Clock = GetWorld() ? GetWorld()->GetRealTimeSeconds() : 0.0;
	Fx.Snapshot(S, &Raw, MakeContext(), Clock);
	SyncRubble();
}

void ATCEffects::Reset()
{
	Fx.Reset();
	for (auto& Pair : Batches) if (Pair.Value) Pair.Value->ClearInstances();
	if (SpriteBatch) SpriteBatch->Publish(TArray<FTCFxSpriteDraw>());
	if (CraterBatch) CraterBatch->Publish(FTCFxGroundFrame());
	if (RubbleBatch) RubbleBatch->Publish(FTCFxGroundFrame());
	RubbleKey = 0; RubbleList.Reset();
	LastInstances = 0; LastSprites = 0;
}

UInstancedStaticMeshComponent* ATCEffects::Batch(int32 Key, UStaticMesh* Mesh, const FLinearColor& Colour, bool bUnlit)
{
	if (TObjectPtr<UInstancedStaticMeshComponent>* Found = Batches.Find(Key)) return *Found;
	UInstancedStaticMeshComponent* Ism = NewObject<UInstancedStaticMeshComponent>(this);
	Ism->SetStaticMesh(Mesh);
	Ism->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	Ism->SetCastShadow(false);
	Ism->SetMobility(EComponentMobility::Movable);
	Ism->SetupAttachment(RootComponent);
	Ism->RegisterComponent();
	if (BaseMaterial)
	{
		UMaterialInstanceDynamic* Mid = UMaterialInstanceDynamic::Create(BaseMaterial, this);
		Mid->SetVectorParameterValue(TEXT("Color"), Colour);
		Ism->SetMaterial(0, Mid);
	}
	Batches.Add(Key, Ism);
	return Ism;
}

void ATCEffects::EnsureComponents()
{
	if (SpriteBatch || !GetWorld()) return;
	if (!HasLook() && !bWarnedLook)
	{
		bWarnedLook = true;
		UE_LOG(LogTC, Warning, TEXT("effects: Content/Effects is missing (run unreal/Tools/effects/run.sh): the effects are simulated but not drawn"));
	}
	SpriteBatch = NewObject<UTCFxSpriteComponent>(this, TEXT("FxSprites"));
	SpriteBatch->SetupAttachment(RootComponent);
	SpriteBatch->SetTranslucentSortPriority(10); // after the ground decals and the other see-through things (fx.js renderOrder 1.5)
	if (SpriteMaterial)
	{
		SpriteLight = UMaterialInstanceDynamic::Create(SpriteMaterial, this);
		SpriteBatch->SetFxMaterial(SpriteLight);
	}
	SpriteBatch->RegisterComponent();
	CraterBatch = NewObject<UTCFxGroundComponent>(this, TEXT("FxCraters"));
	CraterBatch->SetupAttachment(RootComponent);
	CraterBatch->SetTranslucentSortPriority(-10); // under everything else that is see-through (fx.js renderOrder 0.5)
	if (CraterMaterial) CraterBatch->SetFxMaterial(CraterMaterial);
	CraterBatch->RegisterComponent();
	RubbleBatch = NewObject<UTCFxGroundComponent>(this, TEXT("FxRubble"));
	RubbleBatch->SetupAttachment(RootComponent);
	if (RubbleMaterial) RubbleBatch->SetFxMaterial(RubbleMaterial);
	RubbleBatch->RegisterComponent();
	LastLightAt = -1.0;
}

void ATCEffects::UpdateLight()
{
	// fx.js light(): the smoke is lit by the world's sun and sky. This scene's sun is read (direction, colour, lux);
	// the sky and ground fill is the scene's sky light (the material reads its irradiance), or without one the
	// reference's hemisphere (moods.js 'day') scaled by this sun's strength over the reference's; the glow of fire is
	// scaled the same way, so the effects sit in whatever exposure the scene uses.
	if (!SpriteLight || !GetWorld()) return;
	const UDirectionalLightComponent* Sun = nullptr;
	for (TObjectIterator<UDirectionalLightComponent> It; It; ++It)
	{
		if (It->GetWorld() != GetWorld() || !It->IsRegistered() || !It->IsVisible()) continue;
		if (!Sun || (It->IsUsedAsAtmosphereSunLight() && !Sun->IsUsedAsAtmosphereSunLight()) || (It->IsUsedAsAtmosphereSunLight() == Sun->IsUsedAsAtmosphereSunLight() && It->Intensity > Sun->Intensity)) Sun = *It;
	}
	FVector SunDir(0.45, 0.3, 0.84); // no sun: about the reference's (42 degrees up)
	FLinearColor SunCol(1.019f, 0.814f, 0.596f);
	float K = 1.f;
	if (Sun)
	{
		SunDir = -Sun->GetDirection();
		FLinearColor E = Sun->GetLightColor() * Sun->Intensity;
		if (Sun->bUseTemperature) E *= FLinearColor::MakeFromColorTemperature(Sun->Temperature);
		SunCol = E / PI;
		K = FMath::Max(0.f, Sun->Intensity) / TCFxImpl::RefSunIntensity;
	}
	SpriteLight->SetVectorParameterValue(TEXT("SunDir"), FLinearColor(SunDir.GetSafeNormal()));
	SpriteLight->SetVectorParameterValue(TEXT("Up"), FLinearColor(0.f, 0.f, 1.f));
	SpriteLight->SetVectorParameterValue(TEXT("SunCol"), SunCol);
	SpriteLight->SetVectorParameterValue(TEXT("SkyCol"), FLinearColor(TCFxImpl::RefSky.X * K, TCFxImpl::RefSky.Y * K, TCFxImpl::RefSky.Z * K));
	SpriteLight->SetVectorParameterValue(TEXT("GroundCol"), FLinearColor(TCFxImpl::RefGround.X * K, TCFxImpl::RefGround.Y * K, TCFxImpl::RefGround.Z * K));
	SpriteLight->SetScalarParameterValue(TEXT("Glow"), K);
	// the world's sky light, when there is one, fills the shadows of the smoke as it fills the ground's
	bool bSky = false;
	for (TObjectIterator<USkyLightComponent> It; It && !bSky; ++It) bSky = It->GetWorld() == GetWorld() && It->IsRegistered() && It->IsVisible() && It->Intensity > 0.f;
	SpriteLight->SetScalarParameterValue(TEXT("SkyFromScene"), bSky ? 1.f : 0.f);
}

void ATCEffects::SyncRubble()
{
	// client/rubble-decals.js: a picture on every known rubble cell ('R'), inset in its cell, turned by a hash of the
	// cell. The cells' material layer (World Conquest's) is not decoded here: every cell uses the brick quarter, as
	// rubble-decals.js does when a cell has no material.
	if (!State) return;
	const FTCMap& M = State->Map;
	TArray<int32> Cells;
	const int32 N = FMath::Min(M.W * M.H, M.Chars.Num());
	for (int32 C = 0; C < N && Cells.Num() < TCFxRules::RubbleCap; C++)
		if (M.Chars[C] == 'R' && (!M.bWorld || (M.Known.IsValidIndex(C) && M.Known[C]))) Cells.Add(C);
	const uint32 Key = FCrc::MemCrc32(Cells.GetData(), Cells.Num() * sizeof(int32), (uint32)M.W + 1);
	if (Key == RubbleKey && Cells.Num() == RubbleList.Num()) return;
	RubbleKey = Key;
	RubbleList = MoveTemp(Cells);
	EnsureComponents();
	if (!RubbleBatch) return;
	FTCFxGroundFrame F;
	constexpr int32 Seg = 4;
	const float Cell = TCFxRules::CellMetres;
	ATCTerrain* T = Terrain.Get();
	for (int32 Slot = 0; Slot < RubbleList.Num(); Slot++)
	{
		const int32 C = RubbleList[Slot], Cx = C % M.W, Cy = C / M.W, Q = 0; // quadrant 0: brick
		const uint32 Turn = ((uint32)(C + 1) * 2654435761u >> 30) & 3u;
		const uint32 Base = F.Vertices.Num();
		for (int32 Z = 0; Z <= Seg; Z++)
			for (int32 X = 0; X <= Seg; X++)
			{
				const float Ux = (float)X / Seg, Uz = (float)Z / Seg;
				const float Px = (Cx + 0.06f + Ux * 0.88f) * Cell, Pz = (Cy + 0.06f + Uz * 0.88f) * Cell;
				float U = Ux, V = 1.f - Uz;
				for (uint32 R = 0; R < Turn; R++) { const float U0 = U; U = V; V = 1.f - U0; }
				FTCFxGroundVertex G;
				G.P = FVector3f(Px * 100.f, Pz * 100.f, ((T ? T->HeightAt(Px, Pz) : 0.f) + 0.025f) * 100.f);
				// half-texel inset keeps the atlas quarters apart; three's v runs up the picture, Unreal's down
				G.Uv = FVector2f((Q % 2) * 0.5f + 0.001f + U * 0.498f, 1.f - ((Q < 2 ? 0.5f : 0.f) + 0.001f + V * 0.498f));
				F.Vertices.Add(G);
			}
		for (int32 Z = 0; Z < Seg; Z++)
			for (int32 X = 0; X < Seg; X++)
			{
				const uint32 A = Base + Z * (Seg + 1) + X;
				F.Indices.Append({ A, A + 5, A + 1, A + 1, A + 5, A + 6 });
			}
	}
	RubbleBatch->Publish(MoveTemp(F));
}

void ATCEffects::Tick(float DeltaSeconds)
{
	Super::Tick(DeltaSeconds);
	if (Alerts) Alerts->Frame(FPlatformTime::Seconds()); // alerts.js frame and pings.js frame (the alert clock)
	if (!State || !State->bHasMatch) { if (LastInstances) Reset(); return; }
	const double Clock = GetWorld()->GetRealTimeSeconds();
	Fx.Frame(MakeContext(), Clock, DeltaSeconds);
	DamageFrame(DeltaSeconds);
	// the sounds of the same shots are chosen and placed by UTCAudio (Client/TCAudio.h, FTCAudioCues: fx.js's own
	// sound calls with their positions); this list of names is only logged now and then
	const TArray<FString> Played = Fx.TakeSounds();
	if (Played.Num() && Clock - LastSoundLog > 30.0)
	{
		LastSoundLog = Clock;
		UE_LOG(LogTC, Verbose, TEXT("combat sound names from the effects (played by UTCAudio): %s ..."), *Played[0]);
	}
	Draw(Clock);
	ApplyShake(Clock);
}

void ATCEffects::DamageFrame(float Dt)
{
	// main.js frame loop: for a vehicle that is not a structure, plane or boat, vehicleDamageFrame with
	// animationInterest (the root visible and a 25 m ball round it in the camera's frustum). Off screen it keeps no debt.
	if (!State || !Table) return;
	FConvexVolume Frustum;
	bool bFrustum = false;
	if (const UWorld* W = GetWorld())
		if (const APlayerController* PC = W->GetFirstPlayerController())
			if (PC->PlayerCameraManager)
			{
				FMatrix ViewM, ProjM, ViewProj;
				UGameplayStatics::GetViewProjectionMatrix(PC->PlayerCameraManager->GetCameraCacheView(), ViewM, ProjM, ViewProj);
				GetViewFrustumBounds(Frustum, ViewProj, false);
				bFrustum = true;
			}
	TSet<int32> Present;
	for (const auto& P : State->Units)
	{
		const FTCUnit& U = P.Value;
		const FTCUnitDef* Def = Table->Find(U.Type);
		if (!Def || Def->bInfantry || Def->bStructure || Def->bAir || FTCUnitMotion::IsSea(U.Type)) continue;
		Present.Add(U.Id);
		const FTCPartMotion* M = UnitView.IsValid() ? UnitView->MotionOf(U.Id) : nullptr;
		const bool bVisible = ATCUnitRenderer::IsDrawn(U) && M && M->bDamageAnchor;
		const bool bDetailed = bVisible && (!bFrustum || Frustum.IntersectSphere(M->DamageAnchor, 2500.f));
		const double Health = Def->MaxHp() > 0 ? (double)U.Hp / Def->MaxHp() : 1.0;
		Fx.VehicleDamageFrame(U.Id, Health, Fx.WasKilled(U.Id), bDetailed, Dt, [M](FVector& Out)
		{
			if (!M || !M->bDamageAnchor) return false;
			Out = M->DamageAnchor / 100.0; // Unreal axes, metres
			return true;
		});
	}
	Fx.RetainDamage(Present);
}

void ATCEffects::Draw(double Clock)
{
	EnsureComponents();
	if (Clock - LastLightAt > 1.0 || LastLightAt < 0.0) { LastLightAt = Clock; UpdateLight(); }
	// the sprites (fx.js particles)
	TArray<FTCFxSpriteDraw> Sprites;
	Fx.SpriteDraws(Sprites);
	LastSprites = Sprites.Num();
	if (SpriteBatch) SpriteBatch->Publish(MoveTemp(Sprites));
	// the craters, burn marks and stains (fx.js decals)
	int32 Decals = 0;
	if (CraterBatch)
	{
		FTCFxGroundFrame F;
		constexpr int32 G = TCFxRules::DecalGrid;
		for (const FTCFxDecal& D : Fx.Decals())
		{
			if (!D.bOn) continue;
			Decals++;
			const float C = FMath::Cos(D.Rot), S = FMath::Sin(D.Rot), Fade = D.Fade();
			const uint32 Base = F.Vertices.Num();
			for (int32 J = 0; J < G; J++)
				for (int32 I = 0; I < G; I++)
				{
					const float U = ((float)I / (G - 1) - 0.5f) * 2.f * D.R, W = ((float)J / (G - 1) - 0.5f) * 2.f * D.R;
					FTCFxGroundVertex V;
					V.P = FVector3f((D.X + U * C - W * S) * 100.f, (D.Z + U * S + W * C) * 100.f, D.Heights[J * G + I] * 100.f);
					V.Uv = FVector2f((float)I / (G - 1), 1.f - (float)J / (G - 1));
					V.Extra = FVector2f(Fade, (float)D.Kind);
					F.Vertices.Add(V);
				}
			for (int32 J = 0; J < G - 1; J++)
				for (int32 I = 0; I < G - 1; I++)
				{
					const uint32 B = Base + J * G + I;
					F.Indices.Append({ B, B + G, B + 1, B + 1, B + G, B + G + 1 });
				}
		}
		CraterBatch->Publish(MoveTemp(F));
	}
	// the placeholder shapes: support planes, canopies, falling sections
	TMap<int32, TArray<FTransform>> Frames;
	for (const auto& Pair : Batches) Frames.Add(Pair.Key, {});
	UStaticMesh* Meshes[] = { SphereMesh, CubeMesh, CylinderMesh, ConeMesh };
	for (const FTCFxParticle& P : Fx.Particles())
	{
		if (P.Preset != ETCFxPreset::None) continue;
		const TCFxImpl::FLookStyle St = TCFxImpl::ShapeStyle(P.Look);
		const int32 Key = (int32)P.Look;
		Batch(Key, Meshes[St.Mesh], St.Colour, true);
		const FVector Loc = P.P * 100.0;
		FTransform T;
		if (P.Look == ETCFxLook::Section) T = FTransform(P.ShapeRot.Quaternion(), Loc, FVector(1.8f, 0.2f, 2.8f));
		else if (P.Look == ETCFxLook::Plane) T = FTransform(FRotationMatrix::MakeFromX(P.V.GetSafeNormal().IsNearlyZero() ? FVector::ForwardVector : P.V.GetSafeNormal()).ToQuat(), Loc, FVector(P.Stretch, 4.f, 0.6f));
		else T = FTransform(FQuat::Identity, Loc, FVector(FMath::Max(P.Size, 0.05f)));
		Frames.FindOrAdd(Key).Add(T);
	}
	// map pings (client/pings.js): a chalk ring at the spot, spreading and fading every 1.2 s for 4 s
	if (Alerts)
	{
		const TArray<FTCPingRing>& Rings = Alerts->Pings().Rings();
		if (Rings.Num()) Batch(TCFxImpl::PingKey, CubeMesh, FLinearColor(0.886f, 0.875f, 0.827f), true);
		const double Now = FPlatformTime::Seconds();
		for (const FTCPingRing& R : Rings)
		{
			FTCPingLook L;
			if (!Alerts->Pings().LookOf(R, Now, L)) continue;
			const float Gr = Terrain.IsValid() ? Terrain->HeightAt(R.X, R.Z) : 0.f;
			for (int32 I = 0; I < 24; I++)
			{
				const double An = I / 24.0 * 2.0 * PI;
				const FVector P(R.X + FMath::Cos(An) * L.Radius, R.Z + FMath::Sin(An) * L.Radius, Gr + 0.2);
				const float S = 0.35f * FMath::Max(0.1f, L.Opacity);
				Frames.FindOrAdd(TCFxImpl::PingKey).Add(FTransform(FQuat(FVector::UpVector, An), P * 100.0, FVector(S, S * 2.f, 0.05f)));
			}
		}
	}
	int32 Shapes = 0;
	for (auto& Pair : Frames)
	{
		TObjectPtr<UInstancedStaticMeshComponent>* Found = Batches.Find(Pair.Key);
		if (!Found || !*Found) continue;
		UInstancedStaticMeshComponent* Ism = *Found;
		Shapes += Pair.Value.Num();
		if (Ism->GetInstanceCount() == Pair.Value.Num()) { if (Pair.Value.Num()) Ism->BatchUpdateInstancesTransforms(0, Pair.Value, true, true, true); }
		else { Ism->ClearInstances(); if (Pair.Value.Num()) Ism->AddInstances(Pair.Value, false, true); }
	}
	LastInstances = LastSprites + Decals + RubbleList.Num() + Shapes;
}

void ATCEffects::ApplyShake(double Clock)
{
	APlayerController* PC = GetWorld() ? GetWorld()->GetFirstPlayerController() : nullptr;
	if (!PC || !PC->PlayerCameraManager) return;
	if (!ShakeModifier) ShakeModifier = Cast<UTCEffectsShake>(PC->PlayerCameraManager->AddNewCameraModifier(UTCEffectsShake::StaticClass()));
	if (ShakeModifier) ShakeModifier->Offset = Fx.ShakeOffset(Clock) * 100.0;
}

bool UTCEffectsShake::ModifyCamera(float DeltaTime, FMinimalViewInfo& InOutPOV)
{
	InOutPOV.Location += Offset;
	return false;
}


// ---------------------------------------------------------------------------------------------------------------------
// UTCEffectsShotCommandlet

namespace TCFxShot
{
	// [id, type, owner, x, z, rot, aim, hp, supp, tgt, cover, cd, flags, stars, built]
	FString Row(const FTCUnitTable& Table, int32 Id, const TCHAR* Type, int32 Owner, double X, double Z, double Rot)
	{
		const FTCUnitDef* D = Table.Find(Type);
		return FString::Printf(TEXT("[%d,\"%s\",%d,%g,%g,%g,%g,%d,0,0,0,0,0,0,1]"), Id, Type, Owner, X, Z, Rot, Rot, D ? FMath::Max(1, D->MaxHp()) : 100);
	}
	struct FStep { double At; FString Json; };
	struct FPicture { FString Moment; double At; FVector2D Focus; float Yaw, Pitch, Dist; };
	struct FShotScene
	{
		FString Name;
		TArray<FString> Units;           // held all along unless a step says otherwise
		TArray<FStep> Steps;             // snapshots after the first (which holds the units)
		TArray<FPicture> Pictures;
	};
	FString Snap(int32 Tick, const TArray<FString>& Units, const FString& Extra)
	{
		return FString::Printf(TEXT(R"({"t":"s","tick":%d,"units":[%s],"points":[]%s%s})"), Tick, *FString::Join(Units, TEXT(",")), Extra.IsEmpty() ? TEXT("") : TEXT(","), *Extra);
	}
	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> Obj;
		return FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Obj) && Obj.IsValid() ? Obj : nullptr;
	}
	// scenes.json's map: open ground with a hedge row and a patch of rubble
	FString Start(const FJsonObject& Map)
	{
		const int32 W = (int32)Map.GetNumberField(TEXT("w")), H = (int32)Map.GetNumberField(TEXT("h"));
		const TSharedPtr<FJsonObject> Hedge = Map.GetObjectField(TEXT("hedge")), Rubble = Map.GetObjectField(TEXT("rubble"));
		TArray<int32> RubbleRows;
		for (const TSharedPtr<FJsonValue>& V : Rubble->GetArrayField(TEXT("rows"))) RubbleRows.Add((int32)V->AsNumber());
		TArray<FString> Rows;
		for (int32 Y = 0; Y < H; Y++)
		{
			FString R = FString::ChrN(W, '.');
			for (int32 X = (int32)Hedge->GetNumberField(TEXT("x0")); X < (int32)Hedge->GetNumberField(TEXT("x1")) && Y == (int32)Hedge->GetNumberField(TEXT("row")); X++) R[X] = 'H';
			for (int32 X = (int32)Rubble->GetNumberField(TEXT("x0")); X < (int32)Rubble->GetNumberField(TEXT("x1")) && RubbleRows.Contains(Y); X++) R[X] = 'R';
			Rows.Add(R);
		}
		return FString::Printf(TEXT(R"({"t":"start","matchId":9,"you":0,"spawn":{"x":10,"z":10},"names":["Me","Foe"],"teams":[0,1],"factions":[0,1],"map":{"name":"fx","w":%d,"h":%d,"rows":["%s"],"points":[]}})"), W, H, *FString::Join(Rows, TEXT("\",\"")));
	}
	FVector CameraAt(const FPicture& P, FRotator& OutRot)
	{
		const FVector Target(P.Focus.X * 100.0, P.Focus.Y * 100.0, 0.0);
		const double Pi = FMath::DegreesToRadians(P.Pitch), Ya = FMath::DegreesToRadians(P.Yaw);
		const FVector Pos = Target + FVector(-FMath::Cos(Pi) * FMath::Cos(Ya), -FMath::Cos(Pi) * FMath::Sin(Ya), FMath::Sin(Pi)) * P.Dist * 100.0;
		OutRot = (Target - Pos).Rotation();
		return Pos;
	}
}

UTCEffectsShotCommandlet::UTCEffectsShotCommandlet()
{
	LogToConsole = true;
}

int32 UTCEffectsShotCommandlet::Main(const FString& Params)
{
	using namespace TCFxShot;
	FString OutDir;
	if (!FParse::Value(*Params, TEXT("Out="), OutDir) || OutDir.IsEmpty()) { UE_LOG(LogTC, Error, TEXT("usage: -run=TCEffectsShot -Out=<dir> -AllowCommandletRendering -RenderOffscreen")); return 1; }
	IFileManager::Get().MakeDirectory(*OutDir, true);
	FTCUnitTable Table;
	FString Error;
	if (!Table.Load(Error)) { UE_LOG(LogTC, Error, TEXT("unit table: %s"), *Error); return 1; }
	constexpr int32 W = 1280, H = 720;
	const double HFov = FMath::RadiansToDegrees(2.0 * FMath::Atan(FMath::Tan(FMath::DegreesToRadians(42.0) / 2.0) * W / H)); // camera.js: 42 degrees vertical

	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCEffectsShot"));
	FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
	Context.SetCurrentWorld(World);
	World->InitializeActorsForPlay(FURL());
	UObject* Outer = World->GetWorldSettings();
	// the light of unreal/Tools/models/compare (client/moods.js 'day' through client/viewer.js): its sun direction and
	// colour, sun 2 lux, the daylight cubemap at 5, manual exposure, bias 0.4
	UDirectionalLightComponent* SunLight = NewObject<UDirectionalLightComponent>(Outer);
	const FVector SunToward = FVector(0.6435823, -0.3715724, 0.6691306).GetSafeNormal();
	SunLight->SetWorldRotation((-SunToward).Rotation());
	SunLight->SetLightColor(FLinearColor(FColor(0xff, 0xe7, 0xc9)));
	SunLight->SetIntensity(2.f);
	SunLight->SetLightingChannels(true, false, false);
	SunLight->RegisterComponentWithWorld(World);
	TCLook::MakeUnitSun(Outer, World, SunLight); // the units' unshadowed sun, as the game's light has it (TCLook::UnitChannel)
	USkyLightComponent* Sky = NewObject<USkyLightComponent>(Outer);
	Sky->SourceType = SLS_SpecifiedCubemap;
	Sky->Cubemap = LoadObject<UTextureCube>(nullptr, TEXT("/Engine/MapTemplates/Sky/DaylightAmbientCubemap.DaylightAmbientCubemap"));
	Sky->SetLightColor(FLinearColor(FColor(0xc3, 0xcd, 0xdd)));
	Sky->SetIntensity(5.f);
	Sky->RegisterComponentWithWorld(World);
	Sky->RecaptureSky();
	USkyLightComponent::UpdateSkyCaptureContents(World);
	UMaterialInterface* Basic = LoadObject<UMaterialInterface>(nullptr, TEXT("/Engine/BasicShapes/BasicShapeMaterial.BasicShapeMaterial"));
	UMaterialInstanceDynamic* GroundMat = UMaterialInstanceDynamic::Create(Basic, Outer);
	GroundMat->SetVectorParameterValue(TEXT("Color"), FLinearColor(FColor(0x6c, 0x6c, 0x45)));
	UStaticMeshComponent* Ground = NewObject<UStaticMeshComponent>(Outer);
	Ground->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Plane.Plane")));
	Ground->SetWorldTransform(FTransform(FQuat::Identity, FVector(6000, 6000, 0), FVector(240, 240, 1)));
	Ground->SetMaterial(0, GroundMat);
	Ground->RegisterComponentWithWorld(World);

	ATCUnitRenderer* Renderer = World->SpawnActor<ATCUnitRenderer>(ATCUnitRenderer::StaticClass(), FTransform::Identity);
	Renderer->SetEnabledBatches((1u << ATCUnitRenderer::BatchCount) - 1);
	ATCEffects* Effects = World->SpawnActor<ATCEffects>(ATCEffects::StaticClass(), FTransform::Identity);
	if (!Effects->HasLook()) { UE_LOG(LogTC, Error, TEXT("effects shot: Content/Effects is missing")); return 1; }

	// the scenes (unreal/Tools/effects/compare/scenes.json, which the browser half reads too)
	FString ScenesPath, ScenesText;
	TSharedPtr<FJsonObject> Doc;
	if (!FParse::Value(*Params, TEXT("Scenes="), ScenesPath) || !FFileHelper::LoadFileToString(ScenesText, *ScenesPath) || !(Doc = Parse(ScenesText)).IsValid())
	{
		UE_LOG(LogTC, Error, TEXT("effects shot: -Scenes=<scenes.json> unreadable ('%s')"), *ScenesPath);
		return 1;
	}
	const TSharedPtr<FJsonObject> CamDoc = Doc->GetObjectField(TEXT("camera"));
	const float Pitch = (float)FMath::RadiansToDegrees(CamDoc->GetNumberField(TEXT("pitch"))), Dist = (float)CamDoc->GetNumberField(TEXT("dist"));
	const TSharedPtr<FJsonObject> MapDoc = Doc->GetObjectField(TEXT("map"));
	TArray<FShotScene> Scenes;
	for (const TSharedPtr<FJsonValue>& SV : Doc->GetArrayField(TEXT("scenes")))
	{
		const TSharedPtr<FJsonObject> SO = SV->AsObject();
		FShotScene S; S.Name = SO->GetStringField(TEXT("name"));
		for (const TSharedPtr<FJsonValue>& UV : SO->GetArrayField(TEXT("units")))
		{
			const TArray<TSharedPtr<FJsonValue>>& U = UV->AsArray();
			S.Units.Add(Row(Table, (int32)U[0]->AsNumber(), *U[1]->AsString(), (int32)U[2]->AsNumber(), U[3]->AsNumber(), U[4]->AsNumber(), U[5]->AsNumber()));
		}
		// the timeline: every step's repeats, merged by time (shots joined, other fields replaced), as fx-compare.mjs does
		TMap<int64, TSharedPtr<FJsonObject>> ByTime;
		for (const TSharedPtr<FJsonValue>& StV : SO->GetArrayField(TEXT("steps")))
		{
			const TSharedPtr<FJsonObject> St = StV->AsObject();
			double Every = 0, Count = 1;
			St->TryGetNumberField(TEXT("every"), Every); St->TryGetNumberField(TEXT("count"), Count);
			for (int32 I = 0; I < (int32)Count; I++)
			{
				const int64 Ms = FMath::RoundToInt64((St->GetNumberField(TEXT("at")) + I * Every) * 1000.0);
				TSharedPtr<FJsonObject>& M = ByTime.FindOrAdd(Ms);
				if (!M) { M = MakeShared<FJsonObject>(); M->SetArrayField(TEXT("shots"), {}); }
				const TArray<TSharedPtr<FJsonValue>>* Gone;
				if (St->TryGetArrayField(TEXT("gone"), Gone)) { TArray<TSharedPtr<FJsonValue>> G = M->HasField(TEXT("gone")) ? M->GetArrayField(TEXT("gone")) : TArray<TSharedPtr<FJsonValue>>(); G.Append(*Gone); M->SetArrayField(TEXT("gone"), G); }
				const TSharedPtr<FJsonObject>* Fields;
				if (St->TryGetObjectField(TEXT("fields"), Fields))
					for (const auto& F : (*Fields)->Values)
					{
						if (F.Key == TEXT("shots")) { TArray<TSharedPtr<FJsonValue>> Sh = M->GetArrayField(TEXT("shots")); Sh.Append(F.Value->AsArray()); M->SetArrayField(TEXT("shots"), Sh); }
						else M->SetField(F.Key, F.Value);
					}
			}
		}
		ByTime.KeySort([](int64 A, int64 B) { return A < B; });
		for (const auto& Pair : ByTime)
		{
			FString Fields;
			TSharedRef<TJsonWriter<>> Writer = TJsonWriterFactory<>::Create(&Fields);
			FJsonSerializer::Serialize(Pair.Value.ToSharedRef(), Writer);
			S.Steps.Add({ Pair.Key / 1000.0, Fields.Mid(1, Fields.Len() - 2) }); // the object's fields, spliced into the snapshot
		}
		for (const TSharedPtr<FJsonValue>& PV : SO->GetArrayField(TEXT("pictures")))
		{
			const TSharedPtr<FJsonObject> PO = PV->AsObject();
			const TArray<TSharedPtr<FJsonValue>>& F = PO->GetArrayField(TEXT("focus"));
			// client/camera.js yaw 0 looks toward -z: the camera stands on +z of its focus (Unreal yaw -90)
			S.Pictures.Add({ PO->GetStringField(TEXT("moment")), PO->GetNumberField(TEXT("at")), FVector2D(F[0]->AsNumber(), F[1]->AsNumber()), -90.f, Pitch, Dist });
		}
		Scenes.Add(MoveTemp(S));
	}

	TArray<TSharedPtr<FJsonValue>> Report;
	int32 Failures = 0;
	for (const FShotScene& Sc : Scenes)
	{
		FTCMatchState State;
		State.ApplyLobby(*Parse(TEXT(R"({"t":"lobby","code":"fx","state":"play","mode":"conquest","you":0,"spectator":false,"players":[{"name":"Me","team":0},{"name":"Foe","team":1}]})")));
		State.ApplyStart(*Parse(Start(*MapDoc)));
		Effects->Bind(&State, &Table, nullptr, Renderer, nullptr);
		Effects->Reset();
		Effects->Fx.Seed = 0x7c2a;
		Effects->OnStart(State);
		Renderer->Reset();
		TArray<FString> Units = Sc.Units;
		int32 Tick = 1, NextStep = 0, NextPicture = 0;
		auto Feed = [&](const FString& Extra, double Clock)
		{
			const TSharedPtr<FJsonObject> M = Parse(Snap(Tick++, Units, Extra));
			State.ApplySnapshot(*M);
			FTCFxContext Ctx;
			Ctx.Table = &Table;
			Ctx.HeightAt = [](float, float) { return 0.f; };
			Ctx.DrawnAt = [Renderer](int32 Id, FVector2D& Out) { return Renderer->DrawnAt(Id, Out); };
			Effects->Fx.Snapshot(State, M.Get(), Ctx, Clock);
		};
		Feed(FString(), 0.0);
		const double Dt = 1.0 / 30.0;
		for (double Clock = 0.0; NextPicture < Sc.Pictures.Num(); Clock += Dt)
		{
			while (NextStep < Sc.Steps.Num() && Sc.Steps[NextStep].At <= Clock)
			{
				const FString& J = Sc.Steps[NextStep++].Json;
				if (const TSharedPtr<FJsonObject> Fields = Parse(TEXT("{") + J + TEXT("}")))
				{
					const TArray<TSharedPtr<FJsonValue>>* Gone;
					if (Fields->TryGetArrayField(TEXT("gone"), Gone))
						for (const TSharedPtr<FJsonValue>& G : *Gone)
						{
							const FString Id = FString::Printf(TEXT("[%d,"), (int32)G->AsNumber());
							Units.RemoveAll([&Id](const FString& R) { return R.StartsWith(Id); });
						}
				}
				Feed(J, Clock);
			}
			Renderer->SetViewPoint(FVector(4000, 4000, 4000));
			Renderer->Draw(State, Table, nullptr, TSet<int32>(), (float)Dt);
			FTCFxContext Ctx;
			Ctx.Table = &Table;
			Ctx.HeightAt = [](float, float) { return 0.f; };
			Ctx.DrawnAt = [Renderer](int32 Id, FVector2D& Out) { return Renderer->DrawnAt(Id, Out); };
			Effects->Fx.Frame(Ctx, Clock, (float)Dt);
			Effects->Present(Clock);
			if (Sc.Pictures[NextPicture].At > Clock + 1e-6) continue;
			const FPicture& P = Sc.Pictures[NextPicture++];
#if WITH_EDITOR
			FAssetCompilingManager::Get().FinishAllCompilation();
			if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();
#endif
			UTextureRenderTarget2D* Target2D = NewObject<UTextureRenderTarget2D>(Outer);
			Target2D->RenderTargetFormat = RTF_RGBA8_SRGB;
			Target2D->ClearColor = FLinearColor(FColor(0xb9, 0xbd, 0xbc));
			Target2D->InitAutoFormat(W, H);
			Target2D->UpdateResourceImmediate(true);
			USceneCaptureComponent2D* Capture = NewObject<USceneCaptureComponent2D>(Outer);
			Capture->TextureTarget = Target2D;
			Capture->CaptureSource = SCS_FinalColorLDR;
			Capture->bCaptureEveryFrame = false;
			Capture->bCaptureOnMovement = false;
			Capture->FOVAngle = HFov;
			Capture->PostProcessSettings.bOverride_AutoExposureMethod = true;
			Capture->PostProcessSettings.AutoExposureMethod = AEM_Manual;
			Capture->PostProcessSettings.bOverride_AutoExposureBias = true;
			Capture->PostProcessSettings.AutoExposureBias = 0.4f;
			Capture->PostProcessSettings.bOverride_AutoExposureApplyPhysicalCameraExposure = true;
			Capture->PostProcessSettings.AutoExposureApplyPhysicalCameraExposure = false;
			Capture->PostProcessSettings.bOverride_DynamicGlobalIlluminationMethod = true;
			Capture->PostProcessSettings.DynamicGlobalIlluminationMethod = EDynamicGlobalIlluminationMethod::None;
			Capture->PostProcessSettings.bOverride_ReflectionMethod = true;
			Capture->PostProcessSettings.ReflectionMethod = EReflectionMethod::ScreenSpace;
			Capture->PostProcessBlendWeight = 1.f;
			FRotator Rot;
			const FVector Pos = CameraAt(P, Rot);
			Capture->SetWorldLocationAndRotation(Pos, Rot);
			Capture->RegisterComponentWithWorld(World);
			for (int32 F = 0; F < 4; F++) { World->SendAllEndOfFrameUpdates(); Capture->CaptureScene(); FlushRenderingCommands(); }
			TArray<FColor> Pixels;
			FTextureRenderTargetResource* Resource = Target2D->GameThread_GetRenderTargetResource();
			const FString Png = FPaths::Combine(OutDir, FString::Printf(TEXT("%s-%s-unreal.png"), *Sc.Name, *P.Moment));
			TSharedRef<FJsonObject> R = MakeShared<FJsonObject>();
			if (Resource && Resource->ReadPixels(Pixels) && Pixels.Num() == W * H)
			{
				for (FColor& C : Pixels) C.A = 255;
				TArray64<uint8> Bytes;
				FImageUtils::PNGCompressImageArray(W, H, TArrayView64<const FColor>(Pixels.GetData(), Pixels.Num()), Bytes);
				FFileHelper::SaveArrayToFile(Bytes, *Png);
				R->SetStringField(TEXT("png"), Png);
			}
			else { Failures++; UE_LOG(LogTC, Error, TEXT("effects shot %s %s: capture unreadable"), *Sc.Name, *P.Moment); }
			Capture->DestroyComponent();
			// what was alive: sprites by preset, decals, rubble cells
			TMap<FString, int32> ByPreset;
			for (const FTCFxParticle& X : Effects->Fx.Particles()) if (X.Age >= 0.f) ByPreset.FindOrAdd(X.Preset == ETCFxPreset::None ? TEXT("shape") : TCFxPresets::Name(X.Preset))++;
			TSharedRef<FJsonObject> Alive = MakeShared<FJsonObject>();
			for (const auto& Pair : ByPreset) Alive->SetNumberField(Pair.Key, Pair.Value);
			R->SetStringField(TEXT("scene"), Sc.Name);
			R->SetStringField(TEXT("moment"), P.Moment);
			R->SetNumberField(TEXT("clock"), Clock);
			R->SetNumberField(TEXT("sprites"), Effects->SpritesDrawn());
			R->SetNumberField(TEXT("decals"), Effects->Fx.DecalsOn());
			R->SetNumberField(TEXT("rubbleCells"), Effects->RubbleCells());
			R->SetObjectField(TEXT("alive"), Alive);
			R->SetStringField(TEXT("units"), Renderer->ModelStatusLine());
			Report.Add(MakeShared<FJsonValueObject>(R));
			UE_LOG(LogTC, Display, TEXT("effects shot %s %s at %.2f s: %d sprites, %d decals, %d rubble cells; %s"), *Sc.Name, *P.Moment, Clock, Effects->SpritesDrawn(), Effects->Fx.DecalsOn(), Effects->RubbleCells(), *Renderer->ModelStatusLine());
		}
	}
	TSharedRef<FJsonObject> Out = MakeShared<FJsonObject>();
	Out->SetArrayField(TEXT("shots"), Report);
	Out->SetNumberField(TEXT("failures"), Failures);
	FString Json;
	FJsonSerializer::Serialize(Out, TJsonWriterFactory<>::Create(&Json));
	FFileHelper::SaveStringToFile(Json, *FPaths::Combine(OutDir, TEXT("effects-shots.json")));
	Effects->Destroy();
	GEngine->DestroyWorldContext(World);
	World->DestroyWorld(false);
	return Failures ? 1 : 0;
}

// ---------------------------------------------------------------------------------------------------------------------
// Automation tests

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "WW2ViewReplay.h"
#include "Engine/Engine.h"

namespace TCFxTest
{
	// 15 messages, generated by the oracle tools (gen-*.mjs); every value is plain ASCII JSON
	const char* const SyntheticEffects[] = {
		R"TC({"t":"lobby","code":"fx","state":"play","mode":"conquest","you":0,"spectator":false,"players":[{"name":"Me","team":0},{"name":"Foe","team":1}]})TC",
		R"TC({"t":"start","matchId":1,"you":0,"spawn":{"x":10,"z":10},"names":["Me","Foe"],"teams":[0,1],"factions":[0,1],"map":{"name":"fx","w":60,"h":60,"rows":["","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","",""],"points":[]}})TC",
		R"TC({"t":"s","tick":100,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[],"flights":[]})TC",
		R"TC({"t":"s","tick":102,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[{"k":"flight","flight":1,"seq":0,"f":1,"fo":0,"kind":"rifle","x":20,"y":1.2,"z":20,"v":[30,4,10],"time":5.1,"local":true},{"k":"flight","flight":2,"seq":0,"f":1,"fo":0,"kind":"mg","x":20,"y":1.2,"z":20,"v":[30,4,10],"time":4.8,"local":true},{"k":"flight","flight":3,"seq":0,"kind":"tank","effect":"rocket","x":40,"y":2,"z":40,"v":[-20,8,-5],"time":5.1,"local":true}],"flights":[{"flight":1,"seq":0,"kind":"rifle","x":20,"y":1.2,"z":20,"v":[30,4,10],"gravity":9.8,"time":5.1,"launched":5.1,"expires":8.1,"until":5.6,"f":1,"fo":0},{"flight":2,"seq":0,"kind":"mg","x":20,"y":1.2,"z":20,"v":[30,4,10],"gravity":9.8,"time":4.8,"launched":4.8,"expires":7.8,"until":5.3,"f":1,"fo":0},{"flight":3,"seq":0,"kind":"tank","effect":"rocket","x":20,"y":1.2,"z":20,"v":[30,4,10],"gravity":9.8,"time":5.1,"launched":5.1,"expires":8.1,"until":5.6}]})TC",
		R"TC({"t":"s","tick":104,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[{"k":"contact","flight":1,"seq":1,"kind":"rifle","time":5.2,"x":23,"y":1.5,"z":21,"hit":false,"terminal":false,"material":"stone","local":true},{"k":"contact","flight":2,"seq":0,"kind":"mg","time":5,"x":30,"y":1,"z":25,"terminal":true,"local":true}],"flights":[{"flight":1,"seq":1,"kind":"rifle","x":23,"y":1.2,"z":21,"v":[-10,2,20],"gravity":9.8,"time":5.2,"launched":5.2,"expires":8.2,"until":5.7,"f":1,"fo":0},{"flight":2,"seq":0,"kind":"mg","x":20,"y":1.2,"z":20,"v":[30,4,10],"gravity":9.8,"time":4.8,"launched":4.8,"expires":7.8,"until":5.3,"f":1,"fo":0},{"flight":3,"seq":0,"kind":"tank","effect":"rocket","x":20,"y":1.2,"z":20,"v":[30,4,10],"gravity":9.8,"time":5.1,"launched":5.1,"expires":8.1,"until":5.6}]})TC",
		R"TC({"t":"s","tick":106,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[{"k":"contact","flight":1,"seq":2,"kind":"rifle","time":5.29,"x":22,"y":1,"z":24,"hit":true,"terminal":true,"material":"flesh","local":true},{"k":"expiry","flight":2,"seq":1,"time":5.3,"x":50,"y":0,"z":40,"local":true},{"k":"contact","flight":3,"seq":1,"kind":"tank","effect":"rocket","time":5.3,"x":34,"y":0.2,"z":38,"terminal":true,"explosive":true,"local":true},{"k":"contact","flight":9,"seq":1,"kind":"rifle","effect":"boom","time":5.25,"x":30,"y":0,"z":30,"terminal":true,"local":true}],"flights":[{"flight":3,"seq":0,"kind":"tank","effect":"rocket","x":20,"y":1.2,"z":20,"v":[30,4,10],"gravity":9.8,"time":5.1,"launched":5.1,"expires":8.1,"until":5.6}]})TC",
		R"TC({"t":"s","tick":108,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[{"k":"contact","flight":4,"seq":1,"kind":"rifle","time":5,"x":10,"y":1,"z":10,"terminal":true,"local":true}],"flights":[{"flight":1,"seq":1,"kind":"rifle","x":20,"y":1.2,"z":20,"v":[30,4,10],"gravity":9.8,"time":5.2,"launched":5.2,"expires":8.2,"until":5.7,"f":1,"fo":0}]})TC",
		R"TC({"t":"s","tick":110,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[{"k":"collapse","id":"collapse:1","at":5.45,"x":30,"z":30,"material":"stone","impulse":2,"direction":[1,0,0]},{"k":"collapse","id":"collapse:0","at":5,"x":31,"z":30}],"flights":[],"falling":[{"id":"s1","kind":"section","material":"stone","section":"wall","x":30,"y":4,"z":30,"dir":0.5,"tilt":0.1,"mass":900,"gravity":9.8,"drag":0.1,"time":5.5,"born":5.4,"at":5.4,"until":7,"impulse":1,"v":[0,-1,0]},{"id":"s2","kind":"section","material":"stone","section":"wall","x":31,"y":5,"z":30,"dir":0.5,"tilt":0,"mass":900,"gravity":9.8,"drag":0.1,"time":5.5,"born":5.4,"at":5.4,"until":7,"impulse":1,"v":[0,-1,0]}],"wrecks":[[50,"tank",1,40,40,0.3,{"id":"w50","kind":"hull","x":40,"y":0.5,"z":40,"tilt":0}]]})TC",
		R"TC({"t":"s","tick":112,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[{"k":"collapse","id":"collapse:1","at":5.45,"x":30,"z":30}],"flights":[],"falling":[{"id":"s1","kind":"section","material":"stone","section":"wall","x":30.5,"y":3,"z":30,"dir":0.5,"tilt":0.4,"mass":900,"gravity":9.8,"drag":0.1,"time":5.5,"born":5.4,"at":5.4,"until":7,"impulse":1,"v":[0,-1,0]}],"wrecks":[[50,"tank",1,40,40,0.3,{"id":"w50","kind":"hull","x":41,"y":0.5,"z":40.5,"tilt":0.2}]]})TC",
		R"TC({"t":"s","tick":114,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[],"flights":[]})TC",
		R"TC({"t":"start","matchId":1,"you":0,"spawn":{"x":10,"z":10},"names":["Me","Foe"],"teams":[0,1],"factions":[0,1],"map":{"name":"fx","w":60,"h":60,"rows":["","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","","",""],"points":[]}})TC",
		R"TC({"t":"s","tick":116,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[{"k":"flight","flight":7,"seq":0,"f":1,"fo":0,"kind":"rifle","x":20,"y":1.2,"z":20,"v":[30,4,10],"time":5.75,"local":true}],"flights":[{"flight":7,"seq":0,"kind":"rifle","x":20,"y":1.2,"z":20,"v":[30,4,10],"gravity":9.8,"time":5.75,"launched":5.75,"expires":8.75,"until":6.25,"f":1,"fo":0}]})TC",
		R"TC({"t":"s","tick":118,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[],"flights":[{"flight":7,"seq":0,"kind":"rifle","x":20,"y":1.2,"z":20,"v":[30,4,10],"gravity":9.8,"time":5.75,"launched":5.75,"expires":8.75,"until":6.25,"f":1,"fo":0}],"falling":[{"id":"s3","kind":"section","material":"stone","section":"wall","x":20,"y":6,"z":20,"dir":0.5,"tilt":0,"mass":900,"gravity":9.8,"drag":0.1,"time":5.5,"born":5.4,"at":5.4,"until":7,"impulse":1,"v":[0,-1,0]}]})TC",
		R"TC({"t":"s","tick":117,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[{"k":"contact","flight":7,"seq":1,"kind":"rifle","time":5.85,"x":25,"y":1,"z":25,"terminal":true,"local":true}],"flights":[]})TC",
		R"TC({"t":"s","tick":120,"units":[[1,"rifle",0,20,20,0,0,100,0,0,0,0,0,0,1],[2,"tank",1,60,60,0,0,100,0,0,0,0,0,0,1]],"held":[2,3],"points":[],"shots":[{"k":"contact","flight":7,"seq":1,"kind":"rifle","time":5.95,"x":25,"y":1,"z":25,"terminal":true,"hit":true,"local":true}],"flights":[]})TC",
	};
	TSharedPtr<FJsonObject> Parse(const FString& Text)
	{
		TSharedPtr<FJsonObject> Obj;
		return FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Obj) && Obj.IsValid() ? Obj : nullptr;
	}
	uint64 Fnv(const FString& Text)
	{
		const FTCHARToUTF8 Utf8(*Text);
		uint64 H = 0xcbf29ce484222325ull;
		for (int32 I = 0; I < Utf8.Length(); I++) { H ^= (uint8)Utf8.Get()[I]; H *= 0x100000001b3ull; }
		return H;
	}
	// the oracle's r3: Math.round(v * 1e6) / 1e6, written as JavaScript's String(n)
	FString N(double V) { return FTCObjectives::JsNumber(FMath::FloorToDouble(V * 1e6 + 0.5) / 1e6); }

	// One stream through FTCMatchState and the two views, logged exactly as the oracle logged the browser's
	// client/projectiles.js and client/debris.js callbacks (fx.js projectileLaunch's gate and projectileContact's
	// classification included). Frames at 0, 0.05, 0.1 and 0.2 s after each snapshot's arrival (tick / 20 s).
	FString ViewsLog(const TArray<FString>& Lines, FString& Problem)
	{
		FTCMatchState St;
		FTCProjectileView Pv;
		FTCDebrisView Dv;
		TArray<FString> Ev, Out;
		Pv.On.Trail = [&Ev](const FTCFlightDraw& D)
		{
			Ev.Add(FString::Printf(TEXT("trail,%d,%s,%s,%s,%s,%s,%s,%s,%s"), D.Flight, *N(D.P.X), *N(D.P.Y), *N(D.P.Z), *N(D.V.X), *N(D.V.Y), *N(D.V.Z), D.Kind.IsEmpty() ? TEXT("-") : *D.Kind, *D.Effect));
		};
		Pv.On.Remove = [&Ev](int32 Id) { Ev.Add(FString::Printf(TEXT("remove,%d"), Id)); };
		Pv.On.Launch = [&Ev](const FTCShot& Sh)
		{
			const bool bGate = Sh.bHasShooter && Sh.Effect != TEXT("boom");
			Ev.Add(FString::Printf(TEXT("launch,%d,%d,%s,%s,%s,%s"), Sh.Flight, bGate ? 1 : 0, *Sh.Kind, *N(Sh.X), Sh.bHasY ? *N(Sh.Y) : TEXT("NaN"), *N(Sh.Z)));
		};
		Pv.On.Contact = [&Ev](const FTCShot& Sh)
		{
			const TCHAR* Class = Sh.Effect == TEXT("boom") ? TEXT("boom") : Sh.Effect == TEXT("rocket") ? TEXT("rocket") : Sh.bTerminal ? TEXT("impact") : TEXT("sparks");
			Ev.Add(FString::Printf(TEXT("contact,%d,%s,%s,%s,%s,%s,%d,%s"), Sh.Flight, Class, *Sh.Kind, *N(Sh.X), Sh.bHasY ? *N(Sh.Y) : TEXT("NaN"), *N(Sh.Z), Sh.bHit ? 1 : 0, Sh.Material.IsEmpty() ? TEXT("soil") : *Sh.Material));
		};
		Dv.On.Pose = [&Ev](const FString& Id, const FTCDebrisRow&) { Ev.Add(TEXT("pose,") + Id); };
		Dv.On.Remove = [&Ev](const FString& Id) { Ev.Add(TEXT("dremove,") + Id); };
		Dv.On.Contact = [&Ev](const FTCShot& Sh, const FTCShotExtra&) { Ev.Add(FString::Printf(TEXT("dcontact,%s,%s,%s"), *Sh.Id, *N(Sh.X), *N(Sh.Z))); };
		TArray<FTCDebrisRow> Wrecks;
		for (int32 L = 0; L < Lines.Num(); L++)
		{
			if (Lines[L].IsEmpty()) continue;
			const TSharedPtr<FJsonObject> M = Parse(Lines[L]);
			if (!M) { Problem = FString::Printf(TEXT("line %d does not parse"), L + 1); return FString(); }
			const FString T = M->GetStringField(TEXT("t"));
			if (T == TEXT("lobby")) St.ApplyLobby(*M);
			else if (T == TEXT("start")) { St.ApplyStart(*M); Pv.Reset(); Dv.Reset(); Wrecks.Reset(); }
			else if (T == TEXT("s"))
			{
				St.ApplySnapshot(*M);
				Ev.Reset();
				const double Clock = St.Tick / 20.0;
				Pv.Snapshot(St, Clock);
				TArray<FTCDebrisRow> Motion;
				if (TCFxImpl::ReadWreckMotion(M.Get(), Motion)) Wrecks = MoveTemp(Motion);
				Dv.Snapshot(St, Wrecks, TCShotExtras::Read(M.Get()), Clock);
				for (const double Dt : { 0.0, 0.05, 0.1, 0.2 }) { Ev.Add(TEXT("frame,") + FTCObjectives::JsNumber(Dt)); Pv.Frame(Clock + Dt); }
				Out.Add(FString::FromInt(St.Tick) + (Ev.Num() ? TEXT(";") + FString::Join(Ev, TEXT(";")) : FString()));
			}
		}
		return FString::Join(Out, TEXT("\n"));
	}

	bool LoadStream(const FString& Stem, TArray<FString>& Lines)
	{
		if (Stem == TEXT("synthetic-effects")) { for (const char* L : SyntheticEffects) Lines.Add(UTF8_TO_TCHAR(L)); return true; }
		const FString Dir = FWW2ViewReplay::ProtocolDir();
		return !Dir.IsEmpty() && FFileHelper::LoadFileToStringArray(Lines, *(Dir / (Stem + TEXT(".jsonl"))));
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCEffectsRecordedTest, "ThreeCrossroads.Effects.Recorded", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCEffectsRecordedTest::RunTest(const FString& Parameters)
{
	// The oracle: reference/ww2-rts at e4e579a, client/projectiles.js and client/debris.js run by node on the same
	// streams (axel-gaps-6-7/oracle/oracle.mjs), every callback logged; a hash of that log per stream. Then the whole
	// effects module on the same stream, checking the privacy rule against each snapshot's held rows.
	using namespace TCFxTest;
	struct FExpect { const TCHAR* Stem; int32 Snapshots; uint64 Fnv; int32 Launches, Gated, Contacts, Collapses; };
	const FExpect Streams[] = {
		{ TEXT("protocol-conquest-p0"), 500, 0x824baef06b6ecca7ull, 189, 180, 185, 2 },
		{ TEXT("protocol-conquest-p1"), 500, 0x69f1e962263a0cafull, 189, 185, 180, 2 },
		{ TEXT("protocol-world-p0"), 100, 0xa60e5f6bd3c683afull, 0, 0, 0, 0 },
		{ TEXT("protocol-world-p1"), 100, 0xa60e5f6bd3c683afull, 0, 0, 0, 0 },
		{ TEXT("synthetic-effects"), 12, 0x50e97de3355c2314ull, 3, 2, 5, 1 },
	};
	FTCUnitTable Table;
	FString Err;
	if (!TestTrue(TEXT("units.json loads"), Table.Load(Err))) { AddError(Err); return false; }
	int32 Ran = 0;
	for (const FExpect& E : Streams)
	{
		TArray<FString> Lines;
		if (!LoadStream(E.Stem, Lines))
		{
			AddWarning(FString::Printf(TEXT("%s: recorded stream not installed; its checks were NOT run. Run unreal/Tools/ww2-ffi.sh build."), E.Stem));
			continue;
		}
		Ran++;
		FString Problem;
		const FString Log = ViewsLog(Lines, Problem);
		if (!TestTrue(FString::Printf(TEXT("%s replays"), E.Stem), Problem.IsEmpty())) { AddError(Problem); continue; }
		TArray<FString> Snaps;
		Log.ParseIntoArray(Snaps, TEXT("\n"));
		TestEqual(FString::Printf(TEXT("%s snapshots"), E.Stem), Snaps.Num(), E.Snapshots);
		const uint64 H = Fnv(Log);
		if (H != E.Fnv)
		{
			const FString Path = FPaths::ProjectSavedDir() / TEXT("TCEffectsTest") / (FString(E.Stem) + TEXT(".fx.txt"));
			FFileHelper::SaveStringToFile(Log, *Path, FFileHelper::EEncodingOptions::ForceUTF8WithoutBOM);
			AddError(FString::Printf(TEXT("%s: projectile and debris events differ from the browser's (hash %016llx, oracle %016llx); this client's log: %s"), E.Stem, (unsigned long long)H, (unsigned long long)E.Fnv, *Path));
		}

		// the whole module: shooter-side effects only for held shooters, counts as the browser's gate
		FTCMatchState St;
		FTCCombatEffects Fx;
		FTCFxContext Ctx;
		Ctx.Table = &Table;
		int32 Bad = 0, Checked = 0, Peak = 0;
		for (const FString& Line : Lines)
		{
			const TSharedPtr<FJsonObject> M = Parse(Line);
			if (!M) continue;
			const FString T = M->GetStringField(TEXT("t"));
			if (T == TEXT("lobby")) St.ApplyLobby(*M);
			else if (T == TEXT("start")) { St.ApplyStart(*M); Fx.Start(St); }
			else if (T == TEXT("s"))
			{
				St.ApplySnapshot(*M);
				const double Clock = St.Tick / 20.0;
				const int32 Before = Fx.ShooterLog().Num();
				Fx.Snapshot(St, M.Get(), Ctx, Clock);
				for (int32 I = Before; I < Fx.ShooterLog().Num(); I++, Checked++) if (!St.Units.Contains(Fx.ShooterLog()[I].Value)) Bad++;
				Fx.Frame(Ctx, Clock + 0.05, 0.05f);
				Fx.Frame(Ctx, Clock + 0.1, 0.05f);
				Peak = FMath::Max(Peak, Fx.Particles().Num());
			}
		}
		const FTCCombatEffects::FStats& C = Fx.Stats();
		TestEqual(FString::Printf(TEXT("%s: shooter-side effects whose shooter was not held"), E.Stem), Bad, 0);
		TestEqual(FString::Printf(TEXT("%s: launches flashed (browser's gate: f delivered, not a grenade)"), E.Stem), C.Launches, E.Gated);
		TestEqual(FString::Printf(TEXT("%s: launches not flashed"), E.Stem), C.LaunchesHidden + C.ShooterNotHeld, E.Launches - E.Gated);
		TestEqual(FString::Printf(TEXT("%s: contacts shown"), E.Stem), C.Contacts, E.Contacts);
		TestTrue(FString::Printf(TEXT("%s: particles within the cap (peak %d)"), E.Stem, Peak), Peak <= TCFxRules::Cap);
		AddInfo(FString::Printf(TEXT("%s: %d snapshots, %d launches flashed, %d hidden, %d contacts, %d blasts, %d collapses, %d shooter effects checked, peak %d particles"), E.Stem, Snaps.Num(), C.Launches, C.LaunchesHidden + C.ShooterNotHeld, C.Contacts, C.Blasts, C.Collapses, Checked, Peak));
		Fx.Reset();
		TestEqual(FString::Printf(TEXT("%s: nothing left after a reset"), E.Stem), Fx.Particles().Num() + Fx.Projectiles.Count() + Fx.Debris.Count(), 0);
	}
	TestTrue(TEXT("at least the synthetic stream ran"), Ran >= 1);
	return true;
}

namespace TCFxTest
{
	// A start and a lobby for the dispatch checks: seat 0 (team 0) against seat 1 (team 1) on a 60 x 60 open map with
	// one burning hedge row in its cells.
	const TCHAR* const DispatchLobby = TEXT(R"({"t":"lobby","code":"fx","state":"play","mode":"conquest","you":0,"spectator":false,"players":[{"name":"Me","team":0},{"name":"Foe","team":1}]})");
	FString DispatchStart()
	{
		TArray<FString> Rows;
		for (int32 Y = 0; Y < 60; Y++) Rows.Add(Y == 30 ? FString::ChrN(60, 'H') : FString());
		return FString::Printf(TEXT(R"({"t":"start","matchId":7,"you":0,"spawn":{"x":10,"z":10},"names":["Me","Foe"],"teams":[0,1],"factions":[0,1],"map":{"name":"fx","w":60,"h":60,"rows":["%s"],"points":[]}})"), *FString::Join(Rows, TEXT("\",\"")));
	}
	// [id, type, owner, x, z, rot, aim, hp, supp, tgt, cover, cd, flags, stars, built]
	FString Row(int32 Id, const TCHAR* Type, int32 Owner, double X, double Z, int32 Hp = 100, int32 Flags = 0)
	{
		return FString::Printf(TEXT("[%d,\"%s\",%d,%g,%g,0,0,%d,0,0,0,0,%d,0,1]"), Id, Type, Owner, X, Z, Hp, Flags);
	}
	FString Snap(int32 Tick, const TArray<FString>& Units, const FString& Extra)
	{
		return FString::Printf(TEXT(R"({"t":"s","tick":%d,"units":[%s],"points":[]%s%s})"), Tick, *FString::Join(Units, TEXT(",")), Extra.IsEmpty() ? TEXT("") : TEXT(","), *Extra);
	}
	int32 CountLook(const FTCCombatEffects& Fx, ETCFxLook L) { int32 N = 0; for (const FTCFxParticle& P : Fx.Particles()) N += P.Look == L; return N; }
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCEffectsDispatchTest, "ThreeCrossroads.Effects.Dispatch", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCEffectsDispatchTest::RunTest(const FString& Parameters)
{
	// fx.js snapshot's branches the recorded streams do not reach (they carry only physical flights at this pin), each
	// checked for what it draws and, above all, for what it must not draw for a shooter this side does not hold.
	using namespace TCFxTest;
	FTCUnitTable Table;
	FString Err;
	if (!TestTrue(TEXT("units.json loads"), Table.Load(Err))) { AddError(Err); return false; }
	FTCMatchState St;
	FTCCombatEffects Fx;
	FTCFxContext Ctx;
	Ctx.Table = &Table;
	Ctx.CameraFocus = FVector2D(30, 30); Ctx.CameraDistance = 60.f;
	double Clock = 0;
	auto Feed = [&](const FString& Json)
	{
		const TSharedPtr<FJsonObject> M = Parse(Json);
		if (!M) { AddError(TEXT("test message does not parse: ") + Json.Left(200)); return; }
		const FString T = M->GetStringField(TEXT("t"));
		if (T == TEXT("lobby")) St.ApplyLobby(*M);
		else if (T == TEXT("start")) { St.ApplyStart(*M); Fx.Start(St); }
		else { St.ApplySnapshot(*M); Clock = St.Tick / 20.0; Fx.Snapshot(St, M.Get(), Ctx, Clock); }
	};
	auto Run = [&](double Seconds) { for (double T = 0; T < Seconds; T += 0.05) { Clock += 0.05; Fx.Frame(Ctx, Clock, 0.05f); } };
	Feed(DispatchLobby);
	Feed(DispatchStart());
	const TArray<FString> Units = { Row(1, TEXT("mg"), 0, 20, 20, 75), Row(2, TEXT("tank"), 1, 40, 40, 360), Row(3, TEXT("mortar"), 0, 15, 15, 60) };

	// direct fire from a held MG: its muzzle flashes and tracers fly; the same shot with the shooter's id not held, or
	// with no shooter id, shows only the arrival
	Feed(Snap(10, Units, TEXT(R"("shots":[{"k":"mg","f":1,"fo":0,"t":2,"to":1,"x":40,"z":40,"hit":true}])")));
	TestEqual(TEXT("held shooter: one shooter-side effect"), Fx.ShooterLog().Num(), 1);
	Run(0.3);
	TestTrue(TEXT("held shooter: tracers in the air or arrived"), CountLook(Fx, ETCFxLook::Tracer) + CountLook(Fx, ETCFxLook::Spark) > 0);
	Feed(Snap(12, Units, TEXT(R"("shots":[{"k":"mg","f":99,"fo":1,"t":1,"to":0,"x":20,"z":20,"hit":true},{"k":"mg","t":1,"to":0,"x":20,"z":20,"hit":false}])")));
	TestEqual(TEXT("unheld or hidden shooter: no shooter-side effect"), Fx.ShooterLog().Num(), 1);
	TestEqual(TEXT("unheld or hidden shooter: both counted as hidden fire"), Fx.Stats().DirectFireHidden, 2);
	TestEqual(TEXT("an id not held is noted"), Fx.Stats().ShooterNotHeld, 1);

	// a throw: drawn from a held thrower only; a physical, local one is projectiles.js's (skipped here)
	Feed(Snap(14, Units, TEXT(R"("shots":[{"k":"throw","f":2,"fo":1,"x":20,"z":20},{"k":"throw","f":1,"fo":0,"x":30,"z":30,"physical":true,"local":true},{"k":"throw","x":21,"z":21}])")));
	TestEqual(TEXT("throw by a held enemy tank: drawn"), Fx.Stats().Throws, 1);
	TestEqual(TEXT("a held enemy's throw is a shooter-side effect of that id"), Fx.ShooterLog().Last().Value, 2);
	TestTrue(TEXT("a grenade in the air"), CountLook(Fx, ETCFxLook::Grenade) >= 1);

	// blasts by kind: shell 4.2 (a column and a shake near the camera), bomb, rocket, boom
	const int32 Blasts0 = Fx.Stats().Blasts;
	Feed(Snap(16, Units, TEXT(R"("shots":[{"k":"shell","x":30,"z":31},{"k":"bomb","x":45,"z":45},{"k":"rocket","x":5,"z":50},{"k":"boom","x":20,"z":21}])")));
	const int32 Craters = Fx.DecalsOn(0);
	TestEqual(TEXT("four blasts"), Fx.Stats().Blasts - Blasts0, 4);
	TestEqual(TEXT("each blast leaves a crater (fx.js explode: scorch)"), Craters, 4);
	TestTrue(TEXT("a shell near the camera shakes it"), Fx.Shake() > 0.004f);
	TestTrue(TEXT("fireballs"), CountLook(Fx, ETCFxLook::Fireball) >= 4);
	Run(1.0);
	TestTrue(TEXT("the shake decays"), Fx.Shake() < 0.05f);

	// salvos: a held mortar's bombs leave it (shooter-side); an unseen launcher's rockets drop from the sky (not)
	const int32 Log0 = Fx.ShooterLog().Num();
	Feed(Snap(30, Units, TEXT(R"("shots":[{"k":"salvo","f":3,"fo":0,"x":40,"z":40,"n":3,"r":3},{"k":"salvo","x":10,"z":50,"n":8,"r":6}])")));
	TestEqual(TEXT("two salvos"), Fx.Stats().Salvos, 2);
	TestEqual(TEXT("only the held mortar's salvo is shooter-side"), Fx.ShooterLog().Num() - Log0, 1);
	Run(0.3);
	TestTrue(TEXT("the mortar's bombs are leaving it"), CountLook(Fx, ETCFxLook::Rocket) >= 1);
	Run(2.0);

	// air shots (aircraft.js): a strafing pass, chutes, flak over a spot; 'aa' with both ends held bursts
	Feed(Snap(40, Units, TEXT(R"("shots":[{"k":"strafe","x":30,"z":30,"dir":0.5,"fo":1},{"k":"chutes","x":50,"z":10},{"k":"flak","x":30,"z":30}])")));
	TestEqual(TEXT("three air shots"), Fx.Stats().AirShots, 3);
	Run(0.1);
	TestTrue(TEXT("the plane is drawn"), CountLook(Fx, ETCFxLook::Plane) >= 1);
	TestEqual(TEXT("five canopies"), CountLook(Fx, ETCFxLook::Canopy), 5);
	Run(3.5);
	TestEqual(TEXT("the pass is over"), CountLook(Fx, ETCFxLook::Plane), 0);

	// fires as delivered cells: the hedge row burns (flames), and a fire that goes out leaves a mark
	Feed(Snap(50, Units, TEXT(R"("fires":[1810,1811,1812])")));
	Run(1.0);
	TestTrue(TEXT("flames over the burning cells"), CountLook(Fx, ETCFxLook::Flame) > 0);
	const int32 Scorch0 = Fx.DecalsOn(1);
	Feed(Snap(52, Units, TEXT(R"("fires":[])")));
	TestEqual(TEXT("three hedge fires out: three burn marks (fx.js scorch kind 1)"), Fx.DecalsOn(1) - Scorch0, 3);

	// smoke screens as delivered; gone on the server: thinned out
	Feed(Snap(54, Units, TEXT(R"("smokes":[[30,30,7,"c1"]])")));
	TestTrue(TEXT("a smoke screen"), CountLook(Fx, ETCFxLook::Screen) > 0);
	{
		// the screen is translucent puffs, not solid shapes: no sprite of it covers more than the preset's 0.9
		Run(1.0);
		TArray<FTCFxSpriteDraw> Draws;
		Fx.SpriteDraws(Draws);
		float MaxAlpha = 0.f;
		int32 Lit = 0;
		for (const FTCFxSpriteDraw& D : Draws) if (D.Mode < 0.f) { Lit++; MaxAlpha = FMath::Max(MaxAlpha, D.Alpha); }
		TestTrue(TEXT("the screen's puffs are drawn as lit sprites"), Lit > 10);
		TestTrue(FString::Printf(TEXT("no lit sprite is more opaque than its preset's peak (max %.3f)"), MaxAlpha), MaxAlpha <= 0.92f);
	}
	Feed(Snap(56, Units, TEXT(R"("smokes":[])")));
	Run(2.5);
	TestEqual(TEXT("the cleared screen thinned out"), CountLook(Fx, ETCFxLook::Screen), 0);

	// wrecks: a held enemy tank gone after a kill shot burns; gone without one (fog) does not; an id never held does not
	const int32 Wrecks0 = Fx.Stats().Wrecks;
	Feed(Snap(60, { Units[0], Units[2] }, TEXT(R"("gone":[2],"shots":[{"k":"tank","t":2,"to":1,"x":40,"z":40,"kill":true},{"k":"rifle","t":77,"to":1,"x":5,"z":5,"kill":true}])")));
	TestEqual(TEXT("one wreck for the killed tank"), Fx.Stats().Wrecks - Wrecks0, 1);
	Feed(Snap(62, Units, FString()));
	Feed(Snap(64, { Units[0], Units[2] }, TEXT(R"("gone":[2])")));
	TestEqual(TEXT("a tank that only went out of sight leaves no wreck"), Fx.Stats().Wrecks - Wrecks0, 1);
	Run(1.0);
	TestTrue(TEXT("the wreck burns"), CountLook(Fx, ETCFxLook::Flame) > 0);

	// the cap and the reset
	for (int32 I = 0; I < 40; I++) Feed(Snap(100 + I * 2, { Units[0], Units[2] }, TEXT(R"("shots":[{"k":"bomb","x":30,"z":30},{"k":"bomb","x":31,"z":30},{"k":"bomb","x":30,"z":31}])")));
	TestTrue(TEXT("particles never pass the cap"), Fx.Particles().Num() <= TCFxRules::Cap);
	TestTrue(TEXT("the cap dropped some"), Fx.Stats().Dropped > 0);
	Fx.Reset();
	TestEqual(TEXT("reset: nothing alive"), Fx.Particles().Num(), 0);
	TestEqual(TEXT("reset: no shake"), Fx.Shake(), 0.f);
	TestEqual(TEXT("reset: no craters"), Fx.DecalsOn(), 0);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCEffectsLookTest, "ThreeCrossroads.Effects.Look", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCEffectsLookTest::RunTest(const FString& Parameters)
{
	// The look against client/fx.js at e4e579a: its presets (values copied by hand from FX, not from this port), the
	// draw pass, the billboards of VERT, the crater decals, and the imported pictures and materials.
	using namespace TCFxTest;
	{
		const FTCFxPreset& Smoke = TCFxPresets::Get(ETCFxPreset::Smoke);
		TestTrue(TEXT("smoke: lit, a 0.72, cells 0..3, grow 2.8, pull 0.45"), Smoke.bLit && FMath::IsNearlyEqual(Smoke.A, 0.72f) && Smoke.Cell == 0 && Smoke.Cells == 4 && FMath::IsNearlyEqual(Smoke.Grow, 2.8f) && FMath::IsNearlyEqual(Smoke.Pull, 0.45f));
		// lin(0x5f574d): three's sRGB-to-linear of 0x5f = 95
		TestTrue(FString::Printf(TEXT("smoke c0 is lin(0x5f574d) (r %.4f)"), Smoke.C0.X), FMath::IsNearlyEqual(Smoke.C0.X, 0.1146f, 0.0005f));
		const FTCFxPreset& Fireball = TCFxPresets::Get(ETCFxPreset::Fireball);
		TestTrue(TEXT("fireball: glows, a 15-cell flipbook from cell 24, add 0.55 -> 0.1"), !Fireball.bLit && Fireball.Cell == 24 && Fireball.Flip == 15 && FMath::IsNearlyEqual(Fireball.Add, 0.55f) && FMath::IsNearlyEqual(Fireball.Add1, 0.1f));
		const FTCFxPreset& Flame = TCFxPresets::Get(ETCFxPreset::Flame);
		TestTrue(TEXT("flame: looped flipbook of 12 at 15 fps from cell 12, anchored, unturned"), Flame.Flip == 12 && FMath::IsNearlyEqual(Flame.Fps, 15.f) && Flame.Cell == 12 && Flame.bAnchor && Flame.bNoRot);
		const FTCFxPreset& Tracer = TCFxPresets::Get(ETCFxPreset::TracerHot);
		TestTrue(TEXT("hot tracer: cell 60, len 4.5, add 1"), Tracer.Cell == 60 && FMath::IsNearlyEqual(Tracer.Len, 4.5f) && FMath::IsNearlyEqual(Tracer.Add, 1.f));
		const FTCFxPreset& Muzzle = TCFxPresets::Get(ETCFxPreset::Muzzle);
		TestTrue(TEXT("muzzle: one of cells 56..59 at a fixed length"), Muzzle.Cell == 56 && Muzzle.Cells == 4 && Muzzle.bFixLen);
		const FTCFxPreset& Screen = TCFxPresets::Get(ETCFxPreset::Screen);
		TestTrue(TEXT("screen: lit, a 0.9, fade in over 22 %"), Screen.bLit && FMath::IsNearlyEqual(Screen.A, 0.9f) && FMath::IsNearlyEqual(Screen.Fin, 0.22f));
		for (int32 I = 1; I < (int32)ETCFxPreset::Count; I++)
		{
			const FTCFxPreset& P = TCFxPresets::Get((ETCFxPreset)I);
			TestTrue(FString::Printf(TEXT("%s: its cells are in the atlas"), TCFxPresets::Name((ETCFxPreset)I)), P.Cell >= 0 && P.Cell + FMath::Max(P.Cells, P.Flip) <= 64);
		}
	}

	// the draw pass: a blast's sprites, flipbooks within their cells, colour and size over life, nothing held back drawn
	FTCMatchState St;
	FTCCombatEffects Fx;
	FTCFxContext Ctx;
	Ctx.HeightAt = [](float X, float Z) { return 0.05f * X; }; // a slope, so the craters' fit can be checked
	St.ApplyLobby(*Parse(DispatchLobby));
	St.ApplyStart(*Parse(DispatchStart()));
	Fx.Start(St);
	St.ApplySnapshot(*Parse(Snap(10, { Row(1, TEXT("mg"), 0, 20, 20, 75) }, TEXT(R"("shots":[{"k":"shell","x":30,"z":30}])"))));
	Fx.Snapshot(St, Parse(Snap(10, { Row(1, TEXT("mg"), 0, 20, 20, 75) }, TEXT(R"("shots":[{"k":"shell","x":30,"z":30}])"))).Get(), Ctx, 0.5);
	TArray<FTCFxSpriteDraw> Draws;
	Fx.SpriteDraws(Draws);
	int32 Held = 0;
	for (const FTCFxParticle& P : Fx.Particles()) Held += P.Age < 0.f;
	TestTrue(TEXT("a shell's smoke waits over the crater (fx.js hold)"), Held > 0);
	TestEqual(TEXT("held-back puffs are not drawn"), Draws.Num(), Fx.Particles().Num() - Held);
	double T = 0.5;
	for (int32 F = 0; F < 10; F++) { T += 0.05; Fx.Frame(Ctx, T, 0.05f); }
	Fx.SpriteDraws(Draws);
	bool bCells = true, bFinite = true, bFireball = false;
	for (const FTCFxSpriteDraw& D : Draws)
	{
		bCells &= D.CellA < 64 && D.CellB < 64 && D.Blend >= 0.f && D.Blend <= 1.f;
		bFinite &= FMath::IsFinite(D.Size) && D.Size > 0.f && FMath::IsFinite(D.Alpha) && D.Alpha >= 0.f && D.Alpha <= 1.f && !D.Center.ContainsNaN();
		bFireball |= D.CellA >= 24 && D.CellA < 39 && D.Mode >= 0.f;
	}
	TestTrue(TEXT("every frame shown is an atlas cell, blends in [0, 1]"), bCells);
	TestTrue(TEXT("sizes, alphas and places are finite"), bFinite);
	TestTrue(TEXT("the fireball plays its flipbook"), bFireball);
	// the crater: fx.js scorch radius min(6.5, 0.7 x 4.2 + 0.4), a 5 x 5 grid 5 cm over the slope
	TestEqual(TEXT("one crater"), Fx.DecalsOn(0), 1);
	for (const FTCFxDecal& D : Fx.Decals())
	{
		if (!D.bOn) continue;
		TestTrue(FString::Printf(TEXT("crater radius %.2f"), D.R), FMath::IsNearlyEqual(D.R, 3.34f, 0.01f));
		const float C = FMath::Cos(D.Rot), S = FMath::Sin(D.Rot);
		bool bFit = true;
		for (int32 J = 0; J < 5; J++) for (int32 I = 0; I < 5; I++)
		{
			const float U = (I / 4.f - 0.5f) * 2.f * D.R, W = (J / 4.f - 0.5f) * 2.f * D.R;
			bFit &= FMath::IsNearlyEqual(D.Heights[J * 5 + I], 0.05f * (D.X + U * C - W * S) + 0.05f, 1e-3f);
		}
		TestTrue(TEXT("the crater grid lies 5 cm over the slope"), bFit);
		TestTrue(FString::Printf(TEXT("fading in (%.2f after 0.5 s)"), D.Fade()), D.Fade() > 0.99f);
	}

	// VERT: a plain sprite faces the view, an anchored one stands on its point, a streak runs from tail to head, pull
	// moves it toward the camera
	{
		const FVector Origin(0, 0, 1000), Right(0, 1, 0), Up(0, 0, 1);
		FTCFxSpriteDraw D;
		D.Center = FVector3f(1000, 0, 0); D.Size = 50.f;
		FVector Pos[4]; FVector2D Rot;
		TCFxView::Quad(D, Origin, Right, Up, Pos, Rot);
		TestTrue(TEXT("plain: corners at +-size along the view's axes"), Pos[0].Equals(FVector(1000, -50, -50), 0.01) && Pos[2].Equals(FVector(1000, 50, 50), 0.01));
		D.Anchor = 1.f;
		TCFxView::Quad(D, Origin, Right, Up, Pos, Rot);
		TestTrue(TEXT("anchored: its bottom on its point"), FMath::IsNearlyEqual(Pos[0].Z, 0.0, 0.01) && FMath::IsNearlyEqual(Pos[2].Z, 100.0, 0.01));
		D.Anchor = 0.f; D.Pull = 30.f;
		TCFxView::Quad(D, Origin, Right, Up, Pos, Rot);
		const FVector Mid = (Pos[0] + Pos[2]) * 0.5;
		TestTrue(TEXT("pulled 30 cm toward the camera"), FMath::IsNearlyEqual(FVector::Dist(Mid, FVector(1000, 0, 0)), 30.0, 0.01) && FVector::Dist(Mid, Origin) < FVector::Dist(FVector(1000, 0, 0), Origin));
		D.Pull = 0.f; D.Dir = FVector3f(0, 1, 0); D.Len = 200.f; D.Size = 10.f;
		TCFxView::Quad(D, Origin, Right, Up, Pos, Rot);
		TestTrue(TEXT("streak: from 2 m behind to its head, along its motion"), FMath::IsNearlyEqual(Pos[0].Y, -210.0, 0.01) && FMath::IsNearlyEqual(Pos[1].Y, 10.0, 0.01) && Rot.Equals(FVector2D(1, 0), 1e-6));
		D.Len = 0.f; D.Center = FVector3f(0, 0, 30); D.GroundZ = 0.f; D.Size = 100.f;
		TestTrue(TEXT("soft edge: 0.3 m over the ground at size 1 m is 0.6 of the way in"), FMath::IsNearlyEqual(TCFxView::SoftRatio(D, FVector(0, 0, 30)), 0.6f, 1e-4f));
	}

	// the pictures and the ports of the shaders (Content/Effects, unreal/Tools/effects/run.sh)
	for (const TCHAR* Path : { TEXT("/Game/Effects/Materials/M_TCFxSprite.M_TCFxSprite"), TEXT("/Game/Effects/Materials/M_TCFxCrater.M_TCFxCrater"), TEXT("/Game/Effects/Materials/M_TCFxRubble.M_TCFxRubble") })
		TestNotNull(FString::Printf(TEXT("%s loads"), Path), LoadObject<UMaterialInterface>(nullptr, Path));
	for (const TCHAR* Path : { TEXT("/Game/Effects/Textures/T_FxAtlas.T_FxAtlas"), TEXT("/Game/Effects/Textures/T_FxCrater.T_FxCrater"), TEXT("/Game/Effects/Textures/T_FxRubble.T_FxRubble") })
		TestNotNull(FString::Printf(TEXT("%s loads"), Path), LoadObject<UTexture>(nullptr, Path));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCEffectsExtrasTest, "ThreeCrossroads.Effects.Extras", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCEffectsExtrasTest::RunTest(const FString& Parameters)
{
	// The raw shot fields the decoder does not keep stay aligned with FTCMatchState::Shots (non-objects skipped).
	using namespace TCFxTest;
	const TSharedPtr<FJsonObject> M = Parse(TEXT(R"({"shots":[{"k":"salvo","n":3,"r":4.5,"physical":true},7,{"k":"say","text":"Hold","localized":{"en":"Hold!"}},{"k":"contact","impulse":2,"direction":[1,0,2],"hit":false},{"k":"strafe","dir":1.25}]})"));
	const TArray<FTCShotExtra> E = TCShotExtras::Read(M.Get());
	if (!TestEqual(TEXT("four objects"), E.Num(), 4)) return false;
	TestTrue(TEXT("salvo n, r, physical"), E[0].bHasN && E[0].N == 3 && E[0].bHasR && E[0].R == 4.5 && E[0].bPhysical);
	TestTrue(TEXT("say text and localized.en"), E[1].bHasText && E[1].Text == TEXT("Hold") && E[1].bHasLocalizedEn && E[1].LocalizedEn == TEXT("Hold!"));
	TestTrue(TEXT("contact impulse, direction, hit present"), E[2].bHasImpulse && E[2].Impulse == 2 && E[2].bHasDirection && E[2].Direction == FVector(1, 0, 2) && E[2].bHasHitField);
	TestTrue(TEXT("strafe dir"), E[3].bHasDir && E[3].Dir == 1.25 && !E[3].bHasHitField);
	TestEqual(TEXT("no raw message: no extras"), TCShotExtras::Read(nullptr).Num(), 0);
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCEffectsActorTest, "ThreeCrossroads.Effects.Actor", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCEffectsActorTest::RunTest(const FString& Parameters)
{
	// The presentation itself: ATCEffects in a transient game world, fed the synthetic effects stream (and a conquest
	// recording when installed) through its message hooks, ticked, drawn as instances; a ping ring drawn; a reset
	// clears every instance. No renderer is needed for the instance counts.
	using namespace TCFxTest;
	FTCUnitTable Table;
	FString Err;
	if (!TestTrue(TEXT("units.json loads"), Table.Load(Err))) { AddError(Err); return false; }
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("TCEffectsTestWorld"));
	if (!TestNotNull(TEXT("a transient world"), World)) return false;
	FWorldContext& WorldContext = GEngine->CreateNewWorldContext(EWorldType::Game);
	WorldContext.SetCurrentWorld(World);
	ATCEffects* Fx = World->SpawnActor<ATCEffects>();
	bool bOk = TestNotNull(TEXT("the effects actor spawns"), Fx);
	if (bOk)
	{
		FTCMatchState St;
		FTCAlerts Alerts;
		Fx->Bind(&St, &Table, nullptr, nullptr, &Alerts);
		TArray<FString> Lines;
		LoadStream(TEXT("synthetic-effects"), Lines);
		TArray<FString> Conquest;
		if (LoadStream(TEXT("protocol-conquest-p0"), Conquest)) Lines.Append(Conquest);
		int32 MaxDrawn = 0, MaxSprites = 0, Snaps = 0;
		bool bRingDrawn = false;
		TestTrue(TEXT("the effect materials are found (Content/Effects)"), Fx->HasLook());
		for (const FString& Line : Lines)
		{
			const TSharedPtr<FJsonObject> M = Parse(Line);
			if (!M) continue;
			const FString T = M->GetStringField(TEXT("t"));
			if (T == TEXT("lobby")) St.ApplyLobby(*M);
			else if (T == TEXT("start")) { St.ApplyStart(*M); Alerts.StartMatch(St); Fx->OnStart(St); }
			else if (T == TEXT("s"))
			{
				St.ApplySnapshot(*M);
				Fx->OnSnapshot(St, *M);
				if (++Snaps == 20)
				{
					FTCAlertPush P;
					bRingDrawn = Alerts.ReceivePing(*Parse(TEXT(R"({"t":"ping","from":0,"x":30,"z":30})")), St, FPlatformTime::Seconds(), P);
				}
				Fx->Tick(0.05f);
				MaxDrawn = FMath::Max(MaxDrawn, Fx->InstancesDrawn());
				MaxSprites = FMath::Max(MaxSprites, Fx->SpritesDrawn());
			}
		}
		TestTrue(FString::Printf(TEXT("instances drawn at some point (peak %d over %d snapshots)"), MaxDrawn, Snaps), MaxDrawn > 0);
		TestTrue(FString::Printf(TEXT("sprites handed to the batch (peak %d)"), MaxSprites), MaxSprites > 0);
		TestTrue(TEXT("a ping was received for its ring"), bRingDrawn);
		TestTrue(TEXT("particles stay within the cap"), Fx->Fx.Particles().Num() <= TCFxRules::Cap);
		Fx->Reset();
		Fx->Tick(0.05f);
		TestEqual(TEXT("after a reset only the ping ring may remain"), Fx->Fx.Particles().Num(), 0);
		Fx->Bind(nullptr, nullptr, nullptr, nullptr, nullptr);
		Fx->Tick(0.05f);
		TestEqual(TEXT("unbound (no match): nothing drawn"), Fx->InstancesDrawn(), 0);
		AddInfo(FString::Printf(TEXT("actor: %d snapshots, peak %d instances, %d sprites"), Snaps, MaxDrawn, MaxSprites));
	}
	GEngine->DestroyWorldContext(World);
	World->DestroyWorld(false);
	return bOk;
}

#endif // WITH_DEV_AUTOMATION_TESTS
