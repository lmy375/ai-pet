# PanelChat `/done <title>` slash 命令

## 背景

Telegram bot 端已经有 `/done <title|N>` 让用户在手机上一行命令标完一条任务（见 `telegram/commands.rs` + `bot.rs`）。桌面 PanelChat 一直只有 `/clear /tasks /search /sleep /image /help` 五条 —— 想标 done 必须切到「任务」tab、滚到目标行、按 d / 点钮。

桌面侧高频操作（聊到一半想起"那条事其实已经做完了"）经常要切 tab 打断 chat flow，体感是缺一条快速通道。

## 改动

### `src/components/panel/slashCommands.ts`

- `SLASH_COMMANDS` 加 `{ name: "done", description: "标记一条任务为完成：/done <标题（支持子串模糊匹配）>", parametric: true }`
- `SlashAction` union 加 `{ kind: "done"; query: string }`
- `parseSlashCommand` switch 加 `case "done"`：
  - 空 arg → `{ kind: "unknown", name: "done" }`（让 UI 提示）
  - 否则 → `{ kind: "done", query: arg }`（arg 已 `.trim()`，由顶层解析逻辑保证）

不在 parser 层做 fuzzy 匹配 —— parser 是纯字符串层，匹配需要异步读 task_list。

### `src/components/panel/PanelChat.tsx`

`executeSlash` switch 加 `case "done"`：

1. `invoke<TaskListResponse>("task_list")` 拉当前所有 butler_tasks（已在前端他处用过同样的命令）
2. 在 `resp.tasks` 里跑模糊匹配：
   - 第一优先：exact title 相等
   - 第二优先：title 子串包含（case-insensitive）—— 多个候选时 ambiguous
3. 命中 0 → `pushLocalAssistantNote("⚠️ 没找到匹配 '<query>' 的任务。/tasks 看完整列表。")`
4. 命中 ≥2 → 拼候选 title 列表打回（最多展 5 条），引导用户精确化
5. 唯一命中 → `invoke<void>("task_mark_done", { title, result: null })` + `pushLocalAssistantNote("✓ 已标 done：<title>")`
6. 调用失败 → 显错误文案

`TaskListResponse` 类型在 PanelTasks 里定义，PanelChat 用 `invoke<{ tasks: Array<{ title: string; status: string }> }>` 局部内联类型（避免跨文件 import 一个仅用一次的复合类型）。

不限定状态：即便是已 done / cancelled 的任务，task_mark_done 在后端会幂等覆盖 description marker（追加 `[done]`），不至于产生伤害；用户偶尔会想"补打个 done"。

`SLASH_COMMANDS` 列表里新 case 自动进入 `recordSlashCommandUsage` 频率排序、`formatHelpText` `/help` 输出、`SlashCommandMenu` 过滤面板，无需额外改动。

## 不做

- 不支持 `/done N`（按 1-indexed 序号）：桌面 PanelChat 没有"上次列出的任务"上下文（不像 TG 每次 /tasks 会缓存 `last_tasks_titles[chat_id]`）。强行加一个全局上次列表反而让语义不清。
- 不支持 `/done <title> -- <result>` 写完成摘要：保持 slash 命令简洁；想写产物的用户走「任务」tab 的 dialog（已支持）。
- 不写单元测试：项目前端无 vitest / jest（无 `*.test.ts*` 文件）；parser 改动仅一条 case，逻辑与 `/sleep` 同构。

## 验收

- `npx tsc --noEmit` ✅
- 切「聊天」tab → 输 `/d` 看到「done」候选 → 选中回填 `/done `
- `/done 某任务` 唯一命中 → 显 `✓ 已标 done：xxx`；再去「任务」tab 看该任务已 done
- `/done <不存在>` → 显 ⚠️ 未找到
- `/done <子串多命中>` → 显候选清单

## 完成

- [x] slashCommands.ts: SLASH_COMMANDS + SlashAction + parser case
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
