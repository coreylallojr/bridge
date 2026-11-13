import { resolve } from 'node:path'
import { defineConfig } from 'vite'

// The landing page (GitHub Pages). It imports the app's own reactor and themes from src/renderer, so the site shows
// the same arc reactor Bridge runs. Build: npx vite build --config site/vite.config.ts  ->  site/dist
export default defineConfig({
  root: __dirname,
  base: './',
  resolve: { alias: { '@shared': resolve(__dirname, '../src/shared') } },
  build: { outDir: resolve(__dirname, 'dist'), emptyOutDir: true, target: 'es2022', chunkSizeWarningLimit: 2500 },
  server: { host: '127.0.0.1', port: 5174, strictPort: true },
})
