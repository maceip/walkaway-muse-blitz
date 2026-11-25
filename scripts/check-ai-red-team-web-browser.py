#!/usr/bin/env python3
"""
Browser check for the AI Red Team page (apps/ai-red-team-web).

The static check, `scripts/verify-ai-red-team-web.py`, proves the build is
internally consistent. This one drives a real browser against a real build and
proves the page *behaves*: every scene opens and shows itself, the films
play, the crossing between two scenes is a fade rather than a cut, the order
is not fixed, nothing overflows, and the orb answers the mouse.

    python3 scripts/check-ai-red-team-web-browser.py                    # local
    BASE=https://redteam.model.tools python3 scripts/check-ai-red-team-web-browser.py

Needs Playwright (`pip install playwright && playwright install chromium`)
and a server already running the build: the preview script on 8130, or
`PORT=8091 HOST=127.0.0.1 node server/index.mjs` from the app directory.

Exit code 0 = every check passed. Exit code 1 = at least one failed.
"""

from __future__ import annotations

import os
import sys
import time

try:
    from playwright.sync_api import sync_playwright
except ImportError:  # pragma: no cover
    print("playwright is required: python3 -m pip install playwright", file=sys.stderr)
    raise SystemExit(2)

BASE = os.environ.get("BASE", "http://127.0.0.1:8130").rstrip("/")

SCENES = [
    "city-globe-spin", "globe-day", "globe-dusk", "city-globe-night",
    "city-spiral", "city-alert", "city-storm", "city-dive", "city-edge",
    "city-zoom", "city-sealevel", "city-scan", "city-globe-scan",
    "city-redteam",
]

fails: list[str] = []


def check(cond: bool, msg: str) -> None:
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        fails.append(msg)


def orb_state(page):
    return page.evaluate("""() => {
      const spin = document.querySelector('.orb__spin');
      const push = document.querySelector('.orb__push');
      const num = (el, name) => parseFloat(el && el.style.getPropertyValue(name) || '0');
      return {
        spin: num(spin, '--spin'),
        px: num(push, '--push-x'),
        py: num(push, '--push-y'),
        squash: push && push.style.getPropertyValue('--squash')
                  ? parseFloat(push.style.getPropertyValue('--squash')) : 1,
        cursor: getComputedStyle(document.querySelector('.orb')).cursor,
      };
    }""")


def main() -> int:
    with sync_playwright() as p:
        browser = p.chromium.launch(args=["--autoplay-policy=no-user-gesture-required"])

        print("scenes")
        for sid in SCENES:
            ctx = browser.new_context(viewport={"width": 1280, "height": 720})
            page = ctx.new_page()
            errors: list[str] = []
            external: list[str] = []
            page.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
            page.on("pageerror", lambda e: errors.append(str(e)))
            page.on("request", lambda r: external.append(r.url) if not r.url.startswith(BASE) else None)
            page.goto(f"{BASE}/?t=100&scene={sid}", wait_until="load")
            page.wait_for_timeout(5000)
            info = page.evaluate("""() => {
              const el = document.querySelector('.orb__layer');
              const img = el && el.querySelector('img');
              const vid = el && el.querySelector('video');
              const orb = document.querySelector('.orb');
              const d = document.documentElement;
              const alert = document.querySelector('.orb__alert');
              return {
                opacity: el ? Number(getComputedStyle(el).opacity) : null,
                natural: img ? [img.naturalWidth, img.naturalHeight] : null,
                video: vid ? {paused: vid.paused, t: vid.currentTime, rs: vid.readyState} : null,
                isAlert: orb ? orb.classList.contains('is-alert') : null,
                scanning: orb ? orb.classList.contains('is-scanning') : null,
                alertOpacity: alert ? Number(getComputedStyle(alert).opacity) : null,
                sw: d.scrollWidth, cw: d.clientWidth,
              };
            }""")
            check(info["opacity"] is not None and info["opacity"] > 0.95,
                  f"{sid}: the scene is showing (opacity {info['opacity']})")
            check(info["natural"] == [1024, 1024], f"{sid}: the picture is {info['natural']}")
            check(info["sw"] == info["cw"], f"{sid}: no horizontal overflow")
            check(not errors, f"{sid}: no console error {errors[:1]}")
            check(not external, f"{sid}: no external request {external[:1]}")
            if sid == "city-alert":
                check(info["isAlert"] and info["alertOpacity"] and info["alertOpacity"] > 0.3,
                      f"code red: the alert grade is on ({info['alertOpacity']}), class {info['isAlert']}")
                check(info["scanning"], "code red: the laser sweep is running")
            if info["video"]:
                check(info["video"]["paused"] is False and info["video"]["rs"] >= 2,
                      f"{sid}: the film is playing (t={info['video']['t']:.2f})")
            ctx.close()

        print("crossing")
        ctx = browser.new_context(viewport={"width": 1280, "height": 720})
        page = ctx.new_page()
        # no ?scene= here: a scene opened directly is pinned and never moves on
        page.goto(f"{BASE}/?t=100", wait_until="load")
        page.wait_for_timeout(2000)
        page.evaluate("() => { window.__t = performance.now(); }")
        start = time.time()
        seen_two = False
        ramp = []
        while time.time() - start < 32:
            layers = page.eval_on_selector_all(
                ".orb__layer",
                'els => els.map(e => [e.querySelector("img")?.getAttribute("src").split("/").pop(), '
                "Number(getComputedStyle(e).opacity)])",
            )
            if len(layers) == 2:
                seen_two = True
                ramp.append(tuple(round(l[1], 3) for l in layers))
            elif seen_two and ramp:
                break
            page.wait_for_timeout(60)
        check(seen_two, "a crossing happened")
        mids = [r for r in ramp if 0.05 < r[0] < 0.95]
        check(len(mids) >= 3, f"the crossing fades rather than cuts ({len(mids)} mid samples)")
        sums = [round(sum(r), 2) for r in ramp]
        check(all(0.85 < s < 1.15 for s in sums), f"the two layers sum to one while crossing ({sums[:6]})")
        ctx.close()

        print("shuffle")
        ctx = browser.new_context(viewport={"width": 1280, "height": 720})
        page = ctx.new_page()
        page.goto(f"{BASE}/?t=100", wait_until="load")
        page.wait_for_timeout(2000)
        order: list[str] = []
        prev = None
        t0 = time.time()
        while time.time() - t0 < 95:
            layers = page.eval_on_selector_all(
                ".orb__layer", 'els => els.map(e => e.querySelector("img")?.getAttribute("src").split("/").pop())'
            )
            cur = layers[-1]
            if cur != prev:
                order.append(cur.replace(".png", ""))
                prev = cur
            page.wait_for_timeout(150)
        print("   order:", " -> ".join(order))
        check(len(order) >= 3, f"the page moved through {len(order)} scenes")
        check(all(a != b for a, b in zip(order, order[1:])), "no scene is shown twice in a row")
        check(order != [s for s in SCENES if s in order], "the order is not the fixed one")
        ctx.close()

        print("the orb answers the hand")
        ctx = browser.new_context(viewport={"width": 1280, "height": 720})
        page = ctx.new_page()
        errors = []
        page.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto(f"{BASE}/?scene=globe-dusk", wait_until="load")
        page.wait_for_timeout(2500)

        rest = orb_state(page)
        check(rest["cursor"] == "pointer", f"the orb shows a pointer ({rest['cursor']})")

        # the wheel turns the globe
        for _ in range(6):
            page.mouse.wheel(0, 100)
            page.wait_for_timeout(30)
        page.wait_for_timeout(120)
        spun = orb_state(page)
        check(abs(spun["spin"]) > 20, f"scrolling turned the globe ({spun['spin']:.1f} deg)")
        page.wait_for_timeout(2500)
        settled = orb_state(page)
        check(abs(settled["spin"]) > abs(spun["spin"]),
              f"the globe coasts on after the scroll stops ({settled['spin']:.1f} deg)")
        # The coast has a long tail — the turn is still worth a fraction of a
        # degree a second several seconds after the wheel stopped — so this
        # waits for the globe to be genuinely still and then checks that two
        # samples three seconds apart are the same, rather than checking that
        # it is nearly still.
        page.wait_for_timeout(8000)
        stopped = orb_state(page)
        page.wait_for_timeout(3000)
        still = orb_state(page)
        check(abs(still["spin"] - stopped["spin"]) < 0.5,
              f"and then it settles ({still['spin']:.1f} deg)")

        # a click shoves it aside and throws it round
        box = page.evaluate(
            "() => { const r = document.querySelector('.orb').getBoundingClientRect();"
            " return {x: r.x, y: r.y, w: r.width, h: r.height}; }"
        )
        before = orb_state(page)
        page.mouse.click(box["x"] + box["w"] * 0.9, box["y"] + box["h"] * 0.5)
        peak = None
        for _ in range(60):
            page.wait_for_timeout(16)
            s = orb_state(page)
            if peak is None or abs(s["px"]) > abs(peak["px"]):
                peak = s
        check(peak is not None and abs(peak["px"]) > 4,
              f"the click shoved the globe aside ({peak['px']:.1f} px of {box['w']:.0f})")
        check(peak is not None and peak["squash"] < 0.99,
              f"the globe squashed as it was shoved ({peak['squash']:.3f})")
        check(peak is not None and abs(peak["spin"] - before["spin"]) > 20,
              f"and it was thrown round ({peak['spin'] - before['spin']:.1f} deg)")
        page.wait_for_timeout(3000)
        rest = orb_state(page)
        check(abs(rest["px"]) < 0.5 and abs(rest["squash"] - 1) < 0.005,
              f"and it springs back ({rest['px']:.1f} px, squash {rest['squash']:.3f})")
        check(not errors, f"no console error while it was being pushed {errors[:1]}")
        ctx.close()

        print("layout")
        for w in (360, 480, 768, 1280, 1600):
            ctx = browser.new_context(viewport={"width": w, "height": 720})
            page = ctx.new_page()
            page.goto(f"{BASE}/?t=250", wait_until="load")
            page.wait_for_timeout(1800)
            m = page.evaluate("""() => {
              const d = document.documentElement;
              const brand = document.querySelector('.brand__link').getBoundingClientRect();
              const hud = [...document.querySelectorAll('.hud')].map(h => h.getBoundingClientRect());
              return {sw: d.scrollWidth, cw: d.clientWidth,
                      brandRight: brand.right, hudRight: Math.max(...hud.map(r => r.right)), vw: window.innerWidth};
            }""")
            check(m["sw"] == m["cw"], f"{w}px: no horizontal overflow ({m['sw']} vs {m['cw']})")
            check(m["brandRight"] <= m["vw"] + 1 and m["hudRight"] <= m["vw"] + 1,
                  f"{w}px: the wordmark and the HUD are inside the frame")
            ctx.close()

        browser.close()

    print()
    if fails:
        print("FAIL (%d)" % len(fails))
        for f in fails:
            print("  -", f)
        return 1
    print("PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
