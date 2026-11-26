import { defineConfig } from '@playwright/test';

/**
 * End-to-end checks of the page in a real Chromium with a real GPU path
 * (ANGLE on Metal on a Mac), against the Vite dev server — or, with
 * REDTEAM_URL set, against a deployed copy of the site:
 *
 *     REDTEAM_URL=https://redteam.model.tools npm run test:e2e
 */
// The app has no @types/node; this is the one Node global the config reads.
declare const process: { env: Record<string, string | undefined> };
const deployed = process.env.REDTEAM_URL;

export default defineConfig({
  testDir: 'e2e',
  timeout: 60_000,
  fullyParallel: false,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: deployed ?? 'http://127.0.0.1:5180',
    viewport: { width: 1440, height: 900 },
    launchOptions: {
      args: ['--use-angle=metal', '--ignore-gpu-blocklist', '--enable-gpu'],
    },
  },
  webServer: deployed
    ? undefined
    : {
        command: 'npx vite',
        url: 'http://127.0.0.1:5180',
        reuseExistingServer: true,
        timeout: 60_000,
      },
});
