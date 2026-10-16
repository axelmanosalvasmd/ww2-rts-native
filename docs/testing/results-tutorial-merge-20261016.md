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
