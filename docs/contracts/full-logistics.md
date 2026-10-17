# Logistics recipient fields

Native baseline: `58986df2`. Prototype: `e4e579a`.

This implements the existing prototype `snapshotFor(...).logistics` object for
ordinary Default, Conquest, Classic and Annihilation recipient delivery. The
Classic and Annihilation fields are documented in
[classic-annihilation-snapshots.md](classic-annihilation-snapshots.md). It introduces no new field or
meaning. Both owners opened full logistics with prototype visibility on issue
#5, comment 6217590741. Runtime acceptance still requires an exact-head review.
World and spectator admission remain closed in this change. LOG-44 therefore
retains its World region-rate and spectator-delivery requirements outside this
ordinary-seat implementation.

The compatibility source is `shared/supply-convoys.js:646-659`, attached by
`shared/sim.js:6016`. The server passes it through to seats at
`server.js:246` and `server/room-tick.js:34`; list trimming at
`server.js:225-244` does not remove or cache logistics. The prototype removes the
whole object from spectators at `server.js:247-251`.

When convoys are absent, the `logistics` key is omitted, matching JavaScript's
undefined property during JSON serialization. Otherwise the object contains
`enabled: true`, `units`, `trucks`, `stores` and `events`, using arrays for empty
collections and omitting `regions` outside World. Field names, nulls, omission and
rounding below are identical to the prototype.

| Field | Recipient rule and encoding |
| --- | --- |
| `units` | Only units owned by the recipient that have reserves, in canonical unit order. Visibility, reveal and alliance do not widen ownership. No health filter is applied. |
| Unit `id` | Unit identity. |
| Unit `ammo` | Carried ammo divided by maximum, rounded to two decimals; null when maximum is zero. |
| Unit `provisions`, `fuel`, `emergency`, `warning` | Two decimals; nullable reserves remain null. |
| Unit `forced`, `stranded` | Booleans. No exhausted flag, raw maximum, ammo price or credit is sent. |
| Unit `supply`, `cut`, `grace` | Cut at ten seconds without supply. Supply is zero when cut, otherwise the last rate or zero, rounded to two decimals. Grace is the ceiling of seconds remaining only while cut time is strictly between zero and ten, otherwise null. |
| `trucks` | Only the recipient's units with convoy state, in canonical unit order. |
| Truck `id`, `state`, `manual`, `hold` | Unit identity, existing state string and two booleans. State strings are `idle`, `collecting`, `loading`, `delivering`, `unloading`, `returning`, `waiting`, `waitingCollect`, `waitingReturn`, `manual`, `held`. |
| Truck `cargo` | Object with `ammo`, `provisions`, `fuel`, each rounded to two decimals. |
| Truck `destination` | Unrounded `{x,z}` or null. |
| Truck `route` | Current unit path, as `{x,z}` objects rounded to two decimals. The convoy's private stored route is not used. |
| `stores` | Active stores belonging to the recipient's team, in store insertion order, without a fog predicate. Enemy and inactive stores are excluded. |
| Store `id`, `owner`, `x`, `z`, `source` | Existing string identity, owning slot, position rounded to two decimals and source boolean. |
| Store `stock` | Unrounded `ammo`, `provisions`, `fuel` available to this recipient. Own stock excludes grants to other recipients; authorized incoming relief is bounded by its payer's remaining stock. Other payer inventories are not exposed. |
| Store `capacity` | Unrounded `ammo`, `provisions`, `fuel` capacity. |
| `events` | Only the recipient's events, in event order. Each contains `id`, `type`, `count`, `message` and optional unrounded `x`, `z`. The private `owner` and timestamp `at` are omitted. Absent coordinates are omitted, not null. |

The stock authorization calculation is the existing core store owner, matching
`shared/supply-convoys.js:38-67`. Projection uses a local copy for the lazy empty
bucket creation, so collecting a frame changes no canonical inventory.

The complete snapshot path refuses nonfinite emitted numbers instead of
serializing invalid canonical state as JSON null. It retains the existing mode
guards and other actor guards. No C ABI, header, message kind or wire version
changes.

The exact shape tests are
`recipient_snapshot::logistics::tests::intent_log44_exact_fields_rounding_and_private_allied_stock_in_complete_frames`
and
`recipient_snapshot::logistics::tests::intent_log44_nulls_grace_cut_boundaries_and_disabled_omission_match_prototype`.
They collect complete seat frames, assert the full logistics object and exercise
allied donations, inactive and enemy stores, reveal, nullable reserves, grace
boundaries and omission. `logistics_cannot_be_requested_with_a_nonseat_recipient`
checks that invalid recipients are rejected before delivery. This does not
implement or claim the spectator projection. Full-match supply lifecycle coverage belongs to the
runtime tests in this PR. These ordinary-seat tests do not claim the full LOG-44
row.
