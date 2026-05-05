# PanelTasks priBadge / btnPrimary / empty 配色迁 token（Iter R154）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks priBadge / btnPrimary / empty 配色迁 token：line 1372 priBadge
> `#fef3c7`+`#92400e` → tint-yellow；line 1373 btnPrimary `#0ea5e9`/`#fff` →
> accent + card；line 1376 empty `#94a3b8` → muted。一次性把 styles 对象内
> 剩余 hardcoded 收口。

## 目标

PanelTasks `s` styles 对象内还剩三组 hardcoded：

| line | key | from | to |
| --- | --- | --- | --- |
| 1372 | priBadge.background | `#fef3c7` | `var(--pet-tint-yellow-bg)` |
| 1372 | priBadge.color | `#92400e` | `var(--pet-tint-yellow-fg)` |
| 1373 | btnPrimary.background | `#0ea5e9` | `var(--pet-color-accent)` |
| 1373 | btnPrimary.color | `#fff` | `var(--pet-color-card)` |
| 1376 | empty.color | `#94a3b8` | `var(--pet-color-muted)` |

## 非目标

- 不动 line 1374 btnDisabled `#94a3b8` / `#fff`：这是 disabled 状态自己的视觉
  约定（不该跟主题切换太多，保持灰板感）。下轮如要彻底统一可单独迁。
- 不动 line 1438 tagChip `#f1f5f9` / `#475569`：tag 是另一语义（neutral
  chip），与本轮 priority/primary/empty 三组无关。
- 不动 STATUS_BADGE 其它三档（pending/done/cancelled）—— 各有色相，本轮
  不收口。

## 设计

### priBadge → tint-yellow

priBadge 当前是 yellow-200 bg + yellow-800 fg：与 R7 既有 tool-history 黄
section 的 tint-yellow-bg/fg 同色相同语义（"提醒"族），直接用 tint。

### btnPrimary → accent + card

light：accent #0ea5e9 + card #fff → 与原 #0ea5e9 + #fff 完全一致
dark：accent #38bdf8 + card #1e293b → 亮 cyan 底 + 深底文字，反相但都是
高对比，标准 dark 主按钮设计。

card token 在两主题下分别是白 / 深蓝，文字色跟主题切，与按钮 bg 形成
"亮底深字 / 浅底白字"的常见 button 反相规则。

### empty.color → muted

empty placeholder 灰 #94a3b8 与 muted dark token 完全相等；light 下 muted
是 #64748b 比 #94a3b8 略深（更易读），整体 empty 提示在 light 主题下也
更清晰。这是顺带的 a11y 改进。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 三处 styles 对象 key 替换 |
| **M2** | tsc + build |

## 复用清单

- iter 1 framework tokens
- iter 7 tints

## 进度日志

- 2026-05-11 11:00 — 创建本文档；准备 M1。
- 2026-05-11 11:20 — M1 完成：priBadge / btnPrimary / empty 三键替换；
  M2 tsc + build 通过。归档。
