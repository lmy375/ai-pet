# 桌面气泡 URL 链接识别 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 桌面气泡链接识别：当 LLM 回复里出现 `https://...` URL，用蓝色下划线渲染并加 `cursor: pointer`，点击调 `@tauri-apps/plugin-opener` 打开浏览器；纯文本 URL 字面渲染太朴素。

## 目标

桌面气泡现在 markdown 已渲染 `**bold**` / `*italic*` / `` `code` `` / 多行 / list，
但 LLM 回复里的 `https://example.com` 仍是字面纯文本。本轮在 inline 解析里
加 URL token：识别 `http://` / `https://` + non-whitespace 序列，剥掉常见句末
标点，渲染为蓝色下划线 `<a>` + onClick 调 `@tauri-apps/plugin-opener` 的
`openUrl` 打开默认浏览器（不能用 `<a target="_blank">` —— Tauri WebView 不会
处理）。

## 非目标

- 不识别 markdown 链接语法 `[text](url)` —— LLM 输出 plain URL 远多于 markdown
  语法链接；先覆盖主路径。
- 不做 URL 缩短（"example.com" 显示完整 URL）—— 桌面气泡 max-height 80px 内
  长 URL 会自动 break-word，无需缩短。
- 不识别 `www.example.com` 等无 scheme 的 URL —— scheme 是判定锚点，无 scheme
  容易误命中"www."这样的引用文本。
- 不识别 `mailto:` / `ftp:` / `file:` —— 安全语义复杂（file: 尤其危险），本轮
  限定在 http/https。
- 不写 README —— 桌面气泡视觉补强。

## 设计

### 解析层

`inlineMarkdown.tsx::parseInlineMarkdown` 加第 4 类 token，**优先级**安排在
backtick 之后（code 内 URL 字面保留）+ bold/italic 之前（避免 `*https://...*`
被解析成斜体吞掉 URL）—— 实际上放在最前不行，因为 backtick 优先；放在 bold
之前会让 `**https://...**` 中的 `**` 被识别为 bold 包住 URL 字面。看代码顺
序更合适：放在 backtick 之后、bold 之前。

```ts
// after backtick check, before bold check
if (input.startsWith("http://", i) || input.startsWith("https://", i)) {
  let end = i;
  while (end < input.length && !/\s/.test(input[end])) end++;
  // 剥掉常见句末标点 (. , ; : ! ? 中英括号 / 引号)
  while (end > i + 8 && /[.,;:!?。，；：！？)）"]'/.test(input[end - 1])) end--;
  if (end > i + 8) {
    flush();
    const url = input.slice(i, end);
    out.push(<UrlLink key={`md-${key++}`} url={url} />);
    i = end;
    continue;
  }
}
```

### `<UrlLink>` 子组件

```tsx
function UrlLink({ url }: { url: string }) {
  return (
    <a
      href={url}
      onClick={(e) => {
        e.preventDefault();
        e.stopPropagation();  // 避免冒泡到气泡 onClick (dismiss + R1b 反馈)
        openUrl(url).catch((err) => console.error("openUrl failed:", err));
      }}
      style={{
        color: "#0ea5e9",
        textDecoration: "underline",
        cursor: "pointer",
        wordBreak: "break-all",
      }}
    >
      {url}
    </a>
  );
}
```

`openUrl` 来自 `@tauri-apps/plugin-opener`（已在 package.json 里）。

### 测试

`parseInlineMarkdown` 已存于 `src/utils/inlineMarkdown.tsx`，是 pure。无 vitest，
靠 jsdoc 边界 case 列举 + tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | URL 识别 + `<UrlLink>` 渲染 |
| **M2** | 边界 case（句末标点 / 中英括号 / 多 URL）已含在 M1 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `parseInlineMarkdown` 状态机（在 backtick 与 bold 之间插入）
- `@tauri-apps/plugin-opener::openUrl` JS API

## 待用户裁定的开放问题

- e.stopPropagation 防止冒泡到气泡 onClick：必须 —— 否则点链接会同时 dismiss
  气泡。
- 如果 URL 中含 ASCII 句号 + 后接空格 → 期望剥掉。已在 trailing-punct strip
  里覆盖。
- URL 含 `**` 之类 markdown 字符（极少见）：本轮 URL 识别先于 bold，保护
  `https://x.com/**foo**` 不被 bold 切（URL 把 `**foo**` 当成 path 字面留下）。

## 进度日志

- 2026-05-05 34:00 — 创建本文档；准备 M1。
- 2026-05-05 34:15 — 完成实现：
  - **M1**：`inlineMarkdown.tsx::parseInlineMarkdown` 在 backtick 之后、bold 之前插入 URL 识别分支：扫到 `http://` / `https://` 起始后向前扩到首个 whitespace 终止；剥常见句末标点 (`. , ; : ! ? 中英括号 / 引号`) —— 让 "Visit https://example.com." 不把句号包进 link；至少 scheme + 1 char host 才视作有效 URL。新增 `UrlLink` 子组件（蓝色下划线 + cursor pointer + wordBreak break-all），onClick `preventDefault + stopPropagation` 防冒泡到气泡 onClick / Tauri WebView 默认导航，调 `@tauri-apps/plugin-opener::openUrl` 打开默认浏览器。
  - **M2**：jsdoc 顶部注释 + 内部分支编号同步（1 backtick / 2 URL / 3 bold / 4 italic），让维护者一眼看出优先级链。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过（`@tauri-apps/plugin-opener` 已在 package.json，无需新增依赖）。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 桌面气泡视觉补强，与既有 inline markdown / 多行 markdown 同性质。
  - **设计取舍**：仅 http/https（不 mailto/ftp/file —— file 安全语义复杂）；不识别 markdown `[text](url)` 语法（LLM 输出 plain URL 远多于 markdown 链接）；不识别无 scheme `www.x.com`（容易误命中文中"www."引用）；URL 优先级先于 bold（让 `**https://x**` 整 URL 识别而非被 bold 拆分）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；解析层是 pure，UrlLink 调 plugin-opener 的 JS API（与 PanelDebug 既有"复制 prompt"等 invoke 模式同源）。
  - **TODO 后续**：列表清空后按规则提 5 条新候选。
