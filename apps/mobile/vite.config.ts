import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

// Mobile shell reuses the desktop UI source directly (single source of
// truth for every screen). The bridge in the desktop code detects
// Tauri and calls this app's engine_request, which forwards to the
// linkfyrd daemon over HTTP.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  resolve: {
    alias: {
      "@linkfyr/types": fileURLToPath(
        new URL("../../packages/types/src/index.ts", import.meta.url),
      ),
    },
  },
  server: {
    port: 1421,
    strictPort: true,
    fs: {
      // Allow reading the shared desktop sources outside this package.
      allow: ["..", "../..", "../../.."],
    },
  },
  build: {
    target: "es2022",
    minify: "esbuild",
    sourcemap: false,
  },
});
