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

The approved default source is `8f37717cb0b78fca9786ae0bd1188160504ccdf4`, reference
`e4e579a838aabae3b1d9dadbc17a0a87147bc52d`. The Unreal consumer requires major 2 and minor at least 1
for placement and eight detached authoring queries. This provider reports minor 2; its five IF-11 exports are
not adopted. An old ABI-2.0 archive has no minor/query symbols and fails the new consumer link. The wrapper
checks major before minor, preserves the old row layouts, and refuses query calls on an incompatible version.

`WW2_FFI_WORK` and `WW2_FFI_DEST` select isolated scratch and staging folders. `CARGO_BUILD_JOBS` defaults
to eight and must be one through eight. After `verify` succeeds against a staging destination,
`ww2-ffi.sh install-staged PATH` checks the staged STAMP and C guard and replaces the installed files only
with no engine clients/builds running. `status` checks the requested source and every installed file hash.
The STAMP includes the full native/reference revisions, provider minor and required consumer minor.

`Tools/ffi/check-compatibility.sh OLD_INSTALL NEW_INSTALL` keeps the previous-header compile and
previous-archive link rejections. Set `WW2_FFI_WORK` to isolated scratch for its outputs. Its committed old
header fixture is unmodified; archives remain generated output. Real Unreal/editor/package acceptance
is separate from these build, C-link and Rust fixture checks.
