# PanelSettings API key 显示掩码 + 复制按钮

## 需求

LLM 配置区的 API Key 输入虽然已是 `type="password"`（默认 •••••），但：

1. 用户想确认自己粘的 key 对不对，只能"全选 + 改 type 到 text + 看 + 改回
   来"，没有就地可见的 reveal 控件
2. 想把 key 复制到另一台机器 / 工具时，"全选 + ⌘C"会因为浏览器先把
   selection 渲成明文一瞬，被录屏 / 截图捕获

加：

- 👁 按钮：长按显示（mouse 松手 / 鼠标离开按钮 / touch 抬起即重新掩码）—
  比 toggle 防泄漏，不会"忘按"
- 📋 按钮：直接走 `navigator.clipboard.writeText` 复制，避开 selection
  明文

## 实现

`src/components/panel/PanelSettings.tsx`：

- 新 state `apiKeyVisible: boolean`，默认 false
- 原 `<input type="password">` 包进 `<div flex gap=6>`，input 加 `flex: 1`
- input.type 算成 `apiKeyVisible ? "text" : "password"`
- 加 👁 按钮：
  - onMouseDown / onTouchStart → setApiKeyVisible(true)
  - onMouseUp / onMouseLeave / onTouchEnd → setApiKeyVisible(false)
  - 按住状态加黄色 tint 让用户感知"现在你在显"
  - 空 key 时 disabled + cursor:not-allowed
- 加 📋 按钮：
  - onClick → clipboard.writeText(form.api_key) + setMessage 3s 反馈
  - 空 key disabled
  - 文案"已复制 API key 到剪贴板（N 字符）" —— N 字符让用户能粗校 key 长度

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 入页：API key 显 ••••• 不变
  - 按住 👁：输入框瞬时变 sk-... 真实文本；松手 / 鼠标移出按钮 → 立即回到 •••
  - 空 key 时 👁 / 📋 都灰 + 鼠标 cursor 禁止
  - 点 📋：toast"已复制 API key 到剪贴板（N 字符）"3s 自清
  - 复制失败（罕见，clipboard 权限被拒）：toast"复制失败：..."

## 不在本轮范围

- 没改 import / export snapshot 的 API key 部分：那已有 securityNotice 红
  banner 提醒用户审核明文，单点加密成本高（要 keyring / 用户口令派生）
- 没改 telegram bot token 字段的掩码：同模式可以照搬，但本轮只针对 OpenAI
  API key（最常用泄漏面）；后续若有 token 类字段统一加一个 `<SecretInput>`
  组件再回头重构

## TODO 池剩余

- ChatMini 流式时图标小动效
- PanelMemory 单条记忆 pin 置顶
- ChatMini 拖拽到面板的过渡视觉
- PanelTasks 卡片"按住拖拽改 priority"
