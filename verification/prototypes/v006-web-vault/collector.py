#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Serves the V-006 probe and accepts reports at POST /report?t=<token>&label=<label>.

Reports land in results/<label>--<host>.json. The token (printed at startup) gates writes so a public tunnel cannot be used to
fill the disk; bodies are capped at 256 KiB and labels are restricted to [a-z0-9-]. Static, local-use test tooling only.
"""
import http.server, json, os, re, secrets, sys, urllib.parse

ROOT = os.path.dirname(os.path.abspath(__file__))
TOKEN = os.environ.get("V006_TOKEN") or secrets.token_urlsafe(9)
PORT = int(os.environ.get("PORT", "8806"))

class H(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **k): super().__init__(*a, directory=ROOT, **k)
    def do_POST(self):
        u = urllib.parse.urlparse(self.path); q = urllib.parse.parse_qs(u.query)
        n = int(self.headers.get("Content-Length") or 0)
        label = re.sub(r"[^a-z0-9-]", "", (q.get("label", ["x"])[0]).lower())[:32] or "x"
        host = re.sub(r"[^a-z0-9.-]", "", (self.headers.get("Host") or "h").split(":")[0].lower())[:80]
        if u.path != "/report" or n > 262144 or not secrets.compare_digest(q.get("t", [""])[0], TOKEN):
            self.send_error(403); return
        try: body = json.loads(self.rfile.read(n))
        except ValueError: self.send_error(400); return
        out = os.path.join(ROOT, "results", f"{label}--{host}.json")
        with open(out, "w") as f: json.dump(body, f, indent=2)
        self.send_response(204); self.end_headers(); print("saved", out, flush=True)

if __name__ == "__main__":
    print(f"token: {TOKEN}\nopen: /?t={TOKEN}", flush=True)
    http.server.ThreadingHTTPServer(("127.0.0.1", PORT), H).serve_forever()
