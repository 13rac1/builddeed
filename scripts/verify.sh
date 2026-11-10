#!/usr/bin/env bash
# Rebuilds an official release from its recorded commit and compares the
# Linux and Windows files with the SHA-256 values in the license contract.
#
#   scripts/verify.sh <version>      # for example: scripts/verify.sh 0.1.1
#
# Needs git, curl, python3 and Podman. The macOS files are not rebuilt:
# they have a provenance attestation only.
#
# Safety: the commit comes from the contract, so this script does not trust
# it. The tag v<version> of this repository must point to the recorded
# commit. The build runs with the scripts of this checkout; the recorded
# commit only gives the source tree, which builds in the container.
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
COMMIT=$(python3 -I -c 'import json,sys; c=json.loads(sys.argv[1])["commit"]; assert len(c)==40 and all(x in "0123456789abcdef" for x in c); print(c)' "$RECORD")
echo "Build $VERSION: commit $COMMIT"

git -C "$ROOT" fetch -q --tags origin
TAGGED=$(git -C "$ROOT" rev-parse -q --verify "refs/tags/v$VERSION^{commit}") \
  || { echo "This repository has no tag v$VERSION" >&2; exit 1; }
[ "$TAGGED" = "$COMMIT" ] \
  || { echo "Tag v$VERSION is $TAGGED, but the contract records $COMMIT" >&2; exit 1; }

W=$(mktemp -d)
trap 'git -C "$ROOT" worktree remove --force "$W/src" >/dev/null 2>&1 || true; rm -rf "$W"' EXIT
git -C "$ROOT" worktree add -q --detach "$W/src" "$COMMIT"
export SRC="$W/src"
export SOURCE_DATE_EPOCH=$(git -C "$SRC" log -1 --format=%ct)
"$ROOT/scripts/app.sh" linux >/dev/null
"$ROOT/scripts/app.sh" windows >/dev/null
"$ROOT/scripts/package.sh" >/dev/null

python3 -I - "$RECORD" "$SRC/dist/SHA256SUMS" <<'PY'
import json, sys
record = json.loads(sys.argv[1])
built = {}
for line in open(sys.argv[2]):
    sha, name = line.split()
    built[name.lstrip("*")] = sha
compared = bad = 0
for a in record["artifacts"]:
    if a["platform"].startswith("macos"):
        print(f"SKIP   {a['file']} (macOS: attestation only)")
        continue
    got = built.get(a["file"])
    ok = got == a["sha256"]
    compared += 1
    bad += not ok
    print(f"{'SAME ' if ok else 'DIFF '}  {a['file']}\n       contract {a['sha256']}\n       rebuilt  {got}")
if compared == 0:
    sys.exit("No Linux or Windows file was compared.")
if bad:
    sys.exit(f"{bad} of {compared} files are different.")
print(f"All {compared} Linux and Windows files are the same as the official build.")
PY
