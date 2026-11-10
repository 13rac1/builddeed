#!/usr/bin/env bash
# Builds and tests the license contract in Podman.
#
#   scripts/contract.sh build   # writes contract/target/.../builddeed_license.wasm and prints its SHA-256
#   scripts/contract.sh test    # unit tests, then the near-sandbox test
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
WASM=contract/target/wasm32-unknown-unknown/release/builddeed_license.wasm

# --cfg near makes near-sdk use the NEAR host functions, so that a failed
# call returns its error message (without it, only "unreachable").
# --remap-path-prefix removes the build paths from the WASM, so that the
# result does not depend on where the repository is.
build() {
  "$HERE/podman-run.sh" bash -c '
    set -euo pipefail
    export RUSTFLAGS="--cfg near --remap-path-prefix=/src=. --remap-path-prefix=$CARGO_HOME=/cargo"
    cargo build --locked --release --target wasm32-unknown-unknown --lib \
      --manifest-path contract/Cargo.toml
    sha256sum '"$WASM"'
  '
}

unit() {
  "$HERE/podman-run.sh" cargo test --locked --lib --manifest-path contract/Cargo.toml
}

sandbox() {
  "$HERE/podman-run.sh" env BUILDDEED_WASM=/src/$WASM \
    cargo test --locked --test sandbox --manifest-path contract/Cargo.toml -- --nocapture
}

case "${1:-}" in
  build) build ;;
  test) unit; build; sandbox ;;
  *) echo "usage: $0 build|test" >&2; exit 2 ;;
esac
