/**
 * The exercise clock.
 *
 * The page runs a red team exercise on a fixed four-minute period:
 *
 *   00:00 - 03:46  nominal    the city drifts in colour
 *   03:46 - 04:00  warning    the rim pulses red, the drill is announced
 *   04:00 - 04:24  exercise   the red laser sweeps the city grid twice
 *   04:24 - 04:36  settle     the city settles into a blue glow
 *
 * The clock is a pure function of elapsed seconds, so the same numbers are
 * used by the renderer, by the countdown readout and by the tests.
 */

export const PERIOD = 240; // seconds between drills
export const FIRST_DRILL = 240; // the first drill starts one full period in
export const DRILL_LEN = 24; // red laser sweep, two passes
export const SETTLE_LEN = 12; // blue glow
export const WARN_LEN = 14; // rim pulses red before the drill
export const SWEEP_LEN = 12; // seconds for one laser pass

export type Phase = 'nominal' | 'warning' | 'exercise' | 'settle';

export const CITY_STATE: Record<Phase, string> = {
  nominal: 'Nominal',
  warning: 'Alert pending',
  exercise: 'Laser scan',
  settle: 'Blue glow',
};

/** Which phase the clock is in at `t` seconds since the page started. */
export function phaseAt(t: number): Phase {
  const prev = FIRST_DRILL + Math.floor((t - FIRST_DRILL) / PERIOD) * PERIOD;
  if (prev >= FIRST_DRILL) {
    if (t < prev + DRILL_LEN) return 'exercise';
    if (t < prev + DRILL_LEN + SETTLE_LEN) return 'settle';
  }
  const next = prev >= FIRST_DRILL ? prev + PERIOD : FIRST_DRILL;
  if (t >= next - WARN_LEN) return 'warning';
  return 'nominal';
}

/** The next drill start, in seconds since the page started. */
export function nextDrillAt(t: number): number {
  if (t < FIRST_DRILL) return FIRST_DRILL;
  return FIRST_DRILL + Math.ceil((t - FIRST_DRILL) / PERIOD) * PERIOD;
}

/** Which laser pass of the drill is running (1 or 2). */
export function sweepPassAt(t: number): number {
  const prev = FIRST_DRILL + Math.floor((t - FIRST_DRILL) / PERIOD) * PERIOD;
  return Math.min(2, Math.floor((t - prev) / SWEEP_LEN) + 1);
}

/** "T-04:00" style countdown to the next drill. */
export function formatCountdown(t: number): string {
  const left = Math.max(0, Math.ceil(nextDrillAt(t) - t));
  const pad = (n: number) => (n < 10 ? `0${n}` : `${n}`);
  return `T-${pad(Math.floor(left / 60))}:${pad(left % 60)}`;
}

/**
 * The globe's own drift: a slow hue rotation and a slow breathing of the
 * city's neon. The range is narrow on purpose — the sphere is green at the
 * rim and magenta in the bay, and a wide swing would drag the green into
 * orange and the magenta into brown.
 */
export const SHIFT_BASE = 10;
export const SHIFT_SWING = 9;
export const SHIFT_PERIOD = 110;
export const LIFT_PERIOD = 37;

export interface Drift {
  /** Hue rotation applied to the globe image. */
  shift: number;
  /** Brightness multiplier for the globe's neon. */
  lift: number;
  /** Accent hue for the page glow. */
  accentHue: number;
}

/** The drift at `t` seconds, for the given phase. */
export function driftAt(t: number, phase: Phase): Drift {
  if (phase === 'exercise' || phase === 'warning') {
    return { shift: -4, lift: 1, accentHue: 354 };
  }
  if (phase === 'settle') {
    return { shift: -10, lift: 1.02, accentHue: 208 };
  }
  const shift = SHIFT_BASE + SHIFT_SWING * Math.sin((t / SHIFT_PERIOD) * Math.PI * 2);
  const lift = 0.95 + 0.07 * Math.sin((t / LIFT_PERIOD) * Math.PI * 2 + 1);
  return {
    shift: -shift,
    lift,
    accentHue: 140 + 26 * Math.sin((t / 90) * Math.PI * 2),
  };
}

/** HSL to an rgb() string, for the accent that drives the page glow. */
export function hslToRgb(h: number, s: number, l: number): string {
  const hh = (((h % 360) + 360) % 360) / 360;
  let r: number;
  let g: number;
  let b: number;
  if (s === 0) {
    r = g = b = l;
  } else {
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    const hue2rgb = (t: number): number => {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    };
    r = hue2rgb(hh + 1 / 3);
    g = hue2rgb(hh);
    b = hue2rgb(hh - 1 / 3);
  }
  return `rgb(${Math.round(r * 255)}, ${Math.round(g * 255)}, ${Math.round(b * 255)})`;
}

/** The accent colour, and its components, for the given drift. */
export function accentFrom(drift: Drift): { color: string; rgb: string } {
  const hh = (((drift.accentHue % 360) + 360) % 360) / 360;
  const s = drift.accentHue === 354 ? 0.92 : drift.accentHue === 208 ? 0.95 : 0.85;
  const l = drift.accentHue === 354 ? 0.56 : 0.58;
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const hue2rgb = (t: number): number => {
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };
  const r = Math.round(hue2rgb(hh + 1 / 3) * 255);
  const g = Math.round(hue2rgb(hh) * 255);
  const b = Math.round(hue2rgb(hh - 1 / 3) * 255);
  return { color: `rgb(${r}, ${g}, ${b})`, rgb: `${r}, ${g}, ${b}` };
}
