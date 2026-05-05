# PanelDebug 工具调用历史行 hover bg（Iter R135）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 工具调用历史行 hover bg：继续 R122/R123/R130/R131/R133 同款 hover overlay 模式；tool history 单条 div 加 className + :hover rgba rule，让密集 list 中光标位置可见。

## 目标

PanelDebug 工具调用历史 section 单条 card：黄底 (`#fffbeb`) + 黄边
(`#fde68a`)。hover 时无视觉变化，密集 list 中扫不到光标位置。

加 hover overlay：与 R122/R123/R130/R131/R133 同款 rgba 半透明 bg 叠加，
保留黄底基色但显出 hover 反馈。

## 非目标

- 不动 inline 黄底 / 黄边 —— 这是"工具调用 section 类型"色块（R7 风格）
- 不与 details 折叠面板冲突 —— hover 是行级，details 内部 args / result
  按钮各自有 hover 行为
- 不动 risk_level / review_status badge

## 设计

### CSS rule

既有 `<style>` block (line 1406+) 末追加：

```css
.pet-tool-history-row {
  transition: background-color 0.12s ease;
}
.pet-tool-history-row:hover {
  background: rgba(0, 0, 0, 0.04) !important;
}
```

`!important` 反压 inline `background: "#fffbeb"`。hover 时 bg 由黄变浅灰
（叠 alpha 后 = 黄 + 灰 ≈ 浅 muddy），与黄底"section 标识"轻度冲突 —
但 hover 是瞬态，移开立即恢复，可接受。

跟其它 row hover 风格统一更重要。

### className 加到 row div

```diff
 <div
   key={i}
+  className="pet-tool-history-row"
   style={{
     border: "1px solid #fde68a",
     ...
     background: "#fffbeb",
   }}
 >
```

### 测试

无单测；手测：
- 工具调用历史行 hover → 黄底变浅 muddy，边框不变
- 移开 → 立即恢复黄
- 与 details 内部 args/result 按钮 hover 无冲突
- light / dark 主题切换都生效

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `<style>` rule + className |
| **M2** | tsc + build |

## 复用清单

- 既有 `<style>` block (line 1406)
- R122 / R123 / R130 / R131 / R133 hover 模式

## 进度日志

- 2026-05-10 16:00 — 创建本文档；准备 M1。
- 2026-05-10 16:08 — M1 完成。`<style>` block 末追加 `.pet-tool-history-row` + `:hover` rgba !important rule（反压 inline yellow bg）；tool history filtered.map 的 row div 加 className。
- 2026-05-10 16:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
