import { expect, test, type Page } from '@playwright/test';

/**
 * The page under a real hand: the live sky, the orb's wobble and shield,
 * and the crossings between scenes, checked in a real browser against the
 * state the page actually holds.
 */

const TILT_MAX = 14;

interface Sample {
  t: number;
  tiltX: number;
  tiltY: number;
  pushX: number;
  shield: number;
  /** where the orb's "up" points after its transform: y < 0 is up */
  upX: number;
  upY: number;
  impacts: number;
}

/** Sample the orb's motion every animation frame for `ms` milliseconds. */
async function sampleOrb(page: Page, ms: number): Promise<Sample[]> {
  return page.evaluate(
    (duration) =>
      new Promise<Sample[]>((resolve) => {
        const push = document.querySelector<HTMLElement>('.orb__push')!;
        const out: Sample[] = [];
        const start = performance.now();
        const num = (name: string) => parseFloat(push.style.getPropertyValue(name)) || 0;
        const frame = (now: number) => {
          const m = new DOMMatrix(getComputedStyle(push).transform);
          const o = m.transformPoint(new DOMPoint(0, 0, 0, 1));
          const u = m.transformPoint(new DOMPoint(0, -100, 0, 1));
          out.push({
            t: now - start,
            tiltX: num('--tilt-x'),
            tiltY: num('--tilt-y'),
            pushX: num('--push-x'),
            shield: num('--shield'),
            upX: u.x / u.w - o.x / o.w,
            upY: u.y / u.w - o.y / o.w,
            impacts: document.querySelectorAll('.orb__impact').length,
          });
          if (now - start < duration) requestAnimationFrame(frame);
          else resolve(out);
        };
        requestAnimationFrame(frame);
      }),
    ms,
  );
}

function expectUpright(samples: Sample[]) {
  for (const s of samples) {
    expect(Math.abs(s.tiltX)).toBeLessThanOrEqual(TILT_MAX + 0.01);
    expect(Math.abs(s.tiltY)).toBeLessThanOrEqual(TILT_MAX + 0.01);
    // "up" stays up: no more than a few degrees of lean in the screen plane
    expect(s.upY).toBeLessThan(-80);
    expect(Math.abs(s.upX)).toBeLessThan(Math.abs(s.upY) * Math.tan((12 * Math.PI) / 180));
  }
}

async function orbBox(page: Page) {
  const box = await page.locator('.orb').boundingBox();
  if (!box) throw new Error('no orb');
  return { ...box, cx: box.x + box.width / 2, cy: box.y + box.height / 2 };
}

/** Mean brightness and a fingerprint of a region of the page. */
async function regionStats(page: Page, clip: { x: number; y: number; width: number; height: number }) {
  const png = await page.screenshot({ clip });
  return page.evaluate(async (b64) => {
    const img = new Image();
    img.src = `data:image/png;base64,${b64}`;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = img.width;
    c.height = img.height;
    const ctx = c.getContext('2d')!;
    ctx.drawImage(img, 0, 0);
    const d = ctx.getImageData(0, 0, c.width, c.height).data;
    let sum = 0;
    for (let i = 0; i < d.length; i += 4) sum += d[i] + d[i + 1] + d[i + 2];
    return { mean: sum / (d.length / 4) / 3, bytes: Array.from(d.slice(0, 4096)) };
  }, png.toString('base64'));
}

const errors: string[] = [];
test.beforeEach(async ({ page }) => {
  errors.length = 0;
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => {
    if (m.type() === 'error') errors.push(m.text());
  });
  await page.goto('/');
  await expect(page.locator('.orb.is-lens')).toBeVisible();
  // let the springs and the first scene settle
  await page.waitForTimeout(800);
});
test.afterEach(() => {
  expect(errors, 'console errors').toEqual([]);
});

test('the sky is a live WebGL universe that moves', async ({ page }) => {
  const sky = page.locator('canvas.universe');
  await expect(sky).toHaveAttribute('data-renderer', 'webgl2');
  // a patch of sky to the left of the globe, clear of the HUD
  const clip = { x: 60, y: 300, width: 260, height: 260 };
  const a = await regionStats(page, clip);
  await page.waitForTimeout(2500);
  const b = await regionStats(page, clip);
  expect(a.mean).toBeGreaterThan(1.5); // not a black page
  expect(a.bytes).not.toEqual(b.bytes); // and it is moving
  // the drawn fallback sky is not loaded when WebGL is there
  await expect(page.locator('.deep-field')).toHaveCount(0);
});

test('the globe is drawn through the WebGL lens', async ({ page }) => {
  const lens = page.locator('.orb__lens');
  await expect(lens).toHaveAttribute('data-renderer', 'webgl2');
  const current = await page.locator('[data-role="current"]').getAttribute('data-scene-id');
  await expect(lens).toHaveAttribute('data-scene', current!);
  const box = await orbBox(page);
  const stats = await regionStats(page, { x: box.cx - 60, y: box.cy - 60, width: 120, height: 120 });
  expect(stats.mean).toBeGreaterThan(20);
});

test('a click wobbles the orb, flares the shield, and never turns it over', async ({ page }) => {
  const box = await orbBox(page);
  const before = await sampleOrb(page, 50);
  expect(before.at(-1)!.shield).toBeLessThan(0.02);

  await page.mouse.click(box.cx + box.width * 0.35, box.cy - box.height * 0.1);
  const samples = await sampleOrb(page, 1500);
  expectUpright(samples);

  // the shield flares and a ripple starts at the point of contact
  expect(Math.max(...samples.map((s) => s.shield))).toBeGreaterThan(0.5);
  expect(samples[0].impacts).toBeGreaterThanOrEqual(1);
  // the touched (right) side tips away, then the globe rocks back past zero
  const tiltY = samples.map((s) => s.tiltY);
  expect(Math.max(...tiltY)).toBeGreaterThan(3);
  expect(Math.min(...tiltY)).toBeLessThan(0);
  // it is nudged away from the touch: to the left
  expect(Math.min(...samples.map((s) => s.pushX))).toBeLessThan(-2);

  // and it settles: shield out, tilt home, ripple gone
  await page.waitForTimeout(1500);
  const after = await sampleOrb(page, 50);
  const last = after.at(-1)!;
  expect(last.shield).toBeLessThan(0.03);
  expect(Math.abs(last.tiltX)).toBeLessThan(0.3);
  expect(Math.abs(last.tiltY)).toBeLessThan(0.3);
  expect(last.impacts).toBe(0);
});

test('a storm of clicks and wheel flicks stays inside the limits', async ({ page }) => {
  const box = await orbBox(page);
  const run = sampleOrb(page, 2200);
  for (let i = 0; i < 12; i += 1) {
    const a = (i / 12) * Math.PI * 2;
    await page.mouse.click(box.cx + Math.cos(a) * box.width * 0.45, box.cy + Math.sin(a) * box.height * 0.45);
    await page.mouse.wheel(0, i % 2 ? 900 : -900);
  }
  const samples = await run;
  expectUpright(samples);
  expect(Math.max(...samples.map((s) => Math.abs(s.tiltX)))).toBeGreaterThan(5);
});

test('the wheel rocks the orb and it rocks back', async ({ page }) => {
  const run = sampleOrb(page, 1600);
  for (let i = 0; i < 6; i += 1) await page.mouse.wheel(0, 300);
  const samples = await run;
  expectUpright(samples);
  // scrolling down tips the top away
  expect(Math.max(...samples.map((s) => s.tiltX))).toBeGreaterThan(4);
  await page.waitForTimeout(1500);
  const last = (await sampleOrb(page, 50)).at(-1)!;
  expect(Math.abs(last.tiltX)).toBeLessThan(0.3);
});

test('dragging leans the orb toward the pointer and it springs home on release', async ({ page }) => {
  const box = await orbBox(page);
  await page.mouse.move(box.cx, box.cy);
  await page.mouse.down();
  for (let i = 1; i <= 10; i += 1) await page.mouse.move(box.cx + i * 15, box.cy);
  await page.waitForTimeout(900);
  const held = (await sampleOrb(page, 50)).at(-1)!;
  expect(held.tiltY).toBeGreaterThan(5);
  expect(held.tiltY).toBeLessThanOrEqual(TILT_MAX);
  expect(held.pushX).toBeGreaterThan(5);
  expect(held.shield).toBeGreaterThanOrEqual(0.3);
  expectUpright([held]);

  await page.mouse.up();
  const release = await sampleOrb(page, 1200);
  expectUpright(release);
  await page.waitForTimeout(1500);
  const last = (await sampleOrb(page, 50)).at(-1)!;
  expect(Math.abs(last.tiltY)).toBeLessThan(0.3);
  expect(Math.abs(last.pushX)).toBeLessThan(0.5);
  expect(last.shield).toBeLessThan(0.03);
});

test('scenes cross through the lens and the old scene leaves the page', async ({ page }) => {
  test.setTimeout(60_000);
  const lens = page.locator('.orb__lens');
  const first = await lens.getAttribute('data-scene');
  await expect(lens).toHaveAttribute('data-transition', 'crossing', { timeout: 40_000 });
  await expect(page.locator('[data-role="previous"]')).toHaveCount(1);
  await expect(lens).toHaveAttribute('data-transition', 'idle', { timeout: 6_000 });
  const next = await lens.getAttribute('data-scene');
  expect(next).not.toBe(first);
  await expect(page.locator('[data-role="current"]')).toHaveAttribute('data-scene-id', next!);
  await expect(page.locator('[data-role="previous"]')).toHaveCount(0, { timeout: 3_000 });
});

test.describe('on a touch screen', () => {
  test.use({ hasTouch: true, isMobile: true, viewport: { width: 390, height: 844 } });

  test('a tap flares the shield and wobbles the orb upright', async ({ page }) => {
    const box = await orbBox(page);
    await page.touchscreen.tap(box.cx - box.width * 0.3, box.cy + box.height * 0.2);
    const samples = await sampleOrb(page, 1000);
    expectUpright(samples);
    expect(Math.max(...samples.map((s) => s.shield))).toBeGreaterThan(0.5);
    expect(Math.max(...samples.map((s) => Math.abs(s.tiltY)))).toBeGreaterThan(3);
    await expect(page.locator('canvas.universe')).toHaveAttribute('data-renderer', 'webgl2');
  });
});

test.describe('with reduced motion', () => {
  test.use({ reducedMotion: 'reduce' });

  test('the sky still draws, and a click still flares the shield with only a small wobble', async ({ page }) => {
    await expect(page.locator('canvas.universe')).toHaveAttribute('data-renderer', 'webgl2');
    const box = await orbBox(page);
    await page.mouse.click(box.cx + box.width * 0.35, box.cy);
    const samples = await sampleOrb(page, 1000);
    expectUpright(samples);
    expect(Math.max(...samples.map((s) => s.shield))).toBeGreaterThan(0.5);
    expect(Math.max(...samples.map((s) => Math.abs(s.tiltY)))).toBeLessThan(6);
  });
});

/** Share of pixels in a region that changed by more than `threshold` levels. */
type Shot = Awaited<ReturnType<Page['screenshot']>>;

async function changedShare(
  page: Page,
  a: Shot,
  b: Shot,
  threshold: number,
): Promise<number> {
  return page.evaluate(
    async ([x, y, t]) => {
      const load = async (b64: string) => {
        const img = new Image();
        img.src = `data:image/png;base64,${b64}`;
        await img.decode();
        const c = document.createElement('canvas');
        c.width = img.width;
        c.height = img.height;
        const ctx = c.getContext('2d')!;
        ctx.drawImage(img, 0, 0);
        return ctx.getImageData(0, 0, c.width, c.height).data;
      };
      const da = await load(x);
      const db = await load(y);
      let changed = 0;
      for (let i = 0; i < da.length; i += 4) {
        const d = Math.max(Math.abs(da[i] - db[i]), Math.abs(da[i + 1] - db[i + 1]), Math.abs(da[i + 2] - db[i + 2]));
        if (d > t) changed += 1;
      }
      return changed / (da.length / 4);
    },
    [a.toString('base64'), b.toString('base64'), threshold] as const,
  );
}

test.describe('depth', () => {
  test('the default film plays packed with its depth', async ({ page }) => {
    const lens = page.locator('.orb__lens');
    await expect(lens).toHaveAttribute('data-depth', 'film', { timeout: 10_000 });
    const video = page.locator('[data-role="current"] video');
    await expect(video).toHaveClass(/orb__video--packed/);
    const size = await video.evaluate((v: HTMLVideoElement) => [v.videoWidth, v.videoHeight]);
    expect(size).toEqual([1536, 1024]);
  });

  test('tipping the globe slides near surfaces against far ones', async ({ page }) => {
    // a still scene, held, so the only thing that can move the city is depth
    await page.goto('/?scene=globe-day');
    const lens = page.locator('.orb__lens');
    await expect(lens).toHaveAttribute('data-depth', 'still', { timeout: 10_000 });
    // take the page's own motion out of the picture: no CSS tilt, no float,
    // no shield, no glass highlight — only what the lens draws
    await page.addStyleTag({
      content:
        '.orb{animation:none!important}.orb__push{transform:none!important}' +
        '.orb__shield,.orb__glint,.orb__reticle{display:none!important}',
    });
    await page.waitForTimeout(600);
    const box = await orbBox(page);
    // the middle of the city, clear of the rim where the held shield glows
    const clip = { x: box.cx - box.width * 0.2, y: box.cy - box.height * 0.2, width: box.width * 0.4, height: box.height * 0.4 };
    const rest = await page.screenshot({ clip });

    await page.mouse.move(box.cx, box.cy);
    await page.mouse.down();
    for (let i = 1; i <= 10; i += 1) await page.mouse.move(box.cx + i * 15, box.cy);
    await page.waitForTimeout(900);
    const parallax = (await lens.getAttribute('data-parallax'))!.split(',').map(Number);
    expect(parallax[0]).toBeGreaterThan(0.02);
    const tipped = await page.screenshot({ clip });
    await page.mouse.up();

    // edges of towers move by several pixels: well past the film grain
    const share = await changedShare(page, rest, tipped, 28);
    expect(share).toBeGreaterThan(0.02);

    // and it all comes back when the globe settles
    await page.waitForTimeout(2200);
    await expect(lens).toHaveAttribute('data-parallax', /^-?0\.000\d,-?0\.000\d$/);
    const settled = await page.screenshot({ clip });
    expect(await changedShare(page, rest, settled, 28)).toBeLessThan(0.002);
  });
});
