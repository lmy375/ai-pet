# PanelTasks chip-bar「🏷 30d rename N」chip（iter #588）

## Background

iter #581 加了 7d rename chip — 周维度 refactoring 节奏信号。本 iter
补 30d cousin — 区分「本周突击 vs 本月持续力度」两 scope。

完成 rename audit 桌面端双 chip + TG 三视角：
- 🏷 7d rename N（桌面）
- 🏷 30d rename N（桌面，本 iter）
- /recent_renames [N] / /aliases <title> / 🏷 chip TG（远程对偶）

## Change

`PanelTasks.tsx` 重构既有 7d effect 为双 cutoff 共享：

```tsx
const [renameCount7d, setRenameCount7d] = useState<number | null>(null);
const [renameCount30d, setRenameCount30d] = useState<number | null>(null);
useEffect(() => {
  const tick = async () => {
    const lines = await invoke<string[]>("get_butler_history", { n: 100 });
    const cutoff7dMs = Date.now() - 7 * 24*60*60*1000;
    const cutoff30dMs = Date.now() - 30 * 24*60*60*1000;
    let n7 = 0; let n30 = 0;
    for (const line of lines) {
      // single scan: parse ts + body, then double cutoff check
      if (!body.startsWith("rename ")) continue;
      if (tsMs >= cutoff30dMs) n30 += 1;
      if (tsMs >= cutoff7dMs) n7 += 1;
    }
    setRenameCount7d(n7); setRenameCount30d(n30);
  };
  // mount + 5min refresh
}, []);
```

紧贴既有 🏷 7d chip 加 🏷 30d chip — slate-tint 比 7d 略深表达「更
广 scope」。click 复制「近 30d N 次 rename」单行。

## Key design decisions

- **共享 fetch 单 scan 双 count**：避免双 useEffect 双 fetch 双 timer
  造成 IO 翻倍 + 不同步状态。一次扫 lines 用 `if (tsMs >= cutoff30d) n30
  += 1; if (tsMs >= cutoff7d) n7 += 1` 嵌套窗口 — 7d 一定 ⊆ 30d 不
  必额外条件
- **slate-tint depth 区分**：7d chip 用 fg 8% / 15% border；30d chip
  用 fg 12% / 20% border — 视觉上 30d 比 7d 略深一点表达「更广 / 更
  历史 scope」语义。owner 扫两 chip 时一眼区分
- **0 时不渲，与既有稀疏 chip family 一致**：避免 dead chip 占视觉
  位置
- **click 复制「近 30d」前缀**：让粘贴出来的字符串自带 scope 标识，
  避免「N 次 rename」单数字脱 scope 后语义模糊

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — chip 紧贴 7d chip 同位置，同 click pattern，无
  layout race

## Future iters (out of scope)

- **「🏷 90d rename」cousin chip**：超长周期 — quarterly review 用。
  但 chip-bar 已偏密集；按需 propose
- **30d / 7d 比率 chip**：「本周占本月 X%」refactoring 集中度信号。
  比单数字信息密度高但 chip 数 unchanged。可作 follow-up
- **click 弹 modal 显近 N 条 rename 详情**：把数字升为入口而非单值。
  与 TG /recent_renames 等价；按需 propose
