# PanelMemory item「📂 在 Finder 显示 detail」按钮（iter #253）

## Background

PanelMemory 每条 item 已有「编辑 / 🚀 外部打开 / 📋📄 复制绝对路径 / 🔗 复制
为 ref」等动作按钮，但缺一个"在 Finder 高亮选中本文件"的入口。`🚀 外部打开`
直接把文件交给 .md 编辑器（VSCode / Typora），适合写；但 owner 有时想做的是
git add / 拖到 chat / 用其它工具操作 → 需要 Finder 选中视图。

PanelTasks 行内已有 📂 reveal 按钮（与外部打开并存），本迭代把 PanelMemory
补上对偶动作。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 在每条 item 的 🚀 按钮之后插入 📂 按钮
- click → `invoke("memory_reveal_detail_in_finder", { detailPath: item.detail_path })`
- 失败 → setMessage 显错误 4s 自清；成功无 toast（owner 看 Finder 切前台
  即视觉确认）

复用既有 `memory_reveal_detail_in_finder` tauri 命令（已实现跨平台
`open -R` / `explorer /select,` / `xdg-open` 退化）—— 不需要新后端代码。

## Key design decisions

- **🚀 与 📂 并存，不合并**：owner 实际使用里二者意图不同（编辑 vs 定位）；
  合并到一个按钮 + 修饰键（如 ⌥click reveal）增加学习成本。两个图标语义自
  明：🚀 = 起飞到编辑器，📂 = 打开文件夹定位。
- **失败显 setMessage，成功无 toast**：Finder 跳到前台本身是视觉确认；多
  弹 "✓ 已在 Finder 显示" toast 反而打扰。失败常见原因是路径不存在（极旧
  memory item 没写 detail.md），需要明确告诉用户。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.18s)
