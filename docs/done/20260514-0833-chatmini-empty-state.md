# ChatMini 空态用 `EmptyState`

## 背景

PanelChat 空态上轮已经接到 EmptyState。ChatMini（pet 窗的迷你聊天列表）首次启动 / 全部 ✕ dismissed 后是个**纯空 div**，没有给用户任何指引：底部有输入框，但用户可能不知道宠物会主动开口、或者怎么打开面板。

加 compact EmptyState 一行 hint。

## 改动

`src/components/ChatMini.tsx`：

- import `EmptyState`
- 在 `visibleItems.map` 之前加：

  ```tsx
  {visibleItems.length === 0 && !currentResponse && (
    <EmptyState
      icon="🐾"
      title="等宠物开口"
      hint="底部输入框敲字开始聊天；宠物也会在 proactive 时主动找你。"
      compact
    />
  )}
  ```

- `currentResponse` 守门：当 LLM 正在 streaming 但还没首个 token 时也不显空态（避免一闪而过）。

`compact` 模式 padding 较小，与 pet 窗 ~300px 宽度匹配。

## 不做

- 不显历史会话提示：pet 窗没历史会话列表概念（panel 才有）
- 不动 mini chat 主体样式

## 验收

- `npx tsc --noEmit` ✅
- 全新启动 pet 窗 → mini chat 区显 🐾 + "等宠物开口" + 输入引导
- 发一条消息或 LLM 主动开口 → 空态消失

## 完成

- [x] ChatMini.tsx: import EmptyState + 空态条件渲染
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
