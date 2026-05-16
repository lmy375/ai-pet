# PanelTasks 行内 tag chip tooltip 显「同 tag 负载」（iter #257）

## Background

PanelTasks 行内 tag chip（如 `#工作` / `#家务`）已支持 click 过滤 / 双击改名
/ 右键调色。但 owner 看 task 列表时常想知道"这个 tag 总共有多少 task / 还在
进行的有多少"以掂量负载 —— 是 #工作 这个 tag 下还有 8 条 pending 让人焦虑，
还是其实只剩 1 条快做完。

当前唯一获取此信息的路径：点击 tag chip 过滤 → 看顶部 visibleTasks 数。但 owner
只是 hover 想"扫读"，不想破坏当前的过滤态。本迭代把同 tag 负载（total /
pending）放进 chip 的 tooltip，hover 即可看到。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **`tagLoadMap` useMemo**：从 tasks 全集派生 `Map<tag, { total, pending }>`：
  - `total`：所有 status 的 task 数（含 done / cancelled）
  - `pending`：仅 `!isFinished(t.status)` 的 task 数（pending / error）
  - 依赖 `[tasks]`，与 `allTags` 同 trigger

- **tag chip `title` 改造**：原 tooltip 在前面拼一段 `📊 同 tag 总 N 条 · 未结束 M · `
  前缀；保留既有"点击 / 双击 / 右键"三段操作说明。负载未命中时（罕见 race，
  如本 task 持有的 tag 在 tasks 列表 stale 期间不存在）退化到无前缀 tooltip。

## Key design decisions

- **走 tooltip 而非可见 hint**：tag chip 已是密集元素（多 tag 时占满行）；外露
  数字会让 chip 视觉拥挤。tooltip 是"有问才查"的密度模式，owner 不 hover
  就当不存在，hover 即看到。
- **派生自 tasks 全集而非 visibleTasks**：让 owner 在 filter 后 hover chip
  仍看到该 tag 的全局负载（"虽然我现在过滤掉了 #家务，但本表里 #工作 还有
  N 条等我处理"）。与 `allTags` chip 行的派生策略一致。
- **total + pending 两段，不显 done**：`done` 数 = `total - pending`，owner 可
  自己心算；展示二选一保 tooltip 简洁。pending 更贴"还要做多少"的诉求。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)

## Notes

同时 drop 了一条 stale TODO（detail.md 复制全文 + meta 按钮 — 既有的
"📤 复制 LLM consume 段" 按钮已能复制完整任务 markdown 含 detail
内容 + 元数据 bullet，新增同效按钮冗余）。
