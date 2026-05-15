# 任务详情 markdown 工具栏 +1：`>` 引用块按钮

## 背景

TODO（auto-proposed 之前几轮）：

> 任务详情 markdown 工具栏 +1 `>` 引用块按钮：补齐 GFM 主要语法的快捷入口。

工具栏现有 5 个按钮（**B** 粗体 / **•** 列表 / **🔗** 链接 / **`</>`** 代码块 / **☐** 待办）覆盖了多数 GFM 主体语法，但 `>` 引用块漏了 —— 粘别人的话 / 引用之前结论 / 写"提示框"风的文档常用。补这一颗。

## 改动（frontend only）

### `src/components/panel/PanelTasks.tsx`

在 ☐ 待办按钮之后追加一个按钮，复用既有 `insertMarkdownAtCursor` helper（`line-prefix` mode + prefix `"> "`）：

```tsx
<button
  type="button"
  onClick={() => insertMarkdownAtCursor("line-prefix", "> ", "")}
  title="引用块（> ...）。每选中行的行首加 >；多行连续就是多行引用。粘别人的话 / 引用之前结论 / 提示框都常用。"
  style={mdToolbarBtnStyle}
>
  ❝
</button>
```

glyph `❝`（U+275D HEAVY DOUBLE TURNED COMMA QUOTATION MARK ORNAMENT）—— 单字符即可表达"引用"，避免`>` 字面量在窄按钮里像 chevron。其它按钮（B / • / 🔗 / `</>` / ☐）也走"单字符或紧凑 emoji 代表语义"的视觉规则，本按钮与之一致。

## 不做

- **不接入 Markdown 解析层的渲染优化**。`parseMarkdown` 当前把 `> ...` 行当普通文本渲（无缩进 / 边框样式），但本 button 的本意是**作者侧便利**而非 viewer 增强。viewer 渲染走 markdown standard 是独立改动。
- **不加 [\\!NOTE] / [\\!WARNING] GFM Alerts** 等扩展语法。基础 `> ` 足够；GFM Alerts 是 GitHub-only 扩展，Obsidian / Notion 都不识别，加 button 反而误导。
- **不写测试**。前端无 vitest；按钮 onClick 调既有 `insertMarkdownAtCursor` helper（textarea selection 路径已被其它 5 按钮间接验证）。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.14s
- 改动 ~15 行（1 个新 button）；既有 5 按钮 + insertMarkdownAtCursor / detailEditorRef 路径全部不动。

## TODO 状态

- 本轮清掉 1 条 stale（detail editor 字数 chip — PanelTasks.tsx:6216 早已渲 `{charCount} 字`）。
- 本轮实现 1 条（`>` 引用块按钮）。
- 当前 TODO 剩 3 条：TG /whoami registry / ChatMini 跳到 Panel deeplink / PanelDebug session size 卡片。

## 后续

- 工具栏视觉规模化：6 按钮已开始往横向溢出靠拢；> 8 按钮时考虑折叠或二级行（"格式 / 块 / 其它" 三组）。
