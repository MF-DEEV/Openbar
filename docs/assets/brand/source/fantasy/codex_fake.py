"""Fake Codex app-server for the README recording: one loaded thread.

  venv/bin/python codex_fake.py CODEX_HOME CWD NAME BRANCH
"""
import asyncio, json, os, sys, time
import websockets
HOME, CWD, NAME, BRANCH = sys.argv[1:5]
SOCK = os.path.join(HOME, "app-server-control", "app-server-control.sock")
os.makedirs(os.path.dirname(SOCK), exist_ok=True)
if os.path.exists(SOCK):
    os.remove(SOCK)
T0 = time.time()

def thread():
    busy = time.time() - T0 < float(os.environ.get("CODEX_BUSY", "9"))
    return {"id": "th-demo-1", "cwd": CWD, "name": NAME, "preview": NAME, "model": "gpt-5.5-codex",
            "gitInfo": {"branch": BRANCH}, "createdAt": int(T0), "updatedAt": int(time.time()),
            "status": {"type": "active", "activeFlags": []} if busy else {"type": "idle"}, "turns": []}

async def handler(ws):
    async for raw in ws:
        m = json.loads(raw)
        if "id" not in m:
            continue
        meth = m.get("method")
        if meth == "initialize":
            res = {"userAgent": "codex_cli_rs/0.164.0"}
        elif meth == "thread/loaded/list":
            res = {"data": ["th-demo-1"]}
        elif meth in ("thread/read", "thread/resume"):
            res = {"thread": thread()}
        else:
            res = {}
        await ws.send(json.dumps({"jsonrpc": "2.0", "id": m["id"], "result": res}))
        if meth == "thread/read":
            async def later():
                await asyncio.sleep(float(os.environ.get("CODEX_BUSY", "9")) + 0.5)
                await ws.send(json.dumps({"method": "thread/status/changed",
                                          "params": {"threadId": "th-demo-1", "status": {"type": "idle"}}}))
            asyncio.create_task(later())

async def main():
    async with websockets.unix_serve(handler, SOCK):
        print("codex fake on", SOCK, flush=True)
        await asyncio.Future()

asyncio.run(main())
