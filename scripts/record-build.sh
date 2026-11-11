#!/usr/bin/env bash
# Records the files in dist/ as an official build in the license contract,
# then reads the record back and checks each file.
#
#   scripts/record-build.sh <version> <commit> <attestation-url> <key-file>
#   DRY_RUN=1 scripts/record-build.sh <version> <commit> <attestation-url>
#
# <key-file> is a NEAR key file ({"account_id", "public_key", "private_key"})
# for a function-call key of the product's developer account. The key's
# receiver is the license contract, and its only method is add_build. The
# storage of the record comes from the product's storage balance. The script needs the near CLI (near-cli-rs) on PATH, and python3.
# A file that the contract already has is skipped, so a second run is safe.
# DRY_RUN=1 prints the add_build arguments and sends nothing.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERSION=$1 COMMIT=$2 ATTESTATION=$3 KEY_FILE=${4:-}
CONTRACT=${CONTRACT:-licenses.builddeed.testnet}
PRODUCT=${PRODUCT:-checksum}
NETWORK=${NETWORK:-testnet}
RPC=${RPC:-https://rpc.testnet.fastnear.com}
# Where the release workflow publishes the files: the record gets each file's URL.
RELEASE_URL=${RELEASE_URL:-https://github.com/13rac1/builddeed/releases/download/v$VERSION}
SUMS=${SUMS:-$ROOT/dist/SHA256SUMS}
DRY_RUN=${DRY_RUN:-}

# Prints the add_build arguments, or nothing when every file is recorded.
build_args() {
  python3 -I - "$SUMS" "$VERSION" "$COMMIT" "$ATTESTATION" "$CONTRACT" "$RPC" "$DRY_RUN" "$PRODUCT" "$RELEASE_URL" <<'PY'
import base64, json, sys, urllib.request
sums, version, commit, attestation, contract, rpc, dry_run, product, release_url = sys.argv[1:10]

def recorded(sha):
    if dry_run:
        return False
    args = base64.b64encode(json.dumps({"product": product, "sha256": sha}).encode()).decode()
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "query", "params": {
        "request_type": "call_function", "finality": "final", "account_id": contract,
        "method_name": "find_artifact", "args_base64": args}}).encode()
    req = urllib.request.Request(rpc, body, {"Content-Type": "application/json"})
    r = json.load(urllib.request.urlopen(req, timeout=20))["result"]["result"]
    return json.loads(bytes(r)) is not None

# The file names that scripts/package.sh and scripts/macos.sh write.
def platform(name):
    if "-linux-x86_64" in name or name.endswith("_amd64.deb"):
        return "linux-x86_64"
    if "-windows-x86_64" in name:
        return "windows-x86_64"
    if "-macos-aarch64" in name:
        return "macos-aarch64"
    raise SystemExit(f"unknown platform for {name}")

artifacts = []
for line in open(sums):
    sha, name = line.split()
    name = name.lstrip("*")
    if recorded(sha):
        print(f"already recorded: {name}", file=sys.stderr)
        continue
    artifacts.append({"platform": platform(name), "file": name, "sha256": sha,
                      "attestation_url": attestation or None,
                      "url": f"{release_url}/{name}"})
print(json.dumps({"product": product, "version": version, "commit": commit, "artifacts": artifacts}) if artifacts else "")
PY
}

ARGS=$(build_args)
if [ -n "$DRY_RUN" ]; then
  printf '%s\n' "$ARGS"
  exit 0
fi
if [ -n "$ARGS" ]; then
  DEVELOPER=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["account_id"])' "$KEY_FILE")
  near contract call-function as-transaction "$CONTRACT" add_build json-args "$ARGS" \
    prepaid-gas '100.0 Tgas' attached-deposit '0 NEAR' \
    sign-as "$DEVELOPER" network-config "$NETWORK" \
    sign-with-access-key-file "$KEY_FILE" send
else
  echo "Every file is already recorded."
fi

# Read back: every file in SHA256SUMS must be in the record of this version.
python3 -I - "$SUMS" "$VERSION" "$COMMIT" "$CONTRACT" "$RPC" "$PRODUCT" <<'PY'
import base64, json, sys, urllib.request
sums, version, commit, contract, rpc, product = sys.argv[1:7]
bad = 0
for line in open(sums):
    sha, name = line.split()
    args = base64.b64encode(json.dumps({"product": product, "sha256": sha}).encode()).decode()
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "query", "params": {
        "request_type": "call_function", "finality": "final", "account_id": contract,
        "method_name": "find_artifact", "args_base64": args}}).encode()
    req = urllib.request.Request(rpc, body, {"Content-Type": "application/json"})
    m = json.loads(bytes(json.load(urllib.request.urlopen(req, timeout=20))["result"]["result"]))
    ok = m is not None and m["version"] == version and m["commit"] == commit
    bad += not ok
    print(f"{'RECORDED' if ok else 'MISSING '} {name.lstrip('*')}")
sys.exit(1 if bad else 0)
PY
