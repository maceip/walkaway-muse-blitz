import { defineConfig } from 'vite';

export default defineConfig({
  base: './',
  server: {
    open: false,
    strictPort: true,
  },
  build: {
    outDir: 'dist',
    assetsInlineLimit: 4096,
  },
});
