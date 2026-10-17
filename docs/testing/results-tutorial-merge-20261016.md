# RESULTS-TUTORIAL merge record



Tutorial merge parents: hosted results `8c2a7fd6a7b5bfa4c2792ab3fc345e97ca3cc3de` and Tutorial rooms `d093945f668fc62ad022c32ad197b1407a7d4828`. Git automatic conflict tree: `9fa873d4aa283e2d30ff5642f5e4f9ff09dac295`.



Every conflict hunk and every additional manual integration edit is listed below. Hunk numbers follow the original merge buffer within each file. Ours means hosted results; theirs means Tutorial rooms.



### rust/crates/core/src/match_firing.rs

1. Original merge-buffer line 969: Keep hosted flight_contact_for and visible_flight_contact. The existing flights::project adapter already uses the same current-visible-body path required by Tutorial.

2. Original merge-buffer line 984: Keep hosted recipient slot and flight_contact_for signature. The duplicate incoming closure-based API is removed with its duplicate flight adapter.

3. Original merge-buffer line 1001: Keep hosted start/end fractions and optional recipient. Live resolution retains previous-pose behavior; snapshot contact retains current poses.

4. Original merge-buffer line 1077: Keep hosted recipient.is_some() condition for current snapshot poses.

### rust/crates/core/src/match_human_observation.rs

1. Original merge-buffer line 83: Keep Horde and Tutorial as HumanShot::Other.

### rust/crates/core/src/match_tick.rs

1. Original merge-buffer line 106: Keep both Horde and Tutorial TickEvent variants; incoming Tutorial lines remain appended after the Tutorial phase.

### rust/crates/core/src/recipient_lists.rs

1. Original merge-buffer line 307: Keep hosted Horde actor/economy validation, all ordinary modes, and Classic node handling. Remove the Tutorial exclusion to admit incoming Tutorial raw lists.

### rust/crates/core/src/recipient_snapshot.rs

1. Original merge-buffer line 123: Keep hosted mode union including Horde, Annihilation and Classic plus geography, unexpected Assault and World-runtime guards. Add Tutorial to the ordinary union, followed by incoming validate_tutorial_state mapped to Rules. Keep all hosted per-mode finish/economy validation.

2. Original merge-buffer line 267: Keep hosted flights::project as the single canonical live-flight adapter, including empty flights output and its validation. Incoming Tutorial uses that adapter.

### rust/crates/core/src/recipient_snapshot/composition.rs

1. Original merge-buffer line 108: Keep hosted flights entry preservation and empty shots initialization. Tick-event delivery applies the actual shot batch through with_shots afterward.

### rust/crates/core/src/recipient_snapshot/public_state.rs

1. Original merge-buffer line 120: Keep hosted nodes insertion. Independently append incoming Tutorial mode row and Assault mode row; existing Horde mode row remains above them.

### rust/crates/core/src/recipient_snapshot/shots.rs

1. Original merge-buffer line 24: Keep both Horde and Tutorial ShotRef variants.

2. Original merge-buffer line 40: Keep hosted Horde and Aviation conversion and add Tutorial conversion. Do not restore the stale incoming Aviation refusal.

3. Original merge-buffer line 222: Keep hosted validated Horde arm and add a separate incoming validated Tutorial arm, each returning its own prototype shot_json.

4. Original merge-buffer line 380: Keep hosted SmallArms refusal and Horde-only Breath/BreathShell validation. Do not restore the stale incoming blanket Horde breath refusal.

5. Original merge-buffer line 689: Preserve complete hosted aviation_events_keep_exact_shapes_order_and_nonlocal_owner_privacy test, and separately retain complete incoming ordinary_throw_uses_prototype_shape_and_local_visibility test.

6. Original merge-buffer line 737: Preserve remaining hosted aviation test assertions and both incoming throw/live-flight test bodies as separate tests. The incoming live-flight test calls the hosted flights::project adapter.

7. Original merge-buffer line 1056: Keep hosted invalid zero-shooter Throw rejection witness. Incoming deletion was unnecessary because valid throws are already supported.

### rust/crates/simtest/tests/common/companion_tick.rs

1. Original merge-buffer line 556: Keep Horde serialization and add Tutorial serialization to the exhaustive event adapter.

### rust/crates/simtest/tests/match_tick_events.rs

1. Original merge-buffer line 71: Keep Horde serialization and add Tutorial serialization.

2. Original merge-buffer line 268: Keep all four original fixture families and reject Scenario, Horde and Tutorial as unexpected in that fixture, naming all three in the panic.

### rust/crates/simtest/tests/recipient_effects.rs

1. Original merge-buffer line 571: Keep hosted explanation of base-field input history and fresh complete-frame receipt.

2. Original merge-buffer line 594: Keep hosted equivalent usize::from accumulation for live-flight frames.

3. Original merge-buffer line 634: Keep hosted unconditional exact flights comparison for every recipient, preserving stronger coverage than incoming conditional comparison.

4. Original merge-buffer line 716: Keep hosted invalid zero-shooter Throw error witness and SnapshotError::Rules.

1. `match_firing.rs`: remove the automatically combined `&& visible(t.id)` predicate. The hosted API has no closure parameter and already filters with `recipient.is_none_or(...seen_by...)`; retaining both would reference an undefined variable.

2. `recipient_snapshot/shots.rs`: remove incoming duplicate `flights_for` implementation. It duplicates hosted `recipient_snapshot/flights.rs`, uses the replaced incoming contact API, and would otherwise be dead code. Retain both incoming tests and redirect every `flights_for` call in the incoming live-flight test to `super::super::flights::project`. No assertion or expected wire object changed.

3. `recipient_lists.rs::recipient_snapshot_lists`: remove the separate nonconflicting Tutorial exclusion from the existing ordinary-mode branch. The raw-list conflict resolution now admits Tutorial; complete recipient list delivery must accept the same mode. Existing ordinary_ground_mode, recipient, mine-shape, observation and display checks stay in place.

4. `point_supply_intent_tests.rs`: change the injected Horde identity snapshot expectation from UnsupportedMode to Rules. Hosted SnapshotCache now recognizes Horde identity and validates canonical Horde state, which this fixture intentionally lacks. Keep the missing-Horde-state point-supply error and all no-mutation assertions. Incoming Tutorial expectation remains Rules; World remains UnsupportedMode.



## Server conflicts



`lib.rs` hunk 1 and `room_runtime.rs` hunk 1: use the Tutorial descriptions of public startup and room runtime ownership.

`trusted_room_process.rs` hunk 1: keep the hosted spectator-aware test name and its assertions; automatic incoming public Start assertions remain.

`native_startup.rs` hunk 1: union Horde and Tutorial mode bindings. Hunk 2: keep hosted Annihilation/Classic bindings and `mode_matches`. Hunk 3: keep both Horde and Tutorial StartPlan constructors. Hunk 4: keep hosted Assault defender validation and elimination constructors.

`lobby.rs` hunk 1: result seat count includes one unattended seat for Horde or Tutorial. Hunk 2: keep Horde result/record persistence. Hunk 3: keep host End admission and ready-socket guard. Hunk 4: validate Tutorial runtime/Enemy index and combine the extra-seat count with Horde. Hunk 5: force Horde/Tutorial human teams to zero and retain Assault defender/attacker remapping. Hunk 6: keep Person::online and generic internal-seat resize. Hunk 7: keep public Start/setup from Tutorial while preserving hosted Horde exclusion from the trusted ordinary path. Hunk 8: keep both front record owner and creation-only Tutorial autostart flag. Hunk 9: add Tutorial to the serializer union while keeping mode-specific timeline refusal. Hunk 10: retain async Start, Horde, ordinary result and Tutorial test module registrations.



## Additional server integration edits



`lobby.rs`: remove automatically combined Tutorial `away.push(false)` because the generic roster resize already includes Enemy; retain the Tutorial-only human/no-spectator attachment guard alongside newer ordinary AI and Horde admission. Remove the old shadowed extra-seat count. These edits precede the first merge commit.



The inherited Tutorial test rewrites replace public Start denial with successful Start or nonhost ignore, and keep role, private-room, restart and unsupported gameplay guards. No test registration was removed. The core injected Horde missing-state check now expects Rules rather than UnsupportedMode, preserving the refusal and no-mutation assertions.

## Main integration

Parents: Tutorial merge `2297cc6f` and main `4a88afd8b1fd33f717532f8f31f1bc87bb8ca2b3`. Automatic conflict tree `997f7e7f8a017649e7ab73f5ee7986369cf39ff9`.

- `recipient_snapshot.rs`, hunk 1: keep both flights and modes modules. Hunk 2: use main's positive wave check to populate profile/boss after the first wave, preserving initial omission.
- `recipient_snapshot/public_state.rs`, hunk 1: use main's shared mode_row producer for all modes, retaining Horde via its delegation. Additional manual edit: remove the redundant Tutorial/Assault projection below; the shared producer already contains both exact branches.
- `lobby.rs`, hunk 1: keep the public Start roster error. Main's old synchronous trusted-start body was matched into this new public constructor; the complete newer trusted admission and restart checks remain in async_start.rs, called by start_trusted. No host or restart guard is transferred to public preparation.

All other main changes merge automatically. Final validation covers shared mode rows, hosted records/AI, Tutorial and the ordinary trusted socket path.

Post-merge runtime repair: remove hosted results' separate ordinary_mode_row helper, whose fallback still refused Tutorial, and route economy_fields through main's shared mode_row. That shared producer validates all represented mode identities and emits the same ordinary/Horde objects plus Tutorial. This also prevents a second mode object from overwriting the canonical public projection.

Post-merge test adapter repair: Tutorial Peer::drain explicitly rejects hosted Output::Watcher, preserving exhaustive handling without accepting spectator output in the seated fixture.

Post-merge runtime correction: lobby result.you is now null when the current result has no story, matching prototype server.js:348. This prevents prior last_match outcomes from leaking after host End. The two-human Tutorial test retains prior victory/defeat outcomes before starting and ending the next room. Tutorial Peer::drain also completes output byte accounting before dispatch, matching the actual socket writer.

Tutorial hold witness clock correction: CatchUpClock starts due at zero. Poll callback 1 at time zero, then advance by 50 ms, so each assertion measures one callback rather than consuming the initial two due ticks in the first poll. No runtime clock behavior changed.

## Qualification repairs

The first 374-test composed run passed 371 tests and exposed three integration gaps:

- `async_start.rs`: restore main's existing start_ready guard before the newer private R1 policy checks. The earlier main conflict had placed this synchronous guard inside public Start's conflict; it belongs in the async trusted owner too. `lobby.rs` public Start now calls the same readiness helper, retaining host/native checks and every capacity/Assault predicate. The inherited mode04 test remains unchanged.
- `recipient_lists.rs`: validate canonical Tutorial state before raw-list and per-recipient list publication. Replacing a malformed Horde state's identity with Tutorial must still refuse. The inherited canonical_horde_global_lists_survive_ticks_and_every_seat_delivery guard assertions remain unchanged.
- `mode_startup_tests.rs::mode01_non_world_mode_rows_match_the_prototype_public_fields`: preserve every literal component field expectation, and replace the obsolete blanket non-Conquest refusal with exact mode projection for every seat in every admitted non-World mode. Malformed mode-pair and owner checks remain in their separate tests.

The later main `6f02f83a` merge into `b9987e7b` was automatic with no file changes. Merge `3c5b08de` exactly matches automatic tree `f5ab02fb30e8b9844bdcdf84d89d436b97de11ff`; no additional merge hunk exists.

## Main integration at 3c33cd04


Parents: `aff9cb62b9d39dbbeb3c5a24a2c8aa672d2a4511` and main `3c33cd040b8fcaa71d2795b3b3030c749b522706`. Automatic conflict tree: `3e2fd05b2b13924446c054b8ef69d9b2f5314829`. All fourteen conflicting paths are accounted for below.


### rust/crates/core/src/match_human_observation.rs

1. Original merge-buffer line 112: Union Horde, Tutorial and AuthoredMission in HumanShot::Other. Keep every other incoming observation-memory/checkpoint and scheduling change.

### rust/crates/core/src/match_tick.rs

1. Original merge-buffer line 106: Union Horde, Tutorial and AuthoredMission TickEvent variants. Keep incoming authored mission phase and existing Horde/Tutorial phase emissions.

### rust/crates/core/src/recipient_snapshot.rs

1. Original merge-buffer line 1: Keep our admitted ordinary/projectile/aviation module description, which matches the composed runtime scope.

2. Original merge-buffer line 142: Keep incoming call to extracted validate_snapshot plus its test cache-build counter. Move the complete ours validation block into that shared validator rather than retain the narrower incoming mode gate.

3. Original merge-buffer line 285: Keep incoming shared project_recipient path, uncached recipient API, row filtering helper and extracted validator. Shared validation retains all ours mode, finish, elimination, Horde, firing/grenade/salvo, engineering, aircraft and projectile guards. Shared projection also emits ours live flights. Preserve pub(crate) seen_by visibility for existing consumers.

### rust/crates/core/src/recipient_snapshot/public_state.rs

1. Original merge-buffer line 83: Keep ours Classic-or-World ghost branch. Incoming authored_mission scenario projection remains separate above the legacy scenario fallback. World room admission stays refused by the common validator.

### rust/crates/core/src/recipient_snapshot/shots.rs

1. Original merge-buffer line 24: Union Horde, Tutorial and AuthoredMission ShotRef variants.

2. Original merge-buffer line 41: Union Horde, Tutorial and AuthoredMission conversions. Keep ours Aviation conversion and existing aviation event projection rather than restore stale incoming Aviation refusal.

3. Original merge-buffer line 224: Keep both ours Horde/Tutorial event validation arms and the incoming AuthoredMission arm, including definition acceptance and atTick checks.

4. Original merge-buffer line 566: Preserve original ShotRef identity in the preprojected event list. Tutorial is public; Scenario and AuthoredMission use incoming mission policy filtering. This avoids assuming Tutorial lines carry scenarioRecipients/scenarioSide.

## Nonconflicting manual edits

1. `recipient_snapshot.rs::validate_snapshot`: replace the incoming extracted validator body with the complete ours SnapshotCache::new admission block, dedented without altering its conditions. This is necessary because the extraction otherwise accepts only Default/Conquest and Classic, dropping Tutorial, Horde, Assault and Annihilation. The extracted helper is called by cached and uncached paths, so each now receives the same validation.

2. `recipient_snapshot.rs::project_recipient`: add the existing `flights::project(state, slot)` insertion next to falling. Both cached and new uncached projections retain the live-flight field and its exact adapter.

3. `recipient_snapshot.rs::seen_by`: retain pub(crate) visibility. The incoming extracted block otherwise makes the existing helper private.

4. `recipient_snapshot/shots.rs::project_shots_perspective`: keep ShotRef with each projected JSON row instead of reducing its identity to a boolean. Before mission policy handling, a Tutorial arm publishes the validated public line directly. The incoming Scenario/AuthoredMission branch keeps its exact public/side/team and spectator checks. Tutorial lines have no scenarioRecipients/scenarioSide, so applying the incoming mission branch to them would refuse the existing Tutorial journey.

## Resolution decisions

- commander_assault.rs conflicts 1 and 2: retain SavedReason and saved_reason. Both parents serialize the same optional reason field. The existing local codec restores all nine valid reasons and rejects unknown names on serialization and deserialization. This preserves the local checkpoint guard while satisfying incoming AI-memory serialization.
- commander_tactics.rs conflicts 1 and 2: retain SavedCounter and saved_counter. The optional counter still serializes as its existing value, and restore accepts only at and mg. Both parents retain Serialize and Deserialize.
- commander_tactics.rs conflict 3: keep the local shared rush_at helper, filtered construction_units and plan_with_classic dispatch with its strategic sightings, node memory, level and random arguments. Pass the incoming construction_view and mirrored filtered units into both World and ordinary/Classic construction. This retains queued-plan mirroring and the endangered-engineer guard without a stale construction_crew reference.
- horde_commander.rs conflict 1: retain is_digging and incoming same_intent in one OwnPlan implementation; retain incoming OwnUnit serialization derives. Numeric JSON comparison, exact path_generation checks and destination/kind comparison for ongoing Dig jobs remain intact. Local delivered digging still excludes busy units.
- human_commander_policy_tests.rs conflict 1: keep ai63_planner_checkpoint_retains_full_memory_without_random_streams and intent_ai12_refused_garrison_still_keeps_the_squad_busy_for_that_look, adding the separate test attribute between them. Both function bodies are unchanged.

## Nonconflict manual edits

- horde_commander.rs: add Serialize and Deserialize derives to DeliveredPlan and DeliveredIntent. The local OwnPlan.delivered field is retained, so these derives are required by incoming OwnPlan/View/HordeCommander checkpoint serialization and same_intent JSON comparison. No fields are removed or defaulted.



`rust/crates/server/src/native_startup.rs`, hunk 1: combine with canonical startup checks
`rust/crates/simtest/tests/common/companion_tick.rs`, hunk 1: combine both event variants
`rust/crates/simtest/tests/legacy_scenario_tick.rs`, hunk 1: combine both event variants
`rust/crates/simtest/tests/match_tick_events.rs`, hunk 1: combine both event variants
`rust/crates/simtest/tests/match_tick_events.rs`, hunk 2: combine with canonical startup checks
`docs/contracts/save-format.md`, hunk 1: keep hosted
`docs/contracts/save-format.md`, hunk 2: combine with canonical startup checks
`native_startup.rs`: preserve incoming authored-mission definition binding before all hosted/Tutorial/Assault/elimination StartPlan branches.
`save-format.md`: retain the hosted AI envelope and continuation sections, add the separate incoming external AI codec, and remove incoming historical room-AI refusal statements superseded by hosted results. Keep non-authored v1 and authored v2 scope explicit.


All parent test bodies and module registrations are retained. Formatting is applied after the merge. The canceled S1 run had not acquired a slot or executed Cargo. The earlier corrected S3 attempt stopped at a test compile error: RecipientProjection exposes fields, not into_json; its mode test now uses that API. The complete Tutorial journey continues to prove delivered frames independently of this component projection check.

## AI checkpoint compatibility after main integration

Main's external v1 HumanPolicy checkpoint has no tactics.node field. The hosted parent adds that required map. `commander_tactics.rs` now defaults only an absent node map to empty, retaining present assignments. The existing `if27_field_and_world_policy_memory_round_trip_and_legacy_defaults` test now proves a nonempty node round trip, removes exactly node from the actual external tagged checkpoint and restores it through restore_ai_memories_json, comparing the complete policy. Its existing field/World omission assertions remain. The incoming Horde memory test's OwnPlan literal also retains the hosted delivered field with None; no queue assertion changes.

Independent source reviews accepted both the core privacy/event resolutions and the AI/startup/event resolutions after these repairs. The inherited spectator adapter still emits an empty flights array (spectator.rs:110); this file is unchanged by the stream and its live-flight preview limitation remains outside the two result rows.
