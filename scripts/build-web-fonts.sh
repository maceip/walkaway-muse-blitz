#!/usr/bin/env bash
# Build web fonts (woff2) for apps/marketing-site from locally installed system fonts.
# Source fonts stay in ~/Library/Fonts; this script only writes derived woff2 files.
set -euo pipefail
PY="${PY:-/tmp/fontenv/bin/python}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/apps/marketing-site/assets/fonts"
SRC="$HOME/Library/Fonts"
UNI="U+0000-00FF,U+2000-206F,U+20A0-20BF,U+2100-214F,U+2190-21FF,U+2202,U+2206,U+220F,U+2211,U+2212,U+2215,U+25A0-25FF,U+2600-26FF,U+2713-2718,U+FEFF,U+FFFD"
mkdir -p "$OUT"
convert() { # $1=source  $2=outname
  "$PY" - "$SRC/$1" "$OUT/$2" "$UNI" <<'PY'
import sys
from fontTools.ttLib import TTFont
from fontTools.subset import Subsetter, Options
src, dst, uni = sys.argv[1], sys.argv[2], sys.argv[3]
f = TTFont(src)
opts = Options()
opts.desubroutinize = True
opts.layout_features = ["*"]
opts.name_IDs = ["*"]
opts.notdef_outline = True
opts.recalc_bounds = True
s = Subsetter(options=opts)
codes = []
for part in uni.split(","):
    part = part.strip()
    if part.startswith("U+"):
        part = part[2:]
    if "-" in part:
        lo, hi = part.split("-")
        codes.extend(range(int(lo, 16), int(hi, 16) + 1))
    else:
        codes.append(int(part, 16))
s.populate(unicodes=sorted(set(codes)))
s.subset(f)
f.flavor = "woff2"
f.save(dst)
print("wrote", dst)
PY
}
convert "DrukWide-Heavy-Trial.otf"   "DrukWide-Heavy.woff2"
convert "DrukWide-Medium-Trial.otf"  "DrukWide-Medium.woff2"
convert "GT-Pressura-Mono-Bold.ttf"  "GTPressuraMono-Bold.woff2"
convert "DepartureMono-Regular.otf"  "DepartureMono-Regular.woff2"
ls -la "$OUT"
