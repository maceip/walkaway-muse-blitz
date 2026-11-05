#!/usr/bin/env python3
"""
Build the Walkaway × Muse Blitz poster and banner set.

Every artboard is a standalone SVG with the three shipped web fonts embedded as
base64 data URIs, flat colour only (one grain filter, no gradients, no
shadows, no scripts, no external references).

Type fitting is measured with fontTools against the real shipped woff2
advances, including letter-spacing and the per-character CSS font fallback
used for the handful of glyphs that Druk Wide / GT Pressura Mono do not carry
(U+002F slash, U+00B7 middot, U+00D7 multiplication sign). Sizes are only ever
scaled DOWN to fit the declared box of every text node; the build exits
non-zero if anything overflows.

Usage:
    python3 scripts/build-posters.py
    PY=/tmp/fontenv/bin/python python3 scripts/build-posters.py

Outputs:
    apps/marketing-site/assets/posters/*.svg   (10 x 1920x1080)
    apps/marketing-site/assets/banners/*.svg   (6 x ad aspect ratios)
"""

from __future__ import annotations

import base64
import math
import os
import sys
import xml.etree.ElementTree as ET
from xml.sax.saxutils import escape, quoteattr

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FONT_DIR = os.path.join(ROOT, "apps", "marketing-site", "assets", "fonts")
POSTER_DIR = os.path.join(ROOT, "apps", "marketing-site", "assets", "posters")
BANNER_DIR = os.path.join(ROOT, "apps", "marketing-site", "assets", "banners")

# --- palette (flat only) ---------------------------------------------------
INK = "#000000"
PAPER = "#EFEDE2"
PAPER_2 = "#E3E0D2"
GREEN = "#008942"
CYAN = "#09B1C7"
ORANGE = "#FF9D00"
SOLAR = "#FFC400"

# --- type sources ----------------------------------------------------------
# family name, shipped woff2 file, css font-weight
FONT_SOURCES = (
    ("Druk Wide", "DrukWide-Heavy.woff2", 900),
    ("GT Pressura Mono", "GTPressuraMono-Bold.woff2", 700),
    ("Departure Mono", "DepartureMono-Regular.woff2", 400),
)

DISPLAY = ("Druk Wide", "Departure Mono")  # display + fallback for U+002F
MICRO = ("GT Pressura Mono", "Departure Mono")  # micro labels + fallback
BODY = ("Departure Mono",)  # quiet body lines

# --- type scale ------------------------------------------------------------
DISPLAY_LS = -0.02  # em, Druk Wide Heavy
MICRO_LS = 0.18  # em, GT Pressura Mono Bold
FIT_EPSILON = 1.0  # px of slack kept on every fitted box

GRAIN_OPACITY = 0.08  # inside the 0.05 - 0.12 band


def ensure_fonttools() -> None:
    """Re-exec under an interpreter that has fontTools, if needed."""
    try:
        import fontTools  # noqa: F401

        return
    except ImportError:
        pass

    if os.environ.get("WALKAWAY_POSTERS_REEXEC") == "1":
        sys.stderr.write("FATAL: fontTools is still missing after re-exec.\n")
        sys.exit(2)

    candidates = []
    env_python = os.environ.get("PY")
    if env_python:
        candidates.append(env_python)
    candidates.append("/tmp/fontenv/bin/python")

    for candidate in candidates:
        if not candidate or not os.path.isfile(candidate):
            continue
        if not os.access(candidate, os.X_OK):
            continue
        # compare the invoked paths, not the resolved binaries: a venv python
        # symlinks to the same base interpreter but has its own site-packages
        if os.path.abspath(candidate) == os.path.abspath(sys.executable):
            continue
        os.environ["WALKAWAY_POSTERS_REEXEC"] = "1"
        os.execv(candidate, [candidate, os.path.abspath(__file__), *sys.argv[1:]])

    sys.stderr.write(
        "FATAL: fontTools is required to measure the poster type.\n"
        "       pip install fonttools brotli, or run with a python that has it:\n"
        "       PY=/tmp/fontenv/bin/python python3 scripts/build-posters.py\n"
    )
    sys.exit(2)


ensure_fonttools()

from fontTools.ttLib import TTFont  # noqa: E402


class Face:
    """One shipped woff2 plus the metrics needed to measure it."""

    def __init__(self, family: str, filename: str, weight: int) -> None:
        from fontTools.ttLib import TTFont as _TTFont

        self.family = family
        self.weight = weight
        self.path = os.path.join(FONT_DIR, filename)
        if not os.path.isfile(self.path):
            raise SystemExit(f"FATAL: missing web font {self.path}")

        font = _TTFont(self.path)
        self.upm = font["head"].unitsPerEm
        self.cmap: dict[int, str] = {}
        for table in font["cmap"].tables:
            self.cmap.update(table.cmap)
        self.hmtx = font["hmtx"]
        # hhea ascent/descent, in em, as browsers use for the line box
        self.ascent = font["hhea"].ascent / self.upm
        self.descent = -font["hhea"].descent / self.upm
        self.cap_height = self._cap_height(font)

        with open(self.path, "rb") as handle:
            self.data_b64 = base64.b64encode(handle.read()).decode("ascii")

    def advance_em(self, char: str) -> float:
        glyph = self.cmap.get(ord(char), ".notdef")
        return self.hmtx[glyph][0] / self.upm

    @staticmethod
    def _cap_height(font) -> float:
        """Cap height in em, from the real glyph outlines."""
        try:
            from fontTools.pens.boundsPen import BoundsPen

            glyph_set = font.getGlyphSet()
            pen = BoundsPen(glyph_set)
            glyph_set["H"].draw(pen)
            return pen.bounds[3] / font["head"].unitsPerEm
        except Exception:  # pragma: no cover - defensive fallback
            return 0.7

    def has(self, char: str) -> bool:
        return ord(char) in self.cmap


class TypeSystem:
    """Measures text against the shipped faces."""

    def __init__(self, font_dir: str = FONT_DIR) -> None:
        self.faces: dict[str, Face] = {}
        for family, filename, weight in FONT_SOURCES:
            self.faces[family] = Face(family, filename, weight)

    # -- stacks ------------------------------------------------------------
    def stack(self, families) -> list[Face]:
        return [self.faces[name] for name in families]

    def css_stack(self, families) -> str:
        parts = [f"'{self.faces[name].family}'" for name in families]
        parts.append("sans-serif")
        return ",".join(parts)

    def resolve(self, char: str, stack: list[Face]) -> Face | None:
        for face in stack:
            if face.has(char):
                return face
        return None

    # -- measurement --------------------------------------------------------
    @staticmethod
    def _gap_count(count: int, ls_em: float) -> int:
        """Letter-spacing gaps that count toward the rendered width.

        Chrome/Safari add spacing after the final glyph too, so a positive
        letter-spacing contributes ``count`` gaps. Negative letter-spacing is
        counted with ``count - 1`` gaps, which is the conservative (larger)
        reading for a shrink.
        """
        if count <= 0:
            return 0
        return count if ls_em > 0 else count - 1

    def width_em(self, text: str, stack: list[Face]) -> float:
        """Advance width of ``text`` in em (font fallback resolved per glyph)."""
        total = 0.0
        for char in text:
            face = self.resolve(char, stack)
            if face is None:
                raise KeyError(
                    f"glyph U+{ord(char):04X} ({char!r}) is not in any of "
                    f"{[f.family for f in stack]}"
                )
            total += face.advance_em(char)
        return total

    def width_coef(self, text: str, stack: list[Face], ls_em: float) -> float:
        """Width of ``text`` as a linear coefficient of the font-size."""
        return self.width_em(text, stack) + ls_em * self._gap_count(len(text), ls_em)

    def measure(self, text: str, stack: list[Face], size: float, ls_px: float) -> float:
        """Rendered width in px at ``size`` with ``ls_px`` of letter-spacing."""
        gaps = self._gap_count(len(text), 1.0 if ls_px > 0 else -1.0)
        return self.width_em(text, stack) * size + ls_px * gaps

    def fit_size(
        self,
        lines,
        stack: list[Face],
        max_size: float,
        max_width: float,
        ls_em: float,
        max_height: float | None = None,
        line_height: float = 0.86,
    ) -> float:
        """Largest size <= max_size at which every line fits the box."""
        coef = max(self.width_coef(line, stack, ls_em) for line in lines)
        if coef <= 0:
            return max_size
        size = (max_width - FIT_EPSILON) / coef
        if max_height is not None:
            vertical = (len(lines) - 1) * line_height + stack[0].ascent + stack[0].descent
            if vertical > 0:
                size = min(size, (max_height - FIT_EPSILON) / vertical)
        return min(max_size, size)

    # -- font embedding -----------------------------------------------------
    def font_face_css(self) -> str:
        rules = []
        for family, _filename, weight in FONT_SOURCES:
            face = self.faces[family]
            rules.append(
                "@font-face{font-family:'%s';font-style:normal;font-weight:%d;"
                "src:url(data:font/woff2;base64,%s) format('woff2');}"
                % (face.family, weight, face.data_b64)
            )
        return "".join(rules)


class Artboard:
    """One SVG artboard: flat shapes, fitted type, one grain filter."""

    def __init__(
        self,
        filename: str,
        width: int,
        height: int,
        base: str,
        title: str,
        desc: str,
        ts: TypeSystem,
    ) -> None:
        self.filename = filename
        self.width = width
        self.height = height
        self.base = base
        self.title = title
        self.desc = desc
        self.ts = ts
        self.elements: list[str] = []
        self.texts: list[dict] = []
        self.clear_rects: list[tuple[float, float, float, float]] = []

    # -- primitives ---------------------------------------------------------
    def rect(self, x, y, w, h, fill, name: str | None = None) -> None:
        attrs = f'x="{x:g}" y="{y:g}" width="{w:g}" height="{h:g}" fill="{fill}"'
        if name:
            attrs += f' data-name="{name}"'
        self.elements.append(f"<rect {attrs}/>")

    def path(self, d: str, stroke: str, stroke_width: float, name: str | None = None) -> None:
        attrs = (
            f'd="{d}" fill="none" stroke="{stroke}" '
            f'stroke-width="{stroke_width:g}" stroke-linecap="butt"'
        )
        if name:
            attrs += f' data-name="{name}"'
        self.elements.append(f"<path {attrs}/>")

    def polygon(self, d: str, fill: str, name: str | None = None) -> None:
        attrs = f'd="{d}" fill="{fill}"'
        if name:
            attrs += f' data-name="{name}"'
        self.elements.append(f"<path {attrs}/>")

    def circle(self, cx, cy, r, fill, name: str | None = None) -> None:
        attrs = f'cx="{cx:g}" cy="{cy:g}" r="{r:g}" fill="{fill}"'
        if name:
            attrs += f' data-name="{name}"'
        self.elements.append(f"<circle {attrs}/>")

    def starburst(self, cx, cy, r_max, color, rays=48, stroke_width=2.0) -> None:
        """Flat lens-flare burst: straight rays from a single point."""
        pattern = (1.0, 0.48, 0.74, 0.40, 0.92, 0.58, 0.84, 0.36)
        for index in range(rays):
            angle = 2.0 * math.pi * index / rays
            radius = r_max * pattern[index % len(pattern)]
            x2 = cx + radius * math.cos(angle)
            y2 = cy + radius * math.sin(angle)
            self.path(
                f"M {cx:.1f} {cy:.1f} L {x2:.1f} {y2:.1f}",
                color,
                stroke_width,
                name="burst-ray",
            )
        self.circle(cx, cy, stroke_width * 2.5, color, name="burst-core")

    # -- type ---------------------------------------------------------------
    def _text_element(
        self,
        x: float,
        y: float,
        text: str,
        families,
        size: float,
        fill: str,
        box,
        ls_em: float,
        anchor: str,
    ) -> None:
        stack = self.ts.stack(families)
        ls_px = ls_em * size
        runs = self._runs(text, stack)

        box_x0, box_y0, box_x1, box_y1 = box
        attrs = [
            f'x="{x:g}"',
            f'y="{y:g}"',
            f'font-family={quoteattr(self.ts.css_stack(families))}',
            f'font-size="{size:.3f}"',
            f'font-weight="{stack[0].weight}"',
            f'fill="{fill}"',
            f'text-anchor="{anchor}"',
            f'data-box="{box_x0:g} {box_y0:g} {box_x1:g} {box_y1:g}"',
        ]
        if abs(ls_px) > 0.0005:
            attrs.append(f'letter-spacing="{ls_px:.3f}"')

        if len(runs) == 1:
            body = escape(text)
        else:
            chunks = []
            for face, run_text in runs:
                chunks.append(
                    "<tspan font-family=%s font-weight=\"%d\">%s</tspan>"
                    % (quoteattr(f"'{face.family}'"), face.weight, escape(run_text))
                )
            body = "".join(chunks)

        self.elements.append(f"<text {' '.join(attrs)}>{body}</text>")
        self.texts.append(
            {
                "text": text,
                "x": x,
                "y": y,
                "size": size,
                "ls_px": ls_px,
                "anchor": anchor,
                "box": box,
                "families": families,
                "ascent": stack[0].ascent,
                "descent": stack[0].descent,
                "width": sum(
                    self.ts.width_em(run_text, [self.ts.faces[face.family]])
                    for face, run_text in runs
                )
                * size
                + ls_px
                * self.ts._gap_count(len(text), 1.0 if ls_px > 0 else -1.0),
            }
        )

    def check_clear_rects(self) -> list[str]:
        """Accent fields declared clear of the type must not touch any text."""
        problems: list[str] = []
        for rect in self.clear_rects:
            rx0, ry0, rx1, ry1 = rect
            for record in self.texts:
                width = record["width"]
                if record["anchor"] == "end":
                    left, right = record["x"] - width, record["x"]
                elif record["anchor"] == "middle":
                    left, right = record["x"] - width / 2.0, record["x"] + width / 2.0
                else:
                    left, right = record["x"], record["x"] + width
                top = record["y"] - record["ascent"] * record["size"]
                bottom = record["y"] + record["descent"] * record["size"]
                if left < rx1 and right > rx0 and top < ry1 and bottom > ry0:
                    problems.append(
                        f"{self.filename}: accent field "
                        f"({rx0:g},{ry0:g})-({rx1:g},{ry1:g}) collides with text "
                        f"{record['text']!r} ({left:.1f},{top:.1f})-({right:.1f},{bottom:.1f})"
                    )
        return problems

    def _runs(self, text: str, stack: list[Face]):
        runs: list[tuple[Face, str]] = []
        for char in text:
            face = self.ts.resolve(char, stack)
            if face is None:
                raise KeyError(
                    f"glyph U+{ord(char):04X} ({char!r}) is not in any of "
                    f"{[f.family for f in stack]}"
                )
            if runs and runs[-1][0] is face:
                runs[-1] = (face, runs[-1][1] + char)
            else:
                runs.append((face, char))
        return runs

    def display(
        self,
        x: float,
        y: float,
        w: float,
        h: float,
        lines,
        fill: str,
        families=DISPLAY,
        max_size: float = 460.0,
        line_height: float = 0.86,
        ls_em: float = DISPLAY_LS,
    ) -> float:
        """Fit a block of display lines into the box; returns the block bottom."""
        stack = self.ts.stack(families)
        size = self.ts.fit_size(
            lines, stack, max_size, w, ls_em, max_height=h, line_height=line_height
        )
        baseline = y + stack[0].ascent * size
        box = (x, y, x + w, y + h)
        for index, line in enumerate(lines):
            line_baseline = baseline + index * line_height * size
            self._text_element(
                x, line_baseline, line, families, size, fill, box, ls_em, "start"
            )
        last_baseline = baseline + (len(lines) - 1) * line_height * size
        return last_baseline + stack[0].descent * size

    def display_ratio(
        self,
        x: float,
        y: float,
        w: float,
        h: float,
        left: str,
        right: str,
        fill: str,
        families=DISPLAY,
        max_size: float = 460.0,
        ls_em: float = DISPLAY_LS,
    ) -> float:
        """A ``10/10`` style line: two display words around a flat slash.

        Druk Wide carries no U+002F, and the Departure Mono fallback draws a
        pixel staircase that breaks at display size, so the slash is a flat
        geometric parallelogram instead.
        """
        stack = self.ts.stack(families)
        # em budget for the slash: leading gap + stroke + lean + trailing gap
        slash_em = 0.02 + 0.22 + 0.20 + 0.08
        coef = (
            self.ts.width_coef(left, stack, ls_em)
            + slash_em
            + self.ts.width_coef(right, stack, ls_em)
        )
        size = min(max_size, (w - FIT_EPSILON) / coef)
        vertical = stack[0].ascent + stack[0].descent
        box_height = h - FIT_EPSILON
        size = min(size, box_height / vertical)

        baseline = y + stack[0].ascent * size
        cap = stack[0].cap_height * size
        left_w = self.ts.measure(left, stack, size, ls_em * size)
        right_w = self.ts.measure(right, stack, size, ls_em * size)
        stroke = 0.22 * size
        lean = 0.20 * size
        slash_x = x + left_w + 0.02 * size

        self.polygon(
            "M %.1f %.1f L %.1f %.1f L %.1f %.1f L %.1f %.1f Z"
            % (
                slash_x,
                baseline,
                slash_x + stroke,
                baseline,
                slash_x + stroke + lean,
                baseline - cap,
                slash_x + lean,
                baseline - cap,
            ),
            fill,
            name="display-slash",
        )
        right_x = slash_x + stroke + lean + 0.08 * size
        self._text_element(
            x,
            baseline,
            left,
            families,
            size,
            fill,
            (x, y, x + left_w + 0.02 * size, y + box_height),
            ls_em,
            "start",
        )
        self._text_element(
            right_x,
            baseline,
            right,
            families,
            size,
            fill,
            (right_x, y, right_x + right_w, y + box_height),
            ls_em,
            "start",
        )
        return baseline + stack[0].descent * size

    def label(
        self,
        x: float,
        y: float,
        text: str,
        size: float,
        fill: str,
        box,
        families=BODY,
        ls_em: float = 0.0,
        anchor: str = "start",
    ) -> None:
        """One fitted line of type; ``y`` is the baseline."""
        stack = self.ts.stack(families)
        box_x0, box_y0, box_x1, box_y1 = box
        width = self.ts.measure(text, stack, size, ls_em * size)
        if width > (box_x1 - box_x0) + 0.5:
            fitted = self.ts.fit_size([text], stack, size, box_x1 - box_x0, ls_em)
            size = fitted
        self._text_element(x, y, text, families, size, fill, box, ls_em, anchor)

    def micro(self, corner: str, text: str, fill: str, margin: float, size: float, top: float, bottom: float) -> None:
        """Pin a GT Pressura Mono micro tag to a corner."""
        stack = self.ts.stack(MICRO)
        ascent = stack[0].ascent * size
        descent = stack[0].descent * size
        half = (self.width - 2 * margin) * 0.5
        if corner == "tl":
            x, y, anchor, box = margin, top, "start", (margin, top - ascent, margin + half, top + descent)
        elif corner == "tr":
            x, y, anchor, box = self.width - margin, top, "end", (self.width - margin - half, top - ascent, self.width - margin, top + descent)
        elif corner == "bl":
            x, y, anchor, box = margin, bottom, "start", (margin, bottom - ascent, margin + half, bottom + descent)
        elif corner == "br":
            x, y, anchor, box = self.width - margin, bottom, "end", (self.width - margin - half, bottom - ascent, self.width - margin, bottom + descent)
        else:
            raise ValueError(f"unknown corner {corner!r}")
        self._text_element(x, y, text, MICRO, size, fill, box, MICRO_LS, anchor)

    def clear_rect(self, x, y, w, h) -> None:
        """Register an accent field that must not collide with any type."""
        self.clear_rects.append((x, y, x + w, y + h))

    # -- render -------------------------------------------------------------
    def render(self) -> str:
        style = self.ts.font_face_css()
        parts = [
            '<?xml version="1.0" encoding="UTF-8"?>',
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{self.width}" '
            f'height="{self.height}" viewBox="0 0 {self.width} {self.height}" '
            'role="img" aria-labelledby="t d">',
            f"<title id=\"t\">{escape(self.title)}</title>",
            f"<desc id=\"d\">{escape(self.desc)}</desc>",
            f"<style>{style}</style>",
            f'<rect x="0" y="0" width="{self.width}" height="{self.height}" '
            f'fill="{self.base}"/>',
        ]
        parts.extend(self.elements)
        parts.append(
            '<filter id="grain" x="0" y="0" width="100%" height="100%" '
            'color-interpolation-filters="sRGB">'
            '<feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" '
            'stitchTiles="stitch" seed="7"/>'
            '<feColorMatrix type="saturate" values="0"/>'
            "</filter>"
        )
        parts.append(
            f'<rect x="0" y="0" width="{self.width}" height="{self.height}" '
            'fill="#FFFFFF" filter="url(#grain)" '
            f'opacity="{GRAIN_OPACITY:g}" style="mix-blend-mode:multiply"/>'
        )
        parts.append("</svg>")
        return "\n".join(parts) + "\n"


# ===========================================================================
# Posters - 1920 x 1080
# ===========================================================================

PW, PH = 1920, 1080
PM = 96  # content margin
PMICRO = 64  # micro tag margin
PMICRO_SIZE = 20
PSIZE = 30  # body size


def poster_01(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "01-local-browser.svg",
        PW,
        PH,
        PAPER,
        "Walkaway - A local intelligent browser",
        "Poster 01 of 10. Flat paper poster. Giant ink display reads A LOCAL / "
        "INTELLIGENT / BROWSER. under it the body line RUNS ON YOUR MACHINE. and "
        "the micro line YOUR SECRETS - YOUR SESSIONS - YOUR NETWORK.",
        ts,
    )
    bottom = a.display(PM, 210, PW - 2 * PM, 560, ["A LOCAL", "INTELLIGENT", "BROWSER."], INK)
    a.label(
        PM,
        bottom + 80,
        "RUNS ON YOUR MACHINE.",
        PSIZE,
        INK,
        (PM, bottom + 80 - PSIZE, PM + 900, bottom + 80 + 12),
    )
    a.label(
        PM,
        bottom + 138,
        "YOUR SECRETS · YOUR SESSIONS · YOUR NETWORK",
        PMICRO_SIZE,
        INK,
        (PM, bottom + 138 - 16, PM + 900, bottom + 138 + 6),
        families=MICRO,
        ls_em=MICRO_LS,
    )
    a.micro("tl", "WALKAWAY", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("tr", "01 / 10", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


def poster_02(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "02-walkaway.svg",
        PW,
        PH,
        INK,
        "Walkaway - the name",
        "Poster 02 of 10. Flat black poster. Giant paper display WALKAWAY across "
        "the upper half above a green square centred on the lower half holding a "
        "cream starburst of thin straight rays.",
        ts,
    )
    a.display(PM, 200, PW - 2 * PM, 260, ["WALKAWAY"], PAPER)
    a.rect(650, 460, 620, 620, GREEN, name="accent-square")
    a.starburst(960, 770, 270, PAPER)
    a.micro("tl", "META MUSE × WALKAWAY BLITZ", PAPER, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("bl", "A LOCAL INTELLIGENT BROWSER", PAPER, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("br", "02 / 10", PAPER, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


def poster_03(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "03-reachable.svg",
        PW,
        PH,
        PAPER,
        "Walkaway - reachable from your phone",
        "Poster 03 of 10. Flat paper poster. Giant ink display reads YOUR PHONE / "
        "TALKS TO IT. with the body line REMOTE ASSISTANCE, DIRECTLY. and a cyan "
        "square in the empty right area.",
        ts,
    )
    bottom = a.display(PM, 240, PW - 2 * PM, 340, ["YOUR PHONE", "TALKS TO IT."], INK)
    a.label(
        PM,
        bottom + 80,
        "REMOTE ASSISTANCE, DIRECTLY.",
        PSIZE,
        INK,
        (PM, bottom + 80 - PSIZE, PM + 1200, bottom + 80 + 12),
    )
    a.rect(1480, 660, 320, 320, CYAN, name="accent-square")
    a.clear_rect(1480, 660, 320, 320)
    a.micro("bl", "LOCAL INTELLIGENT BROWSER", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("br", "03 / 10", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


def poster_04(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "04-one-tap.svg",
        PW,
        PH,
        ORANGE,
        "Walkaway - peace of mind in one tap",
        "Poster 04 of 10. Flat orange poster. Giant ink display reads PEACE OF MIND. "
        "/ IN ONE TAP. with the body line ONE TRUSTED LINE, THROUGH YOUR GLASSES.",
        ts,
    )
    bottom = a.display(PM, 210, PW - 2 * PM, 420, ["PEACE OF MIND.", "IN ONE TAP."], INK)
    a.label(
        PM,
        bottom + 80,
        "ONE TRUSTED LINE, THROUGH YOUR GLASSES.",
        PSIZE,
        INK,
        (PM, bottom + 80 - PSIZE, PM + 1200, bottom + 80 + 12),
    )
    a.micro("tl", "WALKAWAY", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("br", "04 / 10", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


def poster_05(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "05-composes.svg",
        PW,
        PH,
        GREEN,
        "Walkaway - it composes, fetches and finishes",
        "Poster 05 of 10. Flat green poster. Giant paper display reads IT FETCHES. "
        "/ IT PLANS. / IT FINISHES. with the body line LONG-RUNNING, COMPLEX WORK.",
        ts,
    )
    bottom = a.display(PM, 200, PW - 2 * PM, 560, ["IT FETCHES.", "IT PLANS.", "IT FINISHES."], PAPER)
    a.label(
        PM,
        bottom + 80,
        "LONG-RUNNING, COMPLEX WORK.",
        PSIZE,
        PAPER,
        (PM, bottom + 80 - PSIZE, PM + 1000, bottom + 80 + 12),
    )
    a.micro("tl", "COMPOSES", PAPER, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("br", "05 / 10", PAPER, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


def poster_06(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "06-secrets.svg",
        PW,
        PH,
        PAPER,
        "Walkaway - unlimited access to your secrets",
        "Poster 06 of 10. Flat paper poster. Giant ink display reads IT HAS / YOUR / "
        "SECRETS. with the body line THAT'S THE POINT. and a solar square in the "
        "empty right area.",
        ts,
    )
    bottom = a.display(PM, 210, 1650, 540, ["IT HAS", "YOUR", "SECRETS."], INK)
    a.label(
        PM,
        bottom + 80,
        "THAT'S THE POINT.",
        PSIZE,
        INK,
        (PM, bottom + 80 - PSIZE, PM + 700, bottom + 80 + 12),
    )
    a.rect(1520, 210, 300, 300, SOLAR, name="accent-square")
    a.clear_rect(1520, 210, 300, 300)
    a.micro("tl", "UNLIMITED ACCESS", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("br", "06 / 10", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


def poster_07(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "07-coffee.svg",
        PW,
        PH,
        INK,
        "Walkaway - go make coffee",
        "Poster 07 of 10. Flat black poster. Giant paper display reads GO MAKE / "
        "COFFEE. with the body line IT KEEPS WORKING. and an orange square on the "
        "right, clear of the type.",
        ts,
    )
    bottom = a.display(PM, 240, 1330, 420, ["GO MAKE", "COFFEE."], PAPER)
    a.label(
        PM,
        bottom + 80,
        "IT KEEPS WORKING.",
        PSIZE,
        PAPER,
        (PM, bottom + 80 - PSIZE, PM + 700, bottom + 80 + 12),
    )
    a.rect(1484, 240, 340, 340, ORANGE, name="accent-square")
    a.clear_rect(1484, 240, 340, 340)
    a.micro("tl", "WALKAWAY", PAPER, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("br", "07 / 10", PAPER, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


def poster_08(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "08-manifest.svg",
        PW,
        PH,
        PAPER,
        "Walkaway - manifest first",
        "Poster 08 of 10. Flat paper poster. Giant ink display reads MANIFEST / "
        "FIRST. above four quiet mono lines: PROPOSAL ID, FINGERPRINT, GENERATION, "
        "FORM ID.",
        ts,
    )
    bottom = a.display(PM, 200, PW - 2 * PM, 420, ["MANIFEST", "FIRST."], INK)
    for index, entry in enumerate(("PROPOSAL ID", "FINGERPRINT", "GENERATION", "FORM ID")):
        baseline = bottom + 80 + index * 56
        a.label(
            PM,
            baseline,
            entry,
            26,
            INK,
            (PM, baseline - 20, PM + 400, baseline + 8),
            families=MICRO,
            ls_em=MICRO_LS,
        )
    a.micro("tl", "BEFORE ANY COMMIT", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("br", "08 / 10", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


def poster_09(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "09-ten-of-ten.svg",
        PW,
        PH,
        CYAN,
        "Walkaway - ten of ten",
        "Poster 09 of 10. Flat cyan poster. Giant ink display reads 10/10 with the "
        "body line SUBMITTED.",
        ts,
    )
    bottom = a.display_ratio(PM, 240, PW - 2 * PM, 400, "10", "10", INK)
    a.label(
        PM,
        bottom + 80,
        "SUBMITTED.",
        PSIZE,
        INK,
        (PM, bottom + 80 - PSIZE, PM + 400, bottom + 80 + 12),
    )
    a.micro("tl", "ONE RUN", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("br", "09 / 10", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


def poster_10(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "10-broken-form.svg",
        PW,
        PH,
        PAPER,
        "Walkaway - never see a broken form again",
        "Poster 10 of 10. Flat paper poster. Giant ink display reads NEVER SEE A / "
        "BROKEN FORM / AGAIN. with the body line IT FILLS THEM IN FOR YOU. and a "
        "green square in the empty right area.",
        ts,
    )
    bottom = a.display(PM, 220, 1400, 540, ["NEVER SEE A", "BROKEN FORM", "AGAIN."], INK)
    a.label(
        PM,
        bottom + 80,
        "IT FILLS THEM IN FOR YOU.",
        PSIZE,
        INK,
        (PM, bottom + 80 - PSIZE, PM + 1200, bottom + 80 + 12),
    )
    a.rect(1500, 640, 300, 300, GREEN, name="accent-square")
    a.clear_rect(1500, 640, 300, 300)
    a.micro("tl", "WALKAWAY", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    a.micro("br", "10 / 10", INK, PMICRO, PMICRO_SIZE, PMICRO + PMICRO_SIZE, PH - PMICRO - 4)
    return a


# ===========================================================================
# Banners - ad aspect ratios
# ===========================================================================


def banner_01(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "01-feed.svg",
        1200,
        628,
        PAPER,
        "Walkaway - a local intelligent browser",
        "Feed banner 01 of 06. Flat paper banner. Giant ink display reads A LOCAL / "
        "INTELLIGENT / BROWSER. with a green square on the right.",
        ts,
    )
    bottom = a.display(60, 140, 760, 300, ["A LOCAL", "INTELLIGENT", "BROWSER."], INK)
    a.label(
        60,
        bottom + 50,
        "RUNS ON YOUR MACHINE.",
        20,
        INK,
        (60, bottom + 30, 60 + 700, bottom + 58),
    )
    a.rect(850, 160, 300, 300, GREEN, name="accent-square")
    a.clear_rect(850, 160, 300, 300)
    a.micro("tl", "WALKAWAY", INK, 40, 13, 53, 584)
    a.micro("br", "01 / 06", INK, 40, 13, 53, 584)
    return a


def banner_02(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "02-square.svg",
        1080,
        1080,
        INK,
        "Walkaway - peace of mind in one tap",
        "Square banner 02 of 06. Flat black banner. Giant paper display reads ONE "
        "TAP. / PEACE OF MIND. with a cyan square.",
        ts,
    )
    bottom = a.display(60, 230, 960, 420, ["ONE TAP.", "PEACE OF MIND."], PAPER)
    a.label(
        60,
        bottom + 50,
        "ONE TRUSTED LINE, THROUGH YOUR GLASSES.",
        22,
        PAPER,
        (60, bottom + 28, 60 + 900, bottom + 60),
    )
    a.rect(680, 620, 320, 320, CYAN, name="accent-square")
    a.clear_rect(680, 620, 320, 320)
    a.micro("tl", "WALKAWAY", PAPER, 40, 18, 58, 1036)
    a.micro("br", "02 / 06", PAPER, 40, 18, 58, 1036)
    return a


def banner_03(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "03-story.svg",
        1080,
        1920,
        ORANGE,
        "Walkaway - go make coffee",
        "Story banner 03 of 06. Flat orange banner. Giant ink display reads GO MAKE "
        "/ COFFEE. with a solar square.",
        ts,
    )
    bottom = a.display(72, 460, 936, 520, ["GO MAKE", "COFFEE."], INK)
    a.label(
        72,
        bottom + 60,
        "IT KEEPS WORKING.",
        26,
        INK,
        (72, bottom + 34, 72 + 700, bottom + 72),
    )
    a.rect(350, 1120, 380, 380, SOLAR, name="accent-square")
    a.clear_rect(350, 1120, 380, 380)
    a.micro("tl", "WALKAWAY", INK, 48, 20, 68, 1868)
    a.micro("br", "03 / 06", INK, 48, 20, 68, 1868)
    return a


def banner_04(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "04-display.svg",
        300,
        250,
        PAPER,
        "Walkaway - your secrets",
        "Display banner 04 of 06. Flat paper banner. Giant ink display reads YOUR / "
        "SECRETS. with a small green square in the top-right corner.",
        ts,
    )
    bottom = a.display(20, 56, 210, 130, ["YOUR", "SECRETS."], INK)
    a.rect(240, 14, 48, 48, GREEN, name="accent-square")
    a.clear_rect(240, 14, 48, 48)
    a.micro("tl", "WALKAWAY", INK, 12, 7, 19, 236)
    a.micro("br", "04 / 06", INK, 12, 7, 19, 236)
    return a


def banner_05(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "05-feed.svg",
        1200,
        628,
        GREEN,
        "Walkaway - it composes, fetches and finishes",
        "Feed banner 05 of 06. Flat green banner. Giant paper display reads IT "
        "FETCHES. / IT PLANS. / IT FINISHES.",
        ts,
    )
    bottom = a.display(60, 140, 1080, 360, ["IT FETCHES.", "IT PLANS.", "IT FINISHES."], PAPER)
    a.label(
        60,
        bottom + 50,
        "LONG-RUNNING, COMPLEX WORK.",
        20,
        PAPER,
        (60, bottom + 30, 60 + 900, bottom + 58),
    )
    a.micro("tl", "WALKAWAY", PAPER, 40, 13, 53, 584)
    a.micro("br", "05 / 06", PAPER, 40, 13, 53, 584)
    return a


def banner_06(ts: TypeSystem) -> Artboard:
    a = Artboard(
        "06-square.svg",
        1080,
        1080,
        CYAN,
        "Walkaway - ten of ten",
        "Square banner 06 of 06. Flat cyan banner. Giant ink display reads 10/10 "
        "with the body line SUBMITTED.",
        ts,
    )
    bottom = a.display_ratio(60, 340, 960, 420, "10", "10", INK)
    a.label(
        60,
        bottom + 80,
        "SUBMITTED.",
        26,
        INK,
        (60, bottom + 54, 60 + 600, bottom + 90),
    )
    a.micro("tl", "ONE RUN", INK, 40, 18, 58, 1036)
    a.micro("br", "06 / 06", INK, 40, 18, 58, 1036)
    return a


POSTERS = (
    ("01-local-browser.svg", poster_01),
    ("02-walkaway.svg", poster_02),
    ("03-reachable.svg", poster_03),
    ("04-one-tap.svg", poster_04),
    ("05-composes.svg", poster_05),
    ("06-secrets.svg", poster_06),
    ("07-coffee.svg", poster_07),
    ("08-manifest.svg", poster_08),
    ("09-ten-of-ten.svg", poster_09),
    ("10-broken-form.svg", poster_10),
)

BANNERS = (
    ("01-feed.svg", banner_01),
    ("02-square.svg", banner_02),
    ("03-story.svg", banner_03),
    ("04-display.svg", banner_04),
    ("05-feed.svg", banner_05),
    ("06-square.svg", banner_06),
)


# ===========================================================================
# Verification - re-reads the written file and measures every text node
# ===========================================================================

SVG_NS = "{http://www.w3.org/2000/svg}"


def parse_family_stack(value: str, ts: TypeSystem) -> list[Face]:
    names = []
    for part in value.split(","):
        part = part.strip().strip("'\"").strip()
        if part in ts.faces:
            names.append(part)
    if not names:
        raise ValueError(f"no known font family in {value!r}")
    return [ts.faces[name] for name in names]


def verify_file(path: str, width: int, height: int, ts: TypeSystem) -> list[str]:
    """Re-parse a written SVG and prove every text node fits its box."""
    problems: list[str] = []
    source = open(path, encoding="utf-8").read()
    name = os.path.basename(path)

    try:
        root = ET.fromstring(source)
    except ET.ParseError as error:
        return [f"{name}: not well-formed XML: {error}"]

    if root.tag != SVG_NS + "svg":
        problems.append(f"{name}: root element is {root.tag!r}, expected an <svg>")
    if root.get("width") != str(width) or root.get("height") != str(height):
        problems.append(
            f"{name}: canvas is {root.get('width')}x{root.get('height')}, "
            f"expected {width}x{height}"
        )
    if root.get("viewBox") != f"0 0 {width} {height}":
        problems.append(f"{name}: viewBox is {root.get('viewBox')!r}")

    titles = root.findall(SVG_NS + "title")
    descs = root.findall(SVG_NS + "desc")
    if not titles or not (titles[0].text or "").strip():
        problems.append(f"{name}: missing a non-empty <title>")
    if not descs or not (descs[0].text or "").strip():
        problems.append(f"{name}: missing a non-empty <desc>")

    if "<script" in source.lower():
        problems.append(f"{name}: contains a <script>")
    scanned = source.replace('xmlns="http://www.w3.org/2000/svg"', "")
    for token in ("http://", "https://", "xlink:href", "<image", "linearGradient", "radialGradient"):
        if token in scanned:
            problems.append(f"{name}: contains a forbidden reference/token {token!r}")
    for match in scanned.split("url(")[1:]:
        target = match.split(")")[0]
        if not (target.startswith("#") or target.startswith("data:")):
            problems.append(f"{name}: external url() reference {target!r}")
    if "filter=" in source and source.count("<filter") != 1:
        problems.append(f"{name}: expected exactly one <filter>, found {source.count('<filter')}")
    if "feTurbulence" not in source:
        problems.append(f"{name}: missing the grain feTurbulence filter")
    if "mix-blend-mode:multiply" not in source:
        problems.append(f"{name}: grain overlay is missing mix-blend-mode:multiply")
    for family, _filename, _weight in FONT_SOURCES:
        if f"font-family:'{family}'" not in source:
            problems.append(f"{name}: @font-face for {family} is not embedded")
    if source.count("data:font/woff2;base64,") != len(FONT_SOURCES):
        problems.append(f"{name}: expected {len(FONT_SOURCES)} embedded woff2 payloads")

    text_nodes = root.findall(f".//{SVG_NS}text")
    if not text_nodes:
        problems.append(f"{name}: no <text> nodes")

    for node in text_nodes:
        label = "".join(node.itertext()).strip()
        if not label:
            problems.append(f"{name}: empty <text> node")
            continue
        try:
            size = float(node.get("font-size", "0"))
            x = float(node.get("x", "0"))
            y = float(node.get("y", "0"))
            ls_px = float(node.get("letter-spacing", "0"))
            anchor = node.get("text-anchor", "start")
            box_values = [float(v) for v in node.get("data-box", "").split()]
            parent_stack = parse_family_stack(node.get("font-family", ""), ts)
        except (TypeError, ValueError) as error:
            problems.append(f"{name}: unusable <text> attributes on {label!r}: {error}")
            continue

        if len(box_values) != 4:
            problems.append(f"{name}: <text> {label!r} has no usable data-box")
            continue
        box_x0, box_y0, box_x1, box_y1 = box_values

        # per-run font resolution (the fallback model the renderer applies)
        width_em = 0.0
        runs = []
        if len(node) == 0:
            runs = [(label, parent_stack)]
        else:
            for child in node:
                child_stack = parse_family_stack(child.get("font-family", ""), ts)
                runs.append(("".join(child.itertext()), child_stack))
        for run_text, stack in runs:
            for char in run_text:
                face = ts.resolve(char, stack)
                if face is None:
                    problems.append(
                        f"{name}: glyph U+{ord(char):04X} in {label!r} is in no "
                        f"embedded face"
                    )
                    face = stack[0]
                width_em += face.advance_em(char)

        gaps = ts._gap_count(len(label), 1.0 if ls_px > 0 else -1.0)
        text_width = width_em * size + ls_px * gaps

        if text_width > (box_x1 - box_x0) + 0.5:
            problems.append(
                f"{name}: text overflows its box: {label!r} measured "
                f"{text_width:.1f}px > box {(box_x1 - box_x0):.1f}px"
            )

        if anchor == "end":
            left, right = x - text_width, x
        elif anchor == "middle":
            left, right = x - text_width / 2.0, x + text_width / 2.0
        else:
            left, right = x, x + text_width
        if left < box_x0 - 0.5 or right > box_x1 + 0.5:
            problems.append(
                f"{name}: text {label!r} leaves its box horizontally "
                f"({left:.1f}..{right:.1f} vs {box_x0:g}..{box_x1:g})"
            )

        ascent = parent_stack[0].ascent
        descent = parent_stack[0].descent
        top = y - ascent * size
        bottom = y + descent * size
        if top < box_y0 - 0.5 or bottom > box_y1 + 0.5:
            problems.append(
                f"{name}: text {label!r} leaves its box vertically "
                f"({top:.1f}..{bottom:.1f} vs {box_y0:g}..{box_y1:g})"
            )
        if left < -0.5 or right > width + 0.5 or top < -0.5 or bottom > height + 0.5:
            problems.append(f"{name}: text {label!r} leaves the {width}x{height} canvas")

    return problems


def main() -> int:
    ts = TypeSystem()
    os.makedirs(POSTER_DIR, exist_ok=True)
    os.makedirs(BANNER_DIR, exist_ok=True)

    written: list[tuple[str, int]] = []
    problems: list[str] = []

    for filename, builder in POSTERS:
        artboard = builder(ts)
        path = os.path.join(POSTER_DIR, filename)
        with open(path, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(artboard.render())
        written.append((path, os.path.getsize(path)))
        problems.extend(artboard.check_clear_rects())
        problems.extend(verify_file(path, artboard.width, artboard.height, ts))

    for filename, builder in BANNERS:
        artboard = builder(ts)
        path = os.path.join(BANNER_DIR, filename)
        with open(path, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(artboard.render())
        written.append((path, os.path.getsize(path)))
        problems.extend(artboard.check_clear_rects())
        problems.extend(verify_file(path, artboard.width, artboard.height, ts))

    for path, size in written:
        print(f"[ok] {os.path.relpath(path, ROOT)}  {size} bytes")

    # Remove stale artboards. A renamed creative would otherwise leave the old
    # file behind, and the page can end up serving a deleted concept.
    expected = {filename for filename, _ in POSTERS}
    for stale in sorted(set(os.listdir(POSTER_DIR)) - expected):
        stale_path = os.path.join(POSTER_DIR, stale)
        if os.path.isfile(stale_path):
            os.remove(stale_path)
            print(f"[ok] removed stale artboard: {os.path.relpath(stale_path, ROOT)}")
    expected = {filename for filename, _ in BANNERS}
    for stale in sorted(set(os.listdir(BANNER_DIR)) - expected):
        stale_path = os.path.join(BANNER_DIR, stale)
        if os.path.isfile(stale_path):
            os.remove(stale_path)
            print(f"[ok] removed stale artboard: {os.path.relpath(stale_path, ROOT)}")

    print(f"[ok] generated {len(written)} artboards, {len(POSTERS)} posters + {len(BANNERS)} banners")

    if problems:
        for problem in problems:
            print(f"[FAIL] {problem}")
        print(f"\nFAILED: {len(problems)} problem(s)")
        return 1

    print("[ok] every text node measured inside its declared box")
    print("\nPASS: poster and banner set generated and verified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
