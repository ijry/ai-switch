import react from "@vitejs/plugin-react";
import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    exclude: [...configDefaults.exclude, "**/.worktrees/**"],
    setupFiles: ["src/test/setup.ts"],
    globals: true,
  },
});
