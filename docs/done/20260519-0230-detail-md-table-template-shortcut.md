# detail.md 编辑器加「⌘⇧M 插 markdown table 模板」快捷键（iter #524）

## Background

detail.md 编辑器 owner 写笔记常需要 markdown table — 列对比 / 决策清单
/ 进度记录格式都用。但手敲 `| col | col | col |` + 分隔行 + 数据行的
管道符格式 friction：
1. 容易漏写分隔行（markdown parse 失败）
2. 列数不一致 visual 不对齐
3. 多 row 时机械重复

VS Code / Typora 等编辑器有「Insert Table」menu。本 iter 加 detail.md
键盘快捷 ⌘⇧M — 即时插 3x3 模板（header + 分隔 + 2 空数据行）。

## Changes

### `src/components/panel/PanelTasks.tsx`

新 `handleDetailTableTemplate` callback（紧贴 `handleDetailIsoTimestamp`
之后）：

```tsx
const handleDetailTableTemplate = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "m") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    e.preventDefault();
    const template =
      "\n| 列1 | 列2 | 列3 |\n| --- | --- | --- |\n|     |     |     |\n|     |     |     |\n";
    insertMarkdownAtCursor("wrap", template, "");
    return true;
  },
  [insertMarkdownAtCursor],
);
```

模板：

```
| 列1 | 列2 | 列3 |
| --- | --- | --- |
|     |     |     |
|     |     |     |
```

- 3 列 × 4 行（header + 分隔 + 2 空数据）— 实用最小尺寸
- 前后各加 `\n` 保表格独立成块（markdown spec：table 前后必须空行才能
  被识别）
- 空数据格 `|     |` 给 owner 立即可填，加宽白空间让对齐易看

#### 接入 onKeyDown 链

两个 textarea（split / edit-only）都接入：

```tsx
if (handleDetailIsoTimestamp(e)) return;
// ⌘⇧M 插 markdown table 3x3 模板 — 快速搭表格架构免手敲管道符。
if (handleDetailTableTemplate(e)) return;
```

#### Keyboard help modal 新一行

```tsx
["⌘⇧M", "插 markdown table 3x3 模板（header + 分隔 + 2 空行）— 快速搭表"],
```

## Key design decisions

- **3x3 最小实用尺寸**：3 列覆盖大部分对比 / 决策表场景（"项目 / 状态
  / 备注" 三元组最常见）；2 空数据行让 owner 即时填两条 sample +
  按 Enter 加更多。1 列表用 `-` bullet 更顺手，不需要本入口
- **`| 列1 | 列2 | 列3 |`** 中文 header：与 detail.md 中文写作主场景一
  致；owner 即时改为业务列名
- **前后 `\n` 包裹**：markdown table 必须前后空行（CommonMark spec），
  避免插中段时与既有 paragraph 粘连导致 parse 失败
- **空数据格宽白 `|     |`**：visual 对齐 — owner 看模板时即知有 3 列
  + 各列等宽
- **modifier ⌘⇧M**：⌘M 是 OS minimize；shift 修饰避开。⌘⇧M 行业 IDE
  ("Markdown" / "Matrix") 心智匹配；webview 内此键空
- **不写 unit test**：纯字符串模板 + 既有 `insertMarkdownAtCursor`
  helper（production 验证）。逻辑 trivial — GOAL.md "meaningful tests
  only" 规则下不引装饰性测试
- **不引「列数选项」popover**：模板派 keyboard shortcut 设计为「一键
  得到合理 default」；owner 想要 4 / 5 列时直接编辑模板 paste-extend
  即可

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.33s)
- 后端无改动 — 纯前端 keyboard shortcut
- 手测：
  - detail.md 编辑器内 ⌘⇧M → 光标位置插 4 行 markdown table 模板
  - 模板渲染后 markdown preview 正确显 table
  - 跨 split / edit-only 模式都触发
  - ⌘/ 帮助 modal 看到新「⌘⇧M」行

## Future iters (out of scope)

- 「⌘⇧⌥M 自定义列数 prompt」— popover 让 owner 指定 N 列 × M 行；当
  前 3x3 default 已是 80% 场景
- 「自动对齐管道符」— owner 填内容后管道符可能 misaligned；后续 iter
  可加 table format helper
