# Preflight skill

Run `bash scripts/c2das_preflight.sh` from the repository root for canonical validation.  The optional
PowerShell wrapper only opens that same command in WSL; it neither synchronizes nor validates a
Windows checkout.  Use `--full` for workspace tests and `--extended` for corpus inventory/green
corpus execution.
