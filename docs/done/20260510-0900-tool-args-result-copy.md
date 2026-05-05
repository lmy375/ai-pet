# PanelDebug 工具调用历史 args/result 复制按钮（Iter R128）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 工具调用历史 args/result 复制按钮：单条展开后 args / result 块各加小复制图标，方便贴 LLM 调试上下文 / issue（与 R90 决策日志批量复制 / R98 / R106 / R124 各类导出模式一致）。

## 目标

PanelDebug 工具调用历史 section 单条点开 details 后显示 args / result
两段 `<pre>`。debug 时常想把"具体 args 是啥"或"result 内容"贴到 issue /
对话给 LLM，但只能手动选中文字 + ⌘C。

加按钮：每段 pre 上方各一个"📋 复制"图标，点击复制对应字符串到剪贴板。

## 非目标

- 不做组合"args + result 一起复制"按钮 —— 单段是更常见的 debug 场景；想
  一起的用户可以连点两次（剪贴板覆盖前可手动 paste 第一段）
- 不做 JSON pretty-print —— 复制原 excerpt 内容（与展示一致）；用户贴到
  目的地后自己 prettify
- 不动 R4 后端 ring buffer 数据形状

## 设计

### 共享反馈 state

```ts
const [copiedToolKey, setCopiedToolKey] = useState<string | null>(null);
```

key 用 `${index}-args` / `${index}-result` 唯一标识每个按钮。点击成功 →
setCopiedToolKey(key) → 1.5s setTimeout 复位 null。

### 渲染：包 pre + 上方按钮 row

把现有：
```tsx
<pre style={preStyle}>{c.args_excerpt}</pre>
<pre style={preStyle}>{c.result_excerpt}</pre>
```

改成两块 div，每块顶部 row 显标签 + 复制按钮：

```tsx
<div style={{ marginTop: 4 }}>
  <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
    <span style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>args</span>
    <button
      type="button"
      onClick={() => copyExcerpt(`${i}-args`, c.args_excerpt)}
      style={smallCopyBtnStyle(copiedToolKey === `${i}-args`)}
      title={copiedToolKey === `${i}-args` ? "已复制 args" : "复制 args 全文到剪贴板"}
    >
      {copiedToolKey === `${i}-args` ? "✓" : "📋"}
    </button>
  </div>
  <pre style={preStyle}>{c.args_excerpt}</pre>
</div>
<div style={{ marginTop: 4 }}>
  <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
    <span style={{ fontSize: 10, color: "var(--pet-color-muted)" }}>result</span>
    <button ... key=`${i}-result` ... />
  </div>
  <pre style={preStyle}>{c.result_excerpt}</pre>
</div>
```

`smallCopyBtnStyle` 与 PanelChat / PanelTasks 复制按钮一致（小 padding /
border / muted）：

```ts
const smallCopyBtnStyle = (copied: boolean): React.CSSProperties => ({
  fontSize: 10,
  padding: "1px 6px",
  border: "1px solid var(--pet-color-border)",
  borderRadius: 4,
  background: "var(--pet-color-card)",
  color: copied ? "#16a34a" : "var(--pet-color-muted)",
  cursor: "pointer",
});
```

### copy handler

```ts
const copyExcerpt = async (key: string, text: string) => {
  try {
    await navigator.clipboard.writeText(text);
    setCopiedToolKey(key);
    window.setTimeout(() => setCopiedToolKey(null), 1500);
  } catch (e) {
    console.error("clipboard write failed:", e);
  }
};
```

### 测试

无单测；手测：
- 展开任一工具调用 details → args / result 上方各显小 📋 按钮
- 点 args → 按钮变 ✓ 1.5s → 粘贴出 args 全文
- 点 result → 同上，按钮独立
- 同时多条工具调用展开 → 每条各自的复制按钮独立工作（key 含 index）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + handler + smallCopyBtnStyle |
| **M2** | render 改 details 内部布局加按钮 row |
| **M3** | tsc + build |

## 复用清单

- 既有 PanelChat / PanelTasks 复制按钮模式
- 既有 c.args_excerpt / c.result_excerpt 数据
- 既有 `<details>` 折叠

## 进度日志

- 2026-05-10 09:00 — 创建本文档；准备 M1。
- 2026-05-10 09:08 — M1 完成。`copiedToolKey: string | null` state；`copyExcerpt(key, text)` handler 写剪贴板 + 1.5s 自清空；smallCopyBtnStyle 闭包内定义。
- 2026-05-10 09:14 — M2 完成。details 内部用 IIFE 包裹改为 args label + 复制按钮 + pre + result label + 复制按钮 + pre 双块结构；按钮 key = `${i}-args` / `${i}-result` 唯一；点击切 ✓ 已复制 1.5s 反馈，跨多条工具调用时 key 含 index 互不冲突。
- 2026-05-10 09:18 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 995ms)。归档至 done。
