import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri drives this dev server, so the port is fixed and failures must be loud
// rather than silently falling back to another port the window won't load.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Rust rebuilds are handled by Tauri; watching them just churns HMR.
      ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"],
    },
  },
});
