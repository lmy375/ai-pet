# PanelChat 输入框历史浏览态 hover 显 idx / total hint

## 背景

PanelChat 输入框支持 shell-readline 风 ↑/↓ 多条历史召回。owner 进入历史浏览态后看不到"我现在在第几条 / 还有几条可以再 ↑"。本 iter 浮一条 ambient hint 显当前 idx / total + Esc 退出提示。

## 改动

`src/components/panel/PanelChat.tsx` form 内（同 SlashCommandMenu 锚位）加 conditional hint：

```tsx
{historyCursor !== null && messageHistory.length > 0 && (
  <div style={{
    position: "absolute",
    bottom: "calc(100% + 4px)",
    right: 16,
    fontSize: 10,
    fontFamily: monospace,
    color: muted,
    background: card,
    border: ...,
    borderRadius: 4,
    padding: "2px 8px",
    boxShadow: sm,
    opacity: 0.85,
    pointerEvents: "none",
    zIndex: 5,
  }} aria-hidden>
    ↕ 历史 {historyCursor + 1} / {messageHistory.length} · Esc 退出
  </div>
)}
```

仅 historyCursor 非空时显；定位 form 顶 +4px，右靠齐避免遮 textarea 内容。muted 10px ambient style。

## 关键设计

- **历史模式自动 gate**：`historyCursor !== null` 即"owner 按 ↑ 进入" 时显；free typing / 退出后不显。
- **idx + 1 / total 一基显**："2 / 17" 比 "1 / 16" 直观。
- **Esc 退出 hint**：与既有 ArrowDown <0 退出对偶；让 owner 知道有快速退出。
- **pointerEvents none + ambient style**：不挡 textarea / send button click；与 SlashCommandMenu 共形态不冲突。

## 不做

- **不绑 Esc 直接退出**：既有 ArrowDown 路径已支持；Esc 留给 slash menu / 其它面板。
- **不写测试**：纯 conditional render；视觉验证（textarea 按 ↑ → 顶应显 hint）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.20s
- 改动 ~30 行（hint div + 注释）。既有 historyCursor / messageHistory / ↑↓ 路径 / SlashCommandMenu 完全不动。

## TODO 状态

剩 2 条留池：
- PanelMemory item description 行级 hover preview 含完整内容
- detail.md 编辑器 ⌘K 唤起 task quick-find palette

## 后续

- hint click → cycle 到首条历史（让鼠标 owner 也能跳）。
- 历史模式高亮 textarea border accent 色让 owner "我在浏览不是 free typing" 更明显。
