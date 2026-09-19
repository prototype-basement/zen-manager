#!/usr/bin/env bash
# Builds Linux tarballs for x86_64 and arm64 in containers.
#
# Needs a container runtime. On a Mac any of these work:
#   brew install --cask docker        # Docker Desktop
#   brew install colima docker        # lighter, CLI only: colima start
#   brew install podman               # podman machine init && podman machine start
#
# The non-native architecture runs under emulation and is slow — expect
# 10-20 minutes for the arm64 build on an Intel Mac, or x86_64 on Apple
# Silicon. It is still far more reliable than assembling a cross sysroot.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RUNTIME="${RUNTIME:-}"
if [ -z "$RUNTIME" ]; then
    for candidate in docker podman; do
        command -v "$candidate" > /dev/null && { RUNTIME="$candidate"; break; }
    done
fi
[ -n "$RUNTIME" ] || {
    echo "No container runtime found. See the notes at the top of this script." >&2
    exit 1
}

TARGETS="${*:-linux/amd64 linux/arm64}"
mkdir -p dist

for platform in $TARGETS; do
    arch="${platform##*/}"
    tag="zen-manager-build:$arch"
    echo "==> building image for $platform"
    "$RUNTIME" build --platform "$platform" -f packaging/Dockerfile.linux -t "$tag" .

    echo "==> building package for $platform"
    # target/ is mounted per-arch so the two builds cannot clobber each other's
    # artifacts, and neither disturbs the host's own target directory.
    "$RUNTIME" run --rm --platform "$platform" \
        -v "$ROOT:/src" \
        -v "zen-target-$arch:/src/target" \
        "$tag" \
        bash -c './packaging/linux-package.sh && cp target/release/package/*.tar.gz /src/dist/'
done

echo
ls -lh dist/*.tar.gz
