# FLIGHTS-4: Horde production views

Reference: `reference/ww2-rts` at e4e579a838aabae3b1d9dadbc17a0a87147bc52d.
Owner decisions E/O/N/P retain prototype fields and per-field recipient rules;
K admits hosted Horde and L permits AI to use delivered recipient views only.

`shared/sim.js:387-389` makes a defender-owned building accessible to allied
Horde defenders, excluding the internal Horde seat. Other modes use ownership.
The same rule chooses an Airfield for immediate aircraft purchase and home.

`shared/sim.js:6022` deliberately bypasses cached owner queues in Horde.
`queues` includes every accessible producer, in actor order, with the exact
row `[buildingId, roundedProgress, rallyX, rallyZ, ...unitKinds]`. Missing rally
coordinates are `-1`; an empty queue has progress zero. This shares queue,
progress and building rally, including empty queues.

`shared/sim.js:6024` keeps `productionJobs` owner-only in Horde as elsewhere.
Paid job identities/refunds, `air`, plans, orders, movement and player economy
are not shared by access to a producer. `air` keeps the existing prototype
row `[id, stateCode, ceil(fuel), ammo, ceil(timer)]` (`shared/sim.js:6053-6054`).
The server adds no fields or protocol version.

The hosted BASE-18 test purchases fighter, attacker and bomber at an allied
Airfield through recorded commands, checks exact shared empty queue rows,
private job and aircraft lists, then patrols and continues all-seat delivery.
The core test also covers invalid, unfinished and enemy makers without payment
or actor-ID mutation. World admission and logistics delivery are outside this
aviation batch.


The integration with main's watcher stream also retains live flight rows.
Prototype `server.js:204-220,248` gives watchers reveal and full unit visibility;
`shared/sim.js:5962-5999` then emits the same flight fields, retained shooter
identities, including retired actors and a contact-bounded 0.15-second horizon. The native watcher uses
that full-map perspective without changing seated fog or match state.
