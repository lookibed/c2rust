---
name: daslang-macros
description: Compile-time macros and AST programming in daslang (quote/qmacro, [macro] passes, TypeDeclPtr/ExpressionPtr ownership). Invoke before writing or debugging any macro or AST-manipulating code.
---

# daslang macros (c2das)

Read `tmp/daslang-toolchain/skills/das_macros.md` in full first; the concise reference is
`.claude/skills/daslang/references/macros.md`.

c2das emits daScript from C, so macros appear only in hand-written runtime support code, never
in translator output. Prefer a literal rendering of the C construct; reach for a macro only
when daScript cannot express the C form directly, and document the C construct it replaces.
