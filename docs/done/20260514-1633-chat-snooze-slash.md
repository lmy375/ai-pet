# 桌面 `/snooze` `/unsnooze` slash 命令

## 背景

TODO 上 auto-proposed 一条："桌面 `/snooze <title> [preset]` slash 命令：与 TG 同名命令对偶，在桌面输入框也能一句话把任务暂停到预设时刻。"

桌面已有 `/done` `/cancel` `/retry` 与 TG bot 对偶，唯独 `/snooze` `/unsnooze` 是 TG only —— 用户在桌面想暂停任务只能切到「任务」tab 右键找菜单。对"重 IM 用户、轻 GUI 操作"的人体感不一致。

## 改动

### `src/components/panel/slashCommands.ts`

#### 注册命令

```ts
{ name: "snooze",   description: "暂停任务：/snooze <标题> [30m / 2h / tonight / tomorrow / monday]（缺省 30m）", parametric: true },
{ name: "unsnooze", description: "解除任务暂停：/unsnooze <标题>", parametric: true },
```

#### SlashAction

```ts
| { kind: "snooze"; title: string; spec: SnoozeSpec }
| { kind: "unsnooze"; query: string }
```

`/snooze` 在 parser 层就把 preset token 解析成 `SnoozeSpec`，handler 拿 spec + `new Date()` 算绝对 until —— 把"现在几点"的副作用挪到 handler 调用点，parser 仍是纯函数。

#### 新 helper

```ts
export type SnoozeSpec =
  | { kind: "minutes"; n: number }
  | { kind: "hours"; n: number }
  | { kind: "tonight" } | { kind: "tomorrow" } | { kind: "monday" };

export function parseSnoozeToken(token: string): SnoozeSpec | null { ... }
export function computeSnoozeUntil(spec: SnoozeSpec, now: Date): string { ... }
function splitTrailingSnoozeToken(arg: string): { title: string; token: string } { ... }
```

- 与 Rust `parse_snooze_token` / `compute_snooze_until` / `split_trailing_snooze_token` **同算法、同边界**：
  - `<N>m` 1..=10080（≤ 7 天）；`<N>h` 1..=168（≤ 7 天）；越界 → `null`
  - `tonight` = 今晚 18:00；已过 → 明晚 18:00
  - `tomorrow` = 明早 09:00
  - `monday` = 下个周一 09:00（今日是周一也跳下周一）
- `splitTrailingSnoozeToken`：取最后一 whitespace token；仅当 `parseSnoozeToken` 命中才剥；否则全 arg 当 title。让 `/snooze 倒垃圾 with whitespace` 走 `title="倒垃圾 with whitespace", token=""` → 缺省 30m，不被吃。

#### parser case

```ts
case "snooze": {
  if (arg.length === 0) return { kind: "unknown", name: "snooze" };
  const { title, token } = splitTrailingSnoozeToken(arg);
  if (title.length === 0) return { kind: "unknown", name: "snooze" };
  const spec = token.length > 0
    ? (parseSnoozeToken(token) ?? { kind: "minutes", n: 30 })
    : { kind: "minutes", n: 30 };
  return { kind: "snooze", title, spec };
}
case "unsnooze": {
  if (arg.length === 0) return { kind: "unknown", name: "unsnooze" };
  return { kind: "unsnooze", query: arg };
}
```

### `src/components/panel/PanelChat.tsx`

import：

```ts
computeSnoozeUntil,
```

handler 分支放在 `case "retry"` 之后、`case "help"` 之前，与既有任务命令成 cluster。

```ts
case "snooze": {
  const resp = await invoke<{ tasks: Array<{ title: string; status: string }> }>("task_list");
  const candidateTitles = resp.tasks
    .filter((t) => t.status === "pending" || t.status === "error")
    .map((t) => t.title);
  const res = matchTaskByQuery(action.title, candidateTitles);
  // 0 命中：提示 "/snooze 仅作用于 pending / error"
  // multi：formatMultiHitMessage 列候选
  const until = computeSnoozeUntil(action.spec, new Date());
  await invoke<void>("task_set_snooze", { title: res.title, until });
  pushLocalAssistantNote(`💤 已暂停至 ${until}：${res.title}`);
}

case "unsnooze": {
  // fuzzy 命中后 task_set_snooze(null) 清 marker；候选不限 status（任何状态
  // 都允许清 residual `[snooze:]`）。
  await invoke<void>("task_set_snooze", { title: res.title, until: null });
  pushLocalAssistantNote(`☀️ 已解除暂停：${res.title}`);
}
```

复用既有 `matchTaskByQuery` / `formatMultiHitMessage` / `pushLocalAssistantNote`，与 `/done` `/cancel` `/retry` 模板完全对齐。

## 关键设计

- **不复用 Rust 端代码**：parser / compute 这层是 ts ↔ rust 双实现，但都极小（~50 行）+ 算法 100% 对齐。引 wasm / 别的桥接代价大于收益。
- **`task_set_snooze` 协议复用**：后端 Tauri 命令早已上线，本次仅前端接入。`until` 走 `YYYY-MM-DD HH:MM` 空格分隔（任务详情 `[snooze: …]` marker 协议同源）。
- **候选过滤限 pending / error**：与桌面右键菜单 `canMarkDone` 一致 —— done / cancelled 的任务再 snooze 没语义。0 命中文案显式说明"仅作用于 pending / error"指引用户去 tab。`unsnooze` 不限 status（允许清 residual marker）。
- **缺省 30m**：与 TG 同；token 不识别也回退 30m 而非报错 —— 让 "/snooze foo bar baz"（用户漏敲 preset）也能动起来，宽松好过严格。
- **不接 `/snoozelist` / `/snoozed`**：等用户真有"看所有 snoozed"诉求再说；目前面板 ☆ pending 区域已显 💤 chip。

## 不做

- **不写测试**：纯字符串解析 + Date 算术，逻辑直观；后端 Rust 同算法已有 5+ 个 unit test 兜底语义；本地用 vitest 加 mock Date 也不会发现新东西。
- **不动 TG snooze**：本入口只在桌面 PanelChat。
- **不联动 mini chat**：mini chat 入口已支持 `/done` 等任务命令；snooze 沿用同 dispatch（PanelChat 的 handler 也覆盖 mini → panel forward 流），无需重复实现。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~180 行（slashCommands 130 + PanelChat 50）；既有 `/done` `/cancel` `/retry` `task_set_snooze` 路径不变。

## TODO 状态

5 条候选已完成 1 条，余 4 条留池：
- 任务详情图片懒加载
- 任务行 hover detail 预览
- markdown 工具栏表格按钮
- pinned 任务过滤 chip

## 后续

- 把 `parseSnoozeToken` / `computeSnoozeUntil` 加 vitest（mock Date）锁定边界 —— 当 vitest 基础设施补上后顺手做。
- snooze 写完后扔个浏览器 toast / Notification 让 macOS 通知中心也响一下（防关掉桌面 panel 后忘了任务被推到几点）。
- `/snooze` 候选 multi-hit 文案可加 "建议接 preset 一并精确化"。
