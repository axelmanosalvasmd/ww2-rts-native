#!/usr/bin/env bash
# Repeatable negative link check. Arguments: old ABI-2.0 install, new install. Outputs stay in WW2_FFI_WORK.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OLD="${1:?old install required}"
NEW="${2:?new install required}"
WORK="${WW2_FFI_WORK:?set WW2_FFI_WORK to isolated scratch}"
mkdir -p "$WORK/old-header" "$WORK/tmp"
export TMPDIR="$WORK/tmp"
cp "$HERE/fixtures/ww2_core-abi2.h" "$WORK/old-header/ww2_core.h"
LIBS=(-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc)
if cc -std=c11 -Wall -Wextra -Werror -pedantic -I "$WORK/old-header" "$HERE/abi_guard.c" "$OLD/lib/libww2_ffi.a" "${LIBS[@]}" -o "$WORK/old-header-guard" >"$WORK/old-header.log" 2>&1; then
	echo "old header unexpectedly compiled the ABI 2.1 consumer" >&2
	exit 1
fi
if cc -std=c11 -Wall -Wextra -Werror -pedantic -I "$NEW/include" "$HERE/abi_guard.c" "$OLD/lib/libww2_ffi.a" "${LIBS[@]}" -o "$WORK/old-library-guard" >"$WORK/old-library.log" 2>&1; then
	echo "old library unexpectedly linked the ABI 2.1 consumer" >&2
	exit 1
fi
# Do not count arbitrary compiler/linker failures as a passing rejection.
rg 'WW2_ABI_MINOR_VERSION|ww2_abi_minor_version' "$WORK/old-header.log" >/dev/null
rg 'undefined reference to .ww2_abi_minor_version' "$WORK/old-library.log" >/dev/null
cc -std=c11 -Wall -Wextra -Werror -pedantic -I "$NEW/include" "$HERE/abi_guard.c" "$NEW/lib/libww2_ffi.a" "${LIBS[@]}" -o "$WORK/new-guard"
"$WORK/new-guard"
echo "PASS: previous header rejected at compile, previous archive rejected at link, current archive ready"
