import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// The build output is served as static files by server/index.mjs, so the
// asset layout is fixed here: hashed bundles under /assets, everything
// copied from public/ at the root.
export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist',
    assetsDir: 'assets',
    target: 'es2022',
    sourcemap: false,
    cssCodeSplit: true,
    reportCompressedSize: false,
  },
  server: {
    host: '127.0.0.1',
    port: 5180,
    strictPort: true,
  },
  preview: {
    host: '127.0.0.1',
    port: 5180,
    strictPort: true,
  },
});
