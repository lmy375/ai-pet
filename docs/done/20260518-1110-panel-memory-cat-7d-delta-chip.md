# PanelMemory cat header「📊 7d 净增」chip（iter #555）

## Background

PanelMemory 每个 cat header 已有：

- 📊 概览 chip — 当前总量 + 最近 update 时间 = **snapshot 视角**
- 🌱 今日新增 modal（全局） — 看 today created — 整 panel 维度

但**单 cat 7d 滚动活跃度信号**缺：「这个 cat 最近一周新加了几条？」
是 owner 判断「我哪类知识在长 / 哪类已停滞」的核心 delta 指标，
比单日新增（噪音大）、绝对总量（不动量）都更稳。

## Change

在 PanelMemory cat header（紧贴既有 📊 概览 chip 右侧）加 📊 7d +N
chip：

```tsx
{cat.items.length > 0 && (() => {
  const sevenDaysAgoMs = now.getTime() - 7 * 24 * 60 * 60 * 1000;
  let delta = 0;
  for (const it of cat.items) {
    if (!it.created_at) continue;
    const cMs = Date.parse(it.created_at);
    if (isNaN(cMs)) continue;
    if (cMs >= sevenDaysAgoMs) delta += 1;
  }
  if (delta === 0) return null;
  return (
    <button … onClick={async () => {
      const label = categoryLabels[catKey] || cat.label;
      const line = `${label} · 7d 净增 ${delta} 条`;
      await navigator.clipboard.writeText(line);
      setMessage(`📊 已复制：${line}`);
      …
    }} title="本 cat 最近 7 天净增 N 条 item…">
      📊 7d +{delta}
    </button>
  );
})()}
```

## Key design decisions

- **0 时不渲染**：cat 列里多数 cat 7d 不会有新增 — 全部塞 chip 视觉
  噪音。仅当确有增量时挂出，挂出本身就是「注意：这 cat 在动」的信号
- **created_at only**：「净增」语义 = 新建。不计 update / rename /
  跨 cat 移入（item.created_at 一旦初始化就不变 — pinned / tag /
  detail 修改不挪 created_at）
- **7 天滚动窗口**：not 自然周 — 滚动更稳；不像「本周新增」周一突然
  归零造成 cat 一致看着「没活动」
- **复用 now（已有 state，1s tick refresh）**：chip 准实时滚动，
  无需独立 timer
- **click 单行复制**：复用 setMessage 3s feedback pattern（既有 📊
  概览 chip / 复制按钮均同模式）— 跨 cat 取样发同事 / 自己周回顾
- **小写 7d 而非 7天**：与既有 chip family 简洁风格一致 + 7d 含义
  audit 圈内通行

## Verification

- `npx tsc --noEmit`：clean — 无 TS 误用
- 视觉手测：dev 跑下确认 chip 排版（之前曾发生过 chip 太多挤换行）
- 标题 hover tooltip 完整解释「按 created_at 计；不含 update/rename/
  跨 cat 移入」— 避免 owner 误解 chip 数字含义

## Future iters (out of scope)

- **趋势线**：cat header 旁一行 mini 7d sparkline（每日新增条）—
  比单总数信息密度高。本 iter 单 chip 形态优先 ship；trend bar 后续
  按需 propose
- **「📊 30d +N」cousin**：长周期延伸 chip；30d 比 7d 更平滑反映长
  期投入。但 30d 显示数字会大，需要再加压缩规则（>50 → +50+ 简写）；
  out of scope
- **跨 cat 排序**：「净增最快 cat 上头」排序 — 让 owner 看 weekly
  delta 一眼。需 panel 层级排序 logic 改造，单独 propose
