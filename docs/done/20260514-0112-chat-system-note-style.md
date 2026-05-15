# PanelChat 系统反馈消息与 LLM 消息视觉区分

## 背景

`pushLocalAssistantNote` 把 `/clear armed` / `/done 已标` / `/stats 汇总` / `/help 命令清单` / `mute 已解除` 等"本地系统反馈"作为 `type: "assistant"` 推到 items —— 视觉上与真的 LLM 输出一模一样。

后果：聊几条命令后，会话历史变成"AI 主动播报命令结果"的样子，分不清哪是宠物真说的、哪是 panel 自己反馈的。`/help` 之类多行命令清单尤其抢眼，污染对话的人感。

## 改动

### 数据层：`ChatItem.systemNote`

`src/components/panel/panelChatBits.tsx`：

- `ChatItem` 加 `systemNote?: boolean` 字段（缺省 false）

### `bubbleStyle` 加 `subdued` 参数

`bubbleStyle(role, subdued?: boolean)`：当 `subdued === true` 渲染一套抑制版样式：
- 字号 13px（vs 14）
- 行高 1.5（vs 1.65）
- 字色 `var(--pet-color-muted)`（vs 主 fg）
- background `color-mix(in srgb, var(--pet-color-card) 60%, transparent)` —— 半透明 hint
- border `dashed`（vs solid）—— 一眼区分"不是 AI 真说"
- 无 boxShadow（vs shadow-sm）—— 不"立体"，更像 inline 注释

仅 `role === "assistant"` 时认 subdued；user 端从来不会 systemNote，无需分支。

### `pushLocalAssistantNote` 设 systemNote

`PanelChat.tsx`：

```ts
const pushLocalAssistantNote = useCallback((text: string) => {
  setItems((prev) => [...prev, { type: "assistant", content: text, systemNote: true }]);
}, []);
```

### 调用方传 subdued

PanelChat 的 CopyableMessage 渲染处：把 `item.systemNote` 透传给新增的 `subdued` prop，CopyableMessage 内部根据它决定 `bubbleStyle(role, subdued)`。

### 导出排除

`exportSessionAsMarkdown` 加一行过滤：跳过 `it.systemNote === true`。系统反馈不该出现在用户分享给同事 / 归档到 markdown 日志的内容里。

### 类型 + 默认值

ChatItem 不存在的旧 session 加载回来 → `systemNote === undefined` → 走 `!!` 判定为 false，老数据视觉不变。

## 不做

- 不动 user bubble：systemNote 仅出现在 assistant 路径
- 不去掉 systemNote 行的 📋 copy 按钮：用户偶尔可能想复制"已标 done"的反馈作为操作记录，留口
- 不持久化"哪些是 systemNote"的列：与 session JSON 同步存（已有字段集兼容增量）；老 session 没标记即视为非 systemNote
- 不删除 `tasksHelpers` 抽 helper（无关本次）

## 验收

- `npx tsc --noEmit` ✅
- 桌面 `/help` → 命令清单 bubble 出现，但视觉明显比 LLM 真回复更柔（字小、虚线边、半透明）
- 真 LLM 回复 → 仍是正常 assistant 样式
- 复制会话 markdown → 不含系统反馈那几行

## 完成

- [x] ChatItem.systemNote 类型字段
- [x] bubbleStyle 接 subdued 参
- [x] CopyableMessage 接 subdued prop（透传到 bubbleStyle）
- [x] PanelChat: pushLocalAssistantNote 写 systemNote=true + 调用处传 subdued
- [x] exportSessionAsMarkdown 过滤 systemNote
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
