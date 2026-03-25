import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

// https://vite.dev/config/
export default defineConfig(async ({ mode }) => {
  const env = loadEnv(mode, ".", "");
  const host = env.TAURI_DEV_HOST;

  return {
    plugins: [react()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
        protocol: "ws",
        host,
        port: 1421,
      }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
    build: {
      outDir: "dist/web",
      emptyOutDir: true,
      rollupOptions: {
        output: {
          manualChunks(id: string) {
            if (id.includes("/src/features/settings/")) return "settings";
            if (id.includes("/src/features/tag/")) return "tag";
            if (id.includes("/src/features/emoji/")) return "emoji";
            if (id.includes("/src/features/clipboard/components/CompactPreviewWindow")) {
              return "compact-preview";
            }

            if (!id.includes("node_modules")) return;

            if (
              id.includes("/react-select/") ||
              id.includes("/@emotion/") ||
              id.includes("/@floating-ui/") ||
              id.includes("/react-transition-group/") ||
              id.includes("/memoize-one/")
            ) {
              return "vendor-react-select";
            }

            if (
              id.includes("/framer-motion/") ||
              id.includes("/motion-dom/") ||
              id.includes("/motion-utils/")
            ) {
              return "vendor-motion";
            }

            if (id.includes("/react-virtuoso/")) return "vendor-virtuoso";

            if (
              id.includes("/@tauri-apps/api/") ||
              id.includes("/@tauri-apps/plugin-dialog/") ||
              id.includes("/@tauri-apps/plugin-opener/")
            ) {
              return "vendor-tauri";
            }

            if (
              id.includes("/react/") ||
              id.includes("/react-dom/") ||
              id.includes("/scheduler/")
            ) {
              return "vendor-react";
            }
          }
        }
      }
    }
  };
});
