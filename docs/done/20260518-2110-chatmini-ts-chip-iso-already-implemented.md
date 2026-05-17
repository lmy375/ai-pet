# ChatMini bubble timestamp chip → 复制 ISO — 已实现 pivot（iter #500）

## Discovery

本 TODO 项「ChatMini bubble timestamp chip click → 复制本条 ISO 8601 时
间戳到剪贴板」在加入 TODO 前已完整实现。grep 前未发现的原因：搜的是
`copyIsoTimestamp` / `copy_iso_ts` 等关键词，但既有实现用的是 inline
handler 无独立函数名。

定位：`src/components/ChatMini.tsx:2387-2473`

```tsx
{hasValidTime && !hiddenTimestampIdx.has(idx) && (() => {
  const isTsCopied = tsCopyIdx === idx;
  return (
    <span
      className="pet-mini-row-time"
      title={`${formatFullTimestamp(m.ts)} · 单击复制完整 ISO timestamp · 双击复制 "MM-DD HH:MM" 友好短格式`}
      onClick={(e) => {
        e.stopPropagation();
        if (!m.ts) return;
        navigator.clipboard.writeText(m.ts).then(() => {
          setTsCopyIdx(idx);
          window.setTimeout(...);
        });
      }}
      onDoubleClick={(e) => {
        // 复制 "MM-DD HH:MM" 友好短格式
        ...
      }}
      ...
    >
      {isTsCopied ? "✓ " : ""}{timeLabel}
    </span>
  );
})()}
```

行为与 TODO 完全吻合 + 超过：
- 单击 → 复制 raw `m.ts`（ISO 8601 完整）
- **双击** → 复制 `MM-DD HH:MM` 短格式（owner 想发同事 / 写笔记时不要
  ISO 那么长）— TODO 未要求，已附赠
- 1.5s ✓ 视觉反馈
- title attr 说明两个 click 路径

## Decision

不再重复实现。TODO 项删除，本 doc 作记录。

第 4 个 already-implemented pivot 本 cycle（#495 bubble→task / #498
find-replace / #499 /why → /timeline / #500 ts→ISO）。procedure 改进：
proposal 时除了 grep keyword 还要 grep emoji/UX 概念（"timestamp chip" /
"复制 ts" 等多字面量）。

## Verification

- 手测：ChatMini bubble 单击顶部 [HH:MM] chip → ✓ flash + 剪贴板含完
  整 ISO；双击同 chip → 短格式
- 无新代码 / 无新测试

## Future iters

无 — 既有实现已覆盖单击 ISO + 双击短格式两路径，未来增量价值需另立题。
