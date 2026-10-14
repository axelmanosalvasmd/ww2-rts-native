#include "Client/TCUnitTable.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

bool FTCUnitTable::Load(FString& OutError)
{
	const FString Path = FPaths::Combine(FPaths::ProjectContentDir(), TEXT("Data/units.json"));
	FString Text;
	if (!FFileHelper::LoadFileToString(Text, *Path)) { OutError = TEXT("cannot read ") + Path; return false; }
	TSharedPtr<FJsonObject> Root;
	if (!FJsonSerializer::Deserialize(TJsonReaderFactory<>::Create(Text), Root) || !Root.IsValid()) { OutError = TEXT("bad JSON in ") + Path; return false; }
	Root->TryGetStringField(TEXT("reference"), Reference);
	const TSharedPtr<FJsonObject>* Units;
	if (!Root->TryGetObjectField(TEXT("units"), Units)) { OutError = TEXT("no units in ") + Path; return false; }
	Defs.Reset();
	for (const auto& Pair : (*Units)->Values)
	{
		const TSharedPtr<FJsonObject>* O;
		if (!Pair.Value.IsValid() || !Pair.Value->TryGetObject(O)) continue;
		FTCUnitDef D;
		(*O)->TryGetStringField(TEXT("name"), D.Name);
		double V;
		if ((*O)->TryGetNumberField(TEXT("radius"), V)) D.Radius = V;
		if ((*O)->TryGetNumberField(TEXT("models"), V)) D.Models = FMath::Max(1, (int32)V);
		if ((*O)->TryGetNumberField(TEXT("hpPer"), V)) D.HpPer = (int32)V;
		if ((*O)->TryGetNumberField(TEXT("carries"), V)) D.Carries = (int32)V;
		(*O)->TryGetBoolField(TEXT("infantry"), D.bInfantry);
		(*O)->TryGetBoolField(TEXT("structure"), D.bStructure);
		(*O)->TryGetBoolField(TEXT("building"), D.bBuilding);
		(*O)->TryGetBoolField(TEXT("air"), D.bAir);
		(*O)->TryGetBoolField(TEXT("rail"), D.bRail);
		(*O)->TryGetBoolField(TEXT("back"), D.bBack);
		Defs.Add(FString(*Pair.Key), D); // the JSON object key is a shared string in 5.8
	}
	return Defs.Num() > 0;
}
