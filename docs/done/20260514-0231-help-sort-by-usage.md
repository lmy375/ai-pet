# `/help` 按使用频次排序 + 未使用命令独立段

## 背景

`formatHelpText` 现在按 `SLASH_COMMANDS` 声明序输出全部命令清单，与 slash 菜单（已按 usage score 排序）的顺序**不一致** —— 用户用熟了 `/done /stats /today`，slash 菜单把它们顶在最前，但 `/help` 仍把 `/clear /tasks` 排在最前面，体验割裂。

同时 `/help` 列出全部 11 条会有点信息密集；如果按"用过 / 没用过"二分，新用户能一眼看到"还有这些命令没探索过"。

## 改动

`src/components/panel/slashCommands.ts`：

`formatHelpText`：

```ts
export function formatHelpText(): string {
  const scores = readSlashScores();
  // 与 filterCommandsByPrefix 同分桶：score > 0 = 用过；= 0 = 没用过。
  // 用过段按 score 倒序（与 slash 菜单一致）；没用过段保留 SLASH_COMMANDS
  // 声明序（隐式语义：发现引导 = 高频命令先看到）。
  const used: SlashCommand[] = [];
  const unused: SlashCommand[] = [];
  for (const c of SLASH_COMMANDS) {
    if ((scores[c.name] ?? 0) > 0) used.push(c);
    else unused.push(c);
  }
  used.sort((a, b) => (scores[b.name] ?? 0) - (scores[a.name] ?? 0));
  const fmt = (c: SlashCommand): string => {
    const arg = c.parametric ? " <参数>" : "";
    return `/${c.name}${arg}  —  ${c.description}`;
  };
  const lines: string[] = [];
  if (used.length === 0) {
    // 全新用户：没用过任何命令 → 单段输出，按声明序，无 header 反而清爽
    lines.push("可用命令：");
    for (const c of SLASH_COMMANDS) lines.push(fmt(c));
  } else if (unused.length === 0) {
    // 用过所有命令：单段输出，按 score 倒序
    lines.push("可用命令（按近期使用频次）：");
    for (const c of used) lines.push(fmt(c));
  } else {
    lines.push("常用：");
    for (const c of used) lines.push(fmt(c));
    lines.push("");
    lines.push("未试过：");
    for (const c of unused) lines.push(fmt(c));
  }
  return lines.join("\n");
}
```

`readSlashScores` 已经是同模块私有 fn，直接调即可（无需 export）。

## 不做

- 不显具体 score 数值：用户看不出 "1.7 次" 是什么意思；排序本身是用法语义，数字反而噪音
- 不显"上次使用时间"：score 已自带 decay 衰减语义，时间戳是 over-engineering
- 不持久化"是否已读取 unused 段"：每次 `/help` 都重新分桶，让"试新命令"自然推回 used 段

## 验收

- `npx tsc --noEmit` ✅
- 全新 session（无 score history）`/help` → 单段，按声明序（与之前完全一致）
- 用过 `/done /stats /today` 几次后 `/help` → 两段："常用"显这三条按 score 倒序；"未试过"显 image / search / sleep 等

## 完成

- [x] slashCommands.ts: formatHelpText 改造
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
