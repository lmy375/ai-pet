# PanelMemory item 「🔗 复制 inline ref」按钮（iter #414）

## Background

owner 在 detail.md 内想引用另一条 memory item 时（"参见
ai_insights/写作流"、"沿用 chat_persona/expert"），目前要手敲
`[[ai_insights/写作流]]` 这种 token — 易拼错（typo / 缺斜杠 / 大
小写不一致）。本 iter 加 🔗 一键复制 inline ref 到剪贴板。

格式：`[[<category>/<title>]]`，wiki-link 风。当前是纯 plain-text
marker — owner 自己识读约定；未来可加 wiki-link 解析（如 detail.md
渲染时把 `[[...]]` 转 clickable link 跳回 PanelMemory item）。本
iter 只建持久化通路（剪贴板 + 一致格式），渲染 / 跳转 follow-up。

## Changes

### `src/components/panel/PanelMemory.tsx`（紧贴 📑 复制副本之后）

```tsx
<button
  style={s.btn}
  onClick={async (e) => {
    e.stopPropagation();
    const ref = `[[${catKey}/${item.title}]]`;
    try {
      await navigator.clipboard.writeText(ref);
      setMessage(`🔗 已复制 inline ref：${ref}`);
    } catch (err) {
      setMessage(`复制 ref 失败：${err}`);
    }
    setTimeout(() => setMessage(""), 3000);
  }}
  title="复制 inline ref `[[cat/title]]` 到剪贴板 — 在其它 memory item / task detail.md 内粘贴作交叉引用 token..."
>
  🔗
</button>
```

设计要点：
- **emoji 🔗 区分 既有 📋 / 📑**：📋 复制 detail.md / 📑 复制为新
  item / 🔗 复制 ref token — 三个 copy 动作各自语义独立
- **wiki-link 双方括号格式 `[[...]]`**：与 Obsidian / Roam /
  Logseq 等成熟 PKM 工具同社区约定；owner 即使切到外部 markdown
  编辑器粘贴也保识别性
- **cat/title 分隔用 `/`**：与 detail_path 内的 cat/file.md 路径
  形式一致 — owner 看到 ref 可直觉关联到磁盘文件位置
- **stopPropagation**：防 click 冒泡触发 item row click handler
- **复用 setMessage toast**：与既有 📋 / 📑 / alarm chip 同 channel，
  统一反馈节奏
- **完整 ref 在 toast 显**：让 owner 立刻看到生成的 token 是啥，
  方便核对（如发现 title 含空格时仍生效）

## Key design decisions

- **不验证 cat / title 合法性**：catKey / title 都来自既有 item，
  天然合法；不必前端二次 sanitize
- **不去重 title 含 `/`**：理论上 title 含 `/` 会让 `[[cat/foo/bar]]`
  解析歧义（cat=foo? cat=cat?），但 memory_rename 已禁 `/` 在
  title 内（filename safety），不会冲突
- **不抽 `formatMemoryRef(cat, title)` helper**：1 行 template literal
  没必要抽 fn；命中处只 1 个
- **不为单按钮引 unit test**：行为是 string template + clipboard
  write + setMessage；build pass + 手测足够（点 🔗 → toast 显
  `[[cat/title]]` → 粘贴到外部编辑器验完整 token）
- **不引入「@」alternative**：mention-style `@cat/title` 与 TG
  mention / chat ref 冲突；`[[...]]` 是显式 marker 不混淆

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动 — 纯前端字符串 + clipboard 操作
