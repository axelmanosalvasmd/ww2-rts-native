## Boarding from houses and active work (2026-10-16)

Board keeps the prototype's distance and seat assignment before exiting any
house or cancelling work (`shared/sim.js:3520-3540` at `e4e579a`). For assigned
squads, reuse `PreparedGroundExit` and the recipient routing preview used by
Retreat. Route from each staged exit with that exit's sight included, then
publish the complete admitted batch. This preserves canonical unit identity,
grid placement and recipient knowledge without a second boarding state.

The existing owned-work cancellation clears paid Dig, shared project membership
and the construction target together. It neither refunds a job nor removes the
paid construction site. Squads without a reserved seat keep their work and
house position. No command, persistent field, wire shape or ABI is added.

Retreat already retains its final home across bounded route legs. New tests
exercise those legs in Default, Conquest and Classic. Default and Conquest
collect complete recipient snapshots for every seat; Classic retains its
existing complete-snapshot refusal. World territory routing and complete World
room delivery remain outside this batch.
