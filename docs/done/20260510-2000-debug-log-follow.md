# PanelDebug 日志区 follow-tail 模式（Iter R139）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 日志区 follow-tail 模式：现 useEffect 在 logs 变化时无条件 scrollTop=scrollHeight，把用户向上滚读旧日志的视口强行拽回底部；改成"已经在底部"才自动追加 + 加"📌 跟随最新"toggle（与 PanelChat R103 scroll-to-top 互补）。

## 目标

PanelDebug 日志区现"无条件 auto-scroll to bottom"（line 421-425）：每次
`logs` state 变化（轮询拉取新日志），都把 scrollTop 直接拽到底。如果用
户正在向上滚读旧 log，就会被强行拉回 —— 阅读体验断裂。

加 follow-tail 行为：
- 用户在底部 → 新 log 自动追加（与现行为一致）
- 用户向上滚 → 不再 auto-scroll；旁边小 toggle "📌 跟随最新" 显 off 状态
- 点 toggle / 滚回底部 → 重新 follow

## 非目标

- 不动 logs polling 节奏 / 数据结构
- 不在底部时不显 toggle —— 一直显（off 时让用户知道当前不 follow）
- 不引入"暂停轮询"功能 —— 与 follow-tail 是两码事
- 不动 R99 level filter row —— 那是过滤维度，本轮加在它末尾或独立位置

## 设计

### state

```ts
const [followTail, setFollowTail] = useState(true);
```

默认 true（与现状一致）；用户向上滚 →自动 false；点 toggle → 手动切换。

### onScroll detect

挂 onScroll handler 到日志容器：当用户离底 > 阈值（如 8px）→ followTail=false；
回到底（≤ 阈值）→ followTail=true。

```ts
const handleLogScroll = () => {
  const el = scrollRef.current;
  if (!el) return;
  const distFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
  setFollowTail(distFromBottom <= 8);
};
```

阈值 8px：浏览器有时浮点偏差让 scrollTop + clientHeight 比 scrollHeight
小 1-3px；8 给点 buffer。

注意：当 useEffect 自己设 scrollTop 时也会触发 onScroll —— scroll 到
scrollHeight 后 distFromBottom = 0，setFollowTail(true)，与目标一致。

### useEffect 改造

```diff
 useEffect(() => {
-  if (scrollRef.current) {
+  if (followTail && scrollRef.current) {
     scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
   }
-}, [logs]);
+}, [logs, followTail]);
```

deps 加 followTail：用户从 false 切回 true 时立即 scroll-to-bottom（不
用等下一次 logs 更新）。

### toggle 按钮

放在 R99 level filter chip row 末尾右侧（marginLeft: auto）：

```tsx
<button
  type="button"
  onClick={() => setFollowTail((v) => !v)}
  title={
    followTail
      ? "当前跟随最新日志。点击关闭：保持当前滚动位置不被新 log 拽下"
      : "已脱离最新（向上滚读旧 log 触发）。点击重新跟随 + 滚到底"
  }
  style={{
    fontSize: "10px",
    padding: "1px 6px",
    border: "1px solid var(--pet-color-border)",
    borderRadius: 4,
    background: followTail ? "var(--pet-color-card)" : "var(--pet-color-bg)",
    color: followTail ? "var(--pet-color-fg)" : "var(--pet-color-muted)",
    cursor: "pointer",
    marginLeft: "auto",
    whiteSpace: "nowrap",
  }}
>
  {followTail ? "📌 跟随最新" : "📌 已脱离"}
</button>
```

放在 level chip row 现有"显示 N / M"统计 span 之后；marginLeft: auto 让
此 chip 推到行末。但行末已有那个 stats span 在 logLevels.size > 0 时
出现 —— 会冲突。改放 stats 之前，stats 再 marginLeft: auto。

实际看 R99 现有代码：stats span 在 row 末用 `marginLeft: "auto"` 推右；
我的 toggle 应该独立放在 row 末（推到 stats 右侧）或行末左侧（不抢 marginLeft auto）。

更稳妥：把 toggle 放 chip row 末尾，让 stats span 仍在它之前；并且
toggle 永远显（无 levels 选择时也显）。

让 toggle 用 marginLeft: auto；stats span 改去掉 marginLeft（紧跟 chips）。
但这改 R99 行为。简单：toggle 加 marginLeft: 8（普通 gap）+ stats marginLeft
auto 不变 — toggle 落 stats 右边 / 间距 8。

### 测试

无单测；手测：
- 默认 followTail = true，新 log → 自动滚到底
- 向上滚 → followTail=false，toggle 显 "📌 已脱离"
- 期间新 log 来 → 不滚
- 点 toggle → 立即滚到底 + followTail=true
- 滚回底（手动） → onScroll 自动设 followTail=true
- 切 level filter / 清空 → 不破坏 followTail 状态

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + onScroll handler + useEffect 改造 |
| **M2** | toggle 按钮放 R99 chip row 末 |
| **M3** | tsc + build |

## 复用清单

- 既有 `scrollRef` / 自动滚 useEffect
- R99 log level chip row 容器（toggle 顺势加进去）

## 进度日志

- 2026-05-10 20:00 — 创建本文档；准备 M1。
- 2026-05-10 20:08 — M1 完成。`followTail: boolean` state 默认 true；scrollRef 容器加 onScroll handler 计算 distFromBottom 阈值 8px → setFollowTail；useEffect 改条件：仅 followTail=true 才 scrollTop=scrollHeight；deps 加 followTail 让用户切回 true 时立即跳底。
- 2026-05-10 20:14 — M2 完成。R99 chip row 末（stats span 之后）插 toggle 按钮："📌 跟随最新" / "📌 已脱离"；点击 setFollowTail(true) + 立即 scrollTop=scrollHeight；marginLeft 条件 8（stats 显时紧贴）/ auto（stats 不显时推到行末）。
- 2026-05-10 20:18 — M3 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
