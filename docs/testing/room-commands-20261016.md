# Room command stream

## Merge record

Base: PR #296 at `5725bdc2308a89ef66cf0d9f269b80780073cac4`.
Merged PR #366 at `be83659f9783d95f31282228cecf31733b01d2f0`.
The base already contains the earlier #366 integration. All seven conflict
regions retain the base side:

- `docs/contracts/r1-ingress.md`, held-dependency list: retain buy/support admission.
- Same file, PR 296 integration section: retain its limits and denial documentation.
- `commander_ingress.rs`, three regions: retain buy/support tags, parsing and command admission.
- `native_net2_socket_tests.rs`, AppState: retain `static_files: None`.
- `seat_lifecycle_tests.rs`, host pause/resume regression: retain the complete test.

All other merge changes are automatic. No guards or test registrations were removed.

Merged PR #369 at `161bbe3880f0ac06187c64bfe3ff2dab0856841c`, including its
#349 flight ancestry. One conflict region in `commander_ingress.rs` joins
the complete garrisoned movement and production-cancel tests from #296/#366
with #369's complete FireAt/compound refusal test. All three are retained.
Other merge changes are automatic, including `room_runtime.rs`.

Merged PR #375 at `d2954b59b539e82d386ed1f6c1245664c69a6d19` for convoy
tick and recipient delivery. Four conflict regions:

- `save-format.md`: retain the base optional elimination-field documentation.
- `recipient_snapshot.rs`, module registration: retain both flights and logistics.
- Same file, projection: retain both flight insertion and optional logistics insertion.
- `server/src/main.rs`, help text: retain base MAX_ROOMS/static-root help and
  update logistics to on, matching the incoming runtime configuration.

All other changes are automatic.
