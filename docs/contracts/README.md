# Client/server contracts

Coordination: issue #5. Delivery: issue #4.

These files reserve contract topics; no protocol or ABI is approved by their creation. The pinned reference implementation remains the compatibility source while contracts are documented.

| Document | Draft owner | Consumer reviewer | Status |
| --- | --- | --- | --- |
| protocol.md | Julio | Axel | v0 draft; technical answers recorded (#5); operative hit-position, spectator reference behavior and cached-mask decisions confirmed by both owners on 2026-10-16, recorded in protocol.md |
| [r1-ingress.md](r1-ingress.md) | Julio | Axel | Prototype-identical ordinary R1 ingress candidate under the recorded groups F/M grant; exact-head runtime ack pending |
| terrain.md | Julio | Axel | v0 draft; technical answers recorded (#5) |
| [save-format.md](save-format.md) | Julio | Axel | Private JSON v1 with optional elimination state; owner group I recorded in #5 comment 6217590741; implementation exact-head acknowledgement pending |
| [startup-tables/README.md](startup-tables/README.md) | Julio | Axel | Content/schema v1 candidate explicitly accepted by both owners in PR #195 review; internal producer/platform proof integrated through PR #198 at `514e15f4`; transport #199 remains unagreed and unimplemented; Unreal adoption pending |
| [combat-recipient-filtering.md](combat-recipient-filtering.md) | Julio | Axel | Existing prototype unit and ghost shapes; cached-path privacy mask under confirmed owner decision P; no World or Classic room admission |
| rust-unreal-abi.md | Julio | Axel | Six ABI 2 recipient-view calls authoritative through PR #16, merged at `d88ef6d`; broader v0 contract remains draft; approved header unchanged; consumer source published in draft PR #201 at `9551d3976e9145c81030cf5a81ec7f0764b04d12`, explicitly not for merge, neither merged nor adopted on main; fresh live and complete decoder adoption pending |

The [R1 ingress note](r1-ingress.md) records prototype command shapes and the
bounded #296/#366 overlap under owner decisions F and M. Exact-head runtime
review remains required.

[PR #201](https://github.com/axelmanosalvasmd/ww2-rts-native/pull/201) records Axel-reported Linux build/automation on Rust `92fb7fe`, separate from the earlier live/package results on `03a49ac`. These owner reports are not independently validated here. Rust unit rows are the draft's default, but C++ fallback, row tails, cells, fog and independent row parsing remain. Windows/macOS client and input checks, physical mouse/keyboard tests and broader rules-query integration are still pending. Native proof and integration are recorded in [DESIGN.md](../../DESIGN.md); they do not establish Unreal adoption.

Contract PRs must state native/reference baseline, version/status, fields and semantics, ownership/privacy, error/recovery behavior, compatibility transition, fixtures and verification evidence. Both owners must explicitly approve breaking changes before adoption. Mark a contract agreed only after approvals and merge. Do not put credentials or private sessions in fixtures.

The ABI 2 approval is limited to the six recipient-view calls in `rust-unreal-abi.md` and their staged Unreal consumer update. It does not approve additional interfaces. The operative privacy decisions recorded in `protocol.md` have separate owner confirmation. The broader drafts remain drafts; merged jointly approved scope is authoritative.

[Editor maps and Horde records](editor-record-persistence.md) documents authenticated raw-map access, worker catalogue refresh and private Horde record storage under the recorded owner decisions. Hosted Horde admission and World persistence remain separate.
