# 聊天消息复制按钮 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 聊天消息复制按钮：panel chat 每条 user/assistant 消息悬停显示「复制」，把 AI 回复存到外部笔记不必手选。

## 目标

Panel 聊天里每条 user / assistant 消息加一个 hover 才出现的小「复制」按钮，
点击 → `navigator.clipboard.writeText(content)`，并临时显示「已复制」反馈
（1.5s 自动恢复）。让用户把 AI 写的好句子 / 有用代码片段一键存到外部笔记
（Bear / Notion / Obsidian 等），不必鼠标涂选 + 复制 + 粘贴三步走。

## 非目标

- 桌面气泡不加复制按钮 —— 气泡是即时阅读 surface，且只显示一句；要保留 / 引用
  请打开 panel chat。
- 不复制 tool / error 类型消息 —— tool result 通常是结构化 JSON，普通用户不会
  想直接放笔记里；error 也无意义。本轮专注 user / assistant 文字消息。
- 不附加 metadata（时间戳 / 角色前缀）—— 直接复制纯文本，让用户自己决定附什么。
- 不写 README —— 体验微调，与既有 panel 迭代同性质。

## 设计

### UX

- 复制按钮与气泡同行：
  - **assistant**（左对齐气泡）：按钮在气泡**右侧**，水平间距 6px。
  - **user**（右对齐气泡）：按钮在气泡**左侧**，水平间距 6px。
- 默认 `opacity: 0`；hover 整行（包括气泡 + 按钮区）时按钮 `opacity: 0.85`，
  hover 按钮自身时 1.0 + 微变色（`#0ea5e9`）。
- 已复制状态：1.5s 内文案改为「已复制」，按钮颜色变 `#16a34a`（绿）。
- 按钮极小：12px 字号、4×6 px padding、border 1px solid。
- 不破坏现有 `data-item-idx` / 跨会话搜索高亮 / `isHighlighted` 背景 —— 行级
  wrapper 保留这些属性。

### 实现

抽出本地 `CopyableMessage` 子组件，承担：
- props: `role: "user" | "assistant"`, `content: string`, `itemIdx: number`,
  `isHighlighted: boolean`, `copiedIdx: number | null`, `onCopy: (idx, text) => void`
- render: flex container（justify left/right by role），bubble + button 内容；
  CSS hover 选择器（`.pet-chat-row:hover .pet-copy-btn { opacity: 0.85 }`）控制
  按钮可见性，避免 React state-on-hover 抖动。
- 类名 `pet-chat-row` 与 `pet-copy-btn` 注入 panel 已有的 inline `<style>` 块
  （或新加一处）。

PanelChat 状态新增：
- `copiedIdx: number | null`（哪个 item idx 刚被复制了，用于显示「已复制」）
- `handleCopy(idx: number, text: string)`：调 `navigator.clipboard.writeText`，
  成功 → setCopiedIdx + 1500ms timer 清掉；失败 → console.error。

### 测试

前端无测试基础设施。视觉与交互靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `CopyableMessage` 子组件 + 状态 + handleCopy + CSS hover rule |
| **M2** | PanelChat 把 user / assistant 路径改用新组件 |
| **M3** | `pnpm tsc --noEmit` + `pnpm build` + TODO 清理 + done/ |

## 复用清单

- 现有 `bubbleStyle(role)` —— bubble 颜色 / shape 不动
- 现有 `wrapperBase(justify)` 行级样式（含跨会话搜索高亮背景）
- `data-item-idx` 属性留在 wrapper 上，跨会话搜索 scrollIntoView 不受影响
- `navigator.clipboard.writeText`（PanelDebug "看上次 prompt" 的复制按钮已用此 API，无新依赖）

## 待用户裁定的开放问题

- 移动端 / no-hover 场景如何处理？panel 是桌面 Tauri 窗口，无触屏；按钮永远
  hover 触发即可。如未来要做触屏适配再加 always-show toggle。
- 复制时是否带角色前缀（"AI: ..."）？本轮**否**，纯文本最通用，用户带前缀
  的需求自己手动加更可控。

## 进度日志

- 2026-05-05 10:00 — 创建本文档；准备 M1。
- 2026-05-05 10:20 — 完成实现：
  - **M1**：`PanelChat.tsx` 加 `copiedIdx: number | null` 状态 + `handleCopy` async callback（成功 setCopiedIdx 1.5s 自动清掉，按 idx 单条互不干扰）。在 panel root 的 `<style>` 块加 `.pet-chat-row .pet-copy-btn` hover-only 可见性规则（默认 opacity 0，hover 整行 0.85，hover 按钮自身 1.0 + 变色）。
  - **M2**：抽出 `CopyableMessage` 子组件，承载 user / assistant 两个分支：bubble + 复制按钮 flex 同行排列（user 反方向：按钮在 bubble 左侧，与右对齐互补避按钮被屏边挤压；assistant 按钮在 bubble 右侧）。`data-item-idx` 留在最外层 row 保留跨会话搜索的 scrollIntoView 锚点。`copied` 状态用 inline style `opacity: 1` 覆盖 CSS hover-only，确保 1.5s 反馈窗口内即便鼠标移开也可见。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 494 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— panel chat 体验微调，与既有迭代同性质。
  - **设计取舍**：CSS-driven hover 而非 React state-on-hover —— 后者会因每次 mouseenter / mouseleave 触发 setState 抖动；CSS 规则直接走浏览器，完全无重渲染。已复制态用 React state 是因为它跨"鼠标移开"事件持续 1.5s，纯 CSS 做不到；用 inline style 覆盖 CSS opacity 即可。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；CSS hover 规则与 clipboard API 在前端无单测基础设施下，由 tsc + 既有"看上次 prompt"复制按钮模式（PanelDebug 已用同 API）保证可用性。
