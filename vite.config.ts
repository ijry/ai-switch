import react from "@vitejs/plugin-react";
import UnoCSS from "unocss/vite";
import { readFile } from "node:fs/promises";
import { defineConfig, type Plugin } from "vite";

function patchOcradSource(code: string) {
  return code
    .replace(/\b0([0-7]{3})\b/g, "0o$1")
    .replace(/this\[['"]Module['"]\]\s*=\s*Module;/g, 'globalThis["Module"] = Module;');
}

function ocradOptimizeDepsPlugin() {
  return {
    name: "ocrad-optimize-deps",
    setup(build: {
      onLoad: (
        options: { filter: RegExp },
        callback: (args: { path: string }) => Promise<{ contents: string; loader: "js" }>,
      ) => void;
    }) {
      build.onLoad({ filter: /ocrad\.js[\\/]ocrad\.js$/ }, async (args) => ({
        contents: patchOcradSource(await readFile(args.path, "utf8")),
        loader: "js",
      }));
    },
  };
}

function ocradLegacyOctalPlugin(): Plugin {
  return {
    name: "ocrad-legacy-octal",
    enforce: "pre",
    transform(code, id) {
      const normalizedId = id.split("?")[0].replace(/\\/g, "/");
      const isOcradModule =
        normalizedId.endsWith("/ocrad.js") || normalizedId.includes("/.vite/deps/ocrad__js.js");

      if (!isOcradModule) {
        return null;
      }

      return {
        code: patchOcradSource(code),
        map: null,
      };
    },
  };
}

export default defineConfig({
  plugins: [ocradLegacyOctalPlugin(), UnoCSS(), react()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    proxy: {
      "/api": {
        target: "http://127.0.0.1:3090",
        changeOrigin: true,
      },
      "/ws/events": {
        target: "http://127.0.0.1:3090",
        changeOrigin: true,
        ws: true,
      },
    },
  },
  optimizeDeps: {
    esbuildOptions: {
      plugins: [ocradOptimizeDepsPlugin()],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2020",
    minify: false,
  },
});
