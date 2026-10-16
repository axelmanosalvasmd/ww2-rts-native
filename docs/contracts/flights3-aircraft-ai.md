# Aircraft AI and existing recipient fields

FLIGHTS-3 follows owner decisions E/O for existing projectile/event fields and L for AI planning through recipient observations. Prototype baseline: `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.

The internal commander readiness fact means the receiving seat's own aircraft is in state `base`. It is the same information carried by that seat's private `air` row, whose state code is zero. It is false for allied aircraft and other states. No wire field or ABI field is added. The observation producer already filters enemy units by the receiving seat's visibility. Aircraft planning reads those current contacts, never hidden sighting memories (`shared/ai.js:591-602`).

Ground army centre excludes aircraft and falls back to the seat's spawn. A ready attacker selects the nearest visible non-building ground vehicle or AT, flak, mortar or howitzer within a strict 120 metres of that centre. Other ready aircraft move toward the nearest visible enemy aircraft, or the centre. Equal distances use unit ID, retaining source insertion order in canonical native unit order.

Existing JSON WebSocket v0 shapes remain identical to `shared/sim.js:6016-6062`: `air` is `[id,state,ceil(fuel),ammo,ceil(timer)]`, queues carry progress, rally and unit names, and `productionJobs` retain original paid charges and active/waiting status. These lists belong to the receiving owner. Aircraft appear at their selected producer regardless of rally (`shared/sim.js:1071-1079`). Classic head jobs train only at finished living buildings outside Sudden Death (`shared/sim.js:1065-1070,5352`); fighter/attacker/bomber train for 30/35/45 seconds (`shared/sim.js:381`).

This batch adds no World or Horde room admission. Complete mixed rows retain their other stream and deferred World dependencies.

Trusted Classic room attachment now supplies the existing public `nodes` rows to the global list-version cache. The bytes match the recipient projection: `[round(x*10)/10,round(z*10)/10,rate,fuel?1:0]` (`shared/sim.js:6019`). Classic mode and canonical elimination state must match; other held modes still refuse. Public lobby startup is unchanged.
