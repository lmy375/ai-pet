# 任务面板 📌 钉住 chip 过滤

## 背景

TODO 上 auto-proposed 一条："任务面板顶部新增「📌 钉住」chip 过滤：让用户标星 / 钉住任务并能一键过滤只看 pinned；解决"长 pending 队列里关键任务被淹"的问题。"

任务面板支持 due / priority / origin / search / tag 多维过滤，但唯独缺"重要度"维度 —— 一旦 pending 队列超过 ~10 条，用户即使搭配既有 filter 也很难只看"我盯了 N 周还没动手的几条关键事"。把"重要度"做成 `[pinned]` boolean marker（与 `[snooze:]` / `[blockedBy:]` 同 description-marker 协议族）让 owner 一键标记，配 chip 过滤就能在长队列里建立"今天就盯这几条"的工作面。

## 改动

### Backend（Rust）

#### `src-tauri/src/task_queue.rs`

- 新增 `parse_pinned(description) -> bool`：严格匹配字面 `[pinned]`（不接 `[Pinned]` / `[PINNED]` / `[pinned: ...]` / `[pin]` 等变体 —— 单一写入路径保 LLM / UI 看到同一形态）。
- 新增 `strip_pinned_markers(desc) -> String`：剥所有 `[pinned]` 段 + `collapse_whitespace` 归一空白，与 `strip_snooze_markers` 同模式。
- `TaskView` 加 `pinned: bool` 字段（`#[serde(default)]` 兼容老 session）。
- 4 个新单测覆盖：strict-form 命中、variants 拒绝、whitespace normalize、其它 marker 不被误剥（`[task pri=3]` / `[snooze:]` / `[origin:tg:]`）。

#### `src-tauri/src/commands/task.rs`

- `build_task_view`：body 渲染前用 `strip_pinned_markers` 把 `[pinned]` 字面剥掉（与既有 `strip_origin_marker` / `strip_result_marker` 同语义 —— 字段已独立暴露，body 不重复出现）；`pinned` 字段填 `parse_pinned(raw)`。
- 新 `task_set_pinned(title, pinned)` Tauri command —— `pinned=true` 时 `append " [pinned]"`、`false` 时 `strip_pinned_markers` 剥干净。strip-before-write 保多次 set / unset 不让 description 越来越长。**与 `task_set_snooze` 完全同模板**（找 item → strip → 拼新 desc → memory_edit），不推 decision_log（owner 偏好标注非状态转移）。

#### `src-tauri/src/lib.rs`

`invoke_handler!` 注册 `commands::task::task_set_pinned` 紧贴 `task_set_snooze`。

### Frontend（TypeScript）

#### `src/components/panel/PanelTasks.tsx`

- `TaskView` interface 加 `pinned?: boolean`（可选兼容老 backend 输出）。
- 新增 `pinnedFilter: boolean` state，localStorage key `pet-task-pinned-filter` 跨 session 持久。`filteredTasks` 链条 + `filtersActive` + 3 处"清全部过滤" callsite 都把 pinnedFilter 算上。
- 新 `pinnedCount` memo：只数活动态 pinned 任务（与 dueTodayCount 同语义）。
- chip 行 `pinnedCount > 0` 时常驻渲染 📌 chip：amber tint 与 dueFilter 系列色族错开（pinned 是"owner 标注"维度而非 due/时态/priority）；激活态 = 实心 amber + 白字。
- 任务行右键菜单："📂 展开详情" 之后 / "✓ 标 done" 之前插 `📌 钉住` / `📌 取消钉住` 切换按钮 —— done / cancelled 行也允许（owner 标注与状态正交，不放在 canMarkDone gate 后）。颜色反映方向：未 pinned 时 amber 字（强调"可点"）、已 pinned 时 muted 字（强调"已设"）。
- 任务行 chip 区（`💤 snoozed` chip 左侧）渲染 📌 视觉提示：让"哪些被钉住"在 list 一眼可见，不必只能靠 filter chip + 右键查询。

## 关键设计

- **`[pinned]` 严格字面匹配**：与 `[done]` / `[task pri=]` 等结构化 marker 同源；单一形态让 LLM 用 `butler_task_edit` 操作 description 时不必猜 "是写 `[pin]` 还是 `[pinned]`"。`parse_pinned` 拒绝 5 种 variants 测试已覆盖。
- **marker 而非数据库列**：与既有 `[snooze:]` / `[blockedBy:]` / `[origin:]` 协议一致 —— description 是 task 的 source of truth，LLM 通过 `butler_task_edit` 工具也能看到 / 改 pinned 状态，未来扩展 `/pin` `/unpin` TG 命令 / proactive 优先级 boost 都基于同字段。db schema 不动 = 无 migration 风险。
- **strip-before-write**：与 snooze 同手法。owner 反复 toggle 不会让 description 累积冗余 `[pinned] [pinned] [pinned]`；任意时刻 description 恰有 0 或 1 个 `[pinned]`。
- **不影响 sort**：pinned 任务不强制浮顶 —— 既有 sort（pending 优先级 / 创建时间 / 自定义 queue）已是 owner 调过的；把 pinned 任务突然"插队"会破坏 sort 直觉。chip filter 才是主入口："想集中处理 pinned 就开 chip"。
- **localStorage 持久 filter 状态**：用户开过滤后切走 / 重启 panel 仍保留 —— 解决"chip 状态丢"体验割裂。fallback false 不打扰新用户。
- **amber 色族**：与既有 due（red/orange/blue）/ priority（slate/gray）/ origin（accent）配色族错开 —— pinned 是"owner 标注"维度，独立配色让识别更快。
- **right-click 入口 + 列 chip 视觉提示**：右键 toggle = 最小 UI 增量；列 chip = 静态可见。两者互补，不抢主视觉。

## 不做

- **不做单独"📌 钉住"按钮在 row 顶级**：UI 已经够多 chip / 按钮；右键菜单是既有 snooze / cancel / done 同口径入口，pinned 自然落进去而非另起 button。
- **不做 sort-to-top**：见"不影响 sort"说明。
- **不动 TG bot**：没必要立即给 `/pin` `/unpin` —— pin 是 "在面板专注工作时的 quick filter"，TG 端没这个 use case。后续若有需求再加。
- **不做 LLM proactive boost**：让 `[pinned]` 影响 LLM 取单选优先级也合理，但属于 GOAL.md "自我进化"维度的扩展，单独成需求更清楚。本次仅做 owner-facing chip filter。

## 验证

- `cargo test --lib` ✓ **992 / 992 通过**（含 4 条新 parse_pinned / strip_pinned_markers 单测）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s
- 改动 ~250 行（task_queue 30 + commands/task 30 + lib.rs 1 + PanelTasks 180 + tests 35）；既有 snooze / blockedBy / origin marker 路径不变。

## TODO 状态

5 条候选 auto-proposed 全部完成。

## 后续

- LLM proactive 选单：把 `t.pinned` 作为 priority boost（如 `+2`）让宠物自动倾向先做 pinned 任务。
- `/pin <title>` `/unpin <title>` slash 命令：与 `/done` `/cancel` 同模板桌面 + TG 双端。
- detail.md 顶部行加 📌 visual 标 + 一键 toggle 按钮（详情视图也方便操作）。
- pinned 任务"久攥不松"提醒：连续 N 天 pinned 但 updated_at 没动时让 proactive 一句"这条钉住好几天了，要不要拆 / 取消"。
