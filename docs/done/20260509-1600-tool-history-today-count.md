# PanelDebug 工具调用历史"今日累计"计数（Iter R111）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 工具调用历史加"今日累计"计数：仿 R108 决策日志同款 logic，section header "🔧 工具调用历史（N）"旁附 "今日 X 次"小字（按 timestamp 落本地今日 count），让 debug 时直观感知今日工具使用频率。

## 目标

R108 给决策日志 filter 行尾加了"今日 N"计数。工具调用历史 section 是同
类 ring buffer 数据，但没此信号；用户想知道"宠物今天到底调了多少次工
具"得肉眼数 timestamp。

仿同款 logic：从 toolCallHistory 全集（不受 risk_level filter 影响）派生
今日 count，显在 section header "🔧 工具调用历史（N）" 之后。

## 非目标

- 不改后端 ring buffer 容量
- 不与 risk filter 联动 —— 今日 count 是绝对值；filter 是过滤维度，不该
  互相干扰
- 不显 buffer 满 + 暗示（与 R108 不同 —— 工具历史 buffer 容量 cap 比 16
  大，命中"满"概率较低；保持简洁）

## 设计

### useMemo

```ts
const todayToolCallCount = useMemo(() => {
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayMs = todayStart.getTime();
  let count = 0;
  for (const c of toolCallHistory) {
    const ts = Date.parse(c.timestamp);
    if (!Number.isNaN(ts) && ts >= todayMs) count++;
  }
  return count;
}, [toolCallHistory]);
```

放 `todayDecisionCount` useMemo 旁边，模式完全一致（仅源数据 / 字段不同）。

### 渲染

修改 section header 的 "🔧 工具调用历史（N）" 文案：

```diff
 <span>
-  🔧 工具调用历史（{toolCallHistory.length}）
+  🔧 工具调用历史（{toolCallHistory.length}）
+  {toolCallHistory.length > 0 && (
+    <span
+      style={{
+        fontWeight: 400,
+        marginLeft: 6,
+        fontSize: 11,
+        opacity: 0.75,
+      }}
+      title="按 timestamp 落本地今日（00:00 起）的工具调用条数"
+    >
+      · 今日 {todayToolCallCount}
+    </span>
+  )}
 </span>
```

`opacity: 0.75` 让"今日 N"看着"附属于主标题"而非抢视觉。`fontWeight: 400`
（标题本身是 600）加深"附属"语义。

`toolCallHistory.length > 0` 守卫：空 history 时已显示"暂无工具调用记录"，
今日 0 也无意义。

### 测试

无单测；手测：
- 启动后无调用 → 标题不显今日段
- 触发几次 reactive chat → 工具调用入队，标题显 "🔧 工具调用历史（N） · 今日 X"
- 跨午夜后 → 旧 ts 不再算今日，count reset
- 切 risk filter → 今日数不变（不受 filter 影响）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | useMemo + 渲染 |
| **M2** | tsc + build |

## 复用清单

- 既有 R108 todayDecisionCount 同款逻辑
- 既有 toolCallHistory 数据
- 既有 section header layout

## 进度日志

- 2026-05-09 16:00 — 创建本文档；准备 M1。
- 2026-05-09 16:08 — M1 完成。`todayToolCallCount` useMemo（R108 同款 `setHours(0,0,0,0)` 算午夜阈值），依赖 toolCallHistory；section header 在 "🔧 工具调用历史（N）"后追加 "· 今日 X" 小字（fontWeight 400 + opacity 0.75 表"附属"）；toolCallHistory 空时不显（避免 0 占位）。
- 2026-05-09 16:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
