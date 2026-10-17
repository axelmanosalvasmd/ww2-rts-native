# Owner economy projection

Reference: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`, `shared/sim.js:723,1177,1682,6015-6020`. This records prototype-identical fields in the internal Rust recipient projection. It introduces no C interface or public mode admission.

Each field reads only the receiving player's canonical economy. `mp` is floored and `inc` is rounded to one decimal. Construction modes initialize `mun` and `fuel` to zero and project both as floored numbers. Other modes omit both keys, even though native accounting keeps zero balances. Optional `fuelInc` and `upkeep` are omitted when absent and otherwise rounded to one decimal, using the core's deterministic rounding.

`SnapshotCache` admits matching Default, Conquest, Assault, Annihilation, Classic and Tutorial state. It retains the existing actor capability checks and refuses mismatched mode state. This is an internal projection; full transport composition and public room startup retain their own admission rules. World and Horde remain outside this cache. Horde currency initialization and the same wallet projection leaf are tested separately; that does not qualify a hosted Horde snapshot.

Validation: `economy_currency_tests::base01_currency_initialization_and_owner_flooring_follow_mode` checks different wallets for both seats and literal floor/round results, then advances ordinary ticks and captures every seat. `base01_absent_optional_income_fields_stay_absent` checks omission, and `base01_tutorial_and_horde_initialize_mp_and_omit_construction_currencies` covers the special scripted rosters.
