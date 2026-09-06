#!/usr/bin/env python3
"""Drive an MCP stdio server from .mcp.json entry NAME: initialize, tools/list, then CALLS (json list)."""
import json, os, subprocess, sys, time
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
name = sys.argv[1]
calls = json.loads(sys.argv[2]) if len(sys.argv) > 2 else []
cfg = json.load(open(os.path.join(root, ".mcp.json")))["mcpServers"][name]
env = dict(os.environ)
for k, v in (cfg.get("env") or {}).items():
    env[k] = v.replace("${PATH}", os.environ.get("PATH", ""))
p = subprocess.Popen([cfg["command"]] + cfg["args"], cwd=root, env=env,
                     stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=open(f"{root}/logs/smoke_{name}_stderr.log", "ab"), text=True, bufsize=1)
seq = 0
def rpc(method, params=None, notify=False):
    global seq
    msg = {"jsonrpc": "2.0", "method": method}
    if params is not None: msg["params"] = params
    if not notify:
        seq += 1; msg["id"] = seq
    p.stdin.write(json.dumps(msg) + "\n"); p.stdin.flush()
    if notify: return None
    while True:
        line = p.stdout.readline()
        if not line:
            raise SystemExit(f"server closed stdout (rc={p.poll()})")
        try: r = json.loads(line)
        except Exception: continue
        if r.get("id") == seq: return r
t=time.time()
r = rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "smoke", "version": "0"}})
print("initialize:", json.dumps(r.get("result", r))[:200], f"({time.time()-t:.1f}s)")
rpc("notifications/initialized", {}, notify=True)
t=time.time()
r = rpc("tools/list", {})
tools = [x["name"] for x in r.get("result", {}).get("tools", [])]
print(f"tools/list: {len(tools)} tools ({time.time()-t:.1f}s)"); print(" ", " ".join(tools))
for c in calls:
    t=time.time()
    r = rpc("tools/call", {"name": c["name"], "arguments": c.get("arguments", {})})
    res = r.get("result") or r.get("error")
    txt = json.dumps(res, ensure_ascii=False)
    print(f"\n--- {c['name']} {json.dumps(c.get('arguments',{}))} ({time.time()-t:.1f}s) isError={res.get('isError') if isinstance(res,dict) else None}")
    print(txt[:1500])
p.stdin.close(); p.wait(timeout=30); print("\nserver exit:", p.returncode)
