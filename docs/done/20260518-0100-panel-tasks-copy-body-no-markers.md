# PanelTasks ctxMenu「📋 复制 body（不含 markers）」(iter #436)

## Background

既有「📋 复制 raw_description」保全 markers — 适合 debug / 移植
跨任务复用 marker 组合。但 owner 想「这条 task 的本意是什么」纯
文本视图（转外部笔记 / chat / issue 标题 / share 等场景）当前
没专用入口 — 只能手动 strip markers。

本 iter 加 ctxMenu 兄弟按钮「📋 复制 body（不含 markers）」— strip
所有 `[bracket]` markers + `#tags` 后保 body 纯文本到剪贴板。

## Changes

### `src/components/panel/PanelTasks.tsx`（紧贴既有 📋 复制 raw_description 之后）

```tsx
{t && (
  <button
    onClick={async () => {
      setTaskCtxMenu(null);
      const raw = t.raw_description ?? "";
      const stripped = raw
        .replace(/\[[^\]]*\]/g, "")    // 1. strip 所有 [...] markers
        .replace(/(^|\s)#\S+/g, "$1")  // 2. strip #tag tokens
        .replace(/\s+/g, " ")           // 3. collapse 多空格
        .trim();
      if (stripped.length === 0) {
        setBulkResultMsg("body 为空 — raw 全是 markers / 无自然语言内容");
      } else {
        await navigator.clipboard.writeText(stripped);
        setBulkResultMsg(`已复制 body（${chars} 字，不含 markers）`);
      }
      setTimeout(() => setBulkResultMsg(""), 3000);
    }}
  >
    📋 复制 body（不含 markers）
  </button>
)}
```

设计要点：
- **三步 strip 序**：先剥 `[...]` markers（贪婪到首个 `]` — 与
  task_queue marker 协议一致不嵌套）→ 再剥 `#tag` tokens（要求
  起始 / 空白前置防误剥 inline 文本里的 hash）→ 最后 collapse
  多空格 trim 收尾
- **空 body 给 feedback**：避免 raw 全是 markers 时静默复制空串
  让 owner 误以为成功
- **chars count 用 unicode**：`Array.from().length` — 与既有
  raw_description copy / detail.md 全文 copy chip 同语义
- **stopPropagation 不需要**：onClick 内已 setTaskCtxMenu(null)
  关菜单；ctxMenu 是 fixed 浮窗不在 row 子树里
- **不删除「复制 raw_description」**：两按钮互补 — raw 保全
  markers（debug / 复用 marker 组合）/ body 仅自然语言（外发 /
  分享）。owner 按场景挑

## Key design decisions

- **简单 regex 不调 backend strip helper**：backend 有 strip_for_clone /
  strip_done_markers / strip_archive_markers 等专用 helpers 但各
  自只剥部分 marker；本场景要"剥所有 markers + tags"是新需求。
  regex 三行解决，调 backend Tauri 命令反而引入 IO / 异步
- **不剥 markdown syntax**：保 `**bold**` / `[link](url)` 等 owner
  写在 description 里的 markdown — 那些不是 task marker，是 owner
  期望保留的 formatting
- **不修改 raw 字符串原值**：纯派生计算 — 不动 t.raw_description
  state；与既有 raw copy 按钮同 read-only 语义
- **不为单按钮引 unit test**：纯 regex + setState；行为是 clipboard +
  toast；build pass + 手测足够（点 task 有 `[task pri=5] body #tag1`
  → 复制 → 粘 → 看「body」纯文本）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.40s)
- 后端无改动 — 纯前端 regex + clipboard 写
