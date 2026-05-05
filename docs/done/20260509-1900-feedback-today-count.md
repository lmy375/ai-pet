# PanelDebug 反馈记录"今日累计"计数（Iter R114）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 反馈记录加"今日累计"计数：仿 R108 / R111 同款 logic，"💬 宠物反馈记录（N · M/N 回复）"summary 后追加 "· 今日 X" 小字，让用户感知今日反馈频率。

## 目标

R108 / R111 已给决策日志 / 工具调用历史加"今日累计"计数。反馈记录是同
类 ring buffer 数据，但未补此信号；用户想知道"宠物今天的开口收到了多
少互动"得肉眼数 timestamp。

按相同 logic 加：从 feedbackHistory 全集（不受 kind filter 影响）派生今
日 count，附在 section header summary 之后。

## 非目标

- 不与 kind filter 联动 —— 今日 count 是绝对值
- 不区分 replied/liked/ignored/dismissed 各 kind 的今日数 —— 现 header
  summary 已有总 + 回复比例 + dismissed 突出，再细分会拥挤；用户要详细
  分布点击 chip filter 即可
- 不显 buffer 满 + 暗示（与决策日志同思路；feedback ring buffer cap 较
  大，命中"满"概率更低）

## 设计

### useMemo

```ts
const todayFeedbackCount = useMemo(() => {
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayMs = todayStart.getTime();
  let count = 0;
  for (const f of feedbackHistory) {
    const ts = Date.parse(f.timestamp);
    if (!Number.isNaN(ts) && ts >= todayMs) count++;
  }
  return count;
}, [feedbackHistory]);
```

放 `todayToolCallCount` useMemo 旁边，模式完全一致。

### 渲染

修改 header span 的 summary 文案：

```diff
 <span>
   💬 宠物反馈记录（{feedbackHistory.length}{
     feedbackHistory.length > 0 ? (() => {
       const replied = feedbackHistory.filter((f) => f.kind === "replied").length;
       const dismissed = feedbackHistory.filter((f) => f.kind === "dismissed").length;
       const dismissedSuffix = dismissed > 0 ? ` · 👋${dismissed} 点掉` : "";
       return ` · ${replied}/${feedbackHistory.length} 回复${dismissedSuffix}`;
     })() : ""
   }）
+  {feedbackHistory.length > 0 && (
+    <span
+      style={{ fontWeight: 400, marginLeft: 6, fontSize: 11, opacity: 0.75 }}
+      title="按 timestamp 落本地今日（00:00 起）的反馈条数"
+    >
+      · 今日 {todayFeedbackCount}
+    </span>
+  )}
 </span>
```

opacity 0.75 + fontWeight 400 与 R111 工具历史 today 段同款（"附属"语义）。

`feedbackHistory.length > 0` 守卫防止 0 占位。

### 测试

无单测；手测：
- 启动后无反馈 → 标题不显今日段
- 触发反馈 → "💬 宠物反馈记录（N · M/N 回复 · 今日 X）" 显示
- 跨午夜 → 旧 ts 不再算今日，count reset
- 切 kind filter chip → 今日数不变（不受 filter 影响）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | useMemo + 渲染 |
| **M2** | tsc + build |

## 复用清单

- 既有 R108 / R111 同款逻辑
- 既有 feedbackHistory state + summary 文案路径

## 进度日志

- 2026-05-09 19:00 — 创建本文档；准备 M1。
- 2026-05-09 19:08 — M1 完成。`todayFeedbackCount` useMemo（与 R108/R111 同款 `setHours(0,0,0,0)` 逻辑），依赖 feedbackHistory；section header summary 后追加 "· 今日 X" 小字（fontWeight 400 + opacity 0.75 与 R111 一致 "附属"语义）；feedbackHistory 空时不显（避免 0 占位）。
- 2026-05-09 19:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 1.02s)。归档至 done。
