# PanelMemory item「📊 7d updates」chip — 范围外 pivot drop（iter #565）

## Discovery

TODO 提案：「PanelMemory item hover「📊 7d updates」chip：扫本 item
updated_at 算近 7 天有几次 update — 单 item 活跃度」。

实际数据 substrate 不支持「次数」语义：

- `MemoryItem` struct 只有单一 `updated_at: String`（最后 update 的
  ISO timestamp）— 不是历史列表
- 每次 `memory_edit("update")` 覆盖 `updated_at` — 之前的 update ts
  被丢
- 「N 次 update」需要 per-item history log，单 item field 无法导出

## 现有可用历史源

1. **`detail_history.rs` `.history/`**: 每次 `memory_edit("update")`
   写 detail_content 前 snapshot 旧版本到 `<detail_path>.history/
   <YYYYMMDD-HHMMSS>.md`。但：
   - cap = 5（HISTORY_CAP）— 7d 内若 ≥ 6 次 update 会 undercount
   - 仅对 detail_content 改动触发；只改 description 不 snapshot
   - 仅对有 detail_path 的 item 起效（部分 cat item 可能无 detail）
2. **`butler_history.log`**: 每次 `memory_edit` 记录一行；80 字截断。
   但仅对 `butler_tasks` cat 工作 — 其它 cat 的 memory_edit 也写但当
   前 butler_history.summarize 仅过滤 butler_tasks
3. **SQLite mirror**: butler_tasks / todo / task_archive 段被 SQLite
   覆盖 (memory_list 读路径) — 但 mirror 也只存 latest state，无历史

## 真要实现的工作量

1. Backend：新 Tauri 命令 `memory_item_history_count(detail_path:
   String, days_ago: u32) -> u32` — 扫 `.history/` dir 数文件名 ts
   filter
2. Frontend：每个 item 调一次（or 批量命令 batch by paths returning
   Map）+ 渲染 chip
3. 处理 cap：当返回 5（HISTORY_CAP 上限）时显「5+」表达 undercount
4. 处理 missing .history：对无 .history dir 的 item（从未 update 过
   detail 或刚创建）→ 0 not 错误
5. tests：count 0 / count 5+ / 解析失败时 0 / 7d 内多份 + 7d 外多份
   分别 count

跨多层 + 含小坑（path resolve / ts filename parse）— 单 iter 范围
偏大。且 cap 5 让信号天然不准 — 「3 次 update」是真，「5+ 次 update」
歧义（可能是 5 / 6 / 10 / 100，全显 5+）。

## 现有更合理的 alternative

PanelMemory hover preview 已有：
- 📅 创建 X 前 chip（line ~6777）
- 🔄 更新 X 前 chip（line ~6791）

这两 chip 已表达「item 活跃度」— 「更新 1 小时前」明显比「更新 3
个月前」更活跃。owner 想 audit「哪 item 最近活跃」用既有 「📅 按时
间」cat-level sort toggle（按 updated_at 倒序）即得。

count 维度（≥X 次 update）作为 nice-to-have：单 item 真在「持续被
edit」是少数场景；多数 item 是「创建后偶 update 一次」— count 信号
信噪比不高。

## Decision

**不实现 PanelMemory item「📊 7d updates」chip**。两条理由：

1. 数据 substrate 不足（单 updated_at 字段 ≠ 次数）— 需 backend 加
   per-item history count 命令 + cap 5 让信号天然不准
2. 既有 🔄 更新 X 前 chip + 📅 按时间排序 toggle 已覆盖「item 活
   跃度」实用场景；count 维度信噪比低

procedure 教训：propose 「count of X 」类需求时，应预先确认数据源
是否含 history（频次需历史，单字段只能算最近 1 次）— 这是 iter #554
（⌘⇧D 删除最后 user）/ iter #560（/pinned_drop_7d）同类的"data
substrate vs feature intent 差"教训重复出现。

## Future iters (out of scope)

- **「item 活跃度 badge」用现有 updated_at 单字段**：item 行加二态
  「🔥 fresh（updated_at 在 24h 内）」/ 「🍂 cold（>30d）」/ 默认无。
  比 count 信号更直接也更现实；UI 也更简洁。propose 后单独实施
- **真做 history count chip**：需 backend `memory_item_history_count`
  + cap 5 边界处理 + 批量 fetch。可作 mid-size iter 起手做。但优先
  级低（intent 替代品多）
- **butler_tasks 专属「N times touched 7d」**：butler_history.log
  scan 给单 task 历史事件 count — 比 .history snapshot 更细（含
  description 改 not 仅 detail）。但仅适用 1/N cats — 实施 ROI 低
