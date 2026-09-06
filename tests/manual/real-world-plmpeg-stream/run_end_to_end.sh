#!/usr/bin/env bash
# Honest PLMPEG WSL gate:
# C reference -> recorded oracle -> fresh C->daScript output -> daslang -> cmp.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
fixture="$root/tests/manual/real-world-plmpeg-stream"
daslang="$(bash "$(dirname "${BASH_SOURCE[0]}")/../../../scripts/find_daslang.sh")"
expected="$fixture/plmpeg_reference.expected"
work="$(mktemp -d "${TMPDIR:-/tmp}/c2das-plmpeg.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# Keep validation isolated from a developer's incremental Cargo cache.  The
# location is configurable for CI, but deliberately lives in the checkout by
# default so LLVM/bindgen artefacts are reused across gate runs.
export CARGO_TARGET_DIR="${C2DAS_TARGET_DIR:-$root/target-runtime-validation}"

test -x "$daslang" || { echo "daslang is not executable: $daslang" >&2; exit 1; }
test -f "$expected" || { echo "missing recorded C oracle: $expected" >&2; exit 1; }

# This gate is rooted in the checkout, so run it before copying the fixture to
# its isolated output directory.
bash "$fixture/check_c_graph.sh"

# A complete fixture copy gives the transpiler a unique output directory.  In
# particular, a pre-existing src/all.das can never satisfy this runner.
cp -a "$fixture/." "$work/fixture"
f="$work/fixture"
src="$f/src"

clang-18 -std=c11 -DPLM_NO_STDIO \
    -I"$f/include" -I"$f/upstream" -I"$f/fixtures" -I"$src" \
    "$src/all_reference.c" "$src/plmpeg_reference_entry.c" \
    -o "$work/plmpeg_reference"
"$work/plmpeg_reference" > "$work/reference.actual"
cmp -- "$expected" "$work/reference.actual" || {
    echo "recorded C reference changed; update it deliberately after review" >&2
    diff -u "$expected" "$work/reference.actual" >&2 || true
    exit 1
}

cargo run -q -p c2dascript-transpile -- \
    --file "$src/all.c" -DPLM_NO_STDIO \
    -I"$f/include" -I"$f/upstream" -I"$f/fixtures" -I"$src"

test -f "$src/all.das" || { echo "transpiler did not produce fresh all.das" >&2; exit 1; }
"$daslang" "$src/plmpeg_entry.das" > "$work/das.actual"
cmp -- "$expected" "$work/das.actual" || {
    echo "daScript result diverges from recorded C reference" >&2
    diff -u "$expected" "$work/das.actual" >&2 || true
    exit 1
}

echo "PLMPEG end-to-end gate passed"
