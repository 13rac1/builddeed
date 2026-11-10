#!/usr/bin/env bash
# Runs every check: format, lints and tests of the contract and the app, and
# the script tests. CI and the release workflow run it before a build.
#
#   scripts/check.sh
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)

"$HERE/test-scripts.sh"
for crate in contract app; do
  "$HERE/podman-run.sh" bash -c "
    set -euo pipefail
    cargo fmt --manifest-path $crate/Cargo.toml --check
    cargo clippy --locked --manifest-path $crate/Cargo.toml --all-targets -- -D warnings
  "
done
"$HERE/app.sh" test
"$HERE/contract.sh" test
echo "All checks passed."
