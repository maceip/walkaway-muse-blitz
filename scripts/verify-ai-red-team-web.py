#!/usr/bin/env python3
"""
Integrity check for the AI Red Team web app (apps/ai-red-team-web).

Verifies, without a browser:
  1. The Vite build is present and every local asset the page references
     (fonts, media, video) exists on disk and is not empty.
  2. Every @font-face URL in the stylesheet resolves to a real woff2.
  3. The page makes no external network requests: every src/href in the
     document is relative, a mailto:, or a data: URI.
  4. The brand block is exactly what it must be: an anchor whose href is
     mailto:contact@model.tools, whose text is "AI RED TEAM", set in the
     Michroma face, with "San Francisco, CA" beneath it.
  5. The exercise schedule in src/lib/schedule.ts really is one drill every
     four minutes, and the drill is bracketed by a warning and a blue settle.
  6. The scene pipeline holds together: the config, the generated manifest
     and the files on disk agree, and there is both a still scene and a film
     scene so the page can switch between them.
  7. The shipped orb media is the media scripts/build-redteam-media.py
     produces, and the sky is the plate scripts/build-space-backdrop.py
     draws — and the sky still has stars in it once the browser has scaled
     it down to a screen.
  8. The page carries no canvas: the falling specks that read as snowfall
     are gone, and the sky is the drawn plate alone.
  9. The production server rejects path traversal and answers /healthz.

Usage:
    python3 scripts/verify-ai-red-team-web.py

Exit code 0 = all checks pass. Exit code 1 = at least one failure.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys

import numpy as np
from PIL import Image, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APP = os.path.join(ROOT, "apps", "ai-red-team-web")
INDEX = os.path.join(APP, "index.html")
CSS = os.path.join(APP, "src", "styles.css")
SCHEDULE = os.path.join(APP, "src", "lib", "schedule.ts")
PUBLIC = os.path.join(APP, "public")
SERVER = os.path.join(APP, "server", "index.mjs")

MAILTO = "mailto:contact@model.tools"
BRAND_TEXT = "AI RED TEAM"
PLACE_TEXT = "San Francisco, CA"
DISPLAY = "Michroma"

failures: list[str] = []


def fail(msg: str) -> None:
    failures.append(msg)


def shipped_document() -> str:
    """
    The document under test is the built one when it exists: it is what the
    server actually serves, and Vite has already rewritten the module entry
    point into a hashed asset. The source index.html is only used before the
    first build.
    """
    built = os.path.join(APP, "dist", "index.html")
    if os.path.exists(built):
        return built
    return INDEX


def check_local_references() -> int:
    """Every asset the shipped page references must exist and be non-empty."""
    doc = open(shipped_document(), encoding="utf-8").read()
    refs = set(re.findall(r'(?:src|href)="([^"#][^"]*)"', doc))
    checked = 0
    for ref in sorted(refs):
        if ref.startswith(("http://", "https://", "mailto:", "data:", "//")):
            continue
        path = ref.lstrip("/")
        candidates = [os.path.join(PUBLIC, path), os.path.join(APP, "dist", path)]
        for candidate in candidates:
            if os.path.exists(candidate):
                if os.path.getsize(candidate) == 0:
                    fail(f"empty asset: {ref}")
                checked += 1
                break
        else:
            fail(f"missing local reference: {ref}")
    return checked


def check_no_external_requests() -> int:
    """
    No subresource may point off-origin.

    A canonical link and an og:url declare an address rather than request
    one, so they are allowed to be absolute; everything else that the
    browser would fetch has to be same-origin.
    """
    doc = open(shipped_document(), encoding="utf-8").read()
    bad = 0
    for tag in re.findall(r"<(?:link|script|img|source|iframe|video|audio)\b[^>]*>", doc):
        if re.search(r'rel="(?:canonical|alternate)"', tag):
            continue
        for ref in re.findall(r'(?:src|href)="([^"]*)"', tag):
            if ref.startswith(("http://", "https://", "//")):
                fail(f"external request in the document: {ref}")
                bad += 1
    for url in re.findall(r"url\(['\"]?([^'\")]+)", open(CSS, encoding="utf-8").read()):
        if url.startswith(("http://", "https://", "//")):
            fail(f"external request in the stylesheet: {url}")
            bad += 1
    return bad


def check_fonts() -> int:
    """Every @font-face URL must resolve to a real, non-empty woff2."""
    css = open(CSS, encoding="utf-8").read()
    urls = re.findall(r"url\(['\"]?([^'\")]+)", css)
    checked = 0
    for url in urls:
        if not url.endswith(".woff2"):
            continue
        path = url.lstrip("/")
        target = os.path.join(PUBLIC, path)
        if not os.path.exists(target):
            fail(f"font not found: {url}")
            continue
        if os.path.getsize(target) == 0:
            fail(f"empty font: {url}")
            continue
        checked += 1
    return checked


def check_brand() -> int:
    """The wordmark must be exactly the mail link, in the display face."""
    app = open(os.path.join(APP, "src", "components", "App.tsx"), encoding="utf-8").read()
    css = open(CSS, encoding="utf-8").read()
    checked = 0

    if MAILTO not in app:
        fail(f"brand link must point at {MAILTO}")
    else:
        checked += 1
    if BRAND_TEXT not in app:
        fail(f'brand text must be "{BRAND_TEXT}"')
    else:
        checked += 1
    if PLACE_TEXT not in app:
        fail(f'place line must be "{PLACE_TEXT}"')
    else:
        checked += 1
    if f"'{DISPLAY}'" not in css:
        fail(f"the wordmark must be set in {DISPLAY}")
    else:
        checked += 1
    return checked


def check_schedule() -> int:
    """The exercise clock must be one drill every four minutes."""
    src = open(SCHEDULE, encoding="utf-8").read()
    checked = 0

    def const(name: str) -> int | None:
        m = re.search(rf"export const {name} = (\d+);", src)
        return int(m.group(1)) if m else None

    period = const("PERIOD")
    first = const("FIRST_DRILL")
    drill = const("DRILL_LEN")
    settle = const("SETTLE_LEN")
    warn = const("WARN_LEN")

    for name, value in (("PERIOD", period), ("FIRST_DRILL", first), ("DRILL_LEN", drill),
                        ("SETTLE_LEN", settle), ("WARN_LEN", warn)):
        if value is None:
            fail(f"schedule constant {name} is missing")
            return checked
        checked += 1

    if period != 240:
        fail(f"the drill must run every 240 s, found {period}")
    if first != 240:
        fail(f"the first drill must start one full period in, found {first}")
    if drill + settle > period:
        fail(f"the drill and settle ({drill}+{settle}) must fit inside the period ({period})")
    if warn >= period:
        fail(f"the warning window ({warn}) must be shorter than the period ({period})")

    # the warning must sit immediately before the drill
    if not re.search(r"if \(t >= next - WARN_LEN\) return 'warning';", src):
        fail("the warning phase must be the window just before the next drill")
    else:
        checked += 1
    return checked


def check_orb_assets() -> int:
    """The shipped orb media must be the ones the generator produces."""
    proc = subprocess.run(
        [sys.executable, os.path.join(ROOT, "scripts", "build-redteam-media.py"), "--check"],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        fail(f"orb media drifted from the generator: {(proc.stdout + proc.stderr).strip()[:200]}")
        return 0
    return 1


def check_space_backdrop() -> int:
    """The sky must be the plate the drawing script produces."""
    proc = subprocess.run(
        [sys.executable, os.path.join(ROOT, "scripts", "build-space-backdrop.py"), "--check"],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        fail(f"space backdrop drifted from the generator: {(proc.stdout + proc.stderr).strip()[:200]}")
        return 0
    checked = 1

    # ...and it must still be a sky with stars in it.
    #
    # The plate is 4096 wide and a screen is 1280 to 2560, so the browser
    # shrinks every star in it before it reaches the eye. A star drawn too
    # tight arrives smaller than a pixel and is averaged down into grey dust:
    # the sky kept its dust and lost its stars. Counting the bright points
    # that survive the downscale is what catches that, and it is the check
    # that would have failed on the plate that shipped before.
    plate = os.path.join(PUBLIC, "media", "space", "deep-field.webp")
    if not os.path.isfile(plate):
        fail("the sky ships no plate")
        return checked
    page = Image.open(plate).convert("L").resize((1280, 720), Image.LANCZOS)
    lum = np.asarray(page, dtype=float)
    peak = np.asarray(page.filter(ImageFilter.MaxFilter(5)), dtype=float)
    points = int(((lum >= peak - 1e-9) & (lum >= 110)).sum())
    if points < 120:
        fail(f"the sky has no stars in it: {points} distinct bright points at "
             f"page size, where the shipped plate has 407 and the dust-only "
             f"plate that came before it had 40")
    return checked + 1


def check_no_snowfall() -> int:
    """
    The page must not carry the falling specks.

    There used to be a canvas over the sky painting eighty-five white dots
    that fell downwards and wrapped around. On a dark page that read as
    snowfall, and nothing in the rest of the page looked like weather. The
    sky is now the drawn plate alone, so there must be no canvas and no
    starfield module left.
    """
    checked = 0
    doc = open(shipped_document(), encoding="utf-8").read()
    if "<canvas" in doc.lower():
        fail("the shipped page carries a canvas — that is where the snowfall was")
    checked += 1

    src = os.path.join(APP, "src")
    for base, _dirs, files in os.walk(src):
        for name in files:
            if not name.endswith((".ts", ".tsx")):
                continue
            path = os.path.join(base, name)
            body = open(path, encoding="utf-8").read()
            checked += 1
            if "useStarfield" in body:
                fail(f"{os.path.relpath(path, ROOT)} still paints the falling specks")
            if "<canvas" in body:
                fail(f"{os.path.relpath(path, ROOT)} still carries a canvas")
    return checked


def check_scene_pipeline() -> int:
    """
    The page must be able to move through the scenes without the globe moving.

    The manifest is generated from videos/scenes.json, and the app reads it.
    Four things have to hold:
      * the config and the manifest must agree, and every scene the manifest
        names must exist on disk;
      * every scene must be built at the same size, with the globe in the same
        place — that is what makes the crossing from one scene to the next a
        change of colour and camera rather than a jump;
      * the set must contain both kinds — a picture and a film — or the page
        could not move between them at all;
      * and the page must preload the picture of the scene it opens on.
    """
    config_path = os.path.join(ROOT, "videos", "scenes.json")
    manifest_path = os.path.join(APP, "src", "media", "scenes.ts")
    checked = 0

    if not os.path.isfile(config_path):
        fail("missing videos/scenes.json")
        return checked
    with open(config_path, encoding="utf-8") as fh:
        config = json.load(fh)
    checked += 1

    if not os.path.isfile(manifest_path):
        fail("missing the generated scene manifest")
        return checked
    manifest = open(manifest_path, encoding="utf-8").read()
    checked += 1

    scenes = config.get("scenes") or []
    if not scenes:
        fail("videos/scenes.json declares no scenes")
        return checked
    checked += 1

    default = config.get("default")
    if default not in {s.get("id") for s in scenes}:
        fail("the default scene is not one of the declared scenes")
    else:
        checked += 1

    kinds = set()
    default_webp = None
    sizes = set()
    for scene in scenes:
        sid = scene.get("id", "?")
        if '"%s"' % sid not in manifest:
            fail(f"the manifest does not name the scene {sid}")
            continue
        checked += 1
        kinds.add(scene.get("kind"))

        # the picture of the scene, and the film's own file. An alert scene
        # has no media of its own: it borrows another scene's picture.
        if scene.get("kind") == "alert":
            poster = scene.get("poster")
            if not poster:
                fail(f"the alert scene {sid} names no poster")
                continue
            paths = ("/media/scenes/%s.png" % poster, "/media/scenes/%s.webp" % poster)
        else:
            paths = ("/media/scenes/%s.png" % sid, "/media/scenes/%s.webp" % sid)
        for path in paths:
            target = os.path.join(PUBLIC, path.lstrip("/"))
            if not os.path.isfile(target):
                fail(f"the scene {sid} points at a missing file: {path}")
            elif os.path.getsize(target) == 0:
                fail(f"the scene {sid} points at an empty file: {path}")
            else:
                checked += 1

        if scene.get("kind") == "video":
            video = os.path.join(PUBLIC, "video", "%s.mp4" % sid)
            if not os.path.isfile(video) or os.path.getsize(video) == 0:
                fail(f"the film {sid} is missing or empty")
            else:
                checked += 1

        # every scene must be the same size, or the globe jumps between them
        still_id = scene.get("poster") if scene.get("kind") == "alert" else sid
        still = os.path.join(PUBLIC, "media", "scenes", "%s.png" % still_id)
        if os.path.isfile(still):
            with Image.open(still) as im:
                sizes.add(im.size)
        if sid == default:
            default_webp = "/media/scenes/%s.webp" % sid

    if len(sizes) > 1:
        fail(f"the scenes are not all the same size: {sorted(sizes)}")
    elif sizes:
        checked += 1

    # the manifest must agree with the files it names
    for width, height in re.findall(r"width: (\d+),\s*\n\s*height: (\d+)", manifest):
        sizes.add((int(width), int(height)))
    if len(sizes) > 1:
        fail(f"the manifest does not put every scene at one size: {sorted(sizes)}")
    else:
        checked += 1

    # the page preloads the default scene's picture; if the default changes
    # and the preload does not, the preload is wasted and the check fails
    doc = open(shipped_document(), encoding="utf-8").read()
    preloads = re.findall(r'<link[^>]+rel="preload"[^>]+href="([^"]+)"', doc)
    if default_webp and default_webp not in preloads:
        fail(f"the page does not preload the default scene: {default_webp}")
    else:
        checked += 1

    if "image" not in kinds:
        fail("no still scene: the page could not switch to a picture")
    if "video" not in kinds:
        fail("no film scene: the page could not switch to the turning globe")
    if "image" in kinds and "video" in kinds:
        checked += 1
    return checked


def check_no_storm() -> int:
    """
    The globe must not be carrying weather.

    The delivered footage has a bank of cloud lying across the top of the
    globe, lit from inside by the city, with a flash of lightning with it.
    The pipeline takes the cloud off: the hole it leaves is filled with the
    sky of that row, so the light of the cloud is gone and the sky is back.

    This checks that on the media that actually ships. For every scene whose
    delivered source carried the storm, the shipped picture must differ from
    the delivered one over the cloud, and the cloud's own light must be lower
    in the shipped picture than it was in the delivered one. The scenes that
    were delivered without a storm are not checked: their brightness is the
    sky, and the daylight sky is white in the same places a storm is.
    """
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "build_redteam_media", os.path.join(ROOT, "scripts", "build-redteam-media.py"))
    br = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(br)

    config_path = os.path.join(ROOT, "videos", "scenes.json")
    with open(config_path, encoding="utf-8") as fh:
        config = json.load(fh)

    checked = 0
    for scene in config["scenes"]:
        sid = scene["id"]
        stormy = bool(scene.get("repair_storm"))
        if not stormy:
            continue

        source = os.path.join(ROOT, "videos", scene["source"])
        if not os.path.isfile(source):
            fail(f"missing source %s" % scene["source"])
            continue
        if scene.get("kind") == "video":
            frame = next(br.decode_frames(source))
        else:
            frame = np.asarray(Image.open(source).convert("RGB"))
        if scene.get("kind") == "video" and not scene.get("repair_storm"):
            continue

        hole = br._cloud_like(frame) & br._cloud_region(frame)
        if hole.sum() < 200:
            fail(f"the scene {sid} carries no storm to remove")
            continue

        # the same hole, in the geometry the shipped picture is in
        h, w = hole.shape
        cut = max(2, int(round(max(w, h) * 0.005)))
        staged_hole = np.asarray(
            Image.fromarray((hole * 255).astype(np.uint8))
            .crop((cut, cut, w - cut, h - cut))
            .resize((br.CANVAS, br.CANVAS), Image.NEAREST)
        ) > 127
        if staged_hole.sum() < 200:
            continue

        delivered = np.asarray(br.stage(Image.fromarray(frame)).convert("RGB")).astype(int)
        shipped_path = os.path.join(PUBLIC, "media", "scenes", "%s.png" % sid)
        if not os.path.isfile(shipped_path):
            fail(f"the scene {sid} ships no picture")
            continue
        shipped = np.asarray(Image.open(shipped_path).convert("RGB")).astype(int)

        # the cloud must actually have been worked on
        change = np.abs(shipped - delivered).max(axis=2)[staged_hole].mean()
        if change < 6.0:
            fail(f"the scene {sid} ships the storm unchanged (mean change {change:.1f})")
            continue
        checked += 1

        # and the light of the cloud must be gone, not added
        def lum(img):
            return 0.299 * img[:, :, 0] + 0.587 * img[:, :, 1] + 0.114 * img[:, :, 2]

        before = lum(delivered)[staged_hole].mean()
        after = lum(shipped)[staged_hole].mean()
        if after > before * 0.95:
            fail(f"the scene {sid} is brighter over the storm than it was "
                 f"({after:.0f} against {before:.0f})")
            continue
        checked += 1
    return checked


def check_server_guard() -> int:
    """The production server must refuse to escape its dist directory."""
    src = open(SERVER, encoding="utf-8").read()
    checked = 0
    if "function inside(" not in src or "!inside(" not in src:
        fail("the server must check that a resolved path stays inside the dist directory")
    else:
        checked += 1
    if "'/healthz'" not in src:
        fail("the server must expose /healthz")
    else:
        checked += 1
    if "Accept-Ranges" not in src or "Content-Range" not in src:
        fail("the server must support HTTP Range requests for the video")
    else:
        checked += 1
    # a missing asset has to say it is missing. Answering one with the page's
    # HTML and a 200 would hide a broken picture from the browser, from the
    # logs and from this check.
    if "404" not in src or "Not found" not in src:
        fail("the server must answer a missing asset with a 404")
    elif "\\.[A-Za-z0-9]+$" not in src:
        fail("the server must tell a missing asset from a route by its extension")
    else:
        checked += 1
    return checked


def main() -> int:
    checks = {
        "local references": check_local_references,
        "no external requests": check_no_external_requests,
        "fonts": check_fonts,
        "brand block": check_brand,
        "exercise schedule": check_schedule,
        "scene pipeline": check_scene_pipeline,
        "no storm": check_no_storm,
        "orb media": check_orb_assets,
        "space backdrop": check_space_backdrop,
        "no snowfall": check_no_snowfall,
        "server guard": check_server_guard,
    }
    for name, fn in checks.items():
        try:
            count = fn()
            print(f"  {name}: {count} checked")
        except Exception as exc:  # a broken check is a failure, not a crash
            fail(f"{name} check raised: {exc}")
            print(f"  {name}: ERROR {exc}")

    if failures:
        print(f"\nFAIL ({len(failures)} problem(s))")
        for f in failures:
            print(f"  - {f}")
        return 1
    print("\nPASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
