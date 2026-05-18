# PanelMemory cat 排序 toggle「🔥 cat 按 7d 净增 desc」（iter #558）

## Background

PanelMemory cat header「📊 7d +N」chip（iter #555）显单 cat 的滚动
delta；TG 端 /cat_growth_7d（iter #557）是 cross-cat 列表 audit。但
桌面端缺：把活跃 cat 段自动顶到顶部，让 owner 滚动一眼即看到「最近
在长的几类」— 比要 hover 每个 cat 看 chip 高效。

## Change

加新 cat-level 排序 toggle（与既有 item-level sort* 正交）：

1. **State**: `sortCatsByGrowth7d` boolean + localStorage 持久化
   （key `pet-memory-sort-cats-7d`），紧贴 `pinnedOnly` 等顶部状态
2. **Toolbar button**: 🔥 cat 按 7d / 🔥 cat 7d -，染 tint-orange
   active 态。tooltip 解释「0 净增 cat 末尾保默认序」
3. **Ordering 改造**：原 ordered 列表算完（savedCatOrder + CATEGORY_ORDER
   + index unknown）后，若 toggle 激活：算各 cat 7d delta，delta > 0
   提前按 desc，0 delta 保后段顺序

```tsx
if (sortCatsByGrowth7d) {
  const sevenDaysAgoMs = Date.now() - 7 * 24 * 60 * 60 * 1000;
  const deltaOf = (k: string): number => {
    const cat = index.categories[k];
    if (!cat) return 0;
    let n = 0;
    for (const it of cat.items) {
      if (!it.created_at) continue;
      const cMs = Date.parse(it.created_at);
      if (isNaN(cMs)) continue;
      if (cMs >= sevenDaysAgoMs) n += 1;
    }
    return n;
  };
  const active: { k: string; d: number; i: number }[] = [];
  const inactive: string[] = [];
  ordered.forEach((k, i) => {
    const d = deltaOf(k);
    if (d > 0) active.push({ k, d, i });
    else inactive.push(k);
  });
  active.sort((a, b) => b.d - a.d || a.i - b.i);
  return [...active.map((x) => x.k), ...inactive];
}
```

## Key design decisions

- **cat-level 与 item-level sort 正交**：本 toggle 控制段间序，既有
  📅 按时间 / 📏 按字数 / 🔀 按创建 控制段内 item 序。tooltip 强调
  这点避免 owner 混淆
- **0 delta cat 保 default 序而非排末尾按字母**：保 default 序是为
  了「关 toggle 后 inactive cat 不要乱跳」。如果按 alpha 排会让用户
  视觉记忆错乱（"我刚才那 cat 在哪？"）
- **tie-break 用 ordered 原 index**：稳定输出。两 cat delta 都为 5 时，
  显示哪个先按 default ordered 决定 — 让 owner 可解释「为什么 A 在 B
  前」
- **激活时不禁用 drag-reorder**：用户拖时 savedCatOrder 仍持久化，但
  当前视图被 sort 覆盖。关 toggle 后看到拖过的顺序起效 — 这是「sort
  覆盖偏好但不破坏偏好」语义
- **Date.now() 而非 component-level now state**：本 ordering 逻辑在
  render-time 内联，无需逐秒重算（cat 净增是日尺度信号，1s tick 太
  细）— 用 Date.now() 每次 render 取即可，避免读上下文 `now` state
- **tint-orange 配色**：与既有 tint-blue (sortBy*) / tint-yellow
  (pinnedOnly) / tint-* 系列保协调；orange 暗示「热度 / 活跃」语义
  与 🔥 emoji 呼应

## Verification

- `npx tsc --noEmit` clean — 一开始误用了上下文 `now` state（4649
  行），TDZ 错；改 `Date.now()` 后 pass
- 视觉手测 deferred — 改动是 toolbar 单按钮 + ordering 单分支，无
  跨层 race 风险，TS pass + 仔细 review 即上线
- 无 lib test — 是纯 desktop UI

## Future iters (out of scope)

- **「cat 按 30d 净增」长周期 cousin**：与 7d 互补；但 4-stat 排序
  界面已偏满，需要先 audit「30d 是否真有人用 vs 总是用 7d」 — propose
  后单独评估
- **下拉 / radio 切换**：把 4 stat sort toggles 改成单 dropdown 收一
  起。降低 toolbar 视觉密度但牺牲一键切换。等 toggle 数到 6+ 再做
- **cat header 7d chip 染色与 sort active 态联动**：本 toggle 激活时
  各 cat header 📊 7d chip 显得更醒目（边框高亮）让 owner 看出"现在
  按这个排"。改动小但 UX 优雅，按需补
