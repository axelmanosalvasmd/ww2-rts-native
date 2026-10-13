# Client/server contracts

Coordination: issue #5. Delivery: issue #4.

These files reserve contract topics; no protocol or ABI is approved by their creation. The pinned reference implementation remains the compatibility source while contracts are documented.

| Document | Draft owner | Consumer reviewer | Status |
| --- | --- | --- | --- |
| protocol.md | Julio | Axel | v0 draft; technical answers recorded (#5); three security corrections pending |
| terrain.md | Julio | Axel | v0 draft; technical answers recorded (#5) |
| rust-unreal-abi.md | Julio | Axel | v0 draft; six ABI 2 view calls approved by both owners for staged adoption; PR #16 merge and current Unreal verification pending; Linux C-link checks recorded |

Contract PRs must state native/reference baseline, version/status, fields and semantics, ownership/privacy, error/recovery behavior, compatibility transition, fixtures and verification evidence. Both owners must explicitly approve breaking changes before adoption. Mark a contract agreed only after approvals and merge. Do not put credentials or private sessions in fixtures.

The ABI 2 approval is limited to the six recipient-view calls in `rust-unreal-abi.md` and their staged Unreal consumer update. It does not approve additional interfaces or the pending protocol/privacy proposals. The broader drafts remain drafts; merged jointly approved scope is authoritative.
