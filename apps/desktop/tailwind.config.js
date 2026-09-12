// ChatVault 桌面端 Tailwind 配置：语义色 token 映射
/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{vue,js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        cv: {
          bg: "var(--cv-bg)",
          surface: "var(--cv-surface)",
          "surface-2": "var(--cv-surface-2)",
          border: "var(--cv-border)",
          text: "var(--cv-text)",
          "text-2": "var(--cv-text-2)",
          "text-3": "var(--cv-text-3)",
          accent: "var(--cv-accent)",
          "accent-fg": "var(--cv-accent-fg)",
          "accent-soft": "var(--cv-accent-soft)",
          danger: "var(--cv-danger)",
          success: "var(--cv-success)",
          warning: "var(--cv-warning)",
        },
      },
      borderRadius: {
        cv: "6px",
        "cv-lg": "8px",
      },
      fontSize: {
        "cv-page": ["20px", { lineHeight: "1.3", fontWeight: "600" }],
        "cv-section": ["14px", { lineHeight: "1.4", fontWeight: "600" }],
        "cv-body": ["13px", { lineHeight: "1.5", fontWeight: "400" }],
        "cv-caption": ["12px", { lineHeight: "1.4", fontWeight: "400" }],
      },
    },
  },
  plugins: [],
};
