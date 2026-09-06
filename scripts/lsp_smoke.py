#!/usr/bin/env python3
"""LSP stdio smoke: initialize -> didOpen tests/syntax/t10_chain.das -> collect publishDiagnostics -> documentSymbol -> shutdown."""
import json, os, subprocess, sys, time
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
cmd = ["python3", f"{root}/tmp/daslang-toolchain/utils/lsp/lsp_supervisor.py"]
p = subprocess.Popen(cmd, cwd=root, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=open(f"{root}/logs/smoke_lsp_stderr.log","ab"))
def send(msg):
    b = json.dumps(msg).encode(); p.stdin.write(f"Content-Length: {len(b)}\r\n\r\n".encode() + b); p.stdin.flush()
def recv():
    hdr=b""
    while b"\r\n\r\n" not in hdr:
        ch=p.stdout.read(1)
        if not ch: raise SystemExit(f"lsp closed (rc={p.poll()})")
        hdr+=ch
    n=int([l for l in hdr.decode().split("\r\n") if l.lower().startswith("content-length")][0].split(":")[1])
    return json.loads(p.stdout.read(n))
f = sys.argv[1] if len(sys.argv) > 1 else f"{root}/tests/syntax/t10_chain.das"; uri="file://"+f
send({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":os.getpid(),"rootUri":"file://"+root,"capabilities":{},"initializationOptions":{"compiler":"tmp/daslang-toolchain/bin/daslang","project_root":"."}}})
r=recv(); caps=r["result"]["capabilities"]; print("initialize caps:", sorted(k for k,v in caps.items() if v))
send({"jsonrpc":"2.0","method":"initialized","params":{}})
send({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"daslang","version":1,"text":open(f).read()}}})
send({"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{"textDocument":{"uri":uri}}})
got_diag=None; got_sym=None; t=time.time()
while (got_diag is None or got_sym is None) and time.time()-t<180:
    m=recv()
    if m.get("method")=="textDocument/publishDiagnostics": got_diag=m["params"]["diagnostics"]
    elif m.get("id")==2: got_sym=m.get("result")
print(f"diagnostics: {None if got_diag is None else len(got_diag)} ({time.time()-t:.1f}s)")
for d in (got_diag or [])[:5]: print("  ", d.get("severity"), d["range"]["start"], d["message"][:120])
print("documentSymbol:", None if got_sym is None else len(got_sym), [s.get("name") for s in (got_sym or [])[:8]])
send({"jsonrpc":"2.0","id":3,"method":"shutdown"}); recv(); send({"jsonrpc":"2.0","method":"exit"}); p.wait(timeout=20); print("exit", p.returncode)
