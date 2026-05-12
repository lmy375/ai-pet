# ChatMini ⌘F inline 搜历史消息

## 需求

桌面 mini chat 看历史时如果想找"刚才说的那条 X" —— 只能 / 双击进 panel 用
跨会话搜索 / 滚动找。在 mini 里直接 ⌘F 搜更快。

## 实现

`src/components/ChatMini.tsx`：

- 新 state：
  - `searchOpen: boolean`：搜索条显隐
  - `searchQuery: string`：keyword
  - `searchActiveHitIdx: number`：当前 active hit 在 hits 数组中的下标
  - `searchInputRef: HTMLInputElement`：autofocus 用
- 新 `searchHits = useMemo(...)`：visibleItems 内 text 含 keyword 的 idx
  列表，case-insensitive，空 keyword 返空数组
- 新 effect 1：hits 变化时 clamp activeHitIdx，避免 keyword / messages 变
  化让 active 指到不存在的 hit
- 新 effect 2：⌘F / Ctrl+F 全局快捷 → 打开搜索条 + setTimeout(0) focus
  input；preventDefault 防偶发默认行为
- 新 effect 3：searchActiveHitIdx 或 hits 变化时把目标 bubble
  scrollIntoView({ block: "center" })，并把 `followTailRef.current = false`
  —— 搜索期间用户主动跳到中间，下一帧 streaming 不应该把视图甩回底
- 新 `handleSearchInputKeyDown`：
  - Enter → next hit (`(cur+1) % len`)
  - Shift+Enter → prev hit (`(cur-1+len) % len`)
  - Esc → 关 + 清 keyword + reset active
- bubble 渲染：每条 bubble 加 `data-mini-idx={idx}`（scroll 选择器用）
  和条件 outline：
  - active hit → 2px 实线 `#f59e0b` + 0 3px rgba(245,158,11,0.25) 柔光圈
  - 普通 hit → 1px 虚线 `#fbbf24`
  - 非 hit → 无 outline
- 搜索条 JSX：位置 absolute, top:12, left/right:16，搜索框 + counter
  (`N/M` 或 `0` 红字提示无命中) + ✕ 关按钮。stopPropagation onMouseDown
  避免其它"outside-click 关 popover"路径误关

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 桌面 mini 可见状态下按 ⌘F → 搜索条浮在顶部，input 自动 focus
  - 输入 keyword → 实时高亮命中 bubble + counter "1/3" 等
  - Enter 跳下一条 → 颜色变 active；Shift+Enter 上一条；超末 / 首端循环
  - 无命中时 counter 显红色 "0"
  - Esc → 关 + 清空
  - keyword 改动 → active 复位到 0；hits 重算
  - input / textarea 聚焦时 ⌘F 仍触发（与 ⌘+C / Shift+G 不同 —— 搜索本
    身就是要从 input 输入，所以全局拦截更合理）

## 不在本轮范围

- 没做"bubble 内部 keyword 段 mark 高亮"（仅 bubble 外框高亮）：parseMarkdown
  返回的 ReactNode 树穿透改色比较繁；外框 + active 强对比已能视觉定位，
  bubble 文本内"具体哪几字命中"靠用户阅读
- 没做"持久化最近 keyword"：mini 搜索是短期意图，session 内打多次不必持
- 没做"多 keyword AND / OR"：单词 substring 命中即够；多词组合留给 panel
  的跨会话搜索

## TODO 池剩余

- PanelSettings 主题色 accent 自定义
- PanelMemory 一键导出 .md zip
