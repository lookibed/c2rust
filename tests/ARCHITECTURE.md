# Test-system architecture

`tests/registry/catalog.json` is the frozen inventory of every c2das-managed C
fixture and runner.  It records a C graph, Clang facts, owner, entrypoint,
expected result or diagnostic state, runtime requirement, and truth status;
`fixtures.json` is its checked expansion.  A new fixture or runner is invalid
until the registry check is updated.

`tests/syntax` is the canonical executable suite.  Every `.c` there is either
a canonical case (executed against its C reference) or a negative case
(required to fail strict translation with a declared diagnostic).  The `.das`
files next to them are regenerated examples of the translator's current
output, kept for reading and for the editor tooling; they are never a runtime
oracle and no test reads them.  Corpus runners remain separate from the fast
suite.

`tests/canonical/cases.json` is the executable manifest.  The one canonical
runner, `scripts/run_c2das_cases.py`, copies each C graph to a temporary
workspace, builds its C reference with `clang-18`, requires fresh c2das output,
executes it with the real `daslang`, and compares stdout and exit status.  A
case either declares its oracle (`expected.exit_code`/`stdout`) or uses the C
program itself as the oracle (`"expected": {"oracle": "c-reference"}`).
Negative cases declare `expected_error.cause`, which must appear in the strict
diagnostic.  `--all-ready` is the gate, `--all-known-red` surveys the cases
that are expected to fail so they can be promoted when they pass.  The runner
locates `daslang` through `DASLANG`, `DASROOT`, the pinned
`tmp/daslang-toolchain`, `PATH`, or `~/daScript`; nothing is machine-specific.

Rust tests never write into the source tree: the snapshot and survey tests
translate into temporary directories, and the insta snapshots in
`c2dascript-transpile/tests/snapshots/*.snap` are the only checked-in
translator output the Rust suite compares against.  A snapshot of a rejected
input is the diagnostic text itself.
