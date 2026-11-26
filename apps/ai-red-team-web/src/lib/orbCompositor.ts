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
 *  - **A crossing that takes its whole time.** The scenes are lit very
 *    differently — daylight, dusk, neon, blue, the red alert — and a crossing
 *    between two of them is a change of colour as much as of picture. The
 *    new light arrives through a broad, soft front that spends the whole
 *    TRANSITION_MS on the way: each point of the globe turns over well over a
 *    second, the middle a little before the rim. The two scenes are mixed in
 *    OKLab, where equal steps look equal, so a bright scene does not swamp a
 *    dark one and then give way all at once at the end. (The crossing used to
 *    mix in linear light through three stacked easings, which packed nearly
 *    all of the change into about 0.6 s in the middle.)
 *  - **The alert light belongs to its scene.** The red of the alert scene is
 *    graded into that scene here, so it crosses in and out with the city it
 *    lights, on the same front, instead of washing over the globe on a timer
 *    of its own. The drill's red light is graded here too.
 *  - **A focus pull, not a dissolve.** As one scene crosses into the next
 *    the lens drifts out of focus and back. Two different shots never line
 *    up exactly; a rack focus is how film hides that.
 *  - **Depth.** Every scene carries a depth map (scripts/build-redteam-depth.py:
 *    a picture for a still, packed beside the colour for a film). When the
 *    globe wobbles, near surfaces slide against far ones, so the towers move
 *    against the streets like a real object; and a crossing's focus pull is
 *    focused at a depth, travelling from the far city to the near one.
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
export const TRANSITION_MS = 3200;

const SCENE_FS = /* glsl */ `#version 300 es
precision highp float;
in vec2 v_uv;
uniform sampler2D u_a;
uniform sampler2D u_b;
uniform sampler2D u_aDepth;
uniform sampler2D u_bDepth;
uniform float u_aMode;    // 0: no depth, 1: a depth picture, 2: a packed film
uniform float u_bMode;
uniform float u_p;        // the crossing's progress, eased: drives the camera
uniform float u_t;        // the crossing's progress in time, 0 to 1, not eased
uniform float u_aAlert;   // how much alert light each scene is under, 0 to 1
uniform float u_bAlert;
uniform float u_time;
uniform float u_still;    // 1 with reduced motion: no zoom, no focus pull
uniform float u_matched;  // 1 going into a transition from the picture it opens on
uniform vec2 u_parallax;  // how far the nearest surface slides, from the wobble
uniform mat3 u_hue;
uniform float u_lift;
uniform float u_encode;   // 1 when the target is 8-bit and must stay encoded
out vec4 o;
${NOISE_GLSL}

// a packed film: the colour square is the left two thirds of the frame, the
// depth square the top right third
const float PACK = 2.0 / 3.0;

vec3 toLinear(vec3 c) { return pow(max(c, 0.0), vec3(2.2)); }

// OKLab (Björn Ottosson), from and to linear sRGB: a space where equal steps
// look like equal steps of lightness and colour
vec3 toOklab(vec3 c) {
  vec3 lms = vec3(
    dot(c, vec3(0.4122214708, 0.5363325363, 0.0514459929)),
    dot(c, vec3(0.2119034982, 0.6806995451, 0.1073969566)),
    dot(c, vec3(0.0883024619, 0.2817188376, 0.6299787005)));
  lms = pow(max(lms, 0.0), vec3(1.0 / 3.0));
  return vec3(
    dot(lms, vec3(0.2104542553, 0.7936177850, -0.0040720468)),
    dot(lms, vec3(1.9779984951, -2.4285922050, 0.4505937099)),
    dot(lms, vec3(0.0259040371, 0.7827717662, -0.8086757660)));
}
vec3 fromOklab(vec3 c) {
  vec3 lms = vec3(
    c.x + 0.3963377774 * c.y + 0.2158037573 * c.z,
    c.x - 0.1055613458 * c.y - 0.0638541728 * c.z,
    c.x - 0.0894841775 * c.y - 1.2914855480 * c.z);
  lms = lms * lms * lms;
  return vec3(
    dot(lms, vec3(4.0767416621, -3.3077115913, 0.2309699292)),
    dot(lms, vec3(-1.2684380046, 2.6097574011, -0.3413193965)),
    dot(lms, vec3(-0.0041960863, -0.7034186147, 1.7076147010)));
}

// The alert light: the page's alert red laid over the city with the CSS
// 'color' blend at 0.6 — the city keeps its own light and takes the red's
// hue — exactly what .orb__alert does to the page without WebGL.
const vec3 ALERT_RED = vec3(1.0, 0.1255, 0.1882);
float blendLum(vec3 c) { return dot(c, vec3(0.3, 0.59, 0.11)); }
vec3 clipColour(vec3 c) {
  float l = blendLum(c);
  float n = min(c.r, min(c.g, c.b));
  float x = max(c.r, max(c.g, c.b));
  if (n < 0.0) c = l + (c - l) * l / max(l - n, 1e-5);
  if (x > 1.0) c = l + (c - l) * (1.0 - l) / max(x - l, 1e-5);
  return c;
}
vec3 alertLight(vec3 lin, float amount) {
  if (amount < 0.001) return lin;
  vec3 s = pow(max(lin, 0.0), vec3(1.0 / 2.2));
  vec3 red = clipColour(ALERT_RED + (blendLum(s) - blendLum(ALERT_RED)));
  return toLinear(mix(s, red, 0.6 * amount));
}

vec3 grade(vec3 srgb) {
  // the page's slow colour drift, applied the way the stylesheet's
  // hue-rotate() and brightness() would apply it
  return clamp(u_hue * srgb, 0.0, 1.0) * u_lift;
}

vec2 colourUv(vec2 uv, float mode) {
  return mode > 1.5 ? vec2(clamp(uv.x, 0.0, 0.998) * PACK, uv.y) : uv;
}

float depthOf(sampler2D col, sampler2D dep, float mode, vec2 uv) {
  if (mode > 1.5) {
    vec2 q = clamp(uv, vec2(0.004), vec2(0.996));
    return texture(col, vec2(PACK + q.x / 3.0, q.y * 0.5)).r;
  }
  if (mode > 0.5) return texture(dep, uv).r;
  return 0.5;
}

// Where to look in the picture once the globe has tipped: nearer surfaces
// (bigger depth) slide further than far ones, so the towers move against the
// streets. The shift is a few pixels, so two refinement steps are plenty.
vec2 lookup(sampler2D col, sampler2D dep, float mode, vec2 uv, vec2 shift, out float d) {
  d = depthOf(col, dep, mode, uv);
  if (mode < 0.5) return uv;
  vec2 q = uv - shift * (d - 0.5);
  d = depthOf(col, dep, mode, q);
  return uv - shift * (d - 0.5);
}

vec3 tap(sampler2D t, float mode, vec2 uv) {
  return toLinear(grade(texture(t, colourUv(uv, mode)).rgb));
}

vec3 lensed(sampler2D t, float mode, vec2 uv, float blur) {
  vec3 c = tap(t, mode, uv);
  if (blur < 0.0002) return c;
  // a small disc of taps on a golden-angle spiral: the out-of-focus blur of
  // a lens, in linear light so highlights spread the way light does
  vec3 sum = c;
  float wsum = 1.0;
  for (int i = 1; i < 9; i++) {
    float fi = float(i);
    float r = sqrt(fi / 8.0) * blur;
    float a = fi * 2.39996;
    sum += tap(t, mode, uv + vec2(cos(a), sin(a)) * r);
    wsum += 1.0;
  }
  return sum / wsum;
}

// Depth of field across a crossing: the focus travels from the far city to
// the near one, and the further a surface is from the focus the softer it is.
// Without depth the whole picture softens evenly.
float defocus(float mode, float d, float focus) {
  return mode < 0.5 ? 1.0 : 0.45 + 1.6 * abs(d - focus);
}

void main() {
  vec2 uv = vec2(v_uv.x, 1.0 - v_uv.y);
  float p = clamp(u_p, 0.0, 1.0);
  float t = clamp(u_t, 0.0, 1.0);
  float bell = sin(3.14159265 * t);
  float motion = 1.0 - u_still;
  // a transition opens on the very picture the page is showing: there is
  // nothing to hide, so no focus pull and no settling of the camera
  float camera = motion * (1.0 - u_matched);

  // the arriving scene settles as it comes in, the outgoing one drifts on
  float sa = 1.0 + 0.03 * (1.0 - p) * camera;
  float sb = 1.0 + 0.03 * p * camera;
  vec2 ua = 0.5 + (uv - 0.5) / sa;
  vec2 ub = 0.5 + (uv - 0.5) / sb;
  float blur = 0.0042 * bell * camera;
  float focus = mix(0.3, 0.75, p);

  // the parallax fades out toward the glass, which is the shell and does
  // not move against the city inside it
  float shell = smoothstep(0.98, 0.62, length(uv - 0.5) * 2.0);
  vec2 shift = u_parallax * shell;

  float da;
  vec2 pa = lookup(u_a, u_aDepth, u_aMode, ua, shift, da);
  vec3 a = alertLight(lensed(u_a, u_aMode, pa, blur * defocus(u_aMode, da, focus)), u_aAlert);
  vec3 col = a;
  if (t < 0.999) {
    float db;
    vec2 pb = lookup(u_b, u_bDepth, u_bMode, ub, shift, db);
    vec3 b = alertLight(lensed(u_b, u_bMode, pb, blur * defocus(u_bMode, db, focus)), u_bAlert);
    // When this point of the globe turns over: a broad, slowly moving front,
    // the middle a little before the rim. It is low in frequency on purpose —
    // a fine pattern between two differently lit cities reads as blotches.
    float n = fbm(uv * 1.7 + vec2(u_time * 0.02, -u_time * 0.015), 3);
    float r = length(uv - 0.5) * 2.0;
    float order = clamp(0.5 + n * 1.2 + (r - 0.5) * 0.4, 0.0, 1.0);
    // the turns start spread over the first 40% of the crossing, and each
    // takes the remaining 60% (about 1.9 s), so the whole globe is changing
    // for the whole crossing and no moment carries much of it
    float spread = 0.4 * motion;
    float u = clamp((t - order * spread) / (1.0 - spread), 0.0, 1.0);
    // half a plain fade, half eased: each point eases in and out of its turn
    // without the steep middle a full smoothstep has
    float m = 0.5 * (u + u * u * (3.0 - 2.0 * u));
    col = max(fromOklab(mix(toOklab(b), toOklab(a), m)), 0.0);
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
  /** the scene's depth picture, when it has one */
  depthTex: WebGLTexture;
  /** which depth picture is in depthTex */
  depthKey: string;
  /** 0: no depth, 1: the depth picture, 2: depth packed in the film */
  mode: number;
  /** 1 when the scene is lit by the alert light */
  alert: number;
  /** the scenes this one is a transition from */
  after: string[];
}

/**
 * How far the nearest surface slides, as a share of the globe's width, per
 * degree the globe tips. At the 14-degree limit the towers move about two and
 * a half per cent of the globe against the streets.
 */
const PARALLAX_PER_DEG = 0.06 / TILT_MAX;

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
 * How long a crossing into a film waits for the film's first frame before it
 * starts on the film's poster instead.
 *
 * The poster is the film's first frame, but as a lossless picture, and the
 * film is a compressed one: a little softer. A crossing that started on the
 * poster and switched to the film when it arrived softened the city in one
 * frame, part way through. Waiting for the film — usually a fraction of a
 * second, while the scene being left is still on the globe — means the
 * crossing goes straight to the film.
 */
const FILM_WAIT_MS = 1500;

/**
 * Which element to draw a scene layer from: the film once it has a frame,
 * otherwise the picture once it has loaded (if `poster` allows it), otherwise
 * nothing yet.
 */
function sourceOf(layer: Element | null, poster = true): HTMLImageElement | HTMLVideoElement | null {
  if (!layer) return null;
  const video = layer.querySelector('video');
  if (video && video.readyState >= 2 && video.videoWidth > 0) return video;
  if (video && !poster) return null;
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

  const makeTexture = (): WebGLTexture => {
    const tex = gl.createTexture();
    if (!tex) throw new Error('createTexture failed');
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([128, 128, 128, 255]));
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return tex;
  };
  const makeSlot = (): Slot => ({
    tex: makeTexture(),
    sceneId: null,
    source: null,
    key: '',
    ready: false,
    depthTex: makeTexture(),
    depthKey: '',
    mode: 0,
    alert: 0,
    after: [],
  });

  // depth pictures, fetched once each and kept
  const depthPictures = new Map<string, HTMLImageElement>();
  const depthPicture = (url: string): HTMLImageElement | null => {
    let img = depthPictures.get(url);
    if (!img) {
      img = new Image();
      img.decoding = 'async';
      img.src = url;
      depthPictures.set(url, img);
    }
    return img.complete && img.naturalWidth > 0 ? img : null;
  };

  /**
   * Work out what depth a slot has: packed in the film it is drawing, its
   * depth picture, or none yet. Returns true if that changed.
   */
  const updateDepth = (slot: Slot, layer: Element | null): boolean => {
    if (!layer || !slot.ready) return false;
    let mode = 0;
    if (slot.source instanceof HTMLVideoElement && layer.getAttribute('data-packed') === '1') {
      mode = 2;
    } else {
      const url = layer.getAttribute('data-depth');
      const img = url ? depthPicture(url) : null;
      if (url && img) {
        if (slot.depthKey !== url) {
          try {
            gl.bindTexture(gl.TEXTURE_2D, slot.depthTex);
            gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
            slot.depthKey = url;
          } catch {
            slot.depthKey = '';
          }
        }
        if (slot.depthKey === url) mode = 1;
      }
    }
    if (mode === slot.mode) return false;
    slot.mode = mode;
    return true;
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
  /** when the scene now arriving first appeared on the page */
  let arrivedAt = 0;
  let shownScene = '';
  let lastShift = NaN;
  let lastLift = NaN;
  // The colour drift and the drill's red light as drawn. The drift jumps when
  // the clock changes phase (warning, drill, settle) and the drill's light
  // switches on and off; both are eased here so the city never snaps to a
  // new colour. The stylesheet does the same for the page without WebGL.
  let shownShift = NaN;
  let shownLift = NaN;
  let drill = 0;
  let lastDrill = NaN;
  let lastNow = 0;
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
      current.mode = 0;
      current.alert = currentLayer?.getAttribute('data-alert') === '1' ? 1 : 0;
      current.after = (currentLayer?.getAttribute('data-after') ?? '').split(' ').filter(Boolean);
      transitionStart = -1;
      arrivedAt = now;
      canvas.dataset.transition = 'pending';
    }

    let changed = false;
    // keep the outgoing film playing for as long as its element is still
    // on the page; after that it holds its last frame
    if (outgoing.ready && previousLayer?.getAttribute('data-scene-id') === outgoing.sceneId) {
      changed = upload(outgoing, sourceOf(previousLayer)) || changed;
    }
    const wasReady = current.ready;
    // with nothing on the globe yet the poster is better than nothing; with a
    // scene showing, a film is waited for (up to FILM_WAIT_MS)
    const posterOk = !outgoing.ready || now - arrivedAt > FILM_WAIT_MS;
    changed = upload(current, sourceOf(currentLayer, posterOk)) || changed;
    changed = updateDepth(current, currentLayer) || changed;
    if (previousLayer?.getAttribute('data-scene-id') === outgoing.sceneId) {
      changed = updateDepth(outgoing, previousLayer) || changed;
    }
    const depthLabel = !current.ready ? 'none' : current.mode === 2 ? 'film' : current.mode === 1 ? 'still' : 'none';
    if (canvas.dataset.depth !== depthLabel) canvas.dataset.depth = depthLabel;
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

    const dt = lastNow ? Math.min(0.1, Math.max(0, (now - lastNow) / 1000)) : 0;
    lastNow = now;
    const targetShift = readNumber('--shift', 0);
    const targetLift = readNumber('--lift', 1);
    const ease = options.reducedMotion ? 1 : 1 - Math.exp(-dt / 0.8);
    shownShift = Number.isFinite(shownShift) ? shownShift + (targetShift - shownShift) * ease : targetShift;
    shownLift = Number.isFinite(shownLift) ? shownLift + (targetLift - shownLift) * ease : targetLift;
    const shift = shownShift;
    const lift = shownLift;
    const drillOn = host.closest('.orb')?.getAttribute('data-state') === 'exercise';
    drill = Math.min(1, Math.max(0, drill + (drillOn ? dt : -dt) / 0.9));
    const drillLight = drill * drill * (3 - 2 * drill);
    const m = motion.current;
    const motionKey = m
      ? `${m.rx.toFixed(2)}|${m.ry.toFixed(2)}|${m.shield.toFixed(3)}`
      : '';
    const graded =
      Math.abs(shift - lastShift) > 0.4 ||
      Math.abs(lift - lastLift) > 0.004 ||
      Math.abs(drillLight - lastDrill) > 0.002;
    if (!(dirty || changed || crossing || graded || motionKey !== lastMotionKey)) return;
    dirty = false;
    lastShift = shift;
    lastLift = lift;
    lastDrill = drillLight;
    lastMotionKey = motionKey;

    // eased, for the camera: the arriving scene settles slowly into place
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
    const a = current.ready ? current : outgoing;
    gl.activeTexture(gl.TEXTURE2);
    gl.bindTexture(gl.TEXTURE_2D, a.depthTex);
    gl.activeTexture(gl.TEXTURE3);
    gl.bindTexture(gl.TEXTURE_2D, outgoing.depthTex);
    gl.uniform1i(scene.uniform('u_a'), 0);
    gl.uniform1i(scene.uniform('u_b'), 1);
    gl.uniform1i(scene.uniform('u_aDepth'), 2);
    gl.uniform1i(scene.uniform('u_bDepth'), 3);
    gl.uniform1f(scene.uniform('u_aMode'), a.mode);
    gl.uniform1f(scene.uniform('u_bMode'), outgoing.mode);
    // the globe tipping: right side away (ry > 0) brings the near surfaces
    // to the right; top away (rx > 0) brings them up
    const reach = options.reducedMotion ? 0.5 : 1;
    const sx = m ? m.ry * PARALLAX_PER_DEG * reach : 0;
    const sy = m ? -m.rx * PARALLAX_PER_DEG * reach : 0;
    gl.uniform2f(scene.uniform('u_parallax'), sx, sy);
    const parallaxLabel = `${sx.toFixed(4)},${sy.toFixed(4)}`;
    if (canvas.dataset.parallax !== parallaxLabel) canvas.dataset.parallax = parallaxLabel;
    gl.uniform1f(scene.uniform('u_p'), current.ready ? e : 1);
    gl.uniform1f(scene.uniform('u_t'), current.ready ? p : 1);
    gl.uniform1f(scene.uniform('u_aAlert'), Math.max(a.alert, drillLight));
    gl.uniform1f(scene.uniform('u_bAlert'), Math.max(outgoing.alert, drillLight));
    gl.uniform1f(scene.uniform('u_time'), now / 1000);
    gl.uniform1f(scene.uniform('u_still'), options.reducedMotion ? 1 : 0);
    const matched = current.ready && outgoing.sceneId !== null && current.after.includes(outgoing.sceneId);
    gl.uniform1f(scene.uniform('u_matched'), matched ? 1 : 0);
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
      for (const slot of [current, outgoing]) {
        gl.deleteTexture(slot.tex);
        gl.deleteTexture(slot.depthTex);
      }
      gl.deleteVertexArray(vao);
      for (const p of [scene, bright, blur, final]) gl.deleteProgram(p.program);
    },
  };
}
