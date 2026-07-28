/// <reference types="vitest" />
// Imported from vitest/config rather than vite so the `test` block typechecks.
import { defineConfig } from 'vitest/config';

export default defineConfig({
  // Relative asset URLs so the same bundle works from Base44 hosting and from
  // the Tauri webview, which serves over a custom protocol.
  base: './',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    // Tauri ships its own webview; there is no need to support old browsers.
    target: 'es2022',
    // Icons arrive as data URLs from Rust, so nothing should be inlined further.
    assetsInlineLimit: 0,
  },
  server: {
    port: 5173,
    // Fail instead of silently moving ports: the Tauri dev config points here.
    strictPort: true,
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
