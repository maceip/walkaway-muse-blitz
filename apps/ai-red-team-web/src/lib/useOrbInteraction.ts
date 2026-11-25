import { useCallback, useEffect, useRef } from 'react';

/**
 * The orb under the hand.
 *
 * The page drifts through its scenes on its own, but the orb also answers the
 * mouse: the wheel turns it at the speed you scroll, and a click shoves it
 * aside and throws it round.
 *
 * **What turns and what does not.** The turn is the globe's own picture
 * turning. The bay of lights along the bottom and the targeting brackets are
 * the *apparatus* around the globe, not the globe itself, so they stay put.
 * The laser is the exception: it sweeps the city grid, and the grid is what
 * turns, so it turns with it.
 *
 * **Why springs and not a tween.** A thing that is pushed springs back. The
 * shove and the squash are both springs, so the orb settles rather than
 * stopping dead, and a click in the middle of a spin does not cancel it.
 *
 * **Why it sleeps.** The loop only runs while the orb is moving. A page that
 * runs a requestAnimationFrame for ever to draw nothing is a page that
 * never stops working; this one goes quiet the moment the globe settles.
 */

/** Degrees of turn per pixel of wheel travel. */
const SPIN_PER_PIXEL = 0.30;
/** How fast the turn bleeds away, per second. The globe coasts and settles. */
const SPIN_FRICTION = 1.6;
/** The ceiling on the turn, in degrees per second: fast, not silly. */
const MAX_SPIN = 1500;
/** Degrees per second a click throws the globe round. */
const CLICK_SPIN = 340;
/** Pixels per second a click shoves the globe aside. */
const PUSH_KICK = 480;
/** How hard the shove springs back, and how quickly it settles. */
const PUSH_STIFFNESS = 150;
const PUSH_DAMPING = 15;
/** The squash: the globe compressing as it is shoved. */
const SQUASH_KICK = 1.6;
const SQUASH_STIFFNESS = 210;
const SQUASH_DAMPING = 18;
/** The longest step the springs take at once, so a long frame cannot throw it. */
const MAX_STEP = 0.05;
/** Below this, the globe is still enough to stop moving. */
const REST_SPIN = 0.2;

interface Motion {
  spin: number;
  spinVel: number;
  px: number;
  py: number;
  pvx: number;
  pvy: number;
  squash: number;
  squashVel: number;
}

function clamp(value: number, low: number, high: number): number {
  return value < low ? low : value > high ? high : value;
}

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

export interface OrbInteraction {
  /** The element that carries the turn. */
  spinRef: React.RefObject<HTMLDivElement | null>;
  /** The element that carries the shove. */
  pushRef: React.RefObject<HTMLDivElement | null>;
  /** Put this on the orb: a click shoves it aside and throws it round. */
  onPointerDown: (event: React.PointerEvent<HTMLElement>) => void;
}

export function useOrbInteraction(): OrbInteraction {
  const spinRef = useRef<HTMLDivElement>(null);
  const pushRef = useRef<HTMLDivElement>(null);
  const motion = useRef<Motion>({
    spin: 0,
    spinVel: 0,
    px: 0,
    py: 0,
    pvx: 0,
    pvy: 0,
    squash: 1,
    squashVel: 0,
  });
  const raf = useRef(0);
  const last = useRef(0);

  /** Write the state out and decide whether there is anything left to do. */
  const write = useCallback((m: Motion): boolean => {
    const spinEl = spinRef.current;
    if (spinEl) spinEl.style.setProperty('--spin', `${m.spin.toFixed(2)}deg`);
    const pushEl = pushRef.current;
    if (pushEl) {
      pushEl.style.setProperty('--push-x', `${m.px.toFixed(2)}px`);
      pushEl.style.setProperty('--push-y', `${m.py.toFixed(2)}px`);
      pushEl.style.setProperty('--squash', m.squash.toFixed(4));
    }
    const moving =
      Math.abs(m.spinVel) > REST_SPIN ||
      Math.abs(m.pvx) > 0.5 ||
      Math.abs(m.pvy) > 0.5 ||
      Math.abs(m.px) > 0.05 ||
      Math.abs(m.py) > 0.05 ||
      Math.abs(m.squash - 1) > 0.0005 ||
      Math.abs(m.squashVel) > 0.002;
    return moving;
  }, []);

  /** Start the loop if it is not already running. */
  const wake = useCallback(() => {
    if (raf.current) return;
    last.current = performance.now();
    raf.current = requestAnimationFrame(function step(now: number) {
      raf.current = 0;
      const m = motion.current;
      let dt = (now - last.current) / 1000;
      last.current = now;
      if (dt > MAX_STEP) dt = MAX_STEP;

      // the springs are integrated in short steps: a frame that took 200 ms
      // must not throw the globe across the screen
      const steps = Math.max(1, Math.ceil(dt / 0.008));
      const h = dt / steps;
      for (let i = 0; i < steps; i += 1) {
        m.spin += m.spinVel * h;
        m.spinVel *= Math.exp(-SPIN_FRICTION * h);

        m.pvx += (-PUSH_STIFFNESS * m.px - PUSH_DAMPING * m.pvx) * h;
        m.pvy += (-PUSH_STIFFNESS * m.py - PUSH_DAMPING * m.pvy) * h;
        m.px += m.pvx * h;
        m.py += m.pvy * h;

        m.squashVel += (-SQUASH_STIFFNESS * (m.squash - 1) - SQUASH_DAMPING * m.squashVel) * h;
        m.squash += m.squashVel * h;
      }
      if (Math.abs(m.spinVel) < REST_SPIN) m.spinVel = 0;

      if (write(m)) {
        raf.current = requestAnimationFrame(step);
      }
    });
  }, [write]);

  // the wheel turns the globe at the speed you scroll
  useEffect(() => {
    const onWheel = (event: WheelEvent) => {
      if (prefersReducedMotion()) return;
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 400 : 1;
      const delta = event.deltaY * unit;
      if (!delta) return;
      const m = motion.current;
      // scrolling down turns the globe away from you, which is the way round
      // a ball rolls when it is pushed away
      m.spinVel = clamp(m.spinVel - delta * SPIN_PER_PIXEL, -MAX_SPIN, MAX_SPIN);
      wake();
    };
    window.addEventListener('wheel', onWheel, { passive: true });
    return () => window.removeEventListener('wheel', onWheel);
  }, [wake]);

  useEffect(() => () => {
    if (raf.current) cancelAnimationFrame(raf.current);
  }, []);

  const onPointerDown = useCallback((event: React.PointerEvent<HTMLElement>) => {
    const m = motion.current;
    const rect = event.currentTarget.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const dx = event.clientX - cx;
    const dy = event.clientY - cy;
    const len = Math.hypot(dx, dy) || 1;

    const reduced = prefersReducedMotion();
    const shove = reduced ? PUSH_KICK * 0.3 : PUSH_KICK;

    // shoved away from the point that was clicked
    m.pvx += (dx / len) * shove;
    m.pvy += (dy / len) * shove;
    m.squashVel -= reduced ? SQUASH_KICK * 0.3 : SQUASH_KICK;

    if (!reduced) {
      // a globe shoved to the right rolls to the right, and rolling to the
      // right is turning clockwise
      m.spinVel = clamp(m.spinVel + (dx >= 0 ? CLICK_SPIN : -CLICK_SPIN), -MAX_SPIN, MAX_SPIN);
    }
    wake();
  }, [wake]);

  return { spinRef, pushRef, onPointerDown };
}
