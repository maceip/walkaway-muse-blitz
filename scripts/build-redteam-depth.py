#!/usr/bin/env python3
"""
Build depth for the AI Red Team page's orb scenes.

The globe's lens (apps/ai-red-team-web/src/lib/orbCompositor.ts) uses depth
to make the city read as a real object: when the globe wobbles the towers
shift against the ground (parallax), and the focus pull between scenes is
focused at a depth instead of blurring everything evenly.

Depth is estimated from the media the page already ships, so this runs after
scripts/build-redteam-media.py and never changes what that script made:

    public/media/scenes/<id>.png        a still (or a film's first frame)
    public/video/<id>.mp4               a film
        |
        |  this script: Depth Anything V2 Small, per frame
        v
    public/media/depth/<id>.webp        the still's depth, 512x512 grey
    public/media/depth/<id>.mp4         the film, packed: colour | depth
    src/media/depth.ts                  which scenes have depth (GENERATED)
    depth-hashes.json                   what was built from what

**Why films are packed.** A film's depth has to stay on the same frame as
the film, and two <video> elements never stay exactly together. So each film
ships once, 1536x1024: the colour on the left at its full 1024x1024, and the
depth in the 512x512 square at the top right. The page plays that one film;
without WebGL it shows only the left square (object-fit: cover, positioned
left), so nothing changes for a browser that cannot use the depth.

**Why Small.** Depth Anything V2 Small is Apache-2.0. Base and Large are
CC-BY-NC-4.0 — non-commercial — and are not used.

**Steadiness.** Depth from one frame at a time flickers. The film's depth is
normalised with a range that is smoothed over about a second, and each
frame's depth is averaged with its neighbours (two either side, centred, so
nothing lags); a looping film wraps round so the loop has no seam. Nearer
surfaces are dilated a little before the depth is softened, so a tower's
edge moves with the tower and not with the street behind it.

    python3 scripts/build-redteam-depth.py           # build what is out of date
    python3 scripts/build-redteam-depth.py --check   # verify; needs no model

Building needs torch and transformers (e.g. a venv with both) and the model
weights in the Hugging Face cache; --check needs neither.
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
    import numpy as np
    from PIL import Image
except ImportError:  # pragma: no cover
    print("numpy and Pillow are required", file=sys.stderr)
    raise SystemExit(2)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = os.path.join(ROOT, "apps", "ai-red-team-web")
CONFIG = os.path.join(ROOT, "videos", "scenes.json")
SCENE_MEDIA = os.path.join(APP, "public", "media", "scenes")
VIDEO_DIR = os.path.join(APP, "public", "video")
DEPTH_DIR = os.path.join(APP, "public", "media", "depth")
MODULE = os.path.join(APP, "src", "media", "depth.ts")
HASHES = os.path.join(APP, "depth-hashes.json")

MODEL = "depth-anything/Depth-Anything-V2-Small-hf"
MODEL_REVISION = "5426e4f0f36572d16453bbda7a8389317b1bef99"

# The colour square and the depth square of a packed film.
COLOR = 1024
DEPTH = 512
FPS = 24
# Packed films are re-encoded once from the shipped film. At this quality the
# colour half measures ~45 dB PSNR against the shipped film: indistinguishable.
CRF = 23
# How far, in frames, the film's normalisation range is smoothed.
RANGE_SIGMA = 12.0
# Weights of the centred temporal average of the depth itself.
TEMPORAL = (1.0, 2.0, 3.0, 2.0, 1.0)


def _recipe() -> str:
    with open(os.path.abspath(__file__), "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()[:16]


RECIPE = _recipe()


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def load_config() -> dict:
    with open(CONFIG, encoding="utf-8") as fh:
        return json.load(fh)


def source_key(media_path: str) -> str:
    """The cache key: the shipped media, this script, and the model."""
    return hashlib.sha256(
        ("%s:%s:%s@%s" % (RECIPE, sha256(media_path), MODEL, MODEL_REVISION)).encode()
    ).hexdigest()


# --------------------------------------------------------------------------
# what has depth
# --------------------------------------------------------------------------

def plan(config: dict) -> list[dict]:
    """
    Every piece of depth this script makes: one per still and per film
    poster, and one packed film per film.
    """
    jobs = []
    for scene in config["scenes"]:
        sid = scene["id"]
        if scene["kind"] in ("image", "video"):
            jobs.append({
                "kind": "still",
                "id": sid,
                "media": os.path.join(SCENE_MEDIA, "%s.png" % sid),
                "out": os.path.join(DEPTH_DIR, "%s.webp" % sid),
            })
        if scene["kind"] == "video":
            jobs.append({
                "kind": "film",
                "id": sid,
                "media": os.path.join(VIDEO_DIR, "%s.mp4" % sid),
                "out": os.path.join(DEPTH_DIR, "%s.mp4" % sid),
                "loop": bool(scene.get("loop", False)),
            })
    return jobs


def scene_depth(config: dict) -> dict[str, dict]:
    """What each scene uses: its own still depth, a borrowed one, a film."""
    out: dict[str, dict] = {}
    for scene in config["scenes"]:
        sid = scene["id"]
        entry: dict = {}
        if scene["kind"] == "image":
            entry["still"] = "/media/depth/%s.webp" % sid
        elif scene["kind"] == "alert":
            entry["still"] = "/media/depth/%s.webp" % scene["poster"]
        elif scene["kind"] == "video":
            entry["still"] = "/media/depth/%s.webp" % sid
            entry["film"] = "/media/depth/%s.mp4" % sid
        out[sid] = entry
    return out


# --------------------------------------------------------------------------
# the model
# --------------------------------------------------------------------------

class Estimator:
    def __init__(self) -> None:
        import torch  # noqa: WPS433 — only the build needs the model
        from transformers import AutoImageProcessor, AutoModelForDepthEstimation

        self.torch = torch
        self.device = "mps" if torch.backends.mps.is_available() else (
            "cuda" if torch.cuda.is_available() else "cpu")
        self.proc = AutoImageProcessor.from_pretrained(MODEL, revision=MODEL_REVISION)
        self.model = AutoModelForDepthEstimation.from_pretrained(
            MODEL, revision=MODEL_REVISION).to(self.device).eval()

    def raw(self, rgb: np.ndarray) -> np.ndarray:
        """Relative inverse depth (bigger is nearer) at DEPTH x DEPTH, float32."""
        torch = self.torch
        with torch.no_grad():
            x = self.proc(images=Image.fromarray(rgb), return_tensors="pt").to(self.device)
            d = self.model(**x).predicted_depth[:, None]
            d = torch.nn.functional.interpolate(d, size=(DEPTH, DEPTH), mode="bicubic",
                                                align_corners=False)
        return d[0, 0].float().cpu().numpy()

    def finish(self, d: np.ndarray) -> np.ndarray:
        """
        Normalised depth in, softened depth out: near surfaces dilated by a
        couple of pixels so silhouettes move with what is in front, then a
        light blur so the parallax never tears.
        """
        torch = self.torch
        t = torch.from_numpy(np.ascontiguousarray(d, dtype=np.float32))[None, None]
        t = torch.nn.functional.max_pool2d(t, kernel_size=5, stride=1, padding=2)
        k = torch.tensor([1.0, 4.0, 6.0, 4.0, 1.0])
        k = (k / k.sum()).view(1, 1, 1, 5)
        t = torch.nn.functional.conv2d(torch.nn.functional.pad(t, (2, 2, 0, 0), mode="replicate"), k)
        t = torch.nn.functional.conv2d(torch.nn.functional.pad(t, (0, 0, 2, 2), mode="replicate"),
                                       k.view(1, 1, 5, 1))
        return t[0, 0].clamp(0, 1).numpy()


def normalise(d: np.ndarray, lo: float, hi: float) -> np.ndarray:
    return np.clip((d - lo) / max(hi - lo, 1e-6), 0.0, 1.0)


def _inside() -> np.ndarray:
    y, x = np.mgrid[0:DEPTH, 0:DEPTH]
    c = (DEPTH - 1) / 2.0
    return np.hypot(x - c, y - c) <= 0.88 * DEPTH / 2.0


# The range is measured inside the globe only. The model sees the glass ball
# as one near object against far corners; measured over the whole frame, the
# city inside gets a sliver of the range, and at street level almost none.
# Measured inside, the city gets all of it. The glass beyond is clipped, and
# the lens fades the parallax out over it anyway.
INSIDE = _inside()


def depth_range(d: np.ndarray) -> tuple[float, float]:
    v = d[INSIDE]
    return float(np.percentile(v, 2)), float(np.percentile(v, 98))


# --------------------------------------------------------------------------
# stills
# --------------------------------------------------------------------------

def build_still(est: Estimator, media: str, out: str) -> None:
    rgb = np.asarray(Image.open(media).convert("RGB"))
    d = est.raw(rgb)
    lo, hi = depth_range(d)
    d = est.finish(normalise(d, lo, hi))
    Image.fromarray((d * 255 + 0.5).astype(np.uint8), "L").save(out, "WEBP", lossless=True)


# --------------------------------------------------------------------------
# films
# --------------------------------------------------------------------------

def frames_of(path: str):
    """Decode a film to RGB frames, one at a time."""
    cmd = ["ffmpeg", "-v", "error", "-i", path, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE)
    size = COLOR * COLOR * 3
    assert proc.stdout is not None
    while True:
        buf = proc.stdout.read(size)
        if len(buf) < size:
            break
        yield np.frombuffer(buf, dtype=np.uint8).reshape(COLOR, COLOR, 3)
    proc.wait()


def smooth_series(values: np.ndarray, sigma: float, wrap: bool) -> np.ndarray:
    radius = int(sigma * 3)
    k = np.exp(-0.5 * (np.arange(-radius, radius + 1) / sigma) ** 2)
    k /= k.sum()
    mode = "wrap" if wrap else "edge"
    padded = np.pad(values, radius, mode=mode)
    return np.convolve(padded, k, mode="valid")


def build_film(est: Estimator, media: str, out: str, loop: bool) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        raw_path = os.path.join(tmp, "raw.npy")
        count = int(subprocess.check_output([
            "ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0",
            "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0", media,
        ]).decode().strip())
        raw = np.lib.format.open_memmap(raw_path, mode="w+", dtype=np.float16,
                                        shape=(count, DEPTH, DEPTH))
        lows = np.zeros(count, dtype=np.float64)
        highs = np.zeros(count, dtype=np.float64)
        n = 0
        for i, rgb in enumerate(frames_of(media)):
            if i >= count:
                break
            d = est.raw(rgb)
            raw[i] = d.astype(np.float16)
            lows[i], highs[i] = depth_range(d)
            n = i + 1
            if n % 100 == 0:
                print("    %s: %d/%d frames" % (os.path.basename(media), n, count), flush=True)
        if n != count:
            raise SystemExit("decoded %d frames of %d from %s" % (n, count, media))

        # one steady range for the film, so depth does not pump
        lows = smooth_series(lows, RANGE_SIGMA, loop)
        highs = smooth_series(highs, RANGE_SIGMA, loop)

        depth_path = os.path.join(tmp, "depth.gray")
        w = np.array(TEMPORAL)
        w = w / w.sum()
        half = len(TEMPORAL) // 2
        with open(depth_path, "wb") as fh:
            for i in range(count):
                acc = np.zeros((DEPTH, DEPTH), dtype=np.float32)
                for j, weight in enumerate(w):
                    k = i + j - half
                    k = k % count if loop else min(max(k, 0), count - 1)
                    acc += weight * normalise(raw[k].astype(np.float32), lows[k], highs[k])
                d = est.finish(acc)
                fh.write((d * 255 + 0.5).astype(np.uint8).tobytes())

        tmp_out = os.path.join(tmp, "packed.mp4")
        subprocess.run([
            "ffmpeg", "-v", "error", "-y",
            "-i", media,
            "-f", "rawvideo", "-pix_fmt", "gray", "-s", "%dx%d" % (DEPTH, DEPTH),
            "-r", str(FPS), "-i", depth_path,
            "-filter_complex",
            "[1:v]pad=%d:%d:0:0:black[d];[0:v][d]hstack=inputs=2:shortest=1,format=yuv420p[o]"
            % (DEPTH, COLOR),
            "-map", "[o]", "-c:v", "libx264", "-preset", "slow", "-crf", str(CRF),
            "-r", str(FPS), "-an", "-movflags", "+faststart", tmp_out,
        ], check=True)
        packed = int(subprocess.check_output([
            "ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0",
            "-show_entries", "stream=nb_read_frames", "-of", "csv=p=0", tmp_out,
        ]).decode().strip())
        if packed != count:
            raise SystemExit("packed film has %d frames, the film has %d" % (packed, count))
        shutil.move(tmp_out, out)


# --------------------------------------------------------------------------
# the module
# --------------------------------------------------------------------------

def write_module(config: dict) -> None:
    depth = scene_depth(config)
    lines = [
        "// GENERATED by scripts/build-redteam-depth.py — do not edit by hand.",
        "//",
        "// Depth for each orb scene, estimated from the shipped media with",
        "// %s (Apache-2.0)." % MODEL,
        "// `still` is a 512x512 grey depth picture for the scene's still (or a",
        "// film's first frame). `film` is the film packed with its depth:",
        "// 1536x1024, the colour square on the left and the depth square at",
        "// the top right.",
        "",
        "export interface SceneDepth {",
        "  still?: string;",
        "  film?: string;",
        "}",
        "",
        "export const DEPTH_MODEL = %s;" % json.dumps("%s@%s" % (MODEL, MODEL_REVISION[:7])),
        "",
        "/** The packed film's layout, in pixels. */",
        "export const PACKED = { color: %d, depth: %d, width: %d, height: %d } as const;"
        % (COLOR, DEPTH, COLOR + DEPTH, COLOR),
        "",
        "export const SCENE_DEPTH: Record<string, SceneDepth> = {",
    ]
    for sid, entry in depth.items():
        parts = ", ".join("%s: %s" % (k, json.dumps(v)) for k, v in entry.items())
        lines.append("  %s: { %s }," % (json.dumps(sid), parts))
    lines += ["};", ""]
    with open(MODULE, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines))


# --------------------------------------------------------------------------
# build / check
# --------------------------------------------------------------------------

def build() -> int:
    config = load_config()
    os.makedirs(DEPTH_DIR, exist_ok=True)
    entries: dict = {}
    if os.path.isfile(HASHES):
        with open(HASHES, encoding="utf-8") as fh:
            entries = json.load(fh).get("entries", {})

    est: Estimator | None = None
    for job in plan(config):
        if not os.path.isfile(job["media"]):
            raise SystemExit("missing %s — run scripts/build-redteam-media.py first" % job["media"])
        rel = os.path.relpath(job["out"], ROOT)
        key = source_key(job["media"])
        entry = entries.get(rel)
        if (entry and entry.get("source") == key and os.path.isfile(job["out"])
                and sha256(job["out"]) == entry.get("output")):
            print("  %s: up to date" % rel)
            continue
        if est is None:
            est = Estimator()
            print("  model %s on %s" % (MODEL, est.device))
        if job["kind"] == "still":
            build_still(est, job["media"], job["out"])
        else:
            build_film(est, job["media"], job["out"], job["loop"])
        entries[rel] = {"source": key, "output": sha256(job["out"])}
        print("  wrote %s (%d bytes)" % (rel, os.path.getsize(job["out"])))
        with open(HASHES, "w", encoding="utf-8") as fh:
            json.dump({"version": 1, "entries": entries}, fh, indent=2, sort_keys=True)
            fh.write("\n")

    # anything in the depth directory that is not one of the current jobs is stale
    keep = {os.path.basename(j["out"]) for j in plan(config)}
    for name in sorted(os.listdir(DEPTH_DIR)):
        if name not in keep:
            os.remove(os.path.join(DEPTH_DIR, name))
            entries.pop(os.path.relpath(os.path.join(DEPTH_DIR, name), ROOT), None)
            print("  removed stale %s" % name)

    with open(HASHES, "w", encoding="utf-8") as fh:
        json.dump({"version": 1, "entries": entries}, fh, indent=2, sort_keys=True)
        fh.write("\n")
    write_module(config)
    print("  wrote %s" % os.path.relpath(MODULE, ROOT))
    return 0


def check() -> int:
    """Every depth file is the one built from the media the page ships now."""
    config = load_config()
    bad = []
    if not os.path.isfile(HASHES):
        print("FAIL missing %s" % os.path.relpath(HASHES, ROOT))
        return 1
    with open(HASHES, encoding="utf-8") as fh:
        entries = json.load(fh).get("entries", {})
    for job in plan(config):
        rel = os.path.relpath(job["out"], ROOT)
        entry = entries.get(rel)
        if not os.path.isfile(job["out"]):
            bad.append("missing %s" % rel)
        elif not entry:
            bad.append("no recorded hash for %s" % rel)
        elif not os.path.isfile(job["media"]) or entry.get("source") != source_key(job["media"]):
            bad.append("%s is stale: its media changed — rebuild depth" % rel)
        elif entry.get("output") != sha256(job["out"]):
            bad.append("%s was changed by hand" % rel)
    if not os.path.isfile(MODULE):
        bad.append("missing %s" % os.path.relpath(MODULE, ROOT))
    else:
        text = open(MODULE, encoding="utf-8").read()
        for scene in config["scenes"]:
            if json.dumps(scene["id"]) not in text:
                bad.append("depth module does not mention %s" % scene["id"])
    if bad:
        for b in bad:
            print("FAIL " + b)
        return 1
    print("PASS depth matches the shipped media and scripts/build-redteam-depth.py")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="verify the depth matches the shipped media (no model needed)")
    args = ap.parse_args()
    return check() if args.check else build()


if __name__ == "__main__":
    raise SystemExit(main())
