# PanelTasks detail.md 预览模式（Iter R117）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks detail.md 预览模式：detail 编辑现是纯 textarea，写 markdown 看不到 render；加 "✏️ 编辑 / 👁 预览" toggle，预览模式用既有 parseInlineMarkdown 渲染，与 GitHub issue 编辑同模式。

## 目标

PanelTasks 任务详情面板的 detail.md 编辑当前是纯 textarea —— 写
`- 子任务` / `**重点**` / `` `code` `` 时看不到渲染效果。GitHub issue /
GitLab MR 等编辑界面都用 "Edit / Preview" tab 切换；同款模式让用户保存
前确认 markdown 是否符合预期。

加 `detailPreviewMode: boolean` 状态 + 切换按钮：
- 默认 false（edit）—— 与现有行为一致
- true（preview）—— textarea 替换为 read-only `parseMarkdown(content)` 渲染
- 切换不丢未保存内容（state 共享 editingDetailContent）

## 非目标

- 不引入第三方 markdown 库（remark / marked 等） —— `parseMarkdown` 已在
  `src/utils/inlineMarkdown.tsx` 实现 inline + block 子集（- / * 列表、
  空行段落、bold / code / URL）足够 detail.md 笔记类内容
- 不做 split-view（左 edit / 右 preview）—— 占视觉空间，单 toggle 切换
  够用
- 不持久化 preview / edit 选择 —— session 内有效；下次打开默认 edit

## 设计

### state

```ts
const [detailPreviewMode, setDetailPreviewMode] = useState(false);
```

只一个 boolean —— 因为 `editingDetailTitle` 已保证同时只一个 task 处于
edit；不需要 per-task。退出编辑（save / cancel）时不强制 reset
（下次进入时按 default false 渲染就行）。

### import

```diff
+import { parseMarkdown } from "../../utils/inlineMarkdown";
```

### 渲染改造

现有结构：
```tsx
{editingDetailTitle === t.title ? (
  <div ...>
    <textarea ... />
    <div>...保存按钮 / 取消按钮 / cancelArmed...</div>
  </div>
) : (
  /* 不在 edit 模式 - 显原 detail_md */
)}
```

加 toggle 在 textarea 之上、buttons row 之前：

```tsx
<div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
  <div style={{ display: "flex", gap: 4 }}>
    {(["edit", "preview"] as const).map((mode) => {
      const active =
        mode === "edit" ? !detailPreviewMode : detailPreviewMode;
      return (
        <button
          key={mode}
          type="button"
          onClick={() => setDetailPreviewMode(mode === "preview")}
          style={{
            fontSize: 11,
            padding: "2px 8px",
            border: "1px solid",
            borderColor: active ? "#0ea5e9" : "#e2e8f0",
            borderRadius: 4,
            background: active ? "#0ea5e9" : "#fff",
            color: active ? "#fff" : "#475569",
            cursor: active ? "default" : "pointer",
            fontWeight: active ? 600 : 400,
          }}
        >
          {mode === "edit" ? "✏️ 编辑" : "👁 预览"}
        </button>
      );
    })}
  </div>
  {detailPreviewMode ? (
    <div
      style={{
        minHeight: 100,
        padding: "8px 10px",
        fontSize: 12,
        lineHeight: 1.55,
        border: "1px dashed #cbd5e1",
        borderRadius: 4,
        boxSizing: "border-box",
        color: "#1e293b",
        whiteSpace: "pre-wrap",
        background: "var(--pet-color-bg)",
      }}
    >
      {editingDetailContent.trim() === "" ? (
        <span style={{ color: "var(--pet-color-muted)", fontStyle: "italic" }}>
          （空 — 切回 ✏️ 编辑写笔记）
        </span>
      ) : (
        parseMarkdown(editingDetailContent)
      )}
    </div>
  ) : (
    <textarea ... existing ... />
  )}
  {/* 保存按钮 / 取消按钮 row 不动 */}
</div>
```

### `parseMarkdown` 注意

它返 `ReactNode[]`；render 直接展开。处理 `- ` / `* ` 列表 + bold / code
/ URL 已足够 detail.md 场景。不识别标题 / 表格 / 代码块 fence —— 用户写
长文档少见，写 issue 风格"列表 + 强调"已覆盖。

### 测试

无单测；手测：
- 进入 edit 模式：默认 ✏️ 编辑高亮、textarea 显示
- 点 👁 预览：高亮切，div 显 parseMarkdown render（含列表 / bold）
- 切回 ✏️ 编辑：textarea 文本不丢
- 内容空时预览显占位文案
- 保存 / 取消：state 不影响 detail_md 落库逻辑（textarea-bound editingDetailContent 是真源）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + import + render 改造 |
| **M2** | tsc + build |

## 复用清单

- `src/utils/inlineMarkdown.tsx` 的 `parseMarkdown`
- 既有 sortMode toggle 同款 button 视觉

## 进度日志

- 2026-05-09 22:00 — 创建本文档；准备 M1。
- 2026-05-09 22:08 — M1 完成。`detailPreviewMode: boolean` state（单 boolean，editingDetailTitle 单一互斥保证）；textarea 之前加 toggle 按钮组（"✏️ 编辑" / "👁 预览"），accent borderColor + bg 表 active；preview 时 div 用 `parseMarkdown(editingDetailContent)` render，空内容显占位；textarea 不动，编辑时切换 state，buttons row 不影响保存路径（editingDetailContent 仍是真源）。
- 2026-05-09 22:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
