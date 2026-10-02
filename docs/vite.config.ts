import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dir = dirname(fileURLToPath(import.meta.url));

// LinkFYR docs site (discord-botlists docs architecture).
// Static HTML per route is produced by scripts/prerender.ts after build.
export default defineConfig({
  root: __dir,
  base: process.env.VITE_BASE ?? "/",
  plugins: [react(), tailwindcss()],
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false },
  server: { port: 4179 },
});
