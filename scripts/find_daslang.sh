#!/usr/bin/env bash
# Print the path of a usable daslang binary, or exit 127.
# Order: $DASLANG, $DASROOT/{bin,build}/daslang, <repo>/tmp/daslang-toolchain/bin/daslang (pinned
# toolchain shared with the Claude Code MCP/LSP servers), `daslang` on PATH, ~/daScript/{bin,build}/daslang.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
for candidate in "${DASLANG:-}" "${DASROOT:+$DASROOT/bin/daslang}" "${DASROOT:+$DASROOT/build/daslang}" \
    "$repo_root/tmp/daslang-toolchain/bin/daslang" "$(command -v daslang 2>/dev/null || true)" "$HOME/daScript/bin/daslang" "$HOME/daScript/build/daslang"; do
    if [[ -n "$candidate" && -x "$candidate" ]]; then
        printf '%s\n' "$candidate"
        exit 0
    fi
done
echo 'daslang not found: set DASLANG or DASROOT, or add daslang to PATH' >&2
exit 127
