# `@` mention 选单加键盘提示 footer

## 背景

PanelChat 的 `@` 任务引用浮窗（mention picker）和 SlashCommandMenu 结构同款 —— 列表 + 可选 empty state。上轮给 SlashCommandMenu 加了 `↑↓ 选 · Enter 执行 · Tab 补全 · Esc 关` footer，mention picker 还没有。

补齐，让两套浮窗的键盘提示节奏一致。

## 改动

`src/components/panel/PanelChat.tsx`：

在 mention 列表（`mentionFilteredTasks.map(...)`）下方追加一个 Fragment 包住列表 + footer：

```tsx
<>
  {mentionFilteredTasks.map(...)}
  <div style={footerStyle}>
    <span>↑↓ 选</span>
    <span>Enter / Tab 引用</span>
    <span>Esc 取消</span>
  </div>
</>
```

footer 样式与 SlashCommandMenu footer 完全一致（borderTop / muted color / mono font / 10px）。

empty-state 分支（"没有匹配 / 没有任务可引用"）不挂 footer —— 1 行 hint 本身已包含 Esc 提示，再加 footer 会比内容还高。

## 不做

- 不抽 footer 公共组件：两处用 hardcoded 文案差异够大（"执行" vs "引用"），抽出 props 比内联 7 行还啰嗦
- 不动 mention 列表行渲染

## 验收

- `npx tsc --noEmit` ✅
- 聊天输入框敲 `@xxx` → mention picker 浮出，底部多一行 keyboard 提示
- 敲 `@<不存在>` → 仍是原 empty state hint 行（无 footer）

## 完成

- [x] PanelChat.tsx: mention 列表段加 Fragment 包裹 + footer
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
