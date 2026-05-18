# ChatMini bubble「⏱ N 分前」hover chip — 已实现 pivot drop（iter #566）

## Discovery

TODO 提案：「ChatMini bubble hover「⏱ N 分前」chip：与 bubble ts 互补，
显距 now 相对时间（精度比 ts 高 — ambient awareness）」。

**事实：这个 chip 已经存在并工作多 iter 了。**

## Existing implementation

`ChatMini.tsx:2614-2674`：

```tsx
{/* bubble 底相对时间 chip：与顶 [HH:MM] 时钟 chip 对偶 —
    顶给绝对时刻、底给"距现在多久"。row hover 才显（pet-mini-
    row-rel CSS 类透明度 0 → 0.5），存在感比顶 chip 还低
    （ambient 信号）。user 行靠右底 / assistant 行靠左底，与 bubble
    对齐方向同侧。relText 解析失败时不渲染（无 ts / 未来时刻）。
    hiddenTimestampIdx 折叠时跳过（与顶 chip 同 gate）—— 密集
    burst 中间也合并。 */}
{hasValidTime &&
  !hiddenTimestampIdx.has(idx) &&
  (() => {
    const rel = formatBubbleRelative(m.ts);
    if (!rel) return null;
    // …
    return (
      <span className="pet-mini-row-rel" …>
        {isRelCopied ? "✓ " : ""}⏱ {rel}
      </span>
    );
  })()}
```

`formatBubbleRelative` 实现（ChatMini.tsx:286-301）：

```ts
if (ageMs < 60_000) return "刚刚";
if (ageMs < 3_600_000) return `${Math.floor(ageMs / 60_000)} 分前`;
if (ageMs < 86_400_000) return `${Math.floor(ageMs / 3_600_000)} 时前`;
// 跨日：startOfDay 比对，"昨天" vs "N 天前"
if (diffDays === 1) return "昨天";
if (diffDays >= 2) return `${diffDays} 天前`;
```

Behavior：
- row hover 才显（`pet-mini-row-rel` opacity 0 → 0.5）— ambient 信号
- user 右底 / assistant 左底（与 bubble 对齐方向同侧）
- click 复制相对时间字符串到剪贴板 + 1.5s ✓ 视觉反馈
- 与 hiddenTimestampIdx（密集 burst 折叠）gate 一致 — 顶 chip 隐时也隐

## Why TODO 提案此功能

可能因为：
- 视觉「隐于 hover」让 ambient chip 不显眼 — owner 没注意到
- 与 bubble 顶 `[HH:MM]` ts chip 共存，但 ts chip 更显眼

如果 owner 想让相对时间 chip 永久可见（去掉 hover gate），那是不同
需求 — 调整 CSS opacity 即可。但本 TODO 字面要求「hover」chip 显相
对时间，与既有实现完全一致。

## Decision

**不实现**。功能已完整：
- formatBubbleRelative 给出 4 档精度（刚刚 / N 分前 / N 时前 / 昨天 /
  N 天前）—— 精度按 age 自动选档
- click 复制单行
- ✓ 视觉反馈
- 与 hiddenTimestampIdx burst-fold 协同

procedure 教训：propose 「ChatMini bubble hover chip」类需求时，先
grep `pet-mini-row-` CSS 类前缀确认既有 hover chip family 范围 —
chips 在 ChatMini bubble 里很多（ts / rel / chars / save-as-task /
copy / ref token），各自有 hover gate，不一定能感知到全部存在。

## Future iters (out of scope)

- **让 rel chip permanent visible**：去 `pet-mini-row-rel:hover` opacity
  gate 让 chip 默认可见。但这会让 bubble 周围 chip 密度大幅上升 —
  owner 需明确表态偏好（hover-only ambient vs always-visible
  noise）后再做。本 iter 不假设
- **rel chip + ts chip 合并**：同位置浮一 chip，hover 切换 abs / rel
  显示。但合并丢「同时看到 abs 和 rel」UX；owner 可能想都看 — 否决
- **rel 文案 i18n**：当前仅中文（分前 / 时前 / 昨天）。若日后多语，
  formatBubbleRelative 加 locale 参数 + 串表
