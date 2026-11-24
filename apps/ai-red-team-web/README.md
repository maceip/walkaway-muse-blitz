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
        lib/useStarfield.ts      the canvas sky
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

## Working on it

    npm ci
    npm run dev        # http://127.0.0.1:5180
    npm run build      # -> dist/
    npm run typecheck

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
