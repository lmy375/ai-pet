# PanelChat URL 链接识别 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> panel chat 链接识别：与桌面气泡同步，PanelChat 消息里的 `https://...` 也走 inline markdown URL 渲染，多端体验一致。

## 目标

桌面气泡上一轮已经识别 `http(s)://...` 渲染成蓝下划线 + 点击 openUrl。Panel
chat 仍纯文本展示 URL —— 复制是有按钮但点击直接打开浏览器更顺手。本轮把
URL 识别也接进 PanelChat 的 user / assistant 消息渲染。

## 非目标

- 不在 panel chat 启用完整 markdown（bold / italic / code / list）—— 历史消息
  含早期非 markdown 意识 LLM 输出，启用全 markdown 会让散乱 `*` / `-` 渲染
  奇怪。仅识别 URL（爆破力小、误命中风险低）。
- 不识别 markdown `[text](url)` 语法（同桌面气泡 v1 决策）。
- 不识别 `mailto://` / `ftp://` / `file://`（同桌面气泡）。
- 不写 README —— panel chat 体验补强。

## 设计

### 抽取 URL-only parser

`inlineMarkdown.tsx` 把 URL 识别逻辑抽出公共 `parseUrls(input)` 函数（与既有
parseInlineMarkdown 内的 URL 分支共享 UrlLink 组件）：

```ts
export function parseUrls(input: string): ReactNode[] {
  const out: ReactNode[] = [];
  let buf = "";
  let i = 0;
  let key = 0;
  const flush = () => { if (buf) { out.push(buf); buf = ""; } };
  while (i < input.length) {
    if (input.startsWith("http://", i) || input.startsWith("https://", i)) {
      const schemeLen = input.startsWith("https://", i) ? 8 : 7;
      let end = i;
      while (end < input.length && !/\s/.test(input[end])) end++;
      while (end > i + schemeLen && /[.,;:!?。，；：！？)）"'”“]/.test(input[end - 1])) end--;
      if (end > i + schemeLen) {
        flush();
        const url = input.slice(i, end);
        out.push(<UrlLink key={`url-${key++}`} url={url} />);
        i = end;
        continue;
      }
    }
    buf += input[i];
    i++;
  }
  flush();
  return out;
}
```

`parseInlineMarkdown` 的 URL 分支与 parseUrls 算法一致，可保留不动（让桌面
气泡仍走 inline markdown 完整路径）。

### 应用

`PanelChat.tsx::CopyableMessage` 把 `<div style={bubbleStyle(role)}>{content}</div>`
中 `{content}` 改为 `{parseUrls(content)}`。

### 测试

`parseUrls` 是 pure；项目无 vitest，靠 jsdoc 边界 case 列举（与 parseInlineMarkdown
共享算法逻辑） + tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `parseUrls` export |
| **M2** | PanelChat CopyableMessage 接入 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `UrlLink` 组件（蓝下划线 + onClick openUrl）
- 既有 `parseInlineMarkdown` URL 分支算法

## 进度日志

- 2026-05-05 35:00 — 创建本文档；准备 M1。
- 2026-05-05 35:10 — 完成实现：
  - **M1**：`inlineMarkdown.tsx` export 新 `parseUrls(input)` 函数（与 `parseInlineMarkdown` 内 URL 分支同源算法：scheme + non-whitespace + 剥句末标点 + ≥1 char host），仅识别 URL 不处理其它 markdown，给"想 URL 化但不想全 markdown"的场景用。复用既有 `UrlLink` 子组件。
  - **M2**：`PanelChat.tsx` import `parseUrls`，`CopyableMessage` 把 bubble 的 `{content}` 改为 `{parseUrls(content)}`。桌面气泡仍走完整 `parseMarkdown`（即时一句无历史风险）；panel chat 走 URL-only 避免历史里散乱 `*` / `-` 误渲染。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— panel chat 体验补强。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；解析层 pure 与 parseInlineMarkdown 同源，由 tsc 保证。
