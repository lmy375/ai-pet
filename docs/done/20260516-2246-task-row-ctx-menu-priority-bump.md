# PanelTasks 行右键菜单加 ✦ +1 / ✦ -1 priority 邻近微调

## 背景

PanelTasks 行右键菜单已有 "▸ 改 priority（当前 P{N}）" 展开 submenu —— 10 个 priority slot 一次性显。但 owner 常用动作是"再升一档" / "再降一档"，submenu 多一步点击 + 视觉扫读。

加 ✦ +1 / ✦ -1 两个邻近微调按钮直接微调一档，免开 submenu。

## 改动

### `src/components/panel/PanelTasks.tsx`

priority submenu toggle 按钮之前插入 +1/-1 双按钮区：

```tsx
{(() => {
  const canInc = m.priority < PRIORITY_MAX;  // PRIORITY_MAX = 9
  const canDec = m.priority > 0;
  return (
    <div style={{display: "flex", gap: 4, padding: "0 4px"}}>
      <button
        disabled={!canDec}
        onClick={() => {
          if (!canDec) return;
          setTaskCtxMenu(null);
          void handleInlineSetPriority(m.title, m.priority - 1);
        }}
        title={canDec ? "把优先级从 P3 降到 P2..." : "已是最低 P0，无法再降"}
      >
        ✦ -1 (→P{...})
      </button>
      <button
        disabled={!canInc}
        onClick={() => handleInlineSetPriority(m.title, m.priority + 1)}
        title="..."
      >
        ✦ +1 (→P{...})
      </button>
    </div>
  );
})()}
```

边界禁用：当前 P0 时 -1 disabled / P9 时 +1 disabled，opacity 0.4 视觉降级 + cursor default。tooltip 显具体目标 P 值 + 边界 hint。

## 关键设计

- **复用 handleInlineSetPriority**：与 priority submenu 单 click 同 backend pipeline。one-click pipeline 一致性。
- **clamp [0, PRIORITY_MAX]**：边界禁用按钮防"P10" / "P-1" 非法值。
- **target P 显在按钮 label**："✦ +1 (→P4)" 告诉 owner click 后会去哪 —— 比纯 "✦ +1" 直观。
- **两按钮并排（flex gap 4）**：与既有竖向 menu item 视觉略不同（横向 2 cell grid），让"微调一组" 与其它"决策操作"分组。
- **不替换 priority submenu**：保留 submenu 给"我要跳到 P7" 远距离切换；+1/-1 仅给微调。
- **不写测试**：纯 click handler + 既有 handleInlineSetPriority（已验证）；视觉验证（开 ctx menu → 当前 P3 → 点 ✦ +1 → row 显 P4 + menu 关）足够。

## 不做

- **不绑键盘快捷键**：菜单内按钮就够近 reach；hotkey 反而抢系统级。
- **不批量 +1 多 task**：与既有 bulk priority 选区批改 priority 重复。
- **不写 +1 后 menu 仍开 + priority 数字 inline 更新**：menu 关 = 显式动作完成反馈；行更新 + queue 重 sort 自然可见。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.18s
- 改动 ~75 行（IIFE + 双按钮 div + 注释）。既有 priority submenu / 5-grid render / handleInlineSetPriority pipeline / 其它菜单 entries 完全不动。

## TODO 状态

剩 5 条留池：
- detail.md 编辑器 textarea ⌘⇧Enter 保存并关闭
- butler_task 行 [reminderMin: N] chip click 弹快速编辑
- PanelMemory ai_insights banner 加 daily_review 计数链接
- TG /markers 命令一次列 pinned + silenced
- pet 区 hover 显本机时区 chip 浮卡

## 后续

- ⌥+click +1/-1 按钮 = +2/-2 跳两档（细 / 粗调对偶）。
- 加 "↑ MAX" / "↓ MIN" 按钮一键到 P0 / P9（owner 急着"这条放最后" / "这条最高优先"）。
- bulk ctx menu 也加 +1/-1 让选区一起升降。
