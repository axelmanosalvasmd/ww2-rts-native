// Presentation controls for matched visibility diagnostics. Normal play leaves Unit at -1.
#pragma once
#include <cstdint>

namespace TCBodyControl
{
	struct FControl
	{
		int Unit = -1, Slot = -1;
		bool Keep(uint64_t Key) const
		{
			if (Unit < 0) return true;
			if ((Key >> 60) != 1) return false; // corpse and other non-unit instances leave the control
			const int Kind = Key & 15;
			if (Kind == 2 || Kind == 3) return true; // retain the actual owner and selected rings
			if (int(uint32_t(Key >> 24)) != Unit) return false;
			return Slot < 0 || Kind != 1 || int((Key >> 4) & 0xffff) == Slot;
		}
		// Filter only completed presentation batches. Animation, casualties, random draws and recipient state
		// have already advanced normally. Parallel arrays retain their order and their source custom data.
		template <typename TBatch> void Filter(TBatch& Batch) const
		{
			if (Unit < 0) return;
			TBatch Out;
			Out.Floats = Batch.Floats;
			const bool HasData = Batch.Floats > 0 && Batch.Data.Num() == Batch.Transforms.Num() * Batch.Floats;
			for (int I = 0; I < Batch.Keys.Num() && I < Batch.Transforms.Num(); I++)
			{
				if (!Keep(Batch.Keys[I])) continue;
				Out.Transforms.Add(Batch.Transforms[I]); Out.Keys.Add(Batch.Keys[I]);
				if (HasData) Out.Data.Append(Batch.Data.GetData() + I * Batch.Floats, Batch.Floats);
			}
			Batch = static_cast<TBatch&&>(Out);
		}
	};
	// -TCBodyOnlyUnit=<id> [-TCBodyOnlySlot=<slot>]. Combine with the separate sprite-only control.
	const FControl& Get();
}
