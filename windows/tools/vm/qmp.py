#!/usr/bin/env python3
"""Eyes and hands for the headless test VM, over QEMU's QMP socket.

  qmp.py shot out.png            screenshot of the VM display
  qmp.py keys ret                press keys (QEMU key names, e.g. ret, esc, spc, meta_l-r)
  qmp.py type "text"             type ASCII text
  qmp.py click X Y [right]       click at display pixel X,Y
  qmp.py move X Y                move the pointer
  qmp.py scroll X Y N            wheel N notches (negative = up) at X,Y
  qmp.py raw '{"execute": ...}'  any QMP command

Coordinates are pixels of the VM display (VM_W x VM_H, default 1920x1080).
"""
import json
import os
import socket
import subprocess
import sys
import tempfile
import time

VM_DIR = os.environ.get("VM_DIR", os.path.expanduser("~/VMs/win11"))
W = int(os.environ.get("VM_W", "1920"))
H = int(os.environ.get("VM_H", "1080"))

SHIFTED = {
    **{c: c.lower() for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ"},
    "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6", "&": "7",
    "*": "8", "(": "9", ")": "0", "_": "minus", "+": "equal", "{": "bracket_left",
    "}": "bracket_right", "|": "backslash", ":": "semicolon", '"': "apostrophe",
    "<": "comma", ">": "dot", "?": "slash", "~": "grave_accent",
}
PLAIN = {
    " ": "spc", "-": "minus", "=": "equal", "[": "bracket_left", "]": "bracket_right",
    "\\": "backslash", ";": "semicolon", "'": "apostrophe", ",": "comma", ".": "dot",
    "/": "slash", "`": "grave_accent", "\n": "ret", "\t": "tab",
}


class QMP:
    def __init__(self):
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(os.path.join(VM_DIR, "qmp.sock"))
        self.f = self.s.makefile("rw")
        self.f.readline()  # greeting
        self.cmd("qmp_capabilities")

    def cmd(self, name, **args):
        msg = {"execute": name}
        if args:
            msg["arguments"] = args
        self.f.write(json.dumps(msg) + "\n")
        self.f.flush()
        while True:
            reply = json.loads(self.f.readline())
            if "event" in reply:
                continue
            if "error" in reply:
                raise SystemExit(f"QMP {name}: {reply['error']}")
            return reply.get("return")

    def keys(self, combo):
        """One chord like 'ctrl-alt-delete', pressed together then released."""
        names = combo.split("-") if combo != "minus" else ["minus"]
        keys = [{"type": "qcode", "data": k} for k in names]
        self.cmd("send-key", keys=keys, **{"hold-time": 60})

    def type(self, text):
        for ch in text:
            if ch in SHIFTED:
                self.keys(f"shift-{SHIFTED[ch]}")
            elif ch in PLAIN:
                self.keys(PLAIN[ch])
            elif ch.isalnum():
                self.keys(ch.lower())
            else:
                raise SystemExit(f"cannot type {ch!r}")
            time.sleep(0.02)

    def _abs(self, x, y):
        return [
            {"type": "abs", "data": {"axis": "x", "value": int(x * 32767 / (W - 1))}},
            {"type": "abs", "data": {"axis": "y", "value": int(y * 32767 / (H - 1))}},
        ]

    def move(self, x, y):
        self.cmd("input-send-event", events=self._abs(x, y))

    def click(self, x, y, button="left"):
        self.move(x, y)
        # Let the island's pointer watcher (16 ms) take the pointer first.
        time.sleep(0.2)
        for down in (True, False):
            self.cmd("input-send-event", events=[{"type": "btn", "data": {"button": button, "down": down}}])
            time.sleep(0.06)

    def scroll(self, x, y, notches):
        self.move(x, y)
        button = "wheel-up" if notches < 0 else "wheel-down"
        for _ in range(abs(notches)):
            for down in (True, False):
                self.cmd("input-send-event", events=[{"type": "btn", "data": {"button": button, "down": down}}])
            time.sleep(0.03)

    def shot(self, out):
        with tempfile.TemporaryDirectory() as tmp:
            ppm = os.path.join(tmp, "s.ppm")
            self.cmd("screendump", filename=ppm, format="png" if out.endswith(".png") else "ppm")
            # QEMU 7.1+ writes PNG directly when asked; fall back to sips for PPM.
            if os.path.getsize(ppm) and open(ppm, "rb").read(4) == b"\x89PNG":
                os.replace(ppm, out)
            else:
                subprocess.run(["sips", "-s", "format", "png", ppm, "--out", out], check=True, capture_output=True)
        print(out)


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    q = QMP()
    op, rest = sys.argv[1], sys.argv[2:]
    if op == "shot":
        q.shot(os.path.abspath(rest[0] if rest else "vm.png"))
    elif op == "keys":
        for combo in rest:
            q.keys(combo)
            time.sleep(0.08)
    elif op == "type":
        q.type(" ".join(rest))
    elif op == "click":
        q.click(int(rest[0]), int(rest[1]), rest[2] if len(rest) > 2 else "left")
    elif op == "move":
        q.move(int(rest[0]), int(rest[1]))
    elif op == "scroll":
        q.scroll(int(rest[0]), int(rest[1]), int(rest[2]))
    elif op == "raw":
        msg = json.loads(rest[0])
        print(json.dumps(q.cmd(msg["execute"], **msg.get("arguments", {})), indent=2))
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
