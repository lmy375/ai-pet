# PanelChat `/retry <title>` slash 命令

## 背景

前两条 commit 加了桌面 `/done` 和 `/cancel`，与 TG 形成 2/3 对称。本 commit 补齐最后一条 `/retry`，让 desktop 三件套和 TG `/help` 行的 `/done | /cancel | /retry` 完全对齐。

上一条 `/cancel` 的 plan doc 里曾论证"先不加 /retry，等 telemetry" —— 复盘后认为对称完整性本身就是一种确定性价值：用户敲 `/cancel` 后下次顺手敲 `/retry` 发现"没这条"会困惑，反而是断点。三件套是一个 mental unit。

## 改动

### `src/components/panel/slashCommands.ts`

- `SLASH_COMMANDS` 加 `{ name: "retry", description: "重试 Error 任务：/retry <标题（子串模糊匹配）>", parametric: true }`，紧跟 `cancel`
- `SlashAction` union 加 `{ kind: "retry"; query: string }`
- `parseSlashCommand` 加 `case "retry"`：与 done / cancel 同构

### `src/components/panel/PanelChat.tsx`

`executeSlash` 加 `case "retry"`：

- 拉 `task_list` 后**先按 status==error 过滤**再做 fuzzy 匹配 —— retry 语义上只对 Error 行有意义，过滤后大幅降低多命中歧义（用户的 pending 任务跟 error 任务可能同名前缀）
- 命中 0 → 文案改 "没找到匹配 '<q>' 的 Error 任务（/retry 仅作用于 Error 状态；其它状态请去「任务」tab）" —— 显式提示"不是没这条任务，是没 Error 态的这条任务"
- 唯一命中 → `invoke<void>("task_retry", { title })` + 反馈 `↻ 已重试：<title>`
- 多命中（罕见，但同 title 在不同时段反复 Error 时可能） → 同 /done /cancel 候选预览模式

不抽 helper —— 第 3 份 ~45 行的代码，已经到了"抽出 helper 三参就能搞定"的阈值。但抽完后 done/cancel/retry 仍各自需要传 verb 反馈文案 + invoke 名 + status filter 谓词，参数 4+。代码 lint 跑过后再看是否要抽 —— 当前先保住对称可读性。

## 不做

- 不在 fuzzy 阶段对 done / cancel 也加 status filter：那两个命令对任意状态都有合理用法（done 在 pending/error 上明确；cancel 在 pending/error 上明确；新增的"状态守卫"会让 UX 不一致）。retry 是因为后端硬性只允许 Error，所以前端先过滤是改善而非约束。

## 验收

- `npx tsc --noEmit` ✅
- 聊天 `/r` → 候选含 `retry`
- 制造一个 Error 任务（任意手段；或读现有），`/retry <部分标题>` → 显 `↻ 已重试：xxx`，「任务」tab 中状态回 pending
- `/retry <无 Error 命中>` → 显 ⚠️ "没找到匹配 ... 的 Error 任务"

## 完成

- [x] slashCommands.ts: SLASH_COMMANDS + SlashAction + parser case
- [x] PanelChat.tsx: executeSlash case
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
