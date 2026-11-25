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

  * a deep field of ~46,000 stars on a real magnitude distribution — a great
    many faint ones and a handful of bright ones, which is the opposite of
    the evenly-sized dots a CSS starfield gives you;
  * colour by spectral class, so the field is not all one white;
  * a second, brighter tier of stars drawn wide enough to arrive at the
    screen as points rather than as dust — see `draw_stars`;
  * bloom and four-point diffraction on the brightest stars only, because that
    is what a bright star does and it is what sells the scale;
  * a galactic band of unresolved starlight and dust, with a bright core, a
    dark rift running along it and the dust lanes that cut it into pieces;
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

# How much the browser shrinks the plate to fit a screen.
#
# The plate is 4096 wide and a screen is 1280 to 2560, so every star in it is
# scaled down by a factor of between 1.6 and 3.2 before it reaches the eye.
# A star drawn as a single pixel arrives smaller than a pixel and is averaged
# down into grey dust — which is exactly what the sky looked like: a field of
# dust with a galaxy in it, and no stars in it at all.
#
# So the stars are drawn in two tiers. The faint tier is left tight, because
# faint stars *should* be dust: it is the grain that gives the field its
# depth. The bright tier is drawn with a point-spread function wide enough
# that it arrives at the screen as a point of about a pixel across, which is
# what makes a star read as a star rather than as a speck.
DISPLAY_SCALE = 1280.0 / WIDTH      # the scale the plate is usually seen at
STAR_TIER_WIDTH = 2.15              # plate pixels across a bright star
#
# A bright star drawn at STAR_TIER_WIDTH arrives at the screen about a pixel
# across, which is what a star looks like. Drawn any tighter than that — a
# single plate pixel is the obvious mistake — it arrives smaller than a pixel
# and the browser averages it down into the dust with everything else.
#
# The faint tier is deliberately tighter and much dimmer, because faint stars
# *should* be grain: that grain is what gives the field its depth.


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


def warped_noise(rng: np.random.Generator, h: int, w: int, octaves: int, base: int,
                 warp: float = 0.0) -> np.ndarray:
    """
    Value noise whose coordinates are pushed around before it is sampled.

    Straight value noise makes rounded blobs and, where it is used at a fine
    scale, visible rings — the grid of the noise is still there, in arcs. Warp
    the lookup with a second, coarser noise field and the grid disappears into
    something that looks like gas.
    """
    if warp <= 0.0:
        return value_noise(rng, h, w, octaves, base)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    wx = value_noise(rng, h, w, 3, base=max(2, base // 3)) - 0.5
    wy = value_noise(rng, h, w, 3, base=max(2, base // 3)) - 0.5
    sx = np.clip(xx + wx * warp * w, 0, w - 1)
    sy = np.clip(yy + wy * warp * h, 0, h - 1)
    out = np.zeros((h, w), dtype=np.float32)
    amp = 1.0
    total = 0.0
    for octave in range(octaves):
        cells = max(2, base * (2 ** octave))
        gh = cells
        gw = max(2, int(round(cells * w / h)))
        grid = rng.random((gh, gw), dtype=np.float32)
        layer = Image.fromarray((grid * 255.0).astype(np.uint8), "L").resize((w, h), Image.BICUBIC)
        sampled = np.asarray(layer, dtype=np.float32)[
            np.clip(sy.astype(np.int32), 0, h - 1), np.clip(sx.astype(np.int32), 0, w - 1)
        ] / 255.0
        out += amp * sampled
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

    The band also has to have a *core*. A wide, even smear reads as a smudge
    or a lens flare, which is what the first version of this looked like; a
    bright, narrow ridge of starlight down the middle of a wider glow is what
    reads as the Milky Way seen edge-on.
    """
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    cx, cy = w * 0.26, h * 0.58
    ang = np.deg2rad(-19.0)
    dx = xx - cx
    dy = yy - cy
    v = -dx * np.sin(ang) + dy * np.cos(ang)
    d = np.abs(v)

    core = np.exp(-((d / (h * 0.055)) ** 2))     # the bright ridge
    band = np.exp(-((d / (h * 0.13)) ** 2))      # the body of the band
    halo = np.exp(-((d / (h * 0.42)) ** 2))      # and its faint outskirts

    clump = warped_noise(rng, h, w, 6, base=3, warp=0.16)
    clump = np.clip((clump - 0.44) * 2.4, 0.0, 1.0)

    # the rift: a dark lane running along the middle of the band, which is
    # what actually divides the Milky Way in two from this angle
    rift = np.exp(-((np.abs(v - h * 0.012) / (h * 0.020)) ** 2))

    dust = warped_noise(rng, h, w, 5, base=4, warp=0.12)
    dust = np.clip((dust - 0.50) * 3.2, 0.0, 1.0)

    body = core * (0.30 + 1.45 * clump) + band * (0.22 + 1.05 * clump)
    body += halo * (0.12 + 0.22 * clump)
    body *= 1.0 - 0.74 * dust * np.maximum(band, 0.35)
    body *= 1.0 - 0.58 * rift * np.maximum(band, 0.2)
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
    winding = 5.2
    r0 = major * 0.055
    phase = phi - winding * np.log(r / r0)
    wrapped = np.mod(phase + np.pi, 2 * np.pi) - np.pi

    arms = np.zeros_like(r)
    for offset, weight in ((0.0, 1.0), (np.pi, 0.82)):
        dphi = np.mod(wrapped - offset + np.pi, 2 * np.pi) - np.pi
        # The arms have to stay narrow all the way out. Letting them widen
        # with radius merges them into concentric rings, and a galaxy made of
        # concentric rings is not a galaxy.
        width = 0.20 + 0.16 * np.clip(r / major, 0.0, 1.4)
        arms += weight * np.exp(-((dphi / width) ** 2)) * np.exp(-r / (major * 0.60))
    disk = disk + arms

    # the dust lane runs just inside each arm
    lane_phase = np.mod(phase + np.pi * 0.42 + np.pi, 2 * np.pi) - np.pi
    lane = np.exp(-((lane_phase / 0.22) ** 2)) * np.exp(-r / (major * 0.46))

    # star-forming knots: small, bright, blue, and only where the arms are.
    # The noise is warped, because an unwarp one at this scale leaves its grid
    # visible as rings around the galaxy.
    knot_noise = warped_noise(rng, h, w, 6, base=22, warp=0.22)
    knots = np.clip((knot_noise - 0.58) * 3.6, 0.0, 1.0) * np.clip(arms, 0.0, 2.0)

    # the bulge: old, warm stars piled up in the middle
    core = np.exp(-((r / (major * 0.052)) ** 1.7))

    light = (disk + core * 1.5) * (1.0 - 0.60 * lane)
    return light, knots, core


def nebulae(rng: np.random.Generator, h: int, w: int):
    """A few soft clouds of gas, in the page's own colours."""
    cloud = warped_noise(rng, h, w, 6, base=3, warp=0.30)
    detail = warped_noise(rng, h, w, 5, base=7, warp=0.18)
    shape = np.clip((cloud - 0.34) * 2.1, 0.0, 1.0) * (0.45 + 0.80 * detail)
    # filaments: the thin bright threads that run through a real nebula
    thread = np.clip((detail - 0.62) * 3.4, 0.0, 1.0) ** 2

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
        blobs.append(((shape + 0.55 * thread) * falloff,
                      np.array(tint, dtype=np.float32)))
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


def _psf(sigma: float, radius: int) -> np.ndarray:
    """A normalised Gaussian point-spread function over a square stencil."""
    grid = np.arange(-radius, radius + 1, dtype=np.float32)
    gy, gx = np.meshgrid(grid, grid, indexing="ij")
    k = np.exp(-((gx ** 2 + gy ** 2) / (2.0 * sigma * sigma))).astype(np.float32)
    return k / max(float(k.sum()), 1e-9)


def draw_stars(rng: np.random.Generator, img: np.ndarray, count: int,
               density: np.ndarray | None = None,
               sigma: float = 0.95,
               peak_base: float = 12.0,
               peak_span: float = 110.0,
               peak_exp: float = 0.85,
               flux_exp: float = 3.6,
               spikes: int = 0,
               spike_radius: int = 34) -> None:
    """
    Scatter stars over the field, in place.

    The brightness distribution is the thing that matters: a power law with a
    steep exponent, so that a handful of stars are bright and a great many are
    barely there. The faint ones sit close to the noise floor, which is exactly
    how a real field looks and is why the bright ones read as bright.

    `density` is a map of where stars are allowed to land. Without it the
    field is uniform and reads as noise; with it the sky has a crowded band
    and thin corners, which is what a real sky does.

    `sigma` is the width of the point-spread function in plate pixels, and the
    peak is the brightness of the star's centre pixel. Those two are separate:
    a wide point-spread function spreads the same light over more pixels, so
    the *total* light a star lays down is its peak divided by the fraction
    of the profile that lands on the centre pixel. Getting that wrong is how
    the bright tier ends up dimmer than the faint one — which is exactly what
    happened the first time this was written.

    The faint tier is drawn tight and dim, because faint stars should be grain.
    The bright tier is drawn wide and bright, because a wide star survives the
    browser's downscale as a point instead of being averaged into nothing.
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
    flux = u ** flux_exp
    flux = flux / max(flux.max(), 1e-6)

    colour = star_colours(rng, x.size)
    peak = peak_base + peak_span * (flux ** peak_exp)

    xi = np.round(x).astype(np.int32)
    yi = np.round(y).astype(np.int32)
    inside = (xi >= 0) & (xi < w) & (yi >= 0) & (yi < h)
    xi, yi = xi[inside], yi[inside]
    peak = peak[inside]
    colour = colour[inside]

    radius = max(2, int(np.ceil(sigma * 3.0)))
    kernel = _psf(sigma, radius)
    # the light the star lays down, so that its centre pixel comes out at
    # exactly `peak`
    w0 = max(float(kernel[radius, radius]), 1e-9)
    total = peak / w0

    flat = img.reshape(-1, 3)
    for dy in range(-radius, radius + 1):
        for dx in range(-radius, radius + 1):
            weight = kernel[dy + radius, dx + radius]
            if weight < 0.004 * w0:
                continue
            xs = xi + dx
            ys = yi + dy
            valid = (xs >= 0) & (xs < w) & (ys >= 0) & (ys < h)
            idx = (ys * w + xs)[valid]
            if idx.size == 0:
                continue
            for c in range(3):
                vals = (total * weight * colour[:, c])[valid]
                np.add.at(flat[:, c], idx, vals)

    if spikes <= 0:
        return

    # bloom and diffraction spikes on the brightest few. The spikes are long
    # and very thin: a short, wide one reads as a glow blob, and the whole
    # point of a spike is that it is a line.
    order = np.argsort(peak)[::-1]
    sigma_halo = sigma * 1.6
    grid = np.arange(-spike_radius, spike_radius + 1, dtype=np.float32)
    gy, gx = np.meshgrid(grid, grid, indexing="ij")
    halo = np.exp(-((gx ** 2 + gy ** 2) / (2.0 * sigma_halo * sigma_halo))).astype(np.float32)
    thin = 0.9
    spike = (np.exp(-(gy ** 2) / (2.0 * thin * thin))
             * np.exp(-(gx ** 2) / (2.0 * (sigma_halo * 0.85) ** 2))
             + np.exp(-(gx ** 2) / (2.0 * thin * thin))
             * np.exp(-(gy ** 2) / (2.0 * (sigma_halo * 0.85) ** 2)))

    for i in order[:spikes]:
        cx, cy = int(xi[i]), int(yi[i])
        x0, x1 = max(0, cx - spike_radius), min(w, cx + spike_radius + 1)
        y0, y1 = max(0, cy - spike_radius), min(h, cy + spike_radius + 1)
        if x1 <= x0 or y1 <= y0:
            continue
        sub_halo = halo[y0 - (cy - spike_radius):y1 - (cy - spike_radius),
                        x0 - (cx - spike_radius):x1 - (cx - spike_radius)]
        sub_spike = spike[y0 - (cy - spike_radius):y1 - (cy - spike_radius),
                          x0 - (cx - spike_radius):x1 - (cx - spike_radius)]
        weight = min(peak[i] / 255.0, 1.0)
        img[y0:y1, x0:x1] += (sub_halo * 0.42 + sub_spike * 0.26)[:, :, None] \
            * colour[i][None, None, :] * weight


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

    # the galactic band: unresolved starlight, warm and slightly dusty. It is
    # deliberately faint — the band has to read as a lane of dust and star
    # clouds lying behind the globe, not as a painted ribbon across the page.
    band_rgb = np.array([156.0, 150.0, 152.0], dtype=np.float32)
    sky += (band * 0.112)[:, :, None] * band_rgb[None, None, :]

    # the nebulae. They were there in the first version and could not be seen;
    # the sky was a monochrome field with a galaxy in it, and the gas has to
    # carry colour to belong to the same world as the globe.
    for shape, tint in clouds:
        sky += (shape * 0.125)[:, :, None] * tint[None, None, :]

    # andromeda: the disk is blue-white, the knots are bluer, the bulge warm
    sky += (light * 0.135)[:, :, None] * np.array([150.0, 168.0, 205.0], dtype=np.float32)[None, None, :]
    sky += (knots * 0.34)[:, :, None] * np.array([120.0, 170.0, 255.0], dtype=np.float32)[None, None, :]
    sky += (core * 0.22)[:, :, None] * np.array([255.0, 214.0, 168.0], dtype=np.float32)[None, None, :]

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

    # The faint tier: tight and dim. This is the grain of the field.
    draw_stars(rng, img, 46000, density,
               sigma=0.95, peak_base=10.0, peak_span=95.0,
               peak_exp=0.85, flux_exp=3.6)

    # The bright tier: wide enough to arrive at the screen as a point, and
    # bright enough to still be a point when it gets there. This is the tier
    # that was missing, and without it the sky had no stars in it — only dust.
    bright_density = np.clip(density * 0.60 + 0.12, 0.0, 1.0)
    draw_stars(rng, img, 1600, bright_density,
               sigma=STAR_TIER_WIDTH, peak_base=22.0, peak_span=233.0,
               peak_exp=0.62, flux_exp=2.6,
               spikes=240, spike_radius=40)

    # --- dither and quantise ---------------------------------------------
    dither = rng.random(img.shape).astype(np.float32) - 0.5
    img += dither * 1.35
    return Image.fromarray(np.clip(img, 0, 255).astype(np.uint8), "RGB")


def encode(im: Image.Image, path: str) -> None:
    if path.endswith(".webp"):
        im.save(path, "WEBP", quality=88, method=6)
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
