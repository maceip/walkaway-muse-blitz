/**
 * What is in the sky, and where.
 *
 * Everything here is drawn from one fixed seed, so the sky is the same sky on
 * every visit: the same stars, the same galaxies in the same places. What
 * changes is time — the stars drift, the galaxies turn, the planets go round
 * their star.
 *
 * Every point is described by where it sits in its *group*: a star layer
 * that drifts and wraps round the frame, or a system (a galaxy, a cluster, a
 * planetary system) that turns about its own centre. The vertex shader does
 * the moving, so the CPU builds this once and never touches it again.
 */

/** Seeded PRNG (mulberry32). */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function gauss(r: () => number): number {
  const u = Math.max(r(), 1e-9);
  const v = r();
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
}

export const G = {
  STAR_FAR: 0,
  STAR_MID: 1,
  STAR_NEAR: 2,
  GALAXY_A: 3,
  GALAXY_B: 4,
  GALAXY_C: 5,
  CLUSTER: 6,
  SYSTEM: 7,
  BINARY: 8,
} as const;
export const GROUP_COUNT = 9;

/** Floats per vertex: a_a(4) a_b(4) a_c(4) a_e(4). */
export const STRIDE = 16;

/**
 * Where a group sits. x and y are fractions of the frame, r is a fraction of
 * the frame's short side, pa is the position angle on screen, incl the tilt
 * of the system's plane away from face-on.
 */
export interface Placement {
  x: number;
  y: number;
  r: number;
  pa: number;
  incl: number;
  parallax: number;
}

export interface GalaxyGlow {
  group: number;
  arms: number;
  /** tan of the arm pitch: how tightly the arms wind. */
  pitch: number;
  /** angular speed of the pattern, radians per second. */
  omega: number;
  bar: number;
  edgeOn: boolean;
  brightness: number;
}

export interface Nebula {
  x: number;
  y: number;
  r: number;
  seed: number;
  c1: [number, number, number];
  c2: [number, number, number];
  strength: number;
  /** use the page accent for the second colour */
  accent: boolean;
}

export interface PlanetBody {
  x: number;
  y: number;
  r: number;
  kind: 'giant' | 'rocky';
  seed: number;
  c1: [number, number, number];
  c2: [number, number, number];
  c3: [number, number, number];
  /** radians per second the surface turns */
  spin: number;
  ringTilt: number;
  ringPa: number;
  parallax: number;
}

export interface Layout {
  groups: Placement[];
  nebulae: Nebula[];
  planets: PlanetBody[];
  /** galactic band: a point on it, its angle, its half-width as a fraction of the short side */
  band: { x: number; y: number; angle: number; width: number };
}

const DEG = Math.PI / 180;

/**
 * Two arrangements of the same sky: one for a wide frame, one for a tall one.
 * The globe owns the middle of the frame and the wordmark sits under it, so
 * the systems live round the edges in both.
 */
export function layoutFor(width: number, height: number): Layout {
  const tall = width / height < 0.85;
  const star = (parallax: number): Placement => ({ x: 0, y: 0, r: 0, pa: 0, incl: 0, parallax });

  const groups: Placement[] = tall
    ? [
        star(0.15),
        star(0.4),
        star(0.8),
        { x: 0.76, y: 0.11, r: 0.3, pa: -28 * DEG, incl: 58 * DEG, parallax: 0.25 },
        { x: 0.24, y: 0.87, r: 0.16, pa: 128 * DEG, incl: 38 * DEG, parallax: 0.25 },
        { x: 0.15, y: 0.15, r: 0.13, pa: 24 * DEG, incl: 84 * DEG, parallax: 0.25 },
        { x: 0.86, y: 0.8, r: 0.045, pa: 0, incl: 0, parallax: 0.3 },
        { x: 0.17, y: 0.6, r: 0.13, pa: -12 * DEG, incl: 70 * DEG, parallax: 0.45 },
        { x: 0.78, y: 0.94, r: 0.06, pa: 30 * DEG, incl: 50 * DEG, parallax: 0.4 },
      ]
    : [
        star(0.15),
        star(0.4),
        star(0.8),
        { x: 0.8, y: 0.27, r: 0.21, pa: -28 * DEG, incl: 58 * DEG, parallax: 0.25 },
        { x: 0.16, y: 0.78, r: 0.1, pa: 128 * DEG, incl: 38 * DEG, parallax: 0.25 },
        { x: 0.25, y: 0.15, r: 0.075, pa: 24 * DEG, incl: 84 * DEG, parallax: 0.25 },
        { x: 0.6, y: 0.09, r: 0.028, pa: 0, incl: 0, parallax: 0.3 },
        { x: 0.12, y: 0.44, r: 0.085, pa: -12 * DEG, incl: 70 * DEG, parallax: 0.45 },
        { x: 0.89, y: 0.62, r: 0.04, pa: 30 * DEG, incl: 50 * DEG, parallax: 0.4 },
      ];

  const nebulae: Nebula[] = tall
    ? [
        { x: 0.3, y: 0.42, r: 0.5, seed: 3.1, c1: [0.55, 0.12, 0.32], c2: [0.85, 0.3, 0.25], strength: 0.16, accent: false },
        { x: 0.75, y: 0.72, r: 0.45, seed: 9.7, c1: [0.08, 0.28, 0.42], c2: [0.2, 0.6, 0.55], strength: 0.12, accent: true },
        { x: 0.6, y: 0.2, r: 0.35, seed: 15.2, c1: [0.18, 0.1, 0.4], c2: [0.35, 0.25, 0.65], strength: 0.08, accent: false },
      ]
    : [
        { x: 0.3, y: 0.58, r: 0.42, seed: 3.1, c1: [0.55, 0.12, 0.32], c2: [0.85, 0.3, 0.25], strength: 0.16, accent: false },
        { x: 0.72, y: 0.84, r: 0.38, seed: 9.7, c1: [0.08, 0.28, 0.42], c2: [0.2, 0.6, 0.55], strength: 0.12, accent: true },
        { x: 0.55, y: 0.14, r: 0.3, seed: 15.2, c1: [0.18, 0.1, 0.4], c2: [0.35, 0.25, 0.65], strength: 0.08, accent: false },
      ];

  const planets: PlanetBody[] = tall
    ? [
        { x: 0.85, y: 0.635, r: 0.04, kind: 'giant', seed: 2.3, c1: [0.78, 0.66, 0.5], c2: [0.55, 0.4, 0.3], c3: [0.9, 0.84, 0.7], spin: 0.012, ringTilt: 0.26, ringPa: -18 * DEG, parallax: 0.55 },
        { x: 0.08, y: 0.33, r: 0.02, kind: 'rocky', seed: 7.9, c1: [0.62, 0.6, 0.58], c2: [0.36, 0.35, 0.36], c3: [0.75, 0.72, 0.68], spin: 0.006, ringTilt: 0, ringPa: 0, parallax: 0.6 },
      ]
    : [
        { x: 0.93, y: 0.85, r: 0.024, kind: 'giant', seed: 2.3, c1: [0.78, 0.66, 0.5], c2: [0.55, 0.4, 0.3], c3: [0.9, 0.84, 0.7], spin: 0.012, ringTilt: 0.26, ringPa: -18 * DEG, parallax: 0.55 },
        { x: 0.06, y: 0.22, r: 0.013, kind: 'rocky', seed: 7.9, c1: [0.62, 0.6, 0.58], c2: [0.36, 0.35, 0.36], c3: [0.75, 0.72, 0.68], spin: 0.006, ringTilt: 0, ringPa: 0, parallax: 0.6 },
      ];

  const band = tall
    ? { x: 0.5, y: 0.5, angle: -62 * DEG, width: 0.34 }
    : { x: 0.5, y: 0.55, angle: -24 * DEG, width: 0.2 };

  return { groups, nebulae, planets, band };
}

/** The galaxies' diffuse light, drawn in the nebula pass under their stars. */
export const GALAXIES: GalaxyGlow[] = [
  { group: G.GALAXY_A, arms: 2, pitch: 0.36, omega: (2 * Math.PI) / 1500, bar: 0, edgeOn: false, brightness: 0.8 },
  { group: G.GALAXY_B, arms: 2, pitch: 0.3, omega: -(2 * Math.PI) / 1100, bar: 0.32, edgeOn: false, brightness: 0.65 },
  { group: G.GALAXY_C, arms: 2, pitch: 0.4, omega: (2 * Math.PI) / 1800, bar: 0, edgeOn: true, brightness: 0.5 },
];

type RGB = [number, number, number];

/** Star colours by spectral class, with how common each is in a field. */
const SPECTRAL: Array<[number, RGB]> = [
  [0.06, [0.66, 0.76, 1.0]],
  [0.14, [0.8, 0.87, 1.0]],
  [0.2, [0.97, 0.97, 1.0]],
  [0.25, [1.0, 0.94, 0.83]],
  [0.22, [1.0, 0.8, 0.6]],
  [0.13, [1.0, 0.66, 0.48]],
];

function spectral(r: () => number): RGB {
  let u = r();
  for (const [w, c] of SPECTRAL) {
    if (u < w) return c;
    u -= w;
  }
  return SPECTRAL[2][1];
}

export interface Geometry {
  /** interleaved vertex data, STRIDE floats per vertex */
  points: Float32Array;
  /** how many of the leading points are field stars (drawn as a prefix) */
  starCount: number;
  lines: Float32Array;
}

/** Most field stars a frame will ever draw; small screens draw a prefix. */
export const MAX_STARS = 3600;

export function buildGeometry(): Geometry {
  const out: number[] = [];
  const r = rng(0x5eed51);

  const push = (
    a: [number, number, number, number],
    b: [number, number, number, number],
    c: [number, number, number, number],
    e: [number, number, number, number],
  ) => {
    out.push(...a, ...b, ...c, ...e);
  };

  // --- field stars -------------------------------------------------------
  // Magnitudes follow a counts-per-magnitude law (N(<m) grows ~10^(0.35m)):
  // a few bright stars and a great many faint ones, which is what a real
  // field looks like. Flux follows from magnitude, and size and brightness
  // follow from flux.
  const M_MAX = 6.5;
  const SLOPE = 0.35;
  const top = Math.pow(10, SLOPE * M_MAX) - 1;
  for (let i = 0; i < MAX_STARS; i += 1) {
    const u = r();
    const layer = u < 0.6 ? G.STAR_FAR : u < 0.9 ? G.STAR_MID : G.STAR_NEAR;
    const speed = layer === G.STAR_FAR ? 0.7 : layer === G.STAR_MID ? 1.6 : 3.2;
    const mag = Math.log10(r() * top + 1) / SLOPE;
    const flux = Math.pow(10, -0.4 * (mag - M_MAX));
    const bright = 0.11 * Math.pow(flux, 0.5);
    const size = (0.8 + 0.3 * Math.log2(flux)) * (layer === G.STAR_NEAR ? 1.15 : 1);
    const halo = flux > 40 ? 0.12 + 0.12 * Math.log10(flux / 40) : 0;
    const spike = flux > 160 ? 0.3 : 0;
    const twinkles = r() < 0.35;
    const color = spectral(r);
    push(
      [r(), r(), speed, 0],
      [size, bright, 0, r() * Math.PI * 2],
      [color[0], color[1], color[2], layer],
      [twinkles ? 0.08 + r() * 0.14 : 0, 0.35 + r() * 1.2, halo, spike],
    );
  }
  const starCount = MAX_STARS;

  // --- faint distant galaxies: smudges with no core ----------------------
  for (let i = 0; i < 26; i += 1) {
    const warm = 0.85 + r() * 0.15;
    push(
      [r(), r(), 0.7, 0],
      [0.9 + r() * 0.8, 0.05 + r() * 0.05, 0, 0],
      [1.0, warm, warm * 0.85, G.STAR_FAR],
      [0, 0, 0.9, 0],
    );
  }

  // --- spiral galaxies ---------------------------------------------------
  const galaxy = (spec: GalaxyGlow, count: number, scatter: number) => {
    const omega = spec.omega;
    const bulgeShare = spec.edgeOn ? 0.26 : 0.2;
    for (let i = 0; i < count; i += 1) {
      const kind = r();
      let radius: number;
      let angle: number;
      let z: number;
      let color: RGB;
      let bright: number;
      let size: number;
      if (kind < bulgeShare) {
        radius = Math.abs(gauss(r)) * 0.1;
        angle = r() * Math.PI * 2;
        z = gauss(r) * (spec.edgeOn ? 0.07 : 0.05);
        color = [1.0, 0.84 + r() * 0.06, 0.62 + r() * 0.08];
        bright = 0.05 + r() * 0.06;
        size = 1.1 + r() * 0.8;
      } else if (spec.bar > 0 && kind < bulgeShare + 0.1) {
        // the bar: stars strung along a line through the centre
        const x = (r() * 2 - 1) * spec.bar;
        const y = gauss(r) * 0.035;
        radius = Math.hypot(x, y);
        angle = Math.atan2(y, x);
        z = gauss(r) * 0.02;
        color = [1.0, 0.9, 0.74];
        bright = 0.05 + r() * 0.05;
        size = 1.0 + r() * 0.7;
      } else {
        radius = Math.min(1.08, -Math.log(1 - r() * 0.985) * 0.27 + (spec.bar > 0 ? spec.bar * 0.8 : 0.02));
        const inArm = r() < 0.72;
        const arm = Math.floor(r() * spec.arms);
        const armAngle = (arm * 2 * Math.PI) / spec.arms + Math.log(Math.max(radius, 0.02)) / spec.pitch;
        angle = inArm ? armAngle + gauss(r) * scatter * (0.6 + radius * 0.6) : r() * Math.PI * 2;
        z = gauss(r) * 0.018;
        const knot = inArm && r() < 0.045;
        if (knot) {
          color = [1.0, 0.45, 0.62];
          bright = 0.14 + r() * 0.1;
          size = 1.3 + r() * 0.9;
        } else if (inArm) {
          color = [0.62 + r() * 0.1, 0.74 + r() * 0.08, 1.0];
          bright = 0.04 + r() * 0.07;
          size = 0.9 + r() * 0.9;
        } else {
          color = [1.0, 0.92, 0.8];
          bright = 0.025 + r() * 0.04;
          size = 0.8 + r() * 0.6;
        }
      }
      push([radius, angle, z, omega], [size, bright, 0, r() * 6.283], [color[0], color[1], color[2], spec.group], [0, 0, 0.0, 0]);
    }
  };
  galaxy(GALAXIES[0], 9000, 0.34);
  galaxy(GALAXIES[1], 5200, 0.3);
  galaxy(GALAXIES[2], 3400, 0.36);

  // --- globular cluster: a Plummer sphere of old, warm stars --------------
  for (let i = 0; i < 1400; i += 1) {
    const u = Math.min(0.995, Math.max(0.0005, r()));
    const radius = Math.min(1.4, 0.22 / Math.sqrt(Math.pow(u, -2 / 3) - 1));
    const c = r();
    const color: RGB = c < 0.6 ? [1.0, 0.9, 0.72] : c < 0.85 ? [1.0, 0.78, 0.55] : [0.85, 0.9, 1.0];
    push(
      [radius, r() * 6.283, 0, (r() * 2 - 1) * 0.004],
      [0.8 + r() * 0.8, 0.06 + r() * 0.12, 0, r() * 6.283],
      [color[0], color[1], color[2], G.CLUSTER],
      [r() < 0.2 ? 0.1 : 0, 0.5 + r(), 0, 0],
    );
  }

  // --- a planetary system: a star and four worlds on Kepler's clock -------
  // Orbital periods go as a^1.5, so the inner world laps the outer ones.
  push([0, 0, 0, 0], [3.4, 1.5, 0, 0], [1.0, 0.93, 0.8, G.SYSTEM], [0.04, 0.6, 0.55, 0.22]);
  const worlds: Array<[number, number, RGB, number]> = [
    [0.25, 1.9, [1.0, 0.62, 0.48], 0.9],
    [0.42, 2.3, [0.55, 0.85, 1.0], 1.0],
    [0.63, 3.0, [1.0, 0.86, 0.66], 1.1],
    [0.92, 2.2, [0.7, 0.86, 1.0], 0.75],
  ];
  const innerPeriod = 75;
  const systemOrbits: number[] = [];
  for (const [a, size, color, bright] of worlds) {
    const period = innerPeriod * Math.pow(a / 0.25, 1.5);
    systemOrbits.push(a);
    push([a, r() * 6.283, 0, (2 * Math.PI) / period], [size, bright, 0, 0], [color[0], color[1], color[2], G.SYSTEM], [0, 0, 0.12, 0]);
  }

  // --- a binary: two suns round their common centre of mass -------------
  const binaryPeriod = 140;
  const phase = r() * 6.283;
  push([0.38, phase, 0, (2 * Math.PI) / binaryPeriod], [3.0, 1.2, 0, 0], [0.75, 0.85, 1.0, G.BINARY], [0.03, 0.8, 0.45, 0.2]);
  push([0.62, phase + Math.PI, 0, (2 * Math.PI) / binaryPeriod], [2.4, 0.95, 0, 0], [1.0, 0.72, 0.48, G.BINARY], [0.03, 0.9, 0.4, 0]);

  const points = new Float32Array(out);

  // --- orbit traces: hairlines, barely there ------------------------------
  const lines: number[] = [];
  const ring = (group: number, radius: number, alpha: number, color: RGB) => {
    const SEG = 120;
    for (let i = 0; i < SEG; i += 1) {
      const a0 = (i / SEG) * Math.PI * 2;
      const a1 = ((i + 1) / SEG) * Math.PI * 2;
      for (const a of [a0, a1]) {
        lines.push(radius, a, 0, 0, 1, alpha, 0, 0, color[0], color[1], color[2], group, 0, 0, 0, 0);
      }
    }
  };
  for (const a of systemOrbits) ring(G.SYSTEM, a, 0.05, [0.7, 0.85, 1.0]);
  ring(G.BINARY, 0.38, 0.04, [0.75, 0.85, 1.0]);
  ring(G.BINARY, 0.62, 0.04, [1.0, 0.8, 0.6]);

  return { points, starCount, lines: new Float32Array(lines) };
}
