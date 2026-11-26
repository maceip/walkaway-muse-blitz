import { defineConfig } from '@playwright/test';

/**
 * End-to-end checks of the page in a real Chromium with a real GPU path
 * (ANGLE on Metal on a Mac), against the Vite dev server.
 */
export default defineConfig({
  testDir: 'e2e',
  timeout: 60_000,
  fullyParallel: false,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:5180',
    viewport: { width: 1440, height: 900 },
    launchOptions: {
      args: ['--use-angle=metal', '--ignore-gpu-blocklist', '--enable-gpu'],
    },
  },
  webServer: {
    command: 'npx vite',
    url: 'http://127.0.0.1:5180',
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
