"""Claude Code stand-in for the README recording: registers like a real
session (~/.claude-obdemo/sessions/<pid>.json + a transcript), draws a small
terminal UI, and answers replies the island types into this tab.

  python3 claude_standin.py SID CWD TITLE BRANCH [--busy SECONDS]
"""
import json, os, sys, time, re, threading, random
SID, CWD, TITLE, BRANCH = sys.argv[1:5]
BUSY = float(sys.argv[sys.argv.index("--busy") + 1]) if "--busy" in sys.argv else 0
HOME = os.path.expanduser("~/.claude-obdemo")
REG = os.path.join(HOME, "sessions", f"{os.getpid()}.json")
TR = os.path.join(HOME, "projects", "-obdemo-" + os.path.basename(CWD), f"{SID}.jsonl")
os.makedirs(os.path.dirname(REG), exist_ok=True)
os.makedirs(os.path.dirname(TR), exist_ok=True)
MODEL = "claude-opus-4-5-20251101"
now = lambda: int(time.time() * 1000)
iso = lambda: time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime())

def reg(status):
    tmp = REG + ".tmp"
    json.dump({"pid": os.getpid(), "sessionId": SID, "cwd": CWD, "status": status,
               "statusUpdatedAt": now(), "kind": "interactive", "version": "2.1.0"}, open(tmp, "w"))
    os.replace(tmp, REG)

def tr(obj):
    obj.update({"sessionId": SID, "cwd": CWD, "gitBranch": BRANCH, "timestamp": iso()})
    with open(TR, "a") as f:
        f.write(json.dumps(obj) + "\n")

def user(text):
    tr({"type": "user", "message": {"role": "user", "content": text}})

def assistant(text, end=True):
    tr({"type": "assistant", "message": {"model": MODEL, "role": "assistant", "type": "message",
        "content": [{"type": "text", "text": text}], "stop_reason": "end_turn" if end else None,
        "usage": {"input_tokens": random.randint(9000, 30000), "output_tokens": random.randint(400, 2400),
                  "cache_read_input_tokens": random.randint(20000, 90000)}}})

DIM, BOLD, RST, ORG, GRN, GRY = "\033[2m", "\033[1m", "\033[0m", "\033[38;5;209m", "\033[38;5;114m", "\033[38;5;245m"
def out(s=""):
    sys.stdout.write(s + "\n"); sys.stdout.flush()

open(TR, "w").close()
user(TITLE)
tr({"type": "ai-title", "aiTitle": TITLE})
os.system("clear")
out(f"{ORG}✻{RST} {BOLD}Claude Code{RST} {GRY}· {os.path.basename(CWD)} · {BRANCH}{RST}")
out()
out(f"{GRY}>{RST} {TITLE}")
out()
if BUSY:
    reg("busy")
    out(f"{ORG}⏺{RST} Working on it…")
    time.sleep(BUSY)
out(f"{GRN}⏺{RST} Done. Tests pass on {BRANCH}.")
assistant("Done. Tests pass.")
reg("idle")
out()

def answer(text):
    out(f"{GRY}>{RST} {BOLD}{text}{RST}")
    out()
    user(text)
    reg("busy")
    time.sleep(0.4)
    out(f"{ORG}⏺{RST} On it.")
    time.sleep(2.6)
    out(f"{GRN}⏺{RST} Added the dark mode toggle and a test for it.")
    out()
    assistant("Added the dark mode toggle and a test for it.")
    reg("idle")

if not sys.stdin.isatty():
    while True:
        time.sleep(3600)
for line in sys.stdin:
    text = re.sub(r"\x1b\[20[01]~", "", line).strip()
    if text:
        sys.stdout.write("\033[1A\033[2K")      # the echoed line goes; the UI prints it
        answer(text)
