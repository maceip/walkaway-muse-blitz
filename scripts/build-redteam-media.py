#!/usr/bin/env python3
"""
Build the AI Red Team page's media: the orb scenes and the scene manifest.

The page can show the city-globe in any of the delivered looks — the daylight
sphere, the dusk sphere, the night city, or one of the camera moves that take
the camera into the city and back out — and which one it shows, and in what
order, is a build-time decision rather than a code change. This script is that
pipeline.

    videos/scenes.json          the config: which scenes exist, in what order,
                                which is the default
    videos/<file>               the source media, as delivered
        |
        |  this script
        v
    public/media/scenes/<id>.png    the scene, staged and masked to the globe
    public/media/scenes/<id>.webp   the same, lossy
    public/video/<id>.mp4           the scene as a film
    src/media/scenes.ts             the manifest the app imports
    media-hashes.json               what was built from what, for the drift check

Every scene is staged onto one canonical square — 1024x1024 — so that the
globe sits at the same size and in the same place in every one of them. That
is what makes the page able to cut from one scene to the next without the
globe jumping: the geometry is identical and only the colour and the camera
move. The stills are cut down to a mask of the globe itself, so what the page
carries is the globe and nothing around it.

Some films are transitions: a time-lapse that starts on one scene's picture
and carries the same city into a different light, the way a day turns into
night. A scene's `after` names the scenes a transition starts from — the page
goes into it from those, and from nowhere else — and `then` names the scene
it ends on, which the page always goes on to. Between the rest the page
drifts at random.

The films carry a storm: a bank of cloud lying across the top of the globe,
lit from inside by the city. The storm is not an object drawn on the frame,
it is light added on top of the sky, so it comes off by taking that light off
again — the cloud is found in the film's first frame, the sky it sits on is
diffused into the hole, and the repair is applied to every frame that still
carries the cloud. The camera moves do not carry it, so they are left alone.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile

try:
    from PIL import Image, ImageDraw
except ImportError:  # pragma: no cover
    print("Pillow is required: python3 -m pip install Pillow", file=sys.stderr)
    raise SystemExit(2)

try:
    import numpy as np
except ImportError:  # pragma: no cover
    print("numpy is required: python3 -m pip install numpy", file=sys.stderr)
    raise SystemExit(2)


ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = os.path.join(ROOT, "apps", "ai-red-team-web")
INBOX = os.path.join(ROOT, "videos")
CONFIG = os.path.join(INBOX, "scenes.json")
SCENE_MEDIA = os.path.join(APP, "public", "media", "scenes")
VIDEO_DIR = os.path.join(APP, "public", "video")
MANIFEST = os.path.join(APP, "src", "media", "scenes.ts")
HASHES = os.path.join(APP, "media-hashes.json")
OG = os.path.join(APP, "public", "media", "og-ai-red-team.jpg")

# The canonical stage. Every scene is built at this size, which is what makes
# "uniformly sized" true rather than aspirational: the page never has to
# resize one globe to match another.
CANVAS = 1024

# How long a still is held before the page moves on.
STILL_HOLD_MS = 8000

# How much longer than its own length a film is given, so that the page does
# not cross away from a film that has not quite finished.
FILM_GRACE_MS = 500

# The films. Every one of them is slowed to a third of the speed it was
# delivered at: the camera was flying, and a globe that is drifting is the
# point. The spin is also closed on itself, so its loop is seamless; the
# others are the delivered camera moves and run once.
FILMS = {
    "city-globe-spin": {"slow": 3.0},
    "city-dive": {"slow": 3.0},
    "city-edge": {"slow": 3.0},
    "city-zoom": {"slow": 3.0},
    "city-sealevel": {"slow": 3.0},
    "city-storm": {"slow": 3.0},
    # The owner's new clips are already smooth and slow, so they are slowed
    # less: at a third of their speed the dark beat in the middle of the scan
    # would sit on the page for three seconds, which reads as a fault rather
    # than as a cut.
    "city-scan": {"slow": 2.0},
    "city-globe-scan": {"slow": 2.0},
    "city-redteam": {"slow": 2.0},
}

CRF = 26

# The frame rate every film is delivered at. A delivered clip may be at any
# rate — one of the owner's is 60 fps — and the film that comes out of the
# pipeline is always at this one, so the page is not decoding two rates.
OUTPUT_RATE = 24


def _recipe() -> str:
    """A fingerprint of this generator, so that changing it rebuilds the media."""
    with open(os.path.abspath(__file__), "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()[:16]


RECIPE = _recipe()


# --------------------------------------------------------------------------
# helpers
# --------------------------------------------------------------------------

def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_config() -> dict:
    if not os.path.isfile(CONFIG):
        raise SystemExit("missing scene config: %s" % CONFIG)
    with open(CONFIG, encoding="utf-8") as fh:
        return json.load(fh)


def run(cmd: list[str]) -> None:
    subprocess.run(cmd, check=True)


def variant_key(source_sha: str, storm: bool, crop: float = 1.0) -> str:
    """
    The cache key for a build: the source, what was done to it, and this
    script itself.

    The recipe is part of the key because a change to the generator changes
    what the media should look like. Without it, a build after a change to
    this file would find the old outputs still matching their recorded hash
    and quietly keep them, and the page would be serving media this script
    no longer produces.
    """
    return hashlib.sha256(
        ("%s:%s:%s:%g" % (RECIPE, source_sha,
                          "storm" if storm else "plain", crop)).encode()
    ).hexdigest()


# --------------------------------------------------------------------------
# staging
# --------------------------------------------------------------------------

def crop_delivery_border(im: Image.Image) -> Image.Image:
    """
    Take the delivery border off the frame.

    The delivered frames carry a hairline border and a watermark in the
    corner. The border would show up as a scratch across the glass once the
    orb is clipped to a circle, because the circle touches the frame edge at
    the middle of each side. It is a couple of pixels; it is just removed.
    """
    cut = max(2, int(round(max(im.size) * 0.005)))
    w, h = im.size
    if w <= 2 * cut + 8 or h <= 2 * cut + 8:
        return im
    return im.crop((cut, cut, w - cut, h - cut))


def stage(im: Image.Image, crop: float = 1.0) -> Image.Image:
    """
    Put a frame on the canonical stage.

    Every delivered frame has the globe filling it, centred, with the same
    framing — the day sphere, the dusk sphere, the night city and the camera
    moves are all framed on the same globe. So staging is a matter of taking
    the border off and scaling to the canonical square, and every scene then
    carries the globe at the same size in the same place.
    """
    im = crop_delivery_border(im.convert("RGB"))
    if crop < 1.0:
        w, h = im.size
        cw, ch = int(round(w * crop)), int(round(h * crop))
        im = im.crop(((w - cw) // 2, (h - ch) // 2,
                      (w - cw) // 2 + cw, (h - ch) // 2 + ch))
    return im.resize((CANVAS, CANVAS), Image.LANCZOS)


def mask_to_globe(im: Image.Image) -> Image.Image:
    """
    Cut the frame down to the globe.

    The globe is the sphere, and the sphere fills the frame. So the globe is
    the circle inscribed in the square, and everything outside that circle is
    the background of the picture rather than part of it. Cutting it away
    leaves the globe and nothing else — which is what "globe only" means, and
    it also means the page is never showing a square photograph of a city
    with weather around the edges of it.
    """
    out = im.convert("RGBA")
    mask = Image.new("L", (CANVAS, CANVAS), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, CANVAS - 1, CANVAS - 1), fill=255)
    out.putalpha(mask)
    return out


def build_still(source: str, out_stem: str, storm: bool = False,
                crop: float = 1.0) -> list[tuple[str, str]]:
    """
    Write the still of a scene: the globe, masked, as a PNG and as a WebP.

    `storm` takes the storm off the picture first. Most of the delivered
    stills carry the same bank of cloud the films do, and the ones that do
    are marked in the scene config; the rest are clean as delivered and are
    left exactly as they were made.
    """
    frame = np.asarray(Image.open(source).convert("RGB"))
    if storm:
        hole, filled = storm_plan(frame)
        frame = repair_frame(frame, hole, filled)
    im = mask_to_globe(stage(Image.fromarray(frame), crop))

    png = out_stem + ".png"
    webp = out_stem + ".webp"
    im.save(png, "PNG", optimize=True)
    im.save(webp, "WEBP", quality=88, method=6)
    return [(png, sha256(png)), (webp, sha256(webp))]


# --------------------------------------------------------------------------
# the storm
# --------------------------------------------------------------------------

# A pixel belongs to the cloud when it is bright, washed out and smooth. The
# city's own surfaces are either saturated (neon) or sharp (edges), so they
# stay out of it; the bright flash at the top of the globe is washed out
# enough to be caught, which is what we want.
CLOUD_LO, CLOUD_HI, CLOUD_SAT = 78, 258, 78

# A pixel darker than this is plain sky. Only sky is allowed to diffuse into
# the hole the cloud leaves.
SKY_MAX = 85

# The cloud lies in the top of the frame. It is a fixed band because the
# camera is looking down on the city from outside.
CLOUD_BAND = 0.215

# How much of the cloud's own region still looks like cloud. In the wide view
# it is nearly all of it; once the camera is inside the city it is almost
# none of it, and the repair stops.
CLOUD_PRESENT_HI, CLOUD_PRESENT_LO = 0.55, 0.20


def _components(mask: np.ndarray, min_size: int = 200):
    """
    The connected regions of a mask, biggest first.

    Each region is returned as its own mask with the number of pixels in it
    and how much of its own bounding box it fills. The shape of a region is
    what tells a blob of lightning from a streak of the city's own light.
    """
    h, w = mask.shape
    seen = np.zeros_like(mask)
    found = []
    for sy, sx in zip(*np.nonzero(mask)):
        if seen[sy, sx]:
            continue
        stack = [(sy, sx)]
        seen[sy, sx] = True
        points = []
        while stack:
            y, x = stack.pop()
            points.append((y, x))
            for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                ny, nx = y + dy, x + dx
                if 0 <= ny < h and 0 <= nx < w and mask[ny, nx] and not seen[ny, nx]:
                    seen[ny, nx] = True
                    stack.append((ny, nx))
        if len(points) < min_size:
            continue
        ys = [p[0] for p in points]
        xs = [p[1] for p in points]
        box = (max(xs) - min(xs) + 1) * (max(ys) - min(ys) + 1)
        region = np.zeros_like(mask)
        region[ys, xs] = True
        found.append((region, len(points), len(points) / box))
    found.sort(key=lambda item: item[1], reverse=True)
    return found


# The lightning flash: a bright violet blob. The city's own magenta light is
# in streaks — long and thin — so the flash is told from the city's light by
# its shape: a region that fills a good part of its own bounding box is a
# blob, and a blob that is violet in the top of the frame is the flash.
FLASH_LO = 150
FLASH_MIN_AREA = 2500
FLASH_MIN_FILL = 0.30


def _flash(frame: np.ndarray) -> np.ndarray:
    """The lightning flash, where there is one."""
    a = frame.astype(int)
    mx = a.max(axis=2)
    r, g, b = a[:, :, 0], a[:, :, 1], a[:, :, 2]
    violet = (mx >= FLASH_LO) & (b > r + 25) & (r > g + 12)
    violet &= _cloud_region(frame)
    if not violet.any():
        return np.zeros_like(violet)
    out = np.zeros_like(violet)
    for region, area, fill in _components(violet):
        if area >= FLASH_MIN_AREA and fill >= FLASH_MIN_FILL:
            out |= region
    return out


def _cloud_like(frame: np.ndarray) -> np.ndarray:
    """The cloud, and the lightning flash with it."""
    a = frame.astype(int)
    mx = a.max(axis=2)
    sat = mx - a.min(axis=2)
    base = (mx >= CLOUD_LO) & (mx <= CLOUD_HI) & (sat <= CLOUD_SAT)
    return base | _flash(frame)


def _cloud_region(frame: np.ndarray) -> np.ndarray:
    """Where the cloud can be: the top band of the frame."""
    h = frame.shape[0]
    rows = np.arange(h)[:, None] * np.ones((1, frame.shape[1]), dtype=int)
    return rows < int(h * CLOUD_BAND)


def _sky_gradient(frame: np.ndarray, hole: np.ndarray) -> np.ndarray:
    """
    The sky the cloud is sitting on, one colour for each row.

    The sky of a night globe is a smooth gradient: dark at the top of the
    sphere, a little brighter where the city's own light reaches it. So the
    sky is taken row by row from the dark pixels of that row, in the middle of
    the frame where the globe is and the background is not, and the cloud's
    hole is filled with it.

    Taking it from the rows rather than from around the hole matters: the
    bright city sits directly below the cloud, and a fill that lets the city
    diffuse into the hole puts the colour of the street in the sky.
    """
    h, w = frame.shape[:2]
    src = frame.astype(np.float32)
    dark = (src.max(axis=2) < SKY_MAX) & (~hole)
    # the middle of the frame is the inside of the globe; the corners are the
    # background outside it, and are darker than any sky
    middle = np.abs(np.arange(w) - w / 2) < 0.32 * w
    dark &= middle[None, :]

    sky = np.zeros((h, 3), np.float32)
    counts = dark.sum(axis=1)
    for y in range(h):
        if counts[y] >= 5:
            sky[y] = np.median(src[y][dark[y]], axis=0)
    known = np.nonzero(counts >= 5)[0]
    if len(known):
        for y in range(h):
            if counts[y] < 5:
                sky[y] = sky[known[np.argmin(np.abs(known - y))]]
    else:
        sky[:] = np.median(src.reshape(-1, 3), axis=0)
    return np.repeat(sky[:, None, :], w, axis=1)


def storm_plan(first: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """
    Work out where the storm is in a film and what the sky behind it is.

    The plan is made once, from the film's first frame, which is a wide view
    of the globe — the one place the storm is visible. The hole is the cloud
    itself, and the fill is the sky that belongs in it.
    """
    hole = _cloud_like(first) & _cloud_region(first)
    if hole.sum() < 200:
        return hole, first.copy()
    filled = _sky_gradient(first, hole)
    return hole, filled


def repair_frame(frame: np.ndarray, hole: np.ndarray, filled: np.ndarray) -> np.ndarray:
    """
    Take the storm off one frame.

    A frame is repaired only where it still looks like cloud, and only as
    much as the frame carries cloud: a wide view is repaired all the way, and
    a frame taken from inside the city is not repaired at all, because by then
    the camera is below the cloud and there is nothing to take off.
    """
    region = hole
    if region.sum() == 0:
        return frame
    present = float((_cloud_like(frame) & region).sum()) / float(region.sum())
    gate = (present - CLOUD_PRESENT_LO) / (CLOUD_PRESENT_HI - CLOUD_PRESENT_LO)
    gate = float(min(max(gate, 0.0), 1.0))
    if gate <= 0.0:
        return frame
    paint = _cloud_like(frame) & region
    out = frame.astype(np.float32) * (1.0 - gate * paint[:, :, None].astype(np.float32))
    out += filled.astype(np.float32) * (gate * paint[:, :, None].astype(np.float32))
    return np.clip(out, 0, 255).astype(np.uint8)


# --------------------------------------------------------------------------
# films
# --------------------------------------------------------------------------

def probe_size(path: str) -> tuple[int, int]:
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0",
         "-show_entries", "stream=width,height", "-of", "csv=p=0", path],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    w, h = out.split(",")[:2]
    return int(w), int(h)


def probe_rate(path: str) -> float:
    """The frame rate of a clip, as a number."""
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0",
         "-show_entries", "stream=r_frame_rate", "-of", "csv=p=0", path],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    num, _, den = out.partition("/")
    return float(num) / float(den or 1)


def frame_count(path: str) -> int:
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-count_frames",
         "-select_streams", "v:0", "-show_entries", "stream=nb_read_frames",
         "-of", "default=nw=1:nk=1", path],
        capture_output=True, text=True, check=True,
    )
    return int(out.stdout.strip())


def decode_frames(path: str):
    """Yield the frames of a clip as numpy arrays, in order."""
    w, h = probe_size(path)
    proc = subprocess.Popen(
        ["ffmpeg", "-v", "error", "-i", path, "-f", "rawvideo",
         "-pix_fmt", "rgb24", "-"],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    size = w * h * 3
    whole = False
    try:
        while True:
            buf = proc.stdout.read(size)
            if buf is None or len(buf) < size:
                break
            yield np.frombuffer(buf, np.uint8).reshape(h, w, 3)
        whole = True
    finally:
        proc.stdout.close()
        if whole:
            err = proc.stderr.read().decode("utf-8", "replace")
            if proc.wait() != 0:
                raise SystemExit("ffmpeg could not read %s: %s" % (path, err.strip()))
        else:
            # the reader stopped early — the first frame is all it wanted —
            # so the decoder is killed rather than left complaining about a
            # pipe nobody is reading from
            proc.kill()
            proc.wait()
        proc.stderr.close()


def repair_film(source: str, target: str, slow: float, loop: bool,
                repair_storm: bool, crop: float = 1.0) -> None:
    """
    Write a film: the storm taken off, the globe staged on the canonical
    square, and the camera slowed to a third of the speed it was delivered at.

    The frames are read once, repaired, and piped straight into the encoder,
    so nothing large is held in memory.

    Slowing a camera move is not the same as playing it more slowly: the
    frames have to be interpolated up to the rate the slowed film needs, or
    the camera judders instead of gliding, and their timestamps have to be
    stretched to match, or the film is still the same length and simply
    drops frames.
    """
    if not shutil.which("ffmpeg"):
        raise SystemExit("ffmpeg is required to build the films")

    frames = decode_frames(source)
    try:
        first = next(frames)
    except StopIteration:
        raise SystemExit("no frames in %s" % source)
    if repair_storm:
        hole, filled = storm_plan(first)
    else:
        # the storm film keeps its storm: it is the storm
        hole = np.zeros_like(first[:, :, 0], dtype=bool)
        filled = first.copy()

    cut = "" if crop >= 1.0 else (
        "crop=iw*%g:ih*%g:(iw-ow)/2:(ih-oh)/2," % (crop, crop))
    src_rate = probe_rate(source)
    if slow == 1.0:
        chain = "[0:v]%sscale=%d:%d:flags=lanczos[out]" % (cut, CANVAS, CANVAS)
    elif loop:
        # The loop has to close. The delivered clip stops one frame short of
        # where it started, so a copy of the opening frames is appended and
        # the film is trimmed to end on it. The interpolation runs at twice
        # the rate the slowed film needs, because this is the one film whose
        # last frame has to sit exactly on its first.
        # The interpolation runs at twice the rate the slowed film needs,
        # because this is the one film whose last frame has to sit exactly on
        # its first. The rate comes from the clip itself: a delivered clip may
        # be at any rate, and interpolating at 24 when the clip is at 60 would
        # throw two frames out of every five.
        rate = int(round(src_rate * slow * 2))
        frames_total = frame_count(source)
        end = int(round(frames_total * slow * OUTPUT_RATE / src_rate)) + 1
        chain = (
            "[0:v]%ssplit=2[body][head];"
            "[head]trim=start_frame=0:end_frame=2,setpts=PTS-STARTPTS[first2];"
            "[body][first2]concat=n=2:v=1:a=0[ext];"
            "[ext]minterpolate=fps=%d:mi_mode=mci:mc_mode=aobmc:me_mode=bidir,"
            "setpts=%g*PTS,fps=%d,trim=end_frame=%d,setpts=PTS-STARTPTS[slowed];"
            "[slowed]scale=%d:%d:flags=lanczos[out]"
            % (cut, rate, slow, OUTPUT_RATE, end, CANVAS, CANVAS)
        )
    else:
        rate = int(round(src_rate * slow))
        chain = (
            "[0:v]%sminterpolate=fps=%d:mi_mode=mci:mc_mode=aobmc:me_mode=bidir,"
            "setpts=%g*PTS,fps=%d[slowed];"
            "[slowed]scale=%d:%d:flags=lanczos[out]"
            % (cut, rate, slow, OUTPUT_RATE, CANVAS, CANVAS)
        )

    cmd = [
        "ffmpeg", "-y", "-v", "error",
        "-f", "rawvideo", "-pix_fmt", "rgb24",
        "-s", "%dx%d" % (probe_size(source)[0], probe_size(source)[1]),
        "-r", "%g" % src_rate, "-i", "pipe:0",
        "-filter_complex", chain,
        "-map", "[out]",
        "-an",
        "-c:v", "libx264",
        "-crf", str(CRF),
        "-pix_fmt", "yuv420p",
        "-profile:v", "main",
        "-movflags", "+faststart",
        target,
    ]
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE)
    try:
        proc.stdin.write(repair_frame(first, hole, filled).tobytes())
        for frame in frames:
            proc.stdin.write(repair_frame(frame, hole, filled).tobytes())
    finally:
        proc.stdin.close()
        if proc.wait() != 0:
            raise SystemExit("ffmpeg failed writing %s" % target)


def build_film_poster(source: str, out_stem: str,
                      repair_storm: bool = True,
                      crop: float = 1.0) -> list[tuple[str, str]]:
    """
    The poster of a film is the film's own first frame, staged and masked.

    Taking the poster from the film rather than from another picture is what
    makes the cut from the film back to the next scene land: the poster is
    the same globe, in the same place, at the same size.
    """
    first = next(decode_frames(source))
    if repair_storm:
        hole, filled = storm_plan(first)
    else:
        hole = np.zeros_like(first[:, :, 0], dtype=bool)
        filled = first.copy()
    frame = repair_frame(first, hole, filled)
    im = mask_to_globe(stage(Image.fromarray(frame), crop))

    png = out_stem + ".png"
    webp = out_stem + ".webp"
    im.save(png, "PNG", optimize=True)
    im.save(webp, "WEBP", quality=88, method=6)
    return [(png, sha256(png)), (webp, sha256(webp))]


def film_duration_ms(path: str) -> int:
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration",
         "-of", "csv=p=0", path],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
    return int(round(float(out) * 1000))


# --------------------------------------------------------------------------
# manifest
# --------------------------------------------------------------------------

def write_manifest(config: dict, scenes: list[dict]) -> None:
    """Write the TypeScript manifest the app imports."""
    lines = [
        "// GENERATED by scripts/build-redteam-media.py — do not edit by hand.",
        "//",
        "// The page shows one of these scenes, and moves through them in this",
        "// order. Which one it opens on is decided by the `default` field in",
        "// videos/scenes.json, and any scene can be opened directly with",
        "// ?scene=<id>.",
        "",
        "export type SceneKind = 'image' | 'video' | 'alert';",
        "",
        "export interface Scene {",
        "  id: string;",
        "  kind: SceneKind;",
        "  label: string;",
        "  alt: string;",
        "  src: string;",
        "  poster: string;",
        "  posterWebp: string;",
        "  loop: boolean;",
        "  alert: boolean;",
        "  width: number;",
        "  height: number;",
        "  durationMs: number;",
        "  /** A transition is entered only from these scenes, and always from them. */",
        "  after: string[];",
        "  /** The scene a transition ends on, which always comes next; null to drift. */",
        "  then: string | null;",
        "}",
        "",
        "export const DEFAULT_SCENE_ID = %s;" % json.dumps(config["default"]),
        "",
        "export const SCENES: Scene[] = [",
    ]
    for scene in scenes:
        lines.append("  {")
        lines.append("    id: %s," % json.dumps(scene["id"]))
        lines.append("    kind: %s," % json.dumps(scene["kind"]))
        lines.append("    label: %s," % json.dumps(scene["label"]))
        lines.append("    alt: %s," % json.dumps(scene["alt"]))
        lines.append("    src: %s," % json.dumps(scene["src"]))
        lines.append("    poster: %s," % json.dumps(scene["poster"]))
        lines.append("    posterWebp: %s," % json.dumps(scene["posterWebp"]))
        lines.append("    loop: %s," % ("true" if scene["loop"] else "false"))
        lines.append("    alert: %s," % ("true" if scene["alert"] else "false"))
        lines.append("    width: %d," % scene["width"])
        lines.append("    height: %d," % scene["height"])
        lines.append("    durationMs: %d," % scene["durationMs"])
        lines.append("    after: %s," % json.dumps(scene["after"]))
        lines.append("    then: %s," % json.dumps(scene["then"]))
        lines.append("  },")
    lines.append("];")
    lines.append("")
    lines.append("export const SCENE_BY_ID: Record<string, Scene> = Object.fromEntries(")
    lines.append("  SCENES.map((scene) => [scene.id, scene]),")
    lines.append(");")
    lines.append("")

    os.makedirs(os.path.dirname(MANIFEST), exist_ok=True)
    with open(MANIFEST, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines))


def write_og(poster_png: str) -> str:
    """
    The social card: the globe as a disc, centred on black, with a red rule.
    """
    still = Image.open(poster_png).convert("RGBA")
    size = 600
    disc = still.resize((size, size), Image.LANCZOS)

    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).ellipse((0, 0, size - 1, size - 1), fill=255)
    disc.putalpha(mask)

    card = Image.new("RGB", (1200, 630), (0, 0, 0))
    card.paste(disc, ((1200 - size) // 2, 12), disc)
    for x in range(300, 900):
        for y in range(626, 629):
            card.putpixel((x, y), (255, 45, 60))
    card.save(OG, "JPEG", quality=88, optimize=True, progressive=True)
    return sha256(OG)


# --------------------------------------------------------------------------
# build
# --------------------------------------------------------------------------

def resolve_scenes(config: dict) -> list[dict]:
    """Work out what each scene ships, and where."""
    known = {scene["id"] for scene in config["scenes"]}
    resolved = []
    for scene in config["scenes"]:
        sid = scene["id"]
        after = list(scene.get("after", []))
        then = scene.get("then")
        for ref in after + ([then] if then else []):
            if ref not in known or ref == sid:
                raise SystemExit("scene %s names %r as a neighbour, which is not another scene" % (sid, ref))
        alert = scene["kind"] == "alert"
        if scene["kind"] == "image":
            src = "/media/scenes/%s.png" % sid
            poster_id = sid
            loop = False
        elif alert:
            # an alert scene is the city under the red alert light: it has no
            # media of its own, it borrows another scene's picture
            poster_id = scene.get("poster")
            if not poster_id:
                raise SystemExit("the alert scene %s names no poster" % sid)
            src = "/media/scenes/%s.png" % poster_id
            loop = False
        else:
            src = "/video/%s.mp4" % sid
            poster_id = sid
            loop = bool(scene.get("loop", False))
        poster = "/media/scenes/%s.png" % poster_id
        poster_webp = "/media/scenes/%s.webp" % poster_id
        duration = scene.get("holdMs", STILL_HOLD_MS)
        if scene["kind"] == "video":
            built = os.path.join(VIDEO_DIR, "%s.mp4" % sid)
            if os.path.isfile(built):
                # a little over the film's own length, so the page never
                # crosses away from a film that has not finished
                duration = film_duration_ms(built) + FILM_GRACE_MS
        resolved.append({
            "id": sid,
            "kind": scene["kind"],
            "label": scene.get("label", sid),
            "alt": scene["alt"],
            "src": src,
            "poster": poster,
            "posterWebp": poster_webp,
            "loop": loop,
            "alert": alert,
            "width": CANVAS,
            "height": CANVAS,
            "durationMs": int(duration),
            "after": after,
            "then": then,
        })
    return resolved


def build() -> int:
    config = load_config()
    os.makedirs(SCENE_MEDIA, exist_ok=True)
    os.makedirs(VIDEO_DIR, exist_ok=True)

    entries: dict[str, dict] = {}
    if os.path.isfile(HASHES):
        with open(HASHES, encoding="utf-8") as fh:
            entries = json.load(fh).get("entries", {})

    def cached(out_path: str, source_sha: str) -> bool:
        entry = entries.get(os.path.relpath(out_path, ROOT))
        if not entry or entry.get("source") != source_sha:
            return False
        if not os.path.isfile(out_path):
            return False
        return sha256(out_path) == entry.get("output")

    def record(out_path: str, source_sha: str) -> None:
        entries[os.path.relpath(out_path, ROOT)] = {
            "source": source_sha,
            "output": sha256(out_path),
        }

    # the stills first: a film's poster is a still of the film
    for scene in config["scenes"]:
        if scene["kind"] != "image":
            continue
        source = os.path.join(INBOX, scene["source"])
        if not os.path.isfile(source):
            raise SystemExit("missing scene source: %s" % source)
        storm = bool(scene.get("repair_storm", False))
        crop = float(scene.get("crop", 1.0))
        source_sha = variant_key(sha256(source), storm, crop)
        stem = os.path.join(SCENE_MEDIA, scene["id"])
        outputs = [stem + ".png", stem + ".webp"]
        if all(cached(p, source_sha) for p in outputs):
            print("  %s: up to date" % scene["id"])
            continue
        for path, _ in build_still(source, stem, storm, crop):
            record(path, source_sha)
            print("  wrote %s (%d bytes)" % (
                os.path.relpath(path, ROOT), os.path.getsize(path)))

    # the films, and the still of each film's first frame
    for scene in config["scenes"]:
        if scene["kind"] != "video":
            continue
        source = os.path.join(INBOX, scene["source"])
        if not os.path.isfile(source):
            raise SystemExit("missing scene source: %s" % source)
        storm = bool(scene.get("repair_storm", False))
        crop = float(scene.get("crop", 1.0))
        source_sha = variant_key(sha256(source), storm, crop)
        target = os.path.join(VIDEO_DIR, "%s.mp4" % scene["id"])
        stem = os.path.join(SCENE_MEDIA, scene["id"])
        outputs = [target, stem + ".png", stem + ".webp"]
        if all(cached(p, source_sha) for p in outputs):
            print("  %s: up to date" % scene["id"])
            continue
        film = FILMS.get(scene["id"], {"slow": 1.0})
        repair_film(source, target, film["slow"], bool(scene.get("loop", False)),
                    storm, crop)
        record(target, source_sha)
        print("  wrote %s (%d bytes)" % (
            os.path.relpath(target, ROOT), os.path.getsize(target)))
        for path, _ in build_film_poster(source, stem, storm, crop):
            record(path, source_sha)
            print("  wrote %s (%d bytes)" % (
                os.path.relpath(path, ROOT), os.path.getsize(path)))

    # the manifest and the social card
    resolved = resolve_scenes(config)
    write_manifest(config, resolved)
    print("  wrote %s" % os.path.relpath(MANIFEST, ROOT))

    # the social card is cheap and derived from the default scene, so it is
    # always rewritten rather than cached
    default_scene = next(s for s in config["scenes"] if s["id"] == config["default"])
    poster_png = os.path.join(SCENE_MEDIA, "%s.png" % default_scene["id"])
    if os.path.isfile(poster_png):
        write_og(poster_png)
        record(OG, sha256(OG))
        print("  wrote %s" % os.path.relpath(OG, ROOT))

    with open(HASHES, "w", encoding="utf-8") as fh:
        json.dump({"version": 1, "entries": entries}, fh, indent=2, sort_keys=True)
        fh.write("\n")

    prune_stale_output()
    return 0


def prune_stale_output() -> None:
    """
    Remove media the pipeline used to own and no longer does.

    The pipeline is the only writer of `public/video/` and
    `public/media/scenes/`. A file in either directory that is not one of the
    current scenes was left behind by an earlier run of the pipeline, so it is
    cleared out rather than shipped.
    """
    config = load_config()
    keep = {"%s.mp4" % s["id"] for s in config["scenes"] if s["kind"] == "video"}
    for name in sorted(os.listdir(VIDEO_DIR)):
        if name.endswith(".mp4") and name not in keep:
            path = os.path.join(VIDEO_DIR, name)
            os.remove(path)
            print("  removed stale %s" % os.path.relpath(path, ROOT))

    keep_stills = {"%s.%s" % (s["id"], ext)
                   for s in config["scenes"]
                   for ext in ("png", "webp")}
    for name in sorted(os.listdir(SCENE_MEDIA)):
        if name not in keep_stills:
            path = os.path.join(SCENE_MEDIA, name)
            os.remove(path)
            print("  removed stale %s" % os.path.relpath(path, ROOT))


# --------------------------------------------------------------------------
# check
# --------------------------------------------------------------------------

def check() -> int:
    config = load_config()
    bad: list[str] = []

    if not os.path.isfile(HASHES):
        print("FAIL missing %s" % os.path.relpath(HASHES, ROOT))
        return 1
    with open(HASHES, encoding="utf-8") as fh:
        entries = json.load(fh).get("entries", {})

    # every still must be exactly what this script derives from its source
    for scene in config["scenes"]:
        if scene["kind"] != "image":
            continue
        source = os.path.join(INBOX, scene["source"])
        if not os.path.isfile(source):
            bad.append("missing source %s" % scene["source"])
            continue
        with tempfile.TemporaryDirectory() as tmp:
            stem = os.path.join(tmp, scene["id"])
            build_still(source, stem, bool(scene.get("repair_storm", False)),
                        float(scene.get("crop", 1.0)))
            for ext in (".png", ".webp"):
                shipped = os.path.join(SCENE_MEDIA, scene["id"] + ext)
                if not os.path.isfile(shipped):
                    bad.append("missing %s" % os.path.relpath(shipped, ROOT))
                elif sha256(shipped) != sha256(stem + ext):
                    bad.append("stale %s" % os.path.relpath(shipped, ROOT))

    # every film must still be the one that was built from its source
    for scene in config["scenes"]:
        if scene["kind"] != "video":
            continue
        source = os.path.join(INBOX, scene["source"])
        target = os.path.join(VIDEO_DIR, "%s.mp4" % scene["id"])
        if not os.path.isfile(source):
            bad.append("missing source %s" % scene["source"])
            continue
        if not os.path.isfile(target):
            bad.append("missing %s" % os.path.relpath(target, ROOT))
            continue
        entry = entries.get(os.path.relpath(target, ROOT))
        if not entry:
            bad.append("no recorded hash for %s" % os.path.relpath(target, ROOT))
            continue
        if entry.get("source") != variant_key(sha256(source),
                                              bool(scene.get("repair_storm", False)),
                                              float(scene.get("crop", 1.0))):
            bad.append("source changed for %s" % scene["id"])
        elif entry.get("output") != sha256(target):
            bad.append("stale %s" % os.path.relpath(target, ROOT))

    # every scene must be on the same stage
    for scene in config["scenes"]:
        still = os.path.join(SCENE_MEDIA, "%s.png" % scene["id"])
        if not os.path.isfile(still):
            continue
        with Image.open(still) as im:
            if im.size != (CANVAS, CANVAS):
                bad.append("%s is %dx%d, not %dx%d" % (
                    scene["id"], im.size[0], im.size[1], CANVAS, CANVAS))

    # the manifest must name every scene and exist
    if not os.path.isfile(MANIFEST):
        bad.append("missing %s" % os.path.relpath(MANIFEST, ROOT))
    else:
        manifest = open(MANIFEST, encoding="utf-8").read()
        for scene in config["scenes"]:
            if '"%s"' % scene["id"] not in manifest:
                bad.append("manifest does not mention %s" % scene["id"])

    if bad:
        for b in bad:
            print("FAIL " + b)
        return 1
    print("PASS media matches scripts/build-redteam-media.py")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="verify the shipped media matches this generator")
    args = ap.parse_args()
    if args.check:
        return check()
    return build()


if __name__ == "__main__":
    raise SystemExit(main())
