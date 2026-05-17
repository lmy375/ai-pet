# PanelMemory item 右键「📎 复制 [[cat/title]] inline ref」（iter #484 — 已实现 pivot）

## Discovery

TODO 提出「PanelMemory item 右键加 📎 复制 [[cat/title]] inline ref」
作为 既有 🔗 chip click 的 ctx menu 对偶入口。但 grep 既有代码：

```text
src/components/panel/PanelMemory.tsx:9168
              🔗 复制 inline ref
```

iter #439 已加入相同 action 到 item ctx menu — 与既有 inline chip
共生。代码片段（PanelMemory.tsx:9151-9169）：

```tsx
<button
  onClick={async () => {
    setMemItemCtxMenu(null);
    const ref = `[[${m.catKey}/${m.title}]]`;
    try {
      await navigator.clipboard.writeText(ref);
      setMessage(`🔗 已复制 inline ref：${ref}`);
    } catch (err) {
      setMessage(`复制 ref 失败：${err}`);
    }
    window.setTimeout(() => setMessage(""), 3000);
  }}
>
  🔗 复制 inline ref
</button>
```

TODO 作者大概率没注意到该入口已存在 — 是项目历史里多次出现的
"already-implemented pivot" 模式（如 iter #424 detail size chip /
iter #421 inline ref chip / iter #431 等）。

## Resolution

按既有 "already-implemented pivot" 模板处理：
- TODO 行从 docs/TODO.md 移除（已交付）
- 本 doc 记录 discovery 让未来 audit 不重复

icon 差异（spec 用 📎 vs 既有 🔗）— 不改动：
- 既有 🔗 与 PanelMemory 内其它 ref-related entries 视觉一致（PanelTasks
  「🔗 复制 detail.md 绝对路径」等也用 🔗）
- 📎 paperclip 视觉差异不显著改善 + drift 现有 chip family

## Verification

- 既有 ctx menu entry 已 production — PanelMemory.tsx:9151-9169
- 既有 inline chip 入口也存在（PanelMemory.tsx:8203+）— 两入口对偶
- 无代码改动；nothing to test
