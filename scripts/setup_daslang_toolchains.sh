#!/usr/bin/env bash
# Set up the pinned daslang toolchains used by the Claude Code MCP/LSP/DAP servers (.mcp.json,
# .claude/skills/daslang-lsp) under <repo>/tmp/. Idempotent: re-run after `rm -rf tmp/`.
#
#   tmp/daslang-toolchain  daScript 1524b3bf (0.6.4) built with dasHV  -> MCP `daslang`, LSP plugin, lint/test gate
#   tmp/daslang-dap        daScript PR #3937 (5476efa8)               -> MCP `daslang-dap` (debugger bridge)
#
# Sources come from a local daScript clone when DASCRIPT_REPO points at one (default ~/daScript,
# worktrees are added detached), otherwise from a fresh blob-less clone of the upstream repo.
# Build deps: cmake, ninja, a C++17 compiler, libssl-dev (for dasHV), python3 for the servers.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
DASCRIPT_REPO="${DASCRIPT_REPO:-$HOME/daScript}"
UPSTREAM="https://github.com/GaijinEntertainment/daScript.git"
TOOLCHAIN_SHA=1524b3bf62e7decbfe530dc5f2e794b296fa1e68
DAP_SHA=5476efa8770b7a0cd1639d718cba50d35909f3bb
JOBS="${JOBS:-$(nproc)}"

checkout() { # <dir> <sha> <pr-number>
    local dir="$1" sha="$2" pr="$3"
    [[ -e "$dir/utils" ]] && { echo "== $dir already checked out"; return; }
    if [[ -d "$DASCRIPT_REPO/.git" ]]; then
        git -C "$DASCRIPT_REPO" cat-file -e "$sha^{commit}" 2>/dev/null \
            || git -C "$DASCRIPT_REPO" fetch origin "pull/$pr/head" "$sha" 2>/dev/null \
            || git -C "$DASCRIPT_REPO" fetch origin "$sha"
        git -C "$DASCRIPT_REPO" worktree add --detach "$repo_root/$dir" "$sha"
    else
        git clone --filter=blob:none "$UPSTREAM" "$dir"
        git -C "$dir" fetch origin "pull/$pr/head" 2>/dev/null || true
        git -C "$dir" checkout --detach "$sha"
    fi
}

mkdir -p tmp logs
checkout tmp/daslang-toolchain "$TOOLCHAIN_SHA" 3682
checkout tmp/daslang-dap "$DAP_SHA" 3937

echo "== [$(date +%T)] build toolchain"
cmake -S tmp/daslang-toolchain -B tmp/daslang-toolchain/build -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DDAS_MODULES_INCLUDE=dasHV -DDAS_TESTS_DISABLED=ON -DDAS_TUTORIAL_DISABLED=ON -DDAS_AOT_EXAMPLES_DISABLED=ON
ninja -C tmp/daslang-toolchain/build -j"$JOBS" daslang dasModuleHV tree_sitter_daslang

echo "== [$(date +%T)] build dap"
cmake -S tmp/daslang-dap -B tmp/daslang-dap/build -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DDAS_MODULES_INCLUDE= -DDAS_TESTS_DISABLED=ON -DDAS_TUTORIAL_DISABLED=ON -DDAS_AOT_EXAMPLES_DISABLED=ON
ninja -C tmp/daslang-dap/build -j"$JOBS" daslang

echo "== [$(date +%T)] done"
tmp/daslang-toolchain/bin/daslang --version
tmp/daslang-dap/bin/daslang --version
command -v ast-grep >/dev/null || echo "WARNING: ast-grep not on PATH; MCP grep_usage/outline need it (release 0.45.x)"
echo "smoke: python3 scripts/mcp_smoke.py daslang | python3 scripts/lsp_smoke.py | python3 scripts/mcp_smoke.py daslang-dap"
