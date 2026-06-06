# 061 · TG bot 命令数超 Telegram API 上限 — `BOT_COMMANDS_TOO_MUCH`

PanelDebug 截图显示 inline 启动告警：

```
set_my_commands: A Telegram's error: Unknown error: "Bad Request: BOT_COMMANDS_TOO_MUCH"
```

Telegram bot setMyCommands API 上限是 100 条 / 单 scope，本项目 commit history 显示已注册 /here_* (6+)、/cat_* (多)、/recall、/aliases、/streak、/streak_pin、/cat_growth_* 多变种、/cat_decay_* 多变种、/cat_top、/audit_summary、/help_table、/pin_grow_7d、/pinned_drop_7d、/idle_7d、/find_speech_yesterday、/last_speech、/show_speech、加上 011/012/030/032/037/043/044 即将引入的更多命令 — 实际数已超 100。

需求：
- 重组现有 TG 命令，按家族 namespace 收敛：例如 `/cat growth 7d`、`/cat decay 7d`、`/cat top 5` 取代 `/cat_growth_7d`、`/cat_decay_7d`、`/cat_top` 等独立条目；同家族子命令不向 setMyCommands 注册，只注册家族入口（≤ 1 条 per 家族）。
- setMyCommands 只注册"日常高频 + 用户必知"的 ≤ 20 条；其余通过对话 LLM intent 识别走自然语言路径，不依赖 / 命令补全。
- 启动时若 setMyCommands 仍失败 → log warning + 继续启动；不阻塞 bot 全功能（当前可能已经如此，但 banner 提示文案应说明"bot 仍可用 / / 命令面板不全"）。
- 新增命令必须先评估归属哪个家族（不再随手 /xxx_yyy_zzz），无家族归属的命令 review 时拒绝。
- 已有 docs / `/help_table` family 分组逻辑直接复用 — 现成的家族划分是命令收敛的直接依据。
- 060 PanelDebug 整改中的 alert 组件落地后，本 BOT_COMMANDS_TOO_MUCH 错误用该 alert 展示。

---
实现笔记：
- 本刀解决 `BOT_COMMANDS_TOO_MUCH` 触顶**立即问题**：缩减注册给 TG 的命令到 ≤ 20 essential + ≤ 20 custom，全套命令仍能通过 `parse_tg_command` 文字解析工作（用户文字打全名即可，仅 `/` 弹窗补全只显精选）。家族 namespace 重构（`/cat growth 7d` 等）留单独刀。
- `src-tauri/src/telegram/commands.rs` 新加：
  - `ESSENTIAL_TG_COMMAND_NAMES: &[&str]` 19 条 essential（core task lifecycle + 高频 audit + 信号查询 + 控制/帮助），按 TG 弹窗呈现顺序排列
  - `ESSENTIAL_TG_CUSTOM_BUDGET: usize = 20`——19 + 20 = 39 远 < 100，留 60 余量
  - `essential_tg_command_registry(custom, lang)`：从 `merged_command_registry` 全集过滤 hardcoded 到 essential 名单；custom 段截 budget；essential 在前
- `telegram/bot.rs::set_my_commands` 改调 `essential_tg_command_registry`；失败 message 增提示「bot 仍可用，但 / 命令补全弹窗不全；可文字打全名运行」（spec 反指令对应）
- 5 单测：essential ≤ 20 + 总预算 < 100 / 无 custom 时仅 essential / custom 在 budget 内全收 / custom 超 budget 截断 / 总 < 100 留余量
- **缺口**：
  1. **家族 namespace 重构**：spec「`/cat growth 7d` 取代 `/cat_growth_7d`」未做——需重写 parser 支持分级子命令。本刀仅止血。
  2. **review 流程约束**：spec「无家族归属拒收」是 process 不在 code；归入 PR review checklist。
  3. **/help_table family 分组复用**：essential 名单当前手选；后续可改 family-pick 算法。
  4. **060 alert 组件展示**：060 未做，仍走既有 inline banner / `telegram::warnings`。
