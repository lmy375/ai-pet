# PanelMemory 段标题加「📤 export 段为 .md 文件」chip（iter #472）

## Background

PanelMemory 既有 export 入口：
- 顶部「📋 导出」— 全 cat 拼 markdown 到剪贴板
- 顶部「📋 单段…」下拉 — 选 cat 复制到剪贴板
- 顶部「💾 .md」— 全 cat 保存为本地文件
- 段标题「📋 titles」chip — 单 cat 仅 title 列表到剪贴板（iter #449）

但缺一个 **单 cat 完整 .md 文件下载** 入口 — owner 想「把这段送给同
事 / 备份单 cat」时要走「📋 单段… → 复制 → 打开 vim → 粘 → 存盘」
多步。

本 iter 加段标题「📤 .md」chip — 单击直接走 OS Save 对话框保存为
`pet-memory-<catKey>-YYYY-MM-DD.md`。

## Changes

### `src/components/panel/PanelMemory.tsx`

紧贴既有「📋 titles」chip 之后插「📤 .md」chip：

```tsx
{cat.items.length > 0 && (
  <button onClick={() => {
    const label = categoryLabels[catKey] || cat.label;
    const lines: string[] = [];
    const ts = new Date().toLocaleString();
    lines.push(`# ${label} (${cat.items.length} 条 · ${ts})`, "");
    for (const item of cat.items) {
      lines.push(`## ${item.title}`);
      if (item.updated_at) {
        lines.push(`> 更新于 ${item.updated_at.slice(0, 16).replace("T", " ")}`);
      }
      lines.push("", item.description, "");
    }
    const md = lines.join("\n");
    try {
      const blob = new Blob([md], { type: "text/markdown;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const now = new Date();
      const y = now.getFullYear();
      const mo = String(now.getMonth() + 1).padStart(2, "0");
      const d = String(now.getDate()).padStart(2, "0");
      const filename = `pet-memory-${catKey}-${y}-${mo}-${d}.md`;
      const a = document.createElement("a");
      a.href = url;
      a.download = filename;
      document.body.appendChild(a);
      a.click();
      a.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1500);
      setMessage(`已保存「${label}」${cat.items.length} 条到 ${filename}`);
    } catch (e: any) {
      setMessage(`保存失败：${e}`);
    }
    setTimeout(() => setMessage(""), 4000);
  }}>
    📤 .md
  </button>
)}
```

文件结构与既有「📋 单段…」下拉同 schema：
- H1: `# {label} ({N} 条 · {ts})`
- H2: `## {item.title}`
- blockquote: `> 更新于 {ts}`
- description body

filename `pet-memory-<catKey>-YYYY-MM-DD.md` 含 catKey 让多 cat 导出
不互盖；含日期让重复导出不互盖。

## Key design decisions

- **chip 而非右键菜单**：TODO 原文写"段标题右键加 📤 export"。但段标
  题 **右键已被** iter #466 占用为「📁 reveal cat dir」。再加右键动作
  要起 popup 菜单（额外 dismiss / Esc / outside-click 逻辑）。chip
  在 section header chip row 已是 5+ buttons 视觉密度可接受 + 比右键
  popup 更 discoverable。"右键" 是 spec 的 placeholder hint，本质需
  求是"段头有 .md export 入口"
- **复用「💾 .md」blob + a.download 模板**：与既有 handleExportAllToFile
  完全同 pattern；1500ms 延迟 revoke 防 WKWebView 抢在写盘前清 url
  生空文件（既有已 production 验证）
- **不引 pinned-only subset**：与顶部「📋 单段…」下拉的「📌 pinned 子
  组」差异化 — 本 chip 是「整段 .md 备份」用例，pinned subset 是
  「精简分享」用例。后者需要单选下拉，与本 chip「单击就 export 全段」
  原则冲突；想要 pinned subset 走顶部下拉
- **不 sanitize catKey 作文件名**：catKey 仅含 ascii lowercase / `_` /
  digit（pet 后端约束），天然安全可作文件名一部分。不引入 sanitize
  helper
- **不写 unit test**：纯 Blob / a.download / setMessage 副作用；逻辑
  trivial（既有 handleExportAllToFile 同算法 production 验证）。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 纯前端 Blob + download
- 手测：PanelMemory section header → 看 chip 行含「⋯⋯ 📋 titles · 📤
  .md · 🗑 清空 · + 新建」→ 点 📤 .md → OS Save 弹 → 文件名 pet-memory-
  butler_tasks-2026-05-18.md → 内容含 H1 + H2 + descriptions
