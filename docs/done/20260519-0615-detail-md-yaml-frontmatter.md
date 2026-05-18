# detail.md 编辑器加「⌘⇧Y YAML frontmatter」shortcut（iter #540）

## Background

owner 写长 detail.md（publishable note / blog draft / 决策记录 / wiki
entry）常需要 YAML frontmatter 元数据 — jekyll / hugo / obsidian /
zola 等都支持以下结构识别 publication metadata：

```yaml
---
title: ...
date: 2026-05-19
tags: [...]
---
```

但手敲容易：
1. 漏 4 个 `-`（必须 `---` 单独成行）
2. 忘填 date（每次都得心算今天 ISO）
3. 不知 YAML 数组语法（`tags: []` vs `tags:`）

本 iter 加 ⌘⇧Y — 模板 + 自动填今日日期 + 光标落 title 字段。

## Changes

### `src/components/panel/PanelTasks.tsx`

新 `handleDetailYamlFrontmatter` callback（紧贴 `handleDetailBlockquote`
之前）：

```tsx
const handleDetailYamlFrontmatter = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "y") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    e.preventDefault();
    const ta = e.currentTarget;
    const value = ta.value;
    // 已有 frontmatter → noop（frontmatter spec：doc 第 1 行 `---`）
    if (value.startsWith("---\n")) return true;
    const now = new Date();
    const y = now.getFullYear();
    const mo = String(now.getMonth() + 1).padStart(2, "0");
    const d = String(now.getDate()).padStart(2, "0");
    const dateStr = `${y}-${mo}-${d}`;
    const template = `---\ntitle: \ndate: ${dateStr}\ntags: []\n---\n\n`;
    setEditingDetailContent(template + value);
    // 光标落 title: 后（11 chars: "---\ntitle: "）让 owner 立即可输标题
    const cursorPos = 4 + "title: ".length;
    requestAnimationFrame(() => { ... });
    return true;
  },
  [],
);
```

## Key design decisions

- **必须插 doc 起始 + idempotent guard**：YAML frontmatter spec —
  parser 仅识别 doc 第 1 行起的 `---`；插中段无意义。`startsWith("---\n")`
  short-circuit 已有 frontmatter 时 noop，防重复插
- **3 字段最小集**：title / date / tags — 最常用三件套。其它字段
  （author / categories / draft / layout 等）owner 按 site 框架手添
- **自动填 today date**：用 `getFullYear/Month/Date` 本地时区 YYYY-MM-DD —
  与既有 ⌘⇧D 短日期戳 / created_at 协议一致
- **`tags: []`**：YAML 空数组语法 — 比 `tags:` (隐式 null) 更明确。
  owner 填入时改成 `tags: [foo, bar]` 或多行：
  ```yaml
  tags:
    - foo
    - bar
  ```
- **光标落 `title: ` 后**：插完即可输标题；最常需 owner 填写的字段
- **modifier ⌘⇧Y**：⌘Y 是 redo（macOS Chrome / Edge — Tauri webview
  webview 默认绑）；shift 修饰避开
- **不写 unit test**：纯字符串模板 + 单 startsWith 检查 + state set；
  逻辑 trivial（既有 ⌘⇧M / ⌘⇧A / ⌘⇧I template insert 同 pattern
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰
  性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 后端无改动 — 纯前端 keyboard shortcut
- 手测：
  - 空 doc / 无 frontmatter → ⌘⇧Y → 模板插到顶 + cursor 落 title: 后
  - 已有 frontmatter（`---\n...`）→ ⌘⇧Y → noop（idempotent guard 验）
  - 输 title → 自动 hugo / jekyll / obsidian 渲染识别
  - date 字段填的是手测时的今日 YYYY-MM-DD
  - 跨 split / edit-only 模式都触发
  - ⌘/ 帮助 modal 看到「⌘⇧Y」行

## Future iters (out of scope)

- 「自定义 frontmatter schema」— owner 不同 site 用不同字段（draft /
  layout 等）；当前 3 字段 default 够 80% 场景
- 「插 + 解析既有 frontmatter 改单字段」— 与本插入模板分开 axis；后续
  iter 评估
- 「移除 frontmatter」shortcut — 反向；按需 propose
