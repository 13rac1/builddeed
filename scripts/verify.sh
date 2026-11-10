#!/usr/bin/env bash
# Rebuilds an official release from its recorded commit and compares the
# Linux and Windows files with the SHA-256 values in the license contract.
#
#   scripts/verify.sh <version>      # for example: scripts/verify.sh 0.1.0
#
# Needs git, curl, python3 and Podman. The macOS files are not rebuilt:
# they have a provenance attestation only.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERSION=${1:?usage: scripts/verify.sh <version>}
CONTRACT=${CONTRACT:-demo.builddeed.testnet}
RPC=${RPC:-https://rpc.testnet.fastnear.com}

ARGS=$(printf '{"version":"%s"}' "$VERSION" | base64 | tr -d '\n')
RECORD=$(curl -sS -H 'Content-Type: application/json' "$RPC" -d \
  "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"query\",\"params\":{\"request_type\":\"call_function\",\"finality\":\"final\",\"account_id\":\"$CONTRACT\",\"method_name\":\"get_build\",\"args_base64\":\"$ARGS\"}}" \
  | python3 -I -c 'import json,sys; r=json.load(sys.stdin)["result"]["result"]; print(bytes(r).decode())')
[ "$RECORD" != "null" ] || { echo "The contract has no build $VERSION" >&2; exit 1; }
COMMIT=$(python3 -I -c 'import json,sys; print(json.loads(sys.argv[1])["commit"])' "$RECORD")
echo "Build $VERSION: commit $COMMIT"

W=$(mktemp -d)
trap 'git -C "$ROOT" worktree remove --force "$W/src" >/dev/null 2>&1 || true; rm -rf "$W"' EXIT
git -C "$ROOT" fetch -q origin "$COMMIT" 2>/dev/null || true
git -C "$ROOT" worktree add -q --detach "$W/src" "$COMMIT"
"$W/src/scripts/app.sh" linux >/dev/null
"$W/src/scripts/app.sh" windows >/dev/null
"$W/src/scripts/package.sh" >/dev/null

python3 -I - "$RECORD" "$W/src/dist/SHA256SUMS" <<'EOF'
import json, sys
record = json.loads(sys.argv[1])
built = {}
for line in open(sys.argv[2]):
    sha, name = line.split()
    built[name.lstrip("*")] = sha
bad = 0
for a in record["artifacts"]:
    if a["platform"].startswith("macos"):
        print(f"SKIP   {a['file']} (macOS: attestation only)")
        continue
    got = built.get(a["file"])
    ok = got == a["sha256"]
    bad += not ok
    print(f"{'SAME ' if ok else 'DIFF '}  {a['file']}\n       contract {a['sha256']}\n       rebuilt  {got}")
sys.exit(1 if bad else 0)
EOF
echo "All Linux and Windows files are the same as the official build."
