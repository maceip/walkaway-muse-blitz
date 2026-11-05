# Marketing site: Walkaway

Static marketing surface for **Walkaway**, a local intelligent browser. It is a
single-page, App Store-style promotional site: hero, product statements, film
frame, a poster set, paid ad placements, and a downloadable asset kit.

There is **no build step**. `index.html` plus `assets/` is the deployable unit.

```
apps/marketing-site/
├── index.html              # the whole page
├── README.md
└── assets/
    ├── css/site.css        # design system + every component
    ├── js/site.js          # nav, reveal, placement filter (progressive only)
    ├── fonts/*.woff2       # derived from locally installed system fonts
    ├── img/*.webp          # page assets (optimised)
    ├── posters/*.svg       # 10 poster creatives, 1920x1080, generated
    ├── banners/*.svg       # 6 paid placements, generated
    └── download/*          # full-resolution originals, offered as downloads
```

## Design language

A flat Swiss / editorial poster system: flat colour fields only, huge Druk Wide
Heavy caps, hairline rules, hard edges, zero radius, no gradients, no shadows,
no blur, massive negative space, tiny mono corner tags and a quiet mono body.

| Token | Value |
|---|---|
| `--ink` | `#000000` |
| `--paper` | `#efede2` |
| `--paper-2` | `#e3e0d2` |
| `--green` | `#008942` |
| `--cyan` | `#09b1c7` |
| `--orange` | `#ff9d00` |
| `--solar` | `#ffc400` |

Type:

- **Druk Wide**: display headings (`--display`)
- **GT Pressura Mono**: micro labels and data (`--mono`)
- **Departure Mono**: body copy (`--body`)

Druk Wide and GT Pressura Mono ship narrow charsets, so Departure Mono is listed
as a per-glyph fallback in both stacks: `×`, `·`, `→` and `↓` resolve to
Departure Mono rather than silently dropping.

Display type is sized to **fit its container** rather than being guessed. Each
headline carries its measured Druk Wide advance width (`--em`), and the size is
derived from the container's own inline size (`cqw`), so a headline can never
overflow the box it sits in at any viewport width.

Nothing on the page animates on its own. The scroll reveal is a plain fade with
no movement and no stagger, and there are no keyframe animations anywhere in
the stylesheet.

## The product story

Every line of copy comes from one idea: **Walkaway is a local intelligent
browser.** It runs on your machine (your sessions, your secrets, your network);
your phone and your remote assistance can talk to it directly; it has unlimited
access to your most important secrets, which is the point, because it can then
actually finish the job; it composes, fetches and finishes long-running or
complex work; and it keeps a trusted line to you through your display glasses,
so it asks for one tap only when it has to.

Copy rules for this page: no em dashes, no use of the word "portals" (a form is
a static thing and cannot be given agency), and no made-up shorthand. The
interruption model is stated as **one tap**, and the benefit as **peace of
mind**.

## Artwork

The artwork is the real shipped brand material, plus campaign artwork drawn in
the same style:

| Asset | What it is |
|---|---|
| `hero-muse.png` | The hero. A real Hermes browser screenshot with the winged Muse mascot and three winged AI chips flying around it, and the green supervision square baked into the panel. |
| `hero-muse-flight.png` | The mirrored scene: browser on the left, mascot flying in from the right. |
| `og-walkaway.png` | 1200×630 social card. Also the page's `og:image`. |
| `square-muse.png` | 1080×1080 social square. |
| `hero-winged-chips.png` | The original hero, with the app dock. Kept as a downloadable. |
| `muse-mascot.png` | The winged Muse mascot alone, transparent. Also flies into the film frame. |
| `muse-chips.png` | Three golden winged AI chips, transparent. |
| `muse-form.png` | The mascot carrying a form with two rows already filled in. |
| `muse-tap.png` | The mascot pressing one chip. |
| `muse-glasses.png` | The mascot in the display glasses. |
| `muse-rest.png` | The mascot at rest with a chip floating above it. |
| `icon-ios-*.png` | The three iOS app icons. |
| `icon-signal.png` | The supervision mark. |
| `mark-walkaway.png` | The Walkaway wordmark. |
| `icon-device-sync.jpg` | The device-sync icon. |
| `screen-*.png` | Three real screenshots of the Hermes browser. |
| `avatar-adversary.png` | The agent mark. |

Every mascot piece is cut to a transparent alpha channel so it can sit on a flat
colour field. The browser window in every hero is a real Hermes screenshot; only
the mascot and the chips are drawn for the campaign.

The agent mark is **not** the villain of the page. The threat-model framing was
removed: there is no "adversary" band and no threat dossier. The ghost now
reads as *the agent on the other end of the line*, and it appears small and
unlabelled in the "Reachable" block and in the asset kit as "Agent mark".

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

## Rebuilding the hero artwork

The hero images are **composed**, not hand-drawn:

```bash
/tmp/fontenv/bin/python scripts/build-hero-art.py
```

`scripts/build-hero-art.py` crops the real Hermes browser out of
`assets/download/hero-winged-chips.png`, lays the transparent mascot and chip
cutouts over it, bakes the green supervision square, and writes
`hero-muse.png`, `hero-muse-flight.png`, `og-walkaway.png` and
`square-muse.png`. The browser is real in every hero, so the product surface is
never fabricated; only the mascot and the chips are campaign drawings.

## Rebuilding the creatives

The poster set and the paid placements are **generated**, not hand-drawn:

```bash
python3 scripts/build-posters.py
```

`scripts/build-posters.py` writes `assets/posters/*.svg` (10 files, 1920×1080)
and `assets/banners/*.svg` (6 files at true ad ratios: 1200×628, 1080×1080,
1080×1920, 300×250, 1200×628, 1080×1080). Each file is standalone: the `woff2`
faces are embedded as base64 data URIs, there is no `<script>`, no gradient and
no external reference. One subtle `feTurbulence` grain overlay at 0.08 opacity
gives the flat fields the print texture of the reference style.

Type sizes are derived from **measured** advance widths via fontTools, never
eyeballed: the generator scales each line down to fit its declared box and
exits non-zero if anything would overflow. It also checks that accent fields
stay clear of the type, and removes artboards it no longer produces so a renamed
creative cannot leave a stale file behind.

## Verification

```bash
python3 scripts/verify-marketing-site.py                                    # assets + fonts + glyphs
PY=/tmp/fontenv/bin/python python3 scripts/verify-marketing-site.py       # + creative metrics
```

Checks every local reference in `index.html`, every `@font-face` URL, that every
creative's `viewBox` matches the size advertised for it, that no `<text>` node
overflows its canvas in either axis using the real font advance widths and
per-glyph fallback through the declared font stack, and that every non-ASCII
character on the page and in the creatives is covered by at least one shipped
font.

The creatives are also re-measured in a real browser engine: all 16 artboards are
loaded in Chrome and every `<text>` node is measured with
`getComputedTextLength()` and `getBBox()`, because a model of the font metrics
is not the same thing as what a browser actually paints.

Local preview (port `8099` was confirmed unused first, and it is not part of
the demo path):

```bash
cd apps/marketing-site && python3 -m http.server 8099 --bind 127.0.0.1
```

## Adding the demo film

The film section currently ships as a poster frame: there is no demo video
file in the repository yet. When the 3-minute master is final, drop it at
`assets/video/walkaway-blitz-3min.mp4` and replace the `.frame` block in the
`#film` section with a `<video>` element that keeps the existing 16:9 styling.
