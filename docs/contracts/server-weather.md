# Server weather startup

Reference: `reference/ww2-rts` at `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`.
This implements MODE-07's server portion using the existing weather fields.

The host may send `{"t":"weather","v":"random"}` in the lobby. Accepted
values are `map`, `clear`, `fog`, `rain`, `mud`, `snow`, and `random`, identical
to `shared/weather.js:22` and `server.js:678-679`. Other seats and invalid
values cannot change the setting. An attached match locks the setting, including
during its result hold, and the current setting carries into restart.

Each trusted start or restart samples a fresh 31-bit weather seed before match
construction, matching the range and per-match sampling of `server.js:381`.
The private `NATIVE_MATCH` receipt already carries `options.weather`,
`options.weatherSeed`, and `derivedWeatherSeed`. Weather seed is now explicit
rather than constructor-derived. Replay uses the recorded input and never
samples entropy. Entropy failure refuses startup before publication and keeps
any installed match and receipt intact.

This adds no wire fields. Ordinary `start.weather` and snapshot `weather` keep
the prototype's existing current/announced-next rows (`server.js:317,466` and
`shared/weather.js:114-119`); seeds stay private to the host. Existing recipient
snapshots remain the only weather delivery producer. The server supplies the
existing deterministic core with a seed and does not draw randomness on ticks.

Owner decision B assigns the Unreal weather picker to the client owner. Its
Rust portion is accepted under that decision. This change implements no picker
and claims no Unreal execution or acceptance.


## Explicit owner-R1 host profile

The opt-in `--trusted-profile owner-r1` uses the previously requested bounded
owner profile instead of the prototype startup entropy policy above. It passes
`weather_seed: None` to the unchanged canonical constructor and reuses the host's
explicit match seed on Start and healthy Restart. No OS weather entropy is
sampled for this profile. The resulting derived seeds remain host-private in the
existing receipt. Default prototype starts retain the behavior above. This adds
no wire field or client-selected option and does not establish client acceptance.
