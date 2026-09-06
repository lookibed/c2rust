---
name: daslang-formatting
description: Formatting rules for .das files (gen2 layout, MCP format_file, .lint_config policy). Invoke after creating or modifying any hand-written .das file under tests/.
---

# daslang formatting (c2das)

Read the pinned toolchain's instructions in full before formatting:
`tmp/daslang-toolchain/skills/das_formatting.md`.

c2das rules on top of it:

- Hand-written and expected `.das` files start with `options gen2` (see `tests/syntax/`).
- Format with the MCP tool `mcp__daslang__format_file`, never with a shell-invoked
  compiler. Pass absolute paths (`<repo root>/tests/syntax/<file>.das`).
- Never format transpiler output or snapshot files by hand
  (`c2dascript-transpile/tests/snapshots/*.snap`); their layout is produced by the printer
  and checked by the snapshot tests. Fix the printer instead.
- Do not format scratch files under `logs/`, `tmp/`, or `target/`.
