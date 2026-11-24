#!/usr/bin/env python3
"""
Draw the deep-space backdrop for the AI Red Team page.

The sky behind the globe used to be two NASA survey photographs of the
Andromeda galaxy, and they were the reason the page looked fake: the plates
are 580x498 and 550x373 pixels and the page stretched them to 168vmax — a
five- or six-fold upscale of a thumbnail. A stretched thumbnail reads as a
pasted rectangle, not as space.

So the sky is drawn here instead, at 4096x2304, from the things that actually
make a view of space look like a view of space:

  * a deep field of ~40,000 stars on a real magnitude distribution — a great
    many faint ones and a handful of bright ones, which is the opposite of the
    evenly-sized dots a CSS starfield gives you;
  * colour by spectral class, so the field is not all one white;
  * bloom and four-point diffraction on the brightest stars only, because that
    is what a bright star does and it is what sells the scale;
  * a galactic band of unresolved starlight and dust, with the dust rifts that
    cut it into lanes;
  * the Andromeda galaxy itself, at an inclination, with logarithmic spiral
    arms, a warm core, blue star-forming knots along the arms and a dust lane
    running just inside each arm;
  * a few nebulae in the page's own green and magenta, so the sky belongs to
    the same world as the globe;
  * a dither pass, because an 8-bit gradient that size bands badly without one.

The render is deterministic: the same seed produces the same bytes, so the
build can prove that the shipped plate is the one this script draws.

Outputs, into apps/ai-red-team-web/public/media/space/:

    deep-field.webp    the sky, 4096x2304
    deep-field.jpg     the same sky, for anything that cannot take webp

Usage:
    python3 scripts/build-space-backdrop.py
    python3 scripts/build-space-backdrop.py --check
    python3 scripts/build-space-backdrop.py --width 1024 --out /tmp/sky.webp
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sys
import tempfile

try:
    import numpy as np
except ImportError:  # pragma: no cover
    print("numpy is required: python3 -m pip install numpy", file=sys.stderr)
    raise SystemExit(2)

try:
    from PIL import Image
except ImportError:  # pragma: no cover
    print("Pillow is required: python3 -m pip install Pillow", file=sys.stderr)
    raise SystemExit(2)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SITE = os.path.join(ROOT, "apps", "ai-red-team-web")
OUT = os.path.join(SITE, "public", "media", "space")

WIDTH = 4096
HEIGHT = 2304
SEED = 20261123

# The smooth layers are drawn at half resolution and scaled up: a nebula has
# no high-frequency detail to lose, and the stars that do have it are drawn
# afterwards at full resolution. This is the difference between a render that
# takes half a minute and one that takes a couple of seconds.
SMOOTH_SCALE = 0.5

# The sky is mostly black, but not pure black: space is not #000, and a page
# of pure black with a vignette on top of it reads as an empty div.
BASE = np.array([3.0, 4.5, 10.0], dtype=np.float32)


# --------------------------------------------------------------------------
# noise
# --------------------------------------------------------------------------

def value_noise(rng: np.random.Generator, h: int, w: int, octaves: int, base: int) -> np.ndarray:
    """Fractal value noise in [0, 1], built from stacked upscaled random grids."""
    out = np.zeros((h, w), dtype=np.float32)
    amp = 1.0
    total = 0.0
    for octave in range(octaves):
        cells = max(2, base * (2 ** octave))
        gh = cells
        gw = max(2, int(round(cells * w / h)))
        grid = rng.random((gh, gw), dtype=np.float32)
        layer = Image.fromarray((grid * 255.0).astype(np.uint8), "L").resize((w, h), Image.BICUBIC)
        out += amp * (np.asarray(layer, dtype=np.float32) / 255.0)
        total += amp
        amp *= 0.55
    return out / total


# --------------------------------------------------------------------------
# layers
# --------------------------------------------------------------------------

def galactic_band(rng: np.random.Generator, h: int, w: int) -> np.ndarray:
    """
    The wide band of unresolved starlight that crosses the sky, cut by dust.

    A band alone is a stripe; what makes it read as the Milky Way is the
    clumping (the low-frequency noise that varies the brightness along and
    across the band) and the rifts (the dust that eats into it).
    """
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    cx, cy = w * 0.26, h * 0.58
    ang = np.deg2rad(-19.0)
    dx = xx - cx
    dy = yy - cy
    v = -dx * np.sin(ang) + dy * np.cos(ang)
    d = np.abs(v)

    band = np.exp(-((d / (h * 0.15)) ** 2))
    halo = np.exp(-((d / (h * 0.44)) ** 2))

    clump = value_noise(rng, h, w, 6, base=3)
    clump = np.clip((clump - 0.44) * 2.4, 0.0, 1.0)

    dust = value_noise(rng, h, w, 5, base=4)
    dust = np.clip((dust - 0.50) * 3.2, 0.0, 1.0)

    body = band * (0.30 + 0.95 * clump) + halo * (0.16 + 0.24 * clump)
    body *= 1.0 - 0.72 * dust * np.maximum(band, 0.35)
    return body


def andromeda(rng: np.random.Generator, h: int, w: int):
    """
    The Andromeda galaxy, inclined, with two logarithmic arms.

    Returns the light of the disk, the blue star-forming knots that sit along
    the arms, and the warm bulge at the core.
    """
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    cx, cy = w * 0.705, h * 0.27
    major = h * 0.70
    squash = 0.31           # how flattened the disk looks at this inclination
    tilt = np.deg2rad(26.0)

    dx = xx - cx
    dy = yy - cy
    u = dx * np.cos(tilt) + dy * np.sin(tilt)
    v = (-dx * np.sin(tilt) + dy * np.cos(tilt)) / squash
    r = np.sqrt(u * u + v * v) + 1e-3
    phi = np.arctan2(v, u)

    disk = np.exp(-r / (major * 0.30))

    # two logarithmic spiral arms: r grows exponentially with the winding angle
    winding = 4.4
    r0 = major * 0.055
    phase = phi - winding * np.log(r / r0)
    wrapped = np.mod(phase + np.pi, 2 * np.pi) - np.pi

    arms = np.zeros_like(r)
    for offset, weight in ((0.0, 1.0), (np.pi, 0.82)):
        dphi = np.mod(wrapped - offset + np.pi, 2 * np.pi) - np.pi
        width = 0.26 + 0.50 * np.clip(r / major, 0.0, 1.4)
        arms += weight * np.exp(-((dphi / width) ** 2)) * np.exp(-r / (major * 0.60))
    disk = disk + arms

    # the dust lane runs just inside each arm
    lane_phase = np.mod(phase + np.pi * 0.42 + np.pi, 2 * np.pi) - np.pi
    lane = np.exp(-((lane_phase / 0.24) ** 2)) * np.exp(-r / (major * 0.46))

    # star-forming knots: small, bright, blue, and only where the arms are
    knot_noise = value_noise(rng, h, w, 6, base=18)
    knots = np.clip((knot_noise - 0.60) * 3.6, 0.0, 1.0) * np.clip(arms, 0.0, 2.0)

    # the bulge: old, warm stars piled up in the middle
    core = np.exp(-((r / (major * 0.052)) ** 1.7))

    light = (disk + core * 1.5) * (1.0 - 0.60 * lane)
    return light, knots, core


def nebulae(rng: np.random.Generator, h: int, w: int):
    """A few soft clouds of gas, in the page's own colours."""
    cloud = value_noise(rng, h, w, 6, base=3)
    detail = value_noise(rng, h, w, 5, base=7)
    shape = np.clip((cloud - 0.36) * 1.9, 0.0, 1.0) * (0.55 + 0.65 * detail)

    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    blobs = []
    for cx, cy, rad, tint in (
        (0.16, 0.26, 0.40, (26.0, 96.0, 104.0)),    # teal
        (0.86, 0.72, 0.34, (92.0, 30.0, 96.0)),     # magenta
        (0.52, 0.10, 0.30, (24.0, 52.0, 128.0)),    # deep blue
        (0.30, 0.86, 0.26, (24.0, 104.0, 78.0)),    # green
    ):
        d = np.sqrt(((xx / w) - cx) ** 2 + ((yy / h) - cy) ** 2)
        falloff = np.exp(-((d / rad) ** 2) * 2.4)
        blobs.append((shape * falloff, np.array(tint, dtype=np.float32)))
    return blobs


# --------------------------------------------------------------------------
# stars
# --------------------------------------------------------------------------

def star_colours(rng: np.random.Generator, count: int) -> np.ndarray:
    """
    Star colour by spectral class.

    Most stars are hot and blue-white; a few are cool and orange. Drawing
    them all the same white is what makes a synthetic starfield look like
    confetti.
    """
    t = rng.random(count) ** 2.1              # 0 = blue-white, 1 = orange
    blue = np.array([0.74, 0.82, 1.0], dtype=np.float32)
    white = np.array([1.0, 0.98, 0.96], dtype=np.float32)
    orange = np.array([1.0, 0.74, 0.52], dtype=np.float32)

    t = t[:, None]
    warm = np.clip((t - 0.55) / 0.45, 0.0, 1.0)
    cool = np.clip((0.40 - t) / 0.40, 0.0, 1.0)
    colour = white + (orange - white) * warm + (blue - white) * cool
    return np.clip(colour, 0.0, 1.0)


def draw_stars(rng: np.random.Generator, img: np.ndarray, count: int,
               density: np.ndarray | None = None) -> None:
    """
    Scatter stars over the field, in place.

    The brightness distribution is the thing that matters: a power law with a
    steep exponent, so that a handful of stars are bright and a great many are
    barely there. The faint ones sit close to the noise floor, which is exactly
    how a real field looks and is why the bright ones read as bright.

    `density` is a map of where stars are allowed to land. Without it the
    field is uniform and reads as noise; with it the sky has a crowded band
    and thin corners, which is what a real sky does.
    """
    h, w = img.shape[:2]
    if density is None:
        density = np.ones((h, w), dtype=np.float32)

    # rejection-sample, so the number of stars follows the density map
    candidates = int(count * 2.1)
    rx = rng.random(candidates) * w
    ry = rng.random(candidates) * h
    keep = rng.random(candidates) < np.clip(
        density[ry.astype(np.int32), rx.astype(np.int32)], 0.0, 1.0)
    x = rx[keep]
    y = ry[keep]

    u = rng.random(x.size)
    flux = u ** 3.6
    flux = flux / max(flux.max(), 1e-6)

    colour = star_colours(rng, x.size)
    brightness = 16.0 + 225.0 * (flux ** 0.85)

    xi = np.round(x).astype(np.int32)
    yi = np.round(y).astype(np.int32)
    inside = (xi >= 0) & (xi < w) & (yi >= 0) & (yi < h)
    xi, yi = xi[inside], yi[inside]
    brightness = brightness[inside]
    colour = colour[inside]

    # a small point-spread function: the star and its immediate neighbours.
    # Each neighbour is written with its own coordinates so a star at the very
    # edge of the frame lights up the edge of the frame and not the far side
    # of the buffer.
    flat = img.reshape(-1, 3)
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            weight = 1.0 if (dx == 0 and dy == 0) else 0.38
            xs = xi + dx
            ys = yi + dy
            valid = (xs >= 0) & (xs < w) & (ys >= 0) & (ys < h)
            idx = (ys * w + xs)[valid]
            if idx.size == 0:
                continue
            for c in range(3):
                vals = (brightness * weight * colour[:, c])[valid]
                np.add.at(flat[:, c], idx, vals)

    # bloom and diffraction spikes on the brightest few
    order = np.argsort(brightness)[::-1]
    sigma = 3.4
    radius = 13
    grid = np.arange(-radius, radius + 1)
    gy, gx = np.meshgrid(grid, grid, indexing="ij")
    halo = np.exp(-((gx ** 2 + gy ** 2) / (2.0 * sigma * sigma))).astype(np.float32)
    spike = (np.exp(-(gy.astype(np.float32) ** 2) / 2.0)
             * np.exp(-(gx.astype(np.float32) ** 2) / (2.0 * 1.1 ** 2))
             + np.exp(-(gx.astype(np.float32) ** 2) / 2.0)
             * np.exp(-(gy.astype(np.float32) ** 2) / (2.0 * 1.1 ** 2)))
    spikes = (spike * 0.16).astype(np.float32)

    for i in order[:240]:
        cx, cy = int(xi[i]), int(yi[i])
        x0, x1 = max(0, cx - radius), min(w, cx + radius + 1)
        y0, y1 = max(0, cy - radius), min(h, cy + radius + 1)
        if x1 <= x0 or y1 <= y0:
            continue
        sub_halo = halo[y0 - (cy - radius):y1 - (cy - radius),
                        x0 - (cx - radius):x1 - (cx - radius)]
        sub_spike = spikes[y0 - (cy - radius):y1 - (cy - radius),
                           x0 - (cx - radius):x1 - (cx - radius)]
        weight = brightness[i] / 255.0
        img[y0:y1, x0:x1] += (sub_halo * 0.55 + sub_spike)[:, :, None] * colour[i][None, None, :] * weight


# --------------------------------------------------------------------------
# render
# --------------------------------------------------------------------------

def render(width: int = WIDTH, height: int = HEIGHT) -> Image.Image:
    """Draw the whole sky and return it as an image."""
    rng = np.random.default_rng(SEED)

    sw = int(width * SMOOTH_SCALE)
    sh = int(height * SMOOTH_SCALE)

    # --- smooth layers at half resolution ---------------------------------
    band = galactic_band(rng, sh, sw)
    light, knots, core = andromeda(rng, sh, sw)
    clouds = nebulae(rng, sh, sw)

    sky = np.zeros((sh, sw, 3), dtype=np.float32)
    sky += BASE

    # the galactic band: unresolved starlight, warm and slightly dusty
    band_rgb = np.array([150.0, 146.0, 150.0], dtype=np.float32)
    sky += (band * 0.165)[:, :, None] * band_rgb[None, None, :]

    # the nebulae
    for shape, tint in clouds:
        sky += (shape * 0.085)[:, :, None] * tint[None, None, :]

    # andromeda: the disk is blue-white, the knots are bluer, the bulge warm
    sky += (light * 0.135)[:, :, None] * np.array([150.0, 168.0, 205.0], dtype=np.float32)[None, None, :]
    sky += (knots * 0.30)[:, :, None] * np.array([120.0, 170.0, 255.0], dtype=np.float32)[None, None, :]
    sky += (core * 0.20)[:, :, None] * np.array([255.0, 214.0, 168.0], dtype=np.float32)[None, None, :]

    smooth = Image.fromarray(np.clip(sky, 0, 255).astype(np.uint8), "RGB").resize(
        (width, height), Image.BICUBIC
    )
    img = np.asarray(smooth, dtype=np.float32).copy()

    # --- stars at full resolution ----------------------------------------
    # The stars crowd towards the galactic band and thin out away from it, so
    # the sky has a direction to it instead of being an even sprinkle.
    band_full = np.asarray(
        Image.fromarray((np.clip(band, 0, 1) * 255).astype(np.uint8), "L").resize(
            (width, height), Image.BICUBIC),
        dtype=np.float32) / 255.0
    density = 0.16 + 1.05 * band_full
    draw_stars(rng, img, 46000, density)

    # --- dither and quantise ---------------------------------------------
    dither = rng.random(img.shape).astype(np.float32) - 0.5
    img += dither * 1.35
    return Image.fromarray(np.clip(img, 0, 255).astype(np.uint8), "RGB")


def encode(im: Image.Image, path: str) -> None:
    if path.endswith(".webp"):
        im.save(path, "WEBP", quality=84, method=6)
    else:
        im.save(path, "JPEG", quality=92, optimize=True, progressive=True, subsampling=0)


def digest(path: str) -> str:
    with open(path, "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()[:16]


def build() -> list[tuple[str, str]]:
    os.makedirs(OUT, exist_ok=True)
    im = render()
    written = []
    for name, encode_args in (("deep-field.webp", None), ("deep-field.jpg", None)):
        path = os.path.join(OUT, name)
        encode(im, path)
        written.append((path, digest(path)))
    return written


def check() -> int:
    im = render()
    bad = []
    for name in ("deep-field.webp", "deep-field.jpg"):
        path = os.path.join(OUT, name)
        if not os.path.isfile(path):
            bad.append("missing " + name)
            continue
        with tempfile.NamedTemporaryFile(suffix=os.path.splitext(name)[1], delete=False) as tmp:
            tmp_path = tmp.name
        encode(im, tmp_path)
        if digest(path) != digest(tmp_path):
            bad.append("stale " + name)
        os.unlink(tmp_path)
    if bad:
        for b in bad:
            print("FAIL " + b)
        return 1
    print("PASS space backdrop matches scripts/build-space-backdrop.py")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="verify the shipped plate is the one this script draws")
    ap.add_argument("--width", type=int, default=WIDTH,
                    help="render width (for quick iteration)")
    ap.add_argument("--out", default=None,
                    help="write a single preview file here instead of shipping")
    args = ap.parse_args()

    if args.out:
        height = int(round(args.width * HEIGHT / WIDTH))
        im = render(args.width, height)
        encode(im, args.out)
        print("wrote %s  %d bytes" % (args.out, os.path.getsize(args.out)))
        return 0

    if args.check:
        return check()

    for path, sha in build():
        print("wrote %s  %s  %d bytes" % (
            os.path.relpath(path, ROOT), sha, os.path.getsize(path)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
