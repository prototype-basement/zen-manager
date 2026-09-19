#!/usr/bin/env bash
# Builds libusb and libmtp from source for the host architecture, targeting an
# old macOS release, into a private prefix.
#
# Homebrew is not good enough for release builds: its bottles are compiled for
# the macOS version of the machine they were built on, and that minimum travels
# into the .app with the bundled dylibs. Built on a macOS 14 CI runner, the app
# refuses to load on macOS 13 — whatever Info.plist says.
#
# Usage: packaging/macos-deps.sh [prefix]
# Then:  PKG_CONFIG_PATH=<prefix>/lib/pkgconfig packaging/macos-bundle.sh
set -euo pipefail

PREFIX="${1:-$PWD/target/macos-deps}"
LIBUSB_VERSION=1.0.27
LIBMTP_VERSION=1.1.22

# Must match LSMinimumSystemVersion in macos-bundle.sh.
export MACOSX_DEPLOYMENT_TARGET=11.0

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"

curl -fsSL -o libusb.tar.bz2 \
    "https://github.com/libusb/libusb/releases/download/v${LIBUSB_VERSION}/libusb-${LIBUSB_VERSION}.tar.bz2"
curl -fsSL -o libmtp.tar.gz \
    "https://github.com/libmtp/libmtp/releases/download/v${LIBMTP_VERSION}/libmtp-${LIBMTP_VERSION}.tar.gz"

JOBS="$(sysctl -n hw.ncpu)"

tar xjf libusb.tar.bz2
(
    cd "libusb-${LIBUSB_VERSION}"
    ./configure --prefix="$PREFIX" --disable-static --quiet
    make -j"$JOBS" --silent
    make install --silent
)

tar xzf libmtp.tar.gz
(
    cd "libmtp-${LIBMTP_VERSION}"
    # MTPZ is only for Zune-era devices and would drag in libgcrypt.
    PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig" ./configure \
        --prefix="$PREFIX" --disable-static --disable-mtpz --quiet
    make -j"$JOBS" --silent
    make install --silent
)

echo "built libusb ${LIBUSB_VERSION} and libmtp ${LIBMTP_VERSION} into $PREFIX"
echo "minimum macOS: $MACOSX_DEPLOYMENT_TARGET"
