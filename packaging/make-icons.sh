#!/usr/bin/env bash
# Regenerates the icon sets from assets/logo.png.
#
# Not part of the build. The icons it produces live in assets/icons/ and are
# committed, so packaging and CI work without ever running this. Run it only
# when the artwork changes, and commit the result.
#
# Pillow lives in a throwaway venv so nothing is installed system-wide.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV="$ROOT/target/icon-venv"

[ -d "$VENV" ] || {
    python3 -m venv "$VENV"
    "$VENV/bin/pip" install --quiet Pillow
}
exec "$VENV/bin/python" "$ROOT/packaging/make-icons.py"
