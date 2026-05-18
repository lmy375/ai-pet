# ChatMini bubble 右键加「📝 转 reflect」ctx menu 项（iter #527）

## Background

ChatMini 选区 toolbar 已有 📚 「加到 ai_insights」按钮（line 3591+） —
**选中文字** 转 ai_insights memory item。但 bubble **整条** 转 reflect
入口缺：

- bubble 右键 ctx menu 有 💾 转 task（line 3306+） — 整条转 task
- 同 menu 缺整条转 reflect — 与 task 对偶的「我想记一段反思」入口

差异：
- task / 「要做的事」— 走 butler_tasks
- reflect / 「反思 / 自我洞察」— 走 ai_insights
- note / 「杂项 brain-dump」— 走 general（选区 toolbar 📝 已覆盖）

本 iter 补 bubble → reflect 入口，让「整条 bubble + 分类信号 (task /
reflect)」二路径完整。

## Changes

### `src/components/ChatMini.tsx`

紧贴 💾 转 task ctx menu item 之后插入：

```tsx
{hasText && onSaveAsAiInsight && (
  <button
    type="button"
    style={item}
    onClick={() => {
      setCtxMenu(null);
      onSaveAsAiInsight(text);
      setBubbleCopyIdx(ctxMenu.idx);
      window.setTimeout(
        () => setBubbleCopyIdx((cur) =>
          cur === ctxMenu.idx ? null : cur),
        1500,
      );
    }}
    title="把这条 bubble 转 ai_insights memory item — 反思 / 自我洞察分类..."
  >
    📝 转 reflect
  </button>
)}
```

复用既有 `onSaveAsAiInsight` callback prop（App.tsx 1049 +
`handleMiniSaveAsAiInsight` 实现已通 memory_edit("create",
"ai_insights")）+ 既有选区 toolbar 📚 入口同后端。

1.5s ✓ flash visual feedback 与 💾 转 task 同 setBubbleCopyIdx pattern。

## Key design decisions

- **复用既有 onSaveAsAiInsight callback**：App.tsx 已实现 ai_insights
  写入路径 + 用 reflect-YYYY-MM-DDTHH-MM-SS 命名约定（与 /reflect TG
  命令一致）— 不引新 IPC 路径
- **保 hasText gate**：与 💾 转 task 同防御 — 极端兜底防空 reflect
- **不显选区时仍可点 reflect**：与既有 📚 选区 toolbar 入口对偶 —
  那个是「选段反思」；本入口是「整条反思」（bubble 主旨够好就整条入）
- **gate `onSaveAsAiInsight &&`**：App.tsx 未传 callback 时（如未来其
  它 ChatMini consumer）chip 不渲染 — 与 onSaveAsTask 同模板防御
- **1.5s ✓ 视觉反馈**：与 💾 转 task 同 setBubbleCopyIdx — owner 看到
  「✓」就知道存成功；不需 toast / 弹窗 noise
- **emoji 📝**：与 PanelMemory ai_insights 段 emoji（📝 / 🧠）一致；
  /reflect 命令 emoji 也是 🪞 但 ChatMini 内更口语化用 📝（与既有选
  区 📚 / 选区 📝 note 同 cluster）
- **不写 unit test**：纯 ctx menu render + callback 调用；逻辑 trivial
  （既有 💾 转 task / 📚 选区 toolbar 同 pattern production 验证）。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 后端无改动 — 复用既有 memory_edit ai_insights 路径
- 手测：
  - ChatMini 任意有 text 的 bubble 右键 → ctx menu 出现「📝 转 reflect」
    在「💾 转 task」之后
  - click → 1.5s ✓ flash + PanelMemory ai_insights 段看到新 item
    `reflect-YYYY-MM-DDTHH-MM-SS` description = bubble 全文
  - 纯图 bubble（hasText=false）→ chip 不显
  - 不再次单击同一 bubble → 不重复创建（owner 自己控制 idempotency）

## Future iters (out of scope)

- 「📝 转 reflect (with title 编辑)」popover — 当前自动 title 不可改；
  后续需要 owner 控制 title 时加 popover
- 「📝 转 reflect + 自动加 tag」— ai_insights item 自动打 `#bubble-source`
  tag 让回查 origin 容易；后续 propose
