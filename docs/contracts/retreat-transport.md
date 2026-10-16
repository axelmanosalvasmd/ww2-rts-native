# Retreat and transport command results

Reference: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
The command result strings below are identical to the prototype. This records
existing prototype semantics under owner decisions M and N from issue #5. It
adds no message, recipient field, FFI entry point or World room admission.

| Command | Result | Prototype condition | Reference |
| --- | --- | --- | --- |
| `unload` | `needs` | No eligible selected carrier has cargo. | `shared/sim.js:3565-3566` |
| `unload` | `shore` | Every selected loaded carrier is a boat with its nearest dry or wadeable cell more than `CFG.shoreReach` (5 m) away. | `shared/sim.js:3567-3569`, `CFG` at `shared/sim.js:111` |
| `rail` | `needs` | No eligible selected ground units remain after excluding air, naval and forced logistics units. | `shared/sim.js:3546-3548` |
| `rail` | `station` | No owned origin is within 30 m of an eligible unit, or no other owned station exists. | `shared/sim.js:3549-3552` |
| `rail` | `cooldown` | The origin station's team departure clock has not elapsed. | `shared/sim.js:3553` |
| `rail` | `railCut` | No route crosses only owned regions and standing bridges. | `shared/sim.js:3554-3555`, `shared/sim.js:1252-1267` |

Successful unloads process every landing carrier and keep an offshore boat's
cargo aboard when another carrier can unload. Each rider uses the nearest free
cell under its own movement mask. Manual unload applies no wreck damage.
These are `shared/sim.js:2422-2433,3564-3570` semantics.

The native core exposes these results through `Denial::name()`. The existing
server denial envelope and throttling remain unchanged. Rail intent tests use
the existing World core component; they do not admit a running World room or
World recipient snapshots. The ordinary boat lifecycle uses the existing
recipient rows without adding or changing fields.
