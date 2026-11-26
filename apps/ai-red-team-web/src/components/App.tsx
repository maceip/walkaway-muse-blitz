import { useScene } from '../lib/useScene';
import { Orb } from './Orb';
import { Universe } from './Universe';
import { Hud } from './Hud';
import { useExerciseClock } from '../lib/useExerciseClock';

/**
 * The page.
 *
 * A city is held inside a glass globe at the centre of the frame: towers
 * traced in red neon, a bay of lights below, a blue atmosphere around the
 * rim. Behind it, a live universe: drifting stars, turning galaxies and a
 * planetary system. Every four minutes the city runs a red team exercise.
 */
export function App() {
  const { phase, countdown, sweepPass } = useExerciseClock();
  const { current: scene, previous, advance } = useScene();

  return (
    <>
      <div className="space-clip" aria-hidden="true">
        {/* Deep space, live: drifting star layers on a real magnitude
            distribution, galaxies turning on periods of twenty to thirty
            minutes, a planetary system on Kepler's clock, gas and dust. */}
        <Universe />
        <div className="nebula" />
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
