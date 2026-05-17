# ChatMini bubble 右键「📋 复制 thread 5」（iter #477）

## Background

ChatMini bubble ctx menu 已有「📋 复制本条」/「📋 markdown 原文」/「⌚
含时间戳」三个单 bubble 复制变体。但 owner 想 audit「这段对话上下文」
（如 pet 给出建议后 user / pet 来回 3-4 句的小段）时只能逐条 ctx-menu
复制 + 自己拼接。

本 iter 加「📋 复制 thread 5」 — 选中 bubble + 之上 4 条（含本条共 5
条 user / assistant 消息）拼成 markdown 段。

## Changes

### `src/components/ChatMini.tsx`

紧贴「⌚ 复制 · 含时间戳」之后插：

```tsx
<button
  onClick={() => {
    setCtxMenu(null);
    const idx = ctxMenu.idx;
    const startIdx = Math.max(0, idx - 4);
    const slice = visibleItems.slice(startIdx, idx + 1);
    if (slice.length === 0) return;
    const text = slice
      .map((mi) => {
        const glyph = mi.role === "user" ? effectiveUserGlyph : effectiveAssistantGlyph;
        const prefix = copyIncludeTime
          ? `${formatBubbleTimestamp(mi.ts)} ${glyph}`
          : glyph;
        return `${prefix} ${extractText(mi.content)}`.trim();
      })
      .filter((s) => s.length > 0)
      .join("\n\n");
    navigator.clipboard.writeText(text).then(() => {
      setBubbleCopyIdx(ctxMenu.idx);
      window.setTimeout(() => setBubbleCopyIdx((cur) => cur === ctxMenu.idx ? null : cur), 1500);
    }).catch((err) => console.error("copy thread 5 failed:", err));
  }}
  title={`复制本 bubble + 之上 4 条（共 5 条 user / assistant 消息）拼 markdown — 上下文段 audit 场景。${copyIncludeTime ? "含 [HH:MM] timestamp prefix。" : "不含 timestamp（开顶部「⌚ 含时间戳」preference 切换）。"}`}
>
  📋 复制 thread 5
</button>
```

设计：
- **`visibleItems.slice(idx - 4, idx + 1)`**：本 bubble 在 visibleItems 数
  组中的位置 idx；slice 含末位的 4 条历史 + 自身 = 5 条。`Math.max(0, ...)`
  防越界（开头几条 bubble 没 4 条前置）
- **复用 `extractText` + `effectiveUserGlyph` / `AssistantGlyph`**：
  与 copyRecentN（顶部「📋 复制最近 N 条」按钮）同 format 协议；user
  默认 🧑 / assistant 默认 🐾 — owner 自定义 glyph 在两入口一致
- **`copyIncludeTime` 偏好对齐**：与顶部 ⌚ 含时间戳 toggle 同 state —
  owner 在两处复制都遵循一致的「含 / 不含 timestamp」偏好（无需重复
  decide）
- **`\n\n` 段隔**：markdown 渲染时段落自然换行（与 copyRecentN 同）；
  贴到 markdown 编辑器 / GitHub issue 都正确展示

## Key design decisions

- **5 不参数化**：5 是常识"对话上下文一段"经验值（典型 user-pet-user-
  pet-user 来回 5 句）。引入参数化 (3 / 10 / 自定义) 复杂度 vs 受益不
  匹配；owner 想其它长度走顶部「📋 复制最近 N 条」（既有有 5 / 10 /
  20 三 preset 下拉）
- **slice 末位 = 本 bubble**：让 owner 心智「我从这条往前看 4 条」直
  接 — 不是「从这条往后看 4 条」未来视角（pet 后续回应可能还没发）
- **复用 `setBubbleCopyIdx` 1.5s ✓ 反馈**：与「📋 复制本条」/「⌚ 含
  时间戳」复制族同视觉反馈；维护一致心智
- **位置紧贴 ⌚ 含时间戳之后**：copy 族集中 — 「📋 / 📋 markdown / ⌚ /
  📋 thread / 🔗 / 💾」一行内紧凑视觉
- **不引「按对话方向延伸」自动 cluster 检测**：算法复杂度（同 role 连
  续 burst vs 新一轮）vs 受益不大；fixed 5 条简单稳定
- **不写 unit test**：纯 visibleItems slice + extractText + 字符串拼
  接 + clipboard 副作用；逻辑 trivial（既有 copyRecentN 同算法
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰
  性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.31s)
- 后端无改动 — 纯前端 UI
- 手测：ChatMini bubble 右键 → menu 含「📋 复制 thread 5」→ 点击 →
  bubble ✓ 反馈 → 粘到 markdown 编辑器看「🧑 X / 🐾 Y / 🧑 Z / ...」
  5 段；早期 bubble（< 5 条之前）会得到 < 5 条（slice 边界兜底）
