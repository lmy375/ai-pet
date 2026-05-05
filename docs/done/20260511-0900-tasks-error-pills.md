# PanelTasks 错误 / inline error pill 配色迁 token（Iter R152）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 错误 / inline error pill 配色迁 token：line 1375 / 2294 / 2587 /
> 2817 / 2885 / 3006 多处 hardcoded `#fef2f2` / `#dc2626` / `#b91c1c`，
> dark 主题下不一致。统一迁到 `tint-orange` family，按 R147~R150 既有 mapping。

## 目标

PanelTasks 是新加的任务面板，token 化迁移落后于 PanelDebug / PanelChat /
PanelSettings。本轮收尾该面板内 6 处与"错误 / 警告"语义绑定的 hardcoded
红色：

| line | 用途 | hardcoded | 替换 |
| --- | --- | --- | --- |
| 1375 | `s.err` 错误块 | bg `#fef2f2` + color `#b91c1c` | tint-orange-bg / tint-orange-fg |
| 2294 | "有更新"指示点 | color `#dc2626` | tint-orange-fg |
| 2587 | detail_md IO 失败 ⚠ | color `#dc2626` | tint-orange-fg |
| 2817-2819 | cancelEditArmed 按钮 | bg `#fef2f2` + border `#dc2626` + color `#b91c1c` | tint-orange-bg / fg / fg |
| 2885 | history IO 失败 ⚠ | color `#dc2626` | tint-orange-fg |
| 3006 | 🆕 新事件指示 | color `#dc2626` | tint-orange-fg |

## 非目标

- 不动 line 1372 priBadge `#fef3c7` / `#92400e`（priority 黄，非错误语义；
  下轮可单独迁 yellow tint）。
- 不动 line 1373 btnPrimary `#0ea5e9` (= accent 同值，但 inline 写死；下轮
  迁 framework `var(--pet-color-accent)`)。
- 不动 line 1374 btnDisabled `#94a3b8` (≈ muted，但语义不全等)。
- 不动 line 1376 empty `#94a3b8`（这一处迁 muted token 是 trivial 改动，
  本轮 scope 控住"错误 pill"）。

不在本轮：以上四类（priority / accent / disabled / empty）合并下轮一次迁完
更整洁，避免本轮 patch 散到不相关 style key。

## 设计

### 替换规则

所有 `#dc2626` → `var(--pet-tint-orange-fg)`
所有 `#b91c1c` → `var(--pet-tint-orange-fg)` (二者都是红 600/700 阶 fg；
  R147 决策清空 armed 同样把这俩合并成 orange-fg)
所有 `#fef2f2` → `var(--pet-tint-orange-bg)`
borderColor `#dc2626` → `var(--pet-tint-orange-fg)`

注意：`#dc2626` 在文件内可能还出现于其他**非错误**位置；本次只改这 6 处
枚举 line。逐 Edit，不 replace_all。

### 视觉保真

light：
- bg #fef2f2 → #fff7ed (浅红→浅暖橙) — 警示底色相近
- color #b91c1c / #dc2626 → #9a3412 — 红→棕橙，所有"错误文字"统一橙

dark：
- bg → #2b1f10 (暗暖底)
- color → #fdba74 (亮橙) — 暗底高对比，远好于原 #dc2626 在 dark card 上

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 6 处替换 |
| **M2** | tsc + build |

## 复用清单

- iter 7 tints
- R147 / R149 / R150 / R151 "orange = 警示"先例

## 进度日志

- 2026-05-11 09:00 — 创建本文档；准备 M1。
- 2026-05-11 09:20 — M1 完成：6 处 (line 1375 / 2294 / 2587 / 2817-2819 /
  2885 / 3006) 全迁 tint-orange；M2 tsc + build 通过。同文件还有 line 62 /
  142 / 219 / 1430 / 1468 / 2835 是其它语义（status indicator / errorMsg /
  pri 主题）的 hardcoded 红，不在本轮 scope，作为下轮 TODO 补充。归档。
