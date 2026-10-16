#include "Client/TCCameraPawn.h"
#include "Camera/CameraComponent.h"
#include "Misc/AutomationTest.h"
#include "SceneView.h"
#include "Client/TCPersistence.h"
#include "ThreeCrossroads.h"
#include "Engine/GameViewportClient.h"
#include "GameFramework/GameUserSettings.h"
#include "GameFramework/PlayerController.h"
#include "UnrealClient.h"
#include "Widgets/SLeafWidget.h"
#include "Rendering/DrawElementTypes.h"
#include "InputCoreTypes.h"
#include "Widgets/SWindow.h"
#include "GenericPlatform/GenericWindow.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "HAL/PlatformProcess.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#if PLATFORM_LINUX
// ApplicationCore exports the SDL API it uses. A cursor rectangle alone does not enable SDL's native grab.
struct SDL_Window;
struct SDL_Rect { int x, y, w, h; };
extern "C" SDL_Window* SDL_GetKeyboardFocus(void);
extern "C" bool SDL_SetWindowMouseGrab(SDL_Window*, bool);
extern "C" bool SDL_GetWindowMouseGrab(SDL_Window*);
extern "C" bool SDL_SetWindowMouseRect(SDL_Window*, const SDL_Rect*);
extern "C" bool SDL_GetWindowSize(SDL_Window*, int*, int*);
extern "C" unsigned int SDL_GetWindowProperties(SDL_Window*);
extern "C" void* SDL_GetPointerProperty(unsigned int, const char*, void*);
extern "C" int64 SDL_GetNumberProperty(unsigned int, const char*, int64);
extern "C" const char* SDL_GetCurrentVideoDriver(void);
#endif

namespace TCCaptureImpl
{
#if PLATFORM_LINUX
	// SDL's X11 grab may report success while an external core-pointer warp escapes it. Query the native
	// window on SDL's own display so this check sees the real pointer, including synthetic X11 motion.
	class FX11Pointer
	{
		using FQuery = int (*)(void*, unsigned long, unsigned long*, unsigned long*, int*, int*, int*, int*, unsigned int*);
		using FWarp = int (*)(void*, unsigned long, unsigned long, int, int, unsigned int, unsigned int, int, int);
		using FDisplayCall = int (*)(void*);
		using FLock = void (*)(void*);
		void* Library = FPlatformProcess::GetDllHandle(TEXT("libX11.so.6"));
		FQuery Query = Library ? reinterpret_cast<FQuery>(FPlatformProcess::GetDllExport(Library, TEXT("XQueryPointer"))) : nullptr;
		FWarp Warp = Library ? reinterpret_cast<FWarp>(FPlatformProcess::GetDllExport(Library, TEXT("XWarpPointer"))) : nullptr;
		FDisplayCall Flush = Library ? reinterpret_cast<FDisplayCall>(FPlatformProcess::GetDllExport(Library, TEXT("XFlush"))) : nullptr;
		FLock Lock = Library ? reinterpret_cast<FLock>(FPlatformProcess::GetDllExport(Library, TEXT("XLockDisplay"))) : nullptr;
		FLock Unlock = Library ? reinterpret_cast<FLock>(FPlatformProcess::GetDllExport(Library, TEXT("XUnlockDisplay"))) : nullptr;
	public:
		bool Confine(SDL_Window* Handle, FIntPoint Size) const
		{
			if (!Query || !Warp || !Flush || !Lock || !Unlock) return false;
			const auto Properties = SDL_GetWindowProperties(Handle);
			void* Display = SDL_GetPointerProperty(Properties, "SDL.window.x11.display", nullptr);
			const auto Window = static_cast<unsigned long>(SDL_GetNumberProperty(Properties, "SDL.window.x11.window", 0));
			if (!Display || !Window) return false;
			Lock(Display);
			unsigned long Root = 0, Child = 0; int RootX = 0, RootY = 0, X = 0, Y = 0; unsigned int Mask = 0;
			const bool bSameScreen = Query(Display, Window, &Root, &Child, &RootX, &RootY, &X, &Y, &Mask) != 0;
			if (bSameScreen)
			{
				const FVector2D Local(X,Y), Clamped = FTCMouseCapture::ConfinedPosition(Local, Size);
				if (Clamped != Local)
				{
					Warp(Display, 0, Window, 0,0,0,0, int(Clamped.X), int(Clamped.Y));
					Flush(Display);
					if (FParse::Param(FCommandLine::Get(), TEXT("TCSessionLog"))) UE_LOG(LogTC, Log, TEXT("capture mouse: corrected escaped X11 cursor %d,%d to %.0f,%.0f"), X,Y,Clamped.X,Clamped.Y);
				}
			}
			Unlock(Display);
			return bSameScreen;
		}
	};
#endif
	// The arrow polygon and ink/chalk colours are pointer.js SPRITES.arrow, with its (3,2) hot spot.
	class SSourceCursor : public SLeafWidget
	{
	public:
		SLATE_BEGIN_ARGS(SSourceCursor) {} SLATE_END_ARGS()
		void Construct(const FArguments&) {}
		virtual FVector2D ComputeDesiredSize(float) const override { return FVector2D(64, 64); }
		virtual int32 OnPaint(const FPaintArgs&, const FGeometry& G, const FSlateRect&, FSlateWindowElementList& E, int32 Layer, const FWidgetStyle&, bool) const override
		{
			const TArray<FVector2D> Shape = { {3,2}, {3,24}, {8.6,18.8}, {12.5,27.6}, {16.1,26}, {12.2,17.4}, {20,17.4} };
			const FLinearColor Ink(FColor(0x1b,0x1d,0x14)), Chalk(FColor(0xf3,0xe7,0xb0));
			// Scanline fill keeps this text-defined source silhouette independent of an external cursor asset.
			for (double Y = 2.25; Y < 28; Y += 0.5)
			{
				TArray<double> Cross;
				for (int32 I = 0; I < Shape.Num(); I++)
				{
					const auto A = Shape[I], B = Shape[(I + 1) % Shape.Num()];
					if ((A.Y <= Y && B.Y > Y) || (B.Y <= Y && A.Y > Y)) Cross.Add(A.X + (Y - A.Y) * (B.X - A.X) / (B.Y - A.Y));
				}
				Cross.Sort();
				for (int32 I = 0; I + 1 < Cross.Num(); I += 2)
					FSlateDrawElement::MakeLines(E, Layer, G.ToPaintGeometry(), TArray<FVector2D>{ {Cross[I] + 29, Y + 30}, {Cross[I+1] + 29, Y + 30} }, ESlateDrawEffect::None, Chalk, true, 0.7f);
			}
			TArray<FVector2D> Outline; for (const auto P : Shape) Outline.Add(P + FVector2D(29,30)); Outline.Add(Shape[0] + FVector2D(29,30));
			FSlateDrawElement::MakeLines(E, Layer + 1, G.ToPaintGeometry(), Outline, ESlateDrawEffect::None, Ink, true, 1.6f);
			return Layer + 1;
		}
	};
}

FString FTCMouseCapture::ValidMode(const FString& V)
{
	return V == TEXT("always") || V == TEXT("off") ? V : FString(TEXT("fullscreen"));
}

FVector2D FTCMouseCapture::ConfinedPosition(FVector2D Position, FIntPoint Size)
{
	return FVector2D(FMath::Clamp(Position.X, 0., double(FMath::Max(0, Size.X - 1))), FMath::Clamp(Position.Y, 0., double(FMath::Max(0, Size.Y - 1))));
}

bool FTCMouseCapture::WindowFocused(const TSharedPtr<SWindow>& Window)
{
	const auto Native = Window ? Window->GetNativeWindow() : nullptr;
#if PLATFORM_LINUX
	return Native && SDL_GetKeyboardFocus() == Native->GetOSWindowHandle();
#else
	return Native && Native->IsForegroundWindow();
#endif
}

FString FTCMouseCapture::Label() const
{
	return FString(TEXT("Capture mouse: ")) + (ModeValue == TEXT("always") ? TEXT("Always") : ModeValue == TEXT("off") ? TEXT("Off") : TEXT("In fullscreen"));
}

void FTCMouseCapture::Cycle()
{
	SetMode(ModeValue == TEXT("fullscreen") ? TEXT("always") : ModeValue == TEXT("always") ? TEXT("off") : TEXT("fullscreen"));
	FTCSettingsStore::Game().Set(TEXT("ww2-capture"), ModeValue);
}

bool FTCMouseCapture::Frame(bool bPlaying, bool bFullscreen, bool bFocused, bool bActivate, bool bEscape)
{
	if (bPlaying && !bWasPlaying) bSuspended = false;
	bWasPlaying = bPlaying;
	if (bEscape) { bCaptured = false; bSuspended = true; }
	if (!bPlaying || !bFocused || (bWasFullscreen && !bFullscreen && ModeValue == TEXT("fullscreen"))) bCaptured = false;
	else if (!bSuspended && bActivate && (ModeValue == TEXT("always") || (ModeValue == TEXT("fullscreen") && bFullscreen))) bCaptured = true;
	bWasFullscreen = bFullscreen;
	return bCaptured;
}

void FTCMouseCapture::Apply(APlayerController* PC)
{
	if (!PC || !PC->GetWorld() || !PC->GetWorld()->GetGameViewport()) return;
	auto* V = PC->GetWorld()->GetGameViewport();
	const bool bChanged = !bApplied || bAppliedCapture != bCaptured;
	if (bChanged)
	{
		bApplied = true; bAppliedCapture = bCaptured;
		if (!V->HasSoftwareCursor(EMouseCursor::Default)) V->SetSoftwareCursorWidget(EMouseCursor::Default, SNew(TCCaptureImpl::SSourceCursor));
		V->SetUseSoftwareCursorWidgets(bCaptured);
		PC->DefaultMouseCursor = bCaptured ? EMouseCursor::Default : EMouseCursor::Crosshairs;
		FInputModeGameAndUI Input;
		Input.SetHideCursorDuringCapture(false);
		Input.SetLockMouseToViewportBehavior(bCaptured ? EMouseLockMode::LockAlways : EMouseLockMode::DoNotLock);
		PC->SetInputMode(Input);
		PC->bShowMouseCursor = true;
		if (!bCaptured && V->Viewport) V->Viewport->CaptureMouse(false);
	}
#if PLATFORM_LINUX
	const auto Window = V->GetWindow();
	const auto Native = Window ? Window->GetNativeWindow() : nullptr;
	if (auto* Handle = Native ? static_cast<SDL_Window*>(Native->GetOSWindowHandle()) : nullptr)
	{
		// SetInputMode and Slate's mouse capture can replace SDL's grab. Set confinement afterward, and retain
		// an explicit window-relative barrier across focus, resize and UI capture changes.
		SDL_Rect Rect{0,0,0,0};
		SDL_GetWindowSize(Handle, &Rect.w, &Rect.h);
		SDL_SetWindowMouseRect(Handle, bCaptured && Rect.w > 0 && Rect.h > 0 ? &Rect : nullptr);
		if (SDL_GetWindowMouseGrab(Handle) != bCaptured && !SDL_SetWindowMouseGrab(Handle, bCaptured))
		{
			bCaptured = false;
			UE_LOG(LogTC, Warning, TEXT("capture mouse: native window refused the mouse grab"));
		}
		// Wayland owns its global coordinates and uses its native barrier. The X11 fallback only acts on the
		// focused captured window, after normal input-mode setup and SDL confinement.
		const char* Driver = SDL_GetCurrentVideoDriver();
		if (bChanged && bCaptured) UE_LOG(LogTC, Log, TEXT("capture backend: %s, focused %d, client %dx%d"), UTF8_TO_TCHAR(Driver ? Driver : "none"), SDL_GetKeyboardFocus() == Handle, Rect.w, Rect.h);
		if (bCaptured && SDL_GetKeyboardFocus() == Handle && Driver && FCStringAnsi::Strcmp(Driver, "x11") == 0 && Rect.w > 0 && Rect.h > 0)
		{
			static const TCCaptureImpl::FX11Pointer Pointer;
			if (!Pointer.Confine(Handle, FIntPoint(Rect.w,Rect.h)) && bChanged) UE_LOG(LogTC, Warning, TEXT("capture mouse: X11 pointer confinement is unavailable"));
		}
	}
#endif
	if (bChanged) UE_LOG(LogTC, Log, TEXT("capture mouse: mode %s, confined %d, drawn cursor %d, suspended %d"), *ModeValue, bCaptured, V->GetIsUsingSoftwareCursorWidgets(), bSuspended);
}

void FTCMouseCapture::Drive(APlayerController* PC, bool bPlaying)
{
	if (!PC || !PC->GetWorld()) return;
	if (!bLoaded) { const auto Saved = FTCSettingsStore::Game().Get(TEXT("ww2-capture")); SetMode(Saved.Get(FString())); bLoaded = true; }
	auto* V = PC->GetWorld()->GetGameViewport();
	if (!V || !V->Viewport) return;
	Frame(bPlaying, V->Viewport->IsFullscreen(), WindowFocused(V->GetWindow()), PC->WasInputKeyJustPressed(EKeys::LeftMouseButton), PC->WasInputKeyJustPressed(EKeys::Escape));
	Apply(PC);
}

void FTCMouseCapture::Request(APlayerController* PC, bool bPlaying, bool bToggle)
{
	if (bToggle) ToggleNow(bPlaying);
	else
	{
		bSuspended = false;
		auto* V = PC && PC->GetWorld() ? PC->GetWorld()->GetGameViewport() : nullptr;
		if (V && V->Viewport) Frame(bPlaying, V->Viewport->IsFullscreen(), WindowFocused(V->GetWindow()), true, false);
	}
	Apply(PC);
}

void FTCMouseCapture::Fullscreen(APlayerController* PC, bool bPlaying)
{
	auto* S = UGameUserSettings::GetGameUserSettings();
	if (!S) return;
	const bool bEntering = S->GetFullscreenMode() == EWindowMode::Windowed;
	S->SetFullscreenMode(bEntering ? EWindowMode::WindowedFullscreen : EWindowMode::Windowed);
	S->ApplyResolutionSettings(false);
	if (bEntering && ModeValue != TEXT("off")) { bSuspended = false; bCaptured = bPlaying; }
	else if (ModeValue == TEXT("fullscreen")) bCaptured = false;
	Apply(PC);
}

ATCCameraPawn::ATCCameraPawn()
{
	PrimaryActorTick.bCanEverTick = false;
	RootComponent = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));
	Camera = CreateDefaultSubobject<UCameraComponent>(TEXT("Camera"));
	Camera->SetupAttachment(RootComponent);
	// The engine reads FieldOfView as horizontal at the camera's AspectRatio and, with MaintainYFOV, keeps the vertical
	// angle that implies for any window (FMinimalViewInfo::CalculateProjectionMatrixGivenViewRectangle). So the
	// horizontal angle that gives 42 degrees vertically at that aspect is set, and the axis is fixed here rather than
	// left to the LocalPlayer's ini setting.
	Camera->bOverrideAspectRatioAxisConstraint = true;
	Camera->SetAspectRatioAxisConstraint(EAspectRatioAxisConstraint::AspectRatio_MaintainYFOV);
	const float HalfY = FMath::DegreesToRadians(VerticalFovDegrees / 2.f);
	Camera->SetFieldOfView(FMath::RadiansToDegrees(2.f * FMath::Atan(FMath::Tan(HalfY) * Camera->AspectRatio)));
	SetActorEnableCollision(false);
}

void ATCCameraPawn::Place(FVector2D Focus, float GroundMetres, float Yaw, float Pitch, float Distance)
{
	const FVector Target(Focus.X * 100.f, Focus.Y * 100.f, GroundMetres * 100.f);
	const FRotator Look(-Pitch, Yaw, 0.f);
	SetActorLocationAndRotation(Target - Look.Vector() * Distance * 100.f, Look);
}

FTCPinhole::FTCPinhole(FVector Target, float Yaw, float Pitch, float Dist, double Aspect)
{
	const FRotationMatrix RM(FRotator(-Pitch, Yaw, 0.f));
	F = RM.GetUnitAxis(EAxis::X); R = RM.GetUnitAxis(EAxis::Y); U = RM.GetUnitAxis(EAxis::Z);
	Pos = Target - F * Dist;
	TanY = FMath::Tan(FMath::DegreesToRadians(ATCCameraPawn::VerticalFovDegrees / 2.0));
	TanX = TanY * Aspect;
}

bool FTCPinhole::Project(const FVector& P, double VX, double VY, FVector2D& Out) const
{
	const FVector D = P - Pos;
	const double Z = D | F;
	if (Z <= 0.1) return false;
	Out = FVector2D((1.0 + (D | R) / Z / TanX) / 2.0 * VX, (1.0 - (D | U) / Z / TanY) / 2.0 * VY);
	return true;
}

bool FTCPinhole::Ground(FVector2D S, double VX, double VY, double Height, FVector2D& Out) const
{
	const FVector Dir = F + R * ((2.0 * S.X / VX - 1.0) * TanX) + U * ((1.0 - 2.0 * S.Y / VY) * TanY);
	if (FMath::Abs(Dir.Z) < 1e-6) return false;
	const double T = (Height - Pos.Z) / Dir.Z;
	Out = FVector2D(Pos.X + Dir.X * T, Pos.Y + Dir.Y * T);
	return true;
}

bool TCSelectionBounds::Project(const FBox& Local, const FTransform& Model, const FTCPinhole& View, double W, double H, FBox2D& Out, double Near, double Far)
{
	Out = FBox2D(ForceInit);
	if (!Local.IsValid || W <= 0 || H <= 0 || Near <= 0 || Far < Near) return false;
	FVector Corners[8]; double Depth[8];
	TArray<FVector> Clipped;
	for (int32 I = 0; I < 8; I++)
	{
		Corners[I] = Model.TransformPosition(FVector(I & 1 ? Local.Max.X : Local.Min.X, I & 2 ? Local.Max.Y : Local.Min.Y, I & 4 ? Local.Max.Z : Local.Min.Z));
		Depth[I] = (Corners[I] - View.Pos) | View.F;
		if (Depth[I] >= Near && Depth[I] <= Far) Clipped.Add(Corners[I]);
	}
	for (int32 A = 0; A < 8; A++) for (int32 Bit : {1,2,4})
	{
		const int32 B = A ^ Bit; if (A > B) continue;
		for (const double Plane : { Near, Far }) if ((Depth[A] < Plane) != (Depth[B] < Plane)) Clipped.Add(FMath::Lerp(Corners[A], Corners[B], (Plane - Depth[A]) / (Depth[B] - Depth[A])));
	}
	for (const FVector P : Clipped) { FVector2D S; if (View.Project(P, W, H, S) && FMath::IsFinite(S.X) && FMath::IsFinite(S.Y)) Out += S; }
	return Out.bIsValid;
}

double TCSelectionBounds::Distance(const FBox2D& B, FVector2D P)
{
	if (!B.bIsValid) return TNumericLimits<double>::Max();
	const double X = FMath::Max3(B.Min.X - P.X, 0., P.X - B.Max.X), Y = FMath::Max3(B.Min.Y - P.Y, 0., P.Y - B.Max.Y);
	return FMath::Sqrt(X * X + Y * Y);
}

bool TCSelectionBounds::Intersects(const FBox2D& A, const FBox2D& B)
{
	return A.bIsValid && B.bIsValid && A.Max.X >= B.Min.X && A.Min.X <= B.Max.X && A.Max.Y >= B.Min.Y && A.Min.Y <= B.Max.Y;
}

FBox TCSelectionBounds::Blend(TArrayView<const FBox> Keys, TArrayView<const float> Data)
{
	if (Data.Num() < 8) return FBox(ForceInit);
	FBox Result(FVector::ZeroVector, FVector::ZeroVector);
	bool bUsed = false;
	for (int32 I = 0; I < 4; I++)
	{
		if (!FMath::IsFinite(Data[I]) || !FMath::IsFinite(Data[I + 4])) return FBox(ForceInit);
		const float Weight = Data[I + 4];
		if (Weight == 0) continue;
		const int32 K = FMath::RoundToInt(Data[I]);
		if (!Keys.IsValidIndex(K) || !Keys[K].IsValid) return FBox(ForceInit);
		Result.Min += (Weight >= 0 ? Keys[K].Min : Keys[K].Max) * Weight;
		Result.Max += (Weight >= 0 ? Keys[K].Max : Keys[K].Min) * Weight;
		bUsed = true;
	}
	return bUsed ? Result : FBox(ForceInit);
}

// The lead patch adds this declaration to the existing renderer history. Definitions stay in this scope.
#include "Client/TCModelLibrary.h"
#include "Client/TCUnitRenderer.h"
#include "Client/TCPlayerController.h"
#include "Client/TCGameMode.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#if defined(TC_HAS_MODEL_SELECTION)
void ATCUnitRenderer::SelectionBounds(int32 Id, const FTCPinhole& View, double W, double H, TArray<FBox2D>& Out) const
{
	History.SelectionBounds(Id, View, W, H, Out);
	if (Crowd)
	{
#if defined(TC_HAS_POSE_SELECTION)
		Crowd->SelectionBounds(Id, View, W, H, Out);
#else
		Crowd->History.SelectionBounds(Id, View, W, H, Out);
#endif
	}
}

bool ATCPlayerController::ModelBounds(const FTCUnit& U, TArray<FBox2D>& Out) const
{
	Out.Reset();
	const ATCGameMode* M = Mode(); int32 W = 0, H = 0; GetViewportSize(W,H);
	if (!M || !M->Units || !PlayerCameraManager || W <= 0 || H <= 0 || (U.Flags & TCFlags::Garrisoned)) return false;
	const FRotator Rotation = PlayerCameraManager->GetCameraRotation();
	FTCPinhole View(PlayerCameraManager->GetCameraLocation(),Rotation.Yaw,-Rotation.Pitch,0,double(W)/H);
	M->Units->SelectionBounds(U.Id,View,W,H,Out);
	return Out.Num() > 0;
}

void FTCInstanceHistory::SelectionBounds(int32 UnitId, const FTCPinhole& View, double W, double H, TArray<FBox2D>& Out) const
{
	for (const auto& Component : Last)
	{
		const auto* Ism = Component.Key.Get();
		if (!Ism || !Ism->IsVisible() || !Ism->GetStaticMesh()) continue;
		for (const auto& Instance : Component.Value)
		{
			const uint64 Key = Instance.Key;
			if ((Key >> 60) != 1 || int32(uint32(Key >> 24)) != UnitId || (Key & 15) > 1) continue;
			FBox2D Screen;
			if (TCSelectionBounds::Project(Ism->GetStaticMesh()->GetBoundingBox(), Instance.Value, View, W, H, Screen)) Out.Add(Screen);
		}
	}
}
#endif

#if defined(TC_HAS_POSE_SELECTION)
namespace TCSelectionPose
{
	struct FVertex { FVector P; float J[4] = {}, W[4] = {}; };
	// Read the existing reproducible GLB payload, including indexed visible sections. This also works in a cook
	// whose static-mesh CPU buffers are stripped. Models/Infantry/Source is already staged as non-asset content.
	const TArray<FVertex>& Source(const FString& Mesh)
	{
		static TMap<FString, TArray<FVertex>> Cache;
		if (const auto* Found = Cache.Find(Mesh)) return *Found;
		auto& Out = Cache.Add(Mesh);
		const bool bBody = Mesh == TEXT("SM_InfantryCrowdBody") || Mesh == TEXT("SM_InfantryCrowdCrown");
		const FString Relative = bBody ? Mesh : TEXT("Kit/") + Mesh.RightChop(15);
		TArray<uint8> Bytes;
		if (!FFileHelper::LoadFileToArray(Bytes, *FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Models/Infantry/Source"), Relative + TEXT(".glb"))) || Bytes.Num() < 28) return Out;
		auto U32 = [&](int32 At) { uint32 V = 0; if (At >= 0 && At + 4 <= Bytes.Num()) FMemory::Memcpy(&V, Bytes.GetData() + At, 4); return V; };
		const uint32 JsonSize = U32(12);
		if (U32(0) != 0x46546c67 || U32(4) != 2 || U32(16) != 0x4e4f534a || JsonSize > uint32(Bytes.Num() - 28)) return Out;
		const int32 Bin = 28 + JsonSize;
		if (U32(24 + JsonSize) != 0x004e4942) return Out;
		const FUTF8ToTCHAR JsonText(reinterpret_cast<const ANSICHAR*>(Bytes.GetData() + 20), JsonSize);
		TSharedPtr<FJsonObject> Json;
		if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(FString(JsonText.Length(), JsonText.Get())), Json)) return Out;
		const auto& Accessors = Json->GetArrayField(TEXT("accessors"));
		const auto& Views = Json->GetArrayField(TEXT("bufferViews"));
		auto Read = [&](int32 A, int32 Count, TArray<float>& Values) -> bool
		{
			if (!Accessors.IsValidIndex(A)) return false;
			const auto O = Accessors[A]->AsObject();
			const int32 B = O->GetIntegerField(TEXT("bufferView"));
			if (!Views.IsValidIndex(B)) return false;
			const auto V = Views[B]->AsObject();
			const int32 Type = O->GetIntegerField(TEXT("componentType")), N = O->GetIntegerField(TEXT("count"));
			const int32 Size = Type == 5126 || Type == 5125 ? 4 : Type == 5123 ? 2 : 0;
			if (!Size || N < 0 || N > 1000000 || Count < 1 || Count > 4) return false;
			double ViewOffset = 0, AccessorOffset = 0, StrideValue = Size * Count;
			V->TryGetNumberField(TEXT("byteOffset"), ViewOffset); O->TryGetNumberField(TEXT("byteOffset"), AccessorOffset); V->TryGetNumberField(TEXT("byteStride"), StrideValue);
			const int64 Start = int64(Bin) + ViewOffset + AccessorOffset;
			const int64 Stride = StrideValue;
			if (Start < Bin || Stride < Size * Count || Start + int64(N) * Stride > Bytes.Num()) return false;
			Values.SetNum(N * Count);
			for (int32 I = 0; I < N; I++) for (int32 C = 0; C < Count; C++)
			{
				const uint8* P = Bytes.GetData() + Start + I * Stride + C * Size;
				if (Type == 5126) FMemory::Memcpy(&Values[I * Count + C], P, 4);
				else if (Type == 5125) { uint32 X; FMemory::Memcpy(&X, P, 4); Values[I * Count + C] = X; }
				else { uint16 X; FMemory::Memcpy(&X, P, 2); Values[I * Count + C] = X; }
			}
			return true;
		};
		const auto Primitive = Json->GetArrayField(TEXT("meshes"))[0]->AsObject()->GetArrayField(TEXT("primitives"))[0]->AsObject();
		const auto Attributes = Primitive->GetObjectField(TEXT("attributes"));
		TArray<float> Positions, Indices, J01, J23, W01, W23;
		if (!Read(Attributes->GetIntegerField(TEXT("POSITION")), 3, Positions) || !Read(Primitive->GetIntegerField(TEXT("indices")), 1, Indices)) return Out;
		if (bBody && (!Read(Attributes->GetIntegerField(TEXT("TEXCOORD_1")), 2, J01) || !Read(Attributes->GetIntegerField(TEXT("TEXCOORD_2")), 2, J23) ||
			!Read(Attributes->GetIntegerField(TEXT("TEXCOORD_3")), 2, W01) || !Read(Attributes->GetIntegerField(TEXT("TEXCOORD_4")), 2, W23))) return Out;
		TArray<bool> Used; Used.Init(false, Positions.Num() / 3);
		for (const float Index : Indices)
		{
			const int32 I = int32(Index);
			if (!Used.IsValidIndex(I)) { Out.Reset(); return Out; }
			if (Used[I]) continue;
			Used[I] = true;
			FVertex Vertex; Vertex.P = FVector(Positions[I * 3], Positions[I * 3 + 2], Positions[I * 3 + 1]) * 100;
			if (bBody)
			{
				if (!J01.IsValidIndex(I * 2 + 1) || !J23.IsValidIndex(I * 2 + 1) || !W01.IsValidIndex(I * 2 + 1) || !W23.IsValidIndex(I * 2 + 1)) { Out.Reset(); return Out; }
				for (int32 C = 0; C < 4; C++) { Vertex.J[C] = (C < 2 ? J01 : J23)[I * 2 + C % 2]; Vertex.W[C] = (C < 2 ? W01 : W23)[I * 2 + C % 2]; }
			}
			Out.Add(Vertex);
		}
		return Out;
	}
	FTransform Pose(TArrayView<const float> D)
	{
		return FTransform(FQuat(FVector(0,1,0), -D[8]), FVector(D[10],0,D[11]), FVector(1,1,D[9] > 0 ? D[9] : 1));
	}
}

void UTCSoldierCrowd::SelectionBounds(int32 UnitId, const FTCPinhole& View, double W, double H, TArray<FBox2D>& Out) const
{
	// Cache only the immutable per-keyframe local envelopes, never a unit's pose or camera projection.
	static TMap<FString, TArray<FBox>> Envelopes;
	for (const auto& Entry : Frame)
	{
		const auto* Ism = Entry.Key;
		const auto& Batch = Entry.Value;
		if (!Ism || !Ism->IsVisible() || !Ism->GetStaticMesh() || Batch.Floats < 12) continue;
		const FNearFigure* Near = nullptr; const FFarShape* Far = nullptr; FString Figure; int32 Bone = -1;
		for (const auto& F : NearFigures)
		{
			for (int32 L = 0; L < 2; L++)
			{
				if (F.Value.Body[L] == Ism || F.Value.Crown[L] == Ism) { Near = &F.Value; Figure = F.Key; }
				for (const auto& K : F.Value.Kit[L]) if (K.Key == Ism) { Near = &F.Value; Figure = F.Key; Bone = int32(K.Value); }
			}
		}
		if (!Near) for (const auto& F : FarFigures) if (F.Value.Draw[0] == Ism || F.Value.Draw[1] == Ism)
		{ Far = FarData.Find(F.Key); Figure = F.Key; break; }
		if (!Near && !Far) continue;
		const FString CacheKey = Figure + TEXT("|") + Ism->GetStaticMesh()->GetName() + FString::FromInt(Bone);
		const TArray<FBox>* Keys = Envelopes.Find(CacheKey);
		for (int32 I = 0; I < Batch.Keys.Num(); I++)
		{
			const uint64 Key = Batch.Keys[I];
			if ((Key >> 60) != 1 || int32(uint32(Key >> 24)) != UnitId || (Key & 15) != 1 || !Batch.Transforms.IsValidIndex(I) || Batch.Data.Num() < (I + 1) * Batch.Floats) continue;
			if (!Keys)
			{
				TArray<FBox> Boxes; Boxes.Init(FBox(ForceInit), UTCInfantryRigLibrary::NumKeys);
				if (Far)
				{
					for (int32 K = 0; K < Boxes.Num(); K++) for (int32 V = 0; V < Far->Vertices; V++)
					{ const float* P = Far->Positions.GetData() + (K * Far->Vertices + V) * 3; Boxes[K] += FVector(P[0],P[2],P[1]) * 100; }
				}
				else
				{
					TArray<FString> Parts; Figure.ParseIntoArray(Parts, TEXT("|"));
					TArray<FMatrix44f> Skin;
					int32 Slot = 0;
					if (Parts.Num() != 3) continue;
					while (Slot < 16 && UTCInfantryRigLibrary::SoldierKit(Parts[0], Slot, FCString::Atoi(*Parts[1])) != Parts[2]) Slot++;
					if (Slot == 16 || !UTCInfantryRigLibrary::BakeKeyframes(Parts[0], FCString::Atoi(*Parts[1]), Slot, Skin)) continue;
					const auto& Vertices = TCSelectionPose::Source(Ism->GetStaticMesh()->GetName());
					const int32 Bones = UTCInfantryRigLibrary::NumBones;
					for (int32 K = 0; K < Boxes.Num(); K++) for (const auto& V : Vertices)
					{
						FVector3f P = FVector3f::ZeroVector;
						for (int32 C = 0; C < (Bone >= 0 ? 1 : 4); C++)
						{
							const int32 B = Bone >= 0 ? Bone : int32(V.J[C]); const float Weight = Bone >= 0 ? 1 : V.W[C];
							if (Weight > 0 && B >= 0 && B < Bones) P += FVector3f(Skin[K * Bones + B].TransformPosition(FVector3f(V.P))) * Weight;
						}
						Boxes[K] += FVector(P);
						}
				}
				Keys = &Envelopes.Add(CacheKey, MoveTemp(Boxes));
			}
			const auto Data = MakeArrayView(Batch.Data).Slice(I * Batch.Floats, 12);
			const FBox Local = TCSelectionBounds::Blend(*Keys, Data);
			FBox2D Screen;
			if (TCSelectionBounds::Project(Local, TCSelectionPose::Pose(Data) * Batch.Transforms[I], View, W, H, Screen)) Out.Add(Screen);
		}
	}
}
#endif

namespace TCCameraFeel
{
	float PanSpeedFactor(int32 Index)
	{
		static const float Speeds[PanSpeedCount] = { 0.6f, 1.f, 1.6f };
		return Speeds[FMath::Clamp(Index, 0, PanSpeedCount - 1)];
	}

	const TCHAR* PanSpeedName(int32 Index)
	{
		static const TCHAR* const Names[PanSpeedCount] = { TEXT("Slow"), TEXT("Normal"), TEXT("Fast") };
		return Names[FMath::Clamp(Index, 0, PanSpeedCount - 1)];
	}

	float Smoothstep(float T)
	{
		T = FMath::Clamp(T, 0.f, 1.f);
		return T * T * (3.f - 2.f * T);
	}

	float EdgeZone(float W, float H)
	{
		return FMath::Min(48.f, FMath::Max(24.f, FMath::Min(W, H) * 0.03f));
	}

	float EdgeDepth(float FromEdge, float Reach, float Zone)
	{
		return FromEdge < Reach ? 0.3f + 0.7f * (1.f - FromEdge / Zone) : 0.f;
	}

	float GlideHeight(float Current, float Ground, float Dt)
	{
		return Current + (Ground - Current) * FMath::Min(1.f, Dt * HeightGlideRate);
	}

	float ZoomedDistance(float Dist, int32 Notch, float Widest)
	{
		return FMath::Min(Widest, FMath::Max(MinDistance, Dist * (1.f + FMath::Sign((float)Notch) * ZoomStep)));
	}

	bool ShiftToPixel(FVector2D Focus, float Height, float Yaw, float Pitch, float Dist, double VX, double VY, const FVector& P, FVector2D Pixel, FVector2D& OutFocus)
	{
		if (VX <= 0 || VY <= 0) return false;
		// client/camera.js shift: translate the ray through the pixel so it meets the anchor at the anchor's height
		const FTCPinhole V(FVector(Focus.X, Focus.Y, Height), Yaw, Pitch, Dist, VX / VY);
		FVector2D Hit;
		if (!V.Ground(Pixel, VX, VY, P.Z, Hit)) return false;
		OutFocus = Focus + FVector2D(P.X, P.Y) - Hit;
		return true;
	}

	float MiddleDragYawDegrees(float DxPixels)
	{
		return FMath::RadiansToDegrees(MiddleDragRadiansPerPixel * DxPixels);
	}
}

void FTCPointerEdge::Frame(bool bFocused, bool bHasPos, FVector2D NewPos, float W, float H, bool bPressed)
{
	// client/pointer.js: a blur (or a hidden tab) forgets the cursor until it moves again
	if (!bFocused) { Reset(); return; }
	const bool bInside = bHasPos && NewPos.X >= 0 && NewPos.Y >= 0 && NewPos.X < W && NewPos.Y < H;
	if (bHasPos && !bInside)
	{
		// a drag held past the edge (the press captured the mouse): pinned where it crossed
		if (Where == ETCPointerWhere::In) Leave(NewPos, true, W, H);
		Last = NewPos; bKnown = true;
		return;
	}
	if (!bHasPos)
	{
		// the window stopped reporting the cursor: it left the window (UE: the viewport's mouse-leave)
		if (Where == ETCPointerWhere::In) Leave(FVector2D::ZeroVector, false, W, H);
		bKnown = false;
		return;
	}
	const bool bMoved = bKnown && !NewPos.Equals(Last, 0.5);
	if (Where == ETCPointerWhere::In)
	{
		if (bMoved) Vel = NewPos - Pos;
		Pos = NewPos;
	}
	else if (bMoved || bPressed || (Where == ETCPointerWhere::Out && !NewPos.Equals(Last, 0.5)) || (Where == ETCPointerWhere::Out && !bKnown))
	{
		// seen moving (or pressing) in the window; or back in after leaving
		Where = ETCPointerWhere::In;
		Vel = FVector2D::ZeroVector;
		Pos = NewPos;
	}
	Last = NewPos;
	bKnown = true;
}

void FTCPointerEdge::Leave(FVector2D Exit, bool bExitKnown, float W, float H)
{
	// client/pointer.js leave: the side it went (the exit point when known), else the way it was last moving
	const double Dx = bExitKnown ? Exit.X - Pos.X : Vel.X, Dy = bExitKnown ? Exit.Y - Pos.Y : Vel.Y;
	double Px = Pos.X, Py = Pos.Y;
	if (Dx != 0 || Dy != 0)
	{
		double T = TNumericLimits<double>::Max();
		if (Dx > 0) T = (W - 1 - Pos.X) / Dx; else if (Dx < 0) T = -Pos.X / Dx;
		if (Dy > 0) T = FMath::Min(T, (H - 1 - Pos.Y) / Dy); else if (Dy < 0) T = FMath::Min(T, -Pos.Y / Dy);
		Px = Pos.X + Dx * T; Py = Pos.Y + Dy * T;
	}
	Px = FMath::Clamp(Px, 0.0, (double)W - 1); Py = FMath::Clamp(Py, 0.0, (double)H - 1);
	const double D[4] = { Px, W - 1 - Px, Py, H - 1 - Py };
	const double M = FMath::Min(FMath::Min(D[0], D[1]), FMath::Min(D[2], D[3]));
	if (M > 0.5)
	{
		if (M == D[0]) Px = 0; else if (M == D[1]) Px = W - 1; else if (M == D[2]) Py = 0; else Py = H - 1;
	}
	Pos = FVector2D(Px, Py);
	Where = ETCPointerWhere::Out;
}

FTCEdgePush FTCEdgeScroll::Step(const FTCEdgeInput& In, float Dt, bool bFollowing)
{
	FTCEdgePush Out;
	if (!In.bEnabled || !In.bActive || !In.bWorld || In.bBlocked || In.bButtons || In.bDragging || In.bMiddle || In.W <= 0 || In.H <= 0)
	{
		Reset();
		return Out;
	}
	// Over a HUD panel only the outer strip counts, so a button near the edge does not scroll the view while aimed at
	const float Zone = TCCameraFeel::EdgeZone(In.W, In.H);
	const float Reach = In.bOut || In.bOverView ? Zone : FMath::Max(6.f, Zone / 4.f);
	auto Depth = [&](float D) { return TCCameraFeel::EdgeDepth(D, Reach, Zone); };
	const float Rt = Depth(In.W - 1 - In.Pos.X) - Depth(In.Pos.X);
	const float Fw = Depth(In.Pos.Y) - Depth(In.H - 1 - In.Pos.Y);
	if (!Rt && !Fw) { Reset(); return Out; }
	if (bHold && bFollowing) return Out;
	bHold = false;
	Age += Dt;
	const float Ease = TCCameraFeel::Smoothstep(FMath::Min(1.f, Age / TCCameraFeel::EdgeEaseSeconds));
	Out.bPush = true;
	Out.Fw = Fw * Ease;
	Out.Rt = Rt * Ease;
	Out.Dir = FString(Fw > 0 ? TEXT("n") : Fw < 0 ? TEXT("s") : TEXT("")) + (Rt > 0 ? TEXT("e") : Rt < 0 ? TEXT("w") : TEXT(""));
	return Out;
}

bool FTCCameraIntro::Step(float Dt, FTCCameraPose& Out)
{
	if (!bActive) { Out = ToPose; return false; }
	Age = FMath::Min(TCCameraFeel::IntroSeconds, Age + Dt);
	const float T = Age / TCCameraFeel::IntroSeconds, E = TCCameraFeel::Smoothstep(T);
	if (T >= 1.f) { bActive = false; Out = ToPose; return false; }
	Out.Focus = FromPose.Focus + (ToPose.Focus - FromPose.Focus) * E;
	Out.Height = FromPose.Height + (ToPose.Height - FromPose.Height) * E;
	Out.Dist = FromPose.Dist + (ToPose.Dist - FromPose.Dist) * E;
	return true;
}

bool FTCCameraIntro::Skip(FTCCameraPose& Out)
{
	if (!bActive) return false;
	bActive = false;
	Out = ToPose;
	return true;
}

#if WITH_DEV_AUTOMATION_TESTS

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSelectionBoundsTest, "ThreeCrossroads.Camera.SelectionBounds", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCSelectionBoundsTest::RunTest(const FString&)
{
	const FTCPinhole View(FVector::ZeroVector, 0, 0, 0, 16./9.);
	FBox2D Left, Right, Clipped;
	TestTrue(TEXT("left soldier mesh projects separately"), TCSelectionBounds::Project(FBox(FVector(100,-40,-5),FVector(110,-30,5)), FTransform::Identity, View, 1280, 720, Left));
	TestTrue(TEXT("right soldier mesh projects separately"), TCSelectionBounds::Project(FBox(FVector(100,30,-5),FVector(110,40,5)), FTransform::Identity, View, 1280, 720, Right));
	TestTrue(TEXT("gap between soldiers remains empty"), TCSelectionBounds::Distance(Left, {640,360}) > 6 && TCSelectionBounds::Distance(Right, {640,360}) > 6);
	TestFalse(TEXT("box wholly in the gap selects neither model"), TCSelectionBounds::Intersects(Left, FBox2D({635,355},{645,365})) || TCSelectionBounds::Intersects(Right, FBox2D({635,355},{645,365})));
	TestEqual(TEXT("click within mesh has zero distance"), TCSelectionBounds::Distance(Left, Left.GetCenter()), 0.);
	TestTrue(TEXT("touching box edge counts"), TCSelectionBounds::Intersects(Left, FBox2D(Left.Max, Left.Max + FVector2D(5,5))));
	TestTrue(TEXT("near-plane intersections survive"), TCSelectionBounds::Project(FBox(FVector(-10,-2,-2),FVector(20,2,2)), FTransform::Identity, View,1280,720,Clipped));
	TestFalse(TEXT("mesh behind near plane is absent"), TCSelectionBounds::Project(FBox(FVector(-20,-2,-2),FVector(5,2,2)), FTransform::Identity,View,1280,720,Clipped));
	TestFalse(TEXT("mesh beyond far plane is absent"), TCSelectionBounds::Project(FBox(FVector(100,-2,-2),FVector(120,2,2)), FTransform::Identity,View,1280,720,Clipped,10,90));
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCMouseCaptureTest, "ThreeCrossroads.Camera.Capture", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCMouseCaptureTest::RunTest(const FString&)
{
	FTCMouseCapture C;
	TestFalse(TEXT("an offscreen viewport has no native focus"), FTCMouseCapture::WindowFocused(nullptr));
	TestEqual(TEXT("unknown stored mode uses source default"), FTCMouseCapture::ValidMode(TEXT("bad")), FString(TEXT("fullscreen")));
	TestTrue(TEXT("all client edges confine external warps"), FTCMouseCapture::ConfinedPosition(FVector2D(-30,750),FIntPoint(1280,720)) == FVector2D(0,719));
	TestTrue(TEXT("right/top edges confine external warps"), FTCMouseCapture::ConfinedPosition(FVector2D(1400,-10),FIntPoint(1280,720)) == FVector2D(1279,0));
	TestTrue(TEXT("an interior cursor stays exactly where it was"), FTCMouseCapture::ConfinedPosition(FVector2D(125.5,215.25),FIntPoint(1280,720)) == FVector2D(125.5,215.25));
	TestFalse(TEXT("default mode leaves windowed pointer free"), C.Frame(true, false, true, true, false));
	TestTrue(TEXT("fullscreen click captures"), C.Frame(true, true, true, true, false));
	TestFalse(TEXT("Esc releases"), C.Frame(true, true, true, false, true));
	TestFalse(TEXT("next click does not undo Esc suspension"), C.Frame(true, true, true, true, false));
	C.SetMode(TEXT("always"));
	TestTrue(TEXT("Always captures a windowed click"), C.Frame(true, false, true, true, false));
	TestFalse(TEXT("blur releases confinement"), C.Frame(true, false, false, false, false));
	TestFalse(TEXT("focus without activation remains free"), C.Frame(true, false, true, false, false));
	TestTrue(TEXT("fresh click recaptures"), C.Frame(true, false, true, true, false));
	C.SetMode(TEXT("off"));
	TestFalse(TEXT("Off releases and does not auto capture"), C.Frame(true, true, true, true, false));
	C.ToggleNow(true);
	TestTrue(TEXT("explicit Capture mouse works even with automatic capture Off"), C.Frame(true, false, true, false, false));
	TestFalse(TEXT("lobby releases"), C.Frame(false, false, true, false, false));
	C.SetMode(TEXT("always")); C.Frame(true, false, true, true, true);
	C.Frame(false, false, true, false, false);
	TestTrue(TEXT("a new match resets Esc suspension"), C.Frame(true, false, true, true, false));
	return true;
}

#if defined(TC_HAS_POSE_SELECTION)
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCSelectionPoseTest, "ThreeCrossroads.Camera.SelectionPose", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCSelectionPoseTest::RunTest(const FString&)
{
	const TArray<FBox> Keys = { FBox(FVector(0,0,0), FVector(10,20,30)), FBox(FVector(-20,-10,-5), FVector(30,5,8)) };
	const TArray<float> Weights = {0,1,0,0, .25f,.75f,0,0};
	const FBox Mixed = TCSelectionBounds::Blend(Keys, Weights);
	TestTrue(TEXT("absolute morph envelope follows the rendered key weights"), Mixed.Min.Equals(FVector(-15,-7.5,-3.75)) && Mixed.Max.Equals(FVector(25,8.75,13.5)));
	const TArray<float> Invalid = {0,99,0,0, 0,1,0,0};
	TestFalse(TEXT("unknown key does not produce a rest-pose selection target"), TCSelectionBounds::Blend(Keys, Invalid).IsValid != 0);
	auto* Crowd = NewObject<UTCSoldierCrowd>(); FString Error;
	if (!TestTrue(TEXT("original near/far payloads load"), Crowd->Load(Error))) { AddError(Error); return false; }
	const FTCPinhole View(FVector(0,0,60), 90, 0, 500, 1280.0/720);
	for (const bool bFar : {false,true})
	{
		TArray<FBox2D> Boxes[2];
		for (int32 Posture = 0; Posture < 2; Posture++)
		{
			Crowd->BeginFrame();
			FTCSoldierDraw Man; Man.UnitId = 7; Man.ManIndex = 0; Man.FigureType = TEXT("rifle"); Man.Man = FTransform::Identity; Man.bFar = bFar;
			FTCInfantryMotion Motion; Motion.PostureWeights = Posture ? FVector4(0,0,1,0) : FVector4(1,0,0,0);
			Man.Blend = UTCInfantryRigLibrary::BlendFor(Man.FigureType, Motion);
			TestTrue(TEXT("the actual native soldier draw is available"), Crowd->AddMan(Man));
			Crowd->SelectionBounds(7, View, 1280,720,Boxes[Posture]);
			TestTrue(TEXT("drawn posture supplies a projected selection envelope"), Boxes[Posture].Num() > 0);
			TArray<FBox2D> Absent; Crowd->SelectionBounds(8,View,1280,720,Absent);
			TestEqual(TEXT("another unit has no envelope"), Absent.Num(), 0);
		}
		FBox2D Union[2] = { FBox2D(ForceInit),FBox2D(ForceInit) };
		for (int32 P = 0; P < 2; P++) for (const auto& B : Boxes[P]) { Union[P] += B.Min; Union[P] += B.Max; }
		TestTrue(TEXT("prone picking follows the low drawn figure rather than its standing mesh"), Union[0].bIsValid && Union[1].bIsValid && Union[1].GetSize().Y < Union[0].GetSize().Y * .75);
	}
	return true;
}
#endif

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCCameraProjectionTest, "ThreeCrossroads.Camera.Projection", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCCameraProjectionTest::RunTest(const FString&)
{
	// Exercise the engine's real camera-component view, not just the analytic fit camera. A wrong axis or FOV here
	// makes equal zoom distances appear closer even when the camera.js wheel and fit calculations agree.
	const UCameraComponent* Camera = GetDefault<ATCCameraPawn>()->Camera;
	for (const FIntPoint Size : { FIntPoint(1280,720), FIntPoint(720,1280), FIntPoint(1920,1080) })
	{
		FMinimalViewInfo View;
		const_cast<UCameraComponent*>(Camera)->GetCameraView(0, View);
		FSceneViewProjectionData Data;
		const FIntRect Rect(0, 0, Size.X, Size.Y);
		Data.SetViewRectangle(Rect);
		FMinimalViewInfo::CalculateProjectionMatrixGivenViewRectangle(View, AspectRatio_MaintainXFOV, Rect, Data);
		const double Y = 1.0 / FMath::Tan(FMath::DegreesToRadians(42.0 / 2));
		TestEqual(TEXT("actual engine projection keeps the browser's vertical FOV"), Data.ProjectionMatrix.M[1][1], Y, 1e-5);
		TestEqual(TEXT("horizontal projection follows viewport aspect"), Data.ProjectionMatrix.M[0][0], Y * Size.Y / Size.X, 1e-5);
	}
	return true;
}

// ThreeCrossroads.Camera.Feel: client/camera.js's numbers and rules, ported. Run:
//   UnrealEditor-Cmd ThreeCrossroads.uproject -nullrhi -ExecCmds="Automation RunTests ThreeCrossroads.Camera; Quit"
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCCameraFeelTest, "ThreeCrossroads.Camera.Feel", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCCameraFeelTest::RunTest(const FString& Parameters)
{
	using namespace TCCameraFeel;
	// edgeZone: 3% of the short side within [24, 48] (32 px at 1920 x 1080, as camera.js's comment says, 32.4 exactly)
	TestEqual(TEXT("zone at 1920 x 1080"), EdgeZone(1920, 1080), 32.4f, 1e-4f);
	TestEqual(TEXT("zone floor"), EdgeZone(800, 600), 24.f);
	TestEqual(TEXT("zone ceiling"), EdgeZone(4000, 3000), 48.f);
	TestEqual(TEXT("full speed at the edge"), EdgeDepth(0, 32, 32), 1.f);
	TestEqual(TEXT("30% just inside the zone's inner border"), EdgeDepth(31.999f, 32, 32), 0.3f, 1e-3f);
	TestEqual(TEXT("nothing at the inner border"), EdgeDepth(32, 32, 32), 0.f);

	FTCEdgeInput In;
	In.bActive = In.bWorld = true;
	In.W = 1920; In.H = 1080;
	In.Pos = FVector2D(0, 540);
	FTCEdgeScroll E;
	FTCEdgePush P = E.Step(In, EdgeEaseSeconds / 2, false);
	TestTrue(TEXT("the left edge pushes"), P.bPush);
	TestEqual(TEXT("eased in: half the ease time gives half the speed"), P.Rt, -0.5f, 1e-4f);
	TestEqual(TEXT("left only"), P.Fw, 0.f);
	TestEqual(TEXT("direction west"), P.Dir, FString(TEXT("w")));
	P = E.Step(In, EdgeEaseSeconds / 2, false);
	TestEqual(TEXT("full speed once eased in"), P.Rt, -1.f, 1e-4f);
	In.Pos = FVector2D(1919, 0);
	P = E.Step(In, 1.f, false);
	TestTrue(TEXT("a corner pushes both ways"), P.Fw > 0.99f && P.Rt > 0.99f);
	TestEqual(TEXT("direction north-east"), P.Dir, FString(TEXT("ne")));
	// over a HUD panel only the outer strip counts: 10 px in is inside the 32.4 px zone but past its quarter (8.1 px)
	In.Pos = FVector2D(10, 540);
	In.bOverView = false;
	TestFalse(TEXT("over a panel, 10 px in does not push"), E.Step(In, 1.f, false).bPush);
	In.bOverView = true;
	P = E.Step(In, EdgeEaseSeconds / 2, false);
	TestTrue(TEXT("over the view it does"), P.bPush);
	TestEqual(TEXT("the ease restarted after the pause, on the ramp's depth 10 px in"), P.Rt, -0.5f * (0.3f + 0.7f * (1.f - 10.f / 32.4f)), 1e-4f);
	for (bool FTCEdgeInput::*Flag : { &FTCEdgeInput::bBlocked, &FTCEdgeInput::bButtons, &FTCEdgeInput::bDragging, &FTCEdgeInput::bMiddle })
	{
		FTCEdgeInput Stop = In;
		Stop.*Flag = true;
		TestFalse(TEXT("no push while blocked, a button is held, a drag or a rotation runs"), E.Step(Stop, 1.f, false).bPush);
	}
	FTCEdgeInput Off = In;
	Off.bEnabled = false;
	TestFalse(TEXT("the switch turns it off"), E.Step(Off, 1.f, false).bPush);
	// a follow that starts with the cursor in the zone waits until it leaves the zone
	E.HoldForFollow();
	TestFalse(TEXT("held while following"), E.Step(In, 1.f, true).bPush);
	FTCEdgeInput Middle = In;
	Middle.Pos = FVector2D(960, 540);
	TestFalse(TEXT("the middle of the window"), E.Step(Middle, 1.f, true).bPush);
	TestTrue(TEXT("a fresh push after that"), E.Step(In, 1.f, true).bPush);

	// client/pointer.js: none until seen moving, then in; leaving pins it to the border it was moving toward
	FTCPointerEdge Ptr;
	Ptr.Frame(true, true, FVector2D(900, 500), 1920, 1080);
	TestFalse(TEXT("a cursor never seen moving is not known"), Ptr.Active());
	Ptr.Frame(true, true, FVector2D(1900, 520), 1920, 1080);
	TestTrue(TEXT("seen moving: in"), Ptr.Where == ETCPointerWhere::In);
	Ptr.Frame(true, true, FVector2D(1910, 520), 1920, 1080);
	Ptr.Frame(true, false, FVector2D::ZeroVector, 1920, 1080);
	TestTrue(TEXT("left the window: out"), Ptr.Out());
	TestEqual(TEXT("pinned on the right border"), Ptr.Pos.X, 1919.0);
	TestEqual(TEXT("at the height it crossed"), Ptr.Pos.Y, 520.0);
	FTCEdgeInput Pinned;
	Pinned.bActive = Ptr.Active(); Pinned.bOut = Ptr.Out(); Pinned.bWorld = true; Pinned.Pos = Ptr.Pos; Pinned.W = 1920; Pinned.H = 1080;
	FTCEdgeScroll E2;
	TestEqual(TEXT("the pinned cursor keeps pushing right"), E2.Step(Pinned, 1.f, false).Rt, 1.f, 1e-4f);
	Ptr.Frame(true, true, FVector2D(1800, 600), 1920, 1080);
	TestTrue(TEXT("back in"), Ptr.Where == ETCPointerWhere::In);
	Ptr.Frame(false, true, FVector2D(1800, 600), 1920, 1080);
	TestFalse(TEXT("a blur forgets it"), Ptr.Active());
	Ptr.Frame(true, true, FVector2D(1800, 600), 1920, 1080);
	Ptr.Frame(true, true, FVector2D(1800, 600), 1920, 1080, true);
	TestTrue(TEXT("a press counts as seen"), Ptr.Active());
	Ptr.Frame(true, true, FVector2D(-40, 600), 1920, 1080);
	TestTrue(TEXT("a drag held past the left edge is out"), Ptr.Out() && Ptr.Pos.X == 0.0);

	// the intro: 2.5 s, eased; the halfway point is halfway; then the start view, and Skip does nothing more
	FTCCameraIntro Intro;
	FTCCameraPose From, To, Now;
	From.Focus = FVector2D(80, 80); From.Height = 0; From.Dist = 300;
	To.Focus = FVector2D(20, 140); To.Height = 5; To.Dist = 60;
	Intro.Begin(From, To);
	TestTrue(TEXT("gliding"), Intro.Step(IntroSeconds / 2, Now));
	TestEqual(TEXT("halfway distance"), Now.Dist, 180.f, 1e-3f);
	TestTrue(TEXT("halfway target"), Now.Focus.Equals(FVector2D(50, 110), 1e-3));
	TestEqual(TEXT("a quarter in is eased (15.6%)"), Smoothstep(0.25f), 0.15625f, 1e-6f);
	TestFalse(TEXT("done after 2.5 s"), Intro.Step(IntroSeconds / 2, Now));
	TestTrue(TEXT("at the start view"), Now.Focus.Equals(To.Focus) && Now.Dist == To.Dist && Now.Height == To.Height);
	TestFalse(TEXT("nothing to skip"), Intro.Skip(Now));
	Intro.Begin(From, To);
	TestTrue(TEXT("a key skips it"), Intro.Skip(Now) && Now.Dist == To.Dist && !Intro.IsActive());

	// the wheel: 10% a notch, 25 m minimum, the widest view maximum
	TestEqual(TEXT("out"), ZoomedDistance(100, 1, 400), 110.f, 1e-4f);
	TestEqual(TEXT("in"), ZoomedDistance(100, -1, 400), 90.f, 1e-4f);
	TestEqual(TEXT("25 m floor"), ZoomedDistance(26, -1, 400), 25.f);
	TestEqual(TEXT("widest ceiling"), ZoomedDistance(390, 1, 400), 400.f);
	TestEqual(TEXT("height glide, one frame at 60 fps"), GlideHeight(0, 10, 1.f / 60), 10.f * 4 / 60, 1e-4f);
	TestEqual(TEXT("height glide never overshoots"), GlideHeight(0, 10, 1.f), 10.f);
	TestEqual(TEXT("middle-drag: 100 px turn 0.6 rad"), MiddleDragYawDegrees(100), FMath::RadiansToDegrees(0.6f), 1e-4f);
	TestEqual(TEXT("pan speeds"), PanSpeedFactor(0) + PanSpeedFactor(1) + PanSpeedFactor(2), 3.2f, 1e-5f);

	// zoom at the cursor: after the shift the pixel's ray meets the anchor, at several poses and pixels
	const float Pitch = 54.43099f;
	const FVector Anchor(37, 52, 3.5);
	for (const FVector2D Pixel : { FVector2D(100, 200), FVector2D(1820, 1000), FVector2D(960, 540) })
		for (const float Yaw : { -90.f, 10.f, 145.f })
		{
			FVector2D Focus;
			TestTrue(TEXT("the shift is found"), ShiftToPixel(FVector2D(60, 60), 2.f, Yaw, Pitch, 75.f, 1920, 1080, Anchor, Pixel, Focus));
			const FTCPinhole V(FVector(Focus.X, Focus.Y, 2.f), Yaw, Pitch, 75.f, 1920.0 / 1080.0);
			FVector2D Hit, Back;
			TestTrue(TEXT("the ray meets the anchor's height"), V.Ground(Pixel, 1920, 1080, Anchor.Z, Hit));
			TestTrue(TEXT("the pixel looks at the anchor"), Hit.Equals(FVector2D(Anchor.X, Anchor.Y), 1e-3));
			TestTrue(TEXT("and the anchor projects back to the pixel"), V.Project(Anchor, 1920, 1080, Back) && Back.Equals(Pixel, 1e-2));
		}
	return true;
}

#endif
