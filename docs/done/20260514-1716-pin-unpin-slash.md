# 桌面 + TG `/pin` `/unpin` slash 命令

## 背景

TODO 上 auto-proposed 一条："桌面 + TG `/pin <title>` `/unpin <title>` slash 命令：与 `/snooze` 同模板，让 owner 不出聊天框也能钉 / 解钉任务。"

上一轮刚落地 `[pinned]` marker + 桌面 chip 过滤 + 右键 toggle。但 IM 用户 / 移动场景下没法摸右键 —— `/snooze` `/unsnooze` 等任务命令已是双端对偶，pin 这条独缺 slash 入口就让"钉住"成为仅桌面 + 鼠标的功能，体验割裂。补 slash 命令让 marker 的可达性追上 chip + UI 的水平。

## 改动

### Backend（Rust）

#### `src-tauri/src/telegram/commands.rs`

- 新 enum variants：`TgCommand::Pin { title }` / `TgCommand::Unpin { title }`，与 `Snooze` / `Unsnooze` 同形（仅一字段 title，无 preset）。
- `name()` / `title()` / `tg_command_registry_localized` 三处分支补齐（zh + en 双语 description）。
- `parse_tg_command`：`"pin" / "unpin"` 分支，全 arg 当 title（不做 preset 解析，与 done / cancel / retry 同语义）；empty title 由 handler 走 missing-argument。
- `format_help_text` 加 `/pin <title> | /unpin <title>` 一行。
- 单测 `tg_command_registry_covers_all_user_facing_commands` 名单加 "pin" / "unpin"（防 silent gap 钉死）。
- 新 2 个 parser 单测：
  - `parses_pin_unpin`：含多 token 也合法。
  - `parses_pin_unpin_empty_title_yields_command_with_empty`：空 title parser 不特殊化，由 handler 接管。

#### `src-tauri/src/telegram/bot.rs`

- Empty-title gate（`TgCommand::Cancel / Retry / Done / Snooze / Unsnooze` 那条 if-let pattern）追加 `Pin / Unpin` 分支。
- 新 handler `TgCommand::Pin { title }` / `TgCommand::Unpin { title }`：与 `/snooze` 同三层 resolve（数字 index → fuzzy → 错误）+ 调 `task_set_pinned(t, true/false)`。成功文案附反向命令引导（`/pin` → `/unpin`）。

### Frontend（TypeScript）

#### `src/components/panel/slashCommands.ts`

- 注册新命令：`{ name: "unpin", parametric: true }`。`/pin` 不新注册 —— 复用既有 session-pin 入口。
- `SlashAction` 加 `{ kind: "pinTask"; query: string }` 与 `{ kind: "unpin"; query: string }`。
- parser `case "pin"` **双语义**：
  - `arg.length === 0` → `{ kind: "pin" }`（既有"切换当前会话钉住"行为不变）
  - `arg.length > 0` → `{ kind: "pinTask", query: arg }`（新任务钉住）

  两个动作在不同对象上 —— 由是否带 title 消歧（与 `/snooze` 这种始终带 title 的命令对照看，pin 之所以兼容是因为"钉当前会话"是高频且没歧义的 zero-arg alias）。
- 现有 `"pin"` 命令 description 改成 `"无参 → 钉住当前会话；带参 → 钉任务：/pin [<标题>]"`，让 `/help` 也透出双语义。

#### `src/components/panel/PanelChat.tsx`

- 新 handler `case "pinTask"`：与 `/done` `/cancel` 同模板 fuzzy 命中 + `task_set_pinned(title, true)` + `pushLocalAssistantNote("📌 已钉住：${title}")`。候选不限 status（pinned 与状态正交 —— owner 偏好标注，与 PanelTasks 右键菜单同语义）。
- 新 handler `case "unpin"`：同上调 `task_set_pinned(title, false)` 剥 marker。

## 关键设计

- **`/pin` 双语义（zero-arg = session pin / with-arg = task pin）**：原 `/pin` 是"切当前会话钉住"的高频 alias，给 task pin 起新名（如 `/pintask` / `/star`）反而割裂用户心智。带参 vs 无参是天然消歧点，与 IM 业界惯例（Slack `/remind` 也是 zero-arg vs with-arg 不同语义）一致。`/unpin` 没原命令冲突，单义任务取消钉住。
- **TG / 桌面双端同时上线**：与 `/snooze` `/unsnooze` 同一轮覆盖 —— 避免出现"桌面有 / TG 没有" 的临时割裂。TG 端 description / help / registry 三处一起加，让命令首次出现就是完成态。
- **`task_set_pinned` strip-before-write**：上一轮已实现，本轮直接复用。`/pin` 反复调用幂等（不让 description 累积 `[pinned] [pinned] [pinned]`），`/unpin` 没钉住的任务调用也是 no-op-friendly。
- **候选不限 status**：与桌面右键菜单 pin toggle 同放宽 —— done / cancelled 行也允许 pin（owner 标注"重要"与状态正交）。
- **空 title parser 不特殊化**：与 `/done` `/cancel` 同 —— parser 层把空当 `TgCommand::Pin { title: "" }` 透传，由 handler 的 empty-title gate 统一发 `format_missing_argument`。让 parser 保 pure + 单一职责。

## 不做

- **不做 `/pintoggle`**：toggle 语义在 chat 里不直观（用户敲命令时不知当前状态），分立 `/pin` `/unpin` 更明确。
- **不动 LLM proactive 优先级**：pinned 应该 boost LLM 选单优先级是合理扩展，但属于"自我进化"维度的独立需求（已在 TODO 池里另列一条）。本次只补 owner-facing slash 入口。
- **不接 `/pin <title> top`** 这种位置 hint：pin marker 是 boolean，没"钉到顶"语义；要排顶得改 sort 而非 marker。
- **不做"pin 数量上限"**：让 owner 自由判断；过多 pin 自然会失去过滤价值，自我调节即可。

## 验证

- `cargo test --lib` ✓ **994 / 994 通过**（+2 新 parser 单测：`parses_pin_unpin` / `parses_pin_unpin_empty_title_yields_command_with_empty`）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.18s
- 改动 ~150 行（TG enum + parser + handler + tests 80 行；前端 SLASH_COMMANDS + action + parser + PanelChat handler 70 行）；既有 `/snooze` `/unsnooze` `/done` 等命令路径不变。

## TODO 状态

5 条新 auto-proposed 完成 1 条，余 4 条留池：
- pinned 任务 proactive prompt 优先级 boost
- 批量勾选 → 批量 pin / unpin
- detail.md `⌘S` 保存键
- detail.md「📅 当前时间」按钮
- 任务行 chip 区显 created_at 相对时间

## 后续

- 给 `/pintask <title> all` / `/unpin <pattern>` 加批量语义（与 [批量 pin / unpin] TODO 衔接）。
- `/pinned` slash 一键打开「📌 N」chip 过滤（免去先开 panel 再点 chip）。
- TG `/tasks` 输出在 pinned 任务前显 📌 figure，让 owner 在手机端也能一眼看到"哪些钉了"。
