#!/usr/bin/env bash
# Records the files in dist/ as an official build in the license contract.
#
#   scripts/record-build.sh <version> <commit> <attestation-url> <key-file>
#
# <key-file> is a NEAR key file ({"account_id", "public_key", "private_key"})
# for a function-call key of the developer account that can call only
# add_build. The script needs the near CLI (near-cli-rs) on PATH, and python3.
# A file that the contract already has is skipped, so a second run is safe.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERSION=$1 COMMIT=$2 ATTESTATION=$3 KEY_FILE=$4
CONTRACT=${CONTRACT:-demo.builddeed.testnet}
NETWORK=${NETWORK:-testnet}
RPC=${RPC:-https://rpc.testnet.fastnear.com}
DEVELOPER=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["account_id"])' "$KEY_FILE")

ARGS=$(python3 -I - "$ROOT/dist/SHA256SUMS" "$VERSION" "$COMMIT" "$ATTESTATION" "$CONTRACT" "$RPC" <<'PY'
import base64, json, sys, urllib.request
sums, version, commit, attestation, contract, rpc = sys.argv[1:7]

def recorded(sha):
    args = base64.b64encode(json.dumps({"sha256": sha}).encode()).decode()
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "query", "params": {
        "request_type": "call_function", "finality": "final", "account_id": contract,
        "method_name": "find_artifact", "args_base64": args}}).encode()
    req = urllib.request.Request(rpc, body, {"Content-Type": "application/json"})
    r = json.load(urllib.request.urlopen(req, timeout=20))["result"]["result"]
    return json.loads(bytes(r)) is not None

def platform(name):
    for key, plat in (("linux", "linux-x86_64"), ("_amd64.deb", "linux-x86_64"),
                      ("windows", "windows-x86_64"), ("macos", "macos-aarch64")):
        if key in name:
            return plat
    raise SystemExit(f"unknown platform for {name}")

artifacts = []
for line in open(sums):
    sha, name = line.split()
    name = name.lstrip("*")
    if recorded(sha):
        print(f"already recorded: {name}", file=sys.stderr)
        continue
    artifacts.append({"platform": platform(name), "file": name, "sha256": sha,
                      "attestation_url": attestation or None})
print(json.dumps({"version": version, "commit": commit, "artifacts": artifacts}) if artifacts else "")
PY
)
[ -n "$ARGS" ] || { echo "Every file is already recorded."; exit 0; }

near contract call-function as-transaction "$CONTRACT" add_build json-args "$ARGS" \
  prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
  sign-as "$DEVELOPER" network-config "$NETWORK" \
  sign-with-access-key-file "$KEY_FILE" send
