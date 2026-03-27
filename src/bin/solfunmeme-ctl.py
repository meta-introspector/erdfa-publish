#!/usr/bin/env python3
"""Unified SOLFUNMEME deployment controller — status, deploy, test across all platforms."""
import json, os, sys, subprocess, time, urllib.request, hashlib
from pathlib import Path

HOME = os.path.expanduser("~")
DIOXUS = "/mnt/data1/meta-introspector/submodules/solfunmeme-dioxus"
DOCS = f"{DIOXUS}/docs"
MESH_API = "http://127.0.0.1:7780"

# ── Platform definitions ─────────────────────────────────────────

PLATFORMS = {
    "github-pages": {
        "url": "https://meta-introspector.github.io/solfunmeme-dioxus/",
        "type": "git-push",
        "deploy": lambda: git_push("origin"),
    },
    "cloudflare": {
        "url": "https://solfunmeme-dioxus.pages.dev/",
        "type": "cli",
        "deploy": lambda: wrangler_deploy(),
    },
    "vercel": {
        "url": "https://solfunmeme-dioxus.vercel.app/",
        "type": "git-push",
        "deploy": lambda: git_push("jmikedupont2"),
    },
    "huggingface": {
        "url": "https://introspector-solfunmeme-dioxus.hf.space/",
        "type": "api",
        "deploy": lambda: hf_deploy(),
    },
    "oracle-oci": {
        "url": "https://objectstorage.us-ashburn-1.oraclecloud.com/n/id1iqr236pdp/b/solfunmeme-dioxus/o/index.html",
        "type": "api",
        "deploy": lambda: oci_deploy(),
    },
    "netlify": {
        "url": "https://solfunmeme.netlify.app/",
        "type": "api",
        "deploy": lambda: netlify_deploy(),
    },
    "render": {
        "url": "https://solfunmeme-static.onrender.com/",
        "type": "git-push",
        "deploy": lambda: git_push("origin"),  # auto-deploys from GitHub
    },
    "self-hosted": {
        "url": "http://192.168.68.62/dioxus/",
        "type": "systemd",
        "deploy": lambda: run("systemctl --user restart solfunmeme-dioxus"),
    },
    "supabase": {
        "url": "https://aesruozmcbvtutpoyaze.supabase.co",
        "type": "api",
        "deploy": lambda: print("  Supabase: always-on DB backend"),
    },
}

# ── Helpers ───────────────────────────────────────────────────────

def run(cmd, cwd=None):
    r = subprocess.run(cmd, shell=True, capture_output=True, text=True, cwd=cwd, timeout=300)
    return r.returncode == 0, r.stdout.strip()

def http_check(url, timeout=10):
    try:
        req = urllib.request.Request(url)
        resp = urllib.request.urlopen(req, timeout=timeout)
        return resp.status, len(resp.read(1024))
    except urllib.error.HTTPError as e:
        return e.code, 0
    except Exception as e:
        return 0, str(e)[:60]

def read_token(path):
    try: return Path(os.path.expanduser(path)).read_text().strip()
    except: return ""

# ── Deploy functions ──────────────────────────────────────────────

def git_push(remote):
    ok, out = run(f"git add docs/ && git commit -m 'deploy' --allow-empty && git push {remote} HEAD:main", cwd=DIOXUS)
    return ok

def wrangler_deploy():
    token = read_token("~/.cloudflare-pages")
    ok, out = run(
        f'CLOUDFLARE_API_TOKEN={token} CLOUDFLARE_ACCOUNT_ID=0ceffbadd0a04623896f5317a1e40d94 '
        f'nix-shell -p nodejs_22 --run "npx wrangler pages deploy docs/ --project-name=solfunmeme-dioxus --branch=main"',
        cwd=DIOXUS)
    return ok

def netlify_deploy():
    token = read_token("~/.netlify")
    data = open(f"{DOCS}/../docs.zip", "rb") if os.path.exists(f"{DOCS}/../docs.zip") else None
    if not data:
        run(f"cd {DOCS} && zip -qr ../docs.zip .")
        data = open(f"{DIOXUS}/docs.zip", "rb")
    req = urllib.request.Request(
        "https://api.netlify.com/api/v1/sites/5f5c0101-b40a-4c7d-be82-9ee44ee4a1c6/deploys",
        data=data.read(), headers={"Authorization": f"Bearer {token}", "Content-Type": "application/zip"})
    try:
        resp = urllib.request.urlopen(req, timeout=30)
        return resp.status in (200, 201)
    except: return False

def hf_deploy():
    try:
        from huggingface_hub import HfApi
        HfApi().upload_folder(folder_path=DOCS, repo_id="introspector/solfunmeme-dioxus",
                              repo_type="space", commit_message="Deploy from controller")
        return True
    except: return False

def oci_deploy():
    try:
        import oci, mimetypes
        config = oci.config.from_file("~/.solfunmeme-keys/oci_config")
        c = oci.object_storage.ObjectStorageClient(config)
        ns = c.get_namespace().data
        for root, _, files in os.walk(DOCS):
            for f in files:
                p = os.path.join(root, f)
                ct = "application/wasm" if f.endswith(".wasm") else (mimetypes.guess_type(p)[0] or "application/octet-stream")
                if f.endswith(".js"): ct = "application/javascript"
                c.put_object(ns, "solfunmeme-dioxus", os.path.relpath(p, DOCS), open(p, "rb"), content_type=ct)
        return True
    except: return False

# ── Headless browser test ─────────────────────────────────────────

def headless_test(url, port=9270):
    """Test a URL via CDP, return (rendered, main_length, errors)."""
    try:
        import websocket
        tabs = json.loads(urllib.request.urlopen(f"http://127.0.0.1:{port}/json").read())
        tabs = [t for t in tabs if t.get("type") == "page"]
        ws = websocket.create_connection(tabs[0]["webSocketDebuggerUrl"], timeout=10)
        for i, m in enumerate(["Runtime.enable", "Log.enable", "Page.enable"], 1):
            ws.send(json.dumps({"id": i, "method": m})); ws.recv()
        ws.send(json.dumps({"id": 10, "method": "Page.navigate", "params": {"url": url}}))
        errors = []
        end = time.time() + 35
        ws.settimeout(3)
        while time.time() < end:
            try:
                r = json.loads(ws.recv())
                m = r.get("method", "")
                if m == "Runtime.exceptionThrown":
                    errors.append(r["params"]["exceptionDetails"].get("text", "")[:100])
                elif m == "Log.entryAdded" and r["params"]["entry"]["level"] == "error":
                    errors.append(r["params"]["entry"].get("text", "")[:100])
            except: pass
        ws.settimeout(10)
        ws.send(json.dumps({"id": 99, "method": "Runtime.evaluate", "params": {
            "expression": "document.getElementById('main')?document.getElementById('main').innerHTML.length:-1"}}))
        ml = -1
        try:
            while True:
                r = json.loads(ws.recv())
                if r.get("id") == 99: ml = r["result"]["result"].get("value", -1); break
        except: pass
        ws.close()
        return ml > 100, ml, errors[:3]
    except Exception as e:
        return False, -1, [str(e)[:80]]

# ── Commands ──────────────────────────────────────────────────────

def cmd_status():
    print("SOLFUNMEME Deployment Status")
    print("=" * 70)
    for name, p in PLATFORMS.items():
        code, size = http_check(p["url"])
        status = "✅" if code in (200, 301, 302) else f"❌ {code}"
        print(f"  {status} {name:20s} {p['type']:10s} {code:3d} {p['url'][:60]}")
    # Service status
    print("\nServices:")
    for svc in ["solfunmeme-service", "solfunmeme-dioxus", "prometheus", "jaeger", "solfunmeme-mesh-sync.timer"]:
        ok, out = run(f"systemctl --user is-active {svc}")
        s = "✅" if ok else "❌"
        print(f"  {s} {svc}")

def cmd_test(port=9270):
    print("Headless Browser Tests")
    print("=" * 70)
    for name, p in PLATFORMS.items():
        if name == "supabase": continue
        print(f"  {name}...", end=" ", flush=True)
        ok, ml, errs = headless_test(p["url"], port)
        s = "✅" if ok else "❌"
        print(f"{s} main={ml} errors={len(errs)}")
        for e in errs[:1]: print(f"     ⚠ {e[:70]}")

def cmd_deploy(targets=None):
    if targets is None: targets = list(PLATFORMS.keys())
    # Build shards first
    print("Building shards...")
    ok, _ = run(f"bash {DIOXUS}/build.sh")
    if not ok:
        print("❌ Build failed"); return
    print("✅ Build complete\n")
    for name in targets:
        if name not in PLATFORMS: continue
        p = PLATFORMS[name]
        print(f"  Deploying {name}...", end=" ", flush=True)
        try:
            result = p["deploy"]()
            print("✅" if result else "⚠️  check manually")
        except Exception as e:
            print(f"❌ {e}")

def cmd_logs():
    print("Mesh Logs")
    try:
        data = json.loads(urllib.request.urlopen(f"{MESH_API}/mesh/logs", timeout=5).read())
        print(f"  {data['count']} logs stored")
        for log in data.get("logs", [])[:5]:
            print(f"  - {log.get('tags',[])} {json.dumps(log.get('fields',{}))[:80]}")
    except Exception as e:
        print(f"  ❌ {e}")

def cmd_peers():
    print("Mesh Peers")
    try:
        data = json.loads(urllib.request.urlopen(f"{MESH_API}/mesh/peers", timeout=5).read())
        print(f"  This node: {data['node']} ({data['address']}) pubkey={data['pubkey'][:20]}...")
        for p in data.get("peers", []):
            print(f"  - {p['node']:15s} {p['address']:15s} {p.get('endpoint','')}")
    except Exception as e:
        print(f"  ❌ {e}")

# ── Main ──────────────────────────────────────────────────────────

COMMANDS = {
    "status": cmd_status,
    "test": lambda: cmd_test(),
    "deploy": lambda: cmd_deploy(),
    "deploy-all": lambda: cmd_deploy(),
    "logs": cmd_logs,
    "peers": cmd_peers,
}

if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "status"
    if cmd in ("deploy",) and len(sys.argv) > 2:
        cmd_deploy(sys.argv[2:])
    elif cmd in COMMANDS:
        COMMANDS[cmd]()
    else:
        print(f"Usage: {sys.argv[0]} [status|test|deploy|deploy-all|logs|peers]")
        print(f"       {sys.argv[0]} deploy cloudflare netlify  # deploy to specific platforms")
