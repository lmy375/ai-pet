# 任务面板按 due 升序 toggle — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务面板按 due 升序 toggle：当前队列按 compare_for_queue（pri + 状态 + due）排；加一个开关切到"按 due 早→晚"线性排序，让用户对"截止时间近的"建立纯时间维度直觉。

## 目标

`PanelTasks` 队列默认按 `compare_for_queue` 综合排序（status > overdue > pri >
due > created_at）。这种"宠物视角"对宠物执行顺序最优，但对人类规划"截止时间
近的几条"不直观。本轮加一个排序模式开关：
- "队列"（默认）：保留 compare_for_queue 顺序
- "due 升序"：按 due 字段早→晚线性排，无 due 任务排到末尾

## 非目标

- 不做更多排序维度（按 priority / status / created_at）—— "due 早→晚"已覆盖
  "截止时间近"的核心规划场景；多档下拉菜单是过度。
- 不持久化用户的排序偏好 —— panel 内 React state 重启即默认；偏好持久化属于
  另一类需求。
- 不写 README —— 任务面板可见性微调。

## 设计

### 状态

`PanelTasks` 加 `sortMode: "queue" | "due"`，默认 "queue"。

### 排序应用

把现有 `const visibleTasks = tasks.filter(...)...` 的 chain 拆成两步：先 filter，
再按 sortMode 决定是否 sort：

```ts
const filteredTasks = tasks.filter(...).filter(...).filter(...);
const visibleTasks = sortMode === "due"
  ? filteredTasks.slice().sort((a, b) => {
      // 无 due 一律排到末尾
      const ad = a.due ?? "";
      const bd = b.due ?? "";
      if (!ad && !bd) return 0;
      if (!ad) return 1;
      if (!bd) return -1;
      return ad < bd ? -1 : ad > bd ? 1 : 0;
    })
  : filteredTasks;
```

`due` 是 ISO `YYYY-MM-DDThh:mm` —— 字符串比较与时间序一致，无需 Date.parse。

### UI

在「队列（按宠物处理顺序）」标题行右侧加一个 2-button toggle 组（与 viewMode
toggle 风格一致）：

```tsx
<div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
  <div style={s.sectionTitle}>队列{sortMode === "queue" ? "（按宠物处理顺序）" : "（按 due 升序）"}</div>
  <div style={{ display: "flex", gap: 4, fontSize: 11 }}>
    <button onClick={() => setSortMode("queue")} ...>队列</button>
    <button onClick={() => setSortMode("due")} ...>due ↑</button>
  </div>
</div>
```

active 按钮蓝底白字，inactive 灰边白底。

### 测试

逻辑全前端 React state；无 vitest，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | sortMode 状态 + visibleTasks 条件排序 |
| **M2** | 标题行 toggle UI + 标题文案动态化 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 filter 链（不动）
- viewMode toggle 视觉模板（PanelSettings 表单 / 源码切换）

## 进度日志

- 2026-05-05 37:00 — 创建本文档；准备 M1。
- 2026-05-05 37:10 — 完成实现：
  - **M1**：`PanelTasks.tsx` 加 `sortMode: "queue" | "due"` 状态（默认 "queue"）。把 `visibleTasks` chain 拆成 `filteredTasks`（filter 链不变）+ 条件 sort：sortMode === "due" 时 `slice().sort()` 按 due 字符串升序（ISO `YYYY-MM-DDThh:mm` 字典序与时间序一致），无 due 一律到末尾。
  - **M2**：「队列」section 标题行变 flex space-between：左 sectionTitle 文案随 sortMode 动态切换（"按宠物处理顺序" / "按 due 升序"）；右 2-button toggle 组（active 蓝底白字 + cursor default）。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务面板可见性微调。
  - **设计取舍**：仅"队列 / due ↑"两档（多档下拉过度）；不持久化（panel 内 React state 重启即默认，符合"工具偏好不应跨重启锁定"语义）；ISO 字符串字典序 == 时间序，省去 Date.parse。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯 React state + 数据派生由 tsc 保证。
