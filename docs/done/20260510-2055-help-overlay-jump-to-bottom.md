# 三件套：? 帮助层 / ChatMini 跳到底浮标 / Memory 计数 chip 验证

> 对应需求（来自 docs/TODO.md）：
> 1. 全 Panel `?` 键弹快捷键帮助层。
> 2. PanelMemory 各类目标题旁加 item count 小 chip。
> 3. ChatMini 加「跳到最底部」浮标按钮。

## 1. 键盘快捷键帮助层

新文件 `src/components/panel/KeyboardHelpOverlay.tsx`：

- 模态层，背景半透明遮罩，点背景或按 Esc 关闭。
- 展示分组：「Panel 全局」/「任务 tab」。每条带 keys chip + 中文描述。
- 维护提示：列表是事实源（代码层）的镜像，新增快捷键时回填本表。

`PanelApp.tsx`：

- 新 state `showKeyboardHelp`。
- 全局 `?` keydown 监听唤起（input/textarea/select focus 时跳过避免吞输入）。
- Tab bar 加「?」按钮，与「调试 ↗」/ 主题切换并列；title 提示也可按 ?。
- 在最外层渲染 `<KeyboardHelpOverlay visible={...} onClose={...} />`。

## 2. ChatMini 跳到底浮标

`src/components/ChatMini.tsx`：

- 新增 `followTailRef` (ref 给 effect 用) + `notAtBottom` state (给浮标可见态)。
- onScroll 计算 `distFromBottom`，超过 8px 阈值 → notAtBottom=true，followTail=false。
- 自动滚到底 effect 仅在 followTail=true 时执行 —— 用户向上读旧消息不再被强行拉回。
- 浮标按钮：右下角圆形 ↓，仅 notAtBottom 时渲染。点击 → 滚到底 + 重置 followTail。
- 流式 / 新消息到达：仍贴底则照旧自动滚动；离底则不打扰（用户能看到 ↓ 浮标）。

## 3. Memory 类目计数 chip（无代码改动）

PanelMemory.tsx 第 816 行已有 `<span style={s.badge}>{cat.items.length}</span>`，
本轮回头确认 —— 任务实际上之前已完成。该需求不需要新代码。

## 验证

- `tsc --noEmit` 干净。
- `vite build` 干净。
