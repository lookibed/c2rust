# Initial preflight baseline — 2026-08-28

This is the committed starting measurement before the new gates are treated as a compatibility
claim.  A red row is a real baseline failure, not an exemption or a green result.

| Gate | Result | Evidence / next owner |
|---|---|---|
| Windows Rust test invocation | retired | Windows is no longer a source or validation workspace. |
| `cargo fmt --check` | red | Existing workspace formatting differs from the configured rustfmt across pre-existing files; format normalization is a separate, reviewable cleanup. |
| Canonical Git checkout | green | Any native Linux Git clone of `lookibed/c2das`; no source synchronization is involved. |
| Governance contracts | green | `cargo test -p c2dascript-transpile --test governance_tests`: 6 passed. |
| Existing translator architecture | green | `cargo test -p c2dascript-transpile --test architecture_tests`: 11 passed. |
| PLMPEG C graph | green | `tests/manual/real-world-plmpeg-stream/check_c_graph.sh` completed before the next chained gate. |
| Canonical c2das runtime cases | green | `python3 scripts/run_c2das_cases.py --all-ready` proves fresh C reference and daScript equivalence for p17–p28, p30–p41. |
| Isolated exporter known-red contracts | green / empty | `python3 scripts/run_c2das_cases.py --all-exporter-failures` confirms no registered fixture currently depends on a frontend crash; process-boundary negative controls remain in Rust. |
| Legacy ABI daScript shell suite | known red / retired from preflight | It consumes checked-in output and stops at missing `tests/syntax/p26_variadic_sum.das`; it remains inventory evidence, not a runtime gate. |

The new preflight is intentionally fail-closed from this baseline forward.  Each row turns green
only through its owner layer and a new recorded result.
