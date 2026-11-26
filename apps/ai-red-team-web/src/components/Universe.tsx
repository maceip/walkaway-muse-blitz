import { useEffect, useRef, useState } from 'react';
import { startUniverse } from '../lib/universe/renderer';
import { Planets } from './Planets';

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/**
 * The sky behind the page: a live, slowly moving universe drawn in WebGL —
 * drifting star layers, turning galaxies, a planetary system, a binary, a
 * globular cluster, gas and dust, two planets and the odd comet.
 *
 * A browser without WebGL2 gets the drawn still sky and the two CSS planets
 * that were here before, so nobody gets a black page.
 */
export function Universe() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [fallback, setFallback] = useState(false);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const handle = startUniverse(canvas, { reducedMotion: prefersReducedMotion() });
    if (!handle) {
      setFallback(true);
      return;
    }
    return () => handle.stop();
  }, []);

  if (fallback) {
    return (
      <>
        <div className="deep-field" />
        <div className="planets">
          <Planets />
        </div>
      </>
    );
  }
  return <canvas ref={canvasRef} className="universe" />;
}
