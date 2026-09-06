# c2das

**c2das** is an experimental C-to-[daScript](https://dascript.org/) transpiler.
It is an architectural fork of [C2Rust](https://github.com/immunant/c2rust):
the front end keeps C2Rust's Clang-based understanding of C, while the back
end builds and prints daScript AST instead of Rust.

The goal is behavioural translation, not a surface-level C-to-text rewrite.
c2das is under active development and is not yet a complete C ABI or a
production-ready C compiler replacement.

## Architecture

```text
Clang AST -> CBOR -> C AST -> translator -> daScript AST -> printer -> .das
```

![c2das translation roadmap](docs/c2das-roadmap.png)

The translator deliberately keeps C facts separate from daScript
representation. In particular:

- exported Clang facts are the source of truth for C size, alignment, field
  offsets, padding, `packed`, `aligned`, unions, and bitfields;
- the canonical runtime lowers allocation and memory primitives to
  `c2da_rt_*` calls before printing;
- raw addresses, typed pointers, nulls, storage bytes, integer literals, and
  boolean-to-integer conversions use an explicit ABI contract;
- pointer-backed C objects are accessed through address-aware raw-memory
  lowering, with alignment-safe copies for packed or misaligned fields;
- generated daScript is checked by the real `daslang`, not only by Rust
  snapshot tests.

## What is verified

There is no readiness percentage. The only claims made here are the ones the
canonical runner reproduces from fresh output on the real `daslang`:

```sh
python3 scripts/run_c2das_cases.py --all-ready      # every ready case: C reference == fresh daScript
python3 scripts/run_c2das_cases.py --all-known-red  # survey of cases expected to fail
```

- **103 ready cases** in [`tests/canonical/cases.json`](tests/canonical/cases.json):
  the raw-memory/ABI suite `p17`–`p41`, the audit acceptance suite
  `p42`–`p51` (loops, `switch`, `goto`, `static` locals, function pointers,
  evaluation order, C integer semantics, floating point, printer precedence
  and literals, arrays), and the 68 legacy `tests/syntax` programs
  (`c*`, `d*`, `g*`, `p01`–`p10`, `s*`, `t*`, `u*`, `test_*`), which use
  their own `main` as the oracle.
- **4 negative cases** (`n06`–`n08`, `p29`) prove that an unknown external
  call, an unsupported builtin, an unrepresentable field type and a variadic
  function-pointer call are rejected under `--strict` with the declared
  diagnostic and produce no output.
- **1 known-red case**: `plmpeg-stream`, which still stops at the first
  aggregate rvalue read from raw storage (`plm_demux_get_packet`).

On the 97 inherited c2rust unit fixtures (`tests/unit/*/src/*.c`, not part
of the gate) strict translation accepts 66 and 49 of those compile with
`daslang`; 7 of the rejections are Clang errors in intentionally invalid
inputs, the rest are honest diagnostics (`printf`/`strlen` without a
lowering, `__builtin_alloca`, vector fields, flexible array members, inline
asm, statement expressions with declarations).

### What the translator does

- C control flow is rebuilt from the CFG and printed with daScript numeric
  labels (`label N:` / `goto label N`): `for`/`while`/`do`, `switch` with
  fall-through and `default` anywhere, `goto` in any direction, `break`
  and `continue`, early returns. Function-scope `static` objects are hoisted
  to module globals.
- Expressions follow C: integer promotion and the usual arithmetic
  conversions are computed from Clang types (`abi.rs`), `short`/`char`
  wrap in their storage width, `unsigned char` promotes to `int`, comma,
  `&&`/`||` and `?:` evaluate exactly what C evaluates, compound assignment
  and `++`/`--` evaluate their lvalue once, pointer subscripts are signed.
- Literals are exact: double literals print with daslang's `lf` suffix,
  float literals as floats, string literals are NUL-terminated byte arrays
  with static storage, `'\xff'` is `-1`.
- C arrays of constant size are daScript fixed arrays `T[N]` (inline
  storage, copyable, C layout); pointer-backed objects use Clang's layout
  facts (`sizeof`, `alignof`, `offsetof`, padding, packed, bitfields).
- `malloc`/`calloc`/`realloc`/`free`/`memset`/`memcpy`/`memmove`/`memcmp`/
  `memchr` lower to the `c2da_rt_*` runtime prelude emitted into every
  module; every other external call is a strict-mode diagnostic.
- Function pointers are typed daScript function values called through
  `invoke`; `__builtin_popcount/clz/ctz/ffs/bswap*/*_overflow/expect`
  and a few more have real lowerings, every other builtin is a diagnostic.
- The daScript printer renders the AST only: parenthesisation comes from a
  precedence table transcribed from the daslang grammar, and there is no
  text-level repair.

### Known gaps

- Aggregate rvalues read from raw storage, struct copies through pointers
  with a layout that differs from daScript's, by-value structs passed to a
  callee that takes their address, and unions (still a heap handle that
  aliases on copy) are the next semantic layer; plmpeg blocks on the first.
- `_Atomic` is lowered as its plain type; `volatile`, SIMD vectors, inline
  asm, `long double`, `__int128` and packed structs by value are diagnostics.
- The runtime heap is a 64 MiB bump arena without reuse or alignment and
  the byte routines are interpreted loops; it is correct, not fast.
- `va_list` forwarding to another function is rejected.

Unsupported semantics must fail with a precise translation diagnostic rather
than silently becoming an approximation. A construct that translates and
compiles but behaves differently from C is a bug, and the fix belongs in
the translator, never in the generated text.

## Build and translate

The public name is `c2das`, but the current internal Cargo packages and
binaries remain `c2dascript` for compatibility.

Prerequisites on Linux: rustup (the pinned nightly is picked up from
`rust-toolchain.toml`), LLVM/Clang 18 development packages, cmake, a C++
compiler, python3, and a built [daScript](https://github.com/GaijinEntertainment/daScript)
checkout. `llvm-config-N` is discovered on `PATH`; set `LLVM_CONFIG_PATH` only
when several versions are installed. `daslang` is discovered via `DASLANG`,
`DASROOT`, `PATH`, or `~/daScript`.

```sh
sudo apt-get install llvm-18-dev libclang-18-dev clang-18 cmake g++ python3
git clone https://github.com/GaijinEntertainment/daScript ~/daScript
cmake -S ~/daScript -B ~/daScript/build -DCMAKE_BUILD_TYPE=Release
cmake --build ~/daScript/build --target daslang -j
```

Build and run the Rust workspace tests from any checkout of this repository:

```sh
cargo test --workspace
```

Translate one C source file. The generated `.das` is written beside the C
source file:

```sh
cargo run -q -p c2dascript-transpile -- --file tests/syntax/p17_runtime_malloc.c
```

For a real project, prefer its exact compilation database:

```sh
cargo run -q -p c2dascript-transpile -- path/to/compile_commands.json
```

Extra arguments after the input are passed to Clang. They must describe the
real C build: target, include paths, defines, and sysroot all affect the AST
and therefore the translated program.

## Validation pipeline

Validation is layered. A rendered file that merely parses is not a passing
translation.

1. The canonical case runner copies each C graph to a temporary workspace,
   compiles its C reference with `clang-18`, and requires fresh strict c2das
   output there.
2. `daslang` runs the fresh daScript output and compares its stdout and exit
   code either to the declared oracle or, for `"oracle": "c-reference"`
   cases, to what the C program itself produced.
3. Negative cases must fail strict translation with the declared diagnostic
   and write nothing.
4. Rust tests cover the exporter boundary, the ABI/render contracts
   (`ptr_tests`), architecture rules, and insta snapshots of the printed
   output (or of the diagnostic for rejected inputs); they translate into
   temporary directories and never write into the source tree.

```sh
python3 scripts/run_c2das_cases.py --all-ready        # the gate
python3 scripts/run_c2das_cases.py --case p43-switch  # one case
python3 scripts/run_c2das_cases.py --all-known-red    # survey expected failures
cargo test -p c2dascript-transpile                    # Rust suite
python3 scripts/check_test_registry.py --check        # fixture registry is derived from cases.json
```

`daslang` is found through `DASLANG=/path/to/daslang`, `DASROOT`, the pinned
toolchain under `tmp/daslang-toolchain` (see
`scripts/setup_daslang_toolchains.sh`), `PATH`, or `~/daScript`. The
registry in `tests/registry/fixtures.json` exposes every remaining fixture's
exact status instead of treating it as covered.

## Development principles

- Keep the C2Rust architecture where it provides a sound front-end model;
  port mechanisms, not Rust-specific output assumptions.
- Make one canonical owner for every ABI rule. Do not spread pointer casts,
  layout arithmetic, or memory conversion policy across expression lowering.
- Treat Clang layout metadata as C ABI truth. daScript struct layout is a
  different contract unless a representation has been explicitly proven safe.
- Prefer raw-memory operations for pointer-backed objects and union storage.
  Do not replace them with identity casts or direct union field access.
- A known unsupported feature must produce a location-rich diagnostic. A
  plausible-looking but semantically wrong `.das` is a bug.
- Every foundational feature requires Rust AST/render assertions and actual
  `daslang` execution before it is considered complete.

## Relationship to C2Rust

c2das began as a fork of C2Rust and retains substantial C2Rust front-end and
translator architecture. C2Rust is the reference for analysing Clang AST,
preserving C semantics, handling control flow, and organising a durable
translator. c2das differs at the target boundary: it constructs daScript AST,
has a daScript printer, and owns a target-specific raw-memory runtime and ABI
layer.

## Contributing

Issues and patches should describe the C input, Clang invocation, generated
daScript, and the result from the real `daslang` run. Small reproductions in
`tests/syntax` are preferred over textual workarounds. New semantics should
extend the canonical layer that owns the behaviour and add an executable
fixture.

## License and acknowledgements

c2das is distributed under the [BSD-3-Clause license](LICENSE). It contains
and adapts components originating in C2Rust; their notices and third-party
licenses remain in the repository. C2Rust was inspired by Jamey Sharp's
[Corrode](https://github.com/jameysharp/corrode) translator and uses
Emscripten's Relooper approach for arbitrary C control flow.

daScript is an independent language and runtime. See
[dascript.org](https://dascript.org/) for its documentation and licensing.
