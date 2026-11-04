/**
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 * All rights reserved.
 *
 * This source code is licensed under the BSD-style license found in the
 * LICENSE file in the root directory of this source tree.
 */

import { defineConfig } from 'vite';

export default defineConfig({
  base: './',
  server: {
    open: false,
    strictPort: true,
    proxy: {
      '/api': 'http://127.0.0.1:8790',
    },
  },
  build: {
    outDir: 'dist',
    assetsInlineLimit: 4096,
  },
});
