#!/usr/bin/env bash
# Real-world backend gates.
set -euo pipefail

root="$(cd "$(dirname "$0")/../../.." && pwd)"
probe="$root/tests/manual/real-world-plmpeg-stream/src/variadic_abi_probe.c"
macro_probe="$root/tests/manual/real-world-plmpeg-stream/src/macro_origin_probe.c"
inc="$root/tests/manual/real-world-plmpeg-stream"
daslang="$(bash "$root/scripts/find_daslang.sh")"

cargo run -q -p c2dascript-transpile -- \
    --file "$probe" \
    -DPLM_NO_STDIO \
    -I"$inc/include" \
    -I"$inc/upstream" \
    -I"$inc/fixtures" \
    -I"$inc/src"

"$daslang" "${probe%.c}.das" -main plmpeg_variadic_abi_probe

cargo run -q -p c2dascript-transpile -- \
    --file "$macro_probe" \
    -DPLM_NO_STDIO \
    -I"$inc/include" \
    -I"$inc/upstream" \
    -I"$inc/fixtures" \
    -I"$inc/src"

"$daslang" "${macro_probe%.c}.das" -main plmpeg_macro_origin_probe

# The checked-in source corpora have no ASM/SIMD spelling at this revision.
# A match is a new unclassified surface and must update the inventory first.
if grep -R -n -E --include='*.c' --include='*.h' \
    '__asm__|asm[[:space:]]*[(]|__builtin_shufflevector|__builtin_convertvector|vector_size|ext_vector_type' \
    "$root/tests/manual/real-world-plmpeg-stream" \
    "$root/tests/manual/real-world-h264bsd-mp4"; then
    echo "new ASM/SIMD surface requires inventory classification" >&2
    exit 1
fi
