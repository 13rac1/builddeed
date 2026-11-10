#!/usr/bin/env bash
# Runs a command in the BuildDeed build image, with the repository at /src.
#
#   scripts/podman-run.sh cargo test --manifest-path contract/Cargo.toml
#
# SRC=<folder> mounts another source tree at /src (verify.sh uses it for the
# recorded commit). The image always comes from this checkout's build/.
#
# The cargo registry cache is the Podman volume "builddeed-cargo". Build
# output goes to target/ in each crate of the repository.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
SRC=${SRC:-$ROOT}
IMAGE=localhost/builddeed-build:1

if ! podman image exists "$IMAGE"; then
  podman build -t "$IMAGE" -f "$ROOT/build/Containerfile" "$ROOT/build"
fi

# The NEAR sandbox binary goes in the cache volume too.
exec podman run --rm \
  -v "$SRC:/src:Z" \
  -v builddeed-cargo:/cache \
  -e NEAR_SANDBOX_HOME=/cache/near-sandbox \
  -e SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-}" \
  -e BD_TEST_ACCOUNT="${BD_TEST_ACCOUNT:-}" \
  -w /src \
  "$IMAGE" "$@"
