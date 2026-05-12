# ChatMini bubble 单条复制按钮

## 需求

桌面 mini chat 只有顶部"📋 复制最近 N 条"批量入口，想复制单条历史消息得
先 select 文本再 ⌘C，但 mini 字小 + 多个 bubble 选段易跨条。加 hover-only
单条复制按钮，与 PanelChat 同模式。

## 实现

`src/components/ChatMini.tsx`：

- 既有 `MINI_CHAT_STYLES` 块加 `.pet-mini-row .pet-mini-row-copy` 一组 CSS：
  - 默认 `opacity: 0`，行级 hover 升 0.7，自身 hover 升 1 + accent 色
  - 与 PanelChat `.pet-chat-row .pet-copy-btn` 同视觉模式
- 新 state `bubbleCopyIdx: number | null` + `handleBubbleCopy(idx, text)`：
  - 复制成功设 idx → 1.5s 后自动清（条件清防别的条复制后误清当前的）
  - 与既有 `copyToast`（⌘+C 复制最近一条快捷）分离，各管各的反馈
- visibleItems.map 内：
  - row wrapper 加 `className="pet-mini-row"` 让子按钮的 hover-only CSS 生效
  - row wrapper 多了 `alignItems: "flex-end"` + `gap: 3` 让按钮与 bubble 底
    对齐
  - 算 `copyBtn` 节点：仅 `text` 非空时渲染（纯图片 bubble 不显，图片走 lightbox
    的复制路径）
  - user 行：`{copyBtn}{bubble}` —— 复制按钮在 bubble 左侧
  - assistant 行：`{bubble}{copyBtn}{like-btn-block-if-any}` —— 在 bubble 右
    侧，紧贴 bubble；既有 👍 反馈块仍 absolute top:-4 right:0
  - 已复制态 inline `opacity: 1` + `color: #16a34a` + 图标换 `✓`，1.5s 后自
    动回 📋

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - hover 任一 bubble → 📋 按钮弱可见
  - 点 📋 → 写剪贴板成功 → 按钮变绿 + ✓ 1.5s → 回灰 + 📋
  - 纯图 bubble（text 空）→ 无按钮
  - user 行按钮在左、assistant 在右 → 不挤窗口边
  - 与既有"最新 assistant 👍"按钮共存 → 两个 absolute / flex 元素互不重叠
  - 顶部"复制最近 N 条"菜单照常工作

## 不在本轮范围

- 没做"复制带角色前缀 / 时间戳"：单条复制就给纯文本，与批量复制的拼接策
  略区分（批量是 markdown segment，单条是 plain text 方便贴到任何地方）
- 没改 ChatMini 流式 bubble 加按钮：streaming 中文本还在变，没必要复制半截
- 没在 mini 加图片单独复制：ImageLightbox 已有图片复制路径，与 bubble copy
  正交

## TODO 池剩余

- PanelChat 顶部 session 横排 tab-like 标签栏
- PanelDebug LLM 日志多 chip 过滤
- PanelTasks origin 过滤 chip
- PanelSettings motion mapping 即点触发预览（注：实测当前代码已有逐项 ▶ 试一下；后续可考虑加"全部演示一遍"按钮）
