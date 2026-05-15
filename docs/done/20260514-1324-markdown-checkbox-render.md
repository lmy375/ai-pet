# 任务详情 markdown 可勾选 todo checkbox

## 背景

TODO（auto-proposed 上一轮）：

> 任务详情 markdown preview 渲染 `- [x]`/`- [ ]` 为可勾选 checkbox：点击切换 `[ ]↔[x]` 写回 detail.md。

20260514-1312 上一轮的工具栏给 detail.md 加了「☐ 待办」按钮，方便创建 `- [ ]` 行。但渲染层把这些行还当普通无序列表 bullet 渲，文字 `[ ]` 字面量直白外露不像 todo。把渲染升级到 native `<input type="checkbox">` 让 detail.md 真的变成 GitHub / Obsidian / Notion 一样的可勾选清单。

## 改动

### `src/utils/inlineMarkdown.tsx`

**1. 新可选 opts 参数**

```ts
export interface ParseMarkdownOpts {
  checkboxToggle?: {
    lineOffset: number;
    onToggle: (globalLineIdx: number, checked: boolean) => void;
  };
}
export function parseMarkdown(input: string, opts?: ParseMarkdownOpts): ReactNode[]
```

`lineOffset` 让调用方把"slice 内 line idx"加上全局偏移再回传 —— 对 `parseDetailMdWithImages` 这种把 md 按 image 切片再各自调 parseMarkdown 的场景预留。本次 PanelTasks 只用单段 parseMarkdown，lineOffset 全 0。

**2. 行级匹配：在普通 `- ` 列表之前**

```ts
const taskMatch = line.match(/^(\s*)- \[([ xX])\]\s+(.*)$/);
if (taskMatch) {
  const checked = taskMatch[2].toLowerCase() === "x";
  const body = taskMatch[3];
  const toggle = opts?.checkboxToggle;
  const globalIdx = (toggle?.lineOffset ?? 0) + i;
  out.push(
    <div style={LIST_ITEM_STYLE}>
      <input type="checkbox" checked={checked}
             disabled={!toggle}
             onChange={toggle ? (e) => toggle.onToggle(globalIdx, e.currentTarget.checked) : undefined}
             style={{ marginRight: 6, flexShrink: 0,
                      accentColor: "var(--pet-color-accent)",
                      cursor: toggle ? "pointer" : "default" }}
             aria-label={...} />
      <span style={checked ? { textDecoration: "line-through", opacity: 0.6 } : undefined}>
        {parseInlineMarkdown(body)}
      </span>
    </div>
  );
  continue;
}
```

匹配大小写 `[ ]` / `[x]` / `[X]` 三种（GitHub flavor 接受所有）。body 走 `parseInlineMarkdown` 保留链接 / 粗体 / inline code。

**无 callback 时仍渲 checkbox（disabled）**：让读 / 写视图视觉一致 —— 区别只是能否点。如果 callback 仅 toggleable 渲、disabled 全跳过，read-only 视图会回到 `- [ ]` 字面量，体感倒退。

### `src/components/panel/PanelTasks.tsx`

**1. `toggleEditChecklistLine` callback**

```ts
const toggleEditChecklistLine = useCallback((lineIdx: number, checked: boolean) => {
  setEditingDetailContent((cur) => {
    const lines = cur.split("\n");
    if (lineIdx < 0 || lineIdx >= lines.length) return cur;
    const replaced = lines[lineIdx].replace(/- \[[ xX]\]/, checked ? "- [x]" : "- [ ]");
    if (replaced === lines[lineIdx]) return cur;
    lines[lineIdx] = replaced;
    return lines.join("\n");
  });
}, []);
```

Functional setState 让多次连点不同行不会被闭包旧值覆盖。只 toggle marker（不动 body / 前导空白）。空操作（行不存在 / 不含 marker）直接返原值，防止误生成虚假 undo 历史。

**2. edit-mode 预览的两处 parseMarkdown 调用接入 callback**

```tsx
parseMarkdown(editingDetailContent, {
  checkboxToggle: { lineOffset: 0, onToggle: toggleEditChecklistLine },
})
```

`detailViewMode === "split"` 和 `detailViewMode === "preview"` 两个分支同样传 opts。read-only `parseDetailMdWithImages` 路径不传 → checkbox 渲为 disabled。

**3. 已勾选 → 自动 save？不做。**

勾选后只 mutate `editingDetailContent`，「未保存」chip 自然显出来；用户按"保存"按钮才写盘。让 batch 勾选不重复触发磁盘写、也让用户能"勾错了，撤回"（textarea 撤销栈仍可用，因为 setState 不影响浏览器 undo —— 但 controlled input 不进 undo stack，所以撤销只能靠 Esc + 重新进编辑；这是 controlled textarea 的通病，超出本次范围）。

## 不做

- **read-only `parseDetailMdWithImages` 不开 toggle**：要 toggleable 需要把"未编辑模式直接 mutate detail.md 写盘"接通，IO 路径不同、风险高，UX 也不一致（用户在"看"时不期望误触一下就改原始 detail）。当前 disabled checkbox 至少视觉精确反映状态，足够 90% 用例。
- **不做 indeterminate / nested checkbox**。GitHub-flavored task list 是平面 list；嵌套需要 2-space 缩进 + 多级 li，与现有的简易 parseMarkdown 一直没支持的"嵌套列表"是同一档复杂度。当下用 markdown heading 分组替代足够。
- **不写测试**。前端无 vitest；taskMatch 是新加的纯正则 + JSX，逻辑明显；callback 也是 12 行 functional setState。
- **不动 ChatMini / PanelChat 等其它 parseMarkdown 调用**。它们不传 opts → checkbox 渲 disabled，无副作用；要 toggleable 是 case-by-case 决策（聊天消息里的 todo 是宠物给的"建议" 还是用户的"自留地"？语义不清，先不动）。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.10s
- 改动 ~50 行（parser 35 + callback 12 + 两处传 opts ~3 行）；既有 fence code / 表格 / heading / list / inline 路径全部不动。

## 后续

- 把 ChatMini / PanelChat 渲染层也接 toggle（决定"chat 里的 todo 是不是可勾"语义后）。
- read-only detail 视图开 toggle —— 需要直接 invoke `task_save_detail` 写盘 + 错误反馈路径。
- `- [ ]` 行 hover 显示"添加 due:..." 子菜单，让 todo 行也能挂时刻。
