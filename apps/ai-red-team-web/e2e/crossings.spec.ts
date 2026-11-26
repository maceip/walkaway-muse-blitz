import { expect, test, type Page } from '@playwright/test';
import { SCENES, SCENE_BY_ID, type Scene } from '../src/media/scenes';
import { driftPool, nextScene } from '../src/lib/sequence';
import { TRANSITION_MS } from '../src/lib/orbCompositor';

/**
 * The crossings between scenes, measured.
 *
 * The page runs on Playwright's clock, so a crossing can be stepped through
 * a tenth of a second at a time and photographed at every step. The films
 * are held still for the measurement, so what changes from one step to the
 * next is the crossing and nothing else.
 *
 * A crossing is smooth when the change is spread over its time instead of
 * bunched into a moment: from 10% to 90% of the way it takes well over a
 * second and a half, and no tenth of a second carries more than 9% of it
 * (they measure about 1.7 s and 6–7%). The crossing this replaced packed
 * most of the change into 0.6–1.0 s, with steps of 13–18% of the whole.
 */

const STEP_MS = 100;

/** What the page would do next from `from`, and the random draw that gets `to`. */
function drawFor(from: Scene, to: Scene): number | null {
  const forced = nextScene(from, SCENES, () => 0);
  if (from.then || SCENES.some((s) => s.after.includes(from.id))) {
    // the page has no choice here, or chooses among transitions
    const options = from.then ? [forced] : SCENES.filter((s) => s.after.includes(from.id));
    const i = options.findIndex((s) => s.id === to.id);
    return i < 0 ? null : (i + 0.5) / options.length;
  }
  const pool = driftPool(from, SCENES);
  const i = pool.findIndex((s) => s.id === to.id);
  return i < 0 ? null : (i + 0.5) / pool.length;
}

/** The shortest run of scenes the page can take from `from` to `to`. */
function route(from: string, to: string): string[] {
  const prev = new Map<string, string>([[from, '']]);
  const queue = [from];
  while (queue.length) {
    const id = queue.shift()!;
    if (id === to) break;
    for (const next of SCENES) {
      if (prev.has(next.id) || drawFor(SCENE_BY_ID[id], next) === null) continue;
      prev.set(next.id, id);
      queue.push(next.id);
    }
  }
  if (!prev.has(to)) throw new Error(`no way from ${from} to ${to}`);
  const path = [to];
  while (path[0] !== from) path.unshift(prev.get(path[0])!);
  return path;
}

async function currentScene(page: Page): Promise<string> {
  return page.evaluate(() => document.querySelector('[data-role="current"]')!.getAttribute('data-scene-id')!);
}

async function lensState(page: Page): Promise<string> {
  return page.evaluate(() => document.querySelector<HTMLElement>('.orb__lens')!.dataset.transition ?? '');
}

/** Step the clock until the lens says the crossing has started. */
async function untilCrossing(page: Page) {
  for (let i = 0; i < 400; i += 1) {
    if ((await lensState(page)) === 'crossing') return;
    await page.clock.runFor(16);
    await page.waitForTimeout(5);
  }
  throw new Error('the crossing never started');
}

/** Move the page on to `to`, the way it really moves on. */
async function advanceTo(page: Page, to: string) {
  const from = await currentScene(page);
  const draw = drawFor(SCENE_BY_ID[from], SCENE_BY_ID[to]);
  if (draw === null) throw new Error(`${from} cannot go to ${to}`);
  await page.evaluate((r) => {
    const w = window as unknown as { __random?: () => number };
    w.__random ??= Math.random;
    Math.random = () => r;
  }, draw);
  // a film plays to its end and says so; a still waits out its time, which
  // is stepped through rather than jumped over: the page's slow colour drift
  // runs on the same clock, and a jump of a minute would jump the drift too
  await page.evaluate(() => {
    const v = document.querySelector<HTMLVideoElement>('[data-role="current"] video');
    if (v && !v.loop && v.duration) v.currentTime = Math.max(0, v.duration - 0.06);
  });
  for (let i = 0; i < 120 && (await currentScene(page)) !== to; i += 1) {
    await page.clock.runFor(200);
    await page.waitForTimeout(5);
  }
  await expect.poll(() => currentScene(page), { timeout: 20_000 }).toBe(to);
  await page.evaluate(() => {
    const w = window as unknown as { __random?: () => number };
    if (w.__random) Math.random = w.__random;
  });
}

async function openPage(page: Page) {
  await page.clock.install({ time: new Date('2026-11-26T12:00:00Z') });
  // far from any drill, so the clock's phases stay out of the measurement
  await page.goto('/?t=-1000000');
  // the float and the alert's laser sweep run on the stylesheet's own clock,
  // not the page's, and neither is part of a crossing
  await page.addStyleTag({ content: '.orb{animation:none!important}.orb__scan{display:none!important}' });
  await expect(page.locator('.orb.is-lens')).toBeVisible();
  await expect(page.locator('.orb__lens')).toHaveAttribute('data-scene', /.+/, { timeout: 15_000 });
  await page.clock.pauseAt(new Date('2026-11-26T12:00:05Z'));
}

async function walkTo(page: Page, target: string) {
  const path = route(await currentScene(page), target);
  for (const id of path.slice(1)) {
    await advanceTo(page, id);
    await untilCrossing(page);
    await page.clock.runFor(TRANSITION_MS + 400);
  }
}

interface Crossing {
  /** how far each step is from the start, as a share of the whole change */
  progress: number[];
  /** the mean difference between the first and last pictures, in levels */
  total: number;
  /** the largest difference from the first picture at any step, in levels */
  peak: number;
}

/** Photograph a crossing step by step, with the films held still. */
async function measureCrossing(page: Page): Promise<Crossing> {
  await untilCrossing(page);
  await page.evaluate(() => document.querySelectorAll('video').forEach((v) => v.pause()));
  const box = (await page.locator('.orb').boundingBox())!;
  const clip = { x: box.x, y: box.y, width: box.width, height: box.height };
  const shots: string[] = [];
  const steps = Math.ceil((TRANSITION_MS + 400) / STEP_MS);
  for (let i = 0; i <= steps; i += 1) {
    shots.push((await page.screenshot({ clip })).toString('base64'));
    await page.clock.runFor(STEP_MS);
  }
  return page.evaluate(async (pngs) => {
    const pixels = async (b64: string) => {
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await img.decode();
      const c = document.createElement('canvas');
      c.width = img.width;
      c.height = img.height;
      const ctx = c.getContext('2d')!;
      ctx.drawImage(img, 0, 0);
      return ctx.getImageData(0, 0, c.width, c.height);
    };
    const frames = await Promise.all(pngs.map(pixels));
    const { width: w, height: h } = frames[0];
    const inside: number[] = [];
    for (let y = 0; y < h; y += 2)
      for (let x = 0; x < w; x += 2)
        if ((x - w / 2) ** 2 + (y - h / 2) ** 2 < (0.42 * w) ** 2) inside.push((y * w + x) * 4);
    const diff = (a: ImageData, b: ImageData) => {
      let sum = 0;
      for (const i of inside)
        sum += Math.abs(a.data[i] - b.data[i]) + Math.abs(a.data[i + 1] - b.data[i + 1]) + Math.abs(a.data[i + 2] - b.data[i + 2]);
      return sum / inside.length / 3;
    };
    const fromStart = frames.map((f) => diff(f, frames[0]));
    const total = fromStart[fromStart.length - 1];
    return {
      progress: fromStart.map((d) => d / Math.max(total, 1e-6)),
      total,
      peak: Math.max(...fromStart),
    };
  }, shots);
}

function expectSmooth(c: Crossing) {
  const k10 = c.progress.findIndex((p) => p >= 0.1);
  const k90 = c.progress.findIndex((p) => p >= 0.9);
  const spread = (k90 - k10) * STEP_MS;
  let largest = 0;
  for (let i = 1; i < c.progress.length; i += 1) largest = Math.max(largest, c.progress[i] - c.progress[i - 1]);
  expect(c.total, 'the two scenes differ enough to measure').toBeGreaterThan(15);
  expect(spread, '10% to 90% of the change, in ms').toBeGreaterThanOrEqual(1500);
  expect(largest, 'the largest share of the change in one tenth of a second').toBeLessThanOrEqual(0.09);
}

test.describe('the order of the scenes', () => {
  test('a transition is entered from the scene it opens on, always, and goes on to the one it ends on', () => {
    const transitions = SCENES.filter((s) => s.after.length);
    expect(transitions.length).toBeGreaterThan(0);
    for (const t of transitions) {
      for (const from of t.after) {
        // whatever the draw, the scene a transition opens on goes into it
        for (const r of [0, 0.3, 0.7, 0.999]) {
          expect(SCENES.filter((s) => s.after.includes(from)).map((s) => s.id)).toContain(
            nextScene(SCENE_BY_ID[from], SCENES, () => r).id,
          );
        }
      }
      expect(t.then).not.toBeNull();
      expect(nextScene(t, SCENES, () => 0.5).id).toBe(t.then);
    }
    // and no other scene ever drifts into one
    let seed = 7;
    const random = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
    for (const s of SCENES) {
      if (s.then || SCENES.some((t) => t.after.includes(s.id))) continue;
      for (let i = 0; i < 200; i += 1) {
        const next = nextScene(s, SCENES, random);
        expect(next.after, `${s.id} -> ${next.id}`).toEqual([]);
        expect(next.id).not.toBe(s.id);
      }
    }
  });
});

test.describe('crossings', () => {
  test.describe.configure({ timeout: 120_000 });

  // the hardest changes of colour: red into daylight, a still into the red
  // alert light, the alert light out into a blue film, and film to film
  const pairs: [string, string][] = [
    ['city-redteam', 'globe-dusk'],
    ['globe-dusk', 'city-alert'],
    ['city-alert', 'city-scan'],
    ['city-scan', 'city-redteam'],
  ];
  for (const [from, to] of pairs) {
    test(`${from} into ${to} is spread over the crossing`, async ({ page }) => {
      await openPage(page);
      await walkTo(page, from);
      await page.clock.runFor(600);
      await advanceTo(page, to);
      expectSmooth(await measureCrossing(page));
    });
  }

  test('going into a transition does not change the picture', async ({ page }) => {
    const transition = SCENES.find((s) => s.after.includes('globe-day'));
    test.skip(!transition, 'no transition opens on the daylight globe');
    await openPage(page);
    await walkTo(page, 'globe-day');
    await page.clock.runFor(600);
    await advanceTo(page, transition!.id);
    const c = await measureCrossing(page);
    // The film opens on the picture the page is already showing. What is left
    // to cross is the film being a little softer than the still, and it is
    // crossed as gently as anything else; there is no focus pull, which would
    // carry the picture further from where it started than where it ends.
    let largest = 0;
    for (let i = 1; i < c.progress.length; i += 1) largest = Math.max(largest, c.progress[i] - c.progress[i - 1]);
    expect(c.total).toBeLessThan(30);
    expect(c.peak).toBeLessThanOrEqual(c.total * 1.05 + 0.5);
    expect(largest).toBeLessThanOrEqual(0.09);
  });
});
