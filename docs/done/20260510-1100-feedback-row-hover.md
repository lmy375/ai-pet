# PanelDebug 反馈记录行 hover 高亮（Iter R130）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 反馈记录行 hover 高亮：与 R122 PanelMemory / R123 PanelTasks 同款 hover bg 反差模式，feedback 行 className + `<style>` rule 让光标位置 / 行边界即时可见。

## 目标

PanelDebug 反馈记录 section 列出 N 条 user 反馈（replied / ignored /
dismissed / liked），每行只有 padding "4px 0" + dashed bottom border。
hover 时无视觉反馈，光标位置不可见。

加 hover bg 高亮：与 R122 / R123 同模式，row 整体 hover 时背景变浅。

## 非目标

- 不动每行内部已有的 kind badge / ts span / excerpt span 配色 —— hover
  仅改外层 bg
- 不在 dark 模式调整专属 hover 色 —— 既有 var(--pet-tint-green-bg) 已是
  绿色 section tint；hover bg 用同 var 颜色加 alpha 即可（实际用
  rgba(0,0,0,0.04) 跨主题更安全）

## 设计

### CSS rule

PanelDebug 已有 R83 决策行 `<style>` block。在那或独立 block 加：

```css
.pet-feedback-row {
  transition: background-color 0.12s ease;
}
.pet-feedback-row:hover {
  background: rgba(0, 0, 0, 0.04);
}
```

不用 var(--pet-color-bg) 因 feedback section 本身已有绿 tint bg；用
`rgba(0,0,0,0.04)` 在 light / dark 主题下都呈 subtle hover overlay
不破坏 section 配色。

### className 加到 row div

```diff
 <div
   key={i}
+  className="pet-feedback-row"
   style={{
     display: "flex",
     gap: "8px",
     alignItems: "center",
     padding: "4px 0",
     borderBottom: i === feedbackHistory.length - 1 ? "none" : "1px dashed #d1fae5",
   }}
 >
```

### 样式块位置

PanelDebug 有现有 `<style>` block at line 1409 区域（决策行 hover）。复
用同 block 加新 rule 减少散乱。

### 测试

无单测；手测：
- light：hover 反馈行 bg 微暗（叠在绿色 tint 上）
- dark：同样 subtle 反差
- 切 kind filter chip → 仍能 hover
- 与 dashed bottom border 不冲突

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `<style>` 加 rule + className |
| **M2** | tsc + build |

## 复用清单

- 既有 R83 `<style>` block
- R122 / R123 hover 模式

## 进度日志

- 2026-05-10 11:00 — 创建本文档；准备 M1。
- 2026-05-10 11:08 — M1 完成。既有 R83 `<style>` block 末追加 `.pet-feedback-row` + `:hover` rule（rgba(0,0,0,0.04) overlay 跨主题安全）；feedback row div 加 className="pet-feedback-row"；与 dashed bottom border 不冲突。
- 2026-05-10 11:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 1.04s)。归档至 done。
