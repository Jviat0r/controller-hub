import { defineConfig } from "vite";

// Dev server for the dumb UI. Tauri loads http://localhost:1420 in dev and
// the built ../dist folder in release. No framework, no runtime deps.
export default defineConfig({
  root: "ui",
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: "../dist", emptyOutDir: true, target: "es2022" },
});
