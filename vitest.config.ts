import react from "@vitejs/plugin-react";
import { configDefaults, defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    exclude: [
      ...configDefaults.exclude,
      "**/.codex-run/**",
      "**/.worktrees/**",
      // Agent worktrees live here and hold a full checkout, so without this
      // vitest collects a second copy of every suite plus the `.test.mjs`
      // scripts the pattern below only excludes at the repo root.
      "**/.claude/**",
      "scripts/**/*.test.mjs",
    ],
    setupFiles: ["src/test/setup.ts"],
    globals: true,
  },
});
