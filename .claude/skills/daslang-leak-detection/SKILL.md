---
name: daslang-leak-detection
description: Diagnosing daslang memory leaks and teardown crashes (--das-profiler-leaks, --track-smart-ptr, GC APP LEAK, HandleRegistry, jobque leaks). Invoke when a runtime test crashes on free/delete or the leak dump is non-empty.
---

# daslang leak detection (c2das)

Read `tmp/daslang-toolchain/skills/memory_leak_detection.md` in full first, then
`tmp/daslang-toolchain/skills/jobque_debugging.md` if channels or job status are involved.

c2das context: transpiled C uses raw heap memory (`malloc`/`calloc`/`free` mapped onto the
runtime's raw allocation helpers, see `skills/raw_memory_abi.md` and `skills/object_memory.md`
in the repo root) rather than daScript `new`/`delete`. A crash on free or a non-empty leak
dump usually means the translator paired an allocation with the wrong release path, or a
runtime helper lost the raw address. Reproduce with `scripts/run_c2das_cases.py --keep-workdir`, then
use the `daslang-dap-debugging` skill to step through the generated program.
