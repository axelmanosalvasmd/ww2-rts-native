## Mine contact admission before dust

Share mine detonation preflight between ground contact and the damage owner. Ground contact checks hostile mine identity, combat metadata and environment inputs before raising dust; the damage owner checks again at detonation. Friendly mines skip damage admission. Successful contact retains dust before blast as in `shared/sim.js:5513-5533` at `e4e579a`. Refusal restores the precontact dust and preserves the existing full World state assertion. World room admission remains closed.
