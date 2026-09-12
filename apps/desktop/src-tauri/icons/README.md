# 拾文应用图标

`source.png` 是内置 imagegen 工具生成的原始母版，保留透明通道。图形采用翠绿渐变底色、白色文件与归档盒，表达收集聊天附件的用途。界面和 Windows 应用资源均从此母版导出。

生成时间：2026-09-11。使用内置工具，未调用 CLI/API 回退模式。

最终提示词：

> Generate one production desktop application icon for 拾文 / ChatVault, a personal chat attachment archive and retrieval app. A single bold white stylized folded document being gathered into a protective archive tray, visually integrated as one memorable geometric mark, on a rich emerald-to-teal softly graduated rounded-square tile. Front-facing orthographic, restrained premium Windows desktop app aesthetic, crisp edges, large simple shapes with generous spacing and strong readability at 16px. Tile centered filling 90% of a square 1024x1024 canvas. Transparent outside rounded tile, genuine alpha. No text, no letters, no watermark, no presentation mockup, no additional icons, no busy details. Deliver a clean final icon asset.

重新导出（从仓库根目录运行）：

```powershell
pnpm --filter chatvault-desktop exec tauri icon src-tauri/icons/source.png --output ../../target/generated-icons
Copy-Item target/generated-icons/icon.ico,target/generated-icons/32x32.png,target/generated-icons/128x128.png,target/generated-icons/128x128@2x.png -Destination apps/desktop/src-tauri/icons
Copy-Item target/generated-icons/128x128.png -Destination apps/desktop/src/assets/app-icon.png
```

当前只将 Windows 及界面使用的导出资源纳入仓库；其他平台资源留在 `target/`。
