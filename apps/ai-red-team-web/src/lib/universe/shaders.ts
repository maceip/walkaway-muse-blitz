import { NOISE_GLSL } from '../gl';
import { GROUP_COUNT } from './sky';

/**
 * The sky's shaders.
 *
 * Coordinates: everything is worked in canvas pixels with the origin at the
 * top left and y running down, the way the page lays itself out. "World"
 * positions are where a thing sits before the slow camera drift; the screen
 * position is the world position plus the camera offset times the thing's
 * parallax, so nearer things slide further than far ones.
 */

const N = GROUP_COUNT;

/** Points and orbit lines: stars, galaxy stars, worlds. */
export const POINTS_VS = /* glsl */ `#version 300 es
precision highp float;
layout(location = 0) in vec4 a_a;
layout(location = 1) in vec4 a_b;
layout(location = 2) in vec4 a_c;
layout(location = 3) in vec4 a_e;
uniform vec2 u_res;
uniform float u_dpr;
uniform float u_time;
uniform vec2 u_camera;
uniform vec2 u_drift;
uniform vec2 u_margin;
uniform vec4 u_g0[${N}]; // cx, cy, scale, position angle
uniform vec4 u_g1[${N}]; // cos incl, sin incl, parallax, kind (0 drifting layer, 1 system)
out vec3 v_color;
out float v_bright;
out float v_core;
out float v_halo;
out float v_spike;
void main() {
  int g = int(a_c.w + 0.5);
  vec4 g0 = u_g0[g];
  vec4 g1 = u_g1[g];
  vec2 p;
  if (g1.w < 0.5) {
    // a drifting star layer: it slides steadily and wraps round the frame
    vec2 span = u_res + 2.0 * u_margin;
    vec2 moved = a_a.xy * span + u_drift * (a_a.z * u_dpr * u_time) + u_camera * g1.z;
    p = mod(moved, span) - u_margin;
  } else {
    // a system: the point goes round the centre in the system's own plane,
    // and the plane is tilted and turned to face the frame
    float ang = a_a.y + a_a.w * u_time;
    vec3 l = vec3(cos(ang) * a_a.x, sin(ang) * a_a.x, a_a.z);
    vec2 s = vec2(l.x, l.y * g1.x + l.z * g1.y);
    float c = cos(g0.w);
    float sn = sin(g0.w);
    s = vec2(c * s.x - sn * s.y, sn * s.x + c * s.y);
    p = g0.xy + s * g0.z + u_camera * g1.z;
  }
  vec2 clip = (p / u_res) * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);

  float twinkle = 1.0 + a_e.x * sin(u_time * a_e.y + a_b.w);
  float core = a_b.x * u_dpr;
  float bright = a_b.y * twinkle;
  // a star smaller than a pixel and a half is drawn at a pixel and a half
  // and dimmed to match, so it moves smoothly instead of crawling
  float minCore = 1.5;
  if (core < minCore) {
    bright *= (core * core) / (minCore * minCore);
    core = minCore;
  }
  float sprite = core * (1.0 + a_e.z * 6.0 + a_e.w * 9.0) + 2.0;
  gl_PointSize = min(sprite, 96.0);
  v_core = core / gl_PointSize;
  v_halo = a_e.z;
  v_spike = a_e.w;
  v_bright = bright;
  v_color = a_c.rgb;
}
`;

export const POINTS_FS = /* glsl */ `#version 300 es
precision highp float;
in vec3 v_color;
in float v_bright;
in float v_core;
in float v_halo;
in float v_spike;
uniform int u_lines;
out vec4 o;
void main() {
  if (u_lines == 1) {
    o = vec4(v_color * v_bright, 1.0);
    return;
  }
  vec2 pc = gl_PointCoord * 2.0 - 1.0;
  float d = length(pc);
  if (d > 1.0) discard;
  float k = d / max(v_core, 0.001);
  float core = exp(-k * k * 2.2);
  float halo = v_halo * exp(-d * 5.5) * (1.0 - d);
  float spike = 0.0;
  if (v_spike > 0.0) {
    float edge = (1.0 - d) * (1.0 - d);
    spike = v_spike * (exp(-abs(pc.y) * 48.0) + exp(-abs(pc.x) * 48.0)) * edge;
  }
  o = vec4(v_color * (core + halo + spike) * v_bright, 1.0);
}
`;

/**
 * The diffuse sky: gas, the galactic band and its dust, and the unresolved
 * light of the galaxies. Rendered small into a texture that covers the world
 * plus a margin, and only every few frames, because it is soft and it moves
 * very slowly; the composite pass slides it with the camera.
 */
export const NEBULA_FS = /* glsl */ `#version 300 es
precision highp float;
uniform vec2 u_fbo;
uniform vec2 u_span;
uniform vec2 u_margin;
uniform float u_minDim;
uniform float u_time;
uniform vec3 u_accent;
uniform vec4 u_gal0[3]; // cx, cy, R, pa
uniform vec4 u_gal1[3]; // cos incl, pattern angle, arms, pitch
uniform vec4 u_gal2[3]; // bar, edge-on, brightness, -
uniform vec4 u_neb0[3]; // cx, cy, R, seed
uniform vec4 u_neb1[3]; // c1 rgb, strength
uniform vec4 u_neb2[3]; // c2 rgb, use accent
uniform vec4 u_band;    // x, y, angle, half-width
out vec4 o;
${NOISE_GLSL}

vec3 galaxy(int i, vec2 w) {
  vec4 a = u_gal0[i];
  vec4 b = u_gal1[i];
  vec4 c = u_gal2[i];
  vec2 d = w - a.xy;
  float cp = cos(-a.w);
  float sp = sin(-a.w);
  vec2 l = vec2(cp * d.x - sp * d.y, sp * d.x + cp * d.y);
  float yRaw = l.y;
  l.y /= max(b.x, 0.1);
  float r = length(l) / a.z;
  if (r > 1.6) return vec3(0.0);
  float theta = atan(l.y, l.x);
  float disk = exp(-r * 3.4);
  float bulge = exp(-(r * r) / (0.13 * 0.13));
  float lr = log(max(r, 0.02));
  float phase = theta - lr / b.w - b.y;
  float arms = pow(0.5 + 0.5 * cos(b.z * phase), 3.0);
  arms *= 0.65 + 0.7 * fbm(l / a.z * 5.0 + float(i) * 13.0, 4);
  arms *= smoothstep(0.03, 0.18, r) * smoothstep(1.15, 0.5, r);
  if (c.x > 0.0) {
    float bar = exp(-pow(l.x / (a.z * c.x), 4.0)) * exp(-pow(l.y / (a.z * 0.05), 2.0));
    bulge = max(bulge, bar * 0.7);
  }
  vec3 col = vec3(1.0, 0.8, 0.58) * bulge * 0.9
           + vec3(0.72, 0.78, 0.92) * disk * 0.22
           + vec3(0.5, 0.66, 1.0) * arms * disk * 0.55;
  if (c.y > 0.5) {
    // edge-on: a lane of dust down the middle of the disk
    float lane = exp(-pow(yRaw / (a.z * 0.022), 2.0)) * smoothstep(0.9, 0.15, r);
    col *= 1.0 - 0.8 * lane;
  } else {
    // face-on dust: dark filaments along the inner edge of the arms
    float dust = pow(0.5 + 0.5 * cos(b.z * (phase + 0.45)), 6.0) * smoothstep(0.05, 0.3, r);
    col *= 1.0 - 0.45 * dust * smoothstep(-0.1, 0.3, fbm(l / a.z * 9.0, 3));
  }
  return col * c.z;
}

void main() {
  vec2 frag = gl_FragCoord.xy / u_fbo;
  vec2 w = vec2(frag.x, 1.0 - frag.y) * u_span - u_margin;
  vec2 q = w / u_minDim;
  float t = u_time;
  vec3 col = vec3(0.0);

  // the galactic band, with the dust that eats into it
  vec2 bp = vec2(u_band.x, u_band.y);
  vec2 dir = vec2(cos(u_band.z), sin(u_band.z));
  vec2 nrm = vec2(-dir.y, dir.x);
  float along = dot(w - bp, dir) / u_minDim;
  float across = dot(w - bp, nrm) / (u_band.w * u_minDim);
  float wobble = fbm(vec2(along * 1.4, 3.7), 3) * 0.9;
  float bandShape = exp(-pow(across + wobble, 2.0) * 1.4);
  float clouds = 0.55 + 0.9 * fbm(vec2(along * 3.2, (across + wobble) * 1.6) + t * 0.0015, 5);
  float dust = smoothstep(-0.05, 0.35, fbm(vec2(along * 4.5, (across + wobble) * 3.0) + 40.0, 5))
             * exp(-pow((across + wobble) * 1.8, 2.0));
  vec3 band = vec3(0.62, 0.57, 0.52) * bandShape * max(clouds, 0.0) * 0.07
            + vec3(0.3, 0.38, 0.6) * bandShape * 0.018;
  band *= 1.0 - 0.85 * dust;
  col += band;

  // emission nebulae: soft gas threaded with brighter filaments
  for (int i = 0; i < 3; i++) {
    vec4 a = u_neb0[i];
    vec4 c1 = u_neb1[i];
    vec4 c2 = u_neb2[i];
    vec2 p = (w - a.xy) / a.z;
    float s = a.w;
    vec2 warp = vec2(fbm(p * 1.3 + s + t * 0.002, 4), fbm(p * 1.3 + s + 7.3 - t * 0.0017, 4));
    float d = length(p + warp * 0.5);
    float fall = exp(-d * d * 2.6);
    if (fall < 0.002) continue;
    float gas = smoothstep(-0.15, 0.55, fbm(p * 2.2 + warp * 1.6 + s * 3.0, 6));
    float fil = pow(1.0 - abs(gnoise(p * 3.4 + warp * 2.2 + s) * 2.0), 6.0);
    vec3 hue2 = c2.a > 0.5 ? mix(c2.rgb, u_accent, 0.55) : c2.rgb;
    vec3 tint = mix(c1.rgb, hue2, smoothstep(-0.25, 0.35, gnoise(p * 1.7 + s * 5.0)));
    col += tint * (gas * 0.65 + fil * 0.5) * fall * c1.a;
  }

  // the galaxies' unresolved light
  for (int i = 0; i < 3; i++) col += galaxy(i, w);

  o = vec4(col, 1.0);
}
`;

/** The composite: black sky, the diffuse light slid by the camera, dither. */
export const COMPOSITE_FS = /* glsl */ `#version 300 es
precision highp float;
uniform sampler2D u_neb;
uniform vec2 u_res;
uniform vec2 u_span;
uniform vec2 u_margin;
uniform vec2 u_camera;
uniform float u_parallax;
uniform float u_frame;
out vec4 o;
${NOISE_GLSL}
void main() {
  vec2 px = vec2(gl_FragCoord.x, u_res.y - gl_FragCoord.y);
  vec2 w = px - u_camera * u_parallax;
  vec2 uv = vec2((w.x + u_margin.x) / u_span.x, 1.0 - (w.y + u_margin.y) / u_span.y);
  vec3 col = vec3(0.003, 0.005, 0.011);
  col += vec3(0.0, 0.003, 0.009) * (1.0 - px.y / u_res.y);
  col += texture(u_neb, uv).rgb;
  // the diffuse light is dark and smooth: dither it so eight bits do not band
  col += (hash12(gl_FragCoord.xy + vec2(u_frame * 7.0, u_frame * 3.0)) - 0.5) / 255.0;
  o = vec4(col, 1.0);
}
`;

/** One planet: a lit sphere, banded or cratered, with a ring if it has one. */
export const PLANET_FS = /* glsl */ `#version 300 es
precision highp float;
uniform vec2 u_res;
uniform vec4 u_p0; // cx, cy, radius px, kind (0 rocky, 1 giant)
uniform vec4 u_p1; // spin phase, ring tilt, ring pa, seed
uniform vec3 u_c1;
uniform vec3 u_c2;
uniform vec3 u_c3;
out vec4 o;
${NOISE_GLSL}

vec4 ringAt(vec2 q, out float front) {
  front = 0.0;
  if (u_p0.w < 0.5 || u_p1.y <= 0.0) return vec4(0.0);
  float c = cos(-u_p1.z);
  float s = sin(-u_p1.z);
  vec2 r = vec2(c * q.x - s * q.y, s * q.x + c * q.y);
  float e = length(vec2(r.x, r.y / u_p1.y));
  if (e < 1.32 || e > 2.3) return vec4(0.0);
  front = r.y > 0.0 ? 1.0 : 0.0;
  float bands = 0.55 + 0.45 * sin(e * 46.0 + gnoise(vec2(e * 30.0, 1.0)) * 2.0);
  float gap = smoothstep(0.015, 0.05, abs(e - 1.86));
  float edge = smoothstep(1.32, 1.4, e) * smoothstep(2.3, 2.18, e);
  float a = 0.55 * bands * gap * edge;
  vec3 col = mix(u_c3, u_c1, 0.4) * 0.8;
  return vec4(col, a);
}

void main() {
  vec2 px = vec2(gl_FragCoord.x, u_res.y - gl_FragCoord.y);
  vec2 q = (px - u_p0.xy) / u_p0.z;
  float rr = dot(q, q);
  float front;
  vec4 ring = ringAt(q, front);
  vec4 outc = vec4(0.0);
  // the half of the ring behind the planet goes down first
  if (front < 0.5) outc = ring;

  if (rr < 1.0) {
    vec3 n = vec3(q.x, -q.y, sqrt(1.0 - rr));
    vec3 L = normalize(vec3(-0.6, 0.42, 0.68));
    float lat = asin(n.y);
    float lon = atan(n.x, n.z) + u_p1.x;
    vec3 col;
    if (u_p0.w > 0.5) {
      float turb = fbm(vec2(lon * 1.6, lat * 9.0) + u_p1.w, 4);
      float b = sin(lat * 13.0 + turb * 2.2);
      col = mix(u_c2, u_c1, smoothstep(-0.6, 0.6, b));
      col = mix(col, u_c3, smoothstep(0.55, 0.95, sin(lat * 5.0 + turb)) * 0.6);
    } else {
      float h = fbm(vec2(lon * 2.2, lat * 2.2) + u_p1.w, 5);
      float craters = pow(1.0 - abs(gnoise(vec2(lon * 6.0, lat * 6.0) + u_p1.w * 3.0)) , 8.0);
      col = mix(u_c2, u_c1, smoothstep(-0.3, 0.35, h));
      col = mix(col, u_c3, craters * 0.5);
    }
    float ndl = dot(n, L);
    float lit = smoothstep(-0.12, 0.5, ndl);
    float rim = pow(1.0 - n.z, 3.0) * smoothstep(-0.2, 0.4, ndl);
    vec3 shaded = col * (0.015 + 0.95 * lit) + mix(u_c3, vec3(0.6, 0.75, 1.0), 0.5) * rim * 0.35;
    float aa = smoothstep(1.0, 1.0 - 1.5 / u_p0.z, sqrt(rr));
    outc = vec4(mix(outc.rgb, shaded, aa), max(outc.a, aa));
  }

  if (front > 0.5) outc = vec4(mix(outc.rgb, ring.rgb, ring.a), max(outc.a, ring.a));
  if (outc.a <= 0.0) discard;
  o = outc;
}
`;

/** A comet, now and then: a coma and a thin tail, crossing slowly. */
export const COMET_FS = /* glsl */ `#version 300 es
precision highp float;
uniform vec2 u_res;
uniform vec2 u_head;
uniform vec2 u_dir;
uniform float u_len;
uniform float u_fade;
uniform float u_dpr;
out vec4 o;
void main() {
  vec2 px = vec2(gl_FragCoord.x, u_res.y - gl_FragCoord.y);
  vec2 v = px - u_head;
  float along = dot(v, -u_dir);
  float perp = dot(v, vec2(-u_dir.y, u_dir.x));
  float head = exp(-dot(v, v) / (2.2 * u_dpr * u_dpr)) * 1.1 + exp(-length(v) / (5.0 * u_dpr)) * 0.18;
  float tail = 0.0;
  if (along > 0.0) {
    float width = (1.2 + along * 0.06) * u_dpr;
    tail = exp(-along / u_len * 2.6) * exp(-perp * perp / (width * width)) * 0.32;
  }
  vec3 col = vec3(0.85, 0.95, 1.0) * head + vec3(0.55, 0.75, 1.0) * tail;
  o = vec4(col * u_fade, 1.0);
}
`;
