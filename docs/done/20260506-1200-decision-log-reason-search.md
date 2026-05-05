# 决策日志按 reason 关键词搜索 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 决策日志按 reason 关键词搜索：proactive decision log 现支持按 kind 筛选，加一个搜索框过滤 reason 子串（"为什么这小时一直 Skip" → 输 "cooldown" 直接定位）。

## 目标

`PanelDebug` 的决策日志现支持 4 档 kind 筛选（全部 / Spoke / LlmSilent / Skip）。
但当用户挑了 "Skip" 想知道**为什么** Skip 时，仍要肉眼扫一长串中英混排的
reason。本轮加一个搜索框：输入子串 → 实时过滤可见行，让 "为什么这小时
一直 Skip" 直接落到 "cooldown" / "user active" 那几条。

## 非目标

- 不做正则 / 模糊匹配 —— 子串足够。
- 不做"高亮命中部分" —— 想加但 mono 字体下高亮 mark 与已有 ruleChip 视觉
  容易冲突；先做最小实现，需要再补。
- 不持久化搜索词到 localStorage —— 这是临时 debug 视角，每次开 PanelDebug
  应清空，与 chat search 同语义。

## 设计

### State

- `decisionReasonSearch: string`，与 `decisionFilter` 同级新建 useState。
- 搜索词 trim 后空 = 不过滤；非空 = 子串匹配。

### 匹配范围

针对每条 `ProactiveDecision` `d`，匹配域：
```
`${d.kind} ${d.reason} ${localizeReason(d.kind, d.reason)}`
```
全部 lowercase 后判 `includes(query.toLowerCase())`。

理由：
- 用户输 "cooldown" 命中原始 reason `cooldown (60s < 1800s)`
- 用户输 "冷却" 命中本地化文案 `冷却中 (60s < 1800s)`
- 用户输 "skip" 命中 d.kind `Skip`
- 用户输 "spoke" 同样命中 kind（即使顶部 chip 选 "全部"，搜索仍可定位）

### UI

- 在 PanelFilterButtonRow 同行末尾插一个 `<input>`：placeholder
  "搜索 reason / kind"；宽度 ~140px；clear 按钮（仅非空时显示）。
- 视觉与既有 chip 高度对齐；输入实时驱动过滤（无 debounce — 决策日志
  典型 < 200 条，过滤瞬时）。

### 过滤合成

现有过滤位置：
```ts
const filtered = decisionFilter === "all" ? decisions : decisions.filter(...);
```
扩展为：先按 kind 过滤，再按 search 过滤。空态文案沿用 "当前过滤下没有
匹配条目"（已能描述多重过滤的空态）。

## 测试

`PanelDebug` 是 IO 重的容器组件，无 vitest；纯 tsc + 手测。
搜索逻辑是单行子串匹配，复杂度低；不为它写后端测试。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | decisionReasonSearch state + input UI（含 clear 按钮） |
| **M2** | 接入 filtered 链 + lowercase 子串匹配三域 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `decisionFilter` chip 行
- 既有 `localizeReason` 函数
- 既有 "当前过滤下没有匹配条目" 空态文案

## 进度日志

- 2026-05-06 12:00 — 创建本文档；准备 M1。
- 2026-05-06 12:10 — M1 完成。`decisionReasonSearch` state + flex 同行装 PanelFilterButtonRow + `<input type="search">` + 仅非空时显示的 ✕ 清空按钮。
- 2026-05-06 12:15 — M2 完成。filtered 链拆成 kindFiltered → q-substring filter；haystack 拼 `${kind} ${raw_reason} ${localized}` 后 lowercase 包含匹配，让 cooldown/冷却/Skip 三种输入都能定位同一组条目。
- 2026-05-06 12:20 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 955ms)。归档至 done。
