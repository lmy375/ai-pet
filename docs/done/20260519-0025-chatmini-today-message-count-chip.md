# ChatMini ambient 「📊 今日 N 消息」chip（iter #515）

## Background

ChatMini ambient row 已有 📝 transient / 🚨 alarms / 🔇 mute / ⏱ silent
四 chip 显「pet 当前感知 / 状态」信号。但缺**owner 自己的活跃度信号** —
「今天我和 pet 聊了多少」/「今天我用了 pet 多频繁」audit 入口空缺。

本 iter 加 「📊 今日 N」chip — scan messages 中 ts 落在本地今日的
user + assistant 总数。click 复制 `今日（YYYY-MM-DD）N 条消息` line。

## Changes

### `src/components/ChatMini.tsx`

#### 新 `messagesToday` useMemo

```tsx
const messagesToday = useMemo(() => {
  const todayStr = new Date().toLocaleDateString("sv-SE"); // YYYY-MM-DD 本地
  let count = 0;
  for (const m of messages) {
    if (!m.ts) continue;
    const d = new Date(m.ts);
    if (isNaN(d.getTime())) continue;
    const itemStr = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    if (itemStr === todayStr) count += 1;
  }
  return count;
}, [messages, silentTick]);
```

deps：
- `messages` — 源 prop，新消息进来时重算
- `silentTick` — 既有 30s tick，跨午夜时自然刷新（避免 stale "今日"）

依赖 `messages` 而非 `visibleItems`（visibleItems 在此 scope 之后才声
明，TDZ 风险）— 但 messages 是 raw input，覆盖更全。

#### `ambientHasContent` gate 扩展

加 `|| messagesToday > 0` — chip 仅在今日有过消息时显，其它信号都空
时本 chip 仍能拉起 ambient row。idle 态（昨天没聊 + 今日新打开 pet
就立刻不聊）仍空 row。

#### chip 渲染（紧贴 ⏱ silent chip 之后）

```tsx
{messagesToday > 0 && (
  <button
    onClick={async (e) => {
      e.stopPropagation();
      const todayStr = new Date().toLocaleDateString("sv-SE");
      const line = `今日（${todayStr}）${messagesToday} 条消息`;
      await navigator.clipboard.writeText(line);
      console.log(`📊 已复制：${line}`);
    }}
    title={`本会话今日（本地日历日）共 ${messagesToday} 条 user + assistant 消息。点击复制「今日 N 消息」一行到剪贴板。`}
    style={{
      display: "inline-flex",
      alignItems: "center",
      gap: 2,
      padding: "1px 6px",
      borderRadius: 8,
      background: "color-mix(in srgb, var(--pet-color-fg) 6%, transparent)",
      color: "var(--pet-color-muted)",
      fontWeight: 500,
      border: "none",
      cursor: "pointer",
    }}
  >
    📊 今日 {messagesToday}
  </button>
)}
```

## Key design decisions

- **base messages prop 不 visibleItems**：visibleItems TDZ 风险（在此
  scope 后声明）；messages 是 raw input，搜索 / 过滤的 visibleItems 反
  而少 message — 今日 audit 该用 raw 全集
- **本地日历日**：toLocaleDateString("sv-SE") 输出 ISO `YYYY-MM-DD` 同
  时按本地时区切日 — 与 PanelTasks createdToday filter / TG
  /touched_today 等其它「今日」入口一致
- **silentTick 30s 跨午夜自动刷新**：既有 ChatMini silentTick 是 30s
  interval — 复用让本 chip 跨午夜时（owner 在睡前 23:59 开 pet 聊 +
  00:30 醒来看）从昨日切今日自然
- **`> 0` gate**：与既有 ambientHasContent 模式一致 — 0 时 chip 隐藏
  避免占垂直空间。idle 态（全部信号空）整 row 仍不渲染
- **复用 ambient muted 风格**：bg = fg 6% mix；border: none — 与 ⏱
  silent chip 默认态视觉一致（与 transient / alarms 彩色 chip 分层）
- **click 复制 line + console.log**：与 ChatPanel 既有 chip family 同
  feedback pattern（无 toast，console.log 透显）
- **不写 unit test**：纯 React useMemo + Date compare + render condition；
  逻辑 trivial（既有 messagesToday 同 createdToday filter pattern
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰
  性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.34s)
- 后端无改动 — 纯前端 chip
- 手测：
  - 今日有过消息 → ambient row 看到「📊 今日 N」灰 chip
  - 0 消息时 chip 隐藏（其它信号也空 → 整 row 隐藏）
  - click → console 「📊 已复制：今日（YYYY-MM-DD）N 条消息」+ 剪贴板
    含该 line
  - 跨午夜后 30s 内自动刷新（silentTick）

## Future iters (out of scope)

- 「今日 N（owner X · pet Y）」拆分 — 当前合计；想看 owner vs pet 占
  比可后续按需扩
- 「本周 / 本月」级 chip — 当前 daily 足够，更长窗口可单独 propose
