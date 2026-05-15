# PanelChat `/cancel <title>` slash 命令

## 背景

上一条 commit 加了桌面 `/done`，与 TG 的 `/done` 形成对称。但 TG 三件套是 `/done | /cancel | /retry`，桌面只补到 1/3 —— 想"放弃这条任务"还要切到「任务」tab 点取消钮。

`/retry` 暂不加（适用面更窄 —— 只对 Error 态可用，桌面没"批量重试"诉求；先保留缺口给后续看 telemetry）。先把高频的 `/cancel` 也补齐。

## 改动

### `src/components/panel/slashCommands.ts`

- `SLASH_COMMANDS` 加 `{ name: "cancel", description: "取消任务：/cancel <标题（子串模糊匹配）>", parametric: true }`，紧挨在 `done` 后面 —— 与 TG `/help` 行的"done | cancel | retry"近邻语义一致
- `SlashAction` union 加 `{ kind: "cancel"; query: string }`
- `parseSlashCommand` switch 加 `case "cancel"`：空 arg → unknown；否则 `{ kind: "cancel", query: arg }`

实现与 `case "done"` 完全平行。

### `src/components/panel/PanelChat.tsx`

`executeSlash` 加 `case "cancel"`：fuzzy 匹配逻辑与 `/done` 同 —— 抽不抽 helper？

**不抽**：两个 case 加起来 ~80 行，抽出 helper 要传 6+ 参数（query, action verb 名, backend invoke fn name, 反馈文案模板等），抽完反而读起来更绕。两份 ~40 行的代码对称、好对照、好删 —— GOAL.md "代码越少越好" 偏好用"在生产 3 份雷同时再抽"的阈值，这里第 2 份就抽是过早抽象。

唯一差异：
- backend 调 `invoke<void>("task_cancel", { title: target, reason: "" })`（reason 空，对齐 TG bot 行为）
- 成功反馈 `🗑 已取消：<title>`（图标区分 /done 的 ✓）
- 未找到 / 多命中文案改"取消"动词

不限定状态：与 /done 同策略，已 done / cancelled 的会触发 backend 的"终态拒绝" Err，由 catch 分支显示 `/cancel 失败：<err>`，符合预期。

## 不做

- 不加 `/retry`：Error 状态较稀有，且 retry 失败后用户多半要看具体原因再决定（不适合"无脑一行" CLI 流）。如未来 telemetry 显示 Error 单 retry 占比高再补。
- 不加 `--reason` 子参数：保持 slash 命令一行清爽；想填取消原因走「任务」tab 的 dialog。
- 不抽 fuzzy helper：见上分析。

## 验收

- `npx tsc --noEmit` ✅
- 聊天里 `/c` → 候选含 `cancel`
- `/cancel <部分标题>` 唯一命中 → 🗑 反馈；再去「任务」tab 看 status=cancelled
- `/cancel <已 done 任务>` → 显 backend 错误（不能取消终态任务）

## 完成

- [x] slashCommands.ts: SLASH_COMMANDS + SlashAction + parser case
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
