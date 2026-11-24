import { useRef } from 'react';
import { useStarfield } from '../lib/useStarfield';
import { useScene } from '../lib/useScene';
import { Orb } from './Orb';
import { Planets } from './Planets';
import { Hud } from './Hud';
import { useExerciseClock } from '../lib/useExerciseClock';

/**
 * The page.
 *
 * A city is held inside a glass globe at the centre of the frame: towers
 * traced in red neon, a bay of lights below, a blue atmosphere around the
 * rim. Behind it, deep space: a drawn star field with the Andromeda galaxy
 * lying in it. Every four minutes the city runs a red team exercise.
 */
export function App() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { phase, countdown, sweepPass } = useExerciseClock();
  const { current: scene, previous, advance } = useScene();

  useStarfield(canvasRef);

  return (
    <>
      <div className="space-clip" aria-hidden="true">
        {/* Deep space, drawn rather than photographed: a field of forty
            thousand stars on a real magnitude distribution, the galactic band
            running through it, and the Andromeda galaxy off to one side. */}
        <div className="deep-field" />
        <canvas id="space" ref={canvasRef} />
        <div className="nebula" />
        <div className="planets">
          <Planets />
        </div>
      </div>
      <div className="vignette" aria-hidden="true" />

      <Hud phase={phase} countdown={countdown} />

      <main className="stage">
        <Orb
          phase={phase}
          sweepPass={sweepPass}
          scene={scene}
          previous={previous}
          advance={advance}
        />

        <div className={`exercise${phase === 'exercise' ? ' is-on' : ''}`} aria-hidden="true">
          <span className="exercise__tag">Red Team Exercise</span>
          <span className="exercise__rule" />
          <span className="exercise__sub">
            {phase === 'exercise'
              ? `Laser sweep ${sweepPass} of 2 — city grid`
              : 'Laser sweep — city grid'}
          </span>
        </div>

        <div className="brand">
          <a className="brand__link" href="mailto:contact@model.tools">
            AI RED TEAM
          </a>
          <p className="brand__place">San Francisco, CA</p>
        </div>
      </main>
    </>
  );
}
