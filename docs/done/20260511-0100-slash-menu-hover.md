# SlashCommandMenu 非选中行 hover 高亮（Iter R144）

> 对应需求（来自 docs/TODO.md）：
> SlashCommandMenu 非选中行 hover 高亮：现仅 selected idx 有蓝 bg；mouse 用户 hover 其它行无视觉反馈。className + `:hover:not(.selected)` rgba overlay（与 R122/R131/R133 同模式）。

## 目标

SlashCommandMenu 列表中：键盘 ↑/↓ 选中的 idx 有 selected 蓝 bg；鼠标 hover
其它行（非 selected）时无视觉变化。鼠标用户在多命令列表中扫不到光标位置。

加 hover bg overlay：与 R122 / R131 / R133 同款 rgba(0,0,0,0.04)；selected
行 hover 时 inline 蓝 bg 优先级（无 important）保留 — 不破坏选中态。

## 非目标

- 不动 selected idx 的 inline 蓝 bg
- 不动键盘 ↑/↓ 选中逻辑（既有父组件 PanelChat 处理）
- 不动 onMouseDown 触发 onSelect 路径

## 设计

### CSS rule

SlashCommandMenu 单文件、仅这一个组件用样式。在组件返回前 `<style>` 标签
内嵌（最简方法，避免 css module / scoped 引入新文件）：

```tsx
<style>{`
  .pet-slash-row {
    transition: background-color 0.12s ease;
  }
  .pet-slash-row:hover {
    background: rgba(0, 0, 0, 0.04);
  }
`}</style>
```

不需要 `!important`：
- selected 行有 inline `background: "#e0f2fe"` → 优先级高于普通 CSS
- non-selected 行 inline `background: "transparent"` → 普通 CSS rule 能赢

selected hover 时 inline 蓝 bg 仍 winning，hover 失效（OK — 选中态已显眼）。

### className 加到 row

```diff
 <div
   key={cmd.name}
+  className="pet-slash-row"
   data-slash-idx={i}
   onMouseDown={(e) => {...}}
   style={{...selected ? "#e0f2fe" : "transparent"...}}
 >
```

### `<style>` 在组件 root 渲染

放在 menu container `<div style={menuContainerStyle}>` 内最顶部：

```tsx
<div ref={listRef} style={menuContainerStyle}>
  <style>{`...`}</style>
  {commands.map(...)}
</div>
```

或外层（fragment）。inside container 让样式紧贴所属组件。

实际两处都行；放 inside container 更内聚。

### 测试

无单测；手测：
- 键盘 ↑/↓ 选中第 2 项 → 第 2 项蓝 bg 不变
- 鼠标 hover 第 1 项（非 selected）→ 浅灰 overlay
- 鼠标 hover 第 2 项（selected）→ 蓝 bg 不变
- 移开 → 各自恢复
- 没匹配命令时（line 28-）只有 placeholder div，不应用 hover（无 className）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `<style>` 加 rule + className |
| **M2** | tsc + build |

## 复用清单

- 既有 R122 / R131 / R133 hover overlay 模式
- 既有 selected idx + 键盘控制路径

## 进度日志

- 2026-05-11 01:00 — 创建本文档；准备 M1。
- 2026-05-11 01:08 — M1 完成。SlashCommandMenu 容器内顶部插 `<style>` 块定义 `.pet-slash-row` + `:hover` rgba 0.04 rule（无 !important，selected inline 蓝 bg 优先级 winning 自然保留）；commands.map 行加 className。empty placeholder 不应用 className 不受影响。
- 2026-05-11 01:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 978ms)。归档至 done。
