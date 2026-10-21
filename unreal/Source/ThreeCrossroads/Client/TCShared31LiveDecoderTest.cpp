// Opt-in real socket integration for CLI-137. The lead supplies an isolated reference server.
// No server is launched here, and no Rust-host profile restriction is bypassed.
#include "Net/TCConnection.h"
#include "Net/TCMatchState.h"
#include "Net/TCRustViewShadow.h"
#include "Net/TCSession30Policy.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "HAL/PlatformMisc.h"
#include "HAL/PlatformTime.h"
#include "Misc/AutomationTest.h"
#include "Misc/Guid.h"

#if WITH_DEV_AUTOMATION_TESTS
namespace TCShared31Live
{
	struct FSeat
	{
		FTCMatchState State;
		FTCMatchState Cpp;
		FTCRustViewShadow Rust;
		TSharedPtr<FTCConnection> Connection;
		int32 Starts = 0, Snapshots = 0, FromRust = 0, FogOrMapDifferences = 0;
		bool bDrift = false;
		void Receive(const FJsonObject& M)
		{
			const FString T = M.GetStringField(TEXT("t"));
			if (T == TEXT("lobby")) { State.ApplyLobby(M); Cpp.ApplyLobby(M); }
			else if (T == TEXT("start"))
			{
				Cpp.ApplyStart(M);
				const bool bOk = State.ApplyStart(M);
				Rust.AfterStart(State, bOk, false, ++Starts);
			}
			else if (T == TEXT("s"))
			{
				const bool bCppDrift = Cpp.ApplySnapshot(M);
				const bool bRustDrift = State.ApplySnapshot(M);
				Rust.AfterSnapshot(State, bRustDrift);
				bDrift |= bCppDrift || bRustDrift;
				Snapshots++;
				FromRust += State.UnitsFromRustView() ? 1 : 0;
				FogOrMapDifferences += State.FogVisible != Cpp.FogVisible || State.FogExplored != Cpp.FogExplored ||
					State.Map.Chars != Cpp.Map.Chars || State.Map.Levels != Cpp.Map.Levels || State.Map.Known != Cpp.Map.Known;
			}
		}
	};

	class FRun : public IAutomationLatentCommand
	{
	public:
		FRun(FAutomationTestBase* InTest, const FString& Url) : Test(InTest), Started(FPlatformTime::Seconds())
		{
			const FString Room = TEXT("s31") + FGuid::NewGuid().ToString(EGuidFormats::Digits).Left(9);
			for (int32 I = 0; I < 2; I++)
			{
				FSeat& S = Seats[I];
				Test->TestTrue(TEXT("live Rust view opens"), S.Rust.BeginMode(ETCDecoder::Rust, &S.State, true, false));
				FTCConnection::FSettings Settings;
				Settings.Url = Url; Settings.Room = Room; Settings.Name = I == 0 ? TEXT("Shared31A") : TEXT("Shared31B");
				Settings.Token = FGuid::NewGuid().ToString(EGuidFormats::Digits);
				S.Connection = MakeShared<FTCConnection>(Settings);
				S.Connection->OnText = [Seat = &S](const FString& Text) { Seat->Rust.Feed(Text); };
				S.Connection->OnMessage = [Seat = &S](const FJsonObject& M, int32) { Seat->Receive(M); };
				S.Connection->Start();
			}
		}
		~FRun()
		{
			for (FSeat& S : Seats) { S.Connection->Stop(); S.Rust.End(); }
		}
		bool Update() override
		{
			const double Now = FPlatformTime::Seconds();
			for (FSeat& S : Seats) S.Connection->Tick(Now);
			FSeat& Host = Seats[0];
			if (!bStartSent && Host.State.bAmHost && Host.State.Roster.Num() == 2)
			{
				// Reference only. Rust requires a separate qualified endpoint and two-native R1 lane.
				if (Host.Connection->HostKind() != ETCHostKind::Reference)
				{
					Test->AddError(TEXT("LiveDecoder requires an isolated reference endpoint"));
					return true;
				}
				auto M = MakeShared<FJsonObject>(); M->SetStringField(TEXT("t"), TEXT("start"));
				bStartSent = Host.Connection->Send(M);
			}
			if (!bMoveSent && Host.Snapshots >= 2)
			{
				for (const auto& Pair : Host.State.Units)
				{
					const FTCUnit& U = Pair.Value;
					if (!Host.State.IsOwn(U) || U.Type != TEXT("rifle")) continue;
					MoveId = U.Id; MoveFrom = FVector2D(U.X, U.Z);
					auto M = MakeShared<FJsonObject>(); M->SetStringField(TEXT("t"), TEXT("move"));
					TArray<TSharedPtr<FJsonValue>> Order = { MakeShared<FJsonValueNumber>(U.Id), MakeShared<FJsonValueNumber>(U.X + 8), MakeShared<FJsonValueNumber>(U.Z) };
					M->SetArrayField(TEXT("orders"), { MakeShared<FJsonValueArray>(Order) });
					bMoveSent = Host.Connection->Send(M);
					break;
				}
			}
			if (bMoveSent)
				if (const FTCUnit* U = Host.State.Units.Find(MoveId)) bMoved |= FVector2D::Distance(MoveFrom, FVector2D(U->X, U->Z)) > 0.1;
			const bool bDone = bMoved && Seats[0].Snapshots >= 20 && Seats[1].Snapshots >= 20;
			if (!bDone && Now - Started < 30.0) return false;
			Test->TestTrue(TEXT("live match started and delivered movement within 30 seconds"), bDone);
			for (int32 I = 0; I < 2; I++)
			{
				const FSeat& S = Seats[I]; const auto& C = S.Rust.Totals();
				Test->TestEqual(TEXT("one start per seat"), S.Starts, 1);
				Test->TestTrue(TEXT("at least 20 live snapshots per seat"), S.Snapshots >= 20);
				Test->TestEqual(TEXT("every live snapshot adopted"), S.FromRust, S.Snapshots);
				Test->TestEqual(TEXT("every live snapshot compared"), C.Snapshots, S.Snapshots);
				Test->TestEqual(TEXT("zero decoder disagreements"), C.Disagree, 0);
				Test->TestEqual(TEXT("zero apply errors"), S.Rust.ApplyErrorCount(), 0);
				Test->TestEqual(TEXT("no refused or unfed snapshots"), C.Refused + C.NotFed, 0);
				Test->TestEqual(TEXT("fallback fog and map unchanged"), S.FogOrMapDifferences, 0);
				Test->TestFalse(TEXT("no held-set resync"), S.bDrift);
				Test->TestTrue(TEXT("Rust remains authoritative"), S.Rust.IsAuthoritative());
				Test->AddInfo(FString::Printf(TEXT("live seat %d: starts %d, snapshots %d, adopted %d, agree %d, rows %lld"),
					I, S.Starts, S.Snapshots, C.Adopted, C.Agree, (long long)C.RowsAdopted));
			}
			return true;
		}
	private:
		FAutomationTestBase* Test;
		FSeat Seats[2];
		double Started;
		bool bStartSent = false, bMoveSent = false, bMoved = false;
		int32 MoveId = -1;
		FVector2D MoveFrom;
	};
}
IMPLEMENT_SIMPLE_AUTOMATION_TEST(FTCShared31LiveDecoderTest, "ThreeCrossroads.Net.LiveDecoder", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ClientContext | EAutomationTestFlags::ProductFilter)
bool FTCShared31LiveDecoderTest::RunTest(const FString&)
{
	const FString Url = FPlatformMisc::GetEnvironmentVariable(TEXT("TC_SHARED31_LIVE_SERVER"));
	if (Url.IsEmpty()) { AddWarning(TEXT("LiveDecoder NOT RUN: supply TC_SHARED31_LIVE_SERVER for an isolated reference server.")); return true; }
	if (!TestTrue(TEXT("isolated loopback server"), TCSession30::Loopback(TCHAR_TO_UTF8(*Url)))) return false;
	if (!TestTrue(TEXT("required linked core"), FWW2Core::IsReady())) return false;
	ADD_LATENT_AUTOMATION_COMMAND(TCShared31Live::FRun(this, Url));
	return true;
}
#endif
