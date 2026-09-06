# Post-render debt inventory

This is a historical ledger, not a compatibility claim.  It records the
generated-text rewrites removed from `c2dascript-transpile/src/translator/mod.rs`
on 2026-08-27 and names the owner that must implement the C semantics before a
surface is called supported.

## Static reachability proof

Baseline source range: `translator/mod.rs:2919` through EOF, beginning with
`fn normalize_generated_numeric_patterns(mut code: String) -> String` and
including `replace_generated_function` and
`normalize_first_phase_shift_assignments`.

The active `convert_translation_unit` path ends by constructing `DaModule` and
returning `module.to_string()` directly.  A repository search found no call to
`normalize_generated_numeric_patterns`; the only calls to the two helpers were
inside that unreachable function.  Therefore every row below was **dead in the
current render path** before deletion.

Reproducible evidence commands (run from the repository root):

```sh
rg -n -C 4 "module\.to_string\(\)|normalize_generated_numeric_patterns|normalize_first_phase_shift_assignments|replace_generated_function" c2dascript-transpile/src/translator/mod.rs
rg -n "normalize_generated_numeric_patterns\(" c2dascript-transpile/src
```

Expected baseline result: one direct `module.to_string()` return in the active
function; one definition of the outer normalizer; helper uses only nested in
that normalizer.  Post-deletion, the architecture test records the inverse
invariant: none of the three implementation symbols may exist in
`translator/`, and no mutable value may be initialized from
`DaModule::to_string()`.

## Ledger

Every record below had active reachability `dead in current render path`.
“Fixture/diagnostic” is the proof obligation; it deliberately does not claim
that removing a workaround implemented the named semantic.

| Former exact predicate(s) | Category | Canonical owner | Fixture / exact boundary |
| --- | --- | --- | --- |
| `malloc(4)`, generated definitions/replacements of `malloc`, `calloc`, `realloc`, `free`, `memset`, `memcpy`, `memmove`, `memcmp`, `memchr`, `mallocz` | runtime/libc | `runtime.rs`, `functions.rs`, `abi.rs` | p17–p19 and runtime AST assertions prove the canonical nine memory calls. `mallocz` remains a precise unsupported libc diagnostic until routed. |
| generated definitions/replacements of `strlen`, `strdup` | runtime/libc | `functions.rs`, future string-storage owner | A C `strlen`/`strdup` fixture must either execute through a typed implementation or report `TranslationError`; no generated function body replacement. |
| `cast<T?>(0)`, `reinterpret<T?>(uint64(0))`, `cast<array<T>>(0)`, pointer-null comparison text | pointer/null, array/default-init | `abi.rs`, `value_lowering.rs`, `pointers.rs`, `object_memory.rs` | p20/p21 plus null/typed-pointer fixtures; arrays require a C aggregate initialization fixture or diagnostic. |
| `unsafe(addr(...[0]))`, `reinterpret<...?>`, raw-address arithmetic, pointer index parenthesis repairs | pointer/null | `abi.rs`, `pointers.rs`, `object_memory.rs`, `layout.rs` | pointer arithmetic, pointer-backed field, and alignment-safe load/store fixtures; unsupported aggregate rvalues diagnose. |
| hard-coded `firstPhase` shifts, `<<`/`>>` count rewrites, integer promotion/cast repairs, `uint8`/enum literal edits | numeric/shift | `operators.rs`, `value_lowering.rs`, `abi.rs` | Typed shift fixture checks C result type and daScript shift-count type; literals fixture checks target-type construction. |
| boolean-condition text such as `uint64(x) != 0`, unary `!` parenthesis fixes, `int(bool)` substitutions | condition/precedence | `value_lowering.rs`, `operators.rs`, `WithStmts` | bool-in-return/assignment/binary/call fixture, or a precise diagnostic where statement lowering cannot be placed. |
| assignment splitting (`n = int(tmp = ...)`), side-effect ordering repairs, assertion call replacement | condition/precedence | `WithStmts`, CFG lowering | fixture with assignment expression and side-effect sequencing; no text split. |
| standalone `break` replaced with `if (false)` and `goto label ...` replacement | CFG | `cfg/structures.rs`, `cfg/relooper.rs`, `cfg/inc_cleanup.rs` | switch/loop exit classification fixture; unsupported CFG shape must be a `TranslationError`. |
| `memory_read_callback`/`write_callback` renamed to `null`, callback call expressions replaced by `0` | callback | `functions.rs`, `DaType`, future callback ABI | typed callback call/nullable guard fixture, or exact callback-ABI diagnostic. |
| `header_annexb_size(...)`, `build_annexb_sample(...)`, `minimp4_vector_alloc_tail(...)` calls replaced by `0` or altered arguments | corpus-specific hack | owning call/runtime/aggregate layer | H264 WSL end-to-end first failure becomes a tracked owner issue; until then it must diagnose, never return `0`. |
| replacements of `FindSmallestPicOrderCnt`, `Mmcop4`, `Mmcop5`, `DecodeMbPred`, `DecodeSubMbPred`, `MvPrediction8x8`, `h264bsdIntraChromaPrediction`, `h264bsdIntra4x4Prediction` function bodies | corpus-specific hack | CFG, aggregate ABI, callback/object-memory owners | H264 WSL end-to-end goal; unsupported bodies remain source-location diagnostics, never fake exported definitions. |
| `if !code.contains("def main(")` plus injected H264/PLMPEG/default `main` | fixture entrypoint | fixture wrappers and runners | runner supplies entrypoint. Translator never injects `main`. |
| named H264/MP4 string-address substitutions, enum/table literal substitutions, `c2da_fresh*` zeroing, project-local arithmetic replacements | corpus-specific hack | the typed source owner named by its first failure | each is either covered by a focused C fixture or rejected with a precise diagnostic; no project-name branch may enter lowering. |
| `replace_generated_function` line scanner | function-body replacement mechanism | none — prohibited | Source-invariant test forbids its implementation name and any post-render mutable source. |

## Permanent render invariant

The only render boundary is `DaModule` → `DaModule::to_string()` → printer
output.  Printer output is immutable with respect to translation semantics.
Semantic repairs belong to typed AST/lowering owners above; fixture entrypoints
belong to fixture runners.  A source-invariant test enforces this rule.

## Open debt: recover semantics, never the rewriter

The removal is intentionally fail-closed, not a declaration that every former
H264/PLMPEG path now works.  The outstanding debt is to run PLMPEG and then
H264 through the WSL end-to-end pipeline, take the *first* real translation or
daScript failure, and create a focused C fixture under its canonical owner.
Each item must end in one of two states:

1. typed lowering plus Rust AST/render assertion and real WSL execution; or
2. a source-location `TranslationError` naming the unsupported C operation.

Forbidden resolution: restoring a text replacement, replacing a call with a
constant/default value, replacing a generated function body, or injecting an
entrypoint into translator output.

## Proof gates

For a deletion change, record results of:

```sh
cargo test --workspace
cargo test -p c2dascript-transpile --test architecture_tests
bash tests/syntax/check_abi_das.sh
rg -n "normalize_generated_numeric_patterns|normalize_first_phase_shift_assignments|replace_generated_function" c2dascript-transpile/src/translator
```

The ABI runner is executed by the real `daslang`; PLMPEG then
H264 are future end-to-end goals only.  Neither receives readiness credit
until it is transpiled and executed through that pipeline without a text
workaround.
