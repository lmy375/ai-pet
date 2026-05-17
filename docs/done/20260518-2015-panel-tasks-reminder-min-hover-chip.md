# PanelTasks 行加「⏰ reminderMin」hover chip（iter #496）

## Background

reminderMin marker `[reminderMin: N]` 是 task 到点前 N 分钟软提醒的
metadata。既有写入路径：

- **PanelTasks 右键 ctx menu**：⏰ reminderMin 子面板 5/15/30/60/移除
  五选一（line 17674）
- **后端 handleSetReminderMin**：strip + 追加新 marker → memory_edit
  保存（line 5344）

但 hover chip 入口空缺 — 与本 row hover chip 家族（📂 detail / ↗ refs /
📊 sparkline / ↘ expand / ⏭ +1d / 🔁 schedule / 📅 due-countdown / 📜
raw / 🔇 silent indicator 等）相比少了 reminderMin 这种「常改」metadata
的快速可视入口。owner 想看「这条 task reminderMin 当前几分」要走 右键
→ ctx menu → 看 submenu 主项的 label — 两步。

本 iter 加 hover chip：

- **显当前值**：`⏰ 15m` / `⏰ off`，tint-blue 染底色当 cur>0（让 owner
  hover 即看到 "提前几分提醒"）
- **click → 弹既有 ctx menu**：复用 taskCtxMenu state，open at click
  position with `reminderSubmenu: true` — 即时展 preset 5/15/30/60/移除
  全套 UI 不重写

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴 ↗ refs hover chip 之后插：

```tsx
{taskPreviewHoverTitle === t.title &&
  !isFinished(t.status) &&
  (() => {
    const m = t.raw_description.match(/\[reminderMin:\s*(\d+)\s*\]/);
    const cur = m ? Number(m[1]) : 0;
    return (
      <button
        onClick={(e) => {
          e.stopPropagation();
          setTaskCtxMenu({
            title: t.title,
            status: t.status,
            priority: t.priority,
            x: e.clientX, y: e.clientY,
            prioritySubmenu: false,
            reminderSubmenu: true,  // ← 自动展 ⏰ 子项
            dueInMinSubmenu: false,
          });
        }}
        title={...}
        style={{
          ...
          background: cur > 0
            ? "var(--pet-tint-blue-bg)"
            : "transparent",
          color: cur > 0
            ? "var(--pet-tint-blue-fg)"
            : "var(--pet-color-muted)",
        }}
      >
        ⏰ {cur > 0 ? `${cur}m` : "off"}
      </button>
    );
  })()}
```

### Gates

- **`taskPreviewHoverTitle === t.title`**：500ms hover state — 与 📂 /
  ↗ / 📊 / ↘ / ⏭ / 🔁 / 📅 / 📜 同节奏，避免 always-visible 视觉密度
- **`!isFinished(t.status)`**：done / cancelled 行不显（终态改 reminderMin
  无意义 — handleSetReminderMin 后端也会拒）

## Key design decisions

- **不写新 popover**：taskCtxMenu state 已含 `reminderSubmenu: boolean`
  flag — 设 true 直接展子项；e.clientX/Y 锚位让 popover 紧贴 chip。
  比新写「mini popover at chip anchor」省 200+ LOC + 维护一致性
- **正则与 handleSetReminderMin / 右键 submenu 同源**：未来改 marker
  格式只改一处 — `/\[reminderMin:\s*(\d+)\s*\]/`
- **tint-blue 表"已设"vs transparent + muted 表"未设"**：与 PanelMemory
  /排序 toggle 同 active vs inactive 视觉协议，owner 心智复用
- **`${cur}m` 紧凑显**：与 `[reminderMin: 15]` 全文表达冗余 — chip 视觉
  密度优先，rich text 走 tooltip
- **stopPropagation on click**：防 row click（默认展 detail）误触发；
  chip 是显式点击入口
- **不写 unit test**：纯 React state + 正则 parse + 复用既有 ctx menu
  全套行为。逻辑 trivial（既有 hover chip 家族同 pattern + 既有
  handleSetReminderMin 已有 production 验证）。GOAL.md "meaningful tests
  only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.32s)
- 后端无改动 — 纯前端 chip（复用既有 ctx menu）
- 手测：PanelTasks 任意 active row hover 500ms → chip「⏰ Nm」/「⏰ off」
  出现 → click → taskCtxMenu 弹（reminderSubmenu 展开）→ 选 preset →
  reminderMin marker 更新 → 重新 hover 看到新值
- done / cancelled row hover：⏰ chip 不出（终态 gate 验）

## Future iters (out of scope)

- 「自定义 N」input box（preset 之外的任意分钟值）— 当前 5 个预设够 80%
  场景；input mode 引复杂度收益不匹配
- chip 直接 ↑↓ scroll wheel 改值 — 鼠标轮快速调；引 wheel event 听器 +
  rate-limit 后续 iter 评估
