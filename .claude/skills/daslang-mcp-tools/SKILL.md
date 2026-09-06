---
name: daslang-mcp-tools
description: Reference for the daslang MCP server tools (compile_check, lint, grep_usage, outline, find_symbol, run_test, format_file, cpp_* and live_* tools). Invoke when choosing how to search, compile, lint, or run .das code.
---

# daslang MCP tools (c2das)

Read the full tool table and notes first:
`tmp/daslang-toolchain/skills/mcp_tools.md`.

How the server is wired in this project (`.mcp.json`): the `daslang` server is the pinned
toolchain's `utils/mcp/mcp_supervisor.py` with `--repo-root .`, so the daslang child runs
with cwd = c2das root and the pinned binary `tmp/daslang-toolchain/bin/daslang`.

Path conventions that follow from that:

- `compile_check`, `lint`, `run_test`, `run_script`, `format_file`, `find_symbol`,
  `goto_definition`, `find_references`, `type_of`: project-relative paths work
  (`tests/syntax/t10_chain.das`), absolute paths work too.
- `grep_usage` and `outline` resolve relative paths against the toolchain root, so always
  pass absolute paths: `directory: <repo root>/tests/syntax`,
  `file: <repo root>/tests/syntax/t10_chain.das`.
- `cpp_grep_usage`, `cpp_outline`, `cpp_find_symbol` work on C/C++ sources: the C inputs
  under `tests/` and the Clang exporter in `c2rust-ast-exporter/src`; pass absolute paths.
- The MCP results are development aids. `scripts/run_c2das_cases.py` with the real
  `daslang` and `cargo test` are authoritative.
