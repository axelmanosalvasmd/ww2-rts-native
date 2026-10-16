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
