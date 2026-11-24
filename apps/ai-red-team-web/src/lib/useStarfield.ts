import { useEffect, useRef } from 'react';

/**
 * Deterministic pseudo random, so the sky is the same on every load.
 * Same LCG as the original page: it is the sky's fingerprint, and changing
 * it would change every screenshot that has ever been taken of this page.
 */
export function seeded(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

interface Layer {
  count: number;
  size: [number, number];
  speed: number;
  alpha: [number, number];
}

/**
 * The stars in front of the drawn sky.
 *
 * The sky itself is a drawn plate with forty thousand stars on it, so this
 * canvas is not the star field any more — it is the handful of near stars
 * that drift across it. They are the ones that move: the drawn field is
 * thousands of light years away and does not.
 */
const LAYERS: Layer[] = [
  { count: 44, size: [0.5, 1.1], speed: 2.2, alpha: [0.14, 0.34] },
  { count: 28, size: [0.8, 1.7], speed: 5.0, alpha: [0.26, 0.6] },
  { count: 13, size: [1.3, 2.6], speed: 9.5, alpha: [0.45, 0.95] },
];

interface Star {
  layer: number;
  x: number;
  y: number;
  r: number;
  a: number;
  tw: number;
  tws: number;
}

/**
 * The outspace the orb sits in: a slow forward drift through three depths
 * of star. Painted on a canvas because there are hundreds of them and the
 * DOM is the wrong tool for that.
 */
export function useStarfield(canvasRef: React.RefObject<HTMLCanvasElement | null>): void {
  const starsRef = useRef<Star[]>([]);
  const sizeRef = useRef({ w: 0, h: 0 });

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const reduced =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches;

    const buildStars = (w: number, h: number) => {
      const rand = seeded(20261122);
      const stars: Star[] = [];
      for (let l = 0; l < LAYERS.length; l += 1) {
        const cfg = LAYERS[l];
        for (let i = 0; i < cfg.count; i += 1) {
          stars.push({
            layer: l,
            x: rand() * w,
            y: rand() * h,
            r: cfg.size[0] + rand() * (cfg.size[1] - cfg.size[0]),
            a: cfg.alpha[0] + rand() * (cfg.alpha[1] - cfg.alpha[0]),
            tw: rand() * Math.PI * 2,
            tws: 0.4 + rand() * 1.5,
          });
        }
      }
      starsRef.current = stars;
    };

    const resize = () => {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const w = window.innerWidth;
      const h = window.innerHeight;
      canvas.width = Math.max(1, Math.floor(w * dpr));
      canvas.height = Math.max(1, Math.floor(h * dpr));
      canvas.style.width = `${w}px`;
      canvas.style.height = `${h}px`;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      sizeRef.current = { w, h };
      buildStars(w, h);
    };

    const draw = (time: number) => {
      const { w, h } = sizeRef.current;
      const t = time / 1000;
      ctx.clearRect(0, 0, w, h);

      const stars = starsRef.current;
      for (let i = 0; i < stars.length; i += 1) {
        const s = stars[i];
        const cfg = LAYERS[s.layer];

        // slow drift: the camera creeps forward and to the right
        s.x += cfg.speed * 0.22;
        s.y += cfg.speed;
        if (s.y - s.r > h) {
          s.y = -s.r;
          s.x = Math.random() * w;
        }
        if (s.x - s.r > w) {
          s.x = -s.r;
          s.y = Math.random() * h;
        }

        const twinkle = 0.68 + 0.32 * Math.sin(t * s.tws + s.tw);
        ctx.globalAlpha = s.a * twinkle;
        ctx.fillStyle = s.layer === 2 ? '#dbe8ff' : '#ffffff';
        ctx.beginPath();
        ctx.arc(s.x, s.y, s.r, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
    };

    resize();
    window.addEventListener('resize', resize);

    let raf = 0;
    if (reduced) {
      // one static frame is enough when motion is not wanted
      draw(0);
    } else {
      const frame = (now: number) => {
        draw(now);
        raf = window.requestAnimationFrame(frame);
      };
      raf = window.requestAnimationFrame(frame);
    }

    return () => {
      window.removeEventListener('resize', resize);
      window.cancelAnimationFrame(raf);
    };
  }, [canvasRef]);
}
