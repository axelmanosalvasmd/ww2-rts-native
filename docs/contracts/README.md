# Client/server contracts

Coordination: issue #5. Delivery: issue #4.

These files reserve contract topics; no protocol or ABI is approved by their creation. The pinned reference implementation remains the compatibility source while contracts are documented.

| Document | Draft owner | Consumer reviewer | Status |
| --- | --- | --- | --- |
| protocol.md | Julio | Axel | Pending inventory of existing WebSocket messages |
| terrain.md | Julio | Axel | Pending coordinate/height/knowledge agreement |
| rust-unreal-abi.md | Julio | Axel | Pending client query inventory and ABI proposal |

Contract PRs must state native/reference baseline, version/status, fields and semantics, ownership/privacy, error/recovery behavior, compatibility transition, fixtures and verification evidence. Both owners must explicitly approve breaking changes before adoption. Mark a contract agreed only after approvals and merge. Do not put credentials or private sessions in fixtures.
