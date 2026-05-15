# PanelChat session row 日期：今天 / 昨天 / 否则 YYYY-MM-DD

## 背景

session 下拉行下方显的 updated_at 日期一直是 `YYYY-MM-DD` 原始字符串。对今天的 session 来说，"今天"比"2026-05-14"语义更直接；扫眼也更快。

## 改动

`src/components/panel/PanelChat.tsx` 行 3537：

```tsx
{(() => {
  const date = s.updated_at.slice(0, 10);
  const now = new Date();
  const today = now.toLocaleDateString("sv-SE");
  const yest = new Date(now.getTime() - 86_400_000).toLocaleDateString("sv-SE");
  if (date === today) return "今天";
  if (date === yest) return "昨天";
  return date;
})()}
```

外层 `<div>` 加 `title={s.updated_at.replace("T", " ").slice(0, 16)}` 让 hover 仍能看到完整 "YYYY-MM-DD HH:MM"。

`toLocaleDateString("sv-SE")` 给本地时区的 ISO 格式日期前缀，与 updated_at 的 `YYYY-MM-DD` 前 10 字符直接相等比较。

## 不做

- 不显"前天" / 周一-日 / 几小时前：每多一档判断 UI 概念栈就更长；今天/昨天/绝对日期三档已是大部分体感场景
- 不在 search mode 那条 SearchResultRow 改：search 命中是另一种视图（聚焦于内容），日期相对不如绝对（用户可能想找"3 月那次说的话"）

## 验收

- `npx tsc --noEmit` ✅
- 切到 panel 下拉看 session list：今日的显"今天"，昨日的"昨天"，更早原 YYYY-MM-DD
- hover 日期 → tooltip 显完整本地时间

## 完成

- [x] PanelChat.tsx: session row 日期换成相对/绝对混合
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
