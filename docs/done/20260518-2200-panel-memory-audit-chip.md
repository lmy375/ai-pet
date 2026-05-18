# PanelMemory toolbar「📊 audit · N」chip（iter #597）

## Background

iter #587 加 TG /audit_summary；iter #596 加 PanelTasks 📋 audit chip。
缺 PanelMemory 维度对偶 — memory 视角的概览 audit chip。本 iter 补齐
三 surface audit 概览矩阵：

- TG /audit_summary: 跨维度聚合（task + cat + rename + done）
- PanelTasks 📋 audit · N: task 维度（pinned / idle / today P7 / rename
  / done）
- PanelMemory 📊 audit · N: **memory 维度**（cats / items / 7d 净增 /
  active cats / stale cats）

## Change

`PanelMemory.tsx` toolbar 紧贴 cat-sort radio 后加 📊 audit chip：

```tsx
{index && Object.keys(index.categories).length > 0 && (() => {
  const sevenDaysAgoMs = Date.now() - 7 * 24 * 60 * 60 * 1000;
  let totalItems = 0, new7d = 0, activeCats7d = 0,
      staleCats7d = 0, emptyCats = 0;
  for (const cat of Object.values(index.categories)) {
    if (cat.items.length === 0) { emptyCats += 1; continue; }
    let maxUpdated = 0, catNew7d = 0;
    for (const it of cat.items) {
      // count created_at in 7d + max updated_at
    }
    totalItems += cat.items.length;
    new7d += catNew7d;
    if (catNew7d > 0) activeCats7d += 1;
    if (maxUpdated > 0 && maxUpdated < sevenDaysAgoMs) staleCats7d += 1;
  }
  const lines = [
    `📊 memory audit（YYYY-MM-DD）`,
    `· 总 cat: ${totalCats}（含 ${emptyCats} 空）`,
    `· 总 items: ${totalItems}`,
    `· 7d 净增: ${new7d} 条 across ${activeCats7d} cat`,
    `· 7d stale cat: ${staleCats7d} 条`,
  ];
  // chip: 📊 audit · {new7d}
  // hover: tooltip = lines.join("\n")
  // click: copy md
})()}
```

## Key design decisions

- **5 signals 选择**：total cats / total items / 7d 净增 / 活跃 cats /
  stale cats — 与 TG /cat_growth_7d + /cat_decay_7d 数据维度一致
  + 总量背景（cats / items）
- **chip 数字 = 7d 净增**：让 chip 自身有数字信号（不全靠 tooltip）。
  其它 4 维在 tooltip。选 net-7d 因「activity signal」最直观
- **单 inline 扫**：一次遍历 index.categories 同时算多个 signal —
  与 PanelTasks 📋 chip 复用 useMemo 思路一致（频次低不需 memoize）
- **slate-tint fg-10% + fontWeight 600**：与 PanelTasks 📋 audit chip
  视觉一致 — owner 看到「📊 audit · N」立即识别为 audit 概览
- **仅 index 非空时浮**：避免空 memory（启动初）时显 dead chip
- **`new Date().toISOString().slice(0, 10)`**：YYYY-MM-DD 让复制的 md
  含日期 anchor — owner 粘日报时有时间戳

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — chip 加在熟悉 toolbar 位置（紧贴 cat-sort radio
  后），同 click-copy pattern 与既有 chip 一致

## Future iters (out of scope)

- **chip click 弹 modal 展开**：当前 click 复制 md；future 可 click
  弹 modal 内含 deep dive 入口（→ /cat_growth_7d / /cat_decay_7d / 等）
  与 TG /audit_summary 结构一致
- **per-signal click filter**：tooltip 行变 button — click 「stale cat
  3」 直接 toggle stale cat filter（未来加）
- **30d 维度兄弟 chip**：「📊 30d audit · N」cousin — 长周期 audit。
  按需 propose；当前 7d 信号足够
- **chip 数字告警 tint**：stale cat > 阈值时切红 tint。需经验阈值
