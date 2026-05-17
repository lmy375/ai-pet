# detail.md 编辑器底部「¶ 段数 / 🔗 link 数」status bar chip（iter #420）

## Background

detail.md 编辑器底部既有 status bar 已含 N 字 / 〜M 词 / 📐 目标 /
行 N/M / dirty ● 等 chip，但**段数 / link 数**没显 — owner 长文
写作时想看「我已经写了多少段 / 有几条引用」需肉眼数。

本 iter 补两条 chip 完成 TODO 「💡 字数 / 段数 / link 数 status
bar」：字数已存在，段数 + link 数补齐。

## Changes

### `src/components/panel/PanelTasks.tsx`（紧贴既有字数 chip 之后）

#### 1. ¶ 段数 chip

```tsx
{editingDetailTitle === t.title && (() => {
  const content = editingDetailContent.trim();
  if (content.length === 0) return null;
  const paraCount = content
    .split(/\n\s*\n+/)
    .filter((s) => s.trim().length > 0).length;
  return (
    <span style={mutedChipStyle}
      title={`${paraCount} 段（按 markdown 空行分隔...）`}>
      ¶ {paraCount} 段
    </span>
  );
})()}
```

split regex `/\n\s*\n+/` — 连续多空行视作一个分隔，与 markdown
视觉段语义对齐。`.filter(s => s.trim().length > 0)` 过滤掉
boundary 空段（首末多余空行不算）。

#### 2. 🔗 link 数 chip

```tsx
{editingDetailTitle === t.title && (() => {
  const mdLinks = (content.match(/\[[^\]]+\]\([^)]+\)/g) ?? []).length;
  // 裸 URL：前置非 `(` 防双计 markdown link 内的 URL
  const bareUrls = (content.match(/(^|[^(])https?:\/\/[^\s)]+/g) ?? []).length;
  const total = mdLinks + bareUrls;
  if (total === 0) return null;
  return (
    <span style={mutedChipStyle}
      title={`含 ${mdLinks} 条 markdown link + ${bareUrls} 条裸 URL`}>
      🔗 {total} 链
    </span>
  );
})()}
```

两类 link 识别覆盖既有 parseUrls 两类渲染：markdown `[text](url)`
+ 裸 `https?://...` URL。bareUrls regex 前置非 `(` 防把 markdown
link 内的 URL 双计。

设计要点：
- **两 chip 都 muted 配色**：与既有字数 chip 三档配色（默认 / longish
  yellow / danger red）区分 — 段数 / link 数是中性 metric 不该
  抢字数告警视觉
- **0 时不渲**：跟既有 chip 模式一致 — 空内容 / 0 link 显 dead
  chip 是噪音
- **gate by editingDetailTitle === t.title**：与既有字数 chip 同
  gate，确保 chip 只在该 task 处于编辑态时浮起（每个 row 自带 status
  bar 实现）
- **heuristic 精度足够**：段数 split 不 parse 完整 markdown AST；
  link count regex 不严格验证 URL 合法性 — 偏快粗略估计，与字数
  / 词数 heuristic 同精度档

## Key design decisions

- **不为段数引「段长平均」等衍生指标**：chip 应单一信号；衍生指
  标走 future 「📊 文档统计」popover 之类
- **不显「列出所有链接」popover**：超本 iter 范围（status bar 是
  glanceable not interactive）；future 可扩 🔗 chip click 弹列表
- **不修既有字数 chip**：字数已是「completed」TODO 文案一部分，不
  动它，仅追加 ¶ / 🔗 兩 chip
- **不为单 chip 引 unit test**：纯 string regex + setState；build
  pass + 手测足够（写多段文本看 ¶ 计数；插 `[a](b)` 看 🔗 +1；
  插裸 URL 看 +1；纯 markdown link 不双计）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.46s)
- 后端无改动
