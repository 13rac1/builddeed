#!/usr/bin/env bash
# Builds the demo app in Podman.
#
#   scripts/app.sh linux     # app/target/x86_64-unknown-linux-gnu/release/builddeed-checksum
#   scripts/app.sh windows   # app/target/x86_64-pc-windows-gnu/release/builddeed-checksum.exe
#   scripts/app.sh test
#
# SOURCE_DATE_EPOCH is the commit time, so that a rebuild of the same
# commit gives the same bytes.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/.." && pwd)
export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --format=%ct)}

build() {
  "$HERE/podman-run.sh" bash -c '
    set -euo pipefail
    export RUSTFLAGS="--remap-path-prefix=/src=. --remap-path-prefix=$CARGO_HOME=/cargo -C link-arg=-Wl,--build-id=none"
    if [ "$1" = x86_64-pc-windows-gnu ]; then
      export RUSTFLAGS="--remap-path-prefix=/src=. --remap-path-prefix=$CARGO_HOME=/cargo -C link-arg=-Wl,--no-insert-timestamp"
    fi
    cargo build --locked --release --target "$1" --manifest-path app/Cargo.toml
    sha256sum app/target/$1/release/builddeed-checksum*
  ' _ "$1"
}

case "${1:-}" in
  linux) build x86_64-unknown-linux-gnu ;;
  windows) build x86_64-pc-windows-gnu ;;
  test) "$HERE/podman-run.sh" cargo test --locked --manifest-path app/Cargo.toml ;;
  *) echo "usage: $0 linux|windows|test" >&2; exit 2 ;;
esac
