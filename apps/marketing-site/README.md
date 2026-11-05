# Marketing site — Walkaway × Muse Blitz

Static marketing surface for **Walkaway × Muse Blitz — Supervised Batch Browser &
Glasses Commit Gate**. It is an App Store-style promotional page: hero, story,
security model, film, downloadable asset kit, and paid ad placements.

There is **no build step**. `index.html` plus `assets/` is the deployable unit.

```
apps/marketing-site/
├── index.html              # the whole page, including 6 inline SVG ad creatives
├── README.md
└── assets/
    ├── css/site.css        # design system + every component
    ├── js/site.js          # accordion, reveal, slot filter, SVG export
    ├── fonts/*.woff2       # derived from locally installed system fonts
    ├── img/*.webp          # page assets (optimised)
    └── download/*          # full-resolution originals, offered as downloads
```

## Design language

A solarpunk / techno-utopian riff on the Meta Muse page: centred poster
headlines, generous whitespace, rounded cards, a dark "night" palette with a
solar yellow, a signal blue and a bio green, and a paper-white band for the
asset kit and ad placements.

| Token | Value | Source |
|---|---|---|
| `--solar` | `#ffc400` | yellow mascot icon |
| `--signal` | `#0a84ff` | blue mascot icon |
| `--bio` | `#7ed321` | supervision mark |
| `--ember` | `#ff7a1a` | device sync icon |
| `--paper` | `#f3f0e6` | solarised paper |

Type:

- **Druk Wide** — display headings (`--display`)
- **GT Pressura Mono** — eyebrows, labels, data (`--mono`)
- **Departure Mono** — body copy (`--body`)

Druk Wide and GT Pressura Mono ship narrow charsets, so Departure Mono is listed
as a per-glyph fallback in both stacks: `×`, `·`, `→` and `↓` resolve to
Departure Mono rather than silently dropping.

## Rebuilding the web fonts

The source faces live in `~/Library/Fonts` and are **not** committed. Convert
them to `woff2` subsets with:

```bash
python3 -m venv /tmp/fontenv
/tmp/fontenv/bin/pip install fonttools brotli
bash scripts/build-web-fonts.sh
```

`scripts/build-web-fonts.sh` writes `assets/fonts/*.woff2` and prints the
resulting sizes.

## Verification

```bash
python3 scripts/verify-marketing-site.py          # asset + font + glyph checks
PY=/tmp/fontenv/bin/python python3 scripts/verify-marketing-site.py   # + banner metrics
```

Checks every local reference in `index.html`, every `@font-face` URL, that no
banner `<text>` node overflows its SVG canvas using the real font advance
widths, and that every non-ASCII character on the page is covered by at least
one shipped font.

Local preview (port `8099` was confirmed unused first, and it is not part of
the demo path):

```bash
cd apps/marketing-site && python3 -m http.server 8099 --bind 127.0.0.1
```

## Adding the demo film

The film section currently ships as a poster frame. When the 3-minute master is
final, drop it at `assets/video/walkaway-blitz-3min.mp4` and replace the
`.frame-inner` block in the `#film` section with a `<video>` element that
keeps the existing 16:9 frame styling.

## Ad creatives

The six paid placements are **inline SVG** in `index.html` rather than raster
files, so they stay crisp at any size and can be exported at true pixel
dimensions. "Download SVG" serialises the live SVG and embeds the `woff2`
faces as data URIs, producing a self-contained file that opens correctly
anywhere. To export as PNG, open the exported SVG and rasterise at its
`viewBox` size (1200×628, 1080×1080, 1080×1920, 300×250).

Type sizes in the creatives were set from measured advance widths, not by eye —
re-run `scripts/verify-marketing-site.py` after editing any `<text>` node.
