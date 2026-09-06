---
name: daslang-testing
description: dastest conventions for writing and running component tests (test discovery, [test] functions, assertions, expect-files). Invoke before adding or editing anything under tests/.
---

# daslang testing (c2das)

Read the pinned toolchain's instructions in full first:
`tmp/daslang-toolchain/skills/writing_tests.md`.

c2das specifics:

- The authoritative C-to-daScript check is `scripts/run_c2das_cases.py`: it transpiles each
  case, runs the output with the real `daslang`, and compares stdout and exit code with the
  compiled C reference. Point it at the pinned toolchain with
  `DASLANG=tmp/daslang-toolchain/bin/daslang`.
- `tests/syntax/*.das` and `tests/unit/*/src/*.das` are expected outputs; run any of them
  with `mcp__daslang__compile_check` or `mcp__daslang__run_script` to check that they still
  compile under the pinned compiler.
- Rust-side snapshot and architecture tests: `cargo test -p c2dascript-transpile`.
- When an expected value is non-obvious, cite the C construct and source line in a comment.
