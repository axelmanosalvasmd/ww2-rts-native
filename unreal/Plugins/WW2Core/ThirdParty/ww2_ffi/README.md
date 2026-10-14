# ww2_ffi build output (not in git)

`unreal/Tools/ww2-ffi.sh build` installs the Rust core's C interface here, built from a pinned commit of
`axelmanosalvasmd/ww2-rts-native`:

```
Linux/x86_64-unknown-linux-gnu/
  lib/libww2_ffi.a         cargo build -p ww2-ffi --release --target x86_64-unknown-linux-gnu
  include/ww2_core.h       rust/crates/ffi/include/ww2_core.h at the same commit (cbindgen output, committed there)
  fixtures/formation.txt   rust/crates/core/tests/fixtures/formation.txt (the pinned JavaScript's facingSpots)
  fixtures/protocol/       rust/crates/core/tests/fixtures/protocol-{conquest,world}-p{0,1}{,.expected}.jsonl.gz,
                           decompressed: recorded server messages and the browser oracle's report after each
  STAMP                    commit, rustc, system libraries and the sha256 of every installed file
```

`WW2Core.Build.cs` links the library only when all of them are present and the files match the STAMP. With none of
them it builds without the Rust core and says so (set `TC_REQUIRE_WW2_FFI=1` to make that an error). A partial or
changed install is always an error. Win64 and macOS arm64 folders are planned, not built or verified.
