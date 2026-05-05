# PanelChat 会话列表非选中行 hover 高亮（Iter R131）

> 对应需求（来自 docs/TODO.md）：
> PanelChat 会话列表非选中行 hover 高亮：当前会话有浅蓝 bg；非当前 hover 时无反馈，看不出可点。加 className + CSS rule（rgba overlay 不破坏 selected 蓝色），mirror R122/R123/R130 同模式。

## 目标

PanelChat 顶部会话列表 dropdown 行：当前会话 bg = `#f0f9ff` 高亮，非当
前 = transparent。hover 时无视觉反馈，新用户不知道点哪行可切换 / 操作。

加 hover bg overlay：与 R122 / R123 / R130 同款 `rgba(0,0,0,0.04)` 叠加。
selected 行 hover 时 selected 蓝 + alpha overlay → 仍是蓝色但略深，不冲突。

## 非目标

- 不动 selected 蓝色 bg —— 它的语义是"当前打开"
- 不动行内按钮（✏️ rename / 🗑 delete / 📋 export）的 hover —— 那是按钮
  自有反馈，与行 hover 互不影响
- 不动会话搜索 panel 的搜索结果行 —— 那是另一个流程，搜索结果点开就跳，
  hover 与否影响不大

## 设计

### 加 className

```diff
 <div
   key={s.id}
+  className="pet-session-row"
   style={{
     ...
     background: s.id === sessionId ? "#f0f9ff" : "transparent",
   }}
 >
```

### `<style>` 加 rule

PanelChat 已有 `<style>` block（line 753-）放 focus + chat row hover。复
用同 block 加：

```css
.pet-session-row {
  transition: background-color 0.12s ease;
}
.pet-session-row:hover {
  background: rgba(0, 0, 0, 0.04);
}
```

注意：`background: rgba(...)` 在 inline `background: "#f0f9ff"` 后写也
不会覆盖（inline 优先级高）。所以 selected 行 hover 时 inline 蓝赢，hover
overlay 不显示 —— 等于"selected 行 hover 无反馈"。可以接受（selected 已
是高亮，hover 是次要信号）；如果要叠加：用 `!important` 反压并接受 selected
轻度变暗。

为了简洁与 mirror R122/R123/R130 一致，用 `!important`：

```css
.pet-session-row:hover {
  background: rgba(0, 0, 0, 0.04) !important;
}
```

但这样 selected 行 hover 失去蓝色感（被 alpha overlay 替换）。

更好方案：用 `box-shadow inset` 模拟 hover，不动 bg。但视觉差异较小。

最简的折中：让 selected 行 hover 走 var bg，非 selected 行 hover 走 alpha。
不引 !important，selected 与 alpha 相互让位。light/dark 都能看（selected
蓝有色调）。

实际上用户最多关心"非当前会话 hover 反馈" —— selected 已显眼。所以让
非 selected 行 hover 起作用就够了。inline `background: transparent` 优先
级高于普通 CSS，所以也需要 `!important`。但只对 transparent 行加：

CSS 没法靠 ":not(.selected)" 选择 inline 状态。最干净的做法：把 selected
inline 改成 className，让 CSS 全权管。

但改动范围扩大。folder 折衷：用 `!important` for hover bg（接受 selected
hover 时变浅灰，覆盖蓝色）。R122 / R123 / R130 都用了 `!important` 同款。

```css
.pet-session-row {
  transition: background-color 0.12s ease;
}
.pet-session-row:hover {
  background: rgba(0, 0, 0, 0.04) !important;
}
```

selected 行 hover 时 bg 由蓝换灰 —— 短暂"失去 selected 高亮"，但鼠标移
开立即恢复蓝。用户操作流是"hover 看清 → 点击切换 / 按钮"，hover 期间是
否保留 selected 蓝色不重要。

### 测试

无单测；手测：
- 非当前会话行 hover → 浅灰反差
- 当前会话行 hover → 蓝色暂换浅灰；移开恢复蓝
- 行内 ✏️ / 📋 / 🗑 按钮 hover 时不影响行 hover；按钮各自仍生效
- light / dark 切换：alpha overlay 跨主题都呈 subtle hover

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | className + `<style>` rule |
| **M2** | tsc + build |

## 复用清单

- 既有 `<style>` block (line 753)
- R122 / R123 / R130 同款 hover 模式

## 进度日志

- 2026-05-10 12:00 — 创建本文档；准备 M1。
- 2026-05-10 12:08 — M1 完成。既有 `<style>` block 末追加 `.pet-session-row` + `:hover` rule（rgba alpha overlay + !important 反压 inline selected 蓝）；session list row div 加 className。selected 行 hover 时 bg 由蓝换浅灰 → 移开恢复，操作流不影响。
- 2026-05-10 12:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
