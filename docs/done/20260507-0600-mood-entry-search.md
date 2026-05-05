# mood 当日 entry 列表搜索 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood 当日 entry 列表搜索：drill 出来的 entries 多时（30+ 条）找特定文字累；加 search 框过滤 entry text 子串，与 motion chip 过滤可叠加。

## 目标

PanelPersona sparkline drill 当日详情现有 motion chip 过滤（"只看 Flick3"）。
但当 30+ 条 entry 里要找"上次提到 X"具体文字时，靠肉眼扫累。本轮在
DayMotionChips 旁加一个搜索框：输入子串 → 过滤 `entry.text`，与 motion
chip 可叠加。

## 非目标

- 不做正则 / 多关键字 OR —— 子串足够；多关键字想用就分多次搜。
- 不持久化搜索词 —— 与既有 entryFilter / 决策日志 reason search 同语义
  （临时 debug 视角，关闭重置）。
- 不做"高亮命中部分" —— 同决策日志 reason 搜索的取舍：mono 字体下
  inline mark 容易与 motion 颜色块视觉冲突；先做最小实现。

## 设计

### state

`entrySearch: string` default ""。与 entryFilter 同级，selectedDate 切换
useEffect 一并 reset 为 ""（避免跨日残留）。

### 过滤合成

`visibleEntries` 计算从单一 motion filter 扩展为：

```ts
const q = entrySearch.trim().toLowerCase();
return dayEntries.filter((e) => {
  if (entryFilter !== null && e.motion !== entryFilter) return false;
  if (q !== "" && !e.text.toLowerCase().includes(q)) return false;
  return true;
});
```

motion + text 双 axis 都通过才显示。

### UI

把 DayMotionChips 包到一个 flex row 里，末尾加 `<input type="search">`：
- 宽度 ~140px（与决策日志 reason 搜索同款尺寸）
- placeholder "搜 entry 文字"
- 非空时旁边显 ✕ clear 按钮（同 reason search ✕ 模式）

仅当 `dayEntries.length > 0` 时整组（chips + search）一起渲染 —— 与现有
DayMotionChips 渲染条件保持一致。

### 空态文案

`visibleEntries.length === 0` 时根据当前 active filter 给出针对性 hint：
- 仅 motion filter → 沿用 "当日无 {motion} entry"
- 仅 text search → "未匹配 「{search}」"
- 同时 → "未匹配「{search}」（在 {motion} 内）"

避免一句"没匹配"让用户搞不清是哪条 filter 太严。

## 测试

PanelPersona 是 IO 重容器；前端无 vitest，靠 tsc + 手测。filter 逻辑是
两段早退判断，复杂度低。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | entrySearch state + selectedDate reset + visibleEntries 双过滤 |
| **M2** | DayMotionChips 同行加 search input + ✕ clear |
| **M3** | 空态文案分情况 + tsc + build + cleanup |

## 复用清单

- 既有 DayMotionChips 组件
- 既有 reason 搜索 ✕ clear 视觉模式
- 既有 selectedDate 切换 useEffect

## 进度日志

- 2026-05-07 06:00 — 创建本文档；准备 M1。
- 2026-05-07 06:10 — M1 完成。`entrySearch` state + `selectedDate` change useEffect 一并 reset；visibleEntries 重写为双 axis 早退过滤（motion + text）。
- 2026-05-07 06:15 — M2 完成。DayMotionChips 包到 flex row 里，末尾 marginLeft auto + 140px search input + 仅非空时显示的 ✕ clear 按钮（同决策日志 reason 搜索 ✕ 模式）。
- 2026-05-07 06:20 — M3 完成。空态文案分三种：仅 search / 仅 motion / 双 active 各自不同 hint，避免一句"没匹配"让用户搞不清是哪条 filter；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 924ms)。归档至 done。
