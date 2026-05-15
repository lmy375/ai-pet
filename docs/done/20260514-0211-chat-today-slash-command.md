# PanelChat `/today` slash 命令 — 今日叙事视图

## 背景

`/stats` 给 5 个数字（"待办 12 · 逾期 1 · 今日完成 3 · ..."），看的是**结构性指标**。但日常打开聊天最常问的是"我今天该干嘛 / 今天搞定了啥" —— 想直接看**标题列表**而不是数。切去「任务」tab 滚动也行，但桌面聊天面板就地一行命令更顺手。

## 改动

### `src/components/panel/slashCommands.ts`

- `SLASH_COMMANDS` 在 `/stats` 之后插 `{ name: "today", description: "今日叙事视图：到期 / 已完成任务标题清单", parametric: false }`
- `SlashAction` union 加 `{ kind: "today" }`
- `parseSlashCommand` 加 `case "today": return { kind: "today" }`（无参，多余 token 忽略）

### `src/components/panel/PanelChat.tsx`

`executeSlash` 加 `case "today"`：

1. `invoke<TaskListResponse>("task_list")` 拉全集
2. `todayPrefix = new Date().toLocaleDateString("sv-SE")` 取本地 ISO 日期
3. 筛 3 个桶：
   - `dueToday`: status === "pending" && due && due.slice(0,10) === todayPrefix
   - `doneToday`: status === "done" && updated_at.startsWith(todayPrefix)
   - 不显 cancelled / error —— 那是 `/stats` 的活
4. 拼文本：
   ```
   📅 今日（YYYY-MM-DD）
   
   今日到期（N）：
   · 标题1 · HH:MM
   · 标题2 · HH:MM
   …还有 K 条     # 超 5 条
   
   今日已完成（M）：
   · 标题1
   · 标题2
   
   （全空时改 "今日队列清爽 ✨，可去 /stats 看整体队列"）
   ```
5. `pushLocalAssistantNote(formatted)` 走 subdued bubble

每段 cap 5 条 + "…还有 K 条"，避免上百条任务时单条 bubble 撑爆。

## 不做

- 不显逾期 / error 段：那两段查 `/stats`；今日视图是"今天"窗口
- 不带状态 emoji：标题已带 #tag，再叠 emoji 视觉过载
- 不抽 backend helper：与 /stats 不同 —— /stats 已经下沉到 backend（数字汇总稳定 / 多 caller），/today 是"叙事文本拼装"，频次低 + 文案易变，纯前端就近实现更轻
- 不写单测：前端无 vitest；纯字符串拼装与 /stats 同模式

## 验收

- `npx tsc --noEmit` ✅
- 聊天 `/t` → 候选含 today
- `/today` 有今日到期 + 完成任务 → 显两段标题清单
- `/today` 全空 → 显"今日队列清爽 ✨" 一行

## 完成

- [x] slashCommands.ts: 注册 + parser
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] README 一行
- [x] 移到 docs/done/
