#include "Client/TCModelLibrary.h"
#include "Components/InstancedStaticMeshComponent.h"
#include "Engine/StaticMesh.h"
#include "Materials/MaterialInterface.h"
#include "Materials/MaterialInstanceDynamic.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Misc/CommandLine.h"
#include "Misc/Parse.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace
{
	bool ReadVector(const TSharedPtr<FJsonObject>& O, const TCHAR* Field, FVector& Out)
	{
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (!O->TryGetArrayField(Field, A) || A->Num() != 3) return false;
		Out = FVector((*A)[0]->AsNumber(), (*A)[1]->AsNumber(), (*A)[2]->AsNumber());
		return true;
	}

	bool ReadColor(const TSharedPtr<FJsonValue>& V, FLinearColor& Out)
	{
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (!V.IsValid() || !V->TryGetArray(A) || A->Num() != 3) return false;
		Out = FLinearColor((*A)[0]->AsNumber(), (*A)[1]->AsNumber(), (*A)[2]->AsNumber(), 1.f);
		return true;
	}

	TArray<FSoftObjectPath> ReadPaths(const TSharedPtr<FJsonObject>& O, const TCHAR* Field)
	{
		TArray<FSoftObjectPath> Out;
		const TArray<TSharedPtr<FJsonValue>>* A;
		if (O->TryGetArrayField(Field, A)) for (const TSharedPtr<FJsonValue>& V : *A) Out.Add(FSoftObjectPath(V->AsString()));
		return Out;
	}

	FString JoinVariant(const TSharedPtr<FJsonObject>& O)
	{
		TArray<FString> Bits;
		for (const TCHAR* Field : { TEXT("kit"), TEXT("pose"), TEXT("lod"), TEXT("variant") })
		{
			FString S;
			if (O->TryGetStringField(Field, S) && !S.IsEmpty()) Bits.Add(S);
		}
		return FString::Join(Bits, TEXT("|"));
	}
}

int32 FTCModelEntry::FindPart(FName Name) const
{
	return Parts.IndexOfByPredicate([Name](const FTCModelPart& P) { return P.Name == Name; });
}

bool FTCModelLibrary::Load(FString& OutError)
{
	const FString Path = FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Models/ModelCatalog.json"));
	FString Text;
	if (!FFileHelper::LoadFileToString(Text, *Path)) { OutError = TEXT("cannot read ") + Path; return false; }
	return LoadFromString(Text, OutError);
}

bool FTCModelLibrary::LoadFromString(const FString& Json, FString& OutError)
{
	Entries.Reset(); ByKey.Reset(); ByType.Reset(); Seats.Reset(); VehiclePaints.Reset();
	TSharedPtr<FJsonObject> Root;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Json), Root) || !Root.IsValid()) { OutError = TEXT("model catalog: bad JSON"); return false; }
	FString Format;
	if (!Root->TryGetStringField(TEXT("format"), Format) || Format != TEXT("tc-model-catalog-v1")) { OutError = FString::Printf(TEXT("model catalog: format '%s', expected tc-model-catalog-v1"), *Format); return false; }
	Root->TryGetStringField(TEXT("reference_commit"), Reference);
	// the custom data layout is a contract with the generated materials: refuse a catalog written for another one
	const TSharedPtr<FJsonObject>* Data;
	double Floats = 0, Owner = -1, Vehicle = -1, FadeAt = -1, DamageAt = -1;
	if (!Root->TryGetObjectField(TEXT("custom_data"), Data) || !(*Data)->TryGetNumberField(TEXT("floats"), Floats) || !(*Data)->TryGetNumberField(TEXT("owner"), Owner)
		|| !(*Data)->TryGetNumberField(TEXT("vehicle"), Vehicle) || !(*Data)->TryGetNumberField(TEXT("fade"), FadeAt) || !(*Data)->TryGetNumberField(TEXT("damage"), DamageAt)
		|| (int32)Floats != TCModelData::Count || (int32)Owner != TCModelData::OwnerR || (int32)Vehicle != TCModelData::VehicleR || (int32)FadeAt != TCModelData::Fade || (int32)DamageAt != TCModelData::Damage)
	{
		OutError = TEXT("model catalog: its custom data layout is not this client's (TCModelData)");
		return false;
	}
	const TArray<TSharedPtr<FJsonValue>>* List;
	if (Root->TryGetArrayField(TEXT("seat_colors"), List))
		for (const TSharedPtr<FJsonValue>& V : *List) { FLinearColor C; if (ReadColor(V, C)) Seats.Add(C); }
	{
		const TArray<TSharedPtr<FJsonValue>>* N;
		if (Root->TryGetArrayField(TEXT("neutral_color"), N) && N->Num() == 3) Neutral = FLinearColor((*N)[0]->AsNumber(), (*N)[1]->AsNumber(), (*N)[2]->AsNumber(), 1.f);
	}
	if (Root->TryGetArrayField(TEXT("factions"), List))
		for (const TSharedPtr<FJsonValue>& V : *List)
		{
			const TSharedPtr<FJsonObject>* F;
			FLinearColor C = FLinearColor::Black;
			if (V->TryGetObject(F)) { const TSharedPtr<FJsonValue> P = (*F)->TryGetField(TEXT("vehicle")); ReadColor(P, C); }
			VehiclePaints.Add(C);
		}
	const TSharedPtr<FJsonObject>* Fades;
	if (Root->TryGetObjectField(TEXT("fade"), Fades))
	{
		double V;
		if ((*Fades)->TryGetNumberField(TEXT("camouflage"), V)) CamouflageFade = V;
		if ((*Fades)->TryGetNumberField(TEXT("ghost"), V)) GhostFade = V;
	}
	if (!Root->TryGetArrayField(TEXT("models"), List)) { OutError = TEXT("model catalog: no models"); return false; }
	for (const TSharedPtr<FJsonValue>& V : *List)
	{
		const TSharedPtr<FJsonObject>* M;
		if (!V->TryGetObject(M)) continue;
		FTCModelEntry E;
		double Number;
		(*M)->TryGetStringField(TEXT("key"), E.Key);
		(*M)->TryGetStringField(TEXT("type"), E.Type);
		if ((*M)->TryGetNumberField(TEXT("faction"), Number)) E.Faction = (int32)Number;
		(*M)->TryGetStringField(TEXT("look"), E.Look);
		if ((*M)->TryGetNumberField(TEXT("scale"), Number)) E.Scale = Number;
		E.Variant = JoinVariant(*M);
		if (E.Key.IsEmpty() || E.Type.IsEmpty()) { OutError = TEXT("model catalog: a model without key or type"); return false; }
		const TArray<TSharedPtr<FJsonValue>>* Parts;
		if ((*M)->TryGetArrayField(TEXT("parts"), Parts))
			for (const TSharedPtr<FJsonValue>& PV : *Parts)
			{
				const TSharedPtr<FJsonObject>* P;
				if (!PV->TryGetObject(P)) continue;
				FTCModelPart Part;
				FString S;
				if ((*P)->TryGetStringField(TEXT("name"), S)) Part.Name = FName(*S);
				if ((*P)->TryGetStringField(TEXT("parent"), S) && !S.IsEmpty()) Part.Parent = FName(*S);
				if ((*P)->TryGetStringField(TEXT("socket"), S) && !S.IsEmpty()) Part.Socket = FName(*S);
				if ((*P)->TryGetStringField(TEXT("mesh"), S)) Part.Mesh = FSoftObjectPath(S);
				Part.Materials = ReadPaths(*P, TEXT("materials"));
				Part.FadedMaterials = ReadPaths(*P, TEXT("faded_materials"));
				const TArray<TSharedPtr<FJsonValue>>* Sockets;
				if ((*P)->TryGetArrayField(TEXT("sockets"), Sockets)) for (const TSharedPtr<FJsonValue>& SV : *Sockets) Part.Sockets.Add(FName(*SV->AsString()));
				(*P)->TryGetBoolField(TEXT("hidden"), Part.bHidden);
				const TSharedPtr<FJsonObject>* Rest;
				if ((*P)->TryGetObjectField(TEXT("rest"), Rest))
				{
					FVector Location = FVector::ZeroVector, Scale = FVector::OneVector;
					ReadVector(*Rest, TEXT("location"), Location);
					ReadVector(*Rest, TEXT("scale"), Scale);
					FQuat Rotation = FQuat::Identity;
					const TArray<TSharedPtr<FJsonValue>>* Q;
					if ((*Rest)->TryGetArrayField(TEXT("quat"), Q) && Q->Num() == 4) Rotation = FQuat((*Q)[0]->AsNumber(), (*Q)[1]->AsNumber(), (*Q)[2]->AsNumber(), (*Q)[3]->AsNumber()).GetNormalized();
					Part.Rest = FTransform(Rotation, Location, Scale);
				}
				if (Part.Name.IsNone() || Part.Mesh.IsNull()) { OutError = FString::Printf(TEXT("model catalog: %s has a part without name or mesh"), *E.Key); return false; }
				if (Part.FadedMaterials.Num() != Part.Materials.Num()) { OutError = FString::Printf(TEXT("model catalog: %s/%s has %d materials but %d faded ones"), *E.Key, *Part.Name.ToString(), Part.Materials.Num(), Part.FadedMaterials.Num()); return false; }
				E.Parts.Add(MoveTemp(Part));
			}
		// parents before children, so a renderer can walk the list once
		TArray<FTCModelPart> Ordered;
		while (Ordered.Num() < E.Parts.Num())
		{
			const int32 Before = Ordered.Num();
			for (const FTCModelPart& P : E.Parts)
			{
				const bool bPlaced = Ordered.ContainsByPredicate([&P](const FTCModelPart& O) { return O.Name == P.Name; });
				const bool bParentPlaced = P.Parent.IsNone() || Ordered.ContainsByPredicate([&P](const FTCModelPart& O) { return O.Name == P.Parent; });
				if (!bPlaced && bParentPlaced) Ordered.Add(P);
			}
			if (Ordered.Num() == Before) { OutError = FString::Printf(TEXT("model catalog: %s has a part whose parent is missing or a cycle"), *E.Key); return false; }
		}
		for (const FTCModelPart& P : Ordered)
			if (!P.Parent.IsNone())
			{
				const FTCModelPart* Parent = Ordered.FindByPredicate([&P](const FTCModelPart& O) { return O.Name == P.Parent; });
				if (P.Socket.IsNone() || !Parent->Sockets.Contains(P.Socket)) { OutError = FString::Printf(TEXT("model catalog: %s/%s hangs on socket '%s', which %s does not list"), *E.Key, *P.Name.ToString(), *P.Socket.ToString(), *P.Parent.ToString()); return false; }
			}
		E.Parts = MoveTemp(Ordered);
		if (ByKey.Contains(E.Key)) { OutError = TEXT("model catalog: duplicate key ") + E.Key; return false; }
		ByKey.Add(E.Key, Entries.Num());
		ByType.FindOrAdd(E.Type).Add(Entries.Num());
		Entries.Add(MoveTemp(E));
	}
	for (auto& Pair : ByType)
		Pair.Value.Sort([this](int32 A, int32 B) { return Entries[A].Faction != Entries[B].Faction ? Entries[A].Faction < Entries[B].Faction : Entries[A].Variant < Entries[B].Variant; });
	return true;
}

const FTCModelEntry* FTCModelLibrary::Find(const FString& Key) const
{
	const int32* I = ByKey.Find(Key);
	return I ? &Entries[*I] : nullptr;
}

bool FTCModelLibrary::HasOwnUkModel(const FString& Type)
{
	// client/unit-models.js UK_MODELS at the pin
	static const TCHAR* Own[] = { TEXT("churchill"), TEXT("armoredcar"), TEXT("halftrack"), TEXT("medium"), TEXT("mg"), TEXT("at"), TEXT("mortar"), TEXT("flak"), TEXT("truck"), TEXT("lorry"), TEXT("train") };
	for (const TCHAR* T : Own) if (Type == T) return true;
	return false;
}

const FTCModelEntry* FTCModelLibrary::Resolve(const FTCModelQuery& Query, ETCModelMatch* OutMatch) const
{
	auto Pick = [this](const FString& Type, int32 Faction, const FString& Variant) -> const FTCModelEntry*
	{
		const TArray<int32>* List = ByType.Find(Type);
		if (!List) return nullptr;
		for (int32 I : *List) if (Entries[I].Faction == Faction && Entries[I].Variant == Variant) return &Entries[I];
		return nullptr;
	};
	auto Done = [OutMatch](const FTCModelEntry* E, ETCModelMatch M) { if (OutMatch) *OutMatch = E ? M : ETCModelMatch::None; return E; };
	if (const FTCModelEntry* E = Pick(Query.Type, Query.Faction, Query.Variant)) return Done(E, ETCModelMatch::Exact);
	// what buildModel itself draws instead: the American vehicle or gun for a UK type without its own, the engineer
	// figure for medics (soldiers always keep their own faction's kit)
	if (Query.Faction == 3 && !HasOwnUkModel(Query.Type))
		if (const FTCModelEntry* E = Pick(Query.Type, 0, Query.Variant)) return Done(E, ETCModelMatch::Borrowed);
	if (FigureType(Query.Type) != Query.Type)
		if (const FTCModelEntry* E = Pick(FigureType(Query.Type), Query.Faction, Query.Variant)) return Done(E, ETCModelMatch::Borrowed);
	// not the reference's picture: any faction's model of the type, lowest faction first
	if (const TArray<int32>* List = ByType.Find(Query.Type))
		for (int32 I : *List) if (Entries[I].Variant == Query.Variant) return Done(&Entries[I], ETCModelMatch::Substitute);
	return Done(nullptr, ETCModelMatch::None);
}

FLinearColor FTCModelLibrary::SeatColor(int32 Seat) const
{
	return Seats.IsValidIndex(Seat) ? Seats[Seat] : Neutral;
}

FLinearColor FTCModelLibrary::VehiclePaint(int32 Faction) const
{
	return VehiclePaints.IsValidIndex(Faction) ? VehiclePaints[Faction] : FLinearColor::Black;
}

void FTCModelLibrary::WriteCustomData(TArrayView<float> Out, const FLinearColor& Owner, const FLinearColor& Vehicle, float Fade, float Damage)
{
	check(Out.Num() >= TCModelData::Count);
	Out[TCModelData::OwnerR] = Owner.R; Out[TCModelData::OwnerG] = Owner.G; Out[TCModelData::OwnerB] = Owner.B;
	Out[TCModelData::VehicleR] = Vehicle.R; Out[TCModelData::VehicleG] = Vehicle.G; Out[TCModelData::VehicleB] = Vehicle.B;
	Out[TCModelData::Fade] = FMath::Clamp(Fade, 0.f, 1.f);
	Out[TCModelData::Damage] = FMath::Clamp(Damage, 0.f, 1.f);
}

TArray<float> FTCModelLibrary::CustomData(int32 OwnerSeat, int32 Faction, float Fade, float Damage) const
{
	TArray<float> Out;
	Out.SetNumZeroed(TCModelData::Count);
	WriteCustomData(Out, SeatColor(OwnerSeat), VehiclePaint(Faction), Fade, Damage);
	return Out;
}

FTransform FTCModelLibrary::PartTransform(const FTCModelEntry& Entry, int32 PartIndex, const FTransform& Unit, const TMap<FName, FQuat>& Turns)
{
	if (!Entry.Parts.IsValidIndex(PartIndex)) return Unit;
	const FTCModelPart& Part = Entry.Parts[PartIndex];
	const FQuat* Turn = Turns.Find(Part.Name);
	const FTransform Own = Turn ? FTransform(*Turn) : FTransform::Identity;
	const int32 ParentIndex = Part.Parent.IsNone() ? INDEX_NONE : Entry.FindPart(Part.Parent);
	if (ParentIndex == INDEX_NONE) return Own * Part.Rest * Unit;
	// the part's place in its parent's frame, then the parent as it is drawn (with its own turn)
	const FTransform Local = Part.Rest.GetRelativeTransform(Entry.Parts[ParentIndex].Rest);
	return Own * Local * PartTransform(Entry, ParentIndex, Unit, Turns);
}

bool FTCModelLibrary::SetupComponent(UInstancedStaticMeshComponent* Component, const FTCModelPart& Part, bool bFaded)
{
	if (!Component) return false;
	UStaticMesh* Mesh = Cast<UStaticMesh>(Part.Mesh.TryLoad());
	if (!Mesh) return false;
	Component->SetStaticMesh(Mesh);
	const TArray<FSoftObjectPath>& Paths = bFaded ? Part.FadedMaterials : Part.Materials;
	for (int32 I = 0; I < Paths.Num(); I++)
		if (UMaterialInterface* M = Cast<UMaterialInterface>(Paths[I].TryLoad()))
		{
			// Low omits environment reflections. A metallic airframe then loses most of its
			// visible paint. Keep the original vertex paint, but use a matte skin and a
			// small rim fill in both tiers, including support passes and falling copies.
			const FString Path = Paths[I].ToString();
			if (!Part.Mesh.ToString().Contains(TEXT("/Meshes/bomb/")) &&
				(Path.Contains(TEXT("MI_TCModel_METAL_MAT")) || Path.Contains(TEXT("MI_TCModel_BODY_MAT"))))
			{
				UMaterialInstanceDynamic* Skin = UMaterialInstanceDynamic::Create(M, Component);
				// The surface table overrides BaseMetalness while ModelTextures is on.
				Skin->SetScalarParameterValue(TEXT("ModelTextures"), 0.f);
				Skin->SetScalarParameterValue(TEXT("BaseMetalness"), 0.f);
				Skin->SetScalarParameterValue(TEXT("BaseRoughness"), .78f);
				Skin->SetVectorParameterValue(TEXT("RimColor"), FLinearColor(.12f, .14f, .16f));
				M = Skin;
			}
			Component->SetMaterial(I, M);
		}
	Component->SetNumCustomDataFloats(TCModelData::Count);
	Component->SetCollisionEnabled(ECollisionEnabled::NoCollision);
	Component->SetMobility(EComponentMobility::Movable);
	return true;
}

int32 FTCInstanceHistory::PreviousFor(const TArray<FTransform>& Cur, const TArray<uint64>& Keys, const TMap<uint64, FTransform>& Last, TArray<FTransform>& OutPrev)
{
	OutPrev.Reset(Cur.Num());
	int32 Found = 0;
	for (int32 I = 0; I < Cur.Num(); I++)
	{
		const FTransform* Before = Keys.IsValidIndex(I) && Keys[I] != NoKey ? Last.Find(Keys[I]) : nullptr;
		if (Before && FVector::DistSquared(Before->GetLocation(), Cur[I].GetLocation()) <= JumpCm * JumpCm) { OutPrev.Add(*Before); Found++; }
		else OutPrev.Add(Cur[I]);
	}
	return Found;
}

void FTCInstanceHistory::Write(UInstancedStaticMeshComponent* Ism, const TArray<FTransform>& Cur, const TArray<uint64>& Keys)
{
	if (!Ism) return;
	TMap<uint64, FTransform>& Mine = Last.FindOrAdd(Ism);
	const int32 N = Cur.Num();
	if (Ism->GetInstanceCount() != N)
	{
		// ClearInstances keeps the previous transforms' array: turned off and on again it is rebuilt to match
		Ism->SetHasPerInstancePrevTransforms(false);
		Ism->ClearInstances();
		if (N) Ism->AddInstances(Cur, false, true, false);
	}
	Ism->SetHasPerInstancePrevTransforms(true);
	if (N)
	{
		TArray<FTransform> Prev;
		// -TCNoMotionVectors: every instance written still, as before round 17 (a verification switch for the A/B)
		static const bool bOff = FParse::Param(FCommandLine::Get(), TEXT("TCNoMotionVectors"));
		Matched += PreviousFor(Cur, bOff ? TArray<uint64>() : Keys, Mine, Prev);
		for (int32 I = 0; I < N; I++) if (!Prev[I].Equals(Cur[I], 0.01)) Moving++;
		Ism->BatchUpdateInstancesTransforms(0, Cur, Prev, true, false, false);
	}
	Written += N;
	Mine.Reset();
	for (int32 I = 0; I < N && I < Keys.Num(); I++) if (Keys[I] != NoKey) Mine.Add(Keys[I], Cur[I]);
	for (auto It = Last.CreateIterator(); It; ++It) if (!It.Key().IsValid()) It.RemoveCurrent();
}

#if WITH_DEV_AUTOMATION_TESTS
#include "Misc/AutomationTest.h"

namespace
{
	// A catalog as the importer writes it, cut down: a US armored car (hull, turret on the hull's "turret" socket),
	// a US light tank, a German one, an engineer squad and a UK Churchill.
	const TCHAR* TestCatalog = TEXT(R"json({
 "format": "tc-model-catalog-v1", "reference_commit": "e4e579a",
 "custom_data": {"floats": 8, "owner": 0, "vehicle": 3, "fade": 6, "damage": 7},
 "factions": [{"name": "USA", "vehicle": [0.1, 0.12, 0.05]}, {"name": "Germany", "vehicle": [0.08, 0.09, 0.1]}, {"name": "USSR", "vehicle": [0.07, 0.1, 0.04]}, {"name": "UK", "vehicle": [0.09, 0.09, 0.05]}],
 "seat_colors": [[0.0437, 0.1714, 0.6724], [0.6038, 0.0423, 0.0273]], "neutral_color": [0.723, 0.723, 0.723],
 "fade": {"camouflage": 0.45, "ghost": 0.35},
 "models": [
  {"key": "armoredcar|0", "type": "armoredcar", "faction": 0, "look": "vehicle", "scale": 1, "parts": [
   {"name": "turret", "parent": "hull", "socket": "turret", "mesh": "/Game/Models/Meshes/armoredcar/SM_armoredcar_0_turret.SM_armoredcar_0_turret",
    "materials": ["/Game/M.M"], "faded_materials": ["/Game/F.F"], "sockets": ["muzzle"], "rest": {"location": [5, 0, 155], "quat": [0, 0, 0, 1], "scale": [1, 1, 1]}},
   {"name": "hull", "parent": "", "socket": "", "mesh": "/Game/Models/Meshes/armoredcar/SM_armoredcar_0_hull.SM_armoredcar_0_hull",
    "materials": ["/Game/M.M"], "faded_materials": ["/Game/F.F"], "sockets": ["turret"], "rest": {"location": [0, 0, 0], "quat": [0, 0, 0, 1], "scale": [1, 1, 1]}}]},
  {"key": "tank|1", "type": "tank", "faction": 1, "look": "vehicle", "parts": [{"name": "hull", "mesh": "/Game/T1.T1", "materials": [], "faded_materials": [], "sockets": []}]},
  {"key": "tank|0", "type": "tank", "faction": 0, "look": "vehicle", "parts": [{"name": "hull", "mesh": "/Game/T0.T0", "materials": [], "faded_materials": [], "sockets": []}]},
  {"key": "engineer|3", "type": "engineer", "faction": 3, "look": "soldier", "parts": [{"name": "man_0_near", "mesh": "/Game/E.E", "materials": [], "faded_materials": [], "sockets": [],
    "rest": {"location": [-130, 70, 0], "quat": [0, 0, 0, 1], "scale": [1.1, 1.1, 1.1]}}]},
  {"key": "churchill|3", "type": "churchill", "faction": 3, "look": "vehicle", "parts": [{"name": "hull", "mesh": "/Game/C.C", "materials": [], "faded_materials": [], "sockets": []}]},
  {"key": "medium|0", "type": "medium", "faction": 0, "look": "vehicle", "parts": [{"name": "hull", "mesh": "/Game/S.S", "materials": [], "faded_materials": [], "sockets": []}]}
 ]})json");
}

// Run (after a build): -ExecCmds="Automation RunTests ThreeCrossroads.Models; Quit" (works under -nullrhi: no asset is loaded).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCModelLibraryTest, "ThreeCrossroads.Models.Library", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCModelLibraryTest::RunTest(const FString& Parameters)
{
	FTCModelLibrary Lib;
	FString Error;
	TestTrue(TEXT("catalog loads"), Lib.LoadFromString(TestCatalog, Error));
	TestEqual(TEXT("load error"), Error, FString());
	TestEqual(TEXT("models"), Lib.Num(), 6);

	// parts come parents first, whatever order the file lists them in
	const FTCModelEntry* Car = Lib.Find(TEXT("armoredcar|0"));
	if (!TestNotNull(TEXT("armored car"), Car)) return false;
	TestEqual(TEXT("hull first"), Car->Parts[0].Name, FName(TEXT("hull")));
	TestEqual(TEXT("turret second"), Car->Parts[1].Name, FName(TEXT("turret")));
	TestEqual(TEXT("turret socket"), Car->Parts[1].Socket, FName(TEXT("turret")));
	TestEqual(TEXT("faded slots"), Car->Parts[1].FadedMaterials.Num(), 1);

	ETCModelMatch Match;
	TestTrue(TEXT("exact"), Lib.Resolve({ TEXT("tank"), 1, TEXT("") }, &Match) == Lib.Find(TEXT("tank|1")) && Match == ETCModelMatch::Exact);
	// a UK light tank has no model of its own in the reference: the American one, in British paint
	TestFalse(TEXT("tank has no own UK model"), FTCModelLibrary::HasOwnUkModel(TEXT("tank")));
	TestTrue(TEXT("UK borrows the US tank"), Lib.Resolve({ TEXT("tank"), 3, TEXT("") }, &Match) == Lib.Find(TEXT("tank|0")) && Match == ETCModelMatch::Borrowed);
	// a UK Sherman slot (medium) has its own model (Cromwell); a missing one is a substitute, not the reference's picture
	TestTrue(TEXT("medium has its own UK model"), FTCModelLibrary::HasOwnUkModel(TEXT("medium")));
	TestTrue(TEXT("UK medium missing: substitute"), Lib.Resolve({ TEXT("medium"), 3, TEXT("") }, &Match) == Lib.Find(TEXT("medium|0")) && Match == ETCModelMatch::Substitute);
	TestTrue(TEXT("medics wear the engineer figure"), Lib.Resolve({ TEXT("medic"), 3, TEXT("") }, &Match) == Lib.Find(TEXT("engineer|3")) && Match == ETCModelMatch::Borrowed);
	TestTrue(TEXT("Soviet tank missing: lowest faction"), Lib.Resolve({ TEXT("tank"), 2, TEXT("") }, &Match) == Lib.Find(TEXT("tank|0")) && Match == ETCModelMatch::Substitute);
	TestTrue(TEXT("unknown type"), Lib.Resolve({ TEXT("kaiju"), 0, TEXT("") }, &Match) == nullptr && Match == ETCModelMatch::None);
	TestTrue(TEXT("variant must match"), Lib.Resolve({ TEXT("tank"), 1, TEXT("far") }, &Match) == nullptr && Match == ETCModelMatch::None);

	// custom data: owner seat color, then the faction's vehicle paint, fade, damage
	const TArray<float> D = Lib.CustomData(1, 3, 0.45f, 1.f);
	TestEqual(TEXT("custom data floats"), D.Num(), (int32)TCModelData::Count);
	TestEqual(TEXT("owner red"), D[TCModelData::OwnerR], 0.6038f);
	TestEqual(TEXT("owner blue"), D[TCModelData::OwnerB], 0.0273f);
	TestEqual(TEXT("UK vehicle paint"), D[TCModelData::VehicleR], 0.09f);
	TestEqual(TEXT("fade"), D[TCModelData::Fade], 0.45f);
	TestEqual(TEXT("damage"), D[TCModelData::Damage], 1.f);
	TestEqual(TEXT("unknown seat is neutral 0xdddddd"), Lib.SeatColor(7).R, 0.723f);
	TestEqual(TEXT("camouflage fade from the catalog"), Lib.CamouflageFade, 0.45f);

	// the turret hangs at its ring, turns about it, and the unit's heading carries both
	const FTransform Unit(FQuat(FVector::UpVector, PI / 2), FVector(1000, 0, 0));
	const FTransform TurretRest = FTCModelLibrary::PartTransform(*Car, 1, Unit);
	TestTrue(TEXT("turret ring in the world"), TurretRest.GetLocation().Equals(FVector(1000, 5, 155), 1e-3));
	TMap<FName, FQuat> Turns;
	Turns.Add(TEXT("turret"), FQuat(FVector::UpVector, PI / 2));
	const FTransform Turned = FTCModelLibrary::PartTransform(*Car, 1, Unit, Turns);
	TestTrue(TEXT("a turn keeps the ring in place"), Turned.GetLocation().Equals(FVector(1000, 5, 155), 1e-3));
	TestTrue(TEXT("unit heading plus turret traverse"), Turned.GetRotation().RotateVector(FVector::ForwardVector).Equals(FVector(-1, 0, 0), 1e-4));
	Turns.Reset();
	Turns.Add(TEXT("hull"), FQuat(FVector::UpVector, PI));
	TestTrue(TEXT("the turret rides a turned hull"), FTCModelLibrary::PartTransform(*Car, 1, Unit, Turns).GetLocation().Equals(FVector(1000, -5, 155), 1e-3));
	const FTCModelEntry* Squad = Lib.Find(TEXT("engineer|3"));
	TestTrue(TEXT("a man stands at his slot, scaled"), Squad && FTCModelLibrary::PartTransform(*Squad, 0, FTransform::Identity).Equals(FTransform(FQuat::Identity, FVector(-130, 70, 0), FVector(1.1f)), 1e-4));

	// what the loader refuses
	FTCModelLibrary Bad;
	TestFalse(TEXT("other custom data layout"), Bad.LoadFromString(FString(TestCatalog).Replace(TEXT("\"floats\": 8"), TEXT("\"floats\": 4")), Error));
	TestFalse(TEXT("socket the parent does not carry"), Bad.LoadFromString(FString(TestCatalog).Replace(TEXT("\"sockets\": [\"turret\"]"), TEXT("\"sockets\": []")), Error));
	TestFalse(TEXT("orphan part"), Bad.LoadFromString(FString(TestCatalog).Replace(TEXT("\"parent\": \"hull\""), TEXT("\"parent\": \"chassis\"")), Error));
	TestFalse(TEXT("not JSON"), Bad.LoadFromString(TEXT("{"), Error));

	// the imported catalog, when present: every part resolvable without loading assets
	FTCModelLibrary Real;
	if (Real.Load(Error))
	{
		AddInfo(FString::Printf(TEXT("Content/Models/ModelCatalog.json: %d models, reference %s"), Real.Num(), *Real.ReferenceCommit()));
		TestTrue(TEXT("imported catalog has models"), Real.Num() > 0);
	}
	else AddInfo(TEXT("no imported catalog: ") + Error);
	return true;
}
// Round 17: the motion vectors of moving instances (FTCInstanceHistory). Pure matching by key, then a component written
// twice: each instance's previous transform is last frame's by key, a new key, no key or a jump starts still, and a
// change of count keeps the previous transforms in step with the instances (the engine's ClearInstances does not).
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCInstanceHistoryTest, "ThreeCrossroads.Models.InstanceHistory", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)

bool FTCInstanceHistoryTest::RunTest(const FString& Parameters)
{
	using H = FTCInstanceHistory;
	TestNotEqual(TEXT("part and man keys differ"), H::PartKey(7, 0), H::ManKey(7, 0));
	TestNotEqual(TEXT("owner and selection rings differ"), H::RingKey(7, false), H::RingKey(7, true));
	TestNotEqual(TEXT("unit 0's first part is a key"), H::PartKey(0, 0), H::NoKey);
	TestNotEqual(TEXT("different units differ"), H::PartKey(1, 2), H::PartKey(2, 2));
	TMap<uint64, FTransform> Last;
	Last.Add(H::PartKey(1, 0), FTransform(FVector(0, 0, 0)));
	Last.Add(H::PartKey(2, 0), FTransform(FVector(0, 0, 0)));
	const TArray<FTransform> Cur = { FTransform(FVector(50, 0, 0)), FTransform(FVector(5000, 0, 0)), FTransform(FVector(10, 10, 0)), FTransform(FVector(20, 0, 0)) };
	const TArray<uint64> Keys = { H::PartKey(1, 0), H::PartKey(2, 0), H::PartKey(3, 0), H::NoKey };
	TArray<FTransform> Prev;
	TestEqual(TEXT("one match (the jump and the new ones start still)"), H::PreviousFor(Cur, Keys, Last, Prev), 1);
	TestTrue(TEXT("matched: last frame's place"), Prev.Num() == 4 && Prev[0].GetLocation().Equals(FVector::ZeroVector));
	TestTrue(TEXT("50 m jump: still"), Prev.Num() == 4 && Prev[1].GetLocation().Equals(Cur[1].GetLocation()));
	TestTrue(TEXT("new key: still"), Prev.Num() == 4 && Prev[2].GetLocation().Equals(Cur[2].GetLocation()));
	TestTrue(TEXT("no key: still"), Prev.Num() == 4 && Prev[3].GetLocation().Equals(Cur[3].GetLocation()));

	UInstancedStaticMeshComponent* Ism = NewObject<UInstancedStaticMeshComponent>(GetTransientPackage());
	H History;
	History.Write(Ism, { FTransform(FVector(0, 0, 0)), FTransform(FVector(100, 0, 0)), FTransform(FVector(200, 0, 0)) }, { H::ManKey(1, 0), H::ManKey(1, 1), H::ManKey(1, 2) });
	TestEqual(TEXT("first frame: three instances"), Ism->GetInstanceCount(), 3);
	TestEqual(TEXT("first frame: nothing to match"), History.Matched, 0);
	History.ResetCounts();
	// the squad moves 30 cm and loses its second man: two instances, each with its own man's last place
	History.Write(Ism, { FTransform(FVector(30, 0, 0)), FTransform(FVector(230, 0, 0)) }, { H::ManKey(1, 0), H::ManKey(1, 2) });
	FTransform P0, P1, C1;
	TestEqual(TEXT("second frame: two instances"), Ism->GetInstanceCount(), 2);
	TestTrue(TEXT("previous transforms kept for both"), Ism->GetInstancePrevTransform(0, P0, true) && Ism->GetInstancePrevTransform(1, P1, true));
	TestFalse(TEXT("no stale previous transform past the count"), Ism->GetInstancePrevTransform(2, C1, true));
	TestTrue(TEXT("man 0 came from 0"), P0.GetLocation().Equals(FVector(0, 0, 0)));
	TestTrue(TEXT("man 2 came from 200 (not man 1's 100)"), P1.GetLocation().Equals(FVector(200, 0, 0)));
	TestTrue(TEXT("current place written"), Ism->GetInstanceTransform(1, C1, true) && C1.GetLocation().Equals(FVector(230, 0, 0)));
	TestEqual(TEXT("both matched and moving"), History.Matched, 2);
	TestEqual(TEXT("both moving"), History.Moving, 2);
	History.ResetCounts();
	// standing still: the previous transform is the current one (no motion vector)
	History.Write(Ism, { FTransform(FVector(30, 0, 0)), FTransform(FVector(230, 0, 0)) }, { H::ManKey(1, 0), H::ManKey(1, 2) });
	TestTrue(TEXT("still: previous is current"), Ism->GetInstancePrevTransform(0, P0, true) && P0.GetLocation().Equals(FVector(30, 0, 0)));
	TestEqual(TEXT("still: none moving"), History.Moving, 0);
	History.Write(Ism, {}, {});
	TestEqual(TEXT("emptied"), Ism->GetInstanceCount(), 0);
	return true;
}
#endif // WITH_DEV_AUTOMATION_TESTS
