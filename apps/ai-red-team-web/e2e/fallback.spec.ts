import { expect, test } from '@playwright/test';

/**
 * A browser without WebGL gets the page as it was before the live sky and
 * the lens: the drawn still sky, and the scenes shown as plain pictures and
 * films. A film packed with its depth must show only its colour square.
 */
test.use({ launchOptions: { args: ['--disable-3d-apis', '--disable-webgl'] } });

test('without WebGL the page falls back to the drawn sky and the plain scenes', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto('/');
  await expect(page.locator('.deep-field')).toHaveCount(1);
  await expect(page.locator('canvas.universe')).toHaveCount(0);
  await expect(page.locator('.orb')).not.toHaveClass(/is-lens/);
  await expect(page.locator('.orb__lens')).toHaveCSS('opacity', '0');
  const video = page.locator('[data-role="current"] video');
  await expect(video).toHaveClass(/orb__video--packed/);
  expect(await video.evaluate((v) => getComputedStyle(v).objectPosition)).toBe('0% 50%');
  // and a poke still wobbles the globe and flares the shield
  const box = (await page.locator('.orb').boundingBox())!;
  await page.mouse.click(box.x + box.width * 0.8, box.y + box.height * 0.5);
  await expect(page.locator('.orb__impact')).toHaveCount(1);
  expect(errors).toEqual([]);
});
