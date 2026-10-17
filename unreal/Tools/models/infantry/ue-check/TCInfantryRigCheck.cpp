// Verification only (see TCInfantryRigCheck.h). Checks the imported /Game/Models/Infantry assets against the reference:
//  1. SK_Infantry's skeleton: the 31 bone names, a flat hierarchy under root, deform bones at the Blender rest frames;
//  2. SK_Infantry posed by UTCInfantryAnimInstance on a registered USkeletalMeshComponent, skinned on the CPU from
//     the imported LOD model, against the reference vertices of oracle/dump-vertices.mjs;
//  3. the crowd path: BakeKeyframeTexture's texels and CrowdCustomData fed through a C++ copy of M_InfantryCrowd's
//     skinning code over SM_InfantryCrowdBody's UV channels, against the same reference vertices;
//  4. the sockets and material slots the runtime relies on.
// Usage: UnrealEditor-Cmd <scratch>.uproject -run=TCInfantryRigCheck -Vertices=<vertices.json> -Out=<report.json>
#include "TCInfantryRigCheck.h"
#include "Client/TCInfantryRig.h"
#include "Animation/AnimInstance.h"
#include "Components/SkeletalMeshComponent.h"
#include "Dom/JsonObject.h"
#include "Engine/Engine.h"
#include "Engine/SkeletalMesh.h"
#include "Engine/SkeletalMeshSocket.h"
#include "Engine/StaticMesh.h"
#include "Engine/Texture2D.h"
#include "Engine/World.h"
#include "Misc/FileHelper.h"
#include "Rendering/SkeletalMeshLODModel.h"
#include "Rendering/SkeletalMeshModel.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "StaticMeshAttributes.h"
#include "MeshDescription.h"

namespace
{
	// Unreal centimetres (x, y, z) back to reference metres (x, z, y)
	FVector ToRef(const FVector& P) { return FVector(P.X / 100.0, P.Z / 100.0, P.Y / 100.0); }

	// payload vertex index for a bind position, through a 1 mm grid
	struct FBindIndex
	{
		TMultiMap<FIntVector, int32> Cells;
		TArray<FVector> Bind;
		static FIntVector Cell(const FVector& P) { return FIntVector(FMath::FloorToInt(P.X * 1000.0), FMath::FloorToInt(P.Y * 1000.0), FMath::FloorToInt(P.Z * 1000.0)); }
		void Build(const TArray<FVector>& Points)
		{
			Bind = Points;
			for (int32 I = 0; I < Points.Num(); I++) Cells.Add(Cell(Points[I]), I);
		}
		int32 Find(const FVector& P, double& OutDistance) const
		{
			const FIntVector C = Cell(P);
			int32 Best = INDEX_NONE;
			OutDistance = TNumericLimits<double>::Max();
			for (int32 X = -1; X <= 1; X++) for (int32 Y = -1; Y <= 1; Y++) for (int32 Z = -1; Z <= 1; Z++)
			{
				TArray<int32> Found;
				Cells.MultiFind(C + FIntVector(X, Y, Z), Found);
				for (int32 I : Found) { const double D = FVector::Dist(Bind[I], P); if (D < OutDistance) { OutDistance = D; Best = I; } }
			}
			return Best;
		}
	};

	struct FCase
	{
		FString Name, Type;
		int32 Fac = 0, Slot = 0;
		FTCInfantryMotion Motion;
		TArray<FVector> Positions;
	};
}

int32 UTCInfantryRigCheckCommandlet::Main(const FString& Params)
{
	FString VerticesPath, OutPath;
	FParse::Value(*Params, TEXT("Vertices="), VerticesPath);
	FParse::Value(*Params, TEXT("Out="), OutPath);
	TSharedRef<FJsonObject> Report = MakeShared<FJsonObject>();
	TArray<TSharedPtr<FJsonValue>> Failures;
	auto Fail = [&Failures](const FString& What) { UE_LOG(LogTemp, Error, TEXT("CHECK FAIL %s"), *What); Failures.Add(MakeShared<FJsonValueString>(What)); };

	// ------------------------------------------------------------------ the reference cases
	FString Text;
	TSharedPtr<FJsonObject> Root;
	if (!FFileHelper::LoadFileToString(Text, *VerticesPath) || !FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root) || !Root.IsValid())
	{
		UE_LOG(LogTemp, Error, TEXT("CHECK cannot read %s"), *VerticesPath);
		return 1;
	}
	TArray<FCase> Cases;
	TArray<FVector> Bind;
	{
		const TArray<TSharedPtr<FJsonValue>>& B = Root->GetArrayField(TEXT("bind"));
		for (int32 I = 0; I + 2 < B.Num(); I += 3) Bind.Add(FVector(B[I]->AsNumber(), B[I + 1]->AsNumber(), B[I + 2]->AsNumber()));
	}
	for (const TSharedPtr<FJsonValue>& V : Root->GetArrayField(TEXT("cases")))
	{
		const TSharedPtr<FJsonObject>& O = V->AsObject();
		FCase C;
		C.Name = O->GetStringField(TEXT("name"));
		C.Type = O->GetStringField(TEXT("type"));
		C.Fac = static_cast<int32>(O->GetNumberField(TEXT("fac")));
		C.Slot = static_cast<int32>(O->GetNumberField(TEXT("slot")));
		const TSharedPtr<FJsonObject>& M = O->GetObjectField(TEXT("motion"));
		const TArray<TSharedPtr<FJsonValue>>& W = M->GetArrayField(TEXT("w"));
		C.Motion.PostureWeights = FVector4(W[0]->AsNumber(), W[1]->AsNumber(), W[2]->AsNumber(), W[3]->AsNumber());
		C.Motion.Moving = static_cast<float>(M->GetNumberField(TEXT("moving")));
		C.Motion.Aim = static_cast<float>(M->GetNumberField(TEXT("aim")));
		C.Motion.Phase = static_cast<float>(M->GetNumberField(TEXT("phase")));
		C.Motion.Work = static_cast<float>(M->GetNumberField(TEXT("work")));
		C.Motion.WorkPhase = static_cast<float>(M->GetNumberField(TEXT("workPhase")));
		C.Motion.bFallen = M->GetBoolField(TEXT("fallen"));
		const TArray<TSharedPtr<FJsonValue>>& P = O->GetArrayField(TEXT("positions"));
		for (int32 I = 0; I + 2 < P.Num(); I += 3) C.Positions.Add(FVector(P[I]->AsNumber(), P[I + 1]->AsNumber(), P[I + 2]->AsNumber()));
		Cases.Add(MoveTemp(C));
	}
	TArray<FVector2D> PayloadUV;
	{
		const TArray<TSharedPtr<FJsonValue>>& T = Root->GetArrayField(TEXT("uv"));
		for (int32 I = 0; I + 1 < T.Num(); I += 2) PayloadUV.Add(FVector2D(T[I]->AsNumber(), T[I + 1]->AsNumber()));
	}
	FBindIndex Index;
	Index.Build(Bind);

	// ------------------------------------------------------------------ 1. the skeleton
	USkeletalMesh* Mesh = LoadObject<USkeletalMesh>(nullptr, TEXT("/Game/Models/Infantry/SK_Infantry.SK_Infantry"));
	if (!Mesh) { UE_LOG(LogTemp, Error, TEXT("CHECK SK_Infantry missing")); return 1; }
	const FReferenceSkeleton& Ref = Mesh->GetRefSkeleton();
	const TArray<FName> Names = UTCInfantryRigLibrary::BoneNames();
	TArray<TSharedPtr<FJsonValue>> BoneList;
	for (int32 I = 0; I < Ref.GetNum(); I++)
	{
		const int32 Parent = Ref.GetParentIndex(I);
		BoneList.Add(MakeShared<FJsonValueString>(FString::Printf(TEXT("%s<-%s"), *Ref.GetBoneName(I).ToString(), Parent >= 0 ? *Ref.GetBoneName(Parent).ToString() : TEXT("-"))));
	}
	Report->SetArrayField(TEXT("importedBones"), BoneList);
	for (const FName& Name : Names) if (Ref.FindBoneIndex(Name) == INDEX_NONE) Fail(FString::Printf(TEXT("bone %s missing"), *Name.ToString()));
	TArray<FTransform> RefComponent;
	RefComponent.SetNum(Ref.GetNum());
	for (int32 I = 0; I < Ref.GetNum(); I++)
	{
		const int32 Parent = Ref.GetParentIndex(I);
		RefComponent[I] = Parent >= 0 ? Ref.GetRefBonePose()[I] * RefComponent[Parent] : Ref.GetRefBonePose()[I];
	}
	{
		// rest positions from infantry-authored-data.js (reference metres), mapped to Unreal centimetres
		struct { const TCHAR* Bone; FVector At; } Expect[] = { { TEXT("pelvis"), FVector(0, 0.7, 0) }, { TEXT("head"), FVector(0.085, 1.27, 0.008) },
			{ TEXT("hand.R"), FVector(0.081, 0.72, 0.331) }, { TEXT("foot.L"), FVector(-0.145, 0.062, -0.145) } };
		double Worst = 0;
		for (const auto& E : Expect)
		{
			const int32 I = Ref.FindBoneIndex(E.Bone);
			if (I == INDEX_NONE) continue;
			const FVector Want(E.At.X * 100.0, E.At.Z * 100.0, E.At.Y * 100.0);
			Worst = FMath::Max(Worst, FVector::Dist(Want, RefComponent[I].GetTranslation()));
		}
		Report->SetNumberField(TEXT("restPositionMaxErrorCm"), Worst);
		if (Worst > 0.01) Fail(FString::Printf(TEXT("rest positions off by %f cm"), Worst));
	}

	// ------------------------------------------------------------------ sockets and slots
	{
		TArray<TSharedPtr<FJsonValue>> Sockets;
		for (int32 I = 0; I < Mesh->NumSockets(); I++)
		{
			const USkeletalMeshSocket* S = Mesh->GetSocketByIndex(I);
			Sockets.Add(MakeShared<FJsonValueString>(FString::Printf(TEXT("%s@%s"), *S->SocketName.ToString(), *S->BoneName.ToString())));
			if (S->SocketName != S->BoneName) Fail(FString::Printf(TEXT("socket %s is on bone %s"), *S->SocketName.ToString(), *S->BoneName.ToString()));
		}
		Report->SetArrayField(TEXT("sockets"), Sockets);
		if (Mesh->NumSockets() != 15) Fail(TEXT("expected 15 kit sockets"));
	}

	// ------------------------------------------------------------------ the imported skin, for CPU skinning
	const FSkeletalMeshLODModel& Model = Mesh->GetImportedModel()->LODModels[0];
	struct FSkinVertex { FVector Bind; int32 Bones[MAX_TOTAL_INFLUENCES]; float Weights[MAX_TOTAL_INFLUENCES]; int32 Payload; };
	TArray<FSkinVertex> Skin;
	double WorstBindMatch = 0, WorstUV = 0;
	for (const FSkelMeshSection& Section : Model.Sections)
	{
		for (const FSoftSkinVertex& V : Section.SoftVertices)
		{
			FSkinVertex S;
			S.Bind = FVector(V.Position);
			for (int32 K = 0; K < MAX_TOTAL_INFLUENCES; K++)
			{
				S.Bones[K] = V.InfluenceWeights[K] ? Section.BoneMap[V.InfluenceBones[K]] : 0;
				S.Weights[K] = V.InfluenceWeights[K] / 65535.f;
			}
			double D = 0;
			S.Payload = Index.Find(ToRef(S.Bind), D);
			WorstBindMatch = FMath::Max(WorstBindMatch, D);
			// the atlas coordinate: the nearest payload vertex with this position and the same UV (seams duplicate
			// positions with different UVs), so the texture is sampled where infantry-material.js samples it
			{
				double BestUV = TNumericLimits<double>::Max();
				for (int32 P = 0; P < Bind.Num(); P++)
					if (FVector::Dist(Bind[P], Bind[S.Payload]) < 1e-6) BestUV = FMath::Min(BestUV, FVector2D::Distance(FVector2D(V.UVs[0]), PayloadUV[P]));
				WorstUV = FMath::Max(WorstUV, BestUV);
			}
			Skin.Add(S);
		}
	}
	Report->SetNumberField(TEXT("importedVertices"), Skin.Num());
	Report->SetNumberField(TEXT("bindMatchMaxMetres"), WorstBindMatch);
	Report->SetNumberField(TEXT("atlasUVMaxError"), WorstUV);
	if (WorstUV > 1e-5) Fail(FString::Printf(TEXT("imported atlas UVs differ from the payload by %g"), WorstUV));
	if (WorstBindMatch > 1e-5) Fail(FString::Printf(TEXT("imported bind positions differ from the payload by %g m"), WorstBindMatch));

	auto SkinnedError = [&](const TArray<FTransform>& Component, const FCase& C, double& OutWorst)
	{
		OutWorst = 0;
		for (const FSkinVertex& V : Skin)
		{
			FVector P = FVector::ZeroVector;
			for (int32 K = 0; K < MAX_TOTAL_INFLUENCES; K++)
			{
				if (V.Weights[K] <= 0.f) continue;
				const FVector Local = RefComponent[V.Bones[K]].InverseTransformPosition(V.Bind);
				P += Component[V.Bones[K]].TransformPosition(Local) * V.Weights[K];
			}
			OutWorst = FMath::Max(OutWorst, FVector::Dist(ToRef(P), C.Positions[V.Payload]));
		}
	};

	// ------------------------------------------------------------------ 2. the anim instance on a live component
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("InfantryRigCheck"));
	FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
	Context.SetCurrentWorld(World);
	USkeletalMeshComponent* Component = NewObject<USkeletalMeshComponent>(World->GetWorldSettings());
	Component->SetSkeletalMesh(Mesh);
	Component->VisibilityBasedAnimTickOption = EVisibilityBasedAnimTickOption::AlwaysTickPoseAndRefreshBones;
	Component->bEnableUpdateRateOptimizations = false;
	Component->SetAnimationMode(EAnimationMode::AnimationBlueprint);
	Component->SetAnimInstanceClass(UTCInfantryAnimInstance::StaticClass());
	Component->RegisterComponentWithWorld(World);
	UTCInfantryAnimInstance* Anim = Cast<UTCInfantryAnimInstance>(Component->GetAnimInstance());
	if (!Anim) Fail(TEXT("the component did not create a UTCInfantryAnimInstance"));

	TArray<TSharedPtr<FJsonValue>> CaseReports;
	for (const FCase& C : Cases)
	{
		TSharedRef<FJsonObject> R = MakeShared<FJsonObject>();
		R->SetStringField(TEXT("name"), C.Name);
		R->SetStringField(TEXT("job"), UTCInfantryRigLibrary::SoldierKit(C.Type, C.Slot, C.Fac));
		const bool bExactKey = !C.Name.Contains(TEXT("half way")) && !C.Name.Contains(TEXT("walking with aim"));
		R->SetBoolField(TEXT("singleKeyframe"), bExactKey);
		if (Anim)
		{
			Anim->UnitType = C.Type; Anim->Faction = C.Fac; Anim->Slot = C.Slot; Anim->Motion = C.Motion;
			Anim->RebuildFigure();
			Component->TickAnimation(0.f, false);
			Component->RefreshBoneTransforms(nullptr);
			const TArray<FTransform>& Pose = Component->GetComponentSpaceTransforms();
			double Worst = 0;
			SkinnedError(Pose, C, Worst);
			R->SetNumberField(TEXT("animInstanceMaxErrorMetres"), Worst);
			// Single keyframes must match to float precision. Blends of keyframes: the anim instance blends bone
			// transforms (normalized quaternions), the reference blends vertex positions, so a small difference is
			// expected there and reported, not failed (the crowd path below blends exactly like the reference).
			if (bExactKey && Worst > 2e-4) Fail(FString::Printf(TEXT("%s: anim instance off by %g m"), *C.Name, Worst));
		}
		CaseReports.Add(MakeShared<FJsonValueObject>(R));
	}

	// ------------------------------------------------------------------ 3. the crowd path, M_InfantryCrowd on the CPU
	UStaticMesh* Crowd = LoadObject<UStaticMesh>(nullptr, TEXT("/Game/Models/Infantry/SM_InfantryCrowdBody.SM_InfantryCrowdBody"));
	if (!Crowd) Fail(TEXT("SM_InfantryCrowdBody missing"));
	else
	{
		const FMeshDescription* Desc = Crowd->GetMeshDescription(0);
		FStaticMeshConstAttributes Attr(*Desc);
		const TVertexAttributesConstRef<FVector3f> Positions = Attr.GetVertexPositions();
		const TVertexInstanceAttributesConstRef<FVector2f> UVs = Attr.GetVertexInstanceUVs();
		Report->SetNumberField(TEXT("crowdUVChannels"), UVs.GetNumChannels());
		if (UVs.GetNumChannels() < 5) Fail(TEXT("SM_InfantryCrowdBody lost its joint and weight UV channels"));
		for (int32 CaseIndex = 0; CaseIndex < Cases.Num() && UVs.GetNumChannels() >= 5; CaseIndex++)
		{
			const FCase& C = Cases[CaseIndex];
			UTexture2D* Texture = UTCInfantryRigLibrary::BakeKeyframeTexture(C.Type, C.Fac, C.Slot);
			const TArray<float> Data = UTCInfantryRigLibrary::CrowdCustomData(UTCInfantryRigLibrary::BlendFor(C.Type, C.Motion));
			const FTexture2DMipMap& Mip = Texture->GetPlatformData()->Mips[0];
			const float* Texels = static_cast<const float*>(Mip.BulkData.LockReadOnly());
			const int32 Width = Texture->GetSizeX();
			auto Load = [&](int32 X, int32 Row) { const float* T = Texels + (Row * Width + X) * 4; return FVector4(T[0], T[1], T[2], T[3]); };
			double Worst = 0;
			int32 Considered = 0, KeysUsed = 0;
			for (int32 K = 0; K < 4; K++) KeysUsed += Data[4 + K] > 0.f;
			for (const FVertexInstanceID Instance : Desc->VertexInstances().GetElementIDs())
			{
				const FVector P = FVector(Positions[Desc->GetVertexInstanceVertex(Instance)]);
				const FVector2f J01 = UVs.Get(Instance, 1), J23 = UVs.Get(Instance, 2), W01 = UVs.Get(Instance, 3), W23 = UVs.Get(Instance, 4);
				const float J[4] = { J01.X, J01.Y, J23.X, J23.Y }, W[4] = { W01.X, W01.Y, W23.X, W23.Y };
				// the SKIN code of import_infantry.py, Mode 0, not rigid
				FVector Acc = FVector::ZeroVector;
				const FVector4 P4(P.X, P.Y, P.Z, 1.0);
				for (int32 K = 0; K < 4; K++)
				{
					const float KW = Data[4 + K];
					if (KW <= 0.f) continue;
					const int32 Row = static_cast<int32>(Data[K] + 0.5f);
					for (int32 B = 0; B < 4; B++)
					{
						if (W[B] <= 0.f) continue;
						const int32 X = static_cast<int32>(J[B] + 0.5f) * 3;
						const FVector Q(Dot4(Load(X, Row), P4), Dot4(Load(X + 1, Row), P4), Dot4(Load(X + 2, Row), P4));
						Acc += Q * (KW * W[B]);
					}
				}
				Acc.Z *= Data[9];
				const double Cs = FMath::Cos(Data[8]), Sn = FMath::Sin(Data[8]);
				FVector Out(Acc.X * Cs - Acc.Z * Sn, Acc.Y, Acc.X * Sn + Acc.Z * Cs);
				Out.X += Data[10]; Out.Z += Data[11];
				double D = 0;
				const int32 Payload = Index.Find(ToRef(P), D);
				Worst = FMath::Max(Worst, FVector::Dist(ToRef(Out), C.Positions[Payload]));
				Considered++;
			}
			Mip.BulkData.Unlock();
			TSharedPtr<FJsonObject> R = CaseReports[CaseIndex]->AsObject();
			R->SetNumberField(TEXT("crowdMaxErrorMetres"), Worst);
			R->SetNumberField(TEXT("crowdKeysUsed"), KeysUsed);
			R->SetNumberField(TEXT("crowdVertexInstances"), Considered);
			// four keyframes carry every case here, so the crowd path must match the reference to float precision
			if (Worst > 2e-4) Fail(FString::Printf(TEXT("%s: crowd path off by %g m"), *C.Name, Worst));
		}
	}
	Report->SetArrayField(TEXT("cases"), CaseReports);
	Report->SetArrayField(TEXT("failures"), Failures);
	Report->SetBoolField(TEXT("pass"), Failures.IsEmpty());
	GEngine->DestroyWorldContext(World);
	World->DestroyWorld(false);
	FString Json;
	FJsonSerializer::Serialize(Report, TJsonWriterFactory<>::Create(&Json));
	FFileHelper::SaveStringToFile(Json, *OutPath);
	UE_LOG(LogTemp, Display, TEXT("CHECK %s"), Failures.IsEmpty() ? TEXT("PASS") : TEXT("FAIL"));
	return Failures.IsEmpty() ? 0 : 1;
}

// ---------------------------------------------------------------------------------------------- render check
#include "Components/DirectionalLightComponent.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Components/SceneCaptureComponent2D.h"
#include "Components/StaticMeshComponent.h"
#include "Engine/TextureRenderTarget2D.h"
#include "ImageUtils.h"
#include "Materials/Material.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "MaterialShared.h"
#include "RenderingThread.h"
#include "ShaderCompiler.h"
#include "TextureResource.h"
#include "AssetCompilingManager.h"
#include "Components/SkyLightComponent.h"
#include "Engine/TextureCube.h"

namespace
{
	const TCHAR* FacTag(int32 Fac) { static const TCHAR* Tags[] = { TEXT("US"), TEXT("DE"), TEXT("SU"), TEXT("UK") }; return Tags[FMath::Clamp(Fac, 0, 3)]; }
	FString SafeMat(const FString& Mat) { return Mat.Replace(TEXT("-"), TEXT("_")); }
}

int32 UTCInfantryRenderCheckCommandlet::Main(const FString& Params)
{
	FString KitsPath, PngPath, OutPath;
	FParse::Value(*Params, TEXT("Kits="), KitsPath);
	FParse::Value(*Params, TEXT("Png="), PngPath);
	FParse::Value(*Params, TEXT("Out="), OutPath);
	TSharedRef<FJsonObject> Report = MakeShared<FJsonObject>();
	TArray<TSharedPtr<FJsonValue>> Failures;
	auto Fail = [&Failures](const FString& What) { UE_LOG(LogTemp, Error, TEXT("RENDER FAIL %s"), *What); Failures.Add(MakeShared<FJsonValueString>(What)); };
	Report->SetBoolField(TEXT("canRender"), FApp::CanEverRender());
	Report->SetStringField(TEXT("shaderPlatform"), LexToString(GMaxRHIShaderPlatform));

	FString Text;
	TSharedPtr<FJsonObject> Manifest;
	if (!FFileHelper::LoadFileToString(Text, *KitsPath) || !FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Manifest) || !Manifest.IsValid()) { UE_LOG(LogTemp, Error, TEXT("RENDER cannot read %s"), *KitsPath); return 1; }
	TMap<FString, TArray<FString>> MeshMaterials;
	for (const TSharedPtr<FJsonValue>& M : Manifest->GetArrayField(TEXT("meshes")))
	{
		TArray<FString> Mats;
		for (const TSharedPtr<FJsonValue>& V : M->AsObject()->GetArrayField(TEXT("materials"))) Mats.Add(V->AsString());
		MeshMaterials.Add(M->AsObject()->GetStringField(TEXT("name")), Mats);
	}
	auto KitOf = [&Manifest](const FString& Type, int32 Fac, const FString& Job) -> TSharedPtr<FJsonObject>
	{
		for (const TSharedPtr<FJsonValue>& K : Manifest->GetArrayField(TEXT("kits")))
		{
			const TSharedPtr<FJsonObject>& O = K->AsObject();
			if (O->GetStringField(TEXT("type")) == Type && static_cast<int32>(O->GetNumberField(TEXT("fac"))) == Fac && O->GetStringField(TEXT("job")) == Job) return O;
		}
		return nullptr;
	};

	// ------------------------------------------------------------------ materials compile on this shader platform
	const TCHAR* MaterialPaths[] = { TEXT("/Game/Models/Infantry/Materials/M_InfantryBody.M_InfantryBody"), TEXT("/Game/Models/Infantry/Materials/M_InfantryKit.M_InfantryKit"),
		TEXT("/Game/Models/Infantry/Materials/M_InfantryCrowd.M_InfantryCrowd"), TEXT("/Game/Models/Infantry/Materials/M_InfantryCrowdKit.M_InfantryCrowdKit") };
	TArray<UMaterial*> Materials;
	for (const TCHAR* Path : MaterialPaths) if (UMaterial* M = LoadObject<UMaterial>(nullptr, Path)) Materials.Add(M); else Fail(FString::Printf(TEXT("missing %s"), Path));
	for (UMaterial* M : Materials) M->ForceRecompileForRendering();
	if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();
	TArray<TSharedPtr<FJsonValue>> MaterialReports;
	for (UMaterial* M : Materials)
	{
		const FMaterialResource* Resource = M->GetMaterialResource(GMaxRHIShaderPlatform);
		TSharedRef<FJsonObject> R = MakeShared<FJsonObject>();
		R->SetStringField(TEXT("material"), M->GetName());
		TArray<TSharedPtr<FJsonValue>> Errors;
		if (Resource) for (const FString& E : Resource->GetCompileErrors()) Errors.Add(MakeShared<FJsonValueString>(E));
		R->SetArrayField(TEXT("compileErrors"), Errors);
		R->SetBoolField(TEXT("hasShaderMap"), Resource && Resource->GetGameThreadShaderMap() != nullptr);
		if (!Resource || !Errors.IsEmpty() || !Resource->GetGameThreadShaderMap()) Fail(FString::Printf(TEXT("%s does not compile on %s"), *M->GetName(), *LexToString(GMaxRHIShaderPlatform)));
		MaterialReports.Add(MakeShared<FJsonValueObject>(R));
	}
	Report->SetArrayField(TEXT("materials"), MaterialReports);

	// ------------------------------------------------------------------ the scene
	UWorld* World = UWorld::CreateWorld(EWorldType::Game, false, TEXT("InfantryRenderCheck"));
	FWorldContext& Context = GEngine->CreateNewWorldContext(EWorldType::Game);
	Context.SetCurrentWorld(World);
	UObject* Outer = World->GetWorldSettings();
	USkeletalMesh* Mesh = LoadObject<USkeletalMesh>(nullptr, TEXT("/Game/Models/Infantry/SK_Infantry.SK_Infantry"));
	auto Load = [](const FString& Path) { return LoadObject<UObject>(nullptr, *Path); };
	auto KitMaterial = [&](const FString& Mat, bool bCrowd) { return Cast<UMaterialInterface>(Load(FString::Printf(TEXT("/Game/Models/Infantry/Materials/MI_Infantry%s_%s"), bCrowd ? TEXT("CrowdKit") : TEXT("Kit"), *SafeMat(Mat)))); };

	// Meshes compile asynchronously in the editor; a component registered before its mesh is built draws nothing.
	for (const TSharedPtr<FJsonValue>& M : Manifest->GetArrayField(TEXT("meshes"))) Load(FString::Printf(TEXT("/Game/Models/Infantry/Kit/SM_InfantryKit_%s"), *M->AsObject()->GetStringField(TEXT("name"))));
	Load(TEXT("/Game/Models/Infantry/SM_InfantryCrowdBody.SM_InfantryCrowdBody"));
	Load(TEXT("/Engine/BasicShapes/Plane.Plane"));
	FAssetCompilingManager::Get().FinishAllCompilation();

	// Near: three skeletal figures, standing aim (US rifleman), kneeling (DE MG feeder), running (SU leader), kit on sockets.
	struct FNear { const TCHAR* Type; int32 Fac; int32 Slot; FTCInfantryMotion Motion; FVector At; };
	FTCInfantryMotion Stand, Kneel, Run;
	Kneel.PostureWeights = FVector4(0, 1, 0, 0);
	Run.PostureWeights = FVector4(0, 0, 0, 1); Run.Moving = 1.f; Run.Phase = 0.3f;
	const FNear Near[] = { { TEXT("rifle"), 0, 1, Stand, FVector(0, -120, 0) }, { TEXT("mg"), 1, 1, Kneel, FVector(0, 0, 0) }, { TEXT("rifle"), 2, 0, Run, FVector(0, 120, 0) },
		{ TEXT("flamer"), 3, 0, Stand, FVector(0, 240, 0) }, { TEXT("rifle"), 0, 1, Stand, FVector(0, -240, 0) } };
	int32 NearIndex = 0;
	int32 Attached = 0;
	TArray<TSharedPtr<FJsonValue>> AttachReports;
	for (const FNear& N : Near)
	{
		USkeletalMeshComponent* Body = NewObject<USkeletalMeshComponent>(Outer);
		Body->SetSkeletalMesh(Mesh);
		Body->VisibilityBasedAnimTickOption = EVisibilityBasedAnimTickOption::AlwaysTickPoseAndRefreshBones;
		Body->SetAnimationMode(EAnimationMode::AnimationBlueprint);
		Body->SetAnimInstanceClass(UTCInfantryAnimInstance::StaticClass());
		const float Scale = UTCInfantryRigLibrary::FigureScale(N.Type);
		Body->SetWorldTransform(FTransform(FQuat::Identity, N.At, FVector(Scale)));
		UMaterialInterface* Look = Cast<UMaterialInterface>(Load(FString::Printf(TEXT("/Game/Models/Infantry/Materials/MI_InfantryBody_%s"), FacTag(N.Fac))));
		if (NearIndex++ == 4)
		{
			// the same rifleman with a flat normal map: shows what the atlas normal map adds or breaks
			UMaterialInstanceDynamic* Flat = UMaterialInstanceDynamic::Create(Look, Outer);
			Flat->SetTextureParameterValue(TEXT("Normal"), Cast<UTexture>(Load(TEXT("/Engine/EngineMaterials/FlatNormal.FlatNormal"))));
			Look = Flat;
		}
		Body->SetMaterial(0, Look); Body->SetMaterial(1, Look);
		Body->RegisterComponentWithWorld(World);
		// the authored crown (helmet) belongs to the German figure only; the others wear their head_gear mesh
		if (N.Fac != 1) Body->ShowMaterialSection(1, 1, false, 0);
		UTCInfantryAnimInstance* Anim = Cast<UTCInfantryAnimInstance>(Body->GetAnimInstance());
		Anim->UnitType = N.Type; Anim->Faction = N.Fac; Anim->Slot = N.Slot; Anim->Motion = N.Motion;
		Anim->RebuildFigure();
		Body->TickAnimation(0.f, false);
		Body->RefreshBoneTransforms(nullptr);
		const FString Job = UTCInfantryRigLibrary::SoldierKit(N.Type, N.Slot, N.Fac);
		const TSharedPtr<FJsonObject> Kit = KitOf(N.Type, N.Fac, Job);
		if (!Kit) { Fail(FString::Printf(TEXT("no kit for %s/%d/%s"), N.Type, N.Fac, *Job)); continue; }
		for (const TSharedPtr<FJsonValue>& A : Kit->GetArrayField(TEXT("attachments")))
		{
			const FString Bone = A->AsObject()->GetStringField(TEXT("bone")), MeshName = A->AsObject()->GetStringField(TEXT("mesh"));
			UStaticMesh* Part = Cast<UStaticMesh>(Load(FString::Printf(TEXT("/Game/Models/Infantry/Kit/SM_InfantryKit_%s"), *MeshName)));
			if (!Part) { Fail(FString::Printf(TEXT("missing kit mesh %s"), *MeshName)); continue; }
			UStaticMeshComponent* C = NewObject<UStaticMeshComponent>(Outer);
			C->SetStaticMesh(Part);
			C->SetupAttachment(Body, FName(*Bone));
			C->RegisterComponentWithWorld(World);
			Attached++;
			TSharedRef<FJsonObject> B = MakeShared<FJsonObject>();
			B->SetStringField(TEXT("mesh"), MeshName);
			B->SetStringField(TEXT("bone"), Bone);
			B->SetStringField(TEXT("center"), C->Bounds.Origin.ToString());
			B->SetNumberField(TEXT("radius"), C->Bounds.SphereRadius);
			B->SetStringField(TEXT("socketAt"), Body->GetSocketLocation(FName(*Bone)).ToString());
			AttachReports.Add(MakeShared<FJsonValueObject>(B));
		}
	}
	Report->SetNumberField(TEXT("kitComponentsAttached"), Attached);
	Report->SetArrayField(TEXT("attachments"), AttachReports);

	// Crowd: one instanced draw for the body and one per kit mesh, every man a different motion state.
	{
		const FString Type = TEXT("rifle"); const int32 Fac = 0, Slot = 2;
		UTexture2D* Keys = UTCInfantryRigLibrary::BakeKeyframeTexture(Type, Fac, Slot);
		TArray<FTCInfantryMotion> States;
		for (int32 I = 0; I < 8; I++) { FTCInfantryMotion M; M.PostureWeights = FVector4(0, 0, 0, 1); M.Moving = 1.f; M.Phase = I / 8.f; States.Add(M); }
		{ FTCInfantryMotion M; States.Add(M); }
		{ FTCInfantryMotion M; M.PostureWeights = FVector4(0, 1, 0, 0); States.Add(M); }
		{ FTCInfantryMotion M; M.PostureWeights = FVector4(0, 0, 1, 0); States.Add(M); }
		{ FTCInfantryMotion M; M.PostureWeights = FVector4(0, 0, 1, 0); M.Moving = 1.f; M.Phase = 0.25f; States.Add(M); }
		{ FTCInfantryMotion M; M.Work = 1.f; M.WorkPhase = 0.25f; States.Add(M); }
		{ FTCInfantryMotion M; M.bFallen = true; States.Add(M); }
		{ FTCInfantryMotion M; M.PostureWeights = FVector4(0.5, 0, 0.5, 0); States.Add(M); }
		{ FTCInfantryMotion M; M.Moving = 0.8f; M.Aim = 1.f; M.Phase = 0.4f; States.Add(M); }
		const float Scale = UTCInfantryRigLibrary::FigureScale(Type);
		auto Place = [&](UInstancedStaticMeshComponent* Ism, int32 Extra, float Bone)
		{
			Ism->NumCustomDataFloats = 12 + Extra;
			for (int32 I = 0; I < States.Num(); I++)
			{
				const FTCInfantryBlend Blend = UTCInfantryRigLibrary::BlendFor(Type, States[I]);
				const FVector At(-260.0 - (I / 8) * 160.0 + Blend.ManOffset.X * Scale, -360.0 + (I % 8) * 100.0, Blend.ManOffset.Y * Scale);
				const int32 Instance = Ism->AddInstance(FTransform(FQuat::Identity, At, FVector(Scale)), true);
				TArray<float> Data = UTCInfantryRigLibrary::CrowdCustomData(Blend);
				if (Extra) Data.Add(Bone);
				Ism->SetCustomData(Instance, Data, true);
			}
		};
		UInstancedStaticMeshComponent* Bodies = NewObject<UInstancedStaticMeshComponent>(Outer);
		Bodies->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Game/Models/Infantry/SM_InfantryCrowdBody.SM_InfantryCrowdBody")));
		UMaterialInstanceDynamic* BodyLook = UMaterialInstanceDynamic::Create(Cast<UMaterialInterface>(Load(TEXT("/Game/Models/Infantry/Materials/MI_InfantryCrowd_US"))), Outer);
		BodyLook->SetTextureParameterValue(TEXT("Keyframes"), Keys);
		Bodies->SetMaterial(0, BodyLook);
		Bodies->SetMaterial(1, BodyLook);
		Bodies->RegisterComponentWithWorld(World);
		Bodies->SetMaterial(1, BodyLook);
		Place(Bodies, 0, 0.f);
		// the crown section is hidden for this faction: the crowd keeps only section 0 visible through a second ISM
		const TSharedPtr<FJsonObject> Kit = KitOf(Type, Fac, UTCInfantryRigLibrary::SoldierKit(Type, Slot, Fac));
		const TArray<FName> BoneNames = UTCInfantryRigLibrary::BoneNames();
		int32 Draws = 1;
		for (const TSharedPtr<FJsonValue>& A : Kit->GetArrayField(TEXT("attachments")))
		{
			const FString Bone = A->AsObject()->GetStringField(TEXT("bone")), MeshName = A->AsObject()->GetStringField(TEXT("mesh"));
			UInstancedStaticMeshComponent* Part = NewObject<UInstancedStaticMeshComponent>(Outer);
			Part->SetStaticMesh(Cast<UStaticMesh>(Load(FString::Printf(TEXT("/Game/Models/Infantry/Kit/SM_InfantryKit_%s"), *MeshName))));
			const TArray<FString>& Mats = MeshMaterials.FindChecked(MeshName);
			for (int32 S = 0; S < Mats.Num(); S++)
			{
				UMaterialInstanceDynamic* Look = UMaterialInstanceDynamic::Create(KitMaterial(Mats[S], true), Outer);
				Look->SetTextureParameterValue(TEXT("Keyframes"), Keys);
				Part->SetMaterial(S, Look);
			}
			Part->RegisterComponentWithWorld(World);
			Place(Part, 1, static_cast<float>(BoneNames.IndexOfByKey(FName(*Bone))));
			Draws++;
		}
		Report->SetNumberField(TEXT("crowdMen"), States.Num());
		Report->SetNumberField(TEXT("crowdDraws"), Draws);
	}
	// Two lights, so the far side of the figures is not black, and a ground plane.
	UDirectionalLightComponent* Sun = NewObject<UDirectionalLightComponent>(Outer);
	Sun->SetWorldRotation(FRotator(-40.f, 150.f, 0.f)); Sun->SetIntensity(8.f); Sun->RegisterComponentWithWorld(World);
	UDirectionalLightComponent* Fill = NewObject<UDirectionalLightComponent>(Outer);
	Fill->SetWorldRotation(FRotator(-20.f, -60.f, 0.f)); Fill->SetIntensity(4.f); Fill->SetCastShadows(false); Fill->RegisterComponentWithWorld(World);
	USkyLightComponent* Sky = NewObject<USkyLightComponent>(Outer);
	Sky->SourceType = SLS_SpecifiedCubemap;
	Sky->Cubemap = Cast<UTextureCube>(Load(TEXT("/Engine/MapTemplates/Sky/DaylightAmbientCubemap.DaylightAmbientCubemap")));
	Sky->SetIntensity(1.f);
	Sky->RegisterComponentWithWorld(World);
	Sky->RecaptureSky();
	UStaticMeshComponent* Ground = NewObject<UStaticMeshComponent>(Outer);
	Ground->SetStaticMesh(LoadObject<UStaticMesh>(nullptr, TEXT("/Engine/BasicShapes/Plane.Plane")));
	Ground->SetWorldTransform(FTransform(FQuat::Identity, FVector(-300, 0, 0), FVector(14, 14, 1)));
	Ground->RegisterComponentWithWorld(World);

	FAssetCompilingManager::Get().FinishAllCompilation();
	if (GShaderCompilingManager) GShaderCompilingManager->FinishAllCompilation();
	// two views: the near figures from the front-right, and the whole set from above
	struct FView { const TCHAR* Suffix; FVector At; FRotator Look; float Fov; };
	const FView Views[] = { { TEXT("near"), FVector(330, -40, 150), FRotator(-8.f, 180.f, 0.f), 55.f }, { TEXT("crowd"), FVector(380, -60, 520), FRotator(-38.f, 180.f, 0.f), 75.f } };
	TArray<TSharedPtr<FJsonValue>> Shots;
	for (const FView& View : Views)
	{
		UTextureRenderTarget2D* Target = NewObject<UTextureRenderTarget2D>(Outer);
		Target->RenderTargetFormat = RTF_RGBA8_SRGB;
		Target->ClearColor = FLinearColor(0.42f, 0.47f, 0.52f);
		Target->InitAutoFormat(1600, 900);
		Target->UpdateResourceImmediate(true);
		USceneCaptureComponent2D* Capture = NewObject<USceneCaptureComponent2D>(Outer);
		Capture->TextureTarget = Target;
		Capture->CaptureSource = SCS_FinalColorLDR;
		Capture->bCaptureEveryFrame = false;
		Capture->bCaptureOnMovement = false;
		Capture->FOVAngle = View.Fov;
		// fixed exposure: an empty scene with no sky would otherwise adapt to black
		Capture->PostProcessSettings.bOverride_AutoExposureMethod = true;
		Capture->PostProcessSettings.AutoExposureMethod = AEM_Manual;
		Capture->PostProcessSettings.bOverride_AutoExposureBias = true;
		Capture->PostProcessSettings.AutoExposureBias = 1.5f;
		Capture->PostProcessSettings.bOverride_AutoExposureApplyPhysicalCameraExposure = true;
		Capture->PostProcessSettings.AutoExposureApplyPhysicalCameraExposure = false;
		Capture->PostProcessBlendWeight = 1.f;
		Capture->SetWorldLocationAndRotation(View.At, View.Look);
		Capture->RegisterComponentWithWorld(World);
		for (int32 Frame = 0; Frame < 4; Frame++)
		{
			World->SendAllEndOfFrameUpdates();
			Capture->CaptureScene();
			FlushRenderingCommands();
		}
		TArray<FColor> Pixels;
		FTextureRenderTargetResource* Resource = Target->GameThread_GetRenderTargetResource();
		const FString Path = FPaths::GetBaseFilename(PngPath, false) + TEXT("-") + View.Suffix + TEXT(".png");
		if (!Resource || !Resource->ReadPixels(Pixels) || Pixels.IsEmpty()) { Fail(FString::Printf(TEXT("could not read the %s capture"), View.Suffix)); continue; }
		for (FColor& C : Pixels) C.A = 255;
		TArray64<uint8> Png;
		FImageUtils::PNGCompressImageArray(1600, 900, TArrayView64<const FColor>(Pixels.GetData(), Pixels.Num()), Png);
		FFileHelper::SaveArrayToFile(Png, *Path);
		Shots.Add(MakeShared<FJsonValueString>(Path));
	}
	Report->SetArrayField(TEXT("captures"), Shots);
	Report->SetArrayField(TEXT("failures"), Failures);
	Report->SetBoolField(TEXT("pass"), Failures.IsEmpty());
	GEngine->DestroyWorldContext(World);
	World->DestroyWorld(false);
	FString Json;
	FJsonSerializer::Serialize(Report, TJsonWriterFactory<>::Create(&Json));
	FFileHelper::SaveStringToFile(Json, *OutPath);
	UE_LOG(LogTemp, Display, TEXT("RENDER %s"), Failures.IsEmpty() ? TEXT("PASS") : TEXT("FAIL"));
	return Failures.IsEmpty() ? 0 : 1;
}
