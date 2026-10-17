# Ordinary owner lists in the existing snapshot

Status: prototype wire compatibility under owner decisions N and P. This adds no C ABI or public command type. The protocol draft keeps its existing approval status.

The recipient is the seated player slot, including when a teammate can see the same unit. `shared/sim.js:5911,6016-6062` defines these fields. The cache creates an empty `air` array for every seat, including a match without aircraft.

| Field | Prototype shape and scope |
| --- | --- |
| `movement` | Own `[id, outcome, tick, ...result]` rows. |
| `queues` | `[buildingId, trainingProgress, rallyX, rallyZ, ...unitTypes]`. Own production buildings, plus the prototype's shared defender production access in Horde. |
| `productionJobs` | Own `[buildingId, [{id,type,mp,fuel,status},...]]` rows, including original paid refund amounts. Shared Horde queue access does not share paid job identities. |
| `plans`, `orders` | Own unit plans and waiting orders with the existing numeric kinds and coordinates. |
| `rally` | Own rounded `[x,z]`, or `null`. |
| `air` | Own `[id,state,ceil(fuelSeconds),ammo,ceil(rearmSeconds)]`, where state is base 0, out 1, station 2, home 3 or rearm 4. |
| `sup` | Own support cooldowns, each clamped to a nonnegative integer ceiling. |
| `mp`, `inc` | Own MP floor and income rounded to one decimal. |
| `mun`, `fuel`, `fuelInc`, `upkeep` | Own construction economy fields when the prototype initializes them. Wallets use floor; rates use one decimal. |
| `tech` | Own tier, armory levels and research progress when technology is enabled. |
| `logistics` | Omitted without convoys. Otherwise the object below. |

`shared/supply-convoys.js:646-661` defines the logistics object. All quantities described as rounded here use two decimal places.

- `enabled: true`.
- `units`: own reserve objects with `id`, fractional `ammo` (or `null` when ammo capacity is zero), `provisions`, nullable `fuel`, `emergency` and `warning`, boolean `forced` and `stranded`, `supply`, boolean `cut` and nullable integer `grace`. The ten-second cut grace and cached supply rate retain the prototype behavior.
- `trucks`: own `{id,state,manual,hold,cargo,destination,route}` objects. Cargo has rounded `ammo`, `provisions` and `fuel`. Destination is the original point or `null`; route points have rounded `x` and `z`.
- `stores`: active allied `{id,owner,x,z,source,stock,capacity}` objects. Position uses two decimals. Stock includes the receiving seat's available bucket and donations granted to it, preserving exact stock quantities. Other seats' private buckets are excluded. Reads clone the store before the existing lazy bucket calculation, leaving canonical inventory unchanged.
- `events`: own `{id,type,count,message,x?,z?}` objects. The prototype removes both `owner` and the internal `at` timestamp. Optional coordinates remain unrounded.
- `regions` is absent in ordinary modes. The World clause remains deferred with World admission closed.

Ordinary ticks advance reserves and forced withdrawal before each unit's orders, refresh the ground network at its configured beat before reinforcement, and advance convoys after capture and income. Retirement removes reserve and convoy ownership. Later terminal ticks retain serializable owner lists. Inconsistent logistics option/state pairs are refused before tick or delivery mutation.

The witnesses are `match_logistics::net54_tests`. They exercise every ordinary mode with every seated recipient, populated Classic jobs and refunds, and supported physical logistics through ticks, retirement and terminal snapshots. These are core recipient witnesses, not hosted network or Unreal adoption evidence.
