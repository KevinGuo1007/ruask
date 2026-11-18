import path from "node:path"
import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

// base: "./" keeps every asset path relative, so the build works both at
// https://<user>.github.io/ruask/ and on a custom domain root.
export default defineConfig({
  base: "./",
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  server: { port: 4173, host: "127.0.0.1" },
  preview: { port: 4173, host: "127.0.0.1" },
  // React + motion + Radix land at ~185 kB gzip in one chunk; fine for a single page.
  build: { chunkSizeWarningLimit: 650 },
})
