# PanelTasks task 行 hover「⏱ 已挂 N 小时」chip（iter #408）

## Background

owner 在 PanelTasks 看长队列时，每行展示 priority / due / tags /
status 等信号但没"在队列已多久"的即时聚合。created_at ISO 在
itemMeta hover 才浮出且需心算"现在 - 创建"。

迭代 #407 的 sparkline chip 展现"近 30 天事件分布"，本 iter 互补
"自创建以来在队列多久"信号 — 让 owner 一眼看哪些 task 早就该
做但被压着没动（早期积压 vs 新派单）。

复用既有 `formatRelativeAge` 算法（与 itemMeta 创建时间 chip 同
源），仅 hover 时 + active 状态显，click 复制 created_at ISO 到
剪贴板（小 bonus，外发 / 排查场景方便）。

## Changes

### `src/components/panel/PanelTasks.tsx`（紧贴 ✏ rename chip 之后）

```tsx
{taskPreviewHoverTitle === t.title &&
  (t.status === "pending" || t.status === "error") &&
  (() => {
    const rel = formatRelativeAge(t.created_at, nowMs);
    if (!rel) return null;
    return (
      <button
        type="button"
        onClick={async (e) => {
          e.stopPropagation();
          try {
            await navigator.clipboard.writeText(t.created_at);
            setBulkResultMsg(`📋 已复制 created_at：${t.created_at}`);
          } catch (err) {
            setBulkResultMsg(`复制失败：${err}`);
          }
          setTimeout(() => setBulkResultMsg(""), 2500);
        }}
        title={`这条 task 在队列已 ${rel}（创建于 ${t.created_at}）— 点击复制 ISO 创建时间到剪贴板。仅 active 状态显此 chip。`}
        style={{
          fontSize: 10,
          padding: "0 5px",
          marginLeft: 6,
          border: "1px dashed var(--pet-color-border)",
          borderRadius: 3,
          background: "transparent",
          color: "var(--pet-color-muted)",
          cursor: "pointer",
          fontFamily: "inherit",
          lineHeight: 1.5,
          verticalAlign: "middle",
          whiteSpace: "nowrap",
        }}
      >
        ⏱ {rel}
      </button>
    );
  })()}
```

设计要点：
- **复用 taskPreviewHoverTitle**：与 ✏ rename / 既有 hover preview
  tooltip 同 500ms hover state — 0.5s 后 ✏ chip + ⏱ chip + tooltip
  同时浮起，节奏一致
- **仅 active 状态显**：done / cancelled 的"在队列多久"信号意义弱
  （已离开队列）；只对 pending / error 显避免冗余
- **复用 formatRelativeAge**：分钟 / 小时 / 天三档桶（< 60s → "刚
  创建"），与 itemMeta 创建时间 chip 同语义同输出
- **dashed border + muted color**：与 ✏ rename chip 同视觉重量，
  辅助 affordance 不抢 title
- **click 复制 ISO 而非详情展开**：与既有 chip 集群语义一致（chip
  = 单功能动作）；info 已在 title attr 多行 tooltip 内充分
- **stopPropagation**：防 click 冒泡触发 row expand / select

## Key design decisions

- **不显在 done / cancelled 行**：那些 task 已经离开 active 视野，
  显"已挂 X 小时"会误导（应该用 updated_at - created_at 算"完成
  耗时"，是不同信号）。本 chip 聚焦"积压时长"信号
- **不引入第二个 hover state**：与 iter #397 同复用 taskPreviewHoverTitle，
  让所有 hover-only chip 同时浮起视觉一致
- **不为单 chip 引 unit test**：formatRelativeAge 既有覆盖；本行
  为是 setState + clipboard；build pass + 手测足够（hover row →
  看 ⏱ X 分钟 / 小时 / 天 chip 出现 → click 看剪贴板拿到 ISO →
  hover 看 tooltip 显完整 created_at + 角色提示）
- **bulkResultMsg toast 复用**：与既有 PanelTasks 内复制操作（📋
  schedule prefix / 📋 detail.md 全文 / 📋 选区 blockquote）同
  channel，不引第二条反馈系统

## Verification

- `npx tsc --noEmit`（frontend）— clean（first attempt 有 type
  bug — "Pending" / "Error" 大写不在 TaskStatus enum 中；改
  "pending" / "error" 后 clean）
- `npx vite build`（frontend）— clean (1.30s)
- 后端无改动
