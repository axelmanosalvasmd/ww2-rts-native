## Logistics integration (2026-10-17)

The integration retains the existing FireAt command, route and projectile owners and ports the incoming terrain tests onto them. Terrain reloads use suppression captured before the infantry rally phase, matching prototype `shared/sim.js:5345,5559-5566`. Flight projection retains the existing aviation and operative adapters.

Horde uses the existing assigned facility work during its ordinary tick. Idle Horde ticks skip that phase, while direct construction and section repair keep their assignment guards. No new gameplay state or protocol shape is introduced, and World admission stays closed.

The imported LOG-13 stance change is preserved. The pinned prototype's forced-unit whitelist names `holdFire`, `holdPos` and `autoRetreat`, while its actual message type is `stance` (`shared/sim.js:3419,3574-3579`), so its recorded fixtures reject that message. Native LOG-13 deliberately admits the three controls. The two fixture consumers retain every checkpoint and exact state assertion but replace the expected stance result, follow reset and held-position value for those cases. Recorded fixture bytes remain unchanged. These two cases establish native intent, not literal equality with the captured refusal; no parity status or owner acceptance is promoted here.
