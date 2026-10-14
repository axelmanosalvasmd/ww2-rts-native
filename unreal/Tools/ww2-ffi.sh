#!/usr/bin/env bash
# Builds the Rust core's C interface (ww2-ffi, native repo rust/crates/ffi) at a pinned native commit and installs it
# where the WW2Core plugin links it from. The installed files are build output: git ignores them, and this script is
# the only supported way to make them.
#
#   unreal/Tools/ww2-ffi.sh build    build libww2_ffi.a at $WW2_FFI_REV with the commit's pinned toolchain, check it,
#                                    and install it with ww2_core.h, the formation fixture, the recorded protocol
#                                    streams with their browser-oracle reports (decompressed) and a STAMP
#   unreal/Tools/ww2-ffi.sh verify   build as above, plus the crate's own tests (header freshness, error codes,
#                                    panic boundary) and the C example against the formation fixture and every
#                                    protocol stream; needs crates.io once
#   unreal/Tools/ww2-ffi.sh status   what is installed, and whether the files still match their STAMP
#   unreal/Tools/ww2-ffi.sh remove   delete the installed files (the plugin then builds without the Rust core)
#
# Environment (all optional):
#   WW2_FFI_REV      native commit to build (default below: main at 92fb7fe, ABI 2 with the six ww2_view_* calls of
#                    PR #16, merged at d88ef6d; header and fixtures byte-identical to the PR's 03a49ac)
#   WW2_NATIVE_SRC   an existing clone of axelmanosalvasmd/ww2-rts-native to build in. It must be clean; it is checked
#                    out at WW2_FFI_REV (detached). Default: a clone this script makes in unreal/Saved/WW2Ffi/native
#                    (git ignores Saved/), fetched over HTTPS (the repository is public)
#   RUSTUP           rustup to run the pinned toolchain with (default: from PATH, else ~/.cargo/bin/rustup)
#
# The toolchain is the one the native commit pins in rust/rust-toolchain.toml. The script never installs it: when it
# is missing it stops and prints the rustup command (user-local, no root). Only Linux x86_64 is built and proven;
# Win64 (x86_64-pc-windows-msvc) and macOS arm64 (aarch64-apple-darwin) are planned and not attempted here.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$HERE/.." && pwd)"
REV="${WW2_FFI_REV:-92fb7fe66eb7f788c83449ae67223984b969f177}"
TRIPLE="x86_64-unknown-linux-gnu"
# What `cargo rustc -p ww2-ffi --release --target $TRIPLE -- --print native-static-libs` must list. WW2Core.Build.cs
# links exactly these; a build that needs others stops here instead of linking short.
EXPECTED_LIBS="-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc"
# Every symbol WW2Core calls. ABI 1: version, rules hash, last error and the two formation calls. ABI 2 adds the six
# recipient-view calls (docs/contracts/rust-unreal-abi.md, "Recipient-view calls").
SYMBOLS="ww2_abi_version ww2_rules_hash ww2_last_error ww2_formation_slot_size ww2_formation_spots
	ww2_view_new ww2_view_free ww2_view_apply ww2_view_unit_count ww2_view_units ww2_view_needs_resync"
# The recorded server-message streams (harness/protocol-record.mjs) and the browser oracle's report after each message
# (harness/view-check.mjs), from rust/crates/core/tests/fixtures. Installed decompressed for the plugin's replay check.
PROTOCOL_STREAMS="protocol-conquest-p0 protocol-conquest-p1 protocol-world-p0 protocol-world-p1"
DEST="$PROJECT_DIR/Plugins/WW2Core/ThirdParty/ww2_ffi/Linux/$TRIPLE"
WORK="$PROJECT_DIR/Saved/WW2Ffi"
SRC="${WW2_NATIVE_SRC:-$WORK/native}"
REMOTE="https://github.com/axelmanosalvasmd/ww2-rts-native.git"

die() { echo "ww2-ffi: $*" >&2; exit 1; }
say() { echo "ww2-ffi: $*"; }
tool() { # tool name: from PATH, else rustup's default bin folder
	if [[ -n "${!2:-}" ]]; then echo "${!2}"; elif command -v "$1" >/dev/null; then command -v "$1";
	elif [[ -x "$HOME/.cargo/bin/$1" ]]; then echo "$HOME/.cargo/bin/$1"; else die "$1 not found (install rustup: https://rustup.rs, user-local)"; fi
}
sha() { sha256sum "$1" | cut -d' ' -f1; }
# The STAMP key for an installed protocol file: its name with '-' and '.' as '_', then _sha256 (WW2Core.Build.cs agrees).
protocol_key() { echo "$(echo "$1" | tr -- '-.' '__')_sha256"; }
protocol_files() { local s; for s in $PROTOCOL_STREAMS; do echo "$s.jsonl"; echo "$s.expected.jsonl"; done; }

[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || die "only Linux x86_64 is supported; Win64 and macOS arm64 are pending"
[[ "$REV" =~ ^[0-9a-f]{40}$ ]] || die "WW2_FFI_REV must be a full 40-character commit id, got '$REV'"

checkout() {
	if [[ -z "${WW2_NATIVE_SRC:-}" && ! -d "$SRC/.git" ]]; then
		say "cloning $REMOTE into $SRC"
		mkdir -p "$WORK"
		git clone --quiet "$REMOTE" "$SRC" || die "clone failed (check the network, or point WW2_NATIVE_SRC at a clone)"
	fi
	[[ -d "$SRC/.git" || -f "$SRC/.git" ]] || die "$SRC is not a git checkout"
	[[ -z "$(git -C "$SRC" status --porcelain --untracked-files=no)" ]] || die "$SRC has local changes; use a clean clone"
	if ! git -C "$SRC" cat-file -e "$REV^{commit}" 2>/dev/null; then
		say "fetching $REV"
		git -C "$SRC" fetch --quiet origin || die "fetch failed in $SRC"
		git -C "$SRC" cat-file -e "$REV^{commit}" 2>/dev/null || die "commit $REV is not in $REMOTE"
	fi
	git -C "$SRC" checkout --quiet --detach "$REV"
	[[ "$(git -C "$SRC" rev-parse HEAD)" == "$REV" ]] || die "checkout of $REV failed"
}

build() { # sets LIB, HEADER, FIXTURE, RUSTC_VV
	local rust="$SRC/rust"
	[[ -f "$rust/rust-toolchain.toml" ]] || die "$rust/rust-toolchain.toml missing: not a native commit with the Rust workspace"
	[[ -f "$rust/crates/ffi/include/ww2_core.h" ]] || die "no ww2-ffi crate at $REV"
	local rustup channel
	rustup="$(tool rustup RUSTUP)"
	channel="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$rust/rust-toolchain.toml")"
	[[ -n "$channel" ]] || die "no channel in rust-toolchain.toml"
	if ! "$rustup" run "$channel" rustc --version >/dev/null 2>&1; then
		die "the pinned toolchain $channel is not installed. Install it user-local with:
    (cd '$rust' && '$rustup' toolchain install)"
	fi
	RUSTC_VV="$(cd "$rust" && "$rustup" run "$channel" rustc -vV | tr '\n' ';')"
	[[ "$RUSTC_VV" == *"release: $channel;"* ]] || die "rustc reports '$RUSTC_VV', expected release $channel"
	local libs
	# The system libraries the archive needs, from rustc itself (--crate-type staticlib: only the archive is linked).
	libs="$(cd "$rust" && touch crates/ffi/src/lib.rs && "$rustup" run "$channel" cargo rustc --locked -p ww2-ffi --release --target "$TRIPLE" --crate-type staticlib -- --print native-static-libs 2>&1 \
		| sed -n 's/.*note: native-static-libs: //p' | tail -n 1)"
	[[ "$libs" == "$EXPECTED_LIBS" ]] || die "native-static-libs is '$libs', but WW2Core.Build.cs links '$EXPECTED_LIBS': update both together"
	if [[ "${VERIFY:-0}" == 1 ]]; then
		say "running the ww2-ffi tests (header freshness, error codes, panic boundary)"
		(cd "$rust" && "$rustup" run "$channel" cargo test --locked -p ww2-ffi --release --target "$TRIPLE") || die "ww2-ffi tests failed"
	fi
	# Last, so the installed archive is the one the documented command makes.
	say "building ww2-ffi at $REV with rustc $channel for $TRIPLE"
	(cd "$rust" && touch crates/ffi/src/lib.rs && "$rustup" run "$channel" cargo build --locked -p ww2-ffi --release --target "$TRIPLE") || die "cargo build failed"
	LIB="$rust/target/$TRIPLE/release/libww2_ffi.a"
	HEADER="$rust/crates/ffi/include/ww2_core.h"
	FIXTURE="$rust/crates/core/tests/fixtures/formation.txt"
	PROTOCOL_SRC="$rust/crates/core/tests/fixtures"
	[[ -f "$LIB" && -f "$HEADER" && -f "$FIXTURE" ]] || die "build output missing ($LIB, $HEADER, $FIXTURE)"
	local f
	for f in $(protocol_files); do [[ -f "$PROTOCOL_SRC/$f.gz" ]] || die "protocol fixture missing: $PROTOCOL_SRC/$f.gz"; done
	# grep reads all of nm's output (no -q): with pipefail, an early exit would fail nm with SIGPIPE
	local fn
	for fn in $SYMBOLS; do
		nm -g --defined-only "$LIB" 2>/dev/null | grep " T $fn$" >/dev/null || die "$LIB does not define $fn"
	done
}

c_example() {
	local rust="$SRC/rust" out="$WORK/link_check"
	say "running the C example against the fixture"
	mkdir -p "$WORK"
	(cd "$rust" && cc -std=c11 -Wall -Wextra -Werror -pedantic -I crates/ffi/include crates/ffi/examples/link_check.c "$LIB" $EXPECTED_LIBS -o "$out") || die "the C example did not build"
	"$out" "$FIXTURE" >"$WORK/link_check.log" || { tail -n 5 "$WORK/link_check.log"; die "the C example failed (log $WORK/link_check.log)"; }
	tail -n 2 "$WORK/link_check.log"
	local s
	for s in $PROTOCOL_STREAMS; do
		gzip -dc "$PROTOCOL_SRC/$s.jsonl.gz" >"$WORK/$s.jsonl"
		"$out" "$FIXTURE" "$WORK/$s.jsonl" >"$WORK/link_check-$s.log" || { tail -n 5 "$WORK/link_check-$s.log"; die "the C example failed on $s (log $WORK/link_check-$s.log)"; }
		echo "$s: $(grep 'view messages' "$WORK/link_check-$s.log")"
		rm -f "$WORK/$s.jsonl"
	done
}

install_files() {
	rm -rf "$DEST"
	mkdir -p "$DEST/lib" "$DEST/include" "$DEST/fixtures/protocol"
	cp "$LIB" "$DEST/lib/libww2_ffi.a"
	cp "$HEADER" "$DEST/include/ww2_core.h"
	cp "$FIXTURE" "$DEST/fixtures/formation.txt"
	local f
	for f in $(protocol_files); do gzip -dc "$PROTOCOL_SRC/$f.gz" >"$DEST/fixtures/protocol/$f"; done
	# STAMP last: WW2Core.Build.cs reads it, checks the three files against it, and lists it as a dependency so
	# UnrealBuildTool notices a new install.
	{
		echo "native_rev=$REV"
		echo "target=$TRIPLE"
		echo "rustc=$(echo "$RUSTC_VV" | sed -n 's/.*release: \([^;]*\);.*/\1/p')"
		echo "rustc_commit=$(echo "$RUSTC_VV" | sed -n 's/.*commit-hash: \([^;]*\);.*/\1/p')"
		echo "native_static_libs=$EXPECTED_LIBS"
		echo "lib_sha256=$(sha "$DEST/lib/libww2_ffi.a")"
		echo "header_sha256=$(sha "$DEST/include/ww2_core.h")"
		echo "fixture_sha256=$(sha "$DEST/fixtures/formation.txt")"
		for f in $(protocol_files); do echo "$(protocol_key "$f")=$(sha "$DEST/fixtures/protocol/$f")"; done
	} >"$DEST/STAMP.tmp"
	mv "$DEST/STAMP.tmp" "$DEST/STAMP"
	say "installed in $DEST"
	cat "$DEST/STAMP"
}

status() {
	[[ -f "$DEST/STAMP" ]] || { say "not installed ($DEST/STAMP missing): the plugin builds without the Rust core"; return 1; }
	cat "$DEST/STAMP"
	local bad=0 key file
	local checks=(lib_sha256:lib/libww2_ffi.a header_sha256:include/ww2_core.h fixture_sha256:fixtures/formation.txt) f
	for f in $(protocol_files); do checks+=("$(protocol_key "$f"):fixtures/protocol/$f"); done
	for key in "${checks[@]}"; do
		file="$DEST/${key#*:}"
		if [[ ! -f "$file" ]]; then echo "missing: $file"; bad=1;
		elif [[ "$(sha "$file")" != "$(sed -n "s/^${key%%:*}=//p" "$DEST/STAMP")" ]]; then echo "changed since install: $file"; bad=1; fi
	done
	[[ $bad == 0 ]] && say "installed files match their STAMP" || die "installed files do not match their STAMP: run build again"
}

case "${1:-}" in
	build) checkout; build; install_files ;;
	verify) checkout; VERIFY=1 build; c_example; install_files ;;
	status) status ;;
	remove) rm -rf "$DEST"; say "removed $DEST" ;;
	*) sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
