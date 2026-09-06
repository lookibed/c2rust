# real-world-h264bsd-mp4

Manual real-world fixture for c2dascript using the Spider h264bsd + minimp4 case.

This directory intentionally keeps the translated daScript output next to the C inputs:

- `src/module.c`, `src/h264bsd.c`, `src/minimp4.c`, `src/shim.c`
- `src/module.das`, `src/h264bsd.das`, `src/minimp4.das`, `src/shim.das`
- `src/module_all.das` and `src/all.das` combined verification targets
- `compile_commands.json` with local paths for this copy

The original Spider fixture notes are preserved in `README.spider.md`.

## Check daScript Output

From the c2dascript repository root:

```powershell
powershell -ExecutionPolicy Bypass -File tests\manual\real-world-h264bsd-mp4\run_dascheck.ps1
```

By default this checks `src/all.das` with the Windows daScript binary at:

```text
D:\Backups\с2daslang\daScript\bin\Release\daslang.exe
```

Check another generated file:

```powershell
powershell -ExecutionPolicy Bypass -File tests\manual\real-world-h264bsd-mp4\run_dascheck.ps1 -File src\module_all.das
```

## Transpile Input

The local compile database is:

```text
tests\manual\real-world-h264bsd-mp4\compile_commands.json
```

The expected development pipeline is still:

1. edit code in the Windows c2dascript tree
2. apply the changed source files to your checkout
3. build/transpile from WSL
4. verify generated `.das` with the Windows `daslang.exe`

