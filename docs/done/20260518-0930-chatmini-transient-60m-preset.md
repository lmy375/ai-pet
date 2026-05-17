# ChatMini bubble 「🎯 transient 60m」preset（iter #464）

## Background

ChatMini bubble ctx menu 已有「📝 用此话设 transient_note 30m」入口
（iter #364）— 把 pet reply 文本作 30 分钟 transient_note 让 pet 下
轮 proactive cycle 读到调整 context / 语气。

但 30 分钟对「长会议（≥ 1h）/ deep work 一上午 / 客户来访 2 小时」等
场景不够覆盖 — owner 在 chat 里看到 pet reply 想「把这条作下 1 小时
context」时，要切到桌面 ToneStrip 自定义分钟数（多步）。

本 iter 加 sibling 「🎯 transient 60m」preset — 与 30m 同 callback、
不同 minutes，让 1 小时窗口一键搞定。

## Changes

### `src/components/ChatMini.tsx`

紧贴既有 30m preset 之后插：

```tsx
{isAssistant && hasText && onSetTransientNote && (
  <button
    onClick={() => {
      setCtxMenu(null);
      onSetTransientNote(text, 60);
    }}
  >
    🎯 用此话设 transient_note 60m
  </button>
)}
```

- 复用既有 `onSetTransientNote` callback（已 production：iter #364 接
  入 set_transient_note Tauri 后端）
- 同 visibility gate `isAssistant && hasText && onSetTransientNote` —
  与 30m preset 一致（仅 pet reply 作 context 才有意义；owner 自己说
  的话作 context 语义混淆）
- icon 🎯 vs 📝 — 让 owner 一眼区分两 preset 的"时长 + 强度"差别（30m
  📝 = 短期记录；60m 🎯 = 中期目标 / 持续 context）

## Key design decisions

- **不参数化 minutes（不引 popover 选）**：与既有 30m 单击 UX 简单原则
  对齐。owner 想精确分钟数（如 90m / 240m）走桌面 ToneStrip 自定义
  input。两 preset 覆盖最常用的 30m / 60m 两段
- **`🎯` vs `📝` icon 区分**：避免两 chip 同 emoji 看不出差别。🎯 「目
  标 / 锁定」语义 ≈ 1 小时「我有个具体目标，pet 这段 keep this in
  mind」；📝 「随手记」语义 ≈ 30 分钟「快速 dump 个 context」。两 emoji
  与 minutes 长短自然对应
- **位置紧贴 30m preset**：两 preset 视觉相邻让 owner 心智「同一动作，
  长 vs 短两选项」对比明显
- **role gate `isAssistant` 保持**：owner 自己说的话作 transient_note
  含义不清（"我说 X" → pet 下轮读到 → 以为 owner 重申 X？） — 30m
  preset 已做对了，60m 同理
- **不引「⏰ 90m / ⏰ 4h」更多 preset**：30/60 两段已覆盖 95% 场景；
  再加 preset 让 ctx menu 视觉过载。owner 真要 4h 走 ToneStrip 输入
- **不写 unit test**：纯 click callback 不同参数 + 既有 onSetTransientNote
  backend tests 已覆盖。GOAL.md "meaningful tests only" 规则下不引装
  饰性测试。`tsc + vite build` clean 即足够

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.31s)
- 后端无改动 — 复用既有 `set_transient_note` callback path
- 手测：ChatMini 右键 pet bubble → menu 含「📝 transient 30m」+
  「🎯 transient 60m」两 preset → 点 🎯 → ToneStrip 显「transient_note：
  <text> 剩 60 分钟」
