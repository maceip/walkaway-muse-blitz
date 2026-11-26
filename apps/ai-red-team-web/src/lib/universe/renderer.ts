import { FULLSCREEN_VS, canRenderFloat, freeTarget, link, makeTarget, type Program, type Target } from '../gl';
import { COMET_FS, COMPOSITE_FS, NEBULA_FS, PLANET_FS, POINTS_FS, POINTS_VS } from './shaders';
import { GALAXIES, GROUP_COUNT, MAX_STARS, STRIDE, buildGeometry, layoutFor, rng, type Layout } from './sky';

/**
 * The live sky.
 *
 * One WebGL2 canvas behind the page. Each frame it lays down the diffuse sky
 * (gas, the galactic band, the galaxies' own light), then every star and
 * world as a point, then the planets, then — now and then — a comet. The
 * stars drift slowly and wrap round the frame, nearer layers faster than far
 * ones; the galaxies turn on periods of twenty to thirty minutes; the worlds
 * of the planetary system go round their star on Kepler's clock; and a slow
 * camera drift slides it all with parallax.
 *
 * It runs at thirty frames a second, because nothing in it moves fast enough
 * to need more, and it stops when the tab is hidden. With reduced motion it
 * draws one still frame and redraws only when the window changes size.
 */

export interface UniverseHandle {
  stop(): void;
}

const FPS = 30;
/** The diffuse sky is redrawn every this many frames. */
const NEBULA_EVERY = 8;
/** Most canvas pixels the sky will draw: a 4K retina screen is capped. */
const MAX_PIXELS = 7_000_000;

function readAccent(): [number, number, number] {
  const raw = document.documentElement.style.getPropertyValue('--accent-rgb');
  const parts = raw.split(',').map((s) => parseFloat(s));
  if (parts.length === 3 && parts.every((n) => Number.isFinite(n))) {
    return [parts[0] / 255, parts[1] / 255, parts[2] / 255];
  }
  return [63 / 255, 191 / 255, 114 / 255];
}

interface Comet {
  start: [number, number];
  end: [number, number];
  begin: number;
  duration: number;
}

/**
 * Whether a comet is crossing in this stretch of time, and where. Comets come
 * on a fixed, seeded timetable — about one every few minutes — and take most
 * of a minute to cross.
 */
function cometFor(t: number): Comet | null {
  const CYCLE = 150;
  const cycle = Math.floor(t / CYCLE);
  const r = rng(0xc0ffee + cycle * 7919);
  if (r() < 0.45) return null;
  const begin = cycle * CYCLE + 10 + r() * 60;
  const duration = 48 + r() * 24;
  const leftToRight = r() < 0.5;
  const y0 = 0.06 + r() * 0.3;
  const y1 = y0 + (r() * 0.3 - 0.05);
  const start: [number, number] = leftToRight ? [-0.08, y0] : [1.08, y0];
  const end: [number, number] = leftToRight ? [1.08, y1] : [-0.08, y1];
  return { start, end, begin, duration };
}

export function startUniverse(
  canvas: HTMLCanvasElement,
  options: { reducedMotion: boolean },
): UniverseHandle | null {
  const gl = canvas.getContext('webgl2', {
    alpha: false,
    antialias: false,
    depth: false,
    stencil: false,
    premultipliedAlpha: false,
    powerPreference: 'low-power',
    preserveDrawingBuffer: false,
  });
  if (!gl) return null;

  let points: Program;
  let nebula: Program;
  let composite: Program;
  let planet: Program;
  let comet: Program;
  try {
    points = link(gl, POINTS_VS, POINTS_FS);
    nebula = link(gl, FULLSCREEN_VS, NEBULA_FS);
    composite = link(gl, FULLSCREEN_VS, COMPOSITE_FS);
    planet = link(gl, FULLSCREEN_VS, PLANET_FS);
    comet = link(gl, FULLSCREEN_VS, COMET_FS);
  } catch (error) {
    console.warn('[universe] shaders failed, falling back to the drawn sky', error);
    return null;
  }

  const geometry = buildGeometry();
  const float = canRenderFloat(gl);

  const vao = (data: Float32Array) => {
    const v = gl.createVertexArray();
    const b = gl.createBuffer();
    gl.bindVertexArray(v);
    gl.bindBuffer(gl.ARRAY_BUFFER, b);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    for (let i = 0; i < 4; i += 1) {
      gl.enableVertexAttribArray(i);
      gl.vertexAttribPointer(i, 4, gl.FLOAT, false, STRIDE * 4, i * 16);
    }
    gl.bindVertexArray(null);
    return { v, b };
  };
  const pointsVao = vao(geometry.points);
  const linesVao = vao(geometry.lines);
  const emptyVao = gl.createVertexArray();
  const otherCount = geometry.points.length / STRIDE - geometry.starCount;
  const lineCount = geometry.lines.length / STRIDE;

  let width = 0;
  let height = 0;
  let dpr = 1;
  let layout: Layout = layoutFor(1, 1);
  let margin: [number, number] = [0, 0];
  let span: [number, number] = [0, 0];
  let starsDrawn = MAX_STARS;
  let nebTarget: Target | null = null;
  let nebDirty = true;
  const g0 = new Float32Array(GROUP_COUNT * 4);
  const g1 = new Float32Array(GROUP_COUNT * 4);
  let accent = readAccent();

  const resize = () => {
    const cssW = Math.max(1, canvas.clientWidth);
    const cssH = Math.max(1, canvas.clientHeight);
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    if (cssW * cssH * dpr * dpr > MAX_PIXELS) dpr = Math.sqrt(MAX_PIXELS / (cssW * cssH));
    width = Math.round(cssW * dpr);
    height = Math.round(cssH * dpr);
    canvas.width = width;
    canvas.height = height;
    layout = layoutFor(cssW, cssH);
    const m = 0.06 * Math.max(width, height);
    margin = [m, m];
    span = [width + 2 * m, height + 2 * m];
    // a small screen draws a prefix of the stars, so the density per square
    // inch stays about the same on a phone and a desktop
    const area = cssW * cssH;
    starsDrawn = Math.round(Math.min(MAX_STARS, Math.max(1100, (area / 2_000_000) * MAX_STARS)));

    const minDim = Math.min(width, height);
    layout.groups.forEach((p, i) => {
      g0[i * 4 + 0] = p.x * width;
      g0[i * 4 + 1] = p.y * height;
      g0[i * 4 + 2] = p.r * minDim;
      g0[i * 4 + 3] = p.pa;
      g1[i * 4 + 0] = Math.cos(p.incl);
      g1[i * 4 + 1] = Math.sin(p.incl);
      g1[i * 4 + 2] = p.parallax;
      g1[i * 4 + 3] = i < 3 ? 0 : 1;
    });

    const scale = 3;
    const nw = Math.max(64, Math.min(1100, Math.ceil(span[0] / scale)));
    const nh = Math.max(64, Math.min(1100, Math.ceil(span[1] / scale)));
    if (!nebTarget || nebTarget.width !== nw || nebTarget.height !== nh) {
      freeTarget(gl, nebTarget);
      nebTarget = makeTarget(gl, nw, nh, float);
    }
    nebDirty = true;
  };

  /** The camera: a slow, bounded drift on two incommensurate periods. */
  const cameraAt = (t: number): [number, number] => [
    Math.sin((t * 2 * Math.PI) / 420) * 0.028 * width + Math.sin((t * 2 * Math.PI) / 97) * 0.003 * width,
    Math.sin((t * 2 * Math.PI) / 530 + 1.3) * 0.022 * height,
  ];

  const drawNebula = (t: number) => {
    if (!nebTarget) return;
    const minDim = Math.min(width, height);
    gl.bindFramebuffer(gl.FRAMEBUFFER, nebTarget.fbo);
    gl.viewport(0, 0, nebTarget.width, nebTarget.height);
    gl.disable(gl.BLEND);
    gl.useProgram(nebula.program);
    gl.uniform2f(nebula.uniform('u_fbo'), nebTarget.width, nebTarget.height);
    gl.uniform2f(nebula.uniform('u_span'), span[0], span[1]);
    gl.uniform2f(nebula.uniform('u_margin'), margin[0], margin[1]);
    gl.uniform1f(nebula.uniform('u_minDim'), minDim);
    gl.uniform1f(nebula.uniform('u_time'), t);
    gl.uniform3f(nebula.uniform('u_accent'), accent[0], accent[1], accent[2]);

    const gal0 = new Float32Array(12);
    const gal1 = new Float32Array(12);
    const gal2 = new Float32Array(12);
    GALAXIES.forEach((spec, i) => {
      const p = layout.groups[spec.group];
      gal0.set([p.x * width, p.y * height, p.r * minDim, p.pa], i * 4);
      gal1.set([Math.cos(p.incl), spec.omega * t, spec.arms, spec.pitch], i * 4);
      gal2.set([spec.bar, spec.edgeOn ? 1 : 0, spec.brightness, 0], i * 4);
    });
    gl.uniform4fv(nebula.uniform('u_gal0'), gal0);
    gl.uniform4fv(nebula.uniform('u_gal1'), gal1);
    gl.uniform4fv(nebula.uniform('u_gal2'), gal2);

    const n0 = new Float32Array(12);
    const n1 = new Float32Array(12);
    const n2 = new Float32Array(12);
    layout.nebulae.forEach((n, i) => {
      n0.set([n.x * width, n.y * height, n.r * minDim, n.seed], i * 4);
      n1.set([...n.c1, n.strength], i * 4);
      n2.set([...n.c2, n.accent ? 1 : 0], i * 4);
    });
    gl.uniform4fv(nebula.uniform('u_neb0'), n0);
    gl.uniform4fv(nebula.uniform('u_neb1'), n1);
    gl.uniform4fv(nebula.uniform('u_neb2'), n2);
    const b = layout.band;
    gl.uniform4f(nebula.uniform('u_band'), b.x * width, b.y * height, b.angle, b.width);

    gl.bindVertexArray(emptyVao);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  };

  let frame = 0;
  const render = (t: number) => {
    if (gl.isContextLost()) return;
    if (frame % 30 === 0) {
      const next = readAccent();
      if (next.some((v, i) => Math.abs(v - accent[i]) > 0.01)) {
        accent = next;
        nebDirty = true;
      }
    }
    if (nebDirty || frame % NEBULA_EVERY === 0) {
      drawNebula(t);
      nebDirty = false;
    }
    const cam = cameraAt(t);

    gl.viewport(0, 0, width, height);
    gl.disable(gl.SCISSOR_TEST);

    // the diffuse sky
    gl.disable(gl.BLEND);
    gl.useProgram(composite.program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, nebTarget ? nebTarget.tex : null);
    gl.uniform1i(composite.uniform('u_neb'), 0);
    gl.uniform2f(composite.uniform('u_res'), width, height);
    gl.uniform2f(composite.uniform('u_span'), span[0], span[1]);
    gl.uniform2f(composite.uniform('u_margin'), margin[0], margin[1]);
    gl.uniform2f(composite.uniform('u_camera'), cam[0], cam[1]);
    gl.uniform1f(composite.uniform('u_parallax'), 0.25);
    gl.uniform1f(composite.uniform('u_frame'), frame % 64);
    gl.bindVertexArray(emptyVao);
    gl.drawArrays(gl.TRIANGLES, 0, 3);

    // the stars and worlds, added on
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE);
    gl.useProgram(points.program);
    gl.uniform2f(points.uniform('u_res'), width, height);
    gl.uniform1f(points.uniform('u_dpr'), dpr);
    gl.uniform1f(points.uniform('u_time'), t);
    gl.uniform2f(points.uniform('u_camera'), cam[0], cam[1]);
    const driftAngle = -0.24 + Math.sin((t * 2 * Math.PI) / 900) * 0.18;
    gl.uniform2f(points.uniform('u_drift'), -Math.cos(driftAngle), Math.sin(driftAngle) * 0.6);
    gl.uniform2f(points.uniform('u_margin'), margin[0], margin[1]);
    gl.uniform4fv(points.uniform('u_g0'), g0);
    gl.uniform4fv(points.uniform('u_g1'), g1);
    gl.uniform1i(points.uniform('u_lines'), 1);
    gl.bindVertexArray(linesVao.v);
    gl.drawArrays(gl.LINES, 0, lineCount);
    gl.uniform1i(points.uniform('u_lines'), 0);
    gl.bindVertexArray(pointsVao.v);
    gl.drawArrays(gl.POINTS, 0, starsDrawn);
    gl.drawArrays(gl.POINTS, geometry.starCount, otherCount);

    // the planets, in front of every star
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.useProgram(planet.program);
    gl.uniform2f(planet.uniform('u_res'), width, height);
    gl.enable(gl.SCISSOR_TEST);
    gl.bindVertexArray(emptyVao);
    const minDim = Math.min(width, height);
    for (const p of layout.planets) {
      const radius = p.r * minDim;
      const cx = p.x * width + cam[0] * p.parallax;
      const cy = p.y * height + cam[1] * p.parallax;
      const reach = radius * (p.kind === 'giant' ? 2.4 : 1.1) + 2;
      const sx = Math.floor(cx - reach);
      const sy = Math.floor(height - (cy + reach));
      gl.scissor(sx, sy, Math.ceil(reach * 2), Math.ceil(reach * 2));
      gl.uniform4f(planet.uniform('u_p0'), cx, cy, radius, p.kind === 'giant' ? 1 : 0);
      gl.uniform4f(planet.uniform('u_p1'), p.spin * t, p.ringTilt, p.ringPa, p.seed);
      gl.uniform3f(planet.uniform('u_c1'), ...p.c1);
      gl.uniform3f(planet.uniform('u_c2'), ...p.c2);
      gl.uniform3f(planet.uniform('u_c3'), ...p.c3);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }

    // a comet, if one is due
    const c = options.reducedMotion ? null : cometFor(t);
    if (c && t >= c.begin && t <= c.begin + c.duration) {
      const k = (t - c.begin) / c.duration;
      const hx = (c.start[0] + (c.end[0] - c.start[0]) * k) * width;
      const hy = (c.start[1] + (c.end[1] - c.start[1]) * k) * height;
      const dx = (c.end[0] - c.start[0]) * width;
      const dy = (c.end[1] - c.start[1]) * height;
      const len = Math.hypot(dx, dy) || 1;
      const tailLen = 0.12 * minDim;
      const fade = Math.min(1, k / 0.12, (1 - k) / 0.12) * 0.8;
      const reach = tailLen * 2.2;
      gl.blendFunc(gl.ONE, gl.ONE);
      gl.scissor(Math.floor(hx - reach), Math.floor(height - hy - reach), Math.ceil(reach * 2), Math.ceil(reach * 2));
      gl.useProgram(comet.program);
      gl.uniform2f(comet.uniform('u_res'), width, height);
      gl.uniform2f(comet.uniform('u_head'), hx, hy);
      gl.uniform2f(comet.uniform('u_dir'), dx / len, dy / len);
      gl.uniform1f(comet.uniform('u_len'), tailLen);
      gl.uniform1f(comet.uniform('u_fade'), fade);
      gl.uniform1f(comet.uniform('u_dpr'), dpr);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }
    gl.disable(gl.SCISSOR_TEST);
    gl.disable(gl.BLEND);
    frame += 1;
  };

  // The sky starts part-way into its own timeline so that the first thing
  // anybody sees is not every system at its zero angle.
  const T0 = 3600;
  const origin = performance.now();
  let raf = 0;
  let last = 0;
  let stopped = false;

  const loop = (now: number) => {
    raf = 0;
    if (stopped) return;
    if (now - last >= 1000 / FPS - 2) {
      last = now;
      render(T0 + (now - origin) / 1000);
    }
    raf = requestAnimationFrame(loop);
  };

  const still = () => render(T0);

  resize();
  const observer = new ResizeObserver(() => {
    resize();
    if (options.reducedMotion) still();
  });
  observer.observe(canvas);

  const onLost = (event: Event) => {
    event.preventDefault();
    if (raf) cancelAnimationFrame(raf);
    raf = 0;
  };
  canvas.addEventListener('webglcontextlost', onLost);

  if (options.reducedMotion) {
    still();
  } else {
    raf = requestAnimationFrame(loop);
  }
  canvas.dataset.renderer = 'webgl2';

  return {
    stop() {
      stopped = true;
      if (raf) cancelAnimationFrame(raf);
      observer.disconnect();
      canvas.removeEventListener('webglcontextlost', onLost);
      freeTarget(gl, nebTarget);
      gl.deleteVertexArray(pointsVao.v);
      gl.deleteBuffer(pointsVao.b);
      gl.deleteVertexArray(linesVao.v);
      gl.deleteBuffer(linesVao.b);
      gl.deleteVertexArray(emptyVao);
      for (const p of [points, nebula, composite, planet, comet]) gl.deleteProgram(p.program);
    },
  };
}
