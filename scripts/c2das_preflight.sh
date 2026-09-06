#!/usr/bin/env bash
# Linux preflight. Runs from any Git checkout of this repository; needs cargo, python3 and daslang.
set -euo pipefail

# Make the runner independent of interactive shell configuration while preserving ordinary CI PATHs.
if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
fi
command -v cargo >/dev/null 2>&1 || { echo 'cargo is unavailable' >&2; exit 127; }

mode=fast
while (($#)); do
    case "$1" in
        --full) mode=full ;;
        --extended) mode=extended ;;
        --fast) mode=fast ;;
        *) echo "usage: $0 [--fast|--full|--extended]" >&2; exit 64 ;;
    esac
    shift
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export CARGO_TARGET_DIR="${C2DAS_CARGO_TARGET_DIR:-$root/.c2das-target}"

gate() { local name="$1"; shift; printf '\n== c2das gate: %s ==\n' "$name"; "$@"; }

check_repository() {
    git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1 || {
        echo "checkout must be a Git repository: $root" >&2; return 1;
    }
    printf 'native git checkout %s\n' "$(git -C "$root" rev-parse --short HEAD)"
}

check_untracked() {
    local files; files="$(git -C "$root" ls-files --others --exclude-standard)"
    [[ -z "$files" ]] || { echo "untracked artifacts are forbidden:" >&2; printf '%s\n' "$files" >&2; return 1; }
    if git -C "$root" ls-files --others --exclude-standard '*.das' | grep -q .; then
        echo 'unregistered generated .das artifact found' >&2; return 1
    fi
}

check_architecture() {
    cargo test -p c2dascript-transpile --test architecture_tests
    cargo test -p c2dascript-transpile --test governance_tests
}
check_changed_fixture_assertions() { cargo test -p c2dascript-transpile --test ptr_tests; }
check_runtime_owners() {
    local forbidden
    forbidden="$(grep -RInE 'normalize_generated_numeric_patterns|normalize_first_phase_shift_assignments|replace_generated_function' "$root/c2dascript-transpile/src/translator" || true)"
    [[ -z "$forbidden" ]] || {
        echo 'forbidden post-render semantic repair found:' >&2
        printf '%s\n' "$forbidden" >&2
        return 1
    }
    forbidden="$(grep -RInE 'let mut [A-Za-z_][A-Za-z0-9_]* = .*\.to_string\(\)' "$root/c2dascript-transpile/src/translator" || true)"
    [[ -z "$forbidden" ]] || {
        echo 'mutable rendered-source repair found:' >&2
        printf '%s\n' "$forbidden" >&2
        return 1
    }
}
check_corpus_inventory() {
    test -f "$root/docs/followups/real_world_status.md"
    test -f "$root/tests/manual/real-world-h264bsd-mp4/UPSTREAM.md"
    if find "$root/tests/manual/real-world-h264bsd-mp4/upstream" -type d -name .git -print -quit | grep -q .; then
        echo 'nested Git metadata remains in versioned H264 fixture input' >&2; return 1
    fi
    grep -Fq '| PLMPEG stream |' "$root/docs/followups/real_world_status.md"
}

gate repository check_repository
gate untracked-artifacts check_untracked
gate test-registry python3 "$root/scripts/check_test_registry.py" --check
gate rustfmt cargo fmt --check
gate translator-architecture check_architecture
gate changed-fixture-assertions check_changed_fixture_assertions
gate runtime-owner-invariants check_runtime_owners
gate canonical-c2das-runtime python3 "$root/scripts/run_c2das_cases.py" --all-ready
gate isolated-exporter-known-red python3 "$root/scripts/run_c2das_cases.py" --all-exporter-failures
gate plmpeg-c-graph bash "$root/tests/manual/real-world-plmpeg-stream/check_c_graph.sh"

if [[ "$mode" == full || "$mode" == extended ]]; then gate workspace-tests cargo test --workspace; fi
if [[ "$mode" == extended ]]; then
    gate real-world-ledger check_corpus_inventory
    if grep -Fq '| PLMPEG stream | canonical repository graph | known red |' "$root/docs/followups/real_world_status.md"; then
        echo 'PLMPEG is explicitly known-red; refusing to label its runner a success.' >&2; exit 2
    fi
    gate plmpeg-end-to-end bash "$root/tests/manual/real-world-plmpeg-stream/run_end_to_end.sh"
fi
printf '\n== c2das preflight %s: PASS ==\n' "$mode"
