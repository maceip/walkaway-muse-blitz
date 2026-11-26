# Making the orb city read as real

What limits the city's realism today, what the WebGL lens already does about
it, and what else would help, roughly in order of payoff for the work.

## What limits it now

- **The source.** Every scene is a 1024×1024 render or clip, generated, with
  the glass sphere, its highlights and its lighting baked into the pixels. On
  a retina screen the orb is drawn at about 760–900 device pixels, so the
  sources are close to native size, but there is no detail beyond them, and
  the look is stylised (flat grey towers, magenta neon) rather than lit.
- **Eight-bit, no depth.** The pictures carry no light levels above white and
  no distance, so effects that depend on either (true bloom on hot lights,
  depth of field, haze with distance, parallax) can only be approximated.
- **Unrelated shots.** A crossing goes between two independently generated
  clips. Their geometry never lines up, so any crossing is a blend of two
  different cities; it can be made soft, but not continuous.

## Done in this change (`src/lib/orbCompositor.ts`)

- Crossings as a focus pull: defocus, an uneven centre-first front, a slight
  settle in scale — the way film hides two shots that do not match.
- Later: crossings spread over their whole 3.2 s and mixed in OKLab. The first
  version mixed in linear light through three stacked easings, which packed
  most of the change into about 0.6 s; a bright scene also swamped a dark one
  until the very end. The alert's red light now crosses with its scene.
- Bloom on the neon and the glass, with a soft shoulder instead of clipping.
- Dispersion toward the rim of the glass, a reflection that holds still while
  the globe tips, fine grain.
- The page's colour drift applied to films as well as stills, so a crossing
  from a still into a film no longer jumps in colour.

These make it read as photographed; they cannot add detail or geometry that
is not in the sources.

## Next steps, by payoff

1. **Depth maps for each scene — done** (`scripts/build-redteam-depth.py`,
   Depth Anything V2 Small). Wobble parallax and depth-focused crossings are
   in the lens. Still open on this step: haze that thickens with distance,
   and per-scene tuning of the parallax strength (the generated art bakes
   its own atmosphere, so haze needs care not to double it).
2. **Higher-resolution, higher-range sources.** Re-render or upscale to
   2048×2048, ideally kept as 16-bit/HDR through the pipeline, so bloom
   thresholds sit on real light levels and the orb is sharp at 2× on large
   screens. Cost: about four times the media weight; films would want HEVC
   or AV1 to stay reasonable.
3. **Matched cuts instead of crossfades — begun.** The owner's two nightfall
   time-lapses are transitions: each opens on a scene's own picture and ends
   near another's, and the page goes into and out of them in that order, so
   day turns into night continuously. Still open: transitions for the other
   scenes. Author the cycle so each film ends
   on the frame the next begins with (the pipeline already uses each film's
   first frame as its poster), and fill the gaps with short generated
   bridging clips, or with an optical-flow morph computed offline between the
   last frame of one scene and the first of the next and applied in the lens
   during the crossing. This is what makes a transition continuous rather
   than merely soft. Cost: media authoring, plus a flow field per pair of
   scenes for the morph route.
4. **The live sky on the glass.** Feed the universe canvas to the lens as an
   environment texture so the stars and galaxies reflect in the sphere's rim.
   Cost: one texture copy per frame at low resolution.
5. **A real 3D city.** Google Photorealistic 3D Tiles of San Francisco, or a
   Gaussian-splat capture, rendered (three.js / Cesium) inside an actual
   refracting glass sphere lit by the live sky. Real geometry and real light,
   and every transition becomes one continuous camera move. This is the only
   route to a city that is genuinely real rather than convincingly filmed.
   Cost: the largest by far — an API key with quotas and attribution terms
   for the tiles (or a capture for splats), a much heavier bundle, and a GPU
   budget phones may not meet without a lighter fallback.

Step 4 fits the current lens directly; 2 and 3 are media work; 5 is a
different architecture.
