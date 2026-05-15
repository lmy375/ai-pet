# PanelPersona 接入共享 `formatRelativeAgeBuckets`

## 背景

上轮抽出 `src/utils/formatRelativeAge.ts::formatRelativeAgeBuckets`，PanelMemory / PanelTasks / PanelChat 三处接入。PanelPersona 还有两处内联实现，是 dedup 的剩余尾巴。

- L471-480：persona 自画像 "X 天前 / 小时前更新"（**缺 minute bucket**：< 1h
  一律 "刚刚更新"，1h-1m 之间信号缺失）
- L574-585：top tools `last_used_at` "刚刚 / X 分钟前 / X 小时前 / X 天前"
  （4-bucket 标准模板）

L471-480 接入 util 后顺便补齐 minute bucket，行为微正向改善（精度提升）。

## 改动

`src/components/panel/PanelPersona.tsx`：

1. 顶部 import `import { formatRelativeAgeBuckets } from "../../utils/formatRelativeAge";`
2. persona 自画像段：

   ```ts
   const label =
     ageMs < 60_000 ? "刚刚更新" : `${formatRelativeAgeBuckets(ageMs)}更新`;
   ```

   原嵌套三目（仅 days / hours bucket）改成 4-bucket 走 util，附加 minute 精度。

3. top tools `ageStr` 段：

   ```ts
   if (!Number.isFinite(ts)) return t.last_used_at;
   const ageMs = Date.now() - ts;
   if (ageMs < 60_000) return "刚刚";
   return formatRelativeAgeBuckets(ageMs);
   ```

   原 6 行 (`ageMin / ageHr / ageDay` cascading) 压成 3 行。

`stale = ageDays >= 7` 守门保留 — 那是另一维度（视觉提示"久没更新"），与文本格式化无关。

## 不做

- 不动 App.tsx `formatMoodElapsed`：用 "Xs 前 / 分前 / 小时前"（秒级 + "分" 简称非"分钟"），不在 3-bucket 中文模板里
- 不写测试：util 本身已稳，caller 是字符串拼接

## 验收

- `npx tsc --noEmit` ✅
- 「人格」tab 自画像 chip：1h 之内显 "X 分钟前更新"（原为"刚刚更新"），> 1h 显 "X 小时前 / X 天前更新" 行为不变
- 「人格」tab top tools 行的 `last_used_at` 显示与之前完全等价

## 完成

- [x] PanelPersona.tsx: import + 两处内联 → util
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
