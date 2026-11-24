import { useEffect, useRef, useState } from 'react';
import {
  accentFrom,
  driftAt,
  formatCountdown,
  phaseAt,
  sweepPassAt,
  type Phase,
} from './schedule';

export interface ExerciseState {
  phase: Phase;
  countdown: string;
  sweepPass: number;
}

/**
 * Drives the exercise clock.
 *
 * The clock reads `?t=<seconds>` so any phase can be opened directly
 * (?t=240 is the red team exercise, ?t=258 the blue settle), which is how
 * the page is checked in a browser.
 */
export function useExerciseClock(): ExerciseState {
  const [state, setState] = useState<ExerciseState>({
    phase: 'nominal',
    countdown: 'T-04:00',
    sweepPass: 1,
  });
  const startedRef = useRef(0);

  useEffect(() => {
    let offset = 0;
    try {
      const m = /[?&]t=(-?\d+(?:\.\d+)?)/.exec(window.location.search || '');
      if (m) offset = parseFloat(m[1]);
    } catch {
      offset = 0;
    }
    startedRef.current = performance.now() - offset * 1000;

    let raf = 0;
    const frame = (now: number) => {
      const t = (now - startedRef.current) / 1000;

      const phase = phaseAt(t);
      const drift = driftAt(t, phase);
      const accent = accentFrom(drift);
      const root = document.documentElement;

      // The drift drives the page's own glow, so it is written to the root
      // element rather than threaded through React: it changes every frame.
      root.style.setProperty('--shift', `${drift.shift.toFixed(2)}deg`);
      root.style.setProperty('--lift', drift.lift.toFixed(3));
      root.style.setProperty('--accent', accent.color);
      root.style.setProperty('--accent-rgb', accent.rgb);

      const sweepPass = sweepPassAt(t);
      const countdown = phase === 'exercise' ? 'Running' : phase === 'settle' ? 'Complete' : formatCountdown(t);

      setState((prev) => {
        if (prev.phase === phase && prev.countdown === countdown && prev.sweepPass === sweepPass) {
          return prev;
        }
        return { phase, countdown, sweepPass };
      });

      raf = window.requestAnimationFrame(frame);
    };

    raf = window.requestAnimationFrame(frame);
    return () => window.cancelAnimationFrame(raf);
  }, []);

  return state;
}
