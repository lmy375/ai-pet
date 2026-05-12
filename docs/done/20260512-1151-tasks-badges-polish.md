# PanelTasks 状态 / 优先级徽章抛光（UI 美化 迭代 3）

## 背景

接 UI 抛光迭代 1（全局 CSS）+ 2（PanelPersona Section + 发送按钮）继续。任务面板是用户最高频回访的页面之一，每行的 status badge / priority chip 是视觉信息密度的核心元素，旧实现混用 hardcoded hex 与 tint 变量、dark mode 下部分配色会刺眼。

## 改动

`PanelTasks.tsx`：

### `STATUS_BADGE` 全量迁移到 tint var

| 状态 | 旧 | 新 |
|------|----|----|
| pending | `#e0f2fe` / `#075985` | `var(--pet-tint-blue-{bg,fg})` |
| error | tint orange | 不变 |
| done | `#dcfce7` / `#166534` | `var(--pet-tint-green-{bg,fg})` |
| cancelled | `#f1f5f9` / `#64748b` | `var(--pet-color-bg)` / `var(--pet-color-muted)` |

收益：dark 主题下自动跟随 tint 变量低饱和深色 + 高 lightness 文字，不再有"白底蓝字在深底上反白"的刺眼问题。

### `s.badge()` 样式重写

- `borderRadius` 10 → 999（真正的 pill 形）
- `fontSize` 11 → 10.5
- `padding` 2/8 → 2/9
- 加 `fontWeight: 600` + `letterSpacing: 0.3` —— 短文 pill 立体感更强
- 加 1px `color-mix(in srgb, <fg> 18%, transparent)` 边框 —— 让 pill 在浅色卡片上有"轮廓"，不喧宾夺主

### `s.priBadge()` 同步

- P0 红 hardcoded `#fee2e2` / `#991b1b` → `var(--pet-tint-red-{bg,fg})`
- 同步 pill 形态 / typography / 18% alpha 边框

## 不做

- 不动 pending 行的"按钮状 badge"（status picker 入口）`border: "none"` 覆盖 —— 它需要"裸 chip"语义保留可点击感。
- 不重排 task row 布局 —— 仅 token / typography 抛光，scope 严格收敛。
- 不写测试 —— 纯视觉。

## 验收

- 切到「任务」tab：每行右上角 pending/done 等 pill 形更精致；浅 / 深主题均不刺眼。
- P0 任务红 pill 走 tint red，与 task overdue chip 等其它红色信号语义一致。
- pill 文字小但有 weight + spacing，扫读不糊。
- `npx tsc --noEmit` 通过。

## 完成

- [x] STATUS_BADGE token 迁移
- [x] s.badge() / s.priBadge() pill 化
- [x] 移到 docs/done/
