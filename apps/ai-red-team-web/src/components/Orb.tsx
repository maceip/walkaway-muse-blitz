import { useLayoutEffect, useRef, useState } from 'react';
import type { Phase } from '../lib/schedule';
import type { Scene } from '../media/scenes';

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
          className={`orb__video${playing ? ' is-playing' : ''}`}
          src={scene.src}
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

  // The alert scene is the city under the red alert light with the laser
  // sweeping the grid, whether or not the four-minute clock is in its drill.
  const alert = scene.alert || phase === 'exercise';
  const scanning = alert;

  return (
    <div
      className={`orb${scanning ? ' is-scanning' : ''}${scene.alert ? ' is-alert' : ''}`}
      data-state={phase}
    >
      <div className="orb__halo" aria-hidden="true" />
      <div className="orb__body">
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
        <div className="orb__grade" aria-hidden="true" />
        <div className="orb__alert" aria-hidden="true" />
        <div className="orb__water" aria-hidden="true" />
        <div className="orb__water orb__water--second" aria-hidden="true" />
        <div
          className="orb__scan"
          aria-hidden="true"
          key={`${sweepPass}:${scene.alert ? scene.id : ''}`}
          style={{ animationDelay: alert ? '0s' : undefined }}
        >
          <span className="orb__scan--thin" />
        </div>
        <div className="orb__flash" aria-hidden="true" />
        <div className="orb__reticle" aria-hidden="true">
          <span className="orb__bracket orb__bracket--tl" />
          <span className="orb__bracket orb__bracket--tr" />
          <span className="orb__bracket orb__bracket--bl" />
          <span className="orb__bracket orb__bracket--br" />
        </div>
      </div>
      <div className="orb__rim" aria-hidden="true" />
      <div className="orb__shadow" aria-hidden="true" />
    </div>
  );
}
