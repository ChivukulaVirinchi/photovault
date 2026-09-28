import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  // MapLibre creates its worker from a sibling ESM file. Vite's dependency
  // optimizer can move the main module without that worker, producing a
  // missing node_modules/.vite/deps/maplibre-gl-worker.mjs at runtime.
  optimizeDeps: { exclude: ["maplibre-gl"] },
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "esnext",
    // Release builds should not ship the development-sized JS bundle. Keep
    // source maps for the local build so native crash reports can still be
    // symbolized; packaging can omit them if the installer budget requires it.
    minify: true,
    sourcemap: true,
  },
});
