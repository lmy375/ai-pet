# PanelMemory 顶部搜索框 keyword 历史下拉

## 需求

PanelMemory 搜索框是 free-form text input，用户搜过的 keyword 再搜
要重敲。给一个最近 5 个历史 dropdown，省手敲。

## 实现

`src/components/panel/PanelMemory.tsx`：

- 新 state `searchHistory: string[]`，localStorage 持久，挂载时 read
- `pushSearchHistory(kw)` helper：trim 空校验 + 去重 + cap 5 + 写盘；
  仅在 `handleSearch` 成功路径调（误敲清空不污染历史）
- 搜索 input 加 `list="pet-memory-search-history"` 属性 —— 触发 native
  `<datalist>` 自动完成下拉
- 在 input 旁渲一个 `<datalist id="pet-memory-search-history">`，
  options 来自 searchHistory；空 history 不渲染（datalist 无 option =
  浏览器不浮）

用 native datalist 而非自己写 popover：
- 无需 outside-click 关闭 / 上下键 nav / autoComplete 集成 —— 浏览器
  自带，WKWebView 支持稳定
- 单 DOM 节点 + map options，无新 popover 状态机
- 用户敲字也会过滤历史（每个 option 实时模糊匹配 input）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 全新 panel：localStorage 空 → datalist 不浮，input 行为同既往
  - 搜索 "todo" → Enter 成功 → searchHistory 加 ["todo"] 写盘
  - 再敲一字 focus input → 浏览器弹下拉显 "todo" 一项
  - 搜过 5 个 → 第 6 个挤掉最旧的（FIFO 5-cap）
  - 同 keyword 重复搜 → 去重，移到首位（recency-bias）
  - 重启 panel → 历史还原
  - localStorage 损坏 / 私密模式 → 空 history 退化（不阻塞 search）

## 不在本轮范围

- 没做"清除历史"按钮：场景边际；用户清 localStorage 即清
- 没做自定义 popover style（datalist 渲染受浏览器原生样式约束）：
  统一 native 体验 > 视觉精雕；与 OS 输入框自动完成感观一致是好事
- 没做跨 window 同步：localStorage 已是跨 window 共享，多 window 写
  操作天然同步 reading；不需额外 emit / listen

## TODO 池剩余

- PanelDebug 加 "重置 in-process stash" 按钮
