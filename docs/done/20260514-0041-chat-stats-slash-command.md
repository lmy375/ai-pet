# PanelChat `/stats` slash 命令 — 桌面任务状态速览

## 背景

上轮加了 TG bot 的 `/stats`，单行汇总当前 chat 派出的任务状态计数。桌面端没有对称命令 —— 用户在桌面 PanelChat 想看"今天到底干完几条 / 还有几条挂着"得切「任务」tab 滚动数。

加 `/stats` 后桌面与 TG 形成完整的命令对称（`/done /cancel /retry /tasks /stats /help`），且 `/stats` 比 TG 多看全局：桌面会统计所有任务（panel 创建 + TG 派出），TG 只看本 chat 的。

## 改动

### `src/components/panel/slashCommands.ts`

- `SLASH_COMMANDS` 紧跟 `/tasks` 加 `{ name: "stats", description: "汇总：待办 / 逾期 / 今日完成 / 出错 / 今日取消 计数", parametric: false }`
- `SlashAction` 加 `{ kind: "stats" }`
- `parseSlashCommand` 加 `case "stats": return { kind: "stats" }`（无参，多余 token 忽略，与 `/tasks` 同模式）

### `src/components/panel/PanelChat.tsx`

`executeSlash` 加 `case "stats"`：

1. `invoke<TaskListResponse>("task_list")` 拉全集
2. 本地 today prefix 取 `YYYY-MM-DD`（`new Date().toISOString().slice(0, 10)` 是 UTC 不对 — 用 `toLocaleDateString("sv-SE")` 拿 ISO 格式的**本地**日期，与后端 updated_at 的 `+08:00` ISO 一致）
3. now ms 用 `Date.now()`
4. 一次遍历计算 5 个数：
   - pending: status === "pending"
   - overdue: status === "pending" && due && Date.parse(due) < now
   - doneToday: status === "done" && updated_at.startsWith(today)
   - error: status === "error"
   - cancelledToday: status === "cancelled" && updated_at.startsWith(today)
5. 拼 6 行文案，全 0 时附 "（今日很安静 ✨）"
6. `pushLocalAssistantNote(formatted)` 走与其它 slash 命令同款的会话气泡

### 不抽共享 helper

TG 端的 `format_stats_reply` 是 Rust 函数，不能共享给 TS。再说桌面的 `/stats` 统计**全集**（非 origin 过滤），TG 是单 chat —— 计数语义有微妙差异，硬抽反而要传 filter 谓词参数把简单逻辑搞复杂。复用文案样式即可。

## 不做

- 不新建 backend `/task_stats` 命令：task_list 本来就常调（PanelTasks 进入即调）；多一次本地遍历开销可忽略
- 不在文案里列具体 title：那是 `/tasks` slash 的活
- 不为 panel 创建 vs TG 派出做拆分计数：本次只解决"我现在到底状态如何"的 v1 问题；future iteration 可加 `/stats tg` `/stats panel` 等子命令

## 验收

- `npx tsc --noEmit` ✅
- 桌面聊天 `/s` → 候选含 stats
- `/stats` → 6 行汇总文案显示在会话里
- 全无任务 → "今日很安静 ✨"
- 把一个 done 任务的 updated_at 改到昨天前置环境（实际验收靠 manual 触发）→ 不计入今日完成

## 完成

- [x] slashCommands.ts: 注册 + parser
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
