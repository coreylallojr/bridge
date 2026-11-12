import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// The renderer only. Tauri serves dist/ from inside the exe (no dev server, no network).
export default defineConfig({
  root: resolve(__dirname, 'src/renderer'),
  base: './',
  resolve: { alias: { '@shared': resolve(__dirname, 'src/shared') } },
  plugins: [react()],
  build: { outDir: resolve(__dirname, 'dist'), emptyOutDir: true, target: 'es2022', chunkSizeWarningLimit: 2500 },
  server: { host: '127.0.0.1', port: 5173, strictPort: true },
})
