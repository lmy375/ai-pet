# PanelMemory item hover tooltip 加 "✏️ 双击编辑" onboarding hint + 上轮 TODO 替换

## 背景

### 上轮 "pet collapse tab hover 1s ambient card" 不可行说明

上轮 auto-propose 的"桌面 pet collapse tab hover 1s 浮 ambient mini card"在实现层面与 useAutoHide 的"mouse-enter tab → 立刻 slideBack 完全展开" UX 冲突。要让"hover 1s 内不展开 + 显微卡片"工作，需要：
1. useAutoHide 加 "intermediate ambient state"（部分滑出仅 200px）
2. 加 1s timer 决定是否继续完全展开
3. 微卡片渲染在那 200px 区域内（仅 pet window 这个 webview 可在那个屏幕位置渲染）

是个 UX 重构，单 iter risk 过大。移除该项，替换为 5 条新提案。

### 本 iter 实现："双击编辑" onboarding hint

PanelMemory item 列表行支持：
- **双击 title** → inline rename
- **双击 description** → 内联编辑

首次用户难以发现 —— 没有视觉提示告诉这个交互存在。iter #194 给 hover preview tooltip 加了 📅 创建 / 🔄 更新 时间段。本 iter 给 tooltip 底脚追加一行 muted hint：

```
✏️ 双击 title 改名 · 双击 description 改内容
```

让 hover 自然成为 onboarding 路径。

## 改动

### `src/components/panel/PanelMemory.tsx`

既有 hover preview tooltip 底加：

```tsx
<div style={{
  marginTop: 6, paddingTop: 4,
  borderTop: "1px dashed var(--pet-color-border)",
  fontSize: 9, color: muted, fontStyle: "italic", opacity: 0.7,
}}>
  ✏️ 双击 title 改名 · 双击 description 改内容
</div>
```

- dashed top border 与 detail preview 段之间视觉拉开
- fontSize 9 / opacity 0.7 让 hint 不喧宾夺主
- 任何 hover 都显（不 gate 在"首次"—— 即便老 owner 看到这条 hint 也只占一行视觉，不烦）

## 关键设计

- **存在感最低**：9px 字号 + 0.7 opacity + italic + muted color 让 hint 几乎与底色融为一体 —— 知道存在的 owner 不被打扰，首次 hover 的 owner 能扫到。
- **dashed top divider 拉开 hierarchy**：tooltip 内的"内容"段（detail preview）与"hint"段视觉错开，让 owner 阅读时不混。
- **不引新 state / 引导覆盖**：纯 inline 渲染。不需"首次 hover 弹大 onboarding"复杂 flow。
- **统一在 tooltip 底**：不论 detail.md 有无内容（previewText 路径 / "无内容" fallback 路径）都显 —— 让任何 hover 都到 hint。
- **不进 Settings 关闭开关**：占位只一行 + opacity 极低；不值得给 setting 加 toggle 引入维护负担。

## 不做

- **不在每个 row 渲一个永久"✏️"按钮**：visual 噪音；hover preview 已经是"想看更多"信号点，把 hint 挂这里自然。
- **不写"首次 detect+ 弹大 banner"**：onboarding 时机识别复杂 + 重复触发风险；hint 永显小字成本更可控。
- **不引新 i18n / 本地化**：当前所有 UI 中文一致。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.20s
- 改动 ~25 行（既有 fallback 内 + 新 hint div + 注释）。既有 hover preview pipeline / 📅 时间段 / detail preview / 既有 dblclick rename handlers 完全不动。

## TODO 状态

剩 4 条留池：
- PanelMemory 顶 chip 行加 "🔇 仅 silent" 筛选 toggle
- detail.md 编辑器 toolbar 加「📤 复制 LLM consume 段」按钮
- butler_task `[every:]` 解析 "工作日 09:00" / "周末 10:00" 周内限定
- PanelTasks 任务行右键菜单加「📋 复制 title 作 ref token」简短按钮

## 后续

- 同 hint 模式扩到 PanelTasks 行 hover preview / PanelChat user 气泡 / ChatMini bubble —— 让"双击 / 长按 / 右键" 的 hidden interaction 都通过 hover hint 浮现。
- onboarding cheatsheet 一页 panel 集中展示 keyboard shortcut + hidden interactions。
