# Client/server contracts

Coordination: issue #5. Delivery: issue #4.

These files reserve contract topics; no protocol or ABI is approved by their creation. The pinned reference implementation remains the compatibility source while contracts are documented.

| Document | Draft owner | Consumer reviewer | Status |
| --- | --- | --- | --- |
| protocol.md | Julio | Axel | v0 draft; technical answers recorded (#5); spectator reference behavior and cached flag mask clarified 2026-10-16 |
| terrain.md | Julio | Axel | v0 draft; technical answers recorded (#5) |
| [save-format.md](save-format.md) | Julio | Axel | Private JSON v1 for ordinary matches and shared authored v2; owner group I recorded in #5 comment 6217590741; implementation exact-head acknowledgement pending |
| [construction-production.md](construction-production.md) | Julio | Axel | Prototype-identical Classic recipient fields under the recorded wire/privacy decisions; exact-head runtime review and hosted Horde/logistics integration remain pending |
| [startup-tables/README.md](startup-tables/README.md) | Julio | Axel | Content/schema v1 candidate explicitly accepted by both owners in PR #195 review; internal producer/platform proof integrated through PR #198 at `514e15f4`; transport #199 remains unagreed and unimplemented; Unreal adoption pending |
| rust-unreal-abi.md | Julio | Axel | Six ABI 2 recipient-view calls authoritative through PR #16, merged at `d88ef6d`; broader v0 contract remains draft; existing approved declarations unchanged; additive ABI 2.1 queries are a candidate in [abi-queries.md](abi-queries.md); consumer source published in draft PR #201 at `9551d3976e9145c81030cf5a81ec7f0764b04d12`, explicitly not for merge, neither merged nor adopted on main; fresh live and complete decoder adoption pending |

[PR #201](https://github.com/axelmanosalvasmd/ww2-rts-native/pull/201) records Axel-reported Linux build/automation on Rust `92fb7fe`, separate from the earlier live/package results on `03a49ac`. These owner reports are not independently validated here. Rust unit rows are the draft's default, but C++ fallback, row tails, cells, fog and independent row parsing remain. Windows/macOS client and input checks, physical mouse/keyboard tests and broader rules-query integration are still pending. Native proof and integration are recorded in [DESIGN.md](../../DESIGN.md); they do not establish Unreal adoption.

Contract PRs must state native/reference baseline, version/status, fields and semantics, ownership/privacy, error/recovery behavior, compatibility transition, fixtures and verification evidence. Both owners must explicitly approve breaking changes before adoption. Mark a contract agreed only after approvals and merge. Do not put credentials or private sessions in fixtures.

The ABI 2 approval is limited to the six recipient-view calls in `rust-unreal-abi.md` and their staged Unreal consumer update. It does not approve additional interfaces or the pending protocol/privacy proposals. The broader drafts remain drafts; merged jointly approved scope is authoritative.

[Additive ABI 2.1 queries](abi-queries.md) are an implementation candidate authorized by owner decision `julio-373-merged-decisions-20261016` on #5. Existing ABI 2 functions and layouts remain unchanged; exact signatures require runtime review before Unreal adoption.

[Editor maps and Horde records](editor-record-persistence.md) documents authenticated raw-map access, worker catalogue refresh and private Horde record storage under the recorded owner decisions. Hosted Horde admission and World persistence remain separate.
