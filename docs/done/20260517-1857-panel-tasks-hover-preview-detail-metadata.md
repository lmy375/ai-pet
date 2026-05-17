# PanelTasks 行 hover preview 加 detail.md 字数 + 上次编辑时间 chip（iter #323）

## Background

PanelTasks 行 hover preview 已显 priority / due / tags chips + recent history
+ detail.md preview body。但缺"detail.md 多大 / 最后改了多久前"的元数据 —
owner hover 时想"该展开看吗 / 这条新鲜不必重看 / 已经很久没动了？" 这种
决策信号当前要展开详情 + 看字数计数器 / 看 history 时间戳才能拼凑。

本迭代在 hover preview chips 行加「📝 N 字 · X 前」一个 chip — detail.md
字数 + 上次编辑相对时间合并显，给 owner glance 决策信息。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- hover preview IIFE 内新计算 locals：
  - `detailCharCount` = `Array.from(pd.detail_md).length`（按字形计数；
    emoji / 中文 / surrogate pair 统一 1 字 — 与 PanelMemory detail size
    chip 同算法）
  - `detailEditedRel` = `formatRelativeAge(pd.updated_at, nowMs)`（既有
    helper：N 分钟前 / N 小时前 / N 天前）
- `hasChips` 计算扩 `|| detailCharCount > 0` — 让仅有 detail metadata
  时也触发 chips 行渲染（不必有 priority / due / tags）。同时既有
  早 return null 也跟着调整：`detailSnippet.length` 已经 gate 了 detail
  存在，与新 chip 一致。
- chips 行末尾（t.tags.map 之后）插「📝 N 字 · X 前」chip：
  - 仅 `detailCharCount > 0` 时渲染
  - muted bg + muted color 与 priority / due / tags chip 同 visual hierarchy
  - tooltip 含完整文案 + "未知" 兜底（updated_at 不存在 / 解析失败时）

## Key design decisions

- **合并字数 + 时间为一个 chip 而非两个**：让 hover chips 行不变臃肿。
  二者语义相近（都是"detail.md 元数据"），合并后单 chip 携带 "size +
  age" 双维信号。
- **char count 用 `Array.from(s).length`**：与 PanelMemory detail size /
  detail editor 字数 chip 同算法 — emoji / 中文 / surrogate pair 统一按
  字形计数。`pd.detail_md.length` UTF-16 code unit 对中文会算 1（与字形
  同）但对 emoji 算 2（surrogate pair），不一致。
- **复用 `formatRelativeAge` helper**：与 itemMeta "创建于" / 任务行
  "📅 N 前" / "🕰 拖了 N 天" hint 等都同源 — 让 owner 在 panel 各处看
  到的"相对时间"文案完全一致。
- **muted color 而非 priority chip 那种 fg**：detail metadata 是 info
  维度（不像 priority / due 是 actionable 决策），视觉分量降一级让
  actionable chips 仍占注意力。
- **不显 detail file mtime 而是 task updated_at**：task updated_at 在
  description / detail 任一改动时自动推进；detail file mtime 需 stat IO
  + 不一定可靠（trash / iCloud sync 等会改 mtime）。task 层 updated_at
  是 backend 写盘前 set 的内部时刻，更稳定。
- **不引入 unit test**：纯 JSX 渲染 + 已存在 helper 复用；行为通过 vite
  build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.27s)
