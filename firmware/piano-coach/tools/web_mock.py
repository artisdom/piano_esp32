#!/usr/bin/env python3
"""Serve the headless boards' web page (web/index.html) with a mock of the
board's JSON API, for screenshots and page development on the PC:

    tools/web_mock.py [--port 8765] [--library ~/w/piano/midi-piano/assets/midi]

/api/dir lists the real library folders; /api/state shows Learn on 小星星.
"""
import argparse, json, os, re, urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ap = argparse.ArgumentParser()
ap.add_argument("--port", type=int, default=8765)
ap.add_argument("--library", default=os.path.expanduser("~/w/piano/midi-piano/assets/midi"))
args = ap.parse_args()
here = os.path.dirname(os.path.abspath(__file__))
page = open(os.path.join(here, "..", "web", "index.html"), "rb").read()

root = args.library
while True:  # start inside a single top folder, like the firmware
    entries = [e for e in os.listdir(root) if not e.startswith(".")]
    if len(entries) == 1 and os.path.isdir(os.path.join(root, entries[0])):
        root = os.path.join(root, entries[0])
    else:
        break

def natural(s):
    return [int(t) if t.isdigit() else t.lower() for t in re.split(r"(\d+)", s)]

def entry(name, is_dir):
    if is_dir:
        m = re.match(r"^(.*?)（(\d+)）$", name)
        title = m.group(1) if m else name
        title = re.sub(r"^小项.：", "", title)
        return {"name": name, "title": title, "sub": f"{m.group(2)} songs" if m else ""}
    stem = os.path.splitext(name)[0]
    t, _, a = stem.rpartition(" - ")
    return {"name": name, "title": t or stem, "sub": a if t else ""}

STATE = {
    "piano": {"connected": True, "name": "GZUT-MIDI"}, "sd": True,
    "song": {"title": "小星星", "artist": "作者不详", "folder": "儿时回忆", "path": "儿时回忆（41）/小星星 - 作者不详.mid"},
    "error": None, "state": "waiting", "pos": 21000, "dur": 99000, "posText": "0:21", "durText": "1:39",
    "mode": "learn", "hands": "both", "tempo": 90, "volume": 60, "sound": 0,
    "stats": {"acc": 94, "done": 31, "total": 147, "hits": 0, "streak": 12}, "prompt": "G4 C5",
    "playlist": {"dir": "儿时回忆（41）", "index": 26, "count": 41}, "indexed": 2449,
}

class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def send(self, body, ctype="application/json; charset=utf-8"):
        b = body if isinstance(body, bytes) else json.dumps(body, ensure_ascii=False).encode()
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        q = urllib.parse.parse_qs(u.query)
        if u.path == "/":
            return self.send(page, "text/html; charset=utf-8")
        if u.path == "/api/state":
            return self.send(STATE)
        if u.path == "/api/dir":
            rel = q.get("p", [""])[0]
            d = os.path.join(root, rel)
            names = sorted(os.listdir(d), key=natural)
            dirs = [entry(n, True) for n in names if os.path.isdir(os.path.join(d, n))]
            songs = [entry(n, False) for n in names if n.lower().endswith((".mid", ".midi"))]
            title = entry(os.path.basename(rel), True)["title"] if rel else "Library"
            return self.send({"path": rel, "title": title, "dirs": dirs, "songs": songs})
        if u.path in ("/api/search", "/api/play", "/api/cmd"):
            return self.send({"ok": True, "ready": True, "results": []})
        self.send_response(404)
        self.end_headers()

print(f"web mock on http://127.0.0.1:{args.port}/ (library {root})")
ThreadingHTTPServer(("127.0.0.1", args.port), H).serve_forever()
