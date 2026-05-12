# /image 占位行加重试按钮

## 需求

`/image` 失败（API key 错 / 余额不足 / prompt 被 policy 拒 / 网络抖一次）时，用户要么重打整条命令，要么从历史里 ↑↑ 翻出原 prompt 再发。占位行直接挂个🔄重试按钮能把这个流程压到一次点击。

## 实现

### ChatItem 扩字段

`panelChatBits.tsx` 里 `ChatItem` 加可选 `imageRetryPrompt?: string`。失败行专用，UI 看到此字段就渲染重试按钮；成功行不写。

### 抽出可复用的 `runImageGenerate`

把原来塞在 `executeSlash` `case "image"` 里的生图逻辑抽成 component-scope 的 useCallback：

```ts
runImageGenerate(prompt: string, replaceAtIdx: number)
// replaceAtIdx >= 0 → 替换该 idx 处 item 为 pending；否则 append
// 完成时找 pendingNote（引用相等 OR content 相等）替换为成功/失败行
// 失败行带 imageRetryPrompt = prompt
```

`/image` 命令路径：push user echo → `runImageGenerate(prompt, -1)` 走 append 分支。
重试路径：点击按钮 → `runImageGenerate(retryPrompt, i)` 直接替换失败行为 pending → 重跑。

### 渲染

assistant render 分支多一个前置判断：`item.imageRetryPrompt` 非空时不走 `CopyableMessage`，而是渲染一个橙色错误 bubble + 旁边的🔄重试按钮。重试按钮 `onClick={() => runImageGenerate(retryPrompt, i)}`，触发后失败行被替换为 pending，imageRetryPrompt 字段消失，按钮自然不再渲染（state-driven 防双击）。

### 函数顺序

`runImageGenerate` 必须定义在 `executeSlash` 之前 —— `executeSlash` 的 deps 数组要引用它（const TDZ）。`saveCurrentSession` 已经定义在更早，无环。

## 验证

- `npx tsc --noEmit` clean
- 失败行交互：点 🔄 → 立刻变 pending 占位 → 走完整流程；成功 → 渲染图片；再失败 → 错误 + 🔄（无限重试可用）

## 不在本轮范围

- ChatMini 渲染用户图片（#42 — 唯一剩余 TODO）
