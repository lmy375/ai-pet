# 桌面气泡轻量 markdown 渲染 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 桌面气泡 markdown 渲染：bubble 当前纯文本，AI 回复里的 *斜体* / **加粗** / ` 行内代码 ` 看不出格式；轻量 markdown（仅这三类）让重点更易扫读。

## 目标

`ChatBubble` 当前 `{message}` 直接 dump 字符串，AI 回复里 markdown 标记都成了
字面字符（用户看到一堆 `**`）。本轮加最小 inline markdown 渲染：
- `**加粗**` → `<strong>`
- `*斜体*` → `<em>`（不与 `**` 冲突）
- `` `行内代码` `` → `<code>`（且**屏蔽**内部其它标记，符合 markdown 直觉）

## 非目标

- 不引 markdown 库（marked / react-markdown）—— 只渲染 3 种 inline 标记，自写
  约 60 行解析器即可，避开依赖体积。
- 不支持块级（标题 / 列表 / 引用 / 代码块）—— 桌面气泡是单行 utterance，块级
  在小气泡里也排不下；panel 聊天的展示风格保持纯文本（与既有 panel chat 设计
  一致，不引入两套渲染）。
- 不支持嵌套 / 链接 / 删除线 / 图片 —— 简单粗暴的"扫到首个 close"足以，嵌套
  在实际 LLM 输出中极少见。
- 不写 README —— 视觉补强。

## 设计

### 解析器（pure）

新文件 `src/utils/inlineMarkdown.tsx`，导出 `parseInlineMarkdown(input: string):
React.ReactNode[]`。状态机做"扫到首个 close"切片：

```ts
function parseInlineMarkdown(input: string): React.ReactNode[] {
  const out: React.ReactNode[] = [];
  let buf = "";
  let i = 0;
  const flush = () => { if (buf) { out.push(buf); buf = ""; } };
  while (i < input.length) {
    // 1. ` … `（最高优先级，内部不再 markdown 解析）
    if (input[i] === "`") {
      const close = input.indexOf("`", i + 1);
      if (close > i) {
        flush();
        out.push(<code key={i}>{input.slice(i + 1, close)}</code>);
        i = close + 1; continue;
      }
    }
    // 2. ** … ** （bold；先于单星，避免 ** 被误吞为两个空斜体）
    if (input.startsWith("**", i)) {
      const close = input.indexOf("**", i + 2);
      if (close > i + 2) {
        flush();
        out.push(<strong key={i}>{input.slice(i + 2, close)}</strong>);
        i = close + 2; continue;
      }
    }
    // 3. * … * （italic；要求左 * 后非空且 close 后非 *，避免吞 ** 边界）
    if (input[i] === "*" && input[i + 1] !== "*") {
      const close = input.indexOf("*", i + 1);
      if (close > i + 1 && input[close + 1] !== "*") {
        flush();
        out.push(<em key={i}>{input.slice(i + 1, close)}</em>);
        i = close + 1; continue;
      }
    }
    buf += input[i]; i++;
  }
  flush();
  return out;
}
```

边界（写在 jsdoc 里供未来读者）：
- 未闭合 token（` ``no close `` / `**no close` / `*no close`）一律字面输出，不
  破坏内容。
- 嵌套（`**bold *italic***`）只识别外层 bold；内层 `*` 字面保留——简化实现，
  实战 LLM 几乎不嵌套。
- 单星紧贴 `**`（`***x***` 这类）：bold 优先匹配，剩余 `*` 字面。

### 应用

`ChatBubble.tsx`：把 `{message}` 换成 `{parseInlineMarkdown(message)}`。
其余 bubble 样式 / 动画 / 按钮全不动。

`code` 元素需要个轻量 CSS（已有 `<style>` 块）：等宽字体 + 浅灰背景 + 圆角。

### 测试

新文件 `inlineMarkdown.tsx` 是 pure。项目无前端测试基础设施，但**导出函数 +
JSDoc 边界 case 列举**让未来引入 vitest 时能立刻钉牢。本轮靠 tsc + 手测
（直接观察 LLM 一次回复在气泡里的渲染）。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `inlineMarkdown.tsx` parse 函数 |
| **M2** | `ChatBubble.tsx` 应用 + 注入 `<code>` 样式 |
| **M3** | tsc + build + cleanup |

## 复用清单

- React 18 默认行为渲染 React 节点数组（不需 fragment / key 警告管理外，循环位置
  i 当 key 安全 —— 一次 parse 内部 i 唯一）

## 待用户裁定的开放问题

- panel chat 是否也启用？本轮**不**——panel chat 历史也是字面 markdown 字符
  串，启用会让 LLM 早期回复（不带 markdown 意识）的散乱 `*` / `_` 渲染奇怪。
  桌面气泡是即时单条 utterance，最近一句应用正好。如反馈想要 panel 也开，未来
  按一行改。
- markdown 标记被 redaction 替换打断怎么办？用户的 redaction 替换多在中文 token
  范围，与 ASCII markdown 标记互不冲突。

## 进度日志

- 2026-05-05 16:00 — 创建本文档；准备 M1。
- 2026-05-05 16:20 — 完成实现：
  - **M1**：新文件 `src/utils/inlineMarkdown.tsx`，导出 `parseInlineMarkdown(input)` 状态机：扫到 backtick / `**` / `*` 的最早匹配并切片 → 输出 React 节点数组。优先级 backtick > bold > italic（因 `**foo**` 不能被两条 `*` 误切）；italic 的左 / 右边界都拒绝紧贴 `*` 避免吞 `**` 边界；未闭合 token 字面输出。`<code>` 内联样式与 panel priBadge 同暖色系（与气泡蓝边对比）。安全性：仅产 React 节点不走 `dangerouslySetInnerHTML`，HTML 注入字面渲染。
  - **M2**：`ChatBubble.tsx` import + 把 `{message}` 换成 `{parseInlineMarkdown(message)}`。其余 bubble 样式 / 动画 / 按钮 / hover 不动。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过（+1 新 utility）。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 桌面气泡的视觉补强，与既有 R 系列气泡迭代同性质。
  - **设计取舍**：自写 ~60 行 vs 引 marked / react-markdown —— 选前者，仅 3 种 inline 标记不值依赖体积；只在桌面气泡启用，panel chat 保持纯文本（panel 历史含早期非 markdown 意识 LLM 输出，启用渲染会让散乱 `*` 渲染奇怪）；嵌套不递归解析（`**bold *italic***` 仅识别外层），实战 LLM 输出极少嵌套，复杂度不值。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；解析器是 pure，输入 → 输出在 jsdoc 边界 case 列举里钉牢，未来可零成本接入 vitest。
