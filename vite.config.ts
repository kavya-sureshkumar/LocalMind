import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || "0.0.0.0",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    // In dev, proxy the same-origin API endpoints to the Axum LAN server so
    // the phone can hit Vite (1420) for both the React app and `/api`/`/v1`
    // calls. Removes the need to type two different URLs into the phone.
    proxy: {
      "/api": { target: "http://127.0.0.1:3939", changeOrigin: true },
      "/v1": { target: "http://127.0.0.1:3939", changeOrigin: true },
      "/health": { target: "http://127.0.0.1:3939", changeOrigin: true },
      "/sd-images": { target: "http://127.0.0.1:3939", changeOrigin: true },
    },
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
