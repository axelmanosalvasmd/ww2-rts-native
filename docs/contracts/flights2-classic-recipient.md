# Classic recipient fields used by aircraft training

Owner decisions E/O/N/P permit prototype fields and per-field recipient rules. This implementation uses the JSON WebSocket v0 shapes at prototype `e4e579a838aabae3b1d9dadbc17a0a87147bc52d`; it adds no ABI or header surface.

- `mun` and `fuel`: integer floors of the receiving seat's canonical wallets, including zero. Allies and enemies receive their own balances. Conquest leaves these keys absent because its source wallets are undefined (`shared/sim.js:6017`, Classic initialization `1682`).
- `mode`: the public object `{kind:"classic", timeLeft:<nonnegative ceil seconds>, suddenDeath:<boolean>}`. Undefined prototype properties such as `defenderTeam`, `attackerTeam` and `bunkers` remain absent. The internal `teams` count is not a wire field (`shared/sim.js:5895-5897,1679,6032`).
- `nodes`: public rows `[x,z,rate,fuel]`, with coordinates rounded to tenths, unrounded resource rate, and fuel represented as 0 or 1 (`shared/sim.js:6019`).

- `ghosts`: a recipient-filtered list, empty when no remembered building rows exist, including before the first tick. Existing remembered rows retain the recipient visibility filter (`shared/sim.js:6036`).

The ordinary aircraft owner rows, queues, paid production jobs and E/O events retain their existing prototype encodings. `match_aviation::home_tests::mov84_classic_constructs_two_airfields_and_trains_each_plane_at_its_paid_producer` checks these Classic fields on every seat throughout real Airfield construction and six aircraft training completions. World admission remains closed.
