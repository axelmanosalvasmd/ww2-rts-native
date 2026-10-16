## Recipient-only AI planning and the ground command union

The human commander now decodes dynamic unit, wallet and plan facts from the
same recipient producers as the human client. The original public map supplies
start terrain and player metadata. Recipient terrain memory supplies later
changes, owned mines and detected enemy mines. No live own-motion or private
enemy-state fallback remains. This follows owner decision L in issue #5,
comments 6217590741 and 6218552386, at prototype pin e4e579a.

The adapter consumes the real partial recipient projection plus the real
found-mine list, then publishes its staged terrain receipt only after all
observations validate. It does not label that projection a complete network
snapshot. Unsupported logistics refuses without publishing memory. This
replaces the old AI-28 test that accepted authoritative stock without a
recipient field. That former witness no longer proves a delivered logistics
path on this branch; #375 owns the complete recipient prerequisite.

Detached tactics now use the existing canonical command kinds for ground
abilities, focus attack, release and resumed queued attack-move, house shelling,
bridge repair, filling, detected-mine clearance, point-holder fortification,
and headquarters wall/gate planning. The planner preflights the complete
repair stamp using detached visibility and approaches unseen ground before
reserving a dig. Per-look mirroring reserves earlier
spending and busy workers. Engineering memory keeps jobs until an observed dig
ends, the squad disappears or 90 simulation seconds pass. Ties use unit ID.
There is no new canonical Match state, wire field, FFI call or header change.

Prototype evidence is shared/ai.js:43-145 (engineering), 650-680 (abilities and
house shelling), 718-747 (point holders), 805-845 (focus), and 909-958
(headquarters fortification). World-only rail/board/unload at 630-710 and recover at 434 remain
outside closed World admission. Ground FireAt actuation and terrain projectile
delivery belong to #337; unrestricted aircraft commands need #369. No flight shape is invented here. Ordinary human packets keep the internal
`ground_flights` capability false: aimed grenade, satchel and barrage attempts
and house FireAt remain held until their complete flight delivery is available.
Detached component tests enable that capability explicitly to check planning.

AI-13 remains incomplete until those flight prerequisites and its deferred
World command families have complete evidence. AI-64's Conquest and house-free
proof exercises real recipient snapshots and the real human policy at all
three levels, with visible negative controls. Classic admission is owned by
#390; real Assault/Horde recipients and the scripted Horde seat remain required
for the full AI-64 matrix. Neither full row is promoted by this change.
