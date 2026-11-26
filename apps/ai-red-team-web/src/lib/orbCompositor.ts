import { FULLSCREEN_VS, NOISE_GLSL, canRenderFloat, freeTarget, link, makeTarget, type Program, type Target } from './gl';
import type { Motion } from './useOrbInteraction';
import { TILT_MAX } from './useOrbInteraction';

/**
 * The globe, drawn through a lens.
 *
 * The scenes are still pictures and films on the page — the <img> and
 * <video> elements in Orb.tsx — and this draws them again through WebGL so
 * they read as photographed rather than pasted:
 *
 *  - **Light is mixed as light.** The crossing between scenes is done in
 *    linear light, not on the encoded sRGB values, so two scenes crossing do
 *    not sag into a muddy, darker middle.
 *  - **A focus pull, not a dissolve.** As one scene crosses into the next
 *    the lens drifts out of focus and back, and the new scene comes in
 *    through a soft, uneven front rather than all at once. Two different
 *    shots never line up exactly; a rack focus is how film hides that.
 *  - **Bloom.** The neon and the lit glass glow into their surroundings the
 *    way bright light does through a real lens.
 *  - **The glass.** A little dispersion toward the rim of the sphere, and a
 *    window reflection that stays put while the globe tips under it.
 *  - **Grain**, very faint, so the flat colour of a render reads as film.
 *
 * Nothing is drawn unless something changed: a new film frame, a crossing in
 * progress, the globe wobbling, or the colour drift moving on. A still scene
 * with nobody touching it costs nothing.
 *
 * If WebGL2 is not there, none of this runs and the page shows the <img> and
 * <video> elements themselves, crossfaded by the stylesheet, as before.
 */

/** How long a crossing takes, in ms. useScene keeps the old scene this long. */
export const TRANSITION_MS = 2600;

const SCENE_FS = /* glsl */ `#version 300 es
precision highp float;
in vec2 v_uv;
uniform sampler2D u_a;
uniform sampler2D u_b;
uniform float u_p;        // 0: all outgoing, 1: all arriving
uniform float u_time;
uniform float u_still;    // 1 with reduced motion: no zoom, no focus pull
uniform mat3 u_hue;
uniform float u_lift;
uniform float u_encode;   // 1 when the target is 8-bit and must stay encoded
out vec4 o;
${NOISE_GLSL}

vec3 toLinear(vec3 c) { return pow(max(c, 0.0), vec3(2.2)); }

vec3 grade(vec3 srgb) {
  // the page's slow colour drift, applied the way the stylesheet's
  // hue-rotate() and brightness() would apply it
  return clamp(u_hue * srgb, 0.0, 1.0) * u_lift;
}

vec3 tap(sampler2D t, vec2 uv) {
  return toLinear(grade(texture(t, uv).rgb));
}

vec3 lensed(sampler2D t, vec2 uv, float blur) {
  vec3 c = tap(t, uv);
  if (blur < 0.0002) return c;
  // a small disc of taps on a golden-angle spiral: the out-of-focus blur of
  // a lens, in linear light so highlights spread the way light does
  vec3 sum = c;
  float wsum = 1.0;
  for (int i = 1; i < 9; i++) {
    float fi = float(i);
    float r = sqrt(fi / 8.0) * blur;
    float a = fi * 2.39996;
    sum += tap(t, uv + vec2(cos(a), sin(a)) * r);
    wsum += 1.0;
  }
  return sum / wsum;
}

void main() {
  vec2 uv = vec2(v_uv.x, 1.0 - v_uv.y);
  float p = clamp(u_p, 0.0, 1.0);
  float bell = sin(3.14159265 * p);
  float motion = 1.0 - u_still;

  // the arriving scene settles as it comes in, the outgoing one drifts on
  float sa = 1.0 + 0.03 * (1.0 - p) * motion;
  float sb = 1.0 + 0.03 * p * motion;
  vec2 ua = 0.5 + (uv - 0.5) / sa;
  vec2 ub = 0.5 + (uv - 0.5) / sb;
  float blur = 0.0042 * bell * motion;

  vec3 a = lensed(u_a, ua, blur);
  vec3 col = a;
  if (p < 0.999) {
    vec3 b = lensed(u_b, ub, blur);
    // an uneven front: the new light reaches some of the city first, the
    // middle of the globe a little before the rim
    float n = fbm(uv * 3.2 + vec2(u_time * 0.03, -u_time * 0.02), 4);
    float centre = 0.5 - length(uv - 0.5);
    float m = clamp((p - 0.5) * 1.9 + 0.5 + (n * 1.1 + centre * 0.5) * bell * motion, 0.0, 1.0);
    m = m * m * (3.0 - 2.0 * m);
    col = mix(b, a, m);
  }
  if (u_encode > 0.5) col = pow(col, vec3(1.0 / 2.2));
  o = vec4(col, 1.0);
}
`;

const BRIGHT_FS = /* glsl */ `#version 300 es
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec2 u_texel;
uniform float u_decode;
out vec4 o;
vec3 fetchLin(vec2 uv) {
  vec3 c = texture(u_src, uv).rgb;
  return u_decode > 0.5 ? pow(c, vec3(2.2)) : c;
}
void main() {
  // a 4x4 box down to quarter size, then a soft-knee threshold
  vec3 c = vec3(0.0);
  for (int y = -1; y <= 2; y++)
    for (int x = -1; x <= 2; x++)
      c += fetchLin(v_uv + (vec2(x, y) - 0.5) * u_texel);
  c /= 16.0;
  float l = max(c.r, max(c.g, c.b));
  float knee = 0.22;
  float threshold = 0.55;
  float soft = clamp(l - threshold + knee, 0.0, 2.0 * knee);
  soft = soft * soft / (4.0 * knee + 1e-4);
  float w = max(soft, l - threshold) / max(l, 1e-4);
  o = vec4(c * w, 1.0);
}
`;

const BLUR_FS = /* glsl */ `#version 300 es
precision highp float;
in vec2 v_uv;
uniform sampler2D u_src;
uniform vec2 u_dir;
out vec4 o;
void main() {
  // 9-tap gaussian folded into 5 linear fetches
  vec3 c = texture(u_src, v_uv).rgb * 0.2270270270;
  c += texture(u_src, v_uv + u_dir * 1.3846153846).rgb * 0.3162162162;
  c += texture(u_src, v_uv - u_dir * 1.3846153846).rgb * 0.3162162162;
  c += texture(u_src, v_uv + u_dir * 3.2307692308).rgb * 0.0702702703;
  c += texture(u_src, v_uv - u_dir * 3.2307692308).rgb * 0.0702702703;
  o = vec4(c, 1.0);
}
`;

const FINAL_FS = /* glsl */ `#version 300 es
precision highp float;
in vec2 v_uv;
uniform sampler2D u_scene;
uniform sampler2D u_bloom;
uniform float u_decode;
uniform float u_bloomGain;
uniform vec2 u_glint;
uniform float u_shield;
uniform float u_frame;
uniform vec3 u_shieldColor;
out vec4 o;
${NOISE_GLSL}
vec3 sceneLin(vec2 uv) {
  vec3 c = texture(u_scene, uv).rgb;
  return u_decode > 0.5 ? pow(c, vec3(2.2)) : c;
}
void main() {
  vec2 uv = v_uv;
  vec2 d = uv - 0.5;
  float r = length(d) * 2.0;

  // dispersion in the glass: it grows toward the rim of the sphere
  float ca = 0.0026 * r * r * r;
  vec3 col;
  col.r = sceneLin(uv - d * ca).r;
  col.g = sceneLin(uv).g;
  col.b = sceneLin(uv + d * ca).b;

  col += texture(u_bloom, uv).rgb * u_bloomGain;

  // a window reflection on the glass, upper left; it holds still while the
  // globe tips under it, which is how a reflection behaves
  vec2 img = vec2(uv.x, 1.0 - uv.y);
  vec2 gc = vec2(0.33, 0.25) + u_glint;
  float glint = exp(-pow(length((img - gc) * vec2(1.0, 1.7)) / 0.13, 2.0));
  col += vec3(0.9, 0.95, 1.0) * glint * 0.035;

  // the shield lights the glass from outside when it flares
  float rim = smoothstep(0.7, 1.0, r);
  col += u_shieldColor * rim * rim * u_shield * 0.18;

  // a soft shoulder, so bloom on bright neon rolls off instead of clipping
  vec3 knee = vec3(0.82);
  vec3 over = max(col - knee, 0.0);
  col = min(col, knee) + (1.0 - knee) * (1.0 - exp(-over / (1.0 - knee)));

  vec3 srgb = pow(clamp(col, 0.0, 1.0), vec3(1.0 / 2.2));
  // fine grain, as film has
  float g = hash12(gl_FragCoord.xy + vec2(u_frame * 13.0, u_frame * 7.0)) - 0.5;
  srgb += g * 0.014;
  o = vec4(srgb, 1.0);
}
`;

interface Slot {
  tex: WebGLTexture;
  sceneId: string | null;
  /** the element the slot was last filled from */
  source: HTMLImageElement | HTMLVideoElement | null;
  /** the image URL or film time last uploaded */
  key: string;
  ready: boolean;
}

export interface OrbCompositor {
  stop(): void;
}

function hueMatrix(deg: number): Float32Array {
  const a = (deg * Math.PI) / 180;
  const c = Math.cos(a);
  const s = Math.sin(a);
  // the hue-rotate() matrix from the CSS Filter Effects spec, row-major
  return new Float32Array([
    0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928,
    0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.14, 0.072 - c * 0.072 - s * 0.283,
    0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072,
  ]);
}

function readNumber(name: string, fallback: number): number {
  const v = parseFloat(document.documentElement.style.getPropertyValue(name));
  return Number.isFinite(v) ? v : fallback;
}

function readAccent(): [number, number, number] {
  const raw = document.documentElement.style.getPropertyValue('--accent-rgb');
  const parts = raw.split(',').map((s) => parseFloat(s) / 255);
  return parts.length === 3 && parts.every((n) => Number.isFinite(n))
    ? [parts[0], parts[1], parts[2]]
    : [0.25, 0.75, 0.45];
}

/**
 * A count of the frames each film has presented. Where the browser can say
 * when a film has a new frame (requestVideoFrameCallback), the globe uploads
 * exactly the frames the film has — 24 a second — and not one per screen
 * refresh. Where it cannot, the film's clock stands in.
 */
const filmFrames = new WeakMap<HTMLVideoElement, number>();

function filmKey(video: HTMLVideoElement): string {
  type Rvfc = (cb: () => void) => number;
  const rvfc = (video as unknown as { requestVideoFrameCallback?: Rvfc }).requestVideoFrameCallback;
  if (typeof rvfc !== 'function') return `${video.currentSrc}:${video.currentTime}`;
  if (!filmFrames.has(video)) {
    filmFrames.set(video, 0);
    const tick = () => {
      filmFrames.set(video, (filmFrames.get(video) ?? 0) + 1);
      if (video.isConnected) rvfc.call(video, tick);
    };
    rvfc.call(video, tick);
  }
  return `${video.currentSrc}:f${filmFrames.get(video)}`;
}

/**
 * Which element to draw a scene layer from: the film once it has a frame,
 * otherwise the picture once it has loaded, otherwise nothing yet.
 */
function sourceOf(layer: Element | null): HTMLImageElement | HTMLVideoElement | null {
  if (!layer) return null;
  const video = layer.querySelector('video');
  if (video && video.readyState >= 2 && video.videoWidth > 0) return video;
  const img = layer.querySelector('img');
  if (img && img.complete && img.naturalWidth > 0) return img;
  return null;
}

export function startOrbCompositor(
  canvas: HTMLCanvasElement,
  host: HTMLElement,
  motion: React.RefObject<Motion>,
  options: { reducedMotion: boolean },
): OrbCompositor | null {
  const gl = canvas.getContext('webgl2', {
    alpha: false,
    antialias: false,
    depth: false,
    stencil: false,
    premultipliedAlpha: true,
    preserveDrawingBuffer: false,
  });
  if (!gl) return null;

  let scene: Program;
  let bright: Program;
  let blur: Program;
  let final: Program;
  try {
    scene = link(gl, FULLSCREEN_VS, SCENE_FS);
    bright = link(gl, FULLSCREEN_VS, BRIGHT_FS);
    blur = link(gl, FULLSCREEN_VS, BLUR_FS);
    final = link(gl, FULLSCREEN_VS, FINAL_FS);
  } catch (error) {
    console.warn('[orb] shaders failed, showing the scenes directly', error);
    return null;
  }
  const float = canRenderFloat(gl);
  const vao = gl.createVertexArray();

  const makeSlot = (): Slot => {
    const tex = gl.createTexture();
    if (!tex) throw new Error('createTexture failed');
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([0, 0, 0, 255]));
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return { tex, sceneId: null, source: null, key: '', ready: false };
  };
  let current = makeSlot();
  let outgoing = makeSlot();

  let width = 0;
  let height = 0;
  let sceneTarget: Target | null = null;
  let bloomA: Target | null = null;
  let bloomB: Target | null = null;
  let dirty = true;

  const resize = () => {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.max(16, Math.round(canvas.clientWidth * dpr));
    const h = Math.max(16, Math.round(canvas.clientHeight * dpr));
    if (w === width && h === height) return;
    width = w;
    height = h;
    canvas.width = w;
    canvas.height = h;
    freeTarget(gl, sceneTarget);
    freeTarget(gl, bloomA);
    freeTarget(gl, bloomB);
    sceneTarget = makeTarget(gl, w, h, float);
    const bw = Math.max(8, Math.round(w / 4));
    const bh = Math.max(8, Math.round(h / 4));
    bloomA = makeTarget(gl, bw, bh, float);
    bloomB = makeTarget(gl, bw, bh, float);
    dirty = true;
  };

  /** Fill a slot from its element if the element has something new. */
  const upload = (slot: Slot, el: HTMLImageElement | HTMLVideoElement | null): boolean => {
    if (!el) return false;
    const key = el instanceof HTMLVideoElement ? `v:${filmKey(el)}` : `i:${el.currentSrc}`;
    if (slot.source === el && slot.key === key) return false;
    try {
      gl.bindTexture(gl.TEXTURE_2D, slot.tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, el);
    } catch {
      return false;
    }
    slot.source = el;
    slot.key = key;
    slot.ready = true;
    return true;
  };

  let transitionStart = -1;
  let shownScene = '';
  let lastShift = NaN;
  let lastLift = NaN;
  let lastMotionKey = '';
  let frame = 0;

  const render = (now: number) => {
    if (gl.isContextLost() || !sceneTarget || !bloomA || !bloomB) return;

    const currentLayer = host.querySelector('[data-role="current"]');
    const previousLayer = host.querySelector('[data-role="previous"]');
    const id = currentLayer?.getAttribute('data-scene-id') ?? null;

    // a new scene: what was showing becomes the outgoing picture
    if (id && id !== current.sceneId) {
      const progress = transitionStart < 0 ? 1 : (now - transitionStart) / TRANSITION_MS;
      if (current.ready && (progress >= 0.5 || !outgoing.ready)) {
        const spare = outgoing;
        outgoing = current;
        current = spare;
      }
      current.sceneId = id;
      current.ready = false;
      current.source = null;
      current.key = '';
      transitionStart = -1;
      canvas.dataset.transition = 'pending';
    }

    let changed = false;
    // keep the outgoing film playing for as long as its element is still
    // on the page; after that it holds its last frame
    if (outgoing.ready && previousLayer?.getAttribute('data-scene-id') === outgoing.sceneId) {
      changed = upload(outgoing, sourceOf(previousLayer)) || changed;
    }
    const wasReady = current.ready;
    changed = upload(current, sourceOf(currentLayer)) || changed;
    if (!wasReady && current.ready) {
      // the arriving scene is on the page: the crossing starts now, or not
      // at all if there was nothing showing before it
      transitionStart = outgoing.ready ? now : now - TRANSITION_MS;
      canvas.dataset.transition = outgoing.ready ? 'crossing' : 'idle';
    }
    if (!current.ready && !outgoing.ready) return;

    let p = 1;
    if (!current.ready) p = 0;
    else if (transitionStart >= 0) p = Math.min(1, (now - transitionStart) / TRANSITION_MS);
    const crossing = current.ready && p < 1;
    if (current.ready && p >= 1 && canvas.dataset.transition === 'crossing') {
      canvas.dataset.transition = 'idle';
    }
    if (current.ready && p >= 1 && shownScene !== current.sceneId) {
      shownScene = current.sceneId ?? '';
      canvas.dataset.scene = shownScene;
    }

    const shift = readNumber('--shift', 0);
    const lift = readNumber('--lift', 1);
    const m = motion.current;
    const motionKey = m
      ? `${m.rx.toFixed(2)}|${m.ry.toFixed(2)}|${m.shield.toFixed(3)}`
      : '';
    const graded = Math.abs(shift - lastShift) > 0.4 || Math.abs(lift - lastLift) > 0.004;
    if (!(dirty || changed || crossing || graded || motionKey !== lastMotionKey)) return;
    dirty = false;
    lastShift = shift;
    lastLift = lift;
    lastMotionKey = motionKey;

    // eased: slow out of the old scene, slow into the new one
    const e = p * p * p * (p * (p * 6 - 15) + 10);

    gl.bindVertexArray(vao);
    gl.disable(gl.BLEND);

    // 1. the scene, crossed in linear light
    gl.bindFramebuffer(gl.FRAMEBUFFER, sceneTarget.fbo);
    gl.viewport(0, 0, sceneTarget.width, sceneTarget.height);
    gl.useProgram(scene.program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, current.ready ? current.tex : outgoing.tex);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, outgoing.tex);
    gl.uniform1i(scene.uniform('u_a'), 0);
    gl.uniform1i(scene.uniform('u_b'), 1);
    gl.uniform1f(scene.uniform('u_p'), current.ready ? e : 1);
    gl.uniform1f(scene.uniform('u_time'), now / 1000);
    gl.uniform1f(scene.uniform('u_still'), options.reducedMotion ? 1 : 0);
    gl.uniformMatrix3fv(scene.uniform('u_hue'), true, hueMatrix(shift));
    gl.uniform1f(scene.uniform('u_lift'), lift);
    gl.uniform1f(scene.uniform('u_encode'), float ? 0 : 1);
    gl.drawArrays(gl.TRIANGLES, 0, 3);

    // 2. bloom: bright parts, quarter size, blurred twice
    gl.bindFramebuffer(gl.FRAMEBUFFER, bloomA.fbo);
    gl.viewport(0, 0, bloomA.width, bloomA.height);
    gl.useProgram(bright.program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, sceneTarget.tex);
    gl.uniform1i(bright.uniform('u_src'), 0);
    gl.uniform2f(bright.uniform('u_texel'), 1 / sceneTarget.width, 1 / sceneTarget.height);
    gl.uniform1f(bright.uniform('u_decode'), float ? 0 : 1);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.useProgram(blur.program);
    gl.uniform1i(blur.uniform('u_src'), 0);
    for (const spread of [1, 2.2]) {
      gl.bindFramebuffer(gl.FRAMEBUFFER, bloomB.fbo);
      gl.bindTexture(gl.TEXTURE_2D, bloomA.tex);
      gl.uniform2f(blur.uniform('u_dir'), spread / bloomA.width, 0);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      gl.bindFramebuffer(gl.FRAMEBUFFER, bloomA.fbo);
      gl.bindTexture(gl.TEXTURE_2D, bloomB.tex);
      gl.uniform2f(blur.uniform('u_dir'), 0, spread / bloomA.height);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    }

    // 3. through the glass, to the screen
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, width, height);
    gl.useProgram(final.program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, sceneTarget.tex);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, bloomA.tex);
    gl.uniform1i(final.uniform('u_scene'), 0);
    gl.uniform1i(final.uniform('u_bloom'), 1);
    gl.uniform1f(final.uniform('u_decode'), float ? 0 : 1);
    gl.uniform1f(final.uniform('u_bloomGain'), 0.55);
    const tx = m ? m.rx / TILT_MAX : 0;
    const ty = m ? m.ry / TILT_MAX : 0;
    gl.uniform2f(final.uniform('u_glint'), -ty * 0.05, tx * 0.05);
    gl.uniform1f(final.uniform('u_shield'), m ? Math.min(1, m.shield) : 0);
    const accent = readAccent();
    gl.uniform3f(final.uniform('u_shieldColor'), accent[0], accent[1], accent[2]);
    gl.uniform1f(final.uniform('u_frame'), frame % 97);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    frame += 1;
  };

  let raf = 0;
  let stopped = false;
  const loop = (now: number) => {
    raf = 0;
    if (stopped) return;
    render(now);
    raf = requestAnimationFrame(loop);
  };

  resize();
  const observer = new ResizeObserver(() => resize());
  observer.observe(canvas);
  const onLost = (event: Event) => event.preventDefault();
  canvas.addEventListener('webglcontextlost', onLost);
  canvas.dataset.renderer = 'webgl2';
  canvas.dataset.transition = 'idle';
  raf = requestAnimationFrame(loop);

  return {
    stop() {
      stopped = true;
      if (raf) cancelAnimationFrame(raf);
      observer.disconnect();
      canvas.removeEventListener('webglcontextlost', onLost);
      freeTarget(gl, sceneTarget);
      freeTarget(gl, bloomA);
      freeTarget(gl, bloomB);
      gl.deleteTexture(current.tex);
      gl.deleteTexture(outgoing.tex);
      gl.deleteVertexArray(vao);
      for (const p of [scene, bright, blur, final]) gl.deleteProgram(p.program);
    },
  };
}
