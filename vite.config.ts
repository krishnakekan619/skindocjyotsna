import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

// Vite serves the React UI; Tauri loads it from http://localhost:1420 in dev
// and from web-dist/ in release builds (see src-tauri/tauri.conf.json).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    outDir: 'web-dist',
    emptyOutDir: true,
    // WebView2 (Chromium) on Windows, WKWebView on macOS 12+ (Safari 15).
    target: ['es2021', 'chrome105', 'safari15'],
    sourcemap: false,
  },
  test: {
    include: ['src/**/*.test.{ts,tsx}'],
    environment: 'node',
  },
});
