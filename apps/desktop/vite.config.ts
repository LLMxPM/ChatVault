// ChatVault 桌面端 Vite 配置文件
// 配置开发服务器端口、Tauri 环境变量前缀与热重载规则

import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "path";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  // 防止 Vite 遮蔽 Tauri 错误
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // 忽略后端 Rust 源码变更，避免无意义前端热刷新
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: {
    // Tauri 在 Windows 上使用 Chromium
    target: process.env.TAURI_ENV_PLATFORM === "windows" ? "chrome105" : "safari13",
    // 调试模式不进行代码混淆
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
