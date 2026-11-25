# AI Red Team web — redteam.model.tools

The model.tools red team surface. A city held inside a glass globe, drifting
outspace, and a red team exercise every four minutes.

    https://redteam.model.tools/     the page
    https://red.model.tools/         the same page

Both hostnames serve the same page — they are peers, not a redirect pair.

## What the page does

A city is held inside a glass globe at the centre of the frame, with drifting
outspace behind it and a red team exercise every four minutes.

**The globe drifts through fourteen scenes, on its own.** Four are pictures of
the city in the different lights it was drawn in — the daylight sphere, the
dusk sphere, the night city, the spiral city — five are camera moves that take
the camera down into the city and back out, one is a storm rolling over the
city, one is the red alert state with the laser sweeping the grid, and three
are the scanning clips. The order is drawn at random, so the page does not come
round the same way twice.

Every film runs at a third of the speed it was delivered at — the camera was
flying, and a globe that is drifting is the point. The three scanning clips are
already smooth and slow, so they are slowed a half speed rather than a third.
A picture is held for eight
seconds, the alert for nine, and a film for its own length — the film says
when it has finished, so the page never crosses away from one that has not.

The crossing itself is a real crossfade over 2.2 seconds, with the arriving
scene settling as it comes in and the one being left drifting away as it goes.
Every scene is built at the same size with the globe in the same place, which
is what keeps the globe from jumping at the crossing.

    ?scene=<id>     open one scene and stay there (globe-day, city-dive, ...)

**The globe carries no weather** — except once, on purpose. The delivered
footage had a bank of cloud lying across the top of the globe with a flash of
lightning in it; the media pipeline takes it off, scene by scene, and the only
source that keeps its cloud is the storm film, which is one deliberate beat in
the cycle rather than the constant look it was.

Every four minutes the city runs a **red team exercise**:

| Window | Phase | What you see |
|---|---|---|
| 0:00 – 3:46 | nominal | the city drifts in colour, blue through indigo |
| 3:46 – 4:00 | warning | the rim pulses red, the countdown reaches zero |
| 4:00 – 4:24 | exercise | red laser sweeps the city grid twice, under red alert light |
| 4:24 – 4:36 | settle | the city settles into a blue glow |

The clock takes an optional `?t=<seconds>` offset so any phase can be opened
directly: `?t=240` is the red team exercise, `?t=258` the blue settle.

## Layout

    apps/ai-red-team-web/
      src/
        components/App.tsx       the page
        components/Orb.tsx       the globe, the laser, the bay, the crossfade
        components/Hud.tsx       the corner readouts
        components/Planets.tsx   the distant planets
        lib/schedule.ts          the exercise clock (pure, and unit-testable)
        lib/useExerciseClock.ts  the clock driving React
        lib/useScene.ts          the scene cycle and the crossfade
        lib/useOrbInteraction.ts the globe under the hand: scroll turns it,
                                 a click shoves it
        media/scenes.ts          the scene manifest (GENERATED)
        styles.css               the design system
      public/
        media/    the scenes, the drawn sky, favicon, social card (GENERATED)
        fonts/    Michroma (wordmark) + IBM Plex Mono (HUD)
        video/    the camera moves (GENERATED)
      server/index.mjs           the production server (no dependencies)
      deploy/redteam.caddy       the Caddy stanza for this site (tracked copy
                                 of the live /etc/caddy/redteam.caddy)
      redteam-web.service        the systemd unit
      scripts/
        deploy.sh                build, upload, release, restart

## The media pipeline

`scripts/build-redteam-media.py` is the single generator for the page's media
and its manifest. It reads `videos/scenes.json` (which scenes exist, which is
the default, which of the delivered sources carried a storm to take off, which
film loops, and how much of a source to crop in before staging it) and the
source media in `videos/`, and writes:

    public/media/scenes/<id>.png    the globe, masked to the circle
    public/media/scenes/<id>.webp   the same, lossy — what the browser loads
    public/video/<id>.mp4           the camera moves, on the same square
    src/media/scenes.ts             the manifest the app imports
    media-hashes.json               what was built from what

Three things it does that are worth knowing about:

- **One canonical stage.** Every scene is built at 1024×1024 (`CANVAS`): the
  delivery border is cropped and the frame Lanczos-resized onto it. The
  delivered frames all have the globe filling them, centred, with the same
  framing, so this is what makes the globe the same size and in the same
  place in every scene.
- **Globe only.** The stills are masked to the circle inscribed in that
  square, so the page is never showing a square photograph of a city with
  weather around the edges of it.
- **The storm is removed**, for every scene but the storm film. The cloud is
  found as the bright, washed-out, smooth mass in the top band of the frame,
  and the hole it leaves is filled with the sky of that row — taken from the
  dark pixels in the middle of the frame, because the bright city sits
  directly below the cloud and a fill that lets the city diffuse in puts the
  colour of the street in the sky. The lightning in it is a violet blob and
  the city's own magenta light is in thin streaks, and the two are told apart
  by shape rather than by colour.
- **Every film is slowed to a third of the speed it was delivered at.** The
  frames are interpolated up to the rate the slowed film needs, so the camera
  glides rather than juddering, and their timestamps are stretched to match,
  or the film would be the same length and simply dropping frames. The
  owner's three new clips (`city-scan`, `city-globe-scan`, `city-redteam`)
  are slowed a half speed instead: they are already smooth and slow, and at a
  third of their speed the dark beat in the middle of the scan would sit on
  the page for three seconds, which reads as a fault rather than as a cut.
- **A delivered clip's frame rate is read, not assumed.** The pipeline used to
  declare every clip to be 24 fps when it piped the frames to the encoder,
  so a clip delivered at 60 fps would have played two and a half times too
  slow with four frames in five thrown away by the interpolation. The rate
  now comes from the clip itself, the interpolation runs at that rate times
  the slow-down, and every film ships at 24 fps whatever it arrived at.

The media cache is keyed on the source, the repair **and the generator's own
hash**, so changing the generator rebuilds the media rather than leaving the
old media in place.

## The sky

`scripts/build-space-backdrop.py` draws the sky, at 4096×2304, and writes it
to `public/media/space/deep-field.webp` (with a `.jpg` beside it for anything
that cannot take webp). It is a separate generator from the orb media and a
separate capability: the sky is one plate, not a scene.

    python3 scripts/build-space-backdrop.py            # redraw and ship it
    python3 scripts/build-space-backdrop.py --check    # the shipped plate is the one this draws
    python3 scripts/build-space-backdrop.py --width 1024 --out /tmp/sky.webp

Two things about it are worth knowing before changing it:

- **A star has to survive the downscale.** The plate is 4096 wide and a screen
  is 1280 to 2560, so the browser shrinks every star before it reaches the eye.
  A star drawn as a single pixel arrives smaller than a pixel and is averaged
  down into grey dust — the sky keeps its dust and loses its stars. So the
  stars are drawn in two tiers: a faint tier, drawn tight and dim, which is
  the grain of the field, and a bright tier drawn wide enough (`STAR_TIER_WIDTH`)
  to arrive at the screen as a point of about a pixel across. The check counts
  the distinct bright points that survive the downscale and fails if the sky
  has fallen back to dust.
- **The peak and the width are separate.** A wide point-spread function spreads
  the same light over more pixels, so the total light a star lays down is its
  peak divided by the fraction of the profile that lands on the centre pixel.
  Getting that wrong makes the bright tier dimmer than the faint one, which is
  exactly what happened the first time this was written.

There is no canvas over the sky. There used to be — eighty-five white dots
falling downwards and wrapping around, which on a dark page read as snowfall.
The sky is the drawn plate alone, and the check fails if a canvas comes back.

## The orb under the hand

The page drifts through its scenes on its own, but the globe also answers the
mouse. `src/lib/useOrbInteraction.ts` is the whole of it:

- **the wheel turns the globe at the speed you scroll**, and it coasts on
  afterwards and settles;
- **a click shoves it aside** — away from the point that was clicked — and
  throws it round, in the direction a globe would roll if you pushed it that
  way. The shove and the squash are springs, so the globe springs back
  instead of stopping dead.

Two things about it are worth knowing before changing it:

- **What turns and what does not.** The turn is the globe's own picture
  turning. The bay of lights along the bottom and the targeting brackets are
  the *apparatus* around the globe, so they stay put; the laser is the
  exception, because it sweeps the city grid and the grid is what turns. That
  is why `Orb.tsx` has a `.orb__spin` wrapper around the picture and the
  laser, and a `.orb__push` wrapper around the whole orb.
- **It sleeps.** The animation loop only runs while the globe is moving, so a
  page that is sitting still is not burning a frame every 16 ms drawing
  nothing.

With `prefers-reduced-motion: reduce` the globe does not turn at all — not on
the wheel and not on the click — but a click still gives a small push, because
that is an answer to something the person did rather than motion the page
decided on for itself.

## Working on it

    npm ci
    npm run dev        # http://127.0.0.1:5180
    npm run build      # -> dist/
    npm run typecheck

The static check proves the build is internally consistent; the browser check
drives a real browser against a real build and proves the page behaves:

    python3 scripts/verify-ai-red-team-web.py          # no browser needed
    python3 scripts/check-ai-red-team-web-browser.py   # needs playwright + a running server

The production server serves `dist/` and answers `/healthz`:

    PORT=8091 HOST=127.0.0.1 node server/index.mjs

It implements HTTP Range requests, so the films stream instead of downloading
whole.

## Deploying

    ./scripts/deploy.sh axp.computer

The build happens locally, the release is uploaded to
`/opt/redteam-web/releases/<stamp>/`, the `current` symlink is flipped and
`redteam-web.service` is restarted. The previous release stays in place
until the new one is live.

## Rules for this page

- **The globe media is generated, never hand-edited.** It comes from
  `scripts/build-redteam-media.py`; run it to regenerate and `--check` to
  confirm the shipped media has not drifted. The same goes for the manifest:
  it is generated, not written by hand.
- **Every asset is local.** No external request, no CDN, no runtime
  dependency. `scripts/verify-ai-red-team-web.py` enforces this.
- **There is one implementation of this page.** The earlier no-build static
  site was retired on 2026-11-23. Extend this app; do not add a second one.
- On the host, Caddy runs with `admin off`, so a Caddy config change needs a
  **restart**, not a reload — and never give this site's stanza a file access
  log under `/var/log/caddy`: the `caddy` user cannot create files there and
  the unit crash-loops, taking every site on the box down.
- The Caddy stanza for this site is tracked at `deploy/redteam.caddy` and is
  byte-identical to the live `/etc/caddy/redteam.caddy`. It gives both
  hostnames the same proxy through a shared `(redteam_web)` snippet.
- **Verify a redirect change in a cache-free browser context.** A 301 that
  Chrome has already cached keeps being followed after the server stops
  sending it, so a cached redirect is not proof that the server still
  redirects.
