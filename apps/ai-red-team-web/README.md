# AI Red Team web — redteam.model.tools

The model.tools red team surface. A city held inside a glass globe, drifting
outspace, and a red team exercise every four minutes.

    https://redteam.model.tools/     the page
    https://red.model.tools/         the same page

Both hostnames serve the same page — they are peers, not a redirect pair.

## What the page does

A city is held inside a glass globe at the centre of the frame, with drifting
outspace behind it and a red team exercise every four minutes.

**The globe drifts through seventeen scenes, on its own.** Five are pictures of
the city in the different lights it was drawn in — the daylight sphere, the
dusk sphere, the night city, the grey city, the spiral city — five are camera moves that take
the camera down into the city and back out, one is a storm rolling over the
city, one is the red alert state with the laser sweeping the grid, and three
are the scanning clips.

The last two are **transitions**: time-lapses of nightfall, each opening on
one of the scenes and carrying that same city into night as its lights come
on. One opens on the daylight sphere and arrives at the dusk sphere; the other
opens on the grey city and arrives at the night city. The page goes into a
transition from the scene it opens on — always, and from nowhere else — and on
to the scene it ends on (`after` and `then` in `videos/scenes.json`; the rule
is `src/lib/sequence.ts`). Between the other scenes the order is drawn at
random, so the page does not come round the same way twice.

Every film runs at a third of the speed it was delivered at — the camera was
flying, and a globe that is drifting is the point. The three scanning clips are
already smooth and slow, so they are slowed a half speed rather than a third,
and the two nightfall time-lapses play at the speed they were made.
A picture is held for eight
seconds, the alert for nine, and a film for its own length — the film says
when it has finished, so the page never crosses away from one that has not.

The crossing itself takes 3.2 seconds, and the change of light is spread over
all of it rather than bunched into a moment (see the lens, below). Every scene
is built at the same size with the globe in the same place, which is what
keeps the globe from jumping at the crossing.

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
        components/Universe.tsx  the live sky (WebGL), with the drawn plate as fallback
        components/Planets.tsx   the fallback sky's two CSS planets
        lib/schedule.ts          the exercise clock (pure, and unit-testable)
        lib/useExerciseClock.ts  the clock driving React
        lib/useScene.ts          the scene cycle and the crossfade
        lib/useOrbInteraction.ts the globe under the hand: it wobbles and
                                 flares its shield, and never turns over
        lib/orbCompositor.ts     the globe redrawn through a WebGL lens:
                                 depth parallax, bloom, glass, crossings
        lib/universe/            the live sky: layout, shaders, renderer
        lib/gl.ts                shared WebGL plumbing
        media/scenes.ts          the scene manifest (GENERATED)
        media/depth.ts           the scenes' depth (GENERATED)
        styles.css               the design system
      public/
        media/    the scenes, the drawn sky, favicon, social card (GENERATED)
        fonts/    Michroma (wordmark) + IBM Plex Mono (HUD)
        video/    the camera moves (GENERATED)
      e2e/orb.spec.ts            end-to-end checks (Playwright, real GPU path)
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

The sky is live: `src/lib/universe/` draws it in one WebGL2 canvas behind the
page, at thirty frames a second.

- **Stars** in three depth layers drift slowly sideways and wrap round the
  frame, nearer layers faster (about 0.7, 1.6 and 3.2 CSS px a second). Their
  brightnesses follow a counts-per-magnitude law, so there are a few bright
  stars and a great many faint ones; a star smaller than a pixel and a half
  is drawn at that size and dimmed to match, so it glides instead of crawling.
- **Systems** turn about their own centres: three galaxies (a spiral, a
  barred spiral, an edge-on one with a dust lane) on periods of eighteen to
  thirty minutes, a planetary system whose four worlds go round on Kepler's
  clock (periods as a^1.5, the inner one in 75 s), a binary, and a globular
  cluster. The galaxies' arms turn as a rigid pattern, so they never wind up.
- **Gas and dust**: the galactic band with dust eating into it and three
  emission nebulae, one tinted by the page's accent. This is rendered small,
  every eighth frame, into a texture the composite slides with the camera.
- **Two planets**, lit from one side with limb darkening, one ringed, and
  **a comet** now and then on a seeded timetable, taking most of a minute to
  cross.
- A slow, bounded camera drift slides everything with parallax.

Everything comes from one fixed seed, so it is the same sky on every visit.
Phones get a different arrangement of the same bodies, clear of the globe and
the wordmark, and draw fewer stars so the density per inch matches. With
`prefers-reduced-motion: reduce` the sky draws one still frame.

A browser without WebGL2 gets the drawn plate instead.
`scripts/build-space-backdrop.py` draws that plate at 4096×2304 and writes it
to `public/media/space/deep-field.webp` (with a `.jpg` beside it); it is only
loaded when the live sky cannot run.

    python3 scripts/build-space-backdrop.py            # redraw and ship it
    python3 scripts/build-space-backdrop.py --check    # the shipped plate is the one this draws

## The orb under the hand

`src/lib/useOrbInteraction.ts` is the whole of it. A click, a tap, a drag or
the wheel never spins the globe: it **wobbles**, and its **energy shield
flares**.

- **A poke** tips the touched side away from you, nudges the globe away from
  the touch, squashes it and flexes it along the line of the impact; then it
  rings down over a second or so. The shield's rim lights up and a ripple
  runs out across a lattice of cells from the point of contact.
- **A drag** leans the globe toward the pointer and tips it the way the front
  would roll, with the shield held faintly lit; let go and it springs home.
- **The wheel** rocks it, top away on a scroll down, and a hard flick flares
  the shield on the leading edge.

**Why it can never go upside down.** Nothing rotates the globe in the plane of
the screen. The only rotations are rotateX / rotateY, each clamped to 14°
(`TILT_MAX`) with a bounce off the stop, and sprung back to zero. The flex is
`rotate(a) scale() rotate(-a)`, which nets to no rotation. The end-to-end
tests check, every frame, that the globe's "up" still points up.

Everything is an underdamped spring, so a second poke adds to the first, and
the loop sleeps the moment the globe settles. With reduced motion the wobble
is a quarter as large; the shield still answers, because that is a response
to something the person did.

## The lens

`src/lib/orbCompositor.ts` redraws the scenes — the same `<img>` and `<video>`
elements, used as textures — through WebGL, so they read as photographed:

- a crossing **takes its whole time** (3.2 s, `TRANSITION_MS`): the new light
  arrives through a broad, soft front, the middle a little before the rim,
  and each point of the globe turns over in about two seconds, so the change
  runs from 10% to 90% over about 1.7 s and no tenth of a second carries more
  than about 7% of it. The two scenes
  are mixed **in OKLab**, where equal steps look equal, so a bright scene
  does not swamp a dark one and then give way all at once.
  `e2e/crossings.spec.ts` photographs crossings a tenth of a second at a time
  and holds them to that;
- the **alert light belongs to its scene**: the alert's red is graded into
  the alert scene in the lens, so it crosses in and out with the city it
  lights; the drill's red light and the clock's colour drift are eased the
  same way, so the city never snaps to a new colour;
- a crossing into a film **waits for the film** (up to 1.5 s) rather than
  starting on its poster: the poster is lossless and the film a little
  softer, and switching part way through softened the city in one frame;
- a crossing is a **focus pull**: the lens drifts out of focus and back as the
  new scene comes in. Two different shots never line up; this is how film
  hides it. Going into a transition from the picture it opens on, there is
  nothing to hide, so there is no focus pull at all;
- **bloom** on the neon and the lit glass, rolled off with a soft shoulder;
- **dispersion** toward the glass rim, a **reflection** that holds still while
  the globe tips under it, and very fine **grain**.

It draws only when something changed — a new film frame (counted with
`requestVideoFrameCallback`, so films upload at their own 24 fps), a crossing,
a wobble or the colour drift — so a still scene costs nothing. Without WebGL2
the elements underneath show, crossfaded by the stylesheet, as before.

### Depth

Every scene has a depth map, so the lens can treat the city as a solid
object: when the globe wobbles, near surfaces slide against far ones (the
towers move against the streets, up to about 2.5% of the globe at the 14°
limit, fading out toward the glass), and a crossing's focus pull travels from
the far city to the near one instead of blurring evenly.

`scripts/build-redteam-depth.py` makes the depth from the media the page
already ships, with Depth Anything V2 Small (Apache-2.0 — Base and Large are
non-commercial and are not used):

    public/media/depth/<id>.webp   512x512 grey depth for a still / a film's first frame
    public/media/depth/<id>.mp4    the film packed with its depth, 1536x1024
    src/media/depth.ts             which scenes have what (GENERATED)
    depth-hashes.json              what was built from what

A film's depth is packed beside its colour in one video — colour square on the
left, depth square at the top right — because two `<video>` elements never
stay on the same frame. Without WebGL the stylesheet shows only the colour
square. The film's depth is steadied over time (a smoothed normalisation range
and a centred five-frame average, wrapped for the looping film).

It runs after the media pipeline and needs torch and transformers; the check
needs neither, and fails if the shipped media has changed since the depth was
made. `npm run build` runs the check, so a build never ships a film whose
depth was made from a different film. The packed films are build artifacts
and are not committed (like `public/video/`); the depth stills,
`depth-hashes.json` and `src/media/depth.ts` are. A machine that builds the
site needs the packed films present, which means running the depth script
there once (the model is ~99 MB, fetched into the Hugging Face cache):

    python3 scripts/build-redteam-depth.py           # with torch + transformers
    python3 scripts/build-redteam-depth.py --check

`docs/orb-realism.md` is the longer look at what else would make the city
read as real, and what each step would cost.

## Working on it

    npm ci
    npm run dev        # http://127.0.0.1:5180
    npm run build      # -> dist/
    npm run typecheck
    npm run test:e2e   # Playwright, against the dev server

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
