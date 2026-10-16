# COMBAT-RUNTIME batch 1

CMB-145: `match_destroy::tests::intent_cmb145_scuttle_retires_owned_units_and_ejects_passengers_without_refund_or_bounty`.

The prototype at `e4e579a` scuttles own living non-carried units and buildings by setting HP to zero and attributing the loss to their owner (`shared/sim.js:3760-3765`). Retirement happens in the following tick. A passenger leaves a dead or missing carrier onto the nearest usable cell, loses 30% of full squad strength or dies beyond shore reach, and emits hurt before continuing its ground turn (`shared/sim.js:2423-2433,5356-5360`).

The test exercises the parsed Destroy command, ownership and carried-unit denials, both `ids` and `id`, buildings and a loaded halftrack, deferred retirement, physical wrecks and cleared building cells. Four scenes cover a surviving passenger, a fatal injury, a stranded passenger and a carrier already retired before the passenger's next turn. Wallets are compared with a control match's ordinary income; hostile last-hit attribution is overwritten and no enemy kill or bounty is awarded. The surviving rifle squad retains 70 HP from its initial 100.

Runtime changes use the existing internal carrier, placement, health, grid and tick-event owners. Naval and rail transport remain outside this batch. No wire protocol, ABI, privacy or R1 profile change is introduced. Parity table status and owner runtime acknowledgment remain with the coordinator.

Acknowledged issue #5 owner decision 6206861267 permits native behavior tests with independent prototype-intent review; 6207938018 retains exact-head acknowledgment for runtime PRs. ABI 2, the reference pin and draft contract limits remain unchanged. No issue comment is posted by this stream.

CLIENT: CMB-102, CMB-154, CMB-155, CMB-156.

NEXT: CMB-112, CMB-127, CMB-130, CMB-133, CMB-134, CMB-137, CMB-141, CMB-142, CMB-144, CMB-146, CMB-147, CMB-148, CMB-149, CMB-150, CMB-151, CMB-158, CMB-159. Rows 146-150 refer to other streams; 151 needs UI/NET coordination. The existing deterministic random streams stay in place for 159.

Validation on the Vostro: all seven focused core tests and three existing full-tick carrier/command replay checks pass. `cargo fmt --all --check` and strict workspace all-target Clippy pass under the shared slot locks and 6 GiB memory scopes. The private ignored R1 aligned replay was not run; its source and profile remain unchanged.
