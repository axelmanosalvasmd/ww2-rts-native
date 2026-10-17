## Optional browser assets and command admission (2026-10-16)

The NET runtime batch keeps browser assets outside the executable. `WW2_STATIC_ROOT` selects an existing asset tree; the library uses `Config.static_root`. The four prototype prefixes and three page routes are served only when configured. File reads and gzip work run on blocking workers, away from the async socket executor. Canonical directory confinement also rejects symlinks that leave an allowed prefix. The gzip cache retains at most 256 canonical paths and replaces each entry when its size/mtime tag changes. These are host implementation choices, without browser gameplay or Unreal adoption claims.

The server now submits the core's represented gameplay commands to its existing recorded tick queue. Host-stamped identity and sequence ownership remain unchanged. Existing ground-control safeguards stay in place; other families use core command admission and return its denial through the stable person, including after reconnect. Missing core command families remain refused. Host pause/resume and manual end use existing JSON v0 messages. Map-ping budgets use the existing monotonic Hub clock, so tests can advance the rolling window without sleeping and wall-clock changes cannot reset it. Native match preparation remains synchronous under the Hub lock, so no asynchronous start can publish after manual end. This does not implement general mode startup or change World admission, FFI, wire shapes or recipient visibility.

### Integration with main and R1 ingress

Main retains its support lifecycle guard, save/load, diagnostics, sharding and
World refusal. The static cache and confined routes are shared at the sharded
HTTP front as well as the single-server router. New decisions use this fragment;
main's existing DESIGN.md remains unchanged.

Ground commands and host controls follow #366 at `21a1e3fa`, including its
cover/garrison repairs and complete match socket witness. #296 additionally
admits purchases through the existing core purchase guard and support through
the adapter's narrow allowlist. Attack and other held command families stay
closed until their snapshot lifecycles are available. These are capability
limits, and NET-43 remains partial. The merge does not adopt #369's flight or
event transport and does not weaken its future integration gate.
