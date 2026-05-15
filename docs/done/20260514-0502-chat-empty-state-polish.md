# PanelChat 空会话状态用 `EmptyState` 组件

## 背景

新会话打开时，items 为 0、currentResponse 也空 → 渲染一行裸 div：

```
开始聊天吧~
```

字号 14px 灰字。已经够"友好"但**信息密度太低**：新用户不知道有 slash 命令、不知道 `@` 可引用任务、不知道 `Shift+Enter` 可换行。EmptyState 组件已用于"暂无历史会话"等场景，把这条也接进去顺便发现性 +1。

## 改动

`src/components/panel/PanelChat.tsx`：

把：

```tsx
{items.length === 0 && !currentResponse && (
  <div style={{ textAlign: "center", color: "var(--pet-color-muted)", marginTop: "40px", fontSize: "14px" }}>
    开始聊天吧~
  </div>
)}
```

换成：

```tsx
{items.length === 0 && !currentResponse && (
  <EmptyState
    icon="💬"
    title="新会话，开始聊天吧"
    hint="敲 / 看快捷命令 · @ 引用任务 · Shift+Enter 换行"
  />
)}
```

3 个简短 hint 用 `·` 分隔在一行里，覆盖 PanelChat 三个最常被新用户忽略的快捷路径：slash 命令 / mention picker / 多行输入。

## 不做

- 不加 children action 按钮（"试试 /help" / "切到任务 tab"）：button 与 textarea 在视线里重复触发点 —— 用户本来要打字，再多个按钮显得"该点这里别点那里"歧义
- 不区分新用户 / 老用户：用一份 hint 即可；老用户视而不见
- 不复用 compact mode：会话主区视野大，full mode（更大 icon / padding）让"新会话"的过渡更自然

## 验收

- `npx tsc --noEmit` ✅
- 新建会话 / `/clear` 后：空 chat 区显大 💬 + 标题 + 一行 hint，与"暂无历史会话"等空态风格一致

## 完成

- [x] PanelChat.tsx: 替换 inline div 为 EmptyState
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
