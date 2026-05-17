# PanelTasks due chip hover tooltip 精确倒计时（iter #361）

## Background

PanelTasks 任务行 due chip 之前 tooltip 显示 enum-level 紧迫度 +
小时/天级 relative（"已过期：已过 3 小时"、"24 小时内到期：1 小时
内到期"）。两个问题：
1. **分钟级精度缺失**：< 1 小时窗口都显"1 小时内到期"，5 分钟和
   59 分钟同字符串，owner glance 不出真急迫度。
2. **前缀重复**：tooltip prefix（"已过期：" / "24 小时内到期："）+
   formatDueRelative 输出（"已过 N 小时" / "1 小时内到期"）造成
   双重前缀（"已过期：已过 3 小时"），表达冗余。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `formatDueRelative`（~line 1115）重写为分钟级 + 自描述前缀

```tsx
function formatDueRelative(dueIso: string, now: number): string {
  // ... Date.parse ...
  if (absMs < 60_000) {
    return future ? "马上到期" : "刚刚过期";
  }
  if (absMs < 3_600_000) {
    const mins = Math.floor(absMs / 60_000);
    return future ? `还有 ${mins} 分钟到期` : `已逾期 ${mins} 分钟`;
  }
  if (absMs < 86_400_000) {
    const hours = Math.floor(absMs / 3_600_000);
    return future ? `还有 ${hours} 小时到期` : `已逾期 ${hours} 小时`;
  }
  const days = Math.floor(absMs / 86_400_000);
  return future ? `还有 ${days} 天到期` : `已逾期 ${days} 天`;
}
```

阈值表：
- < 60s → "马上到期" / "刚刚过期"
- < 60min → "还有 N 分钟到期" / "已逾期 N 分钟"（新增分钟桶）
- < 24h → "还有 N 小时到期" / "已逾期 N 小时"（旧"已过 N 小时" → "已逾期"统一）
- ≥ 24h → "还有 N 天到期" / "已逾期 N 天"

#### 2. tooltip 拼装简化（~line 8989）

老：
```tsx
const tooltip =
  urgency === "overdue"
    ? `已过期：${relative}`
    : urgency === "soon"
      ? `24 小时内到期：${relative}`
      : relative;
```

新：
```tsx
const tooltip = formatDueRelative(t.due, nowMs);
```

formatDueRelative 已自带"还有/已逾期"语义前缀 — 不需再叠 urgency
词造成 "已过期：已逾期 3 小时" 这种双重前缀。urgency 仍控制颜色 /
背景 / fontWeight（视觉层面），文本完全由 relative 接管（语义层
面），层责分离。

## Key design decisions

- **分钟桶单独切出（< 60 min）**：owner 的核心痛点 — "今天到期"
  vs "还有 47 分钟" 视觉信号天差地远。原"1 小时内到期"包了 5min
  - 59min 整段，没用。
- **`已过` → `已逾期`**：原文案"已过 3 小时"略 ambiguous（"过"也
  可指过节）；"已逾期"更具体 + 与"逾期 task"业务词汇统一。
- **`Math.floor` 而非 `Math.ceil`**：分钟数显示用 floor（"还有 0
  分钟" 不出现 — 因为 < 60s 走"马上到期"桶；剩 90s 显示"还有 1
  分钟"合理）。如果用 ceil 会出现"还有 60 分钟到期"在 59min59s 边
  界 → 让 owner 心智不一致。
- **不抽 `formatDuration(ms): string` 通用工具**：本 fn 业务语义
  绑得紧（"到期" vs "逾期"前缀，分桶阈值与 dueUrgency 同结构），
  抽通用工具反而要传 4-5 个 i18n 字段。当前单 panel 内 35 行直白
  可读。
- **不为单 fn 引 unit test runner**：项目无现存 frontend test
  runner（grep 0 `.test.tsx` / `.test.ts`）；为这一个 fn 拉
  vitest / jest 是 over-eng。改动是 tooltip 文案 + 数字精度，
  build pass + 手 hover 即可验证。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.21s)
