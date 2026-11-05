#!/usr/bin/env python3
"""
Integrity check for the Walkaway marketing surface.

Verifies, without a browser:
  1. Every local asset referenced by index.html exists in the site directory.
  2. Every @font-face URL in site.css resolves to a real file.
  3. Every generated poster / banner SVG declares a viewBox that matches the
     intrinsic size advertised for it in index.html.
  4. No <text> node in a generated SVG overflows its canvas, using the real
     advance widths of the shipped woff2 faces.
  5. No character used in index.html or in a generated SVG is missing from
     every shipped font (Druk Wide, GT Pressura Mono, Departure Mono).

Usage:
    python3 scripts/verify-marketing-site.py
    PY=/path/to/python-with-fonttools python3 scripts/verify-marketing-site.py

Exit code 0 = all checks pass. Exit code 1 = at least one failure.
Metric checks (4 and 5) are skipped, with a warning, when fontTools is absent.
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

# Generated creative directories, relative to the site root.
CREATIVE_DIRS = ("assets/posters", "assets/banners")

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
            "fontTools not installed - skipping creative metric and glyph checks "
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
        hhea = font["hhea"]
        metrics[family] = (upm, cmap, font["hmtx"], hhea.ascent, -hhea.descent)
    return metrics


def text_width(runs, size: float, letter_spacing: float, metrics) -> float:
    """Width of a text run, resolved per glyph through the font stack.

    The browser falls back per character, so a run like "10/10" set in Druk
    Wide measures its slash in Departure Mono. Measuring the whole run in one
    face overstates the width and produces false overflow reports.

    `runs` is a list of (text, stack) pairs - one per <tspan> or the whole
    <text> element.
    """
    em_total = 0.0
    count = 0
    for text, stack in runs:
        if not stack:
            continue
        for char in text:
            advance = None
            for family in stack:
                upm, cmap, hmtx, _asc, _desc = metrics[family]
                if ord(char) in cmap:
                    advance = hmtx[cmap[ord(char)]][0] / upm
                    break
            if advance is None:
                upm, cmap, hmtx, _asc, _desc = metrics[stack[-1]]
                advance = hmtx[cmap.get(ord("?"), ".notdef")][0] / upm
            em_total += advance
            count += 1
    return em_total * size + letter_spacing * max(0, count - 1)


def text_runs(text_attrs: str, body: str, rules) -> list[tuple[str, list[str]]]:
    """Split a <text> body into (text, font stack) runs.

    A <tspan> may name its own font-family; the browser uses it for the
    characters it contains and falls back to the parent stack otherwise.
    """
    base = resolve_stack(resolve_props(text_attrs, rules).get("font-family"))
    runs: list[tuple[str, list[str]]] = []
    pos = 0
    for m in re.finditer(r"<tspan\b([^>]*)>(.*?)</tspan>", body, re.S):
        plain = body[pos:m.start()]
        if plain:
            runs.append((plain, base))
        stack = resolve_stack(resolve_props(m.group(1), rules).get("font-family")) or base
        runs.append((m.group(2), stack))
        pos = m.end()
    if body[pos:]:
        runs.append((body[pos:], base))
    return runs


# --- creative SVG parsing ---------------------------------------------------


def attr(attrs: str, name: str):
    """Read an XML attribute or a CSS declaration, whichever is present."""
    quoted = re.escape(name)
    m = re.search(r"(?:^|\s)" + quoted + r'\s*=\s*"([^"]*)"', attrs)
    if m:
        return m.group(1)
    m = re.search(r"(?:^|[\s;{])" + quoted + r"\s*:\s*([^;\"']+)", attrs)
    if m:
        return m.group(1).strip()
    return None


def num(value):
    if value is None:
        return None
    try:
        return float(value)
    except ValueError:
        return None


def family_of(value):
    """Map an SVG font-family value onto one of the shipped faces."""
    if not value:
        return None
    low = value.lower()
    if "druk" in low:
        return "Druk Wide"
    if "pressura" in low:
        return "GT Pressura Mono"
    if "departure" in low:
        return "Departure Mono"
    return None


def resolve_stack(value) -> list[str]:
    """The shipped faces named by a font-family value, in fallback order."""
    stack: list[str] = []
    for raw in re.findall(r"'([^']*)'|\"([^\"]*)\"|([A-Za-z][\w -]*)", value or ""):
        token = next((part for part in raw if part), "")
        fam = family_of(token)
        if fam and fam not in stack:
            stack.append(fam)
    return stack


def referenced_creatives() -> list[tuple[str, int, int]]:
    """(relative path, advertised width, advertised height) for every SVG
    referenced from index.html."""
    doc = open(INDEX, encoding="utf-8").read()
    out = []
    for path, w, h in re.findall(
        r'<img\s+[^>]*src="(assets/(?:posters|banners)/[^"]+\.svg)"[^>]*'
        r'width="(\d+)"\s+height="(\d+)"',
        doc,
        re.S,
    ):
        out.append((path, int(w), int(h)))
    return out


def check_creative_viewboxes() -> int:
    """Each creative's viewBox must match the size advertised in index.html."""
    checked = 0
    for path, width, height in referenced_creatives():
        full = os.path.join(SITE, path)
        if not os.path.isfile(full):
            continue  # already reported by check_local_references
        checked += 1
        svg = open(full, encoding="utf-8").read()
        m = re.search(r'viewBox\s*=\s*"([-\d.\s]+)"', svg)
        if not m:
            fail(f"{path}: no viewBox declared")
            continue
        parts = m.group(1).split()
        if len(parts) != 4:
            fail(f"{path}: malformed viewBox {m.group(1)!r}")
            continue
        _, _, vw, vh = (float(p) for p in parts)
        if abs(vw - width) > 0.5 or abs(vh - height) > 0.5:
            fail(
                f"{path}: viewBox is {vw:g}x{vh:g} but index.html advertises "
                f"{width}x{height}"
            )
    return checked


def parse_css_rules(svg: str) -> dict[str, dict[str, str]]:
    """class selector -> {property: value}, read from any <style> block."""
    rules: dict[str, dict[str, str]] = {}
    for css in re.findall(r"<style[^>]*>(.*?)</style>", svg, re.S):
        for selector, body in re.findall(r"([^{}]+)\{([^{}]*)\}", css):
            decls: dict[str, str] = {}
            for decl in body.split(";"):
                if ":" in decl:
                    key, value = decl.split(":", 1)
                    decls[key.strip().lower()] = value.strip().strip("'\"")
            sel = selector.strip()
            if re.fullmatch(r"\.[A-Za-z_][\w-]*", sel):
                rules.setdefault(sel[1:], {}).update(decls)
    return rules


def resolve_props(attrs: str, rules: dict[str, dict[str, str]]) -> dict[str, str]:
    """Effective properties for a <text> element.

    Presentation attributes are the weakest, a class rule overrides them, and
    an inline style attribute overrides everything - matching CSS semantics.
    """
    props: dict[str, str] = {}
    for key in ("font-family", "font-size", "letter-spacing", "text-anchor", "x", "y"):
        value = attr(attrs, key)
        if value is not None:
            props[key] = value

    class_attr = attr(attrs, "class") or ""
    for cls in class_attr.split():
        props.update(rules.get(cls, {}))

    inline = attr(attrs, "style")
    if inline:
        for decl in inline.split(";"):
            if ":" in decl:
                key, value = decl.split(":", 1)
                props[key.strip().lower()] = value.strip().strip("'\"")
    return props


def check_creative_fit(metrics) -> int:
    """No text in a generated creative may overflow the canvas."""
    checked = 0
    for path, _width, _height in referenced_creatives():
        full = os.path.join(SITE, path)
        if not os.path.isfile(full):
            continue
        svg = open(full, encoding="utf-8").read()

        m = re.search(r'viewBox\s*=\s*"([-\d.\s]+)"', svg)
        if not m:
            continue
        parts = m.group(1).split()
        canvas_w = float(parts[2])
        canvas_h = float(parts[3])

        rules = parse_css_rules(svg)

        for attrs, content in re.findall(r"<text\b([^>]*)>(.*?)</text>", svg, re.S):
            props = resolve_props(attrs, rules)
            stack = resolve_stack(props.get("font-family"))
            if not stack:
                continue
            size = num(props.get("font-size"))
            if size is None:
                continue

            runs = text_runs(attrs, content, rules)

            raw_x = props.get("x")
            x = None
            if raw_x is not None and raw_x.endswith("%"):
                pct = num(raw_x[:-1])
                x = canvas_w * pct / 100.0 if pct is not None else None
            else:
                x = num(raw_x)
            if x is None:
                continue

            # A translate() on the text element itself shifts the origin.
            tr = re.search(r'transform\s*=\s*"translate\(\s*([-\d.]+)', attrs)
            if tr:
                x += float(tr.group(1))

            ls = num(props.get("letter-spacing")) or 0.0
            anchor = (props.get("text-anchor") or "start").strip()

            label = html.unescape(re.sub(r"<[^>]+>", "", content)).strip()
            label = " ".join(label.split())
            if not label:
                continue
            checked += 1

            width = text_width(runs, size, ls, metrics)
            margin = 24
            if anchor == "middle":
                available = 2 * min(x, canvas_w - x) - margin
            elif anchor == "end":
                available = x - margin
            else:
                available = (canvas_w - margin) - x

            if available <= 0 or width > available:
                fail(
                    f"{path}: text overflows the canvas horizontally: {label!r} "
                    f"(width {width:.0f} > available {available:.0f})"
                )

            # Vertical: the ink box of the line must stay on the canvas.
            y = num(props.get("y"))
            if y is not None:
                _upm, _cmap, _hmtx, asc, desc = metrics[stack[0]]
                top = y - asc * size / _upm
                bottom = y + desc * size / _upm
                slack = canvas_h * 0.02
                if top < -slack or bottom > canvas_h + slack:
                    fail(
                        f"{path}: text overflows the canvas vertically: {label!r} "
                        f"(ink box {top:.0f}..{bottom:.0f}, canvas height {canvas_h:.0f})"
                    )
    return checked


def check_glyph_coverage(metrics) -> int:
    """Every character on the page (and in the creatives) must exist in at
    least one shipped font."""
    doc = open(INDEX, encoding="utf-8").read()
    used = {c for c in doc if ord(c) > 0x7F}

    for directory in CREATIVE_DIRS:
        full_dir = os.path.join(SITE, directory)
        if not os.path.isdir(full_dir):
            continue
        for name in sorted(os.listdir(full_dir)):
            if not name.endswith(".svg"):
                continue
            svg = open(os.path.join(full_dir, name), encoding="utf-8").read()
            for attrs, content in re.findall(r"<text\b([^>]*)>(.*?)</text>", svg, re.S):
                label = html.unescape(re.sub(r"<[^>]+>", "", content))
                used |= {c for c in label if ord(c) > 0x7F}

    covered: set[str] = set()
    for family, (upm, cmap, _hmtx, _asc, _desc) in metrics.items():
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

    viewboxes = check_creative_viewboxes()
    print(f"[ok] creative viewboxes matched: {viewboxes}")

    metrics = load_metrics()
    if metrics:
        fit = check_creative_fit(metrics)
        glyphs = check_glyph_coverage(metrics)
        print(f"[ok] creative text nodes fitted: {fit}")
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
