//! Fails when `include/ww2_core.h` is not what cbindgen writes from the current source, so Unreal
//! never builds against a stale header. To regenerate it:
//!
//!   WW2_WRITE_HEADER=1 cargo test -p ww2-ffi --test header

use std::path::Path;

#[test]
fn committed_header_is_current() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = cbindgen::Config::from_file(dir.join("cbindgen.toml")).expect("cbindgen.toml");
    let bindings =
        cbindgen::generate_with_config(dir, config).expect("cbindgen could not read the crate");
    let mut generated = Vec::new();
    bindings.write(&mut generated);
    let path = dir.join("include/ww2_core.h");
    if std::env::var_os("WW2_WRITE_HEADER").is_some() {
        std::fs::write(&path, &generated).expect("write include/ww2_core.h");
        return;
    }
    // A Windows checkout may turn the committed file's line ends into CRLF.
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        committed.as_bytes() == generated.as_slice(),
        "include/ww2_core.h is stale: run `WW2_WRITE_HEADER=1 cargo test -p ww2-ffi --test header` and commit it"
    );
}
