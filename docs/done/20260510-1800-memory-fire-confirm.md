# PanelMemory "立即处理" 二次确认（Iter R137）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory "立即处理" 按钮二次确认：butler_tasks section header 的"立即处理 (N)"按钮也直接触发 trigger_proactive_turn，与 R125 PanelDebug "立即开口" 同款风险；加 2-step confirm（首点 armed 3s + 红字提示，再点确认）防误触干扰真实节奏。

## 目标

`立即处理 (N)` 按钮在 PanelMemory butler_tasks section 出现，仅 overdue
任务 > 0 时显，点击 = 触发一次 proactive turn。语义与 R125 PanelDebug
"立即开口" 同：绕过 cooldown / quiet hours，强制让宠物跑一次 LLM。误点
干扰真实节奏。

加 2-step confirm（mirror R125）：首点 armed 3s，再点真触发；3s 自动 revert。

## 非目标

- 不动 `handleFireProactive` 既有 trigger 流（保留 await invoke + state
  flag 模式）
- 不限制 overdue=0 时显示 —— 既有逻辑已经"overdue > 0 才显按钮"，本轮不
  改入口
- 不动其它 firingProactive 引用（line 798 的 disabled 视觉）—— 那是请求
  期间灰显语义，与 armed 不同状态轴

## 设计

### state

```ts
const [fireArmed, setFireArmed] = useState(false);
```

放 `firingProactive` 旁边，复用相同 mental model：firing = 请求进行中（async
async 调 invoke），fireArmed = 二次确认 armed（用户已点 1 次还没点 2 次）。

### handler

button onClick 改：

```ts
onClick={() => {
  if (firingProactive) return; // 进行中不响应
  if (!fireArmed) {
    setFireArmed(true);
    window.setTimeout(() => setFireArmed(false), 3000);
    return;
  }
  setFireArmed(false);
  void handleFireProactive();
}}
```

### 三态视觉

| 状态 | 文案 | bg | color |
| --- | --- | --- | --- |
| firingProactive | "处理中…" | `#94a3b8` 灰 | `#fff` |
| fireArmed | "再点确认 (3s)" | `#fef2f2` 浅红 | `#b91c1c` |
| 默认 | "立即处理 (N)" | `#ef4444` 红 | `#fff` |

```diff
 style={{
   ...s.btn,
-  background: firingProactive ? "#94a3b8" : "#ef4444",
-  color: "#fff",
+  background: firingProactive
+    ? "#94a3b8"
+    : fireArmed
+      ? "#fef2f2"
+      : "#ef4444",
+  color: firingProactive ? "#fff" : fireArmed ? "#b91c1c" : "#fff",
   borderColor: "transparent",
+  fontWeight: fireArmed ? 600 : undefined,
   marginLeft: 8,
 }}
 title={
+  fireArmed
+    ? "再次点击立即触发主动开口（3s 内有效）"
+    :
   `${overdueCount} 个任务已过期超过 ${OVERDUE_THRESHOLD_MIN} 分钟。点击立即触发一次主动开口（绕过 cooldown / quiet hours），让宠物现在去看任务列表并选一项处理。点击后 3s 内需再点确认，防误触。`
 }
```

文案随 fireArmed 切换；视觉走"红 → 浅红 → 灰"流（与 R125 同色路径）。

### 测试

无单测；手测：
- overdueCount = 3 → 按钮显 "立即处理 (3)" 红色
- 点 → 切到 "再点确认 (3s)" 浅红 + 红字 + 红边
- 3s 内再点 → 真触发 → 切到 "处理中…" 灰
- 完成 → 回 "立即处理 (3)"（如果还 overdue）/ 不显（overdue=0 时）
- 点 1 次后等 4s → 回默认；再点 1 次再 armed
- 点 1 次后切 catKey / 重渲染 → state 不丢（unmount 才丢）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + click 包装 + 三态视觉 |
| **M2** | tsc + build |

## 复用清单

- 既有 `handleFireProactive` / `firingProactive`
- R125 PanelDebug 立即开口 二次确认模式

## 进度日志

- 2026-05-10 18:00 — 创建本文档；准备 M1。
- 2026-05-10 18:08 — M1 完成。`fireArmed` state 加在 firingProactive 旁；button onClick 改包装：firingProactive 短路 / !fireArmed → setFireArmed + 3s setTimeout / else clearArmed + void handleFireProactive。三态视觉（进行中灰 / armed 浅红+红字+红字粗 / 默认红）；title 随 fireArmed 切换文案。
- 2026-05-10 18:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
