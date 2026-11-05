#!/usr/bin/env python3
"""
Integrity check for the Walkaway × Muse Blitz marketing surface.

Verifies, without a browser:
  1. Every local asset referenced by index.html exists in the site directory.
  2. Every @font-face URL in site.css resolves to a real file.
  3. Every banner <text> node fits inside its SVG canvas, using the real
     advance widths of the shipped woff2 faces.
  4. No character used in index.html is missing from every shipped font
     (Druk Wide, GT Pressura Mono, Departure Mono).

Usage:
    python3 scripts/verify-marketing-site.py
    PY=/path/to/python-with-fonttools python3 scripts/verify-marketing-site.py

Exit code 0 = all checks pass. Exit code 1 = at least one failure.
Metric checks (3 and 4) are skipped, with a warning, when fontTools is absent.
"""

from __future__ import annotations

import html
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SITE = os.path.join(ROOT, "apps", "marketing-site")
INDEX = os.path.join(SITE, "index.html")
CSS = os.path.join(SITE, "assets", "css", "site.css")

FONT_FILES = {
    "Druk Wide": "assets/fonts/DrukWide-Heavy.woff2",
    "Druk Wide Medium": "assets/fonts/DrukWide-Medium.woff2",
    "GT Pressura Mono": "assets/fonts/GTPressuraMono-Bold.woff2",
    "Departure Mono": "assets/fonts/DepartureMono-Regular.woff2",
}

failures: list[str] = []
warnings: list[str] = []


def fail(msg: str) -> None:
    failures.append(msg)


def check_local_references() -> int:
    """Every local src/href in index.html must exist on disk."""
    doc = open(INDEX, encoding="utf-8").read()
    refs = set(re.findall(r'(?:src|href)="([^"#][^"]*)"', doc))
    checked = 0
    for ref in sorted(refs):
        if ref.startswith(("http://", "https://", "mailto:", "data:")):
            continue
        target = os.path.join(SITE, ref.split("?")[0].split("#")[0])
        checked += 1
        if not os.path.isfile(target):
            fail(f"missing asset referenced by index.html: {ref}")
    return checked


def check_css_font_urls() -> int:
    """Every url(...) in a @font-face block must resolve."""
    css = open(CSS, encoding="utf-8").read()
    blocks = re.findall(r"@font-face\s*\{(.*?)\}", css, re.S)
    checked = 0
    for block in blocks:
        for url in re.findall(r"url\(['\"]?([^'\")]+)['\"]?\)", block):
            if url.startswith(("http://", "https://", "data:")):
                continue
            target = os.path.normpath(os.path.join(SITE, "assets", "css", url))
            checked += 1
            if not os.path.isfile(target):
                fail(f"missing font file referenced by site.css: {url}")
    for family, rel in FONT_FILES.items():
        if not os.path.isfile(os.path.join(SITE, rel)):
            fail(f"missing web font: {rel} ({family})")
    return checked


def load_metrics():
    try:
        from fontTools.ttLib import TTFont
    except ImportError:
        warnings.append(
            "fontTools not installed - skipping banner metric and glyph checks "
            "(pip install fonttools brotli, or set PY=... to a venv that has it)"
        )
        return None

    metrics = {}
    for family, rel in FONT_FILES.items():
        path = os.path.join(SITE, rel)
        if not os.path.isfile(path):
            continue
        font = TTFont(path)
        upm = font["head"].unitsPerEm
        cmap: dict[int, str] = {}
        for table in font["cmap"].tables:
            cmap.update(table.cmap)
        metrics[family] = (upm, cmap, font["hmtx"])
    return metrics


def text_width(text: str, family: str, size: float, letter_spacing: float, metrics) -> float:
    upm, cmap, hmtx = metrics[family]
    total = 0
    for char in text:
        glyph = cmap.get(ord(char)) or cmap.get(ord("?"), ".notdef")
        total += hmtx[glyph][0]
    width = total * size / upm + letter_spacing * max(0, len(text) - 1)
    return width


def check_banner_fit(metrics) -> int:
    """No banner text may overflow the canvas of the SVG that holds it."""
    doc = open(INDEX, encoding="utf-8").read()
    svgs = re.findall(
        r'<svg class="banner-svg"[^>]*viewBox="0 0 (\d+) (\d+)"[^>]*>(.*?)</svg>',
        doc,
        re.S,
    )
    if len(svgs) != 6:
        fail(f"expected 6 banner creatives, found {len(svgs)}")

    checked = 0
    for canvas_w, _canvas_h, body in svgs:
        canvas_w = int(canvas_w)
        for attrs, content in re.findall(r"<text\b([^>]*)>(.*?)</text>", body, re.S):
            if "Druk Wide" in attrs:
                family = "Druk Wide"
            elif "GT Pressura" in attrs:
                family = "GT Pressura Mono"
            else:
                family = "Departure Mono"
            size_m = re.search(r'font-size="([\d.]+)"', attrs)
            x_m = re.search(r'x="([\d.]+)"', attrs)
            if not size_m or not x_m:
                continue
            size = float(size_m.group(1))
            x = float(x_m.group(1))
            ls_m = re.search(r'letter-spacing="(-?[\d.]+)"', attrs)
            letter_spacing = float(ls_m.group(1)) if ls_m else 0.0
            anchor_middle = 'text-anchor="middle"' in attrs
            label = html.unescape(re.sub(r"<[^>]+>", "", content)).strip()
            if not label:
                continue
            checked += 1
            width = text_width(label, family, size, letter_spacing, metrics)
            margin = 36
            if anchor_middle:
                available = 2 * min(x, canvas_w - x) - margin
            else:
                available = (canvas_w - margin) - x
            if available <= 0 or width > available:
                fail(
                    f"banner text overflows its canvas: {label!r} "
                    f"(width {width:.0f} > available {available:.0f})"
                )
    return checked


def check_glyph_coverage(metrics) -> int:
    """Every character on the page must exist in at least one shipped font."""
    doc = open(INDEX, encoding="utf-8").read()
    used = {c for c in doc if ord(c) > 0x7F}

    covered: set[str] = set()
    for family, (upm, cmap, _hmtx) in metrics.items():
        covered |= {chr(cp) for cp in cmap if cp > 0x7F}

    missing = sorted(c for c in used if c not in covered)
    if missing:
        fail(
            "characters missing from every shipped font: "
            + " ".join(f"U+{ord(c):04X} {c!r}" for c in missing)
        )
    return len(used)


def main() -> int:
    if not os.path.isfile(INDEX):
        print(f"FATAL: {INDEX} not found")
        return 1

    assets = check_local_references()
    font_urls = check_css_font_urls()
    print(f"[ok] local references resolved: {assets}")
    print(f"[ok] font urls resolved: {font_urls}")

    metrics = load_metrics()
    if metrics:
        fit = check_banner_fit(metrics)
        glyphs = check_glyph_coverage(metrics)
        print(f"[ok] banner text nodes fitted: {fit}")
        print(f"[ok] non-ascii characters covered: {glyphs}")
    else:
        warnings.append("metric checks skipped")

    for warning in warnings:
        print(f"[warn] {warning}")
    for failure in failures:
        print(f"[FAIL] {failure}")

    if failures:
        print(f"\nFAILED: {len(failures)} problem(s)")
        return 1
    print("\nPASS: marketing surface integrity verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
