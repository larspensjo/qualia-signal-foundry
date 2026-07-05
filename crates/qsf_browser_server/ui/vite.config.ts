import { defineConfig } from "vite";

const apiTarget = process.env.QSF_BROWSER_API_URL ?? "http://127.0.0.1:3939";

export default defineConfig({
  server: {
    port: 5173,
    proxy: {
      "/api": apiTarget,
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
