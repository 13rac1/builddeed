#!/usr/bin/env bash
# Makes the Linux and Windows release files from the built app, in Podman.
# Run scripts/app.sh linux and scripts/app.sh windows first.
#
#   scripts/package.sh    # writes dist/ and dist/SHA256SUMS
#
# Every file time is SOURCE_DATE_EPOCH (the commit time), so that the same
# commit gives the same bytes.
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/.." && pwd)
# SRC=<folder> builds another source tree with this checkout's scripts.
export SRC=${SRC:-$ROOT}
export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git -C "$SRC" log -1 --format=%ct)}

"$HERE/podman-run.sh" bash -c '
set -euo pipefail
V=$(sed -n "s/^version = \"\(.*\)\"/\1/p" app/Cargo.toml | head -1)
N=builddeed-checksum
LIN=app/target/x86_64-unknown-linux-gnu/release/$N
WIN=app/target/x86_64-pc-windows-gnu/release/$N.exe
OUT=dist
rm -rf "$OUT" /tmp/pkg && mkdir -p "$OUT" /tmp/pkg
umask 022
tz() { touch -h -d "@$SOURCE_DATE_EPOCH" "$@"; }

# Linux: the bare program
install -m 755 "$LIN" "$OUT/$N-$V-linux-x86_64"

# Linux: tar.gz
D=/tmp/pkg/$N-$V-linux-x86_64
mkdir -p "$D"
install -m 755 "$LIN" "$D/$N"
cp LICENSE "$D/LICENSE"
find "$D" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$SOURCE_DATE_EPOCH" \
    -C /tmp/pkg -cf - "$N-$V-linux-x86_64" | gzip -n -9 > "$OUT/$N-$V-linux-x86_64.tar.gz"

# Linux: .deb
R=/tmp/pkg/deb
mkdir -p "$R/DEBIAN" "$R/usr/bin" "$R/usr/share/applications" "$R/usr/share/doc/$N"
install -m 755 "$LIN" "$R/usr/bin/$N"
cp LICENSE "$R/usr/share/doc/$N/copyright"
cat > "$R/usr/share/applications/$N.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=BuildDeed Checksum
Comment=File checksums and official build records on NEAR
Exec=$N
Terminal=false
Categories=Utility;
DESKTOP
cat > "$R/DEBIAN/control" <<CONTROL
Package: $N
Version: $V
Architecture: amd64
Maintainer: Hardened Minds <noreply@builddeed.invalid>
Section: utils
Priority: optional
Depends: libc6 (>= 2.36), xdg-utils
Description: File checksum tool sold as shareware with a BuildDeed license
 Shows the SHA-256 of a file and its official build record on NEAR.
 The folder check needs a BuildDeed license.
CONTROL
find "$R" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
dpkg-deb --root-owner-group -Zxz --build "$R" "$OUT/${N}_${V}_amd64.deb" >/dev/null

# Windows: the bare program
install -m 755 "$WIN" "$OUT/$N-$V-windows-x86_64.exe"

# Windows: NSIS installer
cp "$WIN" /tmp/pkg/app.exe
tz /tmp/pkg/app.exe
makensis -V2 -DVERSION="$V" -DEXE=/tmp/pkg/app.exe -DOUT="/src/$OUT/$N-$V-windows-x86_64-setup.exe" build/installer.nsi

cd "$OUT" && sha256sum -- * > SHA256SUMS && cat SHA256SUMS
'
