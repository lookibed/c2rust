#!/usr/bin/env bash
# Produce the single c2das output for the canonical PLMPEG C graph.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
fixture="$root/tests/manual/real-world-plmpeg-stream"
src="$fixture/src"
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"

bash "$fixture/check_c_graph.sh"

cargo run -q -p c2dascript-transpile -- \
    --file "$src/all.c" \
    -DPLM_NO_STDIO \
    -I"$fixture/include" \
    -I"$fixture/upstream" \
    -I"$fixture/fixtures" \
    -I"$src"

test -f "$src/all.das"
