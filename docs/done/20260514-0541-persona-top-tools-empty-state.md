# PanelPersona top tools 空态用 `EmptyState`

## 背景

PanelPersona「最近常用的工具」section 在 `topTools.length === 0` 时渲染裸 `<p>` italic 提示。其它 panel（PanelChat / PanelTasks / PanelMemory）的空态都接入了共享 `EmptyState` 组件。这里是漏网之鱼，与上轮 PanelChat 空态升级同模式。

## 改动

`src/components/panel/PanelPersona.tsx`：

- import `EmptyState` from `./EmptyState`
- 把 `<p>还没动过手...</p>` 换成：

  ```tsx
  <EmptyState
    icon="🛠"
    title="还没动过手"
    hint="等下次开口里 LLM 调工具就会出现在这。"
    compact
  />
  ```

`compact` 让 padding 减半，section 卡片不会被空态撑得太高。

## 不做

- 不动其它 PanelPersona 块（self-summary 空态等）：那些有 inline editor 不止是文案；本轮只动纯展示型空态
- 不抽测试：纯 view

## 验收

- `npx tsc --noEmit` ✅
- 「人格」tab "最近常用的工具" section，无 tool_call_history 数据时显大图标 🛠 + 标题 + 提示，节奏与其它 panel 空态一致

## 完成

- [x] PanelPersona.tsx: 替换 inline 空 `<p>` 为 EmptyState
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
