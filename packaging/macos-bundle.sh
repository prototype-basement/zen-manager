#!/usr/bin/env bash
# Builds ZEN Manager.app for the host architecture.
#
# The bundle carries its own copies of libmtp and libusb with their install
# names rewritten to @rpath, so it runs on a Mac that has never seen Homebrew.
# Without this the app launches only on machines with libmtp in exactly the
# same prefix it was built against.
set -euo pipefail

TARGET="${1:-}"
NAME="ZEN Manager"
BIN="zenmanager"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [ -n "$TARGET" ]; then
    cargo build --release --target "$TARGET"
    BIN_PATH="target/$TARGET/release/$BIN"
    OUT="target/$TARGET/release/bundle"
else
    cargo build --release
    BIN_PATH="target/release/$BIN"
    OUT="target/release/bundle"
fi

APP="$OUT/$NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Frameworks" "$APP/Contents/Resources"

cp "$BIN_PATH" "$APP/Contents/MacOS/$BIN"
chmod +x "$APP/Contents/MacOS/$BIN"

# Committed by packaging/make-icons.sh; see that script to change the artwork.
ICNS="$ROOT/assets/icons/ZenManager.icns"
if [ -f "$ICNS" ]; then
    cp "$ICNS" "$APP/Contents/Resources/ZenManager.icns"
else
    echo "warning: $ICNS missing; the app will use the system placeholder"
fi

VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$NAME</string>
    <key>CFBundleDisplayName</key><string>$NAME</string>
    <key>CFBundleExecutable</key><string>$BIN</string>
    <key>CFBundleIdentifier</key><string>dev.zenmanager.app</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleIconFile</key><string>ZenManager</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>MIT-0</string>
</dict>
</plist>
PLIST

# Copy each non-system dependency in and point the binary at the bundled copy.
copy_dep() {
    local lib="$1"
    local base
    base="$(basename "$lib")"
    if [ ! -f "$APP/Contents/Frameworks/$base" ]; then
        cp "$lib" "$APP/Contents/Frameworks/$base"
        chmod u+w "$APP/Contents/Frameworks/$base"
        install_name_tool -id "@rpath/$base" "$APP/Contents/Frameworks/$base"
    fi
    echo "$base"
}

deps_of() {
    otool -L "$1" | tail -n +2 | awk '{print $1}' \
        | grep -vE '^/usr/lib|^/System|^@' || true
}

# libmtp pulls in libusb, so walk the graph rather than assuming one level.
declare -a QUEUE=("$APP/Contents/MacOS/$BIN")
declare -a BUNDLED=()
while [ ${#QUEUE[@]} -gt 0 ]; do
    current="${QUEUE[0]}"
    QUEUE=("${QUEUE[@]:1}")
    while read -r dep; do
        [ -z "$dep" ] && continue
        base="$(basename "$dep")"
        if [ ! -f "$APP/Contents/Frameworks/$base" ]; then
            copy_dep "$dep" > /dev/null
            BUNDLED+=("$base")
            QUEUE+=("$APP/Contents/Frameworks/$base")
        fi
        install_name_tool -change "$dep" "@rpath/$base" "$current"
    done < <(deps_of "$current")
done

install_name_tool -add_rpath "@executable_path/../Frameworks" "$APP/Contents/MacOS/$BIN"

# Ad-hoc signature: without it arm64 macOS refuses to run a rewritten binary.
codesign --force --deep --sign - "$APP" 2>/dev/null || \
    echo "warning: ad-hoc codesign failed; the app may not launch on arm64"

echo "built $APP"
echo "bundled: ${BUNDLED[*]:-none}"
otool -L "$APP/Contents/MacOS/$BIN" | grep -vE '/usr/lib|/System' | tail -n +2
