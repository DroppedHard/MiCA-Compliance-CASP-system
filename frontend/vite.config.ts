import react from "@vitejs/plugin-react"
import { defineConfig } from "vite"

const backend = process.env.VITE_DEV_PROXY_TARGET ?? "http://127.0.0.1:3200"

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5174,
    proxy: {
      "/api": backend,
      "/health": backend,
    },
  },
})
