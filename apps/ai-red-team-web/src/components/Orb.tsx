import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { Phase } from '../lib/schedule';
import { startOrbCompositor } from '../lib/orbCompositor';
import { useOrbInteraction } from '../lib/useOrbInteraction';
import type { Scene } from '../media/scenes';
import { SCENE_DEPTH } from '../media/depth';

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/**
 * One scene of the globe: the picture, and the film of the same city when
 * the scene is a film.
 *
 * The film fades in over the still. The still is not a different picture — it
 * is the film's own first frame — so there is never a jump between the two,
 * and the still is what the page falls back to if the film cannot play.
 */
function SceneLayer({
  scene,
  crossed,
  faded,
  onReady,
  onEnded,
}: {
  scene: Scene;
  /** True once the arriving scene is on the page and the crossing is on. */
  crossed: boolean;
  /** True when this is the scene being crossed away from. */
  faded: boolean;
  onReady?: () => void;
  /** Called when a film has played to its end. */
  onEnded?: () => void;
}) {
  const [playing, setPlaying] = useState(false);
  const isFilm = scene.kind === 'video';
  const layerRef = useRef<HTMLDivElement>(null);
  // A film with depth ships packed: colour on the left, depth at the top
  // right. The page plays that one film, and without WebGL the stylesheet
  // shows only its left square, which is the film as it always was.
  const depth = SCENE_DEPTH[scene.id];
  const filmSrc = depth?.film ?? scene.src;
  const packed = isFilm && !!depth?.film;

  useLayoutEffect(() => {
    // The layer is on the page with its old opacity. Asking the browser for
    // its size makes it work that out now, so that when the opacity changes
    // it has something to fade from — otherwise a picture that is ready the
    // instant it appears has no "before" to fade out of, and the crossing
    // from one scene to the next is a cut rather than a fade.
    void layerRef.current?.offsetWidth;
  }, []);

  // The arriving scene settles as it comes in and the scene being left
  // drifts away as it goes, so the crossing is a movement and not a dissolve
  // between two still pictures.
  const style = faded
    ? { opacity: crossed ? 0 : 1, transform: crossed ? 'scale(1.025)' : 'scale(1)' }
    : { opacity: crossed ? 1 : 0, transform: crossed ? 'scale(1)' : 'scale(1.025)' };

  return (
    <div
      className="orb__layer"
      ref={layerRef}
      style={style}
      aria-hidden={faded || undefined}
      data-scene-id={scene.id}
      data-role={faded ? 'previous' : 'current'}
      data-depth={depth?.still}
      data-packed={packed ? '1' : undefined}
      data-alert={scene.alert ? '1' : undefined}
    >
      <picture className="orb__picture">
        <source srcSet={scene.posterWebp} type="image/webp" />
        <img
          className="orb__img"
          src={scene.poster}
          width={scene.width}
          height={scene.height}
          alt={faded ? '' : scene.alt}
          onLoad={onReady}
        />
      </picture>
      {isFilm && (
        <video
          className={`orb__video${packed ? ' orb__video--packed' : ''}${playing ? ' is-playing' : ''}`}
          src={filmSrc}
          poster={scene.poster}
          autoPlay
          muted
          loop={scene.loop}
          playsInline
          preload="auto"
          aria-hidden="true"
          onPlaying={() => setPlaying(true)}
          onEnded={onEnded}
        />
      )}
    </div>
  );
}

/**
 * The globe: the city, the laser that sweeps the grid during a drill, and
 * the bay of lights below.
 *
 * The sphere carries no wireframe. The latitude and longitude grid used to
 * sit over it, and it made the orb read as a wireframe globe — a basketball
 * of lines trying to look three-dimensional. The globe is a smooth glass
 * balloon now, and the neon inside it is the only thing that describes it.
 *
 * The globe is a scene, and the page drifts through the scenes: the city in
 * the different lights it was drawn in, the camera moves that take the
 * camera into the city and back out, the storm rolling over it, and the red
 * alert state. Two scenes are on the page at once while it crosses between
 * them — the one it is leaving underneath, the one it is arriving at over the
 * top — and they are faded over each other. Every scene is built at the same
 * size with the globe in the same place, so what the crossing changes is the
 * colour and the camera, and nothing jumps.
 *
 * The crossing waits for the arriving scene to be on the page before it
 * starts, so a slow picture never leaves a hole in the globe.
 */
export function Orb({
  phase,
  sweepPass,
  scene,
  previous,
  advance,
}: {
  phase: Phase;
  sweepPass: number;
  scene: Scene;
  previous: Scene | null;
  /** Move on to the next scene now — a film says when it has finished. */
  advance: () => void;
}) {
  const [readyFor, setReadyFor] = useState<string | null>(null);
  const crossed = readyFor === scene.id;
  const { pushRef, shieldRef, motion, onPointerDown, onPointerMove, onPointerUp } =
    useOrbInteraction();
  const cityRef = useRef<HTMLDivElement>(null);
  const lensRef = useRef<HTMLCanvasElement>(null);
  const [lens, setLens] = useState(false);

  // the globe drawn through WebGL: bloom, glass, linear-light crossings
  useEffect(() => {
    const canvas = lensRef.current;
    const city = cityRef.current;
    if (!canvas || !city) return;
    const handle = startOrbCompositor(canvas, city, motion, {
      reducedMotion: prefersReducedMotion(),
    });
    if (!handle) return;
    setLens(true);
    return () => {
      handle.stop();
      setLens(false);
    };
  }, [motion]);

  // The alert scene is the city under the red alert light with the laser
  // sweeping the grid, whether or not the four-minute clock is in its drill.
  // The page goes on or off alert when the crossing to or from the alert
  // scene starts — when the arriving scene is on the page — so the red around
  // the globe moves with the red in it rather than ahead of it.
  const alertScene = crossed || !previous ? scene.alert : previous.alert;
  const alert = alertScene || phase === 'exercise';
  const scanning = alert;

  return (
    <div
      className={`orb${scanning ? ' is-scanning' : ''}${alertScene ? ' is-alert' : ''}${lens ? ' is-lens' : ''}`}
      data-state={phase}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
    >
      {/* The wobble lives on its own wrapper. Nothing in it ever turns in
          the plane of the screen: the globe tips in depth, flexes and springs
          back, so the city always stays the right way up. */}
      <div className="orb__push" ref={pushRef}>
        <div className="orb__halo" aria-hidden="true" />
        <div className="orb__body">
          <div className="orb__city" ref={cityRef}>
            {previous && (
              <SceneLayer
                key={previous.id}
                scene={previous}
                crossed={crossed}
                faded
              />
            )}
            <SceneLayer
              key={scene.id}
              scene={scene}
              crossed={crossed}
              faded={false}
              onReady={() => setReadyFor(scene.id)}
              onEnded={advance}
            />
            {/* The same scenes, drawn again through a lens. When WebGL is
                missing this canvas stays blank and hidden, and the layers
                underneath are what shows. */}
            <canvas className="orb__lens" ref={lensRef} aria-hidden="true" />
            <div
              className="orb__scan"
              aria-hidden="true"
              key={`${sweepPass}:${alertScene ? scene.id : ''}`}
              style={{ animationDelay: alert ? '0s' : undefined }}
            >
              <span className="orb__scan--thin" />
            </div>
          </div>
          <div className="orb__grade" aria-hidden="true" />
          <div className="orb__alert" aria-hidden="true" />
          <div className="orb__water" aria-hidden="true" />
          <div className="orb__water orb__water--second" aria-hidden="true" />
          <div className="orb__flash" aria-hidden="true" />
          <div className="orb__glint" aria-hidden="true" />
          <div className="orb__reticle" aria-hidden="true">
            <span className="orb__bracket orb__bracket--tl" />
            <span className="orb__bracket orb__bracket--tr" />
            <span className="orb__bracket orb__bracket--bl" />
            <span className="orb__bracket orb__bracket--br" />
          </div>
        </div>
        <div className="orb__rim" aria-hidden="true" />
        {/* The energy shield: invisible at rest, it flares where the globe
            is touched — a ripple out from the point of contact across a
            lattice of cells, and the rim lighting up. */}
        <div className="orb__shield" aria-hidden="true">
          <span className="orb__shield-rim" />
          <div className="orb__shield-field" ref={shieldRef}>
            <span className="orb__shield-cells" />
          </div>
        </div>
        <div className="orb__shadow" aria-hidden="true" />
      </div>
    </div>
  );
}
