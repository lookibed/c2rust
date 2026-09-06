# Workspace hygiene

- Keep source, generated `.das`, logs, and temporary probes separate.  Never use a deploy,
  corpus, or source directory as scratch space.
- Generated `.das` is evidence, not source: it may be committed only when registered as an
  intentional fixture artifact.  Do not overwrite a checked-in oracle while diagnosing.
- Build, test, commit, and push from the Git checkout you are working in.  Never hardcode an
  absolute checkout, toolchain, or runtime path in docs, scripts, or tests; discover tools via
  `PATH` or environment variables (`LLVM_CONFIG_PATH`, `DASLANG`, `DASROOT`) with sane defaults.
- Before changing translator semantics, read `CODEX.md`, the nearest `ARCHITECTURE.md`, the
  nearest `REVIEW.md`, and the matching skill in `skills/`.
- Unsupported C semantics fail closed with a source-located diagnostic.  No generated-text
  replacement, fixture-specific body rewrite, or silent daScript-value fallback is permitted.
