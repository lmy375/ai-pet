# PanelChat 复制按钮 alt-click 保留 ref token

## 需求

iter #178 引入 `「task title」` ref token 让 ⌘K picker 把任务引用插
入 chat。但用户复制消息到外部（Slack / 邮件 / 普通 IM）粘贴时，全
角直角引号是宠物内部约定的标记，外部看着突兀。让普通点 = 去装饰
（"复制干净文本"），⌥/Alt 点 = 原样保留（"复制 markdown 含 ref"）。

## 实现

`src/components/panel/panelChatBits.tsx`：

- `CopyableMessage` 的 `onCopy` 签名加第三参 `asMarkdown: boolean`
- 复制按钮 `onClick` 改 `(e) => onCopy(itemIdx, content, e.altKey)`
- button title tooltip 改成两行说明：默认 "去掉「」引用装饰" + ⌥/Alt
  保留原始 markdown

`src/components/panel/PanelChat.tsx`：

- `handleCopy` 加 `asMarkdown` 参；非 markdown → 用 `/「([^「」]+)」/g`
  把 ref token 剥成纯标题（捕获组 $1 替换整个 match）
- markdown 路径原样 writeText（含全部 「」）

行为参数化为一处替换，无新 state、无新组件。

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 消息含 `已经处理了「整理 Downloads」，移走 38 个文件。`
    - 普通点 "复制" → 剪贴板 = "已经处理了整理 Downloads，移走 38 个文件。"
    - ⌥/Alt+点 → 剪贴板 = 原文（含 「」）
  - 无 ref token 的消息 → 两种点击行为等效
  - 嵌套 / 异常括号（如 「a「b」c」）→ 内层先匹配 → 输出 `a「bc」`
    （已知 limitation，实际 LLM / picker 不产此形）
  - 复制成功后 1.5s 显 "已复制" → 同既有 UX

## 不在本轮范围

- 没做 right-click 菜单提供更多复制 mode（"含 history" / "含 detail"
  等）：本轮聚焦 ref token 装饰一项；复杂多 mode 等用户提需求再做
- 没改三键反馈 reaction 按钮：那条路径不复制消息，与 ref 装饰无关
- 没做"全局开关偏好"（用户希望永远 strip 或永远 keep）：通用 UX 是
  默认去装饰 + 高级用户用 Alt，与 OS 复制行为（普通 vs Ctrl/⌥ 修
  饰）习惯一致

## TODO 池剩余

- PanelDebug 加 "上次 manual fire" 行
- PanelChat 双击 `「task title」` ref → 切到 PanelTasks tab + scroll
