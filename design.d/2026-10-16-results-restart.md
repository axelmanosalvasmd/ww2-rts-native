## Fresh deterministic seeds on restart

The trusted host derives each new match seed by adding the last published runtime epoch to its configured seed, with wrapping u64 arithmetic. Its first match retains the configured seed. Failed preparation leaves the epoch unchanged, so retrying a refused restart uses the same pending seed and keeps the current match. Random weather and environment seeds derive from the new match seed through the existing native streams. The private startup receipt records these seeds for replay.

This preserves the prototype's fresh-match intent at `server.js:359-389,688-689` while following the native requirement for recorded deterministic randomness. It introduces no socket field, command, save field, ABI change or mode admission.
