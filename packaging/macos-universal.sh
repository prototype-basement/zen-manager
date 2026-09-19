#!/usr/bin/env bash
# Builds a universal (arm64 + x86_64) ZEN Manager.app.
#
# Apple's clang cross-compiles between the two Mac architectures out of the
# box, so the hard part is not the compiler — it is libmtp. libmtp-sys locates
# it through pkg-config, and Homebrew only ever installs for the architecture
# it was itself installed as. Building x86_64 therefore needs a second,
# Rosetta Homebrew under /usr/local providing an x86_64 libmtp.
#
# Setup, once:
#   rustup target add x86_64-apple-darwin aarch64-apple-darwin
#   arch -x86_64 /bin/bash -c \
#     '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'
#   arch -x86_64 /usr/local/bin/brew install libmtp
set -euo pipefail

NAME="ZEN Manager"
BIN="zenmanager"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARM_PKG="/opt/homebrew/opt/libmtp/lib/pkgconfig"
INTEL_PKG="/usr/local/opt/libmtp/lib/pkgconfig"

missing=0
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target list --installed | grep -qx "$t" || {
        echo "missing rust target: $t  →  rustup target add $t"; missing=1; }
done
[ -d "$ARM_PKG" ]   || { echo "missing arm64 libmtp   → brew install libmtp"; missing=1; }
[ -d "$INTEL_PKG" ] || { echo "missing x86_64 libmtp  → see the setup notes at the top of this script"; missing=1; }
[ "$missing" -eq 0 ] || exit 1

# PKG_CONFIG_ALLOW_CROSS is required because pkg-config refuses to answer for a
# target that is not the host unless told it is safe.
echo "building arm64…"
PKG_CONFIG_PATH="$ARM_PKG" PKG_CONFIG_ALLOW_CROSS=1 \
    cargo build --release --target aarch64-apple-darwin

echo "building x86_64…"
PKG_CONFIG_PATH="$INTEL_PKG" PKG_CONFIG_ALLOW_CROSS=1 \
    cargo build --release --target x86_64-apple-darwin

OUT="target/universal/release"
mkdir -p "$OUT"
lipo -create \
    "target/aarch64-apple-darwin/release/$BIN" \
    "target/x86_64-apple-darwin/release/$BIN" \
    -output "$OUT/$BIN"

# Bundle from the arm64 tree, then swap in the fat binary and fatten the
# dylibs the same way, or the app would only run on one architecture.
./packaging/macos-bundle.sh aarch64-apple-darwin > /dev/null
APP="target/universal/release/bundle/$NAME.app"
rm -rf "$(dirname "$APP")"
mkdir -p "$(dirname "$APP")"
ditto "target/aarch64-apple-darwin/release/bundle/$NAME.app" "$APP"
cp "$OUT/$BIN" "$APP/Contents/MacOS/$BIN"

for lib in "$APP/Contents/Frameworks/"*.dylib; do
    base="$(basename "$lib")"
    intel="/usr/local/opt/$(echo "$base" | sed 's/^lib//; s/[.-].*//')/lib/$base"
    # Homebrew's layout varies per formula; fall back to a filesystem search.
    [ -f "$intel" ] || intel="$(find /usr/local/Cellar -name "$base" -type f 2>/dev/null | head -1)"
    if [ -n "$intel" ] && [ -f "$intel" ]; then
        tmp="$(mktemp -t fatlib)"
        lipo -create "$lib" "$intel" -output "$tmp" 2>/dev/null && mv "$tmp" "$lib"
        install_name_tool -id "@rpath/$base" "$lib"
    else
        echo "warning: no x86_64 build of $base found; the app stays arm64-only"
    fi
done

# Re-point and re-sign after the swap; the old signature is now invalid.
for lib in "$APP/Contents/Frameworks/"*.dylib; do
    for dep in $(otool -L "$lib" | tail -n +2 | awk '{print $1}' | grep -vE '^/usr/lib|^/System|^@'); do
        install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$lib" 2>/dev/null || true
    done
done
for dep in $(otool -L "$APP/Contents/MacOS/$BIN" | tail -n +2 | awk '{print $1}' | grep -vE '^/usr/lib|^/System|^@'); do
    install_name_tool -change "$dep" "@rpath/$(basename "$dep")" "$APP/Contents/MacOS/$BIN"
done
install_name_tool -add_rpath "@executable_path/../Frameworks" "$APP/Contents/MacOS/$BIN" 2>/dev/null || true
codesign --force --deep --sign - "$APP" 2>/dev/null || true

echo "built $APP"
lipo -archs "$APP/Contents/MacOS/$BIN"
