# mood entry 列表展开 / 折叠 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood entry 列表展开 / 折叠记忆：当日详情列表多于 20 条时高度暴涨；加 "展开全部 / 收起" 按钮，默认折叠超 20 条仅显前 20。

## 目标

drill 当日 entry 列表已支持 motion 过滤 + text 搜索 + 单条 / 整段复制。
但有的日子情绪记录 50+ 条，drill 一打开页面被撑爆，sparkline 都看不到了。
本轮加默认折叠：超 20 条仅显前 20，末尾按钮"展开剩余 N 条 / 收起"切换。

## 非目标

- 不持久化展开偏好 —— 每次切日重置（与 entryFilter / entrySearch 同语义）。
- 不做"分页加载" —— 后端一次性把当日所有 entries 取回内存，前端切片 0
  延迟；分页是不必要的复杂度。
- 不做"前 20 / 后 20" / 多 anchor —— 用户已能 search 找特定文字 / 跨日
  导航，分多 anchor 没必要。

## 设计

### state

`entryListExpanded: boolean` default false。`selectedDate` 切换 useEffect
一并 reset。

### 阈值常量

`ENTRY_LIST_DEFAULT_LIMIT = 20`（与 task 面板若干 N 限的默认 20 一致）。

### 切片

```ts
const overLimit = visibleEntries.length > ENTRY_LIST_DEFAULT_LIMIT;
const displayedEntries = !entryListExpanded && overLimit
  ? visibleEntries.slice(0, ENTRY_LIST_DEFAULT_LIMIT)
  : visibleEntries;
```

`.map` 改读 displayedEntries。末尾插一个按钮（仅 overLimit 时渲染）。

### 按钮文案

- 折叠态："展开剩余 {N} 条"（N = total - limit）
- 展开态："收起，仅显前 20 条"

按钮样式与 "复制为 MD" / 关闭 ✕ 同款（10px 字、灰边、白底）。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + reset + slice + 按钮 |
| **M2** | tsc + build + cleanup |

## 进度日志

- 2026-05-07 20:00 — 创建本文档；准备 M1。
- 2026-05-07 20:10 — M1 完成。`entryListExpanded` state default false；selectedDate 切换 useEffect 一并 reset；`.map(visibleEntries)` 包到 IIFE 内 → `LIMIT=20` → 切片 → 渲染 + overLimit 末尾 toggle 按钮（"展开剩余 N 条 / 收起"）。修了一处 JSX 闭合 mismatch（外层 `<>` + `: (...)` 的关闭）。
- 2026-05-07 20:15 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 938ms)。归档至 done。
