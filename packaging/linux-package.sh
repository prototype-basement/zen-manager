#!/usr/bin/env bash
# Builds a Linux tarball for the host architecture.
#
# libmtp is NOT bundled. On Linux it is a normal distro package that also
# installs the udev rules deciding whether a non-root user may touch the
# device; shipping a private copy would not install those rules and the app
# would fail with permission errors that look like bugs.
set -euo pipefail

NAME="zen-manager"
BIN="zenmanager"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="$(uname -m)"
VERSION="$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"
STAGE="target/release/package/$NAME-$VERSION-linux-$ARCH"

cargo build --release

rm -rf "$STAGE"
mkdir -p "$STAGE"
cp "target/release/$BIN" "$STAGE/$NAME"
chmod +x "$STAGE/$NAME"
cp README.md LICENSE THIRD-PARTY.md "$STAGE/"
cp fonts/OFL.txt "$STAGE/OFL.txt"

# Committed by packaging/make-icons.sh; see that script to change the artwork.
if [ -d assets/icons/linux ]; then
    mkdir -p "$STAGE/icons"
    cp assets/icons/linux/*.png "$STAGE/icons/"
else
    echo "warning: assets/icons/linux missing; no icon will be installed"
fi

cat > "$STAGE/$NAME.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=ZEN Manager
GenericName=Music transfer
Comment=Modern software for two decade old hardware
Exec=$NAME
Icon=$NAME
Categories=AudioVideo;Audio;Utility;
Terminal=false
DESKTOP

cat > "$STAGE/install.sh" <<'INSTALL'
#!/usr/bin/env bash
# Installs into ~/.local, so no root is needed.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m755 "$HERE/zen-manager" "$HOME/.local/bin/zen-manager"
install -m644 "$HERE/zen-manager.desktop" "$HOME/.local/share/applications/zen-manager.desktop"

# hicolor is the theme every desktop falls back to, so the icon shows up
# regardless of which theme the user has chosen.
for png in "$HERE"/icons/*.png; do
    [ -e "$png" ] || continue
    size="$(basename "$png" .png)"
    dir="$HOME/.local/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$dir"
    install -m644 "$png" "$dir/zen-manager.png"
done
command -v gtk-update-icon-cache > /dev/null && \
    gtk-update-icon-cache -q -t -f "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

echo "Installed to ~/.local/bin/zen-manager"
echo
echo "This needs libmtp at runtime. If the app reports no device, install it:"
echo "  Debian/Ubuntu : sudo apt install libmtp9 libmtp-runtime"
echo "  Fedora        : sudo dnf install libmtp"
echo "  Arch          : sudo pacman -S libmtp"
echo
echo "libmtp-runtime (or your distro's equivalent) also installs the udev rules"
echo "that let a normal user access the player. Without them you would need root."
INSTALL
chmod +x "$STAGE/install.sh"

tar -czf "$STAGE.tar.gz" -C "$(dirname "$STAGE")" "$(basename "$STAGE")"
echo "built $STAGE.tar.gz"
