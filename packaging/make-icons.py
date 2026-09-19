#!/usr/bin/env python3
"""Generates the macOS and Linux icon sets from logo.png.

The source is fine black line art on white. Two things follow from that:

  * It is kept as a white rounded tile rather than knocked through to
    transparency. Bare black strokes on a transparent background disappear
    against a dark Dock or menu; a white tile reads the same in both themes.

  * Thin strokes average out to near-white when downscaled, so at 16 and 32 px
    the artwork is unrecognisable. Small renditions are therefore generated
    from a darkened, stroke-thickened copy. Large ones use the original.

Requires Pillow. Run via packaging/make-icons.sh, which handles the venv.
"""

import os
import shutil
import subprocess
import sys
import tempfile
from PIL import Image, ImageEnhance, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(ROOT, "assets", "logo.png")
OUT = os.path.join(ROOT, "assets", "icons")

CANVAS = 1024
BODY = 824           # Apple's macOS grid: the tile inside a 1024 canvas.
RADIUS = 185         # ~22.5% of the body, matching the system shape.

MAC_SIZES = [16, 32, 64, 128, 256, 512, 1024]
LINUX_SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
# Below this, unmodified line art stops being legible.
BOLD_BELOW = 64


def rounded_mask(size, radius):
    mask = Image.new("L", (size, size), 0)
    from PIL import ImageDraw
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1], radius, fill=255)
    return mask


def bolden(img):
    """Thickens dark strokes so they survive downscaling.

    MinFilter spreads the darkest pixel in a neighbourhood, which grows black
    lines against white; the contrast bump stops what is left going grey.
    """
    out = img.convert("RGB").filter(ImageFilter.MinFilter(5))
    return ImageEnhance.Contrast(out).enhance(1.6).convert("RGBA")


def tile(source, size, bold):
    """One square rendition: white rounded tile, artwork inset, alpha outside."""
    art = bolden(source) if bold else source.convert("RGBA")

    body = round(size * BODY / CANVAS)
    radius = max(1, round(body * RADIUS / BODY))

    # Flatten onto white first: the source's own background is opaque white,
    # and compositing keeps any antialiased edges clean.
    inner = Image.new("RGBA", (CANVAS, CANVAS), (255, 255, 255, 255))
    inner.alpha_composite(art)
    inner = inner.resize((body, body), Image.LANCZOS)
    inner.putalpha(rounded_mask(body, radius))

    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.alpha_composite(inner, (round((size - body) / 2), round((size - body) / 2)))
    return canvas


def main():
    if not os.path.exists(SRC):
        sys.exit(
            f"missing {SRC}\n"
            "Put the 1024x1024 master artwork there to regenerate the icons.\n"
            "The icons in assets/icons/ are committed, so the\n"
            "app still packages without it — you only need this to change them."
        )
    source = Image.open(SRC).convert("RGBA")
    os.makedirs(OUT, exist_ok=True)

    # macOS iconset: each point size at 1x and 2x. Built in a temp directory,
    # since it is only ever input to iconutil.
    tmp = tempfile.mkdtemp()
    iconset = os.path.join(tmp, "ZenManager.iconset")
    os.makedirs(iconset, exist_ok=True)
    for pt in [16, 32, 128, 256, 512]:
        for scale, suffix in ((1, ""), (2, "@2x")):
            px = pt * scale
            tile(source, px, px < BOLD_BELOW).save(
                os.path.join(iconset, f"icon_{pt}x{pt}{suffix}.png"))

    icns = os.path.join(OUT, "ZenManager.icns")
    if subprocess.run(["iconutil", "-c", "icns", iconset, "-o", icns],
                      capture_output=True).returncode == 0:
        print(f"wrote {os.path.relpath(icns, ROOT)}")
    else:
        print("iconutil failed; the .icns was not written", file=sys.stderr)

    # Linux hicolor sizes.
    shutil.rmtree(tmp, ignore_errors=True)

    linux = os.path.join(OUT, "linux")
    os.makedirs(linux, exist_ok=True)
    for px in LINUX_SIZES:
        tile(source, px, px < BOLD_BELOW).save(os.path.join(linux, f"{px}.png"))
    print(f"wrote {len(LINUX_SIZES)} Linux PNGs")

    # Square copy for the About page, at 2x the display size for retina.
    about = tile(source, 192, False)
    about.save(os.path.join(OUT, "appIcon.png"))
    print("wrote assets/icons/appIcon.png for the About page")


if __name__ == "__main__":
    main()
