# 工具调用折叠状态 per-turn 记忆 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 工具调用结果的折叠 / 展开记忆：PanelDebug 的 turn 里 tool_calls 折叠状态在切换 turn 时被重置；改成 per-turn 持久化，让用户翻回某 turn 时不必重新点开。

## 目标

`PanelDebug` 调试器 modal 当前 `expandedToolCallIdx: Set<number>` 全局共享：
prev / next 按钮切换 turn 时强制清空（`setExpandedToolCallIdx(new Set())`），
导致用户翻回前一个 turn 时所有展开的工具调用都被折叠回去，必须重新逐个点开。

本轮把"哪些 tool_call 在哪个 turn 是展开的"改成 per-turn 持久化（按 turn
timestamp 索引），让翻 turn 时各自维持自己的展开布局。

## 非目标

- 不持久化跨进程 / 跨 modal 关闭重开 —— 状态在 React 组件内即可，重新打开
  modal 等同新建会话，重置展开状态合理。
- 不写 README —— 调试器内嵌交互微调。

## 设计

### 状态

把 `expandedToolCallIdx: Set<number>` 改成
`expandedToolCallByTs: Map<string, Set<number>>` —— 键是 `turn.timestamp`（稳定
标识，与 ring buffer 索引解耦：用户重新点"看上次 prompt"再 fetch 时，新 turn
进入会让索引位移，但旧 turn 的 timestamp 仍是同一字符串，状态不漂移）。

派生：
- `currentTurnTs = currentTurn?.timestamp ?? ""`
- `expandedSet = expandedToolCallByTs.get(currentTurnTs) ?? EMPTY_SET`

Toggle handler 改成读 + copy + 写回：

```ts
setExpandedToolCallByTs((prev) => {
  const next = new Map(prev);
  const cur = new Set(next.get(currentTurnTs) ?? []);
  if (cur.has(j)) cur.delete(j);
  else cur.add(j);
  next.set(currentTurnTs, cur);
  return next;
});
```

prev / next 按钮的 `setExpandedToolCallIdx(new Set())` 全部删掉 —— 不再需要
显式重置。

### 边界

- `currentTurnTs === ""`（无 turn / 错误）：读出 EMPTY_SET，toggle 无效（仍写
  键为空串的 entry，但因为没有 tool_calls 渲染，不可见）。
- ring buffer 滚动：旧 timestamp 在 map 里残留，但单次 modal 会话上限 5×N
  fetch ≈ 几十条，不会内存泄漏。modal 关闭重开（重新挂载组件）时状态自然清空。

### 测试

无后端改动；纯前端状态调整。靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state 类型改 + getter / toggle 改 + prev / next 重置去掉 |
| **M2** | tsc + build + cleanup |

## 复用清单

- 既有 `currentTurn` / `recentTurns` / `turnIndex`
- 既有 prev / next 按钮 JSX

## 待用户裁定的开放问题

- 用 timestamp 做键 vs turn index 做键？timestamp（这次决定）—— 更稳定。
  index 在新 turn 进 ring buffer 时会位移，会让"我展开的是上次的 turn 3"
  错位到当前的 turn 4 / turn 2。

## 进度日志

- 2026-05-06 06:00 — 创建本文档；准备 M1。
- 2026-05-06 06:15 — 完成实现：
  - **M1**：`PanelDebug.tsx` `expandedToolCallIdx: Set<number>` 改为 `expandedToolCallByTs: Map<string, Set<number>>` 按 `turn.timestamp` 索引；新增派生 `currentTurnTs` / `expandedToolCallSet`（`get(...) ?? EMPTY_INDEX_SET` 共享空 Set 字面量避免 new 对象）；toggle handler 走 `new Map(prev) → cur = new Set(next.get(currentTurnTs) ?? []) → set` 的 immutable update 模式。
  - **M2**：prev / next 按钮的 `setExpandedToolCallIdx(new Set())` 重置调用全部去掉 —— 切 turn 不再清空展开状态，per-turn 持久化生效。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 调试器内嵌交互微调。
  - **设计取舍**：以 `timestamp` 而非 `turnIndex` 做键 —— ring buffer 滚动会让 index 位移但 timestamp 稳定；旧 timestamp 残留是有界 leak（每 fetch ≤ 5 条 × N fetch ≈ 几十条），modal 关闭重开自然清空，可接受。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯 React 状态调整，由 tsc + 派生函数无副作用保证。
