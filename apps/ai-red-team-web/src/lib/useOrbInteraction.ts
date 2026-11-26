import { useCallback, useEffect, useRef } from 'react';

/**
 * The orb under the hand.
 *
 * A click, a tap, a drag or a turn of the wheel never spins the globe. It
 * wobbles: the globe tips a little in depth, squashes, springs back and rings
 * down, and its energy shield flares where it was touched. The city inside
 * always stays the right way up.
 *
 * **Why it can never go upside down.** Nothing here rotates the globe in the
 * plane of the screen. The only rotations are a tip toward and away from the
 * viewer (rotateX / rotateY), each clamped to TILT_MAX degrees and sprung
 * back to zero. The squash is a scale along the line of the impact, applied
 * as rotate(a) scale(..) rotate(-a), which nets to no rotation at all.
 *
 * **Why springs.** Everything is an underdamped spring, so a poke rings down
 * over a second or so instead of stopping dead, and a second poke adds to the
 * first instead of cancelling it.
 *
 * **Why it sleeps.** The loop only runs while something is moving. The
 * moment the globe settles the loop stops.
 */

/** The furthest the globe tips, in degrees. Never anywhere near upside down. */
export const TILT_MAX = 14;
/** Tip spring: ~1.5 Hz with a damping ratio near 0.3 — a few visible wobbles. */
const TILT_STIFFNESS = 90;
const TILT_DAMPING = 5.6;
/** Degrees per second of tip a poke at the very edge gives. */
const TILT_KICK = 240;
/** The jelly: the globe flexing along the line of the impact. */
const JELLY_STIFFNESS = 260;
const JELLY_DAMPING = 4.4;
const JELLY_KICK = 1.25;
const JELLY_MAX = 0.05;
/** The shove: the globe nudged away from where it was touched. */
const PUSH_STIFFNESS = 160;
const PUSH_DAMPING = 9;
const PUSH_KICK = 230;
const PUSH_MAX = 26;
/** The squash: the globe pressed in by a poke, springing back out. */
const SQUASH_STIFFNESS = 220;
const SQUASH_DAMPING = 12;
const SQUASH_KICK = 0.9;
/** The shield's flare decays with this time constant, in seconds. */
const SHIELD_DECAY = 0.42;
/** How much a drag tips the globe, degrees per pixel. */
const DRAG_TILT = 0.09;
/** How far a drag leans the globe, as a share of the pointer travel. */
const DRAG_LEAN = 0.12;
/** The longest step the springs take at once, so a long frame cannot throw it. */
const MAX_STEP = 0.05;
/** How long a shield impact ripple lives, in ms (matches styles.css). */
export const IMPACT_MS = 1300;

export interface Motion {
  /** tip about the horizontal axis, degrees (positive: top tips away) */
  rx: number;
  rxv: number;
  /** tip about the vertical axis, degrees (positive: right side tips away) */
  ry: number;
  ryv: number;
  /** where the tip springs toward: zero, or wherever a drag is holding it */
  rxRest: number;
  ryRest: number;
  px: number;
  py: number;
  pvx: number;
  pvy: number;
  pxRest: number;
  pyRest: number;
  jelly: number;
  jellyV: number;
  /** the line the jelly flexes along, radians */
  jellyAxis: number;
  squash: number;
  squashV: number;
  /** the shield's flare, 0 at rest */
  shield: number;
  /** a floor under the shield while the globe is held */
  shieldHold: number;
}

export function restingMotion(): Motion {
  return {
    rx: 0,
    rxv: 0,
    ry: 0,
    ryv: 0,
    rxRest: 0,
    ryRest: 0,
    px: 0,
    py: 0,
    pvx: 0,
    pvy: 0,
    pxRest: 0,
    pyRest: 0,
    jelly: 0,
    jellyV: 0,
    jellyAxis: 0,
    squash: 1,
    squashV: 0,
    shield: 0,
    shieldHold: 0,
  };
}

function clamp(value: number, low: number, high: number): number {
  return value < low ? low : value > high ? high : value;
}

/**
 * One integration step. Pure, so it can be reasoned about on its own: it
 * moves every spring forward by h seconds and enforces the limits.
 */
export function stepMotion(m: Motion, h: number): void {
  m.rxv += (-TILT_STIFFNESS * (m.rx - m.rxRest) - TILT_DAMPING * m.rxv) * h;
  m.ryv += (-TILT_STIFFNESS * (m.ry - m.ryRest) - TILT_DAMPING * m.ryv) * h;
  m.rx += m.rxv * h;
  m.ry += m.ryv * h;
  // the hard limit: past it the globe bounces back off the stop
  if (Math.abs(m.rx) > TILT_MAX) {
    m.rx = Math.sign(m.rx) * TILT_MAX;
    m.rxv = -m.rxv * 0.3;
  }
  if (Math.abs(m.ry) > TILT_MAX) {
    m.ry = Math.sign(m.ry) * TILT_MAX;
    m.ryv = -m.ryv * 0.3;
  }

  m.pvx += (-PUSH_STIFFNESS * (m.px - m.pxRest) - PUSH_DAMPING * m.pvx) * h;
  m.pvy += (-PUSH_STIFFNESS * (m.py - m.pyRest) - PUSH_DAMPING * m.pvy) * h;
  m.px = clamp(m.px + m.pvx * h, -PUSH_MAX, PUSH_MAX);
  m.py = clamp(m.py + m.pvy * h, -PUSH_MAX, PUSH_MAX);

  m.jellyV += (-JELLY_STIFFNESS * m.jelly - JELLY_DAMPING * m.jellyV) * h;
  m.jelly = clamp(m.jelly + m.jellyV * h, -JELLY_MAX, JELLY_MAX);

  m.squashV += (-SQUASH_STIFFNESS * (m.squash - 1) - SQUASH_DAMPING * m.squashV) * h;
  m.squash = clamp(m.squash + m.squashV * h, 0.9, 1.06);

  m.shield = Math.max(m.shieldHold, m.shield * Math.exp(-h / SHIELD_DECAY));
}

function isMoving(m: Motion): boolean {
  return (
    Math.abs(m.rx - m.rxRest) > 0.02 ||
    Math.abs(m.ry - m.ryRest) > 0.02 ||
    Math.abs(m.rxv) > 0.05 ||
    Math.abs(m.ryv) > 0.05 ||
    Math.abs(m.px - m.pxRest) > 0.05 ||
    Math.abs(m.py - m.pyRest) > 0.05 ||
    Math.abs(m.pvx) > 0.3 ||
    Math.abs(m.pvy) > 0.3 ||
    Math.abs(m.jelly) > 0.0004 ||
    Math.abs(m.jellyV) > 0.004 ||
    Math.abs(m.squash - 1) > 0.0004 ||
    Math.abs(m.squashV) > 0.004 ||
    m.shield > m.shieldHold + 0.004
  );
}

function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

export interface OrbInteraction {
  /** The element that carries the wobble. */
  pushRef: React.RefObject<HTMLDivElement | null>;
  /** The layer the shield's impact ripples are added to. */
  shieldRef: React.RefObject<HTMLDivElement | null>;
  /** The live motion, read by the globe's renderer for its glass highlight. */
  motion: React.RefObject<Motion>;
  onPointerDown: (event: React.PointerEvent<HTMLElement>) => void;
  onPointerMove: (event: React.PointerEvent<HTMLElement>) => void;
  onPointerUp: (event: React.PointerEvent<HTMLElement>) => void;
}

export function useOrbInteraction(): OrbInteraction {
  const pushRef = useRef<HTMLDivElement>(null);
  const shieldRef = useRef<HTMLDivElement>(null);
  const motion = useRef<Motion>(restingMotion());
  const raf = useRef(0);
  const last = useRef(0);
  const drag = useRef<{ id: number; x: number; y: number; moved: boolean } | null>(null);
  const lastWheelImpact = useRef(0);

  /** Write the motion out to the stylesheet's custom properties. */
  const write = useCallback((m: Motion) => {
    const el = pushRef.current;
    if (!el) return;
    const s = el.style;
    s.setProperty('--tilt-x', `${m.rx.toFixed(3)}deg`);
    s.setProperty('--tilt-y', `${m.ry.toFixed(3)}deg`);
    s.setProperty('--push-x', `${m.px.toFixed(2)}px`);
    s.setProperty('--push-y', `${m.py.toFixed(2)}px`);
    s.setProperty('--jelly-axis', `${m.jellyAxis.toFixed(4)}rad`);
    s.setProperty('--jelly-a', (1 - m.jelly).toFixed(4));
    s.setProperty('--jelly-b', (1 + m.jelly * 0.85).toFixed(4));
    s.setProperty('--squash', m.squash.toFixed(4));
    s.setProperty('--shield', Math.min(1, m.shield).toFixed(3));
    // the glass highlight slides against the tip, the way a reflection
    // stays put while the surface under it turns
    s.setProperty('--glint-x', `${(-m.ry * 0.9).toFixed(2)}px`);
    s.setProperty('--glint-y', `${(m.rx * 0.9).toFixed(2)}px`);
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
      const steps = Math.max(1, Math.ceil(dt / 0.004));
      const h = dt / steps;
      for (let i = 0; i < steps; i += 1) stepMotion(m, h);
      write(m);
      if (isMoving(m) || drag.current) {
        raf.current = requestAnimationFrame(step);
      }
    });
  }, [write]);

  /**
   * A shield impact ripple at a point on the orb, as fractions of its box.
   * Each one is its own element so overlapping pokes overlap.
   */
  const ripple = useCallback((fx: number, fy: number, strength: number) => {
    const host = shieldRef.current;
    if (!host) return;
    const el = document.createElement('span');
    el.className = 'orb__impact';
    el.style.setProperty('--x', `${(fx * 100).toFixed(2)}%`);
    el.style.setProperty('--y', `${(fy * 100).toFixed(2)}%`);
    el.style.setProperty('--strength', strength.toFixed(3));
    host.appendChild(el);
    window.setTimeout(() => el.remove(), IMPACT_MS + 60);
  }, []);

  /** A poke at a point, in units of the orb's radius from its centre. */
  const poke = useCallback(
    (nx: number, ny: number, strength: number) => {
      const m = motion.current;
      const reduced = prefersReducedMotion();
      const k = reduced ? 0.25 : 1;
      const len = Math.hypot(nx, ny);
      if (len > 1) {
        nx /= len;
        ny /= len;
      }
      // the side that was touched tips away from the viewer
      m.ryv += nx * TILT_KICK * strength * k;
      m.rxv += -ny * TILT_KICK * strength * k;
      // and the globe is nudged away from the touch
      m.pvx += -nx * PUSH_KICK * strength * k;
      m.pvy += -ny * PUSH_KICK * strength * k;
      m.squashV -= SQUASH_KICK * strength * k;
      // the flex runs along the line of the impact; a poke dead centre
      // flexes it across, so there is always some give
      m.jellyAxis = len > 0.08 ? Math.atan2(ny, nx) : 0;
      m.jellyV += JELLY_KICK * strength * k * (0.5 + 0.5 * Math.min(1, len * 1.5));
      m.shield = Math.min(1.25, m.shield + 0.85 * strength);
      wake();
    },
    [wake],
  );

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      const el = event.currentTarget;
      const rect = el.getBoundingClientRect();
      const r = rect.width / 2 || 1;
      const nx = (event.clientX - (rect.left + r)) / r;
      const ny = (event.clientY - (rect.top + rect.height / 2)) / r;
      poke(nx, ny, 1);
      ripple((event.clientX - rect.left) / rect.width, (event.clientY - rect.top) / rect.height, 1);
      drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY, moved: false };
      try {
        el.setPointerCapture(event.pointerId);
      } catch {
        /* a synthetic event may have no capturable pointer */
      }
    },
    [poke, ripple],
  );

  const onPointerMove = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      const d = drag.current;
      if (!d || d.id !== event.pointerId) return;
      const dx = event.clientX - d.x;
      const dy = event.clientY - d.y;
      if (!d.moved && Math.hypot(dx, dy) < 4) return;
      d.moved = true;
      const m = motion.current;
      const k = prefersReducedMotion() ? 0.3 : 1;
      // dragging rolls the front of the globe the way the pointer goes,
      // within the same limit as everything else
      m.ryRest = clamp(dx * DRAG_TILT * k, -TILT_MAX * 0.85, TILT_MAX * 0.85);
      m.rxRest = clamp(-dy * DRAG_TILT * k, -TILT_MAX * 0.85, TILT_MAX * 0.85);
      m.pxRest = clamp(dx * DRAG_LEAN * k, -PUSH_MAX * 0.9, PUSH_MAX * 0.9);
      m.pyRest = clamp(dy * DRAG_LEAN * k, -PUSH_MAX * 0.9, PUSH_MAX * 0.9);
      m.shieldHold = 0.32;
      wake();
    },
    [wake],
  );

  const onPointerUp = useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      const d = drag.current;
      if (!d || d.id !== event.pointerId) return;
      drag.current = null;
      const m = motion.current;
      const wasDragged = d.moved;
      m.rxRest = 0;
      m.ryRest = 0;
      m.pxRest = 0;
      m.pyRest = 0;
      m.shieldHold = 0;
      if (wasDragged) {
        // let go: it springs home and rings down, with a little flare
        m.jellyAxis = Math.atan2(m.py, m.px || 0.0001);
        m.jellyV += JELLY_KICK * 0.6;
        m.shield = Math.min(1.25, m.shield + 0.35);
      }
      wake();
    },
    [wake],
  );

  // the wheel tips the globe in the direction of the scroll and lets it rock
  // back; a hard flick flares the shield on the leading edge
  useEffect(() => {
    const onWheel = (event: WheelEvent) => {
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 400 : 1;
      const delta = clamp(event.deltaY * unit, -240, 240);
      if (!delta) return;
      const m = motion.current;
      const k = prefersReducedMotion() ? 0.25 : 1;
      // scrolling down tips the top away, the way a ball rolls when it is
      // pushed away from you
      m.rxv += delta * 0.7 * k;
      m.jellyAxis = Math.PI / 2;
      m.jellyV += Math.abs(delta) * 0.0016 * k;
      m.shield = Math.min(1.0, m.shield + Math.abs(delta) * 0.0012);
      const now = performance.now();
      if (Math.abs(delta) > 40 && now - lastWheelImpact.current > 450) {
        lastWheelImpact.current = now;
        ripple(0.5, delta > 0 ? 0.08 : 0.92, 0.6);
      }
      wake();
    };
    window.addEventListener('wheel', onWheel, { passive: true });
    return () => window.removeEventListener('wheel', onWheel);
  }, [wake, ripple]);

  useEffect(
    () => () => {
      if (raf.current) cancelAnimationFrame(raf.current);
    },
    [],
  );

  return { pushRef, shieldRef, motion, onPointerDown, onPointerMove, onPointerUp };
}
