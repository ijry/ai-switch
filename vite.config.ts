import react from "@vitejs/plugin-react";
import UnoCSS from "unocss/vite";
import { defineConfig, type Plugin } from "vite";

function ocradLegacyOctalPlugin(): Plugin {
  return {
    name: "ocrad-legacy-octal",
    enforce: "pre",
    transform(code, id) {
      if (!id.includes("ocrad.js") || !id.endsWith("ocrad.js")) {
        return null;
      }

      return {
        code: code
          .replace(/\b0([0-7]{3})\b/g, "0o$1")
          .replace(/this\[['"]Module['"]\]\s*=\s*Module;/g, 'globalThis["Module"] = Module;'),
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
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "es2020",
    minify: false,
  },
});
