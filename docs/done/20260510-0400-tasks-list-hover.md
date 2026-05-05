# PanelTasks 任务列表 hover 高亮（Iter R123）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 任务列表 hover 高亮：与 R122 PanelMemory 同模式，task item 行 hover 时 bg 切到 var(--pet-color-bg) 与 card 反差；让用户知道光标位置 + 可点性（任务卡支持点击展开详情）。

## 目标

PanelTasks 任务卡 (`s.item`) 现 bg = card 白底 + border，hover 无变化。
但任务卡支持 click 展开详情（line 2226 onClick），用户没视觉提示哪行
hover 中。R122 已给 PanelMemory 加了同款 hover；本轮镜像到 PanelTasks。

加 hover 高亮：bg 切到 `var(--pet-color-bg)` 反差一档。

## 非目标

- 不动 focus outline（已有 `outline: 2px solid #93c5fd` for focused 行 ——
  键盘导航 highlight，与 hover 是不同维度）
- 不动按钮 / chip / detail expand area 内部 hover —— 那些有自己的 hover
  样式（pet-detail-copy-btn 等）
- 不动其它 panel 的 hover 默认色（mirror R122 即可）

## 设计

### 复用既有 `<style>` block

PanelTasks 已有 `<style>` block（line 1636-）放 `.pet-detail-copy-btn`
hover 样式。本轮加 `.pet-task-card` rule 进同 block：

```diff
       <style>{`
         .pet-detail-section .pet-detail-copy-btn { ... }
         .pet-detail-section:hover .pet-detail-copy-btn { ... }
         .pet-detail-section .pet-detail-copy-btn:hover { ... }
+        .pet-task-card {
+          transition: background-color 0.12s ease;
+        }
+        .pet-task-card:hover {
+          background: var(--pet-color-bg) !important;
+        }
       `}</style>
```

`!important` 反压 inline `s.item` 优先级（与 R122 同思路）。

### className 加到 task card

```diff
 <div
   data-task-idx={idx}
+  className="pet-task-card"
   style={{
     ...s.item,
     ...(focused ? { outline: "2px solid #93c5fd", outlineOffset: "-2px" } : {}),
   }}
 >
```

### detail expand 内层不传染

`s.item` 内部展开时还有 detail 区块（带自己的 padding / 子元素）；
`pet-task-card:hover` 只改 bg，不影响内部布局；detail 区块有自己的 bg
（如 detail textarea / preview block），层级覆盖外层 hover。验证时确认
detail 展开下 hover 不抢戏。

### 测试

无单测；手测：
- light 模式：hover 任务卡 → bg 由白切浅灰
- dark 模式：hover → bg 由暗卡切到深底
- focus + hover 同时生效（outline 是 focus，bg 是 hover）
- detail 展开内部子区块不被外层 hover 染色
- 点击展开详情 → 正常工作（onClick 不被 hover 干扰）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `<style>` block 加 rule + className |
| **M2** | tsc + build |

## 复用清单

- 既有 `<style>` block (line 1636-)
- 既有 `s.item` 样式
- R122 PanelMemory 同款 hover 模式

## 进度日志

- 2026-05-10 04:00 — 创建本文档；准备 M1。
- 2026-05-10 04:08 — M1 完成。既有 `<style>` block 末追加 `.pet-task-card` rule（transition + bg `var(--pet-color-bg) !important`，反压 inline s.item 优先级）；taskCard div 加 `className="pet-task-card"`，与 data-task-idx / focus outline 共存不冲突。
- 2026-05-10 04:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
