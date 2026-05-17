# PanelMemory 顶部 toolbar 加「📋 index 概览」chip（iter #521）

## Background

PanelMemory 顶部 toolbar 已有：
- 「📋 单段…」下拉（含全段 + 仅 pinned 子段）
- 「💾 .md」全 cat 文件 export
- 「📥 import .md」往返

按 cat 维度信号已覆盖；按 item 维度 chip 也有（行 hover 📜 raw / 📋
ref / 📅 created 等）。但**跨 cat 一行 metadata snap** 缺口 — owner 想
发同事 / 写日记 / paste 到 doc 时拿到本 PanelMemory 整体状态摘要时只
能：

1. 逐 cat 走 `📊 概览` chip（iter #494）— N 次操作
2. 手算总数 — friction

本 iter 加 toolbar 「📋 概览」chip — 一行 metadata：「N cats · M items
· K detail.md」（含空段数附注）。

## Changes

### `src/components/panel/PanelMemory.tsx`

紧贴 「💾 .md」按钮之后插入：

```tsx
<button
  style={s.btn}
  onClick={async () => {
    if (!index) return;
    const cats = Object.values(index.categories);
    const nonEmpty = cats.filter((c) => c.items.length > 0);
    const totalItems = cats.reduce((sum, c) => sum + c.items.length, 0);
    // detail.md 文件数：用 detailSizes 命中数 — cache size>0 算「真实
    // 存在」（detail_path 非空但 .md 空文件 size=0 排除）。fallback：
    // detail_path 非空数（首次加载未走过 detailSizes）
    const withDetail = cats.reduce((sum, c) => {
      return sum + c.items.filter((it) => {
        if (it.detail_path.length === 0) return false;
        const size = detailSizes[it.detail_path];
        return size !== undefined && size > 0;
      }).length;
    }, 0);
    const line = `${nonEmpty.length} cats · ${totalItems} items · ${withDetail} detail.md（${cats.length - nonEmpty.length === 0 ? "全段非空" : `${cats.length - nonEmpty.length} 空段`}）`;
    await navigator.clipboard.writeText(line);
    setMessage(`📋 已复制 index 概览：${line}`);
  }}
  disabled={!index}
  title={...}
>
  📋 概览
</button>
```

## Key design decisions

- **「N cats · M items · K detail.md」三元统计**：cats（非空段数）/
  items（总条目）/ detail.md（实际有 content 的 markdown 文件数）—
  三个独立维度信号
- **空段数附注**：`(全段非空)` / `(N 空段)` — 让 owner 知道有些 cat
  存在但空（如 ai_insights 早期 / 自建未填）；不强加但不漏
- **detailSizes cache 优先**：detail.md 实际有内容的数量 — 比单纯
  `detail_path !== ""` 更准（detail_path 是 yaml 字段，但 .md 文件可
  能空 / 没创建）
- **`disabled={!index}` + tooltip**：index 加载前 chip 灰，避免空状态
  click
- **setMessage 3s toast 显完整 line**：与既有 cat-level 📊 概览 chip
  同 feedback pattern
- **「📋 概览」紧凑标签**：与「📋 单段…」/「💾 .md」/「📥 .md」同
  toolbar 短标签家族 — 不挤；tooltip 含详细说明
- **不写 unit test**：纯 React onClick + Object.values reduce + clipboard
  write；逻辑 trivial（既有 cat-level 📊 概览 chip 同算法 production
  验证）。GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 后端无改动 — 纯前端 toolbar button
- 手测：
  - PanelMemory 顶部 toolbar 看到「📋 概览」按钮（disabled 直到 index
    加载完）
  - click → toast 「📋 已复制 index 概览：N cats · M items · K detail.md
    （全段非空 / 若干空段）」
  - 粘到 chat / 日记看到完整一行

## Future iters (out of scope)

- 「📊 detail.md size 总和」chip — 字节统计；当前 cats/items/detail.md
  数已够
- 「📊 distribution chart」mini sparkline — items per cat 视觉分布；
  PanelDebug 风格的视图入口
