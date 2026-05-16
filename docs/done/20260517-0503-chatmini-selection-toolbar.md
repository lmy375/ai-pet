# ChatMini 选区浮 mini toolbar（iter #251）

## Background

ChatMini 内已经有 hover bubble 时浮的「💭 针对这条问」/「💾 转 task」/
「📋 复制」按钮，但这些都按"整条 bubble"粒度操作。owner 想拿"宠物刚说的
某段 N 字"是高频场景 — 比如 LLM 输出一段日程总结，owner 只想把其中"明天
14:00 开会"这一句存为 task，或把它复制 / 让 AI 重写更精炼。

本迭代加选区浮 toolbar：在 chat 列表区拖选文字 → 释放鼠标 → toolbar 浮在
选区上方（视口空间不够则浮下方），3 个动作：
- 💾 转 task — 把选中字作为 task body 走既有 `onSaveAsTask` 跨窗口 pipeline
- 📋 复制 — `navigator.clipboard.writeText` + 1.5s ✓ 反馈
- 🔄 改写 — dispatch `pet-mini-rewrite-selection` 事件 → ChatPanel prefill 输入框

## Changes

- `src/components/ChatMini.tsx`：
  - 新增 `selectionToolbar: { text, x, y } | null` state + `selectionCopyOk`
    1.5s ✓ 反馈状态
  - useEffect 挂 mouseup / selectionchange / Esc / scroll 监听：
    - **mouseup**：等 selection settle 后跑 computeToolbar；用 `setTimeout(0)`
      不阻塞事件 + 让 selectionchange 先 settle
    - **selectionchange**：选区被清掉时关 toolbar（点空白 / 双击单词后变成
      点单字 / 输入框聚焦等）
    - **Esc**：键盘关 toolbar
    - **scroll**：chat 滚动时关 toolbar 防卡屏
  - selection 命中要求：`scrollRef.current?.contains(range.commonAncestorContainer)`
    —— 保证仅在 chat 列表区域内的选区才浮（输入框 / 顶部 chip 区域选区不弹）
  - toolbar UI：fixed 定位，viewport-clamp 防超边缘；上方空间不够（≤ 4px）
    时翻到选区下方；3 个按钮，💾 仅在 onSaveAsTask 传入时显（与 hover bubble
    按钮同 gate）

- `src/components/ChatPanel.tsx`：
  - 新增 useEffect 监听 `pet-mini-rewrite-selection` 事件：把选中文字以
    `请改写：\n\n[text]` prefill 到输入框（覆盖现有内容 —— 改写动作语义足够
    强，prefix-保留旧文会污染请求）
  - prefill 后聚焦 textarea + 光标落末尾让用户能立即调整 prompt 后发送

## Key design decisions

- **mouseup 而非 selectionchange 触发显示**：selectionchange 在拖选过程每帧
  都触发，每帧重定位 toolbar 跟手感差且 flicker。mouseup 让选区 settle 后再
  一次性算位置最自然（与 macOS / iOS 内置选区菜单同模式）。
- **selectionchange 仍挂上但只用来"清空时关"**：保证 owner 点空白后 toolbar
  不卡在屏幕上；不参与显示触发。
- **改写 prefix `请改写：\n\n` 而不是 `关于「...」`**：响应改写动作的语义—
  让 LLM 知道"以下是要改写的原文"，而 `关于「...」` 是 follow-up question
  锚点。两条 prefix 走两个事件，互不串扰。
- **改写动作覆盖现有输入**：与「💭 针对这条问」的 prefix-保留行为不同 ——
  改写场景下"我想发的是这段重写后的版本"，旧输入不再相关；保留反而会被一
  起塞进 prompt 让 LLM 困惑。
- **viewport clamp + 上下翻转**：toolbar 132×32 px 较小，但 chat 列表顶部
  附近的选区（如 streaming 首字）会让 aboveY 落到 4px 以内；翻到选区下方
  保证 toolbar 不被裁切。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.15s)

## Notes

`pet-mini-rewrite-selection` 事件仅在 ChatPanel 内 visible / 桌面 pet 窗口
有效；panel 内 PanelChat 走不同 input pipeline，本迭代未为其挂监听 —— pet
窗口 ChatMini 是高频选区场景，panel chat 自身已可框选 + ⌘C 复制 + 拖到任
意 textarea 自行 paste。
