#include "WW2ViewReplay.h"
#include "WW2Core.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Interfaces/IPluginManager.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

namespace
{
	constexpr uint64 FnvBasis = 0xCBF29CE484222325ull;
	constexpr int32 MaxProblems = 8;

	void Feed(uint64& Hash, const uint8* Bytes, int64 Len)
	{
		for (int64 I = 0; I < Len; I++) Hash = (Hash ^ Bytes[I]) * 0x00000100000001B3ull;
	}
	void FeedByte(uint64& Hash, uint8 B) { Feed(Hash, &B, 1); }
	void FeedString(uint64& Hash, const FString& S)
	{
		const FTCHARToUTF8 Utf8(*S, S.Len());
		FeedByte(Hash, 's');
		Feed(Hash, reinterpret_cast<const uint8*>(Utf8.Get()), Utf8.Length());
		FeedByte(Hash, 0);
	}

	uint64 Bits(double X) { uint64 B; FMemory::Memcpy(&B, &X, sizeof B); return B; }

	// A JSON number that is a whole uint32 (as the protocol's ids, targets, flags and stars are).
	bool AsUint32(const TSharedPtr<FJsonValue>& V, uint32& Out)
	{
		if (!V.IsValid() || V->Type != EJson::Number) return false;
		const double D = V->AsNumber();
		if (!(D >= 0.0 && D <= 4294967295.0) || D != FMath::FloorToDouble(D)) return false;
		Out = (uint32)D;
		return true;
	}

	TSharedPtr<FJsonValue> ParseValue(const FString& Text)
	{
		TSharedPtr<FJsonValue> Value;
		TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Text);
		if (!FJsonSerializer::Deserialize(Reader, Value)) return nullptr;
		return Value;
	}

	// The file's lines as raw bytes (no trailing newline), exactly as recorded.
	bool ReadLines(const FString& Path, TArray<TArray<uint8>>& Lines)
	{
		TArray<uint8> Bytes;
		if (!FFileHelper::LoadFileToArray(Bytes, *Path, FILEREAD_Silent)) return false;
		int32 From = 0;
		for (int32 I = 0; I <= Bytes.Num(); I++)
		{
			if (I == Bytes.Num() || Bytes[I] == '\n')
			{
				if (I > From) Lines.Emplace(Bytes.GetData() + From, I - From);
				else if (I < Bytes.Num()) Lines.Emplace(); // an empty line inside the file stays a line
				From = I + 1;
			}
		}
		return true;
	}

	FString Utf8ToString(const TArray<uint8>& Line)
	{
		const FUTF8ToTCHAR Text(reinterpret_cast<const ANSICHAR*>(Line.GetData()), Line.Num());
		return FString::ConstructFromPtrSize(Text.Get(), Text.Length());
	}

	FString IdList(TConstArrayView<uint32> Ids)
	{
		FString S;
		for (int32 I = 0; I < Ids.Num() && I < 12; I++) S += FString::Printf(TEXT("%s%u"), I ? TEXT(",") : TEXT(""), Ids[I]);
		if (Ids.Num() > 12) S += FString::Printf(TEXT(",... (%d)"), Ids.Num());
		return S;
	}

	// The fixed columns of one ww2_view row against the raw wire row it came from; empty when they agree.
	FString CompareRow(const FWW2ViewUnit& U, const TArray<TSharedPtr<FJsonValue>>& Row)
	{
		if (Row.Num() < 15) return FString::Printf(TEXT("raw row has %d columns"), Row.Num());
		FString Kind;
		if (!Row[1].IsValid() || !Row[1]->TryGetString(Kind) || Kind != U.Kind) return FString::Printf(TEXT("kind '%s' vs raw '%s'"), *U.Kind, *Kind);
		const double Owner = Row[2].IsValid() && Row[2]->Type == EJson::Number ? Row[2]->AsNumber() : NAN;
		if (!(Owner == (double)U.Owner)) return FString::Printf(TEXT("owner %d vs raw %.17g"), U.Owner, Owner);
		const struct { int32 Col; double Got; const TCHAR* Name; } Doubles[] = {
			{ 3, U.X, TEXT("x") }, { 4, U.Z, TEXT("z") }, { 5, U.Rot, TEXT("rot") }, { 6, U.Aim, TEXT("aim") }, { 7, U.Hp, TEXT("hp") },
			{ 8, U.Supp, TEXT("supp") }, { 11, U.Cd, TEXT("cd") }, { 14, U.Built, TEXT("built") } };
		for (const auto& D : Doubles)
		{
			if (!Row[D.Col].IsValid() || Row[D.Col]->Type != EJson::Number) return FString::Printf(TEXT("%s: raw column %d is not a number"), D.Name, D.Col);
			const double Raw = Row[D.Col]->AsNumber();
			if (Bits(Raw) != Bits(D.Got)) return FString::Printf(TEXT("%s %.17g vs raw %.17g"), D.Name, D.Got, Raw);
		}
		const struct { int32 Col; uint32 Got; const TCHAR* Name; } Ints[] = {
			{ 9, U.Target, TEXT("target") }, { 10, U.Cover, TEXT("cover") }, { 12, U.Flags, TEXT("flags") }, { 13, U.Stars, TEXT("stars") } };
		for (const auto& I : Ints)
		{
			uint32 Raw = 0;
			if (!AsUint32(Row[I.Col], Raw) || Raw != I.Got) return FString::Printf(TEXT("%s %u vs raw column %d"), I.Name, I.Got, I.Col);
		}
		return FString();
	}

	struct FEdges
	{
		TArray<FString>& Lines;
		TArray<FString>& Failures;
		void Check(bool bOk, const FString& What)
		{
			Lines.Add(FString::Printf(TEXT("%s %s"), bOk ? TEXT("ok  ") : TEXT("FAIL"), *What));
			if (!bOk) Failures.Add(What);
		}
		void Code(const FWW2Result& R, int32 Want, const FString& What)
		{
			Check(R.Code == Want, FString::Printf(TEXT("%s: %d %s (want %d %s)%s"), *What, R.Code, FWW2Core::CodeName(R.Code), Want, FWW2Core::CodeName(Want),
				R.Error.IsEmpty() ? TEXT("") : *FString::Printf(TEXT(": %s"), *R.Error)));
		}
	};

	FString Row(uint64 Id, const TCHAR* Kind, int64 Owner, uint64 Target, uint64 Flags)
	{
		return FString::Printf(TEXT("[%llu,\"%s\",%lld,1.5,-2.25,0.5,-0.0,87.5,3,%llu,0,1.25,%llu,2,0.75]"), Id, Kind, Owner, Target, Flags);
	}
}

void FWW2ViewReplay::CanonicalHash(uint64& Hash, const TSharedPtr<FJsonValue>& Value)
{
	if (!Value.IsValid()) { FeedByte(Hash, 'n'); return; }
	switch (Value->Type)
	{
	case EJson::Boolean: FeedByte(Hash, Value->AsBool() ? 't' : 'f'); break;
	case EJson::Number:
	{
		const double D = Value->AsNumber();
		const uint64 B = Bits(D == 0.0 ? 0.0 : D); // negative zero hashes as zero
		FeedByte(Hash, 'd');
		for (int32 I = 0; I < 8; I++) FeedByte(Hash, (uint8)(B >> (8 * I)));
		break;
	}
	case EJson::String: FeedString(Hash, Value->AsString()); break;
	case EJson::Array:
		FeedByte(Hash, 'a');
		for (const TSharedPtr<FJsonValue>& V : Value->AsArray()) CanonicalHash(Hash, V);
		FeedByte(Hash, ']');
		break;
	case EJson::Object:
	{
		const TSharedPtr<FJsonObject> O = Value->AsObject();
		TArray<TPair<FString, TSharedPtr<FJsonValue>>> Fields;
		for (const auto& P : O->Values) Fields.Emplace(FString::ConstructFromPtrSize(*P.Key, P.Key.Len()), P.Value);
		// JavaScript's sort: UTF-16 code units, which is what TCHAR holds.
		Fields.Sort([](const TPair<FString, TSharedPtr<FJsonValue>>& FA, const TPair<FString, TSharedPtr<FJsonValue>>& FB)
		{
			const FString& A = FA.Key;
			const FString& B = FB.Key;
			const int32 N = FMath::Min(A.Len(), B.Len());
			for (int32 I = 0; I < N; I++) if (A[I] != B[I]) return (uint16)A[I] < (uint16)B[I];
			return A.Len() < B.Len();
		});
		FeedByte(Hash, 'o');
		for (const auto& F : Fields) { FeedString(Hash, F.Key); CanonicalHash(Hash, F.Value); }
		FeedByte(Hash, '}');
		break;
	}
	default: FeedByte(Hash, 'n'); break;
	}
}

TArray<FString> FWW2ViewReplay::Streams()
{
	return { TEXT("protocol-conquest-p0"), TEXT("protocol-conquest-p1"), TEXT("protocol-world-p0"), TEXT("protocol-world-p1") };
}

FString FWW2ViewReplay::ProtocolDir()
{
#if PLATFORM_LINUX && PLATFORM_CPU_X86_FAMILY
	const TSharedPtr<IPlugin> Plugin = IPluginManager::Get().FindPlugin(TEXT("WW2Core"));
	if (!Plugin) return FString();
	return FPaths::ConvertRelativePathToFull(Plugin->GetBaseDir() / TEXT("ThirdParty/ww2_ffi/Linux/x86_64-unknown-linux-gnu/fixtures/protocol"));
#else
	return FString();
#endif
}

bool FWW2ReplayReport::Passed() const
{
	return Fatal.IsEmpty() && Messages > 0 && Messages == ExpectedLines && ApplyErrors == 0 && IdMismatches == 0 && CountMismatches == 0 && ResyncMismatches == 0
		&& ModelIdMismatches == 0 && ModelHashMismatches == 0 && RowMismatches == 0 && ReadErrors == 0;
}

FString FWW2ReplayReport::Summary() const
{
	if (!Fatal.IsEmpty()) return FString::Printf(TEXT("%s: could not run: %s"), *Stream, *Fatal);
	return FString::Printf(TEXT("%s: %s; %d messages (%d starts, %d snapshots), %d oracle lines; apply errors %d; ww2_view vs oracle: id mismatches %d, resync mismatches %d (oracle resync true after %d); ")
		TEXT("model vs oracle: id mismatches %d, units-hash mismatches %d; ww2_view vs model: %lld rows compared, %d differ; count mismatches %d, read errors %d; final units %d"),
		*Stream, Passed() ? TEXT("PASS") : TEXT("FAIL"), Messages, Starts, Snapshots, ExpectedLines, ApplyErrors, IdMismatches, ResyncMismatches, ResyncTrue,
		ModelIdMismatches, ModelHashMismatches, (long long)RowsCompared, RowMismatches, CountMismatches, ReadErrors, FinalUnits);
}

EWW2Fixtures FWW2ViewReplay::FindFixtures(const FString& Dir, const FString& Stem)
{
	if (Dir.IsEmpty()) return EWW2Fixtures::Absent;
	const bool bMessages = FPaths::FileExists(Dir / (Stem + TEXT(".jsonl")));
	const bool bOracle = FPaths::FileExists(Dir / (Stem + TEXT(".expected.jsonl")));
	return bMessages && bOracle ? EWW2Fixtures::Present : !bMessages && !bOracle ? EWW2Fixtures::Absent : EWW2Fixtures::Partial;
}

bool FWW2ViewReplay::Run(const FString& Stem, FWW2ReplayReport& Out)
{
	return RunIn(ProtocolDir(), Stem, Out);
}

bool FWW2ViewReplay::RunIn(const FString& Dir, const FString& Stem, FWW2ReplayReport& Out)
{
	Out = FWW2ReplayReport();
	Out.Stream = Stem;
	if (Dir.IsEmpty()) { Out.Fatal = TEXT("no protocol fixtures on this platform"); return false; }
	TArray<TArray<uint8>> Messages, Expected;
	if (!ReadLines(Dir / (Stem + TEXT(".jsonl")), Messages)) { Out.Fatal = FString::Printf(TEXT("cannot read %s"), *(Dir / (Stem + TEXT(".jsonl")))); return false; }
	if (!ReadLines(Dir / (Stem + TEXT(".expected.jsonl")), Expected)) { Out.Fatal = FString::Printf(TEXT("cannot read %s"), *(Dir / (Stem + TEXT(".expected.jsonl")))); return false; }
	Out.ExpectedLines = Expected.Num();

	FWW2View View;
	const FWW2Result Opened = View.Open();
	if (!Opened.IsOk()) { Out.Fatal = FString::Printf(TEXT("ww2_view_new: %d %s"), Opened.Code, *Opened.Error); return false; }

	auto Problem = [&Out](const FString& P) { if (Out.Problems.Num() < MaxProblems) Out.Problems.Add(P); };
	TMap<uint32, TSharedPtr<FJsonValue>> Model; // raw rows held, by id

	for (int32 L = 0; L < Messages.Num(); L++)
	{
		const int32 LineNo = L + 1;
		Out.Messages++;
		// 1. The library gets the recorded bytes unchanged.
		const FWW2Result Applied = View.Apply(TConstArrayView<UTF8CHAR>(reinterpret_cast<const UTF8CHAR*>(Messages[L].GetData()), Messages[L].Num()));
		if (!Applied.IsOk()) { Out.ApplyErrors++; Problem(FString::Printf(TEXT("%s:%d: apply %d %s"), *Stem, LineNo, Applied.Code, *Applied.Error)); }

		// 2. The independent model, from Unreal's parse of the same line.
		const TSharedPtr<FJsonValue> Msg = ParseValue(Utf8ToString(Messages[L]));
		const TSharedPtr<FJsonObject> M = Msg.IsValid() && Msg->Type == EJson::Object ? Msg->AsObject() : nullptr;
		if (!M) { Out.Fatal = FString::Printf(TEXT("%s:%d: Unreal's JSON parser cannot read the message"), *Stem, LineNo); return false; }
		FString T;
		M->TryGetStringField(TEXT("t"), T);
		if (T == TEXT("start")) { Out.Starts++; Model.Reset(); }
		else if (T == TEXT("s"))
		{
			Out.Snapshots++;
			bool bAll = false;
			M->TryGetBoolField(TEXT("all"), bAll);
			const TSharedPtr<FJsonValue> Held = M->TryGetField(TEXT("held"));
			if (bAll || !Held.IsValid() || Held->Type == EJson::Null) Model.Reset();
			const TArray<TSharedPtr<FJsonValue>>* Arr;
			if (M->TryGetArrayField(TEXT("gone"), Arr)) for (const TSharedPtr<FJsonValue>& V : *Arr) { uint32 Id; if (AsUint32(V, Id)) Model.Remove(Id); }
			if (M->TryGetArrayField(TEXT("units"), Arr))
				for (const TSharedPtr<FJsonValue>& V : *Arr)
				{
					uint32 Id;
					if (V.IsValid() && V->Type == EJson::Array && V->AsArray().Num() > 0 && AsUint32(V->AsArray()[0], Id)) Model.Add(Id, V);
					else { Out.Fatal = FString::Printf(TEXT("%s:%d: a unit row without a uint32 id"), *Stem, LineNo); return false; }
				}
		}
		Model.KeySort([](uint32 A, uint32 B) { return A < B; });
		TArray<uint32> ModelIds;
		Model.GetKeys(ModelIds);

		// 3. The oracle's report for this message.
		if (L >= Expected.Num()) { Out.Fatal = FString::Printf(TEXT("%s: no oracle line for message %d"), *Stem, LineNo); return false; }
		const TSharedPtr<FJsonValue> Want = ParseValue(Utf8ToString(Expected[L]));
		const TSharedPtr<FJsonObject> W = Want.IsValid() && Want->Type == EJson::Object ? Want->AsObject() : nullptr;
		const TArray<TSharedPtr<FJsonValue>>* WantIdsJson;
		FString WantHash;
		bool bWantResync = false;
		if (!W || !W->TryGetArrayField(TEXT("ids"), WantIdsJson) || !W->TryGetStringField(TEXT("units"), WantHash) || !W->TryGetBoolField(TEXT("resync"), bWantResync))
		{
			Out.Fatal = FString::Printf(TEXT("%s.expected:%d: not an oracle report"), *Stem, LineNo);
			return false;
		}
		TArray<uint32> WantIds;
		for (const TSharedPtr<FJsonValue>& V : *WantIdsJson) { uint32 Id; if (!AsUint32(V, Id)) { Out.Fatal = FString::Printf(TEXT("%s.expected:%d: bad id"), *Stem, LineNo); return false; } WantIds.Add(Id); }
		Out.ResyncTrue += bWantResync ? 1 : 0;

		// The model must be the oracle's view of the rows before it can judge the library's.
		if (ModelIds != WantIds) { Out.ModelIdMismatches++; Problem(FString::Printf(TEXT("%s:%d: model ids [%s], oracle [%s]"), *Stem, LineNo, *IdList(ModelIds), *IdList(WantIds))); }
		uint64 Hash = FnvBasis;
		FeedByte(Hash, 'a');
		for (const TPair<uint32, TSharedPtr<FJsonValue>>& P : Model) CanonicalHash(Hash, P.Value);
		FeedByte(Hash, ']');
		const FString ModelHash = FString::Printf(TEXT("%016llx"), (unsigned long long)Hash);
		if (ModelHash != WantHash) { Out.ModelHashMismatches++; Problem(FString::Printf(TEXT("%s:%d: model units hash %s, oracle %s"), *Stem, LineNo, *ModelHash, *WantHash)); }

		// 4. The library's view against both.
		TArray<FWW2ViewUnit> Rows;
		int32 Count = -1;
		bool bResync = false;
		const FWW2Result RU = View.Units(Rows), RC = View.UnitCount(Count), RR = View.NeedsResync(bResync);
		if (!RU.IsOk() || !RC.IsOk() || !RR.IsOk())
		{
			Out.ReadErrors++;
			Problem(FString::Printf(TEXT("%s:%d: reads failed: units %d %s, count %d %s, resync %d %s"), *Stem, LineNo, RU.Code, *RU.Error, RC.Code, *RC.Error, RR.Code, *RR.Error));
			continue;
		}
		if (Count != Rows.Num()) { Out.CountMismatches++; Problem(FString::Printf(TEXT("%s:%d: unit_count %d, rows %d"), *Stem, LineNo, Count, Rows.Num())); }
		TArray<uint32> Ids;
		for (const FWW2ViewUnit& U : Rows) Ids.Add(U.Id);
		if (Ids != WantIds) { Out.IdMismatches++; Problem(FString::Printf(TEXT("%s:%d: ww2_view ids [%s], oracle [%s]"), *Stem, LineNo, *IdList(Ids), *IdList(WantIds))); }
		if (bResync != bWantResync) { Out.ResyncMismatches++; Problem(FString::Printf(TEXT("%s:%d: needs_resync %d, oracle %d"), *Stem, LineNo, bResync, bWantResync)); }
		for (const FWW2ViewUnit& U : Rows)
		{
			const TSharedPtr<FJsonValue>* Raw = Model.Find(U.Id);
			Out.RowsCompared++;
			const FString Diff = Raw ? CompareRow(U, (*Raw)->AsArray()) : FString(TEXT("no raw row in the model"));
			if (!Diff.IsEmpty()) { Out.RowMismatches++; Problem(FString::Printf(TEXT("%s:%d: unit %u: %s"), *Stem, LineNo, U.Id, *Diff)); }
		}
		Out.FinalUnits = Rows.Num();
	}
	if (Messages.Num() != Expected.Num()) Problem(FString::Printf(TEXT("%s: %d messages, %d oracle lines"), *Stem, Messages.Num(), Expected.Num()));
	const FWW2Result Closed = View.Close();
	if (!Closed.IsOk()) { Out.ReadErrors++; Problem(FString::Printf(TEXT("%s: free %d %s"), *Stem, Closed.Code, *Closed.Error)); }
	return Out.Passed();
}

bool FWW2ViewReplay::RunEdges(TArray<FString>& Lines, TArray<FString>& Failures)
{
	FEdges E{ Lines, Failures };
	FWW2View V;
	int32 Count = -1;
	bool bResync = true;
	TArray<FWW2ViewUnit> Rows;
	E.Code(V.UnitCount(Count), FWW2Core::ErrClosed, TEXT("a view never opened refuses calls"));
	E.Code(V.Open(), FWW2Core::Ok, TEXT("ww2_view_new"));
	if (!V.IsOpen()) return false;
	E.Code(V.UnitCount(Count), FWW2Core::Ok, TEXT("empty view: unit_count"));
	E.Check(Count == 0, FString::Printf(TEXT("empty view: 0 units (%d)"), Count));
	E.Code(V.NeedsResync(bResync), FWW2Core::Ok, TEXT("empty view: needs_resync"));
	E.Check(!bResync, TEXT("empty view: no resync"));
	Rows.SetNum(2);
	E.Code(V.Units(Rows), FWW2Core::Ok, TEXT("empty view: units"));
	E.Check(Rows.Num() == 0, FString::Printf(TEXT("empty view: no rows (%d)"), Rows.Num()));

	// Empty and invalid input: parse errors, and the view stays empty.
	E.Code(V.Apply(TConstArrayView<UTF8CHAR>()), FWW2Core::ErrParse, TEXT("zero-byte message"));
	E.Code(V.Apply(FString(TEXT("{"))), FWW2Core::ErrParse, TEXT("truncated JSON"));
	E.Code(V.Apply(FString(TEXT("[]"))), FWW2Core::ErrParse, TEXT("JSON that is not an object"));
	E.Code(V.Apply(FString(TEXT("{\"x\":1}"))), FWW2Core::ErrParse, TEXT("an object without t"));
	const UTF8CHAR BadUtf8[] = { '{', '"', 't', '"', ':', '"', (UTF8CHAR)0xFF, '"', '}' };
	E.Code(V.Apply(TConstArrayView<UTF8CHAR>(BadUtf8, UE_ARRAY_COUNT(BadUtf8))), FWW2Core::ErrParse, TEXT("invalid UTF-8"));
	E.Code(V.Apply(FString(TEXT("{\"t\":\"s\",\"units\":[],\"held\":[0,0]}"))), FWW2Core::ErrWrongState, TEXT("snapshot before start"));
	E.Code(V.Apply(FString(TEXT("{\"t\":\"start\"}"))), FWW2Core::ErrParse, TEXT("start without map"));
	E.Code(V.Apply(FString(TEXT("{\"t\":\"start\",\"map\":{\"w\":0,\"h\":1}}"))), FWW2Core::ErrBadArgument, TEXT("start with a zero-width map (decoder bound)"));
	E.Code(V.Apply(FString(TEXT("{\"t\":\"later-protocol\"}"))), FWW2Core::Ok, TEXT("an unknown message type is ignored"));
	E.Code(V.UnitCount(Count), FWW2Core::Ok, TEXT("after invalid input: unit_count"));
	E.Check(Count == 0, TEXT("after invalid input: still 0 units"));

	// Full-width ids: 1, 2^31 and 2^32-1 (above INT32_MAX), a full-width target and flags, a negative owner.
	E.Code(V.Apply(FString(TEXT("{\"t\":\"start\",\"map\":{\"w\":2,\"h\":2}}"))), FWW2Core::Ok, TEXT("start (2x2 map)"));
	const FString Full = FString::Printf(TEXT("{\"t\":\"s\",\"units\":[%s,%s,%s],\"held\":[3,2147483646]}"),
		*Row(4294967295ull, TEXT("rifle"), 0, 2147483648ull, 4294967295ull), *Row(2147483648ull, TEXT("tank"), -1, 0, 0), *Row(1, TEXT("mg"), 1, 4294967295ull, 32));
	E.Code(V.Apply(Full), FWW2Core::Ok, TEXT("snapshot with full-width ids, held [3, xor as int32]"));
	E.Code(V.Units(Rows), FWW2Core::Ok, TEXT("full width: units"));
	E.Check(Rows.Num() == 3 && Rows[0].Id == 1 && Rows[1].Id == 2147483648u && Rows[2].Id == 4294967295u,
		FString::Printf(TEXT("full width: ascending ids 1, 2147483648, 4294967295 (%s)"), *FString::JoinBy(Rows, TEXT(","), [](const FWW2ViewUnit& U) { return FString::Printf(TEXT("%u"), U.Id); })));
	if (Rows.Num() == 3)
	{
		const FWW2ViewUnit& Big = Rows[2];
		E.Check(Big.Kind == TEXT("rifle") && Big.Owner == 0 && Big.Target == 2147483648u && Big.Flags == 4294967295u && Big.X == 1.5 && Big.Z == -2.25 && Big.Rot == 0.5
			&& Big.Hp == 87.5 && Big.Supp == 3 && Big.Cd == 1.25 && Big.Stars == 2 && Big.Built == 0.75, TEXT("full width: id 4294967295's columns (target 2^31, flags 2^32-1)"));
		E.Check(Bits(Big.Aim) == Bits(-0.0), TEXT("full width: aim keeps negative zero's bits"));
		E.Check(Rows[1].Owner == -1 && Rows[1].Kind == TEXT("tank"), TEXT("full width: owner -1"));
		E.Check(Rows[0].Target == 4294967295u && Rows[0].Flags == 32, TEXT("full width: id 1's target 2^32-1"));
	}
	E.Code(V.NeedsResync(bResync), FWW2Core::Ok, TEXT("full width: needs_resync"));
	E.Check(!bResync, TEXT("full width: the held xor of uint32 ids read as int32 matches"));
	E.Code(V.Apply(FString(TEXT("{\"t\":\"s\",\"units\":[],\"gone\":[2147483648],\"held\":[2,-2]}"))), FWW2Core::Ok, TEXT("delta: gone 2147483648, held [2, -2]"));
	E.Code(V.UnitCount(Count), FWW2Core::Ok, TEXT("delta: unit_count"));
	E.Check(Count == 2, FString::Printf(TEXT("delta: 2 units (%d)"), Count));
	E.Code(V.NeedsResync(bResync), FWW2Core::Ok, TEXT("delta: needs_resync"));
	E.Check(!bResync, TEXT("delta: held matches"));
	E.Code(V.Apply(FString(TEXT("{\"t\":\"s\",\"units\":[],\"held\":[5,0]}"))), FWW2Core::Ok, TEXT("held mismatch applies"));
	E.Code(V.NeedsResync(bResync), FWW2Core::Ok, TEXT("held mismatch: needs_resync"));
	E.Check(bResync, TEXT("held mismatch: resync is asked for"));
	E.Code(V.Apply(FString::Printf(TEXT("{\"t\":\"s\",\"units\":[%s],\"held\":[1,1]}"), *Row(4294967296ull, TEXT("rifle"), 0, 0, 0))), FWW2Core::ErrParse, TEXT("id 2^32 does not fit uint32"));
	E.Code(V.Apply(FString(TEXT("{\"t\":\"s\",\"units\":[],\"held\":[1,2147483648]}"))), FWW2Core::ErrParse, TEXT("held xor 2^31 does not fit int32"));
	E.Code(V.UnitCount(Count), FWW2Core::Ok, TEXT("after rejected snapshots: unit_count"));
	E.Check(Count == 2, TEXT("after rejected snapshots: still the same 2 units"));
	E.Code(V.NeedsResync(bResync), FWW2Core::Ok, TEXT("after rejected snapshots: needs_resync"));
	E.Check(bResync, TEXT("after rejected snapshots: the resync flag is unchanged"));

	// The kind limit: 63 UTF-8 bytes fit; 64 make ww2_view_units fail without touching the caller's rows.
	const FString Kind63 = FString::ChrN(63, TEXT('k')), Kind64 = FString::ChrN(64, TEXT('k'));
	E.Code(V.Apply(FString::Printf(TEXT("{\"t\":\"s\",\"all\":true,\"units\":[%s],\"held\":[1,7]}"), *Row(7, *Kind63, 0, 0, 0))), FWW2Core::Ok, TEXT("all: one unit with a 63-byte kind"));
	E.Code(V.Units(Rows), FWW2Core::Ok, TEXT("63-byte kind: units"));
	E.Check(Rows.Num() == 1 && Rows[0].Kind == Kind63, TEXT("63-byte kind: read back whole"));
	E.Code(V.Apply(FString::Printf(TEXT("{\"t\":\"s\",\"units\":[%s],\"held\":[2,15]}"), *Row(8, *Kind64, 0, 0, 0))), FWW2Core::Ok, TEXT("a 64-byte kind is held"));
	TArray<FWW2ViewUnit> Sentinel;
	Sentinel.SetNum(1);
	Sentinel[0].Id = 99; Sentinel[0].Kind = TEXT("sentinel");
	E.Code(V.Units(Sentinel), FWW2Core::ErrBadArgument, TEXT("64-byte kind: units refuses"));
	E.Check(Sentinel.Num() == 1 && Sentinel[0].Id == 99 && Sentinel[0].Kind == TEXT("sentinel"), TEXT("64-byte kind: no partial write, the caller's rows are untouched"));
	E.Code(V.UnitCount(Count), FWW2Core::Ok, TEXT("64-byte kind: the view still answers (not poisoned)"));
	E.Check(Count == 2 && !V.IsPoisoned(), TEXT("64-byte kind: 2 units held, view not poisoned"));

	// Reconnect: a start replaces the view, even for the same match.
	E.Code(V.Apply(FString(TEXT("{\"t\":\"start\",\"matchId\":1,\"map\":{\"w\":2,\"h\":2}}"))), FWW2Core::Ok, TEXT("a second start (reconnect)"));
	E.Code(V.UnitCount(Count), FWW2Core::Ok, TEXT("reconnect: unit_count"));
	E.Code(V.NeedsResync(bResync), FWW2Core::Ok, TEXT("reconnect: needs_resync"));
	E.Check(Count == 0 && !bResync, TEXT("reconnect: 0 units, no resync"));

	// Ownership: a move hands the handle over; close frees it once; a closed view refuses calls.
	FWW2View Moved(MoveTemp(V));
	E.Check(!V.IsOpen() && Moved.IsOpen(), TEXT("move: the handle moved, the source is closed"));
	E.Code(V.UnitCount(Count), FWW2Core::ErrClosed, TEXT("move: the source refuses calls"));
	E.Code(Moved.UnitCount(Count), FWW2Core::Ok, TEXT("move: the destination answers"));
	E.Code(Moved.Close(), FWW2Core::Ok, TEXT("ww2_view_free"));
	E.Code(Moved.Close(), FWW2Core::Ok, TEXT("a second close is a no-op (never a double free)"));
	E.Code(Moved.Apply(FString(TEXT("{\"t\":\"later\"}"))), FWW2Core::ErrClosed, TEXT("closed: apply refused without a call"));
	return Failures.Num() == 0;
}

bool FWW2ViewReplay::StartupSelfCheck(FString& Summary)
{
	return StartupSelfCheckIn(ProtocolDir(), Summary);
}

bool FWW2ViewReplay::StartupSelfCheckIn(const FString& Dir, FString& Summary, bool* bOutReplaySkipped)
{
	if (bOutReplaySkipped) *bOutReplaySkipped = false;
	if (!FWW2Core::IsReady()) { Summary = FString::Printf(TEXT("view self-check: FAIL, bridge %s"), *FWW2Core::Describe()); return false; }
	const FString Stem = SelfCheckStream();
	FWW2ReplayReport Report;
	bool bReplay = false;
	FString ReplayText;
	switch (FindFixtures(Dir, Stem))
	{
	case EWW2Fixtures::Absent:
		// A package made before tc-package-linux.sh staged the stream, or an install without it: skip, say so, and let
		// the edge checks (which need no file) decide.
		bReplay = true;
		if (bOutReplaySkipped) *bOutReplaySkipped = true;
		ReplayText = FString::Printf(TEXT("%s: SKIPPED, the recorded stream is not installed or staged at %s"), *Stem, Dir.IsEmpty() ? TEXT("(no folder on this platform)") : *Dir);
		UE_LOG(LogWW2Core, Log, TEXT("view self-check: replay skipped: %s and %s.expected.jsonl are not at %s (unreal/Tools/tc-package-linux.sh stages them into a package); the edge checks still run"),
			*(Stem + TEXT(".jsonl")), *Stem, Dir.IsEmpty() ? TEXT("(no folder on this platform)") : *Dir);
		break;
	case EWW2Fixtures::Partial:
		ReplayText = FString::Printf(TEXT("%s: FAIL, only one of %s.jsonl and %s.expected.jsonl is at %s (a partial install or staging)"), *Stem, *Stem, *Stem, *Dir);
		break;
	case EWW2Fixtures::Present:
		bReplay = RunIn(Dir, Stem, Report);
		ReplayText = Report.Summary();
		break;
	}
	TArray<FString> Lines, Failures;
	const bool bEdges = RunEdges(Lines, Failures);
	Summary = FString::Printf(TEXT("view self-check: %s. Replay %s. Edges: %d of %d checks pass%s%s"), bReplay && bEdges ? TEXT("PASS") : TEXT("FAIL"), *ReplayText,
		Lines.Num() - Failures.Num(), Lines.Num(), Failures.Num() ? TEXT("; failed: ") : TEXT(""), *FString::Join(Failures, TEXT("; ")));
	for (const FString& P : Report.Problems) Summary += TEXT(" | ") + P;
	return bReplay && bEdges;
}
