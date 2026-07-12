import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: "#12110f",
        paper: "#f4efe5",
        moss: "#59684f",
        ember: "#c45b38",
        steel: "#516170",
      },
      fontFamily: {
        display: ["Aptos Display", "Bahnschrift", "Segoe UI", "sans-serif"],
        body: ["Aptos", "Segoe UI", "sans-serif"],
      },
    },
  },
  plugins: [],
} satisfies Config;
