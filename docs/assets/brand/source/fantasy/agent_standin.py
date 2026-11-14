"""Hook-agent stand-in (gemini | pi | opencode) for the README recording:
talks to the island over its socket as the installed hooks would.

  python3 agent_standin.py PROVIDER SID CWD TITLE [--busy SECONDS]
"""
import json, os, socket, sys, time, threading, re
PROV, SID, CWD, TITLE = sys.argv[1:5]
BUSY = float(sys.argv[sys.argv.index("--busy") + 1]) if "--busy" in sys.argv else 0
SOCK = os.path.expanduser("~/Library/Application Support/com.openbar.app.dev/claude-hooks.sock")
now = lambda: int(time.time() * 1000)

def send(hook, ev, keep=False):
    s = socket.socket(socket.AF_UNIX)
    s.connect(SOCK)
    s.sendall(json.dumps({"openbar": PROV, "hook": hook, "pid": os.getpid(), "event": ev}).encode())
    if keep:
        return s
    s.shutdown(socket.SHUT_WR)
    try:
        s.settimeout(2); s.recv(65536)
    except Exception:
        pass
    s.close()

def out(s=""):
    sys.stdout.write(s + "\n"); sys.stdout.flush()

os.system("clear")
out(f"{PROV} · {os.path.basename(CWD)}\n> {TITLE}\n")
if PROV == "gemini":
    base = {"session_id": SID, "cwd": CWD}
    send("SessionStart", {**base, "hook_event_name": "SessionStart"})
    send("BeforeAgent", {**base, "hook_event_name": "BeforeAgent", "prompt": TITLE})
    if BUSY:
        time.sleep(BUSY)
        send("AfterAgent", {**base, "hook_event_name": "AfterAgent", "prompt_response": "Done."})
    while not sys.stdin.isatty():
        time.sleep(3600)
    for line in sys.stdin:
        t = re.sub(r"\x1b\[20[01]~", "", line).strip()
        if t:
            out("> " + t)
else:
    base = {"session": SID, "directory": CWD}
    send("event", {**base, "kind": "created", "at": now()})
    send("event", {**base, "kind": "title", "title": TITLE, "at": now()})
    send("event", {**base, "kind": "status", "status": "busy", "at": now()})
    if BUSY:
        time.sleep(BUSY)
        send("event", {**base, "kind": "status", "status": "idle", "at": now()})
    while True:
        try:
            s = send("listen", {"kind": "listen"}, keep=True)
            data = b""
            while True:
                chunk = s.recv(65536)
                if not chunk:
                    break
                data += chunk
            s.close()
            if data:
                try:
                    out("> " + json.loads(data)["prompt"]["text"])
                except Exception:
                    pass
        except Exception:
            time.sleep(1)
