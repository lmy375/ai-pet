# parseMarkdown 块级语法扩展

## 需求

`parseMarkdown` 既有支持：空行 / 无序列表 / 普通行 + inline markdown
（bold / italic / inline code / URL）。LLM 输出常含表格、有序列表、fence
code block —— mini chat / panel chat 历史里这些都退化成原始 ` ``` ` 字面
量 + 一坨纯文本，不好读。扩展 parseMarkdown 支持块级语法。

## 实现

`src/utils/inlineMarkdown.tsx` 重写 `parseMarkdown` 为多行状态机：

1. **Fence code block** ` ``` lang ... ``` `
   - 扫开闭合 ` ``` `，中间 lines.join("\n") 作为内容
   - 渲染 `<pre><code>` + 右上角 lang badge（仅 fence 第一行带 lang 时显）
   - 暖琥珀色与 inline code 同色系但块状，overflowX:auto 防长行撑爆
   - EOF 时缺闭合 fence 容错：剩余视为代码块，不报错
2. **表格** `| col | col |` + 下一行 `|---|---|` separator
   - 检测：当前行 `|...|` 且下一行 trim 后匹配 `^\|[\s:|-]+\|$` 含 `-`
   - 解析：剥首尾 `|` 后按 `|` split，trim each cell
   - 渲染 `<table>` + 紧凑 `<th>` `<td>`；border-collapse；th 浅 bg
   - body 行连续读取直到非 `|...|` 行
3. **有序列表** `1. ` `2. ` ...
   - regex `/^(\s*)(\d+)\.\s+(.*)$/`
   - 渲染为 flex div，前缀显数字 `N.`（muted 色）替代 `•`
4. **标题** `# ` / `## ` / `### `（cap 3 级避免气泡里超大字号）
   - 加粗 + 字号 1.25 / 1.1 / 1.0em + 上下 margin
5. 其余沿用：空行 gap / 无序 `- ` `* ` / 普通段落

inline 部分仍调 `parseInlineMarkdown` —— bold / italic / inline code / URL
在所有块级元素内继续工作。

## 验证

- `npx tsc --noEmit` clean
- 行为（在 ChatMini / PanelChat history 都生效，因为它们都走 parseMarkdown）：
  - ` ``` rust\nfn main() {}\n``` ` → 黄底 monospace pre 块，右上角 "rust"
    badge
  - ` ``` \n... \n``` ` 无 lang → 仍渲染，无 badge
  - `| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |` → 渲染 2x2 表格
  - `1. first\n2. second` → 有序列表，前缀 `1.` `2.` muted
  - `# 标题`/`## 子标题` → 加粗 + 大字号
  - 缺闭合 fence 的 ``` 输入 → 把剩余作 code 块（容错）
  - 表格里 `**bold**` 仍生效（td 走 parseInlineMarkdown）

## 不在本轮范围

- 没做完整 markdown 解析（GFM checklist / 嵌套列表 / 引用 / image / link）：
  桌面气泡 / mini chat 空间限制；引入 commonmark 库 ~50KB bundle 增量
- 没做按 lang 的真实 syntax highlight：highlight.js / prismjs 都太重；只
  上 lang badge + 整块暖色 tint 已能区分代码
- 没做表格列对齐（`:---:` / `---:`）：mini 空间小，左对齐已够；后续可扩

## TODO 池

清空后按规则 #1 自主提出 5 条新需求（写入 TODO.md）。

## TODO 池新提案

1. ChatMini fence code block 一键复制
2. PanelChat 待发 attachment 区合并显示（图片 + 文本文件）
3. PanelTasks 任务卡 hover detail.md preview tooltip
4. PanelMemory category 顺序自定义
5. PanelDebug 今日开口小时分布 mini bar
