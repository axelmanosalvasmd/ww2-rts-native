## Logistics integration (2026-10-17)

The integration retains the existing FireAt command, route and projectile owners and ports the incoming terrain tests onto them. Terrain reloads use suppression captured before the infantry rally phase, matching prototype `shared/sim.js:5346,5559-5566`. Flight projection retains the existing aviation and operative adapters.

Horde uses the existing assigned facility work during its ordinary tick. Idle Horde ticks skip that phase, while direct construction and section repair keep their assignment guards. No new gameplay state or protocol shape is introduced, and World admission stays closed.
