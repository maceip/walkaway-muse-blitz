#!/usr/bin/env python3
"""Build the Walkaway hero artwork from real parts.

The campaign mascot (winged Muse robot) and the winged AI chips are campaign
drawings. The browser window in every hero is a real screenshot of the Hermes
browser, cropped out of ``assets/download/hero-winged-chips.png`` so the
product surface is never fabricated.

Outputs
    assets/download/hero-muse.png          1920 x 1080  main hero
    assets/img/hero-muse.webp              1920 x 1080  web version
    assets/download/hero-muse-flight.png   1920 x 1080  mirrored scene
    assets/img/hero-muse-flight.webp       1920 x 1080  web version
    assets/download/og-walkaway.png        1200 x  630  social card
    assets/download/square-muse.png        1080 x 1080  social square

Run:  /tmp/fontenv/bin/python scripts/build-hero-art.py
"""
from __future__ import annotations

import os
import sys

from PIL import Image

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SITE = os.path.join(ROOT, "apps", "marketing-site")
DL = os.path.join(SITE, "assets", "download")
IMG = os.path.join(SITE, "assets", "img")

INK = (0, 0, 0, 255)
GREEN = (0, 137, 66, 255)
PAPER = (239, 237, 226, 255)

# The real Hermes window, cropped out of the original hero artwork.
WINDOW_SRC = os.path.join(DL, "hero-winged-chips.png")
WINDOW_BOX = (230, 5, 1403, 874)

MASCOT = os.path.join(DL, "muse-mascot.png")
CHIPS = os.path.join(DL, "muse-chips.png")
MARK = os.path.join(DL, "mark-walkaway.png")

CANVAS = (1920, 1080)


def load(path: str) -> Image.Image:
    if not os.path.exists(path):
        sys.exit("missing input: %s" % path)
    return Image.open(path).convert("RGBA")


def scaled(path: str, width: int) -> Image.Image:
    """Scale a transparent asset to ``width``, keeping its aspect."""
    im = load(path)
    height = round(im.height * width / im.width)
    return im.resize((width, height), Image.LANCZOS)


def window_image() -> Image.Image:
    return load(WINDOW_SRC).crop(WINDOW_BOX)


def supervision_square(size: int) -> Image.Image:
    """Flat green square with the supervision mark knocked out of it."""
    sq = Image.new("RGBA", (size, size), GREEN)
    mark = load(MARK)
    inner = round(size * 0.62)
    mark = mark.resize((inner, inner), Image.LANCZOS)
    off = (size - inner) // 2
    sq.alpha_composite(mark, (off, off))
    return sq


def place(canvas: Image.Image, im: Image.Image, x: int, y: int) -> None:
    canvas.alpha_composite(im, (x, y))


def build_hero() -> Image.Image:
    """Main hero: real browser right, chips and mascot drifting in from the left."""
    canvas = Image.new("RGBA", CANVAS, INK)
    place(canvas, supervision_square(250), 48, 48)
    place(canvas, scaled(CHIPS, 380), 48, 330)
    place(canvas, scaled(MASCOT, 400), 190, 660)
    place(canvas, window_image(), 612, 96)
    return canvas


def build_flight() -> Image.Image:
    """Mirrored scene: real browser left, mascot flying in from the right."""
    canvas = Image.new("RGBA", CANVAS, INK)
    place(canvas, supervision_square(250), 1920 - 48 - 250, 48)
    place(canvas, scaled(CHIPS, 380).transpose(Image.FLIP_LEFT_RIGHT), 1920 - 48 - 380, 330)
    mascot = scaled(MASCOT, 400).transpose(Image.FLIP_LEFT_RIGHT)
    place(canvas, mascot, 1920 - 190 - 400, 660)
    place(canvas, window_image(), 1920 - 612 - 1173, 96)
    return canvas


def crop_social(hero: Image.Image, size: tuple[int, int]) -> Image.Image:
    """Fit the whole hero composition into a social format.

    The hero is 16:9 on pure black, so a format that is taller than the
    composition can simply be padded with black instead of cropping the
    mascot or the chips out of frame.
    """
    width, height = size
    scale = width / hero.width
    scaled = hero.resize((width, round(hero.height * scale)), Image.LANCZOS)
    if scaled.height >= height:
        top = (scaled.height - height) // 2
        return scaled.crop((0, top, width, top + height)).convert("RGB")
    canvas = Image.new("RGBA", size, INK)
    canvas.alpha_composite(scaled, (0, (height - scaled.height) // 2))
    return canvas


def save_png(im: Image.Image, path: str) -> None:
    im.convert("RGB").save(path, "PNG", optimize=True)
    print("wrote %s (%d bytes)" % (os.path.relpath(path, ROOT), os.path.getsize(path)))


def save_webp(im: Image.Image, path: str, quality: int = 82) -> None:
    im.convert("RGB").save(path, "WEBP", quality=quality, method=6)
    print("wrote %s (%d bytes)" % (os.path.relpath(path, ROOT), os.path.getsize(path)))


def main() -> None:
    hero = build_hero()
    save_png(hero, os.path.join(DL, "hero-muse.png"))
    save_webp(hero, os.path.join(IMG, "hero-muse.webp"))

    flight = build_flight()
    save_png(flight, os.path.join(DL, "hero-muse-flight.png"))
    save_webp(flight, os.path.join(IMG, "hero-muse-flight.webp"))

    save_png(crop_social(hero, (1200, 630)), os.path.join(DL, "og-walkaway.png"))
    save_png(crop_social(hero, (1080, 1080)), os.path.join(DL, "square-muse.png"))


if __name__ == "__main__":
    main()
