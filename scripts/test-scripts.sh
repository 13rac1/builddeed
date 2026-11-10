#!/usr/bin/env bash
# Tests of the release scripts that need no network and no build.
#
#   scripts/test-scripts.sh
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

# record-build.sh maps each release file name to its platform.
cat > "$T/SHA256SUMS" <<'EOF'
1111111111111111111111111111111111111111111111111111111111111111  builddeed-checksum-0.1.1-linux-x86_64
2222222222222222222222222222222222222222222222222222222222222222  builddeed-checksum-0.1.1-linux-x86_64.tar.gz
3333333333333333333333333333333333333333333333333333333333333333  builddeed-checksum_0.1.1_amd64.deb
4444444444444444444444444444444444444444444444444444444444444444  builddeed-checksum-0.1.1-windows-x86_64.exe
5555555555555555555555555555555555555555555555555555555555555555  builddeed-checksum-0.1.1-windows-x86_64-setup.exe
6666666666666666666666666666666666666666666666666666666666666666  builddeed-checksum-0.1.1-macos-aarch64
7777777777777777777777777777777777777777777777777777777777777777  builddeed-checksum-0.1.1-macos-aarch64.dmg
8888888888888888888888888888888888888888888888888888888888888888  builddeed-checksum-0.1.1-macos-aarch64.zip
EOF
OUT=$(SUMS="$T/SHA256SUMS" DRY_RUN=1 "$ROOT/scripts/record-build.sh" 0.1.1 "$(printf 'a%.0s' {1..40})" https://example.com/a)
python3 -I - "$OUT" <<'PY' || fail "record-build.sh platform mapping"
import json, sys
a = json.loads(sys.argv[1])
got = {x["file"]: x["platform"] for x in a["artifacts"]}
want = {
    "builddeed-checksum-0.1.1-linux-x86_64": "linux-x86_64",
    "builddeed-checksum-0.1.1-linux-x86_64.tar.gz": "linux-x86_64",
    "builddeed-checksum_0.1.1_amd64.deb": "linux-x86_64",
    "builddeed-checksum-0.1.1-windows-x86_64.exe": "windows-x86_64",
    "builddeed-checksum-0.1.1-windows-x86_64-setup.exe": "windows-x86_64",
    "builddeed-checksum-0.1.1-macos-aarch64": "macos-aarch64",
    "builddeed-checksum-0.1.1-macos-aarch64.dmg": "macos-aarch64",
    "builddeed-checksum-0.1.1-macos-aarch64.zip": "macos-aarch64",
}
assert got == want, got
assert a["version"] == "0.1.1" and all(x["attestation_url"] == "https://example.com/a" for x in a["artifacts"])
PY

# An unknown file name stops the record.
echo "9999999999999999999999999999999999999999999999999999999999999999  notes.txt" >> "$T/SHA256SUMS"
if SUMS="$T/SHA256SUMS" DRY_RUN=1 "$ROOT/scripts/record-build.sh" 0.1.1 x y >/dev/null 2>&1; then
  fail "record-build.sh accepted an unknown file"
fi

# The app and the site use the same register link and contract methods.
grep -q 'register.html#device={device}&version={VERSION}' "$ROOT/app/src/main.rs" || fail "app register link"
for p in device version; do
  grep -q "params.get(\"$p\")" "$ROOT/site/src/pages/register.js" || fail "site reads no $p"
done
for m in add_device remove_device device_license; do
  grep -q "pub fn $m(" "$ROOT/contract/src/lib.rs" || fail "contract has no $m"
done
grep -q '"add_device"' "$ROOT/site/src/pages/register.js" || fail "site does not call add_device"
grep -q '"remove_device"' "$ROOT/site/src/pages/licenses.js" || fail "site does not call remove_device"
grep -q '"device_license"' "$ROOT/app/src/near.rs" || fail "app does not call device_license"

# The contract account and RPC host are the same everywhere.
for f in app/src/main.rs site/src/near.js scripts/verify.sh scripts/record-build.sh; do
  grep -q 'demo.builddeed.testnet' "$ROOT/$f" || fail "$f does not name demo.builddeed.testnet"
  grep -q 'rpc.testnet.fastnear.com' "$ROOT/$f" || fail "$f does not use rpc.testnet.fastnear.com"
done

echo "script tests: ok"
