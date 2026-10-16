# AI-PLANNING-3 row evidence

Prototype: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
Base: #407, stacking on #391. Dependencies merged: #414, #413 and #369
(the latter includes #349 and #337). Exact parents and conflict resolutions
are in `merge-record.md` and the two committed patches.

Owner decision L is addressed by detached recipient packets. AI policies receive
no Match/Game handle. Roles use the delivered mode object and seat team. Public
starting terrain is retained separately from later delivered changes; the
existing owner work rows provide wall-work knowledge. World and Tutorial remain
outside this delivery adapter. No wire or FFI fields are added. Ordinary firing also accepts a pending aimed ability while it approaches, as the prototype targeting/firing block does at `shared/sim.js:5557-5593`. Air, rail and operative firing guards remain unchanged.

## Literal row mapping

- AI-64: `ai_privacy_modes_tests::ai64_all_modes_all_seats_and_levels_ignore_hidden_state_in_complete_recipient_frames`; `ai64_visible_own_health_wallet_and_horde_stance_change_actual_commands`; `ai64_real_mode_policy_orders_and_all_recipient_adapters_survive_ticks_and_ending`.
- AI-13 (ordinary R1): `commander_union_tests::ai13_union_contains_every_ordinary_command_family_and_engineering_variant`; `ai13_union_ability_grenade_satchel_barrage_execute_live_flights_and_finish`; `ai13_union_fireat_executes_live_flights_every_seat_delivery_and_finish`; `ai13_pending_aimed_approach_survives_weapon_turns_delivery_and_finish`.

AI-64 constructs real Conquest with houses, house-free Conquest, Classic,
Assault and Horde. Both seats, two seeds, two delivery ticks and all three
levels compare complete recipient snapshots, detached observations and actual
policy outputs. Nine perturbation families cover enemy wallets/support, hidden
armies, added tanks, secret depots, visible enemies' private plans/stance,
sub-precision positions/health, hidden footprints, future weather and hidden
mines on open/mud/road. The scripted Horde seat uses its actual Horde policy
adapter and an active, staffed wave. There are 1,080 paired level comparisons.
Visible negative controls change commands. Lifecycle tests queue policy output,
collect every seat on each ordinary tick and continue through 60 ending ticks.
The scripted look matches the prototype's scope at `test.js:4587-4733`; the
human camera/motor layer is outside this row.

AI-13 asserts the exact set of 16 ordinary command families: buy, support,
build, assist, tech, dig, entrench, move, amove, attack, retreat, ability,
stance, garrison, cover and fireat. Separate sets require bridge/fill/demine/
mines/gate and arc/strongpoint/wire/traps/wall. Each member comes from the
integrated planner, with bounded intent inputs and notice memory. Aimed grenade,
satchel, barrage and FireAt additionally use real human observations, the
canonical queue, retained live flights, natural contact/expiry, every-seat
snapshots, VP victory and 60 ending ticks.

Prototype command sites: repairs `shared/ai.js:76,117,136`, buy `438`, support
`522`, abilities `659-668`, FireAt `678`, holders `739-747`, focus `821-843`,
stance `847`, movement `872-874`, walls/gates `946-953`, tech `977`, assist
`1025`. World rail/board/unload (`630-710`) and recover (`434`) remain deferred
under the owner R1-clause decision in issue #5 comment 6224738109. Aircraft
mission planning remains AI-43 and is not promoted by this command-kind union.

## Validation

The PR body records the final producing head, focused selection, actual results,
format and strict workspace Clippy. Earlier failed runs remain diagnostics and
do not establish row completion. Shared Cargo
slots use matching targets, Rust 1.99.0, 6 GiB memory, zero swap and `-j4`.
No Source recorder or full-workspace nextest run is used. Hermes' exact-head
runtime acknowledgment, hosted CI and integration belong to the orchestrator.
