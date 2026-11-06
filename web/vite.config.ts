import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
export default defineConfig({
  plugins: [vue()],
  base: "/dashboard/",
  server: {
    proxy: {
      "/api": {
        target: process.env.API_URL || "http://127.0.0.1:3006",
        // Preserve the browser Host so control Origin checks also work in development.
        changeOrigin: false,
      },
    },
  },
});
