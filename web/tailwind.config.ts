import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: ["class", '[data-theme="dark"]'],
  theme: {
    extend: {
      colors: {
        bg: "var(--color-bg)",
        surface: "var(--color-surface)",
        border: "var(--color-border)",
        muted: "var(--color-muted)",
        fg: "var(--color-fg)",
        "fg-muted": "var(--color-fg-muted)",
        accent: "var(--color-accent)",
        "accent-fg": "var(--color-accent-fg)",
        destructive: "var(--color-destructive)",
      },
      fontFamily: {
        sans: ["'Inter Variable'", "Inter", "system-ui", "sans-serif"],
        mono: ["'JetBrains Mono'", "ui-monospace", "monospace"],
      },
      borderRadius: { sm: "4px", DEFAULT: "6px", md: "8px", lg: "12px" },
      transitionTimingFunction: { swift: "cubic-bezier(0.16, 1, 0.3, 1)" },
    },
  },
} satisfies Config;
