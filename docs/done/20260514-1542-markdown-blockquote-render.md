# Markdown `>` 引用块视觉渲染

## 背景

TODO（上一轮 auto-proposed）：

> 任务详情 markdown preview 渲染 `> ...` 引用块为蓝边竖条 + 缩进底色：当前 fallback 走普通段落渲，丢了引用感。

20260514-1453 给 markdown 工具栏加了 ❝ 引用块按钮，作者侧便利完成；但渲染侧仍把 `> ...` 行当普通段落渲（首字 `>` 字面量裸露）。GitHub / Obsidian / Notion 都把 blockquote 渲成"左侧竖条 + 缩进 + 底色"的视觉块，是这种语法的标准呈现。补这一刀。

## 改动（frontend only）

### `src/utils/inlineMarkdown.tsx`

`parseMarkdown` 在 heading 匹配之前加 blockquote 段：

```ts
if (line.match(/^>(\s|$)/) || line.startsWith(">")) {
  const isQuote =
    /^>(\s|$)/.test(line) ||
    (line.startsWith(">") && line.length > 1 && line[1] !== ">");
  if (isQuote) {
    // consume 连续 `>` 起首行
    const quoteLines: string[] = [];
    let j = i;
    while (j < lines.length) {
      const l = lines[j];
      if (
        /^>(\s|$)/.test(l) ||
        (l.startsWith(">") && l.length > 1 && l[1] !== ">")
      ) {
        quoteLines.push(l.replace(/^>\s?/, ""));
        j++;
      } else {
        break;
      }
    }
    out.push(
      <div key={`md-blk-${i}`} style={{
        borderLeft: "3px solid color-mix(in srgb, var(--pet-color-accent) 50%, var(--pet-color-border))",
        padding: "4px 10px",
        margin: "4px 0",
        color: "var(--pet-color-muted)",
        background: "color-mix(in srgb, var(--pet-color-accent) 4%, transparent)",
        borderRadius: "0 4px 4px 0",
      }}>
        {quoteLines.map((ql, k) => (
          <div key={k} style={{ lineHeight: 1.55 }}>
            {ql.length === 0 ? " " : parseInlineMarkdown(ql)}
          </div>
        ))}
      </div>,
    );
    i = j - 1; // for-loop ++ 跳到 j
    continue;
  }
}
```

**关键设计**：

- **合并连续 `>` 行成单 div**：与 fence code block 同 consume 模式，让多行引用渲成"一整段"而非每行独立 border（视觉一致性）。
- **首字 `>` 后空格灵活**：`> text` / `>text` / 单独 `>`（空引用行）都接受。`>` 后必须不接第二个 `>` —— 避免误伤 `>>=` 等 C bit-shift 操作符 / `>>` 嵌套引用记号。本实现暂不支持 `>>` 嵌套，按 1 级渲染。
- **`color-mix` 半透明 accent 边**：与 `.pet-card-elev` / .pet-divider 等既有 utility 同视觉语言。`borderRadius: "0 4px 4px 0"` 让右下圆角呼应"内容有界 + 左竖条无界"。
- **`color: muted` 文字色**：引用相对正文要"次一档"才像引用，与 GitHub / Obsidian 渲染观感一致。
- **inline markdown 仍 parseInlineMarkdown**：保留链接 / 粗体 / 行内代码在引用内仍可识别。
- **空引用行 ` ` (单空格)**：让"`>\n> foo\n>\n> bar`" 这种多段引用的中间空行有视觉间隔。

## 不做

- **不支持 `>>` 嵌套引用**：parseMarkdown 整体不支持嵌套（list / quote / heading 都平面）。要加得引入 nesting depth 机制，超本特性范围。
- **不动作者侧（工具栏按钮）**：已 ship。
- **不区分 GFM Alerts (`> [!NOTE]` 等)**：早决策（20260514-1453 done doc 已列）跳过 GFM 扩展，本视觉对所有 blockquote 一视同仁。
- **不写测试**：前端无 vitest；逻辑是 regex 命中 + DOM render 同 fence-code-block 模式（既有路径已测）。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.22s
- 改动 ~50 行（parseMarkdown 内单段 if-block）；既有 fence / heading / list / inline / 表格 / 任务列表 / checkbox 路径全部不动。

## TODO 状态

- 本轮实现 1 条。
- TODO 剩 3 条：会话标题 LLM 自动重写按钮 / PanelMemory inline edit description / 桌面顶栏陪伴 chip。
