# Sandbox and convoy command composition

Prototype-identical shapes at `e4e579a`:

- `spawn`: `{t:'spawn',unit,x,z,enemy?}`. The sandbox flag gates free ordinary actor publication; enemy selects the first enemy seat (or Horde slot). See `shared/sim.js:3408-3417,4348-4357` and the sandbox contract.
- `supply`: `{t:'supply',ids?,orders?,target}`; `logisticsResume`: `{t:'logisticsResume',ids?,orders?}`. IDs retain selection order and duplicates, with orders fallback only when IDs is not an array. See `shared/supply-convoys.js:573-600` and the logistics contract.
- Non-World `recover` and unknown orders return `blocked`, as `shared/sim.js:3415,3812` requires. World recovery remains deferred with World room admission.

Commands share the recorded tick queue and the ordinary tick, reserve, convoy and recipient owners. This composition adds no wire field, message kind, save schema or ABI declaration.
