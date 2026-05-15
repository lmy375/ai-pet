# `/clearstats` slash 命令 — 清掉 slash 命令使用历史

## 背景

slash 菜单 + `/help` 都按 `readSlashScores` 排序 —— 用户用过 N 次的命令置顶。这套偏好持久在 `pet-slash-history` localStorage 里，半衰期约 6.5 次（DECAY 0.9）。

某些场景下用户希望"重置发现态" —— 比如试新命令前清掉旧顺序、隐私偏好重置、debug 排序逻辑。当前**没有清除入口** —— 用户要么手动清 localStorage（DevTools），要么等 score 自然衰减到 prune 阈值（DECAY ≤ 0.05 才 prune）。

加个 `/clearstats` 让 owner 一行重置。

## 改动

### `src/components/panel/slashCommands.ts`

- 新增 export `clearSlashScores()`：清掉 localStorage key
  ```ts
  export function clearSlashScores(): void {
    try {
      localStorage.removeItem(SLASH_HISTORY_KEY);
    } catch {
      // 隐私 / 配额 → 静默；下次重启读到空也是同效果
    }
  }
  ```
- `SLASH_COMMANDS` 在 `/version` 之后插 `{ name: "clearstats", description: "清掉 slash 命令使用历史（重置 /help 与菜单的排序）", parametric: false }`
- `SlashAction` 加 `{ kind: "clearstats" }`
- `parseSlashCommand` 加 `case "clearstats": return { kind: "clearstats" }`

### `src/components/panel/PanelChat.tsx`

`executeSlash` 加 `case "clearstats"`：

```ts
case "clearstats": {
  clearSlashScores();
  pushLocalAssistantNote("🧹 已清掉 slash 命令使用历史。/help 与菜单的排序回到声明默认序。");
  break;
}
```

注：`recordSlashCommandUsage(action.kind)` 在 executeSlash 顶部对所有命令（除 incomplete / unknown / imageHelp）都会调一次 —— 意味着 `/clearstats` 本身**马上**会写一条 score=1 的 entry。这是符合预期的：用户用完 clearstats 后，clearstats 本身就是"最近用过"的命令。如果想做"纯净一次"，需要在 case 体内手动 setItem 把刚记的 score 抹掉 —— 但这是 over-engineering，留着 score=1 完全合理。

## 不做

- 不持久化"上次 clearstats 时间"：用户敲一次就知道了，没必要记 timestamp
- 不弹二次确认：score 表是软偏好，不是不可逆数据；误清不痛
- 不写测试：纯前端，项目无 vitest
- 不复制扩展 `/clearimage` 清图片 prompt 历史：image 历史可在生图界面手动删；不一并清避免误清掉珍贵 prompt

## 验收

- `npx tsc --noEmit` ✅
- 用过几条 slash 命令后，敲 `/clearstats` → "已清掉" 反馈
- 之后 `/help` 输出回到"可用命令："声明序（无 "常用 / 未试过" 分段）

## 完成

- [x] slashCommands.ts: clearSlashScores 导出 + 命令注册 + parser
- [x] PanelChat.tsx: executeSlash case + 导入
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
