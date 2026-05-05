# 深色 / 浅色主题（迭代 8）— PanelPersona 核心 surface 迁移

> 对应需求（来自 docs/TODO.md）：
> 把 PanelPersona 的 inline color 迁到 var(--pet-color-*)；mood 列表 / sparkline / 当日详情等核心 surface，保留 motion 色族 + sparkline 色阶。

## 目标

最后一个未迁的大 panel（1727 行 / 95 处 hex）。完成后 4 个 panel 全部走
token 系统，dark 主题在主要交互面下都能切换。

迁移点（按区块）：

1. **`Section` wrapper**（顶部所有"陪伴时长 / 自我画像 / 心情谱 / ..."卡片
   外框）—— bg / border / 标题 fg / subtitle muted
2. **SparklinePopover**（sparkline hover tooltip）—— bg / border / fg / 日期 muted
3. **Sparkline 空日 baseline + AM/PM 分隔线**—— track #e2e8f0 → border；
   AM/PM 分隔线 #fff → border
4. **当日详情 entry 列表**—— 各按钮 / 输入 / 行内文字
5. **散落框架色**：
   - `color: "#94a3b8"` → muted（多处 placeholder / hint / 时间戳）
   - `color: "#475569"` → fg（body 文字）
   - `color: "#1e293b" / "#0f172a"` → fg（强调标题）
   - 单点 `color: "#64748b"` → muted（注意：MOTION_META.Idle.color 也是
     `#64748b`，但作为对象字面量 `color:` 后跟分号在源码中是另一种语法上下文，
     需要逐一手动确认避免动到 Idle motion 色）
   - `border: "1px solid #cbd5e1"` / `"1px solid #e2e8f0"` → border
   - `borderTop: "1px dashed #e2e8f0"` → border 同样写法
   - `background: "#fff"` → card（除 sparkline AM/PM 分隔线）

## 非目标 — 保留 motion 语义色

- **MOTION_META 4 色族**（Tap 粉 `#ec4899` / Flick 黄 `#f59e0b` / Flick3 橘
  `#ea580c` / Idle 灰 `#64748b`）—— 所有 mood / motion 视觉的根色，跨主题
  保持
- **陪伴天数 teal `#0d9488`** —— 关系性时长强调色
- **persona-summary stale 红 `#dc2626`** —— "consolidate 长时间没跑"警示
- **danger 边框 `#fecaca`** —— 删按钮配色
- **sparkline 选中 outline `#0ea5e9`** —— 走 accent token? 暂保留 hex（与
  sparkline 像素栅格紧绑）
- **AM/PM 分隔线**：当前 `#fff` 在 light 模式下 = 白色细线在堆叠 motion
  bar 中央切成两段。dark 下 `#fff` 太刺眼，需切到 border token。
- **DayMotionChips / ChipButton 的 motion 色填充 + 文字** —— motion 色直接
  当 chip 的颜色源（`color={meta.color}`），保持。
- **persona summary `#1e293b` 主体文字** → 走 fg token（不是 motion）

## 设计

### 阶段拆分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 特殊点 2 处（sparkline track #e2e8f0、AM/PM 分隔线 #fff）；这两处必须先做，避免下面 replace_all 误伤 |
| **M2** | replace_all 4 个 muted 色：`color: "#94a3b8"` → muted |
| **M3** | replace_all `color: "#475569"` / `color: "#1e293b"` / `color: "#0f172a"` → fg |
| **M4** | replace_all `border: "1px solid #cbd5e1"` / `border: "1px solid #e2e8f0"` / `borderTop: "1px dashed #e2e8f0"` → border |
| **M5** | replace_all `background: "#fff"` → card（已先把 AM/PM 分隔线挪走） |
| **M6** | 手动迁 `color: "#64748b"` 的非-MOTION_META 处（Idle motion 字面量保留） |
| **M7** | 手动迁 `border: "1px dashed #e2e8f0"` （非 borderTop）+ stale 提示外的杂项 |
| **M8** | tsc + build + 手测 |

### 测试

无单测；手测：
- light：与切换前完全一致
- 切 dark：Section 卡片 / popover / sparkline 空日 / AM-PM 分隔 / hover tooltip / 当日详情 全部跟着切深；MOTION_META 配色 / chip 选中色 / 陪伴 teal / stale 红 全部保留

## 复用清单

- iter1-7 token 系统
- iter7 的 6 对 tint（PanelPersona 没用 tinted section，不涉及 tint）

## 进度日志

- 2026-05-08 13:00 — 创建本文档；准备 M1。
- 2026-05-08 13:08 — M1 完成。sparkline 空日 baseline (#e2e8f0) → border；选中 outline (#0ea5e9) → accent；AM/PM 分隔细线 (#fff) → border（dark 下 visible 关键）；3 处 sparkline outline 由 sed 批量改 accent token。
- 2026-05-08 13:15 — M2-M5 完成。replace_all 批量迁：`color: "#94a3b8"` → muted；`color: "#475569"` / `"#1e293b"` / `"#0f172a"` → fg；`border: "1px solid #cbd5e1"` / `"1px solid #e2e8f0"` → border；`borderTop: "1px dashed #e2e8f0"` → border；`background: "#fff"` → card。MotionFilterChips 全部 chip "全部" 默认态 (#0ea5e9 accent + #e2e8f0/#fff/#475569 default) 全迁 token。
- 2026-05-08 13:22 — M6/M7 完成。手动迁 conditional ternary 内 hex（不含 `color: ` 前缀的 replace_all 不到的）：userName trim fg/muted、stale red 保留 / muted / 整理 disabled bg、splitHalfDay cyan 保留 fg 替、disabled text fg、空 chip 6/cbd5e1 → muted、copied green 保留 fg 替、ChipButton "全部" color prop = muted、MotionFilterChips per-motion inactive color/bg/border 全 token。
- 2026-05-08 13:25 — M8 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 983ms)。剩 hex 仅 motion 色族（MOTION_META 4 色 / FALLBACK_MOTION_COLOR / 陪伴 teal #0d9488 / consolidating 紫 #8b5cf6 / disabled 灰 #94a3b8 等 ternary 一侧 / stale #dc2626 / 清理 #b91c1c #fecaca / splitHalfDay 浅蓝 + 浅灰 / 历史 d-toggle 浅灰 / copied #16a34a），全部按 plan 保留 motion。归档至 done。
