import {chromium} from 'playwright';
import {mkdirSync} from 'node:fs';
import {join} from 'node:path';

const ARTIFACT_DIR =
  '/Users/mac/.gemini/antigravity/brain/fe8b99f7-f96f-4bb3-b19e-b16e765f2dbb';
mkdirSync(ARTIFACT_DIR, {recursive: true});

const browser = await chromium.launch({
  headless: true,
  executablePath:
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
});

try {
  const context = await browser.newContext({
    viewport: {width: 600, height: 600},
  });
  const page = await context.newPage();

  // 1. Passkey screen + Approve action -> real backend /api/decision-ack
  await page.goto('http://127.0.0.1:8799/?mode=passkey', {waitUntil: 'load'});
  await page.waitForSelector('h1:has-text("Sign in with Passkey?")');
  await page.screenshot({
    path: join(ARTIFACT_DIR, 'astryx_glasses_passkey.png'),
  });

  await page.click('button:has-text("Approve Passkey")');
  await page.waitForSelector('text=Approved Passkey ES256 Signature');

  // 2. Cascading Failure screen + Abort Run action
  await page.click('[role="radio"]:has-text("Failure")');
  await page.waitForSelector('h1:has-text("Cascading failure — Take over?")');
  await page.screenshot({
    path: join(ARTIFACT_DIR, 'astryx_glasses_failure.png'),
  });
  await page.click('button:has-text("Take Over")');
  await page.waitForSelector('text=Initiated Manual Worktree Takeover');

  // 3. Ready to Start screen
  await page.click('[role="radio"]:has-text("Start")');
  await page.waitForSelector('h1:has-text("Ready to start the live demo?")');
  await page.screenshot({
    path: join(ARTIFACT_DIR, 'astryx_glasses_start.png'),
  });

  // 4. WebAuthn Probe screen + 1D keyboard navigation
  await page.click('[role="radio"]:has-text("WebAuthn")');
  await page.waitForSelector('h1:has-text("window.credentials Status")');
  await page.keyboard.press('ArrowRight');
  await page.screenshot({
    path: join(ARTIFACT_DIR, 'astryx_glasses_probe.png'),
  });

  // Verify backend reports count incremented
  const reportsRes = await page.evaluate(async () => {
    const r = await fetch('/api/reports');
    return r.json();
  });
  if (!reportsRes.count || reportsRes.count < 3) {
    throw new Error(`Expected >= 3 backend reports, got ${reportsRes.count}`);
  }
  console.log('Astryx E2E verification passed! Reports:', reportsRes.count);
} finally {
  await browser.close();
}
