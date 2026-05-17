# ChatMini 选区 toolbar 「📌 标记」按钮（iter #412）

## Background

ChatMini 选区 toolbar 既有按钮：💾 转 task / 📝 记到 note / 📋 复制 /
💬 推到 ChatPanel / 🔄 让 AI 改写 — 都"主动消费"选段。但「先标记下
来，回头看」入口缺位。owner 在 ChatMini 看到一句话值得保留时只能
立即转 task / note —— 决策成本高，标记是更轻动作。

本 iter 加 📌 标记按钮，与 PanelChat 既有 bookmark chip strip /
modal 共用 `pet-chat-marked-messages` localStorage key — 但用
distinct key 形式 `${sessionId}::sel-${ts}` 让两种标记并存：
- PanelChat 整条消息标记 → key = `${sessionId}::${itemIdx}` (numeric)
- ChatMini 选段标记 → key = `${sessionId}::sel-${ms}` (字符串)

PanelChat 的既有 idx-parseInt → Number.isNaN filter 自动跳过 sel-*
项不渲；本 iter 同步更新 PanelChat 顶部 "📌 N" badge 用
renderableCount（仅 numeric-idx）+ marks modal header 用 entries
真实加载数，避免 badge 显数 vs modal 内容数不一致的视觉断裂。

## Changes

### `src/components/ChatMini.tsx`

selection toolbar 加 📌 button（紧贴 💬 与 🔄 之间）：

```tsx
<button
  type="button"
  style={btnStyle}
  title="📌 标记选段：写入 markedMessages localStorage..."
  onClick={async () => {
    const text = selectionToolbar.text;
    setSelectionToolbar(null);
    try {
      const idx = await invoke<{ active_id: string }>("list_sessions");
      const sid = idx.active_id?.trim();
      if (!sid) { setCopyToast("err"); return; }
      const KEY = "pet-chat-marked-messages";
      let parsed: Record<string, unknown> = {};
      try {
        const raw = localStorage.getItem(KEY);
        if (raw) {
          const got = JSON.parse(raw);
          if (got && typeof got === "object" && !Array.isArray(got)) parsed = got;
          else if (Array.isArray(got)) {  // 老 Array<string> 格式迁移
            for (const s of got) if (typeof s === "string") parsed[s] = 0;
          }
        }
      } catch {}
      const ts = Date.now();
      parsed[`${sid}::sel-${ts}`] = ts;
      localStorage.setItem(KEY, JSON.stringify(parsed));
      setCopyToast("done");
      console.info(`[ChatMini] 📌 marked selection (${text.length} chars):`, text.slice(0, 60));
    } catch (e) {
      setCopyToast("err");
    }
  }}
>
  📌
</button>
```

设计要点：
- **list_sessions Tauri 调用拿 active_id**：useChat 不暴露 sessionId，
  invoke 是最干净的获取路径
- **localStorage 兼容老 Array<string> 格式**：与 PanelChat read 路径
  同 migrate-on-write 模式
- **空 sid → err toast 不写盘**：防 sessionless 状态下污染存储
- **复用 copyToast 反馈通道**：与 📋 复制按钮同视觉 mechanism — 不
  引第二条 toast 系统
- **console.info dump selection 文本**：MVP 阶段没有"看 marks"UI，
  console 让 owner 至少能 audit 标过啥。后续 iter 可加 ChatMini own
  marks 弹窗 / chip strip 渲所有 sel-* 项

### `src/components/panel/PanelChat.tsx`

#### 1. 顶部 "📌 N" badge 用 renderableCount（仅 numeric-idx）

```tsx
{(() => {
  let renderableCount = 0;
  for (const k of markedMessages.keys()) {
    const sep = k.indexOf("::");
    if (sep < 0) continue;
    const idx = parseInt(k.slice(sep + 2), 10);
    if (!Number.isNaN(idx)) renderableCount += 1;
  }
  if (renderableCount === 0) return null;
  return <button ...>📌 {renderableCount}</button>;
})()}
```

排除 sel-* 项（ChatMini 写）— 与 modal 实际渲染数对齐。

#### 2. marks modal header 用 entries.length 而非 .size

```tsx
<span>📌 全部标记消息 ({marksModalEntries?.length ?? "…"})</span>
```

entries 是 openMarksModal 已 IO-resolved 后的真实数（已经 skip 了
sel-* 和 dangling idx），与 modal body 渲染条数一致。

## Key design decisions

- **同 localStorage key 而非 sibling key**：忠实 TODO 文案"与
  PanelChat bookmark chip strip 同 localStorage 通道" — 一个 key
  承载两类标记，未来加 ChatMini own marks UI 时也只读这一个 key
- **distinct key 形式 sel-${ts} 而非 numeric idx**：ChatMini visibleItems
  idx ↔ session.items idx 不直接对齐（messages 含 system，items 不
  含；tool calls 可能合并），强行映射会导致 PanelChat 标错条。sel-
  是「文本标记，不对应整条消息」明确语义
- **value 仍存 timestamp**：与 PanelChat 同 schema（idx-key 也存 ts），
  保 read-side 通用解析；text snippet 不入 value（违反 PanelChat 的
  `typeof ts === "number"` filter 会被丢）
- **不为本 iter 加 ChatMini own marks UI**：MVP 仅建持久化通路；后
  续 iter 可加 ChatMini "📌 N" chip + popover 渲所有 sel-* 项 +
  delete affordance（需要 invoke list_sessions 每次或缓存 sessionId）
- **不为单按钮引 unit test**：行为是 invoke + localStorage write +
  setState；build pass + 手测足够：（1）选段 → 见 toolbar 弹起 →
  点 📌 → toast 显 ✓ → reload ChatMini → 标记仍在 localStorage（用
  devtools 验）→（2）选第二段 → 标 → localStorage 含两条 sel-*
  →（3）在 PanelChat 标整条消息 → 看 "📌 N" badge 计入 numeric-idx
  项不含 sel-*

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用既有 list_sessions Tauri 命令
